//! The parser of a sequence diagram, as Pele's parser.ts has it: a loop over the tokens of
//! its lexer that agrees with Mermaid's sequenceDiagram.jison, its errors worded as Jison
//! words them. Blocks are read without recursion, and the statements they hold are applied to
//! the diagram once they are all read.

use super::db::{
    ACTIVE_END, ACTIVE_START, ALT_ELSE, ALT_END, ALT_START, AUTONUMBER, BIDIRECTIONAL_DOTTED,
    BIDIRECTIONAL_SOLID, BREAK_END, BREAK_START, CENTRAL_CONNECTION, CENTRAL_CONNECTION_DUAL,
    CENTRAL_CONNECTION_REVERSE, CRITICAL_END, CRITICAL_OPTION, CRITICAL_START, DOTTED,
    DOTTED_CROSS, DOTTED_OPEN, DOTTED_POINT, LEFTOF, LOOP_END, LOOP_START, NoteActor, OPT_END,
    OPT_START, OVER, PAR_AND, PAR_END, PAR_OVER_START, PAR_START, RECT_END, RECT_START, RIGHTOF,
    SOLID, SOLID_ARROW_BOTTOM_REVERSE, SOLID_ARROW_BOTTOM_REVERSE_DOTTED, SOLID_ARROW_TOP_REVERSE,
    SOLID_ARROW_TOP_REVERSE_DOTTED, SOLID_BOTTOM, SOLID_BOTTOM_DOTTED, SOLID_CROSS, SOLID_OPEN,
    SOLID_POINT, SOLID_TOP, SOLID_TOP_DOTTED, STICK_ARROW_BOTTOM_REVERSE,
    STICK_ARROW_BOTTOM_REVERSE_DOTTED, STICK_ARROW_TOP_REVERSE, STICK_ARROW_TOP_REVERSE_DOTTED,
    STICK_BOTTOM, STICK_BOTTOM_DOTTED, STICK_TOP, STICK_TOP_DOTTED, SeqDb, SeqItem, SeqStatement,
    release,
};
use super::lexer::{Tok, tokenize};
use super::tokens::T;
use crate::detect::DiagramType;
use crate::errors::{Error, syntax_error};
use crate::js::chars::trim;
use crate::js::number::number;

/// The kind of message each arrow token draws, Pele's SIGNAL.
fn signal_of(t: T) -> Option<i32> {
    Some(match t {
        T::SOLID_OPEN_ARROW => SOLID_OPEN,
        T::DOTTED_OPEN_ARROW => DOTTED_OPEN,
        T::SOLID_ARROW => SOLID,
        T::DOTTED_ARROW => DOTTED,
        T::BIDIRECTIONAL_SOLID_ARROW => BIDIRECTIONAL_SOLID,
        T::BIDIRECTIONAL_DOTTED_ARROW => BIDIRECTIONAL_DOTTED,
        T::SOLID_CROSS => SOLID_CROSS,
        T::DOTTED_CROSS => DOTTED_CROSS,
        T::SOLID_POINT => SOLID_POINT,
        T::DOTTED_POINT => DOTTED_POINT,
        T::SOLID_ARROW_TOP => SOLID_TOP,
        T::SOLID_ARROW_BOTTOM => SOLID_BOTTOM,
        T::STICK_ARROW_TOP => STICK_TOP,
        T::STICK_ARROW_BOTTOM => STICK_BOTTOM,
        T::SOLID_ARROW_TOP_DOTTED => SOLID_TOP_DOTTED,
        T::SOLID_ARROW_BOTTOM_DOTTED => SOLID_BOTTOM_DOTTED,
        T::STICK_ARROW_TOP_DOTTED => STICK_TOP_DOTTED,
        T::STICK_ARROW_BOTTOM_DOTTED => STICK_BOTTOM_DOTTED,
        T::SOLID_ARROW_TOP_REVERSE => SOLID_ARROW_TOP_REVERSE,
        T::SOLID_ARROW_BOTTOM_REVERSE => SOLID_ARROW_BOTTOM_REVERSE,
        T::STICK_ARROW_TOP_REVERSE => STICK_ARROW_TOP_REVERSE,
        T::STICK_ARROW_BOTTOM_REVERSE => STICK_ARROW_BOTTOM_REVERSE,
        T::SOLID_ARROW_TOP_REVERSE_DOTTED => SOLID_ARROW_TOP_REVERSE_DOTTED,
        T::SOLID_ARROW_BOTTOM_REVERSE_DOTTED => SOLID_ARROW_BOTTOM_REVERSE_DOTTED,
        T::STICK_ARROW_TOP_REVERSE_DOTTED => STICK_ARROW_TOP_REVERSE_DOTTED,
        T::STICK_ARROW_BOTTOM_REVERSE_DOTTED => STICK_ARROW_BOTTOM_REVERSE_DOTTED,
        _ => return None,
    })
}

