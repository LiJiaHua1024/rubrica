//! A [`Diagram`] and a text measurement into a [`Scene`]: boxes, edges, arrows,
//! slices, all positioned and none of them painted.
//!
//! The scene is deliberately dumb. It holds closed [`ShapePath`]s that may be filled
//! and stroked, open ones that are padded along their length, and [`Label`]s that
//! name their [`Ink`] by role. Which face draws a label, and what colour an ink is,
//! are the caller's business -- a theme is a property of the reader, not of the
//! diagram -- and that is what lets the same scene come out on a light page and a
//! dark one, in a window and in an exported PDF, without the crate knowing either.
//!
//! Everything is measured rather than assumed. The caller hands in a closure that
//! measures a line of text, so a test can pass a fixed-width mock and assert the
//! arithmetic, and the reader passes DirectWrite and gets the real advances: the box
//! a node reserves and the ink drawn inside it are then the same width by
//! construction.

use crate::parse::{Diagram, DiagramKind, Direction, Head, LineStyle, Node, NodeShape, TreeItem};

/// Measure one line of text: its advance, its ascent, and its descent below the
/// baseline, in points.
///
/// `role` is the part of the diagram the text belongs to, so a caller may set an edge
/// label or a slice's name in a different face from the words in a box, and `italic`
/// is whether the line asks for an italic face -- a tree view's `##` description does.
/// Both are passed to every measurement, so the face a label is measured in is the
/// face it is drawn in.
pub type DiagramMeasure<'a> = &'a mut dyn FnMut(&str, f32, TextRole, bool) -> TextMetrics;

/// Which part of a diagram a label belongs to. The size is the layout's own advice in
/// points, and the role is what the caller may choose a face by.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextRole {
    /// The words inside a node, a participant, an entity or a class.
    Node,
    /// The name on a link, a message or a transition.
    Edge,
    /// A subgraph's, a block's or a pie's heading.
    Title,
    /// A participant's own name over its lifeline.
    Section,
}

/// What one measured line of text took.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TextMetrics {
    pub advance: f32,
    /// Height above the baseline.
    pub ascent: f32,
    /// Depth below it.
    pub descent: f32,
}

impl TextMetrics {
    /// Height of one line at these metrics.
    pub fn height(&self) -> f32 {
        self.ascent + self.descent
    }
}

/// The ink roles a diagram is drawn in. Roles rather than colours, because a page has
/// a light answer and a dark one to each of them, and the reader owns that answer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Ink {
    /// The outline of a shape.
    Stroke,
    /// A shape that is filled rather than outlined.
    Fill,
    /// Words.
    Text,
    /// The arrow head at the end of an edge.
    Arrow,
    /// Words that must be legible over something else: an edge's label sitting on its
    /// line, a pie's legend, a slice's name on its own slice.
    Backdrop,
    /// Words the reader's theme wants to call attention to: a `:::highlight` row of a
    /// tree view.
    Accent,
    /// Words that stand beside the drawing rather than in it: a tree view's `##`
    /// description, the words a diagram is annotated with rather than labelled by.
    Muted,
    /// One slice of a pie, by its index.
    Slice(u8),
}

/// How wide a shape's outline is drawn, as a multiple of the diagram's stroke width.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stroke {
    None,
    Thin,
    Normal,
    Heavy,
}

impl Stroke {
    pub fn width(self, width: f32) -> f32 {
        match self {
            Stroke::None => 0.0,
            Stroke::Thin => width * 0.7,
            Stroke::Normal => width,
            Stroke::Heavy => width * 1.4,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

impl Point {
    pub fn new(x: f32, y: f32) -> Point {
        Point { x, y }
    }
}

/// One step of a path.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PathCmd {
    MoveTo(Point),
    LineTo(Point),
    /// The two control points and the end point.
    CubicTo(Point, Point, Point),
}

/// A path, either closed (a shape, which may be filled) or open (a line).
#[derive(Clone, Debug, PartialEq)]
pub struct ShapePath {
    pub start: Point,
    pub closed: bool,
    pub cmds: Vec<PathCmd>,
}

impl ShapePath {
    pub fn open(start: Point) -> ShapePath {
        ShapePath { start, closed: false, cmds: Vec::new() }
    }

    pub fn closed(start: Point) -> ShapePath {
        ShapePath { start, closed: true, cmds: Vec::new() }
    }

    pub fn line_to(mut self, p: Point) -> ShapePath {
        self.cmds.push(PathCmd::LineTo(p));
        self
    }

    pub fn curve_to(mut self, c1: Point, c2: Point, p: Point) -> ShapePath {
        self.cmds.push(PathCmd::CubicTo(c1, c2, p));
        self
    }

    /// A straight line from one point to another, which is the whole of most edges.
    pub fn line(from: Point, to: Point) -> ShapePath {
        ShapePath::open(from).line_to(to)
    }

    /// Start another figure inside the same path. Only the first figure is closed by
    /// `closed`; the ones a `MoveTo` opens are the inner lines a subroutine box has.
    pub fn move_to(mut self, p: Point) -> ShapePath {
        self.cmds.push(PathCmd::MoveTo(p));
        self
    }

    /// Every point the path names, ends included. Used for the bounding box, which
    /// takes control points as the curve's own extent -- the curve is inside their
    /// hull, so the box is never smaller than the ink.
    pub fn points(&self) -> impl Iterator<Item = Point> + '_ {
        let mut out = vec![self.start];
        for cmd in &self.cmds {
            match cmd {
                PathCmd::MoveTo(p) | PathCmd::LineTo(p) => out.push(*p),
                PathCmd::CubicTo(a, b, c) => {
                    out.push(*a);
                    out.push(*b);
                    out.push(*c);
                }
            }
        }
        out.into_iter()
    }

    fn translate(&mut self, dx: f32, dy: f32) {
        shift(&mut self.start, dx, dy);
        for cmd in &mut self.cmds {
            match cmd {
                PathCmd::MoveTo(p) | PathCmd::LineTo(p) => shift(p, dx, dy),
                PathCmd::CubicTo(a, b, c) => {
                    shift(a, dx, dy);
                    shift(b, dx, dy);
                    shift(c, dx, dy);
                }
            }
        }
    }
}

/// One thing to draw, in the scene's own coordinates.
#[derive(Clone, Debug, PartialEq)]
pub struct Shape {
    pub path: ShapePath,
    pub fill: Option<Ink>,
    pub stroke: Stroke,
    pub ink: Ink,
}

impl Shape {
    fn points(&self) -> impl Iterator<Item = Point> + '_ {
        self.path.points()
    }

    fn translate(&mut self, dx: f32, dy: f32) {
        self.path.translate(dx, dy);
    }
}

/// Where a label's lines sit inside their box.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Alignment {
    Left,
    Center,
    Right,
}

/// One line of words, with the box it belongs in, its baseline, and its ink.
#[derive(Clone, Debug, PartialEq)]
pub struct Label {
    pub text: String,
    /// The baseline's y.
    pub baseline: f32,
    /// Where the line starts and ends, for a caller that wants to hit-test it.
    pub left: f32,
    pub right: f32,
    pub size: f32,
    pub align: Alignment,
    pub ink: Ink,
    pub role: TextRole,
    /// The weight the layout recommends, so a heading of a diagram reads as one.
    pub weight: u16,
    /// Whether the words ask for an italic face -- a tree view's `##` description,
    /// which Mermaid sets in italic beside the label.
    pub italic: bool,
}

/// What a drawing of the diagram is made of, ready to be laid out on a page.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Scene {
    pub width: f32,
    pub height: f32,
    /// Painted in order: panels first, then shapes, then edges, then labels.
    pub shapes: Vec<Shape>,
    pub labels: Vec<Label>,
    /// How many nodes, participants or slices the diagram declares.
    pub elements: usize,
    /// How many links, messages or transitions it declares.
    pub edges: usize,
    /// The names of the diagram kinds that were drawn, for the report.
    pub kind: &'static str,
}

impl Scene {
    /// How many drawings the scene carries: the number `--report` prints as the
    /// diagram's work.
    pub fn primitives(&self) -> usize {
        self.shapes.len() + self.labels.len()
    }

    pub fn is_empty(&self) -> bool {
        self.shapes.is_empty() && self.labels.is_empty()
    }

    /// The widest and lowest ink in the scene, which is what a caller scales a
    /// diagram to fit its measure by.
    pub fn ink_box(&self) -> (f32, f32) {
        (self.width, self.height)
    }

    /// Re-scale the whole scene. Shrinking a diagram to fit the measure shrinks its
    /// type too -- there is nowhere else for it to go -- and the sizes travel with it
    /// so a word drawn from this scene is set at the size the layout asked for,
    /// multiplied by the fit.
    pub fn scale(&mut self, factor: f32) {
        if !(factor.is_finite() && factor > 0.0) || (factor - 1.0).abs() < 1e-6 {
            return;
        }
        for shape in &mut self.shapes {
            scale_path(&mut shape.path, factor);
        }
        for label in &mut self.labels {
            label.baseline *= factor;
            label.left *= factor;
            label.right *= factor;
            label.size *= factor;
        }
        self.width *= factor;
        self.height *= factor;
    }
}

fn scale_path(path: &mut ShapePath, factor: f32) {
    shift_scale(&mut path.start, factor);
    for cmd in &mut path.cmds {
        match cmd {
            PathCmd::MoveTo(p) | PathCmd::LineTo(p) => shift_scale(p, factor),
            PathCmd::CubicTo(a, b, c) => {
                shift_scale(a, factor);
                shift_scale(b, factor);
                shift_scale(c, factor);
            }
        }
    }
}

fn shift_scale(point: &mut Point, factor: f32) {
    point.x *= factor;
    point.y *= factor;
}

fn shift(point: &mut Point, dx: f32, dy: f32) {
    point.x += dx;
    point.y += dy;
}

/// The most lines one label may be drawn as.
pub const MAX_LABEL_LINES: usize = 32;
/// The most characters one label may be drawn with.
pub const MAX_DIAGRAM_TEXT: usize = 2000;

/// Space between two nodes of the same rank, in ems of the body size.
const SIBLING_GAP: f32 = 1.4;
/// Space between two ranks, before the text of either is counted.
const RANK_GAP: f32 = 2.2;
/// How much room a node keeps around its words, in ems.
const PAD_X: f32 = 0.9;
const PAD_Y: f32 = 0.7;
/// The shortest an empty shape may be, in ems.
const MIN_SPAN: f32 = 1.6;
/// An arrow head's length and half-width, in ems.
const HEAD_LEN: f32 = 0.9;
const HEAD_HALF: f32 = 0.34;
/// Panel padding, in ems.
const GROUP_PAD: f32 = 0.9;

/// Read `diagram` at `size` points of body text and hand back what to draw.
pub fn layout(diagram: &Diagram, size: f32, m: DiagramMeasure<'_>) -> Scene {
    let size = size.max(1.0);
    match diagram {
        Diagram::Graph { kind, direction, nodes, edges, groups, title } => {
            let mut model = Model::build(nodes, edges, groups);
            model.measure(size, m);
            model.size(size);
            model.tree = *kind == DiagramKind::Mindmap;
            match kind {
                DiagramKind::Mindmap => graph_scene(&mut model, *direction, size, m, title, "mindmap"),
                DiagramKind::State => graph_scene(&mut model, *direction, size, m, title, "state"),
                DiagramKind::Class => graph_scene(&mut model, *direction, size, m, title, "class"),
                DiagramKind::Er => graph_scene(&mut model, *direction, size, m, title, "er"),
                _ => graph_scene(&mut model, *direction, size, m, title, "flowchart"),
            }
        }
        Diagram::Sequence { participants, messages, notes, fragments, title } => {
            sequence(participants, messages, notes, fragments, size, m, title)
        }
        Diagram::Pie { title, show_data, slices } => pie(title, *show_data, slices, size, m),
        Diagram::Tree { items, title } => tree_view(items, size, m, title),
    }
}

// ---------------------------------------------------------------------------
// Measuring text
// ---------------------------------------------------------------------------

/// The lines a label is drawn as: a label's own newlines, which is how an author
/// breaks a member list over several lines.
fn lines_of(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut total = 0usize;
    for line in text.split('\n') {
        if out.len() >= MAX_LABEL_LINES || total >= MAX_DIAGRAM_TEXT {
            break;
        }
        total += line.chars().count();
        out.push(line.trim_end().to_string());
    }
    if out.is_empty() {
        out.push(String::new());
    }
    out
}

/// A label's lines, its widest line, and the height of the block they make.
#[derive(Clone)]
struct Measured {
    lines: Vec<String>,
    widths: Vec<f32>,
    line_h: f32,
    ascent: f32,
    descent: f32,
}

impl Measured {
    fn width(&self) -> f32 {
        self.widths.iter().cloned().fold(0.0f32, f32::max)
    }

    fn height(&self) -> f32 {
        self.line_h * self.lines.len() as f32
    }

