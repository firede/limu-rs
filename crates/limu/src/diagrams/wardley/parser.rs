//! The parser of Wardley maps, as Pele's parser.ts reads them.

use std::sync::OnceLock;

use crate::common::tokens::{
    ACC_TITLE, DIRECTIVE, END, NEWLINE, Pattern, Reader, SINGLE_LINE_COMMENT, STRING, Scan, TITLE,
    Table, TokenType, WHITESPACE, YAML, acc_descr_named, acc_descr_value, acc_title_value, int,
    string_value, title_value, token, tokenize,
};
use crate::detect::DiagramType;
use crate::errors::Error;
use crate::js::chars::{is_letter, is_space, is_word, trim, unit};
use crate::js::number::{number, parse_int};

// The kinds of token, by index in WARDLEY_TOKENS, as Pele's T, and those of the hidden types
// after them.
pub(crate) const T_LBRACKET: usize = 0;
pub(crate) const T_COMMA: usize = 1;
pub(crate) const T_RBRACKET: usize = 2;
pub(crate) const T_AT: usize = 3;
pub(crate) const T_SLASH: usize = 4;
pub(crate) const T_LPAREN: usize = 5;
pub(crate) const T_RPAREN: usize = 6;
pub(crate) const T_MINUS: usize = 7;
pub(crate) const T_LBRACE: usize = 8;
pub(crate) const T_RBRACE: usize = 9;
pub(crate) const T_NUMBER: usize = 10;
pub(crate) const T_ARROW: usize = 11;
pub(crate) const T_LINK_PORT: usize = 12;
pub(crate) const T_LINK_ARROW: usize = 13;
pub(crate) const T_LINK_LABEL: usize = 14;
pub(crate) const T_STRATEGY: usize = 15;
pub(crate) const T_WARDLEY: usize = 16;
pub(crate) const T_SIZE: usize = 17;
pub(crate) const T_EVOLUTION: usize = 18;
pub(crate) const T_ANCHOR: usize = 19;
pub(crate) const T_COMPONENT: usize = 20;
pub(crate) const T_LABEL: usize = 21;
pub(crate) const T_INERTIA: usize = 22;
pub(crate) const T_EVOLVE: usize = 23;
pub(crate) const T_PIPELINE: usize = 24;
pub(crate) const T_NOTE: usize = 25;
pub(crate) const T_ANNOTATIONS: usize = 26;
pub(crate) const T_ANNOTATION: usize = 27;
pub(crate) const T_ACCELERATOR: usize = 28;
pub(crate) const T_DEACCELERATOR: usize = 29;
pub(crate) const T_NAME: usize = 30;
pub(crate) const T_WS: usize = 31;
pub(crate) const T_ACC_DESCR: usize = 32;
pub(crate) const T_ACC_TITLE: usize = 33;
pub(crate) const T_TITLE: usize = 34;
pub(crate) const T_INT: usize = 35;
pub(crate) const T_STRING: usize = 36;
pub(crate) const T_ID: usize = 37;
pub(crate) const T_NEWLINE: usize = 38;
pub(crate) const T_WHITESPACE: usize = 39;
pub(crate) const T_YAML: usize = 40;
pub(crate) const T_DIRECTIVE: usize = 41;
pub(crate) const T_COMMENT: usize = 42;

/// The end of the run of ASCII digits from byte i, as Pele's digits finds it.
fn digits(s: &str, mut i: usize) -> usize {
    let b = s.as_bytes();
    while i < b.len() && b[i].is_ascii_digit() {
        i += 1;
    }
    i
}

/// Answers as WARDLEY_NUMBER's pattern, /[0-9]+\.[0-9]+/y, does, as Pele's numberScanner does.
/// The pattern reads a whole run of digits before it finds there is no fraction, and would do
/// so again from every digit of a long run, so the run is remembered.
fn number_scanner() -> Scan {
    let (mut from, mut end) = (0, 0);
    Box::new(move |s, p, _| {
        if p < from || p >= end {
            from = p;
            end = digits(s, p);
        }
        if end == p || s.as_bytes().get(end) != Some(&b'.') {
            return None;
        }
        let stop = digits(s, end + 1);
        (stop > end + 1).then_some(stop)
    })
}

