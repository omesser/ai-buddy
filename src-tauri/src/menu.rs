//! The context menu: what is in it, and how it reaches the screen.
//!
//! Split in two because the two halves have different constraints. What the
//! menu contains is a description made of owned Strings and bools — no native
//! handles, no platform types, `Send`. The frame-loop thread builds one of
//! those and can be tested doing it on any platform.
//!
//! Turning a description into a menu the window server draws is the other half,
//! and it can only happen on the main thread: the native objects behind it are
//! `Rc`-counted and not `Send`, so a worker thread cannot hold one even for as
//! long as it takes to hand it over. `show` therefore takes a description
//! rather than a menu, and is called from inside `run_on_main_thread`.
//!
//! Selections do not come back from the popup. It returns as soon as the menu
//! is on screen, and the click — if there is one — arrives later on the app's
//! menu event channel. `MenuAction` is looked up there, by id, through the
//! description's action table.

use std::collections::HashMap;

/// What a menu item does when it is chosen.
///
/// Quit is absent: it is a `PredefinedMenuItem`, which the platform handles
/// itself, so nothing here has to recognise its id.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MenuAction {
    /// Character ▸ <name>. Switch to the named Character Package.
    SwitchCharacter(String),
    /// Do Not Disturb checkbox toggle.
    ToggleDnd,
    /// Hide the Character instantly, same path as the hotkey.
    Hide,
}

/// One row of the menu, as data.
///
/// Ids are only on the rows that can be chosen. A submenu is opened rather than
/// selected, and a separator cannot be clicked at all, so neither has one to
/// look up.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MenuEntry {
    /// A plain row. Disabled rows are still listed: the menu says what exists.
    Item {
        id: String,
        label: String,
        enabled: bool,
    },
    /// A row with a checkmark, which is how state is shown rather than told.
    Check {
        id: String,
        label: String,
        enabled: bool,
        checked: bool,
    },
    /// A row that opens another list.
    Submenu {
        label: String,
        items: Vec<MenuEntry>,
    },
    /// Quit, which the platform supplies and labels in its own words.
    Quit,
}

/// The whole menu as data: the rows, and what the clickable ones do.
///
/// Everything here is owned, so this crosses a thread boundary. That is the
/// point of it: the description is built where the state lives, on the frame
/// loop, and the menu is built where the window server insists, on the main
/// thread.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MenuDescription {
    pub entries: Vec<MenuEntry>,
    pub actions: HashMap<String, MenuAction>,
}

/// The id of the Do Not Disturb checkbox.
const DND_ID: &str = "dnd";

/// The id of the Hide row.
const HIDE_ID: &str = "hide";

/// The id of the Chat row.
///
/// It has one despite being disabled, so that enabling it when #17 lands is a
/// change to one flag rather than to the shape of the menu.
const CHAT_ID: &str = "chat";

/// The id prefix for a Character row, so `character:bmo` cannot collide with a
/// package that happens to be called `hide`.
const CHARACTER_PREFIX: &str = "character:";

/// Describe the menu for one Instance.
///
/// `installed` is the Character Package names the loader found. `current` is
/// the one on screen, which is the one check-marked. `do_not_disturb` is the
/// Engine's own answer, so the checkbox reports the Engine rather than a copy
/// of it that could drift.
pub fn describe(installed: &[String], current: &str, do_not_disturb: bool) -> MenuDescription {
    let mut entries = Vec::new();
    let mut actions = HashMap::new();

    // Chat… — #17. Disabled, and present anyway: the menu is where the feature
    // will be, and an absent row would move everything under it on the day it
    // arrives. No action is registered, so a click cannot do anything.
    entries.push(MenuEntry::Item {
        id: CHAT_ID.to_string(),
        label: "Chat…".to_string(),
        enabled: false,
    });

    // Character ▸ — one row per installed package, current check-marked.
    //
    // Omitted entirely when nothing is installed rather than shown empty. An
    // empty submenu is a dead end that looks like a bug; no submenu says there
    // is nothing to choose between, which is the truth.
    if !installed.is_empty() {
        let items = installed
            .iter()
            .map(|name| {
                let id = format!("{CHARACTER_PREFIX}{name}");
                actions.insert(id.clone(), MenuAction::SwitchCharacter(name.clone()));
                MenuEntry::Check {
                    id,
                    label: name.clone(),
                    enabled: true,
                    checked: name == current,
                }
            })
            .collect();

        entries.push(MenuEntry::Submenu {
            label: "Character".to_string(),
            items,
        });
    }

    entries.push(MenuEntry::Check {
        id: DND_ID.to_string(),
        label: "Do Not Disturb".to_string(),
        enabled: true,
        checked: do_not_disturb,
    });
    actions.insert(DND_ID.to_string(), MenuAction::ToggleDnd);

    entries.push(MenuEntry::Item {
        id: HIDE_ID.to_string(),
        label: "Hide".to_string(),
        enabled: true,
    });
    actions.insert(HIDE_ID.to_string(), MenuAction::Hide);

    // What the buddy can see… — absent until #148 exists. Not a disabled row:
    // a disabled row promises a feature, and there is nothing to promise yet.

    entries.push(MenuEntry::Quit);

    MenuDescription { entries, actions }
}

