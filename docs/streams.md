# Streams

FACTSTR exposes streams as the post-commit delivery surface behind projection updates.

The main use is still the same: a feature slice owns a query model, registers a stream for the facts relevant to that model, and updates it from committed batches.

## Shared Stream Contract

- `stream_all(handle)` observes all future committed batches
- `stream_to(&EventQuery, handle)` observes only future committed facts that match that query
- notifications happen only after successful persistence
- each committed append batch is delivered as one batch
- mixed committed batches are delivered as one filtered batch when matches exist
- delivery order follows committed global sequence order
- failed conditional append emits nothing
- `EventStream::unsubscribe()` stops future deliveries
- a batch already snapshotted for asynchronous delivery may still arrive after `unsubscribe()`

## Durable Streams

- `stream_all_durable(&DurableStream, handle)` resumes from that durable stream's stored cursor, replays committed batches after it, then continues with future committed batches
- `stream_to_durable(&DurableStream, &EventQuery, handle)` does the same with query-defined filtering
- replay starts strictly after the stored cursor
- replay uses ascending committed order
- replay/live transition has no duplicates or gaps
- the durable cursor does not advance past undelivered committed facts

Shared reusable durable-stream conformance now exists in `factstore-conformance`.
Remaining store-specific tests prove only store-local boundaries such as restart persistence, in-memory lifetime limits, and explicit replay-history rejection on older persistent databases.

## Current Store Status

- `factstore-memory`
  - implements `stream_all`, `stream_to`, `stream_all_durable`, and `stream_to_durable`
  - implements durable streams within one `MemoryStore` instance only
  - keeps durable stream cursor state only for the lifetime of one `MemoryStore` instance
- `factstore-sqlite`
  - implements `stream_all`, `stream_to`, `stream_all_durable`, and `stream_to_durable`
  - persists durable stream cursors and replay state across restart
  - replays committed batches from stored cursors before switching to future committed delivery
  - rejects durable replay on older databases that do not have contiguous `append_batches` history
  - durable replay depends on persisted `append_batches` history
- `factstore-postgres`
  - implements `stream_all`, `stream_to`, `stream_all_durable`, and `stream_to_durable`
  - persists durable stream cursors and replay state across restart
  - replays committed batches from stored cursors before switching to future committed delivery
  - rejects durable replay on older databases that do not have contiguous `append_batches` history
  - durable replay depends on persisted `append_batches` history

## Projection Use Case

A feature slice can:

- define a read model it owns locally
- define an `EventQuery` for the facts that should update that model
- call `stream_to(...)` once for future-only projection updates
- call `stream_to_durable(...)` when it needs persisted replay/catch-up
- update its own query model from each delivered committed batch

This keeps unrelated facts out of that feature slice by contract instead of by ad-hoc manual filtering after delivery.