/// Pele's ARROW, /->/y.
fn arrow(s: &str, at: usize) -> Option<usize> {
    s[at..].starts_with("->").then_some(at + 2)
}

/// Pele's LINK_ARROW, /-->|-\.->|>|\+'[^']*'<>|\+'[^']*'<|\+'[^']*'>/y. [^']* can only end at
/// the first quote after the opening one, so each of the last three alternatives looks at what
/// follows that quote. A label may run over lines.
fn link_arrow(s: &str, at: usize) -> Option<usize> {
    let rest = &s[at..];
    if rest.starts_with("-->") {
        return Some(at + 3);
    }
    if rest.starts_with("-.->") {
        return Some(at + 4);
    }
    if rest.starts_with('>') {
        return Some(at + 1);
    }
    if !rest.starts_with("+'") {
        return None;
    }
    let end = at + 2 + s[at + 2..].find('\'')? + 1;
    let after = &s[end..];
    if after.starts_with("<>") {
        Some(end + 2)
    } else if after.starts_with('<') || after.starts_with('>') {
        Some(end + 1)
    } else {
        None
    }
}

/// Pele's LINK_PORT, /\+<>|\+>|\+</y.
fn link_port(s: &str, at: usize) -> Option<usize> {
    let rest = &s[at..];
    if rest.starts_with("+<>") {
        Some(at + 3)
    } else if rest.starts_with("+>") || rest.starts_with("+<") {
        Some(at + 2)
    } else {
        None
    }
}

/// Pele's LINK_LABEL, /;[^\n\r]+/y: a semicolon and the rest of its line, which must not be
/// empty.
fn link_label(s: &str, at: usize) -> Option<usize> {
    let b = s.as_bytes();
    if b[at] != b';' {
        return None;
    }
    let mut end = at + 1;
    while end < b.len() && b[end] != b'\n' && b[end] != b'\r' {
        end += 1;
    }
    (end > at + 1).then_some(end)
}

/// Pele's STRATEGY, /build|buy|outsource|market/y, which a letter after it does not stop:
/// `builder` starts with one.
fn strategy(s: &str, at: usize) -> Option<usize> {
    ["build", "buy", "outsource", "market"]
        .iter()
        .find(|w| s[at..].starts_with(*w))
        .map(|w| at + w.len())
}

/// Pele's word: a keyword matched by the regular expression of its text, which holds no
/// character special to one, so that it matches the text itself, followed by anything.
const fn word(name: &'static str, text: &'static str) -> TokenType {
    token(name, Pattern::Literal(text))
}

/// Whether the character at byte i continues a name, as (?:[A-Za-z0-9_()&]|-(?!>)) takes it.
fn name_char(s: &str, i: usize) -> bool {
    let b = s.as_bytes();
    match b[i] {
        c if c.is_ascii_alphanumeric() || matches!(c, b'_' | b'(' | b')' | b'&') => true,
        b'-' => b.get(i + 1) != Some(&b'>'),
        _ => false,
    }
}

/// The end of the run of characters that continue a name from byte i.
fn name_run(s: &str, mut i: usize) -> usize {
    while i < s.len() && name_char(s, i) {
        i += 1;
    }
    i
}

/// Pele's NAME_WITH_SPACES,
/// /(?!title\s|accTitle|accDescr)[A-Za-z](?:[A-Za-z0-9_()&]|-(?!>))*(?:[ \t]+[A-Za-z(](?:[A-Za-z0-9_()&]|-(?!>))*)*/y:
/// words set apart by tabs and spaces, each word after the first starting with a letter or a
/// parenthesis. A run of blanks that no such word follows is given back, whole, as each word's
/// run can only end where the next character cannot continue it.
fn name(s: &str, at: usize) -> Option<usize> {
    let rest = &s[at..];
    if rest.starts_with("title") && is_space(unit(s, at + 5))
        || rest.starts_with("accTitle")
        || rest.starts_with("accDescr")
    {
        return None;
    }
    let b = s.as_bytes();
    if !is_letter(i32::from(b[at])) {
        return None;
    }
    let mut end = name_run(s, at + 1);
    loop {
        let mut j = end;
        while j < b.len() && (b[j] == b' ' || b[j] == b'\t') {
            j += 1;
        }
        if j == end || j == b.len() || !(is_letter(i32::from(b[j])) || b[j] == b'(') {
            return Some(end);
        }
        end = name_run(s, j + 1);
    }
}

