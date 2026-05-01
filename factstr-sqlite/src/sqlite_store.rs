use std::fs;
use std::future::Future;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{
    Arc, Mutex,
    mpsc::{self, Receiver, Sender},
};
use std::thread::{self, JoinHandle};

use factstr::{
    AppendResult, DurableStream, EventQuery, EventRecord, EventStore, EventStoreError, EventStream,
    HandleStream, NewEvent, QueryResult,
};
use serde_json::Value;
use sqlx::sqlite::SqliteRow;
use sqlx::{QueryBuilder, Row, Sqlite, SqlitePool};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;
use tokio::runtime::Builder;

use crate::connection::open_pool;
use crate::query_match::matches_query;
use crate::schema::initialize_schema;
use crate::stream_registry::{DeliveryOutcome, PendingDelivery, SubscriptionRegistry};

#[derive(Clone, Debug)]
struct CommittedAppend {
    append_result: AppendResult,
    event_records: Vec<EventRecord>,
}

enum DeliveryCommand {
    Deliver(Vec<PendingDelivery>),
    Shutdown,
}

pub struct SqliteStore {
    database_path: PathBuf,
    pool: SqlitePool,
    subscription_registry: Arc<Mutex<SubscriptionRegistry>>,
    delivery_sender: Sender<DeliveryCommand>,
    delivery_thread: Mutex<Option<JoinHandle<()>>>,
}

#[derive(Clone, Debug)]
struct ReplayBatch {
    last_processed_sequence_number: u64,
    delivered_batch: Vec<EventRecord>,
}

#[derive(Clone, Debug)]
struct DurableReplayState {
    subscription_id: u64,
    last_processed_sequence_number: u64,
    replay_until_sequence_number: u64,
}

impl SqliteStore {
    pub fn open(database_path: impl AsRef<Path>) -> Result<Self, sqlx::Error> {
        let database_path = database_path.as_ref().to_path_buf();
        ensure_parent_directory(&database_path).map_err(sqlx_io_error)?;

        let (result_sender, result_receiver) = mpsc::sync_channel(1);
        let bootstrap_path = database_path.clone();

        let bootstrap_thread = thread::Builder::new()
            .name("factstr-sqlite-bootstrap".to_owned())
            .spawn(move || {
                let runtime = Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .map_err(sqlx_io_error);

                let result = match runtime {
                    Ok(runtime) => runtime.block_on(async {
                        let pool = open_pool(&bootstrap_path).await?;
                        initialize_schema(&pool).await?;
                        Ok::<_, sqlx::Error>(pool)
                    }),
                    Err(error) => Err(error),
                };

                let _ = result_sender.send(result);
            })
            .map_err(sqlx_io_error)?;

        let pool = match result_receiver.recv() {
            Ok(result) => result,
            Err(error) => Err(sqlx_io_error(io::Error::other(format!(
                "sqlite bootstrap thread did not return a result: {error}"
            )))),
        }?;

        bootstrap_thread.join().map_err(|_| {
            sqlx_io_error(io::Error::other(
                "sqlite bootstrap thread panicked before open completed",
            ))
        })?;

        let subscription_registry = Arc::new(Mutex::new(SubscriptionRegistry::default()));
        let (delivery_sender, delivery_receiver) = mpsc::channel();
        let delivery_pool = pool.clone();
        let delivery_registry = Arc::clone(&subscription_registry);
        let delivery_thread = thread::Builder::new()
            .name("factstr-sqlite-delivery".to_owned())
            .spawn(move || run_delivery_thread(delivery_pool, delivery_registry, delivery_receiver))
            .map_err(sqlx_io_error)?;

        Ok(Self {
            database_path,
            pool,
            subscription_registry,
            delivery_sender,
            delivery_thread: Mutex::new(Some(delivery_thread)),
        })
    }

    pub fn database_path(&self) -> &Path {
        &self.database_path
    }

