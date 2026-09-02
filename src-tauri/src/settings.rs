//! The user's standing choices, as a file they own.
//!
//! Settings is how #18 reaches the Director, hide rules, Memory, and launch
//! without finding the sprite. The document is JSON so a hand-edit is a text
//! editor, the same deal Memory already makes. Missing keys take their
//! defaults, so an older file keeps working when a field is added.

pub mod form;

use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};

use ai_buddy_core::memory::MemoryManifest;
use ai_buddy_core::roster::InstanceSpec;
use ai_buddy_core::visibility::HideRules;
use serde::{Deserialize, Serialize};
use tauri::AppHandle;

use crate::consent::{self, CapabilityId, ConsentRow};
use crate::model::{self, DirectorInspect, DirectorSettings};
use crate::secrets::{SecretStore, DIRECTOR_API_KEY};

/// One running buddy, as settings lists it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstanceRow {
    pub id: String,
    pub name: String,
    pub character: String,
}

/// What the settings window shows. Built from the live file and roster so the
/// window holds no copy that could drift.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SettingsView {
    pub director_enabled: bool,
    pub ambient_wakes: bool,
    pub do_not_disturb: bool,
    pub hidden: bool,
    pub hide_in_fullscreen: bool,
    pub hide_hotkey: String,
    pub excluded_applications: Vec<String>,
    pub character: String,
    pub memory_path: String,
    pub last_payload: Option<String>,
    pub installed: Vec<String>,
    pub instances: Vec<InstanceRow>,
    pub director_base_url: String,
    pub director_model: String,
    /// Whether a key is stored — never the raw secret itself.
    pub api_key_set: bool,
    pub api_key_fingerprint: String,
    /// Non-empty when the last store read failed. Distinct from unset.
    pub api_key_error: String,
    /// Live OS grants, not a file field. The window rereads them on become-key.
    pub consent: Vec<ConsentRow>,
    /// The name Privacy & Security will show for this process.
    pub consent_listed_as: String,
}

impl SettingsView {
    pub fn from_parts(
        settings: &Settings,
        memory_path: &Path,
        last_payload: Option<String>,
        installed: Vec<String>,
        instances: Vec<InstanceRow>,
        api_key: (bool, String, String),
    ) -> Self {
        let (api_key_set, api_key_fingerprint, api_key_error) = api_key;
        Self {
            director_enabled: settings.director_enabled,
            ambient_wakes: settings.ambient_wakes,
            do_not_disturb: settings.do_not_disturb,
            hidden: settings.hidden,
            hide_in_fullscreen: settings.hide_in_fullscreen,
            hide_hotkey: settings.hide_hotkey.clone(),
            excluded_applications: settings.excluded_applications.clone(),
            character: settings.character.clone(),
            memory_path: memory_path.display().to_string(),
            last_payload,
            installed,
            instances,
            director_base_url: settings.director_base_url.clone(),
            director_model: settings.director_model.clone(),
            api_key_set,
            api_key_fingerprint,
            api_key_error,
            consent: consent::rows(|id| settings.wants_consent(id)),
            consent_listed_as: String::new(),
        }
    }

    /// The pane copy. The listed name is live: a `cargo run` from Cursor is
    /// Cursor, a packaged build is ai-buddy.
    pub fn consent_intro(&self) -> String {
        consent::pane_intro(&self.consent_listed_as)
    }

    /// One name per line, the same text the excluded-applications field edits.
    pub fn excluded_text(&self) -> String {
        self.excluded_applications.join("\n")
    }

    /// The Instances list as the window prints it: name, then Character.
    pub fn instance_lines(&self) -> Vec<String> {
        self.instances
            .iter()
            .map(|row| format!("{} ({})", row.name, row.character))
            .collect()
    }

    #[cfg(test)]
    pub fn api_key_placeholder(&self) -> String {
        if !self.api_key_error.is_empty() {
            format!("Unavailable — {}", self.api_key_error)
        } else if self.api_key_set {
            format!("Set — {}", self.api_key_fingerprint)
        } else {
            "Not set".into()
        }
    }

    #[cfg(test)]
    pub fn clear_key_enabled(&self) -> bool {
        self.api_key_set || !self.api_key_error.is_empty()
    }
}

