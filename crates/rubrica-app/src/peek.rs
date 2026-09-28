//! The spacebar peek: a background service that shows a file in a quick window.
//!
//! `rubrica-app --peek` runs a small windowless process beside the reader. It holds a
//! low-level keyboard hook, and when space is pressed with a document selected in
//! Explorer -- or on the desktop -- it lays that file out with the same engine the
//! reader uses and raises a topmost window that never takes the focus. The selection
//! stays in the folder where it was, so the arrow keys keep moving it and the preview
//! follows; space again, Escape, or releasing a long-held space puts it away.
//!
//! The judgement of what deserves a preview lives in the hook, and it is built to be
//! nearly free: a key code, a handful of `GetAsyncKeyState` reads, one class name.
//! Anything slower -- the COM walk through the shell, the file read, the typesetting
//! -- happens on the service's window thread after a `PostMessage`, where a few
//! hundred milliseconds cost nothing because no key is waiting on them.

use std::collections::HashMap;
use std::os::windows::ffi::OsStringExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use windows::core::{w, GUID, Interface, PCWSTR};
use windows::Win32::Foundation::{
    GetLastError, ERROR_ALREADY_EXISTS, ERROR_SUCCESS, HWND, LRESULT, LPARAM, POINT,
    RECT, WPARAM,
};
use windows::Win32::Graphics::Direct2D::Common::{
    D2D1_ALPHA_MODE_PREMULTIPLIED, D2D1_PIXEL_FORMAT, D2D_RECT_F, D2D_SIZE_U,
};
use windows::Win32::Graphics::Direct2D::{
    D2D1CreateFactory, ID2D1Factory, ID2D1HwndRenderTarget, ID2D1RenderTarget,
    ID2D1SolidColorBrush, D2D1_ANTIALIAS_MODE_PER_PRIMITIVE,
    D2D1_BITMAP_INTERPOLATION_MODE_LINEAR, D2D1_FACTORY_TYPE_SINGLE_THREADED,
    D2D1_FEATURE_LEVEL_DEFAULT, D2D1_HWND_RENDER_TARGET_PROPERTIES, D2D1_PRESENT_OPTIONS_NONE,
    D2D1_RENDER_TARGET_PROPERTIES, D2D1_RENDER_TARGET_TYPE_DEFAULT,
    D2D1_RENDER_TARGET_USAGE_NONE, D2D1_TEXT_ANTIALIAS_MODE_CLEARTYPE,
};
use windows::Win32::Graphics::DirectWrite::{
    DWRITE_GLYPH_RUN, DWRITE_MEASURING_MODE_NATURAL,
};
use windows::Win32::Graphics::Dwm::{
    DwmExtendFrameIntoClientArea, DwmSetWindowAttribute, DWMWA_USE_IMMERSIVE_DARK_MODE,
    DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND,
};
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM;
use windows::Win32::Graphics::Gdi::{
    BeginPaint, EndPaint, GetMonitorInfoW, InvalidateRect, MonitorFromPoint, PAINTSTRUCT,
    MONITORINFO, MONITOR_DEFAULTTONEAREST,
};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, IDataObject, IServiceProvider, CLSCTX_LOCAL_SERVER,
    COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE, DVASPECT_CONTENT, FORMATETC,
    TYMED_HGLOBAL,
};
use windows::Win32::System::Diagnostics::Debug::OutputDebugStringW;
use windows::Win32::System::LibraryLoader::{GetModuleFileNameW, GetModuleHandleW};
use windows::Win32::System::Ole::{CF_HDROP, ReleaseStgMedium};
use windows::Win32::System::Registry::{
    RegDeleteKeyValueW, RegGetValueW, RegSetKeyValueW, HKEY_CURRENT_USER, REG_SZ, RRF_RT_REG_SZ,
};
use windows::Win32::System::Threading::{
    CreateMutexW, CreateProcessW, PROCESS_INFORMATION, STARTUPINFOW,
    CREATE_NEW_PROCESS_GROUP, DETACHED_PROCESS,
};
use windows::Win32::System::Variant::{VARIANT, VARIANT_0, VARIANT_0_0, VARIANT_0_0_0, VT_I4};
use windows::Win32::UI::Accessibility::{SetWinEventHook, UnhookWinEvent, HWINEVENTHOOK};
use windows::Win32::UI::Controls::MARGINS;
use windows::Win32::UI::HiDpi::{
    GetDpiForWindow, SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, VIRTUAL_KEY, VK_CONTROL, VK_DOWN, VK_END, VK_ESCAPE,
    VK_F5, VK_HOME, VK_LEFT, VK_LWIN, VK_MENU, VK_NEXT, VK_PRIOR, VK_RETURN, VK_RIGHT, VK_RWIN,
    VK_SHIFT, VK_SPACE, VK_TAB, VK_UP,
};
use windows::Win32::UI::Shell::{
    DragQueryFileW, ShellExecuteW, Shell_NotifyIconW, IShellBrowser, IShellWindows,
    IWebBrowser2, HDROP, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NOTIFYICONDATAW,
    SID_STopLevelBrowser, SWC_DESKTOP, SWFO_NEEDDISPATCH,
    SVGIO_SELECTION,
};
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CallNextHookEx, CreatePopupMenu, CreateWindowExW, CW_USEDEFAULT,
    CREATESTRUCTW, DefWindowProcW, DestroyMenu, DestroyWindow, DispatchMessageW,
    FindWindowExW, GetClassNameW, GetClientRect, GetCursorPos, GetForegroundWindow,
    GetMessageW, GetGUIThreadInfo, GetShellWindow, GetWindowLongPtrW, GetWindowThreadProcessId,
    IsWindowVisible,
    FindWindowW, KillTimer, LoadCursorW, LoadIconW, PostMessageW, PostQuitMessage, RegisterClassExW,
    RegisterWindowMessageW, SetForegroundWindow, SetTimer, SetWindowLongPtrW, SetWindowsHookExW,
    SetWindowPos, ShowWindow, TrackPopupMenu, TranslateMessage, UnhookWindowsHookEx,
    WindowFromPoint,
    EVENT_SYSTEM_FOREGROUND, GWLP_USERDATA, GUITHREADINFO, GUITHREADINFO_FLAGS, HWND_TOPMOST,
    IDC_ARROW, IDI_APPLICATION, KBDLLHOOKSTRUCT, LLKHF_INJECTED,
    MA_NOACTIVATE,
    MENU_ITEM_FLAGS, MF_CHECKED, MF_SEPARATOR, MF_STRING, MSG, MSLLHOOKSTRUCT, SW_HIDE, SW_SHOWNOACTIVATE, SW_SHOWNORMAL,
    SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, TPM_RETURNCMD, TPM_RIGHTBUTTON,
    WINEVENT_OUTOFCONTEXT, WNDCLASSEXW, WH_KEYBOARD_LL, WH_MOUSE_LL, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST,
    WS_POPUP, WM_APP, WM_DESTROY, WM_ERASEBKGND, WM_KEYDOWN, WM_LBUTTONDOWN, WM_MBUTTONDOWN,
    WM_MOUSEACTIVATE, WM_MOUSEWHEEL, WM_NCCREATE,
    WM_PAINT, WM_RBUTTONDOWN, WM_RBUTTONUP, WM_SYSKEYDOWN, WM_TIMER, WHEEL_DELTA,
};
use windows_numerics::{Matrix3x2, Vector2};
use crate::theme::{ColorRole, Theme};
use crate::view::{build_ops, d2d, glyph_origin, paint_run, system_prefers_dark, utf16, Op, Objects, Palette};
use crate::{hyphen, images, instance, profiles, reading, settings};

/// Messages the hook posts to the service window. The hook itself never touches
/// anything slower than a Win32 query, so every decision that costs time is made
/// here, in the window thread, where latency is invisible.
const WM_APP_SPACE: u32 = WM_APP + 51;
const WM_APP_RELEASE: u32 = WM_APP + 52;
const WM_APP_ESCAPE: u32 = WM_APP + 53;
const WM_APP_OPEN: u32 = WM_APP + 54;
const WM_APP_RELOAD: u32 = WM_APP + 55;
const WM_APP_TRAY: u32 = WM_APP + 56;
/// The reader's menu turning its peek switch off sends this to the service window.
const WM_APP_QUIT: u32 = WM_APP + 57;
/// The foreground moving away under a preview that lives only as long as its
/// source has the focus: posted by the foreground hook, which hears the change
/// the moment it happens instead of waiting for the next poll.
const WM_APP_FOCUS: u32 = WM_APP + 58;

/// A space held at least this long is a glance: releasing it closes the preview.
/// A quicker tap is a toggle instead -- press to open, press again to close -- which
/// is how a preview is read for longer than a glance. A finger's own tap rarely
/// outlives 250 milliseconds, so half a second -- the number long-press has settled
/// on everywhere else -- says "held" without ever catching a tap on its way up.
const HOLD_TO_CLOSE: Duration = Duration::from_millis(500);

/// What a press of the space bar means, as the reader's menu states it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SpaceMode {
    /// Press to show, press again to hide; releasing never hides.
    Tap,
    /// Hold to show; releasing hides, however briefly the key was held down.
    Hold,
    /// Both at once: a quick tap toggles, a held key hides on its release.
    #[default]
    Mixed,
}

