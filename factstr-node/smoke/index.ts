import {
  type AppendIfResult,
  type EventQuery,
  type NewEvent,
  FactstrMemoryStore,
} from '@factstr/factstr-node';

function assert(condition: unknown, message: string): asserts condition {
  if (!condition) {
    throw new Error(message);
  }
}

const store = new FactstrMemoryStore();

const openedAccount: NewEvent = {
  event_type: 'account-opened',
  payload: { accountId: 'a-1', owner: 'Rico' },
};

const appendResult = store.append([
  openedAccount,
]);

const accountQuery: EventQuery = {
  filters: [
    {
      event_types: ['account-opened'],
      payload_predicates: [{ accountId: 'a-1' }],
    },
  ],
};

const queryResult = store.query(accountQuery);

assert(queryResult.event_records.length === 1, 'expected one queried event');
assert(queryResult.event_records[0].sequence_number === 1n, 'expected queried sequence number 1n');
assert(typeof queryResult.event_records[0].occurred_at === 'string', 'expected occurred_at string');
assert(queryResult.event_records[0].occurred_at.length > 0, 'expected non-empty occurred_at');
assert(queryResult.last_returned_sequence_number === 1n, 'expected last returned sequence number 1n');
assert(queryResult.current_context_version === 1n, 'expected current context version 1n');

const depositEvent: NewEvent = {
  event_type: 'money-deposited',
  payload: { accountId: 'a-1', amount: 25 },
};

const conflictResult: AppendIfResult = store.appendIf(
  [depositEvent],
  accountQuery,
  0n,
);
assert(appendResult.first_sequence_number === 1n, 'expected first sequence number 1n');
assert(appendResult.last_sequence_number === 1n, 'expected last sequence number 1n');
assert(appendResult.committed_count === 1n, 'expected committed count 1n');

assert(conflictResult.append_result == null, 'expected no append result on conflict');
assert(conflictResult.conflict != null, 'expected explicit conflict result');
assert(
  conflictResult.conflict.expected_context_version === 0n,
  'expected conflict expected_context_version 0n',
);
assert(
  conflictResult.conflict.actual_context_version === 1n,
  'expected conflict actual_context_version 1n',
);

console.log('factstr-node TypeScript smoke test passed');
