//! The parser of sankey diagrams, as Pele's parser.ts reads them: the keyword, a line break,
//! and rows of a source, a target and an amount, separated by commas, each field plain or in
//! double quotes, with a doubled quote for a quote in it.

use super::db::SankeyDb;
use super::lexer::{
    T_COMMA, T_DQUOTE, T_END, T_EOF, T_ESCAPED_TEXT, T_NEWLINE, T_NON_ESCAPED_TEXT, T_SANKEY,
    T_STUCK, TOKEN_NAMES, tokenize,
};
use crate::detect::DiagramType;
use crate::errors::{Edit, Error, Steps, input_offset, push, syntax_error};
use crate::js::chars::{is_space, trim, trim_start, unit_at, unit_before};
use crate::js::number::parse_float;
use crate::text::decode_entities;

/// Whether code unit c is white space to `\s` that does not break a line, as Pele's isBlank
/// says.
fn is_blank(c: i32) -> bool {
    c != 10 && c != 13 && is_space(c)
}

/// Takes the blanks off the start and end of each row, outside quoted fields, as Pele's
/// trimRows does, and adds an edit for each run it takes to edits. Blanks before a comma at
/// the start of a row are kept, as Mermaid reads them as an empty name.
fn trim_rows(text: &str, edits: &mut Vec<Edit>) -> String {
    let mut end = text.len();
    while end > 0 && is_blank(unit_before(text, end)) {
        end -= text[..end].chars().next_back().map_or(1, char::len_utf8);
    }
    let mut out = String::new();
    let mut kept = 0;
    let mut quoted = false;
    let b = text.as_bytes();
    let mut i = 0;
    while i < end {
        let (c, size) = unit_at(text, i);
        if quoted {
            if c == 34 {
                if b.get(i + 1) == Some(&b'"') {
                    i += 1;
                } else {
                    quoted = false;
                }
            }
            i += size;
        } else if c == 34 {
            quoted = true;
            i += 1;
        } else if is_blank(c) {
            let mut j = i + size;
            loop {
                let (d, n) = unit_at(text, j);
                if !is_blank(d) {
                    break;
                }
                j += n;
            }
            let (p, q) = (unit_before(text, i), unit_at(text, j).0);
            // Blank first fields are kept, which Mermaid reads as empty names.
            if i == 0 || q == 10 || q == 13 || (p == 10 || p == 13) && q != 44 {
                out.push_str(&text[kept..i]);
                edits.push(Edit::cut(i, j));
                kept = j;
            }
            i = j;
        } else {
            i += size;
        }
    }
    if end < text.len() {
        edits.push(Edit::cut(end, text.len()));
    }
    out.push_str(&text[kept..end]);
    out
}

/// Prepares a text as Pele's prepareTextForParsing does, which is as Mermaid's sankeyUtils
/// does but that blanks are also taken off the ends of each row outside quoted fields: runs
/// of line breaks then become one, so that no row is empty, and what is left is trimmed. The
/// edits of each of these passes are added to steps, where they are recorded, so that an
/// error can be reported where it is in the text as written.
pub(crate) fn prepare_text_for_parsing(text: &str, mut steps: Steps) -> String {
    let mut edits = Vec::new();
    let rows = trim_rows(text, &mut edits);
    // .replace(/[\n\r]+/g, '\n')
    let (mut breaks, mut collapsed) = (Vec::new(), String::new());
    let mut chars = rows.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        if c != '\n' && c != '\r' {
            collapsed.push(c);
            continue;
        }
        let mut j = i + 1;
        while let Some(&(k, '\n' | '\r')) = chars.peek() {
            j = k + 1;
            chars.next();
        }
        breaks.push(Edit {
            from: i,
            to: j,
            text: "\n".into(),
        });
        collapsed.push('\n');
    }
    let trimmed = trim(&collapsed);
    let lead = collapsed.len() - trim_start(&collapsed).len();
    push(&mut steps, edits);
    push(&mut steps, breaks);
    push(
        &mut steps,
        vec![
            Edit::cut(0, lead),
            Edit::cut(lead + trimmed.len(), collapsed.len()),
        ],
    );
    trimmed.to_string()
}

