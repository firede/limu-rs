//! What a requirement diagram's parser fills, as Pele's RequirementDb in db.ts has it: its
//! requirements, elements, relationships and classes, as Mermaid's requirementDb keeps them.

use std::collections::HashMap;

use crate::detect::DiagramType;
use crate::errors::{Error, ErrorCode};
use crate::js::chars::{drop_indents, trim_start};

/// What the keyword tokens of requirement types stand for, Pele's RequirementType.
pub(crate) const REQUIREMENT_TYPES: [&str; 6] = [
    "Requirement",
    "Functional Requirement",
    "Interface Requirement",
    "Performance Requirement",
    "Physical Requirement",
    "Design Constraint",
];

/// What the keyword tokens of risks stand for, Pele's RiskLevel.
pub(crate) const RISK_LEVELS: [&str; 3] = ["Low", "Medium", "High"];

/// What the keyword tokens of verification stand for, Pele's VerifyType.
pub(crate) const VERIFY_TYPES: [&str; 4] = ["Analysis", "Demonstration", "Inspection", "Test"];

/// What the keyword tokens of relationships stand for, Pele's Relationships.
pub(crate) const RELATIONSHIPS: [&str; 7] = [
    "contains",
    "copies",
    "derives",
    "satisfies",
    "verifies",
    "refines",
    "traces",
];

/// The relationship of containment, Pele's Relationships.CONTAINS.
pub(crate) const CONTAINS: &str = "contains";

/// A requirement, Pele's Requirement.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Requirement {
    pub(crate) name: String,
    pub(crate) kind: String,
    pub(crate) requirement_id: String,
    pub(crate) text: String,
    pub(crate) risk: String,
    pub(crate) verify_method: String,
    pub(crate) css_styles: Vec<String>,
    pub(crate) classes: Vec<String>,
}

/// An element, Pele's RequirementElement.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Element {
    pub(crate) name: String,
    pub(crate) kind: String,
    pub(crate) doc_ref: String,
    pub(crate) css_styles: Vec<String>,
    pub(crate) classes: Vec<String>,
}

/// A relationship, Pele's Relation.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Relation {
    pub(crate) kind: String,
    pub(crate) src: String,
    pub(crate) dst: String,
}

/// A class classDef defines, Pele's RequirementClass.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct RequirementClass {
    pub(crate) id: String,
    pub(crate) styles: Vec<String>,
    pub(crate) text_styles: Vec<String>,
}

/// Items kept in the order they were added, and found by their names, as a JavaScript Map
/// keeps them.
#[derive(Clone, Debug)]
pub(crate) struct Ordered<T> {
    items: Vec<T>,
    at: HashMap<String, usize>,
}

impl<T> Default for Ordered<T> {
    fn default() -> Self {
        Ordered {
            items: Vec::new(),
            at: HashMap::new(),
        }
    }
}

impl<T> Ordered<T> {
    pub(crate) fn get(&self, id: &str) -> Option<&T> {
        self.at.get(id).map(|&i| &self.items[i])
    }

    fn get_mut(&mut self, id: &str) -> Option<&mut T> {
        self.at.get(id).map(|&i| &mut self.items[i])
    }

    pub(crate) fn contains(&self, id: &str) -> bool {
        self.at.contains_key(id)
    }

    /// Adds the item of a name that has none.
    fn add(&mut self, id: &str, item: T) {
        self.at.insert(id.to_string(), self.items.len());
        self.items.push(item);
    }

    pub(crate) fn values(&self) -> impl Iterator<Item = &T> {
        self.items.iter()
    }

    fn values_mut(&mut self) -> impl Iterator<Item = &mut T> {
        self.items.iter_mut()
    }
}

/// Applying a class copies its declarations onto each node, so a short text can ask for
/// hundreds of millions of copies. Past this many the diagram is refused, as Pele's
/// MAX_STYLE_COPIES says.
const MAX_STYLE_COPIES: usize = 1_000_000;

/// A requirement diagram as its parser fills it, Pele's RequirementDb.
#[derive(Clone, Debug)]
pub(crate) struct RequirementDb {
    pub(crate) direction: String,
    pub(crate) requirements: Ordered<Requirement>,
    pub(crate) elements: Ordered<Element>,
    pub(crate) relations: Vec<Relation>,
    pub(crate) classes: Ordered<RequirementClass>,
    pub(crate) title: Option<String>,
    pub(crate) acc_title: Option<String>,
    pub(crate) acc_descr: Option<String>,

    // The grammar reports a body's fields before the requirement or element they belong to.
    new_id: String,
    new_text: String,
    new_risk: String,
    new_verify_method: String,
    new_element_type: String,
    new_doc_ref: String,
    style_copies: usize,
}

impl Default for RequirementDb {
    fn default() -> Self {
        RequirementDb {
            direction: "TB".into(),
            requirements: Ordered::default(),
            elements: Ordered::default(),
            relations: Vec::new(),
            classes: Ordered::default(),
            title: None,
            acc_title: None,
            acc_descr: None,
            new_id: String::new(),
            new_text: String::new(),
            new_risk: String::new(),
            new_verify_method: String::new(),
            new_element_type: String::new(),
            new_doc_ref: String::new(),
            style_copies: 0,
        }
    }
}

