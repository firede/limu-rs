//! The roles of the items limu lays out, by the diagram they are of, as "flowchart.node". A
//! host styles items by role, keying [`Look::overrides`](crate::scene::style::Look) by these
//! names, and a host that draws items its own way tells them apart by role, as an item's
//! [`Role`](crate::scene::Role) equals the name it has.
//!
//! A diagram's module is built with the diagram's feature.
//!
//! ```
//! # #[cfg(feature = "flowchart")]
//! # {
//! use limu::roles::flowchart;
//! use limu::scene::style::{Color, Look, Paint, Style};
//!
//! // Fills a flowchart's nodes in a color of the host's.
//! let mut look = Look::default();
//! let fill = Paint::own(Color::hex("#fde68a"));
//! let style = Style { fill: Some(fill), ..Style::default() };
//! look.overrides.insert(flowchart::NODE.to_string(), style);
//! # }
//! ```

#[cfg(feature = "agentflow")]
pub mod agentflow {
    //! The roles of the items of an agent flow.

    /// "agentflow.action".
    pub const ACTION: &str = "agentflow.action";

    /// "agentflow.casing".
    pub const CASING: &str = "agentflow.casing";

    /// "agentflow.collapsed".
    pub const COLLAPSED: &str = "agentflow.collapsed";

    /// "agentflow.collapsed-dots".
    pub const COLLAPSED_DOTS: &str = "agentflow.collapsed-dots";

    /// "agentflow.collapsed-rule".
    pub const COLLAPSED_RULE: &str = "agentflow.collapsed-rule";

    /// "agentflow.connector".
    pub const CONNECTOR: &str = "agentflow.connector";

    /// "agentflow.decision".
    pub const DECISION: &str = "agentflow.decision";

    /// "agentflow.edge-label".
    pub const EDGE_LABEL: &str = "agentflow.edge-label";

    /// "agentflow.failure".
    pub const FAILURE: &str = "agentflow.failure";

    /// "agentflow.flow".
    pub const FLOW: &str = "agentflow.flow";

    /// "agentflow.input".
    pub const INPUT: &str = "agentflow.input";

    /// "agentflow.refdoc".
    pub const REFDOC: &str = "agentflow.refdoc";

    /// "agentflow.reference".
    pub const REFERENCE: &str = "agentflow.reference";

    /// "agentflow.sequence".
    pub const SEQUENCE: &str = "agentflow.sequence";

    /// "agentflow.task".
    pub const TASK: &str = "agentflow.task";

    /// "agentflow.title".
    pub const TITLE: &str = "agentflow.title";

    /// "agentflow.tool".
    pub const TOOL: &str = "agentflow.tool";

    /// Every role of the diagram.
    pub const ALL: &[&str] = &[
        ACTION,
        CASING,
        COLLAPSED,
        COLLAPSED_DOTS,
        COLLAPSED_RULE,
        CONNECTOR,
        DECISION,
        EDGE_LABEL,
        FAILURE,
        FLOW,
        INPUT,
        REFDOC,
        REFERENCE,
        SEQUENCE,
        TASK,
        TITLE,
        TOOL,
    ];
}

#[cfg(feature = "architecture")]
pub mod architecture {
    //! The roles of the items of an architecture diagram.

    /// "architecture.edge".
    pub const EDGE: &str = "architecture.edge";

    /// "architecture.edge-label".
    pub const EDGE_LABEL: &str = "architecture.edge-label";

    /// "architecture.group".
    pub const GROUP: &str = "architecture.group";

    /// "architecture.icon".
    pub const ICON: &str = "architecture.icon";

    /// "architecture.icon-text".
    pub const ICON_TEXT: &str = "architecture.icon-text";

    /// "architecture.junction".
    pub const JUNCTION: &str = "architecture.junction";

    /// "architecture.service".
    pub const SERVICE: &str = "architecture.service";

    /// "architecture.title".
    pub const TITLE: &str = "architecture.title";

    /// Every role of the diagram.
    pub const ALL: &[&str] = &[
        EDGE, EDGE_LABEL, GROUP, ICON, ICON_TEXT, JUNCTION, SERVICE, TITLE,
    ];
}

#[cfg(feature = "block")]
pub mod block {
    //! The roles of the items of a block diagram.

    /// "block.block".
    pub const BLOCK: &str = "block.block";

    /// "block.casing".
    pub const CASING: &str = "block.casing";

    /// "block.composite".
    pub const COMPOSITE: &str = "block.composite";

    /// "block.edge".
    pub const EDGE: &str = "block.edge";

    /// "block.edge-label".
    pub const EDGE_LABEL: &str = "block.edge-label";

    /// "block.title".
    pub const TITLE: &str = "block.title";

    /// Every role of the diagram.
    pub const ALL: &[&str] = &[BLOCK, CASING, COMPOSITE, EDGE, EDGE_LABEL, TITLE];
}

#[cfg(feature = "c4")]
pub mod c4 {
    //! The roles of the items of a C4 diagram.

    /// "c4.boundary".
    pub const BOUNDARY: &str = "c4.boundary";

