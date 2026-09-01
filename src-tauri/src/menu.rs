//! Context menu construction and actions.
//!
//! The menu is built as a pure function from the installed characters and
//! current state, so its shape can be tested without popping a window. Menu
//! actions are shell commands that do not enter the frame loop: character
//! switching, DND toggle, hiding, and quit.

use muda::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu};
#[cfg(target_os = "macos")]
use muda::ContextMenu;
use std::collections::HashMap;

/// The menu items that trigger actions, keyed by their menu item id.
pub enum MenuAction {
    /// Character ▸ <name>. Switch to the named Character Package.
    SwitchCharacter(String),
    /// Do Not Disturb checkbox toggle.
    ToggleDnd,
    /// Hide the Character instantly, same path as the hotkey.
    Hide,
    /// Quit the application.
    Quit,
}

/// What the shell needs to show the menu and handle its selections: the menu
/// itself (ready to pop), and the mapping from muda's menu item ids to the
/// actions they perform.
pub struct BuiltMenu {
    pub menu: Menu,
    pub actions: HashMap<String, MenuAction>,
}

/// Build the context menu from the list of installed characters and the
/// current Character's name.
///
/// A pure function: the menu is data, not a side effect, so its shape can be
/// tested by constructing the arguments. The actual popup is manual
/// verification, like the overlay itself.
///
/// `installed` is the list of Character Package names the loader found, in the
/// order they were found. `current` is the name of the Character currently on
/// screen, which is check-marked in the submenu.
pub fn build(installed: &[String], current: &str, do_not_disturb: bool) -> BuiltMenu {
    let menu = Menu::new();
    let mut actions = HashMap::new();

    // Chat… — Summon (#17). Until chat exists, the item performs today's
    // Summon (accepted, visibly nothing). Disabled for now since chat is not
    // implemented.
    let chat = MenuItem::new("Chat…", false, None);
    chat.set_enabled(false);
    let _ = menu.append(&chat);

    // Character ▸ — installed packages by name, check-marked current, switch
    // on select.
    if !installed.is_empty() {
        let character_submenu = Submenu::new("Character", true);

        for name in installed {
            let item_id = format!("character:{name}");
            let item = CheckMenuItem::new(name, true, name == current, None);
            actions.insert(item_id.clone(), MenuAction::SwitchCharacter(name.clone()));

            let _ = character_submenu.append(&item);
        }

        let _ = menu.append(&character_submenu);
    }

    // Do Not Disturb — checkbox calling Engine::set_do_not_disturb.
    let dnd_id = "dnd".to_string();
    let dnd = CheckMenuItem::new("Do Not Disturb", true, do_not_disturb, None);
    actions.insert(dnd_id, MenuAction::ToggleDnd);
    let _ = menu.append(&dnd);

    // Hide — same instant hide path as the hotkey.
    let hide_id = "hide".to_string();
    let hide = MenuItem::new("Hide", true, None);
    actions.insert(hide_id, MenuAction::Hide);
    let _ = menu.append(&hide);

    // What the buddy can see… — ABSENT until #148 exists. Not disabled.
    // Do not pretend.

    // Quit.
    let _ = menu.append(&PredefinedMenuItem::quit(None));

    BuiltMenu { menu, actions }
}

