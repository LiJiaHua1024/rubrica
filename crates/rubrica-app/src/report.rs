//! Headless numeric verification of a rendered document.
//!
//! Typography is a visual property, but the failure modes that matter most are
//! measurable: a justified line that stops short of the measure, an overfull line
//! hanging past it, a run of Chinese set in a Latin face. This reports those
//! numbers through the *same* `build_ops` the window draws from, so it describes
//! what is actually painted rather than what a second implementation would have
//! produced -- and it never touches the desktop.
//!
//! Run: `rubrica-app --report [file.md] [--width 1080] [--dpi 96]`

use std::collections::BTreeMap;

use crate::font::FontEngine;
use crate::theme::Theme;
use crate::view::build_ops;
use crate::{Error, Result};

/// One text line reconstructed from its paint runs.
struct Line {
    left: f32,
    right: f32,
    families: Vec<String>,
}

pub fn report(source: &str, path: Option<&str>, width: f32, dpi: f32) -> Result<()> {
    let mut font = FontEngine::new().map_err(|e| -> Error { format!("DirectWrite: {e}").into() })?;
    if !font.probe() {
        return Err("no usable font face".into());
    }
    let theme = Theme::default();
    let doc = rubrica_doc::Document::parse(source);
    // No render target exists here, so figures are measured from their files
    // through WIC but not decoded to bitmaps -- enough to lay out and report.
    let _ = unsafe { windows::Win32::System::Com::CoInitializeEx(None, windows::Win32::System::Com::COINIT_MULTITHREADED) };
    let store = crate::images::ImageStore::new().ok();
    let base = path.map(std::path::Path::new).and_then(|p| p.parent());
    let (ops, height, column_pt, _left) =
        build_ops(&mut font, &theme, &doc, width, dpi, store.as_ref(), base);
    let k = dpi / 72.0;

    let mut lines: Vec<Line> = Vec::new();
    let mut census: BTreeMap<String, usize> = BTreeMap::new();
    for op in &ops {
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
    println!("document   : {}", path.unwrap_or("(built-in sample)"));
    println!("blocks     : {}   lines: {}   height {:.0}pt", doc.blocks.len(), lines.len(), height);
    println!("measure    : {:.1}pt ({:.1} em)  = {col_dip:.1}dip at {dpi}dpi", column_pt, column_pt / theme.base);
    println!();
    println!("  #      left    right    fill  faces");

    // A justified line must land on the measure; the last line of each block is
    // legitimately short, so report the tight cluster and the outliers separately.
    let mut fills: Vec<f32> = Vec::new();
    let mut off_measure = 0usize;
    for (i, l) in lines.iter().enumerate() {
        let fill = (l.right - l.left) / col_dip;
        let short = fill < 0.985;
        // Compare a width against a width: the right edge is absolute, so
        // subtracting the measure from it silently flags every centred line.
        let over = (l.right - l.left - col_dip) / k;
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
    println!("glyph census:");
    for (fam, n) in &census {
        println!("  {fam:<24} {n:>6} glyphs");
    }
    let han: usize = source.chars().filter(|c| matches!(*c as u32, 0x4E00..=0x9FFF)).count();
    let latin: usize = source.chars().filter(|c| c.is_ascii_alphabetic()).count();
    println!("source has {han} ideographs and {latin} latin letters");
    Ok(())
}
