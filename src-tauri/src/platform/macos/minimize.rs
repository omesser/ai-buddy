//! Telling the Shell when AppKit minimizes a window.
//!
//! Tauri has no minimize event. Its focus and resize events stand in for one,
//! but AppKit sends neither when the window it minimizes is not focused, as
//! when an Accessibility client minimizes Chat behind another app.

use std::ptr::NonNull;
use std::sync::Arc;

use block2::RcBlock;
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObjectProtocol, ProtocolObject};
use objc2_app_kit::{NSWindowDidDeminiaturizeNotification, NSWindowDidMiniaturizeNotification};
use objc2_foundation::{NSNotification, NSNotificationCenter, NSNotificationName};

/// One window's miniaturize and deminiaturize observers. Dropping it removes both.
pub struct MinimizeObserver([Retained<ProtocolObject<dyn NSObjectProtocol>>; 2]);

// SAFETY: the tokens only go to `removeObserver`, which is thread-safe, and
// the callback their blocks hold is `Send + Sync`.
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
/// `minimized(false)` when it comes back.
pub fn observe_minimize(
    window: &tauri::WebviewWindow,
    minimized: impl Fn(bool) + Send + Sync + 'static,
) -> Result<MinimizeObserver, String> {
    let ptr = window
        .ns_window()
        .map_err(|e| format!("window has no native handle: {e}"))? as *mut AnyObject;
    // SAFETY: Tauri hands us a live NSWindow for a window it is still holding.
    Ok(observe(unsafe { &*ptr }, minimized))
}

fn observe(
    object: &AnyObject,
    minimized: impl Fn(bool) + Send + Sync + 'static,
) -> MinimizeObserver {
    let center = NSNotificationCenter::defaultCenter();
    let minimized = Arc::new(minimized);
    let on = |name: &NSNotificationName, state: bool| {
        let minimized = Arc::clone(&minimized);
        let block = RcBlock::new(move |_: NonNull<NSNotification>| minimized(state));
        // SAFETY: with no queue the block runs on the posting thread, and
        // everything it captures is `Send + Sync`.
        unsafe {
            center.addObserverForName_object_queue_usingBlock(
                Some(name),
                Some(object),
                None,
                &block,
            )
        }
    };
    let (gone, back) = names();
    MinimizeObserver([on(gone, true), on(back, false)])
}

/// The miniaturize and deminiaturize notification names.
fn names() -> (&'static NSNotificationName, &'static NSNotificationName) {
    // SAFETY: AppKit defines both names for the life of the process.
    unsafe {
        (
            NSWindowDidMiniaturizeNotification,
            NSWindowDidDeminiaturizeNotification,
        )
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

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
        let seen = Arc::new(Mutex::new(Vec::new()));
        let observer = observe(&window, {
            let seen = Arc::clone(&seen);
            move |minimized| seen.lock().unwrap().push(minimized)
        });
        let (gone, back) = names();

        post(gone, &window);
        post(gone, &other);
        post(back, &window);
        drop(observer);
        post(gone, &window);

        assert_eq!(*seen.lock().unwrap(), vec![true, false]);
    }
}
