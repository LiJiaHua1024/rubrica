//! Which of a code block's words is which.
//!
//! A fence arrives as one run of ink in one size, which is the right answer for a
//! quotation and the wrong one for a program: the eye reads source by its shape -- where
//! the sayings are, where the data, and which of it is a note to the reader rather than
//! an instruction to the machine -- and a wall of uniform grey has no shape at all.
//!
//! So this cuts a block's text into the five kinds the page has an ink for, and asks
//! nothing of the language beyond what is on the page. There is no parser here, and that
//! is deliberate. A parser needs the grammar of every language a document might hold and
//! is wrong in interesting ways the moment it meets a dialect it was not told about,
//! while the questions actually being asked of this text are three: does a quote close on
//! this line, where does a comment end, and is this word one of the language's own. A
//! scanner answers all three from the characters in front of it, and the difference
//! between it and a parser shows up only in code that would not compile -- which nobody
//! is reading for its highlighting.
//!
//! Unknown languages are left entirely alone. A wrong guess colours a string as prose and
//! a division as a comment, and a reader who asked for `kotlin` in a build that has not
//! heard of it is better off with the plain page than with a confident lie about it.

use std::ops::Range;

use crate::theme::ColorRole;

/// Cut a block's text into tokens, and give back the spaces at their tails.
///
/// A shaped run that ends in a space can lose it, and the page then sets `let x` as
/// `letx` -- one glyph short of the source and legible for a moment before it is not.
/// The space belongs to whichever run follows, which is the block's own plain ink.
fn scan(text: &str, s: &Spec) -> Vec<(Range<usize>, ColorRole)> {
    let mut out = if s.markup { markup(text) } else { plain_scan(text, s) };
    for (r, _) in out.iter_mut() {
        while r.end > r.start && matches!(text.as_bytes()[r.end - 1], b' ' | b'\t') {
            r.end -= 1;
        }
    }
    out.retain(|(r, _)| r.start < r.end);
    out
}

/// A block's text, cut into the runs that want another ink.
///
/// Ranges are byte offsets into `text`, never overlapping and in order, and only the
/// coloured runs appear: what is not here stays the block's own colour, so a caller that
/// paints what it is handed cannot lose a character. An empty answer means either that
/// nothing in the block is anything in particular, or that [`languages`] has not heard of
/// `lang` -- the page sets both as plain source, which is the honest rendering.
pub fn tokens(lang: &str, text: &str) -> Vec<(Range<usize>, ColorRole)> {
    let Some(s) = spec(lang) else { return Vec::new() };
    scan(text, s)
}

/// The scan every family but mark-up is read by.
fn plain_scan(text: &str, s: &Spec) -> Vec<(Range<usize>, ColorRole)> {
    let b = text.as_bytes();
    let mut out: Vec<(Range<usize>, ColorRole)> = Vec::new();
    // The loop's one invariant: `i` is always a character boundary, so the string
    // slices below are legal. Every branch advances to a token's end, which is a
    // boundary because a token's delimiters are ASCII, and the last advance is a whole
    // character rather than a byte.
    let mut i = 0usize;
    while i < b.len() {
        // Longest first: `/*` before a division, and a tripled quote before the single
        // one it is made of, or a Python string opens on its own second character.
        if let Some((open, close)) = s.block.iter().copied().find(|(o, _)| b[i..].starts_with(o.as_bytes())) {
            let rest = i + open.len();
            let end = text[rest..]
                .find(close)
                .map(|p| rest + p + close.len())
                .unwrap_or(b.len());
            push(&mut out, i..end, ColorRole::Comment);
            i = end;
            continue;
        }
        if s.line.iter().any(|o| b[i..].starts_with(o.as_bytes())) {
            let end = text[i..].find('\n').map(|p| i + p).unwrap_or(b.len());
            push(&mut out, i..end, ColorRole::Comment);
            i = end;
            continue;
        }
        // The C preprocessor's line and the shell's shebang are both one instruction
        // written in the language's own voice, and both run to the end of the line.
        if s.preproc && b[i] == b'#' && at_line_start(text, i) {
            let end = text[i..].find('\n').map(|p| i + p).unwrap_or(b.len());
            push(&mut out, i..end, ColorRole::Keyword);
            i = end;
            continue;
        }
        if let Some(t) = s.triples.iter().find(|t| b[i..].starts_with(t.as_bytes())) {
            let rest = i + t.len();
            if let Some(p) = text[rest..].find(*t) {
                let end = rest + p + t.len();
                push(&mut out, i..end, ColorRole::String);
                i = end;
                continue;
            }
            // Never closed, so it is three of them rather than one of those, and the
            // plain quote rule below reads it as such.
        }
        let ch = b[i];
        if s.quotes.as_bytes().contains(&ch) {
            // A character literal is bounded by length as well as by its close, so a
            // lifetime's apostrophe cannot open a string running to the next one.
            let limit = if s.char_lit && ch == b'\'' { Some(4) } else { None };
            if let Some(end) = string_end(text, i, ch, s.escapes, limit) {
                let role = if s.keys && is_key(text, end) { ColorRole::Type } else { ColorRole::String };
                push(&mut out, i..end, role);
                i = end;
                continue;
            }
        }
        if ch.is_ascii_digit() {
            let end = number_end(b, i);
            push(&mut out, i..end, ColorRole::Number);
            i = end;
            continue;
        }
        // `@media`, `@override`, `@Controller`: a directive, wherever it stands.
        if s.at_rule
            && (ch == b'@' || ch == b'#')
            && b.get(i + 1).is_some_and(|c| c.is_ascii_alphabetic())
        {
            let end = word_end(text, i + 1);
            push(&mut out, i..end, ColorRole::Keyword);
            i = end;
            continue;
        }
        if s.dollar && ch == b'$' {
            let end = word_end(text, i + 1);
            if end > i + 1 {
                push(&mut out, i..end, ColorRole::Type);
                i = end;
                continue;
            }
            // `$` alone is the shell's own name, and one character is not worth an ink.
        }
        if is_word_start(ch) {
            let end = word_end(text, i);
            if end > i {
                if let Some(role) = classify(&text[i..end], s) {
                    push(&mut out, i..end, role);
                }
                i = end;
                continue;
            }
        }
        i += text[i..].chars().next().map_or(1, |c| c.len_utf8());
    }
    out
}

