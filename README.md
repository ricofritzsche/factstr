# FACTSTR

FACTSTR is a Rust event store built around immutable facts, command-context consistency, committed-batch streams, and durable replay.

It is meant to be easy to start with: append the first fact, read the relevant facts, check context safely, and keep feature-owned query models current from committed batches.

## Links

- Website: [factstr.com](https://factstr.com)
- Local docs entry page: [`docs/index.md`](docs/index.md)
- Getting started: [`docs/getting-started.md`](docs/getting-started.md)
- Streams: [`docs/streams.md`](docs/streams.md)
- Stores: [`docs/stores.md`](docs/stores.md)
- Reference: [`docs/reference.md`](docs/reference.md)

## Why FACTSTR

FACTSTR is for software that should grow feature by feature without forcing aggregate-centric structure first.

It keeps the important behavior explicit:

- facts are append-only
- consistency is checked against the relevant command context
- reads stay ordered
- committed batches are delivered only after append succeeds
- durable streams replay from a stored cursor and then continue live

This gives a direct path for:

- starting with the first real feature
- keeping decisions local to the relevant facts
- updating feature-owned query models from committed batches
- replaying and catching up after restart
- choosing embedded or database-backed persistence without changing the core model

## Implemented Now

The current shared contract supports:

- append
- query
- conditional append with typed conflict failure
- `stream_all`
- `stream_to`
- `stream_all_durable`
- `stream_to_durable`
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
- `HandleStream`
- `StreamHandlerError`
- `DurableStream`
- `EventStream`
- `EventStore`
- `EventStoreError`

## Stores

FACTSTR currently includes three store implementations.

### `factstr-memory`

The memory store is the semantic reference implementation.

Use it for:

- tests
- local development
- direct reasoning about behavior

It implements:

- append
- query
- append_if
- `stream_all`
- `stream_to`
- `stream_all_durable`
- `stream_to_durable`

Durable streams in memory are limited to the lifetime of one `MemoryStore` instance.

### `factstr-sqlite`

The SQLite store is the embedded persistent implementation.

Use it for:

- local persistence without external infrastructure
- durable replay and catch-up across restart
- validating the shared contract against an embedded store

It implements:

- append
- query
- append_if
- `stream_all`
- `stream_to`
- `stream_all_durable`
- `stream_to_durable`

### `factstr-postgres`

The PostgreSQL store implements the same contract on top of PostgreSQL using SQLx.

Use it for:

- teams that already operate PostgreSQL
- persistence with familiar database tooling
- validating the shared contract against a database-backed store

It implements:

- append
- query
- append_if
- `stream_all`
- `stream_to`
- `stream_all_durable`
- `stream_to_durable`

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
- `last_returned_sequence_number` describes only the returned rows
- `current_context_version` describes the full matching context
- `current_context_version` ignores `min_sequence_number`

### Conditional append

- `append_if` checks the full conflict context
- `append_if` ignores `min_sequence_number` for conflict detection
- stale context returns `EventStoreError::ConditionalAppendConflict`
- failed conditional append does not partially append a batch
- failed conditional append does not consume sequence numbers that later successful appends would observe

### Streams

- notifications happen only after a successful commit
- each committed append batch is delivered as one batch
- mixed committed batches are delivered as one filtered batch when matches exist
- delivery order follows committed global sequence order
- failed conditional append delivers nothing
- multiple stream handlers can observe the same committed batches
- handler failure does not roll back append success

### Durable streams

- durable replay starts strictly after the stored cursor
- replay/live transition has no duplicates or gaps
- durable cursors do not advance past undelivered committed facts
- Memory implements durable streams within one `MemoryStore` instance only
- SQLite implements durable streams with persisted cursors and replay across restart
- PostgreSQL implements durable streams with persisted cursors and replay across restart

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
- within one filter, event-type matching and payload matching are combined with AND
- omitted or empty `filters` means all events
- `event_types: None` means event type is unconstrained
- `payload_predicates: None` means payload is unconstrained
- `event_types: Some([])` means explicit empty event-type match set
- `payload_predicates: Some([])` means explicit empty payload-predicate match set

Payload predicates use recursive subset matching.

Current rules:

- scalar values must be equal
- objects match recursively by subset
- arrays match if every predicate element is contained somewhere in the payload array
- array element containment uses the same recursive subset logic
- extra keys in the event payload are allowed
- extra array elements in the event payload are allowed

## Quick Start

Check the workspace:

```bash
cargo check
```

Run the memory-store tests:

```bash
cargo test -p factstr-memory
```

Run the basic in-memory example:

```bash
cargo run --manifest-path examples/basic-memory/Cargo.toml
```

Run the feature-owned query-model example:

```bash
cargo run --manifest-path examples/account-projection/Cargo.toml
```

Run the PostgreSQL store tests:

```bash
DATABASE_URL=postgres://postgres:postgres@localhost:5432/postgres cargo test -p factstr-postgres
```

Run the full workspace test suite with PostgreSQL enabled:

```bash
DATABASE_URL=postgres://postgres:postgres@localhost:5432/postgres cargo test
```

## Using FACTSTR From Another Repository

The current intended consumption path is a git dependency.

Use the shared contract only:

```toml
[dependencies]
factstr = { git = "https://github.com/ricofritzsche/factstr.git" }
```

Use the in-memory store:

```toml
[dependencies]
factstr = { git = "https://github.com/ricofritzsche/factstr.git" }
factstr-memory = { git = "https://github.com/ricofritzsche/factstr.git" }
```

Use the SQLite store:

```toml
[dependencies]
factstr = { git = "https://github.com/ricofritzsche/factstr.git" }
factstr-sqlite = { git = "https://github.com/ricofritzsche/factstr.git" }
```

Use the PostgreSQL store:

```toml
[dependencies]
factstr = { git = "https://github.com/ricofritzsche/factstr.git" }
factstr-postgres = { git = "https://github.com/ricofritzsche/factstr.git" }
```

## Workspace

This repository currently contains:

* `factstr`
* `factstr-memory`
* `factstr-sqlite`
* `factstr-postgres`
* `factstr-conformance`

Crate roles:

* `factstr`: shared runtime contract crate
* `factstr-memory`: publishable in-memory runtime store
* `factstr-sqlite`: publishable embedded SQLite runtime store
* `factstr-postgres`: publishable PostgreSQL runtime store
* `factstr-conformance`: internal reusable semantic test support

## Current Scope Boundary

Implemented now:

* shared contract
* memory store
* sqlite store
* postgres store
* reusable store conformance tests
* streams
* durable streams in Memory, SQLite, and PostgreSQL

Not implemented now:

* file store
* transport adapters
* migrations framework
* first-class query-model runtime above streams

## License

Licensed under either of:

* MIT license ([LICENSE-MIT](LICENSE-MIT))
* Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
