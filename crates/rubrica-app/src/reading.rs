//! Byte decoding and neighboring documents, independent of the window and typography.

use std::io::{self, BufRead, BufReader, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use rubrica_doc::plain::{Chapter, ChapterIndex, ParagraphRule, TextOptions, is_chapter};
use windows::Win32::Globalization::{MultiByteToWideChar, MB_ERR_INVALID_CHARS, MULTI_BYTE_TO_WIDE_CHAR_FLAGS};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Encoding {
    #[default]
    Auto,
    Utf8,
    Utf16Le,
    Utf16Be,
    Gb18030,
    Big5,
    ShiftJis,
    EucKr,
}

impl Encoding {
    pub const ALL: [Self; 8] = [Self::Auto, Self::Utf8, Self::Utf16Le, Self::Utf16Be,
        Self::Gb18030, Self::Big5, Self::ShiftJis, Self::EucKr];

    pub fn label(self) -> &'static str {
        match self {
            Self::Auto => "Automatic", Self::Utf8 => "UTF-8", Self::Utf16Le => "UTF-16 LE",
            Self::Utf16Be => "UTF-16 BE", Self::Gb18030 => "GB18030", Self::Big5 => "Big5",
            Self::ShiftJis => "Shift JIS", Self::EucKr => "EUC-KR",
        }
    }
}

pub struct Decoded {
    pub text: String,
    pub encoding: Encoding,
    pub guessed: bool,
}

/// Above this size a plain UTF-8 book may be indexed and read by chapter instead of
/// decoding the whole file before the first page is laid out.
pub const LAZY_TEXT_THRESHOLD: u64 = 4 * 1024 * 1024;

pub fn is_large_text(path: &Path) -> bool {
    std::fs::metadata(path).map(|meta| meta.len() > LAZY_TEXT_THRESHOLD).unwrap_or(false)
}

/// Probe whether a large text file may use chapter windows. Automatic mode is allowed
/// to try the incremental scanner: it will fall back to the complete decode only if the
/// line-by-line decoder cannot establish a valid encoding.
pub fn can_window_text(path: &Path, requested: Encoding) -> bool {
    if !is_large_text(path) {
        return false;
    }
    if requested == Encoding::Auto
        || matches!(requested, Encoding::Gb18030 | Encoding::Big5 | Encoding::ShiftJis | Encoding::EucKr)
    {
        return true;
    }
    if requested != Encoding::Utf8 {
        return false;
    }
    let Ok(mut file) = std::fs::File::open(path) else { return false };
    let mut bytes = [0u8; 64 * 1024];
    let Ok(read) = file.read(&mut bytes) else { return false };
    std::str::from_utf8(&bytes[..read]).is_ok()
}

/// Index chapter boundaries by scanning encoded lines incrementally. UTF-8 uses the
/// document crate's direct scanner; explicit legacy code pages are decoded one line at a
/// time, so a large book never has to exist as one giant `String` merely to find chapter
/// starts.
pub fn scan_chapters(path: &Path, requested: Encoding, detect: bool) -> io::Result<ChapterIndex> {
    if matches!(requested, Encoding::Auto | Encoding::Utf8) {
        if let Ok(index) = ChapterIndex::from_path(path, detect) {
            return Ok(index);
        }
    }
    let file = std::fs::File::open(path)?;
    let length = file.metadata()?.len() as usize;
    let mut reader = BufReader::new(file);
    let mut chapters: Vec<Chapter> = Vec::new();
    let mut decoded_starts: Vec<usize> = Vec::new();
    let mut offset = 0usize;
    let mut decoded_offset = 0usize;
    let mut blank_separated = false;
    let mut first = true;
    loop {
        let mut line = Vec::new();
        let read = reader.read_until(b'\n', &mut line)?;
        if read == 0 { break; }
        let mut decoded = decode(&line, requested)?;
        if first {
            if let Some(stripped) = decoded.text.strip_prefix('\u{feff}') {
                decoded.text = stripped.to_string();
            }
            first = false;
        }
        if decoded.text.trim().is_empty() { blank_separated = true; }
        if detect && is_chapter(&decoded.text) {
            if let Some(last) = chapters.last_mut() {
                last.range.end = offset;
            } else if offset > 0 {
                chapters.push(Chapter { title: "Beginning".into(), range: 0..offset });
                decoded_starts.push(0);
            }
            chapters.push(Chapter { title: decoded.text.trim().into(), range: offset..length });
            decoded_starts.push(decoded_offset);
        }
        offset += read;
        decoded_offset += decoded.text.len();
    }
    if chapters.is_empty() {
        chapters.push(Chapter { title: "Beginning".into(), range: 0..length });
        decoded_starts.push(0);
    } else if let Some(last) = chapters.last_mut() {
        last.range.end = length;
    }
    Ok(ChapterIndex::from_parts_with_offsets(chapters, decoded_starts, blank_separated))
}

