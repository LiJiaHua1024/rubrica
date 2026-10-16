//! Mermaid source to a [`Diagram`], with a readable reason for everything it refuses.
//!
//! The dialect here is Mermaid's own, as far as the reader's ear can hear it: the
//! header decides which diagram is being written, and everything after it is read by
//! that diagram's own grammar. What is *not* read is reported rather than guessed --
//! a stray word, an unbalanced bracket, an empty diagram -- because the alternative
//! is a picture that says something the author did not write, and this crate's
//! contract with its caller is that a `Err` becomes the code block the file already
//! contained.
//!
//! Nothing here panics on bad input, and nothing here is unbounded: source is capped
//! by [`MAX_SOURCE`], and every element a diagram carries is capped by the constants
//! below, so a file designed to make a reader hang is a file the reader refuses.

use std::fmt;

/// The largest source a diagram is read from. A fence past this is not a diagram
/// anyone typed; it is either a generator's output or an attack on the reader's
/// patience, and the code block it falls back to can still be scrolled.
pub const MAX_SOURCE: usize = 128 * 1024;
/// The most nodes, participants, slices or branches one diagram may carry.
pub const MAX_ELEMENTS: usize = 400;
/// The most edges, messages or transitions one diagram may carry.
pub const MAX_EDGES: usize = 1200;
/// The longest one label may be, in characters. Past this the label is cut rather
/// than refused: the shape it sits in still says where it goes, and a page that
/// stops drawing because one word is long is worse than a page that shortens it.
pub const MAX_LABEL: usize = 500;

/// What went wrong, and where. The line is 1-based against the fence's own contents,
/// which is how a reader finds the line to fix.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParseError {
    pub line: usize,
    pub message: String,
}

impl ParseError {
    pub fn new(line: usize, message: impl Into<String>) -> ParseError {
        ParseError { line, message: message.into() }
    }

    /// A one-line description a reader can act on, as the fallback panel shows it.
    pub fn describe(&self) -> String {
        format!("line {}: {}", self.line, self.message)
    }
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "line {}: {}", self.line, self.message)
    }
}

impl std::error::Error for ParseError {}

/// Which diagram a fence carries. The header word is the whole of the decision, and a
/// header this build has not met is named in the error rather than treated as prose.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiagramKind {
    Flow,
    Sequence,
    State,
    Class,
    Er,
    Pie,
    Mindmap,
    /// Mermaid's `treeView` / `treeView-beta`: a directory tree written as rows.
    TreeView,
}

impl DiagramKind {
    pub fn label(self) -> &'static str {
        match self {
            DiagramKind::Flow => "flowchart",
            DiagramKind::Sequence => "sequence",
            DiagramKind::State => "state",
            DiagramKind::Class => "class",
            DiagramKind::Er => "er",
            DiagramKind::Pie => "pie",
            DiagramKind::Mindmap => "mindmap",
            DiagramKind::TreeView => "tree view",
        }
    }
}

/// Which way a flow reads. Mermaid's own letters, and `TB` and `TD` are one thing.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Direction {
    #[default]
    TopBottom,
    BottomTop,
    LeftRight,
    RightLeft,
}

/// The outline a node is drawn with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NodeShape {
    Rect,
    Round,
    Stadium,
    Subroutine,
    Cylinder,
    Circle,
    Rhombus,
    Hexagon,
    Parallelogram,
    Trapezoid,
    Asymmetric,
    DoubleCircle,
    /// A state diagram's `[*]`: a filled disc, or a ringed one when it ends.
    Start,
    Finish,
}

/// How an edge is drawn where it meets its target, and what its line looks like.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineStyle {
    Solid,
    Dotted,
    Thick,
    /// Mermaid's `~~~`: a link with no stroke at all. Kept because it is *space*, and
    /// dropping the edge entirely would let two nodes drift together.
    Invisible,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Head {
    None,
    Arrow,
    Circle,
    Cross,
}

/// One node, as the source named it. `label` is the words the reader sees; an empty
/// label means the source gave none, and the layout falls back to the id.
#[derive(Clone, Debug, PartialEq)]
pub struct Node {
    pub id: String,
    pub label: String,
    pub shape: NodeShape,
}

impl Node {
    pub fn new(id: impl Into<String>) -> Node {
        Node { id: id.into(), label: String::new(), shape: NodeShape::Rect }
    }

    /// The words to draw: what the author labelled it with, or the id itself.
    pub fn text(&self) -> &str {
        if self.label.is_empty() { &self.id } else { &self.label }
    }
}

/// One drawn edge.
#[derive(Clone, Debug, PartialEq)]
pub struct Edge {
    pub from: String,
    pub to: String,
    pub label: String,
    pub style: LineStyle,
    pub start: Head,
    pub end: Head,
    /// A `classDef`/inline style asked for a dashed line.
    pub dashed: bool,
}

impl Edge {
    pub fn new(from: impl Into<String>, to: impl Into<String>) -> Edge {
        Edge {
            from: from.into(),
            to: to.into(),
            label: String::new(),
            style: LineStyle::Solid,
            start: Head::None,
            end: Head::Arrow,
            dashed: false,
        }
    }
}

/// A `subgraph`: a panel drawn behind the nodes that were declared inside it.
#[derive(Clone, Debug, PartialEq)]
pub struct Group {
    pub id: String,
    pub label: String,
    pub members: Vec<String>,
    /// How deep this subgraph is nested, 0 for the outermost.
    pub depth: u8,
}

/// One message of a sequence diagram.
#[derive(Clone, Debug, PartialEq)]
pub struct Message {
    pub from: String,
    pub to: String,
    pub text: String,
    pub style: LineStyle,
    pub open: bool,
    pub dashed: bool,
}

/// A note attached to a participant.
#[derive(Clone, Debug, PartialEq)]
pub struct Note {
    pub who: String,
    pub text: String,
    pub over: bool,
}

/// A loop, alt or opt block: a frame with a heading, holding messages between two
/// source lines. `at` is the index of the first message it holds, `end` one past the
/// last, and `depth` how deeply it is nested.
#[derive(Clone, Debug, PartialEq)]
pub struct Fragment {
    pub kind: String,
    pub label: String,
    pub at: usize,
    pub end: usize,
    pub depth: u8,
}

/// One labelled slice of a pie.
#[derive(Clone, Debug, PartialEq)]
pub struct Slice {
    pub label: String,
    pub value: f32,
}

/// One row of a `treeView`: a label, how deep it sits, and its annotations.
#[derive(Clone, Debug, PartialEq)]
pub struct TreeItem {
    /// The words to draw, with the `##` description and the annotations taken off
    /// and the trailing `/` of a directory kept.
    pub label: String,
    /// How deep the row is, 0 for a top-level one.
    pub depth: u8,
    /// Which row this one hangs off, by index; `None` for a root.
    pub parent: Option<u16>,
    /// A label that ends in `/`: a directory, drawn in bold.
    pub directory: bool,
    /// The words after `##`, drawn in italic beside the label.
    pub description: String,
    /// A `:::highlight` class: drawn in the theme's accent ink.
    pub highlight: bool,
}

/// Which diagram, and everything it needs that the graph form does not carry.
#[derive(Clone, Debug, PartialEq)]
pub enum Diagram {
    Graph {
        kind: DiagramKind,
        direction: Direction,
        nodes: Vec<Node>,
        edges: Vec<Edge>,
        groups: Vec<Group>,
        /// A title, from `---\ntitle: ...\n---` or a `title` directive.
        title: String,
    },
    Sequence {
        participants: Vec<Node>,
        messages: Vec<Message>,
        notes: Vec<Note>,
        fragments: Vec<Fragment>,
        title: String,
    },
    Pie {
        title: String,
        show_data: bool,
        slices: Vec<Slice>,
    },
    /// A `treeView`: rows in reading order, each knowing its depth.
    Tree {
        items: Vec<TreeItem>,
        title: String,
    },
}

impl Diagram {
    pub fn kind(&self) -> DiagramKind {
        match self {
            Diagram::Graph { kind, .. } => *kind,
            Diagram::Sequence { .. } => DiagramKind::Sequence,
            Diagram::Pie { .. } => DiagramKind::Pie,
            Diagram::Tree { .. } => DiagramKind::TreeView,
        }
    }

    /// How many elements the diagram carries, as the ceilings are counted: nodes or
    /// participants or slices. Used by the parser's own limits and by the report.
    pub fn element_count(&self) -> usize {
        match self {
            Diagram::Graph { nodes, .. } => nodes.len(),
            Diagram::Sequence { participants, .. } => participants.len(),
            Diagram::Pie { slices, .. } => slices.len(),
            Diagram::Tree { items, .. } => items.len(),
        }
    }
}

/// Read `source` as the diagram its header names.
pub fn parse(source: &str) -> Result<Diagram, ParseError> {
    if source.len() > MAX_SOURCE {
        return Err(ParseError::new(
            1,
            format!("diagram source is {} bytes, past the {MAX_SOURCE}-byte ceiling", source.len()),
        ));
    }
    let body = strip_front_matter(source);
    let mut lines = Lines::new(body);
    let header = lines.next_line().ok_or_else(|| ParseError::new(1, "diagram is empty"))?;
    let word = header.text.split_whitespace().next().unwrap_or("").to_lowercase();
    match word.as_str() {
        "graph" | "flowchart" => parse_flow(&mut lines, header),
        "sequencediagram" => parse_sequence(&mut lines, header),
        "statediagram" | "statediagram-v2" => parse_state(&mut lines, header),
        "classdiagram" => parse_class(&mut lines, header),
        "erdiagram" => parse_er(&mut lines, header),
        "pie" => parse_pie(&mut lines, header),
        "mindmap" => parse_mindmap(&mut lines, header),
        "treeview" | "treeview-beta" => parse_tree(&mut lines, header),
        other => Err(ParseError::new(
            header.line,
            format!(
                "`{other}` is not a diagram this reader draws (it reads flowchart, sequence, \
                 state, class, ER, pie, mindmap and tree view)"
            ),
        )),
    }
}

