// Put the application icon into the executable's resource section.
//
// `include_bytes!` would not do it: the shell reads Explorer, the taskbar, Alt-Tab
// and a pinned shortcut's icon out of the PE resource directory, so the .ico has to
// be there rather than merely somewhere in the binary. `rc.exe`, which ships with the
// same Windows SDK this project already links Direct2D against, is what puts it there.
//
// `assets/rubrica.ico` is the nine sizes Windows asks for, cut from
// `assets/rubrica-icon.png`. The source is 1254 square but its artwork only spans
// 950x991 of that, and Windows scales the whole canvas rather than the artwork: left
// uncropped the icon reads as a fifth smaller than its neighbours on the desktop. So
// the .ico is cropped to the artwork's edge and refitted to 95% of a square canvas,
// which is where the other desktop icons sit. Both files are committed; replacing the
// artwork means recutting the .ico with that crop, not rebuilding anything else.

fn main() {
    // The build script runs on the host, so `cfg!(windows)` here would answer for the
    // build machine rather than for the target; only the environment variable is right.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    // Naming these opts out of the default "rerun on anything in the package" behaviour:
    // what the build actually reads is this file and the artwork.
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=assets/rubrica.ico");

    let mut res = winresource::WindowsResource::new();
    // Relative to CARGO_MANIFEST_DIR, which `compile` hands to rc.exe as an include
    // path. The id it assigns is 1, the lowest, which is the one the shell takes for
    // the executable -- and the one `src/icon.rs` asks for by name at runtime.
    res.set_icon("assets/rubrica.ico");
    // `WindowsResource` fills the version struct from the package name, which would
    // file the reader's own metadata under "rubrica-app" in Explorer's details pane.
    // Spell the strings out instead, and read the description rather than repeat it.
    res.set("ProductName", "Rubrica")
        .set("FileDescription", &std::env::var("CARGO_PKG_DESCRIPTION").unwrap_or_default())
        .set("OriginalFilename", "rubrica-app.exe")
        .set("LegalCopyright", "Copyright (c) 2026 LiJiaHua1024");
    res.compile().expect("rc.exe could not compile assets/rubrica.ico");
}