/// The space bar's meaning, as it was last written down. A mode absent from the
/// registry is no choice that was taken away -- it is the reader who has never
/// opened the menu, and the mixed answer is what they have always had.
pub fn space_mode() -> SpaceMode {
    match settings::plain_word("PeekSpace") {
        Some(0) => SpaceMode::Tap,
        Some(1) => SpaceMode::Hold,
        _ => SpaceMode::Mixed,
    }
}

pub fn record_space_mode(mode: SpaceMode) {
    let value = match mode {
        SpaceMode::Tap => 0,
        SpaceMode::Hold => 1,
        SpaceMode::Mixed => 2,
    };
    settings::record_plain_word("PeekSpace", value);
}

/// Whether a preview lives only while the folder that opened it keeps the focus.
pub fn focus_close() -> bool {
    settings::plain_word("PeekFocusClose") == Some(1)
}

pub fn record_focus_close(on: bool) {
    settings::record_plain_word("PeekFocusClose", u32::from(on));
}

/// Whether a click on the preview window itself closes it as well. Off by
/// default: a preview is what a user clicks away to, and a document being read
/// is not dismissed by the click that chose to read it.
pub fn click_close() -> bool {
    settings::plain_word("PeekClickClose") == Some(1)
}

pub fn record_click_close(on: bool) {
    settings::record_plain_word("PeekClickClose", u32::from(on));
}

/// A key outside the preview vocabulary starts a cooldown that silences space for a
/// moment: a word typed into the rename box should not end in a preview an instant
/// after the typing stopped. Cleared when the foreground window changes.
const INVALID_KEY_COOLDOWN: Duration = Duration::from_secs(1);

/// While the preview is up, the selection in the folder is watched on this cadence.
/// The arrow keys are left for Explorer to answer -- moving the selection is what
/// they are for -- and the preview simply notices what moved.
const SELECTION_POLL_MS: u32 = 500;

/// The bar above the page, and the padding inside it, in the page's own units.
const BAR_H: f32 = 42.0;
const BAR_PAD: f32 = 14.0;

const TRAY_ID: u32 = 1;
const POLL_TIMER: usize = 1;
/// The service's own beat, on the message-only window rather than the preview's, and
/// armed for as long as the service runs. A hook the system takes away is never
/// announced, and the state it was maintaining is the only casualty the service can
/// actually notice, so something has to keep asking.
const SERVICE_TIMER: usize = 2;
const SERVICE_TICK_MS: u32 = 1000;
const MENU_AUTOSTART: usize = 100;
const MENU_EXIT: usize = 101;

const SERVICE_CLASS: &str = "Rubrica.Peek";
const PREVIEW_CLASS: &str = "Rubrica.PeekView";

// ---------------------------------------------------------------- entry point

/// Run the peek service until its tray menu says to stop. A second `--peek` launch
/// finds the mutex already taken and leaves quietly: one watcher per session.
pub fn daemon() -> crate::Result<()> {
    if acquire().is_none() {
        return Ok(());
    }
    unsafe {
        // The preview is positioned on the monitor the pointer is over, and laid out
        // for that monitor's DPI, both of which need the process to have said so
        // before the first window exists.
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE);
    }

    let hwnd = unsafe { create_service_window() }?;
    // The documented home for a hook that never leaves the process is user32.
    let hook = unsafe {
        SetWindowsHookExW(
            WH_KEYBOARD_LL,
            Some(hook_proc),
            Some(GetModuleHandleW(None)?.into()),
            0,
        )
        .map_err(|e| -> crate::Error { format!("keyboard hook: {e}").into() })?
    };
    // The mouse watches for a leaving the foreground cannot hear: a preview of a
    // desktop icon sits on the desktop, which is already the foreground of its own
    // click -- a click on it moves the foreground nowhere. The click itself is
    // the leaving instead.
    let mouse = unsafe {
        SetWindowsHookExW(
            WH_MOUSE_LL,
            Some(mouse_proc),
            Some(GetModuleHandleW(None)?.into()),
            0,
        )
        .map_err(|e| -> crate::Error { format!("mouse hook: {e}").into() })?
    };
    // A foreground change ends the invalid-key cooldown: the window the typing
    // happened in is gone, and the next space is a fresh request.
    let event_hook = unsafe {
        SetWinEventHook(
            EVENT_SYSTEM_FOREGROUND,
            EVENT_SYSTEM_FOREGROUND,
            None,
            Some(foreground_changed),
            0,
            0,
            WINEVENT_OUTOFCONTEXT,
        )
    };
    unsafe { add_tray_icon(hwnd) };

    let mut service = Box::new(Service {
        window: hwnd,
        space_down: false,
        hold_started: None,
        invalid_at: None,
        shown: false,
        vocabulary_down: None,
        peek: None,
    });
    SERVICE.with(|slot| slot.set(Some(&mut *service as *mut Service)));
    unsafe { SetWindowLongPtrW(hwnd, GWLP_USERDATA, &mut *service as *mut Service as isize) };
    unsafe { let _ = SetTimer(Some(hwnd), SERVICE_TIMER, SERVICE_TICK_MS, None); }

    unsafe {
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }

    SERVICE.with(|slot| slot.set(None));
    unsafe {
        let _ = KillTimer(Some(hwnd), SERVICE_TIMER);
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
    }
    drop(service);
    unsafe {
        if !event_hook.is_invalid() {
            let _ = UnhookWinEvent(event_hook);
        }
        let _ = UnhookWindowsHookEx(hook);
        let _ = UnhookWindowsHookEx(mouse);
        let _ = Shell_NotifyIconW(NIM_DELETE, &tray_icon(hwnd));
    }
    Ok(())
}

/// One peek service per session, the same bargain the reader makes. The handle is
/// deliberately left open -- closing it would release the mutex, and a raw `HANDLE`
/// is `Copy`, so letting it fall out of scope closes nothing -- and the system lets
/// the mutex go if the process dies, so a crashed watcher never blocks the next one.
/// `None` says another watcher is already here.
fn acquire() -> Option<()> {
    let _handle = unsafe { CreateMutexW(None, false, w!("Local\\Rubrica.Peek")) }.ok()?;
    if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
        return None;
    }
    Some(())
}

/// Whether a peek service is watching this session, judged by the window it is
/// named by: the mutex alone would say yes to a service still starting up, and a
/// window that answers is one that can be messaged.
pub fn is_running() -> bool {
    let class = utf16(SERVICE_CLASS);
    unsafe { FindWindowW(PCWSTR(class.as_ptr()), PCWSTR::null()) }.is_ok()
}

/// Ask a running service to end itself. A service that is not there stays not
/// there, and one that is takes its tray icon with it through `WM_DESTROY`.
pub fn request_exit() {
    let class = utf16(SERVICE_CLASS);
    unsafe {
        let Ok(hwnd) = FindWindowW(PCWSTR(class.as_ptr()), PCWSTR::null()) else { return };
        let _ = PostMessageW(Some(hwnd), WM_APP_QUIT, WPARAM(0), LPARAM(0));
    }
}

/// Start the service as its own detached process. The reader's lifetime is the
/// reader's -- windows close, sessions end -- and the watcher must outlive every
/// one of them, so it is not a thread of the reader's but a program of its own.
/// A service already running is left alone: the mutex makes the launch a no-op.
pub fn ensure_running() {
    if is_running() {
        return;
    }
    let mut exe = [0u16; 1024];
    let len = unsafe { GetModuleFileNameW(None, &mut exe) } as usize;
    let command = format!("\"{}\" --peek", String::from_utf16_lossy(&exe[..len]));
    let mut wide: Vec<u16> = command.encode_utf16().chain(std::iter::once(0)).collect();
    unsafe {
        let si = STARTUPINFOW {
            cb: std::mem::size_of::<STARTUPINFOW>() as u32,
            ..Default::default()
        };
        let mut pi = PROCESS_INFORMATION::default();
        let started = CreateProcessW(
            None,
            Some(windows::core::PWSTR(wide.as_mut_ptr())),
            None,
            None,
            false,
            DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP,
            None,
            None,
            &si,
            &mut pi,
        );
        if let Err(error) = started {
            eprintln!("peek: {error}");
        }
    }
}

thread_local! {
    /// The hook proc runs on this thread -- a low-level hook is delivered through the
    /// installing thread's message loop -- so the service state is a thread-local
    /// rather than a lookup through a window.
    static SERVICE: std::cell::Cell<Option<*mut Service>> = const { std::cell::Cell::new(None) };
}

struct Service {
    window: HWND,
    /// The physical space key is down; set once per press, so the repeats a held key
    /// generates are swallowed without re-triggering anything.
    space_down: bool,
    hold_started: Option<Instant>,
    /// The last non-preview key seen, for the cooldown.
    invalid_at: Option<Instant>,
    /// Mirrored from the preview window so the hook can ask "is it up" in O(1).
    shown: bool,
    /// The virtual key whose press the hook swallowed for the preview and whose
    /// release has not come back yet. The release is swallowed wherever the focus
    /// has moved since: a release the foreground never saw a press for is a
    /// keystroke of its own.
    vocabulary_down: Option<u32>,
    peek: Option<Box<Peek>>,
}

