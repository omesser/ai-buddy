//! The Character as the overlay needs it: art resolved, once, at startup.
//!
//! A validated `Character` names its frames; it does not carry them. Two things
//! need the pixels themselves and neither can afford to open a file per tick —
//! the hit-test, which asks whether the cursor is over a drawn pixel, and the
//! webview, which draws it. So both are resolved here when the Character is
//! loaded and looked up by name thereafter.
//!
//! Frames reach the webview as `data:` URLs rather than as file paths. A
//! Character Package lives outside the front end's own directory — in the
//! user's Application Support, or wherever they put it — so there is no URL the
//! webview could fetch it from. Handing over the bytes avoids granting the
//! webview a filesystem scope for the sake of drawing a sprite.

use std::collections::BTreeMap;

use ai_buddy_core::character::Character;
use ai_buddy_core::overlay::AlphaMask;
use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;

use crate::package::Package;

/// One frame, ready for both readers.
struct Art {
    mask: AlphaMask,
    /// The frame as a `data:` URL, built once.
    url: String,
}

/// A loaded Character with its art resolved.
pub struct Cast {
    character: Character,
    frames: BTreeMap<String, Art>,
    /// Each Animation's frames as `data:` URLs, in play order.
    ///
    /// Built alongside `frames` rather than derived from it on demand, so that
    /// this list and the index `draw` returns cannot come from two different
    /// walks of the same Animation. One that silently dropped a frame would
    /// shift every later index, and the sprite on screen would stop being the
    /// one the hit-test is measuring.
    art: BTreeMap<String, Vec<String>>,
}

/// What the renderer needs to draw one tick.
pub struct Drawn<'a> {
    pub mask: &'a AlphaMask,
    /// The frame's size in pixels, before the integer blow-up.
    pub art_size: (i32, i32),
    /// Which frame of the Animation is on screen.
    pub index: usize,
}

impl Cast {
    /// Resolve every frame the Character declares.
    ///
    /// Every frame is art the loader has already opened, so a failure here is a
    /// disagreement between this and the loader rather than a bad package.
    pub fn new(package: Package, alpha_threshold: u8) -> Result<Self, String> {
        let mut frames: BTreeMap<String, Art> = BTreeMap::new();
        let mut art = BTreeMap::new();

        for (name, animation) in &package.character.animations {
            let mut urls = Vec::with_capacity(animation.frames.len());

            for frame in &animation.frames {
                if !frames.contains_key(frame) {
                    // Two Animations may share a frame; the art is resolved
                    // once and named twice.
                    let bytes = package
                        .files
                        .get(frame)
                        .ok_or_else(|| format!("{frame} is not in the package"))?;
                    frames.insert(
                        frame.clone(),
                        Art {
                            mask: AlphaMask::from_png(bytes, alpha_threshold)?,
                            url: format!("data:image/png;base64,{}", STANDARD.encode(bytes)),
                        },
                    );
                }
                urls.push(frames[frame].url.clone());
            }

            art.insert(name.clone(), urls);
        }

        Ok(Self {
            character: package.character,
            frames,
            art,
        })
    }

    pub fn name(&self) -> &str {
        &self.character.name
    }

    /// Which frame of `animation` is on screen `animation_ms` after it started.
    ///
    /// The arithmetic is `character::Animation::frame_at`, which is where fps
    /// and loop mode come from the Character Manifest rather than a constant.
    /// This only looks up the Animation and the art the index lands on.
    ///
    /// `None` only for an Animation this Character does not have, which a
    /// validated Character cannot be asked for: the Engine names one of the
    /// eight required Animations, and a package missing one was rejected.
    /// Substituting a different Animation would be worse than drawing nothing,
    /// because the webview would still be told the name it asked for.
    pub fn draw(&self, animation: &str, animation_ms: u32) -> Option<Drawn<'_>> {
        let animation = self.character.animations.get(animation)?;
        let index = animation.frame_at(animation_ms);
        let art = self.frames.get(animation.frames.get(index)?)?;

