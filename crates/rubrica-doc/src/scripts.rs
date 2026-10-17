//! The sub- and superscripts Pandoc writes with `~` and `^`, read once the parser
//! is done with a block.
//!
//! Markdown has no script syntax: Pandoc's reader takes `H~2~O` and `x^2^` as its
//! own extension, and pulldown-cmark has an extension of the same name that is not
//! the same rule -- it refuses to open or close inside a word, so the canonical
//! `H~2~O` is exactly the case it turns down, and with the extension off it reads
//! `~` as the start of a deletion instead: `a ~b~ c` leaves the parser as one
//! strikethrough. What arrives here is therefore a block's finished text, and the
//! work is the same as [`crate::emphasis`]'s: read the marks the parser would not
//! and take them out of the text, because a delimiter the reader has consumed must
//! not still be printed.
//!
//! The rules are Pandoc's, checked against it run by run:
//!
//! * a `^` raises the words up to the next `^`, a `~` lowers them to the next `~`;
//! * the marks may touch a word on either side -- `H~2~O` is the reason the rule
//!   exists -- and may pair across emphasis or a link (`x^*a*^`) as Pandoc does;
//! * the content may hold no whitespace: a space is legal only escaped, `x^a\ b^`,
//!   and becomes a non-breaking space (`U+00A0`), which is what Pandoc prints;
//! * an escaped mark (`\~`, `\^`) is the author's own character: it is printed and
//!   pairs with nothing, which is why the source, not the text, is asked;
//! * a `~` or `^` alone is printed as itself: no pair, no mark;
//! * marks pair left to right, each closing its nearest open same mark, and a mark
//!   that cannot close may still open -- `a^b c^d^` is `a^b c` with a raised `d`,
//!   not `a` with a raised `b c`, which is the reading Pandoc gives it.
//!
//! Two lengths are read, and no others: a run of two or more unescaped marks is
//! left exactly as it was. That is where this pass parts with Pandoc on purpose:
//! Pandoc's reader eats the extra tildes and can leave an *empty* script behind
//! (`x~~~~y` and `x^^2^^` both produce a zero-width mark), which no author wrote
//! and no reader wants; `~~del~~` keeps its ordinary meaning and `x^^2^^` prints
//! as it was typed. An object -- a figure or a formula -- is not wrapped either:
//! a placeholder character carries none of the size or position a script needs,
//! so `x^$a$^` stays literal where Pandoc would raise the formula.

use std::ops::Range;

use crate::emphasis::{self, Text};
use crate::{source_at, Action, InlineStyle, ObjectSpan, Scripts, SourceSpan, Span};

/// What an escaped space becomes: Pandoc's `x^a\ b^` prints `a` + U+00A0 + `b`.
/// A plain space would be a break opportunity, which is the one thing the escape
/// exists to take away.
const NBSP: &str = "\u{a0}";

/// One `^` or `~` the text admits as a delimiter: a run of exactly one unescaped
/// mark, outside code.
struct Run {
    range: Range<usize>,
    /// `^` raises, `~` lowers.
    raised: bool,
}

/// One change to the text: a range taken out, and what stands in its place.
struct Edit {
    range: Range<usize>,
    text: &'static str,
    /// Where the replacement's bytes come from, for the source map.
    source: Option<usize>,
}

