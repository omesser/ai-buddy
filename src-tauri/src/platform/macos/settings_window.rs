//! Native settings. Checkboxes and fields, not a webview.
//!
//! SPEC gives the webview to the sprite and the chat surface. Settings is
//! Shell furniture, the same as the tray menu, so it is AppKit.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;

use objc2::rc::Retained;
use objc2::runtime::{AnyObject, ProtocolObject};
use objc2::{define_class, msg_send, sel, DefinedClass, MainThreadOnly};
use objc2_app_kit::{
    NSAlert, NSAlertFirstButtonReturn, NSApplication, NSAutoresizingMaskOptions,
    NSBackingStoreType, NSBox, NSBoxType, NSButton, NSColor, NSControlStateValueOff,
    NSControlStateValueOn, NSControlTextEditingDelegate, NSFont, NSPopUpButton, NSScrollView,
    NSSecureTextField, NSStatusWindowLevel, NSTabView, NSTabViewItem, NSTextDelegate, NSTextField,
    NSTextFieldDelegate, NSTextView, NSTextViewDelegate, NSView, NSWindow, NSWindowDelegate,
    NSWindowLevel, NSWindowStyleMask,
};
use objc2_foundation::{
    MainThreadMarker, NSNotification, NSObject, NSObjectProtocol, NSPoint, NSRect, NSSize, NSString,
};

use crate::settings::form::{self, CompositeControl, FormRow};
use crate::settings::{DirectorDraft, SettingsPatch, SettingsSession, SettingsView};

const WINDOW_WIDTH: f64 = 560.0;
const WINDOW_HEIGHT: f64 = 720.0;
const DOC_HEIGHT: f64 = 1600.0;
const MARGIN: f64 = 28.0;
const FIELD_WIDTH: f64 = WINDOW_WIDTH - MARGIN * 2.0;
/// The gap above a row, and the smaller one above a help line. The ratio
/// between them is the only thing that says which control a help line
/// describes; equal gaps read as a caption for the row below.
const ROW_GAP: f64 = 12.0;
const HINT_GAP: f64 = 4.0;
/// A section break: the rule above a heading and the heading's own leading
/// gap. Larger than `ROW_GAP`, so a heading groups with the rows under it.
const SECTION_GAP: f64 = 24.0;
thread_local! {
    static CONTROLLER: RefCell<Option<Retained<SettingsController>>> = const { RefCell::new(None) };
}

#[derive(Default)]
struct Ivars {
    session: RefCell<Option<SettingsSession>>,
    window: RefCell<Option<Retained<NSWindow>>>,
    tab_view: RefCell<Option<Retained<NSTabView>>>,
    /// Each tab's scroll view and the height its rows need, in tab order.
    panes: RefCell<Vec<(Retained<NSScrollView>, f64)>>,
    director: RefCell<Option<Retained<NSButton>>>,
    base_url: RefCell<Option<Retained<NSTextField>>>,
    model: RefCell<Option<Retained<NSTextField>>>,
    api_key: RefCell<Option<Retained<NSTextField>>>,
    clear_key: RefCell<Option<Retained<NSButton>>>,
    apply: RefCell<Option<Retained<NSButton>>>,
    cancel: RefCell<Option<Retained<NSButton>>>,
    /// Clear key was clicked and Apply has not run yet. The whole of the
    /// staged delete: the key field itself is blank either way, so nothing
    /// else could tell a staged clear from an untouched field (#279).
    clear_pending: Cell<bool>,
    ambient: RefCell<Option<Retained<NSButton>>>,
    dnd: RefCell<Option<Retained<NSButton>>>,
    sound: RefCell<Option<Retained<NSButton>>>,
    hidden: RefCell<Option<Retained<NSButton>>>,
    fullscreen: RefCell<Option<Retained<NSButton>>>,
    tag_to_id: RefCell<HashMap<isize, String>>,
    /// Every checkbox and text field in the window, by row id, so `refresh`
    /// can draw the rows `SettingsView` carries in a map rather than a named
    /// field. A named ivar per Development switch is the same value written in
    /// ten places (#273).
    checkboxes: RefCell<Vec<(String, Retained<NSButton>)>>,
    fields: RefCell<Vec<(String, Retained<NSTextField>)>>,
    consent: RefCell<Vec<Retained<NSButton>>>,
    consent_intro: RefCell<Option<Retained<NSTextField>>>,
    hotkey: RefCell<Option<Retained<NSTextField>>>,
    excluded: RefCell<Option<Retained<NSTextView>>>,
    payload: RefCell<Option<Retained<NSTextField>>>,
    memory_path: RefCell<Option<Retained<NSTextField>>>,
    character: RefCell<Option<Retained<NSPopUpButton>>>,
    harness: RefCell<Option<Retained<NSPopUpButton>>>,
    harness_state: RefCell<Option<Retained<NSTextField>>>,
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

        #[unsafe(method(windowDidResize:))]
        fn did_resize(&self, _notification: &NSNotification) {
            self.fit_to_window();
        }

