use std::future::Future;
use std::io;
use std::sync::{
    Arc, Mutex,
    mpsc::{self, Receiver, Sender},
};
use std::thread::{self, JoinHandle};

use factstr::{
    AppendResult, DurableStream, EventQuery, EventRecord, EventStore, EventStoreError, EventStream,
    HandleStream, NewEvent, QueryResult,
};
use sqlx::{
    PgPool, Postgres, QueryBuilder, Row, Transaction,
    postgres::{PgPoolOptions, PgRow},
};
use time::OffsetDateTime;
use tokio::runtime::Builder;

use crate::query_match::matches_query;
use crate::query_sql::push_query_conditions;
use crate::stream_registry::{DeliveryOutcome, PendingDelivery, SubscriptionRegistry};

#[derive(Clone, Debug)]
struct CommittedAppend {
    append_result: AppendResult,
    event_records: Vec<EventRecord>,
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

enum WorkerCommand {
    Query {
        event_query: EventQuery,
        reply: Sender<Result<QueryResult, EventStoreError>>,
    },
    Append {
        new_events: Vec<NewEvent>,
        reply: Sender<Result<AppendResult, EventStoreError>>,
    },
    AppendIf {
        new_events: Vec<NewEvent>,
        context_query: EventQuery,
        expected_context_version: Option<u64>,
        reply: Sender<Result<AppendResult, EventStoreError>>,
    },
    Shutdown,
}

enum DeliveryCommand {
    Deliver(Vec<PendingDelivery>),
    Shutdown,
}

pub struct PostgresStore {
    connection_string: String,
    subscription_registry: Arc<Mutex<SubscriptionRegistry>>,
    worker_sender: Mutex<Sender<WorkerCommand>>,
    worker_thread: Mutex<Option<JoinHandle<()>>>,
    delivery_sender: Sender<DeliveryCommand>,
    delivery_thread: Mutex<Option<JoinHandle<()>>>,
}

impl PostgresStore {
    pub fn connect(connection_string: &str) -> Result<Self, sqlx::Error> {
        let connection_string = connection_string.to_owned();
        bootstrap_connection(&connection_string)?;

        let subscription_registry = Arc::new(Mutex::new(SubscriptionRegistry::default()));
        let (delivery_sender, delivery_receiver) = mpsc::channel();
        let delivery_thread = thread::Builder::new()
            .name("factstr-postgres-delivery".to_owned())
            .spawn({
                let connection_string = connection_string.clone();
                let subscription_registry = Arc::clone(&subscription_registry);
                move || {
                    run_delivery_thread(connection_string, subscription_registry, delivery_receiver)
                }
            })
            .map_err(sqlx_io_error)?;

        let (worker_sender, worker_receiver) = mpsc::channel();
        let (ready_sender, ready_receiver) = mpsc::sync_channel(1);
        let worker_thread = thread::Builder::new()
            .name("factstr-postgres-worker".to_owned())
            .spawn({
                let connection_string = connection_string.clone();
                let subscription_registry = Arc::clone(&subscription_registry);
                let delivery_sender = delivery_sender.clone();
                move || {
                    run_worker_thread(
                        connection_string,
                        worker_receiver,
                        ready_sender,
                        subscription_registry,
                        delivery_sender,
                    )
                }
            })
            .map_err(sqlx_io_error)?;

        match ready_receiver.recv() {
            Ok(Ok(())) => {}
            Ok(Err(error)) => {
                let _ = worker_thread.join();
                let _ = delivery_sender.send(DeliveryCommand::Shutdown);
                let _ = delivery_thread.join();
                return Err(error);
            }
            Err(error) => {
                let _ = worker_thread.join();
                let _ = delivery_sender.send(DeliveryCommand::Shutdown);
                let _ = delivery_thread.join();
                return Err(sqlx_io_error(io::Error::other(format!(
                    "postgres worker startup channel failed: {error}"
                ))));
            }
        }

        Ok(Self {
            connection_string,
            subscription_registry,
            worker_sender: Mutex::new(worker_sender),
            worker_thread: Mutex::new(Some(worker_thread)),
            delivery_sender,
            delivery_thread: Mutex::new(Some(delivery_thread)),
        })
    }

