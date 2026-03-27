# factstore

A Rust event store built around facts, command-context consistency, and multiple store implementations.

`factstore` preserves the core semantics of the existing TypeScript eventstore while moving toward a smaller, more stable Rust implementation. The goal is not a direct port. The goal is a durable core that is easy to reason about, works well without external infrastructure, and can still support PostgreSQL when needed.

## Status

Early work in progress.

The repository currently focuses on design, semantics, and project structure before the first implementation is considered stable.

## What this project is

`factstore` is an event store with:

* an append-only event log
* global monotonically increasing sequence numbers
* ordered reads
* conditional append based on a query-defined conflict context
* support for multiple store implementations behind one shared contract

The main idea is simple:

events are facts, and consistency is checked against the facts relevant to a command.

## Core idea

This project does not treat aggregates as the primary consistency boundary.

Instead, a command defines its conflict context through a query.
A conditional append succeeds only if that context still has the expected version.

That allows consistency boundaries such as:

* all events for an account
* all events for a business process
* any other context that can be expressed through event type and payload predicates

## Planned store implementations

The shared contract is the product.
Different stores are implementations of that contract.

Planned implementations:

* **memory** for tests and local development
* **embedded persistent store** for production cases without a separate database instance
* **postgres** for teams that want an external database and operational familiarity

## Principles

* **facts first**
  events are immutable facts in one chronological log

* **context defines consistency**
  conflict boundaries come from queries, not aggregate ownership

* **one contract, multiple stores**
  store implementations may differ internally, but observable behavior should stay aligned

* **explicit semantics**
  read cursors and conflict context versions should be modeled clearly

* **small durable core**
  start with a strong single-node design before considering anything more complex

## Current focus

The first milestones are:

1. define the shared Rust contract
2. implement the memory store as the semantic reference
3. implement the first persistent embedded store
4. add PostgreSQL as a first-class store implementation

## What this project is not

At least at the start, `factstore` is not trying to be:

* a distributed event store
* a framework for rich domain models
* an aggregate-centric stream store
* a generic document database
* a large architecture playground

## Development style

This repository is being shaped for clear local reasoning by both humans and coding agents.

That means:

* precise names
* explicit contracts
* no generic technical buckets
* no speculative abstraction layers
* no forced OOP structure

## License

TBD

