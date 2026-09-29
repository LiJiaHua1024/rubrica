//! One reader per session, and the hand-off to the one already running.
//!
//! Explorer answers a double-click by launching the handler with the file's path, and a
//! second launch would mean a second window over the first. So a new process first asks
//! the session's mutex whether the reader is already here, and if it is, hands the paths
//! to the existing window over `WM_COPYDATA` and exits: "open" means a tab in the window
//! the reader has, not a second window.

use std::ffi::OsString;
use std::os::windows::ffi::{OsStrExt, OsStringExt};
use std::path::PathBuf;
use std::time::Duration;

use windows::core::{w, BOOL};
use windows::Win32::Foundation::{
    CloseHandle, GetLastError, ERROR_ALREADY_EXISTS, HANDLE, HWND, LPARAM, WPARAM,
};
use windows::Win32::System::DataExchange::COPYDATASTRUCT;
use windows::Win32::System::Threading::{AttachThreadInput, CreateMutexW, GetCurrentThreadId};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, FlashWindowEx, GetClassNameW, GetForegroundWindow, GetWindowThreadProcessId,
    IsIconic, IsWindowVisible, SendMessageW, SetForegroundWindow, ShowWindow, FLASHWINFO,
    FLASHW_ALL, FLASHW_TIMERNOFG, SW_RESTORE, WM_COPYDATA,
};

/// The mark carried by a forwarded payload, so a `WM_COPYDATA` from anywhere else is
/// dropped rather than parsed.
const TAG: usize = u32::from_be_bytes(*b"RUBR") as usize;

/// The most a forwarded payload may claim to carry, in bytes. A file list is a few
/// hundred bytes a name, and 64 KB is a hundred-odd of them; `cbData` is the sender's
/// number, and the mark above is a fixed constant any local process can post.
const MAX_FORWARD: u32 = 64 * 1024;

/// Held for the life of the process. The system releases the mutex if the process dies,
/// so a crashed reader is never a locked one.
pub struct SingleInstance(HANDLE);

impl Drop for SingleInstance {
    fn drop(&mut self) {
        unsafe { let _ = CloseHandle(self.0); }
    }
}

/// Claim the session's single reader seat. `None` means a reader is already running and
/// the caller should forward to it instead of building a second window.
pub fn acquire() -> Option<SingleInstance> {
    let handle = unsafe { CreateMutexW(None, false, w!("Local\\Rubrica.Reader")) }.ok()?;
    if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
        unsafe { let _ = CloseHandle(handle); }
        return None;
    }
    Some(SingleInstance(handle))
}

/// Hand the documents to the reader that is already open, saying which window took
/// them. An empty list is still a summons: a bare launch means "show me the reader",
/// not "start another one".
///
/// The reader answers the moment the paths are copied out -- it opens them on its own
/// next pass -- so this costs the caller a window procedure, not a document read.
pub fn forward(paths: &[PathBuf]) -> Option<HWND> {
    let hwnd = reader_window()?;
    // Null-terminated strings, the whole list closed by a second nought.
    let mut payload: Vec<u16> = Vec::new();
    for path in paths {
        payload.extend(path.as_os_str().encode_wide());
        payload.push(0);
    }
    payload.push(0);
    let mut copy = COPYDATASTRUCT {
        dwData: TAG,
        cbData: (payload.len() * 2) as u32,
        lpData: payload.as_mut_ptr().cast(),
    };
    let sent = unsafe {
        SendMessageW(
            hwnd,
            WM_COPYDATA,
            Some(WPARAM(0)),
            Some(LPARAM(&mut copy as *mut COPYDATASTRUCT as isize)),
        )
    };
    (sent.0 != 0).then_some(hwnd)
}

