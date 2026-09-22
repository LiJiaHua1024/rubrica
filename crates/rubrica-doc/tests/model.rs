use rubrica_doc::{BlockKind, Document, InlineStyle};

fn kinds(src: &str) -> Vec<BlockKind> {
    Document::parse(src).blocks.iter().map(|b| b.kind).collect()
}

#[test]
fn headings_paragraphs_and_code_keep_their_order_and_identity() {
    let src = "# Title\n\nA paragraph.\n\n```rust\nfn main() {}\n```\n\n## Second\n";
    let doc = Document::parse(src);
    assert_eq!(
        kinds(src),
        vec![
            BlockKind::Heading(1),
            BlockKind::Paragraph,
            BlockKind::Code,
            BlockKind::Heading(2),
        ]
    );
    assert_eq!(doc.blocks[0].text, "Title");
    assert_eq!(doc.blocks[2].text, "fn main() {}");
    assert!(doc.blocks[2].ragged(), "code must never be justified");
    assert!(doc.blocks[0].ragged(), "a heading must never be justified");
    assert!(!doc.blocks[1].ragged());
}

#[test]
fn a_fence_keeps_the_language_it_was_opened_with() {
    let code = |src: &str| {
        Document::parse(src)
            .blocks
            .iter()
            .find(|b| b.kind == BlockKind::Code)
            .and_then(|b| b.lang.clone())
            .unwrap_or_default()
    };
    assert_eq!(code("```rust\nfn main() {}\n```"), "rust".to_string());
    // The comma is mdBook's way of saying "not a test" and the brace is
    // Python-Markdown's attributes; neither moves the name.
    assert_eq!(code("```rust,ignore\nfn main() {}\n```"), "rust".to_string());
    assert_eq!(code("```py {#intro .highlight}\n1\n```"), "py".to_string());
    assert_eq!(code("```TypeScript\n1\n```"), "typescript".to_string());
    // Said nothing is not the same as said `text`: a block whose language is
    // unknown is set as the plain source it is.
    assert_eq!(code("```\nhello\n```"), String::new(), "no name, no claim");
    assert_eq!(code("    indented\n"), String::new());
}

#[test]
fn a_soft_break_continues_the_paragraph_rather_than_breaking_the_line() {
    // The whole point of owning Block::text: slicing the source verbatim would
    // hand the engine a newline per wrapped line and every paragraph would set
    // ragged on its own column widths.
    let src = "first line of prose\nsecond line of prose\n\nnew paragraph\n";
    let doc = Document::parse(src);
    assert_eq!(doc.blocks.len(), 2);
    assert_eq!(doc.blocks[0].text, "first line of prose second line of prose");
    assert!(!doc.blocks[0].text.contains('\n'));
}

#[test]
fn a_hard_break_survives_as_a_forced_line_break() {
    let doc = Document::parse("two lines\\\nclose together\n");
    assert_eq!(doc.blocks[0].text, "two lines\nclose together");
}

#[test]
fn a_br_tag_breaks_the_line_like_a_backslash() {
    // The one inline-HTML spelling with typographic meaning -- and the only way to
    // break a line inside a table cell, where a backslash does not work.
    let src = "第一行<br>第二行\n\n甲<br />乙<BR>丙\n\n丙<span class=\"x\">丁</span>戊\n";
    let doc = Document::parse(src);
    assert_eq!(doc.blocks[0].text, "第一行\n第二行");
    assert_eq!(doc.blocks[1].text, "甲\n乙\n丙");
    // Markup that is not a break still leaves no trace of its tags in the text.
    assert_eq!(doc.blocks[2].text, "丙丁戊");
}

