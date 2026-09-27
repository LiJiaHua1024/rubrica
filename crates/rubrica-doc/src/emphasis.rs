//! A second chance at the emphasis the parser turned down.
//!
//! CommonMark decides whether a `*` can open emphasis by looking at the characters
//! on either side of it, and the test it uses was written for prose that separates
//! its words with spaces: a run is left-flanking when the character after it is
//! neither whitespace nor punctuation. Chinese separates nothing, so `**` sits
//! directly against an ideograph on one side and a quote on the other, and a pair
//! written as plainly as `**粗体**` fails the test on both counts. The parser then
//! hands the asterisks back as ordinary text and the reader is shown them.
//!
//! A run that reaches [`relax`] is one the parser has already refused, so nothing
//! here can contradict a decision it made. What this module adds is the reading a
//! Chinese author meant, and it is deliberately narrow: a run qualifies only when
//! the ink on its *inner* side -- the side facing the words it would wrap -- is
//! Chinese punctuation. That is the whole of the gap, because a run wrapped around
//! a Chinese *letter* never failed to begin with: the spec lets `*` open inside a
//! word, and a hanzi is not punctuation, so `AI**模型**` was never in question.
//! `锁死在**“超级增长”` and `他说“**粗体**”` both were, and both are read here.
//! What is left alone is a run facing punctuation the spec means in the Western
//! sense, so `x**"y"**` stays the text the author wrote.

use std::ops::Range;

use rubrica_type::classify::{self, Role};

use crate::{Action, InlineStyle, ObjectSpan, SourceSpan, Span};

/// The parts of a block or a cell that an edit to the text has to move together.
///
/// A block and a table cell carry the same five, so the pass is written once and
/// both hand it over through [`crate::Block::body`] and [`crate::Cell::body`].
pub(crate) struct Text<'a> {
    pub text: &'a mut String,
    pub spans: &'a mut Vec<Span>,
    pub sources: &'a mut Vec<SourceSpan>,
    pub objects: &'a mut Vec<ObjectSpan>,
    pub actions: &'a mut Vec<Action>,
}

/// One `*` run, its byte range and how many asterisks it holds.
struct Run {
    range: Range<usize>,
    len: usize,
}

/// Read the emphasis the parser left as text, and take the asterisks with it.
///
/// The run is removed from the text rather than merely hidden, because a styled
/// span over text that still spells `**` would print the asterisks anyway: a
/// delimiter the parser consumed is gone from [`crate::Block::text`], and one it
/// refuses has to be treated the same way to read the same way.
pub(crate) fn relax(t: &mut Text<'_>) {
    if !t.text.contains('*') {
        return;
    }
    let code: Vec<Range<usize>> =
        t.spans.iter().filter(|s| s.style.contains(InlineStyle::CODE)).map(|s| s.range.clone()).collect();
    let runs: Vec<Run> = runs(t.text).into_iter().filter(|r| !inside_code(&code, r)).collect();
    let mut stack: Vec<usize> = Vec::new();
    let mut pairs: Vec<(Range<usize>, Range<usize>, InlineStyle)> = Vec::new();
    for (i, r) in runs.iter().enumerate() {
        if closes(t.text, r) {
            // Only a run of the same length pairs, so `**粗*体**` loses its nesting
            // rather than gaining a guess at one: the outer pair is the reading the
            // author plainly meant, and the inner `*` stays text inside it.
            if let Some(open) = stack.iter().rposition(|&j| runs[j].len == r.len) {
                pairs.push((runs[open].range.clone(), r.range.clone(), flag(r.len)));
                stack.truncate(open);
                continue;
            }
        }
        if opens(t.text, r) {
            stack.push(i);
        }
    }
    if pairs.is_empty() {
        return;
    }
    // Styled before the text is cut, so that the wrapped words are a span of their
    // own by the time the bytes on either side of them are gone.
    for (open, close, style) in &pairs {
        style_range(t.spans, open.end..close.start, *style);
    }
    let mut cuts: Vec<Range<usize>> = pairs.iter().flat_map(|(open, close, _)| [open.clone(), close.clone()]).collect();
    cuts.sort_unstable_by_key(|r| r.start);
    cut(t, &cuts);
}

/// The runs of one or two asterisks, which are the only lengths read here.
///
/// Three or more is left to the parser: the spec's own rules for `***` are
/// intricate, and a run that long in Chinese prose is rarer than the case this
/// module exists for.
fn runs(text: &str) -> Vec<Run> {
    let mut out: Vec<Run> = Vec::new();
    let mut start: Option<usize> = None;
    let push = |out: &mut Vec<Run>, range: Range<usize>| {
        if range.len() == 1 || range.len() == 2 {
            out.push(Run { len: range.len(), range });
        }
    };
    for (at, c) in text.char_indices() {
        if c == '*' {
            start.get_or_insert(at);
        } else if let Some(from) = start.take() {
            push(&mut out, from..at);
        }
    }
    if let Some(from) = start {
        push(&mut out, from..text.len());
    }
    out
}

/// Whether the run sits in a code span, where the asterisks are the author's to
/// print. A formula cannot hold one: it reaches the text as a placeholder.
fn inside_code(code: &[Range<usize>], r: &Run) -> bool {
    code.iter().any(|c| c.start < r.range.end && r.range.start < c.end)
}

