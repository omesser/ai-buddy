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
use crate::settings::{BoolField, TextField};

/// Operations the settings window requests.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RowOperation {
    Spawn,
    OpenMemory,
    WipeMemory,
    ClearKey,
    /// Send the whole Director tab as one patch.
    Apply,
    /// Redraw the Director tab from live state, writing nothing.
    Cancel,
}

/// One section of the settings form.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FormSection {
    pub heading: String,
    pub rows: Vec<FormRow>,
    pub comment: Option<String>,
}

/// One row of the settings form, as data.
///
/// A row that writes carries the field it writes, so its kind and its field
/// have to agree: a `Checkbox` can only name a bool. That is what makes a
/// control writing nothing unrepresentable rather than merely tested (#287).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FormRow {
    /// A checkbox that writes a bool to Settings.
    Checkbox {
        id: String,
        label: String,
        writes: BoolField,
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
    Popup {
        id: String,
        label: Option<String>,
        writes: TextField,
        help: Option<String>,
        /// The choices, when the form knows them. Empty leaves them to the
        /// renderer, which is how the Character popup gets the installed
        /// packages — a list the form cannot see.
        options: Vec<String>,
        /// Read-only, for the same reason as `TextField::frozen`.
        frozen: bool,
    },
    /// A multiline text field that writes to Settings.
    Multiline {
        id: String,
        label: Option<String>,
        writes: TextField,
        help: Option<String>,
        editable: bool,
    },
    /// An editable text field that writes a string to Settings.
    TextField {
        id: String,
        label: Option<String>,
        placeholder: String,
        writes: TextField,
        /// Read-only: the value shown is not the user's to change. True when
        /// an exported variable owns the field, since `model::resolve` gives
        /// it the last word and would discard an edit made here (#272).
        frozen: bool,
        /// Committed by Apply rather than on every blur.
        ///
        /// Declared here so neither renderer decides it for itself. The
        /// Director's four controls only mean anything together: committing
        /// one at a time points the Completer at a host and model that were
        /// never meant to go together, and every commit drops the in-flight
        /// session history with it (#279).
        batched: bool,
    },
    /// A secure text field for passwords/keys.
    ///
    /// Always batched, so it carries no flag of its own. A secret cannot be
    /// compared to the file, so a secure field committed on blur retargets
    /// every single time — which is the cost `TextField::batched` exists to
    /// avoid, and there is no value of it that makes a blur-committed key
    /// correct (#279).
    SecureField {
        id: String,
        label: Option<String>,
        writes: TextField,
        /// Read-only, for the same reason as `TextField::frozen`.
        frozen: bool,
    },
    /// A scrollable list of items with dismiss buttons.
    List {
        id: String,
        dismiss_label: String,
        help: Option<String>,
    },
    /// A row of multiple controls (e.g., new instance spawn row).
    Composite {
        id: String,
        controls: Vec<CompositeControl>,
        help: Option<String>,
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
    /// What each button does, by control id. Only buttons: a writing row
    /// carries its own field, so no registry can disagree with one.
    pub operations: HashMap<String, RowOperation>,
}

impl FormDescription {
    /// Every section, in tab order. For the parts of a renderer that want the
    /// rows and not the grouping, such as finding one row by id.
    pub fn sections(&self) -> impl Iterator<Item = &FormSection> + '_ {
        self.tabs.iter().flat_map(|tab| &tab.sections)
    }

    /// Whether the control carrying this id is the environment's rather than
    /// the user's. False for an id no control with a `frozen` field carries.
    ///
    /// Both windows ask this before they read a batched field: a frozen row is
    /// never dirty and never applies, because `model::resolve` would discard
    /// the edit (#272). Asked of the description rather than remembered beside
    /// the widget, so the two answers cannot drift.
    pub fn frozen(&self, id: &str) -> bool {
        self.sections()
            .flat_map(|section| &section.rows)
            .find_map(|row| match row {
                FormRow::Checkbox {
                    id: row_id, frozen, ..
                }
                | FormRow::TextField {
                    id: row_id, frozen, ..
                }
                | FormRow::SecureField {
                    id: row_id, frozen, ..
                }
                | FormRow::Popup {
                    id: row_id, frozen, ..
                } if row_id == id => Some(*frozen),
                FormRow::Composite { controls, .. } => {
                    controls.iter().find_map(|control| match control {
                        CompositeControl::Button {
                            id: control_id,
                            frozen,
                            ..
                        } if control_id == id => Some(*frozen),
                        _ => None,
                    })
                }
                _ => None,
            })
            .unwrap_or(false)
    }

    /// The boolean field the checkbox with this id writes.
    ///
    /// For a renderer holding an id and needing the field — AppKit reaches a
    /// control through the tag the click carries, not through the row.
    // GTK captures the field where it builds the control, and Windows builds
    // no settings window, so the binary's dead-code lint sees no caller there.
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    pub fn bool_write(&self, id: &str) -> Option<BoolField> {
        self.sections()
            .flat_map(|section| &section.rows)
            .find_map(|row| match row {
                FormRow::Checkbox {
                    id: row_id, writes, ..
                } if row_id == id => Some(*writes),
                _ => None,
            })
    }

    /// The text field the row with this id writes.
    pub fn text_write(&self, id: &str) -> Option<TextField> {
        self.sections()
            .flat_map(|section| &section.rows)
            .find_map(|row| match row {
                FormRow::TextField {
                    id: row_id, writes, ..
                }
                | FormRow::SecureField {
                    id: row_id, writes, ..
                }
                | FormRow::Multiline {
                    id: row_id, writes, ..
                }
                | FormRow::Popup {
                    id: row_id, writes, ..
                } if row_id == id => Some(*writes),
                _ => None,
            })
    }
}

