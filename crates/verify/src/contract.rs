//! The agent contract: what a caller of `ai-buddy-verify` may depend on
//! (#647 stone 3 under ADR-0027). A run is a list of checks, one exit code,
//! one optional JSON object, and one appended `PROOF.md` section.
//!
//! | exit | outcome | what it means |
//! |------|---------|---------------|
//! | 0 | `PASS` | everything that ran passed |
//! | 1 | `FAIL` | a check failed. The thing under test is broken |
//! | 2 | `SKIP` | nothing could be proven on this host. An unsupported OS or an absent lane, not a defect |
//! | 3 | `ERROR` | the tool could not run. Bad flags, no ai-buddy checkout, unwritable evidence. Not a verification failure |
//!
//! A run's outcome is the worst of its checks, ranked error, fail, skip, pass.
//! A run that recorded no check at all is an `ERROR`, because it proved
//! nothing. A skip beside a pass leaves the run passing, and the `checks`
//! array is what tells a caller which lanes were not proven.
//!
//! `--json` puts one object on stdout and nothing else:
//!
//! ```json
//! {"checks":[{"detail":"42 tests","name":"cargo-core","outcome":"pass"}],"command":"units","evidence_dir":"/tmp/run/evidence","exit_code":1,"outcome":"fail","run_id":"t1"}
//! ```
//!
//! Every run appends one section to `evidence/PROOF.md`:
//!
//! ```text
//! ## 2026-09-16T18:30:00Z doctor PASS (exit 0)
//!
//! run-id: 20260916-183000-1234
//! evidence: /tmp/ai-buddy-verify-20260916-183000-1234/evidence
//!
//! - PASS workspace layout: Cargo.toml + src-tauri
//! - SKIP swift: not on PATH
//! ```
//!
//! Doctor's host-tool gaps are `SKIP` checks, which is why a missing `swift`
//! or `xdotool` never fails a doctor run.

use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

use crate::paths::RunPaths;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    Pass,
    Fail,
    Skip,
    Error,
}

impl Outcome {
    pub const fn exit_code(self) -> u8 {
        match self {
            Outcome::Pass => 0,
            Outcome::Fail => 1,
            Outcome::Skip => 2,
            Outcome::Error => 3,
        }
    }

    /// The human spelling. The JSON spelling is this lowercased, so the two
    /// cannot drift apart.
    pub const fn label(self) -> &'static str {
        match self {
            Outcome::Pass => "PASS",
            Outcome::Fail => "FAIL",
            Outcome::Skip => "SKIP",
            Outcome::Error => "ERROR",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Check {
    pub name: String,
    pub outcome: Outcome,
    pub detail: String,
}

/// One run of one subcommand: its checks, and how they reach a caller.
pub struct RunReport<'a> {
    command: &'static str,
    paths: &'a RunPaths,
    json: bool,
    checks: Vec<Check>,
}

impl<'a> RunReport<'a> {
    pub fn new(command: &'static str, paths: &'a RunPaths, json: bool) -> Self {
        Self {
            command,
            paths,
            json,
            checks: Vec::new(),
        }
    }

    pub fn command(&self) -> &'static str {
        self.command
    }

    pub fn paths(&self) -> &RunPaths {
        self.paths
    }

    pub fn checks(&self) -> &[Check] {
        &self.checks
    }

    /// Record one check and print its human line.
    pub fn check(&mut self, outcome: Outcome, name: &str, detail: &str) {
        if detail.is_empty() {
            self.say(&format!("  {}  {name}", outcome.label()));
        } else {
            self.say(&format!("  {}  {name}: {detail}", outcome.label()));
        }
        self.checks.push(Check {
            name: name.to_string(),
            outcome,
            detail: detail.to_string(),
        });
    }

    pub fn outcome(&self) -> Outcome {
        let any = |o: Outcome| self.checks.iter().any(|c| c.outcome == o);
        if self.checks.is_empty() || any(Outcome::Error) {
            Outcome::Error
        } else if any(Outcome::Fail) {
            Outcome::Fail
        } else if self.checks.iter().all(|c| c.outcome == Outcome::Skip) {
            Outcome::Skip
        } else {
            Outcome::Pass
        }
    }

    /// Human progress that is not a check.
    pub fn say(&self, msg: &str) {
        if self.json {
            eprintln!("{msg}");
        } else {
            println!("{msg}");
        }
    }

    pub fn to_json(&self) -> String {
        let outcome = self.outcome();
        let checks: Vec<_> = self
            .checks
            .iter()
            .map(|c| {
                serde_json::json!({
                    "name": c.name,
                    "outcome": c.outcome.label().to_ascii_lowercase(),
                    "detail": c.detail,
                })
            })
            .collect();
        serde_json::json!({
            "command": self.command,
            "outcome": outcome.label().to_ascii_lowercase(),
            "exit_code": outcome.exit_code(),
            "run_id": self.paths.run_id,
            "evidence_dir": self.paths.evidence.display().to_string(),
            "checks": checks,
        })
        .to_string()
    }

    pub fn emit(&self) {
        if self.json {
            println!("{}", self.to_json());
        }
    }

    /// Run a child process on behalf of this run. `None` means the spawn
    /// itself failed.
    ///
    /// Under `--json` stdout belongs to the result object, so a child's stdout
    /// is replayed to our stderr rather than inherited: otherwise `cargo test`
    /// or a leaf script would write over the one object a caller parses. With
    /// no log and no `--json` the child keeps our streams, so a long leaf
    /// script still shows progress as it runs.
    pub fn exec(&self, cmd: &mut Command, log: Option<&Path>) -> Option<i32> {
        if log.is_none() && !self.json {
            return match cmd.status() {
                Ok(s) => Some(s.code().unwrap_or(1)),
                Err(e) => {
                    eprintln!("{}: failed to spawn: {e}", self.command);
                    None
                }
            };
        }

        let out = match cmd.stdout(Stdio::piped()).stderr(Stdio::piped()).output() {
            Ok(out) => out,
            Err(e) => {
                let msg = format!("failed to spawn: {e}\n");
                if let Some(path) = log {
                    let _ = fs::write(path, &msg);
                }
                eprint!("{msg}");
                return None;
            }
        };

        if let Some(path) = log {
            let mut combined = out.stdout.clone();
            if !out.stderr.is_empty() {
                if !combined.is_empty() && !combined.ends_with(b"\n") {
                    combined.push(b'\n');
                }
                combined.extend_from_slice(&out.stderr);
            }
            let _ = fs::write(path, &combined);
        }
        if self.json {
            let _ = std::io::stderr().write_all(&out.stdout);
        } else {
            let _ = std::io::stdout().write_all(&out.stdout);
        }
        let _ = std::io::stderr().write_all(&out.stderr);
        Some(out.status.code().unwrap_or(1))
    }
}
