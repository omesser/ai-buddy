//! Consented capabilities settings can offer.
//!
//! DESIGN.md decision 9: first run grants nothing, and the app never prompts
//! at launch. The upgrade path is settings (#148): this catalog names each
//! grant, what it buys, what it costs, and the system prompt fires only when
//! the user flips one on.

use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub enum CapabilityId {
    /// macOS only: reading Dock bounds and window geometry.
    #[cfg(not(target_os = "linux"))]
    Accessibility,
    /// macOS only: reading window titles and screen contents.
    #[cfg(not(target_os = "linux"))]
    ScreenRecording,
    /// macOS only: the idle event tap (#721).
    #[cfg(target_os = "macos")]
    InputMonitoring,
    /// Linux only: xdg-desktop-portal ScreenCast for screen recording/streaming.
    #[cfg(target_os = "linux")]
    PortalScreenCast,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Capability {
    pub id: CapabilityId,
    pub title: &'static str,
    pub buys: &'static str,
    pub costs: &'static str,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
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

#[cfg(not(target_os = "linux"))]
pub const CAPABILITIES: &[Capability] = &[
    #[cfg(not(target_os = "linux"))]
    Capability {
        id: CapabilityId::Accessibility,
        title: "Accessibility",
        buys: "Exact Dock geometry, so the sprite does not walk into the Dock.",
        costs: "macOS Accessibility. The buddy reads the Dock's bounds; it does not control your computer.",
    },
    #[cfg(not(target_os = "linux"))]
    Capability {
        id: CapabilityId::ScreenRecording,
        title: "Screen Recording",
        buys: "Window titles, and Capture when it ships.",
        costs: "macOS Screen Recording, which can see the screen.",
    },
    #[cfg(target_os = "macos")]
    Capability {
        id: CapabilityId::InputMonitoring,
        title: "Input Monitoring",
        buys: "The buddy notices the mouse the moment it moves, instead of up to a second later while it sits idle.",
        costs: "macOS Input Monitoring. The buddy listens for mouse movement and clicks — that a mouse moved, never what you type.",
    },
];

#[cfg(target_os = "linux")]
pub const CAPABILITIES: &[Capability] = &[
    Capability {
        id: CapabilityId::PortalScreenCast,
        title: "Screen Cast",
        buys: "Screen recording and streaming for Capture when it ships.",
        costs: "xdg-desktop-portal ScreenCast. Your desktop prompts when you enable this; accepting shows the consent was granted. Off does not revoke the portal session while the app runs.",
    },
];

/// Tests that do not care about the live OS, and the fallback when a platform
/// has no consent system. Was Linux's answer when X11 sensing was consent-free;
/// the portal Probe replaced it once Capture needed a grant.
#[cfg(any(test, not(any(target_os = "macos", target_os = "linux"))))]
pub struct Null;

#[cfg(not(target_os = "linux"))]
static WANT_ACCESSIBILITY: AtomicBool = AtomicBool::new(false);
#[cfg(not(target_os = "linux"))]
static WANT_SCREEN_RECORDING: AtomicBool = AtomicBool::new(false);
#[cfg(target_os = "macos")]
static WANT_INPUT_MONITORING: AtomicBool = AtomicBool::new(false);
#[cfg(target_os = "linux")]
static WANT_PORTAL_SCREENCAST: AtomicBool = AtomicBool::new(false);

#[cfg(target_os = "linux")]
static GRANTED_PORTAL_SCREENCAST: AtomicBool = AtomicBool::new(false);

/// Whether the buddy should use this grant. The OS grant can remain after
/// the user unchecks; Dock geometry and titles must still follow this.
#[cfg(not(target_os = "windows"))]
pub fn wanted(id: CapabilityId) -> bool {
    match id {
        #[cfg(not(target_os = "linux"))]
        CapabilityId::Accessibility => WANT_ACCESSIBILITY.load(Ordering::Relaxed),
        #[cfg(not(target_os = "linux"))]
        CapabilityId::ScreenRecording => WANT_SCREEN_RECORDING.load(Ordering::Relaxed),
        #[cfg(target_os = "macos")]
        CapabilityId::InputMonitoring => WANT_INPUT_MONITORING.load(Ordering::Relaxed),
        #[cfg(target_os = "linux")]
        CapabilityId::PortalScreenCast => WANT_PORTAL_SCREENCAST.load(Ordering::Relaxed),
    }
}

pub fn set_wanted(id: CapabilityId, on: bool) {
    match id {
        #[cfg(not(target_os = "linux"))]
        CapabilityId::Accessibility => WANT_ACCESSIBILITY.store(on, Ordering::Relaxed),
        #[cfg(not(target_os = "linux"))]
        CapabilityId::ScreenRecording => WANT_SCREEN_RECORDING.store(on, Ordering::Relaxed),
        #[cfg(target_os = "macos")]
        CapabilityId::InputMonitoring => WANT_INPUT_MONITORING.store(on, Ordering::Relaxed),
        #[cfg(target_os = "linux")]
        CapabilityId::PortalScreenCast => WANT_PORTAL_SCREENCAST.store(on, Ordering::Relaxed),
    }
}

#[cfg(target_os = "macos")]
struct Macos;

#[cfg(target_os = "linux")]
struct LinuxPortal;

#[cfg(any(test, not(any(target_os = "macos", target_os = "linux"))))]
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
            CapabilityId::InputMonitoring => macos::input_monitoring_granted(),
        }
    }

    fn prompt(&self, id: CapabilityId) {
        match id {
            CapabilityId::Accessibility => macos::request_accessibility(),
            CapabilityId::ScreenRecording => macos::request_screen_recording(),
            CapabilityId::InputMonitoring => macos::request_input_monitoring(),
        }
    }
}

