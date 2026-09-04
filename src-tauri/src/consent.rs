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

/// The sentence settings prints so the user can find the row the OS will show.
///
/// A `cargo run` binary is neither signed nor sandboxed, so the permission UI
/// attributes the request to whoever launched it — Cursor, Terminal — not to
/// "ai-buddy". A packaged build is listed under its own name. Each platform
/// says this in its own words: naming a pane the user does not have is worse
/// than naming none (#237).
pub fn listed_under_hint(name: &str) -> String {
    #[cfg(target_os = "macos")]
    {
        format!("macOS lists this app as {name}, under Privacy & Security.")
    }
    #[cfg(not(target_os = "macos"))]
    {
        format!("Your desktop will list this app as {name}.")
    }
}

/// The pane copy. The listed name is live: a `cargo run` from Cursor is
/// Cursor, a packaged build is ai-buddy.
pub fn pane_intro(listed_as: &str) -> String {
    let hint = listed_under_hint(listed_as);
    #[cfg(target_os = "macos")]
    {
        format!("Checking a box asks macOS for the permission. {hint}")
    }
    // `live()` is `Null` off macOS: the boxes record intent and no grant is
    // requested. Saying otherwise would promise a prompt that never comes.
    #[cfg(not(target_os = "macos"))]
    {
        format!("Nothing here asks for a permission yet. {hint}")
    }
}

/// The name the OS permission UI will show. Packaged builds are this app;
/// `cargo run` is whatever launched it — the IDE or terminal.
pub fn process_listed_as() -> String {
    #[cfg(target_os = "macos")]
    {
        macos::tcc_list_name()
    }
    #[cfg(target_os = "linux")]
    {
        linux::desktop_list_name()
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        "ai-buddy".into()
    }
}

/// What a Linux desktop will call this process when a portal asks the user.
///
/// Built off Linux too, so the parsing can be tested without a live portal:
/// only the `/proc` and environment reads are Linux-only.
#[cfg(any(target_os = "linux", test))]
mod linux {
    /// Between the buddy and the terminal or IDE sit the build tool and a
    /// shell, and neither is a row anyone will recognise.
    ///
    /// ponytail: a process name, where the portal shows a `.desktop` name. A
    /// GNOME Terminal launch resolves to `gnome-terminal-server` and the
    /// dialog says "Terminal", so the hint can name a row the user cannot
    /// find. No skip list fixes that; the upgrade is asking the desktop which
    /// app owns the pid, which needs a portal we do not yet talk to. Cheaper
    /// and secondary: an unlisted wrapper (tmux, systemd-run) wins over what
    /// launched it, and that one costs a string.
    const PASS_THROUGH: &[&str] = &[
        "cargo", "sh", "bash", "zsh", "fish", "dash", "ksh", "tcsh", "env", "sudo", "su", "login",
    ];

    /// `Name=` out of a `.desktop` entry, which is the string the portal
    /// dialog shows. A localized `Name[de]=` is not ours to choose between.
    pub fn desktop_entry_name(entry: &str) -> Option<String> {
        entry
            .lines()
            .find_map(|line| line.strip_prefix("Name="))
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .map(str::to_owned)
    }

    /// The parent pid from `/proc/<pid>/stat`.
    ///
    /// Read from the last `)` rather than by splitting the line: the second
    /// field is the command in parentheses and may itself hold spaces and
    /// parentheses, so field counting from the left lands on the wrong number.
    pub fn stat_ppid(stat: &str) -> Option<i32> {
        stat.rsplit_once(')')?
            .1
            .split_whitespace()
            .nth(1)?
            .parse()
            .ok()
    }

    /// The command name from `/proc/<pid>/cmdline`. `comm` would be one read
    /// fewer and the kernel truncates it to 15 bytes, which turns
    /// gnome-terminal-server into "gnome-terminal-".
    pub fn arg0_name(cmdline: &str) -> Option<String> {
        let name = cmdline.split('\0').next()?.rsplit('/').next()?;
        (!name.is_empty()).then(|| name.to_owned())
    }

    /// The nearest ancestor a desktop could plausibly have launched.
    pub fn launcher_name(chain: impl IntoIterator<Item = String>) -> Option<String> {
        chain
            .into_iter()
            .find(|name| !PASS_THROUGH.contains(&name.as_str()))
    }