    /// "c4.boundary-descr".
    pub const BOUNDARY_DESCR: &str = "c4.boundary-descr";

    /// "c4.boundary-label".
    pub const BOUNDARY_LABEL: &str = "c4.boundary-label";

    /// "c4.boundary-type".
    pub const BOUNDARY_TYPE: &str = "c4.boundary-type";

    /// "c4.casing".
    pub const CASING: &str = "c4.casing";

    /// "c4.deployment".
    pub const DEPLOYMENT: &str = "c4.deployment";

    /// "c4.element".
    pub const ELEMENT: &str = "c4.element";

    /// "c4.element-descr".
    pub const ELEMENT_DESCR: &str = "c4.element-descr";

    /// "c4.element-techn".
    pub const ELEMENT_TECHN: &str = "c4.element-techn";

    /// "c4.element-type".
    pub const ELEMENT_TYPE: &str = "c4.element-type";

    /// "c4.rel".
    pub const REL: &str = "c4.rel";

    /// "c4.rel-label".
    pub const REL_LABEL: &str = "c4.rel-label";

    /// "c4.rel-techn".
    pub const REL_TECHN: &str = "c4.rel-techn";

    /// "c4.title".
    pub const TITLE: &str = "c4.title";

    /// Every role of the diagram.
    pub const ALL: &[&str] = &[
        BOUNDARY,
        BOUNDARY_DESCR,
        BOUNDARY_LABEL,
        BOUNDARY_TYPE,
        CASING,
        DEPLOYMENT,
        ELEMENT,
        ELEMENT_DESCR,
        ELEMENT_TECHN,
        ELEMENT_TYPE,
        REL,
        REL_LABEL,
        REL_TECHN,
        TITLE,
    ];
}

#[cfg(feature = "class")]
pub mod class {
    //! The roles of the items of a class diagram.

    /// "class.annotation".
    pub const ANNOTATION: &str = "class.annotation";

    /// "class.attribute".
    pub const ATTRIBUTE: &str = "class.attribute";

    /// "class.cardinality".
    pub const CARDINALITY: &str = "class.cardinality";

    /// "class.class".
    pub const CLASS: &str = "class.class";

    /// "class.divider".
    pub const DIVIDER: &str = "class.divider";

    /// "class.edge-label".
    pub const EDGE_LABEL: &str = "class.edge-label";

    /// "class.interface".
    pub const INTERFACE: &str = "class.interface";

    /// "class.method".
    pub const METHOD: &str = "class.method";

    /// "class.namespace".
    pub const NAMESPACE: &str = "class.namespace";

    /// "class.namespace-title".
    pub const NAMESPACE_TITLE: &str = "class.namespace-title";

    /// "class.note".
    pub const NOTE: &str = "class.note";

    /// "class.note-edge".
    pub const NOTE_EDGE: &str = "class.note-edge";

    /// "class.relation".
    pub const RELATION: &str = "class.relation";

    /// "class.title".
    pub const TITLE: &str = "class.title";

    /// Every role of the diagram.
    pub const ALL: &[&str] = &[
        ANNOTATION,
        ATTRIBUTE,
        CARDINALITY,
        CLASS,
        DIVIDER,
        EDGE_LABEL,
        INTERFACE,
        METHOD,
        NAMESPACE,
        NAMESPACE_TITLE,
        NOTE,
        NOTE_EDGE,
        RELATION,
        TITLE,
    ];
}

#[cfg(feature = "cynefin")]
pub mod cynefin {
    //! The roles of the items of a Cynefin diagram.

    /// "cynefin.backdrop".
    pub const BACKDROP: &str = "cynefin.backdrop";

    /// "cynefin.boundary".
    pub const BOUNDARY: &str = "cynefin.boundary";

    /// "cynefin.cliff".
    pub const CLIFF: &str = "cynefin.cliff";

    /// "cynefin.description".
    pub const DESCRIPTION: &str = "cynefin.description";

    /// "cynefin.domain".
    pub const DOMAIN: &str = "cynefin.domain";

    /// "cynefin.item".
    pub const ITEM: &str = "cynefin.item";

    /// "cynefin.overflow".
    pub const OVERFLOW: &str = "cynefin.overflow";

    /// "cynefin.region".
    pub const REGION: &str = "cynefin.region";

    /// "cynefin.title".
    pub const TITLE: &str = "cynefin.title";

    /// "cynefin.transition".
    pub const TRANSITION: &str = "cynefin.transition";

    /// "cynefin.transition-label".
    pub const TRANSITION_LABEL: &str = "cynefin.transition-label";

    /// Every role of the diagram.
    pub const ALL: &[&str] = &[
        BACKDROP,
        BOUNDARY,
        CLIFF,
        DESCRIPTION,
        DOMAIN,
        ITEM,
        OVERFLOW,
        REGION,
        TITLE,
        TRANSITION,
        TRANSITION_LABEL,
    ];
}

#[cfg(feature = "er")]
pub mod er {
    //! The roles of the items of an entity relationship diagram.

