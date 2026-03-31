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

## Account Projection

Source:

- [`examples/account-projection/src/main.rs`](https://github.com/ricofritzsche/factstore/blob/main/examples/account-projection/src/main.rs)

Run it:

```bash
cargo run --manifest-path examples/account-projection/Cargo.toml
```

What it proves:

- define a feature-local read model
- subscribe with `subscribe_to(&EventQuery, handle)` for only the relevant facts
- append one committed batch where some facts match and others do not
- update the read model from the one delivered committed batch
- keep unrelated facts out of that projection by contract

This is the main feature-slice story in the current repository: the feature owns its own query model, receives only relevant future facts, and updates that model from committed batches.

These examples stay on the memory store because it is the fastest way to understand the current contract without database setup.
