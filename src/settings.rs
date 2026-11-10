//! The settings window: a tab for each group of settings, with a row per
//! setting. Which rows exist, and what kind of control each gets, comes from
//! `schema.rs`; values are read and written by name through `Config`, the
//! same way the settings file is.

use std::cell::{Cell, RefCell};
use std::mem::{size_of, zeroed};
use std::ptr::{null, null_mut};
use std::rc::Rc;

use windows_sys::core::{w, PCWSTR};
use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::UI::Controls::Dialogs::*;
use windows_sys::Win32::UI::Controls::*;
use windows_sys::Win32::UI::HiDpi::*;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

use crate::config::{Config, Rgb};
use crate::lang::t;
use crate::schema::{self, Field, Kind, TABS};
use crate::win::{autostart_enabled, wide, with_app};

// Layout, in 96-dpi pixels; scaled to the monitor's dpi when used.
const WIDTH: i32 = 720;
const MARGIN: i32 = 12;
const TAB_HEIGHT: i32 = 28;
const ROWS_TOP: i32 = MARGIN + TAB_HEIGHT + 16;
const ROW_HEIGHT: i32 = 30;
const LABEL_X: i32 = 24;
const CONTROL_X: i32 = 360;
const CONTROL_WIDTH: i32 = WIDTH - LABEL_X - CONTROL_X;
const BUTTON_HEIGHT: i32 = 28;

const ID_APPLY: usize = 100;
const ID_PREVIEW: usize = 101;
const ID_FILE: usize = 102;
/// A row's main control has this plus the row's index as its id ...
const ID_FIELD_BASE: usize = 1000;
/// ... and the "pick a colour" button of a colour row has this plus it.
const ID_COLOR_BASE: usize = 2000;

struct Row {
    field: &'static Field,
    tab: usize,
    /// The tick box, text box or drop-down holding the value.
    control: HWND,
    /// Its label, unit, or colour button.
    others: Vec<HWND>,
}

struct Dialog {
    hwnd: HWND,
    tabs: HWND,
    rows: Vec<Row>,
    font: HFONT,
}

thread_local! {
    /// The open settings window, if there is one.
    static DIALOG: RefCell<Option<Rc<Dialog>>> = const { RefCell::new(None) };
    /// The "custom colours" of the colour picker, kept while the app runs.
    static CUSTOM_COLORS: Cell<[COLORREF; 16]> = const { Cell::new([0x00FF_FFFF; 16]) };
}

/// The open window. Taken out as a counted reference, never left borrowed,
/// because nearly everything done to a window can call back into `proc`.
fn current() -> Option<Rc<Dialog>> {
    DIALOG.with(|dialog| dialog.borrow().clone())
}

/// The settings window's handle, or null if it isn't open.
pub fn window() -> HWND {
    current().map_or(null_mut(), |dialog| dialog.hwnd)
}

