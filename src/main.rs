#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use anyhow::{Context, Result};
use once_cell::sync::OnceCell;
use std::{collections::HashMap, slice};
use utf16_lit::utf16_null;
use windows::{
    core::{h, w, HSTRING, PCWSTR},
    Media::Ocr::OcrEngine,
    Win32::{
        Foundation::{BOOL, HWND, LPARAM, LRESULT, RECT, WPARAM},
        Graphics::Gdi::{GetSysColorBrush, COLOR_MENUBAR},
        System::{
            DataExchange::{AddClipboardFormatListener, RemoveClipboardFormatListener},
            LibraryLoader::{GetModuleHandleW, LoadLibraryW},
        },
        UI::{
            Controls::{
                RichEdit::{EM_GETTEXTLENGTHEX, GETTEXTLENGTHEX, GTL_DEFAULT, MSFTEDIT_CLASS},
                EM_REPLACESEL, EM_SETSEL, WC_COMBOBOXW,
            },
            WindowsAndMessaging::{
                CreateWindowExW, DefWindowProcW, DispatchMessageW, EnumWindows, GetClientRect,
                GetMessageW, GetWindowTextW, IsIconic, PostQuitMessage, RegisterClassW,
                SendMessageW, SetForegroundWindow, ShowWindow, TranslateMessage, CBS_DROPDOWNLIST,
                CBS_HASSTRINGS, CBS_SORT, CB_ADDSTRING, CB_SELECTSTRING, CW_USEDEFAULT,
                ES_AUTOHSCROLL, ES_AUTOVSCROLL, ES_MULTILINE, ES_WANTRETURN, HMENU, MSG, SB_BOTTOM,
                SW_SHOW, WINDOW_EX_STYLE, WINDOW_STYLE, WM_CLIPBOARDUPDATE, WM_CREATE, WM_DESTROY,
                WM_VSCROLL, WNDCLASSW, WS_BORDER, WS_CAPTION, WS_CHILD, WS_EX_STATICEDGE,
                WS_HSCROLL, WS_MINIMIZEBOX, WS_OVERLAPPED, WS_SYSMENU, WS_TABSTOP, WS_VISIBLE,
                WS_VSCROLL,
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

const ID_COMBO: i32 = 5457;
const BUF_SIZE: usize = 8192;

static DISPLAY_NAMES: OnceCell<HashMap<Vec<u16>, Vec<u16>>> = OnceCell::new();
static HWND_RICH_EDIT: OnceCell<HWND> = OnceCell::new();

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
            None,
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
            SendMessageW(hwnd, CB_ADDSTRING, None, LPARAM(h.as_ptr() as isize));
        });

    unsafe {
        SendMessageW(
            hwnd,
            CB_SELECTSTRING,
            None,
            LPARAM(lang.DisplayName()?.as_ptr() as isize),
        )
    };

    Ok(())
}

fn create_richedit(hwnd: HWND) -> Result<()> {
    unsafe { LoadLibraryW(h!("Msftedit.dll"))? };

    let mut rc = RECT::default();
    unsafe { GetClientRect(hwnd, &mut rc)? };

    let hwnd = unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            MSFTEDIT_CLASS,
            None,
            WINDOW_STYLE((ES_MULTILINE | ES_WANTRETURN | ES_AUTOHSCROLL | ES_AUTOVSCROLL) as _)
                | WS_VISIBLE
                | WS_CHILD
                | WS_BORDER
                | WS_TABSTOP
                | WS_VSCROLL
                | WS_HSCROLL,
            0,
            30,
            rc.right,
            rc.bottom - 30,
            hwnd,
            None,
            GetModuleHandleW(None)?,
            None,
        )
    };

    HWND_RICH_EDIT.get_or_init(|| hwnd);

    Ok(())
}

fn create(hwnd: HWND) {
    create_richedit(hwnd).ok();
    create_combobox(hwnd).ok();
    unsafe { AddClipboardFormatListener(hwnd).ok() };
}

fn ocr(hwnd: HWND) -> Result<()> {
    let (width, height, bgra) = clipboard::get()?;

    let mut buf = [0u8; BUF_SIZE];
    let len = ocr::scan(hwnd, width, height, bgra, &mut buf)?;

    let txt = unsafe { slice::from_raw_parts(buf.as_ptr() as *const u16, len / 2) };
    clipboard::set(txt)?;

    let hedit = HWND_RICH_EDIT.get().copied().context("no hwnd.")?;

    // move the caret to the end of the text
    let len = GETTEXTLENGTHEX {
        flags: GTL_DEFAULT,
        codepage: 1200,
    };
    let len = unsafe {
        SendMessageW(
            hedit,
            EM_GETTEXTLENGTHEX,
            WPARAM(&len as *const _ as _),
            None,
        )
        .0 as usize
    };
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

    // scroll to the end of richedit
    unsafe { SendMessageW(hedit, WM_VSCROLL, WPARAM(SB_BOTTOM.0 as _), None) };
    Ok(())
}

fn destroy(hwnd: HWND) {
    unsafe {
        let _ = RemoveClipboardFormatListener(hwnd);
        PostQuitMessage(0);
    }
}

unsafe extern "system" fn enum_win(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let mut buf = [0; 24];
    GetWindowTextW(hwnd, &mut buf);
    if buf.starts_with(TITLE) {
        if lparam.0 > 0 {
            if IsIconic(hwnd).as_bool() {
                let _ = ShowWindow(hwnd, SW_SHOW);
            }
            let _ = SetForegroundWindow(hwnd);
        }
        return false.into();
    }
    true.into()
}

fn is_already_running() -> bool {
    unsafe { EnumWindows(Some(enum_win), None).is_err() }
}

fn set_focus_existing_window() {
    let _ = unsafe { EnumWindows(Some(enum_win), LPARAM(1)) };
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
            None,
            None,
            None,
            None,
        )
    };

    let _ = unsafe { ShowWindow(hwnd, SW_SHOW) };

    let mut msg = MSG::default();

    loop {
        if !unsafe { GetMessageW(&mut msg, None, 0, 0) }.as_bool() {
            break;
        }
        unsafe {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
    Ok(())
}
