//! Diagrams: one parsed ` ```mermaid ` fence per entry, typeset once and drawn
//! through the same display list the prose uses.
//!
//! [`rubrica_mermaid`] stops at a [`Scene`]: shapes with coordinates in points and
//! labels that name an [`Ink`] rather than a colour. This module is the app's half
//! of that contract. It measures the scene's text through [`FontEngine`] -- the
//! same shaper the prose goes through, so a box reserves the width its ink will
//! take -- caches the result the way [`crate::math::MathStore`] caches formulas,
//! and resolves every ink to a [`ColorRole`] so the light and dark palettes both
//! answer for a diagram.
//!
//! The scene is stored at its natural size. Measuring it again for a narrower
//! window would cost a layout per resize, so a diagram wider than the measure is
//! scaled down at paint time instead -- its type shrinks with it, which is what a
//! diagram that cannot fit has to do.

use rubrica_doc::{BlockKind, InlineStyle};
use rubrica_mermaid::{
    Diagram, Ink, PathCmd, Point, Scene, ShapePath, Stroke, TextMetrics, TextRole,
};
use rubrica_type::units::Pt;
use windows::Win32::Graphics::Direct2D::Common::{
    D2D1_BEZIER_SEGMENT, D2D1_FIGURE_BEGIN_FILLED, D2D1_FIGURE_BEGIN_HOLLOW,
    D2D1_FIGURE_END_CLOSED, D2D1_FIGURE_END_OPEN, ID2D1SimplifiedGeometrySink,
};
use windows::Win32::Graphics::Direct2D::{
    ID2D1Brush, ID2D1Factory, ID2D1PathGeometry, ID2D1RenderTarget, ID2D1SolidColorBrush,
    ID2D1StrokeStyle,
};
use windows_numerics::Vector2;

use crate::font::{FaceRequest, FontEngine};
use crate::theme::{ColorRole, Theme};
use crate::view::{paint_run, PaintRun};

/// How tall one line of diagram text is when the face itself says nothing -- an
/// empty label, or a face whose metrics came back zero.
const LINE_ASCENT: f32 = 0.75;
const LINE_DESCENT: f32 = 0.25;

/// The base width of a diagram's outline, at `size`: `Stroke::{Thin,Normal,Heavy}`
/// multiply it. A tenth of an em is thin enough to read as an outline and heavy
/// enough to survive at small sizes.
pub(crate) fn stroke_base(size: Pt) -> f32 {
    (size * 0.10).max(0.6)
}

/// One scene shape, ready for the display list: the path in device-independent
/// pixels with the diagram's own offset and fit already folded in, each ink named
/// by the role the palette resolves.
#[derive(Clone, Debug)]
pub(crate) struct DiagramShape {
    pub path: ShapePath,
    pub fill: Option<ColorRole>,
    /// The outline's width in device-independent pixels and its ink.
    pub stroke: Option<(f32, ColorRole)>,
}

/// What one diagram is painted in, once its words have been shaped.
#[derive(Clone)]
pub(crate) struct DiagramInk {
    pub left: Pt,
    pub top: Pt,
    pub right: Pt,
    pub bottom: Pt,
    pub shapes: Vec<DiagramShape>,
    /// The labels, seated, in baseline order.
    pub runs: Vec<PaintRun>,
}

/// One typeset diagram.
pub(crate) struct Entry {
    key: (String, i32, FaceRequest),
    /// The scene at its natural size, in points.
    pub(crate) scene: Scene,
    /// The size the scene was measured at.
    pub(crate) size: Pt,
    /// The face every label is shaped with, before the role's weight is applied.
    pub(crate) face: FaceRequest,
}

#[derive(Default)]
pub struct DiagramStore {
    entries: Vec<Entry>,
}

impl DiagramStore {
    pub fn new() -> DiagramStore {
        DiagramStore::default()
    }