    /// "er.attribute-comment".
    pub const ATTRIBUTE_COMMENT: &str = "er.attribute-comment";

    /// "er.attribute-keys".
    pub const ATTRIBUTE_KEYS: &str = "er.attribute-keys";

    /// "er.attribute-name".
    pub const ATTRIBUTE_NAME: &str = "er.attribute-name";

    /// "er.attribute-type".
    pub const ATTRIBUTE_TYPE: &str = "er.attribute-type";

    /// "er.casing".
    pub const CASING: &str = "er.casing";

    /// "er.cluster".
    pub const CLUSTER: &str = "er.cluster";

    /// "er.divider".
    pub const DIVIDER: &str = "er.divider";

    /// "er.entity".
    pub const ENTITY: &str = "er.entity";

    /// "er.relationship".
    pub const RELATIONSHIP: &str = "er.relationship";

    /// "er.relationship-label".
    pub const RELATIONSHIP_LABEL: &str = "er.relationship-label";

    /// "er.title".
    pub const TITLE: &str = "er.title";

    /// Every role of the diagram.
    pub const ALL: &[&str] = &[
        ATTRIBUTE_COMMENT,
        ATTRIBUTE_KEYS,
        ATTRIBUTE_NAME,
        ATTRIBUTE_TYPE,
        CASING,
        CLUSTER,
        DIVIDER,
        ENTITY,
        RELATIONSHIP,
        RELATIONSHIP_LABEL,
        TITLE,
    ];
}

#[cfg(feature = "eventmodeling")]
pub mod eventmodeling {
    //! The roles of the items of an event model.

    /// "eventmodeling.command".
    pub const COMMAND: &str = "eventmodeling.command";

    /// "eventmodeling.data".
    pub const DATA: &str = "eventmodeling.data";

    /// "eventmodeling.event".
    pub const EVENT: &str = "eventmodeling.event";

    /// "eventmodeling.frame-id".
    pub const FRAME_ID: &str = "eventmodeling.frame-id";

    /// "eventmodeling.lane".
    pub const LANE: &str = "eventmodeling.lane";

    /// "eventmodeling.lane-label".
    pub const LANE_LABEL: &str = "eventmodeling.lane-label";

    /// "eventmodeling.note".
    pub const NOTE: &str = "eventmodeling.note";

    /// "eventmodeling.note-text".
    pub const NOTE_TEXT: &str = "eventmodeling.note-text";

    /// "eventmodeling.processor".
    pub const PROCESSOR: &str = "eventmodeling.processor";

    /// "eventmodeling.readmodel".
    pub const READMODEL: &str = "eventmodeling.readmodel";

    /// "eventmodeling.relation".
    pub const RELATION: &str = "eventmodeling.relation";

    /// "eventmodeling.specification".
    pub const SPECIFICATION: &str = "eventmodeling.specification";

    /// "eventmodeling.specification-text".
    pub const SPECIFICATION_TEXT: &str = "eventmodeling.specification-text";

    /// "eventmodeling.title".
    pub const TITLE: &str = "eventmodeling.title";

    /// "eventmodeling.ui".
    pub const UI: &str = "eventmodeling.ui";

    /// Every role of the diagram.
    pub const ALL: &[&str] = &[
        COMMAND,
        DATA,
        EVENT,
        FRAME_ID,
        LANE,
        LANE_LABEL,
        NOTE,
        NOTE_TEXT,
        PROCESSOR,
        READMODEL,
        RELATION,
        SPECIFICATION,
        SPECIFICATION_TEXT,
        TITLE,
        UI,
    ];
}

#[cfg(feature = "flowchart")]
pub mod flowchart {
    //! The roles of the items of a flowchart.

    /// "flowchart.casing".
    pub const CASING: &str = "flowchart.casing";

    /// "flowchart.cluster".
    pub const CLUSTER: &str = "flowchart.cluster";

    /// "flowchart.cluster-title".
    pub const CLUSTER_TITLE: &str = "flowchart.cluster-title";

    /// "flowchart.edge".
    pub const EDGE: &str = "flowchart.edge";

    /// "flowchart.edge-label".
    pub const EDGE_LABEL: &str = "flowchart.edge-label";

    /// "flowchart.node".
    pub const NODE: &str = "flowchart.node";

    /// "flowchart.title".
    pub const TITLE: &str = "flowchart.title";

    /// Every role of the diagram.
    pub const ALL: &[&str] = &[
        CASING,
        CLUSTER,
        CLUSTER_TITLE,
        EDGE,
        EDGE_LABEL,
        NODE,
        TITLE,
    ];
}

#[cfg(feature = "gantt")]
pub mod gantt {
    //! The roles of the items of a Gantt chart.

    /// "gantt.axis".
    pub const AXIS: &str = "gantt.axis";

    /// "gantt.excluded".
    pub const EXCLUDED: &str = "gantt.excluded";

    /// "gantt.grid".
    pub const GRID: &str = "gantt.grid";

    /// "gantt.section".
    pub const SECTION: &str = "gantt.section";

