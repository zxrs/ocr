#![windows_subsystem = "windows"]

use anyhow::{Context, Result};
use once_cell::sync::OnceCell;
use std::{collections::HashMap, slice};
use utf16_lit::utf16_null;
use windows::{
    core::{HSTRING, PCWSTR},
    w,
    Media::Ocr::OcrEngine,
    Win32::{
        Foundation::{BOOL, HMODULE, HWND, LPARAM, LRESULT, RECT, WPARAM},
        Graphics::Gdi::{GetSysColorBrush, COLOR_MENUBAR},
        System::DataExchange::{AddClipboardFormatListener, RemoveClipboardFormatListener},
        UI::{
            Controls::{EM_REPLACESEL, EM_SETSEL, WC_COMBOBOXW},
            WindowsAndMessaging::{
                CreateWindowExW, DefWindowProcW, DispatchMessageW, EnumWindows, GetClientRect,
                GetDlgItem, GetMessageW, GetWindowTextLengthW, GetWindowTextW, IsIconic,
                PostQuitMessage, RegisterClassW, SendMessageW, SetForegroundWindow, ShowWindow,
                TranslateMessage, CBS_DROPDOWNLIST, CBS_HASSTRINGS, CBS_SORT, CB_ADDSTRING,
                CB_SELECTSTRING, CW_USEDEFAULT, ES_AUTOHSCROLL, ES_AUTOVSCROLL, ES_MULTILINE,
                ES_WANTRETURN, HMENU, MSG, SW_SHOW, WINDOW_EX_STYLE, WINDOW_STYLE,
                WM_CLIPBOARDUPDATE, WM_CREATE, WM_DESTROY, WNDCLASSW, WS_CAPTION, WS_CHILD,
                WS_EX_STATICEDGE, WS_HSCROLL, WS_MINIMIZEBOX, WS_OVERLAPPED, WS_SYSMENU,
                WS_VISIBLE, WS_VSCROLL,
            },
        },
    },
};

#[rustfmt::skip]
macro_rules! c {
    () => {
        concat!("[", file!(), ", line: ", line!(), ", column: ", column!(), "]")
    };
}

const ID_EDIT: i32 = 5456;
const ID_COMBO: i32 = 5457;
const BUF_SIZE: usize = 8192;

static DISPLAY_NAMES: OnceCell<HashMap<Vec<u16>, Vec<u16>>> = OnceCell::new();

mod clipboard;
mod ocr;

const CLASS_NAME: PCWSTR = w!("ocr_win_class_name");
const TITLE: &[u16] = &utf16_null!(concat!(
    env!("CARGO_PKG_NAME"),
    " ver.",
    env!("CARGO_PKG_VERSION")
));

unsafe extern "system" fn wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_CREATE => create(hwnd),
        WM_CLIPBOARDUPDATE => {
            // Adobe PDF reader:    WPARAM(0 | 3 | 5 | 6)
            // Firefox:             WPARAM(6 | 4)
            // Cut & Sketch:        WPARAM(7 | 7 | 4 | 8)
            // Snipping Tool:       WPARAM(3 | 4)
            // IrfanView:           WPARAM(3)
            if wparam.eq(&WPARAM(3))
                || wparam.eq(&WPARAM(4))
                || wparam.eq(&WPARAM(6))
                || wparam.eq(&WPARAM(7))
            {
                ocr(hwnd).ok();
            }
        }
        WM_DESTROY => destroy(hwnd),
        _ => return DefWindowProcW(hwnd, msg, wparam, lparam),
    }
    LRESULT::default()
}

fn create_combobox(hwnd: HWND) -> Result<()> {
    let hwnd = unsafe {
        CreateWindowExW(
            WS_EX_STATICEDGE,
            WC_COMBOBOXW,
            w!(""),
            WINDOW_STYLE((CBS_DROPDOWNLIST | CBS_HASSTRINGS | CBS_SORT) as u32)
                | WS_CHILD
                | WS_VISIBLE
                | WS_VSCROLL,
            1,
            1,
            120,
            200,
            hwnd,
            HMENU(ID_COMBO as isize),
            HMODULE::default(),
            None,
        )
    };
    let engine = OcrEngine::TryCreateFromUserProfileLanguages()?;
    let lang = engine.RecognizerLanguage()?;

    //dbg!(lang.DisplayName()?.as_wide().to_vec());

    DISPLAY_NAMES.get_or_init(|| {
        OcrEngine::AvailableRecognizerLanguages()
            .unwrap()
            .First()
            .unwrap()
            .filter_map(|lang| {
                Some((
                    lang.DisplayName()
                        .ok()?
                        .as_wide()
                        .iter()
                        .chain(Some(&0))
                        .copied()
                        .collect(),
                    lang.LanguageTag()
                        .ok()?
                        .as_wide()
                        .iter()
                        .chain(Some(&0))
                        .copied()
                        .collect(),
                ))
            })
            .collect()
    });

    DISPLAY_NAMES
        .get()
        .context(c!())?
        .keys()
        .filter_map(|k| HSTRING::from_wide(k).ok())
        .for_each(|h| unsafe {
            SendMessageW(
                hwnd,
                CB_ADDSTRING,
                WPARAM::default(),
                LPARAM(h.as_ptr() as isize),
            );
        });

    unsafe {
        SendMessageW(
            hwnd,
            CB_SELECTSTRING,
            WPARAM::default(),
            LPARAM(lang.DisplayName()?.as_ptr() as isize),
        )
    };

    Ok(())
}

