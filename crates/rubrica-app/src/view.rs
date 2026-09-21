//! Window host and Direct2D renderer.
//!
//! The display list is built in document points, then scaled to device independent
//! pixels whenever the client width or the window DPI changes. That split is
//! deliberate: `DrawGlyphRun` takes advance widths in the same units as its
//! `fontEmSize`, so scaling at paint time would mean copying every advance array on
//! every frame. Doing it once per relayout keeps scrolling allocation-free.

use std::collections::HashMap;
use std::path::PathBuf;

use rubrica_doc::{Align, Block, BlockKind, Document, InlineStyle};
use rubrica_type::justification::place;
use rubrica_type::paragraph::{Item, Spacing, StyleId, StyleSpan};
use rubrica_type::units::Pt;
use rubrica_type::{BreakOptions, typeset};
use windows::core::{w, Interface, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Direct2D::Common::{
    D2D1_ALPHA_MODE_PREMULTIPLIED, D2D1_COLOR_F, D2D1_PIXEL_FORMAT, D2D_RECT_F, D2D_SIZE_U,
};
use windows::Win32::Graphics::Direct2D::{
    D2D1_BITMAP_INTERPOLATION_MODE_LINEAR, D2D1_FACTORY_TYPE_SINGLE_THREADED, D2D1_FEATURE_LEVEL_DEFAULT,
    D2D1_HWND_RENDER_TARGET_PROPERTIES, D2D1_PRESENT_OPTIONS_NONE, D2D1_RENDER_TARGET_PROPERTIES,
    D2D1_RENDER_TARGET_TYPE_DEFAULT, D2D1_RENDER_TARGET_USAGE_NONE,
    D2D1_TEXT_ANTIALIAS_MODE_CLEARTYPE, ID2D1Factory, ID2D1HwndRenderTarget, ID2D1RenderTarget,
    ID2D1SolidColorBrush, D2D1CreateFactory,
};
use windows::Win32::Graphics::DirectWrite::{
    DWRITE_GLYPH_OFFSET, DWRITE_GLYPH_RUN, DWRITE_MEASURING_MODE_NATURAL, IDWriteFontFace,
};
use windows::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWA_USE_IMMERSIVE_DARK_MODE};
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM;
use windows::Win32::Graphics::Gdi::{HBRUSH, InvalidateRect};
use windows::Win32::System::Com::{
    CoInitializeEx, COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Registry::{RRF_RT_DWORD, RegGetValueW, HKEY_CURRENT_USER};
use windows::Win32::UI::HiDpi::{
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, GetDpiForWindow, SetProcessDpiAwarenessContext,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetKeyState, ReleaseCapture, SetCapture, VK_CONTROL, VK_O, VK_UP,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{VK_DOWN, VK_ESCAPE, VK_NEXT, VK_PRIOR};
use windows::Win32::UI::Controls::Dialogs::{
    GetOpenFileNameW, OPENFILENAMEW, OFN_FILEMUSTEXIST, OFN_HIDEREADONLY, OFN_PATHMUSTEXIST,
};
use windows::Win32::UI::Shell::{DragAcceptFiles, DragFinish, DragQueryFileW, HDROP};
use windows::Win32::UI::WindowsAndMessaging::{
    CS_HREDRAW, CS_VREDRAW, CREATESTRUCTW, CreateWindowExW, DefWindowProcW, DispatchMessageW,
    GWLP_USERDATA, GetClientRect, GetWindowLongPtrW, GetMessageW, HWND_TOP, IDC_ARROW, KillTimer,
    LoadCursorW, MSG, PostQuitMessage, RegisterClassExW, SW_SHOWNORMAL, SetTimer,
    SetWindowLongPtrW, SetWindowPos, ShowWindow, TranslateMessage, WNDCLASSEXW, WM_DESTROY,
    WM_DPICHANGED, WM_ERASEBKGND, WM_KEYDOWN, WM_MOUSEWHEEL, WM_NCCREATE, WM_PAINT, WM_SIZE,
    WM_TIMER, WS_EX_APPWINDOW, WS_OVERLAPPEDWINDOW, SWP_NOACTIVATE, SWP_NOZORDER, WM_DROPFILES,
    WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE,
};
use windows_numerics::Vector2;

use crate::font::{FaceRequest, FontEngine, ObjectBox, Style as RunStyle};
use crate::images::ImageStore;
use crate::theme::{ColorRole, Theme};
use crate::{Error, Result};

/// Page margin, in ems of the body size.
const MARGIN_EM: Pt = 2.6;
/// Wheel notch travel, in points.
const WHEEL_STEP: Pt = 72.0;
/// Appearance is polled, not pushed; see `WM_TIMER` below.
const APPEARANCE_TIMER: usize = 0x5140;
const APPEARANCE_TICK_MS: u32 = 400;

#[derive(Clone, Copy, Debug, PartialEq)]
struct Rgb {
    r: f32,
    g: f32,
    b: f32,
}

impl Rgb {
    const fn gray(v: f32) -> Rgb {
        Rgb { r: v, g: v, b: v }
    }
}

/// Light and dark ink. The light paper is warm rather than pure white, and the dark
/// text is a little under full contrast: both reduce glare over a long read, and a
/// reader is read for pages at a time rather than glanced at.
#[derive(Clone, Debug)]
struct Palette {
    dark: bool,
    bg: Rgb,
    text: Rgb,
    muted: Rgb,
    accent: Rgb,
    code_bg: Rgb,
}

impl Palette {
    fn of(dark: bool) -> Palette {
        if dark {
            Palette {
                dark,
                bg: Rgb::gray(0.112),
                text: Rgb { r: 0.855, g: 0.86, b: 0.875 },
                muted: Rgb { r: 0.60, g: 0.62, b: 0.66 },
                accent: Rgb { r: 0.44, g: 0.67, b: 0.95 },
                code_bg: Rgb { r: 0.155, g: 0.162, b: 0.178 },
            }
        } else {
            Palette {
                dark,
                bg: Rgb { r: 0.977, g: 0.974, b: 0.966 },
                text: Rgb { r: 0.13, g: 0.135, b: 0.15 },
                muted: Rgb { r: 0.42, g: 0.44, b: 0.47 },
                accent: Rgb { r: 0.12, g: 0.35, b: 0.66 },
                code_bg: Rgb { r: 0.937, g: 0.935, b: 0.928 },
            }
        }
    }

    fn ink(&self, role: ColorRole) -> Rgb {
        match role {
            ColorRole::Text | ColorRole::Code => self.text,
            ColorRole::Muted | ColorRole::Faint => self.muted,
            ColorRole::Accent => self.accent,
            ColorRole::Surface => self.code_bg,
        }
    }
}

fn d2d(c: Rgb) -> D2D1_COLOR_F {
    D2D1_COLOR_F { r: c.r, g: c.g, b: c.b, a: 1.0 }
}

/// A style-table entry: what a `StyleId` means for measuring and painting.
#[derive(Clone, Debug, PartialEq)]
struct AppStyle {
    face: FaceRequest,
    size: Pt,
    tracking: f32,
    color: ColorRole,
    /// Set instead of font properties for an inline object such as an image.
    object: Option<ObjectBox>,
    /// The file an object style draws, when it has one.
    image: Option<PathBuf>,
}

/// One face's positioned glyphs, in device independent pixels.
pub struct PaintRun {
    face: IDWriteFontFace,
    /// Resolved family, recorded at layout time so the report can show which face
    /// actually carried each run without a COM round trip per frame.
    pub family: String,
    pub em: f32,
    pub glyphs: Vec<u16>,
    pub advances: Vec<f32>,
    offsets: Vec<DWRITE_GLYPH_OFFSET>,
    pub x: f32,
    pub baseline: f32,
    color: ColorRole,
}

pub enum Op {
    Runs(Vec<PaintRun>),
    /// An inline object, positioned in device independent pixels.
    Image { path: PathBuf, x: f32, y: f32, w: f32, h: f32 },
    Rect { x: f32, y: f32, w: f32, h: f32, color: ColorRole },
    Line { x0: f32, y0: f32, x1: f32, y1: f32, thickness: f32, color: ColorRole },
}

pub struct View {
    d2d: ID2D1Factory,
    target: Option<ID2D1RenderTarget>,
    hwnd_target: Option<ID2D1HwndRenderTarget>,
    font: FontEngine,
    theme: Theme,
    doc: Document,
    ops: Vec<Op>,
    palette: Palette,
    brushes: HashMap<ColorRole, ID2D1SolidColorBrush>,
    scroll: Pt,
    content_h: Pt,
    client_w: f32,
    client_h: f32,
    dpi: f32,
    path: Option<PathBuf>,
    /// Set while the pointer is dragging the scroll thumb.
    dragging: bool,
    /// Built once a render target exists, since bitmaps need one.
    images: Option<ImageStore>,
}

/// Thumb geometry, in device independent pixels, or `None` when the document fits.
///
/// Kept free of `View` so the arithmetic -- thumb height as the viewport fraction,
/// and the scroll-to-position mapping that must stay consistent with it -- can be
/// tested without a render target or a window.
fn thumb_rect(content_h: Pt, client_w: f32, client_h: f32, scroll: Pt, dpi: f32) -> Option<(f32, f32, f32, f32)> {
    let k = scale_of(dpi);
    let doc = content_h * k;
    if doc <= client_h + 1.0 {
        return None;
    }
    let track = 10.0;
    let x = client_w - track - 3.0;
    let h = (client_h * client_h / doc).max(28.0);
    let max_scroll = (content_h - client_h / k).max(0.0);
    let frac = if max_scroll > 0.0 { (scroll / max_scroll).clamp(0.0, 1.0) } else { 0.0 };
    Some((x, frac * (client_h - h).max(0.0), track, h))
}

/// Points to device independent pixels at the target's DPI.
#[inline]
fn scale_of(dpi: f32) -> f32 {
    dpi / 72.0
}

pub fn run(source: String, path: Option<PathBuf>) -> Result<()> {
    // Must happen before the first window exists, or the process is already
    // bitmap-scaled and text on a secondary high-density monitor is soft.
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        // WIC, and so every image, is created through COM: without this the factory
        // call fails and figures silently degrade to their placeholder.
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE);
    }

    let font = FontEngine::new().map_err(|e| -> Error { format!("DirectWrite: {e}").into() })?;
    if !font.probe() {
        return Err("could not open any installed font face".into());
    }
    let d2d: ID2D1Factory = unsafe { D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None) }
        .map_err(|e| -> Error { format!("Direct2D: {e}").into() })?;

    let mut view = Box::new(View {
        d2d,
        target: None,
        hwnd_target: None,
        font,
        theme: Theme::default(),
        doc: Document::parse(&source),
        ops: Vec::new(),
        palette: Palette::of(system_prefers_dark()),
        brushes: HashMap::new(),
        scroll: 0.0,
        content_h: 0.0,
        client_w: 1.0,
        client_h: 1.0,
        dpi: 96.0,
        path,
        dragging: false,
        images: None,
    });

    const CLASS: &str = "Rubrica.Main";
    unsafe {
        let hinst = GetModuleHandleW(None)?;
        let wide = utf16(CLASS);
        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(wnd_proc),
            hInstance: hinst.into(),
            hCursor: LoadCursorW(None, IDC_ARROW)?,
            // Null on purpose: the render target covers every client pixel, and a
            // system background brush flashes white while a corner is dragged.
            hbrBackground: HBRUSH::default(),
            lpszClassName: PCWSTR(wide.as_ptr()),
            ..Default::default()
        };
        if RegisterClassExW(&wc) == 0 {
            return Err("RegisterClassExW failed".into());
        }
        let title = utf16(&window_title(view.path.as_deref()));
        let hwnd = CreateWindowExW(
            WS_EX_APPWINDOW,
            PCWSTR(wide.as_ptr()),
            PCWSTR(title.as_ptr()),
            WS_OVERLAPPEDWINDOW,
            120,
            100,
            1080,
            800,
            None,
            None,
            Some(hinst.into()),
            Some(&mut *view as *mut View as *const core::ffi::c_void),
        )
        .map_err(|e| -> Error { format!("CreateWindowExW: {e}").into() })?;

        view.attach(hwnd);
        let _ = ShowWindow(hwnd, SW_SHOWNORMAL);
        let _ = SetTimer(Some(hwnd), APPEARANCE_TIMER, APPEARANCE_TICK_MS, None);

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        let _ = KillTimer(Some(hwnd), APPEARANCE_TIMER);
        // `view` is dropped here; the pointer stored in GWLP_USERDATA dies with it.
    }
    Ok(())
}

