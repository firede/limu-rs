//! The parser of an agent flow, as Pele's parser.ts has it: recursive descent over the tokens
//! of its lexer, agreeing with Mermaid's agentflow.jison, its errors worded as Jison words
//! them.

use std::borrow::Cow;

use super::db::{AgentflowDb, DocItem, FlowText, LabelType, LinkInfo, Loc};
use super::lexer::{Tok, tokenize};
use super::tokens::{DIRECTION_TOKENS, T};
use crate::detect::DiagramType;
use crate::errors::{Edit, Error, input_offset, syntax_error};
use crate::js::chars::{is_space_char, trim, trim_end};
use crate::value::{Map, Value};

/// The tokens an id is made of, Pele's ID.
fn is_id(t: T) -> bool {
    matches!(
        t,
        T::NUM
            | T::NODE_STRING
            | T::DOWN
            | T::MINUS
            | T::DEFAULT
            | T::COMMA
            | T::COLON
            | T::AMP
            | T::BRKT
            | T::MULT
            | T::UNICODE_TEXT
    )
}

/// The tokens a callback's name or a curve is made of, Pele's ALPHANUM.
fn is_alpha_num(t: T) -> bool {
    matches!(
        t,
        T::NUM
            | T::UNICODE_TEXT
            | T::NODE_STRING
            | T::DIR
            | T::DOWN
            | T::MINUS
            | T::COMMA
            | T::COLON
            | T::AMP
            | T::BRKT
            | T::MULT
    )
}

/// The tokens the id of a flow or a connector is made of, Pele's NO_TAGS.
fn is_no_tags(t: T) -> bool {
    matches!(
        t,
        T::NUM
            | T::NODE_STRING
            | T::SPACE
            | T::MINUS
            | T::AMP
            | T::UNICODE_TEXT
            | T::COLON
            | T::MULT
            | T::BRKT
            | T::STYLE
            | T::LINKSTYLE
            | T::CLASSDEF
            | T::CLASS
            | T::CLICK
            | T::GRAPH
            | T::DIR
            | T::flow
            | T::connector
            | T::global
            | T::end
            | T::DOWN
            | T::UP
            | T::START_LINK
    )
}

/// The tokens a style is made of, Pele's STYLE_PART.
fn is_style_part(t: T) -> bool {
    matches!(
        t,
        T::NUM | T::NODE_STRING | T::COLON | T::SPACE | T::BRKT | T::STYLE
    )
}

/// The tokens that end a statement, Pele's SEPARATOR.
fn is_separator(t: T) -> bool {
    matches!(t, T::NEWLINE | T::SEMI | T::EOF | T::COMMENT)
}

/// The tokens that may trail the text, which an error at its end is placed before, Pele's
/// TRAILING.
fn is_trailing(t: T) -> bool {
    matches!(t, T::COMMENT | T::NEWLINE | T::SPACE | T::EOF)
}

const TEXT_TOKENS: &str = "'UNICODE_TEXT', 'TEXT', 'TAGSTART'";
const SEPARATORS: &str = "'COMMENT', 'SEMI', 'NEWLINE', 'EOF'";
const DIRECTIONS: [&str; 5] = ["TB", "BT", "RL", "LR", "TD"];
const STATEMENT_START: &str = "'COMMENT', 'SEMI', 'NEWLINE', 'SPACE', 'EOF', 'flow', 'global', 'connector', 'acc_title', 'acc_descr', 'acc_descr_multiline_value', 'STYLE', 'LINKSTYLE', 'CLASSDEF', 'CLASS', 'CLICK', 'NODE_STRING'";

/// A block being read, `flow` or `global`, with the list and collect flag of the body it is
/// in, Pele's Frame. A connector's header is read into one too.
struct Frame {
    global: bool,
    /// The token of the keyword.
    at: usize,
    id: Option<FlowText>,
    title: Option<FlowText>,
    /// The metadata of the header, and the tokens it was read from.
    data: Option<String>,
    data_from: usize,
    data_to: usize,
    list: Vec<DocItem>,
    collect: bool,
}

