//! Settings form description: sections, rows, labels, and what they write.
//!
//! Split the same way the menu is: the form as data crosses the platform
//! boundary, and the AppKit window builds from that description. Linux and
//! Windows consume the same description when they ship, so labels cannot
//! drift.

use std::collections::HashMap;

use crate::consent;
use crate::dev_flags;
use crate::model;

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
        /// Read-only: the value shown is not the user's to change. True when
        /// an exported variable owns the field, since `model::resolve` gives
        /// it the last word and would discard an edit made here (#272).
        frozen: bool,
    },
    /// A secure text field for passwords/keys.
    SecureField {
        id: String,
        label: Option<String>,
        /// Read-only, for the same reason as `TextField::frozen`.
        frozen: bool,
    },
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
    TextField {
        id: String,
        placeholder: String,
    },
    Popup {
        id: String,
    },
    Button {
        id: String,
        label: String,
        /// Disabled, for the same reason as `FormRow::TextField::frozen`.
        frozen: bool,
    },
}

/// One tab of the settings form, holding the sections that belong together.
///
/// The grouping is data here rather than a layout decision in each renderer,
/// so AppKit's `NSTabView` and GTK's `gtk::Notebook` cannot disagree about
/// which heading sits where.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FormTab {
    pub title: String,
    pub sections: Vec<FormSection>,
}

/// The whole settings form as data: tabs, sections, rows, and what they write.
///
/// Everything here is owned, so this crosses a thread boundary. That is the
/// point of it: the description is built where the state lives, and the
/// platform window builds from it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FormDescription {
    pub tabs: Vec<FormTab>,
    pub actions: HashMap<String, RowAction>,
}

impl FormDescription {
    /// Every section, in tab order. For the parts of a renderer that want the
    /// rows and not the grouping, such as finding one row by id.
    // AppKit is the last caller: the GTK window builds a page per tab, so the
    // binary's dead-code lint sees no caller on the other targets.
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    pub fn sections(&self) -> impl Iterator<Item = &FormSection> + '_ {
        self.tabs.iter().flat_map(|tab| &tab.sections)
    }
}

/// Row ids for the settings form controls.
pub const DIRECTOR_ID: &str = "director";
pub const AMBIENT_ID: &str = "ambient";
pub const DIRECTOR_BASE_URL_ID: &str = "director_base_url";
pub const DIRECTOR_MODEL_ID: &str = "director_model";
pub const DIRECTOR_API_KEY_ID: &str = "director_api_key";
pub const CLEAR_KEY_ID: &str = "clear_key";
pub const DND_ID: &str = "dnd";
pub const SOUND_ID: &str = "sound";
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
pub const TRACE_FRAMES_ID: &str = "trace_frames";
pub const TRACE_HITTEST_ID: &str = "trace_hittest";
pub const TRACE_DIRECTOR_ID: &str = "trace_director";
#[cfg(target_os = "macos")]
pub const CAPTURABLE_ID: &str = "capturable";
pub const DIRECTOR_TIMEOUT_SECS_ID: &str = "director_timeout_secs";
pub const DIRECTOR_MAX_TOKENS_ID: &str = "director_max_tokens";

/// Platform-specific help text for excluded applications.
fn excluded_help() -> String {
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        "One app name per line, matched on WM_CLASS.".to_string()
    }
    #[cfg(not(all(unix, not(target_os = "macos"))))]
    {
        "One app name per line.".to_string()
    }
}

/// The label of a row an environment variable can own, and whether it does.
///
/// A frozen row says it is overridden and names the variable doing it, so both
/// the reason it takes no edit and the export to drop are on screen beside it.
/// The one ownership question, for every row a variable can own: the Director
/// endpoint, the Completer limits, and the development switches.
fn env_row(label: &str, var: &str) -> (String, bool) {
    match model::env_override(var) {
        Some(_) => (format!("{label} (overridden by env: {var})"), true),
        None => (label.to_string(), false),
    }
}

/// A checkbox for one development switch.
///
/// Frozen when the process exported the variable: `dev_flags::Flag::env_value`
/// gives the variable the value, so the click would change nothing.
fn flag_row(id: &str, flag: &dev_flags::Flag, label: &str, help: &str) -> FormRow {
    let (label, frozen) = env_row(label, flag.var());
    FormRow::Checkbox {
        id: id.to_string(),
        label,
        frozen,
        help: Some(help.to_string()),
        comment: None,
    }
}