/// Read the scripts an author wrote with `~` and `^` out of a block's text, and say
/// how many there were.
pub(crate) fn relax(t: &mut Text<'_>, original: &str) -> Scripts {
    if !(t.text.contains('^') || t.text.contains('~')) {
        return Scripts::default();
    }
    let code: Vec<Range<usize>> = t
        .spans
        .iter()
        .filter(|s| s.style.contains(InlineStyle::CODE))
        .map(|s| s.range.clone())
        .collect();
    let (runs, wide) = scan(t.text, t.sources, original, &code);
    if runs.is_empty() {
        return Scripts::default();
    }
    // Two stacks -- one per mark -- and a list of the delimiters already spent.
    // Left to right, a mark closes the nearest open mark of its own kind when the
    // words between them qualify, and otherwise opens one; a pair whose content
    // reaches across a spent delimiter is refused, so two pairs can nest but never
    // cross.
    let mut open: [Vec<usize>; 2] = [Vec::new(), Vec::new()];
    let mut spent: Vec<Range<usize>> = Vec::new();
    let mut pairs: Vec<(Range<usize>, InlineStyle)> = Vec::new();
    let mut edits: Vec<Edit> = Vec::new();
    for (i, r) in runs.iter().enumerate() {
        let kind = usize::from(!r.raised);
        let mut closed = false;
        if let Some(&top) = open[kind].last() {
            let content = runs[top].range.end..r.range.start;
            if let Some(spaces) = wraps(t, &content, &wide, &spent, original) {
                pairs.push((content.clone(), flag(r.raised)));
                edits.push(Edit { range: runs[top].range.clone(), text: "", source: None });
                edits.push(Edit { range: r.range.clone(), text: "", source: None });
                spent.push(runs[top].range.clone());
                spent.push(r.range.clone());
                for space in spaces {
                    let source = source_at(t.sources, space.start + 1);
                    edits.push(Edit { range: space, text: NBSP, source });
                }
                open[kind].pop();
                // An open mark of the other kind inside the pair has been wrapped
                // and can open nothing more: Pandoc reads `~a^b~c^` as a lowered
                // `a^b` and a printed `c^`, not as a pair of crossing marks.
                let (from, to) = (content.start, content.end);
                let other = 1 - kind;
                open[other].retain(|&j| !(from <= runs[j].range.start && runs[j].range.end <= to));
                closed = true;
            }
        }
        if !closed {
            open[kind].push(i);
        }
    }
    if pairs.is_empty() {
        return Scripts::default();
    }
    let raised = pairs.iter().filter(|(_, style)| style.contains(InlineStyle::SUPERSCRIPT)).count();
    let found = Scripts { raised, lowered: pairs.len() - raised };
    // Styled before the text is cut, so a wrapped word is one run of its own by the
    // time the marks around it are gone -- and a pair inside a pair, `x^~a~^`, ends
    // up wearing both bits.
    for (content, style) in &pairs {
        emphasis::style_range(t.spans, content.clone(), *style);
    }
    // One edit per range: two nested pairs that each found the same escaped space
    // must not turn it into two spaces.
    edits.sort_by_key(|e| e.range.start);
    edits.dedup_by(|b, a| b.range == a.range);
    // A replacement carries the style of the byte it stands for, which the pass
    // above has already made the pair's own.
    let styles: Vec<InlineStyle> = edits
        .iter()
        .map(|e| style_at(t.spans, e.range.start))
        .collect();
    apply(t, &edits, &styles);
    found
}

/// Every mark the text admits, and every stretch of two or more that no pair may
/// wrap.
///
/// A stretch is split where the source's backslashes change its meaning: in `\~~`
/// the author escaped one tilde and wrote one, so the readable run is one mark and
/// not two, and only the source can say so -- by the time the text is here the
/// backslash is gone.
fn scan(text: &str, sources: &[SourceSpan], original: &str, code: &[Range<usize>]) -> (Vec<Run>, Vec<Range<usize>>) {
    let mut runs = Vec::new();
    let mut wide = Vec::new();
    let bytes = text.as_bytes();
    let mut at = 0usize;
    while at < bytes.len() {
        let mark = bytes[at];
        if mark != b'^' && mark != b'~' {
            at += 1;
            continue;
        }
        let mut end = at;
        while end < bytes.len() && bytes[end] == mark {
            end += 1;
        }
        let mut start = at;
        let mut escaped = is_escaped(original, source_at(sources, at));
        let mut i = at + 1;
        while i <= end {
            let this = i < end && is_escaped(original, source_at(sources, i));
            if i == end || this != escaped {
                let range = start..i;
                // An escaped piece is the author's own character and bears no
                // reading at all; only an unescaped one is a mark or a stretch.
                if !escaped && !code.iter().any(|c| overlaps(c, &range)) {
                    if range.len() == 1 {
                        runs.push(Run { range: range.clone(), raised: mark == b'^' });
                    } else {
                        wide.push(range);
                    }
                }
                start = i;
                escaped = this;
            }
            i += 1;
        }
        at = end;
    }
    (runs, wide)
}

/// Whether the words between two marks can carry a script, and the escaped spaces
/// among them that stand for non-breaking ones. `None` is a refusal, and the marks
/// are then printed as they were written.
fn wraps(
    t: &Text<'_>,
    content: &Range<usize>,
    wide: &[Range<usize>],
    spent: &[Range<usize>],
    original: &str,
) -> Option<Vec<Range<usize>>> {
    // A stretch of two or more is syntax this reader has no reading for: wrapping
    // it would print the extra marks from inside a raised word.
    if wide.iter().any(|r| overlaps(r, content)) {
        return None;
    }
    // A mark already spent on another pair: wrapping it would cross the two pairs
    // rather than nest them.
    if spent.iter().any(|r| overlaps(r, content)) {
        return None;
    }
    // A figure or a formula is a box, not words: there is nothing here to raise.
    if t.objects.iter().any(|o| overlaps(&o.range, content)) {
        return None;
    }
    let mut spaces = Vec::new();
    let mut at = content.start;
    while at < content.end {
        let c = t.text[at..].chars().next()?;
        let width = c.len_utf8();
        if c == ' ' {
            // A space is legal only escaped, and the backslash must be one the
            // author wrote: `x^a\\ b^` has a literal backslash before the space,
            // and Pandoc reads that space as one of the whitespace a script may
            // not hold.
            let backslash = at.checked_sub(1).filter(|&b| b >= content.start)?;
            if t.text.as_bytes()[backslash] != b'\\' || is_escaped(original, source_at(t.sources, backslash)) {
                return None;
            }
            spaces.push(backslash..at + width);
        } else if c.is_whitespace() {
            return None;
        }
        at += width;
    }
    Some(spaces)
}