    /// "gantt.section-label".
    pub const SECTION_LABEL: &str = "gantt.section-label";

    /// "gantt.task".
    pub const TASK: &str = "gantt.task";

    /// "gantt.tick".
    pub const TICK: &str = "gantt.tick";

    /// "gantt.title".
    pub const TITLE: &str = "gantt.title";

    /// "gantt.today".
    pub const TODAY: &str = "gantt.today";

    /// "gantt.vert".
    pub const VERT: &str = "gantt.vert";

    /// "gantt.vert-label".
    pub const VERT_LABEL: &str = "gantt.vert-label";

    /// Every role of the diagram.
    pub const ALL: &[&str] = &[
        AXIS,
        EXCLUDED,
        GRID,
        SECTION,
        SECTION_LABEL,
        TASK,
        TICK,
        TITLE,
        TODAY,
        VERT,
        VERT_LABEL,
    ];
}

#[cfg(feature = "gitgraph")]
pub mod gitgraph {
    //! The roles of the items of a git graph.

    /// "gitgraph.branch".
    pub const BRANCH: &str = "gitgraph.branch";

    /// "gitgraph.commit".
    pub const COMMIT: &str = "gitgraph.commit";

    /// "gitgraph.commit-label".
    pub const COMMIT_LABEL: &str = "gitgraph.commit-label";

    /// "gitgraph.commit-mark".
    pub const COMMIT_MARK: &str = "gitgraph.commit-mark";

    /// "gitgraph.edge".
    pub const EDGE: &str = "gitgraph.edge";

    /// "gitgraph.lane".
    pub const LANE: &str = "gitgraph.lane";

    /// "gitgraph.merge".
    pub const MERGE: &str = "gitgraph.merge";

    /// "gitgraph.tag".
    pub const TAG: &str = "gitgraph.tag";

    /// "gitgraph.tag-hole".
    pub const TAG_HOLE: &str = "gitgraph.tag-hole";

    /// "gitgraph.title".
    pub const TITLE: &str = "gitgraph.title";

    /// Every role of the diagram.
    pub const ALL: &[&str] = &[
        BRANCH,
        COMMIT,
        COMMIT_LABEL,
        COMMIT_MARK,
        EDGE,
        LANE,
        MERGE,
        TAG,
        TAG_HOLE,
        TITLE,
    ];
}

#[cfg(feature = "info")]
pub mod info {
    //! The roles of the items of an info diagram.

    /// "info.title".
    pub const TITLE: &str = "info.title";

    /// "info.version".
    pub const VERSION: &str = "info.version";

    /// Every role of the diagram.
    pub const ALL: &[&str] = &[TITLE, VERSION];
}

#[cfg(feature = "ishikawa")]
pub mod ishikawa {
    //! The roles of the items of an Ishikawa diagram.

    /// "ishikawa.bones".
    pub const BONES: &str = "ishikawa.bones";

    /// "ishikawa.category".
    pub const CATEGORY: &str = "ishikawa.category";

    /// "ishikawa.cause".
    pub const CAUSE: &str = "ishikawa.cause";

    /// "ishikawa.head".
    pub const HEAD: &str = "ishikawa.head";

    /// "ishikawa.link".
    pub const LINK: &str = "ishikawa.link";

    /// "ishikawa.spine".
    pub const SPINE: &str = "ishikawa.spine";

    /// "ishikawa.title".
    pub const TITLE: &str = "ishikawa.title";

    /// Every role of the diagram.
    pub const ALL: &[&str] = &[BONES, CATEGORY, CAUSE, HEAD, LINK, SPINE, TITLE];
}

#[cfg(feature = "journey")]
pub mod journey {
    //! The roles of the items of a user journey.

    /// "journey.actor".
    pub const ACTOR: &str = "journey.actor";

    /// "journey.face".
    pub const FACE: &str = "journey.face";

    /// "journey.grid".
    pub const GRID: &str = "journey.grid";

    /// "journey.legend".
    pub const LEGEND: &str = "journey.legend";

    /// "journey.line".
    pub const LINE: &str = "journey.line";

    /// "journey.scale".
    pub const SCALE: &str = "journey.scale";

    /// "journey.score".
    pub const SCORE: &str = "journey.score";

    /// "journey.section".
    pub const SECTION: &str = "journey.section";

    /// "journey.section-label".
    pub const SECTION_LABEL: &str = "journey.section-label";

    /// "journey.stem".
    pub const STEM: &str = "journey.stem";

    /// "journey.task".
    pub const TASK: &str = "journey.task";

    /// "journey.title".
    pub const TITLE: &str = "journey.title";

    /// Every role of the diagram.
    pub const ALL: &[&str] = &[
        ACTOR,
        FACE,
        GRID,
        LEGEND,
        LINE,
        SCALE,
        SCORE,
        SECTION,
        SECTION_LABEL,
        STEM,
        TASK,
        TITLE,
    ];
}

#[cfg(feature = "kanban")]
pub mod kanban {
    //! The roles of the items of a Kanban board.

    /// "kanban.assigned".
    pub const ASSIGNED: &str = "kanban.assigned";

