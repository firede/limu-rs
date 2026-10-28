//! The parser of a flowchart, as Pele's parser.ts has it: recursive descent over the tokens
//! of its lexer, agreeing with Mermaid's flow.jison, its errors worded as Jison words them.

use std::borrow::Cow;

use super::db::{DocItem, FlowDb, FlowText, LabelType, LinkInfo};
use super::lexer::{Tok, tokenize};
use super::tokens::{DIRECTION_TOKENS, T};
use crate::detect::DiagramType;
use crate::errors::{Edit, Error, input_offset, syntax_error};
use crate::js::chars::{space_run, trim};
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

/// The tokens a subgraph's id is made of, Pele's NO_TAGS.
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
            | T::subgraph
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

const TEXT_TOKENS: &str = "'UNICODE_TEXT', 'TEXT', 'TAGSTART'";
const DIRECTIONS: [&str; 5] = ["TB", "BT", "RL", "LR", "TD"];
const STATEMENT_START: &str = "'SEMI', 'NEWLINE', 'SPACE', 'EOF', 'subgraph', 'acc_title', 'acc_descr', 'acc_descr_multiline_value', 'STYLE', 'LINKSTYLE', 'CLASSDEF', 'CLASS', 'CLICK', 'NODE_STRING'";

/// A subgraph being read, with the list and collect flag of the body it is in, Pele's Frame.
struct Frame {
    id: Option<FlowText>,
    title: Option<FlowText>,
    /// Whether the title is the id itself, as no [title] was written.
    title_is_id: bool,
    list: Vec<DocItem>,
    collect: bool,
}

/// source.replace(/}\s*\n/g, '}\n'), as Mermaid trims the white space between a closing
/// brace and the end of its line before parsing, with the edits it makes, as Pele's
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
        let run = space_run(source, at + 1);
        // \s* takes the whole run and backs off to the last line feed in it.
        match source[at + 1..run].rfind('\n') {
            Some(nl) => {
                let end = at + 1 + nl + 1;
                out.push_str(&source[last..at]);
                out.push_str("}\n");
                edits.push(Edit {
                    from: at,
                    to: end,
                    text: "}\n".into(),
                });
                (last, i) = (end, end);
            }
            None => i = at + 1,
        }
    }
    out.push_str(&source[last..]);
    (Cow::Owned(out), edits)
}

struct Parser<'a, 'd> {
    /// The text the parser was given, which its errors are located in.
    source: &'a str,
    toks: Vec<Tok<'a>>,
    /// The edits that made the text read of source, where it is not source.
    edits: Option<Vec<Edit>>,
    i: usize,
    pending_data: Option<String>,
    db: &'d mut FlowDb,
}

/// Parses a flowchart's text into db, as Pele's parseFlowchart does.
pub(crate) fn parse_flowchart(source: &str, db: &mut FlowDb) -> Result<(), Error> {
    let (src, edits) = trim_brace_trails(source);
    let changed = *src != *source;
    let toks = tokenize(&src);
    let mut p = Parser {
        source,
        toks,
        edits: changed.then_some(edits),
        i: 0,
        pending_data: None,
        db,
    };
    p.parse()
}

impl Parser<'_, '_> {
    fn kind(&self) -> T {
        self.toks[self.i].kind
    }

    fn kind_at(&self, i: usize) -> T {
        self.toks[i].kind
    }

    /// The text of the token at i, and moves past it.
    fn take(&mut self) -> String {
        let s = self.toks[self.i].text.to_string();
        self.i += 1;
        s
    }

    fn fail(&self, expected: &str) -> Error {
        let tok = &self.toks[self.i];
        let at = match &self.edits {
            None => tok.start,
            Some(edits) => input_offset(std::slice::from_ref(edits), tok.start),
        };
        if tok.kind == T::ERROR {
            return syntax_error(DiagramType::Flowchart, self.source, at, "", true);
        }
        let detail = format!("Expecting {expected}, got '{}'", tok.kind.name());
        syntax_error(DiagramType::Flowchart, self.source, at, &detail, false)
    }

    fn expect(&mut self, kind: T, also: &str) -> Result<String, Error> {
        if self.kind() != kind {
            let sep = if also.is_empty() { "" } else { ", " };
            return Err(self.fail(&format!("'{}'{sep}{also}", kind.name())));
        }
        Ok(self.take())
    }

