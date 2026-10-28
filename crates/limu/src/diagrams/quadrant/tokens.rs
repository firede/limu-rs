//! The tokens of a quadrant chart, as Pele's tokens.ts has them.

/// A token's type, Pele's T. The parser counts the quadrant keywords from QUADRANT_1, so the
/// order is Pele's.
#[allow(
    non_camel_case_types,
    clippy::upper_case_acronyms,
    clippy::enum_variant_names
)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
pub(crate) enum T {
    END,
    ERROR,
    NEWLINE,
    title,
    title_value,
    acc_title,
    acc_title_value,
    acc_descr,
    acc_descr_value,
    acc_descr_multiline_value,
    X_AXIS,
    Y_AXIS,
    DELIMITER,
    QUADRANT_1,
    QUADRANT_2,
    QUADRANT_3,
    QUADRANT_4,
    CLASSDEF,
    MD_STR,
    STR,
    class_name,
    point_start,
    point_x,
    point_y,
    QUADRANT,
    ALPHA,
    UNICODE_TEXT,
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
    SPACE,
    SEMI,
    PUNCTUATION,
    EOF,
}

/// The names of the types, as Jison's errors give them, in the order of T, Pele's
/// TOKEN_NAMES.
pub(crate) const TOKEN_NAMES: [&str; 42] = [
    "$end",
    "INVALID",
    "NEWLINE",
    "title",
    "title_value",
    "acc_title",
    "acc_title_value",
    "acc_descr",
    "acc_descr_value",
    "acc_descr_multiline_value",
    "X-AXIS",
    "Y-AXIS",
    "AXIS-TEXT-DELIMITER",
    "QUADRANT_1",
    "QUADRANT_2",
    "QUADRANT_3",
    "QUADRANT_4",
    "CLASSDEF",
    "MD_STR",
    "STR",
    "class_name",
    "point_start",
    "point_x",
    "point_y",
    "QUADRANT",
    "ALPHA",
    "UNICODE_TEXT",
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
    "SPACE",
    "SEMI",
    "PUNCTUATION",
    "EOF",
];

/// The quadrant keywords, from the first quadrant to the fourth.
pub(crate) const QUADRANTS: [T; 4] = [T::QUADRANT_1, T::QUADRANT_2, T::QUADRANT_3, T::QUADRANT_4];

impl T {
    /// The type's name, as Jison's errors give it.
    pub(crate) fn name(self) -> &'static str {
        TOKEN_NAMES[self as usize]
    }
}
