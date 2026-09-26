//! What the reader has chosen about the page, what they had open and where they left the
//! window, kept for the next one.
//!
//! Appearance, window geometry and per-document preferences under `Software\Rubrica`,
//! which is where a Windows program puts them: no path to choose for a settings file, no
//! format to invent, no dependency to carry, and the palette the system prefers is
//! already read out of the same store.
//!
//! The encoding of a state into those numbers and back is kept apart from the calls
//! that carry them, so what a stored number means can be read -- and tested -- without a
//! live registry in the way.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};

use windows::core::PCWSTR;
use windows::Win32::Foundation::{ERROR_SUCCESS, RECT};
use windows::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, KEY_READ, RegCloseKey, RegGetValueW, RegOpenKeyExW, RegQueryValueExW,
    RegSetKeyValueW, RRF_RT_REG_BINARY, REG_BINARY, REG_DWORD, REG_SZ,
};

use crate::i18n::Language;
use crate::theme::{Measure, TextFace, Zoom};
use crate::view::utf16;
use rubrica_workspace::{DocumentRef, FileRef, SessionSnapshot, SessionTab, TabKind, Workspace, WorkspaceSnapshot};

const SUBKEY: &str = "Software\\Rubrica";
/// The names the numbers are stored under, in the order [`words`] writes them.
const NAMES: [&str; 4] = ["Zoom", "Dark", "Face", "Measure"];
/// The document to open again next time. The one value that is a path rather than a
/// number -- and the thing [`ANCHOR`] is a place in, which is why the two are written in
/// the same breath.
const OPENED: &str = "Document";
/// How far down that document the reader had got: the index of the character sitting at
/// the top of the window, counted through the page's characters in reading order.
///
/// A place in the prose rather than a distance in points, because the same page is a
/// different length in every width, face and zoom -- and a start-up can lose one of those
/// choices, since a remembered face that has left the machine is dropped. A remembered
/// distance would then name a paragraph the reader was never in; an index names the same
/// stretch of text in either layout. It is the same thing [`crate::view`] keeps the
/// reader's place with across a measure change, and a place worth keeping mid-window is
/// worth keeping across a session.
const ANCHOR: &str = "Anchor";
/// The active chapter for a windowed TXT document. `u32::MAX` means that the document
/// is being read as one continuous page, so a stale chapter cannot survive a mode change.
const CHAPTER: &str = "Chapter";
const NO_CHAPTER: u32 = u32::MAX;
/// What `Dark` holds when the reader asked for the system's own setting to decide. The
/// same answer as no value at all, which is why nothing has to be deleted to get back
/// there: a reader who chooses `Follow System` is not asking for a different number, they
/// are asking for the number to stop mattering.
const FOLLOW_SYSTEM: u32 = 2;
/// The four numbers of the window's frame: the top-left corner and the size, in the
/// physical screen pixels `GetWindowPlacement` gives out and `CreateWindowExW` takes back.
///
/// Signed, and written as a `DWORD` with its sign bit intact -- a monitor to the left of
/// the primary one has negative coordinates, and a window remembered there has to be able
/// to come back there. Which is also why these four are not read by [`word`]'s habit of
/// turning an unrecognised number into no number at all: every bit pattern of a coordinate
/// is a coordinate.
const FRAME: [&str; 4] = ["Left", "Top", "Width", "Height"];
/// Whether the window was maximised when its frame was written down. The frame kept is
/// then the window it would go back to, which is what `GetWindowPlacement` calls the
/// normal position and the reason one call answers both halves of this pair.
const MAXIMISED: &str = "Maximised";

/// The reader's appearance, as it was written down. Every part is optional because a
/// value that is not in the registry is not a choice that was taken away -- it is a
/// choice that was never made, and the difference is what the defaults are for.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Settings {
    pub zoom: Option<Zoom>,
    pub dark: Option<bool>,
    pub face: Option<usize>,
    pub measure: Option<usize>,
}

/// The numbers a state is carried by: the size as a percentage of the design, the
/// palette as one of three answers, and the face and the measure as their index in the
/// list the menu shows them in.
fn words(zoom: Zoom, dark: Option<bool>, face: usize, measure: usize) -> [u32; 4] {
    [
        zoom.percent(),
        dark.map_or(FOLLOW_SYSTEM, |d| d as u32),
        face as u32,
        measure as u32,
    ]
}

/// What those numbers -- or the absence of any of them -- mean. Nothing here is trusted:
/// a number from a later version of this program, or one edited by hand, becomes no
/// choice rather than a crash or a page set in a face that has no index.
fn read(zoom: Option<u32>, dark: Option<u32>, face: Option<u32>, measure: Option<u32>) -> Settings {
    Settings {
        // Zero is not a percentage of the design; it is what an unfinished write leaves
        // behind, so it reads as no choice rather than as the smallest step.
        zoom: zoom.filter(|p| *p > 0).map(|p| Zoom::nearest_percent(p as f32)),
        dark: match dark {
            Some(0) => Some(false),
            Some(1) => Some(true),
            _ => None,
        },
        face: face.map(|i| i as usize).filter(|i| *i < TextFace::ALL.len()),
        measure: measure.map(|i| i as usize).filter(|i| *i < Measure::ALL.len()),
    }
}

/// One open handle on a subkey, and the several values asked of it on the way through.
///
/// `RegGetValueW` opens the key it reads from and shuts it again behind the caller's back,
/// so a start-up that asks forty questions of one key pays for the opening forty times --
/// and every one of those openings is a walk of the registry on the way in and the way
/// out. Asking every value of one open handle costs the opening once.
///
/// Reads only: the writes further down still go through the value-only calls, which bring
/// a missing key into being on a first run.
pub(crate) struct Batch {
    key: HKEY,
}

impl Batch {
    /// Open `sub` under the reader's own key, or nothing when it is not there.
    pub(crate) fn open(sub: &str) -> Option<Self> {
        let wide = utf16(sub);
        let mut key = HKEY::default();
        let opened = unsafe {
            RegOpenKeyExW(HKEY_CURRENT_USER, PCWSTR(wide.as_ptr()), None, KEY_READ, &mut key)
        };
        // A key that is not on this machine is a first run rather than a fault, which every
        // reader above this one already treats as nothing stored.
        (opened == ERROR_SUCCESS && !key.is_invalid()).then(|| Self { key })
    }