/// Row ids for the settings form controls.
pub const DIRECTOR_ID: &str = "director";
pub const AMBIENT_ID: &str = "ambient";
pub const DIRECTOR_BASE_URL_ID: &str = "director_base_url";
pub const DIRECTOR_MODEL_ID: &str = "director_model";
pub const DIRECTOR_API_KEY_ID: &str = "director_api_key";
pub const CLEAR_KEY_ID: &str = "clear_key";
pub const APPLY_ID: &str = "director_apply";
pub const CANCEL_ID: &str = "director_cancel";
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
pub const TRACE_ENGINE_ID: &str = "trace_engine";
#[cfg(target_os = "macos")]
pub const CAPTURABLE_ID: &str = "capturable";
pub const DIRECTOR_TIMEOUT_SECS_ID: &str = "director_timeout_secs";
pub const DIRECTOR_MAX_TOKENS_ID: &str = "director_max_tokens";
pub const DIRECTOR_WAKE_SECS_ID: &str = "director_wake_secs";
pub const HARNESS_ID: &str = "harness";
pub const HARNESS_COMMAND_ID: &str = "harness_command";
pub const HARNESS_STATE_ID: &str = "harness_state";

/// The two Completer-source titles the file does not spell the same way: Off
/// is the empty string and Custom is `custom`, which defers to the command
/// line beside it.
pub const HARNESS_OFF: &str = "Off";
pub const HARNESS_CUSTOM: &str = "Custom";
/// What `harness_choice` writes for Custom. Not a value `AI_BUDDY_HARNESS`
/// can take, so it cannot collide with a Harness of that name.
pub const HARNESS_CUSTOM_VALUE: &str = "custom";
/// The named launch rows, in ADR-0017's order. Grok, Copilot and Gemini reach
/// the same Completer through Custom until a turn has been smoked.
pub const HARNESS_PRESETS: [&str; 3] = ["claude", "hermes", "opencode"];

/// The Completer-source popup's choices, in the order it draws them.
pub fn harness_options() -> Vec<String> {
    let mut options = vec![HARNESS_OFF.to_string()];
    options.extend(HARNESS_PRESETS.iter().map(|name| name.to_string()));
    options.push(HARNESS_CUSTOM.to_string());
    options
}

/// The popup title for a source in force, and the command line that belongs
/// beside it.
///
/// One function because the two rows are one choice spelled two ways:
/// `AI_BUDDY_HARNESS` puts a custom command line in the value itself, while
/// the file keeps `custom` plus a field of its own, so picking a preset and
/// coming back does not lose what was typed (#436).
pub fn harness_rows(value: &str, command: &str) -> (String, String) {
    match value.trim() {
        "" => (HARNESS_OFF.to_string(), command.to_string()),
        HARNESS_CUSTOM_VALUE => (HARNESS_CUSTOM.to_string(), command.to_string()),
        preset if HARNESS_PRESETS.contains(&preset) => (preset.to_string(), command.to_string()),
        line => (HARNESS_CUSTOM.to_string(), line.to_string()),
    }
}