    /// "kanban.card".
    pub const CARD: &str = "kanban.card";

    /// "kanban.column".
    pub const COLUMN: &str = "kanban.column";

    /// "kanban.count".
    pub const COUNT: &str = "kanban.count";

    /// "kanban.priority".
    pub const PRIORITY: &str = "kanban.priority";

    /// "kanban.ticket".
    pub const TICKET: &str = "kanban.ticket";

    /// "kanban.title".
    pub const TITLE: &str = "kanban.title";

    /// Every role of the diagram.
    pub const ALL: &[&str] = &[ASSIGNED, CARD, COLUMN, COUNT, PRIORITY, TICKET, TITLE];
}

#[cfg(feature = "mindmap")]
pub mod mindmap {
    //! The roles of the items of a mind map.

    /// "mindmap.edge".
    pub const EDGE: &str = "mindmap.edge";

    /// "mindmap.node".
    pub const NODE: &str = "mindmap.node";

    /// "mindmap.root".
    pub const ROOT: &str = "mindmap.root";

    /// "mindmap.title".
    pub const TITLE: &str = "mindmap.title";

    /// Every role of the diagram.
    pub const ALL: &[&str] = &[EDGE, NODE, ROOT, TITLE];
}

#[cfg(feature = "packet")]
pub mod packet {
    //! The roles of the items of a packet diagram.

    /// "packet.bit".
    pub const BIT: &str = "packet.bit";

    /// "packet.field".
    pub const FIELD: &str = "packet.field";

    /// "packet.title".
    pub const TITLE: &str = "packet.title";

    /// Every role of the diagram.
    pub const ALL: &[&str] = &[BIT, FIELD, TITLE];
}

#[cfg(feature = "pie")]
pub mod pie {
    //! The roles of the items of a pie chart.

    /// "pie.legend".
    pub const LEGEND: &str = "pie.legend";

    /// "pie.slice".
    pub const SLICE: &str = "pie.slice";

    /// "pie.slice-label".
    pub const SLICE_LABEL: &str = "pie.slice-label";

    /// "pie.title".
    pub const TITLE: &str = "pie.title";

    /// Every role of the diagram.
    pub const ALL: &[&str] = &[LEGEND, SLICE, SLICE_LABEL, TITLE];
}

#[cfg(feature = "quadrant")]
pub mod quadrant {
    //! The roles of the items of a quadrant chart.

    /// "quadrant.axis".
    pub const AXIS: &str = "quadrant.axis";

    /// "quadrant.point".
    pub const POINT: &str = "quadrant.point";

    /// "quadrant.point-label".
    pub const POINT_LABEL: &str = "quadrant.point-label";

    /// "quadrant.quadrant".
    pub const QUADRANT: &str = "quadrant.quadrant";

    /// "quadrant.title".
    pub const TITLE: &str = "quadrant.title";

    /// Every role of the diagram.
    pub const ALL: &[&str] = &[AXIS, POINT, POINT_LABEL, QUADRANT, TITLE];
}

#[cfg(feature = "radar")]
pub mod radar {
    //! The roles of the items of a radar chart.

    /// "radar.axes".
    pub const AXES: &str = "radar.axes";

    /// "radar.axis-label".
    pub const AXIS_LABEL: &str = "radar.axis-label";

    /// "radar.curve".
    pub const CURVE: &str = "radar.curve";

    /// "radar.graticule".
    pub const GRATICULE: &str = "radar.graticule";

    /// "radar.legend".
    pub const LEGEND: &str = "radar.legend";

    /// "radar.title".
    pub const TITLE: &str = "radar.title";

    /// Every role of the diagram.
    pub const ALL: &[&str] = &[AXES, AXIS_LABEL, CURVE, GRATICULE, LEGEND, TITLE];
}

#[cfg(feature = "railroad")]
pub mod railroad {
    //! The roles of the items of a railroad diagram.

    /// "railroad.arrows".
    pub const ARROWS: &str = "railroad.arrows";

    /// "railroad.count".
    pub const COUNT: &str = "railroad.count";

    /// "railroad.end".
    pub const END: &str = "railroad.end";

    /// "railroad.name".
    pub const NAME: &str = "railroad.name";

    /// "railroad.nonterminal".
    pub const NONTERMINAL: &str = "railroad.nonterminal";

    /// "railroad.special".
    pub const SPECIAL: &str = "railroad.special";

    /// "railroad.special-mark".
    pub const SPECIAL_MARK: &str = "railroad.special-mark";

    /// "railroad.start".
    pub const START: &str = "railroad.start";

    /// "railroad.terminal".
    pub const TERMINAL: &str = "railroad.terminal";

    /// "railroad.title".
    pub const TITLE: &str = "railroad.title";

    /// "railroad.track".
    pub const TRACK: &str = "railroad.track";

    /// Every role of the diagram.
    pub const ALL: &[&str] = &[
        ARROWS,
        COUNT,
        END,
        NAME,
        NONTERMINAL,
        SPECIAL,
        SPECIAL_MARK,
        START,
        TERMINAL,
        TITLE,
        TRACK,
    ];
}