/// source.replace(/}[^\S\n]*\n/g, '}\n'), as Mermaid trims the spaces and tabs between a
/// closing brace and the end of its line before parsing, with the edits it makes, as Pele's
/// replacements gives them.
fn trim_brace_trails(source: &str) -> (Cow<'_, str>, Vec<Edit>) {
    if !source.contains('}') {
        return (Cow::Borrowed(source), Vec::new());
    }
    let mut out = String::with_capacity(source.len());
    let mut edits = Vec::new();
    let (mut last, mut i) = (0, 0);
    while let Some(n) = source[i..].find('}') {
        let at = i + n;
        let run = at
            + 1
            + source[at + 1..]
                .chars()
                .take_while(|&c| c != '\n' && is_space_char(c))
                .map(char::len_utf8)
                .sum::<usize>();
        if source.as_bytes().get(run) == Some(&b'\n') {
            let end = run + 1;
            out.push_str(&source[last..at]);
            out.push_str("}\n");
            edits.push(Edit {
                from: at,
                to: end,
                text: "}\n".into(),
            });
            (last, i) = (end, end);
        } else {
            i = at + 1;
        }
    }
    out.push_str(&source[last..]);
    (Cow::Owned(out), edits)
}

struct Parser<'a, 'd> {
    /// The text the parser was given, which its errors are located in.
    source: &'a str,
    /// The text that was read, source with its brace trails trimmed.
    src: &'a str,
    toks: Vec<Tok<'a>>,
    /// The edits that made the text read of source, where it is not source.
    edits: Option<Vec<Edit>>,
    i: usize,
    pending_data: Option<String>,
    /// The first and last tokens of the metadata last read, Pele's dataFrom and dataTo.
    data_from: usize,
    data_to: usize,
    db: &'d mut AgentflowDb,
}

/// Parses an agent flow's text into db, as Pele's parseAgentflow does.
pub(crate) fn parse_agentflow(source: &str, db: &mut AgentflowDb) -> Result<(), Error> {
    let (src, edits) = trim_brace_trails(source);
    let changed = *src != *source;
    let toks = tokenize(&src);
    let mut p = Parser {
        source,
        src: &src,
        toks,
        edits: changed.then_some(edits),
        i: 0,
        pending_data: None,
        data_from: 0,
        data_to: 0,
        db,
    };
    p.parse()
}

impl Parser<'_, '_> {
    fn kind(&self) -> T {
        self.kind_at(self.i)
    }

    /// The type of the token at i, END past the last.
    fn kind_at(&self, i: usize) -> T {
        self.toks.get(i).map_or(T::END, |t| t.kind)
    }

    /// The text of the token at i, and moves past it.
    fn take(&mut self) -> String {
        let s = self.toks[self.i].text.to_string();
        self.i += 1;
        s
    }

    fn fail(&self, expected: &str) -> Error {
        let tok = &self.toks[self.i];
        let mut from = tok.start;
        if matches!(tok.kind, T::EOF | T::END) {
            // Comments are tokens here, so trailing ones are skipped.
            let mut k = self.i;
            while k > 0 && is_trailing(self.toks[k - 1].kind) {
                k -= 1;
            }
            from = trim_end(&self.src[..self.toks[k].start]).len();
        }
        let at = match &self.edits {
            None => from,
            Some(edits) => input_offset(std::slice::from_ref(edits), from),
        };
        if tok.kind == T::ERROR {
            return syntax_error(DiagramType::AgentFlow, self.source, at, "", true);
        }
        let detail = format!("Expecting {expected}, got '{}'", tok.kind.name());
        syntax_error(DiagramType::AgentFlow, self.source, at, &detail, false)
    }

    /// The span from the start of token a to the end of token b, Pele's span.
    fn span(&self, a: usize, b: usize) -> Loc {
        let (a, b) = (&self.toks[a].loc, &self.toks[b].loc);
        Loc {
            first_line: a[0],
            first_column: a[1],
            last_line: b[2],
            last_column: b[3],
        }
    }

    fn expect(&mut self, kind: T, also: &str) -> Result<String, Error> {
        if self.kind() != kind {
            let sep = if also.is_empty() { "" } else { ", " };
            return Err(self.fail(&format!("'{}'{sep}{also}", kind.name())));
        }
        Ok(self.take())
    }

    fn separator(&mut self) -> Result<(), Error> {
        if !is_separator(self.kind()) {
            return Err(self.fail(SEPARATORS));
        }
        self.i += 1;
        Ok(())
    }

