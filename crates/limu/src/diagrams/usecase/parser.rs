//! The parser of a use case diagram, as Pele's parser.ts has it. Mermaid parses use case
//! diagrams with a Chevrotain parser, which decides each choice by looking at most three
//! tokens ahead and reports the first token that fits nowhere. This parser makes the same
//! decisions from the same lookahead, so it accepts the same text and names the same token
//! in its errors. It builds the model directly, where Mermaid builds a syntax tree and then
//! visits it.

use std::sync::OnceLock;

use super::db::{
    DraftBoundary, DraftClassDef, DraftClasses, DraftElement, DraftEndpoint, DraftJson, DraftLabel,
    DraftMetadata, DraftNote, DraftProperty, DraftRelationship, DraftStyle, Drafts, Named,
    PropertyValue, resolve_usecase,
};
use super::json::parse_ordered_json;
use super::lexer::{line_column, location_text, placed, tokenize};
use super::tokens::{T, TOKEN_NAMES};
use super::types::{
    BACK_ARROW, CIRCLE_ARROW, CIRCLE_ARROW_REVERSED, CROSS_ARROW, CROSS_ARROW_REVERSED,
    EdgeOccurrence, GraphStatement, LINE_SOLID, LabelType, MetadataOccurrence, NodeOccurrence,
    SOLID_ARROW, Span, UsecaseModel,
};
use crate::errors::{Error, ErrorCode};
use crate::js::chars::trim;
use crate::value::Map;

// The sets of token types the parser decides by, as bits of Pele's FLAGS.
const LINE: u8 = 1;
const RELATION: u8 = 2;
const STYLE_PART: u8 = 4;
const FOLLOWS_NAME: u8 = 8;
const BOUNDARY_LINE: u8 = 16;
const CLOSES_LABEL: u8 = 32;
const METADATA_FIRST: u8 = 64;

const ARROWS: &str = "DEPENDENCY_ARROW|GENERALIZATION|FORWARD_SOLID|BACKWARD_SOLID|MARKERLESS_SOLID|FORWARD_CIRCLE|BACKWARD_CIRCLE|FORWARD_CROSS|BACKWARD_CROSS";
const NAMES: &str = "IDENTIFIER|PLAIN_STRING|MARKDOWN_STRING";
const LABELS: &str = "PLAIN_STRING|MARKDOWN_STRING|LABEL_TEXT";
const STATEMENTS: &str = "ACC_TITLE_LINE|ACC_DESCR_LINE|ACC_DESCR_BLOCK|DIRECTION|ACTOR|SYSTEM_BOUNDARY|NOTE|JSON_DECLARATION_START|CLASS_DEF|CLASS|STYLE";
const STYLE_PARTS: &str = "WORD|PLAIN_STRING|NUMBER|HASH_COLOR|CSS_IDENTIFIER|CSS_ESCAPED_COMMA|DASH|DOT|PERCENT|CSS_PUNCTUATION|COLON|LPAREN|RPAREN|LBRACKET|RBRACKET|LBRACE|RBRACE|AT|MARKERLESS_SOLID";

/// The sets each token type is in, as Pele's FLAGS holds them.
fn flags(t: T) -> u8 {
    static FLAGS: OnceLock<[u8; TOKEN_NAMES.len()]> = OnceLock::new();
    FLAGS.get_or_init(|| {
        let mut out = [0u8; TOKEN_NAMES.len()];
        let mut flag = |names: &str, bits: u8| {
            for name in names.split('|') {
                // WORD is a category: every keyword and IDENTIFIER.
                if name == "WORD" {
                    for f in &mut out[T::USECASE as usize..=T::IDENTIFIER as usize] {
                        *f |= bits;
                    }
                } else if let Some(t) = TOKEN_NAMES.iter().position(|&n| n == name) {
                    out[t] |= bits;
                }
            }
        };
        flag(&format!("NEWLINE|COMMENT|{STATEMENTS}|{NAMES}"), LINE);
        flag(&format!("IDENTIFIER|{ARROWS}"), RELATION);
        flag(STYLE_PARTS, STYLE_PART);
        flag(
            &format!("STEREOTYPE_START|CLASS_SEPARATOR|IDENTIFIER|{ARROWS}|NEWLINE|EOF"),
            FOLLOWS_NAME,
        );
        flag(&format!("NEWLINE|COMMENT|ACTOR|{NAMES}"), BOUNDARY_LINE);
        flag(
            "FORWARD_SOLID|MARKERLESS_SOLID|FORWARD_CIRCLE|FORWARD_CROSS",
            CLOSES_LABEL,
        );
        flag("NEWLINE|IDENTIFIER|PLAIN_STRING|RBRACE", METADATA_FIRST);
        out
    })[t as usize]
}

/// The PlantUML statements people are most likely to try, which Mermaid turns away, Pele's
/// FORBIDDEN.
const FORBIDDEN: [&str; 5] = [
    "allowmixing",
    "newpage",
    "package",
    "rectangle",
    "skinparam",
];

