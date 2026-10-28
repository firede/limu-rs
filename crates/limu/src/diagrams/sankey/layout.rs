//! The placing of a sankey diagram's nodes and bands, as Pele's layoutSankey in layout.ts
//! places them.

use std::cmp::Ordering;

use super::db::SankeyDb;
use crate::detect::DiagramType;
use crate::errors::{Error, ErrorCode};
use crate::js::math::{max, min};

/// The column a node goes in, Pele's Alignment.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Alignment {
    Justify,
    Left,
    Right,
    Center,
}

/// What Pele's layoutSankey is given, its LayoutOptions. label_height is the height of a line
/// of a label, and thin nodes are kept at least this far apart where there is room.
pub(crate) struct LayoutOptions {
    pub(crate) width: f64,
    pub(crate) height: f64,
    pub(crate) node_width: f64,
    pub(crate) node_padding: f64,
    pub(crate) label_height: f64,
    pub(crate) alignment: Alignment,
}

/// A node laid out, Pele's PlacedNode. value is the total flow through it, in the diagram's
/// units.
#[derive(Clone, Debug, Default)]
pub(crate) struct PlacedNode {
    pub(crate) layer: usize,
    pub(crate) value: f64,
    pub(crate) x: f64,
    pub(crate) y: f64,
    pub(crate) height: f64,
}

/// A link laid out, Pele's PlacedLink: source_y and target_y are the middles of its band
/// where it leaves its source and where it enters its target.
#[derive(Clone, Debug)]
pub(crate) struct PlacedLink {
    pub(crate) source: usize,
    pub(crate) target: usize,
    pub(crate) value: f64,
    pub(crate) thickness: f64,
    pub(crate) source_y: f64,
    pub(crate) target_y: f64,
}

/// How many times the columns settle, Pele's PASSES.
const PASSES: i32 = 6;

/// Sorts a list as Pele's sort does: by insertion where it is short, and with the built-in
/// sort otherwise. The comparisons Pele gives it order every two items one way, ties broken
/// by their index, so any sort puts the list in the same order as V8's.
fn sort(list: &mut [usize], compare: impl Fn(usize, usize) -> f64) {
    if list.len() > 8 {
        list.sort_by(|&a, &b| compare(a, b).partial_cmp(&0.0).unwrap_or(Ordering::Equal));
        return;
    }
    for i in 1..list.len() {
        let item = list[i];
        let mut j = i;
        while j > 0 && compare(list[j - 1], item) > 0.0 {
            list[j] = list[j - 1];
            j -= 1;
        }
        list[j] = item;
    }
}

/// `a || b` of two numbers: a, unless it is 0 or NaN.
fn or(a: f64, b: f64) -> f64 {
    if a != 0.0 && !a.is_nan() { a } else { b }
}

