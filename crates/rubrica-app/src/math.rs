//! Math typesetting against the face's own `MATH` table.
//!
//! `rubrica-math` asks four questions of whatever platform it is running on -- what
//! does this text measure, what is this constant, what is this percentage, how does
//! this delimiter grow -- and this module answers them with DirectWrite. Widths come
//! from the same shaper the painter uses, so a formula's box cannot disagree with its
//! ink; the constants and the growing come from the `MATH` table the math face
//! carries, which is what makes the result that face's design rather than a guess.
//!
//! A formula is laid out once per source, size and style and then cached, because
//! two passes need it: line breaking wants its box, and painting wants its shapes.

use rubrica_math::layout::{Extents, MathMeasure, Shape, Stacked};
use rubrica_math::table::{assemble, MathTable};
use rubrica_type::units::Pt;
use windows::Win32::Graphics::DirectWrite::{
    DWRITE_FONT_METRICS, DWRITE_GLYPH_OFFSET, IDWriteFontFace,
};

use crate::font::{FaceRequest, FontEngine, GlyphRun, ObjectBox};

/// One formula, prepared: the box the line has to make room for and the pieces to
/// draw, all relative to the formula's own origin at its left edge and baseline.
pub struct Entry {
    key: (String, i32, bool),
    pub object: ObjectBox,
    /// Whether this formula was measured from the face's `MATH` table rather than from
    /// the layout's fallback constants.
    from_table: bool,
    /// Glyph runs with their x from the origin and their y *below* the baseline, in
    /// points.
    pub parts: Vec<(GlyphRun, Pt, Pt)>,
    /// Bars as x, top edge, width, thickness, in the same space. A bar is drawn
    /// rather than shaped because the assembly API only grows in one direction.
    pub rules: Vec<(Pt, Pt, Pt, Pt)>,
    /// The family that actually carried the formula, recorded from its first run: a
    /// requested math face that is not installed still shapes something, and the
    /// report has to say which.
    family: String,
}

#[derive(Default)]
pub struct MathStore {
    entries: Vec<Entry>,
}

impl MathStore {
    pub fn new() -> MathStore {
        MathStore::default()
    }

    /// Typeset `source` unless it is already typeset, and return its index.
    ///
    /// The key is the source text rather than a document position, so the same
    /// formula repeated across a document -- and across a relayout -- is set once.
    pub fn intern(
        &mut self,
        font: &FontEngine,
        req: &FaceRequest,
        source: &str,
        size: Pt,
        display: bool,
    ) -> Option<usize> {
        let key = (source.to_string(), (size * 64.0).round() as i32, display);
        if let Some(i) = self.entries.iter().position(|e| e.key == key) {
            return Some(i);
        }
        let face = font.open_face(&req.family, req.weight, req.italic);
        let (formula, from_table) = {
            let mut adapter = Adapter::open(font, req, face);
            // Recorded here rather than inferred later, because it is the one fact the
            // report cannot get from the picture: a formula drawn from the face's own
            // `MATH` table and one drawn from fallback constants both look plausible.
            let read = adapter.table().is_some();
            (rubrica_math::typeset(source, size, display, &mut adapter), read)
        };
        let mut parts = Vec::new();
        let mut rules = Vec::new();
        for s in &formula.shapes {
            match s {
                // Shaped through the same entry point the prose uses, so a formula's
                // text and the sentence around it cannot be measured two different ways.
                Shape::Run { text, x, y, size } => {
                    let mut at = *x;
                    for r in font.shape_runs(text, 0..text.len(), req, *size, 0.0) {
                        let w = r.width();
                        parts.push((r, at, *y));
                        at += w;
                    }
                }
                // One part of a grown delimiter, addressed by glyph id because the
                // assembly's pieces are not characters.
                Shape::Glyph { index, x, y, size } => {
                    let Some(face) = face else { continue };
                    let (advance, ascent, descent) = font.glyph_extents(face, *index, *size);
                    parts.push((
                        GlyphRun {
                            face,
                            size: *size,
                            text: 0..0,
                            glyphs: vec![*index],
                            advances: vec![advance],
                            offsets: vec![DWRITE_GLYPH_OFFSET::default()],
                            clusters: vec![0],
                            ascent,
                            descent,
                            line_gap: 0.0,
                        },
                        *x,
                        *y,
                    ));
                }
                Shape::Rule { x, y, width, thickness } => rules.push((*x, *y, *width, *thickness)),
            }
        }
        // The box has to contain the ink. A display limit wider than its operator is
        // centred *under* it, which puts part of the formula left of the origin, and a
        // line that reserved room only for the advance would have the equation hang
        // out over the margin.
        let lo = parts
            .iter()
            .map(|(_, x, _)| *x)
            .chain(rules.iter().map(|(x, _, _, _)| *x))
            .fold(0.0f32, f32::min);
        if lo < 0.0 {
            for (_, x, _) in parts.iter_mut() {
                *x -= lo;
            }
            for (x, _, _, _) in rules.iter_mut() {
                *x -= lo;
            }
        }
        let ink = parts
            .iter()
            .map(|(r, x, _)| *x + r.width())
            .chain(rules.iter().map(|(x, _, w, _)| *x + w))
            .fold(0.0f32, f32::max);
        self.entries.push(Entry {
            key,
            object: ObjectBox {
                advance: formula.width.max(ink),
                ascent: formula.ascent,
                descent: formula.descent,
            },
            from_table,
            family: parts.first().map(|(r, _, _)| font.face_family(r.face)).unwrap_or_default(),
            parts,
            rules,
        });
        Some(self.entries.len() - 1)
    }

    pub fn get(&self, i: usize) -> Option<&Entry> {
        self.entries.get(i)
    }