/// An element as a statement names it, Pele's Item. It is explicit where the mention says
/// something about the element, and so declares it.
struct Item {
    id: String,
    label: DraftLabel,
    span: Span,
    generated: bool,
    shape: Option<&'static str>,
    metadata: Option<Vec<DraftProperty>>,
    stereotype: Option<String>,
    stereotype_span: Option<Span>,
    classes: Vec<String>,
    class_spans: Vec<Span>,
    explicit: bool,
    node_span: Span,
}

/// The arrow of a relationship and what it points at, Pele's Tail.
struct Tail {
    explicit_id: Option<String>,
    explicit_id_span: Option<Span>,
    kind: &'static str,
    arrow_type: usize,
    label: Option<DraftLabel>,
    minlen: f64,
    target: Item,
    span: Span,
}

type Parsed<T> = Result<T, Error>;

/// The state of a parse, which Pele keeps in its module's variables.
struct Parser<'a> {
    src: &'a str,
    types: Vec<T>,
    starts: Vec<usize>,
    ends: Vec<usize>,
    p: usize,
    /// Where the last token before the most recent line end stops, which is where its
    /// statement stops.
    last_end: usize,
    drafts: Drafts,
    /// The index of the boundary the statements are in, among the drafts' boundaries.
    parent: Option<usize>,
    /// The index of each `json` node among the drafts, and of the token of its text.
    pending_json: Vec<(usize, usize)>,
    edge_count: usize,
    note_count: usize,
    /// The statements at the top, read so far.
    statements: Vec<GraphStatement>,
}

/// The id Pele's generatedId makes of a label: each code unit outside `\w` replaced by an
/// underscore.
fn generated_id(label: &str) -> String {
    let mut out = String::with_capacity(label.len());
    for c in label.chars() {
        if c.is_ascii_alphanumeric() || c == '_' {
            out.push(c);
        } else {
            out.extend(std::iter::repeat_n('_', c.len_utf16()));
        }
    }
    out
}

fn is_word(t: T) -> bool {
    (T::USECASE..=T::IDENTIFIER).contains(&t)
}

fn is_label_text(t: T) -> bool {
    (T::USECASE..=T::LABEL_SYMBOL).contains(&t)
}

fn is_key(t: T) -> bool {
    t == T::IDENTIFIER || t == T::PLAIN_STRING
}

fn occurrences(properties: &[DraftProperty]) -> Vec<MetadataOccurrence> {
    properties.iter().map(|p| p.at.clone()).collect()
}

/// The token sequences Chevrotain lists where no statement starts, as Pele's
/// statementSequences writes them.
fn statement_sequences() -> String {
    let names: Vec<&str> = NAMES.split('|').collect();
    let metadata: Vec<String> = names
        .iter()
        .flat_map(|name| {
            ["NEWLINE", "IDENTIFIER", "PLAIN_STRING", "RBRACE"]
                .map(|next| format!("{name}, METADATA_START, {next}"))
        })
        .collect();
    let follows = format!("STEREOTYPE_START|CLASS_SEPARATOR|IDENTIFIER|{ARROWS}|NEWLINE|EOF");
    let follows: Vec<&str> = follows.split('|').collect();
    let entities = names.iter().enumerate().flat_map(|(i, name)| {
        let mut nexts = if i == 0 {
            vec!["LPAREN", "LBRACKET"]
        } else {
            vec![]
        };
        nexts.extend(&follows);
        nexts
            .into_iter()
            .map(|next| format!("{name}, {next}"))
            .collect::<Vec<_>>()
    });
    let mut all = vec![STATEMENTS.to_string()];
    all.extend(metadata.iter().cloned());
    all.extend(entities);
    all.extend(metadata);
    all.join("|")
}

