//! Reader for the OpenType `MATH` table.
//!
//! Everything here is derived from the public font-format specification: the
//! constants are addressed by their documented order, and the variant and assembly
//! subtables are walked as documented. What the MATH table gives a layout engine is
//! font-specific numbers -- where a fraction bar sits, how far a subscript drops,
//! which glyphs can grow -- and reading them is what makes the output match the
//! design of the face in use rather than an approximation of some other font.
//!
//! Values come back in design units; [`MathTable::scale`] converts them for a given
//! pixel size, and callers should never divide by `units_per_em` by hand.

/// Byte offsets of the constants, in the documented field order. The first four are
/// bare 16-bit values; the rest are MathValueRecords (a value plus a device-table
/// offset), and the last is bare again.
pub mod constant {
    pub const SCRIPT_PERCENT_SCALE_DOWN: usize = 0;
    pub const SCRIPT_SCRIPT_PERCENT_SCALE_DOWN: usize = 1;
    pub const DELIMITED_SUB_FORMULA_MIN_HEIGHT: usize = 2;
    pub const DISPLAY_OPERATOR_MIN_HEIGHT: usize = 3;
    pub const MATH_LEADING: usize = 4;
    pub const AXIS_HEIGHT: usize = 5;
    pub const ACCENT_BASE_HEIGHT: usize = 6;
    pub const FLATTENED_ACCENT_BASE_HEIGHT: usize = 7;
    pub const SUBSCRIPT_SHIFT_DOWN: usize = 8;
    pub const SUBSCRIPT_TOP_MAX: usize = 9;
    pub const SUBSCRIPT_BASELINE_DROP_MIN: usize = 10;
    pub const SUPERSCRIPT_SHIFT_UP: usize = 11;
    pub const SUPERSCRIPT_SHIFT_UP_CRAMPED: usize = 12;
    pub const SUPERSCRIPT_BOTTOM_MIN: usize = 13;
    pub const SUPERSCRIPT_BASELINE_DROP_MAX: usize = 14;
    pub const SUB_SUPERSCRIPT_GAP_MIN: usize = 15;
    pub const SUPERSCRIPT_BOTTOM_MAX_WITH_SUBSCRIPT: usize = 16;
    pub const SPACE_AFTER_SCRIPT: usize = 17;
    pub const UPPER_LIMIT_GAP_MIN: usize = 18;
    pub const UPPER_LIMIT_BASELINE_RISE_MIN: usize = 19;
    pub const LOWER_LIMIT_GAP_MIN: usize = 20;
    pub const LOWER_LIMIT_BASELINE_DROP_MIN: usize = 21;
    pub const STACK_TOP_SHIFT_UP: usize = 22;
    pub const STACK_TOP_DISPLAY_STYLE_SHIFT_UP: usize = 23;
    pub const STACK_BOTTOM_SHIFT_DOWN: usize = 24;
    pub const STACK_BOTTOM_DISPLAY_STYLE_SHIFT_DOWN: usize = 25;
    pub const STACK_GAP_MIN: usize = 26;
    pub const STACK_DISPLAY_STYLE_GAP_MIN: usize = 27;
    pub const STRETCH_STACK_TOP_SHIFT_UP: usize = 28;
    pub const STRETCH_STACK_BOTTOM_SHIFT_DOWN: usize = 29;
    pub const STRETCH_STACK_GAP_ABOVE_MIN: usize = 30;
    pub const STRETCH_STACK_GAP_BELOW_MIN: usize = 31;
    pub const FRACTION_NUMERATOR_SHIFT_UP: usize = 32;
    pub const FRACTION_NUMERATOR_DISPLAY_STYLE_SHIFT_UP: usize = 33;
    pub const FRACTION_DENOMINATOR_SHIFT_DOWN: usize = 34;
    pub const FRACTION_DENOMINATOR_DISPLAY_STYLE_SHIFT_DOWN: usize = 35;
    pub const FRACTION_NUMERATOR_GAP_MIN: usize = 36;
    pub const FRACTION_NUM_DISPLAY_STYLE_GAP_MIN: usize = 37;
    pub const FRACTION_RULE_THICKNESS: usize = 38;
    pub const FRACTION_DENOMINATOR_GAP_MIN: usize = 39;
    pub const FRACTION_DENOM_DISPLAY_STYLE_GAP_MIN: usize = 40;
    pub const SKEWED_FRACTION_HORIZONTAL_GAP: usize = 41;
    pub const SKEWED_FRACTION_VERTICAL_GAP: usize = 42;
    pub const OVERBAR_VERTICAL_GAP: usize = 43;
    pub const OVERBAR_RULE_THICKNESS: usize = 44;
    pub const OVERBAR_EXTRA_ASCENDER: usize = 45;
    pub const UNDERBAR_VERTICAL_GAP: usize = 46;
    pub const UNDERBAR_RULE_THICKNESS: usize = 47;
    pub const UNDERBAR_EXTRA_DESCENDER: usize = 48;
    pub const RADICAL_VERTICAL_GAP: usize = 49;
    pub const RADICAL_DISPLAY_STYLE_VERTICAL_GAP: usize = 50;
    pub const RADICAL_RULE_THICKNESS: usize = 51;
    pub const RADICAL_EXTRA_ASCENDER: usize = 52;
    pub const RADICAL_KERN_BEFORE_DEGREE: usize = 53;
    pub const RADICAL_KERN_AFTER_DEGREE: usize = 54;
    pub const RADICAL_DEGREE_BOTTOM_RAISE_PERCENT: usize = 55;

