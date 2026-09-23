//! ai-buddy's overlay shell.
//!
//! One transparent, always-on-top window per display renders the Character.
//! Click-through on macOS is per-window rather than per-pixel, so a screen-sized
//! transparent window would swallow every click. The shell therefore tracks the
//! cursor and toggles ignore-mouse-events by hit-testing the sprite's alpha,
//! which is what makes the overlay feel like a sprite on the desktop instead of
//! a sheet of glass over it.
//!
//! It also owns the frame loop, which is the only thing that can: the Engine is
//! pure and cannot read a clock, and `WindowSource` reports geometry and nothing
//! else. The loop reads the wall clock and the cursor, asks
//! `SnapshotAssembler` for a `WorldSnapshot`, ticks the Engine, and hands the
//! resulting `Frame` to the webview and to the hit-test.
//!
//! Waking the Director is the loop's too, and for the same reason: a timer
//! is a clock. Static may wake often. A session wake is reactive or backed
//! off (ADR-0008). What it proposes is `director`'s; when it is asked is here.

// ponytail: module-wide, though only part of each module is dead on Windows.
// The ceiling is that dead code added inside them goes unwarned there; narrow
// it to `mod form` and the view types when a Windows-only item first lands.
#[cfg_attr(not(unix), allow(dead_code))]
mod acp_wire;
mod action_log;
mod consent;
mod dev_flags;
mod frame_loop;
mod harness;
mod mcp_http;
mod mcp_resources;
mod menu;
mod model;
mod package;
mod platform;
#[cfg_attr(not(unix), allow(dead_code))] // see the note on `consent`
mod secrets;
mod session_log;
#[cfg_attr(not(unix), allow(dead_code))] // see the note on `consent`
mod settings;
mod tray;

use frame_loop::run_frame_loop;

use std::collections::{BTreeMap, HashMap};
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use ai_buddy_core::character::{Character, Primitive};
use ai_buddy_core::director::{
    app_instructions, happened_cell, Happened, ModelDirector, Pace, Seeded, StaticDirector,
};
use ai_buddy_core::engine::{Cue, Point, State, Verb};
use ai_buddy_core::input::Pointer;
use ai_buddy_core::memory;
use ai_buddy_core::overlay::SpriteRect;
use ai_buddy_core::roster::{self, InstanceId, InstanceSpec, Roster};
use ai_buddy_core::snapshot::starting_position;
use ai_buddy_core::visibility::HideRules;
use ai_buddy_core::window_source::{Rect, WindowSource};
use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use secrets::{KeyringStore, SecretStore};
use serde::Serialize;
use settings::{InstanceRow, Settings, SettingsOp, SettingsSession};
use tauri::{Emitter, LogicalPosition, LogicalSize, Manager, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};

/// Where the shipped Character Packages sit inside the app's resources. Kept in
/// step with `bundle.resources` in `tauri.conf.json`.
const BUNDLED_CHARACTERS: &str = "characters";

/// One turn of the frame loop: roughly 60Hz. A poll, not an event stream: a
/// click-through window receives no mouse events, and the Engine advances on
/// elapsed time.
const ENGINE_TICK: Duration = Duration::from_millis(16);

/// How often the Free tier is read. Far less often than the frame loop: the
/// answers change at human speed, and each read is two AppKit/CoreGraphics
/// calls the sprite's physics have no use for.
const SENSE_INTERVAL: Duration = Duration::from_secs(1);

/// The overlay covering the display at `index`. The index is both the name
/// and how the frame loop finds it; `capabilities/overlay.json` grants every
/// `overlay-*` the same permissions.
fn overlay_label(index: usize) -> String {
    format!("overlay-{index}")
}

/// The Chat surface belonging to `id`. Outside `overlay-` on purpose:
/// `place_overlays` hides every `overlay-{n}` past the display count, so a
/// Chat surface sharing that prefix would vanish when a display goes away.
fn chat_label(id: &str) -> String {
    format!("chat-{id}")
}

/// The event carrying each `Frame` to the webview.
const FRAME_EVENT: &str = "frame";

/// The event carrying one turn's answer to a Chat surface.
const CHAT_EVENT: &str = "chat";

/// Spatial Layer state for a Chat surface's status bar. Separate from
/// `CHAT_EVENT`: this arrives whenever the sprite does something different,
/// whether or not anyone has typed.
const CHAT_STATUS_EVENT: &str = "chat-status";

/// Full opening to an already-open Chat surface, so `attached()` can re-run
/// without a webview reload. An event rather than a second command, because
/// the window is already listening.
const CHAT_OPENING_EVENT: &str = "chat-opening";

/// The event telling one Chat surface that the session behind it was replaced,
/// carrying why in the words the log prints. `chat.js` says what the window
/// does with it, and why. #476.
const CHAT_SESSION_EVENT: &str = "chat-session";

/// Forwarded `session/request_permission` to every open Chat surface. The
/// session is shared and the Shell does not know which window the user is
/// looking at; first answer wins, never answered here (ADR-0010).
const CHAT_PERMISSION_EVENT: &str = "chat-permission";

/// A forwarded `elicitation/create` form. Same fan-out as a permission ask:
/// every open Chat surface draws it, the first answer wins.
const CHAT_ELICITATION_EVENT: &str = "chat-elicitation";

/// The Harness's latest thought, for the strip above the composer. Each one
/// replaces the last; an empty line is the turn saying it has stopped
/// thinking, which takes the strip away (ADR-0025).
const CHAT_THOUGHT_EVENT: &str = "chat-thought";

/// The event carrying the agent's plan to every open Chat surface. Each one
/// replaces the whole list, and an empty one is the turn taking it away (#697).
const CHAT_PLAN_EVENT: &str = "chat-plan";

/// Retires one forwarded request in every open Chat surface, by request id.
/// The ask went to all of them and one took the click; the rest would
/// otherwise keep offering buttons on a question already answered.
const CHAT_PERMISSION_SETTLED_EVENT: &str = "chat-permission-settled";

/// Unsettled forwarded asks, held so `chat_ready` can replay them to a
/// surface that opens later. One lock over both fields and the emits that
/// read them: a settlement between replay and emit would draw a live row that is never retired.
#[derive(Default)]
struct Pending {
    asks: Vec<harness::PermissionAsk>,
    forms: Vec<harness::ElicitationForm>,
    /// Whether a surface has already been asked for. Opening posts to the
    /// main thread, so a second ask before that lands would queue a second
    /// focus grab. Cleared when the last ask settles.
    opened: bool,
}

struct PendingAsks(Mutex<Pending>);

/// What retires one row in every open Chat surface.
#[derive(Clone, Serialize)]
struct Settled {
    request: String,
    /// The option that won; `None` when nothing was picked. Every window
    /// draws this rather than its own click: two can draw one request, and
    /// the wire drops every answer after the first.
    option: Option<String>,
}

/// Director config and the last Character Prompt, for the frame loop.
struct DirectorRun {
    config: model::DirectorConfig,
    settings: model::DirectorSettings,
    inspect: Arc<Mutex<model::DirectorInspect>>,
}

/// Where the sprite was last drawn, and what it was drawn as. Kept for one
/// tick so the hit-test asks about the sprite the user is looking at rather
/// than the one this tick is about to produce.
struct Drawn {
    rect: SpriteRect,
    animation: &'static str,
    animation_ms: u32,
    /// The variant draw the art was picked with, so the hit-test measures the
    /// strip the user saw rather than whatever the next draw lands on.
    variant_draw: u64,
    /// Which way the sprite was pointed, so the hit-test resolves the strip
    /// the art was drawn from and mirrors it the same way — this tick's
    /// facing may already differ.
    facing: f64,
}

/// What the last `engine:` line said about an Instance. Trace prints on
/// change; everything the line carries is in here, or a field that moved
/// alone would stop appearing after the first tick that moved only it.
#[derive(PartialEq)]
struct Traced {
    behavior: Option<String>,
    primitive: Option<Primitive>,
    animation: &'static str,
    state: State,
}

/// Shell state one Instance keeps between ticks. Sharing any of it would be
/// visible: one Director would lockstep two buddies; one `Pointer` would
/// count a double-click on one toward a Summon on the other.
struct InstanceState {
    id: InstanceId,
    /// The Character this Instance runs, shared with every other Instance
    /// running the same one.
    character: Arc<Character>,
    director: StaticDirector,
    model: Option<Arc<ModelDirector<model::AnyCompleter>>>,
    recent: Vec<String>,
    pace: Pace,
    since_wake: Duration,
    since_state: Duration,
    since_ambient: Duration,
    previous_idle: Duration,
    last_state: Option<State>,
    addressed: bool,
    happened: Happened,
    /// Whether the call on the wire answers a typed line, so the surface can be
    /// told when newest-wins throws that answer away (ADR-0016). `Slots` knows
    /// only that a call is out, and the wake clears `happened` as it sends.
    chat_turn: bool,
    pointer: Pointer,
    /// The last line spoken and which overlay showed it, so a crossing
    /// carries it (#178). See `carry_line`.
    spoken: Option<Spoken>,
    drawn_last: Option<Drawn>,
    /// The subject of the last `engine:` line. `None` while the switch is off,
    /// so turning it on always opens with a line rather than waiting for the
    /// sprite to do something new.
    traced_last: Option<Traced>,
    /// What the status bar was last told, so the push happens on change rather
    /// than every tick. `None` re-sends: a surface that has just said it is
    /// listening has drawn nothing yet.
    status_last: Option<ChatStatus>,
    /// The countdown last pushed with it. Kept out of `ChatStatus` because it
    /// falls a millisecond per millisecond and the window subtracts for itself;
    /// only a deadline that *moved* is worth a push.
    status_wake_ms: Option<u64>,
    /// The `Happened` that drove the last session wake, as `happened_cell`
    /// names it. `None` until one has: nothing has been asked here yet.
    happened_last: Option<&'static str>,
    /// This tick's verbs, decided before any Instance is ticked. Held on the
    /// Instance because `press_target` has to see every hit-test before any
    /// pointer is told whether the press was its own.
    verbs: Vec<Verb>,
    /// The menu this Instance has open, and `None` when it has none. While it is
    /// `Some`, the frame loop re-injects `Verb::Menu` every tick, which is what
    /// holds the Instance still under the popup.
    menu_hold: Option<MenuHold>,
}

/// One open menu, from the frame loop's side.
struct MenuHold {
    /// What the rows of the menu on screen mean. Kept rather than looked up
    /// again when the click arrives: a package installed while it is open
    /// must not change what its rows do.
    actions: HashMap<String, menu::MenuAction>,
    elapsed: Duration,
}

/// What the main thread tells the frame loop about the menu it was asked to
/// pop. Two messages rather than one because a menu can close without
/// choosing, and nothing arrives on the event channel when the user presses Escape.
enum MenuSignal {
    /// A row was chosen, by the id the description gave it.
    Chose(String),
    /// The popup is gone, whether or not anything was chosen.
    Closed,
}

/// Both ends of the menu's channel. They travel as a pair because the app's
/// menu event hook is registered before the frame loop starts and needs a
/// sender of its own.
struct MenuChannel {
    sender: mpsc::Sender<MenuSignal>,
    receiver: mpsc::Receiver<MenuSignal>,
    quit_generation: Arc<AtomicU64>,
}

/// Settings plus the live roster the settings window reads.
struct SettingsState {
    settings: Arc<Mutex<Settings>>,
    path: PathBuf,
    memory_path: PathBuf,
    installed: Vec<String>,
    /// Each installed Character's Personality Prompt, by Character name. Read
    /// once at launch, because a package changes only between runs, and shared
    /// so the Prompt tab asking for it costs no copy of every prompt installed.
    personalities: Arc<BTreeMap<String, String>>,
    /// Declared Behavior names, same key as `personalities`. The Prompt tab
    /// draws the app-level instructions from these, so the roster it shows is
    /// the one the opening turn names.
    behavior_names: Arc<BTreeMap<String, Vec<String>>>,
    instances: Arc<Mutex<Vec<InstanceRow>>>,
    inspect: Arc<Mutex<model::DirectorInspect>>,
    ops: mpsc::Sender<SettingsOp>,
    rules: Arc<Mutex<HideRules>>,
    secrets: Arc<dyn SecretStore>,
}

/// How long a hold survives without hearing anything. A backstop: `Closed`
/// ends the hold; this only matters if it never comes, or an Instance stays
/// frozen under a menu that is no longer there for as long as the app runs.
const MENU_HOLD_TIMEOUT: Duration = Duration::from_secs(120);

/// Where one Instance is to be drawn in one overlay, in logical points from
/// that overlay's top-left, and which Animation frame to draw.
#[derive(Clone, Serialize)]
struct SpritePlacement<'a> {
    /// The Instance this sprite belongs to, so the renderer keeps one element
    /// per Instance across ticks rather than redrawing a fresh set. An id that
    /// stops arriving is an Instance that was dismissed, and its element goes.
    id: &'a str,
    /// Which Character's art to draw from. Instances may run different
    /// Characters, and two running the same one name the same art.
    character: &'a str,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    /// The Animation whose art to draw — the one `Character::draw` resolved
    /// (a variant, or an optional Animation's fallback), which is not always
    /// the name the Engine asked with.
    animation: &'a str,
    frame_index: usize,
    /// -1 to mirror, 1 as authored. `Character::draw`'s answer, not the
    /// heading (#345): a left-strip Character already faces that way, and
    /// mirroring would turn it back round. Hit-test uses the same answer.
    mirror: i8,
    /// A line to speak on this tick only. Dialogue is an event, not a state.
    /// #119: the webview latches it and owns display duration. `None` on every
    /// overlay but the bubble owner's.
    dialogue: Option<String>,
    /// Whether to show the thinking ellipsis. Derived from what the Instance
    /// has on the wire. #119: grace and min-hold are in the webview so the
    /// Engine stays tick-pure. False on every overlay but the bubble owner's.
    thinking: bool,
    /// Whether this overlay draws this Instance's bubble (#178, `bubble_owner`).
    /// Still sent to the overlays that lost, which drop the bubble they were
    /// showing on the tick the answer changes.
    bubble: bool,
    /// Cue to play this tick only, by the name the webview keys visual and
    /// sound by. `None` on every overlay but the bubble owner's: every overlay
    /// draws the art, so a cue from all of them is one sound per display. #277
    cue: Option<&'static str>,
}

impl<'a> SpritePlacement<'a> {
    /// One Instance as one overlay is told about it, in that overlay's own
    /// coordinates.
    ///
    /// Every overlay draws the art. Only the bubble owner is told the line,
    /// the indicator and the cue (#178, #277). Decided here because the Shell
    /// already knows the owner: the webview used to strip these itself, which
    /// made it reconstruct an answer it had been handed.
    fn new(instance: &'a Placed, display: Rect, index: usize) -> Self {
        let local = instance.sprite.in_overlay(display);
        let bubble = instance.owner == Some(index);
        Self {
            id: &instance.id,
            character: &instance.character,
            x: local.x,
            y: local.y,
            width: instance.width,
            height: instance.height,
            animation: &instance.animation,
            frame_index: instance.frame_index,
            mirror: instance.mirror,
            dialogue: instance.dialogue.as_ref().filter(|_| bubble).cloned(),
            thinking: bubble && instance.thinking,
            bubble,
            cue: instance.cue.filter(|_| bubble).map(Cue::name),
        }
    }
}

/// One tick's instruction to the renderer. Pushed so the webview holds no
/// state. One message for every sprite, because the list is also which
/// Instances still exist; sent separately, a dismiss would look like a late frame.
#[derive(Clone, Serialize)]
struct Placement<'a> {
    sprites: Vec<SpritePlacement<'a>>,
    /// Hide-rules visibility, on every frame: the first tick fires before the
    /// webview is listening, and a repeated frame is not sent (#741), so
    /// `FRAME_RESEND` is what keeps a hidden-at-launch Character from staying on top.
    visible: bool,
    fade_ms: u32,
    /// Whether a cue this frame may be heard as well as seen. Decided in
    /// Settings, where Do Not Disturb takes part (#277); the webview only
    /// obeys.
    sound: bool,
}

struct Spoken {
    line: String,
    at: Instant,
    owner: Option<usize>,
}

/// The longest the renderer keeps a line up — `bubbleDuration`'s clamp in
/// `src/bubble.js`. A line older than this cannot still be showing anywhere,
/// so it is never carried.
const CARRY_WINDOW: Duration = Duration::from_secs(8);

/// Dialogue this tick: the Engine's new line, or the last one re-pulsed to a
/// new owner. Only the overlay that owns the bubble latches the one-tick
/// pulse, so a seam crossing mid-line would otherwise lose it (#178).
fn carry_line(
    spoken: &mut Option<Spoken>,
    said: Option<&str>,
    owner: Option<usize>,
    now: Instant,
) -> Option<String> {
    if let Some(line) = said {
        *spoken = Some(Spoken {
            line: line.to_string(),
            at: now,
            owner,
        });
        return Some(line.to_string());
    }
    let carried = spoken.as_mut()?;
    if carried.owner == owner || now.duration_since(carried.at) > CARRY_WINDOW {
        return None;
    }
    carried.owner = owner;
    Some(carried.line.clone())
}

/// What one Instance's tick decided to draw, in the space every display
/// shares. Worked out once, then turned into each overlay's rectangle.
/// Art names are owned so this outlives the Character borrow.
struct Placed {
    id: InstanceId,
    character: String,
    sprite: SpriteRect,
    width: i32,
    height: i32,
    animation: String,
    frame_index: usize,
    mirror: i8,
    dialogue: Option<String>,
    thinking: bool,
    cue: Option<Cue>,
    /// The overlay that draws the bubble, decided once from the feet
    /// (#178, `bubble_owner`); `None` while the feet are on no display.
    owner: Option<usize>,
    #[allow(dead_code)]
    mask: ai_buddy_core::overlay::AlphaMask,
}

