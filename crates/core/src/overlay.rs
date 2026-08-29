//! Hit-testing the Character's sprite against the cursor.
//!
//! Click-through on macOS is per-window, not per-pixel: a small sprite in a
//! screen-sized transparent window swallows every click unless something decides,
//! per cursor position, whether the pixel under it is actually drawn. That
//! decision is this module. It is pure arithmetic so it can be tested without a
//! windowing system.
//!
//! Placing the overlay is here for the same reason. Which display the window
//! covers and where the art lands inside it are two more sums the Shell would
//! otherwise do by hand beside the window server, where nothing can check them.

use crate::window_source::Rect;

/// Convert a global cursor reading into overlay-window coordinates, in points.
///
/// Both inputs are "physical pixels", but they are not the same physical space.
/// The windowing layer scales the global cursor by the *primary* display's scale
/// factor and the window's own origin by the *window's* scale factor. On a
/// mixed-DPI desktop those differ, so subtracting the two raw readings and
/// dividing once yields a hit region nowhere near the sprite. Each reading has
/// to be returned to points using the factor that produced it.
pub fn cursor_in_window(
    cursor_physical: (f64, f64),
    cursor_scale: f64,
    window_origin_physical: (f64, f64),
    window_scale: f64,
) -> (i32, i32) {
    let cursor_x = cursor_physical.0 / cursor_scale;
    let cursor_y = cursor_physical.1 / cursor_scale;
    let origin_x = window_origin_physical.0 / window_scale;
    let origin_y = window_origin_physical.1 / window_scale;

    (
        (cursor_x - origin_x).round() as i32,
        (cursor_y - origin_y).round() as i32,
    )
}

