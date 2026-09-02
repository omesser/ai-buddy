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
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MenuAction {
    /// Character ▸ <name>. Switch to the named Character Package.
    SwitchCharacter(String),
    /// Spawn another buddy of the current Character.
    SpawnInstance,
    /// Session Director on/off. Off leaves Static weights running the life.
    ToggleDirector,
    /// Quiet without hiding. #84: proposals stop; the Character stays on screen.
    ToggleDnd,
    /// Hide the Character instantly, same path as the hotkey.
    Hide,
    /// Fade away when a fullscreen application is frontmost. Settings writes the same flag.
    ToggleFullscreenHide,
    /// Open Memory in the user's editor.
    OpenMemory,
    /// Open the settings window. Tray and sprite both reach it this way.
    OpenSettings,
    /// Leave. Ours, not `PredefinedMenuItem::quit`: that calls `terminate:`
    /// from inside the tray menu and deadlocks the overlay webviews.
    Quit,
}

/// Everything `describe` needs to draw one menu. Tray and sprite build the
/// same description from this, so a row cannot exist in one and not the other.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MenuSnapshot<'a> {
    pub installed: &'a [String],
    pub current_character: &'a str,
    pub instances: &'a [(String, String)],
    pub director_enabled: bool,
    pub do_not_disturb: bool,
    pub hidden: bool,
    pub hide_in_fullscreen: bool,
    pub hide_hotkey: &'a str,
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

/// The id of the Hide / Come back row.
const HIDE_ID: &str = "hide";

/// The id of the Chat row.
///
/// It has one despite being disabled, so that enabling it when #17 lands is a
/// change to one flag rather than to the shape of the menu.
const CHAT_ID: &str = "chat";

/// The id of the Director checkbox.
const DIRECTOR_ID: &str = "director";

/// The id of the Memory row.
const MEMORY_ID: &str = "memory";

/// The id of the Settings row, and of Hotkey… which opens the same window.
const SETTINGS_ID: &str = "settings";

/// The id of New… under Instances.
const SPAWN_ID: &str = "spawn";

/// The id of Hide in fullscreen apps.
const FULLSCREEN_ID: &str = "fullscreen";

/// The id of the hotkey row under Hide rules.
const HOTKEY_ID: &str = "hotkey";

/// The id of Quit. Owned here so the tray event hook and the action table
/// cannot drift onto different strings.
pub(crate) const QUIT_ID: &str = "quit";

/// The id prefix for a Character row, so `character:bmo` cannot collide with a
/// package that happens to be called `hide`.
const CHARACTER_PREFIX: &str = "character:";