    /// The text of a run of tokens that in is true of, at least one.
    fn run_of(&mut self, is: fn(T) -> bool, expected: &str) -> Result<String, Error> {
        if !is(self.kind()) {
            return Err(self.fail(expected));
        }
        let mut s = self.take();
        while is(self.kind()) {
            s.push_str(&self.toks[self.i].text);
            self.i += 1;
        }
        Ok(s)
    }

    fn id_string(&mut self) -> Result<String, Error> {
        self.run_of(is_id, "'NODE_STRING'")
    }

    fn alpha_num(&mut self) -> Result<String, Error> {
        self.run_of(is_alpha_num, "'NODE_STRING'")
    }

    /// A label's text, as Pele's text reads it.
    fn text(&mut self) -> Result<FlowText, Error> {
        let kind = match self.kind() {
            T::STR => LabelType::String,
            T::MD_STR => LabelType::Markdown,
            T::TEXT | T::TAGSTART | T::TAGEND | T::UNICODE_TEXT => LabelType::Text,
            _ => return Err(self.fail(&format!("{TEXT_TOKENS}, 'TAGEND', 'STR', 'MD_STR'"))),
        };
        let mut s = self.take();
        while matches!(
            self.kind(),
            T::TEXT | T::TAGSTART | T::TAGEND | T::UNICODE_TEXT
        ) {
            s.push_str(&self.toks[self.i].text);
            self.i += 1;
        }
        Ok(FlowText { text: s, kind })
    }

    fn closed(&mut self, kind: T) -> Result<(), Error> {
        self.expect(kind, TEXT_TOKENS).map(drop)
    }

    fn edge_text(&mut self) -> Result<FlowText, Error> {
        let kind = match self.kind() {
            T::STR => LabelType::String,
            T::MD_STR => LabelType::Markdown,
            T::EDGE_TEXT | T::UNICODE_TEXT => LabelType::Text,
            _ => return Err(self.fail("'EDGE_TEXT', 'STR', 'MD_STR'")),
        };
        let mut s = self.take();
        while matches!(self.kind(), T::EDGE_TEXT | T::UNICODE_TEXT) {
            s.push_str(&self.toks[self.i].text);
            self.i += 1;
        }
        Ok(FlowText { text: s, kind })
    }

    fn text_no_tags(&mut self) -> Result<FlowText, Error> {
        let t = self.kind();
        let kind = match t {
            T::STR => LabelType::Text,
            T::MD_STR => LabelType::Markdown,
            _ if is_no_tags(t) => LabelType::Text,
            _ => return Err(self.fail("'NODE_STRING', 'STR', 'MD_STR'")),
        };
        let mut s = self.take();
        while is_no_tags(self.kind()) {
            s.push_str(&self.toks[self.i].text);
            self.i += 1;
        }
        Ok(FlowText { text: s, kind })
    }

    fn shape_data(&mut self) -> String {
        self.data_from = self.i;
        let mut s = self.take();
        while self.kind() == T::SHAPE_DATA {
            s.push_str(&self.toks[self.i].text);
            self.i += 1;
        }
        self.data_to = self.i - 1;
        s
    }

    /// Gives a node the metadata read from the tokens from..=to, and widens its span over
    /// them, as Pele's attach does.
    fn attach(&mut self, id: &str, data: &str, from: usize, to: usize) -> Result<(), Error> {
        let loc = self.span(from, to);
        self.db
            .add_vertex(id, None, None, None, None, Some(data), Some(loc))?;
        self.db.extend_vertex_mapping(id, loc);
        Ok(())
    }

    /// Gives a flow or a connector the metadata of its header, as Pele's attachData does.
    fn attach_data(&mut self, id: &str, frame: &Frame) -> Result<(), Error> {
        let Some(data) = &frame.data else {
            return Ok(());
        };
        let loc = self.span(frame.data_from, frame.data_to);
        self.db
            .add_vertex(id, None, None, None, None, Some(data), Some(loc))
    }

    /// A label between brackets, and its closing bracket.
    fn label(&mut self, close: T) -> Result<FlowText, Error> {
        self.i += 1;
        let label = self.text()?;
        self.closed(close)?;
        Ok(label)
    }

