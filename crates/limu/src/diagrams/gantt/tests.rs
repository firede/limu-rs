//! Gantt charts read, scheduled and laid out as Pele reads, schedules and draws them, checked
//! against the oracle's gantt, gantt-dates and gantt-render fixtures, each in the time zones
//! it was written in.

use std::collections::HashMap;
use std::time::{Duration, SystemTime};

use limu_fixtures::{Drawing, Num, load, load_drawings};
use serde::Deserialize;

use super::axis::{auto_ticks, interval_ticks, time_format};
use super::dates::{format, parse_strict};
use super::db::{GanttDb, Moment};
use super::parse;
use super::parser::parse_gantt;
use super::render::{
    ROLE_AXIS, ROLE_EXCLUDED, ROLE_GRID, ROLE_SECTION, ROLE_SECTION_LABEL, ROLE_TASK, ROLE_TICK,
    ROLE_TITLE, ROLE_TODAY, ROLE_VERT, ROLE_VERT_LABEL, layout,
};
use crate::detect::DiagramType;
use crate::errors::{Error, ErrorCode};
use crate::js::date::Zone;
use crate::layout::Options;
use crate::prepare::{ParseOptions, prepare};
use crate::scene::style::Paint;
use crate::scene::{Item, Op, Outline, Path, Scene};
use crate::testing::pelesvg::{id, own_fill};
use crate::testing::tzif::{Tzif, zone};
use crate::testing::{Enc, Failures, PeleError, pele_num, same, sorted};
use crate::value::Value;

/// A value Pele leaves undefined, as the oracle writes it.
fn undefined() -> Value {
    object(vec![("$", Value::String("undefined".into()))])
}

fn object(entries: Vec<(&str, Value)>) -> Value {
    Value::Object(
        entries
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect(),
    )
}

fn string(s: &str) -> Value {
    Value::String(s.to_string())
}

/// A string Pele may leave undefined.
fn optional(s: &Option<String>) -> Value {
    s.as_deref().map_or_else(undefined, string)
}

fn strings(s: &[String]) -> Value {
    Value::Array(s.iter().map(|s| string(s)).collect())
}

/// The chart's model as the oracle writes Pele's, field by field.
fn dump_model(db: &GanttDb) -> Value {
    let links = db
        .links
        .iter()
        .map(|(id, href)| Value::Array(vec![string(id), string(href)]))
        .collect();
    let tasks = db
        .tasks
        .iter()
        .map(|t| {
            object(vec![
                ("id", string(&t.id)),
                ("task", string(&t.task)),
                ("section", string(&t.section)),
                (
                    "flags",
                    Value::Array(
                        [t.active, t.done, t.crit, t.milestone, t.vert]
                            .map(Value::Bool)
                            .to_vec(),
                    ),
                ),
                ("order", Value::Number(t.order as f64)),
                ("classes", strings(&t.classes)),
                ("prevTaskId", optional(&t.prev_task_id)),
                (
                    "raw",
                    Value::Array(vec![
                        string(&t.raw.data),
                        optional(&t.raw.start),
                        string(&t.raw.end),
                    ]),
                ),
            ])
        })
        .collect();
    object(vec![
        ("title", optional(&db.title)),
        ("accTitle", optional(&db.acc_title)),
        ("accDescr", optional(&db.acc_descr)),
        ("dateFormat", string(&db.date_format)),
        ("axisFormat", string(&db.axis_format)),
        ("tickInterval", optional(&db.tick_interval)),
        ("todayMarker", string(&db.today_marker)),
        ("includes", strings(&db.includes)),
        ("excludes", strings(&db.excludes)),
        ("inclusiveEndDates", Value::Bool(db.inclusive_end_dates)),
        ("topAxis", Value::Bool(db.top_axis)),
        ("displayMode", string(&db.display_mode)),
        ("weekday", optional(&db.weekday)),
        ("weekend", string(&db.weekend)),
        ("sections", strings(&db.sections)),
        ("links", Value::Array(links)),
        ("tasks", Value::Array(tasks)),
    ])
}

fn moment(m: Moment) -> Value {
    m.map_or_else(undefined, Value::Number)
}

/// An error as the oracle writes Pele's.
fn dump_error(e: &Error) -> Value {
    object(vec![
        ("code", string(e.code.as_str())),
        ("message", string(&e.message)),
        ("type", e.diagram.map_or(Value::Null, |d| string(d.name()))),
        ("line", Value::Number(e.line as f64)),
        ("column", Value::Number(e.column as f64)),
        ("snippet", string(&e.snippet)),
    ])
}

