//! The cells a scene is drawn in, and how what is drawn in them is written as text.
//!
//! A line through a cell is kept as the sides of the cell it leaves by, so that lines that
//! meet or cross in a cell are written as one character that joins them, as "┬" or "┼". A
//! character drawn in a cell, a letter or a marker, takes the cell's place in the text.

use unicode_width::UnicodeWidthChar;

use crate::Charset;

const UP: u8 = 1;
const DOWN: u8 = 2;
const LEFT: u8 = 4;
const RIGHT: u8 = 8;

/// How a line is drawn.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Stroke {
    /// Dashed or dotted.
    pub(crate) dashed: bool,
    /// Thicker than most.
    pub(crate) heavy: bool,
    /// A shape's outline, not an edge's line.
    pub(crate) outline: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Cell {
    // The sides lines leave the cell by.
    lines: u8,
    dashed: bool,
    heavy: bool,
    // Whether the corner the lines turn is rounded.
    round: bool,
    // The sides the lines of shapes' outlines leave the cell by.
    outline: u8,
    // A character drawn in the cell, which is written in place of its lines.
    glyph: Option<char>,
    // Whether the cell is the second of a character two cells wide.
    tail: bool,
}

/// A grid of cells, by column and row from the top left.
///
/// The grid starts as the cells a scene is laid out in, each where the scene puts it, and
/// may then grow to make room for text: by columns at either side, and by rows put in
/// between those of the scene. A cell of the scene is found by [`Grid::at`].
#[derive(Clone)]
pub(crate) struct Grid {
    cols: usize,
    rows: usize,
    cells: Vec<Cell>,
    // The column of the scene the first column is.
    left: i64,
    // The row of the scene the first row is.
    top: i64,
    // The rows of the scene a row was put in before, once for each.
    put_in: Vec<i64>,
    // The first and last rows of each text of more than a line set in the grid, which a row
    // put in between would split.
    blocks: Vec<(i64, i64)>,
}

impl Grid {
    pub(crate) fn new(cols: usize, rows: usize) -> Grid {
        Grid {
            cols,
            rows,
            cells: vec![Cell::default(); cols * rows],
            left: 0,
            top: 0,
            put_in: Vec::new(),
            blocks: Vec::new(),
        }
    }

    /// Where a cell of the scene is in the grid, once it has grown.
    pub(crate) fn at(&self, (c, r): (i64, i64)) -> (i64, i64) {
        let before = self.put_in.iter().filter(|&&p| p <= r).count() as i64;
        (c - self.left, r - self.top + before)
    }

    /// Keeps the rows a text of more than a line is set in, from the first to the last.
    pub(crate) fn block(&mut self, first: i64, last: i64) {
        if last > first {
            self.blocks.push((first, last));
        }
    }

    /// How many texts of more than a line a row put in before a row of the scene would
    /// split.
    pub(crate) fn splits(&self, before: i64) -> usize {
        let r = self.at((0, before)).1;
        self.blocks
            .iter()
            .filter(|&&(a, b)| a < r && r <= b)
            .count()
    }

    /// Puts a row in before a row of the scene, and after any put in there already. Lines
    /// down that pass from the row above it into the row below pass through it, as do those
    /// that end in a character set over them there, as an arrow at a box's side.
    pub(crate) fn put_row_in(&mut self, before: i64) {
        let r = self.at((0, before)).1.clamp(0, self.rows as i64) as usize;
        let row: Vec<Cell> = (0..self.cols)
            .map(|c| {
                let above = (r > 0).then(|| self.cells[(r - 1) * self.cols + c]);
                let below = (r < self.rows).then(|| self.cells[r * self.cols + c]);
                let down = |a: Cell| Cell {
                    lines: UP | DOWN,
                    dashed: a.dashed,
                    heavy: a.heavy,
                    ..Cell::default()
                };
                match (above, below) {
                    (Some(a), Some(b)) if a.lines & DOWN != 0 && b.lines & UP != 0 => down(a),
                    (Some(a), Some(b)) if a.lines & DOWN != 0 && b.glyph.is_some() => down(a),
                    (Some(a), Some(b)) if a.glyph.is_some() && b.lines & UP != 0 => down(b),
                    _ => Cell::default(),
                }
            })
            .collect();
        self.cells.splice(r * self.cols..r * self.cols, row);
        self.rows += 1;
        self.put_in.push(before);
        let r = r as i64;
        for (a, b) in &mut self.blocks {
            *a += (*a >= r) as i64;
            *b += (*b >= r) as i64;
        }
    }

