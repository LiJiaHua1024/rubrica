//! The `==highlight==` marks of the Pandoc `mark` extension, read once the parser
//! is done with a block.
//!
//! pulldown-cmark has no such extension: `==` reaches a block as the two characters
//! the author typed, whatever they meant by them -- so the reading is not a gap in
//! the parser's own rules but a syntax it never had, and it is read here the way
//! [`crate::scripts`] reads `~` and `^`: from the finished text, with the marks
//! taken out of it, because a delimiter the reader has consumed must not still be
//! printed.
//!
//! The rules are the shape the mark has in every reader that has it, checked
//! against Pandoc's own parser (its `mark` rule reads `==`, a lookahead for a
//! non-space, and then words up to the next `==`) run by run:
//!
//! * a `==` run highlights the words up to the next `==`; the pair may touch a word
//!   on either side, so `中文==高亮==中文`, `x==y==z` and `x ==y== z` are read;
//! * what a run opens on must be a word, not a space -- `a == b` is a comparison,
//!   because the mark after the `==` is a space -- while nothing is asked of what
//!   stands before the run, which is why a mark may be written mid-sentence;
//! * the words end on ink and hold no `==` of their own: `==two words==` is a phrase
//!   with a space inside it, while `== a ==` and `==a ==` are the author's own runs
//!   of equals signs and stay as written, `==a == b==` is left alone because the
//!   middle run is neither of the pair, and in `==a ==b== c==` the last run is the
//!   author's too, since the pair before it was spent on `b`;
//! * an escaped `=` (`\==`) is the author's own character: it is printed and pairs
//!   with nothing, which is why the source, not the text, is asked;
//! * `===` and longer are printed as themselves: a run this reader has no reading
//!   for is left for the author to shorten;
//! * the marks may reach across emphasis and a link (`==**bold**==`), and a formula
//!   may sit inside the pair (`==$x$==`), but never inside code, where the equals
//!   signs are the author's to print.
//!
//! One deviation from Pandoc, on purpose: Pandoc cuts a `==` out of a longer run and
//! still highlights `====text====`, leaving the extra `=` printed on either side, and
//! such a run can also stand in a pair's way. Here every run but exactly two is
//! literal and takes no part in pairing, so `====text====`, `===text===` and
//! `==a====` keep their whole runs. The reading is meant to be the one a writer of
//! prose meant, and a run of four equals signs is far more likely to be a line drawn
//! by hand than a highlight with its edges doubled.
//!
//! A second, smaller difference follows from reading the finished text: a `==` that
//! Pandoc's scan would have swallowed inside another construct it parses as a unit --
//! the text of a link, of an autolink -- is a run here like any other, so a pair that
//! crosses such a construct can be drawn differently than in Pandoc. The characters
//! are still printed in the order written; only the pair's edges can move.

use std::ops::Range;

use crate::emphasis::{self, Text};
use crate::{source_at, InlineStyle, SourceSpan};

/// One `==` the text admits as a delimiter: exactly two, unescaped, outside code.
struct Run {
    range: Range<usize>,
}

