//! Window host and Direct2D renderer.
//!
//! The display list is built in document points, then scaled to device independent
//! pixels whenever the client width or the window DPI changes. That split is
//! deliberate: `DrawGlyphRun` takes advance widths in the same units as its
//! `fontEmSize`, so scaling at paint time would mean copying every advance array on
//! every frame. Doing it once per relayout keeps scrolling allocation-free.

use std::collections::HashMap;
use std::path::PathBuf;

use rubrica_doc::{Action, ActionKind, Align, Block, BlockKind, Document, InlineStyle};
use rubrica_type::justification::place;
use rubrica_type::paragraph::{Item, Spacing, StyleId, StyleSpan};
use rubrica_type::units::Pt;
use rubrica_type::{BreakOptions, Hyphenation, typeset, typeset_hyphenated};
use windows::core::{w, Interface, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
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
use windows::Win32::Graphics::Gdi::{HBRUSH, InvalidateRect, ScreenToClient};
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
use windows::Win32::UI::Input::KeyboardAndMouse::{
    VK_0, VK_A, VK_ADD, VK_C, VK_D, VK_NUMPAD0, VK_OEM_MINUS, VK_OEM_PLUS, VK_SUBTRACT,
    VIRTUAL_KEY,
};
use windows::Win32::UI::Controls::Dialogs::{
    GetOpenFileNameW, OPENFILENAMEW, OFN_FILEMUSTEXIST, OFN_HIDEREADONLY, OFN_PATHMUSTEXIST,
};
use windows::Win32::UI::Shell::{DragAcceptFiles, DragFinish, DragQueryFileW, ShellExecuteW, HDROP};
use windows::Win32::UI::WindowsAndMessaging::{
    CS_DBLCLKS, CS_HREDRAW, CS_VREDRAW, CREATESTRUCTW, CreateWindowExW, DefWindowProcW, DispatchMessageW,
    GWLP_USERDATA, GetClientRect, GetCursorPos, GetWindowLongPtrW, GetMessageW, HCURSOR, HWND_TOP,
    HTCLIENT, IDC_ARROW, IDC_HAND, KillTimer, LoadCursorW, MSG, PostQuitMessage, RegisterClassExW,
    SetCursor, SW_SHOWNORMAL, SetTimer, SetWindowLongPtrW, SetWindowPos, ShowWindow, TranslateMessage,
    WNDCLASSEXW, WM_DESTROY, WM_DPICHANGED, WM_ERASEBKGND, WM_KEYDOWN, WM_MOUSEWHEEL, WM_NCCREATE,
    WM_PAINT, WM_SETCURSOR, WM_SIZE, WM_TIMER, WS_EX_APPWINDOW, WS_OVERLAPPEDWINDOW, SWP_NOACTIVATE,
    SWP_NOZORDER, WM_DROPFILES, WM_LBUTTONDOWN, WM_LBUTTONDBLCLK, WM_LBUTTONUP, WM_MOUSEMOVE,
};
use windows_numerics::Vector2;

use crate::clipboard;
use crate::font::{FaceRequest, FontEngine, GlyphRun, ObjectBox, Style as RunStyle};
use crate::hyphen::Hyphenator;
use crate::images::ImageStore;
use crate::math::MathStore;
use crate::theme::{ColorRole, Leading, Theme, Zoom};
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
/// How far the pointer has to travel, in device pixels, before a held left button stops
/// meaning "here" and starts meaning "from here to there".
const DRAG_SLOP: f32 = 3.0;

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
    color: ColorRole,
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
    /// The page's text, character by character, as the current layout drew it.
    sel_index: Vec<SelLine>,
    /// What the reader has dragged out, if anything. Cleared by a relayout, whose
    /// rewrapped lines would otherwise leave a selection pointing at other words.
    selection: Option<Selection>,
    /// Where the button went down in the text, which is the end of a selection that
    /// has not been dragged yet and the point a double click expands from.
    press_caret: Option<Caret>,
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

    let mut view = Box::new(View {
        d2d,
        target: None,
        hwnd_target: None,
        font,
        theme: Theme::default(),
        doc: Document::parse(&source),
        ops: Vec::new(),
        palette: Palette::of(system_prefers_dark()),
        dark_override: None,
        brushes: HashMap::new(),
        hotspots: Vec::new(),
        note_tops: Vec::new(),
        sel_index: Vec::new(),
        selection: None,
        press_caret: None,
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
        dragging: false,
        press_at: None,
        images: None,
        math: MathStore::new(),
        hyphenator: Hyphenator::english(),
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
                match wp.0 as u32 {
                    k if k == VK_UP.0 as u32 => self.scroll_by(-step),
                    k if k == VK_DOWN.0 as u32 => self.scroll_by(step),
                    k if k == VK_PRIOR.0 as u32 => self.scroll_by(-page),
                    k if k == VK_NEXT.0 as u32 => self.scroll_by(page),
                    k if k == VK_ESCAPE.0 as u32 => PostQuitMessage(0),
                    k if k == VK_O.0 as u32 && ctrl => {
                        if let Some(path) = self.prompt_for_file(hwnd) {
                            self.load_document(&path);
                        }
                    }
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
                    // Both palettes are always one keypress apart, whichever way the
                    // system setting points: a reader in a bright room at 2pm has the
                    // same claim on the dark one as anyone whose OS says so.
                    k if ctrl && k == VK_D.0 as u32 => {
                        let dark = !self.palette.dark;
                        self.dark_override = Some(dark);
                        self.set_dark(dark, hwnd);
                    }
                    // A copy with nothing selected leaves the clipboard alone. Clearing
                    // it would throw away what the reader put there from somewhere else,
                    // to no purpose: an empty selection is not an edit.
                    k if k == VK_C.0 as u32 && ctrl => {
                        if let Some(s) = self.selection {
                            let text = selection_text(&self.sel_index, s);
                            if let Err(e) = clipboard::copy_text(hwnd, &text) {
                                eprintln!("clipboard: {e}");
                            }
                        }
                    }
                    k if k == VK_A.0 as u32 && ctrl => self.select_all(),
                    _ => {}
                }
                let _ = InvalidateRect(Some(hwnd), None, false);
                LRESULT(0)
            }
            WM_TIMER => {
                // A poll, not a push: there is no message for this setting. The reader's
                // own choice outranks it, and `set_dark` does nothing when the answer it
                // gets is already on screen.
                let dark = self.dark_override.unwrap_or_else(system_prefers_dark);
                self.set_dark(dark, hwnd);
                let _ = InvalidateRect(Some(hwnd), None, false);
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
        let y = y + self.scroll * scale_of(self.dpi);
        self.hotspots
            .iter()
            .position(|h| x >= h.x && x <= h.x + h.w && y >= h.y && y <= h.y + h.h)
    }

    /// The place in the page's text under a pointer position given in client pixels,
    /// translated into document pixels the same way [`View::hot_at`] translates.
    fn caret_under(&self, x: f32, y: f32) -> Caret {
        caret_at(&self.sel_index, x, y + self.scroll * scale_of(self.dpi))
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

    /// Act on the target at this index in [`View::hotspots`].
    fn activate(&mut self, i: usize, hwnd: HWND) {
        let Some(kind) = self.hotspots.get(i).map(|h| h.kind.clone()) else { return };
        match kind {
            HotKind::Url(url) => open_url(&url),
            HotKind::Cite(note) => {
                if let Some(&top) = self.note_tops.get(note) {
                    // The note lands a line below the top edge rather than against it,
                    // so some of the page just left stays in view: a citation is followed
                    // to read, and reading means being able to come back.
                    self.scroll = (top - self.theme.base).max(0.0);
                    self.clamp_scroll();
                    let _ = unsafe { InvalidateRect(Some(hwnd), None, false) };
                }
            }
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
        // A selection is a pair of places in the old wrapping. Lines have moved, so
        // the words they named are elsewhere, and holding on to the range would show
        // the reader ink they never dragged over.
        self.selection = None;
        self.press_caret = None;
        self.sel_index = page.sel;
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
    notes: &HashMap<String, usize>,
    top: Pt,
    line_h: Pt,
    k: f32,
) {
    for (i, h) in hit.iter().enumerate() {
        let Some((x0, x1)) = *h else { continue };
        let Some(kind) = hot_kind(&actions[i].kind, notes) else { continue };
        hots.push(Hot { x: x0 * k, y: top * k, w: (x1 - x0) * k, h: line_h * k, kind });
    }
}

/// The jump an action offers, or `None` when its destination is not on the page or
/// not openable.
fn hot_kind(kind: &ActionKind, notes: &HashMap<String, usize>) -> Option<HotKind> {
    match kind {
        ActionKind::Url(u) if openable(u) => Some(HotKind::Url(u.clone())),
        ActionKind::Cite(label) => notes.get(label.as_str()).copied().map(HotKind::Cite),
        _ => None,
    }
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
    let Ctx { theme, styles, math, notes, k } = *ctx;
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
    let spacing = Spacing::for_size(size);
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
        emit_hots(hots, actions, &hit, notes, top, line_h, k);
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
            // The bands of a selection, over the ink and under nothing: their geometry
            // is the character index's, so a drag never costs a relayout, and drawing
            // them last is what keeps them visible inside a code panel.
            if let Some(s) = self.selection {
                if let Some(brush) = self.sel_brush.clone() {
                    for (x, y, w, h) in selection_rects(&self.sel_index, s) {
                        if y + h < top || y > bottom {
                            continue;
                        }
                        let r = D2D_RECT_F { left: x, top: y, right: x + w, bottom: y + h };
                        target.FillRectangle(&r, &brush);
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
            let _ = target.EndDraw(None, None);
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
                emit_hots(hots, &c.actions, &hit, ctx.notes, ly, size * leading, k);
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

    let ctx = Ctx { theme, styles: &styles, math: &*objects.math, notes: &notes, k };

    for (b, p) in doc.blocks.iter().zip(prepared) {
        let base_left = left;
        // Offsets are computed against the block's own text, which already carries
        // the list marker, so they need no shifting.
        let (hyphens, hyphen_width) = hyphenation_for(&p.text, &styles, p.base, hyphenator, font);
        let size = theme.body_size(b.kind);
        y += theme.space_before(b.kind, first);
        first = false;
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

    Page { ops, height: y + theme.base * 2.0, column, left, hotspots: hots, note_tops, sel }
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
}
