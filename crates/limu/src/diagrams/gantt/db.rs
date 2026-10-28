//! What a gantt chart's parser fills, as Pele's GanttDb in db.ts has it, and the schedule of
//! its tasks, which Pele's getTasks works out in local time as Mermaid's ganttDb does with
//! dayjs.

use std::collections::{HashMap, HashSet};

use super::dates::{Unit, WEEKDAYS, add, add_days, format, iso_date, parse_strict};
use crate::detect::DiagramType;
use crate::errors::{Error, ErrorCode};
use crate::js::chars::{
    drop_indents, is_space, is_space_char, len16, to_lower, trim, trim_start, unit_at,
};
use crate::js::date::{Zone, time_clip, valid};
use crate::js::number::number;
use crate::layout::TimeZone;
use crate::url::safe_url;

/// Scheduling walks tasks day by day and parses dates with a format the chart supplies. This
/// bounds the total, in rough character operations, so a hostile chart fails instead of
/// hanging, as Pele's WORK_LIMIT does.
const WORK_LIMIT: f64 = 4e8;
const DAY_STEP: f64 = 400.0;
const DATED_STEP: f64 = 200.0;
const FORMAT_STEP: f64 = 400.0;

/// A time of a task as Pele holds one: undefined, or a date, which may be invalid.
pub(crate) type Moment = Option<f64>;

/// A moment as a number, as `+date` gives it: NaN where it is undefined. Pele's getStartDate
/// and getEndDate compare tasks' times so.
pub(super) fn value(m: Moment) -> f64 {
    m.unwrap_or(f64::NAN)
}

/// What a task's data said, before it is scheduled, Pele's raw.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Raw {
    pub(crate) data: String,
    /// The start as written. With none, a task starts where the task before it ends.
    pub(crate) start: Option<String>,
    pub(crate) end: String,
}

/// A task of a chart, Pele's GanttTask.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Task {
    pub(crate) id: String,
    /// The task's label, under the name Mermaid gives it.
    pub(crate) task: String,
    pub(crate) section: String,
    /// When it starts and ends once scheduled, none where what it refers to never resolves.
    pub(crate) start_time: Moment,
    pub(crate) end_time: Moment,
    /// Where the bar ends when excluded days pushed the task's end out, none (Pele's null)
    /// where they did not.
    pub(crate) render_end_time: Moment,
    pub(crate) manual_end_time: bool,
    pub(crate) processed: bool,
    pub(crate) active: bool,
    pub(crate) done: bool,
    pub(crate) crit: bool,
    pub(crate) milestone: bool,
    pub(crate) vert: bool,
    /// The task's row in source order, -1 for a vertical marker, which takes no row.
    pub(crate) order: i64,
    pub(crate) classes: Vec<String>,
    pub(crate) prev_task_id: Option<String>,
    pub(crate) raw: Raw,
}

/// A gantt chart as its parser fills it, Pele's GanttDb.
#[derive(Clone, Debug)]
pub(crate) struct GanttDb {
    pub(crate) title: Option<String>,
    pub(crate) acc_title: Option<String>,
    pub(crate) acc_descr: Option<String>,
    pub(crate) date_format: String,
    pub(crate) axis_format: String,
    pub(crate) tick_interval: Option<String>,
    pub(crate) today_marker: String,
    pub(crate) includes: Vec<String>,
    pub(crate) excludes: Vec<String>,
    pub(crate) inclusive_end_dates: bool,
    pub(crate) top_axis: bool,
    pub(crate) display_mode: String,
    /// The day week-based ticks start on, where the chart names one.
    pub(crate) weekday: Option<String>,
    pub(crate) weekend: String,
    pub(crate) sections: Vec<String>,
    /// The tasks' links, by id, in the order Pele's Map of them keeps.
    pub(crate) links: Vec<(String, String)>,
    pub(crate) tasks: Vec<Task>,

    /// The place in tasks of the last task of each id.
    by_id: HashMap<String, usize>,
    current_section: String,
    task_count: usize,
    last_order: i64,
    last_task_id: Option<String>,
}

