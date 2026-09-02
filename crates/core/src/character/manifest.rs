//! TOML parse of a Character Manifest, yielding `Declared*` for `load`.

use std::collections::BTreeMap;
use std::time::Duration;

use toml_edit::{Document, Item};

use super::{
    CursorReaction, Primitive, Trigger, CHARACTER_MANIFEST_FILE, DEFAULT_FPS, DEFAULT_WEIGHT,
    MAX_FPS, MAX_FRAMES, MAX_SCALE, PRIMITIVES,
};

/// A Character Manifest as written, before its declarations are checked
/// against the art and against each other.
#[derive(Default)]
pub(super) struct Declared {
    pub(super) name: Option<String>,
    pub(super) smooth: Option<bool>,
    pub(super) scale: Option<u32>,
    pub(super) model_base: Option<u32>,
    pub(super) model_power: Option<u32>,
    pub(super) near_reaction: Option<CursorReaction>,
    pub(super) rush_reaction: Option<CursorReaction>,
    pub(super) animations: BTreeMap<String, DeclaredAnimation>,
    pub(super) behaviors: BTreeMap<String, DeclaredBehavior>,
}

pub(super) struct DeclaredAnimation {
    pub(super) frames: Vec<String>,
    pub(super) fps: u32,
    pub(super) looping: bool,
    pub(super) variant_of: Option<String>,
}

pub(super) struct DeclaredBehavior {
    pub(super) primitives: Vec<Primitive>,
    pub(super) then: Option<String>,
    pub(super) weight: u32,
    pub(super) trigger: Option<Trigger>,
}

/// Read the Character Manifest.
///
/// TOML gives the container: keys are unique, values are typed, and comments
/// are the parser's problem. Everything after that is still a closed set — a
/// declaration the loader does not know is an error and never a guess.
///
/// `None` when the manifest is not TOML at all. That is one error, not one
/// per declaration: past the first syntax mistake the parser would be
/// guessing, and a guess would report mistakes the author has not made.
pub(super) fn parse(manifest: &str, errors: &mut Vec<String>) -> Option<Declared> {
    let mut declared = Declared::default();

    let document = match Document::parse(manifest) {
        Ok(document) => document,
        Err(error) => {
            let at = error
                .span()
                .map(|span| format!(" at line {}", line_of(manifest, span.start)))
                .unwrap_or_default();
            // The parser's message can be as terse as "duplicate key", so the
            // offending text is quoted after it when the span has any.
            let offender = match wrote(manifest, error.span()) {
                Some(text) if !text.is_empty() => format!(" ({text})"),
                _ => String::new(),
            };
            errors.push(format!(
                "{CHARACTER_MANIFEST_FILE} is not TOML{at}: {}{offender}",
                error.message()
            ));
            return None;
        }
    };

    for (key, item) in document.iter() {
        match key {
            "name" => match item.as_str() {
                Some("") => errors.push("\"name\" is empty".to_string()),
                Some(name) => declared.name = Some(name.to_string()),
                None => errors.push(format!(
                    "\"name\" is {}, and must be text, as name = \"Blip\"",
                    wrote(manifest, item.span()).unwrap_or("?")
                )),
            },
            "animations" => match item.as_table_like() {
                Some(table) => {
                    for (name, item) in table.iter() {
                        if let Some(animation) = parse_animation(name, item, manifest, errors) {
                            declared.animations.insert(name.to_string(), animation);
                        }
                    }
                }
                None => errors.push(
                    "\"animations\" is not a set of tables; an Animation reads \
                     [animations.idle] with its frames, fps and loop below"
                        .to_string(),
                ),
            },
            "behaviors" => match item.as_table_like() {
                Some(table) => {
                    for (name, item) in table.iter() {
                        if let Some(behavior) = parse_behavior(name, item, manifest, errors) {
                            declared.behaviors.insert(name.to_string(), behavior);
                        }
                    }
                }
                None => errors.push(
                    "\"behaviors\" is not a set of tables; a Behavior reads \
                     [behaviors.greet] with its play, then, weight and when below"
                        .to_string(),
                ),
            },
            "render_mode" => match item.as_str() {
                Some("pixelated") => declared.smooth = Some(false),
                Some("smooth") => declared.smooth = Some(true),
                _ => errors.push(format!(
                    "\"render_mode\" is {}, and must be \"pixelated\" or \"smooth\"",
                    wrote(manifest, item.span()).unwrap_or("?")
                )),
            },
            "scale" => match item.as_integer() {
                Some(scale) if (1..=i64::from(MAX_SCALE)).contains(&scale) => {
                    declared.scale = Some(scale as u32)
                }
                _ => errors.push(format!(
                    "\"scale\" is {}, and must be a whole number from 1 to {MAX_SCALE}",
                    wrote(manifest, item.span()).unwrap_or("?")
                )),
            },
            "director" => parse_director(item, manifest, &mut declared, errors),
            "cursor" => parse_cursor(item, manifest, &mut declared, errors),
            other => errors.push(format!(
                "unknown declaration {other:?}; a Character Manifest declares \
                 name, render_mode, scale, animations, behaviors, director and cursor"
            )),
        }
    }

    Some(declared)
}