/// Pele's ID, /[\w]([-\w]*\w)?/y: word characters and hyphens, from a word character to the
/// last word character of the run.
fn id(s: &str, at: usize) -> Option<usize> {
    let b = s.as_bytes();
    if !is_word(i32::from(b[at])) {
        return None;
    }
    let mut end = at + 1;
    let mut k = at + 1;
    while k < b.len() && (is_word(i32::from(b[k])) || b[k] == b'-') {
        if b[k] != b'-' {
            end = k + 1;
        }
        k += 1;
    }
    Some(end)
}

const LETTERS: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";

/// Pele's WARDLEY_TOKENS.
pub(crate) const WARDLEY_TOKENS: &[TokenType] = &[
    token("[", Pattern::Literal("[")),
    token(",", Pattern::Literal(",")),
    token("]", Pattern::Literal("]")),
    token("@", Pattern::Literal("@")),
    token("/", Pattern::Literal("/")),
    token("(", Pattern::Literal("(")),
    token(")", Pattern::Literal(")")),
    // An arrow and front matter also start with a dash, and Langium lets the longer win.
    TokenType {
        longer: &[T_ARROW, T_LINK_ARROW, T_YAML],
        ..token("-", Pattern::Literal("-"))
    },
    token("{", Pattern::Literal("{")),
    token("}", Pattern::Literal("}")),
    TokenType {
        first: Some("0123456789"),
        ..token("WARDLEY_NUMBER", Pattern::Scanner(number_scanner))
    },
    TokenType {
        first: Some("-"),
        ..token("ARROW", Pattern::Match(arrow))
    },
    TokenType {
        first: Some("+"),
        ..token("LINK_PORT", Pattern::Match(link_port))
    },
    TokenType {
        first: Some("->+"),
        ..token("LINK_ARROW", Pattern::Match(link_arrow))
    },
    TokenType {
        first: Some(";"),
        ..token("LINK_LABEL", Pattern::Match(link_label))
    },
    TokenType {
        first: Some("bom"),
        ..token("STRATEGY", Pattern::Match(strategy))
    },
    word("KW_WARDLEY", "wardley-beta"),
    word("KW_SIZE", "size"),
    word("KW_EVOLUTION", "evolution"),
    word("KW_ANCHOR", "anchor"),
    word("KW_COMPONENT", "component"),
    word("KW_LABEL", "label"),
    word("KW_INERTIA", "inertia"),
    word("KW_EVOLVE", "evolve"),
    word("KW_PIPELINE", "pipeline"),
    word("KW_NOTE", "note"),
    word("KW_ANNOTATIONS", "annotations"),
    word("KW_ANNOTATION", "annotation"),
    word("KW_ACCELERATOR", "accelerator"),
    word("KW_DEACCELERATOR", "deaccelerator"),
    TokenType {
        first: Some(LETTERS),
        ..token("NAME_WITH_SPACES", Pattern::Match(name))
    },
    TokenType {
        name: "WS",
        ..WHITESPACE
    },
    acc_descr_named("ACC_DESCR"),
    ACC_TITLE,
    TITLE,
    TokenType {
        first: Some("0123456789"),
        ..token("INT", Pattern::Match(int))
    },
    STRING,
    TokenType {
        first: Some("_0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz"),
        ..token("ID", Pattern::Match(id))
    },
    NEWLINE,
    WHITESPACE,
    YAML,
    DIRECTIVE,
    SINGLE_LINE_COMMENT,
];

/// WARDLEY_TOKENS made ready to tokenize with.
pub(crate) fn table() -> &'static Table {
    static TABLE: OnceLock<Table> = OnceLock::new();
    TABLE.get_or_init(|| Table::new(WARDLEY_TOKENS))
}

