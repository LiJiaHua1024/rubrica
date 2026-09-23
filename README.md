# Rubrica

A Markdown reader for Windows built around a typesetting engine rather than a browser.

Rubrica sets a document the way a typesetter would: it solves each paragraph's line
breaks as a whole, measures every formula from the font's own `MATH` table, and draws
the result directly with Direct2D and DirectWrite. There is no webview, no Electron, no
HTML or CSS anywhere in the pipeline — which is what lets the page keep its rhythm when
the text is Chinese, when a line has to be hyphenated, or when a fraction has to grow
the delimiters around it.

## What that buys you

**Global line breaking.** Paragraphs are laid out with Knuth–Plass: every legal set of
breaks in the paragraph is scored and the cheapest is kept, instead of filling one line
and accepting whatever raggedness falls out. The scoring model is TeX's — cubic badness,
four fitness classes with class-jump penalties, a `\pretolerance`/`\tolerance` pair, and
`\emergencystretch` as a last resort before conceding an overfull line.

**Chinese and English in the same box.** Han text has no word spaces, so justification
has nothing to work with unless the engine supplies it. Rubrica inserts elastic glue at
every script boundary and between ideographs, treats punctuation that may not open a
line as unbreakable, and gives each script the face that actually carries it. A Chinese
line stretches at the joins; it never compresses them, because two ideographs at natural
width are already touching and any shrink there is ink laid over ink.

**Formulas from the font, not from CSS.** `\frac`, `\sum`, matrices, roots, accents and
delimiters are placed with the metrics in the `MATH` table: the table's constants decide
where a fraction rule sits, its glyph-construction tables decide whether a bracket is
swapped for a taller variant or built from parts, and its horizontal-coverage table
decides how far a `\widehat` stretches. Cambria Math is what this was developed against;
any font carrying a `MATH` table works.

**Discretionary hyphenation.** English words break at Knuth–Liang points, and the solver
withholds them from its first pass, so a paragraph that can be set without hyphens is
set without hyphens. A code line too long for the measure is cut at a character instead,
with no mark drawn — a hyphen inside an identifier would be a lie about its name.

## Building

Requires the MSVC toolchain and Windows.

```
cargo build --release
target\release\rubrica-app.exe            # opens the document you left off on
target\release\rubrica-app.exe notes.md
```

```
cargo test --workspace                    # the whole engine, headless
cargo clippy --workspace --all-targets
```

## Verifying a page without opening a window

`--report` rebuilds the *same* display list the window paints and prints the numbers:
per-line fill, which lines hang past the measure, which face drew each glyph, how many
formulae were measured from a `MATH` table, how many hyphenation points the solver took
versus how many marks were actually drawn. It is how most of this repository's typography
bugs were found, and it never touches the desktop.

```
rubrica-app --report --width 900 document.md
rubrica-app --report --dpi 144 --zoom 120 --face 1 --measure 2 document.md
rubrica-app --report --shapes document.md      # every piece of a formula, with coordinates
rubrica-app --report --no-hyphenate document.md
```

## What it reads

CommonMark and GFM: headings, paragraphs, lists (ordered, nested, task lists), block
quotes, fenced code with syntax colouring, tables with alignment and rules on every row
and column, images, links and footnotes, strikethrough, autolinks, raw HTML kept as text,
YAML front matter dropped as metadata, and `\(...\)`, `\[...\]` and `$...$` formulas.
Definition lists (`Term` over a line starting `: `) are read as terms and definitions.

Math covers fractions and roots, scripts and limits, big operators with stacked
sub-superscripts, matrices and cases and `aligned`, `\hline` and column rulings,
`\over`/`\choose`/`\atop`/`\brace`/`\brack`/`\above`, extensible brackets and braces,
wide accents, `\overset`/`\underset`/`\stackrel`/`\boxed`, the lettering styles
(`\mathbb`, `\mathbf`, `\mathcal`, `\mathrm`, `\mathfrak`, `\mathscr`, `\mathsf`,
`\mathtt`, `\text`), and the TeX style switches `\displaystyle`, `\textstyle`,
`\scriptstyle` and `\scriptscriptstyle`.

## Reading features

Selection by mouse and keyboard with clipboard copy, a caret that follows the pointer,
links and footnote citations as jump targets with back and forward history, an outline
of the document's headings, find in page, zoom, light and dark themes, a reader-set
measure, per-document reading position and window frame restored on reopen (clamped back
onto a screen when the monitor setup changed), and a reload when the file changes on
disk.

Right-click opens the menu: navigation, copy, select all, find, the contents, zoom, the
installed reading faces, the measure, and the theme.

| Key | |
| --- | --- |
| `Ctrl+O` / `Ctrl+R` | open a document · read it again from disk |
| `Ctrl+F` / `F3` | find · next match |
| `Ctrl+A` / `Ctrl+C` | select all · copy |
| `Ctrl+D` | light, dark, follow the system |
| `Ctrl` + `[` `]` | the measure: two margins moving apart, and together |
| `+` `-` `0` | zoom in · out · actual size |
| `←↑↓→` `Home` `End` `PgUp` `PgDn` | move the caret |
| `Shift` + any of those | extend the selection |
| `Alt` + `←` `→` | back and forward through where you have been |
| `Enter` | step to the next find match |
| `Esc` | give up the selection, the search, the page's marking |
| `Apps` / `Shift+F10` | the menu, without a mouse |

Preferences live in `HKCU\Software\Rubrica`.

## Layout of the code

| Crate | |
| --- | --- |
| `rubrica-type` | the line breaker: paragraph model, Knuth–Plass solver, justification, script classification, hyphenation |
| `rubrica-doc` | Markdown to blocks, spans, tables, footnotes and inline objects |
| `rubrica-math` | a LaTeX subset to boxes, measured through a `MathMeasure` the caller implements |
| `rubrica-app` | the window: Direct2D/DirectWrite painting, selection, hit-testing, themes, settings, `--report` |

The first three have no dependency on Windows and no dependency on each other's
internals, which is why the whole engine is testable from a console.

## Known limitations

- **No right-to-left text.** `bidiLevel` is fixed at 0, so Arabic and Hebrew are shaped
  but laid out left to right. `--report` counts how much of a page that affects.
- Windows-only by construction, and no installer: it is a single `.exe`.
- No document writing, no tabs, no PDF export.

## Licence

Dual-licensed under either of your choice:

- [Apache License, Version 2.0](LICENSE-APACHE)
- [MIT license](LICENSE-MIT)

Unless you explicitly state otherwise, any contribution intentionally submitted for
inclusion in this work shall be dual licensed as above, without any additional terms or
conditions.
