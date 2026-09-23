//! Rubrica: a native Windows Markdown reader.
//!
//! Direct2D surface, DirectWrite-shaped text, line breaks computed by
//! `rubrica-type`'s global solver. No webview anywhere in the process, which is the
//! architectural premise: fine typography requires the break decisions and the
//! glyph advances to come from the same place.

#![windows_subsystem = "windows"]

mod clipboard;
mod font;
mod find;
mod highlight;
mod hyphen;
mod images;
mod math;
mod report;
mod reading;
mod profiles;
mod typography;
mod sample;
mod settings;
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
        let face = flag(&argv, "--face").unwrap_or(0.0) as usize;
        let measure = flag(&argv, "--measure").unwrap_or(theme::Measure::DESIGN as f32) as usize;
        let file = positional(&argv);
        let (path, source) = load(file.as_deref())?;
        let shown = path.and_then(|p| p.to_str().map(str::to_string));
        let hyphenate = !argv.iter().any(|a| a == "--no-hyphenate");
        let options = report::Options {
            width,
            dpi,
            hyphenate,
            zoom,
            face,
            measure,
            shapes: argv.iter().any(|a| a == "--shapes"),
            keep_line_breaks: argv.iter().any(|a| a == "--keep-line-breaks"),
            source_view: argv.iter().any(|a| a == "--source"),
            plain: if argv.iter().any(|a| a == "--plain") { Some(true) }
                else if argv.iter().any(|a| a == "--markdown") { Some(false) } else { None },
        };
        return report::report(&source, shown.as_deref(), &options);
    }
    // A file named on the command line is what the reader asked for, and one that cannot
    // be read is worth stopping on. Nothing named is not a request for the sample,
    // though: the reader was here before, and the page they left is the one to come back
    // to.
    let arg = std::env::args_os().nth(1).map(PathBuf::from);
    let (path, source) = match arg.as_ref().and_then(|p| p.to_str()) {
        Some(p) => {
            let path = PathBuf::from(p);
            let source = reading::read(&path, settings::document(&path).encoding)?.text;
            (Some(path), source)
        }
        None => reopen(),
    };
    view::run(source, path)
}

/// What to show when nothing was named: the document the reader had open last, and the
/// sample if there is no such thing or it has outgrown its name.
///
/// A page that has moved or lost its drive since is not worth refusing to start over --
/// a remembered preference is a guess about what the reader wants next, and a guess that
/// cannot be honoured is dropped rather than argued about. The place in the page is not
/// taken from here: the window asks for it against the path it ended up with, so that a
/// document named on the command line gets the same treatment.
fn reopen() -> (Option<PathBuf>, String) {
    if let Some((path, _)) = settings::reading() {
        if let Ok(source) = reading::read(&path, settings::document(&path).encoding).map(|d| d.text) {
            return (Some(path), source);
        }
    }
    (None, sample::DOCUMENT.to_string())
}

/// Read a document, falling back to the built-in sample.
fn load(file: Option<&str>) -> Result<(Option<PathBuf>, String)> {
    match file {
        None => Ok((None, sample::DOCUMENT.to_string())),
        Some(p) => match reading::read(std::path::Path::new(p), reading::Encoding::Auto).map(|d| d.text) {
            Ok(s) => Ok((Some(PathBuf::from(p)), s)),
            Err(e) => Err(format!("cannot read {p}: {e}").into()),
        },
    }
}

/// The document path, skipping the values that belong to `--width` and friends.
fn positional(argv: &[String]) -> Option<String> {
    const TAKES_VALUE: [&str; 5] = ["--width", "--dpi", "--zoom", "--face", "--measure"];
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