fn read_range(path: &Path, range: std::ops::Range<usize>, encoding: Encoding) -> io::Result<Decoded> {
    let mut file = std::fs::File::open(path)?;
    file.seek(SeekFrom::Start(range.start as u64))?;
    let mut bytes = vec![0; range.end.saturating_sub(range.start)];
    file.read_exact(&mut bytes)?;
    decode(&bytes, encoding)
}

pub fn read_chapter_source(
    path: &Path,
    index: &ChapterIndex,
    chapter: usize,
    encoding: Encoding,
) -> io::Result<Decoded> {
    read_range(path, index.range(chapter), encoding)
}

pub fn read_chapter(
    path: &Path,
    index: &ChapterIndex,
    chapter: usize,
    encoding: Encoding,
    options: TextOptions,
) -> io::Result<(Decoded, rubrica_doc::Document)> {
    let decoded = read_chapter_source(path, index, chapter, encoding)?;
    let document = index.window_text(&decoded.text, chapter, options);
    Ok((decoded, document))
}

pub fn read(path: &Path, requested: Encoding) -> std::io::Result<Decoded> {
    decode_owned(std::fs::read(path)?, requested)
}

/// BOM and automatic detection for a buffer, returning the BOM encoding, the byte
/// offset past it, the resolved encoding, and whether the text had to be guessed.
fn detect_encoding(bytes: &[u8], requested: Encoding) -> (Option<Encoding>, usize, Encoding, bool) {
    let (bom, offset) = if bytes.starts_with(&[0xef, 0xbb, 0xbf]) { (Some(Encoding::Utf8), 3) }
        else if bytes.starts_with(&[0xff, 0xfe]) { (Some(Encoding::Utf16Le), 2) }
        else if bytes.starts_with(&[0xfe, 0xff]) { (Some(Encoding::Utf16Be), 2) }
        else { (None, 0) };
    let encoding = if requested != Encoding::Auto { requested }
        else if let Some(bom) = bom { bom }
        else if std::str::from_utf8(bytes).is_ok() { Encoding::Utf8 }
        else { Encoding::Gb18030 };
    let guessed = requested == Encoding::Auto && bom.is_none() && encoding == Encoding::Gb18030;
    (bom, offset, encoding, guessed)
}

fn decode_error(encoding: Encoding) -> io::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidData,
        format!("Cannot decode as {}. Choose another text encoding.", encoding.label()))
}

pub fn decode(bytes: &[u8], requested: Encoding) -> std::io::Result<Decoded> {
    let (bom, offset, encoding, guessed) = detect_encoding(bytes, requested);
    let bytes = if bom == Some(encoding) { &bytes[offset..] } else { bytes };
    let text = match encoding {
        Encoding::Utf8 => std::str::from_utf8(bytes).map_err(|_| decode_error(encoding))?.to_string(),
        Encoding::Utf16Le | Encoding::Utf16Be => {
            if bytes.len() % 2 != 0 { return Err(decode_error(encoding)); }
            let units: Vec<_> = bytes.as_chunks::<2>().0.iter().map(|p| if encoding == Encoding::Utf16Le {
                u16::from_le_bytes([p[0], p[1]])
            } else { u16::from_be_bytes([p[0], p[1]]) }).collect();
            String::from_utf16(&units).map_err(|_| decode_error(encoding))?
        }
        Encoding::Gb18030 | Encoding::Big5 | Encoding::ShiftJis | Encoding::EucKr => {
            if bytes.is_empty() { String::new() } else {
                let cp = match encoding {
                    Encoding::Gb18030 => 54936, Encoding::Big5 => 950,
                    Encoding::ShiftJis => 932, _ => 51949,
                };
                // EUC-KR permits only zero flags on Windows. GB18030 must be strict:
                // it is the auto fallback and invalid input must never silently vanish.
                let flags = if encoding == Encoding::EucKr { MULTI_BYTE_TO_WIDE_CHAR_FLAGS(0) }
                    else { MB_ERR_INVALID_CHARS };
                let len = unsafe { MultiByteToWideChar(cp, flags, bytes, None) };
                if len == 0 { return Err(decode_error(encoding)); }
                let mut units = vec![0; len as usize];
                let read = unsafe { MultiByteToWideChar(cp, flags, bytes, Some(&mut units)) };
                if read != len { return Err(decode_error(encoding)); }
                String::from_utf16(&units).map_err(|_| decode_error(encoding))?
            }
        }
        Encoding::Auto => unreachable!(),
    };
    Ok(Decoded { text, encoding, guessed })
}