/// Whether a fence's language name reaches a family here, so the report can say which
/// of the page's code was read as a language and which was set plain as a fact about
/// this build rather than a choice about the document.
pub fn knows(lang: &str) -> bool {
    spec(lang).is_some()
}

/// A word is one kind of thing, or nothing worth an ink.
fn classify(word: &str, s: &Spec) -> Option<ColorRole> {
    // `SELECT` and `select` are one word to a language that gave up caring, and every
    // table below holds one spelling of each of its entries.
    let probe = if s.insensitive { word.to_lowercase() } else { word.to_string() };
    if s.keywords.contains(&&probe[..]) {
        return Some(ColorRole::Keyword);
    }
    if s.types.contains(&&probe[..]) {
        return Some(ColorRole::Type);
    }
    // A name begun with a capital is a type in every family that has types at all, which
    // is what colours a struct the author invented as well as the ones on the list. The
    // shell has no such convention, and `IF` there is a typo rather than a class.
    if s.capitals && word.chars().next().is_some_and(|c| c.is_uppercase()) {
        return Some(ColorRole::Type);
    }
    None
}

/// Whether a colon is the next thing a reader would see.
fn is_key(text: &str, after: usize) -> bool {
    text[after..]
        .chars()
        .find(|c| !c.is_whitespace())
        .is_some_and(|c| c == ':')
}

/// Whether only space stands between this position and the line's start.
fn at_line_start(text: &str, i: usize) -> bool {
    match text[..i].bytes().rev().find(|b| *b != b' ' && *b != b'\t') {
        Some(b'\n') | None => true,
        Some(_) => false,
    }
}

/// Where a string opened at `at` ends, or `None` when it does not.
///
/// Not on the line it opened on is not a string: an open quote is a typo or an
/// apostrophe, and swallowing the rest of a block for it would colour half a page wrong
/// to no purpose.
fn string_end(text: &str, at: usize, quote: u8, escapes: bool, limit: Option<usize>) -> Option<usize> {
    let b = text.as_bytes();
    let mut p = at + 1;
    while p < b.len() {
        if escapes && b[p] == b'\\' {
            p += 2;
            continue;
        }
        if b[p] == b'\n' {
            return None;
        }
        if b[p] == quote {
            let end = p + 1;
            return if limit.is_some_and(|l| end - at > l) { None } else { Some(end) };
        }
        p += 1;
    }
    None
}

/// How much of a number `at` is: its radix, its point, its exponent, and the unit or
/// suffix that belongs to it -- `0xFF_u32`, `1.5e-3`, `2.5rem`, `10px`.
fn number_end(b: &[u8], at: usize) -> usize {
    let n = b.len();
    let mut p = at;
    if b[p] == b'0' && p + 1 < n && matches!(b[p + 1], b'x' | b'X' | b'b' | b'B' | b'o' | b'O') {
        p += 2;
        while p < n && (b[p].is_ascii_alphanumeric() || b[p] == b'_') {
            p += 1;
        }
        return p;
    }
    while p < n && (b[p].is_ascii_digit() || b[p] == b'_') {
        p += 1;
    }
    // A point with a figure after it is the number's own, and a version is one number
    // rather than three: `1.2.3` is a thing a reader reads at a glance.
    while p + 1 < n && b[p] == b'.' && b[p + 1].is_ascii_digit() {
        p += 1;
        while p < n && (b[p].is_ascii_digit() || b[p] == b'_') {
            p += 1;
        }
    }
    if p < n && matches!(b[p], b'e' | b'E') {
        let mut q = p + 1;
        if q < n && matches!(b[q], b'+' | b'-') {
            q += 1;
        }
        if q < n && b[q].is_ascii_digit() {
            p = q;
            while p < n && (b[p].is_ascii_digit() || b[p] == b'_') {
                p += 1;
            }
        }
    }
    if p < n && (b[p].is_ascii_alphabetic() || b[p] == b'_') {
        while p < n && (b[p].is_ascii_alphanumeric() || b[p] == b'_') {
            p += 1;
        }
    }
    p
}

