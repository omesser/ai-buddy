//! The part of Cursor's project config that is ours: one `ai-buddy` server in
//! `<cwd>/.cursor/mcp.json`, and one line in the repo's `info/exclude` so the
//! file never shows in `git status`.
//!
//! `cursor-agent acp` ignores the `mcpServers` handed over on `session/new`
//! and loads servers only from an approved `mcp.json` (#1020). Cursor's
//! approval id hashes the entry's `command`, `args` and `env`, so the entry
//! carries no `env`: a per-run token there would mean a re-approval on every
//! launch, and a secret at rest. The shim is told where to dial in `args`, a
//! socket path whose 0600 mode is the authorization.
//!
//! Everything here is idempotent. `install` runs on every attach and
//! `remove` on every quit, so a second install leaves the file's bytes alone
//! and `remove` undoes only what `install` recorded creating. `serde_json`
//! carries `preserve_order` in this workspace, so a rewrite keeps the user's
//! key order as well as their values.

use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use serde_json::{json, Value};

pub const SERVER: &str = "ai-buddy";
pub const FILE: &str = "mcp.json";
const DIR: &str = ".cursor";

/// `cursor-agent mcp enable` takes a third of a second measured; ten is the
/// point past which it is hung, and the attach goes on without it.
const ENABLE_TIMEOUT: Duration = Duration::from_secs(10);

/// The entry Cursor loads. No `env`, on purpose.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub command: PathBuf,
    pub args: Vec<String>,
}

impl Entry {
    fn value(&self) -> Value {
        json!({
            "type": "stdio",
            "command": self.command.to_string_lossy(),
            "args": self.args,
        })
    }
}

/// What `install` changed, so `remove` undoes exactly that and nothing of the
/// user's.
#[derive(Debug)]
pub struct Installed {
    file: PathBuf,
    created_dir: bool,
    created_file: bool,
    /// `<gitdir>/info/exclude` and the pattern line, when the cwd is inside a
    /// git worktree.
    exclude: Option<(PathBuf, String)>,
    added_exclude: bool,
}

/// Merge `entry` into `<cwd>/.cursor/mcp.json` and hide the file from git.
/// A file that does not parse is an error and is not written over.
pub fn install(cwd: &Path, entry: &Entry) -> Result<Installed, String> {
    let dir = cwd.join(DIR);
    let file = dir.join(FILE);
    let created_dir = !dir.exists();
    let (mut root, created_file) = match fs::read_to_string(&file) {
        Ok(text) => (parse(&file, &text)?, false),
        Err(error) if error.kind() == ErrorKind::NotFound => (json!({}), true),
        Err(error) => return Err(format!("{}: {error}", file.display())),
    };
    let before = root.clone();
    let servers = root
        .as_object_mut()
        .expect("parse checked the root is an object")
        .entry("mcpServers")
        .or_insert_with(|| json!({}));
    let servers = servers
        .as_object_mut()
        .ok_or_else(|| format!("{}: mcpServers is not an object", file.display()))?;
    servers.insert(SERVER.to_string(), entry.value());
    if root != before {
        fs::create_dir_all(&dir).map_err(|error| format!("{}: {error}", dir.display()))?;
        write(&file, &root)?;
    }
    let exclude = exclude_for(cwd);
    let added_exclude = match &exclude {
        Some((path, line)) => add_line(path, line)?,
        None => false,
    };
    Ok(Installed {
        file,
        created_dir,
        created_file,
        exclude,
        added_exclude,
    })
}

fn parse(file: &Path, text: &str) -> Result<Value, String> {
    let root: Value = serde_json::from_str(text)
        .map_err(|_| format!("{} is not JSON, so it was left alone", file.display()))?;
    if !root.is_object() {
        return Err(format!(
            "{} is not an object, so it was left alone",
            file.display()
        ));
    }
    Ok(root)
}

fn write(path: &Path, value: &Value) -> Result<(), String> {
    let mut bytes = serde_json::to_vec_pretty(value).map_err(|error| error.to_string())?;
    bytes.push(b'\n');
    fs::write(path, bytes).map_err(|error| format!("{}: {error}", path.display()))
}

