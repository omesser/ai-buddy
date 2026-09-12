//! Unit suites mirroring helpers/prove-units.sh.

use std::fs::{self, File};
use std::io::{self, Write};
use std::path::Path;
use std::process::{Command, Stdio};

use crate::paths::RunPaths;
use crate::proof;

/// Run unit proofs. Returns process exit code.
pub fn run(repo_root: &Path, paths: &RunPaths) -> i32 {
    let dest = paths.evidence.join("units");
    if let Err(e) = fs::create_dir_all(&dest) {
        eprintln!("units: cannot create {}: {e}", dest.display());
        return 1;
    }

    let mut status = 0i32;
    let summary = dest.join("summary.txt");
    let _ = fs::remove_file(&summary);

    println!("prove-units: cargo test -p ai-buddy-core");
    if tee_command(
        Command::new("cargo")
            .args(["test", "-p", "ai-buddy-core"])
            .current_dir(repo_root),
        &dest.join("cargo-core.txt"),
    ) {
        append_summary(&summary, "PASS cargo-core");
    } else {
        append_summary(&summary, "FAIL cargo-core");
        status = 1;
    }

    println!("prove-units: node --test");
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
    if tee_command(&mut node, &dest.join("node-tests.txt")) {
        append_summary(&summary, "PASS node-tests");
    } else {
        append_summary(&summary, "FAIL node-tests");
        status = 1;
    }

    println!("prove-units: test_verify_overlay_diagnostics.sh");
    if tee_command(
        Command::new("bash")
            .arg("scripts/test_verify_overlay_diagnostics.sh")
            .current_dir(repo_root),
        &dest.join("overlay-diagnostics.txt"),
    ) {
        append_summary(&summary, "PASS overlay-diagnostics");
    } else {
        append_summary(&summary, "FAIL overlay-diagnostics");
        status = 1;
    }

    write_gui_gap(&dest);

    if status == 0 {
        let _ = proof::append_proof(
            paths,
            &format!(
                "prove-units PASS — see {} (GUI gap noted in GUI-GAP.md if any)",
                dest.display()
            ),
        );
    } else {
        let _ = proof::append_proof(paths, &format!("prove-units FAIL — see {}", dest.display()));
    }
    status
}

fn tee_command(cmd: &mut Command, log_path: &Path) -> bool {
    let output = cmd.stdout(Stdio::piped()).stderr(Stdio::piped()).output();
    match output {
        Ok(out) => {
            let mut combined = Vec::new();
            combined.extend_from_slice(&out.stdout);
            if !out.stderr.is_empty() {
                if !combined.is_empty() && !combined.ends_with(b"\n") {
                    combined.push(b'\n');
                }
                combined.extend_from_slice(&out.stderr);
            }
            let _ = fs::write(log_path, &combined);
            let _ = io::stdout().write_all(&out.stdout);
            let _ = io::stderr().write_all(&out.stderr);
            out.status.success()
        }
        Err(e) => {
            let msg = format!("failed to spawn: {e}\n");
            let _ = fs::write(log_path, &msg);
            eprint!("{msg}");
            false
        }
    }
}

fn append_summary(summary: &Path, line: &str) {
    println!("{line}");
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
             Overlay presence / poke / summon GUI paths remain covered by that \
             script on a proper X11 desktop;\n\
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