    fn vertex(&mut self) -> Result<String, Error> {
        let from = self.i;
        let id = self.id_string()?;
        let mut props = None;
        let (label, kind) = match self.kind() {
            T::SQS => (self.label(T::SQE)?, "square"),
            T::DOUBLECIRCLESTART => (self.label(T::DOUBLECIRCLEEND)?, "doublecircle"),
            T::PS => {
                self.i += 1;
                if self.kind() == T::PS {
                    let label = self.label(T::PE)?;
                    self.expect(T::PE, "")?;
                    (label, "circle")
                } else {
                    let label = self.text()?;
                    self.closed(T::PE)?;
                    (label, "round")
                }
            }
            T::ELLIPSE_START => (self.label(T::ELLIPSE_END)?, "ellipse"),
            T::STADIUMSTART => (self.label(T::STADIUMEND)?, "stadium"),
            T::SUBROUTINESTART => (self.label(T::SUBROUTINEEND)?, "subroutine"),
            T::VERTEX_WITH_PROPS_START => {
                self.i += 1;
                let field = self.expect(T::NODE_STRING, "")?;
                self.expect(T::COLON, "")?;
                let value = self.expect(T::NODE_STRING, "")?;
                self.expect(T::PIPE, "")?;
                let label = self.text()?;
                self.closed(T::SQE)?;
                let mut m = Map::new();
                m.insert(field, Value::String(value));
                props = Some(m);
                (label, "rect")
            }
            T::CYLINDERSTART => (self.label(T::CYLINDEREND)?, "cylinder"),
            T::DIAMOND_START => {
                self.i += 1;
                if self.kind() == T::DIAMOND_START {
                    let label = self.label(T::DIAMOND_STOP)?;
                    self.expect(T::DIAMOND_STOP, "")?;
                    (label, "hexagon")
                } else {
                    let label = self.text()?;
                    self.closed(T::DIAMOND_STOP)?;
                    (label, "diamond")
                }
            }
            T::TAGEND => (self.label(T::SQE)?, "odd"),
            T::TRAPSTART => {
                self.i += 1;
                let label = self.text()?;
                if self.kind() == T::INVTRAPEND {
                    self.i += 1;
                    (label, "lean_right")
                } else {
                    self.closed(T::TRAPEND)?;
                    (label, "trapezoid")
                }
            }
            T::INVTRAPSTART => {
                self.i += 1;
                let label = self.text()?;
                if self.kind() == T::TRAPEND {
                    self.i += 1;
                    (label, "lean_left")
                } else {
                    self.closed(T::INVTRAPEND)?;
                    (label, "inv_trapezoid")
                }
            }
            _ => {
                self.db
                    .add_vertex(&id, None, None, None, None, None, None)?;
                let loc = self.span(from, self.i - 1);
                self.db.add_vertex_mapping(&id, false, loc);
                return Ok(id);
            }
        };
        self.db
            .add_vertex(&id, Some(&label), Some(kind), None, props, None, None)?;
        let loc = self.span(from, self.i - 1);
        self.db.add_vertex_mapping(&id, true, loc);
        Ok(id)
    }

    fn styled_vertex(&mut self) -> Result<String, Error> {
        let id = self.vertex()?;
        if self.kind() == T::STYLE_SEPARATOR {
            self.i += 1;
            let class = self.id_string()?;
            self.db.set_class(&id, &class);
        }
        Ok(id)
    }

    fn skip_spaces(&mut self) {
        while self.kind() == T::SPACE {
            self.i += 1;
        }
    }

    /// The nodes of `a & b`, each with its metadata, as Pele's node reads them. The metadata
    /// of the last is left in pending_data, and the tokens it was read from in data_from and
    /// data_to.
    fn node(&mut self) -> Result<Vec<String>, Error> {
        let mut ids = vec![self.styled_vertex()?];
        self.pending_data = None;
        loop {
            let data = (self.kind() == T::SHAPE_DATA).then(|| self.shape_data());
            let (from, to) = (self.data_from, self.data_to);
            if self.kind() != T::SPACE {
                self.pending_data = data;
                return Ok(ids);
            }
            self.skip_spaces();
            if self.kind() != T::AMP {
                if data.is_some() {
                    return Err(self.fail("'AMP'"));
                }
                return Ok(ids);
            }
            self.i += 1;
            if self.kind() != T::SPACE {
                return Err(self.fail("'SPACE'"));
            }
            self.skip_spaces();
            let next = self.styled_vertex()?;
            if let Some(data) = &data {
                self.attach(&ids[ids.len() - 1], data, from, to)?;
            }
            ids.push(next);
        }
    }

