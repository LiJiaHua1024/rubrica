//! Terminal proof of line breaking: prints a paragraph as a grid with the measure
//! edge marked, so evenness and overfull lines are visible rather than asserted.
//!
//! Run: cargo run -p rubrica-type --example preview -- [--cjk] [--ems N]

use rubrica_type::paragraph::{Item, MonospaceMeasure, Spacing, StyleId};
use rubrica_type::units::Pt;
use rubrica_type::{BreakOptions, Paragraph, place, typeset};

const SIZE: Pt = 16.0;
const CELL: Pt = 8.0; // MonospaceMeasure { size: 16, factor: 0.5 }

const WESTERN: &str = "Typography is the art of arranging type so that written language stays legible and pleasant to read, which means every line has to be balanced against the ones around it instead of filled one at a time";

const CJK: &str = "中文排版是一件需要认真对待的事情。行首与行尾都要对齐，才能形成稳定的版面节奏，这也是一篇长文读起来舒适的前提。引擎必须在整段范围内权衡每一行的松紧，而不是逐行填满以后再接受由此产生的参差。";

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let use_cjk = args.iter().any(|a| a == "--cjk");
    let ems: Pt = args
        .iter()
        .position(|a| a == "--ems")
        .and_then(|i| args.get(i + 1))
        .and_then(|s| s.parse().ok())
        .unwrap_or(34.0);
    let indent: Pt = args
        .iter()
        .position(|a| a == "--indent")
        .and_then(|i| args.get(i + 1))
        .and_then(|s| s.parse().ok())
        .unwrap_or(0.0);

    let text = if use_cjk { CJK } else { WESTERN };
    let column = ems * SIZE;
    let spacing = Spacing::for_size(SIZE);
    let mut measure = MonospaceMeasure { size: SIZE, factor: 0.5 };
    let mut opts = BreakOptions::new(column);
    opts.par_indent = indent;
    let (para, plan) = typeset(text, &spacing, StyleId(0), &opts, &mut measure);

    println!(
        "measure {column:.0}pt ({ems:.1} em)  indent {indent:.0}   pass {}   demerits {:.1}",
        plan.pass, plan.demerits
    );
    println!("cells/line {:.1}", column / CELL);
    println!("{}", "-".repeat((column / CELL) as usize + 2));
    if plan.lines.is_empty() {
        println!("!! no lines produced");
        return;
    }
    for l in plan.lines.iter() {
        let placed = place(&para, l);
        let right = placed.last().map(|p| p.x + p.w).unwrap_or(0.0);
        let body = content(&para, text, l);
        let cells = (right / CELL).round() as usize;
        println!(
            "{body}<{}{}r={cells} nat={:.1} stretch={:.1} bad={} fit={}>",
            if l.forced { '\\' } else { ' ' },
            if l.is_ragged() { "ragged" } else { "just" },
            l.natural,
            l.stretch.min(9999.0),
            l.badness,
            l.fitness,
        );

    }
    println!("{}", "-".repeat((column / CELL) as usize + 2));
    let ragged = plan.lines.len() - 1;
    let widths: Vec<f64> = plan.lines[..ragged]
        .iter()
        .map(|l| {
            let placed = place(&para, l);
            (placed.last().map(|p| p.x + p.w).unwrap_or(0.0) / column).into()
        })
        .collect();
    if widths.is_empty() {
        println!("only one line: nothing was justified");
    } else {
        println!("fill ratios: {}", widths.iter().map(|w| format!("{w:.4}")).collect::<Vec<_>>().join(" "));
        let mean = widths.iter().sum::<f64>() / widths.len() as f64;
        let var = widths.iter().map(|w| (w - mean) * (w - mean)).sum::<f64>() / widths.len() as f64;
        println!("mean {mean:.4}  variance {var:.6}  (variance is the thing to minimize)");
    }
}

fn content(para: &Paragraph, src: &str, line: &rubrica_type::Line) -> String {
    let mut s = String::new();
    for i in line.items.clone() {
        match para.items[i] {
            Item::Box { node } => {
                let n = para.node(node);
                s.push_str(&src[n.text.clone()]);
            }
            Item::Glue { .. } => s.push(' '),
            Item::Penalty { .. } => {}
        }
    }
    s
}
