//! Ctrl+C is delivered to every process attached to this console.
//!
//! `SetConsoleCtrlHandler(null, true)` makes this process ignore Ctrl+C, and
//! a child spawned while that is set inherits the ignore. WebView2's browser
//! process is such a child. Its default handler would `ExitProcess` off the
//! UI thread, and Chromium would then fail `UnregisterClass` for
//! `Chrome_WidgetWin_0` with ERROR_CLASS_DOES_NOT_EXIST (1411).
//!
//! Existing children keep the bit when this process turns Ctrl+C back on, so
//! the handler installed afterwards is the host's alone.

use windows_sys::Win32::Foundation::{FALSE, TRUE};
use windows_sys::Win32::System::Console::SetConsoleCtrlHandler;

// `pub` so `windows` can re-export these. `pub(super)` is only visible inside
// `windows`, and rustc rejects a re-export wider than the item.
/// Children spawned while this is set inherit "ignore Ctrl+C".
pub fn suppress_ctrl_c_for_children() -> bool {
    // SAFETY: a null handler with TRUE is the documented ignore switch, not
    // a function pointer we call.
    unsafe { SetConsoleCtrlHandler(None, TRUE) != FALSE }
}

/// This process receives Ctrl+C again. Already-spawned children do not.
pub fn restore_ctrl_c() -> bool {
    // SAFETY: null with FALSE clears the ignore bit set above. No pointer.
    unsafe { SetConsoleCtrlHandler(None, FALSE) != FALSE }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The ignore bit is process-wide. Put it back even when an assert fails,
    /// or a later test in this process never sees Ctrl+C.
    struct Restore;

    impl Drop for Restore {
        fn drop(&mut self) {
            let _ = restore_ctrl_c();
        }
    }

    #[test]
    fn spawned_children_can_be_kept_off_ctrl_c_and_this_process_turned_back_on() {
        let _restore = Restore;
        assert!(
            suppress_ctrl_c_for_children(),
            "SetConsoleCtrlHandler(null, true) is what a WebView2 child inherits"
        );
        assert!(
            restore_ctrl_c(),
            "the host has to receive Ctrl+C again or the quit handler never runs"
        );
    }
}
