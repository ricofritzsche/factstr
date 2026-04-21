# Project Brief

## Working Idea

Build a Rust event store focused on **facts, context, stable behavior, and feature-owned query models**.

The project preserves the useful semantics of the existing TypeScript event store while moving to a more explicit, more direct, and more durable Rust implementation.

This is **not** a direct port.

FACTSTR treats events as immutable facts in one global chronological log. Consistency is enforced per **command context**, not per aggregate or stream identity. Different storage implementations may exist, but they must preserve the same observable semantics.

The product is not only an append-only log.  
It is also a practical way to start with the **first real feature**, append the **first fact**, and grow a system **feature by feature** without needing a large predefined structure first.

## Product Problem

Many event-sourced systems become harder to use than they need to be.

The difficulty often begins with structure:

- consistency tied to predefined aggregate or stream boundaries
- decisions forced into one entity-shaped owner
- projections treated as secondary infrastructure
- too much architectural setup before the first useful feature exists
- a gap between how features are built and how the event model is explained

That often leads teams to spend too much time deciding:

- what the aggregate is
- which stream owns a decision
- where a rule belongs
- how a projection should be updated
- how to start without committing to too much structure too early

FACTSTR is meant to reduce that burden.

It should let a feature:

- append facts
- read the facts it needs
- check its relevant context safely
- update its own query model from committed batches
- continue from there without introducing a larger framework first

## Purpose

The goal is to provide a Rust event store that is:

- easy to integrate
- simple to start with
- stable under load
- predictable in concurrency behavior
- fast for append and query operations
- usable without external infrastructure by default
- extensible through multiple store implementations
- simple enough for agents and humans to reason about locally

The default experience should support:

- in-memory usage
- embedded persistent usage without a separate database instance
- optional PostgreSQL-backed usage for teams that want an external database
- direct feature-local query model updates through committed batches
- durable replay and catch-up through stored cursors

## Core Philosophy

This project is based on the following principles.

### Facts first

Events are immutable facts. They do not belong to an aggregate in a hard structural sense. They exist in one append-only log.

### Context defines consistency

Consistency is not defined by aggregate identity. It is defined by the facts relevant to a decision. A command checks the version of its context and appends only if that context has not changed.

### Start with the first feature

The system should support beginning with one useful feature instead of a large predefined architecture. A feature should be able to append facts, read relevant facts, and keep its own query model current without requiring a broad framework first.

### Feature-owned query models

A query model belongs to the feature that needs it. It is not a separate architectural world. It is one part of the feature’s own runtime behavior.

### Streams are the update substrate

Committed batches are the delivery surface behind query-model updates. Durable streams are the replay and catch-up mechanism behind feature-owned query models.

### One shared semantic contract, multiple stores

The product includes a shared semantic contract across store implementations. Memory, embedded persistent, and PostgreSQL stores may differ internally, but they must preserve the same observable behavior.

This is an architectural property, not the main reason users should care.

### Explicit over implicit

The system should make important meanings explicit. In particular, it should distinguish:

- the last sequence returned by a read
- the current version of the full conflict context

It should also keep durable cursor behavior explicit and separate from query-model state.

### Direct durable core

The engine should begin as a compact, understandable core rather than a distributed system. Single-node strength comes before network complexity.

### Agentic-coding friendly

The repository must be easy for coding agents to extend safely. That means:

- clear contracts
- explicit ownership
- local reasoning
- precise names
- no speculative abstraction layers
- no generic OOP-shaped architecture

## Project Goals

### Functional goals

The project must support:

- querying facts by expressive filter semantics
- appending facts unconditionally
- appending facts conditionally against a context version
- global monotonically increasing sequence numbers
- ordered reads by ascending sequence number
- `stream_all`
- `stream_to`
- `stream_all_durable`
- `stream_to_durable`
- multiple stream handlers observing the same committed facts

The project should also grow toward:

- feature-owned query-model runtimes built on top of committed streams
- persisted query-model state paired with persisted durable cursors
- query-model modes for continuous, on-demand, and inline use
- query-model inspection and rebuild support

### Semantic goals

The Rust implementation must preserve these core semantics:

- append-only immutable event records
- global monotonic sequencing
- consecutive sequence numbers for a committed batch
- query results ordered by ascending sequence number
- `minSequenceNumber` as incremental read cursor only
- context-scoped optimistic locking
- post-commit stream delivery
- stream handler failure does not roll back the append
- stream handler isolation from one another
- committed stream delivery ordered by committed sequence order
- one committed append batch delivered as one committed batch unless the contract is explicitly changed later
- durable replay starts strictly after the stored cursor
- replay/live transition has no duplicates or gaps
- durable cursors do not advance past undelivered committed facts

### Query-model goals

The query-model direction should remain aligned with feature ownership, not with a separate read-side architecture.

The system should grow toward a query-model runtime where:

- a feature defines the query model it owns
- a feature defines the fact query that updates that model
- pure query-model logic stays separate from runtime mechanics
- durable replay is driven by stored cursors
- query-model state and query-model cursor are paired explicitly
- query models can exist in different runtime modes without changing the core store contract

Planned query-model modes:

#### 1. Continuous durable query models

- updated continuously from durable committed-batch streams
- persist cursor and state
- catch up after restart
- suited for always-needed query models

#### 2. On-demand persistent query models

- persist cursor and state
- not always active
- catch up before serving if behind
- suited for query models that matter but do not need to run constantly

#### 3. Inline ephemeral query models

- built directly from facts when needed
- no persisted runtime state
- no long-running runtime presence
- suited for local, temporary, or rare query use cases

### Operational goals

The persistent embedded implementation should support:

- explicit format versioning
- checksums where appropriate
- crash recovery
- clear fsync modes
- corruption handling rules
- stable recovery after restart

