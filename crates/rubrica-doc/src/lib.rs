//! Markdown to styled, typesettable blocks.
//!
//! Deliberately not an HTML tree: the layout engine wants, per block, a string to
//! set and the inline style runs inside it, so nodes can be cut at both UAX #14
//! opportunities and style boundaries. An element tree would just be undone.
//!
//! Blocks carry *their own* text rather than a range into the source, because two
//! parts of Markdown's line model cannot survive a plain slice:
//!
//! * a soft line break in the source continues the same paragraph, so it must
//!   become a space -- slicing it verbatim would make the engine treat every
//!   wrapped line of prose as a hard break;
//! * a list marker is not in the source at all, and prefixing one would shift
//!   every span offset.
//!
//! So normalisation happens once, here, and `Span` ranges are always byte offsets
//! into [`Block::text`].

use pulldown_cmark::{
    CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd,
};

/// Byte range into [`Block::text`], plus the style it carries.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Span {
    pub range: std::ops::Range<usize>,
    pub style: InlineStyle,
}

/// Character-level decoration. Block-level scale and colour come from
/// [`BlockKind`], so a heading is not also "just a big paragraph".
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct InlineStyle(pub u8);

impl InlineStyle {
    pub const EMPTY: InlineStyle = InlineStyle(0);
    pub const EMPHASIS: InlineStyle = InlineStyle(1 << 0);
    pub const STRONG: InlineStyle = InlineStyle(1 << 1);
    pub const CODE: InlineStyle = InlineStyle(1 << 2);
    pub const LINK: InlineStyle = InlineStyle(1 << 3);
    pub const STRIKETHROUGH: InlineStyle = InlineStyle(1 << 4);
    /// Marks an inline object placeholder rather than a decoration. The character
    /// in the span is U+FFFC; what it stands for lives in [`Block::objects`].
    pub const OBJECT: InlineStyle = InlineStyle(1 << 5);
    /// Raised text that belongs to the word before it: a footnote's citation number.
    ///
    /// A position rather than an emphasis, which is why it is its own bit: nothing
    /// about the *voice* of the digits changes, only where they sit on the line.
    pub const SUPERSCRIPT: InlineStyle = InlineStyle(1 << 6);

    #[inline]
    pub const fn bits(self) -> u8 {
        self.0
    }
    #[inline]
    pub const fn contains(self, other: InlineStyle) -> bool {
        self.0 & other.0 == other.0
    }
    #[inline]
    pub fn insert(&mut self, other: InlineStyle) {
        self.0 |= other.0;
    }
    #[inline]
    pub fn remove(&mut self, other: InlineStyle) {
        self.0 &= !other.0;
    }
}

