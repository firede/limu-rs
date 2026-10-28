//! What a C4 diagram's parser fills, as Pele's C4Db in db.ts and the types of types.ts have
//! it: its shapes, its boundaries and its relations.

use std::collections::HashMap;

use crate::js::chars::{drop_indents, trim_start};
use crate::js::number::parse_int;

/// parseInt(s) with no radix, as Pele calls it: a number written after 0x is read in base 16.
fn int(s: &str) -> f64 {
    let rest = trim_start(s);
    let rest = rest.strip_prefix(['+', '-']).unwrap_or(rest);
    if rest.starts_with("0x") || rest.starts_with("0X") {
        parse_int(s, 16)
    } else {
        parse_int(s, 10)
    }
}

/// A `$key="value"` argument, Pele's C4Named.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct C4Named {
    pub(crate) key: String,
    pub(crate) value: String,
}

/// An argument, Pele's C4Attr: a value written where it is, or a named one.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum C4Attr {
    Text(String),
    Named(C4Named),
}

/// The named arguments that set no known field, Pele's extra, in the order their keys were
/// first given. A later one of the same key replaces the value, as a Map's set does.
pub(crate) type Extra = Option<Vec<(String, String)>>;

/// Sets a key of extra, as Pele's `(target.extra ??= new Map()).set(key, value)` does.
fn set_extra(extra: &mut Extra, key: &str, value: &str) {
    let list = extra.get_or_insert_with(Vec::new);
    match list.iter_mut().find(|(k, _)| k == key) {
        Some(entry) => entry.1 = value.to_string(),
        None => list.push((key.to_string(), value.to_string())),
    }
}

/// What shapes and boundaries share, Pele's C4Element. What Pele leaves undefined is none.
/// Pele's type is r#type, and parent is the boundary's place among the boundaries.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct C4Element {
    pub(crate) alias: String,
    pub(crate) label: String,
    /// A named argument written where the alias or the label belongs. It is applied to the
    /// field it names.
    pub(crate) alias_attr: Option<C4Named>,
    pub(crate) label_attr: Option<C4Named>,
    pub(crate) parent: Option<usize>,
    pub(crate) r#type: Option<String>,
    pub(crate) descr: Option<String>,
    pub(crate) techn: Option<String>,
    pub(crate) sprite: Option<String>,
    pub(crate) tags: Option<String>,
    pub(crate) link: Option<String>,
    pub(crate) bg_color: Option<String>,
    pub(crate) font_color: Option<String>,
    pub(crate) border_color: Option<String>,
    pub(crate) shadowing: Option<String>,
    pub(crate) shape: Option<String>,
    pub(crate) legend_text: Option<String>,
    pub(crate) legend_sprite: Option<String>,
    pub(crate) extra: Extra,
}

/// A person, a system, a container or a component, Pele's C4Shape. Its kind is Mermaid's
/// typeC4Shape: person, external_system_db, container_queue and so on.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct C4Shape {
    pub(crate) el: C4Element,
    pub(crate) kind: String,
}

/// A boundary or a deployment node, Pele's C4Boundary. Its node type is set on deployment
/// nodes: node, nodeL or nodeR.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct C4Boundary {
    pub(crate) el: C4Element,
    pub(crate) node_type: Option<String>,
}

/// A relation, Pele's C4Rel. What Pele leaves undefined is none.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct C4Rel {
    pub(crate) r#type: String,
    pub(crate) from: String,
    pub(crate) to: String,
    pub(crate) label: String,
    pub(crate) from_attr: Option<C4Named>,
    pub(crate) to_attr: Option<C4Named>,
    pub(crate) label_attr: Option<C4Named>,
    pub(crate) techn: Option<String>,
    pub(crate) descr: Option<String>,
    pub(crate) sprite: Option<String>,
    pub(crate) tags: Option<String>,
    pub(crate) link: Option<String>,
    pub(crate) text_color: Option<String>,
    pub(crate) line_color: Option<String>,
    /// As parseInt reads them, NaN among them.
    pub(crate) offset_x: Option<f64>,
    pub(crate) offset_y: Option<f64>,
    pub(crate) extra: Extra,
}