/// The tasks a chart schedules in a zone against a time, as the oracle writes them.
fn dump_schedule(db: &GanttDb, now: f64, tz: &Tzif) -> Value {
    match db.schedule(now, tz) {
        Err(e) => object(vec![("error", dump_error(&e))]),
        Ok(s) => {
            let tasks = s
                .tasks
                .iter()
                .map(|t| {
                    Value::Array(vec![
                        moment(t.start_time),
                        moment(t.end_time),
                        t.render_end_time.map_or(Value::Null, Value::Number),
                        Value::Bool(t.processed),
                        Value::Bool(t.manual_end_time),
                    ])
                })
                .collect();
            object(vec![("tasks", Value::Array(tasks))])
        }
    }
}

#[derive(Deserialize)]
struct Cases {
    zones: Vec<String>,
    cases: Vec<Case>,
}

#[derive(Deserialize, Default)]
struct Result_ {
    model: Option<Enc>,
    schedules: Option<Vec<Enc>>,
    error: Option<Enc>,
}

#[derive(Deserialize)]
struct Case {
    src: String,
    now: f64,
    #[serde(rename = "type")]
    diagram: Option<String>,
    direct: Result_,
    #[serde(default)]
    same: bool,
    model: Option<Enc>,
    schedules: Option<Vec<Enc>>,
    error: Option<Enc>,
}

/// Whether a parse and the schedules of what it parsed in each zone are Pele's. The schedules
/// are left out where schedule is not set: a model that matches one already scheduled
/// schedules as it did.
fn check(
    got: &Result<GanttDb, Error>,
    want: (&Option<Enc>, &Option<Vec<Enc>>, &Option<Enc>),
    now: f64,
    zones: &[Tzif],
    schedule: bool,
) -> Result<(), String> {
    let (model, schedules, error) = want;
    match (got, error) {
        (Err(e), Some(want)) => {
            if same(&sorted(&dump_error(e)), &sorted(&want.0)) {
                Ok(())
            } else {
                Err(format!("error {:?}, Pele {:?}", dump_error(e), want.0))
            }
        }
        (Err(e), None) => Err(format!("error {e:?}, Pele none")),
        (Ok(_), Some(want)) => Err(format!("no error, Pele {:?}", want.0)),
        (Ok(db), None) => {
            let got = dump_model(db);
            let want = &model.as_ref().ok_or("no model")?.0;
            if !same(&sorted(&got), &sorted(want)) {
                return Err(format!("model {got:?}\nPele {want:?}"));
            }
            if !schedule {
                return Ok(());
            }
            let schedules = schedules.as_ref().ok_or("no schedules")?;
            for (tz, want) in zones.iter().zip(schedules) {
                let got = dump_schedule(db, now, tz);
                if !same(&sorted(&got), &sorted(&want.0)) {
                    return Err(format!("schedule {got:?}\nPele {:?}", want.0));
                }
            }
            Ok(())
        }
    }
}

#[test]
fn parse_matches_pele() {
    let f: Cases = load("gantt");
    let zones: Vec<Tzif> = f.zones.iter().map(|z| zone(z)).collect();
    let mut failures = Failures::new();
    for c in &f.cases {
        let mut db = GanttDb::default();
        let direct = parse_gantt(&c.src, &mut db).map(|_| db);
        let d = &c.direct;
        let r = check(
            &direct,
            (&d.model, &d.schedules, &d.error),
            c.now,
            &zones,
            true,
        );
        let ok = r.is_ok();
        failures.check(ok, || {
            format!("parse_gantt({:?}): {}", c.src, r.unwrap_err())
        });
        if !ok {
            continue;
        }
        let want = if c.same {
            (&d.model, &d.schedules, &d.error)
        } else {
            (&c.model, &c.schedules, &c.error)
        };
        let r = match prepare(&c.src, &ParseOptions::default()) {
            Err(e) if e.code == ErrorCode::UnsupportedDiagram && want.2.is_none() => Ok(()),
            Err(e) => match want.2 {
                Some(w) if same(&sorted(&dump_error(&e)), &sorted(&w.0)) => Ok(()),
                _ => Err(format!("prepare: {e:?}, Pele {:?}", want.2)),
            },
            Ok(p) if c.diagram.as_deref() != Some(p.found.diagram.name()) => {
                Err(format!("type {}, Pele {:?}", p.found.diagram, c.diagram))
            }
            Ok(p) if p.found.diagram == DiagramType::Gantt => {
                let got = p.parse_with(false, |s| parse(s, &p.config, p.title.as_deref()));
                check(&got, want, c.now, &zones, !c.same)
            }
            Ok(_) => Ok(()),
        };
        failures.check(r.is_ok(), || {
            format!("parse({:?}): {}", c.src, r.unwrap_err())
        });
    }
    failures.done("gantt", 2 * 5000);
}