#[cfg(feature = "requirement")]
pub mod requirement {
    //! The roles of the items of a requirement diagram.

    /// "requirement.element".
    pub const ELEMENT: &str = "requirement.element";

    /// "requirement.field".
    pub const FIELD: &str = "requirement.field";

    /// "requirement.kind".
    pub const KIND: &str = "requirement.kind";

    /// "requirement.relationship".
    pub const RELATIONSHIP: &str = "requirement.relationship";

    /// "requirement.relationship-label".
    pub const RELATIONSHIP_LABEL: &str = "requirement.relationship-label";

    /// "requirement.requirement".
    pub const REQUIREMENT: &str = "requirement.requirement";

    /// "requirement.title".
    pub const TITLE: &str = "requirement.title";

    /// "requirement.value".
    pub const VALUE: &str = "requirement.value";

    /// Every role of the diagram.
    pub const ALL: &[&str] = &[
        ELEMENT,
        FIELD,
        KIND,
        RELATIONSHIP,
        RELATIONSHIP_LABEL,
        REQUIREMENT,
        TITLE,
        VALUE,
    ];
}

#[cfg(feature = "sankey")]
pub mod sankey {
    //! The roles of the items of a Sankey diagram.

    /// "sankey.label".
    pub const LABEL: &str = "sankey.label";

    /// "sankey.link".
    pub const LINK: &str = "sankey.link";

    /// "sankey.node".
    pub const NODE: &str = "sankey.node";

    /// "sankey.title".
    pub const TITLE: &str = "sankey.title";

    /// "sankey.value".
    pub const VALUE: &str = "sankey.value";

    /// Every role of the diagram.
    pub const ALL: &[&str] = &[LABEL, LINK, NODE, TITLE, VALUE];
}

#[cfg(feature = "sequence")]
pub mod sequence {
    //! The roles of the items of a sequence diagram.

    /// "sequence.activation".
    pub const ACTIVATION: &str = "sequence.activation";

    /// "sequence.actor".
    pub const ACTOR: &str = "sequence.actor";

    /// "sequence.box".
    pub const BOX: &str = "sequence.box";

    /// "sequence.condition".
    pub const CONDITION: &str = "sequence.condition";

    /// "sequence.destroy".
    pub const DESTROY: &str = "sequence.destroy";

    /// "sequence.divider".
    pub const DIVIDER: &str = "sequence.divider";

    /// "sequence.frame".
    pub const FRAME: &str = "sequence.frame";

    /// "sequence.frame-kind".
    pub const FRAME_KIND: &str = "sequence.frame-kind";

    /// "sequence.lifeline".
    pub const LIFELINE: &str = "sequence.lifeline";

    /// "sequence.message".
    pub const MESSAGE: &str = "sequence.message";

    /// "sequence.message-label".
    pub const MESSAGE_LABEL: &str = "sequence.message-label";

    /// "sequence.note".
    pub const NOTE: &str = "sequence.note";

    /// "sequence.number".
    pub const NUMBER: &str = "sequence.number";

    /// "sequence.rect".
    pub const RECT: &str = "sequence.rect";

    /// "sequence.title".
    pub const TITLE: &str = "sequence.title";

    /// Every role of the diagram.
    pub const ALL: &[&str] = &[
        ACTIVATION,
        ACTOR,
        BOX,
        CONDITION,
        DESTROY,
        DIVIDER,
        FRAME,
        FRAME_KIND,
        LIFELINE,
        MESSAGE,
        MESSAGE_LABEL,
        NOTE,
        NUMBER,
        RECT,
        TITLE,
    ];
}

#[cfg(feature = "state")]
pub mod state {
    //! The roles of the items of a state diagram.

    /// "state.casing".
    pub const CASING: &str = "state.casing";

    /// "state.choice".
    pub const CHOICE: &str = "state.choice";

    /// "state.composite".
    pub const COMPOSITE: &str = "state.composite";

    /// "state.description".
    pub const DESCRIPTION: &str = "state.description";

    /// "state.divider".
    pub const DIVIDER: &str = "state.divider";

    /// "state.edge-label".
    pub const EDGE_LABEL: &str = "state.edge-label";

    /// "state.end".
    pub const END: &str = "state.end";

    /// "state.fork".
    pub const FORK: &str = "state.fork";

    /// "state.join".
    pub const JOIN: &str = "state.join";

    /// "state.note".
    pub const NOTE: &str = "state.note";

    /// "state.note-edge".
    pub const NOTE_EDGE: &str = "state.note-edge";

    /// "state.start".
    pub const START: &str = "state.start";

    /// "state.state".
    pub const STATE: &str = "state.state";

    /// "state.title".
    pub const TITLE: &str = "state.title";

    /// "state.transition".
    pub const TRANSITION: &str = "state.transition";

    /// Every role of the diagram.
    pub const ALL: &[&str] = &[
        CASING,
        CHOICE,
        COMPOSITE,
        DESCRIPTION,
        DIVIDER,
        EDGE_LABEL,
        END,
        FORK,
        JOIN,
        NOTE,
        NOTE_EDGE,
        START,
        STATE,
        TITLE,
        TRANSITION,
    ];
}