/// Whether s holds a character XML does not allow, as Pele's RE_INVALID_XML finds one: a
/// control but for the tab, the line feed and the carriage return, or U+FFFE or U+FFFF. A
/// lone surrogate, which the regular expression finds too, is never decoded from an entity,
/// and a Rust string holds none.
fn invalid_xml(s: &str) -> bool {
    s.chars().any(|c| {
        matches!(c, '\0'..='\x08' | '\x0b' | '\x0c' | '\x0e'..='\x1f' | '\u{fffe}' | '\u{ffff}')
    })
}

/// Parses a prepared text into db, as Pele's parseSankey does. An error is reported in
/// source, the text as it was before prepare_text_for_parsing, which is src where src was not
/// prepared.
pub(crate) fn parse_sankey(src: &str, db: &mut SankeyDb, source: &str) -> Result<(), Error> {
    let t = tokenize(src);
    let (types, texts, starts) = (&t.types, &t.texts, &t.starts);
    let mut i = 0;

    let error = |at: usize, detail: &str| -> Error {
        let mut steps = Vec::new();
        if source != src {
            prepare_text_for_parsing(source, Some(&mut steps));
        }
        // The end of src is mapped to just after its last character, before the blanks that
        // were trimmed after it.
        let offset = if at > 0 && at >= src.len() {
            let size = src.chars().next_back().map_or(1, char::len_utf8);
            input_offset(&steps, src.len() - size) + size
        } else {
            input_offset(&steps, at)
        };
        syntax_error(DiagramType::Sankey, source, offset, detail, false)
    };
    let fail = |i: usize, expected: &str| -> Error {
        let kind = types[i];
        let got = if kind == T_STUCK {
            TOKEN_NAMES[types[i - 1]]
        } else {
            TOKEN_NAMES[kind]
        };
        error(starts[i], &format!("Expecting {expected}, got '{got}'"))
    };
    let expect = |i: &mut usize, kind: usize, also: &str| -> Result<(), Error> {
        if types[*i] != kind {
            return Err(fail(*i, &format!("'{}'{also}", TOKEN_NAMES[kind])));
        }
        *i += 1;
        Ok(())
    };
    let field = |i: &mut usize| -> Result<String, Error> {
        if types[*i] == T_NON_ESCAPED_TEXT {
            *i += 1;
            return Ok(texts[*i - 1].to_string());
        }
        if types[*i] != T_DQUOTE {
            return Err(fail(*i, "'DQUOTE', 'NON_ESCAPED_TEXT'"));
        }
        *i += 1;
        if types[*i] != T_ESCAPED_TEXT {
            return Err(fail(*i, "'ESCAPED_TEXT'"));
        }
        let text = texts[*i];
        *i += 1;
        expect(i, T_DQUOTE, "")?;
        Ok(text.to_string())
    };
    let name = |i: &mut usize| -> Result<String, Error> {
        let start = starts[*i];
        let text = trim(&field(i)?).replace("\"\"", "\"");
        // Entity codes can stand for characters the lexer leaves out.
        if invalid_xml(&decode_entities(&text)) {
            return Err(error(start, "Invalid XML character in node name"));
        }
        Ok(text)
    };

    expect(&mut i, T_SANKEY, "")?;
    expect(&mut i, T_NEWLINE, "")?;
    loop {
        let from = name(&mut i)?;
        expect(&mut i, T_COMMA, ", 'NEWLINE', 'EOF'")?;
        let to = name(&mut i)?;
        expect(&mut i, T_COMMA, ", 'NEWLINE', 'EOF'")?;
        let start = starts[i];
        // What parseFloat reads at the start of the field is the value, as in `50%`, as
        // Mermaid reads it, but a value that is not finite is an error.
        let value = parse_float(&field(&mut i)?);
        if !value.is_finite() {
            return Err(error(start, "Expecting a number"));
        }
        let source = db.find_or_create_node(&from);
        let target = db.find_or_create_node(&to);
        db.add_link(source, target, value);
        if types[i] != T_NEWLINE {
            break;
        }
        i += 1;
    }
    if types[i] == T_EOF {
        i += 1;
    }
    if types[i] != T_END {
        return Err(fail(i, "'NEWLINE', 'EOF'"));
    }
    Ok(())
}
