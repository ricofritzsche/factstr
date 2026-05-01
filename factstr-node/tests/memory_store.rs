use factstr_node::{EventFilter, EventQuery, FactstrMemoryStore, NewEvent};
use napi::bindgen_prelude::BigInt;
use serde_json::json;

fn bigint_to_u64(value: &BigInt) -> u64 {
    let (sign_bit, unsigned_value, lossless) = value.get_u64();
    assert!(!sign_bit, "expected non-negative BigInt");
    assert!(lossless, "expected lossless BigInt");
    unsigned_value
}

fn option_bigint_to_u64(value: &Option<BigInt>) -> Option<u64> {
    value.as_ref().map(bigint_to_u64)
}

#[test]
fn append_returns_the_append_result_shape() {
    let store = FactstrMemoryStore::new();

    let append_result = store
        .append(vec![NewEvent {
            event_type: "account-opened".to_owned(),
            payload: json!({ "accountId": "a-1" }),
        }])
        .expect("append should succeed");

    assert_eq!(bigint_to_u64(&append_result.first_sequence_number), 1);
    assert_eq!(bigint_to_u64(&append_result.last_sequence_number), 1);
    assert_eq!(bigint_to_u64(&append_result.committed_count), 1);
}

#[test]
fn query_returns_the_query_result_shape() {
    let store = FactstrMemoryStore::new();
    store
        .append(vec![NewEvent {
            event_type: "money-deposited".to_owned(),
            payload: json!({ "accountId": "a-1", "amount": 25 }),
        }])
        .expect("append should succeed");

    let query_result = store
        .query(EventQuery {
            filters: Some(vec![EventFilter {
                event_types: Some(vec!["money-deposited".to_owned()]),
                payload_predicates: Some(vec![json!({ "accountId": "a-1" })]),
            }]),
            min_sequence_number: None,
        })
        .expect("query should succeed");

    assert_eq!(query_result.event_records.len(), 1);
    assert_eq!(
        option_bigint_to_u64(&query_result.last_returned_sequence_number),
        Some(1)
    );
    assert_eq!(
        option_bigint_to_u64(&query_result.current_context_version),
        Some(1)
    );
    assert_eq!(
        bigint_to_u64(&query_result.event_records[0].sequence_number),
        1
    );
    assert!(
        !query_result.event_records[0].occurred_at.is_empty(),
        "expected occurred_at to be present",
    );
    assert_eq!(query_result.event_records[0].event_type, "money-deposited");
}

#[test]
fn append_if_returns_an_explicit_conflict_shape() {
    let store = FactstrMemoryStore::new();
    store
        .append(vec![NewEvent {
            event_type: "account-opened".to_owned(),
            payload: json!({ "accountId": "a-1" }),
        }])
        .expect("append should succeed");

    let append_if_result = store
        .append_if(
            vec![NewEvent {
                event_type: "money-deposited".to_owned(),
                payload: json!({ "accountId": "a-1", "amount": 25 }),
            }],
            EventQuery {
                filters: Some(vec![EventFilter {
                    event_types: Some(vec!["account-opened".to_owned()]),
                    payload_predicates: None,
                }]),
                min_sequence_number: None,
            },
            Some(0u64.into()),
        )
        .expect("append_if should return an explicit conflict result");

    assert!(append_if_result.append_result.is_none());
    let conflict = append_if_result
        .conflict
        .expect("conflict shape should be present");
    assert_eq!(
        option_bigint_to_u64(&conflict.expected_context_version),
        Some(0)
    );
    assert_eq!(
        option_bigint_to_u64(&conflict.actual_context_version),
        Some(1)
    );
}
