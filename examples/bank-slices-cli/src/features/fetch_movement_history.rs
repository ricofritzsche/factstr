//! Fetch-movement-history feature slice.
//!
//! Decide:
//! - no command decision is owned here
//! - this slice is read-side only
//!
//! Read:
//! - derive an account-specific movement log from `money-deposited`,
//!   `money-withdrawn`, `money-transferred-out`, and `money-transferred-in`
//! - format those facts into human-readable history lines in sequence order
//!
//! FACTSTR interface usage:
//! - `query(...)` reads the relevant facts for one account with explicit
//!   `EventQuery` filters
//! - the projection returns formatted history for the CLI

use crate::features::open_account::ensure_account_exists;
use factstore::{EventFilter, EventQuery, EventStore};
use serde_json::json;
use std::error::Error;

pub struct MovementHistoryQuery {
    account_id: String,
}

struct MovementEntry {
    sequence_number: u64,
    description: String,
}

impl MovementHistoryQuery {
    pub fn from_args(args: &[String]) -> Result<Self, String> {
        let account_id = required_flag(args, "--account")?;
        Ok(Self { account_id })
    }

    pub fn run(self, store: &impl EventStore) -> Result<String, Box<dyn Error>> {
        ensure_account_exists(store, &self.account_id)?;
        let movements = project_history(store, &self.account_id)?;

        let mut lines = vec![format!("movement history for account {}:", self.account_id)];
        if movements.is_empty() {
            lines.push("no matching facts".to_owned());
        } else {
            for movement in movements {
                lines.push(format!(
                    "{} {}",
                    movement.sequence_number, movement.description
                ));
            }
        }

        Ok(lines.join("\n"))
    }
}

fn project_history(
    store: &impl EventStore,
    account_id: &str,
) -> Result<Vec<MovementEntry>, Box<dyn Error>> {
    let query_result = store.query(&account_fact_query(account_id))?;
    let mut movements = Vec::new();

    for event_record in query_result.event_records {
        let amount = required_i64(&event_record.payload, "amount")?;

        let description = match event_record.event_type.as_str() {
            "money-deposited" => format!("deposit +{amount}"),
            "money-withdrawn" => format!("withdraw -{amount}"),
            "money-transferred-out" => format!(
                "transfer-out -> {} -{}",
                required_string(&event_record.payload, "toAccountId")?,
                amount
            ),
            "money-transferred-in" => format!(
                "transfer-in <- {} +{}",
                required_string(&event_record.payload, "fromAccountId")?,
                amount
            ),
            other => {
                return Err(format!("history projection saw unexpected event type {other}").into());
            }
        };

        movements.push(MovementEntry {
            sequence_number: event_record.sequence_number,
            description,
        });
    }

    Ok(movements)
}

fn account_fact_query(account_id: &str) -> EventQuery {
    EventQuery::all().with_filters([
        EventFilter::for_event_types(["money-deposited"])
            .with_payload_predicates([json!({ "accountId": account_id })]),
        EventFilter::for_event_types(["money-withdrawn"])
            .with_payload_predicates([json!({ "accountId": account_id })]),
        EventFilter::for_event_types(["money-transferred-out"])
            .with_payload_predicates([json!({ "fromAccountId": account_id })]),
        EventFilter::for_event_types(["money-transferred-in"])
            .with_payload_predicates([json!({ "toAccountId": account_id })]),
    ])
}

fn required_flag(args: &[String], flag: &str) -> Result<String, String> {
    let value = args
        .windows(2)
        .find(|pair| pair[0] == flag)
        .map(|pair| pair[1].clone())
        .ok_or_else(|| format!("history requires {flag}"))?;

    if value.starts_with("--") {
        return Err(format!("history requires a value after {flag}"));
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