    /// The installed entry's `Name=`, for a packaged build the desktop did
    /// not launch through GLib.
    ///
    /// Without this a `.deb` started from KRunner or by path falls to the
    /// parent walk and answers `plasmashell` or "Unknown" — a wrong name
    /// rather than a missing one. The macOS side takes the same shape, asking
    /// `packaged()` before it walks.
    pub fn installed_entry_name(mut read: impl FnMut(&str) -> Option<String>) -> Option<String> {
        read(&format!("/usr/share/applications/{IDENTIFIER}.desktop"))
            .as_deref()
            .and_then(desktop_entry_name)
    }

    /// The app-id Tauri packages under, so the entry we look for is our own.
    const IDENTIFIER: &str = "dev.omesser.ai-buddy";

    /// The name a desktop permission UI will show, in the order that decides
    /// it. The two environment reads and the file read are arguments so the
    /// whole order is testable off Linux; only the callers touch the world.
    pub fn desktop_list_name_from(
        flatpak_id: Option<String>,
        launched_entry: Option<String>,
        installed_entry: Option<String>,
        ancestor: Option<String>,
    ) -> String {
        flatpak_id
            .or(launched_entry)
            .or(installed_entry)
            .or(ancestor)
            // What a portal shows for a process it cannot attribute to an app.
            .unwrap_or_else(|| "Unknown".into())
    }

    #[cfg(target_os = "linux")]
    pub fn desktop_list_name() -> String {
        desktop_list_name_from(
            flatpak_id(),
            launched_desktop_name(),
            installed_entry_name(|path| std::fs::read_to_string(path).ok()),
            launching_ancestor(),
        )
    }

    /// Inside a Flatpak the portal keys off the app-id, and that is the name
    /// it resolves the dialog's title from.
    #[cfg(target_os = "linux")]
    fn flatpak_id() -> Option<String> {
        std::env::var("FLATPAK_ID").ok().filter(|id| !id.is_empty())
    }

    /// GLib exports this when the desktop launched us from a `.desktop` entry.
    #[cfg(target_os = "linux")]
    fn launched_desktop_name() -> Option<String> {
        let path = std::env::var_os("GIO_LAUNCHED_DESKTOP_FILE")?;
        desktop_entry_name(&std::fs::read_to_string(path).ok()?)
    }