    fn run_async<T, Fut, F>(&self, operation: &'static str, work: F) -> Result<T, EventStoreError>
    where
        T: Send + 'static,
        Fut: Future<Output = Result<T, EventStoreError>> + Send + 'static,
        F: FnOnce(SqlitePool) -> Fut + Send + 'static,
    {
        let pool = self.pool.clone();
        let (result_sender, result_receiver) = mpsc::sync_channel(1);

        let worker_thread = thread::Builder::new()
            .name(format!("factstr-sqlite-{operation}"))
            .spawn(move || {
                let runtime = Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .map_err(sqlx_io_error)
                    .map_err(sqlx_backend_failure);

                let result = match runtime {
                    Ok(runtime) => runtime.block_on(work(pool)),
                    Err(error) => Err(error),
                };

                let _ = result_sender.send(result);
            })
            .map_err(sqlx_io_error)
            .map_err(sqlx_backend_failure)?;

        let result = result_receiver
            .recv()
            .map_err(|error| EventStoreError::BackendFailure {
                message: format!("sqlite {operation} thread did not return a result: {error}"),
            })?;

        worker_thread
            .join()
            .map_err(|_| EventStoreError::BackendFailure {
                message: format!("sqlite {operation} thread panicked before completion"),
            })?;

        result
    }

    fn pending_deliveries(&self, committed_batch: &[EventRecord]) -> Vec<PendingDelivery> {
        match self.subscription_registry.lock() {
            Ok(mut subscription_registry) => {
                subscription_registry.pending_deliveries(committed_batch)
            }
            Err(poisoned) => poisoned.into_inner().pending_deliveries(committed_batch),
        }
    }

    fn enqueue_delivery(&self, pending_deliveries: Vec<PendingDelivery>) {
        if pending_deliveries.is_empty() {
            return;
        }

        if let Err(error) = self
            .delivery_sender
            .send(DeliveryCommand::Deliver(pending_deliveries))
        {
            eprintln!(
                "factstr-sqlite delivery dispatcher stopped after commit: {}",
                error
            );
        }
    }

    fn register_all_durable_stream(
        &self,
        subscriber_id: impl Into<String>,
        handle: HandleStream,
    ) -> Result<EventStream, EventStoreError> {
        self.subscribe_durable(subscriber_id.into(), EventQuery::all(), handle)
    }

    fn register_durable_stream(
        &self,
        subscriber_id: impl Into<String>,
        event_query: &EventQuery,
        handle: HandleStream,
    ) -> Result<EventStream, EventStoreError> {
        self.subscribe_durable(subscriber_id.into(), event_query.clone(), handle)
    }

    fn subscribe_durable(
        &self,
        subscriber_id: String,
        event_query: EventQuery,
        handle: HandleStream,
    ) -> Result<EventStream, EventStoreError> {
        let normalized_event_query = normalized_durable_event_query(&event_query);
        let event_query_json = serialize_event_query(&normalized_event_query)?;
        let subscription_registry = Arc::clone(&self.subscription_registry);
        let durable_subscriber_id = subscriber_id.clone();
        let normalized_event_query_for_registry = normalized_event_query.clone();
        let handle_for_registry = handle.clone();
        let replay_subscriber_id = subscriber_id.clone();

        let replay_state = self.run_async("subscribe_durable", move |pool| async move {
            let mut connection = pool.acquire().await.map_err(sqlx_backend_failure)?;

            sqlx::query("BEGIN IMMEDIATE")
                .execute(connection.as_mut())
                .await
                .map_err(sqlx_backend_failure)?;

            let last_processed_sequence_number = load_or_create_subscriber_cursor(
                connection.as_mut(),
                &durable_subscriber_id,
                &event_query_json,
            )
            .await?;
            let replay_until_sequence_number =
                current_max_sequence_number(connection.as_mut()).await?;

            let subscription_id = match subscription_registry.lock() {
                Ok(mut subscription_registry) => {
                    if normalized_event_query_for_registry.filters.is_none() {
                        subscription_registry
                            .subscribe_all_durable(
                                durable_subscriber_id.clone(),
                                replay_until_sequence_number,
                                handle_for_registry.clone(),
                            )
                            .map_err(subscription_registry_backend_failure)?
                    } else {
                        subscription_registry
                            .subscribe_to_durable(
                                durable_subscriber_id.clone(),
                                Some(normalized_event_query_for_registry.clone()),
                                replay_until_sequence_number,
                                handle_for_registry.clone(),
                            )
                            .map_err(subscription_registry_backend_failure)?
                    }
                }
                Err(poisoned) => {
                    let mut subscription_registry = poisoned.into_inner();
                    if normalized_event_query_for_registry.filters.is_none() {
                        subscription_registry
                            .subscribe_all_durable(
                                durable_subscriber_id.clone(),
                                replay_until_sequence_number,
                                handle_for_registry.clone(),
                            )
                            .map_err(subscription_registry_backend_failure)?
                    } else {
                        subscription_registry
                            .subscribe_to_durable(
                                durable_subscriber_id.clone(),
                                Some(normalized_event_query_for_registry.clone()),
                                replay_until_sequence_number,
                                handle_for_registry.clone(),
                            )
                            .map_err(subscription_registry_backend_failure)?
                    }
                }
            };

            if let Err(error) = sqlx::query("COMMIT").execute(connection.as_mut()).await {
                match subscription_registry.lock() {
                    Ok(mut subscription_registry) => {
                        subscription_registry.unsubscribe(subscription_id)
                    }
                    Err(poisoned) => poisoned.into_inner().unsubscribe(subscription_id),
                }
                return Err(sqlx_backend_failure(error));
            }

            Ok(DurableReplayState {
                subscription_id,
                last_processed_sequence_number,
                replay_until_sequence_number,
            })
        })?;

        let subscription = self.build_subscription_handle(replay_state.subscription_id);

        let replay_batches = match self.run_async("durable_replay", {
            let event_query = normalized_event_query.clone();
            move |pool| async move {
                ensure_replay_history_is_available_for_pool(
                    &pool,
                    replay_state.replay_until_sequence_number,
                )
                .await?;
                load_replay_batches(
                    &pool,
                    &event_query,
                    replay_state.last_processed_sequence_number,
                    replay_state.replay_until_sequence_number,
                )
                .await
            }
        }) {
            Ok(replay_batches) => replay_batches,
            Err(error) => {
                self.cleanup_durable_subscription(replay_state.subscription_id);
                return Err(error);
            }
        };

        for replay_batch in replay_batches {
            let pending_delivery = PendingDelivery {
                subscription_id: replay_state.subscription_id,
                durable_subscriber_id: Some(replay_subscriber_id.clone()),
                last_processed_sequence_number: replay_batch.last_processed_sequence_number,
                delivered_batch: replay_batch.delivered_batch,
                handle: handle.clone(),
            };

            match self.process_durable_delivery(pending_delivery) {
                Ok(true) => {}
                Ok(false) => {
                    self.cleanup_durable_subscription(replay_state.subscription_id);

                    return Err(EventStoreError::BackendFailure {
                        message: format!(
                            "durable replay for subscriber {} did not complete successfully",
                            replay_subscriber_id
                        ),
                    });
                }
                Err(error) => {
                    self.cleanup_durable_subscription(replay_state.subscription_id);
                    return Err(error);
                }
            }
        }

        self.finish_durable_replay(replay_state.subscription_id);
        Ok(subscription)
    }

    fn build_subscription_handle(&self, subscription_id: u64) -> EventStream {
        let subscription_registry = Arc::clone(&self.subscription_registry);

        EventStream::new(
            subscription_id,
            Arc::new(move |subscription_id| match subscription_registry.lock() {
                Ok(mut subscription_registry) => subscription_registry.unsubscribe(subscription_id),
                Err(poisoned) => poisoned.into_inner().unsubscribe(subscription_id),
            }),
        )
    }

    fn process_durable_delivery(
        &self,
        pending_delivery: PendingDelivery,
    ) -> Result<bool, EventStoreError> {
        match pending_delivery.deliver() {
            DeliveryOutcome::Succeeded {
                durable_subscriber_id,
                last_processed_sequence_number,
                ..
            } => {
                if let Some(durable_subscriber_id) = durable_subscriber_id {
                    self.run_async("advance_durable_cursor", move |pool| async move {
                        update_subscriber_cursor(
                            &pool,
                            &durable_subscriber_id,
                            last_processed_sequence_number,
                        )
                        .await
                    })?;
                }

                Ok(true)
            }
            DeliveryOutcome::Failed { .. } | DeliveryOutcome::Panicked { .. } => Ok(false),
        }
    }

    fn finish_durable_replay(&self, subscription_id: u64) {
        match self.subscription_registry.lock() {
            Ok(mut subscription_registry) => {
                let buffered_deliveries = subscription_registry.finish_replay(subscription_id);
                self.enqueue_delivery(buffered_deliveries);
            }
            Err(poisoned) => {
                let mut subscription_registry = poisoned.into_inner();
                let buffered_deliveries = subscription_registry.finish_replay(subscription_id);
                self.enqueue_delivery(buffered_deliveries);
            }
        }
    }

    fn cleanup_durable_subscription(&self, subscription_id: u64) {
        match self.subscription_registry.lock() {
            Ok(mut subscription_registry) => subscription_registry.unsubscribe(subscription_id),
            Err(poisoned) => poisoned.into_inner().unsubscribe(subscription_id),
        }
    }
}

impl Drop for SqliteStore {
    fn drop(&mut self) {
        let _ = self.delivery_sender.send(DeliveryCommand::Shutdown);

        if let Ok(mut delivery_thread) = self.delivery_thread.lock() {
            if let Some(delivery_thread) = delivery_thread.take() {
                let _ = delivery_thread.join();
            }
        }
    }
}

impl EventStore for SqliteStore {
    fn query(&self, event_query: &EventQuery) -> Result<QueryResult, EventStoreError> {
        let matching_records = self.run_async("query", {
            let event_query = event_query.clone();
            move |pool| async move { load_matching_records(&pool, &event_query).await }
        })?;

        let current_context_version = matching_records
            .last()
            .map(|event_record| event_record.sequence_number);

        let event_records = matching_records
            .into_iter()
            .filter(|event_record| {
                event_query
                    .min_sequence_number
                    .is_none_or(|min_sequence_number| {
                        event_record.sequence_number > min_sequence_number
                    })
            })
            .collect::<Vec<_>>();

        let last_returned_sequence_number = event_records
            .last()
            .map(|event_record| event_record.sequence_number);

        Ok(QueryResult {
            event_records,
            last_returned_sequence_number,
            current_context_version,
        })
    }

