//! Which display an Instance is on, and where its feet sit in that display.
//!
//! Identity is the index in the list `read_displays` last returned. Tauri's
//! Monitor has no native id. macOS does not keep CGDirectDisplayID, Windows
//! does not keep HMONITOR, and Linux does not keep the GDK monitor. The index
//! lasts for one arrangement. The next successful `read_displays` replaces the
//! list. Wayland with no X server still does not refresh the display cache.

use crate::engine::Point;
use crate::overlay::bubble_owner;
use crate::window_source::Rect;

/// One connected display, in platform enumeration order.
#[derive(Clone, Copy, Debug)]
pub struct Screen {
    index: usize,
    frame: Rect,
    usable: Rect,
}

impl Screen {
    /// Where this display sits in the enumeration.
    pub fn index(&self) -> usize {
        self.index
    }

    /// This display's full frame.
    pub fn frame(&self) -> Rect {
        self.frame
    }

    /// The usable rectangle for this display.
    pub fn usable(&self) -> Rect {
        self.usable
    }
}

/// Feet in one display's full-frame coordinates.
#[derive(Clone, Copy, Debug)]
pub struct Placement {
    index: usize,
    x: f64,
    y: f64,
}

impl Placement {
    /// The display these feet are on.
    pub fn index(&self) -> usize {
        self.index
    }

    /// Offset from the frame's left edge.
    pub fn x(&self) -> f64 {
        self.x
    }

    /// Offset from the frame's top edge.
    pub fn y(&self) -> f64 {
        self.y
    }
}

/// What one Instance can report about the desktop right now.
#[derive(Clone, Debug)]
pub struct Whereabouts {
    displays: Vec<Screen>,
    place: Place,
}

#[derive(Clone, Copy, Debug)]
enum Place {
    On(Placement),
    Off,
}

impl Whereabouts {
    /// Connected displays, in enumeration order.
    pub fn displays(&self) -> &[Screen] {
        &self.displays
    }

    /// The display under the feet, when there is one.
    pub fn current(&self) -> Option<&Screen> {
        match self.place {
            Place::On(placement) => Some(&self.displays[placement.index]),
            Place::Off => None,
        }
    }

    /// Feet in that display's frame, when the feet are on one.
    pub fn placement(&self) -> Option<Placement> {
        match self.place {
            Place::On(placement) => Some(placement),
            Place::Off => None,
        }
    }

    /// One line for a session prompt.
    pub fn prompt_line(&self) -> String {
        match self.placement() {
            Some(place) => format!(
                "displays: {}; on display: {}; placement: ({}, {})",
                self.displays.len(),
                place.index(),
                place.x(),
                place.y()
            ),
            None => format!(
                "displays: {}; on display: none; placement: none",
                self.displays.len()
            ),
        }
    }

