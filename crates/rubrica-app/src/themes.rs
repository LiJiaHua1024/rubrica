//! The reader's own colour themes: the files under `%APPDATA%\Rubrica\themes`, the six
//! built-ins every file is written against, and the two slots that remember what the
//! reader chose for each appearance.
//!
//! A theme file is TOML, in a small enough corner of it that this module reads it with
//! a parser of its own rather than a dependency the whole build would carry: three
//! tables, quoted strings and `#` comments is the whole of the grammar below.
//!
//! Everything about a theme file is untrusted. It is a document like any other, it
//! arrives from wherever the reader got it, and it is read on a timer -- so the limits
//! here are bounds on damage rather than matters of taste: a file over
//! [`MAX_FILE_BYTES`] is not read at all, no more than [`MAX_FILES`] files are, a
//! `based_on` chain is walked with a ring check and a depth bound, and nothing a file
//! can say reaches a panic.
//!
//! A theme that cannot be used is never half-used: a file with no `name`, no
//! `appearance`, a ring in its `based_on`, or a byte sequence that is not UTF-8 is set
//! aside whole and named with the reason (the reason the menu stays honest), while a
//! single bad key -- a colour that is not a colour -- costs only that key: the rest of
//! the file still says what it says.
//!
//! A theme names a *layer*, not a palette: the keys it leaves out come from whatever
//! it builds on, and a theme that builds on nothing comes from the built-in default of
//! its appearance. That is what makes a seven-line file a theme, and what makes the
//! reader's own "Copy as My Theme" a starting point rather than a wall of numbers: the
//! copy says every key it knows, so nothing about it can move when a later build moves
//! a default.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::view::{Palette, Rgb};

/// The largest theme file that is read. A theme is a page of keys and a comment or
/// two; anything past this is not a theme whatever it is.
pub const MAX_FILE_BYTES: u64 = 64 * 1024;

/// How many theme files are read, in name order. Past this the folder is not a set of
/// themes but a pile: the rest are reported and skipped rather than read on every tick
/// of the clock that watches the folder.
pub const MAX_FILES: usize = 256;

/// How many themes one `based_on` chain may hold, the theme itself included.
const MAX_DEPTH: usize = 8;

/// How long a theme's `name` may be, in characters: it has to fit a menu row and a slot.
const MAX_NAME_CHARS: usize = 80;

/// How long the file name a copied theme becomes may be, before the suffix and the
/// extension: long enough to read, short enough to leave room for ` Copy 2.toml`.
const MAX_STEM_CHARS: usize = 60;

/// Which of the two appearances a theme is drawn for, and which slot remembers it.
///
/// An appearance is not a preference: a light theme on a night page is unreadable, so
/// the name says which page it is for and a slot built for one never takes the other.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Appearance {
    Light,
    Dark,
}

impl Appearance {
    pub fn is_dark(self) -> bool {
        matches!(self, Appearance::Dark)
    }

    pub fn of_dark(dark: bool) -> Appearance {
        if dark {
            Appearance::Dark
        } else {
            Appearance::Light
        }
    }

    /// The word a theme file, a menu and a listing all spell it with.
    pub fn label(self) -> &'static str {
        if self.is_dark() {
            "dark"
        } else {
            "light"
        }
    }

    /// The built-in a slot that names nothing is drawing with. It is a real theme like
    /// any other -- it can be copied, and it is what a `based_on` chain ends at.
    pub fn default_name(self) -> &'static str {
        if self.is_dark() {
            "Default Dark"
        } else {
            "Default Light"
        }
    }

    /// The appearance a file says it is for, or `None` for a word that is not one.
    /// Case is not information here: `Light`, `light` and `LIGHT` are one appearance.
    fn parse(word: &str) -> Option<Appearance> {
        let word = word.trim();
        if word.eq_ignore_ascii_case("light") {
            Some(Appearance::Light)
        } else if word.eq_ignore_ascii_case("dark") {
            Some(Appearance::Dark)
        } else {
            None
        }
    }
}

/// Where a theme came from. A path travels with a file theme because the reader may
/// have two themes of one name, and the menu has to be able to say which one a row
/// would use -- and because the listing has to name the file to open.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Origin {
    BuiltIn,
    File(PathBuf),
}

impl Origin {
    fn path(&self) -> Option<&Path> {
        match self {
            Origin::BuiltIn => None,
            Origin::File(path) => Some(path),
        }
    }

    /// What a listing calls it.
    fn label(&self) -> String {
        match self {
            Origin::BuiltIn => "built-in".to_string(),
            Origin::File(path) => path.display().to_string(),
        }
    }
}

/// The fourteen `[colors]` keys, in the order [`apply_colors`] reads them: the page
/// itself and the inks on it.
const COLOR_KEYS: [&str; 14] = [
    "bg",
    "text",
    "muted",
    "accent",
    "code_bg",
    "page_surface",
    "page_shadow",
    "on_accent",
    "keyword",
    "string",
    "comment",
    "number",
    "ty",
    "highlight",
];

/// The nine `[chrome]` keys, in the order [`apply_chrome`] reads them: the window's own
/// furniture. The last six of them are also the ones a theme may leave out entirely,
/// because the page's own colours already answer for them -- a sidebar in the panel's
/// colour, a tab label in the quiet ink -- and only a theme that wants them *other*
/// than the page has to say so.
const CHROME_KEYS: [&str; 9] = [
    "sidebar",
    "sidebar_text",
    "tab_strip",
    "tab_text",
    "active_tab",
    "active_tab_text",
    "tab_inactive",
    "tab_hover",
    "divider",
];

/// One theme's own answers: `Some` where the file said so, `None` where the key waits
/// for what the theme builds on.
#[derive(Clone, Debug, Default)]
struct Layer {
    colors: [Option<Rgb>; 14],
    chrome: [Option<Rgb>; 9],
}

/// A theme as its file says it, before anything has been resolved.
#[derive(Clone, Debug)]
struct Theme {
    name: String,
    appearance: Appearance,
    based_on: Option<String>,
    origin: Origin,
    layer: Layer,
    /// The keys that were passed over on the way in, one line each, in the words the
    /// reader needs to fix them. Kept with the theme rather than printed, because the
    /// one moment they are worth reading is the one moment somebody is looking at the
    /// theme.
    skipped: Vec<String>,
}

/// A theme with its palette worked out: what a window, an export or a listing draws
/// with.
#[derive(Clone, Debug)]
pub struct Resolved {
    pub name: String,
    pub appearance: Appearance,
    /// The theme this one builds on, kept for the listing: a reader looking at a theme
    /// they did not write needs to know what it was written against.
    pub based_on: Option<String>,
    pub origin: Origin,
    pub palette: Palette,
    pub skipped: Vec<String>,
}

/// Why a slot's name is not the theme on the screen.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SlotFault {
    /// A name nothing in the folder -- and no built-in -- answers to.
    Missing(String),
    /// A name that is a theme, but one for the other appearance.
    WrongAppearance(String),
}

/// Everything the reader's theme folder holds, resolved and in the order the menu shows
/// it: the built-ins first, then the files in name order.
#[derive(Clone, Debug, Default)]
pub struct Shelf {
    /// The folder the files were read from. Empty when this machine has no theme
    /// folder at all, which is not an error: the built-ins are always there.
    pub dir: PathBuf,
    /// The themes that can be drawn with, built-ins and files together.
    pub themes: Vec<Resolved>,
    /// The files that were found and are not themes, each with the reason.
    pub dropped: Vec<(PathBuf, String)>,
    /// The layers themselves, kept so a file outside the folder (a `--theme` given as
    /// a path) can be resolved against the same built-ins and the same file themes.
    layers: Vec<Theme>,
}

impl Shelf {
    fn built_ins(&self) -> usize {
        self.themes.iter().filter(|t| t.origin == Origin::BuiltIn).count()
    }

    fn files(&self) -> usize {
        self.themes.len() - self.built_ins()
    }

    /// The theme of one appearance with this name, case-insensitively -- the way the
    /// menu, a slot and a `--theme` word all mean a name.
    pub fn find(&self, appearance: Appearance, name: &str) -> Option<&Resolved> {
        let name = name.trim();
        self.themes
            .iter()
            .find(|t| t.appearance == appearance && t.name.eq_ignore_ascii_case(name))
    }

    fn find_any(&self, name: &str) -> Option<&Resolved> {
        let name = name.trim();
        self.themes.iter().find(|t| t.name.eq_ignore_ascii_case(name))
    }
}

/// The folder the reader's own themes live in: `%APPDATA%\Rubrica\themes`. `None` on a
/// machine with no `APPDATA`, which is a machine with no reader's folders of any kind.
///
/// The folder is not made here. A reader who has never saved a theme has no folder, and
/// an application that leaves empty folders behind it everywhere it goes is the reason
/// a `%APPDATA%` is a place to look for things you did not ask for.
pub fn dir() -> Option<PathBuf> {
    let base = std::env::var_os("APPDATA")?;
    if base.is_empty() {
        return None;
    }
    Some(PathBuf::from(base).join("Rubrica").join("themes"))
}

/// The themes this machine has, the built-ins first. A folder that cannot be read is a
/// folder with no files in it: the window opens with what it has.
pub fn shelf() -> Shelf {
    match dir() {
        Some(folder) => shelf_in(&folder),
        None => Shelf { themes: built_ins(), layers: built_in_layers(), ..Shelf::default() },
    }
}

