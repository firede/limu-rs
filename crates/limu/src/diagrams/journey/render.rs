//! A user journey laid out as a scene, as Pele's renderJourney in render.ts draws it.

use std::collections::HashMap;

use super::JourneyDb;
use super::db::{JourneyTask, journey_actors};
use crate::common::{fit_label, line, plain_label, setting, turn_to_fit};
use crate::js::chars::trim;
use crate::js::math::{max, min, round};
use crate::layout::Options;
use crate::scene::style::{Cap, Join, Paint, Style};
use crate::scene::text::{Face, Label};
use crate::scene::{
    Align, Edge, Item, Kind, Painting, Part, Path, Placed, Point, Rect, Scene, Shape,
};
use crate::text::Setting;
use crate::value::Map;

/// The roles of the items of a user journey: its title, the lines of the chart of scores and
/// the scores beside them, the dashed stems from the cards to the faces, the line through the
/// faces, or between the cards running down, a face, a section's band with its label, a
/// section's label on its own running down, a task's card, the mark of an actor on a task,
/// a score written beside a face running down, and an entry of the legend of actors.
pub(crate) const ROLE_TITLE: &str = "journey.title";
pub(crate) const ROLE_GRID: &str = "journey.grid";
pub(crate) const ROLE_SCALE: &str = "journey.scale";
pub(crate) const ROLE_STEM: &str = "journey.stem";
pub(crate) const ROLE_LINE: &str = "journey.line";
pub(crate) const ROLE_FACE: &str = "journey.face";
pub(crate) const ROLE_SECTION: &str = "journey.section";
pub(crate) const ROLE_SECTION_LABEL: &str = "journey.section-label";
pub(crate) const ROLE_TASK: &str = "journey.task";
pub(crate) const ROLE_ACTOR: &str = "journey.actor";
pub(crate) const ROLE_SCORE: &str = "journey.score";
pub(crate) const ROLE_LEGEND: &str = "journey.legend";

// The measures of a user journey, as Pele's render.ts has them.
const PAD_X: f64 = 10.0;
const PAD_Y: f64 = 8.0;
const MIN_TEXT: f64 = 124.0;
const MAX_TEXT: f64 = 200.0;
const GAP: f64 = 12.0;
const BAND_PAD: f64 = 6.0;
const GUTTER: f64 = 24.0;
const LEVEL: f64 = 26.0;
const FACE: f64 = 11.0;
const DOT: f64 = 5.0;
const DOT_STEP: f64 = 13.0;
const LEGEND_GAP: f64 = 20.0;

/// Lays a user journey out as a scene, as Pele's renderJourney draws it: across, the tasks
/// as cards in a row with each one's score as a face on a chart under them, or down, where
/// the host's width calls for it and allows it.
pub(crate) fn layout(db: &JourneyDb, _config: &Map, o: &Options) -> Scene {
    turn_to_fit(o, |down| {
        if down {
            vertical_journey(db, o)
        } else {
            draw(db, o)
        }
    })
}

/// The expression of a face for a score, as Pele chooses its mouth: a score that is not a
/// number is neutral.
fn mood(score: f64) -> &'static str {
    if score > 3.0 {
        "happy"
    } else if score < 3.0 {
        "sad"
    } else {
        "neutral"
    }
}

/// A circle about x, y.
fn circle(x: f64, y: f64, r: f64) -> Path {
    let mut p = Path::new();
    p.ellipse(x, y, r, r);
    p
}

