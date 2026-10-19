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
cargo run --release --features pdf -p rubrica-app -- --export-pdf document.pdf --pdf-width 612 --pdf-height 792 document.md
```

PNG uses the native WIC encoder and keeps the reader's colours, images, wide-content
panning geometry, and typography. PDF embeds the DirectWrite font files, writes glyph
positions and Unicode mappings, and wraps text runs in PDF `ActualText` spans so CJK,
ligatures, emoji, and bidirectional text remain selectable. Page breaks are planned at line boundaries and keep headings with the lines that follow them. Tables repeat their header on continuation pages, and continued footnotes receive an explicit continuation marker. PDF images support the enabled PNG/JPEG/GIF/BMP/TIFF/WebP decoders; formula source is also carried in an accessible text layer for copying. PDF export lives behind the `pdf` feature because the reading window never touches the font and image stack it pulls in, so a plain `cargo build --release` produces the smaller reader without it.

## What it reads

CommonMark and GFM: headings, paragraphs, lists (ordered, nested, task lists), block
quotes, fenced code with syntax colouring (273 fence names over 56 scanners, and a tab
inside a fence is a tab stop every four characters), tables with alignment and rules on
every row and column, images, links and footnotes, strikethrough, autolinks, Pandoc's
`H~2~O` and
`x^2^` scripts (the scale and the shift are read from the formula face's `MATH` table,
so a script, a `<sup>` and a footnote's `¹` are set with one pair of numbers; a mark
alone, or a run of two or more, is printed as written rather than read as Pandoc's
degenerate empty script), `==marked==` words (a pair of equals signs paints a band
behind its words wherever prose stands -- a wrapped phrase, a table cell, a definition
-- wrapping emphasis, links, formulas and scripts without disturbing them, while a run
of three or more equals signs is printed as written), raw HTML read for its
meaning -- `<b>` and `<i>` dress their words, `<sub>`/`<sup>` set as real scripts,
`<br>` and separator tags part fields, `<img>` draws the figure it names (Typora's
`style="zoom:87%"` included), a whole `<table>` or `<details>` is shown as the source it
was written as, and a wrapper tag is dropped rather than printed -- YAML front matter
dropped as metadata, and `\(...\)`, `\[...\]` and `$...$` formulas.
Definition lists (`Term` over a line starting `: `) are read as terms and definitions.

One deliberate leave from CommonMark, settled heuristically: an ordered marker (`3.` or
`3)`) at the head of a line starts its own item even without a blank line above it --
but only when the line above finished its thought, meaning sentence punctuation with
any closing quotes or brackets riding on it, or a lead-in colon. A line still in
mid-sentence keeps its wrap, which is what saves the hard-wrapped English sentence the
strict rule exists for. Fences, display equations and a list already running are read
exactly as the spec reads them.

Math covers fractions and roots, scripts and limits, big operators with stacked
sub-superscripts, matrices and cases and `aligned`, `\hline` and column rulings,
`\over`/`\choose`/`\atop`/`\brace`/`\brack`/`\above`, extensible brackets and braces,
wide accents, `\overset`/`\underset`/`\stackrel`/`\boxed`, the lettering styles
(`\mathbb`, `\mathbf`, `\mathcal`, `\mathrm`, `\mathfrak`, `\mathscr`, `\mathsf`,
`\mathtt`, `\text`), and the TeX style switches `\displaystyle`, `\textstyle`,
`\scriptstyle` and `\scriptscriptstyle`.

Chemical equations support extensible reaction arrows such as
`\xrightarrow[below]{above}` and `\xrightleftharpoons[below]{above}`, equilibrium
symbols, gas/precipitate arrows and scripted ions. A common subset of mhchem's
`\ce{...}` notation sets elements upright and handles coefficients, automatic
subscripts, charges (`SO4^2-`, `NH4+`), states, isotopes, bonds, hydrates and labelled
reactions (`\ce{N2 + 3H2 <=>[催化剂][高温、高压] 2NH3}`). It does not implement the
complete mhchem language. See `fixtures/chemistry.md` for examples.
Ordinary TeX reactions whose identifiers can be recognised as chemical elements
also use upright lettering and longer reaction arrows. Arrow conditions are centred
at a readable size; arrows grow to leave room on both sides of their labels.

Mermaid fences are drawn rather than set as code. Flowcharts (`flowchart`/`graph`,
both directions, the full set of node shapes, arrow styles, edge labels in pipes or
between two dashes, subgraphs and self-loops), sequence diagrams (participants,
messages, notes, fragments), state diagrams (including `[*]` and `: transition
labels`), class diagrams, ER diagrams, pie charts, mindmaps and tree views
(`treeView`/`treeView-beta`: indentation or box-drawing rows, quoted names,
`##` descriptions, `:::highlight`, `icon(...)` read and dropped) come out as
shapes, arrows and labels. Lines, fills and words take their colours from the light
and dark palettes, and the words are shaped through the same pipeline as the prose,
so they stay real, selectable text in exported PDFs. A fence that cannot be drawn --
a syntax error, or a diagram kind this reader does not draw such as `gantt` --
stays a code block with a one-line reason above it, and diagrams past the size
ceiling are refused the same way rather than laid out. `fixtures/mermaid-flow.md`
and its siblings exercise the families; `--report` counts each diagram's elements,
links, shapes and labels, and `--report --shapes` dumps them.

