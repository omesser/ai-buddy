//! The menu bar icon. Same menu as the sprite's right-click.
//!
//! One definition (`menu::describe`) and two entry points, so a row cannot
//! exist on the sprite and not here. WindowPet's tray is the reference for
//! putting settings and quit on a menu bar icon; this is our menu, not theirs.

use crate::menu::{self, MenuDescription};
use tauri::tray::TrayIconBuilder;
use tauri::AppHandle;

/// Put the shared menu on a menu bar icon.
///
/// Left-click opens the same menu as right-click: settings has to be
/// reachable without finding the sprite, and a click that did nothing would
/// look like a broken icon.
pub fn install(
    app: &AppHandle,
    description: &MenuDescription,
) -> Result<tauri::tray::TrayIcon, tauri::Error> {
    let menu = menu::build(app, description)?;
    let mut builder = TrayIconBuilder::new()
        .menu(&menu)
        .show_menu_on_left_click(true)
        .tooltip("ai-buddy");

    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }

    builder.build(app)
}

/// Rebuild the tray menu after a toggle, so the checkboxes match the Engine.
pub fn refresh(
    tray: &tauri::tray::TrayIcon,
    app: &AppHandle,
    description: &MenuDescription,
) -> Result<(), tauri::Error> {
    let menu = menu::build(app, description)?;
    tray.set_menu(Some(menu))
}