/// Opens the settings window, or brings it to the front if it is open.
pub fn open(hinstance: HINSTANCE) {
    unsafe {
        let existing = window();
        if !existing.is_null() {
            ShowWindow(existing, SW_RESTORE);
            SetForegroundWindow(existing);
            return;
        }
        let Some(config) = with_app(|app| app.config()) else {
            return;
        };

        let classes = INITCOMMONCONTROLSEX {
            dwSize: size_of::<INITCOMMONCONTROLSEX>() as u32,
            dwICC: ICC_TAB_CLASSES | ICC_STANDARD_CLASSES,
        };
        InitCommonControlsEx(&classes);

        // Registering again on a later opening fails harmlessly.
        let mut class: WNDCLASSW = zeroed();
        class.hInstance = hinstance;
        class.lpszClassName = w!("screenSleep.Settings");
        class.lpfnWndProc = Some(proc);
        class.hCursor = LoadCursorW(null_mut(), IDC_ARROW);
        class.hbrBackground = (COLOR_BTNFACE + 1) as usize as HBRUSH;
        class.hIcon = LoadIconW(hinstance, 1 as PCWSTR);
        RegisterClassW(&class);

        // Open on the monitor the cursor is on, sized for that monitor's
        // scaling.
        let mut cursor = POINT { x: 0, y: 0 };
        GetCursorPos(&mut cursor);
        let monitor = MonitorFromPoint(cursor, MONITOR_DEFAULTTOPRIMARY);
        let (mut dpi, mut dpi_y) = (96u32, 96u32);
        GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dpi, &mut dpi_y);
        let px = |pixels: i32| pixels * dpi as i32 / 96;

        let height = ROWS_TOP + schema::max_rows() as i32 * ROW_HEIGHT + 16 + BUTTON_HEIGHT + MARGIN;
        let style = WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX;
        let mut frame = RECT {
            left: 0,
            top: 0,
            right: px(WIDTH),
            bottom: px(height),
        };
        AdjustWindowRectExForDpi(&mut frame, style, 0, WS_EX_CONTROLPARENT, dpi);
        let (frame_width, frame_height) = (frame.right - frame.left, frame.bottom - frame.top);
        let mut info: MONITORINFO = zeroed();
        info.cbSize = size_of::<MONITORINFO>() as u32;
        GetMonitorInfoW(monitor, &mut info);
        let area = info.rcWork;

        let hwnd = CreateWindowExW(
            WS_EX_CONTROLPARENT,
            w!("screenSleep.Settings"),
            wide(t("screenSleep settings")).as_ptr(),
            style,
            (area.left + (area.right - area.left - frame_width) / 2).max(area.left),
            (area.top + (area.bottom - area.top - frame_height) / 2).max(area.top),
            frame_width,
            frame_height,
            null_mut(),
            null_mut(),
            hinstance,
            null(),
        );
        if hwnd.is_null() {
            return;
        }

        let font = CreateFontW(
            -px(12),
            0,
            0,
            0,
            FW_NORMAL as i32,
            0,
            0,
            0,
            DEFAULT_CHARSET as _,
            0,
            0,
            CLEARTYPE_QUALITY as _,
            0,
            w!("Segoe UI"),
        );
        // Creates one control. Positions and sizes are in 96-dpi pixels.
        let child = |class: PCWSTR, text: &str, style: u32, edge: bool, at: (i32, i32, i32, i32), id: usize| {
            let (x, y, width, height) = at;
            let control = CreateWindowExW(
                if edge { WS_EX_CLIENTEDGE } else { 0 },
                class,
                wide(text).as_ptr(),
                WS_CHILD | style,
                px(x),
                px(y),
                px(width),
                px(height),
                hwnd,
                id as HMENU,
                hinstance,
                null(),
            );
            SendMessageW(control, WM_SETFONT, font as WPARAM, 1);
            control
        };

        // Only the strip of tabs is used; the rows sit below it, straight on
        // the window, which keeps their backgrounds matching.
        let tab_style = WS_VISIBLE | WS_TABSTOP;
        let tabs = child(
            w!("SysTabControl32"),
            "",
            tab_style,
            false,
            (MARGIN, MARGIN, WIDTH - 2 * MARGIN, TAB_HEIGHT),
            0,
        );

        let mut rows = Vec::new();
        for (tab_index, tab) in TABS.iter().enumerate() {
            let mut title = wide(t(tab.title));
            let mut item: TCITEMW = zeroed();
            item.mask = TCIF_TEXT;
            item.pszText = title.as_mut_ptr();
            SendMessageW(tabs, TCM_INSERTITEMW, tab_index, &raw const item as LPARAM);

            // Rows on tabs other than the first start out hidden.
            let visible = if tab_index == 0 { WS_VISIBLE } else { 0 };
            let input = visible | WS_TABSTOP;
            for (row_index, field) in tab.fields.iter().enumerate() {
                let y = ROWS_TOP + row_index as i32 * ROW_HEIGHT;
                let id = ID_FIELD_BASE + rows.len();
                let label = |others: &mut Vec<HWND>| {
                    let at = (LABEL_X, y + 4, CONTROL_X - LABEL_X - 8, 20);
                    others.push(child(w!("STATIC"), t(field.label), visible, false, at, 0));
                };
                let mut others = Vec::new();
                let control = match field.kind {
                    Kind::Bool => {
                        let at = (LABEL_X, y, WIDTH - 2 * LABEL_X, 24);
                        child(
                            w!("BUTTON"),
                            t(field.label),
                            input | BS_AUTOCHECKBOX as u32,
                            false,
                            at,
                            id,
                        )
                    }
                    Kind::Number(unit) => {
                        label(&mut others);
                        let unit_at = (CONTROL_X + 88, y + 4, CONTROL_WIDTH - 88, 20);
                        others.push(child(w!("STATIC"), t(unit), visible, false, unit_at, 0));
                        let style = input | (ES_NUMBER | ES_AUTOHSCROLL) as u32;
                        child(w!("EDIT"), "", style, true, (CONTROL_X, y, 80, 24), id)
                    }
                    Kind::Text => {
                        label(&mut others);
                        let style = input | ES_AUTOHSCROLL as u32;
                        child(w!("EDIT"), "", style, true, (CONTROL_X, y, CONTROL_WIDTH, 24), id)
                    }
                    Kind::Color => {
                        label(&mut others);
                        let button_at = (CONTROL_X + 118, y, 40, 24);
                        let button_id = ID_COLOR_BASE + rows.len();
                        let style = input | ES_AUTOHSCROLL as u32;
                        let edit = child(w!("EDIT"), "", style, true, (CONTROL_X, y, 110, 24), id);
                        others.push(child(w!("BUTTON"), "...", input, false, button_at, button_id));
                        edit
                    }
                    Kind::Choice(options) => {
                        label(&mut others);
                        // The height is that of the opened list.
                        let style = input | WS_VSCROLL | CBS_DROPDOWNLIST as u32;
                        let combo = child(w!("COMBOBOX"), "", style, false, (CONTROL_X, y, CONTROL_WIDTH, 300), id);
                        for (_, option) in options() {
                            SendMessageW(combo, CB_ADDSTRING, 0, wide(t(option)).as_ptr() as LPARAM);
                        }
                        combo
                    }
                };
                rows.push(Row {
                    field,
                    tab: tab_index,
                    control,
                    others,
                });
            }
        }

        let y = height - MARGIN - BUTTON_HEIGHT;
        let button = |text: &str, x: i32, width: i32, id: usize, default: bool| {
            let style = WS_VISIBLE | WS_TABSTOP | if default { BS_DEFPUSHBUTTON as u32 } else { 0 };
            child(w!("BUTTON"), t(text), style, false, (x, y, width, BUTTON_HEIGHT), id);
        };
        button("Apply and preview", MARGIN, 190, ID_PREVIEW, false);
        button("Open settings file", MARGIN + 198, 220, ID_FILE, false);
        button("OK", WIDTH - MARGIN - 3 * 88 + 4, 84, IDOK as usize, true);
        button("Cancel", WIDTH - MARGIN - 2 * 88 + 4, 84, IDCANCEL as usize, false);
        button("Apply", WIDTH - MARGIN - 88 + 4, 84, ID_APPLY, false);

        let dialog = Rc::new(Dialog { hwnd, tabs, rows, font });
        fill(&dialog, &config);
        DIALOG.with(|slot| slot.replace(Some(dialog)));
        ShowWindow(hwnd, SW_SHOW);
        SetForegroundWindow(hwnd);
    }
}