/// Work the settings window asks the frame loop to do.
#[derive(Clone, Debug)]
pub enum SettingsOp {
    Spawn {
        character: String,
        name: String,
    },
    Dismiss {
        id: String,
    },
    SwitchAll {
        character: String,
    },
    /// Completer target changed. Resolved off the frame thread so the loop
    /// never reads Keychain. Drops in-flight session history (ADR-0008).
    Retarget {
        settings: DirectorSettings,
        enabled: bool,
        ambient_allowed: bool,
        configured: bool,
    },
}

/// Write the Director API key to the secret store, never to the settings file.
///
/// Empty after the same trim env keys use is delete: a quoted blank must not
/// become a Bearer of quotes.
pub fn write_director_key(store: &dyn SecretStore, patch: &SettingsPatch) -> Result<(), String> {
    match patch.director_api_key.as_deref() {
        None => Ok(()),
        Some(value) => match model::trim_key(value) {
            Some(key) => store.set(DIRECTOR_API_KEY, &key),
            None => store.delete(DIRECTOR_API_KEY),
        },
    }
}

/// Env, then the file, then the store. A store read error is `Err`, not Unset:
/// treating it as no key would drop a remote Completer to Static on Retarget.
pub fn director_settings(
    settings: &Settings,
    secrets: &dyn SecretStore,
) -> Result<DirectorSettings, String> {
    let stored = secrets.get(DIRECTOR_API_KEY)?;
    Ok(model::resolve(
        &settings.director_base_url,
        &settings.director_model,
        stored.as_deref(),
    ))
}

/// Write the key first so a store error cannot leave a URL in memory that
/// was never saved or sent as Retarget.
#[cfg(test)]
fn apply_with_store(
    settings: &mut Settings,
    store: &dyn SecretStore,
    patch: SettingsPatch,
) -> Result<(), String> {
    write_director_key(store, &patch)?;
    settings.apply(patch);
    Ok(())
}

/// `Some` on the key always retargets: we cannot compare a secret to the
/// file. URL and model retarget only when the value actually changed —
/// `commit_endpoint` sends `Some` on every blur, including an unchanged field.
fn completer_retargets(settings: &Settings, patch: &SettingsPatch) -> bool {
    patch.director_api_key.is_some()
        || patch
            .director_base_url
            .as_ref()
            .is_some_and(|url| url != &settings.director_base_url)
        || patch
            .director_model
            .as_ref()
            .is_some_and(|model| model != &settings.director_model)
}

/// Fingerprint plus an error string: a get `Err` is not Unset. Log like the
/// other store call sites so a locked Keychain is visible.
fn stored_key_status(store: &dyn SecretStore) -> (bool, String, String) {
    match store.get(DIRECTOR_API_KEY) {
        Ok(Some(key)) => (true, model::key_fingerprint(&key), String::new()),
        Ok(None) => (false, String::new(), String::new()),
        Err(why) => {
            eprintln!("director: secret store: {why}");
            (false, String::new(), why)
        }
    }
}

/// Resolve Director settings on the settings thread. The frame loop only applies them.
fn retarget_payload(settings: &Settings, store: &dyn SecretStore) -> Result<SettingsOp, String> {
    let director = director_settings(settings, store)?;
    let cfg = model::config_from(&director);
    Ok(SettingsOp::Retarget {
        settings: director,
        enabled: settings.director_enabled && cfg.configured,
        ambient_allowed: settings.ambient_wakes,
        configured: cfg.configured,
    })
}

/// Everything the native settings window needs to read and write.
pub struct SettingsSession {
    pub settings: Arc<Mutex<Settings>>,
    pub path: PathBuf,
    pub memory_path: PathBuf,
    pub rules: Arc<Mutex<HideRules>>,
    pub inspect: Arc<Mutex<DirectorInspect>>,
    pub instances: Arc<Mutex<Vec<InstanceRow>>>,
    pub installed: Vec<String>,
    pub ops: mpsc::Sender<SettingsOp>,
    pub app: AppHandle,
    pub on_rebind: fn(&AppHandle, &str),
    pub secrets: Arc<dyn SecretStore>,
    /// Last successful store fingerprint. Become-key must not hit Keychain
    /// every focus; a failed read is not cached, so unlocking can recover.
    pub key_cache: Mutex<Option<(bool, String)>>,
}