    /// Typeset `source` unless it is already typeset, and return its index.
    ///
    /// The key is the fence's own text rather than a document position, so the same
    /// diagram repeated across a document -- and across a relayout -- is measured
    /// once. The face is part of the key because a label's width belongs to it: a
    /// diagram set under a different profile is a different piece of work.
    pub(crate) fn intern(
        &mut self,
        font: &FontEngine,
        source: &str,
        diagram: &Diagram,
        size: Pt,
        face: &FaceRequest,
    ) -> usize {
        let key = (source.to_string(), (size * 64.0).round() as i32, face.clone());
        if let Some(i) = self.entries.iter().position(|e| e.key == key) {
            return i;
        }
        let scene = {
            let mut measure = |text: &str, size: f32, role: TextRole, italic: bool| -> TextMetrics {
                measure_line(font, &label_request(face, role, italic), text, size)
            };
            rubrica_mermaid::layout(diagram, size, &mut measure)
        };
        self.entries.push(Entry { key, scene, size, face: face.clone() });
        self.entries.len() - 1
    }

    pub(crate) fn get(&self, index: usize) -> Option<&Entry> {
        self.entries.get(index)
    }

    /// One row per diagram: what it declares and what it came out as.
    pub(crate) fn rows(&self) -> Vec<Row> {
        self.entries
            .iter()
            .map(|e| Row {
                kind: e.scene.kind,
                elements: e.scene.elements,
                edges: e.scene.edges,
                shapes: e.scene.shapes.len(),
                labels: e.scene.labels.len(),
                width: e.scene.width,
                height: e.scene.height,
            })
            .collect()
    }

    /// Every label with the role, size and face it is drawn at, for the report's
    /// glyph census: a diagram's words are not `Op::Runs`, so the page's own census
    /// would otherwise be silent about them.
    pub(crate) fn each_label(&self, mut f: impl FnMut(&str, TextRole, f32, bool, &FaceRequest)) {
        for entry in &self.entries {
            for label in &entry.scene.labels {
                f(&label.text, label.role, label.size, label.italic, &entry.face);
            }
        }
    }

    /// Every shape and label, at the coordinates it is drawn at. `--shapes` prints
    /// this, because a summary cannot tell a box that wrapped from one that was
    /// placed a line too low.
    pub(crate) fn dump(&self, font: &FontEngine) {
        for (i, entry) in self.entries.iter().enumerate() {
            let scene = &entry.scene;
            println!(
                "  [{i}] {}  {} element(s), {} link(s)  scene {:.1} x {:.1}pt at {:.1}pt  {} shape(s), {} label(s)",
                scene.kind,
                scene.elements,
                scene.edges,
                scene.width,
                scene.height,
                entry.size,
                scene.shapes.len(),
                scene.labels.len(),
            );
            for (n, shape) in scene.shapes.iter().enumerate() {
                let fill = shape.fill.map_or_else(|| "none".to_string(), |f| format!("{f:?}"));
                let stroke = match shape.stroke {
                    Stroke::None => "none".to_string(),
                    s => format!("{:?}({:.2})", s, s.width(stroke_base(entry.size))),
                };
                let mut points = shape.path.points();
                let first = points.next().unwrap_or(Point::new(0.0, 0.0));
                let mut curves = 0;
                let mut moves = 0;
                for cmd in &shape.path.cmds {
                    match cmd {
                        PathCmd::CubicTo(..) => curves += 1,
                        PathCmd::MoveTo(_) => moves += 1,
                        PathCmd::LineTo(_) => {}
                    }
                }
                println!(
                    "      shape {n:>3}  {} from ({:.1}, {:.1})  {} cmd(s), {} curve(s), {} inner figure(s)  fill {fill}  stroke {stroke}",
                    if shape.path.closed { "closed" } else { "open  " },
                    first.x,
                    first.y,
                    shape.path.cmds.len(),
                    curves,
                    moves,
                );
            }
            for (n, label) in scene.labels.iter().enumerate() {
                let req = label_request(&entry.face, label.role, label.italic);
                let glyphs = font.shape_runs(&label.text, 0..label.text.len(), &req, label.size, 0.0);
                let missing: usize = glyphs.iter().flat_map(|r| r.glyphs.iter()).filter(|g| **g == 0).count();
                println!(
                    "      label {n:>3}  {:?} {:?} {:.2}pt  left {:>7.2} baseline {:>7.2}  ink {:?}  {}{}{:?}",
                    label.role,
                    label.align,
                    label.size,
                    label.left,
                    label.baseline,
                    label.ink,
                    if missing > 0 { format!("!! {missing} missing glyph(s) ") } else { String::new() },
                    if label.italic { "italic " } else { "" },
                    label.text,
                );
            }
        }
    }
    /// Every part of a scene that falls outside the scene's own box.
    ///
    /// A diagram is scaled to the column and its box is the room reserved for it, so a
    /// shape that reaches beyond the box is painted over whatever stands beside it --
    /// a margin, or the block below -- and nothing on the page says so. The layout's
    /// arithmetic should never produce one, which is exactly why it is worth counting:
    /// it is the difference between a diagram that was scaled to fit and one that only
    /// looks like it was.
    pub(crate) fn outside(&self) -> Vec<String> {
        let mut out = Vec::new();
        for (i, entry) in self.entries.iter().enumerate() {
            let scene = &entry.scene;
            let beyond = |x: f32, y: f32| {
                x < -0.5 || y < -0.5 || x > scene.width + 0.5 || y > scene.height + 0.5
            };
            for (n, shape) in scene.shapes.iter().enumerate() {
                if let Some(p) = shape.path.points().find(|p| beyond(p.x, p.y)) {
                    out.push(format!(
                        "[{i}] shape {n} reaches ({:.1}, {:.1}) outside {:.1} x {:.1}pt",
                        p.x, p.y, scene.width, scene.height
                    ));
                }
            }
            for (n, label) in scene.labels.iter().enumerate() {
                if beyond(label.left, label.baseline) || beyond(label.right, label.baseline) {
                    out.push(format!(
                        "[{i}] label {n} spans {:.1}..{:.1} on baseline {:.1}, outside {:.1} x {:.1}pt  {:?}",
                        label.left,
                        label.right,
                        label.baseline,
                        scene.width,
                        scene.height,
                        label.text
                    ));
                }
            }
        }
        out
    }
}

