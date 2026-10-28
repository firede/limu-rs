//! What a user journey's parser fills, as Pele's JourneyDb in db.ts has it: its sections, and
//! its tasks with their scores and people.

use std::collections::HashSet;

use crate::js::chars::{drop_indents, trim, trim_start};
use crate::js::number::number;

/// A step of a journey, Pele's JourneyTask.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct JourneyTask {
    pub(crate) task: String,
    /// The score as written: any number, or NaN where it is not one.
    pub(crate) score: f64,
    pub(crate) people: Vec<String>,
    /// The name of the section in force when the task was declared, "" before the first one.
    pub(crate) section: String,
}

/// Everyone named on a task, once each, in the order of their code units, as Pele's
/// journeyActors gives them.
pub(crate) fn journey_actors(tasks: &[JourneyTask]) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut names: Vec<String> = Vec::new();
    for task in tasks {
        for name in &task.people {
            if seen.insert(name.as_str()) {
                names.push(name.clone());
            }
        }
    }
    // JavaScript's sort compares strings by their UTF-16 code units, which orders characters
    // above the Basic Multilingual Plane before those from U+E000 up, where UTF-8 orders
    // them after.
    names.sort_by(|a, b| a.encode_utf16().cmp(b.encode_utf16()));
    names
}

/// A user journey as its parser fills it, Pele's JourneyDb.
#[derive(Clone, Debug, Default)]
pub(crate) struct JourneyDb {
    pub(crate) title: Option<String>,
    pub(crate) acc_title: Option<String>,
    pub(crate) acc_descr: Option<String>,
    pub(crate) sections: Vec<String>,
    pub(crate) tasks: Vec<JourneyTask>,

    current_section: String,
}

impl JourneyDb {
    /// An empty db, of the title its front matter gives.
    pub(crate) fn new(title: Option<&str>) -> JourneyDb {
        JourneyDb {
            title: title.map(str::to_string),
            ..JourneyDb::default()
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

    pub(crate) fn add_section(&mut self, text: &str) {
        self.current_section = text.to_string();
        self.sections.push(text.to_string());
    }

    /// Adds a task, as Pele's addTask does. data is the rest of the line from its first
    /// colon, `:score` or `:score:actor, actor`.
    pub(crate) fn add_task(&mut self, task: &str, data: &str) {
        let pieces: Vec<&str> = data[1..].split(':').collect();
        let people = match pieces.get(1) {
            Some(names) => names.split(',').map(|s| trim(s).to_string()).collect(),
            None => Vec::new(),
        };
        self.tasks.push(JourneyTask {
            task: task.to_string(),
            score: number(pieces[0]),
            people,
            section: self.current_section.clone(),
        });
    }
}
