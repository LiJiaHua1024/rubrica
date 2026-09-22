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

    #[inline]
    pub const fn bits(self) -> u8 {
        self.0
    }
    #[inline]
    pub const fn contains(self, other: InlineStyle) -> bool {
        self.0 & other.0 == other.0
    }
    #[inline]
    fn insert(&mut self, other: InlineStyle) {
        self.0 |= other.0;
    }
    #[inline]
    fn remove(&mut self, other: InlineStyle) {
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
}

/// An inline non-text box.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ObjectSpan {
    pub range: std::ops::Range<usize>,
    pub kind: ObjectKind,
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
}

impl Document {
    pub fn parse(source: &str) -> Document {
        let mut opts = Options::ENABLE_STRIKETHROUGH
            | Options::ENABLE_TASKLISTS
            | Options::ENABLE_TABLES
            | Options::ENABLE_MATH;
        // A footnote definition would become a block orphaned from the paragraph
        // that cites it, and citations are invisible without a reference UI. Keep
        // the syntax literal until it has one.
        opts.remove(Options::ENABLE_FOOTNOTES);
        let mut st = Builder::default();
        for ev in Parser::new_ext(source, opts) {
            st.event(ev);
        }
        st.finish()
    }
}

#[derive(Default)]
struct Builder {
    blocks: Vec<Block>,
    cur: Option<Block>,
    inline: InlineStyle,
    quote_depth: u8,
    /// One entry per open list, holding its info and the next item number.
    lists: Vec<(ListInfo, u64)>,
    /// Accumulated while inside `Tag::Image`; its alt text is captured, not set.
    image: Option<(String, String)>,
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
                } else if let Some(c) = self.cell.as_mut() {
                    push_cell(c, &t, self.inline);
                } else {
                    self.push(&t, self.inline);
                }
            }
            Event::Code(t) => {
                let mut st = self.inline;
                st.insert(InlineStyle::CODE);
                match self.cell.as_mut() {
                    Some(c) => push_cell(c, &t, st),
                    None => self.push(&t, st),
                }
            }
            Event::SoftBreak => self.push(" ", InlineStyle::EMPTY),
            Event::HardBreak => self.push("\n", InlineStyle::EMPTY),
            Event::TaskListMarker(checked) => {
                if let Some(b) = self.cur.as_mut() {
                    b.task = Some(checked);
                }
            }
            // Inline HTML and entities are deliberately dropped: a reader that
            // silently swallows markup it cannot render is worse than one that
            // shows plain text, but showing raw HTML would be a lie too.
            Event::Html(_) | Event::InlineHtml(_) => {}
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
            Event::FootnoteReference(_) => {}
        }
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
            Tag::Link { .. } => self.inline.insert(InlineStyle::LINK),
            Tag::Image { dest_url, .. } => {
                self.image = Some((dest_url.to_string(), String::new()));
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
            }
            TagEnd::BlockQuote(_) => self.quote_depth = self.quote_depth.saturating_sub(1),
            TagEnd::List(_) => {
                self.lists.pop();
            }
            TagEnd::Emphasis => self.inline.remove(InlineStyle::EMPHASIS),
            TagEnd::Strong => self.inline.remove(InlineStyle::STRONG),
            TagEnd::Strikethrough => self.inline.remove(InlineStyle::STRIKETHROUGH),
            TagEnd::Link => self.inline.remove(InlineStyle::LINK),
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
            task: None,
            objects: Vec::new(),
            table: None,
        });
    }

    fn push(&mut self, s: &str, style: InlineStyle) {
        if s.is_empty() {
            return;
        }
        if self.cur.is_none() {
            self.open(BlockKind::Paragraph);
        }
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
            let empty = b.text.trim().is_empty();
            let has_table = b.table.as_ref().is_some_and(|t| !t.head.is_empty() || !t.rows.is_empty());
            if !empty || b.kind == BlockKind::Rule || has_table {
                self.blocks.push(b);
            }
        }
    }

    fn finish(mut self) -> Document {
        self.close();
        // A table and a rule carry no block text of their own, so a test on
        // `text` alone would drop them; this must agree with `close`.
        self.blocks.retain(|b| {
            !b.text.trim().is_empty()
                || b.kind == BlockKind::Rule
                || b.table.as_ref().is_some_and(|t| !t.head.is_empty() || !t.rows.is_empty())
        });
        Document { blocks: self.blocks }
    }
}

/// Append text to a cell, merging with the previous span when the style matches.
fn push_cell(c: &mut Cell, t: &str, style: InlineStyle) {
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
