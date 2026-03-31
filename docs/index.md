# FACTSTR

FACTSTR is a Rust event store built around facts, query-defined consistency context, and multiple store implementations behind one shared contract.

The current implemented scope includes the shared contract, the memory store, the PostgreSQL store, append/query/conditional append behavior, and live subscriptions. It does not yet include embedded persistence, durable subscriber cursors, replay, or transport adapters.

- [Getting Started](getting-started.md)
- [Core Concepts](core-concepts.md)
- [Live Subscriptions](live-subscriptions.md)
- [Stores](stores.md)
