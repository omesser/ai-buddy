//! Native Win32 settings window.
//!
//! Consumes `settings::form::describe()`, the same data source macOS and Linux
//! read. Win32 because it ships with every Windows install and needs no
//! additional runtime.

use std::cell::RefCell;
use std::collections::HashMap;
use std::ffi::CString;
use std::ptr;
use std::sync::{Arc, Mutex};

use windows_sys::core::BOOL;
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{
    ClientToScreen, CreateCompatibleDC, DeleteDC, DrawTextW, EnumDisplayMonitors, GetMonitorInfoA,
    GetStockObject, ScreenToClient, SelectObject, UpdateWindow, DEFAULT_GUI_FONT, DT_CALCRECT,
    DT_WORDBREAK, HGDIOBJ, HMONITOR, MONITORINFO,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleA;
use windows_sys::Win32::UI::Controls::NMHDR;
use windows_sys::Win32::UI::Controls::{BST_CHECKED, BST_UNCHECKED};
use windows_sys::Win32::UI::Controls::{
    TCHITTESTINFO, TCHT_ONITEM, TCIF_TEXT, TCITEMA, TCM_ADJUSTRECT, TCM_GETCURSEL, TCM_HITTEST,
    TCM_INSERTITEMA, WC_TABCONTROLA,
};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_MENU};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    ChildWindowFromPointEx, CreateWindowExA, DefWindowProcA, DestroyWindow, GetClassNameA,
    GetClientRect, GetDlgItem, GetParent, GetWindow, GetWindowLongPtrA, GetWindowTextA,
    GetWindowTextLengthA, MessageBoxA, SendMessageA, SendMessageW, SetWindowLongPtrA, SetWindowPos,
    SetWindowTextA, ShowWindow, BM_GETCHECK, BM_SETCHECK, BS_AUTOCHECKBOX, BS_PUSHBUTTON,
    CWP_SKIPINVISIBLE, CW_USEDEFAULT, EN_CHANGE, ES_AUTOVSCROLL, ES_MULTILINE, ES_PASSWORD,
    ES_READONLY, ES_WANTRETURN, GWLP_USERDATA, GW_CHILD, GW_HWNDNEXT, HTCAPTION, HTCLIENT, IDYES,
    MB_ICONQUESTION, MB_OK, MB_YESNO, SWP_NOZORDER, SW_HIDE, SW_SHOW, WM_CLOSE, WM_COMMAND,
    WM_CTLCOLORSTATIC, WM_ENABLE, WM_NCHITTEST, WM_NOTIFY, WM_SETFONT, WM_SIZE, WNDCLASSA,
    WS_BORDER, WS_CHILD, WS_DISABLED, WS_EX_CLIENTEDGE, WS_OVERLAPPEDWINDOW, WS_TABSTOP,
    WS_VISIBLE, WS_VSCROLL,
};

use crate::settings::form::{self, FormRow, RowOperation};
use crate::settings::move_drag::{should_begin_move, Hit};
use crate::settings::{DirectorDraft, SettingsPatch, SettingsSession, SettingsView, TextField};

const WINDOW_WIDTH: i32 = 560;
const WINDOW_HEIGHT: i32 = 720;
const MARGIN: i32 = 28;
const ROW_HEIGHT: i32 = 24;
const LABEL_HEIGHT: i32 = 18;
const ROW_GAP: i32 = 12;
const HINT_GAP: i32 = 4;
const SECTION_GAP: i32 = 24;
const FIELD_WIDTH: i32 = WINDOW_WIDTH - MARGIN * 2;
const MULTILINE_HEIGHT: i32 = 120;
const INSPECT_BLOCK_HEIGHT: i32 = 100;

const ID_TAB_CONTROL: i32 = 100;
const ID_BASE: i32 = 2000;
/// STATIC swallows BN_CLICKED; Dismiss is a child of this host. #460
const INSTANCES_LIST_CLASS: &std::ffi::CStr = c"AiBuddySettingsList";
const TCN_SELCHANGE_CODE: u32 = 0xFFFFFDDA_u32.wrapping_sub(1);
const EM_SETCUEBANNER: u32 = 0x1501;
const EM_SETREADONLY: u32 = 0x00CF;
const SS_LEFT: u32 = 0x0;
const CBS_DROPDOWNLIST: u32 = 0x0003;
const CB_ADDSTRING: u32 = 0x0143;
const CB_RESETCONTENT: u32 = 0x014B;
const CB_SETCURSEL: u32 = 0x014E;
const CB_GETCURSEL: u32 = 0x0147;
const CB_GETLBTEXT: u32 = 0x0148;
const CB_GETLBTEXTLEN: u32 = 0x0149;
const CBN_SELCHANGE: u16 = 1;

thread_local! {
    static WINDOW: RefCell<Option<Arc<SettingsWindow>>> = const { RefCell::new(None) };
}

struct SettingsWindow {
    hwnd: HWND,
    session: Mutex<Option<SettingsSession>>,
    controls: RefCell<HashMap<String, Control>>,
    control_id_to_form_id: RefCell<HashMap<i32, String>>,
    clear_pending: RefCell<bool>,
    refreshing: RefCell<bool>,
    current_tab: RefCell<usize>,
    disclosure_expanded: RefCell<HashMap<String, bool>>,
}

#[derive(Clone)]
enum Control {
    Checkbox(HWND, usize),
    Edit(HWND, usize),
    Label(HWND, usize),
    Button(HWND, usize),
    ComboBox(HWND, usize, Vec<String>),
    InstancesList(HWND, usize),
    Disclosure(HWND, HWND, usize), // button, label, tab_index
}

impl SettingsWindow {
    fn new(hwnd: HWND) -> Arc<Self> {
        #[allow(clippy::arc_with_non_send_sync)]
        Arc::new(Self {
            hwnd,
            session: Mutex::new(None),
            controls: RefCell::new(HashMap::new()),
            control_id_to_form_id: RefCell::new(HashMap::new()),
            clear_pending: RefCell::new(false),
            refreshing: RefCell::new(false),
            current_tab: RefCell::new(0),
            disclosure_expanded: RefCell::new(HashMap::new()),
        })
    }

    /// `fresh` is a window built this instant: its controls are still empty,
    /// so nothing on the Director tab is staged and the first draw has to fill
    /// every row. Read back as staged instead, they leave the tab showing
    /// placeholders and arm Apply over a patch of empty strings (#530). A
    /// window that was already open keeps whatever the user left staged.
    fn set_session(&self, session: SettingsSession, fresh: bool) {
        *self.session.lock().unwrap() = Some(session);
        self.draw(fresh);
    }

    fn refresh(&self) {
        self.draw(false);
    }

    /// Re-apply enabled/frozen state from the form description.
    ///
    /// Called on every draw/refresh so runtime changes (e.g. switching AI
    /// source from Harness → Model API) unfreeze rows immediately (#593).
    fn apply_enabled_states(&self, description: &form::FormDescription) {
        let controls = self.controls.borrow();

        unsafe {
            for (id, control) in controls.iter() {
                let frozen = row_frozen(description, id);
                if let Some(frozen) = frozen {
                    match control {
                        Control::Edit(hwnd, _) => {
                            // Use EM_SETREADONLY for Edit controls so freeze/unfreeze
                            // matches AppKit setEditable / GTK set_editable.
                            // EM_SETREADONLY: wParam = TRUE for read-only, FALSE for editable.
                            SendMessageA(*hwnd, EM_SETREADONLY, if frozen { 1 } else { 0 }, 0);
                        }
                        Control::Checkbox(hwnd, _)
                        | Control::Button(hwnd, _)
                        | Control::ComboBox(hwnd, _, _) => {
                            // For other control types, use WM_ENABLE.
                            // WM_ENABLE: wParam = TRUE to enable, FALSE to disable.
                            SendMessageA(*hwnd, WM_ENABLE, if frozen { 0 } else { 1 }, 0);
                        }
                        _ => continue,
                    }
                }
            }
        }
    }

    fn draw(&self, reset_director: bool) {
        let view = {
            let guard = self.session.lock().unwrap();
            let Some(session) = guard.as_ref() else {
                return;
            };
            session.view()
        };

        *self.refreshing.borrow_mut() = true;

        // Re-apply enabled/frozen state from the fresh form description so
        // runtime source switches unfreeze rows immediately (#593).
        let description = form::describe();
        self.apply_enabled_states(&description);

        let staged = if reset_director {
            *self.clear_pending.borrow_mut() = false;
            crate::settings::Staged::default()
        } else {
            self.director_staged(&view)
        };

        let controls = self.controls.borrow();

        unsafe {
            for (id, control) in controls.iter() {
                match control {
                    Control::Checkbox(hwnd, _) => {
                        let checked = match id.as_str() {
                            form::DIRECTOR_ID => view.director_enabled,
                            form::AMBIENT_ID => view.ambient_wakes,
                            form::DND_ID => view.do_not_disturb,
                            form::SOUND_ID => view.sound,
                            form::HIDDEN_ID => view.hidden,
                            form::FULLSCREEN_ID => view.hide_in_fullscreen,
                            _ => view.development_switches.get(id).copied().unwrap_or(false),
                        };
                        SendMessageA(
                            *hwnd,
                            BM_SETCHECK,
                            if checked { BST_CHECKED } else { BST_UNCHECKED } as WPARAM,
                            0,
                        );
                    }
                    Control::Edit(hwnd, _) => {
                        let text = match id.as_str() {
                            form::DIRECTOR_BASE_URL_ID if !staged.base_url => {
                                view.director_base_url.clone()
                            }
                            form::DIRECTOR_MODEL_ID if !staged.model => view.director_model.clone(),
                            form::DIRECTOR_API_KEY_ID if !staged.key => String::new(),
                            form::EXCLUDED_ID => view.excluded_text(),
                            _ => {
                                if let Some(value) = view.development_texts.get(id) {
                                    value.clone()
                                } else {
                                    String::new()
                                }
                            }
                        };
                        set_window_text(*hwnd, &text);
                    }
                    Control::Label(hwnd, _) => {
                        if !should_update_label_text(id) {
                            continue;
                        }
                        let text = match id.as_str() {
                            form::MEMORY_PATH_ID => view.memory_path.clone(),
                            form::HOTKEY_ID => view.hide_hotkey.clone(),
                            form::PAYLOAD_ID => view
                                .last_payload
                                .clone()
                                .unwrap_or_else(|| "Nothing sent yet.".to_string()),
                            form::HARNESS_STATE_ID => view.harness_state.clone(),
                            _ => String::new(),
                        };
                        set_window_text(*hwnd, &text);
                    }
                    Control::Button(_, _) => {
                        if id == form::APPLY_ID || id == form::CANCEL_ID {
                            let description = form::describe();
                            let _dirty = self.director_draft(&description).patch(&view).is_some();
                        }
                    }
                    Control::ComboBox(hwnd, _, options) => {
                        // No options of its own leaves the list to the renderer,
                        // which is how the two Character rows get the installed
                        // packages — a list the form cannot see (#468).
                        let choices: &[String] = if options.is_empty() {
                            &view.installed
                        } else {
                            options
                        };
                        let current = view.popup_value(id.as_str()).unwrap_or_default();
                        SendMessageA(*hwnd, CB_RESETCONTENT, 0, 0);
                        for (index, choice) in choices.iter().enumerate() {
                            let title = CString::new(choice.as_str()).unwrap();
                            SendMessageA(*hwnd, CB_ADDSTRING, 0, title.as_ptr() as LPARAM);
                            if choice.as_str() == current {
                                SendMessageA(*hwnd, CB_SETCURSEL, index, 0);
                            }
                        }
                    }
                    Control::InstancesList(hwnd, _) => {
                        if id == form::INSTANCES_ID {
                            self.rebuild_instances_list(*hwnd, &view);
                        }
                    }
                    Control::Disclosure(_, _, _) => {}
                }
            }
        }

        *self.refreshing.borrow_mut() = false;
    }

    fn director_staged(&self, view: &SettingsView) -> crate::settings::Staged {
        let description = form::describe();
        self.director_draft(&description).staged(view)
    }

