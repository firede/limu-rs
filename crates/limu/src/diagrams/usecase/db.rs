//! What the parser of a use case diagram reads, and the model it is resolved into once the
//! whole diagram is read, as Pele's db.ts has them.

use std::collections::HashMap;

use super::lexer::{line_column, location_text, placed};
use super::ordered::Ordered;
use super::types::{
    Actor, ClassDef, EdgeOccurrence, GraphStatement, LabelType, MetadataOccurrence, Relationship,
    Span, SystemBoundary, UseCase, UsecaseJsonNode, UsecaseModel, UsecaseNote,
};
use crate::errors::{Error, ErrorCode};
use crate::js::chars::drop_indents;
use crate::value::Map;

/// A label as written, Pele's DraftLabel.
#[derive(Clone, Debug)]
pub(crate) struct DraftLabel {
    pub(crate) text: String,
    pub(crate) kind: LabelType,
    pub(crate) span: Span,
}

/// The value of a property of a metadata block: a string, or true or false.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum PropertyValue {
    String(String),
    Bool(bool),
}

impl PropertyValue {
    fn as_str(&self) -> Option<&str> {
        match self {
            PropertyValue::String(s) => Some(s),
            PropertyValue::Bool(_) => None,
        }
    }

    fn as_bool(&self) -> Option<bool> {
        match self {
            PropertyValue::Bool(b) => Some(*b),
            PropertyValue::String(_) => None,
        }
    }
}

/// A property of a metadata block, Pele's DraftProperty. Its key span is also where an
/// error about it points.
#[derive(Clone, Debug)]
pub(crate) struct DraftProperty {
    pub(crate) at: MetadataOccurrence,
    pub(crate) value: PropertyValue,
}

/// What every mention of an id carries: where it is, and whether the id was made from a
/// label, Pele's Named.
#[derive(Clone, Debug)]
pub(crate) struct Named {
    pub(crate) id: String,
    pub(crate) label: DraftLabel,
    pub(crate) span: Span,
    pub(crate) generated: bool,
}

/// An actor or a use case as a statement declares it, Pele's DraftElement. Its parent is
/// the index of the boundary it is in, among the drafts' boundaries.
#[derive(Clone, Debug)]
pub(crate) struct DraftElement {
    pub(crate) named: Named,
    pub(crate) kind: &'static str,
    pub(crate) parent: Option<usize>,
    pub(crate) shape: Option<&'static str>,
    pub(crate) metadata: Option<Vec<DraftProperty>>,
    pub(crate) stereotype: Option<String>,
    pub(crate) classes: Vec<String>,
}

/// A system boundary as a statement declares it, Pele's DraftBoundary.
#[derive(Clone, Debug)]
pub(crate) struct DraftBoundary {
    pub(crate) named: Named,
    pub(crate) classes: Vec<String>,
}

/// A `json` node as a statement declares it, Pele's DraftJson.
#[derive(Clone, Debug)]
pub(crate) struct DraftJson {
    pub(crate) id: String,
    pub(crate) value: Map,
    pub(crate) property_order: Vec<(String, Vec<String>)>,
    pub(crate) span: Span,
    pub(crate) classes: Vec<String>,
}

/// An end of a relationship, Pele's DraftEndpoint: whether this mention declares the
/// element, and whether it carries a `:::` suffix.
#[derive(Clone, Debug)]
pub(crate) struct DraftEndpoint {
    pub(crate) named: Named,
    pub(crate) declares: bool,
    pub(crate) classed: bool,
}

/// A relationship as a statement writes it, Pele's DraftRelationship.
#[derive(Clone, Debug)]
pub(crate) struct DraftRelationship {
    pub(crate) source: DraftEndpoint,
    pub(crate) target: DraftEndpoint,
    pub(crate) span: Span,
    pub(crate) id: Option<String>,
    pub(crate) id_span: Option<Span>,
    pub(crate) kind: &'static str,
    pub(crate) arrow_type: usize,
    pub(crate) label: Option<DraftLabel>,
    pub(crate) minlen: f64,
}

#[derive(Clone, Debug)]
pub(crate) struct DraftNote {
    pub(crate) target: String,
    pub(crate) span: Span,
    pub(crate) label: DraftLabel,
}

/// A metadata block given to an id. Its statement is the index of the statement that holds
/// it, among the diagram's statements, which are all at the top.
#[derive(Clone, Debug)]
pub(crate) struct DraftMetadata {
    pub(crate) target: String,
    pub(crate) span: Span,
    pub(crate) properties: Vec<DraftProperty>,
    pub(crate) statement: usize,
}

