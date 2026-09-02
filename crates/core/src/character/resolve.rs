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
/// new Behavior. The renderer cycles a ring by whole loops on its own clock,
/// so a member that holds its last frame would stall it, and a variant of a
/// variant would make the ring's order a puzzle: both are rejected.
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