    /// Where `feet` are among `frames`.
    pub fn locate(feet: Point, frames: &[Rect], usable: &[Rect]) -> Self {
        // `bubble_owner` names the display under the feet, not `display_index_for`.
        // Nearest display names a display the feet are not on.
        let place = match bubble_owner((feet.x, feet.y), frames) {
            Some(index) => {
                let frame = frames[index];
                Place::On(Placement {
                    index,
                    x: feet.x - frame.x,
                    y: feet.y - frame.y,
                })
            }
            None => Place::Off,
        };
        let displays = frames
            .iter()
            .enumerate()
            .map(|(index, frame)| Screen {
                index,
                frame: *frame,
                usable: usable.get(index).copied().unwrap_or(*frame),
            })
            .collect();
        Whereabouts { displays, place }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::Point;
    use crate::window_source::Rect;

    fn frame(x: f64, y: f64, width: f64, height: f64) -> Rect {
        Rect {
            x,
            y,
            width,
            height,
        }
    }

    #[test]
    fn feet_inside_one_display_are_on_that_display() {
        let frames = [frame(0.0, 0.0, 1920.0, 1080.0)];
        let here = Whereabouts::locate(Point { x: 100.0, y: 200.0 }, &frames, &frames);

        assert_eq!(here.current().map(Screen::index), Some(0));
        let place = here.placement().expect("on the only display");
        assert_eq!(place.x(), 100.0);
        assert_eq!(place.y(), 200.0);
        assert_eq!(here.displays().len(), 1);
        assert_eq!(
            here.prompt_line(),
            "displays: 1; on display: 0; placement: (100, 200)"
        );
    }

    #[test]
    fn a_seam_with_a_display_below_belongs_to_the_lower_one() {
        let frames = [
            frame(0.0, 0.0, 1920.0, 1080.0),
            frame(0.0, 1080.0, 1920.0, 1080.0),
        ];
        let here = Whereabouts::locate(
            Point {
                x: 960.0,
                y: 1080.0,
            },
            &frames,
            &frames,
        );

        assert_eq!(here.current().map(Screen::index), Some(1));
        let place = here.placement().expect("on the lower display");
        assert_eq!(place.y(), 0.0);
    }

    #[test]
    fn feet_on_the_bottom_edge_stay_on_that_display() {
        let frames = [frame(0.0, 0.0, 1920.0, 1080.0)];
        let here = Whereabouts::locate(Point { x: 10.0, y: 1080.0 }, &frames, &frames);

        assert_eq!(here.current().map(Screen::index), Some(0));
        let place = here.placement().expect("still on the display");
        assert_eq!(place.y(), 1080.0);
    }

    #[test]
    fn two_displays_stay_in_enumeration_order() {
        let frames = [
            frame(0.0, 0.0, 1920.0, 1080.0),
            frame(1920.0, 0.0, 1920.0, 1080.0),
        ];
        let here = Whereabouts::locate(
            Point {
                x: 2000.0,
                y: 100.0,
            },
            &frames,
            &frames,
        );

        assert_eq!(here.displays().len(), 2);
        assert_eq!(here.displays()[1].frame().x, 1920.0);
        assert_eq!(here.current().map(Screen::index), Some(1));
        let place = here.placement().expect("on the right display");
        assert_eq!(place.index(), 1);
        assert_eq!(place.x(), 80.0);
        assert_eq!(place.y(), 100.0);
        assert_eq!(
            here.prompt_line(),
            "displays: 2; on display: 1; placement: (80, 100)"
        );
    }

    #[test]
    fn feet_in_a_gap_are_on_no_display() {
        let frames = [
            frame(0.0, 0.0, 1000.0, 1080.0),
            frame(1200.0, 0.0, 1000.0, 1080.0),
        ];
        let feet = Point {
            x: 1100.0,
            y: 100.0,
        };
        let here = Whereabouts::locate(feet, &frames, &frames);

        assert_eq!(here.displays().len(), 2);
        assert_eq!(here.current().map(Screen::index), None);
        assert_eq!(here.placement().map(|place| place.index()), None);
        assert_eq!(
            here.prompt_line(),
            "displays: 2; on display: none; placement: none"
        );

        let first_only = [frames[0]];
        let later = Whereabouts::locate(feet, &first_only, &first_only);
        assert_eq!(later.displays().len(), 1);
        assert_eq!(later.current().map(Screen::index), None);
        assert_eq!(later.placement().map(|place| place.x()), None);
    }

    #[test]
    fn a_frame_without_a_usable_rect_uses_its_frame() {
        let frames = [frame(0.0, 0.0, 1920.0, 1080.0)];
        let here = Whereabouts::locate(Point { x: 100.0, y: 200.0 }, &frames, &[]);

        assert_eq!(
            here.displays()[0].usable(),
            Rect {
                x: 0.0,
                y: 0.0,
                width: 1920.0,
                height: 1080.0,
            }
        );
        assert_eq!(
            here.displays()[0].frame(),
            Rect {
                x: 0.0,
                y: 0.0,
                width: 1920.0,
                height: 1080.0,
            }
        );

        let usable = [frame(0.0, 25.0, 1920.0, 1055.0)];
        let docked = Whereabouts::locate(Point { x: 100.0, y: 200.0 }, &frames, &usable);
        assert_eq!(
            docked.displays()[0].usable(),
            Rect {
                x: 0.0,
                y: 25.0,
                width: 1920.0,
                height: 1055.0,
            }
        );

        let extra = [frame(0.0, 25.0, 1920.0, 1055.0), frame(9.0, 9.0, 1.0, 1.0)];
        let dropped = Whereabouts::locate(Point { x: 100.0, y: 200.0 }, &frames, &extra);
        assert_eq!(dropped.displays().len(), 1);
    }

    #[test]
    fn no_displays_reports_off() {
        let here = Whereabouts::locate(Point { x: 10.0, y: 10.0 }, &[], &[]);

        assert_eq!(here.displays().len(), 0);
        assert_eq!(here.current().map(Screen::index), None);
        assert_eq!(here.placement().map(|place| place.index()), None);
        assert_eq!(
            here.prompt_line(),
            "displays: 0; on display: none; placement: none"
        );
    }
}
