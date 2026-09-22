//! Unit suites mirroring helpers/prove-units.sh.

use std::fs::{self, File};
use std::io::Write;
use std::path::Path;
use std::process::Command;

use crate::contract::{Outcome, RunReport};

/// Run unit proofs.
pub fn run(repo_root: &Path, report: &mut RunReport) {
    let dest = report.paths().evidence.join("units");
    if let Err(e) = fs::create_dir_all(&dest) {
        report.check(
            Outcome::Error,
            "units evidence",
            &format!("cannot create {}: {e}", dest.display()),
        );
        return;
    }

    let summary = dest.join("summary.txt");
    let _ = fs::remove_file(&summary);

    report.say("prove-units: cargo test -p ai-buddy-core");
    let mut cargo = Command::new("cargo");
    cargo
        .args(["test", "-p", "ai-buddy-core"])
        .current_dir(repo_root);
    suite(report, "cargo-core", &mut cargo, &dest, &summary);

    report.say("prove-units: node --test");
    let mut node = Command::new("node");
    node.arg("--test");
    // Expand tests/*.test.js like the shell does.
    let pattern = repo_root.join("tests");
    let mut found = false;
    if let Ok(rd) = fs::read_dir(&pattern) {
        let mut files: Vec<_> = rd
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| {
                p.file_name()
                    .and_then(|n| n.to_str())
                    .map(|n| n.ends_with(".test.js"))
                    .unwrap_or(false)
            })
            .collect();
        files.sort();
        for f in files {
            found = true;
            node.arg(f);
        }
    }
    if !found {
        // Still invoke with the glob string so failure is visible if empty.
        node.arg("tests/*.test.js");
    }
    node.current_dir(repo_root);
    suite(report, "node-tests", &mut node, &dest, &summary);

    report.say("prove-units: test_verify_overlay_diagnostics.sh");
    let mut diagnostics = Command::new("bash");
    diagnostics
        .arg("scripts/test_verify_overlay_diagnostics.sh")
        .current_dir(repo_root);
    suite(
        report,
        "overlay-diagnostics",
        &mut diagnostics,
        &dest,
        &summary,
    );

    write_gui_gap(&dest);
}

/// Run one suite, log it, and record it in both `summary.txt` and the report.
fn suite(report: &mut RunReport, name: &str, cmd: &mut Command, dest: &Path, summary: &Path) {
    let code = report.exec(cmd, Some(&dest.join(format!("{name}.txt"))));
    let (outcome, detail) = match code {
        Some(0) => (Outcome::Pass, String::new()),
        Some(c) => (Outcome::Fail, format!("exit {c}")),
        None => (Outcome::Fail, "failed to spawn".to_string()),
    };
    append_summary(summary, &format!("{} {name}", outcome.label()));
    report.check(outcome, name, &detail);
}

fn append_summary(summary: &Path, line: &str) {
    if let Ok(mut f) = File::options().create(true).append(true).open(summary) {
        let _ = writeln!(f, "{line}");
    }
}

fn write_gui_gap(dest: &Path) {
    let os = std::env::consts::OS;
    let arch = std::env::consts::ARCH;
    let display = std::env::var("DISPLAY").unwrap_or_else(|_| "<unset>".into());
    let xterm = which_or_missing("xterm");
    let openbox = which_or_missing("openbox");
    let xdotool = which_or_missing("xdotool");
    let mut body = format!(
        "# GUI / overlay gap (this host)\n\n\
         - OS: {os} {arch}\n\
         - DISPLAY={display}\n\
         - xterm: {xterm}\n\
         - openbox: {openbox}\n\
         - xdotool: {xdotool}\n"
    );
    let display_unset = display == "<unset>" || display.is_empty();
    let missing_xterm = xterm == "MISSING";
    let missing_openbox = openbox == "MISSING";
    if display_unset || missing_xterm || missing_openbox {
        body.push_str(
            "\n`scripts/verify-overlay-x11.sh` was not runnable here without \
             `DISPLAY` + `xterm` + supporting WM/`openbox`.\n\
             Overlay presence and Poke remain covered by that script on a proper \
             X11 desktop; `ai-buddy-verify poke`/`summon` prove both gestures \
             directly on macOS.\n\
             this evidence pack proves the unit + diagnostic subset only.\n",
        );
    }
    let _ = fs::write(dest.join("GUI-GAP.md"), body);
}

fn which_or_missing(name: &str) -> String {
    Command::new("sh")
        .args(["-c", &format!("command -v {name} || echo MISSING")])
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "MISSING".into())
}
