//! Script classification, used to pick the glue recipe at each boundary.

use unicode_script::{Script, UnicodeScript};
use unicode_width::UnicodeWidthChar;

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

/// Which side of a pair carries the ink in a full-width punctuation glyph.
///
/// The advance can be shortened without narrowing the glyph itself: an opening mark
/// uses the blank on its left, while a closing mark uses the blank on its right.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PunctuationKind {
    Opening,
    Closing,
    Other,
}

impl Role {
    pub fn of(ch: char) -> Role {
        if is_cjk_punct(ch) {
            return Role::Cjk;
        }
        // A glyph the CJK face draws in a full em carries its own air, whichever script
        // property the character happens to carry. The two disagree more often than the
        // script alone suggests: `・` is `Common` and a fullwidth `Ａ` is `Latin`, so both
        // fall through to `Western` and were handed the quarter em that belongs to a
        // *narrow* Latin letter -- a second gap on top of the one their box already has.
        // `间隔・号` set as `间隔 ・ 号`, and `全角Ａ与Ｂ之间` as `全角 Ａ 与 Ｂ 之间`, the very
        // seam this crate exists to close. East Asian Width is what says which box a face
        // draws, and it is the property the glue recipe actually depends on.
        //
        // Ambiguous-width characters stay out of it. `—`, `…` and `·` are narrow in a
        // Latin face and wide in a CJK one, and the reader cannot see which face drew
        // them, so `is_shared_mark` reads them from their neighbours instead.
        if ch.width() == Some(2) {
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

/// A full-width mark whose blank half may be reclaimed, which is what makes it worth
/// segmenting on its own so the painter can shorten its advance.
///
/// The test is East Asian Width, not membership of the block: the block `0xFF5B..=0xFF65`
/// holds both `｛｝｜` -- full-width, ink at one end of an em box -- and `｢｣､｡･`, which
/// are *halfwidth* and fill their box with ink from edge to edge. Charging the second
/// group the first group's treatment slides an opening `｢` a quarter of an em to the
/// left, onto the character before it, and pulls the character after a closing `｣` into
/// its ink. The Ambiguous-width marks that share the block -- `“”‘’` and `·` -- fall out
/// of the same test for the reason they were excluded by hand: they are narrow in a
/// Latin face, so there is no blank half to reclaim.
pub fn is_compressible_punct(ch: char) -> bool {
    is_cjk_punct(ch) && ch.width() == Some(2)
}

pub fn punctuation(ch: char) -> PunctuationKind {
    match ch {
        '（' | '［' | '｛' | '〈' | '《' | '「' | '『' | '【' | '〔' | '〖' | '“' | '‘'
        | '｟' | '｢' => PunctuationKind::Opening,
        '）' | '］' | '｝' | '〉' | '》' | '」' | '』' | '】' | '〕' | '〗' | '”' | '’'
        | '｠' | '｣' | '、' | '。' | '，' | '．' | '！' | '？' | '；' | '：' => {
            PunctuationKind::Closing
        }
        ch if is_compressible_punct(ch) => PunctuationKind::Other,
        _ => PunctuationKind::Other,
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