    fn link(&mut self) -> Result<LinkInfo, Error> {
        let id = (self.kind() == T::LINK_ID).then(|| self.take());
        match self.kind() {
            T::LINK => {
                let text = self.take();
                let mut info = self.db.destruct_link(&text, None);
                if id.is_some() {
                    info.id = id;
                }
                if self.kind() == T::PIPE {
                    self.i += 1;
                    info.text = Some(self.text()?);
                    self.closed(T::PIPE)?;
                    if self.kind() == T::SPACE {
                        self.i += 1;
                    }
                }
                Ok(info)
            }
            T::START_LINK => {
                let start = self.take();
                let label = self.edge_text()?;
                let end = self.expect(T::LINK, "'EDGE_TEXT'")?;
                let mut info = self.db.destruct_link(&end, Some(&start));
                info.text = Some(label);
                if id.is_some() {
                    info.id = id;
                }
                Ok(info)
            }
            _ => Err(self.fail("'LINK', 'START_LINK'")),
        }
    }

    /// Attaches the metadata node left pending to the last of nodes.
    fn attach_pending(&mut self, nodes: &[String]) -> Result<(), Error> {
        if let Some(data) = self.pending_data.take() {
            let (from, to) = (self.data_from, self.data_to);
            self.attach(&nodes[nodes.len() - 1], &data, from, to)?;
        }
        Ok(())
    }

    fn vertex_statement(&mut self, collect: bool) -> Result<Vec<String>, Error> {
        let from = self.i;
        let mut stmt = self.node()?;
        self.attach_pending(&stmt)?;
        let mut nodes = stmt.clone();
        loop {
            if !matches!(self.kind(), T::LINK | T::START_LINK | T::LINK_ID) {
                return Ok(nodes);
            }
            let info = self.link()?;
            let next = self.node()?;
            self.attach_pending(&next)?;
            self.db.add_link(&stmt, &next, &info)?;
            let loc = self.span(from, self.i - 1);
            self.db.add_edge_mapping(&next, loc);
            if collect {
                nodes.splice(0..0, next.iter().cloned());
            }
            stmt = next;
        }
    }

    fn style(&mut self) -> Result<String, Error> {
        self.run_of(is_style_part, "'NODE_STRING', 'NUM', 'COLON', 'BRKT'")
    }

    fn styles_opt(&mut self) -> Result<Vec<String>, Error> {
        let mut out = vec![self.style()?];
        while self.kind() == T::COMMA {
            self.i += 1;
            out.push(self.style()?);
        }
        Ok(out)
    }

    fn link_style_statement(&mut self) -> Result<(), Error> {
        self.i += 1;
        self.expect(T::SPACE, "")?;
        let mut positions;
        if self.kind() == T::DEFAULT {
            positions = vec![self.take()];
        } else {
            positions = vec![self.expect(T::NUM, "'DEFAULT'")?];
            while self.kind() == T::COMMA {
                self.i += 1;
                positions.push(self.expect(T::NUM, "")?);
            }
        }
        self.expect(T::SPACE, "")?;
        if self.kind() != T::INTERPOLATE {
            let styles = self.styles_opt()?;
            return self.db.update_link(&positions, styles);
        }
        self.i += 1;
        self.expect(T::SPACE, "")?;
        let interpolate = self.alpha_num()?;
        if self.kind() == T::SPACE {
            self.i += 1;
            let styles = self.styles_opt()?;
            self.db.update_link_interpolate(&positions, &interpolate)?;
            self.db.update_link(&positions, styles)
        } else {
            self.db.update_link_interpolate(&positions, &interpolate)
        }
    }

    fn link_tail(&mut self, id: &str, url: &str) -> Result<(), Error> {
        if self.kind() != T::SPACE {
            self.db.set_link(id, url, None);
            return Ok(());
        }
        self.i += 1;
        if self.kind() == T::LINK_TARGET {
            let target = self.take();
            self.db.set_link(id, url, Some(&target));
            return Ok(());
        }
        let tooltip = self.expect(T::STR, "'LINK_TARGET'")?;
        if self.kind() == T::SPACE {
            self.i += 1;
            let target = self.expect(T::LINK_TARGET, "")?;
            self.db.set_link(id, url, Some(&target));
        } else {
            self.db.set_link(id, url, None);
        }
        self.db.set_tooltip(id, &tooltip);
        Ok(())
    }

