//! Formula layout driven by the font's own `MATH` table.
//!
//! The shape of this module is one-way: a [`Node`] tree goes in, a [`Formula`] of
//! positioned shapes comes out, and nothing in between knows about Windows. The
//! numbers that place every piece come from the face in use through [`MathMeasure`],
//! which is the point of the pillar -- a fraction bar at the font's axis height and a
//! subscript dropped by the font's own offset is what makes a formula look like it was
//! set by the type designer rather than by an approximation of someone else's.
//!
//! Two conventions run through the file:
//!
//! - Coordinates are offsets from the formula's own baseline, with **y positive
//!   downwards**, matching how the painter and the text layout core both think. A
//!   superscript therefore has a negative baseline, and a [`Shape::Rule`]'s `y` is its
//!   top edge.
//! - Every sub-layout returns shapes in local coordinates and the caller translates
//!   them. Threading absolute positions downward instead produces an offset bug in
//!   every construct at once.
//!
//! The specification supplies the constants but deliberately leaves the algorithms to
//! the engine, so each placement below is derived from the documented *meaning* of the
//! constant and the resulting inequality is written beside it. Where a face has no
//! `MATH` table [`MathMeasure::constant`] returns `None` and a ratio of the type size
//! stands in, so a formula still sets acceptably in a font like Consolas instead of
//! collapsing.

use crate::parse::{AccentKind, ArrayKind, BarSide, ColAlign, FracStyle, Limits, Node};
use crate::table::constant;

pub type Pt = f32;

/// Vertical extents and the trailing italic correction of a measured run.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Extents {
    pub advance: Pt,
    pub ascent: Pt,
    pub descent: Pt,
    /// How far the top of a slanted run leans past its advance width.
    pub italic: Pt,
}

impl Extents {
    pub fn height(&self) -> Pt {
        self.ascent + self.descent
    }
}

/// One glyph of a stretched shape, positioned inside it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stacked {
    /// Index into the face the caller will draw with.
    pub index: u16,
    /// Distance from the bottom edge of the assembled shape up to this part's
    /// baseline, which is the part's own descent plus its offset in the assembly.
    pub baseline_up: Pt,
    pub width: Pt,
}

/// One glyph of a shape grown *sideways*, as the wide accents are.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Running {
    pub index: u16,
    /// Distance from the left edge of the assembled shape to this part's left edge.
    pub x: Pt,
    pub width: Pt,
}

/// A piece of a laid-out formula, positioned relative to the formula's origin.
#[derive(Clone, Debug, PartialEq)]
pub enum Shape {
    /// `text` set at `size`, its left edge at `x` and its baseline at `y`.
    Run { text: String, x: Pt, y: Pt, size: Pt },
    /// A bare glyph index, for the parts a stretched delimiter or radical is made of.
    Glyph { index: u16, x: Pt, y: Pt, size: Pt },
    /// A filled rectangle: fraction bars, over- and underbars, the radical's rule.
    Rule { x: Pt, y: Pt, width: Pt, thickness: Pt },
}

/// A laid-out formula. Its width, ascent and descent are what an inline formula
/// contributes to the line of text it sits on.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Formula {
    pub shapes: Vec<Shape>,
    pub width: Pt,
    pub ascent: Pt,
    pub descent: Pt,
}

impl Formula {
    pub fn height(&self) -> Pt {
        self.ascent + self.descent
    }
}

/// Everything the layout needs from the platform: glyph metrics and `MATH` numbers.
pub trait MathMeasure {
    /// Measure `text` at `size`, including its trailing italic correction.
    fn measure(&mut self, text: &str, size: Pt) -> Extents;

    /// A length constant converted to points at `size`, or `None` if the face has no
    /// `MATH` table for it.
    fn constant(&mut self, index: usize, size: Pt) -> Option<Pt>;

    /// A percentage constant -- script sizes, the radical degree's raise -- as a
    /// ratio, so `80` comes back as `0.8`.
    fn percent(&mut self, index: usize, fallback: Pt) -> Pt;

    /// `ch` grown to `height`, as the glyphs to draw from bottom to top, or `None`
    /// when the face cannot stretch it.
    fn stretch(&mut self, ch: char, size: Pt, height: Pt) -> Option<Vec<Stacked>>;

    /// `ch` grown to `width`, as the glyphs to draw from left to right, or `None` when
    /// the face has no wider drawing of it. The other direction of the same table:
    /// where a bracket keeps its taller heights, a hat keeps its wider ones.
    fn widen(&mut self, ch: char, size: Pt, width: Pt) -> Option<Vec<Running>>;
}

/// The style a piece is set in: its size, and whether limits stack or ride as
/// scripts. Cramped is the rule that material already inside a dependent position may
/// not reach as high as free material; the constants name it, so the flag has to
/// exist.
#[derive(Clone, Copy, Debug)]
struct Style {
    size: Pt,
    display: bool,
    cramped: bool,
}

/// Ratios used when the face reports nothing. The two script percentages are the
/// values the specification suggests; the rest are the em fractions the suggested
/// values work out to for a typical text face.
const FALLBACK_AXIS: Pt = 0.25;
const FALLBACK_RULE: Pt = 0.06;
const FALLBACK_SUP_SHIFT: Pt = 0.42;
const FALLBACK_SUB_SHIFT: Pt = 0.28;
const FALLBACK_SCRIPT_PERCENT: Pt = 0.8;
const FALLBACK_SCRIPT_SCRIPT_PERCENT: Pt = 0.6;

/// TeX's muskips as fractions of an em; 18 mu make an em, and the defaults are
/// `\thinmuskip` 3 mu, `\medmuskip` 4 mu and `\thickmuskip` 5 mu.
const THIN_MU: Pt = 3.0 / 18.0;
const MED_MU: Pt = 4.0 / 18.0;
const THICK_MU: Pt = 5.0 / 18.0;
const PUNCT_MU: Pt = 2.0 / 18.0;

/// Room between the columns of a grid, as a ratio of the type size: `MATH` says
/// nothing about arrays, and TeX's `\arraycolsep` -- 5 mu on each side of the rule
/// between two columns, which is what one column gap therefore is -- is the number
/// the shape of a matrix is read from.
const ARRAY_COL_GAP: Pt = THICK_MU;
/// A `cases` condition stands further from its value than two matrix columns do,
/// because it is read as a separate clause rather than as more data.
const CASES_COND_GAP: Pt = 8.0 / 18.0;
/// Row separation when a face reports nothing for `mathLeading`, which is most of
/// them: a ratio of the type size, floored by three rule thicknesses wherever the
/// table is present enough to give those.
const FALLBACK_ARRAY_LEADING: Pt = 0.2;

