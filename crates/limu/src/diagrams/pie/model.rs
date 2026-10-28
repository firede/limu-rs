//! The model of a pie chart, as Pele's model.ts makes it.

use std::collections::HashSet;

use super::parser::PieAst;
use crate::detect::DiagramType;
use crate::errors::{Error, ErrorCode};
use crate::js::chars::{drop_indents, trim_start};
use crate::js::number::to_string;

/// A parsed pie chart, Pele's PieModel. Its sections, a label with its value, are in the
/// order their labels first appear. A label written again keeps its first value, as Pele's
/// Map of sections does.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct PieModel {
    pub(crate) title: Option<String>,
    pub(crate) acc_title: Option<String>,
    pub(crate) acc_descr: Option<String>,
    pub(crate) show_data: bool,
    pub(crate) sections: Vec<(String, f64)>,
}

/// Makes the model of a pie chart's tree, as Pele's buildPie does. A negative value is an
/// error, and a label written again is left out. A title the text gives takes the place of
/// the one its front matter gives, unless it is empty, as `||` has it.
pub(crate) fn build_pie(ast: PieAst, title: Option<&str>) -> Result<PieModel, Error> {
    let mut seen = HashSet::new();
    let mut sections = Vec::new();
    for (label, value) in ast.sections {
        if value < 0.0 {
            let message = format!(
                "\"{label}\" has invalid value: {}. Negative values are not allowed in pie \
                 charts. All slice values must be >= 0.",
                to_string(value)
            );
            return Err(Error {
                diagram: Some(DiagramType::Pie),
                ..Error::new(message, ErrorCode::Semantic)
            });
        }
        if seen.insert(label.clone()) {
            sections.push((label, value));
        }
    }
    let given = |s: Option<String>| s.filter(|s| !s.is_empty());
    Ok(PieModel {
        title: given(ast.title).or_else(|| title.map(str::to_string)),
        acc_title: given(ast.acc_title).map(|s| trim_start(&s).to_string()),
        acc_descr: given(ast.acc_descr).map(|s| drop_indents(&s)),
        show_data: ast.show_data,
        sections,
    })
}