/// The kinds of block, Pele's K.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum K {
    Top,
    Box,
    Loop,
    Rect,
    Opt,
    Break,
    Alt,
    Par,
    Critical,
}

/// The kind of block a token opens, Pele's BLOCK.
fn block_of(t: T) -> Option<K> {
    Some(match t {
        T::r#loop => K::Loop,
        T::rect => K::Rect,
        T::opt => K::Opt,
        T::r#break => K::Break,
        T::alt => K::Alt,
        T::par | T::par_over => K::Par,
        T::critical => K::Critical,
        T::r#box => K::Box,
        _ => return None,
    })
}

/// A block being read, Pele's Frame.
struct Frame {
    kind: K,
    text: String,
    start_type: i32,
    /// The current section's statements. Slot 0 of a block's first section is kept for its
    /// start entry.
    list: Vec<SeqItem>,
    /// Finished sections, each with the divider text that ended it.
    sections: Vec<(Vec<SeqItem>, String)>,
}

impl Drop for Frame {
    fn drop(&mut self) {
        release(std::mem::take(&mut self.list));
        for (list, _) in std::mem::take(&mut self.sections) {
            release(list);
        }
    }
}

const SIGNAL_TOKENS: &str = "'SOLID_OPEN_ARROW', 'DOTTED_OPEN_ARROW', 'SOLID_ARROW', 'DOTTED_ARROW', 'SOLID_CROSS', 'DOTTED_CROSS', 'SOLID_POINT', 'DOTTED_POINT'";

/// s.substring(n): s without its first n code units. A cut through a surrogate pair leaves
/// U+FFFD for the half JavaScript would keep.
fn substring(s: &str, n: usize) -> String {
    let mut left = n;
    for (i, c) in s.char_indices() {
        if left == 0 {
            return s[i..].to_string();
        }
        if c.len_utf16() > left {
            return format!("\u{fffd}{}", &s[i + c.len_utf8()..]);
        }
        left -= c.len_utf16();
    }
    String::new()
}

fn statement(kind: &'static str) -> SeqStatement {
    SeqStatement {
        kind,
        ..SeqStatement::default()
    }
}

fn item(s: SeqStatement) -> SeqItem {
    SeqItem::Statement(Box::new(s))
}

struct Parser<'a, 'd> {
    src: &'a str,
    toks: Vec<Tok<'a>>,
    i: usize,
    db: &'d mut SeqDb,
}

impl Parser<'_, '_> {
    fn kind(&self) -> T {
        self.toks[self.i].kind
    }

    fn fail(&self, expected: &str) -> Error {
        let t = &self.toks[self.i];
        if t.kind == T::ERROR {
            return syntax_error(DiagramType::Sequence, self.src, t.start, "", true);
        }
        let detail = format!("Expecting {expected}, got '{}'", t.kind.name());
        syntax_error(DiagramType::Sequence, self.src, t.start, &detail, false)
    }

