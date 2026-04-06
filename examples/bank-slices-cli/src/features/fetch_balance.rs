//! Fetch-balance feature slice.
//!
//! Decide:
//! - no command decision is owned here
//! - this slice is read-side only
//!
//! Read:
//! - build an initial balance snapshot from persisted facts
//! - then keep a local balance read model current from committed stream batches
//! - preserve committed sequence order while folding those facts into balances
//!
//! FACTSTR interface usage:
//! - `stream_to(...)` is the important long-running interface here
//! - `query(...)` is used once during startup to bootstrap the initial state
//! - `BalanceQuery` reads the local projection state for CLI output
//! - `BalanceProjectionRuntime::current_balance(...)` is reused by withdraw
//!   and transfer
//! - `BalanceProjectionRuntime::wait_until_applied(...)` lets write commands
//!   wait until the projection has observed their committed batch

use crate::features::open_account::ensure_account_exists;
use factstore::{EventQuery, EventRecord, EventStore, EventStream, StreamHandlerError};
use std::collections::BTreeMap;
use std::error::Error;
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

pub struct BalanceQuery {
    account_id: String,
}

pub struct BalanceProjectionRuntime {
    state: Arc<(Mutex<BalanceProjectionState>, Condvar)>,
    _stream: EventStream,
}

struct BalanceView {
    current_balance: i64,
    observed_facts: usize,
}

#[derive(Default)]
struct BalanceProjectionState {
    balances_by_account_id: BTreeMap<String, i64>,
    observed_fact_count_by_account_id: BTreeMap<String, usize>,
    last_applied_sequence_number: u64,
    bootstrapped: bool,
    buffered_batches: Vec<Vec<EventRecord>>,
}

impl BalanceQuery {
    pub fn from_args(args: &[String]) -> Result<Self, String> {
        let account_id = required_flag(args, "--account")?;
        Ok(Self { account_id })
    }

    pub fn run(
        self,
        store: &impl EventStore,
        balance_projection: &BalanceProjectionRuntime,
    ) -> Result<String, Box<dyn Error>> {
        ensure_account_exists(store, &self.account_id)?;
        let balance = balance_projection.view_for_account(&self.account_id)?;

        Ok(format!(
            "account {} balance: {} (from {} facts)",
            self.account_id, balance.current_balance, balance.observed_facts
        ))
    }
}

impl BalanceProjectionRuntime {
    pub fn start(store: &impl EventStore) -> Result<Self, Box<dyn Error>> {
        let state = Arc::new((
            Mutex::new(BalanceProjectionState::default()),
            Condvar::new(),
        ));
        let handler_state = Arc::clone(&state);

        let stream = store.stream_to(
            &projection_query(),
            Arc::new(move |committed_batch| {
                apply_or_buffer_batch(&handler_state, committed_batch)
                    .map_err(|error| StreamHandlerError::new(error.to_string()))
            }),
        )?;

        let snapshot = store.query(&projection_query())?;
        let snapshot_last_sequence_number = snapshot.last_returned_sequence_number.unwrap_or(0);
        finish_bootstrap(
            &state,
            snapshot.event_records,
            snapshot_last_sequence_number,
        )?;

        Ok(Self {
            state,
            _stream: stream,
        })
    }

    pub fn current_balance(&self, account_id: &str) -> Result<i64, Box<dyn Error>> {
        let state = self
            .state
            .0
            .lock()
            .map_err(|_| "balance projection lock should succeed")?;

        Ok(state
            .balances_by_account_id
            .get(account_id)
            .copied()
            .unwrap_or(0))
    }

    pub fn wait_until_applied(&self, sequence_number: u64) -> Result<(), Box<dyn Error>> {
        let deadline = Instant::now() + Duration::from_secs(1);
        let (lock, condvar) = &*self.state;
        let mut state = lock
            .lock()
            .map_err(|_| "balance projection lock should succeed")?;

        while state.last_applied_sequence_number < sequence_number {
            let now = Instant::now();
            if now >= deadline {
                return Err(format!(
                    "balance projection did not observe committed sequence {} in time",
                    sequence_number
                )
                .into());
            }

            let timeout = deadline.saturating_duration_since(now);
            let (next_state, wait_result) = condvar
                .wait_timeout(state, timeout)
                .map_err(|_| "balance projection wait should succeed")?;
            state = next_state;

            if wait_result.timed_out() && state.last_applied_sequence_number < sequence_number {
                return Err(format!(
                    "balance projection did not observe committed sequence {} in time",
                    sequence_number
                )
                .into());
            }
        }

        Ok(())
    }

    fn view_for_account(&self, account_id: &str) -> Result<BalanceView, Box<dyn Error>> {
        let state = self
            .state
            .0
            .lock()
            .map_err(|_| "balance projection lock should succeed")?;

        Ok(BalanceView {
            current_balance: state
                .balances_by_account_id
                .get(account_id)
                .copied()
                .unwrap_or(0),
            observed_facts: state
                .observed_fact_count_by_account_id
                .get(account_id)
                .copied()
                .unwrap_or(0),
        })
    }
}

