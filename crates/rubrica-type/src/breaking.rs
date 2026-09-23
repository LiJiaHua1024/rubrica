//! Knuth-Plass global line breaking.
//!
//! DirectWrite, like every browser, breaks lines greedily: fill one line, accept
//! whatever raggedness falls out, move on. This module scores every legal set of
//! breaks for a paragraph at once and keeps the cheapest. That is the difference
//! between "acceptable" and "even" justification -- and for Chinese, where the
//! only elastic material is the thin space between ideographs and it only opens
//! one way, it is the difference between justifying at all and not.
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
    /// TeX's `\hangindent`: what every line but the first gives up, so that the first
    /// line runs out to the left of the rest. A bullet or a footnote's number sits in
    /// the space the other lines leave, which is the only way a marker can hang in the
    /// margin and still have the lines under it break at their own, narrower measure.
    pub hang_indent: Pt,
    /// Max badness on the first pass (TeX's `\pretolerance`).
    pub pretolerance: i32,
    /// Max badness on the second pass (TeX's `\tolerance`).
    pub tolerance: i32,
    /// Stretch granted to every line on the second pass, tried before conceding
    /// an overfull line (TeX's `\emergencystretch`).
    pub emergencystretch: Pt,
    /// Whether discretionary hyphens may be broken at at all. The solver still
    /// withholds them from its first pass, so a paragraph that can be set without
    /// hyphens is set without hyphens; this only turns the facility off entirely.
    pub hyphenate: bool,
    /// Penalties for jumping into a worse fitness class.
    pub lousy_demerits: i32,
    pub awful_demerits: i32,
    pub nasty_demerits: i32,
    /// Set the whole block ragged-right instead of justified. Headings and code
    /// must never be stretched to the measure -- doing so is a typographic error --
    /// and over at TeX this is `\raggedright`'s infinite `\rightskip`.
    pub ragged: bool,
    /// Break rather than hang, whatever the glue could give back.
    ///
    /// A ragged line wider than its measure is normally accepted on the theory that
    /// its shrinkable glue will pull it back -- but [crate::place] only shrinks a
    /// line it is stretching anyway, because squeezing a heading or a code line is
    /// the error rather than the fix. Where the ink has a neighbour to its right --
    /// a table cell inside its column -- an accepted-here, refused-there overfull
    /// line overwrites that neighbour, so this option makes the line unbreakable by
    /// shrinking and lets the solver do the only remaining thing: break it.
    pub tight_box: bool,
}

impl BreakOptions {
    pub fn new(column_width: Pt) -> Self {
        Self {
            column_width,
            par_indent: 0.0,
            hang_indent: 0.0,
            pretolerance: 100,
            tolerance: 200,
            hyphenate: true,
            // TeX leaves this at zero because hyphenation keeps bad breaks rare; an
            // engine without them needs a real budget or every awkward paragraph
            // falls through to the tolerance-free pass and stops reporting badness.
            emergencystretch: column_width * 0.03,
            lousy_demerits: 100,
            awful_demerits: 1000,
            nasty_demerits: 1000,
            ragged: false,
            tight_box: false,
        }
    }