/// A word begins with a letter of any alphabet or with an underscore. The high bytes are
/// a lead byte's range -- a continuation byte is not a word's first character, and an
/// ASCII lead byte is 0x80 at the very most.
fn is_word_start(ch: u8) -> bool {
    ch.is_ascii_alphabetic() || ch == b'_' || ch >= 0xC2
}

/// The end of the word at `at`, whose letters may be of any alphabet: what counts as a
/// letter is the alphabet's business rather than ASCII's.
fn word_end(text: &str, at: usize) -> usize {
    let mut end = at;
    for c in text[at..].chars() {
        if !c.is_alphanumeric() && c != '_' {
            break;
        }
        end += c.len_utf8();
    }
    end
}

/// Mark-up: a name between angle brackets, its attributes, and the comment that is not a
/// comment in any other family here.
///
/// Its own scan because a tag's rules are not a program's. Inside `<...>` a word is a
/// thing or a property of a thing and there are no keywords at all, and outside it the
/// only marks are a comment, an entity, and the next tag.
fn markup(text: &str) -> Vec<(Range<usize>, ColorRole)> {
    let b = text.as_bytes();
    let mut out: Vec<(Range<usize>, ColorRole)> = Vec::new();
    let mut i = 0usize;
    while i < b.len() {
        if b[i..].starts_with(b"<!--") {
            let rest = i + 4;
            let end = text[rest..].find("-->").map(|p| rest + p + 3).unwrap_or(b.len());
            push(&mut out, i..end, ColorRole::Comment);
            i = end;
            continue;
        }
        // `&amp;` and `&#39;` are a character written as a saying about it, which is
        // what an entity is: neither the page's prose nor its ink, but a value.
        if b[i] == b'&' {
            let end = word_end(text, i + 1);
            if end > i + 1 && b.get(end) == Some(&b';') {
                push(&mut out, i..end + 1, ColorRole::Number);
                i = end + 1;
                continue;
            }
        }
        // A `<` that begins neither a name nor a slash is the character rather than a
        // tag, and `a < b` in prose keeps its right-hand side.
        let opens = b.get(i + 1).is_some_and(|c| *c == b'/' || c.is_ascii_alphabetic());
        if b[i] == b'<' && opens {
            let mut p = i + 1;
            if b.get(p) == Some(&b'/') {
                p += 1;
            }
            let name = p;
            while p < b.len() && (b[p].is_ascii_alphanumeric() || matches!(b[p], b'-' | b':' | b'_' | b'.')) {
                p += 1;
            }
            if p > name {
                // The bracket is part of the name's ink: `</p>` is one thing, not a
                // punctuation mark with an opinion about it.
                push(&mut out, i..p, ColorRole::Type);
                i = tag_attrs(text, p, &mut out);
                continue;
            }
        }
        i += text[i..].chars().next().map_or(1, |c| c.len_utf8());
    }
    out
}

/// The rest of a tag, from after its name to its closing bracket.
fn tag_attrs(text: &str, from: usize, out: &mut Vec<(Range<usize>, ColorRole)>) -> usize {
    let b = text.as_bytes();
    let mut i = from;
    while i < b.len() && b[i] != b'>' {
        let ch = b[i];
        if (ch == b'"' || ch == b'\'') && string_end(text, i, ch, false, None).is_some() {
            let end = string_end(text, i, ch, false, None).unwrap();
            push(out, i..end, ColorRole::String);
            i = end;
            continue;
        }
        if is_word_start(ch) {
            let end = word_end(text, i);
            // A word with an `=` after it is a property of the element. One without is
            // its value, and a value inside a tag means no more here than the prose
            // between the tags does.
            if text[end..].trim_start().starts_with('=') {
                push(out, i..end, ColorRole::Keyword);
            }
            i = end;
            continue;
        }
        i += text[i..].chars().next().map_or(1, |c| c.len_utf8());
    }
    i
}

/// Add a run, joining it to the one before when the two are of one kind and touch: a
/// comment's words are not four tokens because the scanner looked at them in pieces.
fn push(out: &mut Vec<(Range<usize>, ColorRole)>, range: Range<usize>, role: ColorRole) {
    if range.start >= range.end {
        return;
    }
    if let Some(last) = out.last_mut() {
        if last.1 == role && last.0.end == range.start {
            last.0.end = range.end;
            return;
        }
    }
    out.push((range, role));
}

