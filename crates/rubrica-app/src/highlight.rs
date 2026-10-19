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
        // The `#` half belongs to the families that spell directives with one --
        // `#region`, `#if` -- and not to CSS, where a `#` opens a hex colour or
        // an id selector and painted `#fff` in the directive's ink beside `#333`
        // in the ordinary one.
        if s.at_rule
            && (ch == b'@' || (ch == b'#' && s.hash_directive))
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
            // A line-continuation backslash is not an escape inside a fence: stepping
            // over the newline after it carried the string into the next line and
            // swallowed the code there whole.
            if p + 1 >= b.len() || b[p + 1] == b'\n' {
                return None;
            }
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
            // A lead byte starts a word for `is_word_start`, but a non-letter
            // (a fullwidth mark, an arrow) leaves `word_end` where it began:
            // step past the character whatever the word said, or this loop
            // never leaves the byte.
            i = end.max(i + text[i..].chars().next().map_or(1, |c| c.len_utf8()));
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
    /// A `#` names a directive too, as the C family's `#region` and `#if` do. Not
    /// CSS's: there a `#` opens a hex colour or an id selector, and reading it as
    /// a directive gave one stylesheet two inks for one kind of value.
    hash_directive: bool,
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
    hash_directive: false,
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
    // The names that arrive from a tool rather than from a person: the long form of a
    // two-letter alias, which Pandoc and an editor's modeline write out, and the
    // scripting languages a reader on this platform pastes -- batch and PowerShell,
    // whose fences used to be set plain because only `sh` and `bash` were known.
    ("bat", &SHELL),
    ("cc", &CPP),
    ("cmd", &SHELL),
    ("cxx", &CPP),
    ("docker", &SHELL),
    ("golang", &GO),
    ("javascript", &JS),
    ("powershell", &SHELL),
    ("ps1", &SHELL),
    ("pwsh", &SHELL),
    ("ruby", &RUBY),
    // G9: the names a fence is likely to carry that this build had not met. Most of them
    // are spellings of a family already here -- a dialect shares its parent's words, and
    // `mariadb` is set by the SQL scanner rather than by a table of its own -- so what
    // grew here is the list of names, and only the families above are new scanners.
    ("aarch64", &ASM),
    ("ada", &ADA),
    ("adb", &ADA),
    ("ads", &ADA),
    ("ansible", &YAML),
    ("applescript", &APPLESCRIPT),
    ("arm", &ASM),
    ("ash", &SHELL),
    ("asm", &ASM),
    ("astro", &HTML),
    ("atom", &HTML),
    ("awk", &AWK),
    ("bazel", &STARLARK),
    ("bigquery", &SQL),
    ("bzl", &STARLARK),
    ("c11", &C),
    ("c17", &C),
    ("c99", &C),
    ("cake", &CS),
    ("cassandra", &SQL),
    ("cjs", &JS),
    ("cl", &LISP),
    ("clickhouse", &SQL),
    ("clj", &LISP),
    ("cljc", &LISP),
    ("cljs", &LISP),
    ("clojure", &LISP),
    ("compose", &YAML),
    ("conf", &INI),
    ("containerfile", &SHELL),
    ("cql", &SQL),
    ("cr", &CRYSTAL),
    ("crystal", &CRYSTAL),
    ("csharp", &CS),
    ("csx", &CS),
    ("cts", &JS),
    ("cu", &C),
    ("cuh", &C),
    ("cython", &PY),
    ("d", &D),
    ("dash", &SHELL),
    ("desktop", &INI),
    ("di", &D),
    ("dlang", &D),
    ("docker-compose", &YAML),
    ("dosbatch", &SHELL),
    ("dotenv", &INI),
    ("duckdb", &SQL),
    ("edn", &LISP),
    ("editorconfig", &INI),
    ("el", &LISP),
    ("elisp", &LISP),
    ("elixir", &ELIXIR),
    ("elm", &HASKELL),
    ("elvish", &SHELL),
    ("env", &INI),
    ("erl", &ERLANG),
    ("erlang", &ERLANG),
    ("ex", &ELIXIR),
    ("exs", &ELIXIR),
    ("f03", &FORTRAN),
    ("f08", &FORTRAN),
    ("f90", &FORTRAN),
    ("f95", &FORTRAN),
    ("fish", &SHELL),
    ("for", &FORTRAN),
    ("fortran", &FORTRAN),
    ("fs", &FSHARP),
    ("fsi", &FSHARP),
    ("fsharp", &FSHARP),
    ("fsx", &FSHARP),
    ("gawk", &AWK),
    ("geojson", &JSON),
    ("gitconfig", &INI),
    ("gleam", &GLEAM),
    ("gql", &GRAPHQL),
    ("graphql", &GRAPHQL),
    ("gradle", &GROOVY),
    ("groovy", &GROOVY),
    ("gvy", &GROOVY),
    ("haskell", &HASKELL),
    ("hcl", &HCL),
    ("hh", &CPP),
    ("hjson", &JSONC),
    ("hql", &SQL),
    ("hrl", &ERLANG),
    ("hs", &HASKELL),
    ("hxx", &CPP),
    ("inf", &INI),
    ("ipp", &CPP),
    ("ipython", &PY),
    ("jenkinsfile", &GROOVY),
    ("jl", &JULIA),
    ("julia", &JULIA),
    ("ksh", &SHELL),
    ("ksh93", &SHELL),
    ("kt", &JAVA),
    ("kts", &JAVA),
    ("latex", &LATEX),
    ("lhs", &HASKELL),
    ("lisp", &LISP),
    ("lsp", &LISP),
    ("ltx", &LATEX),
    ("luau", &LUA),
    ("make", &SHELL),
    ("mariadb", &SQL),
    ("matlab", &MATLAB),
    ("mjs", &JS),
    ("mk", &SHELL),
    ("mksh", &SHELL),
    ("ml", &OCAML),
    ("mli", &OCAML),
    ("mm", &CPP),
    ("mts", &JS),
    ("mysql", &SQL),
    ("nasm", &ASM),
    ("ndjson", &JSONC),
    ("nim", &NIM),
    ("nimble", &NIM),
    ("nims", &NIM),
    ("nix", &NIX),
    ("npmrc", &INI),
    ("nse", &LUA),
    ("nu", &SHELL),
    ("nushell", &SHELL),
    ("ocaml", &OCAML),
    ("octave", &MATLAB),
    ("odin", &ODIN),
    ("osascript", &APPLESCRIPT),
    ("pas", &PASCAL),
    ("pascal", &PASCAL),
    ("pcss", &CSS),
    ("perl", &PERL),
    ("pgsql", &SQL),
    ("php3", &PHP),
    ("php4", &PHP),
    ("php5", &PHP),
    ("php7", &PHP),
    ("phtml", &PHP),
    ("pl", &PERL),
    ("playbook", &YAML),
    ("plist", &HTML),
    ("plpgsql", &SQL),
    ("plsql", &SQL),
    ("pm", &PERL),
    ("podspec", &RUBY),
    ("postcss", &CSS),
    ("postgres", &SQL),
    ("postgresql", &SQL),
    ("pp", &PASCAL),
    ("proto", &PROTO),
    ("protobuf", &PROTO),
    ("psd1", &SHELL),
    ("psm1", &SHELL),
    ("puppet", &PUPPET),
    ("py3", &PY),
    ("pyi", &PY),
    ("python3", &PY),
    ("pyx", &PY),
    ("r", &R),
    ("racket", &LISP),
    ("rake", &RUBY),
    ("redshift", &SQL),
    ("reg", &INI),
    ("riscv", &ASM),
    ("rkt", &LISP),
    ("rmd", &R),
    ("rockspec", &LUA),
    ("rss", &HTML),
    ("ru", &RUBY),
    ("sage", &PY),
    ("scheme", &LISP),
    ("scm", &LISP),
    ("service", &INI),
    ("sh-session", &SHELL),
    ("shell-session", &SHELL),
    ("shtml", &HTML),
    ("snowflake", &SQL),
    ("sol", &SOLIDITY),
    ("solidity", &SOLIDITY),
    ("sqlite", &SQL),
    ("sqlite3", &SQL),
    ("starlark", &STARLARK),
    ("sty", &LATEX),
    ("sv", &VERILOG),
    ("svelte", &HTML),
    ("systemd", &INI),
    ("systemverilog", &VERILOG),
    ("tcl", &TCL),
    ("terraform", &HCL),
    ("tex", &LATEX),
    ("tf", &HCL),
    ("tfvars", &HCL),
    ("tk", &TCL),
    ("transactsql", &SQL),
    ("tsql", &SQL),
    ("unit", &INI),
    ("v", &V),
    ("verilog", &VERILOG),
    ("vh", &VERILOG),
    ("vim", &VIM),
    ("viml", &VIM),
    ("vlang", &V),
    ("vue", &HTML),
    ("wasm", &WAT),
    ("wat", &WAT),
    ("wsdl", &HTML),
    ("xaml", &HTML),
    ("xhtml", &HTML),
    ("xonsh", &SHELL),
    ("xsd", &HTML),
    ("xsl", &HTML),
    ("xslt", &HTML),
    ("zig", &ZIG),
    ("zon", &ZIG),
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
    hash_directive: true,
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