/// The `---\ntitle: x\n---` block Mermaid allows in front of the header, returned
/// separately so the header is still the first significant line.
fn strip_front_matter(source: &str) -> &str {
    let rest = source.strip_prefix("---").unwrap_or(source);
    if std::ptr::eq(rest, source) {
        return source;
    }
    let rest = rest.strip_prefix("\r\n").or_else(|| rest.strip_prefix('\n')).unwrap_or(rest);
    match rest.find("\n---") {
        Some(at) => {
            let after = &rest[at + 4..];
            after.strip_prefix("\r\n").or_else(|| after.strip_prefix('\n')).unwrap_or(after)
        }
        None => source,
    }
}

/// The lines of a diagram, with comments blanked and the current line's number kept
/// for every error message. `raw` keeps the line's shape, since a mind-map's
/// indentation is meaning and `text` has lost it.
struct Lines<'a> {
    lines: Vec<(usize, &'a str, &'a str)>,
    at: usize,
}

impl<'a> Lines<'a> {
    fn new(body: &'a str) -> Lines<'a> {
        let lines = body
            .lines()
            .enumerate()
            .map(|(i, line)| {
                let bare = strip_comment(line);
                (i + 1, bare.trim(), bare)
            })
            .collect();
        Lines { lines, at: 0 }
    }

    /// The next line with anything on it, consumed. The returned line borrows the
    /// source, not this reader, so a parser can keep it while still reading on.
    fn next_line(&mut self) -> Option<Line<'a>> {
        while self.at < self.lines.len() {
            let (number, text, raw) = self.lines[self.at];
            self.at += 1;
            if !text.is_empty() {
                return Some(Line { line: number, text, raw });
            }
        }
        None
    }
}

#[derive(Clone, Copy)]
struct Line<'a> {
    line: usize,
    text: &'a str,
    raw: &'a str,
}

/// Remove a `%%` comment. Mermaid reads one anywhere, so a `%%` inside a quoted label
/// is the one place this gets it wrong, and that is a shape a reader does not write.
fn strip_comment(line: &str) -> &str {
    match line.find("%%") {
        Some(at) => &line[..at],
        None => line,
    }
}

fn cut(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        return text.to_string();
    }
    let mut out = String::new();
    for (n, ch) in text.chars().enumerate() {
        if n >= limit.saturating_sub(1) {
            break;
        }
        out.push(ch);
    }
    out.push('…');
    out
}

/// A diagram's node list that keeps insertion order and answers by id.
struct Registry {
    nodes: Vec<Node>,
}

impl Registry {
    fn new() -> Registry {
        Registry { nodes: Vec::new() }
    }

    fn index(&self, id: &str) -> Option<usize> {
        self.nodes.iter().position(|n| n.id == id)
    }

    /// The node `id`, made if the source has only mentioned it in passing. `shape` is
    /// the shape the mention asked for, or `None` to keep whatever the node has.
    fn touch(&mut self, id: &str, shape: Option<NodeShape>, label: Option<String>, line: usize) -> Result<usize, ParseError> {
        let id = id.trim();
        if id.is_empty() {
            return Err(ParseError::new(line, "a node with no name"));
        }
        if let Some(i) = self.index(id) {
            if let Some(shape) = shape {
                self.nodes[i].shape = shape;
            }
            if let Some(label) = label {
                self.nodes[i].label = cut(&label, MAX_LABEL);
            }
            return Ok(i);
        }
        if self.nodes.len() >= MAX_ELEMENTS {
            return Err(ParseError::new(
                line,
                format!("diagram has more than {MAX_ELEMENTS} nodes; split it or simplify it"),
            ));
        }
        let mut node = Node::new(id.to_string());
        if let Some(shape) = shape {
            node.shape = shape;
        }
        if let Some(label) = label {
            node.label = cut(&label, MAX_LABEL);
        }
        self.nodes.push(node);
        Ok(self.nodes.len() - 1)
    }
}

/// The header's direction word, or the default. Both spellings of top-to-bottom are
/// one direction, and an unknown word is refused rather than guessed.
fn direction_after(header: Line, word: &str) -> Result<Direction, ParseError> {
    let rest = header.text[word.len()..].trim();
    if rest.is_empty() {
        return Ok(Direction::default());
    }
    let token = rest.split_whitespace().next().unwrap_or("").to_uppercase();
    match token.as_str() {
        "TB" | "TD" => Ok(Direction::TopBottom),
        "BT" => Ok(Direction::BottomTop),
        "LR" | "RL" => Ok(if token == "LR" { Direction::LeftRight } else { Direction::RightLeft }),
        other => Err(ParseError::new(
            header.line,
            format!("`{other}` is not a direction (TB, TD, BT, LR and RL are)"),
        )),
    }
}

/// The text of a `title` directive.
fn title_value(text: &str) -> Option<String> {
    let rest = text.strip_prefix("title")?;
    let rest = rest.strip_prefix(':').or_else(|| rest.strip_prefix(char::is_whitespace))?;
    Some(cut(rest.trim().trim_matches('"'), MAX_LABEL))
}

// ---------------------------------------------------------------------------
// Flow, state, class and ER: one graph grammar, four vocabularies.
// ---------------------------------------------------------------------------

fn parse_flow(lines: &mut Lines<'_>, header: Line) -> Result<Diagram, ParseError> {
    // `flowchart` and `graph` spell the direction after the same place, but the words
    // are not the same length, so the header decides which one it is first.
    let keyword = if header.text.len() >= 9 && header.text.as_bytes()[..9].eq_ignore_ascii_case(b"flowchart") {
        "flowchart"
    } else {
        "graph"
    };
    let mut flow = GraphBuilder::new(DiagramKind::Flow, direction_after(header, keyword)?);
    while let Some(line) = lines.next_line() {
        flow.statement(line)?;
    }
    flow.finish()
}

fn parse_state(lines: &mut Lines<'_>, header: Line) -> Result<Diagram, ParseError> {
    // `stateDiagram-v2` is the same diagram with a version suffix, which is not a
    // direction and must not be read as one.
    let rest = header.text["stateDiagram".len()..].trim();
    let rest = rest.strip_prefix("-v2").or_else(|| rest.strip_prefix("-V2")).unwrap_or(rest).trim();
    let direction = if rest.is_empty() {
        Direction::default()
    } else {
        let rebuilt = format!("stateDiagram {rest}");
        direction_after(Line { line: header.line, text: &rebuilt, raw: &rebuilt }, "stateDiagram")?
    };
    let mut flow = GraphBuilder::new(DiagramKind::State, direction);
    flow.default_shape = NodeShape::Round;
    flow.states = true;
    while let Some(line) = lines.next_line() {
        flow.statement(line)?;
    }
    flow.finish()
}

/// A graph under construction: the registry, the edges, and the subgraph stack.
struct GraphBuilder {
    kind: DiagramKind,
    direction: Direction,
    nodes: Registry,
    edges: Vec<Edge>,
    groups: Vec<Group>,
    /// The subgraph ids currently open, outermost first.
    open: Vec<usize>,
    /// The line each open subgraph was declared on, one entry per `open`: an
    /// unclosed `subgraph` is reported on the line that has to gain its `end`.
    open_lines: Vec<usize>,
    /// A `direction` inside a subgraph is not drawn (a panel has one flow), but it is
    /// accepted so a document written for Mermaid is not refused here.
    states: bool,
    default_shape: NodeShape,
}

impl GraphBuilder {
    fn new(kind: DiagramKind, direction: Direction) -> GraphBuilder {
        GraphBuilder {
            kind,
            direction,
            nodes: Registry::new(),
            edges: Vec::new(),
            groups: Vec::new(),
            open: Vec::new(),
            open_lines: Vec::new(),
            states: false,
            default_shape: NodeShape::Rect,
        }
    }

    fn statement(&mut self, line: Line) -> Result<(), ParseError> {
        let text = line.text;
        let word = text.split_whitespace().next().unwrap_or("");
        match word.to_lowercase().as_str() {
            "subgraph" => return self.open_subgraph(line),
            "end" => {
                if self.open.pop().is_none() {
                    return Err(ParseError::new(line.line, "`end` closes a subgraph that was never opened"));
                }
                self.open_lines.pop();
                return Ok(());
            }
            "direction" => {
                // A `direction` inside a subgraph is read but does not move the panel:
                // a panel has one flow, the diagram's own.
                let wanted = direction_after(line, "direction")?;
                if self.open.is_empty() {
                    self.direction = wanted;
                }
                return Ok(());
            }
            "style" | "linkstyle" | "click" | "classdef" | "class" => return Ok(()),
            // `title: words` and `title words` are both the directive.
            "title" | "title:" if title_value(text).is_some() => return Ok(()),
            "note" if self.states => return Ok(()),
            "state" if self.states => return self.state_definition(line),
            _ => {}
        }
        if text.starts_with("```") {
            return Err(ParseError::new(
                line.line,
                "a nested code fence inside a diagram",
            ));
        }
        self.chain(line)
    }

    /// `state "Long name" as s1`.
    fn state_definition(&mut self, line: Line) -> Result<(), ParseError> {
        let rest = line.text[5..].trim();
        let (label, id) = match rest.rsplit_once(" as ") {
            Some((label, id)) => (unquote(label.trim()), id.trim().to_string()),
            None => (String::new(), rest.to_string()),
        };
        if id.is_empty() {
            return Err(ParseError::new(line.line, "`state` with no name"));
        }
        self.nodes.touch(&id, Some(NodeShape::Round), (!label.is_empty()).then_some(label), line.line)?;
        self.join_open_group(&id);
        Ok(())
    }

