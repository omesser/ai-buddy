//! The Character Package loader: bytes in, either a validated Character or the
//! list of mistakes its author has to fix.
//!
//! The one validator on the load path. A loaded Character carries everything
//! rendering needs — each distinct frame's PNG and its alpha mask, decoded
//! here — so nothing downstream reopens the art, and nothing downstream can
//! refuse a Character this module declared valid. Every content bound lives in
//! the constants below; the reader's own limits (bytes, files, depth) guard
//! the I/O before these bytes exist and live with it in the Shell.
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
//! declaration at fault, because the author is meant to fix their package
//! without reading this file.
//!
//! Two properties keep a package from enabling anything. The Character Manifest
//! rejects every declaration it does not know, so no package can invent a key
//! that grants a capability. And the Personality Prompt is prose in a file of
//! its own, never a declaration, so it can describe a Character that jumps
//! without the Character gaining a jump.
//!
//! The Character Manifest is TOML (ADR-0008): a name, a table per Animation,
//! a table per Behavior, and an optional `[director]` for how proactive model
//! calls space themselves. TOML replaces only the container — the `when`
//! condition is still this module's own small language, checked here. It stays
//! internal and undocumented until v2, so this is the whole of it:
//!
//! ```text
//! name = "Blip"
//!
//! [animations.idle]
//! frames = ["idle-0.png", "idle-1.png"]
//! fps = 12
//!
//! [animations.land]
//! frames = ["land-0.png"]
//! loop = "once"
//!
//! [behaviors.greet]
//! play = ["react", "talk"]
//! then = "settle"
//! weight = 3
//! when = "idle over 2m"
//! ```
//!
//! Frame count and frame size are read from the art instead of declared, since
//! a declared size can disagree with the art and a derived one cannot.

use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;

use toml_edit::{Document, Item};

use crate::overlay::AlphaMask;

/// A Character Package as bytes: file name to contents, as a directory walk or
/// an archive reader would produce it. The Shell does the reading; the loader
/// never touches a path.
pub type PackageBytes = BTreeMap<String, Vec<u8>>;

/// The Character Manifest, at the root of the package.
pub const CHARACTER_MANIFEST_FILE: &str = "character.manifest";

/// The Personality Prompt, at the root of the package. Optional: a Character
/// with no prompt still has its whole idle life, which is local and model-free.
pub const PERSONALITY_FILE: &str = "personality.txt";

/// The animations every Character must supply, fixed at nine so a hobbyist
/// package stays an evening's drawing (ADR-0002, #98). Any other Animation a
/// package declares is optional: used when present, absent silently when not.
pub const REQUIRED_ANIMATIONS: [&str; 9] = [
    "idle", "walk", "fall", "land", "sit", "sleep", "react", "talk", "hold",
];

/// How long a Personality Prompt may be, in characters.
///
/// The prompt is untrusted text that goes into every Character Prompt the
/// Director sends, so an unbounded one spends a user's tokens and buries the
/// sensing context under prose. Generous enough for a paragraph of personality.
pub const PERSONALITY_LIMIT: usize = 2000;

/// How large a Character Manifest may be, in bytes.
///
/// A bound rather than a preference, and the same kind of bound as
/// `MAX_FRAME_SIDE`: every declaration the loader reads costs an error String
/// when it is malformed, so a manifest of junk lines that compresses to
/// kilobytes in the archive spends gigabytes being rejected. Generous for a
/// file that is one short line per Animation and Behavior.
pub const MANIFEST_LIMIT: usize = 1024 * 1024;

/// Frames per second an Animation plays at when it does not say.
///
/// Eight is the cadence the Engine already runs every Animation at, so a
/// package that declares no fps looks exactly as it did before fps existed.
pub const DEFAULT_FPS: u32 = 8;

/// How likely a Behavior is to be picked when it does not say.
///
/// One rather than zero, so that a Character written before weights existed
/// still has every Behavior in the running, all of them equally.
pub const DEFAULT_WEIGHT: u32 = 1;

/// The scale the renderer uses when a Character does not say.
///
/// Four is what the shipped pixel-art Characters have always been drawn at;
/// a package written before `scale` existed renders exactly as it did.
pub const DEFAULT_SCALE: u32 = 4;

/// How proactive model-call waits grow when no one addresses the buddy.
///
/// `wait * model_base.pow(model_power)` after each proactive model call.
/// Two and one is the doubling Pace already had, so a package written
/// before `[director]` existed backs off exactly as it did.
pub const DEFAULT_MODEL_BASE: u32 = 2;
pub const DEFAULT_MODEL_POWER: u32 = 1;

/// The largest integer factor a Character may ask to be drawn at.
///
/// ADR-0006 constrains display scaling to small integer factors; art wanting
/// to be bigger on screen should be authored bigger instead.
pub const MAX_SCALE: u32 = 4;

/// How long the renderer holds each member of a variant ring, at least.
///
/// Rounded up to whole loops of the member playing, so the ring never cuts an
/// Animation mid-stride. A few seconds reads as a mood; loop-for-loop
/// alternation reads as a costume glitch.
pub const VARIANT_HOLD_MS: u32 = 4000;

/// The largest either side of a frame may be, in pixels.
///
/// A PNG header costs the same few dozen bytes whatever size it claims, so a
/// package can declare a 100000x100000 frame for nothing and leave the renderer
/// to allocate forty gigabytes for one sprite. A desktop mascot is a couple of
/// hundred pixels tall, so 1024 is generous even for art drawn at twice the
/// size of a Retina display.
pub const MAX_FRAME_SIDE: u32 = 1024;

/// The most frames an Animation may declare.
///
/// The bound `MAX_FRAME_SIDE` is missing half of: a frame reference costs eight
/// bytes of manifest and buys a whole copy of the art in the renderer, so a
/// manifest that fits the package budget can still name one frame often enough
/// to ask for terabytes. A hand-drawn Animation is a handful of frames.
pub const MAX_FRAMES: usize = 256;

/// The most pixels all of a Character's distinct frames may add up to.
///
/// The half `MAX_FRAMES` is still missing: it bounds one Animation, and a
/// Character may declare as many Animations as it likes. Every distinct frame
/// buys a whole alpha mask, a byte per pixel held for as long as the Character
/// is loaded, so four thousand full-size frames are twenty-five megabytes of
/// package and four gigabytes of mask. This is 256 frames at the largest size a
/// frame may be.
pub const MAX_CHARACTER_PIXELS: u64 = 256 * (MAX_FRAME_SIDE as u64) * (MAX_FRAME_SIDE as u64);

