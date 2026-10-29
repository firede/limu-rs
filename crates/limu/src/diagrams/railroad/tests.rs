//! Railroad diagrams read and laid out as Pele reads and draws them, checked against the
//! oracle's railroad and railroad-render fixtures.

use limu_fixtures::load;
use serde::Deserialize;

use super::abnf::{ABNF_TOKENS, AbnfPart, parse_abnf};
use super::common::{Grammar, Parsed};
use super::db::{Notation, RailroadDb, RailroadNode};
use super::ebnf::{EBNF_TOKENS, EbnfPart, EbnfPostfix, parse_ebnf};
use super::ir::{IR_TOKENS, IrExpression, parse_ir};
use super::peg::{PEG_TOKENS, PegPart, parse_peg};
use super::render::{
    ROLE_ARROWS, ROLE_COUNT, ROLE_END, ROLE_NAME, ROLE_NONTERMINAL, ROLE_SPECIAL,
    ROLE_SPECIAL_MARK, ROLE_START, ROLE_TERMINAL, ROLE_TITLE, ROLE_TRACK, layout,
};
use super::{abnf, ebnf, ir, parse, peg};
use crate::common::tokens::{Table, TokenType, tokenize};
use crate::detect::DiagramType;
use crate::errors::{Error, ErrorCode};
use crate::js::number::to_string as n;
use crate::layout::Options;
use crate::prepare::{ParseOptions, prepare};
use crate::scene::style::Paint;
use crate::scene::text::{Face, Label, Measurer};
use crate::scene::{Align, Item, Op, Outline, Painting, Path, Placed, Point, Scene};
use crate::testing::pelesvg::{cap_join, id};
use crate::testing::{Enc, Failures, PeleError, same, sorted};
use crate::text::Arial;
use crate::value::{Map, Value};

#[derive(Deserialize)]
struct Cases {
    cases: Vec<Case>,
}

#[derive(Deserialize)]
struct Case {
    src: String,
    direct: Direct,
    #[serde(rename = "type")]
    diagram: Option<String>,
    model: Option<Enc>,
    error: Option<PeleError>,
}

#[derive(Deserialize)]
struct Direct {
    notation: String,
    tokens: Option<Vec<(String, String)>>,
    ast: Option<Enc>,
    error: Option<PeleError>,
}

/// Checks that each kind, numbered by hand, is the index of its type in the table.
fn check_kinds(table: &[TokenType], len: usize, kinds: &[(usize, &str)]) {
    assert_eq!(table.len(), len, "the table's types");
    for &(kind, name) in kinds {
        assert_eq!(table[kind].name, name, "kind {kind}");
    }
}

#[test]
fn token_kinds() {
    use super::{abnf as a, ebnf as e, ir as i, peg as p};
    check_kinds(
        IR_TOKENS,
        24,
        &[
            (0, "railroad-beta"),
            (i::T_NONTERMINAL, "nonterminal"),
            (i::T_ZERO_OR_MORE, "zeroOrMore"),
            (i::T_ONE_OR_MORE, "oneOrMore"),
            (i::T_SEQUENCE, "sequence"),
            (i::T_OPTIONAL, "optional"),
            (i::T_TERMINAL, "terminal"),
            (i::T_SPECIAL, "special"),
            (i::T_CHOICE, "choice"),
            (i::T_EQUALS, "="),
            (i::T_SEMICOLON, ";"),
            (i::T_OPEN, "("),
            (i::T_COMMA, ","),
            (i::T_CLOSE, ")"),
            (i::T_TITLE, "TITLE"),
            (i::T_TITLE + 1, "ACC_TITLE"),
            (i::T_TITLE + 2, "ACC_DESCR"),
            (i::T_ID, "RR_ID"),
            (i::T_STRING, "RR_STRING"),
        ],
    );
    check_kinds(
        EBNF_TOKENS,
        28,
        &[
            (0, "railroad-ebnf-beta"),
            (e::T_DEFINES, "::="),
            (e::T_EQUALS, "="),
            (e::T_SEMICOLON, ";"),
            (e::T_BAR, "|"),
            (e::T_COMMA, ","),
            (e::T_GROUP, "("),
            (e::T_GROUP + 1, ")"),
            (e::T_OPTION, "["),
            (e::T_OPTION + 1, "]"),
            (e::T_REPEAT, "{"),
            (e::T_REPEAT + 1, "}"),
            (e::T_QUESTION, "?"),
            (e::T_STAR, "*"),
            (e::T_PLUS, "+"),
            (e::T_MINUS, "-"),
            (e::T_TITLE, "TITLE"),
            (e::T_TITLE + 1, "ACC_TITLE"),
            (e::T_TITLE + 2, "ACC_DESCR"),
            (e::T_ID, "EBNF_ID"),
            (e::T_STRING, "EBNF_STRING"),
            (e::T_SPECIAL, "EBNF_SPECIAL_SEQUENCE"),
            (e::T_YAML, "EBNF_YAML"),
            (e::T_ISO_COMMENT, "EBNF_ISO_COMMENT"),
        ],
    );
    check_kinds(
        ABNF_TOKENS,
        21,
        &[
            (0, "railroad-abnf-beta"),
            (a::T_EQUALS, "="),
            (a::T_SEMICOLON, ";"),
            (a::T_SLASH, "/"),
            (a::T_GROUP, "("),
            (a::T_GROUP + 1, ")"),
            (a::T_OPTION, "["),
            (a::T_OPTION + 1, "]"),
            (a::T_TITLE, "TITLE"),
            (a::T_TITLE + 1, "ACC_TITLE"),
            (a::T_TITLE + 2, "ACC_DESCR"),
            (a::T_NAME, "ABNF_RULENAME"),
            (a::T_STRING, "ABNF_STRING"),
            (a::T_NUMBER, "ABNF_NUMVAL"),
            (a::T_REPEAT, "ABNF_REPEAT"),
            (a::T_EXACT, "ABNF_EXACT_REPEAT"),
            (a::T_COMMENT, "ABNF_COMMENT"),
        ],
    );
    check_kinds(
        PEG_TOKENS,
        22,
        &[
            (0, "railroad-peg-beta"),
            (p::T_ARROW, "<-"),
            (p::T_SEMICOLON, ";"),
            (p::T_SLASH, "/"),
            (p::T_AND, "&"),
            (p::T_NOT, "!"),
            (p::T_QUESTION, "?"),
            (p::T_STAR, "*"),
            (p::T_PLUS, "+"),
            (p::T_GROUP, "("),
            (p::T_GROUP_END, ")"),
            (p::T_DOT, "."),
            (p::T_TITLE, "TITLE"),
            (p::T_TITLE + 1, "ACC_TITLE"),
            (p::T_TITLE + 2, "ACC_DESCR"),
            (p::T_ID, "PEG_ID"),
            (p::T_STRING, "PEG_STRING"),
        ],
    );
}

