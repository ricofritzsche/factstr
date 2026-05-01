# FACTSTR

FACTSTR is a Rust event store built around immutable facts, command-context consistency, committed-batch streams, and durable replay.

It is meant to be easy to start with: append the first fact, read the relevant facts, check context safely, and keep feature-owned query models current from committed batches.

## Repository Shape

The repository is organized around explicit categories:

- `factstr` defines the shared Rust contract
- `factstr-memory`, `factstr-sqlite`, and `factstr-postgres` are store implementations that preserve that contract
- `factstr-conformance` holds reusable semantic checks for store implementations
- `factstr-interop` defines a backend-independent interop DTO boundary above the Rust core for later language adapters
- `factstr-node` is the first language adapter and currently exposes a memory-backed TypeScript and Node package on top of `factstr-interop`

`factstr-interop` is not a store implementation. It does not reimplement append, query, conditional append, streams, or durable streams. It keeps the first cross-language boundary explicit without making a language-specific adapter define the contract shape by accident.
It is internal support for language adapters in this repository, not the user-facing package target for this step.

`factstr-node` is not a store implementation. It is not a transport adapter. The current scope is intentionally narrow: a memory-backed TypeScript and Node package for append, query, and conditional append through the interop DTO boundary. The public package is intended to resolve prebuilt native binaries by platform, while `factstr-interop` stays internal support.

## Release Model

Rust releases are managed by `release-plz`.

FACTSTR uses one unified public version line for its public artifacts. The current public version is `0.2.1`.

- publishable crates: `factstr`, `factstr-memory`, `factstr-sqlite`, `factstr-postgres`
- non-products on crates.io: `factstr-interop`, the Rust `factstr-node` crate, and `factstr-conformance`

The intended steady-state Rust path is GitHub Actions trusted publishing. Brand-new crates may still need a one-time bootstrap `CARGO_REGISTRY_TOKEN` in GitHub Actions for the first publish before trusted publishing takes over.

The npm release lane publishes only `@factstr/factstr-node`.

- `@factstr/factstr-node` is the public npm package
- `@factstr/factstr-node` follows the same public release version as the Rust core crates
- the prebuilt packages under `factstr-node/npm/*` exist only for native distribution support

Release automation stays split by lane:

- Rust crates publish through the Rust release lane
- `@factstr/factstr-node` publishes through the npm release lane

The public version line is shared even though the publish lanes remain separate. Node publishing is intended to run through GitHub Actions trusted publishing on GitHub-hosted runners, not from a developer laptop.

## Links

- Website: https://factstr.com
- Local docs entry page: docs/index.md
- Getting started: docs/getting-started.md
- Streams: docs/streams.md
- Stores: docs/stores.md
- SQLite store guidance: docs/sqlite.md
- Reference: docs/reference.md

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

The current interop boundary types are:

- `InteropNewEvent`
- `InteropEventRecord`
- `InteropEventFilter`
- `InteropEventQuery`
- `InteropQueryResult`
- `InteropAppendResult`
- `InteropConditionalAppendConflict`
- `InteropError`

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

The SQLite store is the embedded persistent implementation. For guidance on when SQLite is the right store and when it is not, see [docs/sqlite.md](docs/sqlite.md).

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
- each returned `EventRecord` includes `occurred_at`
- `occurred_at` is recorded event time, not the ordering key
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