impl Default for GanttDb {
    fn default() -> Self {
        GanttDb {
            title: None,
            acc_title: None,
            acc_descr: None,
            date_format: String::new(),
            axis_format: String::new(),
            tick_interval: None,
            today_marker: String::new(),
            includes: Vec::new(),
            excludes: Vec::new(),
            inclusive_end_dates: false,
            top_axis: false,
            display_mode: String::new(),
            weekday: None,
            weekend: "saturday".into(),
            sections: Vec::new(),
            links: Vec::new(),
            tasks: Vec::new(),
            by_id: HashMap::new(),
            current_section: String::new(),
            task_count: 0,
            last_order: 0,
            last_task_id: None,
        }
    }
}

/// An error of a gantt chart, as Pele's PeleError gives one of the chart's type.
fn error(message: impl Into<String>, code: ErrorCode) -> Error {
    let mut e = Error::new(message, code);
    e.diagram = Some(DiagramType::Gantt);
    e
}

/// Pele's mergeTokens: the tokens of txt, lowered and split at white space and commas, added
/// to those there are, each once.
fn merge_tokens(existing: &[String], txt: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut seen = HashSet::new();
    let lower = to_lower(txt);
    let tokens = lower.split(|c: char| c == ',' || is_space_char(c));
    for t in existing.iter().map(String::as_str).chain(tokens) {
        if !t.is_empty() && seen.insert(t.to_string()) {
            out.push(t.to_string());
        }
    }
    out
}

/// The tag a field is, as `/^\s*(active|done|crit|milestone|vert)\s*$/`, Pele's RE_TAG, reads
/// it.
fn tag(field: &str) -> Option<&str> {
    let t = trim(field);
    matches!(t, "active" | "done" | "crit" | "milestone" | "vert").then_some(t)
}

impl GanttDb {
    pub(crate) fn set_date_format(&mut self, txt: &str) {
        self.date_format = txt.to_string();
    }

    pub(crate) fn enable_inclusive_end_dates(&mut self) {
        self.inclusive_end_dates = true;
    }

    /// Mermaid's grammar calls a method its database does not define, so `topAxis` crashes
    /// there.
    pub(crate) fn set_top_axis(&mut self) {
        self.top_axis = true;
    }

    pub(crate) fn set_axis_format(&mut self, txt: &str) {
        self.axis_format = txt.to_string();
    }

    pub(crate) fn set_tick_interval(&mut self, txt: &str) {
        self.tick_interval = Some(txt.to_string());
    }

    pub(crate) fn set_today_marker(&mut self, txt: &str) {
        self.today_marker = txt.to_string();
    }

    pub(crate) fn set_includes(&mut self, txt: &str) {
        self.includes = merge_tokens(&self.includes, txt);
    }

    pub(crate) fn set_excludes(&mut self, txt: &str) {
        self.excludes = merge_tokens(&self.excludes, txt);
    }

    pub(crate) fn set_weekday(&mut self, txt: &str) {
        self.weekday = Some(txt.to_string());
    }

    pub(crate) fn set_weekend(&mut self, start_day: &str) {
        self.weekend = start_day.to_string();
    }

    pub(crate) fn set_diagram_title(&mut self, txt: &str) {
        self.title = Some(txt.to_string());
    }

    pub(crate) fn set_acc_title(&mut self, txt: &str) {
        self.acc_title = Some(trim_start(txt).to_string());
    }

    pub(crate) fn set_acc_description(&mut self, txt: &str) {
        self.acc_descr = Some(drop_indents(txt));
    }

    pub(crate) fn add_section(&mut self, txt: &str) {
        self.current_section = txt.to_string();
        self.sections.push(txt.to_string());
    }

