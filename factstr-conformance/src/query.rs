use factstr::{EventFilter, EventQuery, EventStore};
use serde_json::json;

use crate::support::new_event;

pub fn query_returns_events_in_ascending_order<S, F>(create_store: F)
where
    S: EventStore,
    F: Fn() -> S,
{
    let store = create_store();

    store
        .append(vec![
            new_event("account-opened", json!({ "accountId": "a1" })),
            new_event("account-credited", json!({ "accountId": "a1" })),
            new_event("account-debited", json!({ "accountId": "a1" })),
        ])
        .expect("append should succeed");

    let query_result = store
        .query(&EventQuery::all())
        .expect("query should succeed");

    assert_eq!(query_result.event_records.len(), 3);
    assert_eq!(query_result.event_records[0].sequence_number, 1);
    assert_eq!(query_result.event_records[1].sequence_number, 2);
    assert_eq!(query_result.event_records[2].sequence_number, 3);
    assert_eq!(query_result.last_returned_sequence_number, Some(3));
    assert_eq!(query_result.current_context_version, Some(3));
}

pub fn query_records_include_occurred_at<S, F>(create_store: F)
where
    S: EventStore,
    F: Fn() -> S,
{
    let store = create_store();

    store
        .append(vec![new_event(
            "account-opened",
            json!({ "accountId": "a1" }),
        )])
        .expect("append should succeed");

    let query_result = store
        .query(&EventQuery::all())
        .expect("query should succeed");

    assert_eq!(query_result.event_records.len(), 1);
    assert_ne!(
        query_result.event_records[0]
            .occurred_at
            .unix_timestamp_nanos(),
        0,
        "event record should carry a real occurred_at timestamp",
    );
}

pub fn query_with_min_sequence_number_only_returns_events_after_that_sequence<S, F>(create_store: F)
where
    S: EventStore,
    F: Fn() -> S,
{
    let store = create_store();

    store
        .append(vec![
            new_event("account-opened", json!({ "accountId": "a1" })),
            new_event("account-credited", json!({ "accountId": "a1" })),
            new_event("account-debited", json!({ "accountId": "a1" })),
        ])
        .expect("append should succeed");

    let query_result = store
        .query(&EventQuery::all().with_min_sequence_number(2))
        .expect("query should succeed");

    assert_eq!(query_result.event_records.len(), 1);
    assert_eq!(query_result.event_records[0].sequence_number, 3);
    assert_eq!(query_result.last_returned_sequence_number, Some(3));
    assert_eq!(query_result.current_context_version, Some(3));
}

pub fn query_against_an_empty_store_returns_explicit_empty_result<S, F>(create_store: F)
where
    S: EventStore,
    F: Fn() -> S,
{
    let store = create_store();

    let query_result = store
        .query(&EventQuery::all())
        .expect("query should succeed");

    assert!(query_result.event_records.is_empty());
    assert_eq!(query_result.last_returned_sequence_number, None);
    assert_eq!(query_result.current_context_version, None);
}

pub fn current_context_version_for_payload_filtered_queries_uses_the_full_matching_context<S, F>(
    create_store: F,
) where
    S: EventStore,
    F: Fn() -> S,
{
    let store = create_store();

    store
        .append(vec![
            new_event("account-opened", json!({ "accountId": "a1" })),
            new_event("account-opened", json!({ "accountId": "a2" })),
            new_event(
                "account-opened",
                json!({ "accountId": "a1", "name": "Rico" }),
            ),
        ])
        .expect("append should succeed");

    let query_result = store
        .query(&EventQuery::all().with_filters([
            EventFilter::default().with_payload_predicates([json!({ "accountId": "a1" })]),
        ]))
        .expect("query should succeed");

    assert_eq!(query_result.event_records.len(), 2);
    assert_eq!(query_result.last_returned_sequence_number, Some(3));
    assert_eq!(query_result.current_context_version, Some(3));
}

pub fn min_sequence_number_does_not_narrow_current_context_version_for_payload_filtered_queries<
    S,
    F,
>(
    create_store: F,
) where
    S: EventStore,
    F: Fn() -> S,
{
    let store = create_store();

    store
        .append(vec![
            new_event("account-opened", json!({ "accountId": "a1" })),
            new_event("account-opened", json!({ "accountId": "a2" })),
            new_event("account-opened", json!({ "accountId": "a1" })),
        ])
        .expect("append should succeed");

    let query_result = store
        .query(
            &EventQuery::all()
                .with_filters([
                    EventFilter::default().with_payload_predicates([json!({ "accountId": "a1" })])
                ])
                .with_min_sequence_number(3),
        )
        .expect("query should succeed");

    assert!(query_result.event_records.is_empty());
    assert_eq!(query_result.last_returned_sequence_number, None);
    assert_eq!(query_result.current_context_version, Some(3));
}

pub fn all_events_query_and_filtered_query_report_their_own_context_versions<S, F>(create_store: F)
where
    S: EventStore,
    F: Fn() -> S,
{
    let store = create_store();

    store
        .append(vec![
            new_event("account-opened", json!({ "accountId": "a1" })),
            new_event("account-credited", json!({ "accountId": "a2" })),
            new_event("account-opened", json!({ "accountId": "a1" })),
            new_event("account-debited", json!({ "accountId": "a3" })),
        ])
        .expect("append should succeed");

    let all_events_query_result = store
        .query(&EventQuery::all().with_min_sequence_number(3))
        .expect("query should succeed");
    let filtered_query_result = store
        .query(&EventQuery::for_event_types(["account-opened"]).with_min_sequence_number(3))
        .expect("query should succeed");

    assert_eq!(all_events_query_result.event_records.len(), 1);
    assert_eq!(
        all_events_query_result.last_returned_sequence_number,
        Some(4)
    );
    assert_eq!(all_events_query_result.current_context_version, Some(4));

    assert!(filtered_query_result.event_records.is_empty());
    assert_eq!(filtered_query_result.last_returned_sequence_number, None);
    assert_eq!(filtered_query_result.current_context_version, Some(3));
}
