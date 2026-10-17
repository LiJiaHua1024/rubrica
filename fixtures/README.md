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

The parser tests also read these exact files so their document structure remains
reproducible without a font or a desktop session.