/// The themes in one folder, for the tests and for the one reader that asks about a
/// folder other than its own.
pub fn shelf_in(folder: &Path) -> Shelf {
    let mut layers: Vec<Theme> = built_in_layers();
    let mut dropped: Vec<(PathBuf, String)> = Vec::new();
    let mut files = toml_files(folder);
    // The cap is on reading, not on finding: a folder somebody dropped a thousand
    // files into is listed as "256 read, the rest not" rather than walked whole, and
    // the ones that are read are the ones a reader can see come first.
    if files.len() > MAX_FILES {
        for path in files.split_off(MAX_FILES) {
            dropped.push((path, format!("more than {MAX_FILES} files")));
        }
    }
    let mut parsed = Vec::new();
    for path in files {
        match read_theme(&path) {
            Ok(theme) => parsed.push(theme),
            Err(reason) => dropped.push((path, reason)),
        }
    }
    // A name is the whole vocabulary a slot, a menu and a `based_on` have, so a name
    // that is already taken is not one: the second file to claim it is set aside with
    // that reason rather than shadowing the first, which would leave a menu row doing
    // something other than the row beside it that looks the same.
    let mut taken: Vec<String> = layers.iter().map(|t| t.name.to_ascii_lowercase()).collect();
    for theme in parsed {
        let key = theme.name.to_ascii_lowercase();
        if taken.contains(&key) {
            let path = theme.origin.path().map(Path::to_path_buf).unwrap_or_default();
            dropped.push((path, format!("the name `{}` is taken by another theme", theme.name)));
        } else {
            taken.push(key);
            layers.push(theme);
        }
    }
    // Resolve every theme against all of them at once, so the answer for one file does
    // not depend on the order the folder was listed in. A theme whose chain cannot be
    // walked is set aside with the reason and takes no one with it.
    let mut resolved: Vec<(usize, Resolved)> = Vec::new();
    for index in 0..layers.len() {
        match resolve(index, &layers) {
            Ok(theme) => resolved.push((index, theme)),
            Err(reason) => {
                let path = layers[index].origin.path().map(Path::to_path_buf).unwrap_or_default();
                let name = layers[index].name.clone();
                dropped.push((path, format!("{name}: {reason}")));
            }
        }
    }
    let mut themes = Vec::with_capacity(resolved.len());
    let mut kept = Vec::with_capacity(resolved.len());
    for (index, theme) in resolved {
        let layer = layers[index].clone();
        themes.push(theme);
        kept.push(layer);
    }
    Shelf { dir: folder.to_path_buf(), themes, dropped, layers: kept }
}

/// Walk one theme's `based_on` chain from its root down and work out the palette.
fn resolve(index: usize, layers: &[Theme]) -> Result<Resolved, String> {
    let theme = &layers[index];
    // By name, first one wins: built-ins are first in the list, so a file may not
    // stand in for one, which is why a file whose name is a built-in's is set aside
    // above. Duplicated file names are set aside there too, so every chain step here
    // has one answer.
    let by_name: HashMap<String, usize> = {
        let mut map = HashMap::new();
        for (i, layer) in layers.iter().enumerate() {
            map.entry(layer.name.to_ascii_lowercase()).or_insert(i);
        }
        map
    };
    let mut chain = vec![index];
    let mut current = index;
    while let Some(want) = layers[current].based_on.as_deref() {
        let key = want.trim().to_ascii_lowercase();
        let Some(&next) = by_name.get(&key) else {
            return Err(format!("`based_on = \"{want}\"` names no theme this reader has"));
        };
        if chain.contains(&next) {
            return Err(format!("`based_on` comes back round to `{}`", layers[next].name));
        }
        if chain.len() >= MAX_DEPTH {
            return Err(format!("the `based_on` chain is longer than {MAX_DEPTH} themes"));
        }
        if layers[next].appearance != theme.appearance {
            return Err(format!(
                "`based_on` names `{}`, a {} theme, and this one is {}",
                layers[next].name,
                layers[next].appearance.label(),
                theme.appearance.label(),
            ));
        }
        chain.push(next);
        current = next;
    }
    // From the root down: each theme's own answers written over the ones before it, so
    // the theme closest to the end of the chain is the one that decides.
    chain.reverse();
    let mut merged = Layer::default();
    for i in chain {
        let layer = &layers[i].layer;
        for (out, value) in merged.colors.iter_mut().zip(layer.colors) {
            if value.is_some() {
                *out = value;
            }
        }
        for (out, value) in merged.chrome.iter_mut().zip(layer.chrome) {
            if value.is_some() {
                *out = value;
            }
        }
    }
    let mut palette = Palette::of(theme.appearance.is_dark());
    apply_colors(&mut palette, &merged.colors);
    // The furniture that follows the page: settled *after* the page's own colours are
    // final, so a theme that names only `code_bg` gets a sidebar of that colour rather
    // than of the default's -- and a theme that names `sidebar` keeps it, because
    // `apply_chrome` runs last.
    palette.settle();
    apply_chrome(&mut palette, &merged.chrome);
    Ok(Resolved {
        name: theme.name.clone(),
        appearance: theme.appearance,
        based_on: theme.based_on.clone(),
        origin: theme.origin.clone(),
        palette,
        skipped: theme.skipped.clone(),
    })
}

fn apply_colors(p: &mut Palette, v: &[Option<Rgb>; 14]) {
    if let Some(c) = v[0] { p.bg = c; }
    if let Some(c) = v[1] { p.text = c; }
    if let Some(c) = v[2] { p.muted = c; }
    if let Some(c) = v[3] { p.accent = c; }
    if let Some(c) = v[4] { p.code_bg = c; }
    if let Some(c) = v[5] { p.page_surface = c; }
    if let Some(c) = v[6] { p.page_shadow = c; }
    if let Some(c) = v[7] { p.on_accent = c; }
    if let Some(c) = v[8] { p.keyword = c; }
    if let Some(c) = v[9] { p.string = c; }
    if let Some(c) = v[10] { p.comment = c; }
    if let Some(c) = v[11] { p.number = c; }
    if let Some(c) = v[12] { p.ty = c; }
    if let Some(c) = v[13] { p.highlight = c; }
}

fn apply_chrome(p: &mut Palette, v: &[Option<Rgb>; 9]) {
    if let Some(c) = v[0] { p.sidebar = c; }
    if let Some(c) = v[1] { p.sidebar_text = c; }
    if let Some(c) = v[2] { p.tab_strip = c; }
    if let Some(c) = v[3] { p.tab_text = c; }
    if let Some(c) = v[4] { p.active_tab = c; }
    if let Some(c) = v[5] { p.on_tab_active = c; }
    if let Some(c) = v[6] { p.tab_inactive = c; }
    if let Some(c) = v[7] { p.tab_hover = c; }
    if let Some(c) = v[8] { p.divider = c; }
}

/// The six built-in themes, resolved. What a window draws with when nothing is chosen.
pub fn built_ins() -> Vec<Resolved> {
    let layers = built_in_layers();
    (0..layers.len()).filter_map(|i| resolve(i, &layers).ok()).collect()
}

/// The built-in themes as layers, in the order the menu lists them: the default pair
/// first, then the homage pairs.
fn built_in_layers() -> Vec<Theme> {
    vec![
        built_in(Appearance::Light, "Default Light", &[], &[]),
        built_in(Appearance::Dark, "Default Dark", &[], &[]),
        // Mono: a page with no hue in it at all. Nothing here is a colour a reader
        // could name, which is the point -- the page is paper and ink, and the only
        // place a tone is asked to carry meaning is the code panel, where the five
        // inks stay apart by lightness alone.
        built_in(
            Appearance::Light,
            "Mono Light",
            &[
                ("bg", rgb(0xFA, 0xFA, 0xF8)),
                ("text", rgb(0x1B, 0x1B, 0x1B)),
                ("muted", rgb(0x5A, 0x5A, 0x5A)),
                ("accent", rgb(0x3A, 0x3A, 0x3A)),
                ("code_bg", rgb(0xF0, 0xF0, 0xEE)),
                ("page_surface", rgb(0xFF, 0xFF, 0xFF)),
                ("page_shadow", rgb(0x7A, 0x7A, 0x7A)),
                ("on_accent", rgb(0xFF, 0xFF, 0xFF)),
                ("keyword", rgb(0x33, 0x33, 0x33)),
                ("string", rgb(0x55, 0x55, 0x55)),
                ("comment", rgb(0x66, 0x66, 0x66)),
                ("number", rgb(0x4A, 0x4A, 0x4A)),
                ("ty", rgb(0x3D, 0x3D, 0x3D)),
                ("highlight", rgb(0xD8, 0xD8, 0xD2)),
            ],
            &[
                ("tab_strip", rgb(0xEF, 0xEF, 0xEC)),
                ("tab_inactive", rgb(0xF6, 0xF6, 0xF3)),
                ("tab_hover", rgb(0xE9, 0xE9, 0xE5)),
            ],
        ),
        built_in(
            Appearance::Dark,
            "Mono Dark",
            &[
                ("bg", rgb(0x26, 0x26, 0x26)),
                ("text", rgb(0xDC, 0xDC, 0xDC)),
                ("muted", rgb(0xA0, 0xA0, 0xA0)),
                ("accent", rgb(0xC8, 0xC8, 0xC8)),
                ("code_bg", rgb(0x1E, 0x1E, 0x1E)),
                ("page_surface", rgb(0x2C, 0x2C, 0x2C)),
                ("page_shadow", rgb(0x05, 0x05, 0x05)),
                ("on_accent", rgb(0x1A, 0x1A, 0x1A)),
                ("keyword", rgb(0xF2, 0xF2, 0xF2)),
                ("string", rgb(0xC6, 0xC6, 0xC6)),
                ("comment", rgb(0x8A, 0x8A, 0x8A)),
                ("number", rgb(0xD8, 0xD8, 0xD8)),
                ("ty", rgb(0xE4, 0xE4, 0xE4)),
                ("highlight", rgb(0x4A, 0x4A, 0x44)),
            ],
            &[
                ("tab_strip", rgb(0x1A, 0x1A, 0x1A)),
                ("tab_inactive", rgb(0x2A, 0x2A, 0x2A)),
                ("tab_hover", rgb(0x33, 0x33, 0x33)),
            ],
        ),
        // Solarized, after the published palette of that name: its paper, its panels
        // and its five code inks. The quiet inks are a step deeper than the published
        // ones, because the published pair is a low-contrast design and this reader's
        // own bar for a word is 4.5:1 -- the hues are the homage, the depth is the bar.
        // The tab colours follow the same rule: the published blue is a step deeper
        // under the paper's own ink than it is published, because the label on a tab is
        // a word like any other.
        built_in(
            Appearance::Light,
            "Solarized Light",
            &[
                ("bg", rgb(0xFD, 0xF6, 0xE3)),
                ("text", rgb(0x4E, 0x61, 0x67)),
                ("muted", rgb(0x55, 0x69, 0x6F)),
                ("accent", rgb(0x26, 0x8B, 0xD2)),
                ("code_bg", rgb(0xEE, 0xE8, 0xD5)),
                ("page_surface", rgb(0xFF, 0xFC, 0xF0)),
                ("page_shadow", rgb(0xB2, 0xA8, 0x8E)),
                ("on_accent", rgb(0xFD, 0xF6, 0xE3)),
                ("keyword", rgb(0x5C, 0x6B, 0x00)),
                ("string", rgb(0x1A, 0x6E, 0x68)),
                ("comment", rgb(0x55, 0x69, 0x6F)),
                ("number", rgb(0x8E, 0x23, 0x56)),
                ("ty", rgb(0x7A, 0x5C, 0x00)),
                ("highlight", rgb(0xE7, 0xDF, 0xC8)),
            ],
            &[
                ("tab_strip", rgb(0xED, 0xE6, 0xD0)),
                ("tab_inactive", rgb(0xF7, 0xF1, 0xDE)),
                ("tab_hover", rgb(0xE4, 0xDC, 0xC6)),
                ("active_tab", rgb(0x1A, 0x6C, 0xAE)),
                ("on_tab_active", rgb(0xFD, 0xF6, 0xE3)),
            ],
        ),
        built_in(
            Appearance::Dark,
            "Solarized Dark",
            &[
                ("bg", rgb(0x00, 0x2B, 0x36)),
                ("text", rgb(0x93, 0xA1, 0xA1)),
                ("muted", rgb(0x83, 0x94, 0x96)),
                ("accent", rgb(0x26, 0x8B, 0xD2)),
                ("code_bg", rgb(0x07, 0x36, 0x42)),
                ("page_surface", rgb(0x01, 0x30, 0x3C)),
                ("page_shadow", rgb(0x00, 0x0A, 0x0E)),
                ("on_accent", rgb(0x00, 0x2B, 0x36)),
                ("keyword", rgb(0xA3, 0xB8, 0x00)),
                ("string", rgb(0x3C, 0xC0, 0xB4)),
                ("comment", rgb(0x93, 0xA1, 0xA1)),
                ("number", rgb(0xF2, 0x73, 0xB4)),
                ("ty", rgb(0xD2, 0xA6, 0x1E)),
                ("highlight", rgb(0x0A, 0x3B, 0x47)),
            ],
            &[
                ("tab_strip", rgb(0x00, 0x1F, 0x27)),
                ("tab_inactive", rgb(0x07, 0x36, 0x42)),
                ("tab_hover", rgb(0x0D, 0x45, 0x51)),
                // The sidebar is the published `base02` panel, and its words are the
                // published `base1` rather than the quiet ink, which is a step too
                // close to the panel for this reader's bar.
                ("sidebar_text", rgb(0x93, 0xA1, 0xA1)),
                ("active_tab", rgb(0x2E, 0x9A, 0xD9)),
                ("on_tab_active", rgb(0x00, 0x2B, 0x36)),
            ],
        ),
    ]
}

