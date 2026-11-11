//! A generic node-graph editor: Unreal Blueprint-style nodes with typed
//! pins, wires, comments, pan and zoom, box select, copy and paste, and a
//! searchable node menu.
//!
//! It edits any [`NodeGraph`], and what nodes there are comes from a
//! [`NodeLibrary`], so the UI Event Graph (the UI Designer's Event Graph
//! tab) and behavior trees (the Behavior Tree tab) share it. The view only keeps where it's looking
//! and what's selected ([`GraphView`]); the graph is the caller's.
//!
//! - Right-click the canvas (or **Add node**) for the node menu; drop a
//!   wire on the canvas for it filtered to nodes that fit the wire, and the
//!   new node is wired to it.
//! - Drag from a pin to another to wire them, from either end. Alt-click a
//!   pin, or right-click it > Break links, to break its wires.
//! - Click a node to select it (Shift or Ctrl adds); left-drag the
//!   background for a box. Drag a node to move the selection. Delete
//!   deletes, Ctrl+D duplicates, Ctrl+C, Ctrl+X and Ctrl+V copy, cut and
//!   paste (as text, so between graphs too).
//! - **Comment** puts a box behind the selected nodes; drag its title to
//!   move it with the nodes inside, its corner to resize it, and
//!   double-click its title to rename it.
//! - Right- or middle-drag and the wheel pan; Ctrl + wheel (or pinching)
//!   zooms. **Frame all** fits every node into view.
//!
//! Each finished edit comes back as [`GraphResponse::step`], the graph as
//! it was before, so a drag or a typed value is one undo step.

mod draw;
mod menu;
#[cfg(test)]
mod tests;
mod variables;

use std::collections::{BTreeMap, BTreeSet};

use axcenity_blocks::node_graph::{
    FieldKind, FieldSpec, GraphNode, NodeGraph, NodeLibrary, NodeSpec, ObjectRoot, PinRef, PinSpec,
    PinType,
};
use axcenity_blocks::ui_graph::GraphObject;
use eframe::egui::{
    self, Color32, CornerRadius, CursorIcon, FontId, Pos2, Rect, Sense, Stroke, Vec2, pos2, vec2,
};

use crate::theme::{self, icons};
pub use draw::draw_wire;
use draw::{NodeDraw, draw_node, scale_style};
use menu::{MenuChoice, NodeMenu};
pub use variables::variables_panel;

// Sizes at zoom 1, in graph units (points).
const HEADER: f32 = 28.0;
const ROW: f32 = 26.0;
const PIN: f32 = 5.0;
/// From a node's edge to the text beside a pin.
const PIN_SPACE: f32 = 20.0;
const MIN_WIDTH: f32 = 170.0;
/// The width of a field's control, and of a value typed into a pin.
const FIELD_WIDTH: f32 = 130.0;
/// The widest a field's control grows to fit its value; longer is cut short.
const MAX_FIELD_WIDTH: f32 = 280.0;
const VALUE_WIDTH: f32 = 70.0;
/// A comment's title strip.
const COMMENT_HEADER: f32 = 26.0;
/// How near a pin a wire has to be let go to join it.
const PIN_REACH: f32 = 14.0;
const MIN_ZOOM: f32 = 0.25;
const MAX_ZOOM: f32 = 2.0;
/// The least a graph opens at, so its text reads without zooming in. A
/// bigger graph opens on its start node instead of all of it.
const OPEN_ZOOM: f32 = 0.8;
/// Below this zoom, values show as text rather than boxes to type in.
const WIDGETS_ZOOM: f32 = 0.55;
pub const TOOLBAR_HEIGHT: f32 = 34.0;

/// Where a graph starts: its first event or root node, else its top left
/// node.
fn start_node(graph: &NodeGraph, library: &dyn NodeLibrary) -> Option<u32> {
    let at = |node: &&GraphNode| (node.position[0], node.position[1]);
    let first = |nodes: Vec<&GraphNode>| {
        nodes
            .into_iter()
            .min_by(|a, b| {
                at(a)
                    .partial_cmp(&at(b))
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|node| node.id)
    };
    let starts: Vec<&GraphNode> = graph
        .nodes
        .iter()
        .filter(|node| {
            library
                .spec(&node.kind)
                .is_some_and(|spec| matches!(spec.category, "Events" | "Root"))
        })
        .collect();
    first(starts).or_else(|| first(graph.nodes.iter().collect()))
}

/// A node category's header colour.
pub fn category_color(category: &str) -> Color32 {
    match category {
        // A behavior tree's Root starts it, as an event starts a chain.
        "Events" | "Root" => Color32::from_rgb(168, 42, 42),
        "Actions" => Color32::from_rgb(52, 104, 186),
        "Flow" | "Composites" => Color32::from_rgb(92, 96, 110),
        "Conditions" => Color32::from_rgb(170, 112, 36),
        "Data" => Color32::from_rgb(44, 134, 92),
        "Variables" | "Parameters" => Color32::from_rgb(128, 78, 170),
        // A material graph's.
        "Output" => Color32::from_rgb(168, 42, 42),
        "Inputs" => Color32::from_rgb(44, 112, 140),
        "Textures" => Color32::from_rgb(52, 104, 186),
        "Color" => Color32::from_rgb(176, 96, 40),
        "Noise" => Color32::from_rgb(40, 132, 120),
        _ => Color32::from_rgb(92, 96, 110),
    }
}

/// A pin type's colour; exec pins are white.
pub fn pin_color(ty: PinType) -> Color32 {
    match ty {
        PinType::Exec => Color32::WHITE,
        PinType::Number => Color32::from_rgb(106, 214, 92),
        PinType::Text => Color32::from_rgb(241, 105, 190),
        PinType::Bool => Color32::from_rgb(220, 64, 64),
        PinType::Vector2 => Color32::from_rgb(232, 196, 64),
        PinType::Vector3 => Color32::from_rgb(242, 140, 52),
        PinType::Any => Color32::from_rgb(160, 162, 172),
    }
}

/// What the editor needs besides the graph.
pub struct GraphContext<'a> {
    pub library: &'a dyn NodeLibrary,
    /// What object fields can pick.
    pub objects: &'a [GraphObject],
    /// What image fields can pick: the project's images, by path.
    pub images: &'a [String],
    /// Errors to show on nodes, by node id.
    pub errors: &'a BTreeMap<u32, Vec<String>>,
    /// False: the graph is shown but not edited.
    pub editable: bool,
    /// Tells this graph's egui ids from another's.
    pub key: &'a str,
}

