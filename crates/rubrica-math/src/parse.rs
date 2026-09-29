//! A LaTeX subset for formulas, parsed into the tree the layout engine consumes.
//!
//! Deliberately a subset, not a TeX implementation: the goal is the math people
//! actually write in Markdown, so fractions, scripts, radicals, stretchy delimiters,
//! big operators with limits, accents, stacked labels and framed boxes, lettering
//! switches (`\mathbb`, `\mathbf`), multi-row environments and the common symbol names
//! are handled, and anything else degrades to its literal characters rather than
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
        /// Whether the fraction takes its proportions from the formula around it.
        style: FracStyle,
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
        /// Whether the author asked for the *wide* spelling. Which glyph is written
        /// down is the same either way -- `\widehat` is `\hat` drawn from the face's
        /// wider drawings -- so this is not a second kind of accent but a licence: only
        /// the wide form may be stretched across its base, and `\hat` over a word is
        /// meant to sit small on top of it.
        wide: bool,
    },
    /// `\bigl`, `\Big`, `\biggr` and the rest: one delimiter, sized by the author
    /// rather than by the body around it.
    Big {
        delim: char,
        /// Which of the four heights was asked for, 1 through 4.
        step: u8,
        role: BigRole,
    },
    /// `\overline` / `\underline`: a rule the width of the body.
    Bar {
        body: Box<Node>,
        side: BarSide,
    },
    /// `\overbrace` and `\underbrace`: the same geometry as a [`Node::Bar`], but the
    /// mark is a brace grown across the whole body from the face's horizontal
    /// coverage -- the same half of the `MATH` table a wide accent is read from.
    Brace {
        body: Box<Node>,
        side: BarSide,
    },
    /// `\overset`, `\underset` and `\stackrel`: a label stacked over or under a base.
    ///
    /// All three are TeX's forced `\limits` on a base operator, so the label is a
    /// script-sized box in the positions a display limit takes and the construct keeps
    /// the spacing class of its base -- `\overset{?}{=}` is still a relation. `side`
    /// names which side of the base the label goes, reusing the two sides a
    /// [`Node::Bar`] already speaks of.
    Stack {
        base: Box<Node>,
        label: Box<Node>,
        side: BarSide,
    },
    /// `\boxed`, `\fbox`: the body inside a rectangle.
    Boxed { body: Box<Node> },
    /// A fixed space, in mu (1/18 em).
    Space(i16),
    /// `\displaystyle`, `\textstyle`, `\scriptstyle`, `\scriptscriptstyle`: the body is
    /// the group the switch was written in, as far as this parser can tell it -- see
    /// [`MathStyle`].
    Styled { body: Box<Node>, style: MathStyle },
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
        /// A rule across the grid before row `i`, for `i` in `0..=rows.len()`: the last
        /// entry is the rule *below* the last row, which is where an author written
        /// `\hline` before `\end` puts it. One entry per boundary, so `rules.len()` is
        /// always one more than `rows.len()` -- `\hline` twice in a row is one rule,
        /// because two adjacent rules would be drawn on top of each other.
        rules: Vec<bool>,
        /// The column rules the spec's `|` asked for, one entry per boundary: before each
        /// column and one after the last, so its length is the column count plus one. An
        /// environment that names none leaves this empty, which is read as none.
        col_rules: Vec<bool>,
    },
}

/// Whether a fraction is set by the formula around it or by its own name.
///
/// `\dfrac` and `\tfrac` are the same stack read at two sizes: one keeps a displayed
/// formula's proportions wherever it is written, the other is put small enough to sit in
/// a line of prose without pushing that line apart.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FracStyle {
    /// Take the surrounding style, which is plain `\frac`.
    #[default]
    Auto,
    /// Display proportions, whatever the formula around it is doing.
    Display,
    /// Text proportions, at one script step down.
    Text,
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
    /// `\ddot`: two dots, which is a second derivative and not two `\dot`s.
    Ddot,
    Vec,
    /// `\overleftarrow`: a direction is not a decoration one way only.
    Backvec,
    /// The marks of the phonetic and the accented-Latin alphabet, none of which a
    /// reader can reach any other way.
    Check,
    Breve,
    Acute,
    Grave,
    Mathring,
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
            AccentKind::Ddot => '\u{308}',
            AccentKind::Vec => '\u{20d7}',
            AccentKind::Backvec => '\u{20d6}',
            AccentKind::Check => '\u{30c}',
            AccentKind::Breve => '\u{306}',
            AccentKind::Acute => '\u{301}',
            AccentKind::Grave => '\u{300}',
            AccentKind::Mathring => '\u{30a}',
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BarSide {
    Over,
    Under,
}

/// What spacing a hand-sized delimiter stands as, which is the one thing the four
/// spellings of each height differ by: `\bigl(` opens, `\bigr)` closes, `\bigm|` relates
/// and a bare `\big(` is ordinary.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BigRole {
    Open,
    Close,
    Rel,
    Ord,
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
    /// The lettering a switch command put the parser into, for as long as its argument
    /// is being read: `\mathbb{R^n}` reaches the `n`, `\mathbb{R}^n` does not. `None`
    /// is the state a formula is read in, where a written letter is math italic.
    alphabet: Option<Alphabet>,
    /// Whether the node just collected is a run of written letters, which is the only
    /// thing a following written letter joins. A command's own text is not: welding
    /// `\sin` onto the `x` after it would set the operator's name as a variable.
    welding: bool,
    /// An infix fraction command seen while collecting, waiting for the run around it to
    /// be assembled into the two halves it asks for. See [`Parser::list`].
    infix: Option<Infix>,
    /// Set by an `\hline` and claimed by the environment reader, which is the only thing
    /// that knows whether the rule it stands for goes above the next row or below the
    /// last one.
    hline: bool,
    /// A style switch read while collecting, which the run it belongs to wraps itself in
    /// when it closes. See [`Node::Styled`].
    style: Option<MathStyle>,
    /// The boundaries an environment's rows were read with, claimed by the builder that
    /// read them. See [`Parser::env_rows`].
    row_rules: Vec<bool>,
    /// How deep the braces, commands and environments have stacked. A formula is
    /// reader-supplied text, and `{{{{{...` or `\frac\frac\frac...` built to do
    /// exactly one thing would otherwise recurse once per level until the stack is
    /// gone; past the cap the parser reads them whole, linearly, instead of
    /// descending.
    depth: usize,
    /// Consecutive script attachments -- `x^a^b^c...` wraps one Box around
    /// another per token, and unwinding that chain recurses as deep as it is
    /// long, in the drop as well as the layout. The counter resets wherever
    /// something other than a script lands, so only the chain itself is counted.
    script_chain: usize,
}

/// The two commands that sit *between* their operands instead of in front of them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Infix {
    /// `{a \over b}`: the stacked fraction, with its rule.
    Over,
    /// `{n \choose k}`: the same stack, in parentheses and without the rule.
    Choose,
    /// `{a \atop b}`: the stack with no rule and nothing around it.
    Atop,
    /// `{a \brace b}` and `{a \brack b}`: the stack in the author's own braces or
    /// brackets, which is how `\begin{Bmatrix}` looks when it is written the old way.
    Brace,
    Brack,
    /// `{a \above 4pt b}`: `\over` with the rule's gap asked for. The gap is the
    /// layout's own business, so the dimension is read and dropped.
    Above,
}

/// The style a run of the formula asks to be set in, written with TeX's four switch
/// commands. These are not decoration: `\displaystyle` is what makes a fraction in an
/// inline formula keep its displayed proportions and a `\sum` stack its limits, and
/// `\scriptstyle` is the name for the size two levels down.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MathStyle {
    Display,
    Text,
    Script,
    ScriptScript,
}