    /// How many formulas are set, how much of them is shaped rather than drawn, which
    /// families carried them, and how many were measured from a real `MATH` table.
    ///
    /// The headless report prints this, because a formula that silently fell back to
    /// a face with no `MATH` table still draws -- at the wrong shape, with TeX's own
    /// constants -- and a number is the only way to see that from outside the window.
    /// Bars are counted separately because they are rectangles, so nothing else in the
    /// display list tells them apart from a table's header panel.
    pub fn census(&self) -> (usize, usize, Vec<String>, usize) {
        (
            self.entries.len(),
            self.entries.iter().map(|e| e.rules.len()).sum(),
            self.entries.iter().map(|e| e.family.clone()).collect(),
            self.entries.iter().filter(|e| e.from_table).count(),
        )
    }
}

/// The face a formula is set from: its `MATH` table for the numbers, and its glyph
/// ids for the parts that grow.
struct Adapter<'a> {
    font: &'a FontEngine,
    req: &'a FaceRequest,
    face: Option<usize>,
    /// The raw table, copied out so nothing depends on how long DirectWrite keeps its
    /// own pointer valid.
    bytes: Vec<u8>,
    upem: u16,
}

impl<'a> Adapter<'a> {
    fn open(font: &'a FontEngine, req: &'a FaceRequest, face: Option<usize>) -> Adapter<'a> {
        let (bytes, upem) = face
            .and_then(|i| font.font_face(i))
            .and_then(|f| math_table_bytes(&f))
            .unwrap_or_default();
        Adapter { font, req, face, bytes, upem: upem.max(1) }
    }

    fn table(&self) -> Option<MathTable<'_>> {
        MathTable::parse(&self.bytes, self.upem)
    }

    /// Design units to points for this face at `size`.
    fn scale(&self, size: Pt) -> Pt {
        size / self.upem as f32
    }
}

impl MathMeasure for Adapter<'_> {
    fn measure(&mut self, text: &str, size: Pt) -> Extents {
        // The advance comes from the shaper, so the width the line breaks on is the
        // width the painter draws. The ink comes from the glyph outlines instead,
        // because a fraction bar and an accent sit against the real top of their base
        // rather than against the face's tallest ascent.
        let advance: Pt = self
            .font
            .shape_runs(text, 0..text.len(), self.req, size, 0.0)
            .iter()
            .map(|r| r.width())
            .sum();
        let Some(face) = self.face else {
            return Extents { advance, ascent: 0.0, descent: 0.0, italic: 0.0 };
        };
        let mut ascent: Pt = 0.0;
        let mut descent: Pt = 0.0;
        let mut italic: Pt = 0.0;
        let table = self.table();
        for g in self.font.glyph_ids(face, text).unwrap_or_default() {
            let (_, up, down) = self.font.glyph_extents(face, g, size);
            ascent = ascent.max(up);
            descent = descent.max(down);
            if let Some(t) = &table {
                italic = italic.max(t.italics_correction(g) as f32 * self.scale(size));
            }
        }
        Extents { advance, ascent, descent, italic }
    }

    fn constant(&mut self, index: usize, size: Pt) -> Option<Pt> {
        // A documented zero means "no requirement here" and is passed on as such:
        // falling back instead would invent a clearance the type designer declined.
        let t = self.table()?;
        Some(t.constant_pt(index, size))
    }

    fn percent(&mut self, index: usize, fallback: Pt) -> Pt {
        match self.table() {
            Some(t) => t.constant(index) as f32 / 100.0,
            None => fallback,
        }
    }

    fn stretch(&mut self, ch: char, size: Pt, height: Pt) -> Option<Vec<Stacked>> {
        let t = self.table()?;
        let face = self.face?;
        let g = *self.font.glyph_ids(face, &ch.to_string())?.first()?;
        if g == 0 {
            return None;
        }
        let want = (height / self.scale(size)).clamp(0.0, u16::MAX as f32) as u16;
        // A ready-made variant tall enough is the font's own drawing of the taller
        // shape, so it wins; only when the font has none is one built from parts.
        let placed = match t.pick_variant(g, want) {
            Some(v) => vec![rubrica_math::Placed { glyph: v, offset: 0, full_advance: want }],
            None => {
                let c = t.construction(g, true)?;
                if c.assembly.is_empty() {
                    return None;
                }
                assemble(&c.assembly, t.min_connector_overlap(), want)
            }
        };
        let s = self.scale(size);
        let out = placed
            .iter()
            .map(|p| {
                let (advance, up, down) = self.font.glyph_extents(face, p.glyph, size);
                let band = p.full_advance as f32 * s;
                // Each piece is centred in the band the assembly allotted it. The
                // table says where pieces join and never where a piece's baseline
                // sits, so this is the reading that keeps the connectors meeting.
                let baseline_up = p.offset as f32 * s + (band - (up + down)) / 2.0 + down;
                Stacked { index: p.glyph, baseline_up, width: advance }
            })
            .collect();
        Some(out)
    }
}

/// The raw `MATH` table of a face, or `None` when it has none -- which is how a face
/// without math support is reported, and what makes the layout use its fallbacks.
///
/// Read from the file rather than from `TryGetFontTable`, which reports no tables at
/// all for a face from the system collection: without this the whole formula layout
/// silently runs on its fallback constants and looks, on screen, almost right.
fn math_table_bytes(face: &IDWriteFontFace) -> Option<(Vec<u8>, u16)> {
    let mut metrics = DWRITE_FONT_METRICS::default();
    unsafe { face.GetMetrics(&mut metrics) };
    Some((crate::tables::table_bytes(face, b"MATH")?, metrics.designUnitsPerEm))
}