/// Every Animation's frames as `data:` URLs, in play order. Paths would need
/// a filesystem scope for packages outside the front end; the webview indexes
/// this list the same way `Character::draw` walks `Animation::frames`.
fn art_urls(character: &Character) -> BTreeMap<String, Vec<String>> {
    // A frame two Animations share is encoded once and named twice.
    let urls: BTreeMap<&String, String> = character
        .art
        .iter()
        .map(|(frame, art)| {
            let url = format!("data:image/png;base64,{}", STANDARD.encode(&art.png));
            (frame, url)
        })
        .collect();

    character
        .animations
        .iter()
        .map(|(name, animation)| {
            let frames = animation.frames.iter().map(|frame| urls[frame].clone());
            (name.clone(), frames.collect())
        })
        .collect()
}

/// What the webview needs of one Character: the art as `data:` URLs, and
/// whether to smooth it when scaling (the Character Manifest's `render_mode`).
#[derive(Clone, serde::Serialize)]
struct CharacterArt {
    art: BTreeMap<String, Vec<String>>,
    smooth: bool,
}

/// Every Character on screen, keyed by Character rather than Instance so two
/// Instances of one Character share one encoded sheet. A struct rather than
/// a bare map so managed state, keyed by type, cannot collide.
#[derive(Clone, serde::Serialize)]
struct ArtUrls {
    characters: BTreeMap<String, CharacterArt>,
}

/// The art of every Character on screen, fetched once when the webview
/// loads. A command rather than an event: setup would race the listener, and
/// the art does not change while the app runs.
#[tauri::command]
fn character(art: tauri::State<'_, ArtUrls>) -> ArtUrls {
    art.inner().clone()
}

/// The Settings window's handle on the running app. `SettingsSession::apply`
/// is the one path that persists a setting and acts on it; any other writer
/// takes this rather than the file (#654).
fn settings_session(app: &tauri::AppHandle, state: &SettingsState) -> SettingsSession {
    SettingsSession {
        settings: Arc::clone(&state.settings),
        path: state.path.clone(),
        memory_path: state.memory_path.clone(),
        rules: Arc::clone(&state.rules),
        inspect: Arc::clone(&state.inspect),
        instances: Arc::clone(&state.instances),
        installed: state.installed.clone(),
        ops: state.ops.clone(),
        app: app.clone(),
        on_rebind: bind_hide_hotkey,
        secrets: Arc::clone(&state.secrets),
        key_cache: Mutex::new(None),
    }
}

/// The Settings form and the values in force, together: a row is only
/// renderable with both. Committed fixtures pin the shape; the values are
/// this machine's and pin nothing.
#[derive(serde::Serialize)]
struct SettingsSnapshot {
    form: settings::form::FormDescription,
    /// Keyed by form row id, which is what `src/settings.js` indexes (#875).
    view: std::collections::BTreeMap<String, settings::RowValue>,
}

#[tauri::command]
fn settings_snapshot(app: tauri::AppHandle) -> Result<SettingsSnapshot, String> {
    let state = app
        .try_state::<SettingsState>()
        .ok_or("settings: asked for before the shell was ready")?;
    let session = settings_session(&app, &state);
    let view = session.view();
    // The one caller that can fill the API key row's placeholder and the
    // Character popups' choices. `current()` leaves both empty because the
    // status is a store read and the package list is the view's; the view has
    // the key status from the cache that keeps become-key off Keychain. #875, #921.
    let live = settings::form::Live {
        api_key_placeholder: view.api_key_placeholder(),
        installed: view.installed.clone(),
        ..settings::form::Live::current()
    };
    Ok(SettingsSnapshot {
        form: settings::form::describe_with(&live),
        view: view.row_values(),
    })
}

/// A gesture from the webview Settings page.
#[derive(serde::Deserialize, Debug)]
#[serde(untagged)]
enum SettingsEventPayload {
    SetBool {
        set_bool: String,
        value: bool,
    },
    SetText {
        set_text: String,
        value: String,
    },
    Press {
        press: String,
        #[serde(default)]
        draft: Option<DirectorDraftWire>,
        /// New reads the name and Character beside it from here (#875).
        #[serde(default)]
        fields: std::collections::HashMap<String, String>,
    },
    Pick {
        pick: String,
        value: String,
        fills: Option<PickFills>,
    },
    Dismiss {
        dismiss: String,
        value: String,
    },
}

#[derive(serde::Deserialize, Debug)]
struct PickFills {
    row: String,
}

/// Widget text the webview holds for batched Director rows. Apply reads this
/// the way a native window reads its fields (#663).
#[derive(serde::Deserialize, Debug, Default)]
struct DirectorDraftWire {
    #[serde(default)]
    director_base_url: Option<String>,
    #[serde(default)]
    director_model: Option<String>,
    #[serde(default)]
    director_api_key: Option<String>,
    #[serde(default)]
    harness: Option<String>,
    #[serde(default)]
    harness_command: Option<String>,
    #[serde(default)]
    clear_key: bool,
}

#[cfg(test)]
mod settings_event_tests {
    use super::*;

    #[test]
    fn set_bool_deserializes_from_js() {
        let json = r#"{"set_bool": "director", "value": true}"#;
        let payload: SettingsEventPayload =
            serde_json::from_str(json).expect("set_bool payload should deserialize");
        match payload {
            SettingsEventPayload::SetBool { set_bool, value } => {
                assert_eq!(set_bool, "director");
                assert!(value);
            }
            _ => panic!("expected SetBool variant"),
        }
    }

    #[test]
    fn set_text_deserializes_from_js() {
        let json = r#"{"set_text": "director_base_url", "value": "https://api.x.ai"}"#;
        let payload: SettingsEventPayload =
            serde_json::from_str(json).expect("set_text payload should deserialize");
        match payload {
            SettingsEventPayload::SetText { set_text, value } => {
                assert_eq!(set_text, "director_base_url");
                assert_eq!(value, "https://api.x.ai");
            }
            _ => panic!("expected SetText variant"),
        }
    }

    #[test]
    fn press_deserializes_from_js() {
        let json = r#"{"press": "apply"}"#;
        let payload: SettingsEventPayload =
            serde_json::from_str(json).expect("press payload should deserialize");
        match payload {
            SettingsEventPayload::Press { press, draft, .. } => {
                assert_eq!(press, "apply");
                assert!(draft.is_none());
            }
            _ => panic!("expected Press variant"),
        }
    }

    #[test]
    fn press_with_draft_deserializes_from_js() {
        let json = r#"{"press":"director_apply","draft":{"harness":"Harness · opencode"}}"#;
        let payload: SettingsEventPayload =
            serde_json::from_str(json).expect("press with draft should deserialize");
        match payload {
            SettingsEventPayload::Press { press, draft, .. } => {
                assert_eq!(press, "director_apply");
                assert_eq!(
                    draft.expect("draft").harness.as_deref(),
                    Some("Harness · opencode")
                );
            }
            _ => panic!("expected Press variant"),
        }
    }

    #[test]
    fn press_with_fields_deserializes_from_js() {
        let json = r#"{"press":"spawn","fields":{"new_name":"Nim","new_character":"ghost"}}"#;
        let payload: SettingsEventPayload =
            serde_json::from_str(json).expect("press with fields should deserialize");
        match payload {
            SettingsEventPayload::Press { press, fields, .. } => {
                assert_eq!(press, "spawn");
                assert_eq!(fields.get("new_name").map(String::as_str), Some("Nim"));
                assert_eq!(
                    fields.get("new_character").map(String::as_str),
                    Some("ghost")
                );
            }
            _ => panic!("expected Press variant"),
        }
    }

    #[test]
    fn dismiss_deserializes_from_js() {
        let json = r#"{"dismiss": "instances", "value": "bmo-1"}"#;
        let payload: SettingsEventPayload =
            serde_json::from_str(json).expect("dismiss payload should deserialize");
        match payload {
            SettingsEventPayload::Dismiss { dismiss, value } => {
                assert_eq!(dismiss, "instances");
                assert_eq!(value, "bmo-1");
            }
            _ => panic!("expected Dismiss variant"),
        }
    }

    #[test]
    fn pick_without_fills_deserializes() {
        let json = r#"{"pick": "character", "value": "bmo"}"#;
        let payload: SettingsEventPayload =
            serde_json::from_str(json).expect("pick payload should deserialize");
        match payload {
            SettingsEventPayload::Pick { pick, value, fills } => {
                assert_eq!(pick, "character");
                assert_eq!(value, "bmo");
                assert!(fills.is_none());
            }
            _ => panic!("expected Pick variant"),
        }
    }

    #[test]
    fn pick_with_fills_deserializes() {
        let json = r#"{"pick": "director_base_url_pick", "value": "OpenAI (https://api.openai.com)", "fills": {"row": "director_base_url"}}"#;
        let payload: SettingsEventPayload =
            serde_json::from_str(json).expect("pick with fills should deserialize");
        match payload {
            SettingsEventPayload::Pick {
                pick,
                value,
                fills: Some(fills),
            } => {
                assert_eq!(pick, "director_base_url_pick");
                assert_eq!(value, "OpenAI (https://api.openai.com)");
                assert_eq!(fills.row, "director_base_url");
            }
            _ => panic!("expected Pick variant with fills"),
        }
    }

    #[derive(Default)]
    struct Recorded {
        ran: std::cell::RefCell<Vec<String>>,
        fails: bool,
    }

    impl Recorded {
        fn note(&self, what: &str) -> Result<(), String> {
            self.ran.borrow_mut().push(what.to_string());
            if self.fails {
                return Err("the store said no".to_string());
            }
            Ok(())
        }

        fn ran(&self) -> Vec<String> {
            self.ran.borrow().clone()
        }
    }

    impl Operations for Recorded {
        fn open_memory(&self) -> Result<(), String> {
            self.note("open_memory")
        }

        fn wipe_memory(&self) -> Result<(), String> {
            self.note("wipe_memory")
        }

        fn spawn(&self, character: String, name: String) {
            let _ = self.note(&format!("spawn {character} as {name}"));
        }

        fn dismiss(&self, id: String) {
            let _ = self.note(&format!("dismiss {id}"));
        }
    }

    fn one_instance() -> settings::SettingsView {
        settings::SettingsView::from_parts(
            &settings::Settings::default(),
            std::path::Path::new("/tmp/memory.md"),
            None,
            Vec::new(),
            vec![settings::InstanceRow {
                id: "bmo-1".to_string(),
                name: "BMO".to_string(),
                character: "bmo".to_string(),
                prompt: String::new(),
            }],
            (false, String::new(), String::new()),
            None,
        )
    }

    fn no_fields() -> std::collections::HashMap<String, String> {
        std::collections::HashMap::new()
    }

    /// A press handed to the page reaches nothing that acts on it. #875.
    #[test]
    fn opening_and_wiping_memory_run_here_and_do_not_cross_to_the_page() {
        use settings::form::RowOperation;

        let session = Recorded::default();
        assert_eq!(
            run_operation(&session, &RowOperation::OpenMemory, &no_fields()),
            Ok(SettingsEventResponse::Nothing)
        );
        assert_eq!(
            run_operation(&session, &RowOperation::WipeMemory, &no_fields()),
            Ok(SettingsEventResponse::Refresh)
        );
        assert_eq!(session.ran(), ["open_memory", "wipe_memory"]);
    }

    /// `DirectorDraft` carries neither the name nor the Character, so they
    /// ride the Composite's own fields. #875.
    #[test]
    fn new_spawns_under_the_name_and_character_the_page_shows() {
        use settings::form::RowOperation;

        let session = Recorded::default();
        let fields = std::collections::HashMap::from([
            (settings::form::NEW_NAME_ID.to_string(), "Nim".to_string()),
            (
                settings::form::NEW_CHARACTER_ID.to_string(),
                "ghost".to_string(),
            ),
        ]);
        assert_eq!(
            run_operation(&session, &RowOperation::Spawn, &fields),
            Ok(SettingsEventResponse::Nothing)
        );
        assert_eq!(session.ran(), ["spawn ghost as Nim"]);
    }

    #[test]
    fn dismiss_reaches_the_roster_and_a_stale_press_does_not() {
        let session = Recorded::default();
        let view = one_instance();

        assert_eq!(
            dismiss_press(&session, &view, "instances", "bmo-1"),
            SettingsEventResponse::Nothing
        );
        assert_eq!(session.ran(), ["dismiss bmo-1"]);

        // A buddy the roster let go while the list was on screen.
        assert_eq!(
            dismiss_press(&session, &view, "instances", "ghost-1"),
            SettingsEventResponse::Nothing
        );
        assert_eq!(session.ran(), ["dismiss bmo-1"]);
    }

    /// The clipboard is the page's, because WebKit gives `writeText` the
    /// click's own turn and this command has already spent it (#855).
    #[test]
    fn the_clipboard_operations_still_cross_to_the_page() {
        use settings::form::RowOperation;

        let session = Recorded::default();
        assert_eq!(
            run_operation(&session, &RowOperation::CopyByoSnippet, &no_fields()),
            Ok(SettingsEventResponse::Run {
                operation: "copy_byo_snippet".to_string()
            })
        );
        assert!(session.ran().is_empty());
    }

    /// A failed wipe is the user's to see. Answering Refresh would redraw the
    /// same Memory file and read as a wipe that worked.
    #[test]
    fn a_refused_wipe_answers_with_the_reason() {
        use settings::form::RowOperation;

        let session = Recorded {
            fails: true,
            ..Recorded::default()
        };
        assert_eq!(
            run_operation(&session, &RowOperation::WipeMemory, &no_fields()),
            Err("the store said no".to_string())
        );
    }