/// `read`'s decoder: the UTF-8 path converts the buffer in place, so a whole book
/// is never validated, copied and validated again; every other encoding decodes
/// `decode`'s borrowed slice.
fn decode_owned(mut bytes: Vec<u8>, requested: Encoding) -> io::Result<Decoded> {
    let (bom, offset, encoding, guessed) = detect_encoding(&bytes, requested);
    if encoding != Encoding::Utf8 {
        return decode(&bytes, requested);
    }
    if bom == Some(encoding) {
        bytes.drain(..offset);
    }
    let text = String::from_utf8(bytes).map_err(|_| decode_error(encoding))?;
    Ok(Decoded { text, encoding, guessed })
}

/// Convert a decoded UTF-8 byte offset into a 1-based line and Unicode-scalar column.
/// The offset is clamped to the nearest character boundary, which is what an editor
/// command needs when a remembered place came from an older layout.
pub fn line_column(source: &str, byte: usize) -> (usize, usize) {
    let mut at = byte.min(source.len());
    while at > 0 && !source.is_char_boundary(at) {
        at -= 1;
    }
    let before = &source[..at];
    let line = before.bytes().filter(|b| *b == b'\n').count() + 1;
    let column = before.rsplit_once('\n').map_or(before, |(_, tail)| tail).chars().count() + 1;
    (line, column)
}

/// A plain-text book: read without Markdown, one source line per paragraph unless
/// blank lines say otherwise. A log file is the same thing -- lines that are meant
/// to be read exactly as they were written -- so it is read the same way.
/// The paragraph rule a plain-text document opens with.
///
/// A log file's lines are records, not wrapped prose: folding them together on a
/// blank line's cue would bury every entry of a run but the first. The automatic
/// rule stays the default for books, whose hard breaks are an artifact of their
/// export rather than the shape of their sentences. A reader who has chosen a rule
/// keeps it -- only the untouched default follows the file's kind.
pub fn text_options_for(path: Option<&Path>, stored: TextOptions) -> TextOptions {
    let is_log = path
        .and_then(Path::extension)
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("log"));
    if is_log && stored == TextOptions::default() {
        TextOptions { paragraphs: ParagraphRule::Lines, chapters: false }
    } else {
        stored
    }
}

pub fn is_plain(path: Option<&Path>) -> bool {
    path.and_then(Path::extension).and_then(|s| s.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("txt") || e.eq_ignore_ascii_case("log"))
}

/// A Markdown document, the one thing the reader sets rather than reads as text.
pub fn is_markdown(path: Option<&Path>) -> bool {
    path.and_then(Path::extension).and_then(|s| s.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("md") || e.eq_ignore_ascii_case("markdown"))
}

/// A file the reader opens: plain text or Markdown, which is the same set the
/// workspace tree lists and a Markdown link may address.
pub fn is_document(path: &Path) -> bool {
    is_plain(Some(path)) || is_markdown(Some(path))
}

pub fn neighbor(path: &Path, forward: bool) -> Option<PathBuf> {
    let parent = path.parent()?;
    let plain = is_plain(Some(path));
    let mut files: Vec<_> = std::fs::read_dir(parent).ok()?.filter_map(Result::ok)
        .map(|e| e.path()).filter(|p| p.is_file() && if plain { is_plain(Some(p)) }
            else { p.extension().and_then(|e| e.to_str()).is_some_and(|e|
                e.eq_ignore_ascii_case("md") || e.eq_ignore_ascii_case("markdown")) }).collect();
    files.sort_by(|a, b| natural_cmp(&a.file_name().unwrap().to_string_lossy(), &b.file_name().unwrap().to_string_lossy()));
    let index = files.iter().position(|p| p.file_name() == path.file_name())?;
    files.get(if forward { index + 1 } else { index.checked_sub(1)? }).cloned()
}