    fn director_draft<'a>(&self, description: &'a form::FormDescription) -> DirectorDraft<'a> {
        let controls = self.controls.borrow();
        DirectorDraft {
            base_url: get_control_text(&controls, form::DIRECTOR_BASE_URL_ID),
            model: get_control_text(&controls, form::DIRECTOR_MODEL_ID),
            key: get_control_text(&controls, form::DIRECTOR_API_KEY_ID),
            clear_key: *self.clear_pending.borrow(),
            description,
        }
    }

    fn apply(&self, patch: SettingsPatch) -> bool {
        let result = {
            let session = self.session.lock().unwrap();
            let Some(session) = session.as_ref() else {
                return false;
            };
            session.apply(patch)
        };

        match result {
            Ok(()) => true,
            Err(why) => {
                eprintln!("settings: {why}");
                unsafe {
                    let msg = CString::new(format!("Could not save settings: {}", why)).unwrap();
                    MessageBoxA(
                        self.hwnd,
                        msg.as_ptr() as *const u8,
                        c"Error".as_ptr() as *const u8,
                        MB_OK,
                    );
                }
                false
            }
        }
    }

    fn handle_command(&self, control_id: i32, notification: u16) {
        if *self.refreshing.borrow() {
            return;
        }

        if notification == 0 {
            self.handle_button_click(control_id);
        } else if notification == EN_CHANGE as u16 {
            self.handle_text_change(control_id);
        } else if notification == CBN_SELCHANGE {
            self.handle_combobox_change(control_id);
        }
    }

    fn handle_button_click(&self, control_id: i32) {
        if control_id >= ID_BASE + 5000 {
            self.handle_dismiss((control_id - ID_BASE - 5000) as usize);
            return;
        }
        let form_id = self
            .control_id_to_form_id
            .borrow()
            .get(&control_id)
            .cloned();
        if let Some(form_id) = form_id {
            if let Some(Control::Checkbox(..)) = self.controls.borrow().get(&form_id) {
                self.handle_checkbox_toggle(control_id);
                return;
            }
            if let Some(Control::Disclosure(_, label_hwnd, _)) =
                self.controls.borrow().get(&form_id).cloned()
            {
                self.handle_disclosure_toggle(&form_id, label_hwnd);
                return;
            }
        }
        self.handle_operation(control_id);
    }

    fn handle_checkbox_toggle(&self, control_id: i32) {
        let form_id = self
            .control_id_to_form_id
            .borrow()
            .get(&control_id)
            .cloned();
        if let Some(form_id) = form_id {
            let controls = self.controls.borrow();
            if let Some(Control::Checkbox(hwnd, _)) = controls.get(&form_id) {
                let checked =
                    unsafe { SendMessageA(*hwnd, BM_GETCHECK, 0, 0) == BST_CHECKED as isize };

                if let Some(field) = form::describe().bool_write(&form_id) {
                    let mut patch = SettingsPatch::default();
                    patch.set_bool(field, checked);
                    drop(controls);
                    self.apply(patch);
                }
            }
        }
    }

    fn handle_combobox_change(&self, control_id: i32) {
        let form_id = self
            .control_id_to_form_id
            .borrow()
            .get(&control_id)
            .cloned();
        if let Some(form_id) = form_id {
            let controls = self.controls.borrow();
            if let Some(Control::ComboBox(hwnd, _, options)) = controls.get(&form_id) {
                let index = unsafe { SendMessageA(*hwnd, CB_GETCURSEL, 0, 0) };
                if index < 0 {
                    return;
                }
                
                let selected_title = options.get(index as usize).cloned();
                
                if let Some(title) = selected_title {
                    if let Some(field) = form::describe().text_write(&form_id) {
                        let value = match field {
                            TextField::Harness => form::harness_choice(&title),
                            _ => title,
                        };
                        let mut patch = SettingsPatch::default();
                        patch.set_text(field, &value);
                        drop(controls);
                        self.apply(patch);
                    }
                }
            }
        }
    }

    fn handle_disclosure_toggle(&self, form_id: &str, label_hwnd: HWND) {
        let mut expanded = self.disclosure_expanded.borrow_mut();
        let is_expanded = *expanded.get(form_id).unwrap_or(&false);
        let new_state = !is_expanded;
        expanded.insert(form_id.to_string(), new_state);
        drop(expanded);

        unsafe {
            ShowWindow(label_hwnd, if new_state { SW_SHOW } else { SW_HIDE });
        }
    }

    fn handle_operation(&self, control_id: i32) {
        let description = form::describe();
        let form_id = self
            .control_id_to_form_id
            .borrow()
            .get(&control_id)
            .cloned();

        if let Some(form_id) = form_id {
            if let Some(op) = description.operations.get(&form_id) {
                match op {
                    RowOperation::Spawn => self.do_spawn(),
                    RowOperation::OpenMemory => self.do_memory_open(),
                    RowOperation::WipeMemory => self.do_memory_wipe(),
                    RowOperation::ClearKey => self.do_clear_key(),
                    RowOperation::Apply => self.do_apply(),
                    RowOperation::Cancel => self.do_cancel(),
                }
            }
        }
    }

    fn handle_text_change(&self, control_id: i32) {
        let form_id = self
            .control_id_to_form_id
            .borrow()
            .get(&control_id)
            .cloned();

        if let Some(form_id) = form_id {
            if form::DIRECTOR_BASE_URL_ID == form_id
                || form::DIRECTOR_MODEL_ID == form_id
                || form::DIRECTOR_API_KEY_ID == form_id
            {
                let view = {
                    let guard = self.session.lock().unwrap();
                    guard.as_ref().map(|s| s.view())
                };
                if let Some(_view) = view {
                    let controls = self.controls.borrow();
                    for id in [form::APPLY_ID, form::CANCEL_ID] {
                        if let Some(Control::Button(_, _)) = controls.get(id) {
                            let description = form::describe();
                            let _dirty = self.director_draft(&description).patch(&_view).is_some();
                        }
                    }
                }
            } else {
                let controls = self.controls.borrow();
                if let Some(Control::Edit(hwnd, _)) = controls.get(&form_id) {
                    let text = unsafe {
                        let len = GetWindowTextLengthA(*hwnd);
                        if len > 0 {
                            let mut buffer = vec![0u8; (len + 1) as usize];
                            GetWindowTextA(*hwnd, buffer.as_mut_ptr(), len + 1);
                            CString::from_vec_with_nul(buffer)
                                .ok()
                                .and_then(|c| c.into_string().ok())
                                .unwrap_or_default()
                        } else {
                            String::new()
                        }
                    };
                    drop(controls);

                    if let Some(writes) = form::describe().text_write(&form_id) {
                        let mut patch = SettingsPatch::default();
                        if patch.set_text(writes, &text) {
                            self.apply(patch);
                        }
                    }
                }
            }
        }
    }

    fn do_spawn(&self) {
        let controls = self.controls.borrow();
        let name = get_control_text(&controls, form::NEW_NAME_ID)
            .trim()
            .to_string();
        let character = get_control_text(&controls, form::NEW_CHARACTER_ID);

        if !name.is_empty() && !character.is_empty() {
            if let Some(session) = self.session.lock().unwrap().as_ref() {
                session.spawn(character, name);
                if let Some(Control::Edit(hwnd, _)) = controls.get(form::NEW_NAME_ID) {
                    unsafe {
                        SetWindowTextA(*hwnd, c"".as_ptr() as *const u8);
                    }
                }
            }
        }
    }

    fn do_memory_open(&self) {
        if let Some(session) = self.session.lock().unwrap().as_ref() {
            if let Err(why) = session.open_memory() {
                eprintln!("settings: {why}");
            }
        }
    }

    fn do_memory_wipe(&self) {
        unsafe {
            let result = MessageBoxA(
                self.hwnd,
                c"Wipe Memory?\nA backup is kept beside the file.".as_ptr() as *const u8,
                c"Confirm".as_ptr() as *const u8,
                MB_YESNO | MB_ICONQUESTION,
            );
            if result != IDYES {
                return;
            }
        }

        if let Some(session) = self.session.lock().unwrap().as_ref() {
            if let Err(why) = session.wipe_memory() {
                eprintln!("settings: {why}");
            }
        }
    }

    fn do_clear_key(&self) {
        *self.clear_pending.borrow_mut() = true;
        if let Some(Control::Edit(hwnd, _)) = self.controls.borrow().get(form::DIRECTOR_API_KEY_ID)
        {
            unsafe {
                SetWindowTextA(*hwnd, c"".as_ptr() as *const u8);
            }
        }
        let view = self.session.lock().unwrap().as_ref().map(|s| s.view());
        if view.is_some() {
            self.draw(false);
        }
    }

    fn do_apply(&self) {
        let view = {
            let guard = self.session.lock().unwrap();
            guard.as_ref().map(|s| s.view())
        };

        if let Some(view) = view {
            let description = form::describe();
            if let Some(patch) = self.director_draft(&description).patch(&view) {
                if !self.apply(patch) {
                    return;
                }
            }
        }

        self.draw(true);
    }

    fn do_cancel(&self) {
        self.draw(true);
    }

    fn update_tab_visibility(&self) {
        let current_tab = *self.current_tab.borrow();
        let controls = self.controls.borrow();
        let expanded = self.disclosure_expanded.borrow();
        unsafe {
            for (form_id, control) in controls.iter() {
                match control {
                    Control::Checkbox(hwnd, tab_index)
                    | Control::Edit(hwnd, tab_index)
                    | Control::Label(hwnd, tab_index)
                    | Control::Button(hwnd, tab_index)
                    | Control::ComboBox(hwnd, tab_index, _)
                    | Control::InstancesList(hwnd, tab_index) => {
                        ShowWindow(
                            *hwnd,
                            if *tab_index == current_tab {
                                SW_SHOW
                            } else {
                                SW_HIDE
                            },
                        );
                    }
                    Control::Disclosure(button, label, tab_index) => {
                        if *tab_index == current_tab {
                            ShowWindow(*button, SW_SHOW);
                            let is_expanded = expanded.get(form_id).copied().unwrap_or(false);
                            ShowWindow(*label, if is_expanded { SW_SHOW } else { SW_HIDE });
                        } else {
                            ShowWindow(*button, SW_HIDE);
                            ShowWindow(*label, SW_HIDE);
                        }
                    }
                }
            }
        }
    }

    fn rebuild_instances_list(&self, container: HWND, view: &SettingsView) {
        unsafe {
            let mut child = GetWindow(container, GW_CHILD);
            while !child.is_null() {
                let next = GetWindow(child, GW_HWNDNEXT);
                DestroyWindow(child);
                child = next;
            }

            let hfont = GetStockObject(DEFAULT_GUI_FONT) as HGDIOBJ;
            let mut y = 0;
            for (index, instance) in view.instances.iter().enumerate() {
                let line = format!("{} ({})", instance.name, instance.character);
                // Win32 copies lpWindowName; the bind is for clarity, not correctness. #572
                let line_cstr = CString::new(line).unwrap();
                let label = CreateWindowExA(
                    0,
                    c"STATIC".as_ptr() as *const u8,
                    line_cstr.as_ptr() as *const u8,
                    WS_CHILD | WS_VISIBLE | SS_LEFT,
                    0,
                    y,
                    FIELD_WIDTH - 100,
                    ROW_HEIGHT,
                    container,
                    ptr::null_mut(),
                    GetModuleHandleA(ptr::null()),
                    ptr::null_mut(),
                );
                SendMessageA(label, WM_SETFONT, hfont as WPARAM, 1);

                let button = CreateWindowExA(
                    0,
                    c"BUTTON".as_ptr() as *const u8,
                    c"Dismiss".as_ptr() as *const u8,
                    WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_PUSHBUTTON as u32,
                    FIELD_WIDTH - 90,
                    y,
                    80,
                    ROW_HEIGHT,
                    container,
                    (ID_BASE + 5000 + index as i32) as _,
                    GetModuleHandleA(ptr::null()),
                    ptr::null_mut(),
                );
                SendMessageA(button, WM_SETFONT, hfont as WPARAM, 1);

                y += ROW_HEIGHT + ROW_GAP;
            }
        }
    }

    fn handle_dismiss(&self, index: usize) {
        let view = {
            let guard = self.session.lock().unwrap();
            guard.as_ref().map(|s| s.view())
        };
        if let Some(view) = view {
            if let Some(instance) = view.instances.get(index) {
                if let Some(session) = self.session.lock().unwrap().as_ref() {
                    session.dismiss(instance.id.clone());
                }
            }
        }
    }
}

