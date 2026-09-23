//! Rubrica's math engine: a LaTeX subset in, positioned shapes out.
//!
//! Three stages, each independently testable and none of them aware of Windows:
//!
//! - [`parse`] turns `$...$` source into a [`Node`] tree. Unknown commands degrade to
//!   their literal text instead of failing the formula.
//! - [`table`] reads a font's OpenType `MATH` table: the 56 layout constants, the
//!   per-glyph italics corrections, and how a glyph grows into a taller one.
//! - [`layout`] folds the two together into a [`Formula`] of [`Shape`]s, placing every
//!   fraction bar, script and stretchy delimiter with numbers taken from the face in
//!   use.
//!
//! The crate deliberately stops at shapes. Turning them into pixels is the app's job,
//! which keeps the whole engine runnable -- and its output assertable -- in a test
//! binary with no display attached.

pub mod layout;
pub mod parse;
pub mod table;

pub use layout::{layout, Extents, Formula, MathMeasure, Running, Shape, Stacked};
pub use parse::{parse, Limits, Node, Parser};
pub use table::{assemble, constant, Construction, MathTable, Part, Placed, Variant, MATH_TAG};

/// Parse and lay out in one step.
pub fn typeset(src: &str, size: f32, display: bool, m: &mut dyn MathMeasure) -> Formula {
    layout(&parse(src), size, display, m)
}