fn director_sections() -> Vec<FormSection> {
    let (base_url_label, base_url_frozen) = env_row("Base URL", model::BASE_URL);
    let (model_label, model_frozen) = env_row("Model", model::MODEL);
    let (api_key_label, api_key_frozen) = env_row("API key", model::API_KEY);

    vec![
        FormSection {
            heading: "Director".to_string(),
            comment: None,
            rows: vec![
                FormRow::Checkbox {
                    id: DIRECTOR_ID.to_string(),
                    label: "Director on".to_string(),
                    frozen: false,
                    help: Some("The model picks what happens next.".to_string()),
                    comment: None,
                },
                FormRow::Checkbox {
                    id: AMBIENT_ID.to_string(),
                    label: "Ambient session wakes".to_string(),
                    frozen: false,
                    help: Some("Acts on its own, not only when asked.".to_string()),
                    comment: None,
                },
                FormRow::TextField {
                    id: DIRECTOR_BASE_URL_ID.to_string(),
                    label: Some(base_url_label),
                    placeholder: "https://api.openai.com".to_string(),
                    frozen: base_url_frozen,
                },
                FormRow::TextField {
                    id: DIRECTOR_MODEL_ID.to_string(),
                    label: Some(model_label),
                    placeholder: "gpt-4o-mini".to_string(),
                    frozen: model_frozen,
                },
                FormRow::SecureField {
                    id: DIRECTOR_API_KEY_ID.to_string(),
                    label: Some(api_key_label),
                    frozen: api_key_frozen,
                },
                FormRow::Composite {
                    id: "api_key_actions".to_string(),
                    // Clearing the store while a variable supplies the key
                    // would change nothing the Director can see (#272).
                    controls: vec![CompositeControl::Button {
                        id: CLEAR_KEY_ID.to_string(),
                        label: "Clear key".to_string(),
                        frozen: api_key_frozen,
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
                help: Some("The last thing sent to the model.".to_string()),
            }],
        },
    ]
}

fn character_sections() -> Vec<FormSection> {
    vec![
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
                            frozen: false,
                        },
                    ],
                },
            ],
        },
    ]
}

fn presence_sections() -> Vec<FormSection> {
    vec![
        FormSection {
            heading: "Do Not Disturb".to_string(),
            // Named for quiet rather than for hiding: Do Not Disturb leaves the
            // buddy on screen, and a Hide heading would teach the opposite.
            comment: None,
            rows: vec![
                FormRow::Checkbox {
                    id: DND_ID.to_string(),
                    label: "Do Not Disturb".to_string(),
                    frozen: false,
                    help: Some("Stays on screen. Stops starting things.".to_string()),
                    comment: None,
                },
                FormRow::Checkbox {
                    id: SOUND_ID.to_string(),
                    label: "Sound".to_string(),
                    frozen: false,
                    help: Some("Plays a sound on poke and summon.".to_string()),
                    comment: None,
                },
            ],
        },
        FormSection {
            heading: "Hide".to_string(),
            comment: None,
            rows: vec![
                FormRow::Checkbox {
                    id: HIDDEN_ID.to_string(),
                    label: "Go away".to_string(),
                    frozen: false,
                    help: Some("Off screen. Still running.".to_string()),
                    comment: None,
                },
                FormRow::Checkbox {
                    id: FULLSCREEN_ID.to_string(),
                    label: "Hide in fullscreen apps".to_string(),
                    frozen: false,
                    help: Some("Steps aside for fullscreen apps.".to_string()),
                    comment: None,
                },
                FormRow::InspectBlock {
                    id: HOTKEY_ID.to_string(),
                    label: Some("Hide/Show Toggle".to_string()),
                    help: Some("Hides or shows from any app.".to_string()),
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
                // No installed app on any OS yet, so there is nothing for the
                // system to start: a Launch Agent pointing at `cargo run` is
                // not launch-at-login.
                help: Some("Not available yet.".to_string()),
                comment: None,
            }],
        },
    ]
}