/// What happened in the editor this frame.
#[derive(Debug, Default)]
pub struct GraphResponse {
    /// The graph changed this frame.
    pub changed: bool,
    /// An edit finished: the graph as it was before it (for undo).
    pub step: Option<NodeGraph>,
    /// The canvas has the pointer, so keys such as Ctrl+Z are the graph's.
    pub hovered: bool,
}

/// A node's measurements at zoom 1.
#[derive(Clone, Debug)]
pub struct NodeShape {
    pub size: Vec2,
    /// Field rows, then pin rows (an input and an output side by side).
    fields: Vec<FieldSpec>,
    /// Each field's control width, fitted to its value.
    field_widths: Vec<f32>,
    pin_rows: Vec<(Option<PinSpec>, Option<PinSpec>)>,
    /// The errors under the rows, and their height.
    errors: Vec<String>,
    errors_height: f32,
}

impl NodeShape {
    /// Where pin row `row` is, from the node's top, at zoom 1.
    fn row_middle(&self, row: usize) -> f32 {
        HEADER + 4.0 + (self.fields.len() + row) as f32 * ROW + ROW * 0.5
    }

    /// Where `pin` is, from the node's top left, at zoom 1.
    pub fn pin_offset(&self, pin: &str, output: bool) -> Option<Vec2> {
        self.pin_rows
            .iter()
            .enumerate()
            .find_map(|(row, (input, out))| {
                let side = if output { out } else { input };
                side.filter(|spec| spec.name == pin).map(|_| {
                    let x = if output {
                        self.size.x - PIN - 6.0
                    } else {
                        PIN + 6.0
                    };
                    vec2(x, self.row_middle(row))
                })
            })
    }
}

/// How wide the box for a value typed into a pin of `ty` is: text grows
/// with what's typed, up to a limit.
fn value_width(ctx: &egui::Context, ty: PinType, value: &str) -> f32 {
    match ty {
        PinType::Text | PinType::Any | PinType::Vector2 | PinType::Vector3 => {
            (text_width(ctx, value, 12.0) + 18.0).clamp(VALUE_WIDTH, 200.0)
        }
        _ => VALUE_WIDTH,
    }
}

/// How wide a field's control is: wide enough for the value it shows (a
/// menu's text and its arrow), from [`FIELD_WIDTH`] up to [`MAX_FIELD_WIDTH`].
fn field_width(ctx: &egui::Context, field: &FieldSpec, value: &str) -> f32 {
    match field.kind {
        FieldKind::Color => FIELD_WIDTH,
        _ => (text_width(ctx, &draw::shown_value(field, value), 12.0) + 48.0)
            .clamp(FIELD_WIDTH, MAX_FIELD_WIDTH),
    }
}

/// `N` numbers typed as `1, 0.5, 0` (a vector or a color), or `None` when
/// that isn't what `text` is. One number fills all `N`.
pub fn parse_numbers<const N: usize>(text: &str) -> Option<[f32; N]> {
    let numbers: Vec<f32> = text
        .split(',')
        .map(|part| part.trim().parse::<f32>().ok().filter(|n| n.is_finite()))
        .collect::<Option<_>>()?;
    match numbers.len() {
        1 => Some([numbers[0]; N]),
        n if n == N => numbers.try_into().ok(),
        _ => None,
    }
}

/// Numbers as [`parse_numbers`] reads them.
pub fn numbers_text(numbers: &[f32]) -> String {
    let parts: Vec<String> = numbers.iter().map(|n| format!("{}", n)).collect();
    parts.join(", ")
}

fn text_width(ctx: &egui::Context, text: &str, size: f32) -> f32 {
    ctx.fonts_mut(|fonts| {
        fonts
            .layout_no_wrap(text.to_owned(), FontId::proportional(size), Color32::WHITE)
            .size()
            .x
    })
}

