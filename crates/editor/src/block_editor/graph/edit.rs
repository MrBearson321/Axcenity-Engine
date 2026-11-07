//! What wiring nodes does to the blocks.
//!
//! The blocks are the script, so every edit in the graph is an edit of the
//! block tree. An exec wire puts a chain of blocks into a stack (after a
//! block, or into a C-shaped block's mouth); a value wire puts a reporter
//! into a slot. An exec input takes one wire and a value node feeds one pin:
//! wiring a value node that's already wired copies it, which runs the same,
//! since a value node is worked out again wherever it's used.

use super::*;

/// Why a wire can't go where it was dropped.
pub(super) type Refusal = &'static str;

/// The block with `id`, wherever it is.
pub(super) fn find_block(script: &BlockScript, id: u64) -> Option<&Block> {
    fn find(blocks: &[Block], id: u64) -> Option<&Block> {
        for block in blocks {
            if block.id == id {
                return Some(block);
            }
            for body in block.kind.bodies() {
                if let Some(found) = find(body, id) {
                    return Some(found);
                }
            }
        }
        None
    }
    find(&script.blocks, id)
}

/// The stack holding block `id`, and where in it the block is.
pub(super) fn stack_of(script: &BlockScript, id: u64) -> Option<(&[Block], usize)> {
    fn find(blocks: &[Block], id: u64) -> Option<(&[Block], usize)> {
        if let Some(index) = blocks.iter().position(|block| block.id == id) {
            return Some((blocks, index));
        }
        blocks
            .iter()
            .flat_map(|block| block.kind.bodies())
            .find_map(|body| find(body, id))
    }
    find(&script.blocks, id)
}

/// Whether `id` is one of the script's top-level blocks (an event, a loose
/// stack, or Lua at the top).
pub(super) fn is_top_level(script: &BlockScript, id: u64) -> bool {
    script.blocks.iter().any(|block| block.id == id)
}

/// The value at `at`.
pub(super) fn value<'a>(script: &'a BlockScript, at: &ValueAt) -> Option<&'a Expression> {
    let (mut value, rest) = match find_block(script, at.owner) {
        Some(block) => {
            let (first, rest) = at.path.split_first()?;
            (block.kind.expressions().into_iter().nth(*first)?, rest)
        }
        None => {
            let loose = script
                .graph
                .loose
                .iter()
                .find(|loose| loose.id == at.owner)?;
            (&loose.value, &at.path[..])
        }
    };
    for &index in rest {
        value = value.children().into_iter().nth(index)?;
    }
    Some(value)
}

pub(super) fn value_mut<'a>(
    script: &'a mut BlockScript,
    at: &ValueAt,
) -> Option<&'a mut Expression> {
    let is_block = find_block(script, at.owner).is_some();
    let (mut value, rest) = if is_block {
        let (first, rest) = at.path.split_first()?;
        let block = script.find_mut(at.owner)?;
        (block.kind.expressions_mut().into_iter().nth(*first)?, rest)
    } else {
        let loose = script
            .graph
            .loose
            .iter_mut()
            .find(|loose| loose.id == at.owner)?;
        (&mut loose.value, &at.path[..])
    };
    for &index in rest {
        value = value.children_mut().into_iter().nth(index)?;
    }
    Some(value)
}

/// What each value slot of a block expects, in slot order.
pub(super) fn block_slots(kind: &BlockKind) -> Vec<SlotKind> {
    let mut kind = kind.clone();
    sections(&mut kind, &[])
        .into_iter()
        .flat_map(|section| match section {
            Section::Row(parts) => parts,
            Section::Mouth(_) => Vec::new(),
        })
        .filter_map(|part| match part {
            Part::Slot(_, slot) => Some(slot),
            _ => None,
        })
        .collect()
}

/// What each value slot of a reporter expects, in slot order.
pub(super) fn value_slots(expression: &Expression) -> Vec<SlotKind> {
    let mut expression = expression.clone();
    reporter_parts(&mut expression, &[])
        .into_iter()
        .filter_map(|part| match part {
            Part::Slot(_, slot) => Some(slot),
            _ => None,
        })
        .collect()
}

