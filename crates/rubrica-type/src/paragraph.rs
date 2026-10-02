//! Paragraph model: text segmented into unbreakable nodes, interleaved with glue.
//!
//! This is where mixed-script typography is decided. Segmentation comes from
//! UAX #14 break opportunities; the *recipe* of the glue at each opportunity is
//! chosen from the scripts on either side, which is what lets a Chinese paragraph
//! justify without the rubber-band gaps an ASCII-space-only model produces.

use std::ops::Range;

use crate::classify::{is_cjk_punct, is_compressible_punct, punctuation, PunctuationKind, Role};
use crate::units::{INFINITY, Pt};

/// Identifies a run style in the caller's style table. Opaque to this crate.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct StyleId(pub u16);

/// What a node draws.
///
/// A hyphen node is the one case where the glyph is not the source text: it stands
/// for the break character TeX takes from the font's `hyphenchar`, and it is drawn
/// only when the line actually breaks there.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NodeKind {
    Text,
    Hyphen,
}

/// An unbreakable horizontal unit: a word, a single ideograph, or an inline
/// object. `advance` is its natural width, filled in by a [Measure].
#[derive(Clone, Debug)]
pub struct Node {
    pub text: Range<usize>,
    pub style: StyleId,
    pub advance: Pt,
    /// Role of the first code point. It is the right-hand side of the recipe at the
    /// boundary *before* this box; the left-hand side of the boundary after it is read
    /// from the box's last character, which a quoted pair can make a different script
    /// from the first.
    pub role: Role,
    /// Whether this node is opening, closing, or another full-width punctuation mark.
    pub punctuation: Option<PunctuationKind>,
    /// Vertical extent around the baseline. Zero for ordinary text, where the
    /// font's own metrics apply; non-zero for an inline object such as an image or
    /// a displayed formula, which has a height the line must make room for.
    pub ascent: Pt,
    pub descent: Pt,
    pub kind: NodeKind,
}

impl Node {
    #[inline]
    pub fn is_text(&self) -> bool {
        self.kind == NodeKind::Text && self.ascent == 0.0 && self.descent == 0.0
    }
}

/// Elastic spacing recipe.
#[derive(Clone, Copy, Debug)]
pub struct GlueRecipe {
    pub base: Pt,
    pub stretch: Pt,
    pub shrink: Pt,
}

impl GlueRecipe {
    pub const fn fixed(base: Pt) -> Self {
        Self { base, stretch: 0.0, shrink: 0.0 }
    }
}

/// The glue flavours a mixed-script paragraph needs. The recipes are expressed
/// relative to the CJK em so a theme can retune them as one knob set.
#[derive(Clone, Debug)]
pub struct Spacing {
    /// A literal space between two Western words: stretchable, mildly shrinkable.
    pub latin_space: GlueRecipe,
    /// Between two ideographs. Base is zero -- ideographs are already side by
    /// side -- but it must stretch or justification has nothing to work with. This is
    /// the mechanism CJK-LaTeX's `\CJKglue` provides, and the reason a Chinese line
    /// breaks flush at both edges without a single word space in it.
    ///
    /// Its shrink is none, because a gap of no air has none to give: see
    /// [`Item::glue`].
    pub cjk_join: GlueRecipe,
    /// Between an ideograph and a Western word: the classic 1/4 em, adjustable.
    pub mixed: GlueRecipe,
    /// Whether a run of literal spaces is as many spaces as its author wrote.
    ///
    /// Off for prose, where a browser collapses the run to one word space and the
    /// author's alignment was never meant to be seen. On for a monospace block, where
    /// the run *is* the alignment: a column drawn with spaces has to keep its column.
    pub literal_space_runs: bool,
    /// Keep Korean syllables in the same word together; spaces remain breakable.
    pub keep_korean_words: bool,
    /// Fraction removed from a full-width punctuation mark's advance, from 0 to 0.5.
    /// The glyph itself keeps its natural outline; only its empty side bearing is
    /// reclaimed, so the next character moves closer without the mark being squeezed.
    ///
    /// The blank a mark carries sits on the side its ink does not: on the right of a
    /// closing mark (，。：」) and on the left of an opening one （「《. Reclaiming it
    /// therefore tightens the mark against whichever neighbour the font left room for
    /// -- the text after a closing mark, the text before an opening one -- and it is
    /// up to the painter to move an opening mark's glyph into the space its own
    /// advance no longer covers. Only marks with a known blank side are touched: a
    /// centred full-width symbol, the ideographic space and the ambiguous curly
    /// quotes keep whatever width the author's text gave them.
    pub punctuation_compression: f32,
}

