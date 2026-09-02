//! Native settings. Checkboxes and fields, not a webview.
//!
//! SPEC gives the webview to the sprite and the chat surface. Settings is
//! Shell furniture, the same as the tray menu, so it is AppKit.

use std::cell::RefCell;

use objc2::rc::Retained;
use objc2::runtime::{AnyObject, ProtocolObject};
use objc2::{define_class, msg_send, sel, DefinedClass, MainThreadOnly};
use objc2_app_kit::{
    NSAlert, NSAlertFirstButtonReturn, NSApplication, NSBackingStoreType, NSButton, NSColor,
    NSControlStateValueOff, NSControlStateValueOn, NSFont, NSPopUpButton, NSScrollView,
    NSTextDelegate, NSTextField, NSTextView, NSTextViewDelegate, NSView, NSWindow,
    NSWindowDelegate, NSWindowStyleMask,
};
use objc2_foundation::{
    MainThreadMarker, NSNotification, NSObject, NSObjectProtocol, NSPoint, NSRect, NSSize, NSString,
};

use crate::settings::{SettingsPatch, SettingsSession, SettingsView};

const WINDOW_WIDTH: f64 = 560.0;
const WINDOW_HEIGHT: f64 = 720.0;
const DOC_HEIGHT: f64 = 1156.0;
const MARGIN: f64 = 28.0;
const FIELD_WIDTH: f64 = WINDOW_WIDTH - MARGIN * 2.0;

const TAG_DIRECTOR: isize = 1;
const TAG_AMBIENT: isize = 2;
const TAG_DND: isize = 3;
const TAG_HIDDEN: isize = 4;
const TAG_FULLSCREEN: isize = 5;

thread_local! {
    static CONTROLLER: RefCell<Option<Retained<SettingsController>>> = const { RefCell::new(None) };
}

#[derive(Default)]
struct Ivars {
    session: RefCell<Option<SettingsSession>>,
    window: RefCell<Option<Retained<NSWindow>>>,
    director: RefCell<Option<Retained<NSButton>>>,
    ambient: RefCell<Option<Retained<NSButton>>>,
    dnd: RefCell<Option<Retained<NSButton>>>,
    hidden: RefCell<Option<Retained<NSButton>>>,
    fullscreen: RefCell<Option<Retained<NSButton>>>,
    hotkey: RefCell<Option<Retained<NSTextField>>>,
    excluded: RefCell<Option<Retained<NSTextView>>>,
    payload: RefCell<Option<Retained<NSTextField>>>,
    memory_path: RefCell<Option<Retained<NSTextField>>>,
    character: RefCell<Option<Retained<NSPopUpButton>>>,
    new_character: RefCell<Option<Retained<NSPopUpButton>>>,
    new_name: RefCell<Option<Retained<NSTextField>>>,
    instances: RefCell<Option<Retained<NSView>>>,
}

