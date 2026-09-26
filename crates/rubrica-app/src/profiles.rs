//! Named typography presets. Values are validated before reaching layout arithmetic.

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

use crate::{
    i18n::{self, Language},
    settings,
    theme::{Leading, Theme},
};

pub fn font_label(index: usize, lang: Language) -> &'static str {
    i18n::font_label(lang, index)
}

pub fn number_label(index: usize, lang: Language) -> &'static str {
    i18n::number_label(lang, index)
}

const ROOT: &str = "Software\\Rubrica\\Typography";

/// The complete set of font roles a profile can carry. The order is also the stable
/// registry order for new profiles; legacy profiles are mapped explicitly below.
pub const FONT_LABELS: [&str; 19] = [
    "Latin body",
    "Latin headings",
    "Latin code",
    "Chinese body",
    "Chinese headings",
    "Chinese code",
    "Japanese body",
    "Japanese headings",
    "Japanese code",
    "Korean body",
    "Korean headings",
    "Korean code",
    "Latin emphasis",
    "Chinese emphasis",
    "Japanese emphasis",
    "Korean emphasis",
    "Math",
    "Math fallback",
    "Font fallback",
];
#[allow(dead_code)]
pub const NUMBER_LABELS: [&str; 17] = [
    "Size (pt)",
    "Latin body leading",
    "Asian body leading",
    "Tracking (em)",
    "Paragraph gap",
    "First indent (em)",
    "Column (em)",
    "Ragged below (em)",
    "Punctuation compression",
    "Hanging punctuation (em)",
    "Latin heading leading",
    "Asian heading leading",
    "Heading gap",
    "Code gap",
    "Quote indent (em)",
    "List indent (em)",
    "Definition indent (em)",
];
const BOUNDS: [(f32, f32); 17] = [
    (6.0, 72.0),
    (1.0, 4.0),
    (1.0, 4.0),
    (-0.05, 0.3),
    (0.0, 4.0),
    (0.0, 8.0),
    (10.0, 100.0),
    (0.0, 40.0),
    (0.0, 0.5),
    (0.0, 1.0),
    (1.0, 4.0),
    (1.0, 4.0),
    (0.0, 4.0),
    (0.0, 4.0),
    (0.0, 8.0),
    (0.0, 8.0),
    (0.0, 8.0),
];

/// The old eight-slot order, mapped into the complete role order. `None` means the
/// legacy slot has no direct equivalent and is ignored.
const LEGACY_FONT_SLOT: [Option<usize>; 19] = [
    Some(0), Some(1), Some(6), Some(2), Some(2), Some(2),
    Some(3), Some(3), Some(3), Some(4), Some(4), Some(4),
    None, Some(5), None, None, Some(7), None, None,
];

#[derive(Clone, Debug, PartialEq)]
pub struct Profile {
    pub fonts: [String; 19],
    pub numbers: [f32; 17],
    pub keep_korean_words: bool,
}

impl Default for Profile {
    fn default() -> Self {
        Self::from_theme(&Theme::default())
    }
}

impl Profile {
    pub fn from_theme(t: &Theme) -> Self {
        Self {
            fonts: [
                t.fonts.latin[0].clone(),
                t.fonts.latin[1].clone(),
                t.fonts.latin[2].clone(),
                t.fonts.cjk[0].clone(),
                t.fonts.cjk[1].clone(),
                t.fonts.cjk[2].clone(),
                t.fonts.japanese[0].clone(),
                t.fonts.japanese[1].clone(),
                t.fonts.japanese[2].clone(),
                t.fonts.korean[0].clone(),
                t.fonts.korean[1].clone(),
                t.fonts.korean[2].clone(),
                t.fonts.emphasis[0].clone(),
                t.fonts.emphasis[1].clone(),
                t.fonts.emphasis[2].clone(),
                t.fonts.emphasis[3].clone(),
                t.fonts.math[0].clone(),
                t.fonts.math[1].clone(),
                t.fonts.fallback.first().cloned().unwrap_or_default(),
            ],
            numbers: [
                t.design_base,
                t.body_leading.latin,
                t.body_leading.cjk,
                t.tracking_em,
                t.space_before_body,
                t.first_line_indent_em,
                t.max_measure_em,
                t.ragged_below_em,
                t.punctuation_compression,
                t.hanging_punctuation_em,
                t.heading_leading.latin,
                t.heading_leading.cjk,
                t.space_before_heading,
                t.space_before_code,
                t.quote_indent_em,
                t.list_indent_em,
                t.definition_indent_em,
            ],
            keep_korean_words: t.keep_korean_words,
        }
    }