impl Parser<'_> {
    fn ty(&self, k: usize) -> T {
        self.types.get(k).copied().unwrap_or(T::EOF)
    }

    fn cur(&self) -> T {
        self.ty(self.p)
    }

    fn image(&self, k: usize) -> &str {
        &self.src[self.starts[k]..self.ends[k]]
    }

    fn span(&self, k: usize) -> Span {
        [self.starts[k], self.ends[k]]
    }

    fn fail(&self, detail: &str) -> Error {
        let at = self.starts[self.p];
        let (line, column) = line_column(self.src, at);
        let message = format!(
            "Error parsing usecase diagram: {detail} at {}",
            location_text(self.src, at, self.ends[self.p])
        );
        placed(message, ErrorCode::Syntax, line, column)
    }

    fn mismatch(&self, name: &str) -> Error {
        self.fail(&format!(
            "Expecting token of type --> {name} <-- but found --> '{}' <--",
            self.image(self.p)
        ))
    }

    fn expect(&mut self, t: T) -> Parsed<usize> {
        if self.cur() != t {
            return Err(self.mismatch(t.name()));
        }
        self.p += 1;
        Ok(self.p - 1)
    }

    /// The error Chevrotain gives where none of the token sequences it would have accepted
    /// comes next, listing them.
    fn no_alternative(&self, expected: &str) -> Error {
        let list: Vec<String> = expected
            .split('|')
            .enumerate()
            .map(|(i, sequence)| format!("  {}. [{sequence}]", i + 1))
            .collect();
        self.fail(&format!(
            "Expecting: one of these possible Token sequences:\n{}\nbut found: '{}'",
            list.join("\n"),
            self.image(self.p)
        ))
    }

    fn line_end(&mut self) -> Parsed<()> {
        self.last_end = self.ends[self.p - 1];
        if self.cur() == T::NEWLINE {
            self.p += 1;
        } else if self.cur() != T::EOF {
            return Err(self.no_alternative("NEWLINE|EOF"));
        }
        Ok(())
    }

    /// The text of a name or string token, without its quotes.
    fn content(&self, k: usize) -> Span {
        let trim = match self.ty(k) {
            T::MARKDOWN_STRING => 2,
            T::PLAIN_STRING => 1,
            _ => 0,
        };
        [self.starts[k] + trim, self.ends[k] - trim]
    }

    fn token_label(&self, k: usize) -> DraftLabel {
        let at = self.content(k);
        DraftLabel {
            text: self.src[at[0]..at[1]].to_string(),
            kind: if self.ty(k) == T::MARKDOWN_STRING {
                LabelType::Markdown
            } else {
                LabelType::Text
            },
            span: at,
        }
    }

    /// A label, as Pele's label reads it. An unquoted label runs to the next delimiter and is
    /// taken from the source as written.
    fn label(&mut self) -> Parsed<DraftLabel> {
        let t = self.cur();
        if t == T::PLAIN_STRING || t == T::MARKDOWN_STRING {
            self.p += 1;
            return Ok(self.token_label(self.p - 1));
        }
        if !is_label_text(t) {
            return Err(self.no_alternative(LABELS));
        }
        let first = self.p;
        while is_label_text(self.cur()) {
            self.p += 1;
        }
        let at = [self.starts[first], self.ends[self.p - 1]];
        Ok(DraftLabel {
            text: self.src[at[0]..at[1]].to_string(),
            kind: LabelType::Text,
            span: at,
        })
    }

    fn property(&mut self) -> Parsed<DraftProperty> {
        let key = self.p;
        if !is_key(self.cur()) {
            return Err(self.no_alternative("IDENTIFIER|PLAIN_STRING"));
        }
        self.p += 1;
        self.expect(T::COLON)?;
        let value = self.p;
        let t = self.cur();
        if !is_key(t) && t != T::TRUE && t != T::FALSE {
            return Err(self.no_alternative("IDENTIFIER|PLAIN_STRING|TRUE|FALSE"));
        }
        self.p += 1;
        let key_span = self.content(key);
        let value_span = self.content(value);
        Ok(DraftProperty {
            at: MetadataOccurrence {
                key: self.src[key_span[0]..key_span[1]].to_string(),
                span: [self.starts[key], self.ends[value]],
                key_span,
                value_span,
            },
            value: match t {
                T::TRUE => PropertyValue::Bool(true),
                T::FALSE => PropertyValue::Bool(false),
                _ => PropertyValue::String(self.src[value_span[0]..value_span[1]].to_string()),
            },
        })
    }

    /// Whether another property follows, as Pele's separatorAhead says. A comma or line
    /// breaks before the closing brace do not count, but Chevrotain looks only three tokens
    /// ahead, so a comma and two line breaks, or three line breaks, are taken for a separator
    /// and the brace after them is then an error.
    fn separator_ahead(&self) -> bool {
        let (a, b, c) = (self.cur(), self.ty(self.p + 1), self.ty(self.p + 2));
        if a == T::COMMA {
            return is_key(b) || (b == T::NEWLINE && (c == T::NEWLINE || is_key(c)));
        }
        if a != T::NEWLINE {
            return false;
        }
        b == T::COMMA
            || is_key(b)
            || (b == T::NEWLINE && (c == T::NEWLINE || c == T::COMMA || is_key(c)))
    }

    fn metadata(&mut self) -> Parsed<Vec<DraftProperty>> {
        self.expect(T::METADATA_START)?;
        let mut properties = Vec::new();
        self.skip_newlines();
        if is_key(self.cur()) {
            properties.push(self.property()?);
            while self.separator_ahead() {
                if self.cur() == T::COMMA {
                    self.p += 1;
                } else {
                    self.skip_newlines();
                    if self.cur() == T::COMMA {
                        self.p += 1;
                    }
                }
                self.skip_newlines();
                properties.push(self.property()?);
            }
            if self.cur() == T::COMMA {
                self.p += 1;
            }
            self.skip_newlines();
        }
        self.expect(T::RBRACE)?;
        Ok(properties)
    }

    fn skip_newlines(&mut self) {
        while self.cur() == T::NEWLINE {
            self.p += 1;
        }
    }

    fn class_suffix(&mut self, classes: &mut Vec<String>, spans: &mut Vec<Span>) -> Parsed<()> {
        loop {
            self.p += 1;
            let id = self.expect(T::IDENTIFIER)?;
            classes.push(self.image(id).to_string());
            spans.push(self.span(id));
            if self.cur() != T::COMMA {
                return Ok(());
            }
        }
    }

    /// An id with an optional label in parentheses or brackets, or a string that stands for
    /// both, as Pele's name reads it.
    fn name(&mut self, brackets: bool) -> Parsed<Item> {
        let k = self.p;
        let t = self.cur();
        let mut item = Item {
            id: String::new(),
            label: DraftLabel {
                text: String::new(),
                kind: LabelType::Text,
                span: [0, 0],
            },
            span: self.content(k),
            generated: t != T::IDENTIFIER,
            shape: None,
            metadata: None,
            stereotype: None,
            stereotype_span: None,
            classes: Vec::new(),
            class_spans: Vec::new(),
            explicit: false,
            node_span: [0, 0],
        };
        if t == T::IDENTIFIER {
            self.p += 1;
            item.id = self.image(k).to_string();
            let open = self.cur();
            if open == T::LPAREN || (brackets && open == T::LBRACKET) {
                self.p += 1;
                item.label = self.label()?;
                self.expect(if open == T::LPAREN {
                    T::RPAREN
                } else {
                    T::RBRACKET
                })?;
                item.shape = Some(if open == T::LPAREN { "ellipse" } else { "rect" });
            } else {
                item.label = self.token_label(k);
            }
        } else if t == T::PLAIN_STRING || t == T::MARKDOWN_STRING {
            self.p += 1;
            item.label = self.token_label(k);
            item.id = generated_id(&item.label.text);
        } else {
            return Err(self.no_alternative(NAMES));
        }
        Ok(item)
    }

    fn element(&mut self, actor: bool) -> Parsed<Item> {
        let first = self.p;
        let mut item = self.name(!actor)?;
        if actor {
            item.shape = None;
        }
        if self.cur() == T::METADATA_START {
            item.metadata = Some(self.metadata()?);
        }
        if self.cur() == T::STEREOTYPE_START {
            self.p += 1;
            let text = self.expect(T::STEREOTYPE_TEXT)?;
            self.expect(T::STEREOTYPE_END)?;
            item.stereotype = Some(trim(self.image(text)).to_string());
            item.stereotype_span = Some(self.span(text));
        }
        if self.cur() == T::CLASS_SEPARATOR {
            let (mut classes, mut spans) = (Vec::new(), Vec::new());
            self.class_suffix(&mut classes, &mut spans)?;
            item.classes.extend(classes);
            item.class_spans.extend(spans);
        }
        item.explicit = item.shape.is_some()
            || item.metadata.is_some()
            || item.stereotype.as_ref().is_some_and(|s| !s.is_empty());
        item.node_span = [self.starts[first], self.ends[self.p - 1]];
        Ok(item)
    }

    fn draft(&self, item: &Item, kind: &'static str) -> DraftElement {
        DraftElement {
            named: named(item),
            kind,
            parent: self.parent,
            shape: item.shape,
            metadata: item.metadata.clone(),
            stereotype: item.stereotype.clone(),
            classes: item.classes.clone(),
        }
    }

    /// Whether a label and then one of the operators that can close it come next, as Pele's
    /// labeledRight says.
    fn labeled_right(&self, closers: u8) -> bool {
        let label = |t: T| (T::USECASE..=T::MARKDOWN_STRING).contains(&t);
        if !label(self.cur()) {
            return false;
        }
        let mut k = self.p + 1;
        loop {
            let t = self.ty(k);
            if t == T::MARKERLESS_SOLID || flags(t) & closers != 0 {
                return true;
            }
            if !label(t) {
                return false;
            }
            k += 1;
        }
    }

    /// The fewest ranks between the ends of an arrow, as Pele's minimumLength counts them:
    /// each dash beyond the second asks for one more.
    fn minimum_length(&self, k: usize) -> f64 {
        let marker = if self.ty(k) == T::MARKERLESS_SOLID {
            0
        } else {
            1
        };
        let dashes = (self.ends[k] - self.starts[k]) as i64 - marker;
        (dashes - 1).max(1) as f64
    }

    fn relation_tail(&mut self) -> Parsed<Tail> {
        let first = self.p;
        let mut explicit_id = None;
        let mut explicit_id_span = None;
        if self.cur() == T::IDENTIFIER && self.ty(self.p + 1) == T::AT {
            explicit_id = Some(self.image(self.p).to_string());
            explicit_id_span = Some(self.span(self.p));
            self.p += 2;
        }
        let (mut kind, mut arrow_type, mut minlen, mut label) =
            ("association", SOLID_ARROW, 1.0, None);
        let k = self.p;
        let t = self.cur();
        self.p += 1;
        let length = self.ends[k] - self.starts[k];
        match t {
            T::DEPENDENCY_ARROW => {
                self.expect(T::COLON)?;
                let c = self.cur();
                if c != T::INCLUDE && c != T::EXTEND {
                    return Err(self.no_alternative("INCLUDE|EXTEND"));
                }
                kind = if c == T::INCLUDE { "include" } else { "extend" };
                label = Some(DraftLabel {
                    text: kind.to_string(),
                    kind: LabelType::Text,
                    span: self.span(self.p),
                });
                self.p += 1;
            }
            T::GENERALIZATION => kind = "generalization",
            T::FORWARD_SOLID => minlen = self.minimum_length(k),
            T::BACKWARD_SOLID => {
                arrow_type = BACK_ARROW;
                // Only the shortest operator can open a label: `A <-- label -- B`.
                if length == 3 && self.labeled_right(0) {
                    label = Some(self.label()?);
                    let close = self.expect(T::MARKERLESS_SOLID)?;
                    minlen = self.minimum_length(close);
                } else {
                    minlen = self.minimum_length(k);
                }
            }
            T::MARKERLESS_SOLID => {
                arrow_type = LINE_SOLID;
                if length == 2 && self.labeled_right(CLOSES_LABEL) {
                    label = Some(self.label()?);
                    match self.cur() {
                        T::FORWARD_SOLID => arrow_type = SOLID_ARROW,
                        T::FORWARD_CIRCLE => arrow_type = CIRCLE_ARROW,
                        T::FORWARD_CROSS => arrow_type = CROSS_ARROW,
                        T::MARKERLESS_SOLID => {}
                        _ => {
                            return Err(self.no_alternative(
                                "FORWARD_SOLID|MARKERLESS_SOLID|FORWARD_CIRCLE|FORWARD_CROSS",
                            ));
                        }
                    }
                    if arrow_type == SOLID_ARROW || arrow_type == LINE_SOLID {
                        minlen = self.minimum_length(self.p);
                    }
                    self.p += 1;
                } else {
                    minlen = self.minimum_length(k);
                }
            }
            T::FORWARD_CIRCLE => arrow_type = CIRCLE_ARROW,
            T::FORWARD_CROSS => arrow_type = CROSS_ARROW,
            T::BACKWARD_CIRCLE | T::BACKWARD_CROSS => {
                arrow_type = if t == T::BACKWARD_CIRCLE {
                    CIRCLE_ARROW_REVERSED
                } else {
                    CROSS_ARROW_REVERSED
                };
                if self.labeled_right(0) {
                    label = Some(self.label()?);
                    self.expect(T::MARKERLESS_SOLID)?;
                }
            }
            _ => {
                self.p -= 1;
                return Err(self.no_alternative(ARROWS));
            }
        }
        let target = self.element(false)?;
        Ok(Tail {
            explicit_id,
            explicit_id_span,
            kind,
            arrow_type,
            label,
            minlen,
            target,
            span: [self.starts[first], self.ends[self.p - 1]],
        })
    }

    fn relationship(
        &mut self,
        source: &Item,
        declaration: bool,
        tail: Tail,
        mut nodes: Vec<NodeOccurrence>,
    ) -> GraphStatement {
        let target = &tail.target;
        if target.explicit {
            let d = self.draft(target, "usecase");
            self.drafts.elements.push(d);
        }
        self.drafts.relationships.push(DraftRelationship {
            source: endpoint(source, declaration),
            target: endpoint(target, target.explicit),
            span: tail.span,
            id: tail.explicit_id.clone(),
            id_span: tail.explicit_id_span,
            kind: tail.kind,
            arrow_type: tail.arrow_type,
            label: tail.label.clone(),
            minlen: tail.minlen,
        });
        nodes.push(occurrence(target, target.explicit));
        let mut out = GraphStatement::new("edge");
        out.nodes = Some(nodes);
        let id = match tail.explicit_id {
            Some(id) => id,
            None => {
                self.edge_count += 1;
                format!("edge-{}", self.edge_count - 1)
            }
        };
        out.edges = Some(vec![EdgeOccurrence {
            id,
            span: out.span,
            label_span: tail.label.map(|l| l.span),
            id_span: tail.explicit_id_span,
            metadata: None,
        }]);
        out
    }

    /// `actor A, B` declares several actors, and at the top `actor A --> B` declares one and
    /// relates it, as Pele's actors reads them.
    fn actors(&mut self, top: bool) -> Parsed<GraphStatement> {
        self.p += 1;
        let mut items = vec![self.element(true)?];
        let mut tail = None;
        if self.cur() == T::COMMA {
            while self.cur() == T::COMMA {
                self.p += 1;
                items.push(self.element(true)?);
            }
        } else if top && flags(self.cur()) & RELATION != 0 {
            tail = Some(self.relation_tail()?);
        }
        self.line_end()?;
        for item in &items {
            let d = self.draft(item, "actor");
            self.drafts.elements.push(d);
        }
        let nodes = items.iter().map(|item| occurrence(item, true)).collect();
        Ok(match tail {
            Some(tail) => self.relationship(&items[0], true, tail, nodes),
            None => declaration(nodes),
        })
    }

    fn entity(&mut self, top: bool) -> Parsed<GraphStatement> {
        let mut source = self.element(false)?;
        let tail = if top && flags(self.cur()) & RELATION != 0 {
            Some(self.relation_tail()?)
        } else {
            None
        };
        self.line_end()?;
        if tail.is_none() {
            source.explicit = true;
            source.shape = source.shape.or(Some("ellipse"));
        }
        if source.explicit {
            let d = self.draft(&source, "usecase");
            self.drafts.elements.push(d);
        }
        let nodes = vec![occurrence(&source, source.explicit)];
        Ok(match tail {
            Some(tail) => {
                let declares = source.explicit;
                self.relationship(&source, declares, tail, nodes)
            }
            None => declaration(nodes),
        })
    }

    fn boundary(&mut self) -> Parsed<GraphStatement> {
        self.p += 1;
        let mut named = self.name(true)?;
        let inline = if self.cur() == T::METADATA_START {
            Some(self.metadata()?)
        } else {
            None
        };
        if self.cur() == T::CLASS_SEPARATOR {
            let (mut classes, mut spans) = (Vec::new(), Vec::new());
            self.class_suffix(&mut classes, &mut spans)?;
            named.classes.extend(classes);
            named.class_spans.extend(spans);
        }
        self.line_end()?;
        let group = DraftBoundary {
            named: Named {
                id: named.id.clone(),
                label: named.label.clone(),
                span: named.span,
                generated: named.generated,
            },
            classes: named.classes.clone(),
        };
        self.drafts.boundaries.push(group);
        self.parent = Some(self.drafts.boundaries.len() - 1);
        let mut children = Vec::new();
        while flags(self.cur()) & BOUNDARY_LINE != 0 {
            children.push(self.line(false)?);
        }
        self.parent = None;
        let end = self.expect(T::END)?;
        self.line_end()?;
        let mut out = GraphStatement::new("group");
        out.group = Some(named.id.clone());
        out.id_span = Some(named.span);
        out.title_span = Some(named.label.span);
        out.end_span = Some(self.span(end));
        out.class_spans = Some(named.class_spans);
        if !children.is_empty() {
            out.children = Some(children);
        }
        if let Some(inline) = inline {
            out.metadata = Some(occurrences(&inline));
            self.drafts.metadata.push(DraftMetadata {
                target: named.id,
                span: named.span,
                properties: inline,
                statement: self.statements.len(),
            });
        }
        Ok(out)
    }

    fn style(&mut self) -> Parsed<String> {
        let first = self.p;
        let t = self.cur();
        if is_word(t) || t == T::CSS_IDENTIFIER {
            self.p += 1;
        } else if t == T::MARKERLESS_SOLID {
            self.p += 1;
            if !is_word(self.cur()) {
                return Err(self.mismatch("WORD"));
            }
            self.p += 1;
        } else {
            return Err(self.no_alternative("WORD|CSS_IDENTIFIER|MARKERLESS_SOLID"));
        }
        self.expect(T::COLON)?;
        if flags(self.cur()) & STYLE_PART == 0 {
            return Err(self.fail(&format!(
                "Expecting: expecting at least one iteration which starts with one of these possible Token sequences::\n  <[{}]>\nbut found: '{}'",
                STYLE_PARTS.replace('|', "] ,["),
                self.image(self.p)
            )));
        }
        while flags(self.cur()) & STYLE_PART != 0 {
            self.p += 1;
        }
        // The value is taken from the source, so the spaces inside `1px solid red` survive.
        Ok(self.src[self.starts[first]..self.ends[self.p - 1]].replace("\\,", ","))
    }

    fn styles(&mut self) -> Parsed<Vec<String>> {
        let mut out = vec![self.style()?];
        while self.cur() == T::COMMA {
            self.p += 1;
            out.push(self.style()?);
        }
        Ok(out)
    }

    fn identifiers(&mut self) -> Parsed<Vec<usize>> {
        let mut out = vec![self.expect(T::IDENTIFIER)?];
        while self.cur() == T::COMMA {
            self.p += 1;
            out.push(self.expect(T::IDENTIFIER)?);
        }
        Ok(out)
    }

    fn reference(&self, k: usize) -> NodeOccurrence {
        let at = self.content(k);
        NodeOccurrence {
            id: self.image(k).to_string(),
            span: at,
            id_span: at,
            label_span: None,
            defines: false,
            stereotype_span: None,
            metadata: None,
            class_spans: None,
        }
    }

    fn top_statement(&mut self) -> Parsed<GraphStatement> {
        let k = self.p;
        let t = self.cur();
        let mut out;
        match t {
            T::ACC_TITLE_LINE | T::ACC_DESCR_LINE | T::ACC_DESCR_BLOCK => {
                self.p += 1;
                let text = self.image(k);
                let after = |c: char| text.find(c).map_or(0, |i| i + 1);
                if t == T::ACC_DESCR_BLOCK {
                    let to = text.rfind('}').unwrap_or(0);
                    let from = after('{');
                    let inner = if from <= to { &text[from..to] } else { "" };
                    self.drafts.acc_description = trim(inner).to_string();
                } else if t == T::ACC_DESCR_LINE {
                    self.drafts.acc_description = trim(&text[after(':')..]).to_string();
                } else {
                    self.drafts.acc_title = trim(&text[after(':')..]).to_string();
                }
                out = GraphStatement::new(if t == T::ACC_TITLE_LINE {
                    "accTitle"
                } else {
                    "accDescr"
                });
            }
            T::DIRECTION => {
                self.p += 1;
                let value = self.cur();
                self.drafts.direction = Some(match value {
                    T::TD | T::TB => "TB",
                    T::BT => "BT",
                    T::LR => "LR",
                    T::RL => "RL",
                    _ => return Err(self.no_alternative("TD|TB|BT|LR|RL")),
                });
                self.p += 1;
                out = GraphStatement::new("direction");
            }
            T::ACTOR => return self.actors(true),
            T::SYSTEM_BOUNDARY => return self.boundary(),
            T::NOTE => {
                self.p += 1;
                self.expect(T::FOR)?;
                let target = self.expect(T::IDENTIFIER)?;
                let text = self.label()?;
                let span = text.span;
                self.drafts.notes.push(DraftNote {
                    target: self.image(target).to_string(),
                    span: self.span(target),
                    label: text,
                });
                out = GraphStatement::new("note");
                out.reference = Some(format!("note-{}", self.note_count));
                self.note_count += 1;
                out.ref_span = Some(span);
                out.nodes = Some(vec![self.reference(target)]);
            }
            T::JSON_DECLARATION_START => {
                self.p += 1;
                let literal = self.expect(T::JSON_OBJECT_LITERAL)?;
                let (mut classes, mut class_spans) = (Vec::new(), Vec::new());
                if self.cur() == T::CLASS_SEPARATOR {
                    self.class_suffix(&mut classes, &mut class_spans)?;
                }
                let b = self.src.as_bytes();
                let mut from = self.starts[k] + 4;
                while b[from] == b' ' || b[from] == b'\t' {
                    from += 1;
                }
                let mut to = from;
                while b
                    .get(to)
                    .is_some_and(|&c| c.is_ascii_alphanumeric() || c == b'_')
                {
                    to += 1;
                }
                let json = DraftJson {
                    id: self.src[from..to].to_string(),
                    value: Map::new(),
                    property_order: Vec::new(),
                    span: [from, to],
                    classes,
                };
                out = GraphStatement::new("json");
                out.nodes = Some(vec![NodeOccurrence {
                    id: json.id.clone(),
                    span: json.span,
                    id_span: json.span,
                    label_span: None,
                    defines: true,
                    stereotype_span: None,
                    metadata: None,
                    class_spans: Some(class_spans.clone()),
                }]);
                out.class_spans = Some(class_spans);
                self.pending_json.push((self.drafts.jsons.len(), literal));
                self.drafts.jsons.push(json);
            }
            T::CLASS_DEF => {
                self.p += 1;
                let ids = self.identifiers()?;
                let styles = self.styles()?;
                self.drafts.class_defs.push(DraftClassDef {
                    ids: ids.iter().map(|&id| self.image(id).to_string()).collect(),
                    styles,
                });
                out = GraphStatement::new("classDef");
                out.reference = Some(self.image(ids[0]).to_string());
                out.ref_span = Some(self.span(ids[0]));
            }
            T::CLASS => {
                self.p += 1;
                let targets = self.identifiers()?;
                let classes = self.identifiers()?;
                self.drafts.classes.push(DraftClasses {
                    targets: targets
                        .iter()
                        .map(|&id| (self.image(id).to_string(), self.span(id)))
                        .collect(),
                    classes: classes
                        .iter()
                        .map(|&id| self.image(id).to_string())
                        .collect(),
                });
                out = GraphStatement::new("classAssign");
                out.reference = Some(self.image(classes[0]).to_string());
                out.ref_span = Some(self.span(classes[0]));
                out.nodes = Some(targets.iter().map(|&id| self.reference(id)).collect());
            }
            T::STYLE => {
                self.p += 1;
                let target = self.expect(T::IDENTIFIER)?;
                let styles = self.styles()?;
                self.drafts.styles.push(DraftStyle {
                    target: self.image(target).to_string(),
                    span: self.span(target),
                    styles,
                });
                out = GraphStatement::new("style");
                out.nodes = Some(vec![self.reference(target)]);
            }
            _ => {
                let next = self.ty(self.p + 1);
                if next == T::METADATA_START {
                    if flags(self.ty(self.p + 2)) & METADATA_FIRST == 0 {
                        return Err(self.no_alternative(&statement_sequences()));
                    }
                    self.p += 1;
                    let mut target = self.reference(k);
                    if t != T::IDENTIFIER {
                        target.id = generated_id(&self.src[target.span[0]..target.span[1]]);
                    }
                    let assigned = self.metadata()?;
                    out = GraphStatement::new("metadata");
                    out.metadata = Some(occurrences(&assigned));
                    self.drafts.metadata.push(DraftMetadata {
                        target: target.id.clone(),
                        span: target.span,
                        properties: assigned,
                        statement: self.statements.len(),
                    });
                    out.nodes = Some(vec![target]);
                } else {
                    let follows = flags(next) & FOLLOWS_NAME != 0
                        || (t == T::IDENTIFIER && (next == T::LPAREN || next == T::LBRACKET));
                    // Mermaid turns away the PlantUML statements people are most likely to
                    // try.
                    if !follows
                        || (t == T::IDENTIFIER
                            && FORBIDDEN.contains(&self.image(k).to_ascii_lowercase().as_str()))
                    {
                        return Err(self.no_alternative(&statement_sequences()));
                    }
                    return self.entity(true);
                }
            }
        }
        self.line_end()?;
        Ok(out)
    }

    fn line(&mut self, top: bool) -> Parsed<GraphStatement> {
        let k = self.p;
        let t = self.cur();
        if t == T::NEWLINE {
            self.p += 1;
            let mut out = GraphStatement::new("blank");
            out.span = self.span(k);
            return Ok(out);
        }
        if t == T::COMMENT {
            self.p += 1;
            self.line_end()?;
            let mut out = GraphStatement::new("comment");
            out.span = self.span(k);
            return Ok(out);
        }
        let mut out = if top {
            self.top_statement()?
        } else if t == T::ACTOR {
            self.actors(false)?
        } else {
            self.entity(false)?
        };
        out.span = [self.starts[k], self.last_end];
        for edge in out.edges.iter_mut().flatten() {
            edge.span = out.span;
        }
        Ok(out)
    }
}

