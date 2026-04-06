//! Withdraw feature slice.
//!
//! Decide:
//! - the target account must already exist
//! - the withdrawal amount must be greater than zero
//! - the withdrawal must not make the balance negative
//!
//! Read:
//! - read current account existence
//! - read current projected balance for the same account
//!
//! FACTSTR interface usage:
//! - `query(...)` is used through `ensure_account_exists(...)`
//! - `BalanceProjectionRuntime::current_balance(...)` reads the local
//!   stream-maintained projection
//! - `append(...)` writes one local `money-withdrawn` fact after those checks
//! - the caller can then wait until the balance projection stream has observed
//!   that committed batch

use crate::features::fetch_balance::BalanceProjectionRuntime;
use crate::features::open_account::ensure_account_exists;
use factstore::{EventStore, NewEvent};
use serde_json::json;
use std::error::Error;

pub struct WithdrawCommand {
    account_id: String,
    amount: i64,
}

impl WithdrawCommand {
    pub fn from_args(args: &[String]) -> Result<Self, String> {
        let account_id = required_flag(args, "--account")?;
        let amount = required_amount(args, "--amount")?;

        Ok(Self { account_id, amount })
    }

    pub fn run(
        self,
        store: &impl EventStore,
        balance_projection: &BalanceProjectionRuntime,
    ) -> Result<(String, u64), Box<dyn Error>> {
        ensure_account_exists(store, &self.account_id)?;
        let balance_before_withdraw = balance_projection.current_balance(&self.account_id)?;
        if balance_before_withdraw < self.amount {
            return Err(format!(
                "withdraw would overdraw account {}: current balance is {}, requested {}",
                self.account_id, balance_before_withdraw, self.amount
            )
            .into());
        }

        let append_result = store.append(vec![NewEvent::new(
            "money-withdrawn",
            json!({
                "accountId": self.account_id,
                "amount": self.amount
            }),
        )])?;

        Ok((
            format!(
                "appended money-withdrawn for account {} amount {} at sequences {}..={}",
                self.account_id,
                self.amount,
                append_result.first_sequence_number,
                append_result.last_sequence_number
            ),
            append_result.last_sequence_number,
        ))
    }
}

fn required_flag(args: &[String], flag: &str) -> Result<String, String> {
    let value = args
        .windows(2)
        .find(|pair| pair[0] == flag)
        .map(|pair| pair[1].clone())
        .ok_or_else(|| format!("withdraw requires {flag}"))?;

    if value.starts_with("--") {
        return Err(format!("withdraw requires a value after {flag}"));
    }

    Ok(value)
}

fn required_amount(args: &[String], flag: &str) -> Result<i64, String> {
    let amount = required_flag(args, flag)?;
    let parsed_amount = amount
        .parse::<i64>()
        .map_err(|_| format!("withdraw amount must be an integer, got {amount}"))?;

    if parsed_amount <= 0 {
        return Err("withdraw amount must be greater than zero".to_owned());
    }

    Ok(parsed_amount)
}