#[derive(Clone, Debug)]
pub(crate) struct DraftClasses {
    pub(crate) targets: Vec<(String, Span)>,
    pub(crate) classes: Vec<String>,
}

#[derive(Clone, Debug)]
pub(crate) struct DraftStyle {
    pub(crate) target: String,
    pub(crate) span: Span,
    pub(crate) styles: Vec<String>,
}

#[derive(Clone, Debug)]
pub(crate) struct DraftClassDef {
    pub(crate) ids: Vec<String>,
    pub(crate) styles: Vec<String>,
}

/// Everything the parser read, in source order, before any of it is resolved, Pele's
/// Drafts.
#[derive(Clone, Debug, Default)]
pub(crate) struct Drafts {
    pub(crate) elements: Vec<DraftElement>,
    pub(crate) boundaries: Vec<DraftBoundary>,
    pub(crate) jsons: Vec<DraftJson>,
    pub(crate) relationships: Vec<DraftRelationship>,
    pub(crate) notes: Vec<DraftNote>,
    pub(crate) metadata: Vec<DraftMetadata>,
    pub(crate) classes: Vec<DraftClasses>,
    pub(crate) styles: Vec<DraftStyle>,
    pub(crate) class_defs: Vec<DraftClassDef>,
    pub(crate) direction: Option<&'static str>,
    pub(crate) acc_title: String,
    pub(crate) acc_description: String,
}

/// What an id was first declared as, Pele's Origin. Its kind is "actor", "usecase",
/// "boundary", "json" or "edge", and its label the label a generated id was made from.
#[derive(Clone, Debug)]
struct Origin {
    kind: &'static str,
    span: Span,
    generated: bool,
    label: Option<String>,
}

/// An actor or a use case as the declarations so far make it, Pele's ElementState.
#[derive(Clone, Debug)]
struct ElementState {
    named: Named,
    kind: &'static str,
    parent: Option<usize>,
    shape: Option<&'static str>,
    stereotype: Option<String>,
    stereotype_span: Option<Span>,
    classes: Vec<String>,
    actor_type: Option<&'static str>,
    icon: Option<String>,
    business: Option<bool>,
}

/// A system boundary as the declarations so far make it, Pele's BoundaryState.
#[derive(Clone, Debug)]
struct BoundaryState {
    named: Named,
    classes: Vec<String>,
    kind: Option<&'static str>,
    members: Vec<String>,
}

/// Adds to target each of values it does not hold yet, in order, as Pele's pushUnique does.
fn push_unique(target: &mut Vec<String>, values: &[String]) {
    for v in values {
        if !target.contains(v) {
            target.push(v.clone());
        }
    }
}

/// The kind of element the keys of a metadata block could only belong to, for the error
/// message, as Pele's inferKind finds it.
fn infer_kind(properties: &[DraftProperty]) -> Option<&'static str> {
    let mut possible: Vec<&'static str> = vec!["actor", "usecase", "boundary", "edge"];
    for p in properties {
        match p.at.key.as_str() {
            "icon" => possible = vec!["actor"],
            "animate" | "animation" => possible = vec!["edge"],
            "type" => {
                possible = match p.value.as_str() {
                    Some("rect" | "package") => vec!["boundary"],
                    Some("normal" | "hollow" | "awesome") => vec!["actor"],
                    _ => vec![],
                }
            }
            "business" => possible.retain(|k| *k != "boundary" && *k != "edge"),
            _ => return None,
        }
    }
    (possible.len() == 1).then(|| possible[0])
}

/// The state of a resolution that Pele's resolveUsecase keeps in its locals, with the
/// closures of resolveUsecase that need no more than this as its methods.
struct Resolver<'a> {
    src: &'a str,
    symbols: HashMap<String, Origin>,
    /// The offset at which each id is first mentioned, which decides the order of the model.
    first: HashMap<String, usize>,
}

