//! The window-names notice. Chat shows it when something asked what is on
//! screen and the answer had no names.
//!
//! `NamesHint` is derived on each read from usable consent, the durable
//! dismissal, and a process-lifetime latch. Nothing here can grant consent.

use std::io;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use ai_buddy_core::window_source::{Capabilities, WindowSource, WorldGeometry};

use crate::settings::Settings;

/// What `hint` reads, kept off the stored state so the three inputs cannot drift.
struct Standing {
    names_usable: bool,
    dismissed: bool,
    asked_unnamed: bool,
}

impl From<Standing> for NamesHint {
    fn from(standing: Standing) -> Self {
        if standing.names_usable {
            Self::Quiet
        } else if standing.dismissed {
            Self::Dismissed
        } else if standing.asked_unnamed {
            Self::Due
        } else {
            Self::Quiet
        }
    }
}

/// The event carrying [`HintPush`] to every open Chat surface.
pub const EVENT: &str = "names-hint";

/// What a Chat surface should draw.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum NamesHint {
    /// Names are usable, or nothing has asked for them.
    Quiet,
    /// Something asked what is on screen and got no names back.
    Due,
    /// The user said no, once and for good.
    Dismissed,
}

/// A pushed notice. `generation` climbs only when `hint` changes, so a replay
/// cannot outrank a newer one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub struct HintPush {
    pub hint: NamesHint,
    pub generation: u64,
}

/// Whether window names are usable now. A question, so this module cannot grant.
pub trait NamesConsent: Send + Sync {
    fn usable(&self) -> bool;
}

/// The running app's answer.
pub struct LiveConsent;

impl NamesConsent for LiveConsent {
    fn usable(&self) -> bool {
        crate::consent::usable(
            crate::consent::CapabilityId::WindowNames,
            crate::consent::live(),
        )
    }
}

/// What has asked, and what to draw from it.
pub struct Notice {
    /// Set once and never cleared. The frame loop and the HTTP thread both
    /// write true, so neither waits on the other.
    asked_unnamed: AtomicBool,
    names: &'static dyn NamesConsent,
    /// The last hint a caller published, and how many times that hint changed.
    drawn: Mutex<Drawn>,
}

struct Drawn {
    hint: Option<NamesHint>,
    generation: u64,
}

/// The app's one notice.
pub fn live() -> &'static Notice {
    static LIVE: Notice = Notice::new(&LiveConsent);
    &LIVE
}

impl Notice {
    /// `names` is the only consent question this notice can ask.
    pub const fn new(names: &'static dyn NamesConsent) -> Self {
        Self {
            asked_unnamed: AtomicBool::new(false),
            names,
            drawn: Mutex::new(Drawn {
                hint: None,
                generation: 0,
            }),
        }
    }

    /// The desktop a tool is about to read, watched for an answer with no names.
    pub fn watching<'a>(&'a self, desktop: &'a dyn WindowSource) -> Watching<'a> {
        Watching {
            desktop,
            notice: self,
        }
    }

    /// Quiet, Due, or Dismissed from usable consent, the dismissal, and the latch.
    pub fn hint(&self, settings: &Settings) -> NamesHint {
        Standing {
            names_usable: self.names.usable(),
            dismissed: settings.names_hint_dismissed,
            asked_unnamed: self.asked_unnamed.load(Ordering::Relaxed),
        }
        .into()
    }

    /// Whether this run was asked for names it could not give. The frame loop
    /// stays quiet until then.
    pub fn armed(&self) -> bool {
        self.asked_unnamed.load(Ordering::Relaxed)
    }

    /// The payload Chat should draw.
    pub fn publish(&self, settings: &Settings) -> HintPush {
        let hint = self.hint(settings);
        let mut drawn = self
            .drawn
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if drawn.hint != Some(hint) {
            drawn.generation = drawn.generation.saturating_add(1);
            drawn.hint = Some(hint);
        }
        HintPush {
            hint,
            generation: drawn.generation,
        }
    }

    /// `ai-buddy://windows` was read. The resource path sees no desktop, so
    /// this arms on consent alone.
    pub fn titles_were_read(&self) {
        if !self.names.usable() {
            self.arm();
        }
    }

