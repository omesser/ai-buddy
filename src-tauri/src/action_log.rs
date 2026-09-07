//! The Action Log: one JSON line per thing the attached Harness did.
//!
//! Append-only, in the data folder beside Memory. It points at the Harness's
//! own session (session id, tool call ids) rather than copying a transcript,
//! which is what CONTEXT.md asks of it. No reader yet; `tail -f` is the UI.
//!
//! ## Growth policy
//!
//! The log is rotated when it exceeds 2 MB. K=10 retention: 10 rotated files +
//! 1 current = 11 files total, ~22 MB ceiling.
//!
//! Files are named `action-log.jsonl`, `action-log.jsonl.1`, ..., `action-log.jsonl.10`.
//! When the current file exceeds 2 MB, it's rotated: current → `.1`, `.1` → `.2`,
//! ..., `.9` → `.10`. The oldest (`.10`) is dropped if it exists.
//!
//! Rotation is checked on every append and happens before the write. A rotation
//! or write failure is dropped: the log explains the buddy after the fact and
//! must never block a Director turn. Rate-limited error reporting (max once per
//! 60s) warns operators without log storms.
//!
//! **Single-writer assumption**: This module assumes no other process writes to
//! these log files. Concurrent writes from multiple processes are not supported.
//!
//! ## Implementation choice
//!
//! Hand-rolled rotation over `file-rotate` crate. file-rotate 0.8.x has file
//! corruption issues when FileRotate instances are cached/reused. Hand-rolled
//! logic is simple (rename cascade), testable, and avoids the corruption.
//!
//! K=10 chosen as light retention (weeks of regular use, days of heavy use)
//! without micro-hygiene that drops recent history too quickly. ~10k typical
//! events per file means months of context at 2 MB.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{json, Value};

pub const FILE: &str = "action-log.jsonl";

/// The size bound per log file, in bytes.
///
/// When the current log exceeds this limit, it's rotated to `.1` and a fresh
/// log is started. K=10 retention means 11 files total (current + 10 rotated),
/// so the disk ceiling is ~22 MB (11 × 2 MB).
const MAX_SIZE_BYTES: u64 = 2 * 1024 * 1024; // 2 MB

/// How many rotated files to keep (plus the current file = K+1 total).
const RETENTION_COUNT: usize = 10;

/// Minimum seconds between error reports, to avoid log storms.
const ERROR_REPORT_INTERVAL_SECS: u64 = 60;

/// Rate-limited error state: last error timestamp and consecutive failure count.
static ERROR_STATE: Mutex<Option<(u64, u64)>> = Mutex::new(None);

/// Write one event. `fields` is an object; `event` and `ts` are added to it.
///
/// A write that fails is dropped: the log explains the buddy after the fact
/// and must never be the reason a turn does not happen. Rotation happens
/// automatically when the file exceeds MAX_SIZE_BYTES.
///
/// ponytail: seconds since the epoch, as `memory.rs` does, so no date crate.
pub fn append(dir: &Path, event: &str, mut fields: Value) {
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_secs());
    if let Some(object) = fields.as_object_mut() {
        object.insert("event".into(), json!(event));
        object.insert("ts".into(), json!(ts));
    }

    let log_path = dir.join(FILE);

    // Rotate if needed, but never fail the append on rotation failure
    let _ = rotate_if_needed(&log_path);

    // Append the event
    let Ok(mut file) = OpenOptions::new().create(true).append(true).open(&log_path) else {
        report_error("action_log: failed to open log file");
        return;
    };

    if let Err(e) = writeln!(file, "{fields}") {
        report_error(&format!("action_log: failed to write event: {e}"));
    }
}

/// Rotate the log if it exceeds MAX_SIZE_BYTES.
///
/// Rotation: current → `.1`, `.1` → `.2`, ..., `.9` → `.10`, drop `.10`.
/// Returns Ok when rotation happened or was not needed, Err when rotation
/// was needed but failed. The caller drops the error: rotation must never
/// block an append.
fn rotate_if_needed(log_path: &Path) -> std::io::Result<()> {
    let metadata = match fs::metadata(log_path) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e),
    };

    if metadata.len() <= MAX_SIZE_BYTES {
        return Ok(());
    }

    // Cascade: .9 → .10, .8 → .9, ..., .1 → .2, current → .1
    // Start from the oldest to avoid overwriting
    for i in (1..=RETENTION_COUNT).rev() {
        let from = log_path.with_extension(format!("jsonl.{i}"));
        let to = log_path.with_extension(format!("jsonl.{}", i + 1));

        if i == RETENTION_COUNT {
            // Drop the oldest file
            let _ = fs::remove_file(&from);
        } else if from.exists() {
            fs::rename(&from, &to)?;
        }
    }

    // Rotate current → .1
    let backup = log_path.with_extension("jsonl.1");
    fs::rename(log_path, backup)?;

    Ok(())
}