    fn backend_failure(error: sqlx::Error) -> EventStoreError {
        EventStoreError::BackendFailure {
            message: error.to_string(),
        }
    }

    fn worker_failure(message: impl Into<String>) -> EventStoreError {
        EventStoreError::BackendFailure {
            message: message.into(),
        }
    }

    fn send_command(&self, worker_command: WorkerCommand) -> Result<(), EventStoreError> {
        let worker_sender = self
            .worker_sender
            .lock()
            .map_err(|_| Self::worker_failure("postgres worker sender lock poisoned"))?;

        worker_sender
            .send(worker_command)
            .map_err(|error| Self::worker_failure(format!("postgres worker stopped: {error}")))
    }

    fn run_query(&self, event_query: &EventQuery) -> Result<QueryResult, EventStoreError> {
        let (reply_sender, reply_receiver) = mpsc::channel();
        self.send_command(WorkerCommand::Query {
            event_query: event_query.clone(),
            reply: reply_sender,
        })?;

        reply_receiver.recv().map_err(|error| {
            Self::worker_failure(format!("postgres worker query reply failed: {error}"))
        })?
    }

    fn run_append(&self, new_events: Vec<NewEvent>) -> Result<AppendResult, EventStoreError> {
        let (reply_sender, reply_receiver) = mpsc::channel();
        self.send_command(WorkerCommand::Append {
            new_events,
            reply: reply_sender,
        })?;

        reply_receiver.recv().map_err(|error| {
            Self::worker_failure(format!("postgres worker append reply failed: {error}"))
        })?
    }

    fn run_append_if(
        &self,
        new_events: Vec<NewEvent>,
        context_query: &EventQuery,
        expected_context_version: Option<u64>,
    ) -> Result<AppendResult, EventStoreError> {
        let (reply_sender, reply_receiver) = mpsc::channel();
        self.send_command(WorkerCommand::AppendIf {
            new_events,
            context_query: context_query.clone(),
            expected_context_version,
            reply: reply_sender,
        })?;

        reply_receiver.recv().map_err(|error| {
            Self::worker_failure(format!(
                "postgres worker conditional append reply failed: {error}"
            ))
        })?
    }