/// Look up a row's frozen state in the form description.
fn row_frozen(description: &form::FormDescription, id: &str) -> Option<bool> {
    description
        .sections()
        .flat_map(|s| &s.rows)
        .find_map(|row| match row {
            form::FormRow::Checkbox {
                id: row_id, frozen, ..
            }
            | form::FormRow::TextField {
                id: row_id, frozen, ..
            }
            | form::FormRow::SecureField {
                id: row_id, frozen, ..
            }
            | form::FormRow::Popup {
                id: row_id, frozen, ..
            } if row_id == id => Some(*frozen),
            form::FormRow::Composite { controls, .. } => {
                controls.iter().find_map(|control| match control {
                    form::CompositeControl::Popup {
                        id: control_id,
                        frozen,
                        ..
                    }
                    | form::CompositeControl::Button {
                        id: control_id,
                        frozen,
                        ..
                    } if control_id == id => Some(*frozen),
                    _ => None,
                })
            }
            _ => None,
        })
}

fn get_control_text(controls: &HashMap<String, Control>, id: &str) -> String {
    unsafe {
        if let Some(Control::Edit(hwnd, _)) = controls.get(id) {
            let len = GetWindowTextLengthA(*hwnd);
            if len == 0 {
                return String::new();
            }
            let mut buffer = vec![0u8; (len + 1) as usize];
            GetWindowTextA(*hwnd, buffer.as_mut_ptr(), len + 1);
            CString::from_vec_with_nul(buffer)
                .ok()
                .and_then(|c| c.into_string().ok())
                .unwrap_or_default()
        } else {
            String::new()
        }
    }
}

fn set_window_text(hwnd: HWND, text: &str) {
    unsafe {
        let text_cstr = CString::new(text).unwrap_or_default();
        SetWindowTextA(hwnd, text_cstr.as_ptr() as *const u8);
    }
}

/// Measure the height text would occupy when wrapped to a given width using
/// the default GUI font.
///
/// Returns the height in pixels required to display `text` wrapped at `width`
/// pixels, using DrawTextW with DT_CALCRECT | DT_WORDBREAK to simulate the
/// wrapping that a STATIC control will perform.
fn measure_wrapped_text_height(text: &str, width: i32) -> i32 {
    unsafe {
        let hdc = CreateCompatibleDC(ptr::null_mut());
        if hdc.is_null() {
            return LABEL_HEIGHT * 2; // Fallback
        }
        let hfont = GetStockObject(DEFAULT_GUI_FONT) as HGDIOBJ;
        let old_font = SelectObject(hdc, hfont);

        let wide_text: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
        let mut rect = RECT {
            left: 0,
            top: 0,
            right: width,
            bottom: 0,
        };

        DrawTextW(
            hdc,
            wide_text.as_ptr(),
            wide_text.len() as i32 - 1,
            &mut rect,
            DT_CALCRECT | DT_WORDBREAK,
        );

        SelectObject(hdc, old_font);
        DeleteDC(hdc);

        let height = rect.bottom - rect.top;
        height.max(LABEL_HEIGHT)
    }
}

/// Decides whether a label id should have its text updated from the view.
///
/// Returns `true` when the label displays dynamic state (memory path, hotkey,
/// last payload, harness state). Returns `false` when the label holds static
/// text set at build_ui time: field labels (`*_label`), placeholders
/// (`*_placeholder`), help hints (`*_help`, `composite_help_*`), and section
/// heading/comment labels (`section_heading_*`, `section_comment_*`).
fn should_update_label_text(id: &str) -> bool {
    if id == form::MEMORY_PATH_ID
        || id == form::HOTKEY_ID
        || id == form::PAYLOAD_ID
        || id == form::HARNESS_STATE_ID
    {
        return true;
    }
    if id.ends_with("_label") || id.ends_with("_placeholder") {
        return false;
    }
    if id.ends_with("_help")
        || id.ends_with("_status")
        || id.ends_with("_disclosure")
        || id.starts_with("composite_help_")
        || id.starts_with("composite_status_")
        || id.starts_with("composite_disclosure_")
    {
        return false;
    }
    if id.starts_with("section_heading_")
        || id.starts_with("section_comment_")
        || id.starts_with("section_status_")
        || id.starts_with("section_disclosure_")
    {
        return false;
    }
    true
}

/// Build an Edit control's style flags from frozen and password flags.
fn edit_style(frozen: bool, password: bool) -> u32 {
    let mut style = WS_CHILD | WS_VISIBLE | WS_TABSTOP | WS_BORDER;
    if frozen {
        style |= ES_READONLY as u32;
    }
    if password {
        style |= ES_PASSWORD as u32;
    }
    style
}

pub fn show(session: SettingsSession) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{BringWindowToTop, SetForegroundWindow};

    WINDOW.with(|cell| {
        let mut borrow = cell.borrow_mut();
        if let Some(existing) = borrow.as_ref() {
            existing.set_session(session, false);
            unsafe {
                ShowWindow(existing.hwnd, SW_SHOW);
                BringWindowToTop(existing.hwnd);
                SetForegroundWindow(existing.hwnd);
            }
        } else {
            match create_window(session) {
                Ok(window) => {
                    unsafe {
                        ShowWindow(window.hwnd, SW_SHOW);
                        BringWindowToTop(window.hwnd);
                        SetForegroundWindow(window.hwnd);
                    }
                    *borrow = Some(window);
                }
                Err(e) => {
                    eprintln!("settings: failed to create window: {}", e);
                }
            }
        }
    });
}

pub fn refresh_if_showing() {
    WINDOW.with(|cell| {
        if let Some(window) = cell.borrow().as_ref() {
            window.refresh();
        }
    });
}

fn get_secondary_monitor_position() -> Option<(i32, i32)> {
    unsafe {
        struct MonitorData {
            count: u32,
            secondary_rect: Option<RECT>,
        }

        unsafe extern "system" fn enum_proc(
            hmonitor: HMONITOR,
            _hdc: windows_sys::Win32::Graphics::Gdi::HDC,
            _lprect: *mut RECT,
            lparam: LPARAM,
        ) -> BOOL {
            let data = &mut *(lparam as *mut MonitorData);
            let mut info: MONITORINFO = std::mem::zeroed();
            info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;

            if GetMonitorInfoA(hmonitor, &mut info) != 0 {
                data.count += 1;
                if data.count == 2 {
                    data.secondary_rect = Some(info.rcWork);
                    return 0;
                }
            }
            1
        }

        let mut data = MonitorData {
            count: 0,
            secondary_rect: None,
        };

        EnumDisplayMonitors(
            ptr::null_mut(),
            ptr::null(),
            Some(enum_proc),
            &mut data as *mut _ as LPARAM,
        );

        data.secondary_rect
            .map(|rect| (rect.left + 50, rect.top + 50))
    }
}

