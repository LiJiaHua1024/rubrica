//! Text statistics for a parsed document: characters, words, paragraphs.
//!
//! Counted from the *displayed* text -- each [`Block::text`] and each table cell --
//! rather than from the raw source, so a Markdown marker is not counted as prose. The
//! word rule is the one a mixed-script paragraph needs: every CJK character is a word,
//! and a run of Latin letters or digits is one word, so `中文字` is three words and
//! `hello world` is two. Script classification is [`rubrica_type::classify::Role`], the
//! same table the line breaker uses, so the two never drift apart.

use crate::{Block, BlockKind, Document};
use rubrica_type::classify::Role;

/// What a status bar can say about the text it is showing.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TextCounts {
    /// Non-whitespace characters of the displayed text.
    pub characters: usize,
    /// CJK characters, each counted as its own word, plus runs of Latin or digits.
    pub words: usize,
    /// Blocks carrying visible text: paragraphs, headings, list items, quotes, code,
    /// terms, and tables with any non-empty cell.
    pub paragraphs: usize,
    /// CJK characters alone, kept apart from `words` so a reader's pace can be
    /// estimated from the two scripts separately.
    pub cjk_characters: usize,
    /// Runs of Latin letters or digits, the other half of `words`.
    pub latin_words: usize,
}

impl Document {
    /// Count the text this document displays.
    pub fn counts(&self) -> TextCounts {
        let mut counts = TextCounts::default();
        for block in &self.blocks {
            counts.add_block(block);
        }
        for note in &self.footnotes {
            for block in &note.blocks {
                counts.add_block(block);
            }
        }
        counts
    }
}

impl TextCounts {
    fn add_block(&mut self, block: &Block) {
        match block.kind {
            // A rule is a line, not a word; the counters should not see it.
            BlockKind::Rule => {}
            BlockKind::Table => {
                let Some(table) = block.table.as_ref() else { return };
                let mut any = false;
                for cell in table.head.iter().chain(table.rows.iter().flatten()) {
                    if !cell.text.trim().is_empty() {
                        any = true;
                    }
                    self.add_text(&cell.text);
                }
                if any {
                    self.paragraphs += 1;
                }
            }
            _ => {
                if !block.text.trim().is_empty() {
                    self.paragraphs += 1;
                }
                self.add_text(&block.text);
            }
        }
    }

    fn add_text(&mut self, text: &str) {
        let mut in_word = false;
        for ch in text.chars() {
            // An inline object is a placeholder for an image, not a character of prose.
            if ch.is_whitespace() || ch == '\u{FFFC}' {
                in_word = false;
                continue;
            }
            self.characters += 1;
            if !ch.is_alphanumeric() {
                in_word = false;
                continue;
            }
            if Role::of(ch) == Role::Cjk {
                self.cjk_characters += 1;
                self.words += 1;
                in_word = false;
            } else if !in_word {
                self.latin_words += 1;
                self.words += 1;
                in_word = true;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Document;

    fn counts(source: &str) -> TextCounts {
        Document::parse(source).counts()
    }

    #[test]
    fn han_characters_are_counted_one_word_each() {
        let c = counts("中文字");
        assert_eq!(c.characters, 3);
        assert_eq!(c.words, 3);
        assert_eq!(c.cjk_characters, 3);
        assert_eq!(c.latin_words, 0);
        assert_eq!(c.paragraphs, 1);
    }

    #[test]
    fn a_latin_run_is_one_word() {
        let c = counts("hello world");
        assert_eq!(c.characters, 10);
        assert_eq!(c.words, 2);
        assert_eq!(c.latin_words, 2);
    }

    #[test]
    fn whitespace_and_punctuation_are_not_characters_or_words() {
        let c = counts("one, two.   three\n");
        // Commas and periods are ignored as words but are still characters.
        assert_eq!(c.words, 3);
        assert_eq!(c.characters, "one,two.three".len());
    }

    #[test]
    fn mixed_scripts_split_at_the_boundary() {
        let c = counts("我用 Rust 写代码");
        assert_eq!(c.cjk_characters, 5);
        assert_eq!(c.latin_words, 1);
        assert_eq!(c.words, 6);
    }

    #[test]
    fn markdown_markers_are_not_counted_as_prose() {
        // Bold markers never reach the displayed text; the two words do.
        let c = counts("**bold** text");
        assert_eq!(c.words, 2);
        assert_eq!(c.characters, "boldtext".len());
    }

    #[test]
    fn code_and_headings_carry_text() {
        let c = counts("# Title\n\n```\nfn main() {}\n```\n");
        assert_eq!(c.paragraphs, 2);
        // "Title", then the code's "fn" and "main" -- the braces are punctuation.
        assert_eq!(c.words, 3);
    }

    #[test]
    fn a_rule_is_not_a_paragraph() {
        let c = counts("a\n\n---\n\nb\n");
        assert_eq!(c.paragraphs, 2);
    }

    #[test]
    fn table_cells_are_counted_and_the_table_is_one_paragraph() {
        let c = counts("| a | b |\n|---|---|\n| c | d |\n");
        assert_eq!(c.paragraphs, 1);
        assert_eq!(c.words, 4);
    }
}