fn built_in(
    appearance: Appearance,
    name: &str,
    colors: &[(&str, Rgb)],
    chrome: &[(&str, Rgb)],
) -> Theme {
    Theme {
        name: name.to_string(),
        appearance,
        based_on: None,
        origin: Origin::BuiltIn,
        layer: layer(colors, chrome),
        skipped: Vec::new(),
    }
}

/// A layer from named keys. A key with no place in the tables is dropped here rather
/// than panicking: these are the built-ins' own tables, where a typo is a test failure
/// and never a reader's problem -- but a lookup is cheaper than a third place to keep
/// the key names in step.
fn layer(colors: &[(&str, Rgb)], chrome: &[(&str, Rgb)]) -> Layer {
    let mut out = Layer::default();
    for (key, value) in colors {
        if let Some(i) = COLOR_KEYS.iter().position(|k| k == key) {
            out.colors[i] = Some(*value);
        }
    }
    for (key, value) in chrome {
        if let Some(i) = CHROME_KEYS.iter().position(|k| k == key) {
            out.chrome[i] = Some(*value);
        }
    }
    out
}

fn rgb(r: u8, g: u8, b: u8) -> Rgb {
    Rgb { r: f32::from(r) / 255.0, g: f32::from(g) / 255.0, b: f32::from(b) / 255.0 }
}

// ---------------------------------------------------------------------------
// The reader's own files
// ---------------------------------------------------------------------------

/// Every `*.toml` file in a folder, in name order. A folder that cannot be read is a
/// folder with nothing in it.
fn toml_files(folder: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(folder) else { return Vec::new() };
    let mut files = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if !path
            .extension()
            .map(|e| e.to_string_lossy().eq_ignore_ascii_case("toml"))
            .unwrap_or(false)
        {
            continue;
        }
        if !entry.file_type().map(|t| t.is_file()).unwrap_or(false) {
            continue;
        }
        files.push(path);
    }
    files.sort_by_key(|path| {
        path.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default()
    });
    files
}

/// One file as a theme, bounded before it is read and parsed after.
fn read_theme(path: &Path) -> Result<Theme, String> {
    let meta = std::fs::metadata(path).map_err(|e| format!("cannot be read: {e}"))?;
    if meta.len() > MAX_FILE_BYTES {
        return Err(format!("larger than {} KiB", MAX_FILE_BYTES / 1024));
    }
    let bytes = std::fs::read(path).map_err(|e| format!("cannot be read: {e}"))?;
    let Ok(text) = String::from_utf8(bytes) else {
        return Err("not UTF-8 text".to_string());
    };
    parse(&text, Origin::File(path.to_path_buf()))
}

/// One theme file as a theme.
///
/// The grammar is TOML's small corner: `[table]` headers, `key = "value"` lines, `#` to
/// the end of the line, and blank lines. Nothing is refused whole for a line this
/// parser cannot read -- that is a skipped key, named in `skipped` and reported where
/// the themes are -- but a file that cannot say *what it is* (no name, no appearance,
/// a name past the bound) is not a theme and is refused with the reason.
fn parse(text: &str, origin: Origin) -> Result<Theme, String> {
    enum Table {
        Meta,
        Colors,
        Chrome,
        Unknown,
    }
    let mut name: Option<String> = None;
    let mut appearance: Option<Appearance> = None;
    // A word that is not an appearance is not a key to skip: the file cannot say what
    // it is for, so the whole file is refused, and the reason has to be the word that
    // was wrong rather than "there was no appearance", which reads as if the key were
    // missing and sends the reader looking for a line that is right there.
    let mut appearance_error: Option<String> = None;
    let mut based_on: Option<String> = None;
    let mut layer = Layer::default();
    let mut skipped: Vec<String> = Vec::new();
    // A key before any table is TOML's own top level, which has no place in a theme
    // and is reported the way an unknown table is.
    let mut table = Table::Unknown;
    for (number, raw) in text.lines().enumerate() {
        let line = strip_comment(raw);
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Some(inner) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            table = match inner.trim() {
                "meta" => Table::Meta,
                "colors" => Table::Colors,
                "chrome" => Table::Chrome,
                other => {
                    skipped.push(format!("[{other}]: not a table this reader knows"));
                    Table::Unknown
                }
            };
            continue;
        }
        let Some((key, value)) = split_assign(line) else {
            skipped.push(format!("line {}: `{line}` is not a `key = value` line", number + 1));
            continue;
        };
        let key = key.trim();
        if key.is_empty() {
            skipped.push(format!("line {}: a key with no name", number + 1));
            continue;
        }
        match table {
            Table::Meta => match key {
                "name" => match string_value(value) {
                    Ok(value) => name = Some(value),
                    Err(reason) => skipped.push(format!("meta.name: {reason}")),
                },
                "appearance" => match string_value(value) {
                    Ok(value) => match Appearance::parse(&value) {
                        Some(parsed) => appearance = Some(parsed),
                        None => {
                            appearance_error = Some(format!(
                                "meta.appearance = \"{value}\": not `light` or `dark`"
                            ));
                        }
                    },
                    Err(reason) => appearance_error = Some(format!("meta.appearance: {reason}")),
                },
                "based_on" => match string_value(value) {
                    Ok(value) if value.trim().is_empty() => based_on = None,
                    Ok(value) => based_on = Some(value.trim().to_string()),
                    Err(reason) => skipped.push(format!("meta.based_on: {reason}")),
                },
                other => skipped.push(format!("meta.{other}: not a key this reader knows")),
            },
            Table::Colors | Table::Chrome => {
                let keys: &[&str] = if matches!(table, Table::Colors) { &COLOR_KEYS } else { &CHROME_KEYS };
                let Some(index) = keys.iter().position(|k| *k == key) else {
                    let table = if matches!(table, Table::Colors) { "colors" } else { "chrome" };
                    skipped.push(format!(
                        "{table}.{key} = {value}: not a key this reader knows"
                    ));
                    continue;
                };
                let table = if matches!(table, Table::Colors) { "colors" } else { "chrome" };
                let slot = if matches!(table, "colors") {
                    &mut layer.colors[index]
                } else {
                    &mut layer.chrome[index]
                };
                match string_value(value) {
                    Ok(text) => match parse_hex(&text) {
                        Some(color) => *slot = Some(color),
                        None => skipped.push(format!(
                            "{table}.{key} = {value}: not a colour like `#rrggbb`"
                        )),
                    },
                    Err(reason) => skipped.push(format!("{table}.{key}: {reason}")),
                }
            }
            Table::Unknown => {
                skipped.push(format!("`{key} = {value}`: no table was open"));
            }
        }
    }
    let Some(name) = name.map(|n| n.trim().to_string()) else {
        return Err("no `name` in the `[meta]` table".to_string());
    };
    if name.is_empty() {
        return Err("`meta.name` is empty".to_string());
    }
    if name.chars().count() > MAX_NAME_CHARS {
        return Err(format!("`meta.name` is longer than {MAX_NAME_CHARS} characters"));
    }
    let Some(appearance) = appearance else {
        return Err(appearance_error.unwrap_or_else(|| "no `appearance` in the `[meta]` table".to_string()));
    };
    Ok(Theme { name, appearance, based_on, origin, layer, skipped })
}