fn create_window(session: SettingsSession) -> Result<Arc<SettingsWindow>, String> {
    unsafe {
        let class_name = c"AiBuddySettings";
        let hinstance = GetModuleHandleA(ptr::null());

        let wc = WNDCLASSA {
            style: 0,
            lpfnWndProc: Some(window_proc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: hinstance,
            hIcon: ptr::null_mut(),
            hCursor: windows_sys::Win32::UI::WindowsAndMessaging::LoadCursorA(
                ptr::null_mut(),
                32512 as *const u8,
            ),
            hbrBackground: (5 + 1) as _,
            lpszMenuName: ptr::null(),
            lpszClassName: class_name.as_ptr() as *const u8,
        };

        let result = windows_sys::Win32::UI::WindowsAndMessaging::RegisterClassA(&wc);
        if result == 0 {
            let error = windows_sys::Win32::Foundation::GetLastError();
            if error != 1410 {
                return Err(format!("Failed to register window class: {}", error));
            }
        }

        let list_wc = WNDCLASSA {
            style: 0,
            lpfnWndProc: Some(list_host_proc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: hinstance,
            hIcon: ptr::null_mut(),
            hCursor: ptr::null_mut(),
            hbrBackground: (5 + 1) as _,
            lpszMenuName: ptr::null(),
            lpszClassName: INSTANCES_LIST_CLASS.as_ptr() as *const u8,
        };
        let list_result = windows_sys::Win32::UI::WindowsAndMessaging::RegisterClassA(&list_wc);
        if list_result == 0 {
            let error = windows_sys::Win32::Foundation::GetLastError();
            if error != 1410 {
                return Err(format!("Failed to register list host class: {}", error));
            }
        }

        let (x, y) = get_secondary_monitor_position().unwrap_or((CW_USEDEFAULT, CW_USEDEFAULT));

        let hwnd = CreateWindowExA(
            0,
            class_name.as_ptr() as *const u8,
            c"ai-buddy Settings".as_ptr() as *const u8,
            WS_OVERLAPPEDWINDOW,
            x,
            y,
            WINDOW_WIDTH,
            WINDOW_HEIGHT,
            ptr::null_mut(),
            ptr::null_mut(),
            hinstance,
            ptr::null_mut(),
        );

        if hwnd.is_null() {
            return Err("Failed to create window".to_string());
        }

        let window = SettingsWindow::new(hwnd);

        SetWindowLongPtrA(hwnd, GWLP_USERDATA, Arc::as_ptr(&window) as isize);

        build_ui(hwnd, &window)?;

        window.set_session(session, true);

        Ok(window)
    }
}

fn build_ui(parent: HWND, window: &Arc<SettingsWindow>) -> Result<(), String> {
    unsafe {
        let hfont = GetStockObject(DEFAULT_GUI_FONT) as HGDIOBJ;
        let description = form::describe();

        let mut client_rect: RECT = std::mem::zeroed();
        GetClientRect(parent, &mut client_rect);

        let tab = CreateWindowExA(
            0,
            WC_TABCONTROLA,
            ptr::null(),
            WS_CHILD | WS_VISIBLE | WS_TABSTOP,
            MARGIN,
            MARGIN,
            client_rect.right - MARGIN * 2,
            client_rect.bottom - MARGIN * 2,
            parent,
            ID_TAB_CONTROL as _,
            GetModuleHandleA(ptr::null()),
            ptr::null_mut(),
        );

        if tab.is_null() {
            return Err("Failed to create tab control".to_string());
        }

        SendMessageA(tab, WM_SETFONT, hfont as WPARAM, 1);

        for (tab_index, tab_def) in description.tabs.iter().enumerate() {
            let tab_name = CString::new(tab_def.title.as_str()).unwrap();
            let mut tie = TCITEMA {
                mask: TCIF_TEXT,
                dwState: 0,
                dwStateMask: 0,
                pszText: tab_name.as_ptr() as *mut u8,
                cchTextMax: 0,
                iImage: 0,
                lParam: 0,
            };
            SendMessageA(
                tab,
                TCM_INSERTITEMA,
                tab_index,
                &mut tie as *mut _ as LPARAM,
            );
        }

        let mut display_rect = RECT {
            left: MARGIN,
            top: MARGIN,
            right: client_rect.right - MARGIN,
            bottom: client_rect.bottom - MARGIN,
        };
        SendMessageA(
            tab,
            TCM_ADJUSTRECT,
            0,
            &mut display_rect as *mut _ as LPARAM,
        );

        let display_left = display_rect.left + MARGIN;
        let display_top = display_rect.top;

        let mut control_id = ID_BASE;

        for (tab_index, tab_def) in description.tabs.iter().enumerate() {
            let mut y = display_top + MARGIN;

            for (section_index, section) in tab_def.sections.iter().enumerate() {
                y += SECTION_GAP;

                let heading_cstr = CString::new(section.heading.as_str()).unwrap();
                let heading_hwnd = CreateWindowExA(
                    0,
                    c"STATIC".as_ptr() as *const u8,
                    heading_cstr.as_ptr() as *const u8,
                    WS_CHILD | WS_VISIBLE | SS_LEFT,
                    display_left,
                    y,
                    FIELD_WIDTH,
                    LABEL_HEIGHT,
                    parent,
                    ptr::null_mut(),
                    GetModuleHandleA(ptr::null()),
                    ptr::null_mut(),
                );
                SendMessageA(heading_hwnd, WM_SETFONT, hfont as WPARAM, 1);
                window.controls.borrow_mut().insert(
                    format!("section_heading_{}_{}", tab_index, section_index),
                    Control::Label(heading_hwnd, tab_index),
                );
                y += LABEL_HEIGHT + HINT_GAP;

                if let Some(comment_text) = &section.comment {
                    let comment_cstr = CString::new(comment_text.as_str()).unwrap();
                    let comment_hwnd = CreateWindowExA(
                        0,
                        c"STATIC".as_ptr() as *const u8,
                        comment_cstr.as_ptr() as *const u8,
                        WS_CHILD | WS_VISIBLE | SS_LEFT,
                        display_left,
                        y,
                        FIELD_WIDTH,
                        LABEL_HEIGHT * 2,
                        parent,
                        ptr::null_mut(),
                        GetModuleHandleA(ptr::null()),
                        ptr::null_mut(),
                    );
                    SendMessageA(comment_hwnd, WM_SETFONT, hfont as WPARAM, 1);
                    window.controls.borrow_mut().insert(
                        format!("section_comment_{}_{}", tab_index, section_index),
                        Control::Label(comment_hwnd, tab_index),
                    );
                    y += LABEL_HEIGHT * 2 + HINT_GAP;
                }

                if let Some(status_text) = &section.status {
                    let status_cstr = CString::new(status_text.as_str()).unwrap();
                    let status_hwnd = CreateWindowExA(
                        0,
                        c"STATIC".as_ptr() as *const u8,
                        status_cstr.as_ptr() as *const u8,
                        WS_CHILD | WS_VISIBLE | SS_LEFT,
                        display_left,
                        y,
                        FIELD_WIDTH,
                        LABEL_HEIGHT,
                        parent,
                        ptr::null_mut(),
                        GetModuleHandleA(ptr::null()),
                        ptr::null_mut(),
                    );
                    SendMessageA(status_hwnd, WM_SETFONT, hfont as WPARAM, 1);
                    window.controls.borrow_mut().insert(
                        format!("section_status_{}_{}", tab_index, section_index),
                        Control::Label(status_hwnd, tab_index),
                    );
                    y += LABEL_HEIGHT + HINT_GAP;
                }

                if let Some(disclosure_text) = &section.disclosure {
                    let button_cstr = CString::new("What is this?").unwrap();
                    let button_hwnd = CreateWindowExA(
                        0,
                        c"BUTTON".as_ptr() as *const u8,
                        button_cstr.as_ptr() as *const u8,
                        WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_PUSHBUTTON as u32,
                        display_left,
                        y,
                        120,
                        ROW_HEIGHT,
                        parent,
                        control_id as _,
                        GetModuleHandleA(ptr::null()),
                        ptr::null_mut(),
                    );
                    SendMessageA(button_hwnd, WM_SETFONT, hfont as WPARAM, 1);
                    y += ROW_HEIGHT + HINT_GAP;

                    let disclosure_cstr = CString::new(disclosure_text.as_str()).unwrap();
                    let label_height = measure_wrapped_text_height(disclosure_text, FIELD_WIDTH);
                    let label_hwnd = CreateWindowExA(
                        0,
                        c"STATIC".as_ptr() as *const u8,
                        disclosure_cstr.as_ptr() as *const u8,
                        WS_CHILD | SS_LEFT,
                        display_left,
                        y,
                        FIELD_WIDTH,
                        label_height,
                        parent,
                        ptr::null_mut(),
                        GetModuleHandleA(ptr::null()),
                        ptr::null_mut(),
                    );
                    SendMessageA(label_hwnd, WM_SETFONT, hfont as WPARAM, 1);

                    let disclosure_id =
                        format!("section_disclosure_{}_{}", tab_index, section_index);
                    window
                        .control_id_to_form_id
                        .borrow_mut()
                        .insert(control_id, disclosure_id.clone());
                    window.controls.borrow_mut().insert(
                        disclosure_id,
                        Control::Disclosure(button_hwnd, label_hwnd, tab_index),
                    );
                    y += label_height + HINT_GAP;
                    control_id += 1;
                }

                for row in &section.rows {
                    match row {
                        FormRow::Checkbox {
                            id,
                            label,
                            help,
                            status,
                            disclosure,
                            ..
                        } => {
                            let label_cstr = CString::new(label.as_str()).unwrap();
                            let hwnd = CreateWindowExA(
                                0,
                                c"BUTTON".as_ptr() as *const u8,
                                label_cstr.as_ptr() as *const u8,
                                WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_AUTOCHECKBOX as u32,
                                display_left,
                                y,
                                FIELD_WIDTH,
                                ROW_HEIGHT,
                                parent,
                                control_id as _,
                                GetModuleHandleA(ptr::null()),
                                ptr::null_mut(),
                            );
                            SendMessageA(hwnd, WM_SETFONT, hfont as WPARAM, 1);
                            window
                                .control_id_to_form_id
                                .borrow_mut()
                                .insert(control_id, id.clone());
                            window
                                .controls
                                .borrow_mut()
                                .insert(id.clone(), Control::Checkbox(hwnd, tab_index));
                            y += ROW_HEIGHT + ROW_GAP;
                            if let Some(status_text) = status {
                                let status_cstr = CString::new(status_text.as_str()).unwrap();
                                let status_hwnd = CreateWindowExA(
                                    0,
                                    c"STATIC".as_ptr() as *const u8,
                                    status_cstr.as_ptr() as *const u8,
                                    WS_CHILD | WS_VISIBLE | SS_LEFT,
                                    display_left,
                                    y,
                                    FIELD_WIDTH,
                                    LABEL_HEIGHT,
                                    parent,
                                    ptr::null_mut(),
                                    GetModuleHandleA(ptr::null()),
                                    ptr::null_mut(),
                                );
                                SendMessageA(status_hwnd, WM_SETFONT, hfont as WPARAM, 1);
                                window.controls.borrow_mut().insert(
                                    format!("{}_status", id),
                                    Control::Label(status_hwnd, tab_index),
                                );
                                y += LABEL_HEIGHT + HINT_GAP;
                            }
                            if let Some(disclosure_text) = disclosure {
                                let button_cstr = CString::new("What is this?").unwrap();
                                let button_hwnd = CreateWindowExA(
                                    0,
                                    c"BUTTON".as_ptr() as *const u8,
                                    button_cstr.as_ptr() as *const u8,
                                    WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_PUSHBUTTON as u32,
                                    display_left,
                                    y,
                                    120,
                                    ROW_HEIGHT,
                                    parent,
                                    control_id as _,
                                    GetModuleHandleA(ptr::null()),
                                    ptr::null_mut(),
                                );
                                SendMessageA(button_hwnd, WM_SETFONT, hfont as WPARAM, 1);
                                y += ROW_HEIGHT + HINT_GAP;

                                let disclosure_cstr =
                                    CString::new(disclosure_text.as_str()).unwrap();
                                let label_height =
                                    measure_wrapped_text_height(disclosure_text, FIELD_WIDTH);
                                let label_hwnd = CreateWindowExA(
                                    0,
                                    c"STATIC".as_ptr() as *const u8,
                                    disclosure_cstr.as_ptr() as *const u8,
                                    WS_CHILD | SS_LEFT,
                                    display_left,
                                    y,
                                    FIELD_WIDTH,
                                    label_height,
                                    parent,
                                    ptr::null_mut(),
                                    GetModuleHandleA(ptr::null()),
                                    ptr::null_mut(),
                                );
                                SendMessageA(label_hwnd, WM_SETFONT, hfont as WPARAM, 1);

                                let disclosure_id = format!("{}_disclosure", id);
                                window
                                    .control_id_to_form_id
                                    .borrow_mut()
                                    .insert(control_id, disclosure_id.clone());
                                window.controls.borrow_mut().insert(
                                    disclosure_id,
                                    Control::Disclosure(button_hwnd, label_hwnd, tab_index),
                                );
                                y += label_height + HINT_GAP;
                                control_id += 1;
                            }
                            if let Some(help_text) = help {
                                let help_cstr = CString::new(help_text.as_str()).unwrap();
                                let help_hwnd = CreateWindowExA(
                                    0,
                                    c"STATIC".as_ptr() as *const u8,
                                    help_cstr.as_ptr() as *const u8,
                                    WS_CHILD | WS_VISIBLE | SS_LEFT,
                                    display_left,
                                    y,
                                    FIELD_WIDTH,
                                    LABEL_HEIGHT,
                                    parent,
                                    ptr::null_mut(),
                                    GetModuleHandleA(ptr::null()),
                                    ptr::null_mut(),
                                );
                                SendMessageA(help_hwnd, WM_SETFONT, hfont as WPARAM, 1);
                                window.controls.borrow_mut().insert(
                                    format!("{}_help", id),
                                    Control::Label(help_hwnd, tab_index),
                                );
                                y += LABEL_HEIGHT + HINT_GAP;
                            }
                            control_id += 1;
                        }
                        FormRow::TextField {
                            id,
                            label,
                            placeholder,
                            frozen,
                            help,
                            status,
                            disclosure,
                            ..
                        } => {
                            if let Some(label_text) = label {
                                let label_hwnd = CreateWindowExA(
                                    0,
                                    c"STATIC".as_ptr() as *const u8,
                                    ptr::null(),
                                    WS_CHILD | WS_VISIBLE | SS_LEFT,
                                    display_left,
                                    y,
                                    FIELD_WIDTH,
                                    LABEL_HEIGHT,
                                    parent,
                                    ptr::null_mut(),
                                    GetModuleHandleA(ptr::null()),
                                    ptr::null_mut(),
                                );
                                let label_cstr = CString::new(label_text.as_str()).unwrap();
                                SetWindowTextA(label_hwnd, label_cstr.as_ptr() as *const u8);
                                SendMessageA(label_hwnd, WM_SETFONT, hfont as WPARAM, 1);
                                window.controls.borrow_mut().insert(
                                    format!("{}_label", id),
                                    Control::Label(label_hwnd, tab_index),
                                );
                                y += LABEL_HEIGHT + HINT_GAP;
                            }
                            let hwnd = CreateWindowExA(
                                WS_EX_CLIENTEDGE,
                                c"EDIT".as_ptr() as *const u8,
                                ptr::null(),
                                edit_style(*frozen, false),
                                display_left,
                                y,
                                FIELD_WIDTH,
                                ROW_HEIGHT,
                                parent,
                                control_id as _,
                                GetModuleHandleA(ptr::null()),
                                ptr::null_mut(),
                            );
                            SendMessageA(hwnd, WM_SETFONT, hfont as WPARAM, 1);
                            let cue_text: Vec<u16> = placeholder
                                .encode_utf16()
                                .chain(std::iter::once(0))
                                .collect();
                            SendMessageW(hwnd, EM_SETCUEBANNER, 0, cue_text.as_ptr() as LPARAM);
                            window
                                .control_id_to_form_id
                                .borrow_mut()
                                .insert(control_id, id.clone());
                            window
                                .controls
                                .borrow_mut()
                                .insert(id.clone(), Control::Edit(hwnd, tab_index));
                            y += ROW_HEIGHT + ROW_GAP;
                            if let Some(status_text) = status {
                                let status_cstr = CString::new(status_text.as_str()).unwrap();
                                let status_hwnd = CreateWindowExA(
                                    0,
                                    c"STATIC".as_ptr() as *const u8,
                                    status_cstr.as_ptr() as *const u8,
                                    WS_CHILD | WS_VISIBLE | SS_LEFT,
                                    display_left,
                                    y,
                                    FIELD_WIDTH,
                                    LABEL_HEIGHT,
                                    parent,
                                    ptr::null_mut(),
                                    GetModuleHandleA(ptr::null()),
                                    ptr::null_mut(),
                                );
                                SendMessageA(status_hwnd, WM_SETFONT, hfont as WPARAM, 1);
                                window.controls.borrow_mut().insert(
                                    format!("{}_status", id),
                                    Control::Label(status_hwnd, tab_index),
                                );
                                y += LABEL_HEIGHT + HINT_GAP;
                            }
                            if let Some(disclosure_text) = disclosure {
                                let button_cstr = CString::new("What is this?").unwrap();
                                let button_hwnd = CreateWindowExA(
                                    0,
                                    c"BUTTON".as_ptr() as *const u8,
                                    button_cstr.as_ptr() as *const u8,
                                    WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_PUSHBUTTON as u32,
                                    display_left,
                                    y,
                                    120,
                                    ROW_HEIGHT,
                                    parent,
                                    control_id as _,
                                    GetModuleHandleA(ptr::null()),
                                    ptr::null_mut(),
                                );
                                SendMessageA(button_hwnd, WM_SETFONT, hfont as WPARAM, 1);
                                y += ROW_HEIGHT + HINT_GAP;

                                let disclosure_cstr =
                                    CString::new(disclosure_text.as_str()).unwrap();
                                let label_height =
                                    measure_wrapped_text_height(disclosure_text, FIELD_WIDTH);
                                let label_hwnd = CreateWindowExA(
                                    0,
                                    c"STATIC".as_ptr() as *const u8,
                                    disclosure_cstr.as_ptr() as *const u8,
                                    WS_CHILD | SS_LEFT,
                                    display_left,
                                    y,
                                    FIELD_WIDTH,
                                    label_height,
                                    parent,
                                    ptr::null_mut(),
                                    GetModuleHandleA(ptr::null()),
                                    ptr::null_mut(),
                                );
                                SendMessageA(label_hwnd, WM_SETFONT, hfont as WPARAM, 1);

                                let disclosure_id = format!("{}_disclosure", id);
                                window
                                    .control_id_to_form_id
                                    .borrow_mut()
                                    .insert(control_id, disclosure_id.clone());
                                window.controls.borrow_mut().insert(
                                    disclosure_id,
                                    Control::Disclosure(button_hwnd, label_hwnd, tab_index),
                                );
                                y += label_height + HINT_GAP;
                                control_id += 1;
                            }
                            // The `_help` suffix is what keeps a refresh from
                            // writing the row's value over the hint.
                            if let Some(help_text) = help {
                                let help_hwnd = CreateWindowExA(
                                    0,
                                    c"STATIC".as_ptr() as *const u8,
                                    ptr::null(),
                                    WS_CHILD | WS_VISIBLE | SS_LEFT,
                                    display_left,
                                    y,
                                    FIELD_WIDTH,
                                    LABEL_HEIGHT,
                                    parent,
                                    ptr::null_mut(),
                                    GetModuleHandleA(ptr::null()),
                                    ptr::null_mut(),
                                );
                                let help_cstr = CString::new(help_text.as_str()).unwrap();
                                SetWindowTextA(help_hwnd, help_cstr.as_ptr() as *const u8);
                                SendMessageA(help_hwnd, WM_SETFONT, hfont as WPARAM, 1);
                                window.controls.borrow_mut().insert(
                                    format!("{}_help", id),
                                    Control::Label(help_hwnd, tab_index),
                                );
                                y += LABEL_HEIGHT + HINT_GAP;
                            }
                            control_id += 1;
                        }
                        FormRow::SecureField {
                            id,
                            label,
                            frozen,
                            status,
                            ..
                        } => {
                            if let Some(label_text) = label {
                                let label_hwnd = CreateWindowExA(
                                    0,
                                    c"STATIC".as_ptr() as *const u8,
                                    ptr::null(),
                                    WS_CHILD | WS_VISIBLE | SS_LEFT,
                                    display_left,
                                    y,
                                    FIELD_WIDTH,
                                    LABEL_HEIGHT,
                                    parent,
                                    ptr::null_mut(),
                                    GetModuleHandleA(ptr::null()),
                                    ptr::null_mut(),
                                );
                                let label_cstr = CString::new(label_text.as_str()).unwrap();
                                SetWindowTextA(label_hwnd, label_cstr.as_ptr() as *const u8);
                                SendMessageA(label_hwnd, WM_SETFONT, hfont as WPARAM, 1);
                                window.controls.borrow_mut().insert(
                                    format!("{}_label", id),
                                    Control::Label(label_hwnd, tab_index),
                                );
                                y += LABEL_HEIGHT + HINT_GAP;
                            }
                            let hwnd = CreateWindowExA(
                                WS_EX_CLIENTEDGE,
                                c"EDIT".as_ptr() as *const u8,
                                ptr::null(),
                                edit_style(*frozen, true),
                                display_left,
                                y,
                                FIELD_WIDTH,
                                ROW_HEIGHT,
                                parent,
                                control_id as _,
                                GetModuleHandleA(ptr::null()),
                                ptr::null_mut(),
                            );
                            SendMessageA(hwnd, WM_SETFONT, hfont as WPARAM, 1);
                            let cue_text: Vec<u16> =
                                "••••".encode_utf16().chain(std::iter::once(0)).collect();
                            SendMessageW(hwnd, EM_SETCUEBANNER, 0, cue_text.as_ptr() as LPARAM);
                            window
                                .control_id_to_form_id
                                .borrow_mut()
                                .insert(control_id, id.clone());
                            window
                                .controls
                                .borrow_mut()
                                .insert(id.clone(), Control::Edit(hwnd, tab_index));
                            y += ROW_HEIGHT + ROW_GAP;
                            if let Some(status_text) = status {
                                let status_cstr = CString::new(status_text.as_str()).unwrap();
                                let status_hwnd = CreateWindowExA(
                                    0,
                                    c"STATIC".as_ptr() as *const u8,
                                    status_cstr.as_ptr() as *const u8,
                                    WS_CHILD | WS_VISIBLE | SS_LEFT,
                                    display_left,
                                    y,
                                    FIELD_WIDTH,
                                    LABEL_HEIGHT,
                                    parent,
                                    ptr::null_mut(),
                                    GetModuleHandleA(ptr::null()),
                                    ptr::null_mut(),
                                );
                                SendMessageA(status_hwnd, WM_SETFONT, hfont as WPARAM, 1);
                                window.controls.borrow_mut().insert(
                                    format!("{}_status", id),
                                    Control::Label(status_hwnd, tab_index),
                                );
                                y += LABEL_HEIGHT + HINT_GAP;
                            }
                            control_id += 1;
                        }
                        FormRow::Popup {
                            id,
                            label,
                            help,
                            options,
                            frozen,
                            status,
                            disclosure,
                            ..
                        } => {
                            if let Some(label_text) = label {
                                let label_hwnd = CreateWindowExA(
                                    0,
                                    c"STATIC".as_ptr() as *const u8,
                                    ptr::null(),
                                    WS_CHILD | WS_VISIBLE | SS_LEFT,
                                    display_left,
                                    y,
                                    FIELD_WIDTH,
                                    LABEL_HEIGHT,
                                    parent,
                                    ptr::null_mut(),
                                    GetModuleHandleA(ptr::null()),
                                    ptr::null_mut(),
                                );
                                let label_cstr = CString::new(label_text.as_str()).unwrap();
                                SetWindowTextA(label_hwnd, label_cstr.as_ptr() as *const u8);
                                SendMessageA(label_hwnd, WM_SETFONT, hfont as WPARAM, 1);
                                window.controls.borrow_mut().insert(
                                    format!("{}_label", id),
                                    Control::Label(label_hwnd, tab_index),
                                );
                                y += LABEL_HEIGHT + HINT_GAP;
                            }
                            // An exported variable owns the pick, so the row
                            // shows it and takes no edit (#272). Set once at
                            // creation, where AppKit sets `setEnabled`, because
                            // `frozen` cannot change while the window lives.
                            let mut style = WS_CHILD | WS_VISIBLE | WS_TABSTOP | CBS_DROPDOWNLIST;
                            if *frozen {
                                style |= WS_DISABLED;
                            }
                            let hwnd = CreateWindowExA(
                                0,
                                c"COMBOBOX".as_ptr() as *const u8,
                                ptr::null(),
                                style,
                                display_left,
                                y,
                                FIELD_WIDTH,
                                200,
                                parent,
                                control_id as _,
                                GetModuleHandleA(ptr::null()),
                                ptr::null_mut(),
                            );
                            SendMessageA(hwnd, WM_SETFONT, hfont as WPARAM, 1);
                            window
                                .control_id_to_form_id
                                .borrow_mut()
                                .insert(control_id, id.clone());
                            window.controls.borrow_mut().insert(
                                id.clone(),
                                Control::ComboBox(hwnd, tab_index, options.clone()),
                            );
                            y += ROW_HEIGHT + ROW_GAP;
                            if let Some(status_text) = status {
                                let status_cstr = CString::new(status_text.as_str()).unwrap();
                                let status_hwnd = CreateWindowExA(
                                    0,
                                    c"STATIC".as_ptr() as *const u8,
                                    status_cstr.as_ptr() as *const u8,
                                    WS_CHILD | WS_VISIBLE | SS_LEFT,
                                    display_left,
                                    y,
                                    FIELD_WIDTH,
                                    LABEL_HEIGHT,
                                    parent,
                                    ptr::null_mut(),
                                    GetModuleHandleA(ptr::null()),
                                    ptr::null_mut(),
                                );
                                SendMessageA(status_hwnd, WM_SETFONT, hfont as WPARAM, 1);
                                window.controls.borrow_mut().insert(
                                    format!("{}_status", id),
                                    Control::Label(status_hwnd, tab_index),
                                );
                                y += LABEL_HEIGHT + HINT_GAP;
                            }
                            if let Some(disclosure_text) = disclosure {
                                let button_cstr = CString::new("What is this?").unwrap();
                                let button_hwnd = CreateWindowExA(
                                    0,
                                    c"BUTTON".as_ptr() as *const u8,
                                    button_cstr.as_ptr() as *const u8,
                                    WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_PUSHBUTTON as u32,
                                    display_left,
                                    y,
                                    120,
                                    ROW_HEIGHT,
                                    parent,
                                    control_id as _,
                                    GetModuleHandleA(ptr::null()),
                                    ptr::null_mut(),
                                );
                                SendMessageA(button_hwnd, WM_SETFONT, hfont as WPARAM, 1);
                                y += ROW_HEIGHT + HINT_GAP;

                                let disclosure_cstr =
                                    CString::new(disclosure_text.as_str()).unwrap();
                                let label_height =
                                    measure_wrapped_text_height(disclosure_text, FIELD_WIDTH);
                                let label_hwnd = CreateWindowExA(
                                    0,
                                    c"STATIC".as_ptr() as *const u8,
                                    disclosure_cstr.as_ptr() as *const u8,
                                    WS_CHILD | SS_LEFT,
                                    display_left,
                                    y,
                                    FIELD_WIDTH,
                                    label_height,
                                    parent,
                                    ptr::null_mut(),
                                    GetModuleHandleA(ptr::null()),
                                    ptr::null_mut(),
                                );
                                SendMessageA(label_hwnd, WM_SETFONT, hfont as WPARAM, 1);

                                let disclosure_id = format!("{}_disclosure", id);
                                window
                                    .control_id_to_form_id
                                    .borrow_mut()
                                    .insert(control_id, disclosure_id.clone());
                                window.controls.borrow_mut().insert(
                                    disclosure_id,
                                    Control::Disclosure(button_hwnd, label_hwnd, tab_index),
                                );
                                y += label_height + HINT_GAP;
                                control_id += 1;
                            }
                            if let Some(help_text) = help {
                                let help_cstr = CString::new(help_text.as_str()).unwrap();
                                let help_hwnd = CreateWindowExA(
                                    0,
                                    c"STATIC".as_ptr() as *const u8,
                                    help_cstr.as_ptr() as *const u8,
                                    WS_CHILD | WS_VISIBLE | SS_LEFT,
                                    display_left,
                                    y,
                                    FIELD_WIDTH,
                                    LABEL_HEIGHT,
                                    parent,
                                    ptr::null_mut(),
                                    GetModuleHandleA(ptr::null()),
                                    ptr::null_mut(),
                                );
                                SendMessageA(help_hwnd, WM_SETFONT, hfont as WPARAM, 1);
                                window.controls.borrow_mut().insert(
                                    format!("{}_help", id),
                                    Control::Label(help_hwnd, tab_index),
                                );
                                y += LABEL_HEIGHT + HINT_GAP;
                            }
                            control_id += 1;
                        }
                        FormRow::List {
                            id,
                            help,
                            disclosure,
                            ..
                        } => {
                            let container_hwnd = CreateWindowExA(
                                0,
                                INSTANCES_LIST_CLASS.as_ptr() as *const u8,
                                ptr::null(),
                                WS_CHILD | WS_VISIBLE,
                                display_left,
                                y,
                                FIELD_WIDTH,
                                MULTILINE_HEIGHT,
                                parent,
                                ptr::null_mut(),
                                GetModuleHandleA(ptr::null()),
                                ptr::null_mut(),
                            );
                            window.controls.borrow_mut().insert(
                                id.clone(),
                                Control::InstancesList(container_hwnd, tab_index),
                            );
                            y += MULTILINE_HEIGHT + ROW_GAP;
                            if let Some(disclosure_text) = disclosure {
                                let button_cstr = CString::new("What is this?").unwrap();
                                let button_hwnd = CreateWindowExA(
                                    0,
                                    c"BUTTON".as_ptr() as *const u8,
                                    button_cstr.as_ptr() as *const u8,
                                    WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_PUSHBUTTON as u32,
                                    display_left,
                                    y,
                                    120,
                                    ROW_HEIGHT,
                                    parent,
                                    control_id as _,
                                    GetModuleHandleA(ptr::null()),
                                    ptr::null_mut(),
                                );
                                SendMessageA(button_hwnd, WM_SETFONT, hfont as WPARAM, 1);
                                y += ROW_HEIGHT + HINT_GAP;

                                let disclosure_cstr =
                                    CString::new(disclosure_text.as_str()).unwrap();
                                let label_height =
                                    measure_wrapped_text_height(disclosure_text, FIELD_WIDTH);
                                let label_hwnd = CreateWindowExA(
                                    0,
                                    c"STATIC".as_ptr() as *const u8,
                                    disclosure_cstr.as_ptr() as *const u8,
                                    WS_CHILD | SS_LEFT,
                                    display_left,
                                    y,
                                    FIELD_WIDTH,
                                    label_height,
                                    parent,
                                    ptr::null_mut(),
                                    GetModuleHandleA(ptr::null()),
                                    ptr::null_mut(),
                                );
                                SendMessageA(label_hwnd, WM_SETFONT, hfont as WPARAM, 1);

                                let disclosure_id = format!("{}_disclosure", id);
                                window
                                    .control_id_to_form_id
                                    .borrow_mut()
                                    .insert(control_id, disclosure_id.clone());
                                window.controls.borrow_mut().insert(
                                    disclosure_id,
                                    Control::Disclosure(button_hwnd, label_hwnd, tab_index),
                                );
                                y += label_height + HINT_GAP;
                                control_id += 1;
                            }
                            if let Some(help_text) = help {
                                let help_cstr = CString::new(help_text.as_str()).unwrap();
                                let help_hwnd = CreateWindowExA(
                                    0,
                                    c"STATIC".as_ptr() as *const u8,
                                    help_cstr.as_ptr() as *const u8,
                                    WS_CHILD | WS_VISIBLE | SS_LEFT,
                                    display_left,
                                    y,
                                    FIELD_WIDTH,
                                    LABEL_HEIGHT,
                                    parent,
                                    ptr::null_mut(),
                                    GetModuleHandleA(ptr::null()),
                                    ptr::null_mut(),
                                );
                                SendMessageA(help_hwnd, WM_SETFONT, hfont as WPARAM, 1);
                                window.controls.borrow_mut().insert(
                                    format!("{}_help", id),
                                    Control::Label(help_hwnd, tab_index),
                                );
                                y += LABEL_HEIGHT + HINT_GAP;
                            }
                        }
                        FormRow::Composite {
                            controls,
                            help,
                            disclosure,
                            ..
                        } => {
                            let mut x = display_left;
                            for control in controls {
                                match control {
                                    form::CompositeControl::TextField { id, placeholder } => {
                                        let field_width = 120;
                                        let hwnd = CreateWindowExA(
                                            WS_EX_CLIENTEDGE,
                                            c"EDIT".as_ptr() as *const u8,
                                            ptr::null(),
                                            WS_CHILD | WS_VISIBLE | WS_TABSTOP | WS_BORDER,
                                            x,
                                            y,
                                            field_width,
                                            ROW_HEIGHT,
                                            parent,
                                            control_id as _,
                                            GetModuleHandleA(ptr::null()),
                                            ptr::null_mut(),
                                        );
                                        SendMessageA(hwnd, WM_SETFONT, hfont as WPARAM, 1);
                                        let cue_text: Vec<u16> = placeholder
                                            .encode_utf16()
                                            .chain(std::iter::once(0))
                                            .collect();
                                        SendMessageW(
                                            hwnd,
                                            EM_SETCUEBANNER,
                                            0,
                                            cue_text.as_ptr() as LPARAM,
                                        );
                                        window
                                            .control_id_to_form_id
                                            .borrow_mut()
                                            .insert(control_id, id.clone());
                                        window
                                            .controls
                                            .borrow_mut()
                                            .insert(id.clone(), Control::Edit(hwnd, tab_index));
                                        x += field_width + 8;
                                        control_id += 1;
                                    }
                                    // Skipped entirely until #461 maps a
                                    // control back to its row. This port
                                    // commits no composite pick, and a combo
                                    // box that lists endpoints and then
                                    // ignores the click is the failure #465
                                    // set out to remove. An empty options vec
                                    // is not the way to say that either: the
                                    // refresh above reads empty as "fill from
                                    // `view.installed`", which would offer
                                    // Character packages as endpoints.
                                    form::CompositeControl::Popup { id, .. }
                                        if id == form::DIRECTOR_BASE_URL_PICK_ID => {}
                                    form::CompositeControl::Popup { id, frozen, .. } => {
                                        let combo_width = 100;
                                        // Disabled at creation for the same
                                        // reason `FormRow::Popup` is: an
                                        // exported variable owns the pick, and
                                        // `frozen` cannot change while the
                                        // window lives (#272).
                                        let mut style =
                                            WS_CHILD | WS_VISIBLE | WS_TABSTOP | CBS_DROPDOWNLIST;
                                        if *frozen {
                                            style |= WS_DISABLED;
                                        }
                                        let hwnd = CreateWindowExA(
                                            0,
                                            c"COMBOBOX".as_ptr() as *const u8,
                                            ptr::null(),
                                            style,
                                            x,
                                            y,
                                            combo_width,
                                            200,
                                            parent,
                                            control_id as _,
                                            GetModuleHandleA(ptr::null()),
                                            ptr::null_mut(),
                                        );
                                        SendMessageA(hwnd, WM_SETFONT, hfont as WPARAM, 1);
                                        window
                                            .control_id_to_form_id
                                            .borrow_mut()
                                            .insert(control_id, id.clone());
                                        window.controls.borrow_mut().insert(
                                            id.clone(),
                                            Control::ComboBox(hwnd, tab_index, Vec::new()),
                                        );
                                        x += combo_width + 8;
                                        control_id += 1;
                                    }
                                    form::CompositeControl::Button { id, label, .. } => {
                                        let button_width = 80;
                                        let label_cstr = CString::new(label.as_str()).unwrap();
                                        let hwnd = CreateWindowExA(
                                            0,
                                            c"BUTTON".as_ptr() as *const u8,
                                            label_cstr.as_ptr() as *const u8,
                                            WS_CHILD | WS_VISIBLE | WS_TABSTOP,
                                            x,
                                            y,
                                            button_width,
                                            ROW_HEIGHT,
                                            parent,
                                            control_id as _,
                                            GetModuleHandleA(ptr::null()),
                                            ptr::null_mut(),
                                        );
                                        SendMessageA(hwnd, WM_SETFONT, hfont as WPARAM, 1);
                                        window
                                            .control_id_to_form_id
                                            .borrow_mut()
                                            .insert(control_id, id.clone());
                                        window
                                            .controls
                                            .borrow_mut()
                                            .insert(id.clone(), Control::Button(hwnd, tab_index));
                                        x += button_width + 8;
                                        control_id += 1;
                                    }
                                }
                            }
                            y += ROW_HEIGHT + ROW_GAP;
                            if let Some(disclosure_text) = disclosure {
                                let button_cstr = CString::new("What is this?").unwrap();
                                let button_hwnd = CreateWindowExA(
                                    0,
                                    c"BUTTON".as_ptr() as *const u8,
                                    button_cstr.as_ptr() as *const u8,
                                    WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_PUSHBUTTON as u32,
                                    display_left,
                                    y,
                                    120,
                                    ROW_HEIGHT,
                                    parent,
                                    control_id as _,
                                    GetModuleHandleA(ptr::null()),
                                    ptr::null_mut(),
                                );
                                SendMessageA(button_hwnd, WM_SETFONT, hfont as WPARAM, 1);
                                y += ROW_HEIGHT + HINT_GAP;

                                let disclosure_cstr =
                                    CString::new(disclosure_text.as_str()).unwrap();
                                let label_height =
                                    measure_wrapped_text_height(disclosure_text, FIELD_WIDTH);
                                let label_hwnd = CreateWindowExA(
                                    0,
                                    c"STATIC".as_ptr() as *const u8,
                                    disclosure_cstr.as_ptr() as *const u8,
                                    WS_CHILD | SS_LEFT,
                                    display_left,
                                    y,
                                    FIELD_WIDTH,
                                    label_height,
                                    parent,
                                    ptr::null_mut(),
                                    GetModuleHandleA(ptr::null()),
                                    ptr::null_mut(),
                                );
                                SendMessageA(label_hwnd, WM_SETFONT, hfont as WPARAM, 1);

                                let disclosure_id = format!("composite_disclosure_{}", control_id);
                                window
                                    .control_id_to_form_id
                                    .borrow_mut()
                                    .insert(control_id, disclosure_id.clone());
                                window.controls.borrow_mut().insert(
                                    disclosure_id,
                                    Control::Disclosure(button_hwnd, label_hwnd, tab_index),
                                );
                                y += label_height + HINT_GAP;
                                control_id += 1;
                            }
                            if let Some(help_text) = help {
                                let help_cstr = CString::new(help_text.as_str()).unwrap();
                                let help_hwnd = CreateWindowExA(
                                    0,
                                    c"STATIC".as_ptr() as *const u8,
                                    help_cstr.as_ptr() as *const u8,
                                    WS_CHILD | WS_VISIBLE | SS_LEFT,
                                    display_left,
                                    y,
                                    FIELD_WIDTH,
                                    LABEL_HEIGHT,
                                    parent,
                                    ptr::null_mut(),
                                    GetModuleHandleA(ptr::null()),
                                    ptr::null_mut(),
                                );
                                SendMessageA(help_hwnd, WM_SETFONT, hfont as WPARAM, 1);
                                window.controls.borrow_mut().insert(
                                    format!("composite_help_{}", control_id),
                                    Control::Label(help_hwnd, tab_index),
                                );
                                y += LABEL_HEIGHT + HINT_GAP;
                            }
                        }
                        FormRow::Multiline {
                            id,
                            label,
                            help,
                            editable,
                            disclosure,
                            ..
                        } => {
                            if let Some(label_text) = label {
                                let label_hwnd = CreateWindowExA(
                                    0,
                                    c"STATIC".as_ptr() as *const u8,
                                    ptr::null(),
                                    WS_CHILD | WS_VISIBLE | SS_LEFT,
                                    display_left,
                                    y,
                                    FIELD_WIDTH,
                                    LABEL_HEIGHT,
                                    parent,
                                    ptr::null_mut(),
                                    GetModuleHandleA(ptr::null()),
                                    ptr::null_mut(),
                                );
                                let label_cstr = CString::new(label_text.as_str()).unwrap();
                                SetWindowTextA(label_hwnd, label_cstr.as_ptr() as *const u8);
                                SendMessageA(label_hwnd, WM_SETFONT, hfont as WPARAM, 1);
                                window.controls.borrow_mut().insert(
                                    format!("{}_label", id),
                                    Control::Label(label_hwnd, tab_index),
                                );
                                y += LABEL_HEIGHT + HINT_GAP;
                            }
                            let style = if *editable {
                                WS_CHILD
                                    | WS_VISIBLE
                                    | WS_TABSTOP
                                    | WS_BORDER
                                    | WS_VSCROLL
                                    | ES_MULTILINE as u32
                                    | ES_AUTOVSCROLL as u32
                                    | ES_WANTRETURN as u32
                            } else {
                                WS_CHILD
                                    | WS_VISIBLE
                                    | WS_BORDER
                                    | WS_VSCROLL
                                    | ES_MULTILINE as u32
                                    | ES_AUTOVSCROLL as u32
                                    | ES_READONLY as u32
                            };
                            let hwnd = CreateWindowExA(
                                WS_EX_CLIENTEDGE,
                                c"EDIT".as_ptr() as *const u8,
                                ptr::null(),
                                style,
                                display_left,
                                y,
                                FIELD_WIDTH,
                                MULTILINE_HEIGHT,
                                parent,
                                control_id as _,
                                GetModuleHandleA(ptr::null()),
                                ptr::null_mut(),
                            );
                            SendMessageA(hwnd, WM_SETFONT, hfont as WPARAM, 1);
                            window
                                .control_id_to_form_id
                                .borrow_mut()
                                .insert(control_id, id.clone());
                            window
                                .controls
                                .borrow_mut()
                                .insert(id.clone(), Control::Edit(hwnd, tab_index));
                            y += MULTILINE_HEIGHT + ROW_GAP;
                            if let Some(disclosure_text) = disclosure {
                                let button_cstr = CString::new("What is this?").unwrap();
                                let button_hwnd = CreateWindowExA(
                                    0,
                                    c"BUTTON".as_ptr() as *const u8,
                                    button_cstr.as_ptr() as *const u8,
                                    WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_PUSHBUTTON as u32,
                                    display_left,
                                    y,
                                    120,
                                    ROW_HEIGHT,
                                    parent,
                                    control_id as _,
                                    GetModuleHandleA(ptr::null()),
                                    ptr::null_mut(),
                                );
                                SendMessageA(button_hwnd, WM_SETFONT, hfont as WPARAM, 1);
                                y += ROW_HEIGHT + HINT_GAP;

                                let disclosure_cstr =
                                    CString::new(disclosure_text.as_str()).unwrap();
                                let label_height =
                                    measure_wrapped_text_height(disclosure_text, FIELD_WIDTH);
                                let label_hwnd = CreateWindowExA(
                                    0,
                                    c"STATIC".as_ptr() as *const u8,
                                    disclosure_cstr.as_ptr() as *const u8,
                                    WS_CHILD | SS_LEFT,
                                    display_left,
                                    y,
                                    FIELD_WIDTH,
                                    label_height,
                                    parent,
                                    ptr::null_mut(),
                                    GetModuleHandleA(ptr::null()),
                                    ptr::null_mut(),
                                );
                                SendMessageA(label_hwnd, WM_SETFONT, hfont as WPARAM, 1);

                                let disclosure_id = format!("{}_disclosure", id);
                                window
                                    .control_id_to_form_id
                                    .borrow_mut()
                                    .insert(control_id, disclosure_id.clone());
                                window.controls.borrow_mut().insert(
                                    disclosure_id,
                                    Control::Disclosure(button_hwnd, label_hwnd, tab_index),
                                );
                                y += label_height + HINT_GAP;
                                control_id += 1;
                            }
                            if let Some(help_text) = help {
                                let help_cstr = CString::new(help_text.as_str()).unwrap();
                                let help_hwnd = CreateWindowExA(
                                    0,
                                    c"STATIC".as_ptr() as *const u8,
                                    help_cstr.as_ptr() as *const u8,
                                    WS_CHILD | WS_VISIBLE | SS_LEFT,
                                    display_left,
                                    y,
                                    FIELD_WIDTH,
                                    LABEL_HEIGHT,
                                    parent,
                                    ptr::null_mut(),
                                    GetModuleHandleA(ptr::null()),
                                    ptr::null_mut(),
                                );
                                SendMessageA(help_hwnd, WM_SETFONT, hfont as WPARAM, 1);
                                window.controls.borrow_mut().insert(
                                    format!("{}_help", id),
                                    Control::Label(help_hwnd, tab_index),
                                );
                                y += LABEL_HEIGHT + HINT_GAP;
                            }
                            control_id += 1;
                        }
                        FormRow::InspectBlock {
                            id,
                            label,
                            help,
                            status,
                            disclosure,
                            ..
                        } => {
                            if let Some(label_text) = label {
                                let label_hwnd = CreateWindowExA(
                                    0,
                                    c"STATIC".as_ptr() as *const u8,
                                    ptr::null(),
                                    WS_CHILD | WS_VISIBLE | SS_LEFT,
                                    display_left,
                                    y,
                                    FIELD_WIDTH,
                                    LABEL_HEIGHT,
                                    parent,
                                    ptr::null_mut(),
                                    GetModuleHandleA(ptr::null()),
                                    ptr::null_mut(),
                                );
                                let label_cstr = CString::new(label_text.as_str()).unwrap();
                                SetWindowTextA(label_hwnd, label_cstr.as_ptr() as *const u8);
                                SendMessageA(label_hwnd, WM_SETFONT, hfont as WPARAM, 1);
                                window.controls.borrow_mut().insert(
                                    format!("{}_label", id),
                                    Control::Label(label_hwnd, tab_index),
                                );
                                y += LABEL_HEIGHT + HINT_GAP;
                            }
                            let hwnd = CreateWindowExA(
                                WS_EX_CLIENTEDGE,
                                c"EDIT".as_ptr() as *const u8,
                                ptr::null(),
                                WS_CHILD
                                    | WS_VISIBLE
                                    | WS_BORDER
                                    | WS_VSCROLL
                                    | ES_MULTILINE as u32
                                    | ES_AUTOVSCROLL as u32
                                    | ES_READONLY as u32,
                                display_left,
                                y,
                                FIELD_WIDTH,
                                INSPECT_BLOCK_HEIGHT,
                                parent,
                                ptr::null_mut(),
                                GetModuleHandleA(ptr::null()),
                                ptr::null_mut(),
                            );
                            SendMessageA(hwnd, WM_SETFONT, hfont as WPARAM, 1);
                            window
                                .controls
                                .borrow_mut()
                                .insert(id.clone(), Control::Label(hwnd, tab_index));
                            y += INSPECT_BLOCK_HEIGHT + ROW_GAP;
                            if let Some(status_text) = status {
                                let status_cstr = CString::new(status_text.as_str()).unwrap();
                                let status_hwnd = CreateWindowExA(
                                    0,
                                    c"STATIC".as_ptr() as *const u8,
                                    status_cstr.as_ptr() as *const u8,
                                    WS_CHILD | WS_VISIBLE | SS_LEFT,
                                    display_left,
                                    y,
                                    FIELD_WIDTH,
                                    LABEL_HEIGHT,
                                    parent,
                                    ptr::null_mut(),
                                    GetModuleHandleA(ptr::null()),
                                    ptr::null_mut(),
                                );
                                SendMessageA(status_hwnd, WM_SETFONT, hfont as WPARAM, 1);
                                window.controls.borrow_mut().insert(
                                    format!("{}_status", id),
                                    Control::Label(status_hwnd, tab_index),
                                );
                                y += LABEL_HEIGHT + HINT_GAP;
                            }
                            if let Some(disclosure_text) = disclosure {
                                let button_cstr = CString::new("What is this?").unwrap();
                                let button_hwnd = CreateWindowExA(
                                    0,
                                    c"BUTTON".as_ptr() as *const u8,
                                    button_cstr.as_ptr() as *const u8,
                                    WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_PUSHBUTTON as u32,
                                    display_left,
                                    y,
                                    120,
                                    ROW_HEIGHT,
                                    parent,
                                    control_id as _,
                                    GetModuleHandleA(ptr::null()),
                                    ptr::null_mut(),
                                );
                                SendMessageA(button_hwnd, WM_SETFONT, hfont as WPARAM, 1);
                                y += ROW_HEIGHT + HINT_GAP;

                                let disclosure_cstr =
                                    CString::new(disclosure_text.as_str()).unwrap();
                                let label_height =
                                    measure_wrapped_text_height(disclosure_text, FIELD_WIDTH);
                                let label_hwnd = CreateWindowExA(
                                    0,
                                    c"STATIC".as_ptr() as *const u8,
                                    disclosure_cstr.as_ptr() as *const u8,
                                    WS_CHILD | SS_LEFT,
                                    display_left,
                                    y,
                                    FIELD_WIDTH,
                                    label_height,
                                    parent,
                                    ptr::null_mut(),
                                    GetModuleHandleA(ptr::null()),
                                    ptr::null_mut(),
                                );
                                SendMessageA(label_hwnd, WM_SETFONT, hfont as WPARAM, 1);

                                let disclosure_id = format!("{}_disclosure", id);
                                window
                                    .control_id_to_form_id
                                    .borrow_mut()
                                    .insert(control_id, disclosure_id.clone());
                                window.controls.borrow_mut().insert(
                                    disclosure_id,
                                    Control::Disclosure(button_hwnd, label_hwnd, tab_index),
                                );
                                y += label_height + HINT_GAP;
                                control_id += 1;
                            }
                            if let Some(help_text) = help {
                                let help_cstr = CString::new(help_text.as_str()).unwrap();
                                let help_hwnd = CreateWindowExA(
                                    0,
                                    c"STATIC".as_ptr() as *const u8,
                                    help_cstr.as_ptr() as *const u8,
                                    WS_CHILD | WS_VISIBLE | SS_LEFT,
                                    display_left,
                                    y,
                                    FIELD_WIDTH,
                                    LABEL_HEIGHT,
                                    parent,
                                    ptr::null_mut(),
                                    GetModuleHandleA(ptr::null()),
                                    ptr::null_mut(),
                                );
                                SendMessageA(help_hwnd, WM_SETFONT, hfont as WPARAM, 1);
                                window.controls.borrow_mut().insert(
                                    format!("{}_help", id),
                                    Control::Label(help_hwnd, tab_index),
                                );
                                y += LABEL_HEIGHT + HINT_GAP;
                            }
                        }
                        FormRow::InspectPath { id } => {
                            let hwnd = CreateWindowExA(
                                0,
                                c"STATIC".as_ptr() as *const u8,
                                ptr::null(),
                                WS_CHILD | WS_VISIBLE | SS_LEFT,
                                display_left,
                                y,
                                FIELD_WIDTH,
                                LABEL_HEIGHT,
                                parent,
                                ptr::null_mut(),
                                GetModuleHandleA(ptr::null()),
                                ptr::null_mut(),
                            );
                            SendMessageA(hwnd, WM_SETFONT, hfont as WPARAM, 1);
                            window
                                .controls
                                .borrow_mut()
                                .insert(id.clone(), Control::Label(hwnd, tab_index));
                            y += LABEL_HEIGHT + ROW_GAP;
                        }
                    }
                }
            }
        }

        window.update_tab_visibility();

        Ok(())
    }
}

fn caption_hit_test(alt_held: bool, hit: Hit) -> LRESULT {
    if should_begin_move(alt_held, hit) {
        HTCAPTION as LRESULT
    } else {
        HTCLIENT as LRESULT
    }
}

/// `tab_on_item` is only meaningful for the tab control; ignored otherwise. #460.
fn hit_from_win32(class: &str, tab_on_item: bool) -> Hit {
    match class {
        "Edit" | "Button" | "ComboBox" => Hit::Control,
        "SysTabControl32" => {
            if tab_on_item {
                Hit::Control
            } else {
                Hit::Background
            }
        }
        _ => Hit::Background,
    }
}

/// Map a parent-client point into a child's client space. Same subtraction
/// `MapWindowPoints` does; the live path uses ClientToScreen/ScreenToClient
/// because it has HWNDs. #460
#[cfg(test)]
fn map_into_child_client(pt_in_parent: POINT, child_origin_in_parent: POINT) -> POINT {
    POINT {
        x: pt_in_parent.x - child_origin_in_parent.x,
        y: pt_in_parent.y - child_origin_in_parent.y,
    }
}

fn hit_at(hwnd: HWND, lparam: LPARAM) -> Hit {
    let screen = POINT {
        x: lparam as i16 as i32,
        y: (lparam >> 16) as i16 as i32,
    };
    let mut pt = screen;
    // SAFETY: `pt` is a stack POINT the API writes in place.
    unsafe {
        ScreenToClient(hwnd, &mut pt);
    }
    // SAFETY: parent is our settings HWND; the POINT is in its client space.
    let mut origin = hwnd;
    let mut child = unsafe { ChildWindowFromPointEx(hwnd, pt, CWP_SKIPINVISIBLE) };
    if child.is_null() || child == hwnd {
        return Hit::Background;
    }

    // Instances' Dismiss is a BUTTON inside the list host. ChildWindowFromPointEx
    // wants the child's client space; ScreenToClient on an already-client point
    // treats it as screen and misses the button. #460
    loop {
        // SAFETY: `pt` is origin-client; the pair writes it to child-client.
        unsafe {
            ClientToScreen(origin, &mut pt);
            ScreenToClient(child, &mut pt);
        }
        // SAFETY: `child` is a live descendant of `hwnd`; `pt` is in its client space.
        let nested = unsafe { ChildWindowFromPointEx(child, pt, CWP_SKIPINVISIBLE) };
        if nested.is_null() || nested == child {
            break;
        }
        origin = child;
        child = nested;
    }
    let mut buf = [0u8; 256];
    // SAFETY: buffer is a writable C string of known size.
    let n = unsafe { GetClassNameA(child, buf.as_mut_ptr(), buf.len() as i32) };
    if n <= 0 {
        return Hit::Background;
    }
    let class = std::str::from_utf8(&buf[..n as usize]).unwrap_or("");
    let tab_on_item = if class == "SysTabControl32" {
        let mut tab_pt = screen;
        // SAFETY: `child` is the tab control still owned by this window.
        unsafe {
            ScreenToClient(child, &mut tab_pt);
        }
        let mut info = TCHITTESTINFO {
            pt: tab_pt,
            flags: 0,
        };
        // SAFETY: `info` lives for the SendMessage; TCM_HITTEST only reads/writes it.
        unsafe {
            SendMessageA(child, TCM_HITTEST, 0, &mut info as *mut _ as LPARAM);
        }
        info.flags & TCHT_ONITEM != 0
    } else {
        false
    };
    hit_from_win32(class, tab_on_item)
}

unsafe extern "system" fn list_host_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    // SAFETY: hwnd is the list host we registered; its parent is Settings.
    unsafe {
        if msg == WM_COMMAND || msg == WM_CTLCOLORSTATIC {
            // Parent is the Settings HWND that created this host.
            SendMessageA(GetParent(hwnd), msg, wparam, lparam)
        } else {
            DefWindowProcA(hwnd, msg, wparam, lparam)
        }
    }
}

unsafe extern "system" fn window_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    use windows_sys::Win32::UI::WindowsAndMessaging::{WM_CREATE, WM_DESTROY};
    match msg {
        WM_CREATE => 0,
        WM_NCHITTEST => {
            let def = windows_sys::Win32::UI::WindowsAndMessaging::DefWindowProcA(
                hwnd, msg, wparam, lparam,
            );
            if def != HTCLIENT as LRESULT {
                return def;
            }
            let alt = GetAsyncKeyState(VK_MENU as i32) < 0;
            caption_hit_test(alt, hit_at(hwnd, lparam))
        }
        WM_COMMAND => {
            let window_ptr = GetWindowLongPtrA(hwnd, GWLP_USERDATA);
            if window_ptr != 0 {
                let window = &*(window_ptr as *const SettingsWindow);
                let control_id = (wparam & 0xFFFF) as i32;
                let notification = ((wparam >> 16) & 0xFFFF) as u16;
                window.handle_command(control_id, notification);
            }
            0
        }
        WM_NOTIFY => {
            let window_ptr = GetWindowLongPtrA(hwnd, GWLP_USERDATA);
            if window_ptr != 0 && lparam != 0 {
                let nmhdr = &*(lparam as *const NMHDR);
                if nmhdr.code == TCN_SELCHANGE_CODE {
                    let window = &*(window_ptr as *const SettingsWindow);
                    let tab = GetDlgItem(hwnd, ID_TAB_CONTROL);
                    if !tab.is_null() {
                        let new_tab = SendMessageA(tab, TCM_GETCURSEL, 0, 0) as usize;
                        *window.current_tab.borrow_mut() = new_tab;
                        window.update_tab_visibility();
                    }
                }
            }
            0
        }
        WM_SIZE => {
            let tab = GetDlgItem(hwnd, ID_TAB_CONTROL);
            if !tab.is_null() {
                let mut rect: RECT = std::mem::zeroed();
                GetClientRect(hwnd, &mut rect);
                SetWindowPos(
                    tab,
                    ptr::null_mut(),
                    MARGIN,
                    MARGIN,
                    rect.right - MARGIN * 2,
                    rect.bottom - MARGIN * 2,
                    SWP_NOZORDER,
                );
                UpdateWindow(tab);
            }
            0
        }
        WM_CLOSE => {
            ShowWindow(hwnd, SW_HIDE);
            0
        }
        WM_DESTROY => {
            let window_ptr = GetWindowLongPtrA(hwnd, GWLP_USERDATA);
            if window_ptr != 0 {
                let _ = Arc::from_raw(window_ptr as *const SettingsWindow);
            }
            0
        }
        _ => windows_sys::Win32::UI::WindowsAndMessaging::DefWindowProcA(hwnd, msg, wparam, lparam),
    }
}

