//! Native settings. Checkboxes and fields, not a webview.
//!
//! SPEC gives the webview to the sprite and the chat surface. Settings is
//! Shell furniture, the same as the tray menu, so it is AppKit.

use std::cell::RefCell;
use std::collections::HashMap;

use objc2::rc::Retained;
use objc2::runtime::{AnyObject, ProtocolObject};
use objc2::{define_class, msg_send, msg_send_id, sel, DefinedClass, MainThreadOnly};
use objc2_app_kit::{
    NSAlert, NSAlertFirstButtonReturn, NSBackingStoreType, NSButton, NSColor,
    NSControlStateValueOff, NSControlStateValueOn, NSFont, NSPopUpButton, NSScrollView,
    NSSecureTextField, NSTextDelegate, NSTextField, NSTextView, NSTextViewDelegate, NSView,
    NSWindow, NSWindowDelegate, NSWindowStyleMask,
};
use objc2_foundation::{
    MainThreadMarker, NSNotification, NSObject, NSObjectProtocol, NSPoint, NSRect, NSSize, NSString,
};

use crate::settings::form::{self, CompositeControl, FormRow};
use crate::settings::{SettingsPatch, SettingsSession, SettingsView};

const WINDOW_WIDTH: f64 = 560.0;
const WINDOW_HEIGHT: f64 = 720.0;
const DOC_HEIGHT: f64 = 1332.0;
const MARGIN: f64 = 28.0;
const FIELD_WIDTH: f64 = WINDOW_WIDTH - MARGIN * 2.0;

thread_local! {
    static CONTROLLER: RefCell<Option<Retained<SettingsController>>> = const { RefCell::new(None) };
}

#[derive(Default)]
struct Ivars {
    session: RefCell<Option<SettingsSession>>,
    window: RefCell<Option<Retained<NSWindow>>>,
    director: RefCell<Option<Retained<NSButton>>>,
    base_url: RefCell<Option<Retained<NSTextField>>>,
    model: RefCell<Option<Retained<NSTextField>>>,
    api_key: RefCell<Option<Retained<NSTextField>>>,
    clear_key: RefCell<Option<Retained<NSButton>>>,
    ambient: RefCell<Option<Retained<NSButton>>>,
    dnd: RefCell<Option<Retained<NSButton>>>,
    hidden: RefCell<Option<Retained<NSButton>>>,
    fullscreen: RefCell<Option<Retained<NSButton>>>,
    tag_to_id: RefCell<HashMap<isize, String>>,
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

            let tag = button.tag();
            let tag_to_id = self.ivars().tag_to_id.borrow();
            let Some(id) = tag_to_id.get(&tag) else {
                return;
            };

            let description = form::describe();
            let Some(action) = description.actions.get(id) else {
                return;
            };

