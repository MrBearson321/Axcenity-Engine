//! The graph view of a block script: Unreal Blueprint-style nodes and wires.
//!
//! It shows the same blocks as the block canvas, so a script switches
//! between Code, Blocks and Graph without losing anything. Each block is a
//! node. White exec wires say what runs next: an event's wire leads to the
//! first block of its stack, a block's Then to the block after it, and a
//! loop's Loop Body or a Branch's True and False to the blocks in its
//! mouths. Reporters are value nodes, wired into the inputs they feed;
//! values typed in stay on the pin. Nodes not wired to an event don't run,
//! like loose blocks.
//!
//! Right-click the canvas (or drop a wire on it) for the searchable node
//! menu. Drag from a pin to wire it, Alt-click a pin to break its wires.
//! Left-drag the background to select a box of nodes, right-drag (or the
//! wheel) to pan, and Ctrl + wheel to zoom. Delete deletes the selected
//! nodes and Ctrl+D duplicates them.
//!
//! Where to change behavior:
//! - `edit.rs` — what wiring, breaking and deleting does to the blocks
//! - `nodes.rs` — node titles, pin names and colours
//! - `layout.rs` — where nodes go before they're dragged
//! - `menu.rs` — the node menu
//! - this file — measuring, drawing and the pointer

use std::collections::{BTreeMap, HashMap, HashSet};

use axcenity_blocks::{Graph, LooseValue};

use super::*;

mod edit;
mod layout;
mod menu;
mod nodes;

use edit::*;
use layout::*;
use menu::*;
use nodes::*;

// Sizes at zoom 1, in graph units (points).
pub(super) const HEADER: f32 = 28.0;
const ROW: f32 = 26.0;
const PIN: f32 = 5.0;
/// From a node's edge to the text beside a pin.
const PIN_SPACE: f32 = 22.0;
const MIDDLE_GAP: f32 = 24.0;
const MIN_NODE_WIDTH: f32 = 130.0;
const NAME_GAP: f32 = 6.0;
/// How near a pin a wire has to be let go to join it.
const PIN_REACH: f32 = 14.0;

/// Which mouth or follow-on an exec output is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) enum Out {
    /// What runs in mouth `n` (an event's stack is its mouth 0).
    Body(usize),
    /// The block after this one.
    Then,
}

/// Where a value is: its owner (a block, or a loose value node's id) and
/// the slots down to it. A loose value node itself has an empty path.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(super) struct ValueAt {
    pub(super) owner: u64,
    pub(super) path: Vec<usize>,
}

impl ValueAt {
    pub(super) fn loose(id: u64) -> Self {
        Self {
            owner: id,
            path: Vec::new(),
        }
    }

    pub(super) fn child(&self, index: usize) -> Self {
        let mut path = self.path.clone();
        path.push(index);
        Self {
            owner: self.owner,
            path,
        }
    }

    pub(super) fn parent(&self) -> Option<Self> {
        let mut path = self.path.clone();
        path.pop()?;
        Some(Self {
            owner: self.owner,
            path,
        })
    }

    /// How [`Graph::nodes`] names it: `"12.0.1"`.
    pub(super) fn key(&self) -> String {
        let mut key = self.owner.to_string();
        for index in &self.path {
            key.push_str(&format!(".{index}"));
        }
        key
    }

    /// Whether this is `other` or a slot inside it.
    pub(super) fn within(&self, other: &ValueAt) -> bool {
        self.owner == other.owner && self.path.starts_with(&other.path)
    }
}

/// A node: a block, or a value node (a reporter).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(super) enum NodeRef {
    Block(u64),
    Value(ValueAt),
}

impl NodeRef {
    pub(super) fn key(&self) -> String {
        match self {
            NodeRef::Block(id) => id.to_string(),
            NodeRef::Value(at) => at.key(),
        }
    }

    /// What this node's input slots are numbered under.
    fn slots(&self) -> ValueAt {
        match self {
            NodeRef::Block(id) => ValueAt::loose(*id),
            NodeRef::Value(at) => at.clone(),
        }
    }
}

/// A pin on a node.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(super) enum Pin {
    ExecIn(u64),
    ExecOut(u64, Out),
    /// A value slot.
    Input(ValueAt),
    /// A value node's output.
    Output(ValueAt),
}

impl Pin {
    fn exec(&self) -> bool {
        matches!(self, Pin::ExecIn(_) | Pin::ExecOut(..))
    }
}

/// What the graph view remembers between frames for one script.
#[derive(Clone, Debug)]
struct GraphState {
    pan: Vec2,
    zoom: f32,
    show_lua: bool,
    selected: HashSet<NodeRef>,
    /// A wire being dragged out of this pin.
    wire: Option<Pin>,
    /// Where a box selection started, on screen.
    boxing: Option<Pos2>,
    menu: Option<Menu>,
    /// Why the last wire didn't join, and until when to say so.
    notice: Option<(String, f64)>,
}

impl Default for GraphState {
    fn default() -> Self {
        Self {
            pan: vec2(20.0, 20.0),
            zoom: 1.0,
            show_lua: false,
            selected: HashSet::new(),
            wire: None,
            boxing: None,
            menu: None,
            notice: None,
        }
    }
}

/// A node's measurements at zoom 1.
#[derive(Clone, Debug)]
struct Geometry {
    size: Vec2,
    title: String,
    category: Category,
    layout: Layout,
    /// Each row's height.
    rows: Vec<f32>,
    /// Where each pin is, from the node's top-left corner.
    pins: Vec<(Pin, Vec2)>,
}