unsafe fn create_service_window() -> windows::core::Result<HWND> {
    unsafe {
        let class = utf16(SERVICE_CLASS);
        let instance = GetModuleHandleW(None)?;
        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(service_proc),
            hInstance: instance.into(),
            lpszClassName: PCWSTR(class.as_ptr()),
            ..Default::default()
        };
        RegisterClassExW(&wc);
        // A message-only window: never in a taskbar, never in Alt-Tab, never painted.
        // It exists to receive what the hook posts and to own the tray icon.
        CreateWindowExW(
            Default::default(),
            PCWSTR(class.as_ptr()),
            PCWSTR::null(),
            Default::default(),
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            0,
            0,
            None,
            None,
            Some(instance.into()),
            None,
        )
    }
}

unsafe extern "system" fn service_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    // The taskbar was reborn, so must the icon be.
    if msg == taskbar_created() {
        unsafe { add_tray_icon(hwnd) };
        return LRESULT(0);
    }
    let raw = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) };
    if raw == 0 {
        return unsafe { DefWindowProcW(hwnd, msg, wp, lp) };
    }
    let service = unsafe { &mut *(raw as *mut Service) };
    match msg {
        WM_APP_SPACE => {
            let visible = service
                .peek
                .as_ref()
                .is_some_and(|p| unsafe { IsWindowVisible(p.hwnd) }.as_bool());
            match space_asks(service.shown, visible) {
                // The modes differ exactly here. A tap and the mix both treat a
                // second press as the toggle's other half; hold-to-preview keeps
                // the window up for as long as the key is down, and its press on
                // a standing preview is nothing at all -- the release closes it.
                Ask::Close if space_mode() != SpaceMode::Hold => service.close_preview(),
                Ask::Close => {}
                Ask::Open => {
                    if let Some(path) = unsafe { request_preview() } {
                        service.show_preview(path);
                    }
                }
                Ask::Reopen => {
                    trace("peek: a preview was believed up but no window was there; settling it");
                    service.close_preview();
                    if let Some(path) = unsafe { request_preview() } {
                        service.show_preview(path);
                    }
                }
            }
            LRESULT(0)
        }
        WM_APP_RELEASE => {
            // The hook forwards every release of a key it swallowed; what a
            // release means is the mode's to say. Hold hides on any release,
            // the mix hides a long one, and a tap has already done its work
            // at keydown, so its release is only the finger leaving the key.
            let held_long = service
                .hold_started
                .take()
                .is_some_and(|at| at.elapsed() >= HOLD_TO_CLOSE);
            match space_mode() {
                SpaceMode::Hold if service.shown => service.close_preview(),
                SpaceMode::Mixed if service.shown && held_long => service.close_preview(),
                _ => {}
            }
            LRESULT(0)
        }
        WM_APP_ESCAPE => {
            service.close_preview();
            LRESULT(0)
        }
        WM_APP_OPEN => {
            let mut opened = false;
            if let Some(peek) = service.peek.as_ref() {
                if let Some(path) = peek.path().map(Path::to_owned) {
                    if wp.0 == 1 {
                        // Ctrl+Enter: the reader the user already has open, if it is
                        // running, gets the file as if it had been double-clicked;
                        // otherwise this same program opens it itself, whatever the
                        // file type's default program is.
                        opened = open_in_reader(&path);
                    } else {
                        opened = open_with_default(&path);
                    }
                }
            }
            // The preview goes when something takes the document off it. When nothing
            // did, it stays: it is the page the reader was still reading, and losing it
            // to a failed hand-off is not something anybody asked for.
            if opened {
                service.close_preview();
            }
            LRESULT(0)
        }
        WM_APP_RELOAD => {
            if let Some(peek) = service.peek.as_mut() {
                peek.refresh();
            }
            LRESULT(0)
        }
        // In the icon's original protocol the mouse message rides in lParam, with
        // the icon's own id in wParam.
        // The reader's own switch, turned off: the same exit its tray menu offers,
        // arriving as a message instead of a menu pick.
        WM_APP_QUIT => {
            let _ = unsafe { DestroyWindow(hwnd) };
            LRESULT(0)
        }
        // The foreground left the preview's source while a preview stood that
        // lives only as long as that source holds the focus: the same close the
        // poll would have reached, only the moment the move happened.
        WM_APP_FOCUS => {
            let moved = service
                .peek
                .as_ref()
                .and_then(|p| p.source)
                .is_some_and(|src| unsafe { GetForegroundWindow() } != src);
            if service.shown && moved && focus_close() {
                service.close_preview();
            }
            LRESULT(0)
        }
        WM_TIMER if wp.0 == SERVICE_TIMER => {
            // The one place the service asks the keyboard what it still thinks it is
            // holding. A release the hook never saw leaves a space believed down, and
            // from then on every press reads as a repeat from a key nobody is holding --
            // so the preview answers to nothing at all, for good. The hardware is the
            // truth here, and it costs one call to ask.
            //
            // This is also the only thing that heals a hook the system took away: a
            // low-level hook stops being called the moment its thread stops answering,
            // and nothing says so. The flag it was maintaining simply stays put.
            if service.space_down && !pressed(VK_SPACE) {
                trace("peek: a space the hook never saw released is down no longer");
                service.space_down = false;
                service.hold_started = None;
            }
            LRESULT(0)
        }
        WM_APP_TRAY if lp.0 as u32 == WM_RBUTTONUP => {
            service.tray_menu();
            LRESULT(0)
        }
        WM_DESTROY => {
            unsafe {
                let _ = Shell_NotifyIconW(NIM_DELETE, &tray_icon(hwnd));
                PostQuitMessage(0);
            }
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wp, lp) },
    }
}

impl Service {
    fn show_preview(&mut self, path: PathBuf) {
        if self.peek.is_none() {
            match Peek::new() {
                Ok(peek) => self.peek = Some(peek),
                // A peek window that cannot be built is not retried for every key
                // afterwards: the service keeps running and simply never answers.
                Err(error) => {
                    eprintln!("peek window: {error}");
                    trace("peek: the preview window could not be built");
                    return;
                }
            }
        }
        // The flag follows the reveal itself. A file that could not be read leaves
        // no window standing, and a flag that says one does turns the next press
        // into the close of a preview nobody can see -- the service then needs two
        // presses to answer one question.
        if let Some(peek) = self.peek.as_mut() {
            self.shown = peek.show(&path);
        }
    }

    fn close_preview(&mut self) {
        if let Some(peek) = self.peek.as_mut() {
            peek.close();
        }
        self.shown = false;
    }

    fn tray_menu(&mut self) {
        unsafe {
            let Ok(menu) = CreatePopupMenu() else { return };
            let lang = crate::settings::current_language();
            let check = if autostart_enabled() { MF_CHECKED } else { MENU_ITEM_FLAGS(0) };
            let label = utf16(crate::i18n::t(lang, crate::i18n::Key::TrayLaunchAtSignIn));
            let _ = AppendMenuW(menu, check | MF_STRING, MENU_AUTOSTART, PCWSTR(label.as_ptr()));
            let _ = AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null());
            let exit = utf16(crate::i18n::t(lang, crate::i18n::Key::TrayExit));
            let _ = AppendMenuW(menu, MF_STRING, MENU_EXIT, PCWSTR(exit.as_ptr()));
            let mut pt = POINT::default();
            let _ = GetCursorPos(&mut pt);
            let _ = SetForegroundWindow(self.window);
            // TPM_RETURNCMD answers in the BOOL's own slot, which the API has used
            // as an id since menus had ids.
            let choice = TrackPopupMenu(
                menu,
                TPM_RETURNCMD | TPM_RIGHTBUTTON,
                pt.x,
                pt.y,
                None,
                self.window,
                None,
            );
            let _ = DestroyMenu(menu);
            match choice.0 as usize {
                MENU_AUTOSTART => {
                    if autostart_enabled() {
                        clear_autostart();
                    } else {
                        set_autostart();
                    }
                }
                MENU_EXIT => {
                    let _ = DestroyWindow(self.window);
                }
                _ => {}
            }
        }
    }
}

/// The close the preview window itself asks for -- a poll noticing the focus
/// left, or a click landing on a preview that lives no longer than the focus.
/// The service owns the shown flag and the timer, so the close goes through it
/// rather than around it: a preview closed any other way leaves the service
/// believing one still stands, and the next space press closes it again instead
/// of showing a new one.
fn close_preview_service() {
    if let Some(service) = SERVICE.with(|slot| slot.get()) {
        unsafe { (*service).close_preview() };
    }
}

// ---------------------------------------------------------------- the hook

/// The service window to tell about a click that leaves, and none when nothing
/// leaves: no preview stands, the switch is off, or the click landed on the
/// preview itself while the option that allows that is off. A click on the
/// preview belongs to it -- a document being read is not dismissed by the click
/// that chose to read it -- but the option exists for those who want it gone.
fn leaving_window(pt: POINT) -> Option<HWND> {
    let service = SERVICE.with(|slot| slot.get())?;
    let service = unsafe { &*service };
    if !service.shown || !focus_close() {
        return None;
    }
    let on_preview = service
        .peek
        .as_ref()
        .is_some_and(|p| unsafe { WindowFromPoint(pt) } == p.hwnd);
    if on_preview && !click_close() {
        return None;
    }
    Some(service.window)
}

