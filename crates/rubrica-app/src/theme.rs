//! The typographic design: scale, spacing, and which family serves which script.
//!
//! Only families already installed on Windows are named. Shipping or extracting
//! someone else's font files is not on the table, and a reader that cannot render
//! without them is more fragile than one that picks good system faces.
//!
//! Body serif with sans headings is a deliberate, conventional pairing: the serif
//! carries long-form reading at small sizes while the sans gives headings a change
//! of voice without a change of weight alone.

use rubrica_doc::{BlockKind, InlineStyle};
use rubrica_type::units::Pt;

/// A named role rather than a family, so a theme can remap per language.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Body,
    Heading,
    Mono,
}

/// Per-role family choices. `cjk` wins for Han/Kana/Hangul nodes, which is how
/// one paragraph sets Latin and Chinese from two different faces.
#[derive(Clone, Debug)]
pub struct Fonts {
    pub latin: [String; 3],
    pub cjk: [String; 3],
    /// Tried when a family is missing or a face lacks a glyph.
    pub fallback: Vec<String>,
}

impl Default for Fonts {
    fn default() -> Self {
        let at = |i: usize| ["Segoe UI", "Georgia", "Consolas"][i].to_string();
        Self {
            latin: [at(0), at(1), at(2)],
            cjk: ["Microsoft YaHei".into(), "Microsoft YaHei".into(), "Consolas".into()],
            fallback: vec!["Segoe UI".into(), "Microsoft YaHei".into(), "Segoe UI Symbol".into()],
        }
    }
}

impl Fonts {
    pub fn family(&self, role: Role, cjk: bool) -> &str {
        let list = if cjk { &self.cjk } else { &self.latin };
        &list[role as usize]
    }
}

/// Everything a resolved run needs to be measured and painted.
#[derive(Clone, Debug, PartialEq)]
pub struct ResolvedStyle {
    pub family: String,
    pub cjk_family: String,
    pub size: Pt,
    pub weight: u16,
    pub italic: bool,
    /// Letter spacing in ems, negative for large display sizes.
    pub tracking: f32,
    pub color: ColorRole,
    pub mono: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ColorRole {
    Text,
    Muted,
    Accent,
    Code,
    Faint,
    /// A block surface (the code panel), not an ink: a text colour used as a fill
    /// reads as a smudge rather than a panel.
    Surface,
}

/// Line height as a multiple of the font size. Chinese needs noticeably more
/// leading than Latin at the same point size: ideographs are full-square, so the
/// descender gap that separates Latin lines is absent.
#[derive(Clone, Debug)]
pub struct Leading {
    pub latin: f32,
    pub cjk: f32,
}

impl Leading {
    pub fn for_mixed(&self, mixed: bool) -> f32 {
        if mixed { self.cjk } else { self.latin }
    }
}

#[derive(Clone, Debug)]
pub struct Theme {
    pub fonts: Fonts,
    pub base: Pt,
    /// Heading sizes as a multiple of `base`, index 0 = h1.
    pub heading_scale: [f32; 6],
    pub heading_leading: Leading,
    pub body_leading: Leading,
    /// Vertical space above a block, as multiples of the body line height.
    pub space_before_body: f32,
    pub space_before_heading: f32,
    pub space_before_code: f32,
    /// Longest readable measure, in ems of the body size.
    pub max_measure_em: Pt,
    pub first_line_indent_em: Pt,
    pub quote_indent_em: Pt,
    pub list_indent_em: Pt,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            fonts: Fonts::default(),
            // 13.5pt is ~18px at 96 dpi, the size long-form CJK reading settles on.
            base: 13.5,
            heading_scale: [2.0, 1.6, 1.32, 1.15, 1.0, 0.92],
            heading_leading: Leading { latin: 1.3, cjk: 1.5 },
            body_leading: Leading { latin: 1.7, cjk: 1.95 },
            space_before_body: 0.9,
            space_before_heading: 1.8,
            space_before_code: 1.0,
            max_measure_em: 36.0,
            // Chinese prose conventionally indents the first line two ideographs;
            // Latin prose does not indent at all when paragraphs are separated by
            // space, so this stays small.
            first_line_indent_em: 0.0,
            quote_indent_em: 1.2,
            list_indent_em: 1.6,
        }
    }
}

impl Theme {
    pub fn body_size(&self, kind: BlockKind) -> Pt {
        match kind {
            BlockKind::Heading(l) => {
                let i = (l as usize).clamp(1, 6) - 1;
                self.base * self.heading_scale[i]
            }
            BlockKind::Code => self.base * 0.92,
            BlockKind::Paragraph | BlockKind::Rule | BlockKind::Table => self.base,
        }
    }

