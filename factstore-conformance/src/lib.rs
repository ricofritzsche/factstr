use factstore::{
    EventFilter, EventQuery, EventStore, EventStoreError, NewEvent, TryLiveSubscriptionRecvError,
};
use serde_json::{Value, json};

fn new_event(event_type: &str, payload: Value) -> NewEvent {
    NewEvent::new(event_type, payload)
}

pub fn append_assigns_consecutive_global_sequence_numbers<S, F>(create_store: F)
where
    S: EventStore,
    F: Fn() -> S,
{
    let store = create_store();

    let append_result = store
        .append(vec![
            new_event("account-opened", json!({ "accountId": "a1" })),
            new_event("account-credited", json!({ "accountId": "a1" })),
        ])
        .expect("append should succeed");

    assert_eq!(append_result.first_sequence_number, 1);
    assert_eq!(append_result.last_sequence_number, 2);
    assert_eq!(append_result.committed_count, 2);

    let second_append_result = store
        .append(vec![new_event(
            "account-closed",
            json!({ "accountId": "a1" }),
        )])
        .expect("second append should succeed");

    assert_eq!(second_append_result.first_sequence_number, 3);
    assert_eq!(second_append_result.last_sequence_number, 3);
    assert_eq!(second_append_result.committed_count, 1);
}

pub fn empty_append_input_returns_typed_error<S, F>(create_store: F)
where
    S: EventStore,
    F: Fn() -> S,
{
    let store = create_store();

    let error = store
        .append(Vec::new())
        .expect_err("empty append should fail");

    assert_eq!(error, EventStoreError::EmptyAppend);
}

pub fn subscribe_receives_a_future_committed_append_batch<S, F>(create_store: F)
where
    S: EventStore,
    F: Fn() -> S,
{
    let store = create_store();
    let subscription = store.subscribe_all().expect("subscribe_all should succeed");

    store
        .append(vec![
            new_event("account-opened", json!({ "accountId": "a1" })),
            new_event("account-credited", json!({ "accountId": "a1" })),
        ])
        .expect("append should succeed");

    let committed_batch = subscription
        .next_batch()
        .expect("subscription should receive a batch");
    assert_eq!(committed_batch.len(), 2);
    assert_eq!(committed_batch[0].sequence_number, 1);
    assert_eq!(committed_batch[1].sequence_number, 2);
}

pub fn subscribe_does_not_replay_historical_events<S, F>(create_store: F)
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

    let subscription = store.subscribe_all().expect("subscribe_all should succeed");

    assert_eq!(
        subscription.try_next_batch(),
        Err(TryLiveSubscriptionRecvError::Empty)
    );
}

pub fn two_subscribers_receive_the_same_committed_batches<S, F>(create_store: F)
where
    S: EventStore,
    F: Fn() -> S,
{
    let store = create_store();
    let first_subscription = store
        .subscribe_all()
        .expect("first subscribe_all should succeed");
    let second_subscription = store
        .subscribe_all()
        .expect("second subscribe_all should succeed");

    store
        .append(vec![new_event(
            "account-opened",
            json!({ "accountId": "a1" }),
        )])
        .expect("append should succeed");

    let first_batch = first_subscription
        .next_batch()
        .expect("first subscription should receive a batch");
    let second_batch = second_subscription
        .next_batch()
        .expect("second subscription should receive a batch");

    assert_eq!(first_batch, second_batch);
    assert_eq!(first_batch.len(), 1);
    assert_eq!(first_batch[0].sequence_number, 1);
}

pub fn subscription_batches_arrive_in_commit_order<S, F>(create_store: F)
where
    S: EventStore,
    F: Fn() -> S,
{
    let store = create_store();
    let subscription = store.subscribe_all().expect("subscribe_all should succeed");

    store
        .append(vec![new_event(
            "account-opened",
            json!({ "accountId": "a1" }),
        )])
        .expect("first append should succeed");
    store
        .append(vec![new_event(
            "account-credited",
            json!({ "accountId": "a1" }),
        )])
        .expect("second append should succeed");

    let first_batch = subscription
        .next_batch()
        .expect("first batch should arrive");
    let second_batch = subscription
        .next_batch()
        .expect("second batch should arrive");

    assert_eq!(first_batch[0].sequence_number, 1);
    assert_eq!(second_batch[0].sequence_number, 2);
}

