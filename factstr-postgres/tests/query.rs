mod support;

use factstr_conformance as store_conformance;

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