fn projection_query() -> EventQuery {
    EventQuery::for_event_types([
        "money-deposited",
        "money-withdrawn",
        "money-transferred-out",
        "money-transferred-in",
    ])
}

fn apply_or_buffer_batch(
    state: &Arc<(Mutex<BalanceProjectionState>, Condvar)>,
    committed_batch: Vec<EventRecord>,
) -> Result<(), Box<dyn Error>> {
    let (lock, condvar) = &**state;
    let mut projection_state = lock
        .lock()
        .map_err(|_| "balance projection lock should succeed")?;

    if !projection_state.bootstrapped {
        projection_state.buffered_batches.push(committed_batch);
        return Ok(());
    }

    apply_batch(&mut projection_state, &committed_batch)?;
    condvar.notify_all();
    Ok(())
}

fn finish_bootstrap(
    state: &Arc<(Mutex<BalanceProjectionState>, Condvar)>,
    snapshot_event_records: Vec<EventRecord>,
    snapshot_last_sequence_number: u64,
) -> Result<(), Box<dyn Error>> {
    let (lock, condvar) = &**state;
    let mut projection_state = lock
        .lock()
        .map_err(|_| "balance projection lock should succeed")?;

    projection_state.balances_by_account_id.clear();
    projection_state.observed_fact_count_by_account_id.clear();
    projection_state.last_applied_sequence_number = 0;
    apply_batch(&mut projection_state, &snapshot_event_records)?;
    projection_state.last_applied_sequence_number = snapshot_last_sequence_number;

    let buffered_batches = std::mem::take(&mut projection_state.buffered_batches);
    for batch in buffered_batches {
        let filtered_batch: Vec<EventRecord> = batch
            .into_iter()
            .filter(|event_record| {
                event_record.sequence_number > projection_state.last_applied_sequence_number
            })
            .collect();

        if filtered_batch.is_empty() {
            continue;
        }

        apply_batch(&mut projection_state, &filtered_batch)?;
    }

    projection_state.bootstrapped = true;
    condvar.notify_all();
    Ok(())
}

fn apply_batch(
    projection_state: &mut BalanceProjectionState,
    event_records: &[EventRecord],
) -> Result<(), Box<dyn Error>> {
    for event_record in event_records {
        let amount = required_i64(&event_record.payload, "amount")?;

        match event_record.event_type.as_str() {
            "money-deposited" => {
                let account_id = required_string(&event_record.payload, "accountId")?;
                *projection_state
                    .balances_by_account_id
                    .entry(account_id.to_owned())
                    .or_insert(0) += amount;
                *projection_state
                    .observed_fact_count_by_account_id
                    .entry(account_id.to_owned())
                    .or_insert(0) += 1;
            }
            "money-withdrawn" => {
                let account_id = required_string(&event_record.payload, "accountId")?;
                *projection_state
                    .balances_by_account_id
                    .entry(account_id.to_owned())
                    .or_insert(0) -= amount;
                *projection_state
                    .observed_fact_count_by_account_id
                    .entry(account_id.to_owned())
                    .or_insert(0) += 1;
            }
            "money-transferred-out" => {
                let account_id = required_string(&event_record.payload, "fromAccountId")?;
                *projection_state
                    .balances_by_account_id
                    .entry(account_id.to_owned())
                    .or_insert(0) -= amount;
                *projection_state
                    .observed_fact_count_by_account_id
                    .entry(account_id.to_owned())
                    .or_insert(0) += 1;
            }
            "money-transferred-in" => {
                let account_id = required_string(&event_record.payload, "toAccountId")?;
                *projection_state
                    .balances_by_account_id
                    .entry(account_id.to_owned())
                    .or_insert(0) += amount;
                *projection_state
                    .observed_fact_count_by_account_id
                    .entry(account_id.to_owned())
                    .or_insert(0) += 1;
            }
            other => {
                return Err(format!("balance projection saw unexpected event type {other}").into());
            }
        }
        projection_state.last_applied_sequence_number = event_record.sequence_number;
    }

    Ok(())
}

fn required_flag(args: &[String], flag: &str) -> Result<String, String> {
    let value = args
        .windows(2)
        .find(|pair| pair[0] == flag)
        .map(|pair| pair[1].clone())
        .ok_or_else(|| format!("balance requires {flag}"))?;

    if value.starts_with("--") {
        return Err(format!("balance requires a value after {flag}"));
    }

    Ok(value)
}

fn required_i64(payload: &serde_json::Value, key: &str) -> Result<i64, Box<dyn Error>> {
    payload
        .get(key)
        .and_then(serde_json::Value::as_i64)
        .ok_or_else(|| format!("expected integer {key} in payload {payload}").into())
}

fn required_string<'a>(
    payload: &'a serde_json::Value,
    key: &str,
) -> Result<&'a str, Box<dyn Error>> {
    payload
        .get(key)
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| format!("expected string {key} in payload {payload}").into())
}