impl Spacing {
    /// Defaults scaled for a `size`-pt face.
    ///
    /// The Western space keeps Computer Modern's proportions -- about 1/3 em wide,
    /// stretchable by 1/6 em, shrinkable by 1/9 em. Tighter than that and a narrow
    /// measure has no legal break set at all: the paragraph falls back to one
    /// overfull line, which is a failure of the recipe, not of the solver.
    pub fn for_size(size: Pt) -> Self {
        Self {
            latin_space: GlueRecipe {
                base: size * 0.3333,
                stretch: size * 0.1667,
                shrink: size * 0.1111,
            },
            cjk_join: GlueRecipe { base: 0.0, stretch: size * 0.08, shrink: 0.0 },
            mixed: GlueRecipe { base: size * 0.25, stretch: size * 0.125, shrink: size * 0.125 },
            literal_space_runs: false,
            keep_korean_words: false,
            punctuation_compression: 0.0,
        }
    }

    /// The spacing of a monospace block, whose word space is the face's own advance
    /// rather than a third of an em. Nothing about it stretches or shrinks and a run of
    /// spaces is as wide as it is long, because the author lined something up with it:
    /// two lines of the same character count then end in the same column, which is the
    /// only promise a grid makes.
    pub fn monospace(size: Pt, advance: Pt) -> Self {
        Self { latin_space: GlueRecipe::fixed(advance), literal_space_runs: true, ..Self::for_size(size) }
    }
}

#[derive(Clone, Copy, Debug)]
pub enum Item {
    Box { node: u32 },
    Glue {
        base: Pt,
        stretch: Pt,
        shrink: Pt,
        /// False for glue that exists only to hold two nodes together.
        breakable: bool,
    },
    Penalty {
        /// Line-break desirability; negative invites the break, `i32::MIN` forces it.
        penalty: i32,
        forced: bool,
        /// Extra width contributed when the break is taken.
        width: Pt,
        /// Node to draw at the line's end when this break is taken, which is what
        /// makes a discretionary hyphen show its glyph.
        hyphen: Option<u32>,
    },
}

impl Item {
    pub fn glue(recipe: &GlueRecipe) -> Item {
        Item::Glue {
            base: recipe.base,
            stretch: recipe.stretch,
            // A gap can close; it cannot eat ink. Without the cap a theme that retunes
            // the recipes -- and the inter-ideograph join in particular, whose base is
            // nothing at all -- hands the solver room that is really its neighbour's
            // glyphs sliding on top of each other, because the solver reads shrink as
            // width the line may give back and `place` spends it evenly.
            shrink: recipe.shrink.min(recipe.base),
            breakable: true,
        }
    }

    /// A join that holds two nodes together without moving them apart.
    ///
    /// Emitted where a style changes inside a stretch of text the source offers no
    /// break at, which makes the two nodes one word: a footnote's raised digit after
    /// its word, an image between letters. Giving those the script's glue would set
    /// `word ¹` and let justification widen the gap in the middle of a word.
    pub fn join() -> Item {
        Item::Glue { base: 0.0, stretch: 0.0, shrink: 0.0, breakable: false }
    }

    pub fn is_glue(&self) -> bool {
        matches!(self, Item::Glue { .. })
    }

