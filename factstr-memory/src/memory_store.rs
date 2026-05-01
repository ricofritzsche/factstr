    use std::cell::{Cell, RefCell};
use std::sync::{
    Arc, Mutex,
    mpsc::{self, Receiver, Sender},
};
use std::thread::{self, JoinHandle};

use factstr::{
    AppendResult, DurableStream, EventQuery, EventRecord, EventStore, EventStoreError, EventStream,
    HandleStream, NewEvent, QueryResult,
};
use time::OffsetDateTime;

use crate::query_match::matches_query;
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

#[derive(Clone, Debug)]
struct DurableStreamCursor {
    durable_stream_id: String,
    event_query: EventQuery,
    last_processed_sequence_number: u64,
}

enum DeliveryCommand {
    Deliver(Vec<PendingDelivery>),
    Shutdown,
}

pub struct MemoryStore {
    event_records: RefCell<Vec<EventRecord>>,
    committed_batches: RefCell<Vec<Vec<EventRecord>>>,
    next_sequence_number: Cell<u64>,
    durable_stream_cursors: Arc<Mutex<Vec<DurableStreamCursor>>>,
    subscription_registry: Arc<Mutex<SubscriptionRegistry>>,
    delivery_sender: Sender<DeliveryCommand>,
    delivery_thread: Mutex<Option<JoinHandle<()>>>,
}

impl Default for MemoryStore {
    fn default() -> Self {
        Self::new()
    }
}

impl MemoryStore {
    pub fn new() -> Self {
        let durable_stream_cursors = Arc::new(Mutex::new(Vec::new()));
        let subscription_registry = Arc::new(Mutex::new(SubscriptionRegistry::default()));
        let (delivery_sender, delivery_receiver) = mpsc::channel();
        let delivery_thread = thread::Builder::new()
            .name("factstr-memory-delivery".to_owned())
            .spawn({
                let durable_stream_cursors = Arc::clone(&durable_stream_cursors);
                let subscription_registry = Arc::clone(&subscription_registry);
                move || {
                    run_delivery_thread(
                        durable_stream_cursors,
                        subscription_registry,
                        delivery_receiver,
                    )
                }
            })
            .expect("memory delivery thread should start");

        Self {
            event_records: RefCell::new(Vec::new()),
            committed_batches: RefCell::new(Vec::new()),
            next_sequence_number: Cell::new(1),
            durable_stream_cursors,
            subscription_registry,
            delivery_sender,
            delivery_thread: Mutex::new(Some(delivery_thread)),
        }
    }

