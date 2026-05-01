use napi::bindgen_prelude::BigInt;
use napi_derive::napi;
use serde_json::Value;

use crate::sequence_number_value::option_bigint_to_u64;

#[napi(object)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NewEvent {
    #[napi(js_name = "event_type")]
    pub event_type: String,
    pub payload: Value,
}

impl NewEvent {
    pub fn into_interop(self) -> factstr_interop::InteropNewEvent {
        factstr_interop::InteropNewEvent {
            event_type: self.event_type,
            payload: self.payload,
        }
    }
}

#[napi(object)]
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct EventFilter {
    #[napi(js_name = "event_types")]
    pub event_types: Option<Vec<String>>,
    #[napi(js_name = "payload_predicates")]
    pub payload_predicates: Option<Vec<Value>>,
}

impl EventFilter {
    pub fn into_interop(self) -> factstr_interop::InteropEventFilter {
        factstr_interop::InteropEventFilter {
            event_types: self.event_types,
            payload_predicates: self.payload_predicates,
        }
    }
}

#[napi(object)]
#[derive(Clone, Debug, Default)]
pub struct EventQuery {
    pub filters: Option<Vec<EventFilter>>,
    #[napi(js_name = "min_sequence_number")]
    pub min_sequence_number: Option<BigInt>,
}

impl EventQuery {
    pub fn into_interop(self) -> napi::Result<factstr_interop::InteropEventQuery> {
        Ok(factstr_interop::InteropEventQuery {
            filters: self
                .filters
                .map(|filters| filters.into_iter().map(EventFilter::into_interop).collect()),
            min_sequence_number: option_bigint_to_u64(self.min_sequence_number)?,
        })
    }
}
