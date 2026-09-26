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

**Bidirectional text.** Arabic, Hebrew, Latin, digits and Chinese can share a paragraph.
The Unicode bidirectional algorithm resolves paragraph direction and embedding levels;
visual reordering happens after Knuth–Plass line breaking. DirectWrite supplies script
shaping and mirrored glyphs, while selection, search and copying retain source order.

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

The [build workflow](https://github.com/LiJiaHua1024/rubrica/actions/workflows/build.yml)
produces a Windows x64 artifact on pushes to `main`, version tags, and manual runs.
Download `rubrica-windows-x64` from a run's Artifacts section. It contains the
executable, both licenses, this README, and `SHA256SUMS`; GitHub retains it for 30 days.

## Verifying a page without opening a window

`--report` rebuilds the *same* display list the window paints and prints the numbers:
per-line fill, which lines hang past the measure, which face drew each glyph, how many
formulae were measured from a `MATH` table, how many hyphenation points the solver took
versus how many marks were actually drawn. It is how most of this repository's typography
bugs were found, and it never touches the desktop.

```
rubrica-app --report --width 900 document.md
rubrica-app --report --dpi 144 --zoom 120 --face 1 --measure 2 document.md
rubrica-app --report --shapes document.md      # formula pieces and glyph runs, with coordinates
rubrica-app --report --no-hyphenate document.md
```

## Exporting

The reader can export the same typeset display list without going through a printer:

```
rubrica-app --export-png page.png --export-width 1080 --export-scale 2 document.md
rubrica-app --export-pdf document.pdf --pdf-width 612 --pdf-height 792 document.md
```

PNG uses the native WIC encoder and keeps the reader's colours, images, wide-content
panning geometry, and typography. PDF embeds the DirectWrite font files, writes glyph
positions and Unicode mappings, and wraps text runs in PDF `ActualText` spans so CJK,
ligatures, emoji, and bidirectional text remain selectable. Page breaks are planned at line boundaries and keep headings with the lines that follow them. Tables repeat their header on continuation pages, and continued footnotes receive an explicit continuation marker. PDF images support the enabled PNG/JPEG/GIF/BMP/TIFF/WebP decoders; formula source is also carried in an accessible text layer for copying.

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
measure, an optional vertical page-stack mode with a small peek at the next page and a
soft page-turn fade, per-document reading position and window frame restored on reopen
(clamped back onto a screen when the monitor setup changed), and a reload when the file
changes on disk. Open documents are available as pinned/preview tabs (`Ctrl+Tab`,
`Ctrl+W`), with recent files and a resizable docked workspace tree; the session is
restored on the next launch. TXT files keep chapter boundaries and lay out the active
chapter window, with `Ctrl+Alt+Up/Down` for chapter navigation. `Ctrl+3` switches
source view, `Ctrl+4` switches between continuous scrolling and the page stack, and
`Ctrl+Shift+O` opens the file in an editor at the current source position.

Right-click opens the menu: navigation, copy, select all, find, the contents, zoom, the
installed reading faces, the measure, the theme, open tabs, recent documents, and the
workspace tree.

| Key | |
| --- | --- |
| `Ctrl+O` / `Ctrl+R` | open a document · read it again from disk |
| `Ctrl+4` | continuous scrolling · vertical page stack |
| `Ctrl+Tab` / `Ctrl+W` | next/previous tab · close the active tab |
| `Ctrl+Alt+Up/Down` | previous/next TXT chapter |
| `Ctrl+F` / `F3` | find · next match |
| `Ctrl+A` / `Ctrl+C` | select all · copy |
| `Ctrl+D` | light, dark, follow the system |
| `Ctrl` + `[` `]` | the measure: two margins moving apart, and together |
| `+` `-` `0` | zoom in · out · actual size |
| `←↑↓→` `Home` `End` | move the caret · with no caret, first/last page in page-stack mode |
| `PgUp` `PgDn` | previous/next page · previous/next screen in continuous mode |
| `Shift` + arrows / `Home` / `End` | extend the selection |
| `Alt` + `←` `→` | back and forward through where you have been |
| `Enter` | step to the next find match |
| `Esc` | give up the selection, the search, the page's marking |
| `Apps` / `Shift+F10` | the menu, without a mouse |

Preferences live in `HKCU\Software\Rubrica`.

## The spacebar peek

`rubrica-app --peek` runs a small background service (a tray icon, no window) that adds
a Quick Look to the machine: select a `.md`, `.markdown` or `.txt` file in Explorer —
or on the desktop — and press space. The file is typeset by the same engine the reader
uses and shown in a rounded, topmost window centred on the monitor the pointer is over.

The switch lives in the reader's own menu, under **Spacebar peek** (next to *External
editor*): turning it on starts the service detached and remembers the choice, so the
next reader window starts it again if it is gone; turning it off takes the service
down and clears the memory. That is the whole set-up, and the only command line worth
typing is none. The preview never takes the focus, so the folder keeps the selection:
the arrow keys keep moving it and the preview follows, `Enter` opens the file with its
default program (Typora, say), and `Ctrl+Enter` hands it to a running reader as if it
had been double-clicked. Space again, `Esc`, or releasing a space held longer than a
moment puts the preview away; a quick tap toggles it instead. The tray menu offers
launch-at-sign-in and the exit.

The service watches keys through a low-level hook that does almost nothing per stroke —
a key code, a few `GetAsyncKeyState` reads, one window class — and keeps the shell's
own space bar behaviour wherever a user is typing: in the rename box, the address bar,
or the search field, whose UWP control is judged by its focus window rather than by a
caret. Everything slower (walking the shell for the selection, reading the file,
typesetting) happens off the keystroke, after a posted message.

| Key | in the peek |
| --- | --- |
| `Space` | press to show · again to hide · hold and release to dismiss |
| `Esc` | put the preview away |
| `←` `→` | (left to the folder) move the selection; the preview follows |
| `Enter` | open the file with its default program |
| `Ctrl+Enter` | open it in a running Rubrica reader |
| `F5` | read the file again from disk |
| mouse wheel | scroll the page |

## Layout of the code

| Crate | |
| --- | --- |
| `rubrica-type` | the line breaker: paragraph model, Knuth–Plass solver, justification, script classification, hyphenation |
| `rubrica-doc` | Markdown to blocks, spans, tables, footnotes and inline objects |
| `rubrica-math` | a LaTeX subset to boxes, measured through a `MathMeasure` the caller implements |
| `rubrica-app` | the window: Direct2D/DirectWrite painting, selection, hit-testing, themes, settings, workspace navigation, `--report`, PNG/PDF export, and the `--peek` spacebar service |
| `rubrica-workspace` | tab identity, preview/pinned state, recent files, and versioned session snapshots |

The first three have no dependency on Windows and no dependency on each other's
internals, which is why the whole engine is testable from a console.

## Known limitations

- Windows-only by construction, and no installer: it is a single `.exe`.
- Large TXT books (UTF-8 and explicit legacy code pages) are indexed and read by chapter, with the next chapter prefetched in the background.

## Licence

Dual-licensed under either of your choice:

- [Apache License, Version 2.0](LICENSE-APACHE)
- [MIT license](LICENSE-MIT)

Unless you explicitly state otherwise, any contribution intentionally submitted for
inclusion in this work shall be dual licensed as above, without any additional terms or
conditions.