    /// The distance from the top of the block down to the first baseline, which
    /// centres the block's x-height in the box rather than its ink.
    fn first_baseline(&self) -> f32 {
        (self.line_h + self.ascent - self.descent) * 0.5
    }
}

fn measure_label(text: &str, size: f32, role: TextRole, italic: bool, m: DiagramMeasure<'_>) -> Measured {
    let lines = lines_of(text);
    let mut widths = Vec::with_capacity(lines.len());
    let mut ascent = 0.0f32;
    let mut descent = 0.0f32;
    for line in &lines {
        // An empty line still takes a line's worth of height, so a blank line between
        // two members is the blank line the author wrote.
        let measured = if line.is_empty() { m("Mg", size, role, italic) } else { m(line, size, role, italic) };
        widths.push(if line.is_empty() { 0.0 } else { measured.advance });
        ascent = ascent.max(measured.ascent);
        descent = descent.max(measured.descent);
    }
    let line_h = (ascent + descent).max(size * 0.8);
    Measured { lines, widths, line_h, ascent, descent }
}

/// Lay a measured label out with its text block vertically centred in a box whose top
/// is `top` and whose height is `height`.
#[allow(clippy::too_many_arguments)]
fn place_label(
    out: &mut Vec<Label>,
    measured: &Measured,
    left: f32,
    width: f32,
    top: f32,
    height: f32,
    size: f32,
    align: Alignment,
    ink: Ink,
    role: TextRole,
    weight: u16,
    italic: bool,
) {
    let block = measured.height();
    let baseline0 = top + (height - block) * 0.5 + measured.first_baseline();
    for (i, line) in measured.lines.iter().enumerate() {
        if line.is_empty() {
            continue;
        }
        let at = match align {
            Alignment::Left => left,
            Alignment::Center => left + (width - measured.widths[i]) * 0.5,
            Alignment::Right => left + width - measured.widths[i],
        };
        out.push(Label {
            text: line.clone(),
            baseline: baseline0 + measured.line_h * i as f32,
            left: at,
            right: at + measured.widths[i],
            size,
            align,
            ink,
            role,
            weight,
            italic,
        });
    }
}

// ---------------------------------------------------------------------------
// The graph model every graph-shaped diagram shares
// ---------------------------------------------------------------------------

/// One node with everything the layout needs of it: its words, the box they need, and
/// where it ended up.
struct Item {
    node: Node,
    measured: Measured,
    left: f32,
    top: f32,
    width: f32,
    height: f32,
    /// Which rank (layer) the layout put it in.
    rank: usize,
}

impl Item {
    fn rect(&self) -> Rect {
        Rect { left: self.left, top: self.top, right: self.left + self.width, bottom: self.top + self.height }
    }
}

struct Link {
    from: usize,
    to: usize,
    label: String,
    style: LineStyle,
    start: Head,
    end: Head,
}

/// A diagram's nodes, edges and panels, before anything is placed.
struct Model {
    items: Vec<Item>,
    links: Vec<Link>,
    groups: Vec<(String, Vec<usize>)>,
    /// Whether the diagram's own shape is a tree (a mind-map).
    tree: bool,
}

impl Model {
    fn build(nodes: &[Node], edges: &[crate::parse::Edge], groups: &[crate::parse::Group]) -> Model {
        let empty = Measured { lines: Vec::new(), widths: Vec::new(), line_h: 0.0, ascent: 0.0, descent: 0.0 };
        let items = nodes
            .iter()
            .map(|n| Item { node: n.clone(), measured: empty.clone(), left: 0.0, top: 0.0, width: 0.0, height: 0.0, rank: 0 })
            .collect();
        let mut model = Model { items, links: Vec::new(), groups: Vec::new(), tree: false };
        for edge in edges {
            let (Some(from), Some(to)) = (model.find(&edge.from), model.find(&edge.to)) else { continue };
            model.links.push(Link {
                from,
                to,
                label: edge.label.clone(),
                style: edge.style,
                start: edge.start,
                end: edge.end,
            });
        }
        for g in groups {
            let members: Vec<usize> = g.members.iter().filter_map(|m| model.find(m)).collect();
            if !members.is_empty() {
                model.groups.push((g.label.clone(), members));
            }
        }
        model
    }

    fn find(&self, id: &str) -> Option<usize> {
        self.items.iter().position(|i| i.node.id == id)
    }

    fn measure(&mut self, size: f32, m: DiagramMeasure<'_>) {
        for item in &mut self.items {
            item.measured = measure_label(item.node.text(), size, TextRole::Node, false, m);
        }
    }

    /// The box each node's outline needs, from the words in it and the shape around it.
    fn size(&mut self, size: f32) {
        for item in &mut self.items {
            let shape = item.node.shape;
            let slant = match shape {
                NodeShape::Rhombus => 1.9,
                NodeShape::Hexagon => 1.5,
                NodeShape::Parallelogram | NodeShape::Trapezoid | NodeShape::Asymmetric => 1.4,
                _ => 1.0,
            };
            let mut w = item.measured.width() * slant + size * PAD_X * 2.0;
            let mut h = item.measured.height() + size * PAD_Y * 2.0;
            match shape {
                NodeShape::Circle | NodeShape::DoubleCircle => {
                    let d = w.max(h);
                    w = d;
                    h = d;
                }
                NodeShape::Start | NodeShape::Finish => {
                    w = size * 1.6;
                    h = size * 1.6;
                }
                _ => {}
            }
            item.width = w.max(size * MIN_SPAN);
            item.height = h.max(size * MIN_SPAN);
        }
    }
}

// ---------------------------------------------------------------------------
// The layered layout
// ---------------------------------------------------------------------------

/// Rank the graph, order it to keep the crossings down, place it, route its edges, and
/// draw the whole thing.
fn graph_scene(model: &mut Model, direction: Direction, size: f32, m: DiagramMeasure<'_>, title: &str, kind: &'static str) -> Scene {
    let mut scene = Scene { kind, ..Scene::default() };
    // The flow's own coordinates: `along` runs down the ranks, `across` sideways, and
    // which of the page's axes those are is the diagram's direction.
    let ranks = rank(model);
    for (r, rank) in ranks.iter().enumerate() {
        for &node in rank {
            model.items[node].rank = r;
        }
    }
    let ranks = order(model, &ranks);
    // Every edge name is measured once, here, and then used by the room the gaps
    // reserve, the panel settling and the placing below.
    let edge_text: Vec<Option<Measured>> = model
        .links
        .iter()
        .map(|link| {
            if link.label.is_empty() {
                None
            } else {
                Some(measure_label(&link.label, size * 0.85, TextRole::Edge, false, m))
            }
        })
        .collect();
    let room = edge_room(model, &ranks, &edge_text, size);
    let (along, across) = place(model, &ranks, size, &room);
    let mut centers: Vec<Point> = (0..model.items.len()).map(|i| to_page(along[i], across[i], direction)).collect();
    // A `BT` or `RL` diagram is the same picture read the other way: its first rank
    // stands at the bottom, or at the right, and the arrows follow the boxes they
    // actually connect.
    if matches!(direction, Direction::BottomTop | Direction::RightLeft) {
        let vertical = matches!(direction, Direction::BottomTop);
        let (mut lo, mut hi) = (f32::INFINITY, f32::NEG_INFINITY);
        for (i, item) in model.items.iter().enumerate() {
            let half = if vertical { item.height * 0.5 } else { item.width * 0.5 };
            let at = if vertical { centers[i].y } else { centers[i].x };
            lo = lo.min(at - half);
            hi = hi.max(at + half);
        }
        let mirror = lo + hi;
        for center in centers.iter_mut() {
            if vertical {
                center.y = mirror - center.y;
            } else {
                center.x = mirror - center.x;
            }
        }
    }
    for (i, item) in model.items.iter_mut().enumerate() {
        item.left = centers[i].x - item.width * 0.5;
        item.top = centers[i].y - item.height * 0.5;
    }
    // The panels settle once the boxes are on the page: padding is only worth having
    // if it stays off everything that is not a member, which may move boxes and whole
    // groups before the edges are routed.
    let vertical = matches!(direction, Direction::TopBottom | Direction::BottomTop);
    let panels = settle_panels(model, &ranks, vertical, size, m, &edge_text);
    // Panels first: they sit behind everything they contain.
    for panel in &panels {
        let Some((outer, text)) = panel else { continue };
        let pad = size * GROUP_PAD;
        let heading = text.height().max(size);
        scene.shapes.push(Shape {
            path: rounded_rect(*outer, size * 0.6),
            fill: None,
            stroke: Stroke::Thin,
            ink: Ink::Stroke,
        });
        place_label(
            &mut scene.labels,
            text,
            outer.left + pad * 0.6,
            outer.right - outer.left - pad,
            outer.top,
            heading,
            size * 0.9,
            Alignment::Left,
            Ink::Text,
            TextRole::Title,
            600,
            false,
        );
    }
    // The nodes.
    for item in &model.items {
        let (path, fill) = shape_path(item.node.shape, item.rect(), size);
        scene.shapes.push(Shape { path, fill, stroke: Stroke::Normal, ink: Ink::Stroke });
        // A start or end disc carries no words: it is a mark, not a box.
        if !matches!(item.node.shape, NodeShape::Start | NodeShape::Finish) {
            place_label(
                &mut scene.labels,
                &item.measured,
                item.left + size * PAD_X,
                item.width - size * PAD_X * 2.0,
                item.top,
                item.height,
                size,
                Alignment::Center,
                Ink::Text,
                TextRole::Node,
                400,
                false,
            );
        }
    }
    // Then the edges, longest first, so a short one keeps its pick of the channels.
    let mut order: Vec<usize> = (0..model.links.len()).collect();
    order.sort_by_key(|i| {
        let link = &model.links[*i];
        std::cmp::Reverse(model.items[link.from].rank.abs_diff(model.items[link.to].rank))
    });
    let mut runs: Vec<Rect> = Vec::new();
    // The strip between each pair of ranks, and the slot each edge name there takes:
    // two names in one gap stack instead of landing on each other.
    let bands: Vec<(f32, f32)> = (0..ranks.len().saturating_sub(1)).map(|b| band_of(model, &ranks, b, vertical)).collect();
    let mut tallest = vec![0.0f32; bands.len()];
    let mut total = vec![0usize; bands.len()];
    for (link, text) in model.links.iter().zip(&edge_text) {
        let Some(text) = text else { continue };
        if link.from == link.to {
            continue;
        }
        let (a, b) = (model.items[link.from].rank, model.items[link.to].rank);
        if a.abs_diff(b) != 1 {
            continue;
        }
        let boundary = a.min(b);
        tallest[boundary] = tallest[boundary].max(text.height());
        total[boundary] += 1;
    }
    let mut slot = vec![0usize; bands.len()];
    for index in order {
        let route = {
            let link = &model.links[index];
            let (from, to) = (model.items[link.from].rect(), model.items[link.to].rect());
            route(link, from, to, direction, size, &mut runs)
        };
        let link = &model.links[index];
        if link.style != LineStyle::Invisible {
            let stroke = match link.style {
                LineStyle::Thick => Stroke::Heavy,
                _ => Stroke::Normal,
            };
            scene.shapes.push(Shape { path: route.path, fill: None, stroke, ink: Ink::Stroke });
            for (tip, dir, head) in route.heads {
                scene.shapes.extend(head_shapes(head, tip, dir, size));
            }
        }
        if let Some(text) = &edge_text[index] {
            let at = route.label.unwrap_or(Point::new(0.0, 0.0));
            let (w, h) = (text.width(), text.height());
            let (a, b) = (model.items[link.from].rank, model.items[link.to].rank);
            let center = if link.from != link.to && a.abs_diff(b) == 1 {
                let boundary = a.min(b);
                let mine = slot[boundary];
                slot[boundary] += 1;
                let (lo, hi) = bands[boundary];
                let spacing = (hi - lo - tallest[boundary]) / total[boundary] as f32;
                let sit = lo + tallest[boundary] * 0.5 + (mine as f32 + 0.5) * spacing;
                if vertical {
                    Point::new(at.x, sit)
                } else {
                    Point::new(sit, at.y)
                }
            } else {
                at
            };
            let left = center.x - w * 0.5;
            let top = center.y - h * 0.5;
            let pad = size * 0.3;
            scene.shapes.push(Shape {
                path: rect_path(Rect { left: left - pad, top: top - pad * 0.3, right: left + w + pad, bottom: top + h + pad * 0.3 }),
                fill: Some(Ink::Backdrop),
                stroke: Stroke::None,
                ink: Ink::Backdrop,
            });
            place_label(&mut scene.labels, text, left, w, top, h, size * 0.85, Alignment::Center, Ink::Text, TextRole::Edge, 400, false);
        }
    }
    if !title.is_empty() {
        let text = measure_label(title, size * 1.05, TextRole::Title, false, m);
        place_label(&mut scene.labels, &text, 0.0, text.width(), -text.height() - size, text.height(), size * 1.05, Alignment::Left, Ink::Text, TextRole::Title, 700, false);
    }
    scene.elements = model.items.len();
    scene.edges = model.links.len();
    finish(scene, size)
}