#[cfg(feature = "swimlane")]
pub mod swimlane {
    //! The roles of the items of a swimlane diagram.

    /// "swimlane.casing".
    pub const CASING: &str = "swimlane.casing";

    /// "swimlane.cluster".
    pub const CLUSTER: &str = "swimlane.cluster";

    /// "swimlane.cluster-title".
    pub const CLUSTER_TITLE: &str = "swimlane.cluster-title";

    /// "swimlane.edge".
    pub const EDGE: &str = "swimlane.edge";

    /// "swimlane.edge-label".
    pub const EDGE_LABEL: &str = "swimlane.edge-label";

    /// "swimlane.lane".
    pub const LANE: &str = "swimlane.lane";

    /// "swimlane.node".
    pub const NODE: &str = "swimlane.node";

    /// "swimlane.title".
    pub const TITLE: &str = "swimlane.title";

    /// Every role of the diagram.
    pub const ALL: &[&str] = &[
        CASING,
        CLUSTER,
        CLUSTER_TITLE,
        EDGE,
        EDGE_LABEL,
        LANE,
        NODE,
        TITLE,
    ];
}

#[cfg(feature = "timeline")]
pub mod timeline {
    //! The roles of the items of a timeline.

    /// "timeline.axis".
    pub const AXIS: &str = "timeline.axis";

    /// "timeline.connector".
    pub const CONNECTOR: &str = "timeline.connector";

    /// "timeline.dot".
    pub const DOT: &str = "timeline.dot";

    /// "timeline.event".
    pub const EVENT: &str = "timeline.event";

    /// "timeline.period".
    pub const PERIOD: &str = "timeline.period";

    /// "timeline.section".
    pub const SECTION: &str = "timeline.section";

    /// "timeline.title".
    pub const TITLE: &str = "timeline.title";

    /// Every role of the diagram.
    pub const ALL: &[&str] = &[AXIS, CONNECTOR, DOT, EVENT, PERIOD, SECTION, TITLE];
}

#[cfg(feature = "treemap")]
pub mod treemap {
    //! The roles of the items of a treemap.

    /// "treemap.leaf".
    pub const LEAF: &str = "treemap.leaf";

    /// "treemap.section".
    pub const SECTION: &str = "treemap.section";

    /// "treemap.title".
    pub const TITLE: &str = "treemap.title";

    /// "treemap.value".
    pub const VALUE: &str = "treemap.value";

    /// Every role of the diagram.
    pub const ALL: &[&str] = &[LEAF, SECTION, TITLE, VALUE];
}

#[cfg(feature = "treeview")]
pub mod treeview {
    //! The roles of the items of a tree view.

    /// "treeview.description".
    pub const DESCRIPTION: &str = "treeview.description";

    /// "treeview.directory".
    pub const DIRECTORY: &str = "treeview.directory";

    /// "treeview.file".
    pub const FILE: &str = "treeview.file";

    /// "treeview.guide".
    pub const GUIDE: &str = "treeview.guide";

    /// "treeview.icon".
    pub const ICON: &str = "treeview.icon";

    /// "treeview.title".
    pub const TITLE: &str = "treeview.title";

    /// Every role of the diagram.
    pub const ALL: &[&str] = &[DESCRIPTION, DIRECTORY, FILE, GUIDE, ICON, TITLE];
}

#[cfg(feature = "usecase")]
pub mod usecase {
    //! The roles of the items of a use case diagram.

    /// "usecase.actor".
    pub const ACTOR: &str = "usecase.actor";

    /// "usecase.casing".
    pub const CASING: &str = "usecase.casing";

    /// "usecase.edge-label".
    pub const EDGE_LABEL: &str = "usecase.edge-label";

    /// "usecase.json".
    pub const JSON: &str = "usecase.json";

    /// "usecase.json-key".
    pub const JSON_KEY: &str = "usecase.json-key";

    /// "usecase.json-value".
    pub const JSON_VALUE: &str = "usecase.json-value";

    /// "usecase.note".
    pub const NOTE: &str = "usecase.note";

    /// "usecase.note-link".
    pub const NOTE_LINK: &str = "usecase.note-link";

    /// "usecase.relation".
    pub const RELATION: &str = "usecase.relation";

    /// "usecase.stereotype".
    pub const STEREOTYPE: &str = "usecase.stereotype";

    /// "usecase.system".
    pub const SYSTEM: &str = "usecase.system";

    /// "usecase.title".
    pub const TITLE: &str = "usecase.title";

    /// "usecase.usecase".
    pub const USECASE: &str = "usecase.usecase";

    /// Every role of the diagram.
    pub const ALL: &[&str] = &[
        ACTOR, CASING, EDGE_LABEL, JSON, JSON_KEY, JSON_VALUE, NOTE, NOTE_LINK, RELATION,
        STEREOTYPE, SYSTEM, TITLE, USECASE,
    ];
}