#[test]
fn inline_styles_become_spans_over_the_block_text() {
    let src = "plain **bold** *it* `code` ~~gone~~ [link](https://example.com)";
    let doc = Document::parse(src);
    let b = &doc.blocks[0];
    assert_eq!(b.text, "plain bold it code gone link");
    let named: Vec<(&str, InlineStyle)> = b
        .spans
        .iter()
        .map(|s| (&b.text[s.range.clone()], s.style))
        .collect();
    // pulldown hands back a word together with its trailing space, so compare the
    // trimmed content: the space riding inside a span is harmless, because the
    // layout engine peels whitespace off a node before resolving its style.
    let has = |w: &str, f: InlineStyle| {
        named.iter().any(|(t, s)| t.trim() == w && s.contains(f))
    };
    assert!(has("bold", InlineStyle::STRONG), "{named:?}");
    assert!(has("it", InlineStyle::EMPHASIS), "{named:?}");
    assert!(has("code", InlineStyle::CODE), "{named:?}");
    assert!(has("gone", InlineStyle::STRIKETHROUGH), "{named:?}");
    assert!(has("link", InlineStyle::LINK), "{named:?}");
    assert!(has("plain", InlineStyle::EMPTY), "{named:?}");
    // Spans must tile the text with no gaps, or a glyph goes unpainted.
    let mut cursor = 0usize;
    for s in &b.spans {
        assert_eq!(s.range.start, cursor, "gap before {:?}", &b.text[s.range.clone()]);
        cursor = s.range.end;
    }
    assert_eq!(cursor, b.text.len(), "text after the last span");
}

#[test]
fn emphasis_nesting_combines_both_flags() {
    let doc = Document::parse("_a **b** c_\n");
    let b = &doc.blocks[0];
    let bold = b.spans.iter().find(|s| b.text[s.range.clone()].trim() == "b").unwrap();
    assert!(bold.style.contains(InlineStyle::STRONG | InlineStyle::EMPHASIS));
    let plain = b.spans.iter().find(|s| b.text[s.range.clone()].trim() == "a").unwrap();
    assert!(plain.style.contains(InlineStyle::EMPHASIS));
    assert!(!plain.style.contains(InlineStyle::STRONG));
}

#[test]
fn list_items_carry_markers_and_nesting_depth() {
    // The nested bullet must sit *inside* an item: two spaces of indent after a
    // blank line is a new top-level list to CommonMark, not a child.
    let src = "- one\n  - nested\n- two\n\n1. first\n2. second\n";
    let doc = Document::parse(src);
    let items: Vec<_> = doc.blocks.iter().filter_map(|b| b.list).collect();
    assert_eq!(items.len(), 5, "{items:?}");
    assert_eq!(items[0].index, None, "bullet");
    assert!(!items[0].ordered);
    assert_eq!(items[1].depth, 1, "nested list must be one deeper, got {items:?}");
    assert_eq!(items[3].index, Some(1));
    assert_eq!(items[4].index, Some(2));
    assert!(items[4].ordered);
}

/// What a reader sees if it is not recorded: an item's second paragraph and its code
/// fence standing out at the page margin, beside the marker instead of under the text
/// they go on with. Only the block that opens an item carries a marker, so the level
/// has to be said again for the ones that follow.
#[test]
fn a_loose_item_marks_the_blocks_that_continue_it() {
    let src = "intro\n\n- one\n\n  two\n\n  ```sh\n  code\n  ```\n\n- next\n  - deep\n\n    deeper\n\nafter\n";
    let doc = Document::parse(src);
    let seen: Vec<(String, Option<u8>, Option<u8>)> = doc
        .blocks
        .iter()
        .map(|b| {
            (
                b.text.clone(),
                b.list.map(|l| l.depth),
                b.item_depth,
            )
        })
        .collect();
    let at = |t: &str| {
        seen.iter()
            .find(|(text, _, _)| text.contains(t))
            .unwrap_or_else(|| panic!("no block carrying {t:?} in {seen:?}"))
    };
    assert_eq!(at("intro").2, None, "prose outside any list");
    let one = at("one");
    assert_eq!((one.1, one.2), (Some(0), Some(0)), "the item's own block carries both");
    let two = at("two");
    assert_eq!((two.1, two.2), (None, Some(0)), "its second paragraph continues the item");
    let code = at("code");
    assert_eq!((code.1, code.2), (None, Some(0)), "and so does its fence");
    assert_eq!(code.0, "code", "a fence is its own block, not the item's prose");
    let deep = at("deep");
    assert_eq!((deep.1, deep.2), (Some(1), Some(1)), "a nested item is one level deeper");
    assert_eq!(at("deeper").2, Some(1), "and so is what continues it");
    assert_eq!(at("after").2, None, "the count unwinds with the items");
}

#[test]
fn task_list_items_record_their_checked_state() {
    let doc = Document::parse("- [x] done\n- [ ] todo\n");
    assert_eq!(doc.blocks[0].task, Some(true));
    assert_eq!(doc.blocks[1].task, Some(false));
}

