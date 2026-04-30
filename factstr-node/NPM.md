# @factstr/factstr-node

`@factstr/factstr-node` is the Node and TypeScript package for FACTSTR.

## Current Scope

- memory-backed only
- exposes `FactstrMemoryStore`
- supports `append`, `query`, and `appendIf`

It does not yet expose:

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
import { FactstrMemoryStore } from '@factstr/factstr-node';

const store = new FactstrMemoryStore();

await store.append([
  {
    context: 'cart/1',
    event_type: 'item-added',
    payload: { sku: 'ABC-123', quantity: 1 },
  },
]);

const result = await store.query({
  filters: [
    {
      event_types: ['item-added'],
    },
  ],
});

console.log(result.events[0]?.payload);
```

## BigInt

Sequence and context values are exposed as `bigint` so FACTSTR's Rust `u64` meanings stay lossless in TypeScript.

## More Information

Repository and package homepage:

- [https://factstr.com](https://factstr.com)
- [https://github.com/ricofritzsche/factstr](https://github.com/ricofritzsche/factstr)
