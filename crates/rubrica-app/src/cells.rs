//! Reading a grid of cells back as the text a spreadsheet pastes.
//!
//! A table is the one block whose copy is not a run of prose. The page's own selection
//! is a range of characters over the lines as they were *drawn*, and a drawn line of a
//! grid is a cell's line, not a row: a cell that wraps, or one the author broke with a
//! `<br>`, is two drawn lines and one cell, and a drag that crosses two columns reaches
//! them in the order the painter laid them down rather than in the order a reader reads
//! them. Both are right for prose and wrong for a grid, so a grid is copied from the
//! cells the layout filed here instead: one field per cell, one line per row, tab
//! between the fields -- which is what a spreadsheet reads, and what Telari's
//! cross-cell selection hands over.
//!
//! Nothing here touches the desktop or the font stack: a grid is read out of the boxes
//! the layout filed, so the same numbers are available to the report as to the window.

use std::ops::Range;

/// One cell of a laid-out grid, as the layout filed it.
///
/// The text is the cell's own, not the drawn lines' concatenation: a wrapped cell is
/// one field and the wrap is where the reader's column happened to end, while a break
/// the author wrote is part of the text and stays one. The rectangle is the whole of
/// the cell -- the column's width by the row's height, not the ink's own extent -- which
/// is what a click inside the cell is answered from and what a selection marks, the way
/// a spreadsheet marks one.
pub struct CellBox {
    pub head: bool,
    pub row: usize,
    pub col: usize,
    /// Left, top, width and height in the same device pixels as [`crate::view::Hot`].
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub text: String,
}

/// A table as a spreadsheet reads it: the header row, then the body rows.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Grid {
    pub head: Vec<String>,
    pub rows: Vec<Vec<String>>,
}

impl Grid {
    /// How many columns the widest row has, which is the width of the rectangle the
    /// whole table is.
    pub fn columns(&self) -> usize {
        self.rows
            .iter()
            .map(|r| r.len())
            .fold(self.head.len(), usize::max)
    }

    /// How many rows the table has, header included when the author wrote one.
    pub fn height(&self) -> usize {
        let body = self.rows.len();
        if self.head.is_empty() {
            body.max(1)
        } else {
            body + 1
        }
    }

    /// One row, header first, padded to [`Grid::columns`] so a rectangle of it is a
    /// rectangle.
    pub fn row(&self, r: usize) -> Vec<String> {
        let n = self.columns();
        let mut out = if r == 0 && !self.head.is_empty() {
            self.head.clone()
        } else {
            let at = r - usize::from(!self.head.is_empty());
            self.rows.get(at).cloned().unwrap_or_default()
        };
        out.resize(n.max(out.len()), String::new());
        out
    }
}

/// Where one grid's boxes are, without reading a character of them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GridSpan {
    pub at: Range<usize>,
    /// Whether the grid has a header row, which is what makes row zero the header.
    pub has_head: bool,
    pub rows: usize,
    pub cols: usize,
}

/// A rectangle of cells, inclusive on every side and counted from zero.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rect {
    pub top: usize,
    pub left: usize,
    pub bottom: usize,
    pub right: usize,
}

impl Rect {
    /// The rectangle two cells name, whichever way round they were given: a drag is a
    /// rectangle from whichever corner the reader happened to start at.
    pub fn between(a: (usize, usize), b: (usize, usize)) -> Rect {
        Rect {
            top: a.0.min(b.0),
            left: a.1.min(b.1),
            bottom: a.0.max(b.0),
            right: a.1.max(b.1),
        }
    }

    pub fn rows(&self) -> usize {
        self.bottom - self.top + 1
    }

    pub fn cols(&self) -> usize {
        self.right - self.left + 1
    }

    pub fn cells(&self) -> usize {
        self.rows() * self.cols()
    }
}

/// The cells a reader has marked in one grid, as the two ends of the drag.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CellRect {
    pub grid: usize,
    /// The cell the drag started from, which does not move.
    pub anchor: (usize, usize),
    /// The cell the pointer has reached.
    pub head: (usize, usize),
}

