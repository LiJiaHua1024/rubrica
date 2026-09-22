//! Raw OpenType tables, read out of the font file rather than through DirectWrite.
//!
//! `IDWriteFontFace::TryGetFontTable` returns "no such table" for *every* tag on a face
//! taken from the system font collection: what the font cache hands out is a rebuilt
//! copy of the font, not its file. The two tables this reader exists for are the ones
//! that cannot be done without -- `MATH`, which all of [`crate::math`]'s numbers come
//! from, and `OS/2`, which says where a face's own strike belongs.
//!
//! The face does still know its file, so the file is opened and the table lifted out of
//! it. Everything here answers `None` on any trouble, which is what every caller already
//! treats as "this face has no such table" -- a failed read degrades the page, it never
//! breaks it.

use std::collections::HashMap;
use std::ffi::c_void;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::PathBuf;
use std::sync::{LazyLock, Mutex};

use windows::Win32::Graphics::DirectWrite::IDWriteFontFace;

/// Enough of the file to hold any table directory: a collection header, the offsets of
/// its faces, and the sfnt header plus records of the face being asked about.
const HEADER: usize = 4096;

/// Bytes of `tag` in `face`'s file.
pub fn table_bytes(face: &IDWriteFontFace, tag: &[u8; 4]) -> Option<Vec<u8>> {
    let (path, index) = source(face)?;
    let mut file = File::open(&path).ok()?;
    let mut head = vec![0u8; HEADER];
    let read = file.read(&mut head).ok()?;
    head.truncate(read);
    let (offset, len) = table_range(&head, index, tag)?;
    // A table's own bytes, not the whole file: a Chinese collection runs to tens of
    // megabytes, and only this range is ever wanted.
    let mut bytes = vec![0u8; len as usize];
    file.seek(SeekFrom::Start(offset)).ok()?;
    file.read_exact(&mut bytes).ok()?;
    Some(bytes)
}

/// The unchanging part of a font-file key: eight bytes identifying the file's state as the
/// loader sees it, then a `2a 00` marker. Every system face measured has a key exactly this
/// much longer than its name.
const KEY_HEADER: usize = 10;

/// Where a face lives: the file it was loaded from, and its number inside that file
/// when the file is a collection.
///
/// The key is not a path. For a font installed in a system font directory it is that
/// directory's file name, upper-cased and with no separator -- the font cache keys on the
/// name alone, because where a font was installed from is not something it tracks. So the
/// name has to be looked for in the directories themselves.
fn source(face: &IDWriteFontFace) -> Option<(PathBuf, u32)> {
    let mut count = 0u32;
    unsafe { face.GetFiles(&mut count, None).ok()? };
    let mut files: Vec<Option<_>> = vec![None; count as usize];
    unsafe { face.GetFiles(&mut count, Some(files.as_mut_ptr())).ok()? };
    let file = files.first_mut()?.take()?;
    let mut key: *mut c_void = std::ptr::null_mut();
    let mut size = 0u32;
    unsafe { file.GetReferenceKey(&mut key, &mut size).ok()? };
    if key.is_null() || size as usize <= KEY_HEADER {
        return None;
    }
    let utf16 =
        unsafe { std::slice::from_raw_parts(key.cast::<u16>(), size as usize / 2) };
    let name = key_name(utf16)?;
    Some((installed_font(&name)?, unsafe { face.GetIndex() }))
}

/// The file name a key ends with. Returns nothing for a key too short to hold one, which
/// is also the answer for any layout of key this code has not seen.
fn key_name(utf16: &[u16]) -> Option<String> {
    let tail = utf16.get(KEY_HEADER / 2..)?;
    let end = tail.iter().position(|&c| c == 0)?;
    String::from_utf16(&tail[..end]).ok()
}

/// The directories Windows reads fonts from, machine's first.
fn font_dirs() -> impl Iterator<Item = PathBuf> {
    [
        std::env::var_os("WINDIR").map(|w| PathBuf::from(w).join("Fonts")),
        std::env::var_os("LOCALAPPDATA")
            .map(|l| PathBuf::from(l).join("Microsoft").join("Windows").join("Fonts")),
    ]
    .into_iter()
    .flatten()
}

/// Resolve a bare font name to the file on disk. Cached because a page touches the same
/// handful of faces for every line, and the second branch of the search is a directory
/// listing.
fn installed_font(name: &str) -> Option<PathBuf> {
    static FOUND: LazyLock<Mutex<HashMap<String, Option<PathBuf>>>> =
        LazyLock::new(|| Mutex::new(HashMap::new()));
    let mut cache = FOUND.lock().ok()?;
    if let Some(hit) = cache.get(name) {
        return hit.clone();
    }
    let found = search_fonts(name);
    cache.insert(name.to_string(), found.clone());
    found
}

