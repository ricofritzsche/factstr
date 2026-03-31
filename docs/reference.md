# Reference

This page is a compact reference for the current public FACTSTR contract.

## `NewEvent`

Input event for `append` and `append_if`.

- carries `event_type`
- carries `payload`
- has no assigned `sequence_number` yet

## `EventRecord`

Committed fact stored in the append-only log.

- has a global `sequence_number`
- keeps the committed `event_type`
- keeps the committed `payload`

## `EventFilter`

One query filter inside an `EventQuery`.

- `event_types` is optional
- `payload_predicates` is optional
- one filter matches when its event-type constraint matches, if present, and its payload constraint matches, if present

## `EventQuery`

Current query shape for reads and conflict context.

- `filters` is optional
- `min_sequence_number` is an exclusive read cursor
- `min_sequence_number` affects returned rows only
- omitted or empty `filters` means all events for matching purposes

## `QueryResult`

Result of `query`.

- `event_records` are returned in ascending sequence order
- `last_returned_sequence_number` describes only returned rows
- `current_context_version` describes the full matching context and ignores `min_sequence_number`

## `AppendResult`

Result of one committed append batch.

- `first_sequence_number`
- `last_sequence_number`
- `committed_count`

This makes the committed sequence range explicit without overloading it with context-version meaning.

## `HandleEvents`

Callback type for projection updates and other live subscriptions.

- receives one delivered committed batch as `Vec<EventRecord>`
- returns `Result<(), SubscriptionHandlerError>`
- is used by both `subscribe_all(...)` and `subscribe_to(...)`

## `SubscriptionHandlerError`

Error returned by a subscription handler.

- represents handler-local failure
- does not roll back a successful append

## `EventSubscription`

Active live subscription registration.

- has a stable id
- `unsubscribe()` stops future delivery for that subscriber
- a batch already snapshotted for a committed append may still be delivered

## `EventStore`

Shared runtime contract across store implementations.

- `query`
- `append`
- `append_if`
- `subscribe_all`
- `subscribe_to`

Memory and PostgreSQL must preserve the same observable contract behavior.

- `subscribe_all(handle)` delivers all future committed batches to the handler
- `subscribe_to(&EventQuery, handle)` delivers only future committed facts that match that query, preserving original committed order inside each delivered batch
- the common use is a feature-local read model that updates from those committed batches

## `EventStoreError`

Current shared store error type.

- `EmptyAppend`
- `ConditionalAppendConflict`
- `BackendFailure`

This stays intentionally small and focused on current shared behavior.
