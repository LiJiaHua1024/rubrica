//! Knuth-Plass global line breaking.
//!
//! DirectWrite, like every browser, breaks lines greedily: fill one line, accept
//! whatever raggedness falls out, move on. This module scores every legal set of
//! breaks for a paragraph at once and keeps the cheapest. That is the difference
//! between "acceptable" and "even" justification -- and for Chinese, where the
//! only elastic material is the thin compressible space between ideographs, it is
//! the difference between justifying at all and not.
//!
//! The scoring model is TeX's: cubic badness, four fitness classes with
//! class-jump penalties, and a pretolerance/tolerance pair so the cheap pass runs
//! first. The constants live on [BreakOptions] rather than in the code because a
//! typography app has to expose them, not freeze them.

use crate::paragraph::{Item, Paragraph};
use crate::units::{EPSILON, INFINITY, Pt};
use std::ops::Range;

#[derive(Clone, Copy, Debug)]
pub struct BreakOptions {
    /// Measure of the text column, minus anything the caller already took.
    pub column_width: Pt,
    /// First-line indent; later lines flush to `column_width`.
    pub par_indent: Pt,
    /// Max badness on the first pass (TeX's `\pretolerance`).
    pub pretolerance: i32,
    /// Max badness on the second pass (TeX's `\tolerance`).
    pub tolerance: i32,
    /// Stretch granted to every line on the second pass, tried before conceding
    /// an overfull line (TeX's `\emergencystretch`).
    pub emergencystretch: Pt,
    /// Penalties for jumping into a worse fitness class.
    pub lousy_demerits: i32,
    pub awful_demerits: i32,
    pub nasty_demerits: i32,
    /// Set the whole block ragged-right instead of justified. Headings and code
    /// must never be stretched to the measure -- doing so is a typographic error --
    /// and over at TeX this is `\raggedright`'s infinite `\rightskip`.
    pub ragged: bool,
}

