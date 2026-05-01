export interface NewEvent {
  event_type: string;
  payload: unknown;
}

export interface EventFilter {
  event_types?: string[] | null;
  payload_predicates?: unknown[] | null;
}

export interface EventQuery {
  filters?: EventFilter[] | null;
  min_sequence_number?: bigint | null;
}

export interface EventRecord {
  sequence_number: bigint;
  occurred_at: string;
  event_type: string;
  payload: unknown;
}

export interface QueryResult {
  event_records: EventRecord[];
  last_returned_sequence_number?: bigint | null;
  current_context_version?: bigint | null;
}

export interface AppendResult {
  first_sequence_number: bigint;
  last_sequence_number: bigint;
  committed_count: bigint;
}

export interface ConditionalAppendConflict {
  expected_context_version?: bigint | null;
  actual_context_version?: bigint | null;
}

export interface AppendIfResult {
  append_result?: AppendResult | null;
  conflict?: ConditionalAppendConflict | null;
}

export declare class FactstrMemoryStore {
  constructor();
  append(events: NewEvent[]): AppendResult;
  query(query: EventQuery): QueryResult;
  appendIf(
    events: NewEvent[],
    query: EventQuery,
    expectedContextVersion?: bigint | null,
  ): AppendIfResult;
}