pub use refresh_if_showing as refresh_settings;
pub use show as show_settings;

#[cfg(test)]
mod tests {
    use super::*;

    /// Layout constants must match macOS and GTK for consistent readability
    /// across platforms. These are compile-time assertions so drift is caught
    /// at build time rather than by eyeball comparison.
    #[test]
    fn layout_constants_match_macos_gtk() {
        // Values from platform/macos/settings_window.rs and platform/x11/settings_window.rs
        const EXPECTED_WINDOW_WIDTH: i32 = 560;
        const EXPECTED_MARGIN: i32 = 28;
        const EXPECTED_ROW_GAP: i32 = 12;
        const EXPECTED_SECTION_GAP: i32 = 24;

        assert_eq!(
            WINDOW_WIDTH, EXPECTED_WINDOW_WIDTH,
            "WINDOW_WIDTH must match macOS/GTK"
        );
        assert_eq!(MARGIN, EXPECTED_MARGIN, "MARGIN must match macOS/GTK");
        assert_eq!(ROW_GAP, EXPECTED_ROW_GAP, "ROW_GAP must match macOS/GTK");
        assert_eq!(
            SECTION_GAP, EXPECTED_SECTION_GAP,
            "SECTION_GAP must match macOS/GTK"
        );
        assert_eq!(
            FIELD_WIDTH,
            EXPECTED_WINDOW_WIDTH - EXPECTED_MARGIN * 2,
            "FIELD_WIDTH calculation must match macOS/GTK"
        );
    }