    /// True for a discretionary hyphen, which the solver may refuse.
    pub fn is_hyphen(&self) -> bool {
        matches!(self, Item::Penalty { hyphen: Some(_), .. })
    }
}

#[derive(Debug)]
pub struct Paragraph {
    pub nodes: Vec<Node>,
    pub items: Vec<Item>,
}

impl Paragraph {
    #[inline]
    pub fn node(&self, i: u32) -> &Node {
        &self.nodes[i as usize]
    }

    /// The widest run of content no legal break splits.
    ///
    /// A fragment ends where the solver could end a line: at breakable glue, at a
    /// forced break, and at the paragraph's own end. What a conservative answer
    /// means here is a *wide* one, so a discretion kept out of the break set --
    /// a hyphen the solver may use but was never promised -- holds its halves
    /// together, and a style change mid-word stays one word through the join that
    /// already says so.
    ///
    /// This is the width a column has to hold before its cells can be broken at
    /// all, which is why it is asked of a table's cells: squeeze a column below
    /// its own widest fragment and the solver is left with no break to take.
    pub fn widest_fragment(&self) -> Pt {
        let mut widest: Pt = 0.0;
        let mut run: Pt = 0.0;
        for it in &self.items {
            match *it {
                Item::Box { node } => run += self.node(node).advance,
                // A join holds two nodes of one word together, so its zero width
                // belongs to the fragment on either side of it.
                Item::Glue { base, breakable: false, .. } => run += base,
                // A break the solver may take ends the fragment here.
                Item::Glue { .. } => {
                    widest = widest.max(run);
                    run = 0.0;
                }
                // A penalty's own width is ink it draws when the break is taken,
                // so it belongs to the fragment the break would end -- but only when
                // the break is one the solver has to take. A discretionary is one it
                // may take and may refuse, and a conservative answer is the *wide*
                // one: ending the fragment at every point reported the word a
                // fraction of the width it occupies, and a column sized from that
                // answer could not hold the word it came from.
                Item::Penalty { width, forced: true, .. } => {
                    widest = widest.max(run + width);
                    run = 0.0;
                }
                Item::Penalty { width, .. } => run += width,
            }
        }
        widest.max(run)
    }
}

/// Supplies natural advance widths. The production implementation shapes with
/// rustybuzz; tests substitute a fixed-width model so line breaking is
/// verifiable without a font on disk.
pub trait Measure {
    fn advance(&mut self, text: &str, range: Range<usize>, style: StyleId) -> Pt;

    /// Vertical extent an inline object claims around the baseline, as
    /// `(ascent, descent)` in points. Returning zero -- the default -- means the
    /// line box is derived from the font metrics of the glyphs on it.
    fn extent(&mut self, _text: &str, _range: Range<usize>, _style: StyleId) -> (Pt, Pt) {
        (0.0, 0.0)
    }
}

/// `Measure` that charges `factor * size` per Unicode scalar -- a passable CJK
/// approximation, and handy for unit tests on Latin too.
pub struct MonospaceMeasure {
    pub size: Pt,
    pub factor: Pt,
}

impl Measure for MonospaceMeasure {
    fn advance(&mut self, text: &str, range: Range<usize>, _style: StyleId) -> Pt {
        let n = text[range].chars().count() as Pt;
        n * self.size * self.factor
    }
}

/// Where a line may split a word, and what the split costs in width.
///
/// The offsets come from a dictionary the caller has already consulted; `width` is
/// the advance of the font's hyphen glyph, which only a shaping backend can supply,
/// and which the breaking numbers are meaningless without.
#[derive(Clone, Copy, Debug)]
pub struct Hyphenation<'a> {
    /// Byte offsets inside the text at which a break may be taken.
    pub points: &'a [usize],
    pub width: Pt,
}

impl<'a> Hyphenation<'a> {
    /// No word may be split.
    pub const NONE: Hyphenation<'static> = Hyphenation { points: &[], width: 0.0 };
}

