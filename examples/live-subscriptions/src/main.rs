use factstore::{EventStore, NewEvent};
use factstore_memory::MemoryStore;
use serde_json::json;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let store = MemoryStore::new();
    let subscription = store.subscribe()?;

    store.append(vec![
        NewEvent::new(
            "account-opened",
            json!({
                "accountId": "a1",
                "owner": "Rico"
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

    let committed_batch = subscription.recv()?;

    println!("received committed batch with {} events", committed_batch.len());
    for event_record in &committed_batch {
        println!("  {event_record:#?}");
    }

    Ok(())
}