/// The style a run of nodes is being collected up to. Inside an environment the cell and
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
    /// Brace and command nesting the parser will actually represent. A formula has
    /// no business being a hundred levels deep -- TeX's own macros bottom out long
    /// before -- and past this point the reader loses the arrangement, not the
    /// mathematics, rather than the process losing the stack.
    const MAX_DEPTH: usize = 100;

    pub fn new(s: &'a str) -> Parser<'a> {
        Parser {
            src: s.as_bytes(),
            at: 0,
            alphabet: None,
            welding: false,
            infix: None,
            hline: false,
            style: None,
            row_rules: Vec::new(),
            depth: 0,
            script_chain: 0,
        }
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
        // The style this run was started in. A group nested inside a switch claims it
        // while it is read -- that is the scope a brace group gives -- and hands it back
        // on the way out, so the switch persists to the end of the group it was written
        // in, which is what TeX's declarations do. Without this the rest of the outer
        // run after a nested group lost the switch it was written in.
        let incoming = self.style;
        let mut out: Vec<Node> = Vec::new();
        // What was collected before an `\over` or a `\choose`, which turns the whole run
        // into a fraction: TeX takes everything before the command in this group as the
        // numerator and everything after it as the denominator.
        let mut above: Option<Vec<Node>> = None;
        // Which of the two infix commands made that split, kept so the group can be
        // closed with the rule and the parentheses the author's name asked for.
        let mut stored: Option<Infix> = None;
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
                        self.push(&mut out, Node::Atom("}".into()));
                        continue;
                    }
                    self.bump();
                    break;
                }
                Some(b'{') => {
                    self.bump();
                    let inner = if self.depth >= Self::MAX_DEPTH {
                        // Too deep to represent: the group is skipped whole --
                        // linearly, no recursion -- and its raw text shown, which
                        // keeps the braces balanced and the parse terminating.
                        Node::Atom(self.skip_group())
                    } else {
                        self.depth += 1;
                        let inner = Node::Row(self.list(Ctx::Group));
                        self.depth -= 1;
                        inner
                    };
                    self.push(&mut out, inner);
                }
                Some(b'\\') => {
                    if let Some(n) = self.command() {
                        self.push(&mut out, n);
                        // The second one in a group is what TeX calls a double
                        // denominator; keeping the first split is the reading that still
                        // shows the author's fraction.
                        if above.is_none() {
                            if let Some(kind) = self.infix.take() {
                                stored = Some(kind);
                                above = Some(std::mem::take(&mut out));
                            }
                        }
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
                Some(_) => {
                    // Decoded a character at a time, so a Greek letter or a Han
                    // ideograph typed straight into the formula arrives as itself
                    // rather than as its two or three UTF-8 bytes.
                    let Some(ch) = self.take_char() else {
                        // Not a UTF-8 boundary, which only malformed input can leave
                        // the reader at: step over the byte rather than abandoning the
                        // rest of the formula over it.
                        self.bump();
                        continue;
                    };
                    let text = self.letter(ch);
                    if ch.is_alphabetic() && self.welding {
                        if let Some(Node::Atom(s)) = out.last_mut() {
                            s.push_str(&text);
                            continue;
                        }
                    }
                    // A digit or an operator breaks the run: `x1y` is two identifiers,
                    // because the spacing of the three is not the same.
                    self.welding = ch.is_alphabetic();
                    self.script_chain = 0;
                    out.push(Node::Atom(text));
                }
            }
        }
        if let Some(numerator) = above {
            // The run closed: what was collected after the command is the denominator,
            // and the two become the one node the group stands for. Which of the names
            // was written decides only the rule and the pair around the stack.
            let denominator = std::mem::take(&mut out);
            let stack = Node::Frac {
                num: Box::new(Node::Row(numerator)),
                den: Box::new(Node::Row(denominator)),
                has_bar: matches!(stored, Some(Infix::Over) | Some(Infix::Above)),
                style: FracStyle::Auto,
            };
            out.push(match stored {
                Some(Infix::Choose) => fence('(', ')', stack),
                Some(Infix::Brace) => fence('{', '}', stack),
                Some(Infix::Brack) => fence('[', ']', stack),
                _ => stack,
            });
            self.script_chain = 0;

            fn fence(left: char, right: char, body: Node) -> Node {
                Node::Fence { left, right, body: Box::new(body) }
            }
        }
        if let Some(mode) = self.style.take() {
            // The switch styles the run it was read in. A group nested where it was
            // written closes first and claims it, which is the scope TeX's own gives:
            // `\(\displaystyle ... \)` never reaches out of its parentheses.
            let body = std::mem::take(&mut out);
            out.push(Node::Styled { body: Box::new(Node::Row(body)), style: mode });
            self.script_chain = 0;
        }
        // Whatever switch this run was started in is still in force for whatever is
        // written after it, which is the other half of the scope above.
        self.style = incoming;
        out
    }

    /// Append a node that came from somewhere other than the characters being read: a
    /// group, a command's text, a symbol name. None of them is ever welded to the
    /// identifier before it, which is what keeps `\pi x` two atoms with an italic `x`
    /// rather than one upright word.
    fn push(&mut self, out: &mut Vec<Node>, n: Node) {
        // Whatever lands here breaks any run of consecutive script attachments.
        self.script_chain = 0;
        if matches!(n, Node::Atom(ref s) if s.is_empty()) {
            return;
        }
        self.welding = false;
        out.push(n);
    }

    /// Attach a superscript or subscript to the node already parsed, which is the
    /// whole point: `x^2` is one atom with a script, not a row followed by a script.
    fn attach_script(&mut self, out: &mut Vec<Node>, is_sup: bool, arg: Node) {
        self.welding = false;
        self.script_chain += 1;
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
                // A script on the same slot of a script is the double-superscript
                // TeX refuses to set. Stacking them would grow one Box per `^`
                // token -- a formula of nothing but `^a^a^a...` would overflow the
                // stack unwinding the tree it made -- so both scripts stay visible
                // side by side instead of nesting.
                (is_sup, base) => {
                    // TeX nests a second script on a script, and that reading is
                    // pinned by the tests. But a formula that is nothing but `^`
                    // tokens would grow one Box per token until unwinding the
                    // tree it made overflowed the stack -- so past the same cap
                    // the groups take, the scripts sit side by side instead.
                    if self.script_chain > Self::MAX_DEPTH {
                        out.push(base);
                        out.push(Self::script(is_sup, Node::Atom(String::new()), arg));
                    } else {
                        out.push(Self::script(is_sup, base, arg));
                    }
                }
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
        if self.depth >= Self::MAX_DEPTH {
            // The cap is reached mid-command-chain (`\frac\frac\frac...` recurses
            // through here without ever entering a group). One argument-shaped
            // piece of input is consumed so the parse always moves forward, and
            // the argument itself answers with nothing.
            match self.peek() {
                Some(b'{') => {
                    self.skip_group();
                }
                Some(b'\\') => {
                    self.bump();
                    while self.peek().is_some_and(|c| c.is_ascii_alphabetic()) {
                        self.bump();
                    }
                }
                Some(_) => {
                    let before = self.at;
                    let _ = self.take_char();
                    if self.at == before {
                        self.bump();
                    }
                }
                None => {}
            }
            return Node::Atom(String::new());
        }
        self.depth += 1;
        let node = match self.peek() {
            Some(b'{') => {
                self.bump();
                Node::Row(self.list(Ctx::Group))
            }
            Some(b'\\') => self.command().unwrap_or(Node::Atom(String::new())),
            Some(b) => match self.take_char() {
                Some(c) => Node::Atom(self.letter(c)),
                None => {
                    self.bump();
                    Node::Atom(self.letter(b as char))
                }
            },
            None => Node::Atom(String::new()),
        };
        self.depth -= 1;
        node
    }

    /// Consume one balanced brace group, without recursing, and return the text
    /// between its braces. The exit a too-deep group takes instead of the stack:
    /// scanning braces is linear, and `{`, `}` and `\` are ASCII, so the byte
    /// range between them is always whole characters.
    fn skip_group(&mut self) -> String {
        let start = self.at;
        let mut open = 1usize; // the `{` the caller already consumed
        let mut end = self.src.len();
        while open > 0 {
            match self.bump() {
                Some(b'{') => open += 1,
                Some(b'}') => {
                    open -= 1;
                    if open == 0 {
                        end = self.at - 1;
                        break;
                    }
                }
                // An escaped brace is the character, not a boundary.
                Some(b'\\') => {
                    self.bump();
                }
                Some(_) => {}
                None => break,
            }
        }
        String::from_utf8_lossy(&self.src[start..end]).into_owned()
    }

    /// The body of a `\left` read whole, without recursing: everything up to the
    /// `\right` that closes it, the `}` that ends the group it stands in, or the end
    /// of the input -- whichever comes first, and none of them consumed. The exit a
    /// `\left` too deep to represent takes instead of the stack, and the same linear
    /// scan a too-deep group gets.
    fn skip_left_body(&mut self) -> String {
        let start = self.at;
        while let Some(c) = self.peek() {
            match c {
                // `\rightarrow` is a relation, not the closer this group is waiting for.
                b'\\' if self.at_right() => break,
                b'}' => break,
                b'\\' => {
                    self.bump();
                    self.skip_command_name();
                }
                _ => {
                    self.bump();
                }
            }
        }
        String::from_utf8_lossy(&self.src[start..self.at]).into_owned()
    }

    /// The rest of an environment read whole, without recursing: up to the `\end`
    /// that closes it -- the grids it contains counted, so a nested one does not close
    /// the outer one -- or the end of the input. The text between, and the name the
    /// closing `\end` carried, are what a too-deep environment is set from.
    fn skip_environment(&mut self) -> (String, Option<String>) {
        let start = self.at;
        // The grids this scan is inside of, innermost last, the first entry being
        // the environment the caller is already in. A `begin` opens one more and an
        // `end` closes one, so the `end` that empties the stack is the closer this
        // scan was reading to.
        //
        // Counting alone was not sound. A `end` naming a grid the stack does not hold
        // is a closer written for something else, and the only reading of it that keeps
        // the rest of the document parseable is to take it as this environment's own
        // closer and stop -- which is what the list of names is for.
        let mut open: Vec<String> = Vec::new();
        let mut end = None;
        while let Some(c) = self.peek() {
            match c {
                b'\\' if self.at_end() => {
                    // The grid this closer belongs to closes, whichever grid
                    // that is.
                    let name = self.eat_end();
                    // Nothing this scan opened answers the name: this is the
                    // closer the environment itself was waiting for.
                    if !open.iter().any(|g| *g == name.as_deref().unwrap_or("")) {
                        end = name;
                        break;
                    }
                    // It answers an inner grid, so the outer one stays open.
                    while open.last().is_some_and(|g| *g != name.as_deref().unwrap_or("")) {
                        open.pop();
                    }
                    open.pop();
                }
                b'\\' => {
                    // A `begin` in here is a grid of its own, so the closer this
                    // one is looking for is one further on. `beginX` is not a
                    // `begin`, and `word_at` is what tells the two apart.
                    if self.word_at(b"\\begin") {
                        self.bump();
                        self.skip_command_name();
                        open.push(self.braced_text().unwrap_or_default());
                    } else {
                        self.bump();
                        self.skip_command_name();
                    }
                }
                _ => {
                    self.bump();
                }
            }
        }
        (String::from_utf8_lossy(&self.src[start..self.at]).into_owned(), end)
    }

    /// The letters of the control sequence at the cursor, and the one space TeX
    /// consumes after it.
    fn skip_command_name(&mut self) {
        while self.peek().is_some_and(|c| c.is_ascii_alphabetic()) {
            self.bump();
        }
        if self.peek() == Some(b' ') {
            self.bump();
        }
    }

    /// The atom a character written in the source becomes.
    ///
    /// A letter is set in whatever alphabet the parser is inside of, which by default
    /// is math italic: that is how a `MATH` face is drawn to be read, and spelling the
    /// lettering as a codepoint rather than as a slant is what gives the glyph its own
    /// italics correction from the table. Everything that is not an ASCII letter is
    /// returned as written -- a hand-typed `α` is already the letter its author meant,
    /// a Han ideograph has no italic form to reach, and a digit is upright in TeX too.
    fn letter(&self, ch: char) -> String {
        if !ch.is_ascii_alphabetic() {
            return ch.to_string();
        }
        alphabetize(self.alphabet.unwrap_or(Alphabet::Italic), &ch.to_string())
    }

    /// A `\name` control sequence, or a single escaped character like `\,`.
    fn command(&mut self) -> Option<Node> {
        self.bump(); // the backslash
        let first = self.peek()?;
        if !first.is_ascii_alphabetic() {
            if first >= 0x80 {
                // A multibyte character written straight after the backslash, read
                // whole: bumping one byte of it landed the reader inside the glyph,
                // where the next read failed to decode and the rest of the formula
                // was silently dropped.
                return Some(Node::Atom(match self.take_char() {
                    Some(c) => c.to_string(),
                    // Not a UTF-8 boundary, which only malformed input can leave the
                    // reader at: step over the byte so the parse still moves forward.
                    None => {
                        self.bump();
                        String::new()
                    }
                }));
            }
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
                b'|' => Node::Atom("\u{2016}".into()),
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
            "frac" => Node::Frac {
                num: Box::new(self.argument()),
                den: Box::new(self.argument()),
                has_bar: true,
                style: FracStyle::Auto,
            },
            // The four style switches take no argument: they ask how the rest of the
            // group is to be set, and the run wraps itself when it closes.
            "displaystyle" => self.style_switch(MathStyle::Display),
            "textstyle" => self.style_switch(MathStyle::Text),
            "scriptstyle" => self.style_switch(MathStyle::Script),
            "scriptscriptstyle" => self.style_switch(MathStyle::ScriptScript),
            // A rule across the grid, recorded for the environment reader to place at the
            // boundary it was written at; nothing is emitted here, because `hline` as a
            // word in the middle of a cell is the bug being fixed.
            "hline" => {
                self.hline = true;
                Node::Atom(String::new())
            }
            // The infix pair, which is the one place in this subset where a command does
            // not stand in front of its arguments: TeX takes the whole run before it as
            // the numerator and the whole run after it as the denominator, so `list` is
            // what closes the split when the group ends.
            "over" => {
                self.infix = Some(Infix::Over);
                Node::Atom(String::new())
            }
            "atop" => {
                self.infix = Some(Infix::Atop);
                Node::Atom(String::new())
            }
            "brace" => {
                self.infix = Some(Infix::Brace);
                Node::Atom(String::new())
            }
            "brack" => {
                self.infix = Some(Infix::Brack);
                Node::Atom(String::new())
            }
            "above" => {
                self.infix = Some(Infix::Above);
                self.skip_dimension();
                Node::Atom(String::new())
            }
            "choose" => {
                self.infix = Some(Infix::Choose);
                Node::Atom(String::new())
            }
            "dfrac" | "tfrac" => {
                let a = self.argument();
                let b = self.argument();
                Node::Frac {
                    num: Box::new(a),
                    den: Box::new(b),
                    has_bar: true,
                    style: if name == "dfrac" { FracStyle::Display } else { FracStyle::Text },
                }
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
                        style: FracStyle::Auto,
                    }),
                }
            }
            "sqrt" => {
                let mut degree = None;
                if self.peek() == Some(b'[') {
                    // A `[` of the degree's own is a bracket, and a degree with no
                    // closer is not a degree at all: the reader is put back just after
                    // the `[`, so what follows is read as the body rather than eaten
                    // as the degree.
                    let open = self.at;
                    self.bump();
                    let mut inner: Vec<u8> = Vec::new();
                    let mut depth = 1usize;
                    while let Some(c) = self.bump() {
                        match c {
                            b'[' => depth += 1,
                            b']' => {
                                depth -= 1;
                                if depth == 0 {
                                    break;
                                }
                            }
                            _ => {}
                        }
                        inner.push(c);
                    }
                    if depth == 0 {
                        degree = Some(String::from_utf8_lossy(&inner).to_string());
                    } else {
                        self.at = open + 1;
                    }
                }
                let body = self.argument();
                Node::Sqrt {
                    body: Box::new(body),
                    degree: degree
                        .filter(|d| !d.is_empty())
                        .map(|d| Box::new(Node::Atom(d))),
                }
            }
            "left" => {
                // A delimiter name this subset has no character for is shown as the
                // word that was written, beside the fence rather than inside it --
                // eating it is the one reading that helps nobody.
                let mut written: Vec<Node> = Vec::new();
                let l = match self.delim() {
                    Ok(c) => c,
                    Err(name) => {
                        written.push(Node::Atom(format!("\\{name}")));
                        '\0'
                    }
                };
                let body = if self.depth >= Self::MAX_DEPTH {
                    // Too deep to represent: the body is read whole -- linearly, no
                    // recursion -- and shown as the text it was written as, which is
                    // the same degraded reading a too-deep group gets.
                    Node::Atom(self.skip_left_body())
                } else {
                    self.depth += 1;
                    let body = Node::Row(self.list(Ctx::Group));
                    self.depth -= 1;
                    body
                };
                let r = if self.at_right() {
                    self.bump(); // the backslash of `\right`
                    while self.peek().is_some_and(|c| c.is_ascii_alphabetic()) {
                        self.bump();
                    }
                    match self.delim() {
                        Ok(c) => c,
                        Err(name) => {
                            written.push(Node::Atom(format!("\\{name}")));
                            '\0'
                        }
                    }
                } else {
                    // No closer written: the group still has to end somewhere.
                    '\0'
                };
                let fence = Node::Fence { left: l, right: r, body: Box::new(body) };
                if written.is_empty() {
                    fence
                } else {
                    written.push(fence);
                    Node::Row(written)
                }
            }
            "right" => {
                // A `.` -- or nothing at all, when the `\right` ends the input --
                // means "no delimiter here": an empty atom, which `push` drops,
                // rather than a NUL the shaper would carry to the page as tofu.
                match self.delim() {
                    Ok('\0') => Node::Atom(String::new()),
                    Ok(c) => Node::Atom(c.to_string()),
                    Err(name) => Node::Atom(format!("\\{name}")),
                }
            }
            // Upright words: read as written, spaces and all, because a formula's
            // `\text{as } x` loses a word when the space is treated as a separator.
            "text" | "textrm" | "mbox" => Node::Atom(self.text_argument()),
            "mathrm" | "operatorname" => Node::Atom(self.text_argument()),
            // Invisible in TeX, and the worst thing a reader can do with them is print
            // them: `\label{eq:one}` at the end of every displayed formula would put
            // `label(𝑒𝑞:𝑜𝑛𝑒)` on the page, which is why emitting nothing is the fix
            // rather than a fallback. `\input`-like bookkeeping commands belong here too.
            "label" | "notag" | "nonumber" | "ignorespaces" => {
                // Read the argument and drop it: leaving it on the stream would put the
                // name of the label on the page as a group of atoms.
                if name == "label" {
                    let _ = self.text_argument();
                }
                Node::Atom(String::new())
            }
            // A numbered display's number. Reaching the right margin is the reader's
            // layout, not this engine's, so it is set one em after the formula in the
            // author's own upright text rather than as letters of the alphabet.
            "tag" => Node::Row(vec![
                Node::Space(18),
                Node::Atom(format!("({})", self.text_argument())),
            ]),
            // `\pmod` and `\bmod` spell a word, and a word in a formula is upright: both
            // are the roman `(mod …)` of number theory, not the product of the letters
            // `m`, `o`, `d`.
            "pmod" => Node::Row(vec![
                Node::Space(5),
                Node::Atom("(mod".into()),
                Node::Space(3),
                self.argument(),
                Node::Space(3),
                Node::Atom(")".into()),
            ]),
            "bmod" | "mod" => Node::Row(vec![
                Node::Space(4),
                Node::Atom("mod".into()),
                Node::Space(4),
            ]),
            // The tombstone that closes a proof.
            "qed" | "qedsymbol" | "tombstone" => Node::Atom("\u{220e}".into()),
            "textbf" => Node::Atom(alphabetize(Alphabet::Bold, &self.text_argument())),
            "textit" => Node::Atom(alphabetize(Alphabet::Italic, &self.text_argument())),
            "textsf" => Node::Atom(alphabetize(Alphabet::Sans, &self.text_argument())),
            "texttt" => Node::Atom(alphabetize(Alphabet::Monospace, &self.text_argument())),
            // A change of alphabet in math: the argument's letters become the codepoints
            // of that alphabet, which is how TeX itself spells it. See [`Alphabet`].
            "mathbb" => self.alphabetized(Alphabet::DoubleStruck),
            "mathbf" | "bf" => self.alphabetized(Alphabet::Bold),
            "mathit" | "mathnormal" | "it" => self.alphabetized(Alphabet::Italic),
            "boldsymbol" => self.alphabetized(Alphabet::BoldItalic),
            "mathsf" | "sf" => self.alphabetized(Alphabet::Sans),
            "mathtt" | "tt" => self.alphabetized(Alphabet::Monospace),
            "mathfrak" | "frak" => self.alphabetized(Alphabet::Fraktur),
            "mathcal" | "cal" => self.alphabetized(Alphabet::Script),
            "mathscr" => self.alphabetized(Alphabet::Script),
            "overline" => Node::Bar { body: Box::new(self.argument()), side: BarSide::Over },
            "underline" => Node::Bar { body: Box::new(self.argument()), side: BarSide::Under },
            "overbrace" => Node::Brace { body: Box::new(self.argument()), side: BarSide::Over },
            "underbrace" =>
                Node::Brace { body: Box::new(self.argument()), side: BarSide::Under },
            // The narrow and the wide spelling of the same mark. Which glyph to draw is
            // not the difference -- `\widehat` is `\hat` taken from the face's wider
            // drawings -- so the choice is carried to the layout as a licence to
            // stretch rather than as a fifth kind of accent. `\hat{xy}` is a small hat
            // sitting on a wide base because that is what its author asked for.
            "hat" => self.accent(AccentKind::Hat, false),
            "widehat" => self.accent(AccentKind::Hat, true),
            "bar" | "overline_" => self.accent(AccentKind::Bar, false),
            "tilde" => self.accent(AccentKind::Tilde, false),
            "widetilde" => self.accent(AccentKind::Tilde, true),
            "dot" => self.accent(AccentKind::Dot, false),
            // The rest of the marks over a letter. Each is one glyph of the face, not a
            // pair of the ones above it: `\ddot{x}` is one mark of two dots, and two
            // `\dot`s stacked would sit at two different heights.
            "ddot" | "dotdot" => self.accent(AccentKind::Ddot, false),
            "check" => self.accent(AccentKind::Check, false),
            "breve" => self.accent(AccentKind::Breve, false),
            "acute" => self.accent(AccentKind::Acute, false),
            "grave" => self.accent(AccentKind::Grave, false),
            "mathring" => self.accent(AccentKind::Mathring, false),
            "vec" => self.accent(AccentKind::Vec, false),
            // The named arrows. `\overrightarrow{AB}` is the vector mark a course actually
            // writes, and it is the wide form: an arrow over two letters has to reach
            // both of them or it points at the space between.
            "overrightarrow" => self.accent(AccentKind::Vec, true),
            "overleftarrow" => self.accent(AccentKind::Backvec, true),
            // A label over or under a base. `\stackrel` is the older spelling of
            // `\overset` and both are a forced `\limits` on their base, so they get the
            // same box here; which side the label goes is the only difference that shows.
            "overset" | "stackrel" => self.stack(BarSide::Over),
            "underset" => self.stack(BarSide::Under),
            "boxed" | "fbox" => Node::Boxed { body: Box::new(self.argument()) },
            // The one grid written as an argument rather than as an environment:
            // `\sum_{\substack{i<j\\k\neq l}}` puts two lines under an operator's limit,
            // which is how a condition that will not fit on one line is set. `env_rows`
            // stops at the closing brace on its own, having eaten it.
            "substack" if self.peek() == Some(b'{') => {
                self.bump();
                if self.depth >= Self::MAX_DEPTH {
                    // Too deep to represent: the rows are read whole, linearly, and
                    // shown as the text they were written as.
                    Node::Atom(self.skip_group())
                } else {
                    self.depth += 1;
                    // A grid nested in a limit is its own grid: its boundaries start
                    // empty, and a `\hline` written here belongs to the grid this one
                    // is inside rather than to this one.
                    self.hline = false;
                    let saved = std::mem::take(&mut self.row_rules);
                    let (rows, _) = self.env_rows();
                    let mine = std::mem::replace(&mut self.row_rules, saved);
                    let cols = rows.iter().map(|r| r.len()).max().unwrap_or(0).max(1);
                    let rows = rows.into_iter().map(|r| r.into_iter().map(Node::Row).collect()).collect();
                    self.depth -= 1;
                    Node::Array {
                        rows,
                        columns: columns_for(ArrayKind::Gathered, &[], cols),
                        kind: ArrayKind::Gathered,
                        delimiters: None,
                        rules: mine,
                        col_rules: Vec::new(),
                    }
                }
            }
            // A delimiter sized by hand rather than by what it encloses. The four
            // heights are the only difference between the twelve names; which side of a
            // pair each one is written on decides the spacing the layout keeps around
            // it, and that is the role the name carries.
            "big" => self.big(1, BigRole::Ord),
            "bigl" => self.big(1, BigRole::Open),
            "bigr" => self.big(1, BigRole::Close),
            "bigm" => self.big(1, BigRole::Rel),
            "Big" => self.big(2, BigRole::Ord),
            "Bigl" => self.big(2, BigRole::Open),
            "Bigr" => self.big(2, BigRole::Close),
            "Bigm" => self.big(2, BigRole::Rel),
            "bigg" => self.big(3, BigRole::Ord),
            "biggl" => self.big(3, BigRole::Open),
            "biggr" => self.big(3, BigRole::Close),
            "biggm" => self.big(3, BigRole::Rel),
            "Bigg" => self.big(4, BigRole::Ord),
            "Biggl" => self.big(4, BigRole::Open),
            "Biggr" => self.big(4, BigRole::Close),
            "Biggm" => self.big(4, BigRole::Rel),
            // A modifier on the preceding big operator, which the layout reads off
            // the node itself; emitting nothing keeps `a \lim\limits b` working.
            "limits" | "nolimits" => Node::Atom(String::new()),
            _ => self.symbol_node(name),
        }
    }

    fn accent(&mut self, kind: AccentKind, wide: bool) -> Node {
        Node::Accent { base: Box::new(self.argument()), accent: kind, wide }
    }

    /// A stacked label's two arguments, in the order they are written: the label comes
    /// first and the base second, which is the one place in this subset where the
    /// reading order and the layout's names for the two boxes run against each other.
    fn stack(&mut self, side: BarSide) -> Node {
        let label = self.argument();
        let base = self.argument();
        Node::Stack { base: Box::new(base), label: Box::new(label), side }
    }

    /// A style switch, which emits nothing itself: the run being collected wraps itself
    /// in [`Node::Styled`] when it closes. See [`MathStyle`].
    fn style_switch(&mut self, mode: MathStyle) -> Node {
        self.style = Some(mode);
        Node::Atom(String::new())
    }

    /// The argument of an alphabet-switching command, read *inside* that alphabet:
    /// every written letter in it is retargeted as it is collected, which is why
    /// `\mathbb{R^n}` doubles the `n` as well while `\mathbb{R}^n` leaves that `n` in
    /// the alphabet the formula itself is set in. A nested switch keeps the inner one,
    /// because the inner is the parser still reading when the letter arrives.
    fn alphabetized(&mut self, alphabet: Alphabet) -> Node {
        let outer = self.alphabet;
        self.alphabet = Some(alphabet);
        let n = self.argument();
        self.alphabet = outer;
        n
    }

    /// A brace group read as *words* rather than as tokens: its spaces are the author's
    /// and not separators, so `\text{as } x` keeps the gap the sentence needs. Only the
    /// escapes that stand for a character are resolved; braces around an inner group
    /// vanish and everything else is kept exactly as written.
    fn text_argument(&mut self) -> String {
        if self.peek() != Some(b'{') {
            return self.take_char().map(|c| c.to_string()).unwrap_or_default();
        }
        self.at += 1;
        let mut out = String::new();
        let mut depth = 1usize;
        while depth > 0 && self.at < self.src.len() {
            match self.src[self.at] {
                b'{' => {
                    depth += 1;
                    self.at += 1;
                }
                b'}' => {
                    depth -= 1;
                    self.at += 1;
                }
                b'\\' => {
                    self.at += 1;
                    match self.take_char() {
                        Some(c) if !c.is_ascii_alphabetic() => out.push(c),
                        Some(c) => {
                            out.push('\\');
                            out.push(c);
                        }
                        None => {}
                    }
                }
                _ => match self.take_char() {
                    Some(c) => out.push(c),
                    // Not a UTF-8 boundary, which only a malformed input can leave the
                    // reader at: keep the byte rather than spin on it.
                    None => {
                        out.push(self.src[self.at] as char);
                        self.at += 1;
                    }
                },
            }
        }
        out
    }

    /// The next character, whole: a multibyte one arrives as the letter it is rather
    /// than as the bytes of its UTF-8 encoding.
    fn take_char(&mut self) -> Option<char> {
        let c = std::str::from_utf8(self.rest()).ok()?.chars().next()?;
        self.at += c.len_utf8();
        Some(c)
    }

    /// `\begin{X} ... \end{X}`: a grid of cells. The name decides the kind, and with
    /// it the alignments and the delimiters the environment brings.
    ///
    /// Three failures are handled the same way -- an environment whose name is not one
    /// of ours, one that is never closed, and one closed by an `\end` naming something
    /// else -- by setting the cells that were read inline beside the literal marker
    /// text. A reader loses the arrangement, not the mathematics.
    fn environment(&mut self) -> Node {
        // A `\hline` written *before* a `\begin` belongs to no environment -- TeX
        // calls it misplaced -- and leaving it pending would let the next one claim
        // it and draw a rule its author never wrote.
        self.hline = false;
        let Some(name) = self.braced_text() else {
            // `\begin` with nothing after it: the word itself, and no more eaten.
            return Node::Atom("\\begin".into());
        };
        if self.depth >= Self::MAX_DEPTH {
            // Too deep to represent: the body is read whole -- linearly, no
            // recursion -- and set as the text it was written as, which is the same
            // degraded reading an unclosable environment already gets.
            let (body, end) = self.skip_environment();
            let rows = vec![vec![vec![Node::Atom(body)]]];
            return Self::degraded(&name, rows, end.as_deref());
        }
        self.depth += 1;
        let node = self.grid(&name);
        self.depth -= 1;
        node
    }

    /// The grid a named environment becomes, its body read one level down. A nesting
    /// of environments is a nesting of the whole grammar -- each grid reads its cells
    /// through `list`, which reads them through `command` -- so it is the same cap the
    /// brace groups take.
    fn grid(&mut self, name: &str) -> Node {
        // The boundaries belong to the grid being read, and a grid nested in one of
        // its cells writes its own: one buffer cleared on entry wiped the outer
        // grid's rules, and on the degraded paths left the inner one's behind for the
        // outer grid to adopt.
        let saved = std::mem::take(&mut self.row_rules);
        let Some(kind) = array_kind(name) else {
            let (rows, end) = self.env_rows();
            self.row_rules = saved;
            return Self::degraded(name, rows, end.as_deref());
        };
        // `array` is the one environment whose columns are written out, and the
        // argument has to be taken here or its braces land in the first cell.
        let (spec, col_rules) =
            if kind == ArrayKind::Array { self.column_spec() } else { (Vec::new(), Vec::new()) };
        let (rows, end) = self.env_rows();
        let mine = std::mem::replace(&mut self.row_rules, saved);
        if end.as_deref() != Some(name) {
            return Self::degraded(name, rows, end.as_deref());
        }
        let cols = rows.iter().map(|r| r.len()).max().unwrap_or(0).max(1);
        let columns = columns_for(kind, &spec, cols);
        let rows = rows.into_iter().map(|r| r.into_iter().map(Node::Row).collect()).collect();
        Node::Array {
            rows,
            columns,
            kind,
            delimiters: env_delimiters(name),
            rules: mine,
            col_rules,
        }
    }

    /// Consume a TeX dimension -- `4pt`, `1.5em`, `\mu 3` -- leaving nothing of it in
    /// the cell after it. Only `\above` uses this: how far a fraction's halves sit
    /// apart is read from the face's `MATH` table here, so the author's number is
    /// honoured by dropping it rather than by a second, competing source of truth.
    fn skip_dimension(&mut self) {
        while self.peek() == Some(b' ') {
            self.bump();
        }
        while self
            .peek()
            .is_some_and(|c| c.is_ascii_digit() || c == b'.' || c.is_ascii_alphabetic() || c == b'\\')
        {
            self.bump();
        }
    }

    /// The cells of an environment's body, split on `&` and `\\`, and the name its
    /// `\end` carried if it reached one. Whatever closed the run -- a matching `\end`,
    /// a mismatched one, a stray `}`, a `\right` belonging to an enclosing group or the
    /// end of the input -- the search stops there rather than eating what follows.
    fn env_rows(&mut self) -> (Vec<Vec<Vec<Node>>>, Option<String>) {
        let mut rows: Vec<Vec<Vec<Node>>> = Vec::new();
        let mut row: Vec<Vec<Node>> = Vec::new();
        let mut end = None;
        // One entry per boundary: before each row, and one after the last. Written here
        // rather than returned because the rules belong to the grid the rows become, and
        // both of this file's grid builders read the body through this one function.
        self.row_rules.clear();
        let mut rule = false;
        loop {
            let cell = self.list(Ctx::Cell);
            // Claimed even when the cell turns out to be empty: an `\hline` on a line of
            // its own is a rule at this boundary, not a row of nothing.
            rule |= self.hline;
            self.hline = false;
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
                    self.row_rules.push(rule);
                    rule = false;
                }
                continue;
            }
            if !blank {
                row.push(cell);
                rows.push(std::mem::take(&mut row));
                self.row_rules.push(rule);
                rule = false;
            }
            if self.at_end() {
                end = self.eat_end();
            }
            self.row_rules.push(rule);
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
        // A name is a word, not a span of the page: `\begin {matrix}` works, so
        // `\begin{ matrix }` has to as well -- `eat_end` already skips the space
        // before its braces, and this is the other half of that.
        Some(s.trim().to_string())
    }

    /// The `{ccc}` / `{l|l}` argument an `array` takes. Only `l`, `c` and `r` choose an
    /// alignment; the vertical rules and anything else are consumed and dropped, since
    /// a rule between columns is not something the layout draws yet.
    fn column_spec(&mut self) -> (Vec<ColAlign>, Vec<bool>) {
        let Some(spec) = self.braced_text() else {
            return (Vec::new(), Vec::new());
        };
        let mut cols: Vec<ColAlign> = Vec::new();
        let mut rules: Vec<bool> = Vec::new();
        // A `|` is read as a rule at the boundary it sits on rather than as a column of
        // its own, which is what let it be dropped before: `{l|r}` asked for a line
        // between two columns, and silence was the only wrong answer left.
        let mut pending = false;
        for c in spec.chars() {
            match c {
                'l' | 'c' | 'r' => {
                    rules.push(pending);
                    pending = false;
                    cols.push(match c {
                        'l' => ColAlign::Left,
                        'r' => ColAlign::Right,
                        _ => ColAlign::Center,
                    });
                }
                '|' => pending = true,
                _ => {}
            }
        }
        rules.push(pending);
        (cols, rules)
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

    /// The one token after a size command: a delimiter character, `\{`, `\langle`, or
    /// the `.` that means "nothing here" -- which `delim` already reads for `\left`, and
    /// is read the same way here so the two spellings cannot drift apart.
    fn big(&mut self, step: u8, role: BigRole) -> Node {
        match self.delim() {
            Ok(delim) => Node::Big { delim, step, role },
            // A name that is not one of the delimiter spellings is shown as it was
            // written, the way any other unknown word degrades.
            Err(name) => Node::Atom(format!("\\{name}")),
        }
    }

    /// The one delimiter here, or the name of one this subset does not spell -- which
    /// has been consumed, and which the caller shows rather than drops. `Ok('\0')` is
    /// the deliberate "no delimiter here" of `\left.` and of the end of the input.
    fn delim(&mut self) -> Result<char, String> {
        match self.peek() {
            Some(b'.') => {
                self.bump();
                Ok('\0')
            }
            Some(b'\\') => {
                self.bump();
                self.command_delim()
            }
            // A whole character, not one byte: `\left（` would otherwise land the
            // cursor inside the three-byte glyph and drop the rest of the formula
            // when the next read fails to decode from mid-character.
            Some(_) => match self.take_char() {
                Some(c) => Ok(c),
                None => {
                    self.bump();
                    Ok('\0')
                }
            },
            None => Ok('\0'),
        }
    }

    fn command_delim(&mut self) -> Result<char, String> {
        // `\{`, `\}` and `\|` are the shorthand spellings of `lbrace`, `rbrace` and
        // `Vert`. They are single escaped characters, not words -- the word scanner
        // below collects only letters, so without these arms the escaped byte is
        // left unconsumed: `\left\{x\right\}` would lose both delimiters and read
        // the `{` as the start of a group the `\right` never closes.
        match self.peek() {
            Some(b'{') => {
                self.bump();
                Ok('{')
            }
            Some(b'}') => {
                self.bump();
                Ok('}')
            }
            Some(b'|') => {
                self.bump();
                Ok('\u{2016}')
            }
            _ => {
                let start = self.at;
                while self.peek().is_some_and(|c| c.is_ascii_alphabetic()) {
                    self.bump();
                }
                let name = String::from_utf8_lossy(&self.src[start..self.at]).into_owned();
                match &self.src[start..self.at] {
                    b"langle" => Ok('\u{27e8}'),
                    b"rangle" => Ok('\u{27e9}'),
                    b"lbrace" => Ok('{'),
                    b"rbrace" => Ok('}'),
                    b"lbrack" => Ok('['),
                    b"rbrack" => Ok(']'),
                    b"vert" | b"lvert" | b"rvert" | b"mid" => Ok('|'),
                    b"Vert" | b"lVert" | b"rVert" => Ok('\u{2016}'),
                    // Not a delimiter name: the caller shows the word rather than
                    // losing the delimiter's own text.
                    _ => Err(name),
                }
            }
        }
    }
}

