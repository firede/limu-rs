//! The tokens of an entity relationship diagram, as Pele's tokens.ts has them.

/// A token's type, Pele's T, with Pele's names.
#[allow(
    non_camel_case_types,
    clippy::upper_case_acronyms,
    clippy::enum_variant_names
)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum T {
    END,
    ERROR,
    ER_DIAGRAM,
    EOF,
    NEWLINE,
    COLON,
    STYLE_SEPARATOR,
    BLOCK_START,
    BLOCK_STOP,
    SQS,
    SQE,
    acc_title,
    acc_title_value,
    acc_descr,
    acc_descr_value,
    acc_descr_multiline_value,
    END_KW,
    SUBGRAPH,
    direction_tb,
    direction_bt,
    direction_rl,
    direction_lr,
    CLASSDEF,
    UNICODE_TEXT,
    STYLE_TEXT,
    COMMA,
    CLASS,
    STYLE,
    SEMI,
    NUM,
    BRKT,
    ENTITY_NAME,
    DECIMAL_NUM,
    ENTITY_ONE,
    ATTRIBUTE_WORD,
    ATTRIBUTE_KEY,
    COMMENT,
    ZERO_OR_ONE,
    ZERO_OR_MORE,
    ONE_OR_MORE,
    ONLY_ONE,
    MD_PARENT,
    NON_IDENTIFYING,
    IDENTIFYING,
    WORD,
    /// A character no rule names. The grammar gives it meaning only as `?` and `,` inside a
    /// block.
    CHAR,
}

/// The direction tokens, in the order of Pele's DIRECTIONS.
pub(crate) const DIRECTION_TOKENS: [T; 4] = [
    T::direction_tb,
    T::direction_bt,
    T::direction_rl,
    T::direction_lr,
];

impl T {
    /// The token's name, as Jison's errors name it, Pele's TOKEN_NAMES.
    pub(crate) fn name(self) -> &'static str {
        TOKEN_NAMES[self as usize]
    }
}

const TOKEN_NAMES: [&str; T::CHAR as usize + 1] = [
    "$end",
    "INVALID",
    "ER_DIAGRAM",
    "EOF",
    "NEWLINE",
    "COLON",
    "STYLE_SEPARATOR",
    "BLOCK_START",
    "BLOCK_STOP",
    "SQS",
    "SQE",
    "acc_title",
    "acc_title_value",
    "acc_descr",
    "acc_descr_value",
    "acc_descr_multiline_value",
    "END",
    "SUBGRAPH",
    "direction_tb",
    "direction_bt",
    "direction_rl",
    "direction_lr",
    "CLASSDEF",
    "UNICODE_TEXT",
    "STYLE_TEXT",
    "COMMA",
    "CLASS",
    "STYLE",
    "SEMI",
    "NUM",
    "BRKT",
    "ENTITY_NAME",
    "DECIMAL_NUM",
    "ENTITY_ONE",
    "ATTRIBUTE_WORD",
    "ATTRIBUTE_KEY",
    "COMMENT",
    "ZERO_OR_ONE",
    "ZERO_OR_MORE",
    "ONE_OR_MORE",
    "ONLY_ONE",
    "MD_PARENT",
    "NON_IDENTIFYING",
    "IDENTIFYING",
    "WORD",
    "CHAR",
];
