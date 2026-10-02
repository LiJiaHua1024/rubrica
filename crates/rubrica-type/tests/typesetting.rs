//! End-to-end checks of the typesetting core.
//!
//! Widths come from `MonospaceMeasure`, so every number below is arithmetic we can
//! do by hand. The point is to test the *breaking decisions*, not font data.

use rubrica_type::breaking::BreakOptions;
use rubrica_type::classify::Role;
use rubrica_type::justification::{line_width, place};
use rubrica_type::paragraph::{GlueRecipe, Item, MonospaceMeasure, Spacing, StyleId, StyleSpan};
use rubrica_type::units::{INFINITY, Pt};
use rubrica_type::{Hyphenation, Paragraph, Plan, typeset, typeset_hyphenated};

const SIZE: Pt = 16.0;

#[test]
fn korean_keep_all_retains_words_across_style_changes_and_forced_breaks() {
    let text = "한국어 단어\n다음 中文";
    let mut spacing = Spacing::for_size(SIZE);
    spacing.keep_korean_words = true;
    let spans = [StyleSpan { range: 3..6, style: StyleId(1) }];
    let mut measure = MonospaceMeasure { size: SIZE, factor: 1.0 };
    let mut options = BreakOptions::new(3.5 * SIZE);
    options.ragged = true;
    let (para, plan) = typeset(text, &spacing, StyleId(0), &spans, &options, &mut measure);
    let lines: Vec<_> = plan.lines.iter().map(|l| text_of(&para, text, l)).collect();
    assert_eq!(&lines[..2], ["한국어", "단어"]);
    assert!(lines[2].starts_with("다음"));
    assert_eq!(lines[2..].concat(), "다음中文");
    assert!(para.nodes.iter().any(|n| n.style == StyleId(1)));
    // The setting only suppresses Korean word-internal opportunities.
    let chinese = "中文排版测试";
    let (para, plan) = typeset(chinese, &spacing, StyleId(0), &[], &options, &mut measure);
    assert!(plan.lines.len() > 1);
    assert_eq!(plan.lines.iter().map(|l| text_of(&para, chinese, l)).collect::<String>(), chinese);
}

fn set(text: &str, column: Pt) -> (Paragraph, Plan) {
    set_indent(text, column, 0.0)
}

/// `ems` is the measure in ems, so a test reads like the layout it means to check.
fn set_ems(text: &str, ems: Pt) -> (Paragraph, Plan) {
    set(text, ems * SIZE)
}

/// A measure that gives a full-width character its em, which is what the CJK cases
/// are about -- the ASCII half-em model above would make a full-width space and a
/// half-width one indistinguishable.
fn set_cjk(text: &str, column: Pt) -> (Paragraph, Plan) {
    let spacing = Spacing::for_size(SIZE);
    let mut measure = MonospaceMeasure { size: SIZE, factor: 1.0 };
    typeset(text, &spacing, StyleId(0), &[], &BreakOptions::new(column), &mut measure)
}

fn set_indent(text: &str, column: Pt, indent: Pt) -> (Paragraph, Plan) {
    let spacing = Spacing::for_size(SIZE);
    let mut measure = MonospaceMeasure { size: SIZE, factor: 0.5 };
    let mut opts = BreakOptions::new(column);
    opts.par_indent = indent;
    typeset(text, &spacing, StyleId(0), &[], &opts, &mut measure)
}

/// As [`set_indent`], but for a marker hanging out of the left margin: the width the
/// lines under it give up.
fn set_hang(text: &str, column: Pt, hang: Pt) -> (Paragraph, Plan) {
    let spacing = Spacing::for_size(SIZE);
    let mut measure = MonospaceMeasure { size: SIZE, factor: 0.5 };
    let mut opts = BreakOptions::new(column);
    opts.hang_indent = hang;
    typeset(text, &spacing, StyleId(0), &[], &opts, &mut measure)
}

fn text_of(para: &Paragraph, src: &str, line: &rubrica_type::Line) -> String {
    let mut s = String::new();
    for i in line.items.clone() {
        if let Item::Box { node } = para.items[i] {
            let n = para.node(node);
            s.push_str(&src[n.text.clone()]);
        }
    }
    s
}

/// Sum of squared stretch ratios across justified lines; lower means more even.
fn raggedness(plan: &Plan) -> f64 {
    plan.lines
        .iter()
        .filter(|l| !l.is_ragged())
        .map(|l| {
            let need = f64::from(l.target) - f64::from(l.natural);
            let r = if l.stretch > 0.0 { need / f64::from(l.stretch) } else { 10.0 };
            r * r
        })
        .sum()
}

const PROSE: &str = "Typography is the art of arranging type so that written language stays legible and pleasant to read at a comfortable measure for long sessions";

#[test]
fn a_style_cut_inside_an_unbreakable_word_joins_instead_of_spacing() {
    let spacing = Spacing::for_size(SIZE);
    // "word" and a raised citation digit with nothing between them, which is what a
    // footnote reference does to the text it sits on. UAX #14 offers no break inside
    // `word1`, so the cut is a change of style only.
    let text = "word1 next";
    let spans = [
        StyleSpan { range: 0..4, style: StyleId(0) },
        StyleSpan { range: 4..5, style: StyleId(1) },
        StyleSpan { range: 5..10, style: StyleId(0) },
    ];
    let mut measure = MonospaceMeasure { size: SIZE, factor: 0.5 };
    let (para, _) = typeset(text, &spacing, StyleId(0), &spans, &BreakOptions::new(500.0), &mut measure);
    assert!(para.nodes.iter().any(|n| n.style == StyleId(1)), "the cut did not make its own node");

    let joins = para
        .items
        .iter()
        .filter(|it| matches!(**it, Item::Glue { base, stretch, shrink, breakable }
            if base == 0.0 && stretch == 0.0 && shrink == 0.0 && !breakable))
        .count();
    assert_eq!(joins, 1, "the cut needs a zero-width, unbreakable join");
    // The one real space is the only place a word space belongs.
    let word_spaces = para
        .items
        .iter()
        .filter(|it| matches!(**it, Item::Glue { base, .. } if base == spacing.latin_space.base))
        .count();
    assert_eq!(word_spaces, 1, "a space appeared inside the word: {:?}", para.items);
}

#[test]
fn a_grid_space_keeps_a_coloured_line_where_the_plain_one_sat() {
    // Syntax colours cut a fence into spans, and a cut landing next to a space must not
    // nudge the words after it. Under the prose recipe it does: UAX #14 offers no break
    // between the `;` and the `//`, so the plain line keeps that space inside a box
    // while the coloured one, cut at the same offset, has to make it a word space.
    let text = "let x = 1; // note";
    let spans = [
        StyleSpan { range: 0..3, style: StyleId(1) },
        StyleSpan { range: 11..19, style: StyleId(2) },
    ];
    let unit = SIZE * 0.5;
    let prose = Spacing::for_size(SIZE);
    let grid = Spacing::monospace(SIZE, unit);
    assert_ne!(
        width_of_line(text, &prose, &[]),
        width_of_line(text, &prose, &spans),
        "the prose recipe moves the ink, which is what the grid is there to stop"
    );
    let plain = width_of_line(text, &grid, &[]);
    assert_eq!(plain, width_of_line(text, &grid, &spans), "cutting the line into colours moved its ink");
    // And that width is the source's own character count, spaces included.
    assert_eq!(plain, text.chars().count() as Pt * unit);
}

#[test]
fn a_run_of_spaces_in_a_grid_is_as_wide_as_it_is_long() {
    // The columns an author lined up with spaces are the alignment a monospace block
    // has to keep, so a run of them is as wide as it is long. Prose collapses the run,
    // the way a browser does, and there that behaviour is the point.
    let unit = SIZE * 0.5;
    let grid = Spacing::monospace(SIZE, unit);
    assert_eq!(width_of_line("name   one", &grid, &[]), 10.0 * unit);
    assert_eq!(width_of_line("name   two", &grid, &[]), 10.0 * unit);
    let prose = Spacing::for_size(SIZE);
    assert_eq!(
        width_of_line("name   one", &prose, &[]),
        width_of_line("name one", &prose, &[]),
        "prose stopped collapsing what its author only meant as one space"
    );
}

fn width_of_line(text: &str, spacing: &Spacing, spans: &[StyleSpan]) -> Pt {
    let mut measure = MonospaceMeasure { size: SIZE, factor: 0.5 };
    let (para, plan) = typeset(text, spacing, StyleId(0), spans, &BreakOptions::new(500.0), &mut measure);
    assert_eq!(plan.lines.len(), 1, "the line wrapped: {:?}", plan.lines);
    line_width(&place(&para, &plan.lines[0]))
}