    fn click_statement(&mut self) -> Result<(), Error> {
        let id = self.take();
        match self.kind() {
            T::CALLBACKNAME => {
                self.i += 1;
                if self.kind() == T::CALLBACKARGS {
                    self.i += 1;
                }
                let mut tooltip = None;
                if self.kind() == T::SPACE {
                    self.i += 1;
                    tooltip = Some(self.expect(T::STR, "")?);
                }
                self.db.set_click_event(&id);
                if let Some(tooltip) = tooltip {
                    self.db.set_tooltip(&id, &tooltip);
                }
            }
            T::HREF => {
                self.i += 1;
                let url = self.expect(T::STR, "")?;
                self.link_tail(&id, &url)?;
            }
            T::STR => {
                let url = self.take();
                self.link_tail(&id, &url)?;
            }
            _ => {
                self.alpha_num()?;
                if self.kind() == T::SPACE {
                    self.i += 1;
                    let tooltip = self.expect(T::STR, "")?;
                    self.db.set_click_event(&id);
                    self.db.set_tooltip(&id, &tooltip);
                } else {
                    self.db.set_click_event(&id);
                }
            }
        }
        Ok(())
    }

    /// The header of a `flow` or a `connector` whose keyword is the token at, up to its
    /// separator: an id, an optional bracketed title, and optional metadata, as Pele's
    /// header reads it.
    fn header(&mut self, at: usize) -> Result<Frame, Error> {
        self.expect(T::SPACE, SEPARATORS)?;
        let id = self.text_no_tags()?;
        let mut title = FlowText {
            text: String::new(),
            kind: LabelType::Text,
        };
        if self.kind() == T::SQS {
            self.i += 1;
            title = self.text()?;
            self.closed(T::SQE)?;
        }
        let data = (self.kind() == T::SHAPE_DATA).then(|| self.shape_data());
        if !is_separator(self.kind()) {
            return Err(self.fail(&format!("{SEPARATORS}, 'SHAPE_DATA'")));
        }
        Ok(Frame {
            global: false,
            at,
            id: Some(id),
            title: Some(title),
            data,
            data_from: self.data_from,
            data_to: self.data_to,
            list: Vec::new(),
            collect: false,
        })
    }

    fn statement(&mut self, collect: bool) -> Result<DocItem, Error> {
        let none = || DocItem::Nodes(Vec::new());
        let t = self.kind();
        match t {
            T::STYLE => {
                self.i += 1;
                self.expect(T::SPACE, "")?;
                let id = self.id_string()?;
                self.expect(T::SPACE, "")?;
                let styles = self.styles_opt()?;
                self.db
                    .add_vertex(&id, None, None, Some(styles), None, None, None)?;
                self.separator()?;
                Ok(none())
            }
            T::LINKSTYLE => {
                self.link_style_statement()?;
                self.separator()?;
                Ok(none())
            }
            T::CLASSDEF => {
                self.i += 1;
                self.expect(T::SPACE, "")?;
                let id = self.id_string()?;
                self.expect(T::SPACE, "")?;
                let styles = self.styles_opt()?;
                self.db.add_class(&id, &styles);
                self.separator()?;
                Ok(none())
            }
            T::CLASS => {
                self.i += 1;
                self.expect(T::SPACE, "")?;
                let ids = self.id_string()?;
                self.expect(T::SPACE, "")?;
                let class = self.id_string()?;
                self.db.set_class(&ids, &class);
                self.separator()?;
                Ok(none())
            }
            T::CLICK => {
                self.click_statement()?;
                self.separator()?;
                Ok(none())
            }
            T::connector => {
                let at = self.i;
                self.i += 1;
                let frame = self.header(at)?;
                let sep = self.i;
                self.i += 1;
                let (id_text, title) = (frame.id.as_ref().unwrap(), frame.title.as_ref().unwrap());
                let id = self.db.add_connector(id_text, title);
                self.attach_data(&id, &frame)?;
                let (start, end) = (self.span(frame.at, frame.at), self.span(sep, sep));
                self.db.add_connector_mapping(id_text, start, end);
                Ok(DocItem::Text(id))
            }
            _ if DIRECTION_TOKENS.contains(&t) => {
                self.i += 1;
                let at = DIRECTION_TOKENS
                    .iter()
                    .position(|&d| d == t)
                    .expect("a direction");
                Ok(DocItem::Dir(DIRECTIONS[at]))
            }
            T::acc_title => {
                self.i += 1;
                let value = trim(&self.expect(T::acc_title_value, "")?).to_string();
                self.db.set_acc_title(&value);
                Ok(DocItem::Text(value))
            }
            T::acc_descr => {
                self.i += 1;
                let value = trim(&self.expect(T::acc_descr_value, "")?).to_string();
                self.db.set_acc_description(&value);
                Ok(DocItem::Text(value))
            }
            T::acc_descr_multiline_value => {
                let value = trim(&self.take()).to_string();
                self.db.set_acc_description(&value);
                Ok(DocItem::Text(value))
            }
            _ => {
                if !is_id(t) {
                    return Err(self.fail(STATEMENT_START));
                }
                let nodes = self.vertex_statement(collect)?;
                self.separator()?;
                Ok(DocItem::Nodes(nodes))
            }
        }
    }