/// The mouse half of the watching. A preview that lives only while its source has
/// the focus hears the foreground move, but the foreground does not always move:
/// the desktop, which is the source of a preview shown from its own icons, is
/// already the foreground of the click a user lands on it. The click is the
/// leaving itself, whatever the foreground says -- but the wheel is not, since it
/// rolls a long document under the pointer: only a button down departs.
unsafe extern "system" fn mouse_proc(code: i32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    unsafe {
        if code < 0 {
            return CallNextHookEx(None, code, wp, lp);
        }
        let msg = wp.0 as u32;
        if msg == WM_LBUTTONDOWN || msg == WM_MBUTTONDOWN || msg == WM_RBUTTONDOWN {
            let click = &*(lp.0 as *const MSLLHOOKSTRUCT);
            if let Some(window) = leaving_window(click.pt) {
                let _ = PostMessageW(Some(window), WM_APP_ESCAPE, WPARAM(0), LPARAM(0));
            }
        }
        CallNextHookEx(None, code, wp, lp)
    }
}

/// The vocabulary a preview answers to: the keys that ask for one, the keys the
/// preview itself handles, and the modifiers, whose presses are part of every
/// gesture rather than typing. Everything else starts the cooldown.
fn preview_vocabulary(vk: u32) -> bool {
    let vk = VIRTUAL_KEY(vk as u16);
    vk == VK_SPACE
        || vk == VK_ESCAPE
        || vk == VK_RETURN
        || vk == VK_LEFT
        || vk == VK_UP
        || vk == VK_RIGHT
        || vk == VK_DOWN
        || vk == VK_PRIOR
        || vk == VK_NEXT
        || vk == VK_HOME
        || vk == VK_END
        || vk == VK_F5
        || vk == VK_SHIFT
        || vk == VK_CONTROL
        || vk == VK_MENU
        || vk == VK_LWIN
        || vk == VK_RWIN
        || vk == VK_TAB
}

fn pressed(vk: VIRTUAL_KEY) -> bool {
    (unsafe { GetAsyncKeyState(vk.0 as i32) } as u16) & 0x8000 != 0
}

/// One line of diagnosis, sent to whoever is listening and nowhere else: a debug
/// string costs nothing when nobody reads it, and on the day a preview refuses to
/// appear it says which guard said no. Nothing the user sees carries it.
fn trace(note: &str) {
    let line: Vec<u16> = note.encode_utf16().chain(std::iter::once(0)).collect();
    unsafe { OutputDebugStringW(PCWSTR(line.as_ptr())) };
}

/// What a preview standing open does with a key.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Answer {
    /// Not the preview's: the shell keeps the key whole, down and up.
    Pass,
    /// The preview's, and the press opens it -- the message, and whether `Ctrl` was
    /// down with the key that asked.
    Open { message: u32, ctrl: bool },
    /// The preview's, and nothing to say: the release of a key it has already taken.
    Release,
}

/// The keys a standing preview answers, in the form the hook can act on.
///
/// A key the preview takes goes down and comes back up as a pair, the way the space
/// bar's does. Swallowing only the press leaves whatever was in front holding a
/// release it never saw a press for, and a release is a keystroke of its own: the
/// `Enter` of an `Enter` in a rename box is a second `Enter`, and the reader's folder
/// gets two of them for the one that was meant.
fn preview_answer(key: VIRTUAL_KEY, down: bool, shift: bool, ctrl: bool) -> Answer {
    let message = match key {
        k if k == VK_ESCAPE => WM_APP_ESCAPE,
        // `Shift`+`Enter` is the folder's own gesture and the preview does not take it.
        k if k == VK_RETURN && !shift => WM_APP_OPEN,
        k if k == VK_F5 => WM_APP_RELOAD,
        _ => return Answer::Pass,
    };
    if down {
        Answer::Open { message, ctrl }
    } else {
        Answer::Release
    }
}

/// What a press of the space bar asks of the service. The service's belief and
/// the preview window are two witnesses to the same preview, and they can
/// disagree -- a close the belief never heard about, a reveal that failed after
/// it was counted. The press believes whichever saw less, and settles the
/// difference rather than acting on it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Ask {
    /// Nothing stands, or stands somewhere the service knows nothing about: ask
    /// for a preview.
    Open,
    /// A preview stands and is on the screen: this press is the toggle's other
    /// half.
    Close,
    /// The service believes a preview is up, but no window is there to see. The
    /// belief is wrong; put it right and open, instead of closing nothing.
    Reopen,
}

fn space_asks(shown: bool, visible: bool) -> Ask {
    match (shown, visible) {
        (false, _) => Ask::Open,
        (true, true) => Ask::Close,
        (true, false) => Ask::Reopen,
    }
}

unsafe extern "system" fn hook_proc(code: i32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if code < 0 {
        return unsafe { CallNextHookEx(None, code, wp, lp) };
    }
    let Some(service) = SERVICE.with(|slot| slot.get()) else {
        return unsafe { CallNextHookEx(None, code, wp, lp) };
    };
    let service = unsafe { &mut *service };
    let kb = unsafe { &*(lp.0 as *const KBDLLHOOKSTRUCT) };
    // Keys the system or another program put in are none of this watcher's business.
    if kb.flags & LLKHF_INJECTED == LLKHF_INJECTED {
        return unsafe { CallNextHookEx(None, code, wp, lp) };
    }
    let vk = kb.vkCode;
    let down = wp.0 as u32 == WM_KEYDOWN || wp.0 as u32 == WM_SYSKEYDOWN;

    if vk == VK_SPACE.0 as u32 {
        // Windows key space is the input-language switch; it never means a preview.
        if down && (pressed(VK_LWIN) || pressed(VK_RWIN)) {
            return unsafe { CallNextHookEx(None, code, wp, lp) };
        }
        if down {
            if service.space_down {
                // A repeat from a held key: swallowed, but never a second request.
                return LRESULT(1);
            }
            let modified = pressed(VK_CONTROL) || pressed(VK_MENU) || pressed(VK_SHIFT);
            if in_cooldown(service) {
                trace("peek: space lands inside the cooldown typing leaves behind");
                return unsafe { CallNextHookEx(None, code, wp, lp) };
            }
            if modified {
                return unsafe { CallNextHookEx(None, code, wp, lp) };
            }
            // A press that is not asking for a preview is not this watcher's: the
            // shell keeps it whole, down and up, and no trace is kept here. A
            // folder is asking for one -- with a preview standing, the press is
            // its toggle. Anywhere else the space is that application's, and
            // taking it would drop a character the user meant to type.
            if unsafe { foreground_reader().is_some() } {
                service.space_down = true;
                service.hold_started = Some(Instant::now());
                let _ = unsafe { PostMessageW(Some(service.window), WM_APP_SPACE, WPARAM(0), LPARAM(0)) };
                return LRESULT(1);
            }
            return unsafe { CallNextHookEx(None, code, wp, lp) };
        }
        let was_down = std::mem::replace(&mut service.space_down, false);
        if was_down {
            // The release of a key this service swallowed goes with it, so the shell
            // never sees half a keystroke. What the release *means* -- a tap, a
            // hold, a toggle -- is the mode's business, and the service's, where
            // the choice is read rather than raced against.
            let _ = unsafe {
                PostMessageW(Some(service.window), WM_APP_RELEASE, WPARAM(0), LPARAM(0))
            };
            return LRESULT(1);
        }
        return unsafe { CallNextHookEx(None, code, wp, lp) };
    }

    // The release of a vocabulary key whose press this service swallowed goes
    // with it, wherever the focus has moved since the press -- the pairing is
    // what keeps a half keystroke from landing in whatever is in front now.
    if !down && service.vocabulary_down == Some(vk) {
        service.vocabulary_down = None;
        return LRESULT(1);
    }
    // The preview's own keys answer only while the foreground is still a place
    // files are chosen -- the folder the preview came from, or another one.
    // Anywhere else Enter, F5 and Escape belong to whatever the user is doing:
    // a preview that outlived its folder's focus must not swallow the Enter of
    // a form or the F5 of a browser.
    if service.shown && unsafe { foreground_reader().is_some() } {
        let key = VIRTUAL_KEY(vk as u16);
        match preview_answer(key, down, pressed(VK_SHIFT), pressed(VK_CONTROL)) {
            Answer::Open { message, ctrl } => {
                service.vocabulary_down = Some(vk);
                let _ = unsafe {
                    PostMessageW(Some(service.window), message, WPARAM(ctrl as usize), LPARAM(0))
                };
                return LRESULT(1);
            }
            Answer::Release => return LRESULT(1),
            // Arrows and page keys belong to the folder: Explorer moves the selection,
            // and the preview's timer notices.
            Answer::Pass => {}
        }
    }
    if down && !preview_vocabulary(vk) {
        service.invalid_at = Some(Instant::now());
    }
    unsafe { CallNextHookEx(None, code, wp, lp) }
}

fn in_cooldown(service: &Service) -> bool {
    service
        .invalid_at
        .is_some_and(|at| at.elapsed() < INVALID_KEY_COOLDOWN)
}

unsafe extern "system" fn foreground_changed(
    _hook: HWINEVENTHOOK,
    _event: u32,
    _hwnd: HWND,
    _object: i32,
    _child: i32,
    _thread: u32,
    _time: u32,
) {
    if let Some(service) = SERVICE.with(|slot| slot.get()) {
        let service = unsafe { &mut *service };
        service.invalid_at = None;
        // A preview that lives only while its source holds the focus hears the
        // change here, the moment it happens, rather than at the next poll.
        if service.shown && focus_close() {
            let _ = unsafe {
                PostMessageW(Some(service.window), WM_APP_FOCUS, WPARAM(0), LPARAM(0))
            };
        }
    }
}

