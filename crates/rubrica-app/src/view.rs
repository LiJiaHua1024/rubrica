//! Window host and Direct2D renderer.
//!
//! The display list is built in document points, then scaled to device independent
//! pixels whenever the client width or the window DPI changes. That split is
//! deliberate: `DrawGlyphRun` takes advance widths in the same units as its
//! `fontEmSize`, so scaling at paint time would mean copying every advance array on
//! every frame. Doing it once per relayout keeps scrolling allocation-free.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use rubrica_doc::{Action, ActionKind, Align, Block, BlockKind, Document, InlineStyle};
use rubrica_type::justification::place;
use rubrica_type::paragraph::{Item, Spacing, StyleId, StyleSpan};
use rubrica_type::units::Pt;
use rubrica_type::{BreakOptions, Hyphenation, typeset, typeset_hyphenated};
use windows::core::{w, BOOL, Interface, PCWSTR};
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
    CoInitializeEx, COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE,
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
    VK_SUBTRACT, VIRTUAL_KEY,
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
use crate::find::Needle;
use crate::font::{FaceRequest, FontEngine, GlyphRun, ObjectBox, Style as RunStyle};
use crate::hyphen::Hyphenator;
use crate::images::ImageStore;
use crate::math::MathStore;
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
    /// Extra drop below the line's baseline, positive downwards. Zero for prose; a
    /// formula's pieces each sit somewhere of their own within its box.
    dy: f32,
    /// Which of the theme's inks carries this run, recorded for the same reason
    /// `family` is: the report can then say what the page actually painted rather
    /// than what the layout asked for.
    pub color: ColorRole,
}

pub enum Op {
    Runs(Vec<PaintRun>),
    /// An inline object, positioned in device independent pixels.
    Image { path: PathBuf, x: f32, y: f32, w: f32, h: f32 },
    Rect { x: f32, y: f32, w: f32, h: f32, color: ColorRole },
    Line { x0: f32, y0: f32, x1: f32, y1: f32, thickness: f32, color: ColorRole },
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
    Document(PathBuf),
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
    /// Top edge and height, in the same device independent pixels as [`Hot`] and
    /// measured from the top of the document.
    pub y: f32,
    pub h: f32,
    /// What separates this line from the one before it in the text a copy hands back.
    pub join: Join,
    pub chars: Vec<char>,
    /// The left edge of every boundary: `xs.len() == chars.len() + 1`, non-decreasing.
    /// A step of no width is a character the source has and this line does not paint,
    /// which is what a wrapped line's space between two of its words looks like.
    pub xs: Vec<f32>,
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

/// Where a pointer at `x`, `y` (device independent pixels from the top of the document)
/// lands in the page's text. Above the first line and below the last both clamp rather
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
    let mut ch = l.xs.len().saturating_sub(1);
    for (i, &edge) in l.xs.iter().enumerate() {
        if edge > x {
            ch = if i > 0 && (x - l.xs[i - 1]) < (edge - x) { i - 1 } else { i };
            break;
        }
    }
    Caret { line, ch }
}