    /// One number, or `None` when this machine has nothing of that name to give.
    pub(crate) fn word(&self, name: &str) -> Option<u32> {
        let wide = utf16(name);
        let mut value = 0u32;
        let mut kind = REG_DWORD;
        let mut len = std::mem::size_of::<u32>() as u32;
        let read = unsafe {
            RegQueryValueExW(
                self.key,
                PCWSTR(wide.as_ptr()),
                None,
                Some(&mut kind),
                Some(&mut value as *mut u32 as *mut u8),
                Some(&mut len),
            )
        };
        // A short read is a value that is not a number this program wrote, and a value of
        // another type is one this program never wrote at all.
        (read == ERROR_SUCCESS && kind == REG_DWORD && len == std::mem::size_of::<u32>() as u32)
            .then_some(value)
    }

    /// One string, or `None` when this machine has nothing of that name to give.
    pub(crate) fn text(&self, name: &str) -> Option<String> {
        let wide = utf16(name);
        let mut kind = REG_SZ;
        // The length is asked for first, because a path can be any length and the buffer it
        // is read into has to be cut to size beforehand.
        let mut len = 0u32;
        let sought = unsafe {
            RegQueryValueExW(
                self.key,
                PCWSTR(wide.as_ptr()),
                None,
                None,
                None,
                Some(&mut len),
            )
        };
        if sought != ERROR_SUCCESS || len == 0 {
            return None;
        }
        let mut units = vec![0u16; len as usize / 2];
        let read = unsafe {
            RegQueryValueExW(
                self.key,
                PCWSTR(wide.as_ptr()),
                None,
                Some(&mut kind),
                Some(units.as_mut_ptr() as *mut u8),
                Some(&mut len),
            )
        };
        if read != ERROR_SUCCESS || kind != REG_SZ {
            return None;
        }
        // `len` comes back as the bytes actually copied, terminator included, and a value
        // written by something other than this program may not have one.
        let taken = (len as usize / 2).min(units.len());
        Some(String::from_utf16_lossy(&units[..taken]).trim_end_matches('\0').to_string())
    }
}

impl Drop for Batch {
    fn drop(&mut self) {
        let _ = unsafe { RegCloseKey(self.key) };
    }
}

/// One number, or `None` when this machine has nothing of that name to give.
pub(crate) fn word(sub: &str, name: &str) -> Option<u32> {
    Batch::open(sub)?.word(name)
}

/// Whatever was written down under this key, or an empty state for a first run.
fn read_words(sub: &str) -> Settings {
    let Some(batch) = Batch::open(sub) else { return Settings::default() };
    read(
        batch.word(NAMES[0]),
        batch.word(NAMES[1]),
        batch.word(NAMES[2]),
        batch.word(NAMES[3]),
    )
}

fn write_words(sub: &str, w: &[u32; 4]) {
    for (n, v) in NAMES.iter().zip(w) {
        write_word(sub, n, *v);
    }
}

/// One number, put under `name`, reporting whether Windows accepted it.
pub(crate) fn try_write_word(sub: &str, name: &str, value: u32) -> Result<(), String> {
    let sub = utf16(sub);
    let wide = utf16(name);
    let r = unsafe {
        RegSetKeyValueW(
            HKEY_CURRENT_USER,
            PCWSTR(sub.as_ptr()),
            PCWSTR(wide.as_ptr()),
            REG_DWORD.0,
            Some(&value as *const u32 as *const core::ffi::c_void),
            std::mem::size_of::<u32>() as u32,
        )
    };
    if r.is_err() {
        Err(format!("cannot write {name}: {r:?}"))
    } else {
        Ok(())
    }
}

/// Compatibility wrapper for fire-and-forget settings writes.
pub(crate) fn write_word(sub: &str, name: &str, value: u32) {
    if let Err(error) = try_write_word(sub, name, value) {
        eprintln!("settings: {error}");
    }
}

/// Write down what the reader has just chosen.
///
/// Called at the moment of choosing rather than on the way out, because the usual way to
/// close a window is its own `X`, which ends the process without asking anything of it.
pub fn record(zoom: Zoom, dark: Option<bool>, face: usize, measure: usize) {
    write_words(SUBKEY, &words(zoom, dark, face, measure));
}

/// The reader's own choices, as the last run left them.
pub fn load() -> Settings {
    read_words(SUBKEY)
}

pub fn keep_line_breaks() -> bool {
    word(SUBKEY, "KeepLineBreaks") == Some(1)
}

pub fn record_line_breaks(keep: bool) {
    write_word(SUBKEY, "KeepLineBreaks", u32::from(keep));
}

/// Whether the reader wants the spacebar peek watching: a choice of the reader's
/// own menu, honoured again the next time a reader window opens.
pub fn peek_enabled() -> bool {
    word(SUBKEY, "Peek") == Some(1)
}

pub fn record_peek_enabled(on: bool) {
    write_word(SUBKEY, "Peek", u32::from(on));
}

/// One plain number under the reader's own key, for the peek's preferences, which
/// are neither the four numbers of the appearance row nor any document's own.
pub(crate) fn plain_word(name: &str) -> Option<u32> {
    word(SUBKEY, name)
}

pub(crate) fn record_plain_word(name: &str, value: u32) {
    write_word(SUBKEY, name, value);
}

pub fn editor() -> String {
    text(SUBKEY, "Editor").filter(|s| !s.is_empty()).unwrap_or_else(|| "notepad.exe".into())
}

pub fn record_editor(path: &std::path::Path) {
    write_text(SUBKEY, "Editor", &path.to_string_lossy());
}

pub fn editor_args() -> String {
    text(SUBKEY, "EditorArgs").unwrap_or_default()
}

pub fn record_editor_args(args: &str) {
    write_text(SUBKEY, "EditorArgs", args);
}

pub fn language() -> Option<Language> {
    text(SUBKEY, "Language")
        .filter(|s| !s.is_empty())
        .and_then(|code| Language::from_code(&code))
}

pub fn current_language() -> Language {
    language().unwrap_or_else(Language::system_language)
}

