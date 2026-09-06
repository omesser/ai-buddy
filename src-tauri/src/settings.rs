//! The user's standing choices, as a file they own.
//!
//! Settings is how #18 reaches the Director, hide rules, Memory, and launch
//! without finding the sprite. The document is JSON so a hand-edit is a text
//! editor, the same deal Memory already makes. Missing keys take their
//! defaults, so an older file keeps working when a field is added.

pub mod form;

use std::collections::HashMap;
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
use crate::dev_flags;
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
    pub sound: bool,
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
    /// The Development rows, by form row id: the value in force, which is the
    /// exported variable's where it owns the row and the file's otherwise.
    ///
    /// Keyed rather than one named field per row. Six switches would be six
    /// fields here, six bindings in each window, and a seventh row would need
    /// all of that again before it did anything (#273).
    pub development_switches: HashMap<String, bool>,
    pub development_texts: HashMap<String, String>,
    /// Live OS grants, not a file field. The window rereads them on become-key.
    pub consent: Vec<ConsentRow>,
    /// The name Privacy & Security will show for this process.
    pub consent_listed_as: String,
}

/// The Development switches, by row id, as the window must draw them.
///
/// Through `dev_flags`, which is the one place that decides what an exported
/// variable does to a switch.
fn development_switches(settings: &Settings) -> HashMap<String, bool> {
    HashMap::from([
        (
            form::TRACE_FRAMES_ID.to_string(),
            dev_flags::TRACE_FRAMES.in_force(settings.trace_frames),
        ),
        (
            form::TRACE_HITTEST_ID.to_string(),
            dev_flags::TRACE_HITTEST.in_force(settings.trace_hittest),
        ),
        (
            form::TRACE_DIRECTOR_ID.to_string(),
            dev_flags::TRACE_DIRECTOR.in_force(settings.trace_director),
        ),
        (
            form::TRACE_ENGINE_ID.to_string(),
            dev_flags::TRACE_ENGINE.in_force(settings.trace_engine),
        ),
        #[cfg(target_os = "macos")]
        (
            form::CAPTURABLE_ID.to_string(),
            dev_flags::CAPTURABLE.in_force(settings.capturable),
        ),
    ])
}

/// Every numeric row a variable can own, by row id, with the same precedence.
///
/// Both windows fill such a row from this map by id, so the tab it is drawn on
/// does not matter: the wake interval is the Director tab's.
fn development_texts(settings: &Settings) -> HashMap<String, String> {
    HashMap::from([
        (
            form::DIRECTOR_TIMEOUT_SECS_ID.to_string(),
            limit_in_force::<u64>(model::TIMEOUT_SECS, &settings.director_timeout_secs),
        ),
        (
            form::DIRECTOR_MAX_TOKENS_ID.to_string(),
            limit_in_force::<u32>(model::MAX_TOKENS, &settings.director_max_tokens),
        ),
        (
            form::DIRECTOR_WAKE_SECS_ID.to_string(),
            limit_in_force::<u64>(model::WAKE_SECS, &settings.director_wake_secs),
        ),
    ])
}

/// One Completer limit as the window must show it, in the type the read site
/// parses it as.
///
/// A value `dev_flags::seed` cannot use — blank, non-numeric, out of range, or
/// zero, which it calls unset — shows blank, so the row's placeholder names
/// the default that is in force instead. Showing it verbatim would name a
/// timeout `model::timeout_for` never reaches, and a frozen row offers no way
/// to correct that.
///
/// Blank is also what a blur over the row commits, so clicking into an
/// unusable value and out again saves the emptiness. Nothing is lost:
/// everything it can discard is a value `seed` already treats as unset, and a
/// variable that owns the row freezes it before it can be clicked.
fn limit_in_force<T>(var: &str, file: &str) -> String
where
    T: std::str::FromStr + fmt::Display + Default + PartialEq,
{
    match model::env_or_file(var, file).trim().parse::<T>() {
        Ok(value) if value != T::default() => value.to_string(),
        _ => String::new(),
    }
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
            // The value in force, as the Development rows show theirs: an
            // exported switch reads as it exported, however the file has it.
            director_enabled: model::director_in_force(settings.director_enabled),
            ambient_wakes: settings.ambient_wakes,
            do_not_disturb: settings.do_not_disturb,
            sound: settings.sound,
            hidden: settings.hidden,
            hide_in_fullscreen: settings.hide_in_fullscreen,
            hide_hotkey: display_hotkey(&settings.hide_hotkey),
            excluded_applications: settings.excluded_applications.clone(),
            character: settings.character.clone(),
            memory_path: memory_path.display().to_string(),
            last_payload,
            installed,
            instances,
            // The resolved endpoint, not the file's: an exported variable
            // outranks the file in `model::resolve`, and a window that printed
            // the file value would name a host the Director never calls (#272).
            director_base_url: model::env_or_file(model::BASE_URL, &settings.director_base_url),
            director_model: model::env_or_file(model::MODEL, &settings.director_model),
            api_key_set,
            api_key_fingerprint,
            api_key_error,
            development_switches: development_switches(settings),
            development_texts: development_texts(settings),
            consent: consent::rows(|id| settings.wants_consent(id)),
            consent_listed_as: String::new(),
        }
    }

    /// The pane copy. The listed name is live: a `cargo run` from Cursor is
    /// Cursor, a packaged build is ai-buddy.
    #[cfg(target_os = "macos")]
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

    /// What the key field shows when it is empty. The placeholder, not the
    /// value: a fingerprint sitting in the field would be committed as a key
    /// on the next blur.
    pub fn api_key_placeholder(&self) -> String {
        if model::env_override(model::API_KEY).is_some() {
            // The variable's key is the one `resolve` hands the Completer, so
            // the stored fingerprint would name a key nothing uses (#272).
            "Overridden by env".to_string()
        } else if !self.api_key_error.is_empty() {
            format!("Unavailable — {}", self.api_key_error)
        } else if self.api_key_set {
            format!("Set — {}", self.api_key_fingerprint)
        } else {
            "Not set".into()
        }
    }

    /// Whether Clear key has anything to clear. A key the store would not
    /// hand over counts: an unreadable one can still be wiped.
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

/// Whether what a secure field holds is a key somebody typed.
///
/// Both windows leave that field blank on refresh, so an empty one is an
/// untouched one and a blur over it is not an edit. Empty reaching
/// `write_director_key` deletes the stored key, which is what Clear key is
/// for and never what tabbing past the field should mean.
fn key_was_typed(text: &str) -> bool {
    model::trim_key(text).is_some()
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
    let stored = if model::env_owns_key() {
        None
    } else {
        secrets.get(DIRECTOR_API_KEY)?
    };
    Ok(model::resolve(
        &settings.director_base_url,
        &settings.director_model,
        stored.as_deref(),
    ))
}

