//! Hit-testing the Character's sprite against the cursor.
//!
//! Click-through on macOS is per-window, not per-pixel: a small sprite in a
//! screen-sized transparent window swallows every click unless something decides,
//! per cursor position, whether the pixel under it is actually drawn. That
//! decision is this module. It is pure arithmetic so it can be tested without a
//! windowing system.
//!
//! Placing the overlays is here for the same reason. There is one overlay per
//! display, so which one the cursor is on and where the art lands inside each of
//! them are sums the Shell would otherwise do by hand beside the window server,
//! where nothing can check them.
//!
//! Everything here is in points, the space every display shares and the Engine
//! works in. Physical pixels stop at `window_source::in_points`.

use crate::window_source::Rect;

/// The index of the display a point is on, or of the nearest one when it is on
/// none.
///
/// Every display has an overlay, so this names the overlay a point belongs to —
/// the one whose window a click there would reach.
///
/// Containment answers it almost always, and the fallback is for the moments it
/// cannot: displays need not be flush, so a cursor can sit in the gap between
/// two of them, and a thrown sprite passes outside the outermost edge before
/// physics catches it. The nearest display is the answer there because the point
/// is on its way into or out of that one.
///
/// Containment is half-open, because a window is: a display 1920 wide at x=0
/// covers columns 0 to 1919, and column 1920 is the next display's first. The
/// shared edge has to go to the display whose window is actually there, since
/// this is what decides which overlay stops being click-through — naming the
/// neighbour leaves the overlay under the cursor passing clicks through, and a
/// click on the sprite falls to whatever is beneath it.
///
/// `None` only when no display was reported, which is a machine with no screen.
pub fn display_index_for(point: (f64, f64), displays: &[Rect]) -> Option<usize> {
    displays
        .iter()
        .position(|display| covers(point, display))
        .or_else(|| {
            displays
                .iter()
                .enumerate()
                .min_by(|(_, a), (_, b)| outside_by(point, a).total_cmp(&outside_by(point, b)))
                .map(|(index, _)| index)
        })
}

/// The one overlay that draws an Instance's speech bubble and thinking
/// indicator (#178), or `None` when no overlay should.
///
/// Every overlay is handed every sprite and draws the part that falls inside
/// it, which is right for the art: the wrong display's copy simply clips
/// away. A bubble is not clipped geometry — the renderer keeps it readable by
/// pulling it back inside the display, so the copy on the wrong display comes
/// back into view at an edge. So a bubble is owned by exactly one overlay, the
/// one under the feet. The feet rather than the art's rectangle: a sprite
/// straddling a seam is still standing on one display, and the owner then
/// changes once, at the seam, instead of flickering as the art crosses.
///
/// Unlike `display_index_for`, feet on no display own nothing. That function
/// answers "which overlay is the cursor nearest", where an answer is always
/// needed; here the nearest display would clamp a bubble into view beside a
/// sprite that is off-screen — thrown past an edge, or on a display that was
/// just unplugged — which is the very symptom this exists to remove.
///
/// Two passes, because an edge is either a seam or a floor. Half-open first,
/// so a seam resolves one way only: feet on the shared edge of a vertical
/// stack belong to the display below. A floor is the edge #178 missed — a
/// display reserving nothing along one, as a second screen does, puts the
/// Engine's floor on it, and resting feet land exactly there. The second pass
/// picks those up, `FLOOR_SLACK` wide.
pub fn bubble_owner(feet: (f64, f64), displays: &[Rect]) -> Option<usize> {
    displays
        .iter()
        .position(|display| covers(feet, display))
        .or_else(|| {
            displays
                .iter()
                .position(|display| outside_by(feet, display) <= FLOOR_SLACK * FLOOR_SLACK)
        })
}

