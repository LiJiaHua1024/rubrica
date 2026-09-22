//! Justification: distribute a line's slack across its glue.
//!
//! Which glue is elastic is the whole subject. An English line stretches only at
//! word spaces; a Chinese line has no word spaces, so it stretches at the thin
//! join between ideographs -- which is why the join must be *compressible glue*
//! and not a fixed gap. Because every ideograph pair carries the same recipe,
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
                out.push(Placed { x: x as Pt, w: w as Pt, node: Some(node) });
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
                        // Proportional to each glue's own elasticity, and unbounded:
                        // TeX lets a line stretch past nominal, which is precisely
                        // what a "lousy" fitness class records -- capping here would
                        // silently stop short of the measure instead.
                        w += slack * (avail / total);
                    }
                }
                out.push(Placed { x: x as Pt, w: w as Pt, node: None });
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
        out.push(Placed { x: x as Pt, w: f64::from(node.advance) as Pt, node: Some(h) });
    }
    out
}

/// Right edge of a placed line, for assertions and for hanging punctuation.
pub fn line_width(placed: &[Placed]) -> Pt {
    placed.last().map(|p| p.x + p.w).unwrap_or(0.0)
}