/// Fold a patch into `settings` and put the live development flags back in
/// step with it.
///
/// A development switch is live state as well as a file field, so a patch that
/// only reached the file is the relaunch #273 reports. Both `apply` paths go
/// through here, so the test seam cannot pass while the seeding is gone.
fn apply_and_seed(settings: &mut Settings, patch: SettingsPatch) {
    settings.apply(patch);
    dev_flags::seed(settings);
}

/// Write the key first so a store error cannot leave a URL in memory that
/// was never saved or sent as Retarget.
///
/// The test seam for `SettingsSession::apply`, without the lock, the file and
/// the ops channel.
#[cfg(test)]
fn apply_with_store(
    settings: &mut Settings,
    store: &dyn SecretStore,
    patch: SettingsPatch,
) -> Result<(), String> {
    write_director_key(store, &patch)?;
    apply_and_seed(settings, patch);
    Ok(())
}

/// `Some` on the key always retargets: we cannot compare a secret to the
/// file. URL and model retarget only when the value actually changed — both
/// windows commit on every blur, an untouched field included.
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
        // The timeout and the reply cap are baked into the Endpoint by
        // `model::endpoint_from`, so a change to either only reaches the
        // Director through a rebuild.
        || patch
            .director_timeout_secs
            .as_ref()
            .is_some_and(|secs| secs != &settings.director_timeout_secs)
        || patch
            .director_max_tokens
            .as_ref()
            .is_some_and(|cap| cap != &settings.director_max_tokens)
        // The wake interval reaches a running Director the same way, for its
        // own reason: the rebuild is where `model::config_from` reads it, and
        // the frame loop re-paces every Instance from what it rebuilt (#262).
        || patch
            .director_wake_secs
            .as_ref()
            .is_some_and(|secs| secs != &settings.director_wake_secs)
}

/// Which of the Director tab's fields hold an edit.
///
/// A redraw asks this so it can leave a staged field alone and still take
/// every other one from live state. Per field rather than per tab: a tab
/// dirty only because a key was typed would otherwise freeze the endpoint
/// text beside it, and Apply would write that stale text back (#279).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Staged {
    pub base_url: bool,
    pub model: bool,
    pub key: bool,
}

impl Staged {
    pub fn any(&self) -> bool {
        self.base_url || self.model || self.key
    }
}

/// What the Director tab holds right now, as the window reads it straight
/// back off its own widgets.
///
/// Verbatim text, one field per batched row. Nothing here is filtered by the
/// renderer: `patch` applies the frozen rule itself, from the description, so
/// the two windows cannot disagree about which rows an exported variable owns
/// (#272).
pub struct DirectorDraft<'a> {
    pub base_url: String,
    pub model: String,
    pub key: String,
    pub clear_key: bool,
    pub description: &'a form::FormDescription,
}

impl DirectorDraft<'_> {
    /// The new Base URL, or `None` when the row is frozen or unchanged.
    ///
    /// A frozen row never applies: `model::resolve` gives the exported
    /// variable the last word and would discard the edit (#272).
    fn base_url_edit(&self, view: &SettingsView) -> Option<&str> {
        self.edit(
            form::DIRECTOR_BASE_URL_ID,
            &self.base_url,
            &view.director_base_url,
        )
    }

    fn model_edit(&self, view: &SettingsView) -> Option<&str> {
        self.edit(form::DIRECTOR_MODEL_ID, &self.model, &view.director_model)
    }

    fn edit<'t>(&self, id: &str, text: &'t str, live: &str) -> Option<&'t str> {
        (!self.description.frozen(id) && text != live).then_some(text)
    }

    /// What the key field means: the typed key, or the empty string for a
    /// staged delete. `None` when neither.
    ///
    /// A typed key beats a staged clear, because Clear key blanks the field
    /// and text sitting in it afterwards is the later intent. A blank field is
    /// an untouched one — `key_was_typed` is the whole of that test, and it is
    /// what keeps tabbing past the field from meaning delete.
    fn key_edit(&self, view: &SettingsView) -> Option<&str> {
        if self.description.frozen(form::DIRECTOR_API_KEY_ID) {
            return None;
        }
        if key_was_typed(&self.key) {
            return Some(&self.key);
        }
        // Nothing stored is nothing to delete, so a staged clear of it is not
        // an edit and must not read as dirty.
        (self.clear_key && view.clear_key_enabled()).then_some("")
    }

    /// The patch one Apply sends: only the fields that changed, so at most one
    /// `SettingsOp::Retarget` comes out of it. `None` when the tab is clean,
    /// which is also what disables both buttons.
    ///
    /// Through `set_text` rather than by assignment, so the field names are
    /// the ones every other control writes through. The one exception is the
    /// staged delete: blank is the whole of what a delete is, and `set_text`
    /// exists to refuse exactly that value.
    pub fn patch(&self, view: &SettingsView) -> Option<SettingsPatch> {
        let staged = self.staged(view);
        if !staged.any() {
            return None;
        }
        let mut patch = SettingsPatch::default();
        if let Some(text) = self.base_url_edit(view) {
            patch.set_text(TextField::DirectorBaseUrl, text);
        }
        if let Some(text) = self.model_edit(view) {
            patch.set_text(TextField::DirectorModel, text);
        }
        if let Some(key) = self.key_edit(view) {
            patch.director_api_key = Some(key.to_string());
        }
        Some(patch)
    }

    /// The same three decisions as `patch`, as booleans, so a redraw and an
    /// Apply cannot disagree about what is staged.
    pub fn staged(&self, view: &SettingsView) -> Staged {
        Staged {
            base_url: self.base_url_edit(view).is_some(),
            model: self.model_edit(view).is_some(),
            key: self.key_edit(view).is_some(),
        }
    }
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
    let mut cfg = model::config_from(&director);
    cfg.apply_switch(settings.director_enabled);
    Ok(SettingsOp::Retarget {
        settings: director,
        enabled: cfg.enabled,
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
        // Seeded before `retarget_payload`, which rebuilds the Endpoint from
        // the live timeout and reply cap.
        apply_and_seed(&mut settings, patch);
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
        crate::platform::open_path(&self.memory_path)
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

/// What the settings window can change in one call.
#[derive(Clone, Default, Deserialize)]
pub struct SettingsPatch {
    pub director_enabled: Option<bool>,
    pub ambient_wakes: Option<bool>,
    pub do_not_disturb: Option<bool>,
    pub sound: Option<bool>,
    pub hidden: Option<bool>,
    pub hide_in_fullscreen: Option<bool>,
    pub hide_hotkey: Option<String>,
    pub launch_at_login: Option<bool>,
    pub excluded_applications: Option<Vec<String>>,
    pub character: Option<String>,
    pub director_base_url: Option<String>,
    pub director_model: Option<String>,
    pub director_timeout_secs: Option<String>,
    pub director_max_tokens: Option<String>,
    pub director_wake_secs: Option<String>,
    pub trace_frames: Option<bool>,
    pub trace_hittest: Option<bool>,
    pub trace_director: Option<bool>,
    pub trace_engine: Option<bool>,
    pub capturable: Option<bool>,
    /// Present so callers can write the store; `Settings::apply` ignores it
    /// because the key is not a file field.
    pub director_api_key: Option<String>,
    pub use_accessibility: Option<bool>,
    pub use_screen_recording: Option<bool>,
}

/// A boolean field of `SettingsPatch`, as the form row writing it names it.
///
/// A name rather than a `&str` so the row and the setter cannot disagree: with
/// a string key, a row could name a field no setter knew, and that compiled
/// clean and shipped a checkbox that wrote nothing (#273).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BoolField {
    DirectorEnabled,
    AmbientWakes,
    DoNotDisturb,
    Sound,
    Hidden,
    HideInFullscreen,
    LaunchAtLogin,
    TraceFrames,
    TraceHittest,
    TraceDirector,
    TraceEngine,
    /// Only AppKit has a capture exclusion to drop, so only AppKit offers the
    /// row. The patch field itself is not gated: the file carries it anywhere.
    #[cfg(target_os = "macos")]
    Capturable,
    UseAccessibility,
    UseScreenRecording,
}

/// A text field of `SettingsPatch`, as the form row writing it names it.
///
/// Typed for the reason `BoolField` is. No `hide_hotkey`: the row showing it is
/// an `InspectBlock` that writes nothing, because a text field is not a key
/// recorder.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextField {
    Character,
    DirectorBaseUrl,
    DirectorModel,
    DirectorTimeoutSecs,
    DirectorMaxTokens,
    DirectorWakeSecs,
    DirectorApiKey,
    ExcludedApplications,
}

