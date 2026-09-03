//! Integration tests for shipped Character Packages.
//!
//! These tests validate at the workspace layout seam: that the on-disk
//! character packages in `characters/` can be loaded by the core crate's
//! `character::load` parser and meet the contracts the engine depends on.
//!
//! The seam matters because the parser and the packages are separate: one
//! change can break the other. A broken shipped package is a shipping defect,
//! not a unit test failure. These tests catch that before release.
//!
//! Each character gets its own test because each has different constraints:
//! pixel art scale, personality presence, specific behaviors. The required
//! animation set is a shared contract.

use ai_buddy_core::character::{self, CursorReaction, REQUIRED_ANIMATIONS};
use std::collections::BTreeMap;
use std::io::Cursor;
use std::path::Path;

/// Read a Character Package directory in the workspace root into the same
/// `PackageBytes` map `character::load` takes.
fn package_bytes(name: &str) -> BTreeMap<String, Vec<u8>> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("core is in crates/")
        .parent()
        .expect("crates/ is in workspace");
    let package_dir = root.join("characters").join(name);

    let mut files = BTreeMap::new();
    collect(&package_dir, &package_dir, &mut files)
        .unwrap_or_else(|e| panic!("{}: {}", package_dir.display(), e));
    files
}

/// Load a Character Package from a directory in the workspace root.
fn load_package(name: &str) -> Result<character::Character, Vec<String>> {
    character::load(&package_bytes(name))
}

/// Recursively read a directory into a `PackageBytes` map.
fn collect(root: &Path, dir: &Path, files: &mut BTreeMap<String, Vec<u8>>) -> std::io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            collect(root, &path, files)?;
        } else {
            let name = path
                .strip_prefix(root)
                .expect("every walked path starts at the root")
                .components()
                .map(|part| part.as_os_str().to_string_lossy())
                .collect::<Vec<_>>()
                .join("/");
            files.insert(name, std::fs::read(&path)?);
        }
    }
    Ok(())
}

/// Assert that a Character declares every required animation.
fn assert_required_animations(character: &character::Character) {
    for required in REQUIRED_ANIMATIONS {
        assert!(
            character.animations.contains_key(required),
            "{} missing required animation: {required:?}",
            character.name
        );
    }
}

#[test]
fn cat_package_loads_with_all_required_animations() {
    let character = load_package("cat").expect("Cat package is valid");

    assert_eq!(character.name, "Cat");
    assert_required_animations(&character);
}

#[test]
fn black_mage_package_loads_with_all_required_animations() {
    let character = load_package("black-mage").expect("Black Mage package is valid");

    assert_eq!(character.name, "Black Mage");
    assert_required_animations(&character);
    assert_eq!(
        character.scale, 3,
        "Black Mage uses scale 3 for readability"
    );
}

#[test]
fn timber_wolf_package_loads_with_all_required_animations() {
    let character = load_package("timber-wolf").expect("Timber Wolf package is valid");

    assert_eq!(character.name, "Timber Wolf");
    assert!(
        !character.personality.is_empty(),
        "Timber Wolf has a personality prompt"
    );

    assert_required_animations(&character);

    let walk = &character.animations["walk"];
    assert!(
        walk.frames.len() >= 2,
        "walk has at least 2 frames for animation"
    );
}

#[test]
fn timber_wolf_behaviors_compose_existing_primitives() {
    let character = load_package("timber-wolf").expect("Timber Wolf package is valid");

    assert!(
        !character.behaviors.is_empty(),
        "Timber Wolf declares behaviors"
    );

    assert!(
        character.behaviors.contains_key("patrol"),
        "patrol behavior exists"
    );
    assert!(
        character.behaviors.contains_key("engage"),
        "engage behavior exists for shooting emote"
    );

    let engage = &character.behaviors["engage"];
    assert!(
        engage.primitives.len() >= 2,
        "engage composes multiple primitives"
    );
}

#[test]
fn timber_wolf_uses_scale_1_for_captured_frames() {
    let character = load_package("timber-wolf").expect("Timber Wolf package is valid");

    assert_eq!(
        character.scale, 1,
        "Timber Wolf uses scale 1: frames are ~148px captures on a 176x160 canvas like Jotaro, not tiny pixel art"
    );
}

/// A pixel the eye can see, rather than the feathered edge of the matte.
const VISIBLE: u8 = 32;

/// How many visible pixels a row needs before it counts as a foot on the
/// floor, so a stray speck left by the matte cannot ground a frame.
const FOOT_PIXELS: usize = 3;

/// The canvas size and alpha channel of one frame PNG, row-major.
fn frame_alpha(bytes: &[u8]) -> (usize, usize, Vec<u8>) {
    let mut reader = png::Decoder::new(Cursor::new(bytes))
        .read_info()
        .expect("every frame is a PNG");

    let info = reader.info();
    let color_type = info.color_type;
    let bit_depth = info.bit_depth;
    let (width, height) = (info.width as usize, info.height as usize);
    assert_eq!(color_type, png::ColorType::Rgba, "frames carry alpha");
    assert_eq!(
        bit_depth,
        png::BitDepth::Eight,
        "frames are 8 bits a channel"
    );

    let mut buf = vec![0; reader.output_buffer_size().expect("frame fits in memory")];
    let frame = reader.next_frame(&mut buf).expect("frame decodes");
    let alpha = buf[..frame.buffer_size()]
        .chunks_exact(4)
        .map(|pixel| pixel[3])
        .collect();

    (width, height, alpha)
}

