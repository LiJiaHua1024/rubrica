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

use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{
    CloseHandle, GetLastError, ERROR_ALREADY_EXISTS, HANDLE, HWND, LPARAM, WPARAM,
};
use windows::Win32::System::DataExchange::COPYDATASTRUCT;
use windows::Win32::System::Threading::CreateMutexW;
use windows::Win32::UI::WindowsAndMessaging::{FindWindowW, SendMessageW, WM_COPYDATA};

/// The mark carried by a forwarded payload, so a `WM_COPYDATA` from anywhere else is
/// dropped rather than parsed.
const TAG: usize = u32::from_be_bytes(*b"RUBR") as usize;

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

/// Hand the documents to the reader that is already open and bring it to the front,
/// saying whether the hand-off landed. An empty list is still a summons: a bare launch
/// means "show me the reader", not "start another one".
pub fn forward(paths: &[PathBuf]) -> bool {
    let Some(hwnd) = reader_window() else { return false };
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
    sent.0 != 0
}

/// The reader's main window, waiting out the moments after a first launch where the
/// mutex is already held but the window does not exist to receive anything yet. A reader
/// that never opens its window is not one worth queueing behind: past two seconds the
/// launch degrades to a second window rather than keep anyone waiting.
fn reader_window() -> Option<HWND> {
    for _ in 0..20 {
        if let Ok(hwnd) = unsafe { FindWindowW(w!("Rubrica.Main"), PCWSTR::null()) } {
            return Some(hwnd);
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    None
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