/// A node's script piece, borrowed for measuring or drawing.
enum Body<'a> {
    Block(&'a mut BlockKind),
    Value(&'a mut Expression),
}

/// Calls `visit` for every node, with whether it runs (is wired to an
/// event): events and the blocks after them, top-level Lua, and the value
/// nodes wired into those.
fn visit_nodes(script: &mut BlockScript, visit: &mut dyn FnMut(NodeRef, Body, bool)) {
    fn values(
        at: ValueAt,
        value: &mut Expression,
        runs: bool,
        visit: &mut dyn FnMut(NodeRef, Body, bool),
    ) {
        visit(NodeRef::Value(at.clone()), Body::Value(value), runs);
        for (index, child) in value.children_mut().into_iter().enumerate() {
            if !child.is_literal() {
                values(at.child(index), child, runs, visit);
            }
        }
    }
    fn visit_block(block: &mut Block, runs: bool, visit: &mut dyn FnMut(NodeRef, Body, bool)) {
        let id = block.id;
        visit(NodeRef::Block(id), Body::Block(&mut block.kind), runs);
        for (slot, value) in block.kind.expressions_mut().into_iter().enumerate() {
            if !value.is_literal() {
                values(
                    ValueAt {
                        owner: id,
                        path: vec![slot],
                    },
                    value,
                    runs,
                    visit,
                );
            }
        }
        for body in block.kind.bodies_mut() {
            for inner in body {
                visit_block(inner, runs, visit);
            }
        }
    }
    for top in &mut script.blocks {
        match &mut top.kind {
            BlockKind::Unattached { body } => {
                for inner in body {
                    visit_block(inner, false, visit);
                }
            }
            _ => visit_block(top, true, visit),
        }
    }
    for loose in &mut script.graph.loose {
        values(ValueAt::loose(loose.id), &mut loose.value, false, visit);
    }
}

/// Which exec pins have wires: each output's target, and the blocks with a
/// wire coming in.
#[derive(Default)]
struct Wiring {
    targets: HashMap<(u64, Out), u64>,
    fed: HashSet<u64>,
}

fn wiring(script: &BlockScript) -> Wiring {
    fn stack(blocks: &[Block], wiring: &mut Wiring, loose: bool) {
        for (index, block) in blocks.iter().enumerate() {
            if index > 0 || !loose {
                wiring.fed.insert(block.id);
            }
            if let Some(next) = blocks.get(index + 1) {
                wiring.targets.insert((block.id, Out::Then), next.id);
            }
            mouths(block, wiring);
        }
    }
    fn mouths(block: &Block, wiring: &mut Wiring) {
        for (mouth, body) in block.kind.bodies().into_iter().enumerate() {
            if let Some(first) = body.first() {
                wiring
                    .targets
                    .insert((block.id, Out::Body(mouth)), first.id);
            }
            stack(body, wiring, false);
        }
    }
    let mut wiring = Wiring::default();
    for top in &script.blocks {
        match &top.kind {
            BlockKind::Unattached { body } => stack(body, &mut wiring, true),
            _ => mouths(top, &mut wiring),
        }
    }
    wiring
}

/// The title, colour and rows of a node.
fn describe(body: &Body, objects: &[ObjectInfo]) -> (String, Category, Layout) {
    match body {
        Body::Block(kind) => {
            let mut probe = (*kind).clone();
            let layout = block_layout(kind, &block_parts(&mut probe, objects));
            (block_title(kind), block_category(kind), layout)
        }
        Body::Value(expression) => {
            let mut probe = (*expression).clone();
            let layout = value_layout(expression, &reporter_parts(&mut probe, objects));
            (
                value_names(expression).0,
                reporter_category(expression),
                layout,
            )
        }
    }
}

/// The parts of a node's piece, to measure and draw its menus and inputs.
fn parts_of<'a>(body: &'a mut Body, objects: &[ObjectInfo]) -> Vec<Part<'a>> {
    match body {
        Body::Block(kind) => block_parts(kind, objects),
        Body::Value(expression) => reporter_parts(expression, objects),
    }
}

/// How big an item is, at the painter's zoom.
fn item_size(painter: &BlockPainter, ui: &egui::Ui, item: &Item, parts: &mut [Part]) -> Vec2 {
    let s = |value: f32| painter.s(value);
    match item {
        Item::ExecIn | Item::Output(_) => vec2(s(PIN_SPACE), s(ROW)),
        Item::ExecOut(_, name) => vec2(painter.text_width(ui, name) + s(PIN_SPACE + 4.0), s(ROW)),
        Item::Input { part, name, .. } => {
            let mut width = s(PIN_SPACE) + painter.text_width(ui, name);
            if let Some(Part::Slot(value, kind)) = parts.get(*part)
                && value.is_literal()
            {
                width += s(NAME_GAP) + painter.literal_size(ui, value, *kind).x;
            }
            vec2(width + s(4.0), s(ROW))
        }
        Item::Field { part, name } => {
            let field = parts
                .get_mut(*part)
                .map_or(Vec2::ZERO, |part| painter.part_size(ui, part));
            vec2(
                s(10.0) + painter.text_width(ui, name) + s(NAME_GAP) + field.x + s(4.0),
                s(ROW).max(field.y + s(4.0)),
            )
        }
        Item::Code { part } => {
            let code = parts
                .get_mut(*part)
                .map_or(Vec2::ZERO, |part| painter.part_size(ui, part));
            code + vec2(s(20.0), s(8.0))
        }
    }
}