// The rest of the families, and the reason each has a scanner of its own: a language
// whose comments and strings are not another's cannot be read by another's table. Each
// keyword list holds the words a reader needs told apart rather than a grammar's whole
// reserved set -- a short honest list is worth more than a long invented one -- and a
// language whose case carries no meaning says so, because otherwise `IF` would be
// painted as a class the author invented.
const R: Spec = Spec {
    line: &["#"],
    quotes: "\"'",
    capitals: false,
    keywords: &[
        "break", "else", "FALSE", "for", "function", "if", "in", "Inf", "library", "NA", "NaN",
        "next", "NULL", "repeat", "require", "return", "TRUE", "while",
    ],
    ..BASE
};

const JULIA: Spec = Spec {
    line: &["#"],
    block: &[("#=", "=#")],
    quotes: "\"",
    keywords: &[
        "abstract", "baremodule", "begin", "break", "catch", "const", "continue", "do", "else",
        "elseif", "end", "export", "false", "finally", "for", "function", "global", "if", "import",
        "in", "isa", "let", "local", "macro", "module", "mutable", "nothing", "primitive", "quote",
        "return", "struct", "true", "try", "using", "where", "while",
    ],
    ..BASE
};

const PERL: Spec = Spec {
    line: &["#"],
    quotes: "\"'",
    capitals: false,
    dollar: true,
    keywords: &[
        "and", "cmp", "continue", "die", "do", "else", "elsif", "eq", "for", "foreach", "ge", "gt",
        "if", "last", "le", "local", "lt", "my", "ne", "next", "not", "or", "our", "package", "print",
        "redo", "require", "return", "say", "state", "sub", "unless", "until", "use", "warn", "while",
    ],
    ..BASE
};

