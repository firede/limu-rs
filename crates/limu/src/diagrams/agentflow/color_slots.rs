//! The palette slots of an agent flow, as Pele's colorSlots.ts gives them. There are two
//! rules, as in Mermaid. A node takes the color of its kind, from a fixed slot, so editing the
//! diagram around it never recolors it. Containers count through the slots above the kinds,
//! in the order they were declared, so a frame does not take the color of a node inside it.

use std::collections::HashMap;

use super::graph::GraphNode;

/// The slot of each kind of node, Pele's KIND_SLOT.
const KIND_SLOT: [(&str, usize); 7] = [
    ("tool", 0),
    ("task", 1),
    ("decision", 2),
    ("input", 3),
    ("refdoc", 4),
    ("connector", 5),
    ("action", 6),
];

/// The number of kinds, Pele's KIND_COUNT.
pub(crate) const KIND_COUNT: usize = KIND_SLOT.len();

/// The slot of a kind of node, where it is one.
pub(crate) fn kind_slot(kind: &str) -> Option<usize> {
    KIND_SLOT.iter().find(|(k, _)| *k == kind).map(|&(_, s)| s)
}

/// The number of slots containers count through, at least one, so that a palette shorter
/// than the kinds still gives containers a slot, Pele's containerSlotCount.
fn container_slot_count(palette_length: usize) -> usize {
    palette_length.saturating_sub(KIND_COUNT).max(1)
}

/// The slot of the nth container, Pele's containerSlot.
fn container_slot(n: usize, palette_length: usize) -> usize {
    let slot = KIND_COUNT + n % container_slot_count(palette_length);
    if palette_length > 0 {
        slot % palette_length
    } else {
        slot
    }
}

/// Gives each container its slot, by its place in container_order, and each node its kind,
/// by kind_of, as Pele's assignColorSlots does.
pub(crate) fn assign_color_slots(
    nodes: &mut [GraphNode],
    kind_of: impl Fn(&str) -> Option<&'static str>,
    container_order: &HashMap<String, usize>,
    palette_length: usize,
) {
    let mut fallback = container_order.len();
    for node in nodes {
        // A collapsed container is drawn as a node but keeps its slot, so collapsing one does
        // not recolor the containers after it.
        if node.is_group || node.shape == "collapsedGroup" {
            let n = match container_order.get(&node.id) {
                Some(&n) => n,
                None => {
                    fallback += 1;
                    fallback - 1
                }
            };
            node.color_index = Some(container_slot(n, palette_length));
            continue;
        }
        if let Some(kind) = kind_of(&node.id).filter(|k| kind_slot(k).is_some()) {
            node.kind = Some(kind);
        }
    }
}