    pub fn book() -> Self {
        let mut p = Self::default();
        p.fonts[0] = "Georgia".into();
        p.fonts[1] = "Georgia".into();
        p.fonts[3] = "SimSun".into();
        p.fonts[4] = "SimSun".into();
        p.fonts[6] = "Yu Mincho".into();
        p.fonts[7] = "Yu Mincho".into();
        p.fonts[9] = "Batang".into();
        p.fonts[10] = "Batang".into();
        p.numbers = [14.0, 1.65, 1.9, 0.0, 0.25, 2.0, 32.0, 16.0, 0.5, 0.5, 1.45, 1.6, 1.5, 1.0, 1.2, 1.6, 1.5];
        p
    }

    pub fn validate(&self) -> Result<(), String> {
        self.validate_with_lang(Language::EnUs)
    }

    pub fn validate_with_lang(&self, lang: Language) -> Result<(), String> {
        for (i, font) in self.fonts.iter().enumerate() {
            let optional = (12..16).contains(&i);
            if (!optional && font.trim().is_empty()) || font.len() > 200 || font.chars().any(char::is_control) {
                let label = i18n::font_label(lang, i);
                return Err(i18n::error_font_required(lang, label));
            }
        }
        for (i, value) in self.numbers.iter().enumerate() {
            let (min, max) = BOUNDS[i];
            if !value.is_finite() || !(min..=max).contains(value) {
                let label = i18n::number_label(lang, i);
                return Err(i18n::error_number_between(lang, label, min, max));
            }
        }
        Ok(())
    }

    pub fn apply(&self, t: &mut Theme) {
        if self.validate().is_err() {
            return;
        }
        t.fonts.latin = [self.fonts[0].clone(), self.fonts[1].clone(), self.fonts[2].clone()];
        t.fonts.cjk = [self.fonts[3].clone(), self.fonts[4].clone(), self.fonts[5].clone()];
        t.fonts.japanese = [self.fonts[6].clone(), self.fonts[7].clone(), self.fonts[8].clone()];
        t.fonts.korean = [self.fonts[9].clone(), self.fonts[10].clone(), self.fonts[11].clone()];
        t.fonts.emphasis = [
            self.fonts[12].clone(),
            self.fonts[13].clone(),
            self.fonts[14].clone(),
            self.fonts[15].clone(),
        ];
        t.fonts.math = [self.fonts[16].clone(), self.fonts[17].clone()];
        t.fonts.fallback = vec![self.fonts[18].clone(), "Microsoft YaHei".into(), "Segoe UI Symbol".into()];
        t.design_base = self.numbers[0];
        t.set_zoom(t.zoom);
        t.body_leading = Leading { latin: self.numbers[1], cjk: self.numbers[2] };
        t.tracking_em = self.numbers[3];
        t.space_before_body = self.numbers[4];
        t.first_line_indent_em = self.numbers[5];
        t.max_measure_em = self.numbers[6];
        t.ragged_below_em = self.numbers[7];
        t.punctuation_compression = self.numbers[8];
        t.hanging_punctuation_em = self.numbers[9];
        t.heading_leading = Leading { latin: self.numbers[10], cjk: self.numbers[11] };
        t.space_before_heading = self.numbers[12];
        t.space_before_code = self.numbers[13];
        t.quote_indent_em = self.numbers[14];
        t.list_indent_em = self.numbers[15];
        t.definition_indent_em = self.numbers[16];
        t.keep_korean_words = self.keep_korean_words;
        t.face = crate::theme::TextFace::ALL
            .iter()
            .position(|f| f.body == self.fonts[0] && f.heading == self.fonts[1])
            .unwrap_or(usize::MAX);
        t.measure = crate::theme::Measure::ALL
            .iter()
            .position(|m| m.em == self.numbers[6])
            .unwrap_or(usize::MAX);
    }
}