/// Pele's LabelAst.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct LabelAst {
    pub(crate) neg_x: bool,
    pub(crate) offset_x: f64,
    pub(crate) neg_y: bool,
    pub(crate) offset_y: f64,
}

/// Pele's StageAst. What Pele leaves undefined is none.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct StageAst {
    pub(crate) name: String,
    pub(crate) boundary: Option<f64>,
    pub(crate) second_name: Option<String>,
}

/// Pele's ComponentAst. The strategy is its decorator's.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ComponentAst {
    pub(crate) name: String,
    pub(crate) visibility: f64,
    pub(crate) evolution: f64,
    pub(crate) label: Option<LabelAst>,
    pub(crate) decorator: Option<String>,
    pub(crate) inertia: bool,
}

/// Pele's PipelineComponentAst.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PipelineComponentAst {
    pub(crate) name: String,
    pub(crate) evolution: f64,
    pub(crate) label: Option<LabelAst>,
}

/// Pele's LinkAst. What Pele leaves undefined is none.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct LinkAst {
    pub(crate) from: String,
    pub(crate) from_port: Option<String>,
    pub(crate) arrow: Option<String>,
    pub(crate) to: String,
    pub(crate) to_port: Option<String>,
    pub(crate) link_label: Option<String>,
}

/// An anchor of the tree.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct AnchorAst {
    pub(crate) name: String,
    pub(crate) visibility: f64,
    pub(crate) evolution: f64,
}

/// A note of the tree.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct NoteAst {
    pub(crate) text: String,
    pub(crate) visibility: f64,
    pub(crate) evolution: f64,
}

/// An accelerator or a deaccelerator of the tree, Pele's Placed.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PlacedAst {
    pub(crate) name: String,
    pub(crate) x: f64,
    pub(crate) y: f64,
}

/// A pipeline of the tree.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PipelineAst {
    pub(crate) parent: String,
    pub(crate) components: Vec<PipelineComponentAst>,
}

/// An evolve statement of the tree.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct EvolveAst {
    pub(crate) component: String,
    pub(crate) target: f64,
}

/// A numbered annotation of the tree.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct AnnotationAst {
    pub(crate) number: f64,
    pub(crate) x: f64,
    pub(crate) y: f64,
    pub(crate) text: String,
}

/// Pele's WardleyAst. What Pele leaves undefined is none. The size is its width and height,
/// and each place of the annotations box its x and y.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct WardleyAst {
    pub(crate) title: Option<String>,
    pub(crate) acc_title: Option<String>,
    pub(crate) acc_descr: Option<String>,
    pub(crate) size: Option<(f64, f64)>,
    pub(crate) evolution: Option<Vec<StageAst>>,
    pub(crate) anchors: Vec<AnchorAst>,
    pub(crate) components: Vec<ComponentAst>,
    pub(crate) links: Vec<LinkAst>,
    pub(crate) evolves: Vec<EvolveAst>,
    pub(crate) pipelines: Vec<PipelineAst>,
    pub(crate) notes: Vec<NoteAst>,
    pub(crate) annotations: Vec<(f64, f64)>,
    pub(crate) annotation: Vec<AnnotationAst>,
    pub(crate) accelerators: Vec<PlacedAst>,
    pub(crate) deaccelerators: Vec<PlacedAst>,
}

fn at(r: &Reader, kind: usize) -> bool {
    r.kind() == kind
}

/// Pele's isName.
fn is_name(r: &Reader) -> bool {
    at(r, T_STRING) || at(r, T_ID) || at(r, T_NAME)
}

/// Pele's name: a string, an id or a name with spaces.
fn read_name(r: &mut Reader) -> Result<String, Error> {
    if at(r, T_STRING) {
        return Ok(string_value(r.take()));
    }
    if at(r, T_ID) || at(r, T_NAME) {
        return Ok(r.take().to_string());
    }
    Err(r.fail("a name"))
}

/// Pele's number.
fn read_number(r: &mut Reader) -> Result<f64, Error> {
    Ok(number(r.expect(T_NUMBER)?))
}