    /// Grows the grid by columns or rows at its sides, so that the cells from one to n
    /// across from it are in it, and says where that cell is then.
    pub(crate) fn fit(&mut self, (c, r): (i64, i64), n: i64) -> (i64, i64) {
        let (l, t) = ((-c).max(0), (-r).max(0));
        let right = (c + n - self.cols as i64).max(0);
        let bottom = (r + 1 - self.rows as i64).max(0);
        if l + right + t + bottom == 0 {
            return (c, r);
        }
        let cols = self.cols + (l + right) as usize;
        let rows = self.rows + (t + bottom) as usize;
        let mut cells = vec![Cell::default(); cols * rows];
        for (i, row) in self.cells.chunks(self.cols.max(1)).enumerate() {
            let at = (i + t as usize) * cols + l as usize;
            cells[at..at + row.len()].copy_from_slice(row);
        }
        *self = Grid {
            cols,
            rows,
            cells,
            left: self.left - l,
            top: self.top - t,
            put_in: std::mem::take(&mut self.put_in),
            blocks: self.blocks.iter().map(|&(a, b)| (a + t, b + t)).collect(),
        };
        (c + l, r + t)
    }

    /// Whether text may be set in the cells from one to n across from it: whether each is
    /// empty, or has only lines down in it, which text may break or end. Where it may break
    /// any line, whether each has no text.
    pub(crate) fn free(&self, (c, r): (i64, i64), n: i64, over_lines: bool) -> bool {
        (c..c + n).all(|c| {
            if c < 0 || r < 0 || c as usize >= self.cols || r as usize >= self.rows {
                return true;
            }
            let cell = &self.cells[r as usize * self.cols + c as usize];
            cell.glyph.is_none() && !cell.tail && (over_lines || cell.lines & (LEFT | RIGHT) == 0)
        })
    }

    fn cell(&mut self, (c, r): (i64, i64)) -> Option<&mut Cell> {
        if c < 0 || r < 0 || c as usize >= self.cols || r as usize >= self.rows {
            return None;
        }
        Some(&mut self.cells[r as usize * self.cols + c as usize])
    }

    /// Joins two cells side by side or one above the other with a line.
    fn link(&mut self, a: (i64, i64), b: (i64, i64), s: Stroke) {
        let (from, to) = match (b.0 - a.0, b.1 - a.1) {
            (1, 0) => (RIGHT, LEFT),
            (-1, 0) => (LEFT, RIGHT),
            (0, 1) => (DOWN, UP),
            (0, -1) => (UP, DOWN),
            _ => return,
        };
        for (at, side) in [(a, from), (b, to)] {
            if let Some(cell) = self.cell(at) {
                if cell.glyph.is_some() || cell.tail {
                    *cell = Cell::default();
                }
                cell.lines |= side;
                cell.dashed |= s.dashed;
                cell.heavy |= s.heavy;
                if s.outline {
                    cell.outline |= side;
                }
            }
        }
    }

    /// Draws a line from one cell to another, straight where they share a row or a column,
    /// and otherwise in steps across and down that keep as near the straight line as cells
    /// can.
    pub(crate) fn line(&mut self, a: (i64, i64), b: (i64, i64), s: Stroke) {
        let (nx, ny) = ((b.0 - a.0).abs(), (b.1 - a.1).abs());
        let (sx, sy) = ((b.0 - a.0).signum(), (b.1 - a.1).signum());
        let (mut ix, mut iy) = (0, 0);
        let mut at = a;
        while ix < nx || iy < ny {
            // Steps across while the line has gone further down than across, so far.
            let across = iy == ny
                || (ix < nx && (2 * ix + 1) as f64 / nx as f64 <= (2 * iy + 1) as f64 / ny as f64);
            let next = if across {
                ix += 1;
                (at.0 + sx, at.1)
            } else {
                iy += 1;
                (at.0, at.1 + sy)
            };
            self.link(at, next, s);
            at = next;
        }
    }