/// Describe the menu. Tray and sprite both call this.
///
/// Checkboxes report the Engine and settings rather than a copy that could
/// drift: opening the menu twice around a toggle has to show the toggle.
pub fn describe(snapshot: MenuSnapshot<'_>) -> MenuDescription {
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
    if !snapshot.installed.is_empty() {
        let items = snapshot
            .installed
            .iter()
            .map(|name| {
                let id = format!("{CHARACTER_PREFIX}{name}");
                actions.insert(id.clone(), MenuAction::SwitchCharacter(name.clone()));
                MenuEntry::Check {
                    id,
                    label: name.clone(),
                    enabled: true,
                    checked: name == snapshot.current_character,
                }
            })
            .collect();

        entries.push(MenuEntry::Submenu {
            label: "Character".to_string(),
            items,
        });
    }

    // Instances ▸ — who is on screen, and New… to spawn another. Always
    // present: an empty list still has New…, which is how a dismissed last
    // buddy comes back without hunting settings.
    {
        let mut items: Vec<MenuEntry> = snapshot
            .instances
            .iter()
            .map(|(_, name)| MenuEntry::Item {
                id: format!("instance:{name}"),
                label: name.clone(),
                enabled: false,
            })
            .collect();
        items.push(MenuEntry::Item {
            id: SPAWN_ID.to_string(),
            label: "New…".to_string(),
            enabled: true,
        });
        actions.insert(SPAWN_ID.to_string(), MenuAction::SpawnInstance);
        entries.push(MenuEntry::Submenu {
            label: "Instances".to_string(),
            items,
        });
    }

    entries.push(MenuEntry::Check {
        id: DIRECTOR_ID.to_string(),
        label: "Director".to_string(),
        enabled: true,
        checked: snapshot.director_enabled,
    });
    actions.insert(DIRECTOR_ID.to_string(), MenuAction::ToggleDirector);

    entries.push(MenuEntry::Check {
        id: DND_ID.to_string(),
        label: "Do Not Disturb".to_string(),
        enabled: true,
        checked: snapshot.do_not_disturb,
    });
    actions.insert(DND_ID.to_string(), MenuAction::ToggleDnd);

    // The same flag the hotkey flips. The label says which way it is pointing
    // so a checkmark is not doing two jobs.
    entries.push(MenuEntry::Item {
        id: HIDE_ID.to_string(),
        label: if snapshot.hidden {
            "Come back".to_string()
        } else {
            "Go away".to_string()
        },
        enabled: true,
    });
    actions.insert(HIDE_ID.to_string(), MenuAction::Hide);

    entries.push(MenuEntry::Submenu {
        label: "Hide rules".to_string(),
        items: vec![
            MenuEntry::Check {
                id: FULLSCREEN_ID.to_string(),
                label: "Hide in fullscreen apps".to_string(),
                enabled: true,
                checked: snapshot.hide_in_fullscreen,
            },
            MenuEntry::Item {
                id: HOTKEY_ID.to_string(),
                label: format!("Hotkey: {}", snapshot.hide_hotkey),
                enabled: true,
            },
        ],
    });
    actions.insert(FULLSCREEN_ID.to_string(), MenuAction::ToggleFullscreenHide);
    actions.insert(HOTKEY_ID.to_string(), MenuAction::OpenSettings);

    entries.push(MenuEntry::Item {
        id: MEMORY_ID.to_string(),
        label: "Memory…".to_string(),
        enabled: true,
    });
    actions.insert(MEMORY_ID.to_string(), MenuAction::OpenMemory);

    entries.push(MenuEntry::Item {
        id: SETTINGS_ID.to_string(),
        label: "Settings…".to_string(),
        enabled: true,
    });
    actions.insert(SETTINGS_ID.to_string(), MenuAction::OpenSettings);

    // What the buddy can see… — absent until #148 exists. Not a disabled row:
    // a disabled row promises a feature, and there is nothing to promise yet.

    entries.push(MenuEntry::Item {
        id: QUIT_ID.to_string(),
        label: "Quit".to_string(),
        enabled: true,
    });
    actions.insert(QUIT_ID.to_string(), MenuAction::Quit);

    MenuDescription { entries, actions }
}

/// The next tray draw, if this description is not the one already showing.
///
/// Settings can dismiss or spawn without a menu click. The tray only
/// updates when someone pushes a new description, so "did a row change"
/// is the gate, not "did they click".
pub fn replace_if_changed(
    previous: &mut Option<MenuDescription>,
    next: MenuDescription,
) -> Option<MenuDescription> {
    if previous.as_ref() == Some(&next) {
        None
    } else {
        *previous = Some(next.clone());
        Some(next)
    }
}

/// Build the native menu from a description.
///
/// Must be called on the main thread. Every constructor here reaches the
/// window server, which on macOS answers only the main thread, and the objects
/// they return cannot be sent to another one.
///
/// Tray and sprite both start here, so a row cannot exist in one and not the
/// other.
pub fn build(
    app: &tauri::AppHandle,
    description: &MenuDescription,
) -> Result<tauri::menu::Menu<tauri::Wry>, tauri::Error> {
    use tauri::menu::{CheckMenuItem, Menu, MenuItem, Submenu};

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
                        // One level is all the menu has. Nesting a submenu
                        // inside one is not something `describe` builds.
                        MenuEntry::Submenu { .. } => {}
                    }
                }
                menu.append(&submenu)?;
            }
        }
    }

    Ok(menu)
}