fn create(hwnd: HWND) {
    let mut rc = RECT::default();
    unsafe { GetClientRect(hwnd, &mut rc) };

    unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            w!("EDIT"),
            None,
            WS_CHILD
                | WS_VISIBLE
                | WINDOW_STYLE(
                    (ES_WANTRETURN | ES_MULTILINE | ES_AUTOVSCROLL | ES_AUTOHSCROLL) as u32,
                )
                | WS_VSCROLL
                | WS_HSCROLL,
            0,
            30,
            rc.right,
            rc.bottom - 30,
            hwnd,
            HMENU(ID_EDIT as isize),
            HMODULE::default(),
            None,
        )
    };
    create_combobox(hwnd).ok();
    unsafe { AddClipboardFormatListener(hwnd) };
}

fn ocr(hwnd: HWND) -> Result<()> {
    let (width, height, bgra) = clipboard::get()?;

    let mut buf = [0u8; BUF_SIZE];
    let len = ocr::scan(hwnd, width, height, bgra, &mut buf)?;

    let txt = unsafe { slice::from_raw_parts(buf.as_ptr() as *const u16, len / 2) };
    clipboard::set(txt)?;

    let hedit = unsafe { GetDlgItem(hwnd, ID_EDIT) };

    // move the caret to the end of the text
    let len = unsafe { GetWindowTextLengthW(hedit) as usize };
    unsafe { SendMessageW(hedit, EM_SETSEL, WPARAM(len), LPARAM(len as isize)) };

    // insert the text at the new caret position
    unsafe {
        SendMessageW(
            hedit,
            EM_REPLACESEL,
            WPARAM(1),
            LPARAM(txt.as_ptr() as isize),
        )
    };
    Ok(())
}

fn destroy(hwnd: HWND) {
    unsafe {
        RemoveClipboardFormatListener(hwnd);
        PostQuitMessage(0);
    }
}

unsafe extern "system" fn enum_win(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let mut buf = [0; 24];
    GetWindowTextW(hwnd, &mut buf);
    if buf.starts_with(TITLE) {
        if lparam.0 > 0 {
            if IsIconic(hwnd).as_bool() {
                ShowWindow(hwnd, SW_SHOW);
            }
            SetForegroundWindow(hwnd);
        }
        return false.into();
    }
    true.into()
}

fn is_already_running() -> bool {
    unsafe { !EnumWindows(Some(enum_win), LPARAM::default()).as_bool() }
}

fn set_focus_existing_window() {
    unsafe { EnumWindows(Some(enum_win), LPARAM(1)) };
}

fn main() -> Result<()> {
    if is_already_running() {
        set_focus_existing_window();
        return Ok(());
    }

    let wc = WNDCLASSW {
        lpfnWndProc: Some(wnd_proc),
        lpszClassName: CLASS_NAME,
        hbrBackground: unsafe { GetSysColorBrush(COLOR_MENUBAR) },
        ..Default::default()
    };

    unsafe { RegisterClassW(&wc) };

    let hwnd = unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            CLASS_NAME,
            PCWSTR(TITLE.as_ptr()),
            WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_VISIBLE | WS_MINIMIZEBOX,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            600,
            480,
            HWND::default(),
            HMENU::default(),
            HMODULE::default(),
            None,
        )
    };

    unsafe { ShowWindow(hwnd, SW_SHOW) };

    let mut msg = MSG::default();

    loop {
        if !unsafe { GetMessageW(&mut msg, HWND::default(), 0, 0) }.as_bool() {
            break;
        }
        unsafe {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
    Ok(())
}
