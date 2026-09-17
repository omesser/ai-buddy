//! Shared orchestration for the `poke` and `summon` subcommands.
//!
//! Both gestures need the same rig: build, launch the app traced, wait for
//! the sprite to settle, read its position from the app's own log, click it
//! for real via `scripts/click-cursor.swift`, and assert the resulting
//! `verbs:` line. Only the click count and the verb name differ, so both
//! subcommands share this driver rather than duplicating it (#647 stone 2).
//!
//! macOS only for now — the leaf that posts a real click
//! (`scripts/click-cursor.swift`) is macOS-only. Other hosts SKIP (exit 2)
//! rather than reporting a pass they did not earn.

use std::fs;
use std::path::Path;
use std::process::Command;
use std::time::Duration;

use crate::contract::{Outcome, RunReport};

#[derive(Clone, Copy)]
pub enum Verb {
    Poke,
    Summon,
}

impl Verb {
    fn name(self) -> &'static str {
        match self {
            Verb::Poke => "Poke",
            Verb::Summon => "Summon",
        }
    }

    fn clicks(self) -> u32 {
        match self {
            Verb::Poke => 1,
            Verb::Summon => 2,
        }
    }
}

/// Drive one gesture end to end.
pub fn run(verb: Verb, repo_root: &Path, report: &mut RunReport) {
    if std::env::consts::OS != "macos" {
        report.check(
            Outcome::Skip,
            "gesture lane",
            &format!(
                "no gesture leaf wired for {} yet (macOS is stone 2's proven platform)",
                std::env::consts::OS
            ),
        );
        return;
    }

    let dest = report.paths().evidence.join(verb.name().to_lowercase());
    if let Err(e) = fs::create_dir_all(&dest) {
        report.check(
            Outcome::Fail,
            "evidence dir",
            &format!("cannot create {}: {e}", dest.display()),
        );
        return;
    }

    let click_script = repo_root.join("scripts/click-cursor.swift");
    if !click_script.is_file() {
        report.check(
            Outcome::Fail,
            "click leaf",
            &format!("missing {}", click_script.display()),
        );
        return;
    }

    report.say(&format!("{}: building...", verb.name()));
    let mut build = Command::new("cargo");
    build
        .args(["build", "-p", "ai-buddy"])
        .current_dir(repo_root);
    match report.exec(&mut build, None) {
        Some(0) => {}
        Some(c) => {
            report.check(Outcome::Fail, "cargo build", &format!("exited {c}"));
            return;
        }
        None => {
            report.check(Outcome::Fail, "cargo build", "could not run cargo");
            return;
        }
    }

    // A stray instance of this exact checkout's binary would confuse which
    // app.log and which verbs line belongs to this run. Named as a failure
    // rather than killed: a broad `pkill -f target/debug/ai-buddy` would also
    // catch another worktree's dogfood instance or another agent's run, and
    // this tool only ever owns the child it spawns below.
    let bin_path = repo_root.join("target/debug/ai-buddy");
    if let Some(pid) = stray_pid(&bin_path) {
        report.check(
            Outcome::Fail,
            "stray process",
            &format!(
                "{} is already running (pid {pid}); stop it before running {}",
                bin_path.display(),
                verb.name().to_lowercase()
            ),
        );
        return;
    }

    let log_path = dest.join("app.log");
    let log_out = match fs::File::create(&log_path) {
        Ok(f) => f,
        Err(e) => {
            report.check(
                Outcome::Fail,
                "app log",
                &format!("cannot create {}: {e}", log_path.display()),
            );
            return;
        }
    };
    let log_err = match log_out.try_clone() {
        Ok(f) => f,
        Err(e) => {
            report.check(
                Outcome::Fail,
                "app log",
                &format!("cannot dup log handle: {e}"),
            );
            return;
        }
    };

    let mut child = match Command::new(&bin_path)
        .env("AI_BUDDY_TRACE_HITTEST", "1")
        .env("AI_BUDDY_TRACE_FRAMES", "1")
        .current_dir(repo_root)
        .stdout(log_out)
        .stderr(log_err)
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            report.check(Outcome::Fail, "ai-buddy launch", &format!("spawn: {e}"));
            return;
        }
    };

    drive(verb, repo_root, &click_script, &log_path, report);

    let _ = child.kill();
    let _ = child.wait();
}