    fn current_context_version(&self, event_query: &EventQuery) -> Option<u64> {
        self.event_records
            .borrow()
            .iter()
            .filter(|event_record| matches_query(event_query, event_record))
            .map(|event_record| event_record.sequence_number)
            .next_back()
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
                occurred_at: OffsetDateTime::now_utc(),
                event_type: new_event.event_type,
                payload: new_event.payload,
            })
            .collect::<Vec<_>>();

        self.event_records
            .borrow_mut()
            .extend(committed_event_records.iter().cloned());
        self.committed_batches
            .borrow_mut()
            .push(committed_event_records.clone());
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
                "factstr-memory delivery dispatcher stopped after commit: {}",
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
        let last_processed_sequence_number = load_or_create_durable_stream_cursor(
            &self.durable_stream_cursors,
            &durable_stream_id,
            &normalized_event_query,
        )?;
        let replay_until_sequence_number = self
            .event_records
            .borrow()
            .last()
            .map(|event_record| event_record.sequence_number)
            .unwrap_or(0);

        let subscription_id = match self.subscription_registry.lock() {
            Ok(mut subscription_registry) => {
                if normalized_event_query.filters.is_none() {
                    subscription_registry
                        .subscribe_all_durable(
                            durable_stream_id.clone(),
                            replay_until_sequence_number,
                            handle.clone(),
                        )
                        .map_err(subscription_registry_backend_failure)?
                } else {
                    subscription_registry
                        .subscribe_to_durable(
                            durable_stream_id.clone(),
                            Some(normalized_event_query.clone()),
                            replay_until_sequence_number,
                            handle.clone(),
                        )
                        .map_err(subscription_registry_backend_failure)?
                }
            }
            Err(poisoned) => {
                let mut subscription_registry = poisoned.into_inner();
                if normalized_event_query.filters.is_none() {
                    subscription_registry
                        .subscribe_all_durable(
                            durable_stream_id.clone(),
                            replay_until_sequence_number,
                            handle.clone(),
                        )
                        .map_err(subscription_registry_backend_failure)?
                } else {
                    subscription_registry
                        .subscribe_to_durable(
                            durable_stream_id.clone(),
                            Some(normalized_event_query.clone()),
                            replay_until_sequence_number,
                            handle.clone(),
                        )
                        .map_err(subscription_registry_backend_failure)?
                }
            }
        };

        let replay_state = DurableReplayState {
            subscription_id,
            last_processed_sequence_number,
            replay_until_sequence_number,
        };
        let subscription = self.build_subscription_handle(replay_state.subscription_id);
        let replay_batches = load_replay_batches(
            &self.committed_batches.borrow(),
            &normalized_event_query,
            replay_state.last_processed_sequence_number,
            replay_state.replay_until_sequence_number,
        );

        for replay_batch in replay_batches {
            let pending_delivery = PendingDelivery {
                subscription_id: replay_state.subscription_id,
                durable_stream_id: Some(durable_stream_id.clone()),
                last_processed_sequence_number: replay_batch.last_processed_sequence_number,
                delivered_batch: replay_batch.delivered_batch,
                handle: handle.clone(),
            };

            match self.process_durable_delivery(pending_delivery) {
                Ok(true) => {}
                Ok(false) => {
                    self.cleanup_durable_stream(replay_state.subscription_id);
                    return Err(EventStoreError::BackendFailure {
                        message: format!(
                            "durable replay for stream {} did not complete successfully",
                            durable_stream_id
                        ),
                    });
                }
                Err(error) => {
                    self.cleanup_durable_stream(replay_state.subscription_id);
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
                    update_durable_stream_cursor(
                        &self.durable_stream_cursors,
                        &durable_stream_id,
                        last_processed_sequence_number,
                    )?;
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

    fn cleanup_durable_stream(&self, subscription_id: u64) {
        match self.subscription_registry.lock() {
            Ok(mut subscription_registry) => subscription_registry.unsubscribe(subscription_id),
            Err(poisoned) => poisoned.into_inner().unsubscribe(subscription_id),
        }
    }
}

impl Drop for MemoryStore {
    fn drop(&mut self) {
        let _ = self.delivery_sender.send(DeliveryCommand::Shutdown);

        if let Ok(mut delivery_thread) = self.delivery_thread.lock() {
            if let Some(delivery_thread) = delivery_thread.take() {
                let _ = delivery_thread.join();
            }
        }
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
        let actual_context_version = self.current_context_version(context_query);

        if actual_context_version != expected_context_version {
            return Err(EventStoreError::ConditionalAppendConflict {
                expected: expected_context_version,
                actual: actual_context_version,
            });
        }

        let committed_append = self.append_records(new_events)?;
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

fn run_delivery_thread(
    durable_stream_cursors: Arc<Mutex<Vec<DurableStreamCursor>>>,
    subscription_registry: Arc<Mutex<SubscriptionRegistry>>,
    delivery_receiver: Receiver<DeliveryCommand>,
) {
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
                                if let Err(error) = update_durable_stream_cursor(
                                    &durable_stream_cursors,
                                    &durable_stream_id,
                                    last_processed_sequence_number,
                                ) {
                                    eprintln!(
                                        "factstr-memory durable cursor update failed after delivery for stream {}: {}",
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

fn load_or_create_durable_stream_cursor(
    durable_stream_cursors: &Arc<Mutex<Vec<DurableStreamCursor>>>,
    durable_stream_id: &str,
    event_query: &EventQuery,
) -> Result<u64, EventStoreError> {
    let mut durable_stream_cursors = match durable_stream_cursors.lock() {
        Ok(durable_stream_cursors) => durable_stream_cursors,
        Err(poisoned) => poisoned.into_inner(),
    };

    if let Some(existing_cursor) = durable_stream_cursors
        .iter()
        .find(|cursor| cursor.durable_stream_id == durable_stream_id)
    {
        if existing_cursor.event_query != *event_query {
            return Err(EventStoreError::BackendFailure {
                message: format!(
                    "durable stream {durable_stream_id} was resumed with a different query"
                ),
            });
        }

        return Ok(existing_cursor.last_processed_sequence_number);
    }

    durable_stream_cursors.push(DurableStreamCursor {
        durable_stream_id: durable_stream_id.to_owned(),
        event_query: event_query.clone(),
        last_processed_sequence_number: 0,
    });

    Ok(0)
}

fn update_durable_stream_cursor(
    durable_stream_cursors: &Arc<Mutex<Vec<DurableStreamCursor>>>,
    durable_stream_id: &str,
    last_processed_sequence_number: u64,
) -> Result<(), EventStoreError> {
    let mut durable_stream_cursors = match durable_stream_cursors.lock() {
        Ok(durable_stream_cursors) => durable_stream_cursors,
        Err(poisoned) => poisoned.into_inner(),
    };

    let Some(existing_cursor) = durable_stream_cursors
        .iter_mut()
        .find(|cursor| cursor.durable_stream_id == durable_stream_id)
    else {
        return Err(EventStoreError::BackendFailure {
            message: format!("durable stream {durable_stream_id} is missing its cursor state"),
        });
    };

    existing_cursor.last_processed_sequence_number = last_processed_sequence_number;
    Ok(())
}

fn load_replay_batches(
    committed_batches: &[Vec<EventRecord>],
    event_query: &EventQuery,
    last_processed_sequence_number: u64,
    replay_until_sequence_number: u64,
) -> Vec<ReplayBatch> {
    if replay_until_sequence_number <= last_processed_sequence_number {
        return Vec::new();
    }

    committed_batches
        .iter()
        .filter(|committed_batch| {
            let first_sequence_number = committed_batch
                .first()
                .expect("committed batch should not be empty")
                .sequence_number;
            let last_sequence_number = committed_batch
                .last()
                .expect("committed batch should not be empty")
                .sequence_number;

            last_sequence_number > last_processed_sequence_number
                && first_sequence_number <= replay_until_sequence_number
        })
        .map(|committed_batch| ReplayBatch {
            last_processed_sequence_number: committed_batch
                .last()
                .expect("committed batch should not be empty")
                .sequence_number,
            delivered_batch: committed_batch
                .iter()
                .filter(|event_record| matches_query(event_query, event_record))
                .cloned()
                .collect(),
        })
        .collect()
}

fn normalized_durable_event_query(event_query: &EventQuery) -> EventQuery {
    EventQuery {
        filters: event_query.filters.clone(),
        min_sequence_number: None,
    }
}

fn subscription_registry_backend_failure(message: String) -> EventStoreError {
    EventStoreError::BackendFailure { message }
}
