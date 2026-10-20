//! The one place a key is written down, and the three things read from it.
//!
//! A reader asking "what key does this" used to be answered by a table in the README,
//! which is a second copy of the truth and drifts from it the moment a key is bound. The
//! keys are named here once, in [`TABLE`], beside a payload-free [`Shortcut`] naming
//! *what* a key asks for, and three things are read from that one table:
//!
//! * the page a reader opens from the menu ([`page`]),
//! * the listing a headless run prints ([`describe`]),
//! * and the accelerator column the menu prints beside a row ([`hint`]).
//!
//! Because the menu's own text comes from here too, a key cannot be changed in one place
//! and left standing in another: there is only one place.
//!
//! What is listed is what the menu offers, not every key the window answers: the arrow
//! keys, the wheel and the caret's own chords are how one moves about a page already on
//! the screen, and the README describes them rather than the menu listing them. Every
//! action the menu can be asked to perform is here, and one the menu gains cannot be left
//! out: `view::action_of` is an exhaustive match over the command enum, called on the path
//! that builds the menu, so a command added to the enum stops the release build until it is
//! classified; the tests then check that every action the table holds is one the menu can
//! actually reach.

use crate::i18n::{self, Key, Language};

/// One thing the menu can be asked to do, without the argument that says which item it
/// applies to.
///
/// A radio group is one action rather than one per rung: what a shortcut list is for is
/// the keys, and no key settles on a rung. The commands that carry an argument -- a tab
/// id, a folder index, a preset name -- collapse here for the same reason, because the row
/// that opens the third tab and the row that opens the fifth are one gesture with a
/// different argument, and neither has a key of its own.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Shortcut {
    Back,
    Forward,
    Copy,
    SelectAll,
    SelectTable,
    Find,
    OpenLink,
    CopyLink,
    Contents,
    ZoomIn,
    ZoomOut,
    ZoomReset,
    ContinuousScroll,
    PageStack,
    PreviousPage,
    NextPage,
    EditPreset,
    Presets,
    Faces,
    Measures,
    Light,
    Dark,
    FollowSystem,
    ColourTheme,
    CopyAsMyTheme,
    OpenThemesFolder,
    WorkspaceTree,
    OpenFile,
    StartPage,
    ShortcutsPage,
    Reload,
    OpenTabs,
    PinTab,
    CloseTab,
    CloseOtherTabs,
    CloseAll,
    RecentDocuments,
    FavouriteFolders,
    RecentFolders,
    FavouriteFolder,
    StatusBar,
    SingleNewlines,
    SourceView,
    FormatExt,
    FormatPlain,
    FormatMarkdown,
    HtmlImages,
    Highlights,
    InlineCode,
    TableLines,
    ParagraphsAuto,
    ParagraphsLines,
    ParagraphsBlank,
    DetectChapters,
    PreviousChapter,
    NextChapter,
    WideTableNarrow,
    WideTableWiden,
    PreviousFile,
    NextFile,
    TextEncoding,
    Language,
    OpenInEditor,
    ChooseEditor,
    PeekEnabled,
    PeekTap,
    PeekHold,
    PeekMixed,
    PeekFocusClose,
    PeekClickClose,
    CheckUpdates,
    UpdateLabs,
    SendFeedback,
    PrivacyPolicy,
    License,
}

impl Shortcut {
    /// Every action the table holds, in the order a reader meets them.
    pub(crate) fn all() -> impl Iterator<Item = Shortcut> {
        TABLE.iter().map(|row| row.action)
    }

    /// How many actions there are.
    pub(crate) fn count() -> usize {
        TABLE.len()
    }
}

/// One action, the keys that ask for it, and the keys the menu prints beside it.
///
/// `keys` is every key that reaches the action, which is what the page lists. `menu` is
/// what the menu prints in its accelerator column, and it is deliberately allowed to be a
/// smaller list: a key that does not *choose* the row it would stand beside cannot be
/// printed there without promising something else. `Shift`+`Space` turns the page back
/// rather than forward, so the row for the next page is the `Space` row; `Ctrl`+`4` swaps
/// whichever reading mode is on for the other, which is a thing to look up rather than a
/// thing to point at a row with. Where the two agree -- almost everywhere -- the menu shows
/// the same list the page does, because both are read from this struct.
struct Line {
    action: Shortcut,
    keys: &'static [&'static str],
    menu: &'static [&'static str],
}

