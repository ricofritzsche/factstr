mod cli;
mod features;

use crate::cli::{Command, Mode, Startup};
use crate::features::fetch_balance::BalanceProjectionRuntime;
use factstore_sqlite::SqliteStore;
use rustyline::DefaultEditor;
use rustyline::error::ReadlineError;
use std::error::Error;
use std::fs;
#[cfg(test)]
use std::io::{BufRead, Write};
use std::path::Path;

fn main() {
    let startup = match cli::parse_startup_args(std::env::args()) {
        Ok(startup) => startup,
        Err(message) => {
            eprintln!("{message}");
            std::process::exit(2);
        }
    };

    match run_application(startup) {
        Ok(()) => {}
        Err(error) => {
            eprintln!("bank-slices-cli failed: {error}");
            std::process::exit(1);
        }
    }
}

fn run_application(startup: Startup) -> Result<(), Box<dyn Error>> {
    ensure_database_parent_exists(&startup.database_path)?;
    let store = SqliteStore::open(&startup.database_path)?;
    let balance_projection = BalanceProjectionRuntime::start(&store)?;

    match startup.mode {
        Mode::Interactive => run_terminal_repl(&store, &balance_projection),
        Mode::OneShot(command) => {
            println!("{}", execute_command(command, &store, &balance_projection)?);
            Ok(())
        }
    }
}

#[cfg(test)]
fn run_startup(
    startup: Startup,
    mut input: impl BufRead,
    mut output: impl Write,
) -> Result<(), Box<dyn Error>> {
    ensure_database_parent_exists(&startup.database_path)?;
    let store = SqliteStore::open(&startup.database_path)?;
    let balance_projection = BalanceProjectionRuntime::start(&store)?;

    match startup.mode {
        Mode::OneShot(command) => {
            let result = execute_command(command, &store, &balance_projection)?;
            writeln!(output, "{result}")?;
        }
        Mode::Interactive => {
            run_interactive_session(&store, &balance_projection, &mut input, &mut output)?
        }
    }

    Ok(())
}

fn execute_command(
    command: Command,
    store: &SqliteStore,
    balance_projection: &BalanceProjectionRuntime,
) -> Result<String, Box<dyn Error>> {
    match command {
        Command::OpenAccount(command) => command.run(store),
        Command::Deposit(command) => {
            let (output, last_sequence_number) = command.run(store, balance_projection)?;
            balance_projection.wait_until_applied(last_sequence_number)?;
            Ok(output)
        }
        Command::Withdraw(command) => {
            let (output, last_sequence_number) = command.run(store, balance_projection)?;
            balance_projection.wait_until_applied(last_sequence_number)?;
            Ok(output)
        }
        Command::Transfer(command) => {
            let (output, last_sequence_number) = command.run(store, balance_projection)?;
            balance_projection.wait_until_applied(last_sequence_number)?;
            Ok(output)
        }
        Command::Balance(command) => command.run(store, balance_projection),
        Command::History(command) => command.run(store),
        Command::Help => Ok(cli::command_usage()),
        Command::Exit => Ok("bye".to_owned()),
    }
}

#[cfg(test)]
fn run_interactive_session(
    store: &SqliteStore,
    balance_projection: &BalanceProjectionRuntime,
    input: &mut impl BufRead,
    output: &mut impl Write,
) -> Result<(), Box<dyn Error>> {
    writeln!(
        output,
        "bank-slices-cli interactive mode\n{}\n",
        cli::command_usage()
    )?;

    let mut line = String::new();
    loop {
        write!(output, "bank> ")?;
        output.flush()?;

        line.clear();
        if input.read_line(&mut line)? == 0 {
            writeln!(output, "bye")?;
            break;
        }

        match cli::parse_command_line(&line) {
            Ok(Command::Exit) => {
                writeln!(output, "bye")?;
                break;
            }
            Ok(command) => match execute_command(command, store, balance_projection) {
                Ok(result) => writeln!(output, "{result}")?,
                Err(error) => writeln!(output, "error: {error}")?,
            },
            Err(error) => writeln!(output, "error: {error}")?,
        }
    }

    Ok(())
}

fn run_terminal_repl(
    store: &SqliteStore,
    balance_projection: &BalanceProjectionRuntime,
) -> Result<(), Box<dyn Error>> {
    let mut editor = DefaultEditor::new()?;
    println!(
        "bank-slices-cli interactive mode\n{}\n",
        cli::command_usage()
    );

    loop {
        match editor.readline("bank> ") {
            Ok(line) => {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    println!("error: enter a command or type help");
                    continue;
                }

                editor.add_history_entry(trimmed)?;
                match cli::parse_command_line(trimmed) {
                    Ok(Command::Exit) => {
                        println!("bye");
                        break;
                    }
                    Ok(command) => match execute_command(command, store, balance_projection) {
                        Ok(result) => println!("{result}"),
                        Err(error) => println!("error: {error}"),
                    },
                    Err(error) => println!("error: {error}"),
                }
            }
            Err(ReadlineError::Interrupted) | Err(ReadlineError::Eof) => {
                println!("bye");
                break;
            }
            Err(error) => return Err(Box::new(error)),
        }
    }

    Ok(())
}

