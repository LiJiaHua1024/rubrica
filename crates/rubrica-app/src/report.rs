//! Headless numeric verification of a rendered document.
//!
//! Typography is a visual property, but the failure modes that matter most are
//! measurable: a justified line that stops short of the measure, an overfull line
//! hanging past it, a run of Chinese set in a Latin face. This reports those
//! numbers through the *same* `build_ops` the window draws from, so it describes
//! what is actually painted rather than what a second implementation would have
//! produced -- and it never touches the desktop.
//!
//! Run: `rubrica-app --report [file.md] [--width 1080] [--dpi 96] [--zoom 120] [--face 1]`

use std::collections::BTreeMap;

use crate::font::FontEngine;
use crate::theme::{ColorRole, TextFace, Theme, Zoom};
use crate::view::build_ops;
use crate::{Error, Result};

/// One text line reconstructed from its paint runs.
struct Line {
    left: f32,
    right: f32,
    families: Vec<String>,
}

pub fn report(
    source: &str,
    path: Option<&str>,
    width: f32,
    dpi: f32,
    hyphenate: bool,
    zoom: Zoom,
    face: usize,
) -> Result<()> {
    let mut font = FontEngine::new().map_err(|e| -> Error { format!("DirectWrite: {e}").into() })?;
    if !font.probe() {
        return Err("no usable font face".into());
    }
    let mut theme = Theme::default();
    theme.set_zoom(zoom);
    theme.set_face(face);
    let doc = rubrica_doc::Document::parse(source);
    // No render target exists here, so figures are measured from their files
    // through WIC but not decoded to bitmaps -- enough to lay out and report.
    let _ = unsafe { windows::Win32::System::Com::CoInitializeEx(None, windows::Win32::System::Com::COINIT_MULTITHREADED) };
    let store = crate::images::ImageStore::new().ok();
    let base = path.map(std::path::Path::new).and_then(|p| p.parent());
    let mut math = crate::math::MathStore::new();
    let mut objects = crate::view::Objects::new(store.as_ref(), base, &mut math);
    let hyphenator = if hyphenate { crate::hyphen::Hyphenator::english() } else { None };
    let page = build_ops(
        &mut font,
        &theme,
        &doc,
        width,
        dpi,
        &mut objects,
        hyphenator.as_ref(),
    );
    let ops = &page.ops;
    let (height, column_pt, left_pt) = (page.height, page.column, page.left);
    let k = dpi / 72.0;

    let mut lines: Vec<Line> = Vec::new();
    let mut census: BTreeMap<String, usize> = BTreeMap::new();
    for op in ops {
        let crate::view::Op::Runs(runs) = op else { continue };
        if runs.is_empty() {
            continue;
        }
        let mut l = Line {
            left: f32::INFINITY,
            right: f32::NEG_INFINITY,
            families: Vec::new(),
        };
        for r in runs {
            let w: f32 = r.advances.iter().sum();
            l.left = l.left.min(r.x);
            l.right = l.right.max(r.x + w);
            let fam = r.family.clone();
            *census.entry(fam.clone()).or_default() += r.glyphs.len();
            if !l.families.contains(&fam) {
                l.families.push(fam);
            }
        }
        lines.push(l);
    }

    let col_dip = column_pt * k;
    // The measure's right edge in device pixels: where every justified line ends,
    // whatever its own left happens to be.
    let edge = (left_pt + column_pt) * k;
    println!("document   : {}", path.unwrap_or("(built-in sample)"));
    println!("size       : {:.2}pt body  ({}% of the design's)", theme.base, zoom.percent());
    // Which pairing the page is set in, and which ones this machine could have set it in
    // -- so a face that came back as somebody else's substitute is visible here rather
    // than only on a screen.
    let f = &TextFace::ALL[theme.face];
    let installed: Vec<&str> = TextFace::ALL
        .iter()
        .filter(|t| font.has_family(t.body) && font.has_family(t.heading))
        .map(|t| t.label)
        .collect();
    println!("face       : {}  ({} + {})", f.label, f.body, f.heading);
    println!("             installed: {}", installed.join(", "));
    println!("blocks     : {}   lines: {}   height {:.0}pt", doc.blocks.len(), lines.len(), height);
    println!("measure    : {:.1}pt ({:.1} em)  = {col_dip:.1}dip at {dpi}dpi", column_pt, column_pt / theme.base);
    println!();
    println!("  #      left    right    fill  faces");

    // A justified line must land on the measure; the last line of each block is
    // legitimately short, so report the tight cluster and the outliers separately.
    let mut fills: Vec<f32> = Vec::new();
    let mut off_measure = 0usize;
    for (i, l) in lines.iter().enumerate() {
        // Measure an edge, not a width: a line under a hanging marker or a first-line
        // indent starts further in, so its width is legitimately short while its right
        // edge sits exactly on the measure. Judging the width would report every hung
        // line as a ragged one.
        let fill = l.right / edge;
        let short = fill < 0.985;
        let over = (l.right - edge) / k;
        if !short {
            fills.push(fill);
        }
        if over > 0.6 {
            off_measure += 1;
        }
        println!(
            "{i:>3}  {:>8.1} {:>8.1} {:>6.3}{} {}",
            l.left,
            l.right,
            fill,
            if short { " ragged" } else if over > 0.6 { " OVER" } else { "      " },
            l.families.join("+")
        );
    }

    println!();
    if !fills.is_empty() {
        let mean = fills.iter().sum::<f32>() / fills.len() as f32;
        let var = fills.iter().map(|f| (f - mean) * (f - mean)).sum::<f32>() / fills.len() as f32;
        let worst = fills.iter().cloned().fold(0.0f32, f32::max);
        let best = fills.iter().cloned().fold(1.0f32, f32::min);
        println!(
            "justified lines: {}  mean fill {mean:.4}  spread {:.4}..{worst:.4}  stdev {:.4}",
            fills.len(),
            best,
            var.sqrt()
        );
    }
    if off_measure > 0 {
        println!("!! {off_measure} line(s) hang past the measure (overfull)");
    }
    // Which lines stand back from the margin, and by how much -- the work of a hanging
    // marker. A document whose list items wrap should never report none of these: that
    // the marker stopped being measured is a layout change too subtle to spot in a
    // glyph census.
    let mut offsets: BTreeMap<i32, usize> = BTreeMap::new();
    for l in &lines {
        let back = l.left / k - left_pt;
        if back > 1.0 {
            *offsets.entry((back * 10.0).round() as i32).or_default() += 1;
        }
    }
    if !offsets.is_empty() {
        let n: usize = offsets.values().sum();
        let steps: Vec<String> = offsets
            .iter()
            .map(|(t, c)| format!("{:.1}pt x{c}", *t as f32 / 10.0))
            .collect();
        println!("hang         : {n} line(s) stand back from the margin at {}", steps.join(", "));
    }
    println!("glyph census:");
    for (fam, n) in &census {
        println!("  {fam:<24} {n:>6} glyphs");
    }
    let han: usize = source.chars().filter(|c| matches!(*c as u32, 0x4E00..=0x9FFF)).count();
    let latin: usize = source.chars().filter(|c| c.is_ascii_alphabetic()).count();
    println!("source has {han} ideographs and {latin} latin letters");
    // Counted from the source as well as from the cache: the two differ whenever a
    // formula repeats, and a formula that set from a face with no `MATH` table
    // still draws, just in the wrong shapes.
    let asked = doc
        .blocks
        .iter()
        .flat_map(|b| b.objects.iter())
        .filter(|o| matches!(o.kind, rubrica_doc::ObjectKind::Math { .. }))
        .count();
    if asked > 0 {
        let (set, bars, mut families, measured) = math.census();
        families.sort();
        families.dedup();
        println!(
            "math         : {asked} formulae, {set} set from [{}], {bars} rules drawn, {measured} measured from a MATH table",
            families.join(", ")
        );
    }
    // Counted two ways on purpose: the marks in the text are what the reader can
    // follow, the entries in `footnotes` are what the author defined, and a document
    // where those disagree is the interesting one -- an uncited definition still has
    // to be set, and a citation with no note behind it has nothing to show.
    let cited = {
        let mut numbers: Vec<usize> = Vec::new();
        for (text, s) in all_spans(&doc) {
            if s.style.contains(rubrica_doc::InlineStyle::SUPERSCRIPT) {
                if let Ok(n) = text[s.range.clone()].parse::<usize>() {
                    if !numbers.contains(&n) {
                        numbers.push(n);
                    }
                }
            }
        }
        numbers.len()
    };
    let defined = doc.footnotes.len();
    if cited > 0 || defined > 0 {
        let blocks: usize = doc.footnotes.iter().map(|f| f.blocks.len()).sum();
        println!("footnotes    : {cited} cited, {defined} defined, {blocks} blocks set");
    }
    // What a click can reach, counted on the page rather than in the source. A target
    // that wraps makes one rectangle per line, and a target the reader cannot follow
    // makes none at all -- so `rects` behind `ranges` by more than the wraps is a layout
    // that has lost a link, and `refused` climbing is a document full of `file:`.
    let ranges = all_actions(&doc).count();
    if ranges > 0 {
        // Every heading's address, in the order the page indexed them, so a fragment can
        // be asked whether anything on the page answers it.
        let slugs: Vec<String> = doc
            .blocks
            .iter()
            .filter(|b| matches!(b.kind, rubrica_doc::BlockKind::Heading(_)))
            .map(|b| crate::view::slug(&b.text))
            .collect();
        let refused = all_actions(&doc)
            .filter(|a| match &a.kind {
                rubrica_doc::ActionKind::Url(u) => {
                    !crate::view::openable(u)
                        && !u
                            .strip_prefix('#')
                            .is_some_and(|f| slugs.contains(&crate::view::slug(f)))
                        && crate::view::document_link(base, u).is_none()
                }
                _ => false,
            })
            .count();
        let links = page.hotspots.iter().filter(|h| matches!(h.kind, crate::view::HotKind::Url(_))).count();
        let jumps = page.hotspots.iter().filter(|h| matches!(h.kind, crate::view::HotKind::Cite(_))).count();
        let headings = page.hotspots.iter().filter(|h| matches!(h.kind, crate::view::HotKind::Heading(_))).count();
        let files = page.hotspots.iter().filter(|h| matches!(h.kind, crate::view::HotKind::Document(_))).count();
        // A fragment that names no heading is the interesting kind of dead link: the
        // address was spelled out by hand from a heading's own words, and one of the two
        // has since changed. Nothing on the page says so, so this does.
        let dead = all_actions(&doc)
            .filter(|a| match &a.kind {
                rubrica_doc::ActionKind::Url(u) => {
                    u.strip_prefix('#').is_some_and(|f| !slugs.contains(&crate::view::slug(f)))
                }
                _ => false,
            })
            .count();
        let on_page = slugs.len();
        println!(
            "targets      : {ranges} range(s) -> {links} link rect(s), {jumps} citation rect(s), {headings}/{on_page} heading jump(s), {files} document jump(s), {refused} refused"
        );
        if dead > 0 {
            println!("!! {dead} fragment link(s) name no heading on the page");
        }
        let tops: Vec<String> = page.note_tops.iter().map(|t| format!("{t:.1}")).collect();
        if !tops.is_empty() {
            println!("  notes at     : {} pt", tops.join(", "));
        }
    }
    // Struck runs are the only faint ink a page carries, so a faint rule in the op
    // list can only be a strike. Counted rather than trusted: a struck span that sets
    // no rule has lost the whole point of the mark, and one that sets a rule per
    // ideograph would be the same picture at a dozen times the ops.
    let struck = all_spans(&doc)
        .filter(|(_, s)| s.style.contains(rubrica_doc::InlineStyle::STRIKETHROUGH))
        .count();
    if struck > 0 {
        let rules: Vec<f32> = ops
            .iter()
            .filter_map(|op| match op {
                crate::view::Op::Rect { color: ColorRole::Faint, h, .. } => Some(*h / k),
                _ => None,
            })
            .collect();
        let thinnest = rules.iter().cloned().fold(f32::INFINITY, f32::min);
        println!(
            "strike       : {struck} span(s) -> {} rule(s), thinnest {thinnest:.2}pt",
            rules.len()
        );
        // Where each face on the page puts its own rule, in ems above the baseline:
        // read from `OS/2`, so a page that says these are all the same height has a
        // guessed number in it somewhere.
        let heights = census
            .keys()
            .filter_map(|fam| {
                let face = font.open_face(fam, 400, false)?;
                let (pos, weight) = font.strike_rule(face, theme.base);
                Some(format!("{fam} {:+.3}em/{:.2}pt", pos / theme.base, weight))
            })
            .collect::<Vec<_>>();
        println!("  heights    : {}", heights.join(", "));
    }
    // The selectable text of the page, counted against the ink it is meant to describe.
    // A block that lays out without recording its lines is selectable everywhere except
    // where it was just laid out, which is invisible in a screenshot and shows up here
    // as a char count far behind the glyph count.
    {
        let chars: usize = page.sel.iter().map(|l| l.chars.len()).sum();
        let glyphs: usize = census.values().sum();
        let malformed = page
            .sel
            .iter()
            .filter(|l| l.xs.len() != l.chars.len() + 1 || l.xs.windows(2).any(|w| w[1] < w[0]))
            .count();
        let unordered = page.sel.windows(2).filter(|w| w[1].y < w[0].y).count();
        println!(
            "select       : {} line(s), {chars} char(s) indexed against {glyphs} drawn, {malformed} malformed, {unordered} out of order",
            page.sel.len()
        );
        // Read back the way `Ctrl`+`A` and a copy would: an index that satisfies its
        // invariants but yields nothing when asked for everything is still no use.
        let all = crate::view::Selection {
            from: crate::view::Caret { line: 0, ch: 0 },
            to: crate::view::Caret {
                line: page.sel.len().saturating_sub(1),
                ch: page.sel.last().map_or(0, |l| l.chars.len()),
            },
        };
        let copied = crate::view::selection_text(&page.sel, all);
        let bands = crate::view::selection_rects(&page.sel, all).len();
        // The separators counted apart from the text: a page with a grid on it has to
        // answer with tabs, and one whose blocks run together has lost the boundary
        // between them -- neither of which is visible in a total character count.
        let lines = copied.matches('\n').count();
        let tabs = copied.matches('\t').count();
        println!(
            "  whole page : {} char(s) copied in {bands} band(s), {lines} line break(s), {tabs} tab(s)",
            copied.chars().count()
        );
    }
    Ok(())
}

