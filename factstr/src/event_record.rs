use serde_json::Value;
use time::OffsetDateTime;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EventRecord {
    pub sequence_number: u64,
    pub occurred_at: OffsetDateTime,
    pub event_type: String,
    pub payload: Value,
}
