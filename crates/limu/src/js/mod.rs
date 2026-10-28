//! What JavaScript does wherever it affects what is measured or placed, as V8 does it: its
//! arithmetic, its conversions between numbers and strings, and how it reads text.

// The parsers and layout that use these are ported after them.
#![allow(dead_code)]

pub(crate) mod chars;
pub(crate) mod math;
pub(crate) mod number;

#[cfg(test)]
mod tests;
