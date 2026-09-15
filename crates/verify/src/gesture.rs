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

use crate::paths::RunPaths;
use crate::proof;

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

/// Drive one gesture end to end. Returns 0 pass, 1 fail, 2 skip.
pub fn run(verb: Verb, repo_root: &Path, paths: &RunPaths) -> i32 {
    if std::env::consts::OS != "macos" {
        return skip(
            verb,
            paths,
            &format!(
                "no gesture leaf wired for {} yet (macOS is stone 2's proven platform)",
                std::env::consts::OS
            ),
        );
    }

    let dest = paths.evidence.join(verb.name().to_lowercase());
    if let Err(e) = fs::create_dir_all(&dest) {
        return fail(
            verb,
            paths,
            &format!("cannot create {}: {e}", dest.display()),
        );
    }

    let click_script = repo_root.join("scripts/click-cursor.swift");
    if !click_script.is_file() {
        return fail(verb, paths, &format!("missing {}", click_script.display()));
    }

    println!("{}: building...", verb.name());
    match Command::new("cargo")
        .args(["build", "-p", "ai-buddy"])
        .current_dir(repo_root)
        .status()
    {
        Ok(s) if s.success() => {}
        Ok(s) => return fail(verb, paths, &format!("cargo build exited {s}")),
        Err(e) => return fail(verb, paths, &format!("cargo build: {e}")),
    }

    // Only ever races a stray instance this same script left behind; cleanup
    // (below) kills the one this run starts, same convention as
    // verify-overlay.sh's own trap.
    let _ = Command::new("pkill")
        .args(["-f", "target/debug/ai-buddy"])
        .status();

    let log_path = dest.join("app.log");
    let log_out = match fs::File::create(&log_path) {
        Ok(f) => f,
        Err(e) => {
            return fail(
                verb,
                paths,
                &format!("cannot create {}: {e}", log_path.display()),
            )
        }
    };
    let log_err = match log_out.try_clone() {
        Ok(f) => f,
        Err(e) => return fail(verb, paths, &format!("cannot dup log handle: {e}")),
    };

    let mut child = match Command::new(repo_root.join("target/debug/ai-buddy"))
        .env("AI_BUDDY_TRACE_HITTEST", "1")
        .env("AI_BUDDY_TRACE_FRAMES", "1")
        .current_dir(repo_root)
        .stdout(log_out)
        .stderr(log_err)
        .spawn()
    {
        Ok(c) => c,
        Err(e) => return fail(verb, paths, &format!("spawn ai-buddy: {e}")),
    };

    let code = drive(verb, repo_root, &click_script, &log_path);

    let _ = child.kill();
    let _ = child.wait();

    if code == 0 {
        let msg = format!("{} PASS — evidence under {}", verb.name(), dest.display());
        println!("{msg}");
        let _ = proof::append_proof(paths, &msg);
    } else {
        let msg = format!("{} FAIL — see {}", verb.name(), dest.display());
        println!("{msg}");
        let _ = proof::append_proof(paths, &msg);
    }
    code
}

fn drive(verb: Verb, repo_root: &Path, click_script: &Path, log_path: &Path) -> i32 {
    if !await_line(log_path, 60, |l| l.starts_with("overlay:")) {
        eprintln!("{}: app never published an overlay line", verb.name());
        return 1;
    }
    if !await_line(log_path, 60, |l| {
        l.starts_with("frame: ") && (l.contains(" Grounded ") || l.contains(" Perched "))
    }) {
        eprintln!(
            "{}: sprite never settled (no Grounded/Perched frame)",
            verb.name()
        );
        return 1;
    }

    let Some((x, y)) = sprite_click_point(log_path) else {
        eprintln!(
            "{}: could not read the sprite's position/size from the log",
            verb.name()
        );
        return 1;
    };

    println!(
        "{}: clicking sprite at ({x},{y}) x{}",
        verb.name(),
        verb.clicks()
    );
    match Command::new("swift")
        .arg(click_script)
        .args([x.to_string(), y.to_string(), verb.clicks().to_string()])
        .current_dir(repo_root)
        .status()
    {
        Ok(s) if s.success() => {}
        Ok(s) => {
            eprintln!("{}: click-cursor.swift exited {s}", verb.name());
            return 1;
        }
        Err(e) => {
            eprintln!("{}: could not run click-cursor.swift: {e}", verb.name());
            return 1;
        }
    }

    let verb_name = verb.name();
    if !await_line(log_path, 40, |l| {
        l.starts_with("verbs:") && l.contains(verb_name)
    }) {
        eprintln!(
            "{}: no `verbs:.*{verb_name}` line — click did not produce the verb",
            verb.name()
        );
        return 1;
    }
    println!(
        "{}: verbs:.*{verb_name} found in {}",
        verb.name(),
        log_path.display()
    );
    0
}

fn skip(verb: Verb, paths: &RunPaths, reason: &str) -> i32 {
    let msg = format!("{} SKIP — {reason}", verb.name());
    println!("{msg}");
    let _ = proof::append_proof(paths, &msg);
    2
}

fn fail(verb: Verb, paths: &RunPaths, reason: &str) -> i32 {
    let msg = format!("{} FAIL — {reason}", verb.name());
    eprintln!("{msg}");
    let _ = proof::append_proof(paths, &msg);
    1
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
