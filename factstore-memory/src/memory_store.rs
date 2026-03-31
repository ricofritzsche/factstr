use std::cell::{Cell, RefCell};

use factstore::{
    AppendResult, EventQuery, EventRecord, EventStore, EventStoreError, LiveSubscription, NewEvent,
    QueryResult,
};

use crate::query_match::matches_query;
use crate::subscription_registry::SubscriptionRegistry;

#[derive(Clone, Debug)]
struct CommittedAppend {
    append_result: AppendResult,
    event_records: Vec<EventRecord>,
}

#[derive(Debug, Default)]
pub struct MemoryStore {
    event_records: RefCell<Vec<EventRecord>>,
    next_sequence_number: Cell<u64>,
    subscription_registry: RefCell<SubscriptionRegistry>,
}

impl MemoryStore {
    pub fn new() -> Self {
        Self {
            event_records: RefCell::new(Vec::new()),
            next_sequence_number: Cell::new(1),
            subscription_registry: RefCell::new(SubscriptionRegistry::default()),
        }
    }

    fn current_context_version(&self, event_query: &EventQuery) -> Option<u64> {
        self.event_records
            .borrow()
            .iter()
            .filter(|event_record| matches_query(event_query, event_record))
            .map(|event_record| event_record.sequence_number)
            .last()
    }

    fn append_records(
        &self,
        new_events: Vec<NewEvent>,
    ) -> Result<CommittedAppend, EventStoreError> {
        if new_events.is_empty() {
            return Err(EventStoreError::EmptyAppend);
        }

        let committed_count = new_events.len() as u64;
        let first_sequence_number = self.next_sequence_number.get();
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

        self.event_records
            .borrow_mut()
            .extend(committed_event_records.iter().cloned());

        self.next_sequence_number.set(last_sequence_number + 1);

        Ok(CommittedAppend {
            append_result: AppendResult {
                first_sequence_number,
                last_sequence_number,
                committed_count,
            },
            event_records: committed_event_records,
        })
    }
}

impl EventStore for MemoryStore {
    fn query(&self, event_query: &EventQuery) -> Result<QueryResult, EventStoreError> {
        let current_context_version = self.current_context_version(event_query);
        let event_records: Vec<EventRecord> = self
            .event_records
            .borrow()
            .iter()
            .filter(|event_record| matches_query(event_query, event_record))
            .filter(|event_record| {
                event_query
                    .min_sequence_number
                    .is_none_or(|min_sequence_number| {
                        event_record.sequence_number > min_sequence_number
                    })
            })
            .cloned()
            .collect();

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
        let committed_append = self.append_records(new_events)?;
        self.subscription_registry
            .borrow_mut()
            .notify(&committed_append.event_records);
        Ok(committed_append.append_result)
    }

    fn append_if(
        &self,
        new_events: Vec<NewEvent>,
        context_query: &EventQuery,
        expected_context_version: Option<u64>,
    ) -> Result<AppendResult, EventStoreError> {
        let actual_context_version = self.current_context_version(context_query);

        if actual_context_version != expected_context_version {
            return Err(EventStoreError::ConditionalAppendConflict {
                expected: expected_context_version,
                actual: actual_context_version,
            });
        }

        let committed_append = self.append_records(new_events)?;
        self.subscription_registry
            .borrow_mut()
            .notify(&committed_append.event_records);
        Ok(committed_append.append_result)
    }

    fn subscribe_all(&self) -> Result<LiveSubscription, EventStoreError> {
        Ok(self.subscription_registry.borrow_mut().subscribe_all())
    }

    fn subscribe_to(&self, event_query: &EventQuery) -> Result<LiveSubscription, EventStoreError> {
        Ok(self
            .subscription_registry
            .borrow_mut()
            .subscribe_to(Some(event_query.clone())))
    }
}
