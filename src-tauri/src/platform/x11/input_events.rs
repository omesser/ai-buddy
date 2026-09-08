//! XI2 raw input events for event-driven pointer input on X11.
//!
//! Listens for mouse button and motion events via XInput2 raw events, which are
//! permission-free and work on the root window. This replaces polling when the
//! sprite is idle, letting the frame loop block on `recv()` instead of waking
//! at 16ms. #183.

use std::sync::mpsc;
use std::thread;
use x11rb::connection::Connection;
use x11rb::protocol::xinput::{self, ConnectionExt as _};
use x11rb::protocol::xproto;

/// An input event from XI2 that the frame loop cares about.
#[derive(Clone, Copy, Debug)]
pub enum InputEvent {
    /// Mouse button pressed or released.
    Button,
    /// Mouse moved. The frame loop doesn't need the position (it polls the
    /// cursor separately), only that motion happened.
    Motion,
}

/// Spawn a thread that listens for XI2 raw input events and sends them to the
/// returned channel. Returns `None` if XI2 cannot be set up.
///
/// The thread blocks on the X11 connection's event stream and runs for the life
/// of the process. It filters for button and motion events and sends `InputEvent`
/// to wake the frame loop from idle wait. The frame loop still polls cursor
/// position and button state after waking via the existing Witness path.
pub fn spawn_listener() -> Option<mpsc::Receiver<InputEvent>> {
    let display = super::connection::connection()?;
    let (sender, receiver) = mpsc::channel();

    let screen = display.setup().roots.first()?;
    let root = screen.root;

    if setup_xi2(display, root).is_err() {
        return None;
    }

    let sender_clone = sender.clone();
    thread::Builder::new()
        .name("xi2-input".to_string())
        .spawn(move || listen_loop(sender_clone))
        .ok()?;

    Some(receiver)
}

/// Set up XI2 extension and register for raw input events on the root window.
fn setup_xi2(
    display: &impl Connection,
    root: xproto::Window,
) -> Result<(), Box<dyn std::error::Error>> {
    let version = display.xinput_xi_query_version(2, 0)?.reply()?;

    if version.major_version < 2 {
        return Err("XI2 not available".into());
    }

    // XI2 raw event mask: select RawButtonPress, RawButtonRelease, and RawMotion.
    let mask = vec![
        xinput::XIEventMask::RAW_BUTTON_PRESS,
        xinput::XIEventMask::RAW_BUTTON_RELEASE,
        xinput::XIEventMask::RAW_MOTION,
    ];

    display.xinput_xi_select_events(
        root,
        &[xinput::EventMask {
            deviceid: xinput::Device::ALL.into(),
            mask,
        }],
    )?;

    display.flush()?;
    Ok(())
}

/// Block on the X11 connection and send input events until the channel closes.
fn listen_loop(sender: mpsc::Sender<InputEvent>) {
    let Some(display) = super::connection::connection() else {
        return;
    };

    while let Ok(event) = display.wait_for_event() {
        let Some(input_event) = classify_event(display, &event) else {
            continue;
        };

        if sender.send(input_event).is_err() {
            break;
        }
    }
}

/// Classify an X11 event into an `InputEvent` if it is one the frame loop cares about.
fn classify_event(
    _display: &impl Connection,
    event: &x11rb::protocol::Event,
) -> Option<InputEvent> {
    use x11rb::protocol::Event;

    match event {
        Event::XinputRawButtonPress(_) | Event::XinputRawButtonRelease(_) => {
            Some(InputEvent::Button)
        }
        Event::XinputRawMotion(_) => Some(InputEvent::Motion),
        _ => None,
    }
}