fn string(v: &str) -> Value {
    Value::String(v.to_string())
}

fn object(entries: Vec<(&str, Value)>) -> Value {
    Value::Object(
        entries
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect(),
    )
}

fn typed(t: &str, mut entries: Vec<(&str, Value)>) -> Value {
    entries.insert(0, ("$type", string(t)));
    object(entries)
}

/// A node of the model as the oracle writes it.
fn dump_node(db: &RailroadDb, i: usize) -> Value {
    let kids = |tag: &str, kids: &[usize]| {
        let mut out = vec![string(tag)];
        out.extend(kids.iter().map(|&k| dump_node(db, k)));
        Value::Array(out)
    };
    match &db.nodes[i] {
        RailroadNode::Terminal(t) => Value::Array(vec![string("t"), string(t)]),
        RailroadNode::Nonterminal(t) => Value::Array(vec![string("n"), string(t)]),
        RailroadNode::Special(t) => Value::Array(vec![string("s"), string(t)]),
        RailroadNode::Sequence(k) => kids("seq", k),
        RailroadNode::Choice(k) => kids("alt", k),
        RailroadNode::Optional(e) => Value::Array(vec![string("opt"), dump_node(db, *e)]),
        RailroadNode::Repetition { element, min, max } => Value::Array(vec![
            string("rep"),
            Value::Number(*min),
            Value::Number(*max),
            dump_node(db, *element),
        ]),
    }
}

fn dump_model(db: &RailroadDb) -> Value {
    let mut out = Map::new();
    out.insert("notation", string(db.notation.name()));
    for (k, v) in [
        ("title", &db.title),
        ("accTitle", &db.acc_title),
        ("accDescr", &db.acc_descr),
    ] {
        if let Some(v) = v.as_deref().filter(|v| !v.is_empty()) {
            out.insert(k, string(v));
        }
    }
    let rules = db
        .rules
        .iter()
        .map(|r| Value::Array(vec![string(&r.name), dump_node(db, r.definition)]))
        .collect();
    out.insert("rules", Value::Array(rules));
    Value::Object(out)
}

/// A syntax tree's statements and rules as the oracle writes them, each part by part.
fn dump_grammar<P>(
    g: &Grammar<P>,
    kind: &str,
    rule: &str,
    part: impl Fn(&Grammar<P>, usize) -> Value,
) -> Value {
    let rules = g
        .rules
        .iter()
        .map(|r| {
            typed(
                rule,
                vec![
                    ("name", string(&r.name)),
                    ("definition", part(g, r.definition)),
                ],
            )
        })
        .collect();
    let mut entries = vec![("rules", Value::Array(rules))];
    let Parsed {
        title,
        acc_title,
        acc_descr,
    } = &g.parsed;
    for (k, v) in [
        ("title", title),
        ("accTitle", acc_title),
        ("accDescr", acc_descr),
    ] {
        if let Some(v) = v {
            entries.push((k, string(v)));
        }
    }
    typed(kind, entries)
}

fn list(g: &[usize], part: impl Fn(usize) -> Value) -> Value {
    Value::Array(g.iter().map(|&k| part(k)).collect())
}

fn dump_ir(g: &Grammar<IrExpression>, i: usize) -> Value {
    let part = |k| dump_ir(g, k);
    let element =
        |t: &str, e: &Option<usize>| typed(t, vec![("element", e.map_or(Value::Null, part))]);
    match &g.parts[i] {
        IrExpression::Terminal(v) => typed("RailroadTerminalExpr", vec![("value", string(v))]),
        IrExpression::NonTerminal(v) => typed("RailroadNonTerminalExpr", vec![("name", string(v))]),
        IrExpression::Special(v) => typed("RailroadSpecialExpr", vec![("text", string(v))]),
        IrExpression::Sequence(k) => {
            typed("RailroadSequenceExpr", vec![("elements", list(k, part))])
        }
        IrExpression::Choice(k) => {
            typed("RailroadChoiceExpr", vec![("alternatives", list(k, part))])
        }
        IrExpression::Optional(e) => element("RailroadOptionalExpr", e),
        IrExpression::OneOrMore(e) => element("RailroadOneOrMoreExpr", e),
        IrExpression::ZeroOrMore(e) => element("RailroadZeroOrMoreExpr", e),
    }
}