/// Which of the page's axes a flow's rank axis lands on.
fn to_page(along: f32, across: f32, direction: Direction) -> Point {
    match direction {
        Direction::TopBottom | Direction::BottomTop => Point::new(across, along),
        Direction::LeftRight | Direction::RightLeft => Point::new(along, across),
    }
}

/// The ranks of the graph: the longest path from a root to every node.
fn rank(model: &Model) -> Vec<Vec<usize>> {
    let n = model.items.len();
    let mut depth = vec![0usize; n];
    if model.tree {
        // A mind-map's shape is its indentation, which the parser already turned into
        // parent links: the depth is the branch length. Every branch has one parent,
        // so a walk from the roots settles every depth exactly.
        let mut walk: Vec<usize> = (0..n).filter(|i| !model.links.iter().any(|l| l.to == *i)).collect();
        if walk.is_empty() && n > 0 {
            walk.push(0);
        }
        let mut head = 0;
        while head < walk.len() && head <= 4 * n {
            let at = walk[head];
            head += 1;
            for link in &model.links {
                if link.from == at && link.to != at {
                    depth[link.to] = depth[link.to].max(depth[at] + 1);
                    walk.push(link.to);
                }
            }
        }
    } else {
        // Longest path from a root, by a queue that starts with every node nothing
        // points at and restarts on whatever a cycle left unreached.
        let mut incoming = vec![0usize; n];
        let mut outgoing: Vec<Vec<usize>> = vec![Vec::new(); n];
        for link in &model.links {
            if link.from == link.to {
                continue;
            }
            outgoing[link.from].push(link.to);
            incoming[link.to] += 1;
        }
        let mut queue: Vec<usize> = (0..n).filter(|i| incoming[*i] == 0).collect();
        let mut seen = vec![false; n];
        let mut head = 0;
        let mut next_root = 0;
        loop {
            if head == queue.len() {
                while next_root < n && seen[next_root] {
                    next_root += 1;
                }
                if next_root == n {
                    break;
                }
                queue.push(next_root);
            }
            let at = queue[head];
            head += 1;
            if seen[at] {
                continue;
            }
            seen[at] = true;
            for &next in &outgoing[at] {
                if depth[next] < depth[at] + 1 {
                    depth[next] = depth[at] + 1;
                }
                incoming[next] = incoming[next].saturating_sub(1);
                if incoming[next] == 0 {
                    queue.push(next);
                }
            }
        }
    }
    let mut ranks: Vec<Vec<usize>> = Vec::new();
    for (i, &d) in depth.iter().enumerate() {
        while ranks.len() <= d {
            ranks.push(Vec::new());
        }
        ranks[d].push(i);
    }
    ranks.retain(|r| !r.is_empty());
    ranks
}

/// The order the nodes of each rank are drawn in: a few barycenter passes, which is
/// the cheapest thing that keeps two long edges from crossing when they need not.
fn order(model: &Model, ranks: &[Vec<usize>]) -> Vec<Vec<usize>> {
    let mut ranks: Vec<Vec<usize>> = ranks.to_vec();
    let mut position = vec![0usize; model.items.len()];
    refresh(&ranks, &mut position);
    for pass in 0..4 {
        let down = pass % 2 == 0;
        let range: Vec<usize> = if down {
            (1..ranks.len()).collect()
        } else {
            (0..ranks.len().saturating_sub(1)).rev().collect()
        };
        for r in range {
            let mut keyed: Vec<(f32, usize)> = ranks[r]
                .iter()
                .map(|&node| (barycenter(model, node, down, &position), node))
                .collect();
            keyed.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
            ranks[r] = keyed.into_iter().map(|(_, node)| node).collect();
        }
        refresh(&ranks, &mut position);
    }
    ranks
}

/// Where each node stands in its rank, which is what a barycenter pass reads.
fn refresh(ranks: &[Vec<usize>], position: &mut [usize]) {
    for rank in ranks {
        for (i, node) in rank.iter().enumerate() {
            position[*node] = i;
        }
    }
}

/// The average position, in the neighbour rank, of the nodes this one is linked to.
fn barycenter(model: &Model, node: usize, down: bool, position: &[usize]) -> f32 {
    let mut sum = 0.0;
    let mut count = 0.0;
    for link in &model.links {
        let other = if down && link.to == node && model.items[link.from].rank < model.items[link.to].rank {
            Some(link.from)
        } else if !down && link.from == node && model.items[link.to].rank > model.items[link.from].rank {
            Some(link.to)
        } else {
            None
        };
        if let Some(other) = other {
            sum += position[other] as f32;
            count += 1.0;
        }
    }
    if count > 0.0 {
        sum / count
    } else {
        position[node] as f32
    }
}

/// Where the nodes are: `along` is the distance down the ranks, `across` the distance
/// sideways. Both in points from the flow's own origin.
/// The extra room each rank boundary owes the edge names that cross it: one slot per
/// name, so two names in the same gap stack instead of landing on each other.
fn edge_room(model: &Model, ranks: &[Vec<usize>], edge_text: &[Option<Measured>], size: f32) -> Vec<f32> {
    let boundaries = ranks.len().saturating_sub(1);
    let mut slots = vec![0usize; boundaries];
    let mut tallest = vec![0.0f32; boundaries];
    for (link, text) in model.links.iter().zip(edge_text) {
        let Some(text) = text else { continue };
        let (a, b) = (model.items[link.from].rank, model.items[link.to].rank);
        if a.abs_diff(b) != 1 {
            continue;
        }
        let boundary = a.min(b);
        slots[boundary] += 1;
        tallest[boundary] = tallest[boundary].max(text.height());
    }
    (0..boundaries)
        .map(|b| if slots[b] >= 2 { (slots[b] - 1) as f32 * (tallest[b] + size * 0.6) } else { 0.0 })
        .collect()
}

/// Lay the ranks out along the flow and the boxes out across it. `room` carries one
/// extra gap per rank boundary, in points, for the edge names that have to stack in
/// that gap rather than sit on top of each other.
fn place(model: &mut Model, ranks: &[Vec<usize>], size: f32, room: &[f32]) -> (Vec<f32>, Vec<f32>) {
    let n = model.items.len();
    let mut along = vec![0.0f32; n];
    let mut across = vec![0.0f32; n];
    if ranks.is_empty() {
        return (along, across);
    }
    // Across first: every rank is centred in the widest one, which is what keeps a
    // two-node tail under a four-node head.
    let spans: Vec<f32> = ranks
        .iter()
        .map(|rank| {
            let mut total = 0.0;
            for (i, &node) in rank.iter().enumerate() {
                if i > 0 {
                    total += size * SIBLING_GAP;
                }
                total += model.items[node].width;
            }
            total
        })
        .collect();
    for (r, rank) in ranks.iter().enumerate() {
        let mut at = -spans[r] * 0.5;
        for &node in rank {
            across[node] = at + model.items[node].width * 0.5;
            at += model.items[node].width + size * SIBLING_GAP;
        }
    }
    separate(model, ranks, &mut across, size);
    // Then down: each rank stands below the tallest thing in the rank before it, and
    // is pushed further down while any node is not yet clear of the one before.
    let mut tops: Vec<f32> = vec![0.0; ranks.len()];
    for r in 1..ranks.len() {
        let above = ranks[r - 1]
            .iter()
            .map(|n| tops[r - 1] + model.items[*n].height)
            .fold(f32::NEG_INFINITY, f32::max);
        tops[r] = above + size * RANK_GAP + room[r - 1];
    }
    // A node may be reached by an edge that crosses a rank without landing on it: the
    // ranks it spans have to open up so the edge has a channel to cross in.
    for link in &model.links {
        let a = model.items[link.from].rank;
        let b = model.items[link.to].rank;
        if a.abs_diff(b) <= 1 {
            continue;
        }
        let (from, to) = (a.min(b), a.max(b));
        for r in (from + 1)..to {
            let above = ranks[r - 1]
                .iter()
                .map(|n| tops[r - 1] + model.items[*n].height)
                .fold(f32::NEG_INFINITY, f32::max);
            let wanted = above + size * RANK_GAP * 1.6;
            if tops[r] < wanted {
                tops[r] = wanted;
            }
        }
    }
    // Keep pushing: a rank moved down may now be inside the next one.
    for r in 1..ranks.len() {
        let above = ranks[r - 1]
            .iter()
            .map(|n| tops[r - 1] + model.items[*n].height)
            .fold(f32::NEG_INFINITY, f32::max);
        if tops[r] < above + size * RANK_GAP * 0.8 + room[r - 1] {
            tops[r] = above + size * RANK_GAP * 0.8 + room[r - 1];
        }
        for &node in &ranks[r] {
            along[node] = tops[r] + model.items[node].height * 0.5;
        }
    }
    for &node in &ranks[0] {
        along[node] = tops[0] + model.items[node].height * 0.5;
    }
    (along, across)
}