    fn expect(&mut self, kind: T) -> Result<String, Error> {
        if self.kind() != kind {
            return Err(self.fail(&format!("'{}'", kind.name())));
        }
        self.i += 1;
        Ok(self.toks[self.i - 1].text.to_string())
    }

    fn actor(&mut self) -> Result<SeqStatement, Error> {
        let t = self.kind();
        if t != T::ACTOR && t != T::link {
            return Err(self.fail("'ACTOR'"));
        }
        self.i += 1;
        Ok(SeqStatement {
            actor: self.toks[self.i - 1].text.to_string(),
            ..statement("addParticipant")
        })
    }

    fn text2(&mut self) -> Result<super::db::SeqText, Error> {
        let text = self.expect(T::TXT)?;
        Ok(self.db.parse_message(&substring(trim(&text), 1)))
    }

    fn participant(&mut self) -> Result<SeqStatement, Error> {
        let t = self.kind();
        self.i += 1;
        if t == T::destroy {
            let mut node = self.actor()?;
            self.expect(T::NEWLINE)?;
            node.kind = "destroyParticipant";
            return Ok(node);
        }
        let mut node = self.actor()?;
        if self.kind() == T::CONFIG_START && self.toks[self.i - 1].kind == T::ACTOR {
            self.i += 1;
            let content = self.expect(T::CONFIG_CONTENT)?;
            self.expect(T::CONFIG_END)?;
            node.config = Some(trim(&content).to_string());
        }
        node.draw = Some(if t == T::participant {
            "participant"
        } else {
            "actor"
        });
        if self.kind() == T::AS {
            self.i += 1;
            let alias = self.expect(T::restOfLine)?;
            self.expect(T::NEWLINE)?;
            node.description = Some(self.db.parse_message(&alias));
        } else if self.kind() == T::NEWLINE {
            self.i += 1;
        } else {
            return Err(self.fail("'AS', 'NEWLINE'"));
        }
        Ok(node)
    }