pub fn append_if_conflict_emits_no_subscription_batch<S, F>(create_store: F)
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

    let subscription = store.subscribe_all().expect("subscribe_all should succeed");
    let context_query = EventQuery::all().with_filters([
        EventFilter::default().with_payload_predicates([json!({ "accountId": "a1" })])
    ]);

    let error = store
        .append_if(
            vec![new_event("account-credited", json!({ "accountId": "a1" }))],
            &context_query,
            None,
        )
        .expect_err("conditional append should fail");

    assert_eq!(
        error,
        EventStoreError::ConditionalAppendConflict {
            expected: None,
            actual: Some(1),
        }
    );
    assert_eq!(
        subscription.try_next_batch(),
        Err(TryLiveSubscriptionRecvError::Empty)
    );
}

pub fn dropping_one_subscription_does_not_break_append_for_others<S, F>(create_store: F)
where
    S: EventStore,
    F: Fn() -> S,
{
    let store = create_store();
    let dropped_subscription = store.subscribe_all().expect("subscribe_all should succeed");
    let active_subscription = store.subscribe_all().expect("subscribe_all should succeed");

    drop(dropped_subscription);

    let append_result = store
        .append(vec![new_event(
            "account-opened",
            json!({ "accountId": "a1" }),
        )])
        .expect("append should still succeed");

    assert_eq!(append_result.first_sequence_number, 1);
    assert_eq!(append_result.last_sequence_number, 1);

    let committed_batch = active_subscription
        .next_batch()
        .expect("active subscription should still receive a batch");
    assert_eq!(committed_batch.len(), 1);
    assert_eq!(committed_batch[0].sequence_number, 1);
}

pub fn subscription_delivery_preserves_the_committed_batch_shape<S, F>(create_store: F)
where
    S: EventStore,
    F: Fn() -> S,
{
    let store = create_store();
    let subscription = store.subscribe_all().expect("subscribe_all should succeed");

    store
        .append(vec![
            new_event("account-opened", json!({ "accountId": "a1" })),
            new_event("account-credited", json!({ "accountId": "a1" })),
        ])
        .expect("append should succeed");

    let committed_batch = subscription
        .next_batch()
        .expect("subscription should receive a batch");

    assert_eq!(committed_batch.len(), 2);
    assert_eq!(committed_batch[0].sequence_number, 1);
    assert_eq!(committed_batch[1].sequence_number, 2);
}

pub fn filtered_subscription_with_event_type_receives_only_matching_future_events<S, F>(
    create_store: F,
) where
    S: EventStore,
    F: Fn() -> S,
{
    let store = create_store();
    let subscription = store
        .subscribe_to(&EventQuery::for_event_types(["account-opened"]))
        .expect("subscribe_to should succeed");

    store
        .append(vec![
            new_event("account-opened", json!({ "accountId": "a1" })),
            new_event("account-credited", json!({ "accountId": "a1" })),
            new_event("account-opened", json!({ "accountId": "a2" })),
        ])
        .expect("append should succeed");

    let committed_batch = subscription
        .next_batch()
        .expect("filtered subscription should receive a batch");

    assert_eq!(committed_batch.len(), 2);
    assert_eq!(committed_batch[0].sequence_number, 1);
    assert_eq!(committed_batch[1].sequence_number, 3);
    assert!(
        committed_batch
            .iter()
            .all(|event_record| event_record.event_type == "account-opened")
    );
}

pub fn filtered_subscription_with_payload_predicate_receives_only_matching_future_events<S, F>(
    create_store: F,
) where
    S: EventStore,
    F: Fn() -> S,
{
    let store = create_store();
    let subscription = store
        .subscribe_to(&EventQuery::all().with_filters([
            EventFilter::default().with_payload_predicates([json!({ "accountId": "a1" })]),
        ]))
        .expect("subscribe_to should succeed");

    store
        .append(vec![
            new_event("account-opened", json!({ "accountId": "a1" })),
            new_event("account-opened", json!({ "accountId": "a2" })),
            new_event("account-renamed", json!({ "accountId": "a1" })),
        ])
        .expect("append should succeed");

    let committed_batch = subscription
        .next_batch()
        .expect("filtered subscription should receive a batch");

    assert_eq!(committed_batch.len(), 2);
    assert_eq!(committed_batch[0].sequence_number, 1);
    assert_eq!(committed_batch[1].sequence_number, 3);
}

