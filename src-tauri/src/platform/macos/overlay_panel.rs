//! Turning a Tauri window into a non-activating overlay panel.

use std::sync::OnceLock;

use objc2::runtime::{AnyClass, AnyObject, Bool, ClassBuilder, Sel};
use objc2::{msg_send, sel};
use objc2_app_kit::{
    NSFloatingWindowLevel, NSWindowCollectionBehavior, NSWindowLevel, NSWindowSharingType,
    NSWindowStyleMask,
};

/// An `NSPanel` subclass that refuses to become the key or main window, so
/// the sprite can be clicked while the user keeps typing into whatever was
/// already frontmost.
fn overlay_panel_class() -> &'static AnyClass {
    static CLASS: OnceLock<&'static AnyClass> = OnceLock::new();

    CLASS.get_or_init(|| {
        let superclass = AnyClass::get(c"NSPanel").expect("AppKit always defines NSPanel");
        let mut builder = ClassBuilder::new(c"AiBuddyOverlayPanel", superclass)
            .expect("the class name is ours and registered once");
        // Tauri's window is tao's `TaoWindow`, an NSWindow plus this BOOL. Where
        // NSWindow's ivars end 8-aligned the BOOL grows the instance, so the swap
        // needs the same ivar to keep sizes equal, and tao still reads it by name.
        builder.add_ivar::<Bool>(c"focusable");

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
/// across Spaces and stays out of the application switcher. Capturable by
/// default (ADR-0024); Presence or `AI_BUDDY_CAPTURABLE=0` excludes it.
pub fn configure_overlay(window: &tauri::WebviewWindow) -> Result<(), String> {
    let ptr = window
        .ns_window()
        .map_err(|e| format!("overlay has no native window handle: {e}"))?
        as *mut AnyObject;

    // SAFETY: Tauri hands us a live NSWindow for a window it is still holding.
    let ns_window = unsafe { &*ptr };

    // Re-class to our NSPanel subclass: NSPanel adds no ivars beyond NSWindow,
    // so with tao's ivar mirrored only the method table moves.
    let (from, to) = (ns_window.class(), overlay_panel_class());
    if from.instance_size() != to.instance_size() {
        return Err(format!(
            "cannot re-class {} ({} bytes) as {} ({} bytes)",
            from.name().to_string_lossy(),
            from.instance_size(),
            to.name().to_string_lossy(),
            to.instance_size()
        ));
    }
    // SAFETY: the new class is an NSPanel, hence an NSWindow, of the same size;
    // toolkit messages stay valid.
    unsafe { AnyObject::set_class(ns_window, to) };

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
        let _: () = msg_send![ns_window, setSharingType: sharing_type()];
    }

    Ok(())
}

/// Whether this run allows itself to be captured. See `configure_overlay`.
fn sharing_type() -> NSWindowSharingType {
    if crate::dev_flags::CAPTURABLE.is_on() {
        NSWindowSharingType::ReadOnly
    } else {
        NSWindowSharingType::None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The shape `tao` registers as `TaoWindow`: an NSWindow plus one BOOL.
    fn tao_window_shape() -> &'static AnyClass {
        let superclass = AnyClass::get(c"NSWindow").expect("AppKit always defines NSWindow");
        let mut builder = ClassBuilder::new(c"AiBuddyTaoWindowShape", superclass)
            .expect("registered once per test binary");
        builder.add_ivar::<Bool>(c"focusable");
        builder.register()
    }

    #[test]
    fn the_panel_keeps_the_tao_window_layout_it_replaces() {
        let tao = tao_window_shape();
        let panel = overlay_panel_class();

        let focusable = panel
            .instance_variable(c"focusable")
            .expect("tao still reads its focusable ivar after the swap");
        let tao_focusable = tao.instance_variable(c"focusable").unwrap();
        assert_eq!(focusable.offset(), tao_focusable.offset());
        assert_eq!(focusable.type_encoding(), tao_focusable.type_encoding());
        assert_eq!(panel.instance_size(), tao.instance_size());
    }
}