#[cfg(target_os = "linux")]
impl Probe for LinuxPortal {
    fn granted(&self, id: CapabilityId) -> bool {
        match id {
            CapabilityId::PortalScreenCast => linux::portal_screencast_granted(),
        }
    }

    fn prompt(&self, id: CapabilityId) {
        match id {
            CapabilityId::PortalScreenCast => linux::request_portal_screencast(),
        }
    }
}

#[cfg(target_os = "windows")]
mod windows {
    use std::collections::HashMap;

    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
        TH32CS_SNAPPROCESS,
    };

    pub fn process_list_name() -> String {
        if packaged() {
            return "ai-buddy".into();
        }
        parent_chain_name().unwrap_or_else(|| "ai-buddy".into())
    }

    fn packaged() -> bool {
        // Packaged: not under target/debug or target/release build directories.
        // An installed NSIS build lives in Program Files or AppData; a dev
        // build is always under the Cargo target directory.
        std::env::current_exe().is_ok_and(|exe| {
            !exe.to_string_lossy().contains(r"\target\debug")
                && !exe.to_string_lossy().contains(r"\target\release")
        })
    }

    /// One snapshot of the process list, closed once by `Drop`.
    struct Snapshot(HANDLE);

    impl Drop for Snapshot {
        fn drop(&mut self) {
            // SAFETY: the handle came from CreateToolhelp32Snapshot and the
            // type is neither Copy nor Clone, so this is its only close.
            unsafe { CloseHandle(self.0) };
        }
    }

    impl Snapshot {
        fn processes() -> Option<Self> {
            // SAFETY: TH32CS_SNAPPROCESS takes no pid and nothing goes in by
            // pointer; failure arrives as INVALID_HANDLE_VALUE, which the next
            // line rejects rather than taking ownership of.
            let handle = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
            (handle != INVALID_HANDLE_VALUE).then_some(Self(handle))
        }

        fn table(&self) -> HashMap<u32, (u32, String)> {
            // SAFETY: PROCESSENTRY32W is integers and a `[u16; 260]`, so
            // all-zeroes is a valid value for it. `dwSize` is the one field the
            // API requires the caller to fill, and the next line fills it.
            let mut entry: PROCESSENTRY32W = unsafe { std::mem::zeroed() };
            entry.dwSize = size_of::<PROCESSENTRY32W>() as u32;

            let mut table = HashMap::new();
            // SAFETY: `self.0` is live for as long as `self`, and `entry` is an
            // initialized local carrying its `dwSize` — the two things the walk
            // asks of the caller. It writes only into `entry`.
            if unsafe { Process32FirstW(self.0, &mut entry) } == 0 {
                return table;
            }
            loop {
                table.insert(
                    entry.th32ProcessID,
                    (entry.th32ParentProcessID, parse_exe_name(&entry.szExeFile)),
                );
                // SAFETY: as Process32FirstW above; a zero return ends the walk.
                if unsafe { Process32NextW(self.0, &mut entry) } == 0 {
                    return table;
                }
            }
        }
    }

    fn parent_chain_name() -> Option<String> {
        let current_pid = std::process::id();
        let table = Snapshot::processes()?.table();

        let mut pid = current_pid;
        for _ in 0..24 {
            pid = table.get(&pid)?.0;
            if pid == 0 || pid == current_pid {
                return None;
            }
            let name = &table.get(&pid)?.1;
            // An empty name is as unusable as a toolchain one: keep walking.
            if !name.is_empty() && !is_toolchain(name) {
                return Some(name.clone());
            }
        }
        None
    }

    pub(super) fn is_toolchain(name: &str) -> bool {
        // Skip Rust toolchain processes: a `cargo run` from Cursor is
        // cargo → Cursor, so the first non-toolchain parent is what Privacy
        // will list. Mirrors macOS bundled_ancestor_name skipping Helpers.
        matches!(
            name.to_lowercase().as_str(),
            "cargo" | "rustc" | "rustup" | "rust-analyzer" | "rls"
        )
    }

    fn parse_exe_name(sz_exe: &[u16; 260]) -> String {
        let end = sz_exe.iter().position(|&c| c == 0).unwrap_or(260);
        let name = String::from_utf16_lossy(&sz_exe[..end]);
        name.trim_end_matches(".exe")
            .split('\\')
            .next_back()
            .unwrap_or(&name)
            .to_string()
    }
}