fn dump_ebnf(g: &Grammar<EbnfPart>, i: usize) -> Value {
    let part = |k| dump_ebnf(g, k);
    match &g.parts[i] {
        EbnfPart::Choice(k) => typed("EbnfChoice", vec![("alternatives", list(k, part))]),
        EbnfPart::Sequence(k) => typed("EbnfSequence", vec![("elements", list(k, part))]),
        EbnfPart::Term { base, postfixes } => {
            let postfixes = postfixes
                .iter()
                .map(|f| match f {
                    EbnfPostfix::Exception(e) => {
                        typed("EbnfExceptionPostfix", vec![("except", part(*e))])
                    }
                    EbnfPostfix::Optional(o) => {
                        typed("EbnfOptionalPostfix", vec![("operator", string(o))])
                    }
                    EbnfPostfix::ZeroOrMore(o) => {
                        typed("EbnfZeroOrMorePostfix", vec![("operator", string(o))])
                    }
                    EbnfPostfix::OneOrMore(o) => {
                        typed("EbnfOneOrMorePostfix", vec![("operator", string(o))])
                    }
                })
                .collect();
            typed(
                "EbnfTerm",
                vec![
                    ("base", part(*base)),
                    ("postfixes", Value::Array(postfixes)),
                ],
            )
        }
        EbnfPart::Terminal(v) => typed("EbnfTerminal", vec![("value", string(v))]),
        EbnfPart::NonTerminal(v) => typed("EbnfNonTerminal", vec![("name", string(v))]),
        EbnfPart::Special(v) => typed("EbnfSpecial", vec![("text", string(v))]),
        EbnfPart::Group(e) => typed("EbnfGroup", vec![("element", part(*e))]),
        EbnfPart::Optional(e) => typed("EbnfOptional", vec![("element", part(*e))]),
        EbnfPart::Repetition(e) => typed("EbnfRepetition", vec![("element", part(*e))]),
    }
}

fn dump_abnf(g: &Grammar<AbnfPart>, i: usize) -> Value {
    let part = |k| dump_abnf(g, k);
    match &g.parts[i] {
        AbnfPart::Alternation(k) => typed("AbnfAlternation", vec![("alternatives", list(k, part))]),
        AbnfPart::Concatenation(k) => typed("AbnfConcatenation", vec![("elements", list(k, part))]),
        AbnfPart::Element { repeat, primary } => {
            let mut entries = vec![("primary", part(*primary))];
            if let Some(r) = repeat {
                entries.push(("repeat", string(r)));
            }
            typed("AbnfElement", entries)
        }
        AbnfPart::StringLiteral(v) => typed("AbnfStringLiteral", vec![("value", string(v))]),
        AbnfPart::NumVal(v) => typed("AbnfNumVal", vec![("value", string(v))]),
        AbnfPart::RuleName(v) => typed("AbnfRuleName", vec![("name", string(v))]),
        AbnfPart::Group(e) => typed("AbnfGroup", vec![("element", part(*e))]),
        AbnfPart::OptionalGroup(e) => typed("AbnfOptionalGroup", vec![("element", part(*e))]),
    }
}

fn dump_peg(g: &Grammar<PegPart>, i: usize) -> Value {
    let part = |k| dump_peg(g, k);
    match &g.parts[i] {
        PegPart::Choice(k) => typed("PegOrderedChoice", vec![("alternatives", list(k, part))]),
        PegPart::Sequence(k) => typed("PegSequence", vec![("elements", list(k, part))]),
        PegPart::Prefix { operator, suffix } => {
            let mut entries = vec![("suffix", part(*suffix))];
            if let Some(o) = operator {
                entries.push(("operator", string(o)));
            }
            typed("PegPrefix", entries)
        }
        PegPart::Suffix { primary, operator } => {
            let mut entries = vec![("primary", part(*primary))];
            if let Some(o) = operator {
                entries.push(("operator", string(o)));
            }
            typed("PegSuffix", entries)
        }
        PegPart::Literal(v) => typed("PegLiteral", vec![("value", string(v))]),
        PegPart::Identifier(v) => typed("PegIdentifier", vec![("name", string(v))]),
        PegPart::Any(v) => typed("PegAny", vec![("dot", string(v))]),
        PegPart::Group(e) => typed("PegGroup", vec![("element", part(*e))]),
    }
}

/// Parses a text with the parser of a notation, as Pele's does, and gives its table and its
/// tree as the oracle writes it.
fn direct(src: &str, notation: &str) -> (&'static Table, Result<Value, Error>) {
    match notation {
        "ebnf" => (
            ebnf::table(),
            parse_ebnf(src).map(|g| dump_grammar(&g, "RailroadEbnf", "EbnfRule", dump_ebnf)),
        ),
        "abnf" => (
            abnf::table(),
            parse_abnf(src).map(|g| dump_grammar(&g, "RailroadAbnf", "AbnfRule", dump_abnf)),
        ),
        "peg" => (
            peg::table(),
            parse_peg(src).map(|g| dump_grammar(&g, "RailroadPeg", "PegRule", dump_peg)),
        ),
        _ => (
            ir::table(),
            parse_ir(src).map(|g| dump_grammar(&g, "Railroad", "RailroadRule", dump_ir)),
        ),
    }
}

