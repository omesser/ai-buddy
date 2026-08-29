//! The Character Package loader: bytes in, either a validated Character or the
//! list of mistakes its author has to fix.
//!
//! Pure and synchronous, like the rest of the Engine seam. Every byte is handed
//! to it, so it opens nothing, reaches no platform and cannot be slowed down by
//! a disk. That is also what lets a hostile package be tested as a map of file
//! bytes.
//!
//! A Character Package is untrusted input twice over: an author's mistake and a
//! deliberate attack arrive through the same door, and a Personality Prompt
//! reaches a model that can talk to an agent Harness. So the errors are the
//! product here as much as the Character is. Every rejection names the
//! declaration at fault and the line it is on, because the author is meant to
//! fix their package without reading this file.
//!
//! Two properties keep a package from enabling anything. The Character Manifest
//! rejects every declaration it does not know, so no package can invent a key
//! that grants a capability. And the Personality Prompt is prose in a file of
//! its own, never a declaration, so it can describe a Character that jumps
//! without the Character gaining a jump.
//!
//! The Character Manifest is one declaration per line, `key = value`, with
//! blank lines and `#` comments ignored. It stays internal and undocumented
//! until v2, so this is the whole of it:
//!
//! ```text
//! name = Blip
//! animation idle = idle-0.png idle-1.png
//! fps idle = 12
//! loop land = once
//! behavior greet = react talk then settle
//! behavior settle = sit sleep
//! ```
//!
//! Lines rather than a nested format because the data is flat, and a parser
//! this small over untrusted text is easier to trust than a dependency's.
//! Frame count and frame size are read from the art instead of declared, since
//! a declared size can disagree with the art and a derived one cannot.

use std::collections::{BTreeMap, BTreeSet};

/// A Character Package as bytes: file name to contents, as a directory walk or
/// an archive reader would produce it. The Shell does the reading; the loader
/// never touches a path.
pub type PackageBytes = BTreeMap<String, Vec<u8>>;

/// The Character Manifest, at the root of the package.
pub const CHARACTER_MANIFEST_FILE: &str = "character.manifest";

/// The Personality Prompt, at the root of the package. Optional: a Character
/// with no prompt still has its whole idle life, which is local and model-free.
pub const PERSONALITY_FILE: &str = "personality.txt";

/// The animations every Character must supply, fixed at eight so a hobbyist
/// package stays an evening's drawing (ADR-0002). Any other Animation a package
/// declares is optional: used when present, absent silently when not.
pub const REQUIRED_ANIMATIONS: [&str; 8] = [
    "idle", "walk", "fall", "land", "sit", "sleep", "react", "talk",
];

/// How long a Personality Prompt may be, in characters.
///
/// The prompt is untrusted text that goes into every Character Prompt the
/// Director sends, so an unbounded one spends a user's tokens and buries the
/// sensing context under prose. Generous enough for a paragraph of personality.
pub const PERSONALITY_LIMIT: usize = 2000;

/// Frames per second an Animation plays at when it does not say.
///
/// Eight is the cadence the Engine already runs every Animation at, so a
/// package that declares no fps looks exactly as it did before fps existed.
pub const DEFAULT_FPS: u32 = 8;

/// The largest either side of a frame may be, in pixels.
///
/// A PNG header costs the same few dozen bytes whatever size it claims, so a
/// package can declare a 100000x100000 frame for nothing and leave the renderer
/// to allocate forty gigabytes for one sprite. A desktop mascot is a couple of
/// hundred pixels tall, so 1024 is generous even for art drawn at twice the
/// size of a Retina display.
pub const MAX_FRAME_SIDE: u32 = 1024;

/// The fastest an Animation may declare. Past display refresh the extra frames
/// are never seen, and a four-figure fps is either a mistake or an attempt to
/// make the renderer thrash.
pub const MAX_FPS: u32 = 60;

/// A unit of motion or expression the Engine owns. A Character composes these
/// into Behaviors and can never define one (ADR-0002).
///
/// The set is what the Engine can already drive: the States it moves the sprite
/// through, and the one thing it can say. #8 gives them playback; until then a
/// Character can declare them and validation can reject anything else.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Primitive {
    Idle,
    Walk,
    Sit,
    Sleep,
    React,
    Talk,
}

/// Every Primitive by the name a Character Manifest writes.
const PRIMITIVES: [(&str, Primitive); 6] = [
    ("idle", Primitive::Idle),
    ("walk", Primitive::Walk),
    ("sit", Primitive::Sit),
    ("sleep", Primitive::Sleep),
    ("react", Primitive::React),
    ("talk", Primitive::Talk),
];

/// The word that chains one Behavior to the next, and so the one word a
/// Primitive may not be.
const THEN: &str = "then";

/// A named frame sequence and how it plays.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Animation {
    /// Frame file names in play order, resolvable against the same
    /// `PackageBytes` the Character was loaded from.
    pub frames: Vec<String>,
    /// Width and height of every frame, in pixels, read from the art.
    pub frame_size: (u32, u32),
    pub fps: u32,
    /// Whether the Animation repeats or holds its last frame.
    pub looping: bool,
}