impl Resolver<'_> {
    /// Where a span is, as the at closure of Pele's resolveUsecase writes it.
    fn at(&self, span: Span) -> String {
        location_text(self.src, span[0], span[1])
    }

    /// A semantic error at a span, as the error closure of Pele's resolveUsecase makes it.
    fn error(&self, message: String, span: Span) -> Error {
        let (line, column) = line_column(self.src, span[0]);
        placed(message, ErrorCode::Semantic, line, column)
    }

    /// The error at a span, as the fail closure of Pele's resolveUsecase throws it.
    fn fail(&self, message: &str, span: Span) -> Error {
        self.error(format!("{message} at {}", self.at(span)), span)
    }

    /// The error of a declaration and the one before it that it disagrees with, naming the
    /// labels generated ids were made from, as the conflict closure of Pele's resolveUsecase
    /// throws it.
    fn conflict(
        &self,
        message: &str,
        current: Span,
        previous: Span,
        current_label: Option<&str>,
        previous_label: Option<&str>,
    ) -> Error {
        let suffix =
            |label: Option<&str>| label.map_or(String::new(), |l| format!(" (label \"{l}\")"));
        self.error(
            format!(
                "{message} at {}{}; previous declaration at {}{}",
                self.at(current),
                suffix(current_label),
                self.at(previous),
                suffix(previous_label)
            ),
            current,
        )
    }

    /// The error of a metadata property that does not belong to the kind of element it is
    /// given to, as the invalid closure of Pele's resolveUsecase makes it.
    fn invalid(&self, id: &str, kind: &str, p: &DraftProperty) -> Error {
        self.fail(
            &format!(
                "Metadata property '{}' is invalid for {kind} '{id}'",
                p.at.key
            ),
            p.at.key_span,
        )
    }

    /// Notes an offset at which an id is mentioned, keeping the first, as the mention
    /// closure of Pele's resolveUsecase does.
    fn mention(&mut self, id: &str, offset: usize) {
        match self.first.get_mut(id) {
            Some(previous) => *previous = (*previous).min(offset),
            None => {
                self.first.insert(id.to_string(), offset);
            }
        }
    }

    /// The offset at which an id is first mentioned, 0 for one never mentioned, as
    /// `first.get(id) ?? 0` reads it.
    fn first_of(&self, id: &str) -> usize {
        self.first.get(id).copied().unwrap_or(0)
    }

    /// Ids sorted by where they are first mentioned, as the inOrder closure of Pele's
    /// resolveUsecase sorts them, stably.
    fn in_order(&self, mut ids: Vec<String>) -> Vec<String> {
        ids.sort_by_key(|id| self.first_of(id));
        ids
    }

    /// Records what a declared id is and where it was declared, as the register closure of
    /// Pele's resolveUsecase does.
    fn register(&mut self, n: &Named, kind: &'static str) {
        self.symbols.insert(
            n.id.clone(),
            Origin {
                kind,
                span: n.span,
                generated: n.generated,
                label: Some(n.label.text.clone()),
            },
        );
    }

    /// Checks that an id means one thing in the whole diagram, and that an id made from a
    /// label is declared once, as the claim closure of Pele's resolveUsecase does.
    fn claim(&self, d: &Named, kind: &str) -> Result<(), Error> {
        let Some(o) = self.symbols.get(&d.id) else {
            return Ok(());
        };
        let label = d.generated.then_some(d.label.text.as_str());
        let origin_label = if o.generated {
            o.label.as_deref()
        } else {
            None
        };
        if o.kind != kind {
            return Err(self.conflict(
                &format!("ID '{}' is declared as both {} and {kind}", d.id, o.kind),
                d.span,
                o.span,
                label,
                origin_label,
            ));
        }
        if o.generated || d.generated {
            return Err(self.conflict(
                &format!("Generated ID '{}' collides with another declaration", d.id),
                d.span,
                o.span,
                label,
                origin_label,
            ));
        }
        Ok(())
    }

    /// Declares an id that may be declared only once, a JSON node's or an edge's, as the
    /// unique closure of Pele's resolveUsecase does.
    fn unique(&mut self, id: &str, kind: &'static str, span: Span) -> Result<(), Error> {
        if let Some(previous) = self.symbols.get(id) {
            let previous_label = if previous.generated {
                previous.label.as_deref()
            } else {
                None
            };
            return Err(self.conflict(
                &format!(
                    "ID '{id}' is declared more than once ({} and {kind})",
                    previous.kind
                ),
                span,
                previous.span,
                None,
                previous_label,
            ));
        }
        self.symbols.insert(
            id.to_string(),
            Origin {
                kind,
                span,
                generated: false,
                label: None,
            },
        );
        Ok(())
    }

    /// Sets a property of an actor, as the actorProperty closure of Pele's resolveUsecase
    /// does. A metadata statement replaces what a declaration said, and two declarations
    /// have to agree.
    fn actor_property(
        &self,
        state: &mut ElementState,
        p: &DraftProperty,
        replace: bool,
    ) -> Result<(), Error> {
        let clash = |differs: bool| {
            if !replace && differs {
                return Err(self.conflict(
                    &format!(
                        "Actor '{}' has conflicting {} metadata",
                        state.named.id, p.at.key
                    ),
                    p.at.key_span,
                    state.named.span,
                    None,
                    None,
                ));
            }
            Ok(())
        };
        match (p.at.key.as_str(), &p.value) {
            ("type", PropertyValue::String(s))
                if matches!(s.as_str(), "normal" | "hollow" | "awesome") =>
            {
                let value = match s.as_str() {
                    "normal" => "normal",
                    "hollow" => "hollow",
                    _ => "awesome",
                };
                clash(state.actor_type.is_some_and(|t| t != value))?;
                state.actor_type = Some(value);
            }
            ("icon", PropertyValue::String(s)) => {
                clash(state.icon.as_ref().is_some_and(|i| i != s))?;
                state.icon = Some(s.clone());
            }
            ("business", PropertyValue::Bool(b)) => {
                clash(state.business.is_some_and(|v| v != *b))?;
                state.business = Some(*b);
            }
            _ => return Err(self.invalid(&state.named.id, "actor", p)),
        }
        Ok(())
    }

    /// Applies the properties of a metadata block to an actor or a use case, as the
    /// elementMetadata closure of Pele's resolveUsecase does: a use case takes only business.
    fn element_metadata(
        &self,
        state: &mut ElementState,
        properties: Option<&[DraftProperty]>,
        replace: bool,
    ) -> Result<(), Error> {
        for p in properties.unwrap_or_default() {
            if state.kind == "actor" {
                self.actor_property(state, p, replace)?;
            } else if let ("business", Some(b)) = (p.at.key.as_str(), p.value.as_bool()) {
                if !replace && state.business.is_some_and(|v| v != b) {
                    return Err(self.conflict(
                        &format!(
                            "Use case '{}' has conflicting business metadata",
                            state.named.id
                        ),
                        p.at.key_span,
                        state.named.span,
                        None,
                        None,
                    ));
                }
                state.business = Some(b);
            } else {
                return Err(self.invalid(&state.named.id, "usecase", p));
            }
        }
        Ok(())
    }
}