/// A line with its comment cut off. A `#` inside a quoted string is a letter of the
/// string, not a comment: a colour is written with one.
fn strip_comment(line: &str) -> &str {
    let mut quote: Option<char> = None;
    let mut escaped = false;
    for (i, ch) in line.char_indices() {
        match quote {
            Some(q) => {
                if escaped {
                    escaped = false;
                } else if ch == '\\' && q == '"' {
                    escaped = true;
                } else if ch == q {
                    quote = None;
                }
            }
            None => match ch {
                '"' | '\'' => quote = Some(ch),
                '#' => return &line[..i],
                _ => {}
            },
        }
    }
    line
}

/// The two halves of a `key = value` line, the value as it was written.
///
/// An unquoted `=` inside a value is a letter of the value, which is why the first one
/// outside a string is the one that divides: a reader whose theme names a font with an
/// `=` in it gets an error about the value rather than a key that lost half its name.
fn split_assign(line: &str) -> Option<(&str, &str)> {
    let mut quote: Option<char> = None;
    let mut escaped = false;
    for (i, ch) in line.char_indices() {
        match quote {
            Some(q) => {
                if escaped {
                    escaped = false;
                } else if ch == '\\' && q == '"' {
                    escaped = true;
                } else if ch == q {
                    quote = None;
                }
            }
            None => match ch {
                '"' | '\'' => quote = Some(ch),
                '=' => return Some((&line[..i], line[i + 1..].trim())),
                _ => {}
            },
        }
    }
    None
}

/// One TOML string as the text it holds.
fn string_value(raw: &str) -> Result<String, String> {
    match raw.chars().next() {
        Some('\'') => {
            let body = &raw[1..];
            let Some(end) = body.find('\'') else {
                return Err("a string with no closing `'`".to_string());
            };
            if !body[end + 1..].trim().is_empty() {
                return Err("text after the string".to_string());
            }
            Ok(body[..end].to_string())
        }
        Some('"') => {
            let body = &raw[1..];
            let mut out = String::new();
            let mut chars = body.char_indices();
            while let Some((i, ch)) = chars.next() {
                match ch {
                    '"' => {
                        if !body[i + 1..].trim().is_empty() {
                            return Err("text after the string".to_string());
                        }
                        return Ok(out);
                    }
                    '\\' => {
                        let Some((_, escaped)) = chars.next() else {
                            return Err("a `\\` with nothing after it".to_string());
                        };
                        match escaped {
                            '\\' => out.push('\\'),
                            '"' => out.push('"'),
                            'n' => out.push('\n'),
                            't' => out.push('\t'),
                            other => {
                                return Err(format!("`\\{other}` is not an escape this reader knows"));
                            }
                        }
                    }
                    other => out.push(other),
                }
            }
            Err("a string with no closing `\"`".to_string())
        }
        _ => Err("a value that is not a quoted string".to_string()),
    }
}

/// A colour as `#rgb` or `#rrggbb`.
fn parse_hex(text: &str) -> Option<Rgb> {
    let digits = text.trim().strip_prefix('#')?;
    let nibble = |c: char| c.to_digit(16).map(|d| d as u8);
    let mut channels = [0u8; 3];
    match digits.len() {
        3 => {
            for (i, ch) in digits.chars().enumerate() {
                let d = nibble(ch)?;
                channels[i] = d * 17;
            }
        }
        6 => {
            for (i, pair) in digits.as_bytes().chunks(2).enumerate() {
                let text = std::str::from_utf8(pair).ok()?;
                let mut chars = text.chars();
                let hi = nibble(chars.next()?)?;
                let lo = nibble(chars.next()?)?;
                channels[i] = hi * 16 + lo;
            }
        }
        _ => return None,
    }
    Some(rgb(channels[0], channels[1], channels[2]))
}

/// A colour as the six hex digits a theme file writes it with, rounded to the byte the
/// screen draws it in.
///
/// The screen has eight bits a channel and a theme file has the same eight, so this is
/// the point where a palette becomes text and text becomes a palette again: what a copy
/// writes down is what a reload reads back, to the byte.
pub fn hex(c: Rgb) -> String {
    let byte = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    format!("#{:02x}{:02x}{:02x}", byte(c.r), byte(c.g), byte(c.b))
}

// ---------------------------------------------------------------------------
// What a window or a headless run draws with
// ---------------------------------------------------------------------------

/// The reader's own saved choice for one appearance: the name of a theme, or `None`
/// when the slot is empty and the built-in default is drawing. Whitespace-only counts
/// as empty, because a slot written by hand is still a slot.
pub fn slot_name(raw: Option<String>) -> Option<String> {
    let name = raw?;
    let name = name.trim();
    (!name.is_empty()).then(|| name.to_string())
}

/// The theme a slot names, or the built-in default of that appearance with the reason
/// the slot's name is not what is on the screen.
pub fn pick(shelf: &Shelf, appearance: Appearance, name: Option<&str>) -> (Resolved, Option<SlotFault>) {
    let default = || {
        shelf
            .find(appearance, appearance.default_name())
            .or_else(|| shelf.themes.iter().find(|t| t.appearance == appearance))
            .cloned()
    };
    let Some(name) = name.map(str::trim).filter(|name| !name.is_empty()) else {
        return match default() {
            Some(theme) => (theme, None),
            None => (fallback(appearance), None),
        };
    };
    if let Some(found) = shelf.find(appearance, name) {
        return (found.clone(), None);
    }
    let fault = if shelf.find_any(name).is_some() {
        SlotFault::WrongAppearance(name.to_string())
    } else {
        SlotFault::Missing(name.to_string())
    };
    match default() {
        Some(theme) => (theme, Some(fault)),
        None => (fallback(appearance), Some(fault)),
    }
}

/// A palette for a shelf that has lost even its built-ins, which cannot happen and is
/// not a panic if it does: the defaults are the same colours a fresh palette has.
fn fallback(appearance: Appearance) -> Resolved {
    Resolved {
        name: appearance.default_name().to_string(),
        appearance,
        based_on: None,
        origin: Origin::BuiltIn,
        palette: Palette::of(appearance.is_dark()),
        skipped: Vec::new(),
    }
}

/// A theme named by a word from the reader: a name this machine has, or a path to a
/// theme file somewhere else.
///
/// A path is taken as written when it is one -- a file that exists -- and resolved
/// against the same built-ins and the same file themes a name would be, so a copy of a
/// theme carries the same answers wherever it is kept.
pub fn open(shelf: &Shelf, word: &str) -> Result<Resolved, String> {
    let word = word.trim();
    if word.is_empty() {
        return Err("no theme was named".to_string());
    }
    let path = Path::new(word);
    let looks_like_a_path = (word.ends_with(".toml") || word.ends_with(".TOML"))
        || word.contains('\\')
        || word.contains('/');
    if looks_like_a_path && path.is_file() {
        let theme = read_theme(path)?;
        let mut layers = shelf.layers.clone();
        layers.push(theme);
        let index = layers.len() - 1;
        return resolve(index, &layers);
    }
    shelf
        .find_any(word)
        .cloned()
        .ok_or_else(|| format!("no theme here is named `{word}`"))
}

// ---------------------------------------------------------------------------
// Headless: an export, a report, a listing
// ---------------------------------------------------------------------------

/// What a headless run draws with, and the words the report says about it.
#[derive(Clone, Debug)]
pub struct Headless {
    pub palette: Palette,
    pub name: String,
    pub appearance: Appearance,
    pub based_on: Option<String>,
    pub origin: Origin,
    /// Whether the run is not simply drawing with the built-in default: an explicit
    /// `--theme`, or a saved slot naming something. A run that is drawing with the
    /// default has nothing to say and says nothing, which is what keeps a report the
    /// same report on a machine that has never opened the theme menu.
    pub asked: bool,
    /// Why the saved slot's name is not what is drawn with.
    pub fault: Option<SlotFault>,
}

/// The palette a headless run draws with.
///
/// `word` is the `--theme` argument: a name this machine has, or a path to a theme file
/// anywhere. It is answered strictly -- a word that names nothing is an error, because
/// a reader who typed a theme and got the default pair instead is a reader who has been
/// told something false about what they are looking at. A saved slot is answered the
/// other way: the window and the export both fall back to the default and say why,
/// because a slot is a memory of a choice and a memory that has gone stale is not worth
/// refusing to draw a page over.
pub fn headless(word: Option<&str>, dark: bool, dark_explicit: bool) -> Result<Headless, String> {
    let shelf = shelf();
    headless_in(&shelf, word, dark, dark_explicit, crate::settings::theme_slot(dark))
}

/// [`headless`] against a shelf and a slot somebody else read: what the tests ask,
/// without touching the real folder or the real registry.
pub fn headless_in(
    shelf: &Shelf,
    word: Option<&str>,
    dark: bool,
    dark_explicit: bool,
    slot: Option<String>,
) -> Result<Headless, String> {
    let appearance = Appearance::of_dark(dark);
    if let Some(word) = word.map(str::trim).filter(|word| !word.is_empty()) {
        let found = open(shelf, word)?;
        if dark_explicit && found.appearance != appearance {
            return Err(format!(
                "`{word}` is a {} theme, and `--dark` asks for a dark one",
                found.appearance.label(),
            ));
        }
        return Ok(Headless {
            palette: found.palette,
            name: found.name,
            appearance: found.appearance,
            based_on: found.based_on,
            origin: found.origin,
            asked: true,
            fault: None,
        });
    }
    let slot = slot_name(slot);
    let asked = slot.is_some();
    let (theme, fault) = pick(shelf, appearance, slot.as_deref());
    Ok(Headless {
        palette: theme.palette,
        name: theme.name,
        appearance: theme.appearance,
        based_on: theme.based_on,
        origin: theme.origin,
        asked,
        fault,
    })
}

/// What a report says about the theme in effect, in one line.
pub fn report_line(headless: &Headless) -> String {
    let mut line = format!("{} ({})", headless.name, headless.origin.label());
    if let Some(based_on) = &headless.based_on {
        line.push_str(&format!(", based on {based_on}"));
    }
    if let Some(fault) = &headless.fault {
        match fault {
            SlotFault::Missing(name) => line.push_str(&format!("; `{name}` is not installed")),
            SlotFault::WrongAppearance(name) => {
                line.push_str(&format!("; `{name}` is a {} theme", other(headless.appearance)));
            }
        }
    }
    line
}

