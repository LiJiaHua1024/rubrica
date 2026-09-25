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
    File(FileRef),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TabKind { Pinned, Preview }

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TabId(pub u64);

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
        (id, true)
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
        } else if self.active >= index {
            self.active = (index).min(self.items.len() - 1);
        }
        true
    }

    pub fn pin(&mut self, id: TabId) -> bool {
        let Some(index) = self.find(id) else { return false };
        if self.items[index].document == DocumentRef::Sample || self.items[index].kind == TabKind::Pinned { return false; }
        self.items[index].kind = TabKind::Pinned;
        true
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
    pub fn pin(&mut self, id: TabId) -> bool { self.tabs.pin(id) }
    pub fn move_tab(&mut self, id: TabId, to: usize) -> bool { self.tabs.move_tab(id, to) }
    pub fn clear_recent(&mut self) { self.recent.clear(); }
    pub fn remove_recent(&mut self, file: &FileRef) { self.recent.remove(file); }

    pub fn snapshot(&self) -> WorkspaceSnapshot {
        let tabs = self.tabs.items.iter().filter_map(|tab| match &tab.document {
            DocumentRef::File(file) => Some(SessionTab { document: file.clone(), kind: tab.kind }),
            DocumentRef::Sample => None,
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
        for tab in tabs {
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
}
