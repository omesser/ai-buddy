//! Windows settings window stub.
//!
//! The native settings window is deferred: Windows uses the plain Tauri window
//! that every platform gets. These are the dispatch points platform.rs calls.

use crate::settings::SettingsSession;

pub fn show(_session: SettingsSession) {
    eprintln!("settings: the native window is Windows in a later version");
}

pub fn refresh_settings() {}
