# Tree views

Mermaid's `treeView` drawn as a directory listing: indented rows in the first
fence, box-drawing rows in the second. Annotation rows carry the `##`
descriptions, the `:::highlight` class and the `icon(...)` hints.

```mermaid
treeView
  my-project/ ## the repository root
    src/
      main.rs :::highlight
      "notes on the reader.txt" icon(file)
      components/
        button.rs
        "text field.rs"
    tests/
      layout.rs ## the layout invariants
    README.md
```

The same shape written with box-drawing rows, heavy branches and all:

```mermaid
treeView-beta
┣━━ src/
┃   ┣━━ index.js
┃   ┗━━ lib/
┃       ┗━━ util.js
┗━━ package.json ## the manifest
```

Prose after the trees proves the two ends of the band update: the document
keeps flowing below whatever was drawn.
