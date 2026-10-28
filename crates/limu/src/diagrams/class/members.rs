//! The members of a class, as Pele's members.ts reads them: an attribute or a method, with
//! its visibility, its classifier, and a method's parameters and return type.

use crate::common::parse_generic_types;
use crate::js::chars::{is_line_end, is_space_char, trim, unit, unit_at};

/// Whether a member is an attribute or a method, Pele's memberType.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MemberType {
    Method,
    Attribute,
}

impl MemberType {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            MemberType::Method => "method",
            MemberType::Attribute => "attribute",
        }
    }
}

/// The visibility a member's first character gives it, as Pele's isVisibility reads it, or
/// none.
fn visibility(c: Option<u8>) -> Option<&'static str> {
    match c {
        Some(b'#') => Some("#"),
        Some(b'+') => Some("+"),
        Some(b'~') => Some("~"),
        Some(b'-') => Some("-"),
        _ => None,
    }
}

/// A member of a class, Pele's ClassMember. The visibility and the classifier are empty
/// where the member has none.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ClassMember {
    pub(crate) id: String,
    pub(crate) member_type: MemberType,
    pub(crate) visibility: &'static str,
    pub(crate) classifier: &'static str,
    pub(crate) parameters: String,
    pub(crate) return_type: String,
}

impl ClassMember {
    /// Reads a member as a method or an attribute, as Pele's constructor does. Mermaid
    /// throws a TypeError for a method with no opening parenthesis, such as `run)`. Pele
    /// keeps it as an attribute, so it is shown as written.
    pub(crate) fn new(input: &str, member_type: MemberType) -> ClassMember {
        let mut m = ClassMember {
            id: String::new(),
            member_type,
            visibility: "",
            classifier: "",
            parameters: String::new(),
            return_type: String::new(),
        };
        if member_type != MemberType::Method || !m.parse_method(input) {
            m.parse_attribute(input);
        }
        m.id = if m.id.starts_with(' ') {
            format!(" {}", trim(&m.id))
        } else {
            trim(&m.id).to_string()
        };
        m
    }

    /// Reads a method, as Pele's parseMethod does: the same split as Mermaid's
    /// /([#+~-])?(.+)\((.*)\)([\s$*])?(.*)([$*])?/, found in one pass, from the first line
    /// with an opening parenthesis after its first character and a closing one later. The
    /// parentheses, the visibilities and the classifiers are ASCII, so the work is done on
    /// bytes.
    fn parse_method(&mut self, input: &str) -> bool {
        let b = input.as_bytes();
        let n = b.len();
        let line_end = |mut q: usize| {
            while q < n && !is_line_end(unit(input, q)) {
                q += 1;
            }
            q
        };
        let mut start = 0;
        while start < n {
            let end = line_end(start);
            let mut close = end.saturating_sub(1);
            while close > start && b[close] != b')' {
                close -= 1;
            }
            let mut open = close.saturating_sub(1);
            while open > start && b[open] != b'(' {
                open -= 1;
            }
            if close > start && open > start {
                let first = visibility(b.get(start).copied());
                let visible = first.is_some() && open > start + 1;
                if visible {
                    self.visibility = first.unwrap_or("");
                }
                self.id = input[if visible { start + 1 } else { start }..open].to_string();
                self.parameters = trim(&input[open + 1..close]).to_string();
                let mut rest = close + 1;
                let mut classifier = "";
                if let Some(next) = input.get(rest..).and_then(|s| s.chars().next()) {
                    // A code unit that trims away is white space, which is the classifier
                    // of none.
                    if next == '$' || next == '*' || is_space_char(next) {
                        classifier = match next {
                            '$' => "$",
                            '*' => "*",
                            _ => "",
                        };
                        rest += next.len_utf8();
                    }
                }
                let stop = line_end(rest);
                self.return_type = trim(&input[rest..stop]).to_string();
                if classifier.is_empty() {
                    let last = self.return_type.as_bytes().last().copied();
                    if let Some(c @ (b'$' | b'*')) = last {
                        classifier = if c == b'$' { "$" } else { "*" };
                        self.return_type.pop();
                    }
                }
                self.classifier = classifier;
                return true;
            }
            // Past the line's end, which is one code unit, whatever its bytes.
            start = end + unit_at(input, end).1;
        }
        self.member_type = MemberType::Attribute;
        false
    }

    /// Reads an attribute, as Pele's parseAttribute does: its visibility first, its
    /// classifier last, and its id between.
    fn parse_attribute(&mut self, input: &str) {
        let b = input.as_bytes();
        if let Some(v) = visibility(b.first().copied()) {
            self.visibility = v;
        }
        match b.last() {
            Some(b'$') => self.classifier = "$",
            Some(b'*') => self.classifier = "*",
            _ => {}
        }
        let from = usize::from(!self.visibility.is_empty());
        let to = b.len() - usize::from(!self.classifier.is_empty());
        self.id = input[from..to].to_string();
    }

    /// The text a member is shown with, as Pele's getDisplayDetails gives it: its
    /// visibility, its id, and a method's parameters and return type, with generic types
    /// written with tildes turned to angle brackets.
    pub(crate) fn display_text(&self) -> String {
        let mut text = format!("{}{}", self.visibility, parse_generic_types(&self.id));
        if self.member_type == MemberType::Method {
            text.push('(');
            text.push_str(&parse_generic_types(trim(&self.parameters)));
            text.push(')');
            if !self.return_type.is_empty() {
                text.push_str(" : ");
                text.push_str(&parse_generic_types(&self.return_type));
            }
        }
        trim(&text).to_string()
    }
}
