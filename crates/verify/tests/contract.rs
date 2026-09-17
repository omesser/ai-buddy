//! What a caller of `ai-buddy-verify` is allowed to depend on: the exit code
//! behind each outcome, how a run's checks roll up into one outcome, the
//! `--json` object, and the `PROOF.md` section (#647 stone 3).
//!
//! These are pinned against literal values rather than against whatever the
//! code returns, because the whole point of the contract is that it cannot
//! drift without someone noticing.

use std::fs;

use ai_buddy_verify::contract::{Outcome, RunReport};
use ai_buddy_verify::paths::RunPaths;
use ai_buddy_verify::proof;
use serde_json::Value;
use tempfile::TempDir;

fn run_paths(dir: &TempDir) -> RunPaths {
    RunPaths::resolve(
        Some(dir.path().join("run").join("evidence")),
        Some("t1".into()),
    )
}

#[test]
fn a_run_whose_checks_all_pass_exits_zero() {
    let dir = TempDir::new().unwrap();
    let paths = run_paths(&dir);
    let mut report = RunReport::new("doctor", &paths, false);
    report.check(Outcome::Pass, "workspace layout", "Cargo.toml + src-tauri");
    report.check(Outcome::Pass, "cargo", "on PATH");

    assert_eq!(report.outcome(), Outcome::Pass);
    assert_eq!(report.outcome().exit_code(), 0);
}

#[test]
fn one_failed_check_fails_the_run_and_exits_one() {
    let dir = TempDir::new().unwrap();
    let paths = run_paths(&dir);
    let mut report = RunReport::new("units", &paths, false);
    report.check(Outcome::Pass, "cargo-core", "");
    report.check(Outcome::Fail, "node-tests", "exit 1");
    report.check(Outcome::Skip, "overlay-diagnostics", "macOS only");

    assert_eq!(report.outcome(), Outcome::Fail);
    assert_eq!(report.outcome().exit_code(), 1);
}

#[test]
fn a_run_with_nothing_but_skips_exits_two() {
    let dir = TempDir::new().unwrap();
    let paths = run_paths(&dir);
    let mut report = RunReport::new("poke", &paths, false);
    report.check(Outcome::Skip, "gesture lane", "no click leaf for linux");

    assert_eq!(report.outcome(), Outcome::Skip);
    assert_eq!(report.outcome().exit_code(), 2);
}

#[test]
fn a_skip_beside_a_pass_still_passes_the_run() {
    let dir = TempDir::new().unwrap();
    let paths = run_paths(&dir);
    let mut report = RunReport::new("doctor", &paths, false);
    report.check(Outcome::Pass, "workspace layout", "");
    report.check(Outcome::Skip, "swift", "not on PATH");

    assert_eq!(report.outcome(), Outcome::Pass);
    assert_eq!(report.outcome().exit_code(), 0);
}

#[test]
fn a_tool_error_outranks_a_failed_check_and_exits_three() {
    let dir = TempDir::new().unwrap();
    let paths = run_paths(&dir);
    let mut report = RunReport::new("overlay", &paths, false);
    report.check(Outcome::Fail, "overlay script", "exit 1");
    report.check(Outcome::Error, "repo root", "not an ai-buddy checkout");

    assert_eq!(report.outcome(), Outcome::Error);
    assert_eq!(report.outcome().exit_code(), 3);
}

#[test]
fn a_run_that_recorded_no_check_proved_nothing_and_exits_three() {
    let dir = TempDir::new().unwrap();
    let paths = run_paths(&dir);
    let report = RunReport::new("doctor", &paths, false);

    assert_eq!(report.outcome(), Outcome::Error);
    assert_eq!(report.outcome().exit_code(), 3);
}

