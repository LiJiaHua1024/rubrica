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
mod pagination;
mod report;
mod reading;
mod profiles;
mod typography;
mod sample;
mod settings;
mod tables;
mod theme;
mod tree;
mod view;

use std::path::PathBuf;

pub(crate) type Error = Box<dyn std::error::Error + Send + Sync>;
pub(crate) type Result<T> = std::result::Result<T, Error>;

fn main() -> Result<()> {
    let argv: Vec<String> = std::env::args().collect();
    if argv.iter().any(|a| a == "--export-png") {
        let output = text_flag(&argv, "--export-png").ok_or("--export-png needs an output path")?;
        let width = flag(&argv, "--export-width").unwrap_or(1080.0);
        let scale = flag(&argv, "--export-scale").unwrap_or(1.0);
        let file = positional(&argv);
        let (path, source) = load(file.as_deref())?;
        let prefs = path.as_deref().map(settings::document).unwrap_or_default();
        let plain = if argv.iter().any(|a| a == "--plain") { Some(true) }
            else if argv.iter().any(|a| a == "--markdown") { Some(false) }
            else { prefs.plain };
        let profile = text_flag(&argv, "--profile").or_else(|| {
            path.as_deref().map(|p| crate::profiles::selected(
                prefs.plain.unwrap_or_else(|| reading::is_plain(Some(p))),
            ))
        });
        let zoom = theme::Zoom::nearest_percent(flag(&argv, "--zoom").unwrap_or(100.0));
        let face = flag(&argv, "--face").map(|v| v as usize);
        let measure = flag(&argv, "--measure").map(|v| v as usize);
        return view::export_png(
            &source,
            path.as_deref(),
            std::path::Path::new(&output),
            width,
            scale,
            argv.iter().any(|a| a == "--dark"),
            plain.unwrap_or_else(|| reading::is_plain(path.as_deref())),
            prefs.text,
            argv.iter().any(|a| a == "--source") || prefs.source,
            argv.iter().any(|a| a == "--keep-line-breaks")
                || prefs.line_breaks.unwrap_or_else(settings::keep_line_breaks),
            zoom,
            face,
            measure,
            profile.as_deref(),
            !argv.iter().any(|a| a == "--no-hyphenate"),
        );
    }
    if argv.iter().any(|a| a == "--export-pdf") {
        let output = text_flag(&argv, "--export-pdf").ok_or("--export-pdf needs an output path")?;
        let width = flag(&argv, "--pdf-width").or_else(|| flag(&argv, "--export-width")).unwrap_or(612.0);
        let height = flag(&argv, "--pdf-height").unwrap_or(792.0);
        let file = positional(&argv);
        let (path, source) = load(file.as_deref())?;
        let prefs = path.as_deref().map(settings::document).unwrap_or_default();
        let plain = if argv.iter().any(|a| a == "--plain") { Some(true) }
            else if argv.iter().any(|a| a == "--markdown") { Some(false) }
            else { prefs.plain };
        let profile = text_flag(&argv, "--profile").or_else(|| {
            path.as_deref().map(|p| crate::profiles::selected(
                prefs.plain.unwrap_or_else(|| reading::is_plain(Some(p))),
            ))
        });
        let zoom = theme::Zoom::nearest_percent(flag(&argv, "--zoom").unwrap_or(100.0));
        let face = flag(&argv, "--face").map(|v| v as usize);
        let measure = flag(&argv, "--measure").map(|v| v as usize);
        return view::export_pdf(
            &source,
            path.as_deref(),
            std::path::Path::new(&output),
            width,
            height,
            argv.iter().any(|a| a == "--dark"),
            plain.unwrap_or_else(|| reading::is_plain(path.as_deref())),
            prefs.text,
            argv.iter().any(|a| a == "--source") || prefs.source,
            argv.iter().any(|a| a == "--keep-line-breaks")
                || prefs.line_breaks.unwrap_or_else(settings::keep_line_breaks),
            zoom,
            face,
            measure,
            profile.as_deref(),
            !argv.iter().any(|a| a == "--no-hyphenate"),
        );
    }
    if argv.iter().any(|a| a == "--report") {
        let width = flag(&argv, "--width").unwrap_or(1080.0);
        let dpi = flag(&argv, "--dpi").unwrap_or(96.0);
        let zoom = theme::Zoom::nearest_percent(flag(&argv, "--zoom").unwrap_or(100.0));
        let face = flag(&argv, "--face").map(|v| v as usize).unwrap_or(usize::MAX);
        let measure = flag(&argv, "--measure").map(|v| v as usize).unwrap_or(usize::MAX);
        let file = positional(&argv);
        let (path, source) = load(file.as_deref())?;
        let shown = path.as_deref().and_then(|p| p.to_str());
        let preferences = path.as_deref().map(settings::document).unwrap_or_default();
        let plain = if argv.iter().any(|a| a == "--plain") {
            Some(true)
        } else if argv.iter().any(|a| a == "--markdown") {
            Some(false)
        } else {
            preferences.plain
        };
        let profile = text_flag(&argv, "--profile").or_else(|| {
            path.as_deref().map(|p| {
                let prefs = settings::document(p);
                crate::profiles::selected(
                    prefs.plain.unwrap_or_else(|| reading::is_plain(Some(p))),
                )
            })
        });
        let keep_line_breaks = argv.iter().any(|a| a == "--keep-line-breaks")
            || preferences.line_breaks.unwrap_or_else(settings::keep_line_breaks);
        let source_view = argv.iter().any(|a| a == "--source") || preferences.source;
        let hyphenate = !argv.iter().any(|a| a == "--no-hyphenate");
        let options = report::Options {
            width,
            dpi,
            hyphenate,
            zoom,
            face,
            measure,
            shapes: argv.iter().any(|a| a == "--shapes"),
            keep_line_breaks,
            source_view,
            plain,
            text_options: preferences.text,
            profile,
        };
        return report::report(&source, shown, &options);
    }
    // A file named on the command line is what the reader asked for, and one that cannot
    // be read is worth stopping on. Nothing named is not a request for the sample,
    // though: the reader was here before, and the page they left is the one to come back
    // to.
    let arg = std::env::args_os().nth(1).map(PathBuf::from);
    let (path, source) = match arg.as_ref().and_then(|p| p.to_str()) {
        Some(p) => {
            let path = PathBuf::from(p);
            let prefs = settings::document(&path);
            let source = if should_window(&path, &prefs) {
                String::new()
            } else {
                reading::read(&path, prefs.encoding)?.text
            };
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
    if let Some(snapshot) = settings::workspace() {
        match snapshot.session.active {
            Some(rubrica_workspace::DocumentRef::Sample) => {
                return (None, sample::DOCUMENT.to_string());
            }
            Some(rubrica_workspace::DocumentRef::File(file)) if file.path.is_file() => {
                let path = file.path;
                let prefs = settings::document(&path);
                if should_window(&path, &prefs) {
                    return (Some(path), String::new());
                }
                if let Ok(source) = reading::read(&path, prefs.encoding).map(|d| d.text) {
                    return (Some(path), source);
                }
            }
            _ => {}
        }
    }
    if let Some((path, _)) = settings::reading().filter(|(path, _)| path.is_file()) {
        let prefs = settings::document(&path);
        if should_window(&path, &prefs) {
            return (Some(path), String::new());
        }
        if let Ok(source) = reading::read(&path, prefs.encoding).map(|d| d.text) {
            return (Some(path), source);
        }
    }
    (None, sample::DOCUMENT.to_string())
}

fn should_window(path: &std::path::Path, prefs: &settings::DocumentSettings) -> bool {
    let plain = prefs.plain.unwrap_or_else(|| reading::is_plain(Some(path)));
    plain && !prefs.source && prefs.text.chapters && reading::can_window_text(path, prefs.encoding)
}

/// Read a document, falling back to the built-in sample.
fn load(file: Option<&str>) -> Result<(Option<PathBuf>, String)> {
    match file {
        None => Ok((None, sample::DOCUMENT.to_string())),
        Some(p) => match reading::read(
            std::path::Path::new(p),
            settings::document(std::path::Path::new(p)).encoding,
        )
        .map(|d| d.text)
        {
            Ok(s) => Ok((Some(PathBuf::from(p)), s)),
            Err(e) => Err(format!("cannot read {p}: {e}").into()),
        },
    }
}

/// The document path, skipping the values that belong to `--width` and friends.
fn positional(argv: &[String]) -> Option<String> {
    const TAKES_VALUE: [&str; 12] = [
        "--width", "--dpi", "--zoom", "--face", "--measure", "--profile",
        "--export-png", "--export-width", "--export-scale", "--export-pdf",
        "--pdf-width", "--pdf-height",
    ];
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

fn text_flag(argv: &[String], name: &str) -> Option<String> {
    argv.iter()
        .position(|a| a == name)
        .and_then(|i| argv.get(i + 1))
        .filter(|value| !value.starts_with("--"))
        .cloned()
}

fn flag(argv: &[String], name: &str) -> Option<f32> {
    argv.iter()
        .position(|a| a == name)
        .and_then(|i| argv.get(i + 1))
        .and_then(|v| v.parse().ok())
}