        /// Nothing staged outlives the tab. The window is only ordered out,
        /// never destroyed — `show` reuses this controller — so without this
        /// a typed key would sit in the secure field until the next reopen,
        /// one Apply click from the store (#279).
        #[unsafe(method(windowWillClose:))]
        fn will_close(&self, _notification: &NSNotification) {
            self.draw(true);
        }
    }

    unsafe impl NSTextDelegate for SettingsController {
        #[unsafe(method(textDidEndEditing:))]
        fn text_did_end_editing(&self, _notification: &NSNotification) {
            self.commit_excluded();
        }
    }

    unsafe impl NSTextViewDelegate for SettingsController {}

    unsafe impl NSControlTextEditingDelegate for SettingsController {
        #[unsafe(method(controlTextDidChange:))]
        fn control_text_did_change(&self, _notification: &NSNotification) {
            self.update_director_buttons();
        }
    }

    unsafe impl NSTextFieldDelegate for SettingsController {}

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

            let Some(field) = form::describe().bool_write(id) else {
                return;
            };
            let mut patch = SettingsPatch::default();
            patch.set_bool(field, on);
            self.apply(patch);
        }

        #[unsafe(method(endpointEnded:))]
        fn endpoint_ended(&self, sender: Option<&AnyObject>) {
            let Some(field) = sender.and_then(|s| s.downcast_ref::<NSTextField>()) else {
                return;
            };
            let tag = field.tag();
            let tag_to_id = self.ivars().tag_to_id.borrow();
            let Some(id) = tag_to_id.get(&tag) else {
                return;
            };

            let Some(writes) = form::describe().text_write(id) else {
                return;
            };

            let text = field.stringValue().to_string();
            let mut patch = SettingsPatch::default();
            if !patch.set_text(writes, &text) {
                return;
            }
            self.apply(patch);
        }

        /// Every writing popup, by the tag the pick carries. Reached the same
        /// way `endpointEnded:` reaches a field, so a second popup needs no
        /// selector of its own (#436).
        #[unsafe(method(popupPicked:))]
        fn popup_picked(&self, sender: Option<&AnyObject>) {
            let Some(popup) = sender.and_then(|s| s.downcast_ref::<NSPopUpButton>()) else {
                return;
            };
            let tag = popup.tag();
            let tag_to_id = self.ivars().tag_to_id.borrow();
            let Some(id) = tag_to_id.get(&tag) else {
                return;
            };
            let Some(writes) = form::describe().text_write(id) else {
                return;
            };
            let Some(title) = popup.titleOfSelectedItem() else {
                return;
            };
            let mut patch = SettingsPatch::default();
            if !patch.set_text(writes, &title.to_string()) {
                return;
            }
            self.apply(patch);
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
            let Some(op) = description.operations.get(id) else {
                return;
            };

            match op {
                form::RowOperation::Spawn => self.do_spawn(),
                form::RowOperation::OpenMemory => self.do_memory_open(),
                form::RowOperation::WipeMemory => self.do_memory_wipe(),
                form::RowOperation::ClearKey => self.do_clear_key(),
                form::RowOperation::Apply => self.do_apply(),
                form::RowOperation::Cancel => self.do_cancel(),
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
        // SAFETY: NSObject's init takes no arguments. alloc+init without
        // set_ivars leaves the drop flag Allocated; the next ivars() panics
        // ("tried to access uninitialized instance variable"). #205 dropped
        // this while rewriting the form as data.
        unsafe { msg_send![super(this), init] }
    }

    fn mtm(&self) -> MainThreadMarker {
        MainThreadMarker::from(self)
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
        alert.setInformativeText(&NSString::from_str("A backup is kept beside the file."));
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

    /// Stage the delete rather than write it. Applying here would drop the
    /// session history before the endpoint typed beside it was ever sent, and
    /// Cancel could not take it back (#279).
    fn do_clear_key(&self) {
        self.ivars().clear_pending.set(true);
        if let Some(field) = self.ivars().api_key.borrow().clone() {
            field.setStringValue(&NSString::from_str(""));
        }
        self.update_director_buttons();
    }

    /// The Director tab as the window holds it right now.
    ///
    /// The fields are read back here rather than mirrored in a draft buffer:
    /// the typed key lives in the `NSSecureTextField` and nowhere else, so it
    /// cannot reach the settings file, and closing the window resets it
    /// (#279).
    fn director_draft<'a>(&self, description: &'a form::FormDescription) -> DirectorDraft<'a> {
        let ivars = self.ivars();
        DirectorDraft {
            base_url: field_text(&ivars.base_url),
            model: field_text(&ivars.model),
            key: field_text(&ivars.api_key),
            clear_key: ivars.clear_pending.get(),
            description,
        }
    }

    /// Which fields a redraw must leave alone.
    fn director_staged(&self, view: &SettingsView) -> crate::settings::Staged {
        let description = form::describe();
        self.director_draft(&description).staged(view)
    }

    /// Both buttons say whether there is anything to apply.
    fn update_director_buttons(&self) {
        let Some(view) = self.ivars().session.borrow().as_ref().map(|s| s.view()) else {
            return;
        };
        self.set_director_buttons(&view);
    }

    /// The same, against a view the caller already holds: `draw` has one, and
    /// must not take the session borrow a second time.
    fn set_director_buttons(&self, view: &SettingsView) {
        let description = form::describe();
        let dirty = self.director_draft(&description).patch(view).is_some();
        for cell in [&self.ivars().apply, &self.ivars().cancel] {
            if let Some(button) = cell.borrow().clone() {
                button.setEnabled(dirty);
            }
        }
    }

    /// Resets only once the write landed. A locked Keychain fails
    /// `write_director_key` before the file is touched, and discarding the
    /// typed endpoint on the way out would lose an edit nothing saved (#279).
    fn do_apply(&self) {
        let Some(view) = self.ivars().session.borrow().as_ref().map(|s| s.view()) else {
            return;
        };
        let description = form::describe();
        if let Some(patch) = self.director_draft(&description).patch(&view) {
            if !self.apply(patch) {
                return;
            }
        }
        // Resets even though the store now holds what the key field still
        // shows: only a reset takes the typed key back out of it.
        self.draw(true);
    }

    /// Writes neither the file nor the store: the reset draws every field
    /// from live state, and blanking the key field is what drops the typed
    /// one on the floor.
    fn do_cancel(&self) {
        self.draw(true);
    }

    /// The field comes off the row itself rather than a literal, so the blur
    /// and the row cannot disagree about which field the text belongs to.
    fn commit_excluded(&self) {
        let text = self
            .ivars()
            .excluded
            .borrow()
            .as_ref()
            .map(|field| field.string().to_string())
            .unwrap_or_default();
        let Some(writes) = form::describe().text_write(form::EXCLUDED_ID) else {
            return;
        };
        let mut patch = SettingsPatch::default();
        if !patch.set_text(writes, &text) {
            return;
        }
        self.apply(patch);
    }

    /// Whether the write landed, so Apply knows not to discard a staged edit
    /// nothing saved.
    fn apply(&self, patch: SettingsPatch) -> bool {
        let result = {
            let session = self.ivars().session.borrow();
            let Some(session) = session.as_ref() else {
                return false;
            };
            session.apply(patch)
        };
        match result {
            Ok(()) => true,
            Err(why) => {
                eprintln!("settings: {why}");
                let alert = NSAlert::new(self.mtm());
                alert.setMessageText(&NSString::from_str("Could not save settings"));
                alert.setInformativeText(&NSString::from_str(&why));
                alert.addButtonWithTitle(&NSString::from_str("OK"));
                alert.runModal();
                false
            }
        }
    }

    /// Redraw from live state, leaving anything staged on the Director tab
    /// alone.
    ///
    /// Every caller but Apply and Cancel arrives unasked: `windowDidBecomeKey`
    /// fires on a click back into the window, and `frame_loop` refreshes after
    /// any `SettingsOp`. Overwriting a half-typed endpoint on either would
    /// make batching lossy in ordinary use — switch apps and the edit is gone
    /// — so a dirty tab is the one thing a redraw does not touch (#279).
    fn refresh(&self) {
        self.draw(false);
    }

    /// `reset_director` is Apply and Cancel: the two callers that mean to take
    /// the staged fields back to live state.
    fn draw(&self, reset_director: bool) {
        let Some(view) = self.ivars().session.borrow().as_ref().map(|s| s.view()) else {
            return;
        };
        let description = form::describe();
        // Read before any setter, so this is what the fields held on entry.
        let staged = if reset_director {
            // The staged delete is part of what a reset drops: left set, it
            // would outlive the reset and re-arm Apply on the next redraw.
            self.ivars().clear_pending.set(false);
            crate::settings::Staged::default()
        } else {
            self.director_staged(&view)
        };
        let dismiss_label = description
            .sections()
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
        fill_checkbox(&self.ivars().sound, view.sound);
        fill_checkbox(&self.ivars().hidden, view.hidden);
        fill_checkbox(&self.ivars().fullscreen, view.hide_in_fullscreen);
        {
            let buttons = self.ivars().consent.borrow();
            for (button, row) in buttons.iter().zip(view.consent.iter()) {
                button.setState(if row.granted {
                    NSControlStateValueOn
                } else {
                    NSControlStateValueOff
                });
            }
        }
        if let Some(field) = self.ivars().consent_intro.borrow().clone() {
            field.setStringValue(&NSString::from_str(&view.consent_intro()));
        }
        if let Some(field) = self.ivars().hotkey.borrow().clone() {
            field.setStringValue(&NSString::from_str(&view.hide_hotkey));
        }
        if let Some(field) = self.ivars().api_key.borrow().clone() {
            // The placeholder names the stored key, not the typed one, so it
            // is safe to redraw over a staged edit.
            field.setPlaceholderString(Some(&NSString::from_str(&view.api_key_placeholder())));
        }
        if !staged.base_url {
            if let Some(field) = self.ivars().base_url.borrow().clone() {
                field.setStringValue(&NSString::from_str(&view.director_base_url));
            }
        }
        if !staged.model {
            if let Some(field) = self.ivars().model.borrow().clone() {
                field.setStringValue(&NSString::from_str(&view.director_model));
            }
        }
        if !staged.key {
            if let Some(field) = self.ivars().api_key.borrow().clone() {
                field.setStringValue(&NSString::from_str(""));
            }
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
        for (id, button) in self.ivars().checkboxes.borrow().iter() {
            if let Some(&on) = view.development_switches.get(id) {
                button.setState(if on {
                    NSControlStateValueOn
                } else {
                    NSControlStateValueOff
                });
            }
        }
        for (id, field) in self.ivars().fields.borrow().iter() {
            if let Some(text) = view.development_texts.get(id) {
                field.setStringValue(&NSString::from_str(text));
            }
        }
        if let Some(field) = self.ivars().harness_state.borrow().clone() {
            field.setStringValue(&NSString::from_str(&view.harness_state));
        }
        fill_popup(&self.ivars().character, &view.installed, &view.character);
        // Static choices, so they come from the form rather than the view.
        fill_popup(
            &self.ivars().harness,
            &form::harness_options(),
            &view.harness,
        );
        fill_popup(
            &self.ivars().new_character,
            &view.installed,
            &view.character,
        );
        self.fill_instances(&view, dismiss_label);
        self.set_director_buttons(&view);
    }

    fn fit_to_window(&self) {
        let ivars = self.ivars();
        let Some(window) = ivars.window.borrow().clone() else {
            return;
        };
        let Some(tab_view) = ivars.tab_view.borrow().clone() else {
            return;
        };
        let Some(content) = window.contentView() else {
            return;
        };
        tab_view.setFrame(content.bounds());
        // An unselected tab's view is off the window, and AppKit sizes it only
        // when it comes on. Every pane is sized here so the tab it belongs to
        // is laid out before the user reaches it.
        let pane_frame = tab_view.contentRect();
        let panes = ivars.panes.borrow();
        for (scroll, needed) in panes.iter() {
            scroll.setFrame(pane_frame);
            let Some(document) = scroll.documentView() else {
                continue;
            };
            let visible = scroll.contentSize();
            size_document(
                &document,
                NSSize::new(visible.width, needed.max(visible.height)),
            );
        }
        if let Some(field) = ivars.consent_intro.borrow().as_ref() {
            // Every pane is the same width, so the first one's is the wrap width.
            if let Some((scroll, _)) = panes.first() {
                field.setPreferredMaxLayoutWidth(scroll.contentSize().width - MARGIN * 2.0);
            }
        }
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
            stretch_x(&label);
            // SAFETY: buttonWithTitle_target_action does not retain its target,
            // so `self` must outlive the button. The `CONTROLLER` thread-local
            // holds it for the life of the process, and it implements dismiss:.
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
            pin_right(&dismiss);
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

    let mut next_tag: isize = 1000;

    let mut director_button = None;
    let mut ambient_button = None;
    let mut base_url_field = None;
    let mut model_field = None;
    let mut api_key_field = None;
    let mut clear_key_button = None;
    let mut apply_button = None;
    let mut cancel_button = None;
    let mut dnd_button = None;
    let mut sound_button = None;
    let mut hidden_button = None;
    let mut fullscreen_button = None;
    let mut consent_buttons = Vec::new();
    let mut consent_intro = None;
    let mut hotkey_field = None;
    let mut excluded_text = None;
    let mut payload_field = None;
    let mut memory_path_field = None;
    let mut character_popup = None;
    let mut harness_popup = None;
    let mut harness_state_field = None;
    let mut new_character_popup = None;
    let mut new_name_field = None;
    let mut instances_view = None;

    let tab_view = NSTabView::initWithFrame(
        NSTabView::alloc(mtm),
        NSRect::new(
            NSPoint::new(0.0, 0.0),
            NSSize::new(WINDOW_WIDTH, WINDOW_HEIGHT),
        ),
    );
    stretch_xy(&tab_view);
    let mut panes = Vec::new();

    for tab in &description.tabs {
        let document = NSView::initWithFrame(
            NSView::alloc(mtm),
            NSRect::new(
                NSPoint::new(0.0, 0.0),
                NSSize::new(WINDOW_WIDTH, DOC_HEIGHT),
            ),
        );
        let mut cursor = Cursor {
            y: DOC_HEIGHT,
            parent: document.clone(),
            mtm,
        };

        for (index, section) in tab.sections.iter().enumerate() {
            if index > 0 {
                cursor.rule();
            }
            cursor.heading(&section.heading);
            if let Some(comment) = &section.comment {
                let field = cursor.section_comment(comment);
                if section.heading == "What the buddy can see" {
                    consent_intro = Some(field);
                }
            }

            for row in &section.rows {
                match row {
                    FormRow::Checkbox {
                        id,
                        label,
                        frozen,
                        help,
                        writes: _,
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

                        controller
                            .ivars()
                            .checkboxes
                            .borrow_mut()
                            .push((id.clone(), btn.clone()));

                        match id.as_str() {
                            form::DIRECTOR_ID => director_button = Some(btn.clone()),
                            form::AMBIENT_ID => ambient_button = Some(btn.clone()),
                            form::DND_ID => dnd_button = Some(btn.clone()),
                            form::SOUND_ID => sound_button = Some(btn.clone()),
                            form::HIDDEN_ID => hidden_button = Some(btn.clone()),
                            form::FULLSCREEN_ID => fullscreen_button = Some(btn.clone()),
                            form::CONSENT_ACCESSIBILITY_ID | form::CONSENT_SCREEN_RECORDING_ID => {
                                consent_buttons.push(btn.clone());
                            }
                            _ => {}
                        }
                    }
                    FormRow::TextField {
                        id,
                        label,
                        placeholder,
                        frozen,
                        batched,
                        writes: _,
                    } => {
                        if let Some(label_text) = label {
                            let lbl =
                                NSTextField::labelWithString(&NSString::from_str(label_text), mtm);
                            cursor.place(&lbl, 18.0);
                        }
                        let field = endpoint_field(placeholder, mtm);
                        tag_field(&field, id, &mut next_tag, &controller);
                        freeze_or_bind(&field, *frozen, *batched, &controller);
                        cursor.place(&field, 24.0);

                        controller
                            .ivars()
                            .fields
                            .borrow_mut()
                            .push((id.clone(), field.clone()));

                        match id.as_str() {
                            form::DIRECTOR_BASE_URL_ID => base_url_field = Some(field),
                            form::DIRECTOR_MODEL_ID => model_field = Some(field),
                            _ => {}
                        }
                    }
                    FormRow::SecureField {
                        id,
                        label,
                        frozen,
                        writes: _,
                    } => {
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
                        tag_field(&field, id, &mut next_tag, &controller);
                        // Always batched: `FormRow::SecureField` offers no
                        // other mode.
                        freeze_or_bind(&field, *frozen, true, &controller);
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
                        help,
                    } => {
                        let view = NSView::initWithFrame(
                            NSView::alloc(mtm),
                            NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(FIELD_WIDTH, 80.0)),
                        );
                        cursor.place(&view, 80.0);

                        if id == form::INSTANCES_ID {
                            instances_view = Some(view);
                        }

                        if let Some(help_text) = help {
                            cursor.hint(help_text);
                        }
                    }
                    FormRow::InspectBlock { id, label, help } => {
                        if let Some(label_text) = label {
                            let lbl =
                                NSTextField::labelWithString(&NSString::from_str(label_text), mtm);
                            cursor.place(&lbl, 18.0);
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
                            form::HARNESS_STATE_ID => {
                                let field = inspect_block(mtm);
                                cursor.place(&field, 44.0);
                                harness_state_field = Some(field);
                            }
                            _ => {}
                        }

                        if let Some(help_text) = help {
                            cursor.hint(help_text);
                        }
                    }
                    FormRow::Popup {
                        id,
                        label,
                        help,
                        frozen,
                        writes: _,
                        options: _,
                    } => {
                        if let Some(label_text) = label {
                            let lbl =
                                NSTextField::labelWithString(&NSString::from_str(label_text), mtm);
                            cursor.place(&lbl, 18.0);
                        }
                        let pop = popup(&controller, sel!(popupPicked:), mtm);
                        let tag = next_tag;
                        next_tag += 1;
                        pop.setTag(tag);
                        controller
                            .ivars()
                            .tag_to_id
                            .borrow_mut()
                            .insert(tag, id.clone());
                        pop.setEnabled(!frozen);
                        cursor.place(&pop, 24.0);

                        match id.as_str() {
                            form::CHARACTER_ID => character_popup = Some(pop),
                            form::HARNESS_ID => harness_popup = Some(pop),
                            _ => {}
                        }

                        if let Some(help_text) = help {
                            cursor.hint(help_text);
                        }
                    }
                    FormRow::Multiline {
                        id, help, editable, ..
                    } => {
                        if *editable {
                            let text = editable_block(&controller, mtm);
                            cursor.place(&text, 88.0);
                            if id == form::EXCLUDED_ID {
                                excluded_text = Some(text);
                            }
                        } else {
                            let field = inspect_block(mtm);
                            cursor.place(&field, 88.0);
                        }
                        if let Some(help_text) = help {
                            cursor.hint(help_text);
                        }
                    }
                    FormRow::Composite { controls, help, .. } => {
                        cursor.y -= 24.0 + ROW_GAP;
                        let mut x = MARGIN;

                        for control in controls {
                            match control {
                                CompositeControl::TextField { id, placeholder } => {
                                    let field = NSTextField::textFieldWithString(
                                        &NSString::from_str(""),
                                        mtm,
                                    );
                                    field.setPlaceholderString(Some(&NSString::from_str(
                                        placeholder,
                                    )));
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
                                    stretch_x(&pop);
                                    document.addSubview(&pop);
                                    x += 188.0;

                                    if id == form::NEW_CHARACTER_ID {
                                        new_character_popup = Some(pop);
                                    }
                                }
                                CompositeControl::Button { id, label, frozen } => {
                                    let tag = next_tag;
                                    next_tag += 1;
                                    controller
                                        .ivars()
                                        .tag_to_id
                                        .borrow_mut()
                                        .insert(tag, id.clone());

                                    // SAFETY: the target is not retained, and the
                                    // `CONTROLLER` thread-local holds the controller
                                    // for the life of the process. It implements
                                    // handleAction:.
                                    let btn = unsafe {
                                        NSButton::buttonWithTitle_target_action(
                                            &NSString::from_str(label),
                                            Some(&*controller),
                                            Some(sel!(handleAction:)),
                                            mtm,
                                        )
                                    };
                                    btn.setTag(tag);
                                    btn.setEnabled(!frozen);
                                    btn.setFrame(NSRect::new(
                                        NSPoint::new(x, cursor.y),
                                        NSSize::new(
                                            if label.len() > 10 { 140.0 } else { 72.0 },
                                            24.0,
                                        ),
                                    ));
                                    if id == form::SPAWN_ID {
                                        pin_right(&btn);
                                    }
                                    document.addSubview(&btn);
                                    x += if label.len() > 10 { 148.0 } else { 80.0 };

                                    match id.as_str() {
                                        form::CLEAR_KEY_ID => clear_key_button = Some(btn),
                                        form::APPLY_ID => apply_button = Some(btn),
                                        form::CANCEL_ID => cancel_button = Some(btn),
                                        _ => {}
                                    }
                                }
                            }
                        }

                        if let Some(help_text) = help {
                            cursor.hint(help_text);
                        }
                    }
                }
            }
        }

        // Rows count down from the top of a document whose height is a guess
        // until the tab is finished. `fit_to_window` sizes each document from
        // what its rows used, so a short tab does not scroll past its last row.
        let needed = DOC_HEIGHT - cursor.y + MARGIN;

        let scroll = NSScrollView::initWithFrame(
            NSScrollView::alloc(mtm),
            NSRect::new(
                NSPoint::new(0.0, 0.0),
                NSSize::new(WINDOW_WIDTH, WINDOW_HEIGHT),
            ),
        );
        scroll.setDocumentView(Some(&document));
        scroll.setHasVerticalScroller(true);
        scroll.setHasHorizontalScroller(false);
        stretch_xy(&scroll);

        let item = NSTabViewItem::new();
        item.setLabel(&NSString::from_str(&tab.title));
        item.setView(Some(&scroll));
        tab_view.addTabViewItem(&item);
        panes.push((scroll, needed));
    }

    *controller.ivars().director.borrow_mut() = director_button;
    *controller.ivars().ambient.borrow_mut() = ambient_button;
    *controller.ivars().base_url.borrow_mut() = base_url_field;
    *controller.ivars().model.borrow_mut() = model_field;
    *controller.ivars().api_key.borrow_mut() = api_key_field;
    *controller.ivars().clear_key.borrow_mut() = clear_key_button;
    *controller.ivars().apply.borrow_mut() = apply_button;
    *controller.ivars().cancel.borrow_mut() = cancel_button;
    *controller.ivars().dnd.borrow_mut() = dnd_button;
    *controller.ivars().sound.borrow_mut() = sound_button;
    *controller.ivars().hidden.borrow_mut() = hidden_button;
    *controller.ivars().fullscreen.borrow_mut() = fullscreen_button;
    *controller.ivars().consent.borrow_mut() = consent_buttons;
    *controller.ivars().consent_intro.borrow_mut() = consent_intro;
    *controller.ivars().hotkey.borrow_mut() = hotkey_field;
    *controller.ivars().excluded.borrow_mut() = excluded_text;
    *controller.ivars().payload.borrow_mut() = payload_field;
    *controller.ivars().memory_path.borrow_mut() = memory_path_field;
    *controller.ivars().character.borrow_mut() = character_popup;
    *controller.ivars().harness.borrow_mut() = harness_popup;
    *controller.ivars().harness_state.borrow_mut() = harness_state_field;
    *controller.ivars().new_character.borrow_mut() = new_character_popup;
    *controller.ivars().new_name.borrow_mut() = new_name_field;
    *controller.ivars().instances.borrow_mut() = instances_view;

    let style = NSWindowStyleMask::Titled
        | NSWindowStyleMask::Closable
        | NSWindowStyleMask::Miniaturizable
        | NSWindowStyleMask::Resizable;
    // SAFETY: NSWindow's designated initializer, over a window this call just
    // allocated and has not yet handed anywhere else.
    let window = unsafe {
        NSWindow::initWithContentRect_styleMask_backing_defer(
            NSWindow::alloc(mtm),
            NSRect::new(
                NSPoint::new(100.0, 100.0),
                NSSize::new(WINDOW_WIDTH, WINDOW_HEIGHT),
            ),
            style,
            NSBackingStoreType::Buffered,
            false,
        )
    };
    window.setTitle(&NSString::from_str("Settings"));
    window.setContentView(Some(&tab_view));
    window.setMinSize(NSSize::new(WINDOW_WIDTH, 400.0));
    retain_after_close(&window);
    window.setDelegate(Some(ProtocolObject::from_ref(&*controller)));

    *controller.ivars().window.borrow_mut() = Some(window.clone());
    *controller.ivars().tab_view.borrow_mut() = Some(tab_view);
    *controller.ivars().panes.borrow_mut() = panes;
    controller.fit_to_window();
    controller.refresh();
    raise(&window, mtm);

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
                raise(window, mtm);
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
    fn put(&mut self, widget: &NSView, height: f64, gap: f64) {
        self.y -= height + gap;
        widget.setFrame(NSRect::new(
            NSPoint::new(MARGIN, self.y),
            NSSize::new(FIELD_WIDTH, height),
        ));
        stretch_x(widget);
        self.parent.addSubview(widget);
    }

    fn place(&mut self, widget: &NSView, height: f64) {
        self.put(widget, height, ROW_GAP);
    }

    fn heading(&mut self, title: &str) {
        let label = NSTextField::labelWithString(&NSString::from_str(title), self.mtm);
        label.setFont(Some(&NSFont::boldSystemFontOfSize(14.0)));
        self.put(&label, 20.0, SECTION_GAP);
    }

    /// A rule above every heading but a tab's first, so the heading reads as
    /// the start of the group below it rather than another row in the one
    /// above.
    fn rule(&mut self) {
        let rule = NSBox::initWithFrame(
            NSBox::alloc(self.mtm),
            NSRect::new(NSPoint::new(MARGIN, 0.0), NSSize::new(FIELD_WIDTH, 1.0)),
        );
        rule.setBoxType(NSBoxType::Separator);
        self.put(&rule, 1.0, SECTION_GAP);
    }

    fn section_comment(&mut self, text: &str) -> Retained<NSTextField> {
        let label = NSTextField::wrappingLabelWithString(&NSString::from_str(text), self.mtm);
        label.setTextColor(Some(&NSColor::secondaryLabelColor()));
        label.setFont(Some(&NSFont::systemFontOfSize(11.0)));
        let height = wrapped_height(&label);
        self.put(&label, height, HINT_GAP);
        label
    }

    fn hint(&mut self, text: &str) {
        let label = NSTextField::wrappingLabelWithString(&NSString::from_str(text), self.mtm);
        label.setFont(Some(&NSFont::systemFontOfSize(11.0)));
        label.setTextColor(Some(&NSColor::secondaryLabelColor()));
        let height = wrapped_height(&label);
        self.put(&label, height, HINT_GAP);
    }
}