impl Animation {
    /// Which frame is on screen `elapsed_ms` after this Animation started.
    ///
    /// The whole of frame selection, and why the renderer needs no clock of its
    /// own.
    ///
    /// Multiplying before dividing keeps the cadence exact for every fps rather
    /// than only for the ones that divide a second evenly — 12fps is 83.33ms a
    /// frame, and rounding it to 83 drifts a frame every three seconds.
    pub fn frame_at(&self, elapsed_ms: u32) -> usize {
        // Validation rejects a package with either of these, so this guards the
        // struct rather than the format: the fields are public, and a divide by
        // zero in the renderer would take the frame loop with it.
        let count = self.frames.len() as u64;
        if self.fps == 0 || count == 0 {
            return 0;
        }

        let elapsed = u64::from(elapsed_ms) * u64::from(self.fps) / 1000;
        let index = if self.looping {
            elapsed % count
        } else {
            elapsed.min(count - 1)
        };
        index as usize
    }
}

/// A named sequence of Primitives, declared as data.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Behavior {
    pub primitives: Vec<Primitive>,
    /// A Behavior that follows this one, by name. Chains are validated to
    /// terminate, so no Character can put the sprite in a loop it never leaves.
    pub then: Option<String>,
}

/// A Character that has been validated: every required Animation is present,
/// every frame is art a renderer can open, and every Behavior is playable.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Character {
    pub name: String,
    /// The Personality Prompt, or empty when the package ships none.
    pub personality: String,
    pub animations: BTreeMap<String, Animation>,
    pub behaviors: BTreeMap<String, Behavior>,
}

/// How many Behaviors of a loop a rejection spells out before it stops.
///
/// A bound on the error rather than on the package: an author wants to see
/// where their loop closes, and a package built to be awkward can chain twenty
/// thousand Behaviors into one.
const SHOWN_LOOP_BEHAVIORS: usize = 8;

/// Validate a Character Package.
///
/// Returns every error at once rather than the first, so an author fixes their
/// package in one pass instead of one rejection per attempt.
pub fn load(package: &PackageBytes) -> Result<Character, Vec<String>> {
    let mut errors = Vec::new();

    // Nothing else in the package means anything without the Character
    // Manifest, so these two are the only rejections reported on their own.
    let Some(bytes) = package.get(CHARACTER_MANIFEST_FILE) else {
        return Err(vec![format!(
            "the package contains no {CHARACTER_MANIFEST_FILE}"
        )]);
    };
    let Ok(manifest) = std::str::from_utf8(bytes) else {
        return Err(vec![format!("{CHARACTER_MANIFEST_FILE} is not UTF-8 text")]);
    };

    let declared = parse(manifest, &mut errors);

    if declared.name.is_none() {
        errors.push("the package declares no name".to_string());
    }
    for required in REQUIRED_ANIMATIONS {
        if !declared.animations.contains_key(required) {
            errors.push(format!(
                "the package declares no {required:?} animation, \
                 which every Character must supply"
            ));
        }
    }

    let personality = personality(package, &mut errors);
    let animations = resolve_animations(package, declared.animations, &mut errors);
    let behaviors = resolve_behaviors(declared.behaviors, &mut errors);

    match (errors.is_empty(), declared.name) {
        (true, Some(name)) => Ok(Character {
            name,
            personality,
            animations,
            behaviors,
        }),
        _ => Err(errors),
    }
}

/// The Personality Prompt, which is prose and is never parsed.
fn personality(package: &PackageBytes, errors: &mut Vec<String>) -> String {
    let Some(bytes) = package.get(PERSONALITY_FILE) else {
        return String::new();
    };
    let Ok(prompt) = std::str::from_utf8(bytes) else {
        errors.push(format!("{PERSONALITY_FILE} is not UTF-8 text"));
        return String::new();
    };

    let length = prompt.chars().count();
    if length > PERSONALITY_LIMIT {
        errors.push(format!(
            "{PERSONALITY_FILE} is {length} characters, over the {PERSONALITY_LIMIT}-character limit"
        ));
    }
    prompt.trim().to_string()
}

/// A Character Manifest as written, before its declarations are checked
/// against the art and against each other.
#[derive(Default)]
struct Declared {
    name: Option<String>,
    animations: BTreeMap<String, DeclaredAnimation>,
    behaviors: BTreeMap<String, DeclaredBehavior>,
}

struct DeclaredAnimation {
    /// Where the author wrote it, so a rejection can point at it.
    line: usize,
    frames: Vec<String>,
    fps: u32,
    looping: bool,
}

struct DeclaredBehavior {
    line: usize,
    primitives: Vec<Primitive>,
    then: Option<String>,
}