## Reading features

Selection by mouse and keyboard with clipboard copy, a caret that follows the pointer,
links and footnote citations as jump targets with back and forward history, an outline
of the document's headings, find in page, zoom, light and dark themes, a reader-set
measure, an optional vertical page-stack mode with a small peek at the next page and a
soft page-turn fade, per-document reading position and window frame restored on reopen
(clamped back onto a screen when the monitor setup changed), and a reload when the file
changes on disk. Resting the pointer on a footnote citation opens the note itself in a
bubble, set at the same scale and by the same rules as the apparatus at the foot of the
page; a note too long for the bubble is cut with an ellipsis, and a click on the citation
still goes to the whole of it. Every jump is undone with `Ctrl+Z`, so a reader who has
followed a citation to the end of a long document has one keypress back to the sentence
they left. Open documents are available as pinned/preview tabs (`Ctrl+Tab`,
`Ctrl+W`), with recent files and a resizable docked workspace tree whose own search box
filters its listing by name or extension -- `txt` finds `reading.txt`, `notes txt` asks for
both words, `*.md` asks for the extension itself, and `Esc` empties the box; the session is
restored on the next launch. TXT files keep chapter boundaries and lay out the active
chapter window, with `Ctrl+Alt+Up/Down` for chapter navigation. `Ctrl+3` switches
source view, `Ctrl+4` switches between continuous scrolling and the page stack, and
`Ctrl+Shift+O` opens the file in an editor at the current source position.

Dragging a selection to the top or bottom of the page scrolls it on, and keeps it scrolling
while the pointer stays there: the twenty points next to the page's own edge move at a steady
six points a beat, and past that edge -- over the tab strip, into the status bar, or clean out
of the window -- the further the pointer is taken the faster the page goes, up to four times
the base. A selection longer than the window is finished without letting go, and the text that
scrolls past is what gets marked.

Right-click opens the menu: navigation, copy, select all, select table, find, the contents,
zoom, the installed reading faces, the measure, the theme, the text-reading switches (plain
text or Markdown, **Show HTML image tags**, **Show `==text==` as a highlight**, **Table
lines**, **Inline code**), open tabs, recent documents, and the workspace tree.