/// Rewrite the text with the edits applied, and move every span, source, object
/// and target onto what is left.
///
/// A cut and a replacement are the same walk: what a [`Text`] holds all ranges
/// against the text being rewritten, so one pass that shifts by the length each
/// edit takes out and puts back keeps the lot in step.
fn apply(t: &mut Text<'_>, edits: &[Edit], styles: &[InlineStyle]) {
    let mut kept = String::with_capacity(t.text.len());
    let mut at = 0usize;
    for e in edits {
        kept.push_str(&t.text[at..e.range.start]);
        kept.push_str(e.text);
        at = e.range.end;
    }
    kept.push_str(&t.text[at..]);
    *t.text = kept;
    let mut spans: Vec<Span> = Vec::new();
    for s in t.spans.drain(..) {
        for (range, _) in keep(&s.range, edits) {
            spans.push(Span { range: moved(edits, range.start)..moved(edits, range.end), style: s.style });
        }
    }
    for (e, style) in edits.iter().zip(styles).filter(|(e, _)| !e.text.is_empty()) {
        spans.push(Span { range: moved(edits, e.range.start)..moved(edits, e.range.end), style: *style });
    }
    // A replacement is found where it was put, so the list leaves this walk in the
    // order it was collected rather than the order it stands in the text; the merge
    // joins neighbours and has to see them the way the text reads.
    spans.sort_by_key(|s| (s.range.start, s.range.end));
    *t.spans = emphasis::merge(spans);
    let mut sources: Vec<SourceSpan> = Vec::new();
    for s in t.sources.drain(..) {
        for (range, head) in keep(&s.range, edits) {
            sources.push(SourceSpan {
                range: moved(edits, range.start)..moved(edits, range.end),
                source: s.source + head,
            });
        }
    }
    for e in edits.iter().filter(|e| !e.text.is_empty()) {
        if let Some(source) = e.source {
            let at = moved(edits, e.range.start);
            // One byte per source span, all naming the byte the replacement came
            // from: a mapping that claims `byte + 1` is the next source byte would
            // run off the end of a file whose last byte a script ended on.
            for i in 0..e.text.len() {
                sources.push(SourceSpan { range: at + i..at + i + 1, source });
            }
        }
    }
    sources.sort_by_key(|s| s.range.start);
    *t.sources = sources;
    *t.objects = t
        .objects
        .drain(..)
        .filter_map(|o| joined(&o.range, edits).map(|range| ObjectSpan { range, kind: o.kind }))
        .collect();
    *t.actions = t
        .actions
        .drain(..)
        .filter_map(|a| joined(&a.range, edits).map(|range| Action { range, kind: a.kind }))
        .collect();
}

/// What survives of a range once the edits are taken out of it, each piece with
/// how far into the range it began.
fn keep(range: &Range<usize>, edits: &[Edit]) -> Vec<(Range<usize>, usize)> {
    let mut out: Vec<(Range<usize>, usize)> = Vec::new();
    let mut at = range.start;
    for e in edits {
        if e.range.end <= at {
            continue;
        }
        if range.end <= e.range.start {
            break;
        }
        if e.range.start > at {
            out.push((at..e.range.start, at - range.start));
        }
        at = at.max(e.range.end);
    }
    if at < range.end {
        out.push((at..range.end, at - range.start));
    }
    out
}

/// A range with the edits taken out of it and its ends brought together, or `None`
/// when an edit swallowed it whole.
fn joined(range: &Range<usize>, edits: &[Edit]) -> Option<Range<usize>> {
    let mut kept = keep(range, edits);
    let first = kept.first()?.0.start;
    let last = kept.pop()?.0.end;
    Some(moved(edits, first)..moved(edits, last))
}

/// Where a byte ends up once the edits before it are applied.
fn moved(edits: &[Edit], at: usize) -> usize {
    let mut out = at;
    for e in edits {
        if e.range.end <= at {
            out = out - e.range.len() + e.text.len();
        } else {
            break;
        }
    }
    out
}

/// The style of the byte an edit's range begins at.
fn style_at(spans: &[Span], byte: usize) -> InlineStyle {
    spans
        .iter()
        .find(|s| s.range.start <= byte && byte < s.range.end)
        .map_or(InlineStyle::EMPTY, |s| s.style)
}

/// Whether the byte is escaped: an odd number of backslashes stands in front of it
/// in the source the author wrote.
fn is_escaped(original: &str, at: Option<usize>) -> bool {
    let Some(before) = at.and_then(|at| original.as_bytes().get(..at)) else { return false };
    before.iter().rev().take_while(|&&b| b == b'\\').count() % 2 == 1
}

fn overlaps(a: &Range<usize>, b: &Range<usize>) -> bool {
    a.start < b.end && b.start < a.end
}

fn flag(raised: bool) -> InlineStyle {
    if raised { InlineStyle::SUPERSCRIPT } else { InlineStyle::SUBSCRIPT }
}