#[derive(Clone, Debug)]
pub struct BuildOptions<'a> {
    pub spacing: &'a Spacing,
    /// Style for a given text range. Only consulted for segmentation-relevant
    /// decisions here; the renderer re-derives runs from these ids.
    pub style_of: StyleId,
    /// Byte offsets inside Western words at which a discretionary hyphen break is
    /// permitted, as produced by a Knuth-Liang dictionary. Offsets at a word edge or
    /// on an existing cut are ignored.
    pub hyphens: &'a [usize],
    /// Demerit cost of taking a hyphenated break, TeX's `\hyphenpenalty`.
    pub hyphen_penalty: i32,
    /// Advance of the font's hyphen glyph, measured by the caller so this crate
    /// stays free of any shaping dependency.
    pub hyphen_width: Pt,
    /// Inline style ranges, byte offsets into the same string as `text`.
    ///
    /// Segments are cut at these boundaries as well as at UAX #14 opportunities, so
    /// the invariant "one node, one style" holds and both measurement and painting
    /// can treat a node as atomic.
    pub spans: &'a [StyleSpan],
}

/// A run of source carrying one style.
#[derive(Clone, Debug)]
pub struct StyleSpan {
    pub range: Range<usize>,
    pub style: StyleId,
}

impl StyleSpan {
    /// Resolve the style covering `at`, falling back to `default`.
    pub fn resolve(spans: &[StyleSpan], at: usize, default: StyleId) -> StyleId {
        // Sorted by start; find the last span that begins at or before `at`.
        let i = match spans.binary_search_by(|s| s.range.start.cmp(&at)) {
            Ok(i) => i,
            Err(0) => return default,
            Err(i) => i - 1,
        };
        let s = &spans[i];
        if at < s.range.end {
            s.style
        } else {
            spans[i + 1..]
                .iter()
                .find(|s| s.range.contains(&at))
                .map_or(default, |s| s.style)
        }
    }
}