/// TeX's `\fboxsep` and `\fboxrule` for `\boxed`: 3pt of separation and a 0.4pt rule
/// at a 10pt base, which is what both of them are as ratios of the type size here.
/// `MATH` has nothing to say about a box -- it is not a construct the table describes,
/// and the frame is deliberately thinner than a fraction bar because it is a frame and
/// not an operator.
const BOX_SEPARATION: Pt = 0.3;
const BOX_RULE: Pt = 0.04;

/// A metrics-only box: what a sub-layout reports to its caller.
#[derive(Clone, Copy, Debug, Default)]
struct Mb {
    width: Pt,
    ascent: Pt,
    descent: Pt,
    italic: Pt,
}

impl Mb {
    fn of(e: &Extents) -> Mb {
        Mb { width: e.advance, ascent: e.ascent, descent: e.descent, italic: e.italic }
    }
    fn height(&self) -> Pt {
        self.ascent + self.descent
    }
    /// Width including the ink that leans past it, which is what a bar must cover.
    fn ink_width(&self) -> Pt {
        self.width + self.italic
    }
}

/// Lay `node` out at `size`, as a display or an inline formula.
pub fn layout(node: &Node, size: Pt, display: bool, m: &mut dyn MathMeasure) -> Formula {
    let mut e = Engine { m };
    let (shapes, b) = e.lay(node, Style { size, display, cramped: false });
    Formula { shapes, width: b.width, ascent: b.ascent, descent: b.descent }
}

struct Engine<'a> {
    m: &'a mut dyn MathMeasure,
}