/// Measures a node at zoom 1.
fn measure(
    painter: &BlockPainter,
    ui: &egui::Ui,
    node: &NodeRef,
    title: String,
    category: Category,
    layout: Layout,
    parts: &mut [Part],
) -> Geometry {
    let mut width = painter.text_width(ui, &title) + 2.0 * 12.0;
    let mut rows = Vec::new();
    for row in 0..layout.rows() {
        let left = layout
            .left
            .get(row)
            .map_or(Vec2::ZERO, |item| item_size(painter, ui, item, parts));
        let right = layout
            .right
            .get(row)
            .map_or(Vec2::ZERO, |item| item_size(painter, ui, item, parts));
        width = width.max(left.x + MIDDLE_GAP + right.x);
        rows.push(left.y.max(right.y).max(ROW));
    }
    let width = width.max(MIN_NODE_WIDTH);
    let mut pins = Vec::new();
    let mut y = HEADER + 2.0;
    let slots = node.slots();
    for (row, height) in rows.iter().enumerate() {
        let middle = y + height * 0.5;
        match layout.left.get(row) {
            Some(Item::ExecIn) => {
                if let NodeRef::Block(id) = node {
                    pins.push((Pin::ExecIn(*id), vec2(PIN + 6.0, middle)));
                }
            }
            Some(Item::Input { slot, .. }) => {
                pins.push((Pin::Input(slots.child(*slot)), vec2(PIN + 6.0, middle)));
            }
            _ => {}
        }
        match layout.right.get(row) {
            Some(Item::ExecOut(out, _)) => {
                if let NodeRef::Block(id) = node {
                    pins.push((Pin::ExecOut(*id, *out), vec2(width - PIN - 6.0, middle)));
                }
            }
            Some(Item::Output(_)) => {
                if let NodeRef::Value(at) = node {
                    pins.push((Pin::Output(at.clone()), vec2(width - PIN - 6.0, middle)));
                }
            }
            _ => {}
        }
        y += height;
    }
    Geometry {
        size: vec2(width, y + 6.0),
        title,
        category,
        layout,
        rows,
        pins,
    }
}

/// Every node's measurements at zoom 1.
fn measure_all(
    ui: &egui::Ui,
    script: &mut BlockScript,
    context: &BlockContext,
    variables: &[String],
) -> HashMap<NodeRef, Geometry> {
    let painter = BlockPainter::new(
        ui.painter().clone(),
        1.0,
        false,
        theme::canvas(ui.ctx()).node_body,
        egui::Id::new("graph measure"),
        context,
        variables,
    );
    let mut geometry = HashMap::new();
    visit_nodes(script, &mut |node, mut body, _| {
        let (title, category, layout) = describe(&body, context.objects);
        let mut parts = parts_of(&mut body, context.objects);
        let measured = measure(&painter, ui, &node, title, category, layout, &mut parts);
        geometry.insert(node, measured);
    });
    geometry
}

/// Something done with the pointer or keys this frame, applied once
/// everything is drawn.
enum Event {
    WireFrom(Pin),
    BreakPin(Pin),
    Select(NodeRef, bool),
    MoveStart(NodeRef),
    Move(Vec2),
    Duplicate(NodeRef),
    /// One more input (true) or one fewer on a call's or a local's node.
    Resize(NodeRef, bool),
    Delete(NodeRef),
}

/// Draws the graph view of `script`. Returns true if the script changed.
pub fn show_graph(ui: &mut egui::Ui, script: &mut BlockScript, context: BlockContext) -> bool {
    let before = script.clone();
    let state_id = egui::Id::new(("graph editor", context.key));
    let mut state: GraphState = ui
        .data_mut(|data| data.get_temp(state_id))
        .unwrap_or_default();
    let variables = known_variables(script, &[]);

    let full = ui.available_rect_before_wrap();
    let toolbar = Rect::from_min_size(full.min, vec2(full.width(), TOOLBAR_HEIGHT));
    let body = Rect::from_min_max(pos2(full.left(), toolbar.bottom()), full.max);
    let lua_width = if state.show_lua {
        (body.width() * 0.3).clamp(260.0, 460.0)
    } else {
        0.0
    };
    let canvas = Rect::from_min_max(body.min, pos2(body.right() - lua_width, body.bottom()));
    let lua_rect = Rect::from_min_max(pos2(canvas.right(), body.top()), body.max);

    let add_clicked = show_graph_toolbar(ui, toolbar, &mut state, &context, script);
    show_graph_canvas(
        ui,
        canvas,
        script,
        &mut state,
        &context,
        &variables,
        add_clicked,
    );
    if state.show_lua {
        show_lua(ui, lua_rect, script);
    }
    ui.advance_cursor_after_rect(full);
    ui.data_mut(|data| data.insert_temp(state_id, state));
    *script != before
}

/// The bar over the graph. Returns true when Add Node was clicked.
fn show_graph_toolbar(
    ui: &mut egui::Ui,
    rect: Rect,
    state: &mut GraphState,
    context: &BlockContext,
    script: &mut BlockScript,
) -> bool {
    ui.painter().rect_filled(rect, 0, ui.visuals().panel_fill);
    ui.painter().hline(
        rect.x_range(),
        rect.bottom() - 0.5,
        ui.visuals().widgets.noninteractive.bg_stroke,
    );
    let mut bar = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect.shrink2(vec2(10.0, 6.0)))
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    let palette = theme::palette(ui.ctx());
    let (icon, side, color) = if context.client {
        (icons::USER, "Client graph", palette.success)
    } else {
        (icons::CLOUD, "Server graph", palette.accent)
    };
    egui::Frame::new()
        .fill(color.gamma_multiply(0.18))
        .stroke(Stroke::new(1.0_f32, color))
        .corner_radius(10)
        .inner_margin(egui::Margin::symmetric(8, 2))
        .show(&mut bar, |ui| {
            ui.label(
                egui::RichText::new(theme::with_icon(icon, side))
                    .color(color)
                    .strong(),
            );
        });
    let mut add = false;
    if bar
        .button(theme::with_icon(icons::PLUS, "Add node"))
        .on_hover_text("Or right-click the graph, or drop a wire on it")
        .clicked()
    {
        add = true;
    }
    bar.weak("Drag from a pin to wire it. Right-drag to pan.");
    bar.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        let lua = egui::Button::new(theme::with_icon(icons::CODE, "View as Lua"))
            .selected(state.show_lua);
        if ui
            .add(lua)
            .on_hover_text("Shows the Lua this graph becomes. It runs exactly like a typed script.")
            .clicked()
        {
            state.show_lua = !state.show_lua;
        }
        ui.separator();
        if ui
            .button(icons::MAGNIFYING_GLASS_PLUS)
            .on_hover_text("Zoom in")
            .clicked()
        {
            state.zoom = (state.zoom * 1.2).min(2.0);
        }
        if ui
            .button(format!("{:.0}%", state.zoom * 100.0))
            .on_hover_text("Back to 100%")
            .clicked()
        {
            state.zoom = 1.0;
        }
        if ui
            .button(icons::MAGNIFYING_GLASS_MINUS)
            .on_hover_text("Zoom out")
            .clicked()
        {
            state.zoom = (state.zoom / 1.2).max(0.3);
        }
        if ui
            .button(theme::with_icon(icons::BROOM, "Clean up"))
            .on_hover_text("Lays every node out again, left to right")
            .clicked()
        {
            script.graph.nodes.clear();
            state.pan = GraphState::default().pan;
        }
    });
    add
}

