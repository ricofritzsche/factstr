use factstr::EventRecord;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct InteropEventRecord {
    pub sequence_number: u64,
    pub occurred_at: String,
    pub event_type: String,
    pub payload: Value,
}

impl From<InteropEventRecord> for EventRecord {
    fn from(interop_event_record: InteropEventRecord) -> Self {
        Self {
            sequence_number: interop_event_record.sequence_number,
            occurred_at: OffsetDateTime::parse(&interop_event_record.occurred_at, &Rfc3339)
                .expect("interop occurred_at should be valid RFC3339"),
            event_type: interop_event_record.event_type,
            payload: interop_event_record.payload,
        }
    }
}

impl From<EventRecord> for InteropEventRecord {
    fn from(event_record: EventRecord) -> Self {
        Self {
            sequence_number: event_record.sequence_number,
            occurred_at: event_record
                .occurred_at
                .format(&Rfc3339)
                .expect("event record occurred_at should format as RFC3339"),
            event_type: event_record.event_type,
            payload: event_record.payload,
        }
    }
}