    /// Number of documented constants; guards a table read from a malformed font.
    pub const COUNT: usize = 56;
}

/// Four-character table tag, as a big-endian u32 so it can be handed straight to
/// DirectWrite's `TryGetFontTable`.
pub const fn tag(a: u8, b: u8, c: u8, d: u8) -> u32 {
    ((a as u32) << 24) | ((b as u32) << 16) | ((c as u32) << 8) | d as u32
}

pub const MATH_TAG: u32 = tag(b'M', b'A', b'T', b'H');

/// A glyph that can grow, with the measurement that matters for choosing it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Variant {
    pub glyph: u16,
    /// Advance in the direction of extension, in design units.
    pub measurement: u16,
}

/// One piece of a constructed shape.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Part {
    pub glyph: u16,
    pub start_connector: u16,
    pub end_connector: u16,
    pub full_advance: u16,
    /// Set when the part may be repeated or skipped to reach a target size.
    pub extender: bool,
}

/// How one glyph grows.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Construction {
    pub variants: Vec<Variant>,
    /// Parts ordered bottom-to-top (vertical) or left-to-right (horizontal).
    pub assembly: Vec<Part>,
    pub italics_correction: i16,
}

/// Parsed view of a font's MATH table. Borrows the table bytes, which stay owned by
/// the font face that handed them out.
pub struct MathTable<'a> {
    data: &'a [u8],
    constants: usize,
    glyph_info: usize,
    variants: usize,
    pub units_per_em: u16,
}

