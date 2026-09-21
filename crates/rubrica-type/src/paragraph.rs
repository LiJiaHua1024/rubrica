//! Paragraph model: text segmented into unbreakable nodes, interleaved with glue.
//!
//! This is where mixed-script typography is decided. Segmentation comes from
//! UAX #14 break opportunities; the *recipe* of the glue at each opportunity is
//! chosen from the scripts on either side, which is what lets a Chinese paragraph
//! justify without the rubber-band gaps an ASCII-space-only model produces.

use std::ops::Range;

use crate::classify::Role;
use crate::units::{INFINITY, Pt};

/// Identifies a run style in the caller's style table. Opaque to this crate.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct StyleId(pub u16);

/// An unbreakable horizontal unit: a word, a single ideograph, or an inline
/// object. `advance` is its natural width, filled in by a [Measure].
#[derive(Clone, Debug)]
pub struct Node {
    pub text: Range<usize>,
    pub style: StyleId,
    pub advance: Pt,
    /// Role of the first code point, used for glue selection.
    pub role: Role,
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

/// The three glue flavours a mixed-script paragraph needs. Values are expressed
/// relative to the CJK em so a theme can retune them as one knob set.
#[derive(Clone, Debug)]
pub struct Spacing {
    /// A literal space between two Western words: stretchable, mildly shrinkable.
    pub latin_space: GlueRecipe,
    /// Between two ideographs. Base is zero -- ideographs are already side by
    /// side -- but it must stretch and shrink or justification has nothing to
    /// work with. This is the mechanism CJK-LaTeX's `\CJKglue` provides.
    pub cjk_join: GlueRecipe,
    /// Between an ideograph and a Western word: the classic 1/4 em, adjustable.
    pub mixed: GlueRecipe,
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
            cjk_join: GlueRecipe { base: 0.0, stretch: size * 0.08, shrink: size * 0.20 },
            mixed: GlueRecipe { base: size * 0.25, stretch: size * 0.125, shrink: size * 0.125 },
        }
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
        /// Extra width contributed when the break is taken (a hyphen, say).
        width: Pt,
    },
}

impl Item {
    pub fn glue(recipe: &GlueRecipe) -> Item {
        Item::Glue {
            base: recipe.base,
            stretch: recipe.stretch,
            shrink: recipe.shrink,
            breakable: true,
        }
    }

    pub fn is_glue(&self) -> bool {
        matches!(self, Item::Glue { .. })
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
}

/// Supplies natural advance widths. The production implementation shapes with
/// rustybuzz; tests substitute a fixed-width model so line breaking is
/// verifiable without a font on disk.
pub trait Measure {
    fn advance(&mut self, text: &str, range: Range<usize>, style: StyleId) -> Pt;
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

#[derive(Clone, Debug)]
pub struct BuildOptions<'a> {
    pub spacing: &'a Spacing,
    /// Style for a given text range. Only consulted for segmentation-relevant
    /// decisions here; the renderer re-derives runs from these ids.
    pub style_of: StyleId,
}

/// Segment `text` into nodes and interleave glue.
///
/// `breaks` are byte offsets at which UAX #14 permits a line break (offset 0 and
/// the end of the string excluded), and `mandatory` the subset that *requires* one.
pub fn build(
    text: &str,
    breaks: &[(usize, bool)],
    opts: &BuildOptions,
    measure: &mut dyn Measure,
) -> Paragraph {
    let mut p = Paragraph { nodes: Vec::new(), items: Vec::new() };
    // Split points: every permitted break, plus hard breaks.
    let mut cuts: Vec<(usize, bool)> = Vec::with_capacity(breaks.len() + 1);
    for &(at, required) in breaks {
        if at > 0 && at < text.len() {
            cuts.push((at, required));
        }
    }
    cuts.push((text.len(), false));

    let mut segments: Vec<Range<usize>> = Vec::with_capacity(cuts.len());
    let mut start = 0usize;
    for (end, _) in &cuts {
        if *end > start {
            segments.push(start..*end);
            start = *end;
        }
    }
    if segments.is_empty() {
        return p;
    }

    // Role of the most recent box, when a break opportunity separates two boxes
    // with no literal space between them (the Han/Latin and ideograph cases).
    let mut prev_role: Option<Role> = None;

    for seg in segments.iter() {
        let slice = &text[seg.clone()];
        // UAX #14 attaches a shared space or newline to whichever side of the
        // opportunity its tables happen to fall on, so whitespace has to be peeled
        // from *both* ends. Left inside a box it is counted in the advance and again
        // as glue, and a newline kept inside a box never becomes a break at all.
        let lead = slice.len() - slice.trim_start().len();
        let core = slice[lead..].trim_end();
        let ws_start = seg.start + lead + core.len();
        let trailing = &text[ws_start..seg.end];

        if !slice[..lead].is_empty() {
            emit_space(&mut p, &mut prev_role, &slice[..lead], opts);
        }

        if !core.is_empty() {
            let range = seg.start + lead..seg.start + lead + core.len();
            let role = Role::of(core.chars().next().unwrap());
            if let Some(prev) = prev_role {
                let recipe = glue_recipe_for(prev, role, opts.spacing);
                p.items.push(Item::glue(recipe));
            }
            let advance = measure.advance(text, range.clone(), opts.style_of);
            let id = p.nodes.len() as u32;
            p.nodes.push(Node { text: range, style: opts.style_of, advance, role });
            p.items.push(Item::Box { node: id });
            prev_role = Some(role);
        }

        if !trailing.is_empty() {
            emit_space(&mut p, &mut prev_role, trailing, opts);
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
        p.items.push(Item::Penalty { penalty: -10001, forced: true, width: 0.0 });
    }
    p
}

/// Emit the item for a run of literal whitespace, and record that the following
/// box needs no script-recipe glue because this space already separates them.
fn emit_space(p: &mut Paragraph, prev_role: &mut Option<Role>, ws: &str, opts: &BuildOptions) {
    if ws.contains('\n') {
        p.items.push(Item::Penalty { penalty: i32::MIN, forced: true, width: 0.0 });
    } else {
        p.items.push(Item::glue(&opts.spacing.latin_space));
    }
    *prev_role = None;
}

fn glue_recipe_for(a: Role, b: Role, s: &Spacing) -> &GlueRecipe {
    match (a, b) {
        (Role::Cjk, Role::Cjk) => &s.cjk_join,
        (Role::Cjk, Role::Western) | (Role::Western, Role::Cjk) => &s.mixed,
        (Role::Cjk, Role::Other) | (Role::Other, Role::Cjk) => &s.mixed,
        _ => &s.latin_space,
    }
}

/// Convenience: segment with UAX #14 tables and measure in one call.
pub fn paragraph_from_text(
    text: &str,
    spacing: &Spacing,
    style: StyleId,
    measure: &mut dyn Measure,
) -> Paragraph {
    use unicode_linebreak::BreakOpportunity;
    let breaks: Vec<(usize, bool)> = unicode_linebreak::linebreaks(text)
        .map(|(at, kind)| (at, kind == BreakOpportunity::Mandatory))
        .collect();
    build(
        text,
        &breaks,
        &BuildOptions { spacing, style_of: style },
        measure,
    )
}