// ---------------------------------------------------------------- judging the foreground

/// The window a preview would come from, if the foreground is a place files are
/// chosen. Two guards stand before the class name, each answering a way a user is
/// not "browsing": a menu they have open, and a rename box they are typing in.
/// A typing UWP control -- the search box, the Explorer caption -- owns no Win32
/// caret and so hides from both; it is the cooldown the typing itself starts
/// that keeps space out of it, for exactly as long as the typing lasts.
unsafe fn foreground_reader() -> Option<HWND> {
    unsafe {
        let fg = GetForegroundWindow();
        if fg.is_invalid() {
            return None;
        }
        let tid = GetWindowThreadProcessId(fg, None);
        let mut info = GUITHREADINFO {
            cbSize: std::mem::size_of::<GUITHREADINFO>() as u32,
            ..Default::default()
        };
        if GetGUIThreadInfo(tid, &mut info).is_ok() {
            // A nonzero flag is a menu in progress, a drag, a move: states in which
            // space means something else. A caret means a text field is live.
            if info.flags != GUITHREADINFO_FLAGS(0) {
                trace("peek: the foreground thread is busy (a menu, a drag, a move)");
                return None;
            }
            if !info.hwndCaret.is_invalid() {
                trace("peek: a caret is live -- a rename box, a field being typed in");
                return None;
            }
        }
        let class = class_name(fg);
        match class.as_str() {
            "ExploreWClass" | "CabinetWClass" => Some(fg),
            // The desktop is a Progman or WorkerW window that carries the icon view,
            // and only the one carrying the view is worth asking.
            "Progman" | "WorkerW" => {
                FindWindowExW(Some(fg), None, w!("SHELLDLL_DefView"), PCWSTR::null())
                    .ok()
                    .map(|_| fg)
            }
            _ => {
                trace(&format!("peek: the foreground is a {class}, not a place files are chosen"));
                None
            }
        }
    }
}

unsafe fn class_name(hwnd: HWND) -> String {
    let mut buf = [0u16; 256];
    let len = unsafe { GetClassNameW(hwnd, &mut buf) };
    String::from_utf16_lossy(&buf[..len.max(0) as usize])
}

// ---------------------------------------------------------------- reading the selection

/// The selected file the preview is for, taken through the shell from the window the
/// user was looking at. Any failure along the way is simply no preview: the key has
/// already been swallowed, and a silent nothing is better than a dialog about it.
unsafe fn request_preview() -> Option<PathBuf> {
    let fg = unsafe { foreground_reader() }?;
    unsafe { foreground_selection(fg) }.or_else(|| {
        trace("peek: nothing peekable is selected under the foreground window");
        None
    })
}

/// The shell's collection of open windows and the desktop, `{9BA05972-F6A8-11CF-A442-
/// 00A0C90A8F39}`. The windows crate names the interface but no longer names the class,
/// so the id is written out here once, as the docs give it.
const CLSID_SHELL_WINDOWS: GUID = GUID::from_u128(0x9BA05972_F6A8_11CF_A442_00A0C90A8F39);

unsafe fn foreground_selection(fg: HWND) -> Option<PathBuf> {
    unsafe {
        let windows: IShellWindows =
            CoCreateInstance(&CLSID_SHELL_WINDOWS, None, CLSCTX_LOCAL_SERVER).ok()?;
        let browser: IWebBrowser2 = if GetShellWindow() == fg {
            // The desktop's shell view answers through SWC_DESKTOP rather than by
            // matching a window handle: there is no Explorer frame for it.
            let loc = variant_i32(0);
            let mut hwnd = 0i32;
            windows
                .FindWindowSW(&loc, &loc, SWC_DESKTOP, &mut hwnd, SWFO_NEEDDISPATCH)
                .ok()?
                .cast::<IWebBrowser2>()
                .ok()?
        } else {
            // The frame the user sees may itself be the shell browser, or its tab
            // container one window down; both are matched against the enumeration.
            let tab = FindWindowExW(Some(fg), None, w!("ShellTabWindowClass"), PCWSTR::null()).ok();
            let mut found = None;
            let count = windows.Count().unwrap_or(0).max(0);
            for i in 0..count {
                let Ok(item) = windows.Item(&variant_i32(i)) else { continue };
                let Ok(browser) = item.cast::<IWebBrowser2>() else { continue };
                let Ok(hwnd) = browser.HWND() else { continue };
                if hwnd.0 == fg.0 as isize || tab.is_some_and(|t| t.0 as isize == hwnd.0) {
                    found = Some(browser);
                    break;
                }
            }
            found?
        };
        let services: IServiceProvider = browser.cast().ok()?;
        let shell: IShellBrowser = services.QueryService(&SID_STopLevelBrowser).ok()?;
        let view = shell.QueryActiveShellView().ok()?;
        let data: IDataObject = view.GetItemObject(SVGIO_SELECTION).ok()?;
        let format = FORMATETC {
            cfFormat: CF_HDROP.0,
            dwAspect: DVASPECT_CONTENT.0,
            lindex: -1,
            ptd: std::ptr::null_mut(),
            tymed: TYMED_HGLOBAL.0 as u32,
        };
        let mut medium = data.GetData(&format).ok()?;
        let mut path = None;
        let hdrop = HDROP(medium.u.hGlobal.0);
        let count = DragQueryFileW(hdrop, u32::MAX, None);
        for i in 0..count {
            let len = DragQueryFileW(hdrop, i, None) as usize;
            let mut buf = vec![0u16; len + 1];
            DragQueryFileW(hdrop, i, Some(&mut buf));
            buf.truncate(len);
            let candidate = PathBuf::from(std::ffi::OsString::from_wide(&buf));
            if peekable(&candidate) {
                path = Some(candidate);
                break;
            }
        }
        ReleaseStgMedium(&mut medium);
        path
    }
}

fn variant_i32(value: i32) -> VARIANT {
    VARIANT {
        Anonymous: VARIANT_0 {
            Anonymous: std::mem::ManuallyDrop::new(VARIANT_0_0 {
                vt: VT_I4,
                wReserved1: 0,
                wReserved2: 0,
                wReserved3: 0,
                Anonymous: VARIANT_0_0_0 { lVal: value },
            }),
        },
    }
}

/// The extensions a preview exists for, the same list the reader's own open dialog
/// and directory tree agree on.
fn peekable(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_ascii_lowercase())
            .as_deref(),
        Some("md") | Some("markdown") | Some("txt")
    )
}

fn open_with_default(path: &Path) -> bool {
    let wide = utf16(&path.as_os_str().to_string_lossy());
    let started = unsafe {
        ShellExecuteW(None, PCWSTR::null(), PCWSTR(wide.as_ptr()), PCWSTR::null(), PCWSTR::null(), SW_SHOWNORMAL)
    };
    // The shell answers a failed hand-off with a handle it invents rather than the one
    // it was given, and every value up to the icon it hands back for a real one means
    // it did not happen at all.
    started.0 as isize > 32
}

/// Open the file in the reader: the reader already running first, this very program
/// started without `--peek` otherwise. Ctrl+Enter means "read it in the reader",
/// whatever the file type's default program happens to be. `false` says nothing opened,
/// which is the preview's cue to stay where it is and let the reader ask again.
fn open_in_reader(path: &Path) -> bool {
    let path = path.to_owned();
    // A reader that is already up takes the file as a tab, and answers in the time it
    // takes to copy a path out: the window opens the document on its own next pass.
    // So neither the wait for a window to appear nor the wait for the reader to finish
    // reading is this thread's -- which matters, because this thread owns the global
    // hooks, and every millisecond it spends waiting is a millisecond in which a key or
    // a click anywhere in the system has nobody to answer it.
    if instance::find_reader_window().is_some() {
        if let Some(hwnd) = instance::forward(std::slice::from_ref(&path)) {
            instance::focus(hwnd);
            return true;
        }
    }
    // Start this very program the way a double-click starts it: the shell is what
    // a launch of a GUI program goes through, and it is what the window appears
    // without -- a process started by hand can be handed no console, no desktop,
    // and no standard handles, and a reader that dies at birth dies of those.
    let mut exe = [0u16; 1024];
    let len = unsafe { GetModuleFileNameW(None, &mut exe) } as usize;
    let exe = utf16(&String::from_utf16_lossy(&exe[..len]));
    let args = utf16(&format!("\"{}\"", path.as_os_str().to_string_lossy()));
    let started = unsafe {
        ShellExecuteW(
            None,
            PCWSTR::null(),
            PCWSTR(exe.as_ptr()),
            PCWSTR(args.as_ptr()),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        )
    };
    if started.0 as isize <= 32 {
        return false;
    }
    focus_the_reader();
    true
}

/// Name the window the shell is starting, and give it the keyboard, on a thread of its
/// own.
///
/// Windows will not hand the foreground to the process this one started. This service
/// is detached, it is not the window in front, and the key that asked for the reader
/// was swallowed by this thread's own hook a moment ago -- so the last input the system
/// remembers belongs to whatever was in front before, and a launch inherits nothing. The
/// window therefore comes up behind whatever is there, holding a page no key can reach,
/// and a reader who presses `Ctrl`+`4` on it is pressing it somewhere else. Naming the
/// window from a thread of its own costs the hooks nothing: this one has already gone
/// back to its message loop.
fn focus_the_reader() {
    std::thread::spawn(|| {
        if let Some(hwnd) = instance::reader_window() {
            instance::focus(hwnd);
        }
    });
}

