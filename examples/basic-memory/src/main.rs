use factstore::{EventQuery, EventStore, NewEvent};
use factstore_memory::MemoryStore;
use serde_json::json;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let store = MemoryStore::new();

    let append_result = store.append(vec![
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

    println!("append_result: {append_result:#?}");

    let query_result = store.query(&EventQuery::all())?;

    println!("returned events:");
    for event_record in &query_result.event_records {
        println!("  {event_record:#?}");
    }

    println!(
        "last_returned_sequence_number: {:?}",
        query_result.last_returned_sequence_number
    );
    println!(
        "current_context_version: {:?}",
        query_result.current_context_version
    );

    Ok(())
}
