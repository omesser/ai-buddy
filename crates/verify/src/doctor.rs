//! Read-only readiness checks mirroring helpers/doctor.sh.
//!
//! Hard FAILs only for: missing workspace layout, or no binary and no cargo.
//! OS tool gaps are SKIPs (stone 0).

use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;
#[cfg(windows)]
use std::process::Stdio;

use crate::contract::{Outcome, RunReport};

/// Run doctor.
pub fn run(repo_root: &Path, report: &mut RunReport) {
    report.say(&format!(
        "doctor: repo={} RUN_ID={}",
        repo_root.display(),
        report.paths().run_id
    ));
    report.say(&format!(
        "doctor: evidence={}",
        report.paths().evidence.display()
    ));

    if repo_root.join("Cargo.toml").is_file() && repo_root.join("src-tauri").is_dir() {
        report.check(Outcome::Pass, "workspace layout", "Cargo.toml + src-tauri");
    } else {
        report.check(
            Outcome::Fail,
            "workspace layout",
            "not an ai-buddy checkout",
        );
    }

    match ai_buddy_bin(repo_root) {
        Some(bin) => report.check(
            Outcome::Pass,
            "ai-buddy binary",
            &format!("present: {}", bin.display()),
        ),
        None => {
            if command_on_path("cargo") {
                report.check(
                    Outcome::Pass,
                    "ai-buddy binary",
                    "not built yet; cargo is available to build",
                );
            } else {
                report.check(
                    Outcome::Fail,
                    "ai-buddy binary",
                    "no ai-buddy binary and no cargo",
                );
            }
        }
    }

    match env::consts::OS {
        "macos" => {
            if command_on_path("swift") {
                report.check(Outcome::Pass, "swift", "on PATH");
            } else {
                report.check(Outcome::Skip, "swift", "not on PATH (macOS verify-overlay)");
            }
        }
        "linux" => linux_tool_checks(report),
        "windows" => report.check(
            Outcome::Pass,
            "host os",
            "windows; overlay via scripts/verify-overlay-win.ps1",
        ),
        other => report.check(Outcome::Skip, "host os", &format!("unknown OS {other}")),
    }

    if let Ok(app_pid) = env::var("APP_PID") {
        if !app_pid.is_empty() {
            if pid_alive(&app_pid) {
                report.check(Outcome::Pass, "APP_PID", &format!("{app_pid} alive"));
            } else {
                // Soft: APP_PID is optional context from helpers.
                report.check(Outcome::Skip, "APP_PID", &format!("{app_pid} not running"));
            }
        }
    }

    if report.outcome() == Outcome::Pass {
        report.say("doctor: OK");
    } else {
        let fails = report
            .checks()
            .iter()
            .filter(|c| c.outcome == Outcome::Fail)
            .count();
        report.say(&format!("doctor: {fails} failure(s)"));
    }
}

fn linux_tool_checks(report: &mut RunReport) {
    match env::var("DISPLAY") {
        Ok(d) if !d.is_empty() => report.check(Outcome::Pass, "DISPLAY", &d),
        _ => report.check(
            Outcome::Skip,
            "DISPLAY",
            "unset (X11 overlay drive needs xvfb-run or a session)",
        ),
    }
    for t in ["xdotool", "xprop", "xwininfo"] {
        if command_on_path(t) {
            report.check(Outcome::Pass, t, "on PATH");
        } else {
            report.check(
                Outcome::Skip,
                t,
                "not on PATH (needed for verify-overlay-x11)",
            );
        }
    }
    if command_on_path("xterm") {
        report.check(Outcome::Pass, "xterm", "on PATH");
    } else {
        report.check(
            Outcome::Skip,
            "xterm",
            "not on PATH (verify-overlay-x11 perch prop); unit proof still OK",
        );
    }
    if env::var("DISPLAY").map(|d| !d.is_empty()).unwrap_or(false) {
        if supporting_wm_published() {
            report.check(Outcome::Pass, "supporting WM", "published");
        } else if command_on_path("openbox") {
            report.check(
                Outcome::Skip,
                "supporting WM",
                "none yet; openbox is installed (script can start it)",
            );
        } else {
            report.check(
                Outcome::Skip,
                "supporting WM",
                "none and openbox not installed (Xvfb needs openbox); overlay drive blocked",
            );
        }
    }
    if ayatana_present() {
        report.check(Outcome::Pass, "libayatana-appindicator3", "present");
    } else {
        report.check(
            Outcome::Skip,
            "libayatana-appindicator3",
            "missing; overlay panics on tray init (apt install libayatana-appindicator3-1)",
        );
    }
}