// ---------------------------------------------------------------- autostart

const RUN_KEY: PCWSTR = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
const RUN_VALUE: PCWSTR = w!("RubricaPeek");

fn autostart_enabled() -> bool {
    let mut size = 0u32;
    unsafe {
        RegGetValueW(HKEY_CURRENT_USER, RUN_KEY, RUN_VALUE, RRF_RT_REG_SZ, None, None, Some(&mut size))
            == ERROR_SUCCESS
    }
}

fn set_autostart() {
    let mut exe = [0u16; 1024];
    let len = unsafe { GetModuleFileNameW(None, &mut exe) } as usize;
    let command = format!("\"{}\" --peek", String::from_utf16_lossy(&exe[..len]));
    let wide = utf16(&command);
    unsafe {
        let _ = RegSetKeyValueW(
            HKEY_CURRENT_USER,
            RUN_KEY,
            RUN_VALUE,
            REG_SZ.0,
            Some(wide.as_ptr().cast()),
            (wide.len() * 2) as u32,
        );
    }
}

fn clear_autostart() {
    unsafe {
        let _ = RegDeleteKeyValueW(HKEY_CURRENT_USER, RUN_KEY, RUN_VALUE);
    }
}

// ---------------------------------------------------------------- tray icon

unsafe fn tray_icon(hwnd: HWND) -> NOTIFYICONDATAW {
    let mut tip = [0u16; 128];
    let lang = crate::settings::current_language();
    let tip_text = crate::i18n::t(lang, crate::i18n::Key::TrayPeekTooltip);
    for (slot, ch) in tip.iter_mut().zip(format!("{tip_text}\0").encode_utf16()) {
        *slot = ch;
    }
    NOTIFYICONDATAW {
        cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: hwnd,
        uID: TRAY_ID,
        uFlags: NIF_MESSAGE | NIF_ICON | NIF_TIP,
        uCallbackMessage: WM_APP_TRAY,
        hIcon: unsafe { LoadIconW(None, IDI_APPLICATION).unwrap_or_default() },
        szTip: tip,
        ..Default::default()
    }
}

unsafe fn add_tray_icon(hwnd: HWND) {
    let _ = unsafe { Shell_NotifyIconW(NIM_ADD, &tray_icon(hwnd)) };
}

/// Explorer broadcasts this after a taskbar restart; the message id is registered,
/// not constant, so the service asks once and compares against the answer.
fn taskbar_created() -> u32 {
    static ID: std::sync::OnceLock<u32> = std::sync::OnceLock::new();
    *ID.get_or_init(|| unsafe { RegisterWindowMessageW(w!("TaskbarCreated")) })
}

// ---------------------------------------------------------------- the preview window

struct Peek {
    hwnd: HWND,
    d2d: ID2D1Factory,
    target: Option<ID2D1HwndRenderTarget>,
    /// The same target as `target`, cast once, for the image store's sake.
    rt: Option<ID2D1RenderTarget>,
    palette: Palette,
    dark: bool,
    brushes: HashMap<ColorRole, ID2D1SolidColorBrush>,
    font: Option<crate::font::FontEngine>,
    hyphenator: Option<&'static hyphen::Hyphenator>,
    theme: Theme,
    page: Option<crate::view::Page>,
    title: Vec<crate::view::PaintRun>,
    hint: Vec<crate::view::PaintRun>,
    /// Bitmaps for the current page's figures, rebuilt with each file.
    images: Option<images::ImageStore>,
    path: Option<PathBuf>,
    /// The window the selection is watched in; the timer only follows it while the
    /// foreground is still that window.
    source: Option<HWND>,
    scroll: f32,
    /// Wheel remainder a high-resolution mouse left behind: deltas that are not a
    /// whole notch add up here instead of vanishing, which is what made most
    /// messages of a smooth wheel scroll nothing at all.
    wheel_carry: i32,
}