pub fn record_language(lang: Option<Language>) {
    let code = lang.map_or("", |l| l.code());
    write_text(SUBKEY, "Language", code);
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DocumentSettings {
    pub line_breaks: Option<bool>,
    pub plain: Option<bool>,
    pub source: bool,
    pub page_stack: bool,
    pub text: rubrica_doc::plain::TextOptions,
    pub encoding: crate::reading::Encoding,
}

impl DocumentSettings {
    fn words(self) -> [u32; 7] {
        [self.line_breaks.map_or(2, u32::from), self.plain.map_or(2, u32::from),
            u32::from(self.source), match self.text.paragraphs {
                rubrica_doc::plain::ParagraphRule::Auto => 0,
                rubrica_doc::plain::ParagraphRule::Lines => 1,
                rubrica_doc::plain::ParagraphRule::BlankLines => 2,
            }, u32::from(self.text.chapters),
            crate::reading::Encoding::ALL.iter().position(|e| *e == self.encoding).unwrap_or(0) as u32,
            u32::from(self.page_stack)]
    }

    fn from_words(w: [Option<u32>; 7]) -> Self {
        let optional = |v| match v { Some(0) => Some(false), Some(1) => Some(true), _ => None };
        Self {
            line_breaks: optional(w[0]), plain: optional(w[1]), source: w[2] == Some(1),
            text: rubrica_doc::plain::TextOptions {
                paragraphs: match w[3] {
                    Some(1) => rubrica_doc::plain::ParagraphRule::Lines,
                    Some(2) => rubrica_doc::plain::ParagraphRule::BlankLines,
                    _ => rubrica_doc::plain::ParagraphRule::Auto,
                },
                chapters: w[4] != Some(0),
            },
            encoding: w[5].and_then(|v| crate::reading::Encoding::ALL.get(v as usize).copied()).unwrap_or_default(),
            page_stack: w[6] == Some(1),
        }
    }
}

const DOCUMENT_NAMES: [&str; 7] = ["LineBreaks", "Plain", "Source", "Paragraphs", "Chapters", "Encoding", "PageStack"];

fn document_key(path: &std::path::Path) -> (String, String) {
    // Canonical paths make Explorer, relative links and the open dialog share a record.
    // Retain the exact path beside the hash so a collision never restores another book.
    let absolute = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let raw = absolute.to_string_lossy().into_owned();
    let hash = raw.as_bytes().iter().fold(0xcbf29ce484222325u64,
        |h, b| (h ^ u64::from(*b)).wrapping_mul(0x100000001b3));
    (format!("{SUBKEY}\\Documents\\{hash:016x}"), raw)
}

/// The canonical form of a path, remembered for as long as this process is running.
///
/// `canonicalize` is a trip through `GetFinalPathNameByHandleW`, which is a few
/// milliseconds on a local path and tens of them on one that lives on a share, and one
/// start-up asks about the same document four or five times over -- `main` on the way in,
/// then the window for its place, its preferences and its chapter. Remembering the answer
/// makes the trip once per path instead.
///
/// Only the readers of a document's record are allowed to remember it. A writer asks
/// afresh, because a page that has moved while the reader had it open belongs in the
/// record at its new home, and a remembered answer would keep sending the write to the
/// old one.
static CANONICAL: LazyLock<Mutex<HashMap<PathBuf, (String, String)>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// [`document_key`] for a reader of a document's record, which answers from the cache
/// whenever this process has already asked about `path`.
fn read_document_key(path: &Path) -> (String, String) {
    let Ok(mut canonical) = CANONICAL.lock() else {
        // A poisoned lock is no reason to look in the wrong place: the key is simply
        // worked out afresh, as it was before there was anything to remember.
        return document_key(path);
    };
    if let Some(known) = canonical.get(path) {
        return known.clone();
    }
    let key = document_key(path);
    canonical.insert(path.to_path_buf(), key.clone());
    key
}

pub fn document(path: &std::path::Path) -> DocumentSettings {
    let (sub, raw) = read_document_key(path);
    let Some(batch) = Batch::open(&sub) else { return DocumentSettings::default() };
    if batch.text("Path").as_deref() != Some(raw.as_str()) {
        return DocumentSettings::default();
    }
    DocumentSettings::from_words(DOCUMENT_NAMES.map(|name| batch.word(name)))
}

pub fn record_document(path: &std::path::Path, settings: DocumentSettings) {
    let (sub, raw) = document_key(path);
    write_text(&sub, "Path", &raw);
    for (name, value) in DOCUMENT_NAMES.iter().zip(settings.words()) { write_word(&sub, name, value); }
}

pub fn document_anchor(path: &std::path::Path) -> Option<usize> {
    let (sub, raw) = read_document_key(path);
    let batch = Batch::open(&sub)?;
    if batch.text("Path").as_deref() != Some(raw.as_str()) { return None; }
    batch.word(ANCHOR).map(|v| v as usize)
}

/// The chapter remembered for a windowed TXT document. Older settings have no value,
/// which correctly falls back to the first chapter.
pub fn document_chapter(path: &std::path::Path) -> Option<usize> {
    let (sub, raw) = read_document_key(path);
    let batch = Batch::open(&sub)?;
    if batch.text("Path").as_deref() != Some(raw.as_str()) { return None; }
    batch.word(CHAPTER).and_then(|value| (value != NO_CHAPTER).then_some(value as usize))
}

/// One string, or `None` when this machine has nothing of that name to give.
pub(crate) fn text(sub: &str, name: &str) -> Option<String> {
    Batch::open(sub)?.text(name)
}

/// One string, put under `name`, reporting whether Windows accepted it.
///
/// `RegSetKeyValueW` rather than the value-only call, because it also brings the key into
/// being -- which a first run has no other way of getting.
pub(crate) fn try_write_text(sub: &str, name: &str, raw: &str) -> Result<(), String> {
    let sub = utf16(sub);
    let value = utf16(name);
    // [`utf16`] appends the terminator the registry expects, and the byte count is taken
    // from the encoded length rather than counted separately.
    let units = utf16(raw);
    let r = unsafe {
        RegSetKeyValueW(
            HKEY_CURRENT_USER,
            PCWSTR(sub.as_ptr()),
            PCWSTR(value.as_ptr()),
            REG_SZ.0,
            Some(units.as_ptr() as *const core::ffi::c_void),
            (units.len() * 2) as u32,
        )
    };
    if r.is_err() {
        Err(format!("cannot write {name}: {r:?}"))
    } else {
        Ok(())
    }
}

/// Compatibility wrapper for fire-and-forget settings writes.
pub(crate) fn write_text(sub: &str, name: &str, raw: &str) {
    if let Err(error) = try_write_text(sub, name, raw) {
        eprintln!("settings: {error}");
    }
}

pub(crate) fn binary(sub: &str, name: &str) -> Option<Vec<u8>> {
    let sub = utf16(sub);
    let name = utf16(name);
    let mut len = 0u32;
    let r = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            PCWSTR(sub.as_ptr()),
            PCWSTR(name.as_ptr()),
            RRF_RT_REG_BINARY,
            None,
            None,
            Some(&mut len),
        )
    };
    if r.is_err() || len == 0 {
        return None;
    }
    let mut data = vec![0u8; len as usize];
    let r = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            PCWSTR(sub.as_ptr()),
            PCWSTR(name.as_ptr()),
            RRF_RT_REG_BINARY,
            None,
            Some(data.as_mut_ptr() as *mut core::ffi::c_void),
            Some(&mut len),
        )
    };
    if r.is_err() {
        return None;
    }
    data.truncate(len as usize);
    Some(data)
}