    /// Pele's addTask: it adds a task, its tags read from the start of its data and the rest
    /// of the data kept raw, to be scheduled later. A task with no fields or too many is an
    /// error.
    pub(crate) fn add_task(&mut self, descr: &str, data_str: &str) -> Result<(), Error> {
        let fields: Vec<&str> = data_str
            .strip_prefix(':')
            .unwrap_or(data_str)
            .split(',')
            .collect();
        let mut task = Task {
            task: descr.to_string(),
            section: self.current_section.clone(),
            order: -1,
            prev_task_id: self.last_task_id.clone(),
            raw: Raw {
                data: data_str.to_string(),
                ..Raw::default()
            },
            ..Task::default()
        };
        let mut data = &fields[..];
        while let Some(t) = data.first().and_then(|f| tag(f)) {
            match t {
                "active" => task.active = true,
                "done" => task.done = true,
                "crit" => task.crit = true,
                "milestone" => task.milestone = true,
                _ => task.vert = true,
            }
            data = &data[1..];
        }
        // Mermaid fails with a TypeError on both of these.
        if data.is_empty() {
            let message = format!("Task \"{}\" needs a duration or an end date.", trim(descr));
            return Err(error(message, ErrorCode::Semantic));
        }
        if data.len() > 3 {
            let message = format!(
                "Task \"{}\" has too many fields. A task takes an id, a start, and an end or \
                 duration.",
                trim(descr)
            );
            return Err(error(message, ErrorCode::Semantic));
        }
        let data: Vec<&str> = data.iter().map(|d| trim(d)).collect();
        if data.len() == 3 {
            task.id = data[0].to_string();
        } else {
            self.task_count += 1;
            task.id = format!("task{}", self.task_count);
        }
        if data.len() > 1 {
            task.raw.start = Some(data[data.len() - 2].to_string());
        }
        task.raw.end = data[data.len() - 1].to_string();
        if !task.vert {
            task.order = self.last_order;
            self.last_order += 1;
        }
        self.last_task_id = Some(task.id.clone());
        self.by_id.insert(task.id.clone(), self.tasks.len());
        self.tasks.push(task);
        Ok(())
    }

    /// Pele's setLink: it gives each existing task of ids, a comma-separated list, the link
    /// made safe, and marks them clickable.
    pub(crate) fn set_link(&mut self, ids: &str, link: &str) {
        let href = safe_url(link);
        for id in ids.split(',') {
            if self.by_id.contains_key(id) {
                match self.links.iter_mut().find(|(k, _)| k == id) {
                    Some(entry) => entry.1 = href.clone(),
                    None => self.links.push((id.to_string(), href.clone())),
                }
            }
        }
        self.set_class(ids, "clickable");
    }

    /// Callbacks are recognized and never run; the task is only marked clickable, as in
    /// Mermaid. It is Pele's setClickEvent.
    pub(crate) fn set_click_event(&mut self, ids: &str) {
        self.set_class(ids, "clickable");
    }

    fn set_class(&mut self, ids: &str, class_name: &str) {
        for id in ids.split(',') {
            if let Some(&i) = self.by_id.get(id) {
                self.tasks[i].classes.push(class_name.to_string());
            }
        }
    }

    /// The link of a task, as Pele's links map holds it.
    pub(crate) fn link(&self, id: &str) -> Option<&str> {
        self.links
            .iter()
            .find(|(k, _)| k == id)
            .map(|(_, v)| v.as_str())
    }

    /// Resolves every task's start and end against the time now, in milliseconds since the
    /// epoch, in a zone, from scratch, as Pele's getTasks does, and gives the tasks so
    /// scheduled, leaving the chart's as they were. A task may refer to tasks defined after
    /// it, so, like Mermaid, it makes up to eleven passes, each of which schedules every task
    /// again.
    pub(crate) fn schedule<'a>(
        &'a self,
        now: f64,
        tz: &'a dyn TimeZone,
    ) -> Result<Schedule<'a>, Error> {
        let include_set: HashSet<String> = self.includes.iter().cloned().collect();
        let exclude_set: HashSet<String> = self.excludes.iter().cloned().collect();
        let weekend_start = match self.weekend.as_str() {
            "friday" => 5.0,
            "saturday" => 6.0,
            _ => f64::NAN,
        };
        let weekends = exclude_set.contains("weekends");
        let mut excluded_days = [false; 7];
        for (day, name) in WEEKDAYS.iter().enumerate() {
            let iso = if day == 0 { 7.0 } else { day as f64 };
            excluded_days[day] = weekends && (iso == weekend_start || iso == weekend_start + 1.0)
                || exclude_set.contains(&to_lower(name));
        }
        // A date written with only these tokens and separators can never read as a weekday
        // name, so when nothing else is excluded or included there is no need to format each
        // day.
        let mut dated_tokens =
            !self.includes.is_empty() || !numeric_format(trim(&self.date_format));
        for t in &self.excludes {
            if t != "weekends" && !WEEKDAYS.iter().any(|w| to_lower(w) == *t) {
                dated_tokens = true;
            }
        }
        let mut tasks = self.tasks.clone();
        for t in &mut tasks {
            t.start_time = None;
            t.end_time = None;
            t.render_end_time = None;
            t.processed = false;
            t.manual_end_time = false;
        }
        let mut s = Schedule {
            db: self,
            zone: Zone(tz),
            now,
            work: 0.0,
            include_set,
            exclude_set,
            excluded_days,
            dated_tokens,
            tasks,
        };
        let mut all = s.compile_tasks()?;
        let mut i = 0;
        while !all && i < 10 {
            all = s.compile_tasks()?;
            i += 1;
        }
        Ok(s)
    }
}