#[test]
fn punctuation_compression_reclaims_only_full_width_marks() {
    let text = "界，。";
    let mut spacing = Spacing::for_size(SIZE);
    spacing.punctuation_compression = 0.5;
    let mut measure = MonospaceMeasure { size: SIZE, factor: 0.5 };
    let (para, plan) = typeset(text, &spacing, StyleId(0), &[], &BreakOptions::new(500.0), &mut measure);
    let widths: Vec<_> = para.nodes.iter().map(|n| n.advance).collect();
    assert_eq!(widths, vec![SIZE * 0.5, SIZE * 0.25, SIZE * 0.25]);
    assert_eq!(para.nodes[1].punctuation, Some(rubrica_type::classify::PunctuationKind::Closing));
    assert_eq!(para.nodes[2].punctuation, Some(rubrica_type::classify::PunctuationKind::Closing));
    assert_eq!(line_width(&place(&para, &plan.lines[0])), SIZE * 1.0);
}

#[test]
fn compression_reclaims_the_blank_only_where_a_mark_has_one() {
    // A mark's blank is on the side its ink is not: the right of a closing mark, the
    // left of an opening one. Both are the side a reader sees space on before the
    // knob is turned, and both are what a shortened advance gives up -- the painter
    // moves an opening mark's glyph into the space its box no longer covers.
    //
    // The full-width symbol ％ and the ideographic space are a different case: their
    // ink is in the middle of the box (a space has none at all), so there is no blank
    // to reclaim, and shortening them would only slide them off centre between their
    // neighbours.
    let text = "界，界（界％界　界";
    let mut spacing = Spacing::for_size(SIZE);
    spacing.punctuation_compression = 0.5;
    let mut measure = MonospaceMeasure { size: SIZE, factor: 1.0 };
    let (para, _) = typeset(text, &spacing, StyleId(0), &[], &BreakOptions::new(500.0), &mut measure);
    let nodes: Vec<(char, Pt, Option<rubrica_type::classify::PunctuationKind>)> = para
        .nodes
        .iter()
        .map(|n| (text[n.text.clone()].chars().next().unwrap(), n.advance, n.punctuation))
        .collect();
    let advance = |c: char| nodes.iter().find(|(t, _, _)| *t == c).map(|(_, a, _)| *a);
    let kind = |c: char| nodes.iter().find(|(t, _, _)| *t == c).and_then(|(_, _, k)| *k);
    assert_eq!(advance('，'), Some(SIZE * 0.5), "a closing mark kept its blank half");
    assert_eq!(advance('（'), Some(SIZE * 0.5), "an opening mark kept its blank half");
    assert_eq!(advance('界'), Some(SIZE), "an ideograph was compressed");
    assert_eq!(advance('％'), Some(SIZE), "a centred full-width symbol was compressed");
    assert_eq!(advance('　'), Some(SIZE), "the ideographic space was compressed");
    assert_eq!(kind('，'), Some(rubrica_type::classify::PunctuationKind::Closing));
    assert_eq!(kind('（'), Some(rubrica_type::classify::PunctuationKind::Opening));
    assert_eq!(kind('％'), Some(rubrica_type::classify::PunctuationKind::Other));
}

#[test]
fn a_dash_between_han_takes_no_air_and_the_script_glue_is_still_the_scripts() {
    // The dash and the ellipsis belong to both scripts, and the glue model cannot see
    // the face: beside Han they are Chinese marks whose box already holds their air,
    // so the quarter em the mixed recipe would add is a gap the author never wrote.
    let spacing = Spacing::for_size(SIZE);
    let unit = SIZE * 0.5;
    let width = |text: &str| -> Pt {
        let mut measure = MonospaceMeasure { size: SIZE, factor: 0.5 };
        let (para, plan) = typeset(text, &spacing, StyleId(0), &[], &BreakOptions::new(500.0), &mut measure);
        assert_eq!(plan.lines.len(), 1, "{text:?} wrapped");
        line_width(&place(&para, &plan.lines[0]))
    };
    assert_eq!(width("他说—这样"), 5.0 * unit, "a dash between Han was given air");
    assert_eq!(width("问他……知道了"), 7.0 * unit, "an ellipsis between Han was given air");
    // The quarter em between Han and a Latin letter is not the mark's own air but the
    // mixed-script join, and it is untouched.
    assert_eq!(width("中A"), 2.0 * unit + spacing.mixed.base, "the Han/Latin join went missing");
}

#[test]
fn the_glue_beside_a_box_that_opens_with_a_quote_is_read_from_its_last_character() {
    // `"对的` is one box: UAX #14 breaks nothing inside a quoted pair, so the ASCII
    // quote and the Han characters after it are one node whose own role comes from its
    // first character -- the quote, which is Western. The boundary that follows the box
    // is nonetheless Han beside Han, and reading the left role from the quote put the
    // quarter em meant for Han-beside-Latin between `对` and `的`: bold `**"对的太对"**`
    // printed as `"对 的太对`, and `**"邪修"**` as `"邪 修`.
    let spacing = Spacing::for_size(SIZE);
    let unit = SIZE * 0.5;
    let width = |text: &str| -> Pt {
        let mut measure = MonospaceMeasure { size: SIZE, factor: 0.5 };
        let (para, plan) = typeset(text, &spacing, StyleId(0), &[], &BreakOptions::new(500.0), &mut measure);
        assert_eq!(plan.lines.len(), 1, "{text:?} wrapped");
        line_width(&place(&para, &plan.lines[0]))
    };
    assert_eq!(width("\"对的太对\""), 6.0 * unit, "air appeared inside a quoted Han phrase");
    assert_eq!(width("\"邪修\"专治"), 6.0 * unit, "air appeared inside a quoted Han phrase");
    // A quote is glued to the character before it -- UAX #14 breaks on neither side of
    // one -- so a quoted phrase takes no air at its opening either, and the box's own
    // role is not what decides that: only the character the next boundary lands beside
    // is, which is what the two assertions above are about.
    assert_eq!(width("中\"对\""), 4.0 * unit, "a quote was pushed away from the Han before it");
}

#[test]
fn a_closing_mark_can_hang_but_an_opening_mark_cannot() {
    let text = "甲乙。丙丁";
    let spacing = Spacing::for_size(SIZE);
    let mut measure = MonospaceMeasure { size: SIZE, factor: 0.5 };
    let mut options = BreakOptions::new(1.25 * SIZE);
    options.hanging_punctuation = 0.5 * SIZE;
    let (para, plan) = typeset(text, &spacing, StyleId(0), &[], &options, &mut measure);
    let first = &plan.lines[0];
    let first_text = text_of(&para, text, first);
    assert!(first_text.ends_with('。'), "line did not keep closing mark with its text: {first_text:?}");
    assert!(first.hang > 0.0, "closing mark received no hanging allowance");
    assert!(!first.is_overfull(), "hanging mark was still reported as overfull");
    assert!(first.natural <= first.target + first.hang + 0.01);
}
#[test]
fn a_break_the_source_offers_still_gets_the_scripts_glue() {
    // The join above must not swallow the gap the mixed-script rule exists to put
    // there: these two boundaries really are break opportunities.
    let spacing = Spacing::for_size(SIZE);
    let text = "中文Rust";
    let spans = [
        StyleSpan { range: 0..6, style: StyleId(0) },
        StyleSpan { range: 6..10, style: StyleId(1) },
    ];
    let mut measure = MonospaceMeasure { size: SIZE, factor: 0.5 };
    let (para, _) = typeset(text, &spacing, StyleId(0), &spans, &BreakOptions::new(500.0), &mut measure);
    assert!(
        para.items.iter().any(|it| matches!(*it, Item::Glue { base, breakable, .. }
            if base == spacing.mixed.base && breakable)),
        "quarter-em glue disappeared at the Han/Latin break opportunity"
    );
}

#[test]
fn a_break_the_source_offers_with_nothing_written_at_it_costs_no_width() {
    // ASCII punctuation is `Common`, which makes the hyphen in `rubrica-app` part of a
    // Western word's own characters -- and UAX #14 still offers a break after it. The
    // break may be taken; the gap may not appear, because no file contains a space
    // there. Same story for every `:` in a URL and every `-` in a flag.
    let spacing = Spacing::for_size(SIZE);
    let mut measure = MonospaceMeasure { size: SIZE, factor: 0.5 };
    let (para, plan) =
        typeset("rubrica-app", &spacing, StyleId(0), &[], &BreakOptions::new(500.0), &mut measure);
    assert_eq!(
        line_width(&place(&para, &plan.lines[0])),
        11.0 * SIZE * 0.5,
        "a space appeared where the author wrote a hyphen: {:?}",
        para.items
    );
    assert!(
        para.items.iter().any(|it| matches!(*it, Item::Glue { base, breakable, .. }
            if base == 0.0 && breakable)),
        "the offered break stopped being a place a line may split: {:?}",
        para.items
    );
}

