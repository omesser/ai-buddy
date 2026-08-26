//! Hit-testing the Character's sprite against the cursor.
//!
//! Click-through on macOS is per-window, not per-pixel: a small sprite in a
//! screen-sized transparent window swallows every click unless something decides,
//! per cursor position, whether the pixel under it is actually drawn. That
//! decision is this module. It is pure arithmetic so it can be tested without a
//! windowing system.

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
    pub fn from_rows(rows: &[&str]) -> Self {
        let height = rows.len() as i32;
        let width = rows.first().map_or(0, |r| r.chars().count()) as i32;
        let opaque = rows
            .iter()
            .flat_map(|r| r.chars().map(|c| c == '#'))
            .collect();

        Self { width, height, opaque }
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
        let sprite = SpriteRect { x: 100, y: 200, scale: 4 };

        assert!(!mask.hit(&sprite, 100, 200), "top-left corner is transparent");
        assert!(mask.hit(&sprite, 108, 200), "third column is opaque");
    }

    #[test]
    fn cursor_outside_the_sprite_is_not_a_hit() {
        let mask = AlphaMask::from_rows(&["####", "####"]);
        let sprite = SpriteRect { x: 100, y: 200, scale: 4 };

        // The sprite covers 100..116 horizontally, 200..208 vertically.
        assert!(mask.hit(&sprite, 100, 200), "top-left corner is inside");
        assert!(mask.hit(&sprite, 115, 207), "bottom-right corner is inside");

        assert!(!mask.hit(&sprite, 99, 204), "one point left of the sprite");
        assert!(!mask.hit(&sprite, 116, 204), "one point right of the sprite");
        assert!(!mask.hit(&sprite, 108, 199), "one point above the sprite");
        assert!(!mask.hit(&sprite, 108, 208), "one point below the sprite");
    }

    /// The fixture is a 2x2 RGBA PNG whose alpha values are, row-major:
    /// 0 (clear), 255 (opaque), 200 (mostly opaque), 10 (nearly clear).
    #[test]
    fn png_alpha_above_the_threshold_is_opaque() {
        let mask = AlphaMask::from_png(include_bytes!("../tests/fixtures/alpha-2x2.png"), 128)
            .expect("fixture decodes");
        let sprite = SpriteRect { x: 0, y: 0, scale: 1 };

        assert!(!mask.hit(&sprite, 0, 0), "alpha 0 is clear");
        assert!(mask.hit(&sprite, 1, 0), "alpha 255 is opaque");
        assert!(mask.hit(&sprite, 0, 1), "alpha 200 is above the threshold");
        assert!(!mask.hit(&sprite, 1, 1), "alpha 10 is below the threshold");
    }

    #[test]
    fn a_png_without_an_alpha_channel_is_rejected_by_name() {
        let err = AlphaMask::from_png(
            include_bytes!("../tests/fixtures/greyscale-2x2.png"),
            128,
        )
        .expect_err("greyscale has no alpha to hit-test against");

        assert!(err.contains("8-bit RGBA"), "error states the requirement: {err}");
        assert!(err.contains("Grayscale"), "error names what was found: {err}");
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
}