/// What the slot `at` is in expects. `None` for a loose value node itself.
pub(super) fn slot_kind(script: &BlockScript, at: &ValueAt) -> Option<SlotKind> {
    let (last, parent) = at.path.split_last()?;
    let slots = if parent.is_empty() {
        match find_block(script, at.owner) {
            Some(block) => block_slots(&block.kind),
            None => value_slots(value(script, &at.parent()?)?),
        }
    } else {
        value_slots(value(script, &at.parent()?)?)
    };
    slots.get(*last).copied()
}

/// The first block wired to `out` of block `from`.
pub(super) fn exec_target(script: &BlockScript, from: u64, out: Out) -> Option<u64> {
    match out {
        Out::Body(index) => {
            let block = find_block(script, from)?;
            block
                .kind
                .bodies()
                .get(index)?
                .first()
                .map(|block| block.id)
        }
        Out::Then => {
            let (stack, index) = stack_of(script, from)?;
            stack.get(index + 1).map(|block| block.id)
        }
    }
}

/// Whether block `id` has a wire coming into its exec input: it isn't the
/// first block of a loose stack.
pub(super) fn has_exec_input(script: &BlockScript, id: u64) -> bool {
    !script.blocks.iter().any(|block| {
        matches!(&block.kind, BlockKind::Unattached { body } if body.first().is_some_and(|first| first.id == id))
    })
}

/// The ids of `start`, the blocks after it in its stack, and everything
/// inside them: what moves when a wire into `start` is moved.
fn chain_ids(script: &BlockScript, start: u64) -> HashSet<u64> {
    let mut ids = HashSet::new();
    if let Some((stack, index)) = stack_of(script, start) {
        let chain = BlockScript {
            blocks: stack[index..].to_vec(),
            ..Default::default()
        };
        chain.for_each_block(&mut |block| {
            ids.insert(block.id);
        });
    }
    ids
}

/// Takes `start` and the blocks after it out of their stack.
fn take_chain(script: &mut BlockScript, start: u64) -> Vec<Block> {
    match script.stack_of_mut(start) {
        Some((stack, index)) => stack.split_off(index),
        None => Vec::new(),
    }
}

/// Makes `blocks` a loose stack, if there are any.
fn add_loose_stack(script: &mut BlockScript, blocks: Vec<Block>) {
    if !blocks.is_empty() {
        script
            .blocks
            .push(Block::new(BlockKind::Unattached { body: blocks }));
    }
}

/// Loose stacks emptied by an edit are gone, and an if/else whose else is
/// empty is an if again, so the graph's one Branch node stays one block.
pub(super) fn tidy(script: &mut BlockScript) {
    script
        .blocks
        .retain(|block| !matches!(&block.kind, BlockKind::Unattached { body } if body.is_empty()));
    fn walk(blocks: &mut [Block]) {
        for block in blocks {
            if let BlockKind::IfElse {
                condition,
                then_body,
                else_body,
            } = &mut block.kind
                && else_body.is_empty()
            {
                block.kind = BlockKind::If {
                    condition: std::mem::replace(condition, Expression::Boolean { value: false }),
                    body: std::mem::take(then_body),
                };
            }
            for body in block.kind.bodies_mut() {
                walk(body);
            }
        }
    }
    walk(&mut script.blocks);
}

/// Wires `out` of block `from` to block `to`'s exec input. `to` and the
/// blocks after it move there; what was wired to `out` before is left
/// loose, as is what was before `to`.
pub(super) fn connect_exec(
    script: &mut BlockScript,
    from: u64,
    out: Out,
    to: u64,
) -> Result<(), Refusal> {
    if from == to || chain_ids(script, to).contains(&from) {
        return Err("That would loop back. Use a loop node (Repeat, Forever) instead.");
    }
    if is_top_level(script, to) {
        return Err("Events start a script: nothing can be wired into them.");
    }
    let Some(block) = find_block(script, from) else {
        return Err("That node is gone.");
    };
    let pins = exec_outs(&block.kind);
    if !pins.iter().any(|(pin, _)| *pin == out) {
        return Err("That pin doesn't take a wire.");
    }
    if exec_target(script, from, out) == Some(to) {
        return Ok(());
    }
    // The False pin of a Branch with no else gives it one.
    if out == Out::Body(1)
        && let Some(block) = script.find_mut(from)
        && let BlockKind::If { condition, body } = &mut block.kind
    {
        block.kind = BlockKind::IfElse {
            condition: std::mem::replace(condition, Expression::Boolean { value: false }),
            then_body: std::mem::take(body),
            else_body: Vec::new(),
        };
    }
    let chain = take_chain(script, to);
    let displaced = match out {
        Out::Then => match script.stack_of_mut(from) {
            Some((stack, index)) => {
                let after = stack.split_off(index + 1);
                stack.extend(chain);
                after
            }
            None => chain,
        },
        Out::Body(index) => match script
            .find_mut(from)
            .and_then(|block| block.kind.bodies_mut().into_iter().nth(index))
        {
            Some(body) => std::mem::replace(body, chain),
            None => chain,
        },
    };
    add_loose_stack(script, displaced);
    tidy(script);
    Ok(())
}

