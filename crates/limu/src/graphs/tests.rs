//! The layouts checked against what Pele makes of the same graphs, in the oracle's
//! graphs-layout and graphs-lanes fixtures.

use limu_fixtures::load;
use serde::Deserialize;

use super::compound::{CEdge, CNode, Dir, cnode, compound_layout, shift_layout};
use super::lanes::lane_layout;
use super::layered::{Kind, LEdge, LNode, LayeredOptions, ease, layered, ledge, lnode};
use crate::js::math::hypot;

/// A number as these fixtures write it: a JSON number, or a string for an infinity or NaN,
/// which JSON has no words for. They write -0 as 0.
#[derive(Clone, Copy, Debug)]
struct N(f64);

impl<'de> Deserialize<'de> for N {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<N, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Raw {
            Number(f64),
            Word(String),
        }
        Ok(N(match Raw::deserialize(d)? {
            Raw::Number(n) => n,
            Raw::Word(w) => match w.as_str() {
                "Infinity" => f64::INFINITY,
                "-Infinity" => f64::NEG_INFINITY,
                "NaN" => f64::NAN,
                _ => return Err(serde::de::Error::custom(format!("not a number: {w}"))),
            },
        }))
    }
}

/// Whether a number is the one Pele gives, to the bit but for the sign of a zero, which the
/// fixtures do not keep.
fn same(got: f64, want: N) -> bool {
    got == want.0 || got.is_nan() && want.0.is_nan()
}

fn all_same(got: &[f64], want: &[N]) -> bool {
    got.len() == want.len() && got.iter().zip(want).all(|(&g, &w)| same(g, w))
}

#[derive(Deserialize)]
struct Layout {
    ease: Vec<EaseCase>,
    hypot: Vec<HypotCase>,
    layered: Vec<LayeredCase>,
    compound: Vec<CompoundCase>,
}

#[derive(Deserialize)]
struct EaseCase {
    across: N,
    along: N,
    ease: N,
}