            let mut patch = SettingsPatch::default();
            if let form::RowAction::PatchField(field_name) = action {
                match field_name.as_str() {
                    "director_enabled" => patch.director_enabled = Some(on),
                    "ambient_wakes" => patch.ambient_wakes = Some(on),
                    "do_not_disturb" => patch.do_not_disturb = Some(on),
                    "hidden" => patch.hidden = Some(on),
                    "hide_in_fullscreen" => patch.hide_in_fullscreen = Some(on),
                    "launch_at_login" => patch.launch_at_login = Some(on),
                    _ => return,
                }
            } else {
                return;
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

        #[unsafe(method(handleAction:))]
        fn handle_action(&self, sender: Option<&AnyObject>) {
            let Some(button) = sender.and_then(|s| s.downcast_ref::<NSButton>()) else {
                return;
            };

            let tag = button.tag();
            let tag_to_id = self.ivars().tag_to_id.borrow();
            let Some(id) = tag_to_id.get(&tag) else {
                return;
            };

            let description = form::describe();
            let Some(action) = description.actions.get(id) else {
                return;
            };

            if let form::RowAction::Operation(op) = action {
                match op {
                    form::RowOperation::Spawn => self.do_spawn(),
                    form::RowOperation::OpenMemory => self.do_memory_open(),
                    form::RowOperation::WipeMemory => self.do_memory_wipe(),
                    form::RowOperation::ClearKey => self.do_clear_key(),
                }
            }
        }

        #[unsafe(method(dismiss:))]
        fn dismiss(&self, sender: Option<&AnyObject>) {
            let Some(button) = sender.and_then(|s| s.downcast_ref::<NSButton>()) else {
                return;
            };
            let index = button.tag() as usize;
            if let Some(session) = self.ivars().session.borrow().as_ref() {
                session.dismiss(index);
            }
        }

        fn do_spawn(&self) {
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

        fn do_memory_open(&self) {
            if let Some(session) = self.ivars().session.borrow().as_ref() {
                if let Err(why) = session.open_memory() {
                    eprintln!("settings: {why}");
                }
            }
        }

        fn do_memory_wipe(&self) {
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

        fn do_clear_key(&self) {
            self.apply(SettingsPatch {
                director_api_key: Some("".into()),
                ..SettingsPatch::default()
            });
            self.refresh();
        }

        fn commit_excluded(&self) {
            let text = self
                .ivars()
                .excluded
                .borrow()
                .as_ref()
                .map(|field| field.string().to_string())
                .unwrap_or_default();
            let lines: Vec<String> = text.lines().map(|line| line.trim().to_string()).collect();
            self.apply(SettingsPatch {
                excluded_applications: Some(lines),
                ..SettingsPatch::default()
            });
        }

        fn apply(&self, patch: SettingsPatch) {
            if let Some(session) = self.ivars().session.borrow().as_ref() {
                if let Err(why) = session.apply(patch) {
                    eprintln!("settings: {why}");
                    let alert = NSAlert::new(self.mtm());
                    alert.setMessageText(&NSString::from_str("Could not save settings"));
                    alert.setInformativeText(&NSString::from_str(&why));
                    alert.addButtonWithTitle(&NSString::from_str("OK"));
                    alert.runModal();
                }
            }
        }
    }
);

impl SettingsController {
    fn new(mtm: MainThreadMarker) -> Retained<Self> {
        let controller: Retained<SettingsController> =
            unsafe { msg_send_id![mtm.alloc::<SettingsController>(), init] };
        controller
    }

    fn mtm(&self) -> MainThreadMarker {
        MainThreadMarker::from(self)
    }

    fn refresh(&self) {
        let Some(view) = self.ivars().session.borrow().as_ref().map(|s| s.view()) else {
            return;
        };
        let description = form::describe();
        let dismiss_label = description
            .sections
            .iter()
            .find(|s| s.heading == "Instances")
            .and_then(|s| {
                s.rows.iter().find_map(|r| match r {
                    form::FormRow::List { dismiss_label, .. } => Some(dismiss_label.as_str()),
                    _ => None,
                })
            })
            .unwrap_or("Dismiss");

        fill_checkbox(&self.ivars().director, view.director_enabled);
        fill_checkbox(&self.ivars().ambient, view.ambient_wakes);
        fill_checkbox(&self.ivars().dnd, view.do_not_disturb);
        fill_checkbox(&self.ivars().hidden, view.hidden);
        fill_checkbox(&self.ivars().fullscreen, view.hide_in_fullscreen);
        if let Some(field) = self.ivars().hotkey.borrow().clone() {
            field.setStringValue(&NSString::from_str(&view.hide_hotkey));
        }
        if let Some(field) = self.ivars().base_url.borrow().clone() {
            field.setStringValue(&NSString::from_str(&view.director_base_url));
        }
        if let Some(field) = self.ivars().model.borrow().clone() {
            field.setStringValue(&NSString::from_str(&view.director_model));
        }
        if let Some(field) = self.ivars().api_key.borrow().clone() {
            let display = if view.api_key_set {
                view.api_key_fingerprint.clone()
            } else if !view.api_key_error.is_empty() {
                format!("(error: {})", view.api_key_error)
            } else {
                String::new()
            };
            field.setStringValue(&NSString::from_str(&display));
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
        self.fill_instances(&view, dismiss_label);
    }

    fn fill_instances(&self, view: &SettingsView, dismiss_label: &str) {
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
                    &NSString::from_str(dismiss_label),
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

fn build(mtm: MainThreadMarker, session: SettingsSession) -> Retained<SettingsController> {
    let controller = SettingsController::new(mtm);
    *controller.ivars().session.borrow_mut() = Some(session);

    let description = form::describe();

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

    let mut next_tag: isize = 1000;

    let mut director_button = None;
    let mut ambient_button = None;
    let mut base_url_field = None;
    let mut model_field = None;
    let mut api_key_field = None;
    let mut clear_key_button = None;
    let mut dnd_button = None;
    let mut hidden_button = None;
    let mut fullscreen_button = None;
    let mut hotkey_field = None;
    let mut excluded_text = None;
    let mut payload_field = None;
    let mut memory_path_field = None;
    let mut character_popup = None;
    let mut new_character_popup = None;
    let mut new_name_field = None;
    let mut instances_view = None;

    for section in &description.sections {
        cursor.heading(&section.heading);

        for row in &section.rows {
            match row {
                FormRow::Checkbox {
                    id,
                    label,
                    frozen,
                    help,
                    comment: _,
                } => {
                    let tag = next_tag;
                    next_tag += 1;
                    controller
                        .ivars()
                        .tag_to_id
                        .borrow_mut()
                        .insert(tag, id.clone());

                    let btn = checkbox(label, tag, &controller, mtm);
                    if *frozen {
                        btn.setEnabled(false);
                    }
                    cursor.place(&btn, 22.0);
                    if let Some(help_text) = help {
                        cursor.hint(help_text);
                    }

                    match id.as_str() {
                        form::DIRECTOR_ID => director_button = Some(btn),
                        form::AMBIENT_ID => ambient_button = Some(btn),
                        form::DND_ID => dnd_button = Some(btn),
                        form::HIDDEN_ID => hidden_button = Some(btn),
                        form::FULLSCREEN_ID => fullscreen_button = Some(btn),
                        _ => {}
                    }
                }
                FormRow::TextField {
                    id,
                    label,
                    placeholder,
                } => {
                    if let Some(label_text) = label {
                        let lbl =
                            NSTextField::labelWithString(&NSString::from_str(label_text), mtm);
                        cursor.place(&lbl, 18.0);
                    }
                    let field = endpoint_field(placeholder, &controller, mtm);
                    cursor.place(&field, 24.0);

                    match id.as_str() {
                        form::DIRECTOR_BASE_URL_ID => base_url_field = Some(field),
                        form::DIRECTOR_MODEL_ID => model_field = Some(field),
                        _ => {}
                    }
                }
                FormRow::SecureField { id, label } => {
                    if let Some(label_text) = label {
                        let lbl =
                            NSTextField::labelWithString(&NSString::from_str(label_text), mtm);
                        cursor.place(&lbl, 18.0);
                    }
                    let field = NSSecureTextField::initWithFrame(
                        NSSecureTextField::alloc(mtm),
                        NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(FIELD_WIDTH, 24.0)),
                    )
                    .into_super();
                    bind_commit(&field, &controller);
                    cursor.place(&field, 24.0);

                    if id == form::DIRECTOR_API_KEY_ID {
                        api_key_field = Some(field);
                    }
                }
                FormRow::InspectPath { id } => {
                    let path_field =
                        NSTextField::wrappingLabelWithString(&NSString::from_str(""), mtm);
                    path_field.setFont(NSFont::userFixedPitchFontOfSize(11.0).as_deref());
                    cursor.place(&path_field, 36.0);

                    if id == form::MEMORY_PATH_ID {
                        memory_path_field = Some(path_field);
                    }
                }
                FormRow::List {
                    id,
                    dismiss_label: _,
                } => {
                    let view = NSView::initWithFrame(
                        NSView::alloc(mtm),
                        NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(FIELD_WIDTH, 80.0)),
                    );
                    cursor.place(&view, 80.0);

                    if id == form::INSTANCES_ID {
                        instances_view = Some(view);
                    }
                }
                FormRow::InspectBlock { id, label, help } => {
                    if let Some(label_text) = label {
                        let lbl =
                            NSTextField::labelWithString(&NSString::from_str(label_text), mtm);
                        cursor.place(&lbl, 18.0);
                    }
                    if let Some(help_text) = help {
                        cursor.hint(help_text);
                    }

                    match id.as_str() {
                        form::PAYLOAD_ID => {
                            let field = inspect_block(mtm);
                            cursor.place(&field, 88.0);
                            payload_field = Some(field);
                        }
                        form::HOTKEY_ID => {
                            let field = inspect_line(mtm);
                            cursor.place(&field, 22.0);
                            hotkey_field = Some(field);
                        }
                        _ => {}
                    }
                }
                FormRow::Popup { id, .. } => {
                    let pop = popup(&controller, sel!(characterPicked:), mtm);
                    cursor.place(&pop, 24.0);

                    if id == form::CHARACTER_ID {
                        character_popup = Some(pop);
                    }
                }
                FormRow::Multiline {
                    id, help, editable, ..
                } => {
                    if let Some(help_text) = help {
                        cursor.hint(help_text);
                    }
                    let text = if *editable {
                        editable_block(&controller, mtm)
                    } else {
                        inspect_block(mtm)
                    };
                    cursor.place(&text, 88.0);

                    if id == form::EXCLUDED_ID {
                        excluded_text = Some(text);
                    }
                }
                FormRow::Composite { controls, .. } => {
                    cursor.y -= 24.0;
                    let mut x = MARGIN;

                    for control in controls {
                        match control {
                            CompositeControl::TextField { id, placeholder } => {
                                let field =
                                    NSTextField::textFieldWithString(&NSString::from_str(""), mtm);
                                field.setPlaceholderString(Some(&NSString::from_str(placeholder)));
                                field.setFrame(NSRect::new(
                                    NSPoint::new(x, cursor.y),
                                    NSSize::new(200.0, 24.0),
                                ));
                                document.addSubview(&field);
                                x += 208.0;

                                if id == form::NEW_NAME_ID {
                                    new_name_field = Some(field);
                                }
                            }
                            CompositeControl::Popup { id } => {
                                let pop = popup_plain(mtm);
                                pop.setFrame(NSRect::new(
                                    NSPoint::new(x, cursor.y),
                                    NSSize::new(180.0, 24.0),
                                ));
                                document.addSubview(&pop);
                                x += 188.0;

                                if id == form::NEW_CHARACTER_ID {
                                    new_character_popup = Some(pop);
                                }
                            }
                            CompositeControl::Button { id, label } => {
                                let tag = next_tag;
                                next_tag += 1;
                                controller
                                    .ivars()
                                    .tag_to_id
                                    .borrow_mut()
                                    .insert(tag, id.clone());

                                let btn = unsafe {
                                    NSButton::buttonWithTitle_target_action(
                                        &NSString::from_str(label),
                                        Some(&*controller),
                                        Some(sel!(handleAction:)),
                                        mtm,
                                    )
                                };
                                btn.setTag(tag);
                                btn.setFrame(NSRect::new(
                                    NSPoint::new(x, cursor.y),
                                    NSSize::new(if label.len() > 10 { 140.0 } else { 72.0 }, 24.0),
                                ));
                                document.addSubview(&btn);
                                x += if label.len() > 10 { 148.0 } else { 80.0 };

                                if id == form::CLEAR_KEY_ID {
                                    clear_key_button = Some(btn);
                                }
                            }
                        }
                    }
                    cursor.y -= 16.0;
                }
            }
        }
    }

    *controller.ivars().director.borrow_mut() = director_button;
    *controller.ivars().ambient.borrow_mut() = ambient_button;
    *controller.ivars().base_url.borrow_mut() = base_url_field;
    *controller.ivars().model.borrow_mut() = model_field;
    *controller.ivars().api_key.borrow_mut() = api_key_field;
    *controller.ivars().clear_key.borrow_mut() = clear_key_button;
    *controller.ivars().dnd.borrow_mut() = dnd_button;
    *controller.ivars().hidden.borrow_mut() = hidden_button;
    *controller.ivars().fullscreen.borrow_mut() = fullscreen_button;
    *controller.ivars().hotkey.borrow_mut() = hotkey_field;
    *controller.ivars().excluded.borrow_mut() = excluded_text;
    *controller.ivars().payload.borrow_mut() = payload_field;
    *controller.ivars().memory_path.borrow_mut() = memory_path_field;
    *controller.ivars().character.borrow_mut() = character_popup;
    *controller.ivars().new_character.borrow_mut() = new_character_popup;
    *controller.ivars().new_name.borrow_mut() = new_name_field;
    *controller.ivars().instances.borrow_mut() = instances_view;

    let scroll = NSScrollView::initWithFrame(
        NSScrollView::alloc(mtm),
        NSRect::new(
            NSPoint::new(0.0, 0.0),
            NSSize::new(WINDOW_WIDTH, WINDOW_HEIGHT),
        ),
    );
    scroll.setDocumentView(Some(&document));
    scroll.setHasVerticalScroller(true);

    let window = unsafe {
        let style = NSWindowStyleMask::Titled
            | NSWindowStyleMask::Closable
            | NSWindowStyleMask::Miniaturizable;
        let window = NSWindow::initWithContentRect_styleMask_backing_defer(
            NSWindow::alloc(mtm),
            NSRect::new(
                NSPoint::new(100.0, 100.0),
                NSSize::new(WINDOW_WIDTH, WINDOW_HEIGHT),
            ),
            style,
            NSBackingStoreType::Buffered,
            false,
        );
        window.setTitle(&NSString::from_str("Settings"));
        window.setContentView(Some(&scroll));
        window.setDelegate(Some(ProtocolObject::from_ref(&*controller)));
        window
    };

    *controller.ivars().window.borrow_mut() = Some(window.clone());
    controller.refresh();
    window.makeKeyAndOrderFront(None);

    controller
}

pub fn show(session: SettingsSession) {
    CONTROLLER.with(|cell| {
        let mtm = MainThreadMarker::new().expect("settings window from main thread");
        let mut borrow = cell.borrow_mut();

        if let Some(existing) = borrow.as_ref() {
            *existing.ivars().session.borrow_mut() = Some(session);
            existing.refresh();
            if let Some(window) = existing.ivars().window.borrow().as_ref() {
                window.makeKeyAndOrderFront(None);
            }
        } else {
            *borrow = Some(build(mtm, session));
        }
    });
}

pub fn refresh_if_showing() {
    CONTROLLER.with(|cell| {
        if let Some(controller) = cell.borrow().as_ref() {
            controller.refresh();
        }
    });
}

struct Cursor {
    y: f64,
    parent: Retained<NSView>,
    mtm: MainThreadMarker,
}

impl Cursor {
    fn place(&mut self, widget: &NSView, height: f64) {
        self.y -= height;
        widget.setFrame(NSRect::new(
            NSPoint::new(MARGIN, self.y),
            NSSize::new(FIELD_WIDTH, height),
        ));
        self.parent.addSubview(widget);
    }

    fn heading(&mut self, title: &str) {
        self.y -= 28.0;
        let label = NSTextField::labelWithString(&NSString::from_str(title), self.mtm);
        label.setFont(Some(&NSFont::boldSystemFontOfSize(14.0)));
        label.setFrame(NSRect::new(
            NSPoint::new(MARGIN, self.y),
            NSSize::new(FIELD_WIDTH, 20.0),
        ));
        self.parent.addSubview(&label);
    }

    fn hint(&mut self, text: &str) {
        self.y -= 14.0;
        let label = NSTextField::labelWithString(&NSString::from_str(text), self.mtm);
        label.setFont(Some(&NSFont::systemFontOfSize(11.0)));
        label.setTextColor(Some(&NSColor::secondaryLabelColor()));
        label.setFrame(NSRect::new(
            NSPoint::new(MARGIN, self.y),
            NSSize::new(FIELD_WIDTH, 12.0),
        ));
        self.parent.addSubview(&label);
    }
}

fn checkbox(
    title: &str,
    tag: isize,
    controller: &SettingsController,
    mtm: MainThreadMarker,
) -> Retained<NSButton> {
    let button = unsafe {
        NSButton::buttonWithTitle_target_action(
            &NSString::from_str(title),
            Some(controller),
            Some(sel!(toggle:)),
            mtm,
        )
    };
    button.setButtonType(objc2_app_kit::NSButtonType::Switch);
    button.setTag(tag);
    button
}

fn inspect_line(mtm: MainThreadMarker) -> Retained<NSTextField> {
    let field = NSTextField::labelWithString(&NSString::from_str(""), mtm);
    field.setFont(NSFont::userFixedPitchFontOfSize(11.0).as_deref());
    field
}

fn inspect_block(mtm: MainThreadMarker) -> Retained<NSTextField> {
    let field = NSTextField::wrappingLabelWithString(&NSString::from_str(""), mtm);
    field.setFont(NSFont::userFixedPitchFontOfSize(11.0).as_deref());
    field
}

fn endpoint_field(
    placeholder: &str,
    controller: &SettingsController,
    mtm: MainThreadMarker,
) -> Retained<NSTextField> {
    let field = NSTextField::textFieldWithString(&NSString::from_str(""), mtm);
    field.setPlaceholderString(Some(&NSString::from_str(placeholder)));
    bind_commit(&field, controller);
    field
}

fn bind_commit(field: &NSTextField, controller: &SettingsController) {
    field.setDelegate(Some(ProtocolObject::from_ref(controller)));
    if let Some(cell) = field.cell() {
        unsafe {
            let _: () = msg_send![&cell, setSendsActionOnEndEditing: true];
        }
    }
    field.setTarget(Some(controller));
    field.setAction(Some(sel!(excludedEnded:)));
}

fn editable_block(controller: &SettingsController, mtm: MainThreadMarker) -> Retained<NSTextView> {
    let text = unsafe {
        NSTextView::initWithFrame(
            NSTextView::alloc(mtm),
            NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(FIELD_WIDTH, 88.0)),
        )
    };
    text.setFont(NSFont::userFixedPitchFontOfSize(11.0).as_deref());
    text.setDelegate(Some(ProtocolObject::from_ref(controller)));
    text
}

fn popup(
    controller: &SettingsController,
    action: objc2::runtime::Sel,
    mtm: MainThreadMarker,
) -> Retained<NSPopUpButton> {
    let popup = NSPopUpButton::initWithFrame_pullsDown(
        NSPopUpButton::alloc(mtm),
        NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(FIELD_WIDTH, 24.0)),
        false,
    );
    popup.setTarget(Some(controller));
    popup.setAction(Some(action));
    popup
}

fn popup_plain(mtm: MainThreadMarker) -> Retained<NSPopUpButton> {
    NSPopUpButton::initWithFrame_pullsDown(
        NSPopUpButton::alloc(mtm),
        NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(180.0, 24.0)),
        false,
    )
}

fn fill_checkbox(cell: &RefCell<Option<Retained<NSButton>>>, value: bool) {
    if let Some(button) = cell.borrow().clone() {
        button.setState(if value {
            NSControlStateValueOn
        } else {
            NSControlStateValueOff
        });
    }
}

fn fill_popup(cell: &RefCell<Option<Retained<NSPopUpButton>>>, options: &[String], current: &str) {
    let Some(popup) = cell.borrow().clone() else {
        return;
    };
    popup.removeAllItems();
    for option in options {
        popup.addItemWithTitle(&NSString::from_str(option));
    }
    popup.selectItemWithTitle(Some(&NSString::from_str(current)));
}