fn search_fonts(name: &str) -> Option<PathBuf> {
    for dir in font_dirs() {
        let direct = dir.join(name);
        if direct.is_file() {
            return Some(direct);
        }
        // The key's upper-casing survives a file installed as "msyh.ttc" but not one
        // shortened to an 8.3 alias, which the key reports instead of the real name.
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            if entry.file_name().to_string_lossy().eq_ignore_ascii_case(name) {
                return Some(entry.path());
            }
        }
    }
    None
}


/// `(offset, length)` of `tag` in the directory of face `index` of a font file.
fn table_range(head: &[u8], index: u32, tag: &[u8; 4]) -> Option<(u64, u64)> {
    let start = if head.starts_with(b"ttcf") {
        // A collection: tag, version, number of faces, then one offset each.
        let faces = u32_at(head, 8)? as usize;
        if index as usize >= faces {
            return None;
        }
        u32_at(head, 12 + 4 * index as usize)? as usize
    } else {
        0
    };
    let tables = u16_at(head, start + 4)? as usize;
    for i in 0..tables {
        let r = start + 12 + i * 16;
        if head.get(r..r + 4)? == *tag {
            return Some((u32_at(head, r + 8)? as u64, u32_at(head, r + 12)? as u64));
        }
    }
    None
}

fn u32_at(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_be_bytes(b.get(at..at + 4)?.try_into().ok()?))
}

fn u16_at(b: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_be_bytes(b.get(at..at + 2)?.try_into().ok()?))
}

/// One table directory record, for the tests below and for nothing else.
#[cfg(test)]
fn record(tag: &[u8; 4], offset: u32, len: u32) -> Vec<u8> {
    let mut v = tag.to_vec();
    v.extend([0u8; 4]); // checksum, which nothing here trusts or needs
    v.extend(offset.to_be_bytes());
    v.extend(len.to_be_bytes());
    v
}

/// An sfnt header plus its directory.
#[cfg(test)]
fn sfnt(tables: &[&[u8]]) -> Vec<u8> {
    let mut v = vec![0u8; 12];
    v[0..4].copy_from_slice(&[0, 0, 1, 0]); // TrueType outline version
    v[4..6].copy_from_slice(&(tables.len() as u16).to_be_bytes());
    for t in tables {
        v.extend_from_slice(t);
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_a_table_in_a_plain_sfnt() {
        let f = sfnt(&[&record(b"MATH", 900, 120), &record(b"OS/2", 40, 96)]);
        assert_eq!(table_range(&f, 0, b"OS/2"), Some((40, 96)));
        assert_eq!(table_range(&f, 0, b"MATH"), Some((900, 120)));
        assert_eq!(table_range(&f, 0, b"name"), None);
    }

    #[test]
    fn picks_the_requested_face_of_a_collection() {
        // Two faces, each with its own directory at a different place in the file.
        let a = sfnt(&[&record(b"OS/2", 1000, 96)]);
        let b = sfnt(&[&record(b"OS/2", 2000, 96)]);
        let mut f = vec![0u8; 20];
        f[0..4].copy_from_slice(b"ttcf");
        f[8..12].copy_from_slice(&2u32.to_be_bytes());
        f[12..16].copy_from_slice(&20u32.to_be_bytes());
        f[16..20].copy_from_slice(&(20 + a.len() as u32).to_be_bytes());
        f.extend_from_slice(&a);
        f.extend_from_slice(&b);
        assert_eq!(table_range(&f, 0, b"OS/2"), Some((1000, 96)));
        assert_eq!(table_range(&f, 1, b"OS/2"), Some((2000, 96)));
        // A face the collection does not have is not the same thing as face zero.
        assert_eq!(table_range(&f, 2, b"OS/2"), None);
    }

    #[test]
    fn a_header_cut_off_mid_directory_yields_nothing() {
        // The reader only maps the file's first few kilobytes, so a truncated buffer
        // has to fail rather than read past it.
        let f = sfnt(&[&record(b"OS/2", 1000, 96)]);
        assert_eq!(table_range(&f[..f.len() - 4], 0, b"OS/2"), None);
        assert_eq!(table_range(&[], 0, b"OS/2"), None);
    }

    /// A key as DirectWrite hands one over: the eight state bytes and the `2a 00` marker
    /// of Microsoft YaHei's, then the name it reports for that file.
    fn key(name: &str) -> Vec<u16> {
        let mut v = vec![0x0000u16; KEY_HEADER / 2];
        v[0] = 0xb652;
        v[1] = 0x10d9;
        v[2] = 0x84b6;
        v[3] = 0x01dc;
        v[4] = 0x002a;
        v.extend(name.encode_utf16());
        v.push(0);
        v
    }

    #[test]
    fn a_keys_name_is_the_bare_file_it_ends_with() {
        assert_eq!(key_name(&key("MSYH.TTC")).as_deref(), Some("MSYH.TTC"));
        assert_eq!(key_name(&key("SEGOEUI.TTF")).as_deref(), Some("SEGOEUI.TTF"));
        // Shorter than the header is not an empty name -- nothing was read.
        assert_eq!(key_name(&[]), None);
        assert_eq!(key_name(&[0; 4]), None);
    }
}