fn named(item: &Item) -> Named {
    Named {
        id: item.id.clone(),
        label: item.label.clone(),
        span: item.span,
        generated: item.generated,
    }
}

fn endpoint(item: &Item, declares: bool) -> DraftEndpoint {
    DraftEndpoint {
        named: named(item),
        declares,
        classed: !item.classes.is_empty(),
    }
}

fn occurrence(item: &Item, defines: bool) -> NodeOccurrence {
    NodeOccurrence {
        id: item.id.clone(),
        span: item.node_span,
        id_span: item.span,
        label_span: Some(item.label.span),
        defines,
        stereotype_span: item.stereotype_span,
        metadata: item.metadata.as_deref().map(occurrences),
        class_spans: (!item.class_spans.is_empty()).then(|| item.class_spans.clone()),
    }
}

fn declaration(nodes: Vec<NodeOccurrence>) -> GraphStatement {
    let mut out = GraphStatement::new("node");
    out.nodes = Some(nodes);
    out
}

/// Parses a use case diagram, as Pele's parseUsecase does.
pub(crate) fn parse_usecase(source: &str, title: Option<&str>) -> Result<UsecaseModel, Error> {
    let tokens = tokenize(source)?;
    let mut p = Parser {
        src: source,
        types: tokens.types,
        starts: tokens.starts,
        ends: tokens.ends,
        p: 0,
        last_end: 0,
        drafts: Drafts::default(),
        parent: None,
        pending_json: Vec::new(),
        edge_count: 0,
        note_count: 0,
        statements: Vec::new(),
    };
    let header = p.expect(T::USECASE)?;
    p.line_end()?;
    while flags(p.cur()) & LINE != 0 {
        let s = p.line(true)?;
        p.statements.push(s);
    }
    if p.cur() != T::EOF {
        return Err(p.fail(&format!(
            "Redundant input, expecting EOF but found: {}",
            p.image(p.p)
        )));
    }
    // The whole text has to be well formed before the JSON in it is looked at, as in
    // Mermaid.
    for &(json, literal) in &p.pending_json {
        let at = p.starts[literal];
        let parsed = parse_ordered_json(p.image(literal), || line_column(source, at))?;
        let d = &mut p.drafts.jsons[json];
        d.value = parsed.value;
        d.property_order = parsed.property_order;
    }
    let header_span = p.span(header);
    resolve_usecase(source, p.drafts, header_span, p.statements, title)
}
