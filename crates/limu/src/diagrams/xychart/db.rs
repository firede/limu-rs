//! What an XY chart's parser fills, as Pele's XyChartDb in db.ts has it: its axes, and the
//! data of each of its series.

use crate::js::chars::{drop_indents, trim, trim_start};
use crate::js::number::to_string;

/// An axis of categories, Pele's BandAxis.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct BandAxis {
    pub(crate) title: String,
    pub(crate) categories: Vec<String>,
}

/// An axis of numbers, Pele's LinearAxis.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct LinearAxis {
    pub(crate) title: String,
    pub(crate) min: f64,
    pub(crate) max: f64,
}

/// The x axis, of categories or of numbers.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum XAxis {
    Band(BandAxis),
    Linear(LinearAxis),
}

impl XAxis {
    pub(crate) fn title(&self) -> &str {
        match self {
            XAxis::Band(a) => &a.title,
            XAxis::Linear(a) => &a.title,
        }
    }
}

/// A value of a series as the text writes it, with the label it may have, Pele's DataPoint.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DataPoint {
    pub(crate) value: f64,
    pub(crate) label: String,
}

/// Whether a series is drawn as a line or as bars.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PlotType {
    Line,
    Bar,
}

/// A series, Pele's Plot. Each value has its place on the x axis: a category, or a number
/// written as JavaScript writes it. A category with no value has none, Pele's undefined.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Plot {
    pub(crate) kind: PlotType,
    pub(crate) title: String,
    pub(crate) data: Vec<(String, Option<f64>)>,
    /// The labels of the points of a line, where any of them has one.
    pub(crate) point_labels: Option<Vec<String>>,
}

/// An XY chart as its parser fills it, Pele's XyChartDb.
#[derive(Clone, Debug)]
pub(crate) struct XyChartDb {
    pub(crate) title: Option<String>,
    pub(crate) acc_title: Option<String>,
    pub(crate) acc_descr: Option<String>,
    /// "vertical" or "horizontal" where the text says, for the chart's config to decide
    /// where it does not.
    pub(crate) orientation: Option<&'static str>,
    pub(crate) x_axis: XAxis,
    pub(crate) y_axis: LinearAxis,
    pub(crate) has_set_x_axis: bool,
    /// False while the y range comes from the data and not from a `y-axis` statement.
    pub(crate) has_set_y_axis: bool,
    pub(crate) plots: Vec<Plot>,
}

impl Default for XyChartDb {
    fn default() -> Self {
        XyChartDb {
            title: None,
            acc_title: None,
            acc_descr: None,
            orientation: None,
            x_axis: XAxis::Band(BandAxis::default()),
            y_axis: LinearAxis {
                title: String::new(),
                min: f64::INFINITY,
                max: f64::NEG_INFINITY,
            },
            has_set_x_axis: false,
            has_set_y_axis: false,
            plots: Vec::new(),
        }
    }
}

impl XyChartDb {
    pub(crate) fn set_orientation(&mut self, orientation: &str) {
        self.orientation = Some(if orientation == "horizontal" {
            "horizontal"
        } else {
            "vertical"
        });
    }

    pub(crate) fn set_x_axis_title(&mut self, title: &str) {
        let title = trim(title).to_string();
        match &mut self.x_axis {
            XAxis::Band(a) => a.title = title,
            XAxis::Linear(a) => a.title = title,
        }
    }

    pub(crate) fn set_x_axis_range_data(&mut self, min: f64, max: f64) {
        let title = self.x_axis.title().to_string();
        self.x_axis = XAxis::Linear(LinearAxis { title, min, max });
        self.has_set_x_axis = true;
    }

    pub(crate) fn set_x_axis_band(&mut self, categories: &[String]) {
        self.x_axis = XAxis::Band(BandAxis {
            title: self.x_axis.title().to_string(),
            categories: categories.iter().map(|c| trim(c).to_string()).collect(),
        });
        self.has_set_x_axis = true;
    }

    pub(crate) fn set_y_axis_title(&mut self, title: &str) {
        self.y_axis.title = trim(title).to_string();
    }

    pub(crate) fn set_y_axis_range_data(&mut self, min: f64, max: f64) {
        self.y_axis.min = min;
        self.y_axis.max = max;
        self.has_set_y_axis = true;
    }

    /// Each value with its place on the x axis, as Pele's plotData gives them.
    fn plot_data(&mut self, mut values: Vec<f64>) -> Vec<(String, Option<f64>)> {
        if values.is_empty() {
            return Vec::new();
        }
        // The first series fixes the x range when no x-axis statement did.
        if !self.has_set_x_axis {
            self.set_x_axis_range_data(1.0, values.len() as f64);
        }
        if let XAxis::Band(x) = &self.x_axis {
            values.truncate(x.categories.len());
        }
        if !self.has_set_y_axis {
            let (mut min, mut max) = (self.y_axis.min, self.y_axis.max);
            for &value in &values {
                if value < min {
                    min = value;
                }
                if value > max {
                    max = value;
                }
            }
            self.y_axis.min = min;
            self.y_axis.max = max;
        }
        let x = match &self.x_axis {
            XAxis::Band(x) => {
                return x
                    .categories
                    .iter()
                    .enumerate()
                    .map(|(i, c)| (c.clone(), values.get(i).copied()))
                    .collect();
            }
            XAxis::Linear(x) => x,
        };
        if values.len() == 1 {
            return vec![(to_string(x.min), Some(values[0]))];
        }
        let step = (x.max - x.min) / (values.len() - 1) as f64;
        values
            .iter()
            .enumerate()
            .map(|(i, &v)| (to_string(x.min + i as f64 * step), Some(v)))
            .collect()
    }

    /// Adds a series, as Pele's addPlot does.
    fn add_plot(&mut self, kind: PlotType, title: &str, data: &[DataPoint]) {
        let values = data.iter().map(|d| d.value).collect();
        let mut plot = Plot {
            kind,
            title: trim(title).to_string(),
            data: self.plot_data(values),
            point_labels: None,
        };
        // Mermaid accepts labels on the points of a bar series and ignores them.
        if kind == PlotType::Line && data.iter().any(|d| !trim(&d.label).is_empty()) {
            plot.point_labels = Some(data.iter().map(|d| trim(&d.label).to_string()).collect());
        }
        self.plots.push(plot);
    }

    pub(crate) fn set_line_data(&mut self, title: &str, data: &[DataPoint]) {
        self.add_plot(PlotType::Line, title, data);
    }

    pub(crate) fn set_bar_data(&mut self, title: &str, data: &[DataPoint]) {
        self.add_plot(PlotType::Bar, title, data);
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
}
