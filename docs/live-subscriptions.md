# Live Subscriptions

FACTSTR currently implements first-class live subscriptions in the shared contract.

## Current Behavior

- subscriptions are live only
- subscriptions do not replay historical events
- notifications happen only after a successful commit
- each committed append batch is delivered as one batch
- delivery order follows committed global sequence order
- failed conditional append emits nothing
- multiple subscribers can observe the same committed batches
- dropping a subscription stops future delivery for that subscriber

Each delivered item is a committed batch:

- `Vec<EventRecord>`

This keeps the delivered shape aligned with the append shape instead of splitting one committed append into arbitrary fragments.

For the smallest concrete code path, see the live subscription example in [Examples](examples.md).

## Current Limitations

The current implementation does not provide:

- durable subscriber cursors
- replay
- snapshots for subscriber catch-up
- transport or pub/sub adapters

This means subscriptions are for future live delivery only. Durable subscriber state is later work.

## Implementation Boundary

The shared contract defines the observable subscription behavior.

Notifier mechanics remain local to each store implementation. The contract does not currently expose a shared notifier abstraction.
