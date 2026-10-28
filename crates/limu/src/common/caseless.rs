//! Words matched without regard to case, as Pele's diagrams/common/caseless.ts matches them.

/// Whether word, written in lower case, is at byte at of src, as Pele's wordAt says. Only
/// ASCII letters match in either case, as with the i flag on a regular expression without
/// the u flag. A word that runs past the end of src is not there.
pub(crate) fn word_at(src: &str, at: usize, word: &str) -> bool {
    let (s, w) = (src.as_bytes(), word.as_bytes());
    s.get(at..at + w.len()).is_some_and(|s| {
        s.iter()
            .zip(w)
            .all(|(&c, &w)| c == w || (w.is_ascii_lowercase() && c == w - 32))
    })
}
