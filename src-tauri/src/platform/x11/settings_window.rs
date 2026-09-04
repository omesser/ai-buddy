//! Native GTK settings window on Linux.
//!
//! Consumes `settings::form::describe()`, the same data source macOS reads.
//! GTK 3 because Tauri 2's WebKitGTK uses GTK 3.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use gtk::prelude::*;
use gtk::{
    Align, ButtonsType, DialogFlags, MessageDialog, MessageType, ResponseType, Window,
    WindowPosition, WindowType,
};

use crate::settings::form::{self, CompositeControl, FormRow, RowOperation};
use crate::settings::{DirectorDraft, SettingsPatch, SettingsSession, SettingsView};

const WINDOW_WIDTH: i32 = 560;
const WINDOW_HEIGHT: i32 = 720;
const MARGIN: i32 = 28;
/// The gap above a row, and the smaller one above a help line. The ratio
/// between them is the only thing that says which control a help line
/// describes; equal gaps read as a caption for the row below.
const ROW_GAP: i32 = 12;
const HINT_GAP: i32 = 4;
/// A section break: the rule above a heading and the heading's own gap. Larger
/// than `ROW_GAP`, so a heading groups with the rows under it.
const SECTION_GAP: i32 = 24;

thread_local! {
    static WINDOW: RefCell<Option<Rc<SettingsWindow>>> = const { RefCell::new(None) };
}

struct SettingsWindow {
    window: Window,
    session: Arc<Mutex<Option<SettingsSession>>>,
    controls: Rc<RefCell<HashMap<String, Control>>>,
    /// True while `refresh` is driving the widgets, so the handlers a setter
    /// fires do not write what they were just handed back to the file.
    ///
    /// `Rc`, not a bare `Cell`: every handler takes it by `clone`, and cloning
    /// a `Cell` copies the value into a cell nothing else reads. The guard was
    /// dead, and `refresh` drawing the value an exported variable imposes made
    /// that a write of the override into the file (#273).
    refreshing: Rc<Cell<bool>>,
    /// Clear key was clicked and Apply has not run yet. The whole of the
    /// staged delete: the key entry is blank either way, so nothing else
    /// could tell a staged clear from an untouched field (#279).
    clear_pending: Rc<Cell<bool>>,
}

enum Control {
    CheckButton(gtk::CheckButton),
    Entry(gtk::Entry),
    TextView(gtk::TextView),
    Label(gtk::Label),
    List(gtk::Box, String),
    CharacterPicker(gtk::Box, Vec<String>),
    /// A composite row's button, so `refresh` can reach Apply, Cancel and
    /// Clear key by id rather than by walking the widget tree.
    Button(gtk::Button),
}

impl SettingsWindow {
    fn new() -> Rc<Self> {
        let window = Window::new(WindowType::Toplevel);
        window.set_title("ai-buddy");
        window.set_default_size(WINDOW_WIDTH, WINDOW_HEIGHT);
        window.set_position(WindowPosition::Center);
        window.set_deletable(true);

        window.connect_delete_event(|window, _| {
            window.set_keep_above(false);
            window.hide();
            // Nothing staged outlives the tab. The window is only hidden,
            // never destroyed — `show_internal` reuses it — so without this a
            // typed key would sit in the secure entry until the next reopen,
            // one Apply click from the store (#279).
            reset_director_tab();
            gtk::glib::Propagation::Stop
        });

        let this = Rc::new(Self {
            window,
            session: Arc::new(Mutex::new(None)),
            controls: Rc::new(RefCell::new(HashMap::new())),
            refreshing: Rc::new(Cell::new(false)),
            clear_pending: Rc::new(Cell::new(false)),
        });

        this.build_ui();
        this
    }

    /// Build one notebook page per `FormTab`.
    ///
    /// Each page scrolls on its own, so a long tab does not push the tab strip
    /// off screen. `build_row` registers every control in `self.controls` by
    /// id, and `refresh` looks them up there rather than by walking the widget
    /// tree, so a control on a page that is not on top still refreshes.
    fn build_ui(&self) {
        let notebook = gtk::Notebook::new();
        let description = form::describe();

        for tab in &description.tabs {
            let scrolled =
                gtk::ScrolledWindow::new(None::<&gtk::Adjustment>, None::<&gtk::Adjustment>);
            scrolled.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);

            // No box spacing: every row carries its own gap, because a box's
            // spacing is uniform and a help line needs a smaller one.
            let vbox = gtk::Box::new(gtk::Orientation::Vertical, 0);
            vbox.set_margin_start(MARGIN);
            vbox.set_margin_end(MARGIN);
            vbox.set_margin_top(MARGIN);
            vbox.set_margin_bottom(MARGIN);

            let mut drawn = false;
            for section in &tab.sections {
                drawn |= self.build_section(&vbox, section, &description.operations, drawn);
            }

            scrolled.add(&vbox);
            notebook.append_page(&scrolled, Some(&gtk::Label::new(Some(&tab.title))));
        }

