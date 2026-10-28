//! The parser of radar charts, as Pele's parser.ts reads them.

use std::sync::OnceLock;

use crate::common::tokens::{
    ACC_DESCR, ACC_TITLE, DIRECTIVE, END, NEWLINE, Pattern, Reader, SINGLE_LINE_COMMENT, STRING,
    Scan, TITLE, Table, TokenType, WHITESPACE, YAML, acc_descr, acc_descr_value, acc_title_value,
    keyword, match_title, string_value, title_value, token, tokenize,
};
use crate::detect::DiagramType;
use crate::errors::Error;
use crate::js::chars::space_run;
use crate::js::number::number;

// The kinds of token, by index in RADAR_TOKENS, as Pele's T.
const T_RADAR_COLON: usize = 0;
const T_RADAR: usize = 1;
const T_SHOW_LEGEND: usize = 2;
const T_GRATICULE: usize = 3;
const T_CURVE: usize = 4;
const T_TICKS: usize = 5;
const T_AXIS: usize = 6;
const T_MAX: usize = 7;
const T_MIN: usize = 8;
const T_COLON: usize = 9;
const T_COMMA: usize = 10;
const T_OPEN: usize = 11;
const T_CLOSE: usize = 12;
const T_OPEN_BRACE: usize = 13;
const T_CLOSE_BRACE: usize = 14;
const T_GRATICULE_VALUE: usize = 15;
const T_BOOLEAN: usize = 16;
const T_ACC_DESCR: usize = 17;
const T_ACC_TITLE: usize = 18;
const T_TITLE: usize = 19;
const T_NUMBER: usize = 20;
const T_STRING: usize = 21;
const T_ID: usize = 22;
const T_NEWLINE: usize = 23;

/// The end of the run of ASCII digits from byte at, as Pele's digits gives it.
fn digits(src: &str, at: usize) -> usize {
    let b = src.as_bytes();
    let mut end = at;
    while end < b.len() && b[end].is_ascii_digit() {
        end += 1;
    }
    end
}

/// Makes a matcher that answers as NUMBER's pattern,
/// /(?:[0-9]+\.[0-9]+(?!\.))|(?:0|[1-9][0-9]*(?!\.))/y, does, as Pele's matchNumber does. As a
/// regular expression it reads to the end of a run of digits from every position in the run,
/// which is quadratic for a long run of zeros, so the matcher remembers the last run it read.
fn number_scanner() -> Scan {
    let (mut run_from, mut run_end) = (0, 0);
    Box::new(move |src: &str, at: usize, _| {
        let b = src.as_bytes();
        if at < run_from || at >= run_end {
            run_from = at;
            run_end = digits(src, at);
        }
        let whole = run_end;
        if whole == at {
            return None;
        }
        let dot = b.get(whole) == Some(&b'.');
        if dot {
            let fraction = digits(src, whole + 1);
            let length = fraction - whole - 1;
            if length > 0 && b.get(fraction) != Some(&b'.') {
                return Some(fraction);
            }
            if length > 1 {
                return Some(fraction - 1);
            }
        }
        if b[at] == b'0' {
            return Some(at + 1);
        }
        if !dot {
            return Some(whole);
        }
        (whole - 1 > at).then_some(whole - 1)
    })
}

/// Makes a matcher of ACC_DESCR, as Pele's matchAccDescr matches it. In the other grammars an
/// `accDescr {` that never closes ends the lexing. Here `accDescr` is also a name and `{` a
/// token, so each unclosed one would read to the end of the text again. Where the braces of
/// /[\t ]*accDescr\s*{/y open after the last closing brace of the text, the matcher answers no
/// at once.
fn acc_descr_scanner() -> Scan {
    let mut last_brace: Option<Option<usize>> = None;
    Box::new(move |src: &str, at: usize, _| {
        let b = src.as_bytes();
        let mut i = at;
        while i < b.len() && (b[i] == b' ' || b[i] == b'\t') {
            i += 1;
        }
        if src[i..].starts_with("accDescr") {
            let j = space_run(src, i + "accDescr".len());
            if b.get(j) == Some(&b'{') {
                let last = *last_brace.get_or_insert_with(|| src.rfind('}'));
                if last.is_none_or(|last| j + 1 > last) {
                    return None;
                }
            }
        }
        acc_descr(src, at)
    })
}

/// Whether \w, an ASCII letter, digit or underscore, is the byte.
fn word(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_'
}

/// The end of the run of [-\w] from byte at.
fn name_run(src: &str, at: usize) -> usize {
    let b = src.as_bytes();
    let mut end = at;
    while end < b.len() && (word(b[end]) || b[end] == b'-') {
        end += 1;
    }
    end
}

