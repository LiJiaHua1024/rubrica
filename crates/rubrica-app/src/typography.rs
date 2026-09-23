//! Modeless native typography form. Applying posts a message after the form callback
//! returns, so the reader's layout is never re-entered through a nested dialog loop.

use std::cell::Cell;
use windows::core::PCWSTR;
use windows::Win32::{Foundation::{HWND, LPARAM, LRESULT, WPARAM},
    Graphics::Gdi::{GetStockObject, DEFAULT_GUI_FONT, COLOR_WINDOW},
    System::LibraryLoader::GetModuleHandleW,
    UI::WindowsAndMessaging::*};
use crate::{profiles::{self, Profile, FONT_LABELS, NUMBER_LABELS}, theme::Theme, view::utf16};

pub const APPLIED: u32 = WM_APP + 41;
thread_local! { static OPEN: Cell<Option<HWND>> = const { Cell::new(None) }; }

struct Form {
    owner: HWND,
    name: HWND,
    fields: Vec<HWND>,
    korean: HWND,
    bind: HWND,
}

pub fn route(msg: &MSG) -> bool {
    OPEN.with(|h| h.get()).is_some_and(|h| unsafe {
        (h == msg.hwnd || IsChild(h, msg.hwnd).as_bool()) && IsDialogMessageW(h, msg).as_bool()
    })
}

pub fn show(owner: HWND, theme: &Theme, plain: bool) -> crate::Result<()> {
    if let Some(hwnd) = OPEN.with(|h| h.get()) {
        let _ = unsafe { SetForegroundWindow(hwnd) };
        return Ok(());
    }
    unsafe {
        let class = utf16("Rubrica.Typography");
        let title = utf16("Typography — Rubrica");
        let instance = GetModuleHandleW(None)?;
        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(proc), hInstance: instance.into(),
            hCursor: LoadCursorW(None, IDC_ARROW)?,
            hbrBackground: windows::Win32::Graphics::Gdi::HBRUSH((COLOR_WINDOW.0 + 1) as *mut _),
            lpszClassName: PCWSTR(class.as_ptr()), ..Default::default()
        };
        RegisterClassExW(&wc);
        let hwnd = CreateWindowExW(WS_EX_CONTROLPARENT, PCWSTR(class.as_ptr()), PCWSTR(title.as_ptr()),
            WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU, CW_USEDEFAULT, CW_USEDEFAULT, 720, 620,
            Some(owner), None, Some(instance.into()), None)?;
        let mut state = Box::new(Form { owner, name: HWND::default(), fields: Vec::new(),
            korean: HWND::default(), bind: HWND::default() });
        let build = (|| -> windows::core::Result<()> {
            label(hwnd, "Preset name", 18, 16, 150)?;
            let selected = profiles::selected(plain);
            state.name = child(hwnd, "EDIT", if selected == "Default" || selected == "Book" { "Custom" } else { &selected },
                [178, 14, 504, 25], 100, WS_BORDER | WS_TABSTOP | WINDOW_STYLE(ES_AUTOHSCROLL as u32))?;
            let p = Profile::from_theme(theme);
            for (i, (name, value)) in FONT_LABELS.iter().zip(&p.fonts).enumerate() {
                let y = 54 + i as i32 * 29;
                label(hwnd, name, 18, y + 3, 150)?;
                state.fields.push(child(hwnd, "EDIT", value, [178, y, 504, 25], 110 + i,
                    WS_BORDER | WS_TABSTOP | WINDOW_STYLE(ES_AUTOHSCROLL as u32))?);
            }
            for (i, (name, value)) in NUMBER_LABELS.iter().zip(p.numbers).enumerate() {
                let x = 18 + (i % 2) as i32 * 344;
                let y = 298 + (i / 2) as i32 * 32;
                label(hwnd, name, x, y + 3, 190)?;
                state.fields.push(child(hwnd, "EDIT", &value.to_string(), [x + 198, y, 118, 25], 120 + i,
                    WS_BORDER | WS_TABSTOP | WINDOW_STYLE(ES_AUTOHSCROLL as u32))?);
            }
            state.korean = child(hwnd, "BUTTON", "Keep Korean words together", [18, 436, 330, 26], 130,
                WS_TABSTOP | WINDOW_STYLE(BS_AUTOCHECKBOX as u32))?;
            SendMessageW(state.korean, BM_SETCHECK, Some(WPARAM(usize::from(p.keep_korean_words))), None);
            state.bind = child(hwnd, "BUTTON", "Use this preset for TXT documents", [362, 436, 330, 26], 131,
                WS_TABSTOP | WINDOW_STYLE(BS_AUTOCHECKBOX as u32))?;
            SendMessageW(state.bind, BM_SETCHECK, Some(WPARAM(usize::from(plain))), None);
            label(hwnd, "Use installed font family names. Missing fonts use the fallback families.", 18, 477, 666)?;
            child(hwnd, "BUTTON", "Save and apply", [428, 514, 130, 30], 1, WS_TABSTOP | WINDOW_STYLE(BS_DEFPUSHBUTTON as u32))?;
            child(hwnd, "BUTTON", "Cancel", [570, 514, 112, 30], 2, WS_TABSTOP)?;
            Ok(())
        })();
        if let Err(error) = build { let _ = DestroyWindow(hwnd); return Err(error.into()); }
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, Box::into_raw(state) as isize);
        OPEN.with(|h| h.set(Some(hwnd)));
        let _ = ShowWindow(hwnd, SW_SHOW);
    }
    Ok(())
}

