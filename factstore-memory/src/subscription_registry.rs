use std::sync::mpsc::{self, Sender};

use factstore::{EventQuery, EventRecord, LiveSubscription};

use crate::query_match::matches_query;

#[derive(Debug, Default)]
pub(crate) struct SubscriptionRegistry {
    subscribers: Vec<Subscriber>,
}

#[derive(Debug)]
struct Subscriber {
    event_query: Option<EventQuery>,
    sender: Sender<Vec<EventRecord>>,
}

impl SubscriptionRegistry {
    pub(crate) fn subscribe_all(&mut self) -> LiveSubscription {
        self.subscribe_to(None)
    }

    pub(crate) fn subscribe_to(&mut self, event_query: Option<EventQuery>) -> LiveSubscription {
        let (sender, receiver) = mpsc::channel();
        self.subscribers.push(Subscriber {
            event_query,
            sender,
        });
        LiveSubscription::new(receiver)
    }

    pub(crate) fn notify(&mut self, committed_batch: &[EventRecord]) {
        if committed_batch.is_empty() {
            return;
        }

        self.subscribers.retain(|subscriber| {
            let delivered_batch = match &subscriber.event_query {
                None => committed_batch.to_vec(),
                Some(event_query) => committed_batch
                    .iter()
                    .filter(|event_record| matches_query(event_query, event_record))
                    .cloned()
                    .collect(),
            };

            if delivered_batch.is_empty() {
                return true;
            }

            subscriber.sender.send(delivered_batch).is_ok()
        });
    }
}
