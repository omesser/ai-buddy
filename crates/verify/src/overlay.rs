//! Dispatch platform overlay leaf scripts and collect stamp dirs into evidence.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::paths::{self, RunPaths};
use crate::proof;

/// Run the OS-appropriate overlay script; copy stamps into evidence/overlay/.
///
/// Exit code matches the leaf script. Failures (including EWMH/xprop gaps) are
/// recorded in PROOF — no invented human chore lists.
pub fn run(repo_root: &Path, paths: &RunPaths) -> i32 {
    let Some(script_rel) = paths::overlay_script_for_host() else {
        let msg = format!("overlay SKIP — unsupported OS {}", std::env::consts::OS);
        println!("{msg}");
        let _ = proof::append_proof(paths, &msg);
        return 2;
    };

    let script = repo_root.join(script_rel);
    if !script.is_file() {
        let msg = format!("overlay FAIL — missing {}", script.display());
        eprintln!("{msg}");
        let _ = proof::append_proof(paths, &msg);
        return 1;
    }

    let dest = paths.evidence.join("overlay");
    let _ = fs::create_dir_all(&dest);

    let before = list_stamp_dirs(repo_root);

    println!("overlay: running {script_rel}");
    let status = run_overlay_script(repo_root, script_rel, &script);
    let code = status.unwrap_or(1);

    let after = list_stamp_dirs(repo_root);
    copy_new_stamps(repo_root, &before, &after, &dest);

    let msg = if code == 0 {
        format!(
            "overlay PASS ({script_rel}) — evidence under {}",
            dest.display()
        )
    } else {
        format!(
            "overlay FAIL exit={code} ({script_rel}) — see {} (script failure or host GUI/EWMH gap; no chore list)",
            dest.display()
        )
    };
    println!("{msg}");
    let _ = proof::append_proof(paths, &msg);
    code
}

fn run_overlay_script(repo_root: &Path, script_rel: &str, script: &Path) -> Option<i32> {
    let mut cmd = if script_rel.ends_with(".ps1") {
        let mut c = Command::new("powershell");
        c.args([
            "-NoProfile",
            "-ExecutionPolicy",
            "Bypass",
            "-File",
            &script.to_string_lossy(),
        ]);
        c
    } else {
        let mut c = Command::new("bash");
        c.arg(script);
        c
    };
    cmd.current_dir(repo_root);
    match cmd.status() {
        Ok(s) => Some(s.code().unwrap_or(1)),
        Err(e) => {
            eprintln!("overlay: failed to spawn: {e}");
            None
        }
    }
}

/// Stamp dirs under `.verify/`: `x11-*`, `win-*`, or bare timestamp dirs (macOS).
fn list_stamp_dirs(repo_root: &Path) -> Vec<PathBuf> {
    let verify = repo_root.join(".verify");
    let mut out = Vec::new();
    let Ok(rd) = fs::read_dir(&verify) else {
        return out;
    };
    for ent in rd.filter_map(|e| e.ok()) {
        let p = ent.path();
        if !p.is_dir() {
            continue;
        }
        let name = ent.file_name().to_string_lossy().into_owned();
        if name.starts_with("x11-") || name.starts_with("win-") || looks_like_macos_stamp(&name) {
            out.push(p);
        }
    }
    out.sort();
    out
}

fn looks_like_macos_stamp(name: &str) -> bool {
    // macOS script uses OUT=".verify/$STAMP" with YYYYMMDD-HHMMSS
    name.len() >= 15
        && name
            .as_bytes()
            .iter()
            .all(|b| b.is_ascii_digit() || *b == b'-')
        && !name.starts_with("x11-")
        && !name.starts_with("win-")
}

fn copy_new_stamps(repo_root: &Path, before: &[PathBuf], after: &[PathBuf], dest: &Path) {
    let new: Vec<_> = after
        .iter()
        .filter(|p| !before.contains(p))
        .cloned()
        .collect();
    let to_copy: Vec<PathBuf> = if !new.is_empty() {
        new
    } else {
        // Fallback: newest matching stamp for this OS
        newest_stamp(repo_root).into_iter().collect()
    };
    for dir in to_copy {
        let name = dir.file_name().unwrap_or_default();
        let target = dest.join(name);
        if let Err(e) = copy_dir_recursive(&dir, &target) {
            eprintln!(
                "overlay: copy {} → {}: {e}",
                dir.display(),
                target.display()
            );
        } else {
            println!("overlay: copied {} → {}", dir.display(), target.display());
        }
    }
}

fn newest_stamp(repo_root: &Path) -> Option<PathBuf> {
    let mut dirs = list_stamp_dirs(repo_root);
    dirs.pop()
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> std::io::Result<()> {
    fs::create_dir_all(dst)?;
    for ent in fs::read_dir(src)? {
        let ent = ent?;
        let ty = ent.file_type()?;
        let from = ent.path();
        let to = dst.join(ent.file_name());
        if ty.is_dir() {
            copy_dir_recursive(&from, &to)?;
        } else {
            fs::copy(&from, &to)?;
        }
    }
    Ok(())
}