pub fn filtered_subscription_non_matching_commit_produces_no_delivery<S, F>(create_store: F)
where
    S: EventStore,
    F: Fn() -> S,
{
    let store = create_store();
    let subscription = store
        .subscribe_to(&EventQuery::for_event_types(["account-opened"]))
        .expect("subscribe_to should succeed");

    store
        .append(vec![new_event(
            "account-credited",
            json!({ "accountId": "a1" }),
        )])
        .expect("append should succeed");

    assert_eq!(
        subscription.try_next_batch(),
        Err(TryLiveSubscriptionRecvError::Empty)
    );
}

pub fn filtered_subscription_mixed_committed_batch_yields_one_filtered_batch<S, F>(create_store: F)
where
    S: EventStore,
    F: Fn() -> S,
{
    let store = create_store();
    let subscription = store
        .subscribe_to(&EventQuery::for_event_types(["account-opened"]))
        .expect("subscribe_to should succeed");

    store
        .append(vec![
            new_event("account-opened", json!({ "accountId": "a1" })),
            new_event("account-credited", json!({ "accountId": "a1" })),
            new_event("account-opened", json!({ "accountId": "a2" })),
        ])
        .expect("append should succeed");

    let committed_batch = subscription
        .next_batch()
        .expect("filtered subscription should receive a batch");

    assert_eq!(committed_batch.len(), 2);
    assert_eq!(committed_batch[0].sequence_number, 1);
    assert_eq!(committed_batch[1].sequence_number, 3);
}

pub fn filtered_subscription_preserves_event_order_inside_delivered_batch<S, F>(create_store: F)
where
    S: EventStore,
    F: Fn() -> S,
{
    let store = create_store();
    let subscription = store
        .subscribe_to(&EventQuery::all().with_filters([
            EventFilter::default().with_payload_predicates([json!({ "accountId": "a1" })]),
        ]))
        .expect("subscribe_to should succeed");

    store
        .append(vec![
            new_event("account-opened", json!({ "accountId": "a1" })),
            new_event("account-opened", json!({ "accountId": "a2" })),
            new_event("account-renamed", json!({ "accountId": "a1" })),
            new_event("account-closed", json!({ "accountId": "a1" })),
        ])
        .expect("append should succeed");

    let committed_batch = subscription
        .next_batch()
        .expect("filtered subscription should receive a batch");

    assert_eq!(committed_batch.len(), 3);
    assert_eq!(
        committed_batch
            .iter()
            .map(|event_record| event_record.sequence_number)
            .collect::<Vec<_>>(),
        vec![1, 3, 4]
    );
}

pub fn append_if_conflict_emits_no_filtered_subscription_batch<S, F>(create_store: F)
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

    let subscription = store
        .subscribe_to(&EventQuery::all().with_filters([
            EventFilter::default().with_payload_predicates([json!({ "accountId": "a1" })]),
        ]))
        .expect("subscribe_to should succeed");
    let context_query = EventQuery::all().with_filters([
        EventFilter::default().with_payload_predicates([json!({ "accountId": "a1" })])
    ]);

    let error = store
        .append_if(
            vec![new_event("account-credited", json!({ "accountId": "a1" }))],
            &context_query,
            None,
        )
        .expect_err("conditional append should fail");

    assert_eq!(
        error,
        EventStoreError::ConditionalAppendConflict {
            expected: None,
            actual: Some(1),
        }
    );
    assert_eq!(
        subscription.try_next_batch(),
        Err(TryLiveSubscriptionRecvError::Empty)
    );
}

pub fn differently_filtered_subscribers_observe_the_same_commit_differently<S, F>(create_store: F)
where
    S: EventStore,
    F: Fn() -> S,
{
    let store = create_store();
    let event_type_subscription = store
        .subscribe_to(&EventQuery::for_event_types(["account-opened"]))
        .expect("subscribe_to should succeed");
    let payload_subscription = store
        .subscribe_to(&EventQuery::all().with_filters([
            EventFilter::default().with_payload_predicates([json!({ "accountId": "a2" })]),
        ]))
        .expect("subscribe_to should succeed");

    store
        .append(vec![
            new_event("account-opened", json!({ "accountId": "a1" })),
            new_event("account-credited", json!({ "accountId": "a2" })),
            new_event("account-opened", json!({ "accountId": "a2" })),
        ])
        .expect("append should succeed");

    let event_type_batch = event_type_subscription
        .next_batch()
        .expect("event-type subscription should receive a batch");
    let payload_batch = payload_subscription
        .next_batch()
        .expect("payload subscription should receive a batch");

    assert_eq!(
        event_type_batch
            .iter()
            .map(|event_record| event_record.sequence_number)
            .collect::<Vec<_>>(),
        vec![1, 3]
    );
    assert_eq!(
        payload_batch
            .iter()
            .map(|event_record| event_record.sequence_number)
            .collect::<Vec<_>>(),
        vec![2, 3]
    );
}

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

