//! Git graphs read and laid out as Pele reads and draws them, checked against the oracle's
//! gitgraph and gitgraph-render fixtures.

use limu_fixtures::load;
use serde::Deserialize;

use super::db::{GitModel, build_git};
use super::parser::{GIT_TOKENS, GitGraphAst, StatementAst, parse_git};
use super::render::{
    ROLE_BRANCH, ROLE_COMMIT, ROLE_COMMIT_LABEL, ROLE_COMMIT_MARK, ROLE_EDGE, ROLE_LANE,
    ROLE_MERGE, ROLE_TAG, ROLE_TAG_HOLE, ROLE_TITLE, layout,
};
use crate::detect::DiagramType;
use crate::errors::ErrorCode;
use crate::layout::Options;
use crate::prepare::{ParseOptions, prepare};
use crate::scene::style::{Cap, Join, Paint};
use crate::scene::{Item, Op, Outline, Painting, Part, Scene};
use crate::testing::pelesvg::id;
use crate::testing::{Enc, Failures, PeleError, pele_num, same, sorted};
use crate::value::{Map, Value};

#[derive(Deserialize)]
struct Cases {
    cases: Vec<Case>,
}

#[derive(Deserialize)]
struct Case {
    src: String,
    direct: Outcome,
    #[serde(rename = "type")]
    diagram: Option<String>,
    #[serde(flatten)]
    outcome: Outcome,
    /// Whether the pipeline's outcome is the direct one.
    #[serde(default)]
    same: bool,
}

#[derive(Deserialize)]
struct Outcome {
    ast: Option<Enc>,
    model: Option<Enc>,
    error: Option<PeleError>,
}

/// The kinds of token, numbered by hand, are the indexes of their types in GIT_TOKENS.
#[test]
fn token_kinds() {
    let names: Vec<&str> = GIT_TOKENS.iter().map(|t| t.name).collect();
    assert_eq!(
        names,
        [
            "cherry-pick",
            "gitGraph:",
            "HIGHLIGHT",
            "gitGraph",
            "checkout",
            "REVERSE",
            "parent:",
            "commit",
            "NORMAL",
            "branch",
            "order:",
            "switch",
            "type:",
            "merge",
            "msg:",
            "tag:",
            "id:",
            "LR",
            "TB",
            "BT",
            ":",
            "ACC_DESCR",
            "ACC_TITLE",
            "TITLE",
            "INT",
            "STRING",
            "NEWLINE",
            "WHITESPACE",
            "YAML",
            "DIRECTIVE",
            "SINGLE_LINE_COMMENT",
            "REFERENCE"
        ]
    );
}

fn object(entries: Vec<(&str, Value)>) -> Value {
    Value::Object(
        entries
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect(),
    )
}

fn strings(v: &[String]) -> Value {
    Value::Array(v.iter().cloned().map(Value::String).collect())
}