/// A face about x, y, as Pele's face draws it: a ring, two eyes and a mouth, in the color of
/// lines. Pele draws the mouth with round caps. The face is given them whole, as they show
/// nowhere else on it: its ring is closed and its eyes are not stroked.
fn face(x: f64, y: f64, mood: &'static str) -> Item {
    let mut mouth = Path::new();
    match mood {
        "happy" => {
            mouth
                .move_to(x + -5.0, y + 2.0)
                .arc_to(6.0, 6.0, 0.0, false, false, x + 5.0, y + 2.0);
        }
        "sad" => {
            mouth
                .move_to(x + -5.0, y + 6.5)
                .arc_to(6.0, 6.0, 0.0, false, true, x + 5.0, y + 6.5);
        }
        _ => {
            mouth.move_to(x + -4.5, y + 4.5).line_to(x + 4.5, y + 4.5);
        }
    }
    let mut it = Item::new(Kind::Mark, ROLE_FACE);
    it.shape = Some(Shape::new(mood));
    it.bounds = Rect::new(x - FACE, y - FACE, 2.0 * FACE, 2.0 * FACE);
    it.parts = vec![
        Part::path(circle(x, y, FACE), Painting::Hollow),
        Part::path(circle(x + -3.6, y + -2.8, 1.3), Painting::FillOnly),
        Part::path(circle(x + 3.6, y + -2.8, 1.3), Painting::FillOnly),
        Part::path(mouth, Painting::StrokeOnly),
    ];
    it.style = Style {
        fill: Some(Paint::LINE),
        stroke: Some(Paint::LINE),
        cap: Some(Cap::Round),
        ..Style::default()
    };
    it
}

/// Everyone named on a task but the actor with no name, and the place of each, which is the
/// series it is drawn in.
fn actor_names(tasks: &[JourneyTask]) -> (Vec<String>, HashMap<String, usize>) {
    let names: Vec<String> = journey_actors(tasks)
        .into_iter()
        .filter(|name| !name.is_empty())
        .collect();
    let index = names
        .iter()
        .enumerate()
        .map(|(i, name)| (name.clone(), i))
        .collect();
    (names, index)
}

/// A task's people but the actor with no name.
fn named(people: &[String]) -> Vec<&String> {
    people.iter().filter(|name| !name.is_empty()).collect()
}

/// A label on its own, of one line of text Pele writes as it is, centered on a point.
fn text_item(
    role: &'static str,
    s: &str,
    set: &Setting,
    size: f64,
    at: Point,
    align: Align,
    paint: Option<Paint>,
) -> Item {
    let label = line(
        s,
        set.width(s, size, Face::default()),
        size,
        Face::default(),
    );
    let mut it = Item::new(Kind::Text, role);
    it.label = Some(Placed {
        align,
        ..Placed::new(label, at)
    });
    it.style.text = paint;
    it
}

/// A label placed about a point, where it holds any text.
fn placed(label: Label, at: Point) -> Option<Placed> {
    (!label.is_empty()).then(|| Placed::new(label, at))
}

/// An entry of the legend, as Pele's draw lays it out.
struct Entry {
    name: String,
    label: Label,
    x: f64,
    y: f64,
    h: f64,
}

/// A run of tasks that share a section, with the section's label where it has a name.
struct Run {
    start: usize,
    count: usize,
    label: Option<Label>,
}