/// What one family's source looks like: where its remarks are, where its strings, and
/// which of its words are its own rather than an author's.
#[derive(Clone, Copy)]
struct Spec {
    line: &'static [&'static str],
    block: &'static [(&'static str, &'static str)],
    /// The characters that open a string.
    quotes: &'static str,
    /// Quotes a language triples so its strings can hold the others.
    triples: &'static [&'static str],
    /// A backslash hides whatever follows it.
    escapes: bool,
    /// A `'` is one character rather than an open quote.
    char_lit: bool,
    keywords: &'static [&'static str],
    types: &'static [&'static str],
    /// A `#` at a line's start speaks for the whole line, as the C preprocessor's does.
    preproc: bool,
    /// `@media`, `@override`: an at-sign names a directive.
    at_rule: bool,
    /// A capital letter names a type.
    capitals: bool,
    /// `SELECT` is `select`.
    insensitive: bool,
    /// A string before a colon is a name rather than a value.
    keys: bool,
    /// Angle brackets rather than assignments.
    markup: bool,
    /// `$HOME`: the shell's variables.
    dollar: bool,
}

const BASE: Spec = Spec {
    line: &[],
    block: &[],
    quotes: "\"",
    triples: &[],
    escapes: true,
    char_lit: false,
    keywords: &[],
    types: &[],
    preproc: false,
    at_rule: false,
    capitals: true,
    insensitive: false,
    keys: false,
    markup: false,
    dollar: false,
};

/// Every family, and the names a fence may reach it by. A family shared by several
/// spellings is the same scanner, not a table of near-duplicates: `ts` and `typescript`
/// are one language written two ways.
const SPECS: &[(&str, &Spec)] = &[
    ("bash", &SHELL),
    ("c", &C),
    ("cmake", &SHELL),
    ("console", &SHELL),
    ("cpp", &CPP),
    ("cs", &CS),
    ("css", &CSS),
    ("dart", &CS),
    ("dockerfile", &SHELL),
    ("go", &GO),
    ("h", &C),
    ("hpp", &CPP),
    ("htm", &HTML),
    ("html", &HTML),
    ("ini", &INI),
    ("java", &JAVA),
    ("js", &JS),
    ("json", &JSON),
    ("jsonc", &JSONC),
    ("jsx", &JS),
    ("kotlin", &JAVA),
    ("less", &CSS),
    ("lua", &LUA),
    ("makefile", &SHELL),
    ("php", &PHP),
    ("py", &PY),
    ("python", &PY),
    ("rb", &RUBY),
    ("ron", &RUST),
    ("rs", &RUST),
    ("rust", &RUST),
    ("sass", &CSS),
    ("scala", &JAVA),
    ("scss", &CSS),
    ("sh", &SHELL),
    ("shell", &SHELL),
    ("sql", &SQL),
    ("svg", &HTML),
    ("swift", &CS),
    ("toml", &TOML),
    ("ts", &JS),
    ("tsx", &JS),
    ("typescript", &JS),
    ("xml", &HTML),
    ("yaml", &YAML),
    ("yml", &YAML),
    ("zsh", &SHELL),
];

fn spec(lang: &str) -> Option<&'static Spec> {
    SPECS.iter().find(|(n, _)| *n == lang).map(|(_, s)| *s)
}

const KW_RUST: &[&str] = &[
    "as", "async", "await", "break", "const", "continue", "crate", "dyn", "else", "enum",
    "extern", "false", "fn", "for", "if", "impl", "in", "let", "loop", "match", "mod", "move",
    "mut", "pub", "ref", "return", "self", "static", "struct", "super", "trait", "true", "type",
    "unsafe", "use", "where", "while",
];
const TY_RUST: &[&str] = &[
    "bool", "char", "f32", "f64", "i128", "i16", "i32", "i64", "i8", "isize", "str", "usize",
];

const KW_C: &[&str] = &[
    "auto", "break", "case", "const", "continue", "default", "do", "else", "enum", "extern",
    "for", "goto", "if", "inline", "register", "restrict", "return", "sizeof", "static",
    "struct", "switch", "typedef", "union", "volatile", "while",
];
const TY_C: &[&str] = &["char", "double", "float", "int", "long", "short", "signed", "unsigned", "void"];

const KW_CPP: &[&str] = &[
    "alignas", "alignof", "asm", "break", "case", "catch", "class", "const", "constexpr",
    "continue", "decltype", "default", "delete", "do", "else", "enum", "explicit", "export",
    "extern", "false", "for", "friend", "goto", "if", "inline", "mutable", "namespace", "new",
    "noexcept", "operator", "override", "private", "protected", "public", "return", "sizeof",
    "static", "static_cast", "struct", "switch", "template", "this", "throw", "true", "try",
    "typedef", "typename", "union", "using", "virtual", "volatile", "while",
];