/// An element or a relation, Pele's Target: what an argument sets a field of.
trait Target {
    /// The field of a name other than label, as Pele's ELEMENT_FIELDS or REL_FIELDS list
    /// them, or none for one not listed.
    fn field(&mut self, name: &str) -> Option<&mut Option<String>>;
    fn label(&mut self) -> &mut String;
    fn label_attr(&mut self) -> &mut Option<C4Named>;
    fn extra(&mut self) -> &mut Extra;
    /// Sets what a relation's offsetX or offsetY names, and says whether it did.
    fn offset(&mut self, _attr: &C4Named) -> bool {
        false
    }
}

impl Target for C4Element {
    fn field(&mut self, name: &str) -> Option<&mut Option<String>> {
        Some(match name {
            "type" => &mut self.r#type,
            "descr" => &mut self.descr,
            "techn" => &mut self.techn,
            "sprite" => &mut self.sprite,
            "tags" => &mut self.tags,
            "link" => &mut self.link,
            "bgColor" => &mut self.bg_color,
            "fontColor" => &mut self.font_color,
            "borderColor" => &mut self.border_color,
            "shadowing" => &mut self.shadowing,
            "shape" => &mut self.shape,
            "legendText" => &mut self.legend_text,
            "legendSprite" => &mut self.legend_sprite,
            _ => return None,
        })
    }

    fn label(&mut self) -> &mut String {
        &mut self.label
    }

    fn label_attr(&mut self) -> &mut Option<C4Named> {
        &mut self.label_attr
    }

    fn extra(&mut self) -> &mut Extra {
        &mut self.extra
    }
}

impl Target for C4Rel {
    fn field(&mut self, name: &str) -> Option<&mut Option<String>> {
        Some(match name {
            "descr" => &mut self.descr,
            "techn" => &mut self.techn,
            "sprite" => &mut self.sprite,
            "tags" => &mut self.tags,
            "link" => &mut self.link,
            "textColor" => &mut self.text_color,
            "lineColor" => &mut self.line_color,
            _ => return None,
        })
    }

    fn label(&mut self) -> &mut String {
        &mut self.label
    }

    fn label_attr(&mut self) -> &mut Option<C4Named> {
        &mut self.label_attr
    }

    fn extra(&mut self) -> &mut Extra {
        &mut self.extra
    }

    fn offset(&mut self, attr: &C4Named) -> bool {
        let slot = match attr.key.as_str() {
            "offsetX" => &mut self.offset_x,
            "offsetY" => &mut self.offset_y,
            _ => return false,
        };
        *slot = Some(int(&attr.value));
        true
    }
}

/// Applies a named argument, as Pele's named does. Mermaid assigns a named argument to
/// whatever property it names, structural ones included. Here it reaches only the fields of
/// the target, and any other name is kept aside in extra.
fn named(target: &mut impl Target, attr: &C4Named) {
    if target.offset(attr) {
        return;
    }
    if attr.key == "label" {
        *target.label() = attr.value.clone();
    } else if let Some(field) = target.field(&attr.key) {
        *field = Some(attr.value.clone());
    } else {
        set_extra(target.extra(), &attr.key, &attr.value);
    }
}

/// Applies an argument to the field of its slot, as Pele's put does: a positional one fills
/// that field, and a named one fills the field it names, whichever slot it is in.
fn put(target: &mut impl Target, field: &str, attr: Option<&C4Attr>) {
    match attr {
        None => {}
        Some(C4Attr::Text(s)) => {
            if let Some(f) = target.field(field) {
                *f = Some(s.clone());
            }
        }
        Some(C4Attr::Named(n)) => named(target, n),
    }
}

/// Applies the argument of a text's slot, as Pele's text does: where there is none, the text
/// is empty unless it is set.
fn text(target: &mut impl Target, field: &str, attr: Option<&C4Attr>) {
    match attr {
        None => {
            if let Some(f) = target.field(field) {
                f.get_or_insert_with(String::new);
            }
        }
        Some(_) => put(target, field, attr),
    }
}