#[test]
fn western_paragraph_is_flushed_to_the_measure() {
    // 34 em, a normal book measure; the old 12.5 em test column was unjustifyable
    // by any algorithm and only exercised the overfull fallback.
    let (para, plan) = set_ems(PROSE, 34.0);
    assert!(plan.lines.len() >= 2, "expected multiple lines, got {}", plan.lines.len());
    let column = 34.0 * SIZE;
    let mut justified = 0;
    for l in plan.lines.iter().take(plan.lines.len() - 1) {
        let w = line_width(&place(&para, l));
        assert!(
            (w - column).abs() < 0.5,
            "line not flushed: wanted {column}, got {w:.3} in {:?}",
            text_of(&para, PROSE, l)
        );
        assert!(l.badness < 10000, "line is overfull: {:?}", text_of(&para, PROSE, l));
        justified += 1;
    }
    let last = plan.lines.last().unwrap();
    assert!(last.is_ragged(), "final line must stay ragged");
    assert!(justified >= 1);
}

#[test]
fn chinese_justifies_without_any_word_spaces() {
    // Not one ASCII space: the only elastic material is the ideograph join, which
    // is exactly what a greedy or space-only engine has nothing to work with.
    let text = "中文排版是一件需要认真对待的事情。行首与行尾都要对齐，才能形成稳定的版面节奏，\
                这也是一篇长文读起来舒适的前提。引擎必须在整段范围内权衡每一行的松紧，\
                而不是逐行填满以后再接受由此产生的参差。"
        .to_string();
    assert!(!text.contains(' '));
    let column = 24.0 * SIZE;
    let (para, plan) = set(&text, column);
    assert!(plan.lines.len() >= 2, "expected multiple lines, got {}", plan.lines.len());
    for l in plan.lines.iter().take(plan.lines.len() - 1) {
        let w = line_width(&place(&para, l));
        assert!(
            (w - column).abs() < 0.5,
            "CJK line not flushed: wanted {column}, got {w:.3} in {:?}",
            text_of(&para, &text, l)
        );
        assert!(l.badness < 10000, "CJK line overfull: {:?}", text_of(&para, &text, l));
    }
    assert!(plan.lines.last().unwrap().is_ragged());
}

#[test]
fn global_breaking_beats_greedy_on_the_same_paragraph() {
    let column = 260.0;
    let (para, plan) = set(PROSE, column);
    let optimal = raggedness(&plan);

    // Same paragraph, greedy: take the last legal break before overflow.
    let mut greedy = 0.0;
    let mut lines = 0;
    let mut start = 0usize;
    while start < para.items.len() {
        let mut brk: Option<(usize, f64, f64)> = None;
        let mut w = 0.0f64;
        let mut i = start;
        while i < para.items.len() {
            match para.items[i] {
                Item::Box { node } => w += f64::from(para.node(node).advance),
                Item::Glue { base, stretch, shrink, breakable } => {
                    w += f64::from(base);
                    if breakable && w - f64::from(shrink) <= f64::from(column) + 0.02 {
                        brk = Some((i, w, f64::from(stretch)));
                    }
                }
                Item::Penalty { forced: true, .. } => break,
                Item::Penalty { .. } => {}
            }
            if w > f64::from(column) + 25.0 {
                break;
            }
            i += 1;
        }
        let Some((b, natural, stretch)) = brk else { break };
        if b + 1 >= para.items.len() {
            break; // the ragged remainder is not justified, so it is not scored
        }
        let r = if stretch > 0.0 { (f64::from(column) - natural) / stretch } else { 10.0 };
        greedy += r * r;
        lines += 1;
        start = b + 1;
    }
    assert!(lines >= 2, "greedy baseline degenerate, test is meaningless");
    assert!(
        optimal < greedy,
        "Knuth-Plass should set a more even paragraph than greedy: {optimal} vs {greedy}"
    );
}

#[test]
fn mixed_script_joins_get_the_quarter_em_glue() {
    let spacing = Spacing::for_size(SIZE);
    // No spaces typed around the Latin word: the gap is the engine's to supply,
    // which is the case that actually distinguishes CJK-aware typesetting.
    let text = "使用Rust实现";
    let mut measure = MonospaceMeasure { size: SIZE, factor: 0.5 };
    let (para, _) = typeset(text, &spacing, StyleId(0), &[], &BreakOptions::new(500.0), &mut measure);

    let near = |a: Pt, b: Pt| (a - b).abs() < 0.01;
    let mixed = para
        .items
        .iter()
        .filter(|it| matches!(**it, Item::Glue { base, stretch, shrink, .. }
            if near(base, spacing.mixed.base)
                && near(stretch, spacing.mixed.stretch)
                && near(shrink, spacing.mixed.shrink)))
        .count();
    assert_eq!(mixed, 2, "expected elastic glue at both Han/Latin joins");
    assert!(para.nodes.iter().any(|n| n.role == Role::Cjk));
    assert!(para.nodes.iter().any(|n| n.role == Role::Western));
    // No box may swallow a space or newline: that would double-count width.
    assert!(
        para.nodes.iter().all(|n| !text[n.text.clone()].contains(char::is_whitespace)),
        "a box contains whitespace: {:?}",
        para.nodes.iter().map(|n| &text[n.text.clone()]).collect::<Vec<_>>()
    );
}

#[test]
fn ideographs_are_separated_by_stretchable_glue() {
    let spacing = Spacing::for_size(SIZE);
    assert!(spacing.cjk_join.stretch > 0.0);
    assert_eq!(spacing.cjk_join.shrink, 0.0, "a join with no air in it has none to give back");
    let text = "中文中文中文";
    let mut measure = MonospaceMeasure { size: SIZE, factor: 0.5 };
    let (para, _) = typeset(text, &spacing, StyleId(0), &[], &BreakOptions::new(500.0), &mut measure);
    let joins = para
        .items
        .iter()
        .filter(|it| matches!(**it, Item::Glue { base, stretch, .. }
            if (base - spacing.cjk_join.base).abs() < 0.01
                && (stretch - spacing.cjk_join.stretch).abs() < 0.01))
        .count();
    assert!(joins >= 5, "adjacent ideographs should be separated by glue, found {joins}");
}

#[test]
fn a_recipe_cannot_declare_more_shrink_than_the_gap_holds() {
    // The recipes are one knob set a theme may retune, so the cap is what makes a
    // retune safe rather than a way to overlap ink from a settings screen.
    let roomy = GlueRecipe { base: 8.0, stretch: 2.0, shrink: 3.0 };
    assert!(
        matches!(Item::glue(&roomy), Item::Glue { shrink, .. } if (shrink - 3.0).abs() < 0.01),
        "a space that holds 8pt may give back the 3pt it asked for"
    );
    let tight = GlueRecipe { base: 8.0, stretch: 2.0, shrink: 20.0 };
    assert!(
        matches!(Item::glue(&tight), Item::Glue { shrink, .. } if (shrink - 8.0).abs() < 0.01),
        "a join may close, but the line stops there"
    );
    let airless = GlueRecipe { base: 0.0, stretch: 2.0, shrink: 3.2 };
    assert!(
        matches!(Item::glue(&airless), Item::Glue { shrink, .. } if shrink == 0.0),
        "ideographs touching already have nothing left to give"
    );
}

#[test]
fn closing_punctuation_never_opens_a_line() {
    // Long enough that a naive per-character break would land before the final 。
    let text = "这是一段用来测试行首禁则的中文文本它需要足够长以便在一行的末尾恰好落在句号之前这样才能够验证规则是否真的生效了呢。";
    let (para, plan) = set(text, 160.0);
    assert!(plan.lines.len() >= 2);
    for l in &plan.lines {
        let s = text_of(&para, text, l);
        assert!(
            !s.starts_with('。') && !s.starts_with('、') && !s.starts_with('」'),
            "line opens with forbidden punctuation: {s:?}"
        );
    }
}