#[test]
fn the_json_object_carries_outcome_command_evidence_and_every_check() {
    let dir = TempDir::new().unwrap();
    let paths = run_paths(&dir);
    let mut report = RunReport::new("units", &paths, true);
    report.check(Outcome::Pass, "cargo-core", "42 tests");
    report.check(Outcome::Fail, "node-tests", "exit 1");

    let json: Value = serde_json::from_str(&report.to_json()).unwrap();

    assert_eq!(json["command"], "units");
    assert_eq!(json["outcome"], "fail");
    assert_eq!(json["exit_code"], 1);
    assert_eq!(json["run_id"], "t1");
    assert_eq!(json["evidence_dir"], paths.evidence.to_str().unwrap());
    assert_eq!(json["checks"][0]["name"], "cargo-core");
    assert_eq!(json["checks"][0]["outcome"], "pass");
    assert_eq!(json["checks"][0]["detail"], "42 tests");
    assert_eq!(json["checks"][1]["name"], "node-tests");
    assert_eq!(json["checks"][1]["outcome"], "fail");
    assert_eq!(json["checks"][1]["detail"], "exit 1");
    assert_eq!(json["checks"].as_array().unwrap().len(), 2);
}

#[test]
fn the_json_object_survives_a_path_that_needs_escaping() {
    let dir = TempDir::new().unwrap();
    let evidence = dir.path().join(r#"quote"and\slash"#).join("evidence");
    let paths = RunPaths::resolve(Some(evidence.clone()), Some("t2".into()));
    let mut report = RunReport::new("doctor", &paths, true);
    report.check(Outcome::Pass, "workspace layout", r#"a "quoted" detail"#);

    let json: Value = serde_json::from_str(&report.to_json()).unwrap();

    assert_eq!(json["evidence_dir"], evidence.to_str().unwrap());
    assert_eq!(json["checks"][0]["detail"], r#"a "quoted" detail"#);
}

#[test]
fn a_proof_section_names_the_command_outcome_exit_and_every_check() {
    let dir = TempDir::new().unwrap();
    let paths = run_paths(&dir);
    let mut report = RunReport::new("doctor", &paths, false);
    report.check(Outcome::Pass, "workspace layout", "Cargo.toml + src-tauri");
    report.check(Outcome::Skip, "swift", "not on PATH");

    proof::append_proof(&report).unwrap();

    let text = fs::read_to_string(paths.evidence.join("PROOF.md")).unwrap();
    let heading = text.lines().next().unwrap();
    assert!(heading.starts_with("## "), "heading was {heading:?}");
    assert!(
        heading.ends_with(" doctor PASS (exit 0)"),
        "heading was {heading:?}"
    );
    assert!(text.contains("run-id: t1\n"), "proof was:\n{text}");
    assert!(
        text.contains(&format!("evidence: {}\n", paths.evidence.display())),
        "proof was:\n{text}"
    );
    assert!(
        text.contains("- PASS workspace layout: Cargo.toml + src-tauri\n"),
        "proof was:\n{text}"
    );
    assert!(
        text.contains("- SKIP swift: not on PATH\n"),
        "proof was:\n{text}"
    );
}

#[test]
fn a_check_with_no_detail_is_one_bare_bullet() {
    let dir = TempDir::new().unwrap();
    let paths = run_paths(&dir);
    let mut report = RunReport::new("cleanup", &paths, false);
    report.check(Outcome::Pass, "evidence preserved", "");

    proof::append_proof(&report).unwrap();

    let text = fs::read_to_string(paths.evidence.join("PROOF.md")).unwrap();
    assert!(
        text.contains("- PASS evidence preserved\n"),
        "proof was:\n{text}"
    );
}

#[test]
fn a_second_run_appends_rather_than_replacing() {
    let dir = TempDir::new().unwrap();
    let paths = run_paths(&dir);
    let mut first = RunReport::new("doctor", &paths, false);
    first.check(Outcome::Pass, "workspace layout", "");
    let mut second = RunReport::new("units", &paths, false);
    second.check(Outcome::Fail, "node-tests", "exit 1");

    proof::append_proof(&first).unwrap();
    proof::append_proof(&second).unwrap();

    let text = fs::read_to_string(paths.evidence.join("PROOF.md")).unwrap();
    assert_eq!(text.matches("\n## ").count() + 1, 2, "proof was:\n{text}");
    assert!(text.contains(" doctor PASS (exit 0)"), "proof was:\n{text}");
    assert!(text.contains(" units FAIL (exit 1)"), "proof was:\n{text}");
}