/// The fastest an Animation may declare. Past display refresh the extra frames
/// are never seen, and a four-figure fps is either a mistake or an attempt to
/// make the renderer thrash.
pub const MAX_FPS: u32 = 60;

/// Alpha at or above this counts as drawn, when a frame's mask is built.
///
/// A threshold rather than "alpha > 0" so anti-aliased edges on hand-drawn art
/// do not grow an invisible one-pixel border that swallows clicks.
pub const ALPHA_THRESHOLD: u8 = 128;

/// A unit of motion or expression the Engine owns. A Character composes these
/// into Behaviors and can never define one (ADR-0002).
///
/// The set is what the Engine can already drive: the States it moves the sprite
/// through, the moment a fall ends, and the one thing it can say. Anything a
/// Character needs beyond them is a Primitive added here for everyone, never a
/// scripting runtime handed to a package (ADR-0002).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Primitive {
    Idle,
    Walk,
    Land,
    Sit,
    Sleep,
    React,
    Talk,
    /// Gripping a moving Perch. The Engine plays this itself (#98); a
    /// Character may also compose it.
    Hold,
}

/// Every Primitive by the name a Character Manifest writes.
const PRIMITIVES: [(&str, Primitive); 8] = [
    ("idle", Primitive::Idle),
    ("walk", Primitive::Walk),
    ("land", Primitive::Land),
    ("sit", Primitive::Sit),
    ("sleep", Primitive::Sleep),
    ("react", Primitive::React),
    ("talk", Primitive::Talk),
    ("hold", Primitive::Hold),
];

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
    /// Names of the Animations that declared `variant_of` this one, in name
    /// order. The renderer cycles through this one and each of these in turn,
    /// holding each for a few seconds of whole loops.
    pub variants: Vec<String>,
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

/// What must be true of the Free tier before a Behavior may be picked.
///
/// The closed set is the Free tier itself: an author can gate on how long the
/// user has been away and on which application they are in, because those are
/// the only two things ADR-0005 lets ai-buddy know for nothing. A condition is
/// a declaration like any other, so a trigger the loader does not recognise is
/// rejected rather than quietly never firing.
///
/// ponytail: no time-of-day condition, though the Director's context carries
/// the time. `std` has no local time, and a trigger written as "22 to 6" that
/// silently meant UTC would be worse than no trigger at all. Add it when a
/// local hour reaches the Engine seam.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Trigger {
    /// The user has been away for longer than this.
    IdleOver(Duration),
    /// The user has been away for less than this — freshly back, or still here.
    IdleUnder(Duration),
    /// This application is frontmost, by the name the platform reports.
    Frontmost(String),
}

/// A named sequence of Primitives, declared as data.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Behavior {
    pub primitives: Vec<Primitive>,
    /// A Behavior that follows this one, by name. Chains are validated to
    /// terminate, so no Character can put the sprite in a loop it never leaves.
    pub then: Option<String>,
    /// How likely the Static Director is to pick this Behavior against its
    /// siblings. Zero takes it out of the running entirely, leaving a Behavior
    /// only something else can reach — a chain, a Poke, or a model.
    pub weight: u32,
    /// When this Behavior may be picked, or `None` for any time at all.
    pub trigger: Option<Trigger>,
}

/// One distinct frame's art, decoded once at load.
///
/// Two readers need the pixels and neither can afford to open a file per tick:
/// the hit-test asks the mask whether the cursor is over a drawn pixel, and
/// the webview draws the PNG. A frame two Animations share is one `Art`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Art {
    /// The frame as the package shipped it, for whatever encoding the
    /// renderer hands its webview.
    pub png: Vec<u8>,
    pub mask: AlphaMask,
}

/// A Character that has been validated: every required Animation is present,
/// every frame is art the renderer holds decoded, and every Behavior is
/// playable.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Character {
    pub name: String,
    /// The Personality Prompt, or empty when the package ships none.
    pub personality: String,
    pub animations: BTreeMap<String, Animation>,
    pub behaviors: BTreeMap<String, Behavior>,
    /// Every distinct frame any Animation names, by the name it is named.
    pub art: BTreeMap<String, Art>,
    /// The renderer smooths this art when scaling instead of keeping hard
    /// pixels — the `render_mode` ADR-0006 reserved, for Characters whose
    /// frames are drawn rather than gridded.
    pub smooth: bool,
    /// The integer factor the renderer draws the art at.
    pub scale: u32,
    /// Proactive model-call wait grows by `model_base.pow(model_power)`.
    pub model_base: u32,
    pub model_power: u32,
}

/// What the renderer needs to draw one tick.
pub struct Drawn<'a> {
    /// The Animation actually drawing — the one asked for, its optional
    /// fallback, or the variant the ring has cycled to. The renderer indexes
    /// its art by this name, never by the one the engine asked with.
    pub animation: &'a str,
    pub mask: &'a AlphaMask,
    /// The frame's size in pixels, before any scaling.
    pub frame_size: (u32, u32),
    /// Which frame of the Animation is on screen.
    pub index: usize,
}

impl Character {
    /// Which frame of `animation` is on screen `animation_ms` after it
    /// started, and the mask that outlines it.
    ///
    /// The arithmetic is `Animation::frame_at`, which is where fps and loop
    /// mode come from the Character Manifest rather than a constant. This only
    /// looks up the Animation and the art the index lands on.
    ///
    /// `None` only for an Animation this Character does not have, which a
    /// validated Character cannot be asked for: the Engine names one of the
    /// nine required Animations, and a package missing one was rejected.
    /// Substituting a different Animation would be worse than drawing nothing,
    /// because the renderer would still be told the name it asked for.
    pub fn draw(&self, animation: &str, animation_ms: u32) -> Option<Drawn<'_>> {
        let (name, animation, local_ms) = self.resolve(animation, animation_ms)?;
        let index = animation.frame_at(local_ms);
        let art = self.art.get(animation.frames.get(index)?)?;