impl std::ops::BitOr for InlineStyle {
    type Output = InlineStyle;
    #[inline]
    fn bitor(self, rhs: InlineStyle) -> InlineStyle {
        InlineStyle(self.0 | rhs.0)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlockKind {
    Paragraph,
    /// 1..=6
    Heading(u8),
    /// Fenced or indented code. Always set ragged, never justified.
    Code,
    /// `<hr>`: no text, drawn as a rule.
    Rule,
    /// A GFM table. Its text lives in [`Block::table`], not in [`Block::text`],
    /// because a cell is not a position in the document's reading order.
    Table,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ListInfo {
    pub ordered: bool,
    /// 0 for the outermost list.
    pub depth: u8,
    /// 1-based for ordered lists; `None` for a bullet.
    pub index: Option<u64>,
}

#[derive(Clone, Debug)]
pub struct Block {
    pub kind: BlockKind,
    pub text: String,
    pub spans: Vec<Span>,
    /// How many block quotes contain this block.
    pub quote_depth: u8,
    pub list: Option<ListInfo>,
    /// The depth of the list whose item contains this block, if any.
    ///
    /// A Markdown item can hold several blocks, and only the one that opens it carries
    /// a marker: a loose item's second paragraph, its fenced code, its own nested list
    /// all belong to the item's *text* column rather than to the page. A continuation
    /// block has [`Block::list`] `None`, so without this it would sit out at the margin
    /// beside the marker instead of under the text it goes on with.
    pub item_depth: Option<u8>,
    /// A list item carrying a checkbox, with its state.
    pub task: Option<bool>,
    /// Set for [`BlockKind::Table`].
    pub table: Option<Table>,
    /// Inline objects: the span they occupy in [`Block::text`], and their target.
    ///
    /// An image is set as a single object-replacement character so the layout
    /// engine treats it as one atomic box with an intrinsic size, which is what lets
    /// a tall figure sit inside running prose without special-casing the line model.
    pub objects: Vec<ObjectSpan>,
    /// What the reader can click: the ranges of [`Block::text`] that are a link or a
    /// citation, in text order and never overlapping.
    pub actions: Vec<Action>,
}

/// An inline non-text box.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ObjectSpan {
    pub range: std::ops::Range<usize>,
    pub kind: ObjectKind,
}

/// A range of [`Block::text`] the reader can act on, and what acting on it means.
///
/// Kept beside the text rather than folded into [`Span`]'s style flags because a
/// target is a string and not a bit: the style says this ink is a link, and only the
/// action says where it goes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Action {
    pub range: std::ops::Range<usize>,
    pub kind: ActionKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ActionKind {
    /// A link's destination, kept exactly as the author wrote it. Whether it is
    /// worth opening is the reader's judgement, not the parser's.
    Url(String),
    /// A citation of the footnote carrying this label. The note is already on the
    /// page, below, so a citation is jumped to rather than opened somewhere else.
    Cite(String),
}

/// A grid of cells with one header row and any number of body rows.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Table {
    /// Per-column alignment, in column order.
    pub aligns: Vec<Align>,
    pub head: Vec<Cell>,
    pub rows: Vec<Vec<Cell>>,
}

impl Table {
    pub fn columns(&self) -> usize {
        let mut n = self.head.len();
        for r in &self.rows {
            n = n.max(r.len());
        }
        n
    }
}

/// One table cell and the inline styles its text carries.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Cell {
    pub text: String,
    pub spans: Vec<Span>,
    /// What a click on this cell's text would do. A cell is laid out by the grid rather
    /// than as prose, so its targets are recorded here and not on the block.
    pub actions: Vec<Action>,
}

/// Where a cell's content sits within its column.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Align {
    #[default]
    Left,
    Center,
    Right,
}

/// What a placeholder stands for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ObjectKind {
    /// `src` is as written in the document; resolving it against the document's
    /// directory is the caller's job, since only the caller knows that directory.
    Image { src: String, alt: String },
    /// A formula in the source's own notation, with the delimiters removed.
    ///
    /// `display` is the `$$...$$` form: its own block, with its limits stacked and
    /// its fractions at full height. Inline `$...$` is compressed to fit a line of
    /// prose, which is a different layout rather than a different size.
    Math { source: String, display: bool },
}

impl Block {
    /// Headings and code are set ragged-right; justifying them is a defect.
    /// Headings and code are set ragged-right; a table is laid out per cell.
    pub fn ragged(&self) -> bool {
        matches!(
            self.kind,
            BlockKind::Heading(_) | BlockKind::Code | BlockKind::Rule | BlockKind::Table
        )
    }

    /// Spans covering the whole block, for callers that only need one style.
    pub fn whole_span(&self) -> Span {
        Span { range: 0..self.text.len(), style: InlineStyle::EMPTY }
    }
}

#[derive(Clone, Debug)]
pub struct Document {
    pub blocks: Vec<Block>,
    /// The footnotes, in the order their numbers ask to be read.
    ///
    /// Definitions are kept out of [`Document::blocks`] because a note's blocks
    /// belong under their number, not in the prose's reading order: a note with two
    /// paragraphs is two paragraphs *of the note*, and setting them inline would
    /// orphan them from the citation that explains why they are there.
    pub footnotes: Vec<Footnote>,
}

/// One footnote: its number, the label the author wrote, and its definition.
#[derive(Clone, Debug)]
pub struct Footnote {
    /// 1-based, assigned by first citation. Two citations of one label share it.
    pub number: usize,
    /// The author's own `[^label]`, kept so a reader can offer both orders.
    pub label: String,
    /// The definition's blocks, with their headings, lists and paragraphs intact.
    pub blocks: Vec<Block>,
}

impl Document {
    pub fn parse(source: &str) -> Document {
        let opts = Options::ENABLE_STRIKETHROUGH
            | Options::ENABLE_TASKLISTS
            | Options::ENABLE_TABLES
            | Options::ENABLE_MATH
            | Options::ENABLE_FOOTNOTES;
        let mut st = Builder::default();
        for ev in Parser::new_ext(source, opts) {
            st.event(ev);
        }
        st.finish()
    }
}