/// Stores every node's place, so nothing moves when an edit rewires them.
fn freeze(graph: &mut Graph, places: &HashMap<NodeRef, Pos2>) {
    graph.nodes = places
        .iter()
        .map(|(node, at)| (node.key(), [at.x.round(), at.y.round()]))
        .collect::<BTreeMap<_, _>>();
}

/// The canvas with the nodes and wires.
fn show_graph_canvas(
    ui: &mut egui::Ui,
    rect: Rect,
    script: &mut BlockScript,
    state: &mut GraphState,
    context: &BlockContext,
    variables: &[String],
    add_clicked: bool,
) {
    let colors = theme::canvas(ui.ctx());
    let mut canvas = ui.new_child(egui::UiBuilder::new().max_rect(rect));
    canvas.set_clip_rect(rect);
    let painter = canvas.painter_at(rect);
    painter.rect_filled(rect, 0, colors.background);

    // The background: left-drag selects a box, right- or middle-drag pans,
    // a right-click opens the node menu. The wheel pans; Ctrl + wheel (or
    // pinching) zooms around the pointer.
    let back = canvas.interact(
        rect,
        egui::Id::new(("graph canvas", context.key)),
        Sense::click_and_drag(),
    );
    if back.dragged_by(egui::PointerButton::Secondary)
        || back.dragged_by(egui::PointerButton::Middle)
    {
        state.pan += back.drag_delta();
    }
    let hovered = canvas.rect_contains_pointer(rect);
    if hovered && state.menu.is_none() {
        let (scroll, zoom, pointer) =
            canvas.input(|i| (i.smooth_scroll_delta, i.zoom_delta(), i.pointer.hover_pos()));
        state.pan += scroll;
        if zoom != 1.0
            && let Some(pointer) = pointer
        {
            let old = state.zoom;
            state.zoom = (old * zoom).clamp(0.3, 2.0);
            let from_origin = pointer - rect.min - state.pan;
            state.pan = pointer - rect.min - from_origin * (state.zoom / old);
        }
    }
    let z = state.zoom;
    let origin = rect.min + state.pan;
    let to_screen = |at: Pos2| origin + at.to_vec2() * z;
    let to_graph = |at: Pos2| pos2((at.x - origin.x) / z, (at.y - origin.y) / z);

    // A grid, which moves with the nodes.
    let spacing = 32.0 * z;
    let line = colors.grid;
    let mut x = rect.left() + state.pan.x.rem_euclid(spacing);
    while x < rect.right() {
        painter.vline(x, rect.y_range(), Stroke::new(1.0_f32, line));
        x += spacing;
    }
    let mut y = rect.top() + state.pan.y.rem_euclid(spacing);
    while y < rect.bottom() {
        painter.hline(rect.x_range(), y, Stroke::new(1.0_f32, line));
        y += spacing;
    }

    let geometry = measure_all(&canvas, script, context, variables);
    let sizes: HashMap<NodeRef, Vec2> = geometry
        .iter()
        .map(|(node, geometry)| (node.clone(), geometry.size))
        .collect();
    let places = layout(script, &sizes);
    let wired = wiring(script);
    let pin_at = |pin: &Pin| -> Option<Pos2> {
        geometry.iter().find_map(|(node, geometry)| {
            let place = places.get(node)?;
            geometry
                .pins
                .iter()
                .find(|(candidate, _)| candidate == pin)
                .map(|(_, offset)| to_screen(*place + *offset))
        })
    };
    let pins: Vec<(Pin, Pos2)> = geometry
        .iter()
        .filter_map(|(node, geometry)| Some((places.get(node)?, geometry)))
        .flat_map(|(place, geometry)| {
            geometry
                .pins
                .iter()
                .map(move |(pin, offset)| (pin.clone(), to_screen(*place + *offset)))
        })
        .collect();

    if script.blocks.is_empty() && script.graph.loose.is_empty() {
        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            "Right-click to add a node, such as \"When Game Starts\".",
            FontId::proportional(14.0),
            ui.visuals().weak_text_color(),
        );
    }

    // Wires, under the nodes. Exec wires are white, or dark on a light
    // canvas.
    let exec_wire = colors.wire;
    for ((from, out), to) in &wired.targets {
        if let (Some(start), Some(end)) = (
            pin_at(&Pin::ExecOut(*from, *out)),
            pin_at(&Pin::ExecIn(*to)),
        ) {
            draw_wire(&painter, start, end, exec_wire, 2.5 * z);
        }
    }
    for (pin, end) in &pins {
        if let Pin::Input(at) = pin
            && let Some(value) = value(script, at)
            && !value.is_literal()
            && let Some(start) = pin_at(&Pin::Output(at.clone()))
        {
            draw_wire(&painter, start, *end, pin_color(value_type(value)), 2.0 * z);
        }
    }

    // The nodes.
    let mut drawer = BlockPainter::new(
        painter.clone(),
        z,
        state.menu.is_none(),
        colors.node_body,
        egui::Id::new(("graph", context.key)),
        context,
        variables,
    );
    let mut events = Vec::new();
    let selected = state.selected.clone();
    let wire_held = state.wire.is_some();
    visit_nodes(script, &mut |node, mut body, runs| {
        let (Some(geometry), Some(place)) = (geometry.get(&node), places.get(&node)) else {
            return;
        };
        let top_left = to_screen(*place);
        let node_rect = Rect::from_min_size(top_left, geometry.size * z);
        if !node_rect.intersects(rect) {
            return;
        }
        let highlighted = matches!(node, NodeRef::Block(id) if context.highlighted == Some(id));
        draw_node(
            &mut canvas,
            &mut drawer,
            NodeDraw {
                node: &node,
                body: &mut body,
                geometry,
                rect: node_rect,
                runs,
                selected: selected.contains(&node),
                highlighted,
                wired: &wired,
                wire_held,
                objects: context.objects,
            },
            &mut events,
        );
    });

    // The wire being dragged, from its pin to the pointer.
    let pointer = canvas.input(|i| i.pointer.interact_pos());
    if let (Some(from), Some(pointer)) = (&state.wire, pointer)
        && let Some(start) = pin_at(from)
    {
        let color = match from {
            Pin::Input(at) | Pin::Output(at) => slot_kind(script, at)
                .or_else(|| value(script, at).map(value_type))
                .map_or(exec_wire, pin_color),
            _ => exec_wire,
        };
        let (a, b) = if matches!(from, Pin::ExecIn(_) | Pin::Input(_)) {
            (pointer, start)
        } else {
            (start, pointer)
        };
        draw_wire(&painter, a, b, color, 2.0 * z);
    }

    // The box being dragged out.
    if back.drag_started_by(egui::PointerButton::Primary) {
        state.boxing = back.interact_pointer_pos();
        if !canvas.input(|i| i.modifiers.shift) {
            state.selected.clear();
        }
    }
    if let (Some(start), Some(pointer)) = (state.boxing, pointer) {
        let area = Rect::from_two_pos(start, pointer);
        let accent = ui.visuals().selection.bg_fill;
        painter.rect(
            area,
            2,
            accent.gamma_multiply(0.15),
            Stroke::new(1.0_f32, accent),
            egui::StrokeKind::Inside,
        );
        if back.drag_stopped() {
            for (node, geometry) in &geometry {
                if let Some(place) = places.get(node) {
                    let node_rect = Rect::from_min_size(to_screen(*place), geometry.size * z);
                    if area.intersects(node_rect) {
                        state.selected.insert(node.clone());
                    }
                }
            }
            state.boxing = None;
        }
    }
    if !canvas.input(|i| i.pointer.primary_down()) {
        state.boxing = None;
    }
    if back.clicked() {
        state.selected.clear();
        state.menu = None;
    }

    let mut edited = false;
    let mut moved = Vec2::ZERO;
    for event in events {
        match event {
            Event::WireFrom(pin) => state.wire = Some(pin),
            Event::BreakPin(pin) => {
                freeze(&mut script.graph, &places);
                break_pin(script, &pin);
                edited = true;
            }
            Event::Select(node, add) => {
                if !add {
                    state.selected.clear();
                }
                if add && state.selected.contains(&node) {
                    state.selected.remove(&node);
                } else {
                    state.selected.insert(node);
                }
            }
            Event::MoveStart(node) => {
                if !state.selected.contains(&node) {
                    state.selected.clear();
                    state.selected.insert(node);
                }
                freeze(&mut script.graph, &places);
            }
            Event::Move(delta) => moved += delta,
            Event::Resize(node, grow) => {
                freeze(&mut script.graph, &places);
                resize_inputs(script, &node, grow);
                edited = true;
            }
            Event::Duplicate(node) => {
                freeze(&mut script.graph, &places);
                duplicate_at(script, &node, &places);
                edited = true;
            }
            Event::Delete(node) => {
                freeze(&mut script.graph, &places);
                let mut nodes: Vec<NodeRef> = state.selected.drain().collect();
                if !nodes.contains(&node) {
                    nodes = vec![node];
                }
                delete_nodes(script, &nodes);
                edited = true;
            }
        }
    }
    if moved != Vec2::ZERO {
        for node in &state.selected {
            let key = node.key();
            if let Some(at) = script.graph.nodes.get_mut(&key) {
                at[0] += moved.x / z;
                at[1] += moved.y / z;
            }
        }
    }

    // Letting go of a wire: on a pin, it joins; on the canvas, it opens
    // the node menu to make a node for it.
    if state.wire.is_some() && canvas.input(|i| i.pointer.any_released()) {
        let from = state.wire.take().expect("checked above");
        let target = pointer.and_then(|pointer| {
            pins.iter()
                .filter(|(pin, at)| {
                    *pin != from
                        && pin.exec() == from.exec()
                        && at.distance(pointer) <= PIN_REACH * z.max(0.6)
                })
                .min_by(|a, b| a.1.distance(pointer).total_cmp(&b.1.distance(pointer)))
                .map(|(pin, _)| pin.clone())
        });
        match target {
            Some(to) => {
                freeze(&mut script.graph, &places);
                match join(script, &from, &to) {
                    Ok(()) => edited = true,
                    Err(why) => {
                        let now = canvas.input(|i| i.time);
                        state.notice = Some((why.to_owned(), now + 4.0));
                    }
                }
            }
            None => {
                if let Some(pointer) = pointer
                    && rect.contains(pointer)
                {
                    state.menu = Some(Menu::new(pointer, to_graph(pointer), Some(from)));
                }
            }
        }
    }

    // Keys: Delete, Ctrl+D, Escape.
    let typing = canvas.ctx().memory(|memory| memory.focused().is_some());
    if hovered && !typing && state.menu.is_none() {
        let (delete, duplicate, escape) = canvas.input(|i| {
            (
                i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::Backspace),
                i.modifiers.command && i.key_pressed(egui::Key::D),
                i.key_pressed(egui::Key::Escape),
            )
        });
        if delete && !state.selected.is_empty() {
            freeze(&mut script.graph, &places);
            let nodes: Vec<NodeRef> = state.selected.drain().collect();
            delete_nodes(script, &nodes);
            edited = true;
        }
        if duplicate && !state.selected.is_empty() {
            freeze(&mut script.graph, &places);
            let nodes: Vec<NodeRef> = state.selected.drain().collect();
            for node in nodes {
                if let Some(copy) = duplicate_at(script, &node, &places) {
                    state.selected.insert(copy);
                }
            }
            edited = true;
        }
        if escape {
            state.selected.clear();
        }
    }

    // The node menu.
    if back.secondary_clicked()
        && let Some(pointer) = back.interact_pointer_pos()
    {
        state.menu = Some(Menu::new(pointer, to_graph(pointer), None));
    }
    if add_clicked {
        state.menu = Some(Menu::new(rect.center(), to_graph(rect.center()), None));
    }
    if let Some(mut menu) = state.menu.take() {
        let items = menu_items(script, menu.from.as_ref(), context, variables);
        match show_menu(ui, &mut menu, &items, context.key) {
            MenuChoice::Open => state.menu = Some(menu),
            MenuChoice::Close => {}
            MenuChoice::Pick(index) => {
                freeze(&mut script.graph, &places);
                if let Some(node) = add_node(script, &items[index].item) {
                    script
                        .graph
                        .nodes
                        .insert(node.key(), [menu.graph.x.round(), menu.graph.y.round()]);
                    if let Some(from) = &menu.from
                        && let Err(why) = join_new(script, from, &node)
                    {
                        let now = canvas.input(|i| i.time);
                        state.notice = Some((why.to_owned(), now + 4.0));
                    }
                    state.selected = HashSet::from([node]);
                }
                edited = true;
            }
        }
    }

    // Why the last wire didn't join.
    let now = canvas.input(|i| i.time);
    if let Some((text, until)) = &state.notice {
        if now < *until {
            let at = pos2(rect.center().x, rect.bottom() - 24.0);
            let galley = canvas.fonts(|fonts| {
                fonts.layout_no_wrap(text.clone(), FontId::proportional(13.0), Color32::WHITE)
            });
            let back_rect = Rect::from_center_size(at, galley.size() + vec2(20.0, 10.0));
            painter.rect_filled(
                back_rect,
                6,
                Color32::from_rgba_unmultiplied(160, 40, 40, 230),
            );
            painter.galley(back_rect.min + vec2(10.0, 5.0), galley, Color32::WHITE);
            canvas
                .ctx()
                .request_repaint_after(std::time::Duration::from_millis(250));
        } else {
            state.notice = None;
        }
    }

    if edited {
        tidy(script);
        // Nodes that are gone don't keep their places.
        let keys: HashSet<String> = {
            let mut keys = HashSet::new();
            visit_nodes(script, &mut |node, _, _| {
                keys.insert(node.key());
            });
            keys
        };
        script.graph.nodes.retain(|key, _| keys.contains(key));
        state.selected.retain(|node| keys.contains(&node.key()));
    }
}