const HASKELL: Spec = Spec {
    line: &["--"],
    block: &[("{-", "-}")],
    quotes: "\"",
    char_lit: true,
    keywords: &[
        "as", "case", "class", "data", "default", "deriving", "do", "else", "foreign", "hiding", "if",
        "import", "in", "infix", "infixl", "infixr", "instance", "let", "module", "newtype", "of",
        "qualified", "then", "type", "where",
    ],
    ..BASE
};

const OCAML: Spec = Spec {
    block: &[("(*", "*)")],
    quotes: "\"",
    char_lit: true,
    keywords: &[
        "and", "as", "assert", "begin", "class", "constraint", "do", "done", "downto", "else", "end",
        "exception", "external", "false", "for", "fun", "function", "functor", "if", "in", "include",
        "inherit", "initializer", "lazy", "let", "match", "method", "module", "mutable", "new",
        "object", "of", "open", "or", "private", "rec", "sig", "struct", "then", "to", "true", "try",
        "type", "val", "virtual", "when", "while", "with",
    ],
    ..BASE
};

const FSHARP: Spec = Spec {
    line: &["//"],
    block: &[("(*", "*)")],
    quotes: "\"",
    keywords: &[
        "abstract", "and", "as", "assert", "base", "begin", "class", "default", "delegate", "do",
        "done", "downto", "elif", "else", "end", "exception", "extern", "false", "finally", "for",
        "fun", "function", "if", "in", "inherit", "inline", "interface", "internal", "lazy", "let",
        "match", "member", "module", "mutable", "namespace", "new", "null", "of", "open", "or",
        "override", "private", "public", "rec", "return", "static", "struct", "then", "to", "true",
        "try", "type", "use", "val", "void", "when", "while", "with", "yield",
    ],
    ..BASE
};

const ERLANG: Spec = Spec {
    line: &["%"],
    quotes: "\"",
    char_lit: true,
    capitals: false,
    keywords: &[
        "after", "andalso", "band", "begin", "bnot", "bor", "bsl", "bsr", "bxor", "case", "catch",
        "cond", "div", "end", "fun", "if", "let", "not", "of", "orelse", "receive", "rem", "throw",
        "try", "when", "xor",
    ],
    ..BASE
};