pub(crate) fn try_write_binary(sub: &str, name: &str, data: &[u8]) -> Result<(), String> {
    let sub = utf16(sub);
    let value = utf16(name);
    let r = unsafe {
        RegSetKeyValueW(
            HKEY_CURRENT_USER,
            PCWSTR(sub.as_ptr()),
            PCWSTR(value.as_ptr()),
            REG_BINARY.0,
            Some(data.as_ptr() as *const core::ffi::c_void),
            data.len() as u32,
        )
    };
    if r.is_err() { Err(format!("cannot write {name}: {r:?}")) } else { Ok(()) }
}

/// The path written down under this key, if it is still a file.
///
/// Being a file is the whole test: a directory is what a path with its last part cut off
/// reads as rather than anything else.
fn read_opened(batch: &Batch) -> Option<PathBuf> {
    batch.text(OPENED).map(PathBuf::from).filter(|p| p.is_file())
}

/// The page and the place in it, written under this key as one pair.
fn write_reading(sub: &str, path: &std::path::Path, anchor: usize) {
    // A path that is not valid UTF-8 is not a thing worth writing down: it came from a
    // file name this program could not have opened in the first place, and putting it in
    // the registry would only make the next start read it back wrong.
    if let Some(raw) = path.to_str() {
        write_text(sub, OPENED, raw);
        write_word(sub, ANCHOR, anchor.min(u32::MAX as usize) as u32);
    }
}

/// The page under this key, if it is still a file, and the place the reader had got to in
/// it -- or the top, for a key with nothing to say.
///
/// The number is not checked against the document's length: a place from a page that has
/// since grown shorter names no line here, and the window that cannot find it stays where
/// it would have started anyway.
fn read_reading(sub: &str) -> Option<(PathBuf, usize)> {
    let batch = Batch::open(sub)?;
    let path = read_opened(&batch)?;
    Some((path, batch.word(ANCHOR).unwrap_or(0) as usize))
}

/// Write down what the reader has open and where in it they are standing, so that the
/// next window starts on their own page rather than on somebody else's sample prose.
///
/// The two go together in one breath because one without the other is a wrong answer: a
/// place kept against a different document is a jump into a paragraph nobody chose, and a
/// page that comes back at its top says the reader never got further than they did. For a
/// chapter-windowed TXT book, the chapter is stored beside the local character anchor; the
/// explicit sentinel clears a chapter left by an earlier mode of the same file.
pub fn record_reading_at(path: &std::path::Path, anchor: usize, chapter: Option<usize>) {
    write_reading(SUBKEY, path, anchor);
    let (sub, raw) = document_key(path);
    write_text(&sub, "Path", &raw);
    write_word(&sub, ANCHOR, anchor.min(u32::MAX as usize) as u32);
    write_word(&sub, CHAPTER, chapter.map_or(NO_CHAPTER, |value| value.min(NO_CHAPTER as usize - 1) as u32));
}

/// The document to open again and the place in it, if there is one and it is still where
/// it was left.
///
/// Being a file is the whole test, not carrying one of the extensions the open dialog
/// offers: whatever the reader had open is what they asked for, and a page that has since
/// moved is nothing to start the program over.
pub fn reading() -> Option<(PathBuf, usize)> {
    read_reading(SUBKEY)
}

/// Forget the last file when the reader explicitly moves to the built-in sample.
pub fn clear_reading() {
    write_text(SUBKEY, OPENED, "");
    write_word(SUBKEY, ANCHOR, 0);
}

/// Where the window was standing when the reader last moved it, and whether it was
/// maximised at the time.
///
/// Physical screen pixels, which is what both the call that reads a frame and the call that
/// sets one speak in. Not device independent ones, and not a DIP times the DPI of the
/// monitor this frame was taken on: a remembered window comes back at the same distance
/// from the corner of the screen it was on, which is the only promise worth keeping about
/// where a window goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Frame {
    pub left: i32,
    pub top: i32,
    pub width: i32,
    pub height: i32,
    pub maximised: bool,
}

/// The five numbers this frame is carried by, in the order [`FRAME`] names them.
fn frame_words(f: Frame) -> ([u32; 4], u32) {
    ([f.left as u32, f.top as u32, f.width as u32, f.height as u32], f.maximised as u32)
}

/// What those numbers mean, or `None` when they were never a window.
///
/// A width or a height of zero is the one value that cannot be a frame somebody stood in --
/// no window has ever been that big -- and reads as no value at all, which is what an
/// interrupted write leaves behind. A negative corner is not a mistake but a monitor to the
/// left of the primary one, and a window bigger than the screen is an honest choice until
/// [`placed`] has made it reachable.
///
/// The clamping is for a number that was never a window at all: the registry is editable,
/// and a coordinate of `i32::MIN` would end the arithmetic in [`placed`] rather than bring
/// a window onto the screen. A million pixels is far past any desktop Windows can be given
/// and far inside a signed 32-bit subtraction.
fn frame_of(parts: [Option<u32>; 4], maximised: Option<u32>) -> Option<Frame> {
    let [Some(left), Some(top), Some(width), Some(height)] = parts else { return None };
    let (width, height) = (width as i32, height as i32);
    (width > 0 && height > 0).then(|| Frame {
        left: (left as i32).clamp(-LIMIT, LIMIT),
        top: (top as i32).clamp(-LIMIT, LIMIT),
        width: width.min(LIMIT),
        height: height.min(LIMIT),
        maximised: maximised == Some(1),
    })
}