        self.window.add(&notebook);
    }

    /// Returns whether the section drew anything, so the caller knows whether
    /// the next one is still the first on its page.
    fn build_section(
        &self,
        container: &gtk::Box,
        section: &form::FormSection,
        operations: &HashMap<String, RowOperation>,
        rule: bool,
    ) -> bool {
        let visible_rows: Vec<&FormRow> = section
            .rows
            .iter()
            .filter(|row| !self.should_omit_row(row))
            .collect();

        if visible_rows.is_empty() {
            return false;
        }

        // A rule above every heading but a page's first, so the heading reads
        // as the start of the group below it rather than another row in the one
        // above.
        if rule {
            pack(
                container,
                &gtk::Separator::new(gtk::Orientation::Horizontal),
                SECTION_GAP,
            );
        }

        let heading = gtk::Label::new(Some(&section.heading));
        heading.set_halign(Align::Start);
        heading.set_markup(&format!(
            "<span size='large' weight='bold'>{}</span>",
            gtk::glib::markup_escape_text(&section.heading)
        ));
        pack(container, &heading, SECTION_GAP);

        if let Some(comment) = &section.comment {
            let comment_label = gtk::Label::new(Some(comment));
            comment_label.set_halign(Align::Start);
            comment_label.set_line_wrap(true);
            comment_label.set_xalign(0.0);
            comment_label.set_markup(&format!(
                "<span size='small' foreground='#888888'>{}</span>",
                gtk::glib::markup_escape_text(comment)
            ));
            pack(container, &comment_label, HINT_GAP);
        }

        for row in &visible_rows {
            self.build_row(container, row, operations);
        }

        true
    }

    fn should_omit_row(&self, row: &FormRow) -> bool {
        match row {
            FormRow::Checkbox { id, .. } => {
                id == form::CONSENT_ACCESSIBILITY_ID || id == form::CONSENT_SCREEN_RECORDING_ID
            }
            _ => false,
        }
    }

    fn build_row(
        &self,
        container: &gtk::Box,
        row: &FormRow,
        operations: &HashMap<String, RowOperation>,
    ) {
        match row {
            FormRow::Checkbox {
                id,
                label,
                writes,
                frozen,
                help,
                comment: _,
            } => {
                if id == form::CONSENT_ACCESSIBILITY_ID || id == form::CONSENT_SCREEN_RECORDING_ID {
                    return;
                }

                let check = gtk::CheckButton::with_label(label);
                check.set_sensitive(!frozen);

                if let Some(help_text) = help {
                    pack(container, &check, ROW_GAP);
                    help_line(container, help_text);
                } else {
                    pack(container, &check, ROW_GAP);
                }

                // Frozen like the field arms below, so refresh's `set_active`
                // has nothing to fire into. `set_sensitive(false)` stops a
                // click; only an absent handler stops a programmatic set.
                if !frozen {
                    let writes = *writes;
                    let session = Arc::clone(&self.session);
                    let refreshing = self.refreshing.clone();
                    check.connect_toggled(move |check| {
                        if refreshing.get() {
                            return;
                        }
                        if let Ok(guard) = session.lock() {
                            if let Some(sess) = guard.as_ref() {
                                let mut patch = SettingsPatch::default();
                                patch.set_bool(writes, check.is_active());
                                if let Err(e) = sess.apply(patch) {
                                    eprintln!("settings: {e}");
                                }
                            }
                        }
                    });
                }

                self.controls
                    .borrow_mut()
                    .insert(id.clone(), Control::CheckButton(check));
            }
            FormRow::TextField {
                id,
                label,
                placeholder,
                writes,
                frozen,
                batched,
            } => {
                if let Some(label_text) = label {
                    let label_widget = gtk::Label::new(Some(label_text));
                    label_widget.set_halign(Align::Start);
                    pack(container, &label_widget, ROW_GAP);
                }

                let entry = gtk::Entry::new();
                entry.set_placeholder_text(Some(placeholder));
                entry.set_hexpand(true);
                // Read-only rather than insensitive, so the value stays
                // legible and copyable.
                entry.set_editable(!frozen);

                if *batched {
                    self.bind_batched(&entry);
                } else if !frozen {
                    let writes = *writes;
                    let session = Arc::clone(&self.session);
                    let refreshing = self.refreshing.clone();
                    let entry_clone = entry.clone();

                    let apply_fn = move || {
                        if refreshing.get() {
                            return;
                        }
                        if let Ok(guard) = session.lock() {
                            if let Some(sess) = guard.as_ref() {
                                let text = entry_clone.text().to_string();
                                let mut patch = SettingsPatch::default();
                                if !patch.set_text(writes, &text) {
                                    return;
                                }
                                if let Err(e) = sess.apply(patch) {
                                    eprintln!("settings: {e}");
                                }
                            }
                        }
                    };

                    let apply_fn_activate = apply_fn.clone();
                    entry.connect_activate(move |_| {
                        apply_fn_activate();
                    });

                    entry.connect_focus_out_event(move |_, _| {
                        apply_fn();
                        gtk::glib::Propagation::Proceed
                    });
                }

                pack(container, &entry, ROW_GAP);
                self.controls
                    .borrow_mut()
                    .insert(id.clone(), Control::Entry(entry));
            }
            FormRow::SecureField {
                id,
                label,
                writes,
                frozen,
            } => {
                if let Some(label_text) = label {
                    let label_widget = gtk::Label::new(Some(label_text));
                    label_widget.set_halign(Align::Start);
                    pack(container, &label_widget, ROW_GAP);
                }

                let entry = gtk::Entry::new();
                entry.set_visibility(false);
                entry.set_hexpand(true);
                entry.set_editable(!frozen);

                // Always batched: `FormRow::SecureField` offers no other mode,
                // so there is no blur commit here to skip.
                if !frozen {
                    self.bind_batched(&entry);
                }

                pack(container, &entry, ROW_GAP);
                self.controls
                    .borrow_mut()
                    .insert(id.clone(), Control::Entry(entry));
            }
            FormRow::InspectBlock { id, label, help } => {
                if let Some(label_text) = label {
                    let label_widget = gtk::Label::new(Some(label_text));
                    label_widget.set_halign(Align::Start);
                    pack(container, &label_widget, ROW_GAP);
                }

                if id == form::HOTKEY_ID || id == form::PAYLOAD_ID {
                    let label = gtk::Label::new(None);
                    label.set_halign(Align::Start);
                    label.set_xalign(0.0);
                    label.set_selectable(true);
                    label.set_line_wrap(true);

                    pack(container, &label, ROW_GAP);
                    self.controls
                        .borrow_mut()
                        .insert(id.clone(), Control::Label(label));
                } else {
                    let scrolled = gtk::ScrolledWindow::new(
                        None::<&gtk::Adjustment>,
                        None::<&gtk::Adjustment>,
                    );
                    scrolled.set_policy(gtk::PolicyType::Automatic, gtk::PolicyType::Automatic);
                    scrolled.set_size_request(-1, 88);

                    let text_view = gtk::TextView::new();
                    text_view.set_editable(false);
                    text_view.set_wrap_mode(gtk::WrapMode::Word);
                    text_view.set_monospace(true);

                    scrolled.add(&text_view);
                    pack(container, &scrolled, ROW_GAP);
                    self.controls
                        .borrow_mut()
                        .insert(id.clone(), Control::TextView(text_view));
                }

                if let Some(help_text) = help {
                    help_line(container, help_text);
                }
            }
            FormRow::InspectPath { id } => {
                let label = gtk::Label::new(None);
                label.set_halign(Align::Start);
                label.set_line_wrap(true);
                label.set_xalign(0.0);
                label.set_selectable(true);

                pack(container, &label, ROW_GAP);
                self.controls
                    .borrow_mut()
                    .insert(id.clone(), Control::Label(label));
            }
            FormRow::List {
                id,
                dismiss_label,
                help,
            } => {
                let list_box = gtk::Box::new(gtk::Orientation::Vertical, 4);
                list_box.set_size_request(-1, 80);

                pack(container, &list_box, ROW_GAP);
                self.controls
                    .borrow_mut()
                    .insert(id.clone(), Control::List(list_box, dismiss_label.clone()));

                if let Some(help_text) = help {
                    help_line(container, help_text);
                }
            }
            FormRow::Multiline {
                id,
                writes,
                help,
                editable,
                ..
            } => {
                let scrolled =
                    gtk::ScrolledWindow::new(None::<&gtk::Adjustment>, None::<&gtk::Adjustment>);
                scrolled.set_policy(gtk::PolicyType::Automatic, gtk::PolicyType::Automatic);
                scrolled.set_size_request(-1, 88);

                let text_view = gtk::TextView::new();
                text_view.set_editable(*editable);
                text_view.set_wrap_mode(gtk::WrapMode::Word);
                text_view.set_monospace(true);

                if *editable {
                    let writes = *writes;
                    let buffer = text_view.buffer().expect("text buffer");
                    let session = Arc::clone(&self.session);
                    let refreshing = self.refreshing.clone();
                    buffer.connect_changed(move |buffer| {
                        if refreshing.get() {
                            return;
                        }
                        if let Ok(guard) = session.lock() {
                            if let Some(sess) = guard.as_ref() {
                                let text = buffer
                                    .text(&buffer.start_iter(), &buffer.end_iter(), false)
                                    .map(|s| s.to_string())
                                    .unwrap_or_default();
                                let mut patch = SettingsPatch::default();
                                if !patch.set_text(writes, &text) {
                                    return;
                                }
                                if let Err(e) = sess.apply(patch) {
                                    eprintln!("settings: {e}");
                                }
                            }
                        }
                    });
                }

                scrolled.add(&text_view);
                pack(container, &scrolled, ROW_GAP);
                self.controls
                    .borrow_mut()
                    .insert(id.clone(), Control::TextView(text_view));

                if let Some(help_text) = help {
                    help_line(container, help_text);
                }
            }
            FormRow::Composite { controls, help, .. } => {
                let hbox = gtk::Box::new(gtk::Orientation::Horizontal, 8);

                for control in controls {
                    match control {
                        CompositeControl::TextField { id, placeholder } => {
                            let entry = gtk::Entry::new();
                            entry.set_placeholder_text(Some(placeholder));
                            entry.set_width_request(200);

                            hbox.pack_start(&entry, false, false, 0);
                            self.controls
                                .borrow_mut()
                                .insert(id.clone(), Control::Entry(entry));
                        }
                        CompositeControl::Popup { id } => {
                            let radio_box = gtk::Box::new(gtk::Orientation::Vertical, 2);
                            radio_box.set_size_request(180, -1);

                            hbox.pack_start(&radio_box, false, false, 0);
                            self.controls.borrow_mut().insert(
                                id.clone(),
                                Control::CharacterPicker(radio_box, Vec::new()),
                            );
                        }
                        CompositeControl::Button { id, label, frozen } => {
                            let button = gtk::Button::with_label(label);
                            button.set_sensitive(!frozen);

                            if let Some(op) = operations.get(id) {
                                let op = op.clone();
                                let session = Arc::clone(&self.session);
                                let window_weak = self.window.downgrade();
                                let director = matches!(
                                    op,
                                    RowOperation::ClearKey
                                        | RowOperation::Apply
                                        | RowOperation::Cancel
                                )
                                .then(|| op.clone());

                                if id == form::SPAWN_ID {
                                    let new_name_id = form::NEW_NAME_ID.to_string();
                                    let new_char_id = form::NEW_CHARACTER_ID.to_string();
                                    let controls = self.controls.clone();

                                    button.connect_clicked(move |_| {
                                        if let Ok(guard) = session.lock() {
                                            if let Some(sess) = guard.as_ref() {
                                                if matches!(&op, RowOperation::Spawn) {
                                                    let ctrl = controls.borrow();
                                                    let name = ctrl
                                                        .get(&new_name_id)
                                                        .and_then(|c| {
                                                            if let Control::Entry(e) = c {
                                                                Some(e.text().to_string())
                                                            } else {
                                                                None
                                                            }
                                                        })
                                                        .unwrap_or_default()
                                                        .trim()
                                                        .to_string();
                                                    let character = ctrl
                                                        .get(&new_char_id)
                                                        .and_then(|c| {
                                                            if let Control::CharacterPicker(radio_box, _) = c {
                                                                radio_box.children().into_iter().find_map(|child| {
                                                                    child.downcast::<gtk::RadioButton>().ok().and_then(|radio| {
                                                                        if radio.is_active() {
                                                                            Some(radio.label().unwrap().to_string())
                                                                        } else {
                                                                            None
                                                                        }
                                                                    })
                                                                })
                                                            } else {
                                                                None
                                                            }
                                                        })
                                                        .unwrap_or_default();

                                                    if !name.is_empty() && !character.is_empty()
                                                    {
                                                        sess.spawn(character, name);
                                                        if let Some(Control::Entry(e)) =
                                                            ctrl.get(&new_name_id)
                                                        {
                                                            e.set_text("");
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    });
                                } else if let Some(op) = director {
                                    // The Director tab's three writers. None
                                    // of them may hold the session lock while
                                    // the window redraws: `refresh` locks the
                                    // same non-reentrant mutex (#279).
                                    let controls = self.controls.clone();
                                    let clear_pending = self.clear_pending.clone();

                                    button.connect_clicked(move |_| {
                                        let Some(view) = view_of(&session) else {
                                            return;
                                        };
                                        match &op {
                                            RowOperation::Apply => {
                                                let description = form::describe();
                                                let patch = director_draft(
                                                    &controls.borrow(),
                                                    clear_pending.get(),
                                                    &description,
                                                )
                                                .patch(&view);
                                                // Resets only once the write
                                                // landed. A locked Keychain
                                                // fails before the file is
                                                // touched, and discarding the
                                                // typed endpoint would lose an
                                                // edit nothing saved (#279).
                                                if let Some(patch) = patch {
                                                    let result = match session.lock() {
                                                        Ok(guard) => guard
                                                            .as_ref()
                                                            .map(|sess| sess.apply(patch)),
                                                        Err(_) => None,
                                                    };
                                                    match result {
                                                        Some(Err(e)) => {
                                                            eprintln!("settings: {e}");
                                                            return;
                                                        }
                                                        None => return,
                                                        Some(Ok(())) => {}
                                                    }
                                                }
                                                // Resets even though the store
                                                // now holds what the key entry
                                                // still shows: only a reset
                                                // takes the typed key back out.
                                                reset_director_tab();
                                            }
                                            // Writes neither the file nor the
                                            // store: the reset draws every
                                            // field from live state, and the
                                            // key entry it blanks is the only
                                            // place a typed key ever was.
                                            RowOperation::Cancel => {
                                                reset_director_tab();
                                            }
                                            // Staged, and no redraw: a
                                            // `refresh` here would take a URL
                                            // typed beside it back to the
                                            // file's value.
                                            RowOperation::ClearKey => {
                                                clear_pending.set(true);
                                                let key = match controls
                                                    .borrow()
                                                    .get(form::DIRECTOR_API_KEY_ID)
                                                {
                                                    Some(Control::Entry(entry)) => {
                                                        Some(entry.clone())
                                                    }
                                                    _ => None,
                                                };
                                                if let Some(entry) = key {
                                                    entry.set_text("");
                                                }
                                                update_director_buttons(
                                                    &controls.borrow(),
                                                    &view,
                                                    clear_pending.get(),
                                                );
                                            }
                                            _ => {}
                                        }
                                    });
                                } else {
                                    button.connect_clicked(move |_| {
                                        if let Ok(guard) = session.lock() {
                                            if let Some(sess) = guard.as_ref() {
                                                match &op {
                                                    RowOperation::OpenMemory => {
                                                        if let Err(e) = sess.open_memory() {
                                                            eprintln!("settings: {e}");
                                                        }
                                                    }
                                                    RowOperation::WipeMemory => {
                                                        if let Some(window) = window_weak.upgrade()
                                                        {
                                                            if confirm_wipe(&window) {
                                                                if let Err(e) = sess.wipe_memory() {
                                                                    eprintln!("settings: {e}");
                                                                }
                                                            }
                                                        }
                                                    }
                                                    RowOperation::ClearKey => {
                                                        let patch = SettingsPatch {
                                                            director_api_key: Some(String::new()),
                                                            ..SettingsPatch::default()
                                                        };
                                                        if let Err(e) = sess.apply(patch) {
                                                            eprintln!("settings: {e}");
                                                        }
                                                    }
                                                    _ => {}
                                                }
                                            }
                                        }
                                    });
                                }
                            }

                            hbox.pack_start(&button, false, false, 0);
                            self.controls
                                .borrow_mut()
                                .insert(id.clone(), Control::Button(button));
                        }
                    }
                }

                pack(container, &hbox, ROW_GAP);

                if let Some(help_text) = help {
                    help_line(container, help_text);
                }
            }
            FormRow::Popup { id, help, .. } => {
                let radio_box = gtk::Box::new(gtk::Orientation::Vertical, 2);

                pack(container, &radio_box, ROW_GAP);
                self.controls
                    .borrow_mut()
                    .insert(id.clone(), Control::CharacterPicker(radio_box, Vec::new()));

                if let Some(help_text) = help {
                    help_line(container, help_text);
                }
            }
        }
    }

    /// A batched field gets no commit binding: the tab commits on Apply, and
    /// `connect_changed` only keeps Apply and Cancel in step with what the
    /// field holds (#279).
    fn bind_batched(&self, entry: &gtk::Entry) {
        let session = Arc::clone(&self.session);
        let refreshing = self.refreshing.clone();
        let controls = self.controls.clone();
        let clear_pending = self.clear_pending.clone();
        entry.connect_changed(move |_| {
            if refreshing.get() {
                return;
            }
            let Some(view) = view_of(&session) else {
                return;
            };
            update_director_buttons(&controls.borrow(), &view, clear_pending.get());
        });
    }

    fn show(&self) {
        self.window.set_keep_above(true);
        self.window.show_all();
        self.window.present();
    }

    fn set_session(&self, session: SettingsSession) {
        *self.session.lock().unwrap() = Some(session);
        self.refresh();
    }

    /// Redraw from live state, leaving anything staged on the Director tab
    /// alone.
    ///
    /// Every caller but Apply and Cancel arrives unasked: the frame loop
    /// refreshes after any `SettingsOp`. Overwriting a half-typed endpoint on
    /// one of those would make batching lossy in ordinary use, so a dirty tab
    /// is the one thing a redraw does not touch (#279).
    fn refresh(&self) {
        self.draw(false);
    }

    /// `reset_director` is Apply and Cancel: the two callers that mean to take
    /// the staged fields back to live state.
    fn draw(&self, reset_director: bool) {
        // Snapshot session.view() while holding the lock, then drop the guard
        // before any GTK setter. Widget setters fire toggled/changed/activate
        // synchronously, and those handlers lock the same non-reentrant mutex.
        let view = {
            let guard = self.session.lock().unwrap();
            let Some(session) = guard.as_ref() else {
                return;
            };
            session.view()
        };

        self.refreshing.set(true);

        let mut controls = self.controls.borrow_mut();
        // Read before any setter, so this is what the entries held on entry.
        let staged = if reset_director {
            // The staged delete is part of what a reset drops: left set, it
            // would outlive the reset and re-arm Apply on the next redraw.
            self.clear_pending.set(false);
            crate::settings::Staged::default()
        } else {
            let description = form::describe();
            director_draft(&controls, self.clear_pending.get(), &description).staged(&view)
        };

        if let Some(Control::CheckButton(check)) = controls.get(form::DIRECTOR_ID) {
            check.set_active(view.director_enabled);
        }
        if let Some(Control::CheckButton(check)) = controls.get(form::AMBIENT_ID) {
            check.set_active(view.ambient_wakes);
        }
        if let Some(Control::CheckButton(check)) = controls.get(form::DND_ID) {
            check.set_active(view.do_not_disturb);
        }
        if let Some(Control::CheckButton(check)) = controls.get(form::SOUND_ID) {
            check.set_active(view.sound);
        }
        if let Some(Control::CheckButton(check)) = controls.get(form::HIDDEN_ID) {
            check.set_active(view.hidden);
        }
        if let Some(Control::CheckButton(check)) = controls.get(form::FULLSCREEN_ID) {
            check.set_active(view.hide_in_fullscreen);
        }
        if let Some(Control::Entry(entry)) = controls.get(form::DIRECTOR_API_KEY_ID) {
            // The placeholder names the stored key, not the typed one, so it
            // is safe to redraw over a staged edit.
            entry.set_placeholder_text(Some(&view.api_key_placeholder()));
        }
        if !staged.base_url {
            if let Some(Control::Entry(entry)) = controls.get(form::DIRECTOR_BASE_URL_ID) {
                entry.set_text(&view.director_base_url);
            }
        }
        if !staged.model {
            if let Some(Control::Entry(entry)) = controls.get(form::DIRECTOR_MODEL_ID) {
                entry.set_text(&view.director_model);
            }
        }
        if !staged.key {
            if let Some(Control::Entry(entry)) = controls.get(form::DIRECTOR_API_KEY_ID) {
                entry.set_text("");
            }
        }
        if let Some(Control::Label(label)) = controls.get(form::MEMORY_PATH_ID) {
            label.set_text(&view.memory_path);
        }
        if let Some(Control::Label(label)) = controls.get(form::PAYLOAD_ID) {
            label.set_text(view.last_payload.as_deref().unwrap_or("Nothing sent yet."));
        }
        if let Some(Control::TextView(text_view)) = controls.get(form::EXCLUDED_ID) {
            if let Some(buffer) = text_view.buffer() {
                buffer.set_text(&view.excluded_text());
            }
        }
        if let Some(Control::Label(label)) = controls.get(form::HOTKEY_ID) {
            label.set_text(&view.hide_hotkey);
        }
        // The rows the view carries by id: every Development switch and limit.
        // Bound here rather than one named lookup each, so a row added to
        // `form.rs` is drawn from the value in force with no edit to this file.
        for (id, on) in &view.development_switches {
            if let Some(Control::CheckButton(check)) = controls.get(id) {
                check.set_active(*on);
            }
        }
        for (id, text) in &view.development_texts {
            if let Some(Control::Entry(entry)) = controls.get(id) {
                entry.set_text(text);
            }
        }
        // The picker's own row declares the field, so the radio buttons built
        // here and the row cannot disagree about what a pick writes.
        let character_field = form::describe().text_write(form::CHARACTER_ID);
        if let Some(Control::CharacterPicker(radio_box, cached_installed)) =
            controls.get_mut(form::CHARACTER_ID)
        {
            if cached_installed != &view.installed {
                for child in radio_box.children() {
                    radio_box.remove(&child);
                }

                let mut group: Option<gtk::RadioButton> = None;
                for name in &view.installed {
                    let radio = if let Some(ref first) = group {
                        gtk::RadioButton::from_widget(first)
                    } else {
                        gtk::RadioButton::with_label(name)
                    };
                    if group.is_none() {
                        group = Some(radio.clone());
                    } else {
                        radio.set_label(name);
                    }

                    if name == &view.character {
                        radio.set_active(true);
                    }

                    let session = Arc::clone(&self.session);
                    let refreshing = self.refreshing.clone();
                    let character = name.clone();
                    radio.connect_toggled(move |radio| {
                        if refreshing.get() {
                            return;
                        }
                        if radio.is_active() {
                            let Some(writes) = character_field else {
                                return;
                            };
                            if let Ok(guard) = session.lock() {
                                if let Some(sess) = guard.as_ref() {
                                    let mut patch = SettingsPatch::default();
                                    if !patch.set_text(writes, &character) {
                                        return;
                                    }
                                    if let Err(e) = sess.apply(patch) {
                                        eprintln!("settings: {e}");
                                    }
                                }
                            }
                        }
                    });

                    radio_box.pack_start(&radio, false, false, 0);
                }

                radio_box.show_all();
                *cached_installed = view.installed.clone();
            } else {
                for child in radio_box.children() {
                    if let Ok(radio) = child.downcast::<gtk::RadioButton>() {
                        if let Some(label) = radio.label() {
                            if label == view.character {
                                radio.set_active(true);
                            }
                        }
                    }
                }
            }
        }
        if let Some(Control::CharacterPicker(radio_box, cached_installed)) =
            controls.get_mut(form::NEW_CHARACTER_ID)
        {
            let current_selection = radio_box.children().into_iter().find_map(|child| {
                child.downcast::<gtk::RadioButton>().ok().and_then(|radio| {
                    if radio.is_active() {
                        radio.label().map(|s| s.to_string())
                    } else {
                        None
                    }
                })
            });

            if cached_installed != &view.installed {
                for child in radio_box.children() {
                    radio_box.remove(&child);
                }

                let mut group: Option<gtk::RadioButton> = None;
                for name in &view.installed {
                    let radio = if let Some(ref first) = group {
                        gtk::RadioButton::from_widget(first)
                    } else {
                        gtk::RadioButton::with_label(name)
                    };
                    if group.is_none() {
                        group = Some(radio.clone());
                    } else {
                        radio.set_label(name);
                    }

                    if let Some(ref selected) = current_selection {
                        if name == selected {
                            radio.set_active(true);
                        }
                    } else if name == &view.character {
                        radio.set_active(true);
                    }

                    radio_box.pack_start(&radio, false, false, 0);
                }

                radio_box.show_all();
                *cached_installed = view.installed.clone();
            }
        }
        if let Some(Control::List(list_box, dismiss_label)) = controls.get(form::INSTANCES_ID) {
            for child in list_box.children() {
                list_box.remove(&child);
            }

            for (index, line) in view.instance_lines().iter().enumerate() {
                let hbox = gtk::Box::new(gtk::Orientation::Horizontal, 8);

                let label = gtk::Label::new(Some(line));
                label.set_halign(Align::Start);
                label.set_hexpand(true);

                let dismiss_button = gtk::Button::with_label(dismiss_label);
                let session = Arc::clone(&self.session);
                let instance_id = view.instances.get(index).map(|i| i.id.clone());
                dismiss_button.connect_clicked(move |_| {
                    if let Some(id) = &instance_id {
                        if let Ok(guard) = session.lock() {
                            if let Some(sess) = guard.as_ref() {
                                sess.dismiss(id.clone());
                            }
                        }
                    }
                });

                hbox.pack_start(&label, true, true, 0);
                hbox.pack_start(&dismiss_button, false, false, 0);
                hbox.show_all();

                list_box.pack_start(&hbox, false, false, 0);
            }
        }

        update_director_buttons(&controls, &view, self.clear_pending.get());

        self.refreshing.set(false);
    }
}

/// The live view, with the lock already released.
///
/// Every caller runs GTK setters next, and a setter fires its handlers
/// synchronously against this same non-reentrant mutex.
fn view_of(session: &Arc<Mutex<Option<SettingsSession>>>) -> Option<SettingsView> {
    let guard = session.lock().ok()?;
    guard.as_ref().map(|session| session.view())
}

/// The Director tab as the window holds it right now.
///
/// The fields are read back out of the widgets rather than mirrored in a
/// draft: the typed key lives in the `gtk::Entry` and nowhere else, so it
/// cannot reach the settings file, and closing the window resets it (#279).
fn director_draft<'a>(
    controls: &HashMap<String, Control>,
    clear_pending: bool,
    description: &'a form::FormDescription,
) -> DirectorDraft<'a> {
    let text = |id: &str| match controls.get(id) {
        Some(Control::Entry(entry)) => entry.text().to_string(),
        _ => String::new(),
    };
    DirectorDraft {
        base_url: text(form::DIRECTOR_BASE_URL_ID),
        model: text(form::DIRECTOR_MODEL_ID),
        key: text(form::DIRECTOR_API_KEY_ID),
        clear_key: clear_pending,
        description,
    }
}

/// Both buttons say whether there is anything to apply.
///
/// Takes the view rather than the session: `draw` calls this with the guard
/// dropped, so locking here would be the deadlock that discipline avoids.
fn update_director_buttons(
    controls: &HashMap<String, Control>,
    view: &SettingsView,
    clear_pending: bool,
) {
    let description = form::describe();
    let dirty = director_draft(controls, clear_pending, &description)
        .patch(view)
        .is_some();
    for id in [form::APPLY_ID, form::CANCEL_ID] {
        if let Some(Control::Button(button)) = controls.get(id) {
            button.set_sensitive(dirty);
        }
    }
}

/// Add a widget to a page with `gap` of space above it.
/// A row's help line, packed close under the control it describes.
fn help_line(container: &gtk::Box, text: &str) {
    let label = gtk::Label::new(Some(text));
    label.set_halign(Align::Start);
    label.set_line_wrap(true);
    label.set_xalign(0.0);
    label.set_margin_start(24);
    label.set_markup(&format!(
        "<span size='small' foreground='#888888'>{}</span>",
        gtk::glib::markup_escape_text(text)
    ));
    pack(container, &label, HINT_GAP);
}

fn pack(container: &gtk::Box, widget: &impl gtk::glib::IsA<gtk::Widget>, gap: i32) {
    widget.set_margin_top(gap);
    container.pack_start(widget, false, false, 0);
}

fn confirm_wipe(parent: &Window) -> bool {
    let dialog = MessageDialog::new(
        Some(parent),
        DialogFlags::MODAL,
        MessageType::Question,
        ButtonsType::None,
        "Wipe Memory?",
    );
    dialog.set_secondary_text(Some("A backup is kept beside the file."));
    dialog.add_button("Cancel", ResponseType::Cancel);
    dialog.add_button("Wipe", ResponseType::Accept);

    let response = dialog.run();
    dialog.close();

    response == ResponseType::Accept
}

pub fn show(session: SettingsSession) {
    // Tauri already initialized GTK and owns the main loop. Calling gtk::init()
    // from the running main loop deadlocks. Only mark gtk-rs initialized.
    unsafe {
        gtk::set_initialized();
    }
    show_internal(session);
}

fn show_internal(session: SettingsSession) {
    WINDOW.with(|cell| {
        let mut borrow = cell.borrow_mut();
        if let Some(existing) = borrow.as_ref() {
            existing.set_session(session);
            existing.show();
        } else {
            let window = SettingsWindow::new();
            window.set_session(session);
            window.show();
            *borrow = Some(window);
        }
    });
}

pub fn refresh_if_showing() {
    WINDOW.with(|cell| {
        if let Some(window) = cell.borrow().as_ref() {
            if window.window.is_visible() {
                window.refresh();
            }
        }
    });
}

/// Apply, Cancel and close: the redraw that does take the staged Director
/// fields back to live state.
fn reset_director_tab() {
    WINDOW.with(|cell| {
        if let Some(window) = cell.borrow().as_ref() {
            window.draw(true);
        }
    });
}