    /// edit_style builds the correct flags for frozen and password controls.
    #[test]
    fn edit_style_sets_readonly_and_password_flags() {
        let editable_plain = edit_style(false, false);
        let frozen_plain = edit_style(true, false);
        let frozen_password = edit_style(true, true);

        assert_eq!(
            editable_plain & ES_READONLY as u32,
            0,
            "editable field must not have ES_READONLY"
        );
        assert_ne!(
            frozen_plain & ES_READONLY as u32,
            0,
            "frozen field must have ES_READONLY"
        );
        assert_ne!(
            frozen_password & ES_READONLY as u32,
            0,
            "frozen password field must have ES_READONLY"
        );
        assert_ne!(
            frozen_password & ES_PASSWORD as u32,
            0,
            "password field must have ES_PASSWORD"
        );
    }

    /// text_write returns the field for known text ids, so generic commit works.
    #[test]
    fn generic_text_commit_via_text_write() {
        let description = form::describe();

        // Wake interval (Director tab, not special-cased today)
        let wake_field = description.text_write(form::DIRECTOR_WAKE_SECS_ID);
        assert!(wake_field.is_some(), "wake interval must have a TextField");
        assert_eq!(
            wake_field,
            Some(crate::settings::TextField::DirectorWakeSecs),
            "wake interval writes to DirectorWakeSecs"
        );

        // Completer timeout (Development tab)
        let timeout_field = description.text_write(form::DIRECTOR_TIMEOUT_SECS_ID);
        assert!(timeout_field.is_some(), "timeout must have a TextField");
        assert_eq!(
            timeout_field,
            Some(crate::settings::TextField::DirectorTimeoutSecs),
            "timeout writes to DirectorTimeoutSecs"
        );

        // Completer max tokens (Development tab)
        let max_tokens_field = description.text_write(form::DIRECTOR_MAX_TOKENS_ID);
        assert!(
            max_tokens_field.is_some(),
            "max tokens must have a TextField"
        );
        assert_eq!(
            max_tokens_field,
            Some(crate::settings::TextField::DirectorMaxTokens),
            "max tokens writes to DirectorMaxTokens"
        );

        // Excluded applications (already working, verify it stays)
        let excluded_field = description.text_write(form::EXCLUDED_ID);
        assert!(
            excluded_field.is_some(),
            "excluded apps must have a TextField"
        );
    }

