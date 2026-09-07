//! The Action Log: one JSON line per thing the attached Harness did.
//!
//! Append-only, in the data folder beside Memory. It points at the Harness's
//! own session (session id, tool call ids) rather than copying a transcript,
//! which is what CONTEXT.md asks of it. No reader yet; `tail -f` is the UI.
//!
//! ## Growth policy
//!
//! The log is rotated when it exceeds 2 MB: the current file becomes `.1`
//! (replacing any older `.1`), and a fresh log is started. This bounds growth
//! to at most 4 MB (current + one backup), which is ~20k typical events.
//!
//! Rotation is checked on every append and happens before the write. A rotation
//! failure is dropped the same way a write failure is: the log explains the
//! buddy after the fact and must never block a turn.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{json, Value};

pub const FILE: &str = "action-log.jsonl";

/// The size bound for the Action Log, in bytes.
///
/// When the log exceeds this limit, it is rotated: the current file becomes
/// `.1` and a fresh log is started. This keeps sustained Harness use from
/// quietly filling the data folder.
///
/// 2 MB holds ~10k typical events (200 bytes each: prompt/completion metadata,
/// tool calls, usage) — months of regular use, or days of heavy use. One
/// backup is kept, so the disk footprint is at most 4 MB.
const MAX_SIZE: u64 = 2 * 1024 * 1024; // 2 MB

/// Write one event. `fields` is an object; `event` and `ts` are added to it.
///
/// A write that fails is dropped: the log explains the buddy after the fact
/// and must never be the reason a turn does not happen.
///
/// Rotation happens before the append when the log exceeds MAX_SIZE. Rotation
/// failures are dropped the same way write failures are: the log must never
/// block a turn. The JSONL format and append-only structure are preserved.
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

    // Rotate if needed, but never fail the append on rotation failure.
    let _ = rotate_if_needed(&log_path);

    let Ok(mut file) = OpenOptions::new().create(true).append(true).open(&log_path) else {
        return;
    };
    let _ = writeln!(file, "{fields}");
}

