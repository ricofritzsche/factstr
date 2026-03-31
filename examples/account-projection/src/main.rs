use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;

use factstore::{
    EventFilter, EventQuery, EventRecord, EventStore, NewEvent, SubscriptionHandlerError,
};
use factstore_memory::MemoryStore;
use serde_json::json;

#[derive(Debug, Default)]
struct AccountDirectoryProjection {
    display_name_by_account_id: BTreeMap<String, String>,
}

impl AccountDirectoryProjection {
    fn apply_committed_batch(&mut self, committed_batch: &[EventRecord]) {
        for event_record in committed_batch {
            let account_id = event_record
                .payload
                .get("accountId")
                .and_then(|value| value.as_str())
                .expect("example events should carry accountId")
                .to_owned();
            let display_name = event_record
                .payload
                .get("owner")
                .or_else(|| event_record.payload.get("name"))
                .and_then(|value| value.as_str())
                .expect("example events should carry owner or name")
                .to_owned();

            self.display_name_by_account_id
                .insert(account_id, display_name);
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let store = MemoryStore::new();
    let projection = Arc::new(Mutex::new(AccountDirectoryProjection::default()));
    let (batch_applied_sender, batch_applied_receiver) = mpsc::channel();

    let projection_subscription = store.subscribe_to(
        &EventQuery::all().with_filters([EventFilter::for_event_types([
            "account-opened",
            "account-renamed",
        ])]),
        Arc::new({
            let projection = Arc::clone(&projection);
            move |committed_batch| {
                projection
                    .lock()
                    .expect("projection lock should succeed")
                    .apply_committed_batch(&committed_batch);

                batch_applied_sender
                    .send(committed_batch.len())
                    .expect("example batch signal should succeed");

                Ok::<(), SubscriptionHandlerError>(())
            }
        }),
    )?;

    store.append(vec![
        NewEvent::new(
            "account-opened",
            json!({
                "accountId": "a1",
                "owner": "Rico"
            }),
        ),
        NewEvent::new(
            "account-credited",
            json!({
                "accountId": "a1",
                "amount": 100
            }),
        ),
        NewEvent::new(
            "account-renamed",
            json!({
                "accountId": "a1",
                "name": "FACTSTR"
            }),
        ),
    ])?;

    let matching_event_count = batch_applied_receiver.recv_timeout(Duration::from_secs(1))?;
    println!("projection received one committed filtered batch with {matching_event_count} events");
    println!("The account-credited fact did not reach this projection.");

    let projection_state = projection
        .lock()
        .expect("projection lock should succeed");
    println!("account directory projection: {projection_state:#?}");
    drop(projection_state);

    projection_subscription.unsubscribe();

    Ok(())
}