    /// Takes away the line that leaves a cell toward the next one a way, a step across or
    /// down.
    pub(crate) fn cut(&mut self, at: (i64, i64), (dc, dr): (i64, i64)) {
        let side = match (dc, dr) {
            (1, 0) => RIGHT,
            (-1, 0) => LEFT,
            (0, 1) => DOWN,
            (0, -1) => UP,
            _ => return,
        };
        if let Some(cell) = self.cell(at) {
            cell.lines &= !side;
            cell.outline &= !side;
        }
    }

    /// Whether a shape's outline runs through a cell the same way as a line from it to the
    /// next cell, across or down.
    pub(crate) fn along(&self, (c, r): (i64, i64), across: bool) -> bool {
        if c < 0 || r < 0 || c as usize >= self.cols || r as usize >= self.rows {
            return false;
        }
        let cell = &self.cells[r as usize * self.cols + c as usize];
        cell.outline & if across { LEFT | RIGHT } else { UP | DOWN } != 0
    }

    /// Rounds the corner the lines in a cell turn.
    pub(crate) fn round(&mut self, at: (i64, i64)) {
        if let Some(cell) = self.cell(at) {
            cell.round = true;
        }
    }

    /// The character drawn in a cell, if any.
    pub(crate) fn glyph(&self, (c, r): (i64, i64)) -> Option<char> {
        if c < 0 || r < 0 || c as usize >= self.cols || r as usize >= self.rows {
            return None;
        }
        self.cells[r as usize * self.cols + c as usize].glyph
    }

    /// Empties a cell.
    pub(crate) fn clear(&mut self, at: (i64, i64)) {
        if let Some(cell) = self.cell(at) {
            *cell = Cell::default();
        }
    }

    /// Draws a character in a cell, and in the next one too where it is two cells wide. It
    /// returns how many cells it took.
    pub(crate) fn put(&mut self, at: (i64, i64), ch: char) -> i64 {
        let w = ch.width().unwrap_or(0) as i64;
        if w == 0 {
            return 0;
        }
        // A character whose second half another one covers is lost with it.
        if let Some(cell) = self.cell(at)
            && cell.tail
        {
            *cell = Cell::default();
            self.clear((at.0 - 1, at.1));
        }
        if let Some(cell) = self.cell(at) {
            *cell = Cell {
                glyph: Some(ch),
                ..Cell::default()
            };
        }
        if w == 2 {
            let next = (at.0 + 1, at.1);
            if let Some(cell) = self.cell(next) {
                *cell = Cell {
                    tail: true,
                    ..Cell::default()
                };
            }
            // And a character whose first half is covered.
            if let Some(cell) = self.cell((at.0 + 2, at.1))
                && cell.tail
            {
                *cell = Cell::default();
            }
        }
        w
    }

    /// Draws a string from a cell across.
    pub(crate) fn text(&mut self, at: (i64, i64), s: &str) {
        let mut c = at.0;
        for ch in s.chars() {
            c += self.put((c, at.1), ch);
        }
    }

