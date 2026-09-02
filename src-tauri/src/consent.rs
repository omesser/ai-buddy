//! Consented capabilities settings can offer.
//!
//! DESIGN.md decision 9: first run grants nothing, and the app never prompts
//! at launch. The upgrade path is settings (#148): this catalog names each
//! grant, what it buys, what it costs, and the system prompt fires only when
//! the user flips one on.

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
pub struct Null;

#[cfg(target_os = "macos")]
struct Macos;

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
}

pub fn rows(probe: &dyn Probe) -> Vec<ConsentRow> {
    CAPABILITIES
        .iter()
        .map(|cap| ConsentRow {
            id: cap.id,
            title: cap.title,
            buys: cap.buys,
            costs: cap.costs,
            granted: probe.granted(cap.id),
        })
        .collect()
}

pub fn enable(id: CapabilityId, probe: &dyn Probe) {
    if !probe.granted(id) {
        probe.prompt(id);
    }
}

pub fn capability_from_name(name: &str) -> Option<CapabilityId> {
    match name {
        "accessibility" => Some(CapabilityId::Accessibility),
        "screen_recording" => Some(CapabilityId::ScreenRecording),
        _ => None,
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
        let rows = rows(&Null);
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

    /// A grant the OS already holds must show as on, or the checkbox lies
    /// and flipping it would prompt for something already given.
    #[test]
    fn rows_report_the_live_grants() {
        let probe = Fake::granting(&[CapabilityId::Accessibility]);
        let rows = rows(&probe);
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
}
