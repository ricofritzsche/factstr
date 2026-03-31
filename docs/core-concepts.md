# Core Concepts

## Facts As Immutable Events

FACTSTR treats events as immutable facts. They are not rewritten in place. New information is recorded by appending new facts.

## One Append-Only Log

Events live in one global append-only log. Sequence numbers are global and monotonically increasing across the whole store.

One committed append batch receives one consecutive sequence range.

## Query-Defined Consistency Context

Consistency is defined by the facts relevant to a command, not by a built-in aggregate boundary.

A caller can:

- query a context
- observe its `current_context_version`
- append only if that context still has the expected version

That is what `append_if` enforces.

## Returned Sequence vs Context Version

FACTSTR keeps two meanings explicit in query results:

- `last_returned_sequence_number`
- `current_context_version`

They are not interchangeable.

`last_returned_sequence_number` tells you the last sequence number actually returned by the current read.

`current_context_version` tells you the latest sequence number in the full matching context, even when `min_sequence_number` filters all returned rows out of the result set.

## Conditional Append

`append_if` checks the full conflict context before appending.

Important current behavior:

- the context check ignores `min_sequence_number`
- failed conditional append appends nothing
- failed conditional append does not consume sequence numbers that later successful appends would observe

## Global Sequence Numbers

Sequence numbers are:

- global
- monotonic
- ordered across the whole store

Reads return events in ascending sequence order, and live subscription delivery follows that same committed order.