impl Peek {
    /// The window is built the first time a preview is actually asked for, so a
    /// watching service that never shows anything costs only its hook. The window
    /// procedure hangs onto the box through `WM_NCCREATE`, so the peek lives on the
    /// heap from before the first message and never moves again.
    fn new() -> windows::core::Result<Box<Self>> {
        unsafe {
            let d2d: ID2D1Factory = D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None)?;
            let class = utf16(PREVIEW_CLASS);
            let instance = GetModuleHandleW(None)?;
            let wc = WNDCLASSEXW {
                cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
                lpfnWndProc: Some(preview_proc),
                hInstance: instance.into(),
                hCursor: LoadCursorW(None, IDC_ARROW)?,
                lpszClassName: PCWSTR(class.as_ptr()),
                ..Default::default()
            };
            RegisterClassExW(&wc);
            let mut peek = Box::new(Self {
                hwnd: HWND::default(),
                d2d,
                target: None,
                rt: None,
                palette: Palette::of(false),
                dark: false,
                brushes: HashMap::new(),
                font: None,
                hyphenator: None,
                theme: Theme::default(),
                page: None,
                title: Vec::new(),
                hint: Vec::new(),
                images: None,
                path: None,
                source: None,
                scroll: 0.0,
                wheel_carry: 0,
            });
            peek.hwnd = CreateWindowExW(
                WS_EX_TOOLWINDOW | WS_EX_TOPMOST | WS_EX_NOACTIVATE,
                PCWSTR(class.as_ptr()),
                PCWSTR::null(),
                WS_POPUP,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                0,
                0,
                None,
                None,
                Some(instance.into()),
                Some(&*peek as *const Peek as *const core::ffi::c_void),
            )?;
            // A popup has no frame to hang a shadow on; a one-pixel sheet of glass at
            // the bottom is the smallest lie that buys one back.
            let _ = DwmExtendFrameIntoClientArea(
                peek.hwnd,
                &MARGINS { cyBottomHeight: 1, ..Default::default() },
            );
            let _ = DwmSetWindowAttribute(
                peek.hwnd,
                DWMWA_WINDOW_CORNER_PREFERENCE,
                &DWMWCP_ROUND as *const _ as *const core::ffi::c_void,
                4,
            );
            Ok(peek)
        }
    }

    fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    /// Read, typeset, place, and show -- the whole reveal, and whether it
    /// happened. A file that cannot be read leaves the previous state alone
    /// rather than showing an empty page over it, which also means nothing came
    /// up, and that is what the answer says.
    fn show(&mut self, path: &Path) -> bool {
        unsafe { self.place() };
        if !self.load(path) {
            return false;
        }
        unsafe {
            let _ = ShowWindow(self.hwnd, SW_SHOWNOACTIVATE);
            // Raise above whatever is topmost, but never take the focus: the folder
            // keeps it, and the arrows keep meaning "move the selection".
            let _ = SetWindowPos(
                self.hwnd,
                Some(HWND_TOPMOST),
                0,
                0,
                0,
                0,
                SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOSIZE,
            );
            let _ = SetTimer(Some(self.hwnd), POLL_TIMER, SELECTION_POLL_MS, None);
            let _ = InvalidateRect(Some(self.hwnd), None, false);
        }
        let fg = unsafe { GetForegroundWindow() };
        self.source = if fg.is_invalid() { None } else { Some(fg) };
        true
    }

    fn close(&mut self) {
        unsafe {
            let _ = ShowWindow(self.hwnd, SW_HIDE);
            let _ = KillTimer(Some(self.hwnd), POLL_TIMER);
        }
        self.source = None;
        // Nothing stands, so nothing is the target: a page left behind outlives
        // its file only as a lie a later follow would believe of it.
        self.path = None;
        self.page = None;
        self.images = None;
        self.scroll = 0.0;
    }

    /// Re-lay the file the preview is already on, for a change on disk.
    fn refresh(&mut self) {
        if let Some(path) = self.path.clone() {
            if self.load(&path) {
                unsafe { let _ = InvalidateRect(Some(self.hwnd), None, false); }
            }
        }
    }

    /// The selection moved; the preview follows it if the new selection is one of
    /// its own documents, and keeps the old page otherwise.
    fn follow(&mut self, fg: HWND) {
        let Some(path) = (unsafe { foreground_selection(fg) }) else { return };
        if self.path.as_deref() == Some(path.as_path()) {
            return;
        }
        if self.load(&path) {
            unsafe { let _ = InvalidateRect(Some(self.hwnd), None, false); }
        }
    }

    /// Read and typeset one file into `page`, or report failure by returning false.
    fn load(&mut self, path: &Path) -> bool {
        let prefs = settings::document(path);
        let Ok(decoded) = reading::read(path, prefs.encoding) else { return false };
        let plain = prefs.plain.unwrap_or_else(|| reading::is_plain(Some(path)));
        let doc = if prefs.source {
            rubrica_doc::Document::source(&decoded.text)
        } else if plain {
            rubrica_doc::plain::parse(&decoded.text, prefs.text)
        } else {
            rubrica_doc::Document::parse_with(
                &decoded.text,
                rubrica_doc::ParseOptions {
                    keep_line_breaks: prefs.line_breaks.unwrap_or_else(settings::keep_line_breaks),
                },
            )
        };

        // The appearance is read fresh at each reveal: the reader may have changed
        // its mind, or Windows its palette, while the service sat invisible.
        let saved = settings::load();
        let dark = saved.dark.unwrap_or_else(system_prefers_dark);
        if dark != self.dark {
            self.dark = dark;
            self.palette = Palette::of(dark);
            self.brushes.clear();
            unsafe { self.apply_dark_mode() };
        }
        let mut theme = Theme::default();
        if let Some(zoom) = saved.zoom {
            theme.set_zoom(zoom);
        }
        if let Some(face) = saved.face.filter(|i| crate::theme::TextFace::ALL.get(*i).is_some()) {
            theme.set_face(face);
        }
        if let Some(measure) = saved.measure {
            theme.set_measure(measure);
        }
        let profile = profiles::selected(plain);
        if profile != "Default" {
            profiles::load(&profile).apply(&mut theme);
        }
        self.theme = theme;

        if self.font.is_none() {
            let font = match crate::font::FontEngine::new() {
                Ok(font) => font,
                Err(error) => {
                    eprintln!("peek font: {error}");
                    return false;
                }
            };
            if !font.probe() {
                return false;
            }
            self.hyphenator = hyphen::shared();
            self.font = Some(font);
        }
        let font = self.font.as_mut().expect("font loaded above");

        let mut rect = RECT::default();
        unsafe { let _ = GetClientRect(self.hwnd, &mut rect); }
        let client_w = (rect.right - rect.left).max(1) as f32;
        let dpi = unsafe { GetDpiForWindow(self.hwnd) }.max(96) as f32;
        let k = dpi / 72.0;
        self.images = images::ImageStore::new().ok();
        let mut math = crate::math::MathStore::new();
        let mut objects = Objects::new(self.images.as_ref(), path.parent(), &mut math);
        self.page = Some(build_ops(
            font,
            &self.theme,
            &doc,
            client_w,
            dpi,
            &mut objects,
            self.hyphenator,
        ));
        self.scroll = 0.0;
        self.path = Some(path.to_owned());
        self.shape_bar(client_w / k, k);
        true
    }

    /// The file's name on the left of the bar, the keys that work on the right. Both
    /// are shaped through the same engine the page uses, so the bar's lettering is
    /// the page's lettering at a smaller size.
    fn shape_bar(&mut self, client_w: f32, k: f32) {
        self.title.clear();
        self.hint.clear();
        let Some(font) = self.font.as_mut() else { return };
        let name = self
            .path
            .as_ref()
            .map(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default())
            .unwrap_or_default();
        if name.is_empty() {
            return;
        }
        let req = crate::font::FaceRequest {
            family: self.theme.fonts.family(crate::theme::Role::Body, false).to_string(),
            cjk_family: self.theme.fonts.family(crate::theme::Role::Body, true).to_string(),
            japanese_family: self.theme.fonts.japanese[crate::theme::Role::Body as usize].clone(),
            korean_family: self.theme.fonts.korean[crate::theme::Role::Body as usize].clone(),
            cjk_italic: Some(false),
            fallback: self.theme.fonts.fallback.clone(),
            weight: 500,
            italic: false,
        };
        let size = self.theme.base * 0.95;
        let runs = font.shape_runs(&name, 0..name.len(), &req, size, 0.0);
        let mut at = BAR_PAD;
        let baseline = BAR_H / 2.0 + size * k * 0.35;
        for r in &runs {
            if let Some(mut run) = paint_run(font, r, 0.0, 0.0, k, ColorRole::Text, None) {
                run.x = at;
                run.baseline = baseline;
                at += r.width() * k;
                self.title.push(run);
            }
        }
        let hint = "Esc close  \u{2190}\u{2192} files  Enter open  Ctrl+Enter reader";
        let small = self.theme.base * 0.72;
        let runs = font.shape_runs(hint, 0..hint.len(), &req, small, 0.0);
        let mut right = client_w - BAR_PAD;
        let baseline = BAR_H / 2.0 + small * k * 0.35;
        for r in runs.iter().rev() {
            let width = r.width() * k;
            if right - width < at + BAR_PAD {
                break;
            }
            if let Some(mut run) = paint_run(font, r, 0.0, 0.0, k, ColorRole::Muted, None) {
                right -= width;
                run.x = right;
                run.baseline = baseline;
                self.hint.push(run);
            }
        }
    }

    /// Centered on the monitor the pointer is over, sized to about three quarters of
    /// its working area: a preview should read like a page, not cover the desk.
    unsafe fn place(&mut self) {
        unsafe {
            let mut pt = POINT::default();
            let _ = GetCursorPos(&mut pt);
            let monitor = MonitorFromPoint(pt, MONITOR_DEFAULTTONEAREST);
            let mut info = MONITORINFO {
                cbSize: std::mem::size_of::<MONITORINFO>() as u32,
                ..Default::default()
            };
            let _ = GetMonitorInfoW(monitor, &mut info);
            let work = info.rcWork;
            let width = work.right - work.left;
            let height = work.bottom - work.top;
            let h = height * 3 / 4;
            let w = (width * 3 / 5).min(h * 4 / 3);
            let x = work.left + (width - w) / 2;
            let y = work.top + (height - h) / 2;
            let _ = SetWindowPos(self.hwnd, Some(HWND_TOPMOST), x, y, w, h, SWP_NOACTIVATE);
            self.attach();
        }
    }

    /// The render target is rebuilt rather than resized, since the reveal is the
    /// only moment size changes. Brushes live on the target, so they go with it.
    unsafe fn attach(&mut self) {
        unsafe {
            let mut rect = RECT::default();
            let _ = GetClientRect(self.hwnd, &mut rect);
            let w = (rect.right - rect.left).max(1) as u32;
            let h = (rect.bottom - rect.top).max(1) as u32;
            let dpi = GetDpiForWindow(self.hwnd).max(96) as f32;
            let props = D2D1_RENDER_TARGET_PROPERTIES {
                r#type: D2D1_RENDER_TARGET_TYPE_DEFAULT,
                pixelFormat: D2D1_PIXEL_FORMAT {
                    format: DXGI_FORMAT_B8G8R8A8_UNORM,
                    alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
                },
                dpiX: dpi,
                dpiY: dpi,
                usage: D2D1_RENDER_TARGET_USAGE_NONE,
                minLevel: D2D1_FEATURE_LEVEL_DEFAULT,
            };
            let hp = D2D1_HWND_RENDER_TARGET_PROPERTIES {
                hwnd: self.hwnd,
                pixelSize: D2D_SIZE_U { width: w, height: h },
                presentOptions: D2D1_PRESENT_OPTIONS_NONE,
            };
            if let Ok(target) = self.d2d.CreateHwndRenderTarget(&props, &hp) {
                target.SetTextAntialiasMode(D2D1_TEXT_ANTIALIAS_MODE_CLEARTYPE);
                self.rt = target.cast::<ID2D1RenderTarget>().ok();
                self.target = Some(target);
                self.brushes.clear();
            }
        }
    }

    unsafe fn apply_dark_mode(&self) {
        unsafe {
            let v = self.dark as i32;
            let _ = DwmSetWindowAttribute(
                self.hwnd,
                DWMWA_USE_IMMERSIVE_DARK_MODE,
                &v as *const i32 as *const core::ffi::c_void,
                4,
            );
        }
    }

    fn brush(&mut self, role: ColorRole) -> Option<ID2D1SolidColorBrush> {
        if let Some(brush) = self.brushes.get(&role) {
            return Some(brush.clone());
        }
        let target = self.target.as_ref()?;
        let color = d2d(self.palette.ink(role));
        let made = unsafe { target.CreateSolidColorBrush(&color, None) }.ok()?;
        self.brushes.insert(role, made.clone());
        Some(made)
    }

    fn paint(&mut self) {
        // BeginPaint first, whatever there is to draw with: it is what validates the
        // update region, and a paint that returns without it leaves the region
        // pending -- Windows re-queues WM_PAINT at once, and a window whose render
        // target failed to create spins the service thread at full speed, taking
        // the global keyboard and mouse hooks down with it.
        let mut ps = PAINTSTRUCT::default();
        let _ps = unsafe { BeginPaint(self.hwnd, &mut ps) };
        let Some(target) = self.target.clone() else {
            let _ = unsafe { EndPaint(self.hwnd, &ps) };
            return;
        };
        unsafe {
            target.BeginDraw();
            let mut rect = RECT::default();
            let _ = GetClientRect(self.hwnd, &mut rect);
            let client_w = (rect.right - rect.left).max(1) as f32;
            let client_h = (rect.bottom - rect.top).max(1) as f32;
            let dpi = GetDpiForWindow(self.hwnd).max(96) as f32;
            let k = dpi / 72.0;
            let bg = d2d(self.palette.bg);
            target.Clear(Some(&bg));
            // The bar first, and the page clipped under it.
            if let Some(strip) = self.brush(ColorRole::TabStrip) {
                target.FillRectangle(
                    &D2D_RECT_F { left: 0.0, top: 0.0, right: client_w, bottom: BAR_H },
                    &strip,
                );
            }
            let title_brush = self.brush(ColorRole::Text);
            let hint_brush = self.brush(ColorRole::Muted);
            for run in &self.title {
                if let Some(brush) = title_brush.as_ref() {
                    draw_glyph_run(&target, run, brush);
                }
            }
            for run in &self.hint {
                if let Some(brush) = hint_brush.as_ref() {
                    draw_glyph_run(&target, run, brush);
                }
            }
            target.PushAxisAlignedClip(
                &D2D_RECT_F { left: 0.0, top: BAR_H, right: client_w, bottom: client_h },
                D2D1_ANTIALIAS_MODE_PER_PRIMITIVE,
            );
            // The page is lent out for the loop so the brushes -- owned by self, since
            // they are cache entries -- can still be created mid-paint for a colour
            // the page has not asked for before.
            let page = self.page.take();
            if let Some(page) = page.as_ref() {
                let max_scroll = (page.height - (client_h - BAR_H) / k).max(0.0);
                self.scroll = self.scroll.clamp(0.0, max_scroll);
                target.SetTransform(&Matrix3x2::translation(0.0, BAR_H - self.scroll * k));
                let rt = self.rt.clone();
                for op in &page.ops {
                    match op {
                        Op::Rect { x, y, w, h, color } => {
                            if let Some(brush) = self.brush(*color) {
                                target.FillRectangle(
                                    &D2D_RECT_F { left: *x, top: *y, right: x + w, bottom: y + h },
                                    &brush,
                                );
                            }
                        }
                        Op::Line { x0, y0, x1, y1, thickness, color } => {
                            if let Some(brush) = self.brush(*color) {
                                target.DrawLine(
                                    Vector2::new(*x0, *y0),
                                    Vector2::new(*x1, *y1),
                                    &brush,
                                    *thickness,
                                    None,
                                );
                            }
                        }
                        Op::Image { path, x, y, w, h } => {
                            if let (Some(store), Some(rt)) = (self.images.as_ref(), rt.as_ref()) {
                                if let Some(bitmap) = store.bitmap(rt, path) {
                                    target.DrawBitmap(
                                        &bitmap,
                                        Some(&D2D_RECT_F { left: *x, top: *y, right: x + w, bottom: y + h }),
                                        1.0,
                                        D2D1_BITMAP_INTERPOLATION_MODE_LINEAR,
                                        None,
                                    );
                                }
                            }
                        }
                        Op::Runs(runs) => {
                            for run in runs {
                                let Some(brush) = self.brush(run.color) else { continue };
                                draw_glyph_run(&target, run, &brush);
                            }
                        }
                    }
                }
            }
            self.page = page;
            target.PopAxisAlignedClip();
            target.SetTransform(&Matrix3x2::identity());
            let _ = target.EndDraw(None, None);
        }
        let _ = unsafe { EndPaint(self.hwnd, &ps) };
    }
}

