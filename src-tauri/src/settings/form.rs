//! Settings form description: sections, rows, labels, and what they write.
//!
//! Split the same way the menu is: the form as data crosses the platform
//! boundary, and the AppKit window builds from that description. Linux and
//! Windows consume the same description when they ship, so labels cannot
//! drift.

/// One section of the settings form.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FormSection {
    pub heading: String,
    pub rows: Vec<FormRow>,
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
    },
    /// An inspect-only text field showing current state.
    InspectLine {
        id: String,
        label: Option<String>,
    },
    /// An inspect-only text block showing current state.
    InspectBlock {
        id: String,
        label: Option<String>,
        help: Option<String>,
    },
    /// A popup menu for choosing between options.
    Popup {
        id: String,
        label: Option<String>,
    },
    /// A multiline text field that writes to Settings.
    Multiline {
        id: String,
        label: Option<String>,
        help: Option<String>,
        editable: bool,
    },
    /// A button that performs an action.
    Button {
        id: String,
        label: String,
    },
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

/// A scrollable list of items (e.g., instances).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ListRow {
    pub id: String,
}

/// The whole settings form as data: sections, rows, and help text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FormDescription {
    pub sections: Vec<FormSection>,
}

/// Row ids for the settings form controls.
pub const DIRECTOR_ID: &str = "director";
pub const AMBIENT_ID: &str = "ambient";
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
pub const LAUNCH_ID: &str = "launch";

/// Describe the settings form. The AppKit window builds from this.
pub fn describe() -> FormDescription {
    let mut sections = Vec::new();

    sections.push(FormSection {
        heading: "Director".to_string(),
        rows: vec![
            FormRow::Checkbox {
                id: DIRECTOR_ID.to_string(),
                label: "Director on".to_string(),
                frozen: false,
                help: Some("Off leaves Static weights running the life. No session calls.".to_string()),
            },
            FormRow::Checkbox {
                id: AMBIENT_ID.to_string(),
                label: "Ambient session wakes".to_string(),
                frozen: false,
                help: Some("Off keeps Poke and Summon on the session path. Idle life stays Static.".to_string()),
            },
        ],
    });

    sections.push(FormSection {
        heading: "Last user turn".to_string(),
        rows: vec![
            FormRow::InspectBlock {
                id: PAYLOAD_ID.to_string(),
                label: None,
                help: Some("Inspect only. The last session turn, opening Character Prompt or follow-up.".to_string()),
            },
        ],
    });

    sections.push(FormSection {
        heading: "Character".to_string(),
        rows: vec![
            FormRow::Popup {
                id: CHARACTER_ID.to_string(),
                label: None,
            },
        ],
    });

    sections.push(FormSection {
        heading: "Instances".to_string(),
        rows: vec![
            FormRow::InspectLine {
                id: INSTANCES_ID.to_string(),
                label: None,
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
    });

    sections.push(FormSection {
        heading: "Do Not Disturb".to_string(),
        rows: vec![
            FormRow::Checkbox {
                id: DND_ID.to_string(),
                label: "Do Not Disturb".to_string(),
                frozen: false,
                help: Some("On screen, not starting things.".to_string()),
            },
        ],
    });

    sections.push(FormSection {
        heading: "Hide".to_string(),
        rows: vec![
            FormRow::Checkbox {
                id: HIDDEN_ID.to_string(),
                label: "Go away".to_string(),
                frozen: false,
                help: None,
            },
            FormRow::Checkbox {
                id: FULLSCREEN_ID.to_string(),
                label: "Hide in fullscreen apps".to_string(),
                frozen: false,
                help: None,
            },
            FormRow::InspectLine {
                id: HOTKEY_ID.to_string(),
                label: Some("Hotkey".to_string()),
            },
        ],
    });

    sections.push(FormSection {
        heading: "Memory".to_string(),
        rows: vec![
            FormRow::InspectLine {
                id: MEMORY_PATH_ID.to_string(),
                label: None,
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
    });

    sections.push(FormSection {
        heading: "Excluded applications".to_string(),
        rows: vec![
            FormRow::Multiline {
                id: EXCLUDED_ID.to_string(),
                label: None,
                help: Some("One application name per line. Those windows stay out of MCP sensing, and the Director is not told they are frontmost. The buddy can still sit on them.".to_string()),
                editable: true,
            },
        ],
    });

    sections.push(FormSection {
        heading: "Launch".to_string(),
        rows: vec![
            FormRow::Checkbox {
                id: LAUNCH_ID.to_string(),
                label: "Launch at login (unimplemented)".to_string(),
                frozen: true,
                help: None,
            },
        ],
    });

    FormDescription { sections }
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

        assert_eq!(
            frozen_rows,
            vec![LAUNCH_ID],
            "only Launch should be frozen"
        );
    }

    #[test]
    fn director_section_has_two_checkboxes() {
        let description = describe();
        let director = description
            .sections
            .iter()
            .find(|s| s.heading == "Director")
            .expect("Director section");

        assert_eq!(director.rows.len(), 2);
        assert!(matches!(
            director.rows[0],
            FormRow::Checkbox { ref id, .. } if id == DIRECTOR_ID
        ));
        assert!(matches!(
            director.rows[1],
            FormRow::Checkbox { ref id, .. } if id == AMBIENT_ID
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
        assert!(matches!(
            instances.rows[0],
            FormRow::InspectLine { ref id, .. } if id == INSTANCES_ID
        ));
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
        assert!(matches!(
            memory.rows[0],
            FormRow::InspectLine { ref id, .. } if id == MEMORY_PATH_ID
        ));
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
                            id == row_id && help.as_ref().map_or(false, |h| !h.is_empty())
                        }
                        FormRow::InspectBlock { id, help, .. } => {
                            id == row_id && help.as_ref().map_or(false, |h| !h.is_empty())
                        }
                        FormRow::Multiline { id, help, .. } => {
                            id == row_id && help.as_ref().map_or(false, |h| !h.is_empty())
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
}