fn dump_tokens(src: &str, table: &Table) -> Option<Vec<(String, String)>> {
    let t = tokenize(src, table, DiagramType::Railroad).ok()?;
    let n = t.kinds.len() - 1;
    Some(
        (0..n)
            .map(|i| {
                let name = table.types[t.kinds[i]].name.to_string();
                (name, src[t.starts[i]..t.ends[i]].to_string())
            })
            .collect(),
    )
}

/// Runs a test on a stack that takes the deepest cases, whose syntax trees nest some
/// thousands deep, as the oracle writes them and as the test reads and writes them again.
fn on_a_deep_stack(test: fn()) {
    std::thread::Builder::new()
        .stack_size(1 << 30)
        .spawn(test)
        .expect("a thread")
        .join()
        .unwrap_or_else(|e| std::panic::resume_unwind(e));
}

#[test]
fn parse_matches_pele() {
    on_a_deep_stack(parses_match_pele);
}

fn parses_match_pele() {
    let f: Cases = load("railroad");
    assert!(f.cases.len() >= 8000, "only {} cases", f.cases.len());
    let mut failures = Failures::new();
    let mut models = [0; 4];
    for c in &f.cases {
        let (table, tree) = direct(&c.src, &c.direct.notation);
        let tokens = dump_tokens(&c.src, table);
        failures.check(tokens == c.direct.tokens, || {
            format!(
                "tokens of {:?}: {tokens:?}, Pele {:?}",
                c.src, c.direct.tokens
            )
        });
        let ok = match (&tree, &c.direct.error, &c.direct.ast) {
            (Err(e), Some(want), _) => want.is(e),
            (Ok(t), None, Some(want)) => same(&sorted(t), &sorted(&want.0)),
            // The oracle leaves out the trees of the deepest cases.
            (Ok(_), None, None) => true,
            _ => false,
        };
        failures.check(ok, || {
            format!(
                "parse {} ({:?}): {tree:?}, Pele {:?} {:?}",
                c.direct.notation, c.src, c.direct.ast, c.direct.error
            )
        });

        let p = prepare(&c.src, &ParseOptions::default());
        let mut got = None;
        let ok = match (&p, &c.error) {
            (Err(e), Some(want)) => want.is(e),
            (Err(e), None) => e.code == ErrorCode::UnsupportedDiagram && c.diagram.is_none(),
            (Ok(p), _) if c.diagram.as_deref() != Some(p.found.diagram.name()) => false,
            (Ok(p), want) if p.found.diagram == DiagramType::Railroad => {
                let m = p.parse_with(false, |s| parse(s, p.title.as_deref()));
                let ok = match (&m, want, &c.model) {
                    (Err(e), Some(want), _) => want.is(e),
                    (Ok(m), None, Some(model)) => {
                        models[m.notation as usize] += 1;
                        same(&sorted(&dump_model(m)), &sorted(&model.0))
                    }
                    _ => false,
                };
                got = Some(m);
                ok
            }
            (Ok(_), _) => true,
        };
        failures.check(ok, || {
            format!(
                "{:?}: {:?}, Pele model {:?}, error {:?}",
                c.src,
                got.as_ref().map(|m| m.as_ref().map(dump_model)),
                c.model,
                c.error
            )
        });
    }
    failures.done("railroad", 3 * 8000);
    // Each notation's models must be common, or the comparison says little of it.
    for (k, notation) in [
        Notation::Railroad,
        Notation::Ebnf,
        Notation::Abnf,
        Notation::Peg,
    ]
    .iter()
    .enumerate()
    {
        assert!(
            models[k] >= 400,
            "only {} models in {}",
            models[k],
            notation.name()
        );
    }
}

#[test]
fn get_rule_gives_the_last_definition() {
    let m = parse("railroad-ebnf-beta\nr = a ;\nr = b ;\n", None).expect("a diagram");
    let r = m.get_rule("r").expect("a rule r");
    assert_eq!(m.nodes[r.definition], RailroadNode::Nonterminal("b".into()));
    assert_eq!(m.rules.len(), 2);
    assert!(m.get_rule("s").is_none(), "a rule never defined");
}