impl<'a> MathTable<'a> {
    /// Parse `data`, the raw `MATH` table. `None` if it is truncated or is not a
    /// version 1 table, which is how a font without MATH is reported.
    pub fn parse(data: &'a [u8], units_per_em: u16) -> Option<MathTable<'a>> {
        let r = Reader::new(data);
        if data.len() < 10 || r.u16(0) != 1 {
            return None;
        }
        let t = MathTable {
            data,
            constants: r.u16(4) as usize,
            glyph_info: r.u16(6) as usize,
            variants: r.u16(8) as usize,
            units_per_em: units_per_em.max(1),
        };
        // A table whose offsets point outside itself is not worth trusting.
        if t.constants + constant::COUNT * 4 > data.len() || t.variants > data.len() {
            return None;
        }
        Some(t)
    }

    #[inline]
    pub fn has_math(&self) -> bool {
        self.constants != 0
    }

    /// Design units to points at `size`.
    #[inline]
    pub fn scale(&self, size: f32) -> f32 {
        size / self.units_per_em as f32
    }

    /// A constant in design units, or 0 when the index is out of range.
    pub fn constant(&self, index: usize) -> i16 {
        if index >= constant::COUNT {
            return 0;
        }
        let r = Reader::new(self.data);
        let off = self.constants;
        if index < 4 {
            r.i16(off + index * 2)
        } else if index == constant::COUNT - 1 {
            // The final field is a bare int16 again, after 51 value records.
            r.i16(off + 8 + 51 * 4)
        } else {
            r.i16(off + 8 + (index - 4) * 4)
        }
    }

    /// A constant converted to points at `size`.
    pub fn constant_pt(&self, index: usize, size: f32) -> f32 {
        self.constant(index) as f32 * self.scale(size)
    }

    /// Coverage index of `glyph` in a Coverage table, or `None` when absent.
    fn coverage_index(&self, cov: usize, glyph: u16) -> Option<u32> {
        let r = Reader::new(self.data);
        if cov == 0 || cov >= self.data.len() {
            return None;
        }
        match r.u16(cov) {
            1 => {
                let n = r.u16(cov + 2) as usize;
                (0..n).find(|&i| r.u16(cov + 4 + i * 2) == glyph).map(|i| i as u32)
            }
            2 => {
                // Each range carries its own starting index, so the answer is that
                // index plus the glyph's distance into the range -- and the ranges
                // before it are none of the arithmetic.
                let n = r.u16(cov + 2) as usize;
                for i in 0..n {
                    let at = cov + 4 + i * 6;
                    let base = r.u16(at);
                    let last = r.u16(at + 2);
                    if glyph >= base && glyph <= last {
                        return Some(r.u16(at + 4) as u32 + (glyph - base) as u32);
                    }
                }
                None
            }
            _ => None,
        }
    }

    /// Italics correction for a glyph, in design units. Zero for glyphs the font
    /// does not correct, which is most non-math glyphs.
    pub fn italics_correction(&self, glyph: u16) -> i16 {
        let r = Reader::new(self.data);
        if self.glyph_info + 2 > self.data.len() {
            return 0;
        }
        let info = self.glyph_info + r.u16(self.glyph_info) as usize;
        if info + 4 > self.data.len() {
            return 0;
        }
        // Both offsets are from the beginning of the italics table, not the file.
        let cov = rel(&r, info, 0);
        let n = r.u16(info + 2) as usize;
        let i = match self.coverage_index(cov, glyph) {
            Some(i) if (i as usize) < n => i as usize,
            _ => return 0,
        };
        r.i16(info + 4 + i * 4)
    }

    /// How a glyph grows in the given direction.
    pub fn construction(&self, glyph: u16, vertical: bool) -> Option<Construction> {
        let r = Reader::new(self.data);
        let mv = self.variants;
        if mv + 10 > self.data.len() {
            return None;
        }
        // Both coverage offsets, and every offset below them, are measured from the
        // start of the GlyphVariation table. Read as positions in the file they point
        // into the constants instead, and no glyph in any font is ever found -- which
        // is silent, because the caller's fallback is to draw the delimiter small.
        let cov = mv + r.u16(mv + if vertical { 2 } else { 4 }) as usize;
        let vert_count = r.u16(mv + 6) as usize;
        let count = r.u16(mv + if vertical { 6 } else { 8 }) as usize;
        let idx = self.coverage_index(cov, glyph)? as usize;
        if idx >= count {
            return None;
        }
        // The two offset arrays follow the header back to back, vertical first, and
        // every offset in the table is measured from the table's own start.
        let array = mv + 10 + if vertical { 0 } else { vert_count * 2 };
        let cons = mv + r.u16(array + idx * 2) as usize;
        if cons + 4 > self.data.len() {
            return None;
        }
        let assembly_off = r.u16(cons) as usize;
        let vcount = r.u16(cons + 2) as usize;
        let mut out = Construction::default();
        for i in 0..vcount {
            let at = cons + 4 + i * 4;
            if at + 4 > self.data.len() {
                break;
            }
            out.variants.push(Variant {
                glyph: r.u16(at),
                measurement: r.u16(at + 2),
            });
        }
        if assembly_off != 0 {
            let asm = cons + assembly_off;
            if asm + 4 <= self.data.len() {
                out.italics_correction = r.i16(asm);
                let pc = r.u16(asm + 2) as usize;
                for i in 0..pc {
                    let at = asm + 4 + i * 10;
                    if at + 10 > self.data.len() {
                        break;
                    }
                    out.assembly.push(Part {
                        glyph: r.u16(at),
                        start_connector: r.u16(at + 2),
                        end_connector: r.u16(at + 4),
                        full_advance: r.u16(at + 6),
                        extender: (r.u16(at + 8) & 1) != 0,
                    });
                }
            }
        }
        Some(out)
    }

    /// Minimum overlap two assembly parts must keep, in design units.
    pub fn min_connector_overlap(&self) -> u16 {
        let r = Reader::new(self.data);
        if self.variants + 2 > self.data.len() {
            return 0;
        }
        r.u16(self.variants)
    }

    /// The parts to build `glyph` out of to reach `want` design units tall, ordered
    /// bottom-up, or `None` when the font says nothing about growing this glyph.
    pub fn grow(&self, glyph: u16, want: u16) -> Option<Vec<Placed>> {
        self.grown(glyph, want, true)
    }

    /// The parts to build `glyph` out of to reach `want` design units *wide*, ordered
    /// left to right: the other direction of the same table, which is where a face
    /// keeps the wider drawings of `\widehat` and `\widetilde`.
    pub fn widen(&self, glyph: u16, want: u16) -> Option<Vec<Placed>> {
        self.grown(glyph, want, false)
    }

    /// Three answers in order of preference, and the same three in either direction.
    /// A ready-made variant at least as big as the target is the type designer's own
    /// drawing of the bigger shape, so it wins. Failing that, an assembly of parts is
    /// built out to the target. Failing *that* -- which is the ordinary case for a
    /// parenthesis, since a curve has no straight section to repeat -- the largest
    /// variant the font has comes back instead of nothing. It is short of the target,
    /// but it is far closer than the natural glyph, which would otherwise sit one em
    /// tall beside a grid four em high.
    ///
    /// One statement rather than two because a face that lists a glyph in one coverage
    /// and not the other is ordinary, not an error: a parenthesis has heights and a hat
    /// has widths, and the question asked of either list is the same question.
    fn grown(&self, glyph: u16, want: u16, vertical: bool) -> Option<Vec<Placed>> {
        let c = self.construction(glyph, vertical)?;
        let whole = |v: &Variant| {
            vec![Placed { glyph: v.glyph, offset: 0, full_advance: v.measurement }]
        };
        if let Some(v) = c
            .variants
            .iter()
            .filter(|v| v.measurement >= want)
            .min_by_key(|v| v.measurement)
        {
            return Some(whole(v));
        }
        if !c.assembly.is_empty() {
            let built = assemble(&c.assembly, self.min_connector_overlap(), want);
            if !built.is_empty() {
                return Some(built);
            }
        }
        c.variants.iter().max_by_key(|v| v.measurement).map(whole)
    }
}

/// One part of an assembled shape, with the distance of its bottom edge from the
/// bottom of the whole, in design units.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Placed {
    pub glyph: u16,
    pub offset: u16,
    pub full_advance: u16,
}

