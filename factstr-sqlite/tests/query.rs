mod support;

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::thread;

use factstr::{EventFilter, EventQuery, EventStore, NewEvent};
use factstr_conformance as store_conformance;
use factstr_sqlite::SqliteStore;
use serde_json::json;

use support::TemporaryDatabaseFile;

#[test]
fn query_returns_events_in_ascending_order() {
    support::run_store_test(store_conformance::query_returns_events_in_ascending_order);
}

#[test]
fn query_records_include_occurred_at() {
    support::run_store_test(store_conformance::query_records_include_occurred_at);
}

#[test]
fn query_with_min_sequence_number_only_returns_events_after_that_sequence() {
    support::run_store_test(
        store_conformance::query_with_min_sequence_number_only_returns_events_after_that_sequence,
    );
}

#[test]
fn query_against_an_empty_store_returns_explicit_empty_result() {
    support::run_store_test(
        store_conformance::query_against_an_empty_store_returns_explicit_empty_result,
    );
}

#[test]
fn current_context_version_for_payload_filtered_queries_uses_the_full_matching_context() {
    support::run_store_test(
        store_conformance::current_context_version_for_payload_filtered_queries_uses_the_full_matching_context,
    );
}

#[test]
fn min_sequence_number_does_not_narrow_current_context_version_for_payload_filtered_queries() {
    support::run_store_test(
        store_conformance::min_sequence_number_does_not_narrow_current_context_version_for_payload_filtered_queries,
    );
}

#[test]
fn all_events_query_and_filtered_query_report_their_own_context_versions() {
    support::run_store_test(
        store_conformance::all_events_query_and_filtered_query_report_their_own_context_versions,
    );
}

#[test]
fn query_result_stays_self_consistent_while_concurrent_appends_happen() {
    let database_file = TemporaryDatabaseFile::new("query-snapshot-consistency");
    let query_store = SqliteStore::open(database_file.path()).expect("sqlite store should open");
    let append_store = SqliteStore::open(database_file.path()).expect("sqlite store should open");

    query_store
        .append(vec![
            NewEvent::new("account-opened", json!({ "accountId": "a1" })),
            NewEvent::new("account-opened", json!({ "accountId": "a2" })),
        ])
        .expect("initial append should succeed");

    let should_stop = Arc::new(AtomicBool::new(false));
    let append_should_stop = Arc::clone(&should_stop);

    let append_thread = thread::spawn(move || {
        let mut next_account_number = 3_u64;

        while !append_should_stop.load(Ordering::Relaxed) {
            append_store
                .append(vec![NewEvent::new(
                    "account-opened",
                    json!({ "accountId": format!("a{next_account_number}") }),
                )])
                .expect("concurrent append should succeed");
            next_account_number += 1;
        }
    });

    let filtered_query = EventQuery::all()
        .with_filters([EventFilter::for_event_types(["account-opened"])])
        .with_min_sequence_number(1);

    for _ in 0..200 {
        let query_result = query_store
            .query(&filtered_query)
            .expect("query should succeed while appends happen");

        if let Some(last_returned_sequence_number) = query_result.last_returned_sequence_number {
            let current_context_version = query_result
                .current_context_version
                .expect("matching context should exist when rows were returned");

            assert!(
                last_returned_sequence_number <= current_context_version,
                "query result combined inconsistent snapshots: last returned sequence was {last_returned_sequence_number}, current context version was {current_context_version}"
            );
        }
    }

    should_stop.store(true, Ordering::Relaxed);
    append_thread
        .join()
        .expect("append thread should stop without panicking");
}