#[derive(Deserialize)]
struct Renders {
    cases: Vec<Render>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Render {
    src: String,
    max_width: Option<f64>,
    font_size: Option<f64>,
    #[serde(flatten)]
    drawing: limu_fixtures::Drawing,
}

/// A path's steps, every number in full.
fn path_line(p: &Path) -> String {
    let mut out = Vec::new();
    let mut at = 0;
    for &op in p.ops() {
        let mut s = match op {
            Op::MoveTo => "M",
            Op::LineTo => "L",
            Op::QuadTo => "Q",
            Op::CubeTo => "C",
            Op::Close => "Z",
        }
        .to_string();
        for v in &p.args()[at..at + op.args()] {
            s.push(' ');
            s.push_str(&n(*v));
        }
        at += op.args();
        out.push(s);
    }
    out.join(" ")
}

/// A number of the fixture, as JavaScript's String wrote it.
fn number(s: &str) -> f64 {
    s.parse().expect("a number")
}

/// A path the oracle writes, M, L, A and Z with every number in full, drawn as the scene
/// draws it.
fn pele_path(d: &str) -> Path {
    let mut p = Path::new();
    let f: Vec<&str> = d.split_whitespace().collect();
    let num = |i: usize| number(f[i]);
    let mut i = 0;
    while i < f.len() {
        match f[i] {
            "M" => {
                p.move_to(num(i + 1), num(i + 2));
                i += 3;
            }
            "L" => {
                p.line_to(num(i + 1), num(i + 2));
                i += 3;
            }
            "A" => {
                p.arc_to(
                    num(i + 1),
                    num(i + 2),
                    num(i + 3),
                    f[i + 4] == "1",
                    f[i + 5] == "1",
                    num(i + 6),
                    num(i + 7),
                );
                i += 8;
            }
            "Z" => {
                p.close();
                i += 1;
            }
            c => panic!("unknown command {c}"),
        }
    }
    p
}

/// A line of the fixture with the paths and circles Pele draws drawn again as the scene
/// draws them.
fn pele_line(line: &str) -> String {
    let fields: Vec<&str> = line.split_whitespace().collect();
    match fields[0] {
        "track" | "arrows" => {
            let bar = line.find(" | ").expect("a bar");
            format!(
                "{} | {}",
                &line[..bar],
                path_line(&pele_path(&line[bar + 3..]))
            )
        }
        "start" | "end" => {
            // start cx,cy r fill
            let (x, y) = fields[1].split_once(',').expect("a point");
            let (x, y, r) = (number(x), number(y), number(fields[2]));
            let mut p = Path::new();
            p.ellipse(x, y, r, r);
            format!("{} {} | {}", fields[0], fields[3], path_line(&p))
        }
        _ => line.to_string(),
    }
}

/// Text as Pele's esc writes it, without the control characters XML does not allow.
fn written(s: &str) -> String {
    s.chars()
        .filter(|&c| {
            !(c < ' ' && !matches!(c, '\t' | '\n' | '\r')
                || matches!(c, '\x7f' | '\u{fffe}' | '\u{ffff}'))
        })
        .collect()
}

/// A paint as Pele's SVG names it.
fn paint_name(p: Option<Paint>) -> &'static str {
    match p {
        Some(p) if p == Paint::FG => "var(--_fg)",
        Some(p) if p == Paint::MUTED => "var(--_m)",
        Some(p) if p == Paint::LINE => "var(--_l)",
        Some(p) if p == Paint::BG => "var(--_bg)",
        Some(p) if p == Paint::SURFACE => "var(--_s)",
        Some(p) if p == Paint::BORDER => "var(--_b)",
        _ => "?",
    }
}

/// A face as the oracle names what Pele writes of it.
fn face_name(f: Face) -> &'static str {
    if f == Face::MONO {
        "mono"
    } else if f == Face::ITALIC {
        "italic"
    } else if f == Face::BOLD {
        "var(--_hw)"
    } else {
        "-"
    }
}

/// The text of a one-line label, and the face of its first span.
fn label_text(l: Option<&Label>) -> (String, Face) {
    let Some(line) = l.and_then(|l| l.lines.first()).filter(|l| !l.is_empty()) else {
        return (String::new(), Face::REGULAR);
    };
    let s: String = line.iter().map(|s| s.text.as_str()).collect();
    (written(&s), line[0].face)
}

/// The path of an item's first part.
fn first_path(it: &Item) -> String {
    match &it.parts[0].outline {
        Outline::Path(p) => path_line(p),
        Outline::Box { .. } => "a box".into(),
    }
}

/// An item's placed label.
fn placed(it: &Item) -> &Placed {
    it.label.as_ref().expect("a label")
}