    fn append(&self, new_events: Vec<NewEvent>) -> Result<AppendResult, EventStoreError> {
        if new_events.is_empty() {
            return Err(EventStoreError::EmptyAppend);
        }

        let committed_append = self.run_async("append", move |pool| async move {
            let mut connection = pool.acquire().await.map_err(sqlx_backend_failure)?;

            sqlx::query("BEGIN IMMEDIATE")
                .execute(connection.as_mut())
                .await
                .map_err(sqlx_backend_failure)?;

            let append_result = append_batch(connection.as_mut(), new_events).await;

            match append_result {
                Ok(committed_append) => {
                    sqlx::query("COMMIT")
                        .execute(connection.as_mut())
                        .await
                        .map_err(sqlx_backend_failure)?;
                    Ok(committed_append)
                }
                Err(error) => {
                    let _ = sqlx::query("ROLLBACK").execute(connection.as_mut()).await;
                    Err(error)
                }
            }
        })?;

        let pending_deliveries = self.pending_deliveries(&committed_append.event_records);
        self.enqueue_delivery(pending_deliveries);
        Ok(committed_append.append_result)
    }

    fn append_if(
        &self,
        new_events: Vec<NewEvent>,
        context_query: &EventQuery,
        expected_context_version: Option<u64>,
    ) -> Result<AppendResult, EventStoreError> {
        if new_events.is_empty() {
            return Err(EventStoreError::EmptyAppend);
        }

        let committed_append = self.run_async("append_if", {
            let context_query = context_query.clone();
            move |pool| async move {
                let mut connection = pool.acquire().await.map_err(sqlx_backend_failure)?;

                sqlx::query("BEGIN IMMEDIATE")
                    .execute(connection.as_mut())
                    .await
                    .map_err(sqlx_backend_failure)?;

                let conflict_query = EventQuery {
                    filters: context_query.filters.clone(),
                    min_sequence_number: None,
                };
                let actual_context_version =
                    load_matching_records_from_connection(connection.as_mut(), &conflict_query)
                        .await?
                        .last()
                        .map(|event_record| event_record.sequence_number);

                if actual_context_version != expected_context_version {
                    sqlx::query("ROLLBACK")
                        .execute(connection.as_mut())
                        .await
                        .map_err(sqlx_backend_failure)?;

                    return Err(EventStoreError::ConditionalAppendConflict {
                        expected: expected_context_version,
                        actual: actual_context_version,
                    });
                }

                let append_result = append_batch(connection.as_mut(), new_events).await;

                match append_result {
                    Ok(committed_append) => {
                        sqlx::query("COMMIT")
                            .execute(connection.as_mut())
                            .await
                            .map_err(sqlx_backend_failure)?;
                        Ok(committed_append)
                    }
                    Err(error) => {
                        let _ = sqlx::query("ROLLBACK").execute(connection.as_mut()).await;
                        Err(error)
                    }
                }
            }
        })?;

        let pending_deliveries = self.pending_deliveries(&committed_append.event_records);
        self.enqueue_delivery(pending_deliveries);
        Ok(committed_append.append_result)
    }