pub fn or_across_filters_matches_any_filter<S, F>(create_store: F)
where
    S: EventStore,
    F: Fn() -> S,
{
    let store = create_store();

    store
        .append(vec![
            new_event("account-opened", json!({ "accountId": "a1" })),
            new_event("account-credited", json!({ "accountId": "a2" })),
            new_event("account-debited", json!({ "accountId": "a3" })),
        ])
        .expect("append should succeed");

    let query_result = store
        .query(&EventQuery::all().with_filters([
            EventFilter::for_event_types(["account-opened"]),
            EventFilter::for_event_types(["account-debited"]),
        ]))
        .expect("query should succeed");

    assert_eq!(query_result.event_records.len(), 2);
    assert_eq!(query_result.event_records[0].sequence_number, 1);
    assert_eq!(query_result.event_records[1].sequence_number, 3);
    assert_eq!(query_result.current_context_version, Some(3));
}

pub fn or_across_event_types_inside_one_filter_matches_any_event_type<S, F>(create_store: F)
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
        .query(
            &EventQuery::all().with_filters([EventFilter::for_event_types([
                "account-opened",
                "account-debited",
            ])]),
        )
        .expect("query should succeed");

    assert_eq!(query_result.event_records.len(), 2);
    assert_eq!(query_result.event_records[0].sequence_number, 1);
    assert_eq!(query_result.event_records[1].sequence_number, 3);
    assert_eq!(query_result.current_context_version, Some(3));
}

pub fn or_across_payload_predicates_inside_one_filter_matches_any_payload_predicate<S, F>(
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
            new_event("account-opened", json!({ "accountId": "a3" })),
        ])
        .expect("append should succeed");

    let query_result = store
        .query(
            &EventQuery::all().with_filters([EventFilter::default().with_payload_predicates([
                json!({ "accountId": "a1" }),
                json!({ "accountId": "a3" }),
            ])]),
        )
        .expect("query should succeed");

    assert_eq!(query_result.event_records.len(), 2);
    assert_eq!(query_result.event_records[0].sequence_number, 1);
    assert_eq!(query_result.event_records[1].sequence_number, 3);
    assert_eq!(query_result.current_context_version, Some(3));
}

pub fn and_between_event_type_and_payload_predicate_within_one_filter<S, F>(create_store: F)
where
    S: EventStore,
    F: Fn() -> S,
{
    let store = create_store();

    store
        .append(vec![
            new_event("account-opened", json!({ "accountId": "a1" })),
            new_event("account-credited", json!({ "accountId": "a1" })),
            new_event("account-opened", json!({ "accountId": "a2" })),
        ])
        .expect("append should succeed");

    let query_result = store
        .query(
            &EventQuery::all().with_filters([EventFilter::for_event_types(["account-opened"])
                .with_payload_predicates([json!({ "accountId": "a2" })])]),
        )
        .expect("query should succeed");

    assert_eq!(query_result.event_records.len(), 1);
    assert_eq!(query_result.event_records[0].sequence_number, 3);
    assert_eq!(query_result.current_context_version, Some(3));
}

pub fn scalar_subset_match_works<S, F>(create_store: F)
where
    S: EventStore,
    F: Fn() -> S,
{
    let store = create_store();

    store
        .append(vec![new_event(
            "account-opened",
            json!({ "accountId": "a1", "name": "Rico" }),
        )])
        .expect("append should succeed");

    let query_result = store
        .query(&EventQuery::all().with_filters([
            EventFilter::default().with_payload_predicates([json!({ "accountId": "a1" })]),
        ]))
        .expect("query should succeed");

    assert_eq!(query_result.event_records.len(), 1);
    assert_eq!(query_result.current_context_version, Some(1));
}

