//! What a quadrant chart's parser fills, as Pele's QuadrantDb in db.ts has it: the labels of
//! its quadrants and axes, its points and the classes of their styles.

use crate::detect::DiagramType;
use crate::errors::{Error, ErrorCode};
use crate::js::chars::{drop_indents, is_digit, is_hex, trim, trim_end, trim_start};
use crate::js::map::Ordered;
use crate::js::number::{number, parse_int};

/// A label's text, and whether it is markdown, Pele's QuadrantText.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct QuadrantText {
    pub(crate) text: String,
    pub(crate) markdown: bool,
}

/// How a point is drawn, where its styles or its class say, Pele's PointStyle.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct PointStyle {
    pub(crate) radius: Option<f64>,
    pub(crate) color: Option<String>,
    pub(crate) stroke_color: Option<String>,
    pub(crate) stroke_width: Option<String>,
}

/// A point, Pele's QuadrantPoint. Its place is a number as JavaScript reads it, which is NaN
/// for what Number cannot read.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct QuadrantPoint {
    pub(crate) x: f64,
    pub(crate) y: f64,
    pub(crate) text: QuadrantText,
    pub(crate) class_name: String,
    pub(crate) style: PointStyle,
}

/// A quadrant chart as its parser fills it, Pele's QuadrantDb.
#[derive(Clone, Debug, Default)]
pub(crate) struct QuadrantDb {
    pub(crate) title: Option<String>,
    pub(crate) acc_title: Option<String>,
    pub(crate) acc_descr: Option<String>,
    /// Quadrants 1 to 4: top right, top left, bottom left, bottom right.
    pub(crate) quadrants: [Option<QuadrantText>; 4],
    pub(crate) x_axis_left: Option<QuadrantText>,
    pub(crate) x_axis_right: Option<QuadrantText>,
    pub(crate) y_axis_bottom: Option<QuadrantText>,
    pub(crate) y_axis_top: Option<QuadrantText>,
    pub(crate) points: Vec<QuadrantPoint>,
    /// The classes, each with its name, in the order they were first defined.
    pub(crate) classes: Ordered<(String, PointStyle)>,
}

/// /^\d+$/.
fn is_number(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| is_digit(i32::from(b)))
}

/// /^#?([\dA-Fa-f]{6}|[\dA-Fa-f]{3})$/.
fn is_hex_code(s: &str) -> bool {
    let s = s.strip_prefix('#').unwrap_or(s);
    matches!(s.len(), 3 | 6) && s.bytes().all(|b| is_hex(i32::from(b)))
}

/// /^\d+px$/.
fn is_pixels(s: &str) -> bool {
    s.strip_suffix("px").is_some_and(is_number)
}

fn semantic(message: String) -> Error {
    let mut e = Error::new(message, ErrorCode::Semantic);
    e.diagram = Some(DiagramType::QuadrantChart);
    e
}

fn invalid(style: &str, value: &str, kind: &str) -> Error {
    semantic(format!(
        "value for {style} {value} is invalid, please use a valid {kind}"
    ))
}

/// The first two parts of Mermaid's `style.trim().split(/\s*:\s*/)`, found in one pass, as
/// Pele's keyValue finds them. A style with no colon has the value "undefined".
fn key_value(style: &str) -> (&str, &str) {
    let s = trim(style);
    let Some(colon) = s.find(':') else {
        return (s, "undefined");
    };
    let rest = trim_start(&s[colon + 1..]);
    let value = match rest.find(':') {
        Some(next) => trim_end(&rest[..next]),
        None => rest,
    };
    (trim_end(&s[..colon]), value)
}

/// Reads the styles of a point or a class, as Pele's parseStyles does.
pub(crate) fn parse_styles(styles: &[String]) -> Result<PointStyle, Error> {
    let mut out = PointStyle::default();
    for style in styles {
        let (key, value) = key_value(style);
        match key {
            "radius" => {
                if !is_number(value) {
                    return Err(invalid(key, value, "number"));
                }
                out.radius = Some(parse_int(value, 10));
            }
            "color" => {
                if !is_hex_code(value) {
                    return Err(invalid(key, value, "hex code"));
                }
                out.color = Some(value.to_string());
            }
            "stroke-color" => {
                if !is_hex_code(value) {
                    return Err(invalid(key, value, "hex code"));
                }
                out.stroke_color = Some(value.to_string());
            }
            "stroke-width" => {
                if !is_pixels(value) {
                    return Err(invalid(key, value, "number of pixels (eg. 10px)"));
                }
                out.stroke_width = Some(value.to_string());
            }
            _ => return Err(semantic(format!("style named {key} is not supported."))),
        }
    }
    Ok(out)
}

fn trimmed(label: QuadrantText) -> QuadrantText {
    QuadrantText {
        text: trim(&label.text).to_string(),
        markdown: label.markdown,
    }
}

impl QuadrantDb {
    /// Sets the label of quadrant k, from 0, as Pele's setQuadrant1Text to setQuadrant4Text
    /// do.
    pub(crate) fn set_quadrant_text(&mut self, k: usize, label: QuadrantText) {
        self.quadrants[k] = Some(trimmed(label));
    }

    pub(crate) fn set_x_axis_left_text(&mut self, label: QuadrantText) {
        self.x_axis_left = Some(trimmed(label));
    }

    pub(crate) fn set_x_axis_right_text(&mut self, label: QuadrantText) {
        self.x_axis_right = Some(trimmed(label));
    }

    pub(crate) fn set_y_axis_bottom_text(&mut self, label: QuadrantText) {
        self.y_axis_bottom = Some(trimmed(label));
    }

    pub(crate) fn set_y_axis_top_text(&mut self, label: QuadrantText) {
        self.y_axis_top = Some(trimmed(label));
    }

    pub(crate) fn add_point(
        &mut self,
        label: QuadrantText,
        class_name: &str,
        x: &str,
        y: &str,
        styles: &[String],
    ) -> Result<(), Error> {
        let style = parse_styles(styles)?;
        self.points.push(QuadrantPoint {
            x: number(x),
            y: number(y),
            text: trimmed(label),
            class_name: class_name.to_string(),
            style,
        });
        Ok(())
    }

    pub(crate) fn add_class(&mut self, class_name: &str, styles: &[String]) -> Result<(), Error> {
        let style = parse_styles(styles)?;
        self.classes
            .set(class_name, (class_name.to_string(), style));
        Ok(())
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