    pub fn line_spacing(&self, kind: BlockKind) -> Leading {
        match kind {
            BlockKind::Heading(_) => self.heading_leading.clone(),
            _ => self.body_leading.clone(),
        }
    }

    /// Resolve a block kind plus inline flags into concrete run properties.
    pub fn resolve(&self, kind: BlockKind, inline: InlineStyle) -> ResolvedStyle {
        let role = match inline {
            s if s.contains(InlineStyle::CODE) => Role::Mono,
            _ if kind == BlockKind::Code => Role::Mono,
            _ if matches!(kind, BlockKind::Heading(_)) => Role::Heading,
            _ => Role::Body,
        };
        let size = match kind {
            // Inline code inside a heading should not jump to the mono scale.
            BlockKind::Code => self.base * 0.92,
            BlockKind::Heading(_) => self.body_size(kind) * 0.94,
            _ => self.base,
        };
        let mut weight = match kind {
            BlockKind::Heading(_) => 700,
            BlockKind::Rule => 400,
            BlockKind::Code => 400,
            BlockKind::Paragraph | BlockKind::Table => 400,
        };
        if inline.contains(InlineStyle::STRONG) {
            weight = weight.max(700);
        }
        let color = if inline.contains(InlineStyle::LINK) {
            ColorRole::Accent
        } else if role == Role::Mono {
            ColorRole::Code
        } else if matches!(kind, BlockKind::Heading(_)) {
            ColorRole::Text
        } else if inline.contains(InlineStyle::STRIKETHROUGH) {
            ColorRole::Faint
        } else {
            ColorRole::Text
        };
        // Display sizes lose their natural looseness, so tracking goes negative.
        let tracking = if size > self.base * 1.4 { -0.015 } else { 0.0 };
        ResolvedStyle {
            family: self.fonts.family(role, false).to_string(),
            cjk_family: self.fonts.family(role, true).to_string(),
            size,
            weight,
            italic: inline.contains(InlineStyle::EMPHASIS),
            tracking,
            color,
            mono: role == Role::Mono,
        }
    }

    /// Space above a block, in points.
    pub fn space_before(&self, kind: BlockKind, first: bool) -> Pt {
        if first {
            return self.base * 0.6;
        }
        let lines = match kind {
            BlockKind::Heading(_) => self.space_before_heading,
            BlockKind::Code => self.space_before_code,
            BlockKind::Rule => 1.4,
            BlockKind::Paragraph => self.space_before_body,
            BlockKind::Table => 1.2,
        };
        self.base * lines
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn headings_are_larger_and_bolder_than_body() {
        let t = Theme::default();
        let body = t.resolve(BlockKind::Paragraph, InlineStyle::EMPTY);
        let h1 = t.resolve(BlockKind::Heading(1), InlineStyle::EMPTY);
        assert!(h1.size > body.size * 1.8);
        assert_eq!(h1.weight, 700);
        assert_eq!(body.weight, 400);
        assert!(t.body_size(BlockKind::Heading(1)) > t.body_size(BlockKind::Heading(6)));
    }

    #[test]
    fn chinese_gets_more_leading_than_latin() {
        let t = Theme::default();
        assert!(t.body_leading.cjk > t.body_leading.latin);
        assert!(t.heading_leading.cjk > t.heading_leading.latin);
    }

    #[test]
    fn inline_code_and_links_take_their_own_colour() {
        let t = Theme::default();
        assert_eq!(t.resolve(BlockKind::Paragraph, InlineStyle::CODE).color, ColorRole::Code);
        assert_eq!(t.resolve(BlockKind::Paragraph, InlineStyle::LINK).color, ColorRole::Accent);
        assert_eq!(
            t.resolve(BlockKind::Paragraph, InlineStyle::CODE).family,
            t.fonts.family(Role::Mono, false)
        );
    }

    #[test]
    fn emphasis_only_changes_slant() {
        let t = Theme::default();
        let s = t.resolve(BlockKind::Paragraph, InlineStyle::EMPHASIS);
        assert!(s.italic);
        assert_eq!(s.weight, 400);
    }

    #[test]
    fn large_display_sizes_track_backwards() {
        let t = Theme::default();
        assert!(t.resolve(BlockKind::Heading(1), InlineStyle::EMPTY).tracking < 0.0);
        assert_eq!(t.resolve(BlockKind::Paragraph, InlineStyle::EMPTY).tracking, 0.0);
    }
}