/// The rectangles to lay under a selection, one per line it touches.
pub fn selection_rects(sel: &[SelLine], s: Selection) -> Vec<(f32, f32, f32, f32)> {
    let mut out = Vec::new();
    for (line, lo, hi) in pieces(sel, s) {
        let l = &sel[line];
        let (x0, x1) = (l.xs[lo], l.xs[hi]);
        if x1 - x0 > 0.0 {
            out.push((x0, l.y, x1 - x0, l.h));
        }
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
        out.extend(l.chars[lo..hi].iter().copied());
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
        // The same column where the line has one, and the end of it where it does not:
        // an index that has only drawn lines cannot know which character a reader's eye
        // was following down a paragraph.
        Motion::Up => Caret { line: at.line.saturating_sub(1), ch: at.ch },
        Motion::Down => Caret { line: (at.line + 1).min(last), ch: at.ch },
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
    /// Read the file this page came from again, from disk.
    Reload,
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
struct MenuState {
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
    v.push(MenuRow::Gap);
    for (i, f) in TextFace::ALL.iter().enumerate() {
        let on = i == s.face;
        let ok = s.offered.get(i).copied().unwrap_or(false);
        // The face in use is always clickable: a machine that has lost a family since it
        // was chosen still has to be able to choose away from it.
        v.push(if on { check(Command::Face(i), f.label, true) } else { row(Command::Face(i), f.label, ok) });
    }
    v.push(MenuRow::Gap);
    for (i, m) in Measure::ALL.iter().enumerate() {
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
    let verb = utf16("open");
    let target = utf16(url);
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
        eprintln!("could not open {url}");
    }
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
    let restore = crate::settings::reading()
        .filter(|(left, _)| Some(left.as_path()) == path.as_deref())
        .map(|(_, anchor)| anchor);

    // The file the text above came out of, as it stands at this moment. `main` has already
    // read it, so this is the stamp of the page on the screen rather than of some text that
    // arrived afterwards -- and if a save did land in between, the first poll finds it a
    // fraction of a second later.
    let stamp = path.as_deref().and_then(stamp_of);

    let mut view = Box::new(View {
        d2d,
        target: None,
        hwnd_target: None,
        font,
        theme,
        doc: Document::parse(&source),
        ops: Vec::new(),
        palette: Palette::of(dark),
        dark_override: saved.dark,
        brushes: HashMap::new(),
        hotspots: Vec::new(),
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
        stamp,
        history: History::default(),
        dragging: false,
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
                if held(VK_CONTROL) {
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
                        if self.caret.is_some() || self.selection.is_some() {
                            self.caret = None;
                            self.selection = None;
                        } else {
                            PostQuitMessage(0);
                        }
                    }
                    // Both of these go through the menu's own command, so the key and the
                    // row it repeats cannot drift apart -- one of them is written in terms
                    // of the other.
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
                if alt && k == VK_LEFT.0 as u32 {
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
            .position(|h| x >= h.x && x <= h.x + h.w && y >= h.y && y <= h.y + h.h)
    }

    /// The place in the page's text under a pointer position given in client pixels,
    /// translated into document pixels the same way [`View::hot_at`] translates.
    fn caret_under(&self, x: f32, y: f32) -> Caret {
        caret_at(&self.sel_index, x, y + scroll_dip(self.scroll, self.dpi))
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
            HotKind::Document(path) => {
                self.load_document(&path, hwnd);
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
            Command::Reload => {
                if let Some(path) = self.path.clone() {
                    self.load_document(&path, hwnd);
                }
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
        let text = selection_text(&self.sel_index, s);
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
            if let Some(mut p) = paint_run(&self.font, r, 0.0, 0.0, k, ColorRole::Muted) {
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
    pub text: String,
    pub spans: Vec<StyleSpan>,
    pub align: Align,
    /// What a click on this cell's ink would do. A cell is laid out by the grid, so
    /// its targets ride along with the cell rather than with the block's prose.
    pub actions: Vec<Action>,
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
fn paint_run(font: &FontEngine, r: &GlyphRun, x: Pt, dy: Pt, k: f32, color: ColorRole) -> Option<PaintRun> {
    Some(PaintRun {
        family: font.face_family(r.face),
        face: font.font_face(r.face)?,
        em: r.size * k,
        glyphs: r.glyphs.clone(),
        advances: r.advances.iter().map(|a| a * k).collect(),
        offsets: r.offsets.clone(),
        x: x * k,
        baseline: 0.0,
        dy,
        color,
    })
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

/// Widen this line's clickable rectangles by the node that was just laid out, whose
/// ink runs from `x0` to `x1`. A node belongs to a target whenever their text overlaps,
/// which is also how a hyphen or an ellipsis in the middle of a link stays clickable.
fn merge_hot(hit: &mut [Option<(Pt, Pt)>], actions: &[Action], node: &std::ops::Range<usize>, x0: Pt, x1: Pt) {
    if x1 <= x0 {
        return;
    }
    for (h, a) in hit.iter_mut().zip(actions) {
        if a.range.start < node.end && node.start < a.range.end {
            *h = Some(match *h {
                Some((p, q)) => (p.min(x0), q.max(x1)),
                None => (x0, x1),
            });
        }
    }
}

/// Turn this line's merged target rectangles into clickable hotspots, one per target
/// that leads somewhere. A target whose ink is unopenable -- a relative path, a
/// citation of a label no note answers -- is not marked at all, so a pointer that
/// stays an arrow is the reader's answer to "nowhere".
fn emit_hots(
    hots: &mut Vec<Hot>,
    actions: &[Action],
    hit: &[Option<(Pt, Pt)>],
    ctx: &Ctx,
    top: Pt,
    line_h: Pt,
) {
    let k = ctx.k;
    for (i, h) in hit.iter().enumerate() {
        let Some((x0, x1)) = *h else { continue };
        let Some(kind) = hot_kind(&actions[i].kind, ctx) else { continue };
        hots.push(Hot { x: x0 * k, y: top * k, w: (x1 - x0) * k, h: line_h * k, kind });
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
                ctx.anchors.get(&slug(fragment)).copied().map(HotKind::Heading)
            }
            // A path is a request for another file, and this one reads them: it becomes
            // a target only when the file is actually on disk beside the open document.
            None => document_link(ctx.base, u).map(HotKind::Document),
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
/// DirectWrite reports a cluster as a UTF-16 index into the run's own text, which is
/// also its byte offset for anything in the Basic Multilingual Plane -- the walk below
/// is exact for the rest. Shaping swallows a character into its neighbour's ligature
/// without giving it an entry, and the caller reads that one off the gap.
fn mark_run(text: &str, run: &GlyphRun, base: Pt, out: &mut Vec<(usize, Pt)>) {
    let sub = &text[run.text.clone()];
    let units = sub.encode_utf16().count();
    let mut byte_of = vec![run.text.end; units + 1];
    let mut u = 0usize;
    for (b, c) in sub.char_indices() {
        byte_of[u] = run.text.start + b;
        u += c.len_utf16();
    }
    let mut acc: Pt = 0.0;
    let mut prev = usize::MAX;
    for (gi, cl) in run.clusters.iter().enumerate() {
        let c = (*cl as usize).min(units);
        if c != prev {
            out.push((byte_of[c], base + acc));
            prev = c;
        }
        acc += run.advances.get(gi).copied().unwrap_or(0.0);
    }
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
    /// Points to device independent pixels.
    k: f32,
    join: Join,
}

fn mark_line(
    text: &str,
    band: Band,
    segs: &[(std::ops::Range<usize>, Pt, Pt)],
    marks: &[(usize, Pt)],
    consumed: &mut usize,
) -> Option<SelLine> {
    let Band { top, h, k, join } = band;
    let first = segs.first()?;
    let mut l = SelLine {
        y: top * k,
        h: h * k,
        join,
        chars: Vec::new(),
        xs: Vec::new(),
    };
    let mut mi = 0usize;
    let mut at = first.1;
    for (range, x0, x1) in segs {
        // A line centred or right-aligned in a box too small for it can put its ink
        // left of where the box starts; the index keeps to the box.
        let (x0, x1) = (*x0, x1.max(*x0));
        if range.start > *consumed {
            let gap = &text[*consumed..range.start];
            let n = gap.chars().count().max(1) as f64;
            let (from, to) = (f64::from(at), f64::from(x0));
            for (i, c) in gap.chars().enumerate() {
                l.chars.push(c);
                l.xs.push((from + (to - from) * (i as f64 / n)) as Pt);
            }
            at = x0;
        }
        for (b, c) in text[range.clone()].char_indices() {
            let off = range.start + b;
            while mi < marks.len() && marks[mi].0 < off {
                mi += 1;
            }
            // A mark past this segment's right edge belongs to a later one, since a
            // run that fell back to another face can be shaped out of order.
            let x = marks
                .get(mi)
                .filter(|m| m.0 == off && m.1 <= x1)
                .map_or(at, |m| m.1)
                .max(x0)
                .min(x1);
            l.chars.push(c);
            l.xs.push(x.max(l.xs.last().copied().unwrap_or(x)));
        }
        at = x1;
        *consumed = range.end;
    }
    l.xs.push(at);
    (!l.chars.is_empty()).then_some(l)
}

/// Typeset one block into the display list and return the new document y.
fn layout_block(
    font: &mut FontEngine,
    ctx: &Ctx<'_>,
    blk: &Blk<'_>,
    out: &mut Out<'_>,
    mut y: Pt,
) -> Pt {
    let Out { ops, hots, sel } = out;
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
        return layout_table(font, ctx, blk, t, &mut Out { ops, hots, sel }, y);
    }
    if text.trim().is_empty() {
        return y;
    }

    let mixed = text.chars().any(|c| matches!(c as u32, 0x3000..=0x303F | 0x4E00..=0x9FFF | 0x3040..=0x30FF | 0xFF00..=0xFFEF));
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
    let spacing = if grid > 0.0 { Spacing::monospace(size, grid) } else { Spacing::for_size(size) };
    let mut opts = BreakOptions::new(column);
    opts.ragged = b.ragged();
    opts.par_indent = theme.first_line_indent_em * size;
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

    let bg_role = if b.kind == BlockKind::Code { Some(ColorRole::Surface) } else { None };
    let panel_top = y;
    let mut panel_bottom = y;
    // How far into this block's text the selection index has reached, which is shared
    // by every line of it because the space between two of them is in neither.
    let mut consumed = 0usize;

    for line in &plan.lines {
        let top = y;
        let placed = place(&para, line);
        // Where this line starts. A line under a hanging marker begins at the marker's
        // right edge, which is the space [`Blk::hang`] bought it; the first line begins
        // at the block's own left, where the marker sits.
        let line_left = left + if line.first { opts.par_indent } else { hang };
        // How far each clickable range reaches along this line, if it reaches at all.
        // One rectangle per line rather than one per target, because the pointer is
        // only ever in one of the places a wrapped link happens to be.
        let mut hit: Vec<Option<(Pt, Pt)>> = vec![None; actions.len()];
        let mut runs: Vec<PaintRun> = Vec::new();
        // Bars of a formula, as x, top edge, width, thickness and ink, all still
        // relative to this line's baseline because the line has no position yet.
        let mut bars: Vec<(Pt, Pt, Pt, Pt, ColorRole)> = Vec::new();
        let mut ascent = 0.0f32;
        let mut descent = 0.0f32;
        // This line's ink, in the order it was drawn: the source range each segment
        // paints and the two edges it sits between, plus the marks shaping left behind
        // for the characters inside them.
        let mut segs: Vec<(std::ops::Range<usize>, Pt, Pt)> = Vec::new();
        let mut marks: Vec<(usize, Pt)> = Vec::new();
        // One strike rule per line rather than per node: the layout core makes every
        // ideograph and every word its own node, so a struck Chinese phrase would
        // otherwise draw a separate op per character.
        let mut ruled: Option<Rule> = None;
        for slot in placed {
            let Some(node_id) = slot.node else { continue };
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
                    }
                    Some(ObjectSource::Math(index)) => {
                        // Every piece arrives at its own place inside the formula's
                        // box, so nothing here accumulates an advance.
                        if let Some(entry) = math.get(*index) {
                            for (r, dx, dy) in &entry.parts {
                                if let Some(p) = paint_run(font, r, line_left + slot.x + dx, *dy, k, st.color) {
                                    runs.push(p);
                                }
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
            let shaped = font.shape_runs(text, node.text.clone(), &st.face, st.size, st.tracking);
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
                mark_run(text, &r, at, &mut marks);
                ascent = ascent.max(r.ascent - dy);
                descent = descent.max((r.descent + dy).max(0.0));
                if let Some(p) = paint_run(font, &r, at, dy, k, st.color) {
                    runs.push(p);
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
            merge_hot(&mut hit, actions, &node.text, line_left + slot.x, at);
            segs.push((node.text.clone(), line_left + slot.x, at));
        }
        end_rule(&mut ruled, &mut bars);
        let natural = ascent + descent;
        let line_h = (size * leading.for_mixed(mixed)).max(natural * 1.02);
        let baseline = y + (line_h - natural) * 0.5 + ascent;
        for run in runs.iter_mut() {
            run.baseline = (baseline + run.dy) * k;
        }
        ops.push(Op::Runs(runs));
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
        if let Some(l) =
            mark_line(text, Band { top, h: line_h, k, join }, &segs, &marks, &mut consumed)
        {
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

    /// Replace the open document, resetting the view to its top, and leave the page being
    /// stood on behind where a step back can find it again.
    fn load_document(&mut self, path: &std::path::Path, hwnd: HWND) {
        let from = self.here();
        // Asked before the page is replaced, because after it `self.path` is the very
        // path being compared against.
        let same = self.path.as_deref() == Some(path);
        if !self.show_document(path, hwnd) {
            return;
        }
        // Written here rather than at each place that asks for a document, so that opening
        // one by dialog, by dropping it on the window, or by following a link to it all
        // leave the same thing behind -- and so that a step back to an older page does not
        // write that older page as the one the reader chose last. The new page starts at
        // its top, and says so in the same breath: a page and a place are one pair.
        crate::settings::record_reading(path, 0);
        // Reading the page already open again is not a step: the reader did not go
        // anywhere, so there is nothing to come back from. `Reload` is this call with the
        // same path, and a history that grew on every `Ctrl`+`R` would be a `Back` that
        // landed on the same text at the top of the window.
        if !same {
            self.history.leave(from);
        }
    }

    /// Read a file and make it the page, saying whether that worked.
    fn show_document(&mut self, path: &std::path::Path, hwnd: HWND) -> bool {
        match std::fs::read_to_string(path) {
            Ok(src) => {
                self.set_page(src, Some(path.to_path_buf()), hwnd);
                true
            }
            Err(e) => {
                eprintln!("cannot open {}: {e}", path.display());
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
        let Ok(source) = std::fs::read_to_string(&path) else {
            // Half way through being written, or locked by whatever is writing it. The
            // stamp is left as it was, so the next tick asks again rather than assuming
            // this file has already been read.
            return;
        };
        self.doc = rubrica_doc::Document::parse(&source);
        // The age polled *before* the read, not the one taken after it: if the file was
        // written again in between, the older stamp makes the next tick notice, where the
        // newer one would let a change go unread until the save after it.
        self.stamp = now;
        self.relayout_in_place(hwnd);
    }

    /// Make this text the page, from its top, and name it on the title bar. The title is
    /// part of the page rather than of the call that got here, because this is the one
    /// place every route to a new document passes through -- and a reader who has just
    /// stepped back to a document of the same name as this one needs to see which is
    /// which.
    fn set_page(&mut self, source: String, path: Option<PathBuf>, hwnd: HWND) {
        self.doc = rubrica_doc::Document::parse(&source);
        self.path = path;
        // Filed at the same moment as the text that came out of it, so that the next tick
        // of the poll compares the page on screen against the file it was read from rather
        // than against whatever the last page's file was.
        self.stamp = self.path.as_deref().and_then(stamp_of);
        self.scroll = 0.0;
        self.relayout();
        let title = utf16(&window_title(self.path.as_deref()));
        let _ = unsafe { SetWindowTextW(hwnd, PCWSTR(title.as_ptr())) };
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
                        if y + h < top || *y > bottom {
                            continue;
                        }
                        let role = *color;
                        if let Some(brush) = self.brushes.get(&role).cloned() {
                            let r = D2D_RECT_F {
                                left: *x,
                                top: *y - up,
                                right: x + w,
                                bottom: y + h - up,
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
                                Vector2::new(*x0, y0 - up),
                                Vector2::new(*x1, y1 - up),
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
                            let r = D2D_RECT_F {
                                left: *x,
                                top: y - up,
                                right: x + w,
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
                    Op::Runs(runs) => self.draw_runs(&target, runs, up),
                }
            }
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
                            left: x,
                            top: y - up,
                            right: x + w,
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
                        let r =
                            D2D_RECT_F { left: x, top: y - up, right: x + w, bottom: y + h - up };
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
                            let r = D2D_RECT_F {
                                left: x,
                                top: y - up,
                                right: x + w,
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
                bidiLevel: 0,
            };
            target.DrawGlyphRun(
                Vector2::new(run.x, run.baseline - up),
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
    let Out { ops, hots, sel } = out;
    let Ctx { theme, styles, k, .. } = *ctx;
    let Blk { left, column, .. } = *blk;
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
        // And a cell that is too wide for its column cannot be allowed to shrink its
        // way out of the problem either -- the painter leaves a ragged line alone,
        // so the ink would land on the neighbour to the right. Break instead.
        opts.tight_box = true;
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

    let mut paint_row = |cells: &[PreparedCell], top: Pt, head: bool, ops: &mut Vec<Op>, hots: &mut Vec<Hot>, sel: &mut Vec<SelLine>| -> Pt {
        // Measure every cell first: the row is as tall as its tallest cell.
        let mut heights = vec![0.0f32; cols];
        // Which column reaches each line of the row first. A cell that wraps down is
        // not starting a new row, and the separator a copy uses has to answer to the
        // row's shape -- one tab-separated line per line the reader sees -- rather than
        // to the order the columns happened to be painted in.
        let mut opens: Vec<usize> = Vec::new();
        for (i, c) in cells.iter().enumerate().take(cols) {
            let (_, plan) = set_cell(font, c, &spacing, inner_of(widths[i]));
            let n = plan.lines.len();
            heights[i] = n.max(1) as Pt * size * leading;
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
                let mut segs: Vec<(std::ops::Range<usize>, Pt, Pt)> = Vec::new();
                let mut marks: Vec<(usize, Pt)> = Vec::new();
                let mut hit: Vec<Option<(Pt, Pt)>> = vec![None; c.actions.len()];
                let mut bars: Vec<(Pt, Pt, Pt, Pt, ColorRole)> = Vec::new();
                let mut ruled: Option<Rule> = None;
                for slot in placed {
                    let Some(node_id) = slot.node else { continue };
                    let node = para.node(node_id);
                    let st = &styles[node.style.0 as usize];
                    if ruled.as_ref().is_some_and(|r| r.style != node.style) {
                        end_rule(&mut ruled, &mut bars);
                    }
                    let mut at = x + pad + shift + slot.x;
                    for r in font.shape_runs(&c.text, node.text.clone(), &st.face, st.size, st.tracking) {
                        mark_run(&c.text, &r, at, &mut marks);
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
                        runs.push(PaintRun {
                            family: font.face_family(r.face),
                            face,
                            em: r.size * k,
                            glyphs: r.glyphs,
                            advances: r.advances.iter().map(|a| a * k).collect(),
                            offsets: r.offsets,
                            x: at * k,
                            baseline: 0.0,
                            dy: -st.raise,
                            color: st.color,
                        });
                        at += width;
                    }
                    merge_hot(&mut hit, &c.actions, &node.text, x + pad + shift + slot.x, at);
                    segs.push((node.text.clone(), x + pad + shift + slot.x, at));
                }
                end_rule(&mut ruled, &mut bars);
                for run in runs.iter_mut() {
                    run.baseline = (ly + ascent) * k;
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
                emit_hots(hots, &c.actions, &hit, ctx, ly, size * leading);
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
                if let Some(l) = mark_line(
                    &c.text,
                    Band { top: ly, h: size * leading, k, join },
                    &segs,
                    &marks,
                    &mut consumed,
                ) {
                    sel.push(l);
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

    let head_h = paint_row(&t.head, y, true, ops, hots, sel);
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
        y += paint_row(r, y, false, ops, hots, sel);
    }
    // The header panel is drawn before its text, so its height can only be filled
    // in once the first row has been measured.
    if let Some(Op::Rect { h, .. }) = ops.get_mut(panel) {
        *h = head_h * k;
    }
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
                // One face for the whole formula, and a MATH table is a property of a
                // face rather than of a theme role, so this ignores the block's own
                // fonts entirely. `Cambria Math` is the face the platform ships with
                // real math tables; the fallback is asked for its own.
                let names = &theme.fonts.math;
                let req = FaceRequest {
                    family: names[0].clone(),
                    cjk_family: names[0].clone(),
                    fallback: vec![names[1].clone()],
                    weight: 400,
                    italic: false,
                };
                let index = self.math.intern(font, &req, source, size, *display)?;
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
                            StyleSpan {
                                range: sp.range.clone(),
                                style: intern(styles, &theme.fonts.fallback, r(b.kind, style)),
                            }
                        })
                        .collect(),
                    align: t.aligns.get(i).copied().unwrap_or_default(),
                    actions: c.actions.clone(),
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
fn hyphenation_for(
    text: &str,
    styles: &[AppStyle],
    base: usize,
    hyphenator: Option<&Hyphenator>,
    font: &mut FontEngine,
) -> (Vec<usize>, Pt) {
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
    };

    for (b, p) in doc.blocks.iter().zip(prepared) {
        let base_left = left;
        // Offsets are computed against the block's own text, which already carries
        // the list marker, so they need no shifting.
        let (hyphens, hyphen_width) = hyphenation_for(&p.text, &styles, p.base, hyphenator, font);
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
            + if b.list.is_some() || b.item_depth.is_none() { 0.0 } else { level };
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
            &mut Out { ops: &mut ops, hots: &mut hots, sel: &mut sel },
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
            // Where a citation of this note has to land: the top of its first line,
            // which is what `activate` brings into view.
            note_tops.push(y);
            for (i, (nb, p)) in note.blocks.iter().zip(units).enumerate() {
                if i > 0 {
                    y += theme.note_space(false);
                }
                let (hyphens, hyphen_width) =
                    hyphenation_for(&p.text, &styles, p.base, hyphenator, font);
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
                    &mut Out { ops: &mut ops, hots: &mut hots, sel: &mut sel },
                    y,
                );
            }
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
        note_tops,
        anchor_tops,
        sel,
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

    /// The bar's own arithmetic, which is the whole of what can be asked of it outside a
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
        let mut hit: Vec<Option<(Pt, Pt)>> = vec![None; actions.len()];
        merge_hot(&mut hit, &actions, &(0..6), 0.0, 50.0);
        merge_hot(&mut hit, &actions, &(6..10), 50.0, 90.0);
        merge_hot(&mut hit, &actions, &(10..16), 90.0, 140.0);
        assert_eq!(hit, vec![Some((50.0, 90.0))], "the rectangles reached outside the link");
    }

    #[test]
    fn a_target_split_into_nodes_still_covers_the_gap_between_them() {
        // A link's own text arrives in pieces -- `guide`, ` `, `here` -- and the space
        // between the pieces is part of the word the reader clicked.
        let actions = [url("https://e/x", 6..14)];
        let mut hit: Vec<Option<(Pt, Pt)>> = vec![None; actions.len()];
        for (range, x0, x1) in
            [(6..11usize, 50.0f32, 80.0f32), (11..12, 80.0, 88.0), (12..14, 88.0, 104.0)]
        {
            merge_hot(&mut hit, &actions, &range, x0, x1);
        }
        assert_eq!(hit, vec![Some((50.0, 104.0))]);
    }

    #[test]
    fn two_targets_on_one_line_keep_their_own_widths() {
        let actions = [url("https://e/a", 0..4), url("https://e/b", 6..10)];
        let mut hit: Vec<Option<(Pt, Pt)>> = vec![None; actions.len()];
        merge_hot(&mut hit, &actions, &(0..4), 0.0, 40.0);
        merge_hot(&mut hit, &actions, &(6..10), 70.0, 110.0);
        assert_eq!(hit, vec![Some((0.0, 40.0)), Some((70.0, 110.0))]);
    }

    #[test]
    fn a_target_with_no_width_of_its_own_is_not_a_target() {
        let actions = [url("https://e/a", 0..4)];
        let mut hit: Vec<Option<(Pt, Pt)>> = vec![None; actions.len()];
        merge_hot(&mut hit, &actions, &(0..4), 30.0, 30.0);
        assert_eq!(hit, vec![None], "an empty rectangle would swallow a click at its edge");
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
        let xs = (0..=chars.len()).map(|i| i as f32 * CHAR).collect();
        SelLine { y, h: CHAR * 1.5, join, chars, xs }
    }

    /// A line whose character at `squeezed_at` was given no width by the break that
    /// swallowed it, which is what a wrapped line's own inter-word space looks like.
    fn squeezed(text: &str, y: f32, squeezed_at: usize) -> SelLine {
        let chars: Vec<char> = text.chars().collect();
        let xs = (0..=chars.len())
            .map(|i| (i - (i > squeezed_at) as usize) as f32 * CHAR)
            .collect();
        SelLine { y, h: CHAR * 1.5, join: Join::None, chars, xs }
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
        };
        let clicked = hot_kind(&ActionKind::Url("other.md".into()), &ctx);
        let _ = std::fs::remove_dir_all(&dir);

        assert_eq!(clicked, Some(HotKind::Document(dir.join("other.md"))));
        assert_eq!(found, Some(dir.join("other.md")));
        // An author names a space with `%20` because a link cannot carry the space
        // itself; the file on disk has the space.
        assert_eq!(escaped, Some(dir.join("a chapter.markdown")));
        // A destination may ask for a place inside the other file as well. The file is
        // still the half this reader can answer, and the reader lands at its top.
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

    /// The contents of the `Contents` submenu, with their labels -- which is the one thing
    /// about an outline row the rest of these helpers throw away.
    fn contents(s: &MenuState) -> Vec<(Command, String)> {
        menu_items(s)
            .into_iter()
            .find_map(|r| match r {
                MenuRow::Sub { items, .. } => Some(items),
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
        assert_eq!(
            got.iter().filter(|(_, _, c)| *c).count(),
            3,
            "exactly one palette row, one face row and one measure row are on"
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
            !menu_items(&s).iter().any(|r| matches!(r, MenuRow::Sub { .. })),
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
            (&Command::OpenFile, "Open\u{2026}\tCtrl+O"),
            (&Command::Reload, "Reload\tCtrl+R"),
        ] {
            let (_, label) = got.iter().find(|(c, _)| c == cmd).expect("the row the key answers");
            assert_eq!(*label, key, "a hint that is not the key is worse than none");
        }
        // Nothing else may claim one. `Ctrl`+`D` picks whichever palette is not on the
        // screen rather than the row it would be printed on, and the bracket keys step
        // the measure instead of settling on the rung they are next to.
        assert_eq!(got.iter().filter(|(_, l)| l.contains('\t')).count(), 10);
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
