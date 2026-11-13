//! Integration tests for the `alko` binary's public interface: the binary
//! itself. T1 ships the three subcommands as stubs — every real subcommand
//! must announce «не реализовано» and exit non-zero, while usage surfaces
//! (`--help`, `--version`) must work and the checked-in config template must
//! stay valid TOML.

use std::process::Command;

/// Outcome of one `alko` invocation: exit code plus captured stdout/stderr.
#[derive(Debug)]
struct Run {
    code: i32,
    stdout: String,
    stderr: String,
}

impl Run {
    fn succeeded(&self) -> bool {
        self.code == 0
    }
}

fn run_alko(args: &[&str]) -> Run {
    let output = Command::new(env!("CARGO_BIN_EXE_alko"))
        .args(args)
        .output()
        .expect("alko binary should be runnable");
    Run {
        code: output.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    }
}

#[test]
fn record_is_a_stub_that_fails() {
    let run = run_alko(&["record"]);
    assert!(!run.succeeded(), "stub must exit non-zero, got 0");
    assert!(
        run.stderr.contains("не реализовано"),
        "stderr must announce the stub, got: {:?}",
        run.stderr
    );
}

#[test]
fn paper_is_a_stub_that_fails() {
    let run = run_alko(&["paper"]);
    assert!(!run.succeeded(), "stub must exit non-zero, got 0");
    assert!(
        run.stderr.contains("не реализовано"),
        "stderr must announce the stub, got: {:?}",
        run.stderr
    );
}

#[test]
fn replay_is_a_stub_that_fails() {
    let run = run_alko(&["replay"]);
    assert!(!run.succeeded(), "stub must exit non-zero, got 0");
    assert!(
        run.stderr.contains("не реализовано"),
        "stderr must announce the stub, got: {:?}",
        run.stderr
    );
}

#[test]
fn no_subcommand_is_a_usage_error() {
    let run = run_alko(&[]);
    assert!(!run.succeeded(), "missing subcommand must exit non-zero");
}

#[test]
fn unknown_subcommand_is_a_usage_error() {
    let run = run_alko(&["trade"]);
    assert!(!run.succeeded(), "unknown subcommand must exit non-zero");
}

#[test]
fn version_works() {
    let run = run_alko(&["--version"]);
    assert!(run.succeeded(), "--version must exit zero: {run:?}");
    assert!(
        run.stdout.contains("alko"),
        "--version must name the binary, got: {:?}",
        run.stdout
    );
}

#[test]
fn help_lists_all_three_subcommands() {
    let run = run_alko(&["--help"]);
    assert!(run.succeeded(), "--help must exit zero: {run:?}");
    for subcommand in ["record", "paper", "replay"] {
        assert!(
            run.stdout.contains(subcommand),
            "--help must mention {subcommand}, got: {:?}",
            run.stdout
        );
    }
}

#[test]
fn config_example_template_is_valid_toml() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../config.example.toml");
    let raw = std::fs::read_to_string(path).expect("config.example.toml must exist");
    let parsed: toml::Table = raw.parse().expect("config.example.toml must be valid TOML");
    for section in ["general", "record", "monitoring"] {
        assert!(
            parsed.contains_key(section),
            "template must keep the {section} section"
        );
    }
}