        Some(Drawn {
            animation: name,
            mask: &art.mask,
            frame_size: animation.frame_size,
            index,
        })
    }

    /// Which Animation actually draws: the one asked for, its optional
    /// fallback, or — when it anchors a variant ring — whichever member the
    /// elapsed time has cycled to, with the time already spent in its slot.
    ///
    /// Pure in the same sense as `frame_at`: elapsed milliseconds in, art
    /// out. The engine keeps saying "idle"; that a Character skateboards
    /// through some of its idling is the art's own business.
    fn resolve(&self, requested: &str, ms: u32) -> Option<(&str, &Animation, u32)> {
        let (name, base) = match self.animations.get_key_value(requested) {
            Some(found) => found,
            None => {
                let (_, fallback) = OPTIONAL_FALLBACKS
                    .iter()
                    .find(|(optional, _)| *optional == requested)?;
                self.animations.get_key_value(*fallback)?
            }
        };
        if base.variants.is_empty() {
            return Some((name.as_str(), base, ms));
        }

        let members: Vec<(&str, &Animation)> = std::iter::once((name.as_str(), base))
            .chain(base.variants.iter().filter_map(|variant| {
                self.animations
                    .get_key_value(variant)
                    .map(|(name, animation)| (name.as_str(), animation))
            }))
            .collect();
        // The base holds three slots to every variant's one: a variant is
        // seasoning, and a ring of equal shares reads as a Character that
        // cannot sit still.
        let slot = |animation: &Animation, hold: u32| -> u32 {
            let loop_ms = (animation.frames.len() as u32 * 1000 / animation.fps).max(1);
            loop_ms * hold.div_ceil(loop_ms).max(1)
        };
        let hold_for = |i: usize| {
            if i == 0 {
                3 * VARIANT_HOLD_MS
            } else {
                VARIANT_HOLD_MS
            }
        };
        let total: u32 = members
            .iter()
            .enumerate()
            .map(|(i, (_, member))| slot(member, hold_for(i)))
            .sum();
        let mut t = ms % total.max(1);
        for (i, (member_name, member)) in members.iter().enumerate() {
            let s = slot(member, hold_for(i));
            if t < s {
                return Some((member_name, member, t));
            }
            t -= s;
        }
        unreachable!("the slots sum to the total the time was wrapped to")
    }
}