/// The display a point is on, or the nearest one when it is on none.
///
/// The overlay covers one display at a time, so something has to name which.
/// Containment answers it almost always, and the fallback is for the moments it
/// cannot: displays need not be flush, so a cursor can sit in the gap between
/// two of them, and a thrown sprite passes outside the outermost edge before
/// physics catches it. The nearest display is the answer there because the
/// point is on its way into or out of that one, and because the alternative —
/// no display — leaves the overlay nowhere to be and the Character invisible.
///
/// A point on a shared edge belongs to the first display that reported it,
/// which is arbitrary and has to be: both answers are equally true.
///
/// `None` only when no display was reported, which is a machine with no screen.
pub fn display_for(point: (f64, f64), displays: &[Rect]) -> Option<Rect> {
    displays
        .iter()
        .min_by(|a, b| outside_by(point, a).total_cmp(&outside_by(point, b)))
        .copied()
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
/// every display shares. The webview draws in points from the overlay's
/// top-left, and the art hangs above the feet and is centred on them, so the
/// drawn rectangle sits half a width to the left and a whole height above.
///
/// The window origin arrives in physical pixels against the window's own scale
/// factor, exactly as it does for `cursor_in_window`, and has to be returned to
/// points before it can be subtracted from a position that is already in them.
pub fn place_sprite(
    contact: (f64, f64),
    window_origin_physical: (f64, f64),
    window_scale: f64,
    art_size: (i32, i32),
    scale: i32,
) -> SpriteRect {
    let origin_x = window_origin_physical.0 / window_scale;
    let origin_y = window_origin_physical.1 / window_scale;
    let (width, height) = art_size;

    SpriteRect {
        x: (contact.0 - origin_x).round() as i32 - width / 2,
        y: (contact.1 - origin_y).round() as i32 - height,
        scale,
    }
}

/// Where the sprite sits in the overlay window, and how far its art is blown up.
///
/// `x` and `y` are the sprite's top-left corner in window coordinates. `scale` is
/// the integer nearest-neighbour factor the art is rendered at.
pub struct SpriteRect {
    pub x: i32,
    pub y: i32,
    pub scale: i32,
}

/// Which pixels of an Animation frame are drawn, at the art's own resolution.
#[derive(Debug)]
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
    pub fn hit(&self, sprite: &SpriteRect, cursor_x: i32, cursor_y: i32) -> bool {
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
            !mask.hit(&sprite, 100, 200),
            "top-left corner is transparent"
        );
        assert!(mask.hit(&sprite, 108, 200), "third column is opaque");
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
        assert!(mask.hit(&sprite, 100, 200), "top-left corner is inside");
        assert!(mask.hit(&sprite, 115, 207), "bottom-right corner is inside");

        assert!(!mask.hit(&sprite, 99, 204), "one point left of the sprite");
        assert!(
            !mask.hit(&sprite, 116, 204),
            "one point right of the sprite"
        );
        assert!(!mask.hit(&sprite, 108, 199), "one point above the sprite");
        assert!(!mask.hit(&sprite, 108, 208), "one point below the sprite");
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

        assert!(!mask.hit(&sprite, 0, 0), "alpha 0 is clear");
        assert!(mask.hit(&sprite, 1, 0), "alpha 255 is opaque");
        assert!(mask.hit(&sprite, 0, 1), "alpha 200 is above the threshold");
        assert!(!mask.hit(&sprite, 1, 1), "alpha 10 is below the threshold");
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

    /// Mixed-DPI is the case that matters: a 1x primary display beside a 2x
    /// built-in. The two inputs arrive in different physical spaces, so each has
    /// to be undone with the factor that produced it.
    #[test]
    fn cursor_and_window_are_converted_with_their_own_scale_factors() {
        // Cursor sits at 500,300 in points. The primary display is 1x, so its
        // physical reading is the same numbers.
        // The overlay's top-left is at 100,50 in points on a 2x display, so its
        // physical reading is doubled.
        let (x, y) = cursor_in_window((500.0, 300.0), 1.0, (200.0, 100.0), 2.0);

        assert_eq!((x, y), (400, 250), "500-100 across, 300-50 down, in points");
    }

    #[test]
    fn a_single_scale_desktop_still_converts() {
        let (x, y) = cursor_in_window((800.0, 600.0), 2.0, (200.0, 100.0), 2.0);

        assert_eq!((x, y), (300, 250), "400-100 across, 300-50 down, in points");
    }

    /// The art hangs above the feet and is centred on them, so a Character
    /// standing at a point is drawn above and to the left of it.
    #[test]
    fn the_art_hangs_above_the_contact_point_and_is_centred_on_it() {
        let sprite = place_sprite((500.0, 300.0), (0.0, 0.0), 1.0, (128, 128), 4);

        assert_eq!(
            (sprite.x, sprite.y),
            (436, 172),
            "half a width left, a whole height up"
        );
    }

    /// The same trap as `cursor_in_window`: the window origin arrives in
    /// physical pixels against the window's own scale factor, and the Frame
    /// arrives in points. Subtracting them raw puts the art nowhere near the
    /// Character on any display that is not 1x.
    #[test]
    fn the_window_origin_is_undone_with_the_windows_own_scale() {
        let sprite = place_sprite((500.0, 300.0), (200.0, 100.0), 2.0, (128, 128), 4);

        assert_eq!(
            (sprite.x, sprite.y),
            (336, 122),
            "origin is 100,50 in points"
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
    /// a single display. `display_for` compares rectangles and never sees a
    /// scale factor: converting a display into points is `window_source`'s job
    /// and is tested there. These are the sizes that conversion produces for a
    /// 1080p display beside a Retina one, which is the arrangement the bug was
    /// reported on.
    ///
    /// The seam at 1920 is the point of the fixture. Every case below is about
    /// which side of it a point falls on.
    fn two_displays() -> [Rect; 2] {
        [
            rect(0.0, 0.0, 1920.0, 1080.0),
            rect(1920.0, 0.0, 1728.0, 1117.0),
        ]
    }

    #[test]
    fn a_point_on_a_display_belongs_to_that_display() {
        let displays = two_displays();

        assert_eq!(display_for((960.0, 540.0), &displays), Some(displays[0]));
        assert_eq!(display_for((2600.0, 500.0), &displays), Some(displays[1]));
        assert_eq!(
            display_for((1920.5, 500.0), &displays),
            Some(displays[1]),
            "half a point across the seam is across it"
        );
        assert_eq!(
            display_for((1920.0, 500.0), &displays),
            Some(displays[0]),
            "the seam itself goes to whichever display reported first"
        );
    }

    /// Displays are not always flush, and a cursor can sit in the gap between
    /// two of them. The overlay still has to be somewhere.
    #[test]
    fn a_point_in_the_gap_between_displays_belongs_to_the_nearer_one() {
        let displays = [
            rect(0.0, 0.0, 1920.0, 1080.0),
            rect(2000.0, 0.0, 1728.0, 1117.0),
        ];

        assert_eq!(display_for((1930.0, 500.0), &displays), Some(displays[0]));
        assert_eq!(display_for((1990.0, 500.0), &displays), Some(displays[1]));
    }

    /// A Throw carries the sprite past the outermost edge before physics
    /// catches it, and a display can be unplugged with the sprite on it.
    #[test]
    fn a_point_off_every_display_belongs_to_the_nearest_one() {
        let displays = two_displays();

        assert_eq!(
            display_for((4000.0, 2000.0), &displays),
            Some(displays[1]),
            "off the bottom-right corner of the desktop"
        );
        assert_eq!(
            display_for((-500.0, -500.0), &displays),
            Some(displays[0]),
            "off the top-left corner of the desktop"
        );
    }

    #[test]
    fn no_displays_is_nowhere_to_put_the_overlay() {
        assert_eq!(display_for((960.0, 540.0), &[]), None);
    }
}