/// The exclude file of the repo enclosing `cwd`, and the pattern for our
/// file. A linked worktree's `.git` is a file naming its gitdir, and
/// `info/exclude` lives in the common dir that gitdir points back at. No
/// `git` process: this runs on every attach.
fn exclude_for(cwd: &Path) -> Option<(PathBuf, String)> {
    let root = cwd.ancestors().find(|dir| dir.join(".git").exists())?;
    let dot_git = root.join(".git");
    let gitdir = if dot_git.is_dir() {
        dot_git
    } else {
        let text = fs::read_to_string(&dot_git).ok()?;
        let gitdir = root.join(text.strip_prefix("gitdir:")?.trim());
        match fs::read_to_string(gitdir.join("commondir")) {
            Ok(common) => gitdir.join(common.trim()),
            Err(_) => gitdir,
        }
    };
    let relative: Vec<String> = cwd
        .strip_prefix(root)
        .ok()?
        .components()
        .map(|part| part.as_os_str().to_string_lossy().into_owned())
        .chain([DIR.to_string(), FILE.to_string()])
        .collect();
    Some((
        gitdir.join("info").join("exclude"),
        format!("/{}", relative.join("/")),
    ))
}

/// Append `line` unless present. `true` when this call added it.
fn add_line(path: &Path, line: &str) -> Result<bool, String> {
    let named = |error: std::io::Error| format!("{}: {error}", path.display());
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == ErrorKind::NotFound => String::new(),
        Err(error) => return Err(named(error)),
    };
    if text.lines().any(|have| have == line) {
        return Ok(false);
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(named)?;
    }
    let newline = if text.is_empty() || text.ends_with('\n') {
        ""
    } else {
        "\n"
    };
    fs::write(path, format!("{text}{newline}{line}\n")).map_err(named)?;
    Ok(true)
}

fn remove_line(path: &Path, line: &str) -> Result<(), String> {
    let named = |error: std::io::Error| format!("{}: {error}", path.display());
    let text = fs::read_to_string(path).map_err(named)?;
    let kept: String = text
        .lines()
        .filter(|have| *have != line)
        .map(|have| format!("{have}\n"))
        .collect();
    fs::write(path, kept).map_err(named)
}

/// `<cli> mcp enable ai-buddy` in `cwd`. Approvals are read once per
/// `cursor-agent` process, so this runs before the `acp` spawn, every time.
pub fn enable(cli: &Path, cwd: &Path) -> Result<(), String> {
    let line = format!("`{} mcp enable {SERVER}`", cli.display());
    let mut child = Command::new(cli)
        .args(["mcp", "enable", SERVER])
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| format!("{line}: {error}"))?;
    let deadline = Instant::now() + ENABLE_TIMEOUT;
    loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => return Ok(()),
            Ok(Some(status)) => return Err(format!("{line} exited with {status}")),
            Ok(None) => {}
            Err(error) => return Err(format!("{line}: {error}")),
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!(
                "{line} did not finish in {}s",
                ENABLE_TIMEOUT.as_secs()
            ));
        }
        thread::sleep(Duration::from_millis(50));
    }
}

impl Installed {
    /// Undo `install`. Nothing here fails loudly: a quit is not the moment
    /// to refuse, so each step logs and the next still runs.
    pub fn remove(self) {
        if let Err(why) = self.remove_entry() {
            eprintln!("harness: cursor mcp: {why}");
        }
        if self.created_dir {
            if let Some(dir) = self.file.parent() {
                // Refuses a directory that is not empty, which is the point.
                let _ = fs::remove_dir(dir);
            }
        }
        if let (true, Some((path, line))) = (self.added_exclude, &self.exclude) {
            if let Err(why) = remove_line(path, line) {
                eprintln!("harness: cursor mcp: {why}");
            }
        }
    }