fn privacy_sections() -> Vec<FormSection> {
    vec![
        FormSection {
            heading: "What the buddy can see".to_string(),
            comment: Some(consent::pane_intro(&consent::process_listed_as())),
            rows: vec![
                FormRow::Checkbox {
                    id: CONSENT_ACCESSIBILITY_ID.to_string(),
                    label: "Accessibility".to_string(),
                    frozen: false,
                    help: Some("Reads the Dock's position.".to_string()),
                    comment: None,
                },
                FormRow::Checkbox {
                    id: CONSENT_SCREEN_RECORDING_ID.to_string(),
                    label: "Screen Recording".to_string(),
                    frozen: false,
                    help: Some("Reads window titles.".to_string()),
                    comment: None,
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
            heading: "Memory File".to_string(),
            comment: Some("What your buddy remembers between runs.".to_string()),
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
                            frozen: false,
                        },
                        CompositeControl::Button {
                            id: MEMORY_WIPE_ID.to_string(),
                            label: "Wipe".to_string(),
                            frozen: false,
                        },
                    ],
                },
            ],
        },
    ]
}

fn development_sections() -> Vec<FormSection> {
    let rows = vec![
        flag_row(
            TRACE_FRAMES_ID,
            &dev_flags::TRACE_FRAMES,
            "Trace frames",
            "Prints each frame.",
        ),
        flag_row(
            TRACE_HITTEST_ID,
            &dev_flags::TRACE_HITTEST,
            "Trace hit-test",
            "Prints where each click went.",
        ),
        flag_row(
            TRACE_DIRECTOR_ID,
            &dev_flags::TRACE_DIRECTOR,
            "Trace Director",
            "Prints each model call.",
        ),
        // Only AppKit has a capture exclusion to drop. Gating the element
        // rather than pushing it keeps the binding immutable on the platforms
        // that skip it, which `-D warnings` insists on.
        #[cfg(target_os = "macos")]
        flag_row(
            CAPTURABLE_ID,
            &dev_flags::CAPTURABLE,
            "Capturable",
            // `configure_overlay` reads this when a window is built, and only
            // the Linux frame loop re-runs it. Honest rather than silently
            // inert.
            "Shows in screenshots. Needs a restart.",
        ),
    ];

    let (timeout_label, timeout_frozen) = env_row("Timeout, in seconds", model::TIMEOUT_SECS);
    let (max_tokens_label, max_tokens_frozen) = env_row("Reply cap, in tokens", model::MAX_TOKENS);

    vec![
        FormSection {
            heading: "Traces".to_string(),
            comment: Some("Switches for development and testing.".to_string()),
            rows,
        },
        FormSection {
            heading: "Completer limits".to_string(),
            comment: Some("Also for development and testing. Blank uses the default.".to_string()),
            rows: vec![
                FormRow::TextField {
                    id: DIRECTOR_TIMEOUT_SECS_ID.to_string(),
                    label: Some(timeout_label),
                    placeholder: model::timeout_placeholder(),
                    frozen: timeout_frozen,
                },
                FormRow::TextField {
                    id: DIRECTOR_MAX_TOKENS_ID.to_string(),
                    label: Some(max_tokens_label),
                    placeholder: model::max_tokens_placeholder(),
                    frozen: max_tokens_frozen,
                },
            ],
        },
    ]
}

