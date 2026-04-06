use crate::features::{
    deposit::DepositCommand, fetch_balance::BalanceQuery,
    fetch_movement_history::MovementHistoryQuery, open_account::OpenAccountCommand,
    transfer::TransferCommand, withdraw::WithdrawCommand,
};
use std::path::PathBuf;

pub struct Startup {
    pub database_path: PathBuf,
    pub mode: Mode,
}

pub enum Mode {
    Interactive,
    OneShot(Command),
}

pub enum Command {
    OpenAccount(OpenAccountCommand),
    Deposit(DepositCommand),
    Withdraw(WithdrawCommand),
    Transfer(TransferCommand),
    Balance(BalanceQuery),
    History(MovementHistoryQuery),
    Help,
    Exit,
}

pub fn parse_startup_args(args: impl IntoIterator<Item = String>) -> Result<Startup, String> {
    let mut arguments = args.into_iter();
    let _program_name = arguments.next();
    let mut remaining: Vec<String> = arguments.collect();

    if remaining.first().map(String::as_str) == Some("--help")
        || remaining.first().map(String::as_str) == Some("-h")
    {
        return Err(startup_usage());
    }

    let mut database_path = default_database_path();
    if remaining.first().map(String::as_str) == Some("--db") {
        if remaining.len() < 2 {
            return Err(format!("--db requires a path\n\n{}", startup_usage()));
        }

        database_path = PathBuf::from(remaining[1].clone());
        remaining.drain(0..2);
    }

    let mode = if remaining.is_empty() {
        Mode::Interactive
    } else {
        Mode::OneShot(parse_command_words(&remaining)?)
    };

    Ok(Startup {
        database_path,
        mode,
    })
}

pub fn parse_command_line(input: &str) -> Result<Command, String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err("enter a command or type help".to_owned());
    }

    let words: Vec<String> = trimmed.split_whitespace().map(str::to_owned).collect();
    parse_command_words(&words)
}

pub fn command_usage() -> String {
    [
        "commands:",
        "  open-account --account ACCOUNT --owner OWNER",
        "  deposit --account ACCOUNT --amount AMOUNT",
        "  withdraw --account ACCOUNT --amount AMOUNT",
        "  transfer --from ACCOUNT --to ACCOUNT --amount AMOUNT",
        "  balance --account ACCOUNT",
        "  history --account ACCOUNT",
        "  help",
        "  exit",
    ]
    .join("\n")
}

fn parse_command_words(words: &[String]) -> Result<Command, String> {
    let Some(command_name) = words.first().map(String::as_str) else {
        return Err(command_usage());
    };
    let command_args = &words[1..];

    match command_name {
        "open-account" => Ok(Command::OpenAccount(OpenAccountCommand::from_args(
            command_args,
        )?)),
        "deposit" => Ok(Command::Deposit(DepositCommand::from_args(command_args)?)),
        "withdraw" => Ok(Command::Withdraw(WithdrawCommand::from_args(command_args)?)),
        "transfer" => Ok(Command::Transfer(TransferCommand::from_args(command_args)?)),
        "balance" => Ok(Command::Balance(BalanceQuery::from_args(command_args)?)),
        "history" => Ok(Command::History(MovementHistoryQuery::from_args(
            command_args,
        )?)),
        "help" => Ok(Command::Help),
        "exit" | "quit" => Ok(Command::Exit),
        _ => Err(format!(
            "unknown command: {command_name}\n\n{}",
            command_usage()
        )),
    }
}

fn default_database_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("bank-slices.sqlite3")
}

fn startup_usage() -> String {
    [
        "usage:",
        "  bank-slices-cli [--db PATH]",
        "  bank-slices-cli [--db PATH] open-account --account ACCOUNT --owner OWNER",
        "  bank-slices-cli [--db PATH] deposit --account ACCOUNT --amount AMOUNT",
        "  bank-slices-cli [--db PATH] withdraw --account ACCOUNT --amount AMOUNT",
        "  bank-slices-cli [--db PATH] transfer --from ACCOUNT --to ACCOUNT --amount AMOUNT",
        "  bank-slices-cli [--db PATH] balance --account ACCOUNT",
        "  bank-slices-cli [--db PATH] history --account ACCOUNT",
        "",
        "start without a command to open the interactive CLI.",
    ]
    .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_database_path_points_at_the_example_directory() {
        let startup = parse_startup_args([
            "bank-slices-cli".to_owned(),
            "balance".to_owned(),
            "--account".to_owned(),
            "A".to_owned(),
        ])
        .expect("cli parse should succeed");

        assert!(
            startup
                .database_path
                .ends_with("examples/bank-slices-cli/bank-slices.sqlite3"),
            "expected example-local database path, got {:?}",
            startup.database_path
        );
    }

    #[test]
    fn startup_without_command_enters_interactive_mode() {
        let startup = parse_startup_args(["bank-slices-cli".to_owned()])
            .expect("startup parse should succeed");

        assert!(matches!(startup.mode, Mode::Interactive));
    }

    #[test]
    fn help_and_exit_are_valid_interactive_commands() {
        assert!(matches!(
            parse_command_line("help").expect("help should parse"),
            Command::Help
        ));
        assert!(matches!(
            parse_command_line("exit").expect("exit should parse"),
            Command::Exit
        ));
    }
}