/// The furthest out from the origin a remembered frame is allowed to be, in pixels -- see
/// [`frame_of`].
const LIMIT: i32 = 1_000_000;

/// How much of a title bar to count when asking whether a window is reachable: a point a
/// little way along it, and the same height below that to drag by.
///
/// Deliberately more than a title bar actually is at 96 dpi, because the real number is
/// per-monitor and this has to hold for a frame remembered on a 200% display and restored on
/// a 100% one. A window moved a few pixels further than it needed is nothing to notice; one
/// left with two pixels of caption on screen is a window the reader cannot move.
const CAPTION: i32 = 48;

/// The remembered window, brought back onto the screens there are now.
///
/// The failure to guard against: a reader unplugs the monitor their window was on, and
/// every window after is created off the edge of the only display left. It can still be
/// recovered -- click the taskbar, then `Alt`+`Space`, `M`, an arrow key, then the mouse --
/// which is to say the fix is a shortcut almost nobody knows, so the number is fixed here
/// instead of remembered forever.
///
/// Reachability is judged by the title bar and nothing else. A frame whose bar is already
/// on some screen comes back exactly as it was, including one that spans two of them and
/// one taller than either; shrinking those would be undoing the reader's own arrangement to
/// solve a problem they do not have. Only a window that would land nowhere is resized, and
/// then only to what the nearest screen can hold.
pub fn placed(frame: Frame, works: &[RECT]) -> Frame {
    // A quarter of the way along a title bar's worth of pixels: near enough the left end
    // that a window standing almost off the right edge is judged by the part of it that is
    // still on screen, and far enough from the corner that a frame one pixel outside it is
    // not called reachable.
    let cx = frame.left + (frame.width / 4).max(CAPTION / 2);
    let cy = frame.top + CAPTION / 2;
    if works.iter().any(|w| holds(w, cx, cy)) {
        return frame;
    }
    let Some(best) = works.iter().min_by_key(|w| gap(w, cx, cy)) else {
        // No screen to be on: nothing here can make the frame better than it is, and a
        // window at the remembered place is at least the one the reader left.
        return frame;
    };
    // Narrowed to the screen it is being moved onto and then put wholly inside it. A frame
    // that was already on some screen never comes this far, so nothing that was reachable is
    // rearranged: only a window that had nowhere to be is resized, and only to what the one
    // screen it can go back to holds.
    let mut f = Frame {
        width: frame.width.min(best.right - best.left),
        height: frame.height.min(best.bottom - best.top),
        ..frame
    };
    f.left = f.left.min(best.right - f.width).max(best.left);
    f.top = f.top.min(best.bottom - f.height).max(best.top);
    f
}

/// Whether a point is on a screen.
fn holds(w: &RECT, x: i32, y: i32) -> bool {
    x >= w.left && x < w.right && y >= w.top && y < w.bottom
}

/// How far a coordinate is outside a range, and nothing at all if it is inside it.
fn nudge(v: i32, lo: i32, hi: i32) -> i32 {
    (lo - v).max(0).max(v - hi)
}

/// How far a point is from a screen, squared, so that the nearest of several can be picked.
/// In `i64` because a desktop reaching either end of the range [`LIMIT`] allows has
/// differences too wide for `i32` once multiplied by themselves.
fn gap(w: &RECT, x: i32, y: i32) -> i64 {
    let (dx, dy) =
        (nudge(x, w.left, w.right - 1) as i64, nudge(y, w.top, w.bottom - 1) as i64);
    dx * dx + dy * dy
}

/// The five numbers of a frame, put under `sub`.
fn write_frame(sub: &str, frame: Frame) {
    let (parts, maximised) = frame_words(frame);
    for (name, value) in FRAME.iter().zip(&parts) {
        write_word(sub, name, *value);
    }
    write_word(sub, MAXIMISED, maximised);
}

/// The frame written under `sub`, or `None` when the numbers there were never a window.
fn read_frame(sub: &str) -> Option<Frame> {
    let batch = Batch::open(sub)?;
    frame_of(FRAME.map(|name| batch.word(name)), batch.word(MAXIMISED))
}

/// Write down where the window was left, for the next one to be put there.
pub fn record_window(frame: Frame) {
    write_frame(SUBKEY, frame);
}

/// The frame the reader left the window in, or `None` on a machine that has never had one.
pub fn window() -> Option<Frame> {
    read_frame(SUBKEY)
}

const WORKSPACE_KEY: &str = "Workspace";
const WORKSPACE_MAGIC: &[u8; 4] = b"RWS1";

fn put_workspace_string(out: &mut Vec<u8>, value: &str) -> Option<()> {
    let bytes = value.as_bytes();
    let len = u32::try_from(bytes.len()).ok()?;
    out.extend_from_slice(&len.to_le_bytes());
    out.extend_from_slice(bytes);
    Some(())
}

fn take_workspace_string(input: &mut &[u8]) -> Option<String> {
    let bytes = take_workspace_bytes(input)?;
    String::from_utf8(bytes).ok()
}

fn take_workspace_bytes(input: &mut &[u8]) -> Option<Vec<u8>> {
    let raw = input.get(..4)?;
    let len = u32::from_le_bytes(raw.try_into().ok()?) as usize;
    *input = &input[4..];
    let bytes = input.get(..len)?;
    *input = &input[len..];
    Some(bytes.to_vec())
}

fn put_workspace_file(out: &mut Vec<u8>, file: &FileRef) -> Option<()> {
    out.extend_from_slice(&file.fingerprint.to_le_bytes());
    put_workspace_string(out, file.path.to_str()?)
}

fn take_workspace_file(input: &mut &[u8]) -> Option<FileRef> {
    let raw = input.get(..8)?;
    let fingerprint = u64::from_le_bytes(raw.try_into().ok()?);
    *input = &input[8..];
    Some(FileRef::new(PathBuf::from(take_workspace_string(input)?), fingerprint))
}