/// Segment `text` into nodes and interleave glue.
///
/// `breaks` are byte offsets at which UAX #14 permits a line break (offset 0 and
/// the end of the string excluded), each carrying a flag saying whether the source
/// *requires* one. That flag, not the character at the break, is what becomes a
/// forced line: a caller's list that omits it cannot make the builder recognise
/// every separator, which is why [`emit_space`] still answers from the run itself.
pub fn build(
    text: &str,
    breaks: &[(usize, bool)],
    opts: &BuildOptions,
    measure: &mut dyn Measure,
) -> Paragraph {
    let mut p = Paragraph { nodes: Vec::new(), items: Vec::new() };
    // Direction changes split shaping boxes, but do not create line breaks.
    let bidi = crate::BidiInfo::new(text, None);
    // Split points: every permitted break, plus hard breaks.
    let mut cuts: Vec<(usize, bool)> = Vec::with_capacity(breaks.len() + 1);
    for (at, _) in text.char_indices().skip(1) {
        if bidi.levels[at] != bidi.levels[at - 1] {
            cuts.push((at, false));
        }
    }
    for &(at, required) in breaks {
        if at > 0 && at < text.len() {
            cuts.push((at, required));
        }
    }
    // Punctuation is segmented as its own node so a full-width mark can surrender
    // only its blank side bearing. These are shaping boundaries, not line-break
    // opportunities: UAX #14 still decides where a line may end.
    for (at, ch) in text.char_indices() {
        if is_compressible_punct(ch) {
            if at > 0 {
                cuts.push((at, false));
            }
            let end = at + ch.len_utf8();
            if end < text.len() {
                cuts.push((end, false));
            }
        }
    }
    // Style changes are breaks too, so a node never straddles two styles.
    for s in opts.spans {
        for &at in [&s.range.start, &s.range.end] {
            if at > 0 && at < text.len() {
                cuts.push((at, false));
            }
        }
    }
    cuts.sort_by_key(|&(at, _)| at);
    // Merging duplicates must not lose a mandatory flag hiding behind a style cut.
    cuts.dedup_by(|a, b| {
        if a.0 == b.0 {
            b.1 |= a.1;
            true
        } else {
            false
        }
    });
    // A dictionary point splits a word that UAX #14 would keep whole. It joins the
    // cut list but must not later be handed glue, which is what distinguishes it.
    // The offsets are byte offsets into `text`, so a point that is not on a
    // character boundary is a slicing panic waiting to happen, and a point right
    // after an explicit hyphen lands where the author's own `-` already breaks --
    // taking it as a discretionary would draw a second hyphen over the first.
    let mut hyphen_set: Vec<usize> = opts
        .hyphens
        .iter()
        .copied()
        .filter(|&at| {
            at > 0
                && at < text.len()
                && text.is_char_boundary(at)
                && !text[..at].ends_with(char::is_whitespace)
                && !text[..at].ends_with('-')
        })
        .collect();
    // This list is as long as the block: the reader hands a code fence a
    // discretionary point at every character boundary, and a megabyte of code is a
    // million points. Membership is asked once per segment and a linear scan makes
    // the whole build quadratic in the block's own size -- hours of frozen layout,
    // re-run on every scroll and resize. The points are a set, and a sorted one
    // answers in a logarithm.
    hyphen_set.sort_unstable();
    hyphen_set.dedup();
    for at in &hyphen_set {
        cuts.push((*at, false));
    }
    cuts.sort_by_key(|&(at, _)| at);
    cuts.dedup_by(|a, b| {
        if a.0 == b.0 {
            b.1 |= a.1;
            true
        } else {
            false
        }
    });
    cuts.push((text.len(), false));

    // Where the *source* permits a break, as opposed to where a cut exists only
    // because the style changes. `breaks` arrives sorted from UAX #14.
    let mut allowed: Vec<usize> = breaks
        .iter()
        .filter(|&&(at, _)| at > 0 && at < text.len())
        .map(|&(at, _)| at)
        .collect();
    allowed.sort_unstable();
    allowed.dedup();

    // Each segment with the flag of the cut that ends it: a mandatory break is the
    // one opportunity UAX #14 does not let the source decline, and only the flag
    // knows which cut was that.
    let mut segments: Vec<(Range<usize>, bool)> = Vec::with_capacity(cuts.len());
    let mut start = 0usize;
    for (end, required) in &cuts {
        if *end > start {
            segments.push((start..*end, *required));
            start = *end;
        }
    }
    if segments.is_empty() {
        return p;
    }

    // The final code point of the most recent box, when a break opportunity separates
    // two boxes with no literal space between them (the Han/Latin and ideograph
    // cases). The character is what decides whether a recipe applies at all, and which
    // one: it is the character the next boundary lands beside.
    let mut prev: Option<char> = None;

    for (seg, mandatory) in &segments {
        let slice = &text[seg.clone()];
        // UAX #14 attaches a shared space or newline to whichever side of the
        // opportunity its tables happen to fall on, so whitespace has to be peeled
        // from *both* ends. Left inside a box it is counted in the advance and again
        // as glue, and a newline kept inside a box never becomes a break at all.
        let lead = slice.len() - slice.trim_start_matches(is_stripped).len();
        let core = slice[lead..].trim_end_matches(is_stripped);
        let ws_start = seg.start + lead + core.len();
        let trailing = &text[ws_start..seg.end];

        if !slice[..lead].is_empty() {
            emit_space(&mut p, &mut prev, &slice[..lead], opts);
        }

        if !core.is_empty() {
            let range = seg.start + lead..seg.start + lead + core.len();
            let first = core.chars().next().unwrap();
            let role = Role::of(first);
            let style = StyleSpan::resolve(opts.spans, range.start, opts.style_of);
            if let Some(prev_ch) = prev {
                // Getting here means no whitespace separates the two boxes, because
                // any leading or trailing run of it has already been emitted as a
                // space, which clears `prev`. So the boundary is a cut: use the
                // script's glue only where the source really does allow a break, and
                // otherwise join the two halves of the same word tightly.
                //
                // The left side of the recipe is the role of the character the boundary
                // actually lands beside -- the previous box's *last* character -- and not
                // the box's own role, which is read from its first. A box that opens with
                // a Latin quote and ends in Han is both: `"对` is one box, because UAX #14
                // breaks nothing inside a quoted pair, and reading its role from the quote
                // gave `对`|`的` the quarter em that belongs between Han and Latin. Set in
                // bold, `**"对的太对"**` printed as `"对 的太对`, and `**"邪修"**` as
                // `"邪 修`.
                if allowed.binary_search(&seg.start).is_ok() {
                    p.items.push(punct_glue(prev_ch, first, Role::of(prev_ch), role, opts.spacing));
                } else {
                    p.items.push(Item::join());
                }
            }
            let natural = measure.advance(text, range.clone(), style);
            let (ascent, descent) = measure.extent(text, range.clone(), style);
            let punctuation = core
                .chars()
                .next()
                .filter(|ch| is_compressible_punct(*ch))
                .map(punctuation);
            // Only a mark whose ink sits at one end of its box has a blank side to
            // give up. A centred full-width symbol (％ ＃ －) or the ideographic space
            // has none, and trimming its advance would slide it off the middle of its
            // neighbours instead of closing a gap its author never wrote.
            //
            // Which end that is decides what the painter has to do with the glyph: an
            // opening mark keeps its blank on the left, so the *box* starts where the
            // mark does and the glyph is drawn that much further left; a closing mark
            // gives up the blank on its right, which is a plain shorter advance. See
            // `fit_punctuation_runs` in the painter.
            let compression = match punctuation {
                Some(PunctuationKind::Opening | PunctuationKind::Closing) => {
                    opts.spacing.punctuation_compression.clamp(0.0, 0.5)
                }
                _ => 0.0,
            };
            let id = p.nodes.len() as u32;
            p.nodes.push(Node {
                text: range,
                style,
                advance: natural * (1.0 - compression),
                role,
                punctuation,
                ascent,
                descent,
                kind: NodeKind::Text,
            });
            p.items.push(Item::Box { node: id });
            prev = Some(core.chars().next_back().unwrap());
        }

        let split_here = hyphen_set.binary_search(&seg.end).is_ok() && trailing.is_empty();
        if *mandatory {
            // `\hfil\break`, which is what a forced break is in TeX: the line ends
            // where the author said so and is then left flush at its natural width.
            // Without the filler the line is justified like any other, and because a
            // Chinese line's glue is stretchable anywhere, three characters after a
            // hard break would be spread across the whole column. UAX #14 says where
            // these are; guessing from the characters missed a BK, a ZL and a ZP.
            p.items.push(Item::Glue {
                base: 0.0,
                stretch: INFINITY,
                shrink: 0.0,
                breakable: true,
            });
            p.items.push(Item::Penalty { penalty: i32::MIN, forced: true, width: 0.0, hyphen: None });
            prev = None;
        } else if split_here {
            let style = StyleSpan::resolve(opts.spans, seg.end, opts.style_of);
            let h = p.nodes.len() as u32;
            p.nodes.push(Node {
                text: seg.end..seg.end,
                style,
                advance: opts.hyphen_width,
                role: Role::Western,
                punctuation: None,
                ascent: 0.0,
                descent: 0.0,
                kind: NodeKind::Hyphen,
            });
            p.items.push(Item::Penalty {
                penalty: opts.hyphen_penalty,
                forced: false,
                width: opts.hyphen_width,
                hyphen: Some(h),
            });
            // The penalty is itself the separator: leaving `prev` set would
            // make the next segment insert a word space between the two halves.
            prev = None;
        } else if !trailing.is_empty() {
            emit_space(&mut p, &mut prev, trailing, opts);
        }
    }

    if !p.nodes.is_empty() {
        // Flush the last line: infinite stretch makes it ragged, exactly as
        // \parfillskip does.
        p.items.push(Item::Glue {
            base: 0.0,
            stretch: INFINITY,
            shrink: 0.0,
            breakable: true,
        });
        p.items.push(Item::Penalty { penalty: -10001, forced: true, width: 0.0, hyphen: None });
    }
    p
}

