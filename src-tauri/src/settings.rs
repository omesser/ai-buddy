//! The user's standing choices, as a file they own.
//!
//! Settings is how #18 reaches the Director, hide rules, Memory, and launch
//! without finding the sprite. The document is JSON so a hand-edit is a text
//! editor, the same deal Memory already makes. Missing keys take their
//! defaults, so an older file keeps working when a field is added.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use ai_buddy_core::roster::InstanceSpec;
use ai_buddy_core::visibility::HideRules;
use serde::{Deserialize, Serialize};

/// What the settings window can change in one call.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct SettingsPatch {
    pub director_enabled: Option<bool>,
    pub ambient_wakes: Option<bool>,
    pub do_not_disturb: Option<bool>,
    pub hidden: Option<bool>,
    pub hide_in_fullscreen: Option<bool>,
    pub hide_hotkey: Option<String>,
    pub launch_at_login: Option<bool>,
    pub excluded_applications: Option<Vec<String>>,
    pub character: Option<String>,
}

impl Settings {
    pub fn apply(&mut self, patch: SettingsPatch) {
        if let Some(value) = patch.director_enabled {
            self.director_enabled = value;
        }
        if let Some(value) = patch.ambient_wakes {
            self.ambient_wakes = value;
        }
        if let Some(value) = patch.do_not_disturb {
            self.do_not_disturb = value;
        }
        if let Some(value) = patch.hidden {
            self.hidden = value;
        }
        if let Some(value) = patch.hide_in_fullscreen {
            self.hide_in_fullscreen = value;
        }
        if let Some(value) = patch.hide_hotkey {
            self.hide_hotkey = value;
        }
        if let Some(value) = patch.launch_at_login {
            self.launch_at_login = value;
        }
        if let Some(value) = patch.excluded_applications {
            self.excluded_applications = value;
        }
        if let Some(value) = patch.character {
            self.character = value;
        }
    }
}

/// The hide hotkey shipped until the user binds another.
///
/// Three modifiers, because a global shortcut is taken from every application
/// on the machine and B alone belongs to most of them. Spelled the way a Mac
/// keyboard names the keys, which is also what the menu prints.
pub const DEFAULT_HIDE_HOTKEY: &str = "Control-Option-Command-B";

/// Everything settings owns. Defaults are the v1 first-run answers.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Session Director on. Off leaves Static weights running the life.
    pub director_enabled: bool,
    /// Proactive session wakes. Off keeps the Director for Poke and Summon.
    pub ambient_wakes: bool,
    /// Quiet: on screen, not starting things. Persists so a restart stays quiet.
    pub do_not_disturb: bool,
    /// Off screen, same flag the hotkey flips.
    pub hidden: bool,
    /// Fade away when a fullscreen application is frontmost.
    pub hide_in_fullscreen: bool,
    pub hide_hotkey: String,
    pub launch_at_login: bool,
    pub excluded_applications: Vec<String>,
    /// Last chosen Character Package. Empty means the loader's default.
    pub character: String,
    /// Instances to spawn on launch. Empty means the one buddy first-run runs.
    pub instances: Vec<InstanceSpec>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            director_enabled: true,
            ambient_wakes: true,
            do_not_disturb: false,
            hidden: false,
            hide_in_fullscreen: true,
            hide_hotkey: DEFAULT_HIDE_HOTKEY.to_string(),
            launch_at_login: false,
            excluded_applications: Vec::new(),
            character: String::new(),
            instances: Vec::new(),
        }
    }
}

impl Settings {
    /// Read the document at `path`. A missing file is first-run defaults.
    ///
    /// A file that cannot be parsed is also defaults rather than a refused
    /// launch: a typo in a hand-edit must not cost the buddy, the same
    /// degradation Memory already chose.
    pub fn load(path: &Path) -> Self {
        match fs::read_to_string(path) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => Self::default(),
            Err(_) => Self::default(),
            Ok(text) => serde_json::from_str(&text).unwrap_or_default(),
        }
    }

    /// Write the document, creating the parent directory if needed.
    pub fn save(&self, path: &Path) -> io::Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let text = serde_json::to_string_pretty(self)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        fs::write(path, text)
    }
}