/// A definition being collected, before its number is known.
#[derive(Clone, Debug)]
struct Draft {
    label: String,
    /// Zero until the label is cited: an uncited definition is numbered last, in
    /// document order, rather than dropped.
    number: usize,
    blocks: Vec<Block>,
}

#[derive(Default)]
struct Builder {
    blocks: Vec<Block>,
    cur: Option<Block>,
    inline: InlineStyle,
    quote_depth: u8,
    /// One entry per open list, holding its info and the next item number.
    lists: Vec<(ListInfo, u64)>,
    /// How many `Tag::Item`s are open. Non-zero while a block is being read inside a
    /// list item, which is what makes it that item's continuation; see
    /// [`Block::item_depth`]. Nested items nest the count, so an inner list's blocks
    /// are still marked as belonging to a list.
    in_item: u8,
    /// Definitions by label, in the order they were defined.
    notes: Vec<Draft>,
    /// How many labels have been cited, which is the next number to hand out:
    /// numbers follow first-citation order rather than the order definitions happen
    /// to be parsed in, which pulldown reports last and in its own sequence.
    cited: usize,
    /// Index into `notes` while a definition's blocks are being read, which is where
    /// [`Builder::close`] sends them instead of into the body.
    note: Option<usize>,
    /// Accumulated while inside `Tag::Image`; its alt text is captured, not set.
    image: Option<(String, String)>,
    /// The destination of the `Tag::Link` being read, if one is open: every run of
    /// text pushed while it is set belongs to it.
    link: Option<String>,
    /// Accumulated while inside `Tag::Table`.
    table: Option<Table>,
    row: Vec<Cell>,
    cell: Option<Cell>,
    in_head: bool,
}

impl Builder {
    fn event(&mut self, ev: Event<'_>) {
        match ev {
            Event::Start(tag) => self.start(tag),
            Event::End(tag) => self.end(tag),
            Event::Text(t) => {
                if let Some((_, alt)) = self.image.as_mut() {
                    alt.push_str(&t);
                } else {
                    self.put(&t, self.inline);
                }
            }
            Event::Code(t) => {
                let mut st = self.inline;
                st.insert(InlineStyle::CODE);
                self.put(&t, st);
            }
            Event::SoftBreak => self.put(" ", InlineStyle::EMPTY),
            Event::HardBreak => self.put("\n", InlineStyle::EMPTY),
            Event::TaskListMarker(checked) => {
                if let Some(b) = self.cur.as_mut() {
                    b.task = Some(checked);
                }
            }
            // `<br>` is the one piece of inline HTML with typographic meaning, and
            // authors reach for it because a table cell or a list item has no other
            // way to break a line. Other markup is dropped: a reader that silently
            // swallows what it cannot render is worse than one that shows plain text,
            // but printing the tags would be a lie too.
            Event::InlineHtml(h) => {
                if is_line_break(&h) {
                    self.put("\n", InlineStyle::EMPTY);
                }
            }
            Event::Html(_) => {}
            Event::InlineMath(m) => {
                self.push_object(ObjectKind::Math { source: (*m).to_owned(), display: false })
            }
            Event::DisplayMath(m) => {
                // The parser reports a displayed formula between blocks, with no
                // paragraph around it. Closing first is what stops it swallowing the
                // block that follows: `open` does nothing while one is still current.
                self.close();
                self.push_object(ObjectKind::Math { source: (*m).to_owned(), display: true });
                self.close();
            }
            Event::Rule => {
                self.open(BlockKind::Rule);
                self.close();
            }
            Event::FootnoteReference(label) => {
                // The number's digits, and nothing else: no brackets and no space.
                // `SUPERSCRIPT` is what separates them from the word they belong to,
                // so the citation reads as a mark on the text rather than as text.
                let n = self.cite(label.as_ref());
                let style = self.inline | InlineStyle::SUPERSCRIPT;
                let cite = ActionKind::Cite(label.to_string());
                let digits = n.to_string();
                if let Some(c) = self.cell.as_mut() {
                    // A cell has no paragraph to append to -- the block being built is the
                    // table's, and text landing there is never drawn -- so the number goes
                    // into the cell, where it at least gets its column's width.
                    push_cell(c, &digits, style, Some(cite));
                } else {
                    let range = self.push(&digits, style);
                    if let Some(b) = self.cur.as_mut() {
                        push_action(&mut b.actions, range, cite);
                    }
                }
            }
        }
    }

