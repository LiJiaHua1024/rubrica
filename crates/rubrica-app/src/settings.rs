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
use crate::theme::{Measure, TableLines, TextFace, Zoom};
use crate::view::utf16;
use rubrica_workspace::{
    DocumentRef, FileRef, FolderRef, Places, SessionSnapshot, SessionTab, TabKind, Workspace, WorkspaceSnapshot,
};

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
        // The length and the type are asked for together, before anything is allocated:
        // a path can be any length, so the buffer it is read into has to be cut to size
        // beforehand -- and a value of some other type is never measured into a string
        // at all. The key is editable and a `REG_BINARY` under a path's name is a
        // hand-edit away.
        let mut len = 0u32;
        let sought = unsafe {
            RegQueryValueExW(
                self.key,
                PCWSTR(wide.as_ptr()),
                None,
                Some(&mut kind),
                None,
                Some(&mut len),
            )
        };
        if sought != ERROR_SUCCESS || kind != REG_SZ || len == 0 {
            return None;
        }
        // `len` is a byte count and nothing guarantees it is even -- a value written by
        // something other than this program may not be -- so the buffer is rounded up a
        // whole unit. Sizing it as `len / 2` and then telling the registry it had `len`
        // bytes hands it a pointer to an address it does not own and writes one byte
        // past the end of the allocation.
        let mut units = vec![0u16; len as usize / 2 + 1];
        let mut room = (units.len() * 2) as u32;
        let read = unsafe {
            RegQueryValueExW(
                self.key,
                PCWSTR(wide.as_ptr()),
                None,
                Some(&mut kind),
                Some(units.as_mut_ptr() as *mut u8),
                Some(&mut room),
            )
        };
        if read != ERROR_SUCCESS || kind != REG_SZ {
            return None;
        }
        // `room` comes back as the bytes actually copied, terminator included, and a value
        // written by something other than this program may not have one.
        let taken = (room as usize / 2).min(units.len());
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

/// Whether an `<img>` in a document's markup becomes the figure it names, or whether
/// the reader is shown the words its alt text carries instead: a choice of the reader's
/// own menu, and a missing value means on, since a reader of prose wants its pictures.
pub fn show_html_images() -> bool {
    html_images_of(word(SUBKEY, "HtmlImages"))
}

/// What the stored word means on its own: pictures unless the reader said no, and a
/// value this build never wrote -- a hand-edited registry, an older build's -- means the
/// default as well, because a garbage number is not a reader's choice.
fn html_images_of(stored: Option<u32>) -> bool {
    stored != Some(0)
}

pub fn record_show_html_images(show: bool) {
    write_word(SUBKEY, "HtmlImages", u32::from(show));
}

/// Whether a `==marked==` span is drawn as a band of colour, or read as the two equals
/// signs the author typed. The reader's own switch and the same rule as the HTML
/// figures above: a missing value means on, because a reader of prose wants its marks
/// read rather than explained.
pub fn show_highlights() -> bool {
    highlights_of(word(SUBKEY, "Highlights"))
}

/// What the stored word means on its own: marks unless the reader said no, and a value
/// this build never wrote -- a hand-edited registry, an older build's -- means the
/// default as well, because a garbage number is not a reader's choice.
fn highlights_of(stored: Option<u32>) -> bool {
    stored != Some(0)
}

pub fn record_show_highlights(show: bool) {
    write_word(SUBKEY, "Highlights", u32::from(show));
}

/// Which rules a table draws around and inside itself: the grid, or the three rules of
/// a printed table. The reader's own switch, and a missing value -- and a number this
/// build never wrote -- means the grid, because that is the design every page was drawn
/// at until the switch existed.
pub fn table_lines() -> TableLines {
    table_lines_of(word(SUBKEY, "TableLines"))
}

/// What the stored word means on its own: the grid unless the reader asked for the
/// printed table's three rules by name.
fn table_lines_of(stored: Option<u32>) -> TableLines {
    if stored == Some(1) { TableLines::Booktabs } else { TableLines::Grid }
}

pub fn record_table_lines(lines: TableLines) {
    write_word(SUBKEY, "TableLines", u32::from(lines == TableLines::Booktabs));
}