/// The actors and use cases of the model as Pele's resolveUsecase keeps them, by id.
#[derive(Default)]
struct Published {
    actors: Ordered<String, Actor>,
    use_cases: Ordered<String, UseCase>,
}

impl Published {
    /// Puts an actor or a use case in the model as its state makes it, in place of what the
    /// model held for the id, keeping the styles it had, as the publish closure of Pele's
    /// resolveUsecase does.
    fn publish(&mut self, state: &ElementState, boundaries: &[DraftBoundary]) {
        let id = &state.named.id;
        let stereotype = state.stereotype.clone().filter(|s| !s.is_empty());
        let parent_id = state
            .parent
            .map(|b| boundaries[b].named.id.clone())
            .filter(|p| !p.is_empty());
        let icon = state.icon.clone().filter(|i| !i.is_empty());
        if state.kind == "actor" {
            self.use_cases.delete(id);
            let styles = self
                .actors
                .get(id)
                .map(|a| a.styles.clone())
                .unwrap_or_default();
            let actor = Actor {
                id: id.clone(),
                label: state.named.label.text.clone(),
                label_type: state.named.label.kind,
                kind: if icon.is_some() {
                    "icon"
                } else {
                    state.actor_type.unwrap_or("normal")
                },
                icon,
                business: state.business.unwrap_or(false),
                stereotype,
                parent_id,
                classes: state.classes.clone(),
                styles,
            };
            self.actors.set(id.clone(), actor);
        } else {
            self.actors.delete(id);
            let styles = self
                .use_cases
                .get(id)
                .map(|u| u.styles.clone())
                .unwrap_or_default();
            let use_case = UseCase {
                id: id.clone(),
                label: state.named.label.text.clone(),
                label_type: state.named.label.kind,
                shape: state.shape.unwrap_or("ellipse"),
                business: state.business.unwrap_or(false),
                stereotype,
                parent_id,
                classes: state.classes.clone(),
                styles,
            };
            self.use_cases.set(id.clone(), use_case);
        }
    }
}

/// A declaration, to be run in the order of the source: of an element, a boundary, a JSON
/// node or a relationship's id, by its index among the drafts of its kind.
#[derive(Clone, Copy)]
enum Declaration {
    Element(usize),
    Boundary(usize),
    Json(usize),
    Edge(usize),
}