/// How far outside its display feet may be and still be standing on it, in
/// points.
///
/// Slack rather than equality: the floor is the display rectangle scaled and
/// clamped, which `usable_frame` argues need not land back on the edge it came
/// from. A point is far short of off-screen, so a thrown sprite still owns no
/// bubble. Squared at the call site, as `outside_by` is.
const FLOOR_SLACK: f64 = 1.0;

/// Whether a display's window has this point, right and bottom edges excluded.
fn covers(point: (f64, f64), rect: &Rect) -> bool {
    (rect.x..rect.x + rect.width).contains(&point.0)
        && (rect.y..rect.y + rect.height).contains(&point.1)
}

/// How far outside a rectangle a point lies, squared. Zero anywhere inside it.
///
/// Squared because only the ordering is read, and the square root that would
/// turn this into a distance reorders nothing.
fn outside_by(point: (f64, f64), rect: &Rect) -> f64 {
    let dx = (rect.x - point.0)
        .max(point.0 - (rect.x + rect.width))
        .max(0.0);
    let dy = (rect.y - point.1)
        .max(point.1 - (rect.y + rect.height))
        .max(0.0);

    dx * dx + dy * dy
}

/// Where to draw the art, given where the Character's feet are.
///
/// A `Frame` reports the contact point: the Character's feet, in the point space
/// every display shares. The art hangs above the feet and is centred on them, so
/// the drawn rectangle sits half a width to the left and a whole height above.
///
/// The answer is in that same shared space, which is the space the hit-test asks
/// its question in. `SpriteRect::in_overlay` is what turns it into one overlay's
/// own coordinates for drawing.
pub fn place_sprite(contact: (f64, f64), art_size: (i32, i32), scale: i32) -> SpriteRect {
    let (width, height) = art_size;

    SpriteRect {
        x: contact.0.round() as i32 - width / 2,
        y: contact.1.round() as i32 - height,
        scale,
    }
}

/// Where the sprite sits, and how far its art is blown up.
///
/// `x` and `y` are the sprite's top-left corner, in whichever space it was
/// placed in: `place_sprite` works in the shared point space, `in_overlay` moves
/// a copy into one overlay's own. `scale` is the integer nearest-neighbour
/// factor the art is rendered at.
#[derive(Clone, Copy)]
pub struct SpriteRect {
    pub x: i32,
    pub y: i32,
    pub scale: i32,
}

impl SpriteRect {
    /// The same rectangle in one overlay's coordinates: points from the
    /// top-left corner of the display that overlay covers.
    ///
    /// Every overlay is handed the sprite, including the ones it is nowhere
    /// near, and each draws the part that falls inside it. That is what makes a
    /// Character on a seam whole: both overlays are given the same rectangle in
    /// their own coordinates, so the halves they clip meet at the seam instead
    /// of overlapping or leaving a gap.
    ///
    /// No scale factor appears. Two displays with different backing factors
    /// share one point space, and the overlay's origin is its display's because
    /// the window is placed to cover that display exactly — which is what
    /// `scripts/verify-overlay.sh` asserts on a real desktop.
    pub fn in_overlay(&self, overlay: Rect) -> SpriteRect {
        SpriteRect {
            x: self.x - overlay.x.round() as i32,
            y: self.y - overlay.y.round() as i32,
            scale: self.scale,
        }
    }
}

/// Which pixels of an Animation frame are drawn, at the art's own resolution.
///
/// Comparable and clonable because a validated `character::Character` carries
/// one per distinct frame, and derives both.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AlphaMask {
    width: i32,
    height: i32,
    opaque: Vec<bool>,
}

impl AlphaMask {
    /// Build a mask from an ASCII picture: `#` is drawn, anything else is not.
    ///
    /// Tests only. It trusts every row to be the width of the first, which is
    /// fine for a literal in a test and wrong for anything else.
    #[cfg(test)]
    pub fn from_rows(rows: &[&str]) -> Self {
        let height = rows.len() as i32;
        let width = rows.first().map_or(0, |r| r.chars().count()) as i32;
        let opaque = rows
            .iter()
            .flat_map(|r| r.chars().map(|c| c == '#'))
            .collect();

        Self {
            width,
            height,
            opaque,
        }
    }

