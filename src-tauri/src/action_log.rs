//! The Action Log: one JSON line per thing the attached Harness did.
//!
//! Append-only, in the data folder beside Memory. It points at the Harness's
//! own session (session id, tool call ids) rather than copying a transcript,
//! which is what CONTEXT.md asks of it. No reader yet; `tail -f` is the UI.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{json, Value};

pub const FILE: &str = "action-log.jsonl";

/// Write one event. `fields` is an object; `event` and `ts` are added to it.
///
/// A write that fails is dropped: the log explains the buddy after the fact
/// and must never be the reason a turn does not happen.
///
/// One `write_all` of one whole line, not `writeln!`: the wire thread and the
/// frame loop both append here, and `Display` on a `Value` is many small
/// writes that would interleave into a line no reader can parse. One append
/// write of a short buffer is not torn.
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
    let Ok(mut file) = OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join(FILE))
    else {
        return;
    };
    let _ = file.write_all(format!("{fields}\n").as_bytes());
}
