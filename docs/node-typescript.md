# Node and TypeScript

`@factstr/factstr-node` is the current published Node and TypeScript package for FACTSTR.

## Current Scope

Current scope is intentionally narrow:

- memory-backed only
- exposes `FactstrMemoryStore`
- supports:
  - `append`
  - `query`
  - `appendIf`

Not included yet:

- SQLite support
- PostgreSQL support
- streams
- durable streams
- transport behavior

## Install

```bash
npm install @factstr/factstr-node
```

## First Example

```ts
import {
  type EventQuery,
  type NewEvent,
  FactstrMemoryStore,
} from '@factstr/factstr-node';

const store = new FactstrMemoryStore();

const event: NewEvent = {
  event_type: 'item-added',
  payload: { sku: 'ABC-123', quantity: 1 },
};

store.append([event]);

const query: EventQuery = {
  filters: [
    {
      event_types: ['item-added'],
    },
  ],
};

const result = store.query(query);

console.log(result.event_records[0]?.payload);
console.log(result.last_returned_sequence_number);
console.log(result.current_context_version);
```

This example keeps the current public package shape explicit:

- events use `event_type` and `payload`
- `query(...)` returns `event_records`
- `last_returned_sequence_number` and `current_context_version` stay distinct

## Conditional Append Example

```ts
import {
  type AppendIfResult,
  type EventQuery,
  type NewEvent,
  FactstrMemoryStore,
} from '@factstr/factstr-node';

const store = new FactstrMemoryStore();

const contextQuery: EventQuery = {
  filters: [
    {
      event_types: ['item-added'],
    },
  ],
};

const context = store.query(contextQuery);

const nextEvent: NewEvent = {
  event_type: 'item-added',
  payload: { sku: 'ABC-123', quantity: 1 },
};

const outcome: AppendIfResult = store.appendIf(
  [nextEvent],
  contextQuery,
  context.current_context_version,
);

if (outcome.conflict) {
  console.log('conditional append conflict', outcome.conflict);
} else {
  console.log('append succeeded', outcome.append_result);
}
```

`appendIf(...)` checks whether the query-defined context changed before the new facts are committed.

## BigInt

Sequence and context values use `bigint` so Rust `u64` meanings stay lossless in TypeScript.

## Current Boundary

The Node package is currently the first language adapter for FACTSTR.

It does not yet expose the full Rust/store surface. The current package is a memory-backed package entry path above the Rust core, not a full replacement for the Rust workspace.