define_class!(
    // SAFETY: NSObject has no subclassing requirements; this type does not impl Drop.
    #[unsafe(super = NSObject)]
    #[thread_kind = MainThreadOnly]
    #[ivars = Ivars]
    struct SettingsController;

    unsafe impl NSObjectProtocol for SettingsController {}

    unsafe impl NSWindowDelegate for SettingsController {
        #[unsafe(method(windowDidBecomeKey:))]
        fn became_key(&self, _notification: &NSNotification) {
            self.refresh();
        }
    }

    unsafe impl NSTextDelegate for SettingsController {
        #[unsafe(method(textDidEndEditing:))]
        fn text_did_end_editing(&self, _notification: &NSNotification) {
            self.commit_excluded();
        }
    }

    unsafe impl NSTextViewDelegate for SettingsController {}

    impl SettingsController {
        #[unsafe(method(toggle:))]
        fn toggle(&self, sender: Option<&AnyObject>) {
            let Some(button) = sender.and_then(|s| s.downcast_ref::<NSButton>()) else {
                return;
            };
            let on = button.state() == NSControlStateValueOn;
            let mut patch = SettingsPatch::default();
            match button.tag() {
                TAG_DIRECTOR => patch.director_enabled = Some(on),
                TAG_AMBIENT => patch.ambient_wakes = Some(on),
                TAG_DND => patch.do_not_disturb = Some(on),
                TAG_HIDDEN => patch.hidden = Some(on),
                TAG_FULLSCREEN => patch.hide_in_fullscreen = Some(on),
                _ => return,
            }
            self.apply(patch);
        }

        #[unsafe(method(excludedEnded:))]
        fn excluded_ended(&self, _sender: Option<&AnyObject>) {
            self.commit_excluded();
        }

        #[unsafe(method(characterPicked:))]
        fn character_picked(&self, sender: Option<&AnyObject>) {
            let Some(popup) = sender.and_then(|s| s.downcast_ref::<NSPopUpButton>()) else {
                return;
            };
            let Some(title) = popup.titleOfSelectedItem() else {
                return;
            };
            self.apply(SettingsPatch {
                character: Some(title.to_string()),
                ..SettingsPatch::default()
            });
        }

        #[unsafe(method(memoryOpen:))]
        fn memory_open(&self, _sender: Option<&AnyObject>) {
            if let Some(session) = self.ivars().session.borrow().as_ref() {
                if let Err(why) = session.open_memory() {
                    eprintln!("settings: {why}");
                }
            }
        }

        #[unsafe(method(memoryWipe:))]
        fn memory_wipe(&self, _sender: Option<&AnyObject>) {
            let mtm = self.mtm();
            let alert = NSAlert::new(mtm);
            alert.setMessageText(&NSString::from_str("Wipe Memory?"));
            alert.setInformativeText(&NSString::from_str(
                "A backup is kept beside the file.",
            ));
            alert.addButtonWithTitle(&NSString::from_str("Wipe"));
            alert.addButtonWithTitle(&NSString::from_str("Cancel"));
            if alert.runModal() != NSAlertFirstButtonReturn {
                return;
            }
            if let Some(session) = self.ivars().session.borrow().as_ref() {
                if let Err(why) = session.wipe_memory() {
                    eprintln!("settings: {why}");
                }
            }
        }

        #[unsafe(method(spawn:))]
        fn spawn(&self, _sender: Option<&AnyObject>) {
            let ivars = self.ivars();
            let name = ivars
                .new_name
                .borrow()
                .as_ref()
                .map(|field| field.stringValue().to_string())
                .unwrap_or_default()
                .trim()
                .to_string();
            let character = ivars
                .new_character
                .borrow()
                .as_ref()
                .and_then(|popup| popup.titleOfSelectedItem())
                .map(|title| title.to_string())
                .unwrap_or_default();
            if name.is_empty() || character.is_empty() {
                return;
            }
            if let Some(session) = ivars.session.borrow().as_ref() {
                session.spawn(character, name);
            }
            if let Some(field) = ivars.new_name.borrow().as_ref() {
                field.setStringValue(&NSString::from_str(""));
            }
        }

        #[unsafe(method(dismiss:))]
        fn dismiss(&self, sender: Option<&AnyObject>) {
            let Some(button) = sender.and_then(|s| s.downcast_ref::<NSButton>()) else {
                return;
            };
            let id = button.tag();
            let session = self.ivars().session.borrow();
            let Some(session) = session.as_ref() else {
                return;
            };
            let view = session.view();
            if let Some(row) = view.instances.get(id as usize) {
                session.dismiss(row.id.clone());
            }
        }
    }
);

