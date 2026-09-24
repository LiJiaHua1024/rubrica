//! Plain-text books retain punctuation and Markdown markers literally. Chapter byte
//! ranges let the reader lay out a small window without parsing the whole book again.

use std::io::{self, BufRead, BufReader, Read, Seek, SeekFrom};
use std::ops::Range;
use std::path::Path;
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

/// A cheap, reusable index of chapter boundaries in a decoded TXT document.
///
/// The index stores only byte ranges and titles. Building it scans line boundaries;
/// laying out a window then parses just that range, while source positions remain
/// relative to the complete book rather than to the slice handed to the layout code.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChapterIndex {
    chapters: Vec<Chapter>,
    blank_separated: bool,
}

impl ChapterIndex {
    pub fn new(source: &str, detect: bool) -> Self {
        Self {
            chapters: chapters(source, detect),
            blank_separated: source.lines().any(|line| line.trim().is_empty()),
        }
    }

    /// Build a chapter index by scanning a UTF-8 file line by line, without first
    /// materialising the complete book in memory. Legacy encodings use the decoded-string
    /// constructor because their code-page state cannot safely resume at an arbitrary byte
    /// offset.
    pub fn from_path(path: &Path, detect: bool) -> io::Result<Self> {
        let file = std::fs::File::open(path)?;
        let length = file.metadata()?.len() as usize;
        let mut reader = BufReader::new(file);
        let mut chapters: Vec<Chapter> = Vec::new();
        let mut offset = 0usize;
        let mut blank_separated = false;
        let mut first = true;
        loop {
            let mut line = Vec::new();
            let read = reader.read_until(b'\n', &mut line)?;
            if read == 0 { break; }
            let mut text = std::str::from_utf8(&line)
                .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "TXT window requires UTF-8"))?;
            if first {
                text = text.strip_prefix('\u{feff}').unwrap_or(text);
                first = false;
            }
            if text.trim().is_empty() { blank_separated = true; }
            if detect && is_chapter(text) {
                if let Some(last) = chapters.last_mut() {
                    last.range.end = offset;
                } else if offset > 0 {
                    chapters.push(Chapter { title: "Beginning".into(), range: 0..offset });
                }
                chapters.push(Chapter { title: text.trim().into(), range: offset..length });
            }
            offset += read;
        }
        if chapters.is_empty() {
            chapters.push(Chapter { title: "Beginning".into(), range: 0..length });
        } else if let Some(last) = chapters.last_mut() {
            last.range.end = length;
        }
        Ok(Self { chapters, blank_separated })
    }

    pub fn chapters(&self) -> &[Chapter] {
        &self.chapters
    }

    pub fn chapter_at(&self, source_byte: usize) -> usize {
        self.chapters
            .iter()
            .rposition(|chapter| chapter.range.start <= source_byte)
            .unwrap_or(0)
    }

    pub fn read_window(&self, path: &Path, index: usize, options: TextOptions) -> io::Result<Document> {
        let range = self.chapters.get(index).map(|chapter| chapter.range.clone())
            .unwrap_or(0..0);
        let mut file = std::fs::File::open(path)?;
        file.seek(SeekFrom::Start(range.start as u64))?;
        let mut bytes = vec![0; range.end.saturating_sub(range.start)];
        file.read_exact(&mut bytes)?;
        let source = std::str::from_utf8(&bytes)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "TXT window is not UTF-8"))?;
        Ok(parse_range(source, 0..source.len(), options, self.blank_separated, range.start))
    }

    pub fn read_source_window(&self, path: &Path, index: usize) -> io::Result<String> {
        let range = self.chapters.get(index).map(|chapter| chapter.range.clone())
            .unwrap_or(0..0);
        let mut file = std::fs::File::open(path)?;
        file.seek(SeekFrom::Start(range.start as u64))?;
        let mut bytes = vec![0; range.end.saturating_sub(range.start)];
        file.read_exact(&mut bytes)?;
        Ok(std::str::from_utf8(&bytes)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "TXT window is not UTF-8"))?
            .to_string())
    }

    pub fn window(&self, source: &str, index: usize, options: TextOptions) -> Document {
        let range = self
            .chapters
            .get(index)
            .map(|chapter| chapter.range.clone())
            .unwrap_or(0..source.len());
        parse_window(source, range, options)
    }
}

