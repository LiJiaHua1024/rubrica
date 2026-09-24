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
        self.items.iter().find(|tab| matches!(&tab.document, DocumentRef::File(existing) if existing == file)).map(|tab| tab.id)
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
        self.items.retain(|item| item != &file);
        self.items.insert(0, file);
        self.items.truncate(self.limit);
    }
    pub fn remove(&mut self, file: &FileRef) { self.items.retain(|item| item != file); }
    pub fn clear(&mut self) { self.items.clear(); }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionTab { pub document: FileRef, pub kind: TabKind }

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionSnapshot { pub tabs: Vec<SessionTab>, pub active: usize }

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
        WorkspaceSnapshot { session: SessionSnapshot { tabs, active: self.tabs.active_index().min(self.tabs.items.len().saturating_sub(1)) }, recent: self.recent.items.clone() }
    }

    pub fn restore(snapshot: WorkspaceSnapshot) -> Self {
        let mut workspace = Self::default();
        workspace.recent.items = snapshot.recent.into_iter().take(workspace.recent.limit).collect();
        for tab in snapshot.session.tabs {
            workspace.tabs.open_file(tab.document, tab.kind);
        }
        if !workspace.tabs.items.is_empty() {
            workspace.tabs.active = snapshot.session.active.min(workspace.tabs.items.len() - 1);
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
