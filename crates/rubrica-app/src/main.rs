//! Rubrica: a native Windows Markdown reader.
//!
//! Direct2D surface, DirectWrite-shaped text, line breaks computed by
//! `rubrica-type`'s global solver. No webview anywhere in the process, which is the
//! architectural premise: fine typography requires the break decisions and the
//! glyph advances to come from the same place.

#![windows_subsystem = "windows"]

mod cells;
mod clipboard;
mod diagram;
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
mod presets;
mod report;
mod reading;
mod profiles;
mod typography;
mod sample;
mod settings;
mod tables;
mod theme;
mod themes;
mod tree;
mod view;
mod viewer;

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
    if has_flag(&argv, "--list-themes") {
        // What this machine has to draw with, the reader's own files included, and what
        // each appearance is set to: the answer to "why does it look like that", with
        // no window and no wall clock spent on one.
        let shelf = themes::shelf();
        let slots = [settings::theme_slot(false), settings::theme_slot(true)];
        for line in themes::describe(&shelf, slots) {
            println!("{line}");
        }
        return Ok(());
    }
    if has_flag(&argv, "--list-profiles") {
        // What this machine can set a page with: the built-ins, the presets the form has
        // saved, the reader's own files, what each was written over and what each is
        // bound to -- the answer to "why is this page set like that", with no window.
        let shelf = presets::shelf();
        let slots = [profiles::selected(true), profiles::selected(false)];
        for line in presets::describe(&shelf, slots) {
            println!("{line}");
        }
        return Ok(());
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
        let profile = chosen_profile(&argv, path.as_deref(), plain)?;
        let zoom = theme::Zoom::nearest_percent(flag(&argv, "--zoom").unwrap_or(100.0));
        let face = flag(&argv, "--face").map(|v| v as usize);
        let measure = flag(&argv, "--measure").map(|v| v as usize);
        // The theme the reader asked for by name or by path, answered strictly: a
        // headless run that cannot draw with the theme it was told to has no window to
        // explain itself in, and a file that is not the theme asked for is worse than
        // no file.
        let dark = has_flag(&argv, "--dark");
        let word = text_flag(&argv, "--theme").map(lossy);
        let colour = themes::headless(word.as_deref(), dark, dark)?;
        return view::export_png(
            &source,
            path.as_deref(),
            Path::new(output),
            width,
            scale,
            &colour.palette,
            plain.unwrap_or_else(|| reading::is_plain(path.as_deref())),
            prefs.text,
            has_flag(&argv, "--source") || prefs.source,
            has_flag(&argv, "--keep-line-breaks")
                || prefs.line_breaks.unwrap_or_else(settings::keep_line_breaks),
            !has_flag(&argv, "--no-html-images") && settings::show_html_images(),
            !has_flag(&argv, "--no-highlights") && settings::show_highlights(),
            table_lines(&argv)?,
            code_capsule(&argv)?,
            layout_overrides(&argv)?,
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
        let profile = chosen_profile(&argv, path.as_deref(), plain)?;
        let zoom = theme::Zoom::nearest_percent(flag(&argv, "--zoom").unwrap_or(100.0));
        let face = flag(&argv, "--face").map(|v| v as usize);
        let measure = flag(&argv, "--measure").map(|v| v as usize);
        // Strict for the same reason the PNG export is: the PDF is the whole product.
        let dark = has_flag(&argv, "--dark");
        let word = text_flag(&argv, "--theme").map(lossy);
        let colour = themes::headless(word.as_deref(), dark, dark)?;
        return view::export_pdf(
            &source,
            path.as_deref(),
            Path::new(output),
            width,
            height,
            &colour.palette,
            plain.unwrap_or_else(|| reading::is_plain(path.as_deref())),
            prefs.text,
            has_flag(&argv, "--source") || prefs.source,
            has_flag(&argv, "--keep-line-breaks")
                || prefs.line_breaks.unwrap_or_else(settings::keep_line_breaks),
            !has_flag(&argv, "--no-html-images") && settings::show_html_images(),
            !has_flag(&argv, "--no-highlights") && settings::show_highlights(),
            table_lines(&argv)?,
            code_capsule(&argv)?,
            layout_overrides(&argv)?,
            zoom,
            face,
            measure,
            profile.as_deref(),
            !has_flag(&argv, "--no-hyphenate"),
        );
    }
    // The table selection is read on the report's own page, so a run that asked for one
    // without asking for a report has nothing to print it on -- and, left to fall
    // through, would open a window instead: a reader who typed a table number would get
    // a reader window and no fields. Answered here rather than at the dispatch below,
    // which is where every other unrecognised flag ends up.
    if has_flag(&argv, "--table-select") && !has_flag(&argv, "--report") {
        return Err("--table-select reads a table on the report's own page: add --report".into());
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
        let profile = chosen_profile(&argv, path.as_deref(), plain)?;
        let keep_line_breaks = has_flag(&argv, "--keep-line-breaks")
            || preferences.line_breaks.unwrap_or_else(settings::keep_line_breaks);
        let html_images = !has_flag(&argv, "--no-html-images") && settings::show_html_images();
        let show_highlights = !has_flag(&argv, "--no-highlights") && settings::show_highlights();
        let source_view = has_flag(&argv, "--source") || preferences.source;
        let hyphenate = !has_flag(&argv, "--no-hyphenate");
        // The theme in effect, said in one line -- but only when there is one: a report
        // is compared line for line between builds, and a machine drawing with the
        // built-in default has no theme to tell about.
        let dark = has_flag(&argv, "--dark");
        let word = text_flag(&argv, "--theme").map(lossy);
        let colour = themes::headless(word.as_deref(), dark, dark)?;
        // The two drawing switches in effect for this run, from the flags or the
        // reader's remembered choice.
        let table_lines = table_lines(&argv)?;
        let code_capsule = code_capsule(&argv)?;
        // And the four layout numbers, which a profile may also carry: the flags are the
        // more explicit statement and are applied on top of it.
        let layout = layout_overrides(&argv)?;
        let options = report::Options {
            width,
            dpi,
            hyphenate,
            zoom,
            face,
            measure,
            shapes: has_flag(&argv, "--shapes"),
            keep_line_breaks,
            html_images,
            show_highlights,
            table_lines,
            code_capsule,
            layout,
            source_view,
            plain,
            text_options: preferences.text,
            profile,
            theme: colour.asked.then(|| themes::report_line(&colour)),
            table_select: text_flag(&argv, "--table-select").map(lossy),
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
    if let Err(e) = view::run(path, extra, text_flag(&argv, "--theme").map(lossy)) {
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
    const TAKES_VALUE: [&str; 23] = [
        "--width", "--dpi", "--zoom", "--face", "--measure", "--profile",
        "--export-png", "--export-width", "--export-scale", "--export-pdf",
        "--pdf-width", "--pdf-height", "--theme", "--tables", "--inline-code",
        "--marker-inset", "--list-marker-gap", "--list-bullet-size", "--quote-bar-gap",
        "--narrow-align", "--code-ladder", "--code-language", "--table-select",
    ];
    let mut skip_next = false;
    let mut flags_done = false;
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
        if flags_done {
            named.push(PathBuf::from(a));
        } else if text == "--" {
            // The end of the flags: a document whose name begins with one is still a
            // document.
            flags_done = true;
        } else if text.starts_with("--") {
            continue;
        } else {
            named.push(PathBuf::from(a));
        }
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

/// Which rules a table draws, from `--tables` or the reader's remembered choice.
///
/// A word this build does not know is refused rather than guessed at: a headless run
/// has no window to explain itself in, and drawing the wrong set of rules for a name
/// nobody understood is worse than not drawing at all.
fn table_lines(argv: &[OsString]) -> Result<theme::TableLines> {
    match text_flag(argv, "--tables").map(lossy).as_deref() {
        None => Ok(settings::table_lines()),
        Some("grid") => Ok(theme::TableLines::Grid),
        Some("booktabs") => Ok(theme::TableLines::Booktabs),
        Some(other) => Err(format!("--tables wants `grid` or `booktabs`, not `{other}`").into()),
    }
}

/// Whether an inline code span wears a capsule, from `--inline-code` or the reader's
/// remembered choice. Refused rather than guessed for the reason `--tables` is.
fn code_capsule(argv: &[OsString]) -> Result<bool> {
    match text_flag(argv, "--inline-code").map(lossy).as_deref() {
        None => Ok(settings::code_capsule()),
        Some("plain") => Ok(false),
        Some("capsule") => Ok(true),
        Some(other) => Err(format!("--inline-code wants `plain` or `capsule`, not `{other}`").into()),
    }
}

/// How a column narrower than the page's `ragged_below` width is set, from
/// `--narrow-align`.
///
/// Telari's switch, spelled as the choice it makes rather than as the word the layout
/// uses inside: `left` is the design's own answer -- a narrow column falls back to the
/// left margin rather than being stretched -- and `justify` keeps it justified like every
/// other column. Nothing on the command line leaves whatever the page already carries,
/// which is the preset's own answer. Refused rather than guessed at, for the reason
/// `--tables` is.
fn narrow_align(argv: &[OsString]) -> Result<Option<bool>> {
    match text_flag(argv, "--narrow-align").map(lossy).as_deref() {
        None => Ok(None),
        Some("left") => Ok(Some(true)),
        Some("justify") => Ok(Some(false)),
        Some(other) => Err(format!("--narrow-align wants `left` or `justify`, not `{other}`").into()),
    }
}

/// Whether the five code inks stand on the lightness ladder, from `--code-ladder`.
///
/// `design` is the design's own five, which is what every page has been drawn with;
/// `ladder` keeps their hues and their order and sets them a measured distance apart in
/// CIE L*. Refused rather than guessed at, for the reason `--tables` is.
fn code_ladder(argv: &[OsString]) -> Result<Option<bool>> {
    match text_flag(argv, "--code-ladder").map(lossy).as_deref() {
        None => Ok(None),
        Some("design") => Ok(Some(false)),
        Some("ladder") => Ok(Some(true)),
        Some(other) => {
            Err(format!("--code-ladder wants `design` or `ladder`, not `{other}`").into())
        }
    }
}

/// Whether a fence's language is named in the corner of its own panel, from
/// `--code-language`. `none` is the design's own answer -- the page says nothing -- and
/// `corner` names the language the author wrote.
fn code_language(argv: &[OsString]) -> Result<Option<bool>> {
    match text_flag(argv, "--code-language").map(lossy).as_deref() {
        None => Ok(None),
        Some("none") => Ok(Some(false)),
        Some("corner") => Ok(Some(true)),
        Some(other) => {
            Err(format!("--code-language wants `none` or `corner`, not `{other}`").into())
        }
    }
}

/// The typography preset this run sets its page with: the one the reader named, or the
/// one this kind of document is set with.
///
/// A name is answered strictly. A headless run that cannot set the page the way it was
/// told to has no window to explain itself in, and a page drawn with the design's own
/// numbers when another preset was asked for is a page that lies about what it is -- so a
/// word that names nothing here is an error rather than the default. A document is
/// answered the other way, as it always has been: a binding, or a memory of a choice,
/// that has gone stale is not worth refusing to draw a page over.
///
/// `plain` is what the reader has already said about this document, if anything; a `.txt`
/// file is plain text whatever the reader said, which is how the one binding this reader
/// had before preset files existed still means what it meant.
fn chosen_profile(
    argv: &[OsString],
    path: Option<&Path>,
    plain: Option<bool>,
) -> Result<Option<String>> {
    if let Some(word) = text_flag(argv, "--profile") {
        let name = lossy(word);
        if !presets::exists(&name) {
            return Err(format!("no preset here is named `{name}`").into());
        }
        return Ok(Some(name));
    }
    Ok(path.and_then(|p| presets::for_path(p, plain)).map(|preset| preset.name))
}

/// The four layout numbers a typography preset can move, from the command line.
///
/// These are the numbers that decide where a list's marker hangs, how much air stands
/// between it and the words, how large it is drawn and how far a quotation's bar stands
/// from the column it marks. Each is read strictly: a word that is not a number, and a
/// number outside the range the typography form would accept, is refused rather than
/// ignored, because a headless run has no window to explain itself in -- and a page
/// drawn at the design's spacing when the reader asked for another is a page that lies
/// about what it was asked for.
fn layout_overrides(argv: &[OsString]) -> Result<theme::LayoutOverrides> {
    Ok(theme::LayoutOverrides {
        marker_inset_em: layout_number(argv, "--marker-inset", 0.0, 6.0)?,
        list_marker_gap_em: layout_number(argv, "--list-marker-gap", 0.0, 4.0)?,
        list_bullet_size: layout_number(argv, "--list-bullet-size", 0.4, 2.5)?,
        quote_bar_gap_em: layout_number(argv, "--quote-bar-gap", 0.0, 4.0)?,
        narrow_ragged: narrow_align(argv)?,
        code_ladder: code_ladder(argv)?,
        code_lang_label: code_language(argv)?,
    })
}

/// One of them, or `None` when the command line did not name it.
fn layout_number(argv: &[OsString], name: &str, min: f32, max: f32) -> Result<Option<f32>> {
    let Some(raw) = text_flag(argv, name) else {
        return Ok(None);
    };
    let word = lossy(raw);
    let value: f32 = word
        .parse()
        .map_err(|_| -> Error { format!("{name} wants a number, not `{word}`").into() })?;
    if value.is_finite() && (min..=max).contains(&value) {
        Ok(Some(value))
    } else {
        Err(format!("{name} wants a number between {min} and {max}, not {word}").into())
    }
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

    /// A layout number is read strictly. The four switches exist so that a page can be
    /// asked for by name, and a run that quietly drew the design's spacing because
    /// `--list-marker-gap` was handed a word instead of a number would be a page that
    /// lies about what it was asked for -- and a headless run has no window to say so in.
    #[test]
    fn a_layout_number_is_a_number_or_nothing_at_all() {
        let absent = line(&["rubrica", "--report", "a.md"]);
        assert_eq!(layout_number(&absent, "--list-marker-gap", 0.0, 4.0).expect("absent"), None);
        let given = line(&["rubrica", "--report", "--list-marker-gap", "0.5", "a.md"]);
        assert_eq!(layout_number(&given, "--list-marker-gap", 0.0, 4.0).expect("read"), Some(0.5));
        // A word, a NaN, and a number on either side of the range the form would take.
        for bad in ["nope", "nan", "-1", "9"] {
            let argv = line(&["rubrica", "--report", "--list-marker-gap", bad, "a.md"]);
            assert!(
                layout_number(&argv, "--list-marker-gap", 0.0, 4.0).is_err(),
                "{bad} was accepted as a marker gap",
            );
        }
    }

    /// A preset named on the command line is answered strictly, and the remembered one
    /// is not: a headless run that cannot set the page the way it was told to has no
    /// window to explain itself in, while a memory of a choice that has gone stale is not
    /// worth refusing to draw a page over. A run that named no preset at all is the run
    /// this reader has always had.
    #[test]
    fn a_named_preset_is_answered_strictly_and_a_remembered_one_is_not() {
        // The two built-ins are always here, whatever the machine holds.
        let named = line(&["rubrica", "--report", "--profile", "Book", "a.md"]);
        assert_eq!(chosen_profile(&named, None, None).expect("Book").as_deref(), Some("Book"));
        // Case is the way a reader means a name, so it is the way this reader reads one.
        let named = line(&["rubrica", "--report", "--profile", "book", "a.md"]);
        assert_eq!(chosen_profile(&named, None, None).expect("book").as_deref(), Some("book"));
        // A name that means nothing is refused rather than drawn as the default.
        for bad in ["Nobody At All", ""] {
            let named = line(&["rubrica", "--report", "--profile", bad, "a.md"]);
            assert!(chosen_profile(&named, None, None).is_err(), "`{bad}` was taken as a preset");
        }
        // No name, no document: nothing to remember, nothing chosen.
        assert_eq!(chosen_profile(&line(&["rubrica", "--report"]), None, None).expect("none"), None);
        // A document is set with whatever this machine says: a preset bound to the kind
        // of document it is, or the one remembered for that kind. Whatever the answer is,
        // it is a name this machine has -- a name that means nothing would be a page
        // drawn as the design's own with no word said about it.
        let path = PathBuf::from("C:/books/notes.txt");
        let chosen = chosen_profile(&line(&["rubrica", "--report", "notes.txt"]), Some(&path), None)
            .expect("a document is set with something");
        let name = chosen.expect("a preset name");
        assert!(presets::exists(&name), "`{name}` is not a preset this machine has");
        // A `.txt` file is plain text whatever the reader said about it, and a reader's
        // own word about the document is what a binding is matched under.
        let chosen = chosen_profile(
            &line(&["rubrica", "--report", "--markdown", "notes.txt"]),
            Some(&path),
            Some(false),
        )
        .expect("a document is set with something");
        assert!(chosen.is_some_and(|name| presets::exists(&name)));
    }

    /// A document is not a flag: a file called `report` is not a command -- there is no
    /// such command, the flag is `--report` -- and a name that begins with `--` opens
    /// after the `--` that ends the flags.
    #[test]
    fn a_document_is_not_a_flag() {
        assert_eq!(positionals(&line(&["rubrica", "report"])), vec![PathBuf::from("report")]);
        assert_eq!(
            positionals(&line(&["rubrica", "--", "--notes.md"])),
            vec![PathBuf::from("--notes.md")]
        );
        // The flags themselves are unchanged.
        assert_eq!(positionals(&line(&["rubrica", "--report", "a.md"])), vec![PathBuf::from("a.md")]);
        assert_eq!(
            positionals(&line(&["rubrica", "--width", "900", "b.md"])),
            vec![PathBuf::from("b.md")]
        );
    }
}