/// Whether a code point is stripped from the ends of a segment before it is
/// measured.
///
/// Only ASCII whitespace is inter-word air: a run of it collapses to one word space
/// and what falls at a line's edge is dropped, and this module does the same to the
/// runs that reach [`emit_space`]. Every other White_Space character is a glyph the
/// author wrote, and U+3000 is the one that matters most -- a full-width indent.
/// Peeled as a word space it both lost its width and became a break opportunity, so
/// an indented Chinese paragraph came out flush left with the indent deleted from
/// the measure altogether. The three line separators are stripped too, for a
/// different reason: nothing draws them, and what follows them is a new line
/// rather than another word.
fn is_stripped(ch: char) -> bool {
    matches!(
        ch,
        ' ' | '\t' | '\n' | '\r' | '\u{b}' | '\u{c}' | '\u{85}' | '\u{2028}' | '\u{2029}'
    )
}

/// Emit the item for a run of literal whitespace, and record that the following
/// box needs no script-recipe glue because this space already separates them.
fn emit_space(p: &mut Paragraph, prev: &mut Option<char>, ws: &str, opts: &BuildOptions) {
    // A cut that ends a segment is normally told to be mandatory by UAX #14, and
    // that is what [`build`] acts on; this is the same answer worked out from the
    // run itself, for a caller that hands over a break list without the flag. Every
    // mandatory separator UAX #14 can name, not only `\n`: a CR, a VT, a NEL, a
    // line or paragraph separator is a line the author ended, just spelled
    // differently.
    if ws.contains(['\n', '\r', '\u{b}', '\u{c}', '\u{85}', '\u{2028}', '\u{2029}']) {
        // `\hfil\break`, which is what a forced break is in TeX: the line ends where
        // the author said so and is then left flush at its natural width. Without the
        // filler the line is justified like any other, and because a Chinese line's
        // glue is stretchable anywhere, three characters after a hard break would be
        // spread across the whole column.
        p.items.push(Item::Glue {
            base: 0.0,
            stretch: INFINITY,
            shrink: 0.0,
            breakable: true,
        });
        p.items.push(Item::Penalty { penalty: i32::MIN, forced: true, width: 0.0, hyphen: None });
    } else {
        let r = opts.spacing.latin_space;
        // One space's recipe, or the run's own length worth of it. Breaking anywhere
        // inside the run is the same either way: glue left at a line's end is trimmed.
        let n = if opts.spacing.literal_space_runs { ws.chars().count() as Pt } else { 1.0 };
        p.items.push(Item::Glue {
            base: r.base * n,
            stretch: r.stretch * n,
            // Same cap `Item::glue` applies: a gap can close, it cannot eat ink.
            // A theme that retunes the recipe past the cap would otherwise hand
            // the solver room that is really its neighbour's glyphs sliding over.
            shrink: (r.shrink * n).min(r.base * n),
            breakable: true,
        });
    }
    *prev = None;
}