fn window_title(path: Option<&std::path::Path>) -> String {
    match path {
        Some(p) => format!("Rubrica \u{2014} {}", p.display()),
        None => "Rubrica \u{2014} sample".to_string(),
    }
}

/// Null-terminated UTF-16, the shape every `PCWSTR` window-name parameter wants.
fn utf16(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Reads the same registry value the Settings app writes. There is no window
/// message for this setting, so it is polled; see `WM_TIMER`.
fn system_prefers_dark() -> bool {
    let mut value: u32 = 1;
    let mut len = std::mem::size_of::<u32>() as u32;
    let r = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            w!("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize"),
            w!("AppsUseLightTheme"),
            RRF_RT_DWORD,
            None,
            Some(&mut value as *mut u32 as *mut core::ffi::c_void),
            Some(&mut len),
        )
    };
    r.is_ok() && value == 0
}

unsafe extern "system" fn wnd_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    // The pointer is installed by WM_NCCREATE, before WM_SIZE can arrive.
    let raw = GetWindowLongPtrW(hwnd, GWLP_USERDATA);
    let view = if raw == 0 { None } else { Some(&mut *(raw as *mut View)) };
    match msg {
        WM_NCCREATE => {
            let cs = &*(lp.0 as *const CREATESTRUCTW);
            if !cs.lpCreateParams.is_null() {
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, cs.lpCreateParams as isize);
            }
            DefWindowProcW(hwnd, msg, wp, lp)
        }
        WM_ERASEBKGND => LRESULT(1),
        WM_DESTROY => {
            PostQuitMessage(0);
            LRESULT(0)
        }
        WM_PAINT | WM_SIZE | WM_DPICHANGED | WM_MOUSEWHEEL | WM_KEYDOWN | WM_TIMER => {
            match view {
                Some(v) => v.on_message(hwnd, msg, wp, lp),
                None => DefWindowProcW(hwnd, msg, wp, lp),
            }
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

impl View {
    unsafe fn attach(&mut self, hwnd: HWND) {
        let mut r = RECT::default();
        let _ = GetClientRect(hwnd, &mut r);
        self.client_w = (r.right - r.left).max(1) as f32;
        self.client_h = (r.bottom - r.top).max(1) as f32;
        self.dpi = GetDpiForWindow(hwnd).max(96) as f32;
        let props = D2D1_RENDER_TARGET_PROPERTIES {
            r#type: D2D1_RENDER_TARGET_TYPE_DEFAULT,
            pixelFormat: D2D1_PIXEL_FORMAT {
                format: DXGI_FORMAT_B8G8R8A8_UNORM,
                alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
            },
            dpiX: self.dpi,
            dpiY: self.dpi,
            usage: D2D1_RENDER_TARGET_USAGE_NONE,
            minLevel: D2D1_FEATURE_LEVEL_DEFAULT,
        };
        let hp = D2D1_HWND_RENDER_TARGET_PROPERTIES {
            hwnd,
            pixelSize: D2D_SIZE_U { width: self.client_w as u32, height: self.client_h as u32 },
            presentOptions: D2D1_PRESENT_OPTIONS_NONE,
        };
        match self.d2d.CreateHwndRenderTarget(&props, &hp) {
            Ok(t) => {
                t.SetTextAntialiasMode(D2D1_TEXT_ANTIALIAS_MODE_CLEARTYPE);
                self.hwnd_target = Some(t.clone());
                self.target = t.cast::<ID2D1RenderTarget>().ok();
            }
            Err(e) => eprintln!("render target: {e}"),
        }
        DragAcceptFiles(hwnd, true);
        // Sizing needs only WIC; bitmaps are made later against the render target.
        if self.target.is_some() && self.images.is_none() {
            self.images = ImageStore::new().ok();
        }
        self.apply_dark_titlebar(hwnd);
        self.relayout();
    }

    unsafe fn apply_dark_titlebar(&self, hwnd: HWND) {
        let v: i32 = self.palette.dark as i32;
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_USE_IMMERSIVE_DARK_MODE,
            &v as *const i32 as *const core::ffi::c_void,
            std::mem::size_of::<i32>() as u32,
        );
    }

    /// Create every brush the display list references, before drawing.
    ///
    /// The paint loop only reads `self.ops` and `self.brushes`, so brushes cannot
    /// be created lazily inside it without a mutable borrow colliding with that
    /// iteration. Warming them up here also keeps a cache miss out of the frame path.
    fn ensure_brushes(&mut self) {
        let Some(rt) = self.target.clone() else { return };
        let mut roles: Vec<ColorRole> = Vec::new();
        for op in self.ops.iter() {
            let mut push = |r: ColorRole| {
                if !roles.contains(&r) {
                    roles.push(r);
                }
            };
            match op {
                Op::Rect { color, .. } | Op::Line { color, .. } => push(*color),
                Op::Runs(rs) => {
                    for r in rs {
                        push(r.color);
                    }
                }
                // Images are drawn with their own bitmap, not a brush.
                Op::Image { .. } => {}
            }
        }
        for role in roles {
            if self.brushes.contains_key(&role) {
                continue;
            }
            let c = d2d(self.palette.ink(role));
            if let Ok(b) = unsafe { rt.CreateSolidColorBrush(&c, None) } {
                self.brushes.insert(role, b);
            }
        }
    }

    unsafe fn on_message(&mut self, hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
        match msg {
            WM_SIZE => {
                let w = (lp.0 & 0xFFFF) as u32;
                let h = ((lp.0 >> 16) & 0xFFFF) as u32;
                self.client_w = w.max(1) as f32;
                self.client_h = h.max(1) as f32;
                if let Some(t) = &self.hwnd_target {
                    let _ = t.Resize(&D2D_SIZE_U { width: w.max(1), height: h.max(1) });
                }
                self.dpi = GetDpiForWindow(hwnd).max(96) as f32;
                self.relayout();
                let _ = InvalidateRect(Some(hwnd), None, false);
                LRESULT(0)
            }
            WM_DPICHANGED => {
                let r = &*(lp.0 as *const RECT);
                if let Some(t) = &self.hwnd_target {
                    t.SetDpi(self.dpi.max(1.0), self.dpi.max(1.0));
                }
                self.client_w = (r.right - r.left).max(1) as f32;
                self.client_h = (r.bottom - r.top).max(1) as f32;
                let _ = SetWindowPos(
                    hwnd,
                    Some(HWND_TOP),
                    r.left,
                    r.top,
                    r.right - r.left,
                    r.bottom - r.top,
                    SWP_NOZORDER | SWP_NOACTIVATE,
                );
                self.dpi = GetDpiForWindow(hwnd).max(96) as f32;
                if let Some(t) = &self.target {
                    t.SetDpi(self.dpi, self.dpi);
                }
                self.relayout();
                let _ = InvalidateRect(Some(hwnd), None, false);
                LRESULT(0)
            }
            WM_MOUSEWHEEL => {
                let ticks = ((wp.0 >> 16) & 0xFFFF) as i16 as f32;
                self.scroll_by(-ticks / 120.0 * WHEEL_STEP);
                let _ = InvalidateRect(Some(hwnd), None, false);
                LRESULT(0)
            }
            WM_KEYDOWN => {
                let step = self.theme.base * self.theme.body_leading.latin;
                let page = self.client_h / scale_of(self.dpi) * 0.85;
                match wp.0 as u32 {
                    k if k == VK_UP.0 as u32 => self.scroll_by(-step),
                    k if k == VK_DOWN.0 as u32 => self.scroll_by(step),
                    k if k == VK_PRIOR.0 as u32 => self.scroll_by(-page),
                    k if k == VK_NEXT.0 as u32 => self.scroll_by(page),
                    k if k == VK_ESCAPE.0 as u32 => PostQuitMessage(0),
                    k if k == VK_O.0 as u32
                        && (GetKeyState(VK_CONTROL.0 as i32) as u16 & 0x8000) != 0 =>
                    {
                        if let Some(path) = self.prompt_for_file(hwnd) {
                            self.load_document(&path);
                        }
                    }
                    _ => {}
                }
                let _ = InvalidateRect(Some(hwnd), None, false);
                LRESULT(0)
            }
            WM_TIMER => {
                let dark = system_prefers_dark();
                if dark != self.palette.dark {
                    self.palette = Palette::of(dark);
                    self.brushes.clear();
                    self.apply_dark_titlebar(hwnd);
                    self.relayout();
                }
                let _ = InvalidateRect(Some(hwnd), None, false);
                LRESULT(0)
            }
            WM_DROPFILES => {
                self.open_from_drop(hwnd, wp.0);
                LRESULT(0)
            }
            WM_LBUTTONDOWN => {
                // A press inside the thumb starts a drag; anywhere else pages down.
                let x = ((lp.0 & 0xFFFF) as i16) as f32;
                let y = ((lp.0 >> 16) as i16) as f32;
                if self.thumb_hit(x, y) {
                    self.dragging = true;
                    self.scroll_to_thumb(y);
                } else if y > self.client_h * 0.5 {
                    self.scroll_by(self.page());
                } else {
                    self.scroll_by(-self.page());
                }
                let _ = SetCapture(hwnd);
                let _ = InvalidateRect(Some(hwnd), None, false);
                LRESULT(0)
            }
            WM_MOUSEMOVE => {
                if self.dragging {
                    self.scroll_to_thumb(((lp.0 >> 16) as i16) as f32);
                    let _ = InvalidateRect(Some(hwnd), None, false);
                }
                LRESULT(0)
            }
            WM_LBUTTONUP => {
                if self.dragging {
                    self.dragging = false;
                    let _ = ReleaseCapture();
                }
                LRESULT(0)
            }
            WM_PAINT => {
                self.paint();
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, msg, wp, lp),
        }
    }

    fn scroll_by(&mut self, dy: Pt) {
        let view_h = self.client_h / scale_of(self.dpi);
        let max = (self.content_h + self.theme.base - view_h).max(0.0);
        self.scroll = (self.scroll + dy).clamp(0.0, max);
    }

    fn relayout(&mut self) {
        let (ops, h, _column, _left) =
            build_ops(
                &mut self.font,
                &self.theme,
                &self.doc,
                self.client_w,
                self.dpi,
                self.images.as_ref(),
                self.path.as_deref().and_then(|p| p.parent()),
            );
        self.content_h = h;
        self.ops = ops;
    }

}