    fn signal(&mut self, from: SeqStatement) -> Result<SeqItem, Error> {
        let mut central_from = false;
        if self.kind() == T::CENTRAL {
            central_from = true;
            self.i += 1;
        }
        let Some(signal_type) = signal_of(self.kind()) else {
            return Err(self.fail(SIGNAL_TOKENS));
        };
        self.i += 1;
        let mark = self.kind();
        if mark == T::CENTRAL || (!central_from && (mark == T::PLUS || mark == T::MINUS)) {
            self.i += 1;
        }
        let to = self.actor()?;
        let msg = self.text2()?;
        let mut message = SeqStatement {
            from: from.actor.clone(),
            to: to.actor.clone(),
            signal_type,
            text: Some(msg),
            ..statement("addMessage")
        };
        let mut after = Vec::new();
        let marker = |kind: &'static str, signal_type: i32, actor: &str| SeqStatement {
            signal_type,
            actor: actor.to_string(),
            ..statement(kind)
        };
        if central_from {
            let dual = mark == T::CENTRAL;
            message.activate = dual;
            message.central_connection = if dual {
                CENTRAL_CONNECTION_DUAL
            } else {
                CENTRAL_CONNECTION_REVERSE
            };
            if dual {
                after.push(marker("centralConnection", CENTRAL_CONNECTION, &to.actor));
            }
            after.push(marker(
                "centralConnectionReverse",
                CENTRAL_CONNECTION_REVERSE,
                &from.actor,
            ));
        } else if mark == T::PLUS {
            message.activate = true;
            after.push(marker("activeStart", ACTIVE_START, &to.actor));
        } else if mark == T::MINUS {
            after.push(marker("activeEnd", ACTIVE_END, &from.actor));
        } else if mark == T::CENTRAL {
            message.activate = true;
            message.central_connection = CENTRAL_CONNECTION;
            after.push(marker("centralConnection", CENTRAL_CONNECTION, &to.actor));
        }
        let mut out = vec![item(from), item(to), item(message)];
        out.extend(after.into_iter().map(item));
        Ok(SeqItem::List(out))
    }

    fn menu(&mut self, kind: &'static str) -> Result<SeqItem, Error> {
        self.i += 1;
        let who = self.actor()?;
        let node = SeqStatement {
            actor: who.actor.clone(),
            text: Some(self.text2()?),
            ..statement(kind)
        };
        Ok(SeqItem::List(vec![item(who), item(node)]))
    }

    fn note(&mut self) -> Result<SeqItem, Error> {
        self.i += 1;
        let t = self.kind();
        if t == T::over {
            self.i += 1;
            let first = self.actor()?;
            let (who, ids) = if self.kind() == T::COMMA {
                self.i += 1;
                let second = self.actor()?;
                let ids = NoteActor::Pair(first.actor.clone(), second.actor.clone());
                (SeqItem::List(vec![item(first), item(second)]), ids)
            } else {
                let ids = NoteActor::Pair(first.actor.clone(), first.actor.clone());
                (item(first), ids)
            };
            let node = SeqStatement {
                placement: OVER,
                note_actor: Some(ids),
                text: Some(self.text2()?),
                ..statement("addNote")
            };
            return Ok(SeqItem::List(vec![who, item(node)]));
        }
        if t != T::left_of && t != T::right_of {
            return Err(self.fail("'over', 'left_of', 'right_of'"));
        }
        self.i += 1;
        let who = self.actor()?;
        let node = SeqStatement {
            placement: if t == T::left_of { LEFTOF } else { RIGHTOF },
            note_actor: Some(NoteActor::One(who.actor.clone())),
            text: Some(self.text2()?),
            ..statement("addNote")
        };
        Ok(SeqItem::List(vec![item(who), item(node)]))
    }

    fn autonumber(&mut self) -> Result<SeqStatement, Error> {
        self.i += 1;
        let mut node = SeqStatement {
            sequence_visible: true,
            signal_type: AUTONUMBER,
            ..statement("sequenceIndex")
        };
        if self.kind() == T::NUM {
            node.sequence_index = Some(number(self.toks[self.i].text));
            self.i += 1;
            node.sequence_index_step = Some(if self.kind() == T::NUM {
                self.i += 1;
                number(self.toks[self.i - 1].text)
            } else {
                1.0
            });
        } else if self.kind() == T::off {
            self.i += 1;
            node.sequence_visible = false;
        }
        if self.kind() != T::NEWLINE {
            return Err(self.fail("'NEWLINE', 'NUM', 'off'"));
        }
        self.i += 1;
        Ok(node)
    }

    /// Folds the sections of an alt, par, or critical block from the last one back, as the
    /// grammar's right recursion does, so the texts reach the model builder in the same
    /// order, as Pele's fold does.
    fn fold(&self, frame: &mut Frame, divider: &'static str, signal_type: i32) -> Vec<SeqItem> {
        let mut list = std::mem::take(&mut frame.list);
        let sections = std::mem::take(&mut frame.sections);
        for (mut section, text) in sections.into_iter().rev() {
            let head = SeqStatement {
                signal_type,
                text: Some(self.db.parse_message(&text)),
                ..statement(divider)
            };
            section.push(item(head));
            section.push(SeqItem::List(list));
            list = section;
        }
        list
    }

    /// The statements of a block that has ended, between the entries that start and end it,
    /// as Pele's close gives them.
    fn close(&self, frame: &mut Frame) -> Vec<SeqItem> {
        let text = std::mem::take(&mut frame.text);
        let started = |kind: &'static str, signal_type: i32| SeqStatement {
            signal_type,
            text: Some(self.db.parse_message(&text)),
            ..statement(kind)
        };
        let ended = |kind: &'static str, signal_type: i32| SeqStatement {
            signal_type,
            ..statement(kind)
        };
        let (mut list, start, end) = match frame.kind {
            K::Box => (
                std::mem::take(&mut frame.list),
                SeqStatement {
                    box_data: self.db.parse_box_data(&text),
                    ..statement("boxStart")
                },
                statement("boxEnd"),
            ),
            K::Loop => (
                std::mem::take(&mut frame.list),
                started("loopStart", LOOP_START),
                ended("loopEnd", LOOP_END),
            ),
            K::Rect => (
                std::mem::take(&mut frame.list),
                started("rectStart", RECT_START),
                ended("rectEnd", RECT_END),
            ),
            K::Opt => (
                std::mem::take(&mut frame.list),
                started("optStart", OPT_START),
                ended("optEnd", OPT_END),
            ),
            K::Break => (
                std::mem::take(&mut frame.list),
                started("breakStart", BREAK_START),
                ended("breakEnd", BREAK_END),
            ),
            K::Alt => (
                self.fold(frame, "else", ALT_ELSE),
                started("altStart", ALT_START),
                ended("altEnd", ALT_END),
            ),
            K::Par => (
                self.fold(frame, "and", PAR_AND),
                started("parStart", frame.start_type),
                ended("parEnd", PAR_END),
            ),
            K::Critical | K::Top => (
                self.fold(frame, "option", CRITICAL_OPTION),
                started("criticalStart", CRITICAL_START),
                ended("criticalEnd", CRITICAL_END),
            ),
        };
        list[0] = item(start);
        list.push(item(end));
        list
    }
}