/// Measures `node` of `spec`, with its `errors`.
pub fn node_shape(
    ctx: &egui::Context,
    spec: &NodeSpec,
    node: &GraphNode,
    graph: &NodeGraph,
    errors: &[String],
) -> NodeShape {
    let mut width = MIN_WIDTH.max(text_width(ctx, spec.title, 13.0) + 40.0);
    let field_widths: Vec<f32> = spec
        .fields
        .iter()
        .map(|field| field_width(ctx, field, node.value(field.name)))
        .collect();
    for (field, control) in spec.fields.iter().zip(&field_widths) {
        width = width.max(text_width(ctx, field.name, 12.0) + control + 28.0);
    }
    let rows = spec.inputs.len().max(spec.outputs.len());
    let mut pin_rows = Vec::with_capacity(rows);
    for row in 0..rows {
        let input = spec.inputs.get(row).copied();
        let output = spec.outputs.get(row).copied();
        let mut row_width = 2.0 * PIN_SPACE + 24.0;
        if let Some(input) = input {
            row_width += text_width(ctx, input.name, 12.0);
            let wired = graph
                .wires_into(&PinRef::new(node.id, input.name))
                .next()
                .is_some();
            if input.ty != PinType::Exec && input.typed && !wired {
                row_width += value_width(ctx, input.ty, node.value(input.name)) + 6.0;
            }
        }
        if let Some(output) = output {
            row_width += text_width(ctx, output.name, 12.0);
        }
        width = width.max(row_width);
        pin_rows.push((input, output));
    }
    let errors_height = if errors.is_empty() {
        0.0
    } else {
        ctx.fonts_mut(|fonts| {
            errors
                .iter()
                .map(|error| {
                    fonts
                        .layout(
                            error.clone(),
                            FontId::proportional(11.5),
                            Color32::WHITE,
                            width - 16.0,
                        )
                        .size()
                        .y
                        + 4.0
                })
                .sum::<f32>()
        }) + 8.0
    };
    let height = HEADER + 4.0 + (spec.fields.len() + rows) as f32 * ROW + 6.0 + errors_height;
    NodeShape {
        size: vec2(width.round(), height.round()),
        fields: spec.fields.to_vec(),
        field_widths,
        pin_rows,
        errors: errors.to_vec(),
        errors_height,
    }
}

/// A comment drag under way.
#[derive(Clone, Debug)]
enum CommentDrag {
    /// Its title: it moves with these nodes, inside it when the drag began.
    Move {
        id: u32,
        nodes: Vec<u32>,
    },
    Resize(u32),
}

/// Where the editor is looking and what's selected. Kept per graph while
/// the editor runs.
#[derive(Debug)]
pub struct GraphView {
    pub pan: Vec2,
    pub zoom: f32,
    pub selected: BTreeSet<u32>,
    pub selected_comments: BTreeSet<u32>,
    /// A wire being dragged out of this pin (an output when true).
    wire: Option<(PinRef, bool)>,
    /// Where a box selection started, on screen.
    boxing: Option<Pos2>,
    /// The selected nodes are being dragged.
    moving: bool,
    comment_drag: Option<CommentDrag>,
    /// The comment whose title is being typed.
    renaming: Option<u32>,
    menu: Option<NodeMenu>,
    /// Why the last wire didn't join, and until when to say so.
    notice: Option<(String, f64)>,
    /// The graph before the edit under way (a drag, or typing), for undo.
    before: Option<NodeGraph>,
    /// Fit every node into view on the next frame.
    frame_next: bool,
    /// Whether the graph has been framed since it opened: the first time,
    /// it never zooms out past [`OPEN_ZOOM`].
    opened: bool,
    /// Every pin on screen last frame: the pin, whether it's an output,
    /// and where.
    pins: Vec<(PinRef, bool, Pos2)>,
    /// Every node's rectangle on screen last frame.
    rects: Vec<(u32, Rect)>,
    /// Where the canvas was last frame.
    canvas: Rect,
}

impl Default for GraphView {
    fn default() -> Self {
        Self {
            pan: vec2(40.0, 40.0),
            zoom: 1.0,
            selected: BTreeSet::new(),
            selected_comments: BTreeSet::new(),
            wire: None,
            boxing: None,
            moving: false,
            comment_drag: None,
            renaming: None,
            menu: None,
            notice: None,
            before: None,
            frame_next: true,
            opened: false,
            pins: Vec::new(),
            rects: Vec::new(),
            canvas: Rect::NOTHING,
        }
    }
}

/// Something done to a node this frame, applied after drawing.
enum Event {
    Select(u32, bool),
    MoveStart(u32),
    Move(Vec2),
    MoveEnd,
    WireFrom(PinRef, bool),
    Break(PinRef, bool),
    Duplicate(u32),
    Copy(u32),
    Delete(u32),
}

impl GraphView {
    /// Where `pin` was on screen last frame.
    pub fn pin_on_screen(&self, pin: &PinRef, output: bool) -> Option<Pos2> {
        self.pins
            .iter()
            .find(|(candidate, out, _)| candidate == pin && *out == output)
            .map(|(_, _, at)| *at)
    }

    /// Where node `id` was on screen last frame.
    pub fn node_on_screen(&self, id: u32) -> Option<Rect> {
        self.rects
            .iter()
            .find(|(node, _)| *node == id)
            .map(|(_, rect)| *rect)
    }

