//! Finding a phrase in the page as it was laid out.
//!
//! The search runs over the characters that were *painted* rather than over the
//! document's source, because those are the ones the reader can see, click and copy: a hit
//! answers with the same pair of places a drag makes, so highlighting it, scrolling to it
//! and copying it are the one arithmetic rather than three that have to agree.
//!
//! It follows that a search cannot find what the page has no ink for -- a formula's source
//! is one placeholder here, and a table's cells are read in the order they were drawn --
//! which is the right answer for a reader looking at a rendered page.

use crate::view::{Caret, Join, SelLine, Selection};

/// The page's characters in reading order, lower-cased, with where each line of them
/// began.
///
/// A line's own characters are contiguous here, in the order they were painted, and what
/// separates two lines is the same text a copy hands back between them. That is what makes
/// a phrase survive its own wrapping: the space a break swallowed is still a character of
/// the line that lost it, given no width, so the words either side of it stand together
/// exactly as they read on the page.
#[derive(Default)]
pub struct Needle {
    chars: Vec<char>,
    /// Where each line's characters begin in `chars`. One number per line, and a line is
    /// found by searching them rather than by storing a position against every character,
    /// because a long document is a lot of characters and a page can be open in a reader
    /// that is meant to hold it in a few megabytes.
    starts: Vec<u32>,
    /// How many characters each line has, which is what clamps a position that fell in the
    /// gap between two lines back to the end of the one before it.
    lens: Vec<u32>,
}

impl Needle {
    pub fn of(sel: &[SelLine]) -> Needle {
        let mut n = Needle::default();
        for (line, l) in sel.iter().enumerate() {
            // The first line's join describes nothing: there is no line before it.
            if line > 0 {
                n.chars.extend(separator(l.join).chars());
            }
            n.starts.push(n.chars.len() as u32);
            n.lens.push(l.chars.len() as u32);
            n.chars.extend(l.chars.iter().copied().map(fold));
        }
        n
    }

    /// Every place the query's characters stand, in reading order, as positions in this
    /// needle's own text.
    pub fn hits(&self, query: &str) -> Vec<std::ops::Range<usize>> {
        let needle: Vec<char> = query.chars().map(fold).collect();
        // A reader who has typed nothing yet has asked for nothing, and a query longer
        // than the page cannot stand anywhere on it -- the second is also the guard that
        // keeps the loop's bounds below.
        if needle.is_empty() || needle.len() > self.chars.len() {
            return Vec::new();
        }
        let mut out = Vec::new();
        'word: for start in 0..=self.chars.len() - needle.len() {
            for (n, c) in needle.iter().enumerate() {
                if self.chars[start + n] != *c {
                    continue 'word;
                }
            }
            out.push(start..start + needle.len());
        }
        out
    }

    /// The two places a drag would have to mark to cover a hit's ink.
    ///
    /// `None` for a range that names nothing, which is the only way a stale hit could
    /// arrive here from a page that has since been laid out again.
    pub fn span(&self, hit: &std::ops::Range<usize>) -> Option<Selection> {
        let from = self.caret(hit.start)?;
        let last = self.caret(hit.end.checked_sub(1)?)?;
        Some(Selection { from, to: Caret { line: last.line, ch: last.ch + 1 } })
    }

    /// The start of the line holding a position, which is where a hit begins as far as the
    /// reader's eye is concerned.
    fn caret(&self, at: usize) -> Option<Caret> {
        let at = at as u32;
        // The last line beginning at or before `at`.
        let line = self.starts.partition_point(|s| *s <= at).checked_sub(1)?;
        let (start, len) = (self.starts[line], self.lens[line]);
        // A position in the gap after a line's own characters is one of the separators,
        // and the end of the line is as much of it as there is to mark.
        let ch = at.saturating_sub(start).min(len) as usize;
        Some(Caret { line, ch })
    }
}

/// What a line's [`Join`] puts between it and the one before, in a copy. Kept as the
/// characters rather than as a rule of its own so that a hit and a copy of the same words
/// cannot disagree about what stands between them.
fn separator(join: Join) -> &'static str {
    match join {
        Join::None => "",
        Join::Tab => "\t",
        Join::Newline => "\n",
        Join::Blank => "\n\n",
    }
}

