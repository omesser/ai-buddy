//! Run root / evidence / scratch resolution and repo-root discovery.
//!
//! Default layout:
//!   `$TMPDIR/ai-buddy-verify-$RUN_ID/{evidence,scratch}`
//!   (falls back to `/tmp/...` when `TMPDIR` is unset)
//!
//! `--evidence-dir PATH` overrides only the evidence directory. Scratch is
//! always the sibling `scratch` next to that evidence path's parent:
//!   evidence = PATH
//!   scratch  = PATH.parent()/scratch
//! So callers should pass `<run-root>/evidence` (matching the default layout)
//! so cleanup can `rm -rf` scratch without touching evidence.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process;
use std::time::{SystemTime, UNIX_EPOCH};

/// Paths for one verify run.
#[derive(Debug, Clone)]
pub struct RunPaths {
    pub run_id: String,
    /// Run root when using the default layout; parent of evidence when overridden.
    pub root: PathBuf,
    pub evidence: PathBuf,
    pub scratch: PathBuf,
}

impl RunPaths {
    /// Resolve paths from optional CLI overrides.
    pub fn resolve(evidence_dir: Option<PathBuf>, run_id: Option<String>) -> Self {
        let run_id = run_id.unwrap_or_else(default_run_id);

        if let Some(evidence) = evidence_dir {
            let evidence = if evidence.as_os_str().is_empty() {
                PathBuf::from(".")
            } else {
                evidence
            };
            let parent = evidence
                .parent()
                .filter(|p| !p.as_os_str().is_empty())
                .map(Path::to_path_buf)
                .unwrap_or_else(|| PathBuf::from("."));
            let scratch = parent.join("scratch");
            Self {
                run_id,
                root: parent,
                evidence,
                scratch,
            }
        } else {
            let tmp = env::var_os("TMPDIR")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("/tmp"));
            let root = tmp.join(format!("ai-buddy-verify-{run_id}"));
            let evidence = root.join("evidence");
            let scratch = root.join("scratch");
            Self {
                run_id,
                root,
                evidence,
                scratch,
            }
        }
    }

    /// Ensure evidence and scratch/pids exist.
    pub fn ensure_dirs(&self) -> std::io::Result<()> {
        fs::create_dir_all(&self.evidence)?;
        fs::create_dir_all(self.scratch.join("pids"))?;
        Ok(())
    }
}

fn default_run_id() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    // Compact local-ish stamp: YYYYMMDD-HHMMSS-pid (UTC seconds formatted).
    // Match helper style without pulling chrono.
    let datetime = format_utc_compact(secs);
    format!("{datetime}-{}", process::id())
}

fn format_utc_compact(secs: u64) -> String {
    // Civil UTC from unix seconds (adequate for run ids).
    let days = secs / 86400;
    let tod = secs % 86400;
    let (y, m, d) = civil_from_days(days as i64);
    let hh = tod / 3600;
    let mm = (tod % 3600) / 60;
    let ss = tod % 60;
    format!("{y:04}{m:02}{d:02}-{hh:02}{mm:02}{ss:02}")
}

/// Howard Hinnant's civil_from_days (UTC calendar date from days since 1970-01-01).
pub(crate) fn civil_from_days(z: i64) -> (i32, u32, u32) {
    let z = z + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = (yoe as i64) + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y as i32, m as u32, d as u32)
}

/// Walk up from `start` looking for `Cargo.toml` + `src-tauri/`.
pub fn find_repo_root(start: &Path) -> Option<PathBuf> {
    let mut cur = start.to_path_buf();
    loop {
        if cur.join("Cargo.toml").is_file() && cur.join("src-tauri").is_dir() {
            return Some(cur);
        }
        if !cur.pop() {
            return None;
        }
    }
}

/// Prefer `current_dir`, then walk from the executable's ancestors.
pub fn discover_repo_root() -> Result<PathBuf, String> {
    if let Ok(cwd) = env::current_dir() {
        if let Some(root) = find_repo_root(&cwd) {
            return Ok(root);
        }
    }
    if let Ok(exe) = env::current_exe() {
        if let Some(parent) = exe.parent() {
            if let Some(root) = find_repo_root(parent) {
                return Ok(root);
            }
        }
    }
    Err(
        "could not find ai-buddy repo root (need Cargo.toml + src-tauri); \
         run from a checkout or install beside one"
            .into(),
    )
}

/// Which overlay leaf script to run for this host OS.
pub fn overlay_script_for_os(os: &str) -> Option<&'static str> {
    match os {
        "macos" => Some("scripts/verify-overlay.sh"),
        "linux" => Some("scripts/verify-overlay-x11.sh"),
        "windows" => Some("scripts/verify-overlay-win.ps1"),
        _ => None,
    }
}

/// `std::env::consts::OS` → overlay script path relative to repo root.
pub fn overlay_script_for_host() -> Option<&'static str> {
    overlay_script_for_os(env::consts::OS)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn default_paths_use_tmpdir_and_run_id() {
        let paths = RunPaths::resolve(None, Some("testrun".into()));
        assert!(paths.root.ends_with(Path::new("ai-buddy-verify-testrun")));
        assert_eq!(paths.evidence, paths.root.join("evidence"));
        assert_eq!(paths.scratch, paths.root.join("scratch"));
        assert_eq!(paths.run_id, "testrun");
    }

    #[test]
    fn evidence_dir_override_puts_scratch_beside_parent() {
        let dir = tempdir().unwrap();
        let evidence = dir.path().join("my-run").join("evidence");
        let paths = RunPaths::resolve(Some(evidence.clone()), Some("x".into()));
        assert_eq!(paths.evidence, evidence);
        assert_eq!(paths.scratch, dir.path().join("my-run").join("scratch"));
        assert_eq!(paths.root, dir.path().join("my-run"));
    }

    #[test]
    fn find_repo_root_walks_up() {
        let dir = tempdir().unwrap();
        let root = dir.path().join("repo");
        fs::create_dir_all(root.join("src-tauri")).unwrap();
        fs::write(root.join("Cargo.toml"), "[workspace]\n").unwrap();
        let nested = root.join("a").join("b");
        fs::create_dir_all(&nested).unwrap();
        assert_eq!(find_repo_root(&nested).as_deref(), Some(root.as_path()));
    }

    #[test]
    fn overlay_script_selection_by_os() {
        assert_eq!(
            overlay_script_for_os("macos"),
            Some("scripts/verify-overlay.sh")
        );
        assert_eq!(
            overlay_script_for_os("linux"),
            Some("scripts/verify-overlay-x11.sh")
        );
        assert_eq!(
            overlay_script_for_os("windows"),
            Some("scripts/verify-overlay-win.ps1")
        );
        assert_eq!(overlay_script_for_os("freebsd"), None);
    }
}
