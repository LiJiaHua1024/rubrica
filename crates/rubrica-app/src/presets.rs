//! Typography presets as files the reader can edit.
//!
//! A preset is everything a page is set with: the nineteen font roles, the twenty-one
//! numbers, and the switches that decide how those numbers are spent. The typography form
//! has always been able to
//! write one, into the registry, under a name. This module adds the other half of the
//! idea -- a preset as a *file*, in `%APPDATA%\Rubrica\typography`, which the reader can
//! open in an editor, keep beside a document, or hand to somebody else:
//!
//! ```toml
//! [meta]
//! name = "Wide markers"
//! based_on = "Book"
//!
//! [fonts]
//! latin_body = "Georgia"
//!
//! [metrics]
//! marker_inset = 1.2
//! list_marker_gap = 0.5
//! list_bullet_size = 1.2
//! quote_bar_gap = 0.8
//! ragged_below = 20.0
//!
//! [flags]
//! code_capsule = true
//! table_lines = "booktabs"
//! keep_korean_words = true
//! narrow_ragged = false
//!
//! [bind]
//! extensions = "txt, log"
//! ```
//!
//! The grammar is the same small corner of TOML a theme file is written in -- `[table]`
//! headers, `key = value`, `#` to the end of the line -- with the value vocabulary a
//! preset needs: numbers and switches as well as quoted strings. Nothing is refused
//! whole for one line this reader cannot read. A key it does not know, a number outside
//! the range the form would take, a switch that is neither `true` nor `false`: each is
//! set aside by name in [`Preset::skipped`], which `--list-profiles` prints, so a typo
//! is a line in a listing rather than a preset that quietly is not what it says.
//!
//! What a file *must* say is what it is: a `[meta] name`. A file that cannot say that is
//! not a preset, and is refused with the reason. `Default` and `Book` are names the
//! reader did not write, so a file may not take them; a file whose name is a preset the
//! form has saved stands in its place, because the file is the copy the reader can see
//! and edit. `based_on` names the preset this one is written over -- another file, or a
//! name from the form -- and a file that names nothing builds on the design's own page,
//! which is what `Default` holds.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::profiles::{self, Profile};
use crate::theme::{TableLines, Theme};

/// The largest preset this reader will read. A file larger than this is a mistake or
/// something that is not a preset at all, and either way it is not worth parsing: the
/// whole of one is a page of numbers.
pub const MAX_FILE_BYTES: u64 = 64 * 1024;

/// How many files are read. The cap is on reading, not on finding: a folder somebody
/// dropped a thousand files into is listed as "256 read, the rest not" rather than
/// walked whole.
pub const MAX_FILES: usize = 256;

/// How long a `based_on` chain may be, counted in presets. A chain longer than this is
/// a mistake, and one that comes back round to where it started is a loop.
const MAX_DEPTH: usize = 8;

/// How long a name may be, in characters. Long enough for a sentence about what a preset
/// is for, short enough to be a menu row.
const MAX_NAME_CHARS: usize = 80;

/// The nineteen `[fonts]` keys, in the order [`Profile::fonts`] is read: the same roles
/// the typography form shows, with a name a file can spell.
const FONT_KEYS: [&str; 19] = [
    "latin_body",
    "latin_headings",
    "latin_code",
    "chinese_body",
    "chinese_headings",
    "chinese_code",
    "japanese_body",
    "japanese_headings",
    "japanese_code",
    "korean_body",
    "korean_headings",
    "korean_code",
    "latin_emphasis",
    "chinese_emphasis",
    "japanese_emphasis",
    "korean_emphasis",
    "math",
    "math_fallback",
    "font_fallback",
];

/// The twenty-one `[metrics]` keys, in the order [`Profile::numbers`] is read. The last
/// four are the layout numbers this build added: where a list's marker hangs, how much
/// air stands between it and the words, how large it is drawn, and how far a quotation's
/// bar stands from the column it marks.
const METRIC_KEYS: [&str; 21] = [
    "size",
    "latin_body_leading",
    "asian_body_leading",
    "tracking",
    "paragraph_gap",
    "first_indent",
    "column",
    "ragged_below",
    "punctuation_compression",
    "hanging_punctuation",
    "latin_heading_leading",
    "asian_heading_leading",
    "heading_gap",
    "code_gap",
    "quote_indent",
    "list_indent",
    "definition_indent",
    "marker_inset",
    "list_marker_gap",
    "list_bullet_size",
    "quote_bar_gap",
];

/// Where a preset came from. A path travels with a file preset because the listing has
/// to name the file to open, and because two presets of one name are two real presets:
/// the file is the one that is drawn with, and the listing says which is which.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Origin {
    /// `Default` or `Book`: a preset the reader did not write.
    BuiltIn,
    /// A preset the typography form saved, in `HKCU\Software\Rubrica\Typography`.
    Registry,
    /// A file in the reader's own typography folder.
    File(PathBuf),
}

impl Origin {
    /// What a listing calls it. A built-in and a saved preset are named by what they
    /// are; a file is named by where it is, because that is what a reader needs to open
    /// it.
    pub fn label(&self) -> String {
        match self {
            Origin::BuiltIn => "built-in".to_string(),
            Origin::Registry => "saved".to_string(),
            Origin::File(path) => path.display().to_string(),
        }
    }

    fn path(&self) -> Option<&Path> {
        match self {
            Origin::BuiltIn | Origin::Registry => None,
            Origin::File(path) => Some(path),
        }
    }
}

/// One preset, resolved: every number filled in, whichever of the layers it came from.
#[derive(Clone, Debug)]
pub struct Preset {
    pub name: String,
    /// The preset this one was written over, kept for the listing: a reader looking at a
    /// preset they did not write needs to know what it was written against.
    pub based_on: Option<String>,
    pub origin: Origin,
    /// The fonts, the numbers and the East Asian policy: everything the page needs.
    pub profile: Profile,
    /// Whether an inline code span wears a capsule, when this preset has an opinion. A
    /// preset that says nothing about a switch leaves it to whoever asks -- the reader's
    /// own setting, or the command line -- rather than turning it off by being old.
    pub code_capsule: Option<bool>,
    /// Which set of table rules to draw, on the same terms.
    pub table_lines: Option<TableLines>,
    /// The file extensions this preset is bound to, as words without their dot and in
    /// lower case. Empty is a preset a reader chooses by name rather than by the kind of
    /// document they opened.
    pub extensions: Vec<String>,
    /// The keys this file wrote that this reader could not use, each with the reason.
    pub skipped: Vec<String>,
}

impl Preset {
    /// Draw with this preset: the page's own numbers first, then the two switches it has
    /// an opinion about. The switches come after the profile because they are not part
    /// of it -- a caller that has already chosen one (the command line, the reader's own
    /// setting) has its choice written over by this preset, and a caller that asks after
    /// this call has the last word.
    pub fn apply(&self, t: &mut Theme) {
        self.profile.apply(t);
        if let Some(capsule) = self.code_capsule {
            t.set_code_capsule(capsule);
        }
        if let Some(lines) = self.table_lines {
            t.set_table_lines(lines);
        }
    }
}

/// Everything this machine has to set a page with: the built-ins, the presets the form
/// has saved, and the files in the reader's own folder.
#[derive(Clone, Debug, Default)]
pub struct Shelf {
    /// The folder the files were read from. Empty when this machine has no typography
    /// folder at all, which is not an error: the built-ins are always there.
    pub dir: PathBuf,
    /// The presets, in the order a listing shows them: the built-ins, then the saved
    /// ones, then the files.
    pub presets: Vec<Preset>,
    /// The files that were found and are not presets, each with the reason.
    pub dropped: Vec<(PathBuf, String)>,
}