impl Engine<'_> {
    fn ext(&mut self, text: &str, size: Pt) -> Extents {
        self.m.measure(text, size)
    }

    /// A length constant in points, or `fallback * size` when the face does not give
    /// one. `fallback` is therefore an em ratio throughout this module.
    fn c(&mut self, index: usize, size: Pt, fallback: Pt) -> Pt {
        self.m.constant(index, size).unwrap_or(size * fallback)
    }

    /// A rule of `n` default rule thicknesses, the unit the spec suggests gaps in.
    fn rules(&mut self, n: Pt, size: Pt) -> Pt {
        self.c(constant::FRACTION_RULE_THICKNESS, size, FALLBACK_RULE) * n
    }

    /// Script sizes are composed, so a second-level script is always smaller than the
    /// first no matter what the two percentages happen to be.
    fn script_size(&mut self, st: Style, level: u8) -> Pt {
        let one = self.m.percent(constant::SCRIPT_PERCENT_SCALE_DOWN, FALLBACK_SCRIPT_PERCENT);
        if level <= 1 {
            return st.size * one;
        }
        let two =
            self.m.percent(constant::SCRIPT_SCRIPT_PERCENT_SCALE_DOWN, FALLBACK_SCRIPT_SCRIPT_PERCENT);
        st.size * one * two
    }

    fn lay(&mut self, n: &Node, st: Style) -> (Vec<Shape>, Mb) {
        match n {
            Node::Atom(s) => self.atom(s, st),
            Node::Row(v) => self.row(v, st),
            Node::Frac { num, den, has_bar, style } =>
                self.frac(num, den, *has_bar, *style, st),
            Node::Sup { base, sup } => self.scripts(base, None, Some(sup), st),
            Node::Sub { base, sub } => self.scripts(base, Some(sub), None, st),
            Node::SubSup { base, sub, sup } => self.scripts(base, Some(sub), Some(sup), st),
            Node::Sqrt { body, degree } => self.radical(body, degree.as_deref(), st),
            Node::Fence { left, right, body } => self.fence(*left, *right, body, st),
            Node::BigOp { op, limits, sub, sup } =>
                self.big_op(op, *limits, sub.as_deref(), sup.as_deref(), st),
            Node::Accent { base, accent, wide } => self.accent(base, *accent, *wide, st),
            Node::Bar { body, side } => self.bar(body, *side, st),
            Node::Stack { base, label, side } => self.stack(base, label, *side, st),
            Node::Boxed { body } => self.boxed(body, st),
            Node::Array { rows, columns, kind, delimiters, rules } => self.array(
                rows,
                columns,
                *kind,
                *delimiters,
                rules,
                st,
            ),
            Node::Space(mu) => (
                Vec::new(),
                Mb { width: *mu as Pt / 18.0 * st.size, ..Default::default() },
            ),
        }
    }

    fn atom(&mut self, text: &str, st: Style) -> (Vec<Shape>, Mb) {
        if text.is_empty() {
            return (Vec::new(), Mb::default());
        }
        let e = self.ext(text, st.size);
        (
            vec![Shape::Run { text: text.to_string(), x: 0.0, y: 0.0, size: st.size }],
            Mb::of(&e),
        )
    }

    fn row(&mut self, nodes: &[Node], st: Style) -> (Vec<Shape>, Mb) {
        let mut out: Vec<Shape> = Vec::new();
        let mut b = Mb::default();
        let mut prev: Option<Class> = None;
        for n in nodes {
            let cls = Class::of(n);
            if let Some(p) = prev {
                b.width += glue(p, cls, st.size);
            }
            // Italic correction belongs after the ink it describes, so the next atom
            // clears a slanted letter's leaning top.
            b.width += std::mem::take(&mut b.italic);
            let (s, cb) = self.lay(n, st);
            translate(&s, b.width, 0.0, &mut out);
            b.width += cb.width;
            b.ascent = b.ascent.max(cb.ascent);
            b.descent = b.descent.max(cb.descent);
            b.italic = cb.italic;
            prev = Some(cls);
        }
        (out, b)
    }

    /// Fractions and a big operator's limit stack are the same arrangement: two boxes
    /// about the axis with a minimum ink gap between them.
    fn frac(
        &mut self,
        num: &Node,
        den: &Node,
        has_bar: bool,
        forced: FracStyle,
        st: Style,
    ) -> (Vec<Shape>, Mb) {
        // `\tfrac` is the whole construct one script step down, the way `smallmatrix`
        // is; `\dfrac` keeps its size and only asks for a displayed formula's
        // proportions. Everything below reads `st`, so restating the style here is all
        // a forced one changes.
        let mut base = st;
        if forced == FracStyle::Text {
            base.size = self.script_size(st, 1);
        }
        let st = Style {
            display: match forced {
                FracStyle::Auto => st.display,
                FracStyle::Display => true,
                FracStyle::Text => false,
            },
            ..base
        };
        // Both halves lose the display style and are cramped: a numerator is set in
        // text style even inside a display formula.
        let inner = Style { display: false, cramped: true, ..st };
        let (snum, n_b) = self.lay(num, inner);
        let (sden, d_b) = self.lay(den, inner);

        let axis = self.c(constant::AXIS_HEIGHT, st.size, FALLBACK_AXIS);
        let rule = if has_bar {
            self.c(constant::FRACTION_RULE_THICKNESS, st.size, FALLBACK_RULE)
        } else {
            0.0
        };
        // `\binom` has no rule, so its clearances come from the plain stack constants.
        let (up, down, gap, denom_gap) = if has_bar {
            (
                self.c(
                    if st.display {
                        constant::FRACTION_NUMERATOR_DISPLAY_STYLE_SHIFT_UP
                    } else {
                        constant::FRACTION_NUMERATOR_SHIFT_UP
                    },
                    st.size,
                    FALLBACK_SUP_SHIFT,
                ),
                self.c(
                    if st.display {
                        constant::FRACTION_DENOMINATOR_DISPLAY_STYLE_SHIFT_DOWN
                    } else {
                        constant::FRACTION_DENOMINATOR_SHIFT_DOWN
                    },
                    st.size,
                    FALLBACK_SUB_SHIFT,
                ),
                self.c(
                    if st.display {
                        constant::FRACTION_NUM_DISPLAY_STYLE_GAP_MIN
                    } else {
                        constant::FRACTION_NUMERATOR_GAP_MIN
                    },
                    st.size,
                    FALLBACK_RULE * 2.0,
                ),
                self.c(
                    if st.display {
                        constant::FRACTION_DENOM_DISPLAY_STYLE_GAP_MIN
                    } else {
                        constant::FRACTION_DENOMINATOR_GAP_MIN
                    },
                    st.size,
                    FALLBACK_RULE * 2.0,
                ),
            )
        } else {
            // Without a rule there is one gap between the two boxes, not two, so the
            // same value serves for both halves.
            let gap = self.c(
                if st.display {
                    constant::STACK_DISPLAY_STYLE_GAP_MIN
                } else {
                    constant::STACK_GAP_MIN
                },
                st.size,
                FALLBACK_RULE * 3.0,
            );
            (
                self.c(
                    if st.display {
                        constant::STACK_TOP_DISPLAY_STYLE_SHIFT_UP
                    } else {
                        constant::STACK_TOP_SHIFT_UP
                    },
                    st.size,
                    FALLBACK_SUP_SHIFT,
                ),
                self.c(
                    if st.display {
                        constant::STACK_BOTTOM_DISPLAY_STYLE_SHIFT_DOWN
                    } else {
                        constant::STACK_BOTTOM_SHIFT_DOWN
                    },
                    st.size,
                    FALLBACK_SUB_SHIFT,
                ),
                gap,
                gap,
            )
        };
        let gap_d = if has_bar { denom_gap } else { gap };
        // The rule is centred on the axis, so it spans [-axis - t/2, -axis + t/2].
        let bar_top = -axis - rule / 2.0;
        let bar_bottom = -axis + rule / 2.0;
        // Numerator ink bottom (-u + n.descent) has to stay `gap` above the bar's
        // top, i.e. at least as far above it as the bar is above the baseline.
        let u = up.max(-bar_top + gap + n_b.descent);
        // Denominator ink top (d - den.ascent) must stay below the bar's bottom.
        let d = down.max(bar_bottom + gap_d + d_b.ascent);

        let width = n_b.ink_width().max(d_b.ink_width());
        let mut out = Vec::new();
        translate(&snum, (width - n_b.ink_width()) / 2.0, -u, &mut out);
        translate(&sden, (width - d_b.ink_width()) / 2.0, d, &mut out);
        if has_bar {
            out.push(Shape::Rule { x: 0.0, y: bar_top, width, thickness: rule });
        }
        (out, Mb { width, ascent: u + n_b.ascent, descent: d + d_b.descent, italic: 0.0 })
    }

    /// A grid of cells: each column as wide as its widest cell, each row far enough
    /// below the row above for both rows' own ink plus a clearance, and the whole block
    /// centred on the axis the way a fraction's stack is.
    fn array(
        &mut self,
        rows: &[Vec<Node>],
        columns: &[ColAlign],
        kind: ArrayKind,
        delimiters: Option<(char, char)>,
        rules: &[bool],
        st: Style,
    ) -> (Vec<Shape>, Mb) {
        // Cells are dependent material, so text style and cramped exactly as a
        // fraction's halves are. `smallmatrix` is the one that is also a script size.
        let mut inner = Style { display: false, cramped: st.cramped, ..st };
        if kind == ArrayKind::SmallMatrix {
            inner.size = self.script_size(st, 1);
        }
        let cols = columns.len().max(1);
        let mut laid: Vec<Vec<(Vec<Shape>, Mb)>> = Vec::with_capacity(rows.len());
        for row in rows {
            let mut cells = Vec::with_capacity(row.len());
            for cell in row {
                cells.push(self.lay(cell, inner));
            }
            laid.push(cells);
        }

        // Column advance: the widest cell in the column, its leaning ink included, so
        // that a slanted letter's top cannot overprint the column beside it.
        let mut widths: Vec<Pt> = vec![0.0; cols];
        for row in &laid {
            for (j, (_, cb)) in row.iter().enumerate() {
                let j = j.min(cols - 1);
                widths[j] = widths[j].max(cb.ink_width());
            }
        }
        // Column origins. The gap that would follow the last column is not part of the
        // box, which is why `x` is advanced by it only between columns.
        let mut lefts: Vec<Pt> = vec![0.0; cols];
        let mut x: Pt = 0.0;
        for j in 0..cols {
            lefts[j] = x;
            x += widths[j];
            if j + 1 < cols {
                x += column_gap(kind, j, inner.size);
            }
        }
        let grid_w = x;

        // Each row's ink extent is the tallest and the deepest of its cells, and the
        // rows are separated by those extents rather than by a fixed leading, so a
        // fraction or a stretched delimiter in one row pushes its neighbours apart.
        let mut rises: Vec<Pt> = Vec::with_capacity(laid.len());
        let mut falls: Vec<Pt> = Vec::with_capacity(laid.len());
        for row in &laid {
            let (mut a, mut d): (Pt, Pt) = (0.0, 0.0);
            for (_, cb) in row {
                a = a.max(cb.ascent);
                d = d.max(cb.descent);
            }
            rises.push(a);
            falls.push(d);
        }
        // `mathLeading` is the one MATH number about line spacing and most math faces
        // leave it at zero, so the clearance is the larger of a ratio of the type size
        // and three default rule thicknesses.
        let leading = self.m.percent(constant::MATH_LEADING, FALLBACK_ARRAY_LEADING);
        let gap = self.rules(3.0, inner.size).max(leading * inner.size);
        let n_rows = laid.len() as Pt;
        let stack: Pt = rises.iter().zip(&falls).map(|(a, d)| a + d).sum();
        let height = stack + gap * (n_rows - 1.0).max(0.0);
        let axis = self.c(constant::AXIS_HEIGHT, st.size, FALLBACK_AXIS);
        // The block spans `height` centred on the axis, so its ink top is at
        // `-axis - height/2` and each row's baseline follows from the heights stacked
        // above it -- the same inequality a numerator's shift is derived from.
        let mut baselines = Vec::with_capacity(laid.len());
        let mut top = -axis - height / 2.0;
        for i in 0..laid.len() {
            baselines.push(top + rises[i]);
            top += rises[i] + falls[i] + gap;
        }

        let mut out = Vec::new();
        let mut b = Mb::default();
        for (i, row) in laid.iter().enumerate() {
            for (j, (s, cb)) in row.iter().enumerate() {
                let j = j.min(cols - 1);
                let a = columns.get(j).copied().unwrap_or(ColAlign::Center);
                let dx = lefts[j] + align_in(a, widths[j], cb.ink_width());
                translate(s, dx, baselines[i], &mut out);
            }
            b.ascent = b.ascent.max(rises[i] - baselines[i]);
            b.descent = b.descent.max(falls[i] + baselines[i]);
        }
        b.width = grid_w;
        // `\hline`: a rule across the grid at the boundary it was written at, in the
        // middle of the clearance the rows already keep apart -- the gap is at least
        // three rule thicknesses, so a rule never touches the ink above or under it,
        // and the grid does not have to grow for one.
        let t = self.rules(1.0, inner.size);
        for i in rules.iter().enumerate().filter(|(_, r)| **r).map(|(i, _)| i) {
            let line = if i == 0 {
                baselines.first().map_or(0.0, |b0| b0 - rises[0] - gap / 2.0)
            } else if i >= laid.len() {
                let last = laid.len() - 1;
                baselines[last] + falls[last] + gap / 2.0
            } else {
                (baselines[i] - rises[i] + baselines[i - 1] + falls[i - 1]) / 2.0
            };
            out.push(Shape::Rule { x: 0.0, y: line - t / 2.0, width: grid_w, thickness: t });
            if i == 0 {
                b.ascent = b.ascent.max(t / 2.0 - line);
            } else if i >= laid.len() {
                b.descent = b.descent.max(line + t / 2.0);
            }
        }
        match delimiters {
            Some((left, right)) => self.fenced(left, right, &out, b, st),
            None => (out, b),
        }
    }

    /// Sub- and superscript placement, one inequality per `MATH` constant.
    fn scripts(
        &mut self,
        base: &Node,
        sub: Option<&Node>,
        sup: Option<&Node>,
        st: Style,
    ) -> (Vec<Shape>, Mb) {
        let (sbase, bb) = self.lay(base, st);
        let level1 = Style { size: self.script_size(st, 1), display: false, cramped: true };
        let sup_box = sup.map(|n| self.lay(n, level1));
        let sub_box = sub.map(|n| self.lay(n, level1));
        if sup_box.is_none() && sub_box.is_none() {
            let mut out = Vec::new();
            translate(&sbase, 0.0, 0.0, &mut out);
            return (out, Mb { italic: bb.italic, ..bb });
        }

        // The base's descender is what the subscript's drop is measured against, and
        // the base's ink top what the superscript's drop is measured against; the
        // "bottom" constants are heights above the parent baseline, so they bound the
        // script's own ink.
        let shift_up = self.c(
            if st.cramped {
                constant::SUPERSCRIPT_SHIFT_UP_CRAMPED
            } else {
                constant::SUPERSCRIPT_SHIFT_UP
            },
            st.size,
            FALLBACK_SUP_SHIFT,
        );
        let drop_max = self.c(constant::SUPERSCRIPT_BASELINE_DROP_MAX, st.size, 0.30);
        let bottom_min = if sub_box.is_some() {
            self.c(constant::SUPERSCRIPT_BOTTOM_MAX_WITH_SUBSCRIPT, st.size, 0.32)
        } else {
            self.c(constant::SUPERSCRIPT_BOTTOM_MIN, st.size, 0.11)
        };
        let shift_down = self.c(constant::SUBSCRIPT_SHIFT_DOWN, st.size, FALLBACK_SUB_SHIFT);
        let top_max = self.c(constant::SUBSCRIPT_TOP_MAX, st.size, 0.32);
        let base_drop_min = self.c(constant::SUBSCRIPT_BASELINE_DROP_MIN, st.size, 0.02);

        let mut u = 0.0;
        if let Some((_, sb)) = sup_box.as_ref() {
            // u - sup.descent >= bottom_min, and u >= base.ascent - drop_max: a tall
            // base such as a parenthesised fraction drags its exponent up with it.
            u = shift_up.max(bb.ascent - drop_max).max(bottom_min + sb.descent);
        }
        let mut d = 0.0;
        if let Some((_, sb)) = sub_box.as_ref() {
            d = shift_down.max(sb.ascent - top_max).max(bb.descent + base_drop_min);
            if let Some((_, pb)) = sup_box.as_ref() {
                // Ink gap between the two: d + u - (sub.ascent + sup.descent) >= min.
                let gap = self.c(constant::SUB_SUPERSCRIPT_GAP_MIN, st.size, 0.0);
                let gap = if gap > 0.0 {
                    gap
                } else {
                    self.rules(4.0, st.size)
                };
                d = d.max(gap + sb.ascent + pb.descent - u);
            }
        }

        // A subscript starts immediately after the base's advance; a superscript after
        // its italic correction, because the leaning top of an italic letter is where
        // an exponent has to clear.
        let sub_x = bb.width;
        let sup_x = bb.ink_width();
        let mut out = Vec::new();
        translate(&sbase, 0.0, 0.0, &mut out);
        let mut width = bb.width;
        let mut ascent = bb.ascent;
        let mut descent = bb.descent;
        if let Some((s, sb)) = sup_box {
            translate(&s, sup_x, -u, &mut out);
            width = width.max(sup_x + sb.ink_width());
            ascent = ascent.max(u + sb.ascent);
        }
        if let Some((s, sb)) = sub_box {
            translate(&s, sub_x, d, &mut out);
            width = width.max(sub_x + sb.ink_width());
            descent = descent.max(d + sb.descent);
        }
        // `spaceAfterScript` is the side bearing a script owes whatever follows, so
        // the next atom in the row cannot crowd the subscript's descender.
        width += self.c(constant::SPACE_AFTER_SCRIPT, st.size, 1.0 / 24.0);
        (out, Mb { width, ascent, descent, italic: 0.0 })
    }

    /// A radical: the sign stretched to the body, its horizontal bar drawn as a rule
    /// because the font's assembly only grows in one direction.
    fn radical(&mut self, body: &Node, degree: Option<&Node>, st: Style) -> (Vec<Shape>, Mb) {
        // Under a radical the material is cramped: it may not reach as high as free
        // material, which is what keeps `\sqrt{x^2}` tight.
        let (sbody, bb) = self.lay(body, Style { cramped: true, ..st });
        let rule = self.c(constant::RADICAL_RULE_THICKNESS, st.size, FALLBACK_RULE);
        let gap = self.c(
            if st.display {
                constant::RADICAL_DISPLAY_STYLE_VERTICAL_GAP
            } else {
                constant::RADICAL_VERTICAL_GAP
            },
            st.size,
            FALLBACK_RULE * 1.25,
        );
        let extra = self.c(constant::RADICAL_EXTRA_ASCENDER, st.size, FALLBACK_RULE);

        // The degree, if any, sits outside the hook: `kernBeforeDegree` in front of
        // it, the negative `kernAfterDegree` pulling the sign back over it.
        let level2 = Style { size: self.script_size(st, 2), display: false, cramped: true };
        let deg = degree.map(|d| self.lay(d, level2));
        let (before, after) = if deg.is_some() {
            (
                self.c(constant::RADICAL_KERN_BEFORE_DEGREE, st.size, 5.0 / 18.0),
                self.c(constant::RADICAL_KERN_AFTER_DEGREE, st.size, -10.0 / 18.0),
            )
        } else {
            (0.0, 0.0)
        };
        let sign_x = match &deg {
            Some((_, db)) => (before + db.ink_width() + after).max(0.0),
            None => 0.0,
        };

        // The bar sits `gap` above the body's ink and is `rule` thick, so the sign has
        // to cover from the body's bottom to the top of the bar.
        let bar_top = -(bb.ascent + gap + rule);
        let sign_h = bb.ascent + gap + rule + bb.descent;
        let mut out = Vec::new();
        let mut ascent = bb.ascent + gap + rule + extra;
        let descent = bb.descent;
        let mut sign_w: Pt = 0.0;

        let natural = self.ext("\u{221a}", st.size);
        match self.m.stretch('\u{221a}', st.size, sign_h) {
            Some(parts) => {
                for p in &parts {
                    sign_w = sign_w.max(p.width);
                    out.push(Shape::Glyph {
                        index: p.index,
                        x: sign_x,
                        // A part's baseline is its offset up from the assembly's bottom
                        // edge, which is the deepest ink the whole thing needs.
                        y: descent - p.baseline_up,
                        size: st.size,
                    });
                }
            }
            None => {
                // Nothing stretchable: the design-size sign, which is right for a
                // short body and merely tight for a tall one.
                out.push(Shape::Run {
                    text: "\u{221a}".into(),
                    x: sign_x,
                    y: 0.0,
                    size: st.size,
                });
                sign_w = natural.advance;
                ascent = ascent.max(natural.ascent);
            }
        }

        let body_x = sign_x + sign_w;
        translate(&sbody, body_x, 0.0, &mut out);
        out.push(Shape::Rule { x: body_x, y: bar_top, width: bb.ink_width(), thickness: rule });
        let width = body_x + bb.ink_width();

        if let Some((sd, db)) = deg {
            // A percentage of the radical sign's own height.
            let pct = self.m.percent(constant::RADICAL_DEGREE_BOTTOM_RAISE_PERCENT, 0.6);
            translate(&sd, before, -sign_h * pct - db.descent, &mut out);
        }
        (out, Mb { width, ascent, descent, italic: 0.0 })
    }

    /// `\left( ... \right)`: the body first, then delimiters grown to its height.
    fn fence(&mut self, left: char, right: char, body: &Node, st: Style) -> (Vec<Shape>, Mb) {
        let (sbody, bb) = self.lay(body, st);
        self.fenced(left, right, &sbody, bb, st)
    }

    /// The delimiters around a body that is already laid out. Shared by `\left ...
    /// \right` and by the environments that bring their own, so a `pmatrix` and a
    /// parenthesised fraction are grown by one arithmetic and cannot drift apart.
    fn fenced(
        &mut self,
        left: char,
        right: char,
        sbody: &[Shape],
        bb: Mb,
        st: Style,
    ) -> (Vec<Shape>, Mb) {
        let axis = self.c(constant::AXIS_HEIGHT, st.size, FALLBACK_AXIS);
        let min = self.c(constant::DELIMITED_SUB_FORMULA_MIN_HEIGHT, st.size, 0.0);
        // Under the threshold the delimiters are ordinary glyphs; over it the pair is
        // a sub-formula and grows around the axis, which is why a bracketed fraction
        // looks centred rather than sitting on the baseline.
        let tall = min > 0.0 && bb.height() >= min;
        let (up, down) = if tall {
            (bb.height() / 2.0 + axis, bb.height() / 2.0 - axis)
        } else {
            (bb.ascent, bb.descent)
        };

        let mut out = Vec::new();
        let mut b = Mb { width: 0.0, ascent: up, descent: down, italic: 0.0 };
        if left != '\0' {
            let (w, a, d) = self.delimiter(left, st.size, up, down, 0.0, &mut out);
            b.width += w;
            // A delimiter taller than its body is the body's own business: the line
            // has to make room for the ink that is actually there.
            b.ascent = b.ascent.max(a);
            b.descent = b.descent.max(d);
        }
        let body_x = b.width;
        translate(sbody, body_x, 0.0, &mut out);
        b.width += bb.ink_width();
        if right != '\0' {
            let (w, a, d) = self.delimiter(right, st.size, up, down, b.width, &mut out);
            b.width += w;
            b.ascent = b.ascent.max(a);
            b.descent = b.descent.max(d);
        }
        (out, b)
    }

    /// Draw one delimiter at `x`, reaching `up` above and `down` below the baseline,
    /// and report the box it occupied.
    fn delimiter(
        &mut self,
        ch: char,
        size: Pt,
        up: Pt,
        down: Pt,
        x: Pt,
        out: &mut Vec<Shape>,
    ) -> (Pt, Pt, Pt) {
        let want = up + down;
        let natural = self.ext(&ch.to_string(), size);
        if let Some(parts) = self.m.stretch(ch, size, want) {
            // Each part's baseline is its offset up from the assembly's bottom edge,
            // and that bottom edge sits `down` below the formula's baseline.
            let mut w = natural.advance;
            for p in &parts {
                w = w.max(p.width);
                out.push(Shape::Glyph { index: p.index, x, y: down - p.baseline_up, size });
            }
            return (w, up, down);
        }
        out.push(Shape::Run { text: ch.to_string(), x, y: 0.0, size });
        (natural.advance, natural.ascent, natural.descent)
    }

    /// A big operator with its limits: stacked in display style, as scripts otherwise.
    fn big_op(
        &mut self,
        op: &str,
        limits: Limits,
        sub: Option<&Node>,
        sup: Option<&Node>,
        st: Style,
    ) -> (Vec<Shape>, Mb) {
        let stack = match limits {
            Limits::Always => true,
            Limits::Never => false,
            Limits::Default => st.display,
        };
        if !stack {
            return self.scripts(&Node::Atom(op.to_string()), sub, sup, st);
        }
        let e = self.ext(op, st.size);
        // A display operator is supposed to be big; a face that has no tall variant
        // simply gets scaled, which is the only thing the constant can ask for.
        let min_h = self.c(constant::DISPLAY_OPERATOR_MIN_HEIGHT, st.size, 0.0);
        let k = if min_h > 0.0 && e.height() < min_h { min_h / e.height().max(0.01) } else { 1.0 };
        let ob = Mb {
            width: e.advance * k,
            ascent: e.ascent * k,
            descent: e.descent * k,
            italic: e.italic * k,
        };
        let (a_shapes, _) = self.atom(op, Style { size: st.size * k, ..st });
        self.stacked(a_shapes, ob, sub, sup, st)
    }

    /// A label over or under a base: `\overset`, `\underset`, `\stackrel`.
    ///
    /// TeX writes all three as a forced `\limits` on a math operator, so the label is
    /// given the same script size and the same two gaps a display limit gets, and the
    /// base keeps its own style -- `\overset{?}{=}` in a sentence has to stay the
    /// relation it was.
    fn stack(
        &mut self,
        base: &Node,
        label: &Node,
        side: BarSide,
        st: Style,
    ) -> (Vec<Shape>, Mb) {
        let (b_shapes, bb) = self.lay(base, st);
        let (above, below) = match side {
            BarSide::Over => (Some(label), None),
            BarSide::Under => (None, Some(label)),
        };
        self.stacked(b_shapes, bb, below, above, st)
    }

    /// Limits above and below an already-laid-out base, centred on its width. The base
    /// comes in as shapes because both of this file's callers have one: a big operator
    /// measures its own glyph, while a stacked label keeps whatever its base turned out
    /// to be -- a word, a relation, a whole fraction.
    fn stacked(
        &mut self,
        base: Vec<Shape>,
        ob: Mb,
        sub: Option<&Node>,
        sup: Option<&Node>,
        st: Style,
    ) -> (Vec<Shape>, Mb) {
        let level1 = Style { size: self.script_size(st, 1), display: false, cramped: true };
        let sup_box = sup.map(|n| self.lay(n, level1));
        let sub_box = sub.map(|n| self.lay(n, level1));
        let mut out = Vec::new();
        let mut width = ob.ink_width();
        let mut ascent = ob.ascent;
        let mut descent = ob.descent;

        if let Some((s, sb)) = sup_box {
            // UpperLimitGapMin is the ink gap and UpperLimitBaselineRiseMin the
            // baseline distance; the base's ink top is the reference for both.
            let gap = self.c(constant::UPPER_LIMIT_GAP_MIN, st.size, 0.0);
            let rise = self.c(constant::UPPER_LIMIT_BASELINE_RISE_MIN, st.size, 0.0);
            let gap = if gap > 0.0 { gap } else { self.rules(3.0, st.size) };
            let rise = if rise > 0.0 { rise } else { self.rules(7.0, st.size) };
            let u = ob.ascent + gap.max(rise) + sb.descent;
            translate(&s, (ob.width - sb.ink_width()) / 2.0 + ob.italic / 2.0, -u, &mut out);
            width = width.max(sb.ink_width());
            ascent = ascent.max(u + sb.ascent);
        }
        translate(&base, 0.0, 0.0, &mut out);
        if let Some((s, sb)) = sub_box {
            let gap = self.c(constant::LOWER_LIMIT_GAP_MIN, st.size, 0.0);
            let drop = self.c(constant::LOWER_LIMIT_BASELINE_DROP_MIN, st.size, 0.0);
            let gap = if gap > 0.0 { gap } else { self.rules(3.0, st.size) };
            let drop = if drop > 0.0 { drop } else { self.rules(7.0, st.size) };
            let d = ob.descent + gap.max(drop) + sb.ascent;
            // The lower limit leans back by the same half correction the upper one
            // leans forward, so the pair stays centred on the operator.
            translate(&s, (ob.width - sb.ink_width()) / 2.0 - ob.italic / 2.0, d, &mut out);
            width = width.max(sb.ink_width());
            descent = descent.max(d + sb.descent);
        }
        (out, Mb { width, ascent, descent, italic: 0.0 })
    }

    /// Accents. `accentBaseHeight` is the tallest base that needs no *raising*, so a
    /// capital gets the accent lifted by the excess above it.
    ///
    /// A wide accent is the same mark taken from the face's wider drawings, which the
    /// table lists in its horizontal direction exactly as it lists the taller drawings
    /// of a bracket in its vertical one. `flattenedAccentBaseHeight` -- the one constant
    /// that speaks only of the wide forms -- is where such a mark stops needing the
    /// lift, and it is the larger of the two because a flattened accent tolerates a
    /// taller base before it has to be moved.
    fn accent(
        &mut self,
        base: &Node,
        kind: AccentKind,
        wide: bool,
        st: Style,
    ) -> (Vec<Shape>, Mb) {
        let (sb, bb) = self.lay(base, Style { cramped: true, ..st });
        let mark = kind.glyph();
        let glyph = mark.to_string();
        let sa = self.ext(&glyph, st.size);
        let lift = if wide {
            self.c(constant::FLATTENED_ACCENT_BASE_HEIGHT, st.size, 0.6)
        } else {
            self.c(constant::ACCENT_BASE_HEIGHT, st.size, 0.45)
        };
        let y = -bb.ascent - (bb.ascent - lift).max(0.0);
        // The wider drawing is asked for only once the base is actually wider than the
        // natural mark. Over a single letter the two are the same shape, and asking
        // regardless would let a face that happens to have variants swap the design's
        // own lettering into `\widehat{x}` without anything having been gained.
        let grown = (wide && bb.ink_width() > sa.advance)
            .then(|| self.m.widen(mark, st.size, bb.ink_width()))
            .flatten();
        let mut out = Vec::new();
        translate(&sb, 0.0, 0.0, &mut out);
        match grown {
            Some(parts) => {
                // The box still reports the natural mark's height: a face's wider hat is
                // its flatter one, so reserving the taller of the two can only ever make
                // the line a little roomy above, never clip the ink.
                let drawn = parts.last().map(|p| p.x + p.width).unwrap_or(sa.advance);
                let at = ((bb.width - drawn) / 2.0).max(0.0);
                for p in parts {
                    out.push(Shape::Glyph { index: p.index, x: at + p.x, y, size: st.size });
                }
            }
            None => out.push(Shape::Run {
                text: glyph,
                x: ((bb.width - sa.advance) / 2.0).max(0.0),
                y,
                size: st.size,
            }),
        }
        (
            out,
            Mb {
                width: bb.width,
                ascent: (-y + sa.ascent).max(bb.ascent),
                descent: bb.descent,
                italic: bb.italic,
            },
        )
    }

    /// `\overline` / `\underline`: a rule the width of the body, with the clearance
    /// and thickness the table gives.
    fn bar(&mut self, body: &Node, side: BarSide, st: Style) -> (Vec<Shape>, Mb) {
        let (sb, bb) = self.lay(body, Style { cramped: true, ..st });
        let (gap_c, thick_c, extra_c) = match side {
            BarSide::Over =>
                (constant::OVERBAR_VERTICAL_GAP, constant::OVERBAR_RULE_THICKNESS, constant::OVERBAR_EXTRA_ASCENDER),
            BarSide::Under => (
                constant::UNDERBAR_VERTICAL_GAP,
                constant::UNDERBAR_RULE_THICKNESS,
                constant::UNDERBAR_EXTRA_DESCENDER,
            ),
        };
        let gap = self.c(gap_c, st.size, 0.0);
        let gap = if gap > 0.0 { gap } else { self.rules(3.0, st.size) };
        let t = self.c(thick_c, st.size, FALLBACK_RULE);
        let extra = self.c(extra_c, st.size, FALLBACK_RULE);
        let mut out = Vec::new();
        translate(&sb, 0.0, 0.0, &mut out);
        let w = bb.ink_width();
        let (ascent, descent) = match side {
            BarSide::Over => {
                let y = -bb.ascent - gap - t;
                out.push(Shape::Rule { x: 0.0, y, width: w, thickness: t });
                (bb.ascent + gap + t + extra, bb.descent)
            }
            BarSide::Under => {
                let y = bb.descent + gap;
                out.push(Shape::Rule { x: 0.0, y, width: w, thickness: t });
                (bb.ascent, bb.descent + gap + t + extra)
            }
        };
        (out, Mb { width: w, ascent, descent, italic: 0.0 })
    }

    /// `\boxed` / `\fbox`: four rules around the body, [`BOX_SEPARATION`] clear of its
    /// ink and [`BOX_RULE`] thick.
    ///
    /// The body is moved right by the frame's own so that the box starts exactly where
    /// the formula says it does: a delimiter is read as hanging off what it encloses,
    /// but a frame is the thing the reader points at, and ink left of the origin would
    /// put the equation outside the margin.
    fn boxed(&mut self, body: &Node, st: Style) -> (Vec<Shape>, Mb) {
        let (sb, bb) = self.lay(body, Style { cramped: true, ..st });
        let t = BOX_RULE * st.size;
        let off = BOX_SEPARATION * st.size + t;
        let (w, h) = (bb.ink_width() + 2.0 * off, bb.height() + 2.0 * off);
        let top = -(bb.ascent + off);
        let mut out = Vec::new();
        translate(&sb, off, 0.0, &mut out);
        out.extend([
            Shape::Rule { x: 0.0, y: top, width: w, thickness: t },
            Shape::Rule { x: 0.0, y: top + h - t, width: w, thickness: t },
            Shape::Rule { x: 0.0, y: top, width: t, thickness: h },
            Shape::Rule { x: w - t, y: top, width: t, thickness: h },
        ]);
        (out, Mb { width: w, ascent: bb.ascent + off, descent: bb.descent + off, italic: 0.0 })
    }
}