/// A scene written as the oracle reads Pele's SVG.
fn scene_lines(sc: &Scene) -> Vec<String> {
    let pt = |x: f64, y: f64| format!("{},{}", n(x), n(y));
    let mut lines = vec![format!("size {}", pt(sc.width, sc.height))];
    if let Some(t) = &sc.title {
        lines.push(format!("acc-title {}", id(t)));
    }
    if let Some(d) = &sc.desc {
        lines.push(format!("acc-desc {}", id(d)));
    }
    // What a stroke is drawn with: its paint, width, dashes, and cap and join.
    let pen = |it: &Item| {
        let s = &it.style;
        let width = s.stroke_width.map_or("-".to_string(), n);
        let dash = s.dash.as_ref().map_or("-".to_string(), |d| {
            d.iter().map(|&v| n(v)).collect::<Vec<_>>().join(" ")
        });
        format!("{} {width} {dash} {}", paint_name(s.stroke), cap_join(it))
    };
    // The middle of a box's text and what it writes, as the oracle reads Pele's.
    let leaf_text = |it: &Item| {
        let p = placed(it);
        let at = format!("t {}", pt(p.at.x, p.at.y));
        let (s, face) = label_text(Some(&p.label));
        if s.is_empty() {
            return at;
        }
        format!("{at} {} {} | {s}", n(p.label.size), face_name(face))
    };
    let mut i = 0;
    while i < sc.items.len() {
        let it = &sc.items[i];
        let role = it.role.as_str();
        let item_id = it.id.as_deref().unwrap_or("");
        match role {
            ROLE_TITLE => {
                let p = placed(it);
                let l = &p.label;
                let y = p.at.y - l.height / 2.0 + l.line_height / 2.0 + l.size * 0.35;
                let content: Vec<String> = l
                    .lines
                    .iter()
                    .map(|line| {
                        if line.is_empty() {
                            return " ".to_string();
                        }
                        written(&line.iter().map(|s| s.text.as_str()).collect::<String>())
                    })
                    .collect();
                lines.push(format!(
                    "title t {} | {}",
                    pt(p.at.x, y),
                    content.join(" | ")
                ));
            }
            ROLE_NAME => {
                let p = placed(it);
                let (s, face) = label_text(Some(&p.label));
                let align = match p.align {
                    Align::Middle => "middle",
                    Align::Start => "start",
                    Align::End => "end",
                };
                lines.push(format!("rule {}", id(item_id)));
                lines.push(format!(
                    "name t {} {} {} {align} | {s}",
                    pt(p.at.x, p.at.y),
                    n(p.label.size),
                    face_name(face)
                ));
            }
            ROLE_TRACK => {
                let fill = if it.parts[0].paint == Painting::StrokeOnly {
                    "none"
                } else {
                    "?"
                };
                lines.push(format!("track {fill} {} | {}", pen(it), first_path(it)));
            }
            ROLE_ARROWS => {
                let stroke = if it.parts[0].paint == Painting::FillOnly {
                    "-"
                } else {
                    "?"
                };
                lines.push(format!(
                    "arrows {} {stroke} | {}",
                    paint_name(it.style.fill),
                    first_path(it)
                ));
            }
            ROLE_START | ROLE_END => {
                let mut name = if role == ROLE_END { "end" } else { "start" }.to_string();
                if it.parts[0].paint != Painting::FillOnly {
                    name.push_str(" stroked");
                }
                lines.push(format!(
                    "{name} {} | {}",
                    paint_name(it.style.fill),
                    first_path(it)
                ));
            }
            ROLE_COUNT => {
                let p = placed(it);
                let (s, _) = label_text(Some(&p.label));
                lines.push(format!(
                    "count t {} {} {} | {s}",
                    pt(p.at.x, p.at.y),
                    n(p.label.size),
                    paint_name(it.style.text)
                ));
            }
            ROLE_TERMINAL | ROLE_NONTERMINAL | ROLE_SPECIAL => {
                let mut kind = role.trim_start_matches("railroad.").to_string();
                if role == ROLE_NONTERMINAL {
                    kind = format!("{kind} {}", id(item_id));
                }
                let Outline::Box { rect: r, rounded } = it.parts[0].outline else {
                    panic!("a box");
                };
                let radius = if rounded {
                    it.style.radius.map_or("theme".to_string(), n)
                } else {
                    "-".to_string()
                };
                let head = format!(
                    "{kind} {},{} {radius} {} {}",
                    pt(r.x, r.y),
                    pt(r.w, r.h),
                    paint_name(it.style.fill),
                    pen(it)
                );
                if role != ROLE_SPECIAL {
                    lines.push(format!("{head} | {}", leaf_text(it)));
                    i += 1;
                    continue;
                }
                // The marks are the two items after the box: the first starts where the whole
                // text starts, and the last ends where it ends.
                let p = placed(it);
                let (mid, face) = label_text(Some(&p.label));
                let mut marks = "marks missing".to_string();
                if i + 2 < sc.items.len()
                    && sc.items[i + 1].role == ROLE_SPECIAL_MARK
                    && sc.items[i + 2].role == ROLE_SPECIAL_MARK
                {
                    let (a, b) = (placed(&sc.items[i + 1]), placed(&sc.items[i + 2]));
                    let (sa, fa) = label_text(Some(&a.label));
                    let (sb, _) = label_text(Some(&b.label));
                    let whole = format!("{sa}{mid}{sb}");
                    let half = Arial.width(&whole, a.label.size, Face::ITALIC) / 2.0;
                    marks = "marks misplaced".to_string();
                    if a.at == Point::new(p.at.x - half, p.at.y)
                        && a.align == Align::Start
                        && b.at == Point::new(p.at.x + half, p.at.y)
                        && b.align == Align::End
                        && sc.items[i + 1].style.text == Some(Paint::MUTED)
                        && sc.items[i + 2].style.text == Some(Paint::MUTED)
                        && (mid.is_empty() || face == fa)
                    {
                        marks = format!("{} {} | {sa}|{mid}|{sb}", n(a.label.size), face_name(fa));
                    }
                    i += 2;
                }
                lines.push(format!("{head} | t {} {marks}", pt(p.at.x, p.at.y)));
            }
            _ => lines.push(format!("unknown role {role}")),
        }
        i += 1;
    }
    lines
}