impl Shelf {
    pub fn built_ins(&self) -> usize {
        self.presets.iter().filter(|p| p.origin == Origin::BuiltIn).count()
    }

    pub fn saved(&self) -> usize {
        self.presets.iter().filter(|p| p.origin == Origin::Registry).count()
    }

    pub fn files(&self) -> usize {
        self.presets.iter().filter(|p| matches!(p.origin, Origin::File(_))).count()
    }

    /// The preset with this name, case-insensitively -- the way a menu, a saved choice
    /// and a `--profile` word all mean a name.
    pub fn find(&self, name: &str) -> Option<&Preset> {
        let name = name.trim();
        self.presets.iter().find(|p| p.name.eq_ignore_ascii_case(name))
    }

    /// The preset bound to a document of this kind, if one is: the extension a path ends
    /// with, matched without its dot and without regard to case.
    ///
    /// The *first* binding in listing order wins -- the built-ins and the saved presets
    /// have no bindings at all, so this is the first file that claims the extension in
    /// file-name order -- and the answer does not depend on the order a folder happened
    /// to be listed in. A file with no extension, and a name that is only a dot, are
    /// documents nothing can be bound to.
    pub fn binding(&self, path: &Path) -> Option<&Preset> {
        let extension = path.extension()?.to_string_lossy().to_lowercase();
        if extension.is_empty() {
            return None;
        }
        self.presets.iter().find(|p| p.extensions.contains(&extension))
    }
}

/// The folder the reader's own presets live in: `%APPDATA%\Rubrica\typography`. `None`
/// on a machine with no `APPDATA`, which is a machine with no reader's folders at all.
///
/// The folder is not made here. A reader who has never written a preset has no folder,
/// and an application that leaves empty folders behind it everywhere it goes is the
/// reason a `%APPDATA%` is a place to look for things you did not ask for.
pub fn dir() -> Option<PathBuf> {
    let base = std::env::var_os("APPDATA")?;
    if base.is_empty() {
        return None;
    }
    Some(PathBuf::from(base).join("Rubrica").join("typography"))
}

/// The presets this machine has. A folder that cannot be read is a folder with no files
/// in it: the reader gets what the form has saved, and the built-ins under that.
pub fn shelf() -> Shelf {
    match dir() {
        Some(folder) => shelf_in(&folder),
        None => shelf_with(Path::new(""), &saved_layers()),
    }
}

/// The presets in one folder, against the presets the form has saved on this machine.
pub fn shelf_in(folder: &Path) -> Shelf {
    shelf_with(folder, &saved_layers())
}

/// [`shelf_in`] against presets somebody else read: what the tests ask, without touching
/// the real folder or the real registry.
pub fn shelf_with(folder: &Path, saved: &[(String, Profile)]) -> Shelf {
    // The two the reader did not write. They are layers like any other -- a file may
    // build on `Book` -- and they come first, so a file that took one of their names
    // would be a file standing in for something the reader never asked it to replace.
    let mut layers: Vec<Layer> = vec![
        Layer::whole("Default", Origin::BuiltIn, Profile::default()),
        Layer::whole("Book", Origin::BuiltIn, Profile::book()),
    ];
    for (name, profile) in saved {
        if layers.iter().any(|l| l.name.eq_ignore_ascii_case(name)) {
            continue;
        }
        layers.push(Layer::whole(name, Origin::Registry, profile.clone()));
    }
    let mut dropped: Vec<(PathBuf, String)> = Vec::new();
    let mut files = toml_files(folder);
    if files.len() > MAX_FILES {
        for path in files.split_off(MAX_FILES) {
            dropped.push((path, format!("more than {MAX_FILES} files")));
        }
    }
    // A file stands in for a saved preset of the same name: the file is the copy the
    // reader can see and edit, and a name that meant the file last time meaning the
    // registry the next time would be a page that changed under a reader who touched
    // nothing. Two files of one name are still two presets, and the second is set aside
    // rather than shadowing the first, which would leave a listing row doing something
    // other than the row beside it that looks the same.
    let mut parsed: Vec<Layer> = Vec::new();
    for path in files {
        match read_preset(&path) {
            Ok(layer) => parsed.push(layer),
            Err(reason) => dropped.push((path, reason)),
        }
    }
    for layer in parsed {
        if let Some(built_in) = layers
            .iter()
            .find(|l| l.origin == Origin::BuiltIn && l.name.eq_ignore_ascii_case(&layer.name))
        {
            let path = layer.origin.path().map(Path::to_path_buf).unwrap_or_default();
            dropped.push((path, format!("`{}` is the name of a built-in preset", built_in.name)));
            continue;
        }
        if layers.iter().any(|l| {
            matches!(&l.origin, Origin::File(_)) && l.name.eq_ignore_ascii_case(&layer.name)
        }) {
            let path = layer.origin.path().map(Path::to_path_buf).unwrap_or_default();
            dropped.push((path, format!("the name `{}` is taken by another file", layer.name)));
            continue;
        }
        layers.retain(|l| !(l.origin == Origin::Registry && l.name.eq_ignore_ascii_case(&layer.name)));
        layers.push(layer);
    }
    // Resolve every preset against all of them at once, so the answer for one file does
    // not depend on the order the folder was listed in. A preset whose chain cannot be
    // walked is set aside with the reason and takes no one with it.
    let by_name: HashMap<String, usize> = {
        let mut map = HashMap::new();
        for (i, layer) in layers.iter().enumerate() {
            map.entry(layer.name.to_ascii_lowercase()).or_insert(i);
        }
        map
    };
    let mut presets = Vec::with_capacity(layers.len());
    for index in 0..layers.len() {
        match resolve(index, &layers, &by_name) {
            Ok(preset) => presets.push(preset),
            Err(reason) => {
                let layer = &layers[index];
                let path = layer.origin.path().map(Path::to_path_buf).unwrap_or_default();
                let name = layer.name.clone();
                if path.as_os_str().is_empty() {
                    // A saved preset that cannot be resolved is not worth refusing a
                    // page over: the reader gets the design's own numbers, which is
                    // what a preset that was never written has always given them.
                    presets.push(Preset {
                        name,
                        based_on: None,
                        origin: Origin::Registry,
                        profile: Profile::default(),
                        code_capsule: None,
                        table_lines: None,
                        extensions: Vec::new(),
                        skipped: vec![reason],
                    });
                } else {
                    dropped.push((path, format!("{name}: {reason}")));
                }
            }
        }
    }
    // The files come last in a listing, after the presets the form has saved, because
    // that is the order of how much the reader is expected to touch them.
    presets.sort_by_key(|p| match &p.origin {
        Origin::BuiltIn => 0,
        Origin::Registry => 1,
        Origin::File(_) => 2,
    });
    Shelf { dir: folder.to_path_buf(), presets, dropped }
}

/// The names a reader may ask for, in listing order.
/// The preset with this name, or `None` when this machine has no such preset.
pub fn load(name: &str) -> Option<Preset> {
    shelf().find(name).cloned()
}

/// Whether a name means a preset on this machine. Asked before a headless run draws, so
/// that a name that means nothing is an error rather than the design's own page.
pub fn exists(name: &str) -> bool {
    shelf().find(name).is_some()
}

/// Draw with the named preset, and say whether there was one. A name that means nothing
/// changes nothing: the caller that has to refuse it has already asked [`exists`].
pub fn apply(name: &str, t: &mut Theme) -> bool {
    match load(name) {
        Some(preset) => {
            preset.apply(t);
            true
        }
        None => false,
    }
}

