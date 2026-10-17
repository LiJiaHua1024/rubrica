# Report fixtures

Run these from the repository root. `--report` builds the normal display list
without opening a window.

```powershell
cargo run -q -p rubrica-app -- --report --width 900 fixtures/notes.md
cargo run -q -p rubrica-app -- --report --width 900 fixtures/defs.md
cargo run -q -p rubrica-app -- --report --width 900 fixtures/delims.md
cargo run -q -p rubrica-app -- --report --width 900 fixtures/mermaid-flow.md
cargo run -q -p rubrica-app -- --report --width 900 fixtures/mermaid-tree.md
cargo run -q -p rubrica-app -- --report --width 900 fixtures/html.md
cargo run -q -p rubrica-app -- --report --width 900 --no-html-images fixtures/html.md
cargo run -q -p rubrica-app -- --report --width 900 fixtures/subsuper.md
cargo run -q -p rubrica-app -- --report --width 900 fixtures/highlight.md
cargo run -q -p rubrica-app -- --report --width 900 --no-highlights fixtures/highlight.md
cargo run -q -p rubrica-app -- --report --width 900 fixtures/tables.md
cargo run -q -p rubrica-app -- --report --width 900 --tables booktabs fixtures/tables.md
cargo run -q -p rubrica-app -- --report --width 900 --inline-code capsule fixtures/tables.md
```

- `notes.md` exercises footnotes, citations, table layout, and formulas in cells.
  Its glyph census should include Cambria Math for the two cell formulas.
- `defs.md` exercises definition lines and ordinary wrapped prose.
- `delims.md` exercises TeX inline and display delimiters alongside literal code.
- `mermaid-flow.md` (and `mermaid-sequence.md`, `-state`, `-class`, `-er`,
  `-pie`, `-mindmap`, `-tree`) exercises the drawn diagrams: the report's
  `diagrams` line counts fences, elements, links, shapes and labels per
  document, and `--shapes` dumps the shape list. `mermaid-broken.md` holds two
  fences that cannot be drawn, which must stay code blocks with
  `!! not drawn: ...` reasons instead of panicking.
- `rtl.md` exercises Arabic/Hebrew shaping, mixed scripts, digits, mirrored brackets,
  isolates, wrapped RTL paragraphs, links, and aligned table cells. Use `--shapes` to
  inspect each painted run's embedding level, left edge, DirectWrite origin and glyphs.
- `html.md` exercises the semantics of raw HTML in a Markdown page: `<sub>`/`<sup>`
  set as real scripts, formatting tags worn and dropped, separator tags
  (`<td>`, `<br>`) parting fields without gluing or inserting a space between CJK,
  whole `<table>`/`<details>` constructions shown as their own source, an isolated
  `<div>` wrapper shown without its tags, and figures named by `<img>` (one with
  Typora's `style="zoom:87%"`) laid out from `html-fig.png` and `html-wide.png`.
  The report's `images` line counts figures per source and prints each one's placed
  size. `--no-html-images` reports the same page with every `<img>` read as its alt
  words instead of its box; `--source` shows the file byte for byte.
- `subsuper.md` exercises Pandoc's `~`/`^` scripts: intraword pairs both ways
  (`H~2~O`, `2^10^`), CJK and emphasis inside a pair, the escaped space
  (`x^a\ b^`, printed as a non-breaking space) against the refused plain one, the
  author's own escaped marks (`\~`, `\^`), unpaired marks and runs of two or more
  left literal, `~~deletion~~` untouched, the writer's own `x^$a$^` left literal
  around a formula, and inline/fenced code immune. The report's `scripts` line
  prints the raised/lowered counts and the three numbers the marks were set with
  (from the formula face's `MATH` table); `--shapes` shows a script's run beside a
  citation's mark, the two the same size and raise.
- `highlight.md` exercises Pandoc's `==mark==`: a pair around a word and around a
  phrase carried across a wrapped line (one band under every word it crosses, a
  soft break included), a mark heading a paragraph or glued to the words on
  either side (`x==y==z`, `中文==高亮==中文`, a Hebrew word), the shapes a pair is
  refused (`== a ==`, `==a ==`, `== a==`, comparison signs, a backslash-made
  sign), and runs of three or more equals signs, which this reader keeps whole
  where Pandoc cuts a pair out of them. A mark wraps emphasis, strong, a link, a
  formula and a script without disturbing any of them, marks inside table cells
  and a definition are read the same way, and inline/fenced code is immune. The
  report's `highlight` line counts the marked spans and the bands drawn;
  `--no-highlights` reads the same page with every mark as written.
- `tables.md` exercises the reader's two drawing switches, which change ink and
  nothing else: a table ruled as a full grid against the printed table's three
  rules (`--tables grid|booktabs`), and an inline code span set as its monospaced
  face and ink against the same span on a capsule of its own
  (`--inline-code plain|capsule`). Neither switch moves a row, a column, or the
  reader's place, so the two reports are the same page seen twice -- the report
  carries a `tables` line only when the three rules were asked for and an
  `inline code` line only when capsules were, each the record that the other
  design was not what got drawn. It also holds code in prose, lists, table cells,
  a heading and a definition (all drawn, none of it measured differently with a
  capsule), a wrapped span that draws one box per line it crosses, a fence whose
  own text never wears one, and a 26-row table `--pdf-height` cuts across pages so
  every segment can be checked for a complete frame.

The parser tests also read these exact files so their document structure remains
reproducible without a font or a desktop session.