fn supporting_wm_published() -> bool {
    let out = Command::new("xprop")
        .args(["-root", "_NET_SUPPORTING_WM_CHECK"])
        .output();
    match out {
        Ok(o) => String::from_utf8_lossy(&o.stdout).contains("window id"),
        Err(_) => false,
    }
}

fn ayatana_present() -> bool {
    if Path::new("/usr/lib/x86_64-linux-gnu/libayatana-appindicator3.so.1").exists() {
        return true;
    }
    let out = Command::new("ldconfig").arg("-p").output();
    match out {
        Ok(o) => String::from_utf8_lossy(&o.stdout).contains("ayatana-appindicator3"),
        Err(_) => false,
    }
}

/// Relative paths under the repo root for a built `ai-buddy` binary (with EXE_SUFFIX).
fn ai_buddy_bin_relpaths() -> [String; 2] {
    let suffix = env::consts::EXE_SUFFIX;
    [
        format!("target/release/ai-buddy{suffix}"),
        format!("target/debug/ai-buddy{suffix}"),
    ]
}

fn ai_buddy_bin(repo_root: &Path) -> Option<PathBuf> {
    for rel in ai_buddy_bin_relpaths() {
        let p = repo_root.join(rel);
        if p.is_file() {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                if let Ok(meta) = p.metadata() {
                    if meta.permissions().mode() & 0o111 != 0 {
                        return Some(p);
                    }
                }
            }
            #[cfg(not(unix))]
            {
                return Some(p);
            }
        }
    }
    None
}

/// True if `name` resolves on PATH (Unix: `command -v` via sh; Windows: `where`).
fn command_on_path(name: &str) -> bool {
    #[cfg(windows)]
    {
        Command::new("where")
            .arg(name)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }
    #[cfg(not(windows))]
    {
        Command::new("sh")
            .args(["-c", &format!("command -v {name} >/dev/null 2>&1")])
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }
}

fn pid_alive(pid: &str) -> bool {
    #[cfg(unix)]
    {
        Command::new("kill")
            .args(["-0", pid])
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }
    #[cfg(not(unix))]
    {
        let _ = pid;
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ai_buddy_bin_relpaths_include_exe_suffix_on_windows() {
        let paths = ai_buddy_bin_relpaths();
        assert_eq!(
            paths[0],
            format!("target/release/ai-buddy{}", env::consts::EXE_SUFFIX)
        );
        assert_eq!(
            paths[1],
            format!("target/debug/ai-buddy{}", env::consts::EXE_SUFFIX)
        );
        #[cfg(windows)]
        {
            assert!(paths[0].ends_with(".exe"));
            assert!(paths[1].ends_with(".exe"));
        }
        #[cfg(not(windows))]
        {
            assert!(!paths[0].ends_with(".exe"));
            assert!(!paths[1].ends_with(".exe"));
        }
    }

    #[test]
    fn command_on_path_finds_a_known_command() {
        #[cfg(windows)]
        {
            assert!(
                command_on_path("cmd") || command_on_path("where"),
                "expected cmd or where on Windows PATH"
            );
        }
        #[cfg(not(windows))]
        {
            assert!(
                command_on_path("sh") || command_on_path("true"),
                "expected sh or true on Unix PATH"
            );
        }
    }
}