#[test]
fn a_full_width_mark_carries_its_own_air() {
    let spacing = Spacing::for_size(SIZE);
    let wide = |text: &str| -> usize {
        let mut measure = MonospaceMeasure { size: SIZE, factor: 0.5 };
        let (para, _) =
            typeset(text, &spacing, StyleId(0), &[], &BreakOptions::new(500.0), &mut measure);
        para.items
            .iter()
            .filter(|it| matches!(**it, Item::Glue { base, .. } if base > 0.0))
            .count()
    };
    // The control: a Han/Latin join really is handed the quarter em, which is what
    // makes the three assertions after it say something rather than nothing.
    assert_eq!(wide("界R对"), 2, "both script joins carry the mixed recipe");
    // A full-width mark carries its air inside the glyph already, so the script recipe
    // on top of it was the second gap the author never wrote: `界：对` justified on the
    // page as `界 ： 对`, and `界、R` as `界、 R`.
    assert_eq!(wide("界：对"), 0, "a colon is not spaced from its own clause");
    assert_eq!(wide("界、R"), 0, "nor an enumeration comma from the Latin after it");
    assert_eq!(wide("12。中"), 0, "nor a full stop from what it closes");
}

#[test]
fn an_ideographic_space_is_ink_and_not_a_word_space() {
    // U+3000 is a full-width character, not the run of ASCII whitespace a browser
    // collapses. Peeled as one it lost its width and became a break opportunity, so
    // a two-character indent came out flush left with both spaces dropped from the
    // measure altogether, and a mid-text one set as a third of an em with a line
    // allowed to break on either side of it.
    let indent = "\u{3000}\u{3000}";
    let indented = format!("{indent}这是缩进段落。");
    let (para, plan) = set_cjk(&indented, 24.0 * SIZE);
    let (_, plain) = set_cjk("这是缩进段落。", 24.0 * SIZE);
    assert_eq!(plan.lines.len(), 1, "the case must fit on one line");
    assert_eq!(
        plan.lines[0].natural,
        plain.lines[0].natural + 2.0 * SIZE,
        "the indent is not on the line it opens"
    );
    // It is a node of its own, so it is a full-width mark like any other and may
    // surrender only its blank side bearing.
    assert!(matches!(para.items[0], Item::Box { .. }), "the paragraph opens on glue: {:?}", para.items[0]);
    assert!(para.nodes.iter().any(|n| &indented[n.text.clone()] == "\u{3000}"));

    // Mid-text the same character is a whole em of ink rather than a third of one.
    let (para, plan) = set_cjk("中\u{3000}文", 24.0 * SIZE);
    assert_eq!(plan.lines.len(), 1, "a three-em line must not break");
    assert_eq!(plan.lines[0].natural, 3.0 * SIZE, "a full-width space is a full em");
    let breaks: Vec<_> = para
        .items
        .iter()
        .filter(|it| matches!(**it, Item::Glue { base, breakable, .. } if breakable && base > 0.0))
        .collect();
    assert!(breaks.is_empty(), "an ideographic space is a break opportunity with no glue: {breaks:?}");
}

#[test]
fn hard_break_ends_a_line_and_only_the_final_line_is_ragged() {
    let text = "alpha beta gamma delta epsilon zeta eta theta\none two three four five six seven eight nine ten";
    let (para, plan) = set(text, 20.0 * SIZE);
    assert!(plan.lines.len() >= 3, "expected several lines, got {}", plan.lines.len());
    let hard = plan.lines.iter().position(|l| l.forced).expect("no line ends at the hard break");
    // Nothing after the newline may share a line with something before it.
    let before = text_of(&para, text, &plan.lines[hard]);
    let after = text_of(&para, text, &plan.lines[hard + 1]);
    assert!(before.contains("theta"), "line before the break: {before:?}");
    assert!(after.starts_with("one"), "line after the break: {after:?}");
    assert!(plan.lines.last().unwrap().is_ragged());
    // Only two kinds of line escape justification: the one the hard break ends
    // (`\hfil\break` leaves it flush at its natural width) and the paragraph's last.
    let ragged: Vec<usize> =
        plan.lines.iter().enumerate().filter(|(_, l)| l.is_ragged()).map(|(i, _)| i).collect();
    assert_eq!(ragged, vec![hard, plan.lines.len() - 1], "wrong lines were left ragged");
}

#[test]
fn a_forced_break_does_not_spread_its_line_across_the_measure() {
    let text = "第三行\n第四行";
    let column = 30.0 * SIZE;
    let (para, plan) = set(text, column);
    assert_eq!(plan.lines.len(), 2, "the hard break must end a line");
    assert!(plan.lines[0].forced);
    let w = line_width(&place(&para, &plan.lines[0]));
    assert!(w < column * 0.2, "justified a forced line: {w} of {column}");
}

#[test]
fn par_indent_shortens_only_the_first_line() {
    let column = 34.0 * SIZE;
    let indent = 40.0;
    let (para, plan) = set_indent(PROSE, column, indent);
    assert!(plan.lines.len() >= 2, "need a second line to compare against");

    assert!(plan.lines[0].first);
    assert!(plan.lines.iter().skip(1).all(|l| !l.first));
    let first = line_width(&place(&para, &plan.lines[0]));
    let second = line_width(&place(&para, &plan.lines[1]));
    assert!(
        (first - (column - indent)).abs() < 0.5,
        "first line must flush to measure minus indent: wanted {}, got {first:.3}",
        column - indent
    );
    assert!(
        (second - column).abs() < 0.5,
        "second line must flush to the full measure: wanted {column}, got {second:.3}"
    );
}

#[test]
fn hang_indent_narrows_every_line_but_the_markers_own() {
    let column = 34.0 * SIZE;
    let hang = 40.0;
    let (para, plan) = set_hang(PROSE, column, hang);
    assert!(plan.lines.len() >= 3, "need two hung lines to compare against");

    // The line the marker sits on keeps the whole measure; the marker hangs out of it
    // rather than eating into the text beside it.
    assert!(plan.lines[0].first);
    assert!(
        (plan.lines[0].target - column).abs() < 0.5,
        "first line target should be the full measure {column}, got {}",
        plan.lines[0].target
    );
    for (i, l) in plan.lines.iter().enumerate().skip(1) {
        assert!(!l.first, "only one line may be the first");
        assert!(
            (l.target - (column - hang)).abs() < 0.5,
            "line {i} should be set to {}, got {}",
            column - hang,
            l.target
        );
    }
    // A target is a promise the solver has to keep: every hung line that is not the
    // ragged last one has to be justified out to its own, narrower measure.
    for i in 1..plan.lines.len() - 1 {
        let w = line_width(&place(&para, &plan.lines[i]));
        assert!(
            (w - (column - hang)).abs() < 0.5,
            "line {i} flushed to {w:.3} instead of {}",
            column - hang
        );
    }
    // And the narrowing has to change where the text breaks, not only what it is
    // reported as: set the same prose without the marker and the second line reaches
    // further into the source than the hung one does.
    let ends_at = |para: &Paragraph, l: &rubrica_type::Line| -> usize {
        l.items
            .clone()
            .rev()
            .find_map(|i| match para.items[i] {
                Item::Box { node } => Some(para.node(node).text.end),
                _ => None,
            })
            .unwrap_or(0)
    };
    let (plain_para, plain) = set(PROSE, column);
    assert!(
        ends_at(&para, &plan.lines[1]) < ends_at(&plain_para, &plain.lines[1]),
        "a hung second line must break earlier than a flush one"
    );
}

#[test]
fn breaking_is_deterministic() {
    let text = "排版引擎必须每次给出相同的结果，否则回归测试毫无意义，golden 文件也会不断漂移。".repeat(8);
    let (_, a) = set(&text, 300.0);
    let (_, b) = set(&text, 300.0);
    assert_eq!(a.lines.len(), b.lines.len());
    for (x, y) in a.lines.iter().zip(&b.lines) {
        assert_eq!(x.items, y.items);
    }
}

#[test]
fn a_node_never_straddles_two_styles() {
    // Segments must be cut at style boundaries as well as at line-break
    // opportunities, or the renderer cannot paint a node with one font.
    let text = "plain **loud** tail";
    let loud = text.find("loud").unwrap();
    let tail = text.find("tail").unwrap();
    let spans = vec![
        StyleSpan { range: loud..loud + 4, style: StyleId(1) },
        StyleSpan { range: tail..tail + 4, style: StyleId(2) },
    ];
    let spacing = Spacing::for_size(SIZE);
    let mut measure = MonospaceMeasure { size: SIZE, factor: 0.5 };
    let (para, _) = typeset(text, &spacing, StyleId(0), &spans, &BreakOptions::new(500.0), &mut measure);

    let styles: Vec<u16> = para.nodes.iter().map(|n| n.style.0).collect();
    assert!(styles.contains(&1) && styles.contains(&2) && styles.contains(&0), "{styles:?}");
    for n in &para.nodes {
        let covered = spans.iter().find(|s| s.range.contains(&n.text.start));
        let want = covered.map_or(0, |s| s.style.0);
        assert_eq!(n.style.0, want, "node {:?} has the wrong style", &text[n.text.clone()]);
        assert!(
            covered.is_none_or(|s| s.range.end >= n.text.end),
            "node {:?} runs past its style span",
            &text[n.text.clone()]
        );
    }
}