    /// Record a button press and the notice Chat should draw after it.
    pub fn acted(
        &self,
        press: Press,
        settings: &Mutex<Settings>,
        path: &Path,
    ) -> io::Result<Acted> {
        let push = {
            let mut held = settings.lock().map_err(poisoned)?;
            if press == Press::Dismiss {
                held.names_hint_dismissed = true;
            }
            self.publish(&held)
        };
        if press == Press::Dismiss {
            crate::settings::flush_settings(settings, path)?;
        }
        Ok(Acted {
            push,
            then: press.then(),
        })
    }

    fn arm(&self) {
        self.asked_unnamed.fetch_or(true, Ordering::Relaxed);
    }
}

fn poisoned(error: std::sync::PoisonError<std::sync::MutexGuard<'_, Settings>>) -> io::Error {
    io::Error::other(format!("settings lock poisoned: {error}"))
}

/// What the user pressed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Press {
    /// No, and do not ask again.
    Dismiss,
    /// Take me to the switch. Does not flip it.
    OpenSettings,
}

impl Press {
    /// The wire is a string because the command is. Past this, it is a `Press`.
    pub fn parse(action: &str) -> Option<Self> {
        match action {
            "dismiss" => Some(Self::Dismiss),
            "open-settings" => Some(Self::OpenSettings),
            _ => None,
        }
    }

    fn then(self) -> Then {
        match self {
            Self::Dismiss => Then::Nothing,
            Self::OpenSettings => Then::Reveal(crate::settings::form::Reveal::WindowNamesConsent),
        }
    }
}

/// The state after a press, and whatever only the Shell can still do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Acted {
    pub push: HintPush,
    pub then: Then,
}

/// Work the Shell does. The notice never holds an `AppHandle`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Then {
    Nothing,
    Reveal(crate::settings::form::Reveal),
}

/// A desktop that reports a nameless answer to its notice.
pub struct Watching<'a> {
    desktop: &'a dyn WindowSource,
    notice: &'a Notice,
}

