//! English hyphenation points, from Knuth-Liang patterns.
//!
//! The solver can only use a break the segmenter offers it, and UAX #14 will not
//! break inside a word. Without a dictionary, an English paragraph in a narrow
//! measure has no legal set of breaks at all and falls through to the
//! tolerance-free pass -- which is the "narrow columns look wrong" symptom this
//! module exists to remove.
//!
//! Offsets, not rewritten text: the crate will happily insert characters into the
//! string it is given, but the layout core addresses nodes by byte range into the
//! block's text, so a rewritten word would invalidate every span downstream. This
//! returns the byte offsets to offer, the only form that composes.

use std::sync::OnceLock;

use hyphenation::{Hyphenator as _, Language, Load, Standard};

/// A word shorter than this is not worth splitting; the crate's own edge rules
/// (keep two characters at the start, three at the end) apply on top.
const MIN_WORD: usize = 6;

/// The one dictionary the process builds, on first use.
///
/// Constructing a `Standard` deserializes the embedded pattern file into a trie,
/// which the startup path and every async relayout were each paying for. Built
/// once here; a machine that shipped without the embedded dictionary keeps the
/// `None`, so a load that cannot succeed is not retried on every call.
static SHARED: OnceLock<Option<Hyphenator>> = OnceLock::new();

pub struct Hyphenator {
    inner: Standard,
}

/// The process-wide hyphenator, built from [`Hyphenator::english`] on first use.
/// Callers that can hold a reference should prefer this over building their own,
/// which pays for the whole dictionary again.
pub fn shared() -> Option<&'static Hyphenator> {
    SHARED.get_or_init(Hyphenator::english).as_ref()
}

impl Hyphenator {
    /// The compiled-in American dictionary. `None` if it was not embedded at
    /// build time, in which case the reader simply stops offering word breaks.
    pub fn english() -> Option<Hyphenator> {
        Standard::from_embedded(Language::EnglishUS).ok().map(|inner| Hyphenator { inner })
    }

    /// Byte offsets, relative to `text`, at which a word may be split.
    ///
    /// Only ASCII-letter runs are considered. A Latin word sitting between CJK
    /// characters is still a Latin word, so no script awareness is needed here.
    pub fn points(&self, text: &str) -> Vec<usize> {
        let mut out = Vec::new();
        let mut word = String::new();
        let mut start = 0usize;
        for (i, c) in text.char_indices() {
            // An apostrophe continues the word ("don't"), but never opens it.
            let keeps_going = c.is_ascii_alphabetic() || (c == '\'' && !word.is_empty());
            if keeps_going {
                if word.is_empty() {
                    start = i;
                }
                word.push(c);
            } else if !word.is_empty() {
                self.append(&word, start, &mut out);
                word.clear();
            }
        }
        if !word.is_empty() {
            self.append(&word, start, &mut out);
        }
        out.sort_unstable();
        out.dedup();
        out
    }

    fn append(&self, word: &str, base: usize, out: &mut Vec<usize>) {
        if word.len() < MIN_WORD {
            return;
        }
        // Segments come back as syllables with the break character attached, e.g.
        // ["an", "frac", "tu", "ous"] for the hyphenless form.
        let segs: Vec<&str> = self.inner.hyphenate(word).into_iter().segments().collect();
        if segs.len() < 2 {
            return;
        }
        let mut consumed = 0usize;
        for (n, s) in segs.iter().enumerate() {
            let piece = s.trim_end_matches(['-', '\u{AD}']);
            consumed += piece.len();
            // No break after the final syllable, and never one that would leave a
            // fragment the dictionary's own edge rules already excluded.
            if n + 1 < segs.len() {
                out.push(base + consumed);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hy() -> Hyphenator {
        Hyphenator::english().expect("the embedded en-us dictionary should be available")
    }

    #[test]
    fn a_long_word_yields_offsets_inside_it() {
        let h = hy();
        // "hyphenation" is 11 characters; the dictionary offers points inside it.
        let pts = h.points("hyphenation");
        // TeX's default edge rules keep two characters before a break and three
        // after, which is what these offsets respect.
        assert_eq!(pts, vec![2, 6, 7], "hy-phen-at-ion");
        for p in &pts {
            assert!(*p >= 2 && *p + 3 <= "hyphenation".len(), "break too close to an edge: {p}");
        }
    }

    #[test]
    fn offsets_are_relative_to_the_text_not_to_the_word() {
        let h = hy();
        let bare = h.points("hyphenation");
        let prefixed = h.points("the hyphenation");
        let shift = "the ".len();
        assert_eq!(
            prefixed,
            bare.iter().map(|p| p + shift).collect::<Vec<_>>(),
            "offsets not shifted by the preceding text: {bare:?} vs {prefixed:?}"
        );
    }

    #[test]
    fn short_words_are_never_split() {
        let h = hy();
        for w in ["the", "and", "word", "go", "a", "short"] {
            assert!(h.points(w).is_empty(), "{w} should not be hyphenated");
        }
    }

    #[test]
    fn punctuation_and_cjk_do_not_produce_words() {
        let h = hy();
        assert!(h.points("").is_empty());
        assert!(h.points("，。——").is_empty());
        assert!(h.points("中文中文中文").is_empty());
        // A long Latin word between ideographs is still hyphenated.
        assert!(!h.points("中文hyphenation中文").is_empty());
    }

    #[test]
    fn an_apostrophe_stays_inside_the_word() {
        let h = hy();
        // "unbreakable" would split; a trailing apostrophe must not create a
        // separate one-character word.
        let pts = h.points("don't hyphenation");
        let shift = "don't ".len();
        assert_eq!(pts, vec![shift + 2, shift + 6, shift + 7], "{pts:?} leaked into the contraction");
    }
}
