//! Which Windows top-level windows may become Perches.
//!
//! `windows::window_source` reads the style bits and whether the window
//! belongs to this process. This module decides. It is pure so the rule runs
//! under `cargo test` on every host, not only on a Windows desktop.

/// `WS_VISIBLE`.
const WS_VISIBLE: i32 = 0x1000_0000;
/// `WS_EX_TOOLWINDOW`. `configure_overlay` sets it on the overlay.
const WS_EX_TOOLWINDOW: i32 = 0x0000_0080;

/// Whether this top-level window may be a Perch. Same process does not
/// exclude it: Chat and Settings are this process. The overlay is a tool
/// window, and that bit already keeps it out.
pub(super) fn perch_candidate(
    is_window_visible: bool,
    style: i32,
    ex_style: i32,
    own_process: bool,
) -> bool {
    // Process identity is not a reason to drop the window.
    let _ = own_process;
    is_window_visible && (style & WS_VISIBLE) != 0 && (ex_style & WS_EX_TOOLWINDOW) == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Summon opens Chat in this process. It is an ordinary window: visible,
    /// `WS_VISIBLE`, `WS_EX_APPWINDOW`, not a tool window.
    #[test]
    fn a_chat_window_of_this_process_is_a_perch() {
        assert!(
            perch_candidate(true, 0x1000_0000, 0x0004_0000, true),
            "Chat is a Perch, the same as any other ordinary window"
        );
    }

    /// Settings is raised into the topmost band and stays an ordinary window
    /// of this process. Topmost is not the overlay.
    #[test]
    fn settings_in_the_topmost_band_is_still_a_perch() {
        assert!(perch_candidate(true, 0x1000_0000, 0x0000_0008, true));
    }

    /// A Notepad window: visible, ordinary, another process.
    #[test]
    fn another_applications_window_is_a_perch() {
        assert!(perch_candidate(true, 0x1000_0000, 0x0004_0000, false));
    }

    /// The overlay is this process and a tool window. `configure_overlay`
    /// sets `WS_EX_TOOLWINDOW` with topmost, no-activate, layered, and
    /// click-through. The tool-window bit is what keeps it out.
    #[test]
    fn the_overlay_is_not_a_perch() {
        let ex_style = 0x0000_0080 | 0x0000_0008 | 0x0800_0000 | 0x0008_0000 | 0x0000_0020;
        assert!(!perch_candidate(true, 0x1000_0000, ex_style, true));
    }

    /// Tool windows were already dropped, including ones from other processes.
    #[test]
    fn a_tool_window_is_not_a_perch() {
        assert!(!perch_candidate(true, 0x1000_0000, 0x0000_0080, false));
    }

    /// `IsWindowVisible` and the `WS_VISIBLE` bit are both gates. Either one
    /// missing means the window is not on screen.
    #[test]
    fn a_hidden_window_is_not_a_perch() {
        assert!(!perch_candidate(false, 0x1000_0000, 0x0004_0000, false));
        assert!(!perch_candidate(true, 0, 0x0004_0000, false));
    }

    /// The literals above are the Win32 constants, not private aliases.
    #[cfg(windows)]
    #[test]
    fn style_bits_are_the_win32_ones() {
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            WS_EX_TOOLWINDOW as WIN32_TOOLWINDOW, WS_VISIBLE as WIN32_VISIBLE,
        };
        assert_eq!(WS_VISIBLE, WIN32_VISIBLE as i32);
        assert_eq!(WS_EX_TOOLWINDOW, WIN32_TOOLWINDOW as i32);
    }
}