/// Encode the tab/recent session as one versioned blob. Paths are UTF-8 because the
/// reader could not have opened a non-UTF-8 path on Windows; a path that cannot be encoded
/// makes the whole save fail rather than leaving a half-restored session.
pub fn encode_workspace(snapshot: &WorkspaceSnapshot) -> Option<Vec<u8>> {
    let mut out = WORKSPACE_MAGIC.to_vec();
    out.extend_from_slice(&(snapshot.session.tabs.len() as u32).to_le_bytes());
    for tab in &snapshot.session.tabs {
        out.push(match tab.kind { TabKind::Pinned => 0, TabKind::Preview => 1 });
        put_workspace_file(&mut out, &tab.document)?;
    }
    match &snapshot.session.active {
        None => out.push(0),
        Some(DocumentRef::Sample) => out.push(1),
        Some(DocumentRef::File(file)) => {
            out.push(2);
            put_workspace_file(&mut out, file)?;
        }
    }
    out.extend_from_slice(&(snapshot.recent.len() as u32).to_le_bytes());
    for file in &snapshot.recent {
        put_workspace_file(&mut out, file)?;
    }
    Some(out)
}

pub fn decode_workspace(data: &[u8]) -> Option<WorkspaceSnapshot> {
    let mut input = data;
    if input.get(..4)? != WORKSPACE_MAGIC { return None; }
    input = &input[4..];
    let count = u32::from_le_bytes(input.get(..4)?.try_into().ok()?) as usize;
    input = &input[4..];
    let mut tabs = Vec::with_capacity(count.min(128));
    for _ in 0..count {
        let kind = match *input.first()? { 0 => TabKind::Pinned, 1 => TabKind::Preview, _ => return None };
        input = &input[1..];
        tabs.push(SessionTab { document: take_workspace_file(&mut input)?, kind });
    }
    let active_marker = *input.first()?;
    input = &input[1..];
    let active = match active_marker {
        0 => None,
        1 => Some(DocumentRef::Sample),
        2 => Some(DocumentRef::File(take_workspace_file(&mut input)?)),
        _ => return None,
    };
    let count = u32::from_le_bytes(input.get(..4)?.try_into().ok()?) as usize;
    input = &input[4..];
    let mut recent = Vec::with_capacity(count.min(128));
    for _ in 0..count { recent.push(take_workspace_file(&mut input)?); }
    input.is_empty().then_some(WorkspaceSnapshot {
        session: SessionSnapshot { tabs, active },
        recent,
    })
}

pub fn record_workspace(snapshot: &WorkspaceSnapshot) -> Result<(), String> {
    let data = encode_workspace(snapshot).ok_or("workspace contains a path that cannot be encoded")?;
    try_write_binary(SUBKEY, WORKSPACE_KEY, &data)
}

pub fn workspace() -> Option<WorkspaceSnapshot> {
    decode_workspace(&binary(SUBKEY, WORKSPACE_KEY)?)
}

pub(crate) fn restorable_path(path: &Path) -> bool {
    match std::fs::metadata(path) {
        Ok(metadata) => metadata.is_file(),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
        // A permission or network error is not proof that the document is gone. Keep
        // the remembered entry so a later navigation can report the real problem.
        Err(_) => true,
    }
}

/// Restore the saved workspace without bringing paths that are known to be gone back
/// into the tab strip. The registry format stays raw and versioned; this is the host's
/// file-system policy applied at the point where the pure state is handed to the UI.
pub fn restored_workspace() -> Workspace {
    let Some(snapshot) = workspace() else { return Workspace::default() };
    // The same document is offered three times over -- as the active tab, as one of the
    // tabs and as one of the recent -- and a `metadata` call on a path that lives on a
    // share is not free to repeat. Each distinct path is asked about once.
    let asked: RefCell<HashMap<PathBuf, bool>> = RefCell::new(HashMap::new());
    Workspace::restore_with(snapshot, |path| {
        let mut asked = asked.borrow_mut();
        *asked.entry(path.to_path_buf()).or_insert_with(|| restorable_path(path))
    })
}

#[cfg(test)]
mod tests {    use super::*;

    #[test]
    fn document_preferences_round_trip_and_reject_unknown_values() {
        let chosen = DocumentSettings {
            line_breaks: Some(true), plain: Some(false), source: true, page_stack: true,
            text: rubrica_doc::plain::TextOptions {
                paragraphs: rubrica_doc::plain::ParagraphRule::BlankLines, chapters: false,
            },
            encoding: crate::reading::Encoding::Big5,
        };
        assert_eq!(DocumentSettings::from_words(chosen.words().map(Some)), chosen);
        assert_eq!(DocumentSettings::from_words([None; 7]), DocumentSettings::default());
        assert_eq!(DocumentSettings::from_words([Some(u32::MAX); 7]), DocumentSettings::default());
        let old = DocumentSettings::from_words([Some(0), Some(1), Some(1), Some(2), Some(0), Some(1), None]);
        assert_eq!(old.line_breaks, Some(false));
        assert_eq!(old.plain, Some(true));
        assert!(old.source);
        assert_eq!(old.text.paragraphs, rubrica_doc::plain::ParagraphRule::BlankLines);
        assert!(!old.text.chapters);
        assert!(!old.page_stack);
        for encoding in crate::reading::Encoding::ALL {
            let preferences = DocumentSettings { encoding, ..Default::default() };
            assert_eq!(DocumentSettings::from_words(preferences.words().map(Some)), preferences);
        }
    }
    #[test]
    fn workspace_blob_round_trips_tabs_active_document_and_recent_files() {
        let a = FileRef::new("books/a.md", 7);
        let b = FileRef::new("books/b.md", 9);
        let snapshot = WorkspaceSnapshot {
            session: SessionSnapshot {
                tabs: vec![
                    SessionTab { document: a.clone(), kind: TabKind::Pinned },
                    SessionTab { document: b.clone(), kind: TabKind::Preview },
                ],
                active: Some(DocumentRef::File(b.clone())),
            },
            recent: vec![a, b],
        };
        let encoded = encode_workspace(&snapshot).expect("workspace paths encode");
        assert_eq!(decode_workspace(&encoded), Some(snapshot));
        assert!(decode_workspace(b"RWS2").is_none());
    }

    // Only the tests tidy up after themselves; nothing else this module writes is ever
    // taken away again.
    use windows::Win32::System::Registry::RegDeleteTreeW;

    /// Take a key away before a test starts using it, as well as after: a run that panics
    /// on the way never reaches its own cleanup, and one test's leftover is the next one's
    /// wrong first answer.
    fn clear(sub: &str) {
        let _ = unsafe { RegDeleteTreeW(HKEY_CURRENT_USER, PCWSTR(utf16(sub).as_ptr())) };
    }

