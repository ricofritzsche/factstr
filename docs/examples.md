# Examples

These examples are the shortest concrete code paths for the current FACTSTR API.

## Basic Memory Store

Source:

- [`examples/basic-memory/src/main.rs`](https://github.com/ricofritzsche/factstore/blob/main/examples/basic-memory/src/main.rs)

Run it:

```bash
cargo run --manifest-path examples/basic-memory/Cargo.toml
```

What it proves:

- create a `MemoryStore`
- append one committed batch
- query the stored facts
- inspect:
  - returned `EventRecord` values
  - `last_returned_sequence_number`
  - `current_context_version`

## Live Subscriptions

Source:

- [`examples/live-subscriptions/src/main.rs`](https://github.com/ricofritzsche/factstore/blob/main/examples/live-subscriptions/src/main.rs)

Run it:

```bash
cargo run --manifest-path examples/live-subscriptions/Cargo.toml
```

What it proves:

- subscribe before an append
- append one committed batch after subscription starts
- receive that committed batch through `LiveSubscription`
- observe that delivery preserves the committed batch shape

These examples stay on the memory store because it is the fastest way to understand the current contract without database setup.