impl SettingsController {
    fn new(mtm: MainThreadMarker) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(Ivars::default());
        // SAFETY: NSObject's init takes no arguments.
        unsafe { msg_send![super(this), init] }
    }

    fn commit_excluded(&self) {
        let Some(view) = self.ivars().excluded.borrow().clone() else {
            return;
        };
        let apps = view
            .string()
            .to_string()
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(str::to_string)
            .collect();
        self.apply(SettingsPatch {
            excluded_applications: Some(apps),
            ..SettingsPatch::default()
        });
    }

    fn apply(&self, patch: SettingsPatch) {
        if let Some(session) = self.ivars().session.borrow().as_ref() {
            if let Err(why) = session.apply(patch) {
                eprintln!("settings: {why}");
            }
        }
    }

    fn refresh(&self) {
        let Some(view) = self.ivars().session.borrow().as_ref().map(|s| s.view()) else {
            return;
        };
        fill_checkbox(&self.ivars().director, view.director_enabled);
        fill_checkbox(&self.ivars().ambient, view.ambient_wakes);
        fill_checkbox(&self.ivars().dnd, view.do_not_disturb);
        fill_checkbox(&self.ivars().hidden, view.hidden);
        fill_checkbox(&self.ivars().fullscreen, view.hide_in_fullscreen);
        if let Some(field) = self.ivars().hotkey.borrow().clone() {
            field.setStringValue(&NSString::from_str(&view.hide_hotkey));
        }
        if let Some(field) = self.ivars().memory_path.borrow().clone() {
            field.setStringValue(&NSString::from_str(&view.memory_path));
        }
        if let Some(field) = self.ivars().payload.borrow().clone() {
            field.setStringValue(&NSString::from_str(
                view.last_payload.as_deref().unwrap_or("Nothing sent yet."),
            ));
        }
        if let Some(text) = self.ivars().excluded.borrow().clone() {
            text.setString(&NSString::from_str(&view.excluded_text()));
        }
        fill_popup(&self.ivars().character, &view.installed, &view.character);
        fill_popup(
            &self.ivars().new_character,
            &view.installed,
            &view.character,
        );
        self.fill_instances(&view);
    }

    fn fill_instances(&self, view: &SettingsView) {
        let Some(box_view) = self.ivars().instances.borrow().clone() else {
            return;
        };
        for child in box_view.subviews() {
            child.removeFromSuperview();
        }
        let mtm = self.mtm();
        let width = box_view.frame().size.width;
        let mut y = box_view.frame().size.height - 4.0;
        for (index, line) in view.instance_lines().iter().enumerate() {
            y -= 24.0;
            let label = NSTextField::labelWithString(&NSString::from_str(line), mtm);
            label.setFrame(NSRect::new(
                NSPoint::new(0.0, y),
                NSSize::new(width - 90.0, 22.0),
            ));
            let dismiss = unsafe {
                NSButton::buttonWithTitle_target_action(
                    &NSString::from_str("Dismiss"),
                    Some(self),
                    Some(sel!(dismiss:)),
                    mtm,
                )
            };
            dismiss.setTag(index as isize);
            dismiss.setFrame(NSRect::new(
                NSPoint::new(width - 84.0, y),
                NSSize::new(84.0, 22.0),
            ));
            box_view.addSubview(&label);
            box_view.addSubview(&dismiss);
            y -= 4.0;
        }
    }
}

fn fill_checkbox(slot: &RefCell<Option<Retained<NSButton>>>, on: bool) {
    let slot = slot.borrow();
    if let Some(button) = slot.as_ref() {
        button.setState(if on {
            NSControlStateValueOn
        } else {
            NSControlStateValueOff
        });
    }
}

fn fill_popup(slot: &RefCell<Option<Retained<NSPopUpButton>>>, names: &[String], current: &str) {
    let slot = slot.borrow();
    let Some(popup) = slot.as_ref() else {
        return;
    };
    popup.removeAllItems();
    for name in names {
        popup.addItemWithTitle(&NSString::from_str(name));
    }
    if !current.is_empty() {
        popup.selectItemWithTitle(&NSString::from_str(current));
    }
}

struct Cursor {
    y: f64,
    parent: Retained<NSView>,
    mtm: MainThreadMarker,
}

impl Cursor {
    fn place(&mut self, view: &NSView, height: f64) {
        self.y -= height;
        view.setFrame(NSRect::new(
            NSPoint::new(MARGIN, self.y),
            NSSize::new(FIELD_WIDTH, height),
        ));
        self.parent.addSubview(view);
        self.y -= 8.0;
    }

