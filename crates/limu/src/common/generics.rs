//! Generic types written with tildes, as Pele's src/diagrams/common/generics.ts reads them.

/// `List~int~` as `List<int>`, with the quirks of Mermaid's parseGenericTypes around commas,
/// as Pele's parseGenericTypes turns it. Tildes are ASCII, so the work is done on bytes.
pub(crate) fn parse_generic_types(input: &str) -> String {
    if !input.contains('~') {
        return input.to_string();
    }
    let sets = split_commas(input);
    let mut out: Vec<String> = Vec::new();
    let mut i = 0;
    while i < sets.len() {
        let mut set = sets[i].to_string();
        if set == ","
            && i > 0
            && i + 1 < sets.len()
            && tildes(sets[i - 1]) == 1
            && tildes(sets[i + 1]) == 1
        {
            set = format!("{},{}", sets[i - 1], sets[i + 1]);
            i += 1;
            out.pop();
        }
        out.push(pair_tildes(&set));
        i += 1;
    }
    out.concat()
}

/// How many tildes text has, as Pele's tildes counts them.
fn tildes(text: &str) -> usize {
    text.bytes().filter(|&c| c == b'~').count()
}

/// JavaScript's split(/(,)/): the pieces between commas, and each comma.
fn split_commas(s: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut rest = s;
    while let Some(i) = rest.find(',') {
        out.extend([&rest[..i], ","]);
        rest = &rest[i + 1..];
    }
    out.push(rest);
    out
}

/// Pairs tildes from the outside in, the first with the last and the second with the one
/// before it, as Pele's pairTildes does.
fn pair_tildes(text: &str) -> String {
    let count = tildes(text);
    if count <= 1 {
        return text.to_string();
    }
    let lead = !count.is_multiple_of(2) && text.starts_with('~');
    let mut b = text.as_bytes().to_vec();
    let at: Vec<usize> = (usize::from(lead)..b.len())
        .filter(|&i| b[i] == b'~')
        .collect();
    let (mut lo, mut hi) = (0, at.len() - 1);
    while lo < hi {
        b[at[lo]] = b'<';
        b[at[hi]] = b'>';
        (lo, hi) = (lo + 1, hi - 1);
    }
    String::from_utf8(b).expect("tildes replaced by ASCII")
}