/// Resolves what the parser read into a model, once the whole diagram is read, as Pele's
/// resolveUsecase does: which ids are actors, use cases, boundaries, JSON nodes or edges, in
/// what order they appear, and whether the statements about them agree. It ports Mermaid's
/// UsecaseModelBuilder, with its messages.
pub(crate) fn resolve_usecase(
    src: &str,
    drafts: Drafts,
    header_span: Span,
    mut statements: Vec<GraphStatement>,
    title: Option<&str>,
) -> Result<UsecaseModel, Error> {
    let mut r = Resolver {
        src,
        symbols: HashMap::new(),
        first: HashMap::new(),
    };
    let mut out = Published::default();
    let mut system_boundaries: Ordered<String, SystemBoundary> = Ordered::default();
    let mut json_nodes: Ordered<String, UsecaseJsonNode> = Ordered::default();
    let mut relationships: Vec<Relationship> = Vec::new();
    let mut elements: Ordered<String, ElementState> = Ordered::default();
    let mut boundaries: Ordered<String, BoundaryState> = Ordered::default();
    // The index of each relationship by its id, among the relationships.
    let mut edges: Ordered<String, usize> = Ordered::default();

    let mut declarations: Vec<(usize, Declaration)> = Vec::new();
    for d in &drafts.relationships {
        r.mention(&d.source.named.id, d.source.named.span[0]);
        r.mention(&d.target.named.id, d.target.named.span[0]);
    }
    for (i, d) in drafts.elements.iter().enumerate() {
        r.mention(&d.named.id, d.named.span[0]);
        declarations.push((d.named.span[0], Declaration::Element(i)));
    }
    for (i, d) in drafts.boundaries.iter().enumerate() {
        r.mention(&d.named.id, d.named.span[0]);
        declarations.push((d.named.span[0], Declaration::Boundary(i)));
    }
    for (i, d) in drafts.jsons.iter().enumerate() {
        r.mention(&d.id, d.span[0]);
        declarations.push((d.span[0], Declaration::Json(i)));
    }
    for (i, d) in drafts.relationships.iter().enumerate() {
        if let (Some(_), Some(span)) = (&d.id, d.id_span) {
            declarations.push((span[0], Declaration::Edge(i)));
        }
    }
    declarations.sort_by_key(|&(offset, _)| offset);
    for &(_, declaration) in &declarations {
        match declaration {
            Declaration::Element(i) => declare_element(
                &mut r,
                &mut elements,
                &drafts.elements[i],
                &drafts.boundaries,
            )?,
            Declaration::Boundary(i) => {
                // The boundary is declared, or adds the classes another declaration of it
                // gives, as the declareBoundary closure of Pele's resolveUsecase does.
                let d = &drafts.boundaries[i];
                r.claim(&d.named, "boundary")?;
                if let Some(existing) = boundaries.get_mut(&d.named.id) {
                    if existing.named.label.text != d.named.label.text
                        || existing.named.label.kind != d.named.label.kind
                    {
                        let message = format!("Boundary '{}' has conflicting titles", d.named.id);
                        return Err(r.conflict(
                            &message,
                            d.named.span,
                            existing.named.span,
                            None,
                            None,
                        ));
                    }
                    push_unique(&mut existing.classes, &d.classes);
                    continue;
                }
                boundaries.set(
                    d.named.id.clone(),
                    BoundaryState {
                        named: d.named.clone(),
                        classes: d.classes.clone(),
                        kind: None,
                        members: Vec::new(),
                    },
                );
                r.register(&d.named, "boundary");
            }
            Declaration::Json(i) => {
                let d = &drafts.jsons[i];
                r.unique(&d.id, "json", d.span)?;
            }
            Declaration::Edge(i) => {
                let d = &drafts.relationships[i];
                if let (Some(id), Some(span)) = (&d.id, d.id_span) {
                    r.unique(id, "edge", span)?;
                }
            }
        }
    }

    let element_ids = |r: &Resolver, elements: &Ordered<String, ElementState>| {
        r.in_order(elements.keys().cloned().collect())
    };
    for id in element_ids(&r, &elements) {
        if let Some(state) = elements.get(&id) {
            out.publish(state, &drafts.boundaries);
        }
    }
    for id in r.in_order(boundaries.keys().cloned().collect()) {
        let Some(state) = boundaries.get(&id) else {
            continue;
        };
        let boundary = SystemBoundary {
            id: id.clone(),
            label: state.named.label.text.clone(),
            label_type: state.named.label.kind,
            kind: "rect",
            members: Vec::new(),
            classes: Vec::new(),
            styles: Vec::new(),
        };
        system_boundaries.set(id, boundary);
    }
    let mut jsons: Vec<&DraftJson> = drafts.jsons.iter().collect();
    jsons.sort_by_key(|d| r.first_of(&d.id));
    for d in jsons {
        let node = UsecaseJsonNode {
            id: d.id.clone(),
            value: d.value.clone(),
            property_order: d.property_order.clone(),
            classes: d.classes.clone(),
            styles: Vec::new(),
        };
        json_nodes.set(d.id.clone(), node);
    }

    let mut anonymous = 0;
    for d in &drafts.relationships {
        let (source, target, kind) = (&d.source, &d.target, d.kind);
        let source_kind = endpoint(&mut r, &mut elements, source)?;
        let target_kind = endpoint(&mut r, &mut elements, target)?;
        let related = |kind: &str| matches!(kind, "actor" | "usecase" | "json");
        let declared = |r: &Resolver, id: &str| r.symbols.get(id).map_or([0, 0], |o| o.span);
        if !related(source_kind) {
            let message = format!(
                "Relationship source '{}' cannot be {source_kind}",
                source.named.id
            );
            let previous = declared(&r, &source.named.id);
            return Err(r.conflict(&message, source.named.span, previous, None, None));
        }
        if !related(target_kind) {
            let message = format!(
                "Relationship target '{}' cannot be {target_kind}",
                target.named.id
            );
            let previous = declared(&r, &target.named.id);
            return Err(r.conflict(&message, target.named.span, previous, None, None));
        }
        if (kind == "include" || kind == "extend")
            && (source_kind != "usecase" || target_kind != "usecase")
        {
            return Err(r.fail(
                &format!("{kind} relationship requires use-case endpoints"),
                d.span,
            ));
        }
        if kind == "generalization" && (source_kind == "json" || source_kind != target_kind) {
            return Err(r.fail(
                "Generalization requires actor-to-actor or use-case-to-use-case endpoints",
                d.span,
            ));
        }
        if kind == "association"
            && (source_kind == "json" || target_kind == "json")
            && d.arrow_type > 2
        {
            return Err(r.fail(
                &format!(
                    "JSON relationship '{}' to '{}' permits only point, reversed-point, or markerless solid association",
                    source.named.id, target.named.id
                ),
                d.span,
            ));
        }
        let id = match &d.id {
            Some(id) => id.clone(),
            None => {
                anonymous += 1;
                format!("edge-{}", anonymous - 1)
            }
        };
        let relationship = Relationship {
            id: id.clone(),
            explicit_id: d.id.is_some(),
            source: source.named.id.clone(),
            target: target.named.id.clone(),
            kind,
            arrow_type: d.arrow_type,
            label: d.label.as_ref().map(|l| (l.text.clone(), l.kind)),
            minlen: d.minlen,
            classes: Vec::new(),
            styles: Vec::new(),
            animate: false,
            animation: None,
        };
        edges.set(id, relationships.len());
        relationships.push(relationship);
    }

    // The relationships may have added use cases, so all elements are put back in the order
    // of first mention.
    let declared = std::mem::take(&mut out);
    for id in element_ids(&r, &elements) {
        if let Some(actor) = declared.actors.get(&id) {
            out.actors.set(id, actor.clone());
        } else if let Some(use_case) = declared.use_cases.get(&id) {
            out.use_cases.set(id, use_case.clone());
        } else if let Some(state) = elements.get(&id) {
            out.publish(state, &drafts.boundaries);
        }
    }

    for d in &drafts.metadata {
        let Some(origin) = r.symbols.get(&d.target) else {
            let implied = infer_kind(&d.properties)
                .map_or(String::new(), |k| format!(" (metadata implies {k})"));
            return Err(r.fail(
                &format!("Metadata target '{}' is unresolved{implied}", d.target),
                d.span,
            ));
        };
        let kind = origin.kind;
        if kind == "actor" || kind == "usecase" {
            if let Some(state) = elements.get_mut(&d.target) {
                r.element_metadata(state, Some(&d.properties), true)?;
            }
            continue;
        }
        if kind == "edge" {
            let s = &mut statements[d.statement];
            s.kind = "edgeMetadata";
            s.edges = Some(vec![EdgeOccurrence {
                id: d.target.clone(),
                span: s.span,
                label_span: None,
                id_span: Some(d.span),
                metadata: s.metadata.clone(),
            }]);
            s.nodes = None;
        }
        let edge = edges.get(&d.target).copied();
        for p in &d.properties {
            let (key, value) = (p.at.key.as_str(), &p.value);
            match (kind, key, value) {
                ("boundary", "type", PropertyValue::String(s))
                    if boundaries.has(&d.target) && matches!(s.as_str(), "rect" | "package") =>
                {
                    if let Some(b) = boundaries.get_mut(&d.target) {
                        b.kind = Some(if s == "rect" { "rect" } else { "package" });
                    }
                }
                ("edge", "animate", PropertyValue::Bool(b)) if edge.is_some() => {
                    if let Some(e) = edge {
                        relationships[e].animate = *b;
                    }
                }
                ("edge", "animation", PropertyValue::String(s))
                    if edge.is_some() && matches!(s.as_str(), "fast" | "slow") =>
                {
                    if let Some(e) = edge {
                        relationships[e].animation =
                            Some(if s == "fast" { "fast" } else { "slow" });
                        relationships[e].animate = true;
                    }
                }
                _ => return Err(r.invalid(&d.target, kind, p)),
            }
        }
    }
    for relationship in &mut relationships {
        if relationship.animation.is_some() {
            relationship.animate = true;
        }
    }

    for state in elements.values() {
        let id = &state.named.id;
        if state.kind == "actor" {
            let icon = state.icon.as_ref().is_some_and(|i| !i.is_empty());
            if let Some(t) = state.actor_type.filter(|&t| icon && t != "normal") {
                return Err(r.fail(
                    &format!("Actor '{id}' cannot combine icon with type '{t}'"),
                    state.named.span,
                ));
            }
            if state.business == Some(true) && (icon || state.actor_type == Some("awesome")) {
                return Err(r.fail(
                    &format!("Business actor '{id}' must use normal or hollow geometry"),
                    state.named.span,
                ));
            }
        } else if state.shape == Some("rect") && state.business == Some(true) {
            return Err(r.fail(
                &format!("Rectangular use case '{id}' cannot be a business use case"),
                state.named.span,
            ));
        }
        out.publish(state, &drafts.boundaries);
    }

    let mut sorted: Vec<&DraftElement> = drafts.elements.iter().collect();
    sorted.sort_by_key(|d| d.named.span[0]);
    for d in sorted {
        let Some(parent) = d.parent.map(|b| &drafts.boundaries[b].named.id) else {
            continue;
        };
        if parent.is_empty() {
            continue;
        }
        if let Some(b) = boundaries.get_mut(parent)
            && !b.members.contains(&d.named.id)
        {
            b.members.push(d.named.id.clone());
        }
    }
    for state in boundaries.values() {
        if let Some(b) = system_boundaries.get_mut(&state.named.id) {
            b.kind = state.kind.unwrap_or("rect");
            b.members = state.members.clone();
            b.classes = state.classes.clone();
        }
    }

    let mut notes: Ordered<String, UsecaseNote> = Ordered::default();
    for (i, d) in drafts.notes.iter().enumerate() {
        let Some(origin) = r.symbols.get(&d.target) else {
            return Err(r.fail(&format!("Note target '{}' is unresolved", d.target), d.span));
        };
        if origin.kind != "actor" && origin.kind != "usecase" {
            let message = format!(
                "Note target '{}' must be an actor or use case, not {}",
                d.target, origin.kind
            );
            return Err(r.conflict(&message, d.span, origin.span, None, None));
        }
        let id = format!("note-{i}");
        let note = UsecaseNote {
            id: id.clone(),
            target: d.target.clone(),
            label: d.label.text.clone(),
            label_type: d.label.kind,
        };
        notes.set(id, note);
    }

    let mut class_defs: Ordered<String, ClassDef> = Ordered::default();
    for d in &drafts.class_defs {
        for id in &d.ids {
            let def = ClassDef {
                id: id.clone(),
                styles: d.styles.clone(),
            };
            class_defs.set(id.clone(), def);
        }
    }
    // What a class or style statement gives classes or styles to, as the stylable closure of
    // Pele's resolveUsecase finds it.
    let mut stylable = |r: &Resolver,
                        id: &str,
                        span: Span,
                        give: &mut dyn FnMut(&mut Vec<String>, &mut Vec<String>)|
     -> Result<(), Error> {
        let found = match r.symbols.get(id).map(|o| o.kind) {
            Some("actor") => out
                .actors
                .get_mut(id)
                .map(|a| give(&mut a.classes, &mut a.styles)),
            Some("usecase") => out
                .use_cases
                .get_mut(id)
                .map(|u| give(&mut u.classes, &mut u.styles)),
            Some("boundary") => system_boundaries
                .get_mut(id)
                .map(|b| give(&mut b.classes, &mut b.styles)),
            Some("json") => json_nodes
                .get_mut(id)
                .map(|j| give(&mut j.classes, &mut j.styles)),
            _ => edges.get(id).map(|&e| {
                let e = &mut relationships[e];
                give(&mut e.classes, &mut e.styles)
            }),
        };
        found.ok_or_else(|| {
            r.fail(
                &format!("Class/style target '{id}' is unresolved or anonymous"),
                span,
            )
        })
    };
    for d in &drafts.classes {
        for (id, span) in &d.targets {
            stylable(&r, id, *span, &mut |classes, _| {
                push_unique(classes, &d.classes)
            })?;
        }
    }
    for d in &drafts.styles {
        stylable(&r, &d.target, d.span, &mut |_, styles| {
            styles.extend(d.styles.iter().cloned())
        })?;
    }

    Ok(UsecaseModel {
        actors: out.actors.into_values(),
        use_cases: out.use_cases.into_values(),
        system_boundaries: system_boundaries.into_values(),
        relationships,
        notes: notes.into_values(),
        json_nodes: json_nodes.into_values(),
        class_defs: class_defs.into_values(),
        direction: drafts.direction.unwrap_or("LR"),
        title: title.map(str::to_string),
        acc_title: drafts.acc_title,
        acc_descr: drop_indents(&drafts.acc_description),
        acc_description: drafts.acc_description,
        source: src.to_string(),
        header_span,
        statements,
    })
}