    /// The statements, with blocks nested without recursion: each open one keeps the list
    /// and collect flag of its parent, as Pele's document reads them.
    fn document(&mut self) -> Result<(), Error> {
        let mut open: Vec<Frame> = Vec::new();
        let mut list: Vec<DocItem> = Vec::new();
        let mut collect = false;
        loop {
            let t = self.kind();
            if t == T::END {
                break;
            }
            if t == T::end {
                let Some(frame) = open.pop() else { break };
                let at = self.i;
                self.i += 1;
                if frame.global {
                    self.db.add_global(&list);
                    list = frame.list;
                    collect = frame.collect;
                    continue;
                }
                let id = self
                    .db
                    .add_sub_graph(frame.id.as_ref(), &list, frame.title.as_ref());
                self.attach_data(&id, &frame)?;
                let (start, end) = (self.span(frame.at, frame.at), self.span(at, at));
                self.db.add_subgraph_mapping(frame.id.as_ref(), start, end);
                list = frame.list;
                collect = frame.collect;
                if collect {
                    list.push(DocItem::Text(id));
                }
            } else if matches!(t, T::SEMI | T::NEWLINE | T::SPACE | T::EOF) {
                if collect {
                    list.push(DocItem::Text(self.toks[self.i].text.to_string()));
                }
                self.i += 1;
            } else if t == T::COMMENT {
                self.i += 1;
            } else if t == T::flow || t == T::global {
                let at = self.i;
                self.i += 1;
                let mut frame;
                if t == T::global || is_separator(self.kind()) {
                    frame = Frame {
                        global: t == T::global,
                        at,
                        id: None,
                        title: None,
                        data: None,
                        data_from: 0,
                        data_to: 0,
                        list: Vec::new(),
                        collect: false,
                    };
                    self.separator()?;
                } else {
                    frame = self.header(at)?;
                    self.i += 1;
                }
                frame.list = std::mem::take(&mut list);
                frame.collect = collect;
                open.push(frame);
                collect = true;
            } else {
                let item = self.statement(collect)?;
                if collect && !matches!(&item, DocItem::Nodes(n) if n.is_empty()) {
                    list.push(item);
                }
            }
        }
        if !open.is_empty() {
            return Err(self.fail("'end'"));
        }
        Ok(())
    }

    fn parse(&mut self) -> Result<(), Error> {
        while matches!(self.kind(), T::SPACE | T::NEWLINE | T::COMMENT) {
            self.i += 1;
        }
        self.expect(T::GRAPH, "'SPACE', 'NEWLINE', 'COMMENT'")?;
        if self.kind() == T::NODIR {
            self.i += 1;
            self.db.set_direction("TB");
        } else {
            let dir = self.expect(T::DIR, "'NODIR'")?;
            let t = self.kind();
            if t == T::SPACE {
                self.skip_spaces();
                if self.kind() != T::NEWLINE {
                    return Err(self.fail("'NEWLINE', 'SPACE'"));
                }
            } else if !matches!(t, T::SEMI | T::NEWLINE | T::COMMENT) {
                return Err(self.fail("'COMMENT', 'SEMI', 'NEWLINE', 'SPACE'"));
            }
            self.i += 1;
            self.db.set_direction(&dir);
        }
        self.document()?;
        if self.kind() != T::END {
            return Err(self.fail(STATEMENT_START));
        }
        Ok(())
    }
}