/// Pele's ID, /[\w]([-\w]*\w)?/y: a name that does not end in a hyphen.
fn id(src: &str, at: usize) -> Option<usize> {
    let b = src.as_bytes();
    if !b.get(at).is_some_and(|&c| word(c)) {
        return None;
    }
    let mut end = name_run(src, at);
    while !word(b[end - 1]) {
        end -= 1;
    }
    Some(end)
}

/// Whether Pele's RE_NAME_REST, /[-\w]*\w/y, matches at byte at: whether a name goes on there.
fn name_rest(src: &str, at: usize) -> bool {
    src.as_bytes()[at..name_run(src, at)]
        .iter()
        .any(|&c| word(c))
}

/// The end of one of the words at byte at, the first that matches, or none, as an alternation
/// of them matches, for Pele's optionValue: right after its option, or where no longer name
/// goes on after it.
fn option_value(
    src: &str,
    at: usize,
    previous: Option<usize>,
    words: [&str; 2],
    option: usize,
) -> Option<usize> {
    let w = words.iter().find(|w| src[at..].starts_with(*w))?;
    let end = at + w.len();
    if previous == Some(option) {
        return Some(end);
    }
    (!name_rest(src, end)).then_some(end)
}

/// Pele's word: a word that gives way to a longer name.
const fn word_token(text: &'static str) -> TokenType {
    TokenType {
        longer: &[T_ID],
        ..token(text, Pattern::Literal(text))
    }
}

/// Pele's mark: a literal.
const fn mark(text: &'static str) -> TokenType {
    token(text, Pattern::Literal(text))
}

/// Pele's RADAR_TOKENS.
pub(crate) const RADAR_TOKENS: &[TokenType] = &[
    mark("radar-beta:"),
    TokenType {
        longer: &[T_ID],
        ..keyword("radar-beta")
    },
    word_token("showLegend"),
    word_token("graticule"),
    word_token("curve"),
    word_token("ticks"),
    word_token("axis"),
    word_token("max"),
    word_token("min"),
    mark(":"),
    mark(","),
    mark("["),
    mark("]"),
    mark("{"),
    mark("}"),
    token(
        "GRATICULE",
        Pattern::After(|s, at, previous| {
            option_value(s, at, previous, ["circle", "polygon"], T_GRATICULE)
        }),
    ),
    token(
        "BOOLEAN",
        Pattern::After(|s, at, previous| {
            option_value(s, at, previous, ["true", "false"], T_SHOW_LEGEND)
        }),
    ),
    TokenType {
        pattern: Pattern::Scanner(acc_descr_scanner),
        ..ACC_DESCR
    },
    ACC_TITLE,
    TokenType {
        pattern: Pattern::Match(|s, at| match_title(s, at, Some(name_rest))),
        ..TITLE
    },
    token("NUMBER", Pattern::Scanner(number_scanner)),
    STRING,
    token("ID", Pattern::Match(id)),
    NEWLINE,
    WHITESPACE,
    YAML,
    DIRECTIVE,
    SINGLE_LINE_COMMENT,
];

/// RADAR_TOKENS made ready to tokenize with.
pub(crate) fn table() -> &'static Table {
    static TABLE: OnceLock<Table> = OnceLock::new();
    TABLE.get_or_init(|| Table::new(RADAR_TOKENS))
}

/// An axis of the tree, Pele's AxisAst. What Pele leaves undefined is none.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct AxisAst {
    pub(crate) name: String,
    pub(crate) label: Option<String>,
}

/// A value of a curve, Pele's EntryAst, with the name of the axis it is given for where the
/// text names one.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct EntryAst {
    pub(crate) axis: Option<String>,
    pub(crate) value: f64,
}

/// A curve of the tree, Pele's CurveAst.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct CurveAst {
    pub(crate) name: String,
    pub(crate) label: Option<String>,
    pub(crate) entries: Vec<EntryAst>,
}

/// The value of an option, Pele's OptionAst value: a boolean for showLegend, a word for
/// graticule, and a number for the others.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum OptionValue {
    Bool(bool),
    Number(f64),
    Word(String),
}

/// An option of the tree, Pele's OptionAst.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct OptionAst {
    pub(crate) name: String,
    pub(crate) value: OptionValue,
}

/// The tree of a radar chart, Pele's RadarAst. What Pele leaves undefined is none.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct RadarAst {
    pub(crate) title: Option<String>,
    pub(crate) acc_title: Option<String>,
    pub(crate) acc_descr: Option<String>,
    pub(crate) axes: Vec<AxisAst>,
    pub(crate) curves: Vec<CurveAst>,
    pub(crate) options: Vec<OptionAst>,
}

