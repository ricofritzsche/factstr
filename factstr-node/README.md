# @factstr/factstr-node

`@factstr/factstr-node` is the Node and TypeScript package for FACTSTR.

It currently provides a memory-backed FACTSTR store for Node and TypeScript with a small, explicit API:

- `FactstrMemoryStore`
- `append`
- `query`
- `appendIf`

## Current Scope

Current scope is intentionally narrow:

- memory-backed only
- explicit append, query, and conditional-append behavior
- TypeScript-friendly package surface

Not included yet:

- SQLite or PostgreSQL support
- streams
- durable streams
- transport behavior

## Install

```bash
npm install @factstr/factstr-node
```

## Supported Platforms

Current prebuilt targets:

- `darwin-arm64`
- `darwin-x64`
- `linux-x64-gnu`
- `win32-x64-msvc`

## Quick Start

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

console.log(result.event_records[0]?.occurred_at);
console.log(result.event_records[0]?.payload);
```

## Conditional Append

`appendIf` checks whether the relevant query-defined context has changed before appending new facts.

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

## BigInt

Sequence and context values are exposed as `bigint` so FACTSTR's Rust `u64` meanings stay lossless in TypeScript.

`occurred_at` is exposed as an RFC 3339 string on each returned `EventRecord`.

## Docs and Source

- [https://factstr.com](https://factstr.com)
- [https://github.com/ricofritzsche/factstr](https://github.com/ricofritzsche/factstr)