/// A row whose menu prints every key the action answers to.
const fn line(action: Shortcut, keys: &'static [&'static str]) -> Line {
    Line { action, keys, menu: keys }
}

/// A row the menu prints no key beside.
const fn silent(action: Shortcut, keys: &'static [&'static str]) -> Line {
    Line { action, keys, menu: &[] }
}

/// A row the menu prints some of the action's keys beside.
const fn partly(action: Shortcut, keys: &'static [&'static str], menu: &'static [&'static str]) -> Line {
    Line { action, keys, menu }
}

/// The table: every action the menu offers, with its keys.
///
/// An empty key list is a statement rather than a gap -- the action is on the menu and no
/// key reaches it -- and the page says so in the reader's own words instead of leaving the
/// cell blank. Keys name physical keys and are never translated.
const TABLE: &[Line] = &[
    line(Shortcut::Back, &["Ctrl+Z", "Alt+\u{2190}"]),
    line(Shortcut::Forward, &["Ctrl+Shift+Z", "Alt+\u{2192}"]),
    line(Shortcut::Copy, &["Ctrl+C"]),
    line(Shortcut::SelectAll, &["Ctrl+A"]),
    line(Shortcut::SelectTable, &["Ctrl+Shift+A"]),
    line(Shortcut::Find, &["Ctrl+F"]),
    silent(Shortcut::OpenLink, &[]),
    silent(Shortcut::CopyLink, &[]),
    silent(Shortcut::Contents, &[]),
    // The shifted and the unshifted form of the key, and the numpad's own, all land on one
    // command, so the list names the one every keyboard has.
    line(Shortcut::ZoomIn, &["Ctrl++"]),
    line(Shortcut::ZoomOut, &["Ctrl+-"]),
    line(Shortcut::ZoomReset, &["Ctrl+0"]),
    // One key for both modes, because it is one key. The page names it on both rows, which
    // is what a reader's fingers do; the menu prints it on the page-stack row only, because
    // that is the row a reader in continuous scrolling is looking for, and the row it stands
    // beside has always said it.
    silent(Shortcut::ContinuousScroll, &["Ctrl+4"]),
    line(Shortcut::PageStack, &["Ctrl+4"]),
    // `Space` turns the page on and `Shift` with it turns back, in either reading mode, and
    // a held key walks on through the document. The menu keeps to the key that names the row
    // it stands beside; the page names both, because a reader looking up "how do I go back a
    // page" needs the answer, not the row it was printed on.
    partly(Shortcut::PreviousPage, &["PgUp", "Shift+Space"], &["PgUp"]),
    partly(Shortcut::NextPage, &["PgDn", "Space"], &["PgDn"]),
    silent(Shortcut::EditPreset, &[]),
    silent(Shortcut::Presets, &[]),
    silent(Shortcut::Faces, &[]),
    silent(Shortcut::Measures, &[]),
    silent(Shortcut::Light, &[]),
    silent(Shortcut::Dark, &[]),
    silent(Shortcut::FollowSystem, &[]),
    silent(Shortcut::ColourTheme, &[]),
    silent(Shortcut::CopyAsMyTheme, &[]),
    silent(Shortcut::OpenThemesFolder, &[]),
    silent(Shortcut::WorkspaceTree, &[]),
    line(Shortcut::OpenFile, &["Ctrl+O"]),
    silent(Shortcut::StartPage, &[]),
    silent(Shortcut::ShortcutsPage, &[]),
    line(Shortcut::Reload, &["Ctrl+R"]),
    // Walking the strip is a pair of keys rather than a pair of rows: the menu's own rows
    // are one per tab, and the key that steps to the next one has no single row to stand
    // beside.
    silent(Shortcut::OpenTabs, &["Ctrl+Tab", "Ctrl+Shift+Tab"]),
    silent(Shortcut::PinTab, &[]),
    line(Shortcut::CloseTab, &["Ctrl+W"]),
    silent(Shortcut::CloseOtherTabs, &[]),
    silent(Shortcut::CloseAll, &[]),
    silent(Shortcut::RecentDocuments, &[]),
    silent(Shortcut::FavouriteFolders, &[]),
    silent(Shortcut::RecentFolders, &[]),
    silent(Shortcut::FavouriteFolder, &[]),
    silent(Shortcut::StatusBar, &[]),
    silent(Shortcut::SingleNewlines, &[]),
    line(Shortcut::SourceView, &["Ctrl+3"]),
    silent(Shortcut::FormatExt, &[]),
    silent(Shortcut::FormatPlain, &[]),
    silent(Shortcut::FormatMarkdown, &[]),
    silent(Shortcut::HtmlImages, &[]),
    silent(Shortcut::Highlights, &[]),
    silent(Shortcut::InlineCode, &[]),
    silent(Shortcut::TableLines, &[]),
    silent(Shortcut::ParagraphsAuto, &[]),
    silent(Shortcut::ParagraphsLines, &[]),
    silent(Shortcut::ParagraphsBlank, &[]),
    silent(Shortcut::DetectChapters, &[]),
    line(Shortcut::PreviousChapter, &["Ctrl+Alt+Up"]),
    line(Shortcut::NextChapter, &["Ctrl+Alt+Down"]),
    silent(Shortcut::WideTableNarrow, &[]),
    silent(Shortcut::WideTableWiden, &[]),
    line(Shortcut::PreviousFile, &["Ctrl+Alt+Left"]),
    line(Shortcut::NextFile, &["Ctrl+Alt+Right"]),
    silent(Shortcut::TextEncoding, &[]),
    silent(Shortcut::Language, &[]),
    line(Shortcut::OpenInEditor, &["Ctrl+Shift+O"]),
    silent(Shortcut::ChooseEditor, &[]),
    silent(Shortcut::PeekEnabled, &[]),
    silent(Shortcut::PeekTap, &[]),
    silent(Shortcut::PeekHold, &[]),
    silent(Shortcut::PeekMixed, &[]),
    silent(Shortcut::PeekFocusClose, &[]),
    silent(Shortcut::PeekClickClose, &[]),
    // The rows about the program rather than about the page: none of them is a key,
    // because none of them is something a reader does while reading.
    silent(Shortcut::CheckUpdates, &[]),
    silent(Shortcut::UpdateLabs, &[]),
    silent(Shortcut::SendFeedback, &[]),
    silent(Shortcut::PrivacyPolicy, &[]),
    silent(Shortcut::License, &[]),
];