/// Parse only one source window while preserving byte positions in the full source.
///
/// `range` must lie on UTF-8 character boundaries. Chapter ranges produced by
/// [`chapters`] always do; callers with arbitrary offsets get a safe empty document
/// rather than a panic in the middle of a book.
pub fn parse_window(source: &str, range: Range<usize>, options: TextOptions) -> Document {
    let start = range.start.min(source.len());
    let end = range.end.clamp(start, source.len());
    if !source.is_char_boundary(start) || !source.is_char_boundary(end) {
        return Document { blocks: Vec::new(), footnotes: Vec::new() };
    }
    parse_range(source, start..end, options, source.lines().any(|line| line.trim().is_empty()), start)
}

fn parse_range(
    source: &str,
    range: Range<usize>,
    options: TextOptions,
    blank_separated: bool,
    source_base: usize,
) -> Document {
    // Short lines in a book with blank paragraph separators are likely hard-wrapped.
    // With no blank separators each source line is a paragraph, as in novel TXT files.
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
    let mut offset = 0usize;
    for raw in source[range.clone()].split_inclusive('\n') {
        let line_start = source_base + offset;
        let content_start = source_base + offset + raw.len() - raw.trim_start().len();
        offset += raw.len();
        let line = raw.trim();
        if line.is_empty() {
            flush(&mut paragraph, &mut sources, &mut blocks);
            continue;
        }
        if options.chapters && is_chapter(line) {
            flush(&mut paragraph, &mut sources, &mut blocks);
            let mut block = Block::literal(BlockKind::Heading(1), line.into());
            if let Some(first) = block.sources.first_mut() {
                first.source = content_start;
            }
            blocks.push(block);
        } else {
            if !paragraph.is_empty() {
                let cjk = |c: char| matches!(c as u32, 0x3000..=0x9fff | 0xff00..=0xffef);
                if !paragraph.chars().last().is_some_and(cjk) || !line.chars().next().is_some_and(cjk) {
                    sources.push(SourceSpan {
                        range: paragraph.len()..paragraph.len() + 1,
                        source: line_start.saturating_sub(1),
                    });
                    paragraph.push(' ');
                }
            }
            sources.push(SourceSpan {
                range: paragraph.len()..paragraph.len() + line.len(),
                source: content_start,
            });
            paragraph.push_str(line);
            if !merge {
                flush(&mut paragraph, &mut sources, &mut blocks);
            }
        }
    }
    flush(&mut paragraph, &mut sources, &mut blocks);
    Document { blocks, footnotes: Vec::new() }
}

pub fn parse(source: &str, options: TextOptions) -> Document {
    parse_window(source, 0..source.len(), options)
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
    fn a_chapter_window_keeps_global_source_positions() {
        let src = "前言\n开头\n\n第一章\n正文\n换行\n\n第二章\n结尾";
        let index = ChapterIndex::new(src, true);
        assert_eq!(index.chapters().len(), 3);
        let chapter = index.chapters()[1].clone();
        let doc = index.window(src, 1, TextOptions { paragraphs: ParagraphRule::Lines, chapters: true });
        assert_eq!(doc.blocks[0].text, "第一章");
        assert_eq!(doc.blocks[0].sources[0].source, chapter.range.start);
        assert_eq!(doc.blocks[1].text, "正文");
        assert_eq!(doc.blocks[2].text, "换行");
        assert!(doc.blocks.iter().all(|block| block.text != "结尾"));
    }

    #[test]
    fn automatic_paragraphs_use_the_whole_book_not_only_the_window() {
        let src = "序言\n第一行\n第二行\n\n第一章\n窗内第一行\n窗内第二行";
        let index = ChapterIndex::new(src, true);
        let doc = index.window(src, 1, TextOptions::default());
        assert_eq!(doc.blocks[1].text, "窗内第一行窗内第二行");
    }

    #[test]
    fn a_utf8_file_window_reads_only_the_requested_chapter() {
        let path = std::env::temp_dir().join(format!("rubrica-plain-window-{}.txt", std::process::id()));
        let source = "序言\n开头\n\n第一章\n正文\n换行\n\n第二章\n结尾";
        std::fs::write(&path, source).expect("write text fixture");
        let index = ChapterIndex::from_path(&path, true).expect("index file");
        assert_eq!(index.chapters().len(), 3);
        let doc = index.read_window(&path, 1, TextOptions { paragraphs: ParagraphRule::Lines, chapters: true }).expect("read window");
        assert_eq!(doc.blocks[0].text, "第一章");
        assert_eq!(doc.blocks[0].sources[0].source, index.chapters()[1].range.start);
        assert!(doc.blocks.iter().all(|block| block.text != "结尾"));
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn an_invalid_window_is_empty_instead_of_panicking() {
        let doc = parse_window("中文", 1..2, TextOptions::default());
        assert!(doc.blocks.is_empty());
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