/// Splits a radar chart's text into tokens, as Pele's lexRadar does.
pub(crate) fn lex_radar(src: &str) -> Result<crate::common::tokens::Tokens<'_>, Error> {
    tokenize(src, table(), DiagramType::Radar)
}

fn newlines(r: &mut Reader) {
    while r.kind() == T_NEWLINE {
        r.i += 1;
    }
}

fn end_of_line(r: &mut Reader) -> Result<(), Error> {
    if r.kind() == END {
        return Ok(());
    }
    if r.kind() != T_NEWLINE {
        return Err(r.fail("a line break or the end of input"));
    }
    newlines(r);
    Ok(())
}

fn label(r: &mut Reader) -> Result<Option<String>, Error> {
    if !r.accept(T_OPEN) {
        return Ok(None);
    }
    let text = string_value(r.expect(T_STRING)?);
    r.expect(T_CLOSE)?;
    Ok(Some(text))
}

fn entry(r: &mut Reader, detailed: bool) -> Result<EntryAst, Error> {
    if !detailed {
        let value = number(r.expect(T_NUMBER)?);
        return Ok(EntryAst { axis: None, value });
    }
    let axis = Some(r.expect(T_ID)?.to_string());
    r.accept(T_COLON);
    let value = number(r.expect(T_NUMBER)?);
    Ok(EntryAst { axis, value })
}

fn curve(r: &mut Reader) -> Result<CurveAst, Error> {
    let name = r.expect(T_ID)?.to_string();
    let label = label(r)?;
    r.expect(T_OPEN_BRACE)?;
    newlines(r);
    if r.kind() != T_NUMBER && r.kind() != T_ID {
        return Err(r.fail("a number or an axis name"));
    }
    let detailed = r.kind() == T_ID;
    let mut entries = vec![entry(r, detailed)?];
    while r.accept(T_COMMA) {
        newlines(r);
        entries.push(entry(r, detailed)?);
    }
    newlines(r);
    r.expect(T_CLOSE_BRACE)?;
    Ok(CurveAst {
        name,
        label,
        entries,
    })
}

fn axis(r: &mut Reader) -> Result<AxisAst, Error> {
    let name = r.expect(T_ID)?.to_string();
    let label = label(r)?;
    Ok(AxisAst { name, label })
}

fn option(r: &mut Reader) -> Result<OptionAst, Error> {
    let kind = r.kind();
    let name = r.take().to_string();
    let value = match kind {
        T_SHOW_LEGEND => OptionValue::Bool(r.expect(T_BOOLEAN)? == "true"),
        T_GRATICULE => OptionValue::Word(r.expect(T_GRATICULE_VALUE)?.to_string()),
        _ => OptionValue::Number(number(r.expect(T_NUMBER)?)),
    };
    Ok(OptionAst { name, value })
}

fn is_option(r: &Reader) -> bool {
    matches!(
        r.kind(),
        T_SHOW_LEGEND | T_TICKS | T_MAX | T_MIN | T_GRATICULE
    )
}

/// Parses a radar chart's text into its tree, as Pele's parseRadar does.
pub(crate) fn parse_radar(src: &str) -> Result<RadarAst, Error> {
    let mut r = Reader::new(lex_radar(src)?, table(), DiagramType::Radar);
    let mut ast = RadarAst::default();

    newlines(&mut r);
    if !r.accept(T_RADAR_COLON) {
        r.expect(T_RADAR)?;
        r.accept(T_COLON);
    }
    newlines(&mut r);
    while r.kind() != END {
        match r.kind() {
            T_NEWLINE => r.i += 1,
            T_ACC_DESCR => {
                ast.acc_descr = Some(acc_descr_value(r.take()));
                end_of_line(&mut r)?;
            }
            T_ACC_TITLE => {
                ast.acc_title = Some(acc_title_value(r.take()));
                end_of_line(&mut r)?;
            }
            T_TITLE => {
                ast.title = Some(title_value(r.take()));
                end_of_line(&mut r)?;
            }
            T_AXIS => {
                r.i += 1;
                ast.axes.push(axis(&mut r)?);
                while r.accept(T_COMMA) {
                    ast.axes.push(axis(&mut r)?);
                }
            }
            T_CURVE => {
                r.i += 1;
                ast.curves.push(curve(&mut r)?);
                while r.accept(T_COMMA) {
                    ast.curves.push(curve(&mut r)?);
                }
            }
            _ => {
                if !is_option(&r) {
                    return Err(r.fail("an axis, a curve, an option, a title, or a line break"));
                }
                ast.options.push(option(&mut r)?);
                while r.accept(T_COMMA) {
                    if !is_option(&r) {
                        return Err(r.fail("an option"));
                    }
                    ast.options.push(option(&mut r)?);
                }
            }
        }
    }
    Ok(ast)
}
