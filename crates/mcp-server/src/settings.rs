//! Settings file I/O for the MCP server.
//!
//! Loads settings.json beside Memory to build a DenyList. Missing or unreadable
//! files produce empty excluded lists with password filtering enabled (first-run
//! product behavior).

use ai_buddy_core::dispatch::DenyList;
use serde::Deserialize;
use std::path::Path;

pub fn load_denylist_from_settings(path: &Path) -> DenyList {
    #[derive(Deserialize, Default)]
    struct SettingsDoc {
        #[serde(default)]
        excluded_applications: Vec<String>,
    }

    let excluded_applications = std::fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str::<SettingsDoc>(&text).ok())
        .map(|doc| doc.excluded_applications)
        .unwrap_or_default();

    DenyList {
        excluded_applications,
        filter_password_fields: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU32, Ordering};

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(label: &str) -> Self {
            static NEXT: AtomicU32 = AtomicU32::new(0);
            let unique = NEXT.fetch_add(1, Ordering::Relaxed);
            let dir = std::env::temp_dir().join(format!(
                "ai-buddy-mcp-settings-{label}-{}-{unique}",
                std::process::id()
            ));
            fs::create_dir_all(&dir).expect("temp dir is creatable");
            Self(dir)
        }

        fn join(&self, name: &str) -> PathBuf {
            self.0.join(name)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn denylist_from_settings_hides_those_applications() {
        let dir = TempDir::new("denylist-settings");
        let path = dir.join("settings.json");
        fs::write(
            &path,
            r#"{"excluded_applications":["1Password","Keychain Access"]}"#,
        )
        .expect("write");

        let denylist = load_denylist_from_settings(&path);
        assert!(!denylist.allows("1Password"));
        assert!(!denylist.allows("Keychain Access"));
        assert!(denylist.allows("Terminal"));
        assert!(denylist.filter_password_fields);
    }

    #[test]
    fn denylist_from_a_missing_settings_file_excludes_nothing() {
        let path = std::env::temp_dir().join("ai-buddy-no-such-settings.json");
        let _ = fs::remove_file(&path);
        let denylist = load_denylist_from_settings(&path);
        assert!(denylist.allows("1Password"));
        assert!(denylist.filter_password_fields);
    }
}
