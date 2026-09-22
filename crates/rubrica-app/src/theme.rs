//! The typographic design: scale, spacing, and which family serves which script.
//!
//! Only families already installed on Windows are named. Shipping or extracting
//! someone else's font files is not on the table, and a reader that cannot render
//! without them is more fragile than one that picks good system faces.
//!
//! Sans body with serif headings is the default pairing, and a deliberate one: the
//! screen face carries long-form reading at small sizes, where a serif's fine strokes
//! go muddy on a low-DPI panel, while the serif gives headings a change of voice rather
//! than a change of weight alone. [`TextFace`] offers the alternatives.

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
    /// The face formulas are set from, then a substitute.
    ///
    /// Named here rather than chosen per role because a `MATH` table is a property of
    /// a face: the layout constants that place every bar and script come from it, so
    /// the choice decides how a formula looks more than any size or weight does.
    pub math: [String; 2],
}

impl Default for Fonts {
    fn default() -> Self {
        // The two Latin reading faces are the first entry of the list the reader chooses
        // from, so the default cannot drift away from what the menu calls the default.
        let pair = TextFace::ALL[0];
        Self {
            latin: [pair.body.to_string(), pair.heading.to_string(), "Consolas".into()],
            cjk: ["Microsoft YaHei".into(), "Microsoft YaHei".into(), "Consolas".into()],
            fallback: vec!["Segoe UI".into(), "Microsoft YaHei".into(), "Segoe UI Symbol".into()],
            math: ["Cambria Math".into(), "Segoe UI Symbol".into()],
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
    /// How far the run sits above the baseline of the line carrying it, in points.
    ///
    /// Positive is **up**, which is the direction the only thing that uses it needs:
    /// a raised citation mark. The painter negates it into the run's drop, and the
    /// line loop counts it into the ascent so a raised run cannot be clipped by the
    /// line above. Zero for prose.
    pub raise: Pt,
    /// Draw a rule through the run. Kept as a flag rather than a position because
    /// where the rule goes is the face's business: it comes from that font's `OS/2`
    /// table at paint time, so a struck Han span and a struck Latin span each sit at
    /// the height their own designer chose.
    pub strike: bool,
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

/// The reader's size preference, as a step on a ladder rather than a free ratio.
///
/// A ladder because a reset key has to be able to name the design size again: with
/// a multiplier carried from press to press, the rounding of the last press is the
/// error of the next `Ctrl`+`0`, and the page never quite comes back to what it was.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Zoom(i32);

impl Zoom {
    /// The design's own size. `Ctrl`+`0` returns here and a fresh theme starts here.
    pub const DESIGN: Zoom = Zoom(0);
    /// Per step. Coarse on purpose, and the browser convention: two sizes the eye
    /// cannot separate are one wasted key press, while a jump big enough to be worth
    /// the press is still small enough not to lose the reader's place in a paragraph.
    const RATIO: f32 = 1.2;
    /// The ends of the ladder. Below the first a line is unreadable rather than
    /// merely small; above the last the measure — which is a number of em, so it
    /// grows with the size — has long since run into the window edge, and the page
    /// is a column of two-word lines.
    const MIN: i32 = -3;
    const MAX: i32 = 4;

    pub fn up(self) -> Zoom {
        Zoom((self.0 + 1).min(Self::MAX))
    }

    pub fn down(self) -> Zoom {
        Zoom((self.0 - 1).max(Self::MIN))
    }

    /// The multiplier to apply to the design's body size. Clamped, so a `Zoom` built
    /// from elsewhere cannot escape the ladder.
    pub fn factor(self) -> f32 {
        Self::RATIO.powi(self.0.clamp(Self::MIN, Self::MAX))
    }

    /// As a percentage of the design, which is the unit a reader is shown: 100 at
    /// [`Zoom::DESIGN`].
    pub fn percent(self) -> u32 {
        (f64::from(self.factor()) * 100.0).round() as u32
    }

    /// The step nearest a requested percentage, for input that is not key presses —
    /// a saved preference, or the headless report's `--zoom`.
    pub fn nearest_percent(pct: f32) -> Zoom {
        let mut best = Self::DESIGN;
        let mut closest = f32::INFINITY;
        for step in Self::MIN..=Self::MAX {
            let z = Zoom(step);
            let d = (z.percent() as f32 - pct).abs();
            if d < closest {
                closest = d;
                best = z;
            }
        }
        best
    }
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

/// A face pairing the reader can ask for, and the name it is asked for by.
///
/// Body and heading are chosen as one pair rather than as two settings, because the
/// pairing is the thing that has been judged: a body face swapped on its own leaves the
/// headings in whatever face they had, and two serifs that were never picked to sit
/// together read as a mistake rather than as a choice.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TextFace {
    pub label: &'static str,
    pub body: &'static str,
    pub heading: &'static str,
}

impl TextFace {
    /// Everything the reader is offered, in the order they see it. The first entry is
    /// what a reader who never opens the menu gets, and [`Theme::set_face`] indexes
    /// these.
    pub const ALL: [TextFace; 4] = [
        TextFace { label: "Default", body: "Segoe UI", heading: "Georgia" },
        TextFace { label: "Serif", body: "Georgia", heading: "Georgia" },
        TextFace { label: "Humanist", body: "Candara", heading: "Candara" },
        TextFace { label: "Monospace", body: "Consolas", heading: "Consolas" },
    ];
}

#[derive(Clone, Debug)]
pub struct Theme {
    pub fonts: Fonts,
    /// The body size being read right now, in points: the design's own size scaled by
    /// [`Theme::zoom`]. Written only by [`Theme::set_zoom`], because everything else
    /// here is stated in ems of it.
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
    ///
    /// Ems rather than points, so it is a number of characters: a glyph is about half
    /// an em wide in Latin and a whole em in Chinese, so a measure that is a length in
    /// ems holds the same count of them at any size. Zooming changes the size of the
    /// page, not how much text a line carries.
    pub max_measure_em: Pt,
    pub first_line_indent_em: Pt,
    pub quote_indent_em: Pt,
    pub list_indent_em: Pt,
    /// The reader's size preference, applied to [`Theme::base`] by [`Theme::set_zoom`].
    /// Carried by the theme so a relayout needs only the theme it is already handed.
    pub zoom: Zoom,
    /// Which of [`TextFace::ALL`] the Latin faces were taken from. Kept as an index
    /// rather than read back off [`Theme::fonts`], because the menu has to check the row
    /// that is really on the page and two pairings could share a body face.
    pub face: usize,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            fonts: Fonts::default(),
            base: Self::DESIGN_BASE,
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
            zoom: Zoom::DESIGN,
            face: 0,
        }
    }
}

impl Theme {
    /// 13.5pt is ~18px at 96 dpi, the size long-form CJK reading settles on.
    ///
    /// A constant rather than a value remembered on the instance: the live [`Theme::base`]
    /// is always this number times the zoom and never a step off the last press, so
    /// `Ctrl`+`0` lands exactly here however far the reader had wandered.
    pub const DESIGN_BASE: Pt = 13.5;

