//! Window host and Direct2D renderer.
//!
//! The display list is built in document points, then scaled to device independent
//! pixels whenever the client width or the window DPI changes. That split is
//! deliberate: `DrawGlyphRun` takes advance widths in the same units as its
//! `fontEmSize`, so scaling at paint time would mean copying every advance array on
//! every frame. Doing it once per relayout keeps scrolling allocation-free.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use rubrica_doc::{Action, ActionKind, Align, Block, BlockKind, Document, InlineStyle};
use rubrica_type::justification::place_bidi;
use rubrica_type::paragraph::{Item, Spacing, StyleId, StyleSpan};
use rubrica_type::units::Pt;
use rubrica_type::{BreakOptions, Hyphenation, typeset, typeset_hyphenated};
use printpdf::{
    Actions as PdfActions, BuiltinFont, Color as PdfColor, Codepoint, FontId as PdfFontId,
    LinkAnnotation as PdfLinkAnnotation, Op as PdfOp, ParsedFont, PdfDocument,
    PdfFontHandle, PdfPage, PdfSaveOptions, PdfParseErrorSeverity, Point as PdfPoint, RawImage,
    Rect as PdfRect, DictItem,
    Rgb as PdfRgb, TextItem, TextMatrix as PdfTextMatrix, XObjectTransform,
};
use windows::core::{w, BOOL, Interface, PCWSTR};
use windows::Win32::Foundation::GENERIC_WRITE;
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
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
use windows::Win32::Graphics::Imaging::{
    CLSID_WICImagingFactory, GUID_ContainerFormatPng, GUID_WICPixelFormat32bppPBGRA,
    IWICBitmapSource, IWICImagingFactory, WICBitmapCacheOnLoad, WICBitmapEncoderNoCache,
};
use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_INPROC_SERVER};
use windows::Win32::Graphics::DirectWrite::{
    DWRITE_GLYPH_OFFSET, DWRITE_GLYPH_RUN, DWRITE_MEASURING_MODE_NATURAL, IDWriteFontFace,
};
use windows::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWA_USE_IMMERSIVE_DARK_MODE};
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM;
use windows::Win32::Graphics::Gdi::{
    CLIP_DEFAULT_PRECIS, CreateFontW, CreateSolidBrush, DEFAULT_CHARSET, DEFAULT_QUALITY,
    DeleteObject, EnumDisplayMonitors, GetMonitorInfoW, HBRUSH, HDC, HFONT, HGDIOBJ, HMONITOR,
    InvalidateRect, MONITORINFO, OUT_DEFAULT_PRECIS, ScreenToClient, SetBkColor, SetTextColor,
};
use windows::Win32::System::Com::{
    CoInitializeEx, COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE, COINIT_MULTITHREADED,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Registry::{RRF_RT_DWORD, RegGetValueW, HKEY_CURRENT_USER};
use windows::Win32::UI::HiDpi::{
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, GetDpiForWindow, SetProcessDpiAwarenessContext,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetKeyState, ReleaseCapture, SetCapture, SetFocus, VK_CONTROL, VK_O, VK_UP,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{VK_DOWN, VK_ESCAPE, VK_NEXT, VK_PRIOR};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    VK_0, VK_A, VK_ADD, VK_C, VK_D, VK_END, VK_F, VK_F3, VK_HOME, VK_LEFT, VK_MENU, VK_NUMPAD0,
    VK_OEM_4, VK_OEM_6, VK_OEM_MINUS, VK_OEM_PLUS, VK_R, VK_RETURN, VK_RIGHT, VK_SHIFT,
    VK_SUBTRACT, VK_TAB, VK_W, VIRTUAL_KEY,
};
use windows::Win32::UI::Controls::EM_SETCUEBANNER;
use windows::Win32::UI::Controls::Dialogs::{
    GetOpenFileNameW, OPENFILENAMEW, OFN_FILEMUSTEXIST, OFN_HIDEREADONLY, OFN_PATHMUSTEXIST,
};
use windows::Win32::UI::Shell::{DragAcceptFiles, DragFinish, DragQueryFileW, ShellExecuteW, HDROP};
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CS_DBLCLKS, CS_HREDRAW, CS_VREDRAW, CREATESTRUCTW, CreatePopupMenu,
    CreateWindowExW, DefWindowProcW, DestroyMenu, DispatchMessageW,
    GWLP_USERDATA, GetClientRect, GetCursorPos, GetWindowLongPtrW, GetMessageW, GetWindowPlacement,
    HCURSOR, HMENU, HWND_TOP, HTCLIENT, IDC_ARROW, IDC_HAND, KillTimer, LoadCursorW, MF_CHECKED,
    MF_GRAYED, MF_POPUP, MF_SEPARATOR, MF_STRING, MSG, PostMessageW, PostQuitMessage,
    RegisterClassExW, SetCursor, SetForegroundWindow, SetWindowTextW, SW_SHOWNORMAL, SetTimer,
    SetWindowLongPtrW, SetWindowPos, ShowWindow, SW_SHOWMAXIMIZED, TPM_RETURNCMD,
    TPM_RIGHTBUTTON, TrackPopupMenuEx, TranslateMessage, WM_CONTEXTMENU, WM_NULL, WNDCLASSEXW,
    WM_DESTROY, WM_DPICHANGED, WM_ERASEBKGND, WM_EXITSIZEMOVE, WM_KEYDOWN, WM_MOUSEWHEEL,
    WM_NCCREATE, WM_PAINT, WM_SETCURSOR, WM_SIZE, WM_TIMER, WS_EX_APPWINDOW, WS_OVERLAPPEDWINDOW,
    SWP_NOACTIVATE, SWP_NOZORDER, WM_DROPFILES, WM_LBUTTONDOWN, WM_LBUTTONDBLCLK, WM_LBUTTONUP,
    WM_MOUSEMOVE, WM_SYSKEYDOWN, CallWindowProcW, EN_CHANGE, ES_AUTOHSCROLL, GetParent,
    GetWindowTextW, GWLP_WNDPROC, MoveWindow, SendMessageW, SW_HIDE, SW_SHOW, WM_CHAR, WM_COMMAND,
    WM_CTLCOLOREDIT, WM_GETTEXTLENGTH, WM_SETFONT, WINDOWPLACEMENT, WINDOW_STYLE, WNDPROC,
    WS_BORDER, WS_CHILD, WS_VISIBLE,
};
use windows_numerics::Vector2;

use crate::clipboard;
use crate::reading::{self, Encoding};
use rubrica_doc::plain::{ChapterIndex, ParagraphRule, TextOptions};
use crate::find::Needle;
use crate::font::{cjk_char, FaceRequest, FontEngine, GlyphRun, ObjectBox, Style as RunStyle};
use crate::hyphen::Hyphenator;
use crate::images::ImageStore;
use crate::math::MathStore;
use crate::tree::{self, Entry as TreeEntry, EntryKind as TreeEntryKind};
use rubrica_workspace::{DocumentRef, FileRef, TabId, TabKind, Workspace};
use crate::theme::{ColorRole, Leading, Measure, Role, TextFace, Theme, Zoom};
use crate::{Error, Result};

/// The character drawn at a discretionary break, and the range to shape it from.
const HYPHEN: &str = "-";
const HYPHEN_RANGE: std::ops::Range<usize> = 0..1;

/// Page margin, in ems of the body size.
const MARGIN_EM: Pt = 2.6;
/// Wheel notch travel, in points.
const WHEEL_STEP: Pt = 72.0;
/// Appearance is polled, not pushed; see `WM_TIMER` below.
const APPEARANCE_TIMER: usize = 0x5140;
const APPEARANCE_TICK_MS: u32 = 400;
/// The page on disk is polled for the same reason: there is no message for a file being
/// written. `ReadDirectoryChangesW` would answer the question as it is asked, at the cost
/// of a thread and a wait handle wired into the message loop, to learn something a
/// `GetFileTime` gives in a microsecond.
const DOCUMENT_TIMER: usize = 0x5141;
const DOCUMENT_TICK_MS: u32 = 700;
/// How far the pointer has to travel, in device pixels, before a held left button stops
/// meaning "here" and starts meaning "from here to there".
const DRAG_SLOP: f32 = 3.0;
/// The window a first run is given: a little way in from the top-left of the screen, wide
/// enough for the design's measure plus a margin either side of it.
///
/// Not `CW_USEDEFAULT`, which is the system's answer and cascades off a position this
/// program never knows: a window that moves when nobody moved it is a window the reader has
/// to chase with `Alt`+`Tab` to find.
const FIRST_FRAME: crate::settings::Frame =
    crate::settings::Frame { left: 120, top: 100, width: 1080, height: 800, maximised: false };

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
    keyword: Rgb,
    string: Rgb,
    comment: Rgb,
    number: Rgb,
    ty: Rgb,
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
                // A code panel is darker than the page, so the inks on it are lit
                // rather than dyed: the night page's own text is already 0.855, and a
                // highlight that stayed below it would read as text that had gone off.
                keyword: Rgb { r: 0.78, g: 0.61, b: 0.94 },
                string: Rgb { r: 0.87, g: 0.69, b: 0.51 },
                comment: Rgb { r: 0.52, g: 0.58, b: 0.55 },
                number: Rgb { r: 0.55, g: 0.79, b: 0.75 },
                ty: Rgb { r: 0.62, g: 0.80, b: 0.58 },
            }
        } else {
            Palette {
                dark,
                bg: Rgb { r: 0.977, g: 0.974, b: 0.966 },
                text: Rgb { r: 0.13, g: 0.135, b: 0.15 },
                muted: Rgb { r: 0.42, g: 0.44, b: 0.47 },
                accent: Rgb { r: 0.12, g: 0.35, b: 0.66 },
                code_bg: Rgb { r: 0.937, g: 0.935, b: 0.928 },
                // All five are dark enough to hold their own against the panel's 0.93
                // and far enough apart in hue that a keyword, a string and a number
                // never pass for one another at a glance.
                keyword: Rgb { r: 0.45, g: 0.20, b: 0.55 },
                string: Rgb { r: 0.56, g: 0.28, b: 0.13 },
                comment: Rgb { r: 0.40, g: 0.46, b: 0.42 },
                number: Rgb { r: 0.09, g: 0.40, b: 0.44 },
                ty: Rgb { r: 0.16, g: 0.40, b: 0.22 },
            }
        }
    }

    fn ink(&self, role: ColorRole) -> Rgb {
        match role {
            ColorRole::Text | ColorRole::Code => self.text,
            ColorRole::Muted | ColorRole::Faint => self.muted,
            ColorRole::Accent => self.accent,
            ColorRole::Surface => self.code_bg,
            ColorRole::Keyword => self.keyword,
            ColorRole::String => self.string,
            ColorRole::Comment => self.comment,
            ColorRole::Number => self.number,
            ColorRole::Type => self.ty,
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
    /// Lift off the line's baseline, positive upwards, matching
    /// [`crate::theme::ResolvedStyle::raise`]. Zero for prose; the painter turns it
    /// into the run's negative drop.
    ///
    /// It is not part of [`RunStyle`], because raising a run moves where it is drawn
    /// and not how wide it is: the line must break on the same advance the page paints.
    raise: Pt,
    /// Set instead of font properties for an inline object such as an image.
    object: Option<ObjectBox>,
    /// What an object style draws, once its box is known.
    source: Option<ObjectSource>,
    /// Strike the run's own ink, per face: like `raise`, this is painting rather
    /// than measuring, so it stays out of [`RunStyle`].
    strike: bool,
}

/// The thing behind an object placeholder.
#[derive(Clone, Debug, PartialEq)]
enum ObjectSource {
    Image(PathBuf),
    /// Index into the formula cache, which holds the pieces already shaped: a
    /// formula is typeset while the line's measure is worked out, and painting it
    /// must not run the layout a second time.
    Math(usize),
}

/// One face's positioned glyphs, in device independent pixels.
#[derive(Clone)]
pub struct PaintRun {
    pub bidi_level: u8,
    pub(crate) face: IDWriteFontFace,
    pub(crate) face_index: usize,
    /// Resolved family, recorded at layout time so the report can show which face
    /// actually carried each run without a COM round trip per frame.
    pub family: String,
    pub em: f32,
    pub glyphs: Vec<u16>,
    pub advances: Vec<f32>,
    pub(crate) offsets: Vec<DWRITE_GLYPH_OFFSET>,
    /// The source slice that produced this run, when it came from document text.
    /// PDF export needs it to give each glyph a Unicode value; a formula's synthetic
    /// assembly has no one-to-one source range and leaves it empty.
    pub(crate) source: Option<String>,
    /// DirectWrite's raw UTF-16-unit → glyph map. It is compact and is interpreted only by
    /// the PDF exporter, so normal window layout does not allocate one Unicode string per
    /// glyph.
    pub(crate) clusters: Vec<u16>,
    pub x: f32,
    pub baseline: f32,
    /// Extra drop below the line's baseline, positive downwards. Zero for prose; a
    /// formula's pieces each sit somewhere of their own within its box.
    pub(crate) dy: f32,
    /// Which of the theme's inks carries this run, recorded for the same reason
    /// `family` is: the report can then say what the page actually painted rather
    /// than what the layout asked for.
    pub color: ColorRole,
}

/// DirectWrite places an odd-level run from its right edge; the display list and
/// all hit geometry continue to store the physical left edge.
pub(crate) fn glyph_origin(left: f32, width: f32, level: u8) -> f32 {
    left + if level % 2 == 1 { width } else { 0.0 }
}

#[derive(Clone)]
pub enum Op {
    Runs(Vec<PaintRun>),
    /// An inline object, positioned in device independent pixels.
    Image { path: PathBuf, x: f32, y: f32, w: f32, h: f32 },
    Rect { x: f32, y: f32, w: f32, h: f32, color: ColorRole },
    Line { x0: f32, y0: f32, x1: f32, y1: f32, thickness: f32, color: ColorRole },
}

/// A piece of content wider than the reading column and the interaction it owns.
#[derive(Clone, Debug, PartialEq)]
pub enum WideKind {
    Formula(usize),
    Image(PathBuf),
    Table,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Preview {
    Image(PathBuf),
    Formula(usize),
}

#[derive(Clone, Debug, PartialEq)]
pub struct WideRegion {
    pub kind: WideKind,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub content_w: f32,
}

/// Where a click lands, and what it means.
#[derive(Clone, Debug, PartialEq)]
pub enum HotKind {
    /// A link to open, already accepted by [`openable`].
    Url(String),
    /// A citation of the footnote at this index, whose top of the page is in
    /// [`Page::note_tops`]. The note is on the page already, so a citation is jumped to
    /// rather than opened somewhere else.
    Cite(usize),
    /// A fragment link naming the heading at this index, whose top is in
    /// [`Page::anchor_tops`]. A table of contents is only worth setting if clicking it
    /// goes somewhere.
    Heading(usize),
    /// A link to another Markdown file beside this one, already resolved to a path that
    /// exists. The reader is a document reader, so the answer to such a link is to read
    /// that document rather than to hand the path to some other program.
    Document(DocumentTarget),
    /// A wide object or table; the index names its [`Page::wide_regions`] entry.
    Wide(usize),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DocumentTarget {
    pub path: PathBuf,
    pub fragment: Option<String>,
}

fn fragment_slug(fragment: &str) -> String {
    slug(&crate::images::percent_decode(fragment))
}

fn heading_index(doc: &Document, fragment: &str) -> Option<usize> {
    let name = fragment_slug(fragment);
    doc.blocks.iter().filter(|b| matches!(b.kind, BlockKind::Heading(_)))
        .position(|b| slug(&b.text) == name)
}

/// A rectangle of the document a click can land on, in the same device independent
/// pixels the [`Op`]s are in and measured from the top of the page rather than of the
/// window -- so hit-testing is the pointer's y plus the scroll, and nothing else.
///
/// One per line rather than one per link: a target that wraps is in two places at
/// once, and the pointer is only ever in one of them.
pub struct Hot {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub kind: HotKind,
}

/// How a line's text is set off from the line before it when a selection is read back.
/// The lines of a paragraph need nothing -- the space or the break the author wrote is
/// already carried at the start of the next line -- but two table cells that meet on
/// the page are columns, and two blocks are paragraphs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Join {
    None,
    Tab,
    Newline,
    /// A blank line between, which is what separates one block of prose from the next.
    Blank,
}

/// One drawn line, indexed by character so a drag can begin anywhere inside it and
/// what comes back is what the author wrote rather than what the shaping kept.
///
/// Geometry is per character boundary rather than per run because a run is a
/// shaping artefact: it splits where the script changes, so a word of one language
/// can be drawn in two pieces while still being one word to select.
pub struct SelLine {
    pub source: Option<std::ops::Range<usize>>,
    /// Top edge and height, in the same device independent pixels as [`Hot`] and
    /// measured from the top of the document.
    pub y: f32,
    pub h: f32,
    /// What separates this line from the one before it in the text a copy hands back.
    pub join: Join,
    pub chars: Vec<char>,
    /// Markdown to copy in place of an atomic object, indexed in logical characters.
    pub copies: Vec<(usize, String)>,
    /// Logical leading edges; bidi text can run in either direction.
    /// A step of no width is a character the source has and this line does not paint,
    /// which is what a wrapped line's space between two of its words looks like.
    pub xs: Vec<f32>,
    /// Logical trailing edge per character. At a bidi boundary this need not be
    /// the next character's leading edge: selections can have disjoint rectangles.
    pub ends: Vec<f32>,
}

/// A place in the page's text: a character index in a [`SelLine`], where `ch` equal to
/// the line's length means after its last character.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Caret {
    pub line: usize,
    pub ch: usize,
}

/// The two ends of a drag, in whatever order the pointer made them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Selection {
    pub from: Caret,
    pub to: Caret,
}

impl Selection {
    /// Low end first, so neither the rectangles nor the text has to care which way
    /// the reader dragged.
    fn ordered(&self) -> Selection {
        let (a, b) = (self.from, self.to);
        if (a.line, a.ch) <= (b.line, b.ch) {
            Selection { from: a, to: b }
        } else {
            Selection { from: b, to: a }
        }
    }
}

/// Where a pointer at `x`, `y` (device pixels from the top of the document) lands in the
/// page's text. Above the first line and below the last both clamp rather
/// than miss: a drag thrown past the top of the page means all of it from the start.
pub fn caret_at(sel: &[SelLine], x: f32, y: f32) -> Caret {
    let Some(_) = sel.first() else {
        return Caret { line: 0, ch: 0 };
    };
    let mut line = sel.len() - 1;
    for (i, l) in sel.iter().enumerate() {
        if y < l.y + l.h {
            line = i;
            break;
        }
    }
    let l = &sel[line];
    // The nearest boundary, because a character's own ink starts a point or two to the
    // right of the slot it was allocated, and a click that means the next character
    // must not keep selecting the one before it.
    let ch = l.xs.iter().enumerate().min_by(|(i, a), (j, b)| {
        (*a - x).abs().total_cmp(&(*b - x).abs()).then(j.cmp(i))
    }).map_or(0, |(i, _)| i);
    Caret { line, ch }
}

/// Visual rectangles for a logical selection; a bidi line can need several.
pub fn selection_rects(sel: &[SelLine], s: Selection) -> Vec<(f32, f32, f32, f32)> {
    let mut out = Vec::new();
    for (line, lo, hi) in pieces(sel, s) {
        let l = &sel[line];
        let mut spans: Vec<_> = (0..l.chars.len()).map(|i| {
            (l.xs[i].min(l.ends[i]), l.xs[i].max(l.ends[i]), (lo..hi).contains(&i))
        }).filter(|(a, b, _)| b > a).collect();
        spans.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut merged: Vec<(f32, f32)> = Vec::new();
        let mut adjacent = false;
        for (a, b, selected) in spans {
            if !selected {
                adjacent = false;
                continue;
            }
            // Synthetic CJK/script glue has no source character, but still belongs
            // under the band when both adjacent pieces of ink are selected.
            if let Some(last) = merged.last_mut().filter(|_| adjacent) {
                last.1 = last.1.max(b);
            } else {
                merged.push((a, b));
            }
            adjacent = true;
        }
        out.extend(merged.into_iter().map(|(a, b)| (a, l.y, b - a, l.h)));
    }
    out
}

/// Which characters of each line a selection takes, as `(line, from, to)` in page order.
///
/// Both readings of a selection start here, because the two must agree exactly: a band
/// the reader can see that copies nothing, or a character that copies without ever being
/// highlighted, is the same bug told from the other side.
fn pieces(sel: &[SelLine], s: Selection) -> Vec<(usize, usize, usize)> {
    let s = s.ordered();
    if s.from == s.to {
        return Vec::new();
    }
    let mut out = Vec::new();
    let last = s.to.line.min(sel.len().saturating_sub(1));
    for (line, l) in sel.iter().enumerate().take(last + 1).skip(s.from.line) {
        let lo = if line == s.from.line { s.from.ch } else { 0 };
        let hi = if line == s.to.line { s.to.ch } else { l.chars.len() };
        let (lo, hi) = (lo.min(l.chars.len()), hi.min(l.chars.len()));
        if hi > lo {
            out.push((line, lo, hi));
        }
    }
    out
}

/// Read a selection back as text, with the separators [`Join`] records between the
/// pieces that need them.
///
/// A selection with no character in it copies nothing rather than the whole page: an
/// empty `to` would otherwise read every line below it, since a line the drag never
/// reached is by this arithmetic "wholly inside".
pub fn selection_text(sel: &[SelLine], s: Selection) -> String {
    let mut out = String::new();
    for (n, (line, lo, hi)) in pieces(sel, s).into_iter().enumerate() {
        let l = &sel[line];
        // A line that opens with the break the author wrote -- a `<br>` inside a cell,
        // carried into the text by the gap the index fills in -- already says where it
        // begins. Adding the row's own separator on top of it would copy a blank line
        // where the page shows a single break.
        if n > 0 && !matches!(l.chars.first(), Some('\n')) {
            match l.join {
                Join::None => {}
                Join::Tab => out.push('\t'),
                Join::Newline => out.push('\n'),
                Join::Blank => out.push_str("\n\n"),
            }
        }
        for (i, c) in l.chars.iter().enumerate().take(hi).skip(lo) {
            if let Some((_, text)) = l.copies.iter().find(|(at, _)| *at == i) {
                out.push_str(text);
            } else {
                out.push(*c);
            }
        }
    }
    out
}

/// What a double-click treats as one unit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WordKind {
    /// A space, or the gap a wrapped line leaves where its source space used to be.
    Space,
    /// A letter, a digit, and the marks inside a word: `-` in `well-formed`, `_` in a
    /// key name.
    Word,
    /// A Han, kana or fullwidth character. Selected one at a time: a line of them has
    /// no boundary for a click to land on, so a run that stopped at one character is
    /// the honest answer rather than a guess at the author's vocabulary.
    Ideograph,
    /// A brace, a quote, a comma. Its own unit, since it belongs to neither neighbour.
    Mark,
}

fn word_kind(c: char) -> WordKind {
    if c.is_whitespace() {
        WordKind::Space
    } else if crate::font::cjk_char(c) {
        WordKind::Ideograph
    } else if c.is_alphanumeric() {
        WordKind::Word
    } else if c == '_' || c == '-' || c == '’' {
        // The apostrophe that joins contractions is inside the word, not after it.
        WordKind::Word
    } else {
        WordKind::Mark
    }
}

/// The word a double-click selects: the run of like characters around the caret, on the
/// caret's own line.
///
/// A word never spans a line break here, because the index has no record of which lines
/// were one paragraph -- only of which lines were drawn -- and guessing from `Join`
/// would let a click on the last word of a heading reach into the paragraph under it.
pub fn word_at(sel: &[SelLine], c: Caret) -> Selection {
    let nothing = Selection { from: c, to: c };
    let Some(line) = sel.get(c.line) else { return nothing };
    let n = line.chars.len();
    if n == 0 {
        return nothing;
    }
    // The character the click fell inside. A caret at index `i` sits *before* character
    // `i`, so unless the click was at the very start the character to the left is the
    // one it was on.
    let mut at = c.ch.min(n).saturating_sub(1);
    if word_kind(line.chars[at]) == WordKind::Space {
        // Between two words: the one on the right, since that is the one a reader
        // aiming at a space means. Failing that, the one on the left. Marks are skipped
        // on this search -- a click in the gap after `end."` wants the word, not the
        // punctuation hanging off it -- while still selecting a mark it lands on.
        let word = |i: usize| matches!(word_kind(line.chars[i]), WordKind::Word | WordKind::Ideograph);
        match (at + 1..n).find(|&i| word(i)).or_else(|| (0..=at).rev().find(|&i| word(i))) {
            Some(i) => at = i,
            None => return nothing,
        }
    }
    let kind = word_kind(line.chars[at]);
    let (mut lo, mut hi) = (at, at + 1);
    if kind != WordKind::Ideograph {
        while lo > 0 && word_kind(line.chars[lo - 1]) == kind {
            lo -= 1;
        }
        while hi < n && word_kind(line.chars[hi]) == kind {
            hi += 1;
        }
    }
    Selection { from: Caret { line: c.line, ch: lo }, to: Caret { line: c.line, ch: hi } }
}

/// Where an arrow key takes the caret. `Ctrl` turns the sideways motions into word
/// motions and the rest into the ends of the document, which is what every other text
/// surface on this system means by them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Motion {
    Left,
    Right,
    Up,
    Down,
    Home,
    End,
    WordLeft,
    WordRight,
    DocStart,
    DocEnd,
}

impl Motion {
    /// Whether the motion reads backwards through the text, which is what decides which
    /// edge of a selection an arrow without `Shift` leaves the caret on.
    fn backward(self) -> bool {
        matches!(self, Motion::Left | Motion::WordLeft | Motion::Up | Motion::Home | Motion::DocStart)
    }
}

/// The key a reader pressed, as a motion -- or `None` when it is not one of the arrows.
fn motion_of(key: u32, ctrl: bool) -> Option<Motion> {
    Some(match key {
        k if k == VK_LEFT.0 as u32 => if ctrl { Motion::WordLeft } else { Motion::Left },
        k if k == VK_RIGHT.0 as u32 => if ctrl { Motion::WordRight } else { Motion::Right },
        k if k == VK_UP.0 as u32 => if ctrl { Motion::DocStart } else { Motion::Up },
        k if k == VK_DOWN.0 as u32 => if ctrl { Motion::DocEnd } else { Motion::Down },
        k if k == VK_HOME.0 as u32 => if ctrl { Motion::DocStart } else { Motion::Home },
        k if k == VK_END.0 as u32 => if ctrl { Motion::DocEnd } else { Motion::End },
        _ => return None,
    })
}

/// A place in the index that cannot fall off the page.
fn clamp_caret(sel: &[SelLine], c: Caret) -> Caret {
    if sel.is_empty() {
        return Caret { line: 0, ch: 0 };
    }
    let line = c.line.min(sel.len() - 1);
    Caret { line, ch: c.ch.min(sel[line].chars.len()) }
}

/// The kind of the character a motion is standing on: the one ahead of it reading
/// forwards, the one behind it reading backwards.
fn kind_ahead(sel: &[SelLine], c: Caret, forward: bool) -> Option<WordKind> {
    let i = if forward { c.ch } else { c.ch.checked_sub(1)? };
    sel.get(c.line)?.chars.get(i).copied().map(word_kind)
}

/// The caret one motion away, clamped to the page: an arrow at either end of a document
/// stays where it is rather than falling out of the index and taking the selection with
/// it.
pub fn next_caret(sel: &[SelLine], from: Caret, m: Motion) -> Caret {
    if sel.is_empty() {
        return Caret { line: 0, ch: 0 };
    }
    let at = clamp_caret(sel, from);
    let last = sel.len() - 1;
    let end_of = |line: usize| sel[line].chars.len();
    let current = &sel[at.line];
    if current.xs.windows(2).any(|w| w[1] < w[0]) {
        let mut order: Vec<_> = (0..current.xs.len()).collect();
        order.sort_by(|a, b| current.xs[*a].total_cmp(&current.xs[*b]).then(a.cmp(b)));
        let index = order.iter().position(|ch| *ch == at.ch).unwrap();
        let visual = match m {
            Motion::Left | Motion::WordLeft => index.checked_sub(1),
            Motion::Right | Motion::WordRight => (index + 1 < order.len()).then_some(index + 1),
            Motion::Home => Some(0),
            Motion::End => Some(order.len() - 1),
            Motion::Up | Motion::Down => {
                let line = if matches!(m, Motion::Up) { at.line.saturating_sub(1) } else { (at.line + 1).min(last) };
                return caret_at(sel, current.xs[at.ch], sel[line].y + sel[line].h * 0.5);
            }
            _ => None,
        };
        if let Some(mut i) = visual {
            if matches!(m, Motion::WordLeft | Motion::WordRight) {
                let right = matches!(m, Motion::WordRight);
                let word = word_at(sel, Caret { line: at.line, ch: order[i] });
                loop {
                    let next = if right { i.checked_add(1).filter(|j| *j < order.len()) } else { i.checked_sub(1) };
                    let Some(j) = next else { break };
                    if order[j] < word.from.ch || order[j] > word.to.ch { break; }
                    i = j;
                }
            }
            return Caret { line: at.line, ch: order[i] };
        }
        if matches!(m, Motion::Left | Motion::WordLeft | Motion::Right | Motion::WordRight) {
            let right = matches!(m, Motion::Right | Motion::WordRight);
            let line = if right { (at.line + 1).min(last) } else { at.line.saturating_sub(1) };
            if line == at.line { return at; }
            return caret_at(sel, if right { f32::MIN } else { f32::MAX }, sel[line].y + sel[line].h * 0.5);
        }
    }
    let moved = match m {
        // A line break is one position to cross, not two: coming down at the end of a
        // line lands the caret at the start of the next, the way a drag would.
        Motion::Left => match at.ch {
            0 => match at.line {
                0 => at,
                l => Caret { line: l - 1, ch: end_of(l - 1) },
            },
            n => Caret { line: at.line, ch: n - 1 },
        },
        Motion::Right => {
            if at.ch < end_of(at.line) {
                Caret { line: at.line, ch: at.ch + 1 }
            } else if at.line < last {
                Caret { line: at.line + 1, ch: 0 }
            } else {
                at
            }
        }
        // Keep the visual column even when the neighbouring line reads backwards.
        Motion::Up | Motion::Down => {
            let line = if matches!(m, Motion::Up) { at.line.saturating_sub(1) } else { (at.line + 1).min(last) };
            caret_at(sel, current.xs[at.ch], sel[line].y + sel[line].h * 0.5)
        }
        Motion::Home => Caret { line: at.line, ch: 0 },
        Motion::End => Caret { line: at.line, ch: end_of(at.line) },
        Motion::DocStart => Caret { line: 0, ch: 0 },
        Motion::DocEnd => Caret { line: last, ch: end_of(last) },
        Motion::WordRight => {
            let mut c = at;
            // A word motion does not stop to admire the space in front of the word.
            while kind_ahead(sel, c, true) == Some(WordKind::Space) {
                let n = clamp_caret(sel, Caret { line: c.line, ch: c.ch + 1 });
                if n == c {
                    break;
                }
                c = n;
            }
            let w = word_at(sel, c);
            if w.to.ch > c.ch {
                Caret { line: c.line, ch: w.to.ch }
            } else {
                // Past this line's last word: over the break, where the next motion
                // picks up the next line's first.
                next_caret(sel, c, Motion::Right)
            }
        }
        Motion::WordLeft => {
            let mut c = at;
            while kind_ahead(sel, c, false) == Some(WordKind::Space) {
                let p = clamp_caret(sel, Caret { line: c.line, ch: c.ch.saturating_sub(1) });
                if p == c {
                    break;
                }
                c = p;
            }
            // Onto the word's last character, so that the extent below reads the word
            // rather than whatever the caret was standing after.
            c = next_caret(sel, c, Motion::Left);
            let w = word_at(sel, c);
            if w.from.ch < c.ch {
                Caret { line: c.line, ch: w.from.ch }
            } else {
                c
            }
        }
    };
    clamp_caret(sel, moved)
}

/// Where the caret bar stands: the character edge it names, as tall as its line.
/// A caret past the end of a line that has since been rewound reads as the line's own
/// right edge rather than as no caret at all, because that is where the text it was
/// standing after now ends.
pub fn caret_rect(sel: &[SelLine], c: Caret) -> Option<(f32, f32, f32, f32)> {
    let l = sel.get(c.line)?;
    let x = *l.xs.get(c.ch.min(l.chars.len()))?;
    Some((x, l.y, 1.0, l.h))
}

#[derive(Clone)]
pub(crate) struct TableHeaderFragment {
    pub y: Pt,
    pub height: Pt,
    pub ops: Vec<Op>,
}

pub(crate) struct TableSpan {
    pub y: Pt,
    pub height: Pt,
    pub header: usize,
}

pub(crate) struct NoteSpan {
    pub number: String,
    pub y: Pt,
    pub height: Pt,
}

pub(crate) struct MathTextFragment {
    pub source: String,
    pub x: Pt,
    pub y: Pt,
    pub w: Pt,
    pub h: Pt,
    pub face_index: usize,
    pub em: Pt,
}

/// Everything one typesetting pass produced.
pub struct Page {
    pub ops: Vec<Op>,
    /// The document's height, in points.
    pub height: Pt,
    /// The measure chosen for the page and where it starts, which is what the headless
    /// report measures line edges against.
    pub column: Pt,
    pub left: Pt,
    pub hotspots: Vec<Hot>,
    /// Where each footnote's first line begins, indexed like [`Document::footnotes`].
    pub note_tops: Vec<Pt>,
    /// Where each heading's first line begins, indexed by the same order the source
    /// lists them in, so a fragment link can be resolved to one by [`slug`].
    pub anchor_tops: Vec<Pt>,
    /// Every line of text the page drew, in the order a drag crosses them, which is
    /// what makes the page selectable.
    pub sel: Vec<SelLine>,
    /// Hyphenated breaks taken against hyphen marks drawn, which is the one number that
    /// says whether a split word shows the split.
    pub hyphens: HyphenCount,
    /// Regions whose natural width exceeds the visible column, or whose object has a
    /// click action even when it fits.
    pub wide_regions: Vec<WideRegion>,
    pub(crate) table_headers: Vec<TableHeaderFragment>,
    pub(crate) table_spans: Vec<TableSpan>,
    pub(crate) note_spans: Vec<NoteSpan>,
    pub(crate) math_texts: Vec<MathTextFragment>,
}

pub struct View {
    d2d: ID2D1Factory,
    target: Option<ID2D1RenderTarget>,
    hwnd_target: Option<ID2D1HwndRenderTarget>,
    font: FontEngine,
    theme: Theme,
    profile: String,
    doc: Document,
    source: String,
    source_view: bool,
    text_options: TextOptions,
    /// Chapter ranges for a plain-text book. `None` keeps the whole document in one
    /// layout, which is still the right answer for a short story or a Markdown file.
    chapter_index: Option<ChapterIndex>,
    chapter: usize,
    plain_override: Option<bool>,
    encoding: Encoding,
    decoded_encoding: Encoding,
    encoding_guessed: bool,
    keep_line_breaks: bool,
    line_break_override: Option<bool>,
    ops: Vec<Op>,
    palette: Palette,
    /// The palette the reader asked for with `Ctrl`+`D`. `None` follows the system,
    /// which is what the appearance poll reports; a manual choice has to outrank that
    /// answer or the next timer tick would undo it.
    dark_override: Option<bool>,
    brushes: HashMap<ColorRole, ID2D1SolidColorBrush>,
    /// The targets of the current layout, and where each note begins.
    hotspots: Vec<Hot>,
    note_tops: Vec<Pt>,
    /// Where each heading begins, for the fragment links in a table of contents.
    anchor_tops: Vec<Pt>,
    /// Wide regions in the current page, matching `HotKind::Wide` indices.
    wide_regions: Vec<WideRegion>,
    /// A full-size object preview opened from a wide hotspot.
    preview: Option<Preview>,
    /// The page's text, character by character, as the current layout drew it.
    sel_index: Vec<SelLine>,
    /// What the reader has dragged out, if anything. Cleared by a relayout, whose
    /// rewrapped lines would otherwise leave a selection pointing at other words.
    selection: Option<Selection>,
    /// Where the button went down in the text, which is the end of a selection that
    /// has not been dragged yet and the point a double click expands from.
    press_caret: Option<Caret>,
    /// Where the reader has put the caret, by a click or an arrow. While it is set the
    /// arrow keys belong to it rather than to the scroll, which is what every text
    /// surface on this system does and the reason `Escape` has to give it back.
    caret: Option<Caret>,
    /// The ink laid over selected text. A highlight drawn *under* the ink cannot
    /// work: a code block paints its own opaque panel after it.
    sel_brush: Option<ID2D1SolidColorBrush>,
    /// The pointer over a clickable target. Both cursors are loaded once:
    /// `WM_SETCURSOR` is asked on every move, and a shared system cursor is not
    /// something to fetch afresh each time it answers.
    arrow: HCURSOR,
    hand: HCURSOR,
    /// The hotspot a button is held down on. Releasing somewhere else is a reader
    /// changing their mind, not a click.
    pressed: Option<usize>,
    scroll: Pt,
    content_h: Pt,
    client_w: f32,
    client_h: f32,
    dpi: f32,
    path: Option<PathBuf>,
    /// Open documents and their stable tab identities. The window keeps one materialized
    /// document at a time; this is the authority for what can be switched to next.
    workspace: Workspace,
    tree: Vec<TreeEntry>,
    /// What the file behind the page looked like when this text was read out of it: how
    /// long it was, and when it was last written. See [`Stamp`] and `WM_TIMER`.
    stamp: Option<Stamp>,
    /// The pages the reader has left behind, and the ones they have stepped away from.
    /// Filled by any change of page -- a link, a dropped file, a second document out of
    /// the dialog -- so that the last thing they were reading is never more than one key
    /// away, and empty until there is a second page to have come from.
    history: History,
    /// Set while the pointer is dragging the scroll thumb.
    dragging: bool,
    /// Horizontal offset currently applied to the wide region under the pointer.
    wide_offset: f32,
    /// The wide region currently being panned or previewed.
    wide_active: Option<usize>,
    /// Where the left button went down, in client pixels. A press that has moved past
    /// a few of them is a reader dragging out a selection rather than one aiming at a
    /// link, and the difference has to be settled before the release.
    press_at: Option<(f32, f32)>,
    /// Built once a render target exists, since bitmaps need one.
    images: Option<ImageStore>,
    /// Typeset formulas, kept across relayouts so a document's math is set once.
    math: MathStore,
    /// English word breaks, when the embedded dictionary loaded.
    hyphenator: Option<Hyphenator>,
    /// What the reader is looking for, and everywhere the page has it. `Some` while the
    /// bar is on the window, whether or not anything has been typed into it yet.
    find: Option<Find>,
    /// The box the query is typed into: a window control of its own rather than ink this
    /// view draws, because a reader searching for Chinese searches with an input method,
    /// and an input method attaches to a real edit control and to nothing else.
    edit: Option<HWND>,
    edit_font: Option<HFONT>,
    /// The box's background, which the painter has no say in: USER32 paints a child
    /// window, and asks this window what colours to use.
    edit_brush: Option<HBRUSH>,
    /// How many there are, next to the box.
    find_label: Vec<PaintRun>,
    hit_brush: Option<ID2D1SolidColorBrush>,
    focus_brush: Option<ID2D1SolidColorBrush>,
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

/// How far the document's top sits above the window's, in device independent pixels,
/// when the reader has scrolled down by `scroll` points.
///
/// One function for one number, because it is used from both ends of the same
/// arithmetic: paint lifts the display list by it, and a pointer's y is pushed down by it
/// to be asked of that list. Agree on nothing else and the page paints one paragraph while
/// a click marks the next.
#[inline]
fn scroll_dip(scroll: Pt, dpi: f32) -> Pt {
    scroll * scale_of(dpi)
}

/// One rung of the measure ladder either way, stopping at both ends rather than
/// wrapping: a key pressed past the widest column the page can be set to should do
/// nothing, not silently teleport the reader to the narrow one.
fn step_measure(from: usize, delta: i32) -> usize {
    (from as i32 + delta).clamp(0, Measure::ALL.len() as i32 - 1) as usize
}

fn pan_offset(content_w: Pt, visible_w: Pt, requested: Pt) -> Pt {
    (requested).clamp(0.0, (content_w - visible_w).max(0.0))
}

fn region_shift(region: &WideRegion, offset: Pt, x: f32, y: f32) -> Pt {
    if y < region.y || y > region.y + region.h {
        return 0.0;
    }
    if x >= region.x && x <= region.x + region.content_w { -offset } else { 0.0 }
}

/// A page the reader was on, and how far down it they had got.
///
/// `path` is `None` for the built-in sample, which is a page like any other: the reader
/// can leave it, and be asked back to.
#[derive(Clone, Debug, PartialEq)]
struct Visit {
    path: Option<PathBuf>,
    scroll: Pt,
}

impl Visit {
    /// Whether this place could still be stood on, which for a page that has come off a
    /// removed drive since it was left it cannot.
    fn readable(&self) -> bool {
        self.path.as_ref().is_none_or(|p| p.is_file())
    }
}

/// The pages the reader has left behind, and the ones a step forward would take them to.
///
/// A step is a change of page or a jump within one, because the two are the same
/// experience to the reader: they looked at something, followed a link away from it, and
/// want what they were looking at back.
#[derive(Default)]
struct History {
    past: Vec<Visit>,
    future: Vec<Visit>,
}

impl History {
    /// Leave a place behind on the way to a new one.
    ///
    /// What was ahead is thrown away, as every browser does and for the same reason: the
    /// reader has taken a different road here, so the places further along the old one
    /// are not where they are going next.
    fn leave(&mut self, from: Visit) {
        self.past.push(from);
        self.future.clear();
    }

    /// Step back, leaving the place being stood on where it can be come forward to.
    fn back(&mut self, from: Visit) -> Option<Visit> {
        let there = self.past.pop()?;
        self.future.push(from);
        Some(there)
    }

    fn forward(&mut self, from: Visit) -> Option<Visit> {
        let there = self.future.pop()?;
        self.past.push(from);
        Some(there)
    }

    /// One end of the road, nearest the reader first.
    fn end(&self, back: bool) -> &[Visit] {
        if back {
            &self.past
        } else {
            &self.future
        }
    }

    /// The nearest place at one end that could still be stood on: a step lands there
    /// rather than nowhere, even though a nearer page has come off a removed drive.
    fn walkable(&self, back: bool) -> Option<&Visit> {
        self.end(back).iter().rev().find(|v| v.readable())
    }

    /// Whether a step either way has somewhere to land, which is what dims the two rows
    /// at the top of the menu. It asks of the whole end of the road rather than of its
    /// last entry alone: a reader who deleted a page they had visited should still find
    /// `Back` lit, because the page behind that one is still there.
    fn leads(&self, back: bool) -> bool {
        self.walkable(back).is_some()
    }

    /// Drop the places at one end that nothing can stand on again, so that a step which
    /// has to pass through a deleted page passes through it once rather than on every
    /// press -- and so that the next place to land is the nearest one at that end.
    fn prune(&mut self, back: bool) {
        let stack = if back { &mut self.past } else { &mut self.future };
        while stack.last().is_some_and(|v| !v.readable()) {
            stack.pop();
        }
    }
}

/// What the reader is looking for, and everywhere the page has it.
///
/// The places are kept rather than recomputed per frame: a hit is a pair of caret
/// positions in the current wrapping, so a wheel tick can repaint it for nothing, and
/// only a relayout -- the one event that moves the words -- has to ask again.
#[derive(Default)]
struct Find {
    /// What the box holds, mirrored out of it on every change. The box is a window that
    /// may not exist yet, or any more, and the page has to be able to answer the same
    /// question without one.
    query: String,
    marks: Vec<Selection>,
    /// Which of them `Enter` last landed on, and the one drawn darker than the rest.
    focus: usize,
}

/// The bar's size and station, in device independent pixels.
///
/// It hangs at the top right, over the page, rather than across the bottom of the window
/// where nothing overlaps it: a bar the prose has to make room for is a page that reflows
/// when a key is pressed, and a reader searching has already lost their place to look for.
const FIND_W: f32 = 340.0;
const FIND_H: f32 = 32.0;
const FIND_TOP: f32 = 8.0;
/// Kept from the window's right edge -- more than the thumb's track needs, so that the
/// one thing the reader has to grip is never under the thing that appeared by accident.
const FIND_EDGE: f32 = 16.0;
/// Space at the bar's right for the count of what was found, which the box cannot have:
/// a query is read from its start, and the number belongs out of the way of that.
const FIND_COUNT: f32 = 96.0;
const FIND_PAD: f32 = 6.0;
/// The id the box's notifications come back with, since a child window has no other way
/// to say which of its parent's children is speaking.
const FIND_EDIT_ID: usize = 0x5141;

/// The panel's rect, measured from the top of the *window* -- the bar is pinned to the
/// glass, not to the page.
fn find_panel(client_w: f32) -> (f32, f32, f32, f32) {
    // A window narrower than the bar gives up width rather than hanging off the left edge,
    // where a box with no words beside it is all that would be left of it.
    let w = FIND_W.min((client_w - 2.0 * FIND_EDGE).max(0.0));
    (client_w - FIND_EDGE - w, FIND_TOP, w, FIND_H)
}

/// The text box inside the panel, in the physical pixels `MoveWindow` wants: a child
/// window is placed by the window manager, which knows nothing of this view's scale.
fn find_edit(client_w: f32, dpi: f32) -> (i32, i32, i32, i32) {
    let k = scale_of(dpi);
    let (x, y, w, h) = find_panel(client_w);
    let box_w = (w - 2.0 * FIND_PAD - FIND_COUNT).max(48.0);
    let box_h = (h - 2.0 * FIND_PAD).max(12.0);
    (
        ((x + FIND_PAD) * k) as i32,
        ((y + FIND_PAD) * k) as i32,
        (box_w * k) as i32,
        (box_h * k) as i32,
    )
}

/// Whether a window point is on the bar's panel, which is this view's own paint and so
/// hides whatever page is under it.
fn in_find_panel(x: f32, y: f32, client_w: f32) -> bool {
    let (px, py, pw, ph) = find_panel(client_w);
    x >= px && x <= px + pw && y >= py && y <= py + ph
}

/// What the bar says about a search: how many hits there are, and which one the reader is
/// on. One place rather than a format string in the painter, because the sentence is the
/// only answer a reader gets to "is the word I meant even here", and it has to read as well
/// at one hit as at nine hundred.
fn find_count(focus: usize, hits: usize) -> String {
    match hits {
        0 => "no match".to_string(),
        1 => "1 match".to_string(),
        n => format!("{} of {}", focus + 1, n),
    }
}

/// An 8-bit colour, the shape GDI wants for the one window this painter cannot reach.
fn cref(c: Rgb) -> COLORREF {
    let v = |x: f32| (x.clamp(0.0, 1.0) * 255.0).round() as u32;
    COLORREF(v(c.r) | v(c.g) << 8 | v(c.b) << 16)
}

/// What the right-click menu can be asked to do.
///
/// Every one of these is also reachable some other way -- a key, a click, the wheel --
/// because a menu item that exists only on the menu is a code path nothing but a pointer
/// can test, and the window is not open in a test.
#[derive(Clone, Debug, PartialEq)]
enum Command {
    Typography,
    Profile(String),
    /// Step to the page the reader came from, and to the one they came from it to.
    GoBack,
    GoForward,
    /// Jump to the nth heading, index into the same list the outline is built from.
    Heading(usize),
    Copy,
    SelectAll,
    /// Put the bar on the window that a phrase is typed into.
    Find,
    OpenUrl(String),
    CopyUrl(String),
    ZoomIn,
    ZoomOut,
    ZoomReset,
    /// Set the page in one of the faces [`TextFace::ALL`] offers, by index.
    Face(usize),
    /// Widen or narrow the column, by index into [`Measure::ALL`].
    Measure(usize),
    /// Set the palette and keep it set, against whatever the system says next.
    Palette(bool),
    /// Let the system's own setting decide again, which is the only way back out of a
    /// manual choice once the room's light has changed.
    FollowSystem,
    OpenFile,
    OpenRecent(usize),
    OpenTree(usize),
    TreeDirectory(usize),
    ActivateTab(TabId),
    CloseTab(TabId),
    PinTab(TabId),
    NextTab,
    PreviousTab,
    /// Read the file this page came from again, from disk.
    Reload,
    DefaultLineBreaks(bool),
    DocumentLineBreaks(Option<bool>),
    SourceView,
    PlainText(Option<bool>),
    TextParagraphs(ParagraphRule),
    DetectChapters(bool),
    TextEncoding(Encoding),
    Neighbor(bool),
    WideTableNarrow,
    WideTableWiden,
    PreviousChapter,
    NextChapter,
    OpenEditor,
    ChooseEditor,
}

/// One row of that menu.
#[derive(Clone, Debug, PartialEq)]
enum MenuRow {
    Gap,
    Row { cmd: Command, label: String, enabled: bool, checked: bool },
    /// A list of rows one level down. Only ever built when it has something in it: a
    /// group that opens onto an empty rectangle is a menu teaching the reader that
    /// nothing here is worth their click, which is worse than not offering the group.
    Sub { label: &'static str, items: Vec<MenuRow> },
}

/// A row the reader may or may not be able to ask for.
fn row(cmd: Command, label: impl Into<String>, enabled: bool) -> MenuRow {
    MenuRow::Row { cmd, label: label.into(), enabled, checked: false }
}

/// A row that names which of a group is the one in use.
fn check(cmd: Command, label: impl Into<String>, on: bool) -> MenuRow {
    MenuRow::Row { cmd, label: label.into(), enabled: true, checked: on }
}

/// What the page looks like from the place the menu was called, gathered because the
/// menu has to say true things about the moment it is opened in: an item that offers to
/// copy nothing, or checks a palette that is not on the screen, teaches the reader to
/// distrust the whole list.
#[derive(Default)]
struct MenuState {
    profiles: Vec<String>,
    profile: String,
    source_view: bool,
    plain_override: Option<bool>,
    text_options: TextOptions,
    chapter_titles: Vec<String>,
    chapter: usize,
    tabs: Vec<(TabId, String, bool)>,
    recent: Vec<(usize, String)>,
    tree: Vec<(usize, String, bool)>,
    encoding: Encoding,
    encoding_notice: Option<String>,
    previous: bool,
    next: bool,
    keep_line_breaks: bool,
    line_break_override: Option<bool>,
    /// Whether a step back or forward has a page to land on. A reader who has opened one
    /// document and never followed a link out of it has no road behind them, and a `Back`
    /// that does nothing when pressed teaches them the menu is not to be believed.
    can_back: bool,
    can_forward: bool,
    /// The address under the pointer, when a link that leads out is what is under it.
    link: Option<String>,
    selected: bool,
    /// `None` while the system's setting is the one deciding.
    dark: Option<bool>,
    /// Whether this page came from a file, which is what makes reading it again a
    /// possible thing to ask for.
    from_file: bool,
    /// Whether the page has any text at all, which is the difference between an empty
    /// document and a document nothing has been marked in.
    text: bool,
    /// Which entry of [`TextFace::ALL`] the page is set in right now.
    face: usize,
    /// Which entry of [`Measure::ALL`] the column is capped at right now.
    measure: usize,
    /// The page's own outline, which is a fact about the document rather than about the
    /// reader's choices, and is empty on a page with no headings in it.
    headings: Vec<Outline>,
    /// Which of those faces this machine can actually draw, index for index with
    /// [`TextFace::ALL`]. A face that is not installed is shown and left dim: it is the
    /// reader's own machine, and hiding the choice they cannot have says less about it
    /// than naming it and refusing to do anything when asked.
    offered: Vec<bool>,
}

/// The menu, in the order it appears and with the groups it is divided into.
fn menu_items(s: &MenuState) -> Vec<MenuRow> {
    // A tab in a menu string starts the accelerator column, which Windows sets
    // right-aligned on its own. Only the rows where the key does exactly what the row
    // says print one: `Ctrl`+`D` picks whichever palette is not on rather than the one it
    // is standing next to, and the bracket keys step the measure rather than settling on
    // a rung, so a hint on either would promise a different thing from the one it keeps.
    let mut v = vec![
        row(Command::GoBack, "Back\tAlt+\u{2190}", s.can_back),
        row(Command::GoForward, "Forward\tAlt+\u{2192}", s.can_forward),
        MenuRow::Gap,
        row(Command::Copy, "Copy\tCtrl+C", s.selected),
        row(Command::SelectAll, "Select All\tCtrl+A", s.text),
        // The one item on this menu a reader needs on a page too long to read at once,
        // and lit by the same question as `Select All`: is there any text here at all.
        row(Command::Find, "Find in Document\tCtrl+F", s.text),
    ];
    if let Some(url) = &s.link {
        v.push(MenuRow::Gap);
        v.push(row(Command::OpenUrl(url.clone()), "Open Link", true));
        v.push(row(Command::CopyUrl(url.clone()), "Copy Link Address", true));
    }
    // The document's own shape, one level down, where a long page needs it and a short one
    // gets nothing -- an outline of three lines is quicker to scroll past than to open.
    let contents = outline_rows(&s.headings);
    if !contents.is_empty() {
        v.push(MenuRow::Gap);
        v.push(MenuRow::Sub { label: "Contents", items: contents });
    }
    v.push(MenuRow::Gap);
    v.extend([
        row(Command::ZoomIn, "Increase Text\tCtrl++", true),
        row(Command::ZoomOut, "Decrease Text\tCtrl+-", true),
        row(Command::ZoomReset, "Actual Size\tCtrl+0", true),
    ]);
    let mut typography = vec![row(Command::Typography, "Edit / Save Preset...", true), MenuRow::Gap];
    typography.extend(s.profiles.iter().map(|name| check(Command::Profile(name.clone()), name, *name == s.profile)));
    v.push(MenuRow::Sub { label: "Typography", items: typography });
    v.push(MenuRow::Gap);
    for (i, f) in TextFace::ALL.iter().enumerate().filter(|_| s.profile.is_empty() || s.profile == "Default") {
        let on = i == s.face;
        let ok = s.offered.get(i).copied().unwrap_or(false);
        // The face in use is always clickable: a machine that has lost a family since it
        // was chosen still has to be able to choose away from it.
        v.push(if on { check(Command::Face(i), f.label, true) } else { row(Command::Face(i), f.label, ok) });
    }
    v.push(MenuRow::Gap);
    for (i, m) in Measure::ALL.iter().enumerate().filter(|_| s.profile.is_empty() || s.profile == "Default") {
        v.push(check(Command::Measure(i), m.label, i == s.measure));
    }
    v.push(MenuRow::Gap);
    v.extend([
        check(Command::Palette(false), "Light", s.dark == Some(false)),
        check(Command::Palette(true), "Dark", s.dark == Some(true)),
        check(Command::FollowSystem, "Follow System", s.dark.is_none()),
    ]);
    v.push(MenuRow::Gap);
    v.extend([
        row(Command::OpenFile, "Open\u{2026}\tCtrl+O", true),
        row(Command::Reload, "Reload\tCtrl+R", s.from_file),
    ]);
    if !s.tabs.is_empty() {
        let mut tab_items = Vec::new();
        for (id, title, active) in &s.tabs {
            tab_items.push(row(Command::ActivateTab(*id), format!("{} {}", if *active { "●" } else { "○" }, title), true));
            tab_items.push(row(Command::PinTab(*id), "Pin", !*active));
            tab_items.push(row(Command::CloseTab(*id), "Close", !*active));
        }
        v.push(MenuRow::Gap);
        v.push(MenuRow::Sub { label: "Open tabs", items: tab_items });
    }
    if !s.recent.is_empty() {
        let recent_items = s.recent.iter()
            .map(|(i, path)| row(Command::OpenRecent(*i), path.clone(), true))
            .collect();
        v.push(MenuRow::Sub { label: "Recent documents", items: recent_items });
    }
    if !s.tree.is_empty() {
        let tree_items = s.tree.iter().map(|(i, name, is_dir)| {
            let command = if *is_dir { Command::TreeDirectory(*i) } else { Command::OpenTree(*i) };
            row(command, name.clone(), true)
        }).collect();
        v.push(MenuRow::Sub { label: "Workspace tree", items: tree_items });
    }
    v.insert(v.len() - 3, MenuRow::Sub { label: "Single newlines", items: vec![
        check(Command::DefaultLineBreaks(false), "Default: Merge into paragraph", !s.keep_line_breaks),
        check(Command::DefaultLineBreaks(true), "Default: Keep line breaks", s.keep_line_breaks),
        MenuRow::Gap,
        check(Command::DocumentLineBreaks(None), "This document: Follow default", s.line_break_override.is_none()),
        check(Command::DocumentLineBreaks(Some(false)), "This document: Merge", s.line_break_override == Some(false)),
        check(Command::DocumentLineBreaks(Some(true)), "This document: Keep", s.line_break_override == Some(true)),
    ] });
    v.insert(v.len() - 3, check(Command::SourceView, "Read Source\tCtrl+3", s.source_view));
    v.insert(v.len() - 3, MenuRow::Sub { label: "External editor", items: vec![
        row(Command::OpenEditor, "Open in Editor\tCtrl+Shift+O", s.from_file),
        row(Command::ChooseEditor, "Choose Editor\u{2026}", true),
    ] });
    v.insert(v.len() - 3, MenuRow::Sub { label: "Text reading", items: vec![
        check(Command::PlainText(None), "Format: From file extension", s.plain_override.is_none()),
        check(Command::PlainText(Some(true)), "Format: Plain text", s.plain_override == Some(true)),
        check(Command::PlainText(Some(false)), "Format: Markdown", s.plain_override == Some(false)),
        MenuRow::Gap,
        check(Command::TextParagraphs(ParagraphRule::Auto), "Paragraphs: Automatic", s.text_options.paragraphs == ParagraphRule::Auto),
        check(Command::TextParagraphs(ParagraphRule::Lines), "Paragraphs: Each line", s.text_options.paragraphs == ParagraphRule::Lines),
        check(Command::TextParagraphs(ParagraphRule::BlankLines), "Paragraphs: Blank lines", s.text_options.paragraphs == ParagraphRule::BlankLines),
        check(Command::DetectChapters(!s.text_options.chapters), "Detect chapter headings", s.text_options.chapters),
        MenuRow::Gap,
        row(Command::PreviousChapter, "Previous chapter\tCtrl+Alt+Up", s.chapter > 0),
        row(Command::NextChapter, "Next chapter\tCtrl+Alt+Down", s.chapter + 1 < s.chapter_titles.len()),
        row(Command::WideTableNarrow, "Wide table: Borrow less margin", true),
        row(Command::WideTableWiden, "Wide table: Borrow more margin", true),
        row(Command::Neighbor(false), "Previous file\tCtrl+Alt+Left", s.previous),
        row(Command::Neighbor(true), "Next file\tCtrl+Alt+Right", s.next),
    ] });
    let mut encodings: Vec<_> = Encoding::ALL.into_iter().map(|e|
        check(Command::TextEncoding(e), e.label(), e == s.encoding)).collect();
    if let Some(notice) = &s.encoding_notice {
        encodings.insert(0, row(Command::TextEncoding(s.encoding), notice, false));
    }
    v.insert(v.len() - 3, MenuRow::Sub { label: "Text encoding", items: encodings });
    v
}

/// Append these rows to a menu, collecting the command each appended id answers to.
///
/// A submenu is created, filled and handed to its parent, which then owns it: destroying
/// the menu the reader was shown takes the whole tree with it, so nothing here has to be
/// cleaned up by hand. A group that cannot be created is skipped rather than opened empty,
/// and one that cannot be attached is destroyed at once, since it has no parent to do that
/// for it.
fn append_rows(
    menu: &HMENU,
    rows: &[MenuRow],
    next: &mut usize,
    cmds: &mut HashMap<usize, Command>,
) {
    for item in rows {
        match item {
            MenuRow::Gap => {
                let _ = unsafe { AppendMenuW(*menu, MF_SEPARATOR, 0, PCWSTR::null()) };
            }
            MenuRow::Row { cmd, label, enabled, checked } => {
                let text = utf16(label);
                let mut flags = MF_STRING;
                if !enabled {
                    flags |= MF_GRAYED;
                }
                if *checked {
                    flags |= MF_CHECKED;
                }
                let id = *next;
                *next += 1;
                if unsafe { AppendMenuW(*menu, flags, id, PCWSTR(text.as_ptr())) }.is_ok() {
                    cmds.insert(id, cmd.clone());
                }
            }
            MenuRow::Sub { label, items } => {
                let Ok(sub) = (unsafe { CreatePopupMenu() }) else { continue };
                append_rows(&sub, items, next, cmds);
                let text = utf16(label);
                // The item's number is the submenu's own handle for a `MF_POPUP` row,
                // which is how Windows finds it again -- and why no id is recorded here.
                if unsafe { AppendMenuW(*menu, MF_POPUP, sub.0 as usize, PCWSTR(text.as_ptr())) }
                    .is_err()
                {
                    let _ = unsafe { DestroyMenu(sub) };
                }
            }
        }
    }
}

/// Whether a reader may be asked to open a link. Only the schemes that mean "read
/// this" qualify: `file:`, a bare path and a UNC share are requests to reach the local
/// disk or to run something, and a document that is being looked at has no business
/// launching them. A control character is refused for the same reason -- it cannot be
/// part of an address, and the shell parses more of these strings than a reader would
/// like to think about.
pub(crate) fn editor_arguments(template: &str, path: &Path, line: usize, column: usize) -> Vec<String> {
    if template.trim().is_empty() {
        return vec![path.to_string_lossy().into_owned()];
    }
    let mut args = Vec::new();
    let mut token = String::new();
    let mut quoted = false;
    let mut escaped = false;
    for ch in template.chars() {
        if escaped {
            token.push(ch);
            escaped = false;
        } else if ch == '\\' && quoted {
            escaped = true;
        } else if ch == '"' {
            quoted = !quoted;
        } else if ch.is_whitespace() && !quoted {
            if !token.is_empty() {
                args.push(std::mem::take(&mut token));
            }
        } else {
            token.push(ch);
        }
    }
    if escaped { token.push('\\'); }
    if !token.is_empty() { args.push(token); }
    args.into_iter().map(|arg| {
        arg.replace("{file}", &path.to_string_lossy())
            .replace("{line}", &line.to_string())
            .replace("{column}", &column.to_string())
    }).filter(|arg| !arg.is_empty()).collect()
}

pub(crate) fn openable(url: &str) -> bool {
    let Some((scheme, rest)) = url.split_once(':') else {
        // A relative link or a bare fragment points inside this file, which is not a
        // thing to hand to another program.
        return false;
    };
    if url.chars().any(|c| (c as u32) < 0x20) {
        return false;
    }
    match scheme.to_ascii_lowercase().as_str() {
        "http" | "https" => rest.starts_with("//"),
        "mailto" => !rest.is_empty(),
        _ => false,
    }
}

/// The address a heading answers to, derived from its own text: lowered, a run of
/// whitespace turned into one hyphen, and nothing kept but letters, digits, hyphens and
/// underscores -- so the punctuation a heading's prose carries, and the markup around an
/// inline code span or a formula, all drop out.
///
/// Written down as a rule rather than left to taste, because the only way to point at a
/// heading is for an author to spell this out by hand in a table of contents: if the rule
/// is not the one they can guess, the link is dead and nothing on the page says why.
/// A script that writes words without spaces keeps its characters whole, which is what
/// makes a heading in one language as clickable as a heading in another.
pub(crate) fn slug(text: &str) -> String {
    let mut out = String::new();
    let mut pending = false;
    for c in text.chars() {
        if c.is_whitespace() {
            // Only after something has been kept: a heading that opens with markup or an
            // indent has no reason to start its address with a hyphen.
            pending = !out.is_empty();
            continue;
        }
        if !(c.is_alphanumeric() || c == '-' || c == '_') {
            continue;
        }
        if pending {
            out.push('-');
            pending = false;
        }
        out.extend(c.to_lowercase());
    }
    out
}

/// One line of the document's outline: the heading's own words, and how deep it sits.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Outline {
    pub level: u8,
    pub label: String,
}

/// The document's headings, in the order they were parsed -- which is the order
/// [`View::anchor_tops`] is filled, so the nth line of the outline jumps to the nth
/// anchor. `anchors` is how many of them actually landed on the page, which is what keeps
/// the two lists the same length if a heading ever fails to lay out. A note can carry a
/// heading of its own, and those are not part of the document's outline: they are not in
/// `doc.blocks` to begin with.
fn outline(doc: &Document, anchors: usize) -> Vec<Outline> {
    doc.blocks
        .iter()
        .filter_map(|b| match b.kind {
            BlockKind::Heading(level) => Some(Outline { level, label: b.text.clone() }),
            _ => None,
        })
        .take(anchors)
        .collect()
}

/// The outline as rows of a submenu, indented by level.
///
/// An em space per step, because a menu has no other way to say "this belongs to the line
/// above it": a reader who wrote a third-level heading should see it sit under its own
/// section rather than beside the chapter it is in. A tab is swapped for a space first,
/// because in a menu string a tab is not blank -- it opens the accelerator column, and a
/// heading that happened to carry one would print half of itself on the right.
fn outline_rows(headings: &[Outline]) -> Vec<MenuRow> {
    headings
        .iter()
        .enumerate()
        .map(|(i, h)| {
            let indent = "\u{2003}".repeat(h.level.saturating_sub(1).min(4) as usize);
            row(Command::Heading(i), indent + &h.label.replace('\t', " "), true)
        })
        .collect()
}

/// Hand an accepted address to the shell, which is what decides the browser or the mail
/// client that answers.
fn open_url(url: &str) {
    if !openable(url) {
        return;
    }
    open_path(url);
}

fn open_path(path: &str) {
    let verb = utf16("open");
    let target = utf16(path);
    let r = unsafe {
        ShellExecuteW(
            None,
            PCWSTR(verb.as_ptr()),
            PCWSTR(target.as_ptr()),
            PCWSTR::null(),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        )
    };
    // Failures come back as small integers in the handle rather than as a null one.
    if (r.0 as usize) <= 32 {
        eprintln!("could not open {path}");
    }
}

fn show_error(hwnd: HWND, message: &str) {
    use windows::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONERROR, MB_OK};
    let message = utf16(message);
    let title = utf16("Rubrica");
    unsafe { MessageBoxW(Some(hwnd), PCWSTR(message.as_ptr()), PCWSTR(title.as_ptr()), MB_OK | MB_ICONERROR); }
}

/// Whether a modifier is down. Read from the keyboard state rather than from the
/// message: `WM_KEYDOWN` carries no modifier flags of its own, and a key that only
/// means something with `Ctrl` has to ask.
fn held(vk: VIRTUAL_KEY) -> bool {
    unsafe { (GetKeyState(vk.0 as i32) as u16 & 0x8000) != 0 }
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

    let arrow = unsafe { LoadCursorW(None, IDC_ARROW)? };
    // A pointer that turns into a hand says "this is a target" before the click, which
    // the colour of the ink by itself does not. Falls back to the arrow on a system
    // without the hand cursor rather than refusing to start.
    let hand = unsafe { LoadCursorW(None, IDC_HAND).unwrap_or(arrow) };

    // The reader's own appearance, from the last window, applied before the first layout
    // so a start-up never shows one page and then changes its face.
    //
    // A remembered family that has since left the machine is dropped rather than set:
    // DirectWrite would answer it with a substitute, and a preference that quietly becomes
    // a different font is worse than no preference at all. The size, the width and the
    // palette are kept regardless, since none of them depends on anything installed.
    let saved = crate::settings::load();
    let mut theme = Theme::default();
    if let Some(zoom) = saved.zoom {
        theme.set_zoom(zoom);
    }
    let remembered = saved.face.filter(|i| {
        TextFace::ALL.get(*i).is_some_and(|f| face_drawable(&font, f))
    });
    if let Some(face) = remembered {
        theme.set_face(face);
    }
    // A remembered width the window cannot fit is still the reader's own choice, so it
    // comes back: the layout caps a column at what is available, so a wide measure in a
    // narrow window costs nothing and shows itself the moment the window grows.
    if let Some(measure) = saved.measure {
        theme.set_measure(measure);
    }
    let dark = saved.dark.unwrap_or_else(system_prefers_dark);

    // Where the reader stood the last time this page was open, if this window is the
    // continuation of that one rather than a page chosen afresh. The path decides rather
    // than the route here: a document named on the command line comes back to its place
    // too, which is what double-clicking it in Explorer is. A sample page has no path, and
    // so no place.
    let restore = path.as_deref().and_then(crate::settings::document_anchor).or_else(|| crate::settings::reading()
        .filter(|(left, _)| Some(left.as_path()) == path.as_deref())
        .map(|(_, anchor)| anchor));

    // The file the text above came out of, as it stands at this moment. `main` has already
    // read it, so this is the stamp of the page on the screen rather than of some text that
    // arrived afterwards -- and if a save did land in between, the first poll finds it a
    // fraction of a second later.
    let stamp = path.as_deref().and_then(stamp_of);
    let keep_line_breaks = crate::settings::keep_line_breaks();
    let preferences = path.as_deref().map(crate::settings::document).unwrap_or_default();
    let profile = crate::profiles::selected(preferences.plain.unwrap_or_else(|| reading::is_plain(path.as_deref())));
    if profile != "Default" { crate::profiles::load(&profile).apply(&mut theme); }
    let decoded = path.as_deref().and_then(|p| reading::read(p, preferences.encoding).ok());
    let plain = preferences.plain.unwrap_or_else(|| reading::is_plain(path.as_deref()));
    let chapter_index = plain.then(|| ChapterIndex::new(&source, preferences.text.chapters));
    let doc = if preferences.source { Document::source(&source) }
        else if let Some(index) = chapter_index.as_ref() {
            index.window(&source, 0, preferences.text)
        } else { Document::parse_with(&source, rubrica_doc::ParseOptions {
            keep_line_breaks: preferences.line_breaks.unwrap_or(keep_line_breaks),
        }) };

    let mut workspace = crate::settings::workspace().map(Workspace::restore).unwrap_or_default();
    if let Some(path) = path.as_ref() {
        workspace.open_file(View::workspace_file(path), TabKind::Pinned);
    }
    let tree = path.as_deref().and_then(Path::parent)
        .map(|root| tree::scan(root, 2))
        .unwrap_or_default();

    let mut view = Box::new(View {
        d2d,
        target: None,
        hwnd_target: None,
        font,
        theme,
        profile,
        doc,
        source,
        source_view: preferences.source,
        text_options: preferences.text,
        chapter_index,
        chapter: 0,
        plain_override: preferences.plain,
        encoding: preferences.encoding,
        decoded_encoding: decoded.as_ref().map_or(Encoding::Utf8, |d| d.encoding),
        encoding_guessed: decoded.is_some_and(|d| d.guessed),
        keep_line_breaks,
        line_break_override: preferences.line_breaks,
        ops: Vec::new(),
        palette: Palette::of(dark),
        dark_override: saved.dark,
        brushes: HashMap::new(),
        hotspots: Vec::new(),
        wide_regions: Vec::new(),
        preview: None,
        note_tops: Vec::new(),
        anchor_tops: Vec::new(),
        sel_index: Vec::new(),
        selection: None,
        press_caret: None,
        caret: None,
        sel_brush: None,
        arrow,
        hand,
        pressed: None,
        scroll: 0.0,
        content_h: 0.0,
        client_w: 1.0,
        client_h: 1.0,
        dpi: 96.0,
        path,
        workspace,
        tree,
        stamp,
        history: History::default(),
        dragging: false,
        wide_offset: 0.0,
        wide_active: None,
        press_at: None,
        images: None,
        math: MathStore::new(),
        hyphenator: Hyphenator::english(),
        find: None,
        edit: None,
        edit_font: None,
        edit_brush: None,
        find_label: Vec::new(),
        hit_brush: None,
        focus_brush: None,
    });

    const CLASS: &str = "Rubrica.Main";
    unsafe {
        let hinst = GetModuleHandleW(None)?;
        let wide = utf16(CLASS);
        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            style: CS_HREDRAW | CS_VREDRAW | CS_DBLCLKS,
            lpfnWndProc: Some(wnd_proc),
            hInstance: hinst.into(),
            hCursor: arrow,
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
        // Where the reader left the window, brought back onto the screens there are today.
        let frame = crate::settings::window()
            .map(|f| crate::settings::placed(f, &work_areas()))
            .unwrap_or(FIRST_FRAME);
        let hwnd = CreateWindowExW(
            WS_EX_APPWINDOW,
            PCWSTR(wide.as_ptr()),
            PCWSTR(title.as_ptr()),
            WS_OVERLAPPEDWINDOW,
            frame.left,
            frame.top,
            frame.width,
            frame.height,
            None,
            None,
            Some(hinst.into()),
            Some(&mut *view as *mut View as *const core::ffi::c_void),
        )
        .map_err(|e| -> Error { format!("CreateWindowExW: {e}").into() })?;

        view.attach(hwnd);
        view.update_title(hwnd);
        // Straight to the maximised frame rather than to the normal one and then a state
        // change, because the second way shows the reader the window they did not leave.
        let _ = ShowWindow(hwnd, if frame.maximised { SW_SHOWMAXIMIZED } else { SW_SHOWNORMAL });
        // After the show, because a window that is about to be maximised is resized by being
        // shown -- and a place is a fact about the layout, which that resize has only just
        // redone. Nothing has been drawn yet either way: `WM_PAINT` is a queued message and
        // the first one is not reached until the loop below starts, so the reader still never
        // watches their page begin at the top and move down.
        if let Some(anchor) = restore {
            view.restore_to(anchor);
        }
        let _ = SetTimer(Some(hwnd), APPEARANCE_TIMER, APPEARANCE_TICK_MS, None);
        // Armed even for a page with no file behind it: the sample has no path to poll, and
        // the tick costs a branch, while a document opened later by the dialog or a drop has
        // to find the timer already running.
        let _ = SetTimer(Some(hwnd), DOCUMENT_TIMER, DOCUMENT_TICK_MS, None);

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            if crate::typography::route(&msg) { continue; }
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        let _ = KillTimer(Some(hwnd), APPEARANCE_TIMER);
        let _ = KillTimer(Some(hwnd), DOCUMENT_TIMER);
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
pub(crate) fn utf16(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Whether this machine can draw both halves of a face pairing.
///
/// Asked of the font engine rather than of the theme, because the answer is a fact about
/// the machine: it decides which rows of the menu are live and whether a remembered face
/// is worth setting, neither of which is anything the page has a say in.
pub(crate) fn face_drawable(font: &FontEngine, f: &TextFace) -> bool {
    font.has_family(f.body) && font.has_family(f.heading)
}

/// The character at the top of a window scrolled to `scroll`, counted through the page's
/// characters in reading order.
///
/// A line number is the wrong thing to remember a place with when the place has to
/// survive a reflow, because a reflow is exactly the event that breaks lines apart and
/// joins them up again. The characters do not move, only where the lines end, so an
/// index into them names the same stretch of prose in both layouts.
///
/// `k` is the points-to-pixels scale, since a scroll offset is in points and the lines
/// are painted in pixels.
fn anchor_at(sel: &[SelLine], scroll: f32, k: f32) -> Option<usize> {
    let top = scroll * k;
    let mut seen = 0usize;
    for line in sel {
        // The first line still reaching into the window. One that ends exactly at the
        // top edge is above it, and its text is not what the reader is looking at.
        if line.y + line.h > top {
            return Some(seen);
        }
        seen += line.chars.len();
    }
    None
}

/// Where to scroll so that the line holding `anchor` sits at the top of the window.
///
/// `None` when the new layout has no such line, which leaves the offset alone: a page
/// that has lost the reader's place is worse than one that has not looked for it.
fn scroll_for_anchor(sel: &[SelLine], anchor: usize, k: f32) -> Option<f32> {
    let mut seen = 0usize;
    for (i, line) in sel.iter().enumerate() {
        if anchor < seen + line.chars.len() {
            // The top of the document is the top of the document, not the top of its
            // first line: a reader who was looking at the margin above the text should
            // not lose it because they changed the width of a line further down.
            return Some(if i == 0 { 0.0 } else { line.y / k });
        }
        seen += line.chars.len();
    }
    None
}

fn scroll_for_source(sel: &[SelLine], byte: usize, k: f32) -> Option<f32> {
    if byte == 0 { return Some(0.0); }
    sel.iter().filter_map(|line| {
        let range = line.source.as_ref()?;
        let distance = if byte < range.start { range.start - byte }
            else { byte.saturating_sub(range.end.saturating_sub(1)) };
        Some((distance, line.y / k))
    }).min_by_key(|(distance, _)| *distance).map(|(_, y)| y)
}

fn relocated_source(old: &str, new: &str, byte: usize) -> usize {
    if byte == 0 || old == new { return byte.min(new.len()); }
    let Some(tail) = old.get(byte..) else { return byte.min(new.len()) };
    let context: String = tail.chars().take(80).collect();
    if context.is_empty() { return byte.min(new.len()); }
    new.match_indices(&context).min_by_key(|(at, _)| at.abs_diff(byte))
        .map_or(byte.min(new.len()), |(at, _)| at)
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

/// The work area of every screen there is, in physical pixels.
///
/// The work area rather than the monitor's own extent, because the taskbar is not a place
/// to put a title bar: a reader who docks theirs along the bottom should get their window
/// back above it, not under it.
unsafe fn work_areas() -> Vec<RECT> {
    let mut areas: Vec<RECT> = Vec::new();
    let list = &mut areas as *mut Vec<RECT>;
    let _ = EnumDisplayMonitors(None, None, Some(keep_monitor), LPARAM(list as isize));
    areas
}

/// One monitor, added to the list the caller left in the last parameter.
unsafe extern "system" fn keep_monitor(
    monitor: HMONITOR,
    _dc: HDC,
    _across: *mut RECT,
    state: LPARAM,
) -> BOOL {
    let areas = &mut *(state.0 as *mut Vec<RECT>);
    let mut info = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    if GetMonitorInfoW(monitor, &mut info).as_bool() {
        areas.push(info.rcWork);
    }
    // `true` to be asked for the next one. There is no way to stop this walk early and no
    // reason to want to: the answer is a handful of rectangles, taken once per start-up.
    true.into()
}

/// Write down where the window is standing, and whether it is maximised at the time.
///
/// `GetWindowPlacement` rather than `GetWindowRect`, because the reader is allowed to close
/// a maximised window and what has to come back is the frame they would have gone back to --
/// which is not the screen with its edges off the display, and only means anything at all
/// alongside the flag saying which of the two this window was.
unsafe fn record_geometry(hwnd: HWND) {
    let mut p = WINDOWPLACEMENT {
        length: std::mem::size_of::<WINDOWPLACEMENT>() as u32,
        ..Default::default()
    };
    if GetWindowPlacement(hwnd, &mut p).is_err() {
        return;
    }
    let r = p.rcNormalPosition;
    crate::settings::record_window(crate::settings::Frame {
        left: r.left,
        top: r.top,
        width: r.right - r.left,
        height: r.bottom - r.top,
        maximised: p.showCmd == SW_SHOWMAXIMIZED.0 as u32,
    });
}

/// The two facts about a file that say whether the page on the screen came out of this
/// one of it.
///
/// Length and age rather than a hash, because the question is asked two or three times a
/// second and a hash reads the whole document to answer it. A save that left the file both
/// exactly as long and exactly as old as it was is the one case this can miss, and it is a
/// save that changed nothing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Stamp {
    len: u64,
    /// `None` when the drive will not say, which some network shares will not. The length
    /// is then the only thing that can notice a rewrite, and it usually still can.
    written: Option<std::time::SystemTime>,
}

fn stamp_of(path: &std::path::Path) -> Option<Stamp> {
    let m = std::fs::metadata(path).ok()?;
    Some(Stamp { len: m.len(), written: m.modified().ok() })
}

/// Whether the page on the screen is now a page the file has stopped being.
///
/// A file that is not there to be asked answers `false`, on purpose: the text last read
/// out of it is still true about the document the reader was reading, and a page that
/// blanks itself because an editor renamed a temporary file over the top of it would be
/// three screens of prose lost for a few milliseconds. The next tick finds the new file,
/// and this one keeps the reader where they were standing until it does.
fn is_written(seen: Option<Stamp>, now: Option<Stamp>) -> bool {
    now.is_some() && now != seen
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
            // The last thing the view is asked to do, since after this its lines are
            // already gone and its place with them.
            if let Some(v) = view {
                v.remember_reading();
                if let Err(error) = crate::settings::record_workspace(&v.workspace.snapshot()) {
                    eprintln!("workspace: {error}");
                }
            }
            // The frame goes with the place for the same reason: the usual way to close a
            // window is its own `X`, which asks nothing of the process on the way out.
            record_geometry(hwnd);
            PostQuitMessage(0);
            LRESULT(0)
        }
        // Everything else that a created window can receive goes to the view, which
        // falls through to `DefWindowProcW` itself. Listing the routed messages here as
        // well was a second source of truth, and one the compiler could not check: a
        // handler added below and forgotten above is a click that never arrives.
        _ => match view {
            Some(v) => v.on_message(hwnd, msg, wp, lp),
            None => DefWindowProcW(hwnd, msg, wp, lp),
        },
    }
}

/// The find box's own window procedure, kept when it was subclassed and answered through
/// ever after. `SetWindowLongPtrW` speaks in numbers, and a function pointer is the same
/// number in the other hand.
static EDIT_PROC: AtomicUsize = AtomicUsize::new(0);

/// The find box's procedure, with the keys the search owns lifted out of it.
///
/// A control gets the keyboard before its parent window does, so `Enter` and `Escape`
/// cannot be answered upstairs. Everything else goes back to the procedure the control was
/// made with: an edit box that has been taught to edit nothing is no bargain for three
/// keys, and a reader who cannot correct a mistyped query has no search at all.
unsafe extern "system" fn edit_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let previous: WNDPROC = core::mem::transmute_copy(&EDIT_PROC.load(Ordering::Relaxed));
    if msg == WM_KEYDOWN {
        if let Some(answer) = edit_find_key(hwnd, wp.0 as u32) {
            return answer;
        }
    }
    // `TranslateMessage` has already turned the key down above into this character by the
    // time the procedure is asked about it, so a `Return` that means the search must also
    // be a `Return` that never reaches the query.
    if msg == WM_CHAR && matches!(wp.0 as u32, 0x0D | 0x1B) {
        return LRESULT(0);
    }
    CallWindowProcW(previous, hwnd, msg, wp, lp)
}

/// `Enter` steps to the next hit and `Shift`+`Enter` to the last one; `Escape` closes the
/// bar and hands the keyboard back to the page.
unsafe fn edit_find_key(hwnd: HWND, vk: u32) -> Option<LRESULT> {
    let parent = GetParent(hwnd).ok()?;
    let raw = GetWindowLongPtrW(parent, GWLP_USERDATA);
    if raw == 0 {
        return None;
    }
    let view = &mut *(raw as *mut View);
    let back = held(VK_SHIFT);
    match vk {
        k if k == VK_RETURN.0 as u32 || k == VK_F3.0 as u32 => view.step_find(back, parent),
        k if k == VK_ESCAPE.0 as u32 => view.close_find(parent),
        _ => return None,
    }
    Some(LRESULT(0))
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
        // The thumb is painted from its geometry rather than from an op, because an op
        // would mean relaying out the document on every wheel tick. That leaves it
        // asking for the one brush it needs by name: a document that has no muted text
        // on the page but does overflow the window still has to be able to draw it.
        if self.thumb_rect().is_some() && !roles.contains(&ColorRole::Muted) {
            roles.push(ColorRole::Muted);
        }
        // The bar's panel and its count of matches are drawn from their geometry like the
        // thumb, for the same reason, so the ink they fill with has to be asked for by
        // name rather than found in a list of what the page happens to be made of.
        if self.preview.is_some() {
            for role in [ColorRole::Surface, ColorRole::Text, ColorRole::Muted] {
                if !roles.contains(&role) {
                    roles.push(role);
                }
            }
        }
        if self.find.is_some() {
            for role in [ColorRole::Surface, ColorRole::Muted] {
                if !roles.contains(&role) {
                    roles.push(role);
                }
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
        // The highlight is the accent with most of its opacity given back, because the
        // ink has to stay readable under it. It goes *over* the text rather than under:
        // a code block lays its own opaque panel down after any background would have
        // been drawn, so a band underneath would simply disappear inside the panel.
        if self.sel_brush.is_none() {
            let mut c = d2d(self.palette.accent);
            c.a = 0.28;
            if let Ok(b) = unsafe { rt.CreateSolidColorBrush(&c, None) } {
                self.sel_brush = Some(b);
            }
        }
        // A hit is the same accent and less of it, because everything the search has found
        // is a suggestion of where a word might be -- and the one the reader is on is more
        // of it still, which is what tells a page with forty hits which of them the next
        // `Enter` is about to move away from.
        if self.hit_brush.is_none() {
            let mut c = d2d(self.palette.accent);
            c.a = 0.16;
            self.hit_brush = unsafe { rt.CreateSolidColorBrush(&c, None).ok() };
        }
        if self.focus_brush.is_none() {
            let mut c = d2d(self.palette.accent);
            c.a = 0.45;
            self.focus_brush = unsafe { rt.CreateSolidColorBrush(&c, None).ok() };
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
                self.layout_find();
                let _ = InvalidateRect(Some(hwnd), None, false);
                LRESULT(0)
            }
            WM_EXITSIZEMOVE => {
                // The one message that means the reader has finished putting the window
                // somewhere: it closes a drag and a resize alike. Written as it happens
                // rather than on the way out, because a reader who moves the window and then
                // loses the machine to a power cut has still moved it.
                record_geometry(hwnd);
                DefWindowProcW(hwnd, msg, wp, lp)
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
                // Filed again straight away: a window that has just crossed into another
                // scale is standing at a frame in the new monitor's pixels, and the number
                // remembered before the crossing is one that cannot be restored to anywhere
                // useful.
                record_geometry(hwnd);
                if let Some(t) = &self.target {
                    t.SetDpi(self.dpi, self.dpi);
                }
                // The box's letters were made for the monitor it is leaving, and a font is
                // sized in pixels, so it has to be made again rather than scaled up.
                self.drop_edit_font();
                self.relayout();
                self.layout_find();
                let _ = InvalidateRect(Some(hwnd), None, false);
                LRESULT(0)
            }
            WM_MOUSEWHEEL => {
                let ticks = ((wp.0 >> 16) & 0xFFFF) as i16 as f32;
                if held(VK_SHIFT) {
                    let mut pt = POINT::default();
                    let _ = GetCursorPos(&mut pt);
                    let _ = ScreenToClient(hwnd, &mut pt);
                    if let Some(index) = self.wide_region_at(pt.x as f32, pt.y as f32) {
                        self.wide_active = Some(index);
                    }
                    if self.pan_wide(ticks) {
                        let _ = InvalidateRect(Some(hwnd), None, false);
                    } else {
                        self.scroll_by(-ticks / 120.0 * WHEEL_STEP);
                        let _ = InvalidateRect(Some(hwnd), None, false);
                    }
                } else if held(VK_CONTROL) {
                    // The one gesture every Windows reader already means: `Ctrl` turns
                    // the wheel from a scroller into a magnifier. A notch is a whole
                    // ladder step, which is as fine as a wheel can be made to be.
                    let zoom = if ticks > 0.0 { self.theme.zoom.up() } else { self.theme.zoom.down() };
                    self.zoom_to(zoom, hwnd);
                } else {
                    self.scroll_by(-ticks / 120.0 * WHEEL_STEP);
                    let _ = InvalidateRect(Some(hwnd), None, false);
                }
                LRESULT(0)
            }
            WM_KEYDOWN => {
                let step = self.theme.base * self.theme.body_leading.latin;
                let page = self.page_height();
                let ctrl = held(VK_CONTROL);
                let shift = held(VK_SHIFT);
                if ctrl && wp.0 == 0x33 {
                    self.apply_command(Command::SourceView, hwnd);
                    return LRESULT(0);
                }
                if ctrl && wp.0 == VK_TAB.0 as usize {
                    self.apply_command(
                        if held(VK_SHIFT) { Command::PreviousTab } else { Command::NextTab },
                        hwnd,
                    );
                    return LRESULT(0);
                }
                if ctrl && !shift && wp.0 == VK_W.0 as usize {
                    self.close_active_tab(hwnd);
                    return LRESULT(0);
                }
                // Once the reader has put a caret on the page -- by a click or by an
                // arrow -- the arrows belong to it, up and down included, because that
                // is what they mean on every text surface they have ever used. Before
                // then the same keys are the page's own scroll, which is what a reader
                // who has only used the wheel has no reason to give up.
                let caretless = self.caret.is_none() && self.selection.is_none();
                if let Some(m) = motion_of(wp.0 as u32, ctrl) {
                    // `Shift` is the reader's own answer to "which caret?", so a marked
                    // page never moves by itself: the motion goes to the text even when the
                    // only place to mark from is the top of it.
                    let page_moves = caretless && !shift && self.scroll_page(m, step);
                    if !page_moves {
                        self.move_caret(m, shift);
                    }
                    let _ = InvalidateRect(Some(hwnd), None, false);
                    return LRESULT(0);
                }
                match wp.0 as u32 {
                    k if k == VK_PRIOR.0 as u32 => self.scroll_by(-page),
                    k if k == VK_NEXT.0 as u32 => self.scroll_by(page),
                    // The way back out of a caret the reader has clicked into: while the
                    // page carries a marking, `Escape` gives it up -- and with it the
                    // arrows, which go back to being the scroll. Only an unmarked page
                    // takes the key as a request to close.
                    k if k == VK_ESCAPE.0 as u32 => {
                        if self.preview.take().is_some() {
                            let _ = unsafe { InvalidateRect(Some(hwnd), None, false) };
                        } else if self.caret.is_some() || self.selection.is_some() {
                            self.caret = None;
                            self.selection = None;
                        } else {
                            PostQuitMessage(0);
                        }
                    }
                    // Both of these go through the menu's own command, so the key and the
                    // row it repeats cannot drift apart -- one of them is written in terms
                    // of the other.
                    k if k == VK_O.0 as u32 && ctrl && shift => self.apply_command(Command::OpenEditor, hwnd),
                    k if k == VK_O.0 as u32 && ctrl => self.apply_command(Command::OpenFile, hwnd),
                    // A document with no file behind it has nothing to read again, which is
                    // the same condition that dims the row.
                    k if k == VK_R.0 as u32 && ctrl => self.apply_command(Command::Reload, hwnd),
                    // The virtual-key codes name physical keys, so both the shifted and
                    // unshifted form of the same key (`+` and `=`) arrive as one of
                    // these, and the numpad's own `+`/`-` mean the same thing to a
                    // reader reaching for the zoom.
                    k if ctrl && (k == VK_OEM_PLUS.0 as u32 || k == VK_ADD.0 as u32) => {
                        let z = self.theme.zoom.up();
                        self.zoom_to(z, hwnd);
                    }
                    k if ctrl && (k == VK_OEM_MINUS.0 as u32 || k == VK_SUBTRACT.0 as u32) => {
                        let z = self.theme.zoom.down();
                        self.zoom_to(z, hwnd);
                    }
                    k if ctrl && (k == VK_0.0 as u32 || k == VK_NUMPAD0.0 as u32) => {
                        self.zoom_to(Zoom::DESIGN, hwnd);
                    }
                    // The bracket keys, which are the shape of the thing: two margins
                    // moving apart or together. Nothing collides with them: a reader never
                    // types a bare bracket, and the one text box this window grows has its
                    // own procedure answering its keys before any of these is asked.
                    k if ctrl && k == VK_OEM_4.0 as u32 => {
                        let m = step_measure(self.theme.measure, -1);
                        self.set_measure(m, hwnd);
                    }
                    k if ctrl && k == VK_OEM_6.0 as u32 => {
                        let m = step_measure(self.theme.measure, 1);
                        self.set_measure(m, hwnd);
                    }
                    // Both palettes are always one keypress apart, whichever way the
                    // system setting points: a reader in a bright room at 2pm has the
                    // same claim on the dark one as anyone whose OS says so.
                    k if ctrl && k == VK_D.0 as u32 => {
                        let dark = !self.palette.dark;
                        self.apply_command(Command::Palette(dark), hwnd);
                    }
                    // A copy with nothing selected leaves the clipboard alone. Clearing
                    // it would throw away what the reader put there from somewhere else,
                    // to no purpose: an empty selection is not an edit.
                    k if k == VK_C.0 as u32 && ctrl => self.copy_selection(hwnd),
                    k if k == VK_A.0 as u32 && ctrl => self.select_all(),
                    k if k == VK_F.0 as u32 && ctrl => self.open_find(hwnd),
                    // `F3` is the same key the box answers to while it has the keyboard, so
                    // a reader who has closed the bar with `Escape` is one press from the
                    // phrase they were looking at -- with the query still in the box.
                    k if k == VK_F3.0 as u32 => {
                        if self.find.is_some() {
                            let back = held(VK_SHIFT);
                            self.step_find(back, hwnd);
                        } else {
                            self.open_find(hwnd);
                        }
                    }
                    // A key this window has no use for goes back to the default handler,
                    // which is what turns the keyboard's menu key -- and `Shift`+`F10` --
                    // into the `WM_CONTEXTMENU` that opens the reader's menu. Answering
                    // every unclaimed key with `0` here would silently cost the one way
                    // there is to reach that menu without a mouse.
                    _ => return DefWindowProcW(hwnd, msg, wp, lp),
                }
                let _ = InvalidateRect(Some(hwnd), None, false);
                LRESULT(0)
            }
            // `Alt` plus an arrow arrives here rather than at `WM_KEYDOWN`, which is why
            // the two steps cannot ride on the handler above. Everything this arm does not
            // want is handed straight back, because the default handler is what makes
            // `Alt`+`F4` close a window and `Alt`+`Space` open the system menu.
            WM_SYSKEYDOWN => {
                let k = wp.0 as u32;
                let alt = held(VK_MENU);
                if alt && held(VK_CONTROL) && (k == VK_UP.0 as u32 || k == VK_DOWN.0 as u32) {
                    self.apply_command(
                        if k == VK_DOWN.0 as u32 { Command::NextChapter } else { Command::PreviousChapter },
                        hwnd,
                    );
                    LRESULT(0)
                } else if alt && held(VK_CONTROL) && (k == VK_LEFT.0 as u32 || k == VK_RIGHT.0 as u32) {
                    self.apply_command(Command::Neighbor(k == VK_RIGHT.0 as u32), hwnd);
                    LRESULT(0)
                } else if alt && k == VK_LEFT.0 as u32 {
                    self.apply_command(Command::GoBack, hwnd);
                    LRESULT(0)
                } else if alt && k == VK_RIGHT.0 as u32 {
                    self.apply_command(Command::GoForward, hwnd);
                    LRESULT(0)
                } else {
                    DefWindowProcW(hwnd, msg, wp, lp)
                }
            }
            // The find box has new words in it. Asked about on every keystroke rather than
            // when `Enter` is pressed, because a search that waits is a reader pressing
            // `Enter` to find out whether the word they meant is on the page at all.
            crate::typography::APPLIED => {
                let plain = self.plain_override.unwrap_or_else(|| reading::is_plain(self.path.as_deref()));
                self.apply_profile(crate::profiles::selected(plain), hwnd);
                LRESULT(0)
            }
            WM_COMMAND => {
                let (id, code) = (wp.0 & 0xFFFF, ((wp.0 >> 16) & 0xFFFF) as u32);
                if id == FIND_EDIT_ID && code == EN_CHANGE {
                    let query = self.edit_text();
                    self.apply_find(&query, true);
                    let _ = InvalidateRect(Some(hwnd), None, false);
                    LRESULT(0)
                } else {
                    DefWindowProcW(hwnd, msg, wp, lp)
                }
            }
            // The one piece of this window the painter cannot reach: a child control is
            // erased by USER32, which asks here what ink to use for it.
            WM_CTLCOLOREDIT => {
                let dc = HDC(wp.0 as *mut core::ffi::c_void);
                SetTextColor(dc, cref(self.palette.text));
                SetBkColor(dc, cref(self.palette.bg));
                if self.edit_brush.is_none() {
                    self.edit_brush = Some(CreateSolidBrush(cref(self.palette.bg)));
                }
                LRESULT(self.edit_brush.map_or(0, |b| b.0 as isize))
            }
            WM_CONTEXTMENU => {
                // `wParam` is the window and `lParam` the screen point the right button
                // was over -- except when the key that opened this was the keyboard's
                // menu key, which reports no point at all and takes the pointer's.
                let (x, y) = if lp.0 == -1 {
                    let mut pt = POINT::default();
                    let _ = GetCursorPos(&mut pt);
                    (pt.x, pt.y)
                } else {
                    (((lp.0 & 0xFFFF) as u16) as i16 as i32, (((lp.0 >> 16) as i32) << 16 >> 16))
                };
                let _ = SetForegroundWindow(hwnd);
                self.popup_menu(x, y, hwnd);
                LRESULT(0)
            }
            WM_TIMER => {
                match wp.0 {
                    DOCUMENT_TIMER => self.reload_if_written(hwnd),
                    _ => {
                        // A poll, not a push: there is no message for this setting. The
                        // reader's own choice outranks it, and `set_dark` does nothing when
                        // the answer it gets is already on screen.
                        let dark = self.dark_override.unwrap_or_else(system_prefers_dark);
                        self.set_dark(dark, hwnd);
                        let _ = InvalidateRect(Some(hwnd), None, false);
                    }
                }
                LRESULT(0)
            }
            WM_DROPFILES => {
                self.open_from_drop(hwnd, wp.0);
                LRESULT(0)
            }
            WM_LBUTTONDOWN | WM_LBUTTONDBLCLK => {
                // A press inside the thumb starts a drag; a press on a target waits for
                // the release; a press anywhere else belongs to the text.
                let x = ((lp.0 & 0xFFFF) as i16) as f32;
                let y = ((lp.0 >> 16) as i16) as f32;
                // A press on the bar is not a press on the page. The box itself is a window
                // and never reaches this handler; this is the strip of paint around it,
                // under which the reader's own text is still lying.
                if self.preview.is_some() {
                    self.preview = None;
                    let _ = InvalidateRect(Some(hwnd), None, false);
                    return LRESULT(0);
                }
                if self.find.is_some() && in_find_panel(x, y, self.client_w) {
                    return LRESULT(0);
                }
                self.pressed = None;
                if self.thumb_hit(x, y) {
                    self.dragging = true;
                    self.scroll_to_thumb(y);
                } else if let Some(i) = self.hot_at(x, y) {
                    self.pressed = Some(i);
                } else {
                    let c = self.caret_under(x, y);
                    // The second click of a pair expands the place the first one landed
                    // on into a word. A press that pages the document instead would make
                    // that pair scroll away halfway through, which is why the click in
                    // empty prose is no longer a page turn.
                    self.selection = if msg == WM_LBUTTONDBLCLK {
                        let w = word_at(&self.sel_index, c);
                        (w.from != w.to).then_some(w)
                    } else {
                        None
                    };
                    self.press_caret = Some(c);
                    // Wherever the pointer lands is where the arrows go on from, so a
                    // reader can click into a paragraph and finish the selection with the
                    // keyboard without the caret starting at the top of the page.
                    self.caret = Some(c);
                }
                self.press_at = Some((x, y));
                let _ = SetCapture(hwnd);
                let _ = InvalidateRect(Some(hwnd), None, false);
                LRESULT(0)
            }
            WM_SETCURSOR => {
                // Asked to answer on every move over the window. Only the client area is
                // ours: over the frame the default decides, and overriding it there would
                // cost the resize cursors too.
                if (lp.0 & 0xFFFF) as u32 == HTCLIENT {
                    let mut pt = POINT::default();
                    let _ = GetCursorPos(&mut pt);
                    let _ = ScreenToClient(hwnd, &mut pt);
                    let over = self.hot_at(pt.x as f32, pt.y as f32).is_some();
                    SetCursor(Some(if over { self.hand } else { self.arrow }));
                    LRESULT(1)
                } else {
                    DefWindowProcW(hwnd, msg, wp, lp)
                }
            }
            WM_MOUSEMOVE => {
                let x = ((lp.0 & 0xFFFF) as i16) as f32;
                let y = ((lp.0 >> 16) as i16) as f32;
                if self.dragging {
                    self.scroll_to_thumb(y);
                    let _ = InvalidateRect(Some(hwnd), None, false);
                } else if let Some(from) = self.press_caret {
                    // A button that has travelled since it went down in the text is no
                    // longer aiming at a place: it is drawing a selection, and it goes on
                    // drawing one until it comes up. The slop is what keeps a click on a
                    // word from highlighting the word it landed inside of.
                    let dragged = self
                        .press_at
                        .is_some_and(|(px, py)| (x - px).abs() > DRAG_SLOP || (y - py).abs() > DRAG_SLOP);
                    if dragged {
                        self.auto_scroll(y);
                        let to = self.caret_under(x, y);
                        self.selection = Some(Selection { from, to });
                        let _ = InvalidateRect(Some(hwnd), None, false);
                    }
                }
                LRESULT(0)
            }
            WM_LBUTTONUP => {
                // Captured on every press, so released on every release: a window that
                // keeps the capture after a plain click takes the mouse away from the
                // rest of the desktop, thumb drags being the only case it is meant for.
                let _ = ReleaseCapture();
                self.dragging = false;
                // The drag is over, but what it drew stays selected: a reader lets go of
                // the button to look at the selection, not to be quit out of it.
                self.press_at = None;
                self.press_caret = None;
                if let Some(i) = self.pressed.take() {
                    let x = ((lp.0 & 0xFFFF) as i16) as f32;
                    let y = ((lp.0 >> 16) as i16) as f32;
                    // Released where it began: the press was a click rather than a reader
                    // reaching past the target and deciding against it.
                    if self.hot_at(x, y) == Some(i) {
                        self.activate(i, hwnd);
                    }
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
        self.scroll += dy;
        self.clamp_scroll();
    }

    /// The target under a pointer position, given in device independent pixels from the
    /// window's client origin. The rectangles are stored against the top of the
    /// document, so the only translation the test needs is the scroll.
    fn hot_at(&self, x: f32, y: f32) -> Option<usize> {
        let y = y + scroll_dip(self.scroll, self.dpi);
        self.hotspots
            .iter()
            .position(|h| {
                let shift = self
                    .wide_active
                    .and_then(|i| self.wide_regions.get(i))
                    .map_or(0.0, |r| region_shift(r, self.wide_offset, h.x, h.y));
                let hx = h.x + shift;
                x >= hx && x <= hx + h.w && y >= h.y && y <= h.y + h.h
            })
    }

    fn shift_at(&self, x: f32, y: f32) -> f32 {
        self.wide_active
            .and_then(|i| self.wide_regions.get(i))
            .map_or(0.0, |r| region_shift(r, self.wide_offset, x, y))
    }

    fn pan_wide(&mut self, ticks: f32) -> bool {
        let Some(index) = self.wide_active else { return false };
        let Some(region) = self.wide_regions.get(index).cloned() else { return false };
        self.wide_offset = pan_offset(
            region.content_w,
            region.w,
            self.wide_offset + ticks / 120.0 * WHEEL_STEP * scale_of(self.dpi),
        );
        true
    }

    fn wide_region_at(&self, x: f32, y: f32) -> Option<usize> {
        let y = y + scroll_dip(self.scroll, self.dpi);
        self.wide_regions.iter().position(|r| {
            let shift = self
                .wide_active
                .and_then(|active| self.wide_regions.get(active))
                .map_or(0.0, |active| region_shift(active, self.wide_offset, r.x, r.y));
            let left = r.x + shift;
            x >= left && x <= left + r.w && y >= r.y && y <= r.y + r.h
        })
    }

    /// The place in the page's text under a pointer position given in client pixels,
    /// translated into document pixels the same way [`View::hot_at`] translates.
    fn caret_under(&self, x: f32, y: f32) -> Caret {
        let shift = self.shift_at(x, y + scroll_dip(self.scroll, self.dpi));
        caret_at(&self.sel_index, x - shift, y + scroll_dip(self.scroll, self.dpi))
    }

    /// Scroll the page when a drag is held against its top or bottom edge, so a
    /// selection longer than the viewport can be finished without letting go.
    ///
    /// The band is narrow and the step small because this runs on every pointer message
    /// while the button is down near an edge, including the ones that do not move: too
    /// fast reads as a page flipping under a stationary cursor.
    fn auto_scroll(&mut self, y: f32) {
        const EDGE: f32 = 20.0;
        const STEP: Pt = 6.0;
        if y < EDGE {
            self.scroll_by(-STEP);
        } else if y > self.client_h - EDGE {
            self.scroll_by(STEP);
        }
    }

    /// `Ctrl`+`A`: every line the page drew, from the first character of the first to
    /// after the last of the last. A drag across a whole document has to be slow enough
    /// for the edge auto-scroll to keep up; this does not.
    fn select_all(&mut self) {
        let Some(last) = self.sel_index.last() else { return };
        self.selection = Some(Selection {
            from: Caret { line: 0, ch: 0 },
            to: Caret { line: self.sel_index.len() - 1, ch: last.chars.len() },
        });
        self.press_caret = None;
    }

    /// Take the caret one motion away, holding or dropping the selection as `Shift` says.
    ///
    /// With it, the end the reader started from stays where it is and the far end walks
    /// away, which is what makes a selection out of repeated presses. Without it the
    /// marking is finished: the caret collapses to whichever edge the motion heads away
    /// from, because an arrow pressed at the end of a selection means "on from here", and
    /// `here` is the end it was read to.
    fn move_caret(&mut self, m: Motion, extend: bool) {
        let (anchor, focus) = match self.selection {
            Some(s) if extend => (s.from, s.to),
            Some(s) => {
                let o = s.ordered();
                let c = if m.backward() { o.from } else { o.to };
                (c, c)
            }
            None => {
                let c = self.caret.unwrap_or(Caret { line: 0, ch: 0 });
                (c, c)
            }
        };
        let to = next_caret(&self.sel_index, focus, m);
        self.caret = Some(to);
        self.selection = (to != anchor).then_some(Selection { from: anchor, to });
        self.scroll_to_caret(to);
    }

    /// Move the page instead of a caret, on a page that has none: one line for the
    /// arrows, one of the document's two ends for `Home` and `End` -- with or without
    /// `Ctrl`, since a page with nothing focused has no line for those keys to be the ends
    /// of. Summing instead a caret at the start of the first line, as the motions would on
    /// their own, is a page that appears not to answer `End`: nothing it holds is further
    /// from the top than the place the key left the reader.
    ///
    /// `false` for the sideways motions, which stay the reader's way of putting a caret on
    /// the page without a mouse -- and from then the same keys mean what they mean
    /// everywhere else.
    fn scroll_page(&mut self, m: Motion, step: Pt) -> bool {
        match m {
            Motion::Up => self.scroll_by(-step),
            Motion::Down => self.scroll_by(step),
            Motion::Home | Motion::DocStart => self.go_to_end(true),
            Motion::End | Motion::DocEnd => self.go_to_end(false),
            Motion::Left | Motion::Right | Motion::WordLeft | Motion::WordRight => return false,
        }
        true
    }

    /// Take the page to one of its ends, leaving no caret behind to be the next arrow's
    /// starting point.
    fn go_to_end(&mut self, top: bool) {
        self.scroll = if top { 0.0 } else { Pt::MAX };
        self.clamp_scroll();
    }

    /// Bring the line the caret is on into the window, by the least movement that does.
    fn scroll_to_caret(&mut self, c: Caret) {
        let Some(l) = self.sel_index.get(c.line) else { return };
        let k = scale_of(self.dpi);
        let (top, bottom) = (l.y / k, (l.y + l.h) / k);
        let view = self.client_h / k;
        if top < self.scroll {
            self.scroll = top;
        } else if bottom > self.scroll + view {
            self.scroll = bottom - view;
        }
        self.clamp_scroll();
    }

    /// Act on the target at this index in [`View::hotspots`].
    fn activate(&mut self, i: usize, hwnd: HWND) {
        let Some(kind) = self.hotspots.get(i).map(|h| h.kind.clone()) else { return };
        match kind {
            HotKind::Url(url) => open_url(&url),
            HotKind::Cite(note) => self.jump_to(self.note_tops.get(note).copied(), hwnd),
            HotKind::Heading(at) => self.jump_to(self.anchor_tops.get(at).copied(), hwnd),
            HotKind::Wide(index) => {
                let Some(region) = self.wide_regions.get(index).cloned() else { return };
                match region.kind {
                    WideKind::Image(path) => self.preview = Some(Preview::Image(path)),
                    WideKind::Formula(source) => self.preview = Some(Preview::Formula(source)),
                    WideKind::Table => {
                        self.wide_active = Some(index);
                        self.wide_offset = 0.0;
                    }
                }
                let _ = unsafe { InvalidateRect(Some(hwnd), None, false) };
            }
            HotKind::Document(target) => {
                let from = self.here();
                let same = self.path.as_deref() == Some(target.path.as_path());
                if !same {
                    self.load_document(&target.path, hwnd);
                }
                if let Some(at) = target.fragment.as_deref().and_then(|f| heading_index(&self.doc, f)) {
                    if let Some(top) = self.anchor_tops.get(at) {
                        self.scroll = (*top - self.theme.base).max(0.0);
                    }
                }
                self.clamp_scroll();
                if same && (self.scroll - from.scroll).abs() > 0.5 {
                    self.history.leave(from);
                }
                self.remember_reading();
                let _ = unsafe { InvalidateRect(Some(hwnd), None, false) };
            }
        }
    }

    /// Bring a place on this page to the top of the window.
    ///
    /// It lands a line below the edge rather than against it, so some of what the
    /// reader just left stays in view: a jump is followed in order to read, and reading
    /// means being able to tell where one came from -- and to get back there.
    fn jump_to(&mut self, top: Option<Pt>, hwnd: HWND) {
        let Some(top) = top else { return };
        let from = self.here();
        self.scroll = (top - self.theme.base).max(0.0);
        self.clamp_scroll();
        // Only a jump that goes somewhere is a step, which keeps the deliberate ones --
        // a citation, a table of contents entry -- in the history and the aimless ones out
        // of it: a page whose every click on an already-visible heading added a rung would
        // turn `Back` into a way of walking backwards through one screen of text.
        if (self.scroll - from.scroll).abs() > 0.5 {
            self.history.leave(from);
        }
        let _ = unsafe { InvalidateRect(Some(hwnd), None, false) };
    }

    /// The address of the link under this client point, when one of those is what the
    /// pointer is on. A jump within the page answers to a click well enough that the
    /// menu has nothing to add, and a citation of a note is already on screen.
    fn pointer_link(&self, x: f32, y: f32) -> Option<String> {
        let kind = self.hot_at(x, y).and_then(|i| self.hotspots.get(i)).map(|h| h.kind.clone())?;
        match kind {
            HotKind::Url(u) => Some(u),
            _ => None,
        }
    }

    /// Open the menu at a screen point and do what was picked from it.
    ///
    /// The window comes forward first: a menu opened over a window that is not the
    /// frontmost one takes the click that was meant to dismiss it, which is documented
    /// behaviour of `TrackPopupMenu` rather than a curiosity, and the reason for the
    /// message after the menu has gone -- without it the window has no particular reason
    /// to notice the menu is no longer there.
    fn popup_menu(&mut self, x: i32, y: i32, hwnd: HWND) {
        let mut pt = POINT { x, y };
        let _ = unsafe { ScreenToClient(hwnd, &mut pt) };
        let state = MenuState {
            profiles: crate::profiles::names(),
            profile: self.profile.clone(),
            source_view: self.source_view,
            plain_override: self.plain_override,
            text_options: self.text_options,
            chapter_titles: self
                .chapter_index
                .as_ref()
                .map(|index| index.chapters().iter().map(|chapter| chapter.title.clone()).collect())
                .unwrap_or_default(),
            chapter: self.chapter,
            tabs: self
                .workspace
                .tabs
                .items()
                .iter()
                .map(|tab| {
                    let title = match &tab.document {
                        DocumentRef::Sample => "Sample".to_string(),
                        DocumentRef::File(file) => file.path.file_name()
                            .map(|name| name.to_string_lossy().into_owned())
                            .unwrap_or_else(|| file.path.to_string_lossy().into_owned()),
                    };
                    (tab.id, title, tab.id == self.workspace.tabs.active().id)
                })
                .collect(),
            recent: self.workspace.recent.items().iter().enumerate()
                .filter(|(_, file)| file.path.is_file())
                .map(|(i, file)| (i, file.path.to_string_lossy().into_owned()))
                .collect(),
            tree: self.tree.iter().enumerate()
                .map(|(i, entry)| (i, format!("{}{}", "  ".repeat(entry.depth), entry.name), entry.kind == TreeEntryKind::Directory))
                .collect(),
            encoding: self.encoding,
            encoding_notice: Some(format!("{}{}", self.decoded_encoding.label(),
                if self.encoding_guessed { " (detected by guess; choose if incorrect)" } else { "" })),
            previous: self.path.as_deref().and_then(|p| reading::neighbor(p, false)).is_some(),
            next: self.path.as_deref().and_then(|p| reading::neighbor(p, true)).is_some(),
            keep_line_breaks: self.keep_line_breaks,
            line_break_override: self.line_break_override,
            can_back: self.history.leads(true),
            can_forward: self.history.leads(false),
            link: self.pointer_link(pt.x as f32, pt.y as f32),
            selected: self.selection.is_some_and(|s| s.from != s.to),
            dark: self.dark_override,
            from_file: self.path.is_some(),
            text: !self.sel_index.is_empty(),
            face: self.theme.face,
            measure: self.theme.measure,
            headings: outline(&self.doc, self.anchor_tops.len()),
            offered: TextFace::ALL.iter().map(|f| face_drawable(&self.font, f)).collect(),
        };
        let menu = match unsafe { CreatePopupMenu() } {
            Ok(m) => m,
            Err(_) => return,
        };
        // A row's id is the number it was appended with, taken from one counter shared by
        // the whole tree of menus, so the number the menu answers with reads back into its
        // command without a second table to keep in step. Shared rather than per-menu
        // because a submenu's rows are appended to a different menu, and their positions
        // would otherwise repeat numbers already in use above them.
        let mut rows: HashMap<usize, Command> = HashMap::new();
        let mut next = 1usize;
        append_rows(&menu, &menu_items(&state), &mut next, &mut rows);
        // The flags argument is a plain `u32` here rather than the flag newtype the other
        // menu calls take, which is what makes the `.0` necessary.
        let picked = unsafe {
            TrackPopupMenuEx(menu, (TPM_RETURNCMD | TPM_RIGHTBUTTON).0, x, y, hwnd, None)
        };
        let _ = unsafe { DestroyMenu(menu) };
        if let Some(cmd) = rows.remove(&(picked.0.max(0) as usize)) {
            self.apply_command(cmd, hwnd);
        }
        let _ = unsafe { PostMessageW(Some(hwnd), WM_NULL, WPARAM(0), LPARAM(0)) };
    }

    /// Do one of the things the menu offers, most of which are also keys.
    fn apply_command(&mut self, cmd: Command, hwnd: HWND) {
        match cmd {
            Command::GoBack => self.go_back(hwnd),
            Command::GoForward => self.go_forward(hwnd),
            // The same jump a table of contents link makes, so the two cannot land in
            // different places: both go through `jump_to`, which is what decides where the
            // top of a heading sits on the screen and whether this was a step at all.
            Command::Heading(i) => self.jump_to(self.anchor_tops.get(i).copied(), hwnd),
            Command::Copy => self.copy_selection(hwnd),
            Command::SelectAll => self.select_all(),
            Command::Find => unsafe { self.open_find(hwnd) },
            Command::OpenUrl(u) => open_url(&u),
            Command::CopyUrl(u) => {
                if let Err(e) = clipboard::copy_text(hwnd, &u) {
                    eprintln!("clipboard: {e}");
                }
            }
            Command::ZoomIn => {
                let z = self.theme.zoom.up();
                self.zoom_to(z, hwnd);
            }
            Command::ZoomOut => {
                let z = self.theme.zoom.down();
                self.zoom_to(z, hwnd);
            }
            Command::ZoomReset => self.zoom_to(Zoom::DESIGN, hwnd),
            Command::Face(i) => self.set_face(i, hwnd),
            Command::Measure(i) => self.set_measure(i, hwnd),
            Command::Palette(dark) => {
                self.dark_override = Some(dark);
                self.remember();
                self.set_dark(dark, hwnd);
            }
            Command::FollowSystem => {
                self.dark_override = None;
                self.remember();
                let dark = system_prefers_dark();
                self.set_dark(dark, hwnd);
            }
            Command::OpenFile => {
                if let Some(path) = unsafe { self.prompt_for_file(hwnd) } {
                    self.load_document(&path, hwnd);
                }
            }
            Command::OpenRecent(index) => {
                if let Some(path) = self.workspace.recent.items().get(index).map(|file| file.path.clone()) {
                    self.load_document(&path, hwnd);
                }
            }
            Command::OpenTree(index) => {
                if let Some(path) = self.tree.get(index).map(|entry| entry.path.clone()) {
                    self.load_document(&path, hwnd);
                }
            }
            Command::TreeDirectory(index) => {
                if let Some(path) = self.tree.get(index).map(|entry| entry.path.clone()) {
                    self.tree = tree::scan(&path, 2);
                }
            }
            Command::ActivateTab(id) => { self.switch_to_tab(id, hwnd); }
            Command::CloseTab(id) => {
                if self.workspace.tabs.active().id == id {
                    self.close_active_tab(hwnd);
                } else {
                    self.workspace.close(id);
                }
            }
            Command::PinTab(id) => { self.workspace.pin(id); }
            Command::NextTab => self.switch_relative_tab(1, hwnd),
            Command::PreviousTab => self.switch_relative_tab(-1, hwnd),
            Command::Reload => {
                if let Some(path) = self.path.clone() {
                    if let Ok(decoded) = reading::read(&path, self.encoding) {
                        self.reload_text(decoded, hwnd);
                        self.stamp = stamp_of(&path);
                    }
                }
            }
            Command::DefaultLineBreaks(keep) => {
                self.keep_line_breaks = keep;
                crate::settings::record_line_breaks(keep);
                self.reparse(hwnd);
            }
            Command::DocumentLineBreaks(keep) => {
                self.line_break_override = keep;
                self.reparse(hwnd);
            }
            Command::SourceView => {
                self.source_view = !self.source_view;
                self.reparse(hwnd);
            }
            Command::PlainText(plain) => {
                self.plain_override = plain;
                self.reparse(hwnd);
            }
            Command::TextParagraphs(rule) => {
                self.text_options.paragraphs = rule;
                self.reparse(hwnd);
            }
            Command::DetectChapters(detect) => {
                self.text_options.chapters = detect;
                self.reparse(hwnd);
            }
            Command::TextEncoding(encoding) => {
                if let Some(path) = self.path.as_deref() {
                    match reading::read(path, encoding) {
                        Ok(decoded) => {
                            self.encoding = encoding;
                            self.reload_text(decoded, hwnd);
                        }
                        Err(error) => show_error(hwnd, &error.to_string()),
                    }
                }
            }
            Command::Neighbor(forward) => {
                if let Some(path) = self.path.as_deref().and_then(|p| reading::neighbor(p, forward)) {
                    self.load_document(&path, hwnd);
                }
            }
            Command::WideTableNarrow => self.set_wide_table_factor(self.theme.wide_table_factor - 0.1, hwnd),
            Command::WideTableWiden => self.set_wide_table_factor(self.theme.wide_table_factor + 0.1, hwnd),
            Command::PreviousChapter => self.switch_chapter(-1, hwnd),
            Command::NextChapter => self.switch_chapter(1, hwnd),
            Command::OpenEditor => {
                if let Some(path) = self.path.as_deref() {
                    use std::os::windows::process::CommandExt;
                    let editor = crate::settings::editor();
                    let byte = self.caret_source().or_else(|| self.source_anchor()).unwrap_or(0);
                    let (line, column) = reading::line_column(&self.source, byte);
                    let name = std::path::Path::new(&editor)
                        .file_stem().and_then(|s| s.to_str()).unwrap_or("").to_ascii_lowercase();
                    let template = crate::settings::editor_args();
                    let args = if template.trim().is_empty()
                        && matches!(name.as_str(), "code" | "code-insiders" | "subl" | "sublime_text" | "devenv") {
                        vec![format!("{}:{}:{}", path.display(), line, column)]
                    } else {
                        editor_arguments(&template, path, line, column)
                    };
                    if let Err(error) = std::process::Command::new(editor)
                        .args(args).creation_flags(0x08000000).spawn()
                    { show_error(hwnd, &format!("Cannot start editor: {error}")); }
                }
            }
            Command::ChooseEditor => {
                if let Some(path) = unsafe { self.prompt_file(hwnd, "Applications\0*.exe\0All files\0*.*\0\0") } {
                    crate::settings::record_editor(&path);
                }
            }
            Command::Typography => {
                let plain = self.plain_override.unwrap_or_else(|| reading::is_plain(self.path.as_deref()));
                if let Err(error) = crate::typography::show(hwnd, &self.theme, plain) { show_error(hwnd, &error.to_string()); }
            }
            Command::Profile(name) => {
                let plain = self.plain_override.unwrap_or_else(|| reading::is_plain(self.path.as_deref()));
                if let Err(error) = crate::profiles::select(&name, plain) {
                    eprintln!("typography: {error}");
                }
                self.apply_profile(name, hwnd);
            }
        }
        let _ = unsafe { InvalidateRect(Some(hwnd), None, false) };
    }

    /// Write down the appearance the reader is looking at, for the next window.
    fn remember(&self) {
        crate::settings::record(
            self.theme.zoom,
            self.dark_override,
            self.theme.face,
            self.theme.measure,
        );
    }

    fn parse_source(&self) -> Document {
        if self.source_view { return Document::source(&self.source); }
        if self.plain_override.unwrap_or_else(|| reading::is_plain(self.path.as_deref())) {
            if let Some(index) = self.chapter_index.as_ref() {
                return index.window(&self.source, self.chapter, self.text_options);
            }
            return rubrica_doc::plain::parse(&self.source, self.text_options);
        }
        Document::parse_with(&self.source, rubrica_doc::ParseOptions {
            keep_line_breaks: self.line_break_override.unwrap_or(self.keep_line_breaks),
        })
    }

    fn refresh_chapter_index(&mut self) {
        let plain = self.plain_override.unwrap_or_else(|| reading::is_plain(self.path.as_deref()));
        self.chapter_index = if plain && self.text_options.chapters {
            Some(ChapterIndex::new(&self.source, true))
        } else {
            None
        };
        if let Some(index) = self.chapter_index.as_ref() {
            self.chapter = self.chapter.min(index.chapters().len().saturating_sub(1));
        } else {
            self.chapter = 0;
        }
    }

    fn switch_chapter(&mut self, delta: isize, hwnd: HWND) {
        let Some(index) = self.chapter_index.as_ref() else { return };
        let count = index.chapters().len();
        if count == 0 { return; }
        let next = (self.chapter as isize + delta).clamp(0, count as isize - 1) as usize;
        if next == self.chapter { return; }
        self.chapter = next;
        self.doc = self.parse_source();
        self.scroll = 0.0;
        self.relayout();
        if let Some(chapter) = self.chapter_index.as_ref().and_then(|i| i.chapters().get(self.chapter)) {
            if let Some(scroll) = scroll_for_source(&self.sel_index, chapter.range.start, scale_of(self.dpi)) {
                self.scroll = scroll;
            }
        }
        self.clamp_scroll();
        self.update_title(hwnd);
        let _ = unsafe { InvalidateRect(Some(hwnd), None, false) };
    }

    fn reparse(&mut self, hwnd: HWND) {
        let source = self.source_anchor();
        let profile = crate::profiles::selected(self.plain_override.unwrap_or_else(|| reading::is_plain(self.path.as_deref())));
        if profile != self.profile {
            crate::profiles::load(&profile).apply(&mut self.theme);
            self.profile = profile;
            self.math = MathStore::new();
        }
        self.refresh_chapter_index();
        self.doc = self.parse_source();
        self.relayout_in_place(hwnd);
        if let Some(byte) = source { self.restore_source(byte); }
        self.update_title(hwnd);
        self.remember_document();
        self.remember_reading();
    }

    fn source_anchor(&self) -> Option<usize> {
        if self.scroll == 0.0 { return Some(0); }
        self.sel_index.iter().find(|line| line.y + line.h > self.scroll * scale_of(self.dpi))
            .and_then(|line| line.source.as_ref().map(|range| range.start))
    }

    fn caret_source(&self) -> Option<usize> {
        let caret = self.caret?;
        let line = self.sel_index.get(caret.line)?;
        let start = line.source.as_ref()?.start;
        let prefix = line.chars.iter().take(caret.ch).map(|c| c.len_utf8()).sum::<usize>();
        Some(start + prefix)
    }

    fn restore_source(&mut self, byte: usize) {
        if let Some(scroll) = scroll_for_source(&self.sel_index, byte, scale_of(self.dpi)) { self.scroll = scroll; }
        self.clamp_scroll();
    }

    fn reload_text(&mut self, decoded: reading::Decoded, hwnd: HWND) {
        let source = self.source_anchor().map(|byte| relocated_source(&self.source, &decoded.text, byte));
        self.accept_decoded(decoded);
        self.reparse(hwnd);
        if let Some(byte) = source { self.restore_source(byte); }
        self.remember_reading();
    }

    fn remember_document(&self) {
        if let Some(path) = self.path.as_deref() {
            crate::settings::record_document(path, crate::settings::DocumentSettings {
                line_breaks: self.line_break_override, plain: self.plain_override,
                source: self.source_view, text: self.text_options, encoding: self.encoding,
            });
        }
    }

    fn apply_profile(&mut self, name: String, hwnd: HWND) {
        crate::profiles::load(&name).apply(&mut self.theme);
        self.profile = name;
        self.math = MathStore::new();
        self.relayout_in_place(hwnd);
        self.remember();
    }

    fn update_title(&self, hwnd: HWND) {
        let mut title = window_title(self.path.as_deref());
        if self.source_view { title.push_str(" [Source]"); }
        if self.encoding_guessed { title.push_str(&format!(" [{}?]", self.decoded_encoding.label())); }
        let title = utf16(&title);
        let _ = unsafe { SetWindowTextW(hwnd, PCWSTR(title.as_ptr())) };
    }

    fn accept_decoded(&mut self, decoded: reading::Decoded) {
        self.source = decoded.text;
        self.decoded_encoding = decoded.encoding;
        self.encoding_guessed = decoded.guessed;
    }

    /// Set the page in another face pairing.
    ///
    /// A face that is not installed is refused rather than answered by whatever
    /// DirectWrite would substitute in its place: a reader who asks for Candara on a
    /// machine without it should go on reading the face they had, not a stranger chosen
    /// for them.
    fn set_face(&mut self, face: usize, hwnd: HWND) {
        let Some(f) = TextFace::ALL.get(face) else { return };
        if face == self.theme.face || !face_drawable(&self.font, f) {
            return;
        }
        self.theme.set_face(face);
        self.remember();
        self.relayout_in_place(hwnd);
    }

    /// Set the page to another measure, and keep the reader's place through the reflow.
    fn set_measure(&mut self, measure: usize, hwnd: HWND) {
        if Measure::ALL.get(measure).is_none() || measure == self.theme.measure {
            return;
        }
        self.theme.set_measure(measure);
        self.remember();
        self.relayout_in_place(hwnd);
    }

    fn set_wide_table_factor(&mut self, factor: f32, hwnd: HWND) {
        let next = factor.clamp(0.0, 1.0);
        if (self.theme.wide_table_factor - next).abs() < 0.001 {
            return;
        }
        self.theme.set_wide_table_factor(next);
        self.relayout_in_place(hwnd);
    }

    /// Reflow the page for a change to its shape, without sending the reader somewhere
    /// else to look for the paragraph they were in.
    ///
    /// A measure change has no scale factor to carry the offset through with, the way
    /// [`View::zoom_to`] has, because the reflow is not uniform: a paragraph that was
    /// six lines is now eight. So the place is remembered as what the reader was looking
    /// at rather than as a distance, and found again where the new layout put it.
    fn relayout_in_place(&mut self, hwnd: HWND) {
        let k = scale_of(self.dpi);
        let anchor = anchor_at(&self.sel_index, self.scroll, k);
        self.relayout();
        if let Some(a) = anchor {
            if let Some(scroll) = scroll_for_anchor(&self.sel_index, a, k) {
                self.scroll = scroll;
            }
        }
        self.clamp_scroll();
        let _ = unsafe { InvalidateRect(Some(hwnd), None, false) };
    }

    /// Stand the reader where they were standing when this page was last open.
    ///
    /// The place is a character index rather than an offset, so a window that comes up at
    /// another size -- or in another face, because the remembered one had left the machine
    /// -- finds the same stretch of prose rather than the same number of points down a page
    /// that is no longer that long. A place the new layout has no line for leaves the top
    /// alone: a document that has shrunk since the reader left it is not theirs any more.
    fn restore_to(&mut self, anchor: usize) {
        let k = scale_of(self.dpi);
        if let Some(scroll) = scroll_for_anchor(&self.sel_index, anchor, k) {
            self.scroll = scroll;
        }
        self.clamp_scroll();
    }

    /// Write down where the reader is standing, for the next window on this page.
    ///
    /// At the moment of going rather than of choosing, because a reader who scrolls and
    /// then closes has chosen nothing in between. The document goes with the place: a
    /// number left over from another page would be a jump into a paragraph nobody is in.
    fn remember_reading(&self) {
        let Some(path) = self.path.as_deref() else { return };
        let anchor = anchor_at(&self.sel_index, self.scroll, scale_of(self.dpi));
        crate::settings::record_reading(path, anchor.unwrap_or(0));
    }

    /// Put what the reader has marked on the clipboard. A copy with nothing marked leaves
    /// the clipboard alone.
    fn copy_selection(&self, hwnd: HWND) {
        let Some(s) = self.selection else { return };
        let ordered = s.ordered();
        let all = ordered.from == (Caret { line: 0, ch: 0 }) && self.sel_index.last().is_some_and(|l|
            ordered.to == (Caret { line: self.sel_index.len() - 1, ch: l.chars.len() }));
        let text = if self.source_view && all { self.source.clone() }
            else { selection_text(&self.sel_index, s) };
        if let Err(e) = clipboard::copy_text(hwnd, &text) {
            eprintln!("clipboard: {e}");
        }
    }

    /// A relayout can leave the offset past the end of a document that has just grown.
    fn clamp_scroll(&mut self) {
        let view_h = self.client_h / scale_of(self.dpi);
        let max = (self.content_h + self.theme.base - view_h).max(0.0);
        self.scroll = self.scroll.clamp(0.0, max);
    }

    /// Change the reading size, and keep the reader's place through it.
    ///
    /// The scroll offset is a distance in points, and the points it counted have just
    /// changed size: scaling it by the same ratio leaves the same line of text under
    /// the top edge. Leaving it is a jump to another paragraph, which is the one thing
    /// a reader pressing `+` should never get.
    fn zoom_to(&mut self, zoom: Zoom, hwnd: HWND) {
        let before = self.theme.base;
        self.theme.set_zoom(zoom);
        if (self.theme.base - before).abs() < 0.001 {
            // Already at an end of the ladder: nothing moved, so nothing repaints.
            return;
        }
        self.scroll *= self.theme.base / before;
        self.remember();
        self.relayout();
        self.clamp_scroll();
        let _ = unsafe { InvalidateRect(Some(hwnd), None, false) };
    }

    /// Paint in one palette or the other. Cheap when it is already the right one, since
    /// the appearance poll calls this on every tick.
    fn set_dark(&mut self, dark: bool, hwnd: HWND) {
        if dark == self.palette.dark {
            return;
        }
        self.palette = Palette::of(dark);
        // The brushes hold the old inks, keyed by role rather than by palette, so they
        // have to go; the layout is ink-independent and needs no work.
        self.brushes.clear();
        self.sel_brush = None;
        self.hit_brush = None;
        self.focus_brush = None;
        // The find box is USER32's to erase, and it will ask this window what colour to
        // use -- but only if it is asked to repaint, and only with the brush it is given
        // now rather than the one it was given for the last palette.
        if let Some(b) = self.edit_brush.take() {
            unsafe { let _ = DeleteObject(HGDIOBJ(b.0)); }
        }
        if let Some(e) = self.edit {
            unsafe { let _ = InvalidateRect(Some(e), None, true); }
        }
        unsafe { self.apply_dark_titlebar(hwnd) };
    }

    fn relayout(&mut self) {
        // Every field handed in is borrowed for its own reason: the decoder and the
        // document's directory for figures, the cache so a resize does not reset the
        // document's formulas.
        let mut objects =
            Objects::new(self.images.as_ref(), self.path.as_deref().and_then(|p| p.parent()), &mut self.math);
        let page = build_ops(
            &mut self.font,
            &self.theme,
            &self.doc,
            self.client_w,
            self.dpi,
            &mut objects,
            self.hyphenator.as_ref(),
        );
        // A relayout moves the notes, so a jump still held down from before would now
        // point at a paragraph rather than at a footnote.
        self.pressed = None;
        self.content_h = page.height;
        self.ops = page.ops;
        self.hotspots = page.hotspots;
        self.wide_regions = page.wide_regions;
        self.note_tops = page.note_tops;
        self.anchor_tops = page.anchor_tops;
        // A selection is a pair of places in the old wrapping. Lines have moved, so
        // the words they named are elsewhere, and holding on to the range would show
        // the reader ink they never dragged over.
        self.selection = None;
        self.press_caret = None;
        self.caret = None;
        self.sel_index = page.sel;
        // The hits are pairs of places in lines that have just been broken apart and
        // joined up again, so the query has to be asked of the new page. Nothing is
        // scrolled to: the reader did not ask for a reflow, and the least it can cost
        // them is the paragraph they were already looking at.
        if let Some(query) = self.find.as_ref().map(|f| f.query.clone()) {
            self.apply_find(&query, false);
        }
    }

    /// Put the bar on the window, or bring back the one already there with the query it
    /// still holds.
    unsafe fn open_find(&mut self, hwnd: HWND) {
        if self.edit.is_none() {
            self.create_edit(hwnd);
        }
        let Some(edit) = self.edit else { return };
        let _ = ShowWindow(edit, SW_SHOW);
        self.find = Some(Find::default());
        self.layout_find();
        let query = self.edit_text();
        self.apply_find(&query, false);
        let _ = SetFocus(Some(edit));
        let _ = InvalidateRect(Some(hwnd), None, false);
    }

    /// Make the box. Once, and its window kept: one that is destroyed and remade has lost
    /// what the reader typed into it, which is the only thing worth keeping there.
    unsafe fn create_edit(&mut self, parent: HWND) {
        let Ok(module) = GetModuleHandleW(None) else { return };
        let style = WS_CHILD | WS_VISIBLE | WINDOW_STYLE(ES_AUTOHSCROLL as u32) | WS_BORDER;
        let Ok(edit) = CreateWindowExW(
            Default::default(),
            w!("EDIT"),
            w!(""),
            style,
            0,
            0,
            10,
            10,
            Some(parent),
            Some(HMENU(FIND_EDIT_ID as *mut core::ffi::c_void)),
            Some(module.into()),
            None,
        ) else {
            return;
        };
        // The words shown while the box is empty, drawn and erased by the control itself,
        // and shown even while the box has the keyboard: this window has nothing else
        // saying what the box is for.
        let cue = utf16("Find in document");
        let _ = SendMessageW(
            edit,
            EM_SETCUEBANNER,
            Some(WPARAM(1)),
            Some(LPARAM(cue.as_ptr() as isize)),
        );
        let previous = SetWindowLongPtrW(edit, GWLP_WNDPROC, edit_proc as *const () as isize);
        EDIT_PROC.store(previous as usize, Ordering::Relaxed);
        self.edit = Some(edit);
    }

    /// Where the box belongs now that the window has changed: moved, and its letters made
    /// over if the monitor it is on has.
    unsafe fn layout_find(&mut self) {
        self.position_edit();
        self.shape_find_label();
    }

    unsafe fn position_edit(&mut self) {
        let Some(edit) = self.edit else { return };
        self.ensure_edit_font();
        if let Some(font) = self.edit_font {
            let _ = SendMessageW(edit, WM_SETFONT, Some(WPARAM(font.0 as usize)), Some(LPARAM(1)));
        }
        let (x, y, w, h) = find_edit(self.client_w, self.dpi);
        let _ = MoveWindow(edit, x, y, w, h, true);
    }

    /// The box's letters, made at the size this window is drawn at.
    ///
    /// Set by hand rather than left to the control, whose default is a fixed bitmap font
    /// sized for the DPI the desktop started on: on a second monitor that is neither the
    /// right face nor the right height, and a reader finds it out by typing.
    unsafe fn ensure_edit_font(&mut self) {
        if self.edit_font.is_some() {
            return;
        }
        let face = utf16("Segoe UI");
        // A negative height is the character height rather than the cell's, which is the
        // only way to ask for a size that matches the text beside it. Weight 400 is
        // regular; the character set is left to the system, so a reader whose locale is
        // not Latin gets a face that can hold their own words.
        let height = -((self.theme.base * 0.9 * scale_of(self.dpi)).round() as i32);
        let font = CreateFontW(
            height,
            0,
            0,
            0,
            400,
            0,
            0,
            0,
            DEFAULT_CHARSET,
            OUT_DEFAULT_PRECIS,
            CLIP_DEFAULT_PRECIS,
            DEFAULT_QUALITY,
            0,
            PCWSTR(face.as_ptr()),
        );
        if !font.0.is_null() {
            self.edit_font = Some(font);
        }
    }

    unsafe fn drop_edit_font(&mut self) {
        if let Some(font) = self.edit_font.take() {
            let _ = DeleteObject(HGDIOBJ(font.0));
        }
    }

    /// What is in the box.
    unsafe fn edit_text(&self) -> String {
        let Some(edit) = self.edit else { return String::new() };
        let len = SendMessageW(edit, WM_GETTEXTLENGTH, None, None).0.max(0) as usize;
        let mut buf = vec![0u16; len + 1];
        let n = GetWindowTextW(edit, &mut buf).max(0) as usize;
        buf.truncate(n.min(len));
        String::from_utf16_lossy(&buf)
    }

    /// Ask the page where the query's characters are, and mark every place it found them.
    fn apply_find(&mut self, query: &str, scroll: bool) {
        if query.is_empty() {
            self.find = Some(Find::default());
            self.find_label.clear();
            return;
        }
        let needle = Needle::of(&self.sel_index);
        let marks: Vec<Selection> =
            needle.hits(query).iter().filter_map(|h| needle.span(h)).collect();
        // The first hit at or below the top edge of the window: a reader who has typed a
        // word wants the page's answer to start where they were looking, not at the first
        // line of the document three screens above.
        let top = self.top_line();
        let focus = marks.iter().position(|m| m.from.line >= top).unwrap_or(0);
        if scroll {
            if let Some(m) = marks.get(focus) {
                self.scroll_to_caret(m.from);
            }
        }
        self.find = Some(Find { query: query.to_string(), marks, focus });
        self.shape_find_label();
    }

    /// The line the window is looking at, which is where a search begins.
    fn top_line(&self) -> usize {
        let top = scroll_dip(self.scroll, self.dpi);
        self.sel_index.iter().position(|l| l.y + l.h > top).unwrap_or(0)
    }

    /// `Enter`, and `Shift`+`Enter`: walk the hits the search has already found, bringing
    /// each one into the window as the reader comes to it.
    fn step_find(&mut self, back: bool, hwnd: HWND) {
        let Some(f) = self.find.as_ref() else { return };
        let n = f.marks.len();
        if n == 0 {
            return;
        }
        let focus = if back { (f.focus + n - 1) % n } else { (f.focus + 1) % n };
        let Some(m) = f.marks.get(focus) else { return };
        let at = m.from;
        if let Some(f) = self.find.as_mut() {
            f.focus = focus;
        }
        self.scroll_to_caret(at);
        self.shape_find_label();
        let _ = unsafe { InvalidateRect(Some(hwnd), None, false) };
    }

    /// Take the bar away, and the keyboard with it. The page keeps its last place: a
    /// reader who has finished looking is going on reading what they found.
    unsafe fn close_find(&mut self, hwnd: HWND) {
        self.find = None;
        self.find_label.clear();
        if let Some(edit) = self.edit {
            let _ = ShowWindow(edit, SW_HIDE);
        }
        let _ = SetFocus(Some(hwnd));
        let _ = InvalidateRect(Some(hwnd), None, false);
    }

    /// How many there are, and which one this is, set in the space kept at the bar's right.
    fn shape_find_label(&mut self) {
        self.find_label.clear();
        let Some(f) = self.find.as_ref() else { return };
        if f.query.is_empty() {
            return;
        }
        let text = find_count(f.focus, f.marks.len());
        let k = scale_of(self.dpi);
        let size = self.theme.base * 0.85;
        let req = FaceRequest {
            family: self.theme.fonts.family(Role::Body, false).to_string(),
            cjk_family: self.theme.fonts.family(Role::Body, true).to_string(),
            japanese_family: self.theme.fonts.japanese[Role::Body as usize].clone(),
            korean_family: self.theme.fonts.korean[Role::Body as usize].clone(),
            cjk_italic: Some(false),
            fallback: self.theme.fonts.fallback.clone(),
            weight: 400,
            italic: false,
        };
        let runs = self.font.shape_runs(&text, 0..text.len(), &req, size, 0.02);
        let width: f32 = runs.iter().map(|r| r.width() * k).sum();
        let (px, py, pw, ph) = find_panel(self.client_w);
        // End against the bar's own right edge, so a count that grows from two figures to
        // five does not walk under the box.
        let mut at = px + pw - FIND_PAD - width;
        let baseline = py + ph / 2.0 + size * k * 0.35;
        let mut label = Vec::new();
        for r in &runs {
            if let Some(mut p) = paint_run(&self.font, r, 0.0, 0.0, k, ColorRole::Muted, Some(&text)) {
                p.x = at;
                p.baseline = baseline;
                at += r.width() * k;
                label.push(p);
            }
        }
        self.find_label = label;
    }
}

/// Inputs that are the same for every block in one layout pass.
struct Ctx<'a> {
    theme: &'a Theme,
    styles: &'a [AppStyle],
    /// Typeset formulas, by cache index. Layout only reads this; interning wrote it.
    math: &'a MathStore,
    /// Which footnote a citation's label names, which is how a raised number becomes a
    /// jump to a position on the page.
    notes: &'a HashMap<String, usize>,
    /// Which heading a fragment's slug names, indexed the same way -- the bridge from
    /// the address a table of contents writes to the line it is asking for.
    anchors: &'a HashMap<String, usize>,
    /// Directory the open document's own relative links resolve against, which is the
    /// directory its files are in. `None` for a document with no file behind it.
    base: Option<&'a std::path::Path>,
    /// Points to device independent pixels.
    k: f32,
    /// The extra room a wide table may borrow from the reading margins.
    wide_limit: Pt,
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
    /// Where words in this block may split, and the advance of the hyphen glyph at
    /// this block's body size.
    hyphenation: Hyphenation<'a>,
    /// The block's body size and the leading to set its lines at, both chosen by the
    /// caller rather than read off [`Blk::b`]'s kind.
    ///
    /// A footnote's prose is an ordinary Markdown paragraph that has to sit smaller
    /// and tighter than the page's, and the document model cannot say so itself:
    /// Markdown has no footnote block kind, only a definition the reader collects.
    size: Pt,
    leading: Leading,
    /// Width of the marker leading this block, which is how far every line under it
    /// stands back from the measure so the marker can hang in that space. Zero for a
    /// block with no marker, which is every block but a list item and a footnote.
    hang: Pt,
    /// The block's clickable ranges, already shifted for the marker like [`Blk::spans`]
    /// is, so both index the same text.
    actions: &'a [Action],
}

/// What a layout pass writes into, bundled so that adding a third thing the page
/// carries -- the character index, after the ink and the targets -- costs one argument
/// to every layout function rather than two.
struct Out<'a> {
    ops: &'a mut Vec<Op>,
    hots: &'a mut Vec<Hot>,
    sel: &'a mut Vec<SelLine>,
    hyphens: &'a mut HyphenCount,
    wide: &'a mut Vec<WideRegion>,
    table_headers: &'a mut Vec<TableHeaderFragment>,
    table_spans: &'a mut Vec<TableSpan>,
    note_spans: &'a mut Vec<NoteSpan>,
    math_texts: &'a mut Vec<MathTextFragment>,
}

/// How many lines the solver broke on a discretionary hyphen, and how many hyphen marks
/// the painter drew for them.
///
/// Counted apart because they are decided apart -- one in the break plan, one in the
/// display list -- and a page where the first is 17 and the second is 0 is a page of
/// words split apart with nothing to show where they were cut. That is the whole
/// failure mode of a glyph that is measured and laid but never drawn, and it is
/// invisible to every other number on the page because both halves look reasonable.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HyphenCount {
    pub breaks: usize,
    pub marks: usize,
}

/// A block readied for layout: its text with the marker already in front of it, and
/// every inline style resolved to an id in the table.
struct Prepared {
    text: String,
    spans: Vec<StyleSpan>,
    /// The id of the block's own prose style, which is also what a marker is set in.
    base: usize,
    /// The marker's own advance, which the lines below it give up. Measured rather than
    /// assumed from ems, because a bullet and the number `10` are not the same width and
    /// body text that lines up with neither is not hung.
    hang: Pt,
    table: Option<PreparedTable>,
    /// The block's clickable ranges, with the marker's length added to both ends so the
    /// same offsets index [`Prepared::text`] as for [`Prepared::spans`].
    actions: Vec<Action>,
}

/// A table cell with its styles already resolved to ids, so column sizing and
/// word wrapping measure exactly what prose measures.
pub struct PreparedCell {
    pub sources: Vec<rubrica_doc::SourceSpan>,
    pub text: String,
    pub spans: Vec<StyleSpan>,
    pub align: Align,
    /// What a click on this cell's ink would do. A cell is laid out by the grid, so
    /// its targets ride along with the cell rather than with the block's prose.
    pub actions: Vec<Action>,
    pub objects: Vec<rubrica_doc::ObjectSpan>,
}

pub struct PreparedTable {
    pub head: Vec<PreparedCell>,
    pub rows: Vec<Vec<PreparedCell>>,
}

/// Horizontal padding inside a table cell, in ems of the body size.
const CELL_PAD_EM: Pt = 0.6;
/// The narrowest a column may be squeezed to before it is left to overflow.
const CELL_MIN_EM: Pt = 3.0;

/// A shaped run turned into something the painter can position: scaled to device
/// independent pixels, seated `x` from the page edge and `dy` below the baseline of
/// whichever line carries it.
///
/// Prose uses `dy = 0`. A formula's pieces each bring their own offset, which is why
/// the drop is a property of the run rather than something the line loop assumes.
fn fit_punctuation_runs(runs: &mut [GlyphRun], width: Pt) {
    let natural = runs.iter().map(GlyphRun::width).sum::<Pt>();
    if natural <= 0.0 || width <= 0.0 {
        return;
    }
    let scale = width / natural;
    for run in runs {
        for advance in &mut run.advances {
            *advance *= scale;
        }
    }
}

fn unicode_by_glyph(source: &str, clusters: &[u16], glyph_count: usize) -> Vec<Option<String>> {
    let mut out = vec![None; glyph_count];
    let mut unit = 0usize;
    for c in source.chars() {
        let end = unit + c.len_utf16();
        let mut seen = Vec::new();
        for glyph in clusters.iter().take(end).skip(unit).copied() {
            let index = glyph as usize;
            if index < out.len() && !seen.contains(&index) {
                seen.push(index);
                out[index].get_or_insert_with(String::new).push(c);
            }
        }
        unit = end;
    }
    out
}

fn paint_run(
    font: &FontEngine,
    r: &GlyphRun,
    x: Pt,
    dy: Pt,
    k: f32,
    color: ColorRole,
    source: Option<&str>,
) -> Option<PaintRun> {
    let source = source.and_then(|text| {
        text.get(r.text.clone())
            .filter(|slice| !slice.is_empty())
            .map(str::to_owned)
    });
    Some(PaintRun {
        bidi_level: r.bidi_level,
        family: font.face_family(r.face),
        face: font.font_face(r.face)?,
        face_index: r.face,
        em: r.size * k,
        glyphs: r.glyphs.clone(),
        advances: r.advances.iter().map(|a| a * k).collect(),
        offsets: r
            .offsets
            .iter()
            .map(|offset| DWRITE_GLYPH_OFFSET {
                advanceOffset: offset.advanceOffset * k,
                ascenderOffset: offset.ascenderOffset * k,
            })
            .collect(),
        source,
        clusters: r.clusters.clone(),
        x: x * k,
        baseline: 0.0,
        dy,
        color,
    })
}

/// Seat a line's runs on its baseline, in device pixels: each run's own lift comes
/// off the line it belongs to, so a raised mark leaves the baseline of its word
/// rather than standing on it.
///
/// A prose line and a table cell seat through here because they would otherwise
/// agree by hand-copying, and the cell has already been caught drawing its citations
/// on the baseline -- the ascent grew to make room for the lift, and the lift itself
/// never arrived, so the mark read as a stray digit in the middle of the line.
fn seat(runs: &mut [PaintRun], baseline: Pt, k: f32) {
    for run in runs.iter_mut() {
        run.baseline = (baseline + run.dy) * k;
    }
}

/// A strike rule still being grown, with the style it belongs to.
struct Rule {
    style: StyleId,
    x: Pt,
    top: Pt,
    w: Pt,
    h: Pt,
    color: ColorRole,
}

/// Lay down the rule grown so far, if there is one.
fn end_rule(rule: &mut Option<Rule>, bars: &mut Vec<(Pt, Pt, Pt, Pt, ColorRole)>) {
    if let Some(r) = rule.take() {
        bars.push((r.x, r.top, r.w, r.h, r.color));
    }
}

/// Start or extend the strike rule being accumulated across a line.
///
/// Extended by style, not by geometry: every ideograph and every word is its own
/// node, while the glue between two of them is part of the same struck span and has
/// to be covered too. A different style ends the run -- which is what separates an
/// intervening unstruck word, and what gives a bilingual span one rule per face at
/// that face's own height instead of one averaged over both.
fn merge_rule(
    pending: &mut Option<Rule>,
    bars: &mut Vec<(Pt, Pt, Pt, Pt, ColorRole)>,
    next: Rule,
) {
    if let Some(r) = pending.as_mut().filter(|r| r.style == next.style) {
        if (r.top - next.top).abs() < 0.01 && (r.h - next.h).abs() < 0.01 {
            r.w += next.w;
            return;
        }
    }
    end_rule(pending, bars);
    *pending = Some(next);
}

/// Extend this line's clickable rectangles by the node that was just laid out, whose
/// ink runs from `x0` to `x1`. A node belongs to a target whenever their text overlaps,
/// which is also how a hyphen or an ellipsis in the middle of a link stays clickable.
fn merge_hot(hit: &mut [Vec<(Pt, Pt)>], actions: &[Action], node: &std::ops::Range<usize>, x0: Pt, x1: Pt) {
    if x1 <= x0 {
        return;
    }
    for (h, a) in hit.iter_mut().zip(actions) {
        if a.range.start < node.end && node.start < a.range.end {
            if let Some(last) = h.last_mut().filter(|last| x0 <= last.1 + 0.01 && x1 >= last.0) {
                last.0 = last.0.min(x0);
                last.1 = last.1.max(x1);
            } else {
                h.push((x0, x1));
            }
        }
    }
}

/// Turn this line's target rectangles into hotspots, preserving disjoint bidi spans.
/// A target whose ink is unopenable -- a relative path, a
/// citation of a label no note answers -- is not marked at all, so a pointer that
/// stays an arrow is the reader's answer to "nowhere".
fn emit_hots(
    hots: &mut Vec<Hot>,
    actions: &[Action],
    hit: &[Vec<(Pt, Pt)>],
    ctx: &Ctx,
    top: Pt,
    line_h: Pt,
) {
    let k = ctx.k;
    for (i, h) in hit.iter().enumerate() {
        let Some(kind) = hot_kind(&actions[i].kind, ctx) else { continue };
        for &(x0, x1) in h {
            hots.push(Hot { x: x0 * k, y: top * k, w: (x1 - x0) * k, h: line_h * k, kind: kind.clone() });
        }
    }
}

/// The jump an action offers, or `None` when its destination is not on the page or
/// not openable.
fn hot_kind(kind: &ActionKind, ctx: &Ctx) -> Option<HotKind> {
    match kind {
        ActionKind::Url(u) if openable(u) => Some(HotKind::Url(u.clone())),
        // A fragment is an address into this file, so it is answered here rather than
        // handed to the shell -- and only when the heading it names is on the page.
        // The reader may have written the heading's own words rather than its address,
        // so what they wrote goes through the same rule the heading was indexed by.
        ActionKind::Url(u) => match u.strip_prefix('#') {
            Some(fragment) => {
                ctx.anchors.get(&fragment_slug(fragment)).copied().map(HotKind::Heading)
            }
            // A path is a request for another file, and this one reads them: it becomes
            // a target only when the file is actually on disk beside the open document.
            None => document_link(ctx.base, u).map(|path| HotKind::Document(DocumentTarget {
                path,
                fragment: u.split_once('#').map(|(_, f)| f.to_string()).filter(|f| !f.is_empty()),
            })),
        },
        ActionKind::Cite(label) => ctx.notes.get(label.as_str()).copied().map(HotKind::Cite),
    }
}

/// The document a relative link asks for, or `None` when its target is not a file this
/// reader can take over: an address in another protocol, a name with no Markdown in it,
/// or a file that is not there.
///
/// The `?` and `#` that can follow a destination are stripped before the name is
/// considered, since they address a query or a place rather than part of the filename --
/// and the `%` escapes inside what is left are what let an author name a file whose real
/// name has a space in it.
pub(crate) fn document_link(base: Option<&std::path::Path>, url: &str) -> Option<PathBuf> {
    let target = url.trim().split(['?', '#']).next().filter(|t| !t.is_empty())?;
    let path = std::path::Path::new(target);
    // A colon before any separator is a scheme rather than a path. An absolute path is
    // exempt: on this platform `C:/notes/x.md` puts its colon in exactly that place, and
    // it does name a file.
    if target.contains(':') && !path.is_absolute() {
        return None;
    }
    let full = ImageStore::resolve(base, target);
    if !matches!(
        full.extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase()).as_deref(),
        Some("md") | Some("markdown") | Some("txt")
    ) {
        return None;
    }
    full.is_file().then_some(full)
}

/// Where each character of a shaped run begins, in points from the left of the page.
///
/// DirectWrite maps UTF-16 code units to glyph indices. Convert those indices into
/// advances in the run's direction, then map them back to source UTF-8 boundaries.
fn mark_run(text: &str, run: &GlyphRun, base: Pt, out: &mut Vec<(usize, Pt, Pt)>) {
    // DirectWrite's cluster map is UTF-16 text -> glyph index, not its inverse.
    // Preserve all code units even when a ligature uses fewer glyphs than letters.
    let mut edges = vec![0.0];
    for width in &run.advances {
        edges.push(edges.last().copied().unwrap() + width);
    }
    let mut boundaries: Vec<_> = run.clusters.iter().map(|c| *c as usize).collect();
    boundaries.push(run.glyphs.len());
    boundaries.sort_unstable();
    boundaries.dedup();
    let mut unit = 0;
    let mut chars = Vec::new();
    for (byte, c) in text[run.text.clone()].char_indices() {
        let cluster = run.clusters.get(unit).copied().unwrap_or(0) as usize;
        chars.push((run.text.start + byte, cluster));
        unit += c.len_utf16();
    }
    let mut at = 0;
    while at < chars.len() {
        let cluster = chars[at].1.min(run.glyphs.len());
        let mut end = at + 1;
        while end < chars.len() && chars[end].1 == cluster { end += 1; }
        let next = boundaries.iter().copied().find(|b| *b > cluster).unwrap_or(run.glyphs.len());
        let a = edges[cluster];
        let b = edges[next];
        let position = |offset: Pt| {
            if run.bidi_level % 2 == 1 { base + run.width() - offset } else { base + offset }
        };
        for (i, &(byte, _)) in chars[at..end].iter().enumerate() {
            let start = a + (b - a) * i as Pt / (end - at) as Pt;
            let finish = a + (b - a) * (i + 1) as Pt / (end - at) as Pt;
            out.push((byte, position(start), position(finish)));
        }
        at = end;
    }
}

/// Record elastic whitespace at its actual visual position, including spaces
/// between runs with different directions. Wrapped-away whitespace stays zero width.
fn mark_glue(text: &str, slot: &rubrica_type::Placed, left: Pt,
    segs: &mut Vec<(std::ops::Range<usize>, Pt, Pt)>, marks: &mut Vec<(usize, Pt, Pt)>) {
    let Some((start, end)) = slot.source.filter(|(a, b)| b > a) else { return };
    let count = text[start..end].chars().count() as Pt;
    let x = left + slot.x;
    let pos = |i: usize| {
        let offset = slot.w * i as Pt / count;
        x + if slot.bidi_level % 2 == 1 { slot.w - offset } else { offset }
    };
    for (i, (byte, _)) in text[start..end].char_indices().enumerate() {
        marks.push((start + byte, pos(i), pos(i + 1)));
    }
    segs.push((start..end, x, x + slot.w));
}

/// Index one drawn line character by character, from the source rather than from the
/// pieces it happened to be drawn in.
///
/// The gaps between the segments are the spaces and breaks a reader cannot see -- a
/// justified line's word space, the newline a hard break turned into one -- and a copy
/// has to keep them, so they get characters of their own spread over whatever width the
/// layout left at that edge of the line.
///
/// `consumed` is how far the block's text has reached so far, and carries across a
/// block's lines: a paragraph's wrapping space belongs to whichever line comes next.
/// Where one drawn line sits, and what separates it from the line before: the four
/// numbers `mark_line` otherwise takes as four more arguments.
struct Band {
    top: Pt,
    h: Pt,
    /// Points to device pixels.
    k: f32,
    join: Join,
}

fn mark_line(
    text: &str,
    band: Band,
    segs: &[(std::ops::Range<usize>, Pt, Pt)],
    marks: &[(usize, Pt, Pt)],
    consumed: &mut usize,
) -> Option<SelLine> {
    let Band { top, h, k, join } = band;
    // Placement is visual; copying, find, and document anchors stay logical.
    let mut segs = segs.to_vec();
    segs.sort_by_key(|s| s.0.start);
    let first = segs.first()?;
    let mut l = SelLine {
        source: None,
        y: top * k, h: h * k, join,
        chars: Vec::new(), copies: Vec::new(), xs: Vec::new(), ends: Vec::new(),
    };
    let edge = |byte: usize| marks.binary_search_by_key(&byte, |m| m.0).ok().map(|i| marks[i]);
    let mut at = edge(first.0.start).map_or(first.1, |m| m.1);
    for (range, x0, x1) in &segs {
        let start = edge(range.start).map_or(*x0, |m| m.1);
        if range.start > *consumed {
            let gap = &text[*consumed..range.start];
            let n = gap.chars().count().max(1) as Pt;
            // Between directional runs, use the adjacent visual edges, not the
            // potentially distant leading caret of the next logical character.
            let from = if at <= *x0 || at >= *x1 { at } else { start };
            let to = if from <= *x0 { *x0 } else if from >= *x1 { *x1 } else { from };
            for (i, c) in gap.chars().enumerate() {
                l.chars.push(c);
                l.xs.push((from + (to - from) * i as Pt / n) * k);
                l.ends.push((from + (to - from) * (i + 1) as Pt / n) * k);
            }
        }
        for (byte, c) in text[range.clone()].char_indices() {
            let (_, a, b) = edge(range.start + byte).unwrap_or((0, *x0, *x1));
            l.chars.push(c);
            l.xs.push(a * k);
            l.ends.push(b * k);
            at = b;
        }
        *consumed = range.end;
    }
    l.xs.push(at * k);
    (!l.chars.is_empty()).then_some(l)
}

/// Link object source to logical positions after shaping. Objects still occupy one
/// selectable slot; their source length never changes caret or bidi geometry.
fn copy_objects(line: &mut SelLine, text: &str, start: usize,
    objects: &[rubrica_doc::ObjectSpan], shift: usize)
{
    for object in objects {
        let at = object.range.start + shift;
        if at < start || at > text.len() { continue; }
        let ch = text[start..at].chars().count();
        if line.chars.get(ch) == Some(&'\u{fffc}') {
            line.copies.push((ch, object.kind.markdown()));
        }
    }
}

fn source_line(line: &mut SelLine, text: &str, start: usize, sources: &[rubrica_doc::SourceSpan], shift: usize) {
    let mut positions = text[start..].char_indices().take(line.chars.len()).filter_map(|(at, c)| {
        let byte = (start + at).checked_sub(shift)?;
        rubrica_doc::source_at(sources, byte).map(|source| source..source + c.len_utf8())
    });
    if let Some(first) = positions.next() {
        let end = positions.last().map_or(first.end, |last| last.end);
        line.source = Some(first.start..end.max(first.end));
    }
}

/// Typeset one block into the display list and return the new document y.
fn layout_block(
    font: &mut FontEngine,
    ctx: &Ctx<'_>,
    blk: &Blk<'_>,
    out: &mut Out<'_>,
    mut y: Pt,
) -> Pt {
    let Out { ops, hots, sel, hyphens, wide, table_headers, table_spans, note_spans, math_texts } = out;
    let Ctx { theme, styles, math, k, .. } = *ctx;
    let Blk { b, text, spans, base, left, column, hyphenation, size, hang, actions, .. } = *blk;
    let leading = &blk.leading;
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
        return layout_table(font, ctx, blk, t, &mut Out { ops, hots, sel, hyphens, wide, table_headers, table_spans, note_spans, math_texts }, y);
    }
    if text.trim().is_empty() {
        return y;
    }

    let mixed = text.chars().any(cjk_char);
    // A fence is a grid: its spaces are the face's own space wide, a run of them is as
    // wide as it is long, and none of it moves when the line is set. The prose recipe
    // -- a third of an em, elastic, collapsed -- sets `let x` and `let  x` the same
    // distance apart and leaves two code lines of equal length ending in different
    // columns, which is the promise a monospace face exists to keep.
    let grid = if b.kind == BlockKind::Code {
        let st = &styles[base];
        font.shape_runs(" ", 0..1, &st.face, st.size, st.tracking)
            .iter()
            .map(|r| r.width())
            .sum()
    } else {
        0.0
    };
    let mut spacing = if grid > 0.0 { Spacing::monospace(size, grid) } else { Spacing::for_size(size) };
    spacing.keep_korean_words = grid == 0.0 && theme.keep_korean_words;
    spacing.punctuation_compression = theme.punctuation_compression;
    let mut opts = BreakOptions::new(column);
    opts.ragged = b.ragged() || column < size * theme.ragged_below_em;
    // A heading that will not fit hangs, and that is the end of it: hyphenating a
    // heading is an error, and a reader widens the window. A line of code has nowhere
    // to hang *to* -- there is no horizontal scroll -- so the characters past the
    // measure are not merely off the edge but unreadable. Code therefore asks to break
    // rather than hang, which is what `tight_box` says, and gets a cut offered at
    // every character to do it with.
    opts.tight_box = b.kind == BlockKind::Code;
    opts.hanging_punctuation = theme.hanging_punctuation_em * size;
    opts.par_indent = if b.kind == BlockKind::Paragraph && hang == 0.0 {
        theme.first_line_indent_em * size
    } else { 0.0 };
    // Every line but the first stands back by the marker's own width, so the marker
    // hangs in the margin it clears instead of shoving the body along.
    opts.hang_indent = hang;

    let (para, plan) = typeset_hyphenated(
        text,
        &spacing,
        StyleId(base as u16),
        spans,
        &hyphenation,
        &opts,
        font,
    );
    if plan.lines.is_empty() {
        return y;
    }

    let bg_role = if b.kind == BlockKind::Code && b.lang.as_deref() != Some("markdown-source") {
        Some(ColorRole::Surface)
    } else { None };
    let panel_top = y;
    let mut panel_bottom = y;
    // How far into this block's text the selection index has reached, which is shared
    // by every line of it because the space between two of them is in neither.
    let mut consumed = 0usize;

    let bidi = rubrica_type::BidiInfo::new(text, None);
    for line in &plan.lines {
        let top = y;
        let placed = place_bidi(&para, line, &bidi);
        if line.hyphen.is_some_and(|h| para.node(h).advance > 0.0) {
            hyphens.breaks += 1;
        }
        // Where this line starts. A line under a hanging marker begins at the marker's
        // right edge, which is the space [`Blk::hang`] bought it; the first line begins
        // at the block's own left, where the marker sits.
        let line_left = left + if line.first { opts.par_indent } else { hang };
        // How far each clickable range reaches along this line, if it reaches at all.
        // Mixed-direction links may occupy several disjoint rectangles on one line.
        let mut hit: Vec<Vec<(Pt, Pt)>> = vec![Vec::new(); actions.len()];
        let mut runs: Vec<PaintRun> = Vec::new();
        // Bars of a formula, as x, top edge, width, thickness and ink, all still
        // relative to this line's baseline because the line has no position yet.
        let mut bars: Vec<(Pt, Pt, Pt, Pt, ColorRole)> = Vec::new();
        let mut line_math_texts: Vec<MathTextFragment> = Vec::new();
        let mut ascent = 0.0f32;
        let mut descent = 0.0f32;
        // This line's ink, in the order it was drawn: the source range each segment
        // paints and the two edges it sits between, plus the marks shaping left behind
        // for the characters inside them.
        let mut segs: Vec<(std::ops::Range<usize>, Pt, Pt)> = Vec::new();
        let mut marks: Vec<(usize, Pt, Pt)> = Vec::new();
        // One strike rule per line rather than per node: the layout core makes every
        // ideograph and every word its own node, so a struck Chinese phrase would
        // otherwise draw a separate op per character.
        let mut ruled: Option<Rule> = None;
        let mut line_wide: Option<(WideKind, Pt, Pt, Pt)> = None;
        for slot in placed {
            let Some(node_id) = slot.node else {
                mark_glue(text, &slot, line_left, &mut segs, &mut marks);
                if let Some((a, b)) = slot.source {
                    merge_hot(&mut hit, actions, &(a..b), line_left + slot.x, line_left + slot.x + slot.w);
                }
                continue;
            };
            let node = para.node(node_id);
            let st = &styles[node.style.0 as usize];
            // A node of any other style -- unstruck, or struck in another face --
            // closes the rule, so it never covers text the author did not mark.
            if ruled.as_ref().is_some_and(|r| r.style != node.style) {
                end_rule(&mut ruled, &mut bars);
            }
            if let Some(o) = st.object {
                // An object contributes its own box to the line and is drawn from
                // its source, not from the placeholder character's glyphs.
                ascent = ascent.max(o.ascent);
                descent = descent.max(o.descent);
                match &st.source {
                    Some(ObjectSource::Image(file)) => {
                        images_out.push((
                            file.clone(),
                            (line_left + slot.x) * k,
                            o.advance * k,
                            o.ascent,
                            o.descent,
                        ));
                        line_wide = Some((WideKind::Image(file.clone()), line_left + slot.x, o.advance, column.min(o.advance)));
                    }
                    Some(ObjectSource::Math(index)) => {
                        line_wide = Some((WideKind::Formula(*index), line_left + slot.x, o.advance, column.min(o.advance)));
                        // Every piece arrives at its own place inside the formula's
                        // box, so nothing here accumulates an advance.
                        if let Some(entry) = math.get(*index) {
                            for (r, dx, dy) in &entry.parts {
                                if let Some(p) = paint_run(font, r, line_left + slot.x + dx, *dy, k, st.color, None) {
                                    runs.push(p);
                                }
                            }
                            if let Some((first, _, _)) = entry.parts.first() {
                                line_math_texts.push(MathTextFragment {
                                    source: entry.source.clone(),
                                    x: line_left + slot.x,
                                    y: 0.0,
                                    w: o.advance,
                                    h: o.ascent + o.descent,
                                    face_index: first.face,
                                    em: first.size,
                                });
                            }
                            bars.extend(entry.rules.iter().map(|(x, top, w, h)| {
                                (line_left + slot.x + x, *top, *w, *h, st.color)
                            }));
                        }
                    }
                    None => {}
                }
                merge_hot(&mut hit, actions, &node.text, line_left + slot.x, line_left + slot.x + o.advance);
                segs.push((node.text.clone(), line_left + slot.x, line_left + slot.x + o.advance));
                continue;
            }
            let hyphen = node.kind == rubrica_type::paragraph::NodeKind::Hyphen;
            if hyphen && node.advance == 0.0 {
                // A code line cut because it would not fit: the solver charged nothing
                // for it, so there is no mark to draw -- and a hyphen in the middle of
                // an identifier would be a lie about its name.
                continue;
            }
            // The mark a discretionary break ends a line with is not in the source: the
            // solver charged the line its width and `place` gave it a slot, so the
            // painter is the only one who can supply the glyph -- and the only one who
            // should, since copying a hyphenated word must not hand back a hyphen its
            // author never wrote, and the selectable index must not point at a character
            // that is not there.
            let mut shaped = if hyphen {
                font.shape_runs(HYPHEN, HYPHEN_RANGE, &st.face, st.size, st.tracking)
            } else {
                font.shape_runs(text, node.text.clone(), &st.face, st.size, st.tracking)
            };
            if node.punctuation.is_some() {
                fit_punctuation_runs(&mut shaped, node.advance);
            }
            // A raised run hangs above this line's baseline, so its ink joins the
            // ascent and its own descent is measured from where it now stands: the
            // line grows upwards to make room for it, and a superscript never
            // pushes the baseline down into the line below.
            let dy = -st.raise;
            // One span can need several faces -- Latin and Han in one sentence, or a
            // run that falls back for a symbol -- and each takes up where the last
            // stopped, since only the whole span's width is what the line broke on.
            let mut at = line_left + slot.x;
            for r in shaped {
                if !hyphen {
                    mark_run(text, &r, at, &mut marks);
                }
                ascent = ascent.max(r.ascent - dy);
                descent = descent.max((r.descent + dy).max(0.0));
                if let Some(p) = paint_run(font, &r, at, dy, k, st.color, (!hyphen).then_some(text)) {
                    runs.push(p);
                    if hyphen {
                        hyphens.marks += 1;
                    }
                }
                if st.strike && r.width() > 0.0 {
                    // Read from the face carrying these glyphs, because that is where
                    // the height comes from. `dy` takes a lifted mark's rule up with
                    // it.
                    let (pos, weight) = font.strike_rule(r.face, r.size);
                    merge_rule(
                        &mut ruled,
                        &mut bars,
                        Rule {
                            style: node.style,
                            x: at,
                            top: dy - pos - weight * 0.5,
                            w: r.width(),
                            h: weight,
                            color: st.color,
                        },
                    );
                }
                at += r.width();
            }
            if hyphen {
                // Ink on the line, not text in it: no clickable range and no selectable
                // segment, because there is no character for either to name.
                continue;
            }
            merge_hot(&mut hit, actions, &node.text, line_left + slot.x, at);
            segs.push((node.text.clone(), line_left + slot.x, at));
        }
        end_rule(&mut ruled, &mut bars);
        let natural = ascent + descent;
        let line_h = (size * leading.for_mixed(mixed)).max(natural * 1.02);
        let baseline = y + (line_h - natural) * 0.5 + ascent;
        seat(&mut runs, baseline, k);
        for mut text in line_math_texts.drain(..) {
            text.y = baseline;
            math_texts.push(text);
        }
        ops.push(Op::Runs(runs));
        if let Some((kind, x, content_w, visible_w)) = line_wide {
            let index = wide.len();
            wide.push(WideRegion {
                kind,
                x: x * k,
                y: top * k,
                w: visible_w * k,
                h: line_h * k,
                content_w: content_w * k,
            });
            hots.push(Hot { x: x * k, y: top * k, w: visible_w * k, h: line_h * k, kind: HotKind::Wide(index) });
        }
        for (x, top, w, h, color) in bars {
            ops.push(Op::Rect { x: x * k, y: (baseline + top) * k, w: w * k, h: h * k, color });
        }
        for (file, x, w, a, d) in images_out.drain(..) {
            ops.push(Op::Image { path: file, x, y: (baseline - a) * k, w, h: (a + d) * k });
        }
        emit_hots(hots, actions, &hit, ctx, top, line_h);
        marks.sort_unstable_by_key(|m| m.0);
        // The first character the block has indexed is the first character of the
        // block, wherever on the page it ended up: that line takes the blank line the
        // block before it left off with.
        let join = if consumed == 0 { Join::Blank } else { Join::None };
        let start = consumed;
        if let Some(mut l) =
            mark_line(text, Band { top, h: line_h, k, join }, &segs, &marks, &mut consumed)
        {
            copy_objects(&mut l, text, start, &b.objects, text.len() - b.text.len());
            source_line(&mut l, text, start, &b.sources, text.len() - b.text.len());
            sel.push(l);
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
    /// Client-space height of one text page, in points: most of a window, so a reader
    /// keeps a little of the previous screen as a place to come back to.
    fn page_height(&self) -> Pt {
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
        self.load_document(std::path::Path::new(&path), hwnd);
        let _ = InvalidateRect(Some(hwnd), None, false);
    }

    /// The place the reader is standing: this page, at this offset down it.
    fn here(&self) -> Visit {
        Visit { path: self.path.clone(), scroll: self.scroll }
    }

    fn workspace_file(path: &std::path::Path) -> FileRef {
        let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        FileRef::new(canonical, 0)
    }

    fn switch_to_tab(&mut self, id: TabId, hwnd: HWND) -> bool {
        if !self.workspace.activate(id) {
            return false;
        }
        self.remember_reading();
        match self.workspace.tabs.active().document.clone() {
            DocumentRef::File(file) => self.show_document(&file.path, hwnd),
            DocumentRef::Sample => {
                self.set_page(crate::sample::DOCUMENT.to_string(), None, hwnd);
                true
            }
        }
    }

    fn switch_relative_tab(&mut self, delta: isize, hwnd: HWND) {
        let count = self.workspace.tabs.items().len();
        if count < 2 {
            return;
        }
        let current = self.workspace.tabs.active_index() as isize;
        let next = (current + delta).rem_euclid(count as isize) as usize;
        let id = self.workspace.tabs.items()[next].id;
        self.switch_to_tab(id, hwnd);
    }

    fn close_active_tab(&mut self, hwnd: HWND) {
        let id = self.workspace.tabs.active().id;
        self.remember_reading();
        if !self.workspace.close(id) {
            return;
        }
        let next = self.workspace.tabs.active().document.clone();
        match next {
            DocumentRef::File(file) => { self.show_document(&file.path, hwnd); }
            DocumentRef::Sample => { self.set_page(crate::sample::DOCUMENT.to_string(), None, hwnd); }
        }
        self.update_title(hwnd);
    }

    /// Replace the open document, resetting the view to its top, and leave the page being
    /// stood on behind where a step back can find it again.
    fn load_document(&mut self, path: &std::path::Path, hwnd: HWND) {
        let from = self.here();
        let file = Self::workspace_file(path);
        let already_open = self.workspace.tabs.find_file(&file).is_some();
        let same = self.path.as_deref() == Some(path);
        if already_open {
            self.workspace.open_file(file, TabKind::Pinned);
            if !self.show_document(path, hwnd) {
                return;
            }
        } else {
            // Read before creating a tab: a file that cannot be decoded must not become
            // a tab or a recent-document entry.
            if !self.show_document(path, hwnd) {
                return;
            }
            self.workspace.open_file(file, TabKind::Pinned);
        }
        self.remember_reading();
        if !same {
            self.history.leave(from);
        }
    }


    /// Read a file and make it the page, saying whether that worked.
    fn show_document(&mut self, path: &std::path::Path, hwnd: HWND) -> bool {
        let preferences = crate::settings::document(path);
        match reading::read(path, preferences.encoding) {
            Ok(decoded) => {
                self.decoded_encoding = decoded.encoding;
                self.encoding_guessed = decoded.guessed;
                self.set_page(decoded.text, Some(path.to_path_buf()), hwnd);
                true
            }
            Err(e) => {
                show_error(hwnd, &format!("Cannot open {}: {e}", path.display()));
                false
            }
        }
    }

    /// Read the page again if the file behind it has been written since the text on screen
    /// came out of it.
    ///
    /// The reader keeps their place, because that is what a save is: somebody has changed
    /// a paragraph, not the paragraph being looked at. `relayout_in_place` is the same
    /// arithmetic a resize or a change of face already uses, and the place it keeps is a
    /// character of the prose rather than a distance down the page, so a document that has
    /// grown around the reader carries them along with it. What cannot survive a re-wrap is
    /// a selection -- a pair of places in lines that no longer exist -- and a search's
    /// hits, which `relayout` re-asks of the new page for itself.
    fn reload_if_written(&mut self, hwnd: HWND) {
        // Held rather than borrowed: the page is replaced through the same `self` the path
        // is a part of, and a borrow of one field is a borrow of the whole struct as far as
        // the compiler is concerned. One path per three-quarters of a second is not a
        // copy worth an unsafe for.
        let Some(path) = self.path.clone() else { return };
        let now = stamp_of(&path);
        if !is_written(self.stamp, now) {
            return;
        }
        let Ok(decoded) = reading::read(&path, self.encoding) else {
            // Half way through being written, or locked by whatever is writing it. The
            // stamp is left as it was, so the next tick asks again rather than assuming
            // this file has already been read.
            return;
        };
        self.reload_text(decoded, hwnd);
        // The age polled *before* the read, not the one taken after it: if the file was
        // written again in between, the older stamp makes the next tick notice, where the
        // newer one would let a change go unread until the save after it.
        self.stamp = now;
    }

    /// Make this text the page, from its top, and name it on the title bar. The title is
    /// part of the page rather than of the call that got here, because this is the one
    /// place every route to a new document passes through -- and a reader who has just
    /// stepped back to a document of the same name as this one needs to see which is
    /// which.
    fn set_page(&mut self, source: String, path: Option<PathBuf>, hwnd: HWND) {
        self.remember_reading();
        self.remember_document();
        let preferences = path.as_deref().map(crate::settings::document).unwrap_or_default();
        self.line_break_override = preferences.line_breaks;
        self.plain_override = preferences.plain;
        self.source_view = preferences.source;
        self.text_options = preferences.text;
        self.encoding = preferences.encoding;
        self.source = source;
        self.path = path;
        if let Some(path) = self.path.as_deref() {
            self.workspace.open_file(Self::workspace_file(path), TabKind::Pinned);
        }
        self.tree = self.path.as_deref().and_then(Path::parent)
            .map(|root| tree::scan(root, 2))
            .unwrap_or_default();
        let profile = crate::profiles::selected(self.plain_override.unwrap_or_else(|| reading::is_plain(self.path.as_deref())));
        if profile != self.profile {
            crate::profiles::load(&profile).apply(&mut self.theme);
            self.profile = profile;
            self.math = MathStore::new();
        }
        self.chapter = 0;
        self.refresh_chapter_index();
        self.doc = self.parse_source();
        // Filed at the same moment as the text that came out of it, so that the next tick
        // of the poll compares the page on screen against the file it was read from rather
        // than against whatever the last page's file was.
        self.stamp = self.path.as_deref().and_then(stamp_of);
        self.scroll = 0.0;
        self.relayout();
        if let Some(anchor) = self.path.as_deref().and_then(crate::settings::document_anchor) {
            self.restore_to(anchor);
        }
        self.update_title(hwnd);
    }

    /// Step back, or forward again after a step back.
    fn go_back(&mut self, hwnd: HWND) {
        self.step(hwnd, true);
    }

    fn go_forward(&mut self, hwnd: HWND) {
        self.step(hwnd, false);
    }

    /// Take one step either way along the road the reader has travelled.
    ///
    /// The places that have gone off the disk since they were left are dropped on the way,
    /// so a deleted page costs the reader that page and not their step.
    fn step(&mut self, hwnd: HWND, back: bool) {
        self.history.prune(back);
        let here = self.here();
        let there = if back {
            self.history.back(here)
        } else {
            self.history.forward(here)
        };
        let Some(there) = there else {
            // Nowhere to go, which the menu dims and the key cannot prevent. A reader who
            // has been at one page since the window opened presses Alt+left on instinct,
            // and the answer is that nothing moves.
            return;
        };
        self.reopen(there, hwnd);
    }

    /// Stand on a remembered place: the page it was on, at the offset it was left at.
    fn reopen(&mut self, there: Visit, hwnd: HWND) {
        if let Some(path) = &there.path {
            // A step back to a heading on the page already open is the common kind of
            // jump, and re-reading that file would be the slowest way of doing nothing --
            // so the page is only loaded when it is a different one from the one being
            // stood on. Where it cannot be read at all, the place has come off a removed
            // drive since it was left, and the step is abandoned rather than faked.
            if self.path.as_deref() != Some(path.as_path()) && !self.show_document(path, hwnd) {
                return;
            }
        } else if self.path.is_some() {
            // The built-in sample, which is a page the reader can leave and be asked back
            // to but has no file to read again.
            self.set_page(crate::sample::DOCUMENT.to_string(), None, hwnd);
        }
        self.scroll = there.scroll;
        self.clamp_scroll();
        let _ = unsafe { InvalidateRect(Some(hwnd), None, false) };
    }

    /// The common dialog. Returns the chosen path, if the user did not cancel.
    unsafe fn prompt_for_file(&self, hwnd: HWND) -> Option<PathBuf> {
        self.prompt_file(hwnd, "Documents\0*.md;*.markdown;*.txt\0All files\0*.*\0\0")
    }

    unsafe fn prompt_file(&self, hwnd: HWND, filter: &str) -> Option<PathBuf> {
        let mut buf = [0u16; 1024];
        let filter = utf16(filter);
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
            // The display list is measured from the top of the document, so the whole of
            // it is lifted by the scroll here and nowhere else: a wheel tick costs one
            // subtraction at paint time rather than a relayout of the page, which is the
            // difference between scrolling at the frame rate and building the page again.
            let up = scroll_dip(self.scroll, self.dpi);
            // The document's two edges that the window is over.
            let top = up;
            let bottom = up + self.client_h;
            for op in self.ops.iter() {
                match op {
                    Op::Rect { x, y, w, h, color } => {
                        let dx = self.shift_at(*x, *y);
                        if y + h < top || *y > bottom {
                            continue;
                        }
                        let role = *color;
                        if let Some(brush) = self.brushes.get(&role).cloned() {
                            let r = D2D_RECT_F {
                                left: *x + dx,
                                top: *y - up,
                                right: x + w + dx,
                                bottom: y + h - up,
                            };
                            target.FillRectangle(&r, &brush);
                        }
                    }
                    Op::Line { x0, y0, x1, y1, thickness, color } => {
                        let dx = self.shift_at(*x0, *y0);
                        if (*y1).max(*y0) < top || (*y0).min(*y1) > bottom {
                            continue;
                        }
                        let role = *color;
                        if let Some(brush) = self.brushes.get(&role).cloned() {
                            target.DrawLine(
                                Vector2::new(*x0 + dx, y0 - up),
                                Vector2::new(*x1 + dx, y1 - up),
                                &brush,
                                *thickness,
                                None,
                            );
                        }
                    }
                    Op::Image { path, x, y, w, h } => {
                        let dx = self.shift_at(*x, *y);
                        if y + h < top || *y > bottom {
                            continue;
                        }
                        let (Some(t), Some(store)) = (self.target.clone(), self.images.as_ref()) else {
                            continue;
                        };
                        if let Some(bmp) = store.bitmap(&t, path) {
                            let r = D2D_RECT_F {
                                left: *x + dx,
                                top: y - up,
                                right: x + w + dx,
                                bottom: y + h - up,
                            };
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
                        let mut shifted = Vec::with_capacity(runs.len());
                        for run in runs {
                            let mut run = run.clone();
                            run.x += self.shift_at(run.x, run.baseline);
                            shifted.push(run);
                        }
                        self.draw_runs(&target, &shifted, up);
                    }
                }
            }
            self.draw_preview(&target);
            // Every place the search found what the reader typed. Under their own
            // selection and over the page's ink, because a hit is a suggestion and a drag
            // is a decision.
            if let Some(f) = self.find.as_ref() {
                for (i, m) in f.marks.iter().enumerate() {
                    let brush = if i == f.focus { &self.focus_brush } else { &self.hit_brush };
                    let Some(brush) = brush.clone() else { continue };
                    for (x, y, w, h) in selection_rects(&self.sel_index, *m) {
                        if y + h < top || y > bottom {
                            continue;
                        }
                        let r = D2D_RECT_F {
                            left: x + self.shift_at(x, y),
                            top: y - up,
                            right: x + w + self.shift_at(x, y),
                            bottom: y + h - up,
                        };
                        target.FillRectangle(&r, &brush);
                    }
                }
            }
            // The bands of a selection, over the ink and under nothing: their geometry
            // is the character index's, so a drag never costs a relayout, and drawing
            // them last is what keeps them visible inside a code panel.
            if let Some(s) = self.selection {
                if let Some(brush) = self.sel_brush.clone() {
                    for (x, y, w, h) in selection_rects(&self.sel_index, s) {
                        if y + h < top || y > bottom {
                            continue;
                        }
                        let dx = self.shift_at(x, y);
                        let r = D2D_RECT_F {
                            left: x + dx,
                            top: y - up,
                            right: x + w + dx,
                            bottom: y + h - up,
                        };
                        target.FillRectangle(&r, &brush);
                    }
                }
            }
            // The caret, when the reader has one and is not marking anything with it. An
            // arrow that moves an invisible bar is an arrow the reader cannot aim, and a
            // steady one rather than a blinking one because nothing here ticks at the
            // half-second a blink would need -- the page has no reason to repaint that
            // often, and a blink that misses its beats is worse than no blink.
            if self.selection.is_none() {
                if let (Some(c), Some(brush)) = (self.caret, self.brushes.get(&ColorRole::Text).cloned())
                {
                    if let Some((x, y, w, h)) = caret_rect(&self.sel_index, c) {
                        if y + h >= top && y <= bottom {
                            let dx = self.shift_at(x, y);
                            let r = D2D_RECT_F {
                                left: x + dx,
                                top: y - up,
                                right: x + w + dx,
                                bottom: y + h - up,
                            };
                            target.FillRectangle(&r, &brush);
                        }
                    }
                }
            }
            // The thumb is drawn from its geometry rather than as an op, because an op
            // would mean relaying out the document on every wheel tick. It is the only
            // sign the reader has that the edge of the window can be gripped.
            if let Some((tx, ty, tw, th)) = self.thumb_rect() {
                if let Some(brush) = self.brushes.get(&ColorRole::Muted).cloned() {
                    let r = D2D_RECT_F { left: tx, top: ty, right: tx + tw, bottom: ty + th };
                    target.FillRectangle(&r, &brush);
                }
            }
            // The bar's panel last of all, and over the page: it is pinned to the glass, so
            // what lies under it is whatever the reader has scrolled into place there, and
            // must not be read as part of the answer. The box on the panel's left is USER32's
            // to paint and sits over this ink by being a window.
            if self.find.is_some() {
                let (px, py, pw, ph) = find_panel(self.client_w);
                if let Some(brush) = self.brushes.get(&ColorRole::Surface).cloned() {
                    let r = D2D_RECT_F { left: px, top: py, right: px + pw, bottom: py + ph };
                    target.FillRectangle(&r, &brush);
                }
                self.draw_runs(&target, &self.find_label, 0.0);
            }
            let _ = target.EndDraw(None, None);
        }
    }

    unsafe fn draw_preview(&mut self, target: &ID2D1RenderTarget) {
        let Some(preview) = self.preview.clone() else { return };
        let panel = D2D_RECT_F {
            left: self.client_w * 0.08,
            top: self.client_h * 0.08,
            right: self.client_w * 0.92,
            bottom: self.client_h * 0.92,
        };
        if let Some(brush) = self.brushes.get(&ColorRole::Surface).cloned() {
            target.FillRectangle(&panel, &brush);
        }
        match preview {
            Preview::Image(path) => {
                let Some((w, h)) = self.images.as_ref().and_then(|s| s.natural_size(&path)) else { return };
                let k = scale_of(self.dpi);
                let avail_w = (panel.right - panel.left - 32.0) / k;
                let avail_h = (panel.bottom - panel.top - 32.0) / k;
                let scale = (avail_w / w.max(1.0)).min(avail_h / h.max(1.0));
                let dw = w * scale * k;
                let dh = h * scale * k;
                let r = D2D_RECT_F {
                    left: panel.left + (panel.right - panel.left - dw) * 0.5,
                    top: panel.top + (panel.bottom - panel.top - dh) * 0.5,
                    right: panel.left + (panel.right - panel.left + dw) * 0.5,
                    bottom: panel.top + (panel.bottom - panel.top + dh) * 0.5,
                };
                if let (Some(store), Some(bmp)) = (self.images.as_ref(), self.images.as_ref().and_then(|s| s.bitmap(target, &path))) {
                    let _ = store;
                    target.DrawBitmap(&bmp, Some(&r), 1.0, D2D1_BITMAP_INTERPOLATION_MODE_LINEAR, None);
                }
            }
            Preview::Formula(index) => {
                let Some(entry) = self.math.get(index) else { return };
                let k = scale_of(self.dpi);
                let scale = ((panel.right - panel.left - 32.0) / (entry.object.advance * k).max(1.0))
                    .min((panel.bottom - panel.top - 32.0) / ((entry.object.ascent + entry.object.descent) * k).max(1.0));
                let baseline = panel.top + (panel.bottom - panel.top + (entry.object.ascent - entry.object.descent) * scale * k) * 0.5;
                let left = panel.left + (panel.right - panel.left - entry.object.advance * scale * k) * 0.5;
                let mut runs = Vec::new();
                for (r, x, y) in &entry.parts {
                    if let Some(mut run) = paint_run(&self.font, r, (left / k) + *x * scale, *y * scale, k * scale, ColorRole::Text, None) {
                        run.baseline = baseline;
                        runs.push(run);
                    }
                }
                self.draw_runs(target, &runs, 0.0);
                if let Some(brush) = self.brushes.get(&ColorRole::Text).cloned() {
                    for (x, top, w, h) in &entry.rules {
                        let r = D2D_RECT_F {
                            left: left + *x * scale * k,
                            top: baseline + *top * scale * k,
                            right: left + (*x + *w) * scale * k,
                            bottom: baseline + (*top + *h) * scale * k,
                        };
                        target.FillRectangle(&r, &brush);
                    }
                }
            }
        }
    }

    /// One face's positioned glyphs, lifted by `up` into window coordinates.
    ///
    /// Shared by the page's own runs and by the find bar's count, which is the only other
    /// thing this window writes letters of. Both are measured in their own space against
    /// the same `up`: the page's from the top of the document, the bar's from the top of
    /// the window with nothing to take off.
    unsafe fn draw_runs(
        &self,
        target: &ID2D1RenderTarget,
        runs: &[PaintRun],
        up: Pt,
    ) {
        let bottom = up + self.client_h;
        for run in runs {
            if run.baseline < up || run.baseline > bottom + 40.0 {
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
                bidiLevel: run.bidi_level as u32,
            };
            target.DrawGlyphRun(
                Vector2::new(glyph_origin(run.x, run.advances.iter().sum(), run.bidi_level), run.baseline - up),
                &gr,
                &brush,
                DWRITE_MEASURING_MODE_NATURAL,
            );
            let _ = std::mem::ManuallyDrop::into_inner(gr.fontFace);
        }
    }
}

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
    ctx: &Ctx<'_>,
    blk: &Blk<'_>,
    t: &PreparedTable,
    out: &mut Out<'_>,
    mut y: Pt,
) -> Pt {
    let Out { ops, hots, sel, hyphens, wide, table_headers, table_spans, note_spans: _, math_texts } = out;
    let Ctx { theme, styles, k, math, wide_limit, .. } = *ctx;
    let Blk { left: block_left, column, .. } = *blk;
    let mut left = block_left;
    let size = theme.base;
    let mut spacing = Spacing::for_size(size);
    spacing.keep_korean_words = theme.keep_korean_words;
    spacing.punctuation_compression = theme.punctuation_compression;
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
        // And a cell that is too wide for its column cannot be allowed to shrink its
        // way out of the problem either -- the painter leaves a ragged line alone,
        // so the ink would land on the neighbour to the right. Break instead.
        opts.tight_box = true;
        opts.hanging_punctuation = 0.0;
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
            // The widest line the cell can be made to have, not the sum of its content:
            // a `<br>` is the author asking for two lines here, and a column wide enough
            // for both of them on one line would be a column no cell needed.
            let mut widest: Pt = 0.0;
            let mut run: Pt = 0.0;
            for it in &para.items {
                match *it {
                    Item::Box { node } => run += para.node(node).advance,
                    Item::Glue { base, .. } => run += base,
                    Item::Penalty { width, forced, .. } => {
                        if forced {
                            widest = widest.max(run);
                            run = 0.0;
                        } else {
                            run += width;
                        }
                    }
                }
            }
            widths[i] = widths[i].max(widest.max(run) + pad * 2.0);
        }
    };
    measure(&mut widths, &t.head);
    for r in &t.rows {
        measure(&mut widths, r);
    }

    let natural_total: Pt = widths.iter().sum();
    let mut total = natural_total;
    if total > column {
        if total <= wide_limit {
            // A table that is wider than the measure but still fits the window may use
            // the empty margin rather than shrinking its words. The visible reading
            // column stays the same; the extra width is a pannable region below.
            left -= (total - column) * 0.5;
        } else {
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
    }

    let mut paint_row = |cells: &[PreparedCell], top: Pt, head: bool, ops: &mut Vec<Op>, hots: &mut Vec<Hot>, sel: &mut Vec<SelLine>| -> Pt {
        // Measure every cell first: the row is as tall as its tallest cell.
        let mut heights = vec![0.0f32; cols];
        // How far one line of each cell steps down. Normally the body leading, but an
        // inline object brings a box the leading was never asked about -- a fraction
        // stands taller than the text around it -- and a cell whose step is fixed
        // would otherwise walk that ink into the row underneath.
        let mut step = vec![size * leading; cols];
        // Which column reaches each line of the row first. A cell that wraps down is
        // not starting a new row, and the separator a copy uses has to answer to the
        // row's shape -- one tab-separated line per line the reader sees -- rather than
        // to the order the columns happened to be painted in.
        let mut opens: Vec<usize> = Vec::new();
        for (i, c) in cells.iter().enumerate().take(cols) {
            let (_, plan) = set_cell(font, c, &spacing, inner_of(widths[i]));
            let n = plan.lines.len();
            step[i] = (size * leading).max(
                c.spans
                    .iter()
                    .filter_map(|s| styles[s.style.0 as usize].object)
                    .map(|o| o.ascent + o.descent)
                    .fold(0.0f32, f32::max),
            );
            heights[i] = n.max(1) as Pt * step[i];
            for j in 0..n {
                if j >= opens.len() {
                    opens.push(i);
                }
            }
        }
        let row_h = heights.iter().copied().fold(pad, f32::max) + pad;

        let mut x = left;
        for (i, c) in cells.iter().enumerate().take(cols) {
            let inner = inner_of(widths[i]);
            let (para, plan) = set_cell(font, c, &spacing, inner);
            // A cell is its own piece of text to copy: two of them that sit side by
            // side are columns, so the one on the right is a tab away from the last.
            let mut consumed = 0usize;
            let mut lines_out = 0usize;
            let mut ly = top + pad * 0.5;
            let bidi = rubrica_type::BidiInfo::new(&c.text, None);
            for line in &plan.lines {
                let placed = place_bidi(&para, line, &bidi);
                if line.hyphen.is_some() {
                    hyphens.breaks += 1;
                }
                let start = placed.first().map_or(0.0, |p| p.x);
                let w = placed.last().map(|p| p.x + p.w - start).unwrap_or(0.0);
                let shift = match c.align {
                    Align::Left => 0.0,
                    Align::Center => (inner - w) * 0.5,
                    Align::Right => (inner - w).max(0.0),
                } - start;
                let mut runs: Vec<PaintRun> = Vec::new();
                let mut ascent = 0.0f32;
                let mut segs: Vec<(std::ops::Range<usize>, Pt, Pt)> = Vec::new();
                let mut marks: Vec<(usize, Pt, Pt)> = Vec::new();
                let mut hit: Vec<Vec<(Pt, Pt)>> = vec![Vec::new(); c.actions.len()];
                let mut bars: Vec<(Pt, Pt, Pt, Pt, ColorRole)> = Vec::new();
                let mut line_math_texts: Vec<MathTextFragment> = Vec::new();
                // An image's box, waiting for the line's baseline: file, x and width
                // already in device pixels, ascent and descent still in points.
                let mut pics: Vec<(std::path::PathBuf, f32, f32, Pt, Pt)> = Vec::new();
                let mut ruled: Option<Rule> = None;
                for slot in placed {
                    let Some(node_id) = slot.node else {
                        mark_glue(&c.text, &slot, x + pad + shift, &mut segs, &mut marks);
                        if let Some((a, b)) = slot.source {
                            let left = x + pad + shift + slot.x;
                            merge_hot(&mut hit, &c.actions, &(a..b), left, left + slot.w);
                        }
                        continue;
                    };
                    let node = para.node(node_id);
                    let st = &styles[node.style.0 as usize];
                    if ruled.as_ref().is_some_and(|r| r.style != node.style) {
                        end_rule(&mut ruled, &mut bars);
                    }
                    let mut at = x + pad + shift + slot.x;
                    if let Some(o) = st.object {
                        // A formula or a picture inside a cell. The line already gave it
                        // room -- the box was measured when the span was interned, which
                        // is why a cell with a formula in it used to hold exactly that
                        // formula's width of nothing: the cell loop shaped text and only
                        // text, so the ink had nowhere to come from.
                        ascent = ascent.max(o.ascent);
                        match &st.source {
                            Some(ObjectSource::Image(file)) => {
                                pics.push((file.clone(), at * k, o.advance * k, o.ascent, o.descent));
                            }
                            Some(ObjectSource::Math(index)) => {
                                if let Some(entry) = math.get(*index) {
                                    for (r, dx, dy) in &entry.parts {
                                        if let Some(p) =
                                            paint_run(font, r, at + dx, *dy, k, st.color, None)
                                        {
                                            runs.push(p);
                                        }
                                    }
                                    if let Some((first, _, _)) = entry.parts.first() {
                                        line_math_texts.push(MathTextFragment {
                                            source: entry.source.clone(),
                                            x: at,
                                            y: 0.0,
                                            w: o.advance,
                                            h: o.ascent + o.descent,
                                            face_index: first.face,
                                            em: first.size,
                                        });
                                    }
                                    bars.extend(
                                        entry.rules.iter().map(|(rx, top, w, h)| (*rx + at, *top, *w, *h, st.color)),
                                    );
                                }
                            }
                            None => {}
                        }
                        merge_hot(&mut hit, &c.actions, &node.text, at, at + o.advance);
                        segs.push((node.text.clone(), at, at + o.advance));
                        continue;
                    }
                    // The mark a discretionary break ends a line with, drawn the same
                    // way the prose loop draws it: a cell's words hyphenate too, and the
                    // hyphen is no more in the cell's source than in a paragraph's.
                    let hyphen = node.kind == rubrica_type::paragraph::NodeKind::Hyphen;
                    if hyphen && node.advance == 0.0 {
                        // A code line cut because it would not fit: the solver charged
                        // nothing for it, so there is no mark to draw -- and a hyphen in
                        // the middle of an identifier would be a lie about its name.
                        continue;
                    }
                    let (from, range) = if hyphen {
                        (HYPHEN, HYPHEN_RANGE)
                    } else {
                        (c.text.as_str(), node.text.clone())
                    };
                    let mut shaped = font.shape_runs(from, range.clone(), &st.face, st.size, st.tracking);
                    if node.punctuation.is_some() {
                        fit_punctuation_runs(&mut shaped, node.advance);
                    }
                    for r in shaped {
                        if !hyphen {
                            mark_run(&c.text, &r, at, &mut marks);
                        }
                        // A citation inside a cell is raised like one inside prose.
                        ascent = ascent.max(r.ascent + st.raise);
                        let Some(face) = font.font_face(r.face) else { continue };
                        let width = r.width();
                        if st.strike && width > 0.0 {
                            let (pos, weight) = font.strike_rule(r.face, r.size);
                            merge_rule(
                                &mut ruled,
                                &mut bars,
                                Rule {
                                    style: node.style,
                                    x: at,
                                    // Same offset as the raised mark it may strike through.
                                    top: -st.raise - pos - weight * 0.5,
                                    w: width,
                                    h: weight,
                                    color: st.color,
                                },
                            );
                        }
                        let source = if hyphen { None } else { from.get(range.clone()).map(str::to_owned) };
                        runs.push(PaintRun {
                            bidi_level: r.bidi_level,
                            family: font.face_family(r.face),
                            face,
                            face_index: r.face,
                            em: r.size * k,
                            glyphs: r.glyphs,
                            advances: r.advances.iter().map(|a| a * k).collect(),
                            offsets: r
                                .offsets
                                .iter()
                                .map(|offset| DWRITE_GLYPH_OFFSET {
                                    advanceOffset: offset.advanceOffset * k,
                                    ascenderOffset: offset.ascenderOffset * k,
                                })
                                .collect(),
                            source,
                            clusters: r.clusters,
                            x: at * k,
                            baseline: 0.0,
                            dy: -st.raise,
                            color: st.color,
                        });
                        if hyphen {
                            hyphens.marks += 1;
                        }
                        at += width;
                    }
                    if hyphen {
                        // Ink on the line, not text in the cell.
                        continue;
                    }
                    merge_hot(&mut hit, &c.actions, &node.text, x + pad + shift + slot.x, at);
                    segs.push((node.text.clone(), x + pad + shift + slot.x, at));
                }
                end_rule(&mut ruled, &mut bars);
                seat(&mut runs, ly + ascent, k);
                for mut text in line_math_texts.drain(..) {
                    text.y = ly + ascent;
                    math_texts.push(text);
                }
                for (file, px, w, a, d) in pics.drain(..) {
                    ops.push(Op::Image {
                        path: file,
                        x: px,
                        y: (ly + ascent - a) * k,
                        w,
                        h: (a + d) * k,
                    });
                }
                if !runs.is_empty() {
                    ops.push(Op::Runs(runs));
                }
                for (bx, top, w, h, color) in bars {
                    ops.push(Op::Rect {
                        x: bx * k,
                        y: (ly + ascent + top) * k,
                        w: w * k,
                        h: h * k,
                        color,
                    });
                }
                emit_hots(hots, &c.actions, &hit, ctx, ly, step[i]);
                marks.sort_unstable_by_key(|m| m.0);
                let join = if opens[lines_out] == i {
                    // The first cell of a line the row has reached: a new line of the
                    // grid. The header's own first line is a paragraph away from
                    // whatever precedes the table.
                    if head && lines_out == 0 {
                        Join::Blank
                    } else {
                        Join::Newline
                    }
                } else {
                    // Another cell on the same line: a column away from the last.
                    Join::Tab
                };
                lines_out += 1;
                let start = consumed;
                if let Some(mut l) = mark_line(
                    &c.text,
                    Band { top: ly, h: step[i], k, join },
                    &segs,
                    &marks,
                    &mut consumed,
                ) {
                    copy_objects(&mut l, &c.text, start, &c.objects, 0);
                    source_line(&mut l, &c.text, start, &c.sources, 0);
                    sel.push(l);
                }
                ly += step[i];
            }
            x += widths[i];
        }
        row_h
    };

    let grid_w = total;
    let grid_top = y;
    let header_start = ops.len();
    ops.push(Op::Rect { x: left * k, y: y * k, w: grid_w * k, h: 0.0, color: ColorRole::Surface });
    let panel = ops.len() - 1;

    let head_h = paint_row(&t.head, y, true, ops, hots, sel);
    y += head_h;
    // The header's rule is the one line a grid cannot do without, so it is the darkest
    // of them; the rules between rows only have to say where a row ends, and are drawn
    // lighter so that a page of tables reads as text with structure rather than as a
    // spreadsheet. Both are fractions of the type size, since neither is a device
    // pixel: at 200% dpi a hairline set in pixels is a bar.
    let rule = size * 0.05;
    ops.push(Op::Line {
        x0: left * k,
        y0: y * k,
        x1: (left + grid_w) * k,
        y1: y * k,
        thickness: rule * k,
        color: ColorRole::Muted,
    });
    let header_end = ops.len();
    let mut header_ops = ops[header_start..header_end].to_vec();
    if let Some(Op::Rect { h, .. }) = header_ops.first_mut() {
        *h = head_h * k;
    }
    let header_index = table_headers.len();
    table_headers.push(TableHeaderFragment { y: grid_top, height: head_h, ops: header_ops });
    for r in &t.rows {
        y += paint_row(r, y, false, ops, hots, sel);
        // A cell that wraps has no other ending. Without a rule under each row, the
        // second line of one cell reads as the first line of the cell below it -- which
        // is exactly the failure a grid of narrow columns produces on every row, and the
        // reason the lines are drawn rather than left to the space between rows.
        ops.push(Op::Line {
            x0: left * k,
            y0: y * k,
            x1: (left + grid_w) * k,
            y1: y * k,
            thickness: rule * 0.6 * k,
            color: ColorRole::Faint,
        });
    }
    // Last, because a column rule spans a height the rows have only just told. The
    // boundaries are the widths themselves rather than `grid_w` divided up: a cell is
    // laid out at its column's measured width, and a rule between two other numbers
    // would sit on top of somebody's ink.
    let mut x = left;
    for (i, w) in widths.iter().enumerate() {
        x += w;
        if i + 1 == cols {
            break;
        }
        ops.push(Op::Line {
            x0: x * k,
            y0: grid_top * k,
            x1: x * k,
            y1: y * k,
            thickness: rule * 0.6 * k,
            color: ColorRole::Faint,
        });
    }
    // The header panel is drawn before its text, so its height can only be filled
    // in once the first row has been measured.
    if let Some(Op::Rect { h, .. }) = ops.get_mut(panel) {
        *h = head_h * k;
    }
    if total > column {
        let index = wide.len();
        wide.push(WideRegion {
            kind: WideKind::Table,
            x: left * k,
            y: grid_top * k,
            w: column * k,
            h: (y - grid_top) * k,
            content_w: total * k,
        });
        hots.push(Hot {
            x: left * k,
            y: grid_top * k,
            w: column * k,
            h: (y - grid_top) * k,
            kind: HotKind::Wide(index),
        });
    }
    table_spans.push(TableSpan { y: grid_top, height: y - grid_top, header: header_index });
    y
}

/// Intern an inline object's style: the box the line has to make room for, and what
/// draws inside it.
///
/// No face is asked for, because the object's own pieces -- a bitmap, or a formula's
/// already-shaped runs -- carry everything the painter needs. `color` still matters:
/// it is what makes a formula in a heading match the heading's ink.
fn intern_object(styles: &mut Vec<AppStyle>, source: ObjectSource, color: ColorRole, object: ObjectBox) -> StyleId {
    let app = AppStyle {
        face: FaceRequest {
            family: String::new(),
            cjk_family: String::new(),
            fallback: vec![],
            weight: 400,
            italic: false,
            ..Default::default()
        },
        size: 0.0,
        tracking: 0.0,
        color,
        // An object places itself: its pieces carry their own offsets.
        raise: 0.0,
        object: Some(object),
        source: Some(source),
        strike: false,
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
            japanese_family: r.japanese_family,
            korean_family: r.korean_family,
            cjk_italic: r.cjk_italic,
            fallback: fallback.to_vec(),
            weight: r.weight,
            italic: r.italic,
        },
        size: r.size,
        tracking: r.tracking,
        color: r.color,
        raise: r.raise,
        object: None,
        source: None,
        strike: r.strike,
    };
    if let Some(i) = styles.iter().position(|s| *s == app) {
        return StyleId(i as u16);
    }
    styles.push(app);
    StyleId((styles.len() - 1) as u16)
}

fn marker_for(b: &Block) -> String {
    match b.list {
        // A task item's box replaces its bullet, as GitHub and CommonMark do: the
        // bullet would push the box away from the text it belongs to, and a checked
        // item that reads as a bullet has lost the only information it carried.
        Some(_) if b.task == Some(true) => "\u{2611} ".to_string(),
        Some(_) if b.task == Some(false) => "\u{2610} ".to_string(),
        Some(l) if l.ordered => format!("{}. ", l.index.unwrap_or(1)),
        Some(_) => "\u{2022} ".to_string(),
        None => String::new(),
    }
}
/// The services an inline object needs, bundled so that adding a third kind of
/// figure does not add a third argument to the layout entry point.
pub struct Objects<'a> {
    images: Option<&'a ImageStore>,
    /// Directory a relative image source resolves against, normally the document's.
    base_dir: Option<&'a std::path::Path>,
    /// Formulas typeset so far: written while styles are interned, read back when the
    /// same style ids are laid out, so no document is set twice.
    math: &'a mut MathStore,
}

impl<'a> Objects<'a> {
    pub fn new(
        images: Option<&'a ImageStore>,
        base_dir: Option<&'a std::path::Path>,
        math: &'a mut MathStore,
    ) -> Objects<'a> {
        Objects { images, base_dir, math }
    }

    /// The style for one object span, or `None` when it cannot be shown and its
    /// placeholder should be left to read as the block's own prose.
    ///
    /// `prose` is the block's own resolved style, and gives the object its size and
    /// ink -- which is what keeps a formula in a heading at heading size and in
    /// heading colour. It is owned rather than borrowed because growing `styles` below
    /// cannot alias the table the id was read out of.
    fn intern(
        &mut self,
        font: &FontEngine,
        styles: &mut Vec<AppStyle>,
        theme: &Theme,
        obj: &rubrica_doc::ObjectSpan,
        column: Pt,
        prose: AppStyle,
    ) -> Option<StyleId> {
        let (size, color) = (prose.size, prose.color);
        match &obj.kind {
            rubrica_doc::ObjectKind::Image { src, .. } => {
                // A missing store means COM or WIC could not be started, which is a
                // different failure from one bad file and has to be reported as such
                // rather than passed off as prose.
                let Some(store) = self.images else {
                    eprintln!("figures disabled: no image decoder");
                    return None;
                };
                let path = ImageStore::resolve(self.base_dir, src);
                let Some((w, h)) = store.fit(&path, column) else {
                    // A missing figure keeps measuring nothing, and is reported once.
                    // Swapping in the alt text here would shift every later span
                    // offset, since the block's text and ranges were built already.
                    eprintln!("image not rendered: {}", path.display());
                    return None;
                };
                // Sit the figure on the baseline with a small descent so it does not
                // ride above the text.
                let descent = (h * 0.15).min(theme.base * 0.4);
                Some(intern_object(
                    styles,
                    ObjectSource::Image(path),
                    color,
                    ObjectBox { advance: w, ascent: h - descent, descent },
                ))
            }
            rubrica_doc::ObjectKind::Math { source, display } => {
                // One face for the whole formula's *structure*, because a `MATH` table is
                // a property of a face rather than of a theme role: the constants that
                // place every bar and script come from it, so the choice decides how a
                // formula looks more than any size or weight does. `Cambria Math` is the
                // face the platform ships with real math tables; the fallback is asked
                // for its own. Text the symbol face cannot carry -- a `\text{其中}` in a
                // Chinese document -- goes back to the block's prose face, which is the
                // one thing the math face has no hope of supplying.
                let names = &theme.fonts.math;
                let req = FaceRequest {
                    family: names[0].clone(),
                    cjk_family: names[0].clone(),
                    fallback: vec![names[1].clone()],
                    weight: 400,
                    italic: false,
                    ..Default::default()
                };
                let index = self.math.intern(font, &req, &prose.face, source, size, *display)?;
                let box_ = self.math.get(index)?.object;
                Some(intern_object(styles, ObjectSource::Math(index), color, box_))
            }
        }
    }
}

/// How one block is set: what leads it, and the size override when its own kind
/// does not decide.
///
/// The two arrive together and say one thing -- this paragraph is a footnote's, so
/// it is led by its number and set down from the page's prose -- so they travel
/// together rather than as two arguments every caller has to keep in step.
struct Setting {
    marker: String,
    size: Option<Pt>,
}

/// Read one block ready for layout: its marker in front of the text, and every
/// inline style, object and table cell resolved to an id in `styles`.
///
/// `set.size` overrides the size the block's kind implies, which is how a note's
/// prose is set down: the block is an ordinary Markdown paragraph, and the document
/// model cannot say it is a note's, because Markdown has no footnote block kind --
/// footnotes are a construct of the reader. `None` means the kind decides, which is
/// every block on the page itself.
fn prepare_block(
    font: &FontEngine,
    theme: &Theme,
    styles: &mut Vec<AppStyle>,
    objects: &mut Objects<'_>,
    b: &Block,
    set: &Setting,
    column: Pt,
) -> Prepared {
    let smaller = set.size;
    let r = |kind: BlockKind, inline: InlineStyle| match smaller {
        Some(size) => theme.resolve_at(kind, inline, size),
        None => theme.resolve(kind, inline),
    };
    let shift = set.marker.len();
    let mut text = set.marker.clone();
    text.push_str(&b.text);
    let mut spans: Vec<StyleSpan> = Vec::with_capacity(b.spans.len() + 1);
    let mut hang = 0.0;
    if shift > 0 {
        let id = intern(styles, &theme.fonts.fallback, r(BlockKind::Paragraph, InlineStyle::EMPTY));
        spans.push(StyleSpan { range: 0..shift, style: id });
        // Shaped through the same entry point the body is shaped by, so the measure the
        // lines below give up is exactly the width the marker takes up here -- which is
        // what makes a `☐` and a `10.` each clear their own space rather than a guess.
        let st = &styles[id.0 as usize];
        hang = font
            .shape_runs(&text, 0..shift, &st.face, st.size, st.tracking)
            .iter()
            .map(|r| r.width())
            .sum();
    }
    let base = intern(styles, &theme.fonts.fallback, r(b.kind, InlineStyle::EMPTY));
    for s in &b.spans {
        // An object span is sized from its own source rather than from a font,
        // and only the caller knows the document directory a relative image
        // should resolve against.
        //
        // Accent ink is a promise about the click, so a link the reader cannot follow
        // -- a relative path, a `file:` address -- is set as the prose around it rather
        // than coloured and left to be discovered.
        let mut style = s.style;
        if style.contains(InlineStyle::LINK) && !reaches(&b.actions, &s.range) {
            style.remove(InlineStyle::LINK);
        }
        let id = match style {
            st if st.contains(InlineStyle::OBJECT) => {
                match b.objects.iter().find(|o| o.range == s.range) {
                    Some(obj) => {
                        // Read out of the table before it grows, and by value: the
                        // intern below pushes into it.
                        let prose = styles[base.0 as usize].clone();
                        objects.intern(font, styles, theme, obj, column, prose).unwrap_or(base)
                    }
                    None => base,
                }
            }
            _ => intern(styles, &theme.fonts.fallback, r(b.kind, s.style)),
        };
        spans.push(StyleSpan { range: s.range.start + shift..s.range.end + shift, style: id });
    }
    // A fence's words mean their language's things rather than the sentence's, so the
    // one span the parser hands over for the whole block is cut up here and every piece
    // set in the ink its kind asks for. Where the author named no language, or named one
    // this build has not heard of, nothing is cut and the block reads as plain source --
    // which is the honest page rather than a guess at one.
    let cut = if b.kind == BlockKind::Code {
        b.lang.as_deref().map_or(Vec::new(), |l| crate::highlight::tokens(l, &b.text))
    } else {
        Vec::new()
    };
    if !cut.is_empty() {
        // The marker of the list item the fence sits in stays: it is prose's ink, and
        // it is not part of the code.
        spans.retain(|s| s.range.end <= shift);
        for (range, role) in cut {
            let mut res = r(b.kind, InlineStyle::EMPTY);
            res.color = role;
            spans.push(StyleSpan {
                range: range.start + shift..range.end + shift,
                style: intern(styles, &theme.fonts.fallback, res),
            });
        }
    }
    let table = b.table.as_ref().map(|t| {
        let mut prep = |cells: &[rubrica_doc::Cell]| -> Vec<PreparedCell> {
            cells
                .iter()
                .enumerate()
                .map(|(i, c)| PreparedCell {
                    sources: c.sources.clone(),
                    text: c.text.clone(),
                    spans: c
                        .spans
                        .iter()
                        .map(|sp| {
                            // Same rule as prose: ink is coloured as a link only where
                            // the link leads somewhere the reader can go.
                            let mut style = sp.style;
                            if style.contains(InlineStyle::LINK) && !reaches(&c.actions, &sp.range) {
                                style.remove(InlineStyle::LINK);
                            }
                            // And the same object arm as prose. A cell's formula is
                            // interned here rather than painted there, because a cell is
                            // measured at the block's width and drawn at its column's,
                            // and only this loop has both the cell's own ranges and the
                            // cache to hand.
                            let id = if style.contains(InlineStyle::OBJECT) {
                                match c.objects.iter().find(|o| o.range == sp.range) {
                                    Some(obj) => {
                                        let prose = styles[base.0 as usize].clone();
                                        objects
                                            .intern(font, styles, theme, obj, column, prose)
                                            .unwrap_or(base)
                                    }
                                    None => base,
                                }
                            } else {
                                intern(styles, &theme.fonts.fallback, r(b.kind, style))
                            };
                            StyleSpan { range: sp.range.clone(), style: id }
                        })
                        .collect(),
                    align: t.aligns.get(i).copied().unwrap_or_default(),
                    actions: c.actions.clone(),
                    objects: c.objects.clone(),
                })
                .collect()
        };
        PreparedTable { head: prep(&t.head), rows: t.rows.iter().map(|r| prep(r)).collect() }
    });
    Prepared {
        text,
        spans,
        base: base.0 as usize,
        hang,
        table,
        actions: b
            .actions
            .iter()
            .map(|a| Action { range: a.range.start + shift..a.range.end + shift, kind: a.kind.clone() })
            .collect(),
    }
}

/// Whether any of the block's clickable ranges covers this span's text, i.e. whether
/// there is anywhere for a click on this ink to go.
fn reaches(actions: &[Action], range: &std::ops::Range<usize>) -> bool {
    actions.iter().any(|a| {
        a.range.start < range.end && range.start < a.range.end
            && matches!(&a.kind, ActionKind::Url(u) if openable(u))
    })
}

/// Where the words of `text` may split, and the advance of the hyphen glyph at the
/// block's own base style.
///
/// A code block is the one kind of text that is *never* allowed to run past the measure
/// unread: it has no horizontal scroll, its lines are the longest in a document, and a
/// character cut off at the window edge is a character the reader cannot ask for. So
/// every character boundary is offered as a break -- with no mark drawn, because a
/// hyphen in the middle of an identifier would be a lie about its name. The solver
/// charges a break's width, so a zero width is also what tells the painter to leave the
/// line end alone.
fn hyphenation_for(
    text: &str,
    styles: &[AppStyle],
    base: usize,
    hyphenator: Option<&Hyphenator>,
    font: &mut FontEngine,
    code: bool,
) -> (Vec<usize>, Pt) {
    if code {
        let points: Vec<usize> = text
            .char_indices()
            .skip(1)
            .map(|(at, _)| at)
            .filter(|&at| {
                !text[..at].ends_with(char::is_whitespace)
                    && !text[at..].starts_with(char::is_whitespace)
            })
            .collect();
        return (points, 0.0);
    }
    let points: Vec<usize> = hyphenator.map(|h| h.points(text)).filter(|v| !v.is_empty()).unwrap_or_default();
    let width = if points.is_empty() {
        0.0
    } else {
        let st = &styles[base];
        font.shape_runs(HYPHEN, HYPHEN_RANGE, &st.face, st.size, st.tracking)
            .iter()
            .map(|r| r.width())
            .sum()
    };
    (points, width)
}

/// Typeset the whole document into a display list.
///
/// Shared verbatim by the window and by `rubrica-app --report`, so a numeric check
/// describes exactly what the user sees rather than what a parallel implementation
/// would have drawn.
///
/// Returns a [`Page`]: the display list in device independent pixels, the heights and
/// edges the report measures against, and the rectangles a click can land on.
#[allow(clippy::too_many_arguments)]
/// Render a complete document to a PNG file using the same display list as the reader.
///
/// `width` is expressed in DIPs and `scale` controls the output pixel density. The
/// layout is built at `72 * scale` DPI so the coordinates in `Page.ops` are already
/// pixels and the offscreen target does not apply a second scale.
pub(crate) fn export_png(
    source: &str,
    input_path: Option<&Path>,
    output: &Path,
    width: f32,
    scale: f32,
    dark: bool,
    plain: bool,
    text_options: TextOptions,
    source_view: bool,
    keep_line_breaks: bool,
    zoom: Zoom,
    face: Option<usize>,
    measure: Option<usize>,
    profile: Option<&str>,
    hyphenate: bool,
) -> Result<()> {
    if !width.is_finite() || width <= 0.0 || !scale.is_finite() || scale <= 0.0 {
        return Err("export width and scale must be positive numbers".into());
    }
    let mut theme = Theme::default();
    if let Some(name) = profile {
        crate::profiles::load(name).apply(&mut theme);
    }
    if let Some(face) = face.filter(|i| TextFace::ALL.get(*i).is_some()) {
        theme.set_face(face);
    }
    if let Some(measure) = measure.filter(|i| Measure::ALL.get(*i).is_some()) {
        theme.set_measure(measure);
    }
    theme.set_zoom(zoom);
    let doc = if source_view {
        Document::source(source)
    } else if plain {
        rubrica_doc::plain::parse(source, text_options)
    } else {
        Document::parse_with(source, rubrica_doc::ParseOptions { keep_line_breaks })
    };
    let mut font = FontEngine::new().map_err(|e| -> Error { format!("DirectWrite: {e}").into() })?;
    if !font.probe() {
        return Err("no usable font face".into());
    }
    let _ = unsafe {
        CoInitializeEx(None, COINIT_APARTMENTTHREADED)
    };
    let store = ImageStore::new().ok();
    let base = input_path.and_then(Path::parent);
    let mut math = crate::math::MathStore::new();
    let mut objects = Objects::new(store.as_ref(), base, &mut math);
    let hyphenator = if hyphenate { crate::hyphen::Hyphenator::english() } else { None };
    let page = build_ops(
        &mut font,
        &theme,
        &doc,
        width,
        72.0 * scale,
        &mut objects,
        hyphenator.as_ref(),
    );
    let pixel_width = (width * scale).round().max(1.0) as u32;
    let pixel_height = (page.height * scale).ceil().max(1.0) as u32;
    if pixel_width as u64 * pixel_height as u64 > 100_000_000 {
        return Err("export image is too large; reduce --export-width or --export-scale".into());
    }
    unsafe { write_png(&page, output, pixel_width, pixel_height, dark, store.as_ref()) }
}

unsafe fn write_png(
    page: &Page,
    output: &Path,
    width: u32,
    height: u32,
    dark: bool,
    images: Option<&ImageStore>,
) -> Result<()> {
    let wic: IWICImagingFactory = CoCreateInstance(
        &CLSID_WICImagingFactory,
        None,
        CLSCTX_INPROC_SERVER,
    )?;
    let bitmap = wic.CreateBitmap(
        width,
        height,
        &GUID_WICPixelFormat32bppPBGRA,
        WICBitmapCacheOnLoad,
    )?;
    let factory: ID2D1Factory = D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None)?;
    let props = D2D1_RENDER_TARGET_PROPERTIES {
        r#type: D2D1_RENDER_TARGET_TYPE_DEFAULT,
        pixelFormat: D2D1_PIXEL_FORMAT {
            format: DXGI_FORMAT_B8G8R8A8_UNORM,
            alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
        },
        dpiX: 72.0,
        dpiY: 72.0,
        usage: D2D1_RENDER_TARGET_USAGE_NONE,
        minLevel: D2D1_FEATURE_LEVEL_DEFAULT,
    };
    let target = factory.CreateWicBitmapRenderTarget(&bitmap, &props)?;
    target.BeginDraw();
    let palette = Palette::of(dark);
    let bg = d2d(palette.bg);
    target.Clear(Some(&bg));
    let mut brushes: HashMap<ColorRole, ID2D1SolidColorBrush> = HashMap::new();
    let mut brush = |role: ColorRole| -> Result<ID2D1SolidColorBrush> {
        if let Some(existing) = brushes.get(&role) {
            return Ok(existing.clone());
        }
        let color = d2d(palette.ink(role));
        let made = target.CreateSolidColorBrush(&color, None)?;
        brushes.insert(role, made.clone());
        Ok(made)
    };
    for op in &page.ops {
        match op {
            Op::Rect { x, y, w, h, color } => {
                let r = D2D_RECT_F { left: *x, top: *y, right: x + w, bottom: y + h };
                target.FillRectangle(&r, &brush(*color)?);
            }
            Op::Line { x0, y0, x1, y1, thickness, color } => {
                target.DrawLine(
                    Vector2::new(*x0, *y0),
                    Vector2::new(*x1, *y1),
                    &brush(*color)?,
                    *thickness,
                    None,
                );
            }
            Op::Image { path, x, y, w, h } => {
                if let Some(bitmap) = images.and_then(|store| store.bitmap(&target, path)) {
                    let r = D2D_RECT_F { left: *x, top: *y, right: x + w, bottom: y + h };
                    target.DrawBitmap(&bitmap, Some(&r), 1.0, D2D1_BITMAP_INTERPOLATION_MODE_LINEAR, None);
                }
            }
            Op::Runs(runs) => {
                for run in runs {
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
                        Vector2::new(glyph_origin(run.x, run.advances.iter().sum(), run.bidi_level), run.baseline),
                        &description,
                        &brush(run.color)?,
                        DWRITE_MEASURING_MODE_NATURAL,
                    );
                    let _ = std::mem::ManuallyDrop::into_inner(description.fontFace);
                }
            }
        }
    }
    target.EndDraw(None, None)?;
    let wide = utf16(&output.to_string_lossy());
    let stream = wic.CreateStream()?;
    stream.InitializeFromFilename(PCWSTR(wide.as_ptr()), GENERIC_WRITE.0)?;
    let encoder = wic.CreateEncoder(&GUID_ContainerFormatPng, std::ptr::null())?;
    encoder.Initialize(&stream, WICBitmapEncoderNoCache)?;
    let mut frame = None;
    let mut options = None;
    encoder.CreateNewFrame(&mut frame, &mut options)?;
    let frame = frame.ok_or("PNG encoder returned no frame")?;
    frame.Initialize(None)?;
    frame.SetSize(width, height)?;
    let mut format = GUID_WICPixelFormat32bppPBGRA;
    frame.SetPixelFormat(&mut format)?;
    frame.WriteSource(&bitmap.cast::<IWICBitmapSource>()?, std::ptr::null())?;
    frame.Commit()?;
    encoder.Commit()?;
    Ok(())
}

/// Export the same display list as a multi-page PDF with selectable text.
///
/// The page is measured at 72 dpi, so every coordinate in `Page` is already a PDF
/// point. Glyphs are emitted as positioned glyph ids with a ToUnicode value rather
/// than flattened to a screenshot: copying from the PDF therefore sees the document's
/// Unicode, including CJK, while the embedded sfnt keeps the exact DirectWrite shapes.
#[allow(clippy::too_many_arguments)]
pub fn export_pdf(
    source: &str,
    input_path: Option<&Path>,
    output: &Path,
    width: Pt,
    height: Pt,
    dark: bool,
    plain: bool,
    text_options: TextOptions,
    source_view: bool,
    keep_line_breaks: bool,
    zoom: Zoom,
    face: Option<usize>,
    measure: Option<usize>,
    profile: Option<&str>,
    hyphenate: bool,
) -> Result<()> {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
    }
    let mut theme = Theme::default();
    theme.set_zoom(zoom);
    if let Some(name) = profile {
        crate::profiles::load(name).apply(&mut theme);
    }
    if let Some(face) = face.filter(|i| TextFace::ALL.get(*i).is_some()) {
        theme.set_face(face);
    }
    if let Some(measure) = measure.filter(|i| Measure::ALL.get(*i).is_some()) {
        theme.set_measure(measure);
    }
    let doc = if source_view {
        Document::source(source)
    } else if plain {
        rubrica_doc::plain::parse(source, text_options)
    } else {
        Document::parse_with(source, rubrica_doc::ParseOptions { keep_line_breaks })
    };
    let mut font = FontEngine::new().map_err(|e| -> Error { format!("DirectWrite: {e}").into() })?;
    if !font.probe() {
        return Err("no usable font face".into());
    }
    let store = ImageStore::new().ok();
    let base = input_path.and_then(Path::parent);
    let mut math = crate::math::MathStore::new();
    let mut objects = Objects::new(store.as_ref(), base, &mut math);
    let hyphenator = if hyphenate { crate::hyphen::Hyphenator::english() } else { None };
    let page = build_ops(
        &mut font,
        &theme,
        &doc,
        width,
        72.0,
        &mut objects,
        hyphenator.as_ref(),
    );
    write_pdf(&page, &font, output, width, height, dark)
}

fn pdf_rgb(c: Rgb) -> PdfColor {
    PdfColor::Rgb(PdfRgb::new(c.r, c.g, c.b, None))
}

fn pdf_color(role: ColorRole, dark: bool) -> PdfColor {
    pdf_rgb(Palette::of(dark).ink(role))
}

fn pdf_point(x: f32, y: f32) -> PdfPoint {
    PdfPoint { x: printpdf::Pt(x), y: printpdf::Pt(y) }
}

fn pdf_text_face(font: &FontEngine, preferred: usize, source: &str) -> usize {
    let mut candidates = vec![preferred];
    for name in ["Segoe UI", "Microsoft YaHei", "Arial", "Cambria Math"] {
        if let Some(index) = font.open_face(name, 400, false) {
            if !candidates.contains(&index) { candidates.push(index); }
        }
    }
    candidates.into_iter().find(|index| {
        source.chars().all(|ch| {
            font.glyph_ids(*index, &ch.to_string())
                .is_some_and(|glyphs| glyphs.first().is_some_and(|glyph| *glyph != 0))
        })
    }).unwrap_or(preferred)
}

fn translate_pdf_op(op: &Op, dy: f32) -> Op {
    let mut translated = op.clone();
    match &mut translated {
        Op::Runs(runs) => for run in runs { run.baseline += dy; },
        Op::Image { y, .. } | Op::Rect { y, .. } => *y += dy,
        Op::Line { y0, y1, .. } => { *y0 += dy; *y1 += dy; }
    }
    translated
}

fn pdf_page_starts(page: &Page, page_height: Pt) -> Vec<Pt> {
    if !page_height.is_finite() || page_height <= 0.0 {
        return vec![0.0];
    }
    if page.sel.is_empty() {
        let mut starts = vec![0.0];
        while *starts.last().unwrap_or(&0.0) + page_height < page.height {
            let next = starts.last().copied().unwrap_or(0.0) + page_height;
            starts.push(next);
        }
        return starts;
    }
    let mut starts = vec![0.0];
    let mut limit = page_height;
    for line in &page.sel {
        if line.y + line.h <= limit + 0.01 {
            continue;
        }
        let previous = *starts.last().unwrap_or(&0.0);
        if line.y <= previous + 0.01 {
            continue;
        }
        // A heading is kept with the lines that follow it. If the first line that
        // crosses the nominal boundary is a heading (or follows one closely), move the
        // break back to that heading instead of leaving it as the last line on a page.
        let heading = page
            .anchor_tops
            .iter()
            .copied()
            .find(|top| *top >= previous && *top < line.y && line.y - *top <= line.h * 2.0);
        let start = heading.unwrap_or(line.y);
        if start > previous + 0.01 {
            starts.push(start);
            limit = start + page_height;
        }
    }
    starts
}

fn pdf_glyph_x(base: f32, advances: &[f32], level: u8, index: usize) -> f32 {
    let before: f32 = advances.iter().take(index).sum();
    let after: f32 = advances.iter().take(index + 1).sum();
    if level % 2 == 1 {
        base + advances.iter().sum::<f32>() - after
    } else {
        base + before
    }
}

fn actual_text_span(text: &str) -> PdfOp {
    let mut data = vec![0xFE, 0xFF];
    for unit in text.encode_utf16() {
        data.extend_from_slice(&unit.to_be_bytes());
    }
    let mut map = std::collections::BTreeMap::new();
    map.insert(
        "ActualText".to_string(),
        DictItem::String { data, literal: false },
    );
    PdfOp::BeginMarkedContentWithProperties {
        tag: "Span".to_string(),
        properties: DictItem::Dict { map },
    }
}

/// The Unicode value attached to one DirectWrite glyph, including the characters
/// represented by a ligature's shared cluster.
#[cfg(test)]
fn unicode_for_glyph(source: &str, clusters: &[u16], glyph_count: usize, index: usize) -> Option<String> {
    unicode_by_glyph(source, clusters, glyph_count).get(index).cloned().flatten()
}

fn write_pdf(
    page: &Page,
    font: &FontEngine,
    output: &Path,
    width: Pt,
    height: Pt,
    dark: bool,
) -> Result<()> {
    if !width.is_finite() || !height.is_finite() || width <= 0.0 || height <= 0.0 {
        return Err("PDF page size must be positive".into());
    }
    let page_starts = pdf_page_starts(page, height);
    let mut pdf = PdfDocument::new(output.file_stem().and_then(|s| s.to_str()).unwrap_or("Rubrica document"));
    let mut warnings = Vec::new();
    let mut fonts: HashMap<usize, PdfFontId> = HashMap::new();
    let mut missing_font_faces = Vec::new();
    let mut images: HashMap<PathBuf, (printpdf::XObjectId, usize, usize)> = HashMap::new();
    let palette = Palette::of(dark);

    for (page_index, top) in page_starts.iter().copied().enumerate() {
        let content_height = page_starts
            .get(page_index + 1)
            .map(|next| (next - top).min(height))
            .unwrap_or(height);
        let mut ops = vec![
            PdfOp::SetFillColor { col: pdf_rgb(palette.bg) },
            PdfOp::DrawRectangle {
                rectangle: PdfRect::from_xywh(
                    printpdf::Pt(0.0),
                    printpdf::Pt(0.0),
                    printpdf::Pt(width),
                    printpdf::Pt(height),
                ),
            },
        ];
        let mut source_ops = Vec::new();
        let mut content_shift = 0.0;
        for span in &page.table_spans {
            if span.y < top && span.y + span.height > top {
                if let Some(header) = page.table_headers.get(span.header) {
                    let dy = top - header.y;
                    source_ops.extend(header.ops.iter().map(|op| translate_pdf_op(op, dy)));
                    content_shift += header.height;
                }
            }
        }
        for note in &page.note_spans {
            if note.y < top && note.y + note.height > top {
                let marker = format!("Footnote {} (continued)", note.number);
                let marker_y = content_shift + 12.0;
                ops.push(PdfOp::SetFillColor { col: pdf_color(ColorRole::Muted, dark) });
                ops.push(PdfOp::SetFont {
                    font: PdfFontHandle::Builtin(BuiltinFont::Helvetica),
                    size: printpdf::Pt(9.0),
                });
                ops.push(PdfOp::SetTextMatrix {
                    matrix: PdfTextMatrix::Raw([1.0, 0.0, 0.0, 1.0, page.left, height - marker_y]),
                });
                ops.push(PdfOp::StartTextSection);
                ops.push(PdfOp::ShowText { items: vec![TextItem::Text(marker)] });
                ops.push(PdfOp::EndTextSection);
                content_shift += 18.0;
            }
        }
        for text in &page.math_texts {
            let local_y = text.y + content_shift - top;
            let local_top = local_y - text.h;
            if local_y <= 0.0 || local_top >= content_height || text.w <= 0.0 {
                continue;
            }
            let face = pdf_text_face(font, text.face_index, &text.source);
            let font_id = if let Some(id) = fonts.get(&face) {
                id.clone()
            } else {
                let Some((bytes, file_face_index)) = font.font_file(face) else { continue };
                let mut font_warnings = Vec::new();
                let Some(parsed) = ParsedFont::from_bytes(&bytes, file_face_index, &mut font_warnings) else { continue };
                let id = pdf.add_font(&parsed);
                fonts.insert(face, id.clone());
                id
            };
            let items = text.source.chars().map(|ch| {
                let glyph = font.glyph_ids(face, &ch.to_string())
                    .and_then(|glyphs| glyphs.first().copied()).unwrap_or(0);
                Codepoint::with_cid(glyph, 0.0, ch.to_string())
            }).collect();
            ops.push(PdfOp::SetFillColor { col: pdf_rgb(palette.bg) });
            ops.push(PdfOp::SetFont { font: PdfFontHandle::External(font_id), size: printpdf::Pt(text.em) });
            ops.push(PdfOp::SetTextMatrix {
                matrix: PdfTextMatrix::Raw([1.0, 0.0, 0.0, 1.0, text.x, height - local_y]),
            });
            ops.push(PdfOp::StartTextSection);
            ops.push(actual_text_span(&text.source));
            ops.push(PdfOp::ShowText { items: vec![TextItem::GlyphIds(items)] });
            ops.push(PdfOp::EndMarkedContent);
            ops.push(PdfOp::EndTextSection);
        }
        source_ops.extend(page.ops.iter().map(|op| translate_pdf_op(op, content_shift)));
        for op in &source_ops {
            match op {
                Op::Rect { x, y, w, h, color } => {
                    let y0 = *y - top;
                    let y1 = y0 + *h;
                    if y1 <= 0.0 || y0 >= content_height || *w <= 0.0 || *h <= 0.0 {
                        continue;
                    }
                    let clipped0 = y0.max(0.0);
                    let clipped1 = y1.min(content_height);
                    ops.push(PdfOp::SetFillColor { col: pdf_color(*color, dark) });
                    ops.push(PdfOp::DrawRectangle {
                        rectangle: PdfRect::from_xywh(
                            printpdf::Pt(*x),
                            printpdf::Pt(height - clipped1),
                            printpdf::Pt(*w),
                            printpdf::Pt(clipped1 - clipped0),
                        ),
                    });
                }
                Op::Line { x0, y0, x1, y1, thickness, color } => {
                    let a = *y0 - top;
                    let b = *y1 - top;
                    if (a < 0.0 && b < 0.0) || (a >= content_height && b >= content_height) {
                        continue;
                    }
                    let (a, b) = if a <= b { (a, b) } else { (b, a) };
                    let clipped0 = a.max(0.0);
                    let clipped1 = b.min(content_height);
                    if clipped1 <= clipped0 {
                        continue;
                    }
                    ops.push(PdfOp::SetOutlineColor { col: pdf_color(*color, dark) });
                    ops.push(PdfOp::SetOutlineThickness { pt: printpdf::Pt(*thickness) });
                    ops.push(PdfOp::DrawLine {
                        line: printpdf::Line {
                            points: vec![
                                printpdf::LinePoint { p: pdf_point(*x0, height - clipped0), bezier: false },
                                printpdf::LinePoint { p: pdf_point(*x1, height - clipped1), bezier: false },
                            ],
                            is_closed: false,
                        },
                    });
                }
                Op::Image { path, x, y, w, h } => {
                    let local_y = *y - top;
                    if local_y + *h <= 0.0 || local_y >= content_height || *w <= 0.0 || *h <= 0.0 {
                        continue;
                    }
                    let (id, pixel_w, pixel_h) = if let Some(hit) = images.get(path) {
                        hit.clone()
                    } else {
                        let Ok(bytes) = std::fs::read(path) else { continue };
                        let Ok(image) = RawImage::decode_from_bytes(&bytes, &mut warnings) else { continue };
                        let id = pdf.add_image(&image);
                        let hit = (id, image.width, image.height);
                        images.insert(path.clone(), hit.clone());
                        hit
                    };
                    let transform = XObjectTransform {
                        translate_x: Some(printpdf::Pt(*x)),
                        translate_y: Some(printpdf::Pt(height - local_y - *h)),
                        scale_x: Some(*w / pixel_w.max(1) as f32),
                        scale_y: Some(*h / pixel_h.max(1) as f32),
                        dpi: Some(72.0),
                        no_auto_scale: true,
                        ..Default::default()
                    };
                    ops.push(PdfOp::UseXobject { id, transform });
                }
                Op::Runs(runs) => {
                    for run in runs {
                        if run.glyphs.is_empty() {
                            continue;
                        }
                        let baseline = run.baseline - top;
                        if baseline < 0.0 || baseline >= content_height {
                            continue;
                        }
                        let font_id = if let Some(id) = fonts.get(&run.face_index) {
                            id.clone()
                        } else {
                            let Some((bytes, face_index)) = font.font_file(run.face_index) else {
                                missing_font_faces.push(run.family.clone());
                                continue;
                            };
                            let mut font_warnings = Vec::new();
                            let Some(parsed) = ParsedFont::from_bytes(&bytes, face_index, &mut font_warnings)
                            else {
                                missing_font_faces.push(run.family.clone());
                                continue;
                            };
                            let id = pdf.add_font(&parsed);
                            fonts.insert(run.face_index, id.clone());
                            id
                        };
                        ops.push(PdfOp::SetFillColor { col: pdf_color(run.color, dark) });
                        ops.push(PdfOp::SetFont {
                            font: PdfFontHandle::External(font_id),
                            size: printpdf::Pt(run.em),
                        });
                        ops.push(PdfOp::StartTextSection);
                        if let Some(source) = run.source.as_deref() {
                            ops.push(actual_text_span(source));
                        }
                        let unicode = run
                            .source
                            .as_deref()
                            .map(|source| unicode_by_glyph(source, &run.clusters, run.glyphs.len()));
                        for (i, glyph) in run.glyphs.iter().enumerate() {
                            let item = unicode
                                .as_ref()
                                .and_then(|values| values.get(i).cloned().flatten())
                                .map(|cid| TextItem::GlyphIds(vec![Codepoint::with_cid(*glyph, 0.0, cid)]))
                                .unwrap_or_else(|| TextItem::GlyphIds(vec![Codepoint::new(*glyph, 0.0)]));
                            let offset = run.offsets.get(i).copied().unwrap_or_default();
                            let direction = if run.bidi_level % 2 == 1 { -1.0 } else { 1.0 };
                            let x = pdf_glyph_x(run.x, &run.advances, run.bidi_level, i)
                                + offset.advanceOffset * direction;
                            let glyph_baseline = baseline - offset.ascenderOffset;
                            ops.push(PdfOp::SetTextMatrix {
                                matrix: PdfTextMatrix::Raw([
                                    1.0,
                                    0.0,
                                    0.0,
                                    1.0,
                                    x,
                                    height - glyph_baseline,
                                ]),
                            });
                            ops.push(PdfOp::ShowText { items: vec![item] });
                        }
                        ops.push(PdfOp::EndTextSection);
                        if run.source.is_some() {
                            ops.push(PdfOp::EndMarkedContent);
                        }
                    }
                }
            }
        }
        for hot in &page.hotspots {
            let HotKind::Url(url) = &hot.kind else { continue };
            let local_y = hot.y + content_shift - top;
            let local_bottom = local_y + hot.h;
            if local_bottom <= 0.0 || local_y >= content_height || hot.w <= 0.0 || hot.h <= 0.0 {
                continue;
            }
            let clipped_top = local_y.max(0.0);
            let clipped_bottom = local_bottom.min(content_height);
            let link = PdfLinkAnnotation::new(
                PdfRect::from_xywh(
                    printpdf::Pt(hot.x),
                    printpdf::Pt(height - clipped_bottom),
                    printpdf::Pt(hot.w),
                    printpdf::Pt(clipped_bottom - clipped_top),
                ),
                PdfActions::Uri(url.clone()),
                None,
                None,
                None,
            );
            ops.push(PdfOp::LinkAnnotation { link });
        }
        pdf.pages.push(PdfPage::new(
            printpdf::Mm(width * 25.4 / 72.0),
            printpdf::Mm(height * 25.4 / 72.0),
            ops,
        ));
    }
    if !missing_font_faces.is_empty() {
        missing_font_faces.sort_unstable();
        missing_font_faces.dedup();
        return Err(format!("PDF cannot embed font(s): {}", missing_font_faces.join(", ")).into());
    }
    let bytes = pdf.save(
        &PdfSaveOptions { optimize: true, subset_fonts: true, ..PdfSaveOptions::default() },
        &mut warnings,
    );
    if let Some(error) = warnings.iter().find(|warning| warning.severity == PdfParseErrorSeverity::Error) {
        return Err(format!("PDF serialization failed: {}", error.msg).into());
    }
    std::fs::write(output, bytes).map_err(|e| format!("cannot write {}: {e}", output.display()))?;
    Ok(())
}

pub fn build_ops(
    font: &mut FontEngine,
    theme: &Theme,
    doc: &Document,
    client_w: f32,
    dpi: f32,
    objects: &mut Objects<'_>,
    hyphenator: Option<&Hyphenator>,
) -> Page {
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
    let mut hots = Vec::new();
    let mut sel: Vec<SelLine> = Vec::new();
    let mut wide: Vec<WideRegion> = Vec::new();
    let mut table_headers: Vec<TableHeaderFragment> = Vec::new();
    let mut table_spans: Vec<TableSpan> = Vec::new();
    let mut note_spans: Vec<NoteSpan> = Vec::new();
    let mut math_texts: Vec<MathTextFragment> = Vec::new();
    let mut breaks = HyphenCount::default();
    let mut note_tops: Vec<Pt> = Vec::with_capacity(doc.footnotes.len());
    // A citation names its note by the author's own label, while the page knows notes
    // only by where they ended up. This is the bridge, and it is built before any
    // layout because layout is where a citation's text is first read.
    let notes: HashMap<String, usize> =
        doc.footnotes.iter().enumerate().map(|(i, f)| (f.label.clone(), i)).collect();
    // A fragment link addresses a heading by the words in it, and the page knows
    // headings only by where they landed, so this is the same bridge a citation needs --
    // built before any layout for the same reason, since layout is where the link's text
    // is first read. The index is the heading's ordinal among the document's headings,
    // which is the order `anchor_tops` is filled in below. Where two headings say the
    // same thing, the address points at the first: that is the only one an author
    // writing the address by hand can have meant.
    let mut anchor_tops: Vec<Pt> = Vec::new();
    let anchors: HashMap<String, usize> = doc
        .blocks
        .iter()
        .filter(|b| matches!(b.kind, BlockKind::Heading(_)))
        .enumerate()
        .fold(HashMap::new(), |mut m, (i, b)| {
            m.entry(slug(&b.text)).or_insert(i);
            m
        });
    let mut y = theme.base * 2.0;
    let mut first = true;

    // Interning has to finish before measuring, because the engine resolves a
    // StyleId through the installed table. That includes the notes' table, which is
    // why they are readied here and drawn at the very end.
    let mut prepared: Vec<Prepared> = Vec::with_capacity(doc.blocks.len());
    for b in &doc.blocks {
        let set = Setting { marker: marker_for(b), size: None };
        prepared.push(prepare_block(font, theme, &mut styles, objects, b, &set, column));
    }
    let note_units: Vec<Vec<Prepared>> = doc
        .footnotes
        .iter()
        .map(|note| {
            note.blocks
                .iter()
                .enumerate()
                .map(|(i, nb)| {
                    // The number leads the note's first block. Later blocks are the
                    // same note continuing, so they need no marker of their own.
                    let set = Setting {
                        marker: if i == 0 { format!("{}. ", note.number) } else { String::new() },
                        size: Some(theme.note_body_size(nb.kind)),
                    };
                    prepare_block(font, theme, &mut styles, objects, nb, &set, column)
                })
                .collect()
        })
        .collect();

    // One hanging indent per list level rather than one per item: an ordered list that
    // runs from `9.` to `10.` still sets every body at the same x, the way a table's
    // second column would. The same for the apparatus, where the numbers `9` and `10`
    // sit in one column of notes. Digits are tabular in the faces in use, so the widest
    // marker of a level is also the measure every narrower one is set against.
    let mut level_hang: Vec<Pt> = Vec::new();
    for (b, p) in doc.blocks.iter().zip(&prepared) {
        if let Some(l) = b.list {
            let d = l.depth as usize;
            level_hang.resize(d + 1, 0.0);
            level_hang[d] = level_hang[d].max(p.hang);
        }
    }
    let note_hang = note_units
        .iter()
        .map(|u| u.first().map_or(0.0, |p| p.hang))
        .fold(0.0f32, Pt::max);

    font.begin_layout(
        styles
            .iter()
            .map(|s| RunStyle { face: s.face.clone(), size: s.size, tracking: s.tracking, object: s.object })
            .collect(),
    );

    let ctx = Ctx {
        theme,
        styles: &styles,
        math: &*objects.math,
        notes: &notes,
        anchors: &anchors,
        base: objects.base_dir,
        k,
        wide_limit: {
            let available = (client_pt - margin * 0.5).max(column);
            column + (available - column) * theme.wide_table_factor.clamp(0.0, 1.0)
        },
    };

    for (b, p) in doc.blocks.iter().zip(prepared) {
        let base_left = left;
        // Offsets are computed against the block's own text, which already carries
        // the list marker, so they need no shifting.
        let (hyphens, hyphen_width) =
            hyphenation_for(&p.text, &styles, p.base, hyphenator, font, b.kind == BlockKind::Code);
        let size = theme.body_size(b.kind);
        y += theme.space_before(b.kind, first);
        first = false;
        // Where a fragment link naming this heading has to land. Pushed here, at the top
        // of the block rather than after it, because the line a reader asks to be taken
        // to is the one their eye goes to first.
        if matches!(b.kind, BlockKind::Heading(_)) {
            anchor_tops.push(y);
        }
        // A list's geometry belongs to the list, not to whichever block happens to be
        // standing at its level: an item's code fence set at its own, smaller size would
        // start its text a little left of the prose above it.
        let depth = b.item_depth.unwrap_or(0) as Pt;
        let level = level_hang.get(depth as usize).copied().unwrap_or(0.0);
        let mut left = base_left
            + b.quote_depth as Pt * theme.quote_indent_em * theme.base
            + depth * theme.list_indent_em * theme.base
            // A block that only continues an item has no marker of its own to hang, so
            // it starts where the item's text does rather than where its marker does.
            // Ordinary prose is neither: it has no item to continue, and must not pay
            // for one.
            + if b.list.is_some() || b.item_depth.is_none() { 0.0 } else { level }
            // A definition stands under its term, which is the one thing the syntax
            // said and the page has to keep saying: run at the margin it is the term
            // again, and the reader has nothing left to tell the two apart.
            + if b.kind == BlockKind::Definition { theme.base * theme.definition_indent_em } else { 0.0 };
        // Every point a block is run in from the margin is a point it has to give back
        // at the right: a nested list or a quotation that kept the page's whole measure
        // would end its lines out past the text standing beside it.
        let inner = (column - (left - base_left)).max(size * 4.0);
        // A displayed equation is the only thing on its line, and an equation on its
        // own is centred rather than run in to the margin: the prose around it is
        // justified, so a short line starting at the left edge reads as a line that
        // broke early, not as a display. It only centres when it fits.
        if b.spans.len() == 1
            && matches!(b.objects.first().map(|o| &o.kind), Some(rubrica_doc::ObjectKind::Math { display: true, .. }))
        {
            if let Some(s) = p.spans.last() {
                if let Some(o) = styles[s.style.0 as usize].object {
                    left += ((inner - o.advance) * 0.5).max(0.0);
                }
            }
        }
        // The block starts at its own left and the marker hangs out of it: no extra
        // indent is added here for a list, because the lines under the marker give that
        // width back themselves, at the width the marker actually has.
        y = layout_block(
            font,
            &ctx,
            &Blk {
                b,
                text: &p.text,
                spans: &p.spans,
                base: p.base,
                left,
                column: inner,
                table: p.table.as_ref(),
                hyphenation: Hyphenation { points: &hyphens, width: hyphen_width },
                size,
                leading: theme.line_spacing(b.kind),
                // The level's marker width, shared by every item at that level so their
                // bodies line up. A continuation block has already paid it at `left`.
                hang: if b.list.is_some() { level } else { 0.0 },
                actions: &p.actions,
            },
            &mut Out { ops: &mut ops, hots: &mut hots, sel: &mut sel, hyphens: &mut breaks, wide: &mut wide, table_headers: &mut table_headers, table_spans: &mut table_spans, note_spans: &mut note_spans, math_texts: &mut math_texts },
            y,
        );
    }

    // The apparatus, after the last block: a rule across the measure, then each note
    // under its number. Everything below goes through the same `layout_block` the
    // prose uses, so a note with two paragraphs wraps and justifies exactly as they
    // would on the page -- the only things it changes are the size and the leading the
    // caller hands in.
    let mut apparatus = doc.footnotes.iter().zip(note_units).peekable();
    if apparatus.peek().is_some() {
        y += theme.note_space(true);
        ops.push(Op::Line {
            x0: left * k,
            y0: y * k,
            x1: (left + column) * k,
            y1: y * k,
            thickness: theme.base * 0.05 * k,
            color: ColorRole::Muted,
        });
        for (n, (note, units)) in apparatus.enumerate() {
            // The first note sits below the rule; after that one note is separated
            // from the last by less than two paragraphs of prose are.
            y += if n == 0 { theme.base * 0.5 } else { theme.note_space(false) };
            let note_start = y;
            // Where a citation of this note has to land: the top of its first line,
            // which is what `activate` brings into view.
            note_tops.push(y);
            for (i, (nb, p)) in note.blocks.iter().zip(units).enumerate() {
                if i > 0 {
                    y += theme.note_space(false);
                }
                let (hyphens, hyphen_width) =
                    hyphenation_for(&p.text, &styles, p.base, hyphenator, font, nb.kind == BlockKind::Code);
                let size = theme.note_body_size(nb.kind);
                y = layout_block(
                    font,
                    &ctx,
                    &Blk {
                        b: nb,
                        text: &p.text,
                        spans: &p.spans,
                        base: p.base,
                        left,
                        column,
                        table: p.table.as_ref(),
                        hyphenation: Hyphenation { points: &hyphens, width: hyphen_width },
                        size,
                        leading: theme.note_leading(),
                        hang: note_hang,
                        actions: &p.actions,
                    },
                    &mut Out { ops: &mut ops, hots: &mut hots, sel: &mut sel, hyphens: &mut breaks, wide: &mut wide, table_headers: &mut table_headers, table_spans: &mut table_spans, note_spans: &mut note_spans, math_texts: &mut math_texts },
                    y,
                );
            }
            note_spans.push(NoteSpan { number: note.number.to_string(), y: note_start, height: y - note_start });
        }
    }

    // A grid paints one column at a time, so its lines reach the index column by
    // column: the second line of a tall cell before the first line of the cell beside
    // it. A drag crosses the page by line, and left to right within one, so the index
    // is put back into the order the reader sees. The sort is stable, which is what
    // keeps a row's cells in their column order.
    sel.sort_by(|a, b| {
        a.y.total_cmp(&b.y).then_with(|| {
            a.xs.first().copied().unwrap_or(0.0).total_cmp(&b.xs.first().copied().unwrap_or(0.0))
        })
    });

    Page {
        ops,
        height: y + theme.base * 2.0,
        column,
        left,
        hotspots: hots,
        wide_regions: wide,
        note_tops,
        anchor_tops,
        sel,
        hyphens: breaks,
        table_headers,
        table_spans,
        note_spans,
        math_texts,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DPI: f32 = 96.0;

    /// The page poll's whole decision, taken without a file in the way.
    #[test]
    fn a_page_is_only_reread_when_the_file_behind_it_has_changed() {
        let aged = |secs: u64| {
            Stamp { len: 10, written: Some(std::time::UNIX_EPOCH + std::time::Duration::from_secs(secs)) }
        };
        // Same length, same age: the screen is showing the file that is there.
        assert!(!is_written(Some(aged(1)), Some(aged(1))), "an unchanged file is not a save");
        // Either number moving is a write, and a write is a page to take again.
        assert!(is_written(Some(aged(1)), Some(aged(2))));
        assert!(is_written(Some(Stamp { len: 11, ..aged(1) }), Some(Stamp { len: 12, ..aged(1) })));
        // A file that has gone away is not a page to throw at the reader.
        assert!(!is_written(Some(aged(1)), None), "a missing file keeps the last page read");
        // Nothing filed against a page yet -- the start-up read that could not ask, or the
        // sample -- is not the same as a page already up to date.
        assert!(is_written(None, Some(aged(1))), "a page of unknown origin is a page to read");
        assert!(!is_written(None, None), "and there is nothing to do about a page with no file");
        // A share that tells nobody an age still tells a length.
        let blind = |len: u64| Stamp { len, written: None };
        assert!(is_written(Some(blind(10)), Some(blind(11))));
        assert!(!is_written(Some(blind(10)), Some(blind(10))));
    }

    /// The same decision with a real file behind it, which is the only way to check that
    /// what is asked of the drive is answered in the units the comparison uses.
    #[test]
    fn a_file_says_how_long_it_is_and_when_it_was_written() {
        let dir = std::env::temp_dir().join(format!("rubrica-stamp-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("a directory to write a page into");
        let file = dir.join("chapter.md");

        assert_eq!(stamp_of(&file), None, "a page not written yet has nothing to say");
        std::fs::write(&file, "one two").expect("a page to ask about");
        let first = stamp_of(&file).expect("a page that has been written");
        // Bytes, not characters -- which matters the first time a document is written in
        // Chinese and a length in characters would say two lines are the same size.
        assert_eq!(first.len, 7);
        assert!(first.written.is_some(), "a local drive gives an age");

        // Rewritten to exactly the same length. The age is then put there by hand rather
        // than left to the clock: a filesystem with a coarse timestamp makes two writes a
        // microsecond apart the same age, and the test would be about this machine's drive
        // rather than about the comparison.
        std::fs::write(&file, "two one").expect("the same length, the other way round");
        aged_to(&file, 3_600);
        let second = stamp_of(&file).expect("the page again");
        assert_eq!(second.len, first.len, "the length gives no hint of the change");
        assert_ne!(second, first, "and so only the age can say it");
        assert!(is_written(Some(first), Some(second)));

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Set a file's age to a stated number of seconds after the epoch, and say nothing if
    /// that could not be done: the assertion above is the thing under test, and a drive
    /// that will not be told an age fails there rather than here.
    fn aged_to(path: &std::path::Path, secs: u64) {
        let times = std::fs::FileTimes::new()
            .set_modified(std::time::UNIX_EPOCH + std::time::Duration::from_secs(secs));
        if let Ok(h) = std::fs::OpenOptions::new().write(true).open(path) {
            let _ = h.set_times(times);
        }
    }

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

    #[test]
    fn a_line_is_painted_where_a_click_is_asked_of_it() {
        // Two ends of one subtraction: paint lifts the display list by `scroll_dip`, and a
        // pointer's own y is pushed down by the same number to be asked of that list. A
        // sign flipped on either end is a page that shows one paragraph and marks the one
        // above it -- and nothing outside a window would notice, so the round trip is the
        // check.
        let sel = vec![sel_line("first", 0.0, Join::None), sel_line("second", 32.0, Join::None)];
        let up = scroll_dip(20.0, DPI);
        let painted = sel[1].y - up;
        assert!(painted > 0.0 && painted < 800.0, "the scrolled line is still on screen: {painted}");
        let c = caret_at(&sel, 0.0, painted + up + 1.0);
        assert_eq!(c.line, 1, "a pointer at the painted place reaches the painted line");
    }

    #[test]
    fn pdf_page_starts_break_at_lines_and_keep_a_heading_with_following_text() {
        let line = |y, h| SelLine {
            source: None,
            y,
            h,
            join: Join::None,
            chars: Vec::new(),
            copies: Vec::new(),
            xs: Vec::new(),
            ends: Vec::new(),
        };
        let page = Page {
            ops: Vec::new(),
            height: 1_000.0,
            column: 400.0,
            left: 0.0,
            hotspots: Vec::new(),
            note_tops: Vec::new(),
            anchor_tops: vec![780.0],
            sel: vec![line(700.0, 30.0), line(780.0, 30.0), line(820.0, 30.0), line(900.0, 30.0)],
            hyphens: HyphenCount::default(),
            wide_regions: Vec::new(),
            table_headers: Vec::new(),
            table_spans: Vec::new(),
            note_spans: Vec::new(),
            math_texts: Vec::new(),
        };
        let starts = pdf_page_starts(&page, 800.0);
        assert_eq!(starts, vec![0.0, 780.0]);
    }

    #[test]
    fn pdf_rtl_glyph_positions_follow_visual_edges() {
        let advances = [10.0, 20.0];
        assert_eq!(pdf_glyph_x(100.0, &advances, 0, 0), 100.0);
        assert_eq!(pdf_glyph_x(100.0, &advances, 0, 1), 110.0);
        assert_eq!(pdf_glyph_x(100.0, &advances, 1, 0), 120.0);
        assert_eq!(pdf_glyph_x(100.0, &advances, 1, 1), 100.0);
    }

    #[test]
    fn pdf_glyph_clusters_keep_ligature_text_selectable() {
        assert_eq!(unicode_for_glyph("fi", &[0, 0], 1, 0).as_deref(), Some("fi"));
        assert_eq!(unicode_for_glyph("a😀b", &[0, 1, 1, 3], 3, 1).as_deref(), Some("😀"));
        assert_eq!(unicode_for_glyph("abc", &[0, 1, 2], 3, 2).as_deref(), Some("c"));
    }

    #[test]
    fn wide_pan_is_bounded_and_only_moves_its_region() {
        let region = WideRegion {
            kind: WideKind::Table,
            x: 100.0,
            y: 200.0,
            w: 80.0,
            h: 40.0,
            content_w: 180.0,
        };
        assert_eq!(pan_offset(180.0, 80.0, -20.0), 0.0);
        assert_eq!(pan_offset(180.0, 80.0, 20.0), 20.0);
        assert_eq!(pan_offset(180.0, 80.0, 200.0), 100.0);
        assert_eq!(region_shift(&region, 30.0, 110.0, 210.0), -30.0);
        assert_eq!(region_shift(&region, 30.0, 90.0, 210.0), 0.0);
        assert_eq!(region_shift(&region, 30.0, 110.0, 100.0), 0.0);
    }

    /// window: where it goes, and what it says when it is there.
    #[test]
    fn the_bar_leaves_the_thumb_outside_it_and_the_count_inside_it() {
        let (px, py, pw, ph) = find_panel(1080.0);
        // The reader has to be able to grip the scroll with the bar up -- it appeared
        // because they pressed a key, not because they asked for something to lean on.
        let (tx, _, track, _) = thumb_rect(2400.0, 1080.0, 800.0, 0.0, DPI).unwrap();
        assert!(px + pw <= tx, "the bar is over the thumb's track: {} > {}", px + pw, tx);
        assert!(!in_find_panel(tx + track / 2.0, py + ph / 2.0, 1080.0), "and the grip is still a press");
        // The box sits inside the panel, with the count's room left at its right.
        let k = scale_of(DPI);
        let (ex, ey, ew, eh) = find_edit(1080.0, DPI);
        assert!(ex as f32 >= px * k - 1.0, "the box left the bar: {ex}");
        assert!((ex + ew) as f32 <= (px + pw - FIND_COUNT) * k + 1.0, "the box took the count's room");
        assert!(ey as f32 >= py * k - 1.0 && (ey + eh) as f32 <= (py + ph) * k + 1.0);
    }

    #[test]
    fn a_window_narrower_than_the_bar_narrows_it_rather_than_losing_it() {
        for w in [180.0, 320.0, 1080.0] {
            let (px, _, pw, _) = find_panel(w);
            assert!(px >= 0.0 && px + pw <= w, "the bar left the window at {w}px: {px}+{pw}");
            let (ex, _, ew, _) = find_edit(w, DPI);
            assert!(ew > 40, "and left nothing to type into at {w}px");
            assert!((ex + ew) as f32 <= w * scale_of(DPI) + 1.0, "the box went over the edge at {w}px");
        }
    }

    #[test]
    fn a_press_on_the_bar_does_not_mark_the_page_under_it() {
        let (px, py, pw, ph) = find_panel(1080.0);
        assert!(in_find_panel(px + 4.0, py + 2.0, 1080.0));
        assert!(in_find_panel(px + pw - 4.0, py + ph - 2.0, 1080.0));
        assert!(!in_find_panel(px + pw / 2.0, py + ph + 4.0, 1080.0), "a line below is the reader's own");
        assert!(!in_find_panel(px - 4.0, py + 2.0, 1080.0));
    }

    #[test]
    fn the_count_names_one_hit_without_a_place_and_many_with_one() {
        assert_eq!(find_count(0, 0), "no match");
        assert_eq!(find_count(0, 1), "1 match", "one hit has no position to be at");
        assert_eq!(find_count(0, 9), "1 of 9");
        assert_eq!(find_count(8, 9), "9 of 9");
    }

    fn url(target: &str, range: std::ops::Range<usize>) -> Action {
        Action { range, kind: ActionKind::Url(target.to_string()) }
    }

    /// Only the schemes that mean "read this" are worth a pointer. The rest are a
    /// document asking the reader's machine to run or reach something, and an
    /// address with a control character in it is not an address.
    #[test]
    fn only_an_address_that_means_read_this_is_openable() {
        for ok in ["https://example.org/a?b=1#c", "http://example.org", "HTTPS://EXAMPLE.ORG/x", "mailto:rd@example.org"] {
            assert!(openable(ok), "{ok} should open");
        }
        for no in [
            "other.md",
            "./other.md",
            "#section",
            "file:///C:/Windows/system.ini",
            r#"\\nas\share\document.md"#,
            "javascript:alert(1)",
            "cmd:/c calc",
            "https:/example.org",
            "mailto:",
            "https://exa\0mple.org",
            "https://exa\nmple.org",
        ] {
            assert!(!openable(no), "{no} should not open");
        }
    }

    #[test]
    fn a_target_reaches_only_the_ink_that_belongs_to_it() {
        // "plain [link] plain": the target is the middle node's text alone.
        let actions = [url("https://e/x", 6..10)];
        let mut hit: Vec<Vec<(Pt, Pt)>> = vec![Vec::new(); actions.len()];
        merge_hot(&mut hit, &actions, &(0..6), 0.0, 50.0);
        merge_hot(&mut hit, &actions, &(6..10), 50.0, 90.0);
        merge_hot(&mut hit, &actions, &(10..16), 90.0, 140.0);
        assert_eq!(hit, vec![vec![(50.0, 90.0)]], "the rectangles reached outside the link");
    }

    #[test]
    fn a_target_split_into_nodes_still_covers_the_gap_between_them() {
        // A link's own text arrives in pieces -- `guide`, ` `, `here` -- and the space
        // between the pieces is part of the word the reader clicked.
        let actions = [url("https://e/x", 6..14)];
        let mut hit: Vec<Vec<(Pt, Pt)>> = vec![Vec::new(); actions.len()];
        for (range, x0, x1) in
            [(6..11usize, 50.0f32, 80.0f32), (11..12, 80.0, 88.0), (12..14, 88.0, 104.0)]
        {
            merge_hot(&mut hit, &actions, &range, x0, x1);
        }
        assert_eq!(hit, vec![vec![(50.0, 104.0)]]);
    }

    #[test]
    fn two_targets_on_one_line_keep_their_own_widths() {
        let actions = [url("https://e/a", 0..4), url("https://e/b", 6..10)];
        let mut hit: Vec<Vec<(Pt, Pt)>> = vec![Vec::new(); actions.len()];
        merge_hot(&mut hit, &actions, &(0..4), 0.0, 40.0);
        merge_hot(&mut hit, &actions, &(6..10), 70.0, 110.0);
        assert_eq!(hit, vec![vec![(0.0, 40.0)], vec![(70.0, 110.0)]]);
    }

    #[test]
    fn a_target_with_no_width_of_its_own_is_not_a_target() {
        let actions = [url("https://e/a", 0..4)];
        let mut hit: Vec<Vec<(Pt, Pt)>> = vec![Vec::new(); actions.len()];
        merge_hot(&mut hit, &actions, &(0..4), 30.0, 30.0);
        assert_eq!(hit, vec![Vec::<(Pt, Pt)>::new()], "an empty rectangle would swallow a click at its edge");
    }

    #[test]
    fn ink_is_only_marked_as_a_link_where_it_leads_somewhere() {
        let actions = [url("https://e/a", 6..10), url("relative.md", 20..26)];
        assert!(reaches(&actions, &(6..10)));
        assert!(!reaches(&actions, &(20..26)), "a target the reader cannot follow must not be coloured");
        // The object character inside a linked figure is the link's whole text.
        assert!(reaches(&actions, &(8..9)), "part of a target's ink is still its ink");
    }

    /// One character's advance in the lines the selection tests build.
    const CHAR: f32 = 8.0;

    /// A laid-out line to drag over: monospace, one `CHAR` wide per boundary, so the
    /// arithmetic a test writes is the arithmetic it checks.
    fn sel_line(text: &str, y: f32, join: Join) -> SelLine {
        let chars: Vec<char> = text.chars().collect();
        let xs: Vec<_> = (0..=chars.len()).map(|i| i as f32 * CHAR).collect();
        let ends = xs[1..].to_vec();
        SelLine { source: None, y, h: CHAR * 1.5, join, chars, copies: Vec::new(), xs, ends }
    }

    /// A line whose character at `squeezed_at` was given no width by the break that
    /// swallowed it, which is what a wrapped line's own inter-word space looks like.
    fn squeezed(text: &str, y: f32, squeezed_at: usize) -> SelLine {
        let chars: Vec<char> = text.chars().collect();
        let xs: Vec<_> = (0..=chars.len())
            .map(|i| (i - (i > squeezed_at) as usize) as f32 * CHAR)
            .collect();
        let ends = xs[1..].to_vec();
        SelLine { source: None, y, h: CHAR * 1.5, join: Join::None, chars, copies: Vec::new(), xs, ends }
    }

    #[test]
    fn a_line_bringing_its_own_break_does_not_ask_for_another() {
        // A `<br>` inside a cell: the break is part of that cell's own text, so the
        // index already carries it, and the separator the row records on top would copy
        // a blank line where the page shows a single one.
        let mut carried = squeezed("\n中间再加一段", 35.0, 0);
        carried.join = Join::Tab;
        let sel = [sel_line("构件", 0.0, Join::Newline), carried, sel_line("混排", 70.0, Join::Tab)];
        let all = Selection {
            from: Caret { line: 0, ch: 0 },
            to: Caret { line: 2, ch: 2 },
        };
        assert_eq!(selection_text(&sel, all), "构件\n中间再加一段\t混排");
    }

    /// The pointer arrives in device pixels, so an index whose boundaries are still in
    /// points answers a third of the way in from the margin at 144 dpi: a click lands on
    /// the wrong word, and the band drawn for the selection lies beside the text it is
    /// meant to be under. `y` and `h` were always converted; this is the other half.
    #[test]
    fn a_lines_boundaries_are_stated_in_the_units_a_pointer_arrives_in() {
        let segs = [(0..2, 10.0f32, 30.0f32)];
        let marks = [(0usize, 10.0f32, 20.0f32), (1usize, 20.0f32, 30.0f32)];
        let mut consumed = 0usize;
        let band = Band { top: 4.0, h: 12.0, k: 1.5, join: Join::None };
        let l = mark_line("ab", band, &segs, &marks, &mut consumed).expect("two characters");
        assert_eq!((l.y, l.h), (6.0, 18.0), "the line's own box is in device pixels");
        assert_eq!(l.xs, vec![15.0, 30.0, 45.0], "and so are its boundaries");
        // Read back the way a click reads it: the pointer over the second character's
        // ink has to name that character, not the one a third of the line before it.
        assert_eq!(caret_at(&[l], 32.0, 8.0).ch, 1);
    }

    #[test]
    fn a_drag_picks_the_boundary_it_landed_nearest() {
        let sel = [sel_line("abcdefgh", 0.0, Join::None)];
        // The ink of a glyph starts a point or two inside its slot, so the middle of a
        // character belongs to the character on its right.
        assert_eq!(caret_at(&sel, 3.0 * CHAR + 2.0, 1.0).ch, 3);
        assert_eq!(caret_at(&sel, 3.0 * CHAR + 6.0, 1.0).ch, 4);
    }

    #[test]
    fn a_drag_thrown_past_the_page_clamps_to_its_ends() {
        let sel = [sel_line("first", 0.0, Join::None), sel_line("second", 100.0, Join::Blank)];
        let top = caret_at(&sel, -500.0, -500.0);
        assert_eq!((top.line, top.ch), (0, 0), "a fling past the top means from the start");
        let bottom = caret_at(&sel, 5000.0, 9000.0);
        assert_eq!((bottom.line, bottom.ch), (1, 6), "…and past the bottom, to the end");
        assert_eq!(caret_at(&[], 10.0, 10.0), Caret { line: 0, ch: 0 }, "an empty page has one place");
    }

    #[test]
    fn a_selection_reads_back_the_separators_between_its_pieces() {
        let sel = [
            sel_line("one", 0.0, Join::None),
            sel_line("two", 100.0, Join::Blank),
            sel_line("three", 200.0, Join::Newline),
            sel_line("four", 300.0, Join::Tab),
        ];
        let all = Selection {
            from: Caret { line: 0, ch: 0 },
            to: Caret { line: 3, ch: 4 },
        };
        assert_eq!(selection_text(&sel, all), "one\n\ntwo\nthree\tfour");
    }

    #[test]
    fn dragging_upwards_copies_what_dragging_downwards_copies() {
        let sel = [sel_line("alpha", 0.0, Join::None), sel_line("beta", 100.0, Join::Blank)];
        let down = Selection { from: Caret { line: 0, ch: 1 }, to: Caret { line: 1, ch: 3 } };
        let up = Selection { from: down.to, to: down.from };
        assert_eq!(selection_text(&sel, up), selection_text(&sel, down));
        assert_eq!(selection_rects(&sel, up), selection_rects(&sel, down));
    }

    #[test]
    fn a_click_selects_nothing_and_copies_nothing() {
        let sel = [sel_line("alpha", 0.0, Join::None)];
        let here = Selection { from: Caret { line: 0, ch: 2 }, to: Caret { line: 0, ch: 2 } };
        assert!(selection_rects(&sel, here).is_empty());
        assert_eq!(selection_text(&sel, here), "");
        // The same at either end of the page, where an empty `to` would otherwise read
        // every line after the press.
        let start = Selection { from: Caret { line: 0, ch: 0 }, to: Caret { line: 0, ch: 0 } };
        assert_eq!(selection_text(&sel, start), "");
    }

    #[test]
    fn a_squeezed_space_copies_but_never_shows() {
        // `a b` with the space given no width by the line break: the character is in
        // the source, so a copy has to keep it, and it is not on the page, so a
        // highlight cannot draw it.
        let sel = [squeezed("ab cd", 0.0, 2)];
        assert_eq!(sel[0].xs[2], sel[0].xs[3], "the squeezed boundary must have no width");
        let space = Selection { from: Caret { line: 0, ch: 2 }, to: Caret { line: 0, ch: 3 } };
        assert!(selection_rects(&sel, space).is_empty(), "an invisible character has no ink to lie under");
        assert_eq!(selection_text(&sel, space), " ");
        let all = Selection { from: Caret { line: 0, ch: 0 }, to: Caret { line: 0, ch: 5 } };
        assert_eq!(selection_text(&sel, all), "ab cd");
    }

    #[test]
    fn a_double_click_takes_the_whole_word_and_the_hyphen_inside_it() {
        let sel = [sel_line("well-formed text", 0.0, Join::None)];
        let inside = word_at(&sel, Caret { line: 0, ch: 3 });
        assert_eq!((inside.from.ch, inside.to.ch), (0, 11), "the hyphen is inside the word");
        assert_eq!(selection_text(&sel, inside), "well-formed");
    }

    #[test]
    fn a_double_click_in_a_gap_belongs_to_the_word_on_its_right() {
        let sel = [sel_line("one  two", 0.0, Join::None)];
        let w = word_at(&sel, Caret { line: 0, ch: 4 });
        assert_eq!((w.from.ch, w.to.ch), (5, 8));
        assert_eq!(selection_text(&sel, w), "two");
        // …unless there is no word on the right, which is the end of a line.
        let end = word_at(&sel, Caret { line: 0, ch: 8 });
        assert_eq!((end.from.ch, end.to.ch), (5, 8));
    }

    #[test]
    fn a_double_click_on_han_text_stops_at_one_character() {
        let sel = [sel_line("排版引擎", 0.0, Join::None)];
        let w = word_at(&sel, Caret { line: 0, ch: 2 });
        assert_eq!((w.from.ch, w.to.ch), (1, 2), "no word boundary inside them is a click's to find");
        assert_eq!(selection_text(&sel, w), "版");
        // Mixed with Latin, each script keeps to its own extent.
        let sel = [sel_line("版text版", 0.0, Join::None)];
        let latin = word_at(&sel, Caret { line: 0, ch: 3 });
        assert_eq!(selection_text(&sel, latin), "text");
    }

    #[test]
    fn a_double_click_on_punctuation_takes_only_it() {
        let sel = [sel_line("end. more", 0.0, Join::None)];
        let w = word_at(&sel, Caret { line: 0, ch: 4 });
        assert_eq!(selection_text(&sel, w), ".", "a mark belongs to neither neighbour");
        // A click in the gap between two words goes to the word on its right...
        let gap = word_at(&sel, Caret { line: 0, ch: 5 });
        assert_eq!(selection_text(&sel, gap), "more");
        // ...and to the one on its left when there is no word left to reach, skipping
        // the punctuation trailing off the end rather than selecting it.
        let sel = [sel_line("more. ", 0.0, Join::None)];
        assert_eq!(selection_text(&sel, word_at(&sel, Caret { line: 0, ch: 6 })), "more");
    }

    #[test]
    fn a_headings_words_become_its_address() {
        // Each case is one an author has to be able to guess, because guessing is all
        // writing a fragment link involves.
        for (heading, want) in [
            ("Global Breaking", "global-breaking"),
            ("Using `cargo run`", "using-cargo-run"),
            // The mark goes and the space it stood in stays: a heading's words are still
            // its words with the punctuation taken out, not run together.
            ("Hello, World!", "hello-world"),
            ("em–dash only", "emdash-only"),
            ("  padded  heading  ", "padded-heading"),
            ("snake_case and a – dash", "snake_case-and-a-dash"),
            ("中西文 混排", "中西文-混排"),
            ("$x^2$ display", "x2-display"),
        ] {
            assert_eq!(slug(heading), want, "{heading:?}");
        }
        // What an author writes is the heading's words, not its address, and both sides
        // of a link go through the same rule, so either spelling arrives.
        assert_eq!(slug("Global Breaking"), slug("global  breaking"));
    }

    #[test]
    fn editor_arguments_expand_paths_and_positions_without_shell_splitting() {
        let path = Path::new("C:/books/a book.md");
        assert_eq!(editor_arguments("", path, 3, 7), vec!["C:/books/a book.md".to_string()]);
        assert_eq!(
            editor_arguments("--goto {file}:{line}:{column}", path, 3, 7),
            vec!["--goto".to_string(), "C:/books/a book.md:3:7".to_string()]
        );
        assert_eq!(
            editor_arguments("\"C:/Program Files/Code.exe\" --reuse-window {file}", path, 3, 7),
            vec!["C:/Program Files/Code.exe".to_string(), "--reuse-window".to_string(), "C:/books/a book.md".to_string()]
        );
    }

    #[test]
    fn a_fragment_link_becomes_a_jump_to_its_heading() {
        let theme = Theme::default();
        let styles: Vec<AppStyle> = Vec::new();
        let math = crate::math::MathStore::new();
        let empty = HashMap::new();
        let mut anchors = HashMap::new();
        anchors.insert(slug("Global Breaking"), 0usize);
        let ctx = Ctx {
            theme: &theme,
            styles: &styles,
            math: &math,
            notes: &empty,
            anchors: &anchors,
            base: None,
            k: 1.0,
            wide_limit: 800.0,
        };
        let url = |u: &str| ActionKind::Url(u.to_string());
        assert_eq!(hot_kind(&url("#global-breaking"), &ctx), Some(HotKind::Heading(0)));
        // The heading's own words in place of an address: the same rule reads them.
        assert_eq!(hot_kind(&url("#Global Breaking"), &ctx), Some(HotKind::Heading(0)));
        // A page with no such heading gives no target to click, and a path with no file
        // behind it is nowhere the reader can go.
        assert_eq!(hot_kind(&url("#elsewhere"), &ctx), None);
        assert_eq!(hot_kind(&url("../other.md"), &ctx), None);
        assert_eq!(
            hot_kind(&url("https://example.com"), &ctx),
            Some(HotKind::Url("https://example.com".into()))
        );
    }

    #[test]
    fn a_link_to_a_file_beside_the_document_opens_that_file() {
        let dir = std::env::temp_dir().join(format!("rubrica-links-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        std::fs::write(dir.join("other.md"), "# Other\n").expect("other.md");
        std::fs::write(dir.join("a chapter.markdown"), "# Chapter\n").expect("chapter");
        std::fs::write(dir.join("data.csv"), "a,b\n").expect("data.csv");

        let found = document_link(Some(&dir), "other.md");
        let escaped = document_link(Some(&dir), "a%20chapter.markdown");
        let with_fragment = document_link(Some(&dir), "other.md#somewhere");
        let elsewhere = document_link(Some(&dir), "missing.md");
        let pdf = document_link(Some(&dir), "data.csv");
        let scheme = document_link(Some(&dir), "mailto:someone@example.com");
        let web = document_link(Some(&dir), "https://example.com/x.md");
        let nothing = document_link(Some(&dir), "#anchor");
        // And the target a click acts on is the same answer, not one the layout pass
        // reaches by a different road.
        let theme = Theme::default();
        let styles: Vec<AppStyle> = Vec::new();
        let math = crate::math::MathStore::new();
        let empty = HashMap::new();
        let ctx = Ctx {
            theme: &theme,
            styles: &styles,
            math: &math,
            notes: &empty,
            anchors: &empty,
            base: Some(&dir),
            k: 1.0,
            wide_limit: 800.0,
        };
        let clicked = hot_kind(&ActionKind::Url("other.md".into()), &ctx);
        let _ = std::fs::remove_dir_all(&dir);

        assert_eq!(clicked, Some(HotKind::Document(DocumentTarget {
            path: dir.join("other.md"), fragment: None,
        })));
        assert_eq!(found, Some(dir.join("other.md")));
        // An author names a space with `%20` because a link cannot carry the space
        // itself; the file on disk has the space.
        assert_eq!(escaped, Some(dir.join("a chapter.markdown")));
        // Resolution identifies the file; the hotspot retains the fragment separately.
        assert_eq!(with_fragment, Some(dir.join("other.md")));
        assert_eq!(elsewhere, None, "a file that is not there is not a target");
        assert_eq!(pdf, None, "not a document this reader reads");
        assert_eq!(scheme, None, "an address in another protocol is not a path");
        assert_eq!(web, None, "a page on the web is opened by the browser");
        assert_eq!(nothing, None, "a bare fragment is this file's own address");
    }

    #[test]
    fn the_arrows_walk_the_caret_over_the_page() {
        let sel = [sel_line("one", 0.0, Join::None), sel_line("two", 20.0, Join::Newline)];
        let at = |line, ch| Caret { line, ch };
        assert_eq!(next_caret(&sel, at(0, 0), Motion::Right), at(0, 1));
        // The break is one position to cross, not two, so a walk right reads through the
        // page without a beat at either edge.
        assert_eq!(next_caret(&sel, at(0, 3), Motion::Right), at(1, 0));
        assert_eq!(next_caret(&sel, at(1, 0), Motion::Left), at(0, 3));
        assert_eq!(next_caret(&sel, at(0, 2), Motion::Down), at(1, 2));
        assert_eq!(next_caret(&sel, at(0, 1), Motion::End), at(0, 3));
        assert_eq!(next_caret(&sel, at(1, 2), Motion::Home), at(1, 0));
        // Past either end of the index the caret stays, rather than pointing at a line
        // that was never drawn.
        assert_eq!(next_caret(&sel, at(0, 0), Motion::Left), at(0, 0));
        assert_eq!(next_caret(&sel, at(1, 3), Motion::Right), at(1, 3));
        assert_eq!(next_caret(&sel, at(1, 3), Motion::Down), at(1, 3));
        assert_eq!(next_caret(&sel, at(1, 1), Motion::DocStart), at(0, 0));
        assert_eq!(next_caret(&sel, at(0, 0), Motion::DocEnd), at(1, 3));
        // A column the shorter line never had is clamped to the end of the one reached,
        // so a run of `Down` cannot leave the caret past the text it can be seen in.
        let short = [sel_line("one", 0.0, Join::None), sel_line("t", 20.0, Join::Newline)];
        assert_eq!(next_caret(&short, at(0, 3), Motion::Down), at(1, 1));
    }

    #[test]
    fn a_word_motion_takes_the_whole_word_and_the_space_ahead_of_it() {
        let sel = [sel_line("hello world  again", 0.0, Join::None)];
        let at = |ch| Caret { line: 0, ch };
        assert_eq!(next_caret(&sel, at(0), Motion::WordRight), at(5));
        // From inside the space between two words the next motion lands on the end of
        // the word after it, not on the start -- one press, one word.
        assert_eq!(next_caret(&sel, at(5), Motion::WordRight), at(11));
        assert_eq!(next_caret(&sel, at(16), Motion::WordRight), at(18), "the last word's own end");
        assert_eq!(next_caret(&sel, at(18), Motion::WordRight), at(18), "and no further");
        assert_eq!(next_caret(&sel, at(18), Motion::WordLeft), at(13));
        assert_eq!(next_caret(&sel, at(13), Motion::WordLeft), at(6));
        assert_eq!(next_caret(&sel, at(6), Motion::WordLeft), at(0));
        assert_eq!(next_caret(&sel, at(0), Motion::WordLeft), at(0));
        // Ideographs are words of one, so a `Ctrl`+arrow steps them a character at a
        // time -- the same extent a double-click gives.
        let han = [sel_line("排版引擎", 0.0, Join::None)];
        assert_eq!(next_caret(&han, Caret { line: 0, ch: 1 }, Motion::WordRight), Caret { line: 0, ch: 2 });
    }

    #[test]
    fn a_marking_grown_from_the_caret_reads_back_the_same() {
        // What `Shift`+arrows leave behind is an ordinary selection, so it has to copy
        // exactly what the same range dragged out would.
        let sel = [sel_line("one", 0.0, Join::None), sel_line("two", 20.0, Join::Newline)];
        let from = Caret { line: 0, ch: 1 };
        let mut to = from;
        for m in [Motion::Right, Motion::Right, Motion::Right, Motion::Down, Motion::Right, Motion::Right] {
            to = next_caret(&sel, to, m);
        }
        assert_eq!((to.line, to.ch), (1, 2));
        assert_eq!(selection_text(&sel, Selection { from, to }), "ne\ntw");
        // And drawn, which is the reader's half of the same agreement.
        assert_eq!(selection_rects(&sel, Selection { from, to }).len(), 2);
    }

    #[test]
    fn the_caret_bar_stands_on_a_character_edge() {
        let sel = [sel_line("abc", 10.0, Join::None)];
        let (x, y, w, h) = caret_rect(&sel, Caret { line: 0, ch: 1 }).unwrap();
        assert_eq!((x, y, w, h), (CHAR, 10.0, 1.0, sel[0].h));
        // Past the last character the bar stands at the line's right edge, where the text
        // it was following ends; off the page there is no bar at all.
        assert_eq!(caret_rect(&sel, Caret { line: 0, ch: 9 }).map(|r| r.0), Some(3.0 * CHAR));
        assert_eq!(caret_rect(&sel, Caret { line: 1, ch: 0 }), None);
    }

    /// The rows a state gives, without the gaps that only divide them and without the
    /// rows one level down, so a position in this list is a position in the top menu.
    fn rows(s: &MenuState) -> Vec<(Command, bool, bool)> {
        menu_items(s)
            .into_iter()
            .filter_map(|r| match r {
                MenuRow::Gap => None,
                MenuRow::Sub { .. } => None,
                MenuRow::Row { cmd, enabled, checked, .. } => Some((cmd, enabled, checked)),
            })
            .collect()
    }

    /// Every row a state gives, at any depth. A radio group that moved one level down
    /// still owes the same promise about which of its rows is the one in use, so the
    /// promise is counted over the whole tree rather than over the top menu.
    fn all_rows(s: &MenuState) -> Vec<(Command, bool, bool)> {
        fn walk(rows: Vec<MenuRow>, out: &mut Vec<(Command, bool, bool)>) {
            for r in rows {
                match r {
                    MenuRow::Row { cmd, enabled, checked, .. } => out.push((cmd, enabled, checked)),
                    MenuRow::Sub { items, .. } => walk(items, out),
                    MenuRow::Gap => {}
                }
            }
        }
        let mut out = Vec::new();
        walk(menu_items(s), &mut out);
        out
    }

    /// The contents of the `Contents` submenu, with their labels -- which is the one thing
    /// about an outline row the rest of these helpers throw away.
    ///
    /// Found by name rather than as the first group on the menu, because the menu has
    /// several now and `Typography` sits beside it whether or not the page has headings.
    fn contents(s: &MenuState) -> Vec<(Command, String)> {
        menu_items(s)
            .into_iter()
            .find_map(|r| match r {
                MenuRow::Sub { label: "Contents", items } => Some(items),
                _ => None,
            })
            .unwrap_or_default()
            .into_iter()
            .map(|r| match r {
                MenuRow::Row { cmd, label, .. } => (cmd, label),
                _ => unreachable!("the outline holds nothing but rows"),
            })
            .collect()
    }

    /// A heading list, as [`View`] would gather it from a document: level and words.
    fn outline_of(headings: &[(u8, &str)]) -> Vec<Outline> {
        headings
            .iter()
            .map(|(level, label)| Outline { level: *level, label: (*label).to_string() })
            .collect()
    }

    /// A menu's whole state, written out because the fields are the questions the menu
    /// asks: is anything marked, is there an address here, who is deciding the palette,
    /// did this page come off a disk, is there any text at all.
    fn state(selected: bool, link: Option<&str>, dark: Option<bool>, from_file: bool, text: bool) -> MenuState {
        MenuState {
            keep_line_breaks: false,
            line_break_override: None,
            // A page with no road behind it, which is what a menu asked about the sample
            // after a fresh start reports.
            can_back: false,
            can_forward: false,
            link: link.map(str::to_string),
            selected,
            dark,
            from_file,
            text,
            face: 0,
            offered: vec![true; TextFace::ALL.len()],
            measure: Measure::DESIGN,
            headings: Vec::new(),
            ..MenuState::default()
        }
    }

    #[test]
    fn the_menu_says_true_things_about_the_moment() {
        let empty = state(false, None, None, false, true);
        let got = rows(&empty);
        // Nothing marked, so the copy is dimmed rather than promising an empty clipboard.
        // The two steps come first, so the text's own rows start one group lower.
        assert_eq!(got[0], (Command::GoBack, false, false));
        assert_eq!(got[1], (Command::GoForward, false, false));
        assert_eq!(got[2], (Command::Copy, false, false));
        // A page with text can have it all taken, even with nothing marked yet.
        assert_eq!(got[3], (Command::SelectAll, true, false));
        assert!(
            !got.iter().any(|(c, _, _)| matches!(c, Command::OpenUrl(_) | Command::CopyUrl(_))),
            "no link under the pointer means no item about one"
        );
        // The document came from nowhere, so there is nothing on disk to read again.
        assert_eq!(got.last().map(|(c, e, _)| (c.clone(), *e)), Some((Command::Reload, false)));
        // The newline groups are counted over the whole tree, because they live in a
        // submenu: a radio group one level down still has to mark exactly one row.
        assert_eq!(
            all_rows(&empty)
                .iter()
                .filter(|(cmd, _, c)| *c && matches!(cmd,
                    Command::Palette(_) | Command::FollowSystem | Command::Face(_) | Command::Measure(_)
                    | Command::DefaultLineBreaks(_) | Command::DocumentLineBreaks(_)))
                .count(),
            5,
            "palette, face, measure, newline default and document override each select one row"
        );
        assert!(got.iter().any(|(c, _, c2)| *c2 && c == &Command::FollowSystem));
        assert!(got.iter().any(|(c, _, c2)| *c2 && c == &Command::Face(0)));
        assert!(got.iter().any(|(c, _, c2)| *c2 && c == &Command::Measure(Measure::DESIGN)));

        assert_eq!(rows(&state(true, None, None, false, true))[2], (Command::Copy, true, false));

        let got = rows(&state(false, None, Some(true), true, true));
        assert!(got.iter().any(|(c, _, on)| *on && c == &Command::Palette(true)));
        assert!(!got.iter().any(|(c, _, on)| *on && c == &Command::FollowSystem));
        assert_eq!(got.last().map(|(c, e, _)| (c.clone(), *e)), Some((Command::Reload, true)));

        assert_eq!(rows(&state(false, None, None, false, false))[3], (Command::SelectAll, false, false));
    }

    #[test]
    fn the_two_steps_are_only_lit_when_they_have_somewhere_to_land() {
        let steps = |back: bool, forward: bool| {
            let mut s = state(false, None, None, false, true);
            s.can_back = back;
            s.can_forward = forward;
            let got = rows(&s);
            (got[0].1, got[1].1)
        };
        assert_eq!(steps(false, false), (false, false), "a first page has no road at all");
        assert_eq!(steps(true, false), (true, false));
        assert_eq!(steps(false, true), (false, true));
        // Both at once is the ordinary case after a step back: the page came from and the
        // page was left are both there, and neither may be dimmed.
        assert_eq!(steps(true, true), (true, true));
        // And they are the first thing on the menu, above the group that starts with the
        // copy -- which is where a reader looking for them expects the list to open.
        assert!(menu_items(&state(false, None, None, false, true)).windows(3).any(|w| {
            matches!(
                (&w[0], &w[1], &w[2]),
                (
                    MenuRow::Row { cmd: Command::GoBack, .. },
                    MenuRow::Row { cmd: Command::GoForward, .. },
                    MenuRow::Gap
                )
            )
        }));
    }

    #[test]
    fn a_face_this_machine_cannot_draw_is_named_and_dimmed() {
        let mut s = state(false, None, None, false, true);
        s.offered = vec![true, false, false, true];
        let at = |got: &[(Command, bool, bool)], i| {
            got.iter().find(|(c, _, _)| c == &Command::Face(i)).cloned().expect("face row")
        };
        let got = rows(&s);
        assert_eq!(at(&got, 0), (Command::Face(0), true, true));
        assert_eq!(at(&got, 1), (Command::Face(1), false, false), "not installed, so not offered");
        assert_eq!(at(&got, 3), (Command::Face(3), true, false));

        // A face can go missing after the page has been set in it -- a font uninstalled
        // while the reader was away. The row that says where they are stays true, and
        // stays clickable, because leaving is the one thing that must still work.
        s.face = 1;
        let got = rows(&s);
        assert_eq!(at(&got, 1), (Command::Face(1), true, true));
    }

    #[test]
    fn the_measure_rows_are_the_whole_ladder_with_one_check_on() {
        let row = |got: &[(Command, bool, bool)], i| {
            got.iter().find(|(c, _, _)| c == &Command::Measure(i)).cloned().expect("measure row")
        };
        let got = rows(&state(false, None, None, false, true));
        // Unlike a face, no rung here can be dimmed: a measure is a number of em, not a
        // family installed somewhere, so nothing about the machine decides it.
        for i in 0..Measure::ALL.len() {
            assert_eq!(row(&got, i), (Command::Measure(i), true, i == Measure::DESIGN));
        }
        // And they are listed narrow first, in the order the ladder is walked by the keys.
        let order: Vec<usize> = got
            .iter()
            .filter_map(|(c, ..)| match c {
                Command::Measure(i) => Some(*i),
                _ => None,
            })
            .collect();
        assert_eq!(order, (0..Measure::ALL.len()).collect::<Vec<_>>());

        let mut s = state(false, None, None, false, true);
        s.measure = 2;
        let got = rows(&s);
        assert_eq!(row(&got, 2), (Command::Measure(2), true, true));
        assert_eq!(row(&got, 1), (Command::Measure(1), true, false), "the design's own width stays a choice");
    }

    #[test]
    fn the_outline_is_the_document_in_the_order_it_was_laid_out() {
        let doc = Document::parse("# One\n\ntext\n\n## Two\n\n### Three\n");
        assert_eq!(
            outline(&doc, 3),
            vec![
                Outline { level: 1, label: "One".into() },
                Outline { level: 2, label: "Two".into() },
                Outline { level: 3, label: "Three".into() },
            ],
            "the prose between them is not part of the outline, and the levels come from the source"
        );
        // Fewer anchors on the page than headings in the source: the outline stops with
        // them, because a row pointing at an anchor that was never laid out is a row that
        // does nothing when clicked.
        assert_eq!(outline(&doc, 1).len(), 1);
        assert!(outline(&doc, 0).is_empty());
        assert_eq!(outline(&doc, 99).len(), 3, "and it never invents a heading to fill in");
        assert!(outline(&Document::parse("only prose"), 4).is_empty());
    }

    #[test]
    fn an_outline_row_carries_its_own_place_in_the_document() {
        let got = outline_rows(&outline_of(&[
            (1, "Chapter"),
            (4, "Section"),
            (6, "Deepest"),
            (3, "Has\ttab"),
        ]));
        // Each row's command is its own position in the list, which is the same index
        // `anchor_tops` is kept in -- so the outline and a table of contents link cannot
        // lead to different places.
        let label = |i: usize| match &got[i] {
            MenuRow::Row { cmd, label, .. } => {
                assert_eq!(*cmd, Command::Heading(i));
                label.clone()
            }
            _ => unreachable!("every outline row is a row"),
        };
        assert_eq!(label(0), "Chapter");
        assert_eq!(label(1), "\u{2003}\u{2003}\u{2003}Section");
        // Deeper than four steps, the indent stops growing: a heading pushed off the right
        // of the menu is no more use than a flat list of them.
        assert_eq!(label(2), "\u{2003}\u{2003}\u{2003}\u{2003}Deepest");
        // A tab in a menu string is not blank -- it starts the accelerator column -- so the
        // one a heading happens to carry is swapped out before it can split its own name.
        assert_eq!(label(3), "\u{2003}\u{2003}Has tab");
    }

    #[test]
    fn a_page_with_headings_offers_its_outline_and_a_page_without_does_not() {
        let s = state(false, None, None, false, true);
        assert!(contents(&s).is_empty(), "nothing to name");
        assert!(
            !menu_items(&s).iter().any(|r| matches!(r, MenuRow::Sub { label: "Contents", .. })),
            "no group offering an empty rectangle"
        );

        let mut s = state(false, None, None, false, true);
        s.headings = outline_of(&[(1, "One"), (2, "Two"), (2, "Three")]);
        assert_eq!(
            contents(&s),
            vec![
                (Command::Heading(0), "One".to_string()),
                (Command::Heading(1), "\u{2003}Two".to_string()),
                (Command::Heading(2), "\u{2003}Three".to_string()),
            ]
        );
        // Three of them, and the top menu is no longer than it was: the outline is one row
        // there, however many headings sit behind it.
        assert_eq!(rows(&s).len(), rows(&state(false, None, None, false, true)).len());
    }

    #[test]
    fn a_row_that_a_key_also_repeats_prints_the_key() {
        let rows = |s: &MenuState| {
            menu_items(s)
                .into_iter()
                .filter_map(|r| match r {
                    MenuRow::Row { cmd, label, .. } => Some((cmd, label.clone())),
                    MenuRow::Gap | MenuRow::Sub { .. } => None,
                })
                .collect::<Vec<_>>()
        };
        let got = rows(&state(true, None, None, true, true));
        for (cmd, key) in [
            (&Command::GoBack, "Back\tAlt+\u{2190}"),
            (&Command::GoForward, "Forward\tAlt+\u{2192}"),
            (&Command::Copy, "Copy\tCtrl+C"),
            (&Command::SelectAll, "Select All\tCtrl+A"),
            (&Command::Find, "Find in Document\tCtrl+F"),
            (&Command::ZoomIn, "Increase Text\tCtrl++"),
            (&Command::ZoomOut, "Decrease Text\tCtrl+-"),
            (&Command::ZoomReset, "Actual Size\tCtrl+0"),
            (&Command::SourceView, "Read Source\tCtrl+3"),
            (&Command::OpenFile, "Open\u{2026}\tCtrl+O"),
            (&Command::Reload, "Reload\tCtrl+R"),
        ] {
            let (_, label) = got.iter().find(|(c, _)| c == cmd).expect("the row the key answers");
            assert_eq!(*label, key, "a hint that is not the key is worse than none");
        }
        // Nothing else may claim one. `Ctrl`+`D` picks whichever palette is not on the
        // screen rather than the row it would be printed on, and the bracket keys step
        // the measure instead of settling on the rung they are next to.
        assert_eq!(got.iter().filter(|(_, l)| l.contains('\t')).count(), 11);
    }

    #[test]
    fn the_measure_ladder_stops_at_its_ends() {
        assert_eq!(step_measure(Measure::DESIGN, -1), 0);
        assert_eq!(step_measure(Measure::DESIGN, 1), 2);
        assert_eq!(step_measure(Measure::DESIGN, 1).min(Measure::ALL.len() - 1), 2);
        // Pressed past either end, the key does nothing rather than wrapping: a reader
        // who has gone as wide as the page gets should not find it suddenly narrow.
        assert_eq!(step_measure(0, -1), 0);
        assert_eq!(step_measure(0, -9), 0);
        assert_eq!(step_measure(2, 1), 2);
        assert_eq!(step_measure(2, 9), 2);
        assert_eq!(step_measure(1, 0), 1);
    }

    /// A page with no file behind it -- the built-in sample -- which is a place the
    /// reader can leave and be asked back to like any other.
    fn at(scroll: Pt) -> Visit {
        Visit { path: None, scroll }
    }

    /// A page that is not on this machine, which is what a document the reader deleted
    /// since leaving it becomes.
    fn absent() -> Visit {
        Visit {
            path: Some(std::env::temp_dir().join("Rubrica absent 4f2a.md")),
            scroll: 9.0,
        }
    }

    #[test]
    fn the_two_steps_walk_the_road_the_reader_took() {
        let mut h = History::default();
        assert!(!h.leads(true) && !h.leads(false), "a first page has no road at all");
        // Read A, follow a link to B part-way down, and leave B on the way to C.
        h.leave(at(0.0));
        h.leave(at(12.0));
        assert_eq!(h.end(true).last(), Some(&at(12.0)), "the last place left is the first one back to");
        // Stepping back hands over the place the reader is standing at, which is what
        // makes the same step forward again a possible thing to ask for.
        assert_eq!(h.back(at(30.0)), Some(at(12.0)));
        assert_eq!(h.end(false).last(), Some(&at(30.0)), "the page just left is where forward goes");
        assert_eq!(h.forward(at(12.0)), Some(at(30.0)), "so the road can be walked down again");
        assert!(!h.leads(false), "with nothing further along it");
        assert_eq!(h.back(at(30.0)), Some(at(12.0)), "and back up it, as often as asked");
    }

    #[test]
    fn a_new_road_takes_away_the_old_one() {
        let mut h = History::default();
        h.leave(at(0.0));
        h.back(at(9.0));
        assert!(h.leads(false), "the page stepped back from is still ahead");
        // The reader then follows a link of their own rather than finishing that walk.
        // Where the old road went next is no longer where they are going.
        h.leave(at(4.0));
        assert!(!h.leads(false), "the rest of it is gone with the new one");
        assert!(h.leads(true), "and what was behind is untouched");
    }

    #[test]
    fn a_page_the_reader_deleted_is_walked_past_not_landed_on() {
        let gone = absent();
        assert!(!gone.readable(), "a file that is not there is not a place");
        assert!(at(0.0).readable(), "and a page with no file to read is always one");
        let mut h = History { past: vec![at(2.0), gone.clone()], ..Default::default() };
        assert_eq!(h.end(true).last(), Some(&gone), "the nearest thing behind is the dead page");
        assert!(h.leads(true), "but the road behind it is still somewhere to go");
        h.prune(true);
        assert_eq!(h.end(true).last(), Some(&at(2.0)), "pruning stops at the first place that lives");
        // An end of the road that is all dead is no road: the step is refused rather than
        // taken into nothing, and the place being stood on is not pushed onto the far side
        // by a step that never happened.
        h.past = vec![gone.clone()];
        assert!(!h.leads(true));
        h.prune(true);
        assert!(h.past.is_empty());
        assert_eq!(h.back(at(0.0)), None);
        assert!(h.future.is_empty());
    }

    #[test]
    fn a_place_survives_a_reflow_because_characters_do_not_move() {
        // Six, six and eight characters, stacked at the height the test helper gives
        // every line, and at one pixel to the point so the numbers below are the
        // geometry they are read as.
        let before = vec![
            sel_line("first!", 0.0, Join::None),
            sel_line("second", 12.0, Join::None),
            sel_line("thirdone", 24.0, Join::None),
        ];
        assert_eq!(anchor_at(&before, 0.0, 1.0), Some(0));
        assert_eq!(anchor_at(&before, 12.0, 1.0), Some(6));
        // A line whose bottom edge is exactly at the top of the window is above it, not
        // in it: that text has been read already.
        assert_eq!(anchor_at(&before, 24.0, 1.0), Some(12));
        assert_eq!(anchor_at(&[], 0.0, 1.0), None);
        assert_eq!(anchor_at(&before, 999.0, 1.0), None, "scrolled past the last line");

        // The same prose after the column has narrowed: the middle line has split in
        // two, and everything below it has moved a line down.
        let after = vec![
            sel_line("first!", 0.0, Join::None),
            sel_line("sec", 12.0, Join::None),
            sel_line("ond", 24.0, Join::None),
            sel_line("thirdone", 36.0, Join::None),
        ];
        let anchor = anchor_at(&before, 24.0, 1.0).expect("a place to keep");
        assert_eq!(scroll_for_anchor(&after, anchor, 1.0), Some(36.0));
        // Following the old offset instead would have parked the reader on the tail of a
        // line that no longer ends where it did, twelve pixels above their own text.
        assert_eq!(scroll_for_anchor(&before, anchor, 1.0), Some(24.0), "nothing moved, nothing shifts");
        assert_eq!(anchor_at(&after, 36.0, 1.0), Some(anchor), "and back again");
        // The top of the document is its top, not the top of its first line of ink.
        assert_eq!(scroll_for_anchor(&after, 0, 1.0), Some(0.0));
        // A layout with nothing left to find the place in leaves the offset alone.
        assert_eq!(scroll_for_anchor(&[], anchor, 1.0), None);
        assert_eq!(scroll_for_anchor(&after, 999, 1.0), None);
        // Twice the scale, the same place: the offset is in points and the lines are not.
        assert_eq!(scroll_for_anchor(&after, anchor, 2.0), Some(18.0));
    }

    #[test]
    fn a_menu_opened_over_a_link_carries_its_address() {
        let s = MenuState {
            keep_line_breaks: false,
            line_break_override: None,
            can_back: false,
            can_forward: false,
            link: Some("https://example.com/x".into()),
            selected: false,
            dark: None,
            from_file: false,
            text: true,
            face: 0,
            offered: vec![true; TextFace::ALL.len()],
            measure: Measure::DESIGN,
            headings: Vec::new(),
            ..MenuState::default()
        };
        let got = rows(&s);
        // Two items, in this order: follow it, and take only the address. Both enabled,
        // because the pointer being there is the whole condition.
        let open = got.iter().position(|(c, _, _)| matches!(c, Command::OpenUrl(_)));
        let copy = got.iter().position(|(c, _, _)| matches!(c, Command::CopyUrl(_)));
        assert_eq!(open.map(|i| i + 1), copy);
        assert!(got.iter().any(|(c, e, _)| *e && c == &Command::OpenUrl("https://example.com/x".into())));
        assert!(got.iter().any(|(c, e, _)| *e && c == &Command::CopyUrl("https://example.com/x".into())));
        // The gaps divide the list into groups rather than padding it, and the link's own
        // group is one of them.
        assert!(menu_items(&s).windows(2).any(|w| matches!(&w[0], MenuRow::Gap) && matches!(&w[1], MenuRow::Row { cmd: Command::OpenUrl(_), .. })));
    }
}
#[cfg(test)]
mod bidi_tests {
    use super::*;

    #[test]
    fn glyph_origins_and_disjoint_links_use_the_visual_geometry() {
        assert_eq!(glyph_origin(20.0, 30.0, 0), 20.0);
        assert_eq!(glyph_origin(20.0, 30.0, 1), 50.0);
        assert_eq!(glyph_origin(20.0, 30.0, 2), 20.0);
        let actions = [Action { range: 0..4, kind: ActionKind::Url("https://example.org".into()) }];
        let mut hits = vec![Vec::new()];
        merge_hot(&mut hits, &actions, &(0..1), 0.0, 10.0);
        merge_hot(&mut hits, &actions, &(4..6), 10.0, 30.0);
        merge_hot(&mut hits, &actions, &(1..4), 30.0, 50.0);
        assert_eq!(hits, [vec![(0.0, 10.0), (30.0, 50.0)]]);
    }

    /// Exercise the same placement -> cluster mapping -> selection path as the
    /// renderer, with deterministic advances and no DirectWrite/font dependency.
    fn bidi_line(text: &str, scale: f32) -> SelLine {
        let mut measure = rubrica_type::paragraph::MonospaceMeasure { size: 10.0, factor: 1.0 };
        let (para, plan) = rubrica_type::typeset(text, &Spacing::for_size(10.0), StyleId(0),
            &[], &BreakOptions::new(300.0), &mut measure);
        assert_eq!(plan.lines.len(), 1);
        let bidi = rubrica_type::BidiInfo::new(text, None);
        let mut segs = Vec::new();
        let mut marks = Vec::new();
        for slot in place_bidi(&para, &plan.lines[0], &bidi) {
            let Some(id) = slot.node else {
                mark_glue(text, &slot, 0.0, &mut segs, &mut marks);
                continue;
            };
            let node = para.node(id);
            let chars: Vec<_> = text[node.text.clone()].chars().collect();
            let clusters = chars.iter().enumerate().flat_map(|(i, c)| {
                std::iter::repeat_n(i as u16, c.len_utf16())
            }).collect();
            let run = GlyphRun {
                bidi_level: slot.bidi_level, face: 0, size: 10.0, text: node.text.clone(),
                glyphs: vec![1; chars.len()], advances: vec![10.0; chars.len()],
                offsets: vec![], clusters, ascent: 8.0, descent: 2.0, line_gap: 0.0,
            };
            mark_run(text, &run, slot.x, &mut marks);
            segs.push((node.text.clone(), slot.x, slot.x + slot.w));
        }
        marks.sort_by_key(|m| m.0);
        mark_line(text, Band { top: 0.0, h: 10.0, k: scale, join: Join::None },
            &segs, &marks, &mut 0).unwrap()
    }

    #[test]
    fn rtl_pointer_arrows_and_copy_follow_the_drawn_positions() {
        let line = bidi_line("אב גד", 1.5);
        assert!(line.xs.windows(2).all(|w| w[0] >= w[1]));
        let sel = [line];
        for ch in 0..sel[0].xs.len() {
            assert_eq!(caret_at(&sel, sel[0].xs[ch], 1.0), Caret { line: 0, ch });
        }
        let from = Caret { line: 0, ch: 0 };
        assert_eq!(next_caret(&sel, from, Motion::Left).ch, 1);
        assert_eq!(next_caret(&sel, from, Motion::Home).ch, 5);
        assert_eq!(next_caret(&sel, from, Motion::End).ch, 0);
        let all = Selection { from, to: Caret { line: 0, ch: 5 } };
        assert_eq!(selection_text(&sel, all), "אב גד");
        let rects = selection_rects(&sel, all);
        assert_eq!(rects.len(), 1);
        assert!((rects[0].0 + rects[0].2 - 450.0).abs() < 0.001);
    }

    #[test]
    fn a_logical_bidi_selection_can_have_disjoint_visual_rectangles() {
        let sel = [bidi_line("Aאב12גדZ", 1.0)];
        let selection = Selection { from: Caret { line: 0, ch: 0 }, to: Caret { line: 0, ch: 2 } };
        assert_eq!(selection_text(&sel, selection), "Aא");
        let rects = selection_rects(&sel, selection);
        assert_eq!(rects.len(), 2);
        assert_eq!(rects[0], (0.0, 0.0, 10.0, 10.0));
        assert_eq!(rects[1], (60.0, 0.0, 10.0, 10.0));
        let needle = crate::find::Needle::of(&sel);
        let hits = needle.hits("אב12");
        assert_eq!(selection_text(&sel, needle.span(&hits[0]).unwrap()), "אב12");
    }

    #[test]
    fn clusters_map_utf16_characters_to_glyphs_including_ligatures() {
        let text = "لا😀ב";
        let run = GlyphRun {
            bidi_level: 1, face: 0, size: 10.0, text: 0..text.len(),
            glyphs: vec![1, 2, 3], advances: vec![12.0, 20.0, 8.0], offsets: vec![],
            clusters: vec![0, 0, 1, 1, 2], ascent: 8.0, descent: 2.0, line_gap: 0.0,
        };
        let mut marks = Vec::new();
        mark_run(text, &run, 0.0, &mut marks);
        assert_eq!(marks, [(0, 40.0, 34.0), (2, 34.0, 28.0), (4, 28.0, 8.0), (8, 8.0, 0.0)]);
    }

    #[test]
    fn bidi_spaces_do_not_select_the_intervening_rtl_word() {
        let sel = [bidi_line("abc אבג xyz", 1.0)];
        let selection = Selection { from: Caret { line: 0, ch: 7 }, to: Caret { line: 0, ch: 8 } };
        assert_eq!(selection_text(&sel, selection), " ");
        let rects = selection_rects(&sel, selection);
        assert_eq!(rects.len(), 1);
        assert!(rects[0].2 < 10.0);
    }

    #[test]
    fn a_full_selection_covers_synthetic_script_glue() {
        let sel = [bidi_line("中abc文", 1.0)];
        let all = Selection { from: Caret { line: 0, ch: 0 }, to: Caret { line: 0, ch: 5 } };
        let rects = selection_rects(&sel, all);
        assert_eq!(rects.len(), 1);
        assert_eq!(rects[0].2, sel[0].xs[5] - sel[0].xs[0]);
    }
}

#[cfg(test)]
mod document_interaction_tests {
    use super::*;

    fn indexed(text: &str, start: usize, objects: &[rubrica_doc::ObjectSpan], shift: usize) -> SelLine {
        let mut consumed = start;
        let mut line = mark_line(text,
            Band { top: 0.0, h: 20.0, k: 1.0, join: Join::None },
            &[(start..text.len(), 0.0, 100.0)], &[], &mut consumed).unwrap();
        copy_objects(&mut line, text, start, objects, shift);
        line
    }

    fn copy(lines: &[SelLine], from: usize, to: usize) -> String {
        selection_text(lines, Selection {
            from: Caret { line: 0, ch: from }, to: Caret { line: 0, ch: to },
        })
    }

    #[test]
    fn formula_copy_restores_delimiters_without_expanding_caret_geometry() {
        let doc = Document::parse("中 $x^2$ שלום $y$ end");
        let b = &doc.blocks[0];
        let l = indexed(&b.text, 0, &b.objects, 0);
        assert_eq!(l.chars.len() + 1, l.xs.len());
        let len = l.chars.len();
        let sel = [l];
        assert_eq!(copy(&sel, 0, len), "中 $x^2$ שלום $y$ end");
        assert_eq!(copy(&sel, 2, 3), "$x^2$");
        assert_eq!(copy(&sel, 0, 1), "中");
    }

    #[test]
    fn object_copy_survives_list_prefixes_wrapping_and_table_cells() {
        let doc = Document::parse("- 前文 $a$ after\n\n| Value |\n| --- |\n| $b$ |\n\n$$c=d$$\n");
        let b = &doc.blocks[0];
        let prefix = "1. ";
        let text = format!("{prefix}{}", b.text);
        let start = prefix.len() + b.objects[0].range.start;
        let l = indexed(&text, start, &b.objects, prefix.len());
        let len = l.chars.len();
        assert_eq!(copy(&[l], 0, len), "$a$ after");
        let c = &doc.blocks[1].table.as_ref().unwrap().rows[0][0];
        let l = indexed(&c.text, 0, &c.objects, 0);
        assert_eq!(copy(&[l], 0, 1), "$b$");
        let b = doc.blocks.last().unwrap();
        let l = indexed(&b.text, 0, &b.objects, 0);
        assert_eq!(copy(&[l], 0, 1), "$$c=d$$");
    }

    #[test]
    fn encoded_fragments_find_the_same_heading_as_local_jumps() {
        let doc = Document::parse("# Start\n\n## 第二章\n\n## A *bold* heading\n");
        assert_eq!(heading_index(&doc, "%E7%AC%AC%E4%BA%8C%E7%AB%A0"), Some(1));
        assert_eq!(heading_index(&doc, "a-bold-heading"), Some(2));
        assert_eq!(heading_index(&doc, "missing"), None);
    }
}
