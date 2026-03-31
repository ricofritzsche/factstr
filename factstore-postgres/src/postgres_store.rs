use std::io;
use std::sync::{
    Mutex,
    mpsc::{self, Receiver, Sender, SyncSender},
};
use std::thread::{self, JoinHandle};

use factstore::{
    AppendResult, EventQuery, EventRecord, EventStore, EventStoreError, LiveSubscription, NewEvent,
    QueryResult,
};
use sqlx::{
    PgPool, Postgres, QueryBuilder, Row, Transaction,
    postgres::{PgPoolOptions, PgRow},
};
use tokio::runtime::Builder;

use crate::query_sql::push_query_conditions;
use crate::subscription_registry::SubscriptionRegistry;

pub struct PostgresStore {
    worker_sender: Mutex<Sender<WorkerCommand>>,
    worker_thread: Mutex<Option<JoinHandle<()>>>,
}

enum WorkerCommand {
    Query {
        event_query: EventQuery,
        reply: Sender<Result<QueryResult, EventStoreError>>,
    },
    SubscribeAll {
        reply: Sender<LiveSubscription>,
    },
    SubscribeTo {
        event_query: EventQuery,
        reply: Sender<LiveSubscription>,
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

#[derive(Clone, Debug)]
struct CommittedAppend {
    append_result: AppendResult,
    event_records: Vec<EventRecord>,
}

impl PostgresStore {
    pub fn connect(connection_string: &str) -> Result<Self, sqlx::Error> {
        let (worker_sender, worker_receiver) = mpsc::channel();
        let (ready_sender, ready_receiver) = mpsc::sync_channel(1);
        let connection_string = connection_string.to_owned();

        let worker_thread = thread::Builder::new()
            .name("factstore-postgres-worker".to_owned())
            .spawn(move || run_worker_thread(connection_string, worker_receiver, ready_sender))
            .map_err(sqlx_io_error)?;

        match ready_receiver.recv() {
            Ok(Ok(())) => Ok(Self {
                worker_sender: Mutex::new(worker_sender),
                worker_thread: Mutex::new(Some(worker_thread)),
            }),
            Ok(Err(error)) => {
                let _ = worker_thread.join();
                Err(error)
            }
            Err(error) => {
                let _ = worker_thread.join();
                Err(sqlx_io_error(io::Error::other(format!(
                    "postgres worker startup channel failed: {error}"
                ))))
            }
        }
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

    fn run_subscribe_all(&self) -> Result<LiveSubscription, EventStoreError> {
        let (reply_sender, reply_receiver) = mpsc::channel();
        self.send_command(WorkerCommand::SubscribeAll {
            reply: reply_sender,
        })?;

        reply_receiver.recv().map_err(|error| {
            Self::worker_failure(format!("postgres worker subscribe reply failed: {error}"))
        })
    }

    fn run_subscribe_to(
        &self,
        event_query: &EventQuery,
    ) -> Result<LiveSubscription, EventStoreError> {
        let (reply_sender, reply_receiver) = mpsc::channel();
        self.send_command(WorkerCommand::SubscribeTo {
            event_query: event_query.clone(),
            reply: reply_sender,
        })?;

        reply_receiver.recv().map_err(|error| {
            Self::worker_failure(format!("postgres worker subscribe reply failed: {error}"))
        })
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

    fn subscribe_all(&self) -> Result<LiveSubscription, EventStoreError> {
        self.run_subscribe_all()
    }

    fn subscribe_to(&self, event_query: &EventQuery) -> Result<LiveSubscription, EventStoreError> {
        self.run_subscribe_to(event_query)
    }
}

fn run_worker_thread(
    connection_string: String,
    worker_receiver: Receiver<WorkerCommand>,
    ready_sender: SyncSender<Result<(), sqlx::Error>>,
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

    if let Err(error) = runtime.block_on(initialize_schema(&pool)) {
        let _ = ready_sender.send(Err(error));
        return;
    }

    if ready_sender.send(Ok(())).is_err() {
        return;
    }

    let mut subscription_registry = SubscriptionRegistry::default();
    while let Ok(worker_command) = worker_receiver.recv() {
        match worker_command {
            WorkerCommand::Query { event_query, reply } => {
                let result = runtime
                    .block_on(query_with_pool(&pool, &event_query))
                    .map_err(PostgresStore::backend_failure);
                let _ = reply.send(result);
            }
            WorkerCommand::SubscribeAll { reply } => {
                let _ = reply.send(subscription_registry.subscribe_all());
            }
            WorkerCommand::SubscribeTo { event_query, reply } => {
                let _ = reply.send(subscription_registry.subscribe_to(Some(event_query)));
            }
            WorkerCommand::Append { new_events, reply } => {
                let result = runtime
                    .block_on(append_with_pool(&pool, new_events))
                    .map_err(PostgresStore::backend_failure)
                    .map(|committed_append| {
                        subscription_registry.notify(&committed_append.event_records);
                        committed_append.append_result
                    });
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
                    .and_then(|result| result)
                    .map(|committed_append| {
                        subscription_registry.notify(&committed_append.event_records);
                        committed_append.append_result
                    });
                let _ = reply.send(result);
            }
            WorkerCommand::Shutdown => break,
        }
    }
}

fn sqlx_io_error(error: io::Error) -> sqlx::Error {
    sqlx::Error::Io(error.into())
}

async fn query_with_pool(
    pool: &PgPool,
    event_query: &EventQuery,
) -> Result<QueryResult, sqlx::Error> {
    let current_context_version = current_context_version(pool, event_query).await?;

    let mut query_builder: QueryBuilder<'_, Postgres> =
        QueryBuilder::new("SELECT sequence_number, event_type, payload FROM events");
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
    lock_events_table(&mut transaction).await?;
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
    lock_events_table(&mut transaction).await?;

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
    // The TypeScript baseline uses BIGSERIAL for sequence_number. This store keeps explicit
    // sequence assignment instead so committed batches retain one consecutive range without
    // gaps from failed appends or rolled-back conditional appends.
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

async fn lock_events_table(transaction: &mut Transaction<'_, Postgres>) -> Result<(), sqlx::Error> {
    sqlx::query("LOCK TABLE events IN EXCLUSIVE MODE")
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
            event_type: new_event.event_type,
            payload: new_event.payload,
        })
        .collect::<Vec<_>>();

    for event_record in &committed_event_records {
        sqlx::query(
            "INSERT INTO events (sequence_number, event_type, payload)
             VALUES ($1, $2, $3)",
        )
        .bind(event_record.sequence_number as i64)
        .bind(&event_record.event_type)
        .bind(&event_record.payload)
        .execute(transaction.as_mut())
        .await?;
    }

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
        event_type: row.get("event_type"),
        payload: row.get("payload"),
    }
}