/// Pele's RE_NUMERIC_FORMAT, `/^[YMDHhmsSZXx\-/.:, T]*$/`.
fn numeric_format(s: &str) -> bool {
    s.bytes().all(|c| b"YMDHhmsSZXx-/.:, T".contains(&c))
}

/// The tasks of a chart as scheduled in a zone against a time, with what Pele's GanttDb keeps
/// while it schedules them, as isInvalidDate reads it.
pub(crate) struct Schedule<'a> {
    db: &'a GanttDb,
    pub(crate) zone: Zone<'a>,
    now: f64,
    work: f64,
    include_set: HashSet<String>,
    exclude_set: HashSet<String>,
    excluded_days: [bool; 7],
    dated_tokens: bool,
    pub(crate) tasks: Vec<Task>,
}

impl Schedule<'_> {
    /// Pele's isInvalidDate: whether a day is excluded. A day matches a token by its weekday
    /// name, by its date as YYYY-MM-DD, or by its date written in the chart's own format.
    pub(crate) fn is_invalid_date(&mut self, date: f64) -> Result<bool, Error> {
        let z = self.zone;
        let d = z.day(date);
        let weekday = !d.is_nan() && self.excluded_days[d as usize];
        if !self.dated_tokens {
            return Ok(weekday);
        }
        let template = trim(&self.db.date_format);
        let date_only = iso_date(z, date);
        let mut formatted = Some(date_only.clone());
        self.spend(DATED_STEP)?;
        if template != "YYYY-MM-DD" {
            self.spend(FORMAT_STEP + len16(template) as f64 * 8.0)?;
            formatted = format(z, date, template);
        }
        let listed = |set: &HashSet<String>| {
            set.contains(&date_only) || formatted.as_ref().is_some_and(|f| set.contains(f))
        };
        if listed(&self.include_set) {
            return Ok(false);
        }
        Ok(weekday || listed(&self.exclude_set))
    }

    /// Pele's spend: it counts units of work, which are an error past the limit.
    fn spend(&mut self, units: f64) -> Result<(), Error> {
        self.work += units;
        if self.work > WORK_LIMIT {
            let message = "The gantt chart takes too much work to schedule.";
            return Err(error(message, ErrorCode::Limit));
        }
        Ok(())
    }

    /// Pele's parse: s read strictly as in template, with what it costs spent.
    fn parse(&mut self, s: &str, template: &str) -> Result<Option<f64>, Error> {
        self.spend(((len16(template) + 1) * (len16(s) + 1)) as f64)?;
        Ok(parse_strict(self.zone, s, template, self.now))
    }

    /// Pele's today: the start of the day it is now.
    fn today(&self) -> f64 {
        self.zone.set_hours(self.now, 0.0, 0.0, 0.0, 0.0)
    }

    /// Pele's compileTasks: one pass that schedules every task, saying whether each was
    /// scheduled.
    fn compile_tasks(&mut self) -> Result<bool, Error> {
        let mut all = true;
        for i in 0..self.tasks.len() {
            self.compile_task(i)?;
            all = all && self.tasks[i].processed;
        }
        Ok(all)
    }

    /// Pele's compileTask: it gives a task its start and end where it can, a task with no
    /// start starting where the one before it ends.
    fn compile_task(&mut self, i: usize) -> Result<(), Error> {
        match self.tasks[i].raw.start.clone() {
            None => {
                // Mermaid fails with a TypeError when the first task has no start. It starts
                // today here.
                let previous = self.tasks[i]
                    .prev_task_id
                    .as_ref()
                    .and_then(|id| self.db.by_id.get(id));
                self.tasks[i].start_time = match previous {
                    Some(&p) => self.tasks[p].end_time,
                    None => Some(self.today()),
                };
            }
            Some(start) => {
                if let Some(t) = self.get_start_date(&start)? {
                    self.tasks[i].start_time = Some(t);
                }
            }
        }
        let Some(start) = self.tasks[i].start_time else {
            return Ok(());
        };
        let end = self.tasks[i].raw.end.clone();
        self.tasks[i].end_time = self.get_end_date(start, &end)?;
        if self.tasks[i].end_time.is_none() {
            return Ok(());
        }
        self.tasks[i].processed = true;
        self.tasks[i].manual_end_time = self.parse(&end, "YYYY-MM-DD")?.is_some();
        self.check_task_dates(i)
    }

    /// Pele's references: the tasks a list of ids names. Pele warns of the ids no task has,
    /// to an option its render never gives, and so does not here.
    fn references(&self, list: &str) -> Vec<usize> {
        list.split(' ')
            .filter(|id| !id.is_empty())
            .filter_map(|id| self.db.by_id.get(id).copied())
            .collect()
    }

    /// Pele's getStartDate, with its checked inlined: the start a task's raw start gives, from
    /// a time value, the latest end of the tasks it comes after, the chart's format or,
    /// failing that, the JavaScript date parser. None is a start not known yet.
    fn get_start_date(&mut self, raw: &str) -> Result<Moment, Error> {
        let s = trim(raw);
        let template = trim(&self.db.date_format);

        if (template == "x" || template == "X")
            && !s.is_empty()
            && s.bytes().all(|c| c.is_ascii_digit())
        {
            let d = time_clip(number(s));
            if !valid(d) {
                return Err(invalid_date(s));
            }
            return Ok(Some(d));
        }

        if let Some(list) = match_ref(s, "after") {
            let mut latest: Option<usize> = None;
            for t in self.references(list) {
                if latest
                    .is_none_or(|l| value(self.tasks[t].end_time) > value(self.tasks[l].end_time))
                {
                    latest = Some(t);
                }
            }
            return Ok(match latest {
                Some(l) => self.tasks[l].end_time,
                None => Some(self.today()),
            });
        }

        if let Some(parsed) = self.parse(s, template)? {
            return Ok(Some(parsed));
        }

        // What does not fit the format gets a second chance with the JavaScript date parser,
        // as in Mermaid.
        let date = self.zone.parse(s);
        let year = self.zone.full_year(date);
        if !valid(date) || year < -10000.0 || year > 10000.0 {
            return Err(invalid_date(s));
        }
        Ok(Some(date))
    }

    /// Pele's getEndDate: the end a task's raw end gives, from the earliest start of the
    /// tasks it runs until, a date in the chart's format, or a duration after start.
    fn get_end_date(&mut self, start: f64, raw: &str) -> Result<Moment, Error> {
        let s = trim(raw);

        if let Some(list) = match_ref(s, "until") {
            let mut earliest: Option<usize> = None;
            for t in self.references(list) {
                if earliest.is_none_or(|e| {
                    value(self.tasks[t].start_time) < value(self.tasks[e].start_time)
                }) {
                    earliest = Some(t);
                }
            }
            return Ok(match earliest {
                Some(e) => self.tasks[e].start_time,
                None => Some(self.today()),
            });
        }

        let template = trim(&self.db.date_format);
        if let Some(parsed) = self.parse(s, template)? {
            return Ok(Some(if self.db.inclusive_end_dates {
                add_days(self.zone, parsed, 1.0)
            } else {
                parsed
            }));
        }

        let Some((amount, unit)) = parse_duration(s) else {
            // Pele warns that the task gets no duration, to an option its render never gives.
            return Ok(Some(time_clip(start)));
        };
        let end = add(self.zone, start, amount, unit);
        Ok(Some(if valid(end) { end } else { time_clip(start) }))
    }

    /// Pushes the end of a task out by one day for every excluded day it covers, as Pele's
    /// checkTaskDates does.
    fn check_task_dates(&mut self, i: usize) -> Result<(), Error> {
        let task = &self.tasks[i];
        if self.db.excludes.is_empty() || task.manual_end_time {
            return Ok(());
        }
        let z = self.zone;
        let mut cursor = add_days(z, value(task.start_time), 1.0);
        let mut end_time = time_clip(value(task.end_time));
        let max_end_time = add_days(z, end_time, 10000.0);
        let mut invalid = false;
        let mut render_end_time = None;
        while cursor <= end_time {
            self.spend(DAY_STEP)?;
            if !invalid {
                render_end_time = Some(end_time);
            }
            invalid = self.is_invalid_date(cursor)?;
            if invalid {
                end_time = z.set_date(end_time, z.date(end_time) + 1.0);
                if end_time > max_end_time {
                    let message = "Failed to find a valid date that was not excluded by \
                                   `excludes` after 10,000 iterations.";
                    return Err(error(message, ErrorCode::Semantic));
                }
            }
            cursor = z.set_date(cursor, z.date(cursor) + 1.0);
        }
        let task = &mut self.tasks[i];
        task.end_time = Some(end_time);
        task.render_end_time = render_end_time;
        Ok(())
    }
}

