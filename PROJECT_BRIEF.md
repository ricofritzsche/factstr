# Project Brief

## Working Idea

Build a Rust event store focused on **facts, context, and stable behavior**.

The project preserves the useful semantics of the existing TypeScript eventstore while replacing its runtime model with a more explicit, more direct, and more durable Rust implementation.

This is **not** a direct port.

The project treats events as immutable facts in one global chronological log. Consistency is enforced per **command context**, not per aggregate or stream identity. Different storage implementations may exist, but they must expose the same semantic contract.

## Purpose

The goal is to provide a Rust event store that is:

* stable under load
* predictable in concurrency behavior
* fast for append and query operations
* usable without external infrastructure by default
* extensible through multiple store implementations
* simple enough for agents and humans to reason about locally

The default experience should support:

* in-memory usage
* embedded persistent usage without a separate database instance
* optional PostgreSQL-backed usage for teams that want an external database

## Core Philosophy

This project is based on the following principles:

### Facts first

Events are immutable facts. They do not belong to an aggregate in a hard structural sense. They exist in one append-only log.

### Context defines consistency

Consistency is not defined by aggregate identity. It is defined by the facts relevant to a decision. A command checks the version of its context and appends only if that context has not changed.

### One shared semantic contract, multiple stores

The product is the **contract**, not the storage backend. Memory, file-based, and PostgreSQL stores may differ internally, but they must preserve the same user-visible behavior.

### Explicit over implicit

The system should make important meanings explicit. In particular, it should distinguish:

* the last sequence returned by a read
* the current version of the full conflict context

### Direct durable core

The engine should begin as a compact, understandable core rather than a distributed system. Single-node strength comes before network complexity.

### Agentic coding friendly

The repository must be easy for coding agents to extend safely. That means:

* clear contracts
* explicit ownership
* local reasoning
* precise names
* no speculative abstraction layers
* no generic OOP-shaped architecture

## Project Goals

### Functional goals

The project must support:

* querying events by expressive filter semantics
* appending events unconditionally
* appending events conditionally against a context version
* global monotonically increasing sequence numbers
* ordered reads by ascending sequence number
* `stream_all`
* `stream_to`
* `stream_all_durable`
* `stream_to_durable`
* multiple stream handlers observing the same committed facts

### Semantic goals

The Rust implementation must preserve these core semantics from the TypeScript implementation:

* append-only immutable event records
* global monotonic sequencing
* consecutive sequence numbers for a committed batch
* query results ordered by ascending sequence number
* `minSequenceNumber` as incremental read cursor only
* context-scoped optimistic locking
* post-commit stream delivery
* stream handler failure does not roll back the append
* stream handler isolation from one another
* committed stream delivery ordered by committed sequence order
* one committed append batch delivered as one committed batch unless the contract is explicitly changed later
* durable replay starts strictly after the stored cursor
* replay/live transition has no duplicates or gaps
* durable cursors do not advance past undelivered committed facts

### Operational goals

The persistent embedded implementation should support:

* explicit log format versioning
* checksums
* crash recovery
* clear fsync modes
* bounded stream handler behavior
* corruption handling rules
* stable recovery after restart

## Non-Goals

At the start, this project does **not** aim to provide:

* a distributed event store
* cluster coordination
* multi-node consensus
* a generic document database
* a framework for rich domain models
* aggregate-centric stream ownership
* speculative adapters before the core is stable

## Initial Store Strategy

The first version of the project should support a shared contract with multiple implementations.

### 1. Memory store

Purpose:

* behavioral reference
* tests
* local development
* fastest way to verify semantics

### 2. Embedded persistent store

Purpose:

* default persistent option without a separate database instance
* strong local and single-node production cases
* append-only durable log with indexed reads

The exact implementation may evolve, but it should remain embedded and infrastructure-light.

### 3. PostgreSQL store

Purpose:

* preserve the proven TypeScript model
* support teams that already operate PostgreSQL
* use SQL transactions and indexes to implement the same contract

## Proposed Public Contract Shape

The shared contract should express semantics, not implementation details.

Core operations:

* append
* query
* append against expected context version
* `stream_all`
* `stream_to`
* `stream_all_durable`
* `stream_to_durable`

The contract should explicitly separate:

* **last returned sequence**
* **context version**

This removes ambiguity and makes incremental reads, optimistic locking, and durable replay easier to reason about.

The shared stream contract should make these observable semantics explicit:

* notifications happen only after successful persistence
* delivery preserves committed sequence order
* one committed append batch is delivered as one committed batch
* multiple stream handlers may observe the same committed batch
* handler failure does not roll back a successful append
* durable replay starts strictly after the stored cursor
* replay/live transition has no duplicates or gaps
* durable cursors do not advance past undelivered committed facts
* stream internals remain store-local unless a true cross-store semantic requires a shared type

## Architectural Direction

The Rust codebase should likely be structured around a shared contract and separate store implementations.

Examples of major parts:

* contract
* memory store
* file or embedded store
* postgres store
* stream support with store-local delivery internals
* optional transport adapters later

The repository should not be structured around generic horizontal layers such as services, repositories, managers, helpers, or shared business logic buckets.

The Rust project preserves the useful stream delivery semantics of the TypeScript eventstore, but it is not meant to copy the TypeScript runtime design or internal delivery structure.

## Delivery Plan

### Phase 1

Build the semantic foundation:

* shared contract
* memory store
* test suite proving preserved semantics
* explicit query result model with context version
* explicit stream contract so later work does not lose post-commit delivery semantics

### Phase 2

Build the first persistent embedded store:

* append-only durable log
* deterministic sequence allocation
* indexed reads
* streams
* recovery on restart

### Phase 3

Build PostgreSQL as a first-class store:

* same contract
* same semantic tests
* same conditional append behavior
* same observable read behavior
* same durable-stream replay semantics

### Phase 4

Add stronger operational features:

* snapshots
* faster recovery
* metrics and tracing
* optional transport adapters

## Definition of Success

The project is successful when it provides:

1. a Rust core that is explicit, direct, and easy to reason about
2. one shared semantic contract across store implementations
3. stable context-scoped optimistic locking
4. a default embedded persistent option without external infrastructure
5. PostgreSQL as a first-class optional store
6. a repository structure that agents can extend without drifting into generic architecture

## Intended Audience

This project is for developers and teams who want:

* event sourcing based on facts rather than aggregates
* explicit conflict boundaries
* predictable append behavior
* embedded or database-backed deployment choices
* a more direct model than conventional enterprise event store designs