    /// The number for `label`, handing out the next one on its first citation.
    ///
    /// Keyed by label rather than by position, because a document may cite `[^a]`
    /// before defining it, define the same label twice, or cite one label from two
    /// paragraphs -- and every one of those has to show the same digit.
    fn cite(&mut self, label: &str) -> usize {
        if let Some(d) = self.notes.iter_mut().find(|d| d.label == label) {
            if d.number == 0 {
                self.cited += 1;
                d.number = self.cited;
            }
            return d.number;
        }
        self.cited += 1;
        self.notes.push(Draft { label: label.to_string(), number: self.cited, blocks: Vec::new() });
        self.cited
    }

    fn start(&mut self, tag: Tag<'_>) {
        match tag {
            Tag::Paragraph => self.open(BlockKind::Paragraph),
            Tag::Heading { level, .. } => self.open(BlockKind::Heading(as_level(level))),
            Tag::BlockQuote(_) => self.quote_depth += 1,
            Tag::List(start) => {
                let ordered = start.is_some();
                self.lists.push((
                    ListInfo {
                        ordered,
                        depth: self.lists.len() as u8,
                        index: start,
                    },
                    start.unwrap_or(1),
                ));
            }
            Tag::Item => {
                // A nested tight list arrives while its parent item's block is
                // still open; close it first so the child does not merge into it.
                if self.cur.is_some() {
                    self.close();
                }
                let info = match self.lists.last_mut() {
                    Some((li, next)) => {
                        if li.ordered {
                            li.index = Some(*next);
                            *next += 1;
                        }
                        *li
                    }
                    None => ListInfo { ordered: false, depth: 0, index: None },
                };
                // Opened before the block: the item's own first block is inside the
                // item, and its continuation blocks are indented to match it.
                self.in_item += 1;
                self.open(BlockKind::Paragraph);
                if let Some(b) = self.cur.as_mut() {
                    b.list = Some(info);
                }
            }
            Tag::Table(aligns) => {
                self.open(BlockKind::Table);
                self.table = Some(Table {
                    aligns: aligns.iter().map(|a| match a {
                        pulldown_cmark::Alignment::Left => Align::Left,
                        pulldown_cmark::Alignment::Center => Align::Center,
                        pulldown_cmark::Alignment::Right => Align::Right,
                        pulldown_cmark::Alignment::None => Align::Left,
                    }).collect(),
                    ..Default::default()
                });
                self.row.clear();
                self.in_head = false;
            }
            Tag::TableHead => self.in_head = true,
            Tag::TableRow => self.row.clear(),
            Tag::TableCell => self.cell = Some(Cell::default()),
            Tag::CodeBlock(kind) => {
                self.open(if matches!(kind, CodeBlockKind::Fenced(_) | CodeBlockKind::Indented) {
                    BlockKind::Code
                } else {
                    BlockKind::Paragraph
                });
            }
            Tag::Emphasis => self.inline.insert(InlineStyle::EMPHASIS),
            Tag::Strong => self.inline.insert(InlineStyle::STRONG),
            Tag::Strikethrough => self.inline.insert(InlineStyle::STRIKETHROUGH),
            Tag::Link { dest_url, .. } => {
                self.inline.insert(InlineStyle::LINK);
                // The first destination wins: CommonMark cannot nest anchors, but a
                // malformed document can reach here with a link inside a link, and one
                // run of text can only be clicked into one place.
                if self.link.is_none() {
                    self.link = Some(dest_url.to_string());
                }
            }
            Tag::Image { dest_url, .. } => {
                self.image = Some((dest_url.to_string(), String::new()));
            }
            Tag::FootnoteDefinition(label) => {
                // Definitions are parsed after the whole body, so a cited label is
                // already here with its number; an uncited one joins now and is
                // numbered after every citation.
                let i = match self.notes.iter().position(|d| d.label == label.as_ref()) {
                    Some(i) => i,
                    None => {
                        self.notes.push(Draft {
                            label: label.to_string(),
                            number: 0,
                            blocks: Vec::new(),
                        });
                        self.notes.len() - 1
                    }
                };
                // One label defined twice appends to the same note: both texts are
                // the author's, and the citation cannot point at two numbers.
                // Closing first stops a block still open in the body being filed
                // under the definition when the definition ends.
                self.close();
                self.note = Some(i);
            }
            _ => {}
        }
    }