impl WindowSource for Watching<'_> {
    fn capabilities(&self) -> Capabilities {
        self.desktop.capabilities()
    }

    fn read(&self) -> WorldGeometry {
        self.desktop.read()
    }

    fn snapshot(&self) -> WorldGeometry {
        // `snapshot` drops windows when the platform declares no geometry.
        // Arming from `read` would treat that empty tool result as nameless.
        let geometry = self.desktop.snapshot();
        let nameless = !geometry.windows.is_empty()
            && geometry.windows.iter().all(|window| window.owner.is_none());
        if nameless && !self.notice.names.usable() {
            self.notice.arm();
        }
        geometry
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ai_buddy_core::window_source::{Rect, WindowRect};

    struct FixedConsent(bool);

    impl NamesConsent for FixedConsent {
        fn usable(&self) -> bool {
            self.0
        }
    }

    static NAMES_OFF: FixedConsent = FixedConsent(false);
    static NAMES_ON: FixedConsent = FixedConsent(true);

    struct Desktop {
        windows: Vec<WindowRect>,
        geometry: bool,
    }

    impl WindowSource for Desktop {
        fn capabilities(&self) -> Capabilities {
            Capabilities {
                window_geometry: self.geometry,
                absolute_positioning: false,
            }
        }

        fn read(&self) -> WorldGeometry {
            WorldGeometry {
                windows: self.windows.clone(),
                ..WorldGeometry::default()
            }
        }
    }

    fn window(owner: Option<&str>) -> WindowRect {
        WindowRect {
            id: 1,
            bounds: Rect {
                x: 0.0,
                y: 0.0,
                width: 100.0,
                height: 80.0,
            },
            owner: owner.map(str::to_string),
            title: None,
            layer: 0,
        }
    }

    #[test]
    fn a_nameless_desktop_is_due_and_an_empty_one_stays_quiet() {
        let settings = Settings::default();

        let empty = Notice::new(&NAMES_OFF);
        let empty_read = empty
            .watching(&Desktop {
                windows: vec![],
                geometry: true,
            })
            .snapshot();
        assert!(empty_read.windows.is_empty());
        assert_eq!(empty.hint(&settings), NamesHint::Quiet);

        let named = Notice::new(&NAMES_OFF);
        let _ = named
            .watching(&Desktop {
                windows: vec![window(Some("Terminal"))],
                geometry: true,
            })
            .snapshot();
        assert_eq!(named.hint(&settings), NamesHint::Quiet);

        let nameless = Notice::new(&NAMES_OFF);
        let nameless_read = nameless
            .watching(&Desktop {
                windows: vec![window(None)],
                geometry: true,
            })
            .snapshot();
        assert_eq!(nameless_read.windows.len(), 1);
        assert_eq!(nameless.hint(&settings), NamesHint::Due);

        let withheld = Notice::new(&NAMES_OFF);
        let cleared = withheld
            .watching(&Desktop {
                windows: vec![window(None)],
                geometry: false,
            })
            .snapshot();
        assert!(cleared.windows.is_empty());
        assert_eq!(withheld.hint(&settings), NamesHint::Quiet);
    }

    #[test]
    fn titles_were_read_arms_only_when_names_are_not_usable() {
        let off = Notice::new(&NAMES_OFF);
        off.titles_were_read();
        assert_eq!(off.hint(&Settings::default()), NamesHint::Due);

        let on = Notice::new(&NAMES_ON);
        on.titles_were_read();
        assert_eq!(on.hint(&Settings::default()), NamesHint::Quiet);

        let again = off.publish(&Settings::default());
        let repeat = off.publish(&Settings::default());
        assert_eq!(again.hint, NamesHint::Due);
        assert_eq!(repeat, again);
    }

    #[test]
    fn a_dismissal_is_what_the_next_notice_reads() {
        let path = std::env::temp_dir().join(format!(
            "ai-buddy-names-hint-{}-dismiss.json",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        let settings = Mutex::new(Settings::default());
        let notice = Notice::new(&NAMES_OFF);
        notice.titles_were_read();

        let acted = notice
            .acted(Press::Dismiss, &settings, &path)
            .expect("saved");
        assert_eq!(acted.push.hint, NamesHint::Dismissed);

        let loaded = Settings::load(&path);
        assert!(loaded.names_hint_dismissed);
        assert!(!loaded.use_window_names);
        assert_eq!(Notice::new(&NAMES_OFF).hint(&loaded), NamesHint::Dismissed);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn usable_names_quiet_the_notice_even_when_asked_and_dismissed() {
        let notice = Notice::new(&NAMES_ON);
        notice.titles_were_read();
        let dismissed = Settings {
            names_hint_dismissed: true,
            ..Settings::default()
        };
        assert_eq!(notice.hint(&dismissed), NamesHint::Quiet);
        assert_eq!(notice.hint(&Settings::default()), NamesHint::Quiet);
    }

    #[test]
    fn acting_on_the_notice_grants_nothing() {
        let path = std::env::temp_dir().join(format!(
            "ai-buddy-names-hint-{}-act.json",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        let settings = Mutex::new(Settings::default());
        let notice = Notice::new(&NAMES_OFF);
        notice.titles_were_read();

        let open = notice
            .acted(Press::OpenSettings, &settings, &path)
            .expect("acted");
        assert_eq!(open.push.hint, NamesHint::Due);
        assert_eq!(
            open.then,
            Then::Reveal(crate::settings::form::Reveal::WindowNamesConsent)
        );
        let target = crate::settings::form::Reveal::WindowNamesConsent.target();
        assert_eq!(target.tab, "Privacy");
        assert_eq!(target.row, window_names_row());
        assert!(!settings.lock().expect("lock").names_hint_dismissed);
        assert!(!settings.lock().expect("lock").use_window_names);
        assert!(
            !path.exists(),
            "Open Settings writes nothing, so no settings file appears"
        );

        let dismiss = notice
            .acted(Press::Dismiss, &settings, &path)
            .expect("acted");
        assert_eq!(dismiss.push.hint, NamesHint::Dismissed);
        assert_eq!(dismiss.push.generation, open.push.generation + 1);
        assert_eq!(dismiss.then, Then::Nothing);
        let loaded = Settings::load(&path);
        assert!(!loaded.use_window_names);
        assert!(loaded.names_hint_dismissed);
        let _ = std::fs::remove_file(&path);
    }

    fn window_names_row() -> &'static str {
        #[cfg(target_os = "macos")]
        {
            crate::settings::form::CONSENT_SCREEN_RECORDING_ID
        }
        #[cfg(target_os = "windows")]
        {
            crate::settings::form::CONSENT_WINDOW_NAMES_ID
        }
        #[cfg(target_os = "linux")]
        {
            crate::settings::form::CONSENT_PORTAL_SCREENCAST_ID
        }
    }
}