/// Applies the argument of a label's slot, as Pele's label does.
fn label(target: &mut impl Target, attr: Option<&C4Attr>) {
    *target.label_attr() = None;
    match attr {
        Some(C4Attr::Named(n)) => {
            target.label().clear();
            *target.label_attr() = Some(n.clone());
            named(target, n);
        }
        Some(C4Attr::Text(s)) => *target.label() = s.clone(),
        None => target.label().clear(),
    }
}

/// A shape or a boundary, by its place in its list, as Pele's getC4Shape finds one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum ElementRef {
    Shape(usize),
    Boundary(usize),
}

/// A C4 diagram as its parser fills it, Pele's C4Db.
#[derive(Clone, Debug)]
pub(crate) struct C4Db {
    /// The header keyword: C4Context, C4Container, C4Component, C4Dynamic or C4Deployment.
    pub(crate) c4_type: Option<String>,
    pub(crate) title: Option<String>,
    /// Pele's accTitle, which nothing sets: the grammar hands `accTitle:` to setTitle.
    pub(crate) acc_title: Option<String>,
    pub(crate) acc_descr: Option<String>,
    pub(crate) shapes: Vec<C4Shape>,
    /// The first boundary is the unnamed one that holds everything at the top level.
    pub(crate) boundaries: Vec<C4Boundary>,
    pub(crate) rels: Vec<C4Rel>,
    /// As parseInt reads them, which may be past any integer.
    pub(crate) shape_in_row: f64,
    pub(crate) boundary_in_row: f64,

    shape_index: HashMap<String, usize>,
    // The top-level boundary is not listed, so a boundary a diagram names `global` is its own
    // box. In Mermaid it replaces the top level and nothing is drawn.
    boundary_index: HashMap<String, usize>,
    rel_index: HashMap<String, HashMap<String, usize>>,
    current: usize,
    stack: Vec<usize>,
}

impl Default for C4Db {
    fn default() -> Self {
        let global = C4Boundary {
            el: C4Element {
                alias: "global".into(),
                label: "global".into(),
                r#type: Some("global".into()),
                ..C4Element::default()
            },
            node_type: None,
        };
        C4Db {
            c4_type: None,
            title: None,
            acc_title: None,
            acc_descr: None,
            shapes: Vec::new(),
            boundaries: vec![global],
            rels: Vec::new(),
            shape_in_row: 4.0,
            boundary_in_row: 2.0,
            shape_index: HashMap::new(),
            boundary_index: HashMap::new(),
            rel_index: HashMap::new(),
            current: 0,
            stack: Vec::new(),
        }
    }
}

/// What has an element, a shape or a boundary, for Pele's element.
trait HasElement: Default {
    fn el(&mut self) -> &mut C4Element;
}

impl HasElement for C4Shape {
    fn el(&mut self) -> &mut C4Element {
        &mut self.el
    }
}

impl HasElement for C4Boundary {
    fn el(&mut self) -> &mut C4Element {
        &mut self.el
    }
}

/// The element of an alias, made where the list has none, with its label set, as Pele's
/// element finds or makes it. It gives the element's place in the list.
fn element<E: HasElement>(
    list: &mut Vec<E>,
    index: &mut HashMap<String, usize>,
    alias: &C4Attr,
    title: Option<&C4Attr>,
) -> usize {
    let found = match alias {
        C4Attr::Text(a) => index.get(a).copied(),
        C4Attr::Named(_) => None,
    };
    let at = match found {
        Some(at) => at,
        None => {
            let at = list.len();
            list.push(E::default());
            let el = list[at].el();
            match alias {
                C4Attr::Text(a) => {
                    el.alias = a.clone();
                    index.insert(a.clone(), at);
                }
                C4Attr::Named(n) => {
                    el.alias_attr = Some(n.clone());
                    named(el, n);
                }
            }
            at
        }
    };
    label(list[at].el(), title);
    at
}

impl C4Db {
    pub(crate) fn set_c4_type(&mut self, kind: &str) {
        self.c4_type = Some(kind.to_string());
    }

    pub(crate) fn set_title(&mut self, title: &str) {
        self.title = Some(title.to_string());
    }

    pub(crate) fn set_acc_description(&mut self, descr: &str) {
        self.acc_descr = Some(drop_indents(descr));
    }

