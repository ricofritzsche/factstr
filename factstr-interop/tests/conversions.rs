use factstr::{
    AppendResult, EventFilter, EventQuery, EventRecord, EventStoreError, NewEvent, QueryResult,
};
use factstr_interop::{
    InteropAppendResult, InteropConditionalAppendConflict, InteropError, InteropEventFilter,
    InteropEventQuery, InteropEventRecord, InteropNewEvent, InteropQueryResult,
};
use serde_json::json;
use time::OffsetDateTime;

#[test]
fn interop_query_result_preserves_returned_sequence_and_context_version_distinction() {
    let interop_query_result = InteropQueryResult {
        event_records: vec![InteropEventRecord {
            sequence_number: 7,
            occurred_at: "2026-05-01T08:00:00Z".to_owned(),
            event_type: "account-opened".to_owned(),
            payload: json!({ "accountId": "a-1" }),
        }],
        last_returned_sequence_number: Some(7),
        current_context_version: Some(11),
    };

    let query_result: QueryResult = interop_query_result.into();

    assert_eq!(query_result.last_returned_sequence_number, Some(7));
    assert_eq!(query_result.current_context_version, Some(11));
}

#[test]
fn interop_event_query_round_trip_preserves_filter_and_cursor_shape() {
    let interop_event_query = InteropEventQuery {
        filters: Some(vec![
            InteropEventFilter {
                event_types: Some(vec![
                    "account-opened".to_owned(),
                    "money-deposited".to_owned(),
                ]),
                payload_predicates: Some(vec![json!({ "accountId": "a-1" })]),
            },
            InteropEventFilter {
                event_types: None,
                payload_predicates: Some(vec![]),
            },
        ]),
        min_sequence_number: Some(19),
    };

    let event_query: EventQuery = interop_event_query.clone().into();
    let round_trip = InteropEventQuery::from(event_query);

    assert_eq!(round_trip, interop_event_query);
}

#[test]
fn interop_error_preserves_conditional_append_conflict_shape() {
    let event_store_error = EventStoreError::ConditionalAppendConflict {
        expected: Some(4),
        actual: Some(6),
    };

    let interop_error = InteropError::from(event_store_error);

    assert_eq!(
        interop_error,
        InteropError::ConditionalAppendConflict(InteropConditionalAppendConflict {
            expected_context_version: Some(4),
            actual_context_version: Some(6),
        })
    );
}

#[test]
fn interop_new_event_round_trip_preserves_event_input_shape() {
    let interop_new_event = InteropNewEvent::new(
        "money-deposited",
        json!({ "accountId": "a-1", "amount": 25 }),
    );

    let new_event = NewEvent::from(interop_new_event.clone());
    let round_trip = InteropNewEvent::from(new_event);

    assert_eq!(round_trip, interop_new_event);
}

#[test]
fn interop_append_result_round_trip_preserves_committed_batch_shape() {
    let append_result = AppendResult {
        first_sequence_number: 21,
        last_sequence_number: 24,
        committed_count: 4,
    };

    let interop_append_result = InteropAppendResult::from(append_result.clone());
    let round_trip = AppendResult::from(interop_append_result);

    assert_eq!(round_trip, append_result);
}

#[test]
fn interop_query_result_round_trip_preserves_event_records() {
    let query_result = QueryResult {
        event_records: vec![EventRecord {
            sequence_number: 3,
            occurred_at: OffsetDateTime::from_unix_timestamp(1)
                .expect("unix timestamp should convert"),
            event_type: "account-opened".to_owned(),
            payload: json!({ "accountId": "a-1", "owner": "Rico" }),
        }],
        last_returned_sequence_number: Some(3),
        current_context_version: Some(3),
    };

    let interop_query_result = InteropQueryResult::from(query_result.clone());
    let round_trip = QueryResult::from(interop_query_result);

    assert_eq!(round_trip, query_result);
}

#[test]
fn interop_event_filter_round_trip_preserves_explicit_empty_match_sets() {
    let event_filter = EventFilter {
        event_types: Some(vec![]),
        payload_predicates: Some(vec![]),
    };

    let interop_event_filter = InteropEventFilter::from(event_filter.clone());
    let round_trip = EventFilter::from(interop_event_filter);

    assert_eq!(round_trip, event_filter);
}