/// One positioned run onto the target: the bar's lettering and the page's share the
/// path, since a run's coordinates are already in the space the target draws in.
unsafe fn draw_glyph_run(
    target: &ID2D1HwndRenderTarget,
    run: &crate::view::PaintRun,
    brush: &ID2D1SolidColorBrush,
) {
    unsafe {
        let description = DWRITE_GLYPH_RUN {
            fontFace: std::mem::ManuallyDrop::new(Some(run.face.clone())),
            fontEmSize: run.em,
            glyphCount: run.glyphs.len() as u32,
            glyphIndices: run.glyphs.as_ptr(),
            glyphAdvances: run.advances.as_ptr(),
            glyphOffsets: run.offsets.as_ptr(),
            isSideways: false.into(),
            bidiLevel: run.bidi_level as u32,
        };
        target.DrawGlyphRun(
            Vector2::new(
                glyph_origin(run.x, run.advances.iter().sum(), run.bidi_level),
                run.baseline,
            ),
            &description,
            brush,
            DWRITE_MEASURING_MODE_NATURAL,
        );
        let _ = std::mem::ManuallyDrop::into_inner(description.fontFace);
    }
}

unsafe extern "system" fn preview_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    // The pointer is installed by WM_NCCREATE itself, which arrives while
    // GWLP_USERDATA is still empty -- so it is answered before the lookup below,
    // exactly as the main window does.
    if msg == WM_NCCREATE {
        let cs = unsafe { &*(lp.0 as *const CREATESTRUCTW) };
        if !cs.lpCreateParams.is_null() {
            unsafe { SetWindowLongPtrW(hwnd, GWLP_USERDATA, cs.lpCreateParams as isize) };
        }
        return unsafe { DefWindowProcW(hwnd, msg, wp, lp) };
    }
    let raw = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) };
    if raw == 0 {
        return unsafe { DefWindowProcW(hwnd, msg, wp, lp) };
    }
    let peek = unsafe { &mut *(raw as *mut Peek) };
    match msg {
        WM_ERASEBKGND => LRESULT(1),
        WM_PAINT => {
            peek.paint();
            LRESULT(0)
        }
        WM_TIMER if wp.0 == POLL_TIMER => unsafe {
            // The selection is only followed while the foreground is still the window
            // the preview came from. Anywhere else the preview keeps what it has,
            // since a page the user walked away from is not wrong to stay put --
            // unless the reader has asked for a preview that lives only as long as
            // the folder has the focus, in which case the focus's leaving is the
            // same as the user's saying they are done. The choice is read fresh:
            // a preview may have been standing since before it was made.
            let fg = GetForegroundWindow();
            if !fg.is_invalid() && Some(fg) == peek.source {
                peek.follow(fg);
            } else if !fg.is_invalid() && focus_close() {
                close_preview_service();
            }
            LRESULT(0)
        },
        // A click on the window never moves the focus anywhere: the window is a
        // topmost, non-activating popup whose press the desktop would otherwise
        // have received. Closing on it is a choice of its own, off by default.
        WM_MOUSEACTIVATE if click_close() => {
            close_preview_service();
            LRESULT(MA_NOACTIVATE as isize)
        }
        WM_MOUSEWHEEL => {
            // The window never holds the focus, so this arrives only through the
            // system's "scroll under the pointer" courtesy; honour it by scrolling.
            // The remainder a smooth wheel's sub-notch deltas leave behind is kept
            // for the next message rather than thrown away.
            let delta = (wp.0 as u32 >> 16) as u16 as i16;
            let notch = WHEEL_DELTA as i32;
            peek.wheel_carry = (peek.wheel_carry + delta as i32).clamp(-notch * 8, notch * 8);
            let notches = peek.wheel_carry / notch;
            peek.wheel_carry -= notches * notch;
            peek.scroll -= (notches * 3) as f32 * peek.theme.base * 1.7;
            unsafe { let _ = InvalidateRect(Some(hwnd), None, false); }
            LRESULT(0)
        }
        WM_DESTROY => {
            unsafe { SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0) };
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wp, lp) },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The service's belief and the preview's own window are two witnesses to one
    /// preview, and a press that finds them disagreeing settles the difference
    /// rather than acting on it: closing a window nobody can see is not a close,
    /// and a second press is a price nobody should pay for the first one's error.
    #[test]
    fn a_space_press_asks_what_the_window_is_showing() {
        assert_eq!(space_asks(false, false), Ask::Open);
        assert_eq!(space_asks(false, true), Ask::Open);
        assert_eq!(space_asks(true, true), Ask::Close);
        assert_eq!(space_asks(true, false), Ask::Reopen);
    }

    /// The three keys a standing preview takes, each of them a whole keystroke rather
    /// than half of one: a press the preview answers, and a release the shell must not
    /// see on its own.
    #[test]
    fn a_key_a_preview_answers_is_swallowed_down_and_up() {
        let down = Answer::Open { message: WM_APP_ESCAPE, ctrl: false };
        assert_eq!(preview_answer(VK_ESCAPE, true, false, false), down);
        assert_eq!(preview_answer(VK_ESCAPE, false, false, false), Answer::Release);
        assert_eq!(
            preview_answer(VK_F5, true, false, false),
            Answer::Open { message: WM_APP_RELOAD, ctrl: false }
        );
        assert_eq!(preview_answer(VK_F5, false, false, false), Answer::Release);
    }

    /// `Enter` is the one that carries a choice with it, and the one whose half a
    /// keystroke hurts most: a plain `Enter` reads the document in the reader, the same
    /// key with `Ctrl`, and `Shift`+`Enter` belongs to the folder.
    #[test]
    fn enter_asks_for_the_reader_and_says_which_reader() {
        assert_eq!(
            preview_answer(VK_RETURN, true, false, false),
            Answer::Open { message: WM_APP_OPEN, ctrl: false }
        );
        assert_eq!(
            preview_answer(VK_RETURN, true, false, true),
            Answer::Open { message: WM_APP_OPEN, ctrl: true }
        );
        assert_eq!(preview_answer(VK_RETURN, false, false, true), Answer::Release);
        // `Shift`+`Enter` is the folder's, so the preview never takes either half of it.
        assert_eq!(preview_answer(VK_RETURN, true, true, false), Answer::Pass);
        assert_eq!(preview_answer(VK_RETURN, false, true, false), Answer::Pass);
    }

    /// Everything the preview does not answer belongs to the window that opened it --
    /// the arrows that move a selection most of all, since the preview follows the
    /// selection rather than making it.
    #[test]
    fn every_other_key_is_the_folders() {
        for vk in [VK_LEFT, VK_UP, VK_RIGHT, VK_DOWN, VK_HOME, VK_END, VK_PRIOR, VK_NEXT, VK_TAB] {
            assert_eq!(preview_answer(vk, true, false, false), Answer::Pass, "{vk:?}");
            assert_eq!(preview_answer(vk, false, false, false), Answer::Pass, "{vk:?}");
        }
    }
}