impl SettingsPatch {
    /// Write the boolean field a checkbox declares.
    ///
    /// The one place that turns a row's field into a patch, and the one place
    /// the boolean field list is spelled: a `BoolField` this match does not
    /// cover is a compile error.
    pub fn set_bool(&mut self, field: BoolField, value: bool) {
        match field {
            BoolField::DirectorEnabled => self.director_enabled = Some(value),
            BoolField::AmbientWakes => self.ambient_wakes = Some(value),
            BoolField::DoNotDisturb => self.do_not_disturb = Some(value),
            BoolField::Sound => self.sound = Some(value),
            BoolField::Hidden => self.hidden = Some(value),
            BoolField::HideInFullscreen => self.hide_in_fullscreen = Some(value),
            BoolField::LaunchAtLogin => self.launch_at_login = Some(value),
            BoolField::TraceFrames => self.trace_frames = Some(value),
            BoolField::TraceHittest => self.trace_hittest = Some(value),
            BoolField::TraceDirector => self.trace_director = Some(value),
            BoolField::TraceEngine => self.trace_engine = Some(value),
            #[cfg(target_os = "macos")]
            BoolField::Capturable => self.capturable = Some(value),
            BoolField::UseAccessibility => self.use_accessibility = Some(value),
            BoolField::UseScreenRecording => self.use_screen_recording = Some(value),
        }
    }

    /// Write the text field a row declares, and say whether it took the value.
    ///
    /// False only for a blank API key: both windows leave that field blank on
    /// refresh, so a blur over an untouched one is not an edit, and blank
    /// reaching the store is what Clear key means. `key_was_typed` is the whole
    /// of that test.
    pub fn set_text(&mut self, field: TextField, value: &str) -> bool {
        match field {
            TextField::Character => self.character = Some(value.to_string()),
            TextField::DirectorBaseUrl => self.director_base_url = Some(value.to_string()),
            TextField::DirectorModel => self.director_model = Some(value.to_string()),
            TextField::DirectorTimeoutSecs => self.director_timeout_secs = Some(value.to_string()),
            TextField::DirectorMaxTokens => self.director_max_tokens = Some(value.to_string()),
            TextField::DirectorWakeSecs => self.director_wake_secs = Some(value.to_string()),
            TextField::DirectorApiKey if key_was_typed(value) => {
                self.director_api_key = Some(value.to_string())
            }
            TextField::DirectorApiKey => return false,
            // One name per line, the shape both windows' multiline field holds.
            TextField::ExcludedApplications => {
                self.excluded_applications =
                    Some(value.lines().map(|line| line.trim().to_string()).collect())
            }
        }
        true
    }
}

