//! Check declared Animations against art, and Behaviors against each other.

use std::collections::btree_map::Entry;
use std::collections::{BTreeMap, BTreeSet};

use crate::overlay::AlphaMask;

use super::manifest::{DeclaredAnimation, DeclaredBehavior};
use super::{
    Animation, Art, Behavior, PackageBytes, ALPHA_THRESHOLD, MAX_CHARACTER_PIXELS, MAX_FRAME_SIDE,
    SHOWN_LOOP_BEHAVIORS,
};

/// Check every declared Animation against the art the package carries, and
/// decode what passes. Art the loader cannot open or that changes size
/// mid-sequence draws a broken sprite rather than a Character, and art too
/// large to be a sprite, or too much of it, asks the renderer for memory no
/// Character needs. All of them are rejections.
///
/// Headers first, masks second: the pixel budget is a sum of IHDR sizes, so
/// an over-budget package is refused before any mask is built. Decoding a
/// package we are about to reject would spend the memory the bound exists
/// to refuse.
///
/// Decoding here rather than in the renderer is what makes a loaded Character
/// renderable by construction: art the mask cannot be built from is one more
/// rejection naming its frame, instead of a Character the loader declared
/// valid and the renderer then refused.
pub(super) fn resolve_animations(
    package: &PackageBytes,
    declared: BTreeMap<String, DeclaredAnimation>,
    errors: &mut Vec<String>,
) -> (BTreeMap<String, Animation>, BTreeMap<String, Art>) {
    let mut animations = BTreeMap::new();
    let mut art: BTreeMap<String, Art> = BTreeMap::new();
    // One mask per distinct frame, exactly as the renderer holds them: a frame
    // two Animations share is charged once. The animation name is kept so a
    // decode error still names the declaration, not only the file.
    let mut charged: BTreeMap<String, String> = BTreeMap::new();
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
                    if let Entry::Vacant(slot) = charged.entry(frame.clone()) {
                        slot.insert(name.clone());
                        pixels += u64::from(size.0) * u64::from(size.1);
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
                    weight: declaration.weight,
                },
            );
        }
    }

    if pixels > MAX_CHARACTER_PIXELS {
        errors.push(format!(
            "the package's frames are {pixels} pixels in all, over the \
             {MAX_CHARACTER_PIXELS}-pixel limit"
        ));
        // Past the budget the package is refused, and decoding on regardless
        // would build the very masks the bound exists to refuse. Headers
        // already named every frame; that is the whole of this path.
        return (animations, art);
    }

    for (frame, animation) in charged {
        let bytes = &package[&frame];
        match AlphaMask::from_png(bytes, ALPHA_THRESHOLD) {
            Ok(mask) => {
                art.insert(
                    frame,
                    Art {
                        png: bytes.clone(),
                        mask,
                    },
                );
            }
            Err(why) => errors.push(format!(
                "animation {animation:?} frame {frame:?} is not readable art: {why}"
            )),
        }
    }

    (animations, art)
}

/// Validate every `variant_of` declaration and hand back the (variant, base)
/// pairs worth linking.
///
/// A variant is more art for an Animation the engine already plays, never a
/// new Behavior. A drawn member plays for as long as the engine keeps asking
/// for the Animation, so one that holds its last frame would stall there, and
/// a variant of a variant would leave a ring inside a ring: both are
/// rejected.
pub(super) fn check_variants(
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
                 a drawn variant plays until the engine asks for something else"
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
pub(super) fn resolve_behaviors(
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
    use crate::character::tests::{art, assert_names, declaring, errors, load_manifest};
    use crate::character::{load, CHARACTER_MANIFEST_FILE, REQUIRED_ANIMATIONS};

    /// A 2x2 greyscale frame: a readable header with no alpha to mask.
    const GREYSCALE: &[u8] = include_bytes!("../../tests/fixtures/greyscale-2x2.png");

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
    /// instead of reporting anything. Two thousand links is past a typical
    /// debug stack; the proof is the rejection, not the length of the TOML.
    #[test]
    fn a_very_deep_chain_ending_in_a_loop_is_rejected_rather_than_crashing() {
        const DEPTH: u32 = 2_000;
        let mut manifest = declaring(&REQUIRED_ANIMATIONS);
        for link in 0..DEPTH {
            manifest.push_str(&format!("[behaviors.b{link}]\nthen = \"b{}\"\n", link + 1));
        }
        manifest.push_str(&format!("[behaviors.b{DEPTH}]\nthen = \"b0\"\n"));

        let errors = errors(load_manifest(&manifest));
        assert_names(&errors, "cannot terminate");
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
                 a drawn variant plays until the engine asks for something else"
                    .to_string(),
            ],
        );
    }

    /// The wording is the behavior here, not the refusal: an Animation that is
    /// a variant of itself is also a variant of a variant, so losing the
    /// self-reference guard would still reject it — with the ring message,
    /// which names a mistake the author did not make.
    #[test]
    fn an_animation_that_is_a_variant_of_itself_is_rejected_by_name() {
        let manifest = format!(
            "{}[animations.shimmy]\nframes = [\"idle-0.png\"]\nvariant_of = \"shimmy\"\n",
            declaring(&REQUIRED_ANIMATIONS)
        );
        let errors = errors(load_manifest(&manifest));

        assert_eq!(
            errors,
            vec!["animation \"shimmy\" is a variant_of itself".to_string()],
        );
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

    /// Hostile input: `MAX_FRAME_SIDE` bounds one frame and `MAX_FRAMES` one
    /// Animation, but a Character may declare any number of Animations, so
    /// neither bounds the masks the renderer holds for the whole Character. A
    /// frame two Animations share is one mask, so it is charged once (user
    /// story 48).
    ///
    /// The package that sits on the budget is not loaded: that is 256
    /// megapixels of masks, and the check that it *would* load is the same
    /// arithmetic the over-budget path already uses. The refusal is the
    /// behavior; headers name the frames, so nothing inflates them.
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