    fn end(&mut self, tag: TagEnd) {
        match tag {
            TagEnd::Paragraph | TagEnd::Heading(_) | TagEnd::CodeBlock => self.close(),
            TagEnd::TableCell => {
                if let Some(c) = self.cell.take() {
                    self.row.push(c);
                }
            }
            TagEnd::TableHead | TagEnd::TableRow => {
                let row = std::mem::take(&mut self.row);
                if let Some(t) = self.table.as_mut() {
                    if self.in_head {
                        t.head = row;
                    } else {
                        t.rows.push(row);
                    }
                }
                self.in_head = false;
            }
            TagEnd::Table => {
                if let Some(t) = self.table.take() {
                    if let Some(b) = self.cur.as_mut() {
                        b.table = Some(t);
                    }
                }
                self.close();
            }
            TagEnd::Item => {
                self.close();
                self.in_item = self.in_item.saturating_sub(1);
            }
            TagEnd::FootnoteDefinition => {
                self.close();
                self.note = None;
            }
            TagEnd::BlockQuote(_) => self.quote_depth = self.quote_depth.saturating_sub(1),
            TagEnd::List(_) => {
                self.lists.pop();
            }
            TagEnd::Emphasis => self.inline.remove(InlineStyle::EMPHASIS),
            TagEnd::Strong => self.inline.remove(InlineStyle::STRONG),
            TagEnd::Strikethrough => self.inline.remove(InlineStyle::STRIKETHROUGH),
            TagEnd::Link => {
                self.inline.remove(InlineStyle::LINK);
                self.link = None;
            }
            TagEnd::Image => {
                if let Some((src, alt)) = self.image.take() {
                    self.push_object(ObjectKind::Image { src, alt });
                }
            }
            _ => {}
        }
    }

    fn open(&mut self, kind: BlockKind) {
        if self.cur.is_some() {
            return;
        }
        self.cur = Some(Block {
            kind,
            text: String::new(),
            spans: Vec::new(),
            quote_depth: self.quote_depth,
            list: None,
            // The innermost open list, if an item of it is open: the level a block
            // joining now has to be indented to.
            item_depth: if self.in_item > 0 {
                self.lists.last().map(|(li, _)| li.depth)
            } else {
                None
            },
            task: None,
            objects: Vec::new(),
            actions: Vec::new(),
            table: None,
        });
    }

    /// Append a piece of inline text where it belongs: into the cell being read, or
    /// into the block. The two are not interchangeable -- a table block's own text is
    /// never drawn -- so every inline event comes through here rather than choosing a
    /// sink of its own. A `<br>` in a cell is the only way an author can break a line
    /// there, and it is a real break only if it reaches the cell.
    fn put(&mut self, s: &str, style: InlineStyle) {
        let link = self.link.clone().map(ActionKind::Url);
        match self.cell.as_mut() {
            Some(c) => push_cell(c, s, style, link),
            None => {
                self.push(s, style);
            }
        }
    }

    /// Append text to the current block and return the range of it that was written,
    /// which is what an action needs to point at.
    fn push(&mut self, s: &str, style: InlineStyle) -> std::ops::Range<usize> {
        if s.is_empty() {
            return 0..0;
        }
        if self.cur.is_none() {
            self.open(BlockKind::Paragraph);
        }
        // Taken before `cur` is borrowed: the link and the block are both fields of
        // the builder, and a run of text belongs to both.
        let linked = self.link.clone();
        let b = self.cur.as_mut().unwrap();
        let start = b.text.len();
        b.text.push_str(s);
        let end = b.text.len();
        match b.spans.last_mut() {
            // Merge adjacent runs of one style: pulldown splits on every escape and
            // entity, and a span per fragment would triple the node count.
            Some(prev) if prev.style == style && prev.range.end == start => prev.range.end = end,
            _ => b.spans.push(Span { range: start..end, style }),
        }
        if let Some(url) = linked {
            push_action(&mut b.actions, start..end, ActionKind::Url(url));
        }
        start..end
    }