The broader runtime direction should grow toward:

- query-model state persistence
- query-model cursor persistence
- query-model rebuild/reset operations
- query-model metadata inspection
- bounded query-model runtime behavior

## Non-Goals

At the start, this project does **not** aim to provide:

- a distributed event store
- cluster coordination
- multi-node consensus
- a generic document database
- a framework for rich domain models
- aggregate-centric stream ownership
- a mandatory CQRS read/write split
- a mandatory long-running runtime model for every query model
- speculative adapters before the core is stable

The project is also **not** trying to define every query model as a permanently running background component. Different runtime strategies may exist, but they are not the core model.

## Initial Store Strategy

The first version of the project should support a shared contract with multiple implementations.

### 1. Memory store

Purpose:

- behavioral reference
- tests
- local development
- fastest way to verify semantics

### 2. Embedded persistent store

Purpose:

- default persistent option without a separate database instance
- strong local and single-node production cases
- append-only durable log with indexed reads
- durable replay and catch-up without external infrastructure

The exact implementation may evolve, but it should remain embedded and infrastructure-light.

### 3. PostgreSQL store

Purpose:

- support teams that already operate PostgreSQL
- use SQL transactions and indexes to implement the same contract
- preserve the same observable semantics as the other stores

## Query-Model Runtime Direction

The event store core already provides the substrate:

- committed facts
- ordered reads
- post-commit streams
- durable replay from stored cursors

The next layer above that should be a **query-model runtime for feature-owned query models**.

This runtime should not be treated as a separate “read side” architecture.  
It should be treated as feature-local execution built on top of the fact store.

The query-model runtime should aim to make these concepts explicit:

- query-model identity
- source query
- current cursor
- current state
- query-model mode
- last successful update metadata

A query model should be understandable as:

- a named query model
- updated from committed batches
- resumed from a stored cursor where appropriate
- inspectable and rebuildable as an operational unit

## Proposed Public Contract Shape

The shared contract should express semantics, not implementation details.

Core operations:

- append
- query
- append against expected context version
- `stream_all`
- `stream_to`
- `stream_all_durable`
- `stream_to_durable`

The contract should explicitly separate:

- **last returned sequence**
- **context version**

This removes ambiguity and makes incremental reads, optimistic locking, and durable replay easier to reason about.

The shared stream contract should make these observable semantics explicit:

- notifications happen only after successful persistence
- delivery preserves committed sequence order
- one committed append batch is delivered as one committed batch
- multiple stream handlers may observe the same committed batch
- handler failure does not roll back a successful append
- durable replay starts strictly after the stored cursor
- replay/live transition has no duplicates or gaps
- durable cursors do not advance past undelivered committed facts
- stream internals remain store-local unless a true cross-store semantic requires a shared type

A later query-model runtime surface may be added above this contract, but it should remain separate from the store core unless a true shared semantic requires otherwise.

## Architectural Direction

The Rust codebase should be structured around a shared semantic contract and separate store implementations.

Examples of major parts:

- contract
- memory store
- embedded persistent store
- postgres store
- stream support with store-local delivery internals
- query-model runtime for feature-owned query models
- optional transport adapters later

The repository should not be structured around generic horizontal layers such as services, repositories, managers, helpers, or shared business-logic buckets.

The Rust project preserves the useful stream delivery semantics of the TypeScript event store, but it is not meant to copy the TypeScript runtime design or internal delivery structure.

Query-model runtime work should preserve these boundaries:

- pure query-model logic should remain small and explicit
- runtime mechanics should stay outside pure query-model logic
- CQRS terminology should not drive the design
- runtime strategy should not leak into the core store contract

## Delivery Plan

### Phase 1

Build the semantic foundation:

- shared contract
- memory store
- test suite proving preserved semantics
- explicit query result model with context version
- explicit stream contract so later work does not lose post-commit delivery semantics

### Phase 2

Build the first persistent embedded store:

- append-only durable log
- deterministic sequence allocation
- indexed reads
- streams
- recovery on restart

### Phase 3

Build PostgreSQL as a first-class store:

- same contract
- same semantic tests
- same conditional append behavior
- same observable read behavior
- same durable-stream replay semantics

### Phase 4

Build the first query-model runtime layer:

- pure query-model contract
- explicit pairing of query-model state and durable cursor
- continuous durable query-model runner
- inline ephemeral query-model support
- basic query-model persistence model

### Phase 5

Extend query-model runtime behavior:

- on-demand persistent query models
- query-model metadata inspection
- reset/rebuild/delete operations
- clearer operational query-model boundaries

### Phase 6

Add stronger operational features:

- snapshots or checkpoints where justified
- faster recovery
- metrics and tracing
- optional transport adapters
- later historical query-model inspection if the runtime model proves it useful

Historical query-model inspection at arbitrary points in time is explicitly **later** work, not an initial milestone.

## Definition of Success

The project is successful when it provides:

1. a Rust core that is explicit, direct, and easy to reason about
2. a system that can start with the first real feature immediately
3. stable context-scoped optimistic locking
4. a default embedded persistent option without external infrastructure
5. PostgreSQL as a first-class optional store
6. direct feature-owned query-model updates through committed batches
7. durable replay and catch-up through stored cursors
8. a clear path toward a first-class query-model runtime
9. a repository structure that agents can extend without drifting into generic architecture

## Intended Audience

This project is for developers and teams who want:

- event sourcing based on facts rather than aggregates
- explicit conflict boundaries
- predictable append behavior
- embedded or database-backed deployment choices
- a simple way to start without platform overhead
- feature-owned query models updated from committed facts
- durable replay and catch-up without inventing a separate read-side architecture
- a more direct model than conventional enterprise event store designs