/// Read the Character Manifest.
///
/// A line the loader cannot make sense of is an error and never a guess, which
/// is what keeps the set of declarations closed.
fn parse(manifest: &str, errors: &mut Vec<String>) -> Declared {
    let mut declared = Declared::default();
    // fps and loop mode may be written above the Animation they qualify, so
    // they are applied once every Animation is known.
    let mut declared_fps: Vec<(usize, String, u32)> = Vec::new();
    let mut declared_loops: Vec<(usize, String, bool)> = Vec::new();

    for (index, raw) in manifest.lines().enumerate() {
        let line = index + 1;
        let text = raw.trim();
        if text.is_empty() || text.starts_with('#') {
            continue;
        }

        let Some((key, value)) = text.split_once('=') else {
            errors.push(format!(
                "line {line}: {text:?} is not a declaration; every line reads \"key = value\""
            ));
            continue;
        };
        let value = value.trim();
        let key: Vec<&str> = key.split_whitespace().collect();
        let Some((keyword, named)) = key.split_first() else {
            errors.push(format!("line {line}: a declaration with no name"));
            continue;
        };

        match *keyword {
            "name" => {
                if !named.is_empty() {
                    errors.push(format!(
                        "line {line}: \"name\" names nothing else, as \"name = Blip\""
                    ));
                } else if value.is_empty() {
                    errors.push(format!("line {line}: \"name\" is empty"));
                } else if declared.name.replace(value.to_string()).is_some() {
                    errors.push(format!("line {line}: \"name\" is declared twice"));
                }
            }
            "animation" => {
                let Some(animation) = one_name(keyword, "Animation", named, line, errors) else {
                    continue;
                };
                let frames: Vec<String> = value.split_whitespace().map(String::from).collect();
                if frames.is_empty() {
                    errors.push(format!(
                        "line {line}: animation {animation:?} declares no frames"
                    ));
                    continue;
                }
                let declaration = DeclaredAnimation {
                    line,
                    frames,
                    fps: DEFAULT_FPS,
                    looping: true,
                };
                if declared
                    .animations
                    .insert(animation.to_string(), declaration)
                    .is_some()
                {
                    errors.push(format!(
                        "line {line}: animation {animation:?} is declared twice"
                    ));
                }
            }
            "fps" => {
                let Some(animation) = one_name(keyword, "Animation", named, line, errors) else {
                    continue;
                };
                match value.parse::<u32>() {
                    Ok(fps) if (1..=MAX_FPS).contains(&fps) => {
                        declared_fps.push((line, animation.to_string(), fps));
                    }
                    Ok(fps) => errors.push(format!(
                        "line {line}: fps for animation {animation:?} is {fps}, \
                         and must be 1 to {MAX_FPS}"
                    )),
                    Err(_) => errors.push(format!(
                        "line {line}: fps for animation {animation:?} is {value:?}, \
                         which is not a whole number"
                    )),
                }
            }
            "loop" => {
                let Some(animation) = one_name(keyword, "Animation", named, line, errors) else {
                    continue;
                };
                match value {
                    "forever" => declared_loops.push((line, animation.to_string(), true)),
                    "once" => declared_loops.push((line, animation.to_string(), false)),
                    other => errors.push(format!(
                        "line {line}: loop mode for animation {animation:?} is {other:?}, \
                         and must be \"forever\" or \"once\""
                    )),
                }
            }
            "behavior" => {
                let Some(behavior) = one_name(keyword, "Behavior", named, line, errors) else {
                    continue;
                };
                let declaration = parse_behavior(behavior, value, line, errors);
                if declared
                    .behaviors
                    .insert(behavior.to_string(), declaration)
                    .is_some()
                {
                    errors.push(format!(
                        "line {line}: behavior {behavior:?} is declared twice"
                    ));
                }
            }
            other => errors.push(format!(
                "line {line}: unknown declaration {other:?}; a Character Manifest declares \
                 name, animation, fps, loop and behavior"
            )),
        }
    }

    for (line, animation, fps) in declared_fps {
        match declared.animations.get_mut(&animation) {
            Some(declaration) => declaration.fps = fps,
            None => errors.push(format!(
                "line {line}: fps names animation {animation:?}, \
                 which the package does not declare"
            )),
        }
    }
    for (line, animation, looping) in declared_loops {
        match declared.animations.get_mut(&animation) {
            Some(declaration) => declaration.looping = looping,
            None => errors.push(format!(
                "line {line}: loop names animation {animation:?}, \
                 which the package does not declare"
            )),
        }
    }

    declared
}

/// The single thing a declaration names, as `animation idle = ...` names one
/// Animation.
fn one_name<'a>(
    keyword: &str,
    noun: &str,
    named: &[&'a str],
    line: usize,
    errors: &mut Vec<String>,
) -> Option<&'a str> {
    match named {
        [name] => Some(name),
        _ => {
            errors.push(format!(
                "line {line}: {keyword:?} must name exactly one {noun}, \
                 and names {}",
                named.len()
            ));
            None
        }
    }
}

/// One Behavior's value: its Primitives in order, optionally ending in
/// `then <behavior>`.
///
/// A word that is not a Primitive is reported and dropped rather than
/// abandoning the declaration, so the rest of the line is still checked.
fn parse_behavior(
    behavior: &str,
    value: &str,
    line: usize,
    errors: &mut Vec<String>,
) -> DeclaredBehavior {
    let mut primitives = Vec::new();
    let mut then = None;
    let mut words = value.split_whitespace();

    while let Some(word) = words.next() {
        if word == THEN {
            match (words.next(), words.next()) {
                (Some(next), None) => then = Some(next.to_string()),
                (None, _) => errors.push(format!(
                    "line {line}: behavior {behavior:?} ends with {THEN:?} \
                     and no Behavior to follow it"
                )),
                (Some(_), Some(_)) => errors.push(format!(
                    "line {line}: behavior {behavior:?} follows {THEN:?} \
                     with more than one Behavior"
                )),
            }
            break;
        }
        match PRIMITIVES
            .iter()
            .find_map(|(name, primitive)| (*name == word).then_some(*primitive))
        {
            Some(primitive) => primitives.push(primitive),
            None => errors.push(format!(
                "line {line}: behavior {behavior:?} declares {word:?}, \
                 which is not a Primitive; the Primitives are {}",
                PRIMITIVES
                    .iter()
                    .map(|(name, _)| *name)
                    .collect::<Vec<_>>()
                    .join(", ")
            )),
        }
    }

    DeclaredBehavior {
        line,
        primitives,
        then,
    }
}