/// Inputs that are the same for every block in one layout pass.
struct Ctx<'a> {
    theme: &'a Theme,
    styles: &'a [AppStyle],
    /// Points to device independent pixels.
    k: f32,
}

/// One block plus where on the page it belongs.
struct Blk<'a> {
    b: &'a Block,
    text: &'a str,
    spans: &'a [StyleSpan],
    base: usize,
    left: Pt,
    column: Pt,
    table: Option<&'a PreparedTable>,
}

/// A table cell with its styles already resolved to ids, so column sizing and
/// word wrapping measure exactly what prose measures.
pub struct PreparedCell {
    pub text: String,
    pub spans: Vec<StyleSpan>,
    pub align: Align,
}

pub struct PreparedTable {
    pub head: Vec<PreparedCell>,
    pub rows: Vec<Vec<PreparedCell>>,
}

/// Horizontal padding inside a table cell, in ems of the body size.
const CELL_PAD_EM: Pt = 0.6;
/// The narrowest a column may be squeezed to before it is left to overflow.
const CELL_MIN_EM: Pt = 3.0;

/// Typeset one block into the display list and return the new document y.
fn layout_block(font: &mut FontEngine, ctx: &Ctx<'_>, blk: &Blk<'_>, ops: &mut Vec<Op>, mut y: Pt) -> Pt {
    let Ctx { theme, styles, k } = *ctx;
    let Blk { b, text, spans, base, left, column, table } = *blk;
    let mut images_out: Vec<(PathBuf, f32, f32, Pt, Pt)> = Vec::new();
    if b.kind == BlockKind::Rule {
        y += theme.base * 0.6;
        ops.push(Op::Line {
            x0: left * k,
            y0: y * k,
            x1: (left + column) * k,
            y1: y * k,
            thickness: theme.base * 0.05 * k,
            color: ColorRole::Muted,
        });
        return y + theme.base * 0.6;
    }
    if let Some(t) = blk.table {
        return layout_table(font, theme, styles, t, left, column, ops, y, k);
    }
    if text.trim().is_empty() {
        return y;
    }

    let size = theme.body_size(b.kind);
    let leading = theme.line_spacing(b.kind);
    let mixed = text.chars().any(|c| matches!(c as u32, 0x3000..=0x303F | 0x4E00..=0x9FFF | 0x3040..=0x30FF | 0xFF00..=0xFFEF));
    let spacing = Spacing::for_size(size);
    let mut opts = BreakOptions::new(column);
    opts.ragged = b.ragged();
    opts.par_indent = theme.first_line_indent_em * size;

    let (para, plan) = typeset(text, &spacing, StyleId(base as u16), spans, &opts, font);
    if plan.lines.is_empty() {
        return y;
    }

    let bg_role = if b.kind == BlockKind::Code { Some(ColorRole::Surface) } else { None };
    let panel_top = y;
    let mut panel_bottom = y;

    for line in &plan.lines {
        let placed = place(&para, line);
        let mut runs: Vec<PaintRun> = Vec::new();
        let mut ascent = 0.0f32;
        let mut descent = 0.0f32;
        for slot in placed {
            let Some(node_id) = slot.node else { continue };
            let node = para.node(node_id);
            let st = &styles[node.style.0 as usize];
            if let (Some(o), Some(file)) = (st.object, st.image.clone()) {
                // An object contributes its own box to the line and is drawn
                // from its file, not from any glyph run.
                ascent = ascent.max(o.ascent);
                descent = descent.max(o.descent);
                images_out.push((file, (left + slot.x) * k, o.advance * k, o.ascent, o.descent));
                continue;
            }
            let shaped = font.shape_runs(text, node.text.clone(), &st.face, st.size, st.tracking);
            for r in shaped {
                ascent = ascent.max(r.ascent);
                descent = descent.max(r.descent);
                runs.push(PaintRun {
                    family: font.face_family(r.face),
                    face: match font.font_face(r.face) {
                        Some(f) => f,
                        None => continue,
                    },
                    em: r.size * k,
                    glyphs: r.glyphs,
                    advances: r.advances.iter().map(|a| a * k).collect(),
                    offsets: r.offsets,
                    x: (left + slot.x) * k,
                    baseline: 0.0,
                    color: st.color,
                });
            }
        }
        let natural = ascent + descent;
        let line_h = (size * leading.for_mixed(mixed)).max(natural * 1.02);
        let baseline = y + (line_h - natural) * 0.5 + ascent;
        for run in runs.iter_mut() {
            run.baseline = baseline * k;
        }
        ops.push(Op::Runs(runs));
        for (file, x, w, a, d) in images_out.drain(..) {
            ops.push(Op::Image { path: file, x, y: (baseline - a) * k, w, h: (a + d) * k });
        }
        panel_bottom = y + line_h;
        y += line_h;
    }

    if let Some(role) = bg_role {
        let pad = theme.base * 0.45;
        ops.insert(
            ops.len().saturating_sub(plan.lines.len()),
            Op::Rect {
                x: (left - pad) * k,
                y: (panel_top - pad * 0.6) * k,
                w: (column + pad * 2.0) * k,
                h: (panel_bottom - panel_top + pad * 1.2) * k,
                color: role,
            },
        );
    }
    if b.quote_depth > 0 {
        ops.push(Op::Line {
            x0: (left - theme.base * 0.5) * k,
            y0: panel_top * k,
            x1: (left - theme.base * 0.5) * k,
            y1: panel_bottom * k,
            thickness: theme.base * 0.07 * k,
            color: ColorRole::Muted,
        });
    }
    panel_bottom
}

