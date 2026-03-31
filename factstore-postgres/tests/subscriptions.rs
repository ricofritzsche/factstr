mod support;

use factstore_conformance as store_conformance;

#[test]
fn subscribe_receives_a_future_committed_append_batch() {
    support::run_store_test(store_conformance::subscribe_receives_a_future_committed_append_batch);
}

#[test]
fn subscribe_does_not_replay_historical_events() {
    support::run_store_test(store_conformance::subscribe_does_not_replay_historical_events);
}

#[test]
fn two_subscribers_receive_the_same_committed_batches() {
    support::run_store_test(store_conformance::two_subscribers_receive_the_same_committed_batches);
}

#[test]
fn subscription_batches_arrive_in_commit_order() {
    support::run_store_test(store_conformance::subscription_batches_arrive_in_commit_order);
}

#[test]
fn append_if_conflict_emits_no_subscription_batch() {
    support::run_store_test(store_conformance::append_if_conflict_emits_no_subscription_batch);
}

#[test]
fn dropping_one_subscription_does_not_break_append_for_others() {
    support::run_store_test(
        store_conformance::dropping_one_subscription_does_not_break_append_for_others,
    );
}

#[test]
fn subscription_delivery_preserves_the_committed_batch_shape() {
    support::run_store_test(
        store_conformance::subscription_delivery_preserves_the_committed_batch_shape,
    );
}

#[test]
fn filtered_subscription_with_event_type_receives_only_matching_future_events() {
    support::run_store_test(
        store_conformance::filtered_subscription_with_event_type_receives_only_matching_future_events,
    );
}

#[test]
fn filtered_subscription_with_payload_predicate_receives_only_matching_future_events() {
    support::run_store_test(
        store_conformance::filtered_subscription_with_payload_predicate_receives_only_matching_future_events,
    );
}

#[test]
fn filtered_subscription_non_matching_commit_produces_no_delivery() {
    support::run_store_test(
        store_conformance::filtered_subscription_non_matching_commit_produces_no_delivery,
    );
}

#[test]
fn filtered_subscription_mixed_committed_batch_yields_one_filtered_batch() {
    support::run_store_test(
        store_conformance::filtered_subscription_mixed_committed_batch_yields_one_filtered_batch,
    );
}

#[test]
fn filtered_subscription_preserves_event_order_inside_delivered_batch() {
    support::run_store_test(
        store_conformance::filtered_subscription_preserves_event_order_inside_delivered_batch,
    );
}

#[test]
fn append_if_conflict_emits_no_filtered_subscription_batch() {
    support::run_store_test(
        store_conformance::append_if_conflict_emits_no_filtered_subscription_batch,
    );
}

#[test]
fn differently_filtered_subscribers_observe_the_same_commit_differently() {
    support::run_store_test(
        store_conformance::differently_filtered_subscribers_observe_the_same_commit_differently,
    );
}
