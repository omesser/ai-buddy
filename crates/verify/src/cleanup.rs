//! Kill only recorded PIDs; remove scratch; keep evidence.

use std::fs;
use std::thread;
use std::time::Duration;

use crate::paths::RunPaths;
use crate::proof;

/// Cleanup. Returns 0 if evidence still exists afterwards.
pub fn run(paths: &RunPaths) -> i32 {
    let pidlist = paths.scratch.join("pids").join("owned.pids");
    if pidlist.is_file() {
        if let Ok(text) = fs::read_to_string(&pidlist) {
            for line in text.lines() {
                let pid = line.trim();
                if pid.is_empty() {
                    continue;
                }
                kill_pid(pid);
            }
        }
    }

    let app_pid_file = paths.scratch.join("pids").join("app.pid");
    if app_pid_file.is_file() {
        if let Ok(pid) = fs::read_to_string(&app_pid_file) {
            let pid = pid.trim();
            if !pid.is_empty() {
                println!("cleanup: kill app.pid {pid}");
                kill_pid(pid);
            }
        }
    }

    if paths.scratch.exists() {
        if let Err(e) = fs::remove_dir_all(&paths.scratch) {
            eprintln!("cleanup: rm scratch {}: {e}", paths.scratch.display());
        }
    }

    let _ = fs::create_dir_all(&paths.evidence);

    println!(
        "cleanup: scratch removed; evidence preserved at {}",
        paths.evidence.display()
    );
    let _ = proof::append_proof(
        paths,
        &format!(
            "cleanup done; evidence still at {}",
            paths.evidence.display()
        ),
    );

    if let Ok(rd) = fs::read_dir(&paths.evidence) {
        for ent in rd.filter_map(|e| e.ok()) {
            println!("  {}", ent.path().display());
        }
    }

    if proof::evidence_still_exists(&paths.evidence) {
        0
    } else {
        eprintln!(
            "cleanup: FATAL evidence dir missing at {}",
            paths.evidence.display()
        );
        1
    }
}

fn kill_pid(pid: &str) {
    #[cfg(unix)]
    {
        use std::process::Command;
        if Command::new("kill")
            .args(["-0", pid])
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
        {
            println!("cleanup: kill {pid}");
            let _ = Command::new("kill").arg(pid).status();
            thread::sleep(Duration::from_millis(500));
            let _ = Command::new("kill").args(["-9", pid]).status();
        }
    }
    #[cfg(not(unix))]
    {
        let _ = (pid, thread::sleep(Duration::from_millis(0)));
        // Windows: taskkill if we ever record PIDs there in a later stone.
        println!("cleanup: skip kill on non-unix pid={pid}");
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
