//! Settings form description: sections, rows, labels, and what they write.
//!
//! Split the same way the menu is: the form as data crosses the platform
//! boundary, and the AppKit window builds from that description. Linux and
//! Windows consume the same description when they ship, so labels cannot
//! drift.

use std::collections::HashMap;

use crate::consent;

/// What a settings row writes when changed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RowAction {
    /// Writes to a SettingsPatch field.
    PatchField(String),
    /// Sends a SettingsOp.
    Operation(RowOperation),
}

/// Operations the settings window requests.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RowOperation {
    Spawn,
    OpenMemory,
    WipeMemory,
    ClearKey,
}

/// One section of the settings form.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FormSection {
    pub heading: String,
    pub rows: Vec<FormRow>,
    pub comment: Option<String>,
}

/// One row of the settings form, as data.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FormRow {
    /// A checkbox that writes a bool to Settings.
    Checkbox {
        id: String,
        label: String,
        frozen: bool,
        help: Option<String>,
        comment: Option<String>,
    },
    /// An inspect-only text block showing current state.
    InspectBlock {
        id: String,
        label: Option<String>,
        help: Option<String>,
    },
    /// An inspect-only wrapping label showing a path.
    InspectPath { id: String },
    /// A popup menu for choosing between options.
    Popup { id: String, label: Option<String> },
    /// A multiline text field that writes to Settings.
    Multiline {
        id: String,
        label: Option<String>,
        help: Option<String>,
        editable: bool,
    },
    /// An editable text field that writes a string to Settings.
    TextField {
        id: String,
        label: Option<String>,
        placeholder: String,
    },
    /// A secure text field for passwords/keys.
    SecureField { id: String, label: Option<String> },
    /// A scrollable list of items with dismiss buttons.
    List { id: String, dismiss_label: String },
    /// A row of multiple controls (e.g., new instance spawn row).
    Composite {
        id: String,
        controls: Vec<CompositeControl>,
    },
}

/// One control in a composite row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CompositeControl {
    TextField { id: String, placeholder: String },
    Popup { id: String },
    Button { id: String, label: String },
}

/// The whole settings form as data: sections, rows, and what they write.
///
/// Everything here is owned, so this crosses a thread boundary. That is the
/// point of it: the description is built where the state lives, and the
/// platform window builds from it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FormDescription {
    pub sections: Vec<FormSection>,
    pub actions: HashMap<String, RowAction>,
}

/// Row ids for the settings form controls.
pub const DIRECTOR_ID: &str = "director";
pub const AMBIENT_ID: &str = "ambient";
pub const DIRECTOR_BASE_URL_ID: &str = "director_base_url";
pub const DIRECTOR_MODEL_ID: &str = "director_model";
pub const DIRECTOR_API_KEY_ID: &str = "director_api_key";
pub const CLEAR_KEY_ID: &str = "clear_key";
pub const DND_ID: &str = "dnd";
pub const HIDDEN_ID: &str = "hidden";
pub const FULLSCREEN_ID: &str = "fullscreen";
pub const HOTKEY_ID: &str = "hotkey";
pub const EXCLUDED_ID: &str = "excluded";
pub const PAYLOAD_ID: &str = "payload";
pub const MEMORY_PATH_ID: &str = "memory_path";
pub const CHARACTER_ID: &str = "character";
pub const INSTANCES_ID: &str = "instances";
pub const NEW_NAME_ID: &str = "new_name";
pub const NEW_CHARACTER_ID: &str = "new_character";
pub const SPAWN_ID: &str = "spawn";
pub const MEMORY_OPEN_ID: &str = "memory_open";
pub const MEMORY_WIPE_ID: &str = "memory_wipe";
pub const CONSENT_ACCESSIBILITY_ID: &str = "consent_accessibility";
pub const CONSENT_SCREEN_RECORDING_ID: &str = "consent_screen_recording";
pub const LAUNCH_ID: &str = "launch";