#[cfg(target_os = "linux")]
mod linux {
    use super::GRANTED_PORTAL_SCREENCAST;
    use std::sync::atomic::Ordering;

    /// xdg-desktop-portal ScreenCast grant check. Returns true after a successful
    /// portal session creation. Process-local: a restart clears the grant, but that
    /// is honest (the portal has no query API and Settings flip-on is the prompt).
    pub fn portal_screencast_granted() -> bool {
        GRANTED_PORTAL_SCREENCAST.load(Ordering::Relaxed)
    }

    /// Request xdg-desktop-portal ScreenCast permission. Opens the desktop's
    /// portal dialog when Settings flips the row on. Creates a minimal session
    /// (no frames, no PipeWire connection) to show the consent dialog.
    pub fn request_portal_screencast() {
        let runtime = match tokio::runtime::Builder::new_current_thread().build() {
            Ok(r) => r,
            Err(why) => {
                eprintln!("ai-buddy: portal screencast runtime failed: {why}");
                return;
            }
        };

        runtime.block_on(async {
            match screencast_request().await {
                Ok(_) => {
                    GRANTED_PORTAL_SCREENCAST.store(true, Ordering::Relaxed);
                }
                Err(why) => {
                    eprintln!("ai-buddy: portal screencast request failed: {why}");
                }
            }
        });
    }

