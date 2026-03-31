use crate::{AppendResult, EventQuery, LiveSubscription, NewEvent, QueryResult};
use std::error::Error;
use std::fmt::{Display, Formatter};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EventStoreError {
    EmptyAppend,
    ConditionalAppendConflict {
        expected: Option<u64>,
        actual: Option<u64>,
    },
    BackendFailure {
        message: String,
    },
}

impl Display for EventStoreError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyAppend => formatter.write_str("append requires at least one event"),
            Self::ConditionalAppendConflict { expected, actual } => {
                write!(
                    formatter,
                    "context version mismatch: expected {expected:?}, actual {actual:?}"
                )
            }
            Self::BackendFailure { message } => write!(formatter, "backend failure: {message}"),
        }
    }
}

impl Error for EventStoreError {}

/// Shared event-store contract across all store implementations.
///
/// Load-bearing sequence guarantees:
/// - sequence numbers are global and monotonically increasing
/// - one committed batch receives one consecutive sequence range
/// - failed appends and failed conditional appends must not partially commit a batch
/// - under the current Rust contract, failed appends also must not consume
///   sequence numbers that later successful commits would observe
/// - live subscriptions observe only batches committed after subscription becomes active
/// - `subscribe_all` delivers each successful committed append as one committed
///   batch in commit order
/// - `subscribe_to` uses `EventQuery` matching semantics to deliver only the
///   matching facts from each future committed batch, preserving their original
///   committed order inside one filtered delivered batch
/// - failed conditional appends deliver nothing
/// - slow or disconnected subscribers must not weaken append correctness
pub trait EventStore {
    fn query(&self, event_query: &EventQuery) -> Result<QueryResult, EventStoreError>;

    fn append(&self, new_events: Vec<NewEvent>) -> Result<AppendResult, EventStoreError>;

    fn append_if(
        &self,
        new_events: Vec<NewEvent>,
        context_query: &EventQuery,
        expected_context_version: Option<u64>,
    ) -> Result<AppendResult, EventStoreError>;

    fn subscribe_all(&self) -> Result<LiveSubscription, EventStoreError>;

    fn subscribe_to(&self, event_query: &EventQuery) -> Result<LiveSubscription, EventStoreError>;

    fn subscribe(&self) -> Result<LiveSubscription, EventStoreError> {
        self.subscribe_all()
    }
}