/// Whether the run may open emphasis here.
fn opens(text: &str, r: &Run) -> bool {
    after(text, r).is_some_and(|c| cjk(c) && before(text, r).is_none_or(|p| !p.is_whitespace()))
}

/// Whether the run may close emphasis here.
fn closes(text: &str, r: &Run) -> bool {
    before(text, r).is_some_and(|c| cjk(c) && after(text, r).is_none_or(|n| !n.is_whitespace()))
}

/// Han, Kana, Hangul, and the punctuation those scripts are written with.
///
/// The classification belongs to the typesetting core, which already answers the
/// question to pick the glue between two characters. A second table here would
/// only drift away from it.
fn cjk(c: char) -> bool {
    classify::is_cjk_punct(c) || classify::Role::of(c) == Role::Cjk
}

fn before(text: &str, r: &Run) -> Option<char> {
    text[..r.range.start].chars().next_back()
}

fn after(text: &str, r: &Run) -> Option<char> {
    text[r.range.end..].chars().next()
}

fn flag(len: usize) -> InlineStyle {
    if len == 2 { InlineStyle::STRONG } else { InlineStyle::EMPHASIS }
}

/// Give a byte range a style, splitting the spans it lands on so that the range
/// comes out whole in one piece of the run list.
fn style_range(spans: &mut Vec<Span>, range: Range<usize>, style: InlineStyle) {
    let mut out: Vec<Span> = Vec::new();
    for s in spans.drain(..) {
        if s.range.end <= range.start || range.end <= s.range.start {
            out.push(s);
            continue;
        }
        if s.range.start < range.start {
            out.push(Span { range: s.range.start..range.start, style: s.style });
        }
        out.push(Span {
            range: s.range.start.max(range.start)..s.range.end.min(range.end),
            style: s.style | style,
        });
        if range.end < s.range.end {
            out.push(Span { range: range.end..s.range.end, style: s.style });
        }
    }
    *spans = merge(out);
}

/// Delete the given byte ranges and move everything that pointed into the text
/// onto what is left of it.
fn cut(t: &mut Text<'_>, cuts: &[Range<usize>]) {
    let mut kept = String::with_capacity(t.text.len());
    let mut at = 0usize;
    for c in cuts {
        kept.push_str(&t.text[at..c.start]);
        at = c.end;
    }
    kept.push_str(&t.text[at..]);
    *t.text = kept;
    *t.spans = merge(t.spans.drain(..).flat_map(|s| {
        pieces(&s.range, cuts).into_iter().map(move |(range, _)| Span { range: shift_range(cuts, range), style: s.style })
    }).collect());
    *t.sources = t.sources.drain(..).flat_map(|s| {
        pieces(&s.range, cuts).into_iter()
            .map(move |(range, head)| SourceSpan { range: shift_range(cuts, range), source: s.source + head })
    }).collect();
    *t.objects = t.objects.drain(..)
        .filter_map(|o| join(&o.range, cuts).map(|range| ObjectSpan { range, kind: o.kind.clone() }))
        .collect();
    *t.actions = t.actions.drain(..)
        .filter_map(|a| join(&a.range, cuts).map(|range| Action { range, kind: a.kind.clone() }))
        .collect();
}

/// What survives of a range once the cuts are taken out of it, each piece with how
/// far into the range it began.
///
/// A [`SourceSpan`] has to be read this way rather than re-ranged across the cut:
/// its claim is that display bytes map to source bytes one for one, and a cut
/// through the middle of it leaves two such runs, not one.
fn pieces(range: &Range<usize>, cuts: &[Range<usize>]) -> Vec<(Range<usize>, usize)> {
    let mut out: Vec<(Range<usize>, usize)> = Vec::new();
    let mut at = range.start;
    for c in cuts {
        if c.end <= at {
            continue;
        }
        if range.end <= c.start {
            break;
        }
        if c.start > at {
            out.push((at..c.start, at - range.start));
        }
        at = c.end;
    }
    if at < range.end {
        out.push((at..range.end, at - range.start));
    }
    out
}

/// A range with the cuts taken out of it, its surviving ends brought together, or
/// `None` when a cut swallowed it whole.
///
/// What was on either side of a deleted run becomes adjacent, which is what a
/// target or an object wants: it was one thing before the cut and is one after.
fn join(range: &Range<usize>, cuts: &[Range<usize>]) -> Option<Range<usize>> {
    let mut kept = pieces(range, cuts);
    let first = kept.first()?.0.start;
    let last = kept.pop()?.0.end;
    Some(shift(cuts, first)..shift(cuts, last))
}

/// Where a byte ends up once the bytes before it are gone.
fn shift(cuts: &[Range<usize>], at: usize) -> usize {
    at - cuts.iter().filter(|c| c.end <= at).map(|c| c.len()).sum::<usize>()
}

fn shift_range(cuts: &[Range<usize>], range: Range<usize>) -> Range<usize> {
    shift(cuts, range.start)..shift(cuts, range.end)
}

/// Drop empty runs and join neighbours of one style, so the spans keep tiling the
/// text the way every writer here assumes they do.
fn merge(spans: Vec<Span>) -> Vec<Span> {
    let mut out: Vec<Span> = Vec::with_capacity(spans.len());
    for s in spans {
        if s.range.start >= s.range.end {
            continue;
        }
        match out.last_mut() {
            Some(prev) if prev.style == s.style && prev.range.end == s.range.start => prev.range.end = s.range.end,
            _ => out.push(s),
        }
    }
    out
}
