# FACTSTR Bank Slices CLI

This example is a small, production-like SQLite-backed CLI that demonstrates the architectural point behind FACTSTR:

- feature slices are independent
- facts are local to the slice that appends them
- projections stay local to the slice that owns them
- at least one read model is kept current through the real FACTSTR stream API
- there is no shared domain model
- there is no central `Event` enum
- there are no aggregates

The scenario is intentionally familiar: deposit, withdraw, transfer, balance, and history.
An account must be opened first with a unique identifier.

## What This Example Proves

Each feature slice owns its own command input handling, fact shape, and execution logic:

- [`src/features/deposit.rs`](src/features/deposit.rs)
- [`src/features/withdraw.rs`](src/features/withdraw.rs)
- [`src/features/transfer.rs`](src/features/transfer.rs)
- [`src/features/open_account.rs`](src/features/open_account.rs)
- [`src/features/fetch_balance.rs`](src/features/fetch_balance.rs)
- [`src/features/fetch_movement_history.rs`](src/features/fetch_movement_history.rs)

There is no shared account entity and no central event registry.

The projection slices depend only on the facts they need:

- `fetch_balance` owns a real FACTSTR stream and keeps a local balance read model current from committed batches
- `fetch_movement_history` reads the same facts directly and formats an account-specific movement log

That means adding a new feature slice does not require editing a central enum or aggregate.

## Feature Walkthrough

This example uses the words `decide` and `read` on purpose.

- `decide` means the write-side rule check a feature performs before appending new facts
- `read` means the query/projection logic a feature uses to understand existing facts

### `open_account`

File:
- [`src/features/open_account.rs`](src/features/open_account.rs)

Decide:
- an account id may be opened exactly once
- duplicate account ids are rejected

Read:
- query for one `account-opened` fact with the requested `accountId`
- treat absence as "account does not exist yet"

FACTSTR interface usage:
- `append_if(...)` is the important interface here
- it appends `account-opened` only when the account-specific context version is still `None`
- `query(...)` is also used by `ensure_account_exists(...)`, which other slices reuse

Local facts:
- `account-opened`

### `deposit`

File:
- [`src/features/deposit.rs`](src/features/deposit.rs)

Decide:
- the account must already exist
- the amount must be greater than zero

Read:
- no local projection
- only a small existence check through `ensure_account_exists(...)`

FACTSTR interface usage:
- `query(...)` is used indirectly for existence checking
- `append(...)` writes one `money-deposited` fact
- after the append succeeds, the running application waits until the balance projection stream has observed that committed batch

Local facts:
- `money-deposited`

### `withdraw`

File:
- [`src/features/withdraw.rs`](src/features/withdraw.rs)

Decide:
- the account must already exist
- the amount must be greater than zero
- the account must have enough money so the balance stays `>= 0`

Read:
- account existence through `ensure_account_exists(...)`
- current balance through the `fetch_balance` slice

FACTSTR interface usage:
- `query(...)` is used indirectly for the existence check
- `BalanceProjectionRuntime::current_balance(...)` reads the local stream-maintained balance state
- `append(...)` writes one `money-withdrawn` fact only after those reads succeed
- after the append succeeds, the running application waits until the balance projection stream has observed that committed batch

Local facts:
- `money-withdrawn`

### `transfer`

File:
- [`src/features/transfer.rs`](src/features/transfer.rs)

Decide:
- source and destination accounts must both exist
- source and destination must be different ids
- the amount must be greater than zero
- the source account must have enough money so its balance stays `>= 0`

Read:
- source account existence
- destination account existence
- current source balance through the `fetch_balance` slice

FACTSTR interface usage:
- `query(...)` is used indirectly for account checks
- `BalanceProjectionRuntime::current_balance(...)` reads the local stream-maintained balance state
- `append(...)` writes one committed batch with two local facts
- after the append succeeds, the running application waits until the balance projection stream has observed that committed batch

Local facts:
- `money-transferred-out`
- `money-transferred-in`

### `fetch_balance`

File:
- [`src/features/fetch_balance.rs`](src/features/fetch_balance.rs)