/// Joins a wire dragged from `from` and let go on `to`, in either order.
fn join(script: &mut BlockScript, from: &Pin, to: &Pin) -> Result<(), Refusal> {
    match (from, to) {
        (Pin::ExecOut(block, out), Pin::ExecIn(target))
        | (Pin::ExecIn(target), Pin::ExecOut(block, out)) => {
            connect_exec(script, *block, *out, *target)
        }
        (Pin::Output(value), Pin::Input(slot)) | (Pin::Input(slot), Pin::Output(value)) => {
            connect_value(script, value, slot)
        }
        _ => Err("Wire an output to an input."),
    }
}

/// Wires a node just made from the menu to the pin its wire came from.
fn join_new(script: &mut BlockScript, from: &Pin, node: &NodeRef) -> Result<(), Refusal> {
    match (from, node) {
        (Pin::ExecOut(..), NodeRef::Block(id)) => join(script, from, &Pin::ExecIn(*id)),
        (Pin::ExecIn(_), NodeRef::Block(id)) => {
            let kind = &find_block(script, *id).ok_or("That node is gone.")?.kind;
            let (out, _) = *exec_outs(kind).first().ok_or("It has no exec output.")?;
            join(script, from, &Pin::ExecOut(*id, out))
        }
        (Pin::Input(_), NodeRef::Value(at)) => join(script, from, &Pin::Output(at.clone())),
        (Pin::Output(at), node) => {
            let shape = value(script, at).ok_or("That node is gone.")?.shape();
            let slots = match node {
                NodeRef::Block(id) => {
                    block_slots(&find_block(script, *id).ok_or("That node is gone.")?.kind)
                }
                NodeRef::Value(at) => value_slots(value(script, at).ok_or("That node is gone.")?),
            };
            let slot = slots
                .iter()
                .position(|kind| kind.accepts(shape))
                .ok_or("It has no input this value fits.")?;
            join(script, from, &Pin::Input(node.slots().child(slot)))
        }
        _ => Err("That node can't be wired there."),
    }
}

