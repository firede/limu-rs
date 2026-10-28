//! Gantt charts, as Pele's src/diagrams/gantt reads and draws them: a lexer and a parser that
//! accept what Mermaid's gantt.jison accepts, a db that schedules the tasks in local time as
//! Mermaid's ganttDb does with dayjs, and a layout that draws the chart on a time axis as d3
//! would lay it out.
//!
//! Dates are computed as JavaScript computes them in local time (see [`crate::js::date`]), in
//! the zone [`Options::time_zone`](crate::layout::Options::time_zone) gives, and against the
//! time [`Options::now`](crate::layout::Options::now) gives, so that a host decides both.
//!
//! A task's item (`gantt.task`) has, as its tags and in this order, "active" or "done",
//! "crit", "milestone", "clickable" where a click line gives it a link or a callback, and
//! "unscheduled" where its dates could not be worked out. A section's band (`gantt.section`)
//! has "section-N", N the section's place among the colors numberSectionStyles sets, counted
//! from 0.

mod axis;
mod dates;
mod db;
mod lexer;
mod parser;
mod render;

#[cfg(test)]
mod tests;

pub(crate) use db::GanttDb;
pub(crate) use render::layout;

use crate::errors::Error;
use crate::value::{Map, Value};

/// Parses a gantt chart's text, as Pele's gantt.parse does. title is the one its front matter
/// gives.
pub(crate) fn parse(source: &str, config: &Map, title: Option<&str>) -> Result<GanttDb, Error> {
    let mut db = GanttDb::default();
    if let Some(title) = title.filter(|t| !t.is_empty()) {
        db.title = Some(title.to_string());
    }
    let mode = config
        .get("gantt")
        .and_then(Value::as_object)
        .and_then(|g| g.get("displayMode"));
    if let Some(Value::String(mode)) = mode {
        db.display_mode = mode.clone();
    }
    parser::parse_gantt(source, &mut db)?;
    Ok(db)
}