/// A lettering style in math, spelled the way TeX and Unicode spell it: as a different
/// codepoint rather than as a different face. That is what lets a blackboard-bold `ℝ`
/// travel through the layout as one ordinary atom -- measured by the same shaper, drawn
/// by the same run path -- with the face itself chosen by the platform's font fallback.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Alphabet {
    Bold,
    Italic,
    BoldItalic,
    Sans,
    SansBold,
    /// `\mathsf{\mathit{...}}`, which no command of ours reaches on its own.
    Fraktur,
    BoldFraktur,
    /// `\mathcal`, and `\mathscr` as well: a script face is one alphabet to a font,
    /// and the two names differ only in which designer's curves they ask for.
    Script,
    BoldScript,
    /// `\mathbb`: the double-struck numerals `\N \Z \Q \R \C`.
    DoubleStruck,
    Monospace,
}

impl Alphabet {
    /// The codepoint each block starts at, for `A`, `a` and `0`. `None` for the digits
    /// means the alphabet has no numerals encoded, which is most of them.
    fn bases(self) -> (u32, u32, Option<u32>) {
        use Alphabet::*;
        match self {
            Bold => (0x1D400, 0x1D41A, Some(0x1D7CE)),
            Italic => (0x1D434, 0x1D44E, None),
            BoldItalic => (0x1D468, 0x1D482, None),
            Sans => (0x1D5A0, 0x1D5BA, Some(0x1D7E2)),
            SansBold => (0x1D5D4, 0x1D5EE, Some(0x1D7EC)),
            Fraktur => (0x1D504, 0x1D51E, None),
            BoldFraktur => (0x1D56C, 0x1D586, None),
            Script => (0x1D49C, 0x1D4B6, None),
            BoldScript => (0x1D4D0, 0x1D4EA, None),
            DoubleStruck => (0x1D538, 0x1D552, Some(0x1D7D8)),
            Monospace => (0x1D670, 0x1D68A, Some(0x1D7F6)),
        }
    }