impl View {
    /// Client-space height of one text page, in points.
    fn page(&self) -> Pt {
        self.client_h / scale_of(self.dpi) * 0.85
    }

    fn thumb_rect(&self) -> Option<(f32, f32, f32, f32)> {
        thumb_rect(self.content_h, self.client_w, self.client_h, self.scroll, self.dpi)
    }

    fn thumb_hit(&self, x: f32, y: f32) -> bool {
        match self.thumb_rect() {
            Some((tx, ty, _tw, th)) => x >= tx - 8.0 && y >= ty - 8.0 && y <= ty + th + 8.0,
            None => false,
        }
    }

    fn scroll_to_thumb(&mut self, pointer_y: f32) {
        let Some((_, _, _, th)) = self.thumb_rect() else { return };
        let view = self.client_h;
        let max_scroll = (self.content_h - view / scale_of(self.dpi)).max(0.0);
        let room = (view - th).max(1.0);
        let centre = (pointer_y - th * 0.5).clamp(0.0, room);
        self.scroll = centre / room * max_scroll;
    }

    /// Open the first dropped file; a reader shows one document at a time.
    unsafe fn open_from_drop(&mut self, hwnd: HWND, hdrop: usize) {
        let h = HDROP(hdrop as *mut _);
        if DragQueryFileW(h, u32::MAX, None) < 1 {
            DragFinish(h);
            return;
        }
        let mut buf = [0u16; 1024];
        let written = DragQueryFileW(h, 0, Some(&mut buf)) as usize;
        DragFinish(h);
        let path = String::from_utf16_lossy(&buf[..written]);
        let path = path.trim_end_matches(char::from(0)).to_string();
        self.load_document(std::path::Path::new(&path));
        let _ = InvalidateRect(Some(hwnd), None, false);
    }