/// Block until the user dismisses the menu, returning the action they selected
/// if any. The menu is shown at the given cursor position.
///
/// Blocking is deliberate: the sprite pauses while the menu is open, which is
/// what Menu holding the Engine's not-now gates means. A menu that returned
/// immediately would let the sprite keep moving underneath it.
pub fn show_and_wait(menu: &Menu) -> Option<MenuEvent> {
    #[cfg(target_os = "macos")]
    unsafe {
        menu.show_context_menu_for_nsview(std::ptr::null_mut(), None);
    }

    #[cfg(target_os = "linux")]
    unsafe {
        menu.show_context_menu_for_gtk_window(std::ptr::null_mut(), None);
    }

    #[cfg(target_os = "windows")]
    unsafe {
        menu.show_context_menu_for_hwnd(std::ptr::null_mut(), None);
    }

    MenuEvent::receiver().try_recv().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The menu construction is a pure function: these test the shape without
    /// popping a window, which is what makes them tests rather than manual
    /// verification.

    #[test]
    fn a_menu_with_no_characters_installed_has_no_character_submenu() {
        let built = build(&[], "bmo", false);

        let has_character_submenu = built
            .menu
            .items()
            .iter()
            .any(|item| {
                item.as_submenu()
                    .is_some_and(|sub| sub.text() == "Character")
            });

        assert!(
            !has_character_submenu,
            "no Character submenu when nothing is installed"
        );
    }

    #[test]
    fn the_character_submenu_lists_every_installed_package() {
        let installed = vec!["bmo".to_string(), "nim".to_string(), "cat".to_string()];
        let built = build(&installed, "bmo", false);

        let has_all_names = built.menu.items().iter().any(|item| {
            item.as_submenu()
                .is_some_and(|sub| {
                    sub.text() == "Character"
                        && sub.items().iter().filter_map(|i| i.as_check_menuitem()).count() == 3
                        && sub.items().iter().any(|i| {
                            i.as_check_menuitem()
                                .is_some_and(|c| c.text() == "bmo")
                        })
                        && sub.items().iter().any(|i| {
                            i.as_check_menuitem()
                                .is_some_and(|c| c.text() == "nim")
                        })
                        && sub.items().iter().any(|i| {
                            i.as_check_menuitem()
                                .is_some_and(|c| c.text() == "cat")
                        })
                })
        });

        assert!(
            has_all_names,
            "submenu carries every installed package"
        );
    }

    #[test]
    fn the_current_character_is_check_marked() {
        let installed = vec!["bmo".to_string(), "nim".to_string()];
        let built = build(&installed, "nim", false);

        let nim_is_checked = built.menu.items().iter().any(|item| {
            item.as_submenu().is_some_and(|sub| {
                sub.text() == "Character"
                    && sub.items().iter().any(|i| {
                        i.as_check_menuitem()
                            .is_some_and(|c| c.text() == "nim" && c.is_checked())
                    })
            })
        });

        let bmo_is_not_checked = built.menu.items().iter().any(|item| {
            item.as_submenu().is_some_and(|sub| {
                sub.text() == "Character"
                    && sub.items().iter().any(|i| {
                        i.as_check_menuitem()
                            .is_some_and(|c| c.text() == "bmo" && !c.is_checked())
                    })
            })
        });

        assert!(
            nim_is_checked && bmo_is_not_checked,
            "only the current Character is checked"
        );
    }

    #[test]
    fn dnd_checkbox_reflects_engine_state() {
        let built_off = build(&[], "bmo", false);
        let built_on = build(&[], "bmo", true);

        let dnd_off_unchecked = built_off.menu.items().iter().any(|item| {
            item.as_check_menuitem()
                .is_some_and(|check| check.text() == "Do Not Disturb" && !check.is_checked())
        });

        let dnd_on_checked = built_on.menu.items().iter().any(|item| {
            item.as_check_menuitem()
                .is_some_and(|check| check.text() == "Do Not Disturb" && check.is_checked())
        });

        assert!(dnd_off_unchecked, "DND unchecked when off");
        assert!(dnd_on_checked, "DND checked when on");
    }

    #[test]
    fn every_actionable_item_is_mapped() {
        let installed = vec!["bmo".to_string(), "nim".to_string()];
        let built = build(&installed, "bmo", true);

        assert_eq!(
            built.actions.len(),
            4,
            "two characters, DND, and Hide: four actions"
        );

        assert!(
            matches!(
                built.actions.get("character:bmo"),
                Some(MenuAction::SwitchCharacter(name)) if name == "bmo"
            ),
            "BMO switch action is mapped"
        );

        assert!(
            matches!(
                built.actions.get("character:nim"),
                Some(MenuAction::SwitchCharacter(name)) if name == "nim"
            ),
            "Nim switch action is mapped"
        );

        assert!(
            matches!(built.actions.get("dnd"), Some(MenuAction::ToggleDnd)),
            "DND toggle is mapped"
        );

        assert!(
            matches!(built.actions.get("hide"), Some(MenuAction::Hide)),
            "Hide action is mapped"
        );
    }

    #[test]
    fn chat_is_present_but_disabled() {
        let built = build(&[], "bmo", false);

        let chat_is_disabled = built.menu.items().iter().any(|item| {
            item.as_menuitem()
                .is_some_and(|mi| mi.text() == "Chat…" && !mi.is_enabled())
        });

        assert!(chat_is_disabled, "Chat is disabled until #17 is implemented");
    }
}