    fn stream_all(&self, handle: HandleStream) -> Result<EventStream, EventStoreError> {
        let subscription_registry = Arc::clone(&self.subscription_registry);
        let id = match subscription_registry.lock() {
            Ok(mut subscription_registry) => subscription_registry.subscribe_all(handle),
            Err(poisoned) => poisoned.into_inner().subscribe_all(handle),
        };

        Ok(EventStream::new(
            id,
            Arc::new(move |subscription_id| match subscription_registry.lock() {
                Ok(mut subscription_registry) => subscription_registry.unsubscribe(subscription_id),
                Err(poisoned) => poisoned.into_inner().unsubscribe(subscription_id),
            }),
        ))
    }

    fn stream_to(
        &self,
        event_query: &EventQuery,
        handle: HandleStream,
    ) -> Result<EventStream, EventStoreError> {
        let subscription_registry = Arc::clone(&self.subscription_registry);
        let id = match subscription_registry.lock() {
            Ok(mut subscription_registry) => {
                subscription_registry.subscribe_to(Some(event_query.clone()), handle)
            }
            Err(poisoned) => poisoned
                .into_inner()
                .subscribe_to(Some(event_query.clone()), handle),
        };

        Ok(EventStream::new(
            id,
            Arc::new(move |subscription_id| match subscription_registry.lock() {
                Ok(mut subscription_registry) => subscription_registry.unsubscribe(subscription_id),
                Err(poisoned) => poisoned.into_inner().unsubscribe(subscription_id),
            }),
        ))
    }