/// Whether an inline code span is drawn with a capsule behind it, or as the plain text
/// its author typed. A missing value means plain -- the design as it stood, and the two
/// switches of this pair are the only ones a missing value reads as off rather than on,
/// because both change how the page has always looked.
pub fn code_capsule() -> bool {
    code_capsule_of(word(SUBKEY, "CodeCapsule"))
}

/// What the stored word means on its own: plain unless the reader asked for a capsule
/// by name, and a number this build never wrote is no request at all.
fn code_capsule_of(stored: Option<u32>) -> bool {
    stored == Some(1)
}

pub fn record_code_capsule(on: bool) {
    write_word(SUBKEY, "CodeCapsule", u32::from(on));
}

/// The colour theme the reader chose for one appearance, where `false` is light and
/// `true` is dark: a theme's name, or `None` when the slot holds nothing and the
/// built-in default pair is drawing. The name is not checked here -- whether this build
/// has a theme by that name is known where the theme is read, and a name that is not
/// installed falls back with the reason said, rather than refusing to open a page.
pub fn theme_slot(dark: bool) -> Option<String> {
    text(SUBKEY, if dark { "ThemeDark" } else { "ThemeLight" })
}

/// Write down one theme slot. The built-in default is written as an empty string rather
/// than deleted: the value is a memory of a choice, and the menu reads an empty memory
/// and no memory alike, but only one of them can be got back from the registry.
pub fn record_theme_slot(dark: bool, name: Option<&str>) {
    write_text(SUBKEY, if dark { "ThemeDark" } else { "ThemeLight" }, name.unwrap_or(""));
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

/// The bits of the status bar, one per item it can show. A bit set is an item on.
pub const STATUS_CHARACTERS: u32 = 1 << 0;
pub const STATUS_WORDS: u32 = 1 << 1;
pub const STATUS_READING_TIME: u32 = 1 << 2;
pub const STATUS_PROGRESS: u32 = 1 << 3;
pub const STATUS_PAGE: u32 = 1 << 4;
pub const STATUS_CHAPTER: u32 = 1 << 5;
pub const STATUS_PARAGRAPHS: u32 = 1 << 6;
pub const STATUS_ENCODING: u32 = 1 << 7;
pub const STATUS_FORMAT: u32 = 1 << 8;
pub const STATUS_SIZE: u32 = 1 << 9;
pub const STATUS_MODIFIED: u32 = 1 << 10;

/// Every bit a stored mask is allowed to carry, in the order the menu offers them --
/// which is also the order the bar prints them.
pub const STATUS_ITEMS: [u32; 11] = [
    STATUS_CHARACTERS, STATUS_WORDS, STATUS_READING_TIME, STATUS_PROGRESS, STATUS_PAGE,
    STATUS_CHAPTER, STATUS_PARAGRAPHS, STATUS_ENCODING, STATUS_FORMAT, STATUS_SIZE,
    STATUS_MODIFIED,
];

const EVERY_STATUS_ITEM: u32 = STATUS_CHARACTERS | STATUS_WORDS | STATUS_READING_TIME
    | STATUS_PROGRESS | STATUS_PAGE | STATUS_CHAPTER | STATUS_PARAGRAPHS | STATUS_ENCODING
    | STATUS_FORMAT | STATUS_SIZE | STATUS_MODIFIED;

/// What a bar shows when the reader has never said: characters, and nothing else.
///
/// The reading experience is the reason the default is one item rather than the whole
/// row -- a bar crowded with numbers is a bar the eye keeps going back to. Every other
/// item is opt-in, and clearing the last of them is a real choice that has to come back
/// as an empty bar rather than as the default again.
const DEFAULT_STATUS_ITEMS: u32 = STATUS_CHARACTERS;

/// Which items the status bar shows, from a stored word. Absence means no choice yet.
fn status_of(value: Option<u32>) -> u32 {
    // Bits this build does not know -- a row an older one wrote, or a hand-edited key --
    // are dropped rather than shown as an item with nothing behind it.
    value.unwrap_or(DEFAULT_STATUS_ITEMS) & EVERY_STATUS_ITEM
}

pub fn status_items() -> u32 {
    status_of(word(SUBKEY, "Status"))
}

pub fn record_status_items(items: u32) {
    write_word(SUBKEY, "Status", items & EVERY_STATUS_ITEM);
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
    // Hashed from the path's own UTF-16 rather than its lossy UTF-8: two names that
    // differ only in bytes UTF-8 cannot carry -- an unpaired surrogate, which NTFS
    // allows -- used to fold into one key and share one book's place and preferences.
    (format!("{SUBKEY}\\Documents\\{:016x}", wide_fingerprint(&absolute, 0)), raw)
}

/// The FNV-1a hash of a path's own UTF-16, salted: the lossless spelling of a name, so
/// no two names a filesystem can hold share a key.
fn wide_fingerprint(path: &std::path::Path, salt: u64) -> u64 {
    use std::os::windows::ffi::OsStrExt;
    let mut hash = 0xcbf29ce484222325u64 ^ salt;
    for unit in path.as_os_str().encode_wide() {
        for byte in unit.to_le_bytes() {
            hash = (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3);
        }
    }
    hash
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
    let absolute = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let raw = absolute.to_string_lossy().into_owned();
    let mut sub = format!("{SUBKEY}\\Documents\\{:016x}", wide_fingerprint(&absolute, 0));
    // The key is 64 bits of a name an author controls, so a collision is a way to
    // overwrite another document's record. The path remembered beside the record is
    // read before the write: a clash moves this document to a salted key rather than
    // taking the other's place.
    let mut salt = 0u64;
    while let Some(batch) = Batch::open(&sub) {
        match batch.text("Path") {
            Some(known) if known != raw => {
                salt += 1;
                sub = format!("{SUBKEY}\\Documents\\{:016x}", wide_fingerprint(&absolute, salt));
            }
            _ => break,
        }
    }
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
const WORKSPACE_MAGIC: &[u8; 4] = b"RWS2";

fn put_workspace_file(out: &mut Vec<u8>, file: &FileRef) -> Option<()> {
    out.extend_from_slice(&file.fingerprint.to_le_bytes());
    put_workspace_path(out, &file.path);
    Some(())
}

/// A path as its own UTF-16, length-prefixed: the session is the one thing that has to
/// survive a name the platform allows and UTF-8 cannot carry, and a path that could not
/// be written used to take the whole session down with it -- silently, because the
/// failure is a note on stderr of a GUI process.
fn put_workspace_path(out: &mut Vec<u8>, path: &std::path::Path) {
    use std::os::windows::ffi::OsStrExt;
    let wide: Vec<u16> = path.as_os_str().encode_wide().collect();
    out.extend_from_slice(&(wide.len() as u32).to_le_bytes());
    for unit in wide {
        out.extend_from_slice(&unit.to_le_bytes());
    }
}

fn take_workspace_file(input: &mut &[u8]) -> Option<FileRef> {
    let raw = input.get(..8)?;
    let fingerprint = u64::from_le_bytes(raw.try_into().ok()?);
    *input = &input[8..];
    Some(FileRef::new(take_workspace_path(input)?, fingerprint))
}

fn take_workspace_path(input: &mut &[u8]) -> Option<std::path::PathBuf> {
    use std::os::windows::ffi::OsStringExt as _;
    let raw = input.get(..4)?;
    let units = u32::from_le_bytes(raw.try_into().ok()?) as usize;
    *input = &input[4..];
    let bytes = input.get(..units * 2)?;
    let mut wide = Vec::with_capacity(units);
    for pair in bytes.as_chunks::<2>().0 {
        wide.push(u16::from_le_bytes(*pair));
    }
    *input = &input[units * 2..];
    Some(std::path::PathBuf::from(std::ffi::OsString::from_wide(&wide)))
}

/// Encode the tab/recent session as one versioned blob. A path is its own UTF-16,
/// length-prefixed, because a reader can have opened a name UTF-8 cannot carry and the
/// session is the one thing that must survive it.
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
        Some(DocumentRef::Start) => out.push(3),
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
        3 => Some(DocumentRef::Start),
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

const PLACES_KEY: &str = "Places";
const PLACES_MAGIC: &[u8; 4] = b"RPL1";

/// Encode the folders a reader keeps and the folders they were last in.
///
/// A value of its own rather than a longer workspace blob: a reader who upgrades must
/// not lose the tabs and recent documents they had, and a blob that grew a tail is one
/// an older build can no longer read at all. Either value can go missing without taking
/// the other with it.
pub fn encode_places(places: &Places) -> Option<Vec<u8>> {
    let mut out = PLACES_MAGIC.to_vec();
    for list in [places.favourites(), places.recent()] {
        out.extend_from_slice(&(list.len() as u32).to_le_bytes());
        for folder in list {
            put_workspace_path(&mut out, &folder.path);
        }
    }
    Some(out)
}

pub fn decode_places(data: &[u8]) -> Option<Places> {
    let mut input = data;
    if input.get(..4)? != PLACES_MAGIC { return None; }
    input = &input[4..];
    let mut lists = Vec::with_capacity(2);
    for _ in 0..2 {
        let count = u32::from_le_bytes(input.get(..4)?.try_into().ok()?) as usize;
        input = &input[4..];
        let mut folders = Vec::with_capacity(count.min(64));
        for _ in 0..count {
            folders.push(FolderRef::new(take_workspace_path(&mut input)?));
        }
        lists.push(folders);
    }
    if !input.is_empty() { return None; }
    let recent = lists.pop()?;
    let favourites = lists.pop()?;
    let mut places = Places::default();
    // Written newest first, so rebuilt oldest first: the ordinary operations then apply
    // the same cap the rest of the state uses, and a hand-edited list that names more
    // folders than the cap allows keeps the ones a reader would have kept.
    for folder in favourites.into_iter().rev() { places.favourite(folder); }
    for folder in recent.into_iter().rev() { places.touch_recent(folder); }
    Some(places)
}

pub fn record_places(places: &Places) -> Result<(), String> {
    let data = encode_places(places).ok_or("the folder list cannot be encoded")?;
    try_write_binary(SUBKEY, PLACES_KEY, &data)
}

/// The folder a path names, spelled the way the shelf and the recent list spell it.
///
/// Canonical, for the reason a document's record is: Explorer, a relative path on the
/// command line and the folder the panel is showing must all name one shelf, and a
/// favourite that a second spelling cannot find is a favourite that was not kept.
pub fn folder_ref(path: &Path) -> FolderRef {
    FolderRef::new(path.canonicalize().unwrap_or_else(|_| path.to_path_buf()))
}

/// The folders a reader keeps, with the ones that are known to be gone left out.
///
/// The registry keeps them either way: a favourite on a drive that is not plugged in
/// comes back when it is, and one that is only unreachable for the moment is kept for
/// the same reason a remembered document is.
pub fn restored_places() -> Places {
    let Some(data) = binary(SUBKEY, PLACES_KEY) else { return Places::default() };
    let Some(stored) = decode_places(&data) else { return Places::default() };
    let mut kept = Places::default();
    // The store is newest first and `favourite` puts each folder at the front, so walk the
    // stored list oldest first: the shelf then comes back in the order it was left, and a
    // list longer than the cap drops the oldest rather than the newest. The same shape as
    // the workspace's own restore.
    for folder in stored.favourites().iter().rev() {
        if restorable_folder(&folder.path) { kept.favourite(folder.clone()); }
    }
    for folder in stored.recent().iter().rev() {
        if restorable_folder(&folder.path) { kept.touch_recent(folder.clone()); }
    }
    kept
}

fn restorable_folder(path: &Path) -> bool {
    match std::fs::metadata(path) {
        Ok(metadata) => metadata.is_dir(),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
        // A permission or network error is not proof that the folder is gone.
        Err(_) => true,
    }
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

    /// The HTML-figures switch, whose absence means on: the one setting here where a
    /// missing value is a choice of its own rather than "no answer".
    #[test]
    fn the_html_images_switch_is_on_until_it_is_turned_off() {
        assert!(html_images_of(None), "a reader who never chose gets the pictures");
        assert!(html_images_of(Some(1)));
        assert!(!html_images_of(Some(0)), "a chosen off is an off");
        assert!(html_images_of(Some(7)), "a number this build never wrote is no choice at all");
        // And the choice travels to the registry and back like every other one.
        let sub = "Software\\Rubrica Test HtmlImages";
        clear(sub);
        write_word(sub, "HtmlImages", 0);
        assert_eq!(word(sub, "HtmlImages"), Some(0), "the off came back as something else");
        clear(sub);
    }

    /// The highlight switch is the second setting whose absence means on, and it is a
    /// differently worded choice: a reader who never chose gets the marks drawn.
    #[test]
    fn the_highlight_switch_is_on_until_it_is_turned_off() {
        assert!(highlights_of(None), "a reader who never chose gets the marks");
        assert!(highlights_of(Some(1)));
        assert!(!highlights_of(Some(0)), "a chosen off is an off");
        assert!(highlights_of(Some(7)), "a number this build never wrote is no choice at all");
        // And the choice travels to the registry and back like every other one.
        let sub = "Software\\Rubrica Test Highlights";
        clear(sub);
        write_word(sub, "Highlights", 0);
        assert_eq!(word(sub, "Highlights"), Some(0), "the off came back as something else");
        clear(sub);
    }

    /// The two drawing switches of a page's own furniture: which rules a table draws,
    /// and whether a code span wears a capsule. Both are choices with a shape every
    /// page was drawn in before they existed, so a value nobody wrote -- and a number
    /// this build never wrote -- means the design as it stood: the grid, and the plain
    /// code colour beside it.
    #[test]
    fn the_table_and_code_switches_lean_back_to_the_way_pages_were_already_drawn() {
        assert_eq!(table_lines_of(None), TableLines::Grid, "a reader who never chose gets the grid");
        assert_eq!(table_lines_of(Some(0)), TableLines::Grid);
        assert_eq!(table_lines_of(Some(1)), TableLines::Booktabs);
        assert_eq!(table_lines_of(Some(7)), TableLines::Grid, "a number this build never wrote is no choice at all");

        assert!(!code_capsule_of(None), "a reader who never chose gets plain code");
        assert!(!code_capsule_of(Some(0)));
        assert!(code_capsule_of(Some(1)));
        assert!(!code_capsule_of(Some(7)), "a number this build never wrote is no choice at all");

        // And both choices travel to the registry and back like every other one.
        let sub = "Software\\Rubrica Test TableLines";
        clear(sub);
        write_word(sub, "TableLines", 1);
        assert_eq!(word(sub, "TableLines"), Some(1), "the choice came back as something else");
        write_word(sub, "CodeCapsule", 0);
        assert_eq!(word(sub, "CodeCapsule"), Some(0), "the off came back as something else");
        clear(sub);
    }

    #[test]
    fn an_unchosen_status_bar_shows_characters_and_a_cleared_one_stays_clear() {
        // Never chosen: the one default item, so a first run has a bar worth reading.
        assert_eq!(status_of(None), STATUS_CHARACTERS);
        // Chosen and then cleared: an empty bar is what the reader asked for, not the
        // default again.
        assert_eq!(status_of(Some(0)), 0);
        // A row of bits round-trips whole, and a bit this build does not know is dropped.
        let chosen = STATUS_WORDS | STATUS_PROGRESS | (1 << 30);
        assert_eq!(status_of(Some(chosen)), STATUS_WORDS | STATUS_PROGRESS);
        assert_eq!(status_of(Some(EVERY_STATUS_ITEM)), EVERY_STATUS_ITEM);
    }

    /// The whole road a setting travels: out to the registry and back. What the tests
    /// above check is the encoding, and an encoding nothing can read is no use to anyone.
    #[test]
    fn a_choice_written_down_is_a_choice_read_back() {        let sub = "Software\\Rubrica Test";
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

    /// A value that is not a string is never measured into a string's buffer.
    ///
    /// The key is editable, and these are the values a hand-edit leaves under a name this
    /// program reads as a path: another type, and a byte count that is not even. Sized as
    /// `len / 2` units and then told the buffer was `len` bytes long, the read hands
    /// `RegQueryValueExW` an address it does not own and writes past the end of it --
    /// one byte for a one-byte value, seven for a seven-byte one, into a heap this
    /// process then keeps using.
    #[test]
    fn a_value_that_is_not_a_string_is_never_sized_into_one() {
        let sub = "Software\\Rubrica Test Odd Text";
        clear(sub);
        assert_eq!(text(sub, "Document"), None, "nothing written is no path");
        try_write_binary(sub, OPENED, b"A").expect("a one-byte value under a path's name");
        assert_eq!(text(sub, OPENED), None);
        try_write_binary(sub, OPENED, b"ABCDEFG").expect("a seven-byte value");
        assert_eq!(text(sub, OPENED), None);
        // And an empty value of the right type is still a path of no length, as it was.
        write_text(sub, OPENED, "C:\\books\\a.md");
        assert_eq!(text(sub, OPENED).as_deref(), Some("C:\\books\\a.md"));
        clear(sub);
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

    /// Two names that differ only in bytes UTF-8 cannot carry are two documents: the
    /// key is hashed from the path's own UTF-16, so an unpaired surrogate no longer
    /// shares a subkey -- and with it another book's place and preferences.
    #[test]
    fn two_names_utf8_cannot_carry_do_not_share_a_document_key() {
        use std::os::windows::ffi::OsStringExt as _;
        let a = std::path::PathBuf::from(std::ffi::OsString::from_wide(
            &[0x66, 0x6f, 0x6f, 0xD800, 0x2e, 0x6d, 0x64],
        ));
        let b = std::path::PathBuf::from(std::ffi::OsString::from_wide(
            &[0x66, 0x6f, 0x6f, 0xDC00, 0x2e, 0x6d, 0x64],
        ));
        assert_ne!(document_key(&a).0, document_key(&b).0);
    }

    #[test]
    fn the_places_blob_round_trips_both_lists_in_order() {
        let mut places = Places::default();
        places.favourite(FolderRef::new("C:\\one"));
        places.favourite(FolderRef::new("C:\\two"));
        places.touch_recent(FolderRef::new("C:\\two"));
        places.touch_recent(FolderRef::new("C:\\three"));
        let data = encode_places(&places).expect("a plain path encodes");
        assert_eq!(data.get(..4), Some(&b"RPL1"[..]));
        let back = decode_places(&data).expect("what was written reads back");
        assert_eq!(back, places, "the two lists and their order survive the trip");
    }

    /// A path UTF-8 cannot carry is still a path: the shelf is written as UTF-16, so an
    /// unpaired surrogate in a folder name does not take the reader's favourites with it.
    #[test]
    fn a_folder_name_utf8_cannot_carry_round_trips() {
        use std::os::windows::ffi::OsStringExt as _;
        let odd = std::path::PathBuf::from(std::ffi::OsString::from_wide(
            &[0x43, 0x3a, 0x5c, 0x66, 0xD800, 0x6f],
        ));
        let mut places = Places::default();
        places.favourite(FolderRef::new(odd.clone()));
        let data = encode_places(&places).expect("a wide path encodes");
        let back = decode_places(&data).expect("and reads back");
        assert_eq!(back.favourites()[0].path, odd);
    }

    #[test]
    fn a_places_blob_that_is_not_ours_is_refused_rather_than_half_read() {
        let mut places = Places::default();
        places.favourite(FolderRef::new("C:\\one"));
        places.touch_recent(FolderRef::new("C:\\two"));
        let good = encode_places(&places).expect("encodes");
        let mut wrong_magic = good.clone();
        wrong_magic[..4].copy_from_slice(b"XXXX");
        assert_eq!(decode_places(&wrong_magic), None);
        assert_eq!(decode_places(&[]), None, "an empty value is not an empty shelf");
        assert_eq!(decode_places(&good[..good.len() - 1]), None, "a truncated path is not a path");
        assert_eq!(decode_places(&[good.as_slice(), b"tail"].concat()), None, "a tail means a format we do not know");
    }

    #[test]
    fn a_places_blob_naming_more_folders_than_the_cap_allows_is_clamped() {
        // Hand-edited registry data is data, not a promise: the cap is applied on the
        // way in, newest first, exactly as it is when a reader adds one.
        let mut out = b"RPL1".to_vec();
        out.extend_from_slice(&(40u32).to_le_bytes());
        for i in 0..40u32 {
            let name = format!("C:\\f{i}");
            let wide: Vec<u16> = name.encode_utf16().collect();
            out.extend_from_slice(&(wide.len() as u32).to_le_bytes());
            for unit in wide { out.extend_from_slice(&unit.to_le_bytes()); }
        }
        out.extend_from_slice(&0u32.to_le_bytes());
        let places = decode_places(&out).expect("a long list still reads");
        assert_eq!(places.favourites().len(), rubrica_workspace::MAX_FAVOURITES);
        assert_eq!(places.favourites()[0].path, std::path::Path::new("C:\\f0"));
    }
}