fn natural_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    let (a, b) = (a.to_lowercase(), b.to_lowercase());
    let (mut a, mut b) = (a.as_str(), b.as_str());
    while !a.is_empty() && !b.is_empty() {
        if a.as_bytes()[0].is_ascii_digit() && b.as_bytes()[0].is_ascii_digit() {
            let na = a.bytes().take_while(u8::is_ascii_digit).count();
            let nb = b.bytes().take_while(u8::is_ascii_digit).count();
            let (da, db) = (a[..na].trim_start_matches('0'), b[..nb].trim_start_matches('0'));
            let order = da.len().cmp(&db.len()).then_with(|| da.cmp(db));
            if !order.is_eq() { return order; }
            a = &a[na..]; b = &b[nb..];
        } else {
            let (ca, cb) = (a.chars().next().unwrap(), b.chars().next().unwrap());
            let order = ca.cmp(&cb);
            if !order.is_eq() { return order; }
            a = &a[ca.len_utf8()..]; b = &b[cb.len_utf8()..];
        }
    }
    a.len().cmp(&b.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_boms_and_chinese_legacy_text_without_losing_invalid_bytes() {
        assert_eq!(decode(b"\xef\xbb\xbfhello", Encoding::Auto).unwrap().text, "hello");
        assert_eq!(decode(&[0xff, 0xfe, 0x2d, 0x4e], Encoding::Auto).unwrap().text, "中");
        assert_eq!(decode(&[0xfe, 0xff, 0x4e, 0x2d], Encoding::Auto).unwrap().text, "中");
        let cn = decode(&[0xd6, 0xd0, 0xce, 0xc4], Encoding::Auto).unwrap();
        assert_eq!(cn.text, "中文");
        assert!(cn.guessed);
        assert_eq!(cn.encoding, Encoding::Gb18030);
        assert!(decode(&[0xff], Encoding::Utf8).is_err());
        assert!(decode(&[0xff, 0xfe, 0x01], Encoding::Auto).is_err());
    }

    #[test]
    fn source_offsets_become_stable_editor_line_columns() {
        let source = "first\n中文😀\nlast";
        assert_eq!(line_column(source, 0), (1, 1));
        assert_eq!(line_column(source, 6), (2, 1));
        assert_eq!(line_column(source, 10), (2, 2));
        assert_eq!(line_column(source, 12), (2, 3));
        assert_eq!(line_column(source, 999), (3, 5));
        assert_eq!(line_column("😀x", 1), (1, 1));
        assert_eq!(line_column("😀x", 4), (1, 2));
    }

    #[test]
    fn legacy_encodings_can_index_and_read_one_chapter() {
        let path = std::env::temp_dir().join(format!("rubrica-legacy-window-{}.txt", std::process::id()));
        let source = "Prologue\nbody text\n\nChapter 2\nlast line\n";
        std::fs::write(&path, source).expect("write legacy fixture");
        for encoding in [Encoding::Gb18030, Encoding::Big5, Encoding::ShiftJis, Encoding::EucKr] {
            let index = scan_chapters(&path, encoding, true).expect("index legacy file");
            assert_eq!(index.chapters().len(), 2, "{encoding:?}");
            let (decoded, document) = read_chapter(
                &path,
                &index,
                1,
                encoding,
                rubrica_doc::plain::TextOptions { chapters: true, ..Default::default() },
            ).expect("read legacy chapter");
            assert_eq!(decoded.text.trim(), "Chapter 2\nlast line");
            assert_eq!(document.blocks[0].text, "Chapter 2");
            assert_eq!(document.blocks[0].sources[0].source, index.range(1).start);
        }
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn a_large_legacy_file_uses_the_same_chapter_window_path() {
        let path = std::env::temp_dir().join(format!("rubrica-large-legacy-{}.txt", std::process::id()));
        let mut bytes = b"Prologue\n".to_vec();
        while bytes.len() < LAZY_TEXT_THRESHOLD as usize + 32 {
            bytes.extend_from_slice(b"a long line of body text\n");
        }
        bytes.extend_from_slice(b"\nChapter 2\nlast line\n");
        std::fs::write(&path, &bytes).expect("write large legacy fixture");
        assert!(is_large_text(&path));
        for encoding in [Encoding::Gb18030, Encoding::Big5, Encoding::ShiftJis, Encoding::EucKr] {
            let index = scan_chapters(&path, encoding, true).expect("index large legacy file");
            let (decoded, document) = read_chapter(
                &path,
                &index,
                1,
                encoding,
                TextOptions { chapters: true, ..Default::default() },
            ).expect("read large legacy chapter");
            assert_eq!(decoded.text.trim(), "Chapter 2\nlast line");
            assert_eq!(document.blocks[0].text, "Chapter 2");
            assert_eq!(document.blocks[0].sources[0].source, index.range(1).start);
        }
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn legacy_chapter_source_positions_use_decoded_utf8_offsets() {
        let path = std::env::temp_dir().join(format!("rubrica-legacy-offsets-{}.txt", std::process::id()));
        for (encoding, body) in [
            (Encoding::Gb18030, [0xd6, 0xd0]),
            (Encoding::Big5, [0xa4, 0xa4]),
            (Encoding::ShiftJis, [0x93, 0xfa]),
            (Encoding::EucKr, [0xc7, 0xd1]),
        ] {
            let mut bytes = b"Prologue\n".to_vec();
            bytes.extend_from_slice(&body);
            bytes.extend_from_slice(b"\nChapter 2\nlast line\n");
            std::fs::write(&path, &bytes).expect("write encoded fixture");
            let index = scan_chapters(&path, encoding, true).expect("index encoded fixture");
            let (decoded, document) = read_chapter(
                &path,
                &index,
                1,
                encoding,
                TextOptions { chapters: true, ..Default::default() },
            ).expect("read encoded fixture");
            assert_eq!(index.range(1).start, 12, "{encoding:?}");
            assert_eq!(index.decoded_start(1), 13, "{encoding:?}");
            assert_eq!(index.chapter_at(index.decoded_start(1)), 1, "{encoding:?}");
            assert_eq!(decoded.text.lines().next(), Some("Chapter 2"));
            assert_eq!(document.blocks[0].sources[0].source, 13, "{encoding:?}");
        }
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn a_log_file_is_plain_text_and_markdown_is_not() {
        assert!(is_plain(Some(Path::new("notes.txt"))));
        assert!(is_plain(Some(Path::new("mct.log"))));
        assert!(!is_plain(Some(Path::new("readme.md"))));
        assert!(is_markdown(Some(Path::new("readme.md"))));
        assert!(is_markdown(Some(Path::new("notes.markdown"))));
        assert!(is_document(Path::new("a.log")));
        assert!(!is_document(Path::new("a.csv")));
        // A neighbour walk stays within the same kind: a log is offered other logs
        // and texts, never a Markdown file.
        assert_eq!(neighbor(Path::new("dir/a.log"), true), None);
    }

    #[test]
    fn a_log_file_reads_line_by_line_and_a_reader_keeps_their_rule() {
        let log = Path::new("mct.log");
        let opened = text_options_for(Some(log), TextOptions::default());
        assert_eq!(opened.paragraphs, ParagraphRule::Lines);
        assert!(!opened.chapters, "a log's lines are not chapter headings");
        // A book keeps the wrapping rule its export expects.
        let book = text_options_for(Some(Path::new("book.txt")), TextOptions::default());
        assert_eq!(book.paragraphs, ParagraphRule::Auto);
        // A rule the reader picked themselves is not overridden.
        let picked = TextOptions { paragraphs: ParagraphRule::BlankLines, chapters: true };
        assert_eq!(text_options_for(Some(log), picked), picked);
    }

    #[test]
    fn manual_east_asian_decoders_and_natural_chapter_order() {
        assert_eq!(decode(&[0xa4, 0xa4], Encoding::Big5).unwrap().text, "中");
        assert_eq!(decode(&[0x93, 0xfa], Encoding::ShiftJis).unwrap().text, "日");
        assert_eq!(decode(&[0xc7, 0xd1], Encoding::EucKr).unwrap().text, "한");
        assert!(natural_cmp("chapter2.txt", "chapter10.txt").is_lt());
        assert!(natural_cmp("第9章.txt", "第11章.txt").is_lt());
    }
}