pub fn nested_object_subset_match_works<S, F>(create_store: F)
where
    S: EventStore,
    F: Fn() -> S,
{
    let store = create_store();

    store
        .append(vec![new_event(
            "user-created",
            json!({ "user": { "id": "u1", "role": "admin" } }),
        )])
        .expect("append should succeed");

    let query_result = store
        .query(&EventQuery::all().with_filters([
            EventFilter::default().with_payload_predicates([json!({ "user": { "id": "u1" } })]),
        ]))
        .expect("query should succeed");

    assert_eq!(query_result.event_records.len(), 1);
    assert_eq!(query_result.current_context_version, Some(1));
}

pub fn array_subset_match_with_scalar_elements_works<S, F>(create_store: F)
where
    S: EventStore,
    F: Fn() -> S,
{
    let store = create_store();

    store
        .append(vec![new_event(
            "customer-tagged",
            json!({ "tags": ["vip", "beta"] }),
        )])
        .expect("append should succeed");

    let query_result = store
        .query(&EventQuery::all().with_filters([
            EventFilter::default().with_payload_predicates([json!({ "tags": ["vip"] })]),
        ]))
        .expect("query should succeed");

    assert_eq!(query_result.event_records.len(), 1);
    assert_eq!(query_result.current_context_version, Some(1));
}

pub fn array_subset_match_with_object_elements_works<S, F>(create_store: F)
where
    S: EventStore,
    F: Fn() -> S,
{
    let store = create_store();

    store
        .append(vec![new_event(
            "order-created",
            json!({ "items": [{ "sku": "a", "qty": 2 }, { "sku": "b" }] }),
        )])
        .expect("append should succeed");

    let query_result = store
        .query(&EventQuery::all().with_filters([
            EventFilter::default().with_payload_predicates([json!({ "items": [{ "sku": "a" }] })]),
        ]))
        .expect("query should succeed");

    assert_eq!(query_result.event_records.len(), 1);
    assert_eq!(query_result.current_context_version, Some(1));
}

pub fn payload_predicate_no_match_returns_no_events<S, F>(create_store: F)
where
    S: EventStore,
    F: Fn() -> S,
{
    let store = create_store();

    store
        .append(vec![new_event(
            "account-opened",
            json!({ "accountId": "a1", "name": "Rico" }),
        )])
        .expect("append should succeed");

    let query_result = store
        .query(&EventQuery::all().with_filters([
            EventFilter::default().with_payload_predicates([json!({ "accountId": "missing" })]),
        ]))
        .expect("query should succeed");

    assert!(query_result.event_records.is_empty());
    assert_eq!(query_result.last_returned_sequence_number, None);
    assert_eq!(query_result.current_context_version, None);
}

pub fn empty_event_types_filter_returns_no_events_and_no_context_version<S, F>(create_store: F)
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
        .query(&EventQuery::all().with_filters([EventFilter {
            event_types: Some(Vec::new()),
            payload_predicates: None,
        }]))
        .expect("query should succeed");

    assert!(query_result.event_records.is_empty());
    assert_eq!(query_result.last_returned_sequence_number, None);
    assert_eq!(query_result.current_context_version, None);
}

pub fn conditional_append_uses_empty_event_types_filter_as_empty_context<S, F>(create_store: F)
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

    let context_query = EventQuery::all().with_filters([EventFilter {
        event_types: Some(Vec::new()),
        payload_predicates: None,
    }]);

    let append_result = store
        .append_if(
            vec![new_event("account-renamed", json!({ "accountId": "a2" }))],
            &context_query,
            None,
        )
        .expect("conditional append should succeed");

    assert_eq!(append_result.first_sequence_number, 2);
    assert_eq!(append_result.last_sequence_number, 2);
    assert_eq!(append_result.committed_count, 1);
}

pub fn payload_array_match_is_order_insensitive<S, F>(create_store: F)
where
    S: EventStore,
    F: Fn() -> S,
{
    let store = create_store();

    store
        .append(vec![new_event(
            "customer-tagged",
            json!({ "tags": ["vip", "beta"] }),
        )])
        .expect("append should succeed");

    let query_result = store
        .query(&EventQuery::all().with_filters([
            EventFilter::default().with_payload_predicates([json!({ "tags": ["beta", "vip"] })]),
        ]))
        .expect("query should succeed");

    assert_eq!(query_result.event_records.len(), 1);
    assert_eq!(query_result.current_context_version, Some(1));
}

