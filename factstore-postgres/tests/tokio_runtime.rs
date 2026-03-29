mod support;

use factstore::{EventQuery, EventStore, NewEvent};
use serde_json::json;

#[test]
fn store_operations_are_safe_inside_a_running_tokio_runtime() {
    let store = support::create_store();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime should build");

    runtime.block_on(async {
        let append_result = store
            .append(vec![NewEvent::new(
                "account-opened",
                json!({ "accountId": "a1" }),
            )])
            .expect("append should succeed inside a running tokio runtime");

        assert_eq!(append_result.first_sequence_number, 1);
        assert_eq!(append_result.last_sequence_number, 1);

        let query_result = store
            .query(&EventQuery::all())
            .expect("query should succeed inside a running tokio runtime");

        assert_eq!(query_result.event_records.len(), 1);
        assert_eq!(query_result.event_records[0].sequence_number, 1);
        assert_eq!(query_result.current_context_version, Some(1));
    });
}
