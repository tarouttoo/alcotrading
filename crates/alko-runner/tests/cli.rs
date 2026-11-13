//! Black-box tests for the `alko` binary (issue #14): the `record`,
//! `paper` and `replay` subcommands are stubs — each must say so and
//! exit non-zero.

use assert_cmd::Command;
use predicates::prelude::*;

fn alko() -> Command {
    Command::cargo_bin("alko").expect("the `alko` binary must build")
}

#[test]
fn record_prints_stub_and_exits_nonzero() {
    alko()
        .arg("record")
        .assert()
        .failure()
        .stderr(predicates::str::contains("not implemented"));
}

#[test]
fn paper_prints_stub_and_exits_nonzero() {
    alko()
        .arg("paper")
        .assert()
        .failure()
        .stderr(predicates::str::contains("not implemented"));
}

#[test]
fn replay_prints_stub_and_exits_nonzero() {
    alko()
        .arg("replay")
        .assert()
        .failure()
        .stderr(predicates::str::contains("not implemented"));
}

#[test]
fn help_lists_all_three_subcommands() {
    alko()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicates::str::contains("record"))
        .stdout(predicates::str::contains("paper"))
        .stdout(predicates::str::contains("replay"));
}

#[test]
fn version_is_reported() {
    alko()
        .arg("--version")
        .assert()
        .success()
        .stdout(predicates::str::contains("alko 0.1.0"));
}
