//! Plain-text books retain punctuation and Markdown markers literally. Chapter byte
//! ranges let the reader lay out a small window without parsing the whole book again.

use std::ops::Range;
use crate::{Block, BlockKind, Document, SourceSpan};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ParagraphRule {
    #[default]
    Auto,
    Lines,
    BlankLines,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TextOptions {
    pub paragraphs: ParagraphRule,
    pub chapters: bool,
}

impl Default for TextOptions {
    fn default() -> Self { Self { paragraphs: ParagraphRule::Auto, chapters: true } }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Chapter {
    pub title: String,
    pub range: Range<usize>,
}

/// Recognize standalone book headings, without interpreting Markdown syntax.
pub fn is_chapter(line: &str) -> bool {
    let line = line.trim();
    if line.is_empty() || line.chars().count() > 100 { return false; }
    if matches!(line, "序章" | "序言" | "前言" | "后记" | "尾声" | "楔子" | "终章") { return true; }
    if let Some(rest) = line.strip_prefix('第') {
        let number = |c: char| c.is_ascii_digit() || "零〇一二三四五六七八九十百千万两壹贰叁肆伍陆柒捌玖拾佰仟".contains(c);
        let count = rest.chars().take_while(|c| number(*c)).count();
        if count > 0 && rest.chars().nth(count).is_some_and(|c| "章回节卷部篇".contains(c)) {
            return true;
        }
    }
    let lower = line.to_ascii_lowercase();
    if matches!(lower.as_str(), "prologue" | "epilogue" | "preface" | "introduction") { return true; }
    for prefix in ["chapter ", "part ", "book "] {
        if let Some(rest) = lower.strip_prefix(prefix) {
            let token = rest.split_whitespace().next().unwrap_or("").trim_end_matches(['.', ':']);
            if !token.is_empty() && (token.chars().all(|c| c.is_ascii_digit())
                || token.chars().all(|c| "ivxlcdm".contains(c))) { return true; }
        }
    }
    false
}

pub fn chapters(source: &str, detect: bool) -> Vec<Chapter> {
    let mut out: Vec<Chapter> = Vec::new();
    let mut offset = 0;
    if detect {
        for line in source.split_inclusive('\n') {
            if is_chapter(line) {
                if let Some(last) = out.last_mut() { last.range.end = offset; }
                else if offset > 0 { out.push(Chapter { title: "Beginning".into(), range: 0..offset }); }
                out.push(Chapter { title: line.trim().into(), range: offset..source.len() });
            }
            offset += line.len();
        }
    }
    if out.is_empty() { out.push(Chapter { title: "Beginning".into(), range: 0..source.len() }); }
    out
}

pub fn parse(source: &str, options: TextOptions) -> Document {
    // Short lines in a book with blank paragraph separators are likely hard-wrapped.
    // With no blank separators each source line is a paragraph, as in novel TXT files.
    let blank_separated = source.lines().any(|l| l.trim().is_empty());
    let merge = match options.paragraphs {
        ParagraphRule::Auto => blank_separated,
        ParagraphRule::Lines => false,
        ParagraphRule::BlankLines => true,
    };
    let mut blocks = Vec::new();
    let mut paragraph = String::new();
    let mut sources = Vec::new();
    let flush = |text: &mut String, sources: &mut Vec<SourceSpan>, blocks: &mut Vec<Block>| {
        if !text.is_empty() {
            let mut block = Block::literal(BlockKind::Paragraph, std::mem::take(text));
            block.sources = std::mem::take(sources);
            blocks.push(block);
        }
    };
    let mut offset = 0;
    for raw in source.split_inclusive('\n') {
        let start = offset + raw.len() - raw.trim_start().len();
        offset += raw.len();
        let line = raw.trim();
        if line.is_empty() { flush(&mut paragraph, &mut sources, &mut blocks); continue; }
        if options.chapters && is_chapter(line) {
            flush(&mut paragraph, &mut sources, &mut blocks);
            let mut block = Block::literal(BlockKind::Heading(1), line.into());
            block.sources[0].source = start;
            blocks.push(block);
        } else {
            if !paragraph.is_empty() {
                let cjk = |c: char| matches!(c as u32, 0x3000..=0x9fff | 0xff00..=0xffef);
                if !paragraph.chars().last().is_some_and(cjk) || !line.chars().next().is_some_and(cjk) {
                    sources.push(SourceSpan { range: paragraph.len()..paragraph.len() + 1, source: start.saturating_sub(1) });
                    paragraph.push(' ');
                }
            }
            sources.push(SourceSpan { range: paragraph.len()..paragraph.len() + line.len(), source: start });
            paragraph.push_str(line);
            if !merge { flush(&mut paragraph, &mut sources, &mut blocks); }
        }
    }
    flush(&mut paragraph, &mut sources, &mut blocks);
    Document { blocks, footnotes: Vec::new() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn txt_markers_are_literal_and_chapters_have_byte_ranges() {
        let src = "前言\n# not Markdown\n* stars * and $money$\n\n第二章 开始\n1. literal list\n\nChapter IV: Return\nend";
        let parts = chapters(src, true);
        assert_eq!(parts.len(), 3);
        assert_eq!(parts.iter().map(|c| &src[c.range.clone()]).collect::<String>(), src);
        let doc = parse(src, TextOptions { paragraphs: ParagraphRule::Lines, chapters: true });
        assert_eq!(doc.blocks[0].kind, BlockKind::Heading(1));
        assert_eq!(doc.blocks[1].text, "# not Markdown");
        assert!(doc.blocks.iter().all(|b| b.objects.is_empty() && b.list.is_none()));
        assert_eq!(doc.blocks.iter().filter(|b| matches!(b.kind, BlockKind::Heading(_))).count(), 3);
    }

    #[test]
    fn paragraph_rules_join_wrapped_prose_and_keep_chapters_separate() {
        let src = "第一章\n中文换行\n接在一起\n\nEnglish wraps\non a space\n";
        let doc = parse(src, TextOptions::default());
        assert_eq!(doc.blocks[1].text, "中文换行接在一起");
        assert_eq!(doc.blocks[2].text, "English wraps on a space");
        let doc = parse(src, TextOptions { paragraphs: ParagraphRule::Lines, chapters: false });
        assert_eq!(doc.blocks.len(), 5);
        assert!(doc.blocks.iter().all(|b| b.kind == BlockKind::Paragraph));
        assert!(!is_chapter("Chapter about recipes is not a chapter number."));
        assert!(!is_chapter("第一个人走来"));
    }
}