/// The preset a document of this kind is set with: the one bound to its extension, or
/// the one this reader remembers for the kind of document it is.
///
/// A binding is the reader saying "pages like this are set like that", so it is the
/// first word: a file that claims `.txt` stands over the plain-text slot the typography
/// form has always kept, which can hold one preset for every plain document on the
/// machine. Under it is the remembered choice, and under that the design's own page --
/// so a machine with no bindings and no remembered choice is set exactly as it was
/// before any of this existed.
pub fn for_path(path: &Path, plain: Option<bool>) -> Option<Preset> {
    let plain = plain.unwrap_or_else(|| crate::reading::is_plain(Some(path)));
    let remembered = crate::profiles::selected(plain);
    for_path_in(&shelf(), path, &remembered)
}

/// [`for_path`] against a shelf and a remembered name somebody else has already read:
/// what the tests ask, and what the window will ask with the shelf it is holding.
pub fn for_path_in(shelf: &Shelf, path: &Path, remembered: &str) -> Option<Preset> {
    match shelf.binding(path) {
        Some(bound) => Some(bound.clone()),
        None => shelf.find(remembered).cloned(),
    }
}

/// `--list-profiles`: every preset this machine has, where each came from, what it was
/// written over, what it is bound to, and every key that could not be read.
pub fn describe(shelf: &Shelf, slots: [String; 2]) -> Vec<String> {
    let mut out = Vec::new();
    let mut header = format!(
        "profiles     : {} built-in, {} saved, {} file(s) read, {} not read",
        shelf.built_ins(),
        shelf.saved(),
        shelf.files(),
        shelf.dropped.len(),
    );
    if !shelf.dir.as_os_str().is_empty() {
        header.push_str(&format!(", in {}", shelf.dir.display()));
    }
    out.push(header);
    for preset in &shelf.presets {
        let mut line = format!("  {:<24} : {}", preset.name, preset.origin.label());
        if let Some(based_on) = &preset.based_on {
            line.push_str(&format!(", based on {based_on}"));
        }
        if !preset.extensions.is_empty() {
            line.push_str(&format!(", bound to .{}", preset.extensions.join(", .")));
        }
        out.push(line);
        for note in &preset.skipped {
            out.push(format!("               !! {note}"));
        }
    }
    for (path, reason) in &shelf.dropped {
        out.push(format!("  !! not read: {}: {reason}", path.display()));
    }
    for (plain, slot) in [true, false].into_iter().zip(slots) {
        let what = if plain { "plain text" } else { "documents " };
        out.push(format!("slot {what}: {slot}"));
    }
    out
}

/// A signature of the preset folder: every file's name, size and write time, folded into
/// one number. A window compares this once in three quarters of a second and does nothing
/// more when it has not moved, so a folder nobody is writing to costs a listing and never
/// a parse -- and a preset saved in an editor is noticed without a handle left open on a
/// folder the reader may not have.
pub fn stamp() -> u64 {
    dir().map(|folder| stamp_of(&folder)).unwrap_or(0)
}

pub fn stamp_of(folder: &Path) -> u64 {
    let mut parts: Vec<(String, u64, u128)> = Vec::new();
    let Ok(entries) = std::fs::read_dir(folder) else { return 0 };
    for entry in entries.flatten() {
        let path = entry.path();
        if !is_toml(&path) {
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
    // Order-independent, so a folder that lists its files in another order is the same
    // folder: the parts are sorted in, not concatenated in the order they arrived.
    parts.sort();
    let mut stamp = parts.len() as u64;
    for (name, size, modified) in parts {
        let mut hash = 0xcbf2_9ce4_8422_2325u64;
        for byte in name.as_bytes().iter().chain(&size.to_le_bytes()).chain(&modified.to_le_bytes()) {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x100_0000_01b3);
        }
        stamp = stamp.wrapping_mul(31).wrapping_add(hash);
    }
    stamp
}

/// One preset as a file says it: what the file itself holds, with `None` where it says
/// nothing and a name and an origin that every layer has.
#[derive(Clone, Debug)]
struct Layer {
    name: String,
    origin: Origin,
    based_on: Option<String>,
    fonts: [Option<String>; 19],
    numbers: [Option<f32>; 21],
    keep_korean_words: Option<bool>,
    /// Whether a column narrower than `metrics.ragged_below` is set ragged-right. The
    /// switch and the width travel together, so a preset that narrows the width without
    /// saying anything about the switch keeps whatever the page under it decided.
    narrow_ragged: Option<bool>,
    /// The two code switches, on the same terms: a preset that says nothing about one
    /// leaves it to whoever asks -- the page under it, or the command line.
    code_ladder: Option<bool>,
    code_lang_label: Option<bool>,
    code_capsule: Option<bool>,
    table_lines: Option<TableLines>,
    extensions: Vec<String>,
    skipped: Vec<String>,
}

impl Layer {
    /// A layer that says everything: a built-in preset, or one the form saved, where
    /// every number is known before any file is read.
    fn whole(name: &str, origin: Origin, profile: Profile) -> Self {
        let (fonts, numbers) = (profile.fonts, profile.numbers);
        Self {
            name: name.to_string(),
            origin,
            based_on: None,
            fonts: fonts.map(Some),
            numbers: numbers.map(Some),
            keep_korean_words: Some(profile.keep_korean_words),
            narrow_ragged: Some(profile.narrow_ragged),
            code_ladder: Some(profile.code_ladder),
            code_lang_label: Some(profile.code_lang_label),
            code_capsule: None,
            table_lines: None,
            extensions: Vec::new(),
            skipped: Vec::new(),
        }
    }

    fn empty(name: String, origin: Origin) -> Self {
        Self {
            name,
            origin,
            based_on: None,
            fonts: std::array::from_fn(|_| None),
            numbers: std::array::from_fn(|_| None),
            keep_korean_words: None,
            narrow_ragged: None,
            code_ladder: None,
            code_lang_label: None,
            code_capsule: None,
            table_lines: None,
            extensions: Vec::new(),
            skipped: Vec::new(),
        }
    }

    /// This layer's answers written over the ones before it.
    fn over(&self, merged: &mut Layer) {
        for (out, value) in merged.fonts.iter_mut().zip(&self.fonts) {
            if let Some(value) = value {
                *out = Some(value.clone());
            }
        }
        for (out, value) in merged.numbers.iter_mut().zip(&self.numbers) {
            if let Some(value) = value {
                *out = Some(*value);
            }
        }
        if let Some(value) = self.keep_korean_words {
            merged.keep_korean_words = Some(value);
        }
        if let Some(value) = self.narrow_ragged {
            merged.narrow_ragged = Some(value);
        }
        if let Some(value) = self.code_ladder {
            merged.code_ladder = Some(value);
        }
        if let Some(value) = self.code_lang_label {
            merged.code_lang_label = Some(value);
        }
        if let Some(value) = self.code_capsule {
            merged.code_capsule = Some(value);
        }
        if let Some(value) = self.table_lines {
            merged.table_lines = Some(value);
        }
        // A binding is not a number: a file that says nothing about extensions inherits
        // the ones it is built on rather than losing them, and a file that binds itself
        // replaces them -- an empty `extensions = ""` is how a preset unbinds itself.
        if !self.extensions.is_empty() {
            merged.extensions = self.extensions.clone();
        }
    }
}

/// Walk one preset's `based_on` chain from its root down and fill in every number.
fn resolve(index: usize, layers: &[Layer], by_name: &HashMap<String, usize>) -> Result<Preset, String> {
    let preset = &layers[index];
    let mut chain = vec![index];
    let mut current = index;
    while let Some(want) = layers[current].based_on.as_deref() {
        let key = want.trim().to_ascii_lowercase();
        let Some(&next) = by_name.get(&key) else {
            return Err(format!("`based_on = \"{want}\"` names no preset this reader has"));
        };
        if chain.contains(&next) {
            return Err(format!("`based_on` comes back round to `{}`", layers[next].name));
        }
        if chain.len() >= MAX_DEPTH {
            return Err(format!("the `based_on` chain is longer than {MAX_DEPTH} presets"));
        }
        chain.push(next);
        current = next;
    }
    // From the root down: each layer's own answers written over the ones before it, so
    // the preset closest to the end of the chain is the one that decides.
    chain.reverse();
    let mut merged = Layer::empty(preset.name.clone(), preset.origin.clone());
    // The design's own page is under every chain, whether or not a file names it: a file
    // that says one thing is a file that changes one thing, and the rest of the page is
    // the page this reader draws when nothing is chosen -- which is what `Default` holds.
    layers[0].over(&mut merged);
    for i in chain {
        layers[i].over(&mut merged);
    }
    let profile = Profile {
        fonts: merged.fonts.map(|value| value.unwrap_or_default()),
        numbers: merged.numbers.map(|value| value.unwrap_or_default()),
        keep_korean_words: merged.keep_korean_words.unwrap_or(false),
        // A chain that says nothing about the switch gets the design's own answer, which
        // is on: the fallback is what every narrow column was set with before this
        // switch existed, and a file that never mentions it must not turn it off.
        narrow_ragged: merged.narrow_ragged.unwrap_or(true),
        // The two code switches fall back the other way, because off is their design's
        // own answer: a file that never mentions them leaves the page as every page was
        // drawn before they existed.
        code_ladder: merged.code_ladder.unwrap_or(false),
        code_lang_label: merged.code_lang_label.unwrap_or(false),
    };
    // The form's own bounds, applied here rather than at draw time: a preset that cannot
    // be drawn with is a preset this reader cannot use, and the reader is told which one
    // and why instead of getting a page of default numbers.
    profile.validate()?;
    Ok(Preset {
        name: preset.name.clone(),
        based_on: preset.based_on.clone(),
        origin: preset.origin.clone(),
        profile,
        code_capsule: merged.code_capsule,
        table_lines: merged.table_lines,
        extensions: merged.extensions,
        skipped: preset.skipped.clone(),
    })
}

/// The presets the typography form has saved on this machine, as layers. Read once, when
/// a shelf is built: the form's own module remembers them, so this is a listing and not
/// a registry walk.
fn saved_layers() -> Vec<(String, Profile)> {
    let mut out = Vec::new();
    for name in profiles::names().into_iter().skip(2) {
        let profile = profiles::load(&name);
        out.push((name, profile));
    }
    out
}

/// Every `*.toml` file in a folder, in name order. A folder that cannot be read is a
/// folder with nothing in it.
fn toml_files(folder: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(folder) else { return Vec::new() };
    let mut files = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if !is_toml(&path) || !entry.file_type().map(|t| t.is_file()).unwrap_or(false) {
            continue;
        }
        files.push(path);
    }
    files.sort_by_key(|path| {
        path.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default()
    });
    files
}

