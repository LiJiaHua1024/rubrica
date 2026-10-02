//! The application icon, taken back out of this executable's own resource section.
//!
//! `build.rs` is where the .ico went in; this is where it comes from. Reading it out
//! of the module rather than off disk means there is exactly one copy of the artwork
//! and no second location to fall out of step with the first.

use std::sync::OnceLock;

use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    GetSystemMetrics, LoadImageW, HICON, IMAGE_ICON, LR_DEFAULTCOLOR, SM_CXICON, SM_CXSMICON,
    SM_CYICON, SM_CYSMICON, SYSTEM_METRICS_INDEX,
};

/// The id `build.rs` gave the application icon.
///
/// It is an integer rather than a name so that the name *is* the low integer
/// `MAKEINTRESOURCE` expects -- a `PCWSTR` holding `1` is never a dereference, it is
/// the resource id itself. Keeping it at the lowest id the crate assigns is also what
/// makes the shell pick this image as the executable's.
const ID: u16 = 1;

/// The handle one size of the icon was loaded into, loaded once and kept for good.
///
/// A window class goes on pointing at its `hIcon` for as long as the class is
/// registered, and the shell keeps drawing the tray icon for as long as it owns the
/// entry, so nothing here is ever freed: two GDI handles held to the end of the
/// process is the price of never handing out a dangling one.
fn cached(
    cell: &OnceLock<usize>,
    cx: SYSTEM_METRICS_INDEX,
    cy: SYSTEM_METRICS_INDEX,
) -> HICON {
    let address = *cell.get_or_init(|| unsafe {
        LoadImageW(
            GetModuleHandleW(None).ok().map(Into::into),
            windows::core::PCWSTR(ID as *const u16),
            IMAGE_ICON,
            GetSystemMetrics(cx),
            GetSystemMetrics(cy),
            LR_DEFAULTCOLOR,
        )
        .map_or(0, |handle| handle.0 as usize)
    });
    HICON(address as *mut core::ffi::c_void)
}

/// The size a caption or the taskbar asks for: the standard icon, at whatever
/// physical size this display actually uses.
pub fn large() -> HICON {
    static LARGE: OnceLock<usize> = OnceLock::new();
    cached(&LARGE, SM_CXICON, SM_CYICON)
}

/// The size the shell asks for when the window is shrunk: the small icon, which the
/// taskbar and the notification area both use.
pub fn small() -> HICON {
    static SMALL: OnceLock<usize> = OnceLock::new();
    cached(&SMALL, SM_CXSMICON, SM_CYSMICON)
}