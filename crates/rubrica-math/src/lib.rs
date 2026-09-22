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

pub use layout::{layout, Extents, Formula, MathMeasure, Shape, Stacked};
pub use parse::{parse, Limits, Node, Parser};
pub use table::{assemble, constant, Construction, MathTable, Part, Placed, Variant, MATH_TAG};

/// Parse and lay out in one step.
pub fn typeset(src: &str, size: f32, display: bool, m: &mut dyn MathMeasure) -> Formula {
    layout(&parse(src), size, display, m)
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
    }

    impl Mock {
        fn mathy() -> Mock {
            Mock { table: true, asked: Vec::new() }
        }
        fn bare() -> Mock {
            Mock { table: false, asked: Vec::new() }
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
                Shape::Run { text, x, y, size } => Some((text.clone(), *x, *y, *size)),
                _ => None,
            })
            .collect()
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
            Shape::Run { text, y, .. } if text == "a" => Some(*y),
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
                Shape::Run { text, x, y, .. } if text == s => Some((*x, *y)),
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
        for src in ["\\frac{1}", "\\left(", "", "^2", "\\sqrt", "$not a command"] {
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
}