#[test]
fn a_ragged_block_is_never_stretched_to_the_measure() {
    // Headings and code must opt out of justification; stretching them is an error.
    let text = "a heading that is quite long and would otherwise be justified across the measure";
    let spacing = Spacing::for_size(SIZE);
    let mut measure = MonospaceMeasure { size: SIZE, factor: 0.5 };
    let column = 30.0 * SIZE;
    let mut opts = BreakOptions::new(column);
    opts.ragged = true;
    let (para, plan) = typeset(text, &spacing, StyleId(0), &[], &opts, &mut measure);
    assert!(plan.lines.len() >= 2, "expected the block to wrap");
    for l in &plan.lines {
        assert!(l.is_ragged(), "a ragged block produced a justified line");
        let w = line_width(&place(&para, l));
        assert!(
            (w - l.natural).abs() < 0.5,
            "ragged line was stretched: natural {} placed {w}",
            l.natural
        );
    }
    // Ragged fill should approximate the greedy result: lines are as full as legal.
    assert!(plan.lines[0].natural > 0.85 * column, "ragged lines under-filled the measure");
}

#[test]
fn a_ragged_line_is_scored_on_the_measure_it_leaves_empty() {
    // A heading set flush left, its first line two fifths empty because the word
    // after it is a monster. The free space at the edge is the whole story, and the
    // number the `Line` carries is the cubic badness of exactly that: two fifths
    // empty is a decent line, and reading it as seven tenths of a measure empty --
    // badness is cubic, so twenty-seven times worse -- reported a loose heading as a
    // hopeless one in a number every caller reads.
    let text = "Chapter One Supercalifragilistic";
    let spacing = Spacing::for_size(SIZE);
    let mut measure = MonospaceMeasure { size: SIZE, factor: 0.5 };
    let mut opts = BreakOptions::new(12.5 * SIZE);
    opts.ragged = true;
    let (para, plan) = typeset(text, &spacing, StyleId(0), &[], &opts, &mut measure);
    assert_eq!(plan.lines.len(), 2, "the monster forces a break");
    let line = &plan.lines[0];
    assert_eq!(text_of(&para, text, line), "ChapterOne", "the monster forced the break after the second word");
    let r = (f64::from(line.target) - f64::from(line.natural)) / f64::from(line.target);
    assert!(r > 0.4, "the case must sit where the two answers differ, r = {r}");
    assert_eq!(
        line.badness,
        (100.0 * r * r * r) as i32,
        "badness {} is not the free space's own {r}",
        line.badness
    );
    assert_eq!(line.fitness, 0, "two fifths of a measure empty is a decent line");
    assert_eq!(line_width(&place(&para, line)), line.natural, "a ragged line is not stretched");
}

/// A line of code the reader cannot scroll sideways to reach is not text that hangs a
/// little -- it is text that does not exist. So a block that asked to break rather than
/// hang must be able to, even when the only place to break is inside a token and the
/// block is ragged, which is the case the pass loop used to concede without trying.
#[test]
fn a_tight_ragged_block_cuts_a_token_too_long_to_hang() {
    // Ten characters at half an em each is five ems of ink against a four-em column,
    // so there is no break the source offers and one line cannot be made to fit.
    let src = "abcdefghij";
    let spacing = Spacing::for_size(SIZE);
    let mut measure = MonospaceMeasure { size: SIZE, factor: 0.5 };
    let mut opts = BreakOptions::new(4.0 * SIZE);
    opts.ragged = true;
    opts.tight_box = true;
    let cuts: Vec<usize> = (1..src.len()).collect();
    let hyphenation = Hyphenation { points: &cuts, width: 0.0 };
    let (para, plan) = typeset_hyphenated(
        src,
        &spacing,
        StyleId(0),
        &[],
        &hyphenation,
        &opts,
        &mut measure,
    );
    assert!(plan.lines.len() >= 2, "the token was never cut: {} line(s)", plan.lines.len());
    assert!(
        plan.lines.iter().all(|l| !l.is_overfull()),
        "a tight block hung anyway: {:?}",
        plan.lines.iter().map(|l| l.natural).collect::<Vec<_>>()
    );
    // Nothing was charged for the cut, so nothing may be drawn for it: a hyphen in the
    // middle of an identifier is a lie about its name.
    assert!(
        plan.lines.iter().all(|l| l.hyphen.is_none_or(|h| para.node(h).advance == 0.0)),
        "a code cut grew a mark"
    );
}

/// The other half of the gate. A heading that will not fit hangs, and that is the end
/// of it -- escalating it would let the cuts through and hyphenate a title, which is an
/// error rather than a fix.
#[test]
fn a_ragged_block_that_may_hang_is_not_escalated_into_cuts() {
    let src = "abcdefghij";
    let spacing = Spacing::for_size(SIZE);
    let mut measure = MonospaceMeasure { size: SIZE, factor: 0.5 };
    let mut opts = BreakOptions::new(4.0 * SIZE);
    opts.ragged = true;
    let cuts: Vec<usize> = (1..src.len()).collect();
    let hyphenation = Hyphenation { points: &cuts, width: 0.0 };
    let (_, plan) =
        typeset_hyphenated(src, &spacing, StyleId(0), &[], &hyphenation, &opts, &mut measure);
    assert_eq!(plan.lines.len(), 1, "a block that may hang was cut anyway");
    assert!(plan.lines[0].is_overfull(), "the hang should still be reported as one");
}

#[test]
fn an_unsplittable_word_is_overfull_not_missing() {
    let (_, plan) = set("antiestablishmentarianism", 20.0);
    assert_eq!(plan.lines.len(), 1, "text must never be dropped, even when nothing fits");
    assert!(plan.lines[0].badness >= 10000);
}

#[test]
fn a_token_wider_than_the_measure_does_not_kill_the_solver() {
    // A token no break can split, in a column too narrow to hold it. The line to it
    // is refused for being overfull, and a refused start can never work again, so the
    // active list loses it -- and when that emptied the list no edge was ever created
    // again, the piece's terminal was unreachable, all three passes returned `None`
    // and the whole paragraph fell into `desperate`: every line flush left, every one
    // `ragged: true` with `badness == 10000`, and infinite demerits. One token in a
    // paragraph, and everything around it stopped being typeset.
    //
    // TeX puts such a line on the page and starts the next one after it
    // (`create_new_active_node`), and so does this now. The two cases are the two
    // ways a measure narrows under a token: the column itself, and a list marker
    // hanging out to the left of it.
    let src = "the extraordinarily word";
    for hang in [0.0, 2.0 * SIZE] {
        let (para, plan) = set_hang(src, 6.0 * SIZE, hang);
        assert!(plan.demerits.is_finite(), "the paragraph fell back to `desperate`");
        assert!(
            plan.lines.iter().any(|l| !l.is_ragged()),
            "every line is `ragged: true`: the greedy path answered instead of the solver"
        );
        let joined: String = plan.lines.iter().map(|l| text_of(&para, src, l)).collect();
        assert_eq!(joined, src.replace(' ', ""), "text was lost or repeated");
        // The token itself cannot fit and has to hang; the lines around it are still
        // the solver's, still justified, and still inside the measure.
        let wide = plan
            .lines
            .iter()
            .find(|l| text_of(&para, src, l).contains("extraordinarily"))
            .expect("the wide token is not on the page at all");
        assert!(wide.is_overfull(), "a token wider than the measure must be reported as hanging");
        assert!(!wide.ragged, "a hanging line is one the solver chose, not a greedy one");
        for l in plan.lines.iter().filter(|l| !l.is_overfull()) {
            let w = line_width(&place(&para, l));
            assert!(w <= l.target + 0.5, "line hangs {w} in a {} measure", l.target);
        }
    }
}

#[test]
fn empty_input_yields_no_lines() {
    let (_, plan) = set("", 200.0);
    assert!(plan.lines.is_empty());
}