impl CellRect {
    pub fn rect(&self) -> Rect {
        Rect::between(self.anchor, self.head)
    }
}

/// What a command line asked a table to hand over.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Spec {
    /// Every grid on the page, in the order the document wrote them.
    All,
    /// One grid, whole.
    Table(usize),
    /// One grid, a rectangle of its cells.
    Range(usize, Rect),
}

/// The first cell of a grid: the head of a table's first row.
///
/// Every table the parser reads has a header row, and a body row is never numbered
/// zero, so that cell is where one grid ends and the next begins.
fn opens(b: &CellBox) -> bool {
    b.head && b.row == 0 && b.col == 0
}

/// Where the nth grid's boxes are.
///
/// Walked rather than indexed: the boxes arrive in the order the layout drew them, and
/// a table is a run of them, so the count of grids opened before this one is the index
/// asked for. No text is read, which is what lets a pointer move ask this on every
/// message without building the tables themselves.
pub fn grid_at(boxes: &[CellBox], grid: usize) -> Option<GridSpan> {
    let mut headers = boxes.iter().enumerate().filter(|(_, b)| opens(b)).map(|(i, _)| i);
    let start = match headers.nth(grid) {
        Some(i) => i,
        // No header anywhere: what there is, is one grid. That is a table written
        // without a header row, which the parser never produces and a reader's own
        // document still cannot be ruled out from here.
        None => return (grid == 0 && !boxes.is_empty()).then(|| measure(boxes, 0..boxes.len())),
    };
    let end = headers.next().unwrap_or(boxes.len());
    Some(measure(boxes, start..end))
}

/// How tall and wide a grid's own boxes make it, and whether it has a header.
fn measure(boxes: &[CellBox], at: Range<usize>) -> GridSpan {
    let mut span = GridSpan { at: at.clone(), has_head: false, rows: 1, cols: 0 };
    for b in boxes.iter().take(at.end).skip(at.start) {
        if b.head {
            span.has_head = true;
        }
        // A header is one row, and the body rows follow it: which is why the flag is
        // read as it stands rather than after the walk.
        let row = if b.head { 1 } else { b.row + 1 + usize::from(span.has_head) };
        span.rows = span.rows.max(row);
        span.cols = span.cols.max(b.col + 1);
    }
    span
}

/// Which grid a box belongs to, and that cell's own row and column in it.
pub fn grid_of(boxes: &[CellBox], at: usize) -> Option<(usize, usize, usize)> {
    let mut grid = 0usize;
    let mut has_head = false;
    for (i, b) in boxes.iter().enumerate() {
        if opens(b) {
            if i > 0 {
                grid += 1;
            }
            has_head = true;
        }
        if i == at {
            let row = if b.head { 0 } else { b.row + usize::from(has_head) };
            return Some((grid, row, b.col));
        }
    }
    None
}

/// Group the boxes a layout filed into the tables they spell, with their text.
pub fn grids(boxes: &[CellBox]) -> Vec<Grid> {
    let mut out = Vec::new();
    let mut g = 0usize;
    while let Some(span) = grid_at(boxes, g) {
        out.push(text_of(boxes, &span));
        g += 1;
    }
    out
}

/// The table a grid's own boxes spell.
fn text_of(boxes: &[CellBox], span: &GridSpan) -> Grid {
    let mut grid = Grid::default();
    for b in boxes.iter().take(span.at.end).skip(span.at.start) {
        if b.head {
            grid.head.resize(b.col + 1, String::new());
            if let Some(field) = grid.head.get_mut(b.col) {
                *field = b.text.clone();
            }
        } else {
            grid.rows.resize(b.row + 1, Vec::new());
            if let Some(row) = grid.rows.get_mut(b.row) {
                row.resize(b.col + 1, String::new());
                if let Some(field) = row.get_mut(b.col) {
                    *field = b.text.clone();
                }
            }
        }
    }
    grid
}