/// Breaks every wire on `pin`.
fn break_pin(script: &mut BlockScript, pin: &Pin) {
    match pin {
        Pin::ExecIn(id) => break_exec_in(script, *id),
        Pin::ExecOut(id, out) => break_exec_out(script, *id, *out),
        Pin::Input(at) | Pin::Output(at) => break_value(script, at),
    }
}

/// Duplicates `node` a little below and right of it.
fn duplicate_at(
    script: &mut BlockScript,
    node: &NodeRef,
    places: &HashMap<NodeRef, Pos2>,
) -> Option<NodeRef> {
    let copy = duplicate_node(script, node)?;
    if let Some(place) = places.get(node) {
        script
            .graph
            .nodes
            .insert(copy.key(), [place.x + 30.0, place.y + 30.0]);
    }
    Some(copy)
}

fn draw_wire(painter: &egui::Painter, start: Pos2, end: Pos2, color: Color32, width: f32) {
    let reach = ((end.x - start.x).abs() * 0.5).max(40.0);
    let bezier = egui::epaint::CubicBezierShape::from_points_stroke(
        [start, start + vec2(reach, 0.0), end - vec2(reach, 0.0), end],
        false,
        Color32::TRANSPARENT,
        Stroke::new(width, color),
    );
    painter.add(bezier);
}

