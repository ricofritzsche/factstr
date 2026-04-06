//! Open-account feature slice.
//!
//! Decide:
//! - a new account id may be opened exactly once
//! - duplicate account ids are rejected
//!
//! Read:
//! - query `account-opened` facts for one `accountId`
//! - existence is derived from whether that fact already exists
//!
//! FACTSTR interface usage:
//! - `append_if(...)` appends `account-opened` only when the account-specific
//!   context is still empty
//! - `query(...)` is reused by `ensure_account_exists(...)`

use factstore::{EventFilter, EventQuery, EventStore, EventStoreError, NewEvent};
use serde_json::json;
use std::error::Error;

pub struct OpenAccountCommand {
    account_id: String,
    owner: String,
}

impl OpenAccountCommand {
    pub fn from_args(args: &[String]) -> Result<Self, String> {
        let account_id = required_flag(args, "--account")?;
        let owner = required_flag(args, "--owner")?;

        Ok(Self { account_id, owner })
    }

    pub fn run(self, store: &impl EventStore) -> Result<String, Box<dyn Error>> {
        let append_result = match store.append_if(
            vec![NewEvent::new(
                "account-opened",
                json!({
                    "accountId": self.account_id,
                    "owner": self.owner
                }),
            )],
            &account_opened_query(&self.account_id),
            None,
        ) {
            Ok(append_result) => append_result,
            Err(EventStoreError::ConditionalAppendConflict { .. }) => {
                return Err(format!("account {} already exists", self.account_id).into());
            }
            Err(other) => return Err(Box::new(other)),
        };

        Ok(format!(
            "appended account-opened for account {} owner {} at sequences {}..={}",
            self.account_id,
            self.owner,
            append_result.first_sequence_number,
            append_result.last_sequence_number
        ))
    }
}

pub fn ensure_account_exists(
    store: &impl EventStore,
    account_id: &str,
) -> Result<(), Box<dyn Error>> {
    let query_result = store.query(&account_opened_query(account_id))?;
    if query_result.current_context_version.is_none() {
        return Err(format!("account {account_id} does not exist").into());
    }

    Ok(())
}

fn account_opened_query(account_id: &str) -> EventQuery {
    EventQuery::all().with_filters([EventFilter::for_event_types(["account-opened"])
        .with_payload_predicates([json!({ "accountId": account_id })])])
}

fn required_flag(args: &[String], flag: &str) -> Result<String, String> {
    let value = args
        .windows(2)
        .find(|pair| pair[0] == flag)
        .map(|pair| pair[1].clone())
        .ok_or_else(|| format!("open-account requires {flag}"))?;

    if value.starts_with("--") {
        return Err(format!("open-account requires a value after {flag}"));
    }

    Ok(value)
}