/// The error of a date that cannot be read, as Pele's getStartDate and checked throw it.
fn invalid_date(s: &str) -> Error {
    error(format!("Invalid date:{s}"), ErrorCode::Semantic)
}

/// Whether byte c is in `[\d\w- ]`, the class of Pele's RE_AFTER and RE_UNTIL.
fn is_ref_char(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_' || c == b'-' || c == b' '
}

/// `/^keyword\s+([\d\w- ]+)/`: the ids it refers to. It is Pele's RE_AFTER and RE_UNTIL,
/// written out with the backtracking they do.
fn match_ref<'a>(s: &'a str, keyword: &str) -> Option<&'a str> {
    if !s.starts_with(keyword) {
        return None;
    }
    // The white space \s+ takes, with each character's start, to give back one at a time.
    let mut starts = Vec::new();
    let mut q = keyword.len();
    loop {
        let (c, size) = unit_at(s, q);
        if !is_space(c) {
            break;
        }
        starts.push(q);
        q += size;
    }
    if starts.is_empty() {
        return None;
    }
    let capture = |from: usize| {
        let e = from
            + s.as_bytes()[from..]
                .iter()
                .take_while(|&&c| is_ref_char(c))
                .count();
        (e > from).then(|| &s[from..e])
    };
    capture(q).or_else(|| (1..starts.len()).rev().find_map(|k| capture(starts[k])))
}

/// Pele's parseDuration: a duration such as 3d, as `/^(\d+(?:\.\d+)?)([Mdhmswy]|ms)$/` reads
/// it, or none.
pub(super) fn parse_duration(s: &str) -> Option<(f64, Unit)> {
    let s = trim(s);
    let b = s.as_bytes();
    let mut i = b.iter().take_while(|c| c.is_ascii_digit()).count();
    if i == 0 {
        return None;
    }
    if i + 1 < b.len() && b[i] == b'.' && b[i + 1].is_ascii_digit() {
        i += 1 + b[i + 1..].iter().take_while(|c| c.is_ascii_digit()).count();
    }
    let unit = match &s[i..] {
        "M" => Unit::Month,
        "d" => Unit::D,
        "h" => Unit::H,
        "m" => Unit::M,
        "s" => Unit::S,
        "w" => Unit::W,
        "y" => Unit::Y,
        "ms" => Unit::Ms,
        _ => return None,
    };
    Some((number(&s[..i]), unit))
}
