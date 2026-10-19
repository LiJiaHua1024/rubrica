//! The folders a reader keeps, as the text made of them.
//!
//! The list itself and its cap live in `rubrica-workspace`; this module turns that state
//! into the two things a reader is shown -- the start page, and the rows the headless
//! listing prints -- so what the page says can be checked without a window.

use std::path::{Path, PathBuf};

use rubrica_workspace::Places;

use crate::i18n::{self, Key, Language};

/// The three groups a start page lists, in the order a reader looks for them.
pub struct Start<'a> {
    pub recent_files: &'a [PathBuf],
    pub places: &'a Places,
}

/// The start page: where a reader has been, and what they kept.
///
/// It is a document like any other rather than a drawing of its own, because everything
/// it does -- a heading, a row, a link that opens what it names -- is what the window
/// already knows how to read.
pub fn start_page(lang: Language, start: Start<'_>) -> String {
    let mut out = String::with_capacity(512);
    out.push_str("# ");
    out.push_str(i18n::t(lang, Key::MenuStartPage));
    out.push('\n');
    section(
        &mut out,
        i18n::t(lang, Key::MenuRecentDocuments),
        i18n::t(lang, Key::StartPageEmpty),
        start.recent_files.iter().map(|path| file_row(path)).collect(),
    );
    section(
        &mut out,
        i18n::t(lang, Key::MenuRecentFolders),
        i18n::t(lang, Key::StartPageEmpty),
        start.places.recent().iter().map(|folder| folder_row(&folder.path)).collect(),
    );
    section(
        &mut out,
        i18n::t(lang, Key::MenuFavouriteFolders),
        i18n::t(lang, Key::StartPageEmpty),
        start.places.favourites().iter().map(|folder| folder_row(&folder.path)).collect(),
    );
    out
}

/// What the headless listing prints: one row per entry the page and the menu would show,
/// under a line that says how many of each there are.
pub fn describe(start: Start<'_>) -> Vec<String> {
    let count = |n: usize, what: &str| format!("{n} {what}{}", if n == 1 { "" } else { "s" });
    let mut out = Vec::with_capacity(
        start.places.favourites().len() + start.places.recent().len() + start.recent_files.len() + 1,
    );
    out.push(format!(
        "places       : {}, {}, {}",
        count(start.places.favourites().len(), "favourite folder"),
        count(start.places.recent().len(), "recent folder"),
        count(start.recent_files.len(), "recent document"),
    ));
    out.extend(start.places.favourites().iter().map(|folder| format!("  favourite  : {}", display(&folder.path))));
    out.extend(start.places.recent().iter().map(|folder| format!("  recent dir : {}", display(&folder.path))));
    out.extend(start.recent_files.iter().map(|path| format!("  recent doc : {}", display(path))));
    out
}

fn section(out: &mut String, title: &str, empty: &str, rows: Vec<String>) {
    out.push_str("\n## ");
    out.push_str(title);
    out.push_str("\n\n");
    if rows.is_empty() {
        out.push_str(empty);
        out.push('\n');
        return;
    }
    for row in rows {
        out.push_str("- ");
        out.push_str(&row);
        out.push('\n');
    }
}

/// A document the reader was last in: a name to read and a path to open.
fn file_row(path: &Path) -> String {
    let label = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| display(path));
    format!("[{}]({})", escape_label(&label), escape_destination(&display(path)))
}

/// A folder, as the path itself, in code so a name that holds `*` or `_` is still the
/// name of a folder. It is not a link: a folder is opened in the panel rather than read,
/// and the menu is where that is asked for.
fn folder_row(path: &Path) -> String {
    let shown = display(path);
    if shown.contains('`') { shown } else { format!("`{shown}`") }
}

fn escape_label(label: &str) -> String {
    label.replace('[', "\\[").replace(']', "\\]")
}

/// The characters a link destination cannot carry bare, written the way a reader's own
/// links write them: the window undoes these before it looks at the file system, so a
/// folder called `two words` is found rather than a name with a space in it.
fn escape_destination(path: &str) -> String {
    let mut out = String::with_capacity(path.len());
    for c in path.chars() {
        match c {
            ' ' => out.push_str("%20"),
            '%' => out.push_str("%25"),
            '(' => out.push_str("%28"),
            ')' => out.push_str("%29"),
            '<' => out.push_str("%3C"),
            '>' => out.push_str("%3E"),
            '#' => out.push_str("%23"),
            '?' => out.push_str("%3F"),
            _ => out.push(c),
        }
    }
    out
}

