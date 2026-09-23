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
    /// One thing a definition list names, drawn out at the margin with its
    /// definitions under it.
    ///
    /// Markdown has no syntax for this, and pulldown has no extension for it: a term
    /// and the `: ` line beneath it reach us as one paragraph whose lines were joined
    /// at a soft break. [`Builder`] marks where those joins fell and `close` cuts the
    /// paragraph back along them.
    Term,
    /// One `: ` line under a [`BlockKind::Term`], drawn indented. The colon is syntax,
    /// so it is gone from [`Block::text`] rather than hidden by the renderer.
    Definition,
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
    /// The language a fence names after its opening backticks, lower-cased and with any
    /// attributes after it dropped: `rust,ignore` says `rust`, and ```java title="x"```
    /// says `java`.
    ///
    /// Set only for [`BlockKind::Code`], and only when the author said -- an indented
    /// block has no information to carry, and guessing a language from what the code
    /// looks like colours a wrong guess as confidently as a right one.
    pub lang: Option<String>,
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
    /// The images and formulas this cell holds, ranged against [`Cell::text`] the same
    /// way [`Block::objects`] is ranged against a block's text.
    ///
    /// They have to travel with the cell: a grid lays its cells out on their own, so an
    /// object registered on the block is a placeholder in one text and an ink box in
    /// another, and the cell that owns it paints nothing where its width was measured.
    pub objects: Vec<ObjectSpan>,
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
        // Front matter is the author's metadata for the tool that wrote the file, and
        // nothing of it is prose: shown as itself it arrives as a rule, a paragraph of
        // `key: value` lines, and another rule, which is the top of every document that
        // comes out of a blog or a vault.
        let source = split_front_matter(source).map_or(source, |(_, body)| body);
        // Pandoc's TeX delimiters, read out of the source before anything else touches
        // it: `\(` is a CommonMark escape for a bracket, so the backslash is gone by
        // the time an event carries the text and there is nothing left to recognise.
        let rewritten = tex_delimiters(source);
        let source = rewritten.as_deref().unwrap_or(source);
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

/// Rewrite Pandoc's TeX math delimiters into the `$` form the parser already knows.
///
/// A LaTeX author writes `\(x^2\)` and puts a displayed equation between `\[` and `\]`
/// on lines of their own. CommonMark reads the backslash as an escape for the bracket,
/// so by the time any handler sees the text it is `(x^2)` with nothing left to
/// recognise -- the rewrite has to happen on the source. Which is also why it has to
/// keep its hands off code: a fenced block full of LaTeX is exactly the file that would
/// otherwise be turned into nonsense.
///
/// `\(` and `\)` are read anywhere outside a code span, because nobody writes `\(` to
/// mean a bracket -- `(` needs no escaping in prose. `\[` and `\]` are read only when
/// they stand on a line of their own, because `\[1\]` really is how an escaped bracket
/// looks mid-sentence, and a display delimiter is never written any other way.
fn tex_delimiters(src: &str) -> Option<String> {
    if !(src.contains("\\(") || src.contains("\\)") || src.contains("\\[") || src.contains("\\]")) {
        return None;
    }
    let mut out = String::with_capacity(src.len());
    // The character and run length of a code fence that is still open.
    let mut fence: Option<(char, usize)> = None;
    for line in src.split_inclusive('\n') {
        let body = line.trim_end_matches(['\n', '\r']);
        let text = body.trim_start();
        let indent = body.len() - text.len();
        let first = text.chars().next();
        let run = text.chars().take_while(|c| Some(*c) == first).count();
        let marker = indent < 4 && matches!(first, Some('`') | Some('~')) && run >= 3;
        let ends_fence = fence.is_some_and(|(c, n)| {
            marker && first == Some(c) && run >= n && text.chars().all(|x| x == c)
        });
        if ends_fence || marker && fence.is_none() {
            // The fence's own lines are copied as written: opening or closing it.
            fence = if ends_fence { None } else { Some((first.unwrap(), run)) };
            out.push_str(body);
            out.push_str(&line[body.len()..]);
            continue;
        }
        let plain = fence.is_some() || indent >= 4;
        let display = !plain && (text == "\\[" || text == "\\]");
        if display {
            out.push_str("$$");
        } else if plain {
            out.push_str(body);
        } else {
            out.push_str(&tex_inline(body));
        }
        out.push_str(&line[body.len()..]);
    }
    Some(out)
}