const ELIXIR: Spec = Spec {
    line: &["#"],
    quotes: "\"",
    keywords: &[
        "after", "alias", "and", "case", "catch", "cond", "def", "defdelegate", "defexception",
        "defimpl", "defmacro", "defmodule", "defp", "defprotocol", "defstruct", "do", "else", "end",
        "fn", "for", "if", "import", "in", "not", "or", "quote", "raise", "receive", "require",
        "rescue", "super", "throw", "try", "unless", "unquote", "use", "when", "with",
    ],
    ..BASE
};

const NIM: Spec = Spec {
    line: &["#"],
    block: &[("#[", "]#")],
    quotes: "\"",
    keywords: &[
        "and", "as", "block", "break", "case", "const", "continue", "converter", "defer", "discard",
        "distinct", "div", "do", "elif", "else", "enum", "except", "export", "finally", "for", "from",
        "func", "if", "import", "in", "include", "interface", "is", "isnot", "iterator", "let",
        "macro", "method", "mixin", "mod", "nil", "not", "notin", "object", "of", "or", "proc", "ptr",
        "raise", "ref", "return", "shl", "shr", "static", "template", "try", "tuple", "type", "using",
        "var", "when", "while", "xor", "yield",
    ],
    ..BASE
};

const ZIG: Spec = Spec {
    line: &["//"],
    quotes: "\"",
    char_lit: true,
    keywords: &[
        "align", "allowzero", "and", "anyframe", "anytype", "asm", "async", "await", "break",
        "callconv", "catch", "comptime", "const", "continue", "defer", "else", "enum", "errdefer",
        "error", "export", "extern", "fn", "for", "if", "inline", "linksection", "noalias",
        "nosuspend", "opaque", "or", "orelse", "packed", "pub", "resume", "return", "struct",
        "suspend", "switch", "test", "threadlocal", "try", "union", "unreachable", "usingnamespace",
        "var", "volatile", "while",
    ],
    ..BASE
};

const V: Spec = Spec {
    line: &["//"],
    block: &[("/*", "*/")],
    quotes: "\"'",
    keywords: &[
        "as", "asm", "assert", "atomic", "break", "const", "continue", "defer", "else", "enum",
        "false", "fn", "for", "go", "goto", "if", "import", "in", "interface", "is", "isreftype",
        "lock", "match", "module", "mut", "nil", "none", "or", "pub", "return", "rlock", "select",
        "shared", "sizeof", "spawn", "static", "struct", "true", "type", "typeof", "union", "unsafe",
        "volatile", "while",
    ],
    ..BASE
};

const FORTRAN: Spec = Spec {
    line: &["!"],
    quotes: "\"'",
    insensitive: true,
    capitals: false,
    keywords: &[
        "allocate", "allocatable", "call", "case", "character", "close", "common", "complex",
        "contains", "continue", "cycle", "data", "deallocate", "dimension", "do", "double", "else",
        "elseif", "end", "enddo", "endif", "entry", "equivalence", "exit", "external", "format",
        "function", "goto", "if", "implicit", "in", "include", "inout", "integer", "intent",
        "interface", "intrinsic", "logical", "module", "none", "open", "optional", "parameter",
        "pointer", "precision", "print", "private", "program", "public", "read", "real", "recursive",
        "return", "save", "select", "stop", "subroutine", "then", "type", "use", "where", "while",
        "write",
    ],
    ..BASE
};

const PASCAL: Spec = Spec {
    line: &["//"],
    block: &[("{", "}"), ("(*", "*)")],
    quotes: "'",
    insensitive: true,
    capitals: false,
    keywords: &[
        "and", "array", "begin", "case", "class", "const", "constructor", "destructor", "div", "do",
        "downto", "else", "end", "except", "file", "finally", "for", "function", "goto", "if",
        "implementation", "in", "inherited", "inline", "interface", "label", "mod", "nil", "not",
        "object", "of", "operator", "or", "packed", "procedure", "program", "property", "raise",
        "record", "repeat", "set", "shl", "shr", "then", "to", "try", "type", "unit", "until", "uses",
        "var", "while", "with", "xor",
    ],
    ..BASE
};

