//! A LaTeX subset for formulas, parsed into the tree the layout engine consumes.
//!
//! Deliberately a subset, not a TeX implementation: the goal is the math people
//! actually write in Markdown, so fractions, scripts, radicals, stretchy delimiters,
//! big operators with limits, accents, multi-row environments and the common symbol
//! names are handled, and anything else degrades to its literal characters rather than
//! disappearing.
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
    /// A grid of cells: the `matrix`, `cases`, `aligned` and `array` families, which
    /// are the only forms in the language that need more than one row.
    ///
    /// `rows` holds one entry per cell, each cell already gathered into a row of its
    /// own nodes, and `columns` -- whose length is the widest row's cell count -- says
    /// how a cell sits inside its column's width. `delimiters` is the pair the
    /// environment brings with it, `'\0'` on either side meaning none there, which is
    /// how `cases` carries its lone brace; the layout grows them to the grid's height
    /// exactly as it grows a `\left ... \right` pair.
    Array {
        rows: Vec<Vec<Node>>,
        columns: Vec<ColAlign>,
        kind: ArrayKind,
        delimiters: Option<(char, char)>,
    },
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

/// Where a cell sits inside the width its column was measured to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColAlign {
    Left,
    Center,
    Right,
}

/// Which family an environment belongs to, which is what decides the gaps the layout
/// uses rather than the alignments -- those are already spelled out per column.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArrayKind {
    /// `matrix` and its bracketed spellings: centred columns.
    Matrix,
    /// `smallmatrix`, set one script step down as the name asks.
    SmallMatrix,
    /// `cases`: two left-aligned columns with room before the condition.
    Cases,
    /// `aligned`, `align`: `&` is an alignment tab, so the columns alternate
    /// right-aligned and left-aligned about it and a relation stays in place.
    Align,
    /// `gathered`: centred, one cell per row in the usual case.
    Gathered,
    /// `array`: the column alignments come from the `{ccc}` argument.
    Array,
}

pub struct Parser<'a> {
    src: &'a [u8],
    at: usize,
}