/// What `draw_node` needs about one node.
struct NodeDraw<'a, 'b> {
    node: &'a NodeRef,
    body: &'a mut Body<'b>,
    geometry: &'a Geometry,
    rect: Rect,
    runs: bool,
    selected: bool,
    highlighted: bool,
    wired: &'a Wiring,
    wire_held: bool,
    objects: &'a [ObjectInfo],
}

fn draw_node(
    ui: &mut egui::Ui,
    drawer: &mut BlockPainter,
    node: NodeDraw,
    events: &mut Vec<Event>,
) {
    let z = drawer.z;
    let NodeDraw {
        node: which,
        mut body,
        geometry,
        rect,
        runs,
        selected,
        highlighted,
        wired,
        wire_held,
        objects,
    } = node;
    let id = egui::Id::new(("graph node", which.key())).with(drawer.id);
    let grows = match &mut body {
        Body::Block(kind) => kind.input_list_mut().is_some(),
        Body::Value(value) => value.input_list_mut().is_some(),
    };

    // The node itself: click to select, drag to move.
    if drawer.interactive {
        let response = ui
            .interact(rect, id, Sense::click_and_drag())
            .on_hover_cursor(CursorIcon::Grab);
        let response = if runs {
            response
        } else {
            response.on_hover_text("Not wired to an event, so it doesn't run.")
        };
        if response.clicked() {
            let add = ui.input(|i| i.modifiers.shift || i.modifiers.command);
            events.push(Event::Select(which.clone(), add));
        }
        if response.drag_started_by(egui::PointerButton::Primary) {
            events.push(Event::MoveStart(which.clone()));
        }
        if response.dragged_by(egui::PointerButton::Primary) {
            events.push(Event::Move(response.drag_delta()));
        }
        response.context_menu(|ui| {
            if grows {
                if ui.button("Add Input").clicked() {
                    events.push(Event::Resize(which.clone(), true));
                    ui.close();
                }
                if ui.button("Remove Last Input").clicked() {
                    events.push(Event::Resize(which.clone(), false));
                    ui.close();
                }
            }
            if ui
                .button(theme::with_icon(icons::COPY, "Duplicate"))
                .clicked()
            {
                events.push(Event::Duplicate(which.clone()));
                ui.close();
            }
            if ui
                .button(theme::with_icon(icons::TRASH, "Delete"))
                .clicked()
            {
                events.push(Event::Delete(which.clone()));
                ui.close();
            }
        });
    }

    let painter = drawer.painter.clone();
    let colors = theme::canvas(ui.ctx());
    let corner = drawer.s(6.0);
    painter.rect_filled(
        rect.translate(vec2(0.0, drawer.s(3.0))),
        radius(corner),
        Color32::from_black_alpha(70),
    );
    painter.rect_filled(rect, radius(corner), colors.node_body);
    let header = Rect::from_min_size(rect.min, vec2(rect.width(), drawer.s(HEADER)));
    let mut fill = geometry.category.fill();
    if !runs {
        fill = fill.lerp_to_gamma(Color32::from_gray(90), 0.6);
    }
    let r = radius(corner);
    painter.rect_filled(
        header,
        CornerRadius {
            nw: r,
            ne: r,
            sw: 0,
            se: 0,
        },
        colors.header(fill),
    );
    painter.text(
        pos2(header.left() + drawer.s(10.0), header.center().y),
        egui::Align2::LEFT_CENTER,
        &geometry.title,
        FontId::proportional(13.0 * z),
        colors.header_text,
    );
    let outline = if highlighted {
        Stroke::new(2.5 * z.max(0.8), colors.error)
    } else if selected {
        Stroke::new(2.0 * z.max(0.8), colors.selected)
    } else {
        Stroke::new(1.0_f32, colors.node_edge)
    };
    painter.rect_stroke(rect, radius(corner), outline, egui::StrokeKind::Outside);

    // Rows: inputs and menus on the left, outputs on the right.
    let mut parts = parts_of(body, objects);
    let mut y = rect.top() + drawer.s(HEADER + 2.0);
    for (row, height) in geometry.rows.iter().enumerate() {
        let middle = y + drawer.s(*height) * 0.5;
        if let Some(item) = geometry.layout.left.get(row) {
            let pin_at = pos2(rect.left() + drawer.s(PIN + 6.0), middle);
            let text_x = rect.left() + drawer.s(PIN_SPACE);
            match item {
                Item::ExecIn => {
                    if let NodeRef::Block(block) = which {
                        let pin = Pin::ExecIn(*block);
                        exec_pin(&painter, pin_at, z, wired.fed.contains(block), &colors);
                        pin_response(ui, drawer, &pin, pin_at, wire_held, events);
                    }
                }
                Item::Input {
                    slot,
                    part,
                    kind,
                    name,
                } => {
                    let pin = Pin::Input(which.slots().child(*slot));
                    let mut connected = false;
                    if let Some(Part::Slot(value, _)) = parts.get(*part) {
                        connected = !value.is_literal();
                    }
                    value_pin(&painter, pin_at, z, pin_color(*kind), connected, &colors);
                    pin_response(ui, drawer, &pin, pin_at, wire_held, events);
                    painter.text(
                        pos2(text_x, middle),
                        egui::Align2::LEFT_CENTER,
                        *name,
                        drawer.font(),
                        colors.node_text,
                    );
                    if let Some(Part::Slot(value, kind)) = parts.get_mut(*part)
                        && value.is_literal()
                    {
                        let size = drawer.literal_size(ui, value, *kind);
                        let left = text_x + drawer.text_width(ui, name) + drawer.s(NAME_GAP);
                        let input = Rect::from_min_size(pos2(left, middle - size.y * 0.5), size);
                        drawer.literal(ui, value, *kind, input, id.with(("slot", *slot)));
                    }
                }
                Item::Field { part, name } => {
                    let label_x = rect.left() + drawer.s(10.0);
                    painter.text(
                        pos2(label_x, middle),
                        egui::Align2::LEFT_CENTER,
                        *name,
                        drawer.font(),
                        colors.node_text,
                    );
                    if let Some(field) = parts.get_mut(*part) {
                        let size = drawer.part_size(ui, field);
                        let left = label_x + drawer.text_width(ui, name) + drawer.s(NAME_GAP);
                        let field_rect =
                            Rect::from_min_size(pos2(left, middle - size.y * 0.5), size);
                        drawer.draw_field(ui, field, field_rect, id.with(("part", *part)));
                    }
                }
                Item::Code { part } => {
                    if let Some(field) = parts.get_mut(*part) {
                        let size = drawer.part_size(ui, field);
                        let field_rect = Rect::from_min_size(
                            pos2(rect.left() + drawer.s(10.0), middle - size.y * 0.5),
                            size,
                        );
                        drawer.draw_field(ui, field, field_rect, id.with(("part", *part)));
                    }
                }
                Item::ExecOut(..) | Item::Output(_) => {}
            }
        }
        if let Some(item) = geometry.layout.right.get(row) {
            let pin_at = pos2(rect.right() - drawer.s(PIN + 6.0), middle);
            let text_right = rect.right() - drawer.s(PIN_SPACE);
            match item {
                Item::ExecOut(out, name) => {
                    if let NodeRef::Block(block) = which {
                        let pin = Pin::ExecOut(*block, *out);
                        exec_pin(
                            &painter,
                            pin_at,
                            z,
                            wired.targets.contains_key(&(*block, *out)),
                            &colors,
                        );
                        pin_response(ui, drawer, &pin, pin_at, wire_held, events);
                        painter.text(
                            pos2(text_right, middle),
                            egui::Align2::RIGHT_CENTER,
                            *name,
                            drawer.font(),
                            colors.node_text,
                        );
                    }
                }
                Item::Output(kind) => {
                    if let NodeRef::Value(at) = which {
                        let pin = Pin::Output(at.clone());
                        value_pin(
                            &painter,
                            pin_at,
                            z,
                            pin_color(*kind),
                            !at.path.is_empty(),
                            &colors,
                        );
                        pin_response(ui, drawer, &pin, pin_at, wire_held, events);
                    }
                }
                _ => {}
            }
        }
        y += drawer.s(*height);
    }
}