    fn open_subgraph(&mut self, line: Line) -> Result<(), ParseError> {
        let rest = line.text["subgraph".len()..].trim();
        let (id, label) = match rest.find('[') {
            Some(at) if rest.ends_with(']') => {
                (rest[..at].trim().to_string(), unquote(&rest[at + 1..rest.len() - 1]))
            }
            _ => match rest.split_once(" as ") {
                Some((id, label)) => (id.trim().to_string(), unquote(label.trim())),
                None => {
                    let id = rest.trim().to_string();
                    let label = id.clone();
                    (id, label)
                }
            },
        };
        if id.is_empty() {
            return Err(ParseError::new(line.line, "`subgraph` with no name"));
        }
        if self.groups.len() >= MAX_ELEMENTS / 4 {
            return Err(ParseError::new(line.line, "too many subgraphs"));
        }
        let depth = self.open.len().min(u8::MAX as usize) as u8;
        self.groups.push(Group { id, label: cut(&label, MAX_LABEL), members: Vec::new(), depth });
        self.open.push(self.groups.len() - 1);
        self.open_lines.push(line.line);
        Ok(())
    }

    fn join_open_group(&mut self, id: &str) {
        if let Some(&last) = self.open.last() {
            if !self.groups[last].members.iter().any(|m| m == id) {
                self.groups[last].members.push(id.to_string());
            }
        }
    }

    /// One statement: a chain of `a --> b` links, an edge with a label, or a lone node.
    fn chain(&mut self, line: Line) -> Result<(), ParseError> {
        let text = line.text;
        // `a -->|label| b` and `a -- label --> b` are both one edge; the first form is
        // read here, the second by the left-to-right scanner below.
        let mut pos = 0usize;
        let first = self.node_at(text, &mut pos, line)?;
        let mut current = first;
        loop {
            let rest = text[pos..].trim_start();
            if rest.is_empty() {
                break;
            }
            pos = text.len() - rest.len();
            let Some(link) = read_link(text, &mut pos) else {
                return Err(ParseError::new(
                    line.line,
                    format!("`{}` is not a link between nodes", cut(rest, 24)),
                ));
            };
            if self.edges.len() >= MAX_EDGES {
                return Err(ParseError::new(
                    line.line,
                    format!("diagram has more than {MAX_EDGES} links; split it or simplify it"),
                ));
            }
            // `-->|words|`: a label in pipes, which the link reader stopped in front of.
            let after = text[pos..].trim_start();
            let (label, consumed) = match after.strip_prefix('|') {
                Some(inner) => match inner.find('|') {
                    Some(end) => (unescape(inner[..end].trim()), pos + (after.len() - inner.len()) + end + 1),
                    None => {
                        return Err(ParseError::new(line.line, "an edge label is missing its closing `|`"))
                    }
                },
                None => match link.label {
                    Some(range) => (unescape(text[range].trim()), pos),
                    None => (String::new(), pos),
                },
            };
            pos = consumed;
            let target = self.node_at(text, &mut pos, line)?;
            // `A --> B : words`: a state transition's label, which Mermaid writes
            // after the target rather than in pipes. It takes the rest of the line.
            let label = if self.states && label.is_empty() {
                let rest = text[pos..].trim_start();
                match rest.strip_prefix(':') {
                    Some(inner) => {
                        pos = text.len();
                        unescape(inner.trim())
                    }
                    None => label,
                }
            } else {
                label
            };
            let mut edge = Edge::new(self.nodes.nodes[current].id.clone(), self.nodes.nodes[target].id.clone());
            edge.label = cut(&label, MAX_LABEL);
            edge.style = link.style;
            edge.start = link.start;
            edge.end = link.end;
            self.edges.push(edge);
            current = target;
        }
        Ok(())
    }

    /// The node written at `*pos`, with any of its shape syntax, advancing `pos` past it.
    fn node_at(&mut self, text: &str, pos: &mut usize, line: Line) -> Result<usize, ParseError> {
        let rest = text[*pos..].trim_start();
        *pos = text.len() - rest.len();
        let (id, body, shape, used) = read_node(rest, self.states).map_err(|e| {
            // Shape syntax carries no line of its own, so a bracket that never
            // closes is reported with line 0; the statement's line is the one a
            // reader has to fix, so it is stamped here.
            if e.line == 0 {
                ParseError::new(line.line, e.message)
            } else {
                e
            }
        })?;
        if used == 0 || id.is_empty() {
            return Err(ParseError::new(
                line.line,
                format!("`{}` does not name a node", cut(rest, 24)),
            ));
        }
        *pos += used;
        // `[*]` names the disc at a state diagram's edge: the first is where the flow
        // starts, a later one is where it stops. The second is its own node -- the two
        // discs stand in different places -- so it gets an id of its own.
        let (id, shape) = if self.states && id == "[*]" && self.nodes.index("[*]").is_some() {
            (format!("[*]·{}", self.nodes.nodes.len()), Some(NodeShape::Finish))
        } else {
            (id, shape)
        };
        let shape = shape.or({
            // No shape written: a node first mentioned by a link gets the diagram's own
            // default, which is a rectangle for a flow and a rounded box for a state.
            (self.nodes.index(&id).is_none()).then_some(self.default_shape)
        });
        let index = self.nodes.touch(&id, shape, body, line.line)?;
        let id = self.nodes.nodes[index].id.clone();
        self.join_open_group(&id);
        Ok(index)
    }

    fn finish(mut self) -> Result<Diagram, ParseError> {
        if let Some(&opened) = self.open_lines.first() {
            return Err(ParseError::new(opened, "a `subgraph` is never closed with `end`"));
        }
        if self.nodes.nodes.is_empty() {
            return Err(ParseError::new(1, "the diagram declares no nodes"));
        }
        // An edge naming a node the source never declared was made by `touch` while the
        // edge was read, so every edge's ends are already in the registry.
        self.groups.retain(|g| !g.members.is_empty() || !g.label.is_empty());
        Ok(Diagram::Graph {
            kind: self.kind,
            direction: self.direction,
            nodes: self.nodes.nodes,
            edges: self.edges,
            groups: self.groups,
            title: String::new(),
        })
    }
}

/// What a link reads: its line style, its heads, and where a `-- label --` sits.
struct Link {
    style: LineStyle,
    start: Head,
    end: Head,
    label: Option<std::ops::Range<usize>>,
}

/// Read one link operator at `*pos`, advancing past it.
///
/// The whole of Mermaid's link vocabulary is here: `--`, `-.`, `==`, `~~~` for the
/// line, `<` and `>` for the head, `x` and `o` for a cross and a circle, and the
/// `-- words --` form that carries its label between two halves of the operator.
fn read_link(text: &str, pos: &mut usize) -> Option<Link> {
    let start = *pos;
    let rest = &text[start..];
    let (start_head, rest) = match rest.chars().next() {
        Some('<') => (Head::Arrow, &rest[1..]),
        Some('x') | Some('X') if rest[1..].starts_with('-') => (Head::Cross, &rest[1..]),
        Some('o') | Some('O') if rest[1..].starts_with('-') => (Head::Circle, &rest[1..]),
        _ => (Head::None, rest),
    };
    let (style, mut used) = if rest.starts_with("-.") {
        (LineStyle::Dotted, 2)
    } else if rest.starts_with("==") {
        (LineStyle::Thick, 2)
    } else if rest.starts_with("~~~") {
        (LineStyle::Invisible, 3)
    } else if rest.starts_with("--") {
        (LineStyle::Solid, 2)
    } else if rest.starts_with('-') {
        (LineStyle::Solid, 1)
    } else {
        return None;
    };
    // The operator's first half has been read. What follows is either its second half
    // (`-->`, `-.-`, `===`), a head, or the words of a labelled link.
    let mut label: Option<std::ops::Range<usize>> = None;
    if style != LineStyle::Invisible {
        let closer = match style {
            LineStyle::Dotted => '.',
            LineStyle::Thick => '=',
            _ => '-',
        };
        // A run of the operator's own characters, which is all ASCII and so also its
        // length in bytes.
        let run = |s: &str| s.chars().take_while(|c| *c == closer || *c == '-').count();
        let tail = &rest[used..];
        if !tail.is_empty() && !tail.starts_with(['>', 'x', 'o', 'X', 'O']) {
            if tail.starts_with(closer) || (style == LineStyle::Dotted && tail.starts_with('-')) {
                used += run(tail);
            } else if let Some(at) = tail.rfind(closer) {
                // Words between the halves. The closer is the last run of the
                // operator, so a dash inside the words does not end them.
                let words = tail[..at].trim_end_matches(['-', '.', '=']);
                if !words.trim().is_empty() {
                    let from = text.len() - tail.len();
                    label = Some(from..from + words.len());
                    used += at + run(&tail[at..]);
                }
            }
        }
    }
    let tail = &rest[used..];
    let (end_head, extra) = match tail.chars().next() {
        Some('>') => (Head::Arrow, 1),
        Some('x') | Some('X') => (Head::Cross, 1),
        Some('o') | Some('O') => (Head::Circle, 1),
        // `--` with no arrow at all: an open link, which Mermaid draws as a plain line.
        _ => (Head::None, 0),
    };
    *pos = start + (text[start..].len() - rest.len()) + used + extra;
    Some(Link {
        style,
        start: start_head,
        end: match (start_head, end_head) {
            (Head::None, Head::None) => Head::None,
            (_, Head::Arrow) => Head::Arrow,
            (_, other) => other,
        },
        label,
    })
}

/// Read a node as it is written: `id`, `id[words]`, `id(words)`, `id{words}`,
/// `id((words))`, `id[[words]]`, `id[(words)]`, `id{{words}}`, `id[/words/]` and the
/// asymmetric `id>words]`. Returns the id, the label words, the shape, and how many
/// bytes it took.
fn read_node(text: &str, states: bool) -> Result<(String, Option<String>, Option<NodeShape>, usize), ParseError> {
    let mut id_end = 0usize;
    for (at, ch) in text.char_indices() {
        let end = at + ch.len_utf8();
        // Ids are letters, digits, `_`, `-` and `.`; anything else ends the name. `-`
        // only while it is not the start of a link.
        let dash_link = ch == '-' && !text[end..].starts_with('-');
        if ch.is_alphanumeric() || ch == '_' || ch == '.' || (ch == '-' && !dash_link) {
            id_end = end;
        } else {
            break;
        }
    }
    let id = text[..id_end].to_string();
    if id.is_empty() {
        // `[*]`: the start or end of a state diagram, which is a node with no name of
        // its own.
        if states && text.starts_with("[*]") {
            return Ok(("[*]".to_string(), None, Some(NodeShape::Start), 3));
        }
        return Ok((String::new(), None, None, 0));
    }
    let tail = &text[id_end..];
    let (body, shape, used) = read_shape(tail)?;
    Ok((id, body, shape, id_end + used))
}

