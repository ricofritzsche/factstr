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
- carries `occurred_at` as the recorded occurrence time
- keeps the committed `event_type`
- keeps the committed `payload`

`sequence_number` defines committed log order. `occurred_at` does not.

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

## `HandleStream`

Callback type for streams and durable streams.

- receives one delivered committed batch as `Vec<EventRecord>`
- returns `Result<(), StreamHandlerError>`
- is used by `stream_all(...)`, `stream_to(...)`, `stream_all_durable(...)`, and `stream_to_durable(...)`

## `StreamHandlerError`

Error returned by a stream handler.

- represents handler-local failure
- does not roll back a successful append

## `DurableStream`

Stable durable stream identity.

- names one durable stream cursor
- is used by `stream_all_durable(...)` and `stream_to_durable(...)`

## `EventStream`

Active stream registration.

- has a stable id
- `unsubscribe()` stops future delivery for that stream registration
- a batch already snapshotted for a committed append may still be delivered

## `EventStore`

Shared runtime contract across store implementations.

- `append`
- `query`
- `append_if`
- `stream_all`
- `stream_to`
- `stream_all_durable`
- `stream_to_durable`

All stores must preserve the same observable append/query/append-if/stream behavior.

- `stream_all(handle)` delivers all future committed batches to the handler
- `stream_to(&EventQuery, handle)` delivers only future committed facts that match that query, preserving original committed order inside each delivered batch
- `stream_all_durable(&DurableStream, handle)` resumes from the stored durable cursor, replays committed batches after it, and then continues with future committed batches
- `stream_to_durable(&DurableStream, &EventQuery, handle)` does the same with query-defined filtering
- durable replay starts strictly after the stored cursor
- durable replay transitions into future committed delivery without duplicates or gaps
- durable cursors do not advance past undelivered committed facts
- the common use is still a feature-local read model that updates from committed batches

## `EventStoreError`

Current shared store error type.

- `EmptyAppend`
- `ConditionalAppendConflict`
- `NotImplemented`
- `BackendFailure`

This stays intentionally small and focused on current shared behavior.

## Node and TypeScript Boundary

`@factstr/factstr-node` currently exposes a smaller package boundary than the full Rust contract.

- current Node package surface:
  - `FactstrMemoryStore`
  - `append`
  - `query`
  - `appendIf`
- current Node package is memory-backed only
- current Node package does not yet expose SQLite, PostgreSQL, streams, durable streams, or transport behavior

See [Node and TypeScript](node-typescript.md) for the current package examples.