/// Pop the described menu over `window`.
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
    use tauri::Manager;

    let menu = build(app, description)?;
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

    fn snapshot<'a>(
        installed: &'a [String],
        current: &'a str,
        do_not_disturb: bool,
    ) -> MenuSnapshot<'a> {
        MenuSnapshot {
            installed,
            current_character: current,
            instances: &[],
            director_enabled: true,
            do_not_disturb,
            hidden: false,
            hide_in_fullscreen: true,
            hide_hotkey: "Control-Option-Command-B",
        }
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
        let installed = names(&["bmo"]);
        let description = describe(snapshot(&installed, "bmo", true));
        let moved = std::thread::spawn(move || description.entries.len());

        assert!(moved.join().expect("the thread panicked") > 0);
    }

    /// The row a description carries for `id`, whatever kind it is.
    fn entry_with_id<'a>(description: &'a MenuDescription, id: &str) -> Option<&'a MenuEntry> {
        description.entries.iter().find(|entry| match entry {
            MenuEntry::Item { id: got, .. } | MenuEntry::Check { id: got, .. } => got == id,
            _ => false,
        })
    }

    /// The rows of the Instances submenu.
    fn instance_items(description: &MenuDescription) -> Option<&Vec<MenuEntry>> {
        description.entries.iter().find_map(|entry| match entry {
            MenuEntry::Submenu { label, items } if label == "Instances" => Some(items),
            _ => None,
        })
    }

    fn instance_labels(description: &MenuDescription) -> Vec<&str> {
        instance_items(description)
            .into_iter()
            .flatten()
            .filter_map(|entry| match entry {
                MenuEntry::Item { label, .. } => Some(label.as_str()),
                _ => None,
            })
            .collect()
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
        let description = describe(snapshot(&[], "bmo", false));

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
        let description = describe(snapshot(&[], "bmo", false));

        assert!(
            character_items(&description).is_none(),
            "no Character submenu when nothing is installed"
        );
    }

    /// Every package the loader found is offered, in the order it found them.
    #[test]
    fn the_character_submenu_lists_every_installed_package() {
        let installed = names(&["bmo", "nim", "cat"]);
        let description = describe(snapshot(&installed, "bmo", false));

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
        let installed = names(&["bmo", "nim"]);
        let description = describe(snapshot(&installed, "nim", false));

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
        let off = describe(snapshot(&[], "bmo", false));
        let on = describe(snapshot(&[], "bmo", true));

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
        let installed = names(&["bmo"]);
        let description = describe(snapshot(&installed, "bmo", false));

        let mentions_seeing = description.entries.iter().any(|entry| match entry {
            MenuEntry::Item { label, .. } | MenuEntry::Check { label, .. } => label.contains("see"),
            MenuEntry::Submenu { label, .. } => label.contains("see"),
        });

        assert!(
            !mentions_seeing,
            "nothing claims to show what the buddy can see"
        );
    }

    /// Quit is a row we handle, not the platform's `terminate:`. That call
    /// deadlocks the overlay webviews when it runs from inside the tray menu.
    #[test]
    fn quit_maps_to_an_action() {
        let description = describe(snapshot(&[], "bmo", false));

        assert_eq!(
            entry_with_id(&description, "quit"),
            Some(&MenuEntry::Item {
                id: "quit".to_string(),
                label: "Quit".to_string(),
                enabled: true,
            })
        );
        assert_eq!(description.actions.get("quit"), Some(&MenuAction::Quit));
    }

    fn clickable_ids(description: &MenuDescription) -> Vec<&String> {
        fn walk<'a>(entries: &'a [MenuEntry], out: &mut Vec<&'a String>) {
            for entry in entries {
                match entry {
                    MenuEntry::Item { id, enabled, .. } if *enabled => out.push(id),
                    MenuEntry::Check { id, enabled, .. } if *enabled => out.push(id),
                    MenuEntry::Submenu { items, .. } => walk(items, out),
                    _ => {}
                }
            }
        }
        let mut ids = Vec::new();
        walk(&description.entries, &mut ids);
        ids
    }

    /// The contract between the two halves: every row that can be chosen has an
    /// action under the same id the native item will report.
    #[test]
    fn every_clickable_row_maps_to_an_action() {
        let installed = names(&["bmo", "nim"]);
        let description = describe(snapshot(&installed, "bmo", true));

        for id in clickable_ids(&description) {
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
        assert_eq!(
            description.actions.get("director"),
            Some(&MenuAction::ToggleDirector)
        );
        assert_eq!(
            description.actions.get("memory"),
            Some(&MenuAction::OpenMemory)
        );
        assert_eq!(
            description.actions.get("settings"),
            Some(&MenuAction::OpenSettings)
        );
    }

    /// A package called `hide` must not become the Hide row.
    #[test]
    fn a_character_named_like_a_row_does_not_collide_with_it() {
        let installed = names(&["hide", "dnd", "quit"]);
        let description = describe(snapshot(&installed, "hide", false));

        assert_eq!(description.actions.get("hide"), Some(&MenuAction::Hide));
        assert_eq!(
            description.actions.get("character:hide"),
            Some(&MenuAction::SwitchCharacter("hide".to_string()))
        );
        assert_eq!(description.actions.get("dnd"), Some(&MenuAction::ToggleDnd));
        assert_eq!(description.actions.get("quit"), Some(&MenuAction::Quit));
        assert_eq!(
            description.actions.get("character:quit"),
            Some(&MenuAction::SwitchCharacter("quit".to_string()))
        );
    }

    /// Settings and Memory are how the tray reaches configuration without
    /// finding the sprite. Both entry points share this description.
    #[test]
    fn settings_and_memory_are_reachable() {
        let description = describe(snapshot(&[], "bmo", false));

        assert_eq!(
            entry_with_id(&description, "settings"),
            Some(&MenuEntry::Item {
                id: "settings".to_string(),
                label: "Settings…".to_string(),
                enabled: true,
            })
        );
        assert_eq!(
            entry_with_id(&description, "memory"),
            Some(&MenuEntry::Item {
                id: "memory".to_string(),
                label: "Memory…".to_string(),
                enabled: true,
            })
        );
    }

    /// The Director checkbox is how ambient life is turned off without
    /// hunting the sprite. Settings owns the same flag.
    #[test]
    fn the_director_checkbox_reports_whether_it_is_on() {
        let installed = names(&["bmo"]);
        let mut off = snapshot(&installed, "bmo", false);
        off.director_enabled = false;
        let description = describe(off);

        assert!(
            matches!(
                entry_with_id(&description, "director"),
                Some(MenuEntry::Check { checked: false, .. })
            ),
            "unchecked when the Director is off"
        );
    }

    /// Go away / Come back is the same flag as the hotkey, so the label has
    /// to say which way it is pointing.
    #[test]
    fn hide_says_come_back_when_the_character_is_away() {
        let installed = names(&["bmo"]);
        let mut away = snapshot(&installed, "bmo", false);
        away.hidden = true;
        let description = describe(away);

        assert_eq!(
            entry_with_id(&description, "hide"),
            Some(&MenuEntry::Item {
                id: "hide".to_string(),
                label: "Come back".to_string(),
                enabled: true,
            })
        );
    }

    #[test]
    fn hide_rules_include_fullscreen_and_the_hotkey() {
        let description = describe(snapshot(&[], "bmo", false));
        let items = description.entries.iter().find_map(|entry| match entry {
            MenuEntry::Submenu { label, items } if label == "Hide rules" => Some(items),
            _ => None,
        });
        let items = items.expect("Hide rules submenu");

        assert!(
            items.iter().any(|entry| matches!(
                entry,
                MenuEntry::Check {
                    id,
                    checked: true,
                    ..
                } if id == "fullscreen"
            )),
            "fullscreen hide is on by default"
        );
        assert_eq!(
            description.actions.get("hotkey"),
            Some(&MenuAction::OpenSettings),
            "the hotkey row opens settings, where it is bound"
        );
        // DESIGN.md: quiet is not gone. DND lives on the menu, not in this list.
        assert!(
            items.iter().all(|entry| match entry {
                MenuEntry::Check { id, .. } | MenuEntry::Item { id, .. } => id != "dnd",
                _ => true,
            }),
            "Do Not Disturb is not a hide rule"
        );
    }

    /// The tray only redraws when this says the description changed. A
    /// dismiss that does not pass through a menu click still has to.
    #[test]
    fn a_dismissed_instance_is_a_menu_that_must_be_pushed() {
        let installed = names(&["bmo"]);
        let two = [
            ("id-nim".to_string(), "Nim".to_string()),
            ("id-bmo".to_string(), "BMO".to_string()),
        ];
        let mut snap = snapshot(&installed, "bmo", false);
        snap.instances = &two;
        let before = describe(snap.clone());

        let one = [("id-nim".to_string(), "Nim".to_string())];
        snap.instances = &one;
        let after = describe(snap);

        assert_eq!(instance_labels(&before), ["Nim", "BMO", "New…"]);
        assert_eq!(instance_labels(&after), ["Nim", "New…"]);

        let mut last = Some(before);
        let pushed = replace_if_changed(&mut last, after);
        assert!(pushed.is_some(), "a dismissed Instance is a different menu");
        assert_eq!(
            instance_labels(pushed.as_ref().expect("pushed")),
            ["Nim", "New…"]
        );
    }

    #[test]
    fn an_unchanged_menu_is_not_pushed_again() {
        let installed = names(&["bmo"]);
        let first = describe(snapshot(&installed, "bmo", false));
        let again = describe(snapshot(&installed, "bmo", false));
        let mut last = Some(first);
        assert!(
            replace_if_changed(&mut last, again).is_none(),
            "the same rows are not a rebuild"
        );
    }
}