/// Puts the current settings into the controls.
unsafe fn fill(dialog: &Dialog, config: &Config) {
    let values = config.values();
    for row in &dialog.rows {
        let value = if row.field.key == schema::AUTOSTART {
            autostart_enabled().to_string()
        } else {
            let found = values.iter().find(|(key, _)| key == row.field.key);
            found.map(|(_, value)| value.clone()).unwrap_or_default()
        };
        match row.field.kind {
            Kind::Bool => {
                let state = if value == "true" { BST_CHECKED } else { BST_UNCHECKED };
                SendMessageW(row.control, BM_SETCHECK, state as WPARAM, 0);
            }
            Kind::Choice(options) => {
                let index = options().iter().position(|(option, _)| *option == value).unwrap_or(0);
                SendMessageW(row.control, CB_SETCURSEL, index, 0);
            }
            Kind::Number(_) | Kind::Text | Kind::Color => {
                SetWindowTextW(row.control, wide(&value).as_ptr());
            }
        }
    }
}

/// A row's value as it now stands in its control, in settings-file form.
unsafe fn read(row: &Row) -> String {
    match row.field.kind {
        Kind::Bool => {
            let checked = SendMessageW(row.control, BM_GETCHECK, 0, 0) == BST_CHECKED as LRESULT;
            checked.to_string()
        }
        Kind::Choice(options) => {
            let index = SendMessageW(row.control, CB_GETCURSEL, 0, 0);
            let chosen = usize::try_from(index)
                .ok()
                .and_then(|index| options().get(index).copied());
            chosen.map_or(String::new(), |(option, _)| option.to_string())
        }
        Kind::Number(_) | Kind::Text | Kind::Color => {
            let mut buffer = [0u16; 1024];
            let length = GetWindowTextW(row.control, buffer.as_mut_ptr(), buffer.len() as i32);
            String::from_utf16_lossy(&buffer[..length.max(0) as usize])
                .trim()
                .to_string()
        }
    }
}