/// Takes the wire off `out` of block `from`: what was wired there is left
/// loose.
pub(super) fn break_exec_out(script: &mut BlockScript, from: u64, out: Out) {
    if let Some(to) = exec_target(script, from, out) {
        let chain = take_chain(script, to);
        add_loose_stack(script, chain);
        tidy(script);
    }
}

/// Takes the wire off block `to`'s exec input, leaving it and the blocks
/// after it loose.
pub(super) fn break_exec_in(script: &mut BlockScript, to: u64) {
    if has_exec_input(script, to) && !is_top_level(script, to) {
        let chain = take_chain(script, to);
        add_loose_stack(script, chain);
        tidy(script);
    }
}

/// Moves stored positions under `from` (a node and the value nodes wired
/// into it) to `to`.
pub(super) fn rekey(graph: &mut Graph, from: &ValueAt, to: &ValueAt) {
    let (old, new) = (from.key(), to.key());
    let moved: Vec<(String, [f32; 2])> = graph
        .nodes
        .iter()
        .filter(|(key, _)| **key == old || key.starts_with(&format!("{old}.")))
        .map(|(key, at)| (key.clone(), *at))
        .collect();
    for (key, _) in &moved {
        graph.nodes.remove(key);
    }
    for (key, at) in moved {
        graph
            .nodes
            .insert(format!("{new}{}", &key[old.len()..]), at);
    }
}

/// Leaves the reporter in slot `at` loose where it is, and puts the slot
/// back to an empty value.
pub(super) fn break_value(script: &mut BlockScript, at: &ValueAt) {
    let Some(kind) = slot_kind(script, at) else {
        return;
    };
    let Some(slot) = value_mut(script, at) else {
        return;
    };
    if slot.is_literal() {
        return;
    }
    let taken = std::mem::replace(slot, kind.empty());
    let loose = LooseValue::new(taken);
    rekey(&mut script.graph, at, &ValueAt::loose(loose.id));
    script.graph.loose.push(loose);
}

/// Wires value node `from`'s output to slot `to`. A loose node moves there;
/// one already wired is copied. A reporter already in the slot is left loose.
pub(super) fn connect_value(
    script: &mut BlockScript,
    from: &ValueAt,
    to: &ValueAt,
) -> Result<(), Refusal> {
    if to.within(from) {
        return Err("That would loop back into itself.");
    }
    let Some(kind) = slot_kind(script, to) else {
        return Err("That isn't an input.");
    };
    let Some(moved) = value(script, from).cloned() else {
        return Err("That node is gone.");
    };
    if !kind.accepts(moved.shape()) {
        return Err(if kind == SlotKind::Condition {
            "This input needs true or false: a red pointed value, such as a comparison."
        } else {
            "That value doesn't fit this input."
        });
    }
    let loose = from.path.is_empty();
    if loose {
        script.graph.loose.retain(|value| value.id != from.owner);
    }
    break_value(script, to);
    if let Some(slot) = value_mut(script, to) {
        *slot = moved;
    }
    if loose {
        rekey(&mut script.graph, from, to);
    }
    Ok(())
}

/// Deletes value node `at`. What was wired into it is left loose.
fn delete_value(script: &mut BlockScript, at: &ValueAt) {
    let children = value(script, at).map_or(0, |value| value.children().len());
    for index in 0..children {
        break_value(script, &at.child(index));
    }
    if at.path.is_empty() {
        script.graph.loose.retain(|loose| loose.id != at.owner);
    } else if let Some(kind) = slot_kind(script, at)
        && let Some(slot) = value_mut(script, at)
    {
        *slot = kind.empty();
    }
    script.graph.nodes.remove(&at.key());
}