/// Reads the statements of a sequence diagram, as Pele's parseSequence does, and applies them
/// to db once they are all read.
pub(crate) fn parse_sequence(src: &str, db: &mut SeqDb) -> Result<(), Error> {
    let mut p = Parser {
        src,
        toks: tokenize(src),
        i: 0,
        db,
    };
    while p.kind() == T::NEWLINE {
        p.i += 1;
    }
    p.expect(T::SD)?;

    let mut stack: Vec<Frame> = Vec::new();
    let mut frame = Frame {
        kind: K::Top,
        text: String::new(),
        start_type: 0,
        list: Vec::new(),
        sections: Vec::new(),
    };
    loop {
        let t = p.kind();
        if frame.kind == K::Box {
            match t {
                T::participant | T::participant_actor | T::destroy => {
                    let node = p.participant()?;
                    frame.list.push(item(node));
                }
                T::NEWLINE => {
                    frame.list.push(SeqItem::List(Vec::new()));
                    p.i += 1;
                }
                T::end => {
                    p.i += 1;
                    let done = p.close(&mut frame);
                    frame = stack.pop().expect("a box is inside a block");
                    frame.list.push(SeqItem::List(done));
                }
                _ => {
                    return Err(
                        p.fail("'end', 'NEWLINE', 'participant', 'participant_actor', 'destroy'")
                    );
                }
            }
            continue;
        }
        match t {
            T::NEWLINE | T::INVALID => {
                frame.list.push(SeqItem::List(Vec::new()));
                p.i += 1;
            }
            T::participant | T::participant_actor | T::destroy => {
                let node = p.participant()?;
                frame.list.push(item(node));
            }
            T::create => {
                p.i += 1;
                let next = p.kind();
                if next != T::participant && next != T::participant_actor && next != T::destroy {
                    return Err(p.fail("'participant', 'participant_actor', 'destroy'"));
                }
                let mut node = p.participant()?;
                node.kind = "createParticipant";
                frame.list.push(item(node));
            }
            T::ACTOR => {
                let from = p.actor()?;
                let signal = p.signal(from)?;
                frame.list.push(signal);
                p.expect(T::NEWLINE)?;
            }
            T::link => {
                let next = p.toks[p.i + 1].kind;
                let read = if next == T::ACTOR || next == T::link {
                    p.menu("addALink")?
                } else {
                    let from = p.actor()?;
                    p.signal(from)?
                };
                frame.list.push(read);
                p.expect(T::NEWLINE)?;
            }
            T::links => {
                let read = p.menu("addLinks")?;
                frame.list.push(read);
                p.expect(T::NEWLINE)?;
            }
            T::properties => {
                let read = p.menu("addProperties")?;
                frame.list.push(read);
                p.expect(T::NEWLINE)?;
            }
            T::details => {
                let read = p.menu("addDetails")?;
                frame.list.push(read);
                p.expect(T::NEWLINE)?;
            }
            T::note => {
                let read = p.note()?;
                frame.list.push(read);
                p.expect(T::NEWLINE)?;
            }
            T::autonumber => {
                let node = p.autonumber()?;
                frame.list.push(item(node));
            }
            T::activate | T::deactivate => {
                p.i += 1;
                let who = p.actor()?;
                p.expect(T::NEWLINE)?;
                let node = if t == T::activate {
                    SeqStatement {
                        signal_type: ACTIVE_START,
                        actor: who.actor,
                        ..statement("activeStart")
                    }
                } else {
                    SeqStatement {
                        signal_type: ACTIVE_END,
                        actor: who.actor,
                        ..statement("activeEnd")
                    }
                };
                frame.list.push(item(node));
            }
            T::title | T::legacy_title => {
                let n = if t == T::title { 6 } else { 7 };
                let title = substring(p.toks[p.i].text, n);
                p.i += 1;
                p.db.set_diagram_title(&title);
                frame.list.push(SeqItem::Text(title));
            }
            T::acc_title => {
                p.i += 1;
                let value = trim(&p.expect(T::acc_title_value)?).to_string();
                p.db.set_acc_title(&value);
                frame.list.push(SeqItem::Text(value));
            }
            T::acc_descr => {
                p.i += 1;
                let value = trim(&p.expect(T::acc_descr_value)?).to_string();
                p.db.set_acc_description(&value);
                frame.list.push(SeqItem::Text(value));
            }
            T::acc_descr_multiline_value => {
                let value = trim(p.toks[p.i].text).to_string();
                p.i += 1;
                p.db.set_acc_description(&value);
                frame.list.push(SeqItem::Text(value));
            }
            T::r#loop
            | T::rect
            | T::opt
            | T::r#break
            | T::alt
            | T::par
            | T::par_over
            | T::critical
            | T::r#box => {
                p.i += 1;
                let text = p.expect(T::restOfLine)?;
                let opened = Frame {
                    kind: block_of(t).unwrap_or(K::Top),
                    text,
                    start_type: if t == T::par_over {
                        PAR_OVER_START
                    } else {
                        PAR_START
                    },
                    list: vec![SeqItem::List(Vec::new())],
                    sections: Vec::new(),
                };
                stack.push(std::mem::replace(&mut frame, opened));
            }
            T::r#else | T::and | T::option => {
                let want = match t {
                    T::r#else => K::Alt,
                    T::and => K::Par,
                    _ => K::Critical,
                };
                if frame.kind != want {
                    return Err(p.fail("'end'"));
                }
                p.i += 1;
                let text = p.expect(T::restOfLine)?;
                let list = std::mem::take(&mut frame.list);
                frame.sections.push((list, text));
            }
            T::end => {
                if frame.kind == K::Top {
                    return Err(p.fail("'NEWLINE', 'EOF'"));
                }
                p.i += 1;
                let done = p.close(&mut frame);
                frame = stack.pop().expect("a block is inside another");
                frame.list.push(SeqItem::List(done));
            }
            T::END => {
                if frame.kind != K::Top {
                    return Err(p.fail("'end'"));
                }
                return p.db.apply(&frame.list);
            }
            _ => return Err(p.fail("'NEWLINE', 'ACTOR', 'participant', 'note', 'end'")),
        }
    }
}
