# Getting Started

This page is the shortest path to running the current FACTSTR repository and understanding what is already implemented.

## Prerequisites

- Rust toolchain with Cargo
- PostgreSQL only if you want to run the PostgreSQL store tests

## Clone The Repository

```bash
git clone https://github.com/ricofritzsche/factstore.git
cd factstore
```

## Check The Workspace

```bash
cargo check
```

This verifies the shared contract crate, the memory store, the SQLite store, the PostgreSQL store, and the conformance test crate all compile together.

## Start With The Memory Store

Run the in-memory store tests first:

```bash
cargo test -p factstore-memory
```

This is the simplest way to see the current semantic contract in action:

- append
- query
- conditional append
- projection updates through streams

If you want the first direct code path after that, run the basic memory example:

```bash
cargo run --manifest-path examples/basic-memory/Cargo.toml
```

See [Examples](examples.md) for the source and what it proves.

For the common feature-slice path, run the account projection example next:

```bash
cargo run --manifest-path examples/account-projection/Cargo.toml
```

That example shows a feature slice owning a read model, streaming only the facts relevant to that model, and updating it from committed batches.

For the SQLite-backed feature-slice example, run the bank CLI after that:

```bash
cargo run --manifest-path examples/bank-slices-cli/Cargo.toml
```

That example shows:

- write-side `*Command` slices such as `open_account`, `deposit`, `withdraw`, and `transfer`
- read-side query slices such as `fetch_balance` and `fetch_movement_history`
- a real `stream_to(...)` registration owned locally by `fetch_balance`
- SQLite-backed facts with an interactive CLI

## Run The PostgreSQL Store Tests

Set `DATABASE_URL` to a PostgreSQL database where the configured user can create schemas, then run:

```bash
DATABASE_URL=postgres://postgres:postgres@localhost:5432/postgres cargo test -p factstore-postgres
```

The PostgreSQL tests create a fresh schema per test run and exercise the same conformance behavior as the memory store.

## What You Should Understand After This Page

After these commands, you should know:

- the repository already has a shared runtime contract
- memory, SQLite, and PostgreSQL preserve the same observable append/query/conditional-append behavior
- projection-style updates are implemented as part of the current contract through streams
- a feature slice can stream relevant future facts with `stream_to(&EventQuery, handle)`
- the SQLite bank example uses a real stream-driven read-side slice while keeping command and query naming separate
- all three stores implement durable replay/catch-up through `stream_*_durable(...)`
- shared reusable durable-stream conformance exists in `factstore-conformance`
- the current scope is still intentionally narrow and focused on core behavior
- there are two direct runnable memory-store examples you can build on next