/// Lays the journey out across, as Pele's draw does where down is not set.
fn draw(db: &JourneyDb, o: &Options) -> Scene {
    let size = o.font_size;
    let small = round(size * 0.875);
    let tiny = round(size * 0.75);
    let set = setting(o);
    let pad = o.pad();
    let tasks = &db.tasks;
    let n = tasks.len();

    // `Task: 5:` names an actor with no name. Mermaid gives it a dot and an empty legend row.
    let (names, actor) = actor_names(tasks);
    let series = |name: &str| Paint::series(actor.get(name).copied().unwrap_or(0));

    // Cards widen, up to a point, for a word that would not fit. Past that the word is cut.
    let mut text_w = MIN_TEXT;
    let mut labels: Vec<Label> = tasks
        .iter()
        .map(|t| plain_label(&t.task, &set, small, text_w, Face::default()))
        .collect();
    let mut widest = 0.0;
    for label in &labels {
        widest = max(widest, label.width);
    }
    if widest > text_w {
        text_w = min(widest.ceil(), MAX_TEXT);
        labels = tasks
            .iter()
            .map(|t| fit_label(&t.task, &set, small, text_w, Face::default()))
            .collect();
    }

    let card_w = text_w + 2.0 * PAD_X;
    let pitch = card_w + GAP;
    let plot_w = if n > 0 { n as f64 * pitch - GAP } else { 0.0 };

    let line_h = round(small * 1.5);
    let limit = max(plot_w, 240.0);
    let mut legend: Vec<Entry> = Vec::new();
    let (mut legend_w, mut legend_h, mut lx) = (0.0, 0.0, 0.0);
    for name in &names {
        let label = fit_label(name, &set, small, limit - 2.0 * DOT - 6.0, Face::default());
        let w = 2.0 * DOT + 6.0 + label.width;
        if lx > 0.0 && lx + w > limit {
            lx = 0.0;
            legend_h += 4.0;
        }
        let y = match legend.last() {
            Some(last) if lx > 0.0 => last.y,
            _ => legend_h,
        };
        let h = max(label.height, line_h);
        legend.push(Entry {
            name: name.clone(),
            label,
            x: lx,
            y,
            h,
        });
        lx += w;
        legend_w = max(legend_w, lx);
        legend_h = max(legend_h, y + h);
        lx += LEGEND_GAP;
    }

    let body_w = if n > 0 { GUTTER + plot_w } else { 0.0 };
    let title = fit_label(
        db.title.as_deref().unwrap_or(""),
        &set,
        size,
        max(body_w, 320.0),
        Face::BOLD,
    );
    let title_h = if title.height > 0.0 {
        title.height + 16.0
    } else {
        0.0
    };
    let legend_row = if n > 0 { GUTTER + legend_w } else { 0.0 };
    let width = max(max(body_w, title.width), legend_row);
    let left = pad + (width - body_w) / 2.0 + GUTTER;
    let top = pad + title_h;

    // A band covers a run of tasks that share a section, as in Mermaid. A section with no
    // tasks draws nothing.
    let mut runs: Vec<Run> = Vec::new();
    let mut band_h = 0.0;
    let mut i = 0;
    while i < n {
        let mut end = i + 1;
        while end < n && tasks[end].section == tasks[i].section {
            end += 1;
        }
        let mut label = None;
        if !tasks[i].section.is_empty() {
            let room = (end - i) as f64 * pitch - GAP - 2.0 * PAD_X;
            let l = fit_label(&tasks[i].section, &set, small, room, Face::BOLD);
            band_h = max(band_h, max(l.height, line_h) + 2.0 * BAND_PAD);
            label = Some(l);
        }
        runs.push(Run {
            start: i,
            count: end - i,
            label,
        });
        i = end;
    }

    let mut text_h = line_h;
    for label in &labels {
        text_h = max(text_h, label.height);
    }
    let dots_h = if names.is_empty() {
        0.0
    } else {
        2.0 * DOT + 6.0
    };
    let card_h = text_h + dots_h + 2.0 * PAD_Y;
    let cards_y = top + if band_h > 0.0 { band_h + BAND_PAD } else { 0.0 };
    let chart_top = cards_y + card_h + FACE + 16.0;
    let chart_bottom = chart_top + 4.0 * LEVEL;
    let right = left + plot_w;

    let mut items = Vec::new();
    let mut bottom = top;
    if n > 0 {
        let mut grid = Path::new();
        let mut scale = Vec::new();
        for score in (1..=5).rev() {
            let y = chart_top + f64::from(5 - score) * LEVEL;
            grid.move_to(left - 6.0, y).line_to(right, y);
            let at = Point::new(left - 12.0, y);
            let text = score.to_string();
            scale.push(text_item(
                ROLE_SCALE,
                &text,
                &set,
                tiny,
                at,
                Align::End,
                Some(Paint::MUTED),
            ));
        }
        let mut it = Item::new(Kind::Mark, ROLE_GRID);
        it.bounds = grid.bounds();
        it.parts = vec![Part::path(grid, Painting::StrokeOnly)];
        it.style.stroke = Some(Paint::SURFACE_ALT);
        items.push(it);
        items.extend(scale);

        let mut path = Path::new();
        let mut route = Vec::new();
        let mut stems = Path::new();
        let mut faces = Vec::new();
        let mut cards: Vec<Vec<Item>> = Vec::with_capacity(n);
        for (i, task) in tasks.iter().enumerate() {
            let x = left + i as f64 * pitch;
            let cx = x + card_w / 2.0;
            let score = task.score;
            let level = if score.is_nan() {
                3.0
            } else {
                min(5.0, max(1.0, score))
            };
            let y = chart_top + (5.0 - level) * LEVEL;
            if i == 0 {
                path.move_to(cx, y);
            } else {
                path.line_to(cx, y);
            }
            route.push(Point::new(cx, y));
            stems.move_to(cx, cards_y + card_h).line_to(cx, y - FACE);
            faces.push(face(cx, y, mood(score)));

            let mut card = Item::new(Kind::Node, ROLE_TASK);
            card.shape = Some(Shape::new("rect"));
            card.id = Some(trim(&task.task).to_string());
            card.bounds = Rect::new(x, cards_y, card_w, card_h);
            card.parts = vec![Part::rect(card.bounds, true, Painting::FillAndStroke)];
            card.style = Style {
                fill: Some(Paint::SURFACE),
                stroke: Some(Paint::BORDER),
                ..Style::default()
            };
            let at = Point::new(cx, cards_y + PAD_Y + text_h / 2.0);
            card.label = placed(labels[i].clone(), at);
            let mut members = vec![card];
            let people = named(&task.people);
            let k = people.len() as f64;
            let step = min(
                DOT_STEP,
                (card_w - 2.0 * PAD_X - 2.0 * DOT) / max(k - 1.0, 1.0),
            );
            for (k, &name) in people.iter().enumerate() {
                let dx = cx + (k as f64 - (people.len() as f64 - 1.0) / 2.0) * step;
                let dy = cards_y + card_h - PAD_Y - DOT;
                let mut it = Item::new(Kind::Mark, ROLE_ACTOR);
                it.shape = Some(Shape::new("circle"));
                it.id = Some(name.clone());
                it.tooltip = Some(name.clone());
                it.bounds = Rect::new(dx - DOT, dy - DOT, 2.0 * DOT, 2.0 * DOT);
                it.parts = vec![Part::path(circle(dx, dy, DOT), Painting::FillAndStroke)];
                it.style = Style {
                    fill: Some(series(name)),
                    stroke: Some(Paint::SURFACE),
                    ..Style::default()
                };
                members.push(it);
            }
            cards.push(members);
        }
        let mut it = Item::new(Kind::Mark, ROLE_STEM);
        it.bounds = stems.bounds();
        it.parts = vec![Part::path(stems, Painting::StrokeOnly)];
        it.style = Style {
            stroke: Some(Paint::BORDER),
            dash: Some(vec![2.0, 3.0]),
            ..Style::default()
        };
        items.push(it);
        if n > 1 {
            // Pele turns the line's corners round.
            let mut it = Item::new(
                Kind::Edge(Edge {
                    route,
                    ends: [None, None],
                }),
                ROLE_LINE,
            );
            it.bounds = path.bounds();
            it.parts = vec![Part::path(path, Painting::StrokeOnly)];
            it.style = Style {
                stroke: Some(Paint::LINE),
                join: Some(Join::Round),
                ..Style::default()
            };
            items.push(it);
        }
        items.extend(faces);

        let mut cards = cards.into_iter();
        for run in runs {
            if let Some(label) = run.label {
                let x = left + run.start as f64 * pitch;
                let w = run.count as f64 * pitch - GAP;
                let mut band = Item::new(Kind::Group, ROLE_SECTION);
                band.shape = Some(Shape::new("rect"));
                band.id = Some(trim(&tasks[run.start].section).to_string());
                band.bounds = Rect::new(x, top, w, band_h);
                band.parts = vec![Part::rect(band.bounds, true, Painting::FillOnly)];
                band.label = placed(label, Point::new(x + w / 2.0, top + band_h / 2.0));
                band.style.fill = Some(Paint::SURFACE_ALT);
                items.push(band);
            }
            for members in cards.by_ref().take(run.count) {
                items.extend(members);
            }
        }
        bottom = chart_bottom + FACE;

        if !legend.is_empty() {
            let x0 = left + (plot_w - legend_w) / 2.0;
            let y0 = bottom + 16.0;
            for e in legend {
                let cy = y0 + e.y + e.h / 2.0;
                let mut it = Item::new(Kind::Node, ROLE_LEGEND);
                it.shape = Some(Shape::new("circle"));
                it.bounds = Rect::new(
                    x0 + e.x,
                    cy - e.h / 2.0,
                    2.0 * DOT + 6.0 + e.label.width,
                    e.h,
                );
                let dot = circle(x0 + e.x + DOT, cy, DOT);
                it.parts = vec![Part::path(dot, Painting::FillOnly)];
                it.style.fill = Some(series(&e.name));
                let at = Point::new(x0 + e.x + 2.0 * DOT + 6.0 + e.label.width / 2.0, cy);
                it.label = placed(e.label, at);
                it.id = Some(e.name);
                items.push(it);
            }
            bottom = y0 + legend_h;
        }
    }

    let mut sc = Scene {
        diagram: "journey".into(),
        width: (width + 2.0 * pad).ceil(),
        height: (max(bottom, pad) + pad).ceil(),
        font_size: size,
        title: db.acc_title.clone(),
        desc: db.acc_descr.clone(),
        items: Vec::new(),
    };
    if !title.is_empty() {
        let mut it = Item::new(Kind::Text, ROLE_TITLE);
        let at = Point::new(pad + width / 2.0, pad + title.height / 2.0);
        it.label = Some(Placed::new(title, at));
        sc.items.push(it);
    }
    sc.items.extend(items);
    sc
}

