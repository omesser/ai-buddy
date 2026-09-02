//! The State machine, in one place.
//!
//! The only writer of State in the Engine: verbs move it before physics and
//! contacts move it after, and `tick` stores nothing but what these two
//! functions return. Geometry reports Surfaces and `integrate` reports
//! Contacts precisely so that neither has an opinion on what the sprite
//! becomes — a transition added anywhere else is the scattering this module
//! exists to end.

use super::{State, Surface, Verb};

/// What the sprite's body met while `Engine::integrate` moved it.
#[derive(Clone, Copy)]
pub enum Contact {
    /// Nothing underfoot: still in the air, or the footing is gone.
    Airborne,
    /// Came down on a surface at the end of a fall.
    Landed(Surface),
    /// Put on top of a window that had come to contain it. Not a landing:
    /// the sprite never left the ground it thought it had.
    Lifted(Surface),
    /// Still standing exactly where it stood.
    Standing,
    /// Reached a screen edge sideways, mid-air.
    Wall,
    /// Met the usable top while rising or climbing. #100.
    Ceiling,
}

/// Where the user's hand leaves the sprite before any physics runs, and
/// whether a verb woke it. The wake is reported rather than left for the
/// caller to re-derive, so what waking means is written here once.
pub fn on_verbs(state: State, verbs: &[Verb]) -> (State, bool) {
    let woke = state == State::Asleep && !verbs.is_empty();

    if verbs.contains(&Verb::Grab) {
        return (State::Dragged, woke);
    }
    let state = match state {
        // Let go. Thrown or simply dropped, what follows is a fall.
        State::Dragged => State::Falling,
        // Woken. Whether it is still standing on anything is settled by
        // falling, the same as any other loss of footing.
        _ if woke => State::Falling,
        _ => state,
    };
    (state, woke)
}

/// Where this tick's physics leaves the sprite. `rested` is whether it
/// has been untouched and still for long enough to nod off.
pub fn on_contact(state: State, contact: Option<Contact>, rested: bool) -> State {
    match contact {
        // Physics had nothing to report: a held sprite, or a climb still
        // under way.
        None => state,
        Some(Contact::Wall) => State::Climbing,
        Some(Contact::Airborne | Contact::Ceiling) => State::Falling,
        Some(Contact::Landed(surface) | Contact::Lifted(surface)) => match surface {
            Surface::Floor => State::Grounded,
            Surface::Perch => State::Perched,
        },
        Some(Contact::Standing) if rested => State::Asleep,
        Some(Contact::Standing) => state,
    }
}