const KW_CS: &[&str] = &[
    "abstract", "as", "base", "break", "case", "catch", "class", "const", "continue", "default",
    "delegate", "do", "else", "enum", "event", "explicit", "extern", "false", "finally", "for",
    "foreach", "get", "goto", "if", "implicit", "in", "interface", "internal", "is", "lock",
    "namespace", "new", "null", "operator", "out", "override", "params", "private", "protected",
    "public", "readonly", "ref", "return", "sealed", "set", "static", "struct", "switch", "this",
    "throw", "true", "try", "typeof", "using", "var", "virtual", "volatile", "while", "yield",
];
const TY_CS: &[&str] = &["bool", "byte", "char", "decimal", "double", "float", "int", "long", "object", "sbyte", "short", "string", "uint", "ulong", "ushort"];

const KW_GO: &[&str] = &[
    "break", "case", "chan", "const", "continue", "default", "defer", "else", "fallthrough",
    "for", "func", "go", "goto", "if", "import", "interface", "map", "nil", "package", "range",
    "return", "select", "struct", "switch", "type", "var",
];
const TY_GO: &[&str] = &[
    "bool", "byte", "complex64", "complex128", "error", "float32", "float64", "int", "int8",
    "int16", "int32", "int64", "rune", "string", "uint", "uint8", "uint16", "uint32", "uint64",
    "uintptr",
];

const KW_JAVA: &[&str] = &[
    "abstract", "assert", "break", "byte", "case", "catch", "char", "class", "const", "continue",
    "default", "do", "double", "else", "enum", "extends", "final", "finally", "float", "for",
    "goto", "if", "implements", "import", "instanceof", "int", "interface", "long", "native",
    "new", "null", "package", "private", "protected", "public", "record", "return", "sealed",
    "short", "static", "super", "switch", "synchronized", "this", "throw", "throws", "transient",
    "try", "var", "volatile", "while",
];

const KW_JS: &[&str] = &[
    "async", "await", "break", "case", "catch", "class", "const", "continue", "debugger",
    "default", "delete", "do", "else", "export", "extends", "false", "finally", "for", "from",
    "function", "get", "if", "import", "in", "instanceof", "interface", "let", "new", "null",
    "of", "return", "set", "static", "super", "switch", "this", "throw", "true", "try", "typeof",
    "undefined", "var", "void", "while", "with", "yield",
];

const KW_PY: &[&str] = &[
    "and", "as", "assert", "async", "await", "break", "class", "continue", "def", "del", "elif",
    "else", "except", "finally", "for", "from", "global", "if", "import", "in", "is", "lambda",
    "None", "not", "or", "pass", "raise", "return", "self", "True", "False", "try", "while",
    "with", "yield",
];
const TY_PY: &[&str] = &["bool", "dict", "float", "int", "list", "object", "set", "str", "tuple"];

const KW_RUBY: &[&str] = &[
    "alias", "and", "begin", "break", "case", "class", "def", "defined", "do", "else", "elsif",
    "end", "ensure", "false", "for", "if", "in", "module", "next", "nil", "not", "or", "redo",
    "require", "rescue", "retry", "return", "self", "super", "then", "true", "undef", "unless",
    "until", "when", "while", "yield",
];

const KW_PHP: &[&str] = &[
    "abstract", "and", "array", "as", "break", "case", "catch", "class", "clone", "const",
    "continue", "declare", "default", "do", "echo", "else", "elseif", "extends", "false",
    "finally", "for", "foreach", "function", "global", "if", "implements", "include",
    "instanceof", "interface", "isset", "list", "namespace", "new", "null", "or", "print",
    "private", "protected", "public", "require", "return", "static", "switch", "throw", "trait",
    "true", "try", "unset", "use", "var", "while", "xor", "yield",
];

const KW_SHELL: &[&str] = &[
    "alias", "begin", "break", "case", "cd", "command", "continue", "do", "done", "echo", "elif",
    "else", "end", "esac", "eval", "exec", "exit", "export", "fi", "for", "function", "if", "in",
    "local", "readonly", "return", "select", "set", "shift", "source", "then", "time", "trap",
    "until", "unset", "while",
];

const KW_SQL: &[&str] = &[
    "add", "all", "alter", "and", "any", "as", "asc", "begin", "between", "by", "case", "cast",
    "column", "commit", "constraint", "create", "cross", "default", "delete", "desc", "distinct",
    "drop", "else", "end", "exists", "foreign", "from", "full", "grant", "group", "having", "if",
    "in", "index", "inner", "insert", "into", "is", "join", "key", "left", "like", "limit", "not",
    "null", "offset", "on", "or", "order", "outer", "primary", "references", "returning", "right",
    "rollback", "select", "set", "table", "then", "union", "unique", "update", "using", "values",
    "view", "when", "where", "with",
];

const KW_JSON: &[&str] = &["false", "null", "true"];

const KW_CSS: &[&str] = &[
    "and", "charset", "counter", "each", "else", "extend", "face", "if", "import", "include",
    "keyframes", "media", "mixin", "namespace", "page", "supports", "use", "var",
];

