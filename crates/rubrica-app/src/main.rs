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
mod instance;
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
    // be read is worth stopping on -- visibly, though: a reader with no window to show an
    // error in would otherwise be a double-click that simply did nothing. Nothing named is
    // not a request for the sample, either: the reader was here before, and the page they
    // left is the one to come back to.
    let paths: Vec<PathBuf> = positionals(&argv).into_iter().map(PathBuf::from).collect();
    // One reader to a session: a second launch is what a double-click produces, but not
    // what it means. The mutex is held to the end of the process, and the system lets it
    // go if the process dies, so a crashed reader is never a locked one.
    let reader = instance::acquire();
    if reader.is_none() {
        // Another reader owns this session. Its window gets what was asked for, and this
        // launch becomes nothing; a bare launch is still a summons, an empty list that
        // only brings the window forward.
        if instance::forward(&paths) {
            return Ok(());
        }
        // The other reader never opened a window to receive anything, so the launch
        // proceeds as a second one rather than wait out a start that may never land. The
        // mutex stays with the first; this window simply lives without it.
    }
    let (path, source, extra) = if paths.is_empty() {
        let (path, source) = reopen();
        (path, source, Vec::new())
    } else {
        let mut opened: Vec<(PathBuf, String)> = Vec::new();
        let mut failures: Vec<String> = Vec::new();
        for path in &paths {
            let prefs = settings::document(path);
            let read = if should_window(path, &prefs) {
                Ok(String::new())
            } else {
                reading::read(path, prefs.encoding).map(|d| d.text)
            };
            match read {
                Ok(text) => opened.push((path.clone(), text)),
                Err(e) => failures.push(view::document_open_error(path, &e)),
            }
        }
        if !failures.is_empty() {
            view::startup_error(&failures);
            std::process::exit(1);
        }
        let mut docs = opened.into_iter();
        let (path, source) = docs.next().expect("at least one document opened");
        (Some(path), source, docs.map(|(p, _)| p).collect())
    };
    if let Err(e) = view::run(source, path, extra) {
        view::startup_error(&[e.to_string()]);
        std::process::exit(1);
    }
    Ok(())
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
            Some(rubrica_workspace::DocumentRef::File(file)) if settings::restorable_path(&file.path) => {
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
    if let Some((path, _)) = settings::reading().filter(|(path, _)| settings::restorable_path(path)) {
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
            Err(e) => Err(view::document_open_error(std::path::Path::new(p), &e).into()),
        },
    }
}

/// Every document path on the command line, in order, skipping the values that belong to
/// `--width` and friends. A double-click names one; a multi-selection "open with" names
/// several, and each of them is wanted.
fn positionals(argv: &[String]) -> Vec<String> {
    const TAKES_VALUE: [&str; 12] = [
        "--width", "--dpi", "--zoom", "--face", "--measure", "--profile",
        "--export-png", "--export-width", "--export-scale", "--export-pdf",
        "--pdf-width", "--pdf-height",
    ];
    let mut skip_next = false;
    let mut named = Vec::new();
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
        named.push(a.clone());
    }
    named
}

/// The document path, skipping the values that belong to `--width` and friends.
fn positional(argv: &[String]) -> Option<String> {
    positionals(argv).into_iter().next()
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