/// A measure that gives one object node a real box, to prove the model carries it.
#[derive(Default)]
struct ObjectMeasure {
    /// Byte start of the node that should be treated as an inline object.
    object_at: Option<usize>,
    box_: (Pt, Pt, Pt),
}

impl rubrica_type::paragraph::Measure for ObjectMeasure {
    fn advance(&mut self, text: &str, range: std::ops::Range<usize>, _style: StyleId) -> Pt {
        if self.object_at == Some(range.start) {
            self.box_.0
        } else {
            text[range].chars().count() as Pt * SIZE * 0.5
        }
    }

    fn extent(&mut self, _text: &str, range: std::ops::Range<usize>, _style: StyleId) -> (Pt, Pt) {
        if self.object_at == Some(range.start) {
            (self.box_.1, self.box_.2)
        } else {
            (0.0, 0.0)
        }
    }
}

#[test]
fn an_inline_object_node_keeps_its_intrinsic_box() {
    // A figure taller than the text line has to make the line taller, which is the
    // reason nodes carry extents at all rather than only widths.
    let text = "\u{FFFC}";
    let spacing = Spacing::for_size(SIZE);
    let mut m = ObjectMeasure { object_at: Some(0), box_: (240.0, 100.0, 20.0) };
    let (para, plan) = typeset(text, &spacing, StyleId(0), &[], &BreakOptions::new(480.0), &mut m);
    assert_eq!(para.nodes.len(), 1);
    let n = &para.nodes[0];
    assert_eq!(n.advance, 240.0, "object advance lost");
    assert_eq!((n.ascent, n.descent), (100.0, 20.0), "object extents lost");
    assert!(!n.is_text());
    assert_eq!(plan.lines.len(), 1);
}

#[test]
fn an_object_too_wide_for_the_measure_still_reports_a_line() {
    let text = "\u{FFFC}";
    let spacing = Spacing::for_size(SIZE);
    let mut m = ObjectMeasure { object_at: Some(0), box_: (900.0, 100.0, 20.0) };
    let (_, plan) = typeset(text, &spacing, StyleId(0), &[], &BreakOptions::new(480.0), &mut m);
    assert_eq!(plan.lines.len(), 1, "an oversized figure must not vanish");
}

#[test]
fn text_nodes_still_report_zero_extent() {
    // The line box for ordinary prose comes from the fonts; a non-zero default here
    // would silently override it.
    let spacing = Spacing::for_size(SIZE);
    let mut m = MonospaceMeasure { size: SIZE, factor: 0.5 };
    let (para, _) = typeset("plain words", &spacing, StyleId(0), &[], &BreakOptions::new(480.0), &mut m);
    assert!(para.nodes.iter().all(|n| n.is_text()), "{:?}", para.nodes);
}

#[test]
fn a_dictionary_point_lets_a_line_break_inside_a_word() {
    // "the unbreakable word" in a 96pt measure: no break between whole words fits
    // (24pt alone is far too short, 117pt is too wide), so without a dictionary the
    // paragraph has no legal set of breaks at all.
    let text = "the unbreakable word";
    let column = 96.0;
    let (_, tight) = hyph_set(text, column, &[], false);
    // With no dictionary the only legal sets of breaks leave a word stranded on its
    // own line, which is what the third, tolerance-free pass exists to survive.
    assert!(tight.lines.iter().all(|l| l.hyphen.is_none()));
    assert!(
        tight.pass == 3,
        "expected the no-hyphenation case to need the tolerance-free pass, got pass {}",
        tight.pass
    );

    // Splitting "unbreak|able" gives a line of 24 + space + 56 + hyphen = 89.33,
    // whose 6.67pt shortfall the word space can absorb.
    let (_, split) = hyph_set(text, column, &[11], true);
    assert!(split.lines.len() >= 2, "hyphenation should allow a real break");
    assert!(split.lines[0].badness < 10000, "first line should no longer be flagged");
    assert!(split.lines[0].hyphen.is_some(), "the broken line must carry its hyphen");
}

#[test]
fn a_hyphenated_line_pays_for_the_glyph_it_shows() {
    let text = "the unbreakable word";
    let (para, plan) = hyph_set(text, 96.0, &[11], true);
    let l = plan.lines.iter().find(|l| l.hyphen.is_some()).expect("no hyphenated line");
    let h = l.hyphen.unwrap();
    let node = para.node(h);
    assert_eq!(node.advance, 4.0, "hyphen advance not carried onto the node");
    assert_eq!(node.kind, rubrica_type::paragraph::NodeKind::Hyphen);
    assert!(node.text.is_empty(), "a hyphen is not source text");
    // The placed line must end with the hyphen slot, or the glyph is never drawn.
    let placed = place(&para, l);
    assert_eq!(placed.last().and_then(|p| p.node), Some(h), "hyphen not placed at the line end");
}

#[test]
fn hyphenation_is_refused_when_the_option_is_off() {
    let text = "the unbreakable word";
    let (_, off) = hyph_set(text, 96.0, &[11], false);
    assert!(off.lines.iter().all(|l| l.hyphen.is_none()), "hyphenated despite being disabled");
    let (_, on) = hyph_set(text, 96.0, &[11], true);
    assert!(on.lines.iter().any(|l| l.hyphen.is_some()));
}


/// Measure that also reports a hyphen advance, so hyphenation is testable.
fn hyph_set(text: &str, column: Pt, hyphens: &[usize], allow: bool) -> (Paragraph, Plan) {
    let spacing = Spacing::for_size(SIZE);
    let mut measure = MonospaceMeasure { size: SIZE, factor: 0.5 };
    let mut opts = BreakOptions::new(column);
    opts.hyphenate = allow;
    let para = rubrica_type::paragraph::paragraph_from_text_hyphenated(
        text, &spacing, StyleId(0), &[], &Hyphenation { points: hyphens, width: 4.0 }, &mut measure,
    );
    let plan = rubrica_type::breaking::break_paragraph(&para, &opts);
    (para, plan)
}

#[test]
fn a_final_line_too_wide_for_the_measure_shrinks() {
    // `\parfillskip`'s infinite stretch excuses the last line from being *short*. It
    // has never excused it from being wide -- and the solver calls a wide line legal
    // as long as its glue can shrink, so somebody has to do that shrinking.
    let column = 26.0 * SIZE;
    let (para, plan) = set(PROSE, column);
    let last = plan.lines.last().unwrap();
    assert!(last.is_ragged() && last.stretch >= INFINITY / 2.0, "the final line must hold \\parfillskip");

    let base = line_width(&place(&para, last));
    assert!(last.shrink > 4.0, "the line needs glue to shrink: {}", last.shrink);

    // Too wide for the measure by half of what its own spaces hold: the glue has to
    // take all of it back. The line's ink is untouched -- only its word spaces
    // tighten. Half rather than a fixed number of points, because the budget is what
    // the line has, and a case that spends all of it cannot tell a cap from a cure.
    let mut over = last.clone();
    let deficit = last.shrink * 0.5;
    over.natural = column + deficit;
    let w = line_width(&place(&para, &over));
    assert!(
        (w - (base - deficit)).abs() < deficit * 0.1,
        "an overfull final line was not shrunk: placed {w}, natural-width sum {base}"
    );

    // Starved: a deficit larger than the glue holds gives back only what the joins
    // own, and the line hangs by the rest instead of overlapping its glyphs.
    let mut starved = last.clone();
    starved.natural = column + last.shrink + 30.0;
    let w = line_width(&place(&para, &starved));
    assert!(
        (w - (base - last.shrink)).abs() < 0.5,
        "a starved line was compressed past its glue budget: {w} vs {}",
        base - last.shrink
    );

    // Two exemptions the shrink must not swallow: a final line that merely has room
    // left over stays ragged, and a block that opted out of justification hangs past
    // the measure rather than being squeezed -- squeezing a heading or a code line
    // is the typographic error, not the fix.
    let mut short = last.clone();
    short.natural = column - 40.0;
    let w = line_width(&place(&para, &short));
    assert!((w - base).abs() < 0.5, "a short final line was justified: {w} vs {base}");

    let mut block = last.clone();
    block.ragged = true;
    block.stretch = 0.0;
    block.natural = column + 24.0;
    let w = line_width(&place(&para, &block));
    assert!((w - base).abs() < 0.5, "a ragged block was squeezed to fit: {w} vs {base}");
}