/// The largest and smallest height a sequence of parts can reach.
///
/// Smallest: neighbours overlap by as much as their shared connector allows,
/// `min(end of one, start of the next)`. Largest: they overlap only by
/// `min_connector_overlap`, the least the font will tolerate.
fn span(seq: &[&Part], min_overlap: u16) -> (u32, u32) {
    let total: u32 = seq.iter().map(|p| p.full_advance as u32).sum();
    let (mut max_sum, mut min_sum) = (0u32, 0u32);
    for w in seq.windows(2) {
        let shared = (w[0].end_connector as u32).min(w[1].start_connector as u32);
        max_sum += shared;
        min_sum += shared.min(min_overlap as u32);
    }
    (total - max_sum, total - min_sum)
}

/// Parts in order, with `reps` copies of every extender left in place of each one.
///
/// Keeping the original order is what puts the repeats in the middle of a
/// bottom/extender/top assembly, which is where a brace's straight section goes.
fn weave<'p>(parts: &'p [&'p Part], reps: usize) -> Vec<&'p Part> {
    let mut out = Vec::with_capacity(parts.len());
    for &p in parts {
        if p.extender {
            out.extend(std::iter::repeat_n(p, reps));
        } else {
            out.push(p);
        }
    }
    out
}

/// Grow a GlyphAssembly to `target` design units, following the three steps the
/// specification lays out: assemble the parts without extenders at maximum overlap,
/// open the connections out equally as far as they will go, and only then add one of
/// each extender and start over. Doing it in any other order makes the shape lopsided,
/// because repeating a bar before the joints have given all they can overshoot.
pub fn assemble(parts: &[Part], min_overlap: u16, target: u16) -> Vec<Placed> {
    if parts.is_empty() {
        return Vec::new();
    }
    let refs: Vec<&Part> = parts.iter().collect();

    // A shape with no extender can only reach its own maximum; `>= 64` reps would
    // never help, and the caller is told to take the largest result available.
    for reps in 0..=64usize {
        // `reps` of n puts n copies of every extender back at its own position, so
        // step one -- no extenders at all -- is simply n = 0, and a bottom/extender/top
        // assembly stays the right way round as they are added.
        let seq = weave(&refs, reps);
        if seq.is_empty() {
            continue;
        }
        let (small, large) = span(&seq, min_overlap);
        if target as u32 <= large || reps == 64 {
            return lay_out(&seq, min_overlap, target as u32 - small.min(target as u32));
        }
    }
    Vec::new()
}

fn lay_out(seq: &[&Part], min_overlap: u16, extra: u32) -> Vec<Placed> {
    let joints = seq.len().saturating_sub(1);
    let room: Vec<u32> = seq
        .windows(2)
        .map(|w| {
            let shared = (w[0].end_connector as u32).min(w[1].start_connector as u32);
            shared - shared.min(min_overlap as u32)
        })
        .collect();
    let per = if joints == 0 { 0 } else { extra / joints as u32 };
    let mut left = extra - per * joints as u32;

    let mut out = Vec::with_capacity(seq.len());
    let mut at = 0u32;
    for (i, p) in seq.iter().enumerate() {
        out.push(Placed {
            glyph: p.glyph,
            offset: at as u16,
            full_advance: p.full_advance,
        });
        if let Some(next) = seq.get(i + 1) {
            let shared = (p.end_connector as u32).min(next.start_connector as u32);
            let give = (per + (left != 0) as u32).min(room[i]).min(shared);
            left = left.saturating_sub(1);
            at += p.full_advance as u32 - (shared - give);
        }
    }
    out
}

/// Big-endian accessor that never panics: reads past the end give 0, so a truncated
/// or hostile font degrades to missing data rather than aborting a paint.
struct Reader<'a>(&'a [u8]);