    async fn screencast_request() -> ashpd::Result<()> {
        use ashpd::desktop::screencast::{Screencast, SourceType};

        let proxy = Screencast::new().await?;
        let session = proxy.create_session(Default::default()).await?;

        let opts = ashpd::desktop::screencast::SelectSourcesOptions::default();
        proxy
            .select_sources(
                &session,
                opts.set_sources(SourceType::Monitor | SourceType::Window)
                    .set_multiple(false),
            )
            .await?;

        let _response = proxy
            .start(&session, None, Default::default())
            .await?
            .response()?;

        Ok(())
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use objc2_application_services::{
        kAXTrustedCheckOptionPrompt, AXIsProcessTrusted, AXIsProcessTrustedWithOptions,
    };
    use objc2_core_foundation::{CFBoolean, CFDictionary, CFString};
    use objc2_core_graphics::{
        CGPreflightListenEventAccess, CGPreflightScreenCaptureAccess, CGRequestListenEventAccess,
        CGRequestScreenCaptureAccess,
    };

    pub fn accessibility_granted() -> bool {
        // SAFETY: the binding takes nothing and documents no precondition;
        // objc2 marks every generated C function `unsafe` regardless. It is the
        // non-prompting half of the pair, so settings may poll it (decision 9).
        unsafe { AXIsProcessTrusted() }
    }

    pub fn screen_recording_granted() -> bool {
        // Preflight is the half that does not prompt — polling
        // `CGRequestScreenCaptureAccess` instead would put a dialog on screen
        // at launch.
        CGPreflightScreenCaptureAccess()
    }

    pub fn request_accessibility() {
        // SAFETY: a CoreFoundation string constant the dynamic linker binds
        // before any ApplicationServices entry point can run, never the
        // uninitialized memory the rule exists to catch.
        let key: &CFString = unsafe { kAXTrustedCheckOptionPrompt };
        let options = CFDictionary::from_slices(&[key], &[CFBoolean::new(true)]);
        // The answer is dropped: it reports the grant before the prompt this call raises.
        // SAFETY: the dictionary's generics match the key; kAXTrustedCheckOptionPrompt
        // takes a CFBoolean, the binding's one documented requirement.
        unsafe {
            AXIsProcessTrustedWithOptions(Some(options.as_ref()));
        }
    }

    pub fn request_screen_recording() {
        // The answer is dropped: it reports the grant as it stands now, before
        // the user has answered the dialog, and the checkbox re-reads it after.
        let _ = CGRequestScreenCaptureAccess();
    }

    pub fn input_monitoring_granted() -> bool {
        // Preflight is the half that does not prompt, as with Screen Recording
        // above. `CGEventTapCreate` is not a test: it can return a port that
        // was never enabled when the grant is missing.
        CGPreflightListenEventAccess()
    }

    pub fn request_input_monitoring() {
        // macOS opens Privacy & Security rather than granting in place, and
        // the tap only starts once the grant lands; `platform::spawn_event_tap`
        // retries every idle wait until it does.
        let _ = CGRequestListenEventAccess();
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
        // SAFETY: `-2` is RTLD_DEFAULT, so nothing has to have been dlopen'd
        // first; the name is a `c"…"` literal, NUL-terminated and alive for the
        // call. A missing symbol comes back null; the check below stops it being called.
        let symbol = unsafe {
            libc::dlsym(
                -2isize as *mut std::ffi::c_void,
                c"responsibility_get_pid_responsible_for_pid".as_ptr(),
            )
        };
        if symbol.is_null() {
            return None;
        }
        // SAFETY: `symbol` is non-null, checked directly above, and dlsym
        // returns the address of the code it resolved, so transmuting it to a
        // function pointer is the intended use of the pair. The declared shape
        // is the load-bearing part: `int f(int)` is what libSystem has exported
        // under this name for years, and a call through a wrong signature would
        // be undefined, not merely a wrong pid.
        //
        // This is the one FFI declaration in the file with no crate to defer
        // to: a private SPI with no public header, so there is nothing for a
        // binding generator to read and nothing to check the shape against.
        // The comment is the whole of the available mitigation, not an excuse
        // for a risk something else in the tree already carries.
        let pid = unsafe {
            let get: GetResponsible = std::mem::transmute(symbol);
            get(std::process::id() as i32)
        };
        (pid > 0).then_some(pid)
    }

    /// Walk parents until one is a bundled app. A `cargo run` from Cursor's
    /// terminal is often zsh → Cursor Helper → Cursor; TCC names Cursor.
    fn bundled_ancestor_name() -> Option<String> {
        let mut pid = std::os::unix::process::parent_id() as i32;
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

    /// The parent of `pid`, or `None` when the kernel will not say.
    ///
    /// Flavor, struct and field offset come from `libc`. A hand-written `PROC_PIDTBSDINFO` was 5 (`PROC_PIDTHREADINFO`) and every call wrote nothing.
    pub(super) fn parent_pid(pid: i32) -> Option<i32> {
        // SAFETY: proc_bsdinfo is integers and byte arrays, so all-zeroes is a
        // value it can hold; the call overwrites it on success.
        let mut info: libc::proc_bsdinfo = unsafe { std::mem::zeroed() };
        // SAFETY: the buffer is a local of exactly the type PROC_PIDTBSDINFO
        // names, and the size passed is its own, so nothing can be written past
        // it. A pid that has exited fails the call rather than writing.
        let wrote = unsafe {
            libc::proc_pidinfo(
                pid,
                libc::PROC_PIDTBSDINFO,
                0,
                std::ptr::from_mut(&mut info).cast(),
                size_of::<libc::proc_bsdinfo>() as i32,
            )
        };
        // A short write leaves pbi_ppid zeroed, which would read as a pid.
        if wrote != size_of::<libc::proc_bsdinfo>() as i32 {
            return None;
        }
        (info.pbi_ppid > 1).then_some(info.pbi_ppid as i32)
    }

    #[cfg(test)]
    mod tests {
        /// The old flavor made this return `None` for every pid, and nothing
        /// noticed: the only test over this path asserts a non-empty name.
        /// std's `parent_id` is the independent answer to check against.
        #[test]
        fn the_parent_pid_is_the_one_std_reports() {
            let ppid = std::os::unix::process::parent_id() as i32;
            assert!(
                ppid > 1,
                "the test binary should have a real parent, got {ppid}"
            );
            assert_eq!(super::parent_pid(std::process::id() as i32), Some(ppid));
        }
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
/// An unsigned `cargo run` is listed as whoever launched it, not "ai-buddy".
#[cfg(target_os = "macos")]
pub fn listed_under_hint(name: &str) -> String {
    format!("macOS lists this app as {name}, under Privacy & Security.")
}

/// The pane copy. The listed name is live: a `cargo run` from Cursor is
/// Cursor, a packaged build is ai-buddy.
#[cfg(target_os = "macos")]
pub fn pane_intro(listed_as: &str) -> String {
    format!(
        "Checking a box asks macOS for the permission. {}",
        listed_under_hint(listed_as)
    )
}

/// Linux-specific intro: xdg-desktop-portal for Capture when it ships.
///
/// Checkboxes are named after portal capabilities: ScreenCast and Screenshot.
/// No macOS-specific vocabulary (TCC, Privacy & Security, Accessibility).
#[cfg(target_os = "linux")]
pub fn linux_pane_intro() -> String {
    "Checking a box prompts via xdg-desktop-portal. Window positions are already readable without a grant."
        .to_string()
}

/// Windows-specific hint: names the process Privacy will list.
#[cfg(target_os = "windows")]
pub fn listed_under_hint(name: &str) -> String {
    format!("Windows lists this process as {name}.")
}

/// Windows pane intro: names the process for future reference.
#[cfg(target_os = "windows")]
pub fn pane_intro(listed_as: &str) -> String {
    format!(
        "These permissions are not required yet. When needed later, Windows will prompt. {}",
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
    #[cfg(target_os = "windows")]
    {
        windows::process_list_name()
    }
    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
    {
        "ai-buddy".into()
    }
}

pub fn live() -> &'static dyn Probe {
    #[cfg(target_os = "macos")]
    {
        &Macos
    }
    #[cfg(target_os = "linux")]
    {
        &LinuxPortal
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
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

    /// The window prints this catalog. Dropping a row makes that grant unreachable: nothing else names the trade.
    #[test]
    fn the_catalog_names_each_capability_and_its_trade() {
        let rows = rows(|_| false);

        #[cfg(target_os = "linux")]
        {
            assert_eq!(rows.len(), 1);
            assert_eq!(rows[0].id, CapabilityId::PortalScreenCast);
            assert_eq!(rows[0].title, "Screen Cast");
            assert!(
                rows[0].buys.contains("recording") || rows[0].buys.contains("Capture"),
                "ScreenCast has to say what recording buys, got {:?}",
                rows[0].buys
            );
            assert!(
                rows[0].costs.contains("portal") || rows[0].costs.contains("ScreenCast"),
                "ScreenCast has to name the portal, got {:?}",
                rows[0].costs
            );
            assert!(!rows[0].granted);
        }

        #[cfg(not(target_os = "linux"))]
        {
            assert_eq!(rows.len(), if cfg!(target_os = "macos") { 3 } else { 2 });

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

            #[cfg(target_os = "macos")]
            {
                assert_eq!(rows[2].id, CapabilityId::InputMonitoring);
                assert_eq!(rows[2].title, "Input Monitoring");
                assert!(
                    rows[2].buys.contains("mouse"),
                    "Input Monitoring has to say hearing the mouse is what it buys, got {:?}",
                    rows[2].buys
                );
                assert!(
                    rows[2].costs.contains("Input Monitoring"),
                    "Input Monitoring has to name the macOS grant, got {:?}",
                    rows[2].costs
                );
                assert!(
                    rows[2].costs.contains("never what you type"),
                    "the row has to say the tap is mouse-only, got {:?}",
                    rows[2].costs
                );
                assert!(!rows[2].granted);
            }
        }
    }

    /// The checkbox is settings intent, not the OS grant. An OS grant the
    /// user turned off here must show as off or they cannot stop the buddy
    /// using it.
    #[test]
    fn rows_report_wanted_capabilities() {
        #[cfg(target_os = "linux")]
        {
            let rows = rows(|id| id == CapabilityId::PortalScreenCast);
            assert!(rows[0].granted);
            assert!(!rows[1].granted);
        }
        #[cfg(not(target_os = "linux"))]
        {
            let rows = rows(|id| id == CapabilityId::Accessibility);
            assert!(rows[0].granted);
            assert!(!rows[1].granted);
        }
    }

    /// Flipping on is what decision 9 allows: the system prompt at that
    /// moment, never at launch.
    #[test]
    fn enabling_an_ungranted_capability_prompts() {
        #[cfg(target_os = "linux")]
        {
            let probe = Fake::granting(&[]);
            enable(CapabilityId::PortalScreenCast, &probe);
            assert_eq!(
                *probe.prompted.lock().expect("prompt log"),
                [CapabilityId::PortalScreenCast]
            );
        }
        #[cfg(not(target_os = "linux"))]
        {
            let probe = Fake::granting(&[]);
            enable(CapabilityId::ScreenRecording, &probe);
            assert_eq!(
                *probe.prompted.lock().expect("prompt log"),
                [CapabilityId::ScreenRecording]
            );
        }
    }

    /// Already granted: a second prompt is noise, and on macOS can look
    /// like the app is asking again for something the user already gave.
    #[test]
    fn enabling_a_granted_capability_does_not_prompt() {
        #[cfg(target_os = "linux")]
        {
            let probe = Fake::granting(&[CapabilityId::PortalScreenCast]);
            enable(CapabilityId::PortalScreenCast, &probe);
            assert!(probe.prompted.lock().expect("prompt log").is_empty());
        }
        #[cfg(not(target_os = "linux"))]
        {
            let probe = Fake::granting(&[CapabilityId::Accessibility]);
            enable(CapabilityId::Accessibility, &probe);
            assert!(probe.prompted.lock().expect("prompt log").is_empty());
        }
    }

    /// A `cargo run` from Cursor is listed as Cursor, not ai-buddy. The
    /// hint has to carry that name or the Accessibility list is a guessing game.
    #[test]
    #[cfg(target_os = "macos")]
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

    /// The walk in `bundled_ancestor_name` is 24 hops and only pays past the
    /// first: `zsh -> Cursor Helper -> Cursor` needs two. A wrong `proc_pidinfo`
    /// flavor writes nothing; the short-write guard reads that as "no parent".
    #[test]
    #[cfg(target_os = "macos")]
    fn the_parent_walk_climbs_past_the_first_hop() {
        let mut pid = std::process::id() as i32;
        assert_eq!(
            macos::parent_pid(pid),
            Some(std::os::unix::process::parent_id() as i32),
            "the first hop has to be the parent the kernel reports"
        );

        let mut chain = vec![pid];
        while let Some(parent) = macos::parent_pid(pid) {
            pid = parent;
            chain.push(pid);
            if chain.len() > 24 {
                break;
            }
        }
        assert!(
            chain.len() > 2,
            "the walk stopped at {chain:?}; it has to reach a grandparent"
        );
    }

    #[test]
    fn process_listed_as_is_not_empty() {
        assert!(!process_listed_as().is_empty());
    }

    /// Pins the assembled Privacy pane sentence, which nothing else covers.
    ///
    /// The name is a sample, not a discovered value. `pane_intro` formats
    /// whatever it is handed, so this passes wherever it runs. What TCC really
    /// lists comes from `process_listed_as`, and `process_listed_as_is_not_empty`
    /// is what covers that. `settings::form::Live::current` builds the Privacy
    /// section comment from this sentence (#871).
    #[test]
    #[cfg(target_os = "macos")]
    fn macos_pane_intro_wraps_the_listed_name_in_the_pane_copy() {
        assert_eq!(
            pane_intro("Terminal"),
            "Checking a box asks macOS for the permission. macOS lists this app as Terminal, under Privacy & Security."
        );
    }

    /// Linux rows are portal capabilities, not TCC. The catalog is the real
    /// rows settings shows, not a mock (#250).
    #[test]
    #[cfg(target_os = "linux")]
    fn linux_rows_are_portal_capabilities() {
        let rows = rows(|_| false);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, CapabilityId::PortalScreenCast);
        assert!(
            !rows[0].costs.contains("macOS"),
            "Linux rows must not mention macOS, got {:?}",
            rows[0].costs
        );
        assert!(
            !rows[0].costs.contains("Accessibility"),
            "Linux rows must not mention Accessibility, got {:?}",
            rows[0].costs
        );
    }

    #[test]
    fn the_null_probe_grants_nothing_and_prompts_for_nothing() {
        let probe = Null;
        #[cfg(target_os = "linux")]
        {
            assert!(!probe.granted(CapabilityId::PortalScreenCast));
            probe.prompt(CapabilityId::PortalScreenCast);
            assert!(!probe.granted(CapabilityId::PortalScreenCast));
        }
        #[cfg(not(target_os = "linux"))]
        {
            assert!(!probe.granted(CapabilityId::Accessibility));
            assert!(!probe.granted(CapabilityId::ScreenRecording));
            probe.prompt(CapabilityId::ScreenRecording);
            assert!(!probe.granted(CapabilityId::ScreenRecording));
        }
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn linux_pane_intro_is_portal_specific_not_tcc() {
        let prose = linux_pane_intro();
        assert!(!prose.is_empty(), "Linux prose must not be empty");
        assert!(
            prose.contains("portal"),
            "Linux prose must mention the portal, got {prose:?}"
        );
        assert!(
            prose.contains("window") || prose.contains("Window"),
            "Linux prose must name what is readable without a grant, got {prose:?}"
        );
        assert!(
            !prose.contains("Accessibility"),
            "Linux prose must not mention TCC Accessibility, got {prose:?}"
        );
        assert!(
            !prose.contains("Screen Recording"),
            "Linux prose must not mention TCC Screen Recording, got {prose:?}"
        );
        assert!(
            !prose.contains("Privacy & Security"),
            "Linux prose must not mention macOS Privacy & Security, got {prose:?}"
        );
        assert!(
            !prose.contains("macOS"),
            "Linux prose must not mention macOS, got {prose:?}"
        );
    }

    /// Linux uses the portal probe, not Null. Null is tests-only or
    /// platforms with no consent system. First run grants nothing.
    #[test]
    #[cfg(target_os = "linux")]
    fn linux_live_probe_is_portal_not_null() {
        let probe = live();
        assert!(!probe.granted(CapabilityId::PortalScreenCast));
    }

    /// The fake portal grants what it was told to grant. Settings can flip
    /// rows on and off, and the consent model stays testable without a live
    /// portal or a window server.
    #[test]
    #[cfg(target_os = "linux")]
    fn fake_portal_grants_what_it_was_constructed_with() {
        let granted = Fake::granting(&[CapabilityId::PortalScreenCast]);
        assert!(granted.granted(CapabilityId::PortalScreenCast));

        let nothing = Fake::granting(&[]);
        assert!(!nothing.granted(CapabilityId::PortalScreenCast));
    }

    #[test]
    #[cfg(target_os = "windows")]
    fn windows_hint_names_process_without_macos_vocabulary() {
        let hint = listed_under_hint("Cursor");
        assert!(
            hint.contains("Cursor"),
            "the user has to see the process name Privacy will list, got {hint:?}"
        );
        assert!(
            !hint.contains("macOS"),
            "Windows hint must not mention macOS, got {hint:?}"
        );
        assert!(
            !hint.contains("Privacy & Security"),
            "Windows hint must not mention macOS Privacy & Security pane, got {hint:?}"
        );
        assert!(
            !hint.contains("TCC"),
            "Windows hint must not mention TCC, got {hint:?}"
        );
        assert!(
            !hint.contains("Accessibility"),
            "Windows hint must not mention macOS Accessibility, got {hint:?}"
        );
    }

    #[test]
    #[cfg(target_os = "windows")]
    fn windows_pane_intro_is_not_macos_specific() {
        let intro = pane_intro(&process_listed_as());
        assert!(!intro.is_empty(), "Windows intro must not be empty");
        assert!(
            intro.contains(&process_listed_as()),
            "Windows intro must name the process, got {intro:?}"
        );
        assert!(
            !intro.contains("macOS"),
            "Windows intro must not mention macOS, got {intro:?}"
        );
        assert!(
            !intro.contains("Privacy & Security"),
            "Windows intro must not mention macOS Privacy & Security, got {intro:?}"
        );
        assert!(
            !intro.contains("TCC"),
            "Windows intro must not mention TCC, got {intro:?}"
        );
    }

    #[test]
    #[cfg(target_os = "windows")]
    fn windows_toolchain_classification() {
        use crate::consent::windows::is_toolchain;

        assert!(is_toolchain("cargo"));
        assert!(is_toolchain("rustc"));
        assert!(is_toolchain("rustup"));
        assert!(is_toolchain("rust-analyzer"));
        assert!(is_toolchain("rls"));
        assert!(is_toolchain("Cargo")); // case insensitive

        assert!(!is_toolchain("Cursor"));
        assert!(!is_toolchain("Code"));
        assert!(!is_toolchain("WindowsTerminal"));
        assert!(!is_toolchain("powershell"));
        assert!(!is_toolchain("pwsh"));
        assert!(!is_toolchain("cmd"));
        assert!(!is_toolchain("ai-buddy"));
    }
}
