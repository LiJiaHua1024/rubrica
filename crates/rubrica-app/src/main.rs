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
mod i18n;
mod images;
mod icon;
mod instance;
mod math;
mod pagination;
mod peek;
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

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

pub(crate) type Error = Box<dyn std::error::Error + Send + Sync>;
pub(crate) type Result<T> = std::result::Result<T, Error>;

fn main() -> Result<()> {
    // DIAGNOSTIC (temporary): a start-up that dies with no window and no message
    // is a panic a GUI process cannot print anywhere. Put it on disk instead.
    std::panic::set_hook(Box::new(|info| {
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(std::env::temp_dir().join("rubrica-panic.log"))
        {
            use std::io::Write as _;
            let _ = writeln!(file, "{info}");
        }
    }));
    // First thing, before an argument is looked at: the start-up clock starts here, so
    // every later mark is a distance from the process rather than from the window.
    view::trace("main-entry");
    // `args_os` rather than `args`: NTFS allows a file name to hold any UTF-16 sequence
    // except the nine characters and the NUL, so a name Explorer can hand over is not
    // necessarily one Rust calls valid Unicode -- and an unpaired surrogate from a
    // POSIX-side tool, an archiver or a low-level restore is exactly such a name. With
    // `panic = "abort"` a panic here is a process that dies before a window exists, and
    // there is no console to say so on.
    let argv: Vec<OsString> = std::env::args_os().collect();
    if has_flag(&argv, "--peek") {
        // The spacebar peek service: a background watcher that previews what the
        // user has selected. It holds no mutex the reader wants and shares no
        // window with it; two of these are one too many, and it knows that.
        return peek::daemon();
    }
    if has_flag(&argv, "--export-png") {
        let output = text_flag(&argv, "--export-png").ok_or("--export-png needs an output path")?;
        let width = flag(&argv, "--export-width").unwrap_or(1080.0);
        let scale = flag(&argv, "--export-scale").unwrap_or(1.0);
        let file = positional(&argv);
        let (path, source) = load(file.as_deref())?;
        let prefs = path.as_deref().map(settings::document).unwrap_or_default();
        let plain = if has_flag(&argv, "--plain") { Some(true) }
            else if has_flag(&argv, "--markdown") { Some(false) }
            else { prefs.plain };
        let profile = text_flag(&argv, "--profile").map(lossy).or_else(|| {
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
            Path::new(output),
            width,
            scale,
            has_flag(&argv, "--dark"),
            plain.unwrap_or_else(|| reading::is_plain(path.as_deref())),
            prefs.text,
            has_flag(&argv, "--source") || prefs.source,
            has_flag(&argv, "--keep-line-breaks")
                || prefs.line_breaks.unwrap_or_else(settings::keep_line_breaks),
            zoom,
            face,
            measure,
            profile.as_deref(),
            !has_flag(&argv, "--no-hyphenate"),
        );
    }
    // PDF export lives behind the `pdf` feature, which carries the whole font and
    // image stack the reader window itself never touches. A build without it simply
    // has no `--export-pdf` to answer.
    #[cfg(feature = "pdf")]
    if has_flag(&argv, "--export-pdf") {
        let output = text_flag(&argv, "--export-pdf").ok_or("--export-pdf needs an output path")?;
        let width = flag(&argv, "--pdf-width").or_else(|| flag(&argv, "--export-width")).unwrap_or(612.0);
        let height = flag(&argv, "--pdf-height").unwrap_or(792.0);
        let file = positional(&argv);
        let (path, source) = load(file.as_deref())?;
        let prefs = path.as_deref().map(settings::document).unwrap_or_default();
        let plain = if has_flag(&argv, "--plain") { Some(true) }
            else if has_flag(&argv, "--markdown") { Some(false) }
            else { prefs.plain };
        let profile = text_flag(&argv, "--profile").map(lossy).or_else(|| {
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
            Path::new(output),
            width,
            height,
            has_flag(&argv, "--dark"),
            plain.unwrap_or_else(|| reading::is_plain(path.as_deref())),
            prefs.text,
            has_flag(&argv, "--source") || prefs.source,
            has_flag(&argv, "--keep-line-breaks")
                || prefs.line_breaks.unwrap_or_else(settings::keep_line_breaks),
            zoom,
            face,
            measure,
            profile.as_deref(),
            !has_flag(&argv, "--no-hyphenate"),
        );
    }
    if has_flag(&argv, "--report") {
        let width = positive("--width", flag(&argv, "--width").unwrap_or(1080.0))?;
        let dpi = positive("--dpi", flag(&argv, "--dpi").unwrap_or(96.0))?;
        let zoom = theme::Zoom::nearest_percent(flag(&argv, "--zoom").unwrap_or(100.0));
        let face = flag(&argv, "--face").map(|v| v as usize).unwrap_or(usize::MAX);
        let measure = flag(&argv, "--measure").map(|v| v as usize).unwrap_or(usize::MAX);
        let file = positional(&argv);
        let (path, source) = load(file.as_deref())?;
        let shown = path.as_deref().and_then(|p| p.to_str());
        let preferences = path.as_deref().map(settings::document).unwrap_or_default();
        let plain = if has_flag(&argv, "--plain") {
            Some(true)
        } else if has_flag(&argv, "--markdown") {
            Some(false)
        } else {
            preferences.plain
        };
        let profile = text_flag(&argv, "--profile").map(lossy).or_else(|| {
            path.as_deref().map(|p| {
                let prefs = settings::document(p);
                crate::profiles::selected(
                    prefs.plain.unwrap_or_else(|| reading::is_plain(Some(p))),
                )
            })
        });
        let keep_line_breaks = has_flag(&argv, "--keep-line-breaks")
            || preferences.line_breaks.unwrap_or_else(settings::keep_line_breaks);
        let source_view = has_flag(&argv, "--source") || preferences.source;
        let hyphenate = !has_flag(&argv, "--no-hyphenate");
        let options = report::Options {
            width,
            dpi,
            hyphenate,
            zoom,
            face,
            measure,
            shapes: has_flag(&argv, "--shapes"),
            keep_line_breaks,
            source_view,
            plain,
            text_options: preferences.text,
            profile,
        };
        return report::report(&source, shown, &options);
    }
    // A file named on the command line is what the reader asked for, and one that cannot
    // be read is worth stopping on -- visibly, though: the window the reader gets shows
    // the reason rather than closing silently. Nothing named is not a request for the
    // sample, either: the reader was here before, and the page they left is the one to
    // come back to. What is named is handed to the window unread: the reader is answered
    // with a window first and a page second, which is the only order that opens fast.
    let paths: Vec<PathBuf> = positionals(&argv);
    // One reader to a session: a second launch is what a double-click produces, but not
    // what it means. The mutex is held to the end of the process, and the system lets it
    // go if the process dies, so a crashed reader is never a locked one.
    let reader = instance::acquire();
    if reader.is_none() {
        // Another reader owns this session. Its window gets what was asked for, and this
        // launch becomes nothing; a bare launch is still a summons, an empty list that
        // only brings the window forward.
        if instance::forward(&paths).is_some() {
            return Ok(());
        }
        // The other reader never opened a window to receive anything, so the launch
        // proceeds as a second one rather than wait out a start that may never land. The
        // mutex stays with the first; this window simply lives without it.
    }
    let (path, extra) = if paths.is_empty() {
        (reopen(), Vec::new())
    } else {
        let mut named = paths.into_iter();
        (named.next(), named.collect())
    };
    if let Err(e) = view::run(path, extra) {
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
fn reopen() -> Option<PathBuf> {
    if let Some(snapshot) = settings::workspace() {
        match snapshot.session.active {
            Some(rubrica_workspace::DocumentRef::Sample) => None,
            Some(rubrica_workspace::DocumentRef::File(file)) if settings::restorable_path(&file.path) => {
                Some(file.path)
            }
            _ => restorable_reading(),
        }
    } else {
        restorable_reading()
    }
}

/// The document the reader was last reading, if it is still where it was left.
fn restorable_reading() -> Option<PathBuf> {
    settings::reading()
        .filter(|(path, _)| settings::restorable_path(path))
        .map(|(path, _)| path)
}

/// Read a document, falling back to the built-in sample.
fn load(file: Option<&Path>) -> Result<(Option<PathBuf>, String)> {
    match file {
        None => Ok((None, sample::DOCUMENT.to_string())),
        Some(p) => match reading::read(p, settings::document(p).encoding).map(|d| d.text) {
            Ok(s) => Ok((Some(p.to_path_buf()), s)),
            Err(e) => Err(view::document_open_error(p, &e).into()),
        },
    }
}

/// A flag's value as text. Every value this program reads is a name, a number or a
/// path, and one that is not UTF-8 is not any of them: `--profile` falls back to the
/// document's own, a number fails to parse and takes its default. The path itself
/// travels as an `OsString` and is never spelled here.
fn lossy(value: &OsStr) -> String {
    value.to_string_lossy().into_owned()
}

/// Whether `argv` carries a flag of no value.
fn has_flag(argv: &[OsString], name: &str) -> bool {
    argv.iter().any(|a| a.to_string_lossy() == name)
}

/// Every document path on the command line, in order, skipping the values that belong to
/// `--width` and friends. A double-click names one; a multi-selection "open with" names
/// several, and each of them is wanted.
fn positionals(argv: &[OsString]) -> Vec<PathBuf> {
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
        let text = a.to_string_lossy();
        if TAKES_VALUE.iter().any(|t| *t == &*text) {
            skip_next = true;
            continue;
        }
        if text == "report" || text.starts_with("--") {
            continue;
        }
        named.push(PathBuf::from(a));
    }
    named
}

/// The document path, skipping the values that belong to `--width` and friends.
fn positional(argv: &[OsString]) -> Option<PathBuf> {
    positionals(argv).into_iter().next()
}

/// The word after a flag, as it was written. A path is handed on as it stands, since a
/// name Windows can produce is not always one Rust calls valid Unicode.
fn text_flag<'a>(argv: &'a [OsString], name: &str) -> Option<&'a OsStr> {
    argv.iter()
        .map(|a| a.to_string_lossy())
        .position(|a| a == name)
        .and_then(|i| argv.get(i + 1))
        .filter(|value| !value.to_string_lossy().starts_with("--"))
        .map(|value| value.as_os_str())
}

fn flag(argv: &[OsString], name: &str) -> Option<f32> {
    argv.iter()
        .map(|a| a.to_string_lossy())
        .position(|a| a == name)
        .and_then(|i| argv.get(i + 1))
        .and_then(|v| v.to_string_lossy().parse().ok())
}

/// A size the report can divide by.
///
/// `k = dpi / 72` and the measure's right edge are both divisors of the numbers the
/// report prints, so a `--dpi 0` or a `--width 0` is not a very small page: it is an
/// infinite edge, a `NaN` fill, every line reported OVER and a hang histogram of
/// infinities. Refused here, where the number came from and where the reader is told
/// what was wrong with it.
fn positive(name: &str, value: f32) -> Result<f32> {
    if value.is_finite() && value > 0.0 {
        Ok(value)
    } else {
        Err(format!("{name} must be a positive number, not {value}").into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::windows::ffi::OsStringExt;

    fn line(args: &[&str]) -> Vec<OsString> {
        args.iter().map(|a| OsString::from(*a)).collect()
    }

    /// A file name holding an unpaired surrogate: NTFS allows it, a POSIX-side tool or a
    /// low-level restore can produce it, and `std::env::args` panics on one. With
    /// `panic = "abort"` that is a process gone before a window exists, so nothing about
    /// the command line is spelled as Unicode on its way to the path it names.
    #[test]
    fn a_document_name_that_is_not_unicode_is_still_a_document() {
        let unpaired = OsString::from_wide(&[0x66, 0x6f, 0x6f, 0xD800, 0x2e, 0x6d, 0x64]);
        assert!(unpaired.to_str().is_none(), "the fixture is not the case it means to be");
        let argv = vec![OsString::from("rubrica.exe"), unpaired.clone()];
        assert_eq!(positionals(&argv), vec![PathBuf::from(unpaired.clone())]);
        assert!(!has_flag(&argv, "--report"), "a name is not a flag");
    }

    /// Flags and their values, asked the way the reader's own start-up asks: a double
    /// click names one path, an export names a second and a number after each.
    #[test]
    fn flags_are_matched_and_their_values_read_from_the_raw_command_line() {
        let argv =
            line(&["rubrica", "--report", "--dpi", "144", "--width", "800", "C:/books/a.md"]);
        assert!(has_flag(&argv, "--report"));
        assert!(!has_flag(&argv, "--export-png"));
        assert_eq!(flag(&argv, "--dpi"), Some(144.0));
        assert_eq!(flag(&argv, "--width"), Some(800.0));
        // A flag's value is the word after it, and another flag is not a value.
        assert_eq!(text_flag(&argv, "--report"), None);
        assert_eq!(text_flag(&argv, "--dpi").map(lossy).as_deref(), Some("144"));
        // The path is a path, and the values belonging to the flags are not documents.
        assert_eq!(positional(&argv), Some(PathBuf::from("C:/books/a.md")));
        assert_eq!(positionals(&argv), vec![PathBuf::from("C:/books/a.md")]);
    }

    /// A page of no size is not a small page, and neither is a page of nonsense: the
    /// report divides by both, so both are refused before anything is measured.
    #[test]
    fn a_page_size_that_nothing_can_divide_by_is_refused() {
        assert_eq!(positive("--dpi", 96.0).unwrap(), 96.0);
        assert_eq!(positive("--width", 1080.0).unwrap(), 1080.0);
        for bad in [0.0, -1.0, f32::NAN, f32::INFINITY] {
            assert!(positive("--dpi", bad).is_err(), "{bad} is not a page size");
            assert!(positive("--width", bad).is_err(), "{bad} is not a page size");
        }
    }
}