A table is copied as a table rather than as a run of prose. A press inside a grid marks the
cell it landed on, a drag across one marks the rectangle of cells between the two, and
`Ctrl+C` puts them on the clipboard as tab-separated fields, one line per row, with a field
that holds a tab, a line break or a quote wrapped in quotes so the shape survives; a cell
that wraps in the column is still one field, and a `<br>` in a cell is still a break inside
one. `Ctrl+Shift+A` (or the menu's **Select Table**) marks the whole grid the caret is in,
which is the only way to take a table taller than the window. Any other way of marking text
— an arrow key, `Ctrl+A`, a click in the prose — puts the cells away. The headless runs take
`--table-select all`, `--table-select N`, or `--table-select N:B2:D3`, and print the fields
under a `table select` line; a spec that names no table, row or column is refused with the
count the page has, and the switch is read on the report's own page, so it is written with
`--report` (a run that asks for a table without one is told so rather than opening a
window).

A page's own furniture has two more choices, and both are drawn rather than measured: a
table can be ruled as the full grid or as the printed table's three rules (a rule above,
one under the header, one below the last row, and no verticals), and an inline code span
can be set as the monospaced face and its own ink or on a capsule of its own. Neither
switch moves a row, a column, or the reader's place — the lines and the box stand on
coordinates the layout had already placed, so both can be changed mid-page. The headless
runs take `--tables grid|booktabs` and `--inline-code plain|capsule`, both defaulting to
the design every page was drawn with before the switches existed (the grid, and plain
code); a `--report` carries a `tables` or `inline code` line only when the other design
was asked for.

A fence's code has two more, and both are ink as well. The five syntax inks can be kept as
the design picked them or set on a lightness ladder — the same five colours, held at least
7.5 L\* apart in CIE L\*ab and each still clearing 4.5:1 against the panel behind it — and
a fence that named a language can have that name set small and quiet in its panel's own
corner. Neither moves a line: the ladder is spent on the resolved palette as it is handed
to the painter, and the label is drawn into the display list after the panel and before
the words, so the page's height and every coordinate are the ones it had. The headless
runs take `--code-ladder design|ladder` and `--code-language none|corner`, both defaulting
to the design every page was drawn with (the hand-picked five, and no label); a `--report`
carries a `code ladder` or `code label` line only when the other design was asked for.

A figure opens when it is clicked. The page's measure is the right size for prose and the
wrong size for a screenshot, so the picture is shown over the whole window, at fit to
begin with: the wheel zooms about the pointer — the pixel under the cursor is the pixel
that stays under it — a drag pans, `0` shows the figure at its own size (one image pixel
to one CSS pixel), `F` fits it again, and `Esc` or a click on the backdrop around it puts
it away. A figure with no backdrop left to click is put away with `Esc`, or by dragging it
back into view. An animated GIF plays while it is up, each frame held for the delay its
own bytes state and never for less than a twentieth of a second; an *optimized* GIF, whose
frames are stored as the rectangles that changed rather than as whole pictures, is shown
as its first frame, because compositing those is a decoder's work and a slideshow of
fragments is worse than a still. A `--report` names the frames of an animated figure on
its line, and says `partial` where the file is one this build will not play.

A page's letters have a setting of their own, and a narrow column has two. The
**Typography** form — the menu's *Typography ▸ Edit / Save Preset…* — carries the tracking
every run is set with (the letter spacing, in ems), the *Ragged below* width that says when
a column is too narrow to justify, and the switch that turns that fallback off. A column
narrower than the width is set to the left margin instead of being stretched, because a few
words spread across a short measure read worse than a ragged edge; a reader who wants the
justification at every width unticks the switch. Headless runs take
`--narrow-align left|justify`, whose default is the design's own answer (`left`), and the
`layout` line a `--report` carries names whichever of these numbers or switches the page is
no longer set with.

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
| `Esc` | give up the selection, the search, the page's marking, the workspace tree's filter |
| `Apps` / `Shift+F10` | the menu, without a mouse |
| mouse wheel | scroll the page · `Ctrl` zooms · `Shift` pans a table wider than the window |
| click a figure | show it over the whole window: wheel zooms at the pointer · drag pans · `0` actual size · `F` fit · `+` `-` zoom · `Esc` puts it away |

Preferences live in `HKCU\Software\Rubrica`.

## Colour themes