/// A score from 1 to 5 as Pele's num writes it: to two decimals, with no zeros at the end.
fn score_text(v: f64) -> String {
    let n = round(v * 100.0);
    let q = (n / 100.0).floor();
    let i = (n - q * 100.0) as u32;
    match i {
        0 => format!("{q}"),
        _ if i.is_multiple_of(10) => format!("{q}.{}", i / 10),
        _ => format!("{q}.{i:02}"),
    }
}

/// Lays the journey out down, as Pele's verticalJourney does: a card for each task, under
/// the head of its section, with its face and score at its right and its people under its
/// name.
fn vertical_journey(db: &JourneyDb, o: &Options) -> Scene {
    let size = o.font_size;
    let small = round(size * 0.875);
    let set = setting(o);
    let pad = o.pad();
    // A host that gives no width is taken to give 320, as Pele's `?? 320` takes it.
    let max_width = if o.max_width.is_finite() {
        o.max_width
    } else {
        320.0
    };
    let room = max(140.0, max_width - 2.0 * pad);
    let title = fit_label(
        db.title.as_deref().unwrap_or(""),
        &set,
        size,
        room,
        Face::BOLD,
    );
    let (_, actors) = actor_names(&db.tasks);
    let mut y = pad
        + if title.height != 0.0 {
            title.height + 16.0
        } else {
            0.0
        };
    let mut section: Option<&str> = None;
    let mut items = Vec::new();
    let mut width = title.width;
    for (index, task) in db.tasks.iter().enumerate() {
        if section != Some(task.section.as_str()) {
            section = Some(&task.section);
            let head = fit_label(&task.section, &set, size, room - 16.0, Face::BOLD);
            if head.height != 0.0 {
                let mut it = Item::new(Kind::Text, ROLE_SECTION_LABEL);
                let at = Point::new(pad + 8.0 + head.width / 2.0, y + head.height / 2.0);
                let h = head.height;
                width = max(width, head.width + 16.0);
                it.label = Some(Placed::new(head, at));
                items.push(it);
                y += h + 8.0;
            }
        }
        // Pele measures a task's label at the small size and writes its text with no
        // font-size, so that a browser draws it at the diagram's own size, wider than it was
        // measured. Here it is drawn at the size it was measured at, as the scene's labels
        // are. Where it is placed is Pele's.
        let label = fit_label(&task.task, &set, small, room - 72.0, Face::default());
        let people: Vec<(&String, Label)> = named(&task.people)
            .into_iter()
            .map(|name| {
                let l = fit_label(name, &set, small, room - 44.0, Face::default());
                (name, l)
            })
            .collect();
        let text_h = max(label.height, 42.0);
        let mut h = 2.0 * PAD_Y + text_h;
        for (_, l) in &people {
            h += max(16.0, l.height) + 4.0;
        }
        let w = max(room, label.width + 72.0);
        let mut card = Item::new(Kind::Node, ROLE_TASK);
        card.shape = Some(Shape::new("rect"));
        card.id = Some(trim(&task.task).to_string());
        card.bounds = Rect::new(pad, y, w, h);
        card.parts = vec![Part::rect(card.bounds, true, Painting::FillAndStroke)];
        card.style = Style {
            fill: Some(Paint::SURFACE),
            stroke: Some(Paint::BORDER),
            ..Style::default()
        };
        let at = Point::new(
            pad + PAD_X + label.width / 2.0,
            y + PAD_Y + label.height / 2.0,
        );
        card.label = placed(label, at);
        items.push(card);
        let cx = pad + w - 28.0;
        items.push(face(cx, y + PAD_Y + FACE, mood(task.score)));
        if !task.score.is_nan() {
            let text = score_text(min(5.0, max(1.0, task.score)));
            // Pele writes the score with its baseline under the face.
            let baseline = y + PAD_Y + 2.0 * FACE + small;
            let at = Point::new(cx, baseline - small * 0.35);
            items.push(text_item(
                ROLE_SCORE,
                &text,
                &set,
                small,
                at,
                Align::Middle,
                None,
            ));
        }
        let mut py = y + PAD_Y + text_h + 4.0;
        for (name, l) in people {
            let ph = max(16.0, l.height);
            let cy = py + ph / 2.0;
            let mut it = Item::new(Kind::Node, ROLE_ACTOR);
            it.shape = Some(Shape::new("circle"));
            it.id = Some(name.clone());
            it.bounds = Rect::new(pad + 16.0 - DOT, py, 12.0 + DOT + l.width, ph);
            it.parts = vec![Part::path(circle(pad + 16.0, cy, DOT), Painting::FillOnly)];
            it.style.fill = Some(Paint::series(actors.get(name).copied().unwrap_or(0)));
            let at = Point::new(pad + 28.0 + l.width / 2.0, cy);
            it.label = placed(l, at);
            items.push(it);
            py += ph + 4.0;
        }
        y += h;
        if index + 1 < db.tasks.len() {
            let x = pad + w / 2.0;
            let mut link = Path::new();
            link.move_to(x, y).line_to(x, y + 16.0);
            let route = vec![Point::new(x, y), Point::new(x, y + 16.0)];
            let mut it = Item::new(
                Kind::Edge(Edge {
                    route,
                    ends: [None, None],
                }),
                ROLE_LINE,
            );
            it.bounds = link.bounds();
            it.parts = vec![Part::path(link, Painting::StrokeOnly)];
            it.style.stroke = Some(Paint::LINE);
            items.push(it);
        }
        y += 16.0;
        width = max(width, w);
    }
    let total_width = (width + 2.0 * pad).ceil();
    let last = if db.tasks.is_empty() { y } else { y - 16.0 };
    let mut sc = Scene {
        diagram: "journey".into(),
        width: total_width,
        height: (max(pad + title.height, last) + pad).ceil(),
        font_size: size,
        title: db.acc_title.clone(),
        desc: db.acc_descr.clone(),
        items: Vec::new(),
    };
    if !title.is_empty() {
        let mut it = Item::new(Kind::Text, ROLE_TITLE);
        let at = Point::new(total_width / 2.0, pad + title.height / 2.0);
        it.label = Some(Placed::new(title, at));
        sc.items.push(it);
    }
    sc.items.extend(items);
    sc
}
