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
pub use unicode_bidi::{BidiInfo, Level};

pub use breaking::{BreakOptions, Line, Plan};
pub use justification::{LinePlacer, Placed, place};
pub use paragraph::{Hyphenation, Item, Node, Paragraph, Spacing, StyleId};
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
    typeset_hyphenated(text, spacing, style, spans, &Hyphenation::NONE, opts, measure)
}

/// As [`typeset`], additionally offering the given [`Hyphenation`] as discretionary
/// hyphen breaks: the byte offsets come from a dictionary the caller has already
/// consulted, and the hyphen glyph's advance from a shaping backend.
pub fn typeset_hyphenated(
    text: &str,
    spacing: &Spacing,
    style: StyleId,
    spans: &[paragraph::StyleSpan],
    hyphenation: &Hyphenation<'_>,
    opts: &BreakOptions,
    measure: &mut dyn paragraph::Measure,
) -> (Paragraph, Plan) {
    let para = paragraph::paragraph_from_text_hyphenated(
        text, spacing, style, spans, hyphenation, measure,
    );
    let plan = breaking::break_paragraph(&para, opts);
    (para, plan)
}
