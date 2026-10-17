# Table rules and inline `code`

A table can be ruled two ways, and an inline code span can be set two ways. Both
are the reader's own choices: they say nothing about the text, so switching one
for the other cannot move a row, a column, or the reader's place on the page.

## The grid, or the printed table's three rules

With the full grid every boundary of the table is drawn -- a rule under each row
and a rule between each column. With the printed table's three rules only the top
of the table, the line under its header, and its foot are ruled, and the columns
stand apart by their own spacing. Both sets of lines stand on the same
coordinates, so the two are the same page seen twice.

| command | what it draws |
| --- | --- |
| `--tables grid` | a rule under every row and between every column |
| `--tables booktabs` | a rule above, a rule under the header, and a rule below |
| `--inline-code plain` | code as the monospaced face and its own ink |
| `--inline-code capsule` | the same span on a box of its own |

## An inline code span

An inline code span is written with backticks: `code` is what a fence's own text
is called when it stands in a sentence. It may head a paragraph -- `so` does --
and it may end one, as the next word shows: end. A span may carry `two words`, and
two spans may stand apart, as `first` and `second` do, without the words between
them being pulled into a box. A long enough span wraps across the line like any
other text, `so that a single span carried past the measure draws one box on each
line it crosses`, and a heading holds one too.

### A heading with `code` in it

Code inside a table cell is read the same way:

| what | how |
| --- | --- |
| cell | `x` and `y` are two spans |
| cell | `wrapped code in a cell wraps with the cell` |

定义列表里的行内代码
: 释义里写 `code` 也照样读，中文与 `代码` 之间不留空格。

## Not every span is a code span, and a fence is not a span

A fence's text is set as a block and never wears a capsule:

```
`a` and `b` stay as written inside a fence
```

## A table long enough to be cut by a page

`--pdf-height 300` cuts this table across several pages. Every segment the page
shows has to carry a complete frame of its own: the top edge and the header rule
the continuation page redraws, and the closing rule at the foot of each cut.

| row | column one | column two |
| --- | --- | --- |
| 01 | a row that fills the first line of the grid | and one beside it |
| 02 | a row that fills the first line of the grid | and one beside it |
| 03 | a row that fills the first line of the grid | and one beside it |
| 04 | a row that fills the first line of the grid | and one beside it |
| 05 | a row that fills the first line of the grid | and one beside it |
| 06 | a row that fills the first line of the grid | and one beside it |
| 07 | a row that fills the first line of the grid | and one beside it |
| 08 | a row that fills the first line of the grid | and one beside it |
| 09 | a row that fills the first line of the grid | and one beside it |
| 10 | a row that fills the first line of the grid | and one beside it |
| 11 | a row that fills the first line of the grid | and one beside it |
| 12 | a row that fills the first line of the grid | and one beside it |
| 13 | a row that fills the first line of the grid | and one beside it |
| 14 | a row that fills the first line of the grid | and one beside it |
| 15 | a row that fills the first line of the grid | and one beside it |
| 16 | a row that fills the first line of the grid | and one beside it |
| 17 | a row that fills the first line of the grid | and one beside it |
| 18 | a row that fills the first line of the grid | and one beside it |
| 19 | a row that fills the first line of the grid | and one beside it |
| 20 | a row that fills the first line of the grid | and one beside it |
| 21 | a row that fills the first line of the grid | and one beside it |
| 22 | a row that fills the first line of the grid | and one beside it |
| 23 | a row that fills the first line of the grid | and one beside it |
| 24 | a row that fills the first line of the grid | and one beside it |
| 25 | a row that fills the first line of the grid | and one beside it |
| 26 | last row of the long grid | and its last cell |