/// What a popup title means in the file. The inverse of `harness_rows`.
pub fn harness_choice(title: &str) -> String {
    match title {
        HARNESS_OFF => String::new(),
        HARNESS_CUSTOM => HARNESS_CUSTOM_VALUE.to_string(),
        preset => preset.to_string(),
    }
}

/// The label of a row an environment variable can own, and whether it does.
///
/// A frozen row says it is overridden and names the variable doing it, so both
/// the reason it takes no edit and the export to drop are on screen beside it.
/// The ownership question for a row holding text: the Director endpoint and
/// the Completer limits, which take any value the process exports.
fn env_row(label: &str, var: &str) -> (String, bool) {
    owned_row(label, var, model::env_override(var).is_some())
}

/// The same question for a row holding a switch, which answers to a narrower
/// set of values: one `model::env_switch` cannot read owns nothing, so the row
/// stays the user's rather than freezing over a value nobody obeyed.
fn switch_row(label: &str, var: &str) -> (String, bool) {
    owned_row(label, var, model::env_switch(var).is_some())
}

/// One wording for both, so a frozen row reads the same wherever it is drawn.
fn owned_row(label: &str, var: &str, owned: bool) -> (String, bool) {
    match owned {
        true => (format!("{label} (overridden by env: {var})"), true),
        false => (label.to_string(), false),
    }
}

/// One of the three HTTP rows, which answer to an attachment as well as to a
/// variable.
///
/// An attached Harness *is* the Completer (ADR-0008), so these three drive
/// nothing while one is up, and #272's rule applies for the same reason it
/// applies to an exported variable: an edit the Director would discard is not
/// an edit to offer. The source row below is never frozen by an attachment,
/// so Off stays one pick and one launch away.
fn http_row(label: &str, var: &str, attached: bool) -> (String, bool) {
    match attached {
        true => (
            format!("{label} (not in use: a Harness is the Completer)"),
            true,
        ),
        false => env_row(label, var),
    }
}

/// A checkbox for one development switch.
///
/// Frozen when the exported value is one `model::env_switch` reads: that
/// value is the switch, so the click would change nothing.
fn flag_row(
    id: &str,
    flag: &dev_flags::Flag,
    writes: BoolField,
    label: &str,
    help: &str,
) -> FormRow {
    let (label, frozen) = switch_row(label, flag.var());
    FormRow::Checkbox {
        id: id.to_string(),
        label,
        writes,
        frozen,
        help: Some(help.to_string()),
        comment: None,
    }
}

