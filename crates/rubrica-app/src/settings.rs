//! What the reader has chosen about the page, what they had open and where they left the
//! window, kept for the next one.
//!
//! Ten numbers and one path under `Software\Rubrica` in the current user's registry,
//! which is where a Windows program puts them: no path to choose for a settings file, no
//! format to invent, no dependency to carry, and the palette the system prefers is
//! already read out of the same store.
//!
//! The encoding of a state into those numbers and back is kept apart from the calls
//! that carry them, so what a stored number means can be read -- and tested -- without a
//! live registry in the way.

use std::path::PathBuf;

use windows::core::PCWSTR;
use windows::Win32::Foundation::RECT;
use windows::Win32::System::Registry::{
    HKEY_CURRENT_USER, RegGetValueW, RegSetKeyValueW, RRF_RT_REG_DWORD, RRF_RT_REG_SZ, REG_DWORD,
    REG_SZ,
};

use crate::theme::{Measure, TextFace, Zoom};
use crate::view::utf16;

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

/// One number, or `None` when this machine has nothing of that name to give.
fn word(sub: &str, name: &str) -> Option<u32> {
    let sub = utf16(sub);
    let name = utf16(name);
    let mut value = 0u32;
    let mut len = std::mem::size_of::<u32>() as u32;
    let r = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            PCWSTR(sub.as_ptr()),
            PCWSTR(name.as_ptr()),
            RRF_RT_REG_DWORD,
            None,
            Some(&mut value as *mut u32 as *mut core::ffi::c_void),
            Some(&mut len),
        )
    };
    // A short read is a value that is not a number this program wrote.
    if r.is_err() || len != std::mem::size_of::<u32>() as u32 {
        return None;
    }
    Some(value)
}

/// Whatever was written down under this key, or an empty state for a first run.
fn read_words(sub: &str) -> Settings {
    read(
        word(sub, NAMES[0]),
        word(sub, NAMES[1]),
        word(sub, NAMES[2]),
        word(sub, NAMES[3]),
    )
}

fn write_words(sub: &str, w: &[u32; 4]) {
    for (n, v) in NAMES.iter().zip(w) {
        write_word(sub, n, *v);
    }
}

/// One number, put under `name`.
fn write_word(sub: &str, name: &str, value: u32) {
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
        // A settings write that fails costs the reader their next start-up, which is
        // worth one line on the console even though nothing else can be done about it.
        eprintln!("settings: cannot write {name}: {r:?}");
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

/// One string, or `None` when this machine has nothing of that name to give.
fn text(sub: &str, name: &str) -> Option<String> {
    let sub = utf16(sub);
    let name = utf16(name);
    // The length is asked for first, because a path can be any length and the buffer it
    // is read into has to be cut to size beforehand.
    let mut len = 0u32;
    let r = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            PCWSTR(sub.as_ptr()),
            PCWSTR(name.as_ptr()),
            RRF_RT_REG_SZ,
            None,
            None,
            Some(&mut len),
        )
    };
    if r.is_err() || len == 0 {
        return None;
    }
    let mut units = vec![0u16; len as usize / 2];
    let r = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            PCWSTR(sub.as_ptr()),
            PCWSTR(name.as_ptr()),
            RRF_RT_REG_SZ,
            None,
            Some(units.as_mut_ptr() as *mut core::ffi::c_void),
            Some(&mut len),
        )
    };
    if r.is_err() {
        return None;
    }
    // `len` comes back as the bytes actually copied, terminator included, and a value
    // written by something other than this program may not have one.
    let taken = (len as usize / 2).min(units.len());
    Some(String::from_utf16_lossy(&units[..taken]).trim_end_matches('\0').to_string())
}

/// One string, put under `name`.
///
/// `RegSetKeyValueW` rather than the value-only call, because it also brings the key into
/// being -- which a first run has no other way of getting.
fn write_text(sub: &str, name: &str, raw: &str) {
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
        eprintln!("settings: cannot write {name}: {r:?}");
    }
}

/// The path written down under this key, if it is still a file.
fn read_opened(sub: &str) -> Option<PathBuf> {
    text(sub, OPENED).map(PathBuf::from).filter(|p| p.is_file())
}

/// The page and the place in it, written under this key as one pair.
fn write_reading(sub: &str, path: &std::path::Path, anchor: usize) {
    // A path that is not valid UTF-8 is not a thing worth writing down: it came from a
    // file name this program could not have opened in the first place, and putting it in
    // the registry would only make the next start read it back wrong.
    if let Some(raw) = path.to_str() {
        write_text(sub, OPENED, raw);
        write_word(sub, ANCHOR, anchor as u32);
    }
}

/// The page under this key, if it is still a file, and the place the reader had got to in
/// it -- or the top, for a key with nothing to say.
///
/// The number is not checked against the document's length: a place from a page that has
/// since grown shorter names no line here, and the window that cannot find it stays where
/// it would have started anyway.
fn read_reading(sub: &str) -> Option<(PathBuf, usize)> {
    let path = read_opened(sub)?;
    Some((path, word(sub, ANCHOR).unwrap_or(0) as usize))
}

/// Write down what the reader has open and where in it they are standing, so that the
/// next window starts on their own page rather than on somebody else's sample prose.
///
/// The two go together in one breath because one without the other is a wrong answer: a
/// place kept against a different document is a jump into a paragraph nobody chose, and a
/// page that comes back at its top says the reader never got further than they did.
pub fn record_reading(path: &std::path::Path, anchor: usize) {
    write_reading(SUBKEY, path, anchor);
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
    frame_of(FRAME.map(|name| word(sub, name)), word(sub, MAXIMISED))
}

/// Write down where the window was left, for the next one to be put there.
pub fn record_window(frame: Frame) {
    write_frame(SUBKEY, frame);
}

/// The frame the reader left the window in, or `None` on a machine that has never had one.
pub fn window() -> Option<Frame> {
    read_frame(SUBKEY)
}

#[cfg(test)]
mod tests {    use super::*;
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
        assert_eq!(read_opened(sub), None, "nothing written is nothing to reopen");
        write_text(sub, OPENED, file.to_str().expect("a path in unicode"));
        // The path survives the trip through UTF-16 with its space in it, which is the
        // part a fixed-size numeric buffer could never get wrong.
        assert_eq!(read_opened(sub).as_deref(), Some(file.as_path()));

        // A page written without a place is a page at its top, which is what the reader of
        // a document they have only just opened deserves.
        let (path, anchor) = read_reading(sub).expect("the page just written down");
        assert_eq!(path, file);
        assert_eq!(anchor, 0, "nothing written is no place");

        // A page the reader has moved or deleted is not a thing to start an error over;
        // it is simply no document, and the window opens on the sample.
        std::fs::remove_file(&file).expect("the page taken away again");
        assert_eq!(read_opened(sub), None);
        assert_eq!(read_reading(sub), None, "nor a place in it");
        // So is a directory, which is what a path with its last part cut off reads as.
        write_text(sub, OPENED, dir.to_str().expect("a path in unicode"));
        assert_eq!(read_opened(sub), None, "a folder is not a document");

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