/// The worst negative width any glue on any justified line of `text` ends up with,
/// over a sweep of measures. A glue slot below zero is ink laid over ink.
fn worst_squeeze(text: &str) -> Pt {
    let mut worst = 0.0f32;
    for tenth in 130u16..260 {
        let (para, plan) = set_ems(text, f32::from(tenth) / 10.0);
        for l in plan.lines.iter().take(plan.lines.len().saturating_sub(1)) {
            for p in place(&para, l) {
                if p.node.is_none() {
                    worst = worst.min(p.w);
                }
            }
        }
    }
    worst
}

#[test]
fn a_join_never_gives_back_more_air_than_it_holds() {
    // Inter-ideograph glue has a base of nothing: two Han characters at natural width
    // already touch, so every point of shrink the recipe offers slides one glyph onto
    // the next. The solver reads that shrink as room and buys an extra character with
    // it, which is why a squeezed footnote read as characters running into each other.
    // A gap may close; it may not eat ink.
    let cjk = "全局断行的代价函数决定了每一行的富余量，重复引用同一个脚注得到的是同一个数字。上标和公式的下标是同一套机制，渲染的每一段文字本来就带着自己的下降量。";
    let mixed = "引擎在 Han 与 Latin 的边界自动插入约四分之一 em 的可调间距，作者不需要手动加空格：使用Rust编写、Direct2D绘制、以及Microsoft YaHei渲染中文。";
    assert!(worst_squeeze(cjk) >= 0.0, "a CJK line overlapped its glyphs by {}pt", worst_squeeze(cjk));
    assert!(worst_squeeze(mixed) >= 0.0, "a mixed line overlapped its glyphs by {}pt", worst_squeeze(mixed));
}

#[test]
fn no_line_hangs_past_the_measure() {
    // The ideograph join shrinks, so a line wider than the column is not "overfull"
    // to the solver -- but the painter still has to take that ink back, or the page
    // hangs into the margin. The final line used to be exempt from both checks.
    let text = "引擎在 Han 与 Latin 的边界自动插入约四分之一 em 的可调间距，作者不需要手动加空格：使用Rust编写、Direct2D绘制、以及Microsoft YaHei渲染中文。";
    let column = 13.0 * SIZE;
    let (para, plan) = set(text, column);
    assert!(plan.lines.len() >= 3, "need a paragraph that wraps, got {}", plan.lines.len());
    for l in &plan.lines {
        let w = line_width(&place(&para, l));
        assert!(
            w <= column + 0.5,
            "line hangs {:.1}pt past the {column}pt measure: natural {} stretch {} shrink {} badness {} pass {}",
            w - column,
            l.natural,
            l.stretch,
            l.shrink,
            l.badness,
            plan.pass
        );
    }
}

/// As [`set_indent`], but for a block that opted out of justification -- a heading,
/// a code line, a table cell -- optionally with the shrink taken away from the
/// solver as well.
fn set_box(text: &str, column: Pt, tight: bool) -> (Paragraph, Plan) {
    let spacing = Spacing::for_size(SIZE);
    let mut measure = MonospaceMeasure { size: SIZE, factor: 0.5 };
    let mut opts = BreakOptions::new(column);
    opts.ragged = true;
    opts.tight_box = tight;
    typeset(text, &spacing, StyleId(0), &[], &opts, &mut measure)
}

#[test]
fn a_tight_box_breaks_where_a_loose_ragged_block_hangs() {
    // A ragged block is never shrunk by `place`, yet the solver has been counting its
    // shrinkable joins as room -- so it happily accepts one line wider than the
    // measure and the painter leaves it hanging there. In prose that only trespasses
    // into the margin. A table cell has a neighbour at that exact spot, and the two
    // overwrite each other. Withdrawing the shrink leaves the solver one answer: break.
    //
    // The box is sized from the block's own width rather than written down, because the
    // hang has to be small enough for the glue to cover -- and an ideograph join covers
    // nothing any more, which is what `a_join_never_gives_back_more_air_than_it_holds`
    // is for. Word spaces still do, so this is the case they make.
    let text = "arranging type so that written language stays legible";
    let (_, wide) = set_box(text, 10_000.0, false);
    assert_eq!(wide.lines.len(), 1, "the probe needs the whole block on one line");
    let whole = &wide.lines[0];
    assert!(whole.shrink > 8.0, "the case needs glue that shrinks: {}", whole.shrink);
    let column = whole.natural - whole.shrink * 0.5;

    let (para, loose) = set_box(text, column, false);
    assert_eq!(loose.lines.len(), 1, "a loose box was already forced to break");
    let line = &loose.lines[0];
    let w = line_width(&place(&para, line));
    assert!(w > column + 0.5, "the case needs a line that hangs: {w} vs {column}");
    assert!(
        line.shrink >= w - column,
        "the hang must be the glue's doing: shrink {} for {:.1} of excess",
        line.shrink,
        w - column
    );

    let (tpara, tight) = set_box(text, column, true);
    assert!(tight.lines.len() >= 2, "a tight box hung instead of breaking");
    for l in &tight.lines {
        let w = line_width(&place(&tpara, l));
        assert!(w <= column + 0.5, "a tight line hangs {w} in a {column} box");
    }
}


/// Build a paragraph whose item count is well past any default piece limit.
fn long_prose(words: usize) -> String {
    (0..words).map(|i| format!("word{} ", i % 97)).collect()
}

#[test]
fn ordinary_paragraphs_never_split_and_the_answer_is_unchanged() {
    // Below the default piece limit the splitter is inert, so the plan is exactly
    // what the whole-paragraph solver produced before it existed.
    let src = long_prose(600);
    let (para, split) = {
        let mut measure = MonospaceMeasure { size: SIZE, factor: 0.5 };
        let opts = BreakOptions::new(20.0 * SIZE);
        typeset(&src, &Spacing::for_size(SIZE), StyleId(0), &[], &opts, &mut measure)
    };
    let (upara, whole) = {
        let mut measure = MonospaceMeasure { size: SIZE, factor: 0.5 };
        let mut opts = BreakOptions::new(20.0 * SIZE);
        opts.piece_limit = usize::MAX;
        typeset(&src, &Spacing::for_size(SIZE), StyleId(0), &[], &opts, &mut measure)
    };
    assert!(whole.lines.len() > 1);
    assert_eq!(split.lines.len(), whole.lines.len());
    // Spaces are glue and belong to no box, so the boxes are compared to the words.
    let joined: String = split.lines.iter().map(|l| text_of(&para, &src, l)).collect();
    assert_eq!(joined, src.replace(' ', ""));
    let _ = upara;
}

#[test]
fn a_paragraph_longer_than_one_piece_keeps_every_character() {
    let src = long_prose(4000);
    let (para, plan) = {
        let mut measure = MonospaceMeasure { size: SIZE, factor: 0.5 };
        let mut opts = BreakOptions::new(20.0 * SIZE);
        opts.piece_limit = 400;
        typeset(&src, &Spacing::for_size(SIZE), StyleId(0), &[], &opts, &mut measure)
    };
    assert!(para.items.len() > 400, "the case must outrun the piece limit");
    assert!(plan.lines.len() > 20, "the case must wrap into many lines");
    let joined: String = plan.lines.iter().map(|l| text_of(&para, &src, l)).collect();
    assert_eq!(joined, src.replace(' ', ""), "a split paragraph lost or repeated content");
    for line in &plan.lines {
        assert!(!line.is_overfull(), "a split line hangs past the measure: natural {} target {}",
            line.natural, line.target);
    }
}

#[test]
fn split_han_text_also_keeps_every_character() {
    let src: String = "中文字符排版引擎的折行求解测试".repeat(400);
    let (para, plan) = {
        let mut measure = MonospaceMeasure { size: SIZE, factor: 0.5 };
        let mut opts = BreakOptions::new(20.0 * SIZE);
        opts.piece_limit = 300;
        typeset(&src, &Spacing::for_size(SIZE), StyleId(0), &[], &opts, &mut measure)
    };
    let joined: String = plan.lines.iter().map(|l| text_of(&para, &src, l)).collect();
    assert_eq!(joined, src, "a split Han paragraph lost or repeated content");
    assert!(plan.lines.len() > 20);
}

/// The widest unbreakable run of `text`, set at a measure wide enough that
/// nothing is obliged to break.
fn fragment(text: &str) -> Pt {
    fragment_spans(text, &[])
}

fn fragment_spans(text: &str, spans: &[StyleSpan]) -> Pt {
    let mut measure = MonospaceMeasure { size: SIZE, factor: 0.5 };
    let (para, _) =
        typeset(text, &Spacing::for_size(SIZE), StyleId(0), spans, &BreakOptions::new(4000.0), &mut measure);
    para.widest_fragment()
}