fn director_sections() -> Vec<FormSection> {
    let attached = crate::harness::attached().is_some();
    let (base_url_label, base_url_frozen) = http_row("Base URL", model::BASE_URL, attached);
    let (model_label, model_frozen) = http_row("Model", model::MODEL, attached);
    let (api_key_label, api_key_frozen) = http_row("API key", model::API_KEY, attached);
    let (director_label, director_frozen) = switch_row("Director on", model::ENABLED);
    let (wake_label, wake_frozen) = env_row("First wake, in seconds", model::WAKE_SECS);

    vec![
        FormSection {
            heading: "Director".to_string(),
            comment: None,
            rows: vec![
                FormRow::Checkbox {
                    id: DIRECTOR_ID.to_string(),
                    label: director_label,
                    writes: BoolField::DirectorEnabled,
                    frozen: director_frozen,
                    help: Some("The model picks what happens next.".to_string()),
                    comment: None,
                },
                FormRow::Checkbox {
                    id: AMBIENT_ID.to_string(),
                    label: "Ambient session wakes".to_string(),
                    writes: BoolField::AmbientWakes,
                    frozen: false,
                    help: Some("Acts on its own, not only when asked.".to_string()),
                    comment: None,
                },
                // Not batched, and above the Apply the three endpoint rows
                // answer to: this one is a number the user turns to watch the
                // buddy get chattier or quieter, and a button between the
                // change and its effect would undo that (#262).
                FormRow::TextField {
                    id: DIRECTOR_WAKE_SECS_ID.to_string(),
                    label: Some(wake_label),
                    placeholder: model::wake_secs_placeholder(),
                    writes: TextField::DirectorWakeSecs,
                    frozen: wake_frozen,
                    batched: false,
                },
                FormRow::TextField {
                    id: DIRECTOR_BASE_URL_ID.to_string(),
                    label: Some(base_url_label),
                    placeholder: "https://api.openai.com".to_string(),
                    writes: TextField::DirectorBaseUrl,
                    frozen: base_url_frozen,
                    batched: true,
                },
                FormRow::TextField {
                    id: DIRECTOR_MODEL_ID.to_string(),
                    label: Some(model_label),
                    placeholder: "gpt-4o-mini".to_string(),
                    writes: TextField::DirectorModel,
                    frozen: model_frozen,
                    batched: true,
                },
                FormRow::SecureField {
                    id: DIRECTOR_API_KEY_ID.to_string(),
                    label: Some(api_key_label),
                    writes: TextField::DirectorApiKey,
                    frozen: api_key_frozen,
                },
                FormRow::Composite {
                    id: "api_key_actions".to_string(),
                    help: None,
                    // Clearing the store while a variable supplies the key
                    // would change nothing the Director can see (#272).
                    controls: vec![CompositeControl::Button {
                        id: CLEAR_KEY_ID.to_string(),
                        label: "Clear key".to_string(),
                        frozen: api_key_frozen,
                    }],
                },
                // A row of their own rather than beside Clear key: these two
                // answer for the four rows above, and Clear key is one of the
                // four. Never frozen, because Cancel has to stay reachable
                // even when a variable owns every field it would restore.
                //
                // The help says which rows, now that the wake interval sits
                // above them and commits on its own.
                FormRow::Composite {
                    id: "director_actions".to_string(),
                    help: Some("The endpoint rows take effect on Apply.".to_string()),
                    controls: vec![
                        CompositeControl::Button {
                            id: APPLY_ID.to_string(),
                            label: "Apply".to_string(),
                            frozen: false,
                        },
                        CompositeControl::Button {
                            id: CANCEL_ID.to_string(),
                            label: "Cancel".to_string(),
                            frozen: false,
                        },
                    ],
                },
            ],
        },
        completer_source_section(),
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

/// Which mind answers a wake, and what the current attachment is doing.
///
/// One variable owns both rows because `AI_BUDDY_HARNESS` spells the whole
/// choice in one value — a preset name or a command line (ADR-0017) — so
/// freezing them apart would offer an edit the launch throws away (#272).
///
/// No credential row of any kind, now or later: the Harness signs itself in
/// and ai-buddy holds nothing for it (ADR-0010's eight rules). The login
/// command the state line names is text, and nothing here runs it.
fn completer_source_section() -> FormSection {
    let (source_label, frozen) = env_row("Harness", crate::harness::VAR);
    FormSection {
        heading: "Completer source".to_string(),
        comment: Some(
            "Director off runs on static weights. Director on with no Harness is the \
             HTTP Completer above. Director on with a Harness makes that Harness the \
             mind for every buddy, and the HTTP rows stop driving it."
                .to_string(),
        ),
        rows: vec![
            FormRow::Popup {
                id: HARNESS_ID.to_string(),
                label: Some(source_label),
                writes: TextField::Harness,
                help: Some(
                    "Off leaves the HTTP Completer. A Harness takes over on the next launch."
                        .to_string(),
                ),
                options: harness_options(),
                frozen,
            },
            // Not batched: one launch stands between this and the Completer
            // either way, so a button between the typing and the file would
            // only be one more thing to click.
            FormRow::TextField {
                id: HARNESS_COMMAND_ID.to_string(),
                label: Some("Custom command line".to_string()),
                placeholder: "opencode acp".to_string(),
                writes: TextField::HarnessCommand,
                frozen,
                batched: false,
            },
            FormRow::InspectBlock {
                id: HARNESS_STATE_ID.to_string(),
                label: None,
                help: Some(
                    "ai-buddy never asks for the Harness's credential — it signs itself in."
                        .to_string(),
                ),
            },
        ],
    }
}

fn character_sections() -> Vec<FormSection> {
    vec![
        FormSection {
            heading: "Character".to_string(),
            comment: None,
            rows: vec![FormRow::Popup {
                id: CHARACTER_ID.to_string(),
                label: None,
                writes: TextField::Character,
                help: Some("The character your buddy wears.".to_string()),
                options: Vec::new(),
                frozen: false,
            }],
        },
        FormSection {
            heading: "Instances".to_string(),
            comment: None,
            rows: vec![
                FormRow::List {
                    id: INSTANCES_ID.to_string(),
                    dismiss_label: "Dismiss".to_string(),
                    help: Some("Buddies on screen now.".to_string()),
                },
                FormRow::Composite {
                    id: "new_instance".to_string(),
                    help: Some("Adds another buddy.".to_string()),
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
            // Named for quiet rather than for hiding: Do Not Disturb leaves the
            // buddy on screen, and a Hide heading would teach the opposite.
            heading: "Do Not Disturb".to_string(),
            comment: None,
            rows: vec![
                FormRow::Checkbox {
                    id: DND_ID.to_string(),
                    label: "Do Not Disturb".to_string(),
                    writes: BoolField::DoNotDisturb,
                    frozen: false,
                    help: Some(
                        "Stays on screen. Silences sounds and stops initiating actions."
                            .to_string(),
                    ),
                    comment: None,
                },
                FormRow::Checkbox {
                    id: SOUND_ID.to_string(),
                    label: "Sound".to_string(),
                    writes: BoolField::Sound,
                    frozen: false,
                    help: Some("Off silences audio cues.".to_string()),
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
                    writes: BoolField::Hidden,
                    frozen: false,
                    help: Some("Go off screen. But still exist.".to_string()),
                    comment: None,
                },
                FormRow::Checkbox {
                    id: FULLSCREEN_ID.to_string(),
                    label: "Hide in fullscreen apps".to_string(),
                    writes: BoolField::HideInFullscreen,
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
                writes: BoolField::LaunchAtLogin,
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
    #[cfg(target_os = "macos")]
    let consent_comment = Some(consent::pane_intro(&consent::process_listed_as()));

    #[cfg(not(target_os = "macos"))]
    let consent_comment = Some(consent::linux_pane_intro());

    vec![
        FormSection {
            heading: "What the buddy can see".to_string(),
            comment: consent_comment,
            rows: vec![
                FormRow::Checkbox {
                    id: CONSENT_ACCESSIBILITY_ID.to_string(),
                    label: "Accessibility".to_string(),
                    writes: BoolField::UseAccessibility,
                    frozen: false,
                    help: Some("Reads the Dock's position.".to_string()),
                    comment: None,
                },
                FormRow::Checkbox {
                    id: CONSENT_SCREEN_RECORDING_ID.to_string(),
                    label: "Screen Recording".to_string(),
                    writes: BoolField::UseScreenRecording,
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
                writes: TextField::ExcludedApplications,
                help: Some("One application name per line. Those windows stay out of MCP sensing. The buddy can still sit on them.".to_string()),
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
                    help: None,
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
            BoolField::TraceFrames,
            "Trace frames",
            "Prints each frame.",
        ),
        flag_row(
            TRACE_HITTEST_ID,
            &dev_flags::TRACE_HITTEST,
            BoolField::TraceHittest,
            "Trace hit-test",
            "Prints where each click went.",
        ),
        flag_row(
            TRACE_DIRECTOR_ID,
            &dev_flags::TRACE_DIRECTOR,
            BoolField::TraceDirector,
            "Trace Director",
            "Prints each model call.",
        ),
        flag_row(
            TRACE_ENGINE_ID,
            &dev_flags::TRACE_ENGINE,
            BoolField::TraceEngine,
            "Trace Engine",
            "Prints each change of Behavior or Animation.",
        ),
        // Only AppKit has a capture exclusion to drop. Gating the element
        // rather than pushing it keeps the binding immutable on the platforms
        // that skip it, which `-D warnings` insists on.
        #[cfg(target_os = "macos")]
        flag_row(
            CAPTURABLE_ID,
            &dev_flags::CAPTURABLE,
            BoolField::Capturable,
            "Show in screenshots and shares",
            // `configure_overlay` reads this when a window is built, and only
            // the Linux frame loop re-runs it. Honest rather than silently
            // inert.
            "Normally left out of both. Needs a restart.",
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
                    writes: TextField::DirectorTimeoutSecs,
                    frozen: timeout_frozen,
                    batched: false,
                },
                FormRow::TextField {
                    id: DIRECTOR_MAX_TOKENS_ID.to_string(),
                    label: Some(max_tokens_label),
                    placeholder: model::max_tokens_placeholder(),
                    writes: TextField::DirectorMaxTokens,
                    frozen: max_tokens_frozen,
                    batched: false,
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
    // Only the buttons: every writing row carries the field it writes.
    let operations = HashMap::from([
        (SPAWN_ID.to_string(), RowOperation::Spawn),
        (MEMORY_OPEN_ID.to_string(), RowOperation::OpenMemory),
        (MEMORY_WIPE_ID.to_string(), RowOperation::WipeMemory),
        (CLEAR_KEY_ID.to_string(), RowOperation::ClearKey),
        (APPLY_ID.to_string(), RowOperation::Apply),
        (CANCEL_ID.to_string(), RowOperation::Cancel),
    ]);

    FormDescription { tabs, operations }
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
            "Completer source",
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

    /// #272's rule for a row the env owns: no edit, and name the variable.
    /// Either direction owns it — on is as much the variable's word as off.
    #[test]
    fn an_env_owned_director_row_is_read_only_and_names_its_variable() {
        for exported in ["off", "on"] {
            crate::model::tests::with_env_switch(exported, || {
                let description = describe();
                let (label, frozen) = description
                    .sections()
                    .flat_map(|section| &section.rows)
                    .find_map(|row| match row {
                        FormRow::Checkbox {
                            id, label, frozen, ..
                        } if id == DIRECTOR_ID => Some((label.clone(), *frozen)),
                        _ => None,
                    })
                    .expect("the Director row exists");

                assert!(frozen, "a switch the env owns takes no edit, {exported:?}");
                assert!(
                    label.contains(crate::model::ENABLED),
                    "the row must name the variable, not {label:?}"
                );
            });
        }
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
                    ..
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
                        description.frozen(id),
                        "{id} is what a renderer asks before it reads the field"
                    );
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
                assert!(!description.frozen(id));
                assert!(!label.contains(var), "{id} must not mention {var}");
            }
            assert!(
                !description.frozen(CLEAR_KEY_ID),
                "Clear key answers to the key row's variable"
            );
        });
    }

    #[test]
    fn director_section_has_two_checkboxes() {
        let description = describe();
        let director = description
            .sections()
            .find(|s| s.heading == "Director")
            .expect("Director section");

        assert_eq!(director.rows.len(), 8);
        assert!(matches!(
            director.rows[0],
            FormRow::Checkbox { ref id, .. } if id == DIRECTOR_ID
        ));
        assert!(matches!(
            director.rows[1],
            FormRow::Checkbox { ref id, .. } if id == AMBIENT_ID
        ));
        // Under the switch that turns ambient wakes on, because it is how
        // often those wakes start out (#262).
        assert!(matches!(
            director.rows[2],
            FormRow::TextField { ref id, .. } if id == DIRECTOR_WAKE_SECS_ID
        ));
        assert!(matches!(
            director.rows[3],
            FormRow::TextField { ref id, .. } if id == DIRECTOR_BASE_URL_ID
        ));
        assert!(matches!(
            director.rows[4],
            FormRow::TextField { ref id, .. } if id == DIRECTOR_MODEL_ID
        ));
        assert!(matches!(
            director.rows[5],
            FormRow::SecureField { ref id, .. } if id == DIRECTOR_API_KEY_ID
        ));
        assert!(matches!(
            director.rows[6],
            FormRow::Composite { ref id, .. } if id == "api_key_actions"
        ));
        assert!(matches!(
            director.rows[7],
            FormRow::Composite { ref id, .. } if id == "director_actions"
        ));
    }

    /// The Director endpoint, and no other editable row in the window.
    ///
    /// The Completer limits are the ones this test is really about: #273
    /// landed them to be changed and watched, and a button between a limit
    /// and its effect would undo that.
    ///
    /// Clear key is the fourth batched control and is absent here, because it
    /// is an operation rather than a value: `RowOperation::ClearKey` stages,
    /// and that is the whole of what it means. `actions_map_to_patches_or_ops`
    /// is what holds that end (#279).
    #[test]
    fn only_the_director_endpoint_batches() {
        let description = describe();
        let mut batched: Vec<&str> = Vec::new();
        for row in description.sections().flat_map(|section| &section.rows) {
            match row {
                FormRow::TextField {
                    id, batched: true, ..
                } => batched.push(id),
                // No flag of its own: every secure field batches.
                FormRow::SecureField { id, .. } => batched.push(id),
                _ => {}
            }
        }
        batched.sort_unstable();
        let mut expected = vec![DIRECTOR_API_KEY_ID, DIRECTOR_BASE_URL_ID, DIRECTOR_MODEL_ID];
        expected.sort_unstable();
        assert_eq!(batched, expected);
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

    /// Every row on the Character tab says what it does.
    ///
    /// `Popup`, `List` and `Composite` were the three kinds with nowhere to
    /// put a help line, so this tab shipped without one.
    #[test]
    fn every_character_tab_row_has_help() {
        let description = describe();
        let tab = description
            .tabs
            .iter()
            .find(|tab| tab.title == "Character")
            .expect("the Character tab exists");

        for section in &tab.sections {
            for row in &section.rows {
                let help = match row {
                    FormRow::Popup { id, help, .. }
                    | FormRow::List { id, help, .. }
                    | FormRow::Composite { id, help, .. } => (id, help),
                    other => panic!("unexpected row kind on the Character tab: {other:?}"),
                };
                assert!(
                    help.1.is_some(),
                    "{} on the Character tab has no help",
                    help.0
                );
            }
        }
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
        assert!(has_help("Completer source", HARNESS_STATE_ID));
        assert!(has_help("Last user turn", PAYLOAD_ID));
        assert!(has_help("Do Not Disturb", DND_ID));
        assert!(has_help("Do Not Disturb", SOUND_ID));
        assert!(has_help("Excluded applications", EXCLUDED_ID));
    }

    /// A button is the one control still reached by name, so it is the one
    /// that can still be drawn with nothing behind it. Every other control
    /// carries the field it writes, which the compiler checks.
    #[test]
    fn every_button_has_an_operation() {
        let description = describe();

        for section in description.sections() {
            for row in &section.rows {
                if let FormRow::Composite { controls, .. } = row {
                    for control in controls {
                        if let CompositeControl::Button { id, .. } = control {
                            assert!(
                                description.operations.contains_key(id),
                                "Button {id} has no operation"
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn buttons_map_to_operations() {
        let description = describe();

        assert_eq!(
            description.operations.get(SPAWN_ID),
            Some(&RowOperation::Spawn)
        );
        assert_eq!(
            description.operations.get(MEMORY_OPEN_ID),
            Some(&RowOperation::OpenMemory)
        );
        assert_eq!(
            description.operations.get(APPLY_ID),
            Some(&RowOperation::Apply)
        );
        assert_eq!(
            description.operations.get(CANCEL_ID),
            Some(&RowOperation::Cancel)
        );
    }

    #[test]
    fn consent_section_has_two_checkboxes() {
        let description = describe();
        let consent = description
            .sections()
            .find(|s| s.heading == "What the buddy can see")
            .expect("Consent section exists");

        #[cfg(target_os = "macos")]
        {
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
                .find(
                    |r| matches!(r, FormRow::Checkbox { id, .. } if id == CONSENT_ACCESSIBILITY_ID),
                )
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

            // The compiler pins each of these to *a* bool; only the test pins it
            // to the right one, and a swap here would grant the other capability.
            assert_eq!(
                description.bool_write(CONSENT_ACCESSIBILITY_ID),
                Some(BoolField::UseAccessibility)
            );
            assert_eq!(
                description.bool_write(CONSENT_SCREEN_RECORDING_ID),
                Some(BoolField::UseScreenRecording)
            );
        }

        #[cfg(not(target_os = "macos"))]
        {
            assert_eq!(
                consent.rows.len(),
                2,
                "Linux still declares the rows; the renderer omits them"
            );
            let comment = consent
                .comment
                .as_ref()
                .expect("Linux section has prose when rows are omitted");
            assert!(
                !comment.contains("Accessibility"),
                "Linux prose must not use TCC vocabulary, got {comment:?}"
            );
            assert!(
                !comment.contains("Screen Recording"),
                "Linux prose must not use TCC vocabulary, got {comment:?}"
            );
            assert!(
                !comment.contains("Privacy & Security"),
                "Linux prose must not use TCC vocabulary, got {comment:?}"
            );
            assert!(
                comment.contains("no permission is requested")
                    || comment.contains("no permission requested"),
                "Linux prose must say nothing is requested, got {comment:?}"
            );
            assert!(
                comment.contains("window") || comment.contains("Window"),
                "Linux prose must name what is read without a grant, got {comment:?}"
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
    fn the_development_tab_warns_on_every_section() {
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
                row_id(row).expect("every development row is a control that writes");
            }
        }
    }

    /// A switch the env owns names the variable it answers to, so the export
    /// to drop is on screen, and takes no click.
    ///
    /// A value the vocabulary reads freezes the row, `0` included. One it
    /// cannot read owns nothing, so that row stays the user's — freezing it
    /// would claim an export the switch never obeyed.
    #[test]
    fn an_env_owned_flag_row_is_frozen_and_names_its_variable() {
        crate::model::tests::with_env(None, None, None, || {
            let var = dev_flags::TRACE_FRAMES.var();
            for (exported, owned) in [
                (Some("1"), true),
                (Some("0"), true),
                (Some("true"), true),
                (Some("banana"), false),
                (None, false),
            ] {
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
                        assert_eq!(*frozen, owned, "exported {exported:?} decides the click");
                        assert_eq!(
                            label.contains(var),
                            owned,
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

    fn source_section(description: &FormDescription) -> &FormSection {
        description
            .sections()
            .find(|section| section.heading == "Completer source")
            .expect("the Completer source section exists")
    }

    fn popup_row(description: &FormDescription, id: &str) -> (String, Vec<String>, bool) {
        source_section(description)
            .rows
            .iter()
            .find_map(|row| match row {
                FormRow::Popup {
                    id: row_id,
                    label,
                    options,
                    frozen,
                    ..
                } if row_id == id => {
                    Some((label.clone().unwrap_or_default(), options.clone(), *frozen))
                }
                _ => None,
            })
            .expect("the source popup exists")
    }

    /// Off, the three named launch rows, and the escape hatch — ADR-0017's
    /// table, and nothing for Grok, Copilot or Gemini until one is smoked.
    #[test]
    fn the_completer_source_offers_off_three_presets_and_custom() {
        crate::model::tests::with_harness(None, || {
            let description = describe();
            let (_, options, _) = popup_row(&description, HARNESS_ID);
            assert_eq!(options, ["Off", "claude", "hermes", "opencode", "Custom"]);
            assert_eq!(
                description.text_write(HARNESS_ID),
                Some(TextField::Harness),
                "a pick has to reach the file"
            );
            assert_eq!(
                description.text_write(HARNESS_COMMAND_ID),
                Some(TextField::HarnessCommand),
            );
        });
    }

    /// #272's rule, for the one variable that owns two rows: `AI_BUDDY_HARNESS`
    /// spells the preset and the command line in one value, so an edit to
    /// either would be discarded at launch.
    #[test]
    fn an_exported_harness_freezes_both_source_rows_and_names_its_variable() {
        for exported in ["hermes", "opencode acp"] {
            crate::model::tests::with_harness(Some(exported), || {
                let description = describe();
                let (label, _, frozen) = popup_row(&description, HARNESS_ID);
                assert!(frozen, "{exported:?} owns the source row");
                assert!(
                    label.contains(crate::harness::VAR),
                    "the row must name the variable, not {label:?}"
                );
                assert!(description.frozen(HARNESS_ID));
                assert!(
                    description.frozen(HARNESS_COMMAND_ID),
                    "one value owns the command line too"
                );
            });
        }
    }

    #[test]
    fn the_source_rows_are_the_users_when_no_variable_is_exported() {
        crate::model::tests::with_harness(None, || {
            let description = describe();
            let (label, _, frozen) = popup_row(&description, HARNESS_ID);
            assert!(!frozen);
            assert!(!label.contains(crate::harness::VAR));
            assert!(!description.frozen(HARNESS_COMMAND_ID));
        });
    }

    /// ADR-0010 rules 1 and 6: no field here ever asks for a Harness
    /// credential, and the login command is words the user runs themselves.
    #[test]
    fn the_completer_source_asks_for_no_credential() {
        crate::model::tests::with_harness(None, || {
            let description = describe();
            for row in &source_section(&description).rows {
                assert!(
                    !matches!(row, FormRow::SecureField { .. }),
                    "the Harness signs itself in; ADR-0010 forbids a field for it"
                );
            }
        });
    }

    /// An attached Harness is the Completer (ADR-0008), so the three HTTP rows
    /// drive nothing and say so. Asserted through the labels rather than a
    /// live attachment: `harness::attached` is a process global one test may
    /// not set for the whole binary.
    #[test]
    fn an_attached_harness_takes_the_http_rows_out_of_use() {
        for (label, frozen) in [
            http_row("Base URL", crate::model::BASE_URL, true),
            http_row("Model", crate::model::MODEL, true),
            http_row("API key", crate::model::API_KEY, true),
        ] {
            assert!(frozen, "an attached Harness discards an edit here");
            assert!(
                label.contains("not in use"),
                "the row has to say why it is dead, not {label:?}"
            );
        }
    }
}