    /// Replace the open document, resetting the view to its top.
    fn load_document(&mut self, path: &std::path::Path) {
        match std::fs::read_to_string(path) {
            Ok(src) => {
                self.doc = rubrica_doc::Document::parse(&src);
                self.path = Some(path.to_path_buf());
                self.scroll = 0.0;
                self.relayout();
            }
            Err(e) => eprintln!("cannot open {}: {e}", path.display()),
        }
    }

    /// The common dialog. Returns the chosen path, if the user did not cancel.
    unsafe fn prompt_for_file(&self, hwnd: HWND) -> Option<PathBuf> {
        let mut buf = [0u16; 1024];
        let filter = utf16("Markdown\0*.md;*.markdown;*.txt\0All files\0*.*\0\0");
        let mut ofn = OPENFILENAMEW {
            lStructSize: std::mem::size_of::<OPENFILENAMEW>() as u32,
            hwndOwner: hwnd,
            lpstrFilter: PCWSTR(filter.as_ptr()),
            lpstrFile: windows::core::PWSTR(buf.as_mut_ptr()),
            nMaxFile: buf.len() as u32,
            Flags: OFN_FILEMUSTEXIST | OFN_HIDEREADONLY | OFN_PATHMUSTEXIST,
            ..Default::default()
        };
        if GetOpenFileNameW(&mut ofn).as_bool() {
            let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
            Some(PathBuf::from(String::from_utf16_lossy(&buf[..end])))
        } else {
            None
        }
    }