/// Move a sub-layout's shapes into place, appending them to the caller's list.
fn translate(shapes: &[Shape], dx: Pt, dy: Pt, out: &mut Vec<Shape>) {
    if dx == 0.0 && dy == 0.0 {
        out.extend(shapes.iter().cloned());
        return;
    }
    for s in shapes {
        out.push(match s {
            Shape::Run { text, x, y, size } =>
                Shape::Run { text: text.clone(), x: *x + dx, y: *y + dy, size: *size },
            Shape::Glyph { index, x, y, size } =>
                Shape::Glyph { index: *index, x: *x + dx, y: *y + dy, size: *size },
            Shape::Rule { x, y, width, thickness } =>
                Shape::Rule { x: *x + dx, y: *y + dy, width: *width, thickness: *thickness },
        });
    }
}

/// The atom classes that decide inter-atom spacing: the ones that change the output.
/// Relations and binary operators need room on both sides, punctuation only after,
/// and a delimiter keeps whatever is nearest its inside tight.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Class {
    Ident,
    Rel,
    Bin,
    Punct,
    Open,
    Close,
    /// An integral-like operator, which TeX follows with a thin space.
    Big,
    Ord,
}

fn is_rel(ch: char) -> bool {
    matches!(
        ch,
        '=' | '<'
            | '>'
            | '\u{2264}'
            | '\u{2265}'
            | '\u{2260}'
            | '\u{2261}'
            | '\u{2248}'
            | '\u{2243}'
            | '\u{2242}'
            | '\u{221d}'
            | '\u{226a}'
            | '\u{226b}'
            | '\u{2208}'
            | '\u{2209}'
            | '\u{220b}'
            | '\u{2282}'
            | '\u{2283}'
            | '\u{2286}'
            | '\u{2287}'
            | '\u{2192}'
            | '\u{2190}'
            | '\u{2194}'
            | '\u{21d2}'
            | '\u{21d0}'
            | '\u{21d4}'
            | '\u{21a6}'
            | '\u{2191}'
            | '\u{2193}'
            | '\u{2195}'
            | '\u{21d1}'
            | '\u{21d3}'
            | '\u{27f5}'
            | '\u{27f6}'
            | '\u{27f7}'
            | '\u{27f8}'
            | '\u{27f9}'
            | '\u{27fa}'
            | '\u{22a2}'
            | '\u{22a3}'
            | '\u{22a4}'
            | '\u{22a5}'
            | '\u{22a8}'
            | '\u{2250}'
            | '\u{2234}'
            | '\u{2235}'
            | '\u{223c}'
    )
}

