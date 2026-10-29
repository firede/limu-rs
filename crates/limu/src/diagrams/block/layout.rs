//! The blocks of a diagram placed in the grids of their composites, as Pele's layout.ts
//! places them.

use crate::detect::DiagramType;
use crate::errors::{Error, ErrorCode};
use crate::js::math::{max, min};
use crate::js::number::to_string;

/// A block as it is placed, Pele's Cell. Its children are the cells at their indices in the
/// cells laid out.
#[derive(Clone, Debug, Default)]
pub(crate) struct Cell {
    pub(crate) children: Vec<usize>,
    /// The width of the grid of a container: a positive count, or -1 to put every child on
    /// one row.
    pub(crate) columns: f64,
    pub(crate) span: f64,
    pub(crate) space: bool,
    /// The height kept above a container's grid for its title.
    pub(crate) head: f64,
    /// The size the content needs, given for leaves and worked out here for containers.
    pub(crate) nw: f64,
    pub(crate) nh: f64,
    // Where the cell is: its place in the grid, then its center and size.
    pub(crate) col: f64,
    pub(crate) row: f64,
    pub(crate) cols: f64,
    pub(crate) heights: Vec<f64>,
    /// The room added after each row or column. A gap never set is 0.
    pub(crate) row_gaps: Vec<f64>,
    pub(crate) col_gaps: Vec<f64>,
    pub(crate) x: f64,
    pub(crate) y: f64,
    pub(crate) w: f64,
    pub(crate) h: f64,
}

/// The most columns a block may span, Pele's MAX_SPAN.
const MAX_SPAN: f64 = 1000.0;

fn semantic(message: String) -> Error {
    let mut e = Error::new(message, ErrorCode::Semantic);
    e.diagram = Some(DiagramType::Block);
    e
}

/// Whether a number is an integer, as Number.isInteger says.
fn is_integer(v: f64) -> bool {
    v.is_finite() && v.trunc() == v
}

/// The column and the row of the block at a position in a grid of columns, as Pele's
/// calculateBlockPosition gives them.
pub(crate) fn calculate_block_position(columns: f64, position: f64) -> Result<(f64, f64), Error> {
    if columns == 0.0 || !is_integer(columns) {
        return Err(semantic("Columns must be an integer !== 0.".into()));
    }
    if position < 0.0 || !is_integer(position) {
        return Err(semantic(format!(
            "Position must be a non-negative integer.{}",
            to_string(position)
        )));
    }
    if columns < 0.0 {
        return Ok((position, 0.0));
    }
    Ok((position % columns, (position / columns).floor()))
}

/// The sum of gaps, Pele's total.
fn total(gaps: &[f64]) -> f64 {
    gaps.iter().sum()
}

/// A gap, 0 where none was set.
fn gap_at(gaps: &[f64], i: f64) -> f64 {
    gaps.get(i as usize).copied().unwrap_or(0.0)
}

/// Puts each child in its grid cell and sizes the container, as Pele's measure does. Every
/// column is as wide as the widest block needs. Every row that holds a plain block is as tall
/// as the tallest block in the container, as in Mermaid. A row of nothing but containers is
/// only as tall as they need.
fn measure(cells: &mut [Cell], at: usize, gap: f64, inset: f64) -> Result<(), Error> {
    let columns = cells[at].columns;
    let mut heights: Vec<f64> = Vec::new();
    let mut position = 0.0;
    let mut cols = 0.0;
    let mut cw = 0.0;
    let mut tallest = 0.0;
    for k in 0..cells[at].children.len() {
        let c = cells[at].children[k];
        let child = &mut cells[c];
        let (px, py) = calculate_block_position(columns, position)?;
        // Mermaid lets a block wider than the rest of its row stick out of the grid. Here it
        // stops at the last column.
        let mut span = if child.span >= 1.0 {
            min(child.span, MAX_SPAN)
        } else {
            1.0
        };
        if columns > 0.0 && span > columns - px {
            span = columns - px;
        }
        child.span = span;
        child.col = px;
        child.row = py;
        position += span;
        if px + span > cols {
            cols = px + span;
        }
        let row = py as usize;
        if row == heights.len() {
            heights.push(0.0);
        }
        if child.children.is_empty() {
            heights[row] = -1.0;
        } else if heights[row] >= 0.0 && child.nh > heights[row] {
            heights[row] = child.nh;
        }
        if child.space {
            continue;
        }
        let width = (child.nw - gap * (span - 1.0)) / span;
        if width > cw {
            cw = width;
        }
        if child.nh > tallest {
            tallest = child.nh;
        }
    }
    let b = &mut cells[at];
    let mut height = (heights.len() as f64 - 1.0) * gap + total(&b.row_gaps);
    for h in &mut heights {
        if *h < 0.0 {
            *h = tallest;
        }
        height += *h;
    }
    b.cols = cols;
    b.heights = heights;
    b.nw = max(
        b.nw,
        cols * cw + (cols - 1.0) * gap + total(&b.col_gaps) + 2.0 * inset,
    );
    b.nh = height + 2.0 * inset + b.head;
    Ok(())
}