/// Rotate the log if it exceeds MAX_SIZE.
///
/// The current log becomes `.1` (replacing any older `.1`), and a fresh log
/// is started. Returns Ok when rotation happened or was not needed, Err when
/// rotation was needed but failed. The caller drops the error: rotation must
/// never block an append.
fn rotate_if_needed(log_path: &Path) -> std::io::Result<()> {
    let metadata = match std::fs::metadata(log_path) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e),
    };

    if metadata.len() <= MAX_SIZE {
        return Ok(());
    }

    let backup_path = log_path.with_extension("jsonl.1");
    std::fs::rename(log_path, backup_path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::atomic::{AtomicU32, Ordering};

    /// A directory of our own under the system temp dir, removed when the test ends.
    struct TempDir(std::path::PathBuf);

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

        // Append a few events, well under MAX_SIZE
        for i in 0..10 {
            append(dir.path(), "test", json!({"index": i}));
        }

        let _log = dir.path().join(FILE);
        assert!(_log.exists(), "the log file exists");
        assert!(
            fs::metadata(&_log).unwrap().len() < MAX_SIZE,
            "the log is well under the rotation threshold"
        );

        let backup = dir.path().join("action-log.jsonl.1");
        assert!(!backup.exists(), "no backup was created");
    }

    #[test]
    fn a_large_log_is_rotated_when_it_exceeds_max_size() {
        let dir = TempDir::new("rotation");
        let log = dir.path().join(FILE);

        // Create a log larger than MAX_SIZE
        let large_line = "x".repeat(1024); // 1 KB line
        for i in 0..2500 {
            append(
                dir.path(),
                "large",
                json!({"index": i, "data": &large_line}),
            );
        }

        // The log should have been rotated at least once
        let backup = dir.path().join("action-log.jsonl.1");
        assert!(backup.exists(), "a backup was created after rotation");

        // The current log should be smaller than MAX_SIZE + typical event size
        // (it rotates before append, so one more event can make it slightly over)
        let current_size = fs::metadata(&log).unwrap().len();
        assert!(
            current_size < MAX_SIZE + 2048,
            "the current log is near or under MAX_SIZE: {current_size} bytes"
        );

        // The backup should contain old events
        let backup_content = fs::read_to_string(&backup).unwrap();
        assert!(
            backup_content.contains("\"index\":0"),
            "the backup contains early events"
        );
    }

    #[test]
    fn rotation_replaces_an_older_backup() {
        let dir = TempDir::new("replace-backup");
        let log = dir.path().join(FILE);
        let backup = dir.path().join("action-log.jsonl.1");

        // Create an old backup
        fs::write(&backup, "old backup content\n").unwrap();

        // Create a log large enough to trigger rotation
        let large_line = "x".repeat(1024);
        for i in 0..2500 {
            append(
                dir.path(),
                "large",
                json!({"index": i, "data": &large_line}),
            );
        }

        // The old backup should have been replaced
        let backup_content = fs::read_to_string(&backup).unwrap();
        assert!(
            backup_content.contains("\"event\":\"large\""),
            "the old backup was replaced with rotated log content"
        );
        assert!(
            !backup_content.contains("old backup"),
            "the old backup content is gone"
        );
    }

    #[test]
    fn append_never_fails_even_when_rotation_would() {
        let dir = TempDir::new("rotation-failure");
        let log = dir.path().join(FILE);

        // Create a log that would exceed MAX_SIZE
        let large_data = "x".repeat((MAX_SIZE as usize) + 1);
        fs::write(&log, large_data).unwrap();

        // Make the backup path read-only so rotation cannot create it
        // (on Unix; on Windows this test just verifies append succeeds)
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o555)).unwrap();
        }

        // Append should still succeed even though rotation will fail
        append(dir.path(), "test", json!({"after": "rotation-failure"}));

        // The append happened (though rotation may have failed on Unix)
        let content = fs::read_to_string(&log).unwrap_or_else(|_| {
            // If the log couldn't be opened, the append itself failed (not just rotation)
            panic!("append should never fail, even when rotation cannot happen");
        });

        // On platforms where we could block rotation, verify the event landed anyway
        #[cfg(unix)]
        assert!(
            content.contains("after"),
            "the event was appended despite rotation being blocked"
        );
    }

    #[test]
    fn the_log_stays_valid_jsonl_after_rotation() {
        let dir = TempDir::new("valid-jsonl");

        // Append enough to trigger rotation
        let large_line = "x".repeat(1024);
        for i in 0..2500 {
            append(
                dir.path(),
                "large",
                json!({"index": i, "data": &large_line}),
            );
        }

        // Both files should be valid JSONL (every line is valid JSON)
        let log = dir.path().join(FILE);
        let backup = dir.path().join("action-log.jsonl.1");

        for path in [&log, &backup].iter().filter(|p| p.exists()) {
            let content = fs::read_to_string(path).unwrap();
            for (i, line) in content.lines().enumerate() {
                if !line.is_empty() {
                    serde_json::from_str::<Value>(line).unwrap_or_else(|e| {
                        panic!(
                            "line {i} in {} is not valid JSON: {e}\n{line}",
                            path.display()
                        );
                    });
                }
            }
        }
    }

    #[test]
    fn sustained_append_storm_stays_within_disk_bound() {
        let dir = TempDir::new("append-storm");
        let log = dir.path().join(FILE);
        let backup = dir.path().join("action-log.jsonl.1");

        // Simulate sustained heavy use: many appends
        let line = "x".repeat(200); // typical event size
        for i in 0..15_000 {
            append(dir.path(), "storm", json!({"index": i, "data": &line}));
        }

        // Total footprint should be at most 2 files × MAX_SIZE
        let log_size = fs::metadata(&log).map(|m| m.len()).unwrap_or(0);
        let backup_size = fs::metadata(&backup).map(|m| m.len()).unwrap_or(0);
        let total = log_size + backup_size;

        // Allow some overage for the last append that triggered rotation
        let bound = 2 * MAX_SIZE + 2048;
        assert!(
            total <= bound,
            "total disk footprint {total} bytes is within ~2×MAX_SIZE bound ({bound} bytes)"
        );
    }
}