        Some(Drawn {
            mask: &art.mask,
            art_size: (animation.frame_size.0 as i32, animation.frame_size.1 as i32),
            index,
        })
    }

    /// Every Animation's frames as `data:` URLs, for the webview to draw from.
    ///
    /// Handed over once. The webview picks a frame out of this by the name and
    /// index each `Frame` carries, so it still holds no authoritative state —
    /// only the art, which is not state.
    pub fn art(&self) -> &BTreeMap<String, Vec<String>> {
        &self.art
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ai_buddy_core::character::{PackageBytes, CHARACTER_MANIFEST_FILE, REQUIRED_ANIMATIONS};
    use ai_buddy_core::overlay::SpriteRect;

    /// A 2x2 RGBA frame whose top-left pixel is transparent.
    const PATCHY: &[u8] = include_bytes!("../../crates/core/tests/fixtures/alpha-2x2.png");

    /// A 2x2 RGBA frame with every pixel drawn, so one lookup tells a mask
    /// built from it apart from one built from `PATCHY`.
    const SOLID: &[u8] = include_bytes!("../../crates/core/tests/fixtures/opaque-2x2.png");

    /// A 2x2 greyscale frame. The loader takes it, because size comes from the
    /// PNG header; `AlphaMask::from_png` refuses it, because a mask needs the
    /// pixels. That gap is the only way art gets this far and fails.
    const GREYSCALE: &[u8] = include_bytes!("../../crates/core/tests/fixtures/greyscale-2x2.png");

    /// Matches `ALPHA_THRESHOLD` in `main`, which is the only caller.
    const THRESHOLD: u8 = 128;

    /// Every Animation in a test package plays at this, so one frame is 125ms.
    const FPS: u32 = 8;

    /// One Animation as these tests declare it: its name, then each frame as a
    /// file name and the bytes behind it.
    type Declared<'a> = (&'a str, &'a [(&'a str, &'a [u8])]);

    /// A Cast whose Animations are `animations`, plus one frame each for every
    /// required Animation they do not name.
    ///
    /// No disk: `Cast::new` takes bytes, and getting them off one is
    /// `package`'s job rather than this module's.
    fn cast(animations: &[Declared<'_>]) -> Result<Cast, String> {
        let mut manifest = String::from("name = Blip\n");
        let mut files = PackageBytes::new();

        let mut declare = |name: &str, frames: &[(&str, &[u8])]| {
            let names: Vec<&str> = frames.iter().map(|(file, _)| *file).collect();
            manifest.push_str(&format!("animation {name} = {}\n", names.join(" ")));
            manifest.push_str(&format!("fps {name} = {FPS}\n"));
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
        files.insert(
            ai_buddy_core::character::PERSONALITY_FILE.to_string(),
            b"Blip is cheerful.".to_vec(),
        );

        let character = ai_buddy_core::character::load(&files).expect("the package is valid");
        Cast::new(Package { character, files }, THRESHOLD)
    }

    /// The `data:` URL `art` should carry for a frame of these bytes.
    fn url(bytes: &[u8]) -> String {
        format!("data:image/png;base64,{}", STANDARD.encode(bytes))
    }

    /// Whether the mask says the frame's top-left pixel is drawn. `SOLID`'s is
    /// and `PATCHY`'s is not, which is how one frame's mask is told from the
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
        )
    }

    /// The invariant the whole module exists for: the webview indexes this list
    /// by the index the Shell computed, so a dropped or reordered URL would put
    /// a different frame on screen from the one the hit-test measured.
    #[test]
    fn an_animations_urls_stand_in_the_order_its_frames_do() {
        let cast = cast(&[(
            "walk",
            &[("a.png", PATCHY), ("b.png", SOLID), ("c.png", PATCHY)],
        )])
        .expect("the art resolves");

        assert_eq!(
            cast.art()["walk"],
            vec![url(PATCHY), url(SOLID), url(PATCHY)]
        );
        assert_eq!(
            cast.art()["walk"].len(),
            cast.character.animations["walk"].frames.len()
        );
    }

    /// The frame two Animations share is resolved once and named twice, so the
    /// second Animation to reach it takes the branch that resolves nothing. It
    /// still has to append the URL, or every later frame of that Animation
    /// answers to the index before its own.
    #[test]
    fn a_frame_two_animations_share_stands_at_each_animations_own_index() {
        let cast = cast(&[
            ("idle", &[("shared.png", PATCHY), ("bob.png", SOLID)]),
            ("sit", &[("down.png", SOLID), ("shared.png", PATCHY)]),
        ])
        .expect("the art resolves");

        assert_eq!(cast.art()["idle"], vec![url(PATCHY), url(SOLID)]);
        assert_eq!(cast.art()["sit"], vec![url(SOLID), url(PATCHY)]);
    }

    #[test]
    fn draw_returns_the_frame_the_declared_cadence_has_reached() {
        let cast =
            cast(&[("idle", &[("a.png", PATCHY), ("b.png", SOLID)])]).expect("the art resolves");

        let first = cast.draw("idle", 124).expect("idle is declared");
        assert_eq!(first.index, 0, "still inside the first of two 125ms frames");
        assert_eq!(first.art_size, (2, 2));
        assert!(!corner_drawn(&first), "the mask is the one PATCHY makes");

        let second = cast.draw("idle", 125).expect("idle is declared");
        assert_eq!(second.index, 1);
        assert!(
            corner_drawn(&second),
            "and the mask moves to the frame the index landed on"
        );

        let wrapped = cast.draw("idle", 250).expect("idle is declared");
        assert_eq!(wrapped.index, 0, "a looping strip comes back round");
        assert!(!corner_drawn(&wrapped));
    }

    /// Nothing rather than a substitute: the webview was told the name it asked
    /// for, so drawing a different Animation under it would be a lie the
    /// hit-test also believed.
    #[test]
    fn an_animation_the_character_does_not_have_draws_nothing() {
        let cast = cast(&[]).expect("the art resolves");
        assert!(cast.draw("cartwheel", 0).is_none());
    }

    /// Art the loader accepted and this cannot open. `load_character` reports
    /// it and tries the next candidate, which is why the failure names the
    /// frame rather than only the package.
    #[test]
    fn a_frame_no_mask_can_be_built_from_is_refused() {
        let Err(why) = cast(&[("idle", &[("idle-0.png", GREYSCALE)])]) else {
            panic!("a greyscale frame has no alpha to build a mask from");
        };
        assert!(why.contains("RGBA"), "{why}");
    }
}
