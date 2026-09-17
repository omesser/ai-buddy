//! The contract as an agent meets it: the real binary, its exit status, and
//! what lands on stdout (#647 stone 3).

use std::path::Path;
use std::process::{Command, Output};

use serde_json::Value;
use tempfile::TempDir;

const BIN: &str = env!("CARGO_BIN_EXE_ai-buddy-verify");

fn verify(evidence: &Path, args: &[&str]) -> Output {
    let mut cmd = Command::new(BIN);
    cmd.args(["--evidence-dir", evidence.to_str().unwrap()]);
    cmd.args(["--run-id", "clitest"]);
    cmd.args(args);
    cmd.output().unwrap()
}

fn evidence_in(dir: &TempDir) -> std::path::PathBuf {
    dir.path().join("run").join("evidence")
}

#[test]
fn json_doctor_puts_one_parseable_object_on_stdout_and_exits_zero() {
    let dir = TempDir::new().unwrap();
    let evidence = evidence_in(&dir);
    let out = verify(&evidence, &["--json", "doctor"]);

    assert_eq!(
        out.status.code(),
        Some(0),
        "stderr:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );

    let json: Value = serde_json::from_slice(&out.stdout).unwrap_or_else(|e| {
        panic!(
            "stdout was not one JSON object ({e}): {:?}",
            String::from_utf8_lossy(&out.stdout)
        )
    });
    assert_eq!(json["command"], "doctor");
    assert_eq!(json["outcome"], "pass");
    assert_eq!(json["exit_code"], 0);
    assert_eq!(json["run_id"], "clitest");
    assert_eq!(json["evidence_dir"], evidence.to_str().unwrap());
    assert!(
        json["checks"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c["name"] == "workspace layout"),
        "checks were {:?}",
        json["checks"]
    );
}

#[test]
fn human_doctor_keeps_its_own_output_and_prints_no_json() {
    let dir = TempDir::new().unwrap();
    let evidence = evidence_in(&dir);
    let out = verify(&evidence, &["doctor"]);

    assert_eq!(out.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("doctor: OK"), "stdout was:\n{stdout}");
    assert!(
        stdout.contains("  PASS  workspace layout"),
        "stdout was:\n{stdout}"
    );
    assert!(!stdout.contains("\"outcome\""), "stdout was:\n{stdout}");
}

#[test]
fn json_cleanup_reports_the_evidence_it_preserved() {
    let dir = TempDir::new().unwrap();
    let evidence = evidence_in(&dir);
    let out = verify(&evidence, &["--json", "cleanup"]);

    assert_eq!(
        out.status.code(),
        Some(0),
        "stderr:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let json: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(json["command"], "cleanup");
    assert_eq!(json["outcome"], "pass");
    assert_eq!(json["exit_code"], 0);
    assert!(evidence.is_dir(), "cleanup must never delete evidence");
}

#[test]
fn a_bad_flag_is_a_tool_error_not_a_verification_failure() {
    let out = Command::new(BIN).arg("--no-such-flag").output().unwrap();

    assert_eq!(out.status.code(), Some(3));
}

#[test]
fn an_unknown_subcommand_is_a_tool_error() {
    let out = Command::new(BIN).arg("frobnicate").output().unwrap();

    assert_eq!(out.status.code(), Some(3));
}

#[test]
fn help_is_not_an_error() {
    let out = Command::new(BIN).arg("--help").output().unwrap();

    assert_eq!(out.status.code(), Some(0));
}

#[test]
fn every_run_appends_its_own_proof_section() {
    let dir = TempDir::new().unwrap();
    let evidence = evidence_in(&dir);
    verify(&evidence, &["doctor"]);
    verify(&evidence, &["--json", "cleanup"]);

    let text = std::fs::read_to_string(evidence.join("PROOF.md")).unwrap();
    assert!(text.contains(" doctor PASS (exit 0)"), "proof was:\n{text}");
    assert!(
        text.contains(" cleanup PASS (exit 0)"),
        "proof was:\n{text}"
    );
    assert!(
        text.contains(&format!("evidence: {}\n", evidence.display())),
        "proof was:\n{text}"
    );
}