    /// The art's own dimensions, before any scaling.
    pub fn size(&self) -> (i32, i32) {
        (self.width, self.height)
    }

    /// Raw mask data for platform-specific input region APIs.
    ///
    /// Returns (width, height, row-major opaque bools). X11's XShapeCombineMask
    /// needs this to carve the click-through region from the sprite's alpha.
    pub fn raw(&self) -> (i32, i32, &[bool]) {
        (self.width, self.height, &self.opaque)
    }

    /// Build a mask from an 8-bit RGBA PNG, treating alpha at or above
    /// `threshold` as drawn.
    ///
    /// A threshold rather than "alpha > 0" so anti-aliased edges on
    /// hand-drawn art do not grow an invisible one-pixel border that
    /// swallows clicks.
    pub fn from_png(bytes: &[u8], threshold: u8) -> Result<Self, String> {
        let mut reader = png::Decoder::new(std::io::Cursor::new(bytes))
            .read_info()
            .map_err(|e| format!("sprite is not a readable PNG: {e}"))?;

        let size = reader
            .output_buffer_size()
            .ok_or_else(|| "sprite dimensions are implausibly large".to_string())?;
        let mut buf = vec![0; size];
        let info = reader
            .next_frame(&mut buf)
            .map_err(|e| format!("sprite has no readable frame: {e}"))?;

        if info.color_type != png::ColorType::Rgba || info.bit_depth != png::BitDepth::Eight {
            return Err(format!(
                "sprite must be 8-bit RGBA, found {:?} at {:?}",
                info.color_type, info.bit_depth
            ));
        }

        let opaque = buf[..info.buffer_size()]
            .chunks_exact(4)
            .map(|px| px[3] >= threshold)
            .collect();

        Ok(Self {
            width: info.width as i32,
            height: info.height as i32,
            opaque,
        })
    }