/// Where settings lives beside Memory, so both are in one folder the user owns.
pub fn settings_path(data_dir: &Path) -> PathBuf {
    data_dir.join("settings.json")
}

/// The modifiers and key a hide-hotkey string names.
///
/// Parsed here rather than by the shortcut plugin so a bad binding is a
/// settings problem, not a plugin one, and the default can take over.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hotkey {
    pub control: bool,
    pub option: bool,
    pub shift: bool,
    pub command: bool,
    pub key: char,
}

/// Read `Control-Option-Command-B` into parts. Unknown tokens refuse the
/// whole string so a typo cannot silently drop a modifier.
pub fn parse_hotkey(spec: &str) -> Option<Hotkey> {
    let mut hotkey = Hotkey {
        control: false,
        option: false,
        shift: false,
        command: false,
        key: '\0',
    };
    let mut key = None;
    for token in spec
        .split('-')
        .map(str::trim)
        .filter(|token| !token.is_empty())
    {
        match token {
            "Control" | "Ctrl" => hotkey.control = true,
            "Option" | "Alt" => hotkey.option = true,
            "Shift" => hotkey.shift = true,
            "Command" | "Super" | "Meta" => hotkey.command = true,
            one if one.len() == 1 => {
                let letter = one.chars().next()?.to_ascii_uppercase();
                if !letter.is_ascii_alphabetic() {
                    return None;
                }
                if key.is_some() {
                    return None;
                }
                key = Some(letter);
            }
            _ => return None,
        }
    }
    hotkey.key = key?;
    Some(hotkey)
}

/// Flip Go-away and keep `Settings.hidden` on the same flag, so a restart or
/// a later patch cannot undo a hotkey hide the menu already persisted.
pub fn toggle_away(rules: &mut HideRules, settings: &mut Settings) {
    rules.toggle();
    settings.hidden = rules.is_away();
}