/// Report an error with rate limiting: max once per ERROR_REPORT_INTERVAL_SECS.
///
/// Tracks consecutive failures and reports when the interval elapses. Prevents
/// log storms while ensuring operators notice issues.
fn report_error(msg: &str) {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());

    let mut state = ERROR_STATE.lock().unwrap();
    let should_report = match *state {
        None => {
            *state = Some((now, 1));
            true
        }
        Some((last_ts, count)) => {
            let elapsed = now.saturating_sub(last_ts);
            if elapsed >= ERROR_REPORT_INTERVAL_SECS {
                *state = Some((now, 1));
                true
            } else {
                *state = Some((last_ts, count + 1));
                false
            }
        }
    };

    if should_report {
        if let Some((_, count)) = *state {
            if count > 1 {
                eprintln!("{msg} ({count} consecutive failures)");
            } else {
                eprintln!("{msg}");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU32, Ordering};

    /// A directory of our own under the system temp dir, removed when the test ends.
    struct TempDir(PathBuf);

    impl TempDir {
        fn new(label: &str) -> Self {
            static NEXT: AtomicU32 = AtomicU32::new(0);
            let unique = NEXT.fetch_add(1, Ordering::Relaxed);
            let dir = std::env::temp_dir().join(format!(
                "action-log-{label}-{}-{unique}",
                std::process::id()
            ));
            fs::create_dir_all(&dir).expect("temp dir is creatable");
            Self(dir)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn a_small_log_is_not_rotated() {
        let dir = TempDir::new("no-rotation");

        for i in 0..10 {
            append(dir.path(), "test", json!({"index": i}));
        }

        let log = dir.path().join(FILE);
        assert!(log.exists(), "the log file exists");
        assert!(
            fs::metadata(&log).unwrap().len() < MAX_SIZE_BYTES,
            "the log is well under the rotation threshold"
        );

        let backup_1 = dir.path().join("action-log.jsonl.1");
        assert!(!backup_1.exists(), "no backup was created");
    }

    #[test]
    fn a_large_log_is_rotated_when_it_exceeds_max_size() {
        let dir = TempDir::new("rotation");
        let log = dir.path().join(FILE);

        let large_line = "x".repeat(1024);
        for i in 0..2500 {
            append(
                dir.path(),
                "large",
                json!({"index": i, "data": &large_line}),
            );
        }

        let backup_1 = dir.path().join("action-log.jsonl.1");
        assert!(backup_1.exists(), "a .1 backup was created after rotation");

        let current_size = fs::metadata(&log).unwrap().len();
        assert!(
            current_size < MAX_SIZE_BYTES + 2048,
            "the current log is near or under MAX_SIZE_BYTES: {current_size} bytes"
        );

        let backup_content = fs::read_to_string(&backup_1).unwrap();
        assert!(
            backup_content.contains("\"index\":0"),
            "the .1 backup contains early events"
        );
    }

    #[test]
    fn rotation_keeps_k_files_and_drops_oldest() {
        let dir = TempDir::new("k-retention");

        let large_line = "x".repeat(1024);
        for i in 0..15_000 {
            append(
                dir.path(),
                "storm",
                json!({"index": i, "data": &large_line}),
            );
        }

        let mut existing = Vec::new();
        existing.push(dir.path().join(FILE));
        for n in 1..=RETENTION_COUNT {
            let path = dir.path().join(format!("{FILE}.{n}"));
            if path.exists() {
                existing.push(path);
            }
        }

        assert!(!existing.is_empty(), "at least the current log exists");
        assert!(
            existing.len() <= RETENTION_COUNT + 1,
            "no more than K+1 files: found {} files",
            existing.len()
        );

        let beyond_k = dir.path().join(format!("{FILE}.{}", RETENTION_COUNT + 1));
        assert!(!beyond_k.exists(), "no file beyond K retention");
    }

    #[test]
    fn append_never_fails_even_when_rotation_would() {
        let dir = TempDir::new("rotation-failure");
        let log = dir.path().join(FILE);

        let large_data = "x".repeat((MAX_SIZE_BYTES as usize) + 1);
        fs::write(&log, large_data).unwrap();

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o555)).unwrap();
        }

        // Append should succeed or silently fail without panicking
        append(dir.path(), "test", json!({"after": "rotation-failure"}));

        // The contract is "never block a turn" = drop errors and return
        // This test completes without panicking, which validates the contract
    }

    #[test]
    fn the_log_stays_valid_jsonl_after_rotation() {
        let dir = TempDir::new("valid-jsonl");

        let large_line = "x".repeat(1024);
        for i in 0..2500 {
            append(
                dir.path(),
                "large",
                json!({"index": i, "data": &large_line}),
            );
        }

        let mut log_paths = vec![dir.path().join(FILE)];
        for n in 1..=RETENTION_COUNT {
            log_paths.push(dir.path().join(format!("{FILE}.{n}")));
        }

        for path in log_paths.iter().filter(|p| p.exists()) {
            let content = fs::read_to_string(path).unwrap();
            for (i, line) in content.lines().enumerate() {
                if !line.is_empty() {
                    serde_json::from_str::<Value>(line).unwrap_or_else(|e| {
                        panic!(
                            "line {i} in {} is not valid JSON: {e}\n{}",
                            path.display(),
                            if line.len() > 200 { &line[..200] } else { line }
                        );
                    });
                }
            }
        }
    }

    #[test]
    fn sustained_append_storm_stays_within_disk_bound() {
        let dir = TempDir::new("append-storm");

        let line = "x".repeat(200);
        for i in 0..30_000 {
            append(dir.path(), "storm", json!({"index": i, "data": &line}));
        }

        let mut total_size = 0u64;
        let mut log_paths = vec![dir.path().join(FILE)];
        for n in 1..=RETENTION_COUNT {
            log_paths.push(dir.path().join(format!("{FILE}.{n}")));
        }

        for path in log_paths.iter().filter(|p| p.exists()) {
            total_size += fs::metadata(path).map(|m| m.len()).unwrap_or(0);
        }

        let bound = ((RETENTION_COUNT + 1) as u64) * MAX_SIZE_BYTES + 4096;
        assert!(
            total_size <= bound,
            "total disk footprint {total_size} bytes is within ~{}×MAX_SIZE_BYTES bound ({bound} bytes)",
            RETENTION_COUNT + 1
        );
    }
}
