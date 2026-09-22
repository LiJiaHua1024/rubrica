//! Rubrica: a native Windows Markdown reader.
//!
//! Direct2D surface, DirectWrite-shaped text, line breaks computed by
//! `rubrica-type`'s global solver. No webview anywhere in the process, which is the
//! architectural premise: fine typography requires the break decisions and the
//! glyph advances to come from the same place.

mod clipboard;
mod font;
mod hyphen;
mod images;
mod math;
mod report;
mod sample;
mod tables;
mod theme;
mod view;

use std::path::PathBuf;

pub(crate) type Error = Box<dyn std::error::Error + Send + Sync>;
pub(crate) type Result<T> = std::result::Result<T, Error>;

fn main() -> Result<()> {
    let argv: Vec<String> = std::env::args().collect();
    if argv.iter().any(|a| a == "--report") {
        let width = flag(&argv, "--width").unwrap_or(1080.0);
        let dpi = flag(&argv, "--dpi").unwrap_or(96.0);
        let zoom = theme::Zoom::nearest_percent(flag(&argv, "--zoom").unwrap_or(100.0));
        let file = positional(&argv);
        let (path, source) = load(file.as_deref())?;
        let shown = path.and_then(|p| p.to_str().map(str::to_string));
        let hyphenate = !argv.iter().any(|a| a == "--no-hyphenate");
        return report::report(&source, shown.as_deref(), width, dpi, hyphenate, zoom);
    }
    let arg = std::env::args_os().nth(1).map(PathBuf::from);
    let (path, source) = load(arg.as_ref().and_then(|p| p.to_str()))?;
    view::run(source, path)
}

/// Read a document, falling back to the built-in sample.
fn load(file: Option<&str>) -> Result<(Option<PathBuf>, String)> {
    match file {
        None => Ok((None, sample::DOCUMENT.to_string())),
        Some(p) => match std::fs::read_to_string(p) {
            Ok(s) => Ok((Some(PathBuf::from(p)), s)),
            Err(e) => Err(format!("cannot read {p}: {e}").into()),
        },
    }
}

/// The document path, skipping the values that belong to `--width` and friends.
fn positional(argv: &[String]) -> Option<String> {
    const TAKES_VALUE: [&str; 3] = ["--width", "--dpi", "--zoom"];
    let mut skip_next = false;
    for a in argv.iter().skip(1) {
        if skip_next {
            skip_next = false;
            continue;
        }
        if TAKES_VALUE.contains(&a.as_str()) {
            skip_next = true;
            continue;
        }
        if a == "report" || a.starts_with("--") {
            continue;
        }
        return Some(a.clone());
    }
    None
}

fn flag(argv: &[String], name: &str) -> Option<f32> {
    argv.iter()
        .position(|a| a == name)
        .and_then(|i| argv.get(i + 1))
        .and_then(|v| v.parse().ok())
}
