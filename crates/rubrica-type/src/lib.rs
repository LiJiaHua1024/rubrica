//! Rubrica's headless typesetting core.
//!
//! Deliberately free of any OS or rendering dependency: given text, styles and
//! advance widths it produces line break plans. Every quantity is in
//! [units::Pt] (PostScript points, 1/72").

pub mod breaking;
pub mod classify;
pub mod justification;
pub mod paragraph;
pub mod units;

pub use breaking::{BreakOptions, Line, Plan};
pub use justification::{Placed, place};
pub use paragraph::{Item, Node, Paragraph, Spacing, StyleId};
pub use units::Pt;

/// Segment, measure and break a paragraph of text in one call.
///
/// The convenience that keeps callers from re-deriving UAX #14 opportunities: the
/// renderer and every test go through here.
pub fn typeset(
    text: &str,
    spacing: &Spacing,
    style: StyleId,
    spans: &[paragraph::StyleSpan],
    opts: &BreakOptions,
    measure: &mut dyn paragraph::Measure,
) -> (Paragraph, Plan) {
    let para = paragraph::paragraph_from_text(text, spacing, style, spans, measure);
    let plan = breaking::break_paragraph(&para, opts);
    (para, plan)
}