/// Every clickable range the source asks for, prose and grid alike. A table's targets
/// are recorded on its cells, so counting only the blocks' would report a document
/// whose clickable text vanishes as soon as it is put in a table.
fn all_actions(doc: &rubrica_doc::Document) -> impl Iterator<Item = &rubrica_doc::Action> {
    all_blocks(doc).flat_map(|b| {
        let cells = b
            .table
            .iter()
            .flat_map(|t| t.head.iter().chain(t.rows.iter().flatten()))
            .flat_map(|c| &c.actions);
        b.actions.iter().chain(cells)
    })
}

/// Every styled run, with the text its range indexes: a cell's spans count into its
/// own cell's text, not into the block's.
fn all_spans(doc: &rubrica_doc::Document) -> impl Iterator<Item = (&str, &rubrica_doc::Span)> {
    all_blocks(doc).flat_map(|b| {
        let cells = b
            .table
            .iter()
            .flat_map(|t| t.head.iter().chain(t.rows.iter().flatten()))
            .map(|c| (c.text.as_str(), &c.spans));
        std::iter::once((b.text.as_str(), &b.spans))
            .chain(cells)
            .flat_map(|(text, spans)| spans.iter().map(move |s| (text, s)))
    })
}

/// The page's blocks plus the blocks of its notes: a footnote is set by the same
/// layout, so a census that stops at `blocks` cannot see what it does.
fn all_blocks(doc: &rubrica_doc::Document) -> impl Iterator<Item = &rubrica_doc::Block> {
    doc.blocks
        .iter()
        .chain(doc.footnotes.iter().flat_map(|f| &f.blocks))
}