/// Pele's int.
fn read_int(r: &mut Reader) -> Result<f64, Error> {
    Ok(parse_int(r.expect(T_INT)?, 10))
}

/// Pele's coordinate: a number, or else an integer.
fn coordinate(r: &mut Reader) -> Result<f64, Error> {
    if at(r, T_NUMBER) {
        return Ok(number(r.take()));
    }
    read_int(r)
}

/// Pele's endOfLine: the end of the text, or line breaks.
fn end_of_line(r: &mut Reader) -> Result<(), Error> {
    if at(r, END) {
        return Ok(());
    }
    if !at(r, T_NEWLINE) {
        return Err(r.fail("a line break or the end of input"));
    }
    while at(r, T_NEWLINE) {
        r.i += 1;
    }
    Ok(())
}

/// Pele's pair: two values read alike in brackets, set apart by a comma.
fn pair(r: &mut Reader, read: fn(&mut Reader) -> Result<f64, Error>) -> Result<(f64, f64), Error> {
    r.expect(T_LBRACKET)?;
    let first = read(r)?;
    r.expect(T_COMMA)?;
    let second = read(r)?;
    r.expect(T_RBRACKET)?;
    Ok((first, second))
}

/// Pele's label.
fn label(r: &mut Reader) -> Result<LabelAst, Error> {
    r.expect(T_LABEL)?;
    r.expect(T_LBRACKET)?;
    let neg_x = r.accept(T_MINUS);
    let offset_x = read_int(r)?;
    r.expect(T_COMMA)?;
    let neg_y = r.accept(T_MINUS);
    let offset_y = read_int(r)?;
    r.expect(T_RBRACKET)?;
    Ok(LabelAst {
        neg_x,
        offset_x,
        neg_y,
        offset_y,
    })
}

/// Pele's stage.
fn stage(r: &mut Reader) -> Result<StageAst, Error> {
    let mut out = StageAst {
        name: read_name(r)?,
        boundary: None,
        second_name: None,
    };
    if r.accept(T_AT) {
        out.boundary = Some(read_number(r)?);
    }
    if r.accept(T_SLASH) {
        out.second_name = Some(read_name(r)?);
    }
    Ok(out)
}