    fn heading(&mut self, title: &str) {
        let label = NSTextField::labelWithString(&NSString::from_str(title), self.mtm);
        label.setFont(Some(&NSFont::boldSystemFontOfSize(13.0)));
        self.place(&label, 22.0);
    }

    fn hint(&mut self, text: &str) {
        let label = NSTextField::wrappingLabelWithString(&NSString::from_str(text), self.mtm);
        label.setTextColor(Some(&NSColor::secondaryLabelColor()));
        label.setFont(Some(&NSFont::systemFontOfSize(11.0)));
        self.place(&label, 32.0);
    }
}

/// Redraw if the window is already up. The frame loop publishes Instances
/// after a dismiss; without this the list stays the one from last become-key.
pub fn refresh_if_showing() {
    CONTROLLER.with(|slot| {
        if let Some(controller) = slot.borrow().as_ref() {
            controller.refresh();
        }
    });
}

/// Show the settings window, creating it the first time.
pub fn show(session: SettingsSession) {
    let Some(mtm) = MainThreadMarker::new() else {
        eprintln!("settings: the window can only be built on the main thread");
        return;
    };

    CONTROLLER.with(|slot| {
        if slot.borrow().is_none() {
            *slot.borrow_mut() = Some(build(mtm, session));
        } else if let Some(controller) = slot.borrow().as_ref() {
            *controller.ivars().session.borrow_mut() = Some(session);
            controller.refresh();
            if let Some(window) = controller.ivars().window.borrow().as_ref() {
                window.makeKeyAndOrderFront(None);
            }
        }
        let app = NSApplication::sharedApplication(mtm);
        #[allow(deprecated)]
        app.activateIgnoringOtherApps(true);
    });
}