fn is_toml(path: &Path) -> bool {
    path.extension().map(|e| e.to_string_lossy().eq_ignore_ascii_case("toml")).unwrap_or(false)
}

/// One file as a preset, bounded before it is read and parsed after.
fn read_preset(path: &Path) -> Result<Layer, String> {
    let meta = std::fs::metadata(path).map_err(|e| format!("cannot be read: {e}"))?;
    if meta.len() > MAX_FILE_BYTES {
        return Err(format!("larger than {} KiB", MAX_FILE_BYTES / 1024));
    }
    let bytes = std::fs::read(path).map_err(|e| format!("cannot be read: {e}"))?;
    let Ok(text) = String::from_utf8(bytes) else {
        return Err("not UTF-8 text".to_string());
    };
    // A byte-order mark is what an editor that does not know it is writing TOML puts in
    // front of the first line. It is not a letter of the file: without this, the first
    // line of a preset saved by such an editor would be a line this reader could not
    // read, and a reader's first attempt at a preset would come back with a complaint
    // about a line they can see is a comment.
    let text = text.strip_prefix('\u{feff}').unwrap_or(&text);
    parse(text, path)
}

/// One file as a preset.
///
/// The grammar is TOML's small corner, and the values a preset needs: numbers, switches,
/// quoted strings and one comma-separated list. A line this parser cannot read is a
/// skipped key rather than a refused file -- the file is still the preset its other
/// lines describe -- but a file that cannot say *what it is* (no name, a name past the
/// bound, a name that is a built-in's) is not a preset and is refused with the reason.
fn parse(text: &str, path: &Path) -> Result<Layer, String> {
    enum Table {
        Meta,
        Fonts,
        Metrics,
        Flags,
        Bind,
        Unknown,
    }
    let mut name: Option<String> = None;
    let mut based_on: Option<String> = None;
    let mut fonts: [Option<String>; 19] = std::array::from_fn(|_| None);
    let mut numbers: [Option<f32>; 21] = std::array::from_fn(|_| None);
    let mut keep_korean_words: Option<bool> = None;
    let mut narrow_ragged: Option<bool> = None;
    let mut code_ladder: Option<bool> = None;
    let mut code_lang_label: Option<bool> = None;
    let mut code_capsule: Option<bool> = None;
    let mut table_lines: Option<TableLines> = None;
    let mut extensions: Vec<String> = Vec::new();
    let mut skipped: Vec<String> = Vec::new();
    // A key before any table is TOML's own top level, which has no place in a preset and
    // is reported the way an unknown table is.
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
                "fonts" => Table::Fonts,
                "metrics" => Table::Metrics,
                "flags" => Table::Flags,
                "bind" => Table::Bind,
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
                "based_on" => match string_value(value) {
                    Ok(value) if value.trim().is_empty() => based_on = None,
                    Ok(value) => based_on = Some(value.trim().to_string()),
                    Err(reason) => skipped.push(format!("meta.based_on: {reason}")),
                },
                other => skipped.push(format!("meta.{other}: not a key this reader knows")),
            },
            Table::Fonts => {
                let Some(index) = FONT_KEYS.iter().position(|k| *k == key) else {
                    skipped.push(format!("fonts.{key} = {value}: not a font role this reader knows"));
                    continue;
                };
                match string_value(value) {
                    Ok(face) if face.trim().is_empty() => {
                        skipped.push(format!("fonts.{key}: an empty face"));
                    }
                    Ok(face) if face.len() > 200 || face.chars().any(char::is_control) => {
                        skipped.push(format!("fonts.{key}: not a name a font can have"));
                    }
                    Ok(face) => fonts[index] = Some(face),
                    Err(reason) => skipped.push(format!("fonts.{key}: {reason}")),
                }
            }
            Table::Metrics => {
                let Some(index) = METRIC_KEYS.iter().position(|k| *k == key) else {
                    skipped.push(format!("metrics.{key} = {value}: not a number this reader knows"));
                    continue;
                };
                match number_value(value) {
                    Ok(parsed) => {
                        let (min, max) = profiles::bounds(index);
                        if parsed.is_finite() && (min..=max).contains(&parsed) {
                            numbers[index] = Some(parsed);
                        } else {
                            skipped.push(format!(
                                "metrics.{key} = {value}: outside {min}..{max}"
                            ));
                        }
                    }
                    Err(reason) => skipped.push(format!("metrics.{key}: {reason}")),
                }
            }
            Table::Flags => match key {
                "code_capsule" => match bool_value(value) {
                    Ok(on) => code_capsule = Some(on),
                    Err(reason) => skipped.push(format!("flags.code_capsule: {reason}")),
                },
                "table_lines" => match string_value(value) {
                    Ok(word) => match table_lines_of(&word) {
                        Some(parsed) => table_lines = Some(parsed),
                        None => skipped.push(format!(
                            "flags.table_lines = {value}: not `grid` or `booktabs`"
                        )),
                    },
                    Err(reason) => skipped.push(format!("flags.table_lines: {reason}")),
                },
                "keep_korean_words" => match bool_value(value) {
                    Ok(on) => keep_korean_words = Some(on),
                    Err(reason) => skipped.push(format!("flags.keep_korean_words: {reason}")),
                },
                "narrow_ragged" => match bool_value(value) {
                    Ok(on) => narrow_ragged = Some(on),
                    Err(reason) => skipped.push(format!("flags.narrow_ragged: {reason}")),
                },
                "code_ladder" => match bool_value(value) {
                    Ok(on) => code_ladder = Some(on),
                    Err(reason) => skipped.push(format!("flags.code_ladder: {reason}")),
                },
                "code_lang_label" => match bool_value(value) {
                    Ok(on) => code_lang_label = Some(on),
                    Err(reason) => skipped.push(format!("flags.code_lang_label: {reason}")),
                },
                other => skipped.push(format!("flags.{other}: not a key this reader knows")),
            },
            Table::Bind => match key {
                "extensions" => match string_value(value) {
                    Ok(word) => match extensions_of(&word) {
                        Ok(list) => extensions = list,
                        Err(reason) => skipped.push(format!("bind.extensions: {reason}")),
                    },
                    Err(reason) => skipped.push(format!("bind.extensions: {reason}")),
                },
                other => skipped.push(format!("bind.{other}: not a key this reader knows")),
            },
            Table::Unknown => skipped.push(format!("`{key} = {value}`: no table was open")),
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
    if ["Default", "Book"].iter().any(|n| name.eq_ignore_ascii_case(n)) {
        return Err(format!("`{name}` is the name of a built-in preset"));
    }
    let mut layer = Layer::empty(name, Origin::File(path.to_path_buf()));
    layer.based_on = based_on;
    layer.fonts = fonts;
    layer.numbers = numbers;
    layer.keep_korean_words = keep_korean_words;
    layer.narrow_ragged = narrow_ragged;
    layer.code_ladder = code_ladder;
    layer.code_lang_label = code_lang_label;
    layer.code_capsule = code_capsule;
    layer.table_lines = table_lines;
    layer.extensions = extensions;
    layer.skipped = skipped;
    Ok(layer)
}

