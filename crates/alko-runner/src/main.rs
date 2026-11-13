//! `alko` — the single CLI binary of the alcotrading workspace.
//!
//! One binary, three subcommands (`record`, `paper`, `replay`), per the
//! phase-1 spec. The subcommands are stubs in this ticket; their behaviour
//! lands with the recorder, paper-runner and playground tickets.

use std::process::ExitCode;

use clap::{Parser, Subcommand};

/// Single CLI for the alcotrading workspace: market-data recorder,
/// paper runner and replay backtester.
#[derive(Debug, Parser)]
#[command(name = "alko", version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Record market data from all exchanges (stub).
    Record,
    /// Run a strategy on live data with simulated execution (stub).
    Paper,
    /// Replay a recorded session through the backtester (stub).
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
    match cli.command {
        Commands::Record => stub("record"),
        Commands::Paper => stub("paper"),
        Commands::Replay => stub("replay"),
    }
}

/// Placeholder handler for a not-yet-implemented subcommand: prints a
/// stub notice to stderr and exits non-zero, per ticket T1 (issue #14).
fn stub(name: &str) -> ExitCode {
    tracing::debug!(subcommand = name, "stub invoked");
    eprintln!("error: `alko {name}` is not implemented yet");
    ExitCode::FAILURE
}
