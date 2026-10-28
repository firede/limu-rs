//! The tokens of an XY chart, as Pele's tokens.ts has them.

/// A token's type, Pele's T, in its order.
#[allow(
    non_camel_case_types,
    clippy::upper_case_acronyms,
    clippy::enum_variant_names
)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum T {
    END,
    ERROR,
    NEWLINE,
    title,
    acc_title,
    acc_title_value,
    acc_descr,
    acc_descr_value,
    acc_descr_multiline_value,
    XYCHART,
    CHART_ORIENTATION,
    X_AXIS,
    Y_AXIS,
    ARROW_DELIMITER,
    LINE,
    BAR,
    NUMBER_WITH_DECIMAL,
    SQUARE_BRACES_START,
    SQUARE_BRACES_END,
    STR,
    ALPHA,
    COLON,
    PLUS,
    COMMA,
    EQUALS,
    MULT,
    BRKT,
    UNDERSCORE,
    DOT,
    AMP,
    MINUS,
    NUM,
    SEMI,
    EOF,
}

/// The names of the types, as Jison's errors give them, in the order of T, Pele's
/// TOKEN_NAMES.
pub(crate) const TOKEN_NAMES: [&str; 34] = [
    "$end",
    "INVALID",
    "NEWLINE",
    "title",
    "acc_title",
    "acc_title_value",
    "acc_descr",
    "acc_descr_value",
    "acc_descr_multiline_value",
    "XYCHART",
    "CHART_ORIENTATION",
    "X_AXIS",
    "Y_AXIS",
    "ARROW_DELIMITER",
    "LINE",
    "BAR",
    "NUMBER_WITH_DECIMAL",
    "SQUARE_BRACES_START",
    "SQUARE_BRACES_END",
    "STR",
    "ALPHA",
    "COLON",
    "PLUS",
    "COMMA",
    "EQUALS",
    "MULT",
    "BRKT",
    "UNDERSCORE",
    "DOT",
    "AMP",
    "MINUS",
    "NUM",
    "SEMI",
    "EOF",
];

impl T {
    /// The type's name, as Jison's errors give it.
    pub(crate) fn name(self) -> &'static str {
        TOKEN_NAMES[self as usize]
    }
}
