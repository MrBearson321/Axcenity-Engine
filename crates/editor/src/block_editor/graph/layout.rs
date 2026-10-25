//! Where nodes go when they haven't been placed.
//!
//! Exec flows left to right, as in Unreal: a stack's blocks are a row, each
//! after the one before. The value nodes wired into a node sit to its left,
//! below the header, and what runs in a loop or a branch is a row below.
//! A node that was placed (dragged, or made from the menu) stays where it
//! is, and the nodes after it follow it.

use super::*;

const ROW_GAP: f32 = 56.0;
const INPUT_GAP: f32 = 36.0;
const STACK_GAP: f32 = 48.0;
const VALUE_GAP: f32 = 12.0;
/// How far right of its node a mouth's row starts.
const MOUTH_INDENT: f32 = 40.0;
const MARGIN: f32 = 40.0;
/// How far below a node's header its value nodes start: under its exec
/// pins' row.
const EXEC_ROW: f32 = 36.0;

/// Places every node. `sizes` are in graph units.
pub(super) fn layout(
    script: &BlockScript,
    sizes: &HashMap<NodeRef, Vec2>,
) -> HashMap<NodeRef, Pos2> {
    let mut placer = Placer {
        sizes,
        stored: &script.graph.nodes,
        placed: HashMap::new(),
    };
    let mut y = MARGIN;
    for block in &script.blocks {
        let start = pos2(MARGIN, y);
        let bottom = match &block.kind {
            BlockKind::Unattached { body } => placer.row(body.iter(), start),
            kind if kind.is_event() => {
                let body = kind.bodies().first().map_or(&[][..], |body| &body[..]);
                placer.row(std::iter::once(block).chain(body), start)
            }
            _ => placer.row(std::iter::once(block), start),
        };
        y = bottom + STACK_GAP;
    }
    for loose in &script.graph.loose {
        let at = ValueAt::loose(loose.id);
        let width = placer.width_of(&at, &loose.value);
        let bottom = placer.value(&at, &loose.value, MARGIN + width, y);
        y = y.max(bottom) + STACK_GAP;
    }
    placer.placed
}

struct Placer<'a> {
    sizes: &'a HashMap<NodeRef, Vec2>,
    stored: &'a BTreeMap<String, [f32; 2]>,
    placed: HashMap<NodeRef, Pos2>,
}

impl Placer<'_> {
    fn size(&self, node: &NodeRef) -> Vec2 {
        self.sizes.get(node).copied().unwrap_or(vec2(140.0, 60.0))
    }

    fn stored(&self, node: &NodeRef) -> Option<Pos2> {
        self.stored.get(&node.key()).map(|[x, y]| pos2(*x, *y))
    }

    /// Places a value node with its right edge at `right`, and its inputs
    /// to its left. Returns the bottom of what was placed.
    fn value(&mut self, at: &ValueAt, value: &Expression, right: f32, top: f32) -> f32 {
        let node = NodeRef::Value(at.clone());
        let size = self.size(&node);
        let place = self.stored(&node).unwrap_or(pos2(right - size.x, top));
        self.placed.insert(node, place);
        let mut bottom = place.y + size.y;
        let mut y = place.y;
        for (index, child) in value.children().into_iter().enumerate() {
            if child.is_literal() {
                continue;
            }
            let child_bottom = self.value(&at.child(index), child, place.x - INPUT_GAP, y);
            y = child_bottom + VALUE_GAP;
            bottom = bottom.max(child_bottom);
        }
        bottom
    }

    /// Places a row of blocks from `start`, then the rows of their mouths
    /// below it. Returns the bottom of what was placed.
    fn row<'b>(&mut self, blocks: impl Iterator<Item = &'b Block>, start: Pos2) -> f32 {
        let mut x = start.x;
        let mut y = start.y;
        let mut bottom = start.y;
        let mut mouths = Vec::new();
        for block in blocks {
            let node = NodeRef::Block(block.id);
            let size = self.size(&node);
            let slots: Vec<(usize, &Expression)> = block
                .kind
                .expressions()
                .into_iter()
                .enumerate()
                .filter(|(_, value)| !value.is_literal())
                .collect();
            let inputs = slots
                .iter()
                .map(|(slot, value)| {
                    self.width_of(
                        &ValueAt {
                            owner: block.id,
                            path: vec![*slot],
                        },
                        value,
                    ) + INPUT_GAP
                })
                .fold(0.0, f32::max);
            let place = self.stored(&node).unwrap_or(pos2(x + inputs, y));
            self.placed.insert(node, place);
            bottom = bottom.max(place.y + size.y);
            // Below the exec row, so the wire into the node doesn't cross them.
            let mut input_y = place.y + HEADER + EXEC_ROW;
            for (slot, value) in slots {
                let at = ValueAt {
                    owner: block.id,
                    path: vec![slot],
                };
                let input_bottom = self.value(&at, value, place.x - INPUT_GAP, input_y);
                input_y = input_bottom + VALUE_GAP;
                bottom = bottom.max(input_bottom);
            }
            x = place.x + size.x + ROW_GAP;
            y = place.y;
            if !block.kind.is_event() {
                for body in block.kind.bodies() {
                    if !body.is_empty() {
                        mouths.push((place.x + MOUTH_INDENT, body));
                    }
                }
            }
        }
        for (left, body) in mouths {
            bottom = self.row(body.iter(), pos2(left, bottom + STACK_GAP));
        }
        bottom
    }

    /// How wide a value node and the nodes wired into it are.
    fn width_of(&self, at: &ValueAt, value: &Expression) -> f32 {
        let own = self.size(&NodeRef::Value(at.clone())).x;
        let inputs = value
            .children()
            .into_iter()
            .enumerate()
            .filter(|(_, child)| !child.is_literal())
            .map(|(index, child)| self.width_of(&at.child(index), child) + INPUT_GAP)
            .fold(0.0, f32::max);
        own + inputs
    }
}