    /// A letter encoded outside its block because it predates it: `\R` is `ℝ` at
    /// U+211D, not the twenty-third slot of the double-struck row.
    fn exception(self, c: char) -> Option<char> {
        use Alphabet::*;
        let cp = match (self, c) {
            (Italic, 'h') => '\u{210E}',
            (DoubleStruck, 'C') => '\u{2102}',
            (DoubleStruck, 'H') => '\u{210D}',
            (DoubleStruck, 'N') => '\u{2115}',
            (DoubleStruck, 'P') => '\u{2119}',
            (DoubleStruck, 'Q') => '\u{211A}',
            (DoubleStruck, 'R') => '\u{211D}',
            (DoubleStruck, 'Z') => '\u{2124}',
            (Fraktur, 'C') => '\u{212D}',
            (Fraktur, 'H') => '\u{210C}',
            (Fraktur, 'I') => '\u{2110}',
            (Fraktur, 'R') => '\u{211C}',
            (Fraktur, 'Z') => '\u{2128}',
            (Script, 'B') => '\u{212C}',
            (Script, 'E') => '\u{2130}',
            (Script, 'F') => '\u{2131}',
            (Script, 'H') => '\u{210B}',
            (Script, 'I') => '\u{2110}',
            (Script, 'L') => '\u{2112}',
            (Script, 'M') => '\u{2133}',
            (Script, 'R') => '\u{211B}',
            (Script, 'e') => '\u{212F}',
            (Script, 'i') => '\u{210A}',
            _ => return None,
        };
        Some(cp)
    }

