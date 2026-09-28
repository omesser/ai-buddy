//! Refcount for "children inherit ignore Ctrl+C".
//!
//! The Win32 switch is a bit, not a counter. A webview built inside the setup
//! hold, and a restore that runs twice, must not clear that bit early.

use std::sync::atomic::{AtomicU32, Ordering};

static DEPTH: AtomicU32 = AtomicU32::new(0);

#[cfg(test)]
static TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// `true` when this hold is the one that should turn the OS bit on.
pub(super) fn enter() -> bool {
    DEPTH.fetch_add(1, Ordering::SeqCst) == 0
}

/// `true` when this release is the one that should turn the OS bit off.
/// A release with nothing held does not wrap.
pub(super) fn exit() -> bool {
    DEPTH
        .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |depth| {
            depth.checked_sub(1)
        })
        .is_ok_and(|previous| previous == 1)
}

#[cfg(test)]
pub(super) fn depth() -> u32 {
    DEPTH.load(Ordering::SeqCst)
}

#[cfg(test)]
pub(super) fn lock_tests() -> std::sync::MutexGuard<'static, ()> {
    TEST_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}