fn key(name: &str) -> String {
    let encoded: String = name.as_bytes().iter().map(|b| format!("{b:02x}")).collect();
    format!("{ROOT}\\{encoded}")
}

pub fn names() -> Vec<String> {
    remembered(|memory| &mut memory.names, read_names)
}

fn read_names() -> Vec<String> {
    let mut names = vec!["Default".into(), "Book".into()];
    for name in settings::text(ROOT, "Names").unwrap_or_default().lines().take(100) {
        if !name.is_empty() && !names.iter().any(|n: &String| n.eq_ignore_ascii_case(name)) {
            names.push(name.into());
        }
    }
    names
}

pub fn load(name: &str) -> Profile {
    remembered(|memory| memory.profiles.entry(name.to_string()).or_insert(None), || read_profile(name))
}

fn read_profile(name: &str) -> Profile {
    if name == "Book" {
        return Profile::book();
    }
    let mut p = Profile::default();
    if name == "Default" {
        return p;
    }
    let sub = key(name);
    // A name that is in the list but has no key of its own is a preset an earlier version
    // knew about, or one whose key was taken away by hand. It reads as the defaults --
    // including the East Asian policy, which is on for a key that was never written as
    // much as for a theme that never chose anything else.
    let Some(batch) = settings::Batch::open(&sub) else {
        p.keep_korean_words = true;
        return p;
    };
    for (i, value) in p.fonts.iter_mut().enumerate() {
        if let Some(saved) = batch.text(&format!("FontV2{i}")) {
            *value = saved;
        } else if let Some(old) = LEGACY_FONT_SLOT[i] {
            if let Some(saved) = batch.text(&format!("Font{old}")) {
                *value = saved;
            }
        }
    }
    for (i, value) in p.numbers.iter_mut().enumerate() {
        if let Some(saved) = batch.text(&format!("Number{i}")).and_then(|s| s.parse().ok()) {
            *value = saved;
        }
    }
    p.keep_korean_words = batch.word("KeepKoreanWords") != Some(0);
    if p.validate().is_ok() { p } else { Profile::default() }
}

#[allow(dead_code)]
pub fn save(name: &str, p: &Profile) -> Result<(), String> {
    save_with_lang(name, p, Language::EnUs)
}

pub fn save_with_lang(name: &str, p: &Profile, lang: Language) -> Result<(), String> {
    let saved = write_profile(name, p, lang);
    // Whether it landed or not: a write has just gone through the lists, so nothing
    // remembered about them can be trusted, and the typography form's own page has to see
    // the preset it just saved.
    forget();
    saved
}

fn write_profile(name: &str, p: &Profile, lang: Language) -> Result<(), String> {
    let name = name.trim();
    if name.is_empty() || name.len() > 80 || name.chars().any(char::is_control)
        || ["Default", "Book"].iter().any(|n| name.eq_ignore_ascii_case(n))
    {
        return Err(i18n::t(lang, i18n::Key::ErrorChooseAnotherName).into());
    }
    p.validate_with_lang(lang)?;
    let mut names = names();
    if !names.iter().any(|n| n == name) {
        names.push(name.into());
    }
    if names.len() > 102 {
        return Err(i18n::t(lang, i18n::Key::ErrorTooManyPresets).into());
    }
    let sub = key(name);
    for (i, value) in p.fonts.iter().enumerate() {
        settings::try_write_text(&sub, &format!("FontV2{i}"), value)?;
    }
    for (i, value) in p.numbers.iter().enumerate() {
        settings::try_write_text(&sub, &format!("Number{i}"), &value.to_string())?;
    }
    settings::try_write_word(&sub, "KeepKoreanWords", u32::from(p.keep_korean_words))?;
    settings::try_write_text(ROOT, "Names", &names[2..].join("\n"))?;
    Ok(())
}