/// A path as a reader reads it: without the verbatim prefix the canonical form carries.
fn display(path: &Path) -> String {
    crate::view::display_path(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rubrica_workspace::FolderRef;

    fn places(favourites: &[&str], recent: &[&str]) -> Places {
        let mut places = Places::default();
        for path in favourites.iter().rev() {
            places.favourite(FolderRef::new(*path));
        }
        for path in recent.iter().rev() {
            places.touch_recent(FolderRef::new(*path));
        }
        places
    }

    #[test]
    fn the_start_page_names_the_three_groups_in_the_order_a_reader_looks_for_them() {
        let files = [PathBuf::from(r"C:\notes\a.md"), PathBuf::from(r"C:\notes\b c.md")];
        let kept = places(&[r"C:\keep"], &[r"C:\last"]);
        let page = start_page(Language::EnUs, Start { recent_files: &files, places: &kept });
        assert_eq!(
            page,
            "# Start page\n\
             \n\
             ## Recent documents\n\
             \n\
             - [a.md](C:\\notes\\a.md)\n\
             - [b c.md](C:\\notes\\b%20c.md)\n\
             \n\
             ## Recent folders\n\
             \n\
             - `C:\\last`\n\
             \n\
             ## Favourite folders\n\
             \n\
             - `C:\\keep`\n"
        );
    }

    #[test]
    fn a_group_with_nothing_in_it_says_so_rather_than_going_missing() {
        let empty = Places::default();
        let page = start_page(Language::EnUs, Start { recent_files: &[], places: &empty });
        assert_eq!(page.matches("Nothing here yet.").count(), 3, "one line for each group");
        assert_eq!(page.matches("## ").count(), 3, "and the headings are still there");
    }

    #[test]
    fn the_page_is_written_in_the_language_the_reader_chose() {
        let empty = Places::default();
        let page = start_page(Language::ZhCn, Start { recent_files: &[], places: &empty });
        assert!(page.starts_with("# 起始页\n"), "{page}");
        assert!(page.contains("## 最近文件夹"), "{page}");
        assert!(page.contains("## 收藏的文件夹"), "{page}");
    }

    #[test]
    fn a_folder_row_is_code_so_a_name_cannot_be_read_as_emphasis() {
        let kept = places(&[r"C:\a_b_c"], &[]);
        let page = start_page(Language::EnUs, Start { recent_files: &[], places: &kept });
        assert!(page.contains("- `C:\\a_b_c`"), "{page}");
    }

    #[test]
    fn a_name_with_a_bracket_in_it_does_not_break_the_link() {
        let files = [PathBuf::from(r"C:\odd [1].md")];
        let empty = Places::default();
        let page = start_page(Language::EnUs, Start { recent_files: &files, places: &empty });
        assert!(page.contains(r"[odd \[1\].md](C:\odd%20[1].md)"), "{page}");
    }

    #[test]
    fn the_listing_says_how_many_of_each_and_names_every_row() {
        let files = [PathBuf::from(r"C:\notes\a.md")];
        let kept = places(&[r"C:\keep", r"C:\also"], &[r"C:\last"]);
        let rows = describe(Start { recent_files: &files, places: &kept });
        assert_eq!(
            rows,
            vec![
                "places       : 2 favourite folders, 1 recent folder, 1 recent document".to_string(),
                r"  favourite  : C:\keep".to_string(),
                r"  favourite  : C:\also".to_string(),
                r"  recent dir : C:\last".to_string(),
                r"  recent doc : C:\notes\a.md".to_string(),
            ]
        );
    }

    #[test]
    fn the_listing_counts_one_of_a_thing_as_one_thing() {
        let files = [PathBuf::from(r"C:\notes\a.md")];
        let kept = places(&[r"C:\keep"], &[]);
        let rows = describe(Start { recent_files: &files, places: &kept });
        assert_eq!(rows[0], "places       : 1 favourite folder, 0 recent folders, 1 recent document");
    }

    #[test]
    fn the_verbatim_prefix_the_canonical_form_carries_is_not_shown() {
        let files = [PathBuf::from(r"\\?\C:\notes\a.md")];
        let empty = Places::default();
        let rows = describe(Start { recent_files: &files, places: &empty });
        assert_eq!(rows[1], r"  recent doc : C:\notes\a.md");
        let page = start_page(Language::EnUs, Start { recent_files: &files, places: &empty });
        assert!(page.contains(r"(C:\notes\a.md)"), "{page}");
    }
}