/// Sets a field Pele may leave undefined, where it is set.
fn opt(o: &mut Vec<(&'static str, Value)>, key: &'static str, v: Option<&str>) {
    if let Some(v) = v {
        o.push((key, Value::String(v.to_string())));
    }
}

/// The tree as JSON.stringify writes it.
fn dump_ast(a: &GitGraphAst) -> Value {
    let kind = if a.is_direction {
        "Direction"
    } else {
        "GitGraph"
    };
    let mut o = vec![("$type", Value::String(kind.into()))];
    opt(&mut o, "dir", a.dir);
    opt(&mut o, "title", a.title.as_deref());
    opt(&mut o, "accTitle", a.acc_title.as_deref());
    opt(&mut o, "accDescr", a.acc_descr.as_deref());
    let statements = a.statements.iter().map(|s| {
        let mut v = Vec::new();
        match s {
            StatementAst::Commit {
                id,
                message,
                tags,
                kind,
            } => {
                v.push(("$type", Value::String("Commit".into())));
                opt(&mut v, "id", id.as_deref());
                opt(&mut v, "message", message.as_deref());
                v.push(("tags", strings(tags)));
                opt(&mut v, "type", *kind);
            }
            StatementAst::Branch { name, order } => {
                v.push(("$type", Value::String("Branch".into())));
                v.push(("name", Value::String(name.clone())));
                if let Some(order) = order {
                    v.push(("order", Value::Number(*order)));
                }
            }
            StatementAst::Merge {
                branch,
                id,
                tags,
                kind,
            } => {
                v.push(("$type", Value::String("Merge".into())));
                v.push(("branch", Value::String(branch.clone())));
                opt(&mut v, "id", id.as_deref());
                v.push(("tags", strings(tags)));
                opt(&mut v, "type", *kind);
            }
            StatementAst::Checkout { branch } => {
                v.push(("$type", Value::String("Checkout".into())));
                v.push(("branch", Value::String(branch.clone())));
            }
            StatementAst::CherryPicking { id, tags, parent } => {
                v.push(("$type", Value::String("CherryPicking".into())));
                opt(&mut v, "id", id.as_deref());
                v.push(("tags", strings(tags)));
                opt(&mut v, "parent", parent.as_deref());
            }
        }
        object(v)
    });
    o.push(("statements", Value::Array(statements.collect())));
    object(o)
}

/// The model as JSON.stringify writes it, without its type and what is undefined.
fn dump_model(m: &GitModel) -> Value {
    let mut o = Vec::new();
    opt(&mut o, "title", m.title.as_deref());
    opt(&mut o, "accTitle", m.acc_title.as_deref());
    opt(&mut o, "accDescr", m.acc_descr.as_deref());
    o.push(("direction", Value::String(m.direction.into())));
    let commits = m.commits.values().map(|c| {
        let mut v = vec![
            ("id", Value::String(c.id.clone())),
            ("message", Value::String(c.message.clone())),
            ("seq", Value::Number(c.seq as f64)),
            ("type", Value::Number(f64::from(c.kind))),
            ("tags", strings(&c.tags)),
            ("parents", strings(&c.parents)),
            ("branch", Value::String(c.branch.clone())),
        ];
        if let Some(t) = c.custom_type {
            v.push(("customType", Value::Number(f64::from(t))));
        }
        if let Some(b) = c.custom_id {
            v.push(("customId", Value::Bool(b)));
        }
        object(v)
    });
    o.push(("commits", Value::Array(commits.collect())));
    let branches = m.branches.values().map(|b| {
        let head = b.head.clone().map_or(Value::Null, Value::String);
        Value::Array(vec![Value::String(b.name.clone()), head])
    });
    o.push(("branches", Value::Array(branches.collect())));
    o.push(("lanes", strings(&m.lanes)));
    o.push(("currentBranch", Value::String(m.current_branch.clone())));
    o.push(("head", m.head.clone().map_or(Value::Null, Value::String)));
    o.push(("warnings", strings(&m.warnings)));
    object(o)
}

/// Whether parsing a text with a configuration and a title gives what Pele had: its tree,
/// and then its model, or an error at either.
fn matches(
    tree: Result<GitGraphAst, crate::Error>,
    config: &Map,
    title: Option<&str>,
    want: &Outcome,
) -> bool {
    let ast = match (tree, &want.error) {
        (Err(e), Some(err)) => return want.ast.is_none() && err.is(&e),
        (Err(_), None) => return false,
        (Ok(ast), _) => ast,
    };
    let Some(want_ast) = &want.ast else {
        return false;
    };
    if !same(&sorted(&dump_ast(&ast)), &sorted(&want_ast.0)) {
        return false;
    }
    match (build_git(ast, config, title), &want.error, &want.model) {
        (Err(e), Some(err), _) => err.is(&e),
        (Ok(m), None, Some(model)) => same(&sorted(&dump_model(&m)), &sorted(&model.0)),
        _ => false,
    }
}

#[test]
fn parse_matches_pele() {
    let f: Cases = load("gitgraph");
    let mut failures = Failures::new();
    let mut piped = 0;
    for c in &f.cases {
        let ok = matches(parse_git(&c.src), &Map::new(), None, &c.direct);
        failures.check(ok, || {
            format!(
                "parse({:?}): {:?}, Pele {:?} {:?} {:?}",
                c.src,
                parse_git(&c.src),
                c.direct.ast,
                c.direct.model,
                c.direct.error
            )
        });

        let want = if c.same { &c.direct } else { &c.outcome };
        let p = prepare(&c.src, &ParseOptions::default());
        let ok = match (&p, &want.error) {
            (Err(e), Some(want)) => want.is(e),
            (Err(e), None) => e.code == ErrorCode::UnsupportedDiagram && c.diagram.is_none(),
            (Ok(p), _) if c.diagram.as_deref() != Some(p.found.diagram.name()) => false,
            (Ok(p), _) if p.found.diagram == DiagramType::GitGraph => {
                piped += 1;
                let tree = p.parse_with(false, parse_git);
                matches(tree, &p.config, p.title.as_deref(), want)
            }
            (Ok(_), _) => true,
        };
        failures.check(ok, || {
            format!(
                "{:?}: ast {:?}, model {:?}, error {:?}",
                c.src, want.ast, want.model, want.error
            )
        });
    }
    assert!(piped >= 5000, "only {piped} cases were git graphs");
    failures.done("gitgraph", 2 * 6000);
}

#[derive(Deserialize)]
struct Renders {
    cases: Vec<Render>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Render {
    src: String,
    max_width: Option<f64>,
    font_size: Option<f64>,
    #[serde(flatten)]
    drawing: limu_fixtures::Drawing,
}

/// A paint as the oracle names Pele's: a token of the theme by its name, "none" for none and
/// "-" for none given.
fn paint_name(p: Option<Paint>) -> String {
    match p {
        None => "-".into(),
        Some(p) if p == Paint::NONE => "none".into(),
        Some(p) => p.as_token().map_or("?", |t| t.name()).into(),
    }
}

fn pt(x: f64, y: f64) -> String {
    format!("{},{}", pele_num(x), pele_num(y))
}

/// How a part of an item is painted, as the oracle reads Pele's fill, stroke, width, cap,
/// join and dashes.
fn paint(it: &Item, part: &Part) -> String {
    let (mut fill, mut stroke) = (it.style.fill, it.style.stroke);
    match part.paint {
        Painting::StrokeOnly => fill = Some(Paint::NONE),
        Painting::FillOnly => stroke = Some(Paint::NONE),
        Painting::Hollow => fill = Some(Paint::BG),
        _ => {}
    }
    let stroke = stroke.unwrap_or(Paint::NONE);
    let mut s = format!("f={} s={}", paint_name(fill), paint_name(Some(stroke)));
    if stroke.visible() {
        let pen = it.author.over(&it.style);
        let cap = match pen.cap {
            None => "-",
            Some(Cap::Butt) => "butt",
            Some(Cap::Round) => "round",
            Some(Cap::Square) => "square",
        };
        let join = match pen.join {
            None => "-",
            Some(Join::Miter) => "miter",
            Some(Join::Round) => "round",
            Some(Join::Bevel) => "bevel",
        };
        s += &format!(
            " w={} c={cap} j={join}",
            pele_num(it.style.stroke_width.unwrap_or(1.0))
        );
    }
    let dash = match &it.style.dash {
        Some(d) if !d.is_empty() => d.iter().map(|&v| pele_num(v)).collect::<Vec<_>>().join(" "),
        _ => "-".into(),
    };
    format!("{s} d={dash}")
}

/// A part as the oracle writes it: a box, a circle as Path::ellipse draws it, or the steps of
/// a path.
fn shape(lines: &mut Vec<String>, it: &Item, part: &Part) {
    let p = match &part.outline {
        Outline::Box { rect: r, rounded } => {
            let radius = match (rounded, it.style.radius) {
                (false, _) => "-".into(),
                (true, None) => "theme".into(),
                (true, Some(v)) => pele_num(v),
            };
            lines.push(format!(
                "rect {},{} r={radius} {}",
                pt(r.x, r.y),
                pt(r.w, r.h),
                paint(it, part)
            ));
            return;
        }
        Outline::Path(p) => p,
    };
    let ops = p.ops();
    if ops.len() == 6 && ops[0] == Op::MoveTo && ops[1] == Op::CubeTo && ops[5] == Op::Close {
        // The end of the first curve is straight under the middle.
        let (cx, cy) = (p.args()[6], p.args()[1]);
        lines.push(format!(
            "circle {} {} {}",
            pt(cx, cy),
            pele_num(p.args()[0] - cx),
            paint(it, part)
        ));
        return;
    }
    let mut s = Vec::new();
    let mut at = 0;
    for &op in ops {
        let a = &p.args()[at..at + op.args()];
        at += op.args();
        let mut w = "MLQCZ"[op as usize..op as usize + 1].to_string();
        for xy in a.chunks(2) {
            w += &format!(" {}", pt(xy[0], xy[1]));
        }
        s.push(w);
    }
    lines.push(format!("path {} {}", s.join(" "), paint(it, part)));
}

/// Where Pele's text puts the first line of an item's label, with its fill.
fn label(lines: &mut Vec<String>, it: &Item) {
    let Some(p) = it.label.as_ref().filter(|p| !p.label.lines.is_empty()) else {
        return;
    };
    let l = &p.label;
    let y = p.at.y - l.height / 2.0 + l.line_height / 2.0 + l.size * 0.35;
    lines.push(format!(
        "t {} f={}",
        pt(p.at.x, y),
        paint_name(it.style.text)
    ));
}

/// The lines of a scene as the oracle reads Pele's SVG.
fn scene_lines(sc: &Scene) -> Vec<String> {
    let mut lines = vec![format!("size {}", pt(sc.width, sc.height))];
    let data = |it: &Item| id(it.id.as_deref().unwrap_or(""));
    let mut i = 0;
    while i < sc.items.len() {
        let it = &sc.items[i];
        match it.role.as_str() {
            ROLE_TITLE => {
                lines.push("title".into());
                label(&mut lines, it);
            }
            ROLE_LANE => {
                lines.push(format!("branch {}", data(it)));
                shape(&mut lines, it, &it.parts[0]);
            }
            ROLE_BRANCH => {
                shape(&mut lines, it, &it.parts[0]);
                label(&mut lines, it);
            }
            ROLE_EDGE => {
                let mut class = "pele-edge".to_string();
                if let Some(s) = &it.shape {
                    class += &format!(" pele-edge-{s}");
                }
                lines.push(format!("edge {class}"));
                shape(&mut lines, it, &it.parts[0]);
            }
            role @ (ROLE_COMMIT | ROLE_MERGE) => {
                let kind = it.shape.as_ref().map_or("", |s| s.as_str());
                let mut class = format!("pele-commit pele-commit-{kind}");
                if role == ROLE_MERGE && kind != "merge" {
                    class += " pele-commit-merge";
                }
                lines.push(format!("commit {} {class}", data(it)));
                if let Some(tip) = &it.tooltip {
                    lines.push(format!("tip {}", id(tip)));
                }
                for part in &it.parts {
                    shape(&mut lines, it, part);
                }
            }
            ROLE_COMMIT_MARK | ROLE_TAG_HOLE => {
                for part in &it.parts {
                    shape(&mut lines, it, part);
                }
            }
            ROLE_COMMIT_LABEL => {
                lines.push("label".into());
                for part in &it.parts {
                    shape(&mut lines, it, part);
                }
                match it.label.as_ref().filter(|p| p.rotate != 0.0) {
                    Some(p) => lines.push(format!(
                        "turned {} {} f={}",
                        pele_num(p.rotate),
                        pt(p.at.x, p.at.y),
                        paint_name(it.style.text)
                    )),
                    None => label(&mut lines, it),
                }
            }
            ROLE_TAG => {
                lines.push("tag".into());
                shape(&mut lines, it, &it.parts[0]);
                // Pele draws the hole, an item of its own after the tag, before the tag's
                // text.
                if let Some(hole) = sc.items.get(i + 1).filter(|h| h.role == ROLE_TAG_HOLE) {
                    shape(&mut lines, hole, &hole.parts[0]);
                    i += 1;
                }
                label(&mut lines, it);
            }
            role => lines.push(format!("unknown role {role}")),
        }
        i += 1;
    }
    lines
}

/// What Pele's render draws of git graphs, read back out of its SVG as a line for each thing
/// drawn, in the order drawn, against the same lines written of the scene.
#[test]
fn renders_match_pele() {
    let (f, drawings): (Renders, _) = limu_fixtures::load_drawings("gitgraph-render");
    assert!(f.cases.len() >= 3000, "only {} cases", f.cases.len());
    let mut failures = Failures::new();
    for (k, c) in f.cases.iter().enumerate() {
        let p = prepare(&c.src, &ParseOptions::default()).expect("a git graph");
        let m = p
            .parse_with(false, |s| super::parse(s, &p.config, p.title.as_deref()))
            .expect("a git graph that parses");
        let o = Options {
            max_width: c.max_width.unwrap_or(f64::INFINITY),
            font_size: c.font_size.unwrap_or(16.0),
            ..Options::default()
        };
        let got = scene_lines(&layout(&m, &p.config, &o));
        let diff = drawings.diff(k, &c.drawing, &got, None);
        failures.check(diff.is_none(), || {
            format!(
                "{:?}, max {:?}, size {:?}: {}",
                c.src,
                c.max_width,
                c.font_size,
                diff.unwrap()
            )
        });
    }
    failures.done("renders", 3000);
}