#[test]
fn a_fragment_ends_where_a_break_may_be_taken() {
    // Eight points a character: "longer" is six of them, "words" five, "a" one.
    assert_eq!(fragment("a longer word"), 48.0, "the widest word, not the widest line");
}

#[test]
fn a_forced_break_ends_a_fragment_too() {
    // A `<br>` is the author asking for two lines, so the wider piece answers.
    assert_eq!(fragment("short\nlonger words"), 48.0);
}

#[test]
fn a_style_change_inside_a_word_leaves_the_word_whole() {
    // The bold half of the word is joined to the plain half rather than broken
    // there: a column as wide as "super" would leave "cali" nowhere to go.
    let bold = [StyleSpan { range: 5..9, style: StyleId(1) }];
    assert_eq!(fragment_spans("supercali", &bold), 72.0);
    assert_eq!(fragment("supercali"), 72.0);
}

#[test]
fn a_token_with_no_break_in_it_is_one_fragment() {
    // What a squeezed table column meets: the whole token is the floor, because
    // the solver has no cut to take inside it.
    assert_eq!(fragment("unbreakabletoken"), 128.0);
}

#[test]
fn a_discretionary_point_does_not_end_a_fragment() {
    // A dictionary point is a break the solver *may* take, not one it must, and a
    // conservative answer is the wide one. Ending the fragment at every point
    // reported the word a fraction of the width it occupies, so a column sized from
    // that answer could not hold the word the answer came from.
    let text = "extraordinarily";
    let mut measure = MonospaceMeasure { size: SIZE, factor: 0.5 };
    let points = [2usize, 6, 10];
    let para = rubrica_type::paragraph::paragraph_from_text_hyphenated(
        text,
        &Spacing::for_size(SIZE),
        StyleId(0),
        &[],
        &Hyphenation { points: &points, width: 4.0 },
        &mut measure,
    );
    // Fifteen characters at half an em, plus the three hyphens the one fragment holds
    // together -- any of which the solver may draw into that line.
    assert_eq!(para.widest_fragment(), 15.0 * SIZE * 0.5 + 3.0 * 4.0);
}

#[test]
fn a_paragraph_that_ends_in_a_hard_break_grows_no_blank_line() {
    // The trailing break is the line's end, not a line of its own: the piece after
    // it holds the `\parfillskip` glue and nothing else, and set as-is it was a
    // content-free line at the bottom of every paragraph closed by a `<br>`.
    let (para, plan) = set("abc\n", 6.0 * SIZE);
    let texts: Vec<_> = plan.lines.iter().map(|l| text_of(&para, "abc\n", l)).collect();
    assert_eq!(texts, ["abc"]);
    // A hard break mid-paragraph still separates the two halves it was written
    // between; only the final content-free line goes.
    let (para, plan) = set("a\nb", 6.0 * SIZE);
    let texts: Vec<_> = plan.lines.iter().map(|l| text_of(&para, "a\nb", l)).collect();
    assert_eq!(texts, ["a", "b"]);
}

#[test]
fn a_hard_break_in_a_hanging_block_continues_at_the_hanging_measure() {
    // A list item's first line runs beside the marker and the rest give the width
    // up. The line a hard break starts is one of the rest -- scored, dropped and
    // placed at the hanging measure, not at the wider first-line one the piece's
    // own first line would have. The run after the break is long enough that its
    // first line cannot pass as the piece's exempt parfill line: scored wide, it
    // would overrun the measure outright.
    let column = 12.0 * SIZE;
    let hang = 4.0 * SIZE;
    let src = "aa bb\nAA BB CC DD EE FF GG HH II JJ KK LL MM";
    let spacing = Spacing::for_size(SIZE);
    let mut measure = MonospaceMeasure { size: SIZE, factor: 0.5 };
    let mut opts = BreakOptions::new(column);
    opts.hang_indent = hang;
    opts.ragged = true;
    let (para, plan) = typeset(src, &spacing, StyleId(0), &[], &opts, &mut measure);
    let line = &plan.lines[1];
    assert_eq!(text_of(&para, src, line), "AABBCCDDEEFF");
    assert!(!line.first, "the line after a hard break is not a first line");
    assert_eq!(line.target, column - hang);
    assert!(
        line.natural <= line.target,
        "the line after a hard break overran the hanging measure: {} > {}",
        line.natural,
        line.target
    );
}

#[test]
fn every_mandatory_separator_forces_a_line() {
    // UAX #14 names a handful of mandatory separators, and the builder heard the one
    // it spells with `\n`: a CR, a VT, a NEL, a line separator and a paragraph
    // separator all set as "a b", a word space where the author ended a line.
    for (name, sep) in [
        ("LF", '\n'),
        ("CR", '\r'),
        ("VT", '\u{b}'),
        ("FF", '\u{c}'),
        ("NEL", '\u{85}'),
        ("ZL", '\u{2028}'),
        ("ZP", '\u{2029}'),
    ] {
        let src = format!("a{sep}b");
        let (para, plan) = set(&src, 6.0 * SIZE);
        let texts: Vec<_> = plan.lines.iter().map(|l| text_of(&para, &src, l)).collect();
        assert_eq!(texts, ["a", "b"], "{name} is a line the author ended, not a space");
    }
}

#[test]
fn a_dictionary_point_after_an_explicit_hyphen_draws_one_hyphen() {
    // "foo-bar" already carries the hyphen that shows at its break; the
    // dictionary point sitting right behind it must not add a discretionary of
    // its own, or the line ends "foo--".
    let src = "foo-bar";
    let spacing = Spacing::for_size(SIZE);
    let mut measure = MonospaceMeasure { size: SIZE, factor: 0.5 };
    let mut opts = BreakOptions::new(3.0 * SIZE);
    opts.ragged = true;
    let hyphenation = Hyphenation { points: &[4], width: 0.0 };
    let (para, plan) = typeset_hyphenated(src, &spacing, StyleId(0), &[], &hyphenation, &opts, &mut measure);
    let texts: Vec<_> = plan.lines.iter().map(|l| text_of(&para, src, l)).collect();
    assert_eq!(texts, ["foo-", "bar"], "{texts:?}");
    assert!(plan.lines[0].hyphen.is_none(), "the author's hyphen is the one shown");
}

#[test]
fn word_space_never_hands_the_solver_room_it_cannot_give_back() {
    // `Item::glue` caps a recipe's shrink at its base because a gap can close but
    // cannot eat ink; the literal-space path built its glue raw and skipped the
    // cap, so a theme retuning `shrink` past `base` set overlapping lines.
    let mut spacing = Spacing::for_size(SIZE);
    spacing.latin_space = GlueRecipe { base: 4.0, stretch: 8.0, shrink: 12.0 };
    let mut measure = MonospaceMeasure { size: SIZE, factor: 0.5 };
    let opts = BreakOptions::new(10.0 * SIZE);
    let (para, _plan) = typeset("a b", &spacing, StyleId(0), &[], &opts, &mut measure);
    let word_space = para
        .items
        .iter()
        .find_map(|i| match i {
            Item::Glue { base, shrink, .. } if *base > 0.0 => Some((*base, *shrink)),
            _ => None,
        })
        .expect("the word space is a glue with width");
    assert!(
        word_space.1 <= word_space.0,
        "shrink {} outran base {}",
        word_space.1,
        word_space.0
    );
}

#[test]
fn a_piece_with_no_soft_items_still_splits_and_terminates() {
    // A run longer than the solver's piece limit with nothing soft in it -- a
    // dictionary-hyphenated token, say -- used to spin `split_piece` forever:
    // with no soft item to cut at, the scan re-read the same item and its
    // distance to the piece start wrapped around.
    let src = "a".repeat(200);
    let spacing = Spacing::for_size(SIZE);
    let mut measure = MonospaceMeasure { size: SIZE, factor: 0.5 };
    let mut opts = BreakOptions::new(20.0 * SIZE);
    opts.ragged = true;
    opts.piece_limit = 64;
    let points: Vec<usize> = (2..src.len()).step_by(2).collect();
    let hyphenation = Hyphenation { points: &points, width: 0.0 };
    let (para, plan) =
        typeset_hyphenated(&src, &spacing, StyleId(0), &[], &hyphenation, &opts, &mut measure);
    assert!(!plan.lines.is_empty());
    // Nothing was lost: every letter the source had is still on the page.
    let total: usize = plan
        .lines
        .iter()
        .map(|l| text_of(&para, &src, l).len())
        .sum();
    assert_eq!(total, src.len(), "the run survived the split whole");
}

