use factstr_conformance as store_conformance;
use factstr_memory::MemoryStore;

#[test]
fn query_returns_events_in_ascending_order() {
    store_conformance::query_returns_events_in_ascending_order(MemoryStore::new);
}

#[test]
fn query_records_include_occurred_at() {
    store_conformance::query_records_include_occurred_at(MemoryStore::new);
}

#[test]
fn query_with_min_sequence_number_only_returns_events_after_that_sequence() {
    store_conformance::query_with_min_sequence_number_only_returns_events_after_that_sequence(
        MemoryStore::new,
    );
}

#[test]
fn query_against_an_empty_store_returns_explicit_empty_result() {
    store_conformance::query_against_an_empty_store_returns_explicit_empty_result(MemoryStore::new);
}

#[test]
fn current_context_version_for_payload_filtered_queries_uses_the_full_matching_context() {
    store_conformance::current_context_version_for_payload_filtered_queries_uses_the_full_matching_context(
        MemoryStore::new,
    );
}

#[test]
fn min_sequence_number_does_not_narrow_current_context_version_for_payload_filtered_queries() {
    store_conformance::min_sequence_number_does_not_narrow_current_context_version_for_payload_filtered_queries(
        MemoryStore::new,
    );
}

#[test]
fn all_events_query_and_filtered_query_report_their_own_context_versions() {
    store_conformance::all_events_query_and_filtered_query_report_their_own_context_versions(
        MemoryStore::new,
    );
}