    fn paint(&mut self) {
        let Some(target) = self.target.clone() else { return };
        unsafe {
            self.ensure_brushes();
            target.BeginDraw();
            let bg = d2d(self.palette.bg);
            target.Clear(Some(&bg));
            let scroll_dip = self.scroll * scale_of(self.dpi);
            let top = -scroll_dip;
            let bottom = self.client_h - scroll_dip;
            for op in self.ops.iter() {
                match op {
                    Op::Rect { x, y, w, h, color } => {
                        if y + h < top || *y > bottom {
                            continue;
                        }
                        let role = *color;
                        if let Some(brush) = self.brushes.get(&role).cloned() {
                            let r = D2D_RECT_F {
                                left: *x,
                                top: *y,
                                right: x + w,
                                bottom: y + h,
                            };
                            target.FillRectangle(&r, &brush);
                        }
                    }
                    Op::Line { x0, y0, x1, y1, thickness, color } => {
                        if (*y1).max(*y0) < top || (*y0).min(*y1) > bottom {
                            continue;
                        }
                        let role = *color;
                        if let Some(brush) = self.brushes.get(&role).cloned() {
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
                        if y + h < top || *y > bottom {
                            continue;
                        }
                        let (Some(t), Some(store)) = (self.target.clone(), self.images.as_ref()) else {
                            continue;
                        };
                        if let Some(bmp) = store.bitmap(&t, path) {
                            let r = D2D_RECT_F { left: *x, top: *y, right: x + w, bottom: y + h };
                            target.DrawBitmap(
                                &bmp,
                                Some(&r),
                                1.0,
                                D2D1_BITMAP_INTERPOLATION_MODE_LINEAR,
                                None,
                            );
                        }
                    }
                    Op::Runs(runs) => {
                        for run in runs {
                            if run.baseline < top || run.baseline > bottom + 40.0 {
                                continue;
                            }
                            let role = run.color;
                            let Some(brush) = self.brushes.get(&role).cloned() else { continue };
                            let gr = DWRITE_GLYPH_RUN {
                                fontFace: std::mem::ManuallyDrop::new(Some(run.face.clone())),
                                fontEmSize: run.em,
                                glyphCount: run.glyphs.len() as u32,
                                glyphIndices: run.glyphs.as_ptr(),
                                glyphAdvances: run.advances.as_ptr(),
                                glyphOffsets: run.offsets.as_ptr(),
                                isSideways: false.into(),
                                bidiLevel: 0,
                            };
                            target.DrawGlyphRun(
                                Vector2::new(run.x, run.baseline),
                                &gr,
                                &brush,
                                DWRITE_MEASURING_MODE_NATURAL,
                            );
                            let _ = std::mem::ManuallyDrop::into_inner(gr.fontFace);
                        }
                    }
                }
            }
            let _ = target.EndDraw(None, None);
        }
    }
}

/// Intern a style for an inline object, which has no font properties at all.
/// Lay out a grid.
///
/// Columns size to their widest cell, which is what makes a short table look
/// deliberate rather than striped; the grid only gets squeezed when it genuinely
/// will not fit the measure, and then it is squeezed in proportion to how far each
/// column is above a floor that can still hold a short word. Cell heights are
/// measured from the wrapped result, never assumed, so a cell that wraps to four
/// lines grows its row instead of overwriting the row below it.
fn layout_table(
    font: &mut FontEngine,
    theme: &Theme,
    styles: &[AppStyle],
    t: &PreparedTable,
    left: Pt,
    column: Pt,
    ops: &mut Vec<Op>,
    mut y: Pt,
    k: f32,
) -> Pt {
    let size = theme.base;
    let spacing = Spacing::for_size(size);
    let leading = theme.body_leading.for_mixed(true);
    let pad = size * CELL_PAD_EM;
    let cols = t.head.len().max(t.rows.iter().map(|r| r.len()).max().unwrap_or(0));
    if cols == 0 {
        return y;
    }

    /// The style a cell's text should be measured at when it carries no spans.
    fn base_of(c: &PreparedCell) -> StyleId {
        c.spans.first().map_or(StyleId(0), |s| s.style)
    }

    /// Set a cell inside `inner` points of width.
    fn set_cell(
        font: &mut FontEngine,
        c: &PreparedCell,
        spacing: &Spacing,
        inner: Pt,
    ) -> (rubrica_type::Paragraph, rubrica_type::Plan) {
        let mut opts = BreakOptions::new(inner);
        // A cell is never justified: stretching a short label to fill a wide column
        // would tear its words apart.
        opts.ragged = true;
        typeset(&c.text, spacing, base_of(c), &c.spans, &opts, font)
    }

    let inner_of = |w: Pt| (w - pad * 2.0).max(size * 1.5);

    // Pass 1: natural width of each column, measured with no break opportunities.
    let mut widths = vec![0.0f32; cols];
    let mut measure = |widths: &mut Vec<f32>, cells: &[PreparedCell]| {
        for (i, c) in cells.iter().enumerate().take(cols) {
            let (para, _) = typeset(
                &c.text,
                &spacing,
                base_of(c),
                &c.spans,
                &BreakOptions::new(Pt::MAX / 4.0),
                font,
            );
            let w: Pt = para
                .items
                .iter()
                .map(|it| match *it {
                    Item::Box { node } => para.node(node).advance,
                    Item::Glue { base, .. } => base,
                    Item::Penalty { width, .. } => width,
                })
                .sum::<Pt>()
                + pad * 2.0;
            widths[i] = widths[i].max(w);
        }
    };
    measure(&mut widths, &t.head);
    for r in &t.rows {
        measure(&mut widths, r);
    }

    let mut total: Pt = widths.iter().sum();
    if total > column {
        let floor = size * CELL_MIN_EM + pad * 2.0;
        let excess: Pt = widths.iter().map(|w| (w - floor).max(0.0)).sum();
        if excess > 0.0 {
            let take = (total - column).min(excess);
            for w in widths.iter_mut() {
                let e = (*w - floor).max(0.0);
                *w -= take * (e / excess);
            }
            total = widths.iter().sum();
        }
    }

    let mut paint_row = |cells: &[PreparedCell], top: Pt, ops: &mut Vec<Op>| -> Pt {
        // Measure every cell first: the row is as tall as its tallest cell.
        let mut heights = vec![0.0f32; cols];
        for (i, c) in cells.iter().enumerate().take(cols) {
            let (_, plan) = set_cell(font, c, &spacing, inner_of(widths[i]));
            heights[i] = plan.lines.len().max(1) as Pt * size * leading;
        }
        let row_h = heights.iter().copied().fold(pad, f32::max) + pad;

        let mut x = left;
        for (i, c) in cells.iter().enumerate().take(cols) {
            let inner = inner_of(widths[i]);
            let (para, plan) = set_cell(font, c, &spacing, inner);
            let mut ly = top + pad * 0.5;
            for line in &plan.lines {
                let placed = place(&para, line);
                let w = placed.last().map(|p| p.x + p.w).unwrap_or(0.0);
                let shift = match c.align {
                    Align::Left => 0.0,
                    Align::Center => (inner - w) * 0.5,
                    Align::Right => (inner - w).max(0.0),
                };
                let mut runs: Vec<PaintRun> = Vec::new();
                let mut ascent = 0.0f32;
                for slot in placed {
                    let Some(node_id) = slot.node else { continue };
                    let node = para.node(node_id);
                    let st = &styles[node.style.0 as usize];
                    for r in font.shape_runs(&c.text, node.text.clone(), &st.face, st.size, st.tracking) {
                        ascent = ascent.max(r.ascent);
                        let Some(face) = font.font_face(r.face) else { continue };
                        runs.push(PaintRun {
                            family: font.face_family(r.face),
                            face,
                            em: r.size * k,
                            glyphs: r.glyphs,
                            advances: r.advances.iter().map(|a| a * k).collect(),
                            offsets: r.offsets,
                            x: (x + pad + shift + slot.x) * k,
                            baseline: 0.0,
                            color: st.color,
                        });
                    }
                }
                for run in runs.iter_mut() {
                    run.baseline = (ly + ascent) * k;
                }
                if !runs.is_empty() {
                    ops.push(Op::Runs(runs));
                }
                ly += size * leading;
            }
            x += widths[i];
        }
        row_h
    };

    let grid_w = total.min(column);
    ops.push(Op::Rect { x: left * k, y: y * k, w: grid_w * k, h: 0.0, color: ColorRole::Surface });
    let panel = ops.len() - 1;

    let head_h = paint_row(&t.head, y, ops);
    y += head_h;
    ops.push(Op::Line {
        x0: left * k,
        y0: y * k,
        x1: (left + grid_w) * k,
        y1: y * k,
        thickness: size * 0.05 * k,
        color: ColorRole::Muted,
    });
    for r in &t.rows {
        y += paint_row(r, y, ops);
    }
    // The header panel is drawn before its text, so its height can only be filled
    // in once the first row has been measured.
    if let Some(Op::Rect { h, .. }) = ops.get_mut(panel) {
        *h = head_h * k;
    }
    y
}

fn intern_object(styles: &mut Vec<AppStyle>, path: PathBuf, object: ObjectBox) -> StyleId {
    let app = AppStyle {
        face: FaceRequest {
            family: String::new(),
            cjk_family: String::new(),
            fallback: vec![],
            weight: 400,
            italic: false,
        },
        size: 0.0,
        tracking: 0.0,
        color: ColorRole::Text,
        object: Some(object),
        image: Some(path),
    };
    if let Some(i) = styles.iter().position(|s| *s == app) {
        return StyleId(i as u16);
    }
    styles.push(app);
    StyleId((styles.len() - 1) as u16)
}

fn intern(styles: &mut Vec<AppStyle>, fallback: &[String], r: crate::theme::ResolvedStyle) -> StyleId {
    let app = AppStyle {
        face: FaceRequest {
            family: r.family,
            cjk_family: r.cjk_family,
            fallback: fallback.to_vec(),
            weight: r.weight,
            italic: r.italic,
        },
        size: r.size,
        tracking: r.tracking,
        color: r.color,
        object: None,
        image: None,
    };
    if let Some(i) = styles.iter().position(|s| *s == app) {
        return StyleId(i as u16);
    }
    styles.push(app);
    StyleId((styles.len() - 1) as u16)
}

fn marker_for(b: &Block) -> String {
    match b.list {
        Some(l) if l.ordered => format!("{}. ", l.index.unwrap_or(1)),
        Some(_) => "\u{2022} ".to_string(),
        None => String::new(),
    }
}
/// Typeset the whole document into a display list.
///
/// Shared verbatim by the window and by `rubrica-app --report`, so a numeric check
/// describes exactly what the user sees rather than what a parallel implementation
/// would have drawn.
///
/// Returns the ops, the document height, the chosen measure and its left edge, all
/// in points except the ops which are in device independent pixels.
pub fn build_ops(
    font: &mut FontEngine,
    theme: &Theme,
    doc: &Document,
    client_w: f32,
    dpi: f32,
    images: Option<&ImageStore>,
    base_dir: Option<&std::path::Path>,
) -> (Vec<Op>, Pt, Pt, Pt) {
    let k = scale_of(dpi);
    let margin = theme.base * MARGIN_EM;
    let client_pt = client_w / k;
    let avail = (client_pt - margin * 2.0).max(theme.base * 6.0);
    let column = (theme.max_measure_em * theme.base).min(avail);
    // The measure is capped so a wide window does not produce a 90-character
    // line; the leftover has to go somewhere, and equal margins keep the page
    // centred rather than leaving a bare gutter on the right.
    let left = margin + (avail - column) * 0.5;

    let mut styles: Vec<AppStyle> = Vec::new();
    let mut ops = Vec::new();
    let mut y = theme.base * 2.0;
    let mut first = true;

    // Interning has to finish before measuring, because the engine resolves a
    // StyleId through the installed table.
    let mut prepared: Vec<(String, Vec<StyleSpan>, usize, Option<PreparedTable>)> = Vec::new();
    for b in &doc.blocks {
        let marker = marker_for(b);
        let shift = marker.len();
        let mut text = marker;
        text.push_str(&b.text);
        let mut spans: Vec<StyleSpan> = Vec::with_capacity(b.spans.len() + 1);
        if shift > 0 {
            let id = intern(&mut styles, &theme.fonts.fallback, theme.resolve(BlockKind::Paragraph, InlineStyle::EMPTY));
            spans.push(StyleSpan { range: 0..shift, style: id });
        }
        let base = intern(&mut styles, &theme.fonts.fallback, theme.resolve(b.kind, InlineStyle::EMPTY));
        for s in &b.spans {
            // An object span is sized from its own file rather than from a font,
            // and only the caller knows the document directory a relative source
            // should resolve against.
            let id = match s.style {
                st if st.contains(InlineStyle::OBJECT) => {
                    let obj = b.objects.iter().find(|o| o.range == s.range);
                    match (obj, images) {
                        (Some(o), Some(store)) => {
                            let src = match &o.kind {
                                rubrica_doc::ObjectKind::Image { src, .. } => src,
                            };
                            let path = ImageStore::resolve(base_dir, src);
                            match store.fit(&path, column) {
                                // Sit the figure on the baseline with a small
                                // descent so it does not ride above the text.
                                Some((w, h)) => {
                                    let descent = (h * 0.15).min(theme.base * 0.4);
                                    intern_object(&mut styles, path, ObjectBox {
                                        advance: w,
                                        ascent: h - descent,
                                        descent,
                                    })
                                }
                                None => {
                                    // A missing figure keeps measuring nothing, and
                                    // is reported once. Swapping in the alt text here
                                    // would shift every later span offset, since the
                                    // block's text and ranges were built already.
                                    eprintln!("image not rendered: {}", path.display());
                                    base
                                }
                            }
                        }
                        // A missing store means COM or WIC could not be started,
                        // which is a different failure from one bad file and has to
                        // be reported as such rather than passed off as prose.
                        (Some(_), None) => {
                            eprintln!("figures disabled: no image decoder");
                            base
                        }
                        _ => base,
                    }
                }
                _ => intern(&mut styles, &theme.fonts.fallback, theme.resolve(b.kind, s.style)),
            };
            spans.push(StyleSpan { range: s.range.start + shift..s.range.end + shift, style: id });
        }
        let table = b.table.as_ref().map(|t| {
            let mut prep = |cells: &[rubrica_doc::Cell]| -> Vec<PreparedCell> {
                cells
                    .iter()
                    .enumerate()
                    .map(|(i, c)| PreparedCell {
                        text: c.text.clone(),
                        spans: c
                            .spans
                            .iter()
                            .map(|sp| StyleSpan {
                                range: sp.range.clone(),
                                style: intern(
                                    &mut styles,
                                    &theme.fonts.fallback,
                                    theme.resolve(b.kind, sp.style),
                                ),
                            })
                            .collect(),
                        align: t.aligns.get(i).copied().unwrap_or_default(),
                    })
                    .collect()
            };
            PreparedTable { head: prep(&t.head), rows: t.rows.iter().map(|r| prep(r)).collect() }
        });
        prepared.push((text, spans, base.0 as usize, table));
    }

    font.begin_layout(
        styles
            .iter()
            .map(|s| RunStyle { face: s.face.clone(), size: s.size, tracking: s.tracking, object: s.object })
            .collect(),
    );

    for (b, (text, spans, base, table)) in doc.blocks.iter().zip(prepared) {
        let base_left = left;
        let size = theme.body_size(b.kind);
        y += theme.space_before(b.kind, first);
        first = false;
        let left = base_left
            + b.quote_depth as Pt * theme.quote_indent_em * theme.base
            + b.list.map_or(0.0, |l| l.depth as Pt * theme.list_indent_em * size);
        let body_left = if b.list.is_some() {
            left + theme.list_indent_em * size * 0.9
        } else {
            left
        };
        y = layout_block(
            font,
            &Ctx { theme, styles: &styles, k },
            &Blk {
                b,
                text: &text,
                spans: &spans,
                base,
                left: body_left,
                column: column - (body_left - left),
                table: table.as_ref(),
            },
            &mut ops,
            y,
        );
    }

(ops, y + theme.base * 2.0, column, left)
}

#[cfg(test)]
mod tests {
    use super::*;