/// Build the native menu from a description and pop it over `window`.
///
/// Must be called on the main thread. Every constructor here reaches the
/// window server, which on macOS answers only the main thread, and the objects
/// they return cannot be sent to another one.
///
/// `position` is logical points from the window's top-left corner, which is
/// where the cursor was when the right button went down. Passing the window
/// rather than a raw view is what keeps this honest: the runtime resolves the
/// view it actually drew, so there is no null handle to get wrong.
///
/// Returns as soon as the menu is on screen. The selection, if the user makes
/// one, arrives on the app's menu event channel — see `MenuDescription`.
pub fn show(
    app: &tauri::AppHandle,
    description: &MenuDescription,
    window_label: &str,
    position: tauri::LogicalPosition<f64>,
) -> Result<(), tauri::Error> {
    use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};
    use tauri::Manager;

    // Built one item at a time rather than with `with_items`, because the rows
    // are of three different types and a Vec of them needs boxing either way.
    let menu = Menu::new(app)?;

    for entry in &description.entries {
        match entry {
            MenuEntry::Item { id, label, enabled } => {
                let item = MenuItem::with_id(app, id, label, *enabled, None::<&str>)?;
                menu.append(&item)?;
            }
            MenuEntry::Check {
                id,
                label,
                enabled,
                checked,
            } => {
                let item =
                    CheckMenuItem::with_id(app, id, label, *enabled, *checked, None::<&str>)?;
                menu.append(&item)?;
            }
            MenuEntry::Submenu { label, items } => {
                let submenu = Submenu::new(app, label, true)?;
                for item in items {
                    match item {
                        MenuEntry::Check {
                            id,
                            label,
                            enabled,
                            checked,
                        } => {
                            let child = CheckMenuItem::with_id(
                                app,
                                id,
                                label,
                                *enabled,
                                *checked,
                                None::<&str>,
                            )?;
                            submenu.append(&child)?;
                        }
                        MenuEntry::Item { id, label, enabled } => {
                            let child = MenuItem::with_id(app, id, label, *enabled, None::<&str>)?;
                            submenu.append(&child)?;
                        }
                        // One level is all the menu has. Nesting a submenu or a
                        // Quit inside one is not something `describe` builds,
                        // and drawing it would be inventing a shape nothing
                        // asked for.
                        MenuEntry::Submenu { .. } | MenuEntry::Quit => {}
                    }
                }
                menu.append(&submenu)?;
            }
            MenuEntry::Quit => {
                menu.append(&PredefinedMenuItem::quit(app, None)?)?;
            }
        }
    }

    let window = app
        .get_webview_window(window_label)
        .ok_or(tauri::Error::WindowNotFound)?;

    window.popup_menu_at(&menu, position)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(of: &[&str]) -> Vec<String> {
        of.iter().map(|name| (*name).to_string()).collect()
    }

    /// The whole reason the description exists: it can cross to the main thread.
    ///
    /// A compile-time check rather than a runtime one — it fails the build, not
    /// the run. Putting a native handle in a `MenuEntry` would make the menu
    /// unsendable and there would be nothing to build over there from, which is
    /// the mistake this stands in the way of.
    #[test]
    fn a_description_can_be_sent_to_another_thread() {
        fn assert_send<T: Send>() {}
        assert_send::<MenuDescription>();
        assert_send::<MenuEntry>();
        assert_send::<MenuAction>();

        // And genuinely crosses one, so the bound is not just asserted.
        let description = describe(&names(&["bmo"]), "bmo", true);
        let moved = std::thread::spawn(move || description.entries.len());

        // Chat, Character, Do Not Disturb, Hide, Quit.
        assert_eq!(moved.join().expect("the thread panicked"), 5);
    }

    /// The row a description carries for `id`, whatever kind it is.
    fn entry_with_id<'a>(description: &'a MenuDescription, id: &str) -> Option<&'a MenuEntry> {
        description.entries.iter().find(|entry| match entry {
            MenuEntry::Item { id: got, .. } | MenuEntry::Check { id: got, .. } => got == id,
            _ => false,
        })
    }

    /// The rows of the Character submenu, or none if there is no submenu.
    fn character_items(description: &MenuDescription) -> Option<&Vec<MenuEntry>> {
        description.entries.iter().find_map(|entry| match entry {
            MenuEntry::Submenu { label, items } if label == "Character" => Some(items),
            _ => None,
        })
    }

    /// The menu says what exists. Chat is in it before #17 ships, because the
    /// row moving later is worse than a row that cannot be clicked yet.
    #[test]
    fn chat_is_listed_and_disabled() {
        let description = describe(&[], "bmo", false);

        assert_eq!(
            entry_with_id(&description, "chat"),
            Some(&MenuEntry::Item {
                id: "chat".to_string(),
                label: "Chat…".to_string(),
                enabled: false,
            }),
            "Chat is present and disabled until #17"
        );
        assert!(
            !description.actions.contains_key("chat"),
            "and choosing it cannot do anything"
        );
    }

    /// An empty submenu is a dead end that reads as a bug. Nothing installed is
    /// said by there being nothing to open.
    #[test]
    fn no_character_submenu_when_nothing_is_installed() {
        let description = describe(&[], "bmo", false);

        assert!(
            character_items(&description).is_none(),
            "no Character submenu when nothing is installed"
        );
    }

    /// Every package the loader found is offered, in the order it found them.
    #[test]
    fn the_character_submenu_lists_every_installed_package() {
        let description = describe(&names(&["bmo", "nim", "cat"]), "bmo", false);

        let labels: Vec<&str> = character_items(&description)
            .expect("a submenu")
            .iter()
            .filter_map(|entry| match entry {
                MenuEntry::Check { label, .. } => Some(label.as_str()),
                _ => None,
            })
            .collect();

        assert_eq!(labels, vec!["bmo", "nim", "cat"]);
    }

    /// The checkmark is how the menu says which Character is on screen.
    #[test]
    fn only_the_current_character_is_check_marked() {
        let description = describe(&names(&["bmo", "nim"]), "nim", false);

        let checked: Vec<(&str, bool)> = character_items(&description)
            .expect("a submenu")
            .iter()
            .filter_map(|entry| match entry {
                MenuEntry::Check { label, checked, .. } => Some((label.as_str(), *checked)),
                _ => None,
            })
            .collect();

        assert_eq!(checked, vec![("bmo", false), ("nim", true)]);
    }

    /// The checkbox reports the Engine, so opening the menu twice around a
    /// toggle shows the toggle.
    #[test]
    fn the_dnd_checkbox_reports_the_engine() {
        let off = describe(&[], "bmo", false);
        let on = describe(&[], "bmo", true);

        assert_eq!(
            entry_with_id(&off, "dnd"),
            Some(&MenuEntry::Check {
                id: "dnd".to_string(),
                label: "Do Not Disturb".to_string(),
                enabled: true,
                checked: false,
            })
        );
        assert!(
            matches!(
                entry_with_id(&on, "dnd"),
                Some(MenuEntry::Check { checked: true, .. })
            ),
            "checked once the Engine is quiet"
        );
    }

    /// #148 has not been written. A disabled row would promise it.
    #[test]
    fn the_consent_row_is_absent_rather_than_disabled() {
        let description = describe(&names(&["bmo"]), "bmo", false);

        let mentions_seeing = description.entries.iter().any(|entry| match entry {
            MenuEntry::Item { label, .. } | MenuEntry::Check { label, .. } => label.contains("see"),
            MenuEntry::Submenu { label, .. } => label.contains("see"),
            MenuEntry::Quit => false,
        });

        assert!(
            !mentions_seeing,
            "nothing claims to show what the buddy can see"
        );
    }

    /// Quit is the platform's own item, so the action table does not carry it:
    /// an id nothing looks up is an id that cannot be got wrong.
    #[test]
    fn quit_is_listed_without_an_action() {
        let description = describe(&[], "bmo", false);

        assert!(
            description.entries.contains(&MenuEntry::Quit),
            "Quit is in the menu"
        );
        assert_eq!(
            description.actions.len(),
            2,
            "and is not in the action table: only DND and Hide are, {:?}",
            description.actions
        );
    }

    /// The contract between the two halves: every row that can be chosen has an
    /// action under the same id the native item will report.
    #[test]
    fn every_clickable_row_maps_to_an_action() {
        let description = describe(&names(&["bmo", "nim"]), "bmo", true);

        let clickable: Vec<&String> = description
            .entries
            .iter()
            .chain(character_items(&description).into_iter().flatten())
            .filter_map(|entry| match entry {
                MenuEntry::Item { id, enabled, .. } if *enabled => Some(id),
                MenuEntry::Check { id, enabled, .. } if *enabled => Some(id),
                _ => None,
            })
            .collect();

        for id in &clickable {
            assert!(
                description.actions.contains_key(id.as_str()),
                "{id} is enabled and has no action"
            );
        }

        assert_eq!(
            description.actions.get("character:nim"),
            Some(&MenuAction::SwitchCharacter("nim".to_string())),
            "and a Character row switches to its own Character"
        );
        assert_eq!(description.actions.get("dnd"), Some(&MenuAction::ToggleDnd));
        assert_eq!(description.actions.get("hide"), Some(&MenuAction::Hide));
    }

    /// A package called `hide` must not become the Hide row.
    #[test]
    fn a_character_named_like_a_row_does_not_collide_with_it() {
        let description = describe(&names(&["hide", "dnd"]), "hide", false);

        assert_eq!(description.actions.get("hide"), Some(&MenuAction::Hide));
        assert_eq!(
            description.actions.get("character:hide"),
            Some(&MenuAction::SwitchCharacter("hide".to_string()))
        );
        assert_eq!(description.actions.get("dnd"), Some(&MenuAction::ToggleDnd));
    }
}
