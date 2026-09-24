//! Named typography presets. Values are validated before reaching layout arithmetic.

use crate::{settings, theme::{Theme, Leading}};

const ROOT: &str = "Software\\Rubrica\\Typography";
pub const FONT_LABELS: [&str; 8] = ["Latin body", "Latin headings", "Chinese", "Japanese", "Korean", "Chinese emphasis", "Code", "Math"];
pub const NUMBER_LABELS: [&str; 10] = ["Size (pt)", "Latin line height", "Asian line height", "Tracking (em)", "Paragraph gap", "First indent (em)", "Column (em)", "Ragged below (em)", "Punctuation compression", "Hanging punctuation (em)"];
const BOUNDS: [(f32, f32); 10] = [(6.0, 72.0), (1.0, 4.0), (1.0, 4.0), (-0.05, 0.3), (0.0, 4.0), (0.0, 8.0), (10.0, 100.0), (0.0, 40.0), (0.0, 0.5), (0.0, 1.0)];

#[derive(Clone, Debug, PartialEq)]
pub struct Profile {
    pub fonts: [String; 8],
    pub numbers: [f32; 10],
    pub keep_korean_words: bool,
}

impl Default for Profile {
    fn default() -> Self { Self::from_theme(&Theme::default()) }
}

impl Profile {
    pub fn from_theme(t: &Theme) -> Self {
        Self {
            fonts: [t.fonts.latin[0].clone(), t.fonts.latin[1].clone(), t.fonts.cjk[0].clone(),
                t.fonts.japanese[0].clone(), t.fonts.korean[0].clone(), t.fonts.emphasis[1].clone(),
                t.fonts.latin[2].clone(), t.fonts.math[0].clone()],
            numbers: [t.design_base, t.body_leading.latin, t.body_leading.cjk, t.tracking_em,
                t.space_before_body, t.first_line_indent_em, t.max_measure_em, t.ragged_below_em,
                t.punctuation_compression, t.hanging_punctuation_em],
            keep_korean_words: t.keep_korean_words,
        }
    }

    pub fn book() -> Self {
        let mut p = Self::default();
        p.fonts[0] = "Georgia".into();
        p.fonts[2] = "SimSun".into();
        p.fonts[3] = "Yu Mincho".into();
        p.fonts[4] = "Batang".into();
        p.numbers = [14.0, 1.65, 1.9, 0.0, 0.25, 2.0, 32.0, 16.0, 0.5, 0.5];
        p
    }

    pub fn validate(&self) -> Result<(), String> {
        for (i, font) in self.fonts.iter().enumerate() {
            if font.trim().is_empty() || font.len() > 200 || font.chars().any(char::is_control) {
                return Err(format!("Enter a font family for {}.", FONT_LABELS[i]));
            }
        }
        for (i, value) in self.numbers.iter().enumerate() {
            let (min, max) = BOUNDS[i];
            if !value.is_finite() || !(min..=max).contains(value) {
                return Err(format!("{} must be between {min} and {max}.", NUMBER_LABELS[i]));
            }
        }
        Ok(())
    }

    pub fn apply(&self, t: &mut Theme) {
        if self.validate().is_err() { return; }
        t.fonts.latin = [self.fonts[0].clone(), self.fonts[1].clone(), self.fonts[6].clone()];
        t.fonts.cjk[0] = self.fonts[2].clone(); t.fonts.cjk[1] = self.fonts[2].clone();
        t.fonts.japanese[0] = self.fonts[3].clone(); t.fonts.japanese[1] = self.fonts[3].clone();
        t.fonts.korean[0] = self.fonts[4].clone(); t.fonts.korean[1] = self.fonts[4].clone();
        t.fonts.emphasis[1] = self.fonts[5].clone();
        t.fonts.math[0] = self.fonts[7].clone();
        t.design_base = self.numbers[0]; t.set_zoom(t.zoom);
        t.body_leading = Leading { latin: self.numbers[1], cjk: self.numbers[2] };
        t.tracking_em = self.numbers[3]; t.space_before_body = self.numbers[4];
        t.first_line_indent_em = self.numbers[5]; t.max_measure_em = self.numbers[6];
        t.ragged_below_em = self.numbers[7]; t.keep_korean_words = self.keep_korean_words;
        t.punctuation_compression = self.numbers[8]; t.hanging_punctuation_em = self.numbers[9];
        t.face = crate::theme::TextFace::ALL.iter().position(|f| f.body == self.fonts[0] && f.heading == self.fonts[1]).unwrap_or(usize::MAX);
        t.measure = crate::theme::Measure::ALL.iter().position(|m| m.em == self.numbers[6]).unwrap_or(usize::MAX);
    }
}

fn key(name: &str) -> String {
    // Encode the name, including slashes, so it is always exactly one registry key.
    let encoded: String = name.as_bytes().iter().map(|b| format!("{b:02x}")).collect();
    format!("{ROOT}\\{encoded}")
}

pub fn names() -> Vec<String> {
    let mut names = vec!["Default".into(), "Book".into()];
    for name in settings::text(ROOT, "Names").unwrap_or_default().lines().take(100) {
        if !name.is_empty() && !names.iter().any(|n: &String| n.eq_ignore_ascii_case(name)) { names.push(name.into()); }
    }
    names
}

pub fn load(name: &str) -> Profile {
    if name == "Book" { return Profile::book(); }
    let mut p = Profile::default();
    if name == "Default" { return p; }
    let sub = key(name);
    for (i, value) in p.fonts.iter_mut().enumerate() {
        if let Some(saved) = settings::text(&sub, &format!("Font{i}")) { *value = saved; }
    }
    for (i, value) in p.numbers.iter_mut().enumerate() {
        if let Some(saved) = settings::text(&sub, &format!("Number{i}")).and_then(|s| s.parse().ok()) { *value = saved; }
    }
    p.keep_korean_words = settings::word(&sub, "KeepKoreanWords") != Some(0);
    if p.validate().is_ok() { p } else { Profile::default() }
}

pub fn save(name: &str, p: &Profile) -> Result<(), String> {
    let name = name.trim();
    if name.is_empty() || name.len() > 80 || name.chars().any(char::is_control)
        || ["Default", "Book"].iter().any(|n| name.eq_ignore_ascii_case(n)) {
        return Err("Choose a name other than Default or Book (up to 80 bytes).".into());
    }
    p.validate()?;
    let mut names = names();
    if !names.iter().any(|n| n == name) { names.push(name.into()); }
    let sub = key(name);
    for (i, value) in p.fonts.iter().enumerate() { settings::write_text(&sub, &format!("Font{i}"), value); }
    for (i, value) in p.numbers.iter().enumerate() { settings::write_text(&sub, &format!("Number{i}"), &value.to_string()); }
    settings::write_word(&sub, "KeepKoreanWords", u32::from(p.keep_korean_words));
    settings::write_text(ROOT, "Names", &names[2..].join("\n"));
    Ok(())
}

pub fn selected(plain: bool) -> String {
    let name = if plain { settings::text(ROOT, "PlainText") } else { None }
        .or_else(|| settings::text(ROOT, "Selected")).unwrap_or_else(|| "Default".into());
    if names().contains(&name) { name } else { "Default".into() }
}

pub fn select(name: &str, plain: bool) {
    settings::write_text(ROOT, if plain { "PlainText" } else { "Selected" }, name);
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
}