/// Declares an actor or a use case, or adds what another declaration of it says, as the
/// declareElement closure of Pele's resolveUsecase does.
fn declare_element(
    r: &mut Resolver,
    elements: &mut Ordered<String, ElementState>,
    d: &DraftElement,
    boundaries: &[DraftBoundary],
) -> Result<(), Error> {
    r.claim(&d.named, d.kind)?;
    let parent = d.parent.filter(|&b| !boundaries[b].named.id.is_empty());
    let Some(existing) = elements.get_mut(&d.named.id) else {
        let mut state = ElementState {
            named: d.named.clone(),
            kind: d.kind,
            parent,
            shape: d.shape,
            stereotype: d.stereotype.clone(),
            stereotype_span: d.stereotype.as_ref().map(|_| d.named.span),
            classes: d.classes.clone(),
            actor_type: None,
            icon: None,
            business: None,
        };
        r.element_metadata(&mut state, d.metadata.as_deref(), false)?;
        elements.set(d.named.id.clone(), state);
        r.register(&d.named, d.kind);
        return Ok(());
    };
    if existing.named.label.text != d.named.label.text
        || existing.named.label.kind != d.named.label.kind
    {
        let message = format!("ID '{}' has conflicting labels", d.named.id);
        return Err(r.conflict(&message, d.named.span, existing.named.span, None, None));
    }
    if let (Some(a), Some(b)) = (d.shape, existing.shape)
        && a != b
    {
        let message = format!("Use case '{}' has conflicting shapes", d.named.id);
        return Err(r.conflict(&message, d.named.span, existing.named.span, None, None));
    }
    // A boundary whose title makes an empty id is no parent, but it still takes the place of
    // one.
    if let (Some(p), Some(e)) = (parent, existing.parent) {
        let (p, e) = (&boundaries[p].named, &boundaries[e].named);
        if !e.id.is_empty() && p.id != e.id {
            let message = format!(
                "Element '{}' belongs to more than one system boundary",
                d.named.id
            );
            return Err(r.conflict(&message, p.span, e.span, None, None));
        }
    }
    if let (Some(a), Some(b)) = (&d.stereotype, &existing.stereotype)
        && a != b
    {
        let message = format!("Element '{}' has conflicting stereotypes", d.named.id);
        let previous = existing.stereotype_span.unwrap_or(existing.named.span);
        return Err(r.conflict(&message, d.named.span, previous, None, None));
    }
    existing.shape = existing.shape.or(d.shape);
    existing.parent = existing.parent.or(d.parent);
    if existing.stereotype.is_none() {
        existing.stereotype = d.stereotype.clone();
    }
    if existing.stereotype_span.is_none() && d.stereotype.is_some() {
        existing.stereotype_span = Some(d.named.span);
    }
    push_unique(&mut existing.classes, &d.classes);
    r.element_metadata(existing, d.metadata.as_deref(), false)
}

/// What an end of a relationship is, as the endpoint closure of Pele's resolveUsecase finds
/// it. An end that nothing declares becomes a use case.
fn endpoint(
    r: &mut Resolver,
    elements: &mut Ordered<String, ElementState>,
    end: &DraftEndpoint,
) -> Result<&'static str, Error> {
    if end.classed && !end.declares {
        return Err(r.fail(
            &format!(
                "Relationship endpoint '{}' uses ::: without declaring the node",
                end.named.id
            ),
            end.named.span,
        ));
    }
    if let Some(o) = r.symbols.get(&end.named.id) {
        return Ok(o.kind);
    }
    let state = ElementState {
        named: end.named.clone(),
        kind: "usecase",
        parent: None,
        shape: Some("ellipse"),
        stereotype: None,
        stereotype_span: None,
        classes: Vec::new(),
        actor_type: None,
        icon: None,
        business: None,
    };
    elements.set(end.named.id.clone(), state);
    r.register(&end.named, "usecase");
    r.mention(&end.named.id, end.named.span[0]);
    Ok("usecase")
}