Light and dark each draw with a built-in pair: `Default Light` and `Default Dark`, the
page as it has always been set. Four more ship alongside them — `Mono Light` and
`Mono Dark`, a page with no hue in it at all, and `Solarized Light` and `Solarized
Dark`, after the published palette of that name. Solarized's quiet inks are a step
deeper than published and its selected tab a step further from the page, because the
published pair is a low-contrast design and this reader holds every word on the page
to a 4.5:1 bar; the hues are the homage, the depth is the bar.

The menu's **Colour theme** submenu is where a choice is made. It holds two slots,
**Light theme** and **Dark theme**: each lists the themes of its own appearance with
the one in use ticked, a pick is remembered across restarts, and choosing the built-in
default is how a slot is emptied. `Ctrl+D` still moves between the two appearances. A
remembered name that has gone missing — its file deleted, or replaced by a theme for
the other appearance — falls back to the built-in default and its row says why, rather
than drawing a page in colours nobody chose.

**The reader's own themes** are TOML files in `%APPDATA%\Rubrica\themes`.
**Open Themes Folder** makes the folder if it is not there and opens it, and
**Copy as My Theme…** writes the theme on screen into it as a file of its own — every
colour spelled out, so it is a starting point rather than a reference to what it was
copied from. A theme file is noticed when it is saved: the folder is watched, and a
file added or edited changes the menu and the page within the second, no restart.

A theme file says what it is and what it changes:

```toml
[meta]
name = "My Page"              # required; the name the menu shows
appearance = "light"          # required; "light" or "dark"
# based_on = "Mono Light"     # optional: everything left out comes from this theme

[colors]                      # the page and the inks on it
bg = "#fbfbf9"
accent = "#1f5b9e"
highlight = "#fbe6a4"

[chrome]                      # the window's furniture; may be left out entirely
sidebar = "#efefe9"
```

The fourteen `[colors]` keys are `bg`, `text`, `muted`, `accent`, `code_bg`,
`page_surface`, `page_shadow`, `on_accent`, `keyword`, `string`, `comment`, `number`,
`ty` and `highlight`; the nine `[chrome]` keys are `sidebar`, `sidebar_text`,
`tab_strip`, `tab_text`, `active_tab`, `active_tab_text`, `tab_inactive`, `tab_hover`
and `divider`. Colours are `#rgb` or `#rrggbb` in quotes. A theme is a layer rather
than a palette: a key it leaves out comes from `based_on`, that theme's own `based_on`
before it, and past the end of the chain — eight themes at most — from the built-in
default of the appearance; a `based_on` naming a theme of the other appearance is
refused whole. Most themes leave all of `[chrome]` out: the sidebar is the code
panel's colour, the resting tab label the quiet ink, the selected tab the accent and
the rules the strip's edge — all settled from the page's own colours — and the strip's
three tones stand as the built-in pair's until a theme says otherwise.

A file that cannot be a theme is set aside whole and never half-drawn — no `name`, an
`appearance` that is neither `light` nor `dark`, not UTF-8, over 64 KiB, a name that
is a built-in's or another file's — and the reason is kept for the listing. One key
that is not a colour costs only that key: the rest of the file still says what it
says, and the listing names the key that was passed over.

The same themes are available to headless runs:

```
rubrica-app --list-themes                         # what is installed, and what was not read
rubrica-app --theme "Mono Dark" --report document.md
rubrica-app --export-png page.png --theme my-page.toml document.md
```

`--theme` takes a name (case is not information) or a path to a `.toml` anywhere.
`--list-themes` prints the built-ins, every file read, every file set aside with its
reason and both slots, and exits without opening a window. In the window a `--theme`
that cannot be used is reported on the console and the default pair draws; a report or
an export refuses instead, because a file that is not the theme that was asked for is
worse than no file. A `--report` run whose theme came from a slot or from `--theme`
prints one `theme` line saying what was drawn with; a page nobody gave a theme to
reports exactly as it always did.

## Typography presets

