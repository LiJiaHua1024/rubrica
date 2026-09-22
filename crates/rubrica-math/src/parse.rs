//! A LaTeX subset for formulas, parsed into the tree the layout engine consumes.
//!
//! Deliberately a subset, not a TeX implementation: the goal is the math people
//! actually write in Markdown, so fractions, scripts, radicals, stretchy delimiters,
//! big operators with limits, accents and the common symbol names are handled, and
//! anything else degrades to its literal characters rather than disappearing.
//!
//! Unbalanced braces or an unknown command never fail the parse. A reader that
//! refuses to show a document because one formula is malformed is worse than one
//! that shows the source it could not interpret.

/// A parsed formula node.
#[derive(Clone, Debug, PartialEq)]
pub enum Node {
    /// A run of characters set at the current style: identifiers, digits, operators.
    Atom(String),
    Row(Vec<Node>),
    Frac {
        num: Box<Node>,
        den: Box<Node>,
        /// `\binom` renders the same stack with no rule.
        has_bar: bool,
    },
    Sup {
        base: Box<Node>,
        sup: Box<Node>,
    },
    Sub {
        base: Box<Node>,
        sub: Box<Node>,
    },
    SubSup {
        base: Box<Node>,
        sub: Box<Node>,
        sup: Box<Node>,
    },
    Sqrt {
        body: Box<Node>,
        degree: Option<Box<Node>>,
    },
    /// Delimiters that grow to the enclosed expression.
    Fence {
        left: char,
        right: char,
        body: Box<Node>,
    },
    /// `\sum`, `\int`, `\lim`: a large operator with optional limits.
    BigOp {
        /// The operator's text, which is a word for `\lim` and a single glyph for
        /// `\sum`. Kept as text so the layout never has to know which it is.
        op: String,
        limits: Limits,
        sub: Option<Box<Node>>,
        sup: Option<Box<Node>>,
    },
    Accent {
        base: Box<Node>,
        accent: AccentKind,
    },
    Bar {
        body: Box<Node>,
        side: BarSide,
    },
    /// A fixed space, in mu (1/18 em).
    Space(i16),
}

/// Where a big operator's limits go: above/below in display style, as scripts
/// otherwise. The layout decides which; the parser only records what was written.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Limits {
    /// `\sum` -- stacked in a display formula, scripted inline.
    Default,
    /// `\lim`, `\max` -- always stacked.
    Always,
    /// `\int` -- always a script.
    Never,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AccentKind {
    Hat,
    Bar,
    Tilde,
    Dot,
    Vec,
}