/// Places nodes in columns by their distance from a source, sizes them by their flow, and
/// then lets each column settle next to its neighbours so that the bands cross less, as
/// Pele's layoutSankey does. Links that run in a circle are an error.
pub(crate) fn layout_sankey(
    model: &SankeyDb,
    options: &LayoutOptions,
) -> Result<(Vec<PlacedNode>, Vec<PlacedLink>), Error> {
    let n = model.nodes.len();
    let mut links: Vec<PlacedLink> = model
        .links
        .iter()
        .map(|link| PlacedLink {
            source: link.source,
            target: link.target,
            value: if link.value.is_finite() && link.value > 0.0 {
                link.value
            } else {
                0.0
            },
            thickness: 0.0,
            source_y: 0.0,
            target_y: 0.0,
        })
        .collect();
    let mut outgoing: Vec<Vec<usize>> = vec![Vec::new(); n];
    let mut incoming: Vec<Vec<usize>> = vec![Vec::new(); n];
    for (k, link) in links.iter().enumerate() {
        outgoing[link.source].push(k);
        incoming[link.target].push(k);
    }

    // The longest distance from a source, in topological order. What is left over lies on a
    // cycle.
    let mut depth = vec![0usize; n];
    let mut pending = vec![0usize; n];
    let mut order = Vec::new();
    for i in 0..n {
        pending[i] = incoming[i].len();
        if pending[i] == 0 {
            order.push(i);
        }
    }
    let mut q = 0;
    while q < order.len() {
        let i = order[q];
        for &k in &outgoing[i] {
            let t = links[k].target;
            if depth[t] < depth[i] + 1 {
                depth[t] = depth[i] + 1;
            }
            pending[t] -= 1;
            if pending[t] == 0 {
                order.push(t);
            }
        }
        q += 1;
    }
    if order.len() < n {
        // Every node left over has a predecessor that is left over too, and walking back
        // finds the loop.
        let mut stuck = 0;
        while pending[stuck] == 0 {
            stuck += 1;
        }
        let mut seen = vec![false; n];
        while !seen[stuck] {
            seen[stuck] = true;
            for &k in &incoming[stuck] {
                if pending[links[k].source] > 0 {
                    stuck = links[k].source;
                    break;
                }
            }
        }
        let message = format!(
            "Sankey diagrams cannot have circular links, and \"{}\" is part of one.",
            model.nodes[stuck].id
        );
        return Err(Error {
            diagram: Some(DiagramType::Sankey),
            ..Error::new(message, ErrorCode::Semantic)
        });
    }
    let mut layers = 1;
    for &d in &depth {
        if d + 1 > layers {
            layers = d + 1;
        }
    }
    let mut reach = vec![0usize; n];
    for &i in order.iter().rev() {
        for &k in &outgoing[i] {
            if reach[i] < reach[links[k].target] + 1 {
                reach[i] = reach[links[k].target] + 1;
            }
        }
    }

    // Flows are scaled to the largest one, so that sums of huge values stay finite.
    let mut largest = 0.0;
    for link in &links {
        if link.value > largest {
            largest = link.value;
        }
    }
    let unit = if largest > 0.0 { 1.0 / largest } else { 0.0 };

    let mut nodes = Vec::with_capacity(n);
    let mut flow = vec![0.0; n];
    let mut columns: Vec<Vec<usize>> = vec![Vec::new(); layers];
    let step = if layers > 1 {
        (options.width - options.node_width) / (layers - 1) as f64
    } else {
        0.0
    };
    for i in 0..n {
        let layer = match options.alignment {
            Alignment::Left => depth[i],
            Alignment::Right => layers - 1 - reach[i],
            Alignment::Center => {
                let mut layer = depth[i];
                if incoming[i].is_empty() && !outgoing[i].is_empty() {
                    layer = layers;
                    for &k in &outgoing[i] {
                        if depth[links[k].target] - 1 < layer {
                            layer = depth[links[k].target] - 1;
                        }
                    }
                }
                layer
            }
            Alignment::Justify => {
                if outgoing[i].is_empty() {
                    layers - 1
                } else {
                    depth[i]
                }
            }
        };
        let (mut into, mut out, mut value, mut value_out) = (0.0, 0.0, 0.0, 0.0);
        for &k in &incoming[i] {
            into += links[k].value * unit;
            value += links[k].value;
        }
        for &k in &outgoing[i] {
            out += links[k].value * unit;
            value_out += links[k].value;
        }
        flow[i] = max(into, out);
        nodes.push(PlacedNode {
            layer,
            value: max(value, value_out),
            x: layer as f64 * step,
            y: 0.0,
            height: 0.0,
        });
        columns[layer].push(i);
    }

    let mut tallest = 1;
    for column in &columns {
        if column.len() > tallest {
            tallest = column.len();
        }
    }
    let gap = if tallest > 1 {
        min(options.node_padding, options.height / (tallest - 1) as f64)
    } else {
        options.node_padding
    };
    let mut scale = f64::INFINITY;
    for column in &columns {
        let mut sum = 0.0;
        for &i in column {
            sum += flow[i];
        }
        if sum > 0.0 {
            scale = min(
                scale,
                (options.height - (column.len() as f64 - 1.0) * gap) / sum,
            );
        }
    }
    if !scale.is_finite() || scale < 0.0 {
        scale = 0.0;
    }

    // A thin node still needs room for its label. Where every column can afford it, each node
    // claims at least a label's height, and the flows are scaled down to make that fit.
    let mut claim = max(0.0, options.label_height - gap);
    let fits = |s: f64, claim: f64| {
        for column in &columns {
            let mut extent = (column.len() as f64 - 1.0) * gap;
            for &i in column {
                extent += max(flow[i] * s, claim);
            }
            if extent > options.height + 1e-6 {
                return false;
            }
        }
        true
    };
    if claim > 0.0 && !fits(scale, claim) {
        if fits(0.0, claim) {
            let mut low = 0.0;
            for _ in 0..40 {
                let mid = (low + scale) / 2.0;
                if fits(mid, claim) {
                    low = mid;
                } else {
                    scale = mid;
                }
            }
            scale = low;
        } else {
            claim = 0.0;
        }
    }

    let mut margin = vec![0.0; n];
    for link in &mut links {
        link.thickness = link.value * unit * scale;
    }
    for column in &columns {
        let mut y = 0.0;
        for &i in column {
            nodes[i].height = flow[i] * scale;
            margin[i] = max(0.0, claim - nodes[i].height) / 2.0;
            nodes[i].y = y + margin[i];
            y += nodes[i].height + 2.0 * margin[i] + gap;
        }
        // Spare room is shared out between the nodes of a column.
        let spare = (options.height - y + gap) / (column.len() + 1) as f64;
        for (k, &i) in column.iter().enumerate() {
            nodes[i].y += spare * (k + 1) as f64;
        }
    }

    let mut b = Bands {
        source_offset: vec![0.0; links.len()],
        target_offset: vec![0.0; links.len()],
        nodes,
        links,
        outgoing,
        incoming,
        margin,
        gap,
        height: options.height,
    };
    b.stack_all();
    for pass in 0..PASSES {
        // Math.pow(0.99, pass). For the six passes, powf gives the powers V8 gives, which are
        // the powers correctly rounded.
        let pull = 0.99f64.powf(f64::from(pass));
        for l in (0..layers.saturating_sub(1)).rev() {
            b.settle(&mut columns[l], pull, false);
        }
        b.stack_all();
        for column in columns.iter_mut().skip(1) {
            b.settle(column, pull, true);
        }
        b.stack_all();
    }
    let Bands {
        nodes,
        mut links,
        source_offset,
        target_offset,
        ..
    } = b;
    for (k, link) in links.iter_mut().enumerate() {
        link.source_y = nodes[link.source].y + source_offset[k];
        link.target_y = nodes[link.target].y + target_offset[k];
    }
    Ok((nodes, links))
}