/// A break the source offers where its author wrote nothing: the line may split
/// here, but no air appears on the page. ASCII punctuation is `Common`, so `-`, `/`
/// and `:` all reach this arm -- a word space at one of them would set `rubrica-app`
/// as `rubrica- app`, a character that is in no file.
const NO_AIR: GlueRecipe = GlueRecipe::fixed(0.0);

/// Marks that are punctuation of whichever text they stand in: a dash and an ellipsis
/// are Chinese 破折号/省略号 and English dashes and dots both, and the engine cannot
/// see which face draws them.
///
/// They are read from their neighbours instead. Beside Han they are Chinese marks
/// whose box already carries their air -- the same second gap [`punct_glue`] refuses
/// for `：` -- and the quarter em the mixed recipe would add showed the page as
/// `他说 —— 这样`. Beside Latin they are Latin punctuation, set exactly as they are
/// today.
fn is_shared_mark(ch: char) -> bool {
    matches!(ch, '\u{2014}' | '\u{2013}' | '\u{2026}')
}

/// The glue between two boxes the source runs together.
///
/// A full-width mark carries its air inside the glyph, so the script recipe on top of
/// it is a second gap the author never wrote: `界：对` justified on the page as
/// `界 ： 对`, and `界、R` as `界、 R`. Which side of a line each mark belongs to is
/// already UAX #14's answer, not this one's -- `，` and `（` are never offered as a
/// break at all -- so all a mark changes here is the width.
fn punct_glue(prev_ch: char, next_ch: char, prev: Role, next: Role, s: &Spacing) -> Item {
    let beside_han = |mark: char, other: Role| is_shared_mark(mark) && other == Role::Cjk;
    if is_cjk_punct(prev_ch)
        || is_cjk_punct(next_ch)
        || beside_han(prev_ch, next)
        || beside_han(next_ch, prev)
    {
        Item::glue(&NO_AIR)
    } else {
        Item::glue(glue_recipe_for(prev, next, s))
    }
}