#[test]
fn block_quote_depth_is_recorded_per_block() {
    let src = "> quoted\n>\n> > deeper\n";
    let doc = Document::parse(src);
    assert_eq!(doc.blocks[0].quote_depth, 1);
    assert_eq!(doc.blocks[1].quote_depth, 2);
}

#[test]
fn thematic_breaks_become_rule_blocks() {
    let doc = Document::parse("above\n\n---\n\nbelow\n");
    assert_eq!(kinds("above\n\n---\n\nbelow\n"), vec![
        BlockKind::Paragraph,
        BlockKind::Rule,
        BlockKind::Paragraph
    ]);
    assert!(doc.blocks[1].text.is_empty());
}

#[test]
fn mixed_cjk_and_latin_survives_the_parser() {
    let doc = Document::parse("使用Rust实现**排版**引擎\n");
    let b = &doc.blocks[0];
    assert_eq!(b.text, "使用Rust实现排版引擎");
    assert!(b.spans.iter().any(|s| s.style.contains(InlineStyle::STRONG)));
}

#[test]
fn empty_and_whitespace_only_input_produce_nothing() {
    assert!(Document::parse("").blocks.is_empty());
    assert!(Document::parse("\n\n   \n").blocks.is_empty());
    assert!(Document::parse("<!-- only a comment -->\n").blocks.is_empty());
}

#[test]
fn an_image_becomes_one_object_box_not_prose() {
    let doc = Document::parse("see ![the diagram](img/a.png) inline\n");
    let b = &doc.blocks[0];
    // The alt text must not be set as running prose.
    assert!(!b.text.contains("the diagram"), "alt text leaked into prose: {:?}", b.text);
    assert_eq!(b.objects.len(), 1, "{b:?}");
    let o = &b.objects[0];
    assert_eq!(&b.text[o.range.clone()], "\u{FFFC}");
    let rubrica_doc::ObjectKind::Image { src, alt } = &o.kind else {
        panic!("the image became {:?}", o.kind)
    };
    assert_eq!(src, "img/a.png");
    assert_eq!(alt, "the diagram");
    let sp = b.spans.iter().find(|s| s.range == o.range).expect("no span for the object");
    assert!(sp.style.contains(InlineStyle::OBJECT), "span style: {:?}", sp.style);
    // The placeholder is one node, so surrounding prose still justifies around it.
    assert_eq!(b.spans.len(), 3, "{:?}", b.spans.iter().map(|s| &b.text[s.range.clone()]).collect::<Vec<_>>());
}

#[test]
fn an_image_alone_forms_its_own_block() {
    let doc = Document::parse("![only](x.png)\n");
    assert_eq!(doc.blocks.len(), 1);
    assert_eq!(doc.blocks[0].objects.len(), 1);
}

#[test]
fn an_inline_formula_is_an_object_box_not_prose() {
    let doc = Document::parse("for $a^2+b^2$ and more\n");
    let b = &doc.blocks[0];
    assert_eq!(b.objects.len(), 1, "{b:?}");
    let o = &b.objects[0];
    assert_eq!(&b.text[o.range.clone()], "\u{FFFC}");
    let rubrica_doc::ObjectKind::Math { source, display } = &o.kind else {
        panic!("the formula became {:?}", o.kind)
    };
    assert_eq!(source, "a^2+b^2");
    assert!(!display, "a single pair of dollars is the inline form");
    // The source is what the math engine reads, so the placeholder must not also
    // appear in the prose it would otherwise justify around.
    assert!(!b.text.contains('^'), "source leaked into prose: {:?}", b.text);
}

#[test]
fn a_displayed_formula_stands_between_two_paragraphs() {
    let doc = Document::parse("before\n\n$$\\frac{1}{2}$$\n\nafter\n");
    let texts: Vec<&str> = doc.blocks.iter().map(|b| b.text.as_str()).collect();
    assert_eq!(doc.blocks.len(), 3, "{texts:?}");
    assert_eq!(texts[0], "before");
    assert_eq!(texts[2], "after");
    let o = &doc.blocks[1].objects[0];
    let rubrica_doc::ObjectKind::Math { source, display } = &o.kind else {
        panic!("the formula became {:?}", o.kind)
    };
    assert_eq!(source, "\\frac{1}{2}");
    assert!(display, "$$..$$ asks for display layout");
}

