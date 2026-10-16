//! Rubrica's Mermaid engine: diagram source in, a positioned *scene* out.
//!
//! Three stages, each independently testable and none of them aware of Windows, a
//! font, or a colour:
//!
//! - [`parse`] turns the source between a ```` ```mermaid ```` fence into a
//!   [`Diagram`] -- a graph, a sequence, a pie, a mind-map. Anything it cannot read
//!   comes back as a [`ParseError`] with a line number and a sentence, which is what
//!   the reader is shown when a diagram degrades to the code it was written as.
//! - [`layout`] folds a diagram and a text measurement into a [`Scene`]: boxes with
//!   their outlines, edges with their arrow heads, nothing measured to a pixel. The
//!   measurement is a closure rather than a font object, so the same scene can be
//!   built in a test with a fixed-width mock and in the reader with DirectWrite.
//! - the [`Scene`] itself: paths, strokes, fills and *text with a role*. A text item
//!   names [`Ink`] and a size in ems of the diagram's own scale; which face draws it
//!   and which colour that face uses is the caller's decision, because a theme is a
//!   property of the reader rather than of the diagram.
//!
//! The crate deliberately stops at the scene. Turning it into pixels is the app's
//! job, which keeps the whole engine runnable -- and its output assertable -- in a
//! test binary with no display attached, exactly as [`rubrica_math`] does for
//! formulas.

pub mod layout;
pub mod parse;

pub use layout::{
    kind_label, layout, Alignment, DiagramMeasure, Ink, Label, PathCmd, Point, Scene, Shape,
    ShapePath, Stroke, TextMetrics, TextRole, MAX_DIAGRAM_TEXT, MAX_LABEL_LINES,
};
pub use parse::{
    parse, Diagram, DiagramKind, Direction, Edge, Fragment, Group, Head, LineStyle, Message, Node,
    NodeShape, Note, ParseError, Slice, TreeItem, MAX_EDGES, MAX_ELEMENTS, MAX_LABEL, MAX_SOURCE,
};
