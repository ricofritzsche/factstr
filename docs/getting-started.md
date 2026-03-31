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

This verifies the shared contract crate, the memory store, the PostgreSQL store, and the conformance test crate all compile together.

## Start With The Memory Store

Run the in-memory store tests first:

```bash
cargo test -p factstore-memory
```

This is the simplest way to see the current semantic contract in action:

- append
- query
- conditional append
- projection updates through live subscriptions

If you want the first direct code path after that, run the basic memory example:

```bash
cargo run --manifest-path examples/basic-memory/Cargo.toml
```

See [Examples](examples.md) for the source and what it proves.

For the common feature-slice path, run the account projection example next:

```bash
cargo run --manifest-path examples/account-projection/Cargo.toml
```

That example shows a feature slice owning a read model, subscribing only to the facts relevant to that model, and updating it from committed batches.

## Run The PostgreSQL Store Tests

Set `DATABASE_URL` to a PostgreSQL database where the configured user can create schemas, then run:

```bash
DATABASE_URL=postgres://postgres:postgres@localhost:5432/postgres cargo test -p factstore-postgres
```

The PostgreSQL tests create a fresh schema per test run and exercise the same conformance behavior as the memory store.

## What You Should Understand After This Page

After these commands, you should know:

- the repository already has a shared runtime contract
- memory and PostgreSQL preserve the same observable append/query/conditional-append behavior
- projection-style updates are implemented as part of the current contract
- a feature slice can subscribe to relevant future facts with `subscribe_to(&EventQuery, handle)`
- live subscriptions are the mechanism behind those projection updates
- the current scope is still intentionally narrow and focused on core behavior
- there are two direct runnable memory-store examples you can build on next