impl AccentKind {
    /// The character set above the base. MATH's `AccentBaseHeight` flattening is
    /// applied by the layout; which glyph to use is this table's job.
    pub fn glyph(self) -> char {
        match self {
            AccentKind::Hat => '\u{302}',
            AccentKind::Bar => '\u{305}',
            AccentKind::Tilde => '\u{303}',
            AccentKind::Dot => '\u{307}',
            AccentKind::Vec => '\u{20d7}',
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BarSide {
    Over,
    Under,
}

pub struct Parser<'a> {
    src: &'a [u8],
    at: usize,
}

/// Parse a whole formula, the entry point callers use.
pub fn parse(src: &str) -> Node {
    Parser::new(src).formula()
}

impl<'a> Parser<'a> {
    pub fn new(s: &'a str) -> Parser<'a> {
        Parser { src: s.as_bytes(), at: 0 }
    }

    /// Parse the whole input as a row.
    pub fn formula(&mut self) -> Node {
        Node::Row(self.list(true))
    }

    fn peek(&self) -> Option<u8> {
        self.src.get(self.at).copied()
    }

    fn bump(&mut self) -> Option<u8> {
        let c = self.peek();
        if c.is_some() {
            self.at += 1;
        }
        c
    }

    fn rest(&self) -> &'a [u8] {
        &self.src[self.at.min(self.src.len())..]
    }

    /// True at a `\right` that ends a `\left` group, and not at the longer
    /// `\Rightarrow`-style names.
    fn at_right(&self) -> bool {
        self.rest().strip_prefix(b"\\right").is_some_and(|r| {
            !r.first().is_some_and(|c| c.is_ascii_alphabetic())
        })
    }

    /// Parse nodes until `}` or, when `top`, the end of input. A `\left` group also
    /// ends at its `\right`, which is the only way the pair can be matched up.
    fn list(&mut self, top: bool) -> Vec<Node> {
        let mut out: Vec<Node> = Vec::new();
        loop {
            if !top && self.at_right() {
                break;
            }
            match self.peek() {
                None => break,
                Some(b'}') => {
                    if top {
                        // A stray closer has no group to end, so it is just a
                        // character. Truncating the rest of the formula over it would
                        // lose the part the reader still needs.
                        self.bump();
                        out.push(Node::Atom("}".into()));
                        continue;
                    }
                    self.bump();
                    break;
                }
                Some(b'{') => {
                    self.bump();
                    let inner = Node::Row(self.list(false));
                    self.push(&mut out, inner);
                }
                Some(b'\\') => {
                    if let Some(n) = self.command() {
                        self.push(&mut out, n);
                    }
                }
                Some(b'^') | Some(b'_') => {
                    let c = self.bump().unwrap();
                    let arg = self.argument();
                    self.attach_script(&mut out, c == b'^', arg);
                }
                Some(b' ') => {
                    self.bump();
                }
                Some(b) => {
                    self.bump();
                    let ch = (b as char).to_string();
                    match out.last_mut() {
                        // Letters run together into one atom; digits and operators
                        // each stand alone, because their spacing differs.
                        Some(Node::Atom(s))
                            if letter_run(s) && (b as char).is_alphabetic() =>
                        {
                            s.push_str(&ch)
                        }
                        _ => out.push(Node::Atom(ch)),
                    }
                }
            }
        }
        out
    }

    /// Append a parsed node, merging plain atoms so runs of letters stay together.
    fn push(&mut self, out: &mut Vec<Node>, n: Node) {
        if matches!(n, Node::Atom(ref s) if s.is_empty()) {
            return;
        }
        if let (Node::Atom(a), Some(Node::Atom(b))) = (&n, out.last()) {
            if letter_run(b) && a.starts_with(char::is_alphabetic) {
                if let Some(Node::Atom(b)) = out.last_mut() {
                    b.push_str(a);
                    return;
                }
            }
        }
        out.push(n);
    }

    /// Attach a superscript or subscript to the node already parsed, which is the
    /// whole point: `x^2` is one atom with a script, not a row followed by a script.
    fn attach_script(&mut self, out: &mut Vec<Node>, is_sup: bool, arg: Node) {
        let prev = out.pop().unwrap_or(Node::Atom(String::new()));
        match (is_sup, prev) {
            (true, Node::BigOp { op, limits, sub, sup: None }) => {
                out.push(Node::BigOp { op, limits, sub, sup: Some(Box::new(arg)) });
            }
            (false, Node::BigOp { op, limits, sub: None, sup }) => out.push(Node::BigOp {
                op,
                limits,
                sub: Some(Box::new(arg)),
                sup,
            }),
            (_, Node::BigOp { op, limits, sub, sup }) => {
                // Both limits already given; a third can only sit beside the operator.
                let base = Node::BigOp { op, limits, sub, sup };
                out.push(Self::script(is_sup, base, arg));
            }
            (_, base) => match (is_sup, base) {
                // `x_a^b` and `x^b_a` have to agree, so merge when the other slot is free.
                (true, Node::Sub { base, sub }) => {
                    out.push(Node::SubSup { base, sub, sup: Box::new(arg) })
                }
                (false, Node::Sup { base, sup }) => {
                    out.push(Node::SubSup { base, sub: Box::new(arg), sup })
                }
                (is_sup, base) => out.push(Self::script(is_sup, base, arg)),
            },
        }
    }

    fn script(is_sup: bool, base: Node, arg: Node) -> Node {
        if is_sup {
            Node::Sup { base: Box::new(base), sup: Box::new(arg) }
        } else {
            Node::Sub { base: Box::new(base), sub: Box::new(arg) }
        }
    }

    /// One group, or the single character or command that follows.
    fn argument(&mut self) -> Node {
        match self.peek() {
            Some(b'{') => {
                self.bump();
                Node::Row(self.list(false))
            }
            Some(b'\\') => self.command().unwrap_or(Node::Atom(String::new())),
            Some(_) => {
                let c = self.bump().unwrap() as char;
                Node::Atom(c.to_string())
            }
            None => Node::Atom(String::new()),
        }
    }

    /// A `\name` control sequence, or a single escaped character like `\,`.
    fn command(&mut self) -> Option<Node> {
        self.bump(); // the backslash
        let first = self.peek()?;
        if !first.is_ascii_alphabetic() {
            self.bump();
            return Some(match first {
                b',' | b';' => Node::Space(if first == b',' { 3 } else { 5 }),
                b' ' => Node::Space(18),
                b'!' => Node::Space(-3),
                b'{' => Node::Atom("{".into()),
                b'}' => Node::Atom("}".into()),
                b'%' => Node::Atom("%".into()),
                b'$' => Node::Atom("$".into()),
                b'&' => Node::Atom("&".into()),
                b'\\' => Node::Space(0),
                _ => Node::Atom((first as char).to_string()),
            });
        }
        let start = self.at;
        while self.peek().is_some_and(|c| c.is_ascii_alphabetic()) {
            self.bump();
        }
        let name = std::str::from_utf8(&self.src[start..self.at]).ok()?.to_string();
        // Consume one space after a named command, as TeX does.
        if self.peek() == Some(b' ') {
            self.bump();
        }
        Some(self.named(&name))
    }

    fn named(&mut self, name: &str) -> Node {
        match name {
            "frac" | "dfrac" | "tfrac" => {
                let a = self.argument();
                let b = self.argument();
                Node::Frac { num: Box::new(a), den: Box::new(b), has_bar: true }
            }
            "binom" => {
                let a = self.argument();
                let b = self.argument();
                Node::Fence {
                    left: '(',
                    right: ')',
                    body: Box::new(Node::Frac {
                        num: Box::new(a),
                        den: Box::new(b),
                        has_bar: false,
                    }),
                }
            }
            "sqrt" => {
                let degree = if self.peek() == Some(b'[') {
                    self.bump();
                    let mut inner: Vec<u8> = Vec::new();
                    while let Some(c) = self.bump() {
                        if c == b']' {
                            break;
                        }
                        inner.push(c);
                    }
                    String::from_utf8_lossy(&inner).to_string()
                } else {
                    String::new()
                };
                let body = self.argument();
                Node::Sqrt {
                    body: Box::new(body),
                    degree: if degree.is_empty() {
                        None
                    } else {
                        Some(Box::new(Node::Atom(degree)))
                    },
                }
            }
            "left" => {
                let l = self.delim();
                let body = Node::Row(self.list(false));
                let r = if self.at_right() {
                    self.bump(); // the backslash of `\right`
                    let start = self.at;
                    while self.peek().is_some_and(|c| c.is_ascii_alphabetic()) {
                        self.bump();
                    }
                    let _ = start;
                    self.delim()
                } else {
                    // No closer written: the group still has to end somewhere.
                    '\0'
                };
                Node::Fence { left: l, right: r, body: Box::new(body) }
            }
            "right" => {
                let r = self.delim();
                Node::Atom(r.to_string())
            }
            "text" | "mathrm" | "operatorname" => {
                // Upright text: kept as atoms, since style is not modelled here.
                self.argument()
            }
            "overline" => Node::Bar { body: Box::new(self.argument()), side: BarSide::Over },
            "underline" => Node::Bar { body: Box::new(self.argument()), side: BarSide::Under },
            "hat" => self.accent(AccentKind::Hat),
            "widehat" => self.accent(AccentKind::Hat),
            "bar" | "overline_" => self.accent(AccentKind::Bar),
            "tilde" | "widetilde" => self.accent(AccentKind::Tilde),
            "dot" => self.accent(AccentKind::Dot),
            "vec" => self.accent(AccentKind::Vec),
            // A modifier on the preceding big operator, which the layout reads off
            // the node itself; emitting nothing keeps `a \lim\limits b` working.
            "limits" | "nolimits" => Node::Atom(String::new()),
            _ => self.symbol_node(name),
        }
    }

    fn accent(&mut self, kind: AccentKind) -> Node {
        Node::Accent { base: Box::new(self.argument()), accent: kind }
    }

    /// A command that denotes glyphs rather than structure: a big operator, an
    /// upright name like `\lim`, or a single symbol.
    fn symbol_node(&mut self, name: &str) -> Node {
        if let Some((op, limits)) = big_operator(name) {
            return Node::BigOp { op, limits, sub: None, sup: None };
        }
        match symbol(name) {
            Some(s) => Node::Atom(s.to_string()),
            // Unknown: show what was written rather than swallow it.
            None => Node::Atom(format!("\\{name}")),
        }
    }

    fn delim(&mut self) -> char {
        match self.peek() {
            Some(b'.') => {
                self.bump();
                '\0'
            }
            Some(b'\\') => {
                self.bump();
                self.command_delim()
            }
            Some(_) => self.bump().unwrap() as char,
            None => '\0',
        }
    }

    fn command_delim(&mut self) -> char {
        let start = self.at;
        while self.peek().is_some_and(|c| c.is_ascii_alphabetic()) {
            self.bump();
        }
        match &self.src[start..self.at] {
            b"langle" => '\u{27e8}',
            b"rangle" => '\u{27e9}',
            b"lbrace" => '{',
            b"rbrace" => '}',
            b"lbrack" => '[',
            b"rbrack" => ']',
            b"vert" | b"lvert" | b"rvert" | b"mid" => '|',
            b"Vert" | b"lVert" | b"rVert" => '\u{2016}',
            _ => '\0',
        }
    }
}

/// An atom that is a run of letters, and so may absorb the next letter. Testing the
/// first character rather than the last matters: an unknown command falls back to its
/// own text starting with a backslash, and that must not weld itself to the word
/// after it.
fn letter_run(s: &str) -> bool {
    s.chars().next().is_some_and(|c| c.is_alphabetic())
}

/// LaTeX command name to the text it denotes. Greek, the usual operators and
/// relations, arrows and sets; names that stand for a word (`\lim`) are listed here
/// too, and [`big_operator`] claims the ones that take limits first.
fn symbol(name: &str) -> Option<&'static str> {
    let c = match name {
        "alpha" => "α",
        "beta" => "β",
        "gamma" => "γ",
        "delta" => "δ",
        "epsilon" => "ϵ",
        "varepsilon" => "ε",
        "zeta" => "ζ",
        "eta" => "η",
        "theta" => "θ",
        "vartheta" => "ϑ",
        "iota" => "ι",
        "kappa" => "κ",
        "lambda" => "λ",
        "mu" => "μ",
        "nu" => "ν",
        "xi" => "ξ",
        "omicron" => "ο",
        "pi" => "π",
        "rho" => "ρ",
        "sigma" => "σ",
        "tau" => "τ",
        "upsilon" => "υ",
        "phi" => "ϕ",
        "varphi" => "φ",
        "chi" => "χ",
        "psi" => "ψ",
        "omega" => "ω",
        "Gamma" => "Γ",
        "Delta" => "Δ",
        "Theta" => "Θ",
        "Lambda" => "Λ",
        "Xi" => "Ξ",
        "Pi" => "Π",
        "Sigma" => "Σ",
        "Upsilon" => "Υ",
        "Phi" => "Φ",
        "Psi" => "Ψ",
        "Omega" => "Ω",
        "times" => "×",
        "div" => "÷",
        "pm" => "±",
        "mp" => "∓",
        "cdot" => "⋅",
        "ast" => "∗",
        "star" => "⋆",
        "circ" => "∘",
        "bullet" => "∙",
        "oplus" => "⊕",
        "otimes" => "⊗",
        "le" | "leq" => "≤",
        "ge" | "geq" => "≥",
        "ne" | "neq" => "≠",
        "equiv" => "≡",
        "approx" => "≈",
        "cong" => "≅",
        "sim" => "∼",
        "simeq" => "≃",
        "propto" => "∝",
        "ll" => "≪",
        "gg" => "≫",
        "in" => "∈",
        "notin" => "∉",
        "ni" => "∋",
        "subset" => "⊂",
        "supset" => "⊃",
        "subseteq" => "⊆",
        "supseteq" => "⊇",
        "cup" => "∪",
        "cap" => "∩",
        "emptyset" => "∅",
        "varnothing" => "∅",
        "forall" => "∀",
        "exists" => "∃",
        "nexists" => "∄",
        "neg" | "lnot" => "¬",
        "land" | "wedge" | "and" => "∧",
        "lor" | "vee" | "or" => "∨",
        "to" | "rightarrow" => "→",
        "leftarrow" => "←",
        "leftrightarrow" => "↔",
        "Rightarrow" => "⇒",
        "Leftarrow" => "⇐",
        "Leftrightarrow" => "⇔",
        "mapsto" => "↦",
        "implies" => "⟹",
        "iff" => "⟺",
        "infty" => "∞",
        "partial" => "∂",
        "nabla" => "∇",
        "aleph" => "ℵ",
        "hbar" => "ℏ",
        "ell" => "ℓ",
        "Re" => "ℜ",
        "Im" => "ℑ",
        "wp" => "℘",
        "imath" => "ı",
        "jmath" => "ȷ",
        "sum" => "∑",
        "prod" => "∏",
        "coprod" => "∐",
        "int" => "∫",
        "iint" => "∬",
        "iiint" => "∭",
        "oint" => "∮",
        "bigcup" => "⋃",
        "bigcap" => "⋂",
        // The function names set as themselves: upright in TeX, and a word rather
        // than a product of its letters, which is why they are atoms of their own.
        "lim" => "lim",
        "max" => "max",
        "min" => "min",
        "sup" => "sup",
        "inf" => "inf",
        "gcd" => "gcd",
        "sin" => "sin",
        "cos" => "cos",
        "tan" => "tan",
        "log" => "log",
        "ln" => "ln",
        "exp" => "exp",
        "det" => "det",
        "arg" => "arg",
        "deg" => "deg",
        "dim" => "dim",
        "ker" => "ker",
        "Pr" => "Pr",
        "cdots" => "⋯",
        "ldots" => "…",
        "vdots" => "⋮",
        "ddots" => "⋱",
        "dotsc" => "⋯",
        "angle" => "∠",
        "perp" => "⊥",
        "parallel" => "∥",
        "mid" => "∣",
        "therefore" => "∴",
        "because" => "∵",
        "prime" => "′",
        "backslash" => "∖",
        "setminus" => "∖",
        "lceil" => "⌈",
        "rceil" => "⌉",
        "lfloor" => "⌊",
        "rfloor" => "⌋",
        "langle" => "⟨",
        "rangle" => "⟩",
        "surd" => "√",
        "checkmark" => "✓",
        "dagger" => "†",
        "ddagger" => "‡",
        "S" => "§",
        "degree" => "°",
        "mathsterling" => "£",
        "quad" => "\u{2003}",
        "qquad" => "\u{2003}\u{2003}",
        _ => return None,
    };
    Some(c)
}

/// Which commands are big operators: the glyph or word they show, and how their
/// limits behave. Operators that are not stretchy or stacked still go through the
/// same node, so `\int_0^1` and `\sum_{i=1}^n` take one path.
pub fn big_operator(name: &str) -> Option<(String, Limits)> {
    let text = symbol(name)?;
    let limits = match name {
        "int" | "iint" | "iiint" | "oint" => Limits::Never,
        "lim" | "max" | "min" | "sup" | "inf" | "gcd" => Limits::Always,
        "sum" | "prod" | "coprod" | "bigcup" | "bigcap" => Limits::Default,
        _ => return None,
    };
    Some((text.to_string(), limits))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The tree as a compact string, so an expectation says what was parsed instead
    /// of rebuilding a `Box<Node>` by hand.
    fn sexp(n: &Node) -> String {
        match n {
            Node::Atom(s) => s.clone(),
            Node::Row(v) if v.is_empty() => String::new(),
            // A row of one is just that one: `{x}` and `x` must not look different.
            Node::Row(v) if v.len() == 1 => sexp(&v[0]),
            Node::Row(v) => format!("({})", v.iter().map(sexp).collect::<Vec<_>>().join(" ")),
            Node::Frac { num, den, has_bar } => format!(
                "({} {} {})",
                if *has_bar { "frac" } else { "nob" },
                sexp(num),
                sexp(den)
            ),
            Node::Sup { base, sup } => format!("(sup {} {})", sexp(base), sexp(sup)),
            Node::Sub { base, sub } => format!("(sub {} {})", sexp(base), sexp(sub)),
            Node::SubSup { base, sub, sup } => {
                format!("(subsup {} {} {})", sexp(base), sexp(sub), sexp(sup))
            }
            Node::Sqrt { body, degree } => match degree {
                Some(d) => format!("(sqrt {} {})", sexp(d), sexp(body)),
                None => format!("(sqrt {})", sexp(body)),
            },
            Node::Fence { left, right, body } => {
                format!("(fence {}{} {})", shown(*left), shown(*right), sexp(body))
            }
            Node::BigOp { op, limits, sub, sup } => format!(
                "(big {op}{} {} {})",
                match limits {
                    Limits::Default => "",
                    Limits::Always => "!",
                    Limits::Never => "?",
                },
                sub.as_ref().map_or_else(|| "-".into(), |s| sexp(s)),
                sup.as_ref().map_or_else(|| "-".into(), |s| sexp(s)),
            ),
            Node::Accent { base, accent } => format!("(accent {accent:?} {})", sexp(base)),
            Node::Bar { body, side } => format!("(bar {side:?} {})", sexp(body)),
            Node::Space(mu) => format!("(space {mu})"),
        }
    }

    /// A delimiter the source never wrote shows as `-`, which is the one thing a
    /// reader has to be able to see in an expectation.
    fn shown(c: char) -> String {
        if c == '\0' {
            "-".into()
        } else {
            c.to_string()
        }
    }

    fn of(src: &str) -> String {
        sexp(&parse(src))
    }

    #[test]
    fn letters_run_together_and_a_script_takes_the_whole_run() {
        assert_eq!(of("xy^2"), "(sup xy 2)");
        // Digits and operators stand alone, because their spacing differs from a
        // variable's.
        assert_eq!(of("a+b"), "(a + b)");
        assert_eq!(of("ab1c"), "(ab 1 c)");
    }

    #[test]
    fn both_script_orders_converge() {
        assert_eq!(of("x_a^b"), of("x^b_a"));
        assert_eq!(of("x_a^b"), "(subsup x a b)");
        // A second script on a script nests instead of being lost.
        assert_eq!(of("x^a^b"), "(sup (sup x a) b)");
    }

    #[test]
    fn commands_take_their_arguments_as_groups() {
        assert_eq!(of("\\frac{1}{2}"), "(frac 1 2)");
        assert_eq!(of("\\sqrt{x+1}"), "(sqrt (x + 1))");
        assert_eq!(of("\\sqrt[3]{x}"), "(sqrt 3 x)");
        // An unbraced argument is one token. Reading `rac` as the command name here
        // is the bug this guards: `argument` must not eat the backslash twice.
        assert_eq!(of("\\frac12"), "(frac 1 2)");
        assert_eq!(of("\\hat x"), "(accent Hat x)");
    }

    #[test]
    fn big_operators_carry_their_limits() {
        assert_eq!(of("\\sum_{i=1}^{n}"), "(big ∑ (i = 1) n)");
        assert_eq!(of("\\int_0^1 x"), "((big ∫? 0 1) x)");
        assert_eq!(of("\\lim_{x\\to 0}"), "(big lim! (x → 0) -)");
        assert_eq!(big_operator("sum").unwrap(), ("∑".into(), Limits::Default));
        assert_eq!(big_operator("int").unwrap().1, Limits::Never, "an integral never stacks");
        assert_eq!(big_operator("lim").unwrap().1, Limits::Always);
        assert_eq!(big_operator("alpha"), None, "a letter is not an operator");
    }

    #[test]
    fn left_right_pairs_are_matched() {
        assert_eq!(of("\\left(\\frac{a}{b}\\right)"), "(fence () (frac a b))");
        assert_eq!(of("\\left[ x \\right]"), "(fence [] x)");
        assert_eq!(of("\\left\\langle x\\right\\rangle"), "(fence ⟨⟩ x)");
        // The body of a `\\left` must stop at its `\\right`, or the closer is eaten as
        // an operator and the pair never forms.
        assert_eq!(of("\\left(a\\right)b"), "((fence () a) b)");
        assert_eq!(of("\\left(x"), "(fence (- x)", "an unclosed group still yields the row");
        assert_eq!(of("\\left.x\\right|"), "(fence -| x)");
        // `\\rightarrow` is a relation, not a truncated `\\right`.
        assert_eq!(of("\\left(a\\rightarrow b\\right)"), "(fence () (a → b))");
    }

    #[test]
    fn spacing_commands_become_mu_glue() {
        assert_eq!(of("a\\,b"), "(a (space 3) b)");
        assert_eq!(of("a\\!b"), "(a (space -3) b)");
        assert_eq!(of("a\\;b"), "(a (space 5) b)");
    }

    #[test]
    fn malformed_input_degrades_to_its_own_text() {
        assert_eq!(of("\\frobnicate x"), "(\\frobnicate x)");
        assert_eq!(of("a}b"), "(a } b)", "a stray closer does not truncate the rest");
        assert_eq!(of("\\frac{1}"), "(frac 1 )");
        assert!(
            matches!(parse(""), Node::Row(v) if v.is_empty()),
            "empty input is an empty row, not a panic"
        );
        assert_eq!(of("x^"), "(sup x )");
        assert_eq!(of("\\frac{}{2}"), "(frac  2)");
    }

    #[test]
    fn symbol_names_cover_what_people_actually_write() {
        for src in [
            "\\alpha\\beta\\gamma",
            "\\sum\\prod\\int\\oint",
            "\\le\\ge\\ne\\approx\\equiv\\propto",
            "\\in\\subset\\cup\\cap\\emptyset",
            "\\forall\\exists\\neg\\wedge\\vee",
            "\\infty\\partial\\nabla\\hbar\\ell",
            "\\to\\leftarrow\\Rightarrow\\mapsto",
            "\\sin\\cos\\log\\ln\\exp\\det",
            "\\cdots\\ldots\\vdots",
            "\\lceil x\\rceil",
        ] {
            let f = parse(src);
            let out = sexp(&f);
            assert!(!out.contains('\\'), "{src} fell back to literal text: {out}");
        }
    }
}