impl BreakOptions {
    pub fn new(column_width: Pt) -> Self {
        Self {
            column_width,
            par_indent: 0.0,
            pretolerance: 100,
            tolerance: 200,
            // TeX leaves this at zero because hyphenation keeps bad breaks rare; an
            // engine without them needs a real budget or every awkward paragraph
            // falls through to the tolerance-free pass and stops reporting badness.
            emergencystretch: column_width * 0.03,
            lousy_demerits: 100,
            awful_demerits: 1000,
            nasty_demerits: 1000,
            ragged: false,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Line {
    /// Half-open item range of content. Glue at either end is already trimmed: a
    /// line never begins or ends with glue, per TeX.
    pub items: Range<usize>,
    pub natural: Pt,
    pub stretch: Pt,
    pub shrink: Pt,
    /// Width this line is set to; `column_width` unless it is first or last.
    pub target: Pt,
    pub badness: i32,
    pub fitness: u8,
    /// Ended on a hard break (`\n`), so the next line starts a new visual block.
    pub forced: bool,
    /// First line of the paragraph, which was indented.
    pub first: bool,
    /// The block opted out of justification entirely.
    pub ragged: bool,
}

impl Line {
    /// The final line carries `\parfillskip`'s infinite stretch and stays ragged,
    /// as does every line of an explicitly ragged block.
    #[inline]
    pub fn is_ragged(&self) -> bool {
        self.ragged || self.stretch >= INFINITY / 2.0
    }
}

#[derive(Clone, Debug)]
pub struct Plan {
    pub lines: Vec<Line>,
    pub demerits: f64,
    /// Which pass produced this plan, 1-based. Pass 3 means the paragraph could
    /// not be set without an overfull line, which the caller may want to surface.
    pub pass: u8,
}

#[derive(Default, Clone, Copy)]
struct Edge {
    prev: usize,
    item: usize,
    /// First content item of a line that would start at this edge.
    start: usize,
    cost: f64,
    width: Pt,
    stretch: Pt,
    shrink: Pt,
    fitness: u8,
    badness: i32,
    lo: usize,
    hi: usize,
}

fn badness(ratio: Pt) -> i32 {
    if ratio <= 0.0 {
        0
    } else if ratio >= 10.0 {
        10000
    } else {
        (100.0 * ratio * ratio * ratio).min(10000.0) as i32
    }
}

fn fitness(ratio: Pt) -> u8 {
    let a = ratio.abs();
    if a <= 1.0 {
        0
    } else if a <= 2.0 {
        1
    } else if a <= 3.0 {
        2
    } else {
        3
    }
}

struct Sums {
    w: Vec<f64>,
    s: Vec<f64>,
    k: Vec<f64>,
}

fn prefix(para: &Paragraph) -> Sums {
    let n = para.items.len();
    let mut s = Sums { w: vec![0.0; n + 1], s: vec![0.0; n + 1], k: vec![0.0; n + 1] };
    for (i, it) in para.items.iter().enumerate() {
        let (w, st, sh) = match *it {
            Item::Box { node } => (f64::from(para.node(node).advance), 0.0, 0.0),
            Item::Glue { base, stretch, shrink, .. } => {
                (f64::from(base), f64::from(stretch), f64::from(shrink))
            }
            Item::Penalty { .. } => (0.0, 0.0, 0.0),
        };
        s.w[i + 1] = s.w[i] + w;
        s.s[i + 1] = s.s[i] + st;
        s.k[i + 1] = s.k[i] + sh;
    }
    s
}

/// Content range of a line whose first content item is `start` and which breaks
/// at item `to`.
///
/// Leading and trailing glue is trimmed -- a line must not hang space off either
/// edge -- except for glue with infinite stretch, which is `\parfillskip`: it has
/// to stay inside the final line, because it *is* that line's stretchability and
/// therefore what keeps the last line ragged rather than justified.
fn trim(
    items: &[Item],
    sums: &Sums,
    start: usize,
    to: usize,
    penalty_width: Pt,
) -> (Range<usize>, Pt, Pt, Pt) {
    let ragged_glue = |i: usize| matches!(items[i], Item::Glue { stretch, .. } if stretch >= INFINITY / 2.0);
    let mut lo = start;
    while lo < items.len() && items[lo].is_glue() && !ragged_glue(lo) {
        lo += 1;
    }
    let mut hi = to;
    while hi > lo && items[hi - 1].is_glue() && !ragged_glue(hi - 1) {
        hi -= 1;
    }
    let natural = (sums.w[hi] - sums.w[lo]) + f64::from(penalty_width);
    (lo..hi, natural as Pt, (sums.s[hi] - sums.s[lo]) as Pt, (sums.k[hi] - sums.k[lo]) as Pt)
}

/// Score a candidate line. `None` when the break is illegal at this tolerance.
fn score(
    natural: Pt,
    stretch: Pt,
    shrink: Pt,
    target: Pt,
    tolerance: i32,
    prev_fit: u8,
    opts: &BreakOptions,
) -> Option<(i32, u8, f64)> {
    let delta = f64::from(target) - f64::from(natural);
    let st = f64::from(stretch);
    let sh = f64::from(shrink);
    let eps = f64::from(EPSILON);
    let (ratio, bad) = if delta > eps {
        if st <= eps {
            (10.0, 10000)
        } else {
            let r = (delta / st) as Pt;
            (r, badness(r))
        }
    } else if delta < -eps {
        let r = if sh <= eps { 10.0 } else { (-delta / sh) as Pt };
        (r, badness(r))
    } else {
        (0.0, 0)
    };

    // Overfull (shrink-starved) lines are legal but cost enormously -- that is how
    // TeX reports them instead of dropping text. Underfull ones beyond tolerance
    // are simply not allowed, except in a ragged block, where leftover space is
    // free and the widest-edge-wins tie-break fills lines greedily.
    if delta > 0.0 && bad > tolerance && !(opts.ragged && bad < 10000) {
        return None;
    }
    let fit = fitness(ratio);
    let mut adj = bad + 100;
    if bad >= 10000 {
        adj = 100_000;
    }
    if prev_fit < 1 && fit >= 1 {
        adj += opts.lousy_demerits;
    } else if prev_fit < 2 && fit >= 2 {
        adj += opts.awful_demerits;
    } else if prev_fit >= 2 && fit >= 2 {
        adj += opts.nasty_demerits;
    }
    Some((bad, fit, (f64::from(adj.max(0)) / 100.0).powi(2)))
}

/// Break one paragraph into lines.
pub fn break_paragraph(para: &Paragraph, opts: &BreakOptions) -> Plan {
    let items = &para.items;
    if items.is_empty() {
        return Plan { lines: vec![], demerits: 0.0, pass: 1 };
    }
    let sums = prefix(para);

    // Forced breaks split the paragraph; each piece is solved on its own, which
    // beats simulating `\break` with a gigantic penalty.
    let mut pieces: Vec<(usize, usize)> = Vec::new();
    let mut start = 0usize;
    for (i, it) in items.iter().enumerate() {
        if matches!(it, Item::Penalty { forced: true, .. }) {
            pieces.push((start, i));
            start = i + 1;
        }
    }
    if start < items.len() {
        pieces.push((start, items.len() - 1));
    } else if pieces.is_empty() {
        pieces.push((0, items.len() - 1));
    }

    let mut lines = Vec::new();
    let mut demerits = 0.0;
    let mut pass = 1u8;
    for (from, end) in pieces {
        // TeX can lean on `\pretolerance` alone because hyphenation shatters the
        // window of achievable line widths into something fine. Until discretionary
        // hyphenation lands we cannot assume that window is ever hit, so a third
        // pass with the tolerance removed guarantees the solver always has a
        // solution instead of dropping the paragraph into `desperate`.
        let attempts = [
            (opts.pretolerance, 0.0f32),
            (opts.tolerance, opts.emergencystretch),
            (i32::MAX, opts.emergencystretch),
        ];
        let mut result = None;
        for (n, (tol, extra)) in attempts.into_iter().enumerate() {
            if let Some(out) = solve(items, &sums, from, end, opts, tol, extra) {
                result = Some((out, n as u8 + 1));
                break;
            }
        }
        let ((mut out, cost), p) = match result {
            Some(((out, cost), p)) => ((out, cost), p),
            None => ((desperate(items, &sums, from, end, opts), f64::INFINITY), 3),
        };
        pass = pass.max(p);
        demerits += cost;
        lines.append(&mut out);
    }
    if pass < 3 && lines.iter().any(|l| l.badness >= 10000 && !l.is_ragged()) {
        pass = 3;
    }
    Plan { lines, demerits, pass }
}

/// Dynamic program over the break nodes of one piece. Returns the lines, or `None`
/// when no legal set of breaks exists at this tolerance.
fn solve(
    items: &[Item],
    sums: &Sums,
    from: usize,
    end: usize,
    opts: &BreakOptions,
    tolerance: i32,
    extra_stretch: Pt,
) -> Option<(Vec<Line>, f64)> {
    let first_line = from == 0;
    let mut edges: Vec<Edge> = vec![Edge { prev: usize::MAX, item: from, start: from, cost: 0.0, fitness: 0, ..Default::default() }];
    let mut active: Vec<usize> = vec![0];
    let mut at_start = vec![true];

    // Every legal break point inside the piece, in ascending order, ending with
    // the piece's terminal item.
    let mut nodes: Vec<usize> = Vec::new();
    for i in from + 1..=end {
        let legal = match items[i] {
            Item::Glue { breakable, stretch, .. } => breakable || stretch >= INFINITY / 2.0,
            Item::Penalty { .. } => true,
            Item::Box { .. } => false,
        };
        // `\parfillskip` followed by the closing break is one breakpoint, not two.
        // Keeping the glue node would let a line end *before* the stretch that is
        // supposed to absorb it, and then kill the only viable edge as overwide.
        let redundant = matches!(items[i], Item::Glue { .. })
            && matches!(items.get(i + 1), Some(Item::Penalty { forced: true, .. }));
        if legal && !redundant && i != end {
            nodes.push(i);
        }
    }
    if nodes.last() != Some(&end) {
        nodes.push(end);
    }

    for &k in &nodes {
        let mut best: Option<(f64, usize)> = None;
        let mut next_active: Vec<usize> = Vec::with_capacity(active.len() + 1);
        let mut stop = false;

        for &e in &active {
            if stop {
                next_active.push(e);
                continue;
            }
            let ed = edges[e];
            let pw = match items[k] {
                Item::Penalty { width, .. } => width,
                _ => 0.0,
            };
            let line_target = opts.column_width - if at_start[e] { opts.par_indent } else { 0.0 };
            let (range, natural, stretch, shrink) = trim(items, sums, ed.start, k, pw);
            if range.is_empty() {
                next_active.push(e);
                continue;
            }

            // Too wide even fully shrunk: this start can never work again, since
            // every later break only lengthens the line. Drop it for good -- but
            // not when the line holds `\parfillskip`, because the final line is
            // allowed to run past the measure and be reported overfull, which is
            // how an unsplittable word still gets set instead of dropped.
            let ragged = stretch >= INFINITY / 2.0;
            if !ragged
                && f64::from(natural) - f64::from(shrink) > f64::from(line_target) + f64::from(EPSILON)
            {
                continue;
            }
            let Some((_bad, _fit, demerits)) =
                score(natural, stretch + extra_stretch, shrink, line_target, tolerance, ed.fitness, opts)
            else {
                // Rejected because the line is too short; every remaining active
                // edge starts later and so makes an even shorter line.
                if f64::from(natural) < f64::from(line_target) {
                    stop = true;
                }
                next_active.push(e);
                continue;
            };
            let cost = ed.cost + demerits;
            if best.is_none_or(|(c, _)| cost < c) {
                best = Some((cost, e));
            }
            next_active.push(e);
        }

        let Some((cost, prev)) = best else {
            active = next_active;
            continue;
        };
        let ed = edges[prev];
        let pw = match items[k] {
            Item::Penalty { width, .. } => width,
            _ => 0.0,
        };
        let line_target = opts.column_width - if at_start[prev] { opts.par_indent } else { 0.0 };
        let (range, natural, stretch, shrink) = trim(items, sums, ed.start, k, pw);
        let (bad, fit, _) = score(
            natural,
            stretch + extra_stretch,
            shrink,
            line_target,
            tolerance,
            ed.fitness,
            opts,
        )
        .expect("the winning edge scored legal above");
        let idx = edges.len();
        edges.push(Edge {
            prev,
            item: k,
            start: k + 1,
            cost,
            width: natural,
            stretch,
            shrink,
            fitness: fit,
            badness: bad,
            lo: range.start,
            hi: range.end,
        });
        at_start.resize(edges.len(), false);
        active = next_active;
        active.push(idx);
    }

    let terminal = (0..edges.len()).rev().find(|&i| edges[i].item == end)?;
    let total = edges[terminal].cost;
    let mut chain = Vec::new();
    let mut cur = terminal;
    while edges[cur].prev != usize::MAX {
        chain.push(cur);
        cur = edges[cur].prev;
    }
    chain.reverse();

    let mut lines = Vec::with_capacity(chain.len());
    for (n, &e) in chain.iter().enumerate() {
        let ed = edges[e];
        lines.push(Line {
            items: ed.lo..ed.hi,
            natural: ed.width,
            stretch: ed.stretch,
            shrink: ed.shrink,
            target: opts.column_width - if n == 0 && first_line { opts.par_indent } else { 0.0 },
            badness: ed.badness,
            fitness: ed.fitness,
            forced: matches!(items[ed.item], Item::Penalty { forced: true, .. }),
            first: n == 0 && first_line,
            ragged: opts.ragged,
        });
    }
    Some((lines, total))
}

/// Safety net for input the solver cannot handle at all: fill each line greedily
/// with the last break that fits. Unreachable for normal text -- the third pass
/// always has a solution -- but a paragraph must degrade readably rather than
/// collapse into one line running off the page.
fn desperate(
    items: &[Item],
    sums: &Sums,
    from: usize,
    end: usize,
    opts: &BreakOptions,
) -> Vec<Line> {
    let first_line = from == 0;
    let mut out = Vec::new();
    let mut start = from;
    loop {
        let mut take = None;
        let mut first_legal = None;
        let mut i = start + 1;
        while i <= end {
            let legal = match items[i] {
                Item::Glue { breakable, stretch, .. } => breakable || stretch >= INFINITY / 2.0,
                Item::Penalty { forced: true, .. } => {
                    take = Some(i);
                    break;
                }
                Item::Penalty { .. } => true,
                Item::Box { .. } => false,
            };
            if legal {
                first_legal = first_legal.or(Some(i));
                let (_, natural, _, shrink) = trim(items, sums, start, i, 0.0);
                if f64::from(natural) - f64::from(shrink) > f64::from(opts.column_width) + f64::from(EPSILON) {
                    break;
                }
                take = Some(i);
            }
            i += 1;
        }
        // Nothing fits: still consume the first legal break so the paragraph makes
        // progress and stays on screen, overfull, rather than losing text.
        let b = take.or(first_legal).unwrap_or(end);
        if b <= start {
            break;
        }
        let (range, natural, stretch, shrink) = trim(items, sums, start, b, 0.0);
        let holds_content = range
            .clone()
            .any(|i| matches!(items[i], Item::Box { .. }));
        if !holds_content {
            if b >= end {
                break;
            }
            start = b;
            continue;
        }
        out.push(Line {
            items: range,
            natural,
            stretch,
            shrink,
            target: opts.column_width - if first_line && out.is_empty() { opts.par_indent } else { 0.0 },
            badness: 10000,
            fitness: 3,
            forced: matches!(items[b], Item::Penalty { forced: true, .. }),
            first: first_line && out.is_empty(),
            ragged: true,
        });
        if b >= end {
            break;
        }
        start = b;
    }
    out
}
