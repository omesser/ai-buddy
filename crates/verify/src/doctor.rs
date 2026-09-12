//! Read-only readiness checks mirroring helpers/doctor.sh.
//!
//! Hard FAILs only for: missing workspace layout, or no binary and no cargo.
//! OS tool gaps are WARNs (stone 0).

use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::paths::RunPaths;
use crate::proof;

/// Run doctor. Returns process exit code (0 = no hard fails).
pub fn run(repo_root: &Path, paths: &RunPaths) -> i32 {
    let mut fails = 0u32;

    println!(
        "doctor: repo={} RUN_ID={}",
        repo_root.display(),
        paths.run_id
    );
    println!("doctor: evidence={}", paths.evidence.display());

    if repo_root.join("Cargo.toml").is_file() && repo_root.join("src-tauri").is_dir() {
        pass("workspace layout (Cargo.toml + src-tauri)");
    } else {
        fail("not an ai-buddy checkout");
        fails += 1;
    }

    match ai_buddy_bin(repo_root) {
        Some(bin) => pass(&format!("binary present: {}", bin.display())),
        None => {
            if command_on_path("cargo") {
                pass("no binary yet; cargo is available to build");
            } else {
                fail("no ai-buddy binary and no cargo");
                fails += 1;
            }
        }
    }

    match env::consts::OS {
        "macos" => {
            if command_on_path("swift") {
                pass("swift on PATH");
            } else {
                warn("swift missing (macOS verify-overlay)");
            }
        }
        "linux" => linux_tool_checks(),
        "windows" => {
            pass("Windows host — use scripts/verify-overlay-win.ps1");
        }
        other => warn(&format!("unknown OS {other}")),
    }

    if let Ok(app_pid) = env::var("APP_PID") {
        if !app_pid.is_empty() {
            if pid_alive(&app_pid) {
                pass(&format!("APP_PID={app_pid} alive"));
            } else {
                // Soft: APP_PID is optional context from helpers; warn only.
                warn(&format!("APP_PID={app_pid} not running"));
            }
        }
    }

    if fails == 0 {
        println!("doctor: OK");
        let _ = proof::append_proof(
            paths,
            &format!("doctor OK evidence={}", paths.evidence.display()),
        );
        0
    } else {
        println!("doctor: {fails} failure(s)");
        let _ = proof::append_proof(
            paths,
            &format!("doctor FAILED ({fails}) — do not Drive until fixed"),
        );
        1
    }
}

fn linux_tool_checks() {
    match env::var("DISPLAY") {
        Ok(d) if !d.is_empty() => pass(&format!("DISPLAY={d}")),
        _ => warn("DISPLAY unset (X11 overlay drive needs xvfb-run or a session)"),
    }
    for t in ["xdotool", "xprop", "xwininfo"] {
        if command_on_path(t) {
            pass(&format!("{t} on PATH"));
        } else {
            warn(&format!("{t} missing (needed for verify-overlay-x11)"));
        }
    }
    if command_on_path("xterm") {
        pass("xterm on PATH");
    } else {
        warn("xterm missing (verify-overlay-x11 perch prop) — unit proof still OK");
    }
    if env::var("DISPLAY").map(|d| !d.is_empty()).unwrap_or(false) {
        if supporting_wm_published() {
            pass("supporting WM published");
        } else if command_on_path("openbox") {
            warn("no supporting WM yet; openbox is installed (script can start it)");
        } else {
            warn("no supporting WM and openbox not installed (Xvfb needs openbox) — overlay drive blocked");
        }
    }
    if ayatana_present() {
        pass("libayatana-appindicator3 present");
    } else {
        warn(
            "libayatana-appindicator3 missing — overlay panics on tray init (apt install libayatana-appindicator3-1)",
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

fn ai_buddy_bin(repo_root: &Path) -> Option<PathBuf> {
    for rel in ["target/release/ai-buddy", "target/debug/ai-buddy"] {
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

fn command_on_path(name: &str) -> bool {
    Command::new("sh")
        .args(["-c", &format!("command -v {name} >/dev/null 2>&1")])
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
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

fn pass(msg: &str) {
    println!("  PASS  {msg}");
}
fn fail(msg: &str) {
    println!("  FAIL  {msg}");
}
fn warn(msg: &str) {
    println!("  WARN  {msg}");
}
