//! XI2 raw input events for event-driven pointer input on X11.
//!
//! Listens for mouse button and motion events via XInput2 raw events, which are
//! permission-free and work on the root window. This replaces polling when the
//! sprite is idle, letting the frame loop block on `recv()` instead of waking
//! at 16ms. #183.

use std::sync::mpsc;
use std::thread;
use x11rb::connection::Connection;
use x11rb::protocol::xinput::{self, ConnectionExt as _, EventMask};
use x11rb::protocol::xproto;

use crate::platform::ButtonsDown;

/// An input event from XI2 that the frame loop cares about.
#[derive(Clone, Copy, Debug)]
pub enum InputEvent {
    /// Mouse button pressed or released.
    Button(ButtonsDown),
    /// Mouse moved. The frame loop doesn't need the position (it polls the
    /// cursor separately), only that motion happened.
    Motion,
}

/// Spawn a thread that listens for XI2 raw input events and sends them to the
/// returned channel. Returns `None` if XI2 cannot be set up.
///
/// The thread blocks on the X11 connection's event stream and runs for the life
/// of the process. It filters for button and motion events, derives `ButtonsDown`
/// from the current button mask, and sends `InputEvent` to the channel.
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
    let version = display
        .xinput_xi_query_version(2, 0)?
        .reply()?;

    if version.major_version < 2 {
        return Err("XI2 not available".into());
    }

    let mask = EventMask::RAW_BUTTON_PRESS
        | EventMask::RAW_BUTTON_RELEASE
        | EventMask::RAW_MOTION;

    let mask_bytes = u32::from(mask).to_ne_bytes();

    display.xinput_xi_select_events(
        root,
        &[xinput::EventMask {
            deviceid: xinput::Device::ALL.into(),
            mask: mask_bytes.to_vec(),
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

    loop {
        let Ok(event) = display.wait_for_event() else {
            break;
        };

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
    display: &impl Connection,
    event: &x11rb::protocol::Event,
) -> Option<InputEvent> {
    use x11rb::protocol::Event;

    match event {
        Event::XinputRawButtonPress(_) | Event::XinputRawButtonRelease(_) => {
            Some(InputEvent::Button(current_buttons(display)))
        }
        Event::XinputRawMotion(_) => Some(InputEvent::Motion),
        _ => None,
    }
}

/// Query the current button mask from XQueryPointer.
///
/// Same logic as `pointer::buttons_down`, but local to avoid a circular
/// dependency. XI2 raw events tell us *something changed*, not what the new
/// state is, so the frame loop still needs to query.
fn current_buttons(display: &impl Connection) -> ButtonsDown {
    let screen = &display.setup().roots[0];
    let Ok(reply) = xproto::query_pointer(display, screen.root)
        .ok()
        .and_then(|cookie| cookie.reply().ok())
    else {
        return ButtonsDown::default();
    };

    let mask: u16 = reply.mask.into();
    ButtonsDown {
        primary: (mask & u16::from(xproto::ButtonMask::M1)) != 0,
        secondary: (mask & u16::from(xproto::ButtonMask::M3)) != 0,
    }
}