fn glue_recipe_for(a: Role, b: Role, s: &Spacing) -> &GlueRecipe {
    match (a, b) {
        (Role::Cjk, Role::Cjk) => &s.cjk_join,
        (Role::Cjk, Role::Western) | (Role::Western, Role::Cjk) => &s.mixed,
        (Role::Cjk, Role::Other) | (Role::Other, Role::Cjk) => &s.mixed,
        _ => &NO_AIR,
    }
}

/// As [`paragraph_from_text`], but also splitting Western words at the byte offsets
/// in `hyphenation`, which is how a Knuth-Liang dictionary feeds discretionary
/// breaks in.
pub fn paragraph_from_text_hyphenated(
    text: &str,
    spacing: &Spacing,
    style: StyleId,
    spans: &[StyleSpan],
    hyphenation: &Hyphenation<'_>,
    measure: &mut dyn Measure,
) -> Paragraph {
    let breaks = break_opportunities(text, spacing);
    build(
        text,
        &breaks,
        &BuildOptions {
            spacing,
            style_of: style,
            spans,
            hyphens: hyphenation.points,
            hyphen_penalty: 200,
            hyphen_width: hyphenation.width,
        },
        measure,
    )
}

/// Convenience: segment with UAX #14 tables and measure in one call.
pub fn paragraph_from_text(
    text: &str,
    spacing: &Spacing,
    style: StyleId,
    spans: &[StyleSpan],
    measure: &mut dyn Measure,
) -> Paragraph {
    let breaks = break_opportunities(text, spacing);
    build(
        text,
        &breaks,
        &BuildOptions {
            spacing,
            style_of: style,
            spans,
            hyphens: &[],
            hyphen_penalty: 200,
            hyphen_width: 0.0,
        },
        measure,
    )
}

fn break_opportunities(text: &str, spacing: &Spacing) -> Vec<(usize, bool)> {
    use unicode_linebreak::BreakOpportunity;
    let hangul = |c: char| matches!(c as u32,
        0x1100..=0x11ff | 0x3130..=0x318f | 0xa960..=0xa97f | 0xac00..=0xd7ff);
    unicode_linebreak::linebreaks(text)
        .map(|(at, kind)| (at, kind == BreakOpportunity::Mandatory))
        .filter(|&(at, mandatory)| mandatory || !spacing.keep_korean_words
            || !(text[..at].chars().next_back().is_some_and(hangul)
                && text[at..].chars().next().is_some_and(hangul)))
        .collect()
}
