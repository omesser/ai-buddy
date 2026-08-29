//! Turning a Tauri window into a non-activating overlay panel.

use std::sync::OnceLock;

use objc2::runtime::{AnyClass, AnyObject, Bool, ClassBuilder, Sel};
use objc2::{msg_send, sel};
use objc2_app_kit::{
    NSFloatingWindowLevel, NSWindowCollectionBehavior, NSWindowLevel, NSWindowSharingType,
    NSWindowStyleMask,
};

/// An `NSPanel` subclass that refuses to become the key or main window.
///
/// This is the whole reason for the re-classing below. A window that cannot
/// become key cannot take keyboard focus, so the sprite can be clicked while the
/// user keeps typing into whatever was already frontmost.
fn overlay_panel_class() -> &'static AnyClass {
    static CLASS: OnceLock<&'static AnyClass> = OnceLock::new();

    CLASS.get_or_init(|| {
        let superclass = AnyClass::get(c"NSPanel").expect("AppKit always defines NSPanel");
        let mut builder = ClassBuilder::new(c"AiBuddyOverlayPanel", superclass)
            .expect("the class name is ours and registered once");

        extern "C" fn refuse(_this: &AnyObject, _sel: Sel) -> Bool {
            Bool::NO
        }

        // SAFETY: both selectors return BOOL and take no arguments, which is
        // what `refuse` is declared as.
        unsafe {
            builder.add_method(
                sel!(canBecomeKeyWindow),
                refuse as extern "C" fn(_, _) -> Bool,
            );
            builder.add_method(
                sel!(canBecomeMainWindow),
                refuse as extern "C" fn(_, _) -> Bool,
            );
        }

        builder.register()
    })
}

/// Make the overlay a floating, non-activating panel that follows the user
/// across Spaces, stays out of the application switcher, and is never captured.
///
/// Never captured is DESIGN.md decision 8's screen-share rule, answered by the
/// window server instead of by a rule. macOS publishes no way for an app to
/// learn that its screen is being shared — Apple's own guidance is that there
/// is none, and that guessing breaks on every third-party sharing tool — but a
/// window may declare that its content must not be captured at all. So rather
/// than detect a share and hide, the Character is absent from every screen
/// recording, screen share and remote view while its owner keeps it on screen.
pub fn configure_overlay(window: &tauri::WebviewWindow) -> Result<(), String> {
    let ptr = window
        .ns_window()
        .map_err(|e| format!("overlay has no native window handle: {e}"))?
        as *mut AnyObject;

    // SAFETY: Tauri hands us a live NSWindow for a window it is still holding.
    let ns_window = unsafe { &*ptr };

    // Re-class the window to our NSPanel subclass. NSPanel declares no ivars
    // beyond NSWindow, so the instance layout is unchanged and only the method
    // table moves. This is the established way to get panel behaviour out of a
    // window created by someone else's toolkit.
    //
    // SAFETY: the new class is a subclass of NSPanel, which is itself a subclass
    // of NSWindow, so every message the toolkit still sends remains valid.
    unsafe { AnyObject::set_class(ns_window, overlay_panel_class()) };

    let behavior = NSWindowCollectionBehavior::CanJoinAllSpaces
        | NSWindowCollectionBehavior::Stationary
        | NSWindowCollectionBehavior::IgnoresCycle
        | NSWindowCollectionBehavior::FullScreenAuxiliary;

    // SAFETY: all four are plain AppKit setters on NSWindow/NSPanel, called on
    // the main thread from Tauri's setup hook.
    unsafe {
        let style: NSWindowStyleMask = msg_send![ns_window, styleMask];
        let _: () = msg_send![
            ns_window,
            setStyleMask: style | NSWindowStyleMask::NonactivatingPanel
        ];
        let _: () = msg_send![ns_window, setLevel: NSFloatingWindowLevel as NSWindowLevel];
        let _: () = msg_send![ns_window, setCollectionBehavior: behavior];
        let _: () = msg_send![ns_window, setHidesOnDeactivate: false];
        // AppKit warns that an uncapturable window cannot take part in some
        // system services. The overlay uses none of them: it draws a sprite,
        // takes no focus, and prints nothing.
        let _: () = msg_send![ns_window, setSharingType: NSWindowSharingType::None];
    }

    Ok(())
}