const KW_LUA: &[&str] = &[
    "and", "break", "do", "else", "elseif", "end", "false", "for", "function", "goto", "if", "in",
    "local", "nil", "not", "or", "repeat", "return", "then", "true", "until", "while",
];

const RUST: Spec = Spec {
    line: &["//"],
    block: &[("/*", "*/")],
    quotes: "\"'",
    char_lit: true,
    keywords: KW_RUST,
    types: TY_RUST,
    ..BASE
};

const C: Spec = Spec {
    line: &["//"],
    block: &[("/*", "*/")],
    keywords: KW_C,
    types: TY_C,
    preproc: true,
    ..BASE
};

const CPP: Spec = Spec { keywords: KW_CPP, ..C };

const CS: Spec = Spec {
    line: &["//"],
    block: &[("/*", "*/")],
    keywords: KW_CS,
    types: TY_CS,
    at_rule: true,
    ..BASE
};

const GO: Spec = Spec {
    line: &["//"],
    block: &[("/*", "*/")],
    keywords: KW_GO,
    types: TY_GO,
    ..BASE
};

const JAVA: Spec = Spec {
    line: &["//"],
    block: &[("/*", "*/")],
    keywords: KW_JAVA,
    at_rule: true,
    ..BASE
};

const JS: Spec = Spec {
    line: &["//"],
    block: &[("/*", "*/")],
    quotes: "\"'`",
    keywords: KW_JS,
    ..BASE
};

const PY: Spec = Spec {
    line: &["#"],
    quotes: "\"'",
    triples: &["\"\"\"", "'''"],
    keywords: KW_PY,
    types: TY_PY,
    ..BASE
};

const RUBY: Spec = Spec {
    line: &["#"],
    block: &[("=begin", "=end")],
    quotes: "\"'",
    keywords: KW_RUBY,
    ..BASE
};

const PHP: Spec = Spec {
    line: &["//", "#"],
    block: &[("/*", "*/")],
    quotes: "\"'",
    keywords: KW_PHP,
    ..BASE
};

const SHELL: Spec = Spec {
    line: &["#"],
    quotes: "\"'",
    keywords: KW_SHELL,
    capitals: false,
    dollar: true,
    ..BASE
};

const SQL: Spec = Spec {
    line: &["--"],
    block: &[("/*", "*/")],
    quotes: "'",
    keywords: KW_SQL,
    capitals: false,
    insensitive: true,
    ..BASE
};

const JSON: Spec = Spec { keywords: KW_JSON, capitals: false, keys: true, ..BASE };

/// JSON with the comments a config file grows: the values are still keys and strings,
/// and a `//` is finally what it looks like.
const JSONC: Spec = Spec { line: &["//"], block: &[("/*", "*/")], ..JSON };

const YAML: Spec = Spec {
    line: &["#"],
    quotes: "\"'",
    keywords: &["false", "no", "null", "true", "yes"],
    capitals: false,
    keys: true,
    ..BASE
};

const TOML: Spec = Spec {
    line: &["#"],
    quotes: "\"'",
    triples: &["\"\"\"", "'''"],
    keywords: &["false", "true"],
    capitals: false,
    keys: true,
    ..BASE
};

/// `key = value`, and no quoting rules worth the name: an INI file's value is whatever
/// follows the sign, so a bare word gets the page's own ink and only a remark stands out.
const INI: Spec = Spec { line: &["#", ";"], keywords: &["false", "true"], capitals: false, ..BASE };

const CSS: Spec = Spec {
    block: &[("/*", "*/")],
    quotes: "\"'",
    keywords: KW_CSS,
    at_rule: true,
    capitals: false,
    ..BASE
};

const LUA: Spec = Spec {
    line: &["--"],
    block: &[("--[[", "]]")],
    quotes: "\"'",
    keywords: KW_LUA,
    ..BASE
};

const HTML: Spec = Spec { markup: true, ..BASE };

#[cfg(test)]
mod tests {
    use super::*;