fn drive(
    verb: Verb,
    repo_root: &Path,
    click_script: &Path,
    log_path: &Path,
    report: &mut RunReport,
) {
    if !await_line(log_path, 60, |l| l.starts_with("overlay:")) {
        report.check(
            Outcome::Fail,
            "overlay line",
            "app never published an overlay line",
        );
        return;
    }
    if !await_line(log_path, 60, |l| {
        l.starts_with("frame: ") && (l.contains(" Grounded ") || l.contains(" Perched "))
    }) {
        report.check(Outcome::Fail, "sprite settled", "no Grounded/Perched frame");
        return;
    }

    let Some((x, y)) = sprite_click_point(log_path) else {
        report.check(
            Outcome::Fail,
            "sprite position",
            "could not read the sprite's position/size from the log",
        );
        return;
    };

    report.say(&format!(
        "{}: clicking sprite at ({x},{y}) x{}",
        verb.name(),
        verb.clicks()
    ));
    let mut click = Command::new("swift");
    click
        .arg(click_script)
        .args([x.to_string(), y.to_string(), verb.clicks().to_string()])
        .current_dir(repo_root);
    match report.exec(&mut click, None) {
        Some(0) => {}
        Some(c) => {
            report.check(
                Outcome::Fail,
                "click",
                &format!("click-cursor.swift exited {c}"),
            );
            return;
        }
        None => {
            report.check(Outcome::Fail, "click", "could not run click-cursor.swift");
            return;
        }
    }

    let verb_name = verb.name();
    if !await_line(log_path, 40, |l| {
        l.starts_with("verbs:") && l.contains(verb_name)
    }) {
        report.check(
            Outcome::Fail,
            "verbs line",
            &format!("no `verbs:.*{verb_name}` line; the click did not produce the verb"),
        );
        return;
    }
    report.check(
        Outcome::Pass,
        &verb_name.to_lowercase(),
        &format!("verbs:.*{verb_name} in {}", log_path.display()),
    );
}

/// Pid of an already-running process at `bin_path`, if any. Exact-path match:
/// this scopes to the one checkout `run` is about to build and launch, never
/// another worktree's binary or the user's own dogfood app.
fn stray_pid(bin_path: &Path) -> Option<String> {
    let out = Command::new("pgrep")
        .args(["-f", &bin_path.to_string_lossy()])
        .output()
        .ok()?;
    let pid = String::from_utf8_lossy(&out.stdout)
        .lines()
        .next()?
        .trim()
        .to_string();
    (!pid.is_empty()).then_some(pid)
}

/// Poll the log every 250ms up to `attempts` times for a line matching `pred`.
fn await_line(log_path: &Path, attempts: u32, pred: impl Fn(&str) -> bool) -> bool {
    for _ in 0..attempts {
        if let Ok(contents) = fs::read_to_string(log_path) {
            if contents.lines().any(&pred) {
                return true;
            }
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    false
}

/// The centre of the sprite's drawn art, from the app's own trace lines:
/// the startup `sprite {w}x{h}` size and the latest frame's `sprite(x,y)`
/// top-left corner. Centre rather than the corner so the click lands on
/// drawn (non-transparent) pixels, matching the hit-test the app itself does.
fn sprite_click_point(log_path: &Path) -> Option<(i64, i64)> {
    let log = fs::read_to_string(log_path).ok()?;
    sprite_click_point_impl(&log)
}

fn sprite_click_point_impl(log: &str) -> Option<(i64, i64)> {
    let (w, h) = log.lines().find_map(|line| {
        let after = line.strip_prefix("overlay: ")?;
        let idx = after.find("sprite ")?;
        let rest = &after[idx + "sprite ".len()..];
        let dims = rest.split(';').next()?.trim();
        let (w, h) = dims.split_once('x')?;
        Some((w.trim().parse::<i64>().ok()?, h.trim().parse::<i64>().ok()?))
    })?;

    let (sx, sy) = log.lines().rev().find_map(|line| {
        if !line.starts_with("frame: ") {
            return None;
        }
        let idx = line.find("sprite(")?;
        let rest = &line[idx + "sprite(".len()..];
        let close = rest.find(')')?;
        let (x, y) = rest[..close].split_once(',')?;
        Some((x.trim().parse::<i64>().ok()?, y.trim().parse::<i64>().ok()?))
    })?;

    Some((sx + w / 2, sy + h / 2))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sprite_click_point_centres_on_the_latest_frame() {
        let log = "overlay: 1 display(s); sprite 32x32; other stuff\n\
                    frame: 100 Falling pos(500,10) sprite(400,10) fall#0 buddy-1\n\
                    frame: 200 Grounded pos(500,300) sprite(400,300) idle#0 buddy-1\n";
        assert_eq!(sprite_click_point_impl(log), Some((400 + 16, 300 + 16)));
    }

    #[test]
    fn sprite_click_point_is_none_without_a_frame_line() {
        let log = "overlay: 1 display(s); sprite 32x32; other stuff\n";
        assert_eq!(sprite_click_point_impl(log), None);
    }
}
