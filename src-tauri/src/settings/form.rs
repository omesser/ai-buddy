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
    CopyMcpToken,
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
        /// A hint under the field, in the words every other row's `help` uses.
        ///
        /// A label says what the row is and a placeholder says what blank
        /// means; neither has room for what a value costs. The Completer
        /// timeout is the case that earned it: it budgets a Harness turn as
        /// well as an HTTP call, and nothing on screen said so (#447).
        help: Option<String>,
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
        /// The choices, for the same reason as `FormRow::Popup::options`:
        /// empty leaves them to the renderer, which is how `new_instance`'s
        /// Character popup gets the installed packages.
        options: Vec<String>,
        /// Disabled, for the same reason as `FormRow::TextField::frozen`.
        frozen: bool,
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
                        }
                        | CompositeControl::Popup {
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
pub const DIRECTOR_BASE_URL_PICK_ID: &str = "director_base_url_pick";
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
/// The two consent rows. Gated because Linux offers neither, so the ids exist
/// only where the rows do. #250.
#[cfg(not(target_os = "linux"))]
pub const CONSENT_ACCESSIBILITY_ID: &str = "consent_accessibility";
#[cfg(not(target_os = "linux"))]
pub const CONSENT_SCREEN_RECORDING_ID: &str = "consent_screen_recording";
pub const LAUNCH_ID: &str = "launch";
pub const TRACE_FRAMES_ID: &str = "trace_frames";
pub const TRACE_HITTEST_ID: &str = "trace_hittest";
pub const TRACE_DIRECTOR_ID: &str = "trace_director";
pub const TRACE_ENGINE_ID: &str = "trace_engine";
#[cfg(any(target_os = "macos", target_os = "windows"))]
pub const CAPTURABLE_ID: &str = "capturable";
pub const DIRECTOR_TIMEOUT_SECS_ID: &str = "director_timeout_secs";
pub const DIRECTOR_MAX_TOKENS_ID: &str = "director_max_tokens";
pub const DIRECTOR_WAKE_SECS_ID: &str = "director_wake_secs";
pub const HARNESS_ID: &str = "harness";
pub const HARNESS_COMMAND_ID: &str = "harness_command";
pub const HARNESS_STATE_ID: &str = "harness_state";
pub const HARNESS_AUTH_RETRY_SECS_ID: &str = "harness_auth_retry_secs";
pub const MCP_BIN_ID: &str = "mcp_bin";
pub const MCP_URL_ID: &str = "mcp_url";
pub const MCP_COPY_TOKEN_ID: &str = "mcp_copy_token";

/// The two Completer-source titles the file does not spell the same way: Off
/// is the empty string and Custom is `custom`, which defers to the command
/// line beside it.
pub const HARNESS_OFF: &str = "Off";
pub const HARNESS_CUSTOM: &str = "Custom";
/// What `harness_choice` writes for Custom. Not a value `AI_BUDDY_HARNESS`
/// can take, so it cannot collide with a Harness of that name.
pub const HARNESS_CUSTOM_VALUE: &str = "custom";
/// The named launch rows, in ADR-0022's order. Copilot and Gemini reach the
/// same Completer through Custom until a turn has been smoked.
pub const HARNESS_PRESETS: [&str; 5] = ["claude", "codex", "grok", "hermes", "opencode"];

/// The endpoints the Base URL picker names, as (group, name, base URL).
///
/// Every port was read off that project's own current documentation rather
/// than recalled (#465). Each entry is the host alone because
/// `model::completions_url` adds `/v1` and the path.
///
/// Two local servers `docs/DEVELOPMENT.md` lists are deliberately absent: vLLM
/// answers on oMLX's 8000 and `mlx_lm.server` on llama.cpp's 8080, so a row for
/// either would offer a second name for a URL already on the list. The field
/// below takes both.
const ENDPOINTS: &[(&str, &str, &str)] = &[
    ("Local", "Ollama", "http://localhost:11434"),
    ("Local", "LM Studio", "http://localhost:1234"),
    ("Local", "oMLX", "http://localhost:8000"),
    ("Local", "llama.cpp", "http://localhost:8080"),
    ("Hosted", "OpenAI", "https://api.openai.com"),
    ("Hosted", "Anthropic", "https://api.anthropic.com"),
    ("Hosted", "xAI", "https://api.x.ai"),
];

/// The title the picker rests on for an endpoint it does not name. Picking it
/// writes nothing: the field beside it is what a custom endpoint is.
pub const ENDPOINT_CUSTOM: &str = "Custom";

fn endpoint_title_of(_group: &str, name: &str, url: &str) -> String {
    format!("{name} ({url})")
}

/// The Base URL picker's choices, Custom first.
///
/// No header rows: a header is an item that picks nothing, and GTK draws these
/// choices as a radio group, where an inert radio is a click that silently
/// does nothing. The group is carried by the order instead — the local servers
/// first, the hosted ones after — because `localhost` in the URL beside the
/// name already says which is which.
pub fn endpoint_options() -> Vec<String> {
    let mut options = vec![ENDPOINT_CUSTOM.to_string()];
    options.extend(
        ENDPOINTS
            .iter()
            .map(|(group, name, url)| endpoint_title_of(group, name, url)),
    );
    options
}

/// The base URL a picked title means, or `None` for Custom and for a title off
/// the list — neither of which is a value to write over what the field holds.
// AppKit and GTK spend a pick; Win32 draws the choices and commits none of
// them yet, so the binary's dead-code lint sees no caller there.
#[cfg_attr(target_os = "windows", allow(dead_code))]
pub fn endpoint_url(title: &str) -> Option<&'static str> {
    ENDPOINTS
        .iter()
        .find(|(group, name, url)| endpoint_title_of(group, name, url) == title)
        .map(|(_, _, url)| *url)
}