/// `\(x\)` to `$x$` across one line of prose, leaving anything inside a code span as
/// its author typed it.
fn tex_inline(line: &str) -> String {
    let b = line.as_bytes();
    let mut out = String::with_capacity(line.len());
    let mut i = 0;
    // Length of the backtick run a code span opened with, which is the only run long
    // enough to close it.
    let mut span = 0usize;
    while i < b.len() {
        match b[i] {
            b'`' => {
                let mut n = 0;
                while i + n < b.len() && b[i + n] == b'`' {
                    n += 1;
                }
                if span == 0 {
                    span = n;
                } else if span == n {
                    span = 0;
                }
                out.push_str(&line[i..i + n]);
                i += n;
            }
            // Two backslashes are one literal one, and it does not escape what comes
            // after: `\\(` is a backslash and a bracket, not a delimiter.
            b'\\' if b.get(i + 1) == Some(&b'\\') => {
                out.push_str("\\\\");
                i += 2;
            }
            b'\\' if span == 0 && matches!(b.get(i + 1), Some(b'(' | b')')) => {
                out.push('$');
                i += 2;
            }
            _ => {
                let c = line[i..].chars().next().unwrap();
                out.push(c);
                i += c.len_utf8();
            }
        }
    }
    out
}

/// The source of a formula, with the line breaks it was wrapped on turned into the
/// spaces they were written for.
///
/// A displayed equation over three source lines is one equation, and LaTeX reads the
/// breaks as spaces. Passing them on reached the shaper as characters, which is two
/// wrapped lines and two empty boxes in the middle of the page -- visible in the report
/// as `notdef` and invisible in the source, which is a well-formed equation.
fn math_source(m: &str) -> String {
    m.chars().map(|c| if matches!(c, '\n' | '\r' | '\t') { ' ' } else { c }).collect()
}

/// The line `s` starts on, with its terminator, and what follows it. A final line with
/// no terminator is still a line; an empty `s` has no line at all.
fn line_of(s: &str) -> Option<(&str, &str)> {
    let end = s.find('\n').map_or(s.len(), |i| i + 1);
    (!s.is_empty()).then(|| s.split_at(end))
}

/// The document's front matter and the body to read after it, or `None` when there is
/// no front matter to take.
///
/// The one spelling every authoring tool uses: a line that is `---` first, closed by a
/// later line that is `---` or `...`. A document that opens the fence and never closes
/// it is left exactly as written, because the author was sure about the rule and not
/// about the metadata -- and a reader that swallowed the rest of the file looking for a
/// closer would be a reader with a blank page in it.
pub fn split_front_matter(source: &str) -> Option<(&str, &str)> {
    let body = source.strip_prefix('\u{feff}').unwrap_or(source);
    let (open, rest) = line_of(body)?;
    if !is_marker(open, "---") {
        return None;
    }
    let mut scanned = rest;
    let after_close = loop {
        let (line, after) = line_of(scanned)?;
        if is_marker(line, "---") || is_marker(line, "...") {
            break after;
        }
        scanned = after;
    };
    Some((&body[..body.len() - after_close.len()], after_close))
}

/// Whether `line` is one of the two markers on a line of its own: written with no more
/// leading blanks than CommonMark allows a block marker to have, and with any trailing
/// ones. `---   ` is the same fence as `---` to every tool that reads one.
fn is_marker(line: &str, marker: &str) -> bool {
    let body = line.trim_start_matches(' ');
    line.len() - body.len() <= 3 && bare_line(body) == marker
}