    /// Cuts the empty rows and columns about the grid away, and each run of rows that
    /// nothing but straight lines down passes through, and of columns that nothing but
    /// straight lines across passes through, to one. Nothing comes apart, as lines that
    /// only pass through such a row or column are shortened by it, and nothing else is in
    /// it. It is how text renderers of diagrams space them, a cell between things.
    pub(crate) fn squeeze(&mut self) {
        let cell = |g: &Grid, c: usize, r: usize| g.cells[r * g.cols + c];
        let blank = |x: Cell| x == Cell::default();
        let through =
            |x: Cell, way: u8| x.glyph.is_none() && !x.tail && (x.lines == 0 || x.lines == way);
        let rows: Vec<usize> = keep(
            (0..self.rows).map(|r| (0..self.cols).all(|c| blank(cell(self, c, r)))),
            (0..self.rows).map(|r| (0..self.cols).all(|c| through(cell(self, c, r), UP | DOWN))),
        );
        let cols: Vec<usize> = keep(
            (0..self.cols).map(|c| (0..self.rows).all(|r| blank(cell(self, c, r)))),
            (0..self.cols).map(|c| (0..self.rows).all(|r| through(cell(self, c, r), LEFT | RIGHT))),
        );
        let mut cells = Vec::with_capacity(rows.len() * cols.len());
        for &r in &rows {
            for &c in &cols {
                cells.push(cell(self, c, r));
            }
        }
        *self = Grid {
            cols: cols.len(),
            rows: rows.len(),
            cells,
            ..Grid::new(0, 0)
        };
    }

    /// The grid as text, a line for each row, without the spaces that end a row or the rows
    /// that end the grid empty.
    pub(crate) fn write(&self, charset: Charset) -> String {
        let mut out = String::new();
        for row in self.cells.chunks(self.cols.max(1)) {
            let mut line = String::new();
            for cell in row {
                if cell.tail {
                    continue;
                }
                line.push(cell.glyph.unwrap_or_else(|| lines(cell, charset)));
            }
            out.push_str(line.trim_end());
            out.push('\n');
        }
        let end = out.trim_end().len();
        out.truncate(end);
        out.push('\n');
        out
    }
}

/// The rows or columns kept of a grid's: none of those that are blank at either end, and
/// the first of each run of those that may go.
fn keep(blank: impl Iterator<Item = bool>, may_go: impl Iterator<Item = bool>) -> Vec<usize> {
    let blank: Vec<bool> = blank.collect();
    let may_go: Vec<bool> = may_go.collect();
    let Some(start) = blank.iter().position(|&b| !b) else {
        return Vec::new();
    };
    let end = blank.iter().rposition(|&b| !b).map_or(start, |e| e + 1);
    (start..end)
        .filter(|&i| !(may_go[i] && i > start && may_go[i - 1]))
        .collect()
}

/// The character that draws the lines of a cell.
fn lines(cell: &Cell, charset: Charset) -> char {
    let l = cell.lines;
    let across = l & (UP | DOWN) == 0;
    let down = l & (LEFT | RIGHT) == 0;
    if l == 0 {
        return ' ';
    }
    if charset == Charset::Ascii {
        return match () {
            _ if across && cell.dashed => '.',
            _ if across => '-',
            _ if down && cell.dashed => ':',
            _ if down => '|',
            _ => '+',
        };
    }
    if across {
        return match (cell.dashed, cell.heavy) {
            (true, _) => '╌',
            (_, true) => '━',
            _ => '─',
        };
    }
    if down {
        return match (cell.dashed, cell.heavy) {
            (true, _) => '╎',
            (_, true) => '┃',
            _ => '│',
        };
    }
    match (l, cell.round) {
        (x, true) if x == DOWN | RIGHT => '╭',
        (x, true) if x == DOWN | LEFT => '╮',
        (x, true) if x == UP | RIGHT => '╰',
        (x, true) if x == UP | LEFT => '╯',
        (x, _) if x == DOWN | RIGHT => '┌',
        (x, _) if x == DOWN | LEFT => '┐',
        (x, _) if x == UP | RIGHT => '└',
        (x, _) if x == UP | LEFT => '┘',
        (x, _) if x == UP | DOWN | RIGHT => '├',
        (x, _) if x == UP | DOWN | LEFT => '┤',
        (x, _) if x == LEFT | RIGHT | DOWN => '┬',
        (x, _) if x == LEFT | RIGHT | UP => '┴',
        _ => '┼',
    }
}
