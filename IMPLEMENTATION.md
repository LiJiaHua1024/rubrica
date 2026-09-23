# Reader feature implementation

Scope: implement the complete feature gap review accepted on 2026-09-23. Preserve native DirectWrite/Direct2D rendering, whole-paragraph line breaking, OpenType MATH layout, and the completed bidirectional text support.

## Acceptance checklist

- [ ] Typography profiles: editable Latin, Chinese, Japanese, Korean, emphasis, code and math fonts; font size, leading, tracking, paragraph spacing and indentation; named saved profiles and a book preset; settings UI and persistence.
- [ ] East Asian typography: punctuation compression and hanging policies, narrow-column ragged fallback, language-aware Japanese and Korean font selection and Korean keep-all breaking.
- [ ] Export: whole-document PNG with width/scale controls and native selectable-text PDF; pagination with heading/paragraph protection, repeated table headers and continued footnotes; reading typography and colours retained.
- [ ] Multiple documents: tabs with preview/pinned behavior, workspace tree, recent documents, per-document positions and restored session.
- [ ] Source reading: syntax-coloured source view preserving the original text, position-preserving reload and mode switching; configurable external editor command.
- [ ] Plain text: literal TXT parsing, automatic/manual encoding selection, paragraph rules, chapter outline, chapter-window layout for large files, previous/next document navigation and TXT typography binding.
- [ ] Wide content: overflow indication and horizontal formula panning/full preview, wide tables extending into available margins with alignment/width controls, full-size image viewing.
- [ ] Markdown single-newline policy: global preference and per-document override, preserved across reload.
- [ ] Cross-document heading links: retain and decode the fragment, open/reuse the document and jump to the heading.
- [ ] Formula copying: copy selected inline/display math with delimiters, including inside paragraphs and table cells.

## Verification and delivery

- [ ] Targeted pure-function/parser regression tests for each behavior.
- [ ] Headless display-list/report checks for layout and rendering paths; no desktop automation.
- [ ] `cargo test --workspace` and `cargo clippy --workspace --all-targets` pass.
- [ ] Release build, documentation and requirement-by-requirement audit.
- [ ] Commit completed feature groups as they are implemented; push at the authorized delivery point.

## Work log

- Baseline inspected: clean worktree at `8e8f5fc`; bidirectional text and native script shaping are already committed.
- Implementation in progress. Unchecked requirements remain part of the objective; a passing subset is not completion.
- Added global/document newline policy, formula source copy mapping (including table cells and list prefixes), decoded cross-document heading targets, literal TXT/chapter parsing, native text decoding, source view and external editor commands.
- Baseline: 281 tests passed. After changes, 58 document tests passed and workspace Clippy passed. Application executable execution was blocked by Kaspersky quarantine, confirmed by the user. Application runtime tests and headless report checks remain pending until the antivirus issue is resolved; no antivirus settings were changed.
- Source-mode scroll mapping, full session persistence, chapter-window layout and settings UI are still incomplete. The added commands are foundations for those requirements, not their acceptance.
- Per-document persistence now restores explicit encoding, format, source mode, paragraph rules, chapter detection, newline override and reading anchor. Application tests for preference round trips compile but cannot be executed while the application test binary is quarantined.
- Added Korean word-preserving segmentation and narrow-column ragged alignment. The new Korean/style-boundary regression exposed and fixed ragged scoring for single-word lines without internal glue. All 40 typesetting and bidi tests pass; workspace Clippy passes. Language font choices and punctuation policies remain outstanding.
- Added distinct Japanese/Korean families, paragraph-context Han selection and upright East Asian emphasis. Named typography profiles and a Book preset now persist fonts and eight numeric parameters; the modeless native editor saves/applies profiles and can bind TXT documents to a profile. Zoom uses each profile's design size. Application compilation covers this path; window/DPI behavior and application regression execution still require verification. Punctuation policies remain outstanding.
- Added original-source position maps through front matter removal, TeX delimiter rewriting, definition lists, TXT paragraph merging and table cells. Source-mode transitions and reloads now use these maps; reloads also relocate an unchanged text context when text was inserted above the reader. The 60 document tests pass, including source mapping regressions; all 40 typesetting tests passed in the previous validation run. Latest workspace Clippy and diff checks pass. Application execution remains unverified.
- The user stopped new feature work and requested only completion of the existing commits. Existing code was split into document parsing/source maps, Korean line breaking, decoding/preferences, typography profiles, and native reader integration commits. No further feature development is authorized until the user resumes it.

## Remaining work when implementation resumes

- Multiple tabs, preview/pinned behavior, workspace tree, recent-document UI and complete session restoration.
- Selectable-text PDF and whole-document PNG export, including pagination policies and export controls.
- Large TXT chapter-window layout and navigation across lazily laid-out chapters.
- Punctuation compression/hanging and overflow interactions for formulas, tables and full-size images.
- Settings-window DPI behavior, source mapping edge cases and application/runtime regression verification.
- Final release build, documentation, full requirement audit and delivery push.
