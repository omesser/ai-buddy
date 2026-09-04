//! Spatial queries over a `WorldSnapshot`.

use super::{Point, Rect, Window, WorldSnapshot, CEILING_CLEARANCE};
use crate::window_source::DOCK_PERCH_ID;

/// A surface the sprite can come to rest on.
pub(super) struct Support {
    pub(super) y: f64,
    pub(super) surface: Surface,
}

/// What a Support is made of. The fact geometry reports — what standing on
/// one turns the sprite into is `transition`'s to say, not geometry's.
#[derive(Clone, Copy)]
pub(super) enum Surface {
    /// The bottom of a display.
    Floor,
    /// A window's top edge.
    Perch,
}

/// The first surface at or below `position`: the nearest window top edge, or
/// the floor when no window is in the way.
///
/// Window sides and bottoms are not surfaces, so a rectangle only counts when
/// the sprite is above its top edge. That is what lets the sprite rise through
/// a window from underneath instead of being trapped below it.
pub(super) fn support_below(position: Point, snapshot: &WorldSnapshot) -> Option<Support> {
    let floor = floor_under(position.x, snapshot).map(|y| Support {
        y,
        surface: Surface::Floor,
    });

    snapshot
        .windows
        .iter()
        .enumerate()
        .filter(|(index, window)| {
            window.rect.y >= position.y && is_perch(*index, position.x, snapshot)
        })
        .map(|(_, window)| Support {
            y: window.rect.y,
            surface: Surface::Perch,
        })
        .chain(floor)
        .min_by(|a, b| a.y.total_cmp(&b.y))
}

/// Whether the top edge of the window at `index` is somewhere the sprite can
/// stand at `x`.
///
/// Nearest support still wins over frontmost, so this only narrows the
/// candidates. Walking the windows in order and taking the first match would
/// instead have the sprite fall through an edge it can plainly see, whenever
/// the window in front of that edge is above the sprite and so no support at
/// all.
///
/// Three ways an edge is not a Perch, all of them a resting place the user
/// cannot see:
///
/// - Hidden behind a window drawn in front of it. That is what makes
///   `snapshot.windows` an ordered list rather than a set: an edge covered at
///   this x is not there to be seen, and the sprite resting on it is drawn
///   floating in the middle of the window that hides it. An edge still visible
///   at this x is a Perch whatever its depth.
/// - Over no display. Physics clamps to the union of the display frames and
///   not to the rectangle bounding them, so a window spanning two displays of
///   different heights hangs part of its edge over nothing. A sprite resting
///   out there is invisible and unclickable until the window moves.
/// - Too close to the usable top. The feet are the contact point and the art
///   hangs above them, so a title bar under the menu bar leaves only legs on
///   screen. #100.
pub(super) fn is_perch(index: usize, x: f64, snapshot: &WorldSnapshot) -> bool {
    let window = &snapshot.windows[index].rect;
    window.spans_x(x)
        && on_a_display(Point { x, y: window.y }, snapshot)
        && !snapshot.windows[..index].iter().any(|front| {
            front.rect.spans_x(x) && window.y >= front.rect.y && window.y <= front.rect.bottom()
        })
}

/// Whether the feet can be put down at `position`: some display covers it,
/// with the room the art needs above. The second of `is_perch`'s three
/// questions, on its own because a ride between polls has no window sample to
/// ask the other two of and still must not place the sprite out there. #128.
pub(super) fn on_a_display(position: Point, snapshot: &WorldSnapshot) -> bool {
    displays_spanning(position.x, snapshot).any(|display| {
        position.y >= display.y + CEILING_CLEARANCE && position.y <= display.bottom()
    })
}

/// What a resting sprite is standing on: its Perch, unless a window in front
/// of that Perch has come to contain it — `swallowing` being the caller's
/// judgement of which windows those are, since it takes remembering the
/// previous tick to make. See `Engine::swallowed_by`.
///
/// A window dragged over the sprite, or walked into where it overlaps a lower
/// Perch, would otherwise leave it inside a rectangle rather than on anything.
/// Its top edge is the surface instead, which is also why two overlapping
/// windows settle on one Perch: the topmost edge wins, and it wins again next
/// tick.
///
/// A sprite on the floor is exempt. The floor is under every window and the
/// sprite is drawn in front of them all, so standing on the ground in front of
/// a window is not the trapped-inside case DESIGN.md decision 7 is about — and
/// every window hanging below the usable floor, as anything behind the Dock
/// does, contains the ground the sprite stands on.
pub(super) fn footing(
    position: Point,
    snapshot: &WorldSnapshot,
    swallowing: impl Fn(&Window) -> bool,
) -> Option<Support> {
    if floor_under(position.x, snapshot) == Some(position.y) {
        return support_below(position, snapshot);
    }

    // The Perch it is on: a window whose top edge is exactly its own height.
    // Without one the sprite is standing on nothing and falls. A Perch that
    // moves out from under it leaves it in the air, and not on the first window
    // that happens to contain it. #78.
    let perch = snapshot
        .windows
        .iter()
        .position(|window| window.rect.spans_x(position.x) && window.rect.y == position.y)?;

    // Only a window in front of the Perch can swallow the sprite, which is why
    // the candidates stop at the Perch's own place in the order: what is
    // behind the Perch is behind the sprite too, so the edge it stands on is
    // still there to be seen.
    //
    // An edge you cannot see is gone, including one the sprite is already
    // standing on. Keeping a hidden Perch left it floating in mid-air
    // after alt-tab. A window that already contained the sprite still
    // swallows nothing, so this falls rather than hoisting it under the
    // menu bar. #100, and #78 still holds.
    let held = (is_perch(perch, position.x, snapshot)
        && !snapshot.windows[..perch].iter().any(&swallowing))
    .then_some(Support {
        y: position.y,
        surface: Surface::Perch,
    });

    snapshot.windows[..perch]
        .iter()
        .enumerate()
        // Swallowed only onto a top edge that is somewhere to stand: lifting
        // the sprite onto an edge that is hidden or off-screen strands it in
        // the place this is meant to get it out of.
        .filter(|(index, window)| swallowing(window) && is_perch(*index, position.x, snapshot))
        .map(|(_, window)| Support {
            y: window.rect.y,
            surface: Surface::Perch,
        })
        .chain(held)
        .chain(support_below(position, snapshot))
        .min_by(|a, b| a.y.total_cmp(&b.y))
}