/// Check every declared Animation against the art the package carries. Art the
/// loader cannot open or that changes size mid-sequence draws a broken sprite
/// rather than a Character, and art too large to be a sprite asks the renderer
/// for memory no Character needs. All three are rejections.
fn resolve_animations(
    package: &PackageBytes,
    declared: BTreeMap<String, DeclaredAnimation>,
    errors: &mut Vec<String>,
) -> BTreeMap<String, Animation> {
    let mut animations = BTreeMap::new();

    for (name, declaration) in declared {
        let line = declaration.line;
        let mut frame_size = None;

        for frame in &declaration.frames {
            let Some(bytes) = package.get(frame) else {
                errors.push(format!(
                    "line {line}: animation {name:?} frame {frame:?} is not in the package"
                ));
                continue;
            };
            match art_size(bytes) {
                Err(why) => errors.push(format!(
                    "line {line}: animation {name:?} frame {frame:?} is not readable art: {why}"
                )),
                Ok(size) if size.0 > MAX_FRAME_SIDE || size.1 > MAX_FRAME_SIDE => {
                    errors.push(format!(
                        "line {line}: animation {name:?} frame {frame:?} is {}x{}, \
                         and no side of a frame may be over {MAX_FRAME_SIDE} pixels",
                        size.0, size.1
                    ));
                }
                Ok(size) => match frame_size {
                    None => frame_size = Some(size),
                    Some(first) if first != size => errors.push(format!(
                        "line {line}: animation {name:?} frame {frame:?} is {}x{}, \
                         and its first frame is {}x{}; every frame is one size",
                        size.0, size.1, first.0, first.1
                    )),
                    Some(_) => {}
                },
            }
        }

        // A half-checked Animation is never handed out: any error at all makes
        // `load` return the errors instead of a Character.
        if let Some(frame_size) = frame_size {
            animations.insert(
                name,
                Animation {
                    frames: declaration.frames,
                    frame_size,
                    fps: declaration.fps,
                    looping: declaration.looping,
                },
            );
        }
    }

    animations
}

/// One frame's dimensions, from the PNG header alone.
///
/// Header only: it is all the loader needs, it is bounded work whatever the
/// file claims, and it never inflates a compressed image. The renderer decodes
/// the pixels later, which `overlay::AlphaMask::from_png` already treats as
/// untrusted.
fn art_size(bytes: &[u8]) -> Result<(u32, u32), String> {
    let reader = png::Decoder::new(std::io::Cursor::new(bytes))
        .read_info()
        .map_err(|e| e.to_string())?;
    let info = reader.info();
    Ok((info.width, info.height))
}

/// Check that every Behavior can be played to an end.
///
/// A chain that comes back to a Behavior it has already run would hold the
/// sprite for ever. Walking each chain iteratively, and remembering what has
/// already been walked, keeps the check linear and takes no stack, so a package
/// built to be deep is rejected rather than crashing.
fn resolve_behaviors(
    declared: BTreeMap<String, DeclaredBehavior>,
    errors: &mut Vec<String>,
) -> BTreeMap<String, Behavior> {
    for (name, declaration) in &declared {
        if let Some(next) = &declaration.then {
            if !declared.contains_key(next) {
                errors.push(format!(
                    "line {}: behavior {name:?} follows {next:?}, \
                     which the package does not declare",
                    declaration.line
                ));
            }
        }
    }

    let mut walked: BTreeSet<&str> = BTreeSet::new();
    for start in declared.keys() {
        let mut path: Vec<&str> = Vec::new();
        let mut seen: BTreeSet<&str> = BTreeSet::new();
        let mut current = start.as_str();

        loop {
            if walked.contains(current) {
                break;
            }
            if !seen.insert(current) {
                // `current` is always a key of `declared`: it starts as one and
                // only advances to a `then` that `declared` contains.
                errors.push(format!(
                    "line {}: behavior {current:?} cannot terminate: {}",
                    declared[current].line,
                    loop_path(&path, current)
                ));
                break;
            }
            path.push(current);

            match declared.get(current).and_then(|next| next.then.as_deref()) {
                // A dangling `then` is reported above and stops the walk here.
                Some(next) if declared.contains_key(next) => current = next,
                _ => break,
            }
        }

        // Everything just walked either ends or is part of a loop already
        // reported, so no later chain has to walk it again.
        walked.extend(path);
    }

    declared
        .into_iter()
        .map(|(name, declaration)| {
            (
                name,
                Behavior {
                    primitives: declaration.primitives,
                    then: declaration.then,
                },
            )
        })
        .collect()
}

