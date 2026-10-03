//! One end-to-end scenario from `scripts/scenarios/`, with the binaries its
//! `Usage` line names built and passed in. Without `--go` the script prints
//! its takeover header and exits 2, which is this contract's skip.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::contract::{Outcome, RunReport};

const TEST_BINARY: &str = "<fidget test binary>";

/// Run the scenario `name`, taking over the GUI only when `go` is set.
pub fn run(repo_root: &Path, name: &str, go: bool, extra: &[String], report: &mut RunReport) {
    let dir = repo_root.join("scripts/scenarios");
    let known = scenarios(&dir);
    let Some((_, text)) = known.iter().find(|(n, _)| n == name) else {
        let names: Vec<&str> = known.iter().map(|(n, _)| n.as_str()).collect();
        report.check(
            Outcome::Error,
            "scenario",
            &format!("no scenario {name}; known: {}", names.join(", ")),
        );
        return;
    };

    if !go {
        let header = parse_scenario_header(text);
        report.say(&header);
        report.check(
            Outcome::Skip,
            name,
            "printed the takeover header; post it, and rerun with --go once the owner says go",
        );
        return;
    }

    if std::env::consts::OS != "macos" {
        report.check(
            Outcome::Skip,
            "scenario",
            &format!("scenarios are macOS only, not {}", std::env::consts::OS),
        );
        return;
    }

    let Some(binaries) = binaries(repo_root, text.contains(TEST_BINARY), report) else {
        return;
    };

    let mut script = Command::new("bash");
    script
        .arg(dir.join(format!("{name}.sh")))
        .arg("--go")
        .args(binaries)
        .args(extra)
        .current_dir(repo_root);

    let (outcome, detail) = match report.exec(&mut script, None) {
        Some(0) => (Outcome::Pass, "passed".to_string()),
        Some(1) => (
            Outcome::Fail,
            "failed; the script names its evidence".to_string(),
        ),
        Some(2) => (Outcome::Skip, "the script skipped".to_string()),
        Some(c) => (Outcome::Error, format!("exited {c}")),
        None => (Outcome::Error, "could not run bash".to_string()),
    };
    report.check(outcome, name, &detail);
}

/// Parse the scenario header from script text. Extracts consecutive comment
/// lines after the shebang, stripping leading `#` and optional space.
fn parse_scenario_header(text: &str) -> String {
    let lines = text.lines().skip(1);
    let mut header = Vec::new();
    for line in lines {
        if line.starts_with('#') {
            let content = line
                .strip_prefix("# ")
                .unwrap_or_else(|| line.strip_prefix('#').unwrap_or(line));
            header.push(content);
        } else if !line.trim().is_empty() {
            break;
        }
    }
    header.join("\n")
}

/// Every `<name>.sh` in `dir` that carries a `# Scenario:` header, with its
/// text, sorted by name. A helper such as `fixture-harness.sh` has none.
fn scenarios(dir: &Path) -> Vec<(String, String)> {
    let mut found: Vec<(String, String)> = fs::read_dir(dir)
        .into_iter()
        .flatten()
        .filter_map(|entry| {
            let path = entry.ok()?.path();
            let name = path.file_name()?.to_str()?.strip_suffix(".sh")?.to_string();
            let text = fs::read_to_string(&path).ok()?;
            text.lines()
                .any(|line| line.starts_with("# Scenario:"))
                .then_some((name, text))
        })
        .collect();
    found.sort();
    found
}

/// The app binary, then the test binary when the scenario takes one.
fn binaries(repo_root: &Path, wants_test: bool, report: &mut RunReport) -> Option<Vec<PathBuf>> {
    report.say("scenario: building fidget");
    let mut build = Command::new("cargo");
    build.args(["build", "-p", "fidget"]).current_dir(repo_root);
    if report.exec(&mut build, None) != Some(0) {
        report.check(
            Outcome::Error,
            "cargo build",
            "cargo build -p fidget failed",
        );
        return None;
    }
    let mut found = vec![repo_root.join("target/debug/fidget")];
    if wants_test {
        report.say("scenario: building the fixture Harness test binary");
        let out = Command::new("cargo")
            .args(["test", "-p", "fidget", "--no-run", "--message-format=json"])
            .current_dir(repo_root)
            .output();
        match out
            .ok()
            .and_then(|out| test_binary(&String::from_utf8_lossy(&out.stdout)))
        {
            Some(path) => found.push(path),
            None => {
                report.check(
                    Outcome::Error,
                    "test binary",
                    "cargo test -p fidget --no-run named no fidget test binary",
                );
                return None;
            }
        }
    }
    Some(found)
}

/// The `fidget` bin's test executable from `cargo --message-format=json`
/// output. It holds `harness::tests::fake_acp_agent`, the fixture Harness.
fn test_binary(messages: &str) -> Option<PathBuf> {
    messages.lines().find_map(|line| {
        let message: serde_json::Value = serde_json::from_str(line).ok()?;
        let target = &message["target"];
        let is_bin = target["kind"].as_array()?.iter().any(|k| k == "bin");
        if message["profile"]["test"] != true || !is_bin || target["name"] != "fidget" {
            return None;
        }
        Some(PathBuf::from(message["executable"].as_str()?))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_test_binary_is_the_fidget_bins_test_executable() {
        let messages = [
            r#"{"reason":"compiler-artifact","target":{"kind":["lib"],"name":"fidget_core"},"profile":{"test":true},"executable":"/t/deps/fidget_core-1"}"#,
            r#"{"reason":"compiler-artifact","target":{"kind":["bin"],"name":"fidget"},"profile":{"test":false},"executable":"/t/fidget"}"#,
            r#"{"reason":"compiler-artifact","target":{"kind":["bin"],"name":"fidget"},"profile":{"test":true},"executable":"/t/deps/fidget-5ec"}"#,
            r#"{"reason":"build-finished","success":true}"#,
        ]
        .join("\n");

        assert_eq!(
            test_binary(&messages),
            Some(PathBuf::from("/t/deps/fidget-5ec"))
        );
    }

    #[test]
    fn no_fidget_test_executable_is_none() {
        let messages = r#"{"reason":"compiler-artifact","target":{"kind":["bin"],"name":"fidget"},"profile":{"test":true},"executable":null}"#;

        assert_eq!(test_binary(messages), None);
    }
}
