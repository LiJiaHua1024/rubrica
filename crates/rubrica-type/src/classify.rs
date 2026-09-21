//! Script classification, used to pick the glue recipe at each boundary.

use unicode_script::{Script, UnicodeScript};

/// Typographic role of a code point, deciding which inter-node glue applies.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Role {
    /// Han, Kana, Hangul and their native punctuation: breakable almost anywhere.
    Cjk,
    /// Latin, Greek, Cyrillic, digits and their punctuation: breakable only at
    /// UAX #14 opportunities.
    Western,
    /// Anything else (Thai, Arabic, Devanagari, most symbols). Never given the
    /// compressible CJK glue.
    Other,
}

impl Role {
    pub fn of(ch: char) -> Role {
        if is_cjk_punct(ch) {
            return Role::Cjk;
        }
        match ch.script() {
            Script::Han
            | Script::Hiragana
            | Script::Katakana
            | Script::Hangul
            | Script::Bopomofo
            | Script::Tangut
            | Script::Yi => Role::Cjk,
            Script::Latin | Script::Greek | Script::Cyrillic | Script::Common => Role::Western,
            _ => Role::Other,
        }
    }
}

/// CJK and fullwidth punctuation, which belongs to the surrounding Han run for
/// glue purposes even though its script property is `Common`.
///
/// Line-head and line-tail prohibitions -- 、。」 may not start a line, 「 may not
/// end one -- are deliberately not tabled here. UAX #14 already encodes them, and
/// a second table would only drift out of sync with the first.
pub fn is_cjk_punct(ch: char) -> bool {
    matches!(ch as u32,
        0x3000..=0x303F      // 、。〈〉《》「」『』【】〰
        | 0xFF01..=0xFF0F    // ！＂＃＄％＆＇（）＊＋，－．／
        | 0xFF1A..=0xFF20    // ：；＜＝＞？＠
        | 0xFF3B..=0xFF40    // ［＼］＾＿｀
        | 0xFF5B..=0xFF65    // ｛｜｝～｟｠｡｢｣､･
        | 0x2018..=0x201D    // ‘’“” ― quoted CJK-style in mixed text
        | 0x00B7             // ·
    )
}