const ADA: Spec = Spec {
    line: &["--"],
    quotes: "\"",
    char_lit: true,
    insensitive: true,
    capitals: false,
    keywords: &[
        "abort", "abs", "abstract", "accept", "access", "aliased", "all", "and", "array", "at",
        "begin", "body", "case", "constant", "declare", "delay", "delta", "digits", "do", "else",
        "elsif", "end", "entry", "exception", "exit", "for", "function", "generic", "goto", "if",
        "in", "interface", "is", "limited", "loop", "mod", "new", "not", "null", "of", "or", "others",
        "out", "overriding", "package", "pragma", "private", "procedure", "protected", "raise",
        "range", "record", "rem", "renames", "requeue", "return", "reverse", "select", "separate",
        "subtype", "tagged", "task", "terminate", "then", "type", "until", "use", "when", "while",
        "with", "xor",
    ],
    ..BASE
};

const D: Spec = Spec {
    line: &["//"],
    block: &[("/*", "*/"), ("/+", "+/")],
    quotes: "\"`",
    keywords: &[
        "abstract", "alias", "align", "asm", "assert", "auto", "body", "break", "case", "cast",
        "catch", "class", "const", "continue", "debug", "default", "delegate", "delete",
        "deprecated", "do", "else", "enum", "export", "extern", "false", "final", "finally", "for",
        "foreach", "foreach_reverse", "function", "goto", "if", "immutable", "import", "in", "inout",
        "interface", "invariant", "is", "lazy", "mixin", "module", "new", "nothrow", "null", "out",
        "override", "package", "pragma", "private", "protected", "public", "pure", "ref", "return",
        "scope", "shared", "static", "struct", "super", "switch", "synchronized", "template", "this",
        "throw", "true", "try", "typeid", "typeof", "union", "unittest", "version", "while", "with",
    ],
    ..BASE
};

const MATLAB: Spec = Spec {
    line: &["%"],
    block: &[("%{", "%}")],
    quotes: "\"'",
    capitals: false,
    keywords: &[
        "break", "case", "catch", "classdef", "continue", "else", "elseif", "end", "enumeration",
        "events", "for", "function", "global", "if", "methods", "otherwise", "parfor", "persistent",
        "properties", "return", "spmd", "switch", "try", "while",
    ],
    ..BASE
};

const TCL: Spec = Spec {
    line: &["#"],
    quotes: "\"",
    capitals: false,
    dollar: true,
    keywords: &[
        "after", "append", "array", "break", "catch", "cd", "close", "concat", "continue", "dict",
        "else", "elseif", "error", "eval", "exec", "exit", "expr", "file", "for", "foreach", "format",
        "gets", "global", "if", "incr", "info", "join", "lappend", "lindex", "list", "llength",
        "lrange", "lsearch", "lsort", "namespace", "open", "package", "proc", "puts", "read",
        "regexp", "regsub", "rename", "return", "scan", "set", "source", "split", "string", "subst",
        "switch", "tell", "then", "unset", "uplevel", "upvar", "variable", "vwait", "while",
    ],
    ..BASE
};

const AWK: Spec = Spec {
    line: &["#"],
    quotes: "\"",
    capitals: false,
    keywords: &[
        "BEGIN", "END", "break", "close", "continue", "delete", "do", "else", "exit", "fflush",
        "for", "function", "getline", "gsub", "if", "in", "index", "int", "length", "match", "next",
        "nextfile", "print", "printf", "return", "split", "sprintf", "sub", "substr", "system",
        "tolower", "toupper", "while",
    ],
    ..BASE
};

const LISP: Spec = Spec {
    line: &[";"],
    block: &[("#|", "|#")],
    quotes: "\"",
    capitals: false,
    keywords: &[
        "and", "apply", "car", "case", "cdr", "cond", "cons", "defconstant", "defmacro",
        "defparameter", "defun", "defvar", "do", "dolist", "dotimes", "else", "eval", "funcall",
        "if", "lambda", "let", "list", "loop", "mapcar", "nil", "not", "or", "progn", "quote",
        "reduce", "require", "setf", "setq", "t", "unless", "when",
    ],
    ..BASE
};

const HCL: Spec = Spec {
    line: &["#", "//"],
    block: &[("/*", "*/")],
    quotes: "\"",
    keywords: &[
        "backend", "check", "data", "dynamic", "false", "for", "for_each", "if", "in", "locals",
        "module", "null", "output", "provider", "provisioner", "resource", "terraform", "true",
        "variable",
    ],
    ..BASE
};

const NIX: Spec = Spec {
    line: &["#"],
    block: &[("/*", "*/")],
    quotes: "\"",
    dollar: true,
    keywords: &[
        "assert", "builtins", "derivation", "else", "false", "if", "import", "in", "inherit", "let",
        "null", "or", "rec", "then", "throw", "true", "with",
    ],
    ..BASE
};