fn is_bin(ch: char) -> bool {
    matches!(
        ch,
        '+'
            | '\u{2212}'
            | '\u{00b1}'
            | '\u{2213}'
            | '\u{00d7}'
            | '\u{00f7}'
            | '\u{22c5}'
            | '\u{2217}'
            | '\u{22c6}'
            | '\u{2218}'
            | '\u{2219}'
            | '\u{2295}'
            | '\u{2296}'
            | '\u{2297}'
            | '\u{2298}'
            | '\u{2299}'
            // `\cdots` and `amsmath`'s binary dot forms: a row of centred dots stands in
            // for the operator it is named after, so it takes that operator's room.
            | '\u{22ef}'
            | '\u{222a}'
            | '\u{2229}'
            | '\u{2227}'
            | '\u{2228}'
            | '\u{2216}'
            | '\u{2223}'
            | '\u{2225}'
    )
}

fn is_open(ch: char) -> bool {
    matches!(ch, '(' | '[' | '\u{2308}' | '\u{230a}' | '\u{27e8}' | '{')
}

fn is_close(ch: char) -> bool {
    matches!(ch, ')' | ']' | '\u{2309}' | '\u{230b}' | '\u{27e9}' | '}')
}

fn is_greek(ch: char) -> bool {
    matches!(ch as u32, 0x370..=0x3ff)
}