    /// Append an object-replacement character and register what it stands for.
    fn push_object(&mut self, kind: ObjectKind) {
        if self.cur.is_none() {
            self.open(BlockKind::Paragraph);
        }
        let b = self.cur.as_mut().unwrap();
        let start = b.text.len();
        b.text.push('\u{FFFC}');
        let end = b.text.len();
        b.objects.push(ObjectSpan { range: start..end, kind });
        b.spans.push(Span { range: start..end, style: InlineStyle::OBJECT });
        // A figure inside a link is the link's whole text, so it is what gets clicked.
        if let Some(url) = self.link.clone() {
            push_action(&mut b.actions, start..end, ActionKind::Url(url));
        }
    }

    fn close(&mut self) {
        if let Some(mut b) = self.cur.take() {
            if b.kind == BlockKind::Code {
                // Fenced code always ends with a newline the engine would read as a
                // hard break, i.e. a blank line at the bottom of every block.
                while b.text.ends_with('\n') {
                    b.text.pop();
                }
                b.spans.retain(|s| s.range.start < b.text.len());
                for s in b.spans.iter_mut() {
                    s.range.end = s.range.end.min(b.text.len());
                }
            }
            if worth_setting(&b) {
                // A definition's blocks belong to the definition, not to the page's
                // reading order, so they leave `blocks` here rather than being
                // filtered out of it later.
                match self.note {
                    Some(i) => self.notes[i].blocks.push(b),
                    None => self.blocks.push(b),
                }
            }
        }
    }

    fn finish(mut self) -> Document {
        self.close();
        self.blocks.retain(worth_setting);
        // An uncited definition is numbered last, in document order: losing an
        // author's text is worse than a number no citation points at.
        for d in self.notes.iter_mut() {
            if d.number == 0 {
                self.cited += 1;
                d.number = self.cited;
            }
        }
        let mut footnotes: Vec<Footnote> = self
            .notes
            .into_iter()
            // A label cited but never defined has no blocks to show, and a bare
            // number in the list would claim a note that does not exist.
            .filter(|d| !d.blocks.is_empty())
            .map(|d| Footnote { number: d.number, label: d.label, blocks: d.blocks })
            .collect();
        for f in footnotes.iter_mut() {
            f.blocks.retain(worth_setting);
        }
        footnotes.sort_by_key(|f| f.number);
        Document { blocks: self.blocks, footnotes }
    }
}

/// Whether a block is worth setting. A table and a rule carry no text of their own,
/// so a test on `text` alone would drop them.
fn worth_setting(b: &Block) -> bool {
    !b.text.trim().is_empty()
        || b.kind == BlockKind::Rule
        || b.table.as_ref().is_some_and(|t| !t.head.is_empty() || !t.rows.is_empty())
}

/// Append an actionable range, merging with the previous one when it is the same
/// target and abuts it. A link whose text carries emphasis is pushed in pieces, and
/// one link is one thing to click.
fn push_action(actions: &mut Vec<Action>, range: std::ops::Range<usize>, kind: ActionKind) {
    if range.is_empty() {
        return;
    }
    match actions.last_mut() {
        Some(prev) if prev.kind == kind && prev.range.end == range.start => prev.range.end = range.end,
        _ => actions.push(Action { range, kind }),
    }
}

/// Append text to a cell, merging with the previous span when the style matches.
fn push_cell(c: &mut Cell, t: &str, style: InlineStyle, action: Option<ActionKind>) {
    if t.is_empty() {
        return;
    }
    let start = c.text.len();
    c.text.push_str(t);
    let end = c.text.len();
    match c.spans.last_mut() {
        Some(prev) if prev.style == style && prev.range.end == start => prev.range.end = end,
        _ => c.spans.push(Span { range: start..end, style }),
    }
    if let Some(kind) = action {
        push_action(&mut c.actions, start..end, kind);
    }
}

fn as_level(l: HeadingLevel) -> u8 {
    match l {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

/// `<br>`, `<br/>`, `<br />` and their upper-case spellings: the only tag inline
/// HTML is allowed to change the page with.
fn is_line_break(html: &str) -> bool {
    let tag = html.trim().trim_start_matches('<').trim_end_matches('>');
    tag.trim_end_matches('/').trim_end().eq_ignore_ascii_case("br")
}