/// Describe the settings form. The AppKit and Linux GTK windows build from this.
pub fn describe() -> FormDescription {
    let tabs = vec![
        FormTab {
            title: "Presence".to_string(),
            sections: presence_sections(),
        },
        FormTab {
            title: "Character".to_string(),
            sections: character_sections(),
        },
        FormTab {
            title: "Director".to_string(),
            sections: director_sections(),
        },
        FormTab {
            title: "Privacy".to_string(),
            sections: privacy_sections(),
        },
        FormTab {
            title: "Development".to_string(),
            sections: development_sections(),
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
        SOUND_ID.to_string(),
        RowAction::PatchField("sound".to_string()),
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
        TRACE_FRAMES_ID.to_string(),
        RowAction::PatchField("trace_frames".to_string()),
    );
    actions.insert(
        TRACE_HITTEST_ID.to_string(),
        RowAction::PatchField("trace_hittest".to_string()),
    );
    actions.insert(
        TRACE_DIRECTOR_ID.to_string(),
        RowAction::PatchField("trace_director".to_string()),
    );
    #[cfg(target_os = "macos")]
    actions.insert(
        CAPTURABLE_ID.to_string(),
        RowAction::PatchField("capturable".to_string()),
    );
    actions.insert(
        DIRECTOR_TIMEOUT_SECS_ID.to_string(),
        RowAction::PatchField("director_timeout_secs".to_string()),
    );
    actions.insert(
        DIRECTOR_MAX_TOKENS_ID.to_string(),
        RowAction::PatchField("director_max_tokens".to_string()),
    );
    actions.insert(
        CONSENT_ACCESSIBILITY_ID.to_string(),
        RowAction::PatchField("use_accessibility".to_string()),
    );
    actions.insert(
        CONSENT_SCREEN_RECORDING_ID.to_string(),
        RowAction::PatchField("use_screen_recording".to_string()),
    );

    FormDescription { tabs, actions }
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
        let moved = std::thread::spawn(move || description.sections().count());
        assert!(moved.join().expect("thread panicked") > 0);
    }

    /// A heading that lost its tab, or landed in two, is the failure this
    /// catches.
    #[test]
    fn every_section_is_on_exactly_one_tab() {
        let description = describe();
        let mut headings: Vec<&str> = description
            .sections()
            .map(|section| section.heading.as_str())
            .collect();
        let placed = headings.len();
        headings.sort_unstable();
        headings.dedup();
        assert_eq!(placed, headings.len(), "a heading is on two tabs");

        let mut expected = vec![
            "Character",
            "Completer limits",
            "Director",
            "Do Not Disturb",
            "Excluded applications",
            "Hide",
            "Instances",
            "Last user turn",
            "Launch",
            "Memory File",
            "Traces",
            "What the buddy can see",
        ];
        expected.sort_unstable();
        assert_eq!(headings, expected);
    }

    #[test]
    fn every_tab_has_a_section() {
        let description = describe();
        let titles: Vec<&str> = description
            .tabs
            .iter()
            .map(|tab| tab.title.as_str())
            .collect();
        assert_eq!(
            titles,
            vec![
                "Presence",
                "Character",
                "Director",
                "Privacy",
                "Development"
            ]
        );
        for tab in &description.tabs {
            assert!(!tab.sections.is_empty(), "{} has no section", tab.title);
        }
    }

    #[test]
    fn launch_row_is_frozen() {
        let description = describe();
        let launch_section = description
            .sections()
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

    /// Under the env lock, which clears every Development variable: a shell
    /// that exported one freezes its row, and excluding those rows instead
    /// would leave the one test that can catch a stray `frozen: true` blind
    /// to the whole tab.
    #[test]
    fn no_other_frozen_checkboxes() {
        crate::model::tests::with_env(None, None, None, || {
            let description = describe();
            let frozen_rows: Vec<&String> = description
                .sections()
                .flat_map(|s| &s.rows)
                .filter_map(|r| match r {
                    FormRow::Checkbox { id, frozen, .. } if *frozen => Some(id),
                    _ => None,
                })
                .collect();

            assert_eq!(
                frozen_rows,
                vec![LAUNCH_ID],
                "only Launch should be a frozen checkbox"
            );
        });
    }

    /// Whether the control carrying this id writes a bool, or `None` when no
    /// writing control claims it.
    ///
    /// Composite controls count: a `PatchField` registered against one is
    /// dispatched by the same two setters, so leaving them out would let the
    /// next composite text field ship inert.
    fn writes_bool(description: &FormDescription, id: &str) -> Option<bool> {
        for row in description.sections().flat_map(|section| &section.rows) {
            match row {
                FormRow::Checkbox { id: row_id, .. } if row_id == id => return Some(true),
                FormRow::TextField { id: row_id, .. }
                | FormRow::SecureField { id: row_id, .. }
                | FormRow::Multiline { id: row_id, .. }
                | FormRow::Popup { id: row_id, .. }
                    if row_id == id =>
                {
                    return Some(false)
                }
                FormRow::Composite { controls, .. } => {
                    for control in controls {
                        match control {
                            CompositeControl::TextField { id: c_id, .. }
                            | CompositeControl::Popup { id: c_id, .. }
                                if c_id == id =>
                            {
                                return Some(false)
                            }
                            _ => {}
                        }
                    }
                }
                _ => {}
            }
        }
        None
    }

    /// Every registered `PatchField` has to reach that field through the
    /// setter its own control calls.
    ///
    /// The mapping is by name, so a field name no setter knows compiles clean
    /// and ships a control that writes nothing. Each renderer picks the setter
    /// from the control kind — `set_bool` for a `Checkbox`, `set_text` for a
    /// field — and returns when it refuses the name, so a name only the other
    /// setter knows lands just as inert (#273).
    ///
    /// Walking the actions map rather than the rows is what catches a field
    /// registered against an id no writing control carries, which reaches
    /// neither setter and so cannot be caught by looking at rows alone.
    #[test]
    fn every_patch_field_reaches_the_setter_its_control_calls() {
        let description = describe();
        let mut checked = 0;
        for (id, action) in &description.actions {
            let RowAction::PatchField(name) = action else {
                continue;
            };
            let Some(writes_bool) = writes_bool(&description, id) else {
                panic!("{id} writes {name}, but no writing control carries that id");
            };
            let mut patch = crate::settings::SettingsPatch::default();
            let (took, setter) = if writes_bool {
                (patch.set_bool(name, true), "set_bool")
            } else {
                (patch.set_text(name, "value"), "set_text")
            };
            assert!(
                took,
                "{id} writes {name}, which SettingsPatch::{setter} — the setter its control calls — does not know"
            );
            checked += 1;
        }
        assert!(checked > 0, "no writing control was checked");
    }

    fn described_row(description: &FormDescription, id: &str) -> (String, bool) {
        description
            .sections()
            .flat_map(|section| &section.rows)
            .find_map(|row| match row {
                FormRow::TextField {
                    id: row_id,
                    label,
                    frozen,
                    ..
                } if row_id == id => Some((label.clone().unwrap_or_default(), *frozen)),
                FormRow::SecureField {
                    id: row_id,
                    label,
                    frozen,
                } if row_id == id => Some((label.clone().unwrap_or_default(), *frozen)),
                _ => None,
            })
            .expect("the endpoint row exists")
    }

    const ENDPOINT_ROWS: [(&str, &str); 3] = [
        (DIRECTOR_BASE_URL_ID, crate::model::BASE_URL),
        (DIRECTOR_MODEL_ID, crate::model::MODEL),
        (DIRECTOR_API_KEY_ID, crate::model::API_KEY),
    ];

    /// #272: `model::resolve` gives the env the last word, so a field the env
    /// owns cannot be offered as editable — the window took the edit and the
    /// Director ignored it. Described here so both windows inherit it.
    #[test]
    fn an_env_owned_endpoint_row_is_read_only_and_names_its_variable() {
        crate::model::tests::with_env(
            Some("sk-env-key"),
            Some("https://api.x.ai"),
            Some("grok-4.6"),
            || {
                let description = describe();
                for (id, var) in ENDPOINT_ROWS {
                    let (label, frozen) = described_row(&description, id);
                    assert!(frozen, "{id} must not accept an edit the env discards");
                    assert!(
                        label.contains("(overridden by env"),
                        "{id} must say it is overridden, not {label:?}"
                    );
                    assert!(label.contains(var), "{id} must name {var}, not {label:?}");
                }
            },
        );
    }

    #[test]
    fn endpoint_rows_are_editable_when_no_variable_is_exported() {
        crate::model::tests::with_env(None, None, None, || {
            let description = describe();
            for (id, var) in ENDPOINT_ROWS {
                let (label, frozen) = described_row(&description, id);
                assert!(!frozen, "{id} is the user's to edit when the env is unset");
                assert!(!label.contains(var), "{id} must not mention {var}");
            }
        });
    }

    #[test]
    fn director_section_has_two_checkboxes() {
        let description = describe();
        let director = description
            .sections()
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

    /// Mute sits under Do Not Disturb because that is the heading a user
    /// reads as "quieter", and DND takes the sound with it (#277).
    #[test]
    fn do_not_disturb_section_has_dnd_then_sound() {
        let description = describe();
        let section = description
            .sections()
            .find(|s| s.heading == "Do Not Disturb")
            .expect("Do Not Disturb section");

        assert_eq!(section.rows.len(), 2);
        assert!(matches!(
            section.rows[0],
            FormRow::Checkbox { ref id, .. } if id == DND_ID
        ));
        assert!(matches!(
            section.rows[1],
            FormRow::Checkbox { ref id, .. } if id == SOUND_ID
        ));
    }

    #[test]
    fn character_section_has_popup() {
        let description = describe();
        let character = description
            .sections()
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
            .sections()
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
            .sections()
            .find(|s| s.heading == "Memory File")
            .expect("Memory File section");

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
            .sections()
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
                .sections()
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
        assert!(has_help("Do Not Disturb", SOUND_ID));
        assert!(has_help("Excluded applications", EXCLUDED_ID));
    }

    #[test]
    fn every_control_has_an_action() {
        let description = describe();

        for section in description.sections() {
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
            description.actions.get(SOUND_ID),
            Some(&RowAction::PatchField("sound".to_string()))
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
            .sections()
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
            .sections()
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
    fn development_tab(description: &FormDescription) -> &FormTab {
        description
            .tabs
            .iter()
            .find(|tab| tab.title == "Development")
            .expect("the Development tab exists")
    }

    fn row_id(row: &FormRow) -> Option<&str> {
        match row {
            FormRow::Checkbox { id, .. } | FormRow::TextField { id, .. } => Some(id.as_str()),
            _ => None,
        }
    }

    /// Every section, not the first one: acceptance box 3 asks for a warning
    /// on the tab, and a reader who scrolls to a later heading has left the
    /// first section's comment behind (#273).
    #[test]
    fn the_development_tab_warns_and_registers_every_row() {
        let description = describe();
        let tab = development_tab(&description);

        for section in &tab.sections {
            let warning = section
                .comment
                .as_ref()
                .unwrap_or_else(|| panic!("{} carries no warning", section.heading));
            assert!(
                warning.contains("for development and testing"),
                "{} has to say what its rows are for, got {warning:?}",
                section.heading
            );
            for row in &section.rows {
                let id = row_id(row).expect("every development row is a control");
                assert!(
                    description.actions.contains_key(id),
                    "development row {id} has no action"
                );
            }
        }
    }

    /// A switch the env owns names the variable it answers to, so the export
    /// to drop is on screen, and takes no click.
    ///
    /// Every exported value freezes the row, `0` included, which is the
    /// ownership half of `dev_flags::Flag::env_value`'s convention.
    #[test]
    fn an_env_owned_flag_row_is_frozen_and_names_its_variable() {
        crate::model::tests::with_env(None, None, None, || {
            let var = dev_flags::TRACE_FRAMES.var();
            for exported in [Some("1"), Some("0"), Some("true"), None] {
                match exported {
                    Some(value) => std::env::set_var(var, value),
                    None => std::env::remove_var(var),
                }
                let description = describe();

                let row = development_tab(&description)
                    .sections
                    .iter()
                    .flat_map(|section| &section.rows)
                    .find(|row| row_id(row) == Some(TRACE_FRAMES_ID))
                    .expect("the trace-frames row exists");

                match row {
                    FormRow::Checkbox { label, frozen, .. } => {
                        assert_eq!(
                            *frozen,
                            exported.is_some(),
                            "exported {exported:?} decides the click"
                        );
                        assert_eq!(
                            label.contains(var),
                            exported.is_some(),
                            "exported {exported:?} decides the label, got {label:?}"
                        );
                    }
                    _ => panic!("the trace-frames row is a checkbox"),
                }
            }
            std::env::remove_var(var);
        });
    }

    #[test]
    fn the_completer_limits_show_their_defaults_as_placeholders() {
        let description = describe();
        let rows: Vec<&FormRow> = development_tab(&description)
            .sections
            .iter()
            .flat_map(|section| &section.rows)
            .filter(|row| {
                matches!(
                    row_id(row),
                    Some(DIRECTOR_TIMEOUT_SECS_ID) | Some(DIRECTOR_MAX_TOKENS_ID)
                )
            })
            .collect();

        assert_eq!(rows.len(), 2, "both limits are on the tab");
        for row in rows {
            match row {
                FormRow::TextField {
                    id, placeholder, ..
                } => {
                    assert!(
                        !placeholder.is_empty(),
                        "{id} needs a placeholder, so blank reads as the default"
                    );
                }
                _ => panic!("a limit is a text field"),
            }
        }
    }
}