    fn remove_entry(&self) -> Result<(), String> {
        let text = match fs::read_to_string(&self.file) {
            Ok(text) => text,
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(format!("{}: {error}", self.file.display())),
        };
        let mut root = parse(&self.file, &text)?;
        let before = root.clone();
        let object = root.as_object_mut().expect("parse checked");
        if let Some(servers) = object.get_mut("mcpServers").and_then(Value::as_object_mut) {
            servers.remove(SERVER);
            if servers.is_empty() {
                object.remove("mcpServers");
            }
        }
        if object.is_empty() && self.created_file {
            return fs::remove_file(&self.file)
                .map_err(|error| format!("{}: {error}", self.file.display()));
        }
        if root != before {
            write(&self.file, &root)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir(name: &str) -> PathBuf {
        let path =
            std::env::temp_dir().join(format!("ai-buddy-cursor-mcp-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).expect("temp dir");
        path
    }

    fn entry() -> Entry {
        Entry {
            command: PathBuf::from("/opt/ai-buddy"),
            args: vec!["--mcp-stdio".into(), "--sock".into(), "/tmp/x.sock".into()],
        }
    }

    fn read(path: &Path) -> Value {
        serde_json::from_str(&fs::read_to_string(path).expect("the file")).expect("JSON")
    }

    fn git(cwd: &Path, args: &[&str]) -> String {
        let out = Command::new("git")
            .args(args)
            .current_dir(cwd)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .output()
            .expect("git runs");
        assert!(
            out.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).to_string()
    }

    fn git_init(cwd: &Path) {
        git(cwd, &["init", "-q", "-b", "main"]);
        git(cwd, &["config", "user.email", "t@example.com"]);
        git(cwd, &["config", "user.name", "t"]);
        git(cwd, &["commit", "-q", "--allow-empty", "-m", "root"]);
    }

    #[test]
    fn install_keeps_a_foreign_server_and_writes_no_env() {
        let cwd = dir("foreign");
        let file = cwd.join(DIR).join(FILE);
        fs::create_dir_all(cwd.join(DIR)).unwrap();
        let other = json!({"command": "echo", "args": ["hi"], "env": {"K": "v"}});
        fs::write(
            &file,
            serde_json::to_string(&json!({"keep": 1, "mcpServers": {"other": other}})).unwrap(),
        )
        .unwrap();

        install(&cwd, &entry()).expect("installed");

        let root = read(&file);
        assert_eq!(root["keep"], json!(1));
        assert_eq!(root["mcpServers"]["other"], other);
        assert_eq!(
            root["mcpServers"]["ai-buddy"],
            json!({"type": "stdio", "command": "/opt/ai-buddy", "args": ["--mcp-stdio", "--sock", "/tmp/x.sock"]})
        );
        assert!(root["mcpServers"]["ai-buddy"].get("env").is_none());
    }

    #[test]
    fn a_second_install_leaves_the_bytes_and_the_mtime_alone() {
        let cwd = dir("twice");
        let file = cwd.join(DIR).join(FILE);
        install(&cwd, &entry()).expect("installed");
        let bytes = fs::read(&file).unwrap();
        let mtime = fs::metadata(&file).unwrap().modified().unwrap();
        thread::sleep(Duration::from_millis(20));

        install(&cwd, &entry()).expect("installed again");

        assert_eq!(fs::read(&file).unwrap(), bytes);
        assert_eq!(fs::metadata(&file).unwrap().modified().unwrap(), mtime);
        assert!(
            bytes.ends_with(b"}\n"),
            "pretty-printed with a trailing newline"
        );
    }

    #[test]
    fn install_then_remove_on_a_fresh_dir_leaves_no_cursor_dir() {
        let cwd = dir("fresh");
        let installed = install(&cwd, &entry()).expect("installed");
        assert!(cwd.join(DIR).join(FILE).is_file());

        installed.remove();

        assert!(!cwd.join(DIR).exists(), ".cursor was left behind");
    }

    #[test]
    fn remove_keeps_a_file_we_did_not_create_and_its_foreign_server() {
        let cwd = dir("theirs");
        let file = cwd.join(DIR).join(FILE);
        fs::create_dir_all(cwd.join(DIR)).unwrap();
        fs::write(&file, r#"{"mcpServers":{"other":{"command":"echo"}}}"#).unwrap();

        install(&cwd, &entry()).expect("installed").remove();

        let root = read(&file);
        assert_eq!(root, json!({"mcpServers": {"other": {"command": "echo"}}}));
    }

    #[test]
    fn an_unparsable_file_is_an_error_and_untouched() {
        let cwd = dir("broken");
        let file = cwd.join(DIR).join(FILE);
        fs::create_dir_all(cwd.join(DIR)).unwrap();
        fs::write(&file, "{not json").unwrap();

        let err = install(&cwd, &entry()).expect_err("wrote over garbage");

        assert!(err.contains(FILE), "{err}");
        assert_eq!(fs::read_to_string(&file).unwrap(), "{not json");
    }

    #[test]
    fn the_exclude_line_names_the_cwd_relative_to_the_repo_root() {
        let root = dir("repo");
        git_init(&root);
        let sub = root.join("app").join("web");
        fs::create_dir_all(&sub).unwrap();

        let at_root = install(&root, &entry()).expect("installed at the root");
        let in_sub = install(&sub, &entry()).expect("installed in a subdirectory");

        let exclude = fs::read_to_string(root.join(".git/info/exclude")).unwrap();
        assert!(
            exclude.contains("\n/.cursor/mcp.json\n") || exclude.starts_with("/.cursor/mcp.json\n"),
            "{exclude}"
        );
        assert!(exclude.contains("/app/web/.cursor/mcp.json\n"), "{exclude}");
        assert_eq!(git(&root, &["status", "--porcelain"]), "");

        at_root.remove();
        in_sub.remove();

        let exclude = fs::read_to_string(root.join(".git/info/exclude")).unwrap();
        assert!(!exclude.contains(".cursor/mcp.json"), "{exclude}");
        assert_eq!(git(&root, &["status", "--porcelain"]), "");
    }

    #[test]
    fn a_linked_worktree_excludes_in_the_common_dir() {
        let root = dir("wt-main");
        git_init(&root);
        let linked = dir("wt-linked");
        fs::remove_dir_all(&linked).unwrap();
        git(&root, &["worktree", "add", "-q", linked.to_str().unwrap()]);

        install(&linked, &entry()).expect("installed in the worktree");

        let exclude = fs::read_to_string(root.join(".git/info/exclude")).unwrap();
        assert!(exclude.contains("/.cursor/mcp.json\n"), "{exclude}");
        assert_eq!(git(&linked, &["status", "--porcelain"]), "");
    }

    #[test]
    fn outside_a_repo_there_is_no_exclude() {
        let cwd = dir("norepo");
        let installed = install(&cwd, &entry()).expect("installed");
        assert!(installed.exclude.is_none(), "{installed:?}");
        assert!(!installed.added_exclude);
    }

    #[cfg(unix)]
    fn fake_cli(dir: &Path, exit: u8) -> (PathBuf, PathBuf) {
        use std::os::unix::fs::PermissionsExt;

        let log = dir.join("calls.log");
        let cli = dir.join("cursor-agent");
        fs::write(
            &cli,
            format!(
                "#!/bin/sh\necho \"$PWD|$@\" >> '{}'\nexit {exit}\n",
                log.display()
            ),
        )
        .unwrap();
        fs::set_permissions(&cli, fs::Permissions::from_mode(0o755)).unwrap();
        (cli, log)
    }

    #[cfg(unix)]
    #[test]
    fn enable_runs_the_cli_once_in_the_cwd() {
        let bin = dir("enable-bin");
        let cwd = dir("enable-cwd");
        let (cli, log) = fake_cli(&bin, 0);

        enable(&cli, &cwd).expect("enabled");

        let seen = fs::read_to_string(&log).unwrap();
        assert_eq!(
            seen,
            format!(
                "{}|mcp enable ai-buddy\n",
                cwd.canonicalize().unwrap().display()
            )
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_failing_enable_is_an_error() {
        let bin = dir("enable-fail");
        let (cli, _log) = fake_cli(&bin, 1);
        let err = enable(&cli, &bin).expect_err("exit 1 passed");
        assert!(err.contains("mcp enable"), "{err}");
    }
}