/// What Pele's render draws of railroad diagrams, read back out of its SVG as one line per
/// thing drawn, in the order drawn, with its numbers written out in full and moved to where
/// the scene has them. Pele draws tracks with arcs and the dots at their ends as circles. The
/// test draws them again with arc_to and ellipse from Pele's numbers, and compares those
/// paths with the items'.
#[test]
fn renders_match_pele() {
    let (f, drawings): (Renders, _) = limu_fixtures::load_drawings("railroad-render");
    assert!(f.cases.len() >= 4000, "only {} cases", f.cases.len());
    let mut failures = Failures::new();
    for (k, c) in f.cases.iter().enumerate() {
        let p = prepare(&c.src, &ParseOptions::default()).expect("a railroad diagram");
        let m = p
            .parse_with(false, |s| parse(s, p.title.as_deref()))
            .expect("a railroad diagram that parses");
        let o = Options {
            max_width: c.max_width.filter(|&w| w > 0.0).unwrap_or(f64::INFINITY),
            font_size: c.font_size.filter(|&s| s > 0.0).unwrap_or(16.0),
            ..Options::default()
        };
        let got = scene_lines(&layout(&m, &p.config, &o));
        let diff = drawings.diff(k, &c.drawing, &got, Some(&pele_line));
        failures.check(diff.is_none(), || {
            format!(
                "{:?}, max {:?}, size {:?}: {}",
                c.src,
                c.max_width,
                c.font_size,
                diff.unwrap()
            )
        });
    }
    failures.done("renders", 4000);
}