#[derive(Deserialize)]
struct HypotCase {
    a: N,
    b: N,
    hypot: N,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Opt {
    node_sep: N,
    edge_sep: N,
    rank_sep: N,
    port_sep: N,
    head_room: Option<N>,
    #[serde(default)]
    tracks: bool,
    #[serde(default)]
    straight: bool,
    #[serde(default)]
    apart: bool,
}

impl Opt {
    fn options(&self) -> LayeredOptions {
        LayeredOptions {
            node_sep: self.node_sep.0,
            edge_sep: self.edge_sep.0,
            rank_sep: self.rank_sep.0,
            port_sep: self.port_sep.0,
            tracks: self.tracks,
            straight: self.straight,
            apart: self.apart,
            head_room: self.head_room.map(|n| n.0),
            groups: None,
            order_only: false,
        }
    }
}

#[derive(Deserialize)]
struct Size {
    width: N,
    height: N,
}

#[derive(Deserialize)]
struct LayeredCase {
    name: String,
    nodes: Vec<LayeredNode>,
    edges: Vec<LayeredEdge>,
    opt: Opt,
    out: LayeredOut,
}

#[derive(Deserialize)]
struct LayeredNode {
    w: N,
    h: N,
    kind: u8,
    pin: i32,
    span: N,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LayeredEdge {
    tail: usize,
    head: usize,
    minlen: N,
    label_w: N,
    label_h: N,
    tail_dx: N,
    head_dx: N,
    tail_fixed: bool,
    head_fixed: bool,
}

#[derive(Deserialize)]
struct LayeredOut {
    result: Size,
    nodes: Vec<LayeredPlace>,
    edges: Vec<LayeredRoute>,
}

#[derive(Deserialize)]
struct LayeredPlace {
    x: N,
    y: N,
    rank: N,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LayeredRoute {
    tail_dx: N,
    head_dx: N,
    reversed: bool,
    points: Vec<N>,
    label_x: N,
    label_y: N,
}

fn kind(k: u8) -> Kind {
    match k {
        0 => Kind::Node,
        1 => Kind::Dummy,
        2 => Kind::Label,
        3 => Kind::StartPort,
        4 => Kind::EndPort,
        _ => panic!("no kind {k}"),
    }
}

/// Runs a case as the oracle runs it, and says where it comes out otherwise than Pele.
fn run_layered(c: &LayeredCase) -> Option<String> {
    let mut nodes: Vec<LNode> = c
        .nodes
        .iter()
        .map(|n| {
            let mut ln = lnode(n.w.0, n.h.0, kind(n.kind));
            ln.pin = n.pin;
            ln.span = n.span.0;
            ln
        })
        .collect();
    let mut edges: Vec<LEdge> = c
        .edges
        .iter()
        .map(|e| {
            let mut le = ledge(e.tail, e.head, e.minlen.0, e.label_w.0, e.label_h.0);
            le.tail_dx = e.tail_dx.0;
            le.head_dx = e.head_dx.0;
            le.tail_fixed = e.tail_fixed;
            le.head_fixed = e.head_fixed;
            le
        })
        .collect();
    let result = layered(&mut nodes, &mut edges, &c.opt.options());
    let out = &c.out;
    if !same(result.width, out.result.width) || !same(result.height, out.result.height) {
        return Some(format!(
            "size {}×{}, Pele {}×{}",
            result.width, result.height, out.result.width.0, out.result.height.0
        ));
    }
    for (i, (n, p)) in nodes.iter().zip(&out.nodes).enumerate() {
        if !same(n.x, p.x) || !same(n.y, p.y) || !same(n.rank as f64, p.rank) {
            return Some(format!(
                "node {i} at {},{} rank {}, Pele {},{} rank {}",
                n.x, n.y, n.rank, p.x.0, p.y.0, p.rank.0
            ));
        }
    }
    for (i, (e, r)) in edges.iter().zip(&out.edges).enumerate() {
        if e.reversed != r.reversed
            || !same(e.tail_dx, r.tail_dx)
            || !same(e.head_dx, r.head_dx)
            || !all_same(&e.points, &r.points)
            || !same(e.label_x, r.label_x)
            || !same(e.label_y, r.label_y)
        {
            return Some(format!(
                "edge {i}: {:?} label {},{}, Pele {:?} label {},{}",
                e.points,
                e.label_x,
                e.label_y,
                r.points.iter().map(|n| n.0).collect::<Vec<_>>(),
                r.label_x.0,
                r.label_y.0
            ));
        }
    }
    None
}

fn dir(d: &str) -> Dir {
    match d {
        "TB" => Dir::TB,
        "BT" => Dir::BT,
        "LR" => Dir::LR,
        "RL" => Dir::RL,
        _ => panic!("no direction {d}"),
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CompoundCase {
    name: String,
    root_dir: String,
    opt: Opt,
    nodes: Vec<CompoundNode>,
    edges: Vec<CompoundEdge>,
    shift: Option<(N, N)>,
    out: CompoundOut,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CompoundNode {
    w: N,
    h: N,
    parent: i32,
    is_group: bool,
    dir: Option<String>,
    pad_x: N,
    pad_top: N,
    pad_bottom: N,
    min_w: N,
    seq: N,
    span: N,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CompoundEdge {
    src: usize,
    dst: usize,
    minlen: N,
    label_w: N,
    label_h: N,
}

impl CompoundEdge {
    fn edge(&self) -> CEdge {
        CEdge {
            src: self.src,
            dst: self.dst,
            minlen: self.minlen.0,
            label_w: self.label_w.0,
            label_h: self.label_h.0,
            route: Vec::new(),
            label_x: 0.0,
            label_y: 0.0,
        }
    }
}

#[derive(Deserialize)]
struct CompoundOut {
    result: Size,
    nodes: Vec<CompoundPlace>,
    edges: Vec<CompoundRoute>,
}

#[derive(Deserialize)]
struct CompoundPlace {
    x: N,
    y: N,
    w: N,
    h: N,
    parent: i32,
    seq: N,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CompoundRoute {
    src: usize,
    dst: usize,
    route: Vec<N>,
    label_x: N,
    label_y: N,
}

/// Runs a case as the oracle runs it, and says where it comes out otherwise than Pele.
fn run_compound(c: &CompoundCase) -> Option<String> {
    let mut nodes: Vec<CNode> = c
        .nodes
        .iter()
        .map(|n| {
            let mut cn = cnode(n.w.0, n.h.0, n.parent);
            cn.is_group = n.is_group;
            cn.dir = n.dir.as_deref().map(dir);
            cn.pad_x = n.pad_x.0;
            cn.pad_top = n.pad_top.0;
            cn.pad_bottom = n.pad_bottom.0;
            cn.min_w = n.min_w.0;
            cn.seq = n.seq.0;
            cn.span = n.span.0;
            cn
        })
        .collect();
    let mut edges: Vec<CEdge> = c.edges.iter().map(CompoundEdge::edge).collect();
    let result = compound_layout(&mut nodes, &mut edges, dir(&c.root_dir), &c.opt.options());
    if let Some((dx, dy)) = c.shift {
        shift_layout(&mut nodes, &mut edges, dx.0, dy.0);
    }
    let out = &c.out;
    if !same(result.width, out.result.width) || !same(result.height, out.result.height) {
        return Some(format!(
            "size {}×{}, Pele {}×{}",
            result.width, result.height, out.result.width.0, out.result.height.0
        ));
    }
    for (i, (n, p)) in nodes.iter().zip(&out.nodes).enumerate() {
        if !same(n.x, p.x)
            || !same(n.y, p.y)
            || !same(n.w, p.w)
            || !same(n.h, p.h)
            || n.parent != p.parent
            || !same(n.seq, p.seq)
        {
            return Some(format!(
                "node {i} at {},{} {}×{}, Pele {},{} {}×{}",
                n.x, n.y, n.w, n.h, p.x.0, p.y.0, p.w.0, p.h.0
            ));
        }
    }
    for (i, (e, r)) in edges.iter().zip(&out.edges).enumerate() {
        if e.src != r.src
            || e.dst != r.dst
            || !all_same(&e.route, &r.route)
            || !same(e.label_x, r.label_x)
            || !same(e.label_y, r.label_y)
        {
            return Some(format!(
                "edge {i}: {:?} label {},{}, Pele {:?} label {},{}",
                e.route,
                e.label_x,
                e.label_y,
                r.route.iter().map(|n| n.0).collect::<Vec<_>>(),
                r.label_x.0,
                r.label_y.0
            ));
        }
    }
    None
}

/// Panics with the cases that come out otherwise than Pele, if any do.
fn report(section: &str, total: usize, failed: Vec<(String, String)>) {
    if failed.is_empty() {
        return;
    }
    let shown: Vec<String> = failed
        .iter()
        .take(8)
        .map(|(name, why)| format!("  {name}: {why}"))
        .collect();
    panic!(
        "{} of {total} {section} cases differ from Pele:\n{}",
        failed.len(),
        shown.join("\n")
    );
}

#[test]
fn layouts_match_pele() {
    let p: Layout = load("graphs-layout");
    for c in &p.ease {
        assert!(
            same(ease(c.across.0, c.along.0), c.ease),
            "ease({}, {})",
            c.across.0,
            c.along.0
        );
    }
    for c in &p.hypot {
        assert!(
            same(hypot(c.a.0, c.b.0), c.hypot),
            "hypot({}, {})",
            c.a.0,
            c.b.0
        );
    }
    let failed: Vec<_> = p
        .layered
        .iter()
        .filter_map(|c| run_layered(c).map(|why| (c.name.clone(), why)))
        .collect();
    report("layered", p.layered.len(), failed);
    let failed: Vec<_> = p
        .compound
        .iter()
        .filter_map(|c| run_compound(c).map(|why| (c.name.clone(), why)))
        .collect();
    report("compound", p.compound.len(), failed);
}

#[derive(Deserialize)]
struct Lanes {
    cases: Vec<LanesCase>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LanesCase {
    name: String,
    dir: String,
    nodes: Vec<LanesNode>,
    edges: Vec<CompoundEdge>,
    lane_pad: N,
    opt: Opt,
    out: LanesOut,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LanesNode {
    w: N,
    h: N,
    parent: i32,
    is_group: bool,
    pad_x: N,
    pad_top: N,
    pad_bottom: N,
    min_w: N,
}

#[derive(Deserialize)]
struct LanesOut {
    result: Size,
    nodes: Vec<LanesPlace>,
    edges: Vec<LanesRoute>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LanesPlace {
    x: N,
    y: N,
    w: N,
    h: N,
    pad_top: N,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LanesRoute {
    route: Vec<N>,
    label_x: N,
    label_y: N,
}

/// Runs a case as the oracle runs it, and says where it comes out otherwise than Pele.
fn run_lanes(c: &LanesCase) -> Option<String> {
    let mut nodes: Vec<CNode> = c
        .nodes
        .iter()
        .map(|n| {
            let mut cn = cnode(n.w.0, n.h.0, n.parent);
            cn.is_group = n.is_group;
            cn.pad_x = n.pad_x.0;
            cn.pad_top = n.pad_top.0;
            cn.pad_bottom = n.pad_bottom.0;
            cn.min_w = n.min_w.0;
            cn
        })
        .collect();
    let mut edges: Vec<CEdge> = c.edges.iter().map(CompoundEdge::edge).collect();
    let result = lane_layout(
        &mut nodes,
        &mut edges,
        dir(&c.dir),
        &c.opt.options(),
        c.lane_pad.0,
    );
    let out = &c.out;
    if !same(result.width, out.result.width) || !same(result.height, out.result.height) {
        return Some(format!(
            "size {}×{}, Pele {}×{}",
            result.width, result.height, out.result.width.0, out.result.height.0
        ));
    }
    for (i, (n, p)) in nodes.iter().zip(&out.nodes).enumerate() {
        if !same(n.x, p.x)
            || !same(n.y, p.y)
            || !same(n.w, p.w)
            || !same(n.h, p.h)
            || !same(n.pad_top, p.pad_top)
        {
            return Some(format!(
                "node {i} at {},{} {}×{}, Pele {},{} {}×{}",
                n.x, n.y, n.w, n.h, p.x.0, p.y.0, p.w.0, p.h.0
            ));
        }
    }
    for (i, (e, r)) in edges.iter().zip(&out.edges).enumerate() {
        if !all_same(&e.route, &r.route)
            || !same(e.label_x, r.label_x)
            || !same(e.label_y, r.label_y)
        {
            return Some(format!(
                "edge {i}: {:?} label {},{}, Pele {:?} label {},{}",
                e.route,
                e.label_x,
                e.label_y,
                r.route.iter().map(|n| n.0).collect::<Vec<_>>(),
                r.label_x.0,
                r.label_y.0
            ));
        }
    }
    None
}

#[test]
fn lanes_match_pele() {
    let p: Lanes = load("graphs-lanes");
    let failed: Vec<_> = p
        .cases
        .iter()
        .filter_map(|c| run_lanes(c).map(|why| (c.name.clone(), why)))
        .collect();
    report("lane", p.cases.len(), failed);
}