/// Takes block `id` out of its stack. The value nodes in its inputs and
/// the blocks in its mouths are left loose.
fn remove_block(script: &mut BlockScript, id: u64) {
    let slots = find_block(script, id).map_or(0, |block| block.kind.expressions().len());
    for slot in 0..slots {
        break_value(
            script,
            &ValueAt {
                owner: id,
                path: vec![slot],
            },
        );
    }
    let Some((stack, index)) = script.stack_of_mut(id) else {
        return;
    };
    let mut block = stack.remove(index);
    for body in block.kind.bodies_mut() {
        add_loose_stack(script, std::mem::take(body));
    }
    script.graph.nodes.remove(&id.to_string());
    tidy(script);
}

/// Deletes these nodes. Wires to them go; what they were wired to stays.
pub(super) fn delete_nodes(script: &mut BlockScript, nodes: &[NodeRef]) {
    let mut values: Vec<&ValueAt> = nodes
        .iter()
        .filter_map(|node| match node {
            NodeRef::Value(at) => Some(at),
            NodeRef::Block(_) => None,
        })
        .collect();
    // Deepest first, so a node's address is still right when it goes.
    values.sort_by_key(|at| std::cmp::Reverse(at.path.len()));
    for at in values {
        delete_value(script, at);
    }
    for node in nodes {
        if let NodeRef::Block(id) = node {
            delete_block_node(script, *id);
        }
    }
}

/// Deletes block `id`'s node, whether it's an event at the top or a block
/// in a stack.
fn delete_block_node(script: &mut BlockScript, id: u64) {
    if let Some(index) = script.blocks.iter().position(|block| block.id == id) {
        let mut block = script.blocks.remove(index);
        let body = block
            .kind
            .bodies_mut()
            .into_iter()
            .next()
            .map(std::mem::take)
            .unwrap_or_default();
        add_loose_stack(script, body);
        script.graph.nodes.remove(&id.to_string());
        tidy(script);
        return;
    }
    // The blocks after it are left loose, unless it was the first of a
    // loose stack: then they stay that stack.
    let first_of_loose = !has_exec_input(script, id);
    if !first_of_loose && let Some((stack, index)) = script.stack_of_mut(id) {
        let after = stack.split_off(index + 1);
        add_loose_stack(script, after);
    }
    remove_block(script, id);
}

/// A copy of `node` with nothing wired to its exec pins, as a new node.
/// Returns the copy.
pub(super) fn duplicate_node(script: &mut BlockScript, node: &NodeRef) -> Option<NodeRef> {
    match node {
        NodeRef::Block(id) => {
            let block = find_block(script, *id)?;
            let mut kind = block.kind.clone();
            for body in kind.bodies_mut() {
                body.clear();
            }
            let copy = Block::new(kind).duplicate();
            let copy_id = copy.id;
            if copy.kind.is_event() {
                script.blocks.push(copy);
            } else {
                add_loose_stack(script, vec![copy]);
            }
            Some(NodeRef::Block(copy_id))
        }
        NodeRef::Value(at) => {
            let copy = LooseValue::new(value(script, at)?.clone());
            let id = copy.id;
            script.graph.loose.push(copy);
            Some(NodeRef::Value(ValueAt::loose(id)))
        }
    }
}

/// A new node for a palette item, wired to nothing.
pub(super) fn add_node(script: &mut BlockScript, item: &PaletteItem) -> Option<NodeRef> {
    match item {
        PaletteItem::Block(kind) => {
            let block = Block::new(BlockKind::clone(kind));
            let id = block.id;
            if kind.is_event() {
                script.blocks.push(block);
            } else {
                add_loose_stack(script, vec![block]);
            }
            Some(NodeRef::Block(id))
        }
        PaletteItem::Reporter(expression) => {
            let loose = LooseValue::new(expression.clone());
            let id = loose.id;
            script.graph.loose.push(loose);
            Some(NodeRef::Value(ValueAt::loose(id)))
        }
        PaletteItem::NewVariable => None,
    }
}
