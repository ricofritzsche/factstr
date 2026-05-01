use napi::bindgen_prelude::BigInt;
use napi_derive::napi;
use serde_json::Value;

use crate::sequence_number_value::{bigint_from_u64, option_bigint_from_u64};

#[napi(object)]
#[derive(Clone, Debug)]
pub struct EventRecord {
    #[napi(js_name = "sequence_number")]
    pub sequence_number: BigInt,
    #[napi(js_name = "occurred_at")]
    pub occurred_at: String,
    #[napi(js_name = "event_type")]
    pub event_type: String,
    pub payload: Value,
}

impl From<factstr_interop::InteropEventRecord> for EventRecord {
    fn from(interop_event_record: factstr_interop::InteropEventRecord) -> Self {
        Self {
            sequence_number: bigint_from_u64(interop_event_record.sequence_number),
            occurred_at: interop_event_record.occurred_at,
            event_type: interop_event_record.event_type,
            payload: interop_event_record.payload,
        }
    }
}

#[napi(object)]
#[derive(Clone, Debug, Default)]
pub struct QueryResult {
    #[napi(js_name = "event_records")]
    pub event_records: Vec<EventRecord>,
    #[napi(js_name = "last_returned_sequence_number")]
    pub last_returned_sequence_number: Option<BigInt>,
    #[napi(js_name = "current_context_version")]
    pub current_context_version: Option<BigInt>,
}

impl From<factstr_interop::InteropQueryResult> for QueryResult {
    fn from(interop_query_result: factstr_interop::InteropQueryResult) -> Self {
        Self {
            event_records: interop_query_result
                .event_records
                .into_iter()
                .map(Into::into)
                .collect(),
            last_returned_sequence_number: option_bigint_from_u64(
                interop_query_result.last_returned_sequence_number,
            ),
            current_context_version: option_bigint_from_u64(
                interop_query_result.current_context_version,
            ),
        }
    }
}

#[napi(object)]
#[derive(Clone, Debug)]
pub struct AppendResult {
    #[napi(js_name = "first_sequence_number")]
    pub first_sequence_number: BigInt,
    #[napi(js_name = "last_sequence_number")]
    pub last_sequence_number: BigInt,
    #[napi(js_name = "committed_count")]
    pub committed_count: BigInt,
}

impl From<factstr_interop::InteropAppendResult> for AppendResult {
    fn from(interop_append_result: factstr_interop::InteropAppendResult) -> Self {
        Self {
            first_sequence_number: bigint_from_u64(interop_append_result.first_sequence_number),
            last_sequence_number: bigint_from_u64(interop_append_result.last_sequence_number),
            committed_count: bigint_from_u64(interop_append_result.committed_count),
        }
    }
}

#[napi(object)]
#[derive(Clone, Debug)]
pub struct ConditionalAppendConflict {
    #[napi(js_name = "expected_context_version")]
    pub expected_context_version: Option<BigInt>,
    #[napi(js_name = "actual_context_version")]
    pub actual_context_version: Option<BigInt>,
}

impl From<factstr_interop::InteropConditionalAppendConflict> for ConditionalAppendConflict {
    fn from(interop_conflict: factstr_interop::InteropConditionalAppendConflict) -> Self {
        Self {
            expected_context_version: option_bigint_from_u64(
                interop_conflict.expected_context_version,
            ),
            actual_context_version: option_bigint_from_u64(interop_conflict.actual_context_version),
        }
    }
}