    /// Whether the cursor is over a drawn pixel of the sprite.
    ///
    /// A cursor outside the sprite's rectangle is never a hit. The bounds are
    /// checked before the divide because integer division truncates toward zero,
    /// so a cursor just left of the sprite would otherwise land on column 0.
    ///
    /// `mirrored` samples column `width - 1 - px`: the renderer flips the art
    /// about the box's center when the sprite faces left, and at an integer
    /// scale that is exactly the reversed art column, so the pixels this feels
    /// are the pixels the user sees.
    pub fn hit(&self, sprite: &SpriteRect, cursor_x: i32, cursor_y: i32, mirrored: bool) -> bool {
        let local_x = cursor_x - sprite.x;
        let local_y = cursor_y - sprite.y;
        if local_x < 0 || local_y < 0 {
            return false;
        }

        let px = local_x / sprite.scale;
        let py = local_y / sprite.scale;
        if px >= self.width || py >= self.height {
            return false;
        }

        let px = if mirrored { self.width - 1 - px } else { px };
        self.opaque[(py * self.width + px) as usize]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `#` is an opaque pixel, `.` is fully transparent.
    #[test]
    fn cursor_over_transparent_pixel_is_not_a_hit() {
        let mask = AlphaMask::from_rows(&["..##..", ".####."]);
        let sprite = SpriteRect {
            x: 100,
            y: 200,
            scale: 4,
        };

        assert!(
            !mask.hit(&sprite, 100, 200, false),
            "top-left corner is transparent"
        );
        assert!(mask.hit(&sprite, 108, 200, false), "third column is opaque");
    }

    /// Facing left, the renderer draws the art flipped, so the clickable
    /// pixels must flip with it or an asymmetric sprite is clickable where it
    /// is empty and click-through where it is drawn.
    #[test]
    fn a_mirrored_sprite_hit_tests_against_the_flipped_columns() {
        let mask = AlphaMask::from_rows(&["##....", "##...."]);
        let sprite = SpriteRect {
            x: 100,
            y: 200,
            scale: 4,
        };

        assert!(
            mask.hit(&sprite, 100, 200, false),
            "as authored, the ink is on the left"
        );
        assert!(
            !mask.hit(&sprite, 100, 200, true),
            "mirrored, the left edge is clear"
        );
        assert!(
            mask.hit(&sprite, 123, 200, true),
            "and the ink is on the right"
        );
        assert!(!mask.hit(&sprite, 123, 200, false));
    }

    #[test]
    fn cursor_outside_the_sprite_is_not_a_hit() {
        let mask = AlphaMask::from_rows(&["####", "####"]);
        let sprite = SpriteRect {
            x: 100,
            y: 200,
            scale: 4,
        };

        // The sprite covers 100..116 horizontally, 200..208 vertically.
        assert!(
            mask.hit(&sprite, 100, 200, false),
            "top-left corner is inside"
        );
        assert!(
            mask.hit(&sprite, 115, 207, false),
            "bottom-right corner is inside"
        );

        assert!(
            !mask.hit(&sprite, 99, 204, false),
            "one point left of the sprite"
        );
        assert!(
            !mask.hit(&sprite, 116, 204, false),
            "one point right of the sprite"
        );
        assert!(
            !mask.hit(&sprite, 108, 199, false),
            "one point above the sprite"
        );
        assert!(
            !mask.hit(&sprite, 108, 208, false),
            "one point below the sprite"
        );
    }

    /// The fixture is a 2x2 RGBA PNG whose alpha values are, row-major:
    /// 0 (clear), 255 (opaque), 200 (mostly opaque), 10 (nearly clear).
    #[test]
    fn png_alpha_above_the_threshold_is_opaque() {
        let mask = AlphaMask::from_png(include_bytes!("../tests/fixtures/alpha-2x2.png"), 128)
            .expect("fixture decodes");
        let sprite = SpriteRect {
            x: 0,
            y: 0,
            scale: 1,
        };

        assert!(!mask.hit(&sprite, 0, 0, false), "alpha 0 is clear");
        assert!(mask.hit(&sprite, 1, 0, false), "alpha 255 is opaque");
        assert!(
            mask.hit(&sprite, 0, 1, false),
            "alpha 200 is above the threshold"
        );
        assert!(
            !mask.hit(&sprite, 1, 1, false),
            "alpha 10 is below the threshold"
        );
    }

    #[test]
    fn a_png_without_an_alpha_channel_is_rejected_by_name() {
        let err = AlphaMask::from_png(include_bytes!("../tests/fixtures/greyscale-2x2.png"), 128)
            .expect_err("greyscale has no alpha to hit-test against");

        assert!(
            err.contains("8-bit RGBA"),
            "error states the requirement: {err}"
        );
        assert!(
            err.contains("Grayscale"),
            "error names what was found: {err}"
        );
    }

    /// The art hangs above the feet and is centred on them, so a Character
    /// standing at a point is drawn above and to the left of it.
    #[test]
    fn the_art_hangs_above_the_contact_point_and_is_centred_on_it() {
        let sprite = place_sprite((500.0, 300.0), (128, 128), 4);

        assert_eq!(
            (sprite.x, sprite.y),
            (436, 172),
            "half a width left, a whole height up"
        );
    }

    fn rect(x: f64, y: f64, width: f64, height: f64) -> Rect {
        Rect {
            x,
            y,
            width,
            height,
        }
    }

    /// Two displays side by side, the second starting where the first ends.
    ///
    /// Literal points, so this runs the same on any machine, including one with
    /// a single display. Nothing here sees a scale factor: converting a display
    /// into points is `window_source`'s job and is tested there. These are the
    /// sizes that conversion produces for a 1080p display beside a Retina one,
    /// which is the arrangement the bug was reported on — 1x beside 2x, sharing
    /// one point space.
    ///
    /// The seam at 1920 is the point of the fixture. Every case below is about
    /// which side of it a point falls on.
    fn two_displays() -> [Rect; 2] {
        [
            rect(0.0, 0.0, 1920.0, 1080.0),
            rect(1920.0, 0.0, 1728.0, 1117.0),
        ]
    }

    /// The issue this module is here for: a Character standing on the seam is
    /// drawn by both overlays, and the halves have to meet.
    #[test]
    fn a_sprite_on_a_seam_is_two_halves_that_meet() {
        let displays = two_displays();
        // Feet on the seam. 128 points of art, so 64 fall on each display.
        let sprite = place_sprite((1920.0, 700.0), (128, 128), 4);

        let left = sprite.in_overlay(displays[0]);
        let right = sprite.in_overlay(displays[1]);

        assert_eq!((left.x, left.y), (1856, 572), "overlay 0 draws from 1856");
        assert_eq!((right.x, right.y), (-64, 572), "overlay 1 draws from -64");
        assert_eq!(
            1920 - left.x,
            -right.x,
            "the columns overlay 0 runs out of room for are the ones overlay 1 hides \
             behind its own left edge: no gap and no overlap"
        );
    }

    /// Displays stack vertically too, and the origin subtracted is the whole
    /// origin rather than an x.
    #[test]
    fn an_overlay_below_another_subtracts_its_own_top_edge() {
        let displays = [
            rect(0.0, 0.0, 1920.0, 1080.0),
            rect(0.0, 1080.0, 1920.0, 1080.0),
        ];
        let sprite = place_sprite((500.0, 1080.0), (128, 128), 4);

        assert_eq!(
            sprite.in_overlay(displays[0]).y,
            952,
            "128 up from the seam"
        );
        assert_eq!(sprite.in_overlay(displays[1]).y, -128, "and 128 above it");
    }

    /// A display whose origin is not a whole number of points, which fractional
    /// scaling produces. The sprite still lands on the pixel grid.
    #[test]
    fn a_fractional_display_origin_is_rounded_to_the_pixel_grid() {
        let sprite = place_sprite((500.0, 300.0), (128, 128), 4);

        // Both halves round up, so truncating either one is a wrong answer.
        assert_eq!(
            sprite.in_overlay(rect(100.6, 50.6, 800.0, 600.0)).x,
            335,
            "436 - 101"
        );
        assert_eq!(
            sprite.in_overlay(rect(100.6, 50.6, 800.0, 600.0)).y,
            121,
            "172 - 51"
        );
    }

    /// #178: a bubble is drawn by exactly one overlay, the one under the
    /// feet — and by none when the feet are on no display, where the
    /// cursor's nearest-display fallback would clamp a bubble into view
    /// beside a sprite that is not there.
    #[test]
    fn a_bubble_belongs_to_the_display_under_the_feet_or_to_none() {
        let displays = two_displays();

        // Art wide enough to straddle the seam; the feet are still on the first.
        assert_eq!(bubble_owner((1919.9, 1000.0), &displays), Some(0));
        assert_eq!(
            bubble_owner((1920.0, 1000.0), &displays),
            Some(1),
            "ownership flips once, at the seam column, and nowhere else"
        );

        assert_eq!(
            bubble_owner((4000.0, 500.0), &displays),
            None,
            "thrown past the right edge: no overlay draws a bubble"
        );
        assert_eq!(
            bubble_owner((960.0, 1200.0), &displays),
            None,
            "below the first display's bottom edge, with nothing beneath it"
        );
        assert_eq!(
            display_index_for((4000.0, 500.0), &displays),
            Some(1),
            "where the cursor's question still gets its nearest answer"
        );
    }

    /// #178 read every edge as a seam. An edge with nothing beyond it is a
    /// floor a Character comes to rest on instead — see `bubble_owner`.
    #[test]
    fn an_edge_is_a_seam_when_a_display_lies_beyond_it_and_a_floor_when_none_does() {
        let displays = two_displays();

        assert_eq!(
            bubble_owner((960.0, 1080.0), &displays),
            Some(0),
            "standing on the first display's bottom edge, not off the desktop"
        );
        assert_eq!(
            bubble_owner((3648.0, 500.0), &displays),
            Some(1),
            "and where a walk ends against the outermost side"
        );

        let a_hair_adrift = f64::from_bits(1080.0_f64.to_bits() + 1);
        assert_eq!(
            bubble_owner((960.0, a_hair_adrift), &displays),
            Some(0),
            "a floor one unit in the last place adrift is still that floor"
        );

        let stacked = [
            rect(0.0, 0.0, 1920.0, 1080.0),
            rect(0.0, 1080.0, 1920.0, 1080.0),
        ];
        assert_eq!(
            bubble_owner((960.0, 1080.0), &stacked),
            Some(1),
            "and a display below turns that same edge back into a seam"
        );
    }

    /// #163 with #178: several Instances are several bubbles. Ownership is a
    /// function of one Instance's feet, so two characters standing on two
    /// displays each own a bubble on their own display at the same time —
    /// the case a fix aimed at "only one bubble" could wrongly suppress.
    #[test]
    fn each_instance_owns_a_bubble_on_the_display_it_stands_on() {
        let displays = two_displays();

        let first = bubble_owner((960.0, 1000.0), &displays);
        let second = bubble_owner((2600.0, 1000.0), &displays);

        assert_eq!(first, Some(0));
        assert_eq!(second, Some(1));
        assert_ne!(
            first, second,
            "two characters on two displays are two bubbles, not one"
        );

        assert_eq!(
            bubble_owner((100.0, 1000.0), &displays),
            first,
            "and two characters on the same display share that display's overlay"
        );
    }

    #[test]
    fn a_point_on_a_display_belongs_to_that_display() {
        let displays = two_displays();

        assert_eq!(display_index_for((960.0, 540.0), &displays), Some(0));
        assert_eq!(display_index_for((2600.0, 500.0), &displays), Some(1));
        assert_eq!(
            display_index_for((1920.5, 500.0), &displays),
            Some(1),
            "half a point across the seam is across it"
        );
        assert_eq!(
            display_index_for((1920.0, 500.0), &displays),
            Some(1),
            "the seam column is the second display's first column, not the first \
             display's last: that is where its window starts"
        );
    }

    /// Displays are not always flush, and a cursor can sit in the gap between
    /// two of them. The click still belongs to one overlay.
    #[test]
    fn a_point_in_the_gap_between_displays_belongs_to_the_nearer_one() {
        let displays = [
            rect(0.0, 0.0, 1920.0, 1080.0),
            rect(2000.0, 0.0, 1728.0, 1117.0),
        ];

        assert_eq!(display_index_for((1930.0, 500.0), &displays), Some(0));
        assert_eq!(display_index_for((1990.0, 500.0), &displays), Some(1));
    }

    /// A Throw carries the sprite past the outermost edge before physics
    /// catches it, and a display can be unplugged with the sprite on it.
    #[test]
    fn a_point_off_every_display_belongs_to_the_nearest_one() {
        let displays = two_displays();

        assert_eq!(
            display_index_for((4000.0, 2000.0), &displays),
            Some(1),
            "off the bottom-right corner of the desktop"
        );
        assert_eq!(
            display_index_for((-500.0, -500.0), &displays),
            Some(0),
            "off the top-left corner of the desktop"
        );
    }

    #[test]
    fn no_displays_is_no_overlay_to_belong_to() {
        assert_eq!(display_index_for((960.0, 540.0), &[]), None);
    }
}
