#![allow(dead_code)]

use rubrica_type::units::Pt;

pub type UnitId = usize;
pub type GroupId = usize;

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Owner {
    Body { block: usize },
    Note { note: usize, block: usize },
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Piece {
    Line { owner: Owner, line: usize },
    Rule { block: usize },
    TableHeader { table: usize },
    TableRow { table: usize, row: usize },
    NoteLine { note: usize, block: usize, line: usize },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Unit {
    pub piece: Piece,
    pub height: Pt,
    pub gap_before: Pt,
    pub group: GroupId,
    pub index_in_group: usize,
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GroupKind {
    Paragraph,
    Heading,
    Table,
    Note,
    Rule,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GroupPolicy {
    pub id: GroupId,
    pub kind: GroupKind,
    pub first: UnitId,
    pub len: usize,
    pub orphan: usize,
    pub widow: usize,
    /// Number of following units that must remain on the same page.
    pub keep_with_next: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TableMeta {
    pub table: usize,
    pub header: UnitId,
    pub rows: std::ops::Range<UnitId>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct NoteMeta {
    pub id: usize,
    pub label: String,
    pub first: UnitId,
    pub continuation_height: Pt,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Policy {
    pub orphan: usize,
    pub widow: usize,
    pub heading_follow_lines: usize,
    pub repeat_table_headers: bool,
    pub note_min_start_lines: usize,
}

impl Default for Policy {
    fn default() -> Self {
        Self {
            orphan: 2,
            widow: 2,
            heading_follow_lines: 2,
            repeat_table_headers: true,
            note_min_start_lines: 2,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PageBox {
    pub top: Pt,
    pub height: Pt,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlaceRole {
    Original,
    RepeatTableHeader,
    NoteContinuation,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlacedUnit {
    pub unit: UnitId,
    pub y: Pt,
    pub h: Pt,
    pub role: PlaceRole,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PlannedPage {
    pub top: Pt,
    pub used: Pt,
    pub items: Vec<PlacedUnit>,
    pub relaxed: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PagePlan {
    pub pages: Vec<PlannedPage>,
}

#[derive(Clone, Debug, Default)]
struct State {
    table_rows: Vec<TableMeta>,
    note_first: Vec<Option<UnitId>>,
    note_continuation_height: Vec<Pt>,
    groups: Vec<GroupPolicy>,
    policy: Policy,
}

impl State {
    fn new(input_groups: &[GroupPolicy], tables: &[TableMeta], notes: &[NoteMeta], policy: Policy) -> Self {
        let max_note = notes.iter().map(|n| n.id).max().map_or(0, |n| n + 1);
        let mut table_rows = Vec::with_capacity(tables.len());
        for table in tables {
            table_rows.push(table.clone());
        }
        let mut note_first = vec![None; max_note];
        let mut note_continuation_height = vec![0.0; max_note];
        for note in notes {
            if note.id < note_first.len() {
                note_first[note.id] = Some(note.first);
                note_continuation_height[note.id] = note.continuation_height;
            }
        }
        Self { table_rows, note_first, note_continuation_height, groups: input_groups.to_vec(), policy }
    }

    fn group(&self, id: GroupId) -> Option<&GroupPolicy> {
        self.groups.iter().find(|g| g.id == id)
    }

    fn table_for_row(&self, unit: UnitId, units: &[Unit]) -> Option<TableMeta> {
        let Piece::TableRow { table, .. } = units[unit].piece else { return None };
        self.table_rows.iter().find(|t| t.table == table).cloned()
    }

    fn continuation_height(&self, unit: UnitId, units: &[Unit]) -> Pt {
        match units[unit].piece {
            Piece::Line { owner: Owner::Note { note, .. }, .. } | Piece::NoteLine { note, .. }
                if self.note_first.get(note).copied().flatten().is_some_and(|first| unit > first) =>
            {
                self.note_continuation_height.get(note).copied().unwrap_or(0.0)
            }
            _ => 0.0,
        }
    }
}

fn piece_is_note(unit: &Unit) -> bool {
    matches!(unit.piece, Piece::Line { owner: Owner::Note { .. }, .. } | Piece::NoteLine { .. })
}

fn piece_note(unit: &Unit) -> Option<usize> {
    match unit.piece {
        Piece::Line { owner: Owner::Note { note, .. }, .. } | Piece::NoteLine { note, .. } => Some(note),
        _ => None,
    }
}

fn group_bounds(units: &[Unit], group: GroupId) -> (UnitId, UnitId) {
    let first = units.iter().position(|u| u.group == group).unwrap_or(0);
    let last = units.iter().rposition(|u| u.group == group).map_or(first, |i| i + 1);
    (first, last)
}

fn valid_boundary(state: &State, units: &[Unit], start: UnitId, end: UnitId) -> bool {
    if end <= start || end > units.len() {
        return false;
    }
    let first_group = units[start].group;
    let last_group = units[end - 1].group;
    if first_group == last_group {
        let Some(policy) = state.group(first_group) else { return false };
        let (group_first, group_last) = group_bounds(units, first_group);
        let left = start - group_first;
        let right = group_last - end;
        if left > 0 && left < policy.orphan {
            return false;
        }
        if right > 0 && right < policy.widow {
            return false;
        }
    }
    // Every heading whose group starts on this page must carry its requested following
    // units with it, even when one or more following paragraph lines are already after it.
    for unit in units.iter().take(end).skip(start) {
        let Some(policy) = state.group(unit.group) else { return false };
        if policy.kind != GroupKind::Heading {
            continue;
        }
        let (_, group_last) = group_bounds(units, unit.group);
        if end < group_last.saturating_add(policy.keep_with_next) {
            return false;
        }
    }
    // A table header and its first row travel together.
    if let Some(table) = state.table_for_row(start, units) {
        if table.header >= end {
            return false;
        }
    }
    true
}

fn overhead(state: &State, units: &[Unit], start: UnitId, page_has_header: &mut Option<usize>) -> Pt {
    let mut extra = 0.0;
    if let Some(table) = state.table_for_row(start, units) {
        if state.policy.repeat_table_headers
            && *page_has_header != Some(table.table)
            && table.header < start
        {
            extra += units[table.header].height;
            *page_has_header = Some(table.table);
        }
    }
    if piece_is_note(&units[start]) {
        if let Some(note) = piece_note(&units[start]) {
            if state.note_first.get(note).copied().flatten().is_some_and(|first| start > first) {
                extra += state.continuation_height(start, units);
            }
        }
    }
    extra
}

/// Place the original units in `range`, adding the table header/note continuation
/// decoration at the beginning of a page when needed.
fn place_range(
    state: &State,
    units: &[Unit],
    start: UnitId,
    end: UnitId,
    page_top: Pt,
    box_: PageBox,
    relaxed: bool,
) -> PlannedPage {
    let mut items = Vec::new();
    let mut used = 0.0;
    if let Some(table) = state.table_for_row(start, units) {
        if state.policy.repeat_table_headers && table.header < start {
            let h = units[table.header].height;
            items.push(PlacedUnit { unit: table.header, y: used, h, role: PlaceRole::RepeatTableHeader });
            used += h;
        }
    }
    if let Some(note) = piece_note(&units[start]) {
        if state.note_first.get(note).copied().flatten().is_some_and(|first| start > first) {
            let h = state.continuation_height(start, units);
            items.push(PlacedUnit { unit: start, y: used, h, role: NoteContinuationNote(note) });
            used += h;
        }
    }
    for (offset, unit) in units.iter().enumerate().take(end).skip(start) {
        if offset > start {
            used += unit.gap_before;
        }
        items.push(PlacedUnit { unit: offset, y: used, h: unit.height, role: PlaceRole::Original });
        used += unit.height;
    }
    PlannedPage { top: page_top, used, items, relaxed: relaxed || used > box_.height + 0.001 }
}

// A small private constructor keeps the public `PlaceRole` free of note-specific
// implementation details while still making the continuation visible to renderers.
#[allow(non_snake_case)]
fn NoteContinuationNote(note: usize) -> PlaceRole {
    // Notes are identified by their first unit; the renderer can recover the id from
    // the unit it is drawing. This variant is represented by the original unit role.
    let _ = note;
    PlaceRole::NoteContinuation
}

/// Split an already measured flow into deterministic pages.
pub fn paginate(input: &[Unit], groups: &[GroupPolicy], tables: &[TableMeta], notes: &[NoteMeta], box_: PageBox, policy: Policy) -> PagePlan {
    if input.is_empty() {
        return PagePlan { pages: vec![PlannedPage { top: box_.top, used: 0.0, items: Vec::new(), relaxed: false }] };
    }
    let state = State::new(groups, tables, notes, policy);
    let mut pages = Vec::new();
    let mut start = 0;
    let mut page_top = box_.top;
    while start < input.len() {
        let mut chosen = None;
        let mut end = start + 1;
        while end <= input.len() {
            let fits_page = {
                let mut used = 0.0;
                for (i, unit) in input.iter().enumerate().take(end).skip(start) {
                    if i > start { used += unit.gap_before; }
                    used += unit.height;
                }
                used + overhead(&state, input, start, &mut None) <= box_.height + 0.001
            };
            if fits_page && valid_boundary(&state, input, start, end) {
                chosen = Some(end);
            }
            end += 1;
        }
        let (end, relaxed) = match chosen {
            Some(end) => (end, false),
            None => {
                let group = input[start].group;
                let (_, group_end) = group_bounds(input, group);
                if group_end > start + 1 && state.group(group).is_some_and(|g| g.kind != GroupKind::Rule) {
                    (group_end, true)
                } else {
                    (start + 1, true)
                }
            }
        };
        let page = place_range(&state, input, start, end, page_top, box_, relaxed);
        page_top += box_.height;
        pages.push(page);
        start = end;
    }
    PagePlan { pages }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(height: f32, group: usize, index: usize) -> Unit {
        Unit {
            piece: Piece::Line { owner: Owner::Body { block: group }, line: index },
            height,
            gap_before: 0.0,
            group,
            index_in_group: index,
        }
    }

    fn group(id: usize, first: usize, len: usize, kind: GroupKind) -> GroupPolicy {
        GroupPolicy { id, kind, first, len, orphan: if matches!(kind, GroupKind::Table | GroupKind::Rule) { 0 } else { 2 }, widow: if matches!(kind, GroupKind::Table | GroupKind::Rule) { 0 } else { 2 }, keep_with_next: if matches!(kind, GroupKind::Heading) { 2 } else { 0 } }
    }

    #[test]
    fn each_unit_is_placed_once_and_short_paragraphs_stay_together() {
        let units = vec![line(40.0, 0, 0), line(40.0, 0, 1), line(40.0, 0, 2), line(40.0, 0, 3), line(40.0, 1, 0), line(40.0, 1, 1)];
        let groups = vec![group(0, 0, 4, GroupKind::Paragraph), group(1, 4, 2, GroupKind::Paragraph)];
        let plan = paginate(&units, &groups, &[], &[], PageBox { top: 0.0, height: 100.0 }, Policy::default());
        let placed: Vec<_> = plan.pages.iter().flat_map(|p| p.items.iter()).filter(|i| i.role == PlaceRole::Original).map(|i| i.unit).collect();
        assert_eq!(placed, (0..units.len()).collect::<Vec<_>>());
        assert!(plan.pages.iter().all(|p| p.used <= 100.0));
    }

    #[test]
    fn a_heading_moves_with_two_following_lines() {
        let units = vec![
            Unit { piece: Piece::Line { owner: Owner::Body { block: 0 }, line: 0 }, height: 30.0, gap_before: 0.0, group: 0, index_in_group: 0 },
            Unit { piece: Piece::Line { owner: Owner::Body { block: 1 }, line: 0 }, height: 30.0, gap_before: 0.0, group: 1, index_in_group: 0 },
            Unit { piece: Piece::Line { owner: Owner::Body { block: 1 }, line: 1 }, height: 30.0, gap_before: 0.0, group: 1, index_in_group: 1 },
            Unit { piece: Piece::Line { owner: Owner::Body { block: 1 }, line: 2 }, height: 30.0, gap_before: 0.0, group: 1, index_in_group: 2 },
            Unit { piece: Piece::Line { owner: Owner::Body { block: 2 }, line: 0 }, height: 30.0, gap_before: 0.0, group: 2, index_in_group: 0 },
            Unit { piece: Piece::Line { owner: Owner::Body { block: 2 }, line: 1 }, height: 30.0, gap_before: 0.0, group: 2, index_in_group: 1 },
        ];
        let groups = vec![
            group(0, 0, 1, GroupKind::Paragraph),
            GroupPolicy { id: 1, kind: GroupKind::Heading, first: 1, len: 1, orphan: 0, widow: 0, keep_with_next: 2 },
            group(2, 2, 3, GroupKind::Paragraph),
            group(3, 5, 1, GroupKind::Paragraph),
        ];
        let plan = paginate(&units, &groups, &[], &[], PageBox { top: 0.0, height: 100.0 }, Policy::default());
        assert!(plan.pages.len() > 1);
        assert!(plan.pages[1].items.iter().any(|i| i.unit == 1), "heading stayed behind at page bottom");
    }

    #[test]
    fn a_continuation_page_repeats_a_table_header() {
        let units = vec![
            Unit { piece: Piece::TableHeader { table: 0 }, height: 20.0, gap_before: 0.0, group: 0, index_in_group: 0 },
            Unit { piece: Piece::TableRow { table: 0, row: 0 }, height: 40.0, gap_before: 0.0, group: 1, index_in_group: 0 },
            Unit { piece: Piece::TableRow { table: 0, row: 1 }, height: 40.0, gap_before: 0.0, group: 2, index_in_group: 0 },
            Unit { piece: Piece::TableRow { table: 0, row: 2 }, height: 40.0, gap_before: 0.0, group: 3, index_in_group: 0 },
        ];
        let groups = (0..4).map(|i| group(i, i, 1, GroupKind::Table)).collect::<Vec<_>>();
        let tables = [TableMeta { table: 0, header: 0, rows: 1..4 }];
        let plan = paginate(&units, &groups, &tables, &[], PageBox { top: 0.0, height: 70.0 }, Policy::default());
        assert!(plan.pages.len() > 1);
        assert!(plan.pages[1].items.iter().any(|i| i.role == PlaceRole::RepeatTableHeader));
    }

    #[test]
    fn an_impossible_group_marks_the_page_relaxed_and_terminates() {
        let units = vec![line(200.0, 0, 0), line(20.0, 1, 0)];
        let groups = vec![group(0, 0, 1, GroupKind::Paragraph), group(1, 1, 1, GroupKind::Paragraph)];
        let plan = paginate(&units, &groups, &[], &[], PageBox { top: 0.0, height: 50.0 }, Policy::default());
        assert_eq!(plan.pages.len(), 2);
        assert!(plan.pages[0].relaxed);
    }
}