pub fn payload_array_object_match_can_match_non_first_payload_element<S, F>(create_store: F)
where
    S: EventStore,
    F: Fn() -> S,
{
    let store = create_store();

    store
        .append(vec![new_event(
            "order-created",
            json!({ "items": [{ "sku": "a" }, { "sku": "b", "qty": 2 }] }),
        )])
        .expect("append should succeed");

    let query_result = store
        .query(&EventQuery::all().with_filters([
            EventFilter::default().with_payload_predicates([json!({ "items": [{ "sku": "b" }] })]),
        ]))
        .expect("query should succeed");

    assert_eq!(query_result.event_records.len(), 1);
    assert_eq!(query_result.current_context_version, Some(1));
}

pub fn conditional_append_succeeds_for_matching_payload_filtered_context_version<S, F>(
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
        ])
        .expect("append should succeed");

    let context_query = EventQuery::all().with_filters([
        EventFilter::default().with_payload_predicates([json!({ "accountId": "a1" })])
    ]);
    let append_result = store
        .append_if(
            vec![new_event("account-renamed", json!({ "accountId": "a3" }))],
            &context_query,
            Some(1),
        )
        .expect("conditional append should succeed");

    assert_eq!(append_result.first_sequence_number, 3);
    assert_eq!(append_result.last_sequence_number, 3);
    assert_eq!(append_result.committed_count, 1);
}

pub fn conditional_append_fails_for_stale_payload_filtered_context_version<S, F>(create_store: F)
where
    S: EventStore,
    F: Fn() -> S,
{
    let store = create_store();

    store
        .append(vec![
            new_event("account-opened", json!({ "accountId": "a1" })),
            new_event(
                "account-opened",
                json!({ "accountId": "a1", "name": "Rico" }),
            ),
        ])
        .expect("append should succeed");

    let context_query = EventQuery::all().with_filters([
        EventFilter::default().with_payload_predicates([json!({ "accountId": "a1" })])
    ]);
    let error = store
        .append_if(
            vec![new_event("account-renamed", json!({ "accountId": "a3" }))],
            &context_query,
            Some(1),
        )
        .expect_err("conditional append should fail");

    assert_eq!(
        error,
        EventStoreError::ConditionalAppendConflict {
            expected: Some(1),
            actual: Some(2),
        }
    );
}

pub fn failed_conditional_append_does_not_append_any_part_of_the_batch<S, F>(create_store: F)
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

    let context_query = EventQuery::all()
        .with_filters([
            EventFilter::default().with_payload_predicates([json!({ "accountId": "a1" })])
        ])
        .with_min_sequence_number(1);

    let _ = store
        .append_if(
            vec![
                new_event("account-renamed", json!({ "accountId": "a1" })),
                new_event("account-credited", json!({ "accountId": "a1" })),
            ],
            &context_query,
            None,
        )
        .expect_err("conditional append should fail");

    let query_result = store
        .query(&EventQuery::all())
        .expect("query should succeed");

    assert_eq!(query_result.event_records.len(), 1);
    assert_eq!(query_result.event_records[0].sequence_number, 1);
    assert_eq!(query_result.last_returned_sequence_number, Some(1));
    assert_eq!(query_result.current_context_version, Some(1));
}

pub fn failed_conditional_append_does_not_consume_sequence_numbers_for_later_commits<S, F>(
    create_store: F,
) where
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

    let context_query = EventQuery::all().with_filters([
        EventFilter::default().with_payload_predicates([json!({ "accountId": "a1" })])
    ]);

    let conflict = store
        .append_if(
            vec![new_event("account-renamed", json!({ "accountId": "a1" }))],
            &context_query,
            None,
        )
        .expect_err("conditional append should fail");

    assert_eq!(
        conflict,
        EventStoreError::ConditionalAppendConflict {
            expected: None,
            actual: Some(1),
        }
    );

    let append_result = store
        .append(vec![new_event(
            "account-credited",
            json!({ "accountId": "a1" }),
        )])
        .expect("later append should succeed");

    assert_eq!(append_result.first_sequence_number, 2);
    assert_eq!(append_result.last_sequence_number, 2);
    assert_eq!(append_result.committed_count, 1);

    let query_result = store
        .query(&EventQuery::all())
        .expect("query should succeed");

    assert_eq!(query_result.event_records.len(), 2);
    assert_eq!(query_result.event_records[0].sequence_number, 1);
    assert_eq!(query_result.event_records[1].sequence_number, 2);
    assert_eq!(query_result.last_returned_sequence_number, Some(2));
    assert_eq!(query_result.current_context_version, Some(2));
}