/// Puts what is in the window into effect. If something in it can't be
/// understood, says what and changes nothing. Returns whether it applied.
unsafe fn apply(dialog: &Dialog) -> bool {
    let Some(mut config) = with_app(|app| app.config()) else {
        return false;
    };
    let mut autostart = autostart_enabled();
    let mut problems = Vec::new();
    for row in &dialog.rows {
        let value = read(row);
        if row.field.key == schema::AUTOSTART {
            autostart = value == "true";
        } else if let Err(problem) = config.set(row.field.key, &value) {
            problems.push(format!("{}: {problem}", t(row.field.label)));
        }
    }
    if !problems.is_empty() {
        let text = format!("{}\n\n{}", t("Some settings need fixing:"), problems.join("\n"));
        let title = wide(t("screenSleep settings"));
        MessageBoxW(
            dialog.hwnd,
            wide(&text).as_ptr(),
            title.as_ptr(),
            MB_OK | MB_ICONWARNING,
        );
        return false;
    }
    with_app(|app| app.apply_settings(config, autostart));
    // Show values as they were actually stored, e.g. numbers pulled into range.
    if let Some(config) = with_app(|app| app.config()) {
        fill(dialog, &config);
    }
    true
}

/// Shows the rows of one tab and hides the rest.
unsafe fn show_tab(dialog: &Dialog, tab: usize) {
    for row in &dialog.rows {
        let show = if row.tab == tab { SW_SHOW } else { SW_HIDE };
        ShowWindow(row.control, show);
        for &other in &row.others {
            ShowWindow(other, show);
        }
    }
}

/// Opens the colour picker for a colour row.
unsafe fn pick_color(dialog: &Dialog, row: &Row) {
    let Rgb(r, g, b) = Rgb::parse(&read(row)).unwrap_or(Rgb(0, 0, 0));
    let mut custom = CUSTOM_COLORS.get();
    let mut picker: CHOOSECOLORW = zeroed();
    picker.lStructSize = size_of::<CHOOSECOLORW>() as u32;
    picker.hwndOwner = dialog.hwnd;
    picker.rgbResult = u32::from(r) | u32::from(g) << 8 | u32::from(b) << 16;
    picker.lpCustColors = custom.as_mut_ptr();
    picker.Flags = CC_RGBINIT | CC_FULLOPEN;
    if ChooseColorW(&mut picker) != 0 {
        let c = picker.rgbResult;
        let text = Rgb(c as u8, (c >> 8) as u8, (c >> 16) as u8).to_string();
        SetWindowTextW(row.control, wide(&text).as_ptr());
    }
    CUSTOM_COLORS.set(custom);
}

unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    // Messages that arrive while the window is still being built, or after
    // it has gone, get the default treatment.
    let Some(dialog) = current().filter(|dialog| dialog.hwnd == hwnd) else {
        return DefWindowProcW(hwnd, msg, wparam, lparam);
    };
    match msg {
        WM_COMMAND => {
            let (id, notification) = (wparam & 0xFFFF, (wparam >> 16) & 0xFFFF);
            if notification != BN_CLICKED as usize {
                return 0;
            }
            match id {
                id if id == IDOK as usize => {
                    if apply(&dialog) {
                        DestroyWindow(hwnd);
                    }
                }
                id if id == IDCANCEL as usize => {
                    DestroyWindow(hwnd);
                }
                ID_APPLY => {
                    apply(&dialog);
                }
                ID_PREVIEW => {
                    if apply(&dialog) {
                        with_app(|app| app.preview());
                    }
                }
                ID_FILE => {
                    with_app(|app| app.open_settings_file());
                }
                id if id >= ID_COLOR_BASE => {
                    if let Some(row) = dialog.rows.get(id - ID_COLOR_BASE) {
                        pick_color(&dialog, row);
                    }
                }
                _ => {}
            }
            0
        }
        WM_NOTIFY => {
            let header = &*(lparam as *const NMHDR);
            if header.hwndFrom == dialog.tabs && header.code == TCN_SELCHANGE {
                let tab = SendMessageW(dialog.tabs, TCM_GETCURSEL, 0, 0);
                show_tab(&dialog, tab.max(0) as usize);
            }
            0
        }
        WM_CLOSE => {
            DestroyWindow(hwnd);
            0
        }
        WM_DESTROY => {
            DIALOG.with(|slot| slot.take());
            DeleteObject(dialog.font);
            0
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}
