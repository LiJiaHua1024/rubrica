//! Common mhchem notation lowered to the existing formula parser. This is a
//! bounded subset, not mhchem's complete language: coefficients, element counts,
//! explicit scripts, charges, states, bonds, hydrates and labelled reactions.

use crate::parse::{Node, Parser};

/// Ordinary Markdown chemistry often uses TeX without \ce. Require both a
/// reaction relation and valid element spellings, plus a compound or subscript,
/// before choosing roman lettering. Unknown identifiers and math commands keep
/// their normal mathematical reading.
pub(crate) fn is_reaction(source: &str) -> bool {
    if !(source.contains('=')
        || source.contains('→')
        || source.contains('⇌')
        || source.contains("arrow")
        || source.contains("harpoons"))
    {
        return false;
    }
    let mut at = 0usize;
    let mut atoms = 0usize;
    let mut distinctive = source.contains('_');
    while at < source.len() {
        let ch = source[at..].chars().next().unwrap();
        if ch == '\\' {
            at += 1;
            let start = at;
            while source
                .as_bytes()
                .get(at)
                .is_some_and(u8::is_ascii_alphabetic)
            {
                at += 1;
            }
            let name = &source[start..at];
            if name.starts_with('x') && (name.contains("arrow") || name.contains("harpoons")) {
                while source[at..].starts_with(char::is_whitespace) {
                    at += source[at..].chars().next().unwrap().len_utf8();
                }
                let _ = bracket(source, &mut at);
                while source[at..].starts_with(char::is_whitespace) {
                    at += source[at..].chars().next().unwrap().len_utf8();
                }
                if source[at..].starts_with('{') {
                    copy_group(source, &mut at, &mut String::new());
                }
            } else if matches!(name, "text" | "textrm" | "mbox") {
                while source[at..].starts_with(char::is_whitespace) {
                    at += source[at..].chars().next().unwrap().len_utf8();
                }
                if source[at..].starts_with('{') {
                    copy_group(source, &mut at, &mut String::new());
                }
            } else if !matches!(
                name,
                "" | "mathrm"
                    | "Delta"
                    | "delta"
                    | "cdot"
                    | "rightarrow"
                    | "leftarrow"
                    | "leftrightarrow"
                    | "to"
                    | "longrightarrow"
                    | "rightleftharpoons"
                    | "leftrightharpoons"
                    | "uparrow"
                    | "downarrow"
                    | "overset"
                    | "underset"
                    | "stackrel"
                    | "left"
                    | "right"
            ) {
                return false;
            }
        } else if ch.is_ascii_alphabetic() {
            let start = at;
            while source
                .as_bytes()
                .get(at)
                .is_some_and(u8::is_ascii_alphabetic)
            {
                at += 1;
            }
            let word = &source[start..at];
            if matches!(word, "s" | "l" | "g" | "aq") {
                continue;
            }
            let Some(count) = elements(word) else {
                return false;
            };
            atoms += count;
            distinctive |= word.len() > 1;
        } else {
            at += ch.len_utf8();
        }
    }
    distinctive && atoms >= 2
}

fn elements(word: &str) -> Option<usize> {
    const ELEMENTS: &str = " H He Li Be B C N O F Ne Na Mg Al Si P S Cl Ar K Ca Sc Ti V Cr Mn Fe Co Ni Cu Zn Ga Ge As Se Br Kr Rb Sr Y Zr Nb Mo Tc Ru Rh Pd Ag Cd In Sn Sb Te I Xe Cs Ba La Ce Pr Nd Pm Sm Eu Gd Tb Dy Ho Er Tm Yb Lu Hf Ta W Re Os Ir Pt Au Hg Tl Pb Bi Po At Rn Fr Ra Ac Th Pa U Np Pu Am Cm Bk Cf Es Fm Md No Lr Rf Db Sg Bh Hs Mt Ds Rg Cn Nh Fl Mc Lv Ts Og ";
    let mut at = 0usize;
    let mut count = 0usize;
    while at < word.len() {
        if !word.as_bytes()[at].is_ascii_uppercase() {
            return None;
        }
        let start = at;
        at += 1;
        if word.as_bytes().get(at).is_some_and(u8::is_ascii_lowercase) {
            at += 1;
        }
        if !ELEMENTS.contains(&format!(" {} ", &word[start..at])) {
            return None;
        }
        count += 1;
    }
    Some(count)
}

