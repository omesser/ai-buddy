const WS_VISIBLE: i32 = 0x1000_0000;
const WS_EX_TOOLWINDOW: i32 = 0x0000_0080;
const WS_EX_TOPMOST: i32 = 0x0000_0008;
const WS_EX_LAYERED: i32 = 0x0008_0000;
const WS_EX_NOACTIVATE: i32 = 0x0800_0000;

pub(super) fn perch_candidate(is_window_visible: bool, style: i32, ex_style: i32) -> bool {
    is_window_visible && (style & WS_VISIBLE) != 0 && (ex_style & WS_EX_TOOLWINDOW) == 0
}

/// Drop our overlay hwnds. A style rewrite can clear the tool-window bit,
/// and that window then covers the display it was built on.
pub(super) fn counts_for_desktop(
    is_window_visible: bool,
    style: i32,
    ex_style: i32,
    hwnd: u64,
    overlays: &[u64],
) -> bool {
    perch_candidate(is_window_visible, style, ex_style) && !overlays.contains(&hwnd)
}

/// Put back the tool-window bits a style rewrite drops.
/// `WS_EX_TRANSPARENT` stays as the caller left it.
pub(super) fn restore_overlay_exstyle(ex_style: i32) -> i32 {
    ex_style | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_TOPMOST | WS_EX_LAYERED
}

#[cfg(test)]
mod tests {
    use super::*;

    const WS_EX_TOPMOST: i32 = 0x0000_0008;
    const WS_EX_TRANSPARENT: i32 = 0x0000_0020;
    const WS_EX_APPWINDOW: i32 = 0x0004_0000;
    const WS_EX_LAYERED: i32 = 0x0008_0000;
    const WS_EX_NOACTIVATE: i32 = 0x0800_0000;

    #[test]
    fn a_chat_window_is_a_perch() {
        assert!(perch_candidate(true, WS_VISIBLE, WS_EX_APPWINDOW));
    }

    #[test]
    fn settings_in_the_topmost_band_is_still_a_perch() {
        assert!(perch_candidate(true, WS_VISIBLE, WS_EX_TOPMOST));
    }

    #[test]
    fn the_overlay_is_not_a_perch() {
        let ex_style =
            WS_EX_NOACTIVATE | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_TRANSPARENT | WS_EX_LAYERED;
        assert!(!perch_candidate(true, WS_VISIBLE, ex_style));
    }

    /// tao's click-through rewrite replaces the extended style and never sets
    /// `WS_EX_TOOLWINDOW`. The overlay still covers its display, so it must
    /// stay out of the list by hwnd, and the tool-window bit must come back.
    #[test]
    fn a_click_through_rewrite_keeps_the_overlay_out_of_the_window_list() {
        let rewritten = WS_EX_TOPMOST | WS_EX_TRANSPARENT | WS_EX_LAYERED;
        let overlay = 0x1000_u64;
        let chat = 0x2000_u64;

        assert!(
            !counts_for_desktop(true, WS_VISIBLE, rewritten, overlay, &[overlay]),
            "the display-sized overlay is not an application window"
        );
        assert!(
            counts_for_desktop(true, WS_VISIBLE, rewritten, chat, &[overlay]),
            "another topmost window is still listed"
        );

        let restored = restore_overlay_exstyle(rewritten);
        assert!(
            !perch_candidate(true, WS_VISIBLE, restored),
            "the tool-window bit is back"
        );
        assert_eq!(restored & WS_EX_TRANSPARENT, WS_EX_TRANSPARENT);

        let hovering = restore_overlay_exstyle(WS_EX_TOPMOST);
        assert!(!perch_candidate(true, WS_VISIBLE, hovering));
        assert_eq!(hovering & WS_EX_TRANSPARENT, 0);
    }

    #[test]
    fn a_tool_window_is_not_a_perch() {
        assert!(!perch_candidate(true, WS_VISIBLE, WS_EX_TOOLWINDOW));
    }

    #[test]
    fn a_hidden_window_is_not_a_perch() {
        assert!(!perch_candidate(false, WS_VISIBLE, WS_EX_APPWINDOW));
        assert!(!perch_candidate(true, 0, WS_EX_APPWINDOW));
    }

    #[cfg(windows)]
    #[test]
    fn style_bits_are_the_win32_ones() {
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            WS_EX_APPWINDOW as WIN32_APPWINDOW, WS_EX_LAYERED as WIN32_LAYERED,
            WS_EX_NOACTIVATE as WIN32_NOACTIVATE, WS_EX_TOOLWINDOW as WIN32_TOOLWINDOW,
            WS_EX_TOPMOST as WIN32_TOPMOST, WS_EX_TRANSPARENT as WIN32_TRANSPARENT,
            WS_VISIBLE as WIN32_VISIBLE,
        };
        assert_eq!(WS_VISIBLE, WIN32_VISIBLE as i32);
        assert_eq!(WS_EX_TOPMOST, WIN32_TOPMOST as i32);
        assert_eq!(WS_EX_TRANSPARENT, WIN32_TRANSPARENT as i32);
        assert_eq!(WS_EX_TOOLWINDOW, WIN32_TOOLWINDOW as i32);
        assert_eq!(WS_EX_APPWINDOW, WIN32_APPWINDOW as i32);
        assert_eq!(WS_EX_LAYERED, WIN32_LAYERED as i32);
        assert_eq!(WS_EX_NOACTIVATE, WIN32_NOACTIVATE as i32);
    }
}