    /// alias, label, ?descr, ?sprite, ?tags, $link
    pub(crate) fn add_person_or_system(&mut self, kind: &str, attrs: &[C4Attr]) {
        let at = element(
            &mut self.shapes,
            &mut self.shape_index,
            &attrs[0],
            attrs.get(1),
        );
        text(&mut self.shapes[at].el, "descr", attrs.get(2));
        self.finish_shape(at, kind, attrs, 3);
    }

    /// alias, label, ?techn, ?descr, ?sprite, ?tags, $link
    pub(crate) fn add_container(&mut self, kind: &str, attrs: &[C4Attr]) {
        let at = element(
            &mut self.shapes,
            &mut self.shape_index,
            &attrs[0],
            attrs.get(1),
        );
        text(&mut self.shapes[at].el, "techn", attrs.get(2));
        text(&mut self.shapes[at].el, "descr", attrs.get(3));
        self.finish_shape(at, kind, attrs, 4);
    }

    pub(crate) fn add_component(&mut self, kind: &str, attrs: &[C4Attr]) {
        self.add_container(kind, attrs);
    }

    fn finish_shape(&mut self, at: usize, kind: &str, attrs: &[C4Attr], from: usize) {
        let current = self.current;
        let shape = &mut self.shapes[at];
        put(&mut shape.el, "sprite", attrs.get(from));
        put(&mut shape.el, "tags", attrs.get(from + 1));
        put(&mut shape.el, "link", attrs.get(from + 2));
        shape.kind = kind.to_string();
        shape.el.parent = Some(current);
    }

    fn boundary(&mut self, attrs: &[C4Attr], fallback: &str) -> usize {
        let at = element(
            &mut self.boundaries,
            &mut self.boundary_index,
            &attrs[0],
            attrs.get(1),
        );
        let el = &mut self.boundaries[at].el;
        match attrs.get(2) {
            None => el.r#type = Some(fallback.to_string()),
            attr => put(el, "type", attr),
        }
        at
    }

    fn open(&mut self, boundary: usize) {
        self.boundaries[boundary].el.parent = Some(self.current);
        self.stack.push(self.current);
        self.current = boundary;
    }

    /// alias, label, ?type, ?tags, $link
    pub(crate) fn add_person_or_system_boundary(&mut self, attrs: &[C4Attr], fallback: &str) {
        let at = self.boundary(attrs, fallback);
        let el = &mut self.boundaries[at].el;
        put(el, "tags", attrs.get(3));
        put(el, "link", attrs.get(4));
        self.open(at);
    }

    pub(crate) fn add_container_boundary(&mut self, attrs: &[C4Attr]) {
        self.add_person_or_system_boundary(attrs, "container");
    }

    /// alias, label, ?type, ?descr, ?sprite, ?tags, $link
    pub(crate) fn add_deployment_node(&mut self, node_type: &str, attrs: &[C4Attr]) {
        let at = self.boundary(attrs, "node");
        let boundary = &mut self.boundaries[at];
        text(&mut boundary.el, "descr", attrs.get(3));
        // Mermaid drops whatever is in the sprite slot, a named argument included.
        put(&mut boundary.el, "sprite", attrs.get(4));
        put(&mut boundary.el, "tags", attrs.get(5));
        put(&mut boundary.el, "link", attrs.get(6));
        boundary.node_type = Some(node_type.to_string());
        self.open(at);
    }

    pub(crate) fn pop_boundary_parse_stack(&mut self) {
        self.current = self.stack.pop().unwrap_or(0);
    }