/// A line without its terminator or its trailing blanks.
fn bare_line(line: &str) -> &str {
    line.trim_end_matches(['\r', '\n', ' ', '\t'])
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
    /// Where the current block's paragraph had its lines joined by a soft break.
    /// A definition list reaches us as one wrapped paragraph, and these offsets are
    /// the only trace of the line each `: ` actually began on.
    breaks: Vec<usize>,
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
    /// The source of the raw HTML block being read, gathered whole: see
    /// [`Builder::set_html`].
    html: Option<String>,
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
            Event::SoftBreak => {
                // Mark the join before making it: the space below is what the line
                // break looked like once CommonMark had folded it, and a definition
                // list is only recoverable if where the fold happened survived.
                if let Some(b) = self.cur.as_mut() {
                    if self.cell.is_none() {
                        self.breaks.push(b.text.len());
                    }
                }
                self.put(" ", InlineStyle::EMPTY)
            }
            Event::HardBreak => self.put("\n", InlineStyle::EMPTY),
            Event::TaskListMarker(checked) => {
                if let Some(b) = self.cur.as_mut() {
                    b.task = Some(checked);
                }
            }
            // `<br>` is the one piece of inline HTML with typographic meaning, and
            // authors reach for it because a table cell or a list item has no other
            // way to break a line. An inline tag carries a style the reader's page has
            // no answer for, so it is dropped with its name: the words between it and
            // its partner are already text, and printing the markup among them would be
            // a lie about what the author meant.
            Event::InlineHtml(h) => {
                if is_line_break(&h) {
                    self.put("\n", InlineStyle::EMPTY);
                }
            }
            // A block of raw HTML is collected rather than answered chunk by chunk,
            // because the parser cuts it at line ends and a sentence does not stop
            // there; see [`html_pieces`].
            Event::Html(h) => match self.html.as_mut() {
                Some(buf) => buf.push_str(&h),
                None => self.set_html(&h),
            },
            Event::InlineMath(m) => self.push_object(ObjectKind::Math {
                source: math_source(&m),
                display: false,
            }),
            Event::DisplayMath(m) => {
                // The parser reports a displayed formula between blocks, with no
                // paragraph around it. Closing first is what stops it swallowing the
                // block that follows: `open` does nothing while one is still current.
                self.close();
                self.push_object(ObjectKind::Math { source: math_source(&m), display: true });
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
            // Its own block, begun and ended by the parser rather than by any tag in the
            // text: what the block holds is decided once its whole source is in hand.
            Tag::HtmlBlock => {
                self.close();
                self.html = Some(String::new());
            }
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
                let code = matches!(kind, CodeBlockKind::Fenced(_) | CodeBlockKind::Indented);
                self.open(if code { BlockKind::Code } else { BlockKind::Paragraph });
                // Carried to the reader rather than matched against a list of languages
                // here: what `kotlin` means is the page's business, and a document crate
                // that knows which spellings it recognises would have to be updated to
                // let a reader read a language that has since been born.
                if let CodeBlockKind::Fenced(info) = kind {
                    if let Some(b) = self.cur.as_mut() {
                        b.lang = fence_language(&info);
                    }
                }
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
            TagEnd::HtmlBlock => {
                if let Some(raw) = self.html.take() {
                    self.set_html(&raw);
                }
                self.close();
            }
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

    /// Set what a raw HTML block says, with its markup taken out of it.
    ///
    /// None of the block's text was ever parsed as Markdown -- an HTML block is read as
    /// source -- so what arrives here is what the author typed between the tags, which is
    /// the words of a paragraph and sometimes a `<b>` nobody can see.
    fn set_html(&mut self, raw: &str) {
        for piece in html_pieces(raw) {
            match piece {
                Piece::Text(t) => self.put(&t, self.inline),
                Piece::Image { src, alt } => {
                    self.push_object(ObjectKind::Image { src, alt });
                }
                Piece::Paragraph => self.close(),
                Piece::Rule => {
                    self.close();
                    self.open(BlockKind::Rule);
                    self.close();
                }
                Piece::Heading(level) => {
                    self.close();
                    self.open(BlockKind::Heading(level));
                }
            }
        }
    }

    fn open(&mut self, kind: BlockKind) {
        if self.cur.is_some() {
            return;
        }
        self.breaks.clear();
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
            lang: None,
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
        if let Some(c) = self.cell.as_mut() {
            // In a grid the cell is the text this object will be laid out in, so the
            // placeholder and its record both belong there and not on the block: a
            // range into the block's text means nothing to a cell that has its own.
            let start = c.text.len();
            c.text.push('\u{FFFC}');
            let end = c.text.len();
            c.objects.push(ObjectSpan { range: start..end, kind });
            c.spans.push(Span { range: start..end, style: InlineStyle::OBJECT });
            if let Some(url) = self.link.clone() {
                push_action(&mut c.actions, start..end, ActionKind::Url(url));
            }
            return;
        }
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
                let breaks = std::mem::take(&mut self.breaks);
                // A definition's blocks belong to the definition, not to the page's
                // reading order, so they leave `blocks` here rather than being
                // filtered out of it later.
                for b in split_definitions(b, breaks) {
                    if !worth_setting(&b) {
                        continue;
                    }
                    match self.note {
                        Some(i) => self.notes[i].blocks.push(b),
                        None => self.blocks.push(b),
                    }
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

/// What opens a definition line: the colon and the space the author owed it.
const DEFINITION_OPEN: &str = ": ";

/// Cut a paragraph along the lines its author wrote, wherever one of them opens a
/// definition.
///
/// A definition list is not Markdown syntax, so pulldown hands the whole thing over as
/// one paragraph with its line breaks folded into spaces. `breaks` says where each fold
/// sat, which is the only way back to the structure -- and the fold has to be undone
/// here rather than by the reader, because the `: ` that marks a definition is syntax,
/// and syntax a document model keeps is syntax the page then has to draw.
fn split_definitions(b: Block, breaks: Vec<usize>) -> Vec<Block> {
    if b.kind != BlockKind::Paragraph || b.list.is_some() || b.table.is_some() {
        return vec![b];
    }
    let mut lines = Vec::with_capacity(breaks.len() + 1);
    let mut start = 0usize;
    for at in breaks {
        lines.push(start..at);
        start = at + 1; // the space the fold left behind
    }
    lines.push(start..b.text.len());
    if !lines.iter().any(|r| opens_a_definition(&b.text, r)) {
        return vec![b];
    }
    lines
        .into_iter()
        .filter_map(|r| {
            let def = opens_a_definition(&b.text, &r);
            let head = r.start + if def { leading_definition(&b.text, &r) } else { 0 };
            piece(&b, head..r.end, if def { BlockKind::Definition } else { BlockKind::Term })
        })
        .collect()
}

/// True for a line that opens with a colon and a space, which is the whole syntax.
fn opens_a_definition(text: &str, range: &std::ops::Range<usize>) -> bool {
    let body = &text[range.clone()];
    let t = body.trim_start_matches(' ');
    t.starts_with(DEFINITION_OPEN) || t == ":"
}

/// How far into the line the definition's own text begins, past whatever indent the
/// author left and past the colon.
fn leading_definition(text: &str, range: &std::ops::Range<usize>) -> usize {
    let body = &text[range.clone()];
    let indent = body.len() - body.trim_start_matches(' ').len();
    indent + DEFINITION_OPEN.len()
}

/// One line of a folded paragraph, standing on its own: its slice of the text, and
/// every span, target and object that fell inside it re-based onto that slice.
fn piece(base: &Block, range: std::ops::Range<usize>, kind: BlockKind) -> Option<Block> {
    let end = base.text[..range.end].trim_end().len().max(range.start);
    let range = range.start..end;
    if range.start >= range.end {
        return None;
    }
    let cut = |r: std::ops::Range<usize>| {
        let a = r.start.max(range.start);
        let b = r.end.min(range.end);
        (a < b).then(|| (a - range.start)..(b - range.start))
    };
    Some(Block {
        kind,
        text: base.text[range.clone()].to_string(),
        spans: base
            .spans
            .iter()
            .filter_map(|s| cut(s.range.clone()).map(|range| Span { range, style: s.style }))
            .collect(),
        quote_depth: base.quote_depth,
        list: None,
        item_depth: base.item_depth,
        task: None,
        table: None,
        objects: base
            .objects
            .iter()
            .filter_map(|o| {
                cut(o.range.clone()).map(|range| ObjectSpan { range, kind: o.kind.clone() })
            })
            .collect(),
        actions: base
            .actions
            .iter()
            .filter_map(|a| cut(a.range.clone()).map(|range| Action { range, kind: a.kind.clone() }))
            .collect(),
        lang: None,
    })
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

/// The language a fence's info string names, which is its first word.
///
/// The comma and the brace are what the rest of the ecosystem hangs on the name:
/// `rust,ignore` is the mdBook spelling of "this is not a test", and `{#id .class}`
/// is what Python-Markdown's attribute extension appends. Both leave the language
/// where they found it, so only the word before them is taken. An empty info string
/// is `None` rather than `Some("")` -- the author said nothing, and a reader whose
/// code is not a reader's language should be shown the plain page rather than a
/// partial guess at one.
fn fence_language(info: &str) -> Option<String> {
    let word = info.trim().split([',', ' ', '\t', '\n', '{']).next()?;
    (!word.is_empty()).then(|| word.to_lowercase())
}

/// One piece of a raw HTML block, its markup already turned into what it means on a page.
enum Piece {
    /// Prose, its whitespace collapsed to what a browser makes of it.
    Text(String),
    /// `<img>`: a figure, named as the document names it.
    Image { src: String, alt: String },
    /// `<h1>` to `<h6>`.
    Heading(u8),
    /// The end of a block-level element, so that what follows is another paragraph.
    Paragraph,
    /// `<hr>`, which is `---` under another name.
    Rule,
}

/// The elements whose content is a program's source rather than prose. Not one of their
/// words is text the reader is owed, and a page's script set in the reader's own face is
/// not a document.
const NOT_PROSE: [&str; 6] = ["script", "style", "head", "title", "textarea", "template"];

/// The elements that start on a line of their own, and so end the paragraph they arrive in
/// the middle of. `<li>` is one of them because two of them are two lines rather than one
/// sentence with a gap in it.
const BLOCK_LEVEL: [&str; 27] = [
    "address", "article", "aside", "blockquote", "dd", "div", "dl", "dt", "fieldset",
    "figcaption", "figure", "footer", "form", "header", "hgroup", "li", "main", "nav", "ol",
    "p", "pre", "section", "table", "td", "th", "tr", "ul",
];

/// The prose and the figures a raw HTML block carries, taken out of its markup.
///
/// The whole block is scanned at once rather than line by line as the parser hands it over,
/// because HTML's whitespace has no respect for a line ending: the newline between two
/// lines is one space, exactly as the eight after a tag are.
///
/// What an element *means* is kept where the page has a typographic answer for it -- a
/// break, a figure, a heading, the end of a paragraph -- and the rest of the markup is
/// dropped along with the shape it gave, which is the only reading of it that does not
/// either swallow the author's words or print their tools among them.
fn html_pieces(raw: &str) -> Vec<Piece> {
    let mut f = Fragments::default();
    let mut rest = raw;
    while let Some(at) = rest.find('<') {
        f.words(&rest[..at]);
        let after = &rest[at + 1..];
        // `<!-- ... -->` and `<!DOCTYPE ...>`: markup with no words in it at all. A comment
        // runs to its own terminator rather than to the first `>`, because an author writes
        // `a > b` inside one.
        if let Some(tail) = after.strip_prefix("--") {
            rest = match tail.find("-->") {
                Some(i) => &tail[i + 3..],
                None => "",
            };
            continue;
        }
        if after.starts_with('!') {
            rest = match after.find('>') {
                Some(i) => &after[i + 1..],
                None => "",
            };
            continue;
        }
        // A `<` that is not followed by a name or a slash is the character rather than the
        // start of a tag -- prose says `x < y` -- and only a tag may swallow up to its `>`.
        // The whole run to the next tag goes in together, because the space the author put
        // after the `<` is the one thing a browser would never have dropped.
        let starts_a_tag =
            matches!(after.as_bytes().first(), Some(c) if c.is_ascii_alphabetic() || *c == b'/');
        let close = if starts_a_tag { after.find('>') } else { None };
        let Some(close) = close else {
            // No name after the `<`, or no `>` to close it: text to the next tag, or to
            // the end of the block.
            let end = after.find('<').map_or(rest.len(), |i| at + 1 + i);
            f.words(&rest[at..end]);
            rest = &rest[end..];
            continue;
        };
        f.tag(&after[..close]);
        rest = &after[close + 1..];
    }
    f.words(rest);
    f.finish()
}

/// The block being gathered, one tag at a time.
#[derive(Default)]
struct Fragments {
    out: Vec<Piece>,
    /// The prose gathered since the last piece, its whitespace already collapsed.
    text: String,
    /// A space the text ended with, still owed to the word after it. A tag is not a break in
    /// a sentence: `<b>word</b> next` is a line with a space in it, and `<b>word</b>next` is
    /// one word -- which is what the author between the tags wrote.
    owed: bool,
    /// Whether the element being read holds a program's source rather than prose, which one
    /// tag decides and a later one undoes.
    hidden: bool,
}

impl Fragments {
    /// Take in a run of text that stood between two tags: keep its words, and all of its
    /// whitespace that stands for a space.
    fn words(&mut self, run: &str) {
        if self.hidden || run.is_empty() {
            return;
        }
        for word in run.split_whitespace() {
            if self.owed && !self.text.is_empty() {
                self.text.push(' ');
            }
            self.text.push_str(&resolve_entities(word));
            self.owed = true;
        }
        self.owed = run.ends_with(char::is_whitespace);
    }

    /// The tag between two runs of text.
    fn tag(&mut self, tag: &str) {
        let (name, closing) = tag_name(tag);
        if name.is_empty() {
            return;
        }
        if NOT_PROSE.contains(&name.as_str()) {
            // One of their end tags is what lets the prose back in, and the text gathered
            // before the element stays gathered: a script is invisible, not a full stop.
            self.hidden = !closing;
            return;
        }
        if self.hidden {
            return;
        }
        match name.as_str() {
            "br" => {
                self.text.push('\n');
                self.owed = false;
            }
            "img" => {
                let src = attr(tag, "src").unwrap_or_default();
                let alt = attr(tag, "alt").unwrap_or_default();
                self.flush();
                // A figure with no file named is a hole in the page, which is worse than
                // the empty alt text that was meant to fill it.
                if !src.is_empty() {
                    self.out.push(Piece::Image { src, alt });
                }
            }
            "hr" => {
                self.flush();
                self.out.push(Piece::Rule);
                self.paragraph_end();
            }
            _ if let Some(level) = heading(&name) => {
                self.flush();
                if closing {
                    self.paragraph_end();
                } else {
                    self.out.push(Piece::Heading(level));
                }
            }
            _ if BLOCK_LEVEL.contains(&name.as_str()) => self.paragraph_end(),
            _ => {}
        }
    }

    /// End the paragraph being gathered, if there is one to end: two block tags together
    /// are not a paragraph the reader is owed, and neither is one that opens the block.
    fn paragraph_end(&mut self) {
        self.flush();
        if !self.out.is_empty() && !matches!(self.out.last(), Some(Piece::Paragraph)) {
            self.out.push(Piece::Paragraph);
        }
    }

    /// Hand the prose gathered so far over as a piece of its own.
    fn flush(&mut self) {
        if !self.text.is_empty() {
            self.out.push(Piece::Text(std::mem::take(&mut self.text)));
        }
        self.owed = false;
    }

    fn finish(mut self) -> Vec<Piece> {
        self.flush();
        self.out
    }
}

/// The element a tag names, lower-cased for comparison, and whether it closes the element
/// rather than opening it: `<div>`, `</div>`, `<BR />`, `<img src="a.png">`.
///
/// Empty for anything that is not a name at all, which is how a stray character inside the
/// markup is dropped rather than read as an element nobody heard of.
fn tag_name(tag: &str) -> (String, bool) {
    let closing = tag.starts_with('/');
    let mut name = String::new();
    for c in tag[closing as usize..].chars() {
        if name.is_empty() {
            // A tag's first character is a letter, whatever follows its name: `<p>` and
            // `</p>`, but not `<!--` and not `<3`.
            if !c.is_ascii_alphabetic() {
                return (String::new(), closing);
            }
        } else if !c.is_ascii_alphanumeric() {
            // `h2` is an element's whole name, and the space after it begins the
            // attributes; `<hr width=1>` has no more name in it than that.
            break;
        }
        name.extend(c.to_lowercase());
    }
    (name, closing)
}

/// `h1` to `h6`, as their level. The one element here with a size of its own to carry over,
/// and worth carrying because an author who reaches for it means a heading rather than a
/// box: `header`, `hgroup` and `html` are none of them a level.
fn heading(name: &str) -> Option<u8> {
    let level: u8 = name.strip_prefix('h')?.parse().ok()?;
    (1..=6).contains(&level).then_some(level)
}

/// An attribute's value out of a tag. `src="a.png"`, `src='a.png'` and `src=a.png` are the
/// three spellings an author reaches for, and the last of them ends at a space.
fn attr(tag: &str, key: &str) -> Option<String> {
    let mut rest = tag;
    while let Some(at) = rest.find(key) {
        let after = &rest[at + key.len()..];
        // The name has to begin where an attribute does, or `srcset` answers for `src`.
        let boundary =
            at == 0 || matches!(rest.as_bytes()[at - 1], b' ' | b'\t' | b'\n' | b'"' | b'\'');
        rest = after;
        if !boundary {
            continue;
        }
        let gaps = [' ', '\t', '\n'];
        let Some(value) = after.trim_start_matches(gaps).strip_prefix('=') else {
            continue;
        };
        let value = value.trim_start_matches(gaps);
        if let Some(quote) = value.chars().next().filter(|c| matches!(c, '"' | '\'')) {
            let value = &value[quote.len_utf8()..];
            return Some(resolve_entities(value.split(quote).next().unwrap_or(value)));
        }
        let value = value.split([' ', '\t', '\n', '/']).next().unwrap_or_default();
        return Some(resolve_entities(value));
    }
    None
}

/// The character references worth resolving, besides the numeric ones. An HTML block is
/// source rather than parsed text, so its `&amp;` was never the parser's to decode -- and a
/// reader shown `AT&amp;T` is being read the markup, not the words. `&nbsp;` becomes the
/// ordinary space, since a page set from scratch has nothing to say about which of its
/// spaces a line may not end on.
const ENTITIES: [(&str, char); 20] = [
    ("amp", '&'),
    ("lt", '<'),
    ("gt", '>'),
    ("quot", '"'),
    ("apos", '\''),
    ("nbsp", ' '),
    ("mdash", '\u{2014}'),
    ("ndash", '\u{2013}'),
    ("hellip", '\u{2026}'),
    ("ldquo", '\u{201c}'),
    ("rdquo", '\u{201d}'),
    ("lsquo", '\u{2018}'),
    ("rsquo", '\u{2019}'),
    ("bull", '\u{2022}'),
    ("deg", '\u{b0}'),
    ("copy", '\u{a9}'),
    ("reg", '\u{ae}'),
    ("trade", '\u{2122}'),
    ("times", '\u{d7}'),
    ("middot", '\u{b7}'),
];

/// One word's character references, resolved. Anything that is not a reference -- a bare
/// `&` in `A & B`, a name nobody defined -- stays written as the author wrote it.
fn resolve_entities(word: &str) -> String {
    if !word.contains('&') {
        return word.to_string();
    }
    let mut out = String::with_capacity(word.len());
    let mut rest = word;
    loop {
        let Some(at) = rest.find('&') else {
            out.push_str(rest);
            return out;
        };
        out.push_str(&rest[..at]);
        let after = &rest[at + 1..];
        // Without its terminator a reference is the character an author typed and nothing
        // else, so it is emitted and the scan moves past it rather than looking again.
        let Some(end) = after.find(';') else {
            out.push('&');
            rest = after;
            continue;
        };
        let name = &after[..end];
        let resolved = name
            .strip_prefix('#')
            .and_then(|digits| match digits.strip_prefix(['x', 'X']) {
                Some(hex) => u32::from_str_radix(hex, 16).ok(),
                None => digits.parse::<u32>().ok(),
            })
            .and_then(char::from_u32)
            .or_else(|| ENTITIES.iter().find(|(k, _)| *k == name).map(|(_, c)| *c));
        match resolved {
            Some(c) => {
                out.push(c);
                rest = &after[end + 1..];
            }
            None => {
                out.push('&');
                rest = after;
            }
        }
    }
}