    /// Read the page at a new size.
    ///
    /// One number carries the whole zoom because every other metric in a `Theme` is
    /// already an em of it: leading, block spacing, indents and the measure all follow
    /// the body size, so the page grows as a design rather than as text in a frame.
    pub fn set_zoom(&mut self, zoom: Zoom) {
        self.zoom = zoom;
        self.base = Self::DESIGN_BASE * zoom.factor();
    }

    /// Set the page in one of the offered pairings, by its index in [`TextFace::ALL`].
    ///
    /// Only the Latin roles move. The Han faces stay as they are because no pairing here
    /// is a Han face: a reader asking for serif Latin prose still gets a Chinese
    /// paragraph in the one family that draws it properly.
    pub fn set_face(&mut self, face: usize) {
        let Some(f) = TextFace::ALL.get(face) else { return };
        self.face = face;
        self.fonts.latin[Role::Body as usize] = f.body.to_string();
        self.fonts.latin[Role::Heading as usize] = f.heading.to_string();
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

    /// The size a footnote's prose is set at: smaller than the page, so the
    /// apparatus reads as apparatus rather than as more of the argument.
    pub fn note_size(&self) -> Pt {
        self.base * 0.82
    }

    /// As [`Theme::body_size`], for a block *inside* a note. A heading in a note
    /// keeps its ratio to body rather than collapsing to the note's size, because a
    /// definition's structure is the author's, not the reader's to flatten.
    pub fn note_body_size(&self, kind: BlockKind) -> Pt {
        self.note_size() * (self.body_size(kind) / self.base)
    }

    /// Leading for a note, tighter than body prose: the smaller size already closes
    /// the lines up, and a note is meant to read as one item rather than as a page.
    pub fn note_leading(&self) -> Leading {
        Leading { latin: 1.45, cjk: 1.6 }
    }

    /// Space above a note, or above the rule that opens the list when `first`.
    /// Takes the same `first` flag as [`Theme::space_before`] for the same job.
    pub fn note_space(&self, first: bool) -> Pt {
        self.base * if first { 1.4 } else { 0.45 }
    }

    /// Resolve a block kind plus inline flags into concrete run properties.
    pub fn resolve(&self, kind: BlockKind, inline: InlineStyle) -> ResolvedStyle {
        self.resolve_at(kind, inline, self.body_size(kind))
    }

    /// As [`Theme::resolve`], for a block whose size the caller has chosen rather
    /// than taken from its kind -- a footnote's prose, which is Markdown body text
    /// set at the note's scale.
    pub fn resolve_at(&self, kind: BlockKind, inline: InlineStyle, block: Pt) -> ResolvedStyle {
        let role = match inline {
            s if s.contains(InlineStyle::CODE) => Role::Mono,
            _ if kind == BlockKind::Code => Role::Mono,
            _ if matches!(kind, BlockKind::Heading(_)) => Role::Heading,
            _ => Role::Body,
        };
        let mut size = match kind {
            // Inline code inside a heading should not jump to the mono scale.
            BlockKind::Heading(_) => block * 0.94,
            _ => block,
        };
        // A raised mark: smaller than its word, and seated near its cap height,
        // which is where a superscript belongs whatever the surrounding size is.
        let mut raise = 0.0;
        if inline.contains(InlineStyle::SUPERSCRIPT) {
            size *= 0.7;
            raise = block * 0.62;
        }
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
            raise,
            strike: inline.contains(InlineStyle::STRIKETHROUGH),
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
    fn a_chosen_face_moves_the_running_text_and_leaves_the_rest_alone() {
        let mut t = Theme::default();
        // What the reader gets without asking is the first thing the menu offers, read
        // from the same table the menu reads.
        assert_eq!(t.face, 0);
        assert_eq!(t.fonts.family(Role::Body, false), TextFace::ALL[0].body);
        assert_eq!(t.fonts.family(Role::Heading, false), TextFace::ALL[0].heading);

        t.set_face(3);
        assert_eq!(t.fonts.family(Role::Body, false), "Consolas");
        assert_eq!(t.fonts.family(Role::Heading, false), "Consolas");
        // A code span, a formula and a Chinese paragraph are set by their own rules, and
        // no Latin pairing has any business rewriting them.
        assert_eq!(t.fonts.family(Role::Mono, false), "Consolas");
        assert_eq!(t.fonts.family(Role::Body, true), "Microsoft YaHei");
        assert_eq!(t.fonts.math[0], "Cambria Math");
        // A face that is not on the list asks for nothing.
        t.set_face(TextFace::ALL.len() + 1);
        assert_eq!(t.face, 3);
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

    #[test]
    fn a_superscript_is_smaller_raised_and_keeps_its_words_voice() {
        let t = Theme::default();
        let body = t.resolve(BlockKind::Paragraph, InlineStyle::EMPTY);
        let sup = t.resolve(BlockKind::Paragraph, InlineStyle::SUPERSCRIPT);
        assert!((sup.size - body.size * 0.7).abs() < 0.01, "raised text is 0.7em of its word");
        assert!(sup.raise > 0.0, "a raised run must be lifted");
        assert_eq!(body.raise, 0.0, "only a raised run moves off the baseline");
        // The mark belongs to the word before it, so a different face or ink colour
        // would read as a second word rather than as a mark on the first.
        assert_eq!(sup.family, body.family);
        assert_eq!(sup.color, body.color);
        assert_eq!(sup.weight, body.weight);
        // In a heading it scales with the heading instead of collapsing to body.
        let h = t.resolve(BlockKind::Heading(1), InlineStyle::SUPERSCRIPT);
        assert!(h.size > sup.size * 1.5, "{h:?} vs {sup:?}");
        assert!(h.raise > sup.raise, "a bigger word needs a bigger lift");
    }

    #[test]
    fn notes_are_smaller_and_tighter_than_the_prose_they_annotate() {        let t = Theme::default();
        let body = t.body_size(BlockKind::Paragraph);
        let note = t.note_body_size(BlockKind::Paragraph);
        assert!(note < body, "a note at {note} should sit under body prose at {body}");
        assert!(t.note_leading().latin < t.body_leading.latin);
        assert!(t.note_leading().cjk < t.body_leading.cjk);
        // A note's own structure survives the smaller size: a heading in a note is
        // still bigger than the note's prose, and still smaller than a real heading.
        let nh = t.note_body_size(BlockKind::Heading(1));
        assert!(nh > note);
        assert!(nh < t.body_size(BlockKind::Heading(1)));
        assert!(t.note_body_size(BlockKind::Heading(1)) > t.note_body_size(BlockKind::Heading(3)));
    }

    /// Zooming is one number, and that is the claim worth testing: if any metric were
    /// stated in absolute points instead of ems of the body size, it would stay behind
    /// and the page would reflow into a shape nobody designed.
    #[test]
    fn zoom_scales_every_metric_by_the_same_ratio() {
        let design = Theme::default();
        let ratio = Zoom::DESIGN.up().up().up().factor();
        assert_eq!(design.zoom, Zoom::DESIGN, "a fresh theme is the design size");
        assert_eq!(design.base, Theme::DESIGN_BASE);
        let mut t = design.clone();
        t.set_zoom(Zoom::DESIGN.up().up().up());
        assert!(
            (t.base - design.base * ratio).abs() < 1e-3,
            "body should be {}pt, got {}",
            design.base * ratio,
            t.base
        );
        for (a, b) in [
            (t.body_size(BlockKind::Heading(2)), design.body_size(BlockKind::Heading(2))),
            (t.body_size(BlockKind::Code), design.body_size(BlockKind::Code)),
            (t.note_size(), design.note_size()),
            (t.space_before(BlockKind::Paragraph, false), design.space_before(BlockKind::Paragraph, false)),
            (t.max_measure_em * t.base, design.max_measure_em * design.base),
        ] {
            assert!(
                (a - b * ratio).abs() < 0.02,
                "metric scaled by {} instead of {ratio}",
                a / b
            );
        }
        // Ratios the reader has not asked to change must not change: a heading is the
        // same multiple of body at every size, or zooming would be re-typesetting.
        assert!(
            (t.body_size(BlockKind::Heading(1)) / t.base
                - design.body_size(BlockKind::Heading(1)) / design.base)
                .abs()
                < 1e-4
        );
    }

    #[test]
    fn zoom_resets_exactly_and_cannot_run_away() {
        let mut t = Theme::default();
        for _ in 0..20 {
            t.set_zoom(t.zoom.up());
        }
        assert!(t.base < Theme::DESIGN_BASE * 2.5, "the ladder has a top: {}", t.base);
        let largest = t.zoom;
        for _ in 0..40 {
            t.set_zoom(t.zoom.down());
        }
        assert!(t.base > 6.0, "and a bottom, but not an unreadable one: {}", t.base);
        t.set_zoom(Zoom::DESIGN);
        assert_eq!(t.base, Theme::DESIGN_BASE, "Ctrl+0 must return to the design size");
        // Stepping back and forth lands on the same size each time, which is only true
        // because the factor comes from the step and not from the last press.
        let a = {
            let mut t = Theme::default();
            for _ in 0..5 {
                t.set_zoom(t.zoom.up());
            }
            for _ in 0..3 {
                t.set_zoom(t.zoom.down());
            }
            t.base
        };
        let b = {
            let mut t = Theme::default();
            t.set_zoom(t.zoom.down());
            t.set_zoom(t.zoom.up());
            t.set_zoom(t.zoom.up());
            t.base
        };
        assert!((a - b).abs() < 1e-4, "{a} vs {b}");
        assert!(largest.percent() > 100 && Zoom::DESIGN.percent() == 100);
    }

    #[test]
    fn a_requested_percentage_lands_on_the_nearest_step() {
        assert_eq!(Zoom::nearest_percent(100.0), Zoom::DESIGN);
        // 170% is not on the ladder; the steps around it are 144 and 173.
        assert_eq!(Zoom::nearest_percent(170.0), Zoom::DESIGN.up().up().up());
        assert_eq!(Zoom::nearest_percent(1.0), Zoom::nearest_percent(-50.0), "clamped at the bottom");
        assert_eq!(
            Zoom::nearest_percent(9999.0),
            Zoom::DESIGN.up().up().up().up(),
            "and at the top"
        );
    }
}