fn build(mtm: MainThreadMarker, session: SettingsSession) -> Retained<SettingsController> {
    let controller = SettingsController::new(mtm);
    *controller.ivars().session.borrow_mut() = Some(session);

    let document = NSView::initWithFrame(
        NSView::alloc(mtm),
        NSRect::new(
            NSPoint::new(0.0, 0.0),
            NSSize::new(WINDOW_WIDTH, DOC_HEIGHT),
        ),
    );

    let mut cursor = Cursor {
        y: DOC_HEIGHT - 16.0,
        parent: document.clone(),
        mtm,
    };

    let title = NSTextField::labelWithString(&NSString::from_str("Settings"), mtm);
    title.setFont(Some(&NSFont::boldSystemFontOfSize(20.0)));
    cursor.place(&title, 28.0);

    cursor.heading("Director");
    let director = checkbox("Director on", TAG_DIRECTOR, &controller, mtm);
    cursor.place(&director, 22.0);
    cursor.hint("Off leaves Static weights running the life. No session calls.");
    let ambient = checkbox("Ambient session wakes", TAG_AMBIENT, &controller, mtm);
    cursor.place(&ambient, 22.0);
    cursor.hint("Off keeps Poke and Summon on the session path. Idle life stays Static.");
    cursor.heading("Last user turn");
    cursor.hint("Inspect only. The last session turn, opening Character Prompt or follow-up.");
    let payload = inspect_block(mtm);
    cursor.place(&payload, 88.0);

    cursor.heading("Character");
    let character = popup(&controller, sel!(characterPicked:), mtm);
    cursor.place(&character, 24.0);

    cursor.heading("Instances");
    let instances = NSView::initWithFrame(
        NSView::alloc(mtm),
        NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(FIELD_WIDTH, 80.0)),
    );
    cursor.place(&instances, 80.0);
    let new_name = NSTextField::textFieldWithString(&NSString::from_str(""), mtm);
    new_name.setPlaceholderString(Some(&NSString::from_str("Name")));
    let new_character = popup_plain(mtm);
    let spawn = unsafe {
        NSButton::buttonWithTitle_target_action(
            &NSString::from_str("New"),
            Some(&*controller),
            Some(sel!(spawn:)),
            mtm,
        )
    };
    cursor.y -= 24.0;
    new_name.setFrame(NSRect::new(
        NSPoint::new(MARGIN, cursor.y),
        NSSize::new(200.0, 24.0),
    ));
    new_character.setFrame(NSRect::new(
        NSPoint::new(MARGIN + 208.0, cursor.y),
        NSSize::new(180.0, 24.0),
    ));
    spawn.setFrame(NSRect::new(
        NSPoint::new(MARGIN + 396.0, cursor.y),
        NSSize::new(64.0, 24.0),
    ));
    document.addSubview(&new_name);
    document.addSubview(&new_character);
    document.addSubview(&spawn);
    cursor.y -= 16.0;

    // DESIGN.md: quiet is not gone. A Hide heading would teach the opposite.
    cursor.heading("Do Not Disturb");
    let dnd = checkbox("Do Not Disturb", TAG_DND, &controller, mtm);
    cursor.place(&dnd, 22.0);
    cursor.hint("On screen, not starting things.");

    cursor.heading("Hide");
    let hidden = checkbox("Go away", TAG_HIDDEN, &controller, mtm);
    cursor.place(&hidden, 22.0);
    let fullscreen = checkbox("Hide in fullscreen apps", TAG_FULLSCREEN, &controller, mtm);
    cursor.place(&fullscreen, 22.0);
    let hotkey_label = NSTextField::labelWithString(&NSString::from_str("Hotkey"), mtm);
    cursor.place(&hotkey_label, 18.0);
    // Shown, not edited. A string field is not a key recorder, and there is
    // no capture UI yet.
    let hotkey = inspect_line(mtm);
    cursor.place(&hotkey, 22.0);

    cursor.heading("Memory");
    let memory_path = NSTextField::wrappingLabelWithString(&NSString::from_str(""), mtm);
    memory_path.setFont(NSFont::userFixedPitchFontOfSize(11.0).as_deref());
    cursor.place(&memory_path, 36.0);
    let open_mem = unsafe {
        NSButton::buttonWithTitle_target_action(
            &NSString::from_str("Open in editor"),
            Some(&*controller),
            Some(sel!(memoryOpen:)),
            mtm,
        )
    };
    let wipe = unsafe {
        NSButton::buttonWithTitle_target_action(
            &NSString::from_str("Wipe"),
            Some(&*controller),
            Some(sel!(memoryWipe:)),
            mtm,
        )
    };
    cursor.y -= 24.0;
    open_mem.setFrame(NSRect::new(
        NSPoint::new(MARGIN, cursor.y),
        NSSize::new(140.0, 24.0),
    ));
    wipe.setFrame(NSRect::new(
        NSPoint::new(MARGIN + 148.0, cursor.y),
        NSSize::new(72.0, 24.0),
    ));
    document.addSubview(&open_mem);
    document.addSubview(&wipe);
    cursor.y -= 16.0;

    cursor.heading("Excluded applications");
    {
        let label = NSTextField::wrappingLabelWithString(
            &NSString::from_str(
                "One application name per line. Those windows stay out of MCP sensing, and the Director is not told they are frontmost. The buddy can still sit on them.",
            ),
            mtm,
        );
        label.setTextColor(Some(&NSColor::secondaryLabelColor()));
        label.setFont(Some(&NSFont::systemFontOfSize(11.0)));
        cursor.place(&label, 48.0);
    }
    let (excluded_scroll, excluded) = text_block(true, mtm);
    excluded.setDelegate(Some(ProtocolObject::from_ref(&*controller)));
    cursor.place(&excluded_scroll, 80.0);

    cursor.heading("Launch");
    // A Launch Agent on `cargo run` is not launch-at-login. There is no
    // bundled app to start, on any OS.
    let launch = checkbox("Launch at login (unimplemented)", 0, &controller, mtm);
    launch.setEnabled(false);
    cursor.place(&launch, 22.0);

    *controller.ivars().director.borrow_mut() = Some(director);
    *controller.ivars().ambient.borrow_mut() = Some(ambient);
    *controller.ivars().dnd.borrow_mut() = Some(dnd);
    *controller.ivars().hidden.borrow_mut() = Some(hidden);
    *controller.ivars().fullscreen.borrow_mut() = Some(fullscreen);
    *controller.ivars().hotkey.borrow_mut() = Some(hotkey);
    *controller.ivars().excluded.borrow_mut() = Some(excluded);
    *controller.ivars().payload.borrow_mut() = Some(payload);
    *controller.ivars().memory_path.borrow_mut() = Some(memory_path);
    *controller.ivars().character.borrow_mut() = Some(character);
    *controller.ivars().new_character.borrow_mut() = Some(new_character);
    *controller.ivars().new_name.borrow_mut() = Some(new_name);
    *controller.ivars().instances.borrow_mut() = Some(instances);

    let scroll = NSScrollView::initWithFrame(
        NSScrollView::alloc(mtm),
        NSRect::new(
            NSPoint::new(0.0, 0.0),
            NSSize::new(WINDOW_WIDTH, WINDOW_HEIGHT),
        ),
    );
    scroll.setHasVerticalScroller(true);
    scroll.setDocumentView(Some(&document));

    let window = unsafe {
        NSWindow::initWithContentRect_styleMask_backing_defer(
            NSWindow::alloc(mtm),
            NSRect::new(
                NSPoint::new(0.0, 0.0),
                NSSize::new(WINDOW_WIDTH, WINDOW_HEIGHT),
            ),
            NSWindowStyleMask::Titled
                | NSWindowStyleMask::Closable
                | NSWindowStyleMask::Miniaturizable
                | NSWindowStyleMask::Resizable,
            NSBackingStoreType::Buffered,
            false,
        )
    };
    unsafe { window.setReleasedWhenClosed(false) };
    window.setTitle(&NSString::from_str("ai-buddy"));
    if let Some(content) = window.contentView() {
        scroll.setFrame(content.bounds());
        content.addSubview(&scroll);
    }
    window.setDelegate(Some(ProtocolObject::from_ref(&*controller)));
    window.center();
    window.makeKeyAndOrderFront(None);

    *controller.ivars().window.borrow_mut() = Some(window);
    controller.refresh();
    controller
}