unsafe fn child(parent: HWND, class: &str, title: &str, rect: [i32; 4],
    id: usize, style: WINDOW_STYLE) -> windows::core::Result<HWND> {
    let [x, y, width, height] = rect;
    let (class, title) = (utf16(class), utf16(title));
    let hwnd = unsafe { CreateWindowExW(WINDOW_EX_STYLE(0), PCWSTR(class.as_ptr()), PCWSTR(title.as_ptr()),
        WS_CHILD | WS_VISIBLE | style, x, y, width, height, Some(parent), Some(HMENU(id as *mut _)), None, None)? };
    unsafe { SendMessageW(hwnd, WM_SETFONT, Some(WPARAM(GetStockObject(DEFAULT_GUI_FONT).0 as usize)), Some(LPARAM(1))); }
    Ok(hwnd)
}

unsafe fn label(parent: HWND, title: &str, x: i32, y: i32, width: i32) -> windows::core::Result<()> {
    unsafe { child(parent, "STATIC", title, [x, y, width, 24], 0, WINDOW_STYLE(0))?; }
    Ok(())
}

fn text(hwnd: HWND) -> String {
    let mut chars = vec![0; unsafe { GetWindowTextLengthW(hwnd) }.max(0) as usize + 1];
    let len = unsafe { GetWindowTextW(hwnd, &mut chars) }.max(0) as usize;
    String::from_utf16_lossy(&chars[..len]).trim().to_string()
}

unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let pointer = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as *mut Form;
    if !pointer.is_null() {
        let state = unsafe { &mut *pointer };
        match msg {
            WM_COMMAND if wp.0 & 0xffff == 1 => {
                let mut p = Profile::default();
                for (i, value) in p.fonts.iter_mut().enumerate() { *value = text(state.fields[i]); }
                for (i, value) in p.numbers.iter_mut().enumerate() {
                    *value = text(state.fields[i + 8]).parse().unwrap_or(f32::NAN);
                }
                p.keep_korean_words = unsafe { SendMessageW(state.korean, BM_GETCHECK, None, None) }.0 == 1;
                let name = text(state.name);
                match profiles::save(&name, &p) {
                    Ok(()) => {
                        profiles::select(&name, false);
                        if unsafe { SendMessageW(state.bind, BM_GETCHECK, None, None) }.0 == 1 { profiles::select(&name, true); }
                        let _ = unsafe { PostMessageW(Some(state.owner), APPLIED, WPARAM(0), LPARAM(0)) };
                        let _ = unsafe { DestroyWindow(hwnd) };
                    }
                    Err(error) => {
                        let (error, title) = (utf16(&error), utf16("Typography"));
                        unsafe { MessageBoxW(Some(hwnd), PCWSTR(error.as_ptr()), PCWSTR(title.as_ptr()), MB_OK | MB_ICONERROR); }
                    }
                }
                return LRESULT(0);
            }
            WM_COMMAND if wp.0 & 0xffff == 2 => { let _ = unsafe { DestroyWindow(hwnd) }; return LRESULT(0); }
            WM_NCDESTROY => {
                unsafe { SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0); drop(Box::from_raw(pointer)); }
                OPEN.with(|h| h.set(None));
            }
            _ => {}
        }
    }
    unsafe { DefWindowProcW(hwnd, msg, wp, lp) }
}