const PROTO: Spec = Spec {
    line: &["//"],
    block: &[("/*", "*/")],
    quotes: "\"",
    keywords: &[
        "default", "enum", "extend", "extensions", "false", "import", "map", "message", "oneof",
        "option", "optional", "package", "public", "repeated", "required", "reserved", "returns",
        "rpc", "service", "stream", "syntax", "to", "true", "weak",
    ],
    ..BASE
};

const GRAPHQL: Spec = Spec {
    line: &["#"],
    quotes: "\"",
    triples: &["\"\"\""],
    keywords: &[
        "directive", "enum", "extend", "false", "fragment", "implements", "input", "interface",
        "mutation", "null", "on", "query", "repeatable", "scalar", "schema", "subscription", "true",
        "type", "union",
    ],
    ..BASE
};

const SOLIDITY: Spec = Spec {
    line: &["//"],
    block: &[("/*", "*/")],
    quotes: "\"'",
    keywords: &[
        "abstract", "address", "anonymous", "as", "assembly", "break", "calldata", "case", "catch",
        "constant", "constructor", "continue", "contract", "default", "delete", "do", "else", "emit",
        "enum", "error", "event", "external", "fallback", "false", "for", "function", "if",
        "immutable", "import", "indexed", "interface", "internal", "is", "library", "mapping",
        "memory", "modifier", "new", "override", "payable", "pragma", "private", "public", "pure",
        "receive", "return", "returns", "revert", "storage", "struct", "true", "try", "type",
        "unchecked", "using", "view", "virtual", "while",
    ],
    ..BASE
};

const LATEX: Spec = Spec {
    line: &["%"],
    capitals: false,
    keywords: &[
        "author", "begin", "bibliography", "caption", "cdot", "centering", "chapter", "cite",
        "documentclass", "emph", "end", "figure", "frac", "hline", "hspace", "include",
        "includegraphics", "infty", "int", "item", "label", "left", "maketitle", "mathrm",
        "paragraph", "part", "ref", "renewcommand", "right", "section", "sqrt", "subsection",
        "subsubsection", "sum", "table", "textbf", "textit", "texttt", "times", "title", "today",
        "usepackage", "vspace",
    ],
    ..BASE
};

const VIM: Spec = Spec {
    line: &["\""],
    quotes: "'",
    capitals: false,
    keywords: &[
        "abort", "augroup", "autocmd", "break", "call", "catch", "command", "continue", "echo",
        "echom", "else", "elseif", "endfor", "endfunction", "endif", "endtry", "endwhile", "execute",
        "finally", "finish", "for", "function", "highlight", "if", "in", "let", "map", "nmap",
        "nnoremap", "normal", "return", "set", "silent", "syntax", "throw", "try", "unlet", "vmap",
        "while",
    ],
    ..BASE
};

const ASM: Spec = Spec {
    line: &[";"],
    quotes: "\"",
    char_lit: true,
    insensitive: true,
    capitals: false,
    keywords: &[
        "add", "and", "call", "cmp", "db", "dd", "dec", "div", "dq", "dw", "equ", "global", "imul",
        "inc", "int", "je", "jg", "jl", "jmp", "jne", "jz", "lea", "mov", "mul", "nop", "not", "or",
        "pop", "push", "resb", "resd", "resw", "ret", "rol", "ror", "sar", "sbb", "section", "shl",
        "shr", "sub", "syscall", "test", "xor",
    ],
    ..BASE
};

const VERILOG: Spec = Spec {
    line: &["//"],
    block: &[("/*", "*/")],
    quotes: "\"",
    keywords: &[
        "always", "assign", "begin", "bit", "case", "casex", "casez", "default", "else", "end",
        "endcase", "endfunction", "endgenerate", "endmodule", "endtask", "for", "forever", "function",
        "generate", "if", "initial", "inout", "input", "integer", "localparam", "logic", "module",
        "negedge", "output", "parameter", "posedge", "real", "reg", "repeat", "signed", "task",
        "timescale", "unsigned", "while", "wire",
    ],
    ..BASE
};

const WAT: Spec = Spec {
    line: &[";;"],
    block: &[("(;", ";)")],
    quotes: "\"",
    capitals: false,
    keywords: &[
        "block", "br", "br_if", "br_table", "call", "call_indirect", "data", "drop", "elem", "else",
        "end", "export", "f32", "f64", "func", "global", "i32", "i64", "if", "import", "local",
        "loop", "memory", "module", "mut", "param", "result", "return", "select", "start", "table",
        "type", "v128",
    ],
    ..BASE
};