#[cfg(feature = "venn")]
pub mod venn {
    //! The roles of the items of a Venn diagram.

    /// "venn.label".
    pub const LABEL: &str = "venn.label";

    /// "venn.overlap".
    pub const OVERLAP: &str = "venn.overlap";

    /// "venn.set".
    pub const SET: &str = "venn.set";

    /// "venn.text".
    pub const TEXT: &str = "venn.text";

    /// "venn.title".
    pub const TITLE: &str = "venn.title";

    /// Every role of the diagram.
    pub const ALL: &[&str] = &[LABEL, OVERLAP, SET, TEXT, TITLE];
}

#[cfg(feature = "wardley")]
pub mod wardley {
    //! The roles of the items of a Wardley map.

    /// "wardley.accelerator".
    pub const ACCELERATOR: &str = "wardley.accelerator";

    /// "wardley.anchor".
    pub const ANCHOR: &str = "wardley.anchor";

    /// "wardley.annotation".
    pub const ANNOTATION: &str = "wardley.annotation";

    /// "wardley.annotation-text".
    pub const ANNOTATION_TEXT: &str = "wardley.annotation-text";

    /// "wardley.annotations".
    pub const ANNOTATIONS: &str = "wardley.annotations";

    /// "wardley.axis".
    pub const AXIS: &str = "wardley.axis";

    /// "wardley.axis-label".
    pub const AXIS_LABEL: &str = "wardley.axis-label";

    /// "wardley.component".
    pub const COMPONENT: &str = "wardley.component";

    /// "wardley.deaccelerator".
    pub const DEACCELERATOR: &str = "wardley.deaccelerator";

    /// "wardley.grid".
    pub const GRID: &str = "wardley.grid";

    /// "wardley.inertia".
    pub const INERTIA: &str = "wardley.inertia";

    /// "wardley.label".
    pub const LABEL: &str = "wardley.label";

    /// "wardley.link".
    pub const LINK: &str = "wardley.link";

    /// "wardley.link-label".
    pub const LINK_LABEL: &str = "wardley.link-label";

    /// "wardley.note".
    pub const NOTE: &str = "wardley.note";

    /// "wardley.pipeline".
    pub const PIPELINE: &str = "wardley.pipeline";

    /// "wardley.pipeline-component".
    pub const PIPELINE_COMPONENT: &str = "wardley.pipeline-component";

    /// "wardley.pipeline-line".
    pub const PIPELINE_LINE: &str = "wardley.pipeline-line";

    /// "wardley.pipeline-parent".
    pub const PIPELINE_PARENT: &str = "wardley.pipeline-parent";

    /// "wardley.stage".
    pub const STAGE: &str = "wardley.stage";

    /// "wardley.stage-label".
    pub const STAGE_LABEL: &str = "wardley.stage-label";

    /// "wardley.title".
    pub const TITLE: &str = "wardley.title";

    /// "wardley.trend".
    pub const TREND: &str = "wardley.trend";

    /// "wardley.trend-target".
    pub const TREND_TARGET: &str = "wardley.trend-target";

    /// Every role of the diagram.
    pub const ALL: &[&str] = &[
        ACCELERATOR,
        ANCHOR,
        ANNOTATION,
        ANNOTATION_TEXT,
        ANNOTATIONS,
        AXIS,
        AXIS_LABEL,
        COMPONENT,
        DEACCELERATOR,
        GRID,
        INERTIA,
        LABEL,
        LINK,
        LINK_LABEL,
        NOTE,
        PIPELINE,
        PIPELINE_COMPONENT,
        PIPELINE_LINE,
        PIPELINE_PARENT,
        STAGE,
        STAGE_LABEL,
        TITLE,
        TREND,
        TREND_TARGET,
    ];
}

#[cfg(feature = "xychart")]
pub mod xychart {
    //! The roles of the items of an XY chart.

    /// "xychart.axis".
    pub const AXIS: &str = "xychart.axis";

    /// "xychart.axis-label".
    pub const AXIS_LABEL: &str = "xychart.axis-label";

    /// "xychart.axis-title".
    pub const AXIS_TITLE: &str = "xychart.axis-title";

    /// "xychart.bars".
    pub const BARS: &str = "xychart.bars";

    /// "xychart.data-label".
    pub const DATA_LABEL: &str = "xychart.data-label";

    /// "xychart.grid".
    pub const GRID: &str = "xychart.grid";

    /// "xychart.legend".
    pub const LEGEND: &str = "xychart.legend";

    /// "xychart.line".
    pub const LINE: &str = "xychart.line";

    /// "xychart.point-label".
    pub const POINT_LABEL: &str = "xychart.point-label";

    /// "xychart.title".
    pub const TITLE: &str = "xychart.title";

    /// Every role of the diagram.
    pub const ALL: &[&str] = &[
        AXIS,
        AXIS_LABEL,
        AXIS_TITLE,
        BARS,
        DATA_LABEL,
        GRID,
        LEGEND,
        LINE,
        POINT_LABEL,
        TITLE,
    ];
}
