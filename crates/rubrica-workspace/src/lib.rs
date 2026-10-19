//! Pure tab and recent-document state.
//!
//! The Windows shell owns file reads and rendering; this crate only decides which
//! documents are open, which one is active, and which paths were most recently used.

use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct FileRef {
    pub fingerprint: u64,
    pub path: PathBuf,
}

impl FileRef {
    pub fn new(path: impl Into<PathBuf>, fingerprint: u64) -> Self {
        Self { fingerprint, path: path.into() }
    }

    /// File identity for tabs and recent history. The fingerprint is a version stamp,
    /// not an identity: a save must not make the same path become a second tab.
    pub fn same_file(&self, other: &Self) -> bool {
        self.path == other.path
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DocumentRef {
    Sample,
    /// The page that lists the folders the reader keeps and the ones they were last in.
    /// It is made when it is shown rather than read from anywhere, so the session file
    /// does not carry it and a restart simply does not have one.
    Start,
    File(FileRef),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TabKind { Pinned, Preview }

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TabId(pub u64);

/// How many tabs a session keeps.
///
/// Recent documents are capped at ten; the tab strip was not, and every open tab is
/// written to the session file whole, so a reader working through a book grew a
/// strip -- and a file behind it -- that only ever got longer.
pub const MAX_TABS: usize = 32;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tab {
    pub id: TabId,
    pub document: DocumentRef,
    pub kind: TabKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TabSet {
    items: Vec<Tab>,
    active: usize,
    next_id: u64,
}

impl Default for TabSet {
    fn default() -> Self {
        Self { items: vec![Tab { id: TabId(0), document: DocumentRef::Sample, kind: TabKind::Pinned }], active: 0, next_id: 1 }
    }
}

impl TabSet {
    pub fn items(&self) -> &[Tab] { &self.items }
    pub fn active(&self) -> &Tab { &self.items[self.active] }
    pub fn active_mut(&mut self) -> &mut Tab { &mut self.items[self.active] }
    pub fn active_index(&self) -> usize { self.active }
    pub fn find(&self, id: TabId) -> Option<usize> { self.items.iter().position(|tab| tab.id == id) }
    pub fn find_file(&self, file: &FileRef) -> Option<TabId> {
        self.items
            .iter()
            .find(|tab| matches!(&tab.document, DocumentRef::File(existing) if existing.same_file(file)))
            .map(|tab| tab.id)
    }

    pub fn open_file(&mut self, file: FileRef, kind: TabKind) -> (TabId, bool) {
        if let Some(id) = self.find_file(&file) {
            let index = self.find(id).expect("found tab");
            self.active = index;
            if kind == TabKind::Pinned {
                self.items[index].kind = TabKind::Pinned;
            }
            return (id, false);
        }
        let id = TabId(self.next_id);
        self.next_id += 1;
        let index = if kind == TabKind::Preview {
            if let Some(old) = self.items.iter().position(|tab| tab.kind == TabKind::Preview) {
                self.items.remove(old);
                old
            } else {
                self.items.len()
            }
        } else {
            self.items.len()
        };
        self.items.insert(index, Tab { id, document: DocumentRef::File(file), kind });
        self.active = index;
        self.trim();
        (id, true)
    }

    /// Open the start page, or come back to the one that is already open: it is a page
    /// like any other in the strip, and a reader who asks for it twice wants one of it.
    pub fn open_start_page(&mut self) -> (TabId, bool) {
        if let Some(index) = self.items.iter().position(|tab| tab.document == DocumentRef::Start) {
            let id = self.items[index].id;
            self.active = index;
            return (id, false);
        }
        let id = TabId(self.next_id);
        self.next_id += 1;
        self.items.push(Tab { id, document: DocumentRef::Start, kind: TabKind::Pinned });
        self.active = self.items.len() - 1;
        self.trim();
        (id, true)
    }

    /// Hold the list within [`MAX_TABS`] by giving up the oldest tab it can.
    ///
    /// A tab the reader has not pinned is the first to go, and only a list of
    /// nothing else is shortened from its oldest end: the cap is a bound, and one a
    /// reader can lift by pinning everything is not a bound. Two tabs are never
    /// given up however long the list gets -- the one being read, whose selection
    /// would otherwise jump, and the sample, which is what the strip falls back to
    /// when everything else has been closed.
    fn trim(&mut self) {
        while self.items.len() > MAX_TABS {
            let reading = self.items[self.active].id;
            let oldest = self
                .items
                .iter()
                .position(|tab| tab.kind == TabKind::Preview && tab.id != reading)
                .or_else(|| {
                    self.items
                        .iter()
                        .position(|tab| tab.document != DocumentRef::Sample && tab.id != reading)
                });
            let Some(oldest) = oldest else { return };
            self.items.remove(oldest);
            if self.active > oldest {
                self.active -= 1;
            } else if self.active == oldest {
                self.active = self.active.min(self.items.len() - 1);
            }
        }
    }

    pub fn replace_document(&mut self, id: TabId, document: DocumentRef) -> bool {
        let Some(index) = self.find(id) else { return false };
        self.items[index].document = document;
        true
    }

    pub fn activate(&mut self, id: TabId) -> bool {
        let Some(index) = self.find(id) else { return false };
        self.active = index;
        true
    }

    pub fn close(&mut self, id: TabId) -> bool {
        let Some(index) = self.find(id) else { return false };
        self.items.remove(index);
        if self.items.is_empty() {
            self.items.push(Tab { id: TabId(self.next_id), document: DocumentRef::Sample, kind: TabKind::Pinned });
            self.next_id += 1;
            self.active = 0;
        } else if self.active == index {
            // The active tab itself closed: its right neighbour takes over, and at
            // the right edge the selection falls back to the last tab left.
            self.active = index.min(self.items.len() - 1);
        } else if self.active > index {
            // A tab left of the active one closed: the active tab itself slid one
            // place down, and keeps the selection rather than handing it away.
            self.active -= 1;
        }
        true
    }

    pub fn pin(&mut self, id: TabId) -> bool {
        let Some(index) = self.find(id) else { return false };
        if self.items[index].document == DocumentRef::Sample || self.items[index].kind == TabKind::Pinned { return false; }
        self.items[index].kind = TabKind::Pinned;
        true
    }

    /// Close every tab but this one. It is the tab the reader named that survives, not
    /// whichever one happened to be showing: "the others" is said about a tab.
    pub fn close_others(&mut self, id: TabId) -> bool {
        if self.find(id).is_none() { return false; }
        self.items.retain(|tab| tab.id == id);
        self.active = 0;
        true
    }

    /// Close every tab, and leave the sample document where the last of them stood --
    /// the same end a reader reaches by closing them one at a time.
    pub fn close_all(&mut self) {
        self.items.clear();
        self.items.push(Tab { id: TabId(self.next_id), document: DocumentRef::Sample, kind: TabKind::Pinned });
        self.next_id += 1;
        self.active = 0;
    }

    pub fn move_tab(&mut self, id: TabId, to: usize) -> bool {
        let Some(from) = self.find(id) else { return false };
        let to = to.min(self.items.len() - 1);
        if from == to { return true; }
        let tab = self.items.remove(from);
        self.items.insert(to, tab);
        self.active = if self.active == from { to } else if from < self.active && to >= self.active { self.active - 1 } else if from > self.active && to <= self.active { self.active + 1 } else { self.active };
        true
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecentFiles {
    items: Vec<FileRef>,
    limit: usize,
}

impl Default for RecentFiles {
    fn default() -> Self { Self { items: Vec::new(), limit: 10 } }
}

impl RecentFiles {
    pub fn items(&self) -> &[FileRef] { &self.items }
    pub fn touch(&mut self, file: FileRef) {
        self.items.retain(|item| !item.same_file(&file));
        self.items.insert(0, file);
        self.items.truncate(self.limit);
    }
    pub fn remove(&mut self, file: &FileRef) { self.items.retain(|item| !item.same_file(file)); }
    pub fn clear(&mut self) { self.items.clear(); }
}

/// How many folders a reader can keep as favourites.
///
/// A favourite is a shelf, not a history: it is put there by hand, so the cap is what
/// keeps a shelf a shelf. A reader who wants more than this is looking for a bookmark
/// file, which is a different feature.
pub const MAX_FAVOURITES: usize = 24;

/// How many folders are remembered as recently used, the way documents are.
pub const MAX_RECENT_FOLDERS: usize = 10;

/// A folder the reader has been to.
///
/// Identity is the path, as it is for a file: a folder that moved is a folder the reader
/// has not been to, and a stale entry that cannot be opened is dropped by the host.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct FolderRef {
    pub path: PathBuf,
}

impl FolderRef {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn same_folder(&self, other: &Self) -> bool {
        self.path == other.path
    }
}

/// The folders a reader keeps, and the folders they were last in.
///
/// The two lists are deliberately separate and are allowed to overlap: a favourite is a
/// promise the reader made, a recent folder is a note the program kept, and one must not
/// quietly edit the other.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Places {
    favourites: Vec<FolderRef>,
    recent: Vec<FolderRef>,
    favourite_limit: usize,
    recent_limit: usize,
}

impl Default for Places {
    fn default() -> Self {
        Self {
            favourites: Vec::new(),
            recent: Vec::new(),
            favourite_limit: MAX_FAVOURITES,
            recent_limit: MAX_RECENT_FOLDERS,
        }
    }
}

impl Places {
    pub fn favourites(&self) -> &[FolderRef] { &self.favourites }
    pub fn recent(&self) -> &[FolderRef] { &self.recent }

    pub fn is_favourite(&self, folder: &FolderRef) -> bool {
        self.favourites.iter().any(|item| item.same_folder(folder))
    }

    /// Put a folder on the shelf, newest first. A folder already there keeps the place
    /// it had: adding a favourite twice is one intention, not two.
    pub fn favourite(&mut self, folder: FolderRef) -> bool {
        if self.is_favourite(&folder) { return false; }
        self.favourites.insert(0, folder);
        self.favourites.truncate(self.favourite_limit);
        true
    }

    pub fn unfavourite(&mut self, folder: &FolderRef) -> bool {
        let before = self.favourites.len();
        self.favourites.retain(|item| !item.same_folder(folder));
        self.favourites.len() != before
    }

    /// Add or remove, and say which it did: the menu row is one row whose label depends
    /// on the answer.
    pub fn toggle_favourite(&mut self, folder: FolderRef) -> bool {
        if self.unfavourite(&folder) { return false; }
        self.favourite(folder);
        true
    }

    pub fn touch_recent(&mut self, folder: FolderRef) {
        self.recent.retain(|item| !item.same_folder(&folder));
        self.recent.insert(0, folder);
        self.recent.truncate(self.recent_limit);
    }

    pub fn forget_recent(&mut self, folder: &FolderRef) -> bool {
        let before = self.recent.len();
        self.recent.retain(|item| !item.same_folder(folder));
        self.recent.len() != before
    }

    pub fn clear_recent(&mut self) { self.recent.clear(); }

    pub fn clear_favourites(&mut self) { self.favourites.clear(); }

    /// The favourites that are no longer on disk, in the order they are kept.
    ///
    /// The state crate does not know what a file system can see, so the host asks the
    /// question and gets the answer back rather than the crate guessing.
    pub fn missing<F>(&self, exists: F) -> Vec<FolderRef>
    where
        F: Fn(&Path) -> bool,
    {
        self.favourites.iter().filter(|item| !exists(&item.path)).cloned().collect()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionTab { pub document: FileRef, pub kind: TabKind }

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionSnapshot { pub tabs: Vec<SessionTab>, pub active: Option<DocumentRef> }

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkspaceSnapshot { pub session: SessionSnapshot, pub recent: Vec<FileRef> }

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Workspace { pub tabs: TabSet, pub recent: RecentFiles }

impl Workspace {
    pub fn open_file(&mut self, file: FileRef, kind: TabKind) -> (TabId, bool) {
        let result = self.tabs.open_file(file.clone(), kind);
        self.recent.touch(file);
        result
    }
    pub fn activate(&mut self, id: TabId) -> bool { self.tabs.activate(id) }
    pub fn replace_document(&mut self, id: TabId, document: DocumentRef) -> bool {
        self.tabs.replace_document(id, document)
    }
    pub fn close(&mut self, id: TabId) -> bool { self.tabs.close(id) }
    pub fn close_others(&mut self, id: TabId) -> bool { self.tabs.close_others(id) }
    pub fn close_all(&mut self) { self.tabs.close_all() }
    /// Open the start page, or come back to the one that is already open.
    pub fn open_start_page(&mut self) -> (TabId, bool) { self.tabs.open_start_page() }
    pub fn pin(&mut self, id: TabId) -> bool { self.tabs.pin(id) }
    pub fn move_tab(&mut self, id: TabId, to: usize) -> bool { self.tabs.move_tab(id, to) }
    pub fn clear_recent(&mut self) { self.recent.clear(); }
    pub fn remove_recent(&mut self, file: &FileRef) { self.recent.remove(file); }

    pub fn snapshot(&self) -> WorkspaceSnapshot {
        let tabs = self.tabs.items.iter().filter_map(|tab| match &tab.document {
            DocumentRef::File(file) => Some(SessionTab { document: file.clone(), kind: tab.kind }),
            DocumentRef::Sample | DocumentRef::Start => None,
        }).collect();
        WorkspaceSnapshot {
            session: SessionSnapshot {
                tabs,
                active: Some(self.tabs.active().document.clone()),
            },
            recent: self.recent.items.clone(),
        }
    }

    pub fn restore(snapshot: WorkspaceSnapshot) -> Self {
        Self::restore_inner(snapshot, |_| true, false)
    }

    /// Restore a snapshot while letting the host discard paths that are no longer there.
    ///
    /// The state crate deliberately does not know what a file system can see. The host
    /// supplies that small bit of policy, so a remembered tab can be treated as a hint
    /// without teaching the pure state layer how to inspect a path.
    pub fn restore_with<F>(snapshot: WorkspaceSnapshot, available: F) -> Self
    where
        F: Fn(&Path) -> bool,
    {
        Self::restore_inner(snapshot, available, true)
    }

    fn restore_inner<F>(snapshot: WorkspaceSnapshot, available: F, fallback_to_sample: bool) -> Self
    where
        F: Fn(&Path) -> bool,
    {
        let WorkspaceSnapshot { session: SessionSnapshot { tabs, active }, recent } = snapshot;
        let mut workspace = Self::default();
        for file in recent.into_iter().rev() {
            if available(&file.path) {
                workspace.recent.touch(file);
            }
        }
        // A session file is data, not a promise: one written by a build that counted
        // differently, or trimmed by hand, may name more tabs than the cap allows.
        // The newest are the ones a reader was last looking at, so the oldest go.
        let keep_from = tabs.len().saturating_sub(MAX_TABS);
        for tab in tabs.into_iter().skip(keep_from) {
            if available(&tab.document.path) {
                workspace.tabs.open_file(tab.document, tab.kind);
            }
        }

        let sample = workspace
            .tabs
            .items()
            .iter()
            .position(|tab| tab.document == DocumentRef::Sample);
        let active = match active {
            Some(DocumentRef::File(file)) if available(&file.path) => workspace
                .tabs
                .items()
                .iter()
                .position(|tab| matches!(&tab.document, DocumentRef::File(existing) if existing.same_file(&file))),
            Some(DocumentRef::Sample) => sample,
            // The start page is made rather than remembered, so a session that names it
            // is a session whose page is gone: the sample is what is left to show.
            Some(DocumentRef::Start) => None,
            None if !fallback_to_sample => None,
            _ if fallback_to_sample => sample,
            _ => None,
        };
        if let Some(index) = active.or(if fallback_to_sample { sample } else { None }) {
            workspace.tabs.active = index;
        }
        workspace
    }
}

/// Convenience for callers that only have a path. The host supplies the identity
/// fingerprint because canonicalization rules belong to the Windows adapter.
pub fn file_ref(path: impl AsRef<Path>, fingerprint: u64) -> FileRef {
    FileRef::new(path.as_ref(), fingerprint)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(name: &str) -> FileRef { FileRef::new(name, name.len() as u64) }

    #[test]
    fn the_same_file_never_gets_two_tabs() {
        let mut workspace = Workspace::default();
        let (first, opened) = workspace.open_file(file("a.md"), TabKind::Pinned);
        let (second, opened_again) = workspace.open_file(file("a.md"), TabKind::Pinned);
        assert!(opened && !opened_again && first == second);
        assert_eq!(workspace.tabs.items().len(), 2);
    }

    #[test]
    fn a_new_file_stamp_does_not_create_a_second_tab() {
        let mut workspace = Workspace::default();
        let (first, _) = workspace.open_file(FileRef::new("book.md", 1), TabKind::Pinned);
        let (second, opened) = workspace.open_file(FileRef::new("book.md", 2), TabKind::Pinned);
        assert!(!opened);
        assert_eq!(first, second);
        assert_eq!(workspace.tabs.items().len(), 2);
    }

    #[test]
    fn replacing_a_document_keeps_the_tab_identity() {
        let mut workspace = Workspace::default();
        let (id, _) = workspace.open_file(file("old.md"), TabKind::Pinned);
        assert!(workspace.replace_document(id, DocumentRef::File(file("new.md"))));
        assert_eq!(workspace.tabs.active().document, DocumentRef::File(file("new.md")));
        assert!(workspace.tabs.find(id).is_some());
    }

    #[test]
    fn a_preview_is_replaced_but_pinned_tabs_are_not() {
        let mut workspace = Workspace::default();
        let (preview, _) = workspace.open_file(file("a.md"), TabKind::Preview);
        let (pinned, _) = workspace.open_file(file("b.md"), TabKind::Pinned);
        let (_, _) = workspace.open_file(file("c.md"), TabKind::Preview);
        assert!(workspace.tabs.find(preview).is_none());
        assert!(workspace.tabs.find(pinned).is_some());
    }

    #[test]
    fn closing_the_active_tab_prefers_the_right_neighbor() {
        let mut workspace = Workspace::default();
        let (a, _) = workspace.open_file(file("a"), TabKind::Pinned);
        let (b, _) = workspace.open_file(file("b"), TabKind::Pinned);
        let (c, _) = workspace.open_file(file("c"), TabKind::Pinned);
        workspace.activate(a);
        workspace.close(a);
        assert_eq!(workspace.tabs.active().id, b);
        workspace.activate(c);
        workspace.close(c);
        assert_eq!(workspace.tabs.active().id, b);
    }

    #[test]
    fn recent_files_deduplicate_and_stay_bounded() {
        let mut workspace = Workspace::default();
        for i in 0..12 { workspace.open_file(FileRef::new(format!("{i}.md"), i), TabKind::Pinned); }
        workspace.open_file(file("7.md"), TabKind::Pinned);
        assert_eq!(workspace.recent.items()[0], file("7.md"));
        assert_eq!(workspace.recent.items().len(), 10);
        assert!(!workspace.recent.items().contains(&file("0.md")));
    }

    #[test]
    fn restoring_recent_files_keeps_their_newest_first_order() {
        let mut workspace = Workspace::default();
        for name in ["a", "b", "c"] {
            workspace.open_file(file(name), TabKind::Pinned);
        }
        let restored = Workspace::restore(workspace.snapshot());
        assert_eq!(restored.recent.items(), workspace.recent.items());
    }

    #[test]
    fn restoring_drops_unavailable_tabs_and_recent_files() {
        let mut workspace = Workspace::default();
        workspace.open_file(file("gone.md"), TabKind::Pinned);
        workspace.open_file(file("kept.md"), TabKind::Preview);
        workspace.recent.touch(file("also-gone.md"));

        let restored = Workspace::restore_with(workspace.snapshot(), |path| {
            path == Path::new("kept.md")
        });
        assert_eq!(restored.tabs.items().len(), 2);
        assert!(restored.tabs.items().iter().skip(1).all(|tab| {
            matches!(&tab.document, DocumentRef::File(file) if file.path == Path::new("kept.md"))
        }));
        assert_eq!(restored.recent.items(), &[file("kept.md")]);
    }

    #[test]
    fn restoring_an_unavailable_active_document_falls_back_to_sample() {
        let mut workspace = Workspace::default();
        let (gone, _) = workspace.open_file(file("gone.md"), TabKind::Pinned);
        workspace.open_file(file("kept.md"), TabKind::Preview);
        workspace.activate(gone);

        let restored = Workspace::restore_with(workspace.snapshot(), |path| {
            path == Path::new("kept.md")
        });
        assert_eq!(restored.tabs.active().document, DocumentRef::Sample);
        assert_eq!(restored.tabs.items().len(), 2);
    }

    #[test]
    fn snapshot_restores_order_and_active_index_without_tab_ids() {
        let mut workspace = Workspace::default();
        workspace.open_file(file("a"), TabKind::Pinned);
        workspace.open_file(file("b"), TabKind::Preview);
        workspace.open_file(file("c"), TabKind::Pinned);
        let snapshot = workspace.snapshot();
        let restored = Workspace::restore(snapshot.clone());
        assert_eq!(restored.snapshot().session.tabs, snapshot.session.tabs);
        assert_eq!(restored.snapshot().session.active, snapshot.session.active);
        assert_eq!(restored.tabs.active().document, workspace.tabs.active().document);
    }

    #[test]
    fn restoring_without_an_active_marker_keeps_the_legacy_last_tab() {
        let mut workspace = Workspace::default();
        workspace.open_file(file("a"), TabKind::Pinned);
        workspace.open_file(file("b"), TabKind::Pinned);
        let mut snapshot = workspace.snapshot();
        snapshot.session.active = None;

        let restored = Workspace::restore(snapshot);
        assert_eq!(restored.tabs.active().document, workspace.tabs.active().document);
    }

    #[test]
    fn pin_and_move_do_not_change_the_active_document() {
        let mut workspace = Workspace::default();
        let (a, _) = workspace.open_file(file("a"), TabKind::Preview);
        let (b, _) = workspace.open_file(file("b"), TabKind::Pinned);
        workspace.activate(a);
        assert!(workspace.pin(a));
        assert!(workspace.move_tab(a, 0));
        assert_eq!(workspace.tabs.active().id, a);
        assert!(workspace.tabs.find(b).is_some());
    }

    #[test]
    fn closing_the_others_keeps_the_tab_that_was_named() {
        let mut workspace = Workspace::default();
        let (a, _) = workspace.open_file(file("a"), TabKind::Pinned);
        let (b, _) = workspace.open_file(file("b"), TabKind::Pinned);
        let (c, _) = workspace.open_file(file("c"), TabKind::Pinned);
        workspace.activate(a);
        assert!(workspace.close_others(b));
        assert_eq!(workspace.tabs.items().len(), 1);
        assert_eq!(workspace.tabs.active().id, b);
        assert!(workspace.tabs.find(c).is_none());
    }

    #[test]
    fn closing_the_others_of_a_tab_that_is_gone_does_nothing() {
        let mut workspace = Workspace::default();
        let (a, _) = workspace.open_file(file("a"), TabKind::Pinned);
        workspace.close(a);
        assert!(!workspace.close_others(a));
        assert_eq!(workspace.tabs.items().len(), 1);
    }

    #[test]
    fn closing_every_tab_leaves_the_sample_the_way_the_last_close_does() {
        let mut workspace = Workspace::default();
        workspace.open_file(file("a"), TabKind::Pinned);
        workspace.open_file(file("b"), TabKind::Pinned);
        workspace.close_all();
        assert_eq!(workspace.tabs.items().len(), 1);
        assert_eq!(workspace.tabs.active().document, DocumentRef::Sample);
        assert_eq!(workspace.tabs.active_index(), 0);
    }

    #[test]
    fn closing_a_tab_left_of_the_active_one_keeps_the_active_tab() {
        // The tab that closed slid the active one down an index; the selection
        // belongs to the tab the reader was reading, not to whatever now sits
        // where the closed one was.
        let mut workspace = Workspace::default();
        let (a, _) = workspace.open_file(file("a"), TabKind::Pinned);
        let (_b, _) = workspace.open_file(file("b"), TabKind::Pinned);
        let (c, _) = workspace.open_file(file("c"), TabKind::Pinned);
        workspace.activate(c);
        workspace.close(a);
        assert_eq!(workspace.tabs.active().id, c);
        workspace.close(_b);
        assert_eq!(workspace.tabs.active().id, c);
        // The active-tab case still prefers the right neighbour, then falls back.
        let (d, _) = workspace.open_file(file("d"), TabKind::Pinned);
        workspace.activate(d);
        workspace.close(d);
        assert_eq!(workspace.tabs.active().id, c);
    }

    #[test]
    fn the_tab_list_stops_at_the_cap_and_keeps_what_is_being_read() {
        // Every open tab is written to the session file whole, so a list with no
        // ceiling is a session file that only ever gets longer.
        let mut workspace = Workspace::default();
        let mut last = None;
        for i in 0..(MAX_TABS * 2) as u64 {
            let (id, _) = workspace.open_file(FileRef::new(format!("{i}.md"), i), TabKind::Pinned);
            assert!(workspace.tabs.items().len() <= MAX_TABS, "after {} files", i + 1);
            // The tab the reader is on is the last one opened, and stays: the cap
            // takes the oldest, not the one whose selection is live.
            assert_eq!(workspace.tabs.active().id, id, "after {} files", i + 1);
            assert!(workspace.tabs.find(id).is_some(), "the id handed back must name a tab");
            last = Some(id);
        }
        assert_eq!(workspace.tabs.find(last.unwrap()), Some(workspace.tabs.active_index()));
        // The sample is the strip's floor: it is never what the cap gives up.
        assert!(workspace.tabs.items().iter().any(|t| t.document == DocumentRef::Sample));
    }

    #[test]
    fn an_unpinned_tab_is_given_up_before_a_pinned_one() {
        // The preview slot is the one the reader has not committed to, so it goes
        // first; the pinned tabs around it are the ones that were asked for.
        let mut workspace = Workspace::default();
        for i in 0..(MAX_TABS - 1) as u64 {
            workspace.open_file(FileRef::new(format!("{i}.md"), i), TabKind::Pinned);
        }
        assert_eq!(workspace.tabs.items().len(), MAX_TABS);
        let (preview, _) = workspace.open_file(file("later.md"), TabKind::Preview);
        workspace.open_file(file("last.md"), TabKind::Pinned);
        assert_eq!(workspace.tabs.items().len(), MAX_TABS, "past the cap, something goes");
        assert!(workspace.tabs.find(preview).is_none(), "the unpinned tab is the one that goes");
    }

    #[test]
    fn a_session_file_naming_more_tabs_than_the_cap_allows_is_clamped() {
        // A session file is data, not a promise: one written by a build that counted
        // differently, or edited by hand, may hold more than the ceiling.
        let last = (MAX_TABS * 2 - 1) as u64;
        let tabs = (0..=last)
            .map(|i| SessionTab { document: FileRef::new(format!("{i}.md"), i), kind: TabKind::Pinned })
            .collect();
        let snapshot = WorkspaceSnapshot {
            session: SessionSnapshot {
                tabs,
                active: Some(DocumentRef::File(FileRef::new(format!("{last}.md"), last))),
            },
            recent: Vec::new(),
        };
        let restored = Workspace::restore(snapshot);
        assert_eq!(restored.tabs.items().len(), MAX_TABS, "an unbounded strip came back");
        let active = &restored.tabs.active().document;
        assert!(
            matches!(active, DocumentRef::File(f) if f.path == std::path::Path::new(&format!("{last}.md"))),
            "the tab the file marked active: {active:?}"
        );
        // The clamp drops the oldest, so the tab the file marked active is one of
        // the ones that came back and the index still names a tab that exists.
        assert!(restored.tabs.active_index() < restored.tabs.items().len());
    }

    fn folder(name: &str) -> FolderRef { FolderRef::new(name) }

    #[test]
    fn a_folder_favourited_twice_is_on_the_shelf_once() {
        let mut places = Places::default();
        assert!(places.favourite(folder("a")), "the first one is news");
        assert!(!places.favourite(folder("a")), "the second is the same intention");
        assert_eq!(places.favourites().len(), 1);
        assert!(places.is_favourite(&folder("a")));
        assert!(!places.is_favourite(&folder("b")));
    }

    #[test]
    fn the_shelf_keeps_the_newest_and_stops_at_the_cap() {
        let mut places = Places::default();
        for i in 0..(MAX_FAVOURITES + 6) {
            places.favourite(folder(&i.to_string()));
        }
        assert_eq!(places.favourites().len(), MAX_FAVOURITES, "an unbounded shelf came back");
        assert_eq!(places.favourites()[0].path, Path::new(&(MAX_FAVOURITES + 5).to_string()));
        assert!(!places.is_favourite(&folder("0")), "the oldest favourite goes first");
        assert!(places.is_favourite(&folder(&(MAX_FAVOURITES + 6 - MAX_FAVOURITES).to_string())),
            "the last one the cap still has room for stays");
    }

    #[test]
    fn taking_a_folder_off_the_shelf_says_whether_it_was_there() {
        let mut places = Places::default();
        places.favourite(folder("a"));
        assert!(places.unfavourite(&folder("a")));
        assert!(!places.unfavourite(&folder("a")), "the second removal changes nothing");
        assert!(!places.unfavourite(&folder("never")), "one that was never there");
        assert!(places.favourites().is_empty());
    }

    #[test]
    fn the_toggle_row_reports_the_state_it_left_behind() {
        let mut places = Places::default();
        assert!(places.toggle_favourite(folder("a")), "on");
        assert!(places.is_favourite(&folder("a")));
        assert!(!places.toggle_favourite(folder("a")), "off");
        assert!(!places.is_favourite(&folder("a")));
    }

    #[test]
    fn recent_folders_deduplicate_and_stay_bounded() {
        let mut places = Places::default();
        for i in 0..(MAX_RECENT_FOLDERS + 2) {
            places.touch_recent(folder(&i.to_string()));
        }
        assert_eq!(places.recent().len(), MAX_RECENT_FOLDERS);
        places.touch_recent(folder("0"));
        assert_eq!(places.recent().len(), MAX_RECENT_FOLDERS, "a revisit is not a new folder");
        assert_eq!(places.recent()[0].path, Path::new("0"), "the one just visited is first");
    }

    #[test]
    fn a_favourite_and_a_recent_folder_are_two_lists() {
        // A favourite is a promise the reader made; a recent folder is a note the
        // program kept. Neither is allowed to edit the other.
        let mut places = Places::default();
        places.favourite(folder("a"));
        places.touch_recent(folder("a"));
        assert!(places.forget_recent(&folder("a")));
        assert!(places.is_favourite(&folder("a")), "forgetting where I was is not unshelving it");
        places.clear_recent();
        assert_eq!(places.recent().len(), 0);
        assert_eq!(places.favourites().len(), 1);
        places.clear_favourites();
        assert!(places.favourites().is_empty());
    }

    #[test]
    fn the_folders_that_are_gone_are_the_ones_the_host_cannot_see() {
        let mut places = Places::default();
        places.favourite(folder("here"));
        places.favourite(folder("gone"));
        let missing = places.missing(|path| path == Path::new("here"));
        assert_eq!(missing.len(), 1);
        assert_eq!(missing[0].path, Path::new("gone"));
        assert_eq!(places.favourites().len(), 2, "asking is not the same as removing");
    }

    #[test]
    fn a_folder_path_is_its_own_identity() {
        assert!(folder("a").same_folder(&folder("a")));
        assert!(!folder("a").same_folder(&folder("A")));
    }

    #[test]
    fn the_start_page_is_one_tab_however_often_it_is_asked_for() {
        let mut workspace = Workspace::default();
        let (first, opened) = workspace.open_start_page();
        assert!(opened, "the first ask makes the page");
        assert_eq!(workspace.tabs.items().len(), 2, "the sample and the page");
        assert_eq!(workspace.tabs.active().document, DocumentRef::Start);
        let (again, opened_again) = workspace.open_start_page();
        assert_eq!(again, first, "the same page, not a second one");
        assert!(!opened_again);
        assert_eq!(workspace.tabs.items().len(), 2);
    }

    #[test]
    fn a_session_does_not_carry_the_start_page() {
        // The page is made when it is shown, so there is nothing in it to write down and
        // nothing to find when the reader comes back: the sample is what is left.
        let mut workspace = Workspace::default();
        workspace.open_start_page();
        let snapshot = workspace.snapshot();
        assert!(snapshot.session.tabs.is_empty(), "the page is not a document");
        assert_eq!(snapshot.session.active, Some(DocumentRef::Start));
        let restored = Workspace::restore(snapshot);
        assert!(!restored.tabs.items().iter().any(|tab| tab.document == DocumentRef::Start));
        assert_eq!(restored.tabs.active().document, DocumentRef::Sample);
    }
}