/// The title to rest on for the base URL in force. The inverse of
/// `endpoint_url`, and Custom for the endpoints this list does not name —
/// which is most of what the field can hold.
// Win32 selects from `SettingsView::popup_value`, which answers for no
// composite control; the same dead-code note as `endpoint_url`.
#[cfg_attr(target_os = "windows", allow(dead_code))]
pub fn endpoint_title(base_url: &str) -> String {
    let base_url = base_url.trim().trim_end_matches('/');
    ENDPOINTS
        .iter()
        .find(|(_, _, url)| *url == base_url)
        .map(|(group, name, url)| endpoint_title_of(group, name, url))
        .unwrap_or_else(|| ENDPOINT_CUSTOM.to_string())
}

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

/// The command line a source value carries in the value itself, if it does.
///
/// The catch-all of `harness_rows`: `AI_BUDDY_HARNESS` spells a custom Harness
/// as its command line, and a hand-edit of the file may too. `Settings::apply`
/// moves it to the row that owns it (#452).
pub fn harness_command_line(value: &str) -> Option<&str> {
    match value.trim() {
        "" | HARNESS_CUSTOM_VALUE => None,
        preset if HARNESS_PRESETS.contains(&preset) => None,
        line => Some(line),
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

/// The Completer source rows' ownership question.
///
/// Exported at all owns them, empty included, because
/// `harness::from_settings` reads an empty value as Off rather than falling
/// through to the file — so a row left editable there would take an edit the
/// launch discards (#452). `env_row`'s emptiness rule belongs to the endpoint
/// rows and does not carry over.
fn harness_env_row(label: &str) -> (String, bool) {
    owned_row(
        label,
        crate::harness::VAR,
        std::env::var_os(crate::harness::VAR).is_some(),
    )
}

/// One of the three HTTP rows, which answer to an attachment as well as to a
/// variable.
///
/// A Harness that *answers* is the Completer (ADR-0008), so these three drive
/// nothing while one is up, and #272's rule applies for the same reason it
/// applies to an exported variable: an edit the Director would discard is not
/// an edit to offer. The source row below is never frozen by an attachment,
/// so Off stays one pick and one launch away.
///
/// Driving rather than merely configured, which is what `harness::driving`
/// asks: a Harness this machine has not got leaves a handle that never
/// answers, and freezing these three on that would leave no reachable
/// Completer at all (#452).
///
/// That leaves a third state, and #469 is what it costs: the handle stays the
/// configured Completer whether or not its child is up, so an edit here is
/// saved and not used. Live, because it is the way back; labelled, because a
/// row that takes a key and changes nothing is worse than a frozen one.
/// Handing the Director to the HTTP Completer the moment a session dies is the
/// second mind ADR-0008 refuses.
///
/// #500 narrowed the label rather than removing it. The wait is no longer a
/// relaunch — the Session retries the child on its own backoff, and Off in the
/// source row hands these three back at once — so the label names the pick
/// that ends it instead of a launch.
fn http_row(label: &str, var: &str, driving: bool, configured: bool) -> (String, bool) {
    if driving {
        return (
            format!("{label} (not in use: a Harness is the Completer)"),
            true,
        );
    }
    let (label, frozen) = env_row(label, var);
    match configured {
        true => (
            format!("{label} (not in use until the source below is Off: a Harness is still the Completer)"),
            frozen,
        ),
        false => (label, frozen),
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
    let driving = crate::harness::driving();
    // Configured, not driving: the handle still shadows these three (#469).
    let configured = crate::harness::attached().is_some();
    let (base_url_label, base_url_frozen) =
        http_row("Base URL", model::BASE_URL, driving, configured);
    let (model_label, model_frozen) = http_row("Model", model::MODEL, driving, configured);
    let (api_key_label, api_key_frozen) = http_row("API key", model::API_KEY, driving, configured);
    let (director_label, director_frozen) = switch_row("AI on", model::ENABLED);
    let (wake_label, wake_frozen) = env_row("First wake, in seconds", model::WAKE_SECS);

    vec![
        FormSection {
            heading: "AI".to_string(),
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
                    help: None,
                },
                // A shortcut above the field, not a replacement for it: almost
                // every endpoint typed here is one of a dozen well-known
                // strings, and a typo in one of those fails silently several
                // layers down (#465). Anything off the list stays typeable.
                //
                // It writes to the field rather than to the file, which is why
                // it is a composite control and carries no `writes` of its
                // own: the four Director rows only mean anything together, and
                // a pick that saved on its own would point the Completer at a
                // host and model that were never meant to go together (#279).
                FormRow::Composite {
                    id: "base_url_pick".to_string(),
                    help: Some(match base_url_frozen {
                        true => "Off, for the same reason the Base URL below is.".to_string(),
                        false => "Fills in the Base URL below. Any other \
                                  OpenAI-compatible endpoint can be typed there."
                            .to_string(),
                    }),
                    controls: vec![CompositeControl::Popup {
                        id: DIRECTOR_BASE_URL_PICK_ID.to_string(),
                        options: endpoint_options(),
                        frozen: base_url_frozen,
                    }],
                },
                FormRow::TextField {
                    id: DIRECTOR_BASE_URL_ID.to_string(),
                    label: Some(base_url_label),
                    placeholder: "https://api.openai.com".to_string(),
                    writes: TextField::DirectorBaseUrl,
                    frozen: base_url_frozen,
                    batched: true,
                    help: None,
                },
                FormRow::TextField {
                    id: DIRECTOR_MODEL_ID.to_string(),
                    label: Some(model_label),
                    placeholder: "gpt-4o-mini".to_string(),
                    writes: TextField::DirectorModel,
                    frozen: model_frozen,
                    batched: true,
                    help: None,
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
///
/// ponytail: all three renderers draw these rows — AppKit from the start, GTK
/// since #467, Win32 since #468. What Win32 still cannot do is commit one: no
/// control in that window maps back to the row it belongs to, because its
/// handlers look a row up by the numeric child id, so nothing typed or picked
/// there is written — the wake interval and both Completer limits included.
/// One generic commit handler plus that id map fixes them together and is
/// #461, not written blind here: Win32 does not compile on the machine this
/// landed from. The file field is the setting either way, so a hand-edit
/// works everywhere today.
fn completer_source_section() -> FormSection {
    let (source_label, frozen) = harness_env_row("Harness");
    FormSection {
        heading: "AI source".to_string(),
        comment: Some(
            "Director off runs on static weights. Director on with no Harness is the \
             HTTP Completer above. Director on with a Harness that answers makes \
             that Harness the mind for every Instance, and the HTTP rows stop \
             driving it."
                .to_string(),
        ),
        rows: vec![
            FormRow::Popup {
                id: HARNESS_ID.to_string(),
                label: Some(source_label),
                writes: TextField::Harness,
                help: Some(
                    "Every pick takes effect now: Off leaves the HTTP Completer, \
                     and a Harness is attached at once, answering once its \
                     child is up. The line below says what is attached."
                        .to_string(),
                ),
                options: harness_options(),
                frozen,
            },
            // Not batched: the blur that writes the row is what re-opens the
            // attachment since #500, so a button between the typing and the
            // file would only be one more thing to click.
            FormRow::TextField {
                id: HARNESS_COMMAND_ID.to_string(),
                label: Some("Custom command line".to_string()),
                placeholder: "opencode acp".to_string(),
                writes: TextField::HarnessCommand,
                frozen,
                batched: false,
                help: None,
            },
            FormRow::InspectBlock {
                id: HARNESS_STATE_ID.to_string(),
                label: None,
                help: Some(
                    "ai-buddy never asks for the Harness's credential - it signs itself in."
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
                            options: Vec::new(),
                            frozen: false,
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
                #[cfg(any(target_os = "macos", target_os = "windows"))]
                FormRow::Checkbox {
                    id: CAPTURABLE_ID.to_string(),
                    label: "Appear in screenshots and screen shares".to_string(),
                    writes: BoolField::Capturable,
                    frozen: false,
                    help: Some("Checked: buddy is visible in screen captures (default). Unchecked: excluded. Needs a restart.".to_string()),
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

    #[cfg(target_os = "windows")]
    let consent_comment = Some(consent::pane_intro(&consent::process_listed_as()));

    #[cfg(target_os = "linux")]
    let consent_comment = Some(consent::linux_pane_intro());

    // Linux sensing asks the user for nothing, so it has no grant to offer a
    // row for. #250. Windows keeps its rows; that platform is its own ticket.
    #[cfg(not(target_os = "linux"))]
    let consent_rows = vec![
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
    ];

    #[cfg(target_os = "linux")]
    let consent_rows: Vec<FormRow> = Vec::new();

    vec![
        FormSection {
            heading: "What the buddy can see".to_string(),
            comment: consent_comment,
            rows: consent_rows,
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
    ];

    let (timeout_label, timeout_frozen) = env_row("Timeout, in seconds", model::TIMEOUT_SECS);
    let (max_tokens_label, max_tokens_frozen) = env_row("Reply cap, in tokens", model::MAX_TOKENS);
    let (auth_retry_label, auth_retry_frozen) =
        env_row("Auth retry, in seconds", crate::harness::AUTH_RETRY_SECS);
    let (mcp_bin_label, mcp_bin_frozen) = env_row("MCP server binary", crate::harness::MCP_BIN);

    vec![
        FormSection {
            heading: "Traces".to_string(),
            comment: Some("Switches for development and testing.".to_string()),
            rows,
        },
        FormSection {
            heading: "HTTP limits".to_string(),
            comment: Some("Also for development and testing. Blank uses the default.".to_string()),
            rows: vec![
                FormRow::TextField {
                    id: DIRECTOR_TIMEOUT_SECS_ID.to_string(),
                    label: Some(timeout_label),
                    placeholder: model::timeout_placeholder(),
                    writes: TextField::DirectorTimeoutSecs,
                    frozen: timeout_frozen,
                    batched: false,
                    help: Some(
                        "One turn's budget, whichever mind serves it: an HTTP \
                         Completer request or a Harness session/prompt. Expiry \
                         cancels the turn, and the buddy stays as it was."
                            .to_string(),
                    ),
                },
                FormRow::TextField {
                    id: DIRECTOR_MAX_TOKENS_ID.to_string(),
                    label: Some(max_tokens_label),
                    placeholder: model::max_tokens_placeholder(),
                    writes: TextField::DirectorMaxTokens,
                    frozen: max_tokens_frozen,
                    batched: false,
                    help: Some(
                        "The HTTP Completer's alone. A Harness decides its own \
                         reply length."
                            .to_string(),
                    ),
                },
            ],
        },
        // Development rather than the Director tab, which is where a user
        // picks a Harness: neither row is a choice anyone makes to get a
        // buddy working, and both exist so a test or a CI job can be told
        // where to look and how long to wait (#447).
        FormSection {
            heading: "Harness attachment".to_string(),
            comment: Some(
                "Also for development and testing. Blank uses the default, and \
                 both take effect on the next attach."
                    .to_string(),
            ),
            rows: vec![
                FormRow::TextField {
                    id: HARNESS_AUTH_RETRY_SECS_ID.to_string(),
                    label: Some(auth_retry_label),
                    placeholder: crate::harness::auth_retry_placeholder(),
                    writes: TextField::HarnessAuthRetrySecs,
                    frozen: auth_retry_frozen,
                    batched: false,
                    help: Some(
                        "How long a Harness that has not signed in is left \
                         alone before session/new is tried again."
                            .to_string(),
                    ),
                },
                FormRow::TextField {
                    id: MCP_BIN_ID.to_string(),
                    label: Some(mcp_bin_label),
                    placeholder: "beside the app, else this app on --mcp-stdio".to_string(),
                    writes: TextField::McpBin,
                    frozen: mcp_bin_frozen,
                    batched: false,
                    help: Some(
                        "The stdio MCP server handed to the Harness session. A \
                         path that is not a file falls back to the default."
                            .to_string(),
                    ),
                },
            ],
        },
        FormSection {
            heading: "MCP".to_string(),
            comment: Some(
                "These rows are for development and testing: for pointing a Harness \
                 you run yourself at ai-buddy. The URL and token change on every launch."
                    .to_string(),
            ),
            rows: vec![
                FormRow::InspectBlock {
                    id: MCP_URL_ID.to_string(),
                    label: Some("URL".to_string()),
                    help: None,
                },
                FormRow::Composite {
                    id: "mcp_token_row".to_string(),
                    controls: vec![CompositeControl::Button {
                        id: MCP_COPY_TOKEN_ID.to_string(),
                        label: "Copy token to clipboard".to_string(),
                        frozen: false,
                    }],
                    help: Some(
                        "The token never appears on screen. Copying it to the \
                         clipboard is the trade ADR-0026 makes."
                            .to_string(),
                    ),
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
            title: "AI".to_string(),
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
        (MCP_COPY_TOKEN_ID.to_string(), RowOperation::CopyMcpToken),
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
            "HTTP limits",
            "AI source",
            "AI",
            "Do Not Disturb",
            "Excluded applications",
            "Harness attachment",
            "Hide",
            "Instances",
            "Last user turn",
            "Launch",
            "MCP",
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
            vec!["Presence", "Character", "AI", "Privacy", "Development"]
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
    fn ai_section_has_two_checkboxes() {
        let description = describe();
        let director = description
            .sections()
            .find(|s| s.heading == "AI")
            .expect("AI section");

        assert_eq!(director.rows.len(), 9);
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
        // The picker stands above the field it fills, so the shortcut is read
        // before the typing starts (#465).
        assert!(matches!(
            director.rows[3],
            FormRow::Composite { ref id, .. } if id == "base_url_pick"
        ));
        assert!(matches!(
            director.rows[4],
            FormRow::TextField { ref id, .. } if id == DIRECTOR_BASE_URL_ID
        ));
        assert!(matches!(
            director.rows[5],
            FormRow::TextField { ref id, .. } if id == DIRECTOR_MODEL_ID
        ));
        assert!(matches!(
            director.rows[6],
            FormRow::SecureField { ref id, .. } if id == DIRECTOR_API_KEY_ID
        ));
        assert!(matches!(
            director.rows[7],
            FormRow::Composite { ref id, .. } if id == "api_key_actions"
        ));
        assert!(matches!(
            director.rows[8],
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

        assert!(has_help("AI", DIRECTOR_ID));
        assert!(has_help("AI", AMBIENT_ID));
        assert!(has_help("AI source", HARNESS_STATE_ID));
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
            let comment = consent
                .comment
                .as_ref()
                .expect("Non-macOS section has prose when rows are omitted");
            assert!(
                !comment.contains("Accessibility"),
                "Non-macOS prose must not use TCC vocabulary, got {comment:?}"
            );
            assert!(
                !comment.contains("Screen Recording"),
                "Non-macOS prose must not use TCC vocabulary, got {comment:?}"
            );
            assert!(
                !comment.contains("Privacy & Security"),
                "Non-macOS prose must not use TCC vocabulary, got {comment:?}"
            );

            #[cfg(target_os = "linux")]
            {
                assert!(
                    consent.rows.is_empty(),
                    "Linux has no grant to offer a row for, so it declares none (#250), got {:?}",
                    consent.rows
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

            #[cfg(target_os = "windows")]
            {
                assert_eq!(
                    consent.rows.len(),
                    2,
                    "Windows still declares the rows; #250 is about Linux"
                );
                let listed = crate::consent::process_listed_as();
                assert!(
                    comment.contains(&listed),
                    "Windows prose must name the process Privacy will list ({listed}), got {comment:?}"
                );
                assert!(
                    !comment.contains("macOS"),
                    "Windows prose must not mention macOS, got {comment:?}"
                );
            }
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

    /// The Base URL picker's help line, choices and frozen flag.
    fn base_url_picker(description: &FormDescription) -> (String, Vec<String>, bool) {
        description
            .sections()
            .flat_map(|section| &section.rows)
            .find_map(|row| match row {
                FormRow::Composite { controls, help, .. } => {
                    controls.iter().find_map(|control| match control {
                        CompositeControl::Popup {
                            id,
                            options,
                            frozen,
                        } if id == DIRECTOR_BASE_URL_PICK_ID => {
                            Some((help.clone().unwrap_or_default(), options.clone(), *frozen))
                        }
                        _ => None,
                    })
                }
                _ => None,
            })
            .expect("the Base URL picker exists")
    }

    /// The local runtimes are the point of the list: someone running one has a
    /// Completer sitting there already, and should never have to look its port
    /// up. The ports are `ENDPOINTS`' — each read off its project's own docs.
    #[test]
    fn the_base_url_picker_names_the_local_runtimes_and_their_ports() {
        let (_, options, _) = base_url_picker(&describe());

        assert_eq!(
            options.first().map(String::as_str),
            Some(ENDPOINT_CUSTOM),
            "Custom rests first, so the picker opens on what the field holds"
        );
        for (name, url) in [
            ("Ollama", "http://localhost:11434"),
            ("LM Studio", "http://localhost:1234"),
            ("oMLX", "http://localhost:8000"),
            ("llama.cpp", "http://localhost:8080"),
        ] {
            assert!(
                options.iter().any(|o| o.contains(name) && o.contains(url)),
                "{name} on {url} must be one pick away, not in {options:?}"
            );
        }
    }

    /// #447: one timeout budgets both minds. A user who reads the row as the
    /// HTTP Completer's alone cannot explain a Harness turn that was cancelled
    /// halfway, and the row is where that has to be said.
    #[test]
    fn the_timeout_row_says_it_budgets_a_harness_turn() {
        let description = describe();
        let help = development_tab(&description)
            .sections
            .iter()
            .flat_map(|section| &section.rows)
            .find_map(|row| match row {
                FormRow::TextField { id, help, .. } if id == DIRECTOR_TIMEOUT_SECS_ID => {
                    help.clone()
                }
                _ => None,
            })
            .expect("the timeout row carries help");

        assert!(
            help.contains("Harness"),
            "the help has to name the other mind it budgets, got {help:?}"
        );
        assert!(
            help.contains("session/prompt"),
            "the help has to name the call it budgets, got {help:?}"
        );
        assert!(
            help.contains("cancel"),
            "the help has to say what expiry does, got {help:?}"
        );
    }

    /// #447: both knobs are for testing, so they sit on Development rather
    /// than beside the Completer source a user picks.
    #[test]
    fn the_harness_knobs_are_development_rows() {
        let description = describe();
        let ids: Vec<&str> = development_tab(&description)
            .sections
            .iter()
            .flat_map(|section| &section.rows)
            .filter_map(row_id)
            .collect();

        for (id, writes) in [
            (HARNESS_AUTH_RETRY_SECS_ID, TextField::HarnessAuthRetrySecs),
            (MCP_BIN_ID, TextField::McpBin),
        ] {
            assert!(ids.contains(&id), "{id} is a Development row");
            assert_eq!(
                description.text_write(id),
                Some(writes),
                "{id} writes the field it names"
            );
        }
    }

    /// Every title the picker offers is one `endpoint_url` can spend, and
    /// the value it names comes back as the title that was picked. Custom is
    /// the one that writes nothing, because the field is what Custom means.
    #[test]
    fn every_endpoint_title_comes_back_as_the_base_url_it_names() {
        for title in endpoint_options() {
            match endpoint_url(&title) {
                Some(url) => assert_eq!(endpoint_title(url), title),
                None => assert_eq!(title, ENDPOINT_CUSTOM, "only Custom picks nothing"),
            }
        }
        assert_eq!(endpoint_title("https://example.invalid"), ENDPOINT_CUSTOM);
    }

    /// A picker that looks live while a variable owns the value invites a
    /// click that does nothing, which is worse than the frozen field this
    /// shortcut sits above (#465).
    #[test]
    fn an_exported_base_url_freezes_the_picker_and_says_so() {
        crate::model::tests::with_env(None, Some("http://localhost:11434"), None, || {
            let description = describe();
            let (help, _, frozen) = base_url_picker(&description);

            assert!(frozen, "the variable owns the pick as well as the field");
            assert!(description.frozen(DIRECTOR_BASE_URL_PICK_ID));
            assert!(
                help.contains("Off"),
                "the help must say the picker is off, not {help:?}"
            );
            let (label, _) = described_row(&description, DIRECTOR_BASE_URL_ID);
            assert!(
                label.contains(model::BASE_URL),
                "the field above still names the variable, not {label:?}"
            );
        });
    }

    #[test]
    fn the_base_url_picker_is_the_users_when_no_variable_is_exported() {
        crate::model::tests::with_env(None, None, None, || {
            let description = describe();
            let (help, _, frozen) = base_url_picker(&description);

            assert!(!frozen);
            assert!(!description.frozen(DIRECTOR_BASE_URL_PICK_ID));
            assert!(
                help.contains("typed"),
                "the help must keep saying a custom endpoint is typeable, not {help:?}"
            );
        });
    }

    fn source_section(description: &FormDescription) -> &FormSection {
        description
            .sections()
            .find(|section| section.heading == "AI source")
            .expect("the AI source section exists")
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

    /// Off, the named launch rows, and the escape hatch — ADR-0022's table,
    /// and nothing for Copilot or Gemini until one is smoked.
    #[test]
    fn the_completer_source_offers_off_the_presets_and_custom() {
        crate::model::tests::with_harness(None, || {
            let description = describe();
            let (_, options, _) = popup_row(&description, HARNESS_ID);
            assert_eq!(
                options,
                ["Off", "claude", "codex", "grok", "hermes", "opencode", "Custom"]
            );
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

    /// A picker commits the title it drew, and then has to find that title
    /// again to draw the pick. AppKit calls `selectItemWithTitle` and GTK
    /// selects the radio whose label matches, so an option that stored as
    /// something the file spells another way would leave the picker with
    /// nothing selected (#467).
    #[test]
    fn every_source_title_comes_back_as_the_title_that_was_picked() {
        for title in harness_options() {
            let stored = harness_choice(&title);
            assert_eq!(
                harness_rows(&stored, "opencode acp").0,
                title,
                "{title} stored as {stored:?} draws as another choice"
            );
        }
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

    /// A Harness that answers is the Completer (ADR-0008), so the three HTTP
    /// rows drive nothing and say so — and a Harness that never started is
    /// not one, because freezing them on it would leave no reachable
    /// Completer at all (#452).
    ///
    /// Asserted through `http_row` rather than a live attachment:
    /// `harness::driving` reads a process global one test may not set for the
    /// whole binary.
    #[test]
    fn only_a_harness_that_answers_takes_the_http_rows_out_of_use() {
        const ROWS: [(&str, &str); 3] = [
            ("Base URL", crate::model::BASE_URL),
            ("Model", crate::model::MODEL),
            ("API key", crate::model::API_KEY),
        ];
        for (label, var) in ROWS {
            let (label, frozen) = http_row(label, var, true, true);
            assert!(frozen, "a driving Harness discards an edit here");
            assert!(
                label.contains("not in use"),
                "the row has to say why it is dead, not {label:?}"
            );
        }
        crate::model::tests::with_env(None, None, None, || {
            for (label, var) in ROWS {
                let (label, frozen) = http_row(label, var, false, false);
                assert!(
                    !frozen,
                    "with nothing driving, {label:?} is the only Completer left"
                );
                assert!(!label.contains("not in use"));
                assert!(!label.contains("next launch"));
            }
        });
    }

    /// #469: the rows a dead Harness leaves live take an edit the Director
    /// does not read while the handle is set, because that handle stays the
    /// configured Completer (ADR-0008). Editable so there is a way back, and
    /// labelled so the wait is on screen rather than discovered.
    ///
    /// #500: the wait ends with a pick rather than a relaunch, so the label
    /// names the pick.
    #[test]
    fn a_dead_harness_leaves_the_http_rows_editable_and_names_the_way_back() {
        crate::model::tests::with_env(None, None, None, || {
            for (label, var) in [
                ("Base URL", crate::model::BASE_URL),
                ("Model", crate::model::MODEL),
                ("API key", crate::model::API_KEY),
            ] {
                let (label, frozen) = http_row(label, var, false, true);
                assert!(!frozen, "the way back has to stay typeable, got {label:?}");
                assert!(
                    !label.contains("next launch"),
                    "no edit here waits for one any more, got {label:?}"
                );
                assert!(
                    label.contains("Off"),
                    "the row has to name what ends the wait, not {label:?}"
                );
            }
        });
    }

    /// Production change that would fail this: source copy still promising a
    /// relaunch after `harness::retarget` made every pick live. A row that
    /// tells the user to restart is how #452's limit was survivable and is now
    /// just wrong (#500).
    #[test]
    fn no_completer_source_copy_promises_a_relaunch() {
        crate::model::tests::with_harness(None, || {
            let section = completer_source_section();
            let mut copy = section.comment.clone().unwrap_or_default();
            for row in &section.rows {
                if let FormRow::Popup { help, .. } | FormRow::InspectBlock { help, .. } = row {
                    copy.push(' ');
                    copy.push_str(help.as_deref().unwrap_or_default());
                }
            }
            assert!(
                !copy.to_lowercase().contains("next launch")
                    && !copy.to_lowercase().contains("restart"),
                "got {copy:?}"
            );
        });
    }
}
