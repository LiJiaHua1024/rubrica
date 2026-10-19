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

/// How deep a scan may go however what the caller asked for: a directory cycle -- a
/// junction pointing back up the tree -- would otherwise be walked until the stack
/// runs out, and a stack overflow is not a thing a reader survives.
const HARD_MAX_DEPTH: usize = 32;

fn visit(path: &Path, depth: usize, max_depth: usize, out: &mut Vec<Entry>) {
    let kind = if path.is_dir() { EntryKind::Directory } else if is_document(path) { EntryKind::Document } else { return };
    out.push(Entry { path: path.to_path_buf(), name: name(path), depth, kind });
    if kind != EntryKind::Directory || depth >= max_depth.min(HARD_MAX_DEPTH) {
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

/// What the panel's search box holds, split the way a reader means it: on the spaces,
/// with every term folded once so that a listing does not fold the same word per row.
///
/// A term is a substring of the file name -- the extension included, since the extension
/// is part of the name -- so `txt` finds `notes.txt` and `notes txt` asks for both. A
/// term written as a pattern, `*.md`, asks for the extension itself rather than for a
/// name that happens to contain those letters.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Query {
    text: String,
    terms: Vec<String>,
}

impl Query {
    pub fn of(text: &str) -> Query {
        Query {
            // The reader's own words, with the runs of spaces a box holds down to the
            // single space the query means: a heading that echoed the spaces would be
            // showing the keyboard's accidents rather than the question.
            text: text.split_whitespace().collect::<Vec<_>>().join(" "),
            terms: text.split_whitespace().map(str::to_lowercase).collect(),
        }
    }

    /// What the reader typed, for a heading or a report line.
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Whether the query asks for anything. A reader who has typed nothing yet, or only
    /// spaces, has asked for the whole listing.
    pub fn is_empty(&self) -> bool {
        self.terms.is_empty()
    }

    pub fn matches(&self, name: &str) -> bool {
        if self.terms.is_empty() {
            return true;
        }
        let name = name.to_lowercase();
        self.terms.iter().all(|term| term_matches(&name, term))
    }
}

fn term_matches(name: &str, term: &str) -> bool {
    match term.strip_prefix("*.") {
        Some(extension) => name.rsplit_once('.').is_some_and(|(_, ext)| ext == extension),
        None => name.contains(term),
    }
}

/// The rows a query leaves of a scan, in the scan's own order: every entry whose name
/// matches, and the folders above it -- a name three folders down is no use to a reader
/// if the folder that holds it is not on screen. A folder that matches is kept as itself
/// and not opened: what is inside it is what opening it will show.
pub fn filtered(entries: &[Entry], query: &Query) -> Vec<Entry> {
    if query.is_empty() {
        return entries.to_vec();
    }
    let mut keep = vec![false; entries.len()];
    for i in (0..entries.len()).rev() {
        if query.matches(&entries[i].name) {
            keep[i] = true;
        }
        if !keep[i] {
            continue;
        }
        // The listing is in display order, so the folder that holds this row is the
        // nearest row above it that is shallower -- and that folder's own keeper, if it
        // has one, is found the same way when the loop reaches it.
        let depth = entries[i].depth;
        if let Some(parent) = (0..i).rev().find(|j| entries[*j].depth < depth) {
            if entries[parent].kind == EntryKind::Directory {
                keep[parent] = true;
            }
        }
    }
    entries.iter().zip(keep).filter(|(_, keep)| *keep).map(|(entry, _)| entry.clone()).collect()
}

/// What the headless listing prints: one row per entry the panel would show, indented the
/// way the panel indents it, under a line that says how much of the listing the query
/// left. The rows are the panel's own, so the answer to "what would the sidebar show" is
/// this and not a second implementation of it.
pub fn describe(entries: &[Entry], root: &Path, query: &Query) -> Vec<String> {
    let shown = filtered(entries, query);
    let count = |n: usize| format!("{n} {}", if n == 1 { "entry" } else { "entries" });
    let mut out = Vec::with_capacity(shown.len() + 1);
    out.push(if query.is_empty() {
        format!("tree         : {} in {}", count(entries.len()), root.display())
    } else {
        format!(
            "tree         : {} of {} match \"{}\" in {}",
            shown.len(),
            count(entries.len()),
            query.text(),
            root.display()
        )
    });
    out.extend(shown.iter().map(|entry| format!("  {}{}", "  ".repeat(entry.depth), entry.name)));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn listed(rows: &[(&str, usize, bool)]) -> Vec<Entry> {
        rows.iter()
            .map(|(name, depth, dir)| Entry {
                path: PathBuf::from(name),
                name: (*name).to_string(),
                depth: *depth,
                kind: if *dir { EntryKind::Directory } else { EntryKind::Document },
            })
            .collect()
    }

    /// The panel's own listing, with the folders a reader would see in it.
    fn shelf() -> Vec<Entry> {
        listed(&[
            ("books", 0, true),
            ("notes.md", 1, false),
            ("notes.txt", 1, false),
            ("old", 1, true),
            ("notes.txt", 2, false),
            ("src", 0, true),
            ("main.rs", 1, false),
            ("notes.rs", 1, false),
        ])
    }

    fn names(entries: &[Entry]) -> Vec<String> {
        entries.iter().map(|entry| entry.name.clone()).collect()
    }

    #[test]
    fn a_term_matches_anywhere_in_a_name_and_every_term_must_match() {
        let query = Query::of("txt");
        assert!(query.matches("notes.txt"));
        assert!(!query.matches("notes.md"));
        let query = Query::of("notes txt");
        assert!(query.matches("notes.txt"));
        assert!(!query.matches("notes.md"));
        // A name with both words in it is a hit wherever they stand.
        assert!(query.matches("notes.txt.bak"));
    }

    #[test]
    fn a_query_is_matched_without_regard_to_case() {
        assert!(Query::of("NOTES").matches("notes.txt"));
        assert!(Query::of("notes").matches("NOTES.TXT"));
        assert!(Query::of("*.MD").matches("notes.md"));
    }

    #[test]
    fn a_pattern_asks_for_the_extension_itself() {
        let query = Query::of("*.md");
        assert!(query.matches("notes.md"));
        assert!(!query.matches("notes.md.txt"));
        assert!(!query.matches("md"));
        assert!(!query.matches("notes.mdx"));
    }

    #[test]
    fn a_query_of_spaces_asks_for_everything() {
        assert!(Query::of("").is_empty());
        assert!(Query::of("   ").is_empty());
        assert!(Query::of(" \t ").is_empty());
        assert_eq!(Query::of("").text(), "");
        assert_eq!(Query::of("  notes  txt ").text(), "notes txt");
        assert_eq!(names(&filtered(&shelf(), &Query::of("  "))), names(&shelf()));
    }

    #[test]
    fn a_hit_keeps_the_folders_above_it() {
        // Both `notes.txt` files, the folder that holds the deeper one, and `books` above
        // that -- in the order the scan listed them.
        let shown = filtered(&shelf(), &Query::of("txt"));
        assert_eq!(names(&shown), vec!["books", "notes.txt", "old", "notes.txt"]);
        let shown = filtered(&shelf(), &Query::of("main"));
        assert_eq!(names(&shown), vec!["src", "main.rs"]);
    }

    #[test]
    fn a_folder_that_matches_is_kept_without_its_children() {
        let shown = filtered(&shelf(), &Query::of("old"));
        assert_eq!(names(&shown), vec!["books", "old"]);
        // `notes.rs` matches on its own name; `main.rs` keeps `src`.
        let shown = filtered(&shelf(), &Query::of("rs"));
        assert_eq!(names(&shown), vec!["src", "main.rs", "notes.rs"]);
    }

    #[test]
    fn a_query_nothing_matches_leaves_no_rows() {
        assert!(filtered(&shelf(), &Query::of("nothing here")).is_empty());
    }

    #[test]
    fn the_listing_names_how_much_the_query_left() {
        let root = Path::new("C:/shelf");
        let all = describe(&shelf(), root, &Query::of(""));
        assert_eq!(all[0], "tree         : 8 entries in C:/shelf");
        assert_eq!(all.len(), 9);
        // The indentation is the panel's: two spaces per folder of depth.
        assert_eq!(all[2], "    notes.md");
        assert_eq!(all[5], "      notes.txt");
        let some = describe(&shelf(), root, &Query::of("notes txt"));
        assert_eq!(some[0], "tree         : 4 of 8 entries match \"notes txt\" in C:/shelf");
        assert_eq!(some.len(), 5);
        let one = describe(&shelf(), root, &Query::of("main"));
        assert_eq!(one[0], "tree         : 2 of 8 entries match \"main\" in C:/shelf");
    }

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