/// One diagram as the report prints it.
pub(crate) struct Row {
    pub(crate) kind: &'static str,
    pub(crate) elements: usize,
    pub(crate) edges: usize,
    pub(crate) shapes: usize,
    pub(crate) labels: usize,
    pub(crate) width: f32,
    pub(crate) height: f32,
}

/// The document's body face and size, which is what a diagram's labels are set in.
///
/// The page's own face rather than a face of the diagram's choosing: a diagram is
/// part of the document, and a figure that changed typeface at the fence would read
/// as pasted in. `size` overrides the body size where the fence stands outside the
/// page's prose -- inside a note, whose blocks are set smaller than the page's.
pub(crate) fn body_face(theme: &Theme, size: Option<Pt>) -> (FaceRequest, Pt) {
    let body = match size {
        Some(size) => theme.resolve_at(BlockKind::Paragraph, InlineStyle::EMPTY, size),
        None => theme.resolve(BlockKind::Paragraph, InlineStyle::EMPTY),
    };
    let req = FaceRequest {
        family: body.family,
        cjk_family: body.cjk_family,
        japanese_family: body.japanese_family,
        korean_family: body.korean_family,
        cjk_italic: body.cjk_italic,
        fallback: theme.fonts.fallback.clone(),
        weight: body.weight,
        italic: body.italic,
    };
    (req, body.size)
}

/// The request one part of a diagram is shaped with. The role decides the weight,
/// and both the measuring pass and the drawing pass ask this, so a box's reserved
/// width and the ink inside it cannot disagree.
pub(crate) fn role_request(base: &FaceRequest, role: TextRole) -> FaceRequest {
    let mut req = base.clone();
    req.weight = match role {
        TextRole::Title => req.weight.max(700),
        TextRole::Section => req.weight.max(600),
        TextRole::Node | TextRole::Edge => req.weight,
    };
    req
}

/// The request one *label* is shaped with: its role's weight, and the italic a
/// `##` description asks for. The census, the `--shapes` dump and the painter all
/// go through this, so what the report measures is what the page draws.
pub(crate) fn label_request(base: &FaceRequest, role: TextRole, italic: bool) -> FaceRequest {
    let mut req = role_request(base, role);
    req.italic |= italic;
    req
}