/// The row this action holds, if it holds one.
fn find(action: Shortcut) -> Option<&'static Line> {
    TABLE.iter().find(|row| row.action == action)
}

/// The keys that ask for an action. An action no key reaches answers with nothing.
pub(crate) fn keys(action: Shortcut) -> &'static [&'static str] {
    find(action).map_or(&[], |row| row.keys)
}

/// The accelerator column a menu row prints, tab-separated, or nothing at all.
///
/// Windows sets that column right-aligned on its own, so what this returns is appended to a
/// row's words: one tab and then each key.
pub(crate) fn hint(action: Shortcut) -> String {
    let Some(row) = find(action) else { return String::new() };
    let mut out = String::new();
    for key in row.menu {
        out.push('\t');
        out.push_str(key);
    }
    out
}

/// The name the menu gives an action, in the reader's language.
///
/// The menu's own label, so the page and the menu cannot call one action by two names. The
/// three that are a list of choices rather than a label -- the faces, the measures and the
/// presets -- are named by their choices, because a group the menu never drew has no name
/// of its own to borrow.
fn label(lang: Language, action: Shortcut) -> String {
    let key = match action {
        Shortcut::Back => Key::MenuBack,
        Shortcut::Forward => Key::MenuForward,
        Shortcut::Copy => Key::MenuCopy,
        Shortcut::SelectAll => Key::MenuSelectAll,
        Shortcut::SelectTable => Key::MenuSelectTable,
        Shortcut::Find => Key::MenuFind,
        Shortcut::OpenLink => Key::MenuOpenLink,
        Shortcut::CopyLink => Key::MenuCopyLink,
        Shortcut::Contents => Key::MenuContents,
        Shortcut::ZoomIn => Key::MenuZoomIn,
        Shortcut::ZoomOut => Key::MenuZoomOut,
        Shortcut::ZoomReset => Key::MenuZoomReset,
        Shortcut::ContinuousScroll => Key::MenuContinuousScroll,
        Shortcut::PageStack => Key::MenuPageStack,
        Shortcut::PreviousPage => Key::MenuPreviousPage,
        Shortcut::NextPage => Key::MenuNextPage,
        Shortcut::EditPreset => Key::MenuEditSavePreset,
        Shortcut::Light => Key::MenuLight,
        Shortcut::Dark => Key::MenuDark,
        Shortcut::FollowSystem => Key::MenuFollowSystem,
        Shortcut::ColourTheme => Key::MenuColourTheme,
        Shortcut::CopyAsMyTheme => Key::MenuCopyTheme,
        Shortcut::OpenThemesFolder => Key::MenuOpenThemesFolder,
        Shortcut::WorkspaceTree => Key::MenuWorkspaceTree,
        Shortcut::OpenFile => Key::MenuOpenFile,
        Shortcut::StartPage => Key::MenuStartPage,
        Shortcut::ShortcutsPage => Key::MenuShortcuts,
        Shortcut::Reload => Key::MenuReload,
        Shortcut::OpenTabs => Key::MenuOpenTabs,
        Shortcut::PinTab => Key::MenuPinTab,
        Shortcut::CloseTab => Key::MenuCloseTab,
        Shortcut::CloseOtherTabs => Key::MenuCloseOtherTabs,
        Shortcut::CloseAll => Key::MenuCloseAllTabs,
        Shortcut::RecentDocuments => Key::MenuRecentDocuments,
        Shortcut::FavouriteFolders => Key::MenuFavouriteFolders,
        Shortcut::RecentFolders => Key::MenuRecentFolders,
        Shortcut::FavouriteFolder => Key::MenuFavouriteFolder,
        Shortcut::StatusBar => Key::MenuStatusLabel,
        Shortcut::SingleNewlines => Key::MenuSingleNewlines,
        Shortcut::SourceView => Key::MenuReadSource,
        Shortcut::FormatExt => Key::MenuFormatExt,
        Shortcut::FormatPlain => Key::MenuFormatPlain,
        Shortcut::FormatMarkdown => Key::MenuFormatMarkdown,
        Shortcut::HtmlImages => Key::MenuHtmlImages,
        Shortcut::Highlights => Key::MenuHighlights,
        Shortcut::InlineCode => Key::MenuInlineCode,
        Shortcut::TableLines => Key::MenuTableLines,
        Shortcut::ParagraphsAuto => Key::MenuParagraphsAuto,
        Shortcut::ParagraphsLines => Key::MenuParagraphsLines,
        Shortcut::ParagraphsBlank => Key::MenuParagraphsBlank,
        Shortcut::DetectChapters => Key::MenuDetectChapters,
        Shortcut::PreviousChapter => Key::MenuPreviousChapter,
        Shortcut::NextChapter => Key::MenuNextChapter,
        Shortcut::WideTableNarrow => Key::MenuWideTableNarrow,
        Shortcut::WideTableWiden => Key::MenuWideTableWiden,
        Shortcut::PreviousFile => Key::MenuPreviousFile,
        Shortcut::NextFile => Key::MenuNextFile,
        Shortcut::TextEncoding => Key::MenuTextEncoding,
        Shortcut::Language => Key::MenuLanguage,
        Shortcut::OpenInEditor => Key::MenuOpenInEditor,
        Shortcut::ChooseEditor => Key::MenuChooseEditor,
        Shortcut::PeekEnabled => Key::MenuPeekEnabled,
        Shortcut::PeekTap => Key::MenuPeekTap,
        Shortcut::PeekHold => Key::MenuPeekHold,
        Shortcut::PeekMixed => Key::MenuPeekMixed,
        Shortcut::PeekFocusClose => Key::MenuPeekFocusClose,
        Shortcut::PeekClickClose => Key::MenuPeekClickClose,
        Shortcut::CheckUpdates => Key::MenuCheckUpdates,
        Shortcut::UpdateLabs => Key::MenuUpdateLabs,
        Shortcut::SendFeedback => Key::MenuSendFeedback,
        Shortcut::PrivacyPolicy => Key::MenuPrivacy,
        Shortcut::License => Key::MenuLicense,
        Shortcut::Presets => {
            let mut out = String::new();
            for (i, name) in ["Default", "Book"].into_iter().enumerate() {
                if i > 0 {
                    out.push_str(" / ");
                }
                out.push_str(&i18n::preset_display_name(lang, name));
            }
            return out;
        }
        Shortcut::Faces => {
            let mut out = String::new();
            for i in 0..4 {
                if i > 0 {
                    out.push_str(" / ");
                }
                out.push_str(i18n::face_label(lang, i));
            }
            return out;
        }
        Shortcut::Measures => {
            let mut out = String::new();
            for i in 0..3 {
                if i > 0 {
                    out.push_str(" / ");
                }
                out.push_str(i18n::measure_label(lang, i));
            }
            return out;
        }
    };
    i18n::t(lang, key).to_string()
}