/// The worst cases of Pele's tests/railroad/stress.test.ts: text that would make a tokenizer
/// rescan, or a parser or a layout recurse, as deep as memory allows. Each is tokenized,
/// parsed or laid out on a thread's usual stack, or fails with a diagram's error.
#[test]
fn worst_cases() {
    const N: usize = 50000;
    let r = |s: &str, n: usize| s.repeat(n);
    let numbered = |n: usize, part: fn(usize) -> String| (0..n).map(part).collect::<String>();
    let (irk, ebnfk, abnfk, pegk) = (
        "railroad-beta\n",
        "railroad-ebnf-beta\n",
        "railroad-abnf-beta\n",
        "railroad-peg-beta\n",
    );
    enum Run {
        Lex(&'static Table),
        Parse,
        Render,
    }
    use Run::{Lex, Parse, Render};
    let (it, et, at, pt) = (ir::table(), ebnf::table(), abnf::table(), peg::table());
    let cases: Vec<(&str, Run, String)> = vec![
        (
            "special sequence opener before a long run with no closer",
            Lex(et),
            format!("{ebnfk}r = ?{}", r(" a", N / 2)),
        ),
        (
            "question marks separated by blanks",
            Lex(et),
            format!("{ebnfk}r = {}", r("?  ", N / 3)),
        ),
        (
            "question marks each followed by a long run and a semicolon",
            Lex(et),
            format!("{ebnfk}{}", r(&format!("? {};", r("a ", 50)), N / 103)),
        ),
        (
            "unclosed ISO comment openers",
            Lex(et),
            format!("{ebnfk}r = {}", r("(* a ", N / 5)),
        ),
        (
            "ISO comment openers that never get a full closer",
            Lex(et),
            format!("{ebnfk}r = {}", r("(*)", N / 3)),
        ),
        (
            "unclosed block comment openers",
            Lex(it),
            format!("{irk}{}", r("/* a ", N / 5)),
        ),
        (
            "block comment open and close, many times",
            Lex(et),
            format!("{ebnfk}{}", r("/**/(**)", N / 8)),
        ),
        (
            "braced descriptions that are never closed",
            Lex(et),
            format!("{ebnfk}{}", r("accDescr { ", N / 11)),
        ),
        (
            "descriptions with long blank runs and no brace",
            Lex(et),
            format!("{ebnfk}{}", r(&format!("accDescr{}", r(" ", 92)), N / 100)),
        ),
        (
            "one-line descriptions with no comment marker",
            Lex(it),
            format!(
                "{irk}{}",
                r(&format!("accDescr: {}\n", r("% ", 45)), N / 101)
            ),
        ),
        (
            "accTitle with no colon, many times",
            Lex(pt),
            format!("{pegk}{}", r(&format!("accTitle{}", r(" ", 42)), N / 50)),
        ),
        (
            "one long title with single percent signs",
            Lex(it),
            format!("{irk}title {}", r("a % ", N / 4)),
        ),
        (
            "title words on one line",
            Lex(at),
            format!("{abnfk}{}", r("title ", N / 6)),
        ),
        (
            "titles with no space after them",
            Lex(it),
            format!("{irk}{}", r("titlea", N / 6)),
        ),
        (
            "unclosed double quote",
            Lex(it),
            format!("{irk}r = terminal(\"{}", r("a\\\"", N / 3)),
        ),
        (
            "quotes that open and never close, with backslashes",
            Lex(pt),
            format!("{pegk}R <- '{}", r("\\\\", N / 2)),
        ),
        (
            "unclosed ABNF string",
            Lex(at),
            format!("{abnfk}r = \"{}", r("a ", N / 2)),
        ),
        (
            "front matter fences that do not close",
            Lex(et),
            format!("{ebnfk}r = a {}", r("---\n", N / 4)),
        ),
        (
            "directive openers that do not close",
            Lex(it),
            format!("{irk}{}", r("%%{ a ", N / 6)),
        ),
        ("dashes", Lex(et), format!("{ebnfk}r = a {}", r("-", N))),
        (
            "digits with no star",
            Lex(at),
            format!("{abnfk}r = {}a ;", r("1", N)),
        ),
        (
            "digit runs each with a star",
            Lex(at),
            format!("{abnfk}r = {}", r("12*34 ", N / 6)),
        ),
        (
            "numeric values with long tails",
            Lex(at),
            format!("{abnfk}r = %x{} ;", r("41.", N / 3)),
        ),
        (
            "semicolon comments",
            Lex(at),
            format!("{abnfk}{}", r("; a comment\n", N / 12)),
        ),
        (
            "hash comments",
            Lex(pt),
            format!("{pegk}{}", r("# a comment\n", N / 12)),
        ),
        (
            "keywords glued into one name",
            Lex(it),
            format!("{irk}{}", r("terminal", N / 8)),
        ),
        (
            "one 50,000 character name",
            Render,
            format!("{ebnfk}{} = {} ;", r("a", N), r("b", N)),
        ),
        (
            "one 50,000 character terminal",
            Render,
            format!("{ebnfk}r = \"{}\" ;", r("word ", N / 5)),
        ),
        (
            "terminal of entity codes and markup",
            Render,
            format!("{ebnfk}r = \"{}\" ;", r("#35;&amp;<br>*_", N / 15)),
        ),
        (
            "terminal of escapes",
            Render,
            format!("{pegk}R <- \"{}\" ;", r("\\n\\t\\\\", N / 6)),
        ),
        (
            "25,000 nested groups",
            Render,
            format!("{ebnfk}r = {}a{} ;", r("(", N / 2), r(")", N / 2)),
        ),
        (
            "12,000 nested optionals",
            Render,
            format!("{ebnfk}r = {}a{} ;", r("[", 12000), r("]", 12000)),
        ),
        (
            "12,000 nested repetitions",
            Render,
            format!("{ebnfk}r = {}a{} ;", r("{", 12000), r("}", 12000)),
        ),
        (
            "50,000 postfix operators",
            Render,
            format!("{ebnfk}r = a{} ;", r("*+?", N / 3)),
        ),
        (
            "chain of 12,000 exceptions",
            Render,
            format!("{ebnfk}r = a{} ;", r(" - b", 12000)),
        ),
        (
            "exceptions nested 8,000 deep",
            Render,
            format!("{ebnfk}r = a{}{} ;", r(" - ( b", 8000), r(" )", 8000)),
        ),
        (
            "unclosed brackets",
            Parse,
            format!("{ebnfk}r = {}", r("([{", N / 3)),
        ),
        (
            "choice of 12,000 alternatives",
            Render,
            format!("{ebnfk}r = {}z ;", numbered(12000, |i| format!("a{i} | "))),
        ),
        (
            "sequence of 12,000 elements",
            Render,
            format!("{ebnfk}r = {};", r("a ", 12000)),
        ),
        (
            "choices nested 5,000 deep",
            Render,
            format!("{ebnfk}r = {}b{} ;", r("( a | ", 5000), r(" )", 5000)),
        ),
        (
            "5,000 rules",
            Render,
            format!("{ebnfk}{}", numbered(5000, |i| format!("r{i} = a ;\n"))),
        ),
        (
            "one rule name defined 5,000 times",
            Render,
            format!("{ebnfk}{}", r("r = a | b ;\n", 5000)),
        ),
        (
            "constructor calls nested 5,000 deep",
            Render,
            format!(
                "{irk}r = {}terminal(\"a\"){} ;",
                r("optional(oneOrMore(", 2500),
                r("))", 2500)
            ),
        ),
        (
            "sequence constructors nested 6,000 deep",
            Render,
            format!(
                "{irk}r = {}terminal(\"a\"){} ;",
                r("sequence(", 6000),
                r(")", 6000)
            ),
        ),
        (
            "choice call with 6,000 arguments",
            Render,
            format!(
                "{irk}r = choice({}special(\"b\")) ;",
                r("terminal(\"a\"),", 6000)
            ),
        ),
        (
            "unclosed constructor calls",
            Parse,
            format!("{irk}r = {}", r("choice(", N / 7)),
        ),
        (
            "ABNF groups nested 10,000 deep with repeats",
            Render,
            format!("{abnfk}r = {}a{} ;", r("2*3( ", 10000), r(" )", 10000)),
        ),
        (
            "ABNF optional groups nested 12,000 deep",
            Render,
            format!("{abnfk}r = {}a{} ;", r("[", 12000), r("]", 12000)),
        ),
        (
            "ABNF repeat of 400 digits",
            Render,
            format!(
                "{abnfk}r = {}*{}a {}b ;",
                r("9", 400),
                r("9", 400),
                r("9", 400)
            ),
        ),
        (
            "ABNF alternation of 12,000",
            Render,
            format!("{abnfk}r = {}b ;", r("a / ", 12000)),
        ),
        (
            "PEG predicates nested 10,000 deep",
            Render,
            format!("{pegk}R <- {}a{} ;", r("!( &( ", 5000), r(" ) )", 5000)),
        ),
        (
            "PEG groups with suffixes nested 10,000 deep",
            Render,
            format!("{pegk}R <- {}a{} ;", r("( ", 10000), r(" )*", 10000)),
        ),
        (
            "PEG run of operators",
            Parse,
            format!("{pegk}R <- {}a ;", r("!&", N / 2)),
        ),
        (
            "PEG sequence of 12,000 predicates",
            Render,
            format!("{pegk}R <- {};", r("!a &\"b\" ", 6000)),
        ),
    ];
    for (_, run, src) in cases {
        match run {
            Lex(table) => drop(tokenize(&src, table, DiagramType::Railroad)),
            Parse => drop(parse(&format!("{src}\n"), None)),
            Render => {
                if let Ok(m) = parse(&format!("{src}\n"), None) {
                    layout(&m, &Map::new(), &Options::default());
                }
            }
        }
    }
}