impl SettingsSession {
    pub fn view(&self) -> SettingsView {
        let settings = self
            .settings
            .lock()
            .map(|guard| guard.clone())
            .unwrap_or_else(|poisoned| poisoned.into_inner().clone());
        let instances = self
            .instances
            .lock()
            .map(|guard| guard.clone())
            .unwrap_or_else(|poisoned| poisoned.into_inner().clone());
        let last_payload = self
            .inspect
            .lock()
            .ok()
            .and_then(|inspect| inspect.last_payload.clone());
        let mut view = SettingsView::from_parts(
            &settings,
            &self.memory_path,
            last_payload,
            self.installed.clone(),
            instances,
            self.key_status_for_view(),
        );
        view.consent_listed_as = consent::process_listed_as();
        view
    }

    /// Flip on: persist intent, then the system prompt if the OS has not
    /// granted yet. Flip off: persist intent and stop using the grant; the
    /// OS grant stays until Privacy & Security revokes it.
    pub fn enable_consent(&self, id: CapabilityId) {
        consent::enable(id, consent::live());
    }

    pub fn apply(&self, patch: SettingsPatch) -> Result<(), String> {
        let switching = patch.character.clone();
        let rebind = patch.hide_hotkey.clone();
        write_director_key(self.secrets.as_ref(), &patch)?;
        if let Some(raw) = patch.director_api_key.as_deref() {
            self.remember_written_key(raw);
        }
        let prompt_ax = patch.use_accessibility == Some(true);
        let prompt_sr = patch.use_screen_recording == Some(true);
        let mut settings = self.settings.lock().map_err(|error| error.to_string())?;
        let retarget = completer_retargets(&settings, &patch);
        settings.apply(patch);
        consent::set_wanted(CapabilityId::Accessibility, settings.use_accessibility);
        consent::set_wanted(CapabilityId::ScreenRecording, settings.use_screen_recording);
        if let Ok(mut rules) = self.rules.lock() {
            rules.set_away(settings.hidden);
            rules.set_hide_in_fullscreen(settings.hide_in_fullscreen);
        }
        let snapshot = settings.clone();
        settings
            .save(&self.path)
            .map_err(|error| error.to_string())?;
        drop(settings);

        if let Some(name) = switching {
            let _ = self.ops.send(SettingsOp::SwitchAll { character: name });
        }
        if retarget {
            match retarget_payload(&snapshot, self.secrets.as_ref()) {
                Ok(op) => {
                    let _ = self.ops.send(op);
                }
                Err(why) => eprintln!("director: secret store: {why}"),
            }
        }
        if let Some(spec) = rebind {
            (self.on_rebind)(&self.app, &spec);
        }
        if prompt_ax {
            self.enable_consent(CapabilityId::Accessibility);
        }
        if prompt_sr {
            self.enable_consent(CapabilityId::ScreenRecording);
        }
        Ok(())
    }

    fn remember_written_key(&self, raw: &str) {
        let cached = match model::trim_key(raw) {
            Some(key) => Some((true, model::key_fingerprint(&key))),
            None => Some((false, String::new())),
        };
        if let Ok(mut cache) = self.key_cache.lock() {
            *cache = cached;
        }
    }

    fn key_status_for_view(&self) -> (bool, String, String) {
        if let Ok(cache) = self.key_cache.lock() {
            if let Some((set, fingerprint)) = cache.as_ref() {
                return (*set, fingerprint.clone(), String::new());
            }
        }
        let (set, fingerprint, error) = stored_key_status(self.secrets.as_ref());
        if error.is_empty() {
            if let Ok(mut cache) = self.key_cache.lock() {
                *cache = Some((set, fingerprint.clone()));
            }
        }
        (set, fingerprint, error)
    }

    pub fn open_memory(&self) -> Result<(), String> {
        open_in_editor(&self.memory_path)
    }

    pub fn wipe_memory(&self) -> Result<(), String> {
        MemoryManifest::new(&self.memory_path)
            .wipe()
            .map(|_| ())
            .map_err(|error| error.to_string())
    }

    pub fn spawn(&self, character: String, name: String) {
        let _ = self.ops.send(SettingsOp::Spawn { character, name });
    }

    pub fn dismiss(&self, id: String) {
        let _ = self.ops.send(SettingsOp::Dismiss { id });
    }
}

fn open_in_editor(path: &Path) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    if !path.exists() {
        fs::write(path, "").map_err(|error| error.to_string())?;
    }
    std::process::Command::new("open")
        .arg(path)
        .spawn()
        .map(|_| ())
        .map_err(|error| error.to_string())
}