/// Keep the boxes in one rank off each other's shoulders, in the order they were
/// ordered in, which is the only thing the barycenter pass can promise.
fn separate(model: &Model, ranks: &[Vec<usize>], across: &mut [f32], size: f32) {
    for rank in ranks {
        if rank.len() < 2 {
            continue;
        }
        // Sweep forward, then back, so a box pushed right does not then collide with
        // the one after it.
        for pass in 0..2 {
            let indices: Vec<usize> = if pass == 0 { (0..rank.len()).collect() } else { (0..rank.len()).rev().collect() };
            for w in indices.windows(2) {
                let (a, b) = (rank[w[0]], rank[w[1]]);
                if across[b] < across[a] {
                    continue;
                }
                let gap = across[b] - across[a] - (model.items[a].width + model.items[b].width) * 0.5;
                if gap < size * SIBLING_GAP * 0.5 {
                    let push = (size * SIBLING_GAP * 0.5 - gap) * 0.5;
                    across[a] -= push;
                    across[b] += push;
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Panels, and the room an edge's name needs
// ---------------------------------------------------------------------------

/// The empty strip between two ranks, first edge to far edge on the page: top to
/// bottom for a rank that runs across, left to right for one that runs down.
fn band_of(model: &Model, ranks: &[Vec<usize>], boundary: usize, vertical: bool) -> (f32, f32) {
    let mut lower = f32::NEG_INFINITY;
    let mut upper = f32::INFINITY;
    for &node in &ranks[boundary] {
        let item = &model.items[node];
        lower = lower.max(if vertical { item.top + item.height } else { item.left + item.width });
    }
    for &node in &ranks[boundary + 1] {
        let item = &model.items[node];
        upper = upper.min(if vertical { item.top } else { item.left });
    }
    (lower.min(upper), lower.max(upper))
}

/// The panel a group's members need: their bounding box, the padding all around it and
/// the heading strip above. `None` when no member was placed.
fn group_panel(model: &Model, members: &[usize], pad: f32, heading: f32) -> Option<Rect> {
    let mut lo = Point::new(f32::INFINITY, f32::INFINITY);
    let mut hi = Point::new(f32::NEG_INFINITY, f32::NEG_INFINITY);
    for &member in members {
        let item = &model.items[member];
        lo.x = lo.x.min(item.left);
        lo.y = lo.y.min(item.top);
        hi.x = hi.x.max(item.left + item.width);
        hi.y = hi.y.max(item.top + item.height);
    }
    lo.x.is_finite().then_some(Rect { left: lo.x - pad, top: lo.y - pad - heading, right: hi.x + pad, bottom: hi.y + pad })
}

/// Where an edge's name would sit, for a check: on its line's lane, in the whole strip
/// between its two ranks, so the check holds for whichever slot the name later takes.
fn edge_name_rect(model: &Model, link: &Link, band: (f32, f32), vertical: bool, text: &Measured) -> Rect {
    let from = model.items[link.from].rect();
    let to = model.items[link.to].rect();
    let (w, h) = (text.width(), text.height());
    if vertical {
        let lane = (from.center().x + to.center().x) * 0.5;
        Rect { left: lane - w * 0.5, top: band.0, right: lane + w * 0.5, bottom: band.1 }
    } else {
        let lane = (from.center().y + to.center().y) * 0.5;
        Rect { left: band.0, top: lane - h * 0.5, right: band.1, bottom: lane + h * 0.5 }
    }
}

/// Settle the panels before the edges are routed: padding is only worth having if it
/// runs under nothing but its members. A box beside a panel moves away from it, and a
/// whole group moves away from an edge name that would sit on it -- the group moves,
/// not the name, because the name has to stay on its own line.
fn settle_panels(
    model: &mut Model,
    ranks: &[Vec<usize>],
    vertical: bool,
    size: f32,
    m: DiagramMeasure<'_>,
    edge_text: &[Option<Measured>],
) -> Vec<Option<(Rect, Measured)>> {
    let pad = size * GROUP_PAD;
    let margin = size * 0.5;
    let headings: Vec<Measured> = model
        .groups
        .iter()
        .map(|(label, _)| measure_label(label, size * 0.9, TextRole::Title, false, m))
        .collect();
    let heading_h: Vec<f32> = headings.iter().map(|t| t.height().max(size)).collect();
    let bands: Vec<(f32, f32)> = (0..ranks.len().saturating_sub(1)).map(|b| band_of(model, ranks, b, vertical)).collect();
    let mut panels: Vec<Option<Rect>> = vec![None; model.groups.len()];
    for _ in 0..8 {
        let mut moved = false;
        for (g, (_, members)) in model.groups.iter().enumerate() {
            panels[g] = group_panel(model, members, pad, heading_h[g]);
        }
        for (g, (_, members)) in model.groups.iter().enumerate() {
            let Some(panel) = panels[g] else { continue };
            let mid = (panel.left + panel.right) * 0.5;
            // The boxes the padding runs under. Each is pushed away from the panel
            // together with every box nearer the same edge of its rank: a lone push
            // would be undone by the sweep that keeps siblings apart from each other.
            let mut left = (0.0f32, Vec::<usize>::new());
            let mut right = (0.0f32, Vec::<usize>::new());
            for (i, item) in model.items.iter().enumerate() {
                if members.contains(&i) {
                    continue;
                }
                let r = item.rect();
                if !r.hits(panel) {
                    continue;
                }
                let need = if r.center().x <= mid { r.right + margin - panel.left } else { panel.right - r.left + margin };
                let touched = if r.center().x <= mid { &mut left } else { &mut right };
                touched.0 = touched.0.max(need);
                if !touched.1.contains(&item.rank) {
                    touched.1.push(item.rank);
                }
            }
            for (need, touched, sign) in [(left.0, &left.1, -1.0f32), (right.0, &right.1, 1.0)] {
                if need <= 0.0 {
                    continue;
                }
                moved = true;
                for (i, item) in model.items.iter_mut().enumerate() {
                    if members.contains(&i) || !touched.contains(&item.rank) {
                        continue;
                    }
                    let c = item.left + item.width * 0.5;
                    if (c <= mid) == (sign < 0.0) {
                        item.left += sign * need;
                    }
                }
            }
            // An edge name the padding would sit on: the group gives way instead.
            let mut push = 0.0f32;
            for (i, link) in model.links.iter().enumerate() {
                let Some(text) = &edge_text[i] else { continue };
                if link.from == link.to {
                    continue;
                }
                let (a, b) = (model.items[link.from].rank, model.items[link.to].rank);
                if a.abs_diff(b) != 1 {
                    continue;
                }
                let name = edge_name_rect(model, link, bands[a.min(b)], vertical, text);
                if !name.hits(panel) {
                    continue;
                }
                let need = if (name.left + name.right) * 0.5 <= mid {
                    name.right + margin - panel.left
                } else {
                    -(panel.right - name.left + margin)
                };
                if need.abs() > push.abs() {
                    push = need;
                }
            }
            if push != 0.0 {
                moved = true;
                for &member in members {
                    model.items[member].left += push;
                }
            }
        }
        if !moved {
            break;
        }
        // Put the ranks back in order after the pushes, so a moved box still keeps a
        // sibling's gap beside it.
        let mut across: Vec<f32> = model
            .items
            .iter()
            .map(|item| if vertical { item.left + item.width * 0.5 } else { item.top + item.height * 0.5 })
            .collect();
        separate(model, ranks, &mut across, size);
        for (i, item) in model.items.iter_mut().enumerate() {
            if vertical {
                item.left = across[i] - item.width * 0.5;
            } else {
                item.top = across[i] - item.height * 0.5;
            }
        }
    }
    panels
        .into_iter()
        .zip(headings)
        .map(|(panel, text)| panel.map(|outer| (outer, text)))
        .collect()
}

// ---------------------------------------------------------------------------
// Routing and arrow heads
// ---------------------------------------------------------------------------

/// A routed edge: its line, the heads at its ends, and where its label belongs.
struct Route {
    path: ShapePath,
    /// One entry per head that was asked for: the tip, the direction it points and
    /// which head it is.
    heads: Vec<(Point, Point, Head)>,
    label: Option<Point>,
}

/// The line from one box to another, and where its head and its label go.
fn route(link: &Link, from: Rect, to: Rect, direction: Direction, size: f32, runs: &mut Vec<Rect>) -> Route {
    if link.from == link.to {
        return self_loop(link, from, size);
    }
    let vertical = matches!(direction, Direction::TopBottom | Direction::BottomTop);
    let (a, b) = (from.center(), to.center());
    if vertical {
        let (start, end, dir) = if a.y <= b.y {
            (from.bottom(), to.top(), Point::new(0.0, 1.0))
        } else {
            (from.top(), to.bottom(), Point::new(0.0, -1.0))
        };
        let mid = (start.y + end.y) * 0.5;
        let lane = lane_x(runs, start.x, end.x, start.y.min(end.y), start.y.max(end.y), size);
        let path = ShapePath::open(start)
            .line_to(Point::new(start.x, mid - (mid - start.y) * 0.4))
            .line_to(Point::new(lane, mid))
            .line_to(Point::new(end.x, mid + (end.y - mid) * 0.4))
            .line_to(trim(end, dir, size, link.end));
        runs.push(Rect { left: lane - size, top: start.y.min(end.y), right: lane + size, bottom: start.y.max(end.y) });
        let mut route = hang_heads(path, start, end, dir, size, link);
        if !link.label.is_empty() {
            route.label = Some(Point::new(lane, mid));
        }
        return route;
    }
    let (start, end, dir) = if a.x <= b.x {
        (from.right(), to.left(), Point::new(1.0, 0.0))
    } else {
        (from.left(), to.right(), Point::new(-1.0, 0.0))
    };
    let mid = (start.x + end.x) * 0.5;
    let lane = lane_y(runs, start.y, end.y, start.x.min(end.x), start.x.max(end.x), size);
    let path = ShapePath::open(start)
        .line_to(Point::new(mid, start.y))
        .line_to(Point::new(mid, lane))
        .line_to(Point::new(end.x, lane))
        .line_to(trim(end, dir, size, link.end));
    runs.push(Rect { left: start.x.min(end.x), top: lane - size, right: start.x.max(end.x), bottom: lane + size });
    let mut route = hang_heads(path, start, end, dir, size, link);
    if !link.label.is_empty() {
        route.label = Some(Point::new(mid, lane));
    }
    route
}

/// Hang the heads of `link` on a line that runs from `start` to `end` along `dir`: the
/// end head sits at `end` pointing along the line, and a start head sits at `start`
/// pointing back into the box the line left.
fn hang_heads(mut path: ShapePath, start: Point, end: Point, dir: Point, size: f32, link: &Link) -> Route {
    let mut heads = Vec::new();
    if link.end != Head::None {
        heads.push((end, dir, link.end));
    }
    if link.start != Head::None {
        // The line's own first step, which is the direction the head points away from.
        let first = path.cmds.first().map_or(end, |cmd| match cmd {
            PathCmd::MoveTo(p) | PathCmd::LineTo(p) => *p,
            PathCmd::CubicTo(c1, _, _) => *c1,
        });
        let (vx, vy) = (first.x - start.x, first.y - start.y);
        let len = (vx * vx + vy * vy).sqrt();
        if len > 1e-3 {
            let (fx, fy) = (vx / len, vy / len);
            path.start = trim(start, Point::new(fx, fy), size, link.start);
            heads.push((start, Point::new(-fx, -fy), link.start));
        }
    }
    Route { path, heads, label: None }
}

/// Where a back-edge comes back down: beside its own box, with room for a label.
fn self_loop(link: &Link, from: Rect, size: f32) -> Route {
    let out = from.right();
    let back = Point::new((from.left + from.right) * 0.5, from.top);
    let reach = (from.right - from.left).max(from.bottom - from.top) * 0.4 + size * 1.8;
    let path = ShapePath::open(out)
        .curve_to(Point::new(out.x + reach, out.y), Point::new(out.x + reach, back.y - reach), Point::new(out.x + size * 0.4, back.y - reach))
        .line_to(Point::new(back.x + size * 0.4, back.y - reach))
        .curve_to(
            Point::new(back.x + size * 0.4, back.y - reach * 0.4),
            Point::new(back.x + size * 0.6, back.y - size * 0.2),
            back,
        );
    let mut route = hang_heads(path, out, back, Point::new(0.0, 1.0), size, link);
    if !link.label.is_empty() {
        route.label = Some(Point::new(out.x + reach * 0.55, back.y - reach * 0.55));
    }
    route
}

/// The end of a line, pulled back so a head's tip is where the box is.
fn trim(end: Point, dir: Point, size: f32, head: Head) -> Point {
    let pull = match head {
        Head::None => return end,
        Head::Arrow => size * HEAD_LEN * 0.9,
        _ => size * HEAD_HALF,
    };
    Point::new(end.x - dir.x * pull, end.y - dir.y * pull)
}

/// A closed triangle at `tip`, pointing along `dir`.
fn arrow_head(tip: Point, dir: Point, size: f32) -> ShapePath {
    let half = size * HEAD_HALF;
    let len = size * HEAD_LEN;
    let back = Point::new(tip.x - dir.x * len, tip.y - dir.y * len);
    let across = Point::new(-dir.y * half, dir.x * half);
    ShapePath::closed(back)
        .line_to(Point::new(tip.x - dir.x * len * 0.2, tip.y - dir.y * len * 0.2))
        .line_to(Point::new(back.x + across.x, back.y + across.y))
        .line_to(tip)
        .line_to(Point::new(back.x - across.x, back.y - across.y))
}

/// What one head of an edge is drawn as.
fn head_shapes(head: Head, tip: Point, dir: Point, size: f32) -> Vec<Shape> {
    match head {
        Head::None => Vec::new(),
        Head::Arrow => vec![Shape {
            path: arrow_head(tip, dir, size),
            fill: Some(Ink::Arrow),
            stroke: Stroke::None,
            ink: Ink::Arrow,
        }],
        Head::Circle => vec![Shape {
            path: circle_path(tip, size * HEAD_HALF),
            fill: None,
            stroke: Stroke::Normal,
            ink: Ink::Arrow,
        }],
        Head::Cross => {
            let half = size * HEAD_HALF * 1.4;
            let across = Point::new(-dir.y, dir.x);
            let at = |along: f32, side: f32| Point::new(tip.x + dir.x * along + across.x * side, tip.y + dir.y * along + across.y * side);
            let path = ShapePath::open(at(-half, -half))
                .line_to(at(half, half))
                .move_to(at(-half, half))
                .line_to(at(half, -half));
            vec![Shape { path, fill: None, stroke: Stroke::Normal, ink: Ink::Arrow }]
        }
    }
}

/// A vertical run's x, kept off the runs already placed: a second edge between the
/// same two ranks takes the next lane along rather than drawing on the first.
fn lane_x(runs: &[Rect], from_x: f32, to_x: f32, top: f32, bottom: f32, size: f32) -> f32 {
    let ideal = (from_x + to_x) * 0.5;
    let blocked = |candidate: f32| {
        runs.iter().any(|r| {
            candidate > r.left - 1.0 && candidate < r.right + 1.0 && bottom.min(r.bottom) - top.max(r.top) > -1.0
        })
    };
    lane(blocked, ideal, size)
}

/// The same, for a run that goes sideways.
fn lane_y(runs: &[Rect], from_y: f32, to_y: f32, left: f32, right: f32, size: f32) -> f32 {
    let ideal = (from_y + to_y) * 0.5;
    let blocked = |candidate: f32| {
        runs.iter().any(|r| {
            candidate > r.top - 1.0 && candidate < r.bottom + 1.0 && right.min(r.right) - left.max(r.left) > -1.0
        })
    };
    lane(blocked, ideal, size)
}

/// Step a lane off its ideal until it is clear of what is already there: half a line's
/// width to either side, then a step further, and no more than a diagram can use.
fn lane(blocked: impl Fn(f32) -> bool, ideal: f32, size: f32) -> f32 {
    let step = size.max(4.0) * 0.45;
    for i in 0..24 {
        let k = (i as f32 + 1.0) * 0.5;
        let candidate = if i == 0 { ideal } else { ideal + if i % 2 == 1 { k * step } else { -k * step } };
        if !blocked(candidate) {
            return candidate;
        }
    }
    ideal
}

// ---------------------------------------------------------------------------
// Shapes
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq)]
struct Rect {
    left: f32,
    top: f32,
    right: f32,
    bottom: f32,
}

impl Rect {
    fn center(&self) -> Point {
        Point::new((self.left + self.right) * 0.5, (self.top + self.bottom) * 0.5)
    }

    fn top(&self) -> Point {
        Point::new((self.left + self.right) * 0.5, self.top)
    }

    fn bottom(&self) -> Point {
        Point::new((self.left + self.right) * 0.5, self.bottom)
    }

    fn left(&self) -> Point {
        Point::new(self.left, (self.top + self.bottom) * 0.5)
    }

    fn right(&self) -> Point {
        Point::new(self.right, (self.top + self.bottom) * 0.5)
    }

    /// Whether the two rectangles share any area at all.
    fn hits(&self, other: Rect) -> bool {
        self.left < other.right && other.left < self.right && self.top < other.bottom && other.top < self.bottom
    }
}

/// The fraction of a radius at which a cubic bezier matches a quarter arc.
const KAPPA: f32 = 0.552_284_8;

fn rect_path(rect: Rect) -> ShapePath {
    ShapePath::closed(Point::new(rect.left, rect.top))
        .line_to(Point::new(rect.right, rect.top))
        .line_to(Point::new(rect.right, rect.bottom))
        .line_to(Point::new(rect.left, rect.bottom))
}

fn rounded_rect(rect: Rect, corner: f32) -> ShapePath {
    let r = corner.min((rect.right - rect.left) * 0.5).min((rect.bottom - rect.top) * 0.5).max(0.0);
    if r <= 0.01 {
        return rect_path(rect);
    }
    let k = r * KAPPA;
    ShapePath::closed(Point::new(rect.left + r, rect.top))
        .line_to(Point::new(rect.right - r, rect.top))
        .curve_to(Point::new(rect.right - r + k, rect.top), Point::new(rect.right, rect.top + r - k), Point::new(rect.right, rect.top + r))
        .line_to(Point::new(rect.right, rect.bottom - r))
        .curve_to(Point::new(rect.right, rect.bottom - r + k), Point::new(rect.right - r + k, rect.bottom), Point::new(rect.right - r, rect.bottom))
        .line_to(Point::new(rect.left + r, rect.bottom))
        .curve_to(Point::new(rect.left + r - k, rect.bottom), Point::new(rect.left, rect.bottom - r + k), Point::new(rect.left, rect.bottom - r))
        .line_to(Point::new(rect.left, rect.top + r))
        .curve_to(Point::new(rect.left, rect.top + r - k), Point::new(rect.left + r - k, rect.top), Point::new(rect.left + r, rect.top))
}

fn circle_path(center: Point, radius: f32) -> ShapePath {
    ellipse_path(center, radius, radius)
}

fn ellipse_path(center: Point, rx: f32, ry: f32) -> ShapePath {
    let kx = rx * KAPPA;
    let ky = ry * KAPPA;
    ShapePath::closed(Point::new(center.x, center.y - ry))
        .curve_to(Point::new(center.x + kx, center.y - ry), Point::new(center.x + rx, center.y - ky), Point::new(center.x + rx, center.y))
        .curve_to(Point::new(center.x + rx, center.y + ky), Point::new(center.x + kx, center.y + ry), Point::new(center.x, center.y + ry))
        .curve_to(Point::new(center.x - kx, center.y + ry), Point::new(center.x - rx, center.y + ky), Point::new(center.x - rx, center.y))
        .curve_to(Point::new(center.x - rx, center.y - ky), Point::new(center.x - kx, center.y - ry), Point::new(center.x, center.y - ry))
}

/// One arc of a pie, as a filled shape from the centre.
fn wedge(center: Point, radius: f32, from: f32, to: f32) -> ShapePath {
    let span = to - from;
    let pieces = ((span.abs() / std::f32::consts::FRAC_PI_2).ceil() as usize).max(1);
    let step = span / pieces as f32;
    // The bezier constant for an arc of `step` radians.
    let k = 4.0 / 3.0 * (step / 4.0).tan();
    let mut path = ShapePath::closed(center);
    for i in 0..pieces {
        let a0 = from + step * i as f32;
        let a1 = from + step * (i + 1) as f32;
        let (p0, p1) = (polar(center, radius, a0), polar(center, radius, a1));
        let t0 = Point::new(-(p0.y - center.y), p0.x - center.x);
        let t1 = Point::new(-(p1.y - center.y), p1.x - center.x);
        path = path.line_to(p0);
        path = path.curve_to(
            Point::new(p0.x + t0.x * k, p0.y + t0.y * k),
            Point::new(p1.x - t1.x * k, p1.y - t1.y * k),
            p1,
        );
    }
    path
}

fn polar(center: Point, radius: f32, angle: f32) -> Point {
    Point::new(center.x + radius * angle.cos(), center.y + radius * angle.sin())
}

/// The outline a node's shape asks for, and whether it is filled.
fn shape_path(shape: NodeShape, rect: Rect, size: f32) -> (ShapePath, Option<Ink>) {
    let w = rect.right - rect.left;
    let h = rect.bottom - rect.top;
    let center = rect.center();
    match shape {
        NodeShape::Rect => (rect_path(rect), None),
        NodeShape::Round => (rounded_rect(rect, size * 0.7), None),
        NodeShape::Stadium => (rounded_rect(rect, h * 0.5), None),
        NodeShape::Subroutine => {
            // A rectangle with an inner line down each side, which is Mermaid's `[[ ]]`.
            let inset = (size * 0.5).min(w * 0.25);
            (
                rect_path(rect)
                    .move_to(Point::new(rect.left + inset, rect.top))
                    .line_to(Point::new(rect.left + inset, rect.bottom))
                    .move_to(Point::new(rect.right - inset, rect.top))
                    .line_to(Point::new(rect.right - inset, rect.bottom)),
                None,
            )
        }
        NodeShape::Cylinder => {
            let cap = (h * 0.16).min(size * 0.7);
            let k = cap * KAPPA;
            let path = ShapePath::closed(Point::new(rect.left, rect.top + cap))
                .curve_to(Point::new(rect.left, rect.top + cap - k), Point::new(center.x - w * 0.25, rect.top), Point::new(center.x, rect.top))
                .curve_to(Point::new(center.x + w * 0.25, rect.top), Point::new(rect.right, rect.top + cap - k), Point::new(rect.right, rect.top + cap))
                .line_to(Point::new(rect.right, rect.bottom - cap))
                .curve_to(Point::new(rect.right, rect.bottom - cap + k), Point::new(center.x + w * 0.25, rect.bottom), Point::new(center.x, rect.bottom))
                .curve_to(Point::new(center.x - w * 0.25, rect.bottom), Point::new(rect.left, rect.bottom - cap + k), Point::new(rect.left, rect.bottom - cap));
            (path, None)
        }
        NodeShape::Circle | NodeShape::DoubleCircle => (circle_path(center, w.min(h) * 0.5), None),
        NodeShape::Rhombus => (
            ShapePath::closed(Point::new(center.x, rect.top))
                .line_to(Point::new(rect.right, center.y))
                .line_to(Point::new(center.x, rect.bottom))
                .line_to(Point::new(rect.left, center.y)),
            None,
        ),
        NodeShape::Hexagon => {
            let cut = (w * 0.2).min(size * 1.4);
            (
                ShapePath::closed(Point::new(rect.left + cut, rect.top))
                    .line_to(Point::new(rect.right - cut, rect.top))
                    .line_to(Point::new(rect.right, center.y))
                    .line_to(Point::new(rect.right - cut, rect.bottom))
                    .line_to(Point::new(rect.left + cut, rect.bottom))
                    .line_to(Point::new(rect.left, center.y)),
                None,
            )
        }
        NodeShape::Parallelogram => {
            let cut = (w * 0.16).min(size * 1.2);
            (
                ShapePath::closed(Point::new(rect.left + cut, rect.top))
                    .line_to(Point::new(rect.right, rect.top))
                    .line_to(Point::new(rect.right - cut, rect.bottom))
                    .line_to(Point::new(rect.left, rect.bottom)),
                None,
            )
        }
        NodeShape::Trapezoid => {
            let cut = (w * 0.16).min(size * 1.2);
            (
                ShapePath::closed(Point::new(rect.left + cut, rect.top))
                    .line_to(Point::new(rect.right - cut, rect.top))
                    .line_to(Point::new(rect.right, rect.bottom))
                    .line_to(Point::new(rect.left, rect.bottom)),
                None,
            )
        }
        NodeShape::Asymmetric => {
            let cut = (w * 0.18).min(size * 1.4);
            (
                ShapePath::closed(Point::new(rect.left, rect.top))
                    .line_to(Point::new(rect.right - cut, rect.top))
                    .line_to(Point::new(rect.right, center.y))
                    .line_to(Point::new(rect.right - cut, rect.bottom))
                    .line_to(Point::new(rect.left, rect.bottom)),
                None,
            )
        }
        NodeShape::Start => (circle_path(center, w.min(h) * 0.5), Some(Ink::Fill)),
        NodeShape::Finish => {
            let r = w.min(h) * 0.5;
            // A ring with a filled centre, which is how a state diagram ends.
            (circle_path(center, r), None)
        }
    }
}

// ---------------------------------------------------------------------------
// Sequence
// ---------------------------------------------------------------------------

/// A sequence diagram: participants over their lifelines, messages between them, the
/// frames of its loops drawn behind, and its notes beside the message they explain.
fn sequence(
    participants: &[Node],
    messages: &[crate::parse::Message],
    notes: &[crate::parse::Note],
    fragments: &[crate::parse::Fragment],
    size: f32,
    m: DiagramMeasure<'_>,
    title: &str,
) -> Scene {
    let mut scene = Scene { kind: "sequence", ..Scene::default() };
    if participants.is_empty() {
        return scene;
    }
    let names: Vec<String> = participants
        .iter()
        .map(|p| if p.label.is_empty() { p.id.clone() } else { p.label.clone() })
        .collect();
    let headings: Vec<Measured> = names.iter().map(|n| measure_label(n, size * 0.95, TextRole::Section, false, m)).collect();
    let head_h = headings.iter().map(Measured::height).fold(0.0f32, f32::max) + size * PAD_Y * 2.0;
    let head_w = headings.iter().map(Measured::width).fold(0.0f32, f32::max) + size * PAD_X * 2.0;
    let after_title = if title.is_empty() { 0.0 } else { size * 2.0 };
    // Participants are centred on their lifelines, and the lifelines are as far apart
    // as their own boxes need.
    let mut centers: Vec<f32> = Vec::with_capacity(participants.len());
    let mut at = head_w * 0.5;
    for (i, _) in participants.iter().enumerate() {
        if i > 0 {
            at += head_w + size * SIBLING_GAP * 1.8;
        }
        centers.push(at);
    }
    // The width of the widest message label, split over the gap it has to cross.
    let row_h = size * 2.1;
    // A frame's tag sits on the frame's top edge, over the first message row it
    // covers, so that row leaves a strip for it -- without the strip the tag lands on
    // the message's own backdrop.
    let tags: Vec<Measured> = fragments
        .iter()
        .map(|f| measure_label(&format!("{} {}", f.kind, f.label), size * 0.8, TextRole::Title, false, m))
        .collect();
    let mut strip = vec![0.0f32; messages.len()];
    for (fragment, tag) in fragments.iter().zip(&tags) {
        if let Some(at) = strip.get_mut(fragment.at) {
            *at = at.max(tag.height() * 1.2 + size * 0.5);
        }
    }
    let mut y = after_title + head_h + row_h;
    // Notes take rows of their own, which is where a note's box is drawn.
    let mut note_top: Vec<f32> = Vec::with_capacity(notes.len());
    let mut rows: Vec<(usize, f32)> = Vec::with_capacity(messages.len());
    for (i, extra) in strip.iter().enumerate() {
        y += row_h + extra;
        rows.push((i, y));
    }
    let body_bottom = rows.last().map(|(_, y)| *y).unwrap_or(after_title + head_h) + row_h * 0.5;
    let mut note_y = body_bottom;
    for _ in notes {
        note_y += size * 1.4;
        note_top.push(note_y);
        note_y += size * 1.8;
    }
    let bottom = note_y.max(body_bottom) + size;
    let width_total = centers.last().copied().unwrap_or(head_w) + head_w * 0.5;
    // Frames behind everything: one box per loop or alt, spanning the participants it
    // covers.
    for (fragment, text) in fragments.iter().zip(&tags) {
        let start = rows
            .get(fragment.at)
            .map(|(_, y)| *y - row_h * 0.85 - text.height() * 1.2 - size * 0.5)
            .unwrap_or(after_title + head_h);
        let end = rows.get(fragment.end.saturating_sub(1)).map(|(_, y)| *y + row_h * 0.5).unwrap_or(bottom);
        let left = size * 0.4 + fragment.depth as f32 * size * 0.5;
        let right = width_total + size * 0.4;
        let top = start - size * 0.3;
        let rect = Rect { left, top, right, bottom: end };
        scene.shapes.push(Shape { path: rect_path(rect), fill: None, stroke: Stroke::Thin, ink: Ink::Stroke });
        let tag = Rect { left: rect.left + size * 0.4, top: rect.top, right: rect.left + size * 0.4 + text.width() + size * 0.6, bottom: rect.top + text.height() * 1.2 };
        scene.shapes.push(Shape { path: rect_path(tag), fill: Some(Ink::Backdrop), stroke: Stroke::None, ink: Ink::Backdrop });
        scene.shapes.push(Shape { path: rect_path(tag), fill: None, stroke: Stroke::Thin, ink: Ink::Stroke });
        place_label(&mut scene.labels, text, tag.left + size * 0.3, text.width(), tag.top, tag.bottom - tag.top, size * 0.8, Alignment::Left, Ink::Text, TextRole::Title, 600, false);
    }
    // The lifelines, then the participant boxes over them.
    for &x in &centers {
        scene.shapes.push(Shape {
            path: ShapePath::line(Point::new(x, after_title + head_h), Point::new(x, bottom)),
            fill: None,
            stroke: Stroke::Thin,
            ink: Ink::Stroke,
        });
    }
    for (i, heading) in headings.iter().enumerate() {
        let rect = Rect { left: centers[i] - head_w * 0.5, top: after_title, right: centers[i] + head_w * 0.5, bottom: after_title + head_h };
        scene.shapes.push(Shape { path: rect_path(rect), fill: None, stroke: Stroke::Normal, ink: Ink::Stroke });
        place_label(&mut scene.labels, heading, rect.left + size * PAD_X, head_w - size * PAD_X * 2.0, rect.top, head_h, size * 0.95, Alignment::Center, Ink::Text, TextRole::Section, 600, false);
    }
    // The messages, over the lifelines.
    for (index, y) in rows {
        let message = &messages[index];
        let from = participants.iter().position(|p| p.id == message.from);
        let to = participants.iter().position(|p| p.id == message.to);
        let (Some(from), Some(to)) = (from, to) else { continue };
        let (a, b) = (centers[from], centers[to]);
        let dir = if b >= a { 1.0 } else { -1.0 };
        let tip = Point::new(b, y);
        let start = Point::new(a, y);
        let end = Point::new(b - dir * size * HEAD_LEN * 0.8, y);
        scene.shapes.push(Shape {
            path: ShapePath::line(start, end),
            fill: None,
            stroke: Stroke::Normal,
            ink: Ink::Stroke,
        });
        let tip_dir = Point::new(dir, 0.0);
        if !message.open {
            scene.shapes.push(Shape {
                path: arrow_head(tip, tip_dir, size),
                fill: Some(Ink::Arrow),
                stroke: Stroke::None,
                ink: Ink::Arrow,
            });
        } else {
            // An open head: two strokes, which is what `->` means against `->>`.
            let back = Point::new(tip.x - dir * size * HEAD_LEN, tip.y);
            let half = size * HEAD_HALF;
            scene.shapes.push(Shape { path: ShapePath::line(back, Point::new(tip.x, tip.y - half)), fill: None, stroke: Stroke::Normal, ink: Ink::Arrow });
            scene.shapes.push(Shape { path: ShapePath::line(back, Point::new(tip.x, tip.y + half)), fill: None, stroke: Stroke::Normal, ink: Ink::Arrow });
        }
        if !message.text.is_empty() {
            let text = measure_label(&message.text, size * 0.85, TextRole::Edge, false, m);
            let mid = (a + b) * 0.5;
            let left = mid - text.width() * 0.5;
            let top = y - row_h * 0.85;
            let band = text.height() * 1.15;
            scene.shapes.push(Shape {
                path: rect_path(Rect { left: left - size * 0.2, top, right: left + text.width() + size * 0.2, bottom: top + band }),
                fill: Some(Ink::Backdrop),
                stroke: Stroke::None,
                ink: Ink::Backdrop,
            });
            place_label(&mut scene.labels, &text, left, text.width(), top, band, size * 0.85, Alignment::Center, Ink::Text, TextRole::Edge, 400, false);
        }
    }
    // Notes last: they are the reader's aside and belong over everything.
    for (i, note) in notes.iter().enumerate() {
        let y = note_top.get(i).copied().unwrap_or(bottom);
        let text = measure_label(&note.text, size * 0.8, TextRole::Edge, false, m);
        let (w, h) = (text.width() + size * 1.2, text.height() + size * 0.8);
        let (left, right) = if let Some(at) = note.who.find(',') {
            let a = participants.iter().position(|p| p.id == note.who[..at].trim());
            let b = participants.iter().position(|p| p.id == note.who[at + 1..].trim());
            match (a, b) {
                (Some(a), Some(b)) => (centers[a].min(centers[b]) - w * 0.5, centers[a].max(centers[b]) + w * 0.5),
                (Some(a), None) => (centers[a] - w * 0.5, centers[a] + w * 0.5),
                _ => (0.0, w),
            }
        } else {
            match participants.iter().position(|p| p.id == note.who) {
                Some(a) if note.over => (centers[a] - w * 0.5, centers[a] + w * 0.5),
                Some(a) => (centers[a] + size * 0.6, centers[a] + size * 0.6 + w),
                _ => (0.0, w),
            }
        };
        let rect = Rect { left: left.min(right - w), top: y, right: left.min(right - w) + w.max(right - left), bottom: y + h };
        let rect = Rect { left: rect.left.min(left), top: rect.top, right: rect.right.max(right), bottom: rect.bottom };
        scene.shapes.push(Shape { path: rounded_rect(rect, size * 0.4), fill: Some(Ink::Backdrop), stroke: Stroke::Thin, ink: Ink::Stroke });
        place_label(&mut scene.labels, &text, rect.left + size * 0.6, rect.right - rect.left - size * 1.2, rect.top, rect.bottom - rect.top, size * 0.8, Alignment::Center, Ink::Text, TextRole::Edge, 400, false);
    }
    if !title.is_empty() {
        let text = measure_label(title, size * 1.05, TextRole::Title, false, m);
        place_label(&mut scene.labels, &text, 0.0, text.width(), 0.0, text.height(), size * 1.05, Alignment::Left, Ink::Text, TextRole::Title, 700, false);
    }
    scene.elements = participants.len();
    scene.edges = messages.len();
    finish(scene, size)
}

// ---------------------------------------------------------------------------
// Pie
// ---------------------------------------------------------------------------

/// A pie chart: a disc cut into slices, with a legend under it.
fn pie(title: &str, show_data: bool, slices: &[crate::parse::Slice], size: f32, m: DiagramMeasure<'_>) -> Scene {
    let mut scene = Scene { kind: "pie", ..Scene::default() };
    let total: f32 = slices.iter().map(|s| s.value).sum();
    let radius = (size * 7.0).max(40.0);
    let center = Point::new(radius, radius);
    let mut angle = -std::f32::consts::FRAC_PI_2;
    for (i, slice) in slices.iter().enumerate() {
        let share = slice.value / total;
        let next = angle + share * std::f32::consts::TAU;
        scene.shapes.push(Shape {
            path: wedge(center, radius, angle, next),
            fill: Some(Ink::Slice((i % 8) as u8)),
            stroke: Stroke::Thin,
            ink: Ink::Backdrop,
        });
        // The slice's own name on the slice, when the slice is wide enough to hold it.
        if share >= 0.05 {
            let mid = (angle + next) * 0.5;
            let at = polar(center, radius * 0.66, mid);
            let text = measure_label(&slice.label, size * 0.8, TextRole::Node, false, m);
            place_label(&mut scene.labels, &text, at.x - text.width() * 0.5, text.width(), at.y - text.height() * 0.5, text.height(), size * 0.8, Alignment::Center, Ink::Backdrop, TextRole::Node, 600, false);
            let value = if show_data { format!("{:.2}%", share * 100.0) } else { format!("{:.0}%", share * 100.0) };
            let value_text = measure_label(&value, size * 0.7, TextRole::Node, false, m);
            place_label(&mut scene.labels, &value_text, at.x - value_text.width() * 0.5, value_text.width(), at.y + text.height() * 0.6, value_text.height(), size * 0.7, Alignment::Center, Ink::Backdrop, TextRole::Node, 400, false);
        }
        angle = next;
    }
    let mut y = radius * 2.0 + size * 1.4;
    for (i, slice) in slices.iter().enumerate() {
        let text = measure_label(&slice.label, size * 0.9, TextRole::Node, false, m);
        let swatch = size * 0.7;
        scene.shapes.push(Shape {
            path: rect_path(Rect { left: 0.0, top: y + size * 0.15, right: swatch, bottom: y + size * 0.15 + swatch }),
            fill: Some(Ink::Slice((i % 8) as u8)),
            stroke: Stroke::Thin,
            ink: Ink::Backdrop,
        });
        place_label(&mut scene.labels, &text, swatch + size * 0.5, text.width(), y, text.height(), size * 0.9, Alignment::Left, Ink::Text, TextRole::Node, 400, false);
        y += text.height().max(swatch) + size * 0.4;
    }
    if !title.is_empty() {
        let text = measure_label(title, size * 1.05, TextRole::Title, false, m);
        place_label(&mut scene.labels, &text, 0.0, text.width(), -text.height() - size * 0.6, text.height(), size * 1.05, Alignment::Left, Ink::Text, TextRole::Title, 700, false);
    }
    scene.elements = slices.len();
    finish(scene, size)
}

// ---------------------------------------------------------------------------
// Tree view
// ---------------------------------------------------------------------------

/// A `treeView`: one row per item, indented to its depth, with the elbow lines a
/// file tree is drawn from.
///
/// The rows are the diagram: an item's depth is the parser's reading of its own
/// indentation or branch column, the elbow under each parent is drawn from the
/// branch's own column, and a directory is set bold by its [`TextRole`]. A `##`
/// description follows its label in italic, and a `:::highlight` row takes the
/// theme's accent ink.
fn tree_view(items: &[TreeItem], size: f32, m: DiagramMeasure<'_>, title: &str) -> Scene {
    let mut scene = Scene { kind: "tree view", ..Scene::default() };
    if items.is_empty() {
        return scene;
    }
    // One level of nesting, and the room a row keeps around its own words.
    let step = size * 1.6;
    let gap = size * 0.45;
    let measured: Vec<(Measured, Option<Measured>)> = items
        .iter()
        .map(|item| {
            let role = if item.directory { TextRole::Section } else { TextRole::Node };
            let label = measure_label(&item.label, size, role, false, m);
            let description = (!item.description.is_empty())
                .then(|| measure_label(&item.description, size * 0.9, TextRole::Node, true, m));
            (label, description)
        })
        .collect();
    let mut tops: Vec<f32> = Vec::with_capacity(items.len());
    let mut heights: Vec<f32> = Vec::with_capacity(items.len());
    let mut y = if title.is_empty() { 0.0 } else { size * 2.2 };
    for (label, description) in &measured {
        let h = label.height().max(description.as_ref().map_or(0.0, Measured::height)) + gap;
        tops.push(y);
        heights.push(h);
        y += h;
    }
    // The elbow lines first, so the words are painted over them: a stub from the
    // parent's column into the label, and the elbow it hangs from under the parent
    // row down to the row's own middle.
    for (i, item) in items.iter().enumerate() {
        let Some(parent) = item.parent else { continue };
        let parent = parent as usize;
        if item.depth == 0 || parent >= items.len() || parent >= i {
            continue;
        }
        let at = item.depth as f32 * step;
        let elbow = at - step * 0.5;
        let middle = tops[i] + heights[i] * 0.5;
        // From under the parent row: a middle child then reads as `├` and a last one
        // as `└`, because the next sibling's own line carries the branch on down.
        let from = tops[parent] + heights[parent];
        scene.shapes.push(Shape {
            path: ShapePath::line(Point::new(elbow, from), Point::new(elbow, middle)),
            fill: None,
            stroke: Stroke::Thin,
            ink: Ink::Stroke,
        });
        scene.shapes.push(Shape {
            path: ShapePath::line(Point::new(elbow, middle), Point::new(at - size * 0.3, middle)),
            fill: None,
            stroke: Stroke::Thin,
            ink: Ink::Stroke,
        });
    }
    // Then the rows themselves.
    for (i, item) in items.iter().enumerate() {
        let (label, description) = &measured[i];
        let at = item.depth as f32 * step;
        let (role, weight) = if item.directory {
            (TextRole::Section, 600)
        } else {
            (TextRole::Node, 400)
        };
        let ink = if item.highlight { Ink::Accent } else { Ink::Text };
        place_label(
            &mut scene.labels,
            label,
            at,
            label.width(),
            tops[i],
            heights[i],
            size,
            Alignment::Left,
            ink,
            role,
            weight,
            false,
        );
        if let Some(description) = description {
            let after = at + label.width() + gap * 0.9;
            place_label(
                &mut scene.labels,
                description,
                after,
                description.width(),
                tops[i],
                heights[i],
                size * 0.9,
                Alignment::Left,
                Ink::Muted,
                TextRole::Node,
                400,
                true,
            );
        }
    }
    if !title.is_empty() {
        let text = measure_label(title, size * 1.05, TextRole::Title, false, m);
        place_label(&mut scene.labels, &text, 0.0, text.width(), 0.0, text.height(), size * 1.05, Alignment::Left, Ink::Text, TextRole::Title, 700, false);
    }
    scene.elements = items.len();
    scene.edges = items.iter().filter(|i| i.parent.is_some()).count();
    finish(scene, size)
}

// ---------------------------------------------------------------------------
// Framing
// ---------------------------------------------------------------------------

/// Give the scene a margin, and take its size from the ink it actually put down --
/// which is the only honest answer once a label has been centred, a curve bulged, or
/// an arrow head drawn past the point it points at.
fn finish(mut scene: Scene, size: f32) -> Scene {
    let mut lo = Point::new(f32::INFINITY, f32::INFINITY);
    let mut hi = Point::new(f32::NEG_INFINITY, f32::NEG_INFINITY);
    for shape in &scene.shapes {
        for p in shape.points() {
            if !p.x.is_finite() || !p.y.is_finite() {
                continue;
            }
            lo.x = lo.x.min(p.x);
            lo.y = lo.y.min(p.y);
            hi.x = hi.x.max(p.x);
            hi.y = hi.y.max(p.y);
        }
    }
    for label in &scene.labels {
        lo.x = lo.x.min(label.left);
        lo.y = lo.y.min(label.baseline - label.size);
        hi.x = hi.x.max(label.right);
        hi.y = hi.y.max(label.baseline + label.size * 0.3);
    }
    if !lo.x.is_finite() || !hi.x.is_finite() {
        return scene;
    }
    let pad = size * 0.5;
    let (dx, dy) = (pad - lo.x, pad - lo.y);
    for shape in &mut scene.shapes {
        shape.translate(dx, dy);
    }
    for label in &mut scene.labels {
        label.baseline += dy;
        label.left += dx;
        label.right += dx;
    }
    scene.width = (hi.x - lo.x).max(0.0) + pad * 2.0;
    scene.height = (hi.y - lo.y).max(0.0) + pad * 2.0;
    scene
}

/// Names for the report: what kind of thing was drawn.
pub fn kind_label(kind: DiagramKind) -> &'static str {
    kind.label()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::parse;

    const SIZE: f32 = 10.0;
    const EPS: f32 = 0.5;

    /// A fixed-width mock: every character takes 0.6 em, the ascent 0.8 em and the
    /// descent 0.2 em, so the arithmetic can be checked by hand.
    fn mock(text: &str, size: f32, _role: TextRole, _italic: bool) -> TextMetrics {
        TextMetrics { advance: text.chars().count() as f32 * size * 0.6, ascent: size * 0.8, descent: size * 0.2 }
    }

    fn scene_of(source: &str) -> Scene {
        let diagram = parse(source).unwrap_or_else(|e| panic!("{source:?} did not parse: {e}"));
        layout(&diagram, SIZE, &mut mock)
    }

    fn bounding_box(shape: &Shape) -> Rect {
        let mut lo = Point::new(f32::INFINITY, f32::INFINITY);
        let mut hi = Point::new(f32::NEG_INFINITY, f32::NEG_INFINITY);
        for p in shape.points() {
            lo.x = lo.x.min(p.x);
            lo.y = lo.y.min(p.y);
            hi.x = hi.x.max(p.x);
            hi.y = hi.y.max(p.y);
        }
        Rect { left: lo.x, top: lo.y, right: hi.x, bottom: hi.y }
    }

    /// A node's box: a closed outline in the stroke ink. A subroutine's inner lines are
    /// part of the same path, an arrow head is filled, and a panel is thin, so this is
    /// exactly the node boxes, in the order the nodes were declared.
    fn node_boxes(scene: &Scene) -> Vec<Rect> {
        scene
            .shapes
            .iter()
            .filter(|s| s.path.closed && s.fill.is_none() && s.stroke == Stroke::Normal && s.ink == Ink::Stroke)
            .map(bounding_box)
            .collect()
    }

    fn contains(outer: Rect, inner: Rect) -> bool {
        inner.left >= outer.left - EPS
            && inner.right <= outer.right + EPS
            && inner.top >= outer.top - EPS
            && inner.bottom <= outer.bottom + EPS
    }

    fn finite(scene: &Scene) {
        assert!(scene.width.is_finite() && scene.height.is_finite(), "{scene:?}");
        assert!(scene.width > 0.0 && scene.height > 0.0, "{scene:?}");
        for shape in &scene.shapes {
            for p in shape.points() {
                assert!(p.x.is_finite() && p.y.is_finite(), "{shape:?}");
                assert!(
                    p.x >= -EPS && p.x <= scene.width + EPS && p.y >= -EPS && p.y <= scene.height + EPS,
                    "{p:?} is outside the {} x {} scene: {shape:?}",
                    scene.width,
                    scene.height
                );
            }
        }
        for label in &scene.labels {
            assert!(!label.text.is_empty(), "{label:?}");
            assert!(label.left.is_finite() && label.right.is_finite() && label.baseline.is_finite(), "{label:?}");
            assert!(label.left <= label.right + EPS, "{label:?}");
            assert!(
                label.left >= -EPS && label.right <= scene.width + EPS,
                "{label:?} is outside the {}-wide scene",
                scene.width
            );
            assert!(
                label.baseline >= -EPS && label.baseline <= scene.height + EPS,
                "{label:?} is outside the {}-tall scene",
                scene.height
            );
        }
    }

    #[test]
    fn boxes_stay_off_each_other_and_on_the_page() {
        let scene = scene_of("graph TD\n a[Alpha] --> b[Beta]\n a --> c[Gamma]\n b --> d[Delta]\n c --> d\n");
        let boxes = node_boxes(&scene);
        assert_eq!(boxes.len(), 4);
        for (i, a) in boxes.iter().enumerate() {
            assert!(a.left >= -EPS && a.right <= scene.width + EPS, "{a:?} is off the {}-wide page", scene.width);
            assert!(a.top >= -EPS && a.bottom <= scene.height + EPS, "{a:?} is off the {}-tall page", scene.height);
            for b in &boxes[i + 1..] {
                let overlap = a.left < b.right - EPS
                    && b.left < a.right - EPS
                    && a.top < b.bottom - EPS
                    && b.top < a.bottom - EPS;
                assert!(!overlap, "{a:?} and {b:?} overlap");
            }
        }
        // And the arrows point the way the source does: all four boxes are ranked, so
        // the picture is taller than it is wide.
        assert!(scene.height > scene.width, "{scene:?}");
    }

    #[test]
    fn a_top_to_bottom_flow_reads_downwards_and_each_direction_has_its_own_axis() {
        let down = node_boxes(&scene_of("graph TD\n a --> b\n"));
        assert!(down[0].top < down[1].top, "TD puts a over b: {down:?}");
        let up = node_boxes(&scene_of("graph BT\n a --> b\n"));
        assert!(up[0].top > up[1].top, "BT puts a under b: {up:?}");
        let right = node_boxes(&scene_of("graph LR\n a --> b\n"));
        assert!(right[0].left < right[1].left, "LR puts a beside b: {right:?}");
        let left = node_boxes(&scene_of("graph RL\n a --> b\n"));
        assert!(left[0].left > left[1].left, "RL puts a beside b too: {left:?}");
    }

    #[test]
    fn an_arrow_head_lands_on_the_edge_of_the_box_it_points_at() {
        let scene = scene_of("graph TD\n a --> b\n");
        let boxes = node_boxes(&scene);
        let head = scene
            .shapes
            .iter()
            .find(|s| s.fill == Some(Ink::Arrow))
            .expect("an arrow head");
        let b = boxes[1];
        assert!(
            head.points().any(|p| (p.y - b.top).abs() < EPS && p.x >= b.left - EPS && p.x <= b.right + EPS),
            "{head:?} does not touch {}",
            b.top
        );
    }

    #[test]
    fn a_self_loop_leaves_its_box_and_comes_back_to_its_top() {
        let scene = scene_of("graph TD\n a --> a\n");
        let boxes = node_boxes(&scene);
        assert_eq!(boxes.len(), 1);
        let a = boxes[0];
        let loop_path = scene
            .shapes
            .iter()
            .filter(|s| !s.path.closed && s.ink == Ink::Stroke)
            .map(bounding_box)
            .find(|r| r.right > a.right + EPS)
            .expect("a loop beside the box");
        assert!(loop_path.top < a.top, "a loop comes back over the top: {loop_path:?}");
        let head = scene.shapes.iter().find(|s| s.fill == Some(Ink::Arrow)).expect("an arrow head");
        assert!(
            head.points().any(|p| (p.y - a.top).abs() < EPS && p.x >= a.left - EPS && p.x <= a.right + EPS),
            "the loop's head lands on the box top"
        );
    }

    #[test]
    fn an_edge_label_sits_on_its_line_with_a_backdrop() {
        let scene = scene_of("graph LR\n a -- yes --> b\n");
        let label = scene.labels.iter().find(|l| l.text == "yes").expect("the edge label");
        assert_eq!(label.role, TextRole::Edge);
        assert_eq!(label.ink, Ink::Text);
        let backdrops = scene.shapes.iter().filter(|s| s.fill == Some(Ink::Backdrop)).count();
        assert_eq!(backdrops, 1, "the label sits on its own backdrop");
        let boxes = node_boxes(&scene);
        let mid = (label.left + label.right) * 0.5;
        assert!(mid > boxes[0].right && mid < boxes[1].left, "the label is between the boxes: {mid}");
    }

    #[test]
    fn a_subgraph_is_a_panel_behind_its_members() {
        let scene = scene_of("graph TD\n subgraph one[Panel]\n a --> b\n end\n c --> a\n");
        let panel = scene
            .shapes
            .iter()
            .filter(|s| s.path.closed && s.fill.is_none() && s.stroke == Stroke::Thin && s.ink == Ink::Stroke)
            .map(bounding_box)
            .next()
            .expect("the panel");
        let title = scene.labels.iter().find(|l| l.text == "Panel").expect("the panel's heading");
        assert_eq!(title.role, TextRole::Title);
        assert_eq!(title.weight, 600);
        let boxes = node_boxes(&scene);
        assert!(contains(panel, boxes[0]) && contains(panel, boxes[1]), "the panel holds a and b");
        assert!(!contains(panel, boxes[2]), "the panel does not hold c");
        // The heading sits in the strip above the box it names.
        assert!(title.baseline < boxes[0].top, "the heading is over its members");
    }

    #[test]
    fn a_start_or_finish_disc_carries_no_words() {
        let scene = scene_of("stateDiagram-v2\n [*] --> Still\n Still --> [*]\n");
        let words: Vec<&str> = scene.labels.iter().filter(|l| l.role == TextRole::Node).map(|l| l.text.as_str()).collect();
        assert_eq!(words, vec!["Still"]);
        assert_eq!(scene.kind, "state");
        assert_eq!(scene.elements, 3, "two discs and one state");
        assert_eq!(scene.edges, 2);
        finite(&scene);
    }

    #[test]
    fn a_subroutine_box_keeps_its_inner_lines_in_its_own_path() {
        let scene = scene_of("graph TD\n a[[Store]]\n");
        let path = &scene
            .shapes
            .iter()
            .find(|s| s.path.closed && s.fill.is_none() && s.stroke == Stroke::Normal && s.ink == Ink::Stroke)
            .expect("the box")
            .path;
        assert!(path.closed, "the outer rectangle closes");
        assert_eq!(
            path.cmds.iter().filter(|c| matches!(c, PathCmd::MoveTo(_))).count(),
            2,
            "two inner lines, each a figure of its own"
        );
    }

    #[test]
    fn a_mind_map_hangs_its_branches_off_the_root() {
        let scene = scene_of("mindmap\n root((idea))\n   one\n     deep\n   two\n");
        assert_eq!(scene.kind, "mindmap");
        assert!(scene.labels.iter().any(|l| l.text == "idea" && l.role == TextRole::Node));
        let boxes = node_boxes(&scene);
        assert_eq!(boxes.len(), 4);
        // `deep` is a rank past `one`, and the ranks run left to right.
        assert!(boxes[1].left < boxes[2].left, "one is over deep: {boxes:?}");
        assert!(boxes[0].left < boxes[1].left, "the root is first: {boxes:?}");
    }

    #[test]
    fn a_tree_view_indents_its_rows_and_hangs_their_elbows() {
        let scene = scene_of(
            "treeView\n    src/\n        index.js\n        lib/\n            util.js\n    \
             package.json\n",
        );
        assert_eq!(scene.kind, "tree view");
        assert_eq!(scene.elements, 5);
        assert_eq!(scene.edges, 3, "three rows hang off a parent; the two roots do not");
        // The rows are laid out in reading order, one level to the right per depth.
        let labels: Vec<&Label> = scene.labels.iter().collect();
        assert_eq!(labels.len(), 5);
        assert_eq!(labels[0].role, TextRole::Section, "a directory reads as a section");
        assert_eq!(labels[0].weight, 600);
        assert_eq!(labels[1].role, TextRole::Node, "a file keeps the node face");
        assert_eq!(labels[1].weight, 400);
        let step = labels[1].left - labels[0].left;
        assert!(step > 0.0, "a level indents");
        assert!((labels[3].left - labels[1].left - step).abs() < EPS, "util.js is one level deeper");
        assert!((labels[4].left - labels[0].left).abs() < EPS, "package.json is a root again");
        // The elbows: two lines per parented row, all thin strokes.
        let lines: Vec<&Shape> = scene.shapes.iter().filter(|s| !s.path.closed).collect();
        assert_eq!(lines.len(), 6, "a vertical and a stub per parented row");
        assert!(lines.iter().all(|s| s.stroke == Stroke::Thin && s.ink == Ink::Stroke));
        // Every elbow's vertical runs from under its parent row down to its own.
        for line in &lines {
            let rect = bounding_box(line);
            assert!(rect.top >= labels[0].baseline - labels[0].size, "{line:?} is above the first row");
        }
        finite(&scene);
    }

    #[test]
    fn a_tree_view_sets_a_description_in_italics_and_a_highlight_in_the_accent() {
        let scene = scene_of("treeView\n    src/ ## the source\n    package.json :::highlight\n");
        let desc = scene.labels.iter().find(|l| l.text == "the source").expect("the description");
        assert!(desc.italic, "a `##` description is italic");
        assert_eq!(desc.ink, Ink::Muted);
        let label = scene.labels.iter().find(|l| l.text == "src/").expect("the label");
        assert!(desc.left > label.right, "the description follows its label: {desc:?}");
        let highlighted = scene.labels.iter().find(|l| l.text == "package.json").expect("the highlighted row");
        assert_eq!(highlighted.ink, Ink::Accent);
        assert!(!highlighted.italic);
        finite(&scene);
    }

    #[test]
    fn a_sequence_puts_its_participants_over_their_lifelines() {
        let scene = scene_of("sequenceDiagram\n participant A as Alice\n participant B\n A->>B: hi\n");
        assert_eq!(scene.kind, "sequence");
        assert_eq!(scene.elements, 2);
        assert_eq!(scene.edges, 1);
        let boxes = node_boxes(&scene);
        assert_eq!(boxes.len(), 2);
        assert!((boxes[0].top - boxes[1].top).abs() < EPS, "the headings line up");
        assert!(boxes[0].right < boxes[1].left, "and stand apart");
        let lifelines = scene.shapes.iter().filter(|s| !s.path.closed && s.stroke == Stroke::Thin && s.ink == Ink::Stroke).count();
        assert_eq!(lifelines, 2);
        let label = scene.labels.iter().find(|l| l.text == "hi").expect("the message");
        assert_eq!(label.role, TextRole::Edge);
        assert!(label.baseline > boxes[0].bottom, "the message is under the headings");
        assert!(scene.labels.iter().any(|l| l.text == "Alice" && l.role == TextRole::Section));
        finite(&scene);
    }

    #[test]
    fn a_pie_is_a_slice_per_value_with_a_legend_under_it() {
        let scene = scene_of("pie title Shares\n \"Cats\" : 60\n \"Dogs\" : 40\n");
        assert_eq!(scene.kind, "pie");
        assert_eq!(scene.elements, 2);
        let wedges: Vec<Ink> = scene
            .shapes
            .iter()
            .filter(|s| matches!(s.fill, Some(Ink::Slice(_))) && s.stroke == Stroke::Thin)
            .map(|s| s.fill.unwrap())
            .collect();
        assert_eq!(wedges.len(), 4, "two slices, two legend swatches");
        assert_eq!(wedges[0], Ink::Slice(0));
        assert_eq!(wedges[1], Ink::Slice(1));
        assert_eq!(scene.labels.iter().filter(|l| l.text == "Cats").count(), 2, "the slice and the legend");
        assert!(scene.labels.iter().any(|l| l.text == "60%"));
        assert!(scene.labels.iter().any(|l| l.text == "Shares" && l.role == TextRole::Title));
        finite(&scene);
    }

    #[test]
    fn scaling_the_scene_moves_its_ink_and_its_type_together() {
        let mut scene = scene_of("graph TD\n a[Alpha] --> b[Beta]\n");
        let before = scene.clone();
        scene.scale(2.0);
        assert!((scene.width - before.width * 2.0).abs() < 0.01);
        assert!((scene.height - before.height * 2.0).abs() < 0.01);
        assert_eq!(scene.shapes.len(), before.shapes.len());
        for (a, b) in before.shapes.iter().zip(&scene.shapes) {
            for (p, q) in a.points().zip(b.points()) {
                assert!((p.x * 2.0 - q.x).abs() < 0.01 && (p.y * 2.0 - q.y).abs() < 0.01, "{p:?} did not double");
            }
        }
        for (a, b) in before.labels.iter().zip(&scene.labels) {
            assert!((a.size * 2.0 - b.size).abs() < 0.01, "type travels with the ink");
            assert!((a.baseline * 2.0 - b.baseline).abs() < 0.01);
        }
        // A factor that is not a factor changes nothing at all.
        let mut same = before.clone();
        same.scale(1.0);
        same.scale(0.0);
        same.scale(-2.0);
        same.scale(f32::NAN);
        assert_eq!(same, before);
    }

    #[test]
    fn a_label_is_drawn_as_at_most_the_lines_the_layout_allows() {
        let text = (0..40).map(|i| format!("line {i}")).collect::<Vec<_>>().join("\n");
        let measured = measure_label(&text, SIZE, TextRole::Node, false, &mut mock);
        assert_eq!(measured.lines.len(), MAX_LABEL_LINES);
        assert_eq!(lines_of(""), vec![String::new()], "an empty label is one empty line");
    }

    #[test]
    fn a_cycle_does_not_stop_the_ranking() {
        let Diagram::Graph { nodes, edges, groups, .. } = parse("graph TD\n a --> b\n b --> a\n c\n").unwrap() else {
            panic!("a graph")
        };
        let mut model = Model::build(&nodes, &edges, &groups);
        model.measure(SIZE, &mut mock);
        model.size(SIZE);
        let ranks = rank(&model);
        assert_eq!(ranks.len(), 3, "c, then a, then b");
        assert_eq!(ranks[0], vec![2], "the node nothing points at comes first");
        assert_eq!(ranks[2], vec![0], "the walk gets past the cycle");
        let mut seen: Vec<usize> = ranks.iter().flatten().copied().collect();
        seen.sort_unstable();
        assert_eq!(seen, vec![0, 1, 2], "every node is ranked exactly once");
    }

    #[test]
    fn no_diagram_of_the_corpus_comes_out_with_missing_arithmetic() {
        let sources = [
            "graph TD\n a --> b\n",
            "graph LR\n a(A) -->|yes| b{B} --> c((C))\n b --> b\n",
            "flowchart BT\n a[[s]] --> b[(cyl)]\n",
            "graph RL\n subgraph one\n a --> b\n end\n",
            "sequenceDiagram\n participant A as Alice\n A->>A: self\n Note right of A: think\n",
            "stateDiagram-v2\n [*] --> a\n a --> [*]\n",
            "classDiagram\n class Animal { +age }\n Animal <|-- Dog\n",
            "erDiagram\n A ||--o{ B : has\n",
            "pie\n \"Only\" : 100\n",
            "mindmap\n root((r))\n  x\n   y\n",
            "treeView\n src/\n  ├── a\n  └── \"b c\"\n",
            "treeView-beta\n    src/\n        thing.js ## the entry\n",
        ];
        for source in sources {
            let scene = scene_of(source);
            finite(&scene);
            assert!(scene.primitives() > 0, "{source:?} drew nothing");
        }
    }

    // -----------------------------------------------------------------------
    // The whole corpus, held to the invariants a reader notices when they break
    // -----------------------------------------------------------------------

    /// The fixture documents the reader actually puts on a page. `mermaid-broken`
    /// is here too: its fences are the ones that must *stay* unreadable.
    const CORPUS: [(&str, &str); 9] = [
        ("mermaid-flow", include_str!("../../../fixtures/mermaid-flow.md")),
        ("mermaid-sequence", include_str!("../../../fixtures/mermaid-sequence.md")),
        ("mermaid-state", include_str!("../../../fixtures/mermaid-state.md")),
        ("mermaid-class", include_str!("../../../fixtures/mermaid-class.md")),
        ("mermaid-er", include_str!("../../../fixtures/mermaid-er.md")),
        ("mermaid-pie", include_str!("../../../fixtures/mermaid-pie.md")),
        ("mermaid-mindmap", include_str!("../../../fixtures/mermaid-mindmap.md")),
        ("mermaid-tree", include_str!("../../../fixtures/mermaid-tree.md")),
        ("mermaid-broken", include_str!("../../../fixtures/mermaid-broken.md")),
    ];

    /// Every ```` ```mermaid ```` fence of a fixture, in reading order.
    fn fences_of(document: &str) -> Vec<String> {
        let mut fences = Vec::new();
        let mut inside: Option<String> = None;
        for line in document.lines() {
            match &mut inside {
                Some(text) => {
                    if line.trim_start().starts_with("```") {
                        fences.push(std::mem::take(text));
                        inside = None;
                    } else {
                        text.push_str(line);
                        text.push('\n');
                    }
                }
                None => {
                    if line.trim_start().starts_with("```mermaid") {
                        inside = Some(String::new());
                    }
                }
            }
        }
        fences
    }

    /// Every fence of the corpus, parsed and laid out, named for the failure messages.
    fn corpus_scenes() -> Vec<(&'static str, usize, crate::parse::Diagram, Scene)> {
        let mut scenes = Vec::new();
        for (name, document) in CORPUS {
            for (i, fence) in fences_of(document).into_iter().enumerate() {
                let Ok(diagram) = parse(&fence) else {
                    assert_eq!(name, "mermaid-broken", "{name} fence {i} stopped parsing: {:?}", parse(&fence).unwrap_err());
                    continue;
                };
                let scene = layout(&diagram, SIZE, &mut mock);
                scenes.push((name, i, diagram, scene));
            }
        }
        scenes
    }

    fn overlaps(a: Rect, b: Rect) -> bool {
        a.left < b.right - EPS && b.left < a.right - EPS && a.top < b.bottom - EPS && b.top < a.bottom - EPS
    }

    /// A label's words as a box, at the mock's own ascent and descent.
    fn label_box(label: &Label) -> Rect {
        Rect { left: label.left, right: label.right, top: label.baseline - label.size * 0.8, bottom: label.baseline + label.size * 0.2 }
    }

    /// The panels of a scene: thin closed outlines, which is what a subgraph draws.
    fn panel_boxes(scene: &Scene) -> Vec<Rect> {
        scene
            .shapes
            .iter()
            .filter(|s| s.path.closed && s.fill.is_none() && s.stroke == Stroke::Thin && s.ink == Ink::Stroke)
            .map(bounding_box)
            .collect()
    }

    #[test]
    fn every_edge_label_of_the_corpus_sits_clear_of_every_other_one() {
        for (name, fence, _, scene) in corpus_scenes() {
            let backdrops: Vec<Rect> = scene.shapes.iter().filter(|s| s.fill == Some(Ink::Backdrop)).map(bounding_box).collect();
            for (i, a) in backdrops.iter().enumerate() {
                for b in &backdrops[i + 1..] {
                    assert!(!overlaps(*a, *b), "{name} fence {fence}: two backdrops overlap: {a:?} and {b:?}");
                }
            }
            finite(&scene);
        }
    }

    #[test]
    fn every_subgraph_of_the_corpus_holds_its_members_and_stays_off_the_rest() {
        for (name, fence, diagram, scene) in corpus_scenes() {
            let crate::parse::Diagram::Graph { groups, nodes, .. } = &diagram else { continue };
            let boxes = node_boxes(&scene);
            let panels = panel_boxes(&scene);
            let filled: Vec<&Vec<String>> = groups
                .iter()
                .map(|g| &g.members)
                .filter(|members| members.iter().any(|id| nodes.iter().any(|n| n.id == *id)))
                .collect();
            assert_eq!(panels.len(), filled.len(), "{name} fence {fence}: one panel per group");
            for (panel, members) in panels.iter().zip(&filled) {
                let held: Vec<usize> = members.iter().filter_map(|id| nodes.iter().position(|n| n.id == *id)).collect();
                for &member in &held {
                    assert!(
                        contains(*panel, boxes[member]),
                        "{name} fence {fence}: the panel does not hold {:?}: {panel:?} vs {:?}",
                        members[member],
                        boxes[member]
                    );
                }
                for (i, b) in boxes.iter().enumerate() {
                    if !held.contains(&i) {
                        assert!(!overlaps(*panel, *b), "{name} fence {fence}: the panel overlaps box {i}: {panel:?} vs {b:?}");
                    }
                }
                // The only words a panel may hold are a member's own and its heading.
                for label in &scene.labels {
                    let words = label_box(label);
                    if label.role == TextRole::Node || !overlaps(*panel, words) {
                        continue;
                    }
                    assert!(
                        label.role == TextRole::Title && contains(*panel, words),
                        "{name} fence {fence}: the panel overlaps {:?}: {panel:?} vs {words:?}",
                        label.text
                    );
                }
            }
        }
    }
}
