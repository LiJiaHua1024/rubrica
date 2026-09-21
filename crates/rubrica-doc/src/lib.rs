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
}

impl Block {
    /// Headings and code are set ragged-right; justifying them is a defect.
    pub fn ragged(&self) -> bool {
        matches!(self.kind, BlockKind::Heading(_) | BlockKind::Code | BlockKind::Rule)
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
        let mut opts = Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS;
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
}

impl Builder {
    fn event(&mut self, ev: Event<'_>) {
        match ev {
            Event::Start(tag) => self.start(tag),
            Event::End(tag) => self.end(tag),
            Event::Text(t) => self.push(&t, self.inline),
            Event::Code(t) => self.push(&t, {
                let mut s = self.inline;
                s.insert(InlineStyle::CODE);
                s
            }),
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
            Event::InlineMath(m) => self.push(&m, InlineStyle::EMPHASIS),
            Event::DisplayMath(m) => self.push(&m, InlineStyle::EMPTY),
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
            Tag::Image { .. } => {}
            _ => {}
        }
    }

    fn end(&mut self, tag: TagEnd) {
        match tag {
            TagEnd::Paragraph | TagEnd::Heading(_) | TagEnd::CodeBlock => self.close(),
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
            TagEnd::Image => {}
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
            if !empty || b.kind == BlockKind::Rule {
                self.blocks.push(b);
            }
        }
    }

    fn finish(mut self) -> Document {
        self.close();
        self.blocks.retain(|b| !b.text.trim().is_empty() || b.kind == BlockKind::Rule);
        Document { blocks: self.blocks }
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