    /// The bar over the graph and the graph under it, in `rect`.
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        rect: Rect,
        graph: &mut NodeGraph,
        context: &GraphContext,
    ) -> GraphResponse {
        let before = graph.clone();
        let toolbar = Rect::from_min_size(rect.min, vec2(rect.width(), TOOLBAR_HEIGHT));
        let canvas = Rect::from_min_max(pos2(rect.left(), toolbar.bottom()), rect.max);
        let add = self.toolbar(ui, toolbar, graph, context);
        let hovered = self.canvas(ui, canvas, graph, context, add);
        ui.advance_cursor_after_rect(rect);

        // A drag or typing is one step, from when it began.
        let typing = ui.ctx().memory(|memory| memory.focused().is_some())
            && ui.ctx().egui_wants_keyboard_input();
        let ongoing = self.moving || self.comment_drag.is_some() || typing;
        let changed = *graph != before;
        if changed && self.before.is_none() {
            self.before = Some(before);
        }
        let mut step = None;
        if !ongoing && let Some(before) = self.before.take() {
            step = (before != *graph).then_some(before);
        }
        // Nothing stays selected that's gone.
        self.selected
            .retain(|id| graph.nodes.iter().any(|node| node.id == *id));
        self.selected_comments
            .retain(|id| graph.comments.iter().any(|comment| comment.id == *id));
        GraphResponse {
            changed,
            step,
            hovered,
        }
    }

    /// Add node, Comment, the zoom and Frame all. Returns true when Add
    /// node was clicked.
    fn toolbar(
        &mut self,
        ui: &mut egui::Ui,
        rect: Rect,
        graph: &mut NodeGraph,
        context: &GraphContext,
    ) -> bool {
        let palette = theme::palette(ui.ctx());
        ui.painter().rect_filled(rect, 0, palette.panel);
        ui.painter().hline(
            rect.x_range(),
            rect.bottom() - 0.5,
            Stroke::new(1.0_f32, palette.border),
        );
        let mut bar = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(rect.shrink2(vec2(8.0, 4.0)))
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
        );
        let mut add = false;
        bar.add_enabled_ui(context.editable, |ui| {
            if ui
                .button(theme::with_icon(icons::PLUS, "Add node"))
                .on_hover_text("Or right-click the graph, or drop a wire on it")
                .clicked()
            {
                add = true;
            }
            if ui
                .button(theme::with_icon(icons::CHAT_TEXT, "Comment"))
                .on_hover_text("Puts a box with a title behind the selected nodes")
                .clicked()
            {
                self.comment_selection(graph);
            }
        });
        bar.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui
                .button(icons::CORNERS_OUT)
                .on_hover_text("Frame all: fit every node into view")
                .clicked()
            {
                self.frame_next = true;
            }
            if ui
                .button(icons::MAGNIFYING_GLASS_PLUS)
                .on_hover_text("Zoom in")
                .clicked()
            {
                self.zoom = (self.zoom * 1.2).min(MAX_ZOOM);
            }
            if ui
                .button(format!("{:.0}%", self.zoom * 100.0))
                .on_hover_text("Back to 100%")
                .clicked()
            {
                self.zoom = 1.0;
            }
            if ui
                .button(icons::MAGNIFYING_GLASS_MINUS)
                .on_hover_text("Zoom out")
                .clicked()
            {
                self.zoom = (self.zoom / 1.2).max(MIN_ZOOM);
            }
        });
        add
    }

    /// Adds a comment around the selected nodes, or in the middle of the view.
    fn comment_selection(&mut self, graph: &mut NodeGraph) {
        let around = self
            .rects
            .iter()
            .filter(|(id, _)| self.selected.contains(id))
            .map(|(_, rect)| *rect)
            .reduce(|a, b| a.union(b));
        let (position, size) = match around {
            Some(area) => {
                let min = self.to_graph(area.min);
                let max = self.to_graph(area.max);
                (
                    [min.x - 20.0, min.y - COMMENT_HEADER - 16.0],
                    [max.x - min.x + 40.0, max.y - min.y + COMMENT_HEADER + 36.0],
                )
            }
            None => {
                let at = pos2(
                    -self.pan.x / self.zoom + 40.0,
                    -self.pan.y / self.zoom + 40.0,
                );
                ([at.x, at.y], [320.0, 180.0])
            }
        };
        let id = graph.add_comment(position, size);
        self.selected_comments = BTreeSet::from([id]);
        self.renaming = Some(id);
    }

    /// A screen point as a graph point, on last frame's canvas.
    fn to_graph(&self, at: Pos2) -> Pos2 {
        let origin = self.canvas.min + self.pan;
        pos2((at.x - origin.x) / self.zoom, (at.y - origin.y) / self.zoom)
    }

    /// The canvas. Returns whether it has the pointer.
    fn canvas(
        &mut self,
        ui: &mut egui::Ui,
        rect: Rect,
        graph: &mut NodeGraph,
        context: &GraphContext,
        add_clicked: bool,
    ) -> bool {
        let ctx = ui.ctx().clone();
        let colors = theme::canvas(&ctx);
        self.canvas = rect;
        let mut canvas = ui.new_child(egui::UiBuilder::new().max_rect(rect));
        canvas.set_clip_rect(rect);
        let painter = canvas.painter_at(rect);
        painter.rect_filled(rect, 0, colors.background);

        let shapes: BTreeMap<u32, NodeShape> = graph
            .nodes
            .iter()
            .filter_map(|node| {
                let spec = context.library.spec(&node.kind)?;
                let errors = context.errors.get(&node.id).map_or(&[][..], Vec::as_slice);
                Some((node.id, node_shape(&ctx, spec, node, graph, errors)))
            })
            .collect();
        if self.frame_next {
            self.frame_next = false;
            let start = (!self.opened)
                .then(|| start_node(graph, context.library))
                .flatten();
            self.fit(rect, graph, &shapes, start);
            self.opened = true;
        }

        let back = canvas.interact(
            rect,
            egui::Id::new(("node graph canvas", context.key)),
            Sense::click_and_drag(),
        );
        if back.dragged_by(egui::PointerButton::Secondary)
            || back.dragged_by(egui::PointerButton::Middle)
        {
            self.pan += back.drag_delta();
        }
        let hovered = canvas.rect_contains_pointer(rect);
        if hovered && self.menu.is_none() {
            let (scroll, zoom, pointer) =
                canvas.input(|i| (i.smooth_scroll_delta, i.zoom_delta(), i.pointer.hover_pos()));
            self.pan += scroll;
            if zoom != 1.0
                && let Some(pointer) = pointer
            {
                let old = self.zoom;
                self.zoom = (old * zoom).clamp(MIN_ZOOM, MAX_ZOOM);
                let from_origin = pointer - rect.min - self.pan;
                self.pan = pointer - rect.min - from_origin * (self.zoom / old);
            }
        }
        let z = self.zoom;
        let origin = rect.min + self.pan;
        let to_screen = |at: Pos2| origin + at.to_vec2() * z;
        let to_graph = |at: Pos2| pos2((at.x - origin.x) / z, (at.y - origin.y) / z);

        // A grid, which moves with the nodes.
        let spacing = 32.0 * z;
        let line = colors.grid;
        let mut x = rect.left() + self.pan.x.rem_euclid(spacing);
        while x < rect.right() {
            painter.vline(x, rect.y_range(), Stroke::new(1.0_f32, line));
            x += spacing;
        }
        let mut y = rect.top() + self.pan.y.rem_euclid(spacing);
        while y < rect.bottom() {
            painter.hline(rect.x_range(), y, Stroke::new(1.0_f32, line));
            y += spacing;
        }

        if graph.nodes.is_empty() && graph.comments.is_empty() {
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "Right-click to add a node, such as \"Button Activated\".",
                FontId::proportional(14.0),
                ui.visuals().weak_text_color(),
            );
        }

        // Comments, behind everything.
        self.comments(&mut canvas, graph, context, &shapes, to_screen);

        // Where every pin and node is on screen.
        self.pins.clear();
        self.rects.clear();
        for node in &graph.nodes {
            let Some(shape) = shapes.get(&node.id) else {
                continue;
            };
            let top_left = to_screen(pos2(node.position[0], node.position[1]));
            self.rects
                .push((node.id, Rect::from_min_size(top_left, shape.size * z)));
            for (input, output) in &shape.pin_rows {
                for (spec, out) in [(input, false), (output, true)] {
                    if let Some(spec) = spec
                        && let Some(offset) = shape.pin_offset(spec.name, out)
                    {
                        self.pins.push((
                            PinRef::new(node.id, spec.name),
                            out,
                            top_left + offset * z,
                        ));
                    }
                }
            }
        }

        // Wires, under the nodes.
        let exec_wire = colors.wire;
        for wire in &graph.wires {
            let (Some(start), Some(end)) = (
                self.pin_on_screen(&wire.from, true),
                self.pin_on_screen(&wire.to, false),
            ) else {
                continue;
            };
            let ty = graph
                .pin_type(context.library, &wire.from, true)
                .unwrap_or(PinType::Any);
            let (color, width) = match ty {
                PinType::Exec => (exec_wire, 2.5),
                ty => (pin_color(ty), 2.0),
            };
            draw_wire(&painter, start, end, color, width * z);
        }

        // The nodes, with their controls drawn at the zoom.
        let mut scaled = canvas.new_child(egui::UiBuilder::new().max_rect(rect));
        scale_style(&mut scaled, z);
        let mut events = Vec::new();
        let interactive = context.editable && self.menu.is_none();
        let wire_held = self.wire.is_some();
        for index in 0..graph.nodes.len() {
            let id = graph.nodes[index].id;
            let (Some(shape), Some(spec)) = (
                shapes.get(&id),
                context.library.spec(&graph.nodes[index].kind),
            ) else {
                continue;
            };
            let node_rect = self.node_on_screen(id).expect("measured above");
            if !node_rect.intersects(rect) {
                continue;
            }
            let wired: BTreeSet<(String, bool)> = graph
                .wires
                .iter()
                .flat_map(|wire| {
                    [
                        (wire.from.node == id).then(|| (wire.from.pin.clone(), true)),
                        (wire.to.node == id).then(|| (wire.to.pin.clone(), false)),
                    ]
                })
                .flatten()
                .collect();
            let variables: Vec<String> = graph
                .variables
                .iter()
                .map(|variable| variable.name.clone())
                .collect();
            let draw = NodeDraw {
                spec,
                shape,
                rect: node_rect,
                z,
                selected: self.selected.contains(&id),
                interactive,
                wire_held,
                wired: &wired,
                objects: context.objects,
                images: context.images,
                variables: &variables,
                key: context.key,
                colors,
            };
            draw_node(
                &mut scaled,
                &painter,
                draw,
                &mut graph.nodes[index],
                &mut events,
            );
        }

        // The wire being dragged, from its pin to the pointer.
        let pointer = canvas.input(|i| i.pointer.interact_pos());
        if let (Some((from, output)), Some(pointer)) = (&self.wire, pointer)
            && let Some(start) = self.pin_on_screen(from, *output)
        {
            let ty = graph
                .pin_type(context.library, from, *output)
                .unwrap_or(PinType::Any);
            let color = if ty == PinType::Exec {
                exec_wire
            } else {
                pin_color(ty)
            };
            let (a, b) = if *output {
                (start, pointer)
            } else {
                (pointer, start)
            };
            draw_wire(&painter, a, b, color, 2.0 * z);
        }

        // The box being dragged out.
        if context.editable && back.drag_started_by(egui::PointerButton::Primary) {
            self.boxing = back.interact_pointer_pos();
            if !canvas.input(|i| i.modifiers.shift || i.modifiers.command) {
                self.selected.clear();
                self.selected_comments.clear();
            }
        }
        if let (Some(start), Some(pointer)) = (self.boxing, pointer) {
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
                for (id, node_rect) in &self.rects {
                    if area.intersects(*node_rect) {
                        self.selected.insert(*id);
                    }
                }
                self.boxing = None;
            }
        }
        if !canvas.input(|i| i.pointer.primary_down()) {
            self.boxing = None;
        }
        if back.clicked() {
            self.selected.clear();
            self.selected_comments.clear();
            self.renaming = None;
            self.menu = None;
        }

        // What was done to the nodes.
        let mut moved = Vec2::ZERO;
        for event in events {
            match event {
                Event::WireFrom(pin, output) => self.wire = Some((pin, output)),
                Event::Break(pin, output) => graph.disconnect(&pin, output),
                Event::Select(id, add) => {
                    if !add {
                        self.selected.clear();
                        self.selected_comments.clear();
                    }
                    if add && self.selected.contains(&id) {
                        self.selected.remove(&id);
                    } else {
                        self.selected.insert(id);
                    }
                }
                Event::MoveStart(id) => {
                    if !self.selected.contains(&id) {
                        self.selected = BTreeSet::from([id]);
                        self.selected_comments.clear();
                    }
                    self.moving = true;
                }
                Event::Move(delta) => moved += delta,
                Event::MoveEnd => self.moving = false,
                Event::Duplicate(id) => {
                    let ids = self.acting_on(id);
                    self.duplicate(graph, &ids);
                }
                Event::Copy(id) => {
                    let ids = self.acting_on(id);
                    ctx.copy_text(copy_text(graph, &ids, &BTreeSet::new()));
                }
                Event::Delete(id) => {
                    let ids = self.acting_on(id);
                    graph.remove_nodes(&ids);
                }
            }
        }
        if !canvas.input(|i| i.pointer.primary_down()) {
            self.moving = false;
        }
        if moved != Vec2::ZERO {
            for node in &mut graph.nodes {
                if self.selected.contains(&node.id) {
                    node.position[0] += moved.x / z;
                    node.position[1] += moved.y / z;
                }
            }
        }

        // Letting go of a wire: on a pin, it joins; on the canvas, it
        // opens the node menu to make a node for it.
        if self.wire.is_some() && canvas.input(|i| i.pointer.any_released()) {
            let (from, output) = self.wire.take().expect("checked above");
            let target = pointer.and_then(|pointer| {
                self.pins
                    .iter()
                    .filter(|(pin, out, at)| {
                        *out != output
                            && pin.node != from.node
                            && at.distance(pointer) <= PIN_REACH * z.max(0.6)
                    })
                    .min_by(|a, b| a.2.distance(pointer).total_cmp(&b.2.distance(pointer)))
                    .map(|(pin, _, _)| pin.clone())
            });
            match target {
                Some(to) => {
                    let (out, input) = if output { (from, to) } else { (to, from) };
                    if let Err(why) = graph.connect(context.library, out, input) {
                        let now = canvas.input(|i| i.time);
                        self.notice = Some((why.to_owned(), now + 4.0));
                    }
                }
                None => {
                    if let Some(pointer) = pointer
                        && rect.contains(pointer)
                    {
                        self.menu = Some(NodeMenu::new(
                            pointer,
                            to_graph(pointer),
                            Some((from, output)),
                        ));
                    }
                }
            }
        }

        if context.editable && hovered && self.menu.is_none() {
            self.keys(&ctx, graph, pointer.map(to_graph));
        }

        // The node menu.
        if context.editable
            && back.secondary_clicked()
            && let Some(pointer) = back.interact_pointer_pos()
        {
            self.menu = Some(NodeMenu::new(pointer, to_graph(pointer), None));
        }
        if add_clicked {
            self.menu = Some(NodeMenu::new(rect.center(), to_graph(rect.center()), None));
        }
        if let Some(mut menu) = self.menu.take() {
            let specs = menu::fitting(graph, context.library, menu.from.as_ref());
            match menu::show(ui, &mut menu, &specs, context.key) {
                MenuChoice::Open => self.menu = Some(menu),
                MenuChoice::Close => {}
                MenuChoice::Pick(index) => {
                    let spec = specs[index];
                    let id = graph.add_node(spec, [menu.graph.x.round(), menu.graph.y.round()]);
                    if let Some((from, output)) = &menu.from
                        && let Err(why) = menu::wire_new(graph, context.library, from, *output, id)
                    {
                        let now = canvas.input(|i| i.time);
                        self.notice = Some((why.to_owned(), now + 4.0));
                    }
                    self.selected = BTreeSet::from([id]);
                    self.selected_comments.clear();
                }
            }
        }

        // Why the last wire didn't join.
        let now = canvas.input(|i| i.time);
        if let Some((text, until)) = &self.notice {
            if now < *until {
                let at = pos2(rect.center().x, rect.bottom() - 24.0);
                let galley = painter.layout_no_wrap(
                    text.clone(),
                    FontId::proportional(13.0),
                    Color32::WHITE,
                );
                let back_rect = Rect::from_center_size(at, galley.size() + vec2(20.0, 10.0));
                painter.rect_filled(
                    back_rect,
                    6,
                    Color32::from_rgba_unmultiplied(160, 40, 40, 230),
                );
                painter.galley(back_rect.min + vec2(10.0, 5.0), galley, Color32::WHITE);
                ctx.request_repaint_after(std::time::Duration::from_millis(250));
            } else {
                self.notice = None;
            }
        }
        hovered
    }

    /// The selected nodes, when `id` is one of them; else just `id`.
    fn acting_on(&self, id: u32) -> BTreeSet<u32> {
        if self.selected.contains(&id) {
            self.selected.clone()
        } else {
            BTreeSet::from([id])
        }
    }

    /// Copies `ids` a little below and right of where they are, and
    /// selects the copies.
    fn duplicate(&mut self, graph: &mut NodeGraph, ids: &BTreeSet<u32>) {
        let copied = graph.copy(ids, &BTreeSet::new());
        let Some(corner) = copied
            .nodes
            .iter()
            .map(|node| node.position)
            .reduce(|a, b| [a[0].min(b[0]), a[1].min(b[1])])
        else {
            return;
        };
        let (nodes, _) = graph.paste(&copied, [corner[0] + 30.0, corner[1] + 30.0]);
        self.selected = nodes;
        self.selected_comments.clear();
    }

    /// Delete, Ctrl+D, copy, cut and paste, select all and Escape, while
    /// nothing is being typed.
    fn keys(&mut self, ctx: &egui::Context, graph: &mut NodeGraph, pointer: Option<Pos2>) {
        if ctx.egui_wants_keyboard_input() {
            return;
        }
        let (delete, duplicate, all, escape, events) = ctx.input(|i| {
            use crate::shortcuts::{Action, pressed};
            (
                pressed(i, Action::DeleteNodes),
                pressed(i, Action::DuplicateNodes),
                pressed(i, Action::SelectAllNodes),
                pressed(i, Action::ClearNodeSelection),
                i.events.clone(),
            )
        });
        if delete {
            graph.remove_nodes(&self.selected);
            graph
                .comments
                .retain(|comment| !self.selected_comments.contains(&comment.id));
        }
        if duplicate {
            let ids = self.selected.clone();
            self.duplicate(graph, &ids);
        }
        if all {
            self.selected = graph.nodes.iter().map(|node| node.id).collect();
        }
        if escape {
            self.selected.clear();
            self.selected_comments.clear();
        }
        for event in events {
            match event {
                egui::Event::Copy | egui::Event::Cut
                    if !self.selected.is_empty() || !self.selected_comments.is_empty() =>
                {
                    ctx.copy_text(copy_text(graph, &self.selected, &self.selected_comments));
                    if event == egui::Event::Cut {
                        graph.remove_nodes(&self.selected);
                        graph
                            .comments
                            .retain(|comment| !self.selected_comments.contains(&comment.id));
                    }
                }
                egui::Event::Paste(text) => {
                    if let Ok(copied) = serde_json::from_str::<NodeGraph>(&text) {
                        let at = pointer.unwrap_or(pos2(0.0, 0.0));
                        let (nodes, comments) = graph.paste(&copied, [at.x.round(), at.y.round()]);
                        self.selected = nodes;
                        self.selected_comments = comments;
                    }
                }
                _ => {}
            }
        }
    }

    /// Fits every node and comment into `rect`.
    /// Fits every node into view; or, when the graph is opening (`start`
    /// is its start node), fits it only down to [`OPEN_ZOOM`] and shows its
    /// start if it doesn't fit.
    fn fit(
        &mut self,
        rect: Rect,
        graph: &NodeGraph,
        shapes: &BTreeMap<u32, NodeShape>,
        start: Option<u32>,
    ) {
        let nodes = graph.nodes.iter().filter_map(|node| {
            let shape = shapes.get(&node.id)?;
            Some(Rect::from_min_size(
                pos2(node.position[0], node.position[1]),
                shape.size,
            ))
        });
        let comments = graph.comments.iter().map(|comment| {
            Rect::from_min_size(
                pos2(comment.position[0], comment.position[1]),
                vec2(comment.size[0], comment.size[1]),
            )
        });
        let Some(area) = nodes.chain(comments).reduce(|a, b| a.union(b)) else {
            self.pan = vec2(40.0, 40.0);
            self.zoom = 1.0;
            return;
        };
        let room = (rect.size() - Vec2::splat(80.0)).max(vec2(1.0, 1.0));
        let fitted = (room.x / area.width().max(1.0))
            .min(room.y / area.height().max(1.0))
            .clamp(MIN_ZOOM, 1.0);
        self.zoom = fitted;
        self.pan = rect.size() * 0.5 - area.center().to_vec2() * self.zoom;
        let start = start.and_then(|id| {
            let node = graph.nodes.iter().find(|node| node.id == id)?;
            let size = shapes.get(&id)?.size;
            Some(Rect::from_min_size(
                pos2(node.position[0], node.position[1]),
                size,
            ))
        });
        if let Some(start) = start
            && fitted < OPEN_ZOOM
        {
            // Too big to read whole: its top left corner, or, if the start
            // node isn't in that, the start node in the middle (without
            // empty room before the graph's first nodes).
            let z = OPEN_ZOOM;
            self.zoom = z;
            let corner = Vec2::splat(40.0) - area.min.to_vec2() * z;
            let centred = rect.size() * 0.5 - start.center().to_vec2() * z;
            let shown = |pan: f32, min: f32, max: f32, size: f32| {
                pan + min * z >= 0.0 && pan + max * z <= size
            };
            self.pan = vec2(
                if shown(corner.x, start.min.x, start.max.x, rect.width()) {
                    corner.x
                } else {
                    centred.x.min(corner.x)
                },
                if shown(corner.y, start.min.y, start.max.y, rect.height()) {
                    corner.y
                } else {
                    centred.y.min(corner.y)
                },
            );
        }
    }

    /// Draws the comments and handles their titles and corners.
    fn comments(
        &mut self,
        canvas: &mut egui::Ui,
        graph: &mut NodeGraph,
        context: &GraphContext,
        shapes: &BTreeMap<u32, NodeShape>,
        to_screen: impl Fn(Pos2) -> Pos2,
    ) {
        let z = self.zoom;
        let colors = theme::canvas(canvas.ctx());
        let painter = canvas.painter().clone();
        let down = canvas.input(|i| i.pointer.primary_down());
        if !down {
            self.comment_drag = None;
        }
        for index in 0..graph.comments.len() {
            let comment = graph.comments[index].clone();
            let area = Rect::from_min_size(
                to_screen(pos2(comment.position[0], comment.position[1])),
                vec2(comment.size[0], comment.size[1]) * z,
            );
            let header = Rect::from_min_size(area.min, vec2(area.width(), COMMENT_HEADER * z));
            let selected = self.selected_comments.contains(&comment.id);
            painter.rect_filled(area, 6.0 * z, colors.comment_fill);
            painter.rect_filled(header, 6.0 * z, colors.comment_header);
            painter.rect_stroke(
                area,
                6.0 * z,
                if selected {
                    Stroke::new(2.0_f32, colors.selected)
                } else {
                    Stroke::new(1.0_f32, colors.comment_edge)
                },
                egui::StrokeKind::Inside,
            );
            let id = egui::Id::new(("node graph comment", context.key, comment.id));
            if self.renaming == Some(comment.id) && context.editable {
                let mut text = comment.text.clone();
                let edit = canvas.put(
                    header.shrink2(vec2(6.0 * z, 2.0 * z)),
                    egui::TextEdit::singleline(&mut text)
                        .font(FontId::proportional(14.0 * z))
                        .id(id.with("title")),
                );
                if !edit.has_focus() && !edit.lost_focus() {
                    edit.request_focus();
                }
                if edit.lost_focus() {
                    self.renaming = None;
                }
                graph.comments[index].text = text;
            } else {
                painter.text(
                    pos2(header.left() + 10.0 * z, header.center().y),
                    egui::Align2::LEFT_CENTER,
                    &comment.text,
                    FontId::proportional(14.0 * z),
                    colors.comment_text,
                );
            }
            if !context.editable {
                continue;
            }
            let title = canvas
                .interact(header, id, Sense::click_and_drag())
                .on_hover_cursor(CursorIcon::Grab);
            if title.clicked() {
                if !canvas.input(|i| i.modifiers.shift || i.modifiers.command) {
                    self.selected.clear();
                    self.selected_comments.clear();
                }
                self.selected_comments.insert(comment.id);
            }
            if title.double_clicked() {
                self.renaming = Some(comment.id);
            }
            if title.drag_started_by(egui::PointerButton::Primary) {
                let inside = graph
                    .nodes
                    .iter()
                    .filter(|node| {
                        let size = shapes.get(&node.id).map_or(Vec2::ZERO, |shape| shape.size);
                        let rect =
                            Rect::from_min_size(pos2(node.position[0], node.position[1]), size);
                        Rect::from_min_size(
                            pos2(comment.position[0], comment.position[1]),
                            vec2(comment.size[0], comment.size[1]),
                        )
                        .contains_rect(rect)
                    })
                    .map(|node| node.id)
                    .collect();
                self.comment_drag = Some(CommentDrag::Move {
                    id: comment.id,
                    nodes: inside,
                });
            }
            let corner = Rect::from_center_size(area.max - Vec2::splat(6.0), Vec2::splat(14.0));
            let resize = canvas
                .interact(corner, id.with("corner"), Sense::drag())
                .on_hover_cursor(CursorIcon::ResizeNwSe);
            painter.line_segment(
                [
                    corner.right_top() + vec2(-3.0, 3.0),
                    corner.left_bottom() + vec2(3.0, -3.0),
                ],
                Stroke::new(1.0_f32, colors.comment_text),
            );
            if resize.drag_started() {
                self.comment_drag = Some(CommentDrag::Resize(comment.id));
            }
            let delta = title.drag_delta() + resize.drag_delta();
            match &self.comment_drag {
                Some(CommentDrag::Move { id, nodes })
                    if *id == comment.id && delta != Vec2::ZERO =>
                {
                    let delta = delta / z;
                    let moved = &mut graph.comments[index];
                    moved.position[0] += delta.x;
                    moved.position[1] += delta.y;
                    for node in &mut graph.nodes {
                        if nodes.contains(&node.id) {
                            node.position[0] += delta.x;
                            node.position[1] += delta.y;
                        }
                    }
                }
                Some(CommentDrag::Resize(id)) if *id == comment.id && delta != Vec2::ZERO => {
                    let resized = &mut graph.comments[index];
                    resized.size[0] = (resized.size[0] + delta.x / z).max(120.0);
                    resized.size[1] = (resized.size[1] + delta.y / z).max(COMMENT_HEADER + 30.0);
                }
                _ => {}
            }
        }
    }
}

/// The selected nodes and comments as text, for the clipboard.
pub fn copy_text(graph: &NodeGraph, nodes: &BTreeSet<u32>, comments: &BTreeSet<u32>) -> String {
    serde_json::to_string(&graph.copy(nodes, comments)).expect("a graph is plain data")
}
