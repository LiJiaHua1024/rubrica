//! Byte decoding and neighboring documents, independent of the window and typography.

use std::path::{Path, PathBuf};
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

/// A conservative probe for the lazy path. Legacy code pages stay on the fully decoded
/// route because a byte range can cut a multibyte character; a malformed prefix simply
/// falls back to the old path rather than guessing.
pub fn can_window_text(path: &Path, requested: Encoding) -> bool {
    if !is_large_text(path) || !matches!(requested, Encoding::Auto | Encoding::Utf8) {
        return false;
    }
    let Ok(mut file) = std::fs::File::open(path) else { return false };
    use std::io::Read;
    let mut bytes = [0u8; 64 * 1024];
    let Ok(read) = file.read(&mut bytes) else { return false };
    std::str::from_utf8(&bytes[..read]).is_ok()
}

pub fn read(path: &Path, requested: Encoding) -> std::io::Result<Decoded> {
    decode(&std::fs::read(path)?, requested)
}

pub fn decode(bytes: &[u8], requested: Encoding) -> std::io::Result<Decoded> {
    let (bom, offset) = if bytes.starts_with(&[0xef, 0xbb, 0xbf]) { (Some(Encoding::Utf8), 3) }
        else if bytes.starts_with(&[0xff, 0xfe]) { (Some(Encoding::Utf16Le), 2) }
        else if bytes.starts_with(&[0xfe, 0xff]) { (Some(Encoding::Utf16Be), 2) }
        else { (None, 0) };
    let encoding = if requested != Encoding::Auto { requested }
        else if let Some(bom) = bom { bom }
        else if std::str::from_utf8(bytes).is_ok() { Encoding::Utf8 }
        else { Encoding::Gb18030 };
    let guessed = requested == Encoding::Auto && bom.is_none() && encoding == Encoding::Gb18030;
    let bytes = if bom == Some(encoding) { &bytes[offset..] } else { bytes };
    let error = || std::io::Error::new(std::io::ErrorKind::InvalidData,
        format!("Cannot decode as {}. Choose another text encoding.", encoding.label()));
    let text = match encoding {
        Encoding::Utf8 => std::str::from_utf8(bytes).map_err(|_| error())?.to_string(),
        Encoding::Utf16Le | Encoding::Utf16Be => {
            if bytes.len() % 2 != 0 { return Err(error()); }
            let units: Vec<_> = bytes.as_chunks::<2>().0.iter().map(|p| if encoding == Encoding::Utf16Le {
                u16::from_le_bytes([p[0], p[1]])
            } else { u16::from_be_bytes([p[0], p[1]]) }).collect();
            String::from_utf16(&units).map_err(|_| error())?
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
                if len == 0 { return Err(error()); }
                let mut units = vec![0; len as usize];
                let read = unsafe { MultiByteToWideChar(cp, flags, bytes, Some(&mut units)) };
                if read != len { return Err(error()); }
                String::from_utf16(&units).map_err(|_| error())?
            }
        }
        Encoding::Auto => unreachable!(),
    };
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

pub fn is_plain(path: Option<&Path>) -> bool {
    path.and_then(Path::extension).and_then(|s| s.to_str()).is_some_and(|e| e.eq_ignore_ascii_case("txt"))
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
    fn manual_east_asian_decoders_and_natural_chapter_order() {
        assert_eq!(decode(&[0xa4, 0xa4], Encoding::Big5).unwrap().text, "中");
        assert_eq!(decode(&[0x93, 0xfa], Encoding::ShiftJis).unwrap().text, "日");
        assert_eq!(decode(&[0xc7, 0xd1], Encoding::EucKr).unwrap().text, "한");
        assert!(natural_cmp("chapter2.txt", "chapter10.txt").is_lt());
        assert!(natural_cmp("第9章.txt", "第11章.txt").is_lt());
    }
}
