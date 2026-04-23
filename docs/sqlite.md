# SQLite Store: What It Is For, and When Not to Use It

The SQLite store is the embedded persistent store for FACTSTR.

Its job is not to be the high-throughput store. Its job is to give you a simple, inspectable, correctness-first store that is easy to run locally and easy to verify.

That makes it useful for local development, reproducible tests, small demos, and single-node setups where a file-based store is an advantage.

## Use the SQLite store when the goal is

- local development
- reproducible tests
- end-to-end feature verification
- direct inspection of committed facts
- simple demos
- embedded or single-node scenarios
- low setup and low operational overhead

## What it is good at

The SQLite store is valuable because it keeps persistence easy to understand.

You can:

- start with a real persistent store without extra infrastructure
- inspect the database file directly
- reset test data easily
- copy the database for debugging or verification
- verify append, query, conditional append, and durable replay behavior in a small environment

That makes SQLite a strong store for proving correctness with real persistence.

## What it is not for

Do not use the SQLite store when the goal is:

- high concurrent write throughput
- production-scale load testing
- multi-node deployment
- sustained write-heavy traffic under strong parallelism
- drawing conclusions about large-scale production latency

SQLite is not the store to use when the main question is throughput under contention.

## Practical expectation

If you run concurrent API stress traffic against a SQLite-backed service, the result is useful mainly as a correctness and contention signal.

It tells you:

- whether the feature behaves correctly under load
- whether facts are committed as expected
- whether duplicate and conflict handling stay correct
- how the current command and store access shape behaves under contention

It does not prove that the same latency or throughput characteristics will hold for a larger production deployment.

## Why this distinction matters

FACTSTR keeps command behavior explicit. Store choice changes how that behavior is persisted and how it performs operationally.

SQLite is the store to choose when simplicity, inspectability, and reproducibility matter most.

It is not the store to choose when you want to evaluate serious concurrent write performance.

## Short version

The SQLite store is the simple, inspectable, correctness-first store for local development, tests, and small demos. It is not the store for high-concurrency throughput or production-scale performance evaluation.