/// The window whose top edge the sprite is standing on, if any.
pub(super) fn perch_at(position: Point, windows: &[Window]) -> Option<Window> {
    windows
        .iter()
        .copied()
        .find(|window| window.rect.spans_x(position.x) && window.rect.y == position.y)
}

/// Whether the sprite is inside `window` rather than on top of it. A top edge
/// is a Perch to stand on, so only what is strictly below it is inside.
pub(super) fn swallows(window: &Rect, position: Point) -> bool {
    window.spans_x(position.x) && window.y < position.y && position.y < window.bottom()
}

/// The bottom of the display the sprite is over, or nothing when it is over no
/// display at all — a sprite outside every display has nothing to land on.
fn floor_under(x: f64, snapshot: &WorldSnapshot) -> Option<f64> {
    displays_spanning(x, snapshot)
        .map(Rect::bottom)
        .max_by(f64::total_cmp)
}

/// The highest the feet may go: the usable top plus the room the art needs
/// above them. A climb lets go here; a Throw bumps it. #100.
pub(super) fn ceiling_over(x: f64, snapshot: &WorldSnapshot) -> Option<f64> {
    displays_spanning(x, snapshot)
        .map(|display| display.y + CEILING_CLEARANCE)
        .min_by(f64::total_cmp)
}

fn displays_spanning<'a>(
    x: f64,
    snapshot: &'a WorldSnapshot,
) -> impl Iterator<Item = &'a Rect> + 'a {
    snapshot
        .displays
        .iter()
        .filter(move |display| display.spans_x(x))
}

/// The screen edge the sprite has just arrived at while moving into it, or the
/// nearest one when it is over no display at all. A sprite out there has no
/// floor under it and would otherwise fall for ever, so grabbing an edge is how
/// it gets back over a display.
///
/// "Over no display" is the union of the display frames, not the rectangle that
/// bounds them. Two displays side by side with a gap between them bound a
/// region the sprite can occupy and no screen covers, and letting go of it
/// there leaves it with no sideways speed to carry it out — it simply falls,
/// unseen, for as long as the app runs. A thrown sprite crosses such a gap
/// before gravity matters, which is why only a drop finds this.
pub(super) fn wall_reached(x: f64, velocity_x: f64, snapshot: &WorldSnapshot) -> Option<f64> {
    if displays_spanning(x, snapshot).next().is_none() {
        return nearest_edge(x, snapshot);
    }

    // Still over a display, so the only wall left is the outermost edge, and
    // only while the sprite is moving into it. Every inner edge is somewhere to
    // cross rather than something to catch.
    let left = snapshot
        .displays
        .iter()
        .map(|display| display.x)
        .min_by(f64::total_cmp)?;
    let right = snapshot
        .displays
        .iter()
        .map(|display| display.x + display.width)
        .max_by(f64::total_cmp)?;

    if velocity_x > 0.0 && x >= right {
        Some(right)
    } else if velocity_x < 0.0 && x <= left {
        Some(left)
    } else {
        None
    }
}

/// The Dock, when the snapshot carries one: the Perch wearing the reserved id.
pub(super) fn dock_in(snapshot: &WorldSnapshot) -> Option<Rect> {
    snapshot
        .windows
        .iter()
        .find(|window| window.id == DOCK_PERCH_ID)
        .map(|window| window.rect)
}