/// Adds styles to a node's, counting them against the limit, as Pele's copy does.
fn copy<S: AsRef<str>>(
    count: &mut usize,
    target: &mut Vec<String>,
    styles: &[S],
) -> Result<(), Error> {
    *count += styles.len();
    if *count > MAX_STYLE_COPIES {
        let mut e = Error::new(
            "Requirement diagram applies too many style declarations.",
            ErrorCode::Limit,
        );
        e.diagram = Some(DiagramType::Requirement);
        return Err(e);
    }
    target.extend(styles.iter().map(|s| s.as_ref().to_string()));
    Ok(())
}

/// The styles and classes of the requirement of an id, or of the element where there is none,
/// as Pele's node finds them.
fn node<'a>(
    requirements: &'a mut Ordered<Requirement>,
    elements: &'a mut Ordered<Element>,
    id: &str,
) -> Option<(&'a mut Vec<String>, &'a mut Vec<String>)> {
    if let Some(r) = requirements.get_mut(id) {
        return Some((&mut r.css_styles, &mut r.classes));
    }
    elements
        .get_mut(id)
        .map(|e| (&mut e.css_styles, &mut e.classes))
}

impl RequirementDb {
    pub(crate) fn set_direction(&mut self, dir: &str) {
        self.direction = dir.to_string();
    }

    pub(crate) fn add_requirement(&mut self, name: &str, kind: &str) {
        if !self.requirements.contains(name) {
            let requirement = Requirement {
                name: name.to_string(),
                kind: kind.to_string(),
                requirement_id: self.new_id.clone(),
                text: self.new_text.clone(),
                risk: self.new_risk.clone(),
                verify_method: self.new_verify_method.clone(),
                css_styles: Vec::new(),
                classes: vec!["default".into()],
            };
            self.requirements.add(name, requirement);
        }
        self.new_id.clear();
        self.new_text.clear();
        self.new_risk.clear();
        self.new_verify_method.clear();
    }

    pub(crate) fn set_new_req_id(&mut self, id: &str) {
        self.new_id = id.to_string();
    }

    pub(crate) fn set_new_req_text(&mut self, text: &str) {
        self.new_text = text.to_string();
    }

    pub(crate) fn set_new_req_risk(&mut self, risk: &str) {
        self.new_risk = risk.to_string();
    }

    pub(crate) fn set_new_req_verify_method(&mut self, verify_method: &str) {
        self.new_verify_method = verify_method.to_string();
    }

    pub(crate) fn add_element(&mut self, name: &str) {
        if !self.elements.contains(name) {
            let element = Element {
                name: name.to_string(),
                kind: self.new_element_type.clone(),
                doc_ref: self.new_doc_ref.clone(),
                css_styles: Vec::new(),
                classes: vec!["default".into()],
            };
            self.elements.add(name, element);
        }
        self.new_element_type.clear();
        self.new_doc_ref.clear();
    }

    pub(crate) fn set_new_element_type(&mut self, kind: &str) {
        self.new_element_type = kind.to_string();
    }

    pub(crate) fn set_new_element_doc_ref(&mut self, doc_ref: &str) {
        self.new_doc_ref = doc_ref.to_string();
    }

    pub(crate) fn add_relationship(&mut self, kind: &str, src: &str, dst: &str) {
        self.relations.push(Relation {
            kind: kind.to_string(),
            src: src.to_string(),
            dst: dst.to_string(),
        });
    }

    pub(crate) fn set_css_style(&mut self, ids: &[String], styles: &[String]) -> Result<(), Error> {
        for id in ids {
            // Mermaid stops at the first unknown id, leaving the rest of the list unstyled.
            let Some((css, _)) = node(&mut self.requirements, &mut self.elements, id) else {
                return Ok(());
            };
            for s in styles {
                if s.contains(',') {
                    copy(
                        &mut self.style_copies,
                        css,
                        &s.split(',').collect::<Vec<_>>(),
                    )?;
                } else {
                    copy(&mut self.style_copies, css, std::slice::from_ref(s))?;
                }
            }
        }
        Ok(())
    }

    pub(crate) fn set_class(
        &mut self,
        ids: &[String],
        class_names: &[String],
    ) -> Result<(), Error> {
        for id in ids {
            let Some((css, classes)) = node(&mut self.requirements, &mut self.elements, id) else {
                continue;
            };
            for name in class_names {
                classes.push(name.clone());
                if let Some(c) = self.classes.get(name) {
                    copy(&mut self.style_copies, css, &c.styles)?;
                }
            }
        }
        Ok(())
    }

    pub(crate) fn define_class(&mut self, ids: &[String], style: &[String]) -> Result<(), Error> {
        for id in ids {
            if !self.classes.contains(id) {
                let class = RequirementClass {
                    id: id.clone(),
                    ..RequirementClass::default()
                };
                self.classes.add(id, class);
            }
            if let Some(class) = self.classes.get_mut(id) {
                for s in style {
                    if s.contains("color") {
                        class.text_styles.push(s.replacen("fill", "bgFill", 1));
                    }
                    class.styles.push(s.clone());
                }
            }
            let split: Vec<&str> = style.iter().flat_map(|s| s.split(',')).collect();
            for r in self.requirements.values_mut() {
                if r.classes.contains(id) {
                    copy(&mut self.style_copies, &mut r.css_styles, &split)?;
                }
            }
            for e in self.elements.values_mut() {
                if e.classes.contains(id) {
                    copy(&mut self.style_copies, &mut e.css_styles, &split)?;
                }
            }
        }
        Ok(())
    }

    pub(crate) fn set_acc_title(&mut self, text: &str) {
        self.acc_title = Some(trim_start(text).to_string());
    }

    pub(crate) fn set_acc_description(&mut self, text: &str) {
        self.acc_descr = Some(drop_indents(text));
    }
}