Decide:
- no write-side decision is owned here
- this is a read-side feature

Read:
- build an initial snapshot of all balance-relevant facts from the persisted SQLite store
- then keep a local balance read model updated from committed stream batches
- fold those facts in committed sequence order into one current balance per account

FACTSTR interface usage:
- `stream_to(...)` is the important interface here
- the slice registers one stream for:
  - `money-deposited`
  - `money-withdrawn`
  - `money-transferred-out`
  - `money-transferred-in`
- `query(...)` is used once during startup to bootstrap the initial projection state
- `BalanceQuery` uses the local projection state for CLI output
- `BalanceProjectionRuntime::current_balance(...)` is reused by `withdraw` and `transfer`
- `BalanceProjectionRuntime::wait_until_applied(...)` lets write commands wait until the projection has caught up with the batch they just committed

### `fetch_movement_history`

File:
- [`src/features/fetch_movement_history.rs`](src/features/fetch_movement_history.rs)

Decide:
- no write-side decision is owned here
- this is a read-side feature

Read:
- query the same account-relevant facts as the balance projection
- format them into movement-history lines in committed sequence order

FACTSTR interface usage:
- `query(...)` is the important interface here
- the projection reads facts directly and formats them for the CLI

## How The Interfaces Are Used

This example uses only a small part of the FACTSTR interface surface.

- `append(...)` is used by `deposit`, `withdraw`, and `transfer` to write new facts
- `append_if(...)` is used by `open_account` to make account-id uniqueness explicit
- `query(...)` is used by:
  - `open_account` for account existence
  - `withdraw` for account existence
  - `transfer` for account existence
  - `fetch_balance` once at startup to build the initial snapshot
  - `fetch_movement_history` to render the movement log
- `stream_to(...)` is used by `fetch_balance` to keep a local read model updated from committed batches

The architectural point is still the same:

- write-side slices decide locally
- read-side slices project facts locally
- no central event enum is required
- no aggregate root is required

## Why This Example Uses `stream_to(...)`

The example uses `stream_to(...)` for the `fetch_balance` slice, not `stream_to_durable(...)`.

That is intentional.

- the `fetch_balance` state in this example is in-memory and local to the running CLI process
- `stream_to(...)` is enough to demonstrate real stream-driven projection ownership inside one running app
- `stream_to_durable(...)` would be the next operational step once the projection state itself is also persisted

In other words:

- this example now shows real stream-driven read-side ownership
- it does not pretend to have durable projection state when it does not

The SQLite store still matters here because the fact log itself is persisted and restartable.
The stream-backed `fetch_balance` slice rebuilds from the persisted facts at startup, then stays current through live committed batches.

## How To Run

Start the interactive CLI from the repository root:

```bash
cargo run --manifest-path examples/bank-slices-cli/Cargo.toml
```

Then run commands inside the app:

```text
open-account --account A --owner Alice
open-account --account B --owner Bob
deposit --account A --amount 100
withdraw --account A --amount 20
transfer --from A --to B --amount 30
balance --account A
history --account A
exit
```

In the interactive terminal, the up/down keys move through previous and next commands through `rustyline`, like a normal shell history.

By default the CLI stores facts in:

```text
examples/bank-slices-cli/bank-slices.sqlite3
```

You can point at a different database file with `--db`:

```bash
cargo run --manifest-path examples/bank-slices-cli/Cargo.toml -- --db /tmp/bank-slices.sqlite3
```

One-shot commands still work when useful for scripts:

```bash
cargo run --manifest-path examples/bank-slices-cli/Cargo.toml -- --db /tmp/bank-slices.sqlite3 balance --account A
```

All commands except `open-account` check that the referenced account already exists.
`withdraw` and `transfer` also reject any command that would make the source account balance negative.

## Why The Code Is Structured This Way

This example avoids:

- `domain`
- `services`
- `repositories`
- `managers`
- `helpers`
- `core`
- `shared`

The only cross-cutting module is [`src/cli.rs`](src/cli.rs), which is strictly technical dispatch for the command-line entry point.