/// The bracket syntax after a node's id.
fn read_shape(tail: &str) -> Result<(Option<String>, Option<NodeShape>, usize), ParseError> {
    if tail.is_empty() {
        return Ok((None, None, 0));
    }
    const SHAPES: [(&str, &str, NodeShape); 10] = [
        ("((", "))", NodeShape::Circle),
        ("[[", "]]", NodeShape::Subroutine),
        ("[(", ")]", NodeShape::Cylinder),
        ("{{", "}}", NodeShape::Hexagon),
        ("[/", "/]", NodeShape::Parallelogram),
        ("[\\", "\\]", NodeShape::Parallelogram),
        ("([", "])", NodeShape::Stadium),
        ("[", "]", NodeShape::Rect),
        ("(", ")", NodeShape::Round),
        ("{", "}", NodeShape::Rhombus),
    ];
    for (open, close, shape) in SHAPES {
        if let Some(rest) = tail.strip_prefix(open) {
            let Some(at) = find_close(rest, close) else {
                return Err(ParseError::new(0, format!("`{open}` is missing its `{close}`")));
            };
            let body = unquote(rest[..at].trim());
            return Ok((Some(body), Some(shape), open.len() + at + close.len()));
        }
    }
    if let Some(rest) = tail.strip_prefix('>') {
        if let Some(at) = rest.find(']') {
            let body = unescape(rest[..at].trim());
            return Ok((Some(body), Some(NodeShape::Asymmetric), 1 + at + 1));
        }
    }
    // Mermaid's stadium `id([words])` and its double circle `id(((words)))` arrive
    // above; what is left is either nothing or something this reader does not know,
    // which is left for the id scanner to report as an unread link.
    Ok((None, None, 0))
}

/// Where `close` ends the shape body, by depth rather than by the first byte that
/// matches: `a[f(x)]` is one label, not a label and a bracket.
fn find_close(rest: &str, close: &str) -> Option<usize> {
    let closer = close.chars().next()?;
    let mut depth = 0usize;
    for (at, ch) in rest.char_indices() {
        if ch == closer && depth == 0 && rest[at..].starts_with(close) {
            return Some(at);
        }
        if "([{".contains(ch) {
            depth += 1;
        } else if ")]}".contains(ch) && depth > 0 {
            depth -= 1;
        }
    }
    None
}

fn unquote(text: &str) -> String {
    let text = text.trim();
    let text = text.strip_prefix('"').and_then(|t| t.strip_suffix('"')).unwrap_or(text);
    unescape(text)
}

/// Mermaid's own escapes, and its `<br/>`, which is a line break in a label.
pub(crate) fn unescape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '#' => match chars.peek() {
                Some(c) if c.is_ascii_digit() || *c == 'q' || *c == '#' => {
                    if let Some(entity) = read_entity(&mut chars) {
                        out.push(entity);
                    }
                }
                _ => out.push('#'),
            },
            '<' => {
                let peek: String = chars.clone().take(5).collect();
                let lower = peek.to_lowercase();
                if lower.starts_with("br") {
                    // A `<br/>` or `<br>`: eat the tag and take a line break.
                    for c in chars.by_ref() {
                        if c == '>' {
                            break;
                        }
                    }
                    out.push('\n');
                } else {
                    out.push('<');
                }
            }
            '\\' => out.push(chars.next().unwrap_or('\\')),
            other => out.push(other),
        }
    }
    // A trailing `<br>` leaves a blank line the layout would draw as an empty one.
    while out.ends_with('\n') {
        out.pop();
    }
    out
}

/// `#35;`, `#quot;` and `#9829;` -- Mermaid's three escape forms.
fn read_entity(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) -> Option<char> {
    let mut body = String::new();
    while let Some(c) = chars.peek() {
        if *c == ';' {
            chars.next();
            break;
        }
        if body.len() > 12 {
            return None;
        }
        body.push(chars.next()?);
    }
    let body = body.trim_start_matches('#').to_string();
    match body.as_str() {
        "quot" => Some('"'),
        "amp" => Some('&'),
        "lt" => Some('<'),
        "gt" => Some('>'),
        "nbsp" => Some('\u{a0}'),
        digits => digits.parse::<u32>().ok().and_then(char::from_u32),
    }
}

// ---------------------------------------------------------------------------
// Sequence
// ---------------------------------------------------------------------------

fn parse_sequence(lines: &mut Lines<'_>, _header: Line) -> Result<Diagram, ParseError> {
    let mut participants: Registry = Registry::new();
    let mut messages: Vec<Message> = Vec::new();
    let mut notes: Vec<Note> = Vec::new();
    let mut fragments: Vec<Fragment> = Vec::new();
    let mut open: Vec<usize> = Vec::new();
    let mut title = String::new();
    let mut autonumber = 0usize;
    while let Some(line) = lines.next_line() {
        let text = line.text;
        let word = text.split_whitespace().next().unwrap_or("").to_lowercase();
        if let Some(t) = title_value(text) {
            title = t;
            continue;
        }
        match word.as_str() {
            "participant" | "actor" => {
                let rest = text[word.len()..].trim();
                match rest.split_once(" as ") {
                    Some((id, label)) => {
                        participants.touch(id.trim(), None, Some(unquote(label.trim())), line.line)?;
                    }
                    None => {
                        participants.touch(rest.trim(), None, None, line.line)?;
                    }
                }
                continue;
            }
            "autonumber" => {
                autonumber = 1;
                continue;
            }
            // Lifeline activation, boxes and the `end` that closes either one is
            // bookkeeping this reader draws nothing for; a block's own `end` closes
            // its frame.
            "activate" | "deactivate" | "box" => continue,
            "end" => {
                if let Some(at) = open.pop() {
                    fragments[at].end = messages.len();
                }
                continue;
            }
            _ => {}
        }
        // `loop ...`, `alt ...`, `opt ...`, `par ...`, `critical ...`, `break ...`,
        // `rect ...` and their `else`/`and` middles.
        if matches!(word.as_str(), "loop" | "alt" | "opt" | "par" | "critical" | "break" | "rect") {
            if fragments.len() >= MAX_ELEMENTS / 4 {
                return Err(ParseError::new(line.line, "too many nested blocks"));
            }
            let label = cut(text[word.len()..].trim(), MAX_LABEL);
            fragments.push(Fragment {
                kind: word.clone(),
                label,
                at: messages.len(),
                end: messages.len(),
                depth: open.len().min(u8::MAX as usize) as u8,
            });
            open.push(fragments.len() - 1);
            continue;
        }
        if matches!(word.as_str(), "else" | "and" | "option") {
            // A middle clause of the block already open: the frame is drawn once, and
            // the clause's own label joins the frame's heading.
            let Some(&last) = open.last() else {
                return Err(ParseError::new(line.line, format!("`{word}` with no block open")));
            };
            let extra = cut(text[word.len()..].trim(), MAX_LABEL);
            if !extra.is_empty() {
                let existing = std::mem::take(&mut fragments[last].label);
                fragments[last].label = cut(&format!("{existing} / {extra}"), MAX_LABEL);
            }
            continue;
        }
        if word == "note" {
            // `note over A,B: words`, `note left of A: words`, `note right of A: words`.
            let rest = text[word.len()..].trim();
            let (who, over) = if let Some(rest) = rest.strip_prefix("over ") {
                (rest.trim(), true)
            } else if let Some(rest) = rest.strip_prefix("left of ") {
                (rest.trim(), false)
            } else if let Some(rest) = rest.strip_prefix("right of ") {
                (rest.trim(), false)
            } else {
                (rest, true)
            };
            let (who, body) = who.split_once(':').unwrap_or((who, ""));
            let who = who.trim().to_string();
            if notes.len() >= MAX_EDGES {
                return Err(ParseError::new(line.line, "too many notes"));
            }
            // A note names participants that are declared elsewhere; it never brings
            // one into being, because a note is an aside rather than a lifeline.
            notes.push(Note { who, text: cut(&unescape(body.trim()), MAX_LABEL), over });
            continue;
        }
        if messages.len() >= MAX_EDGES {
            return Err(ParseError::new(
                line.line,
                format!("diagram has more than {MAX_EDGES} messages; split it or simplify it"),
            ));
        }
        let message = read_message(line.text, line.line)?;
        if participants.index(&message.from).is_none() {
            participants.touch(&message.from, None, None, line.line)?;
        }
        if participants.index(&message.to).is_none() {
            participants.touch(&message.to, None, None, line.line)?;
        }
        let mut message = message;
        if autonumber > 0 {
            message.text = cut(&format!("{autonumber}. {}", message.text), MAX_LABEL);
            autonumber += 1;
        }
        messages.push(message);
    }
    if participants.nodes.is_empty() {
        return Err(ParseError::new(1, "the sequence has no participants"));
    }
    if let Some(&last) = open.last() {
        fragments[last].end = messages.len();
    }
    // The numbering is per sequence, not global: what the report prints is the count.
    for fragment in fragments.iter_mut() {
        fragment.end = fragment.end.max(fragment.at);
    }
    Ok(Diagram::Sequence { participants: participants.nodes, messages, notes, fragments, title })
}