fn checkbox(
    title: &str,
    tag: isize,
    controller: &SettingsController,
    mtm: MainThreadMarker,
) -> Retained<NSButton> {
    let button = unsafe {
        NSButton::checkboxWithTitle_target_action(
            &NSString::from_str(title),
            Some(controller),
            Some(sel!(toggle:)),
            mtm,
        )
    };
    button.setTag(tag);
    button
}

fn popup(
    controller: &SettingsController,
    action: objc2::runtime::Sel,
    mtm: MainThreadMarker,
) -> Retained<NSPopUpButton> {
    let popup = popup_plain(mtm);
    unsafe {
        popup.setTarget(Some(controller));
        popup.setAction(Some(action));
    }
    popup
}

fn popup_plain(mtm: MainThreadMarker) -> Retained<NSPopUpButton> {
    NSPopUpButton::initWithFrame_pullsDown(
        NSPopUpButton::alloc(mtm),
        NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(FIELD_WIDTH, 24.0)),
        false,
    )
}

/// A value the window shows, not a box you type into.
fn inspect_line(mtm: MainThreadMarker) -> Retained<NSTextField> {
    let field = NSTextField::labelWithString(&NSString::from_str(""), mtm);
    field.setSelectable(true);
    field
}

fn inspect_block(mtm: MainThreadMarker) -> Retained<NSTextField> {
    let field = NSTextField::wrappingLabelWithString(&NSString::from_str(""), mtm);
    field.setSelectable(true);
    field
}

fn text_block(
    editable: bool,
    mtm: MainThreadMarker,
) -> (Retained<NSScrollView>, Retained<NSTextView>) {
    let scroll = NSTextView::scrollableTextView(mtm);
    let view = scroll
        .documentView()
        .and_then(|doc| doc.downcast::<NSTextView>().ok())
        .expect("scrollableTextView owns an NSTextView");
    view.setEditable(editable);
    view.setRichText(false);
    (scroll, view)
}