/// One thing's ink, resolved to the theme's roles. Roles rather than colours so a
/// diagram is drawn from the palette in front of the reader: the same scene comes
/// out on a light page and a dark one.
fn ink_role(ink: Ink) -> ColorRole {
    /// The eight inks a pie's slices cycle through, spread across the palette's
    /// hues so neighbouring slices are told apart in both themes.
    const SLICES: [ColorRole; 8] = [
        ColorRole::Accent,
        ColorRole::String,
        ColorRole::Type,
        ColorRole::Keyword,
        ColorRole::Number,
        ColorRole::Comment,
        ColorRole::Muted,
        ColorRole::Faint,
    ];
    match ink {
        Ink::Stroke | Ink::Arrow => ColorRole::Muted,
        Ink::Text | Ink::Fill => ColorRole::Text,
        // A pill behind words that sit on something else, and the words that must
        // be legible over a slice: in the light theme both are white, in the dark
        // theme both are near-black, which is what each of them needs.
        Ink::Backdrop => ColorRole::OnAccent,
        // A `:::highlight` row and a `##` description: the theme's own accent and
        // muted inks, which the palette answers for both dark and light pages.
        Ink::Accent => ColorRole::Accent,
        Ink::Muted => ColorRole::Muted,
        Ink::Slice(i) => SLICES[i as usize % SLICES.len()],
    }
}

/// Measure one line of diagram text: the advance the layout reserves and the
/// vertical box it centres the line in, both at `size` points.
fn measure_line(font: &FontEngine, req: &FaceRequest, text: &str, size: f32) -> TextMetrics {
    if text.is_empty() {
        return TextMetrics {
            advance: 0.0,
            ascent: size * LINE_ASCENT,
            descent: size * LINE_DESCENT,
        };
    }
    let runs = font.shape_runs(text, 0..text.len(), req, size, 0.0);
    let advance: Pt = runs.iter().map(|r| r.width()).sum();
    let mut ascent = 0.0f32;
    let mut descent = 0.0f32;
    for r in &runs {
        ascent = ascent.max(r.ascent);
        descent = descent.max(r.descent);
    }
    TextMetrics {
        advance,
        ascent: ascent.max(size * LINE_ASCENT),
        descent: descent.max(size * LINE_DESCENT),
    }
}

/// One scene path in the display list's own space: scaled by the fit, moved to the
/// diagram's place on the page, and multiplied into device-independent pixels.
fn scene_path(path: &ShapePath, left: Pt, top: Pt, fit: f32, k: f32) -> ShapePath {
    let at = |p: Point| Point::new((left + p.x * fit) * k, (top + p.y * fit) * k);
    ShapePath {
        start: at(path.start),
        closed: path.closed,
        cmds: path
            .cmds
            .iter()
            .map(|cmd| match cmd {
                PathCmd::MoveTo(p) => PathCmd::MoveTo(at(*p)),
                PathCmd::LineTo(p) => PathCmd::LineTo(at(*p)),
                PathCmd::CubicTo(a, b, c) => PathCmd::CubicTo(at(*a), at(*b), at(*c)),
            })
            .collect(),
    }
}

/// Ready one diagram for the display list: every shape and label at `left`/`top`
/// scaled by `fit`, in the units the painter wants.
pub(crate) fn paint(
    font: &FontEngine,
    entry: &Entry,
    left: Pt,
    top: Pt,
    column: Pt,
    k: f32,
) -> DiagramInk {
    let scene = &entry.scene;
    let fit = if scene.width > 0.0 { (column / scene.width).min(1.0) } else { 1.0 };
    let stroke = stroke_base(entry.size);
    let shapes = scene
        .shapes
        .iter()
        .map(|shape| DiagramShape {
            path: scene_path(&shape.path, left, top, fit, k),
            fill: shape.fill.map(ink_role),
            stroke: (shape.stroke != Stroke::None)
                .then(|| (shape.stroke.width(stroke) * fit * k, ink_role(shape.ink))),
        })
        .collect();
    let mut runs: Vec<(f32, PaintRun)> = Vec::with_capacity(scene.labels.len());
    for label in &scene.labels {
        let req = label_request(&entry.face, label.role, label.italic);
        let size = label.size * fit;
        if size <= 0.0 {
            continue;
        }
        let mut at = left + label.left * fit;
        for r in font.shape_runs(&label.text, 0..label.text.len(), &req, size, 0.0) {
            if let Some(mut run) = paint_run(font, &r, at, 0.0, k, ink_role(label.ink), Some(&label.text)) {
                at += r.width();
                run.baseline = (top + label.baseline * fit) * k;
                runs.push((run.baseline, run));
            }
        }
    }
    // The display list's band search reads the first run's baseline as the op's top,
    // so the labels leave in the order their baselines ask for. The sort is stable,
    // which keeps the lines of one multi-line label in the order they are read.
    runs.sort_by(|a, b| a.0.total_cmp(&b.0));
    DiagramInk {
        left: left * k,
        top: top * k,
        right: (left + scene.width * fit) * k,
        bottom: (top + scene.height * fit) * k,
        shapes,
        runs: runs.into_iter().map(|(_, run)| run).collect(),
    }
}