/// One line of the list: the name the menu gives an action and the keys it answers to.
pub(crate) struct Row {
    pub label: String,
    pub keys: &'static [&'static str],
}

/// The list, in the table's order. Both the page and the headless listing start here, so
/// what a reader reads and what a test measures are one text.
pub(crate) fn rows(lang: Language) -> Vec<Row> {
    Shortcut::all().map(|action| Row { label: label(lang, action), keys: keys(action) }).collect()
}

/// The keys of a row written as one cell: each in code, apart, or the reader's own word
/// for there being none.
fn keys_cell(lang: Language, keys: &[&str]) -> String {
    if keys.is_empty() {
        return i18n::t(lang, Key::ShortcutsNone).to_string();
    }
    let mut out = String::new();
    for (i, key) in keys.iter().enumerate() {
        if i > 0 {
            out.push_str(" \u{00B7} ");
        }
        out.push('`');
        out.push_str(key);
        out.push('`');
    }
    out
}

/// The page: a two-column table of every action and its keys.
///
/// A document rather than a drawing of its own, for the reason the start page is one:
/// everything it is -- a heading, a table, a code span -- is what the window already knows
/// how to read, so the page can be laid out and measured with no window at all.
pub(crate) fn page(lang: Language) -> String {
    let mut out = String::with_capacity(4096);
    out.push_str("# ");
    out.push_str(i18n::t(lang, Key::MenuShortcuts));
    out.push_str("\n\n| ");
    out.push_str(i18n::t(lang, Key::ShortcutsAction));
    out.push_str(" | ");
    out.push_str(i18n::t(lang, Key::ShortcutsKeys));
    out.push_str(" |\n| --- | --- |\n");
    for row in rows(lang) {
        out.push_str("| ");
        // A pipe inside a cell ends the cell it is in, so the one a name could carry is
        // written as the escape the reader's own table syntax reads back.
        out.push_str(&row.label.replace('|', "\\|"));
        out.push_str(" | ");
        out.push_str(&keys_cell(lang, row.keys));
        out.push_str(" |\n");
    }
    out
}