/// The cell a point falls in, as the index of the box it was filed as.
///
/// The whole cell rectangle answers rather than the ink: a click in the empty half of a
/// short cell is still a click on that cell, which is the one a reader aiming at a
/// spreadsheet column means.
pub fn cell_at(boxes: &[CellBox], x: f32, y: f32) -> Option<usize> {
    boxes.iter().position(|b| x >= b.x && x <= b.x + b.w && y >= b.y && y <= b.y + b.h)
}

/// A cell's text as a copy hands it over: the objects put back as the Markdown they
/// were written as, the same substitution a prose line's copy makes.
///
/// An image or a formula is one placeholder character in the text the layout sets, and
/// pasting the placeholder into a spreadsheet pastes a box nobody can read.
pub fn copy_text(text: &str, objects: &[rubrica_doc::ObjectSpan]) -> String {
    let mut out = String::with_capacity(text.len());
    let mut next = 0usize;
    for (at, c) in text.char_indices() {
        while let Some(object) = objects.get(next).filter(|o| o.range.start <= at) {
            out.push_str(&object.kind.markdown());
            next += 1;
        }
        if c != '\u{fffc}' {
            out.push(c);
        }
    }
    out
}

/// Read a table, or a rectangle of one, back as tab-separated fields: one line per
/// row, one field per cell, and a field that holds a tab, a break or a quote wrapped
/// in quotes the way every spreadsheet reads them back.
pub fn tsv(grid: &Grid, rect: &Rect) -> String {
    let mut out = String::new();
    for r in rect.top..=rect.bottom {
        let row = grid.row(r);
        for c in rect.left..=rect.right {
            if c > rect.left {
                out.push('\t');
            }
            out.push_str(&quote(row.get(c).map_or("", String::as_str)));
        }
        out.push('\n');
    }
    out
}

/// The fields one grid of boxes hands over, or a rectangle of them.
pub fn rect_text(boxes: &[CellBox], grid: usize, rect: &Rect) -> Option<String> {
    let span = grid_at(boxes, grid)?;
    Some(tsv(&text_of(boxes, &span), rect))
}

/// One field, quoted if it would otherwise be read as more than one.
fn quote(field: &str) -> String {
    if !field.contains(['\t', '\n', '\r', '"']) {
        return field.to_string();
    }
    let mut out = String::with_capacity(field.len() + 2);
    out.push('"');
    for c in field.chars() {
        if c == '"' {
            out.push('"');
        }
        out.push(c);
    }
    out.push('"');
    out
}

/// A column's name the way a spreadsheet writes it: `A`, `Z`, `AA`, `AB`.
pub fn column_label(col: usize) -> String {
    let mut out = Vec::new();
    let mut n = col + 1;
    while n > 0 {
        let digit = (n - 1) % 26;
        out.push(char::from(b'A' + digit as u8));
        n = (n - 1) / 26;
    }
    out.reverse();
    out.into_iter().collect()
}

/// The cells a rectangle names, written the way the command line takes them: `B2` for
/// one cell, `B2:D3` for a rectangle.
pub fn label(rect: &Rect) -> String {
    let from = format!("{}{}", column_label(rect.left), rect.top + 1);
    if rect.cells() == 1 {
        return from;
    }
    format!("{from}:{}{}", column_label(rect.right), rect.bottom + 1)
}

/// What a command line said about which table to hand over.
pub fn parse_spec(text: &str) -> Result<Spec, String> {
    let text = text.trim();
    if text.is_empty() {
        return Err("--table-select wants `all`, a table number, or `N:B2:D3`".into());
    }
    if text.eq_ignore_ascii_case("all") {
        return Ok(Spec::All);
    }
    let (table, range) = match text.split_once(':') {
        Some((table, range)) => (table, Some(range)),
        None => (text, None),
    };
    let n: usize = table
        .trim()
        .parse()
        .map_err(|_| format!("--table-select: `{table}` is not a table number or `all`"))?;
    if n == 0 {
        return Err("--table-select: tables are counted from 1, so `0` names none".into());
    }
    let Some(range) = range else { return Ok(Spec::Table(n)) };
    let (a, b) = match range.split_once(':') {
        Some((a, b)) => (a, b),
        None => (range, range),
    };
    let a = parse_cell(a)?;
    let b = parse_cell(b)?;
    Ok(Spec::Range(n, Rect::between(a, b)))
}