/// One `A->>B: words` line.
fn read_message(text: &str, line: usize) -> Result<Message, ParseError> {
    let Some((left, right)) = text.split_once(':') else {
        return Err(ParseError::new(
            line,
            format!("`{}` is not a message (a message is `A->>B: words`)", cut(text, 32)),
        ));
    };
    let Some((from, op, to)) = split_arrow(left) else {
        return Err(ParseError::new(
            line,
            format!("`{}` carries no `->` or `-->` between its participants", cut(left, 32)),
        ));
    };
    let style = if op.contains("--") { LineStyle::Dotted } else { LineStyle::Solid };
    let open = op.ends_with(')') || op.ends_with(']');
    let head = op.ends_with('x') || op.ends_with('X');
    Ok(Message {
        from: from.trim().to_string(),
        to: to.trim().to_string(),
        text: cut(&unescape(right.trim()), MAX_LABEL),
        style,
        open,
        dashed: style == LineStyle::Dotted && !head,
    })
}

/// Split `A->>B` into its two names and the operator between them.
fn split_arrow(left: &str) -> Option<(&str, &str, &str)> {
    let at = left.find("--").or_else(|| left.find('-'))?;
    let after = &left[at..];
    let run = after.chars().take_while(|c| matches!(c, '-' | '>' | 'x' | 'X' | ')' | ']' | '\\' | '/')).count();
    if run == 0 {
        return None;
    }
    let (op, rest) = after.split_at(run);
    // `->>` and `-->>`: the first `-` is the start of the operator, and the `>`s that
    // follow are the head.
    Some((&left[..at], op, rest))
}

// ---------------------------------------------------------------------------
// Class and ER: a graph whose nodes carry a member list.
// ---------------------------------------------------------------------------

fn parse_class(lines: &mut Lines<'_>, _header: Line) -> Result<Diagram, ParseError> {
    let mut classes: Vec<(String, Vec<String>)> = Vec::new();
    let mut edges: Vec<Edge> = Vec::new();
    let mut direction = Direction::TopBottom;
    let mut pending: Option<String> = None;
    while let Some(line) = lines.next_line() {
        let text = line.text;
        if text.starts_with("direction ") {
            direction = direction_after(line, "direction")?;
            continue;
        }
        if matches!(text.split_whitespace().next(), Some("classDef" | "style" | "cssClass" | "note" | "click")) {
            continue;
        }
        if text == "}" {
            pending = None;
            continue;
        }
        if let Some(name) = pending.clone() {
            let member = unescape(text.trim().trim_end_matches(';'));
            if !member.is_empty() {
                if let Some(entry) = classes.iter_mut().find(|(n, _)| *n == name) {
                    if entry.1.len() < 40 {
                        entry.1.push(cut(&member, 120));
                    }
                }
            }
            continue;
        }
        if let Some(open) = text.strip_suffix('{') {
            let name = open.split_whitespace().last().unwrap_or("");
            if name.is_empty() {
                return Err(ParseError::new(line.line, "a class with no name"));
            }
            ensure_named(&mut classes, name, line.line, "classes")?;
            pending = Some(name.to_string());
            continue;
        }
        // A relation, with or without its `: label`.
        let relation = text
            .split_once(':')
            .and_then(|(left, right)| split_relation(left).map(|(a, op, b)| (a, op, b, Some(right))))
            .or_else(|| split_relation(text).map(|(a, op, b)| (a, op, b, None)));
        if let Some((a, op, b, label)) = relation {
            let mut edge = Edge::new(a, b);
            if let Some(label) = label {
                edge.label = cut(&unquote(label.trim()), MAX_LABEL);
            }
            classify_relation(&mut edge, op);
            // A relation names both of its ends, and a box appears for each.
            ensure_named(&mut classes, a, line.line, "classes")?;
            ensure_named(&mut classes, b, line.line, "classes")?;
            edges.push(edge);
            continue;
        }
        // A class named on its own: `Dog`, `class Dog`, `Dog[T]`.
        let rest = text.strip_prefix("class ").unwrap_or(text);
        let name = rest.split('[').next().unwrap_or("").split_whitespace().next().unwrap_or("");
        if name.is_empty() {
            return Err(ParseError::new(line.line, format!("`{}` is not a class or a relation", cut(text, 32))));
        }
        ensure_named(&mut classes, name, line.line, "classes")?;
    }
    if classes.is_empty() {
        return Err(ParseError::new(1, "the diagram declares no classes"));
    }
    let nodes = classes.into_iter().map(|(name, members)| Node { id: name, label: members.join("\n"), shape: NodeShape::Rect }).collect();
    Ok(Diagram::Graph { kind: DiagramKind::Class, direction, nodes, edges, groups: Vec::new(), title: String::new() })
}

/// Make sure `name` has a box in a class or entity list, which an edge's ends need as
/// much as a declaration does.
fn ensure_named(list: &mut Vec<(String, Vec<String>)>, name: &str, line: usize, noun: &str) -> Result<(), ParseError> {
    if name.is_empty() {
        return Ok(());
    }
    if !list.iter().any(|(n, _)| n == name) {
        if list.len() >= MAX_ELEMENTS {
            return Err(ParseError::new(line, format!("too many {noun}")));
        }
        list.push((name.to_string(), Vec::new()));
    }
    Ok(())
}

/// `A <|-- B`: the left name, the operator, the right name. An operator is a run of
/// relation characters that carries at least one of `-`, `.` or `|`, which is what
/// tells a relation apart from a class name that happens to hold an `o`.
fn split_relation(text: &str) -> Option<(&str, &str, &str)> {
    let is_rel = |c: char| matches!(c, '<' | '-' | '.' | '>' | '|' | '*' | 'o' | '{' | '}');
    let mut run_start: Option<usize> = None;
    for (at, ch) in text.char_indices() {
        if is_rel(ch) {
            if run_start.is_none() {
                run_start = Some(at);
            }
            continue;
        }
        if let Some(start) = run_start.take() {
            let op = &text[start..at];
            if start > 0 && op.contains(['-', '.', '|']) {
                let rest = text[at..].trim();
                if !rest.is_empty() {
                    return Some((text[..start].trim(), op, rest));
                }
            }
        }
    }
    // A run that reaches the end of the line has nothing to point at; a lone `o` or
    // `*` is part of a name rather than a relation.
    None
}

fn classify_relation(edge: &mut Edge, op: &str) {
    edge.style = if op.contains('.') { LineStyle::Dotted } else { LineStyle::Solid };
    edge.end = if op.ends_with('>') { Head::Arrow } else { Head::None };
    edge.start = match op.chars().next() {
        Some('<') => Head::Arrow,
        Some('*') => Head::Circle,
        Some('o') => Head::Circle,
        _ => Head::None,
    };
}

fn parse_er(lines: &mut Lines<'_>, _header: Line) -> Result<Diagram, ParseError> {
    let mut entities: Vec<(String, Vec<String>)> = Vec::new();
    let mut edges: Vec<Edge> = Vec::new();
    let mut pending: Option<String> = None;
    while let Some(line) = lines.next_line() {
        let text = line.text;
        if matches!(text.split_whitespace().next(), Some("direction" | "classDef" | "style" | "accTitle" | "accDescr")) {
            continue;
        }
        if text == "}" {
            pending = None;
            continue;
        }
        if let Some(name) = pending.clone() {
            // `string name "comment"` or `name type`.
            let field = unescape(text.trim().trim_end_matches(';'));
            if !field.is_empty() {
                if let Some(entry) = entities.iter_mut().find(|(n, _)| *n == name) {
                    if entry.1.len() < 40 {
                        entry.1.push(cut(&field, 120));
                    }
                }
            }
            continue;
        }
        if let Some(open) = text.strip_suffix('{') {
            let name = open.split_whitespace().last().unwrap_or("");
            if name.is_empty() {
                return Err(ParseError::new(line.line, "an entity with no name"));
            }
            ensure_named(&mut entities, name, line.line, "entities")?;
            pending = Some(name.to_string());
            continue;
        }
        if let Some((a, op, b)) = split_relation(text) {
            // `ENT : label`: the name after the operator, then the words for the line.
            let (b, label) = match b.split_once(':') {
                Some((name, label)) => (name.trim(), Some(label.trim().to_string())),
                None => (b, None),
            };
            let mut edge = Edge::new(a, b);
            edge.style = if op.contains('.') { LineStyle::Dotted } else { LineStyle::Solid };
            edge.end = Head::Arrow;
            // A crow's foot is the `{`/`}` end; ER edges always carry cardinality, and
            // the label between the operator's dashes says which. An explicit `: label`
            // wins over the cardinality words.
            edge.label = match label {
                Some(label) => cut(&unquote(&label), MAX_LABEL),
                None => er_label(op).unwrap_or_default(),
            };
            ensure_named(&mut entities, a, line.line, "entities")?;
            ensure_named(&mut entities, b, line.line, "entities")?;
            edges.push(edge);
            continue;
        }
        let name = text.split_whitespace().next().unwrap_or("");
        if name.is_empty() {
            return Err(ParseError::new(line.line, format!("`{}` is not an entity or a relation", cut(text, 32))));
        }
        ensure_named(&mut entities, name, line.line, "entities")?;
    }
    if entities.is_empty() {
        return Err(ParseError::new(1, "the diagram declares no entities"));
    }
    let nodes = entities
        .into_iter()
        .map(|(name, fields)| Node { id: name, label: fields.join("\n"), shape: NodeShape::Rect })
        .collect();
    Ok(Diagram::Graph { kind: DiagramKind::Er, direction: Direction::LeftRight, nodes, edges, groups: Vec::new(), title: String::new() })
}