/// Optional Animations, and what draws when a package does not declare them:
/// used when present, and absent silently, never as a missing sprite.
const OPTIONAL_FALLBACKS: [(&str, &str); 1] = [("climb", "walk")];

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
    // Manifest, so these are the only rejections reported on their own.
    let Some(bytes) = package.get(CHARACTER_MANIFEST_FILE) else {
        return Err(vec![format!(
            "the package contains no {CHARACTER_MANIFEST_FILE}"
        )]);
    };
    let Ok(manifest) = std::str::from_utf8(bytes) else {
        return Err(vec![format!("{CHARACTER_MANIFEST_FILE} is not UTF-8 text")]);
    };
    if manifest.len() > MANIFEST_LIMIT {
        return Err(vec![format!(
            "{CHARACTER_MANIFEST_FILE} is {} bytes, over the {MANIFEST_LIMIT}-byte limit",
            manifest.len()
        )]);
    }

    // A manifest that is not TOML is one error, not a cascade: nothing was
    // declared, so reporting a missing name and eight missing Animations on
    // top would be reporting mistakes the author has not made.
    let Some(declared) = parse(manifest, &mut errors) else {
        return Err(errors);
    };

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
    let variant_pairs = check_variants(&declared.animations, &mut errors);
    let (mut animations, art) = resolve_animations(package, declared.animations, &mut errors);
    // Linked after resolution so a variant whose frames failed never joins a
    // ring; its own declaration errors already say why.
    for (variant, base) in variant_pairs {
        if animations.contains_key(&variant) {
            if let Some(base) = animations.get_mut(&base) {
                base.variants.push(variant);
            }
        }
    }
    let behaviors = resolve_behaviors(declared.behaviors, &mut errors);

    match (errors.is_empty(), declared.name) {
        (true, Some(name)) => Ok(Character {
            name,
            personality,
            animations,
            behaviors,
            art,
            smooth: declared.smooth.unwrap_or(false),
            scale: declared.scale.unwrap_or(DEFAULT_SCALE),
            model_base: declared.model_base.unwrap_or(DEFAULT_MODEL_BASE),
            model_power: declared.model_power.unwrap_or(DEFAULT_MODEL_POWER),
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
    smooth: Option<bool>,
    scale: Option<u32>,
    model_base: Option<u32>,
    model_power: Option<u32>,
    animations: BTreeMap<String, DeclaredAnimation>,
    behaviors: BTreeMap<String, DeclaredBehavior>,
}

struct DeclaredAnimation {
    frames: Vec<String>,
    fps: u32,
    looping: bool,
    variant_of: Option<String>,
}

struct DeclaredBehavior {
    primitives: Vec<Primitive>,
    then: Option<String>,
    weight: u32,
    trigger: Option<Trigger>,
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
fn parse(manifest: &str, errors: &mut Vec<String>) -> Option<Declared> {
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
            other => errors.push(format!(
                "unknown declaration {other:?}; a Character Manifest declares \
                 name, render_mode, scale, animations, behaviors and director"
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

/// Check every declared Animation against the art the package carries, and
/// decode what passes. Art the loader cannot open or that changes size
/// mid-sequence draws a broken sprite rather than a Character, and art too
/// large to be a sprite, or too much of it, asks the renderer for memory no
/// Character needs. All of them are rejections.
///
/// Decoding here rather than in the renderer is what makes a loaded Character
/// renderable by construction: art the mask cannot be built from is one more
/// rejection naming its frame, instead of a Character the loader declared
/// valid and the renderer then refused.
fn resolve_animations(
    package: &PackageBytes,
    declared: BTreeMap<String, DeclaredAnimation>,
    errors: &mut Vec<String>,
) -> (BTreeMap<String, Animation>, BTreeMap<String, Art>) {
    let mut animations = BTreeMap::new();
    let mut art: BTreeMap<String, Art> = BTreeMap::new();
    // One mask per distinct frame, exactly as the renderer holds them: a frame
    // two Animations share is charged once.
    let mut charged: BTreeSet<String> = BTreeSet::new();
    let mut pixels: u64 = 0;

    for (name, declaration) in declared {
        let mut frame_size = None;

        for frame in &declaration.frames {
            let Some(bytes) = package.get(frame) else {
                errors.push(format!(
                    "animation {name:?} frame {frame:?} is not in the package"
                ));
                continue;
            };
            // Header first, pixels second: the header says how big the frame
            // claims to be for a few dozen bytes of bounded work, so a frame
            // over the size bound is rejected before anything inflates it.
            match art_size(bytes) {
                Err(why) => errors.push(format!(
                    "animation {name:?} frame {frame:?} is not readable art: {why}"
                )),
                Ok(size) if size.0 > MAX_FRAME_SIDE || size.1 > MAX_FRAME_SIDE => {
                    errors.push(format!(
                        "animation {name:?} frame {frame:?} is {}x{}, \
                         and no side of a frame may be over {MAX_FRAME_SIDE} pixels",
                        size.0, size.1
                    ));
                }
                Ok(size) => {
                    // Decoded only while the pixel budget holds: past it the
                    // package is rejected anyway, and decoding on regardless
                    // would build the very masks the bound exists to refuse.
                    if charged.insert(frame.clone()) {
                        pixels += u64::from(size.0) * u64::from(size.1);
                        if pixels <= MAX_CHARACTER_PIXELS {
                            match AlphaMask::from_png(bytes, ALPHA_THRESHOLD) {
                                Ok(mask) => {
                                    art.insert(
                                        frame.clone(),
                                        Art {
                                            png: bytes.clone(),
                                            mask,
                                        },
                                    );
                                }
                                Err(why) => errors.push(format!(
                                    "animation {name:?} frame {frame:?} \
                                     is not readable art: {why}"
                                )),
                            }
                        }
                    }
                    match frame_size {
                        None => frame_size = Some(size),
                        Some(first) if first != size => errors.push(format!(
                            "animation {name:?} frame {frame:?} is {}x{}, \
                             and its first frame is {}x{}; every frame is one size",
                            size.0, size.1, first.0, first.1
                        )),
                        Some(_) => {}
                    }
                }
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
                    variants: Vec::new(),
                },
            );
        }
    }

    if pixels > MAX_CHARACTER_PIXELS {
        errors.push(format!(
            "the package's frames are {pixels} pixels in all, over the \
             {MAX_CHARACTER_PIXELS}-pixel limit"
        ));
    }

    (animations, art)
}

/// Validate every `variant_of` declaration and hand back the (variant, base)
/// pairs worth linking.
///
/// A variant is more art for an Animation the engine already plays, never a
/// new Behavior. The renderer cycles a ring by whole loops on its own clock,
/// so a member that holds its last frame would stall it, and a variant of a
/// variant would make the ring's order a puzzle: both are rejected.
fn check_variants(
    declared: &BTreeMap<String, DeclaredAnimation>,
    errors: &mut Vec<String>,
) -> Vec<(String, String)> {
    let mut pairs = Vec::new();
    for (name, animation) in declared {
        let Some(base) = &animation.variant_of else {
            continue;
        };
        match declared.get(base) {
            _ if base == name => errors.push(format!("animation {name:?} is a variant_of itself")),
            None => errors.push(format!(
                "animation {name:?} is a variant_of {base:?}, which is not declared"
            )),
            Some(target) if target.variant_of.is_some() => errors.push(format!(
                "animation {name:?} is a variant_of {base:?}, itself a variant; \
                 variants ring one base"
            )),
            Some(target) if !target.looping || !animation.looping => errors.push(format!(
                "animation {name:?} and its base {base:?} must both loop \"forever\"; \
                 the renderer cycles a variant ring by whole loops"
            )),
            Some(_) => pairs.push((name.clone(), base.clone())),
        }
    }
    pairs
}

/// One frame's dimensions, from the PNG header alone.
///
/// Header only: it is bounded work whatever the file claims, and it never
/// inflates a compressed image, so the size bounds are checked before
/// `AlphaMask::from_png` decodes a pixel.
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
                    "behavior {name:?} follows {next:?}, \
                     which the package does not declare"
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
                errors.push(format!(
                    "behavior {current:?} cannot terminate: {}",
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
                    weight: declaration.weight,
                    trigger: declaration.trigger,
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
    use crate::overlay::SpriteRect;

    /// A 2x2 RGBA PNG whose top-left pixel is transparent. Art a renderer can
    /// open is all the loader asks of a frame.
    const FRAME: &[u8] = include_bytes!("../tests/fixtures/alpha-2x2.png");

    /// A 2x2 RGBA frame with every pixel drawn, so one lookup tells a mask
    /// built from it apart from one built from `FRAME`.
    const SOLID: &[u8] = include_bytes!("../tests/fixtures/opaque-2x2.png");

    /// A 2x2 greyscale frame: a readable header with no alpha to mask.
    const GREYSCALE: &[u8] = include_bytes!("../tests/fixtures/greyscale-2x2.png");

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
        let mut manifest = String::from("name = \"Blip\"\n");
        for animation in animations {
            manifest.push_str(&format!(
                "[animations.{animation}]\nframes = [\"{animation}-0.png\"]\n"
            ));
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
            variants: Vec::new(),
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
    fn a_loaded_character_carries_exactly_the_art_its_animations_name() {
        let character = load_manifest(&declaring(&REQUIRED_ANIMATIONS)).expect("package is valid");

        assert_eq!(character.art["idle-0.png"].png, FRAME, "the PNG as shipped");
        assert!(
            !character.art.contains_key("wave-0.png"),
            "art nothing declares is not decoded or carried"
        );
    }

    /// Whether the mask says the frame's top-left pixel is drawn. `SOLID`'s is
    /// and `FRAME`'s is not, which is how one frame's mask is told from the
    /// other's.
    fn corner_drawn(drawn: &Drawn<'_>) -> bool {
        drawn.mask.hit(
            &SpriteRect {
                x: 0,
                y: 0,
                scale: 1,
            },
            0,
            0,
            false,
        )
    }

    #[test]
    fn draw_returns_the_frame_the_declared_cadence_has_reached() {
        let mut package = art();
        package.insert("idle-1.png".to_string(), SOLID.to_vec());
        // Two 125ms frames of idle at the default 8fps.
        let manifest = declaring(&REQUIRED_ANIMATIONS).replace(
            "frames = [\"idle-0.png\"]",
            "frames = [\"idle-0.png\", \"idle-1.png\"]",
        );
        package.insert(CHARACTER_MANIFEST_FILE.to_string(), manifest.into_bytes());
        let character = load(&package).expect("package is valid");

        let first = character.draw("idle", 124).expect("idle is declared");
        assert_eq!(first.index, 0, "still inside the first of two 125ms frames");
        assert_eq!(first.frame_size, (2, 2));
        assert!(!corner_drawn(&first), "the mask is the one FRAME makes");

        let second = character.draw("idle", 125).expect("idle is declared");
        assert_eq!(second.index, 1);
        assert!(
            corner_drawn(&second),
            "and the mask moves to the frame the index landed on"
        );

        let wrapped = character.draw("idle", 250).expect("idle is declared");
        assert_eq!(wrapped.index, 0, "a looping strip comes back round");
        assert!(!corner_drawn(&wrapped));
    }

    /// Nothing rather than a substitute: the webview was told the name it asked
    /// for, so drawing a different Animation under it would be a lie the
    /// hit-test also believed.
    #[test]
    fn an_animation_the_character_does_not_have_draws_nothing() {
        let character = load_manifest(&declaring(&REQUIRED_ANIMATIONS)).expect("package is valid");
        assert!(character.draw("cartwheel", 0).is_none());
    }

    /// A readable header with no alpha behind it. Rejected here, naming the
    /// frame, because nothing downstream reopens the art to discover it —
    /// this loader is the last thing that can refuse a Character.
    #[test]
    fn a_frame_no_mask_can_be_built_from_is_rejected_by_name() {
        let mut package = art();
        package.insert("sit-0.png".to_string(), GREYSCALE.to_vec());
        package.insert(
            CHARACTER_MANIFEST_FILE.to_string(),
            declaring(&REQUIRED_ANIMATIONS).into_bytes(),
        );

        let errors = errors(load(&package));
        assert_names(&errors, "sit-0.png");
        assert_names(&errors, "RGBA");
    }

    #[test]
    fn a_missing_required_animation_is_rejected_by_name() {
        let eight = [
            "idle", "walk", "fall", "sit", "sleep", "react", "talk", "hold",
        ];
        let errors = errors(load_manifest(&declaring(&eight)));

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
        let manifest = declaring(&REQUIRED_ANIMATIONS)
            .replace(
                "frames = [\"walk-0.png\"]",
                "frames = [\"walk-0.png\"]\nfps = 12",
            )
            .replace(
                "frames = [\"land-0.png\"]",
                "frames = [\"land-0.png\"]\nloop = \"once\"",
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
            "{}[behaviors.greet]\nplay = [\"react\", \"talk\"]\nthen = \"settle\"\n\
             [behaviors.settle]\nplay = [\"sit\", \"sleep\"]\n",
            declaring(&REQUIRED_ANIMATIONS)
        );
        let character = load_manifest(&manifest).expect("package is valid");

        assert_eq!(
            character.behaviors["greet"],
            Behavior {
                primitives: vec![Primitive::React, Primitive::Talk],
                then: Some("settle".to_string()),
                weight: DEFAULT_WEIGHT,
                trigger: None,
            }
        );
        assert_eq!(character.behaviors["settle"].then, None);
    }

    #[test]
    fn a_behavior_carries_the_weight_and_trigger_it_declares() {
        let manifest = format!(
            "{}[behaviors.nap]\nplay = [\"sit\", \"sleep\"]\nweight = 4\nwhen = \"idle over 2m\"\n",
            declaring(&REQUIRED_ANIMATIONS)
        );
        let character = load_manifest(&manifest).expect("package is valid");

        assert_eq!(
            character.behaviors["nap"],
            Behavior {
                primitives: vec![Primitive::Sit, Primitive::Sleep],
                then: None,
                weight: 4,
                trigger: Some(Trigger::IdleOver(Duration::from_secs(120))),
            }
        );
    }

    /// A Behavior that says nothing about when it happens is one the Static
    /// Director may pick at any moment, which is what every Character written
    /// before weights existed declares.
    #[test]
    fn a_behavior_that_says_neither_weighs_one_and_waits_for_nothing() {
        let manifest = format!(
            "{}[behaviors.greet]\nplay = [\"react\", \"talk\"]\n",
            declaring(&REQUIRED_ANIMATIONS)
        );
        let character = load_manifest(&manifest).expect("package is valid");

        assert_eq!(character.behaviors["greet"].weight, DEFAULT_WEIGHT);
        assert_eq!(character.behaviors["greet"].trigger, None);
    }

    #[test]
    fn a_trigger_may_name_an_application_of_several_words() {
        let manifest = format!(
            "{}[behaviors.peek]\nplay = [\"react\"]\nwhen = \"app Google Chrome\"\n",
            declaring(&REQUIRED_ANIMATIONS)
        );
        let character = load_manifest(&manifest).expect("package is valid");

        assert_eq!(
            character.behaviors["peek"].trigger,
            Some(Trigger::Frontmost("Google Chrome".to_string()))
        );
    }

    #[test]
    fn a_weight_that_is_not_a_whole_number_is_rejected() {
        let manifest = format!(
            "{}[behaviors.greet]\nplay = [\"react\"]\nweight = \"lots\"\n",
            declaring(&REQUIRED_ANIMATIONS)
        );
        let rejected = errors(load_manifest(&manifest));

        assert_eq!(
            rejected,
            vec!["weight for behavior \"greet\" is \"lots\", \
                 which is not a whole number"
                .to_string()]
        );

        // TOML has negative numbers where the old format had only digits, and
        // a weight is a count.
        let negative = format!(
            "{}[behaviors.greet]\nplay = [\"react\"]\nweight = -3\n",
            declaring(&REQUIRED_ANIMATIONS)
        );
        let rejected = errors(load_manifest(&negative));

        assert_eq!(
            rejected,
            vec!["weight for behavior \"greet\" is -3, \
                 which is not a whole number"
                .to_string()]
        );
    }

    #[test]
    fn a_trigger_that_is_not_a_condition_is_rejected_with_the_conditions() {
        let manifest = format!(
            "{}[behaviors.greet]\nplay = [\"react\"]\nwhen = \"weather rain\"\n",
            declaring(&REQUIRED_ANIMATIONS)
        );
        let errors = errors(load_manifest(&manifest));

        assert_eq!(errors.len(), 1, "{errors:#?}");
        assert_names(&errors, "\"weather rain\"");
        assert_names(&errors, "idle over 2m");
        assert_names(&errors, "app Safari");
    }

    /// Hostile input: a duration whose last byte is the middle of a character.
    /// Splitting it off by byte would panic and take the loader with it.
    #[test]
    fn a_duration_that_is_not_ascii_is_rejected_rather_than_crashing() {
        let manifest = format!(
            "{}[behaviors.nap]\nplay = [\"sit\"]\nwhen = \"idle over 2\u{043c}\"\n",
            declaring(&REQUIRED_ANIMATIONS)
        );

        assert_names(&errors(load_manifest(&manifest)), "nap");
    }

    #[test]
    fn an_unknown_primitive_is_rejected_by_name() {
        let manifest = format!(
            "{}[behaviors.greet]\nplay = [\"talk\", \"jump\"]\n",
            declaring(&REQUIRED_ANIMATIONS)
        );
        let errors = errors(load_manifest(&manifest));

        assert_eq!(
            errors,
            vec![
                "behavior \"greet\" declares \"jump\", which is not a Primitive; \
                 the Primitives are idle, walk, land, sit, sleep, react, talk, hold"
                    .to_string()
            ],
            "the author is told the offending word and what they may write instead"
        );
    }

    #[test]
    fn a_behavior_that_cannot_terminate_is_rejected() {
        let manifest = format!(
            "{}[behaviors.pace]\nplay = [\"walk\"]\nthen = \"turn\"\n\
             [behaviors.turn]\nplay = [\"walk\"]\nthen = \"pace\"\n",
            declaring(&REQUIRED_ANIMATIONS)
        );
        let errors = errors(load_manifest(&manifest));

        assert_eq!(
            errors,
            vec!["behavior \"pace\" cannot terminate: \
                 \"pace\" -> \"turn\" -> \"pace\""
                .to_string()],
            "the author is given the whole cycle"
        );
    }

    #[test]
    fn a_behavior_that_follows_itself_is_rejected() {
        let manifest = format!(
            "{}[behaviors.pace]\nplay = [\"walk\"]\nthen = \"pace\"\n",
            declaring(&REQUIRED_ANIMATIONS)
        );
        let errors = errors(load_manifest(&manifest));

        assert_names(&errors, "cannot terminate");
        assert_names(&errors, "pace");
    }

    #[test]
    fn a_behavior_following_one_that_does_not_exist_is_rejected_by_name() {
        let manifest = format!(
            "{}[behaviors.greet]\nplay = [\"talk\"]\nthen = \"nap\"\n",
            declaring(&REQUIRED_ANIMATIONS)
        );
        let errors = errors(load_manifest(&manifest));

        assert_eq!(
            errors,
            vec!["behavior \"greet\" follows \"nap\", \
                 which the package does not declare"
                .to_string()],
            "the author is told which behavior points at what"
        );
    }

    /// Hostile input: a chain far deeper than any author would write, ending in
    /// a loop. A loader that walked it by recursion would exhaust the stack
    /// instead of reporting anything.
    #[test]
    fn a_very_deep_chain_ending_in_a_loop_is_rejected_rather_than_crashing() {
        let mut manifest = declaring(&REQUIRED_ANIMATIONS);
        for link in 0..20_000 {
            manifest.push_str(&format!("[behaviors.b{link}]\nthen = \"b{}\"\n", link + 1));
        }
        manifest.push_str("[behaviors.b20000]\nthen = \"b0\"\n");

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

    /// Hostile input: a manifest that is megabytes of junk lines. Every line
    /// the loader cannot read costs an error String, so a manifest read
    /// whatever its size answers a small package with gigabytes of rejection.
    #[test]
    fn a_character_manifest_over_the_limit_is_rejected_by_size() {
        let manifest = "z\n".repeat(MANIFEST_LIMIT);

        let errors = errors(load_manifest(&manifest));
        assert_eq!(
            errors,
            vec![format!(
                "character.manifest is {} bytes, over the {MANIFEST_LIMIT}-byte limit",
                manifest.len()
            )],
            "the manifest is refused by size, not read line by line"
        );
    }

    #[test]
    fn an_unknown_declaration_is_rejected_by_name() {
        // Before the tables: a root key written after one would land inside it.
        let manifest = format!(
            "capability = \"screen_recording\"\n{}",
            declaring(&REQUIRED_ANIMATIONS)
        );
        let errors = errors(load_manifest(&manifest));

        assert_eq!(
            errors,
            vec![
                "unknown declaration \"capability\"; a Character Manifest declares \
                 name, render_mode, scale, animations, behaviors and director"
                    .to_string()
            ],
            "no package can invent a declaration, so none can grant itself anything"
        );
    }

    #[test]
    fn director_backoff_is_declared_or_defaulted() {
        let character = load_manifest(&declaring(&REQUIRED_ANIMATIONS)).expect("loads");
        assert_eq!(character.model_base, DEFAULT_MODEL_BASE);
        assert_eq!(character.model_power, DEFAULT_MODEL_POWER);

        let manifest = format!(
            "{}\n[director]\nmodel_base = 3\nmodel_power = 2\n",
            declaring(&REQUIRED_ANIMATIONS)
        );
        let character = load_manifest(&manifest).expect("loads");
        assert_eq!(character.model_base, 3);
        assert_eq!(character.model_power, 2);
    }

    #[test]
    fn a_director_backoff_that_is_not_a_whole_number_from_one_is_rejected() {
        let manifest = format!(
            "{}\n[director]\nmodel_base = 0\nmodel_power = 1\n",
            declaring(&REQUIRED_ANIMATIONS)
        );
        let errors = errors(load_manifest(&manifest));
        assert_names(&errors, "model_base");
    }

    /// The render_mode ADR-0006 reserved: undeclared stays pixelated at the
    /// default scale, so every package written before the fields existed
    /// renders exactly as it did.
    #[test]
    fn render_mode_and_scale_are_declared_or_defaulted() {
        let character = load_manifest(&declaring(&REQUIRED_ANIMATIONS)).expect("loads");
        assert!(!character.smooth);
        assert_eq!(character.scale, DEFAULT_SCALE);

        let manifest = format!(
            "render_mode = \"smooth\"\nscale = 1\n{}",
            declaring(&REQUIRED_ANIMATIONS)
        );
        let character = load_manifest(&manifest).expect("loads");
        assert!(character.smooth);
        assert_eq!(character.scale, 1);
    }

    #[test]
    fn a_render_mode_that_is_neither_option_is_rejected_by_its_text() {
        let manifest = format!(
            "render_mode = \"blurry\"\n{}",
            declaring(&REQUIRED_ANIMATIONS)
        );
        let errors = errors(load_manifest(&manifest));
        assert_eq!(
            errors,
            vec![
                "\"render_mode\" is \"blurry\", and must be \"pixelated\" or \"smooth\""
                    .to_string()
            ],
        );
    }

    #[test]
    fn a_scale_outside_its_bounds_is_rejected_by_its_text() {
        let manifest = format!("scale = 9\n{}", declaring(&REQUIRED_ANIMATIONS));
        let errors = errors(load_manifest(&manifest));
        assert_eq!(
            errors,
            vec![format!(
                "\"scale\" is 9, and must be a whole number from 1 to {MAX_SCALE}"
            )],
        );
    }

    /// The ring is a pure function of elapsed time: the base holds for whole
    /// loops adding up to at least `VARIANT_HOLD_MS`, then each variant does,
    /// then it wraps. The engine keeps asking for "idle" throughout.
    #[test]
    fn a_variant_ring_cycles_by_whole_loops_and_wraps() {
        // idle: one frame at the default 8fps, a 125ms loop; the base holds
        // three variant-slots' worth, exactly 12000ms of whole loops. spin:
        // one frame at 1fps, 1000ms loops, a 4000ms slot. 16000ms around.
        let manifest = format!(
            "{}[animations.spin]\nframes = [\"idle-0.png\"]\nfps = 1\nvariant_of = \"idle\"\n",
            declaring(&REQUIRED_ANIMATIONS)
        );
        let character = load_manifest(&manifest).expect("loads");

        let playing = |ms: u32| {
            character
                .draw("idle", ms)
                .expect("draws")
                .animation
                .to_string()
        };
        assert_eq!(playing(0), "idle");
        assert_eq!(playing(11999), "idle", "the base holds three slots");
        assert_eq!(playing(12000), "spin", "the slot ends on a whole loop");
        assert_eq!(playing(15999), "spin");
        assert_eq!(playing(16000), "idle", "the ring wraps to its base");

        // Time inside a slot starts at that slot's own zero, so a member
        // begins at its first frame rather than mid-loop.
        assert_eq!(character.draw("idle", 12000).expect("draws").index, 0);

        // Asked for directly, a variant is an ordinary Animation.
        assert_eq!(character.draw("spin", 0).expect("draws").animation, "spin");
    }

    /// The optional Animation contract: used when present, absent silently —
    /// a Character without climb art climbs in its walk art, never as a
    /// missing sprite.
    #[test]
    fn climb_is_optional_and_falls_back_to_walk() {
        let character = load_manifest(&declaring(&REQUIRED_ANIMATIONS)).expect("loads");
        assert_eq!(character.draw("climb", 0).expect("draws").animation, "walk");

        let manifest = format!(
            "{}[animations.climb]\nframes = [\"idle-0.png\"]\n",
            declaring(&REQUIRED_ANIMATIONS)
        );
        let character = load_manifest(&manifest).expect("loads");
        assert_eq!(
            character.draw("climb", 0).expect("draws").animation,
            "climb"
        );

        // The fallback list is closed: an unknown name still draws nothing.
        assert!(character.draw("saunter", 0).is_none());
    }

    #[test]
    fn a_variant_of_nothing_a_variant_or_a_once_loop_is_rejected_by_name() {
        let manifest = format!(
            "{}\
             [animations.a]\nframes = [\"idle-0.png\"]\nvariant_of = \"dance\"\n\
             [animations.b]\nframes = [\"idle-0.png\"]\nvariant_of = \"idle\"\n\
             [animations.c]\nframes = [\"idle-0.png\"]\nvariant_of = \"b\"\n\
             [animations.d]\nframes = [\"idle-0.png\"]\nloop = \"once\"\nvariant_of = \"walk\"\n",
            declaring(&REQUIRED_ANIMATIONS)
        );
        let errors = errors(load_manifest(&manifest));
        assert_eq!(
            errors,
            vec![
                "animation \"a\" is a variant_of \"dance\", which is not declared".to_string(),
                "animation \"c\" is a variant_of \"b\", itself a variant; \
                 variants ring one base"
                    .to_string(),
                "animation \"d\" and its base \"walk\" must both loop \"forever\"; \
                 the renderer cycles a variant ring by whole loops"
                    .to_string(),
            ],
        );
    }

    /// A syntax mistake is one error naming its line, never a cascade: past it
    /// the parser would be guessing, and a guess would report mistakes the
    /// author has not made.
    #[test]
    fn a_manifest_that_is_not_toml_is_rejected_with_its_line() {
        let manifest = format!("{}animation idle\n", declaring(&REQUIRED_ANIMATIONS));
        let errors = errors(load_manifest(&manifest));

        assert_eq!(
            errors.len(),
            1,
            "one syntax error, one message: {errors:#?}"
        );
        // Nine required Animations at two lines each follow the name line.
        assert_names(&errors, "character.manifest is not TOML at line 20");
    }

    #[test]
    fn a_frame_that_is_not_in_the_package_is_rejected_by_name() {
        let manifest = declaring(&REQUIRED_ANIMATIONS).replace("sit-0.png", "sit-99.png");
        let errors = errors(load_manifest(&manifest));

        assert_eq!(
            errors,
            vec!["animation \"sit\" frame \"sit-99.png\" is not in the package".to_string()],
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
                    "frames = [\"walk-0.png\"]",
                    "frames = [\"walk-0.png\", \"walk-1.png\"]",
                )
                .into_bytes(),
        );

        let errors = errors(load(&package));
        assert_eq!(
            errors,
            vec![
                "animation \"walk\" frame \"walk-1.png\" is 3x3, and its first frame \
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
                "animation \"sit\" frame \"sit-0.png\" is 100000x1, and no side of a \
                 frame may be over 1024 pixels"
                    .to_string()
            ],
            "the author is told which frame is implausible and how big a frame may be"
        );
    }

    #[test]
    fn an_animation_with_no_frames_is_rejected_by_name() {
        // An empty list and no list at all are the same mistake to an author.
        for wave in ["[animations.wave]\nframes = []\n", "[animations.wave]\n"] {
            let empty = errors(load_manifest(&format!(
                "{}{wave}",
                declaring(&REQUIRED_ANIMATIONS)
            )));
            assert_eq!(
                empty,
                vec!["animation \"wave\" declares no frames".to_string()],
                "the author is told which Animation has no art"
            );
        }
    }

    /// TOML itself refuses a duplicate key, so a declaration written twice is
    /// a syntax error naming the key rather than a check of this module's.
    #[test]
    fn an_animation_or_behavior_declared_twice_is_rejected_by_name() {
        let twice = errors(load_manifest(&format!(
            "{}[animations.idle]\nframes = [\"idle-0.png\"]\n",
            declaring(&REQUIRED_ANIMATIONS)
        )));
        assert_names(&twice, "is not TOML");
        assert_names(&twice, "idle");

        let twice = errors(load_manifest(&format!(
            "{}[behaviors.greet]\nplay = [\"talk\"]\n[behaviors.greet]\nplay = [\"sit\"]\n",
            declaring(&REQUIRED_ANIMATIONS)
        )));
        assert_names(&twice, "is not TOML");
        assert_names(&twice, "greet");
    }

    /// Hostile input: a frame reference is eight bytes of manifest and a whole
    /// copy of the art in the renderer, so an unbounded frame count is a way to
    /// hand the renderer an allocation it dies on. The bound is checked on both
    /// sides so it cannot drift by one.
    #[test]
    fn an_animation_with_more_frames_than_the_bound_is_rejected_by_name() {
        let repeat = |count: usize| {
            format!(
                "{}[animations.wave]\nframes = [{}]\n",
                declaring(&REQUIRED_ANIMATIONS),
                vec!["\"wave-0.png\""; count].join(", ")
            )
        };

        let character = load_manifest(&repeat(MAX_FRAMES)).expect("the bound itself loads");
        assert_eq!(character.animations["wave"].frames.len(), MAX_FRAMES);

        let over = errors(load_manifest(&repeat(MAX_FRAMES + 1)));
        assert_names(&over, "wave");
        assert_names(&over, &format!("{} frames", MAX_FRAMES + 1));
    }

    /// Hostile input: `MAX_FRAME_SIDE` bounds one frame and `MAX_FRAMES` one
    /// Animation, but a Character may declare any number of Animations, so
    /// neither bounds the masks the renderer holds for the whole Character. A
    /// frame two Animations share is one mask, so it is charged once (user
    /// story 48).
    #[test]
    fn a_character_whose_frames_outweigh_the_budget_is_rejected() {
        let frame = png_bytes(MAX_FRAME_SIDE, MAX_FRAME_SIDE);
        let pixels = u64::from(MAX_FRAME_SIDE) * u64::from(MAX_FRAME_SIDE);
        let budget = (MAX_CHARACTER_PIXELS / pixels) as usize;

        // Every required Animation shares one frame, so `count` frames of art
        // are `count` masks however many Animations name them.
        let declaring_distinct = |count: usize| {
            let mut package: PackageBytes = (0..count)
                .map(|i| (format!("big-{i}.png"), frame.clone()))
                .collect();
            let mut manifest = String::from("name = \"Blip\"\n");
            for animation in REQUIRED_ANIMATIONS {
                manifest.push_str(&format!(
                    "[animations.{animation}]\nframes = [\"big-0.png\"]\n"
                ));
            }
            let rest: Vec<String> = (1..count).map(|i| format!("\"big-{i}.png\"")).collect();
            manifest.push_str(&format!(
                "[animations.wave]\nframes = [{}]\n",
                rest.join(", ")
            ));
            package.insert(CHARACTER_MANIFEST_FILE.to_string(), manifest.into_bytes());
            package
        };

        let character = load(&declaring_distinct(budget)).expect("the budget itself loads");
        assert_eq!(character.animations["wave"].frames.len(), budget - 1);

        let over = errors(load(&declaring_distinct(budget + 1)));
        assert_eq!(
            over,
            vec![format!(
                "the package's frames are {} pixels in all, over the \
                 {MAX_CHARACTER_PIXELS}-pixel limit",
                (budget as u64 + 1) * pixels
            )],
            "the author is told how much art they declared and how much a Character may hold"
        );
    }

    /// Hostile input: declarations written to confuse the loader rather than
    /// to declare anything — TOML the parser accepts and the domain does not.
    /// Each one is rejected by name, and none of them is guessed at, ignored,
    /// or allowed to panic.
    #[test]
    fn nonsense_declarations_are_each_rejected_by_name() {
        let manifest = format!(
            "{}\
             [animations.wave]\n\
             frames = \"wave-0.png\"\n\
             mirrored = true\n\
             [behaviors.chase]\n\
             play = \"walk\"\n\
             then = 3\n\
             [behaviors.pounce]\n\
             play = [\"react\", 7]\n\
             when = 6\n\
             [behaviors.\"фыр\"]\n\
             play = [[]]\n",
            declaring(&REQUIRED_ANIMATIONS)
        );

        let errors = errors(load_manifest(&manifest));

        // The whole set, not a count and a prefix: messages reading only
        // "wrong type" would satisfy a structural check while telling the
        // author nothing about what to change.
        assert_eq!(
            errors,
            vec![
                "frames for animation \"wave\" is \"wave-0.png\", and must be a list of \
                 frame files, as frames = [\"idle-0.png\"]"
                    .to_string(),
                "animation \"wave\" declares unknown \"mirrored\"; an Animation declares \
                 frames, fps, loop and variant_of"
                    .to_string(),
                "play for behavior \"chase\" is \"walk\", and must be a list of Primitives, \
                 as play = [\"react\", \"talk\"]"
                    .to_string(),
                "then for behavior \"chase\" is 3, and must name one Behavior, \
                 as then = \"settle\""
                    .to_string(),
                "behavior \"pounce\" declares 7, which is not a Primitive; the Primitives \
                 are idle, walk, land, sit, sleep, react, talk, hold"
                    .to_string(),
                "when for behavior \"pounce\" is 6, which is not a condition; a condition \
                 reads \"idle over 2m\", \"idle under 30s\" or \"app Safari\""
                    .to_string(),
                "behavior \"фыр\" declares [], which is not a Primitive; the Primitives \
                 are idle, walk, land, sit, sleep, react, talk, hold"
                    .to_string(),
            ],
            "each nonsense declaration is rejected by name, saying what is wrong"
        );
    }

    #[test]
    fn a_name_that_is_not_text_is_rejected() {
        let manifest = declaring(&REQUIRED_ANIMATIONS).replace("name = \"Blip\"", "name = 3");
        let numeric = errors(load_manifest(&manifest));

        assert_names(
            &numeric,
            "\"name\" is 3, and must be text, as name = \"Blip\"",
        );
        assert_names(&numeric, "the package declares no name");

        let manifest = declaring(&REQUIRED_ANIMATIONS).replace("name = \"Blip\"", "name = \"\"");
        let rejected = errors(load_manifest(&manifest));

        assert_names(&rejected, "\"name\" is empty");
    }

    #[test]
    fn an_unplayable_fps_or_loop_mode_is_rejected_by_name() {
        // Each case asks for the Animation at fault and what is wrong with
        // it, so a message that says only "fps" cannot pass.
        for (declaration, wanted) in [
            (
                "fps = 0",
                &["animation \"idle\"", "is 0", "must be 1 to 60"][..],
            ),
            ("fps = \"soon\"", &["animation \"idle\"", "\"soon\""]),
            ("fps = 240", &["animation \"idle\"", "is 240"]),
            ("fps = 3.5", &["animation \"idle\"", "3.5", "whole number"]),
            ("loop = \"maybe\"", &["animation \"idle\"", "\"maybe\""]),
        ] {
            let manifest = declaring(&REQUIRED_ANIMATIONS).replace(
                "frames = [\"idle-0.png\"]",
                &format!("frames = [\"idle-0.png\"]\n{declaration}"),
            );
            let errors = errors(load_manifest(&manifest));
            for offender in wanted {
                assert_names(&errors, offender);
            }
        }
    }

    #[test]
    fn a_package_with_no_name_is_rejected() {
        let manifest = declaring(&REQUIRED_ANIMATIONS).replace("name = \"Blip\"\n", "");
        let errors = errors(load_manifest(&manifest));

        assert_eq!(
            errors,
            vec!["the package declares no name".to_string()],
            "the author is told what is absent, not merely the word \"name\""
        );
    }

    #[test]
    fn every_mistake_is_reported_in_one_pass() {
        let eight = [
            "idle", "walk", "fall", "sit", "sleep", "react", "talk", "hold",
        ];
        let manifest = format!(
            "capability = \"screen_recording\"\n{}[behaviors.greet]\nplay = [\"jump\"]\n",
            declaring(&eight)
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
              capability = \"screen_recording\"\n[behaviors.jump]\nplay = [\"jump\"]\n"
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