/// The first and last rows carrying visible pixels, and how many the last row
/// carries — the sprite's silhouette reduced to where it starts and stands.
fn silhouette(width: usize, height: usize, alpha: &[u8]) -> (usize, usize, usize) {
    let mut top = None;
    let mut bottom = 0;
    let mut bottom_pixels = 0;

    for row in 0..height {
        let visible = alpha[row * width..(row + 1) * width]
            .iter()
            .filter(|&&a| a >= VISIBLE)
            .count();
        if visible > 0 {
            top = top.or(Some(row));
            bottom = row;
            bottom_pixels = visible;
        }
    }

    (top.expect("frame is not blank"), bottom, bottom_pixels)
}

/// Every frame of a package, in `(animation, frame name, bytes)` order.
fn frames_of(name: &str) -> Vec<(String, String, Vec<u8>)> {
    let character = load_package(name).unwrap_or_else(|e| panic!("{name} is valid: {e:?}"));
    let files = package_bytes(name);

    let mut frames = Vec::new();
    for (animation, declared) in &character.animations {
        for frame in &declared.frames {
            let bytes = files
                .get(frame)
                .unwrap_or_else(|| panic!("{animation} declares {frame}, which the package holds"))
                .clone();
            frames.push((animation.clone(), frame.clone(), bytes));
        }
    }
    frames
}

/// #161 review: "the mech needs to be placed on the frame's floor so it won't
/// float. at least 1 leg on the floor at all times (since it's not running)."
/// The canvas bottom row is that floor, so every grounded pose has a foot on
/// it. Only `fall` is airborne.
#[test]
fn timber_wolf_stands_on_the_canvas_floor() {
    for (animation, frame, bytes) in frames_of("timber-wolf") {
        let (width, height, alpha) = frame_alpha(&bytes);
        let (_, bottom, bottom_pixels) = silhouette(width, height, &alpha);
        let gap = height - 1 - bottom;

        if animation == "fall" {
            assert!(
                (1..=20).contains(&gap),
                "{frame} is a fall frame, so it hangs clear of the floor by 1-20px, not {gap}px"
            );
            continue;
        }

        assert_eq!(
            gap, 0,
            "{animation} frame {frame} floats {gap}px above the canvas floor"
        );
        assert!(
            bottom_pixels >= FOOT_PIXELS,
            "{animation} frame {frame} touches the floor with only {bottom_pixels} pixels, \
             which is a speck and not a leg"
        );
    }
}

/// #161 review: "The mech is too big, needs scaling down." One canvas for the
/// whole package, and a silhouette that reads as a heavy mech beside Jotaro's
/// 110px without dwarfing the desktop.
#[test]
fn timber_wolf_frames_share_one_canvas_at_a_desktop_scale() {
    let frames = frames_of("timber-wolf");
    let mut canvas = None;
    let mut tallest = 0;

    for (animation, frame, bytes) in &frames {
        let (width, height, alpha) = frame_alpha(bytes);
        let size = (width, height);
        match canvas {
            None => canvas = Some(size),
            Some(first) => assert_eq!(
                size, first,
                "{animation} frame {frame} is {width}x{height}, and the package's other \
                 frames are {}x{}",
                first.0, first.1
            ),
        }

        let (top, bottom, _) = silhouette(width, height, &alpha);
        tallest = tallest.max(bottom - top + 1);
    }

    assert!(
        (120..=152).contains(&tallest),
        "the tallest Timber Wolf pose is {tallest}px, and a 75-ton mech reads between \
         120px (taller than Jotaro's 110px) and 152px (short of dwarfing the desktop)"
    );
}

/// #161 review: idle, land, sit, sleep and hold all shipped as copies of one
/// side-profile pose, and "the mech never sleeps - so we can just use idle +
/// torso twists". Each of those poses, plus react and talk, is its own art.
#[test]
fn timber_wolf_poses_are_not_copies_of_each_other() {
    let posed = ["idle", "land", "sit", "sleep", "hold", "react", "talk"];
    let frames: Vec<_> = frames_of("timber-wolf")
        .into_iter()
        .filter(|(animation, _, _)| posed.contains(&animation.as_str()))
        .collect();

    for (i, (animation, frame, bytes)) in frames.iter().enumerate() {
        for (other_animation, other_frame, other_bytes) in &frames[i + 1..] {
            if animation == other_animation {
                continue;
            }
            assert!(
                bytes != other_bytes,
                "{animation} frame {frame} is the same art as {other_animation} \
                 frame {other_frame}"
            );
        }
    }
}

/// #161 review: "Missing reactions". A patrol mech tracks a contact rather
/// than closing on it, and a rush at the chassis earns a weapon raise.
#[test]
fn timber_wolf_declares_cursor_reactions() {
    let character = load_package("timber-wolf").expect("Timber Wolf package is valid");

    assert_eq!(
        character.near_reaction,
        CursorReaction::Face,
        "the cursor entering the Near radius turns the mech to track it"
    );
    assert_eq!(
        character.rush_reaction,
        CursorReaction::React,
        "a cursor rushing the chassis plays react, the weapon raise"
    );
}