/// What a run of nodes is being collected up to. Inside an environment the cell and
/// row separators are structure, so they end the run instead of turning into text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Ctx {
    /// The whole formula: nothing closes it, and `&`, `\\` and a stray `\end` are text.
    Top,
    /// A brace group or a `\left` body: ends at `}` or at the matching `\right`.
    Group,
    /// One cell of an environment: also ends at `&`, `\\` and any `\end`, none of
    /// which is consumed here -- the environment reads them itself.
    Cell,
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
        Node::Row(self.list(Ctx::Top))
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
        self.word_at(b"\\right")
    }

    /// True at an `\end`, which closes -- or fails to close -- the environment being
    /// read, and not at an `\endash`-style name.
    fn at_end(&self) -> bool {
        self.word_at(b"\\end")
    }

    fn word_at(&self, word: &[u8]) -> bool {
        self.rest().strip_prefix(word).is_some_and(|r| {
            !r.first().is_some_and(|c| c.is_ascii_alphabetic())
        })
    }

    /// True at the end of the cell being collected: a cell separator, a row separator
    /// or the `\end` that closes the environment.
    fn at_cell_break(&self) -> bool {
        matches!(self.peek(), Some(b'&')) || self.at_row_break() || self.at_end()
    }

    fn at_row_break(&self) -> bool {
        self.rest().starts_with(b"\\\\")
    }

    /// Parse nodes until `}` or, when the run is the whole formula, the end of input.
    /// A `\left` group also ends at its `\right`, which is the only way the pair can be
    /// matched up, and a cell ends at the separators that structure it.
    fn list(&mut self, ctx: Ctx) -> Vec<Node> {
        let mut out: Vec<Node> = Vec::new();
        loop {
            if ctx != Ctx::Top && self.at_right() {
                break;
            }
            if ctx == Ctx::Cell && self.at_cell_break() {
                break;
            }
            match self.peek() {
                None => break,
                Some(b'}') => {
                    if ctx == Ctx::Top {
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
                    let inner = Node::Row(self.list(Ctx::Group));
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
                Node::Row(self.list(Ctx::Group))
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
            "begin" => self.environment(),
            "end" => {
                // An `\end` with no `\begin` to answer it: show what was written,
                // braces and all, instead of cutting the formula short over it.
                let name = self.braced_text().unwrap_or_default();
                Node::Atom(format!("\\end{{{name}}}"))
            }
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
                let body = Node::Row(self.list(Ctx::Group));
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

    /// `\begin{X} ... \end{X}`: a grid of cells. The name decides the kind, and with
    /// it the alignments and the delimiters the environment brings.
    ///
    /// Three failures are handled the same way -- an environment whose name is not one
    /// of ours, one that is never closed, and one closed by an `\end` naming something
    /// else -- by setting the cells that were read inline beside the literal marker
    /// text. A reader loses the arrangement, not the mathematics.
    fn environment(&mut self) -> Node {
        let Some(name) = self.braced_text() else {
            // `\begin` with nothing after it: the word itself, and no more eaten.
            return Node::Atom("\\begin".into());
        };
        let Some(kind) = array_kind(&name) else {
            let (rows, end) = self.env_rows();
            return Self::degraded(&name, rows, end.as_deref());
        };
        // `array` is the one environment whose columns are written out, and the
        // argument has to be taken here or its braces land in the first cell.
        let spec = if kind == ArrayKind::Array { self.column_spec() } else { Vec::new() };
        let (rows, end) = self.env_rows();
        if end.as_deref() != Some(name.as_str()) {
            return Self::degraded(&name, rows, end.as_deref());
        }
        let cols = rows.iter().map(|r| r.len()).max().unwrap_or(0).max(1);
        let columns = columns_for(kind, &spec, cols);
        let rows = rows.into_iter().map(|r| r.into_iter().map(Node::Row).collect()).collect();
        Node::Array { rows, columns, kind, delimiters: env_delimiters(&name) }
    }

    /// The cells of an environment's body, split on `&` and `\\`, and the name its
    /// `\end` carried if it reached one. Whatever closed the run -- a matching `\end`,
    /// a mismatched one, a stray `}`, a `\right` belonging to an enclosing group or the
    /// end of the input -- the search stops there rather than eating what follows.
    fn env_rows(&mut self) -> (Vec<Vec<Vec<Node>>>, Option<String>) {
        let mut rows: Vec<Vec<Vec<Node>>> = Vec::new();
        let mut row: Vec<Vec<Node>> = Vec::new();
        let mut end = None;
        loop {
            let cell = self.list(Ctx::Cell);
            if self.peek() == Some(b'&') {
                self.bump();
                row.push(cell);
                continue;
            }
            // A separator with nothing around it is punctuation, not an empty row: a
            // trailing `\\` before the closer, or two in a row.
            let blank = cell.is_empty() && row.is_empty();
            if self.at_row_break() {
                self.bump_row_break();
                if !blank {
                    row.push(cell);
                    rows.push(std::mem::take(&mut row));
                }
                continue;
            }
            if !blank {
                row.push(cell);
                rows.push(std::mem::take(&mut row));
            }
            if self.at_end() {
                end = self.eat_end();
            }
            return (rows, end);
        }
    }

    /// Consume a `\\`, its optional `*` and its optional `[distance]`. The row gap is
    /// not varied per row, but the bracket must not leak into the cell after it.
    fn bump_row_break(&mut self) {
        self.bump();
        self.bump();
        if self.peek() == Some(b'*') {
            self.bump();
        }
        self.skip_bracketed();
    }

    /// Consume `\end` -- which the caller has just seen -- and its braced name.
    fn eat_end(&mut self) -> Option<String> {
        self.at += 4; // `\end`, the four bytes `at_end` has just checked for
        while self.peek() == Some(b' ') {
            self.bump();
        }
        self.braced_text()
    }

    /// An environment's content set in a row, with the markers that failed shown as
    /// the literal text they are -- what an unknown command already does.
    fn degraded(name: &str, rows: Vec<Vec<Vec<Node>>>, end: Option<&str>) -> Node {
        let mut out: Vec<Node> = vec![Node::Atom(format!("\\begin{{{name}}}"))];
        for row in rows {
            for mut cell in row {
                out.append(&mut cell);
            }
        }
        if let Some(end) = end {
            out.push(Node::Atom(format!("\\end{{{end}}}")));
        }
        Node::Row(out)
    }

    /// A braced word right here, with both braces consumed: an environment's name or
    /// an `array`'s column spec. `None` and nothing eaten when the braces are not
    /// there, so a stray `\begin x` leaves the `x` to the formula around it.
    fn braced_text(&mut self) -> Option<String> {
        if self.peek() != Some(b'{') {
            return None;
        }
        let close = self.src[self.at + 1..].iter().position(|c| *c == b'}')?;
        let s = std::str::from_utf8(&self.src[self.at + 1..self.at + 1 + close]).ok()?;
        self.at += close + 2; // past the contents and both braces
        Some(s.to_string())
    }

    /// The `{ccc}` / `{l|l}` argument an `array` takes. Only `l`, `c` and `r` choose an
    /// alignment; the vertical rules and anything else are consumed and dropped, since
    /// a rule between columns is not something the layout draws yet.
    fn column_spec(&mut self) -> Vec<ColAlign> {
        let Some(spec) = self.braced_text() else {
            return Vec::new();
        };
        spec.chars()
            .filter_map(|c| match c {
                'l' => Some(ColAlign::Left),
                'c' => Some(ColAlign::Center),
                'r' => Some(ColAlign::Right),
                _ => None,
            })
            .collect()
    }

    /// A `[...]` option, skipped. Nothing is consumed unless the closer is there, so an
    /// unterminated bracket cannot swallow the rest of the formula.
    fn skip_bracketed(&mut self) {
        if self.peek() != Some(b'[') {
            return;
        }
        if let Some(end) = self.src[self.at + 1..].iter().position(|c| *c == b']') {
            self.at += end + 2;
        }
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

/// Which of the multi-row families an environment name belongs to. `None` is an
/// environment the layout cannot draw, which the caller degrades instead of failing;
/// the starred and unstarred spellings of `align` and `gather` differ only in whether
/// TeX numbers the rows, which a reader never sees here.
fn array_kind(name: &str) -> Option<ArrayKind> {
    Some(match name {
        "matrix" | "pmatrix" | "bmatrix" | "Bmatrix" | "vmatrix" | "Vmatrix" => ArrayKind::Matrix,
        "smallmatrix" => ArrayKind::SmallMatrix,
        "cases" | "numcases" | "dcases" => ArrayKind::Cases,
        "aligned" | "align" | "align*" | "alignat" | "flalign" => ArrayKind::Align,
        "gathered" | "gather" | "gather*" => ArrayKind::Gathered,
        "array" | "subarray" => ArrayKind::Array,
        _ => return None,
    })
}

/// The delimiters an environment is drawn with, `'\0'` standing for none on that side
/// exactly as it does in [`Node::Fence`]. `cases` is the conventional lone brace;
/// `matrix` and `smallmatrix` come with nothing, and `aligned` never has any.
fn env_delimiters(name: &str) -> Option<(char, char)> {
    Some(match name {
        "pmatrix" => ('(', ')'),
        "bmatrix" => ('[', ']'),
        "Bmatrix" => ('\u{27e8}', '\u{27e9}'),
        "vmatrix" => ('|', '|'),
        "Vmatrix" => ('\u{2016}', '\u{2016}'),
        "cases" | "numcases" | "dcases" => ('{', '\0'),
        _ => return None,
    })
}

/// The alignment of each of an environment's `cols` columns. Every family but `array`
/// has one rule for all of them; an `array` whose spec names fewer columns than it got
/// repeats the last one it was given, which is the reading that keeps the text aligned
/// that the writer meant to be.
fn columns_for(kind: ArrayKind, spec: &[ColAlign], cols: usize) -> Vec<ColAlign> {
    (0..cols)
        .map(|j| match kind {
            ArrayKind::Array => spec
                .get(j)
                .or(spec.last())
                .copied()
                .unwrap_or(ColAlign::Left),
            ArrayKind::Cases => ColAlign::Left,
            ArrayKind::Align => if j.is_multiple_of(2) { ColAlign::Right } else { ColAlign::Left },
            ArrayKind::Matrix | ArrayKind::SmallMatrix | ArrayKind::Gathered => ColAlign::Center,
        })
        .collect()
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
            // Columns print as the `l`/`c`/`r` letters they were written with, rows
            // separated by ` / ` and cells by ` & `, which is the shape of the source.
            Node::Array { rows, columns, kind, delimiters } => format!(
                "(array {kind:?} {} {} {})",
                columns.iter().map(col_letter).collect::<String>(),
                delimiters.map_or_else(
                    || "none".into(),
                    |(l, r)| format!("{}{}", shown(l), shown(r)),
                ),
                rows.iter()
                    .map(|r| format!("[{}]", r.iter().map(sexp).collect::<Vec<_>>().join(" & ")))
                    .collect::<Vec<_>>()
                    .join(" / "),
            ),
        }
    }

    fn col_letter(a: &ColAlign) -> char {
        match a {
            ColAlign::Left => 'l',
            ColAlign::Center => 'c',
            ColAlign::Right => 'r',
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

    #[test]
    fn environments_come_out_as_rows_of_cells() {
        assert_eq!(
            of("\\begin{pmatrix} a & b \\\\ c & d \\end{pmatrix}"),
            "(array Matrix cc () [a & b] / [c & d])"
        );
        assert_eq!(
            of("\\begin{cases} x & x > 0 \\\\ -x & \\text{otherwise} \\end{cases}"),
            "(array Cases ll {- [x & (x > 0)] / [(- x) & otherwise])",
            "a piecewise definition keeps both columns and its lone brace"
        );
        assert_eq!(
            of("\\begin{gathered} a \\\\ b \\end{gathered}"),
            "(array Gathered c none [a] / [b])"
        );
        for (env, kind, want) in [
            ("matrix", "Matrix", "none"),
            ("pmatrix", "Matrix", "()"),
            ("bmatrix", "Matrix", "[]"),
            ("Bmatrix", "Matrix", "⟨⟩"),
            ("vmatrix", "Matrix", "||"),
            ("smallmatrix", "SmallMatrix", "none"),
        ] {
            let src = format!("\\begin{{{env}}} a \\end{{{env}}}");
            assert_eq!(of(&src), format!("(array {kind} c {want} [a])"), "{src}");
        }
        // `Vmatrix` is the doubled bar, which needs its own character to be visible.
        assert_eq!(
            of("\\begin{Vmatrix} a \\end{Vmatrix}"),
            format!("(array Matrix c \u{2016}\u{2016} [a])")
        );
        // `aligned` reads `&` as an alignment tab, so the two halves of a row become
        // two columns whose alignments point at the tab from either side.
        assert_eq!(
            of("\\begin{aligned} x &= 1 \\\\ y &= 2 \\end{aligned}"),
            "(array Align rl none [x & (= 1)] / [y & (= 2)])"
        );
        assert_eq!(
            of("\\begin{align*} x &= 1 \\end{align*}"),
            "(array Align rl none [x & (= 1)])",
            "the starred spelling is the same arrangement"
        );
    }

    #[test]
    fn an_arrays_column_spec_is_consumed_not_shown() {
        // The vertical rules are dropped -- the layout draws no column rules yet -- but
        // they cannot be allowed to leak into the first cell as text either.
        assert_eq!(
            of("\\begin{array}{l|r} a & b \\end{array}"),
            "(array Array lr none [a & b])"
        );
        assert_eq!(
            of("\\begin{array}{cc} a & b \\\\ c & d \\end{array}"),
            "(array Array cc none [a & b] / [c & d])"
        );
        // A spec shorter than the row repeats its last column rather than inventing a
        // centred one.
        assert_eq!(
            of("\\begin{array}{cr} a & b & c \\end{array}"),
            "(array Array crr none [a & b & c])"
        );
        assert_eq!(
            of("\\begin{array} a \\end{array}"),
            "(array Array l none [a])",
            "no spec at all is still a grid, not an error"
        );
    }

    #[test]
    fn row_and_cell_separators_end_rows_without_leaving_gaps() {
        // A trailing `\\` is punctuation, not an empty row.
        assert_eq!(
            of("\\begin{matrix} a \\\\ b \\\\ \\end{matrix}"),
            "(array Matrix c none [a] / [b])"
        );
        // `\\*` and `\\[3pt]` are the same row break; the bracket must not reach the
        // cell after it.
        assert_eq!(
            of("\\begin{matrix} a \\\\*[3pt] b \\end{matrix}"),
            "(array Matrix c none [a] / [b])"
        );
        // An `&` with no cell after it is still a column, because the writer asked for
        // one by typing the tab.
        assert_eq!(of("\\begin{matrix} a & \\end{matrix}"), "(array Matrix cc none [a & ])");
        assert_eq!(of("\\begin{matrix} \\end{matrix}"), "(array Matrix c none )", "empty grid");
    }

    #[test]
    fn an_environment_nests_in_a_group_and_in_another_cell() {
        assert_eq!(
            of("\\left(\\begin{matrix} a \\\\ b \\end{matrix}\\right)"),
            "(fence () (array Matrix c none [a] / [b]))"
        );
        assert_eq!(
            of("\\begin{pmatrix} \\frac{1}{2} & \\sqrt{x} \\end{pmatrix}"),
            "(array Matrix cc () [(frac 1 2) & (sqrt x)])",
            "a cell holds whatever a group can"
        );
        assert_eq!(
            of("\\begin{matrix} \\begin{matrix} a \\end{matrix} & b \\end{matrix}"),
            "(array Matrix cc none [(array Matrix c none [a]) & b])"
        );
        assert_eq!(
            of("x\\begin{pmatrix} a \\end{pmatrix}^{2}"),
            "(x (sup (array Matrix c () [a]) 2))",
            "a script attaches to the whole grid"
        );
    }

    #[test]
    fn a_broken_environment_degrades_to_its_own_text() {
        // Unknown name: the markers show as written and the content survives.
        assert_eq!(
            of("\\begin{psst} a & b \\end{psst}"),
            "(\\begin{psst} a b \\end{psst})"
        );
        // Never closed: what was read is still set, inline.
        assert_eq!(of("\\begin{pmatrix} a \\\\ b + c"), "(\\begin{pmatrix} a b + c)");
        // Closed by the wrong name: the grid is abandoned, but neither the written
        // closer nor the material after it is swallowed.
        assert_eq!(
            of("\\begin{pmatrix} a \\end{bmatrix} x"),
            "((\\begin{pmatrix} a \\end{bmatrix}) x)"
        );
        // A bare `\end`, and a `\begin` with no name, are one literal each.
        assert_eq!(of("\\end{matrix}"), "\\end{matrix}");
        assert_eq!(of("\\begin x"), "(\\begin x)");
        // None of them ends the formula early.
        assert_eq!(
            of("\\begin{psst}a\\end{psst}\\frac{1}{2}"),
            "((\\begin{psst} a \\end{psst}) (frac 1 2))"
        );
    }
}