/// Read the highlights an author wrote with `==` out of a block's text, and say how
/// many there were.
pub(crate) fn relax(t: &mut Text<'_>, original: &str) -> usize {
    if !t.text.contains("==") {
        return 0;
    }
    // A code span is where an author's equals signs are theirs to print, and a run
    // overlapping one is no marker at all -- the same rule every pass here keeps.
    let code: Vec<Range<usize>> = t
        .spans
        .iter()
        .filter(|s| s.style.contains(InlineStyle::CODE))
        .map(|s| s.range.clone())
        .collect();
    let runs: Vec<Run> = runs(t.text, t.sources, original, &code);
    if runs.len() < 2 {
        return 0;
    }
    // One open run at a time: a `==` closes the nearest `==` still open when the
    // words between them qualify, and otherwise opens one of its own. A run that
    // could neither open nor close -- the `==` of `a == b == c`, which a space
    // follows -- is printed, and a pair never reaches over or through it.
    let mut open: Vec<usize> = Vec::new();
    let mut pairs: Vec<(Range<usize>, Range<usize>)> = Vec::new();
    for (i, r) in runs.iter().enumerate() {
        // A run that could close does close -- it does not open as well, or the pair
        // would leave the reader two highlights for one set of marks. `==a==b==` is
        // a mark over `a` with the author's own `==` and `b==` after it, which is
        // what the reading here says.
        if let Some(&at) = open.last() {
            // Only the run right after the opener can close it: any run between the
            // two is a `==` of its own that was printed, and Pandoc, whose scan
            // stops at the first `==` it can see, refuses the same pair. So the
            // trailing run of `==a ==b== c==` closes nothing, because the pair in
            // the middle was already spent on `b`.
            if i == at + 1 && wraps(t.text, &(runs[at].range.end..r.range.start)) {
                pairs.push((runs[at].range.clone(), r.range.clone()));
                open.pop();
                continue;
            }
        }
        if opens(t.text, r) {
            open.push(i);
        }
    }
    if pairs.is_empty() {
        return 0;
    }
    // Styled before the text is cut, so the wrapped words are a span of their own by
    // the time the markers around them are gone.
    for (open, close) in &pairs {
        emphasis::style_range(t.spans, open.end..close.start, InlineStyle::HIGHLIGHT);
    }
    let mut cuts: Vec<Range<usize>> = pairs.iter().flat_map(|(o, c)| [o.clone(), c.clone()]).collect();
    cuts.sort_unstable_by_key(|r| r.start);
    cuts.dedup();
    emphasis::cut(t, &cuts);
    pairs.len()
}

/// Whether the words between two marks can carry a highlight: a phrase that ends on
/// ink, with something for the reader to see.
///
/// The test is on the character before the closing run, and it is the one Pandoc
/// applies: `==two words==` is a phrase with a space inside it, while `==a ==` and
/// `== a ==` are the author's own runs of equals signs and stay as written. What
/// stands before the *opening* run is not asked -- a phrase may begin mid-sentence
/// (`x ==a== y`) -- and the other side of that rule is [`opens`], the same test for
/// the phrase's first character.
fn wraps(text: &str, content: &Range<usize>) -> bool {
    !content.is_empty() && !text[content.clone()].chars().next_back().is_some_and(char::is_whitespace)
}

/// Whether the run may open a highlight here: a word follows it.
///
/// The test is Pandoc's own, and it reads the character *after* the run: a mark
/// begins where `==` stands in front of something other than a space, so `a == b`
/// and an unpaired `==` at the end of a line begin nothing. A longer run is stopped
/// before this question is asked, so the `=` Pandoc also refuses there is already
/// gone.
fn opens(text: &str, r: &Run) -> bool {
    text[r.range.end..].chars().next().is_some_and(|c| !c.is_whitespace())
}

/// Every `==` the text admits, and every stretch of three or more that is not one.
///
/// A stretch is split where the source's backslashes change its meaning: in `\==`
/// the author escaped one equals sign and wrote one, so the readable run is one
/// character and not two, and only the source can say so -- by the time the text is
/// here the backslash is gone.
fn runs(text: &str, sources: &[SourceSpan], original: &str, code: &[Range<usize>]) -> Vec<Run> {
    let mut out = Vec::new();
    let bytes = text.as_bytes();
    let mut at = 0usize;
    while at < bytes.len() {
        if bytes[at] != b'=' {
            at += 1;
            continue;
        }
        let mut end = at;
        while end < bytes.len() && bytes[end] == b'=' {
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
                // reading at all; only an unescaped run of exactly two is a marker.
                if !escaped && range.len() == 2 && !code.iter().any(|c| overlaps(c, &range)) {
                    out.push(Run { range });
                }
                start = i;
                escaped = this;
            }
            i += 1;
        }
        at = end;
    }
    out
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