/// A pin's pointer handling: drag to wire, Alt-click or right-click to
/// break its wires.
fn pin_response(
    ui: &mut egui::Ui,
    drawer: &BlockPainter,
    pin: &Pin,
    at: Pos2,
    wire_held: bool,
    events: &mut Vec<Event>,
) {
    if !drawer.interactive {
        return;
    }
    let reach = drawer.s(PIN + 5.0);
    let rect = Rect::from_center_size(at, Vec2::splat(reach * 2.0));
    let response = ui
        .interact(
            rect,
            egui::Id::new(("graph pin", pin.clone())).with(drawer.id),
            Sense::click_and_drag(),
        )
        .on_hover_cursor(CursorIcon::Crosshair);
    if response.hovered() && !wire_held {
        drawer.painter.circle_stroke(
            at,
            reach,
            Stroke::new(
                1.0_f32,
                theme::canvas(ui.ctx()).node_text.gamma_multiply(0.35),
            ),
        );
    }
    if response.drag_started_by(egui::PointerButton::Primary) {
        events.push(Event::WireFrom(pin.clone()));
    }
    if response.clicked() && ui.input(|i| i.modifiers.alt) {
        events.push(Event::BreakPin(pin.clone()));
    }
    response.context_menu(|ui| {
        if ui
            .button(theme::with_icon(icons::LINK_BREAK, "Break link"))
            .clicked()
        {
            events.push(Event::BreakPin(pin.clone()));
            ui.close();
        }
    });
}

/// An exec pin: Unreal's white arrow, filled when it's wired.
fn exec_pin(painter: &egui::Painter, at: Pos2, z: f32, connected: bool, colors: &theme::Canvas) {
    let s = |x: f32, y: f32| at + vec2(x * z, y * z);
    let points = vec![
        s(-5.0, -6.0),
        s(1.0, -6.0),
        s(6.0, 0.0),
        s(1.0, 6.0),
        s(-5.0, 6.0),
    ];
    let fill = if connected {
        colors.wire
    } else {
        Color32::TRANSPARENT
    };
    painter.add(Shape::convex_polygon(
        points,
        fill,
        Stroke::new(1.5 * z.max(0.7), colors.wire),
    ));
}

/// A value pin: a circle in its type's colour, filled when it's wired.
fn value_pin(
    painter: &egui::Painter,
    at: Pos2,
    z: f32,
    color: Color32,
    connected: bool,
    colors: &theme::Canvas,
) {
    let radius = PIN * z;
    if connected {
        painter.circle_filled(at, radius, color);
    } else {
        painter.circle(
            at,
            radius,
            colors.node_body,
            Stroke::new(1.5 * z.max(0.7), color),
        );
    }
}

#[cfg(test)]
mod tests;
