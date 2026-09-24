# Reader feature implementation

Scope: implement the complete feature gap review accepted on 2026-09-23. Preserve native DirectWrite/Direct2D rendering, whole-paragraph line breaking, OpenType MATH layout, and the completed bidirectional text support.

## Acceptance checklist

- [x] Typography profiles: editable Latin, Chinese, Japanese, Korean, emphasis, code and math fonts; font size, leading, tracking, paragraph spacing and indentation; named saved profiles and a book preset; settings UI and persistence.
- [x] East Asian typography: punctuation compression and hanging policies, narrow-column ragged fallback, language-aware Japanese and Korean font selection and Korean keep-all breaking.
- [x] Export: whole-document PNG with width/scale controls and native selectable-text PDF; pagination with heading/paragraph protection, repeated table headers and continued footnotes; reading typography and colours retained.
- [x] Multiple documents: tabs with preview/pinned behavior, a visible tab strip, resizable docked workspace tree, recent documents, per-document positions and restored session.
- [x] Source reading: syntax-coloured source view preserving the original text, position-preserving reload and mode switching; configurable external editor command. (The editor executable and `{file}`/`{line}`/`{column}` argument template are configurable.)
- [x] Plain text: literal TXT parsing, automatic/manual encoding selection, paragraph rules, chapter outline, chapter-window layout for large files, background chapter prefetch, previous/next document navigation and TXT typography binding. (UTF-8 and explicit legacy code-page books are indexed and read by chapter.)
- [x] Wide content: overflow indication and horizontal formula panning/full preview, wide tables extending into available margins, and full-size image viewing; the table margin-borrow control is available in the text menu.
- [x] Markdown single-newline policy: global preference and per-document override, preserved across reload.
- [x] Cross-document heading links: retain and decode the fragment, open/reuse the document and jump to the heading.
- [x] Formula copying: copy selected inline/display math with delimiters, including inside paragraphs and table cells.

## Verification and delivery

- [x] Targeted pure-function/parser regression tests cover the implemented behaviors.
- [x] Headless display-list/report checks cover layout and rendering paths; no desktop automation was used.
- [x] `cargo test --workspace --locked` and `cargo clippy --workspace --all-targets --locked -- -D warnings` pass.
- [x] `cargo build --release --locked`, `git diff --check` and the requirement audit pass.
- [x] Completed feature groups are committed independently; no push was performed because delivery authorization was not given.

## Work log

- Baseline inspected: clean worktree at `8e8f5fc`; bidirectional text and native script shaping were already committed.
- Implemented the accepted reader feature set: typography profiles and Book preset, East Asian typography, selectable-text PDF and whole-document PNG export, tabs/workspace/session restoration, source view and external-editor commands, literal TXT chapter windows with prefetch, wide-content interaction, newline policy, cross-document heading links, and formula copying.
- Added source-position maps through front matter removal, TeX delimiter rewriting, definition lists, TXT paragraph merging and table cells. Reloads and source-mode transitions use those maps and preserve an unchanged reading context when text is inserted above the reader.
- Added incremental chapter indexing and window reads for UTF-8, GB18030, Big5, Shift-JIS and EUC-KR books. Large TXT files keep only the active chapter in memory and prefetch the next chapter; legacy code-page offsets remain book-wide after decoding.
- Added the native display-list/PDF path with embedded fonts, Unicode/ActualText runs, RTL glyph coordinates, image XObjects, repeated table headers, continued-footnote markers and formula source text layers.
- Final regression passed on 2026-09-24: workspace tests 146 + 6 + 58 + 72 + 6 + 37 + 8, all Clippy targets with warnings denied, release build, and whitespace checks. The worktree is clean.
- Desktop mouse/window smoke was not run in this headless verification pass; tab clicks, tree resizing and chapter navigation remain useful manual acceptance checks. No antivirus settings were changed.

## Verification boundary

- Delivery is intentionally local: no `git push` was run because push authorization was not provided.
- A future manual smoke pass can exercise native tab/tree pointer interaction and large-TXT chapter switching in a desktop session; these do not replace the passing automated regression suite.