    const DPI: f32 = 96.0;

    #[test]
    fn no_thumb_when_the_document_fits() {
        // 800 DIP of viewport at 96 dpi is 600pt of page.
        assert!(thumb_rect(500.0, 1000.0, 800.0, 0.0, DPI).is_none());
        assert!(thumb_rect(600.0, 1000.0, 800.0, 0.0, DPI).is_none());
    }

    #[test]
    fn thumb_height_is_the_viewport_fraction_of_the_document() {
        let (_, _, _, h) = thumb_rect(2400.0, 1000.0, 800.0, 0.0, DPI).unwrap();
        // doc = 2400pt * 1.3333 = 3200dip; 800*800/3200 = 200
        assert!((h - 200.0).abs() < 0.5, "thumb {h}");
    }

    #[test]
    fn thumb_travel_maps_onto_the_scrollable_range() {
        let content = 2400.0;
        let view = 800.0;
        let (_, y0, _, h) = thumb_rect(content, 1000.0, view, 0.0, DPI).unwrap();
        assert!(y0.abs() < 0.01, "at the top the thumb must sit at 0, got {y0}");
        let max_scroll = content - view / scale_of(DPI);
        let (_, y_end, _, _) = thumb_rect(content, 1000.0, view, max_scroll, DPI).unwrap();
        assert!((y_end - (view - h)).abs() < 0.5, "at the bottom the thumb must reach the track end: {y_end} vs {}", view - h);
        // Midway in the scroll range is midway along the track.
        let (_, y_mid, _, _) = thumb_rect(content, 1000.0, view, max_scroll * 0.5, DPI).unwrap();
        assert!((y_mid - (view - h) * 0.5).abs() < 1.0, "mid scroll -> {y_mid}");
    }

    #[test]
    fn thumb_stays_inside_the_client_width() {
        let (x, _, track, _) = thumb_rect(2400.0, 400.0, 800.0, 0.0, DPI).unwrap();
        assert!(x + track <= 400.0, "thumb overhangs the window: {x}+{track}");
        assert!(x > 0.0);
    }

    #[test]
    fn scrolling_past_the_end_clamps_instead_of_leaving_the_track() {
        let (_, y_over, _, h) = thumb_rect(2400.0, 1000.0, 800.0, 99_000.0, DPI).unwrap();
        assert!(y_over + h <= 800.0 + 0.5, "thumb left the viewport: {y_over}+{h}");
    }
}