/// The reader's main window, waiting out the moments after a first launch where the
/// mutex is already held but the window does not exist to receive anything yet. A reader
/// that never opens its window is not one worth queueing behind: past two seconds the
/// launch degrades to a second window rather than keep anyone waiting.
pub fn reader_window() -> Option<HWND> {
    for _ in 0..20 {
        if let Some(hwnd) = find_reader_window() {
            return Some(hwnd);
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    None
}

/// The reader class every one of its windows carries.
const READER_CLASS: &str = "Rubrica.Main";

/// What the class walk has found so far.
struct ReaderSearch {
    /// Any window of the class, named so a reader that has not shown itself yet still
    /// counts -- the first launch is exactly that window for a moment.
    any: Option<HWND>,
    /// The one a reader can see, which is the one a document belongs to.
    visible: Option<HWND>,
}

impl ReaderSearch {
    fn new() -> Self {
        Self { any: None, visible: None }
    }
}

unsafe extern "system" fn collect_reader(hwnd: HWND, lp: LPARAM) -> BOOL {
    let search = unsafe { &mut *(lp.0 as *mut ReaderSearch) };
    if search.visible.is_some() {
        // The best answer is already in hand; the rest of the walk costs nothing to
        // skip, and a second visible window has no claim to outrank the first.
        return true.into();
    }
    // Long enough for the class and its terminator, and compared only over the bytes
    // the API says it wrote: a window whose name is longer than the buffer is not this
    // class whatever its beginning looks like.
    let expected: Vec<u16> = READER_CLASS.encode_utf16().collect();
    let mut buffer = [0u16; 64];
    let written = unsafe { GetClassNameW(hwnd, &mut buffer) }.max(0) as usize;
    if written != expected.len() || buffer[..written] != expected[..] {
        return true.into();
    }
    search.any.get_or_insert(hwnd);
    if unsafe { IsWindowVisible(hwnd) }.as_bool() {
        search.visible = Some(hwnd);
    }
    true.into()
}

/// The reader's main window, or `None` when no reader has one yet.
///
/// A session can hold more than one: a launch that arrives while the first reader is
/// still starting up opens its own window rather than queueing behind one that has not
/// appeared (`main`). `FindWindowW` names only the class and the system picks the
/// order, which is the wrong order to hand a document to -- so the window a reader can
/// see wins, and only a reader that has not shown itself yet falls back to the first.
pub fn find_reader_window() -> Option<HWND> {
    let mut search = ReaderSearch::new();
    let state = LPARAM(&mut search as *mut ReaderSearch as isize);
    unsafe { let _ = EnumWindows(Some(collect_reader), state); };
    search.visible.or(search.any)
}

/// Put a window in front, and give it the keyboard, from a thread that has neither.
///
/// Windows grants the foreground to a process that already owns it, that owns the last
/// input, that was started by the window which does, or that has waited out the
/// foreground lock -- and a peek service answers to none of the first three. It is a
/// detached background program, and the key that asked for this was swallowed by its
/// own hook, so the last input belongs to whatever was in front before it. Joining that
/// window's input queue for the length of the call is what makes the request
/// legitimate: for those moments the caller's keys and its foreground are one queue's,
/// and the window it names can be put at the head of it. Joining across processes is
/// what this is for; the system only refuses across desktops, and there is one.
///
/// A process can be refused even having met every condition -- a reader who is holding
/// a key down in another window is a reader who does not want to be taken away from it
/// -- and the system's own answer to that is to blink the taskbar button, so that is
/// what a refusal ends in here too.
pub fn focus(hwnd: HWND) {
    if hwnd.is_invalid() {
        return;
    }
    let previous = unsafe { GetForegroundWindow() };
    if previous == hwnd {
        return;
    }
    unsafe {
        let me = GetCurrentThreadId();
        let theirs = if previous.is_invalid() { 0 } else { GetWindowThreadProcessId(previous, None) };
        // A thread cannot be joined to itself, and a foreground window whose thread has
        // already gone leaves nothing to join.
        let joined = theirs != 0
            && theirs != me
            && AttachThreadInput(me, theirs, true).as_bool();
        if IsIconic(hwnd).as_bool() {
            let _ = ShowWindow(hwnd, SW_RESTORE);
        }
        let fronted = SetForegroundWindow(hwnd).as_bool();
        if joined {
            let _ = AttachThreadInput(me, theirs, false);
        }
        if !fronted {
            let _ = FlashWindowEx(&FLASHWINFO {
                cbSize: std::mem::size_of::<FLASHWINFO>() as u32,
                dwFlags: FLASHW_ALL | FLASHW_TIMERNOFG,
                hwnd,
                uCount: 0,
                dwTimeout: 0,
            });
        }
    }
}

/// The paths in a forwarded payload, taken on the receiving side inside the window
/// procedure. The buffer behind `lp` belongs to the sender and lives only as long as the
/// call, so everything is copied out before a single yielding thing happens.
pub fn take_forward(lp: LPARAM) -> Option<Vec<PathBuf>> {
    if lp.0 == 0 {
        return None;
    }
    let copy = unsafe { *(lp.0 as *const COPYDATASTRUCT) };
    if copy.dwData != TAG {
        return None;
    }
    if copy.cbData == 0 || copy.lpData.is_null() {
        return Some(Vec::new());
    }
    // The length is the sender's to name, and it is what the slice below is built from
    // and what the paths are then cut out of. Windows has already copied `cbData` bytes
    // into this process for the length of the call, so the read itself stays inside
    // them -- but a length of four billion describes no list of documents, and reading
    // it as one walks far past the end of what was actually sent.
    if copy.cbData > MAX_FORWARD {
        return None;
    }
    let words =
        unsafe { std::slice::from_raw_parts(copy.lpData.cast::<u16>(), copy.cbData as usize / 2) };
    let mut paths = Vec::new();
    let mut start = 0;
    for (i, word) in words.iter().enumerate() {
        if *word != 0 {
            continue;
        }
        if i == start {
            break;
        }
        paths.push(PathBuf::from(OsString::from_wide(&words[start..i])));
        start = i + 1;
    }
    Some(paths)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::c_void;

    /// A payload of `wide` under `mark`, posted the way another process would.
    fn posted(mark: usize, wide: &[u16], cb_data: u32) -> Option<Vec<PathBuf>> {
        let mut copy = COPYDATASTRUCT {
            dwData: mark,
            cbData: cb_data,
            lpData: wide.as_ptr() as *mut c_void,
        };
        take_forward(LPARAM(&mut copy as *mut COPYDATASTRUCT as isize))
    }

    /// A list of documents arrives, another program's mark does not, and a length no
    /// list of documents has is refused before anything is read out of it.
    ///
    /// The mark is a fixed published constant, so it is not an accident that this is
    /// reachable: any local process can post a `WM_COPYDATA` carrying it and a `cbData`
    /// of four billion over a buffer of a dozen bytes.
    #[test]
    fn a_payload_larger_than_a_list_of_documents_is_not_taken() {
        let wide: Vec<u16> = "C:\\books\\one.md\0C:\\books\\two.md\0\0".encode_utf16().collect();
        let size = (wide.len() * 2) as u32;
        assert_eq!(
            posted(TAG, &wide, size),
            Some(vec![PathBuf::from("C:\\books\\one.md"), PathBuf::from("C:\\books\\two.md")])
        );
        // A length past the cap is refused before the buffer behind it is read at all,
        // which is the whole point: the sender's number and the sender's allocation are
        // not the same thing, and only one of them is a list of documents.
        assert!(posted(TAG, &wide, MAX_FORWARD + 1).is_none());
        assert!(posted(TAG, &wide, u32::MAX).is_none());
        // Another program's mark is not ours, however well it is sized.
        assert!(posted(TAG + 1, &wide, size).is_none());
    }
}