/// The shortcut plugin's `Code` name for a letter, e.g. `KeyH`.
///
/// Letters only: `parse_hotkey` already refuses anything else, and a name
/// the plugin cannot parse must not silently become `KeyB`.
pub fn key_code_name(key: char) -> Option<String> {
    if key.is_ascii_alphabetic() {
        Some(format!("Key{}", key.to_ascii_uppercase()))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT: AtomicU64 = AtomicU64::new(0);

    fn temp_path() -> PathBuf {
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "ai-buddy-settings-{n}-{:?}.json",
            std::thread::current().id()
        ))
    }

    /// A missing file is first-run, not an error: that is how every new user starts.
    #[test]
    fn a_missing_file_is_first_run_defaults() {
        let path = std::env::temp_dir().join("ai-buddy-settings-does-not-exist.json");
        let _ = fs::remove_file(&path);

        assert_eq!(Settings::load(&path), Settings::default());
        assert!(Settings::default().director_enabled);
        assert!(Settings::default().ambient_wakes);
        assert!(Settings::default().hide_in_fullscreen);
        assert!(!Settings::default().do_not_disturb);
        assert!(!Settings::default().hidden);
        assert!(!Settings::default().launch_at_login);
        assert_eq!(Settings::default().hide_hotkey, DEFAULT_HIDE_HOTKEY);
    }

    /// What the user set is what the next launch reads. That is the whole file.
    #[test]
    fn a_saved_document_round_trips() {
        let path = temp_path();
        let _ = fs::remove_file(&path);

        let settings = Settings {
            director_enabled: false,
            ambient_wakes: false,
            do_not_disturb: true,
            hidden: true,
            hide_in_fullscreen: false,
            hide_hotkey: "Control-Shift-H".to_string(),
            launch_at_login: true,
            excluded_applications: vec!["1Password".to_string(), "Keychain Access".to_string()],
            character: "nim".to_string(),
            instances: vec![InstanceSpec {
                character: "bmo".to_string(),
                name: "Beemo".to_string(),
            }],
        };
        settings.save(&path).expect("save");

        assert_eq!(Settings::load(&path), settings);
        let _ = fs::remove_file(&path);
    }

    /// Ambient wakes are their own switch. Turning them off must not turn the
    /// Director off, or Poke would go silent with idle life.
    #[test]
    fn ambient_wakes_can_be_off_while_the_director_stays_on() {
        let settings = Settings {
            director_enabled: true,
            ambient_wakes: false,
            ..Settings::default()
        };

        assert!(settings.director_enabled);
        assert!(!settings.ambient_wakes);
    }

    /// A hand-edit that drops a key, or an older file, must not refuse to load.
    #[test]
    fn a_partial_document_fills_missing_keys_from_defaults() {
        let path = temp_path();
        fs::write(&path, r#"{"director_enabled":false}"#).expect("write");

        let settings = Settings::load(&path);
        assert!(!settings.director_enabled);
        assert!(settings.ambient_wakes, "unset ambient stays on");
        assert!(settings.hide_in_fullscreen);
        let _ = fs::remove_file(&path);
    }

    /// Garbage is first-run rather than a crash. The buddy staying up is the
    /// product; the file can be rewritten the next time something is toggled.
    #[test]
    fn a_corrupt_document_degrades_to_defaults() {
        let path = temp_path();
        fs::write(&path, "not json {").expect("write");

        assert_eq!(Settings::load(&path), Settings::default());
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn settings_sit_beside_memory_in_the_data_dir() {
        assert_eq!(
            settings_path(Path::new("/tmp/ai-buddy")),
            PathBuf::from("/tmp/ai-buddy/settings.json")
        );
    }

    #[test]
    fn the_default_hide_hotkey_parses() {
        assert_eq!(
            parse_hotkey(DEFAULT_HIDE_HOTKEY),
            Some(Hotkey {
                control: true,
                option: true,
                shift: false,
                command: true,
                key: 'B',
            })
        );
    }

    /// A hotkey hide that does not write `hidden` comes back on restart, and a
    /// later settings patch of something else would overwrite HideRules with
    /// the stale flag. The menu already persists; the hotkey must too.
    #[test]
    fn hiding_from_the_hotkey_is_what_the_next_launch_reads() {
        let mut rules = HideRules::default();
        let mut settings = Settings::default();
        assert!(!settings.hidden);

        toggle_away(&mut rules, &mut settings);
        assert!(rules.is_away());
        assert!(
            settings.hidden,
            "hotkey hide must set the same flag the menu writes"
        );

        let path = temp_path();
        settings.save(&path).expect("save");
        let loaded = Settings::load(&path);
        let mut restarted = HideRules::default();
        restarted.set_away(loaded.hidden);
        assert!(restarted.is_away());
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn a_rebound_hotkey_names_its_own_key_not_the_shipped_b() {
        assert_eq!(parse_hotkey("Control-Shift-H").map(|h| h.key), Some('H'));
        assert_eq!(key_code_name('H').as_deref(), Some("KeyH"));
        assert_eq!(key_code_name('B').as_deref(), Some("KeyB"));
        assert_eq!(key_code_name('1'), None);
    }

    #[test]
    fn a_bad_hotkey_is_refused_rather_than_half_applied() {
        assert_eq!(
            parse_hotkey("B"),
            Some(Hotkey {
                control: false,
                option: false,
                shift: false,
                command: false,
                key: 'B',
            })
        );
        assert_eq!(parse_hotkey("Control-Option-Command"), None);
        assert_eq!(parse_hotkey("Control-F1"), None);
        assert_eq!(parse_hotkey("Control-B-C"), None);
        assert_eq!(parse_hotkey(""), None);
    }
}
