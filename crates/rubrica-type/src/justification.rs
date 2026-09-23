//! Justification: distribute a line's slack across its glue.
//!
//! Which glue is elastic is the whole subject. An English line stretches only at
//! word spaces; a Chinese line has no word spaces, so it stretches at the thin
//! join between ideographs -- which is why the join must be *stretchable glue* and
//! not a fixed gap, and why it may not shrink at all: there is no air there to
//! take back. Because every ideograph pair carries the same recipe,
//! proportional distribution spreads the slack evenly on its own.

use crate::breaking::Line;
use crate::paragraph::{Item, Paragraph};
use crate::units::{EPSILON, INFINITY, Pt};

/// A positioned slot: a box at `x` with width `w`, or glue (`node == None`).
#[derive(Clone, Copy, Debug)]
pub struct Placed {
    pub x: Pt,
    pub w: Pt,
    pub node: Option<u32>,
    /// Logical UTF-8 source range, including real spaces, set by `place_bidi`.
    pub source: Option<(usize, usize)>,
    /// Resolved UBA level for this slot; odd levels read right to left.
    pub bidi_level: u8,
}

/// Lay out one line at its target width. Ragged lines get natural widths -- unless
/// their natural width is already past the measure, which no amount of stretch
/// licence covers.
pub fn place(para: &Paragraph, line: &Line) -> Vec<Placed> {
    let items = &para.items;
    let delta = f64::from(line.target) - f64::from(line.natural);
    // Infinite stretch makes *leftover* space free -- that is what keeps a final line
    // ragged -- but it absorbs no ink. A line whose own width already passes the
    // measure has to be pulled back by shrinking, `\parfillskip` included: TeX
    // excuses the last line from being underfull, never from being overfull.
    let slack = if line.ragged {
        0.0
    } else if line.stretch >= INFINITY / 2.0 {
        delta.min(0.0)
    } else {
        delta
    };
    let eps = f64::from(EPSILON);

    let mut total_stretch = 0.0;
    let mut total_shrink = 0.0;
    for i in line.items.clone() {
        if let Item::Glue { stretch, shrink, .. } = items[i] {
            if slack > 0.0 {
                total_stretch += f64::from(stretch);
            } else {
                total_shrink += f64::from(shrink);
            }
        }
    }

    let mut out = Vec::with_capacity(line.items.len() + 1);
    let mut x = 0.0f64;
    for i in line.items.clone() {
        match items[i] {
            Item::Box { node } => {
                let w = f64::from(para.node(node).advance);
                out.push(Placed { x: x as Pt, w: w as Pt, node: Some(node), source: None, bidi_level: 0 });
                x += w;
            }
            Item::Glue { base, stretch, shrink, .. } => {
                let mut w = f64::from(base);
                if slack.abs() > eps {
                    let (avail, total) = if slack > 0.0 {
                        (f64::from(stretch), total_stretch)
                    } else {
                        (f64::from(shrink), total_shrink)
                    };
                    if total > eps {
                        // Stretch is unbounded: TeX lets a line go past nominal, which
                        // is precisely what a "lousy" fitness class records, and
                        // capping here would silently stop short of the measure.
                        // Shrink is not. A join that gives back more than it has would
                        // slide its glyphs on top of each other, so a starved line
                        // takes what the glue can spare and hangs past the measure --
                        // the overfull box TeX reports rather than overlaps.
                        let give = if slack > 0.0 { slack } else { slack.max(-total) };
                        w += give * (avail / total);
                    }
                }
                out.push(Placed { x: x as Pt, w: w as Pt, node: None, source: None, bidi_level: 0 });
                x += w;
            }
            Item::Penalty { .. } => {
                // An untaken discretionary break contributes nothing at all: the
                // solver charged its width only to the line that actually broke on
                // it, via `Line::hyphen`. Charging it here as well would make every
                // unused dictionary point inflate its line by a hyphen width.
            }
        }
    }
    // A line that broke on a hyphen has already had its width charged for the
    // glyph by the solver; here it is given a slot so the painter draws it.
    if let Some(h) = line.hyphen {
        let node = para.node(h);
        out.push(Placed { x: x as Pt, w: f64::from(node.advance) as Pt, node: Some(h), source: None, bidi_level: 0 });
    }
    out
}

/// Right edge of a placed line, for assertions and for hanging punctuation.
pub fn line_width(placed: &[Placed]) -> Pt {
    placed.last().map(|p| p.x + p.w).unwrap_or(0.0)
}

/// Apply UAX #9 L1/L2 after logical line breaking and glue trimming. Return slots
/// in visual order, with left-edge coordinates; glyph direction is still per run.
/// Keeping the analysis outside this function lets all lines share one UBA pass.
pub fn place_bidi(para: &Paragraph, line: &Line, bidi: &crate::BidiInfo<'_>) -> Vec<Placed> {
    let mut slots = place(para, line);
    let ranges: Vec<_> = slots.iter().filter_map(|s| s.node)
        .map(|n| para.node(n).text.clone()).filter(|r| !r.is_empty()).collect();
    let Some(first) = ranges.first() else { return slots };
    let end = ranges.last().unwrap().end;
    let Some(p) = bidi.paragraphs.iter().find(|p| p.range.contains(&first.start)) else {
        return slots;
    };
    let levels = bidi.reordered_levels(p, first.start..end);
    let mut cursor = first.start;
    let mut slot_levels = Vec::new();
    for i in 0..slots.len() {
        let level = if let Some(n) = slots[i].node {
            let r = &para.node(n).text;
            cursor = r.end;
            slots[i].source = Some((r.start, r.end));
            let at = if r.is_empty() { r.start.saturating_sub(1) } else { r.start };
            levels.get(at).copied().unwrap_or(p.level)
        } else {
            let next = slots[i + 1..].iter().filter_map(|s| s.node)
                .map(|n| para.node(n).text.start).next().unwrap_or(cursor);
            slots[i].source = Some((cursor, next));
            let level = if cursor < next { levels[cursor] } else {
                slot_levels.last().copied().unwrap_or(p.level)
            };
            cursor = next;
            level
        };
        slots[i].bidi_level = level.number();
        slot_levels.push(level);
    }
    let width: Pt = slots.iter().map(|s| s.w).sum();
    let mut x = if p.level.is_rtl() { (line.target - width).max(0.0) } else { 0.0 };
    crate::BidiInfo::reorder_visual(&slot_levels).into_iter().map(|i| {
        let mut slot = slots[i];
        slot.x = x;
        x += slot.w;
        slot
    }).collect()
}
