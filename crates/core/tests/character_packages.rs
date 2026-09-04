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
//! Character-specific tests cover constraints unique to that character. Every
//! shipped character's load and required animations are covered by the shell
//! crate's integration test.

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

/// Load a workspace Character Package, or panic with the loader's errors.
fn load_package(name: &str) -> Result<character::Character, Vec<String>> {
    character::load(&package_bytes(name))
}

/// Walk a package directory into the path→bytes map `character::load` takes.
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

/// Entry test: package parses, has personality, and walk is the shipped 20-frame loop.
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
    assert_eq!(
        walk.frames.len(),
        20,
        "walk must be the shipped 20-frame two-step loop"
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
        "engage behavior exists for weapon-raise emote"
    );

    let engage = &character.behaviors["engage"];
    assert!(
        engage.primitives.len() >= 2,
        "engage composes multiple primitives"
    );
    assert!(
        engage.primitives.contains(&character::Primitive::React),
        "engage includes react (weapon raise)"
    );

    assert!(
        character.behaviors.contains_key("pursue"),
        "pursue behavior exists for walk-toward-cursor"
    );
    let pursue = &character.behaviors["pursue"];
    assert!(
        pursue.primitives.contains(&character::Primitive::Chase),
        "pursue includes Chase primitive for walk-toward-cursor movement"
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

/// Mean x of cyan cheek-light pixels in an RGBA PNG (Buddy Bot status LED).
fn cyan_cheek_light_x(bytes: &[u8]) -> f32 {
    let mut reader = png::Decoder::new(Cursor::new(bytes))
        .read_info()
        .expect("every frame is a PNG");
    let width = reader.info().width as usize;
    let mut buf = vec![0; reader.output_buffer_size().expect("frame fits in memory")];
    let frame = reader.next_frame(&mut buf).expect("frame decodes");
    let mut sx = 0.0f32;
    let mut n = 0.0f32;
    for (i, px) in buf[..frame.buffer_size()].chunks_exact(4).enumerate() {
        let (r, g, b, a) = (px[0], px[1], px[2], px[3]);
        if a > 128 && b > 150 && r < 120 && g > 80 {
            sx += (i % width) as f32;
            n += 1.0;
        }
    }
    assert!(n > 0.0, "expected cyan status-light pixels");
    sx / n
}

/// #161: every grounded pose has a foot on the canvas bottom row.
/// Only `fall` is airborne.
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

/// #161: one canvas for the package, silhouette heavy beside Jotaro's 110px
/// without dwarfing the desktop.
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

/// Decode RGBA8 pixels from a frame PNG.
fn frame_rgba(bytes: &[u8]) -> (usize, usize, Vec<[u8; 4]>) {
    let mut reader = png::Decoder::new(Cursor::new(bytes))
        .read_info()
        .expect("every frame is a PNG");
    let info = reader.info();
    let (width, height) = (info.width as usize, info.height as usize);
    let mut buf = vec![0; reader.output_buffer_size().expect("frame fits in memory")];
    let frame = reader.next_frame(&mut buf).expect("frame decodes");
    let pixels = buf[..frame.buffer_size()]
        .chunks_exact(4)
        .map(|p| [p[0], p[1], p[2], p[3]])
        .collect();
    (width, height, pixels)
}

/// Mean absolute RGB distance over pixels visible in either frame.
/// Near-duplicate frontals from the turn clip score ~1-3; distinct poses score >> 15.
fn mean_rgb_distance(a: &[[u8; 4]], b: &[[u8; 4]]) -> f64 {
    assert_eq!(a.len(), b.len(), "frames share one canvas");
    let mut sum = 0.0;
    let mut n = 0.0;
    for (pa, pb) in a.iter().zip(b.iter()) {
        let va = pa[3] >= VISIBLE;
        let vb = pb[3] >= VISIBLE;
        if !(va || vb) {
            continue;
        }
        let (ra, ga, ba) = if va {
            (pa[0] as f64, pa[1] as f64, pa[2] as f64)
        } else {
            (0.0, 0.0, 0.0)
        };
        let (rb, gb, bb) = if vb {
            (pb[0] as f64, pb[1] as f64, pb[2] as f64)
        } else {
            (0.0, 0.0, 0.0)
        };
        sum += ((ra - rb).abs() + (ga - gb).abs() + (ba - bb).abs()) / 3.0;
        n += 1.0;
    }
    if n == 0.0 {
        0.0
    } else {
        sum / n
    }
}

/// #161: idle, land, sit, sleep, hold, react and talk must each be their own
/// art — not byte-identical copies, and not near-duplicate frontals (mean RGB
/// ~1-3) that look like one shared stand. Sit must also read as a hunker:
/// clearly shorter silhouette than idle.
#[test]
fn timber_wolf_poses_are_not_copies_of_each_other() {
    let posed = ["idle", "land", "sit", "sleep", "hold", "react", "talk"];
    let frames: Vec<_> = frames_of("timber-wolf")
        .into_iter()
        .filter(|(animation, _, _)| posed.contains(&animation.as_str()))
        .collect();

    // Byte-identity still fails exact copies.
    for (i, (animation, frame, bytes)) in frames.iter().enumerate() {
        for (other_animation, other_frame, other_bytes) in &frames[i + 1..] {
            if animation == other_animation {
                continue;
            }
            assert!(
                bytes != other_bytes,
                "{animation} frame {frame} is the same art as {other_animation}                  frame {other_frame}"
            );
        }
    }

    // First frame of each pose vs every other pose: near-duplicate frontals
    // from the turn clip scored mean RGB ~1-3; distinct art clears 15+.
    const MIN_MEAN_RGB: f64 = 12.0;
    let first: Vec<_> = posed
        .iter()
        .map(|anim| {
            let (frame, bytes) = frames
                .iter()
                .find(|(a, _, _)| a == anim)
                .map(|(_, f, b)| (f.clone(), b.clone()))
                .unwrap_or_else(|| panic!("{anim} has a first frame"));
            let (_, _, rgba) = frame_rgba(&bytes);
            ((*anim).to_string(), frame, rgba)
        })
        .collect();

    for (i, (animation, frame, rgba)) in first.iter().enumerate() {
        for (other_animation, other_frame, other_rgba) in &first[i + 1..] {
            let d = mean_rgb_distance(rgba, other_rgba);
            assert!(
                d >= MIN_MEAN_RGB,
                "{animation} frame {frame} is a near-copy of {other_animation}                  frame {other_frame} (mean RGB distance {d:.2}, need >= {MIN_MEAN_RGB})"
            );
        }
    }

    // Sit is a hunker: silhouette at least 12px shorter than idle.
    let idle_h = {
        let bytes = &frames
            .iter()
            .find(|(a, f, _)| a == "idle" && f.ends_with("idle-0.png"))
            .expect("idle-0")
            .2;
        let (w, h, alpha) = frame_alpha(bytes);
        let (top, bottom, _) = silhouette(w, h, &alpha);
        bottom - top + 1
    };
    let sit_h = {
        let bytes = &frames
            .iter()
            .find(|(a, f, _)| a == "sit" && f.ends_with("sit-0.png"))
            .expect("sit-0")
            .2;
        let (w, h, alpha) = frame_alpha(bytes);
        let (top, bottom, _) = silhouette(w, h, &alpha);
        bottom - top + 1
    };
    assert!(
        sit_h + 12 <= idle_h,
        "sit silhouette is {sit_h}px and idle is {idle_h}px; sit must hunker at least 12px lower"
    );
}

/// TAC laser: optional idle variant only (scan-1..3; scan-0 is a dupe of
/// idle-0). Not baked into sit — perched sit→idle escape is #311.
#[test]
fn timber_wolf_declares_tac_laser_scan_as_idle_variant() {
    let character = load_package("timber-wolf").expect("Timber Wolf package is valid");

    assert!(
        character.animations.contains_key("scan"),
        "scan animation is declared"
    );
    let scan = &character.animations["scan"];
    assert_eq!(scan.frames.len(), 4, "scan is a four-frame TAC loop");
    assert!(
        character.animations["idle"]
            .variants
            .contains(&"scan".to_string()),
        "scan is an idle variant"
    );
    assert!(
        character.animations["sit"].variants.is_empty(),
        "sit is not where the laser lives"
    );
    assert!(
        !character.behaviors.contains_key("scan"),
        "no scan Behavior — the variant ring is enough"
    );
    for name in [
        "frames/scan-1.png",
        "frames/scan-2.png",
        "frames/scan-3.png",
    ] {
        assert!(
            scan.frames.iter().any(|f| f == name),
            "scan includes {name}"
        );
    }
    assert!(
        scan.frames.iter().all(|f| f != "frames/scan-0.png"),
        "scan-0 is a dupe of idle-0 and must not be referenced"
    );

    let drawn = character
        .draw("scan", 0)
        .expect("scan draws when asked for by name");
    assert_eq!(drawn.animation, "scan");

    let engage = &character.behaviors["engage"];
    assert!(
        engage.primitives.contains(&character::Primitive::Idle),
        "engage steps through idle so the scan ring can surface"
    );
    assert!(
        engage.primitives.contains(&character::Primitive::React),
        "engage still raises weapons"
    );

    assert!(character.behaviors.contains_key("patrol"), "patrol stays");
    let pursue = &character.behaviors["pursue"];
    assert!(
        pursue.primitives.contains(&character::Primitive::Chase),
        "pursue still walks toward the cursor"
    );
}

/// #161: a patrol mech tracks a near contact and raises a weapon on a rush.
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

/// Buddy Bot: logo mascot package loads with all nine required animations and
/// the authored frame counts from the Grok Imagine pack.
#[test]
fn buddy_bot_package_loads_with_all_required_animations() {
    let character = load_package("buddy-bot").expect("Buddy Bot package is valid");

    assert_eq!(character.name, "Buddy Bot");
    assert!(
        !character.personality.is_empty(),
        "Buddy Bot has a personality prompt"
    );

    assert_required_animations(&character);

    let expected_frames = [
        ("idle", 6),
        ("walk", 8),
        ("talk", 6),
        ("sleep", 4),
        ("sit", 4),
        ("react", 5),
        ("land", 4),
        ("hold", 4),
        ("fall", 6),
    ];
    for (animation, count) in expected_frames {
        assert_eq!(
            character.animations[animation].frames.len(),
            count,
            "{animation} must carry the shipped {count}-frame loop"
        );
    }
}

#[test]
fn buddy_bot_uses_scale_1_for_authored_desktop_frames() {
    let character = load_package("buddy-bot").expect("Buddy Bot package is valid");

    assert_eq!(
        character.scale, 1,
        "Buddy Bot uses scale 1: 320×320 frames are already desktop-sized; \
         the schema only allows whole-number scale 1–4"
    );
    assert!(
        character.smooth,
        "soft anti-aliased mascot art uses smooth, not pixelated"
    );
}

#[test]
fn buddy_bot_frames_are_320_square_rgba() {
    let frames = frames_of("buddy-bot");
    assert_eq!(frames.len(), 47, "the pack ships 47 PNG frames");

    for (animation, frame, bytes) in frames {
        let (width, height, alpha) = frame_alpha(&bytes);
        assert_eq!(
            (width, height),
            (320, 320),
            "{animation} frame {frame} must be 320×320"
        );
        assert!(
            alpha.iter().any(|&a| a > 0),
            "{animation} frame {frame} must have visible pixels"
        );
        assert!(
            alpha.contains(&0),
            "{animation} frame {frame} must keep a transparent margin"
        );
    }
}

/// Walk art must face right (engine mirrors for left). The Imagine pack's walk
/// faced left — cheek status light on the viewer's right — which read as
/// moonwalking. After the package flip, the cyan cheek light sits in the left
/// half like the front-facing idle frames (character's right cheek).
#[test]
fn buddy_bot_walk_faces_right_like_idle_cheek_light() {
    let files = package_bytes("buddy-bot");
    let idle = files.get("frames/idle-0.png").expect("idle-0");
    let walk = files.get("frames/walk-0.png").expect("walk-0");
    let (width, _, _) = frame_alpha(walk);
    let mid = width as f32 / 2.0;

    let idle_light = cyan_cheek_light_x(idle);
    let walk_light = cyan_cheek_light_x(walk);

    assert!(
        idle_light < mid,
        "idle cheek light is on the viewer's left, got {idle_light} mid {mid}"
    );
    assert!(
        walk_light < mid,
        "walk must face right: cheek light on viewer's left after flip, got {walk_light} mid {mid}"
    );
}

#[test]
fn buddy_bot_declares_friendly_cursor_reactions() {
    let character = load_package("buddy-bot").expect("Buddy Bot package is valid");

    assert_eq!(
        character.near_reaction,
        CursorReaction::Speak,
        "a friendly mascot greets when the cursor enters Near"
    );
    assert_eq!(
        character.rush_reaction,
        CursorReaction::React,
        "a rush earns a lean-pop react, not flight"
    );
}

#[test]
fn buddy_bot_behaviors_cover_a_helpful_idle_life() {
    let character = load_package("buddy-bot").expect("Buddy Bot package is valid");

    for name in [
        "greet", "help", "fidget", "stroll", "patrol", "settle", "nap",
    ] {
        assert!(
            character.behaviors.contains_key(name),
            "Buddy Bot declares {name}"
        );
    }

    assert_eq!(
        character.behaviors["greet"].then.as_deref(),
        Some("stroll"),
        "greet chains into stroll so a hello is followed by a walk          (BMO greet→patrol / cat greet→inspect)"
    );
    assert_eq!(
        character.behaviors["help"].weight, 2,
        "help stays present but light enough that stroll can win the 30s–1m overlap"
    );
    assert_eq!(
        character.behaviors["stroll"].weight, 3,
        "stroll outranks fidget/help so the mascot actually walks the desk"
    );
    assert!(
        character.behaviors["stroll"]
            .primitives
            .contains(&character::Primitive::Walk),
        "stroll is a Walk"
    );
    assert_eq!(
        character.behaviors["settle"].then.as_deref(),
        Some("nap"),
        "settle chains into nap so sitting leads to sleep"
    );
    assert_eq!(
        character.behaviors["nap"].weight, 4,
        "nap outranks late-band stroll/patrol so sleep is visible"
    );
    assert!(
        matches!(
            character.behaviors["nap"].trigger,
            Some(character::Trigger::IdleOver(d)) if d == std::time::Duration::from_secs(120)
        ),
        "nap opens at idle over 2m (was 5m — too rare against walk)"
    );
    assert!(
        character.behaviors["nap"]
            .primitives
            .contains(&character::Primitive::Sleep),
        "nap ends on sleep for an empty desk"
    );
}