/// `[director]`: how proactive model calls space themselves.
fn parse_director(item: &Item, manifest: &str, declared: &mut Declared, errors: &mut Vec<String>) {
    let Some(table) = item.as_table_like() else {
        errors.push(
            "\"director\" is not a table; it reads [director] with \
             model_base and model_power below"
                .to_string(),
        );
        return;
    };

    for (key, item) in table.iter() {
        match key {
            "model_base" => match item.as_integer() {
                Some(base) if base >= 1 && u32::try_from(base).is_ok() => {
                    declared.model_base = Some(base as u32)
                }
                _ => errors.push(format!(
                    "\"director.model_base\" is {}, and must be a whole number from 1",
                    wrote(manifest, item.span()).unwrap_or("?")
                )),
            },
            "model_power" => match item.as_integer() {
                Some(power) if u32::try_from(power).is_ok() => {
                    declared.model_power = Some(power as u32)
                }
                _ => errors.push(format!(
                    "\"director.model_power\" is {}, and must be a whole number from 0",
                    wrote(manifest, item.span()).unwrap_or("?")
                )),
            },
            other => errors.push(format!(
                "unknown declaration director.{other:?}; [director] declares \
                 model_base and model_power"
            )),
        }
    }
}

/// `[cursor]`: how the Character reacts to cursor proximity.
fn parse_cursor(item: &Item, manifest: &str, declared: &mut Declared, errors: &mut Vec<String>) {
    let Some(table) = item.as_table_like() else {
        errors.push(
            "\"cursor\" is not a table; it reads [cursor] with \
             near_reaction and rush_reaction below"
                .to_string(),
        );
        return;
    };

    for (key, item) in table.iter() {
        match key {
            "near_reaction" => match parse_cursor_reaction(item.as_str()) {
                Some(reaction) => declared.near_reaction = Some(reaction),
                None => errors.push(format!(
                    "\"cursor.near_reaction\" is {}, and must be one of: \
                     indifferent, speak, face, toward, away, react",
                    wrote(manifest, item.span()).unwrap_or("?")
                )),
            },
            "rush_reaction" => match parse_cursor_reaction(item.as_str()) {
                Some(reaction) => declared.rush_reaction = Some(reaction),
                None => errors.push(format!(
                    "\"cursor.rush_reaction\" is {}, and must be one of: \
                     indifferent, speak, face, toward, away, react",
                    wrote(manifest, item.span()).unwrap_or("?")
                )),
            },
            other => errors.push(format!(
                "unknown declaration cursor.{other:?}; [cursor] declares \
                 near_reaction and rush_reaction"
            )),
        }
    }
}

/// Parse a cursor reaction from its manifest name.
fn parse_cursor_reaction(value: Option<&str>) -> Option<CursorReaction> {
    match value? {
        "indifferent" => Some(CursorReaction::Indifferent),
        "speak" => Some(CursorReaction::Speak),
        "face" => Some(CursorReaction::Face),
        "toward" => Some(CursorReaction::Toward),
        "away" => Some(CursorReaction::Away),
        "react" => Some(CursorReaction::React),
        _ => None,
    }
}