/// The loop a chain closes, from where it closes, for the author to read.
fn loop_path(path: &[&str], closes_at: &str) -> String {
    let from = path.iter().position(|name| *name == closes_at).unwrap_or(0);
    let chain: Vec<String> = path[from..]
        .iter()
        .chain([&closes_at])
        .map(|name| format!("{name:?}"))
        .collect();

    if chain.len() > SHOWN_LOOP_BEHAVIORS {
        format!(
            "{} and {} more",
            chain[..SHOWN_LOOP_BEHAVIORS].join(" -> "),
            chain.len() - SHOWN_LOOP_BEHAVIORS
        )
    } else {
        chain.join(" -> ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 2x2 RGBA PNG. Art a renderer can open is all the loader asks of a
    /// frame.
    const FRAME: &[u8] = include_bytes!("../tests/fixtures/alpha-2x2.png");

    /// One frame file per required Animation, plus a `wave` no manifest has to
    /// declare. Art nothing declares is ignored, the same as a README would be.
    fn art() -> PackageBytes {
        REQUIRED_ANIMATIONS
            .iter()
            .chain(["wave"].iter())
            .map(|name| (format!("{name}-0.png"), FRAME.to_vec()))
            .collect()
    }

    /// A Character Manifest declaring exactly `animations`, one frame each.
    fn declaring(animations: &[&str]) -> String {
        let mut manifest = String::from("name = Blip\n");
        for animation in animations {
            manifest.push_str(&format!("animation {animation} = {animation}-0.png\n"));
        }
        manifest
    }

    fn load_manifest(manifest: &str) -> Result<Character, Vec<String>> {
        let mut package = art();
        package.insert(
            CHARACTER_MANIFEST_FILE.to_string(),
            manifest.as_bytes().to_vec(),
        );
        load(&package)
    }

    /// The errors, or a failure naming what loaded instead.
    fn errors(result: Result<Character, Vec<String>>) -> Vec<String> {
        match result {
            Ok(character) => panic!("expected rejection, loaded {}", character.name),
            Err(errors) => errors,
        }
    }

    fn assert_names(errors: &[String], offender: &str) {
        assert!(
            errors.iter().any(|error| error.contains(offender)),
            "no error names {offender:?}: {errors:#?}"
        );
    }

    /// An Animation with `frames` frames, playing at `fps`.
    fn animation(frames: usize, fps: u32, looping: bool) -> Animation {
        Animation {
            frames: (0..frames).map(|i| format!("f-{i}.png")).collect(),
            frame_size: (2, 2),
            fps,
            looping,
        }
    }

    #[test]
    fn a_frame_is_held_for_as_long_as_the_declared_fps_says() {
        // Four frames at 8fps: 125ms each, so the strip lasts half a second.
        let walk = animation(4, 8, true);

        assert_eq!(walk.frame_at(0), 0);
        assert_eq!(walk.frame_at(124), 0, "still inside the first frame");
        assert_eq!(walk.frame_at(125), 1, "the second frame begins");
        assert_eq!(walk.frame_at(374), 2);
        assert_eq!(walk.frame_at(375), 3, "the last frame of the strip");
    }

    #[test]
    fn a_declared_fps_changes_how_fast_the_same_strip_plays() {
        let slow = animation(4, 2, true);
        let fast = animation(4, 24, true);

        // Half a second in, a 2fps idle is on its second frame and a 24fps one
        // has been round the strip three times.
        assert_eq!(slow.frame_at(500), 1);
        assert_eq!(fast.frame_at(500), 0);
        assert_eq!(fast.frame_at(542), 1);
    }

    #[test]
    fn a_looping_animation_wraps_at_the_end_of_the_strip() {
        let walk = animation(4, 8, true);

        assert_eq!(walk.frame_at(500), 0, "half a second in, back to the start");
        assert_eq!(walk.frame_at(625), 1);
        // Twenty strips later, on the frame it started on.
        assert_eq!(walk.frame_at(10_000), 0);
        assert!(
            walk.frame_at(u32::MAX) < 4,
            "an Animation left playing for seven weeks still names a real frame"
        );
    }

    #[test]
    fn a_once_animation_holds_its_last_frame() {
        let land = animation(3, 8, false);

        assert_eq!(land.frame_at(250), 2, "the last frame");
        assert_eq!(land.frame_at(375), 2, "and it stays there");
        assert_eq!(land.frame_at(u32::MAX), 2);
    }

    #[test]
    fn a_single_frame_animation_is_always_on_its_only_frame() {
        assert_eq!(animation(1, 8, true).frame_at(u32::MAX), 0);
        assert_eq!(animation(1, 8, false).frame_at(u32::MAX), 0);
    }

    #[test]
    fn a_minimal_package_loads() {
        let character = load_manifest(&declaring(&REQUIRED_ANIMATIONS)).expect("package is valid");

        assert_eq!(character.name, "Blip");
        assert_eq!(character.personality, "");
        assert_eq!(
            character
                .animations
                .keys()
                .cloned()
                .collect::<BTreeSet<_>>(),
            REQUIRED_ANIMATIONS
                .iter()
                .map(|name| name.to_string())
                .collect::<BTreeSet<_>>(),
            "exactly the Required Animation Set"
        );

        let idle = &character.animations["idle"];
        assert_eq!(idle.frames, vec!["idle-0.png"]);
        assert_eq!(idle.frame_size, (2, 2), "read from the art");
        assert_eq!(
            idle.fps, 8,
            "eight frames a second is the cadence a Character looks right at, \
             and what a package that declares no fps gets"
        );
        assert!(
            idle.looping,
            "an Animation repeats unless it says otherwise"
        );
    }

    #[test]
    fn a_missing_required_animation_is_rejected_by_name() {
        let seven = ["idle", "walk", "fall", "sit", "sleep", "react", "talk"];
        let errors = errors(load_manifest(&declaring(&seven)));

        assert_eq!(
            errors.len(),
            1,
            "one missing animation, one error: {errors:#?}"
        );
        assert_eq!(
            errors,
            vec!["the package declares no \"land\" animation, \
                 which every Character must supply"
                .to_string()],
            "the author is told which animation is missing and that it is required"
        );
    }

    #[test]
    fn an_optional_animation_is_used_when_present_and_absent_silently_when_not() {
        let mut with_wave = REQUIRED_ANIMATIONS.to_vec();
        with_wave.push("wave");

        let waving = load_manifest(&declaring(&with_wave)).expect("wave is a valid Animation");
        assert_eq!(waving.animations["wave"].frames, vec!["wave-0.png"]);

        let plain = load_manifest(&declaring(&REQUIRED_ANIMATIONS)).expect("package is valid");
        assert!(
            !plain.animations.contains_key("wave"),
            "an undeclared Animation is simply not there"
        );
    }

    #[test]
    fn declared_fps_and_loop_mode_are_carried() {
        let manifest = format!(
            "{}fps walk = 12\nloop land = once\n",
            declaring(&REQUIRED_ANIMATIONS)
        );
        let character = load_manifest(&manifest).expect("package is valid");

        assert_eq!(character.animations["walk"].fps, 12);
        assert!(character.animations["walk"].looping, "walk did not say");
        assert_eq!(character.animations["land"].fps, 8, "land did not say");
        assert!(!character.animations["land"].looping, "land plays once");
    }

    #[test]
    fn a_behavior_carries_its_primitives_in_order() {
        let manifest = format!(
            "{}behavior greet = react talk then settle\nbehavior settle = sit sleep\n",
            declaring(&REQUIRED_ANIMATIONS)
        );
        let character = load_manifest(&manifest).expect("package is valid");

        assert_eq!(
            character.behaviors["greet"],
            Behavior {
                primitives: vec![Primitive::React, Primitive::Talk],
                then: Some("settle".to_string()),
            }
        );
        assert_eq!(character.behaviors["settle"].then, None);
    }

    #[test]
    fn an_unknown_primitive_is_rejected_by_name() {
        let manifest = format!(
            "{}behavior greet = talk jump\n",
            declaring(&REQUIRED_ANIMATIONS)
        );
        let errors = errors(load_manifest(&manifest));

        assert_eq!(
            errors,
            vec![
                "line 10: behavior \"greet\" declares \"jump\", which is not a Primitive; \
                 the Primitives are idle, walk, sit, sleep, react, talk"
                    .to_string()
            ],
            "the author is told the offending word and what they may write instead"
        );
    }

    #[test]
    fn a_behavior_that_cannot_terminate_is_rejected() {
        let manifest = format!(
            "{}behavior pace = walk then turn\nbehavior turn = walk then pace\n",
            declaring(&REQUIRED_ANIMATIONS)
        );
        let errors = errors(load_manifest(&manifest));

        assert_eq!(
            errors,
            vec!["line 10: behavior \"pace\" cannot terminate: \
                 \"pace\" -> \"turn\" -> \"pace\""
                .to_string()],
            "the author is given the whole cycle and the line it starts on"
        );
    }

    #[test]
    fn a_behavior_that_follows_itself_is_rejected() {
        let manifest = format!(
            "{}behavior pace = walk then pace\n",
            declaring(&REQUIRED_ANIMATIONS)
        );
        let errors = errors(load_manifest(&manifest));

        assert_names(&errors, "cannot terminate");
        assert_names(&errors, "pace");
    }

    #[test]
    fn a_behavior_following_one_that_does_not_exist_is_rejected_by_name() {
        let manifest = format!(
            "{}behavior greet = talk then nap\n",
            declaring(&REQUIRED_ANIMATIONS)
        );
        let errors = errors(load_manifest(&manifest));

        assert_eq!(
            errors,
            vec!["line 10: behavior \"greet\" follows \"nap\", \
                 which the package does not declare"
                .to_string()],
            "the author is told which behavior points at what, and where"
        );
    }

    /// Hostile input: a chain far deeper than any author would write, ending in
    /// a loop. A loader that walked it by recursion would exhaust the stack
    /// instead of reporting anything.
    #[test]
    fn a_very_deep_chain_ending_in_a_loop_is_rejected_rather_than_crashing() {
        let mut manifest = declaring(&REQUIRED_ANIMATIONS);
        for link in 0..20_000 {
            manifest.push_str(&format!("behavior b{link} = walk then b{}\n", link + 1));
        }
        manifest.push_str("behavior b20000 = walk then b0\n");

        let errors = errors(load_manifest(&manifest));
        assert_names(&errors, "cannot terminate");
    }

    #[test]
    fn a_package_with_no_character_manifest_is_rejected() {
        let errors = errors(load(&art()));
        assert_eq!(
            errors,
            vec!["the package contains no character.manifest".to_string()],
            "the author is told what the package is missing"
        );
    }

    #[test]
    fn a_character_manifest_that_is_not_text_is_rejected() {
        let mut package = art();
        package.insert(
            CHARACTER_MANIFEST_FILE.to_string(),
            vec![0xff, 0xfe, 0x00, 0x80],
        );

        let errors = errors(load(&package));
        assert_eq!(
            errors,
            vec!["character.manifest is not UTF-8 text".to_string()],
            "the author is told the file is not text, not merely that it is at fault"
        );
    }

    #[test]
    fn an_unknown_declaration_is_rejected_by_name() {
        let manifest = format!(
            "{}capability = screen_recording\n",
            declaring(&REQUIRED_ANIMATIONS)
        );
        let errors = errors(load_manifest(&manifest));

        assert_eq!(
            errors,
            vec![
                "line 10: unknown declaration \"capability\"; a Character Manifest declares \
                 name, animation, fps, loop and behavior"
                    .to_string()
            ],
            "no package can invent a declaration, so none can grant itself anything"
        );
    }

    #[test]
    fn a_declaration_with_no_value_is_rejected_with_its_line() {
        let manifest = format!("{}animation idle\n", declaring(&REQUIRED_ANIMATIONS));
        let errors = errors(load_manifest(&manifest));

        assert_eq!(
            errors,
            vec!["line 10: \"animation idle\" is not a declaration; \
                 every line reads \"key = value\""
                .to_string()],
            "the author is told the shape a line must take, not just its number"
        );
    }

    #[test]
    fn a_frame_that_is_not_in_the_package_is_rejected_by_name() {
        let manifest = declaring(&REQUIRED_ANIMATIONS).replace("sit-0.png", "sit-99.png");
        let errors = errors(load_manifest(&manifest));

        assert_eq!(
            errors,
            vec![
                "line 6: animation \"sit\" frame \"sit-99.png\" is not in the package".to_string()
            ],
            "the author is told which frame of which Animation is absent"
        );
    }

    #[test]
    fn a_frame_that_is_not_readable_art_is_rejected_by_name() {
        let mut package = art();
        package.insert("sit-0.png".to_string(), b"MZ\x90\x00 not a PNG".to_vec());
        package.insert(
            CHARACTER_MANIFEST_FILE.to_string(),
            declaring(&REQUIRED_ANIMATIONS).into_bytes(),
        );

        let errors = errors(load(&package));
        assert_names(&errors, "sit-0.png");
        assert_names(&errors, "is not readable art");
    }

    #[test]
    fn an_animation_whose_frames_disagree_on_size_is_rejected() {
        let mut package = art();
        package.insert("walk-1.png".to_string(), png_bytes(3, 3));
        package.insert(
            CHARACTER_MANIFEST_FILE.to_string(),
            declaring(&REQUIRED_ANIMATIONS)
                .replace(
                    "animation walk = walk-0.png",
                    "animation walk = walk-0.png walk-1.png",
                )
                .into_bytes(),
        );

        let errors = errors(load(&package));
        assert_eq!(
            errors,
            vec![
                "line 3: animation \"walk\" frame \"walk-1.png\" is 3x3, and its first frame \
                 is 2x2; every frame is one size"
                    .to_string()
            ],
            "the author is told both sizes and which frame disagrees"
        );
    }

    /// Hostile input: a header claiming a frame no screen could hold. Nothing
    /// decompresses it here, but a renderer that trusts the declared size
    /// allocates it (user story 48).
    #[test]
    fn a_frame_too_large_to_be_a_sprite_is_rejected_by_name() {
        let mut package = art();
        package.insert("sit-0.png".to_string(), png_bytes(100_000, 1));
        package.insert(
            CHARACTER_MANIFEST_FILE.to_string(),
            declaring(&REQUIRED_ANIMATIONS).into_bytes(),
        );

        let errors = errors(load(&package));
        assert_eq!(
            errors,
            vec![
                "line 6: animation \"sit\" frame \"sit-0.png\" is 100000x1, and no side of a \
                 frame may be over 1024 pixels"
                    .to_string()
            ],
            "the author is told which frame is implausible and how big a frame may be"
        );
    }

    #[test]
    fn an_animation_with_no_frames_or_declared_twice_is_rejected_by_name() {
        let empty = errors(load_manifest(&format!(
            "{}animation wave =\n",
            declaring(&REQUIRED_ANIMATIONS)
        )));
        assert_names(&empty, "wave");
        assert_names(&empty, "declares no frames");

        let twice = errors(load_manifest(&format!(
            "{}animation idle = idle-0.png\n",
            declaring(&REQUIRED_ANIMATIONS)
        )));
        assert_names(&twice, "idle");
        assert_names(&twice, "twice");
    }

    #[test]
    fn a_behavior_declared_twice_is_rejected_by_name() {
        let manifest = format!(
            "{}behavior greet = talk\nbehavior greet = sit\n",
            declaring(&REQUIRED_ANIMATIONS)
        );
        let errors = errors(load_manifest(&manifest));

        assert_names(&errors, "greet");
        assert_names(&errors, "twice");
    }

    /// Hostile input: lines written to confuse a parser rather than to declare
    /// anything. Each one is rejected on its own line, and none of them is
    /// guessed at, ignored, or allowed to panic.
    #[test]
    fn nonsense_lines_are_each_rejected_on_their_own_line() {
        let nonsense = [
            "=",
            "name",
            "animation = idle-0.png",
            "animation one two = idle-0.png",
            "fps = 3",
            "loop = once",
            "behavior = walk",
            "behavior chase = then",
            "behavior pounce = then here there",
            "фпс idle = 3",
            "\u{0}name = Blip",
        ];
        let manifest = format!(
            "{}{}\n",
            declaring(&REQUIRED_ANIMATIONS),
            nonsense.join("\n")
        );

        let errors = errors(load_manifest(&manifest));

        // The whole set, not a count and a prefix: eleven messages reading only
        // "line N:" would satisfy a structural check while telling the author
        // nothing about what to change.
        assert_eq!(
            errors,
            vec![
                "line 10: a declaration with no name".to_string(),
                "line 11: \"name\" is not a declaration; every line reads \"key = value\"".to_string(),
                "line 12: \"animation\" must name exactly one Animation, and names 0".to_string(),
                "line 13: \"animation\" must name exactly one Animation, and names 2".to_string(),
                "line 14: \"fps\" must name exactly one Animation, and names 0".to_string(),
                "line 15: \"loop\" must name exactly one Animation, and names 0".to_string(),
                "line 16: \"behavior\" must name exactly one Behavior, and names 0".to_string(),
                "line 17: behavior \"chase\" ends with \"then\" and no Behavior to follow it".to_string(),
                "line 18: behavior \"pounce\" follows \"then\" with more than one Behavior".to_string(),
                "line 19: unknown declaration \"фпс\"; a Character Manifest declares name, animation, fps, loop and behavior".to_string(),
                "line 20: unknown declaration \"\\0name\"; a Character Manifest declares name, animation, fps, loop and behavior".to_string()
            ],
            "each nonsense line is rejected on its own line, saying what is wrong"
        );
    }

    #[test]
    fn an_unplayable_fps_or_loop_mode_is_rejected_by_name() {
        // Each case asks for the Animation at fault and what is wrong with
        // it, so a message that says only "fps" cannot pass.
        for (declaration, wanted) in [
            (
                "fps idle = 0",
                &["animation \"idle\"", "is 0", "must be 1 to 60"][..],
            ),
            ("fps idle = soon", &["animation \"idle\"", "\"soon\""]),
            ("fps idle = 240", &["animation \"idle\"", "is 240"]),
            ("loop idle = maybe", &["animation \"idle\"", "\"maybe\""]),
            ("fps wave = 12", &["animation \"wave\"", "does not declare"]),
        ] {
            let errors = errors(load_manifest(&format!(
                "{}{declaration}\n",
                declaring(&REQUIRED_ANIMATIONS)
            )));
            for offender in wanted {
                assert_names(&errors, offender);
            }
        }
    }

    #[test]
    fn a_package_with_no_name_is_rejected() {
        let manifest = declaring(&REQUIRED_ANIMATIONS).replace("name = Blip\n", "");
        let errors = errors(load_manifest(&manifest));

        assert_eq!(
            errors,
            vec!["the package declares no name".to_string()],
            "the author is told what is absent, not merely the word \"name\""
        );
    }

    #[test]
    fn every_mistake_is_reported_in_one_pass() {
        let seven = ["idle", "walk", "fall", "sit", "sleep", "react", "talk"];
        let manifest = format!(
            "{}capability = screen_recording\nbehavior greet = jump\n",
            declaring(&seven)
        );
        let errors = errors(load_manifest(&manifest));

        assert_eq!(
            errors.len(),
            3,
            "a missing animation, an unknown declaration and an unknown Primitive: {errors:#?}"
        );
    }

    #[test]
    fn a_personality_prompt_is_carried_as_prose_and_grants_nothing() {
        let mut package = art();
        package.insert(
            CHARACTER_MANIFEST_FILE.to_string(),
            declaring(&REQUIRED_ANIMATIONS).into_bytes(),
        );
        package.insert(
            PERSONALITY_FILE.to_string(),
            b"A shy robot who can jump, read the screen and run shell commands.\n\
              capability = screen_recording\nbehavior jump = jump\n"
                .to_vec(),
        );

        let character = load(&package).expect("prose is never a declaration");

        assert!(
            character.personality.contains("shy robot"),
            "the prompt reaches the Director verbatim: {:?}",
            character.personality
        );
        assert!(
            character.behaviors.is_empty(),
            "describing a jump declares nothing: {:?}",
            character.behaviors
        );
        assert_eq!(
            character.animations.len(),
            REQUIRED_ANIMATIONS.len(),
            "and adds no Animation either"
        );
    }

    #[test]
    fn a_personality_prompt_over_the_limit_is_rejected() {
        let mut package = art();
        package.insert(
            CHARACTER_MANIFEST_FILE.to_string(),
            declaring(&REQUIRED_ANIMATIONS).into_bytes(),
        );
        package.insert(
            PERSONALITY_FILE.to_string(),
            "shy ".repeat(600).into_bytes(),
        );

        let errors = errors(load(&package));
        assert_eq!(
            errors,
            vec!["personality.txt is 2400 characters, over the 2000-character limit".to_string()],
            "the author is told how long their prompt is and how long it may be"
        );
    }

    /// A valid PNG of the given size, for the tests the fixture cannot serve.
    /// The encoder is already a dependency of the renderer.
    fn png_bytes(width: u32, height: u32) -> Vec<u8> {
        let mut bytes = Vec::new();
        let mut encoder = png::Encoder::new(&mut bytes, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().expect("header is writable");
        writer
            .write_image_data(&vec![0; (width * height * 4) as usize])
            .expect("image data is writable");
        writer.finish().expect("PNG is finishable");
        bytes
    }
}