/// A line with its comment cut off. A `#` inside a quoted string is a letter of the
/// string, not a comment: a font's name may hold one.
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
/// outside a string is the one that divides: a reader whose preset names a font with an
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

/// One number, as a preset writes it. A bare word is the point -- `size = 13.5`, not
/// `size = "13.5"` -- and a value that is not a number is a key to skip rather than a
/// file to refuse.
fn number_value(raw: &str) -> Result<f32, String> {
    let word = raw.trim();
    if word.is_empty() {
        return Err("a number with nothing after it".to_string());
    }
    word.parse::<f32>()
        .map_err(|_| format!("`{word}` is not a number"))
}

/// One switch, bare or quoted: a file written by hand may say either.
fn bool_value(raw: &str) -> Result<bool, String> {
    let word = raw.trim();
    let bare = word.strip_prefix('"').and_then(|w| w.strip_suffix('"')).unwrap_or(word);
    match bare {
        "true" => Ok(true),
        "false" => Ok(false),
        other => Err(format!("`{other}` is not `true` or `false`")),
    }
}

/// The table rules a word names, the way `--tables` names them.
fn table_lines_of(word: &str) -> Option<TableLines> {
    match word.trim().to_ascii_lowercase().as_str() {
        "grid" => Some(TableLines::Grid),
        "booktabs" => Some(TableLines::Booktabs),
        _ => None,
    }
}