    fn run_async<T, Fut, F>(&self, operation: &'static str, work: F) -> Result<T, EventStoreError>
    where
        T: Send + 'static,
        Fut: Future<Output = Result<T, EventStoreError>> + Send + 'static,
        F: FnOnce(PgPool) -> Fut + Send + 'static,
    {
        let connection_string = self.connection_string.clone();
        let (result_sender, result_receiver) = mpsc::sync_channel(1);

        let worker_thread = thread::Builder::new()
            .name(format!("factstr-postgres-{operation}"))
            .spawn(move || {
                let runtime = Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .map_err(sqlx_io_error)
                    .map_err(Self::backend_failure);

                let result = match runtime {
                    Ok(runtime) => runtime.block_on(async {
                        let pool = PgPoolOptions::new()
                            .max_connections(1)
                            .connect(&connection_string)
                            .await
                            .map_err(Self::backend_failure)?;
                        work(pool).await
                    }),
                    Err(error) => Err(error),
                };

                let _ = result_sender.send(result);
            })
            .map_err(sqlx_io_error)
            .map_err(Self::backend_failure)?;

        let result = result_receiver
            .recv()
            .map_err(|error| EventStoreError::BackendFailure {
                message: format!("postgres {operation} thread did not return a result: {error}"),
            })?;

        worker_thread
            .join()
            .map_err(|_| EventStoreError::BackendFailure {
                message: format!("postgres {operation} thread panicked before completion"),
            })?;

        result
    }

    fn enqueue_delivery(&self, pending_deliveries: Vec<PendingDelivery>) {
        Self::enqueue_delivery_with_sender(&self.delivery_sender, pending_deliveries);
    }

    fn enqueue_delivery_with_sender(
        delivery_sender: &Sender<DeliveryCommand>,
        pending_deliveries: Vec<PendingDelivery>,
    ) {
        if pending_deliveries.is_empty() {
            return;
        }

        if let Err(error) = delivery_sender.send(DeliveryCommand::Deliver(pending_deliveries)) {
            eprintln!(
                "factstr-postgres delivery dispatcher stopped after commit: {}",
                error
            );
        }
    }

    fn register_all_durable_stream(
        &self,
        durable_stream_id: impl Into<String>,
        handle: HandleStream,
    ) -> Result<EventStream, EventStoreError> {
        self.stream_durable(durable_stream_id.into(), EventQuery::all(), handle)
    }

    fn register_durable_stream(
        &self,
        durable_stream_id: impl Into<String>,
        event_query: &EventQuery,
        handle: HandleStream,
    ) -> Result<EventStream, EventStoreError> {
        self.stream_durable(durable_stream_id.into(), event_query.clone(), handle)
    }

    fn stream_durable(
        &self,
        durable_stream_id: String,
        event_query: EventQuery,
        handle: HandleStream,
    ) -> Result<EventStream, EventStoreError> {
        let normalized_event_query = normalized_durable_event_query(&event_query);
        let event_query_json = serialize_event_query(&normalized_event_query)?;
        let subscription_registry = Arc::clone(&self.subscription_registry);
        let durable_stream_id_for_registry = durable_stream_id.clone();
        let normalized_event_query_for_registry = normalized_event_query.clone();
        let handle_for_registry = handle.clone();
        let replay_stream_id = durable_stream_id.clone();

        let replay_state = self.run_async("stream_durable", move |pool| async move {
            let mut transaction = pool.begin().await.map_err(Self::backend_failure)?;

            let last_processed_sequence_number = load_or_create_durable_stream_cursor(
                &mut transaction,
                &durable_stream_id_for_registry,
                &event_query_json,
            )
            .await?;
            let replay_until_sequence_number =
                current_max_sequence_number_in_transaction(&mut transaction).await?;

            let subscription_id = match subscription_registry.lock() {
                Ok(mut subscription_registry) => {
                    if normalized_event_query_for_registry.filters.is_none() {
                        subscription_registry
                            .subscribe_all_durable(
                                durable_stream_id_for_registry.clone(),
                                replay_until_sequence_number,
                                handle_for_registry.clone(),
                            )
                            .map_err(subscription_registry_backend_failure)?
                    } else {
                        subscription_registry
                            .subscribe_to_durable(
                                durable_stream_id_for_registry.clone(),
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
                                durable_stream_id_for_registry.clone(),
                                replay_until_sequence_number,
                                handle_for_registry.clone(),
                            )
                            .map_err(subscription_registry_backend_failure)?
                    } else {
                        subscription_registry
                            .subscribe_to_durable(
                                durable_stream_id_for_registry.clone(),
                                Some(normalized_event_query_for_registry.clone()),
                                replay_until_sequence_number,
                                handle_for_registry.clone(),
                            )
                            .map_err(subscription_registry_backend_failure)?
                    }
                }
            };

            if let Err(error) = transaction.commit().await {
                match subscription_registry.lock() {
                    Ok(mut subscription_registry) => {
                        subscription_registry.unsubscribe(subscription_id)
                    }
                    Err(poisoned) => poisoned.into_inner().unsubscribe(subscription_id),
                }
                return Err(Self::backend_failure(error));
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
                durable_stream_id: Some(replay_stream_id.clone()),
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
                            "durable replay for stream {} did not complete successfully",
                            replay_stream_id
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
                durable_stream_id,
                last_processed_sequence_number,
                ..
            } => {
                if let Some(durable_stream_id) = durable_stream_id {
                    self.run_async("advance_durable_cursor", move |pool| async move {
                        update_durable_stream_cursor(
                            &pool,
                            &durable_stream_id,
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

impl Drop for PostgresStore {
    fn drop(&mut self) {
        if let Ok(worker_sender) = self.worker_sender.lock() {
            let _ = worker_sender.send(WorkerCommand::Shutdown);
        }

        if let Ok(mut worker_thread) = self.worker_thread.lock() {
            if let Some(worker_thread) = worker_thread.take() {
                let _ = worker_thread.join();
            }
        }

        let _ = self.delivery_sender.send(DeliveryCommand::Shutdown);

        if let Ok(mut delivery_thread) = self.delivery_thread.lock() {
            if let Some(delivery_thread) = delivery_thread.take() {
                let _ = delivery_thread.join();
            }
        }
    }
}

impl EventStore for PostgresStore {
    fn query(&self, event_query: &EventQuery) -> Result<QueryResult, EventStoreError> {
        self.run_query(event_query)
    }

    fn append(&self, new_events: Vec<NewEvent>) -> Result<AppendResult, EventStoreError> {
        if new_events.is_empty() {
            return Err(EventStoreError::EmptyAppend);
        }

        self.run_append(new_events)
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

        self.run_append_if(new_events, context_query, expected_context_version)
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

fn run_worker_thread(
    connection_string: String,
    worker_receiver: Receiver<WorkerCommand>,
    ready_sender: mpsc::SyncSender<Result<(), sqlx::Error>>,
    subscription_registry: Arc<Mutex<SubscriptionRegistry>>,
    delivery_sender: Sender<DeliveryCommand>,
) {
    let runtime = match Builder::new_current_thread().enable_all().build() {
        Ok(runtime) => runtime,
        Err(error) => {
            let _ = ready_sender.send(Err(sqlx_io_error(io::Error::other(format!(
                "tokio runtime should build: {error}"
            )))));
            return;
        }
    };

    let pool = match runtime.block_on(async {
        PgPoolOptions::new()
            .max_connections(1)
            .connect(&connection_string)
            .await
    }) {
        Ok(pool) => pool,
        Err(error) => {
            let _ = ready_sender.send(Err(error));
            return;
        }
    };

    if ready_sender.send(Ok(())).is_err() {
        return;
    }

    while let Ok(worker_command) = worker_receiver.recv() {
        match worker_command {
            WorkerCommand::Query { event_query, reply } => {
                let result = runtime
                    .block_on(query_with_pool(&pool, &event_query))
                    .map_err(PostgresStore::backend_failure);
                let _ = reply.send(result);
            }
            WorkerCommand::Append { new_events, reply } => {
                let result = runtime
                    .block_on(append_with_pool(&pool, new_events))
                    .map_err(PostgresStore::backend_failure);
                if let Ok(committed_append) = &result {
                    let pending_deliveries = match subscription_registry.lock() {
                        Ok(mut subscription_registry) => subscription_registry
                            .pending_deliveries(&committed_append.event_records),
                        Err(poisoned) => poisoned
                            .into_inner()
                            .pending_deliveries(&committed_append.event_records),
                    };
                    PostgresStore::enqueue_delivery_with_sender(
                        &delivery_sender,
                        pending_deliveries,
                    );
                }
                let result = result.map(|committed_append| committed_append.append_result);
                let _ = reply.send(result);
            }
            WorkerCommand::AppendIf {
                new_events,
                context_query,
                expected_context_version,
                reply,
            } => {
                let result = runtime
                    .block_on(append_if_with_pool(
                        &pool,
                        new_events,
                        &context_query,
                        expected_context_version,
                    ))
                    .map_err(PostgresStore::backend_failure)
                    .and_then(|result| result);
                if let Ok(committed_append) = &result {
                    let pending_deliveries = match subscription_registry.lock() {
                        Ok(mut subscription_registry) => subscription_registry
                            .pending_deliveries(&committed_append.event_records),
                        Err(poisoned) => poisoned
                            .into_inner()
                            .pending_deliveries(&committed_append.event_records),
                    };
                    PostgresStore::enqueue_delivery_with_sender(
                        &delivery_sender,
                        pending_deliveries,
                    );
                }
                let result = result.map(|committed_append| committed_append.append_result);
                let _ = reply.send(result);
            }
            WorkerCommand::Shutdown => break,
        }
    }
}

fn run_delivery_thread(
    connection_string: String,
    subscription_registry: Arc<Mutex<SubscriptionRegistry>>,
    delivery_receiver: Receiver<DeliveryCommand>,
) {
    let runtime = match Builder::new_current_thread().enable_all().build() {
        Ok(runtime) => runtime,
        Err(error) => {
            eprintln!(
                "factstr-postgres delivery runtime could not start: {}",
                error
            );
            return;
        }
    };

    let pool = match runtime.block_on(async {
        PgPoolOptions::new()
            .max_connections(1)
            .connect(&connection_string)
            .await
    }) {
        Ok(pool) => pool,
        Err(error) => {
            eprintln!(
                "factstr-postgres delivery pool could not connect: {}",
                error
            );
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
                            durable_stream_id,
                            last_processed_sequence_number,
                        } => {
                            if let Some(durable_stream_id) = durable_stream_id {
                                if let Err(error) = runtime.block_on(update_durable_stream_cursor(
                                    &pool,
                                    &durable_stream_id,
                                    last_processed_sequence_number,
                                )) {
                                    eprintln!(
                                        "factstr-postgres durable cursor update failed after delivery for stream {}: {}",
                                        durable_stream_id, error
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
                            durable_stream_id,
                        }
                        | DeliveryOutcome::Panicked {
                            subscription_id,
                            durable_stream_id,
                        } => {
                            if durable_stream_id.is_some() {
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

fn sqlx_io_error(error: io::Error) -> sqlx::Error {
    sqlx::Error::Io(error)
}

fn bootstrap_connection(connection_string: &str) -> Result<(), sqlx::Error> {
    let connection_string = connection_string.to_owned();
    let (result_sender, result_receiver) = mpsc::sync_channel(1);

    let bootstrap_thread = thread::Builder::new()
        .name("factstr-postgres-bootstrap".to_owned())
        .spawn(move || {
            let runtime = Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(sqlx_io_error);

            let result = match runtime {
                Ok(runtime) => runtime.block_on(async {
                    let pool = PgPoolOptions::new()
                        .max_connections(1)
                        .connect(&connection_string)
                        .await?;
                    initialize_schema(&pool).await?;
                    Ok::<_, sqlx::Error>(())
                }),
                Err(error) => Err(error),
            };

            let _ = result_sender.send(result);
        })
        .map_err(sqlx_io_error)?;

    let result = match result_receiver.recv() {
        Ok(result) => result,
        Err(error) => Err(sqlx_io_error(io::Error::other(format!(
            "postgres bootstrap thread did not return a result: {error}"
        )))),
    };

    bootstrap_thread.join().map_err(|_| {
        sqlx_io_error(io::Error::other(
            "postgres bootstrap thread panicked before connect completed",
        ))
    })?;

    result
}

async fn query_with_pool(
    pool: &PgPool,
    event_query: &EventQuery,
) -> Result<QueryResult, sqlx::Error> {
    let current_context_version = current_context_version(pool, event_query).await?;

    let mut query_builder: QueryBuilder<'_, Postgres> =
        QueryBuilder::new("SELECT sequence_number, occurred_at, event_type, payload FROM events");
    push_query_conditions(&mut query_builder, event_query, true);
    query_builder.push(" ORDER BY sequence_number ASC");

    let event_records = query_builder
        .build()
        .fetch_all(pool)
        .await?
        .into_iter()
        .map(event_record_from_row)
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

async fn append_with_pool(
    pool: &PgPool,
    new_events: Vec<NewEvent>,
) -> Result<CommittedAppend, sqlx::Error> {
    let mut transaction = pool.begin().await?;
    lock_events_tables(&mut transaction).await?;
    let committed_append = append_records(&mut transaction, new_events).await?;
    transaction.commit().await?;
    Ok(committed_append)
}

async fn append_if_with_pool(
    pool: &PgPool,
    new_events: Vec<NewEvent>,
    context_query: &EventQuery,
    expected_context_version: Option<u64>,
) -> Result<Result<CommittedAppend, EventStoreError>, sqlx::Error> {
    let mut transaction = pool.begin().await?;
    lock_events_tables(&mut transaction).await?;

    let actual_context_version =
        current_context_version_in_transaction(&mut transaction, context_query).await?;

    if actual_context_version != expected_context_version {
        transaction.rollback().await?;
        return Ok(Err(EventStoreError::ConditionalAppendConflict {
            expected: expected_context_version,
            actual: actual_context_version,
        }));
    }

    let committed_append = append_records(&mut transaction, new_events).await?;
    transaction.commit().await?;

    Ok(Ok(committed_append))
}

async fn initialize_schema(pool: &PgPool) -> Result<(), sqlx::Error> {
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS events (
            sequence_number BIGINT PRIMARY KEY,
            occurred_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            event_type TEXT NOT NULL,
            payload JSONB NOT NULL
        )",
    )
    .execute(pool)
    .await?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS append_batches (
            first_sequence_number BIGINT PRIMARY KEY,
            last_sequence_number BIGINT NOT NULL
        )",
    )
    .execute(pool)
    .await?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS durable_stream_cursors (
            durable_stream_id TEXT PRIMARY KEY,
            event_query TEXT NOT NULL,
            last_processed_sequence_number BIGINT NOT NULL
        )",
    )
    .execute(pool)
    .await?;

    sqlx::query("CREATE INDEX IF NOT EXISTS idx_events_type ON events(event_type)")
        .execute(pool)
        .await?;

    sqlx::query("CREATE INDEX IF NOT EXISTS idx_events_occurred_at ON events(occurred_at)")
        .execute(pool)
        .await?;

    sqlx::query("CREATE INDEX IF NOT EXISTS idx_events_payload_gin ON events USING gin(payload)")
        .execute(pool)
        .await?;

    Ok(())
}

async fn lock_events_tables(
    transaction: &mut Transaction<'_, Postgres>,
) -> Result<(), sqlx::Error> {
    sqlx::query("LOCK TABLE events, append_batches IN EXCLUSIVE MODE")
        .execute(transaction.as_mut())
        .await?;

    Ok(())
}

async fn append_records(
    transaction: &mut Transaction<'_, Postgres>,
    new_events: Vec<NewEvent>,
) -> Result<CommittedAppend, sqlx::Error> {
    let committed_count = new_events.len() as u64;
    let first_sequence_number =
        sqlx::query_scalar::<_, i64>("SELECT COALESCE(MAX(sequence_number), 0) + 1 FROM events")
            .fetch_one(transaction.as_mut())
            .await? as u64;
    let last_sequence_number = first_sequence_number + committed_count - 1;
    let committed_event_records = new_events
        .into_iter()
        .enumerate()
        .map(|(offset, new_event)| EventRecord {
            sequence_number: first_sequence_number + offset as u64,
            occurred_at: OffsetDateTime::now_utc(),
            event_type: new_event.event_type,
            payload: new_event.payload,
        })
        .collect::<Vec<_>>();

    for event_record in &committed_event_records {
        sqlx::query(
            "INSERT INTO events (sequence_number, occurred_at, event_type, payload)
             VALUES ($1, $2, $3, $4)",
        )
        .bind(event_record.sequence_number as i64)
        .bind(event_record.occurred_at)
        .bind(&event_record.event_type)
        .bind(&event_record.payload)
        .execute(transaction.as_mut())
        .await?;
    }

    sqlx::query(
        "INSERT INTO append_batches (first_sequence_number, last_sequence_number)
         VALUES ($1, $2)",
    )
    .bind(first_sequence_number as i64)
    .bind(last_sequence_number as i64)
    .execute(transaction.as_mut())
    .await?;

    Ok(CommittedAppend {
        append_result: AppendResult {
            first_sequence_number,
            last_sequence_number,
            committed_count,
        },
        event_records: committed_event_records,
    })
}

async fn current_context_version(
    pool: &PgPool,
    event_query: &EventQuery,
) -> Result<Option<u64>, sqlx::Error> {
    let mut query_builder: QueryBuilder<'_, Postgres> =
        QueryBuilder::new("SELECT MAX(sequence_number) FROM events");
    push_query_conditions(&mut query_builder, event_query, false);

    Ok(query_builder
        .build_query_scalar::<Option<i64>>()
        .fetch_one(pool)
        .await?
        .map(|sequence_number| sequence_number as u64))
}

async fn current_context_version_in_transaction(
    transaction: &mut Transaction<'_, Postgres>,
    event_query: &EventQuery,
) -> Result<Option<u64>, sqlx::Error> {
    let mut query_builder: QueryBuilder<'_, Postgres> =
        QueryBuilder::new("SELECT MAX(sequence_number) FROM events");
    push_query_conditions(&mut query_builder, event_query, false);

    Ok(query_builder
        .build_query_scalar::<Option<i64>>()
        .fetch_one(transaction.as_mut())
        .await?
        .map(|sequence_number| sequence_number as u64))
}

fn event_record_from_row(row: PgRow) -> EventRecord {
    EventRecord {
        sequence_number: row.get::<i64, _>("sequence_number") as u64,
        occurred_at: row.get("occurred_at"),
        event_type: row.get("event_type"),
        payload: row.get("payload"),
    }
}

fn json_backend_failure(error: serde_json::Error) -> EventStoreError {
    EventStoreError::BackendFailure {
        message: error.to_string(),
    }
}

fn subscription_registry_backend_failure(message: String) -> EventStoreError {
    EventStoreError::BackendFailure { message }
}

async fn load_or_create_durable_stream_cursor(
    transaction: &mut Transaction<'_, Postgres>,
    durable_stream_id: &str,
    event_query_json: &str,
) -> Result<u64, EventStoreError> {
    let existing_cursor = sqlx::query(
        "SELECT event_query, last_processed_sequence_number
         FROM durable_stream_cursors
         WHERE durable_stream_id = $1",
    )
    .bind(durable_stream_id)
    .fetch_optional(transaction.as_mut())
    .await
    .map_err(PostgresStore::backend_failure)?;

    match existing_cursor {
        Some(row) => {
            let stored_event_query = row.get::<String, _>("event_query");
            if stored_event_query != event_query_json {
                return Err(EventStoreError::BackendFailure {
                    message: format!(
                        "durable stream {durable_stream_id} was resumed with a different query"
                    ),
                });
            }

            Ok(row.get::<i64, _>("last_processed_sequence_number") as u64)
        }
        None => {
            sqlx::query(
                "INSERT INTO durable_stream_cursors (
                    durable_stream_id,
                    event_query,
                    last_processed_sequence_number
                 ) VALUES ($1, $2, 0)",
            )
            .bind(durable_stream_id)
            .bind(event_query_json)
            .execute(transaction.as_mut())
            .await
            .map_err(PostgresStore::backend_failure)?;

            Ok(0)
        }
    }
}

async fn current_max_sequence_number_in_transaction(
    transaction: &mut Transaction<'_, Postgres>,
) -> Result<u64, EventStoreError> {
    Ok(
        sqlx::query_scalar::<_, Option<i64>>("SELECT MAX(sequence_number) FROM events")
            .fetch_one(transaction.as_mut())
            .await
            .map_err(PostgresStore::backend_failure)?
            .unwrap_or(0) as u64,
    )
}

async fn ensure_replay_history_is_available(
    connection: &mut sqlx::PgConnection,
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
    .map_err(PostgresStore::backend_failure)?;

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
    pool: &PgPool,
    current_max_sequence_number: u64,
) -> Result<(), EventStoreError> {
    let mut connection = pool
        .acquire()
        .await
        .map_err(PostgresStore::backend_failure)?;
    ensure_replay_history_is_available(connection.as_mut(), current_max_sequence_number).await
}

async fn update_durable_stream_cursor(
    pool: &PgPool,
    durable_stream_id: &str,
    last_processed_sequence_number: u64,
) -> Result<(), EventStoreError> {
    sqlx::query(
        "UPDATE durable_stream_cursors
         SET last_processed_sequence_number = $2
         WHERE durable_stream_id = $1",
    )
    .bind(durable_stream_id)
    .bind(last_processed_sequence_number as i64)
    .execute(pool)
    .await
    .map_err(PostgresStore::backend_failure)?;

    Ok(())
}

async fn load_replay_batches(
    pool: &PgPool,
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
         WHERE last_sequence_number > $1
           AND first_sequence_number <= $2
         ORDER BY first_sequence_number ASC",
    )
    .bind(last_processed_sequence_number as i64)
    .bind(replay_until_sequence_number as i64)
    .fetch_all(pool)
    .await
    .map_err(PostgresStore::backend_failure)?;

    let mut replay_batches = Vec::new();

    for batch_row in batch_rows {
        let first_sequence_number = batch_row.get::<i64, _>("first_sequence_number") as u64;
        let last_sequence_number = batch_row.get::<i64, _>("last_sequence_number") as u64;
        let event_rows = sqlx::query(
            "SELECT sequence_number, event_type, payload
             FROM events
             WHERE sequence_number >= $1 AND sequence_number <= $2
             ORDER BY sequence_number ASC",
        )
        .bind(first_sequence_number as i64)
        .bind(last_sequence_number as i64)
        .fetch_all(pool)
        .await
        .map_err(PostgresStore::backend_failure)?;

        let delivered_batch = event_rows
            .into_iter()
            .map(event_record_from_row)
            .filter(|event_record| matches_query(event_query, event_record))
            .collect::<Vec<_>>();

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
