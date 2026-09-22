//! Getting a selection off the screen and onto the clipboard.
//!
//! A copy is a handshake with the whole desktop rather than a call inside this process:
//! the block of text handed over outlives the window that copied it, so the order of
//! operations matters -- open, empty, hand over memory that is never freed again, close
//! -- and the close has to sit on a path that every failure reaches.

use windows::core::{Error, HRESULT};
use windows::Win32::Foundation::{GetLastError, GlobalFree, HANDLE, HWND};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData,
};
use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};

/// `CF_UNICODETEXT`, the format every text field on Windows pastes from.
///
/// Spelled out instead of imported because the `windows` crate files the constant under
/// `Win32_System_Ole` -- a module some megabytes wide, pulled in for one number -- and
/// that number is part of the published clipboard protocol, fixed since the format list
/// was written.
const CF_UNICODETEXT: u32 = 13;

/// Put `text` on the clipboard for `owner`, replacing whatever was there.
pub fn copy_text(owner: HWND, text: &str) -> Result<(), Error> {
    // The block is sized in bytes and read as UTF-16, so the terminator widens the
    // allocation without becoming a character the reader selected.
    let units: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
    unsafe {
        OpenClipboard(Some(owner))?;
        // Everything between here and the close has to reach the close: a clipboard
        // left open to this process cannot be written by any other application until
        // this one exits, which is the reader's whole desktop for the rest of the run.
        let r = paste(&units);
        let _ = CloseClipboard();
        r
    }
}

/// Fill a global block and pass its handle to the clipboard, which owns it from there.
unsafe fn paste(units: &[u16]) -> Result<(), Error> {
    let bytes = std::mem::size_of_val(units);
    EmptyClipboard()?;
    let block = GlobalAlloc(GMEM_MOVEABLE, bytes)?;
    let dst = GlobalLock(block);
    if dst.is_null() {
        // Read before the block goes back: freeing it overwrites the last error the
        // allocator set, which is the only thing that says why the pin failed.
        let e = Error::from_hresult(HRESULT::from_win32(GetLastError().0));
        let _ = GlobalFree(Some(block));
        return Err(e);
    }
    std::ptr::copy_nonoverlapping(units.as_ptr() as *const u8, dst as *mut u8, bytes);
    // Unlocked before the handover: the clipboard may move the block, and it cannot do
    // so while this process still holds it in place.
    let _ = GlobalUnlock(block);
    if let Err(e) = SetClipboardData(CF_UNICODETEXT, Some(HANDLE(block.0))) {
        // The one path on which the block never became the clipboard's, so it is still
        // this process's to free.
        let _ = GlobalFree(Some(block));
        return Err(e);
    }
    Ok(())
}