#[test]
fn a_gfm_table_becomes_a_grid_not_prose() {
    let src = "| Name | Qty |\n|:-----|----:|\n| apple | 3 |\n| **pear** | 12 |\n";
    let doc = Document::parse(src);
    assert_eq!(doc.blocks.len(), 1, "{:?}", doc.blocks.iter().map(|b| (b.kind, &b.text)).collect::<Vec<_>>());
    let b = &doc.blocks[0];
    assert_eq!(b.kind, BlockKind::Table);
    assert!(b.ragged(), "a table is laid out per cell, not justified");
    let t = b.table.as_ref().expect("no table");
    assert_eq!(t.columns(), 2);
    assert_eq!(t.head.len(), 2);
    assert_eq!(t.head[0].text, "Name");
    assert_eq!(t.rows.len(), 2);
    assert_eq!(t.rows[1][0].text, "pear");
    assert_eq!(t.aligns, vec![rubrica_doc::Align::Left, rubrica_doc::Align::Right]);
    // Inline formatting inside a cell survives as a span over the cell's own text.
    let bold = t.rows[1][0].spans.iter().find(|s| s.style.contains(InlineStyle::STRONG));
    assert!(bold.is_some(), "{:?}", t.rows[1][0]);
    assert_eq!(&t.rows[1][0].text[bold.unwrap().range.clone()], "pear");
}

#[test]
fn a_table_with_cjk_cells_keeps_its_grid() {
    let doc = Document::parse("| 名称 | 数量 |\n|---|---|\n| 苹果 | 三 |\n");
    let t = doc.blocks[0].table.as_ref().expect("no table");
    assert_eq!(t.head[0].text, "名称");
    assert_eq!(t.rows[0][1].text, "三");
}

#[test]
fn a_cells_own_text_holds_its_link_its_citation_and_its_break() {
    // A table block's own text is never drawn, so anything landing there instead of in
    // the cell is text the reader never sees.
    let src = "| a | [b](https://e/x)<br>c[^k] |\n|---|---|\n| d | e |\n\n[^k]: the note\n";
    let b = &Document::parse(src).blocks[0];
    let t = b.table.as_ref().expect("no table");
    assert!(b.text.is_empty(), "the grid keeps its cells: block text is {:?}", b.text);
    assert!(b.actions.is_empty(), "a cell's target is recorded on the cell");

    let cell = &t.head[1];
    assert_eq!(cell.text, "b\nc1", "`<br>` reaches the cell");
    assert_eq!(
        cell.actions.iter().map(|a| (a.range.clone(), a.kind.clone())).collect::<Vec<_>>(),
        vec![
            (0..1, rubrica_doc::ActionKind::Url("https://e/x".into())),
            (3..4, rubrica_doc::ActionKind::Cite("k".into())),
        ]
    );
    let raised: Vec<&str> = cell
        .spans
        .iter()
        .filter(|s| s.style.contains(InlineStyle::SUPERSCRIPT))
        .map(|s| &cell.text[s.range.clone()])
        .collect();
    assert_eq!(raised, vec!["1"]);
}

// --- footnotes -------------------------------------------------------------

/// The spans over a block that carry a given flag, with the text they cover.
fn styled(b: &rubrica_doc::Block, flag: InlineStyle) -> Vec<(&str, usize)> {
    b.spans
        .iter()
        .filter(|s| s.style.contains(flag))
        .map(|s| (&b.text[s.range.clone()], s.range.start))
        .collect()
}

#[test]
fn a_citation_becomes_raised_digits_in_the_citing_block() {
    let doc = Document::parse("One[^a] two.\n\n[^a]: the note\n");
    let b = &doc.blocks[0];
    // No brackets and no inserted space: the style is what separates the mark from
    // the word, and a literal gap would let justification open one out.
    assert_eq!(b.text, "One1 two.");
    assert_eq!(styled(b, InlineStyle::SUPERSCRIPT), vec![("1", 3)]);
    // The digits must be their own span, since nothing else carries the flag.
    let plain: Vec<&str> = b
        .spans
        .iter()
        .filter(|s| !s.style.contains(InlineStyle::SUPERSCRIPT))
        .map(|s| &b.text[s.range.clone()])
        .collect();
    assert_eq!(plain, vec!["One", " two."], "{:?}", b.spans);
}