    #[test]
    fn every_step_of_the_ladder_comes_back_as_itself() {
        // A remembered size that came back one step off would be a page the reader did not
        // leave. The percentage is the only thing carried, so the ladder has to survive it.
        assert_eq!(zoom_of(0), Zoom::DESIGN);
        for step in -3..=4 {
            let z = zoom_of(step);
            assert_eq!(read(Some(z.percent()), None, None, None).zoom, Some(z), "step {step}");
        }
    }

    #[test]
    fn a_stored_number_that_is_not_a_choice_becomes_no_choice() {
        let s = read(Some(120), Some(1), Some(2), Some(0));
        assert_eq!(s.zoom, Some(Zoom::nearest_percent(120.0)));
        assert_eq!(s.dark, Some(true));
        assert_eq!(s.face, Some(2));
        assert_eq!(s.measure, Some(0));

        // The palette asked of the system, and no value at all, mean the same thing.
        assert_eq!(read(None, Some(FOLLOW_SYSTEM), None, None).dark, None);
        assert_eq!(read(None, None, None, None), Settings::default());
        // A face or a measure past the end of its list, a zoom of no size at all, and a
        // palette number that has never meant anything: none of them is a choice to make.
        let bogus = read(
            Some(0),
            Some(7),
            Some(TextFace::ALL.len() as u32),
            Some(Measure::ALL.len() as u32),
        );
        assert_eq!(bogus.zoom, None);
        assert_eq!(bogus.dark, None);
        assert_eq!(bogus.face, None);
        assert_eq!(bogus.measure, None);
        // The top rung of each list is a real choice; only the one past it is not.
        assert_eq!(
            read(None, None, Some(TextFace::ALL.len() as u32 - 1), Some(Measure::ALL.len() as u32 - 1)),
            Settings {
                face: Some(TextFace::ALL.len() - 1),
                measure: Some(Measure::ALL.len() - 1),
                ..Settings::default()
            }
        );
    }

    #[test]
    fn the_words_a_state_writes_are_the_words_that_read_back() {
        let w = words(Zoom::DESIGN.up(), Some(false), 3, 2);
        let s = read(Some(w[0]), Some(w[1]), Some(w[2]), Some(w[3]));
        assert_eq!(s.zoom, Some(Zoom::DESIGN.up()));
        assert_eq!(s.dark, Some(false));
        assert_eq!(s.face, Some(3));
        assert_eq!(s.measure, Some(2));
        // `Follow System` round-trips into having no opinion, which is what it asks for.
        let w = words(Zoom::DESIGN, None, 0, 1);
        assert_eq!(w[1], FOLLOW_SYSTEM);
        assert_eq!(read(Some(w[0]), Some(w[1]), Some(w[2]), Some(w[3])).dark, None);
    }

    /// The whole road a setting travels: out to the registry and back. What the tests
    /// above check is the encoding, and an encoding nothing can read is no use to anyone.
    #[test]
    fn a_choice_written_down_is_a_choice_read_back() {
        let sub = "Software\\Rubrica Test";
        write_words(sub, &words(Zoom::DESIGN.up().up(), Some(true), 2, 0));
        assert_eq!(
            read_words(sub),
            Settings {
                zoom: Some(Zoom::DESIGN.up().up()),
                dark: Some(true),
                face: Some(2),
                measure: Some(0),
            }
        );
        // A key that was never written is a first run, not a broken one.
        assert_eq!(read_words(&format!("{sub}\\Absent")), Settings::default());
        clear(sub);
    }

    /// What a reader of this key would see under it, asked the way the program asks: through
    /// one open key rather than one value of it at a time.
    fn opened_under(sub: &str) -> Option<PathBuf> {
        Batch::open(sub).and_then(|batch| read_opened(&batch))
    }