    /// from, to, label, ?techn, ?descr, ?sprite, ?tags, $link
    pub(crate) fn add_rel(&mut self, kind: &str, attrs: &[C4Attr]) {
        let (Some(from), Some(to), Some(_)) = (attrs.first(), attrs.get(1), attrs.get(2)) else {
            return;
        };
        let known = match (from, to) {
            (C4Attr::Text(f), C4Attr::Text(t)) => Some((f, t)),
            _ => None,
        };
        let found = known.and_then(|(f, t)| self.rel_index.get(f)?.get(t).copied());
        let at = match found {
            Some(at) => at,
            None => {
                let at = self.rels.len();
                let mut rel = C4Rel {
                    r#type: kind.to_string(),
                    ..C4Rel::default()
                };
                if let Some((f, t)) = known {
                    self.rel_index
                        .entry(f.clone())
                        .or_default()
                        .insert(t.clone(), at);
                }
                match from {
                    C4Attr::Text(f) => rel.from = f.clone(),
                    C4Attr::Named(n) => rel.from_attr = Some(n.clone()),
                }
                match to {
                    C4Attr::Text(t) => rel.to = t.clone(),
                    C4Attr::Named(n) => rel.to_attr = Some(n.clone()),
                }
                self.rels.push(rel);
                at
            }
        };
        let rel = &mut self.rels[at];
        rel.r#type = kind.to_string();
        label(rel, attrs.get(2));
        text(rel, "techn", attrs.get(3));
        text(rel, "descr", attrs.get(4));
        put(rel, "sprite", attrs.get(5));
        put(rel, "tags", attrs.get(6));
        put(rel, "link", attrs.get(7));
    }

    /// The shape or the boundary of an alias, as Pele's getC4Shape finds it.
    pub(crate) fn get_c4_shape(&self, alias: &str) -> Option<ElementRef> {
        self.shape_index
            .get(alias)
            .map(|&i| ElementRef::Shape(i))
            .or_else(|| {
                self.boundary_index
                    .get(alias)
                    .map(|&i| ElementRef::Boundary(i))
            })
    }

    /// elementName, ?bgColor, ?fontColor, ?borderColor, ?shadowing, ?shape, ?sprite, ?techn,
    /// ?legendText, ?legendSprite
    pub(crate) fn update_el_style(&mut self, attrs: &[C4Attr]) {
        let found = match &attrs[0] {
            C4Attr::Text(a) => self.get_c4_shape(a),
            C4Attr::Named(_) => None,
        };
        let el = match found {
            None => return,
            Some(ElementRef::Shape(i)) => &mut self.shapes[i].el,
            Some(ElementRef::Boundary(i)) => &mut self.boundaries[i].el,
        };
        for (k, field) in [
            "bgColor",
            "fontColor",
            "borderColor",
            "shadowing",
            "shape",
            "sprite",
            "techn",
            "legendText",
            "legendSprite",
        ]
        .into_iter()
        .enumerate()
        {
            put(el, field, attrs.get(k + 1));
        }
    }

    /// from, to, ?textColor, ?lineColor, ?offsetX, ?offsetY
    pub(crate) fn update_rel_style(&mut self, attrs: &[C4Attr]) {
        let found = match (attrs.first(), attrs.get(1)) {
            (Some(C4Attr::Text(f)), Some(C4Attr::Text(t))) => {
                self.rel_index.get(f).and_then(|m| m.get(t)).copied()
            }
            _ => None,
        };
        let Some(at) = found else {
            return;
        };
        let rel = &mut self.rels[at];
        put(rel, "textColor", attrs.get(2));
        put(rel, "lineColor", attrs.get(3));
        match attrs.get(4) {
            Some(C4Attr::Text(x)) => rel.offset_x = Some(int(x)),
            Some(C4Attr::Named(n)) => named(rel, n),
            None => {}
        }
        match attrs.get(5) {
            Some(C4Attr::Text(y)) => rel.offset_y = Some(int(y)),
            Some(C4Attr::Named(n)) => named(rel, n),
            None => {}
        }
    }

    /// ?c4ShapeInRow, ?c4BoundaryInRow. As in Mermaid, the slot decides which one a value
    /// sets, not its name.
    pub(crate) fn update_layout_config(&mut self, attrs: &[C4Attr]) {
        let count = |attr: Option<&C4Attr>| match attr {
            None => f64::NAN,
            Some(C4Attr::Text(s)) => int(s),
            Some(C4Attr::Named(n)) => int(&n.value),
        };
        let shapes = count(attrs.first());
        let boundaries = count(attrs.get(1));
        if shapes >= 1.0 {
            self.shape_in_row = shapes;
        }
        if boundaries >= 1.0 {
            self.boundary_in_row = boundaries;
        }
    }
}
