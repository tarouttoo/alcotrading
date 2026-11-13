//! The single `alko` binary of the monorepo: one entrypoint over
//! `record` / `paper` / `replay`.
//!
//! In T1 every subcommand is a stub that announces «не реализовано» and
//! exits non-zero. Later tickets wire them up: `record` to the three-exchange
//! recorder (T5+), `replay` to the playground backtest (T13), `paper` to the
//! paper runner (T14).

use std::process::ExitCode;

use clap::{Parser, Subcommand};

/// Один бинарник алготрейдинг-монорепо: запись, replay, paper.
#[derive(Debug, Parser)]
#[command(
    name = "alko",
    version,
    about = "Запись стакана и сделок трёх бирж, replay-бэктест, paper-раннер"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Запись стакана (20 уровней) и сырых сделок пяти пар на трёх биржах.
    Record,
    /// Стратегия на живых данных с симулированным исполнением.
    Paper,
    /// Событийный replay-бэктест по записи.
    Replay,
}

fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let cli = Cli::parse();
    let subcommand = match cli.command {
        Command::Record => "record",
        Command::Paper => "paper",
        Command::Replay => "replay",
    };
    stub_failure(subcommand)
}

/// Stub handler shared by every subcommand: announces that the subcommand is
/// not implemented yet and fails loudly instead of pretending success.
fn stub_failure(subcommand: &str) -> ExitCode {
    tracing::debug!(subcommand, "stub invoked");
    eprintln!("alko {subcommand}: не реализовано (заглушка каркаса T1, спека #13)");
    ExitCode::FAILURE
}