/// The nodes and bands as they settle, with what Pele's layoutSankey keeps of them as it lets
/// the columns settle. Bands leave and enter a node in the order of where they go, so they do
/// not cross at it. The offsets are those of each band's middle from the top of the node at
/// either end.
struct Bands {
    nodes: Vec<PlacedNode>,
    links: Vec<PlacedLink>,
    outgoing: Vec<Vec<usize>>,
    incoming: Vec<Vec<usize>>,
    /// The room a node keeps clear above and below itself.
    margin: Vec<f64>,
    source_offset: Vec<f64>,
    target_offset: Vec<f64>,
    gap: f64,
    height: f64,
}

impl Bands {
    fn center(&self, i: usize) -> f64 {
        self.nodes[i].y + self.nodes[i].height / 2.0
    }

    fn stack_out(&mut self, i: usize) {
        let mut list = std::mem::take(&mut self.outgoing[i]);
        sort(&mut list, |a, b| {
            or(
                self.center(self.links[a].target) - self.center(self.links[b].target),
                a as f64 - b as f64,
            )
        });
        let mut y = 0.0;
        for &k in &list {
            self.source_offset[k] = y + self.links[k].thickness / 2.0;
            y += self.links[k].thickness;
        }
        self.outgoing[i] = list;
    }

    fn stack_in(&mut self, i: usize) {
        let mut list = std::mem::take(&mut self.incoming[i]);
        sort(&mut list, |a, b| {
            or(
                self.center(self.links[a].source) - self.center(self.links[b].source),
                a as f64 - b as f64,
            )
        });
        let mut y = 0.0;
        for &k in &list {
            self.target_offset[k] = y + self.links[k].thickness / 2.0;
            y += self.links[k].thickness;
        }
        self.incoming[i] = list;
    }

    fn stack_all(&mut self) {
        for i in 0..self.nodes.len() {
            self.stack_out(i);
            self.stack_in(i);
        }
    }

    /// Moves each node of a column towards where its bands would run level, then restores the
    /// order and the gaps, as Pele's settle does. forward looks at the bands coming in, and
    /// otherwise at those going out.
    fn settle(&mut self, column: &mut [usize], pull: f64, forward: bool) {
        for &i in column.iter() {
            let list = if forward {
                &self.incoming[i]
            } else {
                &self.outgoing[i]
            };
            if list.is_empty() {
                continue;
            }
            if forward {
                self.stack_in(i);
            } else {
                self.stack_out(i);
            }
            let list = if forward {
                &self.incoming[i]
            } else {
                &self.outgoing[i]
            };
            let (mut weighted, mut weight, mut plain) = (0.0, 0.0, 0.0);
            for &k in list {
                let link = &self.links[k];
                let level = if forward {
                    self.nodes[link.source].y + self.source_offset[k] - self.target_offset[k]
                } else {
                    self.nodes[link.target].y + self.target_offset[k] - self.source_offset[k]
                };
                weighted += level * link.thickness;
                weight += link.thickness;
                let other = if forward { link.source } else { link.target };
                plain += self.center(other) - self.nodes[i].height / 2.0;
            }
            let wanted = if weight > 0.0 {
                weighted / weight
            } else {
                plain / list.len() as f64
            };
            self.nodes[i].y += (wanted - self.nodes[i].y) * pull;
        }
        sort(column, |a, b| {
            or(self.center(a) - self.center(b), a as f64 - b as f64)
        });
        let (nodes, margin, gap) = (&mut self.nodes, &self.margin, self.gap);
        let mut y = 0.0;
        for &i in column.iter() {
            if nodes[i].y - margin[i] < y {
                nodes[i].y = y + margin[i];
            }
            y = nodes[i].y + nodes[i].height + margin[i] + gap;
        }
        y = self.height;
        for &i in column.iter().rev() {
            if nodes[i].y + nodes[i].height + margin[i] > y {
                nodes[i].y = y - nodes[i].height - margin[i];
            }
            y = nodes[i].y - margin[i] - gap;
        }
    }
}
