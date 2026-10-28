//! What JavaScript does wherever it affects what is measured or placed, as V8 does it: its
//! arithmetic, its conversions between numbers and strings, and how it reads text.

pub(crate) mod chars;
pub(crate) mod json;
pub(crate) mod math;
pub(crate) mod number;

#[cfg(test)]
mod tests;
