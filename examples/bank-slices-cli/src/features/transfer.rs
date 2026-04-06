//! Transfer feature slice.
//!
//! Decide:
//! - source and destination accounts must both exist
//! - source and destination must be different account ids
//! - the transfer amount must be greater than zero
//! - the transfer must not make the source balance negative
//!
//! Read:
//! - read source existence
//! - read destination existence
//! - read current projected balance of the source account
//!
//! FACTSTR interface usage:
//! - `query(...)` is used through `ensure_account_exists(...)`
//! - `BalanceProjectionRuntime::current_balance(...)` reads the local
//!   stream-maintained balance projection
//! - `append(...)` writes one committed batch with
//!   `money-transferred-out` and `money-transferred-in`
//! - the caller can then wait until the balance projection stream has observed
//!   that committed batch

use crate::features::fetch_balance::BalanceProjectionRuntime;
use crate::features::open_account::ensure_account_exists;
use factstore::{EventStore, NewEvent};
use serde_json::json;
use std::error::Error;
use std::time::{SystemTime, UNIX_EPOCH};

pub struct TransferCommand {
    from_account_id: String,
    to_account_id: String,
    amount: i64,
}

impl TransferCommand {
    pub fn from_args(args: &[String]) -> Result<Self, String> {
        let from_account_id = required_flag(args, "--from")?;
        let to_account_id = required_flag(args, "--to")?;
        let amount = required_amount(args, "--amount")?;

        if from_account_id == to_account_id {
            return Err("transfer requires two distinct accounts".to_owned());
        }

        Ok(Self {
            from_account_id,
            to_account_id,
            amount,
        })
    }

    pub fn run(
        self,
        store: &impl EventStore,
        balance_projection: &BalanceProjectionRuntime,
    ) -> Result<(String, u64), Box<dyn Error>> {
        ensure_account_exists(store, &self.from_account_id)?;
        ensure_account_exists(store, &self.to_account_id)?;
        let from_balance_before_transfer =
            balance_projection.current_balance(&self.from_account_id)?;
        if from_balance_before_transfer < self.amount {
            return Err(format!(
                "transfer would overdraw account {}: current balance is {}, requested {}",
                self.from_account_id, from_balance_before_transfer, self.amount
            )
            .into());
        }

        let transfer_id = next_transfer_id();
        let append_result = store.append(vec![
            NewEvent::new(
                "money-transferred-out",
                json!({
                    "transferId": transfer_id,
                    "fromAccountId": self.from_account_id,
                    "toAccountId": self.to_account_id,
                    "amount": self.amount
                }),
            ),
            NewEvent::new(
                "money-transferred-in",
                json!({
                    "transferId": transfer_id,
                    "fromAccountId": self.from_account_id,
                    "toAccountId": self.to_account_id,
                    "amount": self.amount
                }),
            ),
        ])?;

        Ok((
            format!(
                "appended money-transferred-out and money-transferred-in for {} -> {} amount {} at sequences {}..={}",
                self.from_account_id,
                self.to_account_id,
                self.amount,
                append_result.first_sequence_number,
                append_result.last_sequence_number
            ),
            append_result.last_sequence_number,
        ))
    }
}

fn next_transfer_id() -> String {
    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should be after UNIX_EPOCH")
        .as_nanos();

    format!("transfer-{suffix}")
}

fn required_flag(args: &[String], flag: &str) -> Result<String, String> {
    let value = args
        .windows(2)
        .find(|pair| pair[0] == flag)
        .map(|pair| pair[1].clone())
        .ok_or_else(|| format!("transfer requires {flag}"))?;

    if value.starts_with("--") {
        return Err(format!("transfer requires a value after {flag}"));
    }

    Ok(value)
}

fn required_amount(args: &[String], flag: &str) -> Result<i64, String> {
    let amount = required_flag(args, flag)?;
    let parsed_amount = amount
        .parse::<i64>()
        .map_err(|_| format!("transfer amount must be an integer, got {amount}"))?;

    if parsed_amount <= 0 {
        return Err("transfer amount must be greater than zero".to_owned());
    }

    Ok(parsed_amount)
}