/// Platform-specific help text for excluded applications.
fn excluded_help() -> String {
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        "One application name per line, matched against X11 WM_CLASS. Those windows stay out of MCP sensing, and the Director is not told they are frontmost. The buddy can still sit on them.".to_string()
    }
    #[cfg(not(all(unix, not(target_os = "macos"))))]
    {
        "One application name per line. Those windows stay out of MCP sensing, and the Director is not told they are frontmost. The buddy can still sit on them.".to_string()
    }
}

/// Describe the settings form. The AppKit window builds from this.
pub fn describe() -> FormDescription {
    let sections = vec![
        FormSection {
            heading: "Director".to_string(),
            comment: None,
            rows: vec![
                FormRow::Checkbox {
                    id: DIRECTOR_ID.to_string(),
                    label: "Director on".to_string(),
                    frozen: false,
                    help: Some(
                        "Off leaves Static weights running the life. No session calls.".to_string(),
                    ),
                    comment: None,
                },
                FormRow::Checkbox {
                    id: AMBIENT_ID.to_string(),
                    label: "Ambient session wakes".to_string(),
                    frozen: false,
                    help: Some(
                        "Off keeps Poke and Summon on the session path. Idle life stays Static."
                            .to_string(),
                    ),
                    comment: None,
                },
                FormRow::TextField {
                    id: DIRECTOR_BASE_URL_ID.to_string(),
                    label: Some("Base URL".to_string()),
                    placeholder: "https://api.openai.com".to_string(),
                },
                FormRow::TextField {
                    id: DIRECTOR_MODEL_ID.to_string(),
                    label: Some("Model".to_string()),
                    placeholder: "gpt-4o-mini".to_string(),
                },
                FormRow::SecureField {
                    id: DIRECTOR_API_KEY_ID.to_string(),
                    label: Some("API key".to_string()),
                },
                FormRow::Composite {
                    id: "api_key_actions".to_string(),
                    controls: vec![CompositeControl::Button {
                        id: CLEAR_KEY_ID.to_string(),
                        label: "Clear key".to_string(),
                    }],
                },
            ],
        },
        FormSection {
            heading: "Last user turn".to_string(),
            comment: None,
            rows: vec![FormRow::InspectBlock {
                id: PAYLOAD_ID.to_string(),
                label: None,
                help: Some(
                    "Inspect only. The last session turn, opening Character Prompt or follow-up."
                        .to_string(),
                ),
            }],
        },
        FormSection {
            heading: "Character".to_string(),
            comment: None,
            rows: vec![FormRow::Popup {
                id: CHARACTER_ID.to_string(),
                label: None,
            }],
        },
        FormSection {
            heading: "Instances".to_string(),
            comment: None,
            rows: vec![
                FormRow::List {
                    id: INSTANCES_ID.to_string(),
                    dismiss_label: "Dismiss".to_string(),
                },
                FormRow::Composite {
                id: "new_instance".to_string(),
                controls: vec![
                    CompositeControl::TextField {
                        id: NEW_NAME_ID.to_string(),
                        placeholder: "Name".to_string(),
                    },
                    CompositeControl::Popup {
                        id: NEW_CHARACTER_ID.to_string(),
                    },
                    CompositeControl::Button {
                        id: SPAWN_ID.to_string(),
                        label: "New".to_string(),
                    },
                ],
            },
            ],
        },
        FormSection {
            heading: "Do Not Disturb".to_string(),
            comment: Some("DESIGN.md: quiet is not gone. A Hide heading would teach the opposite.".to_string()),
            rows: vec![FormRow::Checkbox {
                id: DND_ID.to_string(),
                label: "Do Not Disturb".to_string(),
                frozen: false,
                help: Some("On screen, not starting things.".to_string()),
                comment: None,
            }],
        },
        FormSection {
            heading: "Hide".to_string(),
            comment: None,
            rows: vec![
                FormRow::Checkbox {
                    id: HIDDEN_ID.to_string(),
                    label: "Go away".to_string(),
                    frozen: false,
                    help: None,
                    comment: None,
                },
                FormRow::Checkbox {
                    id: FULLSCREEN_ID.to_string(),
                    label: "Hide in fullscreen apps".to_string(),
                    frozen: false,
                    help: None,
                    comment: None,
                },
                FormRow::InspectBlock {
                    id: HOTKEY_ID.to_string(),
                    label: Some("Hotkey".to_string()),
                    help: Some("Shown, not edited. A string field is not a key recorder.".to_string()),
                },
            ],
        },
        FormSection {
            heading: "Memory".to_string(),
            comment: None,
            rows: vec![
                FormRow::InspectPath {
                    id: MEMORY_PATH_ID.to_string(),
                },
            FormRow::Composite {
                id: "memory_actions".to_string(),
                controls: vec![
                    CompositeControl::Button {
                        id: MEMORY_OPEN_ID.to_string(),
                        label: "Open in editor".to_string(),
                    },
                    CompositeControl::Button {
                        id: MEMORY_WIPE_ID.to_string(),
                        label: "Wipe".to_string(),
                    },
                ],
            },
            ],
        },
        FormSection {
            heading: "Excluded applications".to_string(),
            comment: None,
            rows: vec![FormRow::Multiline {
                id: EXCLUDED_ID.to_string(),
                label: None,
                help: Some(excluded_help()),
                editable: true,
            }],
        },
        FormSection {
            heading: "What the buddy can see".to_string(),
            comment: Some(consent::pane_intro(&consent::process_listed_as())),
            rows: vec![
                FormRow::Checkbox {
                    id: CONSENT_ACCESSIBILITY_ID.to_string(),
                    label: "Accessibility".to_string(),
                    frozen: false,
                    help: Some("Exact Dock geometry, so the sprite does not walk into the Dock. macOS Accessibility. The buddy reads the Dock's bounds; it does not control your computer.".to_string()),
                    comment: None,
                },
                FormRow::Checkbox {
                    id: CONSENT_SCREEN_RECORDING_ID.to_string(),
                    label: "Screen Recording".to_string(),
                    frozen: false,
                    help: Some("Window titles, and Capture when it ships. macOS Screen Recording, which can see the screen.".to_string()),
                    comment: None,
                },
            ],
        },
        FormSection {
            heading: "Launch".to_string(),
            comment: None,
            rows: vec![FormRow::Checkbox {
                id: LAUNCH_ID.to_string(),
                label: "Launch at login (unimplemented)".to_string(),
                frozen: true,
                help: None,
                comment: Some("A Launch Agent on `cargo run` is not launch-at-login. There is no bundled app to start, on any OS.".to_string()),
            }],
        },
    ];

    let mut actions = HashMap::new();

    // Register what each control writes
    actions.insert(
        DIRECTOR_ID.to_string(),
        RowAction::PatchField("director_enabled".to_string()),
    );
    actions.insert(
        AMBIENT_ID.to_string(),
        RowAction::PatchField("ambient_wakes".to_string()),
    );
    actions.insert(
        DND_ID.to_string(),
        RowAction::PatchField("do_not_disturb".to_string()),
    );
    actions.insert(
        HIDDEN_ID.to_string(),
        RowAction::PatchField("hidden".to_string()),
    );
    actions.insert(
        FULLSCREEN_ID.to_string(),
        RowAction::PatchField("hide_in_fullscreen".to_string()),
    );
    actions.insert(
        EXCLUDED_ID.to_string(),
        RowAction::PatchField("excluded_applications".to_string()),
    );
    actions.insert(
        CHARACTER_ID.to_string(),
        RowAction::PatchField("character".to_string()),
    );
    actions.insert(
        LAUNCH_ID.to_string(),
        RowAction::PatchField("launch_at_login".to_string()),
    );
    actions.insert(
        SPAWN_ID.to_string(),
        RowAction::Operation(RowOperation::Spawn),
    );
    actions.insert(
        MEMORY_OPEN_ID.to_string(),
        RowAction::Operation(RowOperation::OpenMemory),
    );
    actions.insert(
        MEMORY_WIPE_ID.to_string(),
        RowAction::Operation(RowOperation::WipeMemory),
    );
    actions.insert(
        DIRECTOR_BASE_URL_ID.to_string(),
        RowAction::PatchField("director_base_url".to_string()),
    );
    actions.insert(
        DIRECTOR_MODEL_ID.to_string(),
        RowAction::PatchField("director_model".to_string()),
    );
    actions.insert(
        DIRECTOR_API_KEY_ID.to_string(),
        RowAction::PatchField("director_api_key".to_string()),
    );
    actions.insert(
        CLEAR_KEY_ID.to_string(),
        RowAction::Operation(RowOperation::ClearKey),
    );
    actions.insert(
        CONSENT_ACCESSIBILITY_ID.to_string(),
        RowAction::PatchField("use_accessibility".to_string()),
    );
    actions.insert(
        CONSENT_SCREEN_RECORDING_ID.to_string(),
        RowAction::PatchField("use_screen_recording".to_string()),
    );

    FormDescription { sections, actions }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn form_description_can_be_sent_to_another_thread() {
        fn assert_send<T: Send>() {}
        assert_send::<FormDescription>();
        assert_send::<FormSection>();
        assert_send::<FormRow>();

        let description = describe();
        let moved = std::thread::spawn(move || description.sections.len());
        assert!(moved.join().expect("thread panicked") > 0);
    }

    #[test]
    fn all_sections_are_present() {
        let description = describe();
        let headings: Vec<&str> = description
            .sections
            .iter()
            .map(|s| s.heading.as_str())
            .collect();

        assert_eq!(
            headings,
            vec![
                "Director",
                "Last user turn",
                "Character",
                "Instances",
                "Do Not Disturb",
                "Hide",
                "Memory",
                "Excluded applications",
                "What the buddy can see",
                "Launch",
            ]
        );
    }

    #[test]
    fn launch_row_is_frozen() {
        let description = describe();
        let launch_section = description
            .sections
            .iter()
            .find(|s| s.heading == "Launch")
            .expect("Launch section exists");

        let launch_row = launch_section
            .rows
            .iter()
            .find(|r| matches!(r, FormRow::Checkbox { id, .. } if id == LAUNCH_ID))
            .expect("Launch checkbox exists");

        match launch_row {
            FormRow::Checkbox { frozen, label, .. } => {
                assert!(
                    *frozen,
                    "Launch at login checkbox must be frozen until #132 ships"
                );
                assert!(
                    label.contains("unimplemented"),
                    "Launch checkbox must be labeled unimplemented"
                );
            }
            _ => panic!("Launch row must be a checkbox"),
        }
    }

    #[test]
    fn no_other_frozen_rows() {
        let description = describe();
        let frozen_rows: Vec<&String> = description
            .sections
            .iter()
            .flat_map(|s| &s.rows)
            .filter_map(|r| match r {
                FormRow::Checkbox { id, frozen, .. } if *frozen => Some(id),
                _ => None,
            })
            .collect();

        assert_eq!(frozen_rows, vec![LAUNCH_ID], "only Launch should be frozen");
    }

    #[test]
    fn director_section_has_two_checkboxes() {
        let description = describe();
        let director = description
            .sections
            .iter()
            .find(|s| s.heading == "Director")
            .expect("Director section");

        assert_eq!(director.rows.len(), 6);
        assert!(matches!(
            director.rows[0],
            FormRow::Checkbox { ref id, .. } if id == DIRECTOR_ID
        ));
        assert!(matches!(
            director.rows[1],
            FormRow::Checkbox { ref id, .. } if id == AMBIENT_ID
        ));
        assert!(matches!(
            director.rows[2],
            FormRow::TextField { ref id, .. } if id == DIRECTOR_BASE_URL_ID
        ));
        assert!(matches!(
            director.rows[3],
            FormRow::TextField { ref id, .. } if id == DIRECTOR_MODEL_ID
        ));
        assert!(matches!(
            director.rows[4],
            FormRow::SecureField { ref id, .. } if id == DIRECTOR_API_KEY_ID
        ));
        assert!(matches!(
            director.rows[5],
            FormRow::Composite { ref id, .. } if id == "api_key_actions"
        ));
    }

    #[test]
    fn character_section_has_popup() {
        let description = describe();
        let character = description
            .sections
            .iter()
            .find(|s| s.heading == "Character")
            .expect("Character section");

        assert_eq!(character.rows.len(), 1);
        assert!(matches!(
            character.rows[0],
            FormRow::Popup { ref id, .. } if id == CHARACTER_ID
        ));
    }

    #[test]
    fn instances_section_has_list_and_new() {
        let description = describe();
        let instances = description
            .sections
            .iter()
            .find(|s| s.heading == "Instances")
            .expect("Instances section");

        assert_eq!(instances.rows.len(), 2);
        assert!(
            matches!(
                instances.rows[0],
                FormRow::List { ref id, .. } if id == INSTANCES_ID
            ),
            "Instances row must be a List"
        );
        assert!(matches!(instances.rows[1], FormRow::Composite { .. }));
    }

    #[test]
    fn memory_section_has_path_and_actions() {
        let description = describe();
        let memory = description
            .sections
            .iter()
            .find(|s| s.heading == "Memory")
            .expect("Memory section");

        assert_eq!(memory.rows.len(), 2);
        assert!(
            matches!(
                memory.rows[0],
                FormRow::InspectPath { ref id } if id == MEMORY_PATH_ID
            ),
            "Memory path must be InspectPath"
        );
        assert!(matches!(memory.rows[1], FormRow::Composite { .. }));
    }

    #[test]
    fn excluded_section_has_multiline() {
        let description = describe();
        let excluded = description
            .sections
            .iter()
            .find(|s| s.heading == "Excluded applications")
            .expect("Excluded applications section");

        assert_eq!(excluded.rows.len(), 1);
        assert!(matches!(
            excluded.rows[0],
            FormRow::Multiline { ref id, .. } if id == EXCLUDED_ID
        ));
    }

    #[test]
    fn help_text_is_present() {
        let description = describe();

        let has_help = |section: &str, row_id: &str| -> bool {
            description
                .sections
                .iter()
                .find(|s| s.heading == section)
                .and_then(|s| {
                    s.rows.iter().find(|r| match r {
                        FormRow::Checkbox { id, help, .. } => {
                            id == row_id && help.as_ref().is_some_and(|h| !h.is_empty())
                        }
                        FormRow::InspectBlock { id, help, .. } => {
                            id == row_id && help.as_ref().is_some_and(|h| !h.is_empty())
                        }
                        FormRow::Multiline { id, help, .. } => {
                            id == row_id && help.as_ref().is_some_and(|h| !h.is_empty())
                        }
                        _ => false,
                    })
                })
                .is_some()
        };

        assert!(has_help("Director", DIRECTOR_ID));
        assert!(has_help("Director", AMBIENT_ID));
        assert!(has_help("Last user turn", PAYLOAD_ID));
        assert!(has_help("Do Not Disturb", DND_ID));
        assert!(has_help("Excluded applications", EXCLUDED_ID));
    }

    #[test]
    fn every_control_has_an_action() {
        let description = describe();

        for section in &description.sections {
            for row in &section.rows {
                match row {
                    FormRow::Checkbox { id, .. }
                    | FormRow::Popup { id, .. }
                    | FormRow::Multiline { id, .. }
                    | FormRow::TextField { id, .. }
                    | FormRow::SecureField { id, .. } => {
                        assert!(
                            description.actions.contains_key(id),
                            "Row {id} has no action"
                        );
                    }
                    FormRow::Composite { controls, .. } => {
                        for control in controls {
                            if let CompositeControl::Button { id, .. } = control {
                                assert!(
                                    description.actions.contains_key(id),
                                    "Button {id} has no action"
                                );
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    #[test]
    fn actions_map_to_patches_or_ops() {
        let description = describe();

        assert_eq!(
            description.actions.get(DIRECTOR_ID),
            Some(&RowAction::PatchField("director_enabled".to_string()))
        );
        assert_eq!(
            description.actions.get(SPAWN_ID),
            Some(&RowAction::Operation(RowOperation::Spawn))
        );
        assert_eq!(
            description.actions.get(MEMORY_OPEN_ID),
            Some(&RowAction::Operation(RowOperation::OpenMemory))
        );
    }

    #[test]
    fn consent_section_has_two_checkboxes() {
        let description = describe();
        let consent = description
            .sections
            .iter()
            .find(|s| s.heading == "What the buddy can see")
            .expect("Consent section exists");

        assert_eq!(consent.rows.len(), 2);
        let listed = crate::consent::process_listed_as();
        assert!(
            consent
                .comment
                .as_ref()
                .is_some_and(|c| { c.contains(&listed) && c.contains("Privacy & Security") }),
            "Consent section has to name the TCC row ({listed}), got {:?}",
            consent.comment
        );

        let accessibility = consent
            .rows
            .iter()
            .find(|r| matches!(r, FormRow::Checkbox { id, .. } if id == CONSENT_ACCESSIBILITY_ID))
            .expect("Accessibility checkbox exists");

        let screen_recording = consent
            .rows
            .iter()
            .find(
                |r| matches!(r, FormRow::Checkbox { id, .. } if id == CONSENT_SCREEN_RECORDING_ID),
            )
            .expect("Screen Recording checkbox exists");

        match accessibility {
            FormRow::Checkbox { label, help, .. } => {
                assert_eq!(label, "Accessibility");
                assert!(
                    help.as_ref().is_some_and(|h| h.contains("Dock")),
                    "Accessibility help should mention Dock"
                );
            }
            _ => panic!("Accessibility row must be a checkbox"),
        }

        match screen_recording {
            FormRow::Checkbox { label, help, .. } => {
                assert_eq!(label, "Screen Recording");
                assert!(
                    help.as_ref().is_some_and(|h| h.contains("title")),
                    "Screen Recording help should mention titles"
                );
            }
            _ => panic!("Screen Recording row must be a checkbox"),
        }

        assert_eq!(
            description.actions.get(CONSENT_ACCESSIBILITY_ID),
            Some(&RowAction::PatchField("use_accessibility".to_string()))
        );
        assert_eq!(
            description.actions.get(CONSENT_SCREEN_RECORDING_ID),
            Some(&RowAction::PatchField("use_screen_recording".to_string()))
        );
    }

    #[test]
    fn excluded_help_is_platform_specific() {
        let description = describe();
        let excluded = description
            .sections
            .iter()
            .find(|s| s.heading == "Excluded applications")
            .expect("Excluded applications section");

        let help = match &excluded.rows[0] {
            FormRow::Multiline { help, .. } => help.as_ref().expect("help text present"),
            _ => panic!("Excluded row must be Multiline"),
        };

        #[cfg(all(unix, not(target_os = "macos")))]
        {
            assert!(
                help.contains("WM_CLASS"),
                "Linux help must mention X11 WM_CLASS, got: {help}"
            );
        }

        #[cfg(not(all(unix, not(target_os = "macos"))))]
        {
            assert!(
                !help.contains("WM_CLASS"),
                "Non-Linux help must not mention WM_CLASS, got: {help}"
            );
        }
    }
}