    /// SettingsPatch.set_text commits through the generic text field enum.
    #[test]
    fn patch_commits_non_batched_text_fields() {
        let mut patch = SettingsPatch::default();

        // Wake interval
        let changed = patch.set_text(crate::settings::TextField::DirectorWakeSecs, "300");
        assert!(changed, "wake interval change must succeed");
        assert_eq!(
            patch.director_wake_secs,
            Some("300".to_string()),
            "wake interval must be set in patch"
        );

        // Completer timeout
        let mut patch2 = SettingsPatch::default();
        let changed2 = patch2.set_text(crate::settings::TextField::DirectorTimeoutSecs, "60");
        assert!(changed2, "timeout change must succeed");
        assert_eq!(
            patch2.director_timeout_secs,
            Some("60".to_string()),
            "timeout must be set in patch"
        );
    }

    /// should_update_label_text decides whether a label id needs dynamic updates.
    #[test]
    fn should_update_label_text_preserves_help_hints() {
        assert!(
            should_update_label_text(form::MEMORY_PATH_ID),
            "MEMORY_PATH must be updated dynamically"
        );
        assert!(
            should_update_label_text(form::HOTKEY_ID),
            "HOTKEY must be updated dynamically"
        );
        assert!(
            should_update_label_text(form::PAYLOAD_ID),
            "PAYLOAD must be updated dynamically"
        );
        assert!(
            should_update_label_text(form::HARNESS_STATE_ID),
            "HARNESS_STATE must be updated dynamically"
        );

        assert!(
            !should_update_label_text("director_label"),
            "labels ending with _label must be preserved"
        );
        assert!(
            !should_update_label_text("director_api_key_placeholder"),
            "labels ending with _placeholder must be preserved"
        );
        assert!(
            !should_update_label_text("director_help"),
            "labels ending with _help must be preserved"
        );
        assert!(
            !should_update_label_text("ambient_help"),
            "all _help labels must be preserved"
        );
        assert!(
            !should_update_label_text("composite_help_123"),
            "composite help labels must be preserved"
        );
        assert!(
            !should_update_label_text("composite_help_45"),
            "all composite_help_* labels must be preserved"
        );
        assert!(
            !should_update_label_text("section_heading_0_0"),
            "section heading labels must be preserved"
        );
        assert!(
            !should_update_label_text("section_heading_1_2"),
            "all section_heading_* labels must be preserved"
        );
        assert!(
            !should_update_label_text("section_comment_0_0"),
            "section comment labels must be preserved"
        );
        assert!(
            !should_update_label_text("section_comment_3_1"),
            "all section_comment_* labels must be preserved"
        );
    }