A preset is everything a page is set with: the nineteen font roles, the twenty-one
numbers and the two drawing switches. The typography form has always been able to write
one into the registry under a name. **The reader's own presets** are TOML files in
`%APPDATA%\Rubrica\typography` — the copy that can be opened in an editor, kept beside a
book, or handed to somebody else.

A preset file says what it is and what it changes:

```toml
[meta]
name = "Wide markers"         # required; the name the menu and --profile use
# based_on = "Book"           # optional: everything left out comes from this preset

[fonts]                       # the nineteen font roles; all optional
latin_body = "Georgia"

[metrics]                     # the twenty-one numbers; all optional
size = 15.0
marker_inset = 1.2

[flags]                       # the drawing switches; all optional
code_capsule = true
table_lines = "booktabs"
code_ladder = true
code_lang_label = true

[bind]                        # the extensions this preset is the one for
extensions = "txt, log"
```

The nineteen `[fonts]` keys are `latin_body`, `latin_headings`, `latin_code`,
`chinese_body`, `chinese_headings`, `chinese_code`, `japanese_body`, `japanese_headings`,
`japanese_code`, `korean_body`, `korean_headings`, `korean_code`, `latin_emphasis`,
`chinese_emphasis`, `japanese_emphasis`, `korean_emphasis`, `math`, `math_fallback` and
`font_fallback`, each a font family in quotes. The twenty-one `[metrics]` keys are `size`
(in points), `latin_body_leading`, `asian_body_leading`, `tracking`, `paragraph_gap`,
`first_indent`, `column`, `ragged_below`, `punctuation_compression`,
`hanging_punctuation`, `latin_heading_leading`, `asian_heading_leading`, `heading_gap`,
`code_gap`, `quote_indent`, `list_indent`, `definition_indent`, and the four layout
quantities this build added: `marker_inset` (0–6 em, how far a list's marker hangs),
`list_marker_gap` (0–4 em, the air between the marker and the words it introduces),
`list_bullet_size` (0.4–2.5, how large the marker is drawn) and `quote_bar_gap` (0–4 em,
how far a quotation's bar stands from the column it marks). `[flags]` holds `code_capsule`
(`true`/`false`), `table_lines` (`"grid"` or `"booktabs"`), `keep_korean_words`
(`true`/`false`), `narrow_ragged` (`true`/`false`: the switch that stands beside
`ragged_below`, saying whether a column narrower than that width is set to the left margin),
`code_ladder` (`true`/`false`: whether the five syntax inks are held on the lightness
ladder) and `code_lang_label` (`true`/`false`: whether a fence's language is named in its
panel's corner).

Nothing is refused whole for one line this reader cannot read. A key it does not know, a
number outside the range the form would take, a switch that is neither `true` nor
`false`: each is set aside by name, with the reason, and `--list-profiles` prints both,
while the rest of the file still says what it says — a typo is a line in a listing rather
than a preset that quietly is not what it says. What a file *must* say is what it is: a
`[meta] name`. No name, an empty one, one over 80 characters, or one that takes `Default`
or `Book` — names the reader did not write — is set aside whole with its reason. A file
whose name is one the form has saved stands in its place, because the file is the copy
the reader can see and edit.

`based_on` names the preset this one is written over — another file, or a name from the
form — and a file that names nothing builds on the design's own page, which is what
`Default` holds. A chain is walked root to leaf and may be eight presets long; a name
nothing answers to, or a chain that comes back round to where it started, is set aside
with its reason rather than half-drawn.

`[bind] extensions` is the answer to "what is a `.txt` file set with?": a list of
extensions, with or without the dot, where case is not information. A document is set
with the preset bound to the extension it ends with, ahead of the name this reader
remembers for that kind of document; a `--profile` on the command line is the reader's own
word and stands over both. A preset file is noticed when it is saved: the folder is
watched, and a preset added or edited is live within the second in the window, in the
spacebar peek, and in the next report or export — no restart.

The same presets are available to headless runs:

```
rubrica-app --list-profiles                         # what is installed, and what was not read
rubrica-app --profile "Wide markers" --report document.md
rubrica-app --export-png page.png --profile "Wide markers" document.md
```

`--list-profiles` prints the built-ins, the presets the form saved, every file read with
its path and what it is based on and bound to, every file set aside with its reason, and
the preset in force for each of the two slots, and exits without opening a window.
`--profile` takes a name (case is not information); in a report or an export a name
nothing answers to refuses rather than drawing something else, because a page set with a
preset that was asked for and not found is worse than no page. A report whose preset moved
one of the four layout numbers prints one `layout` line naming the ones it moved.

The four layout numbers can also be given on the command line, where they need no file at
all: `--marker-inset`, `--list-marker-gap`, `--list-bullet-size` and `--quote-bar-gap`,
each taking the range above. A value outside it is refused, exit 1.

## The spacebar peek

`rubrica-app --peek` runs a small background service (a tray icon, no window) that adds
a Quick Look to the machine: select a `.md`, `.markdown` or `.txt` file in Explorer —
or on the desktop — and press space. The file is typeset by the same engine the reader
uses and shown in a rounded, topmost window centred on the monitor the pointer is over.

The switch lives in the reader's own menu, under **Spacebar peek** (next to *External
editor*): turning it on starts the service detached and remembers the choice, so the
next reader window starts it again if it is gone; turning it off takes the service
down and clears the memory. That is the whole set-up, and the only command line worth
typing is none. The same menu says what a press of the space bar means — *tap to
toggle* (press to show, press again to hide), *hold to preview* (show while held, gone
on release), or *tap or hold* (both, told apart by how long the key stayed down) — and
whether a preview closes as soon as the folder that opened it loses the focus. The
preview never takes the focus itself, so the folder keeps the selection: the arrow
keys keep moving it and the preview follows, `Enter` opens the file with its default
program (Typora, say), and `Ctrl+Enter` hands it to a running reader as if it had been
double-clicked. Space again, `Esc`, or releasing a held space puts the preview away.
The tray menu offers launch-at-sign-in and the exit.

The service watches keys through a low-level hook that does almost nothing per stroke —
a key code, a few `GetAsyncKeyState` reads, one window class — and keeps the shell's
own space bar behaviour wherever a user is typing: in the rename box, the address bar,
or the search field, whose UWP control is judged by its focus window rather than by a
caret. Everything slower (walking the shell for the selection, reading the file,
typesetting) happens off the keystroke, after a posted message.

| Key | in the peek |
| --- | --- |
| `Space` | tap to toggle · hold to peek · or both, as the menu says |
| `Esc` | put the preview away |
| `←` `→` | (left to the folder) move the selection; the preview follows |
| `Enter` | open the file with its default program |
| `Ctrl+Enter` | open it in a running Rubrica reader |
| `F5` | read the file again from disk |
| mouse wheel | scroll the page · with `Shift`, pan a table wider than the window |

## Layout of the code

| Crate | |
| --- | --- |
| `rubrica-type` | the line breaker: paragraph model, Knuth–Plass solver, justification, script classification, hyphenation |
| `rubrica-doc` | Markdown to blocks, spans, tables, footnotes and inline objects |
| `rubrica-math` | a LaTeX subset to boxes, measured through a `MathMeasure` the caller implements |
| `rubrica-mermaid` | Mermaid fences to scenes: a parser, a layered layout and shape primitives, with no dependencies at all |
| `rubrica-app` | the window: Direct2D/DirectWrite painting, selection, hit-testing, themes, settings, workspace navigation, `--report`, PNG/PDF export, and the `--peek` spacebar service |
| `rubrica-workspace` | tab identity, preview/pinned state, recent files, and versioned session snapshots |

The first three have no dependency on Windows and no dependency on each other's
internals; `rubrica-mermaid` is portable the same way, and `rubrica-doc` reaches it
only through its public API, which is why the whole engine is testable from a console.

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