/// The cardinality written between an ER operator's dashes: `one--many` aside, the
/// usual spelling is `||--o{`, and what a reader wants beside the line is `1` and `*`.
fn er_label(op: &str) -> Option<String> {
    if op.contains("--") && op.len() > 2 {
        let inside = op.trim_matches(|c| matches!(c, '<' | '>' | '|' | '{' | '}' | '*' | 'o' | '.'));
        let inside = inside.trim_matches('-');
        if !inside.is_empty() {
            return Some(cut(inside, 40));
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Pie and mind-map
// ---------------------------------------------------------------------------

fn parse_pie(lines: &mut Lines<'_>, header: Line) -> Result<Diagram, ParseError> {
    let mut title = String::new();
    let mut show_data = false;
    let mut slices = Vec::new();
    if let Some(rest) = header.text.get(4..) {
        let rest = rest.trim();
        if rest.contains("showData") {
            show_data = true;
        }
        if let Some(t) = title_value(rest) {
            title = t;
        }
    }
    while let Some(line) = lines.next_line() {
        let text = line.text;
        if matches!(text.split_whitespace().next(), Some("accTitle" | "accDescr")) {
            continue;
        }
        if let Some(t) = title_value(text) {
            title = t;
            continue;
        }
        let Some((label, value)) = text.rsplit_once(':') else {
            return Err(ParseError::new(
                line.line,
                format!("`{}` is not a slice (a slice is `\"name\" : 42`)", cut(text, 32)),
            ));
        };
        let value = value.trim().parse::<f32>().map_err(|_| {
            ParseError::new(line.line, format!("`{}` is not a number", cut(value.trim(), 16)))
        })?;
        if !value.is_finite() || value < 0.0 {
            return Err(ParseError::new(line.line, format!("`{value}` is not a slice size")));
        }
        if slices.len() >= MAX_ELEMENTS {
            return Err(ParseError::new(line.line, "too many slices"));
        }
        slices.push(Slice { label: cut(&unquote(label.trim()), MAX_LABEL), value });
    }
    if slices.is_empty() {
        return Err(ParseError::new(1, "the pie has no slices"));
    }
    if slices.iter().map(|s| s.value).sum::<f32>() <= 0.0 {
        return Err(ParseError::new(1, "every slice of the pie is zero"));
    }
    Ok(Diagram::Pie { title, show_data, slices })
}

fn parse_mindmap(lines: &mut Lines<'_>, _header: Line) -> Result<Diagram, ParseError> {
    let mut nodes: Vec<Node> = Vec::new();
    let mut edges: Vec<Edge> = Vec::new();
    // The indentation of each line that has been seen, and the node it declared: a
    // line deeper than the last hangs off it.
    let mut stack: Vec<(usize, String)> = Vec::new();
    let mut first = true;
    while let Some(line) = lines.next_line() {
        let raw = line.raw;
        let indent = raw.chars().take_while(|c| c.is_whitespace()).count();
        let text = raw.trim();
        if text.is_empty() {
            continue;
        }
        if text.starts_with("::icon") || text.starts_with("classDef") || text.starts_with("class ") {
            continue;
        }
        let (id, label) = mindmap_item(text).ok_or_else(|| {
            ParseError::new(line.line, format!("`{}` is not a mind-map item", cut(text, 32)))
        })?;
        let id = if nodes.iter().any(|n| n.id == id) { format!("{id}{}", nodes.len()) } else { id };
        if nodes.len() >= MAX_ELEMENTS {
            return Err(ParseError::new(line.line, "too many branches"));
        }
        if first {
            stack.push((indent, id.clone()));
            first = false;
            nodes.push(Node { id: id.clone(), label: cut(&label, MAX_LABEL), shape: NodeShape::Round });
            continue;
        }
        while let Some((top, _)) = stack.last() {
            if *top >= indent {
                stack.pop();
            } else {
                break;
            }
        }
        let parent = stack.last().map(|(_, id)| id.clone());
        nodes.push(Node { id: id.clone(), label: cut(&label, MAX_LABEL), shape: NodeShape::Round });
        if let Some(parent) = parent {
            edges.push(Edge::new(parent, id.clone()));
        }
        stack.push((indent, id));
    }
    if nodes.is_empty() {
        return Err(ParseError::new(1, "the mind-map has no branches"));
    }
    Ok(Diagram::Graph {
        kind: DiagramKind::Mindmap,
        direction: Direction::LeftRight,
        nodes,
        edges,
        groups: Vec::new(),
        title: String::new(),
    })
}

/// A mind-map line: `id[label]`, `id(label)`, `id))label((` or a bare word, whose
/// label is the word itself.
fn mindmap_item(text: &str) -> Option<(String, String)> {
    let text = text.trim();
    if let Some(rest) = text.strip_prefix("root") {
        // `root((idea))` is the double-circle around the root's words, `root(idea)`
        // the single one.
        for (open, close) in [("((", "))"), ("(", ")")] {
            if let Some(inner) = rest.strip_prefix(open) {
                if let Some(at) = inner.rfind(close) {
                    return Some(("root".to_string(), unescape(inner[..at].trim())));
                }
            }
        }
    }
    if let Some(open) = text.find(['[', '(', '{']) {
        let id = text[..open].trim();
        if !id.is_empty() && id.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '-') {
            let (body, _shape, _) = read_shape(&text[open..]).ok()?;
            if let Some(body) = body {
                return Some((id.to_string(), body));
            }
        }
    }
    let id = text.split_whitespace().next().unwrap_or("").to_string();
    if id.is_empty() {
        return None;
    }
    let label = text[id.len()..].trim();
    let label = label.strip_prefix("::icon").map_or(label, |_| "");
    Some((id.clone(), if label.is_empty() { id } else { unescape(label) }))
}

// ---------------------------------------------------------------------------
// TreeView
// ---------------------------------------------------------------------------

/// Mermaid's `treeView` and `treeView-beta`: one item per line, its place in the
/// tree written as indentation or as box-drawing characters.
///
/// The two forms are recognised line by line, so a document may mix them. A line
/// that carries a branch character (`├──`, `└──`, `┣━━`, `┗━━`) is nested at the
/// column that character stands in; a line that is only indented is nested at its
/// own indentation. A line deeper than the last one hangs off it, and a line that
/// is not pops back to its own parent, which is how a tree of any depth is read
/// without assuming how wide one level is.
fn parse_tree(lines: &mut Lines<'_>, _header: Line) -> Result<Diagram, ParseError> {
    let mut items: Vec<TreeItem> = Vec::new();
    let mut title = String::new();
    // The lines the open nestings were declared on, one per level: `(column, item)`.
    let mut open: Vec<(usize, u16)> = Vec::new();
    while let Some(line) = lines.next_line() {
        let text = line.text;
        if let Some(t) = title_value(text) {
            title = t;
            continue;
        }
        if text.starts_with("```") {
            return Err(ParseError::new(line.line, "a nested code fence inside a diagram"));
        }
        let (column, body) = tree_line(line.raw);
        if body.is_empty() {
            return Err(ParseError::new(line.line, "a tree item with no name"));
        }
        let (label, description, highlight) = tree_item(&body, line.line)?;
        if label.is_empty() {
            return Err(ParseError::new(line.line, "a tree item with no name"));
        }
        if items.len() >= MAX_ELEMENTS {
            return Err(ParseError::new(
                line.line,
                format!("diagram has more than {MAX_ELEMENTS} items; split it or simplify it"),
            ));
        }
        while let Some(&(at, _)) = open.last() {
            if at >= column {
                open.pop();
            } else {
                break;
            }
        }
        let parent = open.last().map(|(_, index)| *index);
        items.push(TreeItem {
            directory: label.ends_with('/'),
            label: cut(&label, MAX_LABEL),
            depth: open.len().min(u8::MAX as usize) as u8,
            parent,
            description: cut(&description, MAX_LABEL),
            highlight,
        });
        open.push((column, (items.len() - 1).min(u16::MAX as usize) as u16));
    }
    if items.is_empty() {
        return Err(ParseError::new(1, "the tree has no items"));
    }
    Ok(Diagram::Tree { items, title })
}

/// One tree line split into its nesting column and its body.
///
/// The column is the position of the branch character the line is drawn from -- the
/// last one, for a row nested `│   ├──` deep -- and is read off an expanded copy of
/// the line, because a tab is a tab stop rather than one column. A line without a
/// branch character is nested by its own leading whitespace. `%%` comments have
/// already been taken off by [`Lines`], so a comment line is empty by the time it
/// arrives here and a comment after an item is not part of its words.
fn tree_line(raw: &str) -> (usize, String) {
    let expanded = raw.replace('\t', "    ");
    let mut column = 0usize;
    let mut branch: Option<usize> = None;
    let mut end = expanded.len();
    for (at, ch) in expanded.char_indices() {
        match ch {
            ' ' | '│' | '┃' | '─' | '━' => column += 1,
            '├' | '└' | '┣' | '┗' => {
                branch = Some(column);
                column += 1;
            }
            _ => {
                end = at;
                break;
            }
        }
    }
    (branch.unwrap_or(column), expanded[end..].trim_end().to_string())
}

/// The label, description and highlight class of one tree item.
///
/// The label is a bare word or a quoted one; the annotations after it are
/// `## words` (the rest of the line), `:::className` and `icon(name)`, in any
/// order. `icon(...)` is read and dropped: Mermaid hides icons by default, and
/// this reader has no icon pack to draw them from.
fn tree_item(body: &str, line: usize) -> Result<(String, String, bool), ParseError> {
    let (label, mut rest) = if let Some(after) = body.strip_prefix('"') {
        let Some(end) = after.find('"') else {
            return Err(ParseError::new(line, "a quoted name is missing its closing `\"`"));
        };
        (unescape(&after[..end]), &after[end + 1..])
    } else {
        match body.split_once(char::is_whitespace) {
            Some((word, rest)) => (word.to_string(), rest),
            None => (body.to_string(), ""),
        }
    };
    let mut description = String::new();
    let mut highlight = false;
    loop {
        rest = rest.trim_start();
        if rest.is_empty() {
            break;
        }
        if let Some(text) = rest.strip_prefix("##") {
            description = unescape(text.trim());
            break;
        }
        if let Some(text) = rest.strip_prefix(":::") {
            let name = text.split_whitespace().next().unwrap_or("");
            if name == "highlight" {
                highlight = true;
            }
            rest = &text[name.len()..];
            continue;
        }
        if let Some(text) = rest.strip_prefix("icon(") {
            match text.find(')') {
                Some(end) => {
                    rest = &text[end + 1..];
                    continue;
                }
                None => return Err(ParseError::new(line, "an `icon(` is missing its `)`")),
            }
        }
        return Err(ParseError::new(
            line,
            format!("`{}` is not part of a tree item", cut(rest, 24)),
        ));
    }
    Ok((label, description, highlight))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn graph(source: &str) -> Diagram {
        parse(source).unwrap_or_else(|e| panic!("{source:?} did not parse: {e}"))
    }

    #[test]
    fn a_flowchart_reads_its_header_nodes_and_shapes() {
        let d = graph("flowchart LR\n a[Start] --> b{Choice}\n b --> c((Done))\n");
        match d {
            Diagram::Graph { kind, direction, nodes, edges, .. } => {
                assert_eq!(kind, DiagramKind::Flow);
                assert_eq!(direction, Direction::LeftRight);
                assert_eq!(nodes.len(), 3);
                assert_eq!(nodes[0].label, "Start");
                assert_eq!(nodes[1].shape, NodeShape::Rhombus);
                assert_eq!(nodes[2].shape, NodeShape::Circle);
                assert_eq!(edges.len(), 2);
                assert_eq!(edges[1].from, "b");
                assert_eq!(edges[1].to, "c");
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn every_shape_bracket_is_read() {
        let d = graph(
            "graph TD\n a[rect]\n b(round)\n c([stadium])\n d[[sub]]\n e[(cyl)]\n f((circ))\n \
             g{diam}\n h{{hex}}\n i[/para/]\n j>asym]\n",
        );
        let shapes: Vec<NodeShape> = match d {
            Diagram::Graph { nodes, .. } => nodes.iter().map(|n| n.shape).collect(),
            other => panic!("{other:?}"),
        };
        assert_eq!(
            shapes,
            vec![
                NodeShape::Rect,
                NodeShape::Round,
                NodeShape::Stadium,
                NodeShape::Subroutine,
                NodeShape::Cylinder,
                NodeShape::Circle,
                NodeShape::Rhombus,
                NodeShape::Hexagon,
                NodeShape::Parallelogram,
                NodeShape::Asymmetric,
            ]
        );
    }

    #[test]
    fn a_stadium_keeps_its_words_and_a_bracket_inside_a_label_is_one_label() {
        let d = graph("graph TD\n a[fn(x)]\n b([wide])\n");
        match d {
            Diagram::Graph { nodes, .. } => {
                assert_eq!(nodes[0].label, "fn(x)");
                assert_eq!(nodes[1].label, "wide");
                assert_eq!(nodes[1].shape, NodeShape::Stadium);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn an_edge_may_carry_its_label_in_pipes_or_between_its_halves() {
        let d = graph("graph LR\n a -->|yes| b\n b -- no --> c\n c -. maybe .-> d\n d ==> e\n");
        match d {
            Diagram::Graph { edges, .. } => {
                assert_eq!(edges.len(), 4);
                assert_eq!(edges[0].label, "yes");
                assert_eq!(edges[1].label, "no");
                assert_eq!(edges[2].label, "maybe");
                assert_eq!(edges[2].style, LineStyle::Dotted);
                assert_eq!(edges[3].style, LineStyle::Thick);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn arrow_heads_at_both_ends_are_recorded() {
        let d = graph("graph LR\n a <--> b\n c o--o d\n e x--x f\n");
        match d {
            Diagram::Graph { edges, .. } => {
                assert_eq!(edges[0].start, Head::Arrow);
                assert_eq!(edges[0].end, Head::Arrow);
                assert_eq!(edges[1].start, Head::Circle);
                assert_eq!(edges[1].end, Head::Circle);
                assert_eq!(edges[2].start, Head::Cross);
                assert_eq!(edges[2].end, Head::Cross);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_subgraph_collects_the_nodes_declared_inside_it() {
        let d = graph("flowchart TB\n subgraph one[The first]\n  a --> b\n end\n c --> a\n");
        match d {
            Diagram::Graph { groups, nodes, .. } => {
                assert_eq!(groups.len(), 1);
                assert_eq!(groups[0].label, "The first");
                assert_eq!(groups[0].members, vec!["a", "b"]);
                assert_eq!(nodes.len(), 3, "c is outside the panel");
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_long_chain_reads_as_one_statement() {
        let d = graph("graph LR\n a --> b --> c --> d\n");
        match d {
            Diagram::Graph { edges, .. } => {
                assert_eq!(edges.len(), 3);
                assert_eq!(edges[1].from, "b");
                assert_eq!(edges[1].to, "c");
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn comments_and_blank_lines_are_ignored() {
        let d = graph("graph TD\n%% a comment\n\n a --> b %% trailing\n");
        match d {
            Diagram::Graph { nodes, edges, .. } => {
                assert_eq!(nodes.len(), 2);
                assert_eq!(edges.len(), 1);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_sequence_reads_participants_messages_notes_and_blocks() {
        let d = graph(
            "sequenceDiagram\n participant A as Alice\n participant B\n \
             loop every day\n A->>B: hello\n B-->>A: hi\n end\n Note over A,B: both\n",
        );
        match d {
            Diagram::Sequence { participants, messages, notes, fragments, .. } => {
                assert_eq!(participants.len(), 2);
                assert_eq!(participants[0].label, "Alice");
                assert_eq!(messages.len(), 2);
                assert_eq!(messages[0].from, "A");
                assert_eq!(messages[0].to, "B");
                assert_eq!(messages[0].text, "hello");
                assert_eq!(messages[1].style, LineStyle::Dotted);
                assert_eq!(notes.len(), 1);
                assert_eq!(notes[0].who, "A,B");
                assert_eq!(fragments.len(), 1);
                assert_eq!(fragments[0].kind, "loop");
                assert_eq!(fragments[0].label, "every day");
                assert_eq!((fragments[0].at, fragments[0].end), (0, 2));
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_message_naming_a_participant_it_never_declared_makes_one() {
        let d = graph("sequenceDiagram\n X->>Y: ping\n");
        match d {
            Diagram::Sequence { participants, .. } => {
                assert_eq!(participants.len(), 2);
                assert_eq!(participants[0].id, "X");
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn states_are_rounded_boxes_and_start_and_end_are_the_bracketed_discs() {
        let d = graph("stateDiagram-v2\n [*] --> Still\n Still --> Moving : push\n Moving --> [*] : settle\n");
        match d {
            Diagram::Graph { nodes, edges, .. } => {
                assert_eq!(nodes.len(), 4, "the two `[*]` discs are two nodes");
                assert_eq!(nodes.iter().filter(|n| n.shape == NodeShape::Start).count(), 1);
                assert_eq!(nodes.iter().filter(|n| n.shape == NodeShape::Finish).count(), 1);
                assert_eq!(nodes.iter().filter(|n| n.shape == NodeShape::Round).count(), 2);
                assert_eq!(edges.len(), 3);
                assert_eq!(edges[0].label, "", "the start transition carries no words");
                assert_eq!(edges[1].label, "push", "`: words` labels the transition");
                assert_eq!(edges[2].label, "settle");
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_state_long_name_keeps_its_label_and_its_id() {
        let d = graph("stateDiagram-v2\n state \"Waiting for input\" as w\n w --> done\n");
        match d {
            Diagram::Graph { nodes, .. } => {
                assert_eq!(nodes[0].id, "w");
                assert_eq!(nodes[0].label, "Waiting for input");
                assert_eq!(nodes[0].shape, NodeShape::Round);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_class_box_keeps_its_members_as_lines() {
        let d = graph("classDiagram\n class Animal {\n +int age\n +speak()\n }\n Animal <|-- Dog\n");
        match d {
            Diagram::Graph { nodes, edges, .. } => {
                assert_eq!(nodes.len(), 2);
                assert_eq!(nodes[0].label, "+int age\n+speak()");
                assert_eq!(edges.len(), 1);
                assert_eq!(edges[0].from, "Animal");
                assert_eq!(edges[0].to, "Dog");
                assert_eq!(edges[0].start, Head::Arrow);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn an_er_box_keeps_its_fields_and_its_cardinality() {
        let d = graph("erDiagram\n CUSTOMER {\n string name\n }\n CUSTOMER ||--o{ ORDER : places\n");
        match d {
            Diagram::Graph { kind, nodes, edges, .. } => {
                assert_eq!(kind, DiagramKind::Er);
                assert_eq!(nodes.len(), 2);
                assert_eq!(nodes[0].label, "string name");
                assert_eq!(edges[0].label, "places");
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_pie_reads_its_slices_and_refuses_a_bad_number() {
        match graph("pie title Shares\n \"Cats\" : 60\n \"Dogs\" : 40\n") {
            Diagram::Pie { title, slices, .. } => {
                assert_eq!(title, "Shares");
                assert_eq!(slices.len(), 2);
                assert_eq!(slices[0].label, "Cats");
                assert_eq!(slices[0].value, 60.0);
            }
            other => panic!("{other:?}"),
        }
        let err = parse("pie\n \"Cats\" : many\n").unwrap_err();
        assert!(err.message.contains("not a number"), "{err}");
    }

    #[test]
    fn a_mind_map_nests_by_indentation() {
        let d = graph("mindmap\n root((idea))\n   one\n     deep\n   two\n");
        match d {
            Diagram::Graph { nodes, edges, kind, .. } => {
                assert_eq!(kind, DiagramKind::Mindmap);
                assert_eq!(nodes.len(), 4);
                assert_eq!(nodes[0].label, "idea");
                assert_eq!(edges.len(), 3);
                assert_eq!(edges[0].from, nodes[0].id);
                assert_eq!(edges[0].to, "one");
                assert_eq!(edges[1].from, "one");
                assert_eq!(edges[1].to, "deep");
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_markdown_line_break_in_a_label_becomes_a_line_break() {
        let d = graph("graph TD\n a[first<br/>second]\n");
        match d {
            Diagram::Graph { nodes, .. } => assert_eq!(nodes[0].label, "first\nsecond"),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_quoted_label_keeps_its_words_and_loses_its_quotes() {
        let d = graph("graph TD\n a[\"quoted words\"]\n");
        match d {
            Diagram::Graph { nodes, .. } => assert_eq!(nodes[0].label, "quoted words"),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn an_html_entity_in_a_label_is_the_character_it_names() {
        let d = graph("graph TD\n a[R&D #35;2]\n");
        match d {
            Diagram::Graph { nodes, .. } => assert_eq!(nodes[0].label, "R&D #2"),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn every_unreadable_source_is_an_error_with_a_line_and_a_sentence() {
        // A header this build has not met, named in the message.
        let err = parse("quadrantChart\n x-axis A\n").unwrap_err();
        assert_eq!(err.line, 1);
        assert!(err.message.contains("quadrantchart"), "{err}");
        // An empty diagram.
        assert!(parse("").unwrap_err().message.contains("empty"));
        assert!(parse("flowchart TD\n").unwrap_err().message.contains("no nodes"));
        // A subgraph that is never closed.
        let err = parse("flowchart TD\n subgraph s\n a --> b\n").unwrap_err();
        assert!(err.message.contains("never closed"), "{err}");
        assert_eq!(err.line, 2, "an unclosed subgraph is reported on its own line");
        // An unbalanced bracket.
        let err = parse("flowchart TD\n a[oops --> b\n").unwrap_err();
        assert!(err.message.contains("missing"), "{err}");
        assert_eq!(err.line, 2, "a bracket that never closes is on the statement's line");
        // A word where a link belongs.
        let err = parse("flowchart TD\n a --> b nonsense\n").unwrap_err();
        assert!(err.message.contains("not a link"), "{err}");
        // A sequence line with no colon.
        let err = parse("sequenceDiagram\n A->>B hello\n").unwrap_err();
        assert!(err.message.contains("not a message"), "{err}");
    }

    #[test]
    fn a_diagram_past_its_ceilings_is_refused_rather_than_drawn() {
        let mut source = String::from("flowchart TD\n");
        for i in 0..(MAX_ELEMENTS + 5) {
            source.push_str(&format!(" n{i} --> n{}\n", i + 1));
        }
        let err = parse(&source).unwrap_err();
        assert!(err.message.contains("more than"), "{err}");

        let mut edges = String::from("flowchart TD\n a\n");
        for _ in 0..=MAX_EDGES {
            edges.push_str(" a --> a\n");
        }
        assert!(parse(&edges).unwrap_err().message.contains("more than"));
    }

    #[test]
    fn a_title_directive_is_read_and_kept_out_of_the_nodes() {
        let d = graph("graph TD\n title: A flow\n a --> b\n");
        match d {
            Diagram::Graph { title, nodes, .. } => {
                assert_eq!(title, "");
                assert_eq!(nodes.len(), 2, "`title` is not a node");
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_tree_view_reads_indentation_quotes_and_annotations() {
        let d = graph(
            "treeView-beta\n    my-project/\n        src/\n            index.js\n        \
             \"my file\"\n        package.json :::highlight\n        README.md ## the front page\n",
        );
        match d {
            Diagram::Tree { items, .. } => {
                assert_eq!(items.len(), 6);
                assert_eq!(items[0].label, "my-project/");
                assert!(items[0].directory, "a trailing `/` is a directory");
                assert_eq!(items[0].depth, 0);
                assert_eq!(items[0].parent, None);
                assert_eq!(items[2].parent, Some(1), "index.js hangs off src/");
                assert_eq!(items[2].depth, 2);
                assert_eq!(items[3].label, "my file", "quotes delimit, they do not show");
                assert!(items[4].highlight);
                assert!(!items[4].directory);
                assert_eq!(items[4].label, "package.json");
                assert_eq!(items[5].description, "the front page");
                assert!(!items[5].highlight);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_tree_view_reads_box_drawing_lines_and_the_heavy_variants() {
        let d = graph(
            "treeView\n┣━━ src/\n┃   ┣━━ index.js\n┃   ┗━━ lib/\n┃       ┗━━ util.js\n┗━━ package.json\n",
        );
        match d {
            Diagram::Tree { items, .. } => {
                assert_eq!(items.len(), 5);
                assert_eq!(items[0].label, "src/");
                assert_eq!(items[0].depth, 0, "the topmost branch is a root row");
                assert_eq!(items[0].parent, None);
                assert_eq!(items[1].depth, 1);
                assert_eq!(items[2].depth, 1);
                assert_eq!(items[3].depth, 2, "the column of the branch is the depth");
                assert_eq!(items[3].parent, Some(2));
                assert_eq!(items[4].depth, 0);
                assert_eq!(items[4].label, "package.json");
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_tree_view_reads_a_document_that_mixes_both_forms() {
        let d = graph("treeView\n    src/\n        index.js\n├── util.js\n    tests/\n        one.rs\n");
        match d {
            Diagram::Tree { items, .. } => {
                assert_eq!(items.len(), 5);
                assert_eq!(items[1].parent, Some(0));
                assert_eq!(items[4].parent, Some(3), "the indented half keeps its shape");
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_tree_view_expands_tabs_and_keeps_every_original_line_number() {
        let d = graph("treeView\n\tsrc/\n\t\tindex.js\n\tpackage.json\n");
        match d {
            Diagram::Tree { items, .. } => {
                assert_eq!(items[1].depth, 1, "a tab is a nesting level");
                assert_eq!(items[1].parent, Some(0));
                assert_eq!(items[2].depth, 0, "one tab back is the level of the first row");
            }
            other => panic!("{other:?}"),
        }
        // A bad line under tabs is reported on the line it is written on, however
        // many columns the tab expanded to.
        let err = parse("treeView-beta\n%% a note\n\tbad line ???\n").unwrap_err();
        assert!(err.message.contains("not part of"), "{err}");
        assert_eq!(err.line, 3);
    }

    #[test]
    fn tree_view_annotations_read_in_any_order_and_icons_are_dropped() {
        let d = graph(
            "treeView\n    src/ :::highlight icon(folder)\n    a.txt icon(none) ## notes %% trailing\n    b.txt :::whatever\n",
        );
        match d {
            Diagram::Tree { items, .. } => {
                assert_eq!(items[0].label, "src/");
                assert!(items[0].highlight);
                assert_eq!(items[1].description, "notes");
                assert!(!items[1].highlight);
                assert!(!items[2].highlight, "a class this reader does not draw has no effect");
                assert_eq!(items[2].label, "b.txt");
            }
            other => panic!("{other:?}"),
        }
        // An `icon(` that never closes, and a word that is no annotation at all.
        let err = parse("treeView\n    src/ icon(folder\n").unwrap_err();
        assert!(err.message.contains("icon("), "{err}");
        assert_eq!(err.line, 2);
        let err = parse("treeView\n    src/\n        index.js oops\n").unwrap_err();
        assert!(err.message.contains("not part of"), "{err}");
        assert_eq!(err.line, 3);
        // A quoted name that never closes.
        let err = parse("treeView\n    src/\n        \"closed\n").unwrap_err();
        assert!(err.message.contains("closing"), "{err}");
        assert_eq!(err.line, 3);
        // A row with no words, and a tree with no rows.
        let err = parse("treeView\n    src/\n├── \n").unwrap_err();
        assert!(err.message.contains("no name"), "{err}");
        assert_eq!(err.line, 3);
        assert!(parse("treeView\n").unwrap_err().message.contains("no items"));
    }

    #[test]
    fn a_tree_view_past_its_ceiling_is_refused_rather_than_drawn() {
        let mut source = String::from("treeView\n");
        for i in 0..(MAX_ELEMENTS + 5) {
            source.push_str(&format!("  n{i}\n"));
        }
        let err = parse(&source).unwrap_err();
        assert!(err.message.contains("more than"), "{err}");
    }

    #[test]
    fn the_refusal_names_the_tree_view_among_the_diagrams_it_reads() {
        let err = parse("gantt\n title A\n").unwrap_err();
        assert!(err.message.contains("tree view"), "{err}");
    }

    #[test]
    fn front_matter_is_dropped_before_the_header_is_read() {
        let d = graph("---\ntitle: Where\n---\nflowchart TD\n a --> b\n");
        assert_eq!(d.kind(), DiagramKind::Flow);
    }

    #[test]
    fn a_direction_that_is_not_a_direction_is_refused() {
        let err = parse("flowchart TD\n direction sideways\n").unwrap_err();
        assert!(err.message.contains("not a direction"), "{err}");
    }

    #[test]
    fn a_very_long_label_is_shortened_rather_than_refused() {
        let long = "x".repeat(MAX_LABEL + 50);
        let d = graph(&format!("graph TD\n a[{long}]\n"));
        match d {
            Diagram::Graph { nodes, .. } => assert!(nodes[0].label.chars().count() <= MAX_LABEL),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn source_past_the_ceiling_is_refused_before_it_is_scanned() {
        let huge = "a".repeat(MAX_SOURCE + 1);
        let err = parse(&huge).unwrap_err();
        assert!(err.message.contains("ceiling"), "{err}");
    }

    #[test]
    fn no_input_can_make_the_parser_panic() {
        // Every prefix of a messy document, read as every diagram kind: a panic here
        // would be a panic in the reader.
        let messy = "flowchart TD\n a[x] -->|y| b{?}\n subgraph s\n c((d))\n end\n \
                     sequenceDiagram\n A->>B: hi\n loop x\n A-->>B\n end\n pie\n \"a\" : 1\n \
                     mindmap\n root((r))\n  x\n erDiagram\n A ||--o{ B : c\n \
                     treeView\n src/\n  index.js ## the entry\n";
        for at in 0..messy.len() {
            let slice = &messy[..at];
            let _ = parse(slice);
        }
        // The tree's other form is written in box-drawing characters, which are
        // multi-byte: this one walks the character boundaries instead.
        let boxed = "treeView-beta\n\tsrc/\n\t\tindex.js\n┣━━ util.js\n┃   ┗━━ \"open\n";
        for (at, _) in boxed.char_indices() {
            let _ = parse(&boxed[..at]);
        }
        for bad in ["\u{feff}graph", "graph\n", "%%graph TD", "graph TD\n###", "graph TD\n[]"] {
            let _ = parse(bad);
        }
    }
}