const STARLARK: Spec = Spec {
    line: &["#"],
    quotes: "\"'",
    triples: &["\"\"\""],
    keywords: &[
        "and", "break", "continue", "def", "elif", "else", "for", "if", "in", "lambda", "load",
        "not", "or", "pass", "return", "while",
    ],
    ..BASE
};

const APPLESCRIPT: Spec = Spec {
    line: &["--"],
    block: &[("(*", "*)")],
    quotes: "\"",
    insensitive: true,
    capitals: false,
    keywords: &[
        "and", "as", "at", "begin", "by", "considering", "contains", "continue", "copy", "div", "does",
        "else", "end", "error", "every", "exit", "false", "first", "for", "from", "get", "given",
        "global", "if", "ignoring", "in", "into", "is", "it", "its", "last", "local", "me", "mod",
        "my", "not", "of", "on", "onto", "or", "over", "prop", "property", "put", "ref", "repeat",
        "return", "script", "set", "some", "tell", "then", "to", "transaction", "true", "try",
        "until", "where", "while", "whose", "with", "without",
    ],
    ..BASE
};

const CRYSTAL: Spec = Spec {
    line: &["#"],
    quotes: "\"",
    keywords: &[
        "abstract", "alias", "annotation", "as", "asm", "begin", "break", "case", "class", "def",
        "do", "else", "elsif", "end", "ensure", "enum", "extend", "false", "for", "fun", "if", "in",
        "include", "is_a", "lib", "macro", "module", "next", "nil", "of", "out", "private",
        "protected", "require", "rescue", "return", "select", "self", "sizeof", "struct", "super",
        "then", "true", "type", "typeof", "union", "unless", "until", "verbatim", "when", "while",
        "with", "yield",
    ],
    ..BASE
};

const GLEAM: Spec = Spec {
    line: &["//"],
    quotes: "\"",
    keywords: &[
        "as", "assert", "case", "const", "echo", "else", "fn", "if", "import", "let", "opaque",
        "panic", "pub", "todo", "type", "use",
    ],
    ..BASE
};

const ODIN: Spec = Spec {
    line: &["//"],
    block: &[("/*", "*/")],
    quotes: "\"`",
    keywords: &[
        "asm", "auto_cast", "bit_set", "break", "case", "cast", "context", "continue", "defer",
        "distinct", "do", "dynamic", "else", "enum", "fallthrough", "for", "foreign", "if", "import",
        "in", "map", "matrix", "not_in", "or_else", "or_return", "package", "proc", "return",
        "runtime", "soa", "struct", "switch", "transmute", "typeid", "union", "using", "when",
        "where", "while",
    ],
    ..BASE
};

const PUPPET: Spec = Spec {
    line: &["#"],
    quotes: "\"'",
    dollar: true,
    keywords: &[
        "and", "case", "class", "contain", "default", "define", "else", "elsif", "false", "function",
        "if", "in", "inherits", "include", "node", "not", "or", "realize", "require", "return",
        "tag", "true", "undef", "unless",
    ],
    ..BASE
};