/// One `B2` of a range, as a zero-based row and column.
fn parse_cell(text: &str) -> Result<(usize, usize), String> {
    let text = text.trim();
    let letters = text.chars().take_while(|c| c.is_ascii_alphabetic()).count();
    let (name, digits) = text.split_at(letters);
    if name.is_empty() || digits.is_empty() || !digits.chars().all(|c| c.is_ascii_digit()) {
        return Err(format!("--table-select: `{text}` is not a cell like `B2`"));
    }
    let mut col = 0usize;
    for c in name.chars() {
        col = col * 26 + (c.to_ascii_uppercase() as usize - 'A' as usize + 1);
    }
    let row: usize = digits
        .parse()
        .map_err(|_| format!("--table-select: `{text}` is not a cell like `B2`"))?;
    if row == 0 {
        return Err(format!("--table-select: `{text}` names no row, which start at 1"));
    }
    Ok((row - 1, col - 1))
}

/// The text a spec names, and the one line that says what was handed over.
///
/// The range checks live here rather than in the parser because they are questions
/// about the document: a spec that is well written can still name a table the page does
/// not have, and answering it with an empty paste would be a copy that silently does
/// nothing.
pub fn selection(grids: &[Grid], spec: Spec) -> Result<(String, String), String> {
    match spec {
        Spec::All => {
            if grids.is_empty() {
                return Err("--table-select: this page has no table".into());
            }
            let mut text = String::new();
            let mut cells = 0usize;
            let mut rows = 0usize;
            for (i, g) in grids.iter().enumerate() {
                if i > 0 {
                    text.push('\n');
                }
                let rect = whole(g);
                text.push_str(&tsv(g, &rect));
                cells += rect.cells();
                rows += rect.rows();
            }
            let what = format!("{} grid(s), all {cells} cell(s) -> {rows} row(s)", grids.len());
            Ok((what, text))
        }
        Spec::Table(n) | Spec::Range(n, _) => {
            let g = grids.get(n - 1).ok_or_else(|| {
                format!(
                    "--table-select: this page has {} table(s), so there is no `{n}`",
                    grids.len()
                )
            })?;
            let rect = match spec {
                Spec::Range(_, rect) => rect,
                _ => whole(g),
            };
            if rect.bottom + 1 > g.height() {
                return Err(format!(
                    "--table-select: table {n} has {} row(s), so `{}` names none",
                    g.height(),
                    label(&rect)
                ));
            }
            if rect.right + 1 > g.columns() {
                return Err(format!(
                    "--table-select: table {n} has {} column(s), so `{}` names none",
                    g.columns(),
                    label(&rect)
                ));
            }
            let what = format!(
                "grid {n} of {}, {}, {} cell(s) -> {} row(s)",
                grids.len(),
                label(&rect),
                rect.cells(),
                rect.rows()
            );
            Ok((what, tsv(g, &rect)))
        }
    }
}

/// Every cell of a grid.
pub fn whole(grid: &Grid) -> Rect {
    Rect { top: 0, left: 0, bottom: grid.height() - 1, right: grid.columns().saturating_sub(1) }
}