fn other(appearance: Appearance) -> &'static str {
    Appearance::of_dark(!appearance.is_dark()).label()
}

// ---------------------------------------------------------------------------
// Copy as My Theme
// ---------------------------------------------------------------------------

/// Write a resolved theme into the reader's folder as a file of its own, and say what
/// it was called.
///
/// The copy is every colour, not the differences: a starting point the reader can edit
/// without a second window open on what it was built from. `based_on` is left out for
/// the same reason -- a full theme has nothing to inherit.
pub fn copy_into(folder: &Path, shelf: &Shelf, theme: &Resolved) -> Result<(PathBuf, String), String> {
    std::fs::create_dir_all(folder)
        .map_err(|e| format!("cannot make the themes folder: {e}"))?;
    let name = unique_name(shelf, &format!("{} Copy", copy_root(&theme.name)))?;
    let stem = file_stem(&name);
    let path = unique_path(folder, &stem)?;
    let text = theme_text(&name, theme);
    std::fs::write(&path, text).map_err(|e| format!("cannot write {}: {e}", path.display()))?;
    Ok((path, name))
}

/// The name a copy counts from: `X` for `X`, and also for `X Copy` and `X Copy 2`.
///
/// Copying a copy is how a reader ends up with one, and the second copy of a theme is
/// the second copy of that theme, not a copy of its copy: the suffix is counted rather
/// than stacked, so the names stay names a reader can tell apart at a glance.
fn copy_root(name: &str) -> &str {
    let trimmed = name.trim_end();
    if let Some(root) = trimmed.strip_suffix(" Copy") {
        return root.trim_end();
    }
    if let Some((head, tail)) = trimmed.rsplit_once(' ') {
        if !tail.is_empty() && tail.chars().all(|c| c.is_ascii_digit()) {
            if let Some(root) = head.trim_end().strip_suffix(" Copy") {
                return root.trim_end();
            }
        }
    }
    trimmed
}

/// A name the shelf does not already hold: `X`, then `X 2`, `X 3`, ...
fn unique_name(shelf: &Shelf, base: &str) -> Result<String, String> {
    let trimmed = base.trim();
    let mut base: String = trimmed.chars().take(MAX_NAME_CHARS - 6).collect();
    base = base.trim_end().to_string();
    if base.is_empty() {
        base = "Theme".to_string();
    }
    let taken = |name: &str| shelf.themes.iter().any(|t| t.name.eq_ignore_ascii_case(name));
    for n in 1..1000u32 {
        let candidate = if n == 1 { base.clone() } else { format!("{base} {n}") };
        if !taken(&candidate) {
            return Ok(candidate);
        }
    }
    Err(format!("there are already too many themes named `{base}`"))
}

/// The file name a theme's own name becomes: Windows' forbidden characters and the
/// controls come out as `-`, and the result is bounded, because a name is not a file
/// name and the two are allowed to disagree -- the name in the file is the one that
/// matters.
fn file_stem(name: &str) -> String {
    let mut out = String::new();
    for ch in name.chars().take(MAX_STEM_CHARS) {
        if ch.is_control() || matches!(ch, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*') {
            out.push('-');
        } else {
            out.push(ch);
        }
    }
    let out = out.trim().trim_end_matches('.').trim().to_string();
    if out.is_empty() {
        "theme".to_string()
    } else {
        out
    }
}

/// The first file name in a folder that is not taken, so a copy of a copy is a copy and
/// not a second file of the same name.
fn unique_path(folder: &Path, stem: &str) -> Result<PathBuf, String> {
    let taken = |path: &Path| path.exists();
    let first = folder.join(format!("{stem}.toml"));
    if !taken(&first) {
        return Ok(first);
    }
    for n in 2..1000u32 {
        let path = folder.join(format!("{stem} {n}.toml"));
        if !taken(&path) {
            return Ok(path);
        }
    }
    Err(format!("there are already too many files named `{stem}`"))
}

/// A theme file's text: every colour the window draws with, so the copy is complete.
fn theme_text(name: &str, theme: &Resolved) -> String {
    let mut out = String::new();
    out.push_str("# Rubrica colour theme, written by \"Copy as My Theme\".\n");
    out.push_str("# Colours are `#rrggbb`. Every colour the reader draws with is here,\n");
    out.push_str("# so nothing in this file follows anything else unless it names a\n");
    out.push_str("# `based_on` theme to inherit from.\n");
    out.push_str("\n[meta]\n");
    out.push_str(&format!("name = \"{}\"\n", escape(name)));
    out.push_str(&format!("appearance = \"{}\"\n", theme.appearance.label()));
    out.push_str("\n[colors]\n");
    let p = &theme.palette;
    for (key, color) in [
        ("bg", p.bg),
        ("text", p.text),
        ("muted", p.muted),
        ("accent", p.accent),
        ("code_bg", p.code_bg),
        ("page_surface", p.page_surface),
        ("page_shadow", p.page_shadow),
        ("on_accent", p.on_accent),
        ("keyword", p.keyword),
        ("string", p.string),
        ("comment", p.comment),
        ("number", p.number),
        ("ty", p.ty),
        ("highlight", p.highlight),
    ] {
        out.push_str(&format!("{key} = \"{}\"\n", hex(color)));
    }
    out.push_str("\n[chrome]\n");
    for (key, color) in [
        ("sidebar", p.sidebar),
        ("sidebar_text", p.sidebar_text),
        ("tab_strip", p.tab_strip),
        ("tab_text", p.tab_text),
        ("active_tab", p.active_tab),
        ("active_tab_text", p.on_tab_active),
        ("tab_inactive", p.tab_inactive),
        ("tab_hover", p.tab_hover),
        ("divider", p.divider),
    ] {
        out.push_str(&format!("{key} = \"{}\"\n", hex(color)));
    }
    out
}

/// A name as the inside of a TOML basic string.
fn escape(name: &str) -> String {
    let mut out = String::new();
    for ch in name.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            ch => out.push(ch),
        }
    }
    out
}

// ---------------------------------------------------------------------------
// The listing, and the folder's signature
// ---------------------------------------------------------------------------

/// `--list-themes`: everything this machine has, and which slot is pointing where.
pub fn describe(shelf: &Shelf, slots: [Option<String>; 2]) -> Vec<String> {
    let mut out = Vec::new();
    let mut header = format!(
        "themes       : {} built-in, {} file(s) read, {} not read",
        shelf.built_ins(),
        shelf.files(),
        shelf.dropped.len(),
    );
    if !shelf.dir.as_os_str().is_empty() {
        header.push_str(&format!(", in {}", shelf.dir.display()));
    }
    out.push(header);
    for theme in &shelf.themes {
        let mut line = format!(
            "  {:<10} : {} ({})",
            theme.appearance.label(),
            theme.name,
            theme.origin.label(),
        );
        if let Some(based_on) = &theme.based_on {
            line.push_str(&format!(", based on {based_on}"));
        }
        out.push(line);
        for note in &theme.skipped {
            out.push(format!("               !! {note}"));
        }
    }
    for (path, reason) in &shelf.dropped {
        out.push(format!("  !! not read: {}: {reason}", path.display()));
    }
    for (dark, slot) in [false, true].into_iter().zip(slots) {
        let appearance = Appearance::of_dark(dark);
        let name = format!("slot {}   : ", appearance.label());
        let Some(slot) = slot_name(slot) else {
            out.push(format!("{name}(the default: {})", appearance.default_name()));
            continue;
        };
        match pick(shelf, appearance, Some(&slot)).1 {
            None => out.push(format!("{name}{slot}")),
            Some(SlotFault::Missing(_)) => out.push(format!("{name}{slot} (not installed)")),
            Some(SlotFault::WrongAppearance(_)) => {
                out.push(format!("{name}{slot} (a {} theme)", other(appearance)));
            }
        }
    }
    out
}

/// A signature of the theme folder: every file's name, size and write time, folded into
/// one number. A tick of the clock compares this and does nothing more when it has not
/// moved, so a folder nobody is writing to costs a listing and never a parse.
pub fn stamp() -> u64 {
    dir().map(|folder| stamp_of(&folder)).unwrap_or(0)
}

