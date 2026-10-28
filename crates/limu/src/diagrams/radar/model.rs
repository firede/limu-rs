//! The model of a radar chart, as Pele's model.ts makes it.

use std::collections::HashMap;

use super::parser::{OptionValue, RadarAst};
use crate::detect::DiagramType;
use crate::errors::{Error, ErrorCode};
use crate::js::chars::{drop_indents, trim_start};

/// An axis of the chart, Pele's RadarAxis: its name, and the label it is drawn with, which is
/// its name where the text gives none.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct RadarAxis {
    pub(crate) name: String,
    pub(crate) label: String,
}

/// A series, Pele's RadarCurve: its name, its label, and its values, in the order of the axes
/// where the text names them.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct RadarCurve {
    pub(crate) name: String,
    pub(crate) label: String,
    pub(crate) entries: Vec<f64>,
}

/// The chart's own options, Pele's RadarOptions. max is none where Pele leaves it null, for
/// the largest value to give it. graticule is "circle" or "polygon".
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct RadarOptions {
    pub(crate) show_legend: bool,
    pub(crate) ticks: f64,
    pub(crate) max: Option<f64>,
    pub(crate) min: f64,
    pub(crate) graticule: String,
}

/// A parsed radar chart, Pele's RadarModel.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct RadarModel {
    pub(crate) title: Option<String>,
    pub(crate) acc_title: Option<String>,
    pub(crate) acc_descr: Option<String>,
    pub(crate) axes: Vec<RadarAxis>,
    pub(crate) curves: Vec<RadarCurve>,
    pub(crate) options: RadarOptions,
}

/// The most rings a chart's graticule has, Pele's MAX_TICKS.
const MAX_TICKS: f64 = 32.0;

fn invalid(message: String) -> Error {
    Error {
        diagram: Some(DiagramType::Radar),
        ..Error::new(message, ErrorCode::Semantic)
    }
}

/// Makes the model of a radar chart's tree, as Pele's buildRadar does. A curve whose values
/// name their axes has them put in the order of the axes, the first value for each axis
/// counting, and an option given more than once takes its last value. A title the text gives
/// takes the place of the one its front matter gives, unless it is empty, as `||` has it.
pub(crate) fn build_radar(ast: RadarAst, title: Option<&str>) -> Result<RadarModel, Error> {
    let axes: Vec<RadarAxis> = ast
        .axes
        .into_iter()
        .map(|axis| RadarAxis {
            label: axis.label.unwrap_or_else(|| axis.name.clone()),
            name: axis.name,
        })
        .collect();

    let mut curves = Vec::new();
    for curve in ast.curves {
        let entries = if curve.entries[0].axis.is_none() {
            curve.entries.iter().map(|entry| entry.value).collect()
        } else {
            if axes.is_empty() {
                return Err(invalid(
                    "Axes must be populated before curves for reference entries".into(),
                ));
            }
            let mut by_axis = HashMap::new();
            for entry in &curve.entries {
                if let Some(axis) = &entry.axis {
                    by_axis.entry(axis.as_str()).or_insert(entry.value);
                }
            }
            let mut entries = Vec::new();
            for axis in &axes {
                let Some(&value) = by_axis.get(axis.name.as_str()) else {
                    return Err(invalid(format!("Missing entry for axis {}", axis.label)));
                };
                entries.push(value);
            }
            entries
        };
        curves.push(RadarCurve {
            label: curve.label.unwrap_or_else(|| curve.name.clone()),
            name: curve.name,
            entries,
        });
    }

    let mut options = RadarOptions {
        show_legend: true,
        ticks: 5.0,
        max: None,
        min: 0.0,
        graticule: "circle".into(),
    };
    for option in ast.options {
        match (option.name.as_str(), option.value) {
            ("showLegend", OptionValue::Bool(v)) => options.show_legend = v,
            ("graticule", OptionValue::Word(v)) => options.graticule = v,
            ("ticks", OptionValue::Number(v)) => options.ticks = v,
            ("max", OptionValue::Number(v)) => options.max = Some(v),
            ("min", OptionValue::Number(v)) => options.min = v,
            _ => {}
        }
    }
    if options.ticks > MAX_TICKS {
        options.ticks = MAX_TICKS;
    }

    let given = |s: Option<String>| s.filter(|s| !s.is_empty());
    Ok(RadarModel {
        title: given(ast.title).or_else(|| title.map(str::to_string)),
        acc_title: given(ast.acc_title).map(|s| trim_start(&s).to_string()),
        acc_descr: given(ast.acc_descr).map(|s| drop_indents(&s)),
        axes,
        curves,
        options,
    })
}