/// What the headless listing prints: one line per action, then the count.
///
/// The name first and the keys after a tab, so the listing reads as the page's two columns
/// and cuts as its two fields.
pub(crate) fn describe(lang: Language) -> Vec<String> {
    let mut out = Vec::with_capacity(Shortcut::count() + 1);
    for action in Shortcut::all() {
        let keys = keys(action);
        // A key list of nothing prints the one mark that cannot be mistaken for a key: the
        // page says it in words, but a listing is read field by field, where a dash is what
        // a table puts in an empty field.
        let keys = if keys.is_empty() { "-".to_string() } else { keys.join(" ") };
        out.push(format!("{}\t{keys}", label(lang, action)));
    }
    out.push(format!("shortcuts    : {} action(s)", Shortcut::count()));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_table_holds_each_action_once_and_in_the_order_the_menu_opens_with() {
        let mut seen: Vec<Shortcut> = Shortcut::all().collect();
        let count = seen.len();
        seen.sort_by_key(|action| format!("{action:?}"));
        seen.dedup();
        assert_eq!(seen.len(), count, "an action is in the table twice");
        // The page, the listing and the menu all read this order, so it is the order a
        // reader meets the actions in: the menu's own first group first.
        let first_four: Vec<Shortcut> = Shortcut::all().take(4).collect();
        assert_eq!(
            first_four,
            vec![Shortcut::Back, Shortcut::Forward, Shortcut::Copy, Shortcut::SelectAll],
        );
    }

    #[test]
    fn a_row_with_no_key_says_so_rather_than_leaving_the_cell_blank() {
        let rows = rows(Language::EnUs);
        let keyless = rows.iter().filter(|row| row.keys.is_empty()).count();
        let page = page(Language::EnUs);
        assert_eq!(page.matches("| None |").count(), keyless);
        assert!(keyless > 0, "this reader has actions no key reaches");
        assert!(keyless < rows.len(), "and actions some key does reach");
        // A key is printed as code, so it reads as a key rather than as a word.
        assert!(page.contains("| Copy | `Ctrl+C` |"), "{page}");
    }

    #[test]
    fn the_page_is_a_table_of_every_action_in_the_menu_s_own_words() {
        let page = page(Language::EnUs);
        assert!(page.starts_with("# Keyboard shortcuts\n\n| Action | Keys |\n| --- | --- |\n"));
        // The heading, the blank line, the header row, the rule, one line an action.
        assert_eq!(page.lines().count(), 4 + Shortcut::count());
        // An action that answers to two keys shows both, apart.
        assert!(page.contains("| Back | `Ctrl+Z` \u{00B7} `Alt+\u{2190}` |"), "{page}");
        // And the words are the menu's, not a second set written for this page.
        assert!(page.contains(&format!("| {} |", i18n::t(Language::EnUs, Key::MenuFind))));
        assert!(page.contains(&format!("| {} |", i18n::t(Language::EnUs, Key::MenuStartPage))));
    }

    #[test]
    fn every_action_is_named_in_every_language_this_reader_speaks() {
        for lang in Language::ALL {
            for row in rows(lang) {
                assert!(!row.label.trim().is_empty(), "an action has no name in {lang:?}");
                // A label still holding a template marker is a name nobody wrote.
                assert!(!row.label.contains('{'), "a name with a marker in it, in {lang:?}");
            }
        }
    }

    #[test]
    fn a_key_is_the_same_string_in_every_language_because_it_is_a_key() {
        let en = rows(Language::EnUs);
        for lang in Language::ALL {
            let other = rows(lang);
            for (a, b) in en.iter().zip(&other) {
                assert_eq!(a.keys, b.keys, "a key changed with the language, in {lang:?}");
            }
        }
        // And every key written is one chord: no blank offered as a key, and none holding a
        // space, which is what two keys written as one string would look like.
        for action in Shortcut::all() {
            for key in keys(action) {
                assert!(!key.trim().is_empty(), "{action:?} offers a blank key");
                assert!(!key.contains(' '), "{key:?} is two keys written as one");
                assert_eq!(key.trim(), *key, "{key:?} carries a space at an end");
            }
        }
    }

    #[test]
    fn the_menu_prints_keys_the_page_also_lists_and_never_one_it_does_not() {
        // The menu's accelerator column is this table's text, so a key that moved would
        // move in both or in neither. What the menu prints is a subset of what the page
        // lists, in the same order: a row cannot advertise a key the page does not name.
        let mut printed = 0;
        for action in Shortcut::all() {
            let hint = hint(action);
            let row = find(action).expect("every action is a row");
            if hint.is_empty() {
                assert!(row.menu.is_empty());
                continue;
            }
            printed += 1;
            let segments: Vec<&str> = hint.split('\t').skip(1).collect();
            assert_eq!(segments, row.menu, "{action:?} prints something other than its keys");
            let mut page_keys = keys(action).iter();
            for key in row.menu {
                assert!(
                    page_keys.any(|listed| listed == key),
                    "{action:?} prints {key:?}, which its page row does not list",
                );
            }
        }
        assert!(printed > 0, "some row of the menu prints a key");
        assert!(printed < Shortcut::count(), "and some action has no key to print");
        // A key that only turns a switch over is on the page and not in the menu.
        assert_eq!(hint(Shortcut::PageStack), "\tCtrl+4");
        assert_eq!(hint(Shortcut::ContinuousScroll), "", "the same key, not repeated");
        assert_eq!(hint(Shortcut::Copy), "\tCtrl+C");
    }

    #[test]
    fn the_listing_prints_every_row_and_counts_them() {
        let lines = describe(Language::EnUs);
        assert_eq!(lines.len(), Shortcut::count() + 1);
        assert_eq!(
            lines.last().map(String::as_str),
            Some(format!("shortcuts    : {} action(s)", Shortcut::count()).as_str()),
            "the count is the number of rows above it",
        );
        // A name, a tab, the keys: the page's two columns as two fields, on every row.
        assert!(lines.iter().take(Shortcut::count()).all(|line| line.contains('\t')), "{lines:?}");
        // An action with no key still holds its second field, marked with the one thing that
        // cannot be mistaken for one.
        let start = lines
            .iter()
            .find(|line| line.starts_with(i18n::t(Language::EnUs, Key::MenuStartPage)))
            .expect("the start page row");
        assert!(start.ends_with("\t-"), "{start}");
    }

    #[test]
    fn the_page_is_read_by_the_layout_the_window_already_has() {
        // A table whose row holds a stray pipe is a table with a broken row, so the writer
        // keeps its own promise: every line of the table has exactly the two cells it says.
        let page = page(Language::EnUs);
        for line in page.lines().skip(2) {
            assert!(line.starts_with("| ") && line.ends_with(" |"), "{line:?}");
            let pipes = line.matches('|').count();
            assert_eq!(pipes, 3 + line.matches("\\|").count(), "{line:?}");
        }
    }
}