impl Class {
    fn of(n: &Node) -> Class {
        match n {
            Node::Atom(s) => match s.chars().next() {
                Some(ch) if ch.is_alphabetic() || is_greek(ch) => Class::Ident,
                Some(ch) if is_rel(ch) => Class::Rel,
                Some(ch) if is_bin(ch) => Class::Bin,
                Some(',' | ';' | ':') => Class::Punct,
                Some(ch) if is_open(ch) => Class::Open,
                Some(ch) if is_close(ch) => Class::Close,
                _ => Class::Ord,
            },
            // A group of one presents the atom it holds, which is what makes
            // `\overset{a}{=}` a relation: `{=}` is the group the parser hands over as
            // the base, and both of a one-atom group's ends are that same atom. A wider
            // group reads as ordinary, because one class cannot say what both of its
            // ends are and TeX's own spacing at a brace comes from the atoms inside it.
            Node::Row(v) => match v.as_slice() {
                [only] => Class::of(only),
                _ => Class::Ord,
            },
            // Only an integral behaves like an operator that wants a thin space after
            // it; a `\sum` with its limits is already spaced by its own box.
            Node::BigOp { limits: Limits::Never, .. } => Class::Big,
            // A stacked label is spaced as its base is, because that is what TeX's
            // forced `\limits` on it means: `\overset{a}{=} b` reads as a relation.
            Node::Stack { base, .. } => Class::of(sole(base)),
            _ => Class::Ord,
        }
    }
}

