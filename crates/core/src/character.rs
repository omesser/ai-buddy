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

use std::collections::BTreeMap;
use std::time::Duration;

use crate::overlay::AlphaMask;

mod manifest;
mod resolve;
use manifest::parse;
use resolve::{check_variants, resolve_animations, resolve_behaviors};

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
    /// Steer walk velocity toward the cursor's x along the ground (#153).
    /// Reach cursor x: one react swat, then disengage. Timeout if never
    /// arrives: give up. The cursor is up on the screen; the buddy chases
    /// its shadow on the floor.
    Chase,
}

/// Every Primitive by the name a Character Manifest writes.
const PRIMITIVES: [(&str, Primitive); 9] = [
    ("idle", Primitive::Idle),
    ("walk", Primitive::Walk),
    ("land", Primitive::Land),
    ("sit", Primitive::Sit),
    ("sleep", Primitive::Sleep),
    ("react", Primitive::React),
    ("talk", Primitive::Talk),
    ("hold", Primitive::Hold),
    ("chase", Primitive::Chase),
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
    /// How the Character reacts when the cursor enters its Near radius (#152).
    pub near_reaction: CursorReaction,
    /// How the Character reacts to a cursor rushing at it (#152).
    pub rush_reaction: CursorReaction,
}

/// How a Character reacts to the cursor entering its Near radius or rushing at it (#152).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CursorReaction {
    /// Keep doing whatever it was doing.
    #[default]
    Indifferent,
    /// Speak (`talk`).
    Speak,
    /// Turn to face the cursor (`facing` toward it; art already mirrors).
    Face,
    /// Walk toward the cursor.
    Toward,
    /// Walk away from the cursor.
    Away,
    /// Play `react`.
    React,
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
            near_reaction: declared.near_reaction.unwrap_or_default(),
            rush_reaction: declared.rush_reaction.unwrap_or_default(),
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

#[cfg(test)]
mod tests {
    use super::*;

    use crate::overlay::SpriteRect;

    use std::collections::BTreeSet;

    /// A 2x2 RGBA PNG whose top-left pixel is transparent. Art a renderer can
    /// open is all the loader asks of a frame.
    const FRAME: &[u8] = include_bytes!("../tests/fixtures/alpha-2x2.png");

    /// A 2x2 RGBA frame with every pixel drawn, so one lookup tells a mask
    /// built from it apart from one built from `FRAME`.
    const SOLID: &[u8] = include_bytes!("../tests/fixtures/opaque-2x2.png");

    /// One frame file per required Animation, plus a `wave` no manifest has to
    /// declare. Art nothing declares is ignored, the same as a README would be.
    pub(super) fn art() -> PackageBytes {
        REQUIRED_ANIMATIONS
            .iter()
            .chain(["wave"].iter())
            .map(|name| (format!("{name}-0.png"), FRAME.to_vec()))
            .collect()
    }

    /// A Character Manifest declaring exactly `animations`, one frame each.
    pub(super) fn declaring(animations: &[&str]) -> String {
        let mut manifest = String::from("name = \"Blip\"\n");
        for animation in animations {
            manifest.push_str(&format!(
                "[animations.{animation}]\nframes = [\"{animation}-0.png\"]\n"
            ));
        }
        manifest
    }

    pub(super) fn load_manifest(manifest: &str) -> Result<Character, Vec<String>> {
        let mut package = art();
        package.insert(
            CHARACTER_MANIFEST_FILE.to_string(),
            manifest.as_bytes().to_vec(),
        );
        load(&package)
    }

    /// The errors, or a failure naming what loaded instead.
    pub(super) fn errors(result: Result<Character, Vec<String>>) -> Vec<String> {
        match result {
            Ok(character) => panic!("expected rejection, loaded {}", character.name),
            Err(errors) => errors,
        }
    }

    pub(super) fn assert_names(errors: &[String], offender: &str) {
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
}