/// The rectangles a cell rectangle covers on the page, one per cell in it: a grid's
/// highlight is a rectangle per cell, the way a spreadsheet marks one, rather than one
/// band across the row.
pub fn rects(boxes: &[CellBox], span: &GridSpan, rect: &Rect) -> Vec<(f32, f32, f32, f32)> {
    let mut out = Vec::new();
    for b in boxes.iter().take(span.at.end).skip(span.at.start) {
        let row = if b.head { 0 } else { b.row + usize::from(span.has_head) };
        if row < rect.top || row > rect.bottom || b.col < rect.left || b.col > rect.right {
            continue;
        }
        out.push((b.x, b.y, b.w, b.h));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cell(head: bool, row: usize, col: usize, text: &str) -> CellBox {
        // A grid a hundred pixels a column and twenty a row, the header above the body.
        CellBox {
            head,
            row,
            col,
            x: col as f32 * 100.0,
            y: if head { 0.0 } else { (row + 1) as f32 * 20.0 },
            w: 100.0,
            h: 20.0,
            text: text.to_string(),
        }
    }

    fn table() -> Vec<CellBox> {
        vec![
            cell(true, 0, 0, "name"),
            cell(true, 0, 1, "qty"),
            cell(true, 0, 2, "price"),
            cell(false, 0, 0, "bolt"),
            cell(false, 0, 1, "12"),
            cell(false, 0, 2, "0.40"),
            cell(false, 1, 0, "nut"),
            cell(false, 1, 1, "7"),
            cell(false, 1, 2, "0.05"),
        ]
    }

    #[test]
    fn a_tables_boxes_read_back_as_a_grid() {
        let grids = grids(&table());
        assert_eq!(grids.len(), 1);
        assert_eq!(grids[0].head, ["name", "qty", "price"]);
        assert_eq!(grids[0].rows.len(), 2);
        assert_eq!(grids[0].rows[1], ["nut", "7", "0.05"]);
        assert_eq!(grids[0].columns(), 3);
        assert_eq!(grids[0].height(), 3);
    }

    #[test]
    fn a_second_header_starts_the_second_grid() {
        let mut boxes = table();
        let mut second = table();
        second.extend([cell(false, 2, 0, "washer")]);
        second.remove(0);
        second.insert(0, cell(true, 0, 0, "part"));
        boxes.extend(second);
        let grids = grids(&boxes);
        assert_eq!(grids.len(), 2);
        assert_eq!(grids[1].head[0], "part");
    }

    #[test]
    fn a_grids_span_names_its_boxes_without_reading_them() {
        let mut boxes = table();
        boxes.extend(table());
        let first = grid_at(&boxes, 0).expect("the first grid");
        assert_eq!(first.at, 0..9);
        assert!(first.has_head);
        assert_eq!((first.rows, first.cols), (3, 3));
        let second = grid_at(&boxes, 1).expect("the second grid");
        assert_eq!(second.at, 9..18);
        assert_eq!(grid_at(&boxes, 2), None);
        // A grid whose first row is not a header still has one row and one column.
        let bare = vec![cell(false, 0, 0, "only")];
        let span = grid_at(&bare, 0).expect("a grid with no header");
        assert!(!span.has_head);
        assert_eq!((span.rows, span.cols), (1, 1));
        assert_eq!(grids(&bare)[0].height(), 1);
    }

    #[test]
    fn a_box_knows_which_grid_and_cell_it_is() {
        let mut boxes = table();
        boxes.extend(table());
        assert_eq!(grid_of(&boxes, 0), Some((0, 0, 0)));
        assert_eq!(grid_of(&boxes, 4), Some((0, 1, 1)));
        assert_eq!(grid_of(&boxes, 8), Some((0, 2, 2)));
        assert_eq!(grid_of(&boxes, 9), Some((1, 0, 0)));
        assert_eq!(grid_of(&boxes, 17), Some((1, 2, 2)));
        assert_eq!(grid_of(&boxes, 18), None);
    }

    #[test]
    fn a_whole_table_reads_header_first_and_tab_separated() {
        let grids = grids(&table());
        let text = tsv(&grids[0], &whole(&grids[0]));
        assert_eq!(text, "name\tqty\tprice\nbolt\t12\t0.40\nnut\t7\t0.05\n");
        // And the same text read straight off the boxes, which is what the window copies.
        assert_eq!(rect_text(&table(), 0, &whole(&grids[0])).as_deref(), Some(text.as_str()));
        assert_eq!(rect_text(&table(), 1, &Rect::between((0, 0), (0, 0))), None);
    }

    #[test]
    fn a_rectangle_of_cells_keeps_the_shape_it_was_dragged_as() {
        let grids = grids(&table());
        let rect = Rect::between((1, 1), (2, 2));
        assert_eq!(label(&rect), "B2:C3");
        assert_eq!(tsv(&grids[0], &rect), "12\t0.40\n7\t0.05\n");
        // A drag is a rectangle whichever corner it started from.
        assert_eq!(Rect::between((2, 2), (1, 1)), rect);
        assert_eq!(Rect::between((1, 1), (1, 1)).cells(), 1);
        let marked = CellRect { grid: 0, anchor: (2, 2), head: (1, 1) };
        assert_eq!(marked.rect(), rect);
        assert_eq!(CellRect { grid: 0, anchor: (1, 1), head: (1, 1) }.rect(), Rect::between((1, 1), (1, 1)));
    }

    #[test]
    fn a_field_that_holds_a_tab_a_break_or_a_quote_is_quoted() {
        let mut boxes = vec![cell(true, 0, 0, "a\tb"), cell(true, 0, 1, "c\nd")];
        boxes.push(cell(true, 0, 2, "say \"hi\""));
        boxes.push(cell(false, 0, 0, "plain"));
        boxes.push(cell(false, 0, 1, ""));
        boxes.push(cell(false, 0, 2, "  spaced  "));
        let grids = grids(&boxes);
        let text = tsv(&grids[0], &whole(&grids[0]));
        assert_eq!(text, "\"a\tb\"\t\"c\nd\"\t\"say \"\"hi\"\"\"\nplain\t\t  spaced  \n");
        // Three lines, because the quoted field's own break is inside the quotes.
        assert_eq!(text.lines().count(), 3);
    }

    #[test]
    fn a_ragged_row_is_padded_to_the_grids_width() {
        let boxes = vec![
            cell(true, 0, 0, "a"),
            cell(true, 0, 1, "b"),
            cell(true, 0, 2, "c"),
            cell(false, 0, 0, "only"),
        ];
        let grids = grids(&boxes);
        let text = tsv(&grids[0], &whole(&grids[0]));
        assert_eq!(text, "a\tb\tc\nonly\t\t\n");
    }

    #[test]
    fn a_rectangle_past_the_grid_is_refused_by_name() {
        let grids = grids(&table());
        let e = selection(&grids, parse_spec("1:B2:E3").expect("a well written spec"))
            .expect_err("the grid is three columns wide");
        assert!(e.contains("3 column(s)"), "{e}");
        assert!(e.contains("B2:E3"), "{e}");
        let e = selection(&grids, parse_spec("1:A1:A9").expect("a spec"))
            .expect_err("the grid is three rows tall");
        assert!(e.contains("3 row(s)"), "{e}");
        let e = selection(&grids, parse_spec("2").expect("a spec")).expect_err("there is one table");
        assert!(e.contains("1 table(s)"), "{e}");
    }

    #[test]
    fn a_page_with_no_table_hands_over_nothing() {
        let e = selection(&[], Spec::All).expect_err("no tables at all");
        assert!(e.contains("no table"), "{e}");
    }

    #[test]
    fn every_table_at_once_is_blank_line_separated() {
        let mut boxes = table();
        boxes.extend(table());
        let grids = grids(&boxes);
        let (what, text) = selection(&grids, Spec::All).expect("two tables");
        assert_eq!(what, "2 grid(s), all 18 cell(s) -> 6 row(s)");
        assert_eq!(text.matches('\n').count(), 7);
        assert!(text.contains("\n\n"), "{text:?}");
    }

    #[test]
    fn a_spec_reads_all_the_forms_the_command_line_takes() {
        assert_eq!(parse_spec("all"), Ok(Spec::All));
        assert_eq!(parse_spec(" ALL "), Ok(Spec::All));
        assert_eq!(parse_spec("2"), Ok(Spec::Table(2)));
        assert_eq!(parse_spec("1:B2"), Ok(Spec::Range(1, Rect::between((1, 1), (1, 1)))));
        assert_eq!(
            parse_spec("1:B2:D3"),
            Ok(Spec::Range(1, Rect { top: 1, left: 1, bottom: 2, right: 3 }))
        );
        // A range written backwards is the same rectangle.
        assert_eq!(parse_spec("1:D3:B2"), parse_spec("1:B2:D3"));
        // And the columns count past Z the way a spreadsheet does.
        assert_eq!(
            parse_spec("1:AA1:AB2"),
            Ok(Spec::Range(1, Rect { top: 0, left: 26, bottom: 1, right: 27 }))
        );
    }

    #[test]
    fn a_spec_that_names_nothing_says_why() {
        for bad in ["", " ", "zero", "0", "1:B", "1:2", "1:B2:", "1::B2", "1:B0", "1:1B2"] {
            assert!(parse_spec(bad).is_err(), "`{bad}` was taken");
        }
        assert!(parse_spec("0").expect_err("zero").contains("counted from 1"));
        assert!(parse_spec("1:B").expect_err("no digits").contains("not a cell"));
        assert!(parse_spec("").expect_err("empty").contains("wants"));
    }

    #[test]
    fn column_labels_count_like_a_spreadsheet() {
        assert_eq!(column_label(0), "A");
        assert_eq!(column_label(25), "Z");
        assert_eq!(column_label(26), "AA");
        assert_eq!(column_label(27), "AB");
        assert_eq!(column_label(51), "AZ");
        assert_eq!(column_label(52), "BA");
        assert_eq!(column_label(701), "ZZ");
        assert_eq!(column_label(702), "AAA");
        assert_eq!(label(&Rect::between((0, 0), (0, 0))), "A1");
        assert_eq!(label(&Rect { top: 2, left: 26, bottom: 2, right: 27 }), "AA3:AB3");
    }

    #[test]
    fn a_cells_objects_come_back_as_the_markdown_they_were_written_as() {
        use rubrica_doc::{ObjectKind, ObjectSpan};
        let text = "the \u{fffc} of \u{fffc}";
        let objects = [
            ObjectSpan { range: 4..7, kind: ObjectKind::Math { source: "x^2".into(), display: false } },
            ObjectSpan {
                range: 11..14,
                kind: ObjectKind::Image { src: "a.png".into(), alt: "a".into(), html: false, zoom: None },
            },
        ];
        let out = copy_text(text, &objects);
        assert!(!out.contains('\u{fffc}'), "{out}");
        assert!(out.starts_with("the "), "{out}");
        assert!(out.contains("x^2"), "{out}");
        assert!(out.contains("a.png"), "{out}");
        assert_eq!(copy_text("plain", &[]), "plain");
    }

    #[test]
    fn a_point_is_answered_by_the_cell_it_falls_in() {
        let boxes = table();
        // The header's middle column, by its own rectangle rather than by its ink.
        assert_eq!(cell_at(&boxes, 150.0, 10.0), Some(1));
        assert_eq!(cell_at(&boxes, 250.0, 10.0), Some(2));
        // The first body row, first column, and the second one below it.
        assert_eq!(cell_at(&boxes, 10.0, 25.0), Some(3));
        assert_eq!(cell_at(&boxes, 10.0, 45.0), Some(6));
        // Past the grid's own edge, and above its top.
        assert_eq!(cell_at(&boxes, 350.0, 25.0), None);
        assert_eq!(cell_at(&boxes, 10.0, -1.0), None);
    }

    #[test]
    fn a_rectangle_of_cells_marks_one_rectangle_each() {
        let boxes = table();
        let span = grid_at(&boxes, 0).expect("one grid");
        let rect = Rect::between((1, 0), (2, 2));
        let marks = rects(&boxes, &span, &rect);
        assert_eq!(marks.len(), 6);
        // Left, top, width and height of the first cell of the rectangle.
        assert_eq!(marks[0], (0.0, 20.0, 100.0, 20.0));
        assert_eq!(marks[5], (200.0, 40.0, 100.0, 20.0));
        // The header is outside it.
        assert!(marks.iter().all(|(_, y, _, _)| *y >= 20.0), "{marks:?}");
        assert!(rects(&boxes, &span, &Rect::between((0, 0), (0, 0))).iter().all(|(_, y, _, _)| *y == 0.0));
    }
}
