//! Windows settings window stub.
//!
//! Native settings window deferred: Windows uses the plain Tauri window. These
//! are the dispatch points platform.rs calls.

use crate::settings::SettingsSession;

pub fn show_settings(_session: SettingsSession) {
    eprintln!("settings: the native window is Windows in a later version");
}

pub fn refresh_settings() {}