pub fn stamp_of(folder: &Path) -> u64 {
    let mut parts: Vec<(String, u64, u128)> = Vec::new();
    let Ok(entries) = std::fs::read_dir(folder) else { return 0 };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path
            .extension()
            .map(|e| e.to_string_lossy().eq_ignore_ascii_case("toml"))
            .unwrap_or(false)
        {
            continue;
        }
        let Ok(meta) = entry.metadata() else { continue };
        if !meta.is_file() {
            continue;
        }
        let name = path.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
        let modified = meta
            .modified()
            .ok()
            .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|age| age.as_nanos())
            .unwrap_or(0);
        parts.push((name, meta.len(), modified));
    }
    parts.sort();
    let mut hash: u64 = 0xcbf29ce484222325;
    for (name, len, modified) in &parts {
        for byte in name.as_bytes() {
            hash = (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3);
        }
        for byte in len.to_le_bytes().into_iter().chain(modified.to_le_bytes()) {
            hash = (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3);
        }
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A folder of the test's own, taken away when the test ends. The reader's real
    /// folder is never touched: a test that reads `%APPDATA%` is a test that passes or
    /// fails on what the machine happens to hold.
    struct Scratch {
        dir: PathBuf,
    }

    impl Scratch {
        fn new(tag: &str) -> Scratch {
            let dir = std::env::temp_dir().join(format!("rubrica-themes-{}-{tag}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).expect("a scratch folder");
            Scratch { dir }
        }

        fn write(&self, name: &str, text: &str) -> PathBuf {
            let path = self.dir.join(name);
            std::fs::write(&path, text).expect("a theme file");
            path
        }

        fn shelf(&self) -> Shelf {
            shelf_in(&self.dir)
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    fn names(shelf: &Shelf) -> Vec<String> {
        shelf.themes.iter().map(|t| t.name.clone()).collect()
    }

    fn found<'a>(shelf: &'a Shelf, name: &str) -> &'a Resolved {
        shelf
            .themes
            .iter()
            .find(|t| t.name == name)
            .unwrap_or_else(|| panic!("{name} is a theme"))
    }

    /// Every colour of a palette, by the name a theme file calls it. The one place the
    /// twenty-three names are listed outside the writer, so a field added to `Palette`
    /// and forgotten here fails to compile the array's length rather than passing.
    fn colours(p: &Palette) -> [(&'static str, Rgb); 23] {
        [
            ("bg", p.bg),
            ("text", p.text),
            ("muted", p.muted),
            ("accent", p.accent),
            ("code_bg", p.code_bg),
            ("page_surface", p.page_surface),
            ("page_shadow", p.page_shadow),
            ("on_accent", p.on_accent),
            ("keyword", p.keyword),
            ("string", p.string),
            ("comment", p.comment),
            ("number", p.number),
            ("ty", p.ty),
            ("highlight", p.highlight),
            ("sidebar", p.sidebar),
            ("sidebar_text", p.sidebar_text),
            ("tab_strip", p.tab_strip),
            ("tab_text", p.tab_text),
            ("active_tab", p.active_tab),
            ("active_tab_text", p.on_tab_active),
            ("tab_inactive", p.tab_inactive),
            ("tab_hover", p.tab_hover),
            ("divider", p.divider),
        ]
    }

    fn hexes(p: &Palette) -> Vec<(&'static str, String)> {
        colours(p).into_iter().map(|(name, c)| (name, hex(c))).collect()
    }

    #[test]
    fn the_default_pair_is_exactly_the_built_in_pair() {
        // The whole no-regression argument for this feature: a slot that names nothing
        // draws with the palette this reader has always drawn with, to the bit.
        let built = built_ins();
        let light = Shelf { themes: built.clone(), ..Shelf::default() };
        assert!(!found(&light, "Default Light").palette.dark);
        assert_eq!(hexes(&found(&light, "Default Light").palette), hexes(&Palette::of(false)));
        let dark = Shelf { themes: built, ..Shelf::default() };
        assert_eq!(hexes(&found(&dark, "Default Dark").palette), hexes(&Palette::of(true)));
    }

    #[test]
    fn every_built_in_is_a_named_pair_with_an_appearance() {
        let built = built_ins();
        assert_eq!(built.len(), 6);
        for theme in &built {
            assert_eq!(theme.origin, Origin::BuiltIn);
            assert!(theme.based_on.is_none());
            assert!(theme.skipped.is_empty());
            assert_eq!(theme.palette.dark, theme.appearance.is_dark(), "{}", theme.name);
        }
        // Three of each, and each appearance's default is the one a slot that names
        // nothing gets.
        for appearance in [Appearance::Light, Appearance::Dark] {
            let of = built.iter().filter(|t| t.appearance == appearance).count();
            assert_eq!(of, 3, "{} themes", appearance.label());
            assert!(built.iter().any(|t| t.name == appearance.default_name()));
        }
    }

    #[test]
    fn a_file_theme_overrides_only_the_keys_it_names() {
        let scratch = Scratch::new("partial");
        scratch.write(
            "pale.toml",
            r##"
# The reader's own file: one colour, and the appearance.
[meta]
name = "Pale"
appearance = "light"

[colors]
bg = "#fff8f0"
"##,
        );
        let shelf = scratch.shelf();
        assert_eq!(names(&shelf)[6], "Pale");
        let pale = found(&shelf, "Pale");
        let default = Palette::of(false);
        assert_eq!(hex(pale.palette.bg), "#fff8f0");
        assert_eq!(hex(pale.palette.text), hex(default.text), "the ink is inherited");
        assert_eq!(hex(pale.palette.keyword), hex(default.keyword), "so is the code panel's");
        // And the furniture follows the page: nothing in the file said `sidebar`, so it
        // is the panel colour the theme did not name either.
        assert_eq!(hex(pale.palette.sidebar), hex(default.sidebar));
    }

    #[test]
    fn furniture_the_page_names_carries_the_furniture_with_it() {
        // A theme that names only `code_bg` gets a sidebar of *that* colour, not of the
        // default's: the six furniture colours follow the page's final answers.
        let scratch = Scratch::new("carry");
        scratch.write(
            "grey.toml",
            "[meta]\nname = \"Grey\"\nappearance = \"dark\"\n\n[colors]\ncode_bg = \"#303030\"\nmuted = \"#b0b0b0\"\n",
        );
        let shelf = scratch.shelf();
        let grey = found(&shelf, "Grey");
        assert_eq!(hex(grey.palette.sidebar), "#303030", "the sidebar follows the panel");
        assert_eq!(hex(grey.palette.sidebar_text), "#b0b0b0", "and its ink the quiet ink");
        assert_eq!(hex(grey.palette.divider), hex(grey.palette.tab_inactive));

        // And a theme that names a furniture colour keeps it, page or no page.
        let scratch = Scratch::new("kept");
        scratch.write(
            "own.toml",
            "[meta]\nname = \"Own\"\nappearance = \"dark\"\n\n[colors]\ncode_bg = \"#303030\"\n\n[chrome]\nsidebar = \"#101010\"\n",
        );
        let shelf = scratch.shelf();
        assert_eq!(hex(found(&shelf, "Own").palette.sidebar), "#101010");
    }

    #[test]
    fn a_based_on_chain_applies_from_the_root_down() {
        let scratch = Scratch::new("chain");
        scratch.write("a.toml", "[meta]\nname = \"A\"\nappearance = \"light\"\n\n[colors]\nbg = \"#111111\"\ntext = \"#222222\"\n");
        scratch.write("b.toml", "[meta]\nname = \"B\"\nappearance = \"light\"\nbased_on = \"A\"\n\n[colors]\ntext = \"#333333\"\nmuted = \"#444444\"\n");
        let shelf = scratch.shelf();
        let b = found(&shelf, "B");
        assert_eq!(b.based_on.as_deref(), Some("A"));
        assert_eq!(hex(b.palette.bg), "#111111", "the theme it builds on still answers");
        assert_eq!(hex(b.palette.text), "#333333", "and this theme answers over it");
        assert_eq!(hex(b.palette.muted), "#444444");
        // The chain ends at the built-in default, so a key nothing named is the pair's.
        assert_eq!(hex(b.palette.keyword), hex(Palette::of(false).keyword));
        // A file theme may build on a built-in by name, case-insensitively.
        let scratch = Scratch::new("built-on");
        scratch.write("m.toml", "[meta]\nname = \"M\"\nappearance = \"dark\"\nbased_on = \"mono dark\"\n\n[colors]\naccent = \"#ff0000\"\n");
        let shelf = scratch.shelf();
        assert_eq!(hex(found(&shelf, "M").palette.bg), hex(found(&shelf, "Mono Dark").palette.bg));
        assert_eq!(hex(found(&shelf, "M").palette.accent), "#ff0000");
    }

    #[test]
    fn a_chain_that_rings_or_runs_long_or_changes_appearance_is_not_a_theme() {
        let scratch = Scratch::new("ring");
        scratch.write("a.toml", "[meta]\nname = \"A\"\nappearance = \"light\"\nbased_on = \"B\"\n");
        scratch.write("b.toml", "[meta]\nname = \"B\"\nappearance = \"light\"\nbased_on = \"A\"\n");
        let shelf = scratch.shelf();
        assert!(!names(&shelf).contains(&"A".to_string()));
        assert!(!names(&shelf).contains(&"B".to_string()));
        assert_eq!(shelf.dropped.len(), 2);
        for (_, reason) in &shelf.dropped {
            assert!(reason.contains("comes back round"), "{reason}");
        }

        // A theme naming one that is not here, and one naming the other appearance.
        let scratch = Scratch::new("absent");
        scratch.write("a.toml", "[meta]\nname = \"A\"\nappearance = \"light\"\nbased_on = \"Nowhere\"\n");
        scratch.write("b.toml", "[meta]\nname = \"B\"\nappearance = \"light\"\nbased_on = \"Default Dark\"\n");
        let shelf = scratch.shelf();
        assert!(!names(&shelf).contains(&"A".to_string()));
        let reasons: Vec<&str> = shelf.dropped.iter().map(|(_, r)| r.as_str()).collect();
        assert!(reasons.iter().any(|r| r.contains("names no theme")), "{reasons:?}");
        assert!(reasons.iter().any(|r| r.contains("a dark theme")), "{reasons:?}");

        // Nine themes on one chain is one too many: eight is the bound.
        let scratch = Scratch::new("deep");
        scratch.write("t0.toml", "[meta]\nname = \"T0\"\nappearance = \"dark\"\n");
        for n in 1..9 {
            let base = format!("T{}", n - 1);
            scratch.write(
                &format!("t{n}.toml"),
                &format!("[meta]\nname = \"T{n}\"\nappearance = \"dark\"\nbased_on = \"{base}\"\n"),
            );
        }
        let shelf = scratch.shelf();
        for n in 0..8 {
            assert!(names(&shelf).contains(&format!("T{n}")), "T{n} stands at depth {n}");
        }
        assert!(!names(&shelf).contains(&"T8".to_string()), "the ninth is past the bound");
        assert!(shelf.dropped.iter().any(|(_, r)| r.contains("longer than 8")));
    }

    #[test]
    fn a_key_that_is_not_a_colour_costs_the_key_and_not_the_theme() {
        let scratch = Scratch::new("badkey");
        scratch.write(
            "t.toml",
            r##"
[meta]
name = "Mixed"   # a comment after a value
appearance = "LIGHT"

[colors]
bg = "#0a0b0c"   # the good one
text = "#xyz"    # not a colour
muted = #112233  # not a string
keyword = "#12345"
wat = "#ffffff"  # not a key
"##,
        );
        let shelf = scratch.shelf();
        let mixed = found(&shelf, "Mixed");
        assert_eq!(hex(mixed.palette.bg), "#0a0b0c");
        assert_eq!(hex(mixed.palette.text), hex(Palette::of(false).text), "the bad key is skipped");
        assert_eq!(hex(mixed.palette.keyword), hex(Palette::of(false).keyword));
        let notes = mixed.skipped.join("\n");
        assert_eq!(mixed.skipped.len(), 4, "{notes}");
        assert!(notes.contains("colors.text"), "{notes}");
        assert!(notes.contains("colors.muted"), "{notes}");
        assert!(notes.contains("colors.keyword"), "{notes}");
        assert!(notes.contains("not a key this reader knows"), "{notes}");
    }

    #[test]
    fn a_file_that_cannot_say_what_it_is_is_not_a_theme() {
        let scratch = Scratch::new("headless");
        scratch.write("a.toml", "[meta]\nappearance = \"light\"\n");
        scratch.write("b.toml", "[meta]\nname = \"B\"\n");
        scratch.write("c.toml", "[meta]\nname = \"\"\nappearance = \"dark\"\n");
        scratch.write("d.toml", "[meta]\nname = \"D\"\nappearance = \"twilight\"\n");
        scratch.write(
            "e.toml",
            &format!("[meta]\nname = \"{}\"\nappearance = \"dark\"\n", "x".repeat(MAX_NAME_CHARS + 1)),
        );
        scratch.write("f.toml", "[metа]\nname = \"F\"\nappearance = \"dark\"\n");
        let shelf = scratch.shelf();
        assert_eq!(shelf.files(), 0, "{:?}", names(&shelf));
        assert_eq!(shelf.dropped.len(), 6);
        let all: String = shelf.dropped.iter().map(|(_, r)| format!("{r}\n")).collect();
        assert!(all.contains("no `name`"), "{all}");
        assert!(all.contains("no `appearance`"), "{all}");
        assert!(all.contains("is empty"), "{all}");
        assert!(all.contains("not `light` or `dark`"), "{all}");
        assert!(all.contains("longer than"), "{all}");

        // A table this reader does not know costs its keys and not the theme: a file
        // with a name and an appearance is still a theme, and the listing says which
        // table it did not understand.
        let scratch = Scratch::new("tables");
        scratch.write("g.toml", "[meta]\nname = \"G\"\nappearance = \"light\"\n\n[extras]\nnote = \"x\"\n");
        let shelf = scratch.shelf();
        assert_eq!(shelf.files(), 1);
        let g = found(&shelf, "G");
        assert!(g.skipped.iter().any(|n| n.contains("[extras]")), "{:?}", g.skipped);
        assert!(g.skipped.iter().any(|n| n.contains("no table was open")), "{:?}", g.skipped);
    }

    #[test]
    fn a_bad_byte_a_big_file_and_a_pile_of_files_are_all_bounded() {
        let scratch = Scratch::new("bounds");
        std::fs::write(scratch.dir.join("binary.toml"), [0x5b, 0x6d, 0x65, 0x74, 0x61, 0xff, 0xfe]).unwrap();
        let mut big = String::from("[meta]\nname = \"Big\"\nappearance = \"light\"\n");
        big.push_str(&"#\n".repeat((MAX_FILE_BYTES as usize) / 2));
        scratch.write("big.toml", &big);
        // A file that is not `.toml` is not a theme, whatever it holds.
        scratch.write("notes.txt", "[meta]\nname = \"Text\"\nappearance = \"light\"\n");
        let shelf = scratch.shelf();
        assert_eq!(shelf.files(), 0);
        assert_eq!(shelf.dropped.len(), 2, "both found `.toml` files were set aside");
        let all: String = shelf.dropped.iter().map(|(_, r)| format!("{r}\n")).collect();
        assert!(all.contains("not UTF-8"), "{all}");
        assert!(all.contains("larger than"), "{all}");

        // More files than the cap: the ones read are the ones the listing shows first.
        let scratch = Scratch::new("pile");
        for n in 0..(MAX_FILES + 3) {
            scratch.write(
                &format!("t{n:04}.toml"),
                &format!("[meta]\nname = \"T{n}\"\nappearance = \"light\"\n"),
            );
        }
        let shelf = scratch.shelf();
        drop(shelf);
        let shelf = scratch.shelf();
        assert_eq!(shelf.files(), MAX_FILES);
        assert_eq!(shelf.dropped.len(), 3);
        assert!(shelf.dropped.iter().all(|(_, r)| r.contains("more than")));
    }

    #[test]
    fn one_name_is_one_theme() {
        let scratch = Scratch::new("names");
        scratch.write("a.toml", "[meta]\nname = \"Same\"\nappearance = \"light\"\n\n[colors]\nbg = \"#111111\"\n");
        scratch.write("b.toml", "[meta]\nname = \"same\"\nappearance = \"light\"\n\n[colors]\nbg = \"#222222\"\n");
        // A file may not take a built-in's name either.
        scratch.write("c.toml", "[meta]\nname = \"Default Dark\"\nappearance = \"dark\"\n");
        let shelf = scratch.shelf();
        assert_eq!(shelf.files(), 1);
        assert_eq!(hex(found(&shelf, "Same").palette.bg), "#111111", "the first in name order answers");
        assert_eq!(shelf.dropped.len(), 2);
        for (_, reason) in &shelf.dropped {
            assert!(reason.contains("is taken by another theme"), "{reason}");
        }
    }

    #[test]
    fn the_shelf_is_the_built_ins_first_and_then_the_files_in_name_order() {
        let scratch = Scratch::new("order");
        scratch.write("z.toml", "[meta]\nname = \"Zulu\"\nappearance = \"dark\"\n");
        scratch.write("a.toml", "[meta]\nname = \"Alpha\"\nappearance = \"light\"\n");
        let shelf = scratch.shelf();
        assert_eq!(
            names(&shelf),
            vec![
                "Default Light",
                "Default Dark",
                "Mono Light",
                "Mono Dark",
                "Solarized Light",
                "Solarized Dark",
                "Alpha",
                "Zulu",
            ]
        );
    }

    #[test]
    fn a_slot_that_names_something_that_is_not_there_falls_back_and_says_why() {
        let scratch = Scratch::new("slot");
        scratch.write("m.toml", "[meta]\nname = \"Moon\"\nappearance = \"dark\"\n");
        let shelf = scratch.shelf();

        let (theme, fault) = pick(&shelf, Appearance::Dark, None);
        assert_eq!(theme.name, "Default Dark");
        assert_eq!(fault, None);

        let (theme, fault) = pick(&shelf, Appearance::Dark, Some("Moon"));
        assert_eq!(theme.name, "Moon");
        assert_eq!(fault, None);

        let (theme, fault) = pick(&shelf, Appearance::Dark, Some("Gone"));
        assert_eq!(theme.name, "Default Dark");
        assert_eq!(fault, Some(SlotFault::Missing("Gone".to_string())));

        // A theme of the other appearance is a different fault: it is here, and it is
        // still not this slot's.
        let (theme, fault) = pick(&shelf, Appearance::Dark, Some("Default Light"));
        assert_eq!(theme.name, "Default Dark");
        assert_eq!(fault, Some(SlotFault::WrongAppearance("Default Light".to_string())));

        // Whitespace is not a name.
        assert_eq!(slot_name(Some("  ".to_string())), None);
        assert_eq!(slot_name(Some(" Mono Dark ".to_string())).as_deref(), Some("Mono Dark"));
        let (theme, fault) = pick(&shelf, Appearance::Light, slot_name(Some("   ".to_string())).as_deref());
        assert_eq!(theme.name, "Default Light");
        assert_eq!(fault, None);
    }

    #[test]
    fn a_theme_named_by_path_is_read_wherever_it_is_kept() {
        let scratch = Scratch::new("bypath");
        let path = scratch.write("outside.toml", "[meta]\nname = \"Beside\"\nappearance = \"dark\"\nbased_on = \"Mono Dark\"\n\n[colors]\naccent = \"#abcdef\"\n");
        // A shelf of the built-ins alone, which is the state a `--theme` word resolves
        // against when the reader's folder holds nothing else.
        let shelf = shelf_in(&scratch.dir.join("folder-that-is-not-there"));
        let beside = open(&shelf, path.to_str().expect("a path")).expect("a theme by path");
        assert_eq!(beside.name, "Beside");
        assert_eq!(hex(beside.palette.accent), "#abcdef");
        assert_eq!(hex(beside.palette.bg), hex(found(&shelf, "Mono Dark").palette.bg));

        // A name is looked up in the shelf, and a word that is neither is an error.
        assert_eq!(open(&shelf, "Mono Light").expect("a built-in").name, "Mono Light");
        assert!(open(&shelf, "Nowhere").is_err());
        assert!(open(&shelf, "  ").is_err());
        assert!(open(&shelf, "missing\\folder\\x.toml").is_err());
    }

    #[test]
    fn a_headless_run_answers_a_word_strictly_and_a_slot_gently() {
        let scratch = Scratch::new("headless-run");
        scratch.write("m.toml", "[meta]\nname = \"Moon\"\nappearance = \"dark\"\n");
        let shelf = scratch.shelf();
        let slot = |name: &str| Some(name.to_string());

        // A slot naming nothing, or naming a theme that left, draws the default pair.
        let plain = headless_in(&shelf, None, false, false, None).expect("the default");
        assert!(!plain.asked);
        assert_eq!(plain.name, "Default Light");
        let stale = headless_in(&shelf, None, true, false, slot("Gone")).expect("a stale slot");
        assert!(stale.asked);
        assert_eq!(stale.name, "Default Dark");
        assert_eq!(stale.fault, Some(SlotFault::Missing("Gone".to_string())));
        assert_eq!(
            report_line(&stale),
            "Default Dark (built-in); `Gone` is not installed".to_string(),
        );
        let wrong = headless_in(&shelf, None, true, false, slot("Mono Light")).expect("a wrong slot");
        assert!(report_line(&wrong).contains("`Mono Light` is a light theme"), "{}", report_line(&wrong));

        // `--theme` is strict: a word that names nothing is an error, not a default.
        assert!(headless_in(&shelf, Some("Nowhere"), false, false, None).is_err());
        let named = headless_in(&shelf, Some("Moon"), true, false, None).expect("by name");
        assert_eq!(named.name, "Moon");
        assert_eq!(
            report_line(&named),
            format!("Moon ({})", scratch.dir.join("m.toml").display()),
        );

        // And `--dark` with a light theme is a contradiction rather than a surprise.
        let err = headless_in(&shelf, Some("Mono Light"), true, true, None).unwrap_err();
        assert!(err.contains("`--dark` asks for a dark one"), "{err}");
        // Without `--dark` the theme's own appearance is the answer.
        let light = headless_in(&shelf, Some("Mono Light"), false, false, None).expect("light");
        assert_eq!(light.appearance, Appearance::Light);
        assert_eq!(hex(light.palette.bg), hex(found(&shelf, "Mono Light").palette.bg));
    }

    #[test]
    fn a_copy_written_out_reads_back_as_itself() {
        // The round trip that makes "Copy as My Theme" a starting point: what is
        // written is every colour the window has, and reading the file back gives the
        // same palette to the byte.
        let scratch = Scratch::new("copy");
        let source = Scratch::new("copy-source");
        source.write("s.toml", "[meta]\nname = \"Source\"\nappearance = \"light\"\nbased_on = \"Solarized Light\"\n\n[colors]\ncomment = \"#123456\"\n");
        let shelf = source.shelf();
        let original = found(&shelf, "Source").clone();
        let (path, name) = copy_into(&scratch.dir, &source.shelf(), &original).expect("a copy");
        assert_eq!(name, "Source Copy");
        assert_eq!(path.file_name().and_then(|n| n.to_str()), Some("Source Copy.toml"));

        let after = scratch.shelf();
        let copy = found(&after, "Source Copy");
        assert_eq!(hexes(&copy.palette), hexes(&original.palette));
        assert_eq!(copy.appearance, original.appearance);
        assert!(copy.based_on.is_none(), "a copy is complete, not a difference");
        assert!(copy.skipped.is_empty(), "{:?}", copy.skipped);

        // Copies of copies are counted, not stacked: the reader who copies the copy
        // they just made gets the second copy of the original theme.
        let (_, second) = copy_into(&scratch.dir, &scratch.shelf(), copy).expect("a copy of a copy");
        assert_eq!(second, "Source Copy 2");
        let after = scratch.shelf();
        assert_eq!(after.files(), 2);
        let (third, name) = copy_into(&scratch.dir, &after, found(&after, "Source Copy")).expect("a second copy");
        assert_eq!(name, "Source Copy 3");
        assert_eq!(third.file_name().and_then(|n| n.to_str()), Some("Source Copy 3.toml"));
    }

    #[test]
    fn a_name_is_not_a_file_name() {
        // The illegal characters come out, the length is bounded, and a name that is
        // nothing but dots and spaces still names a file.
        assert_eq!(file_stem("a/b:c*d?e\"f<g>h|i\\j"), "a-b-c-d-e-f-g-h-i-j");
        assert_eq!(file_stem("  trailing dots...  "), "trailing dots");
        assert_eq!(file_stem("   "), "theme");
        assert_eq!(file_stem(&"n".repeat(200)).chars().count(), MAX_STEM_CHARS);
        // A name with a quote in it is escaped, not leaked, and reads back as itself.
        let scratch = Scratch::new("quoting");
        let mut theme = found(&Shelf { themes: built_ins(), ..Shelf::default() }, "Default Light").clone();
        theme.name = "The \"Reader's\" \\ Theme".to_string();
        let (_, name) = copy_into(&scratch.dir, &Shelf { themes: built_ins(), ..Shelf::default() }, &theme).expect("a copy");
        assert_eq!(name, "The \"Reader's\" \\ Theme Copy");
        assert_eq!(hex(found(&scratch.shelf(), &name).palette.bg), hex(theme.palette.bg));
    }

    #[test]
    fn a_theme_file_is_read_as_toml_and_not_as_lines_of_it() {
        // A `#` inside a string is a letter, not a comment; a `'...'` literal string
        // holds the same text; an `=` inside a quoted value is part of the value.
        let scratch = Scratch::new("syntax");
        scratch.write(
            "s.toml",
            r##"
[meta]
name = 'Hash # Theme'
appearance = "dark"
based_on = "Mono Dark"

[colors]
bg = "#003049" # the sea at night
text = "#123456"

[chrome]
accent_note = "no such key"
"##,
        );
        let shelf = scratch.shelf();
        let theme = found(&shelf, "Hash # Theme");
        assert_eq!(hex(theme.palette.bg), "#003049");
        assert_eq!(hex(theme.palette.text), "#123456");
        assert_eq!(hex(theme.palette.keyword), hex(found(&shelf, "Mono Dark").palette.keyword));
        assert_eq!(theme.skipped.len(), 1, "{:?}", theme.skipped);
        assert!(theme.skipped[0].contains("chrome.accent_note"));
        drop(shelf);

        // A line that is not a `key = value` line is passed over with the line number,
        // and a value with a bad escape is refused by name rather than guessed at.
        let scratch = Scratch::new("lines");
        scratch.write(
            "l.toml",
            "[meta]\nname = \"L\"\nappearance = \"light\"\n\nthis is not a line\n[colors]\nbg = \"#ff\\u{44}\"\n",
        );
        let shelf = scratch.shelf();
        let theme = found(&shelf, "L");
        assert_eq!(theme.skipped.len(), 2, "{:?}", theme.skipped);
        assert!(theme.skipped[0].contains("line 5"), "{:?}", theme.skipped);
        assert!(theme.skipped[1].contains("not an escape"), "{:?}", theme.skipped);
        assert_eq!(hex(theme.palette.bg), hex(Palette::of(false).bg));
    }

    #[test]
    fn a_colour_is_a_hash_and_three_or_six_digits() {
        assert_eq!(hex(parse_hex("#fff").expect("short form")), "#ffffff");
        assert_eq!(hex(parse_hex("#000").expect("short form")), "#000000");
        assert_eq!(hex(parse_hex("#00ff80").expect("long form")), "#00ff80");
        assert_eq!(hex(parse_hex(" #ABC ").expect("spaces")), "#aabbcc");
        for bad in ["", "#", "#ff", "#ffff", "#fffff", "#ggghhh", "fff", "0xff", "#ff 00 00"] {
            assert!(parse_hex(bad).is_none(), "{bad} is not a colour");
        }
        // A colour outside the byte range is a rounded byte, not a panic.
        assert_eq!(hex(Rgb { r: -1.0, g: 2.0, b: 0.5 }), "#00ff80");
    }

    #[test]
    fn the_folder_signature_moves_when_a_file_appears_changes_or_leaves() {
        let scratch = Scratch::new("stamp");
        let start = stamp_of(&scratch.dir);
        let path = scratch.write("a.toml", "[meta]\nname = \"A\"\nappearance = \"light\"\n");
        let added = stamp_of(&scratch.dir);
        assert_ne!(start, added);
        // The same content written again is a new write time, and a write time is what
        // a file's identity on disk is made of.
        std::fs::write(&path, "[meta]\nname = \"A\"\nappearance = \"light\"\n").unwrap();
        let later = std::fs::metadata(&path).unwrap().modified().unwrap();
        std::fs::write(&path, "[meta]\nname = \"A\"\nappearance = \"dark\"\n").unwrap();
        let _ = later;
        let changed = stamp_of(&scratch.dir);
        assert_ne!(added, changed);
        std::fs::remove_file(&path).unwrap();
        assert_eq!(stamp_of(&scratch.dir), start);
        // A folder that is not there is a signature of nothing, not an error.
        assert_eq!(stamp_of(&scratch.dir.join("absent")), 0);
    }

    #[test]
    fn the_listing_names_every_theme_every_reason_and_both_slots() {
        let scratch = Scratch::new("listing");
        scratch.write("a.toml", "[meta]\nname = \"Alpha\"\nappearance = \"light\"\nbased_on = \"Mono Light\"\n\n[colors]\ntext = \"#zzz\"\n");
        scratch.write("b.toml", "[meta]\nname = \"Beta\"\nappearance = \"dark\"\n");
        scratch.write("c.toml", "[meta]\nname = \"\"\nappearance = \"dark\"\n");
        let shelf = scratch.shelf();
        let lines = describe(&shelf, [Some("Alpha".to_string()), Some("Beta".to_string())]);
        assert_eq!(
            lines[0],
            format!(
                "themes       : 6 built-in, 2 file(s) read, 1 not read, in {}",
                scratch.dir.display()
            )
        );
        assert!(lines.contains(&"  light      : Default Light (built-in)".to_string()));
        assert!(lines.contains(&format!(
            "  light      : Alpha ({}), based on Mono Light",
            scratch.dir.join("a.toml").display()
        )), "{lines:#?}");
        // A key that was skipped is named under the theme it belongs to.
        assert!(lines.iter().any(|l| l.contains("!! colors.text")), "{lines:#?}");
        // A file that is not a theme is named with the reason.
        assert!(lines.iter().any(|l| l.contains("!! not read:") && l.contains("is empty")), "{lines:#?}");
        assert!(lines.contains(&"slot light   : Alpha".to_string()));
        assert!(lines.contains(&"slot dark   : Beta".to_string()));

        let faulted = describe(&shelf, [Some("Beta".to_string()), Some("Gone".to_string())]);
        assert!(faulted.contains(&"slot light   : Beta (a dark theme)".to_string()), "{faulted:#?}");
        assert!(faulted.contains(&"slot dark   : Gone (not installed)".to_string()), "{faulted:#?}");
        let empty = describe(&shelf, [None, None]);
        assert!(empty.contains(&"slot light   : (the default: Default Light)".to_string()), "{empty:#?}");
        assert!(empty.contains(&"slot dark   : (the default: Default Dark)".to_string()));

        // A machine with no themes folder says so without a path.
        let bare = Shelf { themes: built_ins(), ..Shelf::default() };
        assert_eq!(describe(&bare, [None, None])[0], "themes       : 6 built-in, 0 file(s) read, 0 not read");
    }

    #[test]
    fn an_empty_folder_is_the_six_built_ins() {
        let scratch = Scratch::new("empty");
        let shelf = scratch.shelf();
        assert_eq!(shelf.files(), 0);
        assert_eq!(shelf.built_ins(), 6);
        assert!(shelf.dropped.is_empty());
        assert!(shelf.find(Appearance::Light, "mono light").is_some());
        assert!(shelf.find(Appearance::Dark, "mono light").is_none());
        // A folder that is not there at all is the same shelf.
        let absent = shelf_in(&scratch.dir.join("absent"));
        assert_eq!(names(&absent), names(&shelf));
    }
}
