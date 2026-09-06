//! Consented capabilities settings can offer.
//!
//! DESIGN.md decision 9: first run grants nothing, and the app never prompts
//! at launch. The upgrade path is settings (#148): this catalog names each
//! grant, what it buys, what it costs, and the system prompt fires only when
//! the user flips one on.

use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CapabilityId {
    Accessibility,
    ScreenRecording,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Capability {
    pub id: CapabilityId,
    pub title: &'static str,
    pub buys: &'static str,
    pub costs: &'static str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConsentRow {
    pub id: CapabilityId,
    pub title: &'static str,
    pub buys: &'static str,
    pub costs: &'static str,
    /// Settings has this capability on. Separate from the OS grant: flipping
    /// on prompts if needed; flipping off stops using it and does not revoke.
    pub granted: bool,
}

/// How settings learns a grant and how it asks. Tests inject a fake; macOS
/// talks to TCC; everywhere else is Null.
pub trait Probe: Send + Sync {
    fn granted(&self, id: CapabilityId) -> bool;
    fn prompt(&self, id: CapabilityId);
}

pub const CAPABILITIES: &[Capability] = &[
    Capability {
        id: CapabilityId::Accessibility,
        title: "Accessibility",
        buys: "Exact Dock geometry, so the sprite does not walk into the Dock.",
        costs: "macOS Accessibility. The buddy reads the Dock's bounds; it does not control your computer.",
    },
    Capability {
        id: CapabilityId::ScreenRecording,
        title: "Screen Recording",
        buys: "Window titles, and Capture when it ships.",
        costs: "macOS Screen Recording, which can see the screen.",
    },
];

/// Linux, and tests that do not care about the live OS.
#[cfg(not(target_os = "macos"))]
pub struct Null;

static WANT_ACCESSIBILITY: AtomicBool = AtomicBool::new(false);
static WANT_SCREEN_RECORDING: AtomicBool = AtomicBool::new(false);

/// Whether the buddy should use this grant. The OS grant can remain after
/// the user unchecks; Dock geometry and titles must still follow this.
#[cfg(target_os = "macos")]
pub fn wanted(id: CapabilityId) -> bool {
    match id {
        CapabilityId::Accessibility => WANT_ACCESSIBILITY.load(Ordering::Relaxed),
        CapabilityId::ScreenRecording => WANT_SCREEN_RECORDING.load(Ordering::Relaxed),
    }
}

pub fn set_wanted(id: CapabilityId, on: bool) {
    match id {
        CapabilityId::Accessibility => WANT_ACCESSIBILITY.store(on, Ordering::Relaxed),
        CapabilityId::ScreenRecording => WANT_SCREEN_RECORDING.store(on, Ordering::Relaxed),
    }
}

#[cfg(target_os = "macos")]
struct Macos;

#[cfg(not(target_os = "macos"))]
impl Probe for Null {
    fn granted(&self, _: CapabilityId) -> bool {
        false
    }

    fn prompt(&self, _: CapabilityId) {}
}

#[cfg(target_os = "macos")]
impl Probe for Macos {
    fn granted(&self, id: CapabilityId) -> bool {
        match id {
            CapabilityId::Accessibility => macos::accessibility_granted(),
            CapabilityId::ScreenRecording => macos::screen_recording_granted(),
        }
    }

    fn prompt(&self, id: CapabilityId) {
        match id {
            CapabilityId::Accessibility => macos::request_accessibility(),
            CapabilityId::ScreenRecording => macos::request_screen_recording(),
        }
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use std::ffi::c_void;

    use objc2::rc::Retained;
    use objc2_foundation::{ns_string, NSDictionary, NSNumber, NSString};

    #[link(name = "ApplicationServices", kind = "framework")]
    unsafe extern "C" {
        fn AXIsProcessTrusted() -> bool;
        fn AXIsProcessTrustedWithOptions(options: *const c_void) -> bool;
    }

    #[link(name = "CoreGraphics", kind = "framework")]
    unsafe extern "C" {
        fn CGPreflightScreenCaptureAccess() -> bool;
        fn CGRequestScreenCaptureAccess() -> bool;
    }

    pub fn accessibility_granted() -> bool {
        // SAFETY: takes nothing, returns a BOOL, documented callable anywhere.
        unsafe { AXIsProcessTrusted() }
    }

    pub fn screen_recording_granted() -> bool {
        // SAFETY: takes nothing, returns whether this process may capture.
        unsafe { CGPreflightScreenCaptureAccess() }
    }

    pub fn request_accessibility() {
        let prompt = NSNumber::new_bool(true);
        let options: Retained<NSDictionary<NSString, NSNumber>> = NSDictionary::from_slices(
            &[ns_string!("AXTrustedCheckOptionPrompt")],
            &[prompt.as_ref()],
        );
        // SAFETY: toll-free to CFDictionary; the prompt key is the public
        // constant's string value, and the dict lives for the call.
        unsafe {
            AXIsProcessTrustedWithOptions(std::ptr::from_ref(&*options).cast());
        }
    }

    pub fn request_screen_recording() {
        // SAFETY: the prompt API; we ignore the BOOL, the checkbox rereads.
        unsafe {
            let _ = CGRequestScreenCaptureAccess();
        }
    }

    pub fn tcc_list_name() -> String {
        if packaged() {
            return localized_name(std::process::id() as i32).unwrap_or_else(|| "ai-buddy".into());
        }
        let self_pid = std::process::id() as i32;
        // Unsigned `cargo run` is often "responsible" for itself; TCC still
        // attributes the grant to the bundled parent that launched the shell.
        responsible_pid()
            .filter(|pid| *pid != self_pid)
            .and_then(localized_name)
            .filter(|name| !name.contains("Helper"))
            .or_else(bundled_ancestor_name)
            .or_else(|| localized_name(self_pid))
            .unwrap_or_else(|| "ai-buddy".into())
    }

    fn packaged() -> bool {
        std::env::current_exe().is_ok_and(|exe| {
            exe.ancestors()
                .any(|p| p.extension().is_some_and(|e| e == "app"))
        })
    }

    fn localized_name(pid: i32) -> Option<String> {
        objc2_app_kit::NSRunningApplication::runningApplicationWithProcessIdentifier(pid)?
            .localizedName()
            .map(|name| name.to_string())
            .filter(|name| !name.is_empty())
    }

    fn responsible_pid() -> Option<i32> {
        type GetResponsible = unsafe extern "C" fn(i32) -> i32;
        // SAFETY: RTLD_DEFAULT searches loaded images; libSystem is always in.
        let symbol = unsafe {
            dlsym(
                -2isize as *mut c_void,
                c"responsibility_get_pid_responsible_for_pid".as_ptr(),
            )
        };
        if symbol.is_null() {
            return None;
        }
        // SAFETY: the SPI is pid in, pid out. Self is a valid answer for a
        // packaged app. A reshape would be a wrong pid, and the caller still
        // has to resolve a localized name from it.
        let pid = unsafe {
            let get: GetResponsible = std::mem::transmute(symbol);
            get(std::process::id() as i32)
        };
        (pid > 0).then_some(pid)
    }

    /// Walk parents until one is a bundled app. A `cargo run` from Cursor's
    /// terminal is often zsh → Cursor Helper → Cursor; TCC names Cursor.
    fn bundled_ancestor_name() -> Option<String> {
        // SAFETY: getppid takes no argument, returns a pid, and cannot fail.
        let mut pid = unsafe { getppid() };
        for _ in 0..24 {
            if pid <= 1 {
                break;
            }
            if let Some(app) =
                objc2_app_kit::NSRunningApplication::runningApplicationWithProcessIdentifier(pid)
            {
                let bundled = app.bundleURL().is_some_and(|url| {
                    url.path()
                        .is_some_and(|path| path.to_string().contains(".app"))
                });
                if bundled {
                    let name = app
                        .localizedName()
                        .map(|name| name.to_string())
                        .filter(|name| !name.is_empty())?;
                    // Cursor Helper.app sits inside Cursor.app; TCC names Cursor.
                    if !name.contains("Helper") {
                        return Some(name);
                    }
                }
            }
            pid = parent_pid(pid)?;
        }
        None
    }

    fn parent_pid(pid: i32) -> Option<i32> {
        let mut buf = [0u8; 232];
        // SAFETY: `buf` is a local array and the call is handed its own length,
        // so proc_pidinfo cannot write past it.
        let wrote = unsafe {
            proc_pidinfo(
                pid,
                PROC_PIDTBSDINFO,
                0,
                buf.as_mut_ptr().cast(),
                buf.len() as i32,
            )
        };
        if wrote < 20 {
            return None;
        }
        let ppid = u32::from_ne_bytes(buf[16..20].try_into().ok()?);
        (ppid > 1).then_some(ppid as i32)
    }

    const PROC_PIDTBSDINFO: i32 = 5;

    unsafe extern "C" {
        fn dlsym(handle: *mut c_void, symbol: *const std::ffi::c_char) -> *mut c_void;
        fn getppid() -> i32;
        fn proc_pidinfo(
            pid: i32,
            flavor: i32,
            arg: u64,
            buffer: *mut c_void,
            buffersize: i32,
        ) -> i32;
    }
}

pub fn rows(wanted: impl Fn(CapabilityId) -> bool) -> Vec<ConsentRow> {
    CAPABILITIES
        .iter()
        .map(|cap| ConsentRow {
            id: cap.id,
            title: cap.title,
            buys: cap.buys,
            costs: cap.costs,
            granted: wanted(cap.id),
        })
        .collect()
}

pub fn enable(id: CapabilityId, probe: &dyn Probe) {
    if !probe.granted(id) {
        probe.prompt(id);
    }
}

/// The sentence settings prints so the user can find the row in System Settings.
///
/// A `cargo run` binary is unsigned, so TCC attributes the grant to whoever
/// launched it — Cursor, Terminal — not to "ai-buddy". A packaged
/// `.app` is listed under its own name.
pub fn listed_under_hint(name: &str) -> String {
    format!("macOS lists this app as {name}, under Privacy & Security.")
}

/// The pane copy. The listed name is live: a `cargo run` from Cursor is
/// Cursor, a packaged build is ai-buddy.
pub fn pane_intro(listed_as: &str) -> String {
    format!(
        "Checking a box asks macOS for the permission. {}",
        listed_under_hint(listed_as)
    )
}

/// The localized name TCC will show. Packaged builds are this app; `cargo run`
/// is the responsible parent (the IDE or terminal that launched it).
pub fn process_listed_as() -> String {
    #[cfg(target_os = "macos")]
    {
        macos::tcc_list_name()
    }
    #[cfg(not(target_os = "macos"))]
    {
        "ai-buddy".into()
    }
}

pub fn live() -> &'static dyn Probe {
    #[cfg(target_os = "macos")]
    {
        &Macos
    }
    #[cfg(not(target_os = "macos"))]
    {
        &Null
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    struct Fake {
        granted: Vec<CapabilityId>,
        prompted: Mutex<Vec<CapabilityId>>,
    }

    impl Fake {
        fn granting(ids: &[CapabilityId]) -> Self {
            Self {
                granted: ids.to_vec(),
                prompted: Mutex::new(Vec::new()),
            }
        }
    }

    impl Probe for Fake {
        fn granted(&self, id: CapabilityId) -> bool {
            self.granted.contains(&id)
        }

        fn prompt(&self, id: CapabilityId) {
            self.prompted.lock().expect("prompt log").push(id);
        }
    }

    /// The window prints this catalog. Dropping a row makes that grant
    /// unreachable again: nothing else names the trade. #148.
    #[test]
    fn the_catalog_names_each_capability_and_its_trade() {
        let rows = rows(|_| false);
        assert_eq!(rows.len(), 2);

        assert_eq!(rows[0].id, CapabilityId::Accessibility);
        assert_eq!(rows[0].title, "Accessibility");
        assert!(
            rows[0].buys.contains("Dock"),
            "Accessibility has to say what the Dock grant buys, got {:?}",
            rows[0].buys
        );
        assert!(
            rows[0].costs.contains("Accessibility"),
            "Accessibility has to name the macOS grant, got {:?}",
            rows[0].costs
        );
        assert!(!rows[0].granted);

        assert_eq!(rows[1].id, CapabilityId::ScreenRecording);
        assert_eq!(rows[1].title, "Screen Recording");
        assert!(
            rows[1].buys.contains("title"),
            "Screen Recording has to say titles are what it buys, got {:?}",
            rows[1].buys
        );
        assert!(
            rows[1].costs.contains("Screen Recording"),
            "Screen Recording has to name the macOS grant, got {:?}",
            rows[1].costs
        );
        assert!(!rows[1].granted);
    }

    /// The checkbox is settings intent, not the OS grant. An OS grant the
    /// user turned off here must show as off or they cannot stop the buddy
    /// using it.
    #[test]
    fn rows_report_wanted_capabilities() {
        let rows = rows(|id| id == CapabilityId::Accessibility);
        assert!(rows[0].granted);
        assert!(!rows[1].granted);
    }

    /// Flipping on is what decision 9 allows: the system prompt at that
    /// moment, never at launch.
    #[test]
    fn enabling_an_ungranted_capability_prompts() {
        let probe = Fake::granting(&[]);
        enable(CapabilityId::ScreenRecording, &probe);
        assert_eq!(
            *probe.prompted.lock().expect("prompt log"),
            [CapabilityId::ScreenRecording]
        );
    }

    /// Already granted: a second prompt is noise, and on macOS can look
    /// like the app is asking again for something the user already gave.
    #[test]
    fn enabling_a_granted_capability_does_not_prompt() {
        let probe = Fake::granting(&[CapabilityId::Accessibility]);
        enable(CapabilityId::Accessibility, &probe);
        assert!(probe.prompted.lock().expect("prompt log").is_empty());
    }

    /// A `cargo run` from Cursor is listed as Cursor, not ai-buddy. The
    /// hint has to carry that name or the Accessibility list is a guessing game.
    #[test]
    fn the_grant_hint_names_the_app_macos_will_list() {
        let hint = listed_under_hint("Cursor");
        assert!(
            hint.contains("Cursor"),
            "the user has to see the TCC row name, got {hint:?}"
        );
        assert!(
            hint.contains("Privacy & Security"),
            "the hint has to say where to look, got {hint:?}"
        );
        assert!(!listed_under_hint("Terminal").contains("Cursor"));
    }

    #[test]
    fn process_listed_as_is_not_empty() {
        assert!(!process_listed_as().is_empty());
    }
}
