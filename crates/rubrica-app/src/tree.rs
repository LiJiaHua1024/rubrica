//! A small, filesystem-backed workspace tree.
//!
//! The window owns the scan so the pure workspace crate stays independent of Windows
//! path and registry rules. Directories are visited shallowly by default: a reader's
//! first tree should be useful immediately, while a book-sized tree should not walk a
//! whole disk on the UI thread.

use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntryKind {
    Directory,
    Document,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub path: PathBuf,
    pub name: String,
    pub depth: usize,
    pub kind: EntryKind,
}

pub fn is_document(path: &Path) -> bool {
    crate::reading::is_document(path)
}

fn name(path: &Path) -> String {
    path.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_else(|| path.to_string_lossy().into_owned())
}

fn visit(path: &Path, depth: usize, max_depth: usize, out: &mut Vec<Entry>) {
    let kind = if path.is_dir() { EntryKind::Directory } else if is_document(path) { EntryKind::Document } else { return };
    out.push(Entry { path: path.to_path_buf(), name: name(path), depth, kind });
    if kind != EntryKind::Directory || depth >= max_depth {
        return;
    }
    let Ok(children) = std::fs::read_dir(path) else { return };
    let mut children: Vec<PathBuf> = children.filter_map(Result::ok).map(|entry| entry.path()).collect();
    children.sort_by(|a, b| {
        let a_dir = a.is_dir();
        let b_dir = b.is_dir();
        b_dir.cmp(&a_dir).then_with(|| name(a).to_ascii_lowercase().cmp(&name(b).to_ascii_lowercase()))
    });
    for child in children {
        visit(&child, depth + 1, max_depth, out);
    }
}

/// Scan a directory into display-order rows. The root itself is not returned; its
/// children are, with directories before documents and each directory's descendants
/// directly beneath it.
pub fn scan(root: &Path, max_depth: usize) -> Vec<Entry> {
    let mut out = Vec::new();
    let Ok(children) = std::fs::read_dir(root) else { return out };
    let mut children: Vec<PathBuf> = children.filter_map(Result::ok).map(|entry| entry.path()).collect();
    children.sort_by(|a, b| {
        let a_dir = a.is_dir();
        let b_dir = b.is_dir();
        b_dir.cmp(&a_dir).then_with(|| name(a).to_ascii_lowercase().cmp(&name(b).to_ascii_lowercase()))
    });
    for child in children {
        visit(&child, 0, max_depth, &mut out);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn document_extensions_are_case_insensitive() {
        assert!(is_document(Path::new("README.MD")));
        assert!(is_document(Path::new("book.txt")));
        assert!(is_document(Path::new("build.LOG")));
        assert!(!is_document(Path::new("data.csv")));
    }

    #[test]
    fn a_missing_root_is_an_empty_tree() {
        assert!(scan(Path::new("Z:/rubrica-tree-does-not-exist"), 2).is_empty());
    }
}