    #[test]
    fn alt_on_background_is_caption() {
        assert_eq!(
            caption_hit_test(true, Hit::Background),
            HTCAPTION as LRESULT
        );
    }

    #[test]
    fn without_alt_stays_client() {
        assert_eq!(
            caption_hit_test(false, Hit::Background),
            HTCLIENT as LRESULT
        );
    }

    #[test]
    fn alt_on_a_control_stays_client() {
        assert_eq!(caption_hit_test(true, Hit::Control), HTCLIENT as LRESULT);
    }

    #[test]
    fn edit_button_and_combo_are_controls() {
        for class in ["Edit", "Button", "ComboBox"] {
            assert_eq!(
                hit_from_win32(class, false),
                Hit::Control,
                "{class} must keep the press"
            );
        }
    }

    #[test]
    fn tab_item_is_a_control_empty_tab_body_is_background() {
        assert_eq!(hit_from_win32("SysTabControl32", true), Hit::Control);
        assert_eq!(hit_from_win32("SysTabControl32", false), Hit::Background);
    }

    #[test]
    fn a_static_label_is_background() {
        assert_eq!(hit_from_win32("Static", false), Hit::Background);
    }

    #[test]
    fn a_nested_dismiss_click_maps_into_the_button_not_the_container() {
        // List host at display_left (= 2*MARGIN); Dismiss at (FIELD_WIDTH-90, 0).
        let container_in_window = POINT {
            x: MARGIN * 2,
            y: 200,
        };
        let dismiss_in_container = POINT {
            x: FIELD_WIDTH - 90,
            y: 0,
        };
        let click_in_window = POINT {
            x: container_in_window.x + dismiss_in_container.x + 4,
            y: container_in_window.y + 8,
        };
        let in_container = map_into_child_client(click_in_window, container_in_window);
        assert_eq!(in_container.x, dismiss_in_container.x + 4);
        assert_eq!(in_container.y, 8);
        let in_button = map_into_child_client(in_container, dismiss_in_container);
        assert_eq!(in_button.x, 4);
        assert_eq!(in_button.y, 8);
        // Window-client y handed to the list host as client y misses a first-row
        // button (ROW_HEIGHT tall at y=0). #460
        assert!(click_in_window.y > ROW_HEIGHT);
    }

    #[test]
    fn a_list_host_is_background_and_not_static() {
        assert_ne!(INSTANCES_LIST_CLASS.to_bytes(), b"STATIC");
        assert_eq!(
            hit_from_win32(INSTANCES_LIST_CLASS.to_str().unwrap(), false),
            Hit::Background
        );
    }
}