/// Resolve an Offset16 field at `at` against the subtable starting at `base`.
/// Every offset in the MATH table is measured from the beginning of its own parent
/// table, never from the file, so mixing the two silently reads the wrong glyph.
fn rel(r: &Reader<'_>, base: usize, at: usize) -> usize {
    base + r.u16(at) as usize
}

impl<'a> Reader<'a> {
    const fn new(data: &'a [u8]) -> Reader<'a> {
        Reader(data)
    }
    #[inline]
    fn u16(&self, at: usize) -> u16 {
        self.0.get(at..at + 2).map_or(0, |b| u16::from_be_bytes([b[0], b[1]]))
    }
    #[inline]
    fn i16(&self, at: usize) -> i16 {
        self.u16(at) as i16
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Append one 16-bit field, in the order the specification writes them.
    fn put(t: &mut Vec<u8>, v: u16) {
        t.extend_from_slice(&v.to_be_bytes());
    }

    /// The constants are not read by these tests, but a table whose declared constant
    /// block would run past its own end is refused at parse time -- so it is here.
    const CONSTANTS: usize = 10;
    const VARIANTS: usize = 234;

    /// A `MATH` table for two growable glyphs: one with three ready-made variants and
    /// no assembly, one with a single short variant and a bottom/extender/top assembly.
    ///
    /// Handwritten rather than lifted from a system font because the whole point is the
    /// offsets. A real font cannot tell a reader bug from an absent feature, which is
    /// exactly the confusion that let every delimiter in the reader draw at its natural
    /// size while the face it was read from had tall ones.
    fn two_glyphs() -> Vec<u8> {
        let mut t = Vec::new();
        put(&mut t, 1); // major version
        put(&mut t, 0); // minor version
        put(&mut t, CONSTANTS as u16);
        put(&mut t, 0); // no MathGlyphInfo, so no italics corrections either
        put(&mut t, VARIANTS as u16);
        t.resize(VARIANTS, 0);
        // ---- GlyphVariation, whose every offset is measured from here.
        put(&mut t, 40); // MinConnectorOverlap
        put(&mut t, 20); // VertCoverage -> 254
        put(&mut t, 0); // no horizontal direction in this fixture
        put(&mut t, 2); // VertGlyphCount
        put(&mut t, 0); // HorzGlyphCount
        put(&mut t, 28); // glyph 100's construction -> 262
        put(&mut t, 44); // glyph 200's construction -> 278
        t.resize(254, 0);
        // ---- Coverage format 1: a plain list of the two glyphs.
        put(&mut t, 1);
        put(&mut t, 2);
        put(&mut t, 100);
        put(&mut t, 200);
        // ---- Glyph 100: three heights, and nothing to build with.
        put(&mut t, 0); // no GlyphAssembly
        put(&mut t, 3);
        put(&mut t, 101); put(&mut t, 500);
        put(&mut t, 102); put(&mut t, 900);
        put(&mut t, 103); put(&mut t, 1400);
        // ---- Glyph 200: one short height and an assembly of three parts.
        put(&mut t, 8); // GlyphAssembly -> 286
        put(&mut t, 1);
        put(&mut t, 201); put(&mut t, 300);
        put(&mut t, 0); // the assembly's italics correction
        put(&mut t, 3); // partCount
        for (glyph, start, end, advance, extender) in
            [(210u16, 0u16, 40, 300, 0), (211, 40, 40, 200, 1), (212, 40, 0, 300, 0)]
        {
            put(&mut t, glyph);
            put(&mut t, start);
            put(&mut t, end);
            put(&mut t, advance);
            put(&mut t, extender);
        }
        t
    }

    #[test]
    fn a_glyph_that_can_grow_is_found_by_its_own_number() {
        let bytes = two_glyphs();
        let t = MathTable::parse(&bytes, 2048).expect("the fixture is a version 1 table");
        let c = t.construction(100, true).expect("coverage lists glyph 100");
        assert_eq!(c.variants.len(), 3, "the variant records were not walked");
        assert!(c.assembly.is_empty(), "this one has no assembly to find");
        // Between the two covered glyphs, and above both: absent, which is the answer a
        // caller can fall back on -- but only if it is not also the answer for glyphs
        // the font *does* list.
        assert!(t.construction(150, true).is_none());
        assert!(t.construction(300, true).is_none());
        assert!(t.construction(200, false).is_none(), "the horizontal coverage is empty");
    }

    #[test]
    fn a_variant_already_tall_enough_is_used_rather_than_built() {
        let bytes = two_glyphs();
        let t = MathTable::parse(&bytes, 2048).unwrap();
        let grown = t.grow(100, 800).expect("three variants to choose between");
        assert_eq!(grown.len(), 1, "a ready-made height is one glyph");
        // 900 is the variant's own measurement, not the 800 that was asked for: the
        // caller places the part by the band it actually occupies, so reporting the
        // target instead would leave the shape floating inside a space it does not fill.
        assert_eq!((grown[0].glyph, grown[0].full_advance), (102, 900));
        assert_eq!(grown[0].offset, 0);
    }

    #[test]
    fn the_tallest_height_a_font_has_beats_its_smallest() {
        let bytes = two_glyphs();
        let t = MathTable::parse(&bytes, 2048).unwrap();
        // Past every height the font offers, and with no assembly to build from: the
        // answer is still 1400 units of brace rather than the 500-unit natural glyph,
        // which is the difference between a parenthesis that reads as enclosing the
        // grid above it and one that sits on the baseline inside it.
        let grown = t.grow(100, 5000).expect("the tallest variant stands in");
        assert_eq!((grown[0].glyph, grown[0].full_advance), (103, 1400));
    }

    #[test]
    fn an_assembly_is_built_out_to_the_height_asked() {
        let bytes = two_glyphs();
        let t = MathTable::parse(&bytes, 2048).unwrap();
        let grown = t.grow(200, 1000).expect("three parts to build with");
        assert_eq!(grown.first().map(|p| p.glyph), Some(210), "the bottom part goes first");
        assert_eq!(grown.last().map(|p| p.glyph), Some(212), "and the top part last");
        assert!(grown.len() > 3, "the extender was never repeated: {grown:?}");
        assert!(
            grown.windows(2).all(|w| w[1].offset > w[0].offset),
            "parts must climb: {grown:?}"
        );
        let last = grown.last().unwrap();
        let reach = u32::from(last.offset) + u32::from(last.full_advance);
        assert!(reach >= 1000, "assembled only {reach} units for a target of 1000");
    }

    /// The same kind of constructions, reached through a coverage written as ranges --
    /// the format that carries a starting index per range rather than one glyph per
    /// entry. Four constructions, three glyphs in the first range and one in the second.
    fn ranged() -> Vec<u8> {
        let mut t = Vec::new();
        put(&mut t, 1);
        put(&mut t, 0);
        put(&mut t, CONSTANTS as u16);
        put(&mut t, 0);
        put(&mut t, VARIANTS as u16);
        t.resize(VARIANTS, 0);
        put(&mut t, 40);
        put(&mut t, 18); // VertCoverage -> 252
        put(&mut t, 0);
        put(&mut t, 4); // VertGlyphCount
        put(&mut t, 0);
        for i in 0..4u16 {
            put(&mut t, 34 + i * 8); // -> 268, 276, 284, 292
        }
        // ---- Coverage format 2: 100..=102 at index 0, then 300..=302 at index 3.
        put(&mut t, 2);
        put(&mut t, 2);
        put(&mut t, 100); put(&mut t, 102); put(&mut t, 0);
        put(&mut t, 300); put(&mut t, 302); put(&mut t, 3);
        for glyph in [111u16, 112, 113, 311] {
            put(&mut t, 0); // no assembly
            put(&mut t, 1);
            put(&mut t, glyph);
            put(&mut t, 700);
        }
        t
    }

    #[test]
    fn a_range_in_a_coverage_points_at_its_own_index() {
        let bytes = ranged();
        let t = MathTable::parse(&bytes, 2048).unwrap();
        assert_eq!(t.grow(100, 100).map(|g| g[0].glyph), Some(111));
        assert_eq!(t.grow(102, 100).map(|g| g[0].glyph), Some(113));
        // Each range says where its own indices start, so glyph 300 is the fourth
        // construction -- and summing one per range before it instead lands on the
        // second, which is a different delimiter's height on the page.
        assert_eq!(t.grow(300, 100).map(|g| g[0].glyph), Some(311));
        assert_eq!(t.grow(302, 100).map(|g| g[0].glyph), None, "past the listed ranges");
    }
}
