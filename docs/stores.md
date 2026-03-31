# Stores

FACTSTR currently includes two store implementations.

## Memory Store

The memory store is the simplest runtime implementation and the semantic reference for the repository.

It is useful for:

- local development
- tests
- direct reasoning about the contract

It implements:

- append
- query
- conditional append
- live subscriptions

Operationally, it keeps all state in memory and does not persist across process restarts.

## PostgreSQL Store

The PostgreSQL store implements the same shared contract on top of PostgreSQL using SQLx.

It is useful for:

- teams that already operate PostgreSQL
- persistence with familiar database tooling
- validating the shared contract against a real database-backed implementation

It implements:

- append
- query
- conditional append
- live subscriptions

Operationally, it differs from the memory store by:

- persisting committed events in PostgreSQL
- using SQL transactions and indexes
- using an internal worker thread so the synchronous store API remains safe to call from inside a running Tokio runtime

## Shared Semantics Across Stores

Even though the mechanics differ, the intended observable behavior remains shared:

- append/query semantics
- conditional append semantics
- sequence allocation guarantees
- explicit query result meanings
- live subscription behavior

## Not Implemented Yet

An embedded persistent store is still future work. It is part of the project direction, but it is not implemented in the current repository state.