/// The extensions a preset binds itself to: `"txt, log"`, or `"md"`, or nothing at all.
/// A dot is allowed and dropped, case does not matter, and a word that is not made of
/// the characters an extension is made of is set aside with the reason rather than bound.
fn extensions_of(raw: &str) -> Result<Vec<String>, String> {
    let mut out: Vec<String> = Vec::new();
    for word in raw.split([',', ' ', '\t', ';']).map(str::trim).filter(|w| !w.is_empty()) {
        let word = word.trim_start_matches('.').to_lowercase();
        if word.is_empty() {
            return Err("an extension with no letters".to_string());
        }
        if !word.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') {
            return Err(format!(
                "`{word}` is not an extension: letters, digits, `_` and `-` only"
            ));
        }
        if !out.contains(&word) {
            out.push(word);
        }
    }
    Ok(out)
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
            let dir = std::env::temp_dir().join(format!("rubrica-presets-{}-{tag}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).expect("a scratch folder");
            Scratch { dir }
        }

        fn write(&self, name: &str, text: &str) -> PathBuf {
            let path = self.dir.join(name);
            std::fs::write(&path, text).expect("a preset file");
            path
        }

        /// The shelf this folder makes, against nothing the form has saved.
        fn shelf(&self) -> Shelf {
            shelf_with(&self.dir, &[])
        }

        /// The shelf this folder makes, against presets the form is pretended to have
        /// saved: what a file standing in for a saved preset is asked of.
        fn shelf_against(&self, saved: &[(String, Profile)]) -> Shelf {
            shelf_with(&self.dir, saved)
        }

        /// The one preset of this name, as the reader would get it.
        fn preset(&self, name: &str) -> Preset {
            self.shelf()
                .find(name)
                .cloned()
                .unwrap_or_else(|| panic!("no preset named `{name}`"))
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    /// The narrow-column switch travels with a preset, like the width it stands beside: a
    /// file that turns it off carries that to the page, a file that says nothing about it
    /// leaves the page under it to decide -- which is the design's own answer, on -- and a
    /// file that writes something that is not a switch is told so by name rather than read
    /// as the `false` a missing word would give.
    #[test]
    fn a_preset_carries_the_narrow_column_switch() {
        let scratch = Scratch::new("narrow");
        scratch.write("off.toml", "[meta]\nname = \"Off\"\n\n[flags]\nnarrow_ragged = false\n");
        scratch.write("quiet.toml", "[meta]\nname = \"Quiet\"\n\n[metrics]\nragged_below = 24.0\n");
        scratch.write("loud.toml", "[meta]\nname = \"Loud\"\n\n[flags]\nnarrow_ragged = \"yes\"\n");

        assert!(!scratch.preset("Off").profile.narrow_ragged, "the file's switch was not read");

        let quiet = scratch.preset("Quiet");
        assert!(quiet.profile.narrow_ragged, "a file that said nothing turned the fallback off");
        assert_eq!(quiet.profile.numbers[7], 24.0, "the width the file names");

        let loud = scratch.preset("Loud");
        assert!(loud.profile.narrow_ragged, "an unreadable switch was read as off");
        assert!(
            loud.skipped.iter().any(|s| s.starts_with("flags.narrow_ragged")),
            "the bad switch was not reported by name: {:?}",
            loud.skipped
        );

        // And the switch reaches the page the preset is applied to, which is what makes a
        // report able to say that the layout is no longer the design's own.
        let mut theme = Theme::default();
        scratch.preset("Off").apply(&mut theme);
        assert!(!theme.narrow_ragged);
        assert!(!theme.layout_is_the_design(), "the report would not say what moved");
    }

    /// A file the reader wrote is a preset: its numbers are the page's, its faces are
    /// the page's, its two switches are set, and it says where it came from. And the
    /// numbers it does not mention are the design's own -- a preset that says one thing
    /// is a preset that changes one thing.
    #[test]
    fn a_file_preset_is_read_and_laid_over_the_designs_own_page() {
        let scratch = Scratch::new("read");
        let path = scratch.write(
            "wide.toml",
            "# A preset the reader wrote.\n\
             [meta]\n\
             name = \"Wide markers\"\n\
             \n\
             [fonts]\n\
             latin_body = \"Georgia\"   # a comment after a value\n\
             \n\
             [metrics]\n\
             marker_inset = 1.2\n\
             list_marker_gap = 0.5\n\
             list_bullet_size = 1.2\n\
             quote_bar_gap = 0.8\n\
             size = 15.0\n\
             \n\
             [flags]\n\
             code_capsule = true\n\
             table_lines = \"booktabs\"\n\
             keep_korean_words = true\n\
             \n\
             [bind]\n\
             extensions = \"txt, .LOG\"\n",
        );
        let shelf = scratch.shelf();
        assert!(shelf.dropped.is_empty(), "a good file was set aside: {:?}", shelf.dropped);
        let preset = shelf.find("Wide markers").expect("the preset the file names");
        assert_eq!(preset.origin, Origin::File(path));
        assert!(preset.skipped.is_empty(), "nothing should have been skipped: {:?}", preset.skipped);
        assert_eq!(preset.profile.fonts[0], "Georgia");
        assert_eq!(preset.profile.numbers[0], 15.0, "the size the file names");
        assert_eq!(preset.profile.numbers[17], 1.2);
        assert_eq!(preset.profile.numbers[18], 0.5);
        assert_eq!(preset.profile.numbers[19], 1.2);
        assert_eq!(preset.profile.numbers[20], 0.8);
        // A number the file says nothing about is the design's own.
        let design = Profile::default();
        assert_eq!(preset.profile.numbers[5], design.numbers[5]);
        assert_eq!(preset.profile.fonts[1], design.fonts[1]);
        assert_eq!(preset.code_capsule, Some(true));
        assert_eq!(preset.table_lines, Some(TableLines::Booktabs));
        assert!(preset.profile.keep_korean_words);
        assert_eq!(preset.extensions, vec!["txt".to_string(), "log".to_string()]);

        // And applying it sets a page: the faces, the numbers, and the two switches.
        let mut theme = Theme::default();
        preset.apply(&mut theme);
        assert_eq!(theme.fonts.latin[0], "Georgia");
        assert_eq!(theme.marker_inset_em, 1.2);
        assert_eq!(theme.list_marker_gap_em, 0.5);
        assert_eq!(theme.list_bullet_size, 1.2);
        assert_eq!(theme.quote_bar_gap_em, 0.8);
        assert!(theme.code_capsule);
        assert_eq!(theme.table_lines, TableLines::Booktabs);
        assert!(theme.keep_korean_words);
    }

    /// A preset written over another borrows everything it does not say -- and the other
    /// may be a built-in, a preset the form saved, or another file. The root of a chain
    /// is the design's own page, so a preset that names nothing is the design's page with
    /// its own lines written over it.
    #[test]
    fn a_preset_written_over_another_borrows_what_it_does_not_say() {
        let scratch = Scratch::new("based-on");
        scratch.write(
            "root.toml",
            "[meta]\nname = \"Root\"\n[metrics]\nsize = 15.0\nmarker_inset = 2.0\n",
        );
        scratch.write(
            "leaf.toml",
            "[meta]\nname = \"Leaf\"\nbased_on = \"Root\"\n[metrics]\nquote_bar_gap = 1.5\n",
        );
        scratch.write(
            "book.toml",
            "[meta]\nname = \"From the book\"\nbased_on = \"Book\"\n[metrics]\nsize = 11.0\n",
        );
        let shelf = scratch.shelf();
        assert!(shelf.dropped.is_empty(), "{:?}", shelf.dropped);

        let leaf = shelf.find("Leaf").expect("the leaf");
        assert_eq!(leaf.based_on.as_deref(), Some("Root"));
        assert_eq!(leaf.profile.numbers[0], 15.0, "the size comes from the root");
        assert_eq!(leaf.profile.numbers[17], 2.0, "so does the marker inset");
        assert_eq!(leaf.profile.numbers[20], 1.5, "and the leaf's own line wins");
        assert_eq!(leaf.profile.numbers[5], Profile::default().numbers[5]);

        let book = shelf.find("From the book").expect("the book preset");
        assert_eq!(book.profile.numbers[0], 11.0);
        assert_eq!(book.profile.numbers[1], Profile::book().numbers[1], "the book's leading");
        assert_eq!(book.profile.fonts[0], Profile::book().fonts[0], "and the book's face");

        // A preset the form saved is a base like any other.
        let saved = vec![("Form preset".to_string(), Profile::book())];
        scratch.write(
            "form.toml",
            "[meta]\nname = \"Over the form\"\nbased_on = \"Form preset\"\n[metrics]\nsize = 9.0\n",
        );
        let shelf = scratch.shelf_against(&saved);
        let over = shelf.find("Over the form").expect("the preset over the form's own");
        assert_eq!(over.profile.numbers[0], 9.0);
        assert_eq!(over.profile.numbers[1], Profile::book().numbers[1]);
        assert_eq!(over.profile.fonts[3], Profile::book().fonts[3]);
    }

    /// A line this reader cannot read is a key set aside by name, and the rest of the
    /// file is still the preset it describes: a typo costs the reader one number, not
    /// the whole file. Every reason names the key it belongs to, because a listing that
    /// says "something was wrong" is a listing that sends the reader looking.
    #[test]
    fn a_key_this_reader_cannot_read_is_skipped_by_name() {
        let scratch = Scratch::new("skipped");
        scratch.write(
            "rough.toml",
            "[meta]\n\
             name = \"Rough\"\n\
             nick = \"nobody\"\n\
             \n\
             [metrics]\n\
             size = 14.0\n\
             column = 900.0\n\
             tracking = \"none\"\n\
             mystery = 3\n\
             \n\
             [flags]\n\
             code_capsule = maybe\n\
             table_lines = \"double\"\n\
             \n\
             [bind]\n\
             extensions = \"t!xt\"\n\
             \n\
             [colours]\n\
             bg = \"#fff\"\n\
             \n\
             loose = 1\n",
        );
        let preset = scratch.preset("Rough");
        // The one good number is the page's, and the file is still a preset.
        assert_eq!(preset.profile.numbers[0], 14.0);
        let skipped = preset.skipped.join("\n");
        for expected in [
            "meta.nick",
            "metrics.column",
            "metrics.tracking",
            "metrics.mystery",
            "flags.code_capsule",
            "flags.table_lines",
            "bind.extensions",
            "[colours]",
            "loose",
        ] {
            assert!(skipped.contains(expected), "`{expected}` was not reported in:\n{skipped}");
        }
        // A number outside the range the form takes is refused the same way, and says
        // what the range is.
        assert!(skipped.contains("outside 10..100"), "the range was not named in:\n{skipped}");
        // Nothing was set by the keys that could not be read.
        assert_eq!(preset.profile.numbers[6], Profile::default().numbers[6]);
        assert_eq!(preset.code_capsule, None);
        assert_eq!(preset.table_lines, None);
        assert!(preset.extensions.is_empty());
    }

    /// A file that cannot say what it is, is not a preset: no name, an empty one, one
    /// past the bound, or a name the reader did not write. Each is set aside with the
    /// reason, and takes no one with it -- the other files in the folder are still read.
    #[test]
    fn a_file_that_cannot_say_what_it_is_is_not_a_preset() {
        let scratch = Scratch::new("nameless");
        scratch.write("good.toml", "[meta]\nname = \"Good\"\n");
        scratch.write("none.toml", "[metrics]\nsize = 14.0\n");
        scratch.write("empty.toml", "[meta]\nname = \"   \"\n");
        scratch.write("long.toml", &format!("[meta]\nname = \"{}\"\n", "x".repeat(81)));
        scratch.write("built-in.toml", "[meta]\nname = \"book\"\n");
        let shelf = scratch.shelf();
        assert_eq!(shelf.files(), 1, "only the good file is a preset");
        assert!(shelf.find("Good").is_some());
        let reasons: Vec<String> = shelf.dropped.iter().map(|(_, reason)| reason.clone()).collect();
        assert_eq!(reasons.len(), 4, "{reasons:?}");
        assert!(reasons.iter().any(|r| r.contains("no `name`")), "{reasons:?}");
        assert!(reasons.iter().any(|r| r.contains("is empty")), "{reasons:?}");
        assert!(reasons.iter().any(|r| r.contains("longer than 80")), "{reasons:?}");
        assert!(reasons.iter().any(|r| r.contains("built-in")), "{reasons:?}");
    }

    /// A file stands in for a preset the form saved of the same name: the file is the
    /// copy the reader can see and edit, and one name is one row. A file may not stand in
    /// for a built-in, and two files of one name are one preset and one file set aside.
    #[test]
    fn a_file_stands_in_for_a_saved_preset_and_never_for_a_built_in() {
        let scratch = Scratch::new("standing-in");
        let path = scratch.write("mine.toml", "[meta]\nname = \"Mine\"\n[metrics]\nsize = 17.0\n");
        scratch.write("twice.toml", "[meta]\nname = \"Twice\"\n");
        scratch.write("twice-again.toml", "[meta]\nname = \"Twice\"\n");
        scratch.write("default.toml", "[meta]\nname = \"Default\"\n");
        let saved = vec![
            ("Mine".to_string(), Profile::book()),
            ("Saved only".to_string(), Profile::book()),
        ];
        let shelf = scratch.shelf_against(&saved);

        let mine: Vec<&Preset> = shelf.presets.iter().filter(|p| p.name == "Mine").collect();
        assert_eq!(mine.len(), 1, "one name is one row");
        assert_eq!(mine[0].origin, Origin::File(path), "the file is the one drawn with");
        assert_eq!(mine[0].profile.numbers[0], 17.0, "and it is the file's numbers that stand");
        assert!(shelf.find("Saved only").is_some(), "a saved preset with no file still stands");
        assert_eq!(shelf.find("Saved only").map(|p| p.origin.clone()), Some(Origin::Registry));

        // The built-ins are the reader's, and a file may not take their names.
        assert_eq!(shelf.find("Default").map(|p| p.origin.clone()), Some(Origin::BuiltIn));
        assert!(shelf.dropped.iter().any(|(_, r)| r.contains("built-in")), "{:?}", shelf.dropped);
        // Two files of one name: one preset, one file set aside.
        assert_eq!(shelf.presets.iter().filter(|p| p.name == "Twice").count(), 1);
        assert!(shelf.dropped.iter().any(|(_, r)| r.contains("another file")), "{:?}", shelf.dropped);
    }

    /// A `based_on` that names nothing, a chain that comes back round, and one longer
    /// than the reader will walk: each is set aside with the reason, and each takes no
    /// one with it.
    #[test]
    fn a_chain_that_cannot_be_walked_is_set_aside() {
        let scratch = Scratch::new("chains");
        scratch.write("missing.toml", "[meta]\nname = \"Missing\"\nbased_on = \"Nobody\"\n");
        scratch.write("loop-a.toml", "[meta]\nname = \"Loop A\"\nbased_on = \"Loop B\"\n");
        scratch.write("loop-b.toml", "[meta]\nname = \"Loop B\"\nbased_on = \"Loop A\"\n");
        // Twelve files in one chain: longer than the reader will walk, so the ones past
        // the bound are set aside with the reason rather than followed.
        for i in 0..12 {
            let based_on = if i == 0 {
                String::new()
            } else {
                format!("based_on = \"Long {}\"\n", i - 1)
            };
            scratch.write(
                &format!("long-{i:02}.toml"),
                &format!("[meta]\nname = \"Long {i}\"\n{based_on}"),
            );
        }
        scratch.write("fine.toml", "[meta]\nname = \"Fine\"\n");
        let shelf = scratch.shelf();
        assert!(shelf.find("Fine").is_some(), "a good file is still read");
        let reasons: Vec<String> = shelf.dropped.iter().map(|(_, reason)| reason.clone()).collect();
        assert!(reasons.iter().any(|r| r.contains("names no preset")), "{reasons:?}");
        assert!(reasons.iter().any(|r| r.contains("comes back round")), "{reasons:?}");
        assert!(reasons.iter().any(|r| r.contains("longer than 8")), "{reasons:?}");
        assert!(shelf.find("Missing").is_none());
        assert!(shelf.find("Loop A").is_none());
    }

    /// With no folder and nothing saved, this machine has the two presets it always had,
    /// and every name that meant the design's own page still means it: the whole layer is
    /// an addition, not a change.
    #[test]
    fn a_machine_with_no_preset_files_has_the_presets_it_always_had() {
        let absent = std::env::temp_dir().join("rubrica-presets-folder-that-is-not-there");
        let shelf = shelf_with(&absent, &[]);
        let names: Vec<&str> = shelf.presets.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, vec!["Default", "Book"]);
        assert_eq!(shelf.built_ins(), 2);
        assert_eq!(shelf.files(), 0);
        assert!(shelf.dropped.is_empty());

        let mut theme = Theme::default();
        assert!(apply("Default", &mut theme), "the default is always here");
        assert_eq!(theme.marker_inset_em, Theme::DESIGN_MARKER_INSET_EM);
        let mut theme = Theme::default();
        assert!(apply("Book", &mut theme));
        assert_eq!(theme.base, Profile::book().numbers[0]);
        // And a name that means nothing changes nothing at all.
        let mut theme = Theme::default();
        assert!(!apply("Nobody", &mut theme));
        assert!(theme.layout_is_the_design());
    }

    /// The listing is what a reader reads to find out why a page is set the way it is:
    /// where each preset came from, what it was written over, what it is bound to, and
    /// every line of it that could not be read.
    #[test]
    fn the_listing_names_where_a_preset_came_from_and_what_it_is_bound_to() {
        let scratch = Scratch::new("listing");
        let path = scratch.write(
            "wide.toml",
            "[meta]\nname = \"Wide\"\nbased_on = \"Book\"\n[bind]\nextensions = \"txt\"\n[metrics]\nmystery = 1\n",
        );
        let shelf = scratch.shelf_against(&[("Saved".to_string(), Profile::default())]);
        let lines = describe(&shelf, ["Saved".to_string(), "Default".to_string()]);
        let all = lines.join("\n");
        assert!(lines[0].starts_with("profiles     : 2 built-in, 1 saved, 1 file(s) read"), "{}", lines[0]);
        assert!(all.contains("built-in"), "{all}");
        assert!(all.contains("saved"), "{all}");
        assert!(all.contains(&path.display().to_string()), "{all}");
        assert!(all.contains("based on Book"), "{all}");
        assert!(all.contains("bound to .txt"), "{all}");
        assert!(all.contains("!! metrics.mystery"), "{all}");
        assert!(all.contains("slot plain text: Saved"), "{all}");
        assert!(all.contains("slot documents : Default"), "{all}");
    }

    /// A file saved by an editor that puts a byte-order mark in front of the first line
    /// is still the preset it says it is: the mark is not a letter of the file, and the
    /// first line of a reader's first preset is usually a comment.
    #[test]
    fn a_byte_order_mark_is_not_a_line_of_the_file() {
        let scratch = Scratch::new("bom");
        scratch.write(
            "bom.toml",
            "\u{feff}# saved by an editor that marks its files\n[meta]\nname = \"Bom\"\n[metrics]\nsize = 13.0\n",
        );
        let preset = scratch.preset("Bom");
        assert!(preset.skipped.is_empty(), "{:?}", preset.skipped);
        assert_eq!(preset.profile.numbers[0], 13.0);
    }

    /// A binding is the extension a path ends with, and nothing else about it: not the
    /// folder, not the name, not the case, and not the dot the reader wrote in the file
    /// because they write dots in front of extensions. A document with no extension is a
    /// document nothing can be bound to.
    #[test]
    fn a_binding_is_the_extension_the_path_ends_with() {
        let scratch = Scratch::new("binding");
        scratch.write(
            "text.toml",
            "[meta]\nname = \"For text\"\n[metrics]\nsize = 15.0\n[bind]\nextensions = \".TXT, Log\"\n",
        );
        let shelf = scratch.shelf();
        let bound = shelf.find("For text").expect("the preset");
        assert_eq!(bound.extensions, vec!["txt".to_string(), "log".to_string()]);
        for path in ["C:\\books\\notes.txt", "C:\\books\\NOTES.TXT", "C:\\a\\b\\c.Txt"] {
            assert_eq!(
                shelf.binding(Path::new(path)).map(|p| p.name.as_str()),
                Some("For text"),
                "{path} is a text document",
            );
        }
        for path in ["C:\\books\\notes.md", "C:\\books\\notes.txtx", "C:\\books\\notes", "C:\\books\\"] {
            assert!(shelf.binding(Path::new(path)).is_none(), "{path} is not a text document");
        }
        // A binding reaches the shelf's own presets too: nothing binds, and the answer
        // is nothing rather than the first preset in the list.
        assert!(Shelf::default().binding(Path::new("a.txt")).is_none());
    }

    /// A binding stands over what the reader remembers for that kind of document, and the
    /// remembered choice stands when nothing is bound -- which is what every document was
    /// set with before any of this existed.
    #[test]
    fn a_binding_stands_over_what_the_reader_remembers() {
        let scratch = Scratch::new("over-remembered");
        scratch.write(
            "text.toml",
            "[meta]\nname = \"For text\"\n[bind]\nextensions = \"txt\"\n",
        );
        let saved = vec![
            ("Remembered".to_string(), Profile::book()),
            ("Plain slot".to_string(), Profile::default()),
        ];
        let shelf = scratch.shelf_against(&saved);
        // The document is bound: the binding is the answer, and the remembered name is
        // not consulted at all.
        let bound = for_path_in(&shelf, Path::new("C:\\books\\notes.txt"), "Remembered")
            .expect("a preset for a text document");
        assert_eq!(bound.name, "For text");
        // The document is not bound: the remembered choice is the answer.
        let remembered = for_path_in(&shelf, Path::new("C:\\books\\notes.md"), "Remembered")
            .expect("a preset for a markdown document");
        assert_eq!(remembered.name, "Remembered");
        assert_eq!(remembered.profile.numbers[0], Profile::book().numbers[0]);
        // A remembered name that means nothing here is not an answer at all: the caller
        // is told nothing was chosen rather than being handed somebody else's page.
        assert!(for_path_in(&shelf, Path::new("a.md"), "Nobody").is_none());
    }

    /// The signature a window watches: it moves when a file is written, rewritten or
    /// taken away, and it does not move when nothing was touched -- which is what makes
    /// "saved is in effect" cost a listing and never a parse.
    #[test]
    fn a_saved_file_moves_the_signature() {
        let scratch = Scratch::new("stamp");
        let empty = stamp_of(&scratch.dir);
        assert_eq!(empty, stamp_of(&scratch.dir), "a folder nobody touched is the same folder");
        let path = scratch.write("one.toml", "[meta]\nname = \"One\"\n");
        let first = stamp_of(&scratch.dir);
        assert_ne!(first, empty, "a new file is a new folder");
        assert_eq!(first, stamp_of(&scratch.dir));
        // Rewritten with different words: the size is part of the signature, and so is
        // the write time.
        scratch.write("one.toml", "[meta]\nname = \"One\"\n[metrics]\nsize = 15.0\n");
        let second = stamp_of(&scratch.dir);
        assert_ne!(second, first, "a saved file is a new folder");
        // A file that is not a preset is not part of the folder's signature.
        std::fs::write(scratch.dir.join("notes.txt"), "not a preset").unwrap();
        assert_eq!(stamp_of(&scratch.dir), second, "a text file is not a preset");
        std::fs::remove_file(&path).unwrap();
        assert_ne!(stamp_of(&scratch.dir), second, "a file taken away is a new folder");
        // A folder that is not there is the same as one with nothing in it.
        assert_eq!(stamp_of(&scratch.dir.join("absent")), 0);
    }

    /// The value vocabulary is the part a theme file does not have: numbers are bare,
    /// switches are `true` or `false` with or without quotes, and a binding is one
    /// quoted list of words. A `#` inside a string is a letter of the string, not a
    /// comment: a face may hold one, and so may a name.
    #[test]
    fn the_values_a_preset_writes_are_the_ones_it_reads() {
        assert_eq!(number_value(" 13.5 "), Ok(13.5));
        assert!(number_value("thirteen").is_err());
        assert!(number_value("").is_err());
        assert_eq!(bool_value("true"), Ok(true));
        assert_eq!(bool_value("false"), Ok(false));
        assert_eq!(bool_value("\"true\""), Ok(true));
        assert!(bool_value("yes").is_err());
        assert_eq!(table_lines_of("Booktabs"), Some(TableLines::Booktabs));
        assert_eq!(table_lines_of(" grid "), Some(TableLines::Grid));
        assert_eq!(table_lines_of("double"), None);
        assert_eq!(extensions_of("txt, .LOG"), Ok(vec!["txt".to_string(), "log".to_string()]));
        assert_eq!(extensions_of(""), Ok(Vec::new()));
        assert!(extensions_of("t!xt").is_err());
        assert!(extensions_of(".").is_err());
        // A `#` inside a string is a letter of the string, not a comment: a face may
        // hold one, and so may a name.
        assert_eq!(strip_comment("latin_body = \"A#B\" # a note"), "latin_body = \"A#B\" ");
        assert_eq!(string_value("\"A#B\""), Ok("A#B".to_string()));
        assert_eq!(split_assign("latin_body = \"A=B\""), Some(("latin_body ", "\"A=B\"")));
    }
}