    /// One character in this alphabet, or `None` when it has no lettering of its own and
    /// stays as written -- punctuation, operators, and a letter already spent on the
    /// alphabet of an inner switch.
    fn of(self, c: char) -> Option<char> {
        if let Some(e) = self.exception(c) {
            return Some(e);
        }
        let (cap, lower, digits) = self.bases();
        let (base, value) = match c {
            'A'..='Z' => (cap, c as u32 - 'A' as u32),
            'a'..='z' => (lower, c as u32 - 'a' as u32),
            '0'..='9' => (digits?, c as u32 - '0' as u32),
            _ => return None,
        };
        char::from_u32(base + value)
    }
}

/// Retarget every letter in `s`, leaving anything else alone.
pub fn alphabetize(alphabet: Alphabet, s: &str) -> String {
    s.chars().map(|c| alphabet.of(c).unwrap_or(c)).collect()
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
        // The first group takes its limits above and below: TeX declares each as a
        // `\mathop` with `\limits`, and `\limsup`/`\liminf` belong with `\lim` rather
        // than with `\sin`, because a limit is written under them.
        "lim" => "lim",
        "limsup" => "limsup",
        "liminf" => "liminf",
        "max" => "max",
        "min" => "min",
        "sup" => "sup",
        "inf" => "inf",
        "gcd" => "gcd",
        // The names set upright with a thin space after them. Left out of this list a
        // function is an ordinary atom, and TeX's room between an operator and its
        // argument goes missing: `\log n` sets as `logn`.
        "sin" => "sin",
        "cos" => "cos",
        "tan" => "tan",
        "cot" => "cot",
        "sec" => "sec",
        "csc" => "csc",
        "sinh" => "sinh",
        "cosh" => "cosh",
        "tanh" => "tanh",
        "coth" => "coth",
        "arcsin" => "arcsin",
        "arccos" => "arccos",
        "arctan" => "arctan",
        "log" => "log",
        "ln" => "ln",
        "lg" => "lg",
        "exp" => "exp",
        "sgn" => "sgn",
        "det" => "det",
        "arg" => "arg",
        "deg" => "deg",
        "dim" => "dim",
        "ker" => "ker",
        "Pr" => "Pr",
        "cdots" => "⋯",
        "ldots" => "…",
        "dots" => "…",
        "ddots" => "⋱",
        "vdots" => "⋮",
        "colon" => ":",
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
        // The `\left`-only spellings are also written on their own -- `\lVert x \rVert`
        // for a norm -- where they reach the symbol table instead of a delimiter pair.
        "vert" | "lvert" | "rvert" => "|",
        "Vert" | "lVert" | "rVert" => "\u{2016}",
        "surd" => "√",
        "checkmark" => "✓",
        // The names a logic, lattice or type-theory text is written with: each of these
        // was a word spelled out in variables on the page, because an unknown command
        // falls back to its own letters.
        // `amsmath`'s dotted ellipses, which differ only in what stands around them and
        // so differ in spacing: the binary and the integral forms are the centred row of
        // dots, the ordinary one sits on the baseline.
        "dotsb" | "dotsm" | "dotsi" => "\u{22ef}",
        "dotso" => "…",
        "top" => "\u{22a4}",
        "bot" => "\u{22a5}",
        "vdash" => "\u{22a2}",
        "dashv" => "\u{22a3}",
        "models" => "\u{22a8}",
        "doteq" => "\u{2250}",
        "ominus" => "\u{2296}",
        "oslash" => "\u{2298}",
        "odot" => "\u{2299}",
        "triangle" => "\u{25b7}",
        "triangledown" => "\u{25bd}",
        "square" => "\u{25a1}",
        "Diamond" => "\u{25c7}",
        "uparrow" => "\u{2191}",
        "downarrow" => "\u{2193}",
        "updownarrow" => "\u{2195}",
        "Uparrow" => "\u{21d1}",
        "Downarrow" => "\u{21d3}",
        "longrightarrow" => "\u{27f6}",
        "longleftarrow" => "\u{27f5}",
        "longleftrightarrow" => "\u{27f7}",
        "Longrightarrow" => "\u{27f9}",
        "Longleftarrow" => "\u{27f8}",
        "Longleftrightarrow" => "\u{27fa}",
        "bigwedge" => "\u{22c0}",
        "bigvee" => "\u{22c1}",
        "bigsqcup" => "\u{2a06}",
        "bigoplus" => "\u{2a01}",
        "bigotimes" => "\u{2a02}",
        "bigodot" => "\u{2a00}",
        "biguplus" => "\u{2a04}",
        "Angstrom" | "angstrom" => "\u{212b}",
        // The rest of the arrow and order names a proof is written with, and the two
        // reduced-planck forms -- `\hbar` is the one symbol in this table that Unicode
        // spells twice, and the hand-written ℏ is the one a face draws narrow.
        "mho" => "\u{2127}",
        "hslash" => "\u{210f}",
        "smallsetminus" => "\u{2216}",
        "measuredangle" => "\u{221f}",
        "sphericalangle" => "\u{2221}",
        "proportionality" => "\u{221d}",
        "twoheadrightarrow" => "\u{21a0}",
        "twoheadleftarrow" => "\u{219e}",
        "hookleftarrow" => "\u{21a9}",
        "multimap" => "\u{22b8}",
        "longmapsto" => "\u{27fc}",
        "preccurlyeq" => "\u{2ab0}",
        "succcurlyeq" => "\u{2ab1}",
        "trianglelefteq" => "\u{22b4}",
        "trianglerighteq" => "\u{22b5}",
        "bigtriangleup" => "\u{25b3}",
        "bigtriangledown" => "\u{25bd}",
        // A function named by a word rather than a letter: upright, with the room of an
        // operator, exactly as `\log` and `\max` are set.
        "hom" => "hom",
        "rank" => "rank",
        "argmax" => "argmax",
        "argmin" => "argmin",
        "col" => "col",
        "coker" => "coker",
        "tr" => "tr",
        "triangleleft" | "vartriangleleft" => "\u{22b2}",
        "triangleright" | "vartriangleright" => "\u{22b3}",
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
        "lim" | "limsup" | "liminf" | "max" | "min" | "sup" | "inf" | "gcd" => Limits::Always,
        // The named functions: an operator for spacing purposes, so the argument that
        // follows clears a thin space, but one whose limits would be a subscript
        // written beside it rather than a limit taken under the name.
        "sin" | "cos" | "tan" | "cot" | "sec" | "csc" | "sinh" | "cosh" | "tanh" | "coth"
        | "arcsin" | "arccos" | "arctan" | "log" | "ln" | "lg" | "exp" | "sgn" | "det"
        | "arg" | "deg" | "dim" | "ker" | "Pr" | "hom" | "rank" | "argmax" | "argmin"
        | "col" | "coker" | "tr" => Limits::Never,
        "sum" | "prod" | "coprod" | "bigcup" | "bigcap" | "bigvee" | "bigwedge"
        | "bigsqcup" | "bigoplus" | "bigotimes" | "bigodot" | "biguplus" => Limits::Default,
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
            Node::Frac { num, den, has_bar, style } => format!(
                "({} {} {})",
                match (*has_bar, *style) {
                    (true, FracStyle::Auto) => "frac",
                    (true, FracStyle::Display) => "dfrac",
                    (true, FracStyle::Text) => "tfrac",
                    (false, _) => "nob",
                },
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
            Node::Accent { base, accent, wide } => format!(
                "(accent {accent:?}{} {})",
                if *wide { " wide" } else { "" },
                sexp(base)
            ),
            Node::Bar { body, side } => format!("(bar {side:?} {})", sexp(body)),
            Node::Brace { body, side } => format!("(brace {side:?} {})", sexp(body)),
            Node::Big { delim, step, role } => format!("(big {step} {role:?} {delim:?})"),
            Node::Stack { base, label, side } =>
                format!("(stack {side:?} {} {})", sexp(base), sexp(label)),
            Node::Boxed { body } => format!("(boxed {})", sexp(body)),
            Node::Styled { body, style } => format!("(style {style:?} {})", sexp(body)),
            Node::Space(mu) => format!("(space {mu})"),
            // Columns print as the `l`/`c`/`r` letters they were written with, rows
            // separated by ` / ` and cells by ` & `, which is the shape of the source.
            // The rules are left out of the sketch -- they are asserted where they are
            // the thing under test, on the node itself.
            Node::Array { rows, columns, kind, delimiters, .. } => format!(
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

    /// The tree as a compact string, in the letters the source spelled them with: a
    /// written identifier reaches a node as math italic, and an expectation about which
    /// brace became which box should not have to be written in codepoints. The lettering
    /// itself is what [`a_written_identifier_is_italic_and_a_written_name_is_not`] is for.
    fn of(src: &str) -> String {
        crate::plain(&sexp(&parse(src)))
    }

    /// Every literal run in a tree, in the order it was written: a grid that degraded
    /// to its own text is one Atom holding all of it, so telling the document after
    /// the grid from the last letter of the grid is a question of what follows that
    /// run rather than of what it contains.
    fn atoms_of(n: &Node) -> Vec<String> {
        match n {
            Node::Atom(s) => vec![s.clone()],
            Node::Row(v) => v.iter().flat_map(atoms_of).collect(),
            _ => Vec::new(),
        }
    }

    /// How many letters of `src` came out in the face's own math italic.
    fn italics(src: &str) -> usize {
        sexp(&parse(src))
            .chars()
            // The two italic rows are one block, capitals then lowercase; `h` is the
            // letter the block leaves out and spells as U+210E instead.
            .filter(|&c| matches!(c as u32, 0x1D434..=0x1D467 | 0x210E))
            .count()
    }

    #[test]
    fn a_written_identifier_is_italic_and_a_written_name_is_not() {
        // TeX, CoreText and DirectWrite all draw `$x^2$` slanted, and the slant is a
        // codepoint rather than a simulated oblique because only the face's italic letter
        // carries the italics correction its `MATH` table gives -- which is what an
        // accent, a bar and a superscript are placed against.
        assert_eq!(italics("xy"), 2, "a written run of letters is one identifier");
        assert_eq!(italics("XY"), 2, "in either row of the alphabet");
        // Upright, exactly as TeX sets them: a number, an operator's name, and the prose
        // `\text` brings into a formula.
        assert_eq!(italics("12"), 0);
        assert_eq!(italics("\\sin x"), 1, "the name keeps its letters, the `x` takes its");
        assert_eq!(italics("\\text{in}"), 0);
        // A switch replaces the default alphabet rather than doubling up on it.
        assert_eq!(italics("\\mathbb{R}"), 0);
        assert_eq!(of("\\mathbb{R}"), "\u{211D}", "the double-struck letter, not the italic one");
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
        // The three spellings are one construct with three proportions.
        assert_eq!(of("\\frac12"), "(frac 1 2)");
        assert_eq!(of("\\dfrac12"), "(dfrac 1 2)");
        assert_eq!(of("\\tfrac12"), "(tfrac 1 2)");
    }

    #[test]
    fn a_stacked_label_and_a_frame_take_their_arguments_in_order() {
        // The label is written first and laid out second, which is the one place in this
        // subset where the source's order and the construct's two names run against each
        // other -- so an expectation says which of the braces became which box.
        assert_eq!(of("\\overset{n}{=}"), "(stack Over = n)");
        assert_eq!(of("\\underset{n}{=}"), "(stack Under = n)");
        assert_eq!(of("\\stackrel{n}{\\to}"), "(stack Over → n)");
        assert_eq!(of("\\boxed{x+1}"), "(boxed (x + 1))");
        // An unbraced argument is still one token, in both of them.
        assert_eq!(of("\\overset n="), "(stack Over = n)");
    }

    #[test]
    fn a_numbered_equations_furniture_is_read_rather_than_spelled() {
        // Invisible in TeX, so invisible here: the alternative is a line of italic
        // `label` letters at the end of every formula an author cross-references.
        assert_eq!(of("\\label{eq:one}x"), "x");
        assert_eq!(of("x\\notag\\nonumber"), "x");
        // A tag is the author's own text, set apart -- not the word "tag" in the
        // alphabet of a variable.
        assert_eq!(of("a\\tag{7}"), "(a ((space 18) (7)))");
        // `(mod m)` is roman, and the word is one name rather than three letters.
        let pmod = of("a\\pmod{p}");
        assert!(pmod.contains("(mod") && !pmod.contains("pmod"), "{pmod}");
        let bmod = of("a\\bmod b");
        assert!(bmod.contains(" mod ") && !bmod.contains("bmod"), "{bmod}");
        assert_eq!(of("x\\qed"), "(x ∎)", "the tombstone closes a proof, it does not name it");
    }

    /// The rule boundaries of the grid a formula is, which is what an `\hline` decides.
    fn rules_of(src: &str) -> Vec<bool> {
        match &parse(src) {
            Node::Row(v) => match &v[..] {
                [Node::Array { rules, .. }] => rules.clone(),
                [Node::Fence { body, .. }] => match &**body {
                    Node::Array { rules, .. } => rules.clone(),
                    other => panic!("not a grid in a fence: {other:?}"),
                },
                other => panic!("not one grid: {other:?}"),
            },
            other => panic!("not one grid: {other:?}"),
        }
    }

    /// The boundaries of every grid in a formula, outermost first, which is how a rule
    /// written inside a nested grid is told from one written beside it.
    fn grid_rules(n: &Node, out: &mut Vec<Vec<bool>>) {
        match n {
            Node::Array { rules, rows, .. } => {
                out.push(rules.clone());
                for row in rows {
                    for cell in row {
                        grid_rules(cell, out);
                    }
                }
            }
            Node::Row(v) => v.iter().for_each(|c| grid_rules(c, out)),
            Node::Fence { body, .. } => grid_rules(body, out),
            _ => {}
        }
    }

    #[test]
    fn a_rule_across_a_grid_belongs_to_the_boundary_it_was_written_at() {
        // `\hline` used to arrive in the first cell as the word `hline`, because the
        // environment reader hands every command to `list` and `list` did not know it.
        // One entry per boundary: above each row, plus one below the last.
        assert_eq!(
            rules_of("\\begin{array}{c}\\hline a\\\\b\\\\\\hline\\end{array}"),
            vec![true, false, true],
        );
        // A rule written between two rows is the middle boundary and nothing at the ends.
        assert_eq!(rules_of("\\begin{matrix}a\\\\\\hline b\\end{matrix}"), [false, true, false]);
        // A grid with no rules in it still reports one `false` per boundary, so the
        // layout never has to guess whether the author meant none or wrote none yet.
        assert_eq!(rules_of("\\begin{matrix}a\\\\b\\\\c\\end{matrix}"), [false; 4]);
        assert_eq!(rules_of("\\begin{matrix}a\\end{matrix}"), [false, false]);
        // The name never reaches a cell, however the column spec is written.
        let g = of("\\begin{array}{|c|}\\hline 1\\\\\\hline 2\\\\\\hline\\end{array}");
        assert!(!g.contains("hline"), "{g}");
        assert_eq!(
            rules_of("\\begin{array}{|c|}\\hline 1\\\\\\hline 2\\\\\\hline\\end{array}"),
            [true, true, true],
        );
    }

    #[test]
    fn an_infix_fraction_splits_the_run_it_was_written_in() {
        // `{a+b \over c}` is the old way of writing the stack: the command sits between
        // its two operands, and everything on either side of it in the group belongs to
        // the half it is on.
        let over = of("{a+b \\over c}");
        assert!(over.contains("frac (a + b) c"), "{over}");
        assert!(!over.contains(" over "), "the name is not on the page: {over}");
        // `\choose` is the same split with the parentheses and without the rule; `nob` is
        // what the s-expression calls a barless stack, the one `\binom` draws.
        let choose = of("{n \\choose k}");
        assert_eq!(choose, "(fence () (nob n k))");
        // The split stays inside its own group, and inside its own table cell.
        assert_eq!(of("{1 \\over 2} + {3 \\over 4}").matches("frac").count(), 2);
        let cell = of("\\begin{matrix}a \\over b\\\\c\\end{matrix}");
        assert_eq!(cell.matches("frac").count(), 1, "one cell, one stack: {cell}");
        // The rest of the infix family, which differs only in the rule and what stands
        // around the stack: `\atop` has neither, `\brace` and `\brack` bring their own.
        assert!(of("{a \\atop b}").contains("(nob a b)"), "no rule, no fence");
        assert!(of("{a \\brace b}").contains("(fence {}"), "the author's own braces");
        assert!(of("{a \\brack b}").contains("(fence []"), "and brackets");
        // `\above` asks for a gap the layout reads from the face instead, so the
        // dimension is taken off the stream rather than drawn or left in the cell.
        let above = of("{a \\above 4pt b}");
        assert!(above.contains("(frac a b)"), "{above}");
        assert!(!above.contains("pt"), "the dimension is gone: {above}");
    }

    #[test]
    fn a_substack_stacks_two_conditions_under_the_operator() {
        let got = of("\\sum_{\\substack{i<j\\\\k\\neq l}}");
        assert!(got.contains("array Gathered"), "{got}");
        assert!(got.contains("i < j") && got.contains("k ≠ l"), "{got}");
        assert!(!got.contains("substack"), "the name is not on the page: {got}");
    }

    #[test]
    fn the_marks_over_a_letter_are_each_one_glyph_of_the_face() {
        // `\ddot` was the one that mattered: a second derivative wrote itself out as
        // `ddot`. None of these is a pair of the marks above it -- two `\dot`s stack at
        // two heights, which is not the same drawing.
        for (src, kind) in [
            ("\\ddot{x}", "Ddot"),
            ("\\dotdot{x}", "Ddot"),
            ("\\check{a}", "Check"),
            ("\\breve{a}", "Breve"),
            ("\\acute{e}", "Acute"),
            ("\\grave{e}", "Grave"),
            ("\\mathring{a}", "Mathring"),
        ] {
            let want: String = kind.into();
            let got = sexp(&parse(src));
            assert!(got.starts_with(&format!("(accent {want}")), "{src} -> {got}");
        }
        // The dotted ellipses take the room of the operator they stand in for.
        assert_eq!(of("a\\dotsb b"), "(a ⋯ b)");
        assert_eq!(of("a\\dotso b"), "(a … b)");
    }

    #[test]
    fn the_names_a_logic_or_vector_text_is_written_with_are_not_spelled_out() {
        // Each of these was a word in variables on the page, which is what an unknown
        // command degrades to: `A \vdash B` read as `A vdash B`.
        assert_eq!(of("A\\vdash B"), "(A ⊢ B)");
        assert_eq!(of("\\top \\land \\bot"), "(⊤ ∧ ⊥)");
        assert_eq!(of("x\\mapsto y"), "(x ↦ y)");
        assert_eq!(of("X\\cong Y\\doteq Z"), "(X ≅ Y ≐ Z)");
        // The named big operators take limits exactly as `\sum` does.
        assert_eq!(big_operator("bigwedge").unwrap(), ("⋀".into(), Limits::Default));
        assert_eq!(of("\\bigoplus_{i} V"), "((big ⨁ i -) V)");
        // An arrow over two letters is the arrow itself, asked of the face's wider
        // drawings; `\vec` stays the small mark it always was.
        let wide = parse("\\overrightarrow{AB}");
        assert!(
            matches!(&wide, Node::Row(v) if matches!(&v[..],
                [Node::Accent { accent: AccentKind::Vec, wide: true, .. }])),
            "{wide:?}"
        );
        let back = parse("\\overleftarrow{AB}");
        assert!(
            matches!(&back, Node::Row(v) if matches!(&v[..],
                [Node::Accent { accent: AccentKind::Backvec, wide: true, .. }])),
            "{back:?}"
        );
        let small = parse("\\vec{v}");
        assert!(
            matches!(&small, Node::Row(v) if matches!(&v[..],
                [Node::Accent { accent: AccentKind::Vec, wide: false, .. }])),
            "{small:?}"
        );
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
            "\\limsup\\liminf\\lg\\sec\\csc\\cot\\tanh\\arcsin",
            "\\cdots\\ldots\\vdots",
            "\\lceil x\\rceil",
        ] {
            let f = parse(src);
            let out = sexp(&f);
            assert!(!out.contains('\\'), "{src} fell back to literal text: {out}");
        }
    }

    #[test]
    fn a_limit_is_taken_under_the_name_that_takes_it() {
        // `\limsup` is `\lim`'s sibling, so a limit goes under it. `\sin` and `\log`
        // only look alike: theirs is a subscript written beside the name.
        assert_eq!(of("\\limsup_{n}"), "(big limsup! n -)");
        assert_eq!(of("\\liminf_{n}"), "(big liminf! n -)");
        assert_eq!(of("\\sin_{n}"), "(big sin? n -)");
        assert_eq!(of("\\log_{n}"), "(big log? n -)");
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
            "(array Matrix c \u{2016}\u{2016} [a])"
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

    #[test]
    fn an_alphabet_switch_moves_the_letter_to_its_own_codepoint() {
        assert_eq!(of("\\mathbb{R}"), "\u{211D}");
        assert_eq!(of("\\mathbb{N}\\cap\\mathbb{Z}"), "(\u{2115} \u{2229} \u{2124})");
        assert_eq!(of("\\mathbf{x}"), "\u{1D431}");
        assert_eq!(of("\\mathcal{L}"), "\u{2112}");
        assert_eq!(of("\\mathfrak{g}"), "\u{1D524}");
        assert_eq!(of("\\boldsymbol{v}"), "\u{1D497}");
        // The switch covers the argument it was given, so a whole word is lettered; a
        // script written after the closer is outside it, which is what TeX does too.
        assert_eq!(of("\\mathbb{AB}^{n}"), "(sup \u{1D538}\u{1D539} n)");
        // A block with no numerals of its own leaves the digit as written: a lettered
        // glyph that does not exist would come back as tofu, and 2 is not a substitute
        // for a nonexistent double-struck 2.
        assert_eq!(of("\\mathcal{L}^2"), "(sup \u{2112} 2)");
        // Punctuation and operators have no lettered spelling at all.
        assert_eq!(of("\\mathbb{+}"), "+");
        // Two switches on one letter: the inner one spent the ASCII codepoint, and the
        // outer leaves what it cannot retarget alone rather than mangling it.
        assert_eq!(of("\\mathbb{\\mathbf{R}}"), "\u{1D411}");
    }

    #[test]
    fn text_keeps_the_spaces_inside_its_own_braces() {
        // The space is the author's: `list` treats one between atoms as a separator, so
        // a word group read through the ordinary path arrives missing its own gaps.
        assert_eq!(of("\\text{hello world}"), "hello world");
        // The word group and the identifier after it are two atoms, which is the point of
        // the space being kept: `\text`'s prose is upright and the `x` beside it is a
        // variable, so welding them into one word would set the name slanted. The double
        // gap below is the author's space and the join between two atoms.
        assert_eq!(of("\\text{as }x"), "(as  x)");
        assert_eq!(of("\\mathrm{d}x"), "(d x)");
        assert_eq!(of("\\operatorname{arg min}"), "arg min");
        // Only the escapes that stand for a character are resolved; an inner group
        // contributes its words without the braces that set it off.
        assert_eq!(of("\\text{50\\% off}"), "50% off");
        assert_eq!(of("\\text{a{bc}d}"), "abcd");
        // An unbraced text argument is the one token after it, spaces and all.
        assert_eq!(of("\\text x"), "x");
        assert_eq!(of("\\text{unclosed"), "unclosed", "an unclosed group still gives its words");
    }

    #[test]
    fn a_non_ascii_letter_survives_as_one_character() {
        // Byte-at-a-time reading turned a typed `π` into two latin-1 marks and a Han
        // ideograph into three, which no font can render as the letter it is.
        assert_eq!(of("\u{3C0}+1"), "(\u{3C0} + 1)");
        assert_eq!(of("\u{3C0}\u{3C1}"), "\u{3C0}\u{3C1}", "letters still run together");
        assert_eq!(of("\\text{\u{4E2D}\u{6587}}"), "\u{4E2D}\u{6587}");
    }

    #[test]
    fn the_shorthand_delimiters_reach_the_fence() {
        // `\\{` and `\\}` are single escaped characters, not words, and the word
        // scanner only ever collected letters: both spellings came back as no
        // delimiter at all, and the `{` the reader wrote opened a group instead.
        assert_eq!(of("\\left\\{x\\right\\}"), "(fence {} x)");
        // And the spelled-out names still read the same, as they always did.
        assert_eq!(of("\\left\\lbrace x\\right\\rbrace"), "(fence {} x)");
        // `\\big\\|` keeps its doubled bar instead of eating it as an empty name.
        match parse("\\big\\|x\\big\\|") {
            Node::Row(v) => assert!(
                v.iter().any(|n| matches!(n, Node::Big { delim: '\u{2016}', .. })),
                "{v:?}",
            ),
            other => panic!("not a row: {other:?}"),
        }
    }

    #[test]
    fn a_multibyte_delimiter_is_one_character_and_drops_nothing() {
        // One byte of the three-byte fullwidth paren left the cursor inside the
        // glyph, where the next read failed to decode and the rest of the formula
        // was silently dropped.
        assert_eq!(of("\\left\u{FF08}x+1\\right\u{FF09}"), "(fence \u{FF08}\u{FF09} (x + 1))");
    }

    #[test]
    fn a_delimiterless_right_is_nothing_rather_than_a_nul() {
        // `\\right.` means "no delimiter here"; read as a character it was the
        // one-character string U+0000, measured, shaped and drawn.
        assert_eq!(of("x\\right."), "x");
        match parse("\\left x\\right.") {
            Node::Row(v) => match &v[..] {
                [Node::Fence { left: 'x', right: '\0', .. }] => {}
                other => panic!("the dot is no delimiter, and the left one is kept: {other:?}"),
            },
            other => panic!("not a row: {other:?}"),
        }
    }

    #[test]
    fn an_hline_before_an_environment_belongs_to_no_environment() {
        // TeX calls it misplaced; leaving it pending let the next environment
        // claim it and draw a rule its author never wrote.
        let node = parse("\\hline x \\begin{matrix}a\\end{matrix}");
        let Node::Row(v) = &node else { panic!("not a row: {node:?}") };
        let rules = v
            .iter()
            .find_map(|n| match n {
                Node::Array { rules, .. } => Some(rules.clone()),
                _ => None,
            })
            .expect("the environment is still a grid");
        assert_eq!(rules, vec![false, false], "no rule was claimed from before the begin");
    }

    #[test]
    fn a_left_group_too_deep_to_nest_is_read_whole() {
        // `\left` nested a few thousand deep recursed once per pair with no cap
        // at all, and a formula is a file the reader opened. Past the cap the body
        // is scanned linearly and set as the text it was written as.
        let n = 130;
        let got = of(&format!("{}x{}", "\\left(".repeat(n), ")".repeat(n)));
        assert!(got.contains("\\left("), "the body past the cap is literal text: {got}");
        // Well inside the cap nothing is degraded: every pair is still a pair.
        let few = of(&format!("{}x{}", "\\left(".repeat(20), ")".repeat(20)));
        assert!(!few.contains("\\left("), "a shallow nesting is still a nesting: {few}");
    }

    #[test]
    fn an_environment_too_deep_to_nest_is_read_whole() {
        // The same cap, one construct further out: a grid reads its cells through
        // `list`, which reads them through `command`, so a nesting of environments
        // recursed just as far as a nesting of braces did.
        let n = 130;
        let got = of(&format!("{}a{}", "\\begin{matrix}".repeat(n), "\\end{matrix}".repeat(n)));
        assert!(got.contains("\\begin{matrix}"), "the grid past the cap is literal text: {got}");
        let few = of(&format!("{}a{}", "\\begin{matrix}".repeat(10), "\\end{matrix}".repeat(10)));
        assert!(!few.contains("\\begin{matrix}"), "a shallow nesting is still a grid: {few}");
        // `\substack` reads its rows through the same reader, and is capped with it.
        let stack = of(&format!("{}a{}", "\\substack{".repeat(n), "}".repeat(n)));
        assert!(stack.contains("\\substack{"), "the stack past the cap is literal text: {stack}");
    }

    #[test]
    fn a_grid_nested_in_a_grid_too_deep_leaves_the_rest_of_the_formula() {
        // The `begin` inside a too-deep body raised the open count and no `end`
        // could bring it back down, so the scan ran to the end of the input and
        // everything written after the grid -- the rest of the formula, the whole
        // document -- was swallowed into its literal text. A nested `end` closes
        // the grid it was written with, and the outer one is the closer this one
        // was waiting for, so the cursor resumes after it.
        //
        // Two grids nested inside the collapsed body: the whole shape that used to
        // desynchronise the parse, with a `c` standing for the document written
        // after the formula. The nesting has to reach the layout cap before the
        // grids inside the body are read whole themselves, or there is nothing for
        // the scan to lose track of.
        let lead = 100;
        let src = format!(
            "{lead}\\begin{{matrix}}\\begin{{matrix}}a\\end{{matrix}}b\\end{{matrix}}c",
            lead = "\\begin{matrix}".repeat(lead)
        );
        let atoms = atoms_of(&parse(&src));
        // The deepest literal run is the grid the depth cap collapsed. What now
        // follows it is the document written after the formula -- but when the scan
        // ran on to the end of the input it was the last line of the grid instead,
        // glued in behind braces the author had already closed, and the document
        // still rendered, so nothing looked wrong.
        let collapsed =
            atoms.iter().max_by_key(|t| t.len()).expect("the collapsed grid was set");
        // The inner grid closed inside the collapsed run, at the `end` written
        // with it rather than at the end of the input: the letter that follows
        // is then the document past the formula, and the one that does not is
        // glued in behind a brace the author had already closed.
        assert!(
            collapsed.ends_with("a\\end{matrix}b\\end{matrix}")
                && atoms.last().is_some_and(|t| crate::plain(t) == "c"),
            "the grid closed inside the collapsed run, and the document is its own letter past it: {atoms:?}"
        );

        // One grid nested inside the collapsed body, closed by its own `end`, and
        // a letter of the document after it.
        let nested = parse(&format!(
            "{lead}\\begin{{matrix}}\\begin{{matrix}}b\\end{{matrix}}c\\end{{matrix}}d",
            lead = "\\begin{matrix}".repeat(lead)
        ));
        let atoms = atoms_of(&nested);
        let collapsed =
            atoms.iter().max_by_key(|t| t.len()).expect("the collapsed grid was set");
        assert!(
            collapsed.contains("b\\end{matrix}c") && after_is(&atoms, "d"),
            "the nested grid closed where it did, and the document survived: {atoms:?}"
        );

        // An unknown command is not a `begin`: `beginX` shares its first six letters
        // and must not open a grid that the scan would then have to count back down,
        // which is how the rest of the formula was lost.
        let not_begin = parse(&format!(
            "{lead}b\\beginX\\end{{matrix}}c",
            lead = "\\begin{matrix}".repeat(lead)
        ));
        let atoms = atoms_of(&not_begin);
        assert!(
            after_is(&atoms, "c"),
            "an unknown command does not unbalance the scan: {atoms:?}"
        );
    }

    /// Whether the last literal run of a tree is the letter `c`.
    fn after_is(atoms: &[String], c: &str) -> bool {
        atoms.last().is_some_and(|t| crate::plain(t) == c)
    }
    #[test]
    fn a_nested_grid_keeps_its_own_rules_to_itself() {
        // The boundaries were one shared buffer cleared on entry, so a grid nested
        // in one of these cells wiped the top rule the outer grid had already
        // collected -- and, on the degraded path, left the inner grid's rules behind
        // for the outer grid to adopt.
        assert_eq!(
            rules_of("\\begin{array}\\hline 1 \\\\ \\substack{a\\\\b} \\\\ 2\\end{array}"),
            vec![true, false, false, false]
        );
        assert_eq!(
            rules_of("\\begin{array}\\hline 1 \\\\ \\begin{matrix}a\\end{matrix} \\\\ 2\\end{array}"),
            vec![true, false, false, false]
        );
        assert_eq!(
            rules_of("\\begin{array}\\hline 1 \\\\ \\begin{psst}a\\end{psst} \\\\ 2\\end{array}"),
            vec![true, false, false, false],
            "a grid that degrades takes its rules with it"
        );
        // A `\hline` written before the `\substack` belongs to the grid the substack
        // stands in, the same rule a `\hline` before a `\begin` follows.
        let mut got = Vec::new();
        grid_rules(
            &parse("\\begin{array}\\hline 1 \\\\ \\hline \\substack{a} \\\\ 2\\end{array}"),
            &mut got,
        );
        assert_eq!(got, vec![vec![true, false, false, false], vec![false, false]]);
    }

    #[test]
    fn a_backslash_before_a_multibyte_character_keeps_the_rest_of_the_formula() {
        // One byte of the two-byte `α` was taken for a character of its own, which
        // left the reader inside the glyph: the next read could not decode, and the
        // rest of the formula was dropped rather than shown.
        assert_eq!(of("\\frac{1}{2}\\α + 1"), "((frac 1 2) α + 1)");
    }

    #[test]
    fn a_sqrt_degree_with_no_closer_is_not_a_degree() {
        // The degree was read to the end of the input, so `x^2 + 1` became the index
        // and the radical had an empty body.
        assert_eq!(of("\\sqrt[x^2 + 1"), "((sup (sqrt x) 2) + 1)");
        // A `[` of the degree's own is a bracket, so the closer is the one that
        // matches the opening one rather than the first `]` after it.
        assert_eq!(of("\\sqrt[[3]]{x}"), "(sqrt [3] x)");
    }

    #[test]
    fn an_environment_name_is_read_without_the_space_around_it() {
        // One space after a command name is consumed, which is why `\begin {matrix}`
        // worked and `\begin{ matrix }` did not -- and the whole grid degraded to
        // the prose it was written as.
        assert_eq!(of("\\begin{ matrix }a\\end{ matrix }"), of("\\begin{matrix}a\\end{matrix}"));
        assert_eq!(
            of("\\begin{array}{ c }a\\end{array}"),
            of("\\begin{array}{c}a\\end{array}"),
            "the column spec is read the same way"
        );
    }

    #[test]
    fn a_delimiter_name_with_no_shape_of_its_own_is_shown() {
        // The word was eaten and drew nothing at all, so a typo in a delimiter
        // spelled out in full was invisible on the page.
        assert_eq!(of("\\big\\foo x"), "(\\foo x)");
        assert_eq!(of("\\left\\foo x\\right)"), "(\\foo (fence -) x))");
        assert_eq!(of("x\\right\\foo"), "(x \\foo)");
    }

    #[test]
    fn a_style_switch_lasts_to_the_end_of_the_group_it_was_written_in() {
        // A group nested inside a switch claims it while it is read -- that is the
        // scope a brace group gives -- and gives it back on the way out, so the `c`
        // written after the inner group is still in the switch the outer one asked
        // for, which is what TeX's declarations do.
        assert_eq!(
            of("\\displaystyle{a\\displaystyle{b}c}"),
            "(style Display (style Display (a (style Display b) c)))"
        );
        assert_eq!(of("\\displaystyle a"), "(style Display a)");
    }
}