/// Where to stand when the Dock is in the way: clear of its nearer side, on
/// the floor or in the air.
///
/// The Dock is the one thing on screen drawn in front of the sprite, so under
/// it the sprite can be neither seen nor grabbed. Its side is a wall to climb
/// (#176). Nearer rather than the side it came from, because the Dock can
/// appear around a resting sprite when it unhides, and a dropped sprite
/// arrives from above: on a walk the two are the same side, one step away.
///
/// The wall stands `clearance` out from the Dock's own edge — half a
/// sprite, the same half `at_horizontal_edge` keeps on screen — because a
/// sprite centered on the edge is already half hidden, walking and climbing.
/// Strictly inside that, so the sprite this puts on the line is beside the
/// Dock and stays put: the walk stops going forward instead of being set
/// back, which is the #141 stutter it must not repeat.
///
/// Out in that margin the side is a wall only for a sprite moving into the
/// Dock, the rule `wall_reached` already keeps for a display edge: one moving
/// away has left, and catching it anyway is a wall in the middle of the floor.
/// It is what made the Dock a trap for a walk heading past it — step off the
/// top, get caught by the side just below, climb, and get put back on the top
/// to do it again, for as long as the app ran. #361.
///
/// Strictly behind the Dock there is no such reprieve, whichever way the
/// sprite is going: a real Dock is wide and the way out from under one is a
/// long way sideways, all of it unseen. It climbs.
pub(super) fn dock_side_reached(
    position: Point,
    velocity_x: f64,
    clearance: f64,
    snapshot: &WorldSnapshot,
) -> Option<f64> {
    let dock = dock_in(snapshot)?;
    let (left, right) = (dock.x - clearance, dock.x + dock.width + clearance);
    if position.y <= dock.y || position.x <= left || position.x >= right {
        return None;
    }

    // Behind the Dock's own display, not merely in its x-range: a display
    // stacked below this one shares that range, and every sprite on its floor
    // would otherwise be behind the Dock forever — climb, top out, fall,
    // climb. The Dock's center says which display owns it, the way
    // `window_source::centered_in` does (a Dock touches its display's edges).
    // The display's bottom, not the Dock's: the real Dock stops short of the
    // floor the sprite walks on.
    let center = (dock.x + dock.width / 2.0, dock.y + dock.height / 2.0);
    snapshot.displays.iter().find(|display| {
        display.spans_x(center.0)
            && center.1 >= display.y
            && center.1 <= display.bottom()
            && display.spans_x(position.x)
            && position.y <= display.bottom()
    })?;

    let nearer_left = position.x - left <= right - position.x;

    // Strictly inside, so a sprite level with the Dock's own edge counts as
    // out in the margin and may leave. It is the position stepping off the top
    // lands on, and treating it as behind would keep the trap shut.
    let behind = position.x > dock.x && position.x < dock.x + dock.width;
    let into_dock = if nearer_left {
        velocity_x > 0.0
    } else {
        velocity_x < 0.0
    };
    if !behind && !into_dock {
        return None;
    }

    Some(if nearer_left { left } else { right })
}

/// Where a climb beside the Dock steps onto its top, once the feet reach it.
///
/// Half a sprite in from the side it climbed, so the whole sprite stands on
/// the Dock rather than overhanging the corner — and so it stands somewhere
/// `perch_at` agrees is the Dock, which is what keeps it there. Clamped to the
/// far side for a Dock narrower than a sprite. `None` when the climb is
/// nowhere near the Dock, which is every ordinary climb up a screen edge.
pub(super) fn dock_top_at(x: f64, clearance: f64, dock: Rect) -> Option<f64> {
    let (left, right) = (dock.x, dock.x + dock.width);
    if (left - clearance..=left + clearance).contains(&x) {
        Some((left + clearance).min(right))
    } else if (right - clearance..=right + clearance).contains(&x) {
        Some((right - clearance).max(left))
    } else if dock.spans_x(x) {
        Some(x)
    } else {
        None
    }
}

/// Adjusted position and facing when at a horizontal display edge, so the full
/// sprite stays on-screen and faces away from the wall.
pub(super) fn at_horizontal_edge(
    x: f64,
    clearance: f64,
    snapshot: &WorldSnapshot,
) -> Option<(f64, f64)> {
    let left = snapshot
        .displays
        .iter()
        .map(|display| display.x)
        .min_by(f64::total_cmp)?;
    let right = snapshot
        .displays
        .iter()
        .map(|display| display.x + display.width)
        .max_by(f64::total_cmp)?;

    // Only correct when very close to the boundary. Sprites that settled
    // naturally within a half-sprite of the edge but not AT it should stay put.
    let snap_threshold = clearance / 2.0;

    if x <= left + snap_threshold {
        Some((left + clearance, 1.0))
    } else if x >= right - snap_threshold {
        Some((right - clearance, -1.0))
    } else {
        None
    }
}

/// The display edge nearest `x`, for a sprite that is over none of them.
///
/// Nearest rather than the one it came from: the sprite has no memory of that,
/// and the shortest way back to somewhere it can stand is the least surprising
/// place for it to reappear.
fn nearest_edge(x: f64, snapshot: &WorldSnapshot) -> Option<f64> {
    snapshot
        .displays
        .iter()
        .flat_map(|display| [display.x, display.x + display.width])
        .min_by(|a, b| (a - x).abs().total_cmp(&(b - x).abs()))
}