const GROOVY: Spec = Spec {
    line: &["//"],
    block: &[("/*", "*/")],
    quotes: "\"'",
    triples: &["\"\"\""],
    keywords: &[
        "abstract", "as", "assert", "break", "case", "catch", "class", "const", "continue", "def",
        "default", "do", "else", "enum", "extends", "false", "final", "finally", "for", "goto", "if",
        "implements", "import", "in", "instanceof", "interface", "new", "null", "package", "private",
        "protected", "public", "return", "static", "super", "switch", "synchronized", "this",
        "throw", "throws", "trait", "true", "try", "var", "while",
    ],
    ..BASE
};

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
        // The C family spells its directives with a hash as well, and there the
        // sign is what names one.
        assert_eq!(marked("cs", "#region Docs").first(), Some(&("#region", ColorRole::Keyword)));
    }

    /// CSS's hash is not a directive's sign: it opens a hex colour or an id
    /// selector. Read as a directive, `#fff` came out in the directive's ink
    /// beside `#333` in the ordinary one -- two colours for one kind of value in
    /// one stylesheet.
    #[test]
    fn a_css_hash_is_a_colour_rather_than_a_directive() {
        for src in ["a { color: #fff }", "a { color: #abc }", "#head { color: red }"] {
            let marked = marked("css", src);
            assert!(
                marked.iter().all(|(t, c)| !t.starts_with('#') || *c != ColorRole::Keyword),
                "{src:?} painted a hash as a directive: {marked:?}"
            );
        }
        // The colour keeps its digits either way -- the sign is simply not the
        // start of a keyword any more.
        assert_eq!(marked("css", "color: #333").last(), Some(&("333", ColorRole::Number)));
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
        // The long form of an alias is the name a tool writes, so it has to reach the
        // same family as the short one a person writes.
        for name in ["js", "javascript"] {
            assert_eq!(role_of(name, "const"), Some(ColorRole::Keyword), "{name}");
        }
        for name in ["go", "golang"] {
            assert_eq!(role_of(name, "func"), Some(ColorRole::Keyword), "{name}");
        }
        for name in ["rb", "ruby"] {
            assert_eq!(role_of(name, "def"), Some(ColorRole::Keyword), "{name}");
        }
        for name in ["c", "cc", "cpp", "cxx"] {
            assert_eq!(role_of(name, "int"), Some(ColorRole::Type), "{name}");
        }
        // Batch and PowerShell are shells on this platform, and a fence naming them was
        // set in the plain face. What they are read as is checked here rather than what
        // each token means: they share the shell's rules, and `rem` is not a comment in
        // a `#`-commenting family.
        for name in ["bat", "cmd", "ps1", "pwsh", "powershell", "docker"] {
            assert!(super::knows(name), "{name} is not a language this build reads");
        }
        assert_eq!(role_of("pwsh", "'a string'"), Some(ColorRole::String));
    }

    #[test]
    fn a_tag_with_a_multibyte_mark_terminates() {
        // `is_word_start` takes any lead byte, but a fullwidth mark is not a letter,
        // so `word_end` stood still and the attribute scan spun on the one byte
        // forever. The scanner now steps past the character whatever the word said;
        // this call simply returning is the regression it guards.
        let out = tokens("html", "<a （>中文</a>");
        let text: String = out.iter().map(|(r, _)| &"<a （>中文</a>"[r.start..r.end]).collect();
        assert!(text.contains("<a"), "the tag was still read: {text}");
    }

    /// A backslash at the end of a line is a continuation in some languages and not in
    /// a fence: stepping over the newline after it carried an unclosed string into the
    /// next line and swallowed the code there whole.
    #[test]
    fn an_escape_does_not_step_over_a_newline() {
        let src = "let a = \"oops\\\nlet b = 2;";
        let out = tokens("rust", src);
        let text: String = out.iter().map(|(r, _)| &src[r.start..r.end]).collect();
        assert!(!text.contains("oops\\\nlet b"), "the string crossed the line: {text}");
        let has_string = out.iter().any(|(r, role)| {
            matches!(role, ColorRole::String) && src[r.start..r.end].contains("oops")
        });
        assert!(!has_string, "an unclosed string is not a string: {out:?}");
    }

    /// A name listed twice is a name whose second entry can never be reached, and the
    /// count of languages this build reads is written down nowhere else. The names a
    /// fence may carry are what grew here, so the test also holds the ones a reader of
    /// this file would expect to find, and the two that are still not read.
    #[test]
    fn every_language_is_named_once_and_the_new_names_are_read() {
        let mut names: Vec<&str> = SPECS.iter().map(|(n, _)| *n).collect();
        let count = names.len();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), count, "a name is listed twice");
        assert!(count >= 273, "the table lost names: {count}");
        for lang in [
            "julia", "haskell", "zig", "solidity", "r", "hcl", "latex", "wat", "tcl", "asm",
            "fortran", "ada", "applescript", "crystal", "odin", "gleam", "puppet", "groovy",
            "verilog", "lisp", "matlab", "nim", "v", "d", "erlang", "elixir", "fsharp", "ocaml",
            "perl", "awk", "nix", "proto", "graphql", "starlark", "vim", "kotlin", "mariadb", "vue",
        ] {
            assert!(knows(lang), "`{lang}` is not read");
        }
        // A language with no table here is set plain, which is the honest page rather
        // than a guess at one, and `text` is a fence that says so on purpose.
        assert!(!knows("cobol"));
        assert!(!knows("brainfuck"));
        assert!(!knows("text"));
        // The two names a fixture already carried keep the scanners they had: `html` is
        // the mark-up scan and `powershell` the shell's, and neither may drift.
        assert_eq!(spec("html").map(|s| s.markup), Some(true));
        assert_eq!(spec("powershell").map(|s| s.dollar), Some(true));
    }
}
