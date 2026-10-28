//! The participants of a sequence diagram, as Pele's actors.ts draws them: boxes, stacks and
//! queues that hold their names, and figures that stand above them.

use crate::js::math::max;
use crate::scene::text::Label;
use crate::scene::{Painting, Part, Path, Point, Rect};

// The measures of a participant, as Pele's actors.ts has them.
pub(crate) const PAD_X: f64 = 16.0;
const PAD_Y: f64 = 10.0;
const GLYPH: f64 = 34.0;
const GLYPH_GAP: f64 = 4.0;
const STACK: f64 = 5.0;
const CAP: f64 = 6.0;

/// The kind a participant is drawn as, Pele's actorKind: one of the kinds it knows, or
/// participant.
pub(crate) fn actor_kind(kind: &str) -> &str {
    match kind {
        "participant" | "actor" | "boundary" | "control" | "entity" | "database"
        | "collections" | "queue" => kind,
        _ => "participant",
    }
}

/// Whether a kind is a figure, which stands above its label, as Pele's isFigure says. The
/// other kinds hold the label inside.
pub(crate) fn is_figure(kind: &str) -> bool {
    matches!(
        kind,
        "actor" | "boundary" | "control" | "entity" | "database"
    )
}

/// The width and height of a participant, Pele's actorSize.
pub(crate) fn actor_size(
    kind: &str,
    label: &Label,
    min_w: f64,
    min_h: f64,
    padding_x: f64,
) -> (f64, f64) {
    if is_figure(kind) {
        let h = if label.height > 0.0 {
            GLYPH + (GLYPH_GAP + label.height)
        } else {
            GLYPH
        };
        return (max(40.0, label.width + 8.0), h);
    }
    let extra = match kind {
        "collections" => STACK,
        "queue" => 2.0 * CAP,
        _ => 0.0,
    };
    let w = max(min_w, label.width + 2.0 * padding_x) + extra;
    let h =
        max(min_h, label.height + 2.0 * PAD_Y) + if kind == "collections" { STACK } else { 0.0 };
    (w, h)
}

/// A circle, as Pele's `<circle>` draws one. Pele writes the element inline in its figure.
pub(crate) fn circle(cx: f64, cy: f64, r: f64, paint: Painting) -> Part {
    let mut p = Path::new();
    p.ellipse(cx, cy, r, r);
    Part::path(p, paint)
}

/// A path's part, stroked only.
fn line(p: &mut Path) -> Part {
    Part::path(std::mem::take(p), Painting::StrokeOnly)
}

/// The parts of a participant's figure whose top middle is at cx, top, as Pele's figure
/// draws them: shapes filled and stroked, and lines stroked.
fn figure(kind: &str, cx: f64, top: f64) -> Vec<Part> {
    let y = |dy: f64| top + dy;
    let mut p = Path::new();
    match kind {
        "actor" => vec![
            circle(cx, y(7.0), 6.0, Painting::FillAndStroke),
            line(
                p.move_to(cx, y(13.0))
                    .line_to(cx, y(24.0))
                    .move_to(cx - 10.0, y(17.0))
                    .line_to(cx + 10.0, y(17.0))
                    .move_to(cx - 8.0, y(34.0))
                    .line_to(cx, y(24.0))
                    .line_to(cx + 8.0, y(34.0)),
            ),
        ],
        "boundary" => vec![
            line(
                p.move_to(cx - 19.0, y(6.0))
                    .line_to(cx - 19.0, y(28.0))
                    .move_to(cx - 19.0, y(17.0))
                    .line_to(cx - 8.0, y(17.0)),
            ),
            circle(cx + 3.0, y(17.0), 11.0, Painting::FillAndStroke),
        ],
        "control" => vec![
            circle(cx, y(21.0), 11.0, Painting::FillAndStroke),
            line(
                p.move_to(cx + 5.0, y(5.0))
                    .line_to(cx - 1.0, y(10.0))
                    .line_to(cx + 5.0, y(15.0)),
            ),
        ],
        "entity" => vec![
            circle(cx, y(15.0), 11.0, Painting::FillAndStroke),
            line(p.move_to(cx - 11.0, y(30.0)).line_to(cx + 11.0, y(30.0))),
        ],
        // A database: a cylinder, and the front of its lid.
        _ => {
            let (l, r) = (cx - 14.0, cx + 14.0);
            p.move_to(l, y(7.0))
                .line_to(l, y(27.0))
                .arc_to(14.0, 5.0, 0.0, false, false, r, y(27.0))
                .line_to(r, y(7.0))
                .arc_to(14.0, 5.0, 0.0, false, false, l, y(7.0))
                .close();
            let body = Part::path(std::mem::take(&mut p), Painting::FillAndStroke);
            p.move_to(l, y(7.0))
                .arc_to(14.0, 5.0, 0.0, false, false, r, y(7.0));
            vec![body, line(&mut p)]
        }
    }
}

/// A participant whose box has its top middle at cx, top, as Pele's drawActor draws it: its
/// parts, and the point its label is centered on.
pub(crate) fn draw_actor(
    kind: &str,
    cx: f64,
    top: f64,
    w: f64,
    h: f64,
    label: &Label,
) -> (Vec<Part>, Point) {
    if is_figure(kind) {
        let at = Point::new(cx, top + GLYPH + GLYPH_GAP + label.height / 2.0);
        return (figure(kind, cx, top), at);
    }
    let left = cx - w / 2.0;
    match kind {
        "collections" => {
            let (bw, bh) = (w - STACK, h - STACK);
            (
                vec![
                    Part::rect(
                        Rect::new(left + STACK, top, bw, bh),
                        true,
                        Painting::FillAndStroke,
                    ),
                    Part::rect(
                        Rect::new(left, top + STACK, bw, bh),
                        true,
                        Painting::FillAndStroke,
                    ),
                ],
                Point::new(left + bw / 2.0, top + STACK + bh / 2.0),
            )
        }
        "queue" => {
            let (l, r, b, ry) = (left + CAP, left + w - CAP, top + h, h / 2.0);
            let mut body = Path::new();
            body.move_to(l, top)
                .line_to(r, top)
                .arc_to(CAP, ry, 0.0, false, true, r, b)
                .line_to(l, b)
                .arc_to(CAP, ry, 0.0, false, true, l, top)
                .close();
            let mut end = Path::new();
            end.move_to(r, top).arc_to(CAP, ry, 0.0, false, false, r, b);
            (
                vec![
                    Part::path(body, Painting::FillAndStroke),
                    Part::path(end, Painting::StrokeOnly),
                ],
                Point::new(cx - CAP / 2.0, top + h / 2.0),
            )
        }
        _ => (
            vec![Part::rect(
                Rect::new(left, top, w, h),
                true,
                Painting::FillAndStroke,
            )],
            Point::new(cx, top + h / 2.0),
        ),
    }
}