    /// The values that belong to a document rather than to the page: which one to come
    /// back to, and where in it.
    #[test]
    fn a_remembered_document_is_only_a_document_while_it_exists() {
        let dir = std::env::temp_dir().join(format!("rubrica-opened-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("a directory to write a page into");
        let file = dir.join("chapter one.md");
        std::fs::write(&file, "# One").expect("a page to point at");

        let sub = "Software\\Rubrica Test Document";
        clear(sub);
        assert_eq!(opened_under(sub), None, "nothing written is nothing to reopen");
        write_text(sub, OPENED, file.to_str().expect("a path in unicode"));
        // The path survives the trip through UTF-16 with its space in it, which is the
        // part a fixed-size numeric buffer could never get wrong.
        assert_eq!(opened_under(sub).as_deref(), Some(file.as_path()));

        // A page written without a place is a page at its top, which is what the reader of
        // a document they have only just opened deserves.
        let (path, anchor) = read_reading(sub).expect("the page just written down");
        assert_eq!(path, file);
        assert_eq!(anchor, 0, "nothing written is no place");

        // A page the reader has moved or deleted is not a thing to start an error over;
        // it is simply no document, and the window opens on the sample.
        std::fs::remove_file(&file).expect("the page taken away again");
        assert_eq!(opened_under(sub), None);
        assert_eq!(read_reading(sub), None, "nor a place in it");
        // So is a directory, which is what a path with its last part cut off reads as.
        write_text(sub, OPENED, dir.to_str().expect("a path in unicode"));
        assert_eq!(opened_under(sub), None, "a folder is not a document");

        let _ = std::fs::remove_dir_all(&dir);
        clear(sub);
    }

    /// The number that belongs to a page rather than to the window.
    #[test]
    fn a_place_belongs_to_the_page_it_was_a_place_in() {
        let dir = std::env::temp_dir().join(format!("rubrica-anchor-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("a directory to write two pages into");
        let one = dir.join("one.md");
        let two = dir.join("two.md");
        std::fs::write(&one, "# One").expect("a page to stand in for the reader's");
        std::fs::write(&two, "# Two").expect("a second one");

        let sub = "Software\\Rubrica Test Anchor";
        clear(sub);
        write_reading(sub, &one, 4_300);
        assert_eq!(read_reading(sub), Some((one, 4_300)));
        // Opening another page puts its top on the record rather than leaving the first
        // page's number behind to be stood in at the wrong place in the wrong prose.
        write_reading(sub, &two, 0);
        assert_eq!(read_reading(sub), Some((two, 0)));

        let _ = std::fs::remove_dir_all(&dir);
        clear(sub);
    }

    #[test]
    fn a_windowed_document_remembers_its_chapter_and_can_clear_it() {
        let path = std::env::temp_dir().join(format!("rubrica-chapter-{}.txt", std::process::id()));
        std::fs::write(&path, "Chapter 3\ntext\n").expect("write document");
        let (sub, _) = document_key(&path);
        clear(&sub);
        record_reading_at(&path, 27, Some(3));
        assert_eq!(document_anchor(&path), Some(27));
        assert_eq!(document_chapter(&path), Some(3));
        record_reading_at(&path, 4, None);
        assert_eq!(document_anchor(&path), Some(4));
        assert_eq!(document_chapter(&path), None);
        let _ = std::fs::remove_file(path);
        clear(&sub);
    }

    /// The window's own numbers, which are signed and have to travel as unsigned ones.
    #[test]
    fn a_frame_round_trips_through_the_numbers_it_is_written_as() {
        // A monitor to the left of the primary one is a negative coordinate, not a mistake,
        // and a `DWORD` carries the sign bit of its own accord.
        for f in [
            Frame { left: -1920, top: 40, width: 1280, height: 720, maximised: true },
            Frame { left: 0, top: 0, width: 900, height: 600, maximised: false },
        ] {
            let (parts, flag) = frame_words(f);
            assert_eq!(frame_of(parts.map(Some), Some(flag)), Some(f), "{f:?}");
        }
        // Four numbers with one of them missing are not a window, and neither is a size of
        // nothing or one that has had its sign bit set by something never meant as a width.
        assert_eq!(frame_of([Some(0), Some(0), None, Some(600)], Some(0)), None);
        assert_eq!(frame_of([Some(0), Some(0), Some(0), Some(600)], Some(0)), None);
        assert_eq!(frame_of([Some(0), Some(0), Some(u32::MAX), Some(600)], Some(0)), None);
        // The flag is 0 or 1, and anything else is not a claim about being maximised.
        assert!(!frame_of([Some(0), Some(0), Some(9), Some(9)], Some(7)).unwrap().maximised);
        // A coordinate past any desktop Windows can be given is brought inside the range
        // `placed` can subtract from rather than believed.
        let wild = frame_of([Some(0x8000_0000), Some(1), Some(100), Some(100)], None).unwrap();
        assert_eq!(wild.left, -LIMIT);
    }

    /// The road out to the registry and back, for the numbers that belong to the window
    /// rather than to the page.
    #[test]
    fn a_window_written_down_is_a_window_read_back() {
        let sub = "Software\\Rubrica Test Window";
        clear(sub);
        assert_eq!(read_frame(sub), None, "a first run has no window to remember");
        let f = Frame { left: -1200, top: 0, width: 1000, height: 700, maximised: true };
        write_frame(sub, f);
        assert_eq!(read_frame(sub), Some(f));
        // Putting a second frame on the record changes the five numbers together, so a
        // window that was maximised and now is not cannot come back half-remembered.
        write_frame(sub, Frame { maximised: false, ..f });
        assert_eq!(read_frame(sub), Some(Frame { maximised: false, ..f }));
        clear(sub);
    }

    #[test]
    fn a_window_that_can_be_reached_is_left_exactly_where_the_reader_put_it() {
        let one = [screen(0, 0, 1920, 1040)];
        assert_eq!(placed(window_at(120, 100, 1080, 800), &one), window_at(120, 100, 1080, 800));
        // Over the bottom edge by most of its own height, and taller than the screen
        // besides: the bar is where it was, so this is an arrangement, not a lost window.
        let tall = window_at(400, 900, 700, 3000);
        assert_eq!(placed(tall, &one), tall, "a reachable window is never rearranged");
        // Two screens and one window across the seam between them, which is a thing readers
        // do on purpose and which the next test takes away again.
        let two = [one[0], screen(1920, 0, 1920, 1040)];
        let across = window_at(2000, 100, 1200, 700);
        assert_eq!(placed(across, &two), across);
        assert_ne!(placed(across, &one), across, "and the same frame is lost on one screen");
    }

    #[test]
    fn a_window_left_on_a_screen_that_has_going_is_brought_back() {
        let one = [screen(0, 0, 1920, 1040)];
        // It was standing on a second monitor, to the right of the one that is left.
        let gone = window_at(2400, 300, 900, 600);
        let back = placed(gone, &one);
        assert_eq!((back.width, back.height), (900, 600), "the size was never the problem");
        assert!(holds(&one[0], caption_of(back), back.top + CAPTION / 2), "{back:?}");
        // Wholly inside, in fact: there is no reason to leave a piece of it behind when the
        // whole of it will fit.
        assert!(back.left >= 0 && back.left + back.width <= 1920);
        assert_eq!(back.top, 300, "the axis that was fine is not touched");

        // The same window on a monitor that used to be above the desk comes back at its top.
        let above = placed(window_at(200, -1200, 800, 600), &one);
        assert_eq!((above.left, above.top), (200, 0));

        // Bigger than the only screen, and nowhere near it to begin with.
        let huge = placed(window_at(3000, 500, 4000, 3000), &one);
        assert_eq!((huge.width, huge.height), (1920, 1040), "narrowed to what there is");
        assert_eq!((huge.left, huge.top), (0, 0));

        // Nearest, not first: with a gap between two screens, a window stranded in it
        // belongs on the one it is closer to, whichever order they were listed in.
        let pair = [screen(0, 0, 1920, 1040), screen(4000, 0, 1920, 1040)];
        assert_eq!(placed(window_at(3500, 300, 800, 600), &pair).left, 4000);
        // And with no screen to be on at all, nothing is invented.
        assert_eq!(placed(gone, &[]), gone);
    }

    /// A screen's work area, cut out of a position and an extent.
    fn screen(left: i32, top: i32, width: i32, height: i32) -> RECT {
        RECT { left, top, right: left + width, bottom: top + height }
    }

    fn window_at(left: i32, top: i32, width: i32, height: i32) -> Frame {
        Frame { left, top, width, height, maximised: false }
    }

    /// The point along a window's title bar that `placed` judges its reachability by --
    /// the same arithmetic, written out here so a change to it has to be said twice.
    fn caption_of(f: Frame) -> i32 {
        f.left + (f.width / 4).max(CAPTION / 2)
    }

    /// A step of the ladder, reached by walking it rather than by naming its ratio.
    fn zoom_of(step: i32) -> Zoom {
        let mut z = Zoom::DESIGN;
        for _ in 0..step.max(0) {
            z = z.up();
        }
        for _ in 0..(-step).max(0) {
            z = z.down();
        }
        z
    }
}