fn checkbox(
    title: &str,
    tag: isize,
    controller: &SettingsController,
    mtm: MainThreadMarker,
) -> Retained<NSButton> {
    // SAFETY: buttonWithTitle_target_action does not retain its target, so the
    // controller must outlive the button. The `CONTROLLER` thread-local holds it
    // for the life of the process, and it implements toggle:.
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

/// How tall a wrapping label has to be to show all of its text at
/// `FIELD_WIDTH`.
///
/// Measured rather than assumed: help text is written in `settings::form` and
/// a fixed height silently clips the sentence when someone lengthens it.
/// Widening the window only ever frees space, since `stretch_x` lets the label
/// grow and wrap into fewer lines.
///
/// Untested, and not testable in this harness: `fittingSize` drives autolayout,
/// which aborts the process off the main thread, and `cargo test` runs on a
/// worker. `Cursor` carries a `MainThreadMarker`, so a caller cannot reach here
/// from anywhere else.
fn wrapped_height(label: &NSTextField) -> f64 {
    label.setPreferredMaxLayoutWidth(FIELD_WIDTH);
    label.fittingSize().height.ceil()
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

fn endpoint_field(placeholder: &str, mtm: MainThreadMarker) -> Retained<NSTextField> {
    let field = NSTextField::textFieldWithString(&NSString::from_str(""), mtm);
    field.setPlaceholderString(Some(&NSString::from_str(placeholder)));
    field
}

/// Tag the field with a fresh number and record which row it is, so
/// `endpointEnded:` can tell one field from another.
fn tag_field(field: &NSTextField, id: &str, next_tag: &mut isize, controller: &SettingsController) {
    let tag = *next_tag;
    *next_tag += 1;
    field.setTag(tag);
    controller
        .ivars()
        .tag_to_id
        .borrow_mut()
        .insert(tag, id.to_string());
}

/// Read-only rather than disabled, so the value stays legible and copyable.
///
/// A batched field gets the delegate and no target: `controlTextDidChange:`
/// is what drives Apply and Cancel, and an action here would be the blur
/// commit the tab no longer does (#279).
fn freeze_or_bind(
    field: &NSTextField,
    frozen: bool,
    batched: bool,
    controller: &SettingsController,
) {
    if frozen {
        field.setEditable(false);
    } else if batched {
        // SAFETY: setDelegate: does not retain, so the delegate must outlive
        // the field. The `CONTROLLER` thread-local holds the controller for the
        // life of the process and is never cleared, so it does.
        unsafe {
            field.setDelegate(Some(ProtocolObject::from_ref(controller)));
        }
    } else {
        bind_commit(field, controller);
    }
}

/// What a Director field holds, or `None` when a variable owns the row.
fn field_text(cell: &RefCell<Option<Retained<NSTextField>>>) -> String {
    cell.borrow()
        .clone()
        .map(|field| field.stringValue().to_string())
        .unwrap_or_default()
}

/// Commit on Return and on blur.
fn bind_commit(field: &NSTextField, controller: &SettingsController) {
    // SAFETY: setDelegate: does not retain, so the delegate must outlive the
    // field. The `CONTROLLER` thread-local holds the controller for the life of
    // the process and is never cleared, so it does.
    unsafe {
        field.setDelegate(Some(ProtocolObject::from_ref(controller)));
    }
    if let Some(cell) = field.cell() {
        cell.setSendsActionOnEndEditing(true);
    }
    // SAFETY: setTarget: does not retain, so the target must outlive the field.
    // The `CONTROLLER` thread-local holds the controller for the life of the
    // process. endpointEnded: is implemented above.
    unsafe {
        field.setTarget(Some(controller));
        field.setAction(Some(sel!(endpointEnded:)));
    }
}

fn editable_block(controller: &SettingsController, mtm: MainThreadMarker) -> Retained<NSTextView> {
    let text = NSTextView::initWithFrame(
        NSTextView::alloc(mtm),
        NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(FIELD_WIDTH, 88.0)),
    );
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
    // SAFETY: setTarget: does not retain, so the target must outlive the popup.
    // The `CONTROLLER` thread-local holds the controller for the life of the
    // process. The selector is one the caller took from this class.
    unsafe {
        popup.setTarget(Some(controller));
        popup.setAction(Some(action));
    }
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
    popup.selectItemWithTitle(&NSString::from_str(current));
}

/// Settings sits above the overlay. The overlay is floating; a normal window
/// falls behind it, and tray Settings then looks like a no-op.
fn raise(window: &NSWindow, mtm: MainThreadMarker) {
    let app = NSApplication::sharedApplication(mtm);
    window.setLevel(NSStatusWindowLevel as NSWindowLevel);
    window.setHidesOnDeactivate(false);
    // ponytail: the deprecated forcing call, kept on purpose. Its replacement
    // `activate` is cooperative and documents that it may not activate at all,
    // which is the no-op this function exists to prevent. Swap it when AppKit
    // offers a forcing call that is not deprecated, or when a Settings window
    // that sometimes stays behind the overlay becomes acceptable.
    #[allow(deprecated)]
    app.activateIgnoringOtherApps(true);
    window.orderFrontRegardless();
    window.makeKeyAndOrderFront(None);
}

/// Resize a document view without moving the rows already in it.
///
/// A row's frame is measured from the document's bottom edge, so growing the
/// document would slide the whole form up out of the visible rectangle. Every
/// row moves with the edge instead.
fn size_document(document: &NSView, size: NSSize) {
    let shift = size.height - document.frame().size.height;
    for row in document.subviews() {
        let origin = row.frame().origin;
        row.setFrameOrigin(NSPoint::new(origin.x, origin.y + shift));
    }
    document.setFrameSize(size);
}

fn stretch_x(view: &NSView) {
    view.setTranslatesAutoresizingMaskIntoConstraints(true);
    view.setAutoresizingMask(NSAutoresizingMaskOptions::ViewWidthSizable);
}

fn stretch_xy(view: &NSView) {
    view.setTranslatesAutoresizingMaskIntoConstraints(true);
    view.setAutoresizingMask(
        NSAutoresizingMaskOptions::ViewWidthSizable | NSAutoresizingMaskOptions::ViewHeightSizable,
    );
}

fn pin_right(view: &NSView) {
    view.setTranslatesAutoresizingMaskIntoConstraints(true);
    view.setAutoresizingMask(NSAutoresizingMaskOptions::ViewMinXMargin);
}

/// Apple's default is true. True is the second-open SIGTRAP: we hold a
/// Retained and raise it on the next tray click.
fn release_window_when_closed() -> bool {
    false
}

fn retain_after_close(window: &NSWindow) {
    // SAFETY: `true` here would hand AppKit ownership and dangle the
    // `Retained<NSWindow>` we keep for the next tray click, which is why objc2
    // marks this setter unsafe. `release_window_when_closed` only ever returns
    // false, so the call never transfers ownership.
    unsafe {
        window.setReleasedWhenClosed(release_window_when_closed());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_mtm() -> MainThreadMarker {
        // cargo test is a worker thread. NSObject alloc/init and ivars() do
        // not need a run loop. NSWindow init does, and raises there.
        // SAFETY: the contract is that this thread is the main thread, and
        // under `cargo test` it is not — the claim is knowingly false. It holds
        // because the only APIs these tests reach with the marker are the
        // alloc/init and `ivars()` calls named above, none of which touch
        // AppKit's main-thread state. A test that reached a real main-thread
        // API through this marker would be unsound, so keep them off it.
        unsafe { MainThreadMarker::new_unchecked() }
    }

    /// Opening Settings is `new` then `ivars()`. #205 dropped set_ivars;
    /// that panic was "tried to access uninitialized instance variable".
    #[test]
    fn a_new_controller_can_read_its_ivars() {
        let controller = SettingsController::new(test_mtm());
        assert!(
            controller.ivars().session.borrow().is_none(),
            "ivars must be initialized before build() stores the session"
        );
    }

    /// NSWindow cannot be constructed in `cargo test` (off the main thread
    /// AppKit raises). This is the flag `retain_after_close` writes; leaving
    /// Apple's default is the SIGTRAP on the second tray Settings.
    #[test]
    fn closing_must_keep_the_settings_window() {
        assert!(
            !release_window_when_closed(),
            "the next tray Settings raises this same Retained window"
        );
    }
}