/// One `[animations.<name>]` table.
fn parse_animation(
    name: &str,
    item: &Item,
    manifest: &str,
    errors: &mut Vec<String>,
) -> Option<DeclaredAnimation> {
    let Some(table) = item.as_table_like() else {
        errors.push(format!(
            "animation {name:?} is not a table; an Animation reads \
             [animations.{name}] with its frames, fps and loop below"
        ));
        return None;
    };

    let mut frames = None;
    let mut fps = DEFAULT_FPS;
    let mut looping = true;
    let mut variant_of = None;

    for (key, item) in table.iter() {
        match key {
            "frames" => frames = frame_list(name, item, manifest, errors),
            "variant_of" => match item.as_str() {
                Some(base) if !base.is_empty() => variant_of = Some(base.to_string()),
                _ => errors.push(format!(
                    "variant_of for animation {name:?} is {}, and must name \
                     an animation, as variant_of = \"idle\"",
                    wrote(manifest, item.span()).unwrap_or("?")
                )),
            },
            "fps" => match item.as_integer() {
                Some(declared) if (1..=i64::from(MAX_FPS)).contains(&declared) => {
                    fps = declared as u32;
                }
                Some(declared) => errors.push(format!(
                    "fps for animation {name:?} is {declared}, and must be 1 to {MAX_FPS}"
                )),
                None => errors.push(format!(
                    "fps for animation {name:?} is {}, which is not a whole number",
                    wrote(manifest, item.span()).unwrap_or("?")
                )),
            },
            "loop" => match item.as_str() {
                Some("forever") => looping = true,
                Some("once") => looping = false,
                _ => errors.push(format!(
                    "loop mode for animation {name:?} is {}, \
                     and must be \"forever\" or \"once\"",
                    wrote(manifest, item.span()).unwrap_or("?")
                )),
            },
            other => errors.push(format!(
                "animation {name:?} declares unknown {other:?}; \
                 an Animation declares frames, fps, loop and variant_of"
            )),
        }
    }

    if !table.contains_key("frames") {
        errors.push(format!("animation {name:?} declares no frames"));
    }
    // No usable frame list — rejected above or by `frame_list` — means no
    // Animation, however sound its fps and loop mode were.
    let frames = frames?;
    Some(DeclaredAnimation {
        frames,
        fps,
        looping,
        variant_of,
    })
}

/// An Animation's `frames` list.
///
/// Counted before the file names are copied out, so a list built to be long
/// is rejected for its length alone.
fn frame_list(
    name: &str,
    item: &Item,
    manifest: &str,
    errors: &mut Vec<String>,
) -> Option<Vec<String>> {
    let Some(list) = item.as_array() else {
        errors.push(format!(
            "frames for animation {name:?} is {}, and must be a list of \
             frame files, as frames = [\"idle-0.png\"]",
            wrote(manifest, item.span()).unwrap_or("?")
        ));
        return None;
    };

    if list.len() > MAX_FRAMES {
        errors.push(format!(
            "animation {name:?} declares {} frames, \
             and an Animation may have at most {MAX_FRAMES}",
            list.len()
        ));
        return None;
    }

    let mut frames = Vec::new();
    for frame in list.iter() {
        match frame.as_str() {
            Some(file) => frames.push(file.to_string()),
            None => {
                errors.push(format!(
                    "animation {name:?} declares the frame {}, which is not a file name",
                    wrote(manifest, frame.span()).unwrap_or("?")
                ));
                return None;
            }
        }
    }
    if frames.is_empty() {
        errors.push(format!("animation {name:?} declares no frames"));
        return None;
    }
    Some(frames)
}

/// One `[behaviors.<name>]` table.
fn parse_behavior(
    name: &str,
    item: &Item,
    manifest: &str,
    errors: &mut Vec<String>,
) -> Option<DeclaredBehavior> {
    let Some(table) = item.as_table_like() else {
        errors.push(format!(
            "behavior {name:?} is not a table; a Behavior reads \
             [behaviors.{name}] with its play, then, weight and when below"
        ));
        return None;
    };

    let mut primitives = Vec::new();
    let mut then = None;
    let mut weight = DEFAULT_WEIGHT;
    let mut trigger = None;

    for (key, item) in table.iter() {
        match key {
            "play" => primitives = play_list(name, item, manifest, errors),
            "then" => match item.as_str() {
                Some(next) if !next.is_empty() => then = Some(next.to_string()),
                _ => errors.push(format!(
                    "then for behavior {name:?} is {}, and must name one Behavior, \
                     as then = \"settle\"",
                    wrote(manifest, item.span()).unwrap_or("?")
                )),
            },
            "weight" => match item.as_integer().and_then(|w| u32::try_from(w).ok()) {
                Some(declared) => weight = declared,
                None => errors.push(format!(
                    "weight for behavior {name:?} is {}, which is not a whole number",
                    wrote(manifest, item.span()).unwrap_or("?")
                )),
            },
            "when" => match item.as_str().and_then(parse_trigger) {
                Some(condition) => trigger = Some(condition),
                None => errors.push(format!(
                    "when for behavior {name:?} is {}, which is not a condition; \
                     a condition reads \"idle over 2m\", \"idle under 30s\" or \"app Safari\"",
                    wrote(manifest, item.span()).unwrap_or("?")
                )),
            },
            other => errors.push(format!(
                "behavior {name:?} declares unknown {other:?}; \
                 a Behavior declares play, then, weight and when"
            )),
        }
    }

    Some(DeclaredBehavior {
        primitives,
        then,
        weight,
        trigger,
    })
}