/// Parses a Wardley map's text into its tree, as Pele's parseWardley does.
pub(crate) fn parse_wardley(src: &str) -> Result<WardleyAst, Error> {
    let tokens = tokenize(src, table(), DiagramType::Wardley)?;
    let mut r = Reader::new(tokens, table(), DiagramType::Wardley);
    let r = &mut r;
    let mut ast = WardleyAst::default();

    while at(r, T_NEWLINE) {
        r.i += 1;
    }
    r.expect(T_WARDLEY)?;
    while !at(r, END) {
        match r.kind() {
            T_NEWLINE => r.i += 1,
            T_ACC_DESCR => {
                ast.acc_descr = Some(acc_descr_value(r.take()));
                end_of_line(r)?;
            }
            T_ACC_TITLE => {
                ast.acc_title = Some(acc_title_value(r.take()));
                end_of_line(r)?;
            }
            T_TITLE => {
                ast.title = Some(title_value(r.take()));
                end_of_line(r)?;
            }
            T_SIZE => {
                r.i += 1;
                let size = pair(r, read_int)?;
                end_of_line(r)?;
                ast.size = Some(size);
            }
            T_EVOLUTION => {
                r.i += 1;
                let mut stages = vec![stage(r)?];
                r.expect(T_ARROW)?;
                stages.push(stage(r)?);
                while r.accept(T_ARROW) {
                    stages.push(stage(r)?);
                }
                end_of_line(r)?;
                ast.evolution = Some(stages);
            }
            T_ANCHOR => {
                r.i += 1;
                let name = read_name(r)?;
                let (visibility, evolution) = pair(r, read_number)?;
                end_of_line(r)?;
                ast.anchors.push(AnchorAst {
                    name,
                    visibility,
                    evolution,
                });
            }
            T_COMPONENT => {
                r.i += 1;
                let name = read_name(r)?;
                let (visibility, evolution) = pair(r, read_number)?;
                let mut component = ComponentAst {
                    name,
                    visibility,
                    evolution,
                    label: None,
                    decorator: None,
                    inertia: false,
                };
                if at(r, T_LABEL) {
                    component.label = Some(label(r)?);
                }
                if at(r, T_LPAREN) && r.tokens.kinds[r.i + 1] == T_STRATEGY {
                    r.i += 1;
                    component.decorator = Some(r.take().to_string());
                    r.expect(T_RPAREN)?;
                }
                if r.accept(T_INERTIA) {
                    component.inertia = true;
                } else if r.accept(T_LPAREN) {
                    r.expect(T_INERTIA)?;
                    r.expect(T_RPAREN)?;
                    component.inertia = true;
                }
                end_of_line(r)?;
                ast.components.push(component);
            }
            T_EVOLVE => {
                r.i += 1;
                let component = read_name(r)?;
                let target = read_number(r)?;
                end_of_line(r)?;
                ast.evolves.push(EvolveAst { component, target });
            }
            T_PIPELINE => {
                r.i += 1;
                let parent = read_name(r)?;
                r.expect(T_LBRACE)?;
                r.expect(T_NEWLINE)?;
                while at(r, T_NEWLINE) {
                    r.i += 1;
                }
                let mut components = Vec::new();
                loop {
                    r.expect(T_COMPONENT)?;
                    let name = read_name(r)?;
                    r.expect(T_LBRACKET)?;
                    let evolution = read_number(r)?;
                    r.expect(T_RBRACKET)?;
                    let mut component = PipelineComponentAst {
                        name,
                        evolution,
                        label: None,
                    };
                    if at(r, T_LABEL) {
                        component.label = Some(label(r)?);
                    }
                    end_of_line(r)?;
                    components.push(component);
                    if !at(r, T_COMPONENT) {
                        break;
                    }
                }
                r.expect(T_RBRACE)?;
                end_of_line(r)?;
                ast.pipelines.push(PipelineAst { parent, components });
            }
            T_NOTE => {
                r.i += 1;
                let text = string_value(r.expect(T_STRING)?);
                let (visibility, evolution) = pair(r, read_number)?;
                end_of_line(r)?;
                ast.notes.push(NoteAst {
                    text,
                    visibility,
                    evolution,
                });
            }
            T_ANNOTATIONS => {
                r.i += 1;
                let place = pair(r, coordinate)?;
                end_of_line(r)?;
                ast.annotations.push(place);
            }
            T_ANNOTATION => {
                r.i += 1;
                let number = read_int(r)?;
                r.expect(T_COMMA)?;
                let (x, y) = pair(r, coordinate)?;
                let text = string_value(r.expect(T_STRING)?);
                end_of_line(r)?;
                ast.annotation.push(AnnotationAst { number, x, y, text });
            }
            T_ACCELERATOR | T_DEACCELERATOR => {
                let slow = r.kind() == T_DEACCELERATOR;
                r.i += 1;
                let name = read_name(r)?;
                let (x, y) = pair(r, read_number)?;
                end_of_line(r)?;
                let force = PlacedAst { name, x, y };
                if slow {
                    ast.deaccelerators.push(force);
                } else {
                    ast.accelerators.push(force);
                }
            }
            _ => {
                if !is_name(r) {
                    return Err(r.fail("a statement"));
                }
                let mut link = LinkAst {
                    from: read_name(r)?,
                    ..LinkAst::default()
                };
                if at(r, T_LINK_PORT) {
                    link.from_port = Some(r.take().to_string());
                }
                if at(r, T_LINK_ARROW) || at(r, T_ARROW) {
                    link.arrow = Some(r.take().to_string());
                }
                link.to = read_name(r)?;
                if at(r, T_LINK_PORT) {
                    link.to_port = Some(r.take().to_string());
                }
                if at(r, T_LINK_LABEL) {
                    link.link_label = Some(trim(&r.take()[1..]).to_string());
                }
                end_of_line(r)?;
                ast.links.push(link);
            }
        }
    }
    Ok(ast)
}