/// Gives each child its cell, sharing out any room the container has beyond what it needs,
/// as Pele's arrange does.
fn arrange(cells: &mut [Cell], at: usize, gap: f64, inset: f64) {
    let b = &cells[at];
    let cols = b.cols;
    let rows = b.heights.len() as f64;
    let cw = (b.w - 2.0 * inset - (cols - 1.0) * gap - total(&b.col_gaps)) / cols;
    let room = b.h - 2.0 * inset - b.head - (rows - 1.0) * gap - total(&b.row_gaps);
    let mut offsets: Vec<f64> = Vec::new();
    if !b.col_gaps.is_empty() {
        let mut sum = 0.0;
        let mut col = 0.0;
        while col < cols {
            offsets.push(sum);
            sum += gap_at(&b.col_gaps, col);
            col += 1.0;
        }
    }
    let needed: f64 = b.heights.iter().sum();
    let scale = if needed > 0.0 { room / needed } else { 0.0 };
    let share = if needed > 0.0 { 0.0 } else { room / rows };
    let left = b.x - b.w / 2.0 + inset;
    let mut top = b.y - b.h / 2.0 + inset + b.head;
    let (heights, row_gaps, children) = (b.heights.clone(), b.row_gaps.clone(), b.children.clone());
    let mut row = 0.0;
    for k in children {
        let child = &mut cells[k];
        while row < child.row {
            top += heights[row as usize] * scale + share + gap + gap_at(&row_gaps, row);
            row += 1.0;
        }
        let before = gap_at(&offsets, child.col);
        child.w = cw * child.span
            + gap * (child.span - 1.0)
            + gap_at(&offsets, child.col + child.span - 1.0)
            - before;
        child.h = heights[row as usize] * scale + share;
        child.x = left + child.col * (cw + gap) + before + child.w / 2.0;
        child.y = top + child.h / 2.0;
    }
}

/// Lays out a tree of cells, the root first, in place, as Pele's layout does, and gives its
/// width and height. The root has no border, so its grid starts at the origin.
pub(crate) fn layout(cells: &mut [Cell], gap: f64) -> Result<(f64, f64), Error> {
    let mut order = vec![0];
    let mut i = 0;
    while i < order.len() {
        order.extend(cells[order[i]].children.iter().copied());
        i += 1;
    }
    for (i, &c) in order.iter().enumerate().rev() {
        if !cells[c].children.is_empty() {
            measure(cells, c, gap, if i == 0 { 0.0 } else { gap })?;
        }
    }
    let root = &mut cells[0];
    root.w = root.nw;
    root.h = root.nh;
    root.x = root.w / 2.0;
    root.y = root.h / 2.0;
    for (i, &c) in order.iter().enumerate() {
        if !cells[c].children.is_empty() {
            arrange(cells, c, gap, if i == 0 { 0.0 } else { gap });
        }
    }
    Ok((cells[0].w, cells[0].h))
}
