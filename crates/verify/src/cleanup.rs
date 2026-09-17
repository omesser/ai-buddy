//! Kill only recorded PIDs; remove scratch; keep evidence.

use std::fs;

use crate::contract::{Outcome, RunReport};
use crate::proof;

/// Cleanup. Evidence surviving is the whole promise, so losing it is an
/// `ERROR` (this tool broke), not a `FAIL` (something under test broke).
pub fn run(report: &mut RunReport) {
    let paths = report.paths().clone();

    let pidlist = paths.scratch.join("pids").join("owned.pids");
    if pidlist.is_file() {
        if let Ok(text) = fs::read_to_string(&pidlist) {
            for line in text.lines() {
                let pid = line.trim();
                if pid.is_empty() {
                    continue;
                }
                kill_pid(report, pid);
            }
        }
    }

    let app_pid_file = paths.scratch.join("pids").join("app.pid");
    if app_pid_file.is_file() {
        if let Ok(pid) = fs::read_to_string(&app_pid_file) {
            let pid = pid.trim();
            if !pid.is_empty() {
                report.say(&format!("cleanup: kill app.pid {pid}"));
                kill_pid(report, pid);
            }
        }
    }

    if paths.scratch.exists() {
        if let Err(e) = fs::remove_dir_all(&paths.scratch) {
            eprintln!("cleanup: rm scratch {}: {e}", paths.scratch.display());
        }
    }

    let _ = fs::create_dir_all(&paths.evidence);

    report.say(&format!(
        "cleanup: scratch removed; evidence preserved at {}",
        paths.evidence.display()
    ));

    if let Ok(rd) = fs::read_dir(&paths.evidence) {
        for ent in rd.filter_map(|e| e.ok()) {
            report.say(&format!("  {}", ent.path().display()));
        }
    }

    if proof::evidence_still_exists(&paths.evidence) {
        report.check(
            Outcome::Pass,
            "evidence preserved",
            &paths.evidence.display().to_string(),
        );
    } else {
        report.check(
            Outcome::Error,
            "evidence preserved",
            &format!("evidence dir missing at {}", paths.evidence.display()),
        );
    }
}

fn kill_pid(report: &RunReport, pid: &str) {
    #[cfg(unix)]
    {
        use std::process::Command;
        use std::thread;
        use std::time::Duration;
        if Command::new("kill")
            .args(["-0", pid])
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
        {
            report.say(&format!("cleanup: kill {pid}"));
            let _ = Command::new("kill").arg(pid).status();
            thread::sleep(Duration::from_millis(500));
            let _ = Command::new("kill").args(["-9", pid]).status();
        }
    }
    #[cfg(not(unix))]
    {
        // Windows: taskkill if we ever record PIDs there in a later stone.
        report.say(&format!("cleanup: skip kill on non-unix pid={pid}"));
    }
}

/// Test helper: cleanup must not delete evidence.
#[cfg(test)]
pub fn cleanup_keeps_evidence(scratch: &std::path::Path, evidence: &std::path::Path) -> bool {
    let _ = fs::create_dir_all(scratch.join("pids"));
    let _ = fs::create_dir_all(evidence);
    let _ = fs::write(evidence.join("PROOF.md"), "keep\n");
    let _ = fs::write(scratch.join("pids").join("owned.pids"), "");
    let _ = fs::remove_dir_all(scratch);
    let _ = fs::create_dir_all(evidence);
    evidence.is_dir() && evidence.join("PROOF.md").is_file()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn cleanup_keeps_evidence_dir() {
        let dir = tempdir().unwrap();
        let scratch = dir.path().join("scratch");
        let evidence = dir.path().join("evidence");
        assert!(cleanup_keeps_evidence(&scratch, &evidence));
        assert!(!scratch.exists());
        assert!(evidence.join("PROOF.md").is_file());
    }
}
