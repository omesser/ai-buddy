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

use windows_sys::Win32::Foundation::{BOOL, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{
    EnumDisplayMonitors, GetMonitorInfoA, GetStockObject, UpdateWindow, DEFAULT_GUI_FONT, HGDIOBJ,
    HMONITOR, MONITORINFO,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleA;
use windows_sys::Win32::UI::Controls::NMHDR;
use windows_sys::Win32::UI::Controls::{BST_CHECKED, BST_UNCHECKED};
use windows_sys::Win32::UI::Controls::{
    TCIF_TEXT, TCITEMA, TCM_ADJUSTRECT, TCM_GETCURSEL, TCM_INSERTITEMA, WC_TABCONTROLA,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExA, DestroyWindow, GetClientRect, GetDlgItem, GetWindow, GetWindowLongPtrA,
    GetWindowTextA, GetWindowTextLengthA, MessageBoxA, SendMessageA, SendMessageW,
    SetWindowLongPtrA, SetWindowPos, SetWindowTextA, ShowWindow, BM_GETCHECK, BM_SETCHECK,
    BS_AUTOCHECKBOX, BS_PUSHBUTTON, CW_USEDEFAULT, EN_CHANGE, ES_AUTOVSCROLL, ES_MULTILINE,
    ES_PASSWORD, ES_READONLY, ES_WANTRETURN, GWLP_USERDATA, GW_CHILD, GW_HWNDNEXT, IDYES,
    MB_ICONQUESTION, MB_OK, MB_YESNO, SWP_NOZORDER, SW_HIDE, SW_SHOW, WM_CLOSE, WM_COMMAND,
    WM_NOTIFY, WM_SETFONT, WM_SIZE, WNDCLASSA, WS_BORDER, WS_CHILD, WS_EX_CLIENTEDGE,
    WS_OVERLAPPEDWINDOW, WS_TABSTOP, WS_VISIBLE, WS_VSCROLL,
};

use crate::settings::form::{self, FormRow, RowOperation};
use crate::settings::{DirectorDraft, SettingsPatch, SettingsSession, SettingsView};

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
const TCN_SELCHANGE_CODE: u32 = 0xFFFFFDDA_u32.wrapping_sub(1);
const EM_SETCUEBANNER: u32 = 0x1501;
const SS_LEFT: u32 = 0x0;
const CBS_DROPDOWNLIST: u32 = 0x0003;
const CB_ADDSTRING: u32 = 0x0143;
const CB_SETCURSEL: u32 = 0x014E;

thread_local! {
    static WINDOW: RefCell<Option<Arc<SettingsWindow>>> = const { RefCell::new(None) };
}

struct SettingsWindow {
    hwnd: HWND,
    session: Mutex<Option<SettingsSession>>,
    controls: RefCell<HashMap<String, Control>>,
    clear_pending: RefCell<bool>,
    refreshing: RefCell<bool>,
    current_tab: RefCell<usize>,
}

#[derive(Clone)]
enum Control {
    Checkbox(HWND, usize),
    Edit(HWND, usize),
    Label(HWND, usize),
    Button(HWND, usize),
    ComboBox(HWND, usize, Vec<String>),
    InstancesList(HWND, usize),
}

impl SettingsWindow {
    fn new(hwnd: HWND) -> Arc<Self> {
        #[allow(clippy::arc_with_non_send_sync)]
        Arc::new(Self {
            hwnd,
            session: Mutex::new(None),
            controls: RefCell::new(HashMap::new()),
            clear_pending: RefCell::new(false),
            refreshing: RefCell::new(false),
            current_tab: RefCell::new(0),
        })
    }

    fn set_session(&self, session: SettingsSession) {
        *self.session.lock().unwrap() = Some(session);
        self.refresh();
    }

    fn refresh(&self) {
        self.draw(false);
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
                        let text = match id.as_str() {
                            form::MEMORY_PATH_ID => view.memory_path.clone(),
                            form::HOTKEY_ID => view.hide_hotkey.clone(),
                            form::PAYLOAD_ID => view
                                .last_payload
                                .clone()
                                .unwrap_or_else(|| "Nothing sent yet.".to_string()),
                            _ => {
                                if id.ends_with("_label") || id.ends_with("_placeholder") {
                                    continue;
                                }
                                String::new()
                            }
                        };
                        set_window_text(*hwnd, &text);
                    }
                    Control::Button(_, _) => {
                        if id == form::APPLY_ID || id == form::CANCEL_ID {
                            let description = form::describe();
                            let _dirty = self.director_draft(&description).patch(&view).is_some();
                        }
                    }
                    Control::ComboBox(hwnd, _, _options) => {
                        if id == form::CHARACTER_ID || id == form::NEW_CHARACTER_ID {
                            SendMessageA(*hwnd, 0x014B, 0, 0);
                            for (idx, character) in view.installed.iter().enumerate() {
                                let c_str = CString::new(character.as_str()).unwrap();
                                SendMessageA(*hwnd, CB_ADDSTRING, 0, c_str.as_ptr() as LPARAM);
                                if id == form::CHARACTER_ID && character == &view.character {
                                    SendMessageA(*hwnd, CB_SETCURSEL, idx, 0);
                                }
                            }
                        }
                    }
                    Control::InstancesList(hwnd, _) => {
                        if id == form::INSTANCES_ID {
                            self.rebuild_instances_list(*hwnd, &view);
                        }
                    }
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
        }
    }

    fn handle_button_click(&self, control_id: i32) {
        if control_id >= ID_BASE + 5000 {
            self.handle_dismiss((control_id - ID_BASE - 5000) as usize);
            return;
        }
        let id_str = control_id.to_string();
        if let Some(Control::Checkbox(..)) = self.controls.borrow().get(&id_str) {
            self.handle_checkbox_toggle(control_id);
        } else {
            self.handle_operation(control_id);
        }
    }

    fn handle_checkbox_toggle(&self, control_id: i32) {
        let id_str = control_id.to_string();
        let controls = self.controls.borrow();
        if let Some(Control::Checkbox(hwnd, _)) = controls.get(&id_str) {
            let checked = unsafe { SendMessageA(*hwnd, BM_GETCHECK, 0, 0) == BST_CHECKED as isize };

            if let Some(field) = form::describe().bool_write(&id_str) {
                let mut patch = SettingsPatch::default();
                patch.set_bool(field, checked);
                drop(controls);
                self.apply(patch);
            }
        }
    }

    fn handle_operation(&self, control_id: i32) {
        let description = form::describe();
        let id_str = control_id.to_string();

        if let Some(op) = description.operations.get(&id_str) {
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

    fn handle_text_change(&self, control_id: i32) {
        let id_str = control_id.to_string();

        if form::EXCLUDED_ID == id_str {
            let controls = self.controls.borrow();
            if let Some(Control::Edit(hwnd, _)) = controls.get(&id_str) {
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

                if let Some(writes) = form::describe().text_write(form::EXCLUDED_ID) {
                    let mut patch = SettingsPatch::default();
                    if patch.set_text(writes, &text) {
                        self.apply(patch);
                    }
                }
            }
        } else if form::DIRECTOR_BASE_URL_ID == id_str
            || form::DIRECTOR_MODEL_ID == id_str
            || form::DIRECTOR_API_KEY_ID == id_str
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
        unsafe {
            for control in controls.values() {
                let (hwnd, tab_index) = match control {
                    Control::Checkbox(hwnd, tab_index) => (hwnd, tab_index),
                    Control::Edit(hwnd, tab_index) => (hwnd, tab_index),
                    Control::Label(hwnd, tab_index) => (hwnd, tab_index),
                    Control::Button(hwnd, tab_index) => (hwnd, tab_index),
                    Control::ComboBox(hwnd, tab_index, _) => (hwnd, tab_index),
                    Control::InstancesList(hwnd, tab_index) => (hwnd, tab_index),
                };
                ShowWindow(
                    *hwnd,
                    if *tab_index == current_tab {
                        SW_SHOW
                    } else {
                        SW_HIDE
                    },
                );
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
                let label = CreateWindowExA(
                    0,
                    c"STATIC".as_ptr() as *const u8,
                    CString::new(line).unwrap().as_ptr() as *const u8,
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

pub fn show(session: SettingsSession) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{BringWindowToTop, SetForegroundWindow};

    WINDOW.with(|cell| {
        let mut borrow = cell.borrow_mut();
        if let Some(existing) = borrow.as_ref() {
            existing.set_session(session);
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

        window.set_session(session);

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

            for section in tab_def.sections.iter() {
                y += SECTION_GAP;

                for row in &section.rows {
                    match row {
                        FormRow::Checkbox {
                            id, label, help, ..
                        } => {
                            let hwnd = CreateWindowExA(
                                0,
                                c"BUTTON".as_ptr() as *const u8,
                                CString::new(label.as_str()).unwrap().as_ptr() as *const u8,
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
                                .controls
                                .borrow_mut()
                                .insert(id.clone(), Control::Checkbox(hwnd, tab_index));
                            y += ROW_HEIGHT + ROW_GAP;
                            if let Some(help_text) = help {
                                let help_hwnd = CreateWindowExA(
                                    0,
                                    c"STATIC".as_ptr() as *const u8,
                                    CString::new(help_text.as_str()).unwrap().as_ptr() as *const u8,
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
                                WS_CHILD | WS_VISIBLE | WS_TABSTOP | WS_BORDER,
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
                                .controls
                                .borrow_mut()
                                .insert(id.clone(), Control::Edit(hwnd, tab_index));
                            y += ROW_HEIGHT + ROW_GAP;
                            control_id += 1;
                        }
                        FormRow::SecureField { id, label, .. } => {
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
                                WS_CHILD | WS_VISIBLE | WS_TABSTOP | WS_BORDER | ES_PASSWORD as u32,
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
                                .controls
                                .borrow_mut()
                                .insert(id.clone(), Control::Edit(hwnd, tab_index));
                            y += ROW_HEIGHT + ROW_GAP;
                            control_id += 1;
                        }
                        FormRow::Popup {
                            id, label, help, ..
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
                                0,
                                c"COMBOBOX".as_ptr() as *const u8,
                                ptr::null(),
                                WS_CHILD | WS_VISIBLE | WS_TABSTOP | CBS_DROPDOWNLIST,
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
                                .controls
                                .borrow_mut()
                                .insert(id.clone(), Control::ComboBox(hwnd, tab_index, Vec::new()));
                            y += ROW_HEIGHT + ROW_GAP;
                            if let Some(help_text) = help {
                                let help_hwnd = CreateWindowExA(
                                    0,
                                    c"STATIC".as_ptr() as *const u8,
                                    CString::new(help_text.as_str()).unwrap().as_ptr() as *const u8,
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
                        FormRow::List { id, help, .. } => {
                            let container_hwnd = CreateWindowExA(
                                0,
                                c"STATIC".as_ptr() as *const u8,
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
                            if let Some(help_text) = help {
                                let help_hwnd = CreateWindowExA(
                                    0,
                                    c"STATIC".as_ptr() as *const u8,
                                    CString::new(help_text.as_str()).unwrap().as_ptr() as *const u8,
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
                        FormRow::Composite { controls, help, .. } => {
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
                                            .controls
                                            .borrow_mut()
                                            .insert(id.clone(), Control::Edit(hwnd, tab_index));
                                        x += field_width + 8;
                                        control_id += 1;
                                    }
                                    form::CompositeControl::Popup { id } => {
                                        let combo_width = 100;
                                        let hwnd = CreateWindowExA(
                                            0,
                                            c"COMBOBOX".as_ptr() as *const u8,
                                            ptr::null(),
                                            WS_CHILD | WS_VISIBLE | WS_TABSTOP | CBS_DROPDOWNLIST,
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
                                        window.controls.borrow_mut().insert(
                                            id.clone(),
                                            Control::ComboBox(hwnd, tab_index, Vec::new()),
                                        );
                                        x += combo_width + 8;
                                        control_id += 1;
                                    }
                                    form::CompositeControl::Button { id, label, .. } => {
                                        let button_width = 80;
                                        let hwnd = CreateWindowExA(
                                            0,
                                            c"BUTTON".as_ptr() as *const u8,
                                            CString::new(label.as_str()).unwrap().as_ptr()
                                                as *const u8,
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
                                            .controls
                                            .borrow_mut()
                                            .insert(id.clone(), Control::Button(hwnd, tab_index));
                                        x += button_width + 8;
                                        control_id += 1;
                                    }
                                }
                            }
                            y += ROW_HEIGHT + ROW_GAP;
                            if let Some(help_text) = help {
                                let help_hwnd = CreateWindowExA(
                                    0,
                                    c"STATIC".as_ptr() as *const u8,
                                    CString::new(help_text.as_str()).unwrap().as_ptr() as *const u8,
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
                                .controls
                                .borrow_mut()
                                .insert(id.clone(), Control::Edit(hwnd, tab_index));
                            y += MULTILINE_HEIGHT + ROW_GAP;
                            if let Some(help_text) = help {
                                let help_hwnd = CreateWindowExA(
                                    0,
                                    c"STATIC".as_ptr() as *const u8,
                                    CString::new(help_text.as_str()).unwrap().as_ptr() as *const u8,
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
                        FormRow::InspectBlock { id, label, help } => {
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
                            if let Some(help_text) = help {
                                let help_hwnd = CreateWindowExA(
                                    0,
                                    c"STATIC".as_ptr() as *const u8,
                                    CString::new(help_text.as_str()).unwrap().as_ptr() as *const u8,
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

unsafe extern "system" fn window_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    use windows_sys::Win32::UI::WindowsAndMessaging::{WM_CREATE, WM_DESTROY};
    match msg {
        WM_CREATE => 0,
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
}