pub(crate) fn parse(source: &str, depth: usize) -> Node {
    if depth >= Parser::MAX_DEPTH {
        return Node::Atom(source.to_string());
    }
    let mut out = String::new();
    let mut at = 0usize;
    let mut in_species = false;
    while at < source.len() {
        let rest = &source[at..];
        if let Some((arrow, command)) = [
            ("<=>", "xrightleftharpoons"),
            ("<->", "xleftrightarrow"),
            ("->", "xrightarrow"),
            ("<-", "xleftarrow"),
            ("=>", "xRightarrow"),
            ("<=", "xLeftarrow"),
        ]
        .into_iter()
        .find(|(s, _)| rest.starts_with(s))
        {
            at += arrow.len();
            while source[at..].starts_with(char::is_whitespace) {
                at += source[at..].chars().next().unwrap().len_utf8();
            }
            let above = bracket(source, &mut at);
            let below = bracket(source, &mut at);
            out.push('\\');
            out.push_str(command);
            if let Some(label) = below {
                out.push('[');
                out.push_str(label);
                out.push(']');
            }
            out.push('{');
            if let Some(label) = above {
                out.push_str(label);
            }
            out.push('}');
            in_species = false;
            continue;
        }
        let ch = rest.chars().next().unwrap();
        at += ch.len_utf8();
        match ch {
            '\\' => {
                // Preserve an explicit TeX command and any groups it carries;
                // normalizing digits inside \text{...} would alter its words.
                out.push(ch);
                while source
                    .as_bytes()
                    .get(at)
                    .is_some_and(u8::is_ascii_alphabetic)
                {
                    out.push(source.as_bytes()[at] as char);
                    at += 1;
                }
                if out.ends_with('\\') && at < source.len() {
                    let c = source[at..].chars().next().unwrap();
                    out.push(c);
                    at += c.len_utf8();
                }
                while source[at..].starts_with('{') {
                    copy_group(source, &mut at, &mut out);
                }
                in_species = true;
            }
            '{' => {
                at -= 1;
                copy_group(source, &mut at, &mut out);
                in_species = true;
            }
            '^' if source[at..]
                .chars()
                .next()
                .is_none_or(|c| c.is_whitespace()) =>
            {
                out.push_str("\\uparrow ");
            }
            'v' if !in_species
                && source[at..]
                    .chars()
                    .next()
                    .is_none_or(|c| c.is_whitespace()) =>
            {
                out.push_str("\\downarrow ");
            }
            '^' | '_' => {
                out.push(ch);
                if source[at..].starts_with('{') {
                    copy_group(source, &mut at, &mut out);
                } else {
                    out.push('{');
                    while source
                        .as_bytes()
                        .get(at)
                        .is_some_and(|c| c.is_ascii_digit())
                    {
                        out.push(source.as_bytes()[at] as char);
                        at += 1;
                    }
                    if let Some(c @ ('+' | '-')) = source[at..].chars().next() {
                        out.push(c);
                        at += 1;
                    } else if out.ends_with('{') && at < source.len() {
                        let c = source[at..].chars().next().unwrap();
                        out.push(c);
                        at += c.len_utf8();
                    }
                    out.push('}');
                }
            }
            '0'..='9' => {
                let start = at - 1;
                while source.as_bytes().get(at).is_some_and(u8::is_ascii_digit) {
                    at += 1;
                }
                if in_species {
                    out.push_str("_{");
                }
                out.push_str(&source[start..at]);
                if in_species {
                    out.push('}');
                }
                // Leading digits are coefficients; the next letter starts the species.
            }
            '+' | '-'
                if in_species
                    && source[at..].chars().next().is_none_or(|c| {
                        c.is_whitespace() || matches!(c, '+' | ')' | ']' | '↑' | '↓')
                    }) =>
            {
                out.push_str("^{");
                out.push(ch);
                out.push('}');
            }
            '-' | '=' | '#'
                if in_species
                    && source[at..]
                        .chars()
                        .next()
                        .is_some_and(|c| c.is_alphabetic() || c == '(') =>
            {
                out.push_str("\\bond{");
                out.push(ch);
                out.push('}');
            }
            '+' | '=' => {
                out.push(ch);
                in_species = false;
            }
            '*' | '·' | '.' => {
                out.push_str("\\cdot ");
                in_species = false;
            }
            '↑' => out.push_str("\\uparrow "),
            '↓' => out.push_str("\\downarrow "),
            c if c.is_whitespace() => {
                out.push(' ');
                in_species = false;
            }
            _ => {
                out.push(ch);
                in_species = ch.is_alphabetic() || matches!(ch, ')' | ']');
            }
        }
    }
    Parser::chemical(&out, depth)
}

fn copy_group(source: &str, at: &mut usize, out: &mut String) {
    let start = *at;
    let mut depth = 0usize;
    while *at < source.len() {
        let c = source[*at..].chars().next().unwrap();
        *at += c.len_utf8();
        match c {
            '{' => depth += 1,
            '}' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    break;
                }
            }
            '\\' if *at < source.len() => {
                *at += source[*at..].chars().next().unwrap().len_utf8();
            }
            _ => {}
        }
    }
    out.push_str(&source[start..*at]);
}

fn bracket<'a>(source: &'a str, at: &mut usize) -> Option<&'a str> {
    if !source[*at..].starts_with('[') {
        return None;
    }
    *at += 1;
    let start = *at;
    let mut depth = 1usize;
    let mut braces = 0usize;
    while *at < source.len() {
        let c = source[*at..].chars().next().unwrap();
        match c {
            '{' => braces += 1,
            '}' => braces = braces.saturating_sub(1),
            '[' if braces == 0 => depth += 1,
            ']' if braces == 0 => {
                depth -= 1;
                if depth == 0 {
                    let end = *at;
                    *at += 1;
                    return Some(&source[start..end]);
                }
            }
            '\\' => {
                *at += 1;
                if *at < source.len() {
                    *at += source[*at..].chars().next().unwrap().len_utf8();
                }
                continue;
            }
            _ => {}
        }
        *at += c.len_utf8();
    }
    Some(&source[start..])
}