#[derive(Deserialize)]
struct DateZones {
    zones: Vec<DateCases>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DateCases {
    zone: String,
    now: f64,
    formats: Vec<(f64, String, Enc)>,
    parses: Vec<(String, String, Option<f64>)>,
    time_formats: Vec<(f64, String, String)>,
    ticks: Vec<(f64, f64, f64, String, String, Enc)>,
}

/// The date code on its own, in each zone.
#[test]
fn dates_match_pele() {
    let f: DateZones = load("gantt-dates");
    let mut failures = Failures::new();
    for zc in &f.zones {
        let tz = zone(&zc.zone);
        let z = Zone(&tz);
        let name = &zc.zone;
        for (date, template, want) in &zc.formats {
            let got = format(z, *date, template).map_or_else(undefined, |s| string(&s));
            failures.check(same(&got, &want.0), || {
                format!(
                    "{name}: format({date}, {template:?}) = {got:?}, Pele {:?}",
                    want.0
                )
            });
        }
        for (text, template, want) in &zc.parses {
            let got = parse_strict(z, text, template, zc.now);
            let ok = match (got, want) {
                (Some(a), Some(b)) => Num::same(a, *b),
                (None, None) => true,
                _ => false,
            };
            failures.check(ok, || {
                format!("{name}: parseStrict({text:?}, {template:?}) = {got:?}, Pele {want:?}")
            });
        }
        for (date, spec, want) in &zc.time_formats {
            let got = time_format(z, spec, *date);
            failures.check(got == *want, || {
                format!("{name}: timeFormat({spec:?}, {date}) = {got:?}, Pele {want:?}")
            });
        }
        for (start, stop, count, interval, weekday, want) in &zc.ticks {
            let ticks = if interval.is_empty() {
                Some(auto_ticks(z, *start, *stop, *count))
            } else {
                interval_ticks(z, interval, weekday, *start, *stop, 200.0)
            };
            let got = ticks.map_or_else(undefined, |t| {
                Value::Array(t.into_iter().map(Value::Number).collect())
            });
            failures.check(same(&got, &want.0), || {
                format!(
                    "{name}: ticks({start}, {stop}, {count}, {interval:?}, {weekday:?}) = \
                     {got:?}, Pele {:?}",
                    want.0
                )
            });
        }
    }
    failures.done("gantt-dates", 4 * 4000);
}

#[derive(Deserialize)]
struct Renders {
    zones: Vec<String>,
    cases: Vec<Render>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Render {
    src: String,
    zone: String,
    now: f64,
    max_width: Option<f64>,
    font_size: Option<f64>,
    error: Option<PeleError>,
    #[serde(flatten)]
    drawing: Drawing,
}

/// A paint as Pele's SVG names it, "-" for none given.
fn paint_name(p: Option<Paint>) -> String {
    let Some(p) = p else {
        return "-".into();
    };
    let names = [
        (Paint::BG, "var(--_bg)"),
        (Paint::FG, "var(--_fg)"),
        (Paint::MUTED, "var(--_m)"),
        (Paint::LINE, "var(--_l)"),
        (Paint::SURFACE, "var(--_s)"),
        (Paint::SURFACE_ALT, "var(--_a)"),
        (Paint::BORDER, "var(--_b)"),
        (Paint::ACCENT, "var(--_c)"),
        (Paint::ACCENT.with_opacity(0.25), "var(--_c)/0.25"),
        (Paint::CRITICAL, "var(--pele-critical,#e45756)"),
        (
            Paint::CRITICAL.with_opacity(0.25),
            "var(--pele-critical,#e45756)/0.25",
        ),
        (Paint::NONE, "none"),
    ];
    names
        .iter()
        .find(|(q, _)| *q == p)
        .map_or("?", |(_, s)| s)
        .to_string()
}

/// How an item's lines end and turn their corners, as the oracle reads Pele's stroke-linecap
/// and stroke-linejoin: the author's or the diagram's, or "-" for neither.
fn pen(it: &Item) -> String {
    let cap = it.author.cap.or(it.style.cap);
    let join = it.author.join.or(it.style.join);
    format!(
        "{} {}",
        cap.map_or("-".to_string(), |c| c.to_string()),
        join.map_or("-".to_string(), |j| j.to_string())
    )
}

/// A string as JavaScript's JSON.stringify writes it.
fn quote(s: &str) -> String {
    crate::js::json::stringify(&Value::String(s.to_string()))
}

/// The lines of a path as the oracle reads Pele's, one for each run from a move.
fn paths(lines: &mut Vec<String>, p: &Path, paint: &str) {
    let pt = |x: f64, y: f64| format!("{},{}", pele_num(x), pele_num(y));
    let mut cur: Vec<String> = Vec::new();
    let flush = |cur: &mut Vec<String>, lines: &mut Vec<String>| {
        if !cur.is_empty() {
            lines.push(format!("path {} {paint}", cur.join(" ")));
        }
        cur.clear();
    };
    let mut at = 0;
    for &op in p.ops() {
        let a = &p.args()[at..at + op.args()];
        at += op.args();
        if op == Op::MoveTo {
            flush(&mut cur, lines);
        }
        let mut s = match op {
            Op::MoveTo => "M",
            Op::LineTo => "L",
            Op::QuadTo => "Q",
            Op::CubeTo => "C",
            Op::Close => "Z",
        }
        .to_string();
        for xy in a.chunks(2) {
            s.push(' ');
            s.push_str(&pt(xy[0], xy[1]));
        }
        cur.push(s);
    }
    flush(&mut cur, lines);
}

/// The path of a part.
fn path_of(it: &Item, k: usize) -> &Path {
    match &it.parts[k].outline {
        Outline::Path(p) => p,
        Outline::Box { .. } => panic!("a box where a path was looked for"),
    }
}

/// A scene written as the oracle reads Pele's SVG.
fn scene_lines(sc: &Scene) -> Vec<String> {
    let num = pele_num;
    let pt = |x: f64, y: f64| format!("{},{}", num(x), num(y));
    let mut lines = vec![format!("size {}", pt(sc.width, sc.height))];
    // Where Pele's text puts the first line of a label centered on its point.
    let first_line = |it: &Item| {
        let p = it.label.as_ref().expect("a label");
        let l = &p.label;
        pt(
            p.at.x,
            p.at.y - l.height / 2.0 + l.line_height / 2.0 + l.size * 0.35,
        )
    };
    let text_at = |it: &Item| format!("t {} {}", first_line(it), paint_name(it.style.text));
    let sized = |it: &Item| {
        let size = it.label.as_ref().map_or(0.0, |p| p.label.size);
        format!("{} s{}", text_at(it), num(size))
    };
    for it in &sc.items {
        let ident = id(it.id.as_deref().unwrap_or(""));
        match it.role.as_str() {
            ROLE_TITLE => lines.push(format!("title {}", text_at(it))),
            ROLE_SECTION | ROLE_EXCLUDED => {
                let r = it.bounds;
                let one = it.parts.len() == 1
                    && matches!(it.parts[0].outline, Outline::Box { rect, .. } if rect == r);
                if it.role.as_str() == ROLE_SECTION {
                    if !one || it.style.stroke != Some(Paint::NONE) {
                        lines.push("band not one box".into());
                    }
                    let index = it.tags.join(" ");
                    let index = index.strip_prefix("section-").unwrap_or(&index);
                    lines.push(format!(
                        "band {index} {},{} {} -",
                        pt(r.x, r.y),
                        pt(r.w, r.h),
                        paint_name(own_fill(it))
                    ));
                } else {
                    if !one || it.style.fill != Some(Paint::SURFACE_ALT) {
                        lines.push("excluded not one box".into());
                    }
                    lines.push(format!("excluded {},{}", pt(r.x, r.y), pt(r.w, r.h)));
                }
            }
            ROLE_GRID | ROLE_AXIS => {
                lines.push(it.role.as_str()["gantt.".len()..].to_string());
                let paint = format!("- {} {}", paint_name(it.style.stroke), pen(it));
                paths(&mut lines, path_of(it, 0), &paint);
            }
            ROLE_VERT => {
                lines.push(format!("vert {ident}"));
                paths(&mut lines, path_of(it, 0), &format!("- - {}", pen(it)));
            }
            ROLE_TODAY => {
                // The author's style of the line, which Pele writes as it is, takes the place
                // of the triangle.
                let styled = it.parts.len() == 1;
                lines.push(if styled { "today styled" } else { "today -" }.into());
                let paint = format!("none {} {}", paint_name(it.style.stroke), pen(it));
                paths(&mut lines, path_of(it, 0), &paint);
                if !styled {
                    let paint = format!("{} - {}", paint_name(own_fill(it)), pen(it));
                    paths(&mut lines, path_of(it, 1), &paint);
                }
            }
            ROLE_TASK => {
                lines.push(format!("task {ident} [{}]", it.tags.join(" ")));
                if let Some(link) = &it.link {
                    lines.push(format!("link {link}"));
                }
                let paint = format!(
                    "{} {}",
                    paint_name(own_fill(it)),
                    paint_name(it.style.stroke)
                );
                for (k, part) in it.parts.iter().enumerate() {
                    match &part.outline {
                        Outline::Path(_) => {
                            paths(&mut lines, path_of(it, k), &format!("{paint} {}", pen(it)));
                        }
                        Outline::Box { rect: r, rounded } => {
                            let r = format!(
                                "rect {},{}{}",
                                pt(r.x, r.y),
                                pt(r.w, r.h),
                                if *rounded { " r" } else { "" }
                            );
                            lines.push(format!("{r} {paint}"));
                        }
                    }
                }
                if it.label.is_some() {
                    lines.push(sized(it));
                }
            }
            ROLE_SECTION_LABEL => {
                lines.push(format!("section {ident}"));
                lines.push(sized(it));
            }
            ROLE_VERT_LABEL => {
                lines.push(format!("vert-label {ident}"));
                lines.push(sized(it));
            }
            ROLE_TICK => {
                let l = &it.label.as_ref().expect("a label").label;
                if l.lines.len() != 1
                    || l.lines[0].len() != 1
                    || it.style.text != Some(Paint::MUTED)
                {
                    lines.push("tick not one span".into());
                }
                lines.push(format!(
                    "tick {} s{} {}",
                    first_line(it),
                    num(l.size),
                    quote(&id(&l.lines[0][0].text))
                ));
            }
            role => lines.push(format!("unknown role {role}")),
        }
    }
    lines
}

/// What Pele's render draws of gantt charts, each in one of several time zones against a
/// fixed time, read back out of its SVG as a line for each thing drawn, in the order drawn,
/// against the same lines written of the scene.
#[test]
fn renders_match_pele() {
    let (f, drawings): (Renders, _) = load_drawings("gantt-render");
    assert!(f.cases.len() >= 3000, "only {} cases", f.cases.len());
    let zones: HashMap<&str, Tzif> = f.zones.iter().map(|z| (z.as_str(), zone(z))).collect();
    let mut failures = Failures::new();
    for (k, c) in f.cases.iter().enumerate() {
        let p = prepare(&c.src, &ParseOptions::default()).expect("a gantt chart");
        let db = p
            .parse_with(false, |s| parse(s, &p.config, p.title.as_deref()))
            .expect("a gantt chart that parses");
        let now = if c.now < 0.0 {
            SystemTime::UNIX_EPOCH - Duration::from_millis(-c.now as u64)
        } else {
            SystemTime::UNIX_EPOCH + Duration::from_millis(c.now as u64)
        };
        let o = Options {
            max_width: c.max_width.unwrap_or(f64::INFINITY),
            font_size: c.font_size.unwrap_or(16.0),
            now: Some(now),
            time_zone: &zones[c.zone.as_str()],
            ..Options::default()
        };
        let what = || {
            format!(
                "{:?} in {} at {}, max {:?}, size {:?}",
                c.src, c.zone, c.now, c.max_width, c.font_size
            )
        };
        match (layout(&db, &p.config, &o), &c.error) {
            (Ok(_), Some(want)) => {
                failures.check(false, || format!("{}: no error, Pele {want:?}", what()))
            }
            (Err(e), Some(want)) => {
                // Entity codes are written back as written, as Diagram::layout does.
                let e = e.tagged(p.found);
                let ok = e.code.as_str() == want.code && e.message == want.message;
                failures.check(ok, || format!("{}: {e:?}, Pele {want:?}", what()));
            }
            (Err(e), None) => failures.check(false, || format!("{}: {e:?}", what())),
            (Ok(sc), None) => {
                let got = scene_lines(&sc);
                let diff = drawings.diff(k, &c.drawing, &got, None);
                failures.check(diff.is_none(), || format!("{}: {}", what(), diff.unwrap()));
            }
        }
    }
    failures.done("gantt-render", 6000);
}

/// A chart laid out through the crate's API, with the host's time and zone.
#[test]
fn lays_out_in_the_hosts_zone() {
    let src = "gantt\n    dateFormat YYYY-MM-DD\n    section A\n    One :a1, 2024-03-09, 3d\n";
    let d = crate::parse(src, &ParseOptions::default()).expect("a gantt chart");
    let o = Options {
        now: Some(SystemTime::UNIX_EPOCH + Duration::from_millis(1710000000000)),
        time_zone: &crate::layout::FixedOffset(-5 * 3600),
        ..Options::default()
    };
    let sc = d.layout(&o).expect("a scene");
    assert_eq!(sc.diagram, "gantt");
    assert!(sc.items.iter().any(|it| it.role.as_str() == ROLE_TASK));
}