    #[cfg(target_os = "linux")]
    fn launching_ancestor() -> Option<String> {
        let mut pid = stat_ppid(&std::fs::read_to_string("/proc/self/stat").ok()?)?;
        let mut chain = Vec::new();
        // Bounded because a `/proc` walk reads a tree that is moving under it.
        while pid > 1 && chain.len() < 24 {
            let Ok(stat) = std::fs::read_to_string(format!("/proc/{pid}/stat")) else {
                break;
            };
            // A zombie or a kernel thread has no `cmdline`, and non-UTF-8
            // argv reads as none. Step over it: the terminal may be one hop
            // further up, and stopping here would answer "Unknown" instead.
            if let Some(name) = std::fs::read_to_string(format!("/proc/{pid}/cmdline"))
                .ok()
                .as_deref()
                .and_then(arg0_name)
            {
                chain.push(name);
            }
            let Some(parent) = stat_ppid(&stat) else {
                break;
            };
            pid = parent;
        }
        launcher_name(chain)
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
    /// hint has to carry that name or the permission list is a guessing game.
    #[test]
    fn the_grant_hint_names_the_app_the_os_will_list() {
        let hint = listed_under_hint("Cursor");
        assert!(
            hint.contains("Cursor"),
            "the user has to see the row name, got {hint:?}"
        );
        assert!(!listed_under_hint("Terminal").contains("Cursor"));
    }

    /// TCC's pane is where the row is, and the words are macOS's own.
    #[cfg(target_os = "macos")]
    #[test]
    fn the_grant_hint_says_where_to_look_on_macos() {
        let hint = listed_under_hint("Cursor");
        assert!(
            hint.contains("Privacy & Security"),
            "the hint has to say where to look, got {hint:?}"
        );
    }

    /// No desktop has a Privacy & Security pane, and citing one sends the
    /// user hunting for a window that does not exist. #237.
    #[cfg(not(target_os = "macos"))]
    #[test]
    fn the_grant_copy_does_not_cite_macos_off_macos() {
        for copy in [listed_under_hint("Cursor"), pane_intro("Cursor")] {
            assert!(
                !copy.contains("macOS") && !copy.contains("Privacy & Security"),
                "off macOS the copy has to stand on its own, got {copy:?}"
            );
        }
    }

    /// The pane copy has to carry the where-to-look sentence, not just the
    /// name. Asserted here because `form.rs` only checks the name reaches the
    /// section, so nothing else would notice `pane_intro` dropping the hint.
    #[test]
    fn the_pane_copy_carries_the_hint() {
        assert!(pane_intro("Cursor").contains(&listed_under_hint("Cursor")));
    }

    /// The order that decides the name, exercised without a Linux host.
    #[test]
    fn a_flatpak_id_outranks_every_other_answer() {
        assert_eq!(
            linux::desktop_list_name_from(
                Some("dev.omesser.ai-buddy".into()),
                Some("Launched".into()),
                Some("Installed".into()),
                Some("Ancestor".into()),
            ),
            "dev.omesser.ai-buddy"
        );
    }

    /// A packaged build the desktop did not launch through GLib still names
    /// its own entry rather than falling to whatever process is above it.
    #[test]
    fn an_installed_entry_outranks_the_parent_walk() {
        assert_eq!(
            linux::desktop_list_name_from(
                None,
                None,
                Some("ai-buddy".into()),
                Some("plasmashell".into())
            ),
            "ai-buddy"
        );
    }

    #[test]
    fn a_process_no_entry_claims_falls_to_the_walk_then_unknown() {
        assert_eq!(
            linux::desktop_list_name_from(None, None, None, Some("Cursor".into())),
            "Cursor"
        );
        assert_eq!(
            linux::desktop_list_name_from(None, None, None, None),
            "Unknown"
        );
    }

    #[test]
    fn the_installed_entry_is_read_from_our_own_app_id() {
        let mut asked = String::new();
        let name = linux::installed_entry_name(|path| {
            asked = path.to_string();
            Some("[Desktop Entry]\nName=ai-buddy\n".to_string())
        });
        assert_eq!(name.as_deref(), Some("ai-buddy"));
        assert!(
            asked.ends_with("dev.omesser.ai-buddy.desktop"),
            "looked for the wrong entry: {asked}"
        );
    }

    #[test]
    fn process_listed_as_is_not_empty() {
        assert!(!process_listed_as().is_empty());
    }

    /// The portal dialog shows `Name=`, not the app-id or a translation we
    /// have no way to pick between.
    #[test]
    fn a_desktop_entry_yields_its_name() {
        let entry =
            "[Desktop Entry]\nType=Application\nName[de]=Kumpel\nName=ai-buddy\nExec=ai-buddy\n";
        assert_eq!(
            linux::desktop_entry_name(entry).as_deref(),
            Some("ai-buddy")
        );
        assert_eq!(linux::desktop_entry_name("[Desktop Entry]\nName=\n"), None);
    }

    /// The command field is parenthesised and unescaped, so a process named
    /// `foo bar) 9` shifts every field a left-to-right split would count.
    #[test]
    fn the_parent_pid_survives_a_command_full_of_parens() {
        assert_eq!(stat_ppid_of("(cargo)", 4321), Some(4321));
        assert_eq!(stat_ppid_of("(foo bar) 9)", 77), Some(77));
        assert_eq!(linux::stat_ppid("nothing parseable"), None);
    }

    fn stat_ppid_of(comm: &str, ppid: i32) -> Option<i32> {
        linux::stat_ppid(&format!("1234 {comm} S {ppid} 1234 1234 0 -1 4194304"))
    }

    /// `cargo run` puts the build tool and a shell between the buddy and the
    /// terminal, and it is the terminal the desktop would name.
    #[test]
    fn the_launcher_is_the_first_ancestor_that_is_not_plumbing() {
        let chain = ["cargo", "zsh", "gnome-terminal-server", "systemd"];
        assert_eq!(
            linux::launcher_name(chain.map(str::to_owned)).as_deref(),
            Some("gnome-terminal-server")
        );
        assert_eq!(linux::launcher_name(["cargo".to_owned()]), None);
    }

    /// `comm` truncates at 15 bytes, so the name comes from `cmdline`, which
    /// is NUL-separated and holds a path.
    #[test]
    fn the_process_name_is_the_basename_of_argv_zero() {
        assert_eq!(
            linux::arg0_name("/usr/libexec/gnome-terminal-server\0--foo\0").as_deref(),
            Some("gnome-terminal-server")
        );
        assert_eq!(linux::arg0_name(""), None);
    }
}