/// What the settings window can change in one call.
#[derive(Clone, Default, Deserialize)]
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
    pub director_base_url: Option<String>,
    pub director_model: Option<String>,
    /// Present so callers can write the store; `Settings::apply` ignores it
    /// because the key is not a file field.
    pub director_api_key: Option<String>,
    pub use_accessibility: Option<bool>,
    pub use_screen_recording: Option<bool>,
}

impl fmt::Debug for SettingsPatch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SettingsPatch")
            .field("director_enabled", &self.director_enabled)
            .field("ambient_wakes", &self.ambient_wakes)
            .field("do_not_disturb", &self.do_not_disturb)
            .field("hidden", &self.hidden)
            .field("hide_in_fullscreen", &self.hide_in_fullscreen)
            .field("hide_hotkey", &self.hide_hotkey)
            .field("launch_at_login", &self.launch_at_login)
            .field("excluded_applications", &self.excluded_applications)
            .field("character", &self.character)
            .field("director_base_url", &self.director_base_url)
            .field("director_model", &self.director_model)
            .field(
                "director_api_key",
                &self.director_api_key.as_deref().map(model::key_fingerprint),
            )
            .field("use_accessibility", &self.use_accessibility)
            .field("use_screen_recording", &self.use_screen_recording)
            .finish()
    }
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
        if let Some(value) = patch.director_base_url {
            self.director_base_url = value;
        }
        if let Some(value) = patch.director_model {
            self.director_model = value;
        }
        if let Some(value) = patch.use_accessibility {
            self.use_accessibility = value;
        }
        if let Some(value) = patch.use_screen_recording {
            self.use_screen_recording = value;
        }
        // director_api_key is intentionally ignored: the key lives in the
        // secret store, never in the JSON document.
    }

    pub fn wants_consent(&self, id: CapabilityId) -> bool {
        match id {
            CapabilityId::Accessibility => self.use_accessibility,
            CapabilityId::ScreenRecording => self.use_screen_recording,
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
    /// Empty means unset — Completer resolution falls through to env then defaults.
    pub director_base_url: String,
    /// Empty means unset — Completer resolution falls through to env then defaults.
    pub director_model: String,
    /// Use Accessibility where the OS has granted it. Off does not revoke TCC.
    pub use_accessibility: bool,
    /// Use Screen Recording where the OS has granted it. Off does not revoke TCC.
    pub use_screen_recording: bool,
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
            director_base_url: String::new(),
            director_model: String::new(),
            use_accessibility: false,
            use_screen_recording: false,
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

    /// First launch may write under an app-data dir that does not exist yet.
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
    use crate::secrets::{MemoryStore, SecretStore, DIRECTOR_API_KEY};
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
            director_base_url: "https://api.x.ai".into(),
            director_model: "grok-4.6".into(),
            use_accessibility: true,
            use_screen_recording: false,
        };
        settings.save(&path).expect("save");

        assert_eq!(Settings::load(&path), settings);
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn a_partial_document_fills_missing_director_endpoint_from_defaults() {
        let path = temp_path();
        fs::write(&path, r#"{"director_enabled":false}"#).expect("write");
        let settings = Settings::load(&path);
        assert!(settings.director_base_url.is_empty());
        assert!(settings.director_model.is_empty());
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn the_saved_document_does_not_carry_an_api_key() {
        let path = temp_path();
        let settings = Settings {
            director_base_url: "https://api.x.ai".to_string(),
            director_model: "grok-4.6".to_string(),
            ..Settings::default()
        };
        settings.save(&path).expect("save");
        let text = fs::read_to_string(&path).expect("read");
        assert!(!text.contains("api_key"), "{text}");
        assert!(!text.contains("sk-"), "{text}");
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn the_settings_view_never_holds_the_raw_key() {
        let settings = Settings {
            director_base_url: "https://api.x.ai".to_string(),
            director_model: "grok-4.6".to_string(),
            ..Settings::default()
        };
        let view = SettingsView::from_parts(
            &settings,
            Path::new("/tmp/ai-buddy/memory.md"),
            Some("You are Nim.".to_string()),
            vec!["nim".to_string()],
            Vec::new(),
            (true, "len=12 last=key1".to_string(), String::new()),
        );
        assert_eq!(view.director_base_url, "https://api.x.ai");
        assert_eq!(view.director_model, "grok-4.6");
        assert!(view.api_key_set);
        assert_eq!(view.api_key_fingerprint, "len=12 last=key1");
        assert_eq!(view.api_key_placeholder(), "Set — len=12 last=key1");
        let dump = format!("{view:?}");
        assert!(!dump.contains("sk-"), "{dump}");
    }

    #[test]
    fn the_key_placeholder_is_not_set_when_unset() {
        let view = SettingsView::from_parts(
            &Settings::default(),
            Path::new("/tmp/ai-buddy/memory.md"),
            None,
            Vec::new(),
            Vec::new(),
            (false, String::new(), String::new()),
        );
        assert_eq!(view.api_key_placeholder(), "Not set");
        assert!(!view.clear_key_enabled());
    }

    #[test]
    fn the_key_placeholder_is_unavailable_when_the_store_cannot_be_read() {
        let view = SettingsView::from_parts(
            &Settings::default(),
            Path::new("/tmp/ai-buddy/memory.md"),
            None,
            Vec::new(),
            Vec::new(),
            (false, String::new(), "keychain locked".into()),
        );
        assert!(!view.api_key_set);
        assert_eq!(view.api_key_placeholder(), "Unavailable — keychain locked");
        assert!(
            view.clear_key_enabled(),
            "Clear stays offered so a key we could not read can still be wiped"
        );
        assert_ne!(
            view.api_key_placeholder(),
            "Not set",
            "a locked store must not look like no key"
        );
    }

    #[test]
    fn apply_ignores_the_api_key_patch_on_the_file() {
        let mut settings = Settings::default();
        settings.apply(SettingsPatch {
            director_base_url: Some("https://api.x.ai".to_string()),
            director_model: Some("grok-4.6".to_string()),
            director_api_key: Some("sk-should-not-land".to_string()),
            ..SettingsPatch::default()
        });
        assert_eq!(settings.director_base_url, "https://api.x.ai");
        assert_eq!(settings.director_model, "grok-4.6");
        let path = temp_path();
        settings.save(&path).expect("save");
        let text = fs::read_to_string(&path).expect("read");
        assert!(!text.contains("sk-should-not-land"), "{text}");
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn settings_patch_debug_omits_the_raw_key() {
        let patch = SettingsPatch {
            director_api_key: Some("sk-super-secret-key".into()),
            ..SettingsPatch::default()
        };
        let dump = format!("{patch:?}");
        assert!(
            !dump.contains("sk-super-secret-key"),
            "Debug must not echo the key: {dump}"
        );
        assert!(
            dump.contains(&model::key_fingerprint("sk-super-secret-key")),
            "Debug should name the fingerprint: {dump}"
        );
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

    /// The window reads this snapshot, not the file, so a field that does not
    /// appear here is a field the user cannot see.
    #[test]
    fn the_settings_view_is_what_the_window_shows() {
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
            instances: Vec::new(),
            director_base_url: String::new(),
            director_model: String::new(),
            use_accessibility: true,
            use_screen_recording: false,
        };
        let mut view = SettingsView::from_parts(
            &settings,
            Path::new("/tmp/ai-buddy/memory.md"),
            Some("You are Nim.".to_string()),
            vec!["bmo".to_string(), "nim".to_string()],
            vec![InstanceRow {
                id: "1".to_string(),
                name: "Nim".to_string(),
                character: "nim".to_string(),
            }],
            (false, String::new(), String::new()),
        );
        assert!(!view.director_enabled);
        assert!(!view.ambient_wakes);
        assert!(view.do_not_disturb);
        assert!(view.hidden);
        assert!(!view.hide_in_fullscreen);
        assert_eq!(view.hide_hotkey, "Control-Shift-H");
        assert_eq!(view.excluded_text(), "1Password\nKeychain Access");
        assert_eq!(view.character, "nim");
        assert_eq!(view.memory_path, "/tmp/ai-buddy/memory.md");
        assert_eq!(view.last_payload.as_deref(), Some("You are Nim."));
        assert_eq!(view.installed, ["bmo", "nim"]);
        assert_eq!(view.instances[0].name, "Nim");
        assert_eq!(view.instance_lines(), ["Nim (nim)"]);
        assert!(!view.api_key_set);
        assert_eq!(
            view.consent.iter().map(|row| row.title).collect::<Vec<_>>(),
            ["Accessibility", "Screen Recording"]
        );
        assert!(
            view.consent[0].granted,
            "the checkbox follows settings intent, not the OS grant"
        );
        assert!(!view.consent[1].granted);
        view.consent_listed_as = "Cursor".into();
        assert!(
            view.consent_intro().contains("Cursor"),
            "the pane has to name the TCC row, got {:?}",
            view.consent_intro()
        );
    }

    #[test]
    fn unchecking_consent_stops_using_it_without_a_file_grant() {
        let mut settings = Settings::default();
        settings.apply(SettingsPatch {
            use_accessibility: Some(true),
            ..SettingsPatch::default()
        });
        assert!(settings.use_accessibility);
        settings.apply(SettingsPatch {
            use_accessibility: Some(false),
            ..SettingsPatch::default()
        });
        assert!(!settings.use_accessibility);
        let view = SettingsView::from_parts(
            &settings,
            Path::new("/tmp/memory.md"),
            None,
            Vec::new(),
            Vec::new(),
            (false, String::new(), String::new()),
        );
        assert!(
            !view.consent[0].granted,
            "unchecking has to show off even if the OS still holds the grant"
        );
    }

    /// The Instances list is this view. After a dismiss the window must
    /// redraw from a view that no longer carries the gone row.
    #[test]
    fn a_dismissed_instance_is_gone_from_the_settings_list() {
        let settings = Settings::default();
        let remaining = vec![InstanceRow {
            id: "trump".to_string(),
            name: "Trump".to_string(),
            character: "Trump".to_string(),
        }];
        let view = SettingsView::from_parts(
            &settings,
            Path::new("/tmp/memory.md"),
            None,
            vec!["Trump".to_string(), "Cat".to_string()],
            remaining,
            (false, String::new(), String::new()),
        );
        assert_eq!(view.instance_lines(), ["Trump (Trump)"]);
        assert!(
            view.instance_lines()
                .iter()
                .all(|line| !line.contains("Cat")),
            "Cat must not remain after it was dismissed"
        );
    }

    #[test]
    fn write_director_key_sets_the_store_and_not_the_file() {
        let store = MemoryStore::new();
        let patch = SettingsPatch {
            director_api_key: Some("sk-from-settings".to_string()),
            ..SettingsPatch::default()
        };
        write_director_key(&store, &patch).unwrap();
        assert_eq!(
            store.get(DIRECTOR_API_KEY).unwrap().as_deref(),
            Some("sk-from-settings")
        );
    }

    #[test]
    fn write_director_key_clears_on_empty() {
        let store = MemoryStore::new();
        store.set(DIRECTOR_API_KEY, "sk-from-settings").unwrap();
        let patch = SettingsPatch {
            director_api_key: Some(String::new()),
            ..SettingsPatch::default()
        };
        write_director_key(&store, &patch).unwrap();
        assert_eq!(store.get(DIRECTOR_API_KEY).unwrap(), None);
    }

    #[test]
    fn write_director_key_none_leaves_the_store() {
        let store = MemoryStore::new();
        store.set(DIRECTOR_API_KEY, "sk-from-settings").unwrap();
        write_director_key(&store, &SettingsPatch::default()).unwrap();
        assert_eq!(
            store.get(DIRECTOR_API_KEY).unwrap().as_deref(),
            Some("sk-from-settings")
        );
    }

    struct FailingStore;

    impl SecretStore for FailingStore {
        fn get(&self, _: &str) -> Result<Option<String>, String> {
            Err("keychain locked".into())
        }
        fn set(&self, _: &str, _: &str) -> Result<(), String> {
            Err("keychain locked".into())
        }
        fn delete(&self, _: &str) -> Result<(), String> {
            Err("keychain locked".into())
        }
    }

    #[test]
    fn a_store_get_error_is_not_an_unset_remote_key() {
        let settings = Settings {
            director_base_url: "https://api.openai.com".into(),
            director_model: "gpt-4o-mini".into(),
            ..Settings::default()
        };
        assert!(
            director_settings(&settings, &FailingStore).is_err(),
            "a store error must not resolve as no key"
        );
        let unset = model::resolve(&settings.director_base_url, &settings.director_model, None);
        assert!(
            !model::config_from(&unset).configured,
            "precondition: unset remote is Static"
        );
    }

    #[test]
    fn a_store_get_error_is_not_presented_as_unset() {
        let (set, fingerprint, error) = stored_key_status(&FailingStore);
        assert!(
            !error.is_empty(),
            "a get Err must carry the failure, not look like Unset"
        );
        assert!(!set);
        assert!(fingerprint.is_empty());
        let view = SettingsView::from_parts(
            &Settings::default(),
            Path::new("/tmp/ai-buddy/memory.md"),
            None,
            Vec::new(),
            Vec::new(),
            (set, fingerprint, error),
        );
        assert_ne!(view.api_key_placeholder(), "Not set");
        assert!(view.clear_key_enabled());
        let (unset, empty, no_error) = stored_key_status(&MemoryStore::new());
        assert!(!unset);
        assert!(empty.is_empty());
        assert!(no_error.is_empty());
    }

    fn endpoint_settings() -> Settings {
        Settings {
            director_base_url: "https://api.openai.com".into(),
            director_model: "gpt-4o-mini".into(),
            ..Settings::default()
        }
    }

    #[test]
    fn tabbing_out_of_an_unchanged_endpoint_does_not_retarget() {
        let settings = endpoint_settings();
        let patch = SettingsPatch {
            director_base_url: Some(settings.director_base_url.clone()),
            director_model: Some(settings.director_model.clone()),
            ..SettingsPatch::default()
        };
        assert!(
            !completer_retargets(&settings, &patch),
            "presence of the same URL and model must not reset session history"
        );
    }

    #[test]
    fn a_changed_base_url_or_model_retargets() {
        let settings = endpoint_settings();
        let url = SettingsPatch {
            director_base_url: Some("https://api.x.ai".into()),
            director_model: Some(settings.director_model.clone()),
            ..SettingsPatch::default()
        };
        assert!(completer_retargets(&settings, &url));
        let model = SettingsPatch {
            director_base_url: Some(settings.director_base_url.clone()),
            director_model: Some("grok-4.6".into()),
            ..SettingsPatch::default()
        };
        assert!(completer_retargets(&settings, &model));
    }

    #[test]
    fn a_key_patch_always_retargets() {
        let settings = endpoint_settings();
        let set = SettingsPatch {
            director_base_url: Some(settings.director_base_url.clone()),
            director_model: Some(settings.director_model.clone()),
            director_api_key: Some("sk-new".into()),
            ..SettingsPatch::default()
        };
        assert!(completer_retargets(&settings, &set));
        let clear = SettingsPatch {
            director_api_key: Some(String::new()),
            ..SettingsPatch::default()
        };
        assert!(completer_retargets(&settings, &clear));
    }

    #[test]
    fn retarget_payload_carries_resolved_settings() {
        let store = MemoryStore::new();
        store.set(DIRECTOR_API_KEY, "sk-stored-key").unwrap();
        let settings = endpoint_settings();
        match retarget_payload(&settings, &store).unwrap() {
            SettingsOp::Retarget {
                settings,
                enabled,
                configured,
                ambient_allowed,
            } => {
                assert_eq!(settings.api_key, "sk-stored-key");
                assert!(configured);
                assert!(enabled);
                assert!(ambient_allowed);
                let dump = format!("{settings:?}");
                assert!(
                    !dump.contains("sk-stored-key"),
                    "Retarget Debug must not echo the key: {dump}"
                );
            }
            other => panic!("expected Retarget, got {other:?}"),
        }
    }

    #[test]
    fn a_store_set_error_leaves_settings_unchanged() {
        let mut settings = Settings::default();
        let err = apply_with_store(
            &mut settings,
            &FailingStore,
            SettingsPatch {
                director_base_url: Some("https://api.x.ai".into()),
                director_api_key: Some("sk-new".into()),
                ..SettingsPatch::default()
            },
        );
        assert!(err.is_err());
        assert!(
            settings.director_base_url.is_empty(),
            "a failed key write must not leave a URL that was never saved"
        );
    }

    #[test]
    fn a_store_delete_error_leaves_settings_unchanged() {
        let mut settings = Settings::default();
        let err = apply_with_store(
            &mut settings,
            &FailingStore,
            SettingsPatch {
                director_base_url: Some("https://api.x.ai".into()),
                director_api_key: Some(String::new()),
                ..SettingsPatch::default()
            },
        );
        assert!(err.is_err());
        assert!(
            settings.director_base_url.is_empty(),
            "a failed key delete must not leave a URL that was never saved"
        );
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