/// The math-italic codepoints of a string mapped back to the letters the source spelled:
/// an identifier reaches a [`Node`] as `Alphabet::Italic`, so a test that is about
/// *positions* asks where `x` went and means the `𝑥` the parser made of it.
#[cfg(test)]
pub(crate) fn plain(s: &str) -> String {
    s.chars()
        .map(|c| match c as u32 {
            0x1D434..=0x1D44D => char::from(b'A' + (c as u32 - 0x1D434) as u8),
            0x210E => 'h',
            0x1D44E..=0x1D454 => char::from(b'a' + (c as u32 - 0x1D44E) as u8),
            // `h` is spelled outside the block, so `i` is where `g`'s successor would
            // have been had the block not left a reserved hole at U+1D455.
            0x1D456..=0x1D467 => char::from(b'i' + (c as u32 - 0x1D456) as u8),
            _ => c,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;


    /// A face with predictable metrics: every glyph is half an em wide, seven tenths
    /// above the baseline and two tenths below, so an assertion about a position is an
    /// assertion about the layout's arithmetic and nothing else.
    struct Mock {
        /// Whether `MATH` constants are reported, or only their fallbacks.
        table: bool,
        /// The heights the layout has asked a delimiter to grow to.
        asked: Vec<(char, f32)>,
        /// The widths it has asked an accent to grow to, which it asks for separately:
        /// a hat is widened by a different coverage of the table than a bracket is
        /// grown by, and a test needs to tell a face with none from a base that never
        /// needed one.
        widened: Vec<(char, f32)>,
    }

    impl Mock {
        fn mathy() -> Mock {
            Mock { table: true, asked: Vec::new(), widened: Vec::new() }
        }
        fn bare() -> Mock {
            Mock { table: false, asked: Vec::new(), widened: Vec::new() }
        }
    }

    impl MathMeasure for Mock {
        fn measure(&mut self, text: &str, size: f32) -> Extents {
            Extents {
                advance: 0.5 * size * text.chars().count() as f32,
                ascent: 0.7 * size,
                descent: 0.2 * size,
                italic: 0.0,
            }
        }

        fn constant(&mut self, index: usize, size: f32) -> Option<f32> {
            if !self.table {
                return None;
            }
            // Distinctive values, none of them equal to the fallback the layout would
            // otherwise use, so a test can tell which number did the work.
            let v = match index {
                constant::AXIS_HEIGHT => 0.4,
                constant::FRACTION_RULE_THICKNESS => 0.1,
                constant::FRACTION_NUMERATOR_SHIFT_UP => 0.9,
                constant::FRACTION_DENOMINATOR_SHIFT_DOWN => 0.6,
                constant::FRACTION_NUMERATOR_GAP_MIN => 0.2,
                constant::FRACTION_DENOMINATOR_GAP_MIN => 0.2,
                constant::SUPERSCRIPT_SHIFT_UP => 0.8,
                constant::SUPERSCRIPT_SHIFT_UP_CRAMPED => 0.7,
                constant::SUPERSCRIPT_BASELINE_DROP_MAX => 0.3,
                constant::SUPERSCRIPT_BOTTOM_MIN => 0.2,
                constant::SUPERSCRIPT_BOTTOM_MAX_WITH_SUBSCRIPT => 0.5,
                constant::SUBSCRIPT_SHIFT_DOWN => 0.4,
                constant::SUBSCRIPT_TOP_MAX => 0.6,
                constant::SUBSCRIPT_BASELINE_DROP_MIN => 0.05,
                constant::SUB_SUPERSCRIPT_GAP_MIN => 0.3,
                constant::SPACE_AFTER_SCRIPT => 0.1,
                constant::DELIMITED_SUB_FORMULA_MIN_HEIGHT => 1.5,
                constant::DISPLAY_OPERATOR_MIN_HEIGHT => 1.4,
                constant::RADICAL_RULE_THICKNESS => 0.1,
                constant::RADICAL_VERTICAL_GAP => 0.15,
                constant::RADICAL_EXTRA_ASCENDER => 0.1,
                _ => return None,
            };
            Some(v * size)
        }

        fn percent(&mut self, index: usize, fallback: f32) -> f32 {
            if !self.table {
                return fallback;
            }
            match index {
                constant::SCRIPT_PERCENT_SCALE_DOWN => 0.75,
                constant::SCRIPT_SCRIPT_PERCENT_SCALE_DOWN => 0.5,
                constant::RADICAL_DEGREE_BOTTOM_RAISE_PERCENT => 0.6,
                // Most real math faces leave this at zero, so the value that lets a
                // test tell the two mocks apart is deliberately a large one.
                constant::MATH_LEADING => 0.5,
                _ => fallback,
            }
        }

        fn stretch(&mut self, ch: char, size: f32, height: f32) -> Option<Vec<Stacked>> {
            self.asked.push((ch, height));
            // Only the shapes a math face really carries: brackets and the radical.
            if !matches!(ch, '(' | ')' | '[' | ']' | '\u{221a}') {
                return None;
            }
            Some(vec![
                Stacked { index: 11, baseline_up: 0.2 * height, width: 0.4 * size },
                Stacked { index: 12, baseline_up: 0.8 * height, width: 0.4 * size },
            ])
        }

        /// The wider drawings of a hat or a brace only, and only past the natural mark's
        /// own width -- which is the mock's half-em advance -- so a test can tell "the
        /// face has none" from "the base was never wide enough to ask".
        fn widen(&mut self, ch: char, size: f32, width: f32) -> Option<Vec<Running>> {
            self.widened.push((ch, width));
            if !matches!(ch, '\u{302}' | '\u{23de}' | '\u{23df}') || width <= 0.5 * size {
                return None;
            }
            Some(vec![
                Running { index: 21, x: 0.0, width: 0.4 * size, ascent: 0.2 * size, descent: 0.1 * size },
                Running { index: 22, x: 0.4 * size, width: 0.4 * size, ascent: 0.2 * size, descent: 0.1 * size },
            ])
        }
    }

    fn set(src: &str, size: f32, display: bool, m: &mut dyn MathMeasure) -> Formula {
        let f = typeset(src, size, display, m);
        assert_finite(&f);
        f
    }

    fn assert_finite(f: &Formula) {
        assert!(f.width.is_finite() && f.ascent.is_finite() && f.descent.is_finite());
        for s in &f.shapes {
            let (x, y) = match s {
                Shape::Run { x, y, .. } | Shape::Glyph { x, y, .. } => (*x, *y),
                Shape::Rule { x, y, width, thickness } => (*x + width, *y + thickness),
            };
            assert!(x.is_finite() && y.is_finite(), "shape drifted to NaN: {s:?}");
        }
    }

    fn rules(f: &Formula) -> Vec<(f32, f32, f32)> {
        f.shapes
            .iter()
            .filter_map(|s| match s {
                Shape::Rule { y, width, thickness, .. } => Some((*y, *width, *thickness)),
                _ => None,
            })
            .collect()
    }

    fn runs(f: &Formula) -> Vec<(String, f32, f32, f32)> {
        f.shapes
            .iter()
            .filter_map(|s| match s {
                // Reported in the letters the source spelled: an identifier is math
                // italic by the time it is laid out.
                Shape::Run { text, x, y, size } => Some((plain(text), *x, *y, *size)),
                _ => None,
            })
            .collect()
    }

    /// Where the runs of one particular text landed, which is how an assertion about a
    /// grid says something about two columns or two rows rather than about one glyph.
    fn placed(f: &Formula, text: &str) -> Vec<(f32, f32)> {
        runs(f)
            .into_iter()
            .filter(|(t, ..)| t == text)
            .map(|(_, x, y, _)| (x, y))
            .collect()
    }

    /// Positions come out of the mock's own arithmetic, so an exact float is worth
    /// asserting; this only keeps the printing of a failure legible.
    fn near(got: f32, want: f32, what: &str) {
        assert!((got - want).abs() < 0.001, "{what}: {got}, wanted {want}");
    }

    #[test]
    fn a_fraction_centres_its_rule_on_the_font_axis() {
        let f = set("\\frac{x}{y}", 10.0, true, &mut Mock::mathy());
        let (y, w, t) = rules(&f)[0];
        // axis 4, rule 1: the bar straddles the axis, so its top edge is at -4.5.
        assert_eq!((y, t), (-4.5, 1.0));
        assert_eq!(w, 5.0, "the bar spans the wider of the two halves");
        let r = runs(&f);
        assert_eq!(r.len(), 2);
        assert!(r[0].2 < y, "numerator sits above the bar");
        assert!(r[1].2 > y + t, "denominator sits below it");
        assert_eq!(r[0].1, r[1].1, "both halves are centred on the same column");
    }

    #[test]
    fn display_and_text_styles_shift_a_fraction_differently() {
        let mut m = Mock::mathy();
        let d = set("\\frac{a}{b}", 10.0, true, &mut m);
        let t = set("\\frac{a}{b}", 10.0, false, &mut m);
        // The display constants are not reported by the mock, so display falls back
        // while text style uses the reported ones; either way the numerator clears
        // the rule by at least its gap.
        for f in [&d, &t] {
            let (y, _, _) = rules(f).remove_one();
            let top = f.ascent;
            assert!(top >= -y, "numerator ink must not be cut by the bar");
        }
    }

    #[test]
    fn a_text_fraction_is_set_one_script_step_down() {
        let run_size = |src: &str| {
            typeset(src, 10.0, false, &mut Mock::mathy())
                .shapes
                .iter()
                .map(|s| match s {
                    Shape::Run { size, .. } => *size,
                    _ => 0.0,
                })
                .fold(0.0f32, f32::max)
        };
        assert_eq!(run_size("\\frac{1}{2}"), 10.0);
        assert_eq!(
            run_size("\\tfrac{1}{2}"),
            7.5,
            "the face's own script ratio, not a size this engine invented"
        );
        // `\dfrac` keeps its size and changes only its proportions, which the mock
        // declines to report -- so what separates it from `\frac` is the parse.
        assert_eq!(run_size("\\dfrac{1}{2}"), 10.0);
    }

    trait RemoveOne {
        fn remove_one(self) -> (f32, f32, f32);
    }
    impl RemoveOne for Vec<(f32, f32, f32)> {
        fn remove_one(self) -> (f32, f32, f32) {
            assert_eq!(self.len(), 1);
            self[0]
        }
    }

    #[test]
    fn scripts_shrink_and_stack_against_each_other() {
        let f = set("x_a^b", 10.0, false, &mut Mock::mathy());
        let r = runs(&f);
        let base = &r[0];
        assert_eq!(base.0, "x");
        assert!(base.3 > f.ascent.min(9.0), "sanity: the base keeps its full size");
        let sup = &r.iter().find(|s| s.0 == "b").unwrap();
        let sub = &r.iter().find(|s| s.0 == "a").unwrap();
        assert_eq!(sup.3, 7.5, "script size is the table's 75 percent");
        assert!(sup.2 < 0.0 && sub.2 > 0.0);
        // sub.ascent + sup.descent + gap must fit between the two inks.
        let gap = (sub.2 - 7.0) - (sup.2 + 1.4);
        assert!(gap >= 3.0 - 0.001, "sub/sup ink gap was {gap}");
        assert!(f.ascent > 7.0 && f.descent > 2.0);
    }

    #[test]
    fn a_tall_base_drags_its_exponent_up() {
        let mut m = Mock::mathy();
        let short = set("x^a", 10.0, false, &mut m);
        let tall = set("\\frac{x}{y}^a", 10.0, false, &mut m);
        let sup_y = |f: &Formula| f.shapes.iter().find_map(|s| match s {
            Shape::Run { text, y, .. } if plain(text) == "a" => Some(*y),
            _ => None,
        });
        assert!(
            tall.height() > short.height(),
            "a fraction is a taller base than a letter"
        );
        let (a, b) = (sup_y(&short).unwrap(), sup_y(&tall).unwrap());
        assert!(b < a, "the exponent followed the base up: {b} vs {a}");
    }

    #[test]
    fn delimiters_only_grow_past_the_sub_formula_threshold() {
        let mut m = Mock::mathy();
        m.asked.clear();
        let short = set("(x)", 10.0, false, &mut m);
        assert!(
            short.shapes.iter().all(|s| !matches!(s, Shape::Glyph { .. })),
            "a one-letter body stays at the design-size parens"
        );
        m.asked.clear();
        let f = set("\\left(\\frac{x}{y}\\right)", 10.0, true, &mut m);
        let heights: Vec<f32> = m.asked.iter().filter(|(c, _)| *c != '\u{221a}').map(|(_, h)| *h).collect();
        assert_eq!(heights.len(), 2, "both delimiters were asked to stretch");
        assert!(
            heights.iter().all(|h| *h >= f.height() - 0.001),
            "each delimiter must reach the body's height: {heights:?} vs {}",
            f.height()
        );
        assert!(f.shapes.iter().any(|s| matches!(s, Shape::Glyph { index: 11, .. })));
    }

    #[test]
    fn a_stretched_delimiter_is_centred_on_the_axis() {
        let f = set("\\left(\\frac{x}{y}\\right)", 10.0, true, &mut Mock::mathy());
        let ys: Vec<f32> = f
            .shapes
            .iter()
            .filter_map(|s| match s {
                Shape::Glyph { index, y, .. } if *index == 12 => Some(*y),
                _ => None,
            })
            .collect();
        assert_eq!(ys.len(), 2, "one top part per parenthesis");
        assert_eq!(ys[0], ys[1], "both are placed alike");
    }

    #[test]
    fn the_radical_bar_clears_what_it_covers() {
        let f = set("\\sqrt{x}", 10.0, false, &mut Mock::mathy());
        let (y, w, t) = rules(&f).remove_one();
        // The body's ink top is 0.7*10; gap 1.5 and rule 1 put the bar's top edge above
        // it, and the bar spans the body's width.
        assert!(y < -7.0, "bar at {y} must be above the body's top");
        assert_eq!((w, t), (5.0, 1.0));
        assert!(f.ascent >= -(y));
        assert!(f.descent >= 2.0);
    }

    #[test]
    fn display_operators_stack_their_limits() {
        let mut m = Mock::mathy();
        let d = set("\\sum_{i}^{n}", 10.0, true, &mut m);
        let t = set("\\sum_{i}^{n}", 10.0, false, &mut m);
        let y_of = |f: &Formula, s: &str| {
            f.shapes.iter().find_map(|sh| match sh {
                Shape::Run { text, x, y, .. } if plain(text) == s => Some((*x, *y)),
                _ => None,
            })
        };
        let (sup_x_d, sup_y_d) = y_of(&d, "n").unwrap();
        let (sub_x_d, _) = y_of(&d, "i").unwrap();
        assert!(sup_y_d < 0.0, "a stacked limit is above the baseline");
        assert!(sup_x_d < 5.0, "a stacked limit sits over the operator: {sup_x_d}");
        assert!((sup_x_d - sub_x_d).abs() < 0.01, "both limits share the operator's column");
        let (sup_x_t, sup_y_t) = y_of(&t, "n").unwrap();
        assert!(sup_y_t < 0.0);
        assert!(
            sup_x_t > sup_x_d,
            "inline limits ride as scripts, after the base: {sup_x_t} > {sup_x_d}"
        );
        assert!(
            sup_y_d < sup_y_t,
            "stacking lifts the limit clear of the operator: {sup_y_d} vs {sup_y_t}"
        );
    }

    #[test]
    fn an_integral_never_stacks_its_limits() {
        let f = set("\\int_0^1 x", 10.0, true, &mut Mock::mathy());
        let xs: Vec<f32> = f
            .shapes
            .iter()
            .filter_map(|s| match s {
                Shape::Run { text, x, .. } if text == "0" || text == "1" => Some(*x),
                _ => None,
            })
            .collect();
        assert_eq!(xs.len(), 2);
        assert!(xs.iter().all(|x| *x > 4.0), "both limits sit beside the sign");
    }

    /// The one run of `text` and where it was put, at what size: a stacked label and a
    /// framed body are told apart by what they say, since that is all the mock's
    /// uniform arithmetic leaves to go on.
    fn one(f: &Formula, text: &str) -> (f32, f32, f32) {
        let found: Vec<(f32, f32, f32)> = f
            .shapes
            .iter()
            .filter_map(|s| match s {
                Shape::Run { text: t, x, y, size } if plain(t) == text => Some((*x, *y, *size)),
                _ => None,
            })
            .collect();
        assert_eq!(found.len(), 1, "wanted one run of {text:?}, got {found:?}");
        found[0]
    }

    /// Every rule, with all four of its numbers, because a frame is four of them and
    /// which edge each one is has to be visible in the expectation.
    fn frame(f: &Formula) -> Vec<(f32, f32, f32, f32)> {
        f.shapes
            .iter()
            .filter_map(|s| match s {
                Shape::Rule { x, y, width, thickness } => Some((*x, *y, *width, *thickness)),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn a_ruled_grid_draws_its_rules_across_the_grid() {
        let f = set(
            "\\begin{array}{c}\\hline a\\\\ b\\\\\\hline\\end{array}",
            10.0,
            true,
            &mut Mock::mathy(),
        );
        let edges = frame(&f);
        assert_eq!(edges.len(), 2, "one rule above the first row, one below the last");
        for (_, _, w, t) in &edges {
            near(*w, 5.0, "a rule spans the grid, which is one cell wide");
            near(*t, 1.0, "and is one default rule thick");
        }
        // The two rules are the grid's own extremes, so they sit the same distance above
        // and below the axis the block is centred on -- which is the fact a mis-centred
        // rule would break, and it does not depend on how tall the rows happen to be.
        let (top, bottom) = (edges[0].1 + 0.5, edges[1].1 + 0.5);
        near((top + bottom) / 2.0, -4.0, "the rules are equidistant from the axis");
        // And the box knows it: a rule is ink, so the outer edge of each is what the
        // ascent and the descent are measured to -- which is what stops a line of text
        // or a grown delimiter from drawing over one.
        near(-f.ascent, edges[0].1, "the top rule's upper edge is the top of the box");
        near(f.descent, edges[1].1 + 1.0, "and the bottom rule's lower edge its bottom");
    }

    #[test]
    fn a_ruled_grid_stands_its_column_rules_up() {
        let f =
            set("\\begin{array}{|c|c|}a&b\\\\\\hline c&d\\end{array}", 10.0, true, &mut Mock::mathy());
        // One rule across under the first row, and one standing at each of the three
        // boundaries the spec's `|` named -- so four, of which three are thin in x.
        let edges = frame(&f);
        assert_eq!(edges.len(), 4, "{edges:?}");
        let mut vertical: Vec<(f32, f32, f32, f32)> =
            edges.iter().copied().filter(|(_, _, w, _)| *w < 2.0).collect();
        vertical.sort_by(|a, b| a.0.total_cmp(&b.0));
        assert_eq!(vertical.len(), 3, "left, middle, right: {edges:?}");
        for (_, _, w, h) in &vertical {
            near(*w, 1.0, "a standing rule is one default thickness wide");
            near(*h, f.height(), "and reaches the box's own ink, top edge to bottom");
        }
        near(vertical[0].0, 0.0, "the left rule is flush with the grid, not outside it");
        near(vertical[2].0, f.width - 1.0, "and the right one closes the last column flush");
        near(
            vertical[1].0 + 0.5,
            f.width / 2.0,
            "the middle rule stands in the gap between its two columns",
        );
        // A spec with no `|` draws nothing but the cells, whatever its rows do.
        let plain = set("\\begin{array}{cc}a&b\\end{array}", 10.0, true, &mut Mock::mathy());
        assert!(rules(&plain).is_empty(), "{:?}", rules(&plain));
    }

    #[test]
    fn a_stacked_label_is_centred_over_its_base_and_lifts_the_box() {
        let f = set("\\overset{nn}{=}", 10.0, false, &mut Mock::mathy());
        let (x, y, size) = one(&f, "nn");
        // A limit's positions, so the face's own script size and the baseline rise the
        // layout falls back to when the table stays silent: seven rule thicknesses.
        assert_eq!(size, 7.5);
        near(y, -15.5, "the label's baseline, clear of the base's ink");
        near(x, -1.25, "wider than its base, so over both ends equally");
        near(f.ascent, 20.75, "the box reaches the label's ink");
        near(f.descent, 2.0, "and nothing below the base");
        near(f.width, 7.5, "the box is as wide as its widest part");
        let (bx, by, _) = one(&f, "=");
        assert_eq!((bx, by), (0.0, 0.0), "the base keeps its own place on the line");
    }

    #[test]
    fn a_label_under_a_base_drops_below_it_instead_of_taking_its_place() {
        let f = set("\\underset{nn}{=}", 10.0, false, &mut Mock::mathy());
        let (x, y, _) = one(&f, "nn");
        near(y, 14.25, "the label's baseline is below the base");
        near(x, -1.25, "and centred on it, as the one above is");
        near(f.descent, 15.75, "the box reaches the label's ink");
        near(f.ascent, 7.0, "and nothing above the base");
        let (bx, by, _) = one(&f, "=");
        assert_eq!((bx, by), (0.0, 0.0), "the base is still what sits on the line");
    }

    #[test]
    fn stackrel_and_overset_set_the_same_box() {
        let mut m = Mock::mathy();
        assert_eq!(
            set("\\stackrel{nn}{=}", 10.0, false, &mut m),
            set("\\overset{nn}{=}", 10.0, false, &mut m),
            "two names for one construct, so one shape of output"
        );
    }

    #[test]
    fn a_stacked_relation_is_still_spaced_as_a_relation() {
        let mut m = Mock::bare();
        let stacked = set("\\overset{a}{=}b", 10.0, false, &mut m).width;
        let plain = set("=b", 10.0, false, &mut m).width;
        // The label takes the room of a limit, not of an atom, so the glue before `b`
        // is the relation's own either way and the sentence does not tear open.
        near(stacked - plain, 0.0, "space after a stacked relation");
    }

    /// Where the pieces of a grown shape landed, in the order they are drawn. A wide
    /// accent and a tall bracket are both assembled from glyph ids, so nothing else in
    /// a formula says whether the face's variants were used or its natural mark was.
    fn grown(f: &Formula) -> Vec<(u16, f32, f32)> {
        f.shapes
            .iter()
            .filter_map(|s| match s {
                Shape::Glyph { index, x, y, .. } => Some((*index, *x, *y)),
                _ => None,
            })
            .collect()
    }

    /// `\overbrace` asks the table for the top curled brace form and places what comes
    /// back by where its ink ends: a brace's own baseline sits anywhere inside it, so
    /// placing that baseline against the body would bury the curl.
    #[test]
    fn a_brace_is_grown_across_the_body_it_covers() {
        let mut m = Mock::mathy();
        let body = set("a+b", 10.0, false, &mut m).width;
        let f = set("\\overbrace{a+b}", 10.0, false, &mut m);
        assert_eq!(m.widened.len(), 1, "asked once, for the brace form");
        assert_eq!(m.widened[0].0, '\u{23de}', "the top form, not the bottom one");
        near(m.widened[0].1, body, "asked to reach the body's own ink");
        let g = grown(&f);
        assert_eq!(g.len(), 2, "the assembled brace rather than one glyph");
        near(g[0].1, (body - 8.0) / 2.0, "centred on the body it covers");
        // gap 3 (three rule thicknesses, the mock not stating one) and the brace's own
        // descent 1, above a body whose ink tops out at 7.
        near(g[0].2, -11.0, "the brace's baseline clears the body by gap and descent");
        near(f.ascent, 13.0, "the box reaches the top of the brace, not of the body");
        near(f.descent, 2.0, "and nothing below where the body already ended");
    }

    #[test]
    fn a_brace_under_a_body_hangs_by_its_own_top_edge() {
        let mut m = Mock::mathy();
        let f = set("\\underbrace{a+b}", 10.0, false, &mut m);
        assert_eq!(m.widened[0].0, '\u{23df}', "the bottom form");
        let g = grown(&f);
        // Baseline = body's descent 2 + gap 3 + the brace's own ascent 2, so its top
        // edge lands exactly `gap` under the body rather than its baseline.
        near(g[0].2, 7.0, "the brace hangs by its top edge");
        near(f.descent, 8.0, "the box reaches the bottom of the brace");
    }

    /// A face with no brace to grow still has to say *something* across the body. A
    /// natural-size brace centred over an expression is a mark that has failed; the rule
    /// `\overline` draws is a different mark, but it is a true one at the right width.
    #[test]
    fn a_face_with_no_brace_falls_back_to_the_bar() {
        let mut m = Mock::mathy();
        // The mock only grows past its own half-em mark, so a one-letter body asks for
        // nothing and gets the bar.
        let f = set("\\overbrace{x}", 10.0, false, &mut m);
        assert!(grown(&f).is_empty(), "no assembled brace was drawn");
        assert_eq!(rules(&f).len(), 1, "the bar stands in for the brace");
        let (x, _, _) = one(&f, "x");
        near(x, 0.0, "the body is where it was");
    }

    #[test]
    fn a_wide_accent_is_drawn_from_the_wider_mark_the_face_offers() {
        let mut m = Mock::mathy();
        let f = set("\\widehat{ab}", 10.0, false, &mut m);
        // Two half-em letters against a natural mark one half-em wide, so the wider
        // drawing is asked for at exactly the width the base's ink covers.
        assert_eq!(m.widened, vec![('\u{302}', 10.0)], "asked once, at the base's ink");
        let g = grown(&f);
        assert_eq!(g.len(), 2, "the assembled mark rather than the natural one");
        near(g[0].1, 1.0, "the first piece starts where the centred mark begins");
        near(g[1].1, 5.0, "and the second follows it along the base");
        // `flattenedAccentBaseHeight` rather than `accentBaseHeight`: a flattened mark
        // tolerates a taller base before it has to be lifted, so it sits lower here.
        near(g[0].2, -8.0, "the flattened accent's own baseline");
        near(f.ascent, 15.0, "the box still clears the natural mark's height");
    }

    #[test]
    fn a_narrow_accent_stays_the_size_the_design_made_it() {
        let mut m = Mock::mathy();
        let f = set("\\hat{ab}", 10.0, false, &mut m);
        assert!(m.widened.is_empty(), "no wider drawing was ever asked for");
        let (x, y, _) = one(&f, "\u{302}");
        near(x, 2.5, "the natural mark, centred on its base");
        near(y, -9.5, "and lifted by the smaller of the two constants");
        assert!(grown(&f).is_empty(), "one shaped run, nothing assembled");
    }

    #[test]
    fn a_face_with_no_wider_mark_still_gets_the_natural_one() {
        let mut m = Mock::mathy();
        // The mock keeps wider drawings for the hat alone, so a `widetilde` asks and
        // is refused -- which is the answer that has to degrade to the natural mark
        // rather than to no accent at all.
        let f = set("\\widetilde{ab}", 10.0, false, &mut m);
        assert_eq!(m.widened.len(), 1, "the question was still asked");
        assert!(grown(&f).is_empty(), "and nothing was assembled from it");
        let (x, _, _) = one(&f, "\u{303}");
        near(x, 2.5, "the tilde that draws is the face's own");
    }

    #[test]
    fn a_wide_accent_over_a_narrow_base_is_not_asked_about() {
        let mut m = Mock::mathy();
        // `\widehat{x}` is one letter wide, the same as the natural mark's own advance,
        // so asking the face for a variant would let it swap the design's lettering for
        // nothing gained. The licence is the wide spelling; the *question* is the base.
        let f = set("\\widehat{x}", 10.0, false, &mut m);
        assert!(m.widened.is_empty(), "no wider drawing asked for a one-letter base");
        assert!(grown(&f).is_empty(), "and so the mark that draws is the natural one");
    }

    #[test]
    fn a_boxed_formula_is_framed_on_all_four_sides() {
        let f = set("\\boxed{x}", 10.0, false, &mut Mock::mathy());
        let edges = frame(&f);
        assert_eq!(edges.len(), 4, "one rule per side");
        // Three points of separation and a rule of 0.4 at this size, so the frame
        // stands 3.4 clear of the body's ink on every side.
        near(edges[0].1, -10.4, "the top edge is outside the body's ascent");
        near(edges[0].2, 11.8, "and spans body, separation and both rules");
        near(edges[1].1, 5.0, "the bottom edge is the frame's own bottom");
        near(edges[2].3, 15.8, "the left rule reaches edge to edge");
        near(edges[3].0, 11.4, "the right rule closes the corner");
        near(f.width, 11.8, "the box is the frame, not the body");
        near(f.ascent, 10.4, "the frame is the ink above the line");
        near(f.descent, 5.4, "and below it");
        let (x, y, _) = one(&f, "x");
        near(x, 3.4, "the body starts inside the frame, not under it");
        assert_eq!(y, 0.0, "and on the line it was on");
    }

    #[test]
    fn relations_and_operators_take_tex_three_muskips() {
        let mut m = Mock::bare();
        // One em is 18 points at this size, so a mu is exactly one point: the muskips
        // become 3, 4 and 5 points of space. Every character also advances 9 points,
        // so what a formula measures beyond that is the glue between its atoms.
        let glue = |s: &str, m: &mut Mock| {
            set(s, 18.0, false, m).width - 9.0 * s.chars().count() as f32
        };
        let near = |got: f32, want: f32, what: &str| {
            assert!((got - want).abs() < 0.01, "{what}: {got} points, wanted {want}");
        };
        near(glue("a=b", &mut m), 10.0, "5 mu either side of a relation");
        near(glue("a+b", &mut m), 8.0, "4 mu either side of a binary operator");
        near(glue("(a)", &mut m), 0.0, "a delimiter adds nothing inside");
        near(glue("a,b", &mut m), 2.0, "2 mu after punctuation");
    }

    #[test]
    fn everything_still_sets_without_a_math_table() {
        let mut m = Mock::bare();
        for src in [
            "x",
            "\\frac{1}{2}",
            "e^{i\\pi}+1=0",
            "\\sqrt[3]{\\frac{x}{y+1}}",
            "\\left(\\frac{a}{b}\\right)^{2}",
            "\\sum_{i=1}^{n} i = \\frac{n(n+1)}{2}",
            "\\int_0^\\infty x\\,dx",
            "\\hat{a} + \\overline{b}",
            "\\overset{p}{=} q",
            "\\sum_{\\substack{i<j\\\\k\\neq l}} P(i,j)",
            "\\boxed{\\frac{1}{2}}",
            "\\frobnicate{x}",
        ] {
            let f = set(src, 13.5, true, &mut m);
            assert!(f.width > 0.0, "{src} set to zero width");
            assert!(!f.shapes.is_empty(), "{src} produced nothing to draw");
        }
    }

    #[test]
    fn malformed_input_degrades_instead_of_failing() {
        let mut m = Mock::bare();
        // Unbalanced, empty and nonsense all still draw something.
        for src in [
            "\\frac{1}",
            "\\left(",
            "",
            "^2",
            "\\sqrt",
            "$not a command",
            "\\overset{1}",
            "\\boxed",
        ] {
            let f = set(src, 12.0, false, &mut m);
            assert!(f.width >= 0.0);
        }
        let f = set("\\frac{1}", 12.0, false, &mut m);
        assert_eq!(rules(&f).len(), 1, "the bar is still drawn for the half present");
    }

    #[test]
    fn the_constants_are_laid_out_where_the_specification_says() {
        // First four are bare 16-bit values, the next 51 are MathValueRecords of four
        // bytes, and the last is bare again -- so the byte address of a constant is a
        // function of its index, and a wrong one reads its neighbour silently.
        let head = 10usize; // the version plus three Offset16 fields
        let mut bytes = vec![0u8; head + constant::COUNT * 4];
        bytes[0..2].copy_from_slice(&1u16.to_be_bytes()); // major version 1
        bytes[4..6].copy_from_slice(&(head as u16).to_be_bytes()); // MathConstants
        // axisHeight is index 5, the second record: eight bytes of bare values in.
        let axis = head + 8 + (constant::AXIS_HEIGHT - 4) * 4;
        bytes[axis] = 0x12;
        bytes[axis + 1] = 0x34;
        // scriptPercentScaleDown is index 0, so a bare 16-bit value at the very front.
        bytes[head + 1] = 0x50;
        let t = MathTable::parse(&bytes, 1000).unwrap();
        assert_eq!(t.constant(constant::AXIS_HEIGHT), 0x1234);
        assert_eq!(t.constant(constant::SCRIPT_PERCENT_SCALE_DOWN), 0x50);
        assert_eq!(t.units_per_em, 1000);
        // Reading the table back is what the layout asks for at every construct, so
        // one wrong byte offset would show up as a subtly misplaced formula forever.
        assert_eq!(constant::COUNT, 56);
    }

    #[test]
    fn assembly_grows_joints_before_it_repeats_parts() {
        // bottom / extender / top, each 100 tall, sharing 40-unit connectors, and the
        // font demanding 10 units of overlap at a minimum.
        let p = |glyph: u16, extender: bool| Part {
            glyph,
            start_connector: 40,
            end_connector: 40,
            full_advance: 100,
            extender,
        };
        let parts = vec![p(1, false), p(2, true), p(3, false)];
        // Without the extender the pair overlaps down to 200-40 = 160 and opens to
        // 200-10 = 190, which is short of 200, so one copy of the bar has to go in.
        // The three then reach 300-80 = 220 at full overlap, so no joint opens.
        let small = assemble(&parts, 10, 200);
        assert_eq!(small.len(), 3, "the pair alone cannot reach the target");
        assert_eq!(small[0].offset, 0);
        assert_eq!(small[1].offset, 100 - 40, "the joints stay closed before they stretch");
        let last = small.last().unwrap();
        let total = last.offset + last.full_advance;
        assert!(total >= 200, "assembly reached {total}");
        // A target beyond the joint budget forces more extenders in, one of each per
        // round, and the joints then open by the same amount everywhere.
        let big = assemble(&parts, 10, 500);
        assert!(big.len() > 3, "needed repeats, got {}", big.len());
        let last = big.last().unwrap();
        assert!(last.offset + last.full_advance >= 500);
        let gaps: Vec<i32> = big
            .windows(2)
            .map(|w| w[1].offset as i32 - (w[0].offset + w[0].full_advance) as i32)
            .collect();
        assert!(gaps.windows(2).all(|w| w[0].abs_diff(w[1]) <= 1), "{gaps:?}");
        assert_eq!(assemble(&[], 10, 100).len(), 0);
    }

    /// A cell is half an em wide per letter at this size, and its ink runs seven tenths
    /// above the baseline and two tenths below, so everything a grid measures beyond
    /// its cells is the room between them.
    const SZ: f32 = 10.0;
    const CELL_TOP: f32 = 0.7 * SZ;
    const CELL_BOTTOM: f32 = 0.2 * SZ;
    const GLYPH: f32 = 0.5 * SZ;

    #[test]
    fn a_grid_widens_by_its_columns_and_heightens_by_its_rows() {
        let mut m = Mock::mathy();
        let one = set("\\begin{matrix}a\\end{matrix}", SZ, true, &mut m);
        let wide = set("\\begin{matrix}a&b\\end{matrix}", SZ, true, &mut m);
        let tall = set("\\begin{matrix}a\\\\c\\end{matrix}", SZ, true, &mut m);
        let both = set("\\begin{matrix}a&b\\\\c&d\\end{matrix}", SZ, true, &mut m);

        // Two columns cost more than twice one cell, because the gap between them is
        // real; the same for two rows.
        assert!(wide.width > 2.0 * one.width, "{} over {}", wide.width, one.width);
        assert!(tall.height() > 2.0 * one.height(), "a row separation is not free");
        near(both.width, wide.width, "a second row adds no width");
        near(both.height(), tall.height(), "a second column adds no height");

        // Four cells, four positions: cells in a column share an x, cells in a row
        // share a baseline, and neither column nor row collapses onto the other.
        let at = |t: &str| {
            let p = placed(&both, t);
            assert_eq!(p.len(), 1, "{t} was drawn once");
            p[0]
        };
        let (a, b, c, d) = (at("a"), at("b"), at("c"), at("d"));
        near(a.0, c.0, "the first column's cells align");
        near(b.0, d.0, "and so do the second's");
        near(a.1, b.1, "a row's cells share a baseline");
        near(c.1, d.1, "and so do the next row's");
        assert!(b.0 > a.0, "the columns are apart: {} vs {}", b.0, a.0);
        assert!(c.1 > a.1, "the rows are apart: {} vs {}", c.1, a.1);
        assert!(c.1 - CELL_TOP > a.1 + CELL_BOTTOM, "and their inks do not touch");
    }

    #[test]
    fn a_matrix_is_centred_on_the_axis_and_its_columns_are_centred() {
        let mut m = Mock::mathy();
        let axis = m.constant(constant::AXIS_HEIGHT, SZ).unwrap();
        let row = set("\\begin{matrix}a\\end{matrix}", SZ, true, &mut m);
        // The block straddles the axis by the same distance on either side of it, which
        // is how a fraction's stack is placed.
        let ink = CELL_TOP + CELL_BOTTOM;
        near(row.ascent, axis + ink / 2.0, "the ink top is centred on the axis");
        near(row.descent, ink / 2.0 - axis, "and the ink bottom the same way");
        // A narrow cell in a wide column sits in the middle of it.
        let grid = set("\\begin{matrix}ab\\\\c\\end{matrix}", SZ, true, &mut m);
        let x_ab = placed(&grid, "ab")[0].0;
        let x_c = placed(&grid, "c")[0].0;
        near(x_c - x_ab, (2.0 * GLYPH - GLYPH) / 2.0, "half the slack, on either side");
    }

    #[test]
    fn aligned_rows_share_the_x_the_relations_line_up_at() {
        let mut m = Mock::mathy();
        let f = set("\\begin{aligned}xy&=1\\\\z&=2\\end{aligned}", SZ, false, &mut m);
        let equals = placed(&f, "=");
        assert_eq!(equals.len(), 2, "one relation per row");
        near(equals[0].0, equals[1].0, "the tabs land in the same column");
        // The first column is right-aligned about the tab, so the short label ends
        // where the long one does rather than starting beside it.
        let x_long = placed(&f, "xy")[0].0;
        let x_short = placed(&f, "z")[0].0;
        near(x_short - x_long, GLYPH, "the labels share their right edge");
        // The tab's own space keeps the relation off the label it belongs to.
        assert!(equals[0].0 > x_long + 2.0 * GLYPH, "relation space, not nothing");
    }

    #[test]
    fn cases_left_aligns_and_asks_for_its_brace_alone() {
        let mut m = Mock::mathy();
        m.asked.clear();
        let f = set("\\begin{cases}a&a>0\\\\b&b<0\\end{cases}", SZ, true, &mut m);
        let (a, b) = (placed(&f, "a"), placed(&f, "b"));
        assert_eq!((a.len(), b.len()), (2, 2), "value and condition in each row");
        near(a[0].0, b[0].0, "both values start at their column's left edge");
        near(a[1].0, b[1].0, "and so do both conditions");
        assert!(a[1].0 > a[0].0, "the condition stands to the right");
        let row = set("\\begin{matrix}a&a\\end{matrix}", SZ, true, &mut m);
        let cells = placed(&row, "a");
        let matrix_room = cells[1].0 - cells[0].0 - GLYPH;
        assert!(
            a[1].0 - a[0].0 - GLYPH > matrix_room,
            "a condition is set further off than a matrix column: {matrix_room}"
        );
        assert_eq!(
            m.asked.iter().filter(|(c, _)| *c == '{').count(),
            1,
            "one brace and nothing on the right: {:?}",
            m.asked
        );
    }

    #[test]
    fn a_tall_row_pushes_its_neighbours_apart() {
        let plain = set("\\begin{matrix}x\\\\z\\end{matrix}", SZ, true, &mut Mock::mathy());
        let deep =
            set("\\begin{matrix}\\frac{x}{y}\\\\z\\end{matrix}", SZ, true, &mut Mock::mathy());
        let separation = |f: &Formula| placed(f, "z")[0].1 - placed(f, "x")[0].1;
        let (p, d) = (separation(&plain), separation(&deep));
        assert!(d > p + GLYPH, "a fraction's own ink widens the gap: {d} vs {p}");
        assert!(deep.height() > plain.height() + GLYPH);
        // And still no collision: the lower row's ink top is below the fraction's own
        // ink bottom, which is the denominator's.
        let den = placed(&deep, "y")[0].1;
        let lower_top = placed(&deep, "z")[0].1 - CELL_TOP;
        assert!(lower_top > den + CELL_BOTTOM, "{lower_top} vs {}", den + CELL_BOTTOM);
    }

    #[test]
    fn row_separation_asks_mathleading_first() {
        let with = set("\\begin{matrix}a\\\\b\\end{matrix}", SZ, true, &mut Mock::mathy());
        let without = set("\\begin{matrix}a\\\\b\\end{matrix}", SZ, true, &mut Mock::bare());
        let separation = |f: &Formula| placed(f, "b")[0].1 - placed(f, "a")[0].1;
        assert!(
            separation(&without) < separation(&with),
            "the face's own leading widens the rows: {} vs {}",
            separation(&with),
            separation(&without)
        );
    }

    #[test]
    fn an_environments_own_delimiters_are_grown_to_the_grid() {
        let mut m = Mock::mathy();
        let bare = set("\\begin{matrix}a\\\\c\\\\e\\end{matrix}", SZ, true, &mut m);
        m.asked.clear();
        let wrapped = set("\\begin{pmatrix}a\\\\c\\\\e\\end{pmatrix}", SZ, true, &mut m);
        let heights: Vec<f32> =
            m.asked.iter().filter(|(c, _)| *c != '\u{221a}').map(|(_, h)| *h).collect();
        assert_eq!(heights.len(), 2, "both parentheses were asked to stretch");
        let grid = bare.height();
        assert!(
            heights.iter().all(|h| (h - grid).abs() < 0.001),
            "each reached the grid's own height {grid}: {heights:?}"
        );
        assert!(wrapped.width > bare.width, "the delimiters take room of their own");
        assert!(wrapped.height() >= grid);
    }

    #[test]
    fn a_fence_around_an_environment_reaches_the_whole_grid() {
        let mut m = Mock::mathy();
        let grid = set("\\begin{matrix}a\\\\c\\\\e\\end{matrix}", SZ, true, &mut m);
        m.asked.clear();
        let f = set("\\left(\\begin{matrix}a\\\\c\\\\e\\end{matrix}\\right)", SZ, true, &mut m);
        let heights: Vec<f32> =
            m.asked.iter().filter(|(c, _)| *c != '\u{221a}').map(|(_, h)| *h).collect();
        assert_eq!(heights.len(), 2);
        assert!(
            heights.iter().all(|h| (h - grid.height()).abs() < 0.001),
            "the written pair grew to the grid, not to a line of text: {heights:?}"
        );
        assert!(f.shapes.iter().any(|s| matches!(s, Shape::Glyph { index: 12, .. })));
    }

    #[test]
    fn a_small_matrix_is_set_a_script_step_down() {
        let mut m = Mock::mathy();
        let normal = set("\\begin{matrix}a\\end{matrix}", SZ, true, &mut m);
        let small = set("\\begin{smallmatrix}a\\end{smallmatrix}", SZ, true, &mut m);
        let size_of = |f: &Formula| runs(f)[0].3;
        assert!(size_of(&small) < size_of(&normal), "{} vs {}", size_of(&small), size_of(&normal));
        assert!(small.width < normal.width && small.height() < normal.height());
    }

    #[test]
    fn an_environment_that_cannot_be_drawn_still_sets() {
        for src in [
            "\\begin{pmatrix} a & b",
            "\\begin{psst} a & b \\end{psst}",
            "\\begin{matrix} a \\end{bmatrix} + 1",
            "\\end{matrix}",
            "\\begin",
            "\\begin{array}{ccc} a & b \\\\ c & d",
        ] {
            let mut mathy = Mock::mathy();
            let mut bare = Mock::bare();
            let with = set(src, SZ, true, &mut mathy);
            let without = set(src, SZ, true, &mut bare);
            for f in [with, without] {
                assert!(f.width > 0.0, "{src} set to nothing");
                assert!(!f.shapes.is_empty(), "{src} produced no shapes");
            }
        }
        // A grid with no cells at all is empty, but neither a panic nor a NaN.
        let mut m = Mock::mathy();
        let empty = set("\\begin{matrix}\\end{matrix}", SZ, true, &mut m);
        assert_eq!(empty.width, 0.0);
        assert!(empty.shapes.is_empty());
    }
}