impl fmt::Debug for SettingsPatch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SettingsPatch")
            .field("director_enabled", &self.director_enabled)
            .field("ambient_wakes", &self.ambient_wakes)
            .field("do_not_disturb", &self.do_not_disturb)
            .field("sound", &self.sound)
            .field("hidden", &self.hidden)
            .field("hide_in_fullscreen", &self.hide_in_fullscreen)
            .field("hide_hotkey", &self.hide_hotkey)
            .field("launch_at_login", &self.launch_at_login)
            .field("excluded_applications", &self.excluded_applications)
            .field("character", &self.character)
            .field("director_base_url", &self.director_base_url)
            .field("director_model", &self.director_model)
            .field("director_timeout_secs", &self.director_timeout_secs)
            .field("director_max_tokens", &self.director_max_tokens)
            .field("director_wake_secs", &self.director_wake_secs)
            .field("trace_frames", &self.trace_frames)
            .field("trace_hittest", &self.trace_hittest)
            .field("trace_director", &self.trace_director)
            .field("trace_engine", &self.trace_engine)
            .field("capturable", &self.capturable)
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
    /// Whether a frame may make a sound. Do Not Disturb is quiet but not
    /// gone (#84), so it takes the audio cue and leaves the visual one; the
    /// webview is told the answer and never works it out itself (#277).
    pub fn sound_allowed(&self) -> bool {
        self.sound && !self.do_not_disturb
    }

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
        if let Some(value) = patch.sound {
            self.sound = value;
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
        if let Some(value) = patch.director_timeout_secs {
            self.director_timeout_secs = value;
        }
        if let Some(value) = patch.director_max_tokens {
            self.director_max_tokens = value;
        }
        if let Some(value) = patch.director_wake_secs {
            self.director_wake_secs = value;
        }
        if let Some(value) = patch.trace_frames {
            self.trace_frames = value;
        }
        if let Some(value) = patch.trace_hittest {
            self.trace_hittest = value;
        }
        if let Some(value) = patch.trace_director {
            self.trace_director = value;
        }
        if let Some(value) = patch.trace_engine {
            self.trace_engine = value;
        }
        if let Some(value) = patch.capturable {
            self.capturable = value;
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
/// on the machine and B alone belongs to most of them. One canonical spelling
/// that `parse_hotkey` reads on every OS; what a user reads is
/// `display_hotkey`, in the words that OS gives the keys (#194).
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
    /// The cues a gesture plays are heard, not only seen (#277).
    pub sound: bool,
    /// Off screen, same flag the hotkey flips.
    pub hidden: bool,
    /// Fade away when a fullscreen application is frontmost.
    pub hide_in_fullscreen: bool,
    /// One canonical spec, in any platform's words. Read with `parse_hotkey`
    /// and shown to a user with `display_hotkey`, never raw (#194).
    ///
    /// A string rather than the `Hotkey` it parses into, which would otherwise
    /// be the honest type: `load` turns any parse failure into whole-file
    /// defaults, so a struct-shaped field meeting a string in an installed
    /// `settings.json` would silently reset every other setting with it.
    /// Persist the struct only behind a deserializer that accepts both shapes.
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
    /// Completer timeout, in seconds. Empty means unset, as on the two above.
    pub director_timeout_secs: String,
    /// Reply cap, in tokens. Empty means unset, as on the two above.
    pub director_max_tokens: String,
    /// First ambient wait, in seconds. Empty means unset, and leaves
    /// `Pace::FIRST`. The Character's `model_base` and `model_power` grow the
    /// wait from here; this is only where it starts (#262).
    pub director_wake_secs: String,
    /// Development switches. Off is the shipped answer for all of them; see
    /// `dev_flags`, which holds the live value each read site loads.
    pub trace_frames: bool,
    pub trace_hittest: bool,
    pub trace_director: bool,
    pub trace_engine: bool,
    /// Drop the overlay's capture exclusion. macOS reads it; the field is
    /// unconditional so the document round-trips on every platform.
    pub capturable: bool,
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
            sound: true,
            hidden: false,
            hide_in_fullscreen: true,
            hide_hotkey: DEFAULT_HIDE_HOTKEY.to_string(),
            launch_at_login: false,
            excluded_applications: Vec::new(),
            character: String::new(),
            instances: Vec::new(),
            director_base_url: String::new(),
            director_model: String::new(),
            director_timeout_secs: String::new(),
            director_max_tokens: String::new(),
            director_wake_secs: String::new(),
            trace_frames: false,
            trace_hittest: false,
            trace_director: false,
            trace_engine: false,
            capturable: false,
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
///
/// Each alias set is one key under every OS's name for it, `Win` included, so
/// that everything `Hotkey::display` prints is something this reads back.
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
            "Command" | "Super" | "Meta" | "Win" => hotkey.command = true,
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

/// How the keyboard in front of a user names the modifier keys.
///
/// The chord is the same three modifiers on every OS — the plugin registers
/// one binding — so this is a spelling, not a second hotkey. Taking the words
/// as an argument is what lets a Mac test assert what Linux would read (#194).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModifierWords {
    Mac,
    Linux,
    Windows,
}

impl ModifierWords {
    /// The words this build's OS uses. X11 and Wayland both say Super.
    pub fn current() -> Self {
        if cfg!(target_os = "macos") {
            Self::Mac
        } else if cfg!(target_os = "windows") {
            Self::Windows
        } else {
            Self::Linux
        }
    }

    /// Control, Option and Command under these words. Shift is Shift
    /// everywhere, so it is not in the table.
    fn names(self) -> (&'static str, &'static str, &'static str) {
        match self {
            Self::Mac => ("Control", "Option", "Command"),
            Self::Linux => ("Ctrl", "Alt", "Super"),
            Self::Windows => ("Ctrl", "Alt", "Win"),
        }
    }
}

impl Hotkey {
    /// Print the chord in `words`, e.g. `Control-Option-Command-B` on a Mac
    /// and `Ctrl-Alt-Super-B` on Linux.
    ///
    /// Every spelling it prints is one `parse_hotkey` reads back, because the
    /// settings hotkey field shows this string and takes it again on rebind.
    pub fn display(&self, words: ModifierWords) -> String {
        let (control, option, command) = words.names();
        let mut parts = Vec::with_capacity(5);
        if self.control {
            parts.push(control);
        }
        if self.option {
            parts.push(option);
        }
        if self.shift {
            parts.push("Shift");
        }
        if self.command {
            parts.push(command);
        }
        let letter = self.key.to_ascii_uppercase().to_string();
        parts.push(&letter);
        parts.join("-")
    }
}

/// The hotkey `spec` names, in the words of the OS this build runs on.
///
/// A hand-edited or older file may name the keys in any platform's words, so
/// the stored string is parsed rather than printed. An unreadable one falls
/// back to the default, the same binding the shell registers for it.
pub fn display_hotkey(spec: &str) -> String {
    parse_hotkey(spec)
        .or_else(|| parse_hotkey(DEFAULT_HIDE_HOTKEY))
        .map(|hotkey| hotkey.display(ModifierWords::current()))
        .unwrap_or_default()
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
        assert!(Settings::default().sound);
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
            sound: false,
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
            director_timeout_secs: "45".into(),
            director_max_tokens: "300".into(),
            director_wake_secs: "300".into(),
            trace_frames: true,
            trace_hittest: true,
            trace_director: true,
            trace_engine: true,
            capturable: true,
            use_accessibility: true,
            use_screen_recording: false,
        };
        settings.save(&path).expect("save");

        assert_eq!(Settings::load(&path), settings);
        let _ = fs::remove_file(&path);
    }

    /// Do Not Disturb is quiet but not gone (#84): it takes the sound and
    /// leaves the visual cue, and it never turns the sound back on (#277).
    #[test]
    fn sound_is_allowed_only_when_on_and_not_disturbing() {
        let mut settings = Settings::default();
        assert!(settings.sound_allowed());
        settings.do_not_disturb = true;
        assert!(!settings.sound_allowed());
        settings.sound = false;
        assert!(!settings.sound_allowed());
        settings.do_not_disturb = false;
        assert!(!settings.sound_allowed());
    }

    /// The window toggles one field at a time, and the mute has to land
    /// without a restart, so the patch is the whole path (#277).
    #[test]
    fn a_patch_can_mute_and_unmute() {
        let mut settings = Settings::default();
        settings.apply(SettingsPatch {
            sound: Some(false),
            ..SettingsPatch::default()
        });
        assert!(!settings.sound);
        settings.apply(SettingsPatch {
            sound: Some(true),
            ..SettingsPatch::default()
        });
        assert!(settings.sound);
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
        // The endpoint the view prints is the resolved one, so a var exported
        // in the developer's shell would otherwise decide these two.
        model::tests::with_env(None, None, None, || {
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
        });
    }

    #[test]
    fn the_key_placeholder_is_not_set_when_unset() {
        model::tests::with_env(None, None, None, || {
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
        });
    }

    #[test]
    fn the_key_placeholder_is_unavailable_when_the_store_cannot_be_read() {
        model::tests::with_env(None, None, None, || {
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
        });
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
        assert!(
            settings.sound,
            "a file from before the setting stays audible"
        );
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
            sound: false,
            hidden: true,
            hide_in_fullscreen: false,
            hide_hotkey: "Control-Shift-H".to_string(),
            launch_at_login: true,
            excluded_applications: vec!["1Password".to_string(), "Keychain Access".to_string()],
            character: "nim".to_string(),
            instances: Vec::new(),
            director_base_url: String::new(),
            director_model: String::new(),
            director_timeout_secs: String::new(),
            director_max_tokens: String::new(),
            director_wake_secs: String::new(),
            trace_frames: false,
            trace_hittest: false,
            trace_director: false,
            trace_engine: false,
            capturable: false,
            use_accessibility: true,
            use_screen_recording: false,
        };
        #[cfg_attr(not(target_os = "macos"), allow(unused_mut))]
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
        assert!(!view.sound);
        assert!(view.hidden);
        assert!(!view.hide_in_fullscreen);
        assert_eq!(view.hide_hotkey, display_hotkey("Control-Shift-H"));
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
        #[cfg(target_os = "macos")]
        {
            view.consent_listed_as = "Cursor".into();
            assert!(
                view.consent_intro().contains("Cursor"),
                "the pane has to name the TCC row, got {:?}",
                view.consent_intro()
            );
        }
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
    fn write_director_key_fails_loudly_when_store_set_fails() {
        let patch = SettingsPatch {
            director_api_key: Some("sk-new-key".into()),
            ..SettingsPatch::default()
        };
        let result = write_director_key(&FailingStore, &patch);
        assert!(
            result.is_err(),
            "a store set error must fail loudly, not succeed silently"
        );
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
        // A key exported in the developer's shell would resolve this remote
        // as configured and take the precondition with it.
        model::tests::with_env(None, None, None, || {
            assert!(
                director_settings(&settings, &FailingStore).is_err(),
                "a store error must not resolve as no key"
            );
            let unset = model::resolve(&settings.director_base_url, &settings.director_model, None);
            assert!(
                !model::config_from(&unset).configured,
                "precondition: unset remote is Static"
            );
        });
    }

    /// A store read is a Keychain prompt on macOS, and one whose answer
    /// `resolve` throws away is a prompt for nothing. `FailingStore` is the
    /// assertion: this can only resolve if nothing consulted the store.
    #[test]
    fn an_exported_key_leaves_the_store_unread() {
        let settings = endpoint_settings();
        model::tests::with_env(Some("sk-env-key"), None, None, || {
            let resolved = director_settings(&settings, &FailingStore)
                .expect("the env owns the key, so the store has nothing to say");
            assert_eq!(resolved.api_key, "sk-env-key");
        });
    }

    /// A blank export is a mistake — `$XAI_API_KEY` that expanded to nothing —
    /// and the warning that names it is what the launch owes the user. A
    /// stored key must not answer in its place, quietly or at the price of a
    /// prompt.
    #[test]
    fn a_blank_exported_key_leaves_the_store_unread_and_still_warns() {
        let settings = endpoint_settings();
        model::tests::with_env(Some("  "), None, None, || {
            let resolved = director_settings(&settings, &FailingStore)
                .expect("a blank export is still the env answering");
            assert!(resolved.api_key.is_empty());
            assert!(
                resolved.key_invalid,
                "the blank export must still reach the startup warning"
            );
        });
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

    /// The Director tab's live state, with or without a stored key.
    fn director_view(api_key_set: bool) -> SettingsView {
        let fingerprint = if api_key_set {
            "len=12 last=key1".to_string()
        } else {
            String::new()
        };
        SettingsView::from_parts(
            &endpoint_settings(),
            Path::new("/tmp/ai-buddy/memory.md"),
            None,
            Vec::new(),
            Vec::new(),
            (api_key_set, fingerprint, String::new()),
        )
    }

    /// One patch for the whole tab, so the three edits #279 opens with cost
    /// one rebuild instead of three.
    #[test]
    fn one_apply_retargets_once_for_a_new_url_and_model() {
        model::tests::with_env(None, None, None, || {
            let view = director_view(false);
            let description = form::describe();
            let patch = DirectorDraft {
                base_url: "https://api.x.ai".into(),
                model: "grok-4.6".into(),
                key: String::new(),
                clear_key: false,
                description: &description,
            }
            .patch(&view)
            .expect("a new URL and model is dirty");
            assert_eq!(patch.director_base_url.as_deref(), Some("https://api.x.ai"));
            assert_eq!(patch.director_model.as_deref(), Some("grok-4.6"));
            assert!(
                patch.director_api_key.is_none(),
                "an untouched key field is not part of the batch"
            );
            assert!(
                completer_retargets(&endpoint_settings(), &patch),
                "the one patch has to carry the one Retarget"
            );
        });
    }

    /// #272: the window shows what the variable imposes, so the text in a
    /// frozen field differs from the file and would otherwise read as dirty.
    ///
    /// The variable is exported for real here, so the frozen rule is tested
    /// through the description both windows build from rather than through a
    /// `None` a renderer had to remember to pass.
    #[test]
    fn a_frozen_row_never_applies_even_when_its_text_differs() {
        model::tests::with_env(None, Some("https://env.example"), None, || {
            let view = director_view(false);
            let description = form::describe();
            assert!(
                description.frozen(form::DIRECTOR_BASE_URL_ID),
                "precondition: the variable owns the URL row"
            );
            let draft = DirectorDraft {
                base_url: "https://typed.example".into(),
                model: "grok-4.6".into(),
                key: String::new(),
                clear_key: false,
                description: &description,
            };
            assert!(
                !draft.staged(&view).base_url,
                "a frozen row is never staged, so a redraw always redraws it"
            );
            let patch = draft
                .patch(&view)
                .expect("the model row is still the user's");
            assert!(patch.director_base_url.is_none());
            assert_eq!(patch.director_model.as_deref(), Some("grok-4.6"));
        });
    }

    /// Cancel builds no patch at all — it is `refresh()`. What this pins is
    /// the state it leaves behind: a blank field is untouched, not a delete.
    #[test]
    fn a_cancelled_key_never_reaches_a_patch() {
        model::tests::with_env(None, None, None, || {
            let view = director_view(true);
            let description = form::describe();
            let typed = DirectorDraft {
                base_url: view.director_base_url.clone(),
                model: view.director_model.clone(),
                key: "sk-typed-then-cancelled".into(),
                clear_key: false,
                description: &description,
            };
            assert!(
                typed.patch(&view).is_some(),
                "precondition: a typed key is dirty"
            );
            // What Cancel leaves behind: the blank field a redraw writes.
            let after_cancel = DirectorDraft {
                key: String::new(),
                ..typed
            };
            assert!(
                after_cancel.patch(&view).is_none(),
                "a key never typed is not a delete"
            );
        });
    }

    #[test]
    fn a_staged_clear_deletes_the_stored_key() {
        model::tests::with_env(None, None, None, || {
            let view = director_view(true);
            let description = form::describe();
            let patch = DirectorDraft {
                base_url: view.director_base_url.clone(),
                model: view.director_model.clone(),
                key: String::new(),
                clear_key: true,
                description: &description,
            }
            .patch(&view)
            .expect("a staged clear is dirty");
            assert_eq!(patch.director_api_key.as_deref(), Some(""));
        });
    }

    /// Nothing to clear is nothing to apply: the buttons would otherwise
    /// offer a delete of a key that is not there.
    #[test]
    fn a_staged_clear_on_an_unset_key_is_not_a_change() {
        model::tests::with_env(None, None, None, || {
            let view = director_view(false);
            assert!(!view.clear_key_enabled(), "precondition: no key is stored");
            let description = form::describe();
            let draft = DirectorDraft {
                base_url: view.director_base_url.clone(),
                model: view.director_model.clone(),
                key: String::new(),
                clear_key: true,
                description: &description,
            };
            assert!(draft.patch(&view).is_none());
        });
    }

    /// Clear key blanks the field, so text in it afterwards is the later
    /// intent.
    #[test]
    fn a_typed_key_beats_a_staged_clear() {
        model::tests::with_env(None, None, None, || {
            let view = director_view(true);
            let description = form::describe();
            let patch = DirectorDraft {
                base_url: view.director_base_url.clone(),
                model: view.director_model.clone(),
                key: "sk-typed-after-clear".into(),
                clear_key: true,
                description: &description,
            }
            .patch(&view)
            .expect("a typed key is dirty");
            assert_eq!(
                patch.director_api_key.as_deref(),
                Some("sk-typed-after-clear")
            );
        });
    }

    /// A redraw asks per field, not per tab. Freezing the whole tab on a
    /// typed key would hold stale endpoint text on screen, and Apply would
    /// write it back (#279).
    #[test]
    fn a_typed_key_stages_the_key_alone() {
        model::tests::with_env(None, None, None, || {
            let view = director_view(true);
            let description = form::describe();
            let staged = DirectorDraft {
                base_url: view.director_base_url.clone(),
                model: view.director_model.clone(),
                key: "sk-typed".into(),
                clear_key: false,
                description: &description,
            }
            .staged(&view);
            assert_eq!(
                staged,
                Staged {
                    base_url: false,
                    model: false,
                    key: true,
                }
            );
        });
    }

    #[test]
    fn a_clean_tab_has_nothing_to_apply() {
        model::tests::with_env(None, None, None, || {
            let view = director_view(true);
            let description = form::describe();
            let draft = DirectorDraft {
                base_url: view.director_base_url.clone(),
                model: view.director_model.clone(),
                key: String::new(),
                clear_key: false,
                description: &description,
            };
            assert!(
                draft.patch(&view).is_none(),
                "a clean tab is what disables both buttons"
            );
            assert!(
                !draft.staged(&view).any(),
                "and what lets a redraw take every field from live state"
            );
        });
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
        // The payload is resolved, so an exported key would win over the
        // store's and this would be asserting the shell's environment.
        model::tests::with_env(None, None, None, || {
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
        });
    }

    /// Saving is when the file's switch reaches the frame loop, so it is
    /// where a vetoed Director would come back on.
    #[test]
    fn retarget_cannot_switch_on_a_director_the_process_vetoed() {
        model::tests::with_env_switch("off", || {
            let store = MemoryStore::new();
            store.set(DIRECTOR_API_KEY, "sk-stored-key").unwrap();
            let settings = endpoint_settings();
            assert!(settings.director_enabled, "precondition: the file says on");
            match retarget_payload(&settings, &store).unwrap() {
                SettingsOp::Retarget {
                    enabled,
                    configured,
                    ..
                } => {
                    assert!(configured, "the stored key still configures it");
                    assert!(!enabled, "AI_BUDDY_DIRECTOR=off outranks the file");
                }
                other => panic!("expected Retarget, got {other:?}"),
            }
        });
    }

    /// The row is frozen, so the box it draws has to be the value in force —
    /// in both directions, since either can disagree with the file.
    #[test]
    fn an_env_owned_director_reads_as_exported_in_the_window() {
        for (exported, saved) in [("off", true), ("on", false)] {
            model::tests::with_env_switch(exported, || {
                let settings = Settings {
                    director_enabled: saved,
                    ..Settings::default()
                };
                let view = SettingsView::from_parts(
                    &settings,
                    Path::new("/tmp/ai-buddy/memory.md"),
                    None,
                    Vec::new(),
                    Vec::new(),
                    (false, String::new(), String::new()),
                );

                assert_eq!(
                    view.director_enabled,
                    exported == "on",
                    "the file said {saved}, the process said {exported}"
                );
            });
        }
    }

    fn endpoint_view(settings: &Settings) -> SettingsView {
        SettingsView::from_parts(
            settings,
            Path::new("/tmp/ai-buddy/memory.md"),
            None,
            Vec::new(),
            Vec::new(),
            (false, String::new(), String::new()),
        )
    }

    /// #272: the window has to print the endpoint the Director will use. The
    /// file value it used to print is the one `model::resolve` throws away.
    #[test]
    fn the_view_shows_the_endpoint_the_env_imposes() {
        let settings = endpoint_settings();
        model::tests::with_env(
            Some("sk-env-key"),
            Some("https://api.x.ai"),
            Some("grok-4.6"),
            || {
                let view = endpoint_view(&settings);
                assert_eq!(view.director_base_url, "https://api.x.ai");
                assert_eq!(view.director_model, "grok-4.6");
                assert_eq!(view.api_key_placeholder(), "Overridden by env");
            },
        );
        model::tests::with_env(None, None, None, || {
            let view = endpoint_view(&settings);
            assert_eq!(view.director_base_url, settings.director_base_url);
            assert_eq!(view.director_model, settings.director_model);
            assert_eq!(view.api_key_placeholder(), "Not set");
        });
    }

    /// Every Development control the window draws has to find its value in
    /// the view. A row whose id is missing renders off while the switch is on
    /// (#273), and the first click on a persisted `true` then sends
    /// `Some(true)` — a no-op the user reads as a dead checkbox.
    #[test]
    fn every_development_row_has_a_value_in_the_view() {
        model::tests::with_env(None, None, None, || {
            let view = endpoint_view(&Settings::default());
            let description = form::describe();
            let development = description
                .tabs
                .iter()
                .find(|tab| tab.title == "Development")
                .expect("the Development tab exists");

            for row in development
                .sections
                .iter()
                .flat_map(|section| &section.rows)
            {
                match row {
                    form::FormRow::Checkbox { id, .. } => {
                        assert!(
                            view.development_switches.contains_key(id),
                            "{id} has no value to draw"
                        )
                    }
                    form::FormRow::TextField { id, .. } => {
                        assert!(
                            view.development_texts.contains_key(id),
                            "{id} has no value to draw"
                        )
                    }
                    _ => {}
                }
            }
        });
    }

    /// The value in force, not the file's: an exported variable freezes the
    /// row, so printing the file's would draw a switch off while the trace it
    /// names is running (#273).
    #[test]
    fn the_view_shows_the_switch_the_env_imposes() {
        model::tests::with_env(None, None, None, || {
            let off = Settings {
                trace_frames: false,
                director_timeout_secs: "45".into(),
                ..Settings::default()
            };
            std::env::set_var(dev_flags::TRACE_FRAMES.var(), "1");
            std::env::set_var(model::TIMEOUT_SECS, "7");
            let view = endpoint_view(&off);
            assert!(
                view.development_switches[form::TRACE_FRAMES_ID],
                "the exported variable wins"
            );
            assert_eq!(view.development_texts[form::DIRECTOR_TIMEOUT_SECS_ID], "7");

            std::env::remove_var(dev_flags::TRACE_FRAMES.var());
            std::env::remove_var(model::TIMEOUT_SECS);
            let view = endpoint_view(&off);
            assert!(
                !view.development_switches[form::TRACE_FRAMES_ID],
                "the file wins with nothing exported"
            );
            assert_eq!(view.development_texts[form::DIRECTOR_TIMEOUT_SECS_ID], "45");
        });
    }

    /// A limit no read site can use has to read as unset, not as a number.
    ///
    /// `dev_flags::seed` parses the value and calls zero unset, so `=abc` and
    /// `=0` both leave `model::timeout_for` on its default. The row showed the
    /// export verbatim and named a timeout nothing waited that long for.
    #[test]
    fn an_unusable_limit_shows_as_blank_so_the_placeholder_names_the_default() {
        model::tests::with_env(None, None, None, || {
            let file = Settings {
                director_timeout_secs: "45".into(),
                ..Settings::default()
            };
            // Empty is no override at all, so the file's 45 stands there.
            for exported in ["abc", "0", "-1", ""] {
                std::env::set_var(model::TIMEOUT_SECS, exported);
                dev_flags::seed(&file);
                let expected = match dev_flags::director_timeout_secs() {
                    Some(secs) => secs.to_string(),
                    None => String::new(),
                };
                let view = endpoint_view(&file);
                assert_eq!(
                    view.development_texts[form::DIRECTOR_TIMEOUT_SECS_ID],
                    expected,
                    "exported {exported:?}"
                );
            }
            std::env::remove_var(model::TIMEOUT_SECS);
        });
    }

    /// The hop the settings window makes, end to end, for each endpoint field:
    /// the patch a committed field carries, the retarget decision, the payload
    /// resolved off the frame thread, and the rebuild the frame loop does with
    /// it (#272). The three fields took a route of their own, and only the
    /// decision at the end of it was covered.
    #[test]
    fn a_committed_endpoint_field_rebuilds_the_completer_for_the_new_host() {
        let edits = [
            (
                "base URL",
                SettingsPatch {
                    director_base_url: Some("https://api.x.ai".into()),
                    ..SettingsPatch::default()
                },
                "https://api.x.ai/",
                "grok-4.6",
            ),
            (
                "model",
                SettingsPatch {
                    director_model: Some("gpt-5".into()),
                    ..SettingsPatch::default()
                },
                "https://api.openai.com/",
                "gpt-5",
            ),
            (
                "API key",
                SettingsPatch {
                    director_api_key: Some("sk-typed-in-the-window".into()),
                    ..SettingsPatch::default()
                },
                "https://api.openai.com/",
                "grok-4.6",
            ),
        ];

        model::tests::with_env(None, None, None, || {
            for (field, patch, host, model_name) in edits {
                let store = MemoryStore::new();
                store.set(DIRECTOR_API_KEY, "sk-stored-key").unwrap();
                let mut settings = Settings {
                    director_model: "grok-4.6".into(),
                    ..endpoint_settings()
                };
                assert!(
                    completer_retargets(&settings, &patch),
                    "{field} has to reach the running Director"
                );
                apply_with_store(&mut settings, &store, patch).unwrap();

                let SettingsOp::Retarget {
                    settings: director,
                    configured,
                    ..
                } = retarget_payload(&settings, &store).unwrap()
                else {
                    panic!("an edited endpoint must send Retarget");
                };

                // What frame_loop.rs does with the payload.
                let id = "buddy".to_string();
                let mut slots = model::tests::slots_awaiting_a_wake(&id);
                let mut completer = None;
                model::retarget_model(
                    &mut slots,
                    &id,
                    &mut completer,
                    ["stroll"],
                    &director,
                    configured,
                );
                assert!(completer.is_some(), "{field} needs a Completer");
                // ADR-0008: a Wake already on the wire cannot propose against
                // the target that was just replaced.
                assert!(!slots.waiting(&id), "{field} must drop the open session");

                let endpoint =
                    model::endpoint_from(&director).expect("configured means a Completer");
                assert!(
                    endpoint.url().starts_with(host),
                    "{field}: the next wake has to reach {host}, not {}",
                    endpoint.url()
                );
                assert_eq!(endpoint.model(), model_name, "{field}: model");
                assert_eq!(
                    director.api_key,
                    if field == "API key" {
                        "sk-typed-in-the-window"
                    } else {
                        "sk-stored-key"
                    },
                    "{field}: key"
                );
            }
        });
    }

    /// #275 built the rebuild path. The two limits ride it because
    /// `model::endpoint_from` bakes them into the Endpoint at construction, so
    /// nothing else would carry a change to the Director already running.
    #[test]
    fn an_edited_limit_reaches_the_running_director() {
        let settings = Settings {
            director_timeout_secs: "20".into(),
            director_max_tokens: "80".into(),
            ..endpoint_settings()
        };

        for patch in [
            SettingsPatch {
                director_timeout_secs: Some("45".into()),
                ..SettingsPatch::default()
            },
            SettingsPatch {
                director_max_tokens: Some("300".into()),
                ..SettingsPatch::default()
            },
        ] {
            assert!(completer_retargets(&settings, &patch));
        }

        // Both windows commit every field on blur, so an unchanged value
        // arrives as `Some` and must not drop the open session.
        assert!(!completer_retargets(
            &settings,
            &SettingsPatch {
                director_timeout_secs: Some("20".into()),
                director_max_tokens: Some("80".into()),
                ..SettingsPatch::default()
            }
        ));
    }

    /// The wake interval rides the rebuild for a reason of its own: it is
    /// `model::config_from` that reads it, and the frame loop re-paces every
    /// Instance from the config it rebuilds there (#262).
    #[test]
    fn an_edited_wake_interval_reaches_the_running_director() {
        let settings = Settings {
            director_wake_secs: "120".into(),
            ..endpoint_settings()
        };

        assert!(completer_retargets(
            &settings,
            &SettingsPatch {
                director_wake_secs: Some("300".into()),
                ..SettingsPatch::default()
            }
        ));
        assert!(!completer_retargets(
            &settings,
            &SettingsPatch {
                director_wake_secs: Some("120".into()),
                ..SettingsPatch::default()
            }
        ));
    }

    /// The row is the Director tab's, and the window fills it from the same
    /// keyed map the Development rows use, so a missing key draws blank over
    /// a value that is in force.
    #[test]
    fn the_view_shows_the_wake_interval_the_env_imposes() {
        model::tests::with_env(None, None, None, || {
            let file = Settings {
                director_wake_secs: "300".into(),
                ..Settings::default()
            };
            let view = endpoint_view(&file);
            assert_eq!(view.development_texts[form::DIRECTOR_WAKE_SECS_ID], "300");

            std::env::set_var(model::WAKE_SECS, "30");
            let view = endpoint_view(&file);
            assert_eq!(view.development_texts[form::DIRECTOR_WAKE_SECS_ID], "30");
            std::env::remove_var(model::WAKE_SECS);
        });
    }

    /// The switch is only worth a checkbox if the read sites see it move
    /// without a relaunch, which means the patch has to reach `dev_flags`.
    #[test]
    fn a_patched_switch_moves_the_live_flag() {
        model::tests::with_env(None, None, None, || {
            let mut settings = Settings::default();
            let store = MemoryStore::new();

            apply_with_store(
                &mut settings,
                &store,
                SettingsPatch {
                    trace_director: Some(true),
                    ..SettingsPatch::default()
                },
            )
            .unwrap();
            assert!(settings.trace_director, "the file holds it");
            assert!(model::tracing(), "and the read site loads it");

            apply_with_store(
                &mut settings,
                &store,
                SettingsPatch {
                    trace_director: Some(false),
                    ..SettingsPatch::default()
                },
            )
            .unwrap();
            assert!(!model::tracing());

            // Each switch is its own static, so one of them moving proves
            // nothing about the next one being wired to anything (#273).
            apply_with_store(
                &mut settings,
                &store,
                SettingsPatch {
                    trace_engine: Some(true),
                    ..SettingsPatch::default()
                },
            )
            .unwrap();
            assert!(settings.trace_engine, "the file holds it");
            assert!(
                dev_flags::TRACE_ENGINE.is_on(),
                "and the frame loop loads it"
            );

            apply_with_store(
                &mut settings,
                &store,
                SettingsPatch {
                    trace_engine: Some(false),
                    ..SettingsPatch::default()
                },
            )
            .unwrap();
            assert!(!dev_flags::TRACE_ENGINE.is_on());
        });
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

    /// One chord in four spellings, because the file keeps whichever of them
    /// the user last named.
    #[test]
    fn one_chord_parses_the_same_from_every_platforms_words() {
        let mac = parse_hotkey("Control-Option-Command-B").expect("mac words");
        assert_eq!(parse_hotkey("Ctrl-Alt-Super-B"), Some(mac.clone()));
        assert_eq!(parse_hotkey("Ctrl-Alt-Win-B"), Some(mac.clone()));
        assert_eq!(parse_hotkey("Ctrl-Alt-Meta-B"), Some(mac));
    }

    /// #194: the menu used to print the stored Mac spelling everywhere, which
    /// names keys a Linux or Windows keyboard does not have.
    #[test]
    fn the_shipped_default_reads_as_each_platforms_own_chord() {
        let default = parse_hotkey(DEFAULT_HIDE_HOTKEY).expect("default parses");
        for (words, expected) in [
            (ModifierWords::Mac, "Control-Option-Command-B"),
            (ModifierWords::Linux, "Ctrl-Alt-Super-B"),
            (ModifierWords::Windows, "Ctrl-Alt-Win-B"),
        ] {
            assert_eq!(default.display(words), expected);
        }
        for words in [ModifierWords::Linux, ModifierWords::Windows] {
            let printed = default.display(words);
            assert!(!printed.contains("Option"), "{printed} names a Mac key");
            assert!(!printed.contains("Command"), "{printed} names a Mac key");
        }
    }

    /// What `display` prints is also what the hotkey field accepts back, or a
    /// user who retypes what the settings window shows them loses the binding.
    #[test]
    fn every_platforms_words_parse_back_to_the_chord_they_printed() {
        let chord = Hotkey {
            control: true,
            option: true,
            shift: true,
            command: true,
            key: 'B',
        };
        for words in [
            ModifierWords::Mac,
            ModifierWords::Linux,
            ModifierWords::Windows,
        ] {
            let printed = chord.display(words);
            assert_eq!(
                parse_hotkey(&printed),
                Some(chord.clone()),
                "{printed} did not parse back"
            );
        }
    }

    /// The window renders the view verbatim, so the view is where the stored
    /// spec becomes this machine's words — whichever words the file used.
    #[test]
    fn the_view_shows_the_hotkey_in_this_machines_words() {
        let settings = Settings {
            hide_hotkey: "Ctrl-Alt-Super-B".to_string(),
            ..Settings::default()
        };
        let view = SettingsView::from_parts(
            &settings,
            Path::new("/tmp/memory.md"),
            None,
            Vec::new(),
            Vec::new(),
            (false, String::new(), String::new()),
        );
        let expected = parse_hotkey("Ctrl-Alt-Super-B")
            .expect("stored spec parses")
            .display(ModifierWords::current());
        assert_eq!(view.hide_hotkey, expected);
    }

    /// A file nobody can parse must still name the chord the shell registers.
    #[test]
    fn an_unreadable_spec_is_shown_as_the_default_the_shell_binds() {
        assert_eq!(
            display_hotkey("Control-F1"),
            parse_hotkey(DEFAULT_HIDE_HOTKEY)
                .expect("default parses")
                .display(ModifierWords::current())
        );
    }

    #[test]
    fn a_chord_with_no_modifiers_prints_just_the_letter() {
        let printed = parse_hotkey("H")
            .expect("letter")
            .display(ModifierWords::Mac);
        assert_eq!(printed, "H");
    }
}