#[test]
fn citations_are_numbered_in_first_citation_order_and_a_repeat_reuses_its_number() {
    // `b` is defined first but cited second, so definition order must not win.
    let doc = Document::parse("One[^b] two[^a] three[^b].\n\n[^b]: second\n\n[^a]: first\n");
    let nums: Vec<usize> = doc.footnotes.iter().map(|f| f.number).collect();
    let labels: Vec<&str> = doc.footnotes.iter().map(|f| f.label.as_str()).collect();
    assert_eq!(nums, vec![1, 2], "{doc:?}");
    assert_eq!(labels, vec!["b", "a"], "numbering follows the citations, not the definitions");
    assert_eq!(doc.blocks[0].text, "One1 two2 three1.");
    let marks: Vec<&str> = styled(&doc.blocks[0], InlineStyle::SUPERSCRIPT)
        .into_iter()
        .map(|(t, _)| t)
        .collect();
    assert_eq!(marks, vec!["1", "2", "1"], "the repeat citation reuses its number");
}

#[test]
fn definitions_leave_the_page_and_keep_their_own_blocks() {
    let src = "Cite[^a].\n\n[^a]: note body\n\n[^b]: other note\n";
    let doc = Document::parse(src);
    assert_eq!(doc.blocks.len(), 1, "a definition must not orphan into the prose");
    assert_eq!(doc.blocks[0].text, "Cite1.");
    assert_eq!(doc.footnotes.len(), 2);
    assert_eq!(doc.footnotes[0].blocks[0].text, "note body");
    assert_eq!(doc.footnotes[1].blocks[0].text, "other note");
    assert_eq!(doc.footnotes[1].number, 2, "uncited, so numbered after the cited one");
}

#[test]
fn a_definition_keeps_its_heading_and_list_structure() {
    let doc = Document::parse("Cite[^h].\n\n[^h]: # Real heading\n\n    - one\n    - two\n");
    let blocks = &doc.footnotes[0].blocks;
    let kinds: Vec<rubrica_doc::BlockKind> = blocks.iter().map(|b| b.kind).collect();
    assert_eq!(kinds, vec![BlockKind::Heading(1), BlockKind::Paragraph, BlockKind::Paragraph]);
    assert_eq!(blocks[0].text, "Real heading");
    assert_eq!(blocks.iter().skip(1).map(|b| b.list.map(|l| l.depth)).collect::<Vec<_>>(), vec![Some(0), Some(0)]);
}

#[test]
fn a_definition_with_two_paragraphs_keeps_both() {
    let doc = Document::parse("Cite[^n].\n\n[^n]: para one\n\n    para two\n");
    let blocks = &doc.footnotes[0].blocks;
    let texts: Vec<&str> = blocks.iter().map(|b| b.text.as_str()).collect();
    assert_eq!(texts, vec!["para one", "para two"], "{texts:?}");
}

#[test]
fn a_citation_inside_emphasis_stays_emphasised_as_well_as_raised() {
    let doc = Document::parse("word *bold[^a]* tail\n\n[^a]: note\n");
    let b = &doc.blocks[0];
    let raised = b
        .spans
        .iter()
        .find(|s| s.style.contains(InlineStyle::SUPERSCRIPT))
        .expect("no raised span");
    assert!(
        raised.style.contains(InlineStyle::EMPHASIS),
        "the mark lost the emphasis it was cited inside of: {:?}",
        b.spans
    );
}

#[test]
fn a_link_records_one_action_whatever_pieces_its_text_arrives_in() {
    let doc = Document::parse("see the [guide *and its* title](https://example.org/a) now\n");
    let b = &doc.blocks[0];
    assert_eq!(b.actions.len(), 1, "{:?}", b.actions);
    assert_eq!(b.actions[0].kind, rubrica_doc::ActionKind::Url("https://example.org/a".into()));
    assert_eq!(&b.text[b.actions[0].range.clone()], "guide and its title");
}

#[test]
fn actions_name_the_text_a_click_would_act_on() {
    let doc = Document::parse("Read [**this**](https://e/b) or [![fig](shot.png)](https://e/c).[^n]\n\n[^n]: note\n");
    let b = &doc.blocks[0];
    let targets: Vec<(&str, &str)> = b
        .actions
        .iter()
        .map(|a| {
            (
                &b.text[a.range.clone()],
                match &a.kind {
                    rubrica_doc::ActionKind::Url(u) => u.as_str(),
                    rubrica_doc::ActionKind::Cite(l) => l.as_str(),
                },
            )
        })
        .collect();
    // The image's alt text is not prose, so the second target's text is the single
    // placeholder the layout engine will draw a box for.
    assert_eq!(
        targets,
        vec![("this", "https://e/b"), ("\u{FFFC}", "https://e/c"), ("1", "n")]
    );
}

