//! Menu bar tray placement and icon sizing.
//!
//! tray-icon hardcodes 18pt icons and macOS spawns new status items at the
//! left of the cluster, under the notch on crowded bars. These hooks run
//! around `tray::install` so the icon is findable and readable.

use objc2_app_kit::NSStatusItem;
use objc2_foundation::{ns_string, MainThreadMarker, NSSize, NSUserDefaults};

/// Standard menu bar icon height. tray-icon uses 18pt; 22pt matches what
/// users expect from other menu bar apps.
const MENU_BAR_ICON_HEIGHT: f64 = 22.0;

/// Seed a rightward slot before the status item exists.
///
/// macOS reads `NSStatusItem Preferred Position Item-N` from NSUserDefaults
/// only when the item is first created. Lower values sit further right, toward
/// the clock. We write once — if the key is already present, the user has
/// Cmd-dragged and that choice wins.
pub fn seed_status_item_position() {
    let _mtm = MainThreadMarker::new().expect("tray setup runs on the main thread");
    let defaults = NSUserDefaults::standardUserDefaults();
    let key = ns_string!("NSStatusItem Preferred Position Item-0");
    if defaults.objectForKey(key).is_none() {
        defaults.setFloat_forKey(5.0, key);
    }
}

/// Resize the tray icon to the standard menu bar height after install.
pub fn tune_tray_icon(tray: &tauri::tray::TrayIcon) -> Result<(), tauri::Error> {
    tray.with_inner_tray_icon(|inner| {
        let mtm = MainThreadMarker::new().expect("tray icon lives on the main thread");
        let Some(ns_status_item) = inner.ns_status_item() else {
            return;
        };
        resize_status_item_icon(&ns_status_item, mtm);
    })
}

fn resize_status_item_icon(ns_status_item: &NSStatusItem, mtm: MainThreadMarker) {
    let Some(button) = ns_status_item.button(mtm) else {
        return;
    };
    let Some(image) = button.image() else {
        return;
    };
    let size = image.size();
    if size.height <= 0.0 {
        return;
    }
    let width = size.width * (MENU_BAR_ICON_HEIGHT / size.height);
    image.setSize(NSSize::new(width, MENU_BAR_ICON_HEIGHT));
}