/// The node a construct's spacing class is read from. A brace group of one is that one
/// atom -- which is why `\overset{a}{=}` keeps a relation's glue while
/// `\overset{a}{x = y}` is a box of its own, as it is in TeX.
fn sole(n: &Node) -> &Node {
    match n {
        Node::Row(v) if v.len() == 1 => sole(&v[0]),
        _ => n,
    }
}

/// Space between two adjacent atoms, in points, before either one's own advance.
fn glue(prev: Class, next: Class, size: Pt) -> Pt {
    // The muskip constants are already fractions of an em, and `size` is the em.
    match (prev, next) {
        (_, Class::Close) | (Class::Open, _) => 0.0,
        (Class::Rel, _) | (_, Class::Rel) => THICK_MU * size,
        (Class::Bin, _) | (_, Class::Bin) => MED_MU * size,
        (Class::Punct, _) => PUNCT_MU * size,
        (Class::Big, _) => THIN_MU * size,
        _ => 0.0,
    }
}

/// How far inside a column `w` wide a cell whose ink is `cw` wide starts.
fn align_in(a: ColAlign, w: Pt, cw: Pt) -> Pt {
    match a {
        ColAlign::Left => 0.0,
        ColAlign::Center => (w - cw) / 2.0,
        ColAlign::Right => w - cw,
    }
}

/// Room between column `j` and the one after it. An alignment tab carries relation
/// space on top of the column gap, because a tab usually stands in front of an `=` and
/// the two halves have to read as one relation; a `cases` condition is set further from
/// its value than matrix columns are. Both are em ratios, since `MATH` has no array
/// metrics at all.
fn column_gap(kind: ArrayKind, j: usize, size: Pt) -> Pt {
    let base = ARRAY_COL_GAP * size;
    match kind {
        ArrayKind::Cases if j == 0 => CASES_COND_GAP * size,
        ArrayKind::Align if j.is_multiple_of(2) => base + THICK_MU * size,
        _ => base,
    }
}
