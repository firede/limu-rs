//! What a timeline's parser fills, as Pele's TimelineDb in db.ts has it: its sections, and
//! its time periods with their events.

use crate::detect::DiagramType;
use crate::errors::{Error, ErrorCode};
use crate::js::chars::{drop_indents, trim_start};

/// A time period and its events, Pele's TimelinePeriod.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct TimelinePeriod {
    pub(crate) text: String,
    pub(crate) events: Vec<String>,
    /// The name of the section in force when the period was declared, "" before the first
    /// one.
    pub(crate) section: String,
    /// The index into the sections of that section, -1 before the first one.
    pub(crate) section_index: isize,
}

/// A timeline as its parser fills it, Pele's TimelineDb. Its direction is "LR" or "TD".
#[derive(Clone, Debug)]
pub(crate) struct TimelineDb {
    pub(crate) title: Option<String>,
    pub(crate) acc_title: Option<String>,
    pub(crate) acc_descr: Option<String>,
    pub(crate) direction: &'static str,
    pub(crate) sections: Vec<String>,
    pub(crate) periods: Vec<TimelinePeriod>,

    current_section: String,
}

impl Default for TimelineDb {
    fn default() -> Self {
        TimelineDb {
            title: None,
            acc_title: None,
            acc_descr: None,
            direction: "LR",
            sections: Vec::new(),
            periods: Vec::new(),
            current_section: String::new(),
        }
    }
}

impl TimelineDb {
    /// An empty db, of the title its front matter gives.
    pub(crate) fn new(title: Option<&str>) -> TimelineDb {
        TimelineDb {
            title: title.map(str::to_string),
            ..TimelineDb::default()
        }
    }

    pub(crate) fn set_diagram_title(&mut self, text: &str) {
        self.title = Some(text.to_string());
    }

    pub(crate) fn set_acc_title(&mut self, text: &str) {
        self.acc_title = Some(trim_start(text).to_string());
    }

    pub(crate) fn set_acc_description(&mut self, text: &str) {
        self.acc_descr = Some(drop_indents(text));
    }

    pub(crate) fn set_direction(&mut self, direction: &'static str) {
        self.direction = direction;
    }

    pub(crate) fn add_section(&mut self, text: &str) {
        self.current_section = text.to_string();
        self.sections.push(text.to_string());
    }

    pub(crate) fn add_task(&mut self, period: &str, event: &str) {
        self.periods.push(TimelinePeriod {
            text: period.to_string(),
            events: if event.is_empty() {
                Vec::new()
            } else {
                vec![event.to_string()]
            },
            section: self.current_section.clone(),
            section_index: self.sections.len() as isize - 1,
        });
    }

    pub(crate) fn add_event(&mut self, event: &str) -> Result<(), Error> {
        let Some(period) = self.periods.last_mut() else {
            // Mermaid fails here too, with a TypeError.
            let mut e = Error::new(
                "An event needs a time period before it.",
                ErrorCode::Semantic,
            );
            e.diagram = Some(DiagramType::Timeline);
            return Err(e);
        };
        period.events.push(event.to_string());
        Ok(())
    }
}