/// One scene path as a fillable D2D geometry, moved by `dx`/`dy`.
///
/// The first figure opens `FILLED` when the shape asks for a fill, so a fill and
/// its outline cover the same vertices; a figure a `MoveTo` opens inside the path
/// is the shape's own inner line (a subroutine's bar, a cylinder's cap) and stays
/// `HOLLOW`, because filling it would close across the figure it is drawn in.
///
/// The side is only a hint: D2D fills what the path closes, so a shape with no
/// stroke still fills and a shape with no fill is never filled at all.
unsafe fn diagram_geometry(
    factory: &ID2D1Factory,
    path: &ShapePath,
    filled: bool,
    dx: f32,
    dy: f32,
) -> Option<ID2D1PathGeometry> {
    let at = |p: Point| Vector2::new(p.x + dx, p.y + dy);
    let geometry = unsafe { factory.CreatePathGeometry() }.ok()?;
    let sink = unsafe { geometry.Open() }.ok()?;
    let base: &ID2D1SimplifiedGeometrySink = &sink;
    unsafe {
        base.BeginFigure(
            at(path.start),
            if filled { D2D1_FIGURE_BEGIN_FILLED } else { D2D1_FIGURE_BEGIN_HOLLOW },
        );
        for cmd in &path.cmds {
            match cmd {
                PathCmd::MoveTo(p) => {
                    base.EndFigure(D2D1_FIGURE_END_OPEN);
                    base.BeginFigure(at(*p), D2D1_FIGURE_BEGIN_HOLLOW);
                }
                PathCmd::LineTo(p) => sink.AddLine(at(*p)),
                PathCmd::CubicTo(a, b, c) => sink.AddBezier(&D2D1_BEZIER_SEGMENT {
                    point1: at(*a),
                    point2: at(*b),
                    point3: at(*c),
                }),
            }
        }
        base.EndFigure(if path.closed { D2D1_FIGURE_END_CLOSED } else { D2D1_FIGURE_END_OPEN });
        let _ = base.Close();
    }
    Some(geometry)
}

/// Paint a diagram's outlines at the pane's offset: one geometry per shape, filled
/// and stroked by the brushes its roles resolve to.
///
/// A shape with no brush for a role paints nothing rather than everything: the
/// window builds a brush per role once, and a missing one means the palette never
/// named that role at all, which is a reason to leave the shape out, not to draw it
/// in the wrong ink.
pub(crate) unsafe fn paint_diagram_shapes(
    target: &ID2D1RenderTarget,
    factory: &ID2D1Factory,
    shapes: &[DiagramShape],
    dx: f32,
    dy: f32,
    brush: &mut impl FnMut(ColorRole) -> Option<ID2D1SolidColorBrush>,
) {
    for shape in shapes {
        let Some(geometry) = (unsafe { diagram_geometry(factory, &shape.path, shape.fill.is_some(), dx, dy) })
        else {
            continue;
        };
        if let Some(role) = shape.fill {
            if let Some(b) = brush(role) {
                unsafe { target.FillGeometry(&geometry, &b, None::<&ID2D1Brush>) };
            }
        }
        if let Some((width, role)) = shape.stroke {
            if let Some(b) = brush(role) {
                unsafe { target.DrawGeometry(&geometry, &b, width, None::<&ID2D1StrokeStyle>) };
            }
        }
    }
}