#[test]
fn a_citation_action_points_at_its_number_and_not_at_the_note() {
    let doc = Document::parse("word[^a] more\n\n[^a]: the note\n");
    let b = &doc.blocks[0];
    assert_eq!(b.actions.len(), 1);
    assert_eq!(b.actions[0].kind, rubrica_doc::ActionKind::Cite("a".into()));
    assert_eq!(&b.text[b.actions[0].range.clone()], "1", "must cover the raised number alone");
    // The definition is where a reader lands, not where they clicked, so it carries no
    // action of its own unless the author linked something inside it.
    assert!(doc.footnotes[0].blocks[0].actions.is_empty(), "{:?}", doc.footnotes[0].blocks[0].actions);
}

#[test]
fn a_raw_html_block_keeps_its_words_and_loses_its_markup() {
    // The alternative is the old behaviour, which is a document with a paragraph that
    // only exists as `<p>` tags: an author who wraps prose in a div for one class
    // attribute is not announcing that the prose may be thrown away.
    let doc = Document::parse("<div class=\"note\">\nSee the <b>words</b>.\n</div>\n");
    assert_eq!(doc.blocks.len(), 1, "{:?}", doc.blocks);
    assert_eq!(doc.blocks[0].text, "See the words.");
    assert_eq!(
        doc.blocks[0].spans,
        vec![rubrica_doc::Span { range: 0..14, style: InlineStyle::EMPTY }]
    );
}

#[test]
fn htmls_block_level_tags_end_the_line_they_arrive_on() {
    let src = "<ul><li>one</li><li>two</li></ul>\n";
    let doc = Document::parse(src);
    assert_eq!(doc.blocks.len(), 2, "two items are two lines, not one sentence with a gap");
    assert_eq!(doc.blocks[0].text, "one");
    assert_eq!(doc.blocks[1].text, "two");
}

#[test]
fn a_heading_written_in_html_is_a_heading() {
    // The one element whose meaning survives as more than a line break, because the
    // reader's outline is built from heading blocks and an `<h2>` is an author asking
    // to be in it.
    let doc = Document::parse("<h2>Chapter</h2>\nmore prose\n");
    assert_eq!(kinds("<h2>Chapter</h2>\n"), vec![BlockKind::Heading(2)]);
    assert_eq!(doc.blocks[0].kind, BlockKind::Heading(2));
    assert_eq!(doc.blocks[0].text, "Chapter");
}

#[test]
fn an_img_tag_becomes_the_figure_it_names() {
    let doc = Document::parse("<p><img src=\"shot.png\" alt=\"A shot\"></p>\n");
    let b = &doc.blocks[0];
    assert_eq!(b.text, "\u{FFFC}");
    assert_eq!(
        b.objects,
        vec![rubrica_doc::ObjectSpan {
            range: 0..3,
            kind: rubrica_doc::ObjectKind::Image { src: "shot.png".into(), alt: "A shot".into() }
        }]
    );
}

#[test]
fn a_scripts_words_are_not_prose() {
    let src = "<script>\nvar x = 1;\n</script>\n<p>after</p>\n";
    let doc = Document::parse(src);
    assert_eq!(doc.blocks.len(), 1, "{:?}", doc.blocks);
    assert_eq!(doc.blocks[0].text, "after", "a style rule set in the reader's face is not a document");
}

#[test]
fn character_references_are_resolved_where_no_parser_ever_read_them() {
    let doc = Document::parse("<p>AT&amp;T &#39;quoted&#39; &mdash; &ampund &</p>\n");
    // `&ampund` is not a reference and a bare `&` is a character, so both stay as written:
    // the reader is shown the author's text, not the encoder's output.
    assert_eq!(doc.blocks[0].text, "AT&T 'quoted' — &ampund &");
}

#[test]
fn a_comment_hides_nothing_else_and_a_loose_angle_bracket_stays() {
    let doc = Document::parse("<div><!-- 3 < 4 & 5 --></div>\n<p>x < y</p>\n");
    assert_eq!(doc.blocks.len(), 1, "{:?}", doc.blocks);
    assert_eq!(doc.blocks[0].text, "x < y");
}