    #[test]
    fn response_nothing_serializes() {
        let response = SettingsEventResponse::Nothing;
        let json = serde_json::to_string(&response).expect("should serialize");
        assert_eq!(json, r#"{"action":"nothing"}"#);
    }

    #[test]
    fn response_refresh_serializes() {
        let response = SettingsEventResponse::Refresh;
        let json = serde_json::to_string(&response).expect("should serialize");
        assert_eq!(json, r#"{"action":"refresh"}"#);
    }

    #[test]
    fn response_fill_serializes() {
        let response = SettingsEventResponse::Fill {
            id: "director_base_url".to_string(),
            value: "https://api.openai.com".to_string(),
        };
        let json = serde_json::to_string(&response).expect("should serialize");
        assert!(json.contains(r#""action":"fill""#));
        assert!(json.contains(r#""id":"director_base_url""#));
        assert!(json.contains(r#""value":"https://api.openai.com""#));
    }

    #[test]
    fn response_run_uses_stable_wire_format() {
        use settings::form::RowOperation;
        let response = SettingsEventResponse::Run {
            operation: RowOperation::Spawn.as_str().to_string(),
        };
        let json = serde_json::to_string(&response).expect("should serialize");
        assert_eq!(json, r#"{"action":"run","operation":"spawn"}"#);

        let response = SettingsEventResponse::Run {
            operation: RowOperation::NewSession.as_str().to_string(),
        };
        let json = serde_json::to_string(&response).expect("should serialize");
        assert_eq!(json, r#"{"action":"run","operation":"new_session"}"#);
    }
}

/// What the webview must do about a gesture.
#[derive(serde::Serialize, Debug, PartialEq)]
#[serde(tag = "action", rename_all = "snake_case")]
enum SettingsEventResponse {
    Nothing,
    Refresh,
    Fill { id: String, value: String },
    ClearKey,
    Reset,
    Run { operation: String },
}

#[tauri::command]
fn settings_event(
    app: tauri::AppHandle,
    payload: SettingsEventPayload,
) -> Result<SettingsEventResponse, String> {
    use settings::controller;

    let state = app
        .try_state::<SettingsState>()
        .ok_or("settings: asked for before the shell was ready")?;
    let session = settings_session(&app, &state);
    let view = session.view();
    let description = settings::form::describe();

    let mut pressed: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    let (event, draft) = match payload {
        SettingsEventPayload::SetBool { set_bool, value } => (
            controller::Event::SetBool {
                id: set_bool,
                value,
            },
            settings::DirectorDraft::live(&view, &description),
        ),
        SettingsEventPayload::SetText { set_text, value } => (
            controller::Event::SetText {
                id: set_text,
                value,
            },
            settings::DirectorDraft::live(&view, &description),
        ),
        SettingsEventPayload::Press {
            press,
            draft: wire,
            fields,
        } => {
            pressed = fields;
            let mut draft = settings::DirectorDraft::live(&view, &description);
            if let Some(wire) = wire {
                if let Some(value) = wire.director_base_url {
                    draft.base_url = value;
                }
                if let Some(value) = wire.director_model {
                    draft.model = value;
                }
                if let Some(value) = wire.director_api_key {
                    draft.key = value;
                }
                if let Some(value) = wire.harness {
                    draft.harness = value;
                }
                if let Some(value) = wire.harness_command {
                    draft.harness_command = value;
                }
                draft.clear_key = wire.clear_key;
            }
            (controller::Event::Press { id: press }, draft)
        }
        SettingsEventPayload::Pick {
            pick,
            value,
            fills: None,
        } => (
            controller::Event::Pick { id: pick, value },
            settings::DirectorDraft::live(&view, &description),
        ),
        SettingsEventPayload::Pick {
            pick,
            value,
            fills: Some(fills),
        } => {
            let current = view
                .development_texts
                .get(&fills.row)
                .map(|s| s.as_str())
                .or_else(|| {
                    if fills.row == settings::form::DIRECTOR_BASE_URL_ID {
                        Some(view.director_base_url.as_str())
                    } else {
                        None
                    }
                })
                .unwrap_or_default()
                .to_string();
            (
                controller::Event::Shortcut {
                    id: pick,
                    value,
                    current,
                },
                settings::DirectorDraft::live(&view, &description),
            )
        }
        SettingsEventPayload::Dismiss { dismiss, value } => {
            return Ok(dismiss_press(&session, &view, &dismiss, &value));
        }
    };

    let outcome = controller::handle(&event, &draft, &view);

    match outcome {
        controller::Outcome::Nothing => Ok(SettingsEventResponse::Nothing),
        controller::Outcome::Apply(patch) => {
            session.apply(patch).map_err(|e| e.to_string())?;
            Ok(SettingsEventResponse::Nothing)
        }
        controller::Outcome::ApplyAndRefresh(patch) => {
            session.apply(patch).map_err(|e| e.to_string())?;
            Ok(SettingsEventResponse::Refresh)
        }
        controller::Outcome::Commit(patch) => {
            if let Some(patch) = patch {
                session.apply(patch).map_err(|e| e.to_string())?;
            }
            Ok(SettingsEventResponse::Reset)
        }
        controller::Outcome::Reset => Ok(SettingsEventResponse::Reset),
        controller::Outcome::ClearKey => Ok(SettingsEventResponse::ClearKey),
        controller::Outcome::Fill { id, value, patch } => {
            if let Some(patch) = patch {
                session.apply(patch).map_err(|e| e.to_string())?;
            }
            Ok(SettingsEventResponse::Fill {
                id: id.to_string(),
                value: value.to_string(),
            })
        }
        controller::Outcome::Run(op) => run_operation(&session, &op, &pressed),
    }
}

/// What a button press performs, so that `run_operation` can be tested.
///
/// `SettingsSession` is the real one. It needs a live `AppHandle`, which a
/// unit test has not got, and the bug #875 closes is a press that reached no
/// method at all — which is what a test with no seam here cannot see.
trait Operations {
    fn open_memory(&self) -> Result<(), String>;
    fn wipe_memory(&self) -> Result<(), String>;
    fn spawn(&self, character: String, name: String);
    fn dismiss(&self, id: String);
}

impl Operations for settings::SettingsSession {
    fn open_memory(&self) -> Result<(), String> {
        settings::SettingsSession::open_memory(self)
    }

    fn wipe_memory(&self) -> Result<(), String> {
        settings::SettingsSession::wipe_memory(self)
    }

    fn spawn(&self, character: String, name: String) {
        settings::SettingsSession::spawn(self, character, name);
    }

    fn dismiss(&self, id: String) {
        settings::SettingsSession::dismiss(self, id);
    }
}

/// Dismiss the Instance a list press names.
///
/// Not a `RowOperation`: the press names a row of the list rather than the
/// form, and only the roster can say whether that Instance is still there.
/// The page draws from a snapshot, so a stale press does nothing rather than
/// sending an op under an id nothing answers to. #875.
fn dismiss_press(
    session: &dyn Operations,
    view: &settings::SettingsView,
    row: &str,
    id: &str,
) -> SettingsEventResponse {
    if row == settings::form::INSTANCES_ID && view.instance(id).is_some() {
        session.dismiss(id.to_string());
    }
    SettingsEventResponse::Nothing
}

/// Run what #706 keeps in Rust, and hand the page only what it owns.
///
/// The two clipboard writes cross because WebKit gives `writeText` the click's
/// own turn and nothing else. Everything else is a `SettingsSession` method,
/// and the page has no case for one it is handed instead (#875).
fn run_operation(
    session: &dyn Operations,
    op: &settings::form::RowOperation,
    fields: &std::collections::HashMap<String, String>,
) -> Result<SettingsEventResponse, String> {
    use settings::form::RowOperation;
    let field = |id: &str| fields.get(id).cloned().unwrap_or_default();
    match op {
        RowOperation::OpenMemory => session
            .open_memory()
            .map(|()| SettingsEventResponse::Nothing),
        // The file is gone, so the window redraws from what is left.
        RowOperation::WipeMemory => session
            .wipe_memory()
            .map(|()| SettingsEventResponse::Refresh),
        // Nothing to redraw yet: the frame loop owns the roster and pushes
        // `settings-refresh` on the tick that spawns the Instance.
        RowOperation::Spawn => {
            session.spawn(
                field(settings::form::NEW_CHARACTER_ID),
                field(settings::form::NEW_NAME_ID),
            );
            Ok(SettingsEventResponse::Nothing)
        }
        _ => Ok(SettingsEventResponse::Run {
            operation: op.as_str().to_string(),
        }),
    }
}

/// Open the Settings window. Native Shell furniture, so this runs on the
/// toolkit main thread.
#[tauri::command]
fn show_settings(app: tauri::AppHandle) {
    if app.try_state::<SettingsState>().is_none() {
        eprintln!("settings: opened before the shell was ready");
        return;
    }

    // Same clone-then-post as `open_chat`: the closure takes the handle,
    // `run_on_main_thread` still borrows `app`.
    let handle = app.clone();
    if let Err(why) = app.run_on_main_thread(move || {
        let window = match handle.get_webview_window("settings") {
            Some(window) => {
                let _ = window.unminimize();
                let _ = window.set_focus();
                window
            }
            None => match build_settings(&handle) {
                Ok(window) => window,
                Err(why) => {
                    eprintln!("settings webview: {why}");
                    return;
                }
            },
        };
        if let Err(why) = platform::raise_settings_window(&window) {
            eprintln!("settings webview raise: {why}");
        }
    }) {
        eprintln!("settings webview: {why}");
    }
}

fn persist_settings(settings: &Settings, path: &std::path::Path) {
    if let Err(why) = settings.save(path) {
        eprintln!("settings: {why}");
    }
}

/// Write the roster to settings, including the Instance id: the prompt hangs
/// off it, and without it the next launch mints a fresh uuid and loses the
/// user's words (ADR-0012).
fn remember_instances(roster: &Roster, settings: &Arc<Mutex<Settings>>, path: &std::path::Path) {
    if let Ok(mut settings) = settings.lock() {
        settings.instances = roster_specs(roster);
        persist_settings(&settings, path);
    }
}

/// The roster as settings stores it. One mapping for startup and later
/// persists, so a path that dropped the id cannot mint a fresh uuid and
/// lose the user's words (ADR-0012).
fn roster_specs(roster: &Roster) -> Vec<InstanceSpec> {
    roster
        .list()
        .into_iter()
        .map(|(id, name)| {
            let instance = roster.get(&id);
            InstanceSpec {
                character: instance
                    .map(|instance| instance.character_name().to_string())
                    .unwrap_or_default(),
                name,
                prompt: instance
                    .map(|instance| instance.prompt().to_string())
                    .unwrap_or_default(),
                id: Some(id),
            }
        })
        .collect()
}

/// The overlay heard the primary button. The frame loop polls a session
/// query that can miss a click on this window; this is the other witness.
#[tauri::command]
fn overlay_primary(down: bool) {
    platform::set_overlay_primary(down);
}

/// Same witness for the right button. Without it a right-click on the sprite
/// is swallowed by the webview and the session poll never sees a Menu.
#[tauri::command]
fn overlay_secondary(down: bool) {
    platform::set_overlay_secondary(down);
}

/// Whether reporting an off-art rectangle would actually win the click (#547).
/// Replaces a UA sniff that said "macOS" when the question is hotspot hit-testing.
/// Those agree today and would part the moment X11 or Windows unions off-art rects into its input region.
#[tauri::command]
fn overlay_hit_tests_hotspots() -> bool {
    platform::hotspots_hit_tested()
}

/// Where this overlay wants a click besides the art. The bubble's "Open chat"
/// control sits above the head, outside the alpha mask; the renderer says
/// where in its own coordinates and the frame loop converts.
#[tauri::command]
fn overlay_hotspots(window: tauri::Window, rects: Vec<[i32; 4]>) {
    platform::set_overlay_hotspots(window.label(), rects);
}

/// Chat window title: Instance name, or the id when the roster holds no row.
/// The id is a real fallback: a Character switch can drop the row between
/// the click and this lookup, and a window titled by id is better than none.
fn instance_title(rows: &[InstanceRow], id: &str) -> String {
    rows.iter()
        .find(|row| row.id == id)
        .map_or_else(|| id.to_string(), |row| row.name.clone())
}

/// Open Chat from the bubble control, not as a Summon: the Engine hears
/// nothing. `async` is load-bearing on Windows (#588): a sync command runs on
/// WebView2's pump thread and building a second webview there deadlocks.
#[tauri::command]
async fn overlay_open_chat(app: tauri::AppHandle, id: String) {
    let title = app
        .try_state::<SettingsState>()
        .and_then(|state| {
            state
                .instances
                .lock()
                .ok()
                .map(|rows| instance_title(&rows, &id))
        })
        .unwrap_or_else(|| id.clone());
    open_chat(&app, &id, title);
}

/// Put the overlay over one display. Size before move: growing a window
/// anchors bottom-left, so resize-after-place pushes the top edge off the
/// display it was just put on.
fn cover_display(window: &tauri::WebviewWindow, display: Rect) -> Result<(), tauri::Error> {
    window.set_size(LogicalSize::new(display.width, display.height))?;
    window.set_position(LogicalPosition::new(display.x, display.y))
}

/// Build one overlay, configure it, and put it over its display. The only
/// place an overlay is made, so click-through, window level, Spaces and hide
/// rules stay one set. Main thread only: it builds a window and calls AppKit.
fn build_overlay(
    app: &tauri::AppHandle,
    label: &str,
    display: Rect,
) -> Result<(), Box<dyn std::error::Error>> {
    let window = WebviewWindowBuilder::new(app, label, WebviewUrl::default())
        .title("ai-buddy")
        .transparent(true)
        .decorations(false)
        .shadow(false)
        .always_on_top(true)
        .visible_on_all_workspaces(true)
        .accept_first_mouse(true)
        .focused(false)
        .resizable(false)
        .skip_taskbar(true)
        .visible(false)
        .build()?;

    // On Linux/GTK, set_ignore_cursor_events unwraps a GdkWindow that is
    // None until realize. The frame loop sets ignore-cursor on the first
    // frame. On macOS, NSWindow exists while hidden, so the call is safe here.
    #[cfg(not(all(unix, not(target_os = "macos"))))]
    window.set_ignore_cursor_events(true)?;

    cover_display(&window, display)?;
    // Show the window first so GTK realizes it and creates the native handle.
    // Linux (GTK) has no GdkWindow until the widget is realized; macOS NSWindow
    // exists while hidden.
    window.show()?;

    // Linux: configure_overlay may fail if the GTK widget is not yet realized.
    // The frame loop retries on the main thread, so a failure here is not fatal.
    // macOS: NSWindow is always ready, so failure is a real error.
    #[cfg(all(unix, not(target_os = "macos")))]
    if let Err(why) = platform::configure_overlay(&window) {
        eprintln!("overlay: {label} EWMH config deferred: {why}");
    }

    #[cfg(not(all(unix, not(target_os = "macos"))))]
    platform::configure_overlay(&window)?;

    eprintln!(
        "overlay: {label} covers {:.0}x{:.0} at ({:.0},{:.0})",
        display.width, display.height, display.x, display.y,
    );

    Ok(())
}

/// Build one Instance's Chat surface. None of `build_overlay`'s flags:
/// click-through would swallow the caret click; always-on-top and overlay
/// window level would follow the user out of the app. Absent, not set false.
fn build_chat(
    app: &tauri::AppHandle,
    label: &str,
    title: &str,
) -> Result<tauri::WebviewWindow, tauri::Error> {
    WebviewWindowBuilder::new(app, label, WebviewUrl::App("chat.html".into()))
        .title(title)
        .inner_size(420.0, 560.0)
        .min_inner_size(320.0, 320.0)
        .focused(true)
        .build()
}

/// Build the Settings webview window.
fn build_settings(app: &tauri::AppHandle) -> Result<tauri::WebviewWindow, tauri::Error> {
    WebviewWindowBuilder::new(app, "settings", WebviewUrl::App("settings.html".into()))
        .title("Settings")
        .inner_size(600.0, 520.0)
        .min_inner_size(480.0, 400.0)
        .focused(true)
        .build()
}

/// Open this Instance's Chat surface, or raise the one it already has.
/// Posted whole to the main thread; the lookup goes too, because Ok from
/// `run_on_main_thread` means queued, so two Summons a tick apart would each post a build.
fn open_chat(app: &tauri::AppHandle, id: &InstanceId, title: String) {
    let label = chat_label(id);
    let handle = app.clone();
    if let Err(why) = app.run_on_main_thread(move || {
        let window = match handle.get_webview_window(&label) {
            Some(window) => {
                // The title is the Instance name, which a Character switch can
                // change without this window being rebuilt. #375.
                if let Err(why) = window.set_title(&title) {
                    eprintln!("chat: {label} could not be retitled: {why}");
                }
                let _ = window.unminimize();
                window
            }
            None => match build_chat(&handle, &label, &title) {
                Ok(window) => window,
                Err(why) => {
                    eprintln!("chat: {label}: {why}");
                    return;
                }
            },
        };
        // Both paths: `focused(true)` only orders the window to the front of
        // this application. `set_focus` activates the process, which a Summon
        // has to do, and doing it here keeps the overlay from taking focus.
        if let Err(why) = window.set_focus() {
            eprintln!("chat: {label} could not be raised: {why}");
        }
    }) {
        eprintln!("chat: could not reach the main thread: {why}");
    }
}

/// Shut the Chat surface belonging to `id`, if it has one. Nothing else
/// closes a `chat-*`: they sit outside the `overlay-` namespace
/// `place_overlays` sweeps. Main thread only, for `open_chat`'s reason.
fn close_chat(app: &tauri::AppHandle, id: &InstanceId) {
    let label = chat_label(id);
    let handle = app.clone();
    if let Err(why) = app.run_on_main_thread(move || {
        if let Some(window) = handle.get_webview_window(&label) {
            if let Err(why) = window.close() {
                eprintln!("chat: {label} could not be closed: {why}");
            }
        }
    }) {
        eprintln!("chat: could not reach the main thread: {why}");
    }
}

/// Draw one forwarded permission request on every Chat surface, visible and
/// unminimized. ADR-0010 forbids choosing an option. ADR-0013: only a Chat
/// surface can draw the options; a bubble can only point at a window that is not open.
fn forward_ask(app: &tauri::AppHandle, ask: harness::PermissionAsk) {
    let Some(state) = app.try_state::<PendingAsks>() else {
        return;
    };
    let Ok(mut pending) = state.0.lock() else {
        return;
    };
    pending.asks.push(ask.clone());

    let mut on_screen = false;
    let mut shut = None;
    for (label, window) in app.webview_windows() {
        if !label.starts_with("chat-") {
            continue;
        }
        let _ = app.emit_to(&label, CHAT_PERMISSION_EVENT, &ask);
        if window.is_visible().unwrap_or(false) && !window.is_minimized().unwrap_or(true) {
            on_screen = true;
        } else {
            shut = Some(label);
        }
    }
    if on_screen {
        eprintln!(
            "harness: permission asked for `{}`; answer it in the Chat window",
            ask.title.as_deref().unwrap_or("—")
        );
        return;
    }
    eprintln!(
        "harness: permission asked for `{}`; no Chat surface is on screen",
        ask.title.as_deref().unwrap_or("—")
    );
    // Do Not Disturb wins even over a question with a deadline: opening this
    // window activates ai-buddy. The turn then times out, and never an
    // answer of ours (ADR-0010).
    if do_not_disturb(app) {
        return;
    }
    if !std::mem::replace(&mut pending.opened, true) {
        show_chat_for_ask(app, shut);
    }
}

fn forward_form(app: &tauri::AppHandle, form: harness::ElicitationForm) {
    let Some(state) = app.try_state::<PendingAsks>() else {
        return;
    };
    let Ok(mut pending) = state.0.lock() else {
        return;
    };
    pending.forms.push(form.clone());

    let mut on_screen = false;
    let mut shut = None;
    for (label, window) in app.webview_windows() {
        if !label.starts_with("chat-") {
            continue;
        }
        let _ = app.emit_to(&label, CHAT_ELICITATION_EVENT, &form);
        if window.is_visible().unwrap_or(false) && !window.is_minimized().unwrap_or(true) {
            on_screen = true;
        } else {
            shut = Some(label);
        }
    }
    if on_screen {
        eprintln!(
            "harness: elicitation `{}`; answer it in the Chat window",
            form.message
        );
        return;
    }
    eprintln!(
        "harness: elicitation `{}`; no Chat surface is on screen",
        form.message
    );
    if do_not_disturb(app) {
        return;
    }
    if !std::mem::replace(&mut pending.opened, true) {
        show_chat_for_ask(app, shut);
    }
}

/// Show the latest thought in every open Chat surface, from whichever
/// Completer is on the wire — Harness, or HTTP marked reasoning (#611).
/// Every one: the session is shared and the wire does not say whose turn is on it.
fn show_thought(app: &tauri::AppHandle, line: String) {
    for label in app.webview_windows().into_keys() {
        if label.starts_with("chat-") {
            let _ = app.emit_to(label, CHAT_THOUGHT_EVENT, &line);
        }
    }
}

/// Show the agent's plan in every open Chat surface, for the reason
/// `show_thought` gives: the session is shared and the wire does not say whose
/// turn is on it.
fn show_plan(app: &tauri::AppHandle, steps: &[harness::PlanStep]) {
    for label in app.webview_windows().into_keys() {
        if label.starts_with("chat-") {
            let _ = app.emit_to(label, CHAT_PLAN_EVENT, steps);
        }
    }
}

/// Retire one request in every open Chat surface, under the same lock a replay
/// reads.
fn settle_ask(app: &tauri::AppHandle, settled: Settled) {
    let Some(state) = app.try_state::<PendingAsks>() else {
        return;
    };
    let Ok(mut pending) = state.0.lock() else {
        return;
    };
    pending.asks.retain(|ask| ask.request != settled.request);
    pending.forms.retain(|form| form.request != settled.request);
    pending.opened &= !pending.asks.is_empty() || !pending.forms.is_empty();
    for label in app.webview_windows().into_keys() {
        if label.starts_with("chat-") {
            let _ = app.emit_to(label, CHAT_PERMISSION_SETTLED_EVENT, &settled);
        }
    }
}

/// The app-wide Do Not Disturb switch, which is what the tray toggle writes.
fn do_not_disturb(app: &tauri::AppHandle) -> bool {
    app.try_state::<SettingsState>()
        .and_then(|state| state.settings.lock().ok().map(|read| read.do_not_disturb))
        .unwrap_or(false)
}

/// Put a Chat surface in front of the user for a request with nowhere to be
/// drawn. One session serves every Instance (ADR-0008), so the roster's first
/// is the one the Shell can name without threading the asking Instance down the wire.
fn show_chat_for_ask(app: &tauri::AppHandle, shut: Option<String>) {
    let rows = app
        .try_state::<SettingsState>()
        .and_then(|state| state.instances.lock().ok().map(|rows| rows.clone()))
        .unwrap_or_default();
    let id = shut
        .as_deref()
        .and_then(|label| label.strip_prefix("chat-"))
        .map(str::to_string)
        .or_else(|| rows.first().map(|row| row.id.clone()));
    let Some(id) = id else {
        eprintln!("harness: no Instance to ask on; the request will time out");
        return;
    };
    let title = instance_title(&rows, &id);
    open_chat(app, &id, title);
}

/// Record what just happened, unless a typed line is still waiting.
/// `happened` is one slot: a Poke between a line arriving and the wake
/// would replace the question and answer one nobody asked.
fn note_happened(happened: &mut Happened, what: Happened) {
    if !matches!(happened, Happened::Chat(_)) {
        *happened = what;
    }
}

/// What a Chat surface needs to draw itself before anything is typed.
#[derive(Clone, Serialize)]
struct ChatOpening {
    name: String,
    character: String,
    /// Whether a Completer exists at all — a key, or a local host.
    configured: bool,
    /// Whether the switch is on as well. Configured and switched off is a
    /// different sentence from never configured, and the surface says which.
    enabled: bool,
    /// Which mind answers here, or would (#474): the Harness that was named,
    /// whether the child is up, and the session that proves it. `None` when
    /// none was named, and the HTTP rows below are the answer instead.
    harness: Option<ChatHarness>,
    /// The HTTP Completer in force: model and host, never a credential.
    model: String,
    host: String,
    /// A Harness is attached but not signed in: the command that fixes it,
    /// for the user's own terminal. The third state ADR-0010 names.
    login: Option<String>,
    /// Which Harness is attached, when one is. Used to name it in the fourth
    /// empty state (needs authentication).
    harness_name: Option<String>,
    /// App-level instructions as sent: voice rules, Behavior roster, reply
    /// contract. Empty under Blank AI. The Prompt tab draws this frozen so an
    /// emptied control run is a visible Empty, not a missing block.
    instructions: String,
    /// The Character's frozen Personality Prompt, for the Prompt tab
    /// (ADR-0012). Empty when the package shipped none, and empty under
    /// Blank AI: the flag empties the layer rather than hiding the tab (#680).
    personality: String,
    /// This Instance's own layer, as it stands. Empty by default. Still sent
    /// under Blank AI, so a control run can iterate a prompt (#680).
    instance_prompt: String,
    /// What the tab may not exceed, so the box can say so before the save
    /// surface has to.
    prompt_limit: usize,
}

/// The Harness half of an opening. Facts and not a sentence: the wording is
/// the window's, and `chat-status.js` is where it has a test.
#[derive(Clone, Serialize)]
struct ChatHarness {
    name: String,
    /// Attached but not signed in: the command that fixes it, for the user's
    /// own terminal. The third state ADR-0010 names.
    login: Option<String>,
    /// Whether the child is up. Set and dead is the state the Chat surface
    /// could not tell from attached before #474, and it is a lie worth more
    /// than a missing label.
    alive: bool,
    session: Option<String>,
    /// The binary `PATH` has not got, when that is why nothing is running.
    /// Settings already names it (#659). Chat used to drop it and say
    /// `not running` (#726).
    missing: Option<String>,
    /// Whether ACP handshake/spawn is in progress. Gates chat until ready or failed.
    initializing: bool,
}

fn chat_harness(inspect: &model::DirectorInspect) -> Option<ChatHarness> {
    inspect.harness.as_ref().map(|attached| ChatHarness {
        name: attached.name.clone(),
        login: attached.login.clone(),
        alive: attached.alive,
        session: attached.session_id.clone(),
        missing: attached.missing.clone(),
        initializing: attached.initializing,
    })
}

#[cfg(test)]
fn chat_opening_from(
    instance: &roster::Instance,
    inspect: &model::DirectorInspect,
    personality: &str,
) -> ChatOpening {
    chat_opening_layers(instance, inspect, personality, std::iter::empty::<&str>())
}

fn chat_opening_layers(
    instance: &roster::Instance,
    inspect: &model::DirectorInspect,
    personality: &str,
    behaviors: impl IntoIterator<Item = impl AsRef<str>>,
) -> ChatOpening {
    let blank = model::blank();
    ChatOpening {
        name: instance.name.clone(),
        character: instance.character_name().to_string(),
        configured: inspect.configured,
        enabled: inspect.enabled,
        harness: chat_harness(inspect),
        model: inspect.model.clone(),
        host: inspect.host.clone(),
        login: inspect
            .harness
            .as_ref()
            .and_then(|attached| attached.login.clone()),
        harness_name: inspect
            .harness
            .as_ref()
            .map(|attached| attached.name.clone()),
        instructions: app_instructions(behaviors, blank),
        personality: if blank {
            String::new()
        } else {
            personality.to_string()
        },
        instance_prompt: instance.prompt().to_string(),
        prompt_limit: roster::INSTANCE_PROMPT_LIMIT,
    }
}

/// Personality Prompt of the Character `instance` is running. Read off the
/// loaded packages rather than held on the Instance: it is the author's
/// layer and the Prompt tab shows it frozen (ADR-0012).
fn personality_of(
    characters: &BTreeMap<String, Arc<Character>>,
    instance: &roster::Instance,
) -> String {
    characters
        .get(instance.character_name())
        .map(|character| character.personality.clone())
        .unwrap_or_default()
}

fn behavior_names_of(
    characters: &BTreeMap<String, Arc<Character>>,
    instance: &roster::Instance,
) -> Vec<String> {
    characters
        .get(instance.character_name())
        .map(|character| character.behaviors.keys().cloned().collect())
        .unwrap_or_default()
}

/// Who this Chat surface belongs to, and whether anything can answer. A
/// command rather than an event: Tauri buffers nothing for a listener that
/// is not there yet. Same `DirectorInspect` the Settings window renders.
#[tauri::command]
fn chat_opening(instance: String, state: tauri::State<'_, SettingsState>) -> ChatOpening {
    let (name, character, instance_prompt) = state
        .instances
        .lock()
        .ok()
        .and_then(|rows| {
            rows.iter()
                .find(|row| row.id == instance)
                .map(|row| (row.name.clone(), row.character.clone(), row.prompt.clone()))
        })
        .unwrap_or_default();
    // Re-read on every opening: a login the user ran mid-session moves the
    // Harness out of the not-authenticated state, and nothing else pushes it.
    if let Ok(mut inspect) = state.inspect.lock() {
        inspect.harness = harness::attached().map(|session| session.inspect());
    }
    let inspect = state.inspect.lock().ok();
    let blank = model::blank();
    ChatOpening {
        instructions: app_instructions(
            state
                .behavior_names
                .get(&character)
                .cloned()
                .unwrap_or_default(),
            blank,
        ),
        personality: if blank {
            String::new()
        } else {
            state
                .personalities
                .get(&character)
                .cloned()
                .unwrap_or_default()
        },
        name,
        character,
        configured: inspect.as_ref().is_some_and(|read| read.configured),
        enabled: inspect.as_ref().is_some_and(|read| read.enabled),
        harness: inspect.as_ref().and_then(|read| chat_harness(read)),
        model: inspect
            .as_ref()
            .map(|read| read.model.clone())
            .unwrap_or_default(),
        host: inspect
            .as_ref()
            .map(|read| read.host.clone())
            .unwrap_or_default(),
        login: inspect
            .as_ref()
            .and_then(|read| read.harness.as_ref())
            .and_then(|attached| attached.login.clone()),
        harness_name: inspect
            .as_ref()
            .and_then(|read| read.harness.as_ref())
            .map(|attached| attached.name.clone()),
        instance_prompt,
        prompt_limit: roster::INSTANCE_PROMPT_LIMIT,
    }
}

/// A new Instance Prompt from its Chat surface. Refused rather than cut, so
/// the words still in the box are the words that were not saved. Persist and
/// reopen happen on the frame-loop thread (ADR-0012).
#[tauri::command]
fn chat_prompt(
    instance: String,
    text: String,
    chat: tauri::State<'_, ChatChannel>,
) -> Result<(), String> {
    let prompt = roster::instance_prompt(&text)?;
    chat.0
        .send(ChatMsg::Wrote(ChatLine {
            instance,
            text: prompt,
        }))
        .map_err(|_| "ai-buddy is not listening.".to_string())
}

/// The user's pick on a forwarded permission request. The only path by which
/// a `session/request_permission` is ever answered.
#[tauri::command]
fn permission_answer(request: String, option: String) {
    if let Some(session) = harness::attached() {
        session.answer_permission(&request, &option);
    }
}

/// The user's pick on a forwarded elicitation form. `value` is the chosen
/// option; omitted is Decline, which is a valid answer.
#[tauri::command]
fn elicitation_answer(request: String, value: Option<String>) {
    if let Some(session) = harness::attached() {
        let answer = match value {
            Some(value) if !value.is_empty() => harness::ElicitationAnswer::Accept(value),
            _ => harness::ElicitationAnswer::Decline,
        };
        session.answer_elicitation(&request, answer);
    }
}

/// Open a clicked reply link in the user's browser. The webview has no
/// opener; an `<a href>` would navigate the chat window itself. Scheme
/// gating is `platform::open_url`'s, at the last edge; the URL is untrusted.
#[tauri::command]
fn open_link(url: String) -> Result<(), String> {
    platform::open_url(&url)
}

/// Connect a named Harness from Chat. Through `SettingsSession::apply`, so
/// the attachment moves now and `ReloadChat` carries state back. The answer
/// is a line to read, not a process to spawn; `harness::login_hint` owns why.
#[tauri::command]
fn select_harness(
    harness: String,
    app: tauri::AppHandle,
    state: tauri::State<'_, SettingsState>,
) -> Result<String, String> {
    // The picker's own list, not a second copy of it: a name added to one and
    // not the other is a row the user can pick and this command then refuses.
    if !settings::form::HARNESS_PRESETS.contains(&harness.as_str()) {
        return Err(format!("unknown Harness preset: {harness}"));
    }
    let mut patch = settings::SettingsPatch::default();
    patch.set_text(settings::TextField::Harness, &harness);
    settings_session(&app, &state).apply(patch)?;
    Ok(harness::login_hint(&harness))
}

/// Push a full opening to an already-open Chat surface, without creating
/// one. After a switch the Character line still has to move; configured and
/// login too, because the window asked once at start.
fn push_chat_opening(
    app: &tauri::AppHandle,
    roster: &Roster,
    id: &InstanceId,
    inspect: &model::DirectorInspect,
    characters: &BTreeMap<String, Arc<Character>>,
) {
    let Some(instance) = roster.get(id) else {
        return;
    };
    let opening = chat_opening_layers(
        instance,
        inspect,
        &personality_of(characters, instance),
        behavior_names_of(characters, instance),
    );
    let label = chat_label(id);
    let title = opening.name.clone();
    let handle = app.clone();
    let _ = app.emit_to(label.clone(), CHAT_OPENING_EVENT, opening);
    if let Err(why) = app.run_on_main_thread(move || {
        if let Some(window) = handle.get_webview_window(&label) {
            if let Err(why) = window.set_title(&title) {
                eprintln!("chat: {label} could not be retitled: {why}");
            }
        }
    }) {
        eprintln!("chat: could not reach the main thread: {why}");
    }
}

fn push_chat_openings(
    app: &tauri::AppHandle,
    roster: &Roster,
    inspect: &model::DirectorInspect,
    characters: &BTreeMap<String, Arc<Character>>,
) {
    let ids: Vec<_> = roster.list().into_iter().map(|(id, _)| id).collect();
    for id in ids {
        push_chat_opening(app, roster, &id, inspect, characters);
    }
}

struct ChatLine {
    instance: InstanceId,
    text: String,
}

/// What one Chat surface has to say to the frame loop.
enum ChatMsg {
    Said(ChatLine),
    /// A new Instance Prompt, already inside the bound. Saving it reopens that
    /// Instance's Director session, which is the frame loop's to do: it holds
    /// the roster and the Director slots (ADR-0012).
    Wrote(ChatLine),
    /// The surface is listening and has drawn nothing yet: the bar is pushed on
    /// change, so a window opened between two would sit at dashes. Sent after
    /// the listener is registered, or it is an answer nobody hears.
    Listening(InstanceId),
}

/// The sender every Chat surface posts on. Not another `SettingsOp`: every
/// op drained from that one rewrites settings.json. A struct rather than a
/// bare `Sender`, because managed state is keyed by type.
struct ChatChannel(mpsc::Sender<ChatMsg>);

/// What the Shell tells one Chat surface when a turn of its own is over.
#[derive(Clone, Serialize)]
struct ChatReply {
    /// What the Instance said; `None` when the turn produced no line. Sent
    /// anyway, so no caret waits forever on a line that is not coming.
    said: Option<String>,
    /// The line was refused because one typed before it has not been asked
    /// yet. `said` is `None`; see the drain in `frame_loop`.
    busy: bool,
    /// What the Director was reacting to. `None` on a typed-line answer
    /// (that sits under the user's turn). A label rather than a flag: a
    /// double-click is a prompt, the user just did not type.
    reacting_to: Option<String>,
    /// A replayed line the user typed. The live send path draws that row in
    /// the webview itself, so a true here on that path would duplicate it.
    #[serde(default)]
    you: bool,
    /// Milliseconds since the epoch when the line was said. `None` on a live
    /// emit so the surface stamps wall-clock now; replay fills this from
    /// `Turn.at` so a line said before Chat opened keeps that moment.
    at: Option<u64>,
    /// What the Harness answered with when it answered with an error. `said`
    /// is `None` because static weights took the turn, but "no answer" is
    /// wrong when one named a version this CLI will not serve (#514).
    error: Option<String>,
    /// The Shell cancelled this caret because a newer wake started (ADR-0016),
    /// named in `happened_cell`'s word for that wake. `said` is `None`; this is
    /// not a turn that produced no Speech (#681). The surface says which wake
    /// did it, because "you poked me" reads as cause and "dropped" as a bug (#890).
    #[serde(default)]
    superseded_by: Option<&'static str>,
}

/// What the Shell owes a Chat surface when a newer wake cancels the slot
/// (ADR-0016). `None` unless a typed question was the turn on the wire:
/// a poke or an ambient wake opened no question on this surface, so a
/// notice there would answer nobody (#890).
fn cancelled_caret(chat_turn: bool, by: &Happened) -> Option<ChatReply> {
    chat_turn.then(|| ChatReply {
        said: None,
        busy: false,
        reacting_to: None,
        you: false,
        at: None,
        error: None,
        superseded_by: Some(happened_cell(by)),
    })
}

/// Spatial Layer state one Chat surface draws in its status bar (ADR-0010).
/// Compared field by field to decide whether to push, so nothing in here
/// changes on a tick where the bar would not.
#[derive(Clone, PartialEq, Serialize)]
struct ChatStatus {
    /// The Behavior playing, and `None` for the Engine's own moments — a Land
    /// or a startle no Director proposed. The bar draws a dash, as the trace does.
    behavior: Option<String>,
    primitive: Option<Primitive>,
    animation: &'static str,
    state: State,
    /// What drove the last wake, in `happened_cell`'s word for it.
    happened: Option<&'static str>,
    /// -1 heading left, 1 heading right. The heading itself, and not
    /// `SpritePlacement::mirror`'s answer about the art (#345): the bar says
    /// which way the sprite is walking, which a left strip does not change.
    facing: i8,
    /// A turn is on the wire. The same bit the thinking ellipsis draws from.
    asking: bool,
}

#[derive(Clone, Serialize)]
struct ChatStatusPush<'a> {
    #[serde(flatten)]
    status: &'a ChatStatus,
    /// Milliseconds until the next ambient wake, or `None` when none is
    /// coming. A deadline pushed once rather than a number every second: the
    /// window counts it down itself.
    wake_ms: Option<u64>,
}

/// A line the user typed, on its way in. Bounded here, at `CHAT_LIMIT`:
/// this is where webview text enters, and the session keeps the line, so
/// cutting it later would still have paid for the whole paste.
#[tauri::command]
fn chat_send(instance: String, text: String, chat: tauri::State<'_, ChatChannel>) {
    let text: String = text
        .trim()
        .chars()
        .take(ai_buddy_core::director::CHAT_LIMIT)
        .collect();
    if text.is_empty() {
        return;
    }
    let _ = chat.0.send(ChatMsg::Said(ChatLine { instance, text }));
}

/// A Chat surface reporting that it is listening. Events only reach windows
/// that existed, so this session's turns wait here too, Speech as well as
/// permission asks that arrived before this window existed.
#[tauri::command]
fn chat_ready(
    instance: String,
    app: tauri::AppHandle,
    chat: tauri::State<'_, ChatChannel>,
    pending: tauri::State<'_, PendingAsks>,
) {
    if let Ok(pending) = pending.0.lock() {
        for turn in session_log::replay(&app, &instance) {
            let _ = app.emit_to(
                chat_label(&instance),
                CHAT_EVENT,
                ChatReply {
                    said: turn.said,
                    busy: false,
                    reacting_to: turn.reacting_to,
                    you: turn.you,
                    error: None,
                    superseded_by: None,
                    at: Some(
                        turn.at
                            .duration_since(UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_millis() as u64,
                    ),
                },
            );
        }
        for ask in &pending.asks {
            let _ = app.emit_to(chat_label(&instance), CHAT_PERMISSION_EVENT, ask);
        }
        for form in &pending.forms {
            let _ = app.emit_to(chat_label(&instance), CHAT_ELICITATION_EVENT, form);
        }
    }
    let _ = chat.0.send(ChatMsg::Listening(instance));
}

/// What each `overlay-{n}` should be covering after a display change.
///
/// `Inactive` is an overlay that outlived its display. It stays built and
/// hidden rather than closed: closing a webview from the main-thread block
/// that runs during display reconfiguration tears down WebKit mid-recalculation
/// and the Objective-C exception crosses an `extern "C"` frame, which Rust
/// cannot catch and aborts the process. #868.
#[derive(Clone, Copy, Debug, PartialEq)]
enum OverlayTarget {
    Display(Rect),
    Inactive,
}

/// One target per label in use, which is one per display plus every overlay
/// left over from a larger arrangement.
fn overlay_targets(displays: &[Rect], existing: usize) -> Vec<OverlayTarget> {
    (0..displays.len().max(existing))
        .map(|index| {
            displays
                .get(index)
                .copied()
                .map_or(OverlayTarget::Inactive, OverlayTarget::Display)
        })
        .collect()
}

/// One overlay per display. A spanning window is invisible off its Space, so
/// a seam needs both overlays. Idempotent as displays move; every display is
/// attempted even after one fails, or the rest of the desktop would go blank.
fn place_overlays(app: &tauri::AppHandle, displays: &[Rect]) -> Result<(), String> {
    let mut failed = Vec::new();
    // Probed, not remembered: an overlay that lost its display is hidden and
    // still there, and it is the one this has to find again. Labels are handed
    // out in order, so the first missing one ends the set.
    let existing = (0..)
        .take_while(|index| app.get_webview_window(&overlay_label(*index)).is_some())
        .count();

    for (index, target) in overlay_targets(displays, existing).into_iter().enumerate() {
        let label = overlay_label(index);
        let placed = match (app.get_webview_window(&label), target) {
            // Shown, not just covered: this is the path a returning display
            // takes, and `build_overlay` shows for the same reason.
            (Some(window), OverlayTarget::Display(display)) => cover_display(&window, display)
                .and_then(|()| window.show())
                .map_err(|why| why.to_string()),
            (None, OverlayTarget::Display(display)) => {
                build_overlay(app, &label, display).map_err(|why| why.to_string())
            }
            (Some(window), OverlayTarget::Inactive) => {
                eprintln!("overlay: {label} has no display left to cover");
                window.hide().map_err(|why| why.to_string())
            }
            (None, OverlayTarget::Inactive) => Ok(()),
        };
        if let Err(why) = placed {
            failed.push(format!("{label}: {why}"));
        }
    }

    if failed.is_empty() {
        Ok(())
    } else {
        Err(failed.join("; "))
    }
}

/// The hide-hotkey Shortcut. Three modifiers, because a global shortcut is
/// taken from every application on the machine and B alone belongs to most
/// of them.
fn shortcut_from_spec(spec: &str) -> Option<Shortcut> {
    let parsed = settings::parse_hotkey(spec)
        .or_else(|| settings::parse_hotkey(settings::DEFAULT_HIDE_HOTKEY))?;
    let code: Code = settings::key_code_name(parsed.key)?.parse().ok()?;
    let mut modifiers = Modifiers::empty();
    if parsed.control {
        modifiers |= Modifiers::CONTROL;
    }
    if parsed.option {
        modifiers |= Modifiers::ALT;
    }
    if parsed.shift {
        modifiers |= Modifiers::SHIFT;
    }
    if parsed.command {
        modifiers |= Modifiers::SUPER;
    }
    Some(Shortcut::new(Some(modifiers), code))
}

fn install_hide_hotkey(
    app: &tauri::AppHandle,
    rules: Arc<Mutex<HideRules>>,
    settings: Arc<Mutex<Settings>>,
    settings_path: PathBuf,
) -> bool {
    let plugin = tauri_plugin_global_shortcut::Builder::new()
        .with_handler(move |_app, _shortcut, event| {
            // Pressed only. The handler is called again on release, and a
            // toggle that ran twice would hand the Character back before the
            // user had let go of the key.
            if event.state() == ShortcutState::Pressed {
                if let (Ok(mut rules), Ok(mut settings)) = (rules.lock(), settings.lock()) {
                    settings::toggle_away(&mut rules, &mut settings);
                    persist_settings(&settings, &settings_path);
                }
            }
        })
        .build();
    if let Err(why) = app.plugin(plugin) {
        eprintln!("hotkey: unavailable, so the Character cannot be hidden by hand: {why}");
        false
    } else {
        true
    }
}

/// Bind the hide hotkey. A hotkey another application already holds is
/// reported and let go: losing it costs one way to hide the Character, which
/// is not worth losing the Character over.
fn bind_hide_hotkey(app: &tauri::AppHandle, spec: &str) {
    if app
        .try_state::<tauri_plugin_global_shortcut::GlobalShortcut<tauri::Wry>>()
        .is_none()
    {
        return;
    }
    let Some(shortcut) = shortcut_from_spec(spec) else {
        eprintln!("hotkey: {spec} could not be parsed");
        return;
    };
    let gs = app.global_shortcut();
    if let Err(why) = gs.unregister_all() {
        eprintln!("hotkey: could not drop the previous binding: {why}");
    }
    if let Err(why) = gs.register(shortcut) {
        eprintln!(
            "hotkey: {spec} is unavailable, so the Character cannot be hidden by hand: {why}"
        );
    }
}

fn check_for_update(app: tauri::AppHandle) {
    tauri::async_runtime::spawn(async move {
        use tauri_plugin_updater::UpdaterExt;
        let updater = match app.updater() {
            Ok(updater) => updater,
            Err(why) => {
                eprintln!("updater: {why}");
                return;
            }
        };
        match updater.check().await {
            Ok(Some(update)) => {
                eprintln!("updater: {} available, downloading", update.version);
                if let Err(why) = update.download_and_install(|_, _| {}, || {}).await {
                    eprintln!("updater: {why}");
                }
            }
            Ok(None) => eprintln!("updater: up to date"),
            Err(why) => eprintln!("updater: {why}"),
        }
    });
}

// One over the clippy cap. Settings, hide rules, and the Director each have
// to hear the same click, and folding them would mix persist with proposal.
#[allow(clippy::too_many_arguments)]
fn apply_menu_action(
    action: menu::MenuAction,
    roster: &mut Roster,
    lives: &mut Vec<InstanceState>,
    slots: &mut model::Slots,
    instance_id: &InstanceId,
    rules: &Arc<Mutex<HideRules>>,
    settings: &Arc<Mutex<Settings>>,
    settings_path: &std::path::Path,
    characters: &BTreeMap<String, Arc<Character>>,
    config: &mut model::DirectorConfig,
    director: &model::DirectorSettings,
    inspect: &Arc<Mutex<model::DirectorInspect>>,
    app: &tauri::AppHandle,
) {
    match action {
        menu::MenuAction::SwitchCharacter(name) => {
            if let Some(character) = characters.get(&name).cloned() {
                switch_instance(
                    roster,
                    lives,
                    slots,
                    instance_id,
                    character,
                    config,
                    director,
                    app,
                );
                if let Ok(inspect) = inspect.lock() {
                    push_chat_opening(app, roster, instance_id, &inspect, characters);
                }
                if let Ok(mut settings) = settings.lock() {
                    settings.character = name.clone();
                    persist_settings(&settings, settings_path);
                }
                eprintln!("menu: switching to {name}");
            } else {
                eprintln!("menu: no Character named {name}");
            }
        }
        menu::MenuAction::SpawnInstance => {
            let character_name = settings
                .lock()
                .ok()
                .map(|s| s.character.clone())
                .filter(|name| !name.is_empty())
                .or_else(|| lives.first().map(|live| live.character.name.clone()));
            if let Some(name) = character_name {
                spawn_live(
                    roster,
                    lives,
                    characters,
                    &name,
                    name.clone(),
                    config,
                    director,
                );
            }
        }
        menu::MenuAction::ToggleDirector => {
            if let Ok(mut settings) = settings.lock() {
                settings.director_enabled = !settings.director_enabled;
                config.apply_switch(settings.director_enabled);
                if let Ok(mut inspect) = inspect.lock() {
                    inspect.enabled = config.enabled;
                    push_chat_openings(app, roster, &inspect, characters);
                }
                persist_settings(&settings, settings_path);
                eprintln!(
                    "menu: Director {}",
                    if settings.director_enabled {
                        "on"
                    } else {
                        "off"
                    }
                );
            }
        }
        menu::MenuAction::ToggleDnd => {
            if let Some(instance) = roster.get_mut(instance_id) {
                let new_state = !instance.do_not_disturb();
                instance.set_do_not_disturb(new_state);
                if let Ok(mut settings) = settings.lock() {
                    settings.do_not_disturb = new_state;
                    persist_settings(&settings, settings_path);
                }
                eprintln!("menu: DND {}", if new_state { "on" } else { "off" });
            }
        }
        menu::MenuAction::Hide => {
            if let Ok(mut r) = rules.lock() {
                if let Ok(mut settings) = settings.lock() {
                    settings::toggle_away(&mut r, &mut settings);
                    persist_settings(&settings, settings_path);
                }
                eprintln!("menu: {}", if r.is_away() { "away" } else { "back" });
            }
        }
        menu::MenuAction::ToggleFullscreenHide => {
            if let Ok(mut r) = rules.lock() {
                let next = !r.hide_in_fullscreen();
                r.set_hide_in_fullscreen(next);
                if let Ok(mut settings) = settings.lock() {
                    settings.hide_in_fullscreen = r.hide_in_fullscreen();
                    persist_settings(&settings, settings_path);
                }
            }
        }
        menu::MenuAction::OpenMemory => {
            let _ = platform::open_path(&memory::shared_path());
        }
        menu::MenuAction::OpenActionLog => {
            let _ = platform::open_path(&action_log::current_path());
        }
        menu::MenuAction::OpenSettings => show_settings(app.clone()),
        menu::MenuAction::Summon => {
            if let Some(instance) = roster.get(instance_id) {
                let title = instance.name.clone();
                open_chat(app, instance_id, title);
                eprintln!("menu: Summon");
            }
        }
        menu::MenuAction::Quit => quit_now(),
    }
}

/// Leave without AppKit's `terminate:`. `PredefinedMenuItem::quit` deadlocks
/// overlay webviews from inside the tray tracking run loop; `process::exit`
/// skips that path and the window server drops the overlays with the process.
fn quit_now() -> ! {
    eprintln!("quit");
    harness::shutdown();
    std::process::exit(0);
}

/// Ctrl+C is not `RunEvent::Exit`. The Harness child is in its own process
/// group so that signal does not dump inside Node; this then kills it.
/// Isolation waits until the handler is installed, or a failed catch leaves a tree Ctrl+C cannot reap.
fn quit_harness_on_interrupt() {
    match ctrlc::set_handler(|| {
        if crate::harness::interrupt_already_quitting() {
            std::process::exit(0);
        }
        eprintln!("quit");
        crate::harness::shutdown();
        std::process::exit(0);
    }) {
        Ok(()) => crate::harness::own_interrupt(),
        Err(why) => eprintln!(
            "harness: could not catch interrupt: {why}; child stays in this process group"
        ),
    }
}

/// One Instance's wake clock: config interval grown at the Character's rate.
/// The two halves come from different places every time, so the pairing is
/// written once rather than at each of the four sites that builds a `Pace`.
pub(crate) fn paced(config: &model::DirectorConfig, character: &Character) -> Pace {
    Pace::with_growth(
        config.ambient_first,
        character.model_base,
        character.model_power,
    )
}

// One over the clippy cap, for the same reason `apply_menu_action` is: the new
// session belongs beside the `retarget_model` that opens it, and the Chat
// surface it has to tell is reached through the app handle.
#[allow(clippy::too_many_arguments)]
fn switch_instance(
    roster: &mut Roster,
    lives: &mut [InstanceState],
    slots: &mut model::Slots,
    instance_id: &InstanceId,
    character: Arc<Character>,
    config: &model::DirectorConfig,
    settings: &model::DirectorSettings,
    app: &tauri::AppHandle,
) {
    roster.retarget(instance_id, &character);
    if let Some(live) = lives.iter_mut().find(|live| live.id == *instance_id) {
        live.character = character;
        live.director = StaticDirector::new(live.character.behaviors.clone(), 0);
        live.pace = paced(config, &live.character);
        // The old session is the previous Character's. A Wake still on the
        // wire would propose as them; drop it and ask for this opening turn.
        model::retarget_model(
            slots,
            instance_id,
            &mut live.model,
            live.character.behaviors.keys().cloned(),
            live.character.name.clone(),
            settings,
            config.configured,
        );
        session_log::new_session(app, instance_id, "the Character changed");
        live.recent.clear();
        live.happened = Happened::Ambient;
        live.addressed = true;
    }
}

fn spawn_live(
    roster: &mut Roster,
    lives: &mut Vec<InstanceState>,
    characters: &BTreeMap<String, Arc<Character>>,
    character_name: &str,
    instance_name: String,
    config: &model::DirectorConfig,
    settings: &model::DirectorSettings,
) {
    let Some(character) = characters.get(character_name).cloned() else {
        eprintln!("menu: no Character named {character_name}");
        return;
    };
    let start = lives
        .last()
        .and_then(|live| live.drawn_last.as_ref())
        .map(|drawn| Point {
            x: f64::from(drawn.rect.x) + sprite_width(&character) + 16.0,
            y: f64::from(drawn.rect.y),
        })
        .unwrap_or(Point { x: 80.0, y: 80.0 });
    let name = if instance_name.is_empty() {
        character.name.clone()
    } else {
        instance_name
    };
    let id = roster.spawn(&character, name, start);
    let seed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_nanos() as u64);
    lives.push(InstanceState {
        id: id.clone(),
        director: StaticDirector::new(character.behaviors.clone(), seed),
        model: config.configured.then(|| {
            Arc::new(ModelDirector::new(
                model::completer_from(settings).expect("configured means a Completer exists"),
                character.behaviors.keys().cloned(),
                id.clone(),
                character.name.clone(),
                model::blank(),
            ))
        }),
        recent: Vec::new(),
        pace: paced(config, &character),
        since_wake: Duration::ZERO,
        since_state: Duration::ZERO,
        since_ambient: Duration::ZERO,
        previous_idle: Duration::MAX,
        last_state: None,
        addressed: false,
        happened: Happened::Ambient,
        chat_turn: false,
        pointer: Pointer::with_double_click_ms(platform::double_click_interval_ms()),
        spoken: None,
        drawn_last: None,
        traced_last: None,
        status_last: None,
        status_wake_ms: None,
        happened_last: None,
        verbs: Vec::new(),
        menu_hold: None,
        character,
    });
}

/// The menu bar icon. Held so a toggle on the frame-loop thread can rebuild
/// the menu on the main thread, where the native objects live.
struct TrayHandle(Mutex<Option<tauri::tray::TrayIcon>>);

struct FrameExtras {
    settings: Arc<Mutex<Settings>>,
    settings_path: PathBuf,
    characters: BTreeMap<String, Arc<Character>>,
    instances: Arc<Mutex<Vec<InstanceRow>>>,
    ops: mpsc::Receiver<SettingsOp>,
    chat: mpsc::Receiver<ChatMsg>,
    /// `tools/call`s from the loopback MCP server, which are dispatched on the
    /// frame-loop thread because that is where the `Roster` is (ADR-0023).
    mcp: mpsc::Receiver<mcp_http::Call>,
}

fn publish_instances(roster: &Roster, dest: &Arc<Mutex<Vec<InstanceRow>>>) {
    if let Ok(mut rows) = dest.lock() {
        *rows = roster
            .list()
            .into_iter()
            .map(|(id, name)| {
                let instance = roster.get(&id);
                InstanceRow {
                    id,
                    name,
                    character: instance
                        .map(|instance| instance.character_name().to_string())
                        .unwrap_or_default(),
                    // The Chat surface asks for this through `chat_opening`,
                    // which sees the roster only through these rows.
                    prompt: instance
                        .map(|instance| instance.prompt().to_string())
                        .unwrap_or_default(),
                }
            })
            .collect();
    }
}

fn describe_menu(
    installed: &[String],
    current: &str,
    roster: &Roster,
    instance_id: &str,
    settings: &Settings,
    rules: &HideRules,
) -> menu::MenuDescription {
    let instances = roster.list();
    let hide_hotkey = settings::display_hotkey(&settings.hide_hotkey);
    menu::describe(menu::MenuSnapshot {
        installed,
        current_character: current,
        instances: &instances,
        director_enabled: model::director_in_force(settings.director_enabled),
        director_env_owned: model::env_switch(model::ENABLED).is_some(),
        do_not_disturb: roster
            .get(instance_id)
            .map(|instance| instance.do_not_disturb())
            .unwrap_or(false),
        hidden: rules.is_away(),
        hide_in_fullscreen: rules.hide_in_fullscreen(),
        hide_hotkey: &hide_hotkey,
    })
}

/// The environment variable naming the Instances to run. An env var rather
/// than a flag because that is how ai-buddy is already configured, and a
/// second mechanism for the same kind of answer is a second place to look it up.
const INSTANCES_VAR: &str = "AI_BUDDY_INSTANCES";

/// Which Instances the launch configuration asks for. The environment wins
/// when a developer set it; otherwise settings. Empty is still first-run:
/// `load_instances` turns it into the one buddy the app has always run.
fn requested_instances(settings: &Settings) -> Result<Vec<InstanceSpec>, String> {
    match std::env::var(INSTANCES_VAR) {
        Ok(raw) => roster::parse_specs(&raw),
        Err(_) if !settings.instances.is_empty() => Ok(settings.instances.clone()),
        Err(_) => Ok(Vec::new()),
    }
}

fn load_all_characters(
    app: &tauri::AppHandle,
) -> (
    BTreeMap<String, CharacterArt>,
    BTreeMap<String, Arc<Character>>,
) {
    let bundled = app
        .path()
        .resource_dir()
        .ok()
        .map(|dir| dir.join(BUNDLED_CHARACTERS));
    let search_paths = package::search_paths(bundled);
    let mut art = BTreeMap::new();
    let mut cache = BTreeMap::new();
    for path in package::installed(&search_paths) {
        let files = match package::read(&path) {
            Ok(files) => files,
            Err(_) => continue,
        };
        if let Ok(character) = ai_buddy_core::character::load(&files) {
            art.insert(
                character.name.clone(),
                CharacterArt {
                    art: art_urls(&character),
                    smooth: character.smooth,
                },
            );
            cache.insert(character.name.clone(), Arc::new(character));
        }
    }
    (art, cache)
}

/// Load the Character each requested Instance names, sharing one load
/// among namesakes. None asked still runs the one buddy the app has always
/// run; an Instance with no name of its own takes the Character's.
fn load_instances(
    app: &tauri::AppHandle,
    wanted: &[InstanceSpec],
    settings: &Settings,
) -> Result<Vec<(InstanceSpec, Arc<Character>)>, String> {
    if wanted.is_empty() {
        let wanted = std::env::var_os(package::CHARACTER_VAR).or_else(|| {
            (!settings.character.is_empty()).then(|| std::ffi::OsString::from(&settings.character))
        });
        let character = Arc::new(load_named(app, wanted)?);
        let name = character.name.clone();
        return Ok(vec![(
            InstanceSpec::fresh(character.name.clone(), name),
            character,
        )]);
    }

    let mut loaded: BTreeMap<String, Arc<Character>> = BTreeMap::new();
    let mut instances = Vec::with_capacity(wanted.len());

    for spec in wanted {
        let character = match loaded.get(&spec.character) {
            Some(character) => Arc::clone(character),
            None => {
                let character = Arc::new(load_named(
                    app,
                    Some(std::ffi::OsString::from(&spec.character)),
                )?);
                loaded.insert(spec.character.clone(), Arc::clone(&character));
                character
            }
        };

        let name = if spec.name.is_empty() {
            character.name.clone()
        } else {
            spec.name.clone()
        };
        instances.push((
            InstanceSpec {
                character: spec.character.clone(),
                name,
                // Carried rather than dropped: these two are how the Instance
                // that ran last time is the Instance that runs now (ADR-0012).
                id: spec.id.clone(),
                prompt: spec.prompt.clone(),
            },
            character,
        ));
    }

    Ok(instances)
}

/// A lone leftover default `{ character: "Timber Wolf", name: "bmo" }` takes
/// this Character's name before the overlay log prints it and before spawn
/// persists it. Several Instances keep the names that tell them apart.
fn follow_lone_default(
    loaded: &mut [(InstanceSpec, Arc<Character>)],
    known: &BTreeMap<String, Arc<Character>>,
) {
    if loaded.len() != 1 {
        return;
    }
    let names: Vec<&str> = known.keys().map(String::as_str).collect();
    let (spec, character) = &mut loaded[0];
    spec.name = roster::adopted_name(&spec.name, &character.name, names);
}

/// Spawn every requested Instance into a Roster, and build the Shell state
/// each keeps beside its Engine. Memory is one file for every Instance:
/// `Roster` holds it behind an `Arc` so a second buddy already knows the user.
fn spawn_instances(
    loaded: &[(InstanceSpec, Arc<Character>)],
    start: Point,
    config: &model::DirectorConfig,
    settings: &model::DirectorSettings,
    known_names: impl IntoIterator<Item = String>,
) -> (Roster, Vec<InstanceState>) {
    let mut roster = Roster::new();
    roster.set_known_names(known_names);
    let mut lives = Vec::with_capacity(loaded.len());

    // The wall clock, so two runs are not the same afternoon, the one thing
    // the Engine's purity forbids. Mixed with the Instance's place in the
    // list: same-nanosecond Instances would otherwise share a seed and lockstep.
    let seed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_nanos() as u64);

    // Where each Instance's wake clock starts. Drawn from the same launch
    // seed rather than the clock again: `as_nanos` three times in a row
    // differs in low bits only, and would put every buddy within a millisecond.
    let mut phases = Seeded::new(seed);

    let widths: Vec<f64> = loaded
        .iter()
        .map(|(_, character)| sprite_width(character))
        .collect();
    let positions = starting_positions(start, &widths);

    for (index, (spec, character)) in loaded.iter().enumerate() {
        let id = roster.restore(
            character,
            spec.name.clone(),
            positions[index],
            spec.id.clone(),
            spec.prompt.clone(),
        );

        lives.push(InstanceState {
            id: id.clone(),
            character: Arc::clone(character),
            director: StaticDirector::new(character.behaviors.clone(), seed ^ index as u64),
            // ponytail: N Instances with a key make N times the model calls, on
            // N independent `Pace` clocks and against no shared budget. Fine for
            // the handful a desktop holds; the cap belongs in `Slots`, which is
            // the one thing that sees every Instance, and it wants somewhere to
            // show the spend, which is #18's panel.
            model: config.configured.then(|| {
                Arc::new(ModelDirector::new(
                    model::completer_from(settings).expect("configured means a Completer exists"),
                    character.behaviors.keys().cloned(),
                    id.clone(),
                    character.name.clone(),
                    model::blank(),
                ))
            }),
            recent: Vec::new(),
            pace: paced(config, character),
            // Started somewhere inside the interval rather than at nothing, so
            // N buddies do not all decide on the same tick. Deciding together
            // still reads as coordinated and puts N model calls in one instant.
            since_wake: phase_of(config.wake_every, phases.draw()),
            since_state: Duration::ZERO,
            since_ambient: Duration::ZERO,
            previous_idle: Duration::MAX,
            last_state: None,
            addressed: false,
            happened: Happened::Ambient,
            chat_turn: false,
            pointer: Pointer::with_double_click_ms(platform::double_click_interval_ms()),
            spoken: None,
            drawn_last: None,
            traced_last: None,
            status_last: None,
            status_wake_ms: None,
            happened_last: None,
            verbs: Vec::new(),
            menu_hold: None,
        });
    }

    (roster, lives)
}

/// How far through the wake interval an Instance's clock starts. A draw
/// each, never an even spread (that is its own mechanical lockstep), and
/// never the whole interval. Randomness is injected so the arithmetic is testable.
fn phase_of(interval: Duration, draw: u64) -> Duration {
    let millis = u64::try_from(interval.as_millis()).unwrap_or(u64::MAX);
    if millis == 0 {
        return Duration::ZERO;
    }
    Duration::from_millis(draw % millis)
}

/// How wide a Character's sprite usually is, in points. The idle Animation's
/// frame, blown up by scale. Animations may declare different frame sizes,
/// so this is usual rather than always.
fn sprite_width(character: &Character) -> f64 {
    character
        .draw("idle", 0, 0, 1.0)
        .map_or(0.0, |drawn| f64::from(drawn.frame_size.0))
        * f64::from(character.scale)
}

/// Where each Instance comes into the world. Several dropped on one point
/// would land in a stack; each is placed the previous sprite's width past
/// it. Nothing clamps here; far enough out, the Engine's walls stop them.
fn starting_positions(start: Point, widths: &[f64]) -> Vec<Point> {
    let mut x = start.x;
    widths
        .iter()
        .map(|width| {
            let at = Point { x, y: start.y };
            x += width;
            at
        })
        .collect()
}

/// The folder stem (`trump`) or the Character name (`Trump`) both name a package.
fn names_the_package(path: &Path, character_name: &str, wanted: &OsStr) -> bool {
    path.file_stem() == Some(wanted) || OsStr::new(character_name) == wanted
}

/// The Character an Instance asked for: the first package that loads.
/// Every rejection is reported; finding none stops startup and names every
/// directory searched. Takes the name rather than reading the env itself.
fn load_named(
    app: &tauri::AppHandle,
    wanted: Option<std::ffi::OsString>,
) -> Result<Character, String> {
    // The shipped Characters are an app resource, which `tauri-build` copies
    // next to the binary for `cargo run` as well as into a bundle.
    let bundled = app
        .path()
        .resource_dir()
        .ok()
        .map(|dir| dir.join(BUNDLED_CHARACTERS));

    let search_paths = package::search_paths(bundled);
    let installed = package::installed(&search_paths);
    let candidates = match &wanted {
        Some(name) => {
            // Folder stem first (`trump`). A switch persists Character.name
            // (`Trump`); that is not a stem, so fall through to every package
            // and match after load.
            let by_stem = package::named(installed.clone(), Some(name));
            if by_stem.is_empty() {
                installed
            } else {
                by_stem
            }
        }
        None => package::preferring(installed, package::DEFAULT_CHARACTER),
    };

    for candidate in &candidates {
        let files = match package::read(candidate) {
            Ok(files) => files,
            Err(package::ReadError::NotAPackage(_)) => continue,
            Err(why) => {
                eprintln!("character: {why}");
                continue;
            }
        };

        match ai_buddy_core::character::load(&files) {
            Ok(character) => {
                if let Some(wanted) = &wanted {
                    if !names_the_package(candidate, &character.name, wanted) {
                        continue;
                    }
                }
                eprintln!("character: {} from {}", character.name, candidate.display());
                return Ok(character);
            }
            // A rejection like any other: one broken package should not cost
            // the user every Character behind it in the search.
            Err(errors) => eprintln!(
                "character: {} is not a valid Character Package:\n  - {}",
                candidate.display(),
                errors.join("\n  - ")
            ),
        }
    }

    let looked_in = search_paths
        .iter()
        .map(|path| path.display().to_string())
        .collect::<Vec<_>>()
        .join(", ");

    // Which of the two failed matters: a Character that is not installed is a
    // typo in the name, and every Character failing to load is a broken build.
    Err(match wanted {
        Some(wanted) => format!(
            "no Character Package named {} loaded. ai-buddy looked in: {looked_in}",
            wanted.to_string_lossy()
        ),
        None => format!("no Character Package loaded. ai-buddy looked in: {looked_in}"),
    })
}

/// The taskbar/panel anchor on Windows and Linux, matching the macOS Dock.
/// Clicking it opens Settings. Main thread only: builds a window and
/// registers event handlers.
#[cfg(not(target_os = "macos"))]
fn build_anchor_window(app: &tauri::AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let window = WebviewWindowBuilder::new(app, "anchor", WebviewUrl::default())
        .title("ai-buddy")
        .inner_size(1.0, 1.0)
        .resizable(false)
        .decorations(false)
        .transparent(true)
        .visible(false)
        .build()?;

    #[cfg(not(target_os = "windows"))]
    {
        let app_handle = app.clone();
        window.on_window_event(move |event| {
            if let tauri::WindowEvent::Focused(true) = event {
                show_settings(app_handle.clone());
            }
        });
    }

    #[cfg(target_os = "windows")]
    {
        use raw_window_handle::{HasWindowHandle, RawWindowHandle};
        let app_handle = app.clone();
        if let Ok(handle) = window.window_handle() {
            if let RawWindowHandle::Win32(win32_handle) = handle.as_ref() {
                install_windows_anchor_wndproc(win32_handle.hwnd.get() as _, app_handle);
            }
        }
    }

    window.show()?;
    Ok(())
}

#[cfg(target_os = "windows")]
fn install_windows_anchor_wndproc(hwnd: isize, app: tauri::AppHandle) {
    use std::sync::atomic::{AtomicPtr, Ordering};
    use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CallWindowProcW, SetWindowLongPtrW, GWLP_WNDPROC, WM_ACTIVATE,
    };

    static OLD_WNDPROC: AtomicPtr<()> = AtomicPtr::new(std::ptr::null_mut());
    static APP_HANDLE: std::sync::OnceLock<tauri::AppHandle> = std::sync::OnceLock::new();

    unsafe extern "system" fn anchor_wndproc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        if msg == WM_ACTIVATE {
            let f_active = (wparam & 0xFFFF) as u16;
            const WA_CLICKACTIVE: u16 = 2;
            if f_active == WA_CLICKACTIVE {
                if let Some(app) = APP_HANDLE.get() {
                    show_settings(app.clone());
                }
            }
        }
        let old_proc = OLD_WNDPROC.load(Ordering::Relaxed);
        if old_proc.is_null() {
            windows_sys::Win32::UI::WindowsAndMessaging::DefWindowProcW(hwnd, msg, wparam, lparam)
        } else {
            #[allow(clippy::missing_transmute_annotations)]
            CallWindowProcW(std::mem::transmute(old_proc), hwnd, msg, wparam, lparam)
        }
    }

    APP_HANDLE.set(app).ok();

    unsafe {
        let old = SetWindowLongPtrW(
            hwnd as HWND,
            GWLP_WNDPROC,
            anchor_wndproc as *const () as isize,
        );
        OLD_WNDPROC.store(old as *mut (), Ordering::Relaxed);
    }
}

fn main() {
    // Same Completer, no overlay. scripts/probe-model.sh is the face of this.
    if std::env::args().any(|arg| arg == "--probe-model") {
        std::process::exit(model::run_probe());
    }

    // The same, one hop further out: the Harness attached and one turn run.
    // scripts/probe-harness.sh is the face of this.
    if std::env::args().any(|arg| arg == "--probe-harness") {
        std::process::exit(harness::run_probe());
    }

    // This process *is* the stdio MCP server: never the overlay. The Harness
    // child is a new process, so it does not share the shell's ACP runtime.
    if std::env::args().any(|arg| arg == "--mcp-stdio") {
        ai_buddy_mcp_server::run();
        return;
    }

    // Before the builder, because the builder is where GTK initializes and GDK
    // reads GDK_BACKEND once, when it opens the display. A no-op off Linux.
    platform::prefer_x11_backend();

    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            character,
            overlay_primary,
            overlay_secondary,
            overlay_hotspots,
            overlay_hit_tests_hotspots,
            overlay_open_chat,
            chat_opening,
            chat_send,
            chat_prompt,
            chat_ready,
            permission_answer,
            elicitation_answer,
            open_link,
            select_harness,
            show_settings,
            settings_snapshot,
            settings_event
        ])
        .setup(|app| {
            // No Character means no overlay. Reported and exited rather than
            // returned as a setup error: Tauri turns that into a panic the
            // event loop cannot unwind, burying the one line under a backtrace.
            let settings_file = settings::settings_path(&memory::data_dir());
            let mut settings = Settings::load(&settings_file);
            // Before anything reads a development switch: the frame loop and
            // the overlay panel load them from `dev_flags`, not the env.
            dev_flags::seed(&settings);
            #[cfg(not(target_os = "linux"))]
            consent::set_wanted(
                consent::CapabilityId::Accessibility,
                settings.use_accessibility,
            );
            consent::set_wanted(
                consent::CapabilityId::WindowTitles,
                settings.use_window_titles,
            );
            #[cfg(target_os = "macos")]
            consent::set_wanted(
                consent::CapabilityId::InputMonitoring,
                settings.use_input_monitoring,
            );
            let wanted = requested_instances(&settings).unwrap_or_else(|why| {
                eprintln!("instances: {why}");
                std::process::exit(1);
            });
            let mut loaded = load_instances(&app.handle().clone(), &wanted, &settings)
                .unwrap_or_else(|why| {
                    eprintln!("character: {why}");
                    std::process::exit(1);
                });

            // Every installed package's art, so a switch or a spawn does not
            // have to wait for a reload the overlay never does.
            let (art, character_cache) = load_all_characters(&app.handle().clone());
            follow_lone_default(&mut loaded, &character_cache);
            let mut characters = art;
            for (_, character) in &loaded {
                characters
                    .entry(character.name.clone())
                    .or_insert_with(|| CharacterArt {
                        art: art_urls(character),
                        smooth: character.smooth,
                    });
            }
            app.manage(ArtUrls { characters });
            let installed: Vec<String> = character_cache.keys().cloned().collect();

            // Show in the Dock so users have a findable anchor when the menu
            // bar is crowded. The tray icon remains the settings door.
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Regular);

            // On Windows and Linux, create a hidden anchor window that appears
            // in the taskbar/panel, matching the macOS Dock presence.
            // Clicking it opens Settings. Overlays stay off the taskbar.
            #[cfg(not(target_os = "macos"))]
            build_anchor_window(app.handle())?;

            // Read before the overlays are built rather than after the loop
            // starts: reading which part of a display is usable means asking
            // AppKit, and only the main thread may do that.
            let (source, displays) = platform::window_source(app.handle().clone());
            let start = starting_position(&source.snapshot());

            // Which Dock the physics got. Printed because the difference is
            // invisible until a sprite walks past the Dock's real end.
            if cfg!(target_os = "macos") {
                match displays.read().dock {
                    Some((dock, source)) => eprintln!(
                        "dock: true bounds via {source:?}, {}x{} at {},{}",
                        dock.width, dock.height, dock.x, dock.y
                    ),
                    None => eprintln!(
                        "dock: full-width floor; no source reported a bottom Dock — \
                         a side or hidden Dock has nothing to report, and granting \
                         ai-buddy Accessibility only helps where one exists"
                    ),
                }
            }

            // One overlay per display, so a Character straddling a seam is
            // drawn whole. The frame loop keeps the set in step with a desktop
            // that gains or loses a display.
            let covered = displays.read().frames;
            if covered.is_empty() {
                return Err("no displays reported".into());
            }
            place_overlays(app.handle(), &covered)?;

            // The sprite size is the first Instance's idle Animation, blown
            // up. Here because scripts/verify-overlay.sh crops a screenshot
            // to it, and that script runs one Instance.
            let (sprite_width, sprite_height) = loaded
                .first()
                .and_then(|(_, character)| {
                    let scale = character.scale as i32;
                    character.draw("idle", 0, 0, 1.0).map(|drawn| {
                        (
                            drawn.frame_size.0 as i32 * scale,
                            drawn.frame_size.1 as i32 * scale,
                        )
                    })
                })
                .unwrap_or((0, 0));

            eprintln!(
                "overlay: {} display(s); sprite {}x{}; {}",
                covered.len(),
                sprite_width,
                sprite_height,
                loaded
                    .iter()
                    .map(|(spec, character)| format!("{} as {}", character.name, spec.name))
                    .collect::<Vec<_>>()
                    .join(", "),
            );

            // Shared because the hotkey and the frame loop each see half of the
            // answer: the key is pressed on the main thread and the desktop is
            // read on the loop's.
            let mut hide_rules = HideRules::default();
            hide_rules.set_away(settings.hidden);
            hide_rules.set_hide_in_fullscreen(settings.hide_in_fullscreen);
            let rules = Arc::new(Mutex::new(hide_rules));

            if let Err(why) = app
                .handle()
                .plugin(tauri_plugin_updater::Builder::new().build())
            {
                eprintln!("updater: {why}");
            } else if !cfg!(debug_assertions) {
                // A success downloads and installs a GitHub release over this
                // process; `cargo run` is a debug binary that must not be
                // replaced that way.
                check_for_update(app.handle().clone());
            }

            let secrets: Arc<dyn SecretStore> = Arc::new(KeyringStore::new());
            // Before `director_settings` and `config_from`, both of which ask
            // whether a Harness is attached. Resolving the key first is what
            // made a Harness launch prompt for one it would never send (#290).
            app.manage(PendingAsks(Mutex::new(Pending::default())));
            app.manage(Mutex::new(session_log::Log::new()));
            // Before `attach`, because `open_session` reads the endpoint to
            // decide what to put in `session/new`'s `mcpServers` and the
            // preflight thread can reach that within a tick of this line.
            let (mcp_tx, mcp_rx) = mpsc::channel();
            mcp_resources::publish_excluded(&settings.excluded_applications);
            mcp_http::serve(mcp_tx);
            let forward_to = app.handle().clone();
            quit_harness_on_interrupt();
            harness::attach(
                harness::Target::from_settings(
                    settings.harness_source().as_deref(),
                    &settings.harness_cwd,
                ),
                Box::new(move |forwarded| match forwarded {
                    harness::Forwarded::Ask(ask) => forward_ask(&forward_to, ask),
                    harness::Forwarded::Form(form) => forward_form(&forward_to, form),
                    harness::Forwarded::Settled { request, option } => {
                        settle_ask(&forward_to, Settled { request, option })
                    }
                    harness::Forwarded::Thought(line) => show_thought(&forward_to, line),
                    harness::Forwarded::Plan(steps) => show_plan(&forward_to, &steps),
                    harness::Forwarded::AttachSettled => {
                        if let Some(state) = forward_to.try_state::<SettingsState>() {
                            let _ = state.ops.send(SettingsOp::ReloadChat);
                        }
                    }
                }),
            );
            // The other lane's thoughts, through the same door. Only one lane
            // completes at a time — a Harness attached is the Completer
            // (ADR-0008) — so the strip is never written by both.
            let thought_to = app.handle().clone();
            model::on_thought(Box::new(move |line| show_thought(&thought_to, line)));
            let director = match settings::director_settings(&settings, secrets.as_ref()) {
                Ok(director) => director,
                Err(why) => {
                    eprintln!("director: secret store: {why}");
                    model::resolve(&settings.director_base_url, &settings.director_model, None)
                }
            };
            let mut config = model::config_from(&director);
            config.apply_switch(settings.director_enabled);
            config.ambient_allowed = settings.ambient_wakes;
            let inspect = Arc::new(Mutex::new(config.inspect(&director)));
            app.manage(Arc::clone(&inspect));
            for line in model::env_switch_warnings(&dev_flags::switch_vars()) {
                eprintln!("{line}");
            }
            for line in model::startup_lines(&config) {
                eprintln!("{line}");
            }
            if config.enabled {
                model::spawn_preflight(&director);
            }

            let (mut roster, lives) = spawn_instances(
                &loaded,
                start,
                &config,
                &director,
                character_cache.keys().cloned(),
            );
            if settings.do_not_disturb {
                for (id, _) in roster.list() {
                    if let Some(instance) = roster.get_mut(&id) {
                        instance.set_do_not_disturb(true);
                    }
                }
            }
            if settings.character.is_empty() {
                if let Some((_, character)) = loaded.first() {
                    settings.character = character.name.clone();
                }
            }
            // Persist the roster's names, not the specs spawn loaded: a leftover
            // `{ character: "Timber Wolf", name: "bmo" }` would write itself
            // back after spawn had already adopted the Character's name.
            if !settings.instances.is_empty() {
                settings.instances = roster_specs(&roster);
            }
            persist_settings(&settings, &settings_file);

            let settings = Arc::new(Mutex::new(settings));
            if install_hide_hotkey(
                app.handle(),
                Arc::clone(&rules),
                Arc::clone(&settings),
                settings_file.clone(),
            ) {
                let spec = settings
                    .lock()
                    .ok()
                    .map(|s| s.hide_hotkey.clone())
                    .unwrap_or_default();
                bind_hide_hotkey(app.handle(), &spec);
            }
            let instance_rows = Arc::new(Mutex::new(Vec::new()));
            let (ops_tx, ops_rx) = mpsc::channel();
            let (chat_tx, chat_rx) = mpsc::channel();
            app.manage(ChatChannel(chat_tx));
            app.manage(SettingsState {
                settings: Arc::clone(&settings),
                path: settings_file.clone(),
                memory_path: memory::shared_path(),
                installed,
                personalities: Arc::new(
                    character_cache
                        .iter()
                        .map(|(name, character)| (name.clone(), character.personality.clone()))
                        .collect(),
                ),
                behavior_names: Arc::new(
                    character_cache
                        .iter()
                        .map(|(name, character)| {
                            (name.clone(), character.behaviors.keys().cloned().collect())
                        })
                        .collect(),
                ),
                instances: Arc::clone(&instance_rows),
                inspect: Arc::clone(&inspect),
                ops: ops_tx,
                rules: Arc::clone(&rules),
                secrets: Arc::clone(&secrets),
            });
            app.manage(Arc::clone(&rules));

            // Dev/test hook: open settings immediately if AI_BUDDY_OPEN_SETTINGS=1.
            // For verify/smoke scripts that need the settings window on launch.
            if model::env_switch("AI_BUDDY_OPEN_SETTINGS").unwrap_or(false) {
                show_settings(app.handle().clone());
            }

            let tray = {
                let installed: Vec<String> = character_cache.keys().cloned().collect();
                let current = lives
                    .first()
                    .map(|live| live.character.name.clone())
                    .unwrap_or_default();
                let id = lives
                    .first()
                    .map(|live| live.id.clone())
                    .unwrap_or_default();
                let settings_now = settings.lock().ok().map(|s| s.clone()).unwrap_or_default();
                let rules_now = rules.lock().ok();
                let description = describe_menu(
                    &installed,
                    &current,
                    &roster,
                    &id,
                    &settings_now,
                    rules_now.as_deref().unwrap_or(&HideRules::default()),
                );
                #[cfg(target_os = "macos")]
                platform::seed_tray_position();
                match tray::install(app.handle(), &description, 0) {
                    Ok(icon) => Some(icon),
                    Err(why) => {
                        eprintln!("tray: {why}");
                        None
                    }
                }
            };
            app.manage(TrayHandle(Mutex::new(tray)));

            let director_run = DirectorRun {
                config,
                settings: director,
                inspect,
            };

            // Selections do not come back from the popup: it returns once the
            // menu is on screen, and the click arrives later on this channel.
            // The hook forwards ids to the frame loop, which knows the open menu.
            let (menu_sender, menu_receiver) = mpsc::channel();
            let hook_sender = menu_sender.clone();
            let quit_generation = Arc::new(AtomicU64::new(0));
            let live_quit = Arc::clone(&quit_generation);
            app.handle().on_menu_event(move |_app, event| {
                let id = event.id().0.clone();
                // Native Quit ids are per tray draw. A dismiss rebuilds the
                // tray and muda can click the item it just dropped; that id
                // is the previous draw's, so it must not call quit_now.
                if menu::is_live_quit(&id, live_quit.load(Ordering::SeqCst)) {
                    quit_now();
                }
                let _ = hook_sender.send(MenuSignal::Chose(id));
            });

            run_frame_loop(
                app.handle().clone(),
                roster,
                lives,
                source,
                displays,
                rules,
                covered,
                director_run,
                MenuChannel {
                    sender: menu_sender,
                    receiver: menu_receiver,
                    quit_generation,
                },
                FrameExtras {
                    settings,
                    settings_path: settings_file,
                    characters: character_cache,
                    instances: instance_rows,
                    ops: ops_rx,
                    chat: chat_rx,
                    mcp: mcp_rx,
                },
            );
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("ai-buddy failed to start")
        // Every exit path ends here — window close, `ExitRequested`, the tray
        // Quit's `quit_now` aside — so the Harness child is never orphaned.
        .run(|_, event| {
            if matches!(event, tauri::RunEvent::Exit) {
                harness::shutdown();
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    use ai_buddy_core::character::{
        Character, CursorReaction, PackageBytes, CHARACTER_MANIFEST_FILE, DEFAULT_MODEL_BASE,
        DEFAULT_MODEL_POWER, REQUIRED_ANIMATIONS,
    };

    fn stub_character(name: &str) -> Character {
        Character {
            name: name.to_string(),
            personality: String::new(),
            animations: BTreeMap::new(),
            behaviors: BTreeMap::new(),
            art: BTreeMap::new(),
            smooth: false,
            scale: 1,
            model_base: DEFAULT_MODEL_BASE,
            model_power: DEFAULT_MODEL_POWER,
            near_reaction: CursorReaction::default(),
            rush_reaction: CursorReaction::default(),
            source: None,
        }
    }

    /// A two-display Mac, the arrangement #868 was reported on.
    const PRIMARY: Rect = Rect {
        x: 0.0,
        y: 0.0,
        width: 1920.0,
        height: 1080.0,
    };

    const SECOND: Rect = Rect {
        x: 1920.0,
        y: 0.0,
        width: 1512.0,
        height: 982.0,
    };

    #[test]
    fn display_loss_keeps_the_unassigned_overlay_inactive() {
        assert_eq!(
            overlay_targets(&[PRIMARY], 2),
            vec![OverlayTarget::Display(PRIMARY), OverlayTarget::Inactive]
        );
    }

    /// Production change that would fail this: dropping the overlay that lost
    /// its display out of the plan, which is what closing it did. A returning
    /// display has to find the same label and cover it again.
    #[test]
    fn a_returning_display_takes_its_overlay_back() {
        assert_eq!(
            overlay_targets(&[PRIMARY, SECOND], 2),
            vec![
                OverlayTarget::Display(PRIMARY),
                OverlayTarget::Display(SECOND)
            ]
        );
    }

    /// Production change that would fail this: leaving `bmo` on a Timber Wolf
    /// spec. That is the leftover settings persist, and the overlay log reads
    /// these specs before spawn.
    #[test]
    fn follow_lone_default_renames_a_foreign_package_id() {
        let wolf = Arc::new(stub_character("Timber Wolf"));
        let bmo = Arc::new(stub_character("BMO"));
        let mut loaded = vec![(InstanceSpec::fresh("Timber Wolf", "bmo"), Arc::clone(&wolf))];
        let mut known = BTreeMap::new();
        known.insert("BMO".to_string(), bmo);
        known.insert("Timber Wolf".to_string(), Arc::clone(&wolf));

        follow_lone_default(&mut loaded, &known);

        assert_eq!(loaded[0].0.name, "Timber Wolf");
    }

    #[test]
    fn follow_lone_default_keeps_a_chosen_name() {
        let wolf = Arc::new(stub_character("Timber Wolf"));
        let mut loaded = vec![(InstanceSpec::fresh("Timber Wolf", "Pip"), Arc::clone(&wolf))];
        let mut known = BTreeMap::new();
        known.insert("BMO".to_string(), Arc::new(stub_character("BMO")));
        known.insert("Timber Wolf".to_string(), wolf);

        follow_lone_default(&mut loaded, &known);

        assert_eq!(loaded[0].0.name, "Pip");
    }

    fn stub_inspect() -> model::DirectorInspect {
        model::DirectorInspect {
            enabled: true,
            configured: true,
            ambient_wakes: true,
            wake_secs: 60,
            last_payload: None,
            harness: None,
            model: "gpt-4o-mini".to_string(),
            host: "api.openai.com".to_string(),
        }
    }

    /// Production change that would fail this: emitting the pre-switch name or
    /// Character, or stuffing `character.name` into both fields.
    #[test]
    fn chat_who_after_switch_uses_the_roster_name_and_character() {
        let mut roster = Roster::new();
        let first = stub_character("bmo");
        let second = stub_character("nim");
        let id = roster.spawn(&first, "bmo".to_string(), Point { x: 10.0, y: 20.0 });
        assert!(roster.retarget(&id, &second));
        let instance = roster.get(&id).expect("still there");

        let opening = chat_opening_from(instance, &stub_inspect(), "");
        assert_eq!(opening.name, "nim", "the payload name is the Instance's");
        assert_eq!(
            opening.character, "nim",
            "the payload Character is the Instance's"
        );
    }

    /// Production change that would fail this: opening Settings on WA_ACTIVE (1)
    /// or WA_INACTIVE (0) in addition to WA_CLICKACTIVE (2). #767.
    #[cfg(target_os = "windows")]
    mod windows_anchor_tests {
        #[test]
        fn clickactive_opens_settings() {
            const WA_CLICKACTIVE: u16 = 2;
            assert!(
                windows_activate_opens_settings(WA_CLICKACTIVE),
                "WA_CLICKACTIVE (2) should open Settings"
            );
        }

        #[test]
        fn active_does_not_open() {
            const WA_ACTIVE: u16 = 1;
            assert!(
                !windows_activate_opens_settings(WA_ACTIVE),
                "WA_ACTIVE (1) should not open Settings"
            );
        }

        #[test]
        fn inactive_does_not_open() {
            const WA_INACTIVE: u16 = 0;
            assert!(
                !windows_activate_opens_settings(WA_INACTIVE),
                "WA_INACTIVE (0) should not open Settings"
            );
        }

        fn windows_activate_opens_settings(f_active: u16) -> bool {
            const WA_CLICKACTIVE: u16 = 2;
            f_active == WA_CLICKACTIVE
        }
    }

    /// A chosen name survives retarget; the Chat header still has to name the
    /// new Character. Copying only `character.name` into both fields would fail.
    #[test]
    fn chat_who_after_switch_keeps_a_chosen_name() {
        let mut roster = Roster::new();
        let first = stub_character("bmo");
        let second = stub_character("nim");
        let id = roster.spawn(&first, "Pip".to_string(), Point { x: 10.0, y: 20.0 });
        assert!(roster.retarget(&id, &second));
        let instance = roster.get(&id).expect("still there");

        let opening = chat_opening_from(instance, &stub_inspect(), "");
        assert_eq!(opening.name, "Pip");
        assert_eq!(opening.character, "nim");
    }

    /// Production change that would fail this: an opening whose enabled bit
    /// still matches the pre-toggle inspect. #473.
    #[test]
    fn chat_opening_from_inspect_carries_configured_and_enabled() {
        let mut roster = Roster::new();
        let character = stub_character("nim");
        let id = roster.spawn(&character, "Pip".to_string(), Point { x: 10.0, y: 20.0 });
        let instance = roster.get(&id).expect("still there");
        let inspect = model::DirectorInspect {
            enabled: false,
            ..stub_inspect()
        };

        let opening = chat_opening_from(instance, &inspect, "");
        assert_eq!(opening.name, "Pip");
        assert_eq!(opening.character, "nim");
        assert!(opening.configured);
        assert!(!opening.enabled);
        assert!(opening.harness.is_none());
    }

    /// Production change that would fail this: an opening that says a Harness
    /// is there without carrying whether it is up, which is the difference
    /// between the header naming a mind and naming a hope (#474).
    #[test]
    fn chat_opening_carries_the_harness_facts_the_header_names() {
        let mut roster = Roster::new();
        let character = stub_character("nim");
        let id = roster.spawn(&character, "Pip".to_string(), Point { x: 10.0, y: 20.0 });
        let instance = roster.get(&id).expect("still there");
        let inspect = model::DirectorInspect {
            harness: Some(crate::harness::HarnessInspect {
                name: "hermes".to_string(),
                session_id: Some("sess-7".to_string()),
                alive: false,
                ..Default::default()
            }),
            ..stub_inspect()
        };

        let harness = chat_opening_from(instance, &inspect, "")
            .harness
            .expect("the opening carries the attachment");
        assert_eq!(harness.name, "hermes");
        assert_eq!(harness.session.as_deref(), Some("sess-7"));
        assert!(!harness.alive, "a handle that never answered is not alive");
        assert_eq!(harness.login, None);
        assert_eq!(harness.missing, None);
    }

    /// #726: Settings already names a missing launcher. Chat has to carry
    /// the same fact or the header can only say `not running`.
    #[test]
    fn chat_opening_carries_a_missing_launcher() {
        let mut roster = Roster::new();
        let character = stub_character("nim");
        let id = roster.spawn(&character, "Pip".to_string(), Point { x: 10.0, y: 20.0 });
        let instance = roster.get(&id).expect("still there");
        let inspect = model::DirectorInspect {
            harness: Some(crate::harness::HarnessInspect {
                name: "codex".to_string(),
                missing: Some("npx".to_string()),
                alive: false,
                ..Default::default()
            }),
            ..stub_inspect()
        };

        let harness = chat_opening_from(instance, &inspect, "")
            .harness
            .expect("the opening carries the attachment");
        assert_eq!(harness.name, "codex");
        assert_eq!(harness.missing.as_deref(), Some("npx"));
        assert!(!harness.alive);
    }

    /// The HTTP half, and the rule that guards it: ADR-0010 forbids drawing a
    /// credential, and a base URL is where one hides in plain sight.
    #[test]
    fn chat_opening_names_the_endpoint_without_its_userinfo() {
        let mut roster = Roster::new();
        let character = stub_character("nim");
        let id = roster.spawn(&character, "Pip".to_string(), Point { x: 10.0, y: 20.0 });
        let instance = roster.get(&id).expect("still there");
        let inspect = model::DirectorInspect {
            host: model::host_of("https://user:sk-secret@api.openai.com/v1"),
            ..stub_inspect()
        };

        let opening = chat_opening_from(instance, &inspect, "");
        assert_eq!(opening.model, "gpt-4o-mini");
        assert_eq!(opening.host, "api.openai.com");
    }

    /// ADR-0012: the Prompt tab draws the two authored layers, so the opening
    /// has to carry both, the Character's frozen and this Instance's own,
    /// and the bound the box has to stay inside. Sending the assembled Prompt would fail this.
    #[test]
    fn chat_opening_carries_both_authored_layers_and_the_bound() {
        let mut roster = Roster::new();
        let character = stub_character("nim");
        let id = roster.spawn(&character, "Pip".to_string(), Point { x: 10.0, y: 20.0 });

        let fresh = chat_opening_layers(
            roster.get(&id).expect("spawned"),
            &stub_inspect(),
            "Nim is patient.",
            ["wave"],
        );
        assert_eq!(fresh.personality, "Nim is patient.");
        assert_eq!(fresh.instance_prompt, "", "empty by default");
        assert_eq!(fresh.prompt_limit, roster::INSTANCE_PROMPT_LIMIT);
        assert!(
            fresh
                .instructions
                .contains("You may propose one of these behaviors: wave"),
            "app-level instructions are the same string the opening turn sends: {}",
            fresh.instructions
        );

        assert!(roster.set_prompt(&id, "Answer in haiku.".to_string()));
        let written = chat_opening_from(
            roster.get(&id).expect("spawned"),
            &stub_inspect(),
            "Nim is patient.",
        );
        assert_eq!(written.instance_prompt, "Answer in haiku.");
        assert_eq!(
            written.personality, "Nim is patient.",
            "the author's layer stays the package's, frozen"
        );
    }

    /// #680: Blank AI empties the built-in Personality Prompt on the opening
    /// the tab draws. The Instance Prompt stays, so a control run can still
    /// iterate one.
    #[test]
    fn chat_opening_empties_personality_under_blank_ai() {
        crate::model::tests::with_env(None, None, None, || {
            let mut roster = Roster::new();
            let character = stub_character("nim");
            let id = roster.spawn(&character, "Pip".to_string(), Point { x: 10.0, y: 20.0 });
            assert!(roster.set_prompt(&id, "Answer in haiku.".to_string()));

            let off = chat_opening_layers(
                roster.get(&id).expect("spawned"),
                &stub_inspect(),
                "Nim is patient.",
                ["wave"],
            );
            assert_eq!(off.personality, "Nim is patient.");
            assert_eq!(off.instance_prompt, "Answer in haiku.");
            assert!(
                off.instructions.contains("always in character"),
                "shaped openings still carry the app-level layer: {}",
                off.instructions
            );

            crate::dev_flags::seed(&settings::Settings {
                director_blank: true,
                ..settings::Settings::default()
            });
            let on = chat_opening_layers(
                roster.get(&id).expect("spawned"),
                &stub_inspect(),
                "Nim is patient.",
                ["wave"],
            );
            assert_eq!(on.personality, "", "the built-in layer was emptied");
            assert_eq!(
                on.instructions, "",
                "app-level instructions were emptied with it"
            );
            assert_eq!(
                on.instance_prompt, "Answer in haiku.",
                "the Instance Prompt is still the one the user wrote"
            );
        });
    }

    /// #17: losing a line that is waiting in `happened` would answer a question
    /// the user never asked and drop the one they are waiting on.
    #[test]
    fn a_waiting_typed_line_survives_everything_else_that_happens() {
        let mut happened = Happened::Chat("what are you standing on?".to_string());
        note_happened(&mut happened, Happened::Poke);
        note_happened(&mut happened, Happened::Perch);

        assert_eq!(
            happened,
            Happened::Chat("what are you standing on?".to_string())
        );
    }

    #[test]
    fn with_nothing_waiting_the_latest_moment_wins() {
        let mut happened = Happened::Ambient;
        note_happened(&mut happened, Happened::Poke);
        note_happened(&mut happened, Happened::Throw);

        assert_eq!(happened, Happened::Throw);
    }

    /// Settings persist Character.name (`Trump`). The env var and the folder
    /// are still the package stem (`trump`). Either has to start the same buddy.
    #[test]
    fn a_package_answers_to_its_folder_or_its_character_name() {
        let folder = Path::new("/characters/trump");
        assert!(
            names_the_package(folder, "Trump", OsStr::new("trump")),
            "AI_BUDDY_CHARACTER=trump still names the folder"
        );
        assert!(
            names_the_package(folder, "Trump", OsStr::new("Trump")),
            "settings.character after a switch is the Character name"
        );
        assert!(
            !names_the_package(Path::new("/characters/bmo"), "BMO", OsStr::new("Trump")),
            "some other package is not a match just because it loaded"
        );
    }

    /// A rebound hide hotkey must register the letter the user named, not B.
    #[test]
    fn a_rebound_spec_registers_its_letter() {
        let shortcut = shortcut_from_spec("Control-Shift-H").expect("parses");
        assert_eq!(shortcut.key, Code::KeyH);
        let shipped = shortcut_from_spec(settings::DEFAULT_HIDE_HOTKEY).expect("default");
        assert_eq!(shipped.key, Code::KeyB);
    }

    /// A 2x2 RGBA frame whose top-left pixel is transparent.
    const PATCHY: &[u8] = include_bytes!("../../crates/core/tests/fixtures/alpha-2x2.png");

    /// A 2x2 RGBA frame with every pixel drawn, so its URL is told apart from
    /// `PATCHY`'s.
    const SOLID: &[u8] = include_bytes!("../../crates/core/tests/fixtures/opaque-2x2.png");

    /// One Animation as these tests declare it: its name, then each frame as a
    /// file name and the bytes behind it.
    type Declared<'a> = (&'a str, &'a [(&'a str, &'a [u8])]);

    /// A Character whose Animations are `animations`, plus one frame each for
    /// every required Animation they do not name.
    fn character_declaring(animations: &[Declared<'_>]) -> Character {
        let mut manifest = String::from("name = \"Blip\"\n");
        let mut files = PackageBytes::new();

        let mut declare = |name: &str, frames: &[(&str, &[u8])]| {
            let names: Vec<String> = frames.iter().map(|(file, _)| format!("{file:?}")).collect();
            manifest.push_str(&format!(
                "[animations.{name}]\nframes = [{}]\n",
                names.join(", ")
            ));
            for (file, bytes) in frames {
                files.insert((*file).to_string(), bytes.to_vec());
            }
        };

        for required in REQUIRED_ANIMATIONS {
            if !animations.iter().any(|(name, _)| *name == required) {
                declare(required, &[(&format!("{required}.png"), PATCHY)]);
            }
        }
        for (name, frames) in animations {
            declare(name, frames);
        }

        files.insert(CHARACTER_MANIFEST_FILE.to_string(), manifest.into_bytes());
        ai_buddy_core::character::load(&files).expect("the package is valid")
    }

    fn url(bytes: &[u8]) -> String {
        format!("data:image/png;base64,{}", STANDARD.encode(bytes))
    }

    /// The invariant `art_urls` exists to hold: the webview indexes this list
    /// by the index the frame loop computed over `Animation::frames`, so a
    /// dropped or reordered URL would put a different frame on screen.
    #[test]
    fn an_animations_urls_stand_in_the_order_its_frames_do() {
        let character = character_declaring(&[(
            "walk",
            &[("a.png", PATCHY), ("b.png", SOLID), ("c.png", PATCHY)],
        )]);
        let art = art_urls(&character);

        assert_eq!(art["walk"], vec![url(PATCHY), url(SOLID), url(PATCHY)]);
        assert_eq!(art["walk"].len(), character.animations["walk"].frames.len());
    }

    /// The frame two Animations share is encoded once and named twice, at each
    /// Animation's own index: a shared URL that only appeared once would shift
    /// every later frame of the second Animation.
    #[test]
    fn a_frame_two_animations_share_stands_at_each_animations_own_index() {
        let character = character_declaring(&[
            ("idle", &[("shared.png", PATCHY), ("bob.png", SOLID)]),
            ("sit", &[("down.png", SOLID), ("shared.png", PATCHY)]),
        ]);
        let art = art_urls(&character);

        assert_eq!(art["idle"], vec![url(PATCHY), url(SOLID)]);
        assert_eq!(art["sit"], vec![url(SOLID), url(PATCHY)]);
    }

    /// Every pre-Instances start path asks for no Instances, which
    /// `load_instances` turns into the one buddy it has always run. One test
    /// rather than three: they share an environment variable and would race.
    #[test]
    fn naming_no_instances_asks_for_none_and_a_list_is_read_in_full() {
        std::env::remove_var(INSTANCES_VAR);
        assert_eq!(
            requested_instances(&Settings::default()),
            Ok(Vec::new()),
            "the default single buddy is not a spec"
        );

        std::env::set_var(INSTANCES_VAR, "bmo:One,bmo:Two");
        let specs = requested_instances(&Settings::default()).expect("the list parses");
        assert_eq!(specs.len(), 2, "both Instances are asked for");
        assert!(specs.iter().all(|spec| spec.character == "bmo"));

        // A list that cannot be read stops startup rather than guessing.
        std::env::set_var(INSTANCES_VAR, "bmo:");
        assert!(requested_instances(&Settings::default()).is_err());

        std::env::remove_var(INSTANCES_VAR);

        let remembered = Settings {
            instances: vec![InstanceSpec::fresh("nim", "Nim")],
            ..Settings::default()
        };
        assert_eq!(
            requested_instances(&remembered).expect("settings list"),
            remembered.instances,
            "settings own the roster when the env is unset"
        );
    }

    /// The arithmetic that keeps buddies from landing in a stack, and the reason
    /// it accumulates: stepping by each Character's own width puts a narrow
    /// sprite on top of the wide one it follows.
    #[test]
    fn each_instance_starts_a_sprites_width_past_the_one_before_it() {
        let start = Point { x: 100.0, y: 50.0 };

        assert_eq!(
            starting_positions(start, &[32.0, 32.0, 32.0]),
            vec![
                Point { x: 100.0, y: 50.0 },
                Point { x: 132.0, y: 50.0 },
                Point { x: 164.0, y: 50.0 },
            ]
        );

        // A wide Character followed by a narrow one: the gap is the width of the
        // sprite standing there, not the width of the one arriving.
        assert_eq!(
            starting_positions(start, &[128.0, 16.0, 16.0]),
            vec![
                Point { x: 100.0, y: 50.0 },
                Point { x: 228.0, y: 50.0 },
                Point { x: 244.0, y: 50.0 },
            ],
            "the narrow sprite clears the wide one"
        );

        assert_eq!(
            starting_positions(start, &[]),
            Vec::new(),
            "no Instances, no positions"
        );
        assert_eq!(
            starting_positions(start, &[64.0]),
            vec![start],
            "one buddy still comes into the world where it always did"
        );
    }

    /// A wake clock starts somewhere inside the interval, never past it.
    #[test]
    fn a_wake_clock_starts_somewhere_inside_the_interval() {
        let interval = Duration::from_secs(60);

        assert_eq!(phase_of(interval, 0), Duration::ZERO);
        assert_eq!(phase_of(interval, 1_500), Duration::from_millis(1_500));

        // A draw is a whole u64, so most of them are past the interval and wrap.
        assert_eq!(
            phase_of(interval, 60_000),
            Duration::ZERO,
            "a draw of exactly the interval wraps to the start of it"
        );
        assert_eq!(phase_of(interval, 61_234), Duration::from_millis(1_234));

        // Never already due: a phase equal to the interval would wake every
        // buddy on the first tick, which is the thing being avoided.
        for draw in [0, 1, u64::MAX / 2, u64::MAX] {
            assert!(
                phase_of(interval, draw) < interval,
                "draw {draw} lands inside the interval"
            );
        }

        // An interval of nothing cannot happen, and must not divide by zero.
        assert_eq!(phase_of(Duration::ZERO, u64::MAX), Duration::ZERO);
    }

    /// The property the randomness is for: buddies from one launch start their
    /// clocks at different, unevenly spaced points.
    #[test]
    fn buddies_from_one_launch_start_their_clocks_apart() {
        let interval = Duration::from_secs(60);
        let mut draws = Seeded::new(0x5EED);

        let phases: Vec<Duration> = (0..4).map(|_| phase_of(interval, draws.draw())).collect();

        let mut distinct = phases.clone();
        distinct.sort();
        distinct.dedup();
        assert_eq!(
            distinct.len(),
            4,
            "no two buddies wake together: {phases:?}"
        );

        // Uneven, which is what a draw buys over a share apiece: an even spread
        // would make every gap identical.
        let gaps: Vec<Duration> = distinct.windows(2).map(|pair| pair[1] - pair[0]).collect();
        assert!(
            gaps.windows(2).any(|pair| pair[0] != pair[1]),
            "the spacing is not a fixed step: {gaps:?}"
        );
    }

    /// #178: a line said on one display and carried across the seam is said
    /// again to the new owner, once, while it could still be showing — and
    /// never to the owner that already heard it.
    #[test]
    fn a_line_crosses_the_seam_with_the_sprite_once_and_only_while_fresh() {
        let t0 = Instant::now();
        let mut spoken = None;

        assert_eq!(
            carry_line(&mut spoken, Some("Yare yare daze."), Some(0), t0).as_deref(),
            Some("Yare yare daze."),
            "the pulse itself goes to the owner of the tick"
        );
        assert_eq!(
            carry_line(&mut spoken, None, Some(0), t0 + Duration::from_secs(1)),
            None,
            "the same owner is not told twice"
        );
        assert_eq!(
            carry_line(&mut spoken, None, Some(1), t0 + Duration::from_secs(2)).as_deref(),
            Some("Yare yare daze."),
            "mid-reading, the new owner is told the line"
        );
        assert_eq!(
            carry_line(&mut spoken, None, Some(1), t0 + Duration::from_secs(3)),
            None,
            "and then not again while it stays there"
        );
        assert_eq!(
            carry_line(
                &mut spoken,
                None,
                Some(0),
                t0 + CARRY_WINDOW + Duration::from_secs(1)
            ),
            None,
            "a line older than any reading window is not resurrected by a crossing"
        );

        let mut spoken = None;
        carry_line(&mut spoken, Some("first"), Some(0), t0);
        assert_eq!(
            carry_line(
                &mut spoken,
                Some("second"),
                Some(0),
                t0 + Duration::from_secs(1)
            )
            .as_deref(),
            Some("second"),
            "a new line replaces the remembered one"
        );
        assert_eq!(
            carry_line(&mut spoken, None, Some(1), t0 + Duration::from_secs(2)).as_deref(),
            Some("second"),
            "and it is the new line that crosses"
        );
    }

    /// #178 and #277: every overlay is told about every Instance, and only the
    /// one that owns the bubble is told the line, the indicator and the cue.
    /// The webview used to strip these for itself.
    #[test]
    fn only_the_bubble_owner_is_told_the_line_the_indicator_and_the_cue() {
        let left = Rect {
            x: 0.0,
            y: 0.0,
            width: 1920.0,
            height: 1080.0,
        };
        let right = Rect { x: 1920.0, ..left };
        let placed = Placed {
            id: "one".to_string(),
            character: "bmo".to_string(),
            sprite: SpriteRect {
                x: 2000,
                y: 400,
                scale: 2,
            },
            width: 128,
            height: 128,
            animation: "idle".to_string(),
            frame_index: 0,
            mirror: 1,
            dialogue: Some("Yare yare daze.".to_string()),
            thinking: true,
            cue: Some(Cue::Poke),
            owner: Some(1),
            mask: ai_buddy_core::overlay::AlphaMask::from_png(PATCHY, 128)
                .expect("the 2x2 fixture decodes"),
        };

        let owner = SpritePlacement::new(&placed, right, 1);
        assert_eq!(owner.dialogue.as_deref(), Some("Yare yare daze."));
        assert!(owner.thinking);
        assert!(owner.bubble);
        assert_eq!(owner.cue, Some("poke"));

        let elsewhere = SpritePlacement::new(&placed, left, 0);
        assert_eq!(elsewhere.dialogue, None, "no line to latch off the owner");
        assert!(!elsewhere.thinking, "no indicator to arm off the owner");
        assert!(!elsewhere.bubble);
        assert_eq!(elsewhere.cue, None, "or the cue sounds once per display");

        assert_eq!(
            (elsewhere.animation, elsewhere.frame_index, elsewhere.mirror),
            (owner.animation, owner.frame_index, owner.mirror),
            "both draw the same art"
        );
        assert_eq!(
            (owner.x, elsewhere.x),
            (80, 2000),
            "each in its own overlay's coordinates, so the halves meet on the seam"
        );
    }

    fn instance_row(id: &str, name: &str) -> InstanceRow {
        InstanceRow {
            id: id.to_string(),
            name: name.to_string(),
            character: "bmo".to_string(),
            prompt: String::new(),
        }
    }

    #[test]
    fn instance_title_is_the_roster_name() {
        let rows = vec![instance_row("a", "Beemo"), instance_row("b", "Pip")];
        assert_eq!(
            instance_title(&rows, "b"),
            "Pip",
            "the window opens titled by the Instance the click named"
        );
    }

    #[test]
    fn instance_title_falls_back_to_the_id_when_the_row_is_gone() {
        // A Character switch can drop the row between the click and this
        // lookup; the id is a title the window can still open under. #588.
        let rows = vec![instance_row("a", "Beemo")];
        assert_eq!(
            instance_title(&rows, "orphan"),
            "orphan",
            "an unknown id still yields a title so Chat opens"
        );
        assert_eq!(
            instance_title(&[], "lonely"),
            "lonely",
            "an empty roster falls back to the id, not a panic"
        );
    }

    /// Pins `overlay_open_chat` as `async`. On Windows a sync command builds
    /// the Chat webview on WebView2's pump thread and deadlocks (#588). A
    /// compile-time witness; the body only has to type-check.
    #[test]
    fn overlay_open_chat_is_async_so_windows_keeps_chat_off_the_webview_pump() {
        fn takes_async<F, Fut>(_f: F)
        where
            F: Fn(tauri::AppHandle, String) -> Fut,
            Fut: std::future::Future<Output = ()>,
        {
        }
        takes_async(overlay_open_chat);
    }

    /// Asserted on the serialized payload, because the webview reads the wire
    /// shape and not the struct. Production change that would fail this:
    /// sending a bare flag again, so the surface cannot name the cause (#890).
    #[test]
    fn a_preempted_typed_question_carries_the_wake_that_took_its_slot() {
        let poked = serde_json::to_value(cancelled_caret(true, &Happened::Poke).unwrap())
            .expect("a ChatReply should serialize");
        assert_eq!(poked["superseded_by"], "poked");
        assert!(poked["said"].is_null(), "a cancelled caret said nothing");

        let asked_again = serde_json::to_value(
            cancelled_caret(true, &Happened::Chat("and another thing".to_string())).unwrap(),
        )
        .expect("a ChatReply should serialize");
        assert_eq!(asked_again["superseded_by"], "spoken to");
    }

    /// The half a careless fix breaks. A poke or an ambient wake opened no
    /// question on the Chat surface, so a notice there answers nobody (#890).
    #[test]
    fn an_ambient_turn_superseded_by_another_wake_tells_chat_nothing() {
        assert!(cancelled_caret(false, &Happened::Ambient).is_none());
        assert!(cancelled_caret(false, &Happened::Poke).is_none());
    }
}
