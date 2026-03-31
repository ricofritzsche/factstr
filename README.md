# FACTSTR

[FACTSTR](https://factstore.ricofritzsche.me) (pronounced: factstore) is a Rust event store built around facts, query-defined consistency context, and multiple store implementations behind one shared contract.

## Documentation

- local docs entry page: [`docs/index.md`](docs/index.md)
- examples and contract reference: [`docs/examples.md`](docs/examples.md), [`docs/reference.md`](docs/reference.md)
- GitHub Pages deployment is configured through Actions in [`.github/workflows/docs.yml`](.github/workflows/docs.yml)

This repository currently contains:

- a shared runtime contract crate
- a memory store
- a PostgreSQL store
- a reusable conformance-test crate for store implementations

It does not currently contain:

- an embedded persistent store
- HTTP or gRPC adapters

## Workspace

```mermaid
flowchart LR
    factstore["factstore<br/>shared runtime contract"]
    conformance["factstore-conformance<br/>reusable semantic test support"]
    memory["factstore-memory<br/>in-memory store"]
    postgres["factstore-postgres<br/>PostgreSQL store"]

    conformance --> factstore
    memory --> factstore
    postgres --> factstore
    memory -. test/dev dependency .-> conformance
    postgres -. test/dev dependency .-> conformance
```

Workspace members:

- `factstore`
- `factstore-conformance`
- `factstore-memory`
- `factstore-postgres`

Crate roles:

- `factstore`: shared runtime contract crate
- `factstore-memory`: publishable in-memory runtime store
- `factstore-postgres`: publishable PostgreSQL runtime store
- `factstore-conformance`: internal reusable test-support crate, not intended for publishing now

## What Is Implemented

The current shared contract supports:

- append of one or more events
- query
- conditional append with typed conflict failure
- live subscriptions for future committed batches
- event-type filtering
- payload-predicate filtering
- explicit separation of:
  - `last_returned_sequence_number`
  - `current_context_version`

The current public contract types are:

- `NewEvent`
- `EventRecord`
- `EventFilter`
- `EventQuery`
- `QueryResult`
- `AppendResult`
- `LiveSubscription`
- `LiveSubscriptionRecvError`
- `TryLiveSubscriptionRecvError`
- `EventStore`
- `EventStoreError`

## Core Semantics

These are the load-bearing behaviors implemented today.

### Append

- events are append-only
- sequence numbers are global and monotonically increasing
- one committed batch receives one consecutive sequence range
- `AppendResult` reports:
  - `first_sequence_number`
  - `last_sequence_number`
  - `committed_count`
- empty append input returns `EventStoreError::EmptyAppend`

### Query

- returned events are ordered by ascending `sequence_number`
- `min_sequence_number` is an exclusive read cursor
- returned rows satisfy:
  - `sequence_number > min_sequence_number`
- `last_returned_sequence_number` describes only the returned rows
- `current_context_version` describes the full matching context
- `current_context_version` ignores `min_sequence_number`

### Conditional Append

- `append_if` checks the full conflict context
- `append_if` ignores `min_sequence_number` for conflict detection
- stale context returns `EventStoreError::ConditionalAppendConflict`
- failed conditional append does not partially append a batch
- under the current Rust contract, failed appends also do not consume sequence numbers that later successful appends would observe

### Live Subscriptions

- subscriptions are live only and do not replay history
- notifications happen only after a successful commit
- each committed append batch is delivered as one batch
- delivery order follows committed global sequence order
- failed conditional append delivers nothing
- multiple subscribers can observe the same committed batches
- dropping one subscription does not break append for others

## Query Model

`EventQuery` currently contains:

- `filters: Option<Vec<EventFilter>>`
- `min_sequence_number: Option<u64>`

`EventFilter` currently contains:

- `event_types: Option<Vec<String>>`
- `payload_predicates: Option<Vec<serde_json::Value>>`

Current matching rules:

- `EventQuery.filters` is OR across filters
- within one filter, `event_types` is OR across event types
- within one filter, `payload_predicates` is OR across payload predicates
- within one filter, event-type matching AND payload matching are combined with AND
- omitted or empty `filters` means all events
- `event_types: None` means event type is unconstrained
- `payload_predicates: None` means payload is unconstrained
- `event_types: Some([])` means explicit empty event-type match set
- `payload_predicates: Some([])` means explicit empty payload-predicate match set

```mermaid
flowchart TD
    query["EventQuery"]
    filters["filters<br/>OR across filters"]
    cursor["min_sequence_number<br/>exclusive read cursor only"]
    filter["EventFilter"]
    types["event_types<br/>OR across values"]
    payloads["payload_predicates<br/>OR across values"]
    match["filter matches when<br/>(event type matches if constrained)<br/>AND<br/>(payload matches if constrained)"]

    query --> filters
    query --> cursor
    filters --> filter
    filter --> types
    filter --> payloads
    types --> match
    payloads --> match
```

## Payload Predicate Semantics

Payload predicates use recursive subset matching.

Current rules:

- scalar values must be equal
- objects match recursively by subset
- arrays match if every predicate element is contained somewhere in the payload array
- array element containment uses the same recursive subset logic
- extra keys in the event payload are allowed
- extra array elements in the event payload are allowed

The memory store implements this directly in Rust.
The PostgreSQL store uses `JSONB` containment with `payload @> ...` for the current query slice.

## Query And Conditional Append Flow

```mermaid
sequenceDiagram
    participant C as Caller
    participant S as Store

    C->>S: query(context, min_sequence_number?)
    S-->>C: event_records + last_returned_sequence_number + current_context_version

    C->>S: append_if(events, context, expected_context_version)
    S->>S: compute current_context_version from full context
    alt version matches
        S->>S: append full batch
        S-->>C: AppendResult
    else version differs
        S-->>C: ConditionalAppendConflict
    end
```

## Store Implementations

### factstore-memory

The memory store is the semantic reference implementation.

It keeps:

- all events in memory
- explicit global sequence allocation
- query matching in plain Rust
- no persistence

### factstore-postgres

The PostgreSQL store implements the same contract with SQLx.

It currently:

- creates the required table and indexes on connect
- uses visible SQL
- uses explicit transactions for append and conditional append
- locks the `events` table during append/conditional append to preserve the current Rust sequence guarantee
- uses per-query schema selection in tests via PostgreSQL `search_path`

Current schema created by the postgres crate:

```sql
CREATE TABLE IF NOT EXISTS events (
  sequence_number BIGINT PRIMARY KEY,
  occurred_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  event_type TEXT NOT NULL,
  payload JSONB NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_events_type ON events(event_type);
CREATE INDEX IF NOT EXISTS idx_events_occurred_at ON events(occurred_at);
CREATE INDEX IF NOT EXISTS idx_events_payload_gin ON events USING gin(payload);
```

Important current PostgreSQL note:

- the established TypeScript baseline uses `BIGSERIAL`
- the Rust postgres store intentionally does not use database-owned sequence allocation yet
- it assigns `sequence_number` explicitly under lock so committed batches stay consecutive without gaps from failed appends or rolled-back conditional appends

## Test Structure

Semantic expectations are shared across store implementations.

- `factstore-conformance` owns reusable semantic tests
- `factstore-memory/tests/` uses those conformance helpers
- `factstore-postgres/tests/` uses the same conformance helpers

This means both stores are checked against the same current contract for:

- append sequencing
- query ordering
- `min_sequence_number` behavior
- payload filtering
- context-version behavior
- conditional append conflict handling
- failed conditional append not consuming later sequence numbers

## Running Checks

Format and compile:

```bash
cargo fmt --all
cargo check
```

Run memory-store tests:

```bash
cargo test -p factstore-memory
```

Run postgres-store tests:

```bash
DATABASE_URL=postgres://postgres:postgres@localhost:5432/postgres cargo test -p factstore-postgres
```

Run the full workspace test suite with postgres enabled:

```bash
DATABASE_URL=postgres://postgres:postgres@localhost:5432/postgres cargo test
```

## Using From Another Repository

The current intended consumption path is a git dependency.

Use the shared contract only:

```toml
[dependencies]
factstore = { git = "https://github.com/ricofritzsche/factstore.git" }
```

Use the in-memory store:

```toml
[dependencies]
factstore = { git = "https://github.com/ricofritzsche/factstore.git" }
factstore-memory = { git = "https://github.com/ricofritzsche/factstore.git" }
```

Use the PostgreSQL store:

```toml
[dependencies]
factstore = { git = "https://github.com/ricofritzsche/factstore.git" }
factstore-postgres = { git = "https://github.com/ricofritzsche/factstore.git" }
```

## Publish Readiness

This workspace is being prepared for future publishing, but this repository task does not publish any crate.

Current publishability decision:

- `factstore`: intended to be publishable
- `factstore-memory`: intended to be publishable
- `factstore-postgres`: intended to be publishable
- `factstore-conformance`: kept internal for now with `publish = false`

## Packaging Verification

Safe packaging checks:

```bash
cargo publish --dry-run -p factstore
cargo publish --dry-run -p factstore-memory
cargo publish --dry-run -p factstore-postgres
```

For the current interdependent workspace, the most accurate first verification is:

```bash
cargo publish --dry-run --workspace --exclude factstore-conformance
```

Inspect package contents:

```bash
cargo package --list -p factstore
cargo package --list -p factstore-memory
cargo package --list -p factstore-postgres
```

## PostgreSQL Test Setup

`factstore-postgres` integration tests require `DATABASE_URL`.

Requirements:

- `DATABASE_URL` must point to a PostgreSQL database
- the configured user must be able to create schemas in that database

Current test behavior:

- each test run creates a unique schema named like `factstore_test_<timestamp>_<id>`
- the store under test connects with that schema in `search_path`
- tables are created inside that schema
- the current helper does not automatically drop those schemas after the run

This setup keeps tests isolated without assuming a machine-specific PostgreSQL installation path.

## Current Scope Boundary

This repository is intentionally still narrow.

Implemented now:

- shared contract
- memory store
- postgres store
- reusable store conformance tests
- live subscriptions

Not implemented now:

- durable subscriber cursors and replay
- embedded persistent store
- file store
- transport adapters
- performance/index tuning beyond the current postgres baseline indexes
- migrations framework

## License

Licensed under either of:

- MIT license ([LICENSE-MIT](LICENSE-MIT))
- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
