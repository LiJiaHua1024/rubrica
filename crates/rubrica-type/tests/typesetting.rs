//! End-to-end checks of the typesetting core.
//!
//! Widths come from `MonospaceMeasure`, so every number below is arithmetic we can
//! do by hand. The point is to test the *breaking decisions*, not font data.

use rubrica_type::breaking::BreakOptions;
use rubrica_type::classify::Role;
use rubrica_type::justification::{line_width, place};
use rubrica_type::paragraph::{Item, MonospaceMeasure, Spacing, StyleId, StyleSpan};
use rubrica_type::units::{INFINITY, Pt};
use rubrica_type::{Hyphenation, Paragraph, Plan, typeset};

const SIZE: Pt = 16.0;

fn set(text: &str, column: Pt) -> (Paragraph, Plan) {
    set_indent(text, column, 0.0)
}

/// `ems` is the measure in ems, so a test reads like the layout it means to check.
fn set_ems(text: &str, ems: Pt) -> (Paragraph, Plan) {
    set(text, ems * SIZE)
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
fn ideograph_join_glue_is_compressible() {
    let spacing = Spacing::for_size(SIZE);
    assert!(spacing.cjk_join.stretch > 0.0);
    assert!(spacing.cjk_join.shrink > 0.0);
    let text = "中文中文中文";
    let mut measure = MonospaceMeasure { size: SIZE, factor: 0.5 };
    let (para, _) = typeset(text, &spacing, StyleId(0), &[], &BreakOptions::new(500.0), &mut measure);
    let joins = para
        .items
        .iter()
        .filter(|it| matches!(**it, Item::Glue { base, stretch, shrink, .. }
            if (base - spacing.cjk_join.base).abs() < 0.01
                && (stretch - spacing.cjk_join.stretch).abs() < 0.01
                && (shrink - spacing.cjk_join.shrink).abs() < 0.01))
        .count();
    assert!(joins >= 5, "adjacent ideographs should be separated by glue, found {joins}");
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
fn an_unsplittable_word_is_overfull_not_missing() {
    let (_, plan) = set("antiestablishmentarianism", 20.0);
    assert_eq!(plan.lines.len(), 1, "text must never be dropped, even when nothing fits");
    assert!(plan.lines[0].badness >= 10000);
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
    let text = "中文排版是一件需要认真对待的事情，行首与行尾都要对齐才能形成稳定的版面节奏。";
    let column = 12.0 * SIZE;
    let (para, plan) = set(text, column);
    let last = plan.lines.last().unwrap();
    assert!(last.is_ragged() && last.stretch >= INFINITY / 2.0, "the final line must hold \\parfillskip");

    let base = line_width(&place(&para, last));
    assert!(last.shrink > 24.0, "the line needs glue to shrink: {}", last.shrink);

    // Too wide for the measure by 24pt: the glue has to take all of it back. The
    // line's own ink is untouched -- only its joins tighten.
    let mut over = last.clone();
    over.natural = column + 24.0;
    let w = line_width(&place(&para, &over));
    assert!(
        (w - (base - 24.0)).abs() < 0.5,
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
