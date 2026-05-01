use crate::{
    AppendIfResult, AppendResult, EventQuery, NewEvent, QueryResult,
    sequence_number_value::option_bigint_to_u64,
};
use factstr::{EventStore, EventStoreError};
use factstr_memory::MemoryStore;
use napi::Error;
use napi::bindgen_prelude::BigInt;
use napi::bindgen_prelude::Result;
use napi_derive::napi;

#[napi]
pub struct FactstrMemoryStore {
    memory_store: MemoryStore,
}

impl Default for FactstrMemoryStore {
    fn default() -> Self {
        Self::new()
    }
}

#[napi]
impl FactstrMemoryStore {
    #[napi(constructor)]
    pub fn new() -> Self {
        Self {
            memory_store: MemoryStore::new(),
        }
    }

    #[napi]
    pub fn append(&self, events: Vec<NewEvent>) -> Result<AppendResult> {
        let interop_events = events
            .into_iter()
            .map(NewEvent::into_interop)
            .collect::<Vec<_>>();
        let new_events = interop_events
            .into_iter()
            .map(Into::into)
            .collect::<Vec<_>>();
        let append_result = self
            .memory_store
            .append(new_events)
            .map_err(napi_error_from_event_store_error)?;

        Ok(factstr_interop::InteropAppendResult::from(append_result).into())
    }

    #[napi]
    pub fn query(&self, query: EventQuery) -> Result<QueryResult> {
        let interop_query = query.into_interop()?;
        let event_query = interop_query.into();
        let query_result = self
            .memory_store
            .query(&event_query)
            .map_err(napi_error_from_event_store_error)?;

        Ok(factstr_interop::InteropQueryResult::from(query_result).into())
    }

    #[napi(js_name = "appendIf")]
    pub fn append_if(
        &self,
        events: Vec<NewEvent>,
        query: EventQuery,
        expected_context_version: Option<BigInt>,
    ) -> Result<AppendIfResult> {
        let interop_events = events
            .into_iter()
            .map(NewEvent::into_interop)
            .collect::<Vec<_>>();
        let new_events = interop_events
            .into_iter()
            .map(Into::into)
            .collect::<Vec<_>>();
        let interop_query = query.into_interop()?;
        let event_query = interop_query.into();
        let expected_context_version = option_bigint_to_u64(expected_context_version)?;

        match self
            .memory_store
            .append_if(new_events, &event_query, expected_context_version)
        {
            Ok(append_result) => Ok(AppendIfResult {
                append_result: Some(
                    factstr_interop::InteropAppendResult::from(append_result).into(),
                ),
                conflict: None,
            }),
            Err(EventStoreError::ConditionalAppendConflict { expected, actual }) => {
                Ok(AppendIfResult {
                    append_result: None,
                    conflict: Some(
                        factstr_interop::InteropConditionalAppendConflict::new(expected, actual)
                            .into(),
                    ),
                })
            }
            Err(error) => Err(napi_error_from_event_store_error(error)),
        }
    }
}

fn napi_error_from_event_store_error(event_store_error: EventStoreError) -> Error {
    let interop_error = factstr_interop::InteropError::from(event_store_error);
    let code = match &interop_error {
        factstr_interop::InteropError::EmptyAppend => "EmptyAppend",
        factstr_interop::InteropError::ConditionalAppendConflict(_) => "ConditionalAppendConflict",
        factstr_interop::InteropError::NotImplemented { .. } => "NotImplemented",
        factstr_interop::InteropError::BackendFailure { .. } => "BackendFailure",
    };

    Error::from_reason(format!("{}: {}", code, interop_error))
}