    /// Every run this scanner reports, as the text it names.
    fn marked<'a>(lang: &str, text: &'a str) -> Vec<(&'a str, ColorRole)> {
        tokens(lang, text)
            .into_iter()
            .map(|(r, c)| (&text[r.start..r.end], c))
            .collect()
    }

    fn kinds(lang: &str, text: &str) -> Vec<ColorRole> {
        marked(lang, text).into_iter().map(|(_, c)| c).collect()
    }

    fn role_of(lang: &str, word: &str) -> Option<ColorRole> {
        marked(lang, word).first().map(|(_, c)| *c)
    }

    #[test]
    fn a_remark_is_one_ink_from_here_to_the_end_of_the_line() {
        use ColorRole::{Comment, Keyword, Number};
        assert_eq!(marked("rust", "// note"), vec![("// note", Comment)]);
        assert_eq!(
            marked("rust", "// note\nlet x = 1;"),
            vec![("// note", Comment), ("let", Keyword), ("1", Number)],
            "and not one byte of the line after it"
        );
        assert_eq!(kinds("c", "/* both\nlines */"), vec![Comment]);
        assert_eq!(kinds("c", "/* unclosed"), vec![Comment], "to the end of the block, which is where it is");
        assert_eq!(kinds("py", "# hash"), vec![Comment]);
        assert_eq!(kinds("sql", "-- twice"), vec![Comment]);
        assert_eq!(kinds("html", "<!-- hello -->"), vec![Comment]);
    }

    #[test]
    fn a_string_keeps_the_markers_inside_it() {
        // The reason this is a scan rather than a search for each marker in turn: half
        // the lines in a real program are comments *about* the syntax, and a splitter
        // that finds them there cuts the block in the wrong places.
        let src = r#"let url = "http://example.com"; // the rest"#;
        assert_eq!(
            marked("rust", src),
            vec![
                ("let", ColorRole::Keyword),
                ("\"http://example.com\"", ColorRole::String),
                ("// the rest", ColorRole::Comment),
            ]
        );
    }

    #[test]
    fn the_languages_own_words_are_theirs_and_nobody_elses() {
        assert_eq!(role_of("rust", "fn"), Some(ColorRole::Keyword));
        assert_eq!(role_of("js", "function"), Some(ColorRole::Keyword));
        assert_eq!(role_of("rust", "function"), None, "JS's word in Rust's file is an identifier");
        assert_eq!(role_of("py", "def"), Some(ColorRole::Keyword));
        assert_eq!(role_of("go", "func"), Some(ColorRole::Keyword));
        // A name the author invented, begun with a capital, is a type's name.
        assert_eq!(role_of("rust", "Widget"), Some(ColorRole::Type));
        assert_eq!(role_of("rust", "widget"), None);
        assert_eq!(role_of("shell", "IF"), None, "and the shell has no such convention");
    }

    #[test]
    fn a_language_that_does_not_care_who_shifted_is_read_the_same_either_way() {
        assert_eq!(kinds("sql", "SELECT * FROM t"), kinds("sql", "select * from t"));
        assert_eq!(role_of("sql", "SELECT"), Some(ColorRole::Keyword));
    }

    #[test]
    fn a_number_is_one_token_not_six() {
        for src in ["42", "0xFF", "0b1010", "1_000", "1.5", "1.5e-3", "2.5rem", "3f", "1.2.3"] {
            assert_eq!(marked("rust", src), vec![(src, ColorRole::Number)], "{src}");
        }
        // A point that belongs to the method after it is not a decimal point, and the
        // argument inside the call is still a number of its own.
        assert_eq!(marked("js", "1..toFixed(2)"), vec![("1", ColorRole::Number), ("2", ColorRole::Number)]);
    }

    #[test]
    fn an_apostrophe_that_opens_nothing_does_not_close_everything() {
        use ColorRole::{Keyword, String, Type};
        // Rust's lifetime is the hard case: the quote rule would otherwise find a
        // closing `'` somewhere down the block and paint forty lines as one string.
        assert_eq!(
            marked("rust", "fn f<'a>(x: &'a str) -> &'a str { x }"),
            vec![("fn", Keyword), ("str", Type), ("str", Type)]
        );
        assert_eq!(role_of("rust", "'a'"), Some(String), "and a real one is");
        assert_eq!(
            marked("py", "s = 'a' + 'b'"),
            vec![("'a'", String), ("'b'", String)],
            "Python has no lifetimes, so a pair of quotes is a string there"
        );
    }

    #[test]
    fn an_unclosed_quote_is_a_typo_rather_than_a_string() {
        let src = "let a = \"oops\nlet b = 2;";
        assert_eq!(
            marked("rust", src),
            vec![("let", ColorRole::Keyword), ("let", ColorRole::Keyword), ("2", ColorRole::Number)]
        );
    }

    #[test]
    fn a_triple_holds_what_a_single_cannot() {
        let src = "x = \"\"\"a \"quoted\" docstring\"\"\"";
        assert_eq!(marked("py", src), vec![("\"\"\"a \"quoted\" docstring\"\"\"", ColorRole::String)]);
    }

    #[test]
    fn a_preprocessors_line_is_the_whole_line() {
        assert_eq!(
            marked("c", "#include <stdio.h>\nint main(void) { return 0; }"),
            vec![
                ("#include <stdio.h>", ColorRole::Keyword),
                ("int", ColorRole::Type),
                ("void", ColorRole::Type),
                ("return", ColorRole::Keyword),
                ("0", ColorRole::Number),
            ],
            "the preprocessor's line is one instruction, and `main` is an author's name"
        );
        assert_eq!(marked("c", "    #define X 1"), vec![("#define X 1", ColorRole::Keyword)]);
        // The same line one line down, which is where a real header writes itself: the
        // test of `at the start of a line` is whether anything but space is behind it.
        assert_eq!(
            marked("c", "int x;\n#define Y 2\nint z;"),
            vec![
                ("int", ColorRole::Type),
                ("#define Y 2", ColorRole::Keyword),
                ("int", ColorRole::Type),
            ]
        );
    }

    #[test]
    fn a_directive_names_itself_with_the_sign() {
        assert_eq!(marked("css", "@media (max-width: 600px) { a { color: red } }}").first(), Some(&("@media", ColorRole::Keyword)));
        assert_eq!(marked("css", "@media"), vec![("@media", ColorRole::Keyword)]);
        assert_eq!(marked("java", "@Override"), vec![("@Override", ColorRole::Keyword)]);
    }

    #[test]
    fn a_shell_variable_is_not_the_name_of_a_thing() {
        assert_eq!(marked("sh", "echo $HOME"), vec![("echo", ColorRole::Keyword), ("$HOME", ColorRole::Type)]);
        assert_eq!(marked("sh", "echo $"), vec![("echo", ColorRole::Keyword)], "and one sign is not worth an ink");
    }

    #[test]
    fn a_json_key_is_not_a_json_value() {
        assert_eq!(
            marked("json", "{\"lang\": \"zh\", \"n\": 3}"),
            vec![
                ("\"lang\"", ColorRole::Type),
                ("\"zh\"", ColorRole::String),
                ("\"n\"", ColorRole::Type),
                ("3", ColorRole::Number),
            ],
            "the names are set apart from what they name"
        );
        assert_eq!(role_of("json", "\"a\": 1"), Some(ColorRole::Type));
        assert_eq!(role_of("json", "\"a\""), Some(ColorRole::String));
        assert_eq!(role_of("json", "true"), Some(ColorRole::Keyword));
    }

    #[test]
    fn a_tag_names_its_element_and_its_attributes() {
        assert_eq!(
            marked("html", "<a href=\"#x\" class='big'>&amp;nbsp</a>"),
            vec![
                ("<a", ColorRole::Type),
                ("href", ColorRole::Keyword),
                ("\"#x\"", ColorRole::String),
                ("class", ColorRole::Keyword),
                ("'big'", ColorRole::String),
                ("&amp;", ColorRole::Number),
                ("</a", ColorRole::Type),
            ]
        );
        // A `>` in the prose is not a tag's end, and a `<` that begins no name is not a
        // tag at all: `a < b` in text keeps its right-hand side.
        assert_eq!(marked("html", "5 < 6 and 7 > 8"), vec![]);
    }

    #[test]
    fn an_unknown_language_is_left_entirely_alone() {
        // The colouring is a claim about the text, and this build has no evidence.
        assert!(tokens("klingon", "let x = // not a comment\n").is_empty());
        assert!(tokens("", "x").is_empty());
        assert!(tokens("markdown", "# Heading").is_empty(), "and prose is not source");
        assert!(knows("rust"));
        assert!(knows("zsh"), "a family's aliases reach it too");
        assert!(!knows("klingon"));
    }

    #[test]
    fn every_range_lands_on_a_character_boundary() {
        // The one failure that would be a crash rather than a wrong colour, and the
        // reason the scanner walks characters rather than bytes.
        let src = "// 中文注释：一句话\nfn 变量() { let s = \"值\"; }\n";
        for (r, _) in tokens("rust", src) {
            assert!(src.is_char_boundary(r.start) && src.is_char_boundary(r.end), "{r:?}");
        }
        assert_eq!(marked("rust", "let 变量 = 1"), vec![("let", ColorRole::Keyword), ("1", ColorRole::Number)]);
    }

    #[test]
    fn runs_come_out_in_order_and_never_overlap() {
        let src = "fn main() { /* c */ let s = \"x\"; } // end\n";
        let mut last = 0usize;
        for (r, _) in tokens("rust", src) {
            assert!(r.start >= last, "{r:?} after {last}");
            assert!(r.end > r.start);
            last = r.end;
        }
        assert_eq!(
            kinds("rust", src),
            vec![
                ColorRole::Keyword,
                ColorRole::Comment,
                ColorRole::Keyword,
                ColorRole::String,
                ColorRole::Comment,
            ],
            "`fn` and `let`, the two remarks, and the one string"
        );
    }

    #[test]
    fn a_fence_reaches_its_family_by_any_of_its_names() {
        for name in ["rs", "rust"] {
            assert_eq!(role_of(name, "fn"), Some(ColorRole::Keyword), "{name}");
        }
        for name in ["ts", "tsx", "typescript", "js", "jsx"] {
            assert_eq!(role_of(name, "const"), Some(ColorRole::Keyword), "{name}");
        }
        for name in ["py", "python"] {
            assert_eq!(role_of(name, "def"), Some(ColorRole::Keyword), "{name}");
        }
        assert_eq!(role_of("cpp", "namespace"), Some(ColorRole::Keyword));
        assert_eq!(role_of("hpp", "class"), Some(ColorRole::Keyword));
        assert_eq!(role_of("htm", "<p>"), Some(ColorRole::Type));
    }
}
