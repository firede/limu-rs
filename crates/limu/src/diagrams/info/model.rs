//! The model of an info diagram, as Pele's model.ts makes it.

use super::parser::InfoAst;
use crate::js::chars::{drop_indents, trim_start};

/// A parsed info diagram, Pele's InfoModel.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct InfoModel {
    pub(crate) title: Option<String>,
    pub(crate) acc_title: Option<String>,
    pub(crate) acc_descr: Option<String>,
}

/// Makes the model of an info diagram's tree, as Pele's buildInfo does. A title the text
/// gives takes the place of the one its front matter gives, unless it is empty, as `||`
/// has it.
pub(crate) fn build_info(ast: InfoAst, title: Option<&str>) -> InfoModel {
    let given = |s: Option<String>| s.filter(|s| !s.is_empty());
    InfoModel {
        title: given(ast.title).or_else(|| title.map(str::to_string)),
        acc_title: given(ast.acc_title).map(|s| trim_start(&s).to_string()),
        acc_descr: given(ast.acc_descr).map(|s| drop_indents(&s)),
    }
}