fn ensure_database_parent_exists(database_path: &Path) -> Result<(), Box<dyn Error>> {
    if let Some(parent) = database_path.parent() {
        fs::create_dir_all(parent)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use factstore::EventStore;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn cli_smoke_path_works_against_one_sqlite_database() {
        let database_path = unique_database_path("smoke-path");

        let open_a_output = run_one_shot_ok([
            "--db",
            database_path
                .to_str()
                .expect("database path should be valid UTF-8"),
            "open-account",
            "--account",
            "A",
            "--owner",
            "Alice",
        ]);
        assert!(open_a_output.contains("account-opened"));

        let open_b_output = run_one_shot_ok([
            "--db",
            database_path
                .to_str()
                .expect("database path should be valid UTF-8"),
            "open-account",
            "--account",
            "B",
            "--owner",
            "Bob",
        ]);
        assert!(open_b_output.contains("account-opened"));

        let deposit_output = run_one_shot_ok([
            "--db",
            database_path
                .to_str()
                .expect("database path should be valid UTF-8"),
            "deposit",
            "--account",
            "A",
            "--amount",
            "100",
        ]);
        assert!(deposit_output.contains("money-deposited"));

        let withdraw_output = run_one_shot_ok([
            "--db",
            database_path
                .to_str()
                .expect("database path should be valid UTF-8"),
            "withdraw",
            "--account",
            "A",
            "--amount",
            "20",
        ]);
        assert!(withdraw_output.contains("money-withdrawn"));

        let transfer_output = run_one_shot_ok([
            "--db",
            database_path
                .to_str()
                .expect("database path should be valid UTF-8"),
            "transfer",
            "--from",
            "A",
            "--to",
            "B",
            "--amount",
            "30",
        ]);
        assert!(transfer_output.contains("money-transferred-out"));
        assert!(transfer_output.contains("money-transferred-in"));

        let balance_output = run_one_shot_ok([
            "--db",
            database_path
                .to_str()
                .expect("database path should be valid UTF-8"),
            "balance",
            "--account",
            "A",
        ]);
        assert!(balance_output.contains("account A balance: 50"));

        let history_output = run_one_shot_ok([
            "--db",
            database_path
                .to_str()
                .expect("database path should be valid UTF-8"),
            "history",
            "--account",
            "A",
        ]);
        assert!(history_output.contains("3 deposit +100"));
        assert!(history_output.contains("4 withdraw -20"));
        assert!(history_output.contains("5 transfer-out -> B -30"));

        let store = SqliteStore::open(&database_path).expect("sqlite store should reopen");
        let query_result = store
            .query(&factstore::EventQuery::all())
            .expect("query should succeed");
        assert_eq!(query_result.event_records.len(), 6);

        let _ = std::fs::remove_file(&database_path);
    }

    #[test]
    fn commands_reject_missing_accounts() {
        let database_path = unique_database_path("missing-account");

        let balance_output = run_one_shot_err([
            "--db",
            database_path
                .to_str()
                .expect("database path should be valid UTF-8"),
            "balance",
            "--account",
            "A",
        ]);
        assert!(balance_output.contains("account A does not exist"));

        let _ = std::fs::remove_file(&database_path);
    }

    #[test]
    fn open_account_rejects_duplicate_identifier() {
        let database_path = unique_database_path("duplicate-account");

        let first_open = run_one_shot_ok([
            "--db",
            database_path
                .to_str()
                .expect("database path should be valid UTF-8"),
            "open-account",
            "--account",
            "A",
            "--owner",
            "Alice",
        ]);
        assert!(first_open.contains("account-opened"));

        let duplicate_open = run_one_shot_err([
            "--db",
            database_path
                .to_str()
                .expect("database path should be valid UTF-8"),
            "open-account",
            "--account",
            "A",
            "--owner",
            "Alice Again",
        ]);
        assert!(duplicate_open.contains("account A already exists"));

        let _ = std::fs::remove_file(&database_path);
    }

    #[test]
    fn withdraw_rejects_amounts_that_would_make_balance_negative() {
        let database_path = unique_database_path("withdraw-overdraw");

        run_one_shot_ok([
            "--db",
            database_path
                .to_str()
                .expect("database path should be valid UTF-8"),
            "open-account",
            "--account",
            "A",
            "--owner",
            "Alice",
        ]);
        run_one_shot_ok([
            "--db",
            database_path
                .to_str()
                .expect("database path should be valid UTF-8"),
            "deposit",
            "--account",
            "A",
            "--amount",
            "100",
        ]);

        let withdraw_error = run_one_shot_err([
            "--db",
            database_path
                .to_str()
                .expect("database path should be valid UTF-8"),
            "withdraw",
            "--account",
            "A",
            "--amount",
            "101",
        ]);
        assert!(withdraw_error.contains("withdraw would overdraw account A"));

        let balance_output = run_one_shot_ok([
            "--db",
            database_path
                .to_str()
                .expect("database path should be valid UTF-8"),
            "balance",
            "--account",
            "A",
        ]);
        assert!(balance_output.contains("account A balance: 100"));

        let _ = std::fs::remove_file(&database_path);
    }

    #[test]
    fn transfer_rejects_amounts_that_would_make_source_balance_negative() {
        let database_path = unique_database_path("transfer-overdraw");

        run_one_shot_ok([
            "--db",
            database_path
                .to_str()
                .expect("database path should be valid UTF-8"),
            "open-account",
            "--account",
            "A",
            "--owner",
            "Alice",
        ]);
        run_one_shot_ok([
            "--db",
            database_path
                .to_str()
                .expect("database path should be valid UTF-8"),
            "open-account",
            "--account",
            "B",
            "--owner",
            "Bob",
        ]);
        run_one_shot_ok([
            "--db",
            database_path
                .to_str()
                .expect("database path should be valid UTF-8"),
            "deposit",
            "--account",
            "A",
            "--amount",
            "100",
        ]);

        let transfer_error = run_one_shot_err([
            "--db",
            database_path
                .to_str()
                .expect("database path should be valid UTF-8"),
            "transfer",
            "--from",
            "A",
            "--to",
            "B",
            "--amount",
            "101",
        ]);
        assert!(transfer_error.contains("transfer would overdraw account A"));

        let balance_output = run_one_shot_ok([
            "--db",
            database_path
                .to_str()
                .expect("database path should be valid UTF-8"),
            "balance",
            "--account",
            "A",
        ]);
        assert!(balance_output.contains("account A balance: 100"));

        let _ = std::fs::remove_file(&database_path);
    }

    #[test]
    fn interactive_session_accepts_commands_in_one_running_process() {
        let database_path = unique_database_path("interactive-session");
        let startup = cli::parse_startup_args([
            "bank-slices-cli".to_owned(),
            "--db".to_owned(),
            database_path
                .to_str()
                .expect("database path should be valid UTF-8")
                .to_owned(),
        ])
        .expect("startup parse should succeed");

        let input = [
            "open-account --account A --owner Alice",
            "open-account --account B --owner Bob",
            "deposit --account A --amount 100",
            "withdraw --account A --amount 20",
            "transfer --from A --to B --amount 30",
            "balance --account A",
            "history --account A",
            "exit",
        ]
        .join("\n");
        let mut output = Vec::new();

        run_startup(startup, std::io::Cursor::new(input), &mut output)
            .expect("interactive session should succeed");

        let rendered_output =
            String::from_utf8(output).expect("interactive output should be valid UTF-8");
        assert!(rendered_output.contains("bank-slices-cli interactive mode"));
        assert!(rendered_output.contains("appended account-opened for account A owner Alice"));
        assert!(rendered_output.contains("appended account-opened for account B owner Bob"));
        assert!(rendered_output.contains("appended money-deposited"));
        assert!(rendered_output.contains("appended money-withdrawn"));
        assert!(
            rendered_output.contains("appended money-transferred-out and money-transferred-in")
        );
        assert!(rendered_output.contains("account A balance: 50"));
        assert!(rendered_output.contains("3 deposit +100"));
        assert!(rendered_output.contains("4 withdraw -20"));
        assert!(rendered_output.contains("5 transfer-out -> B -30"));
        assert!(rendered_output.contains("bye"));
    }

    fn run_one_shot_ok<const N: usize>(args: [&str; N]) -> String {
        match run_one_shot(args) {
            Ok(output) => output,
            Err(error) => panic!("expected command success, got error: {error}"),
        }
    }

    fn run_one_shot_err<const N: usize>(args: [&str; N]) -> String {
        match run_one_shot(args) {
            Ok(output) => panic!("expected command failure, got output: {output}"),
            Err(error) => error,
        }
    }

    fn run_one_shot<const N: usize>(args: [&str; N]) -> Result<String, String> {
        let startup = cli::parse_startup_args(
            std::iter::once("bank-slices-cli".to_owned())
                .chain(args.into_iter().map(str::to_owned)),
        )
        .map_err(|error| format!("parse error: {error}"))?;

        let mut output = Vec::new();
        match run_startup(startup, std::io::Cursor::new(Vec::<u8>::new()), &mut output) {
            Ok(()) => Ok(String::from_utf8(output).expect("output should be valid UTF-8")),
            Err(error) => Err(error.to_string()),
        }
    }

    fn unique_database_path(test_name: &str) -> PathBuf {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be after UNIX_EPOCH")
            .as_nanos();

        std::env::temp_dir().join(format!(
            "factstr-bank-slices-cli-{test_name}-{suffix}.sqlite3"
        ))
    }
}