    /// The measure a line is set to. The paragraph's first line pays its own indent and
    /// nothing else; the lines under a hanging marker pay the hang instead, so the two
    /// never compound on the same line.
    #[inline]
    pub fn target(&self, first: bool) -> Pt {
        self.column_width - if first { self.par_indent } else { self.hang_indent }
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
    /// Width this line is set to; [`BreakOptions::target`]'s, so it differs from
    /// `column_width` on an indented first line, on a line under a hanging marker, and
    /// on the last.
    pub target: Pt,
    pub badness: i32,
    pub fitness: u8,
    /// Ended on a hard break (`\n`), so the next line starts a new visual block.
    pub forced: bool,
    /// First line of the paragraph, which was indented.
    pub first: bool,
    /// Hyphen node to draw at the end of this line, when it broke on one.
    pub hyphen: Option<u32>,
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

    /// Ink that no amount of shrinking can pull back inside the measure.
    ///
    /// Deliberately a different question from [Self::is_ragged]: infinite stretch
    /// absorbs leftover space and never surplus ink, so a paragraph's final line can
    /// be both -- ragged, and the worst line on the page. Only this one says the
    /// solver has to try again at a looser tolerance.
    #[inline]
    pub fn is_overfull(&self) -> bool {
        f64::from(self.natural) - f64::from(self.shrink)
            > f64::from(self.target) + f64::from(EPSILON)
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
    // In a tight box shrinking is not a way out of an overfull line, so the solver
    // scores the line as if it had no shrinkable glue at all and reaches for a break.
    let sh = if opts.tight_box { 0.0 } else { f64::from(shrink) };
    let eps = f64::from(EPSILON);
    let (ratio, bad) = if delta > eps && opts.ragged {
        // Ragged alignment leaves free space at the edge. A one-word line has no
        // internal glue at all, but that must not make its unused margin illegal.
        let r = if stretch >= INFINITY / 2.0 { 0.0 }
            else { (delta / f64::from(target.max(EPSILON))) as Pt };
        (r, badness(3.0 * r))
    } else if delta > eps {
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
    // are simply not allowed, except in a ragged block, where margin space costs
    // less than stretching the words and even a single-word line is legal.
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
        // TeX's ordering: the cheap pass may not hyphenate, so hyphenation is a
        // remedy for a paragraph that cannot be set cleanly rather than the first
        // thing tried. Without this, one overfull line is "legal" on pass 1 and the
        // dictionary is never consulted.
        let attempts = [
            (opts.pretolerance, 0.0f32, false),
            (opts.tolerance, opts.emergencystretch, opts.hyphenate),
            (i32::MAX, opts.emergencystretch, opts.hyphenate),
        ];
        let mut result = None;
        for (n, (tol, extra, hyphenate)) in attempts.into_iter().enumerate() {
            let mut o = *opts;
            o.hyphenate = hyphenate;
            if let Some(out) = solve(items, &sums, from, end, &o, tol, extra) {
                // A pass that had to leave a line overfull is not a success: TeX
                // keeps going and lets the next pass hyphenate. Accepting it here
                // would mean the dictionary is never consulted for exactly the
                // paragraphs that need it.
                //
                // Overfull means specifically too wide -- natural width past what
                // the glue can absorb. Badness alone cannot be the test: a line with
                // no stretchable glue at all, such as a one-line list item, also
                // scores 10000, and treating that as overfull would push ordinary
                // paragraphs all the way to the tolerance-free pass and pick worse
                // breaks than pass 1 already had.
                //
                // `is_ragged` is not the gate here: the final line's infinite
                // `\parfillskip` stretch makes it exempt from being *short*, and a
                // paragraph whose last line cannot be made to fit is exactly the one
                // that needs the looser pass -- and the hyphenation the looser pass
                // unlocks. A block that opted out of justification is overfull by
                // request, so it never escalates -- unless it also asked not to hang,
                // which is what [`BreakOptions::tight_box`] says: a cell whose ink has
                // a neighbour, or a line of code the reader cannot scroll sideways to
                // reach. For those, an overfull line is not an answer, and the cuts the
                // later passes unlock are the only way to avoid one.
                let overfull =
                    (!opts.ragged || opts.tight_box) && out.0.iter().any(|l| l.is_overfull());
                let last = n == attempts.len() - 1;
                result = Some((out, n as u8 + 1));
                if last || !overfull {
                    break;
                }
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
    if pass < 3
        && !opts.ragged
        && lines.iter().any(|l| l.is_overfull() || (l.badness >= 10000 && !l.is_ragged()))
    {
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
            Item::Penalty { hyphen: Some(_), .. } => opts.hyphenate,
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
            let line_target = opts.target(at_start[e]);
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
        let line_target = opts.target(at_start[prev]);
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
            target: opts.target(n == 0 && first_line),
            badness: ed.badness,
            fitness: ed.fitness,
            forced: matches!(items[ed.item], Item::Penalty { forced: true, .. }),
            first: n == 0 && first_line,
            ragged: opts.ragged,
            hyphen: match items[ed.item] {
                Item::Penalty { hyphen, .. } => hyphen,
                _ => None,
            },
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
                if f64::from(natural) - f64::from(shrink) > f64::from(opts.target(first_line && out.is_empty())) + f64::from(EPSILON) {
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
            target: opts.target(first_line && out.is_empty()),
            badness: 10000,
            fitness: 3,
            forced: matches!(items[b], Item::Penalty { forced: true, .. }),
            first: first_line && out.is_empty(),
            ragged: true,
            hyphen: None,
        });
        if b >= end {
            break;
        }
        start = b;
    }
    out
}