/// A Behavior's `play` list: Primitives by name, in play order.
///
/// A word that is not a Primitive is reported and dropped rather than
/// abandoning the declaration, so the rest of the list is still checked.
fn play_list(name: &str, item: &Item, manifest: &str, errors: &mut Vec<String>) -> Vec<Primitive> {
    let Some(list) = item.as_array() else {
        errors.push(format!(
            "play for behavior {name:?} is {}, and must be a list of \
             Primitives, as play = [\"react\", \"talk\"]",
            wrote(manifest, item.span()).unwrap_or("?")
        ));
        return Vec::new();
    };

    let mut primitives = Vec::new();
    for word in list.iter() {
        let primitive = word.as_str().and_then(|word| {
            PRIMITIVES
                .iter()
                .find_map(|(known, primitive)| (*known == word).then_some(*primitive))
        });
        match primitive {
            Some(primitive) => primitives.push(primitive),
            None => errors.push(format!(
                "behavior {name:?} declares {}, which is not a Primitive; \
                 the Primitives are {}",
                wrote(manifest, word.span()).unwrap_or("?"),
                PRIMITIVES
                    .iter()
                    .map(|(name, _)| *name)
                    .collect::<Vec<_>>()
                    .join(", ")
            )),
        }
    }
    primitives
}

/// The line a byte offset falls on, for a syntax error to name.
fn line_of(manifest: &str, offset: usize) -> usize {
    let offset = offset.min(manifest.len());
    manifest.as_bytes()[..offset]
        .iter()
        .filter(|byte| **byte == b'\n')
        .count()
        + 1
}

/// What the author wrote, quoted back at them when its type is wrong. The
/// manifest's own bytes rather than a re-rendering, so the author sees text
/// they can search their file for. `None` when the parser kept no span.
fn wrote(manifest: &str, span: Option<std::ops::Range<usize>>) -> Option<&str> {
    span.and_then(|span| manifest.get(span)).map(str::trim)
}

/// One trigger condition, or nothing when it is not one.
///
/// The application name is the rest of the line rather than one word, since
/// "Google Chrome" is what the platform reports and an author writes what they
/// see.
fn parse_trigger(value: &str) -> Option<Trigger> {
    let (condition, rest) = value.split_once(char::is_whitespace)?;
    let rest = rest.trim();

    match condition {
        "idle" => {
            let (comparison, duration) = rest.split_once(char::is_whitespace)?;
            let duration = parse_duration(duration.trim())?;
            match comparison {
                "over" => Some(Trigger::IdleOver(duration)),
                "under" => Some(Trigger::IdleUnder(duration)),
                _ => None,
            }
        }
        "app" => (!rest.is_empty()).then(|| Trigger::Frontmost(rest.to_string())),
        _ => None,
    }
}

/// A span written as a count and a unit, as `30s` or `2m`.
///
/// Stripped as a suffix rather than split at the last byte: a manifest is
/// untrusted text, and the last byte of "2\u{043c}" is the middle of a
/// character, which splitting would panic on.
fn parse_duration(text: &str) -> Option<Duration> {
    let (count, seconds_each) = match (text.strip_suffix('s'), text.strip_suffix('m')) {
        (Some(count), _) => (count, 1),
        (_, Some(count)) => (count, 60),
        _ => return None,
    };
    count
        .parse::<u64>()
        .ok()?
        .checked_mul(seconds_each)
        .map(Duration::from_secs)
}
