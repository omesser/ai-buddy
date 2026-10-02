//! Telling the Shell when AppKit minimizes a window.
//!
//! Tauri has no minimize event. Its focus and resize events stand in for one,
//! but AppKit sends neither when the window it minimizes is not focused, as
//! when an Accessibility client minimizes Chat behind another app.

use std::ptr::NonNull;
use std::rc::Rc;

use block2::RcBlock;
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObjectProtocol, ProtocolObject};
use objc2_app_kit::{NSWindowDidDeminiaturizeNotification, NSWindowDidMiniaturizeNotification};
use objc2_foundation::{NSNotification, NSNotificationCenter, NSNotificationName};

/// One window's miniaturize and deminiaturize observers. Dropping it removes both.
pub struct MinimizeObserver([Retained<ProtocolObject<dyn NSObjectProtocol>>; 2]);

// SAFETY: `observe_minimize` builds it and the window's event handler drops
// it, both on the main thread, where AppKit delivers window events.
unsafe impl Send for MinimizeObserver {}

impl Drop for MinimizeObserver {
    fn drop(&mut self) {
        let center = NSNotificationCenter::defaultCenter();
        for token in &self.0 {
            // SAFETY: each token is an observer this center handed out.
            unsafe { center.removeObserver(token.as_ref()) };
        }
    }
}

/// Call `minimized(true)` when AppKit minimizes `window`, and
/// `minimized(false)` when it comes back. Main thread only.
pub fn observe_minimize(
    window: &tauri::WebviewWindow,
    minimized: impl Fn(bool) + 'static,
) -> Result<MinimizeObserver, String> {
    let ptr = window
        .ns_window()
        .map_err(|e| format!("window has no native handle: {e}"))? as *mut AnyObject;
    // SAFETY: Tauri hands us a live NSWindow for a window it is still holding.
    Ok(observe(unsafe { &*ptr }, minimized))
}

fn observe(object: &AnyObject, minimized: impl Fn(bool) + 'static) -> MinimizeObserver {
    let center = NSNotificationCenter::defaultCenter();
    let minimized = Rc::new(minimized);
    let on = |name: &NSNotificationName, state: bool| {
        let minimized = Rc::clone(&minimized);
        let block = RcBlock::new(move |_: NonNull<NSNotification>| minimized(state));
        // SAFETY: with no queue the block runs on the posting thread, and
        // AppKit posts window notifications on the main thread.
        unsafe {
            center.addObserverForName_object_queue_usingBlock(
                Some(name),
                Some(object),
                None,
                &block,
            )
        }
    };
    // SAFETY: AppKit defines both names for the life of the process.
    let (gone, back) = unsafe {
        (
            NSWindowDidMiniaturizeNotification,
            NSWindowDidDeminiaturizeNotification,
        )
    };
    MinimizeObserver([on(gone, true), on(back, false)])
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use objc2::runtime::NSObject;

    use super::*;

    fn post(name: &NSNotificationName, object: &AnyObject) {
        let center = NSNotificationCenter::defaultCenter();
        // SAFETY: a plain NSObject stands in for the window; nothing reads it.
        unsafe { center.postNotificationName_object(name, Some(object)) };
    }

    #[test]
    fn only_the_observed_window_reports_and_only_until_dropped() {
        let (window, other) = (NSObject::new(), NSObject::new());
        let seen = Rc::new(RefCell::new(Vec::new()));
        let observer = observe(&window, {
            let seen = Rc::clone(&seen);
            move |minimized| seen.borrow_mut().push(minimized)
        });
        // SAFETY: AppKit defines both names for the life of the process.
        let (gone, back) = unsafe {
            (
                NSWindowDidMiniaturizeNotification,
                NSWindowDidDeminiaturizeNotification,
            )
        };

        post(gone, &window);
        post(gone, &other);
        post(back, &window);
        drop(observer);
        post(gone, &window);

        assert_eq!(*seen.borrow(), vec![true, false]);
    }
}