    fn separator(&mut self) -> Result<(), Error> {
        if !matches!(self.kind(), T::NEWLINE | T::SEMI | T::EOF) {
            return Err(self.fail("'SEMI', 'NEWLINE', 'EOF'"));
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
        let mut s = self.take();
        while self.kind() == T::SHAPE_DATA {
            s.push_str(&self.toks[self.i].text);
            self.i += 1;
        }
        s
    }

    /// A label between brackets, and its closing bracket.
    fn label(&mut self, close: T) -> Result<FlowText, Error> {
        self.i += 1;
        let label = self.text()?;
        self.closed(close)?;
        Ok(label)
    }

    fn add(&mut self, id: &str, label: &FlowText, kind: &str) -> Result<(), Error> {
        self.db
            .add_vertex(id, Some(label), Some(kind), None, None, None)
    }

    fn vertex(&mut self) -> Result<String, Error> {
        let id = self.id_string()?;
        match self.kind() {
            T::SQS => {
                let label = self.label(T::SQE)?;
                self.add(&id, &label, "square")?;
            }
            T::DOUBLECIRCLESTART => {
                let label = self.label(T::DOUBLECIRCLEEND)?;
                self.add(&id, &label, "doublecircle")?;
            }
            T::PS => {
                self.i += 1;
                if self.kind() == T::PS {
                    let label = self.label(T::PE)?;
                    self.expect(T::PE, "")?;
                    self.add(&id, &label, "circle")?;
                } else {
                    let label = self.text()?;
                    self.closed(T::PE)?;
                    self.add(&id, &label, "round")?;
                }
            }
            T::ELLIPSE_START => {
                let label = self.label(T::ELLIPSE_END)?;
                self.add(&id, &label, "ellipse")?;
            }
            T::STADIUMSTART => {
                let label = self.label(T::STADIUMEND)?;
                self.add(&id, &label, "stadium")?;
            }
            T::SUBROUTINESTART => {
                let label = self.label(T::SUBROUTINEEND)?;
                self.add(&id, &label, "subroutine")?;
            }
            T::VERTEX_WITH_PROPS_START => {
                self.i += 1;
                let field = self.expect(T::NODE_STRING, "")?;
                self.expect(T::COLON, "")?;
                let value = self.expect(T::NODE_STRING, "")?;
                self.expect(T::PIPE, "")?;
                let label = self.text()?;
                self.closed(T::SQE)?;
                let mut props = Map::new();
                props.insert(field, Value::String(value));
                self.db
                    .add_vertex(&id, Some(&label), Some("rect"), None, Some(props), None)?;
            }
            T::CYLINDERSTART => {
                let label = self.label(T::CYLINDEREND)?;
                self.add(&id, &label, "cylinder")?;
            }
            T::DIAMOND_START => {
                self.i += 1;
                if self.kind() == T::DIAMOND_START {
                    let label = self.label(T::DIAMOND_STOP)?;
                    self.expect(T::DIAMOND_STOP, "")?;
                    self.add(&id, &label, "hexagon")?;
                } else {
                    let label = self.text()?;
                    self.closed(T::DIAMOND_STOP)?;
                    self.add(&id, &label, "diamond")?;
                }
            }
            T::TAGEND => {
                let label = self.label(T::SQE)?;
                self.add(&id, &label, "odd")?;
            }
            T::TRAPSTART => {
                self.i += 1;
                let label = self.text()?;
                if self.kind() == T::INVTRAPEND {
                    self.i += 1;
                    self.add(&id, &label, "lean_right")?;
                } else {
                    self.closed(T::TRAPEND)?;
                    self.add(&id, &label, "trapezoid")?;
                }
            }
            T::INVTRAPSTART => {
                self.i += 1;
                let label = self.text()?;
                if self.kind() == T::TRAPEND {
                    self.i += 1;
                    self.add(&id, &label, "lean_left")?;
                } else {
                    self.closed(T::INVTRAPEND)?;
                    self.add(&id, &label, "inv_trapezoid")?;
                }
            }
            _ => self.db.add_vertex(&id, None, None, None, None, None)?,
        }
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
    /// of the last is left in pending_data.
    fn node(&mut self) -> Result<Vec<String>, Error> {
        let mut ids = vec![self.styled_vertex()?];
        self.pending_data = None;
        loop {
            let data = (self.kind() == T::SHAPE_DATA).then(|| self.shape_data());
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
                self.add_data(&ids[ids.len() - 1], data)?;
            }
            ids.push(next);
        }
    }

    fn add_data(&mut self, id: &str, data: &str) -> Result<(), Error> {
        self.db.add_vertex(id, None, None, None, None, Some(data))
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

    fn vertex_statement(&mut self, collect: bool) -> Result<Vec<String>, Error> {
        let mut stmt = self.node()?;
        if let Some(data) = self.pending_data.take() {
            self.add_data(&stmt[stmt.len() - 1], &data)?;
        }
        let mut nodes = stmt.clone();
        loop {
            if !matches!(self.kind(), T::LINK | T::START_LINK | T::LINK_ID) {
                return Ok(nodes);
            }
            let info = self.link()?;
            let next = self.node()?;
            if let Some(data) = self.pending_data.take() {
                self.add_data(&next[next.len() - 1], &data)?;
            }
            self.db.add_link(&stmt, &next, &info)?;
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

    fn subgraph_header(&mut self) -> Result<Frame, Error> {
        self.i += 1;
        let frame = |id, title, title_is_id| Frame {
            id,
            title,
            title_is_id,
            list: Vec::new(),
            collect: false,
        };
        if matches!(self.kind(), T::NEWLINE | T::SEMI | T::EOF) {
            self.i += 1;
            return Ok(frame(None, None, false));
        }
        self.expect(T::SPACE, "'SEMI', 'NEWLINE', 'EOF'")?;
        let id = self.text_no_tags()?;
        let mut title = None;
        if self.kind() == T::SQS {
            self.i += 1;
            title = Some(self.text()?);
            self.closed(T::SQE)?;
        }
        if !matches!(self.kind(), T::NEWLINE | T::SEMI | T::EOF) {
            return Err(self.fail("'SEMI', 'NEWLINE', 'EOF', 'SQS'"));
        }
        self.i += 1;
        Ok(match title {
            Some(title) => frame(Some(id), Some(title), false),
            None => frame(Some(id.clone()), Some(id), true),
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
                    .add_vertex(&id, None, None, Some(styles), None, None)?;
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

    /// The statements, with subgraphs nested without recursion: each open one keeps the list
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
                self.i += 1;
                let id = self.db.add_sub_graph(
                    frame.id.as_ref(),
                    &list,
                    frame.title.as_ref(),
                    frame.title_is_id,
                );
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
            } else if t == T::subgraph {
                let mut frame = self.subgraph_header()?;
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
        while matches!(self.kind(), T::SPACE | T::NEWLINE) {
            self.i += 1;
        }
        self.expect(T::GRAPH, "'SPACE', 'NEWLINE'")?;
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
            } else if t != T::SEMI && t != T::NEWLINE {
                return Err(self.fail("'SEMI', 'NEWLINE', 'SPACE'"));
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
