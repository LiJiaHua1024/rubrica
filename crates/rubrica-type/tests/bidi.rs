use rubrica_type::{BidiInfo, BreakOptions, Spacing, StyleId, typeset};
use rubrica_type::justification::place_bidi;
use rubrica_type::paragraph::MonospaceMeasure;

fn visual(text: &str, width: f32) -> Vec<(String, f32)> {
    let mut measure = MonospaceMeasure { size: 10.0, factor: 1.0 };
    let mut options = BreakOptions::new(width);
    options.ragged = true;
    let (para, plan) = typeset(text, &Spacing::for_size(10.0), StyleId(0), &[], &options, &mut measure);
    let bidi = BidiInfo::new(text, None);
    plan.lines.iter().map(|line| {
        let slots = place_bidi(&para, line, &bidi);
        for pair in slots.windows(2) {
            assert!((pair[0].x + pair[0].w - pair[1].x).abs() < 0.001);
        }
        let mut result = String::new();
        for slot in &slots {
            let Some((start, end)) = slot.source else { continue };
            if slot.bidi_level % 2 == 1 {
                result.extend(text[start..end].chars().rev());
            } else {
                result.push_str(&text[start..end]);
            }
        }
        (result, slots.first().map_or(0.0, |s| s.x))
    }).collect()
}

#[test]
fn rtl_words_reverse_but_numbers_do_not() {
    let lines = visual("אב 123 גד", 200.0);
    assert_eq!(lines[0].0, "דג 123 בא");
    assert!(lines[0].1 > 0.0, "a short RTL paragraph aligns to its right edge");
}

#[test]
fn cjk_and_rtl_share_one_line_with_nested_ltr_digits() {
    assert_eq!(visual("中אב12גד文", 200.0)[0].0, "中דג12בא文");
    assert_eq!(visual("中مرحبا文", 200.0)[0].0, "中ابحرم文");
}

#[test]
fn wrapping_trims_the_logical_end_before_reordering() {
    let lines = visual("אב גד הו זח", 44.0);
    assert!(lines.len() > 1);
    assert!(lines.iter().all(|(s, _)| !s.starts_with(' ') && !s.ends_with(' ')));
    assert_eq!(lines.iter().map(|(s, _)| s.as_str()).collect::<Vec<_>>(), ["דג בא", "חז וה"]);
}

#[test]
fn paragraph_direction_resets_after_a_hard_break() {
    let lines = visual("אב\nabc", 200.0);
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0].0, "בא");
    assert!(lines[0].1 > 0.0);
    assert_eq!(lines[1], ("abc".to_owned(), 0.0));
}

#[test]
fn isolates_keep_numbers_and_outer_text_in_their_own_runs() {
    let result = visual("A \u{2067}אב 12\u{2069} Z", 200.0)[0].0.clone();
    let visible: String = result.chars().filter(|c| !matches!(c, '\u{2067}' | '\u{2069}')).collect();
    assert_eq!(visible, "A 12 בא Z");
}

#[test]
fn directional_cuts_do_not_introduce_word_breaks() {
    let lines = visual("abcאב123", 20.0);
    assert_eq!(lines.len(), 1, "UBA run boundaries are not UAX #14 opportunities");
    assert_eq!(lines[0].0, "abc123בא");
}
