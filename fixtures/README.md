# Report fixtures

Run these from the repository root. `--report` builds the normal display list
without opening a window.

```powershell
cargo run -q -p rubrica-app -- --report --width 900 fixtures/notes.md
cargo run -q -p rubrica-app -- --report --width 900 fixtures/defs.md
cargo run -q -p rubrica-app -- --report --width 900 fixtures/delims.md
```

- `notes.md` exercises footnotes, citations, table layout, and formulas in cells.
  Its glyph census should include Cambria Math for the two cell formulas.
- `defs.md` exercises definition lines and ordinary wrapped prose.
- `delims.md` exercises TeX inline and display delimiters alongside literal code.

The parser tests also read these exact files so their document structure remains
reproducible without a font or a desktop session.