/// A character as a search sees it.
///
/// Case is given up because a reader is not thinking about it while they type, and a
/// document that spells a word two ways deserves to be found either way. Where
/// [`char::to_lowercase`] has more than one to give back, the extras are dropped rather
/// than searched for: a position here is one painted character, and folding one glyph into
/// two would point every hit after it at the wrong ink.
fn fold(c: char) -> char {
    c.to_lowercase().next().unwrap_or(c)
}

#[cfg(test)]
mod tests {
    use super::*;

    const CHAR: f32 = 10.0;

    /// A painted line. `join` is what stands between it and the line above.
    fn line(text: &str, y: f32, join: Join) -> SelLine {
        let chars: Vec<char> = text.chars().collect();
        SelLine {
            y,
            h: CHAR * 1.5,
            join,
            chars,
            xs: (0..=text.chars().count()).map(|i| i as f32 * CHAR).collect(),
            ends: (1..=text.chars().count()).map(|i| i as f32 * CHAR).collect(),
        }
    }

    fn needle(lines: &[SelLine]) -> Needle {
        Needle::of(lines)
    }

    fn spans(lines: &[SelLine], query: &str) -> Vec<Selection> {
        let n = needle(lines);
        n.hits(query).iter().filter_map(|h| n.span(h)).collect()
    }

    #[test]
    fn a_phrase_finds_itself_whatever_the_line_it_wrapped_on() {
        // The whole reason the search is over the page's own text: a reader copies a
        // phrase out of one paragraph and asks the page where it is, and the paragraph's
        // lines are a fact about the window's width rather than about the words.
        let lines = [line("the queue is ", 0.0, Join::None), line("empty today", 20.0, Join::None)];
        assert_eq!(spans(&lines, "queue is empty").len(), 1);
        let s = spans(&lines, "queue is empty").remove(0);
        assert_eq!((s.from.line, s.from.ch), (0, 4));
        assert_eq!((s.to.line, s.to.ch), (1, 5));
    }

    #[test]
    fn a_search_does_not_care_who_shifted() {
        let lines = [line("Kerning 和 Rubrica", 0.0, Join::None)];
        assert_eq!(needle(&lines).hits("KERNING").len(), 1);
        assert_eq!(needle(&lines).hits("rubrica").len(), 1);
        // The Han characters have no case to give up, and none to lose either.
        assert_eq!(needle(&lines).hits("和").len(), 1);
    }

    #[test]
    fn two_paragraphs_are_not_one_phrase() {
        let lines = [line("one", 0.0, Join::None), line("two", 20.0, Join::Blank)];
        assert_eq!(needle(&lines).hits("onetwo").len(), 0, "a blank line is not nothing");
        assert_eq!(needle(&lines).hits("one\n\ntwo").len(), 1, "and it is the two the copy gives back");
        assert_eq!(needle(&lines).hits("one").len(), 1);
        assert_eq!(needle(&lines).hits("two").len(), 1);
    }

    #[test]
    fn every_place_it_stands_is_a_hit() {
        let lines = [line("ababab", 0.0, Join::None)];
        assert_eq!(needle(&lines).hits("aba").len(), 2, "starts one apart are still two starts");
    }

    #[test]
    fn nothing_typed_is_nothing_to_mark() {
        let lines = [line("some words", 0.0, Join::None)];
        assert!(needle(&lines).hits("").is_empty());
        assert!(needle(&lines).hits("words but longer than the page").is_empty());
        assert!(Needle::of(&[]).hits("some").is_empty(), "an empty page answers as quietly");
    }

    #[test]
    fn a_hit_points_at_the_line_it_was_painted_on() {
        let lines = [
            line("first line", 0.0, Join::None),
            line("second line", 20.0, Join::Newline),
            line("third line", 40.0, Join::Newline),
        ];
        let s = spans(&lines, "third").remove(0);
        assert_eq!(s.from.line, 2, "the second line's newline is not the third line's first word");
        assert_eq!((s.from.ch, s.to.ch), (0, 5));
        assert_eq!(spans(&lines, "second").remove(0).from.line, 1);
    }
}
