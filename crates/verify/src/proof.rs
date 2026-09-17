//! Append-only PROOF.md under the evidence directory.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::contract::RunReport;
use crate::paths;

/// Append one run's section to `evidence/PROOF.md`.
pub fn append_proof(report: &RunReport) -> std::io::Result<()> {
    let paths = report.paths();
    fs::create_dir_all(&paths.evidence)?;
    let proof = paths.evidence.join("PROOF.md");
    let mut f = OpenOptions::new().create(true).append(true).open(proof)?;
    let outcome = report.outcome();
    writeln!(
        f,
        "## {} {} {} (exit {})",
        utc_stamp(),
        report.command(),
        outcome.label(),
        outcome.exit_code()
    )?;
    writeln!(f)?;
    writeln!(f, "run-id: {}", paths.run_id)?;
    writeln!(f, "evidence: {}", paths.evidence.display())?;
    writeln!(f)?;
    for check in report.checks() {
        if check.detail.is_empty() {
            writeln!(f, "- {} {}", check.outcome.label(), check.name)?;
        } else {
            writeln!(
                f,
                "- {} {}: {}",
                check.outcome.label(),
                check.name,
                check.detail
            )?;
        }
    }
    writeln!(f)?;
    Ok(())
}

fn utc_stamp() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let days = secs / 86400;
    let tod = secs % 86400;
    let (y, m, d) = paths::civil_from_days(days as i64);
    let hh = tod / 3600;
    let mm = (tod % 3600) / 60;
    let ss = tod % 60;
    format!("{y:04}-{m:02}-{d:02}T{hh:02}:{mm:02}:{ss:02}Z")
}

/// Whether the evidence directory still exists (cleanup must keep it).
pub fn evidence_still_exists(evidence: &Path) -> bool {
    evidence.is_dir()
}