pub fn selected(plain: bool) -> String {
    remembered(|memory| memory.selected.entry(plain).or_insert(None), || read_selected(plain))
}

fn read_selected(plain: bool) -> String {
    let name = if plain { settings::text(ROOT, "PlainText") } else { None }
        .or_else(|| settings::text(ROOT, "Selected"))
        .unwrap_or_else(|| "Default".into());
    if names().contains(&name) { name } else { "Default".into() }
}

pub fn select(name: &str, plain: bool) -> Result<(), String> {
    let picked = settings::try_write_text(ROOT, if plain { "PlainText" } else { "Selected" }, name);
    // A different profile is now the selected one, which is what every reader of this
    // module is asking about.
    forget();
    picked
}

/// What this process has already been told about the presets on this machine.
///
/// A start-up asks the same questions several times over -- which preset is selected, what
/// the list of names is, and then what the preset it is about to apply holds -- and each
/// answer is a dozen registry values. So the answers are kept, and every write throws them
/// all away rather than trying to say which of them changed.
static REMEMBERED: LazyLock<Mutex<Memory>> = LazyLock::new(|| Mutex::new(Memory::default()));

#[derive(Default)]
struct Memory {
    names: Option<Vec<String>>,
    selected: HashMap<bool, Option<String>>,
    profiles: HashMap<String, Option<Profile>>,
}

/// Take everything remembered away, so the next question is asked of the registry rather
/// than of a stale answer.
fn forget() {
    if let Ok(mut memory) = REMEMBERED.lock() {
        *memory = Memory::default();
    }
}

/// Answer from `slot` when this process has already asked, and remember what `read` says
/// when it has not.
fn remembered<T: Clone>(
    slot: impl Fn(&mut Memory) -> &mut Option<T>,
    read: impl FnOnce() -> T,
) -> T {
    // The lock is let go of while the registry is being asked again, because one question
    // asks another: which preset is selected asks what the names are. Holding it across a
    // read would be the same lock twice, which is a deadlock rather than a cache.
    if let Ok(mut memory) = REMEMBERED.lock() {
        if let Some(known) = slot(&mut memory) {
            return known.clone();
        }
    }
    let found = read();
    if let Ok(mut memory) = REMEMBERED.lock() {
        *slot(&mut memory) = Some(found.clone());
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_profile_keeps_optional_east_asian_policies_off() {
        let p = Profile::default();
        assert_eq!(p.numbers[8], 0.0);
        assert_eq!(p.numbers[9], 0.0);
        assert!(p.validate().is_ok());
    }

    #[test]
    fn profiles_apply_all_metrics_and_zoom_from_their_own_design_size() {
        let p = Profile::book();
        assert!(p.validate().is_ok(), "book profile is invalid: {:?}", p.validate());
        let mut t = Theme::default();
        p.apply(&mut t);
        t.set_zoom(crate::theme::Zoom::nearest_percent(120.0));
        assert!((t.base - 16.8).abs() < 0.001);
        t.set_zoom(crate::theme::Zoom::DESIGN);
        assert_eq!(Profile::from_theme(&t), p);
        assert_eq!(p.numbers[8], 0.5);
        assert_eq!(p.numbers[9], 0.5);
        let mut invalid = p.clone();
        invalid.numbers[3] = f32::NAN;
        assert!(invalid.validate().is_err());
        invalid.apply(&mut t);
        assert_eq!(Profile::from_theme(&t), p);
    }

    #[test]
    fn complete_font_roles_round_trip_through_theme() {
        let mut p = Profile::default();
        for (i, value) in p.fonts.iter_mut().enumerate() {
            *value = format!("Font {i}");
        }
        let mut t = Theme::default();
        p.apply(&mut t);
        assert_eq!(t.fonts.japanese[2], "Font 8");
        assert_eq!(t.fonts.emphasis[3], "Font 15");
        assert_eq!(t.fonts.math[1], "Font 17");
        assert_eq!(t.fonts.fallback.first().map(String::as_str), Some("Font 18"));
        assert_eq!(Profile::from_theme(&t), p);
    }
}