    fn stream_all_durable(
        &self,
        durable_stream: &DurableStream,
        handle: HandleStream,
    ) -> Result<EventStream, EventStoreError> {
        self.register_all_durable_stream(durable_stream.name(), handle)
    }

    fn stream_to_durable(
        &self,
        durable_stream: &DurableStream,
        event_query: &EventQuery,
        handle: HandleStream,
    ) -> Result<EventStream, EventStoreError> {
        self.register_durable_stream(durable_stream.name(), event_query, handle)
    }
}

async fn append_batch(
    connection: &mut sqlx::SqliteConnection,
    new_events: Vec<NewEvent>,
) -> Result<CommittedAppend, EventStoreError> {
    let last_sequence_number =
        sqlx::query_scalar::<_, Option<i64>>("SELECT MAX(sequence_number) FROM events")
            .fetch_one(&mut *connection)
            .await
            .map_err(sqlx_backend_failure)?
            .unwrap_or(0);

    let first_sequence_number = (last_sequence_number as u64) + 1;
    let committed_count = new_events.len() as u64;
    let last_sequence_number = first_sequence_number + committed_count - 1;

    let event_records = new_events
        .into_iter()
        .enumerate()
        .map(|(offset, new_event)| EventRecord {
            sequence_number: first_sequence_number + offset as u64,
            occurred_at: OffsetDateTime::now_utc(),
            event_type: new_event.event_type,
            payload: new_event.payload,
        })
        .collect::<Vec<_>>();

    for event_record in &event_records {
        let payload = serde_json::to_string(&event_record.payload).map_err(json_backend_failure)?;
        let occurred_at = event_record
            .occurred_at
            .format(&Rfc3339)
            .expect("sqlite occurred_at should format as RFC3339");

        sqlx::query(
            "INSERT INTO events (sequence_number, occurred_at, event_type, payload)
             VALUES (?1, ?2, ?3, ?4)",
        )
        .bind(event_record.sequence_number as i64)
        .bind(occurred_at)
        .bind(&event_record.event_type)
        .bind(payload)
        .execute(&mut *connection)
        .await
        .map_err(sqlx_backend_failure)?;
    }

    sqlx::query(
        "INSERT INTO append_batches (first_sequence_number, last_sequence_number)
         VALUES (?1, ?2)",
    )
    .bind(first_sequence_number as i64)
    .bind(last_sequence_number as i64)
    .execute(&mut *connection)
    .await
    .map_err(sqlx_backend_failure)?;

    Ok(CommittedAppend {
        append_result: AppendResult {
            first_sequence_number,
            last_sequence_number,
            committed_count,
        },
        event_records,
    })
}

async fn load_matching_records(
    pool: &SqlitePool,
    event_query: &EventQuery,
) -> Result<Vec<EventRecord>, EventStoreError> {
    let candidate_event_types = event_type_candidates(event_query);
    let rows = fetch_candidate_rows(pool, candidate_event_types.as_deref()).await?;
    matching_records_from_rows(rows, event_query)
}

async fn load_matching_records_from_connection(
    connection: &mut sqlx::SqliteConnection,
    event_query: &EventQuery,
) -> Result<Vec<EventRecord>, EventStoreError> {
    let candidate_event_types = event_type_candidates(event_query);
    let rows =
        fetch_candidate_rows_from_connection(connection, candidate_event_types.as_deref()).await?;
    matching_records_from_rows(rows, event_query)
}

fn matching_records_from_rows(
    rows: Vec<SqliteRow>,
    event_query: &EventQuery,
) -> Result<Vec<EventRecord>, EventStoreError> {
    rows.into_iter()
        .map(row_to_event_record)
        .filter(|result| {
            result
                .as_ref()
                .is_ok_and(|event_record| matches_query(event_query, event_record))
        })
        .collect()
}

async fn fetch_candidate_rows(
    pool: &SqlitePool,
    candidate_event_types: Option<&[String]>,
) -> Result<Vec<SqliteRow>, EventStoreError> {
    let mut query_builder = QueryBuilder::<Sqlite>::new(
        "SELECT sequence_number, occurred_at, event_type, payload FROM events",
    );

    if let Some(candidate_event_types) = candidate_event_types {
        if candidate_event_types.is_empty() {
            query_builder.push(" WHERE FALSE");
        } else {
            query_builder.push(" WHERE event_type IN (");

            let mut separated = query_builder.separated(", ");
            for event_type in candidate_event_types {
                separated.push_bind(event_type);
            }
            separated.push_unseparated(")");
        }
    }

    query_builder.push(" ORDER BY sequence_number ASC");

    query_builder
        .build()
        .fetch_all(pool)
        .await
        .map_err(sqlx_backend_failure)
}

async fn fetch_candidate_rows_from_connection(
    connection: &mut sqlx::SqliteConnection,
    candidate_event_types: Option<&[String]>,
) -> Result<Vec<SqliteRow>, EventStoreError> {
    let mut query_builder = QueryBuilder::<Sqlite>::new(
        "SELECT sequence_number, occurred_at, event_type, payload FROM events",
    );

    if let Some(candidate_event_types) = candidate_event_types {
        if candidate_event_types.is_empty() {
            query_builder.push(" WHERE FALSE");
        } else {
            query_builder.push(" WHERE event_type IN (");

            let mut separated = query_builder.separated(", ");
            for event_type in candidate_event_types {
                separated.push_bind(event_type);
            }
            separated.push_unseparated(")");
        }
    }

    query_builder.push(" ORDER BY sequence_number ASC");

    query_builder
        .build()
        .fetch_all(connection)
        .await
        .map_err(sqlx_backend_failure)
}

fn event_type_candidates(event_query: &EventQuery) -> Option<Vec<String>> {
    let filters = match &event_query.filters {
        None => return None,
        Some(filters) if filters.is_empty() => return None,
        Some(filters) => filters,
    };

    if filters
        .iter()
        .any(|event_filter| event_filter.event_types.is_none())
    {
        return None;
    }

    let mut candidate_event_types = Vec::new();

    for event_filter in filters {
        if let Some(event_types) = &event_filter.event_types {
            for event_type in event_types {
                if !candidate_event_types
                    .iter()
                    .any(|known| known == event_type)
                {
                    candidate_event_types.push(event_type.clone());
                }
            }
        }
    }

    Some(candidate_event_types)
}

fn row_to_event_record(row: SqliteRow) -> Result<EventRecord, EventStoreError> {
    let payload_text = row.get::<String, _>("payload");
    let payload: Value = serde_json::from_str(&payload_text).map_err(json_backend_failure)?;

    Ok(EventRecord {
        sequence_number: row.get::<i64, _>("sequence_number") as u64,
        occurred_at: OffsetDateTime::parse(&row.get::<String, _>("occurred_at"), &Rfc3339)
            .map_err(time_backend_failure)?,
        event_type: row.get::<String, _>("event_type"),
        payload,
    })
}

fn ensure_parent_directory(database_path: &Path) -> Result<(), io::Error> {
    if let Some(parent_directory) = database_path.parent() {
        if !parent_directory.as_os_str().is_empty() {
            fs::create_dir_all(parent_directory)?;
        }
    }

    Ok(())
}

fn sqlx_io_error(error: io::Error) -> sqlx::Error {
    sqlx::Error::Io(error)
}

fn sqlx_backend_failure(error: sqlx::Error) -> EventStoreError {
    EventStoreError::BackendFailure {
        message: error.to_string(),
    }
}

fn json_backend_failure(error: serde_json::Error) -> EventStoreError {
    EventStoreError::BackendFailure {
        message: error.to_string(),
    }
}

fn time_backend_failure(error: time::error::Parse) -> EventStoreError {
    EventStoreError::BackendFailure {
        message: error.to_string(),
    }
}

fn subscription_registry_backend_failure(message: String) -> EventStoreError {
    EventStoreError::BackendFailure { message }
}

async fn load_or_create_subscriber_cursor(
    connection: &mut sqlx::SqliteConnection,
    subscriber_id: &str,
    event_query_json: &str,
) -> Result<u64, EventStoreError> {
    let existing_cursor = sqlx::query(
        "SELECT event_query, last_processed_sequence_number
         FROM subscriber_cursors
         WHERE subscriber_id = ?1",
    )
    .bind(subscriber_id)
    .fetch_optional(&mut *connection)
    .await
    .map_err(sqlx_backend_failure)?;

    match existing_cursor {
        Some(row) => {
            let stored_event_query = row.get::<String, _>("event_query");
            if stored_event_query != event_query_json {
                return Err(EventStoreError::BackendFailure {
                    message: format!(
                        "durable subscriber {subscriber_id} was resumed with a different query"
                    ),
                });
            }

            Ok(row.get::<i64, _>("last_processed_sequence_number") as u64)
        }
        None => {
            sqlx::query(
                "INSERT INTO subscriber_cursors (subscriber_id, event_query, last_processed_sequence_number)
                 VALUES (?1, ?2, 0)",
            )
            .bind(subscriber_id)
            .bind(event_query_json)
            .execute(&mut *connection)
            .await
            .map_err(sqlx_backend_failure)?;

            Ok(0)
        }
    }
}

async fn current_max_sequence_number(
    connection: &mut sqlx::SqliteConnection,
) -> Result<u64, EventStoreError> {
    Ok(
        sqlx::query_scalar::<_, Option<i64>>("SELECT MAX(sequence_number) FROM events")
            .fetch_one(&mut *connection)
            .await
            .map_err(sqlx_backend_failure)?
            .unwrap_or(0) as u64,
    )
}

async fn ensure_replay_history_is_available(
    connection: &mut sqlx::SqliteConnection,
    current_max_sequence_number: u64,
) -> Result<(), EventStoreError> {
    if current_max_sequence_number == 0 {
        return Ok(());
    }

    let batch_rows = sqlx::query(
        "SELECT first_sequence_number, last_sequence_number
         FROM append_batches
         ORDER BY first_sequence_number ASC",
    )
    .fetch_all(&mut *connection)
    .await
    .map_err(sqlx_backend_failure)?;

    if batch_rows.is_empty() {
        return Err(EventStoreError::BackendFailure {
            message: "durable replay requires append_batches history for all persisted events"
                .to_owned(),
        });
    }

    let mut expected_first_sequence_number = 1_u64;

    for batch_row in batch_rows {
        let first_sequence_number = batch_row.get::<i64, _>("first_sequence_number") as u64;
        let last_sequence_number = batch_row.get::<i64, _>("last_sequence_number") as u64;

        if first_sequence_number != expected_first_sequence_number
            || last_sequence_number < first_sequence_number
        {
            return Err(EventStoreError::BackendFailure {
                message:
                    "durable replay requires contiguous append_batches history for all persisted events"
                        .to_owned(),
            });
        }

        expected_first_sequence_number = last_sequence_number + 1;
    }

    if expected_first_sequence_number - 1 != current_max_sequence_number {
        return Err(EventStoreError::BackendFailure {
            message: "durable replay requires append_batches history for all persisted events"
                .to_owned(),
        });
    }

    Ok(())
}

async fn ensure_replay_history_is_available_for_pool(
    pool: &SqlitePool,
    current_max_sequence_number: u64,
) -> Result<(), EventStoreError> {
    let mut connection = pool.acquire().await.map_err(sqlx_backend_failure)?;
    ensure_replay_history_is_available(connection.as_mut(), current_max_sequence_number).await
}

async fn update_subscriber_cursor(
    pool: &SqlitePool,
    subscriber_id: &str,
    last_processed_sequence_number: u64,
) -> Result<(), EventStoreError> {
    sqlx::query(
        "UPDATE subscriber_cursors
         SET last_processed_sequence_number = ?2
         WHERE subscriber_id = ?1",
    )
    .bind(subscriber_id)
    .bind(last_processed_sequence_number as i64)
    .execute(pool)
    .await
    .map_err(sqlx_backend_failure)?;

    Ok(())
}

async fn load_replay_batches(
    pool: &SqlitePool,
    event_query: &EventQuery,
    last_processed_sequence_number: u64,
    replay_until_sequence_number: u64,
) -> Result<Vec<ReplayBatch>, EventStoreError> {
    if replay_until_sequence_number <= last_processed_sequence_number {
        return Ok(Vec::new());
    }

    let batch_rows = sqlx::query(
        "SELECT first_sequence_number, last_sequence_number
         FROM append_batches
         WHERE last_sequence_number > ?1
           AND first_sequence_number <= ?2
         ORDER BY first_sequence_number ASC",
    )
    .bind(last_processed_sequence_number as i64)
    .bind(replay_until_sequence_number as i64)
    .fetch_all(pool)
    .await
    .map_err(sqlx_backend_failure)?;

    let mut replay_batches = Vec::new();

    for batch_row in batch_rows {
        let first_sequence_number = batch_row.get::<i64, _>("first_sequence_number") as u64;
        let last_sequence_number = batch_row.get::<i64, _>("last_sequence_number") as u64;
        let event_rows = sqlx::query(
            "SELECT sequence_number, occurred_at, event_type, payload
             FROM events
             WHERE sequence_number >= ?1 AND sequence_number <= ?2
             ORDER BY sequence_number ASC",
        )
        .bind(first_sequence_number as i64)
        .bind(last_sequence_number as i64)
        .fetch_all(pool)
        .await
        .map_err(sqlx_backend_failure)?;

        let delivered_batch = event_rows
            .into_iter()
            .map(row_to_event_record)
            .filter(|result| {
                result
                    .as_ref()
                    .is_ok_and(|event_record| matches_query(event_query, event_record))
            })
            .collect::<Result<Vec<_>, _>>()?;

        replay_batches.push(ReplayBatch {
            last_processed_sequence_number: last_sequence_number,
            delivered_batch,
        });
    }

    Ok(replay_batches)
}

fn normalized_durable_event_query(event_query: &EventQuery) -> EventQuery {
    EventQuery {
        filters: event_query.filters.clone(),
        min_sequence_number: None,
    }
}

fn serialize_event_query(event_query: &EventQuery) -> Result<String, EventStoreError> {
    let filters = event_query.filters.as_ref().map(|filters| {
        filters
            .iter()
            .map(|filter| {
                serde_json::json!({
                    "event_types": filter.event_types,
                    "payload_predicates": filter.payload_predicates,
                })
            })
            .collect::<Vec<_>>()
    });

    serde_json::to_string(&serde_json::json!({
        "filters": filters,
        "min_sequence_number": event_query.min_sequence_number,
    }))
    .map_err(json_backend_failure)
}

fn run_delivery_thread(
    pool: SqlitePool,
    subscription_registry: Arc<Mutex<SubscriptionRegistry>>,
    delivery_receiver: Receiver<DeliveryCommand>,
) {
    let runtime = match Builder::new_current_thread().enable_all().build() {
        Ok(runtime) => runtime,
        Err(error) => {
            eprintln!("factstr-sqlite delivery runtime could not start: {}", error);
            return;
        }
    };

    while let Ok(delivery_command) = delivery_receiver.recv() {
        match delivery_command {
            DeliveryCommand::Deliver(pending_deliveries) => {
                for pending_delivery in pending_deliveries {
                    match pending_delivery.deliver() {
                        DeliveryOutcome::Succeeded {
                            subscription_id,
                            durable_subscriber_id,
                            last_processed_sequence_number,
                        } => {
                            if let Some(durable_subscriber_id) = durable_subscriber_id {
                                if let Err(error) = runtime.block_on(update_subscriber_cursor(
                                    &pool,
                                    &durable_subscriber_id,
                                    last_processed_sequence_number,
                                )) {
                                    eprintln!(
                                        "factstr-sqlite durable cursor update failed after delivery for subscriber {}: {}",
                                        durable_subscriber_id, error
                                    );
                                    match subscription_registry.lock() {
                                        Ok(mut subscription_registry) => {
                                            subscription_registry.unsubscribe(subscription_id)
                                        }
                                        Err(poisoned) => {
                                            poisoned.into_inner().unsubscribe(subscription_id)
                                        }
                                    }
                                }
                            }
                        }
                        DeliveryOutcome::Failed {
                            subscription_id,
                            durable_subscriber_id,
                        }
                        | DeliveryOutcome::Panicked {
                            subscription_id,
                            durable_subscriber_id,
                        } => {
                            if durable_subscriber_id.is_some() {
                                match subscription_registry.lock() {
                                    Ok(mut subscription_registry) => {
                                        subscription_registry.unsubscribe(subscription_id)
                                    }
                                    Err(poisoned) => {
                                        poisoned.into_inner().unsubscribe(subscription_id)
                                    }
                                }
                            }
                        }
                    }
                }
            }
            DeliveryCommand::Shutdown => break,
        }
    }
}
