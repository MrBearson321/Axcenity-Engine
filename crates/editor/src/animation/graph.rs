//! The state machine graph: a controller's states as boxes and its
//! transitions as arrows, like Unity's Animator window.
//!
//! - Drag a state to move it; drag the background (or the middle button)
//!   to pan; the wheel zooms around the mouse.
//! - Click a state or an arrow to select it; click the background to select
//!   the controller itself.
//! - Right-click a state: Make Transition (then click where it goes), Set
//!   as Default State, Rename, Delete. Right-click the background to add a
//!   state, empty or playing a clip.
//!
//! It only reports what was done ([`GraphEvent`]); the controller editor
//! changes the file.

use axcenity_anim::{ANY_STATE, Motion, StateMachineFile};
use eframe::egui::{self, Color32, Pos2, Rect, Stroke, Vec2};

/// Where the Any State node is drawn until it's moved.
pub const ANY_STATE_POSITION: [f32; 2] = [-220.0, 120.0];
/// A state's box, at zoom 1.
const NODE: Vec2 = Vec2::new(150.0, 46.0);

/// A box in the graph.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Node {
    State(usize),
    Any,
}

/// What's selected in the graph.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Selected {
    /// The controller itself (its parameters).
    #[default]
    Controller,
    Node(Node),
    Transition(usize),
}

/// How the graph is panned and zoomed, kept between frames.
#[derive(Clone, Debug)]
pub struct GraphView {
    pub offset: Vec2,
    pub zoom: f32,
    /// A transition being made: from here to the mouse, until a state is clicked.
    pub linking: Option<Node>,
    /// Where the background was right-clicked, for Add State.
    menu_at: Option<[f32; 2]>,
}

impl Default for GraphView {
    fn default() -> Self {
        Self {
            offset: Vec2::new(300.0, 140.0),
            zoom: 1.0,
            linking: None,
            menu_at: None,
        }
    }
}

/// What changed in the graph this frame.
#[derive(Clone, Debug, PartialEq)]
pub enum GraphEvent {
    Select(Selected),
    /// A node was dragged to this graph position.
    Move(Node, [f32; 2]),
    /// A new transition from a node to a state.
    Link(Node, usize),
    SetDefault(usize),
    Rename(usize),
    Delete(Selected),
    /// Add a state here, playing this clip (or nothing).
    AddState([f32; 2], Option<String>),
    /// Add a state here playing a blend tree.
    AddBlendTree([f32; 2]),
    /// A state was double-clicked, to open its clip.
    Open(usize),
}

/// The state playing in the preview, to show in the graph.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Live {
    /// The state playing and how far through its pass, from 0 to 1.
    pub state: Option<(usize, f32)>,
    /// A blend in progress: from this state, by this transition, this far.
    pub fade: Option<(usize, Option<usize>, f32)>,
}

fn node_position(file: &StateMachineFile, node: Node) -> [f32; 2] {
    match node {
        Node::State(index) => file.states.get(index).map_or([0.0; 2], |s| s.position),
        Node::Any => file.any_state_position.unwrap_or(ANY_STATE_POSITION),
    }
}

/// The node a transition's `from` names.
pub fn from_node(file: &StateMachineFile, from: &str) -> Option<Node> {
    if from == ANY_STATE {
        return Some(Node::Any);
    }
    file.states
        .iter()
        .position(|s| s.name == from)
        .map(Node::State)
}

impl GraphView {
    fn to_screen(&self, rect: Rect, position: [f32; 2]) -> Pos2 {
        rect.min + self.offset + Vec2::new(position[0], position[1]) * self.zoom
    }

    fn to_graph(&self, rect: Rect, point: Pos2) -> [f32; 2] {
        let local = (point - rect.min - self.offset) / self.zoom;
        [local.x, local.y]
    }

    fn node_rect(&self, rect: Rect, file: &StateMachineFile, node: Node) -> Rect {
        let size = match node {
            Node::State(_) => NODE,
            Node::Any => Vec2::new(110.0, 36.0),
        } * self.zoom;
        Rect::from_center_size(self.to_screen(rect, node_position(file, node)), size)
    }

    /// Draws the graph filling `ui`'s space, and returns what was done.
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        file: &StateMachineFile,
        selected: Selected,
        live: Live,
        clips: &[String],
    ) -> Vec<GraphEvent> {
        let mut events = Vec::new();
        let size = ui.available_size();
        let (rect, canvas) = ui.allocate_exact_size(size, egui::Sense::click_and_drag());
        let visuals = ui.visuals().clone();
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, visuals.extreme_bg_color);

        // Pan and zoom.
        if canvas.dragged_by(egui::PointerButton::Primary)
            || canvas.dragged_by(egui::PointerButton::Middle)
        {
            self.offset += canvas.drag_delta();
        }
        if canvas.hovered() {
            let scroll = ui.input(|input| input.smooth_scroll_delta.y);
            if scroll != 0.0
                && let Some(pointer) = ui.input(|input| input.pointer.hover_pos())
            {
                let before = self.to_graph(rect, pointer);
                self.zoom = (self.zoom * (1.0 + scroll * 0.0015)).clamp(0.35, 2.5);
                let after = self.to_screen(rect, before);
                self.offset += pointer - after;
            }
        }

        // A grid, so panning reads as movement.
        let step = 40.0 * self.zoom;
        let grid = visuals.weak_text_color().gamma_multiply(0.15);
        let mut x = rect.left() + (self.offset.x.rem_euclid(step));
        while x < rect.right() {
            painter.line_segment(
                [Pos2::new(x, rect.top()), Pos2::new(x, rect.bottom())],
                Stroke::new(1.0_f32, grid),
            );
            x += step;
        }
        let mut y = rect.top() + (self.offset.y.rem_euclid(step));
        while y < rect.bottom() {
            painter.line_segment(
                [Pos2::new(rect.left(), y), Pos2::new(rect.right(), y)],
                Stroke::new(1.0_f32, grid),
            );
            y += step;
        }

        // Transitions, under the states.
        let accent = visuals.selection.bg_fill;
        let mut segments = Vec::new();
        for (index, transition) in file.transitions.iter().enumerate() {
            let (Some(from), Some(to)) = (
                from_node(file, &transition.from),
                file.states.iter().position(|s| s.name == transition.to),
            ) else {
                continue;
            };
            let a = self.node_rect(rect, file, from).center();
            let b = self.node_rect(rect, file, Node::State(to)).center();
            let direction = (b - a).normalized();
            if !direction.is_finite() {
                continue;
            }
            // Two-way pairs sit side by side.
            let back = file
                .transitions
                .iter()
                .any(|other| other.to == transition.from && other.from == transition.to);
            let side = if back {
                Vec2::new(-direction.y, direction.x) * 7.0 * self.zoom
            } else {
                Vec2::ZERO
            };
            let (a, b) = (a + side, b + side);
            let active = live.fade.is_some_and(|(_, t, _)| t == Some(index));
            let color = if selected == Selected::Transition(index) {
                accent
            } else if active {
                Color32::from_rgb(90, 210, 120)
            } else {
                visuals.text_color().gamma_multiply(0.7)
            };
            let width: f32 = if selected == Selected::Transition(index) {
                3.0
            } else {
                2.0
            };
            if transition.generated {
                painter.extend(egui::Shape::dashed_line(
                    &[a, b],
                    Stroke::new(width, color),
                    8.0,
                    5.0,
                ));
            } else {
                painter.line_segment([a, b], Stroke::new(width, color));
            }
            let middle = a + (b - a) * 0.5;
            let normal = Vec2::new(-direction.y, direction.x);
            let head = 7.0 * self.zoom.max(0.6);
            painter.add(egui::Shape::convex_polygon(
                vec![
                    middle + direction * head,
                    middle - direction * head + normal * head * 0.8,
                    middle - direction * head - normal * head * 0.8,
                ],
                color,
                Stroke::NONE,
            ));
            segments.push((index, a, b));
        }

        // The transition being made follows the mouse.
        if let Some(from) = self.linking
            && let Some(pointer) = ui.input(|input| input.pointer.hover_pos())
        {
            let a = self.node_rect(rect, file, from).center();
            painter.line_segment([a, pointer], Stroke::new(2.0_f32, accent));
            if ui.input(|input| input.key_pressed(egui::Key::Escape)) {
                self.linking = None;
            }
        }

        // States, and the Any State.
        let mut nodes: Vec<Node> = (0..file.states.len()).map(Node::State).collect();
        nodes.push(Node::Any);
        let mut hit_node = false;
        for node in nodes {
            let node_rect = self.node_rect(rect, file, node);
            let id = ui.id().with(("graph node", node_id(node)));
            let response = ui.interact(node_rect, id, egui::Sense::click_and_drag());
            if response.hovered() {
                hit_node = true;
            }
            let (fill, title, subtitle) = match node {
                Node::State(index) => {
                    let state = &file.states[index];
                    let default = file.default_state == state.name
                        || (file.state(&file.default_state).is_none() && index == 0);
                    let fill = if default {
                        Color32::from_rgb(196, 118, 40)
                    } else {
                        Color32::from_rgb(70, 82, 104)
                    };
                    let subtitle = match &state.motion {
                        Motion::Clip(path) if path.is_empty() => "(no clip)".to_owned(),
                        Motion::Clip(path) => axcenity_anim::files::clip_name(path).to_owned(),
                        Motion::BlendTree(tree) => format!(
                            "blend tree · {} · {} clips",
                            tree.parameter,
                            tree.children.len()
                        ),
                    };
                    (fill, state.name.clone(), subtitle)
                }
                Node::Any => (
                    Color32::from_rgb(40, 140, 150),
                    ANY_STATE.to_owned(),
                    String::new(),
                ),
            };
            let rounding = 6.0 * self.zoom;
            painter.rect_filled(node_rect, rounding, fill);
            let is_selected = selected == Selected::Node(node);
            let playing = matches!(node, Node::State(i) if live.state.is_some_and(|(s, _)| s == i));
            let outline = if is_selected {
                Stroke::new(2.5_f32, accent)
            } else if playing {
                Stroke::new(2.5_f32, Color32::from_rgb(90, 210, 120))
            } else if response.hovered() {
                Stroke::new(1.5_f32, Color32::WHITE.gamma_multiply(0.6))
            } else {
                Stroke::new(1.0_f32, Color32::BLACK.gamma_multiply(0.5))
            };
            painter.rect_stroke(node_rect, rounding, outline, egui::StrokeKind::Outside);
            let font = egui::FontId::proportional(13.0 * self.zoom.clamp(0.7, 1.5));
            let small = egui::FontId::proportional(10.5 * self.zoom.clamp(0.7, 1.5));
            let title_at = if subtitle.is_empty() {
                node_rect.center()
            } else {
                node_rect.center() - Vec2::new(0.0, 7.0 * self.zoom)
            };
            painter.text(
                title_at,
                egui::Align2::CENTER_CENTER,
                &title,
                font,
                Color32::WHITE,
            );
            if !subtitle.is_empty() {
                painter.text(
                    node_rect.center() + Vec2::new(0.0, 9.0 * self.zoom),
                    egui::Align2::CENTER_CENTER,
                    subtitle,
                    small,
                    Color32::WHITE.gamma_multiply(0.75),
                );
            }
            if let (Node::State(index), Some((state, progress))) = (node, live.state)
                && index == state
            {
                let bar = Rect::from_min_size(
                    node_rect.left_bottom() - Vec2::new(0.0, 4.0 * self.zoom),
                    Vec2::new(
                        node_rect.width() * progress.clamp(0.0, 1.0),
                        4.0 * self.zoom,
                    ),
                );
                painter.rect_filled(bar, 0.0, Color32::from_rgb(90, 210, 120));
            }

            // Clicks, drags and the right-click menu.
            if response.clicked() {
                match (self.linking.take(), node) {
                    (Some(from), Node::State(to)) if from != node => {
                        events.push(GraphEvent::Link(from, to));
                    }
                    (Some(_), _) => {}
                    (None, _) => events.push(GraphEvent::Select(Selected::Node(node))),
                }
            }
            if response.double_clicked()
                && let Node::State(index) = node
            {
                events.push(GraphEvent::Open(index));
            }
            if response.drag_started() {
                events.push(GraphEvent::Select(Selected::Node(node)));
            }
            if response.dragged() {
                let [x, y] = node_position(file, node);
                let delta = response.drag_delta() / self.zoom;
                events.push(GraphEvent::Move(node, [x + delta.x, y + delta.y]));
            }
            response.context_menu(|ui| {
                if ui.button("Make Transition").clicked() {
                    self.linking = Some(node);
                    ui.close();
                }
                if let Node::State(index) = node {
                    if ui.button("Set as Default State").clicked() {
                        events.push(GraphEvent::SetDefault(index));
                        ui.close();
                    }
                    if ui.button("Rename").clicked() {
                        events.push(GraphEvent::Rename(index));
                        ui.close();
                    }
                    if ui.button("Delete").clicked() {
                        events.push(GraphEvent::Delete(Selected::Node(node)));
                        ui.close();
                    }
                }
            });
        }

        // A click on the background: an arrow under it, or nothing.
        // (Clicking the background while making a transition cancels it.)
        if canvas.clicked() && !hit_node && self.linking.take().is_none() {
            let pointer = canvas.interact_pointer_pos().unwrap_or_default();
            let near = segments
                .iter()
                .map(|&(index, a, b)| (index, distance_to_segment(pointer, a, b)))
                .filter(|&(_, distance)| distance < 7.0)
                .min_by(|x, y| x.1.total_cmp(&y.1));
            events.push(GraphEvent::Select(match near {
                Some((index, _)) => Selected::Transition(index),
                None => Selected::Controller,
            }));
        }
        if canvas.secondary_clicked() {
            self.menu_at = canvas
                .interact_pointer_pos()
                .map(|pointer| self.to_graph(rect, pointer));
        }
        canvas.context_menu(|ui| {
            let at = self.menu_at.unwrap_or([0.0, 0.0]);
            if ui.button("Add Empty State").clicked() {
                events.push(GraphEvent::AddState(at, None));
                ui.close();
            }
            if ui
                .button("Add Blend Tree")
                .on_hover_text("A state that mixes related clips by a Number, such as Idle, Walk and Run by Speed")
                .clicked()
            {
                events.push(GraphEvent::AddBlendTree(at));
                ui.close();
            }
            ui.menu_button("Add State with Clip", |ui| {
                if clips.is_empty() {
                    ui.weak("No clips in the project yet.");
                }
                egui::ScrollArea::vertical()
                    .max_height(300.0)
                    .show(ui, |ui| {
                        for clip in clips {
                            let label = format!(
                                "{}  ({})",
                                axcenity_anim::files::clip_name(clip),
                                super::store::group_of(clip)
                            );
                            if ui.button(label).clicked() {
                                events.push(GraphEvent::AddState(at, Some(clip.clone())));
                                ui.close();
                            }
                        }
                    });
            });
        });

        // The Delete key removes the selection while the mouse is over the graph.
        let typing = ui.ctx().memory(|memory| memory.focused().is_some());
        if canvas.hovered() && !typing && ui.input(|input| input.key_pressed(egui::Key::Delete)) {
            events.push(GraphEvent::Delete(selected));
        }
        if file.states.is_empty() {
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "Right-click to add a state",
                egui::FontId::proportional(14.0),
                visuals.weak_text_color(),
            );
        } else if self.linking.is_some() {
            painter.text(
                rect.left_top() + Vec2::new(10.0, 10.0),
                egui::Align2::LEFT_TOP,
                "Click the state the transition goes to (Escape cancels)",
                egui::FontId::proportional(12.0),
                accent,
            );
        }
        events
    }
}

fn node_id(node: Node) -> i64 {
    match node {
        Node::State(index) => index as i64,
        Node::Any => -1,
    }
}

fn distance_to_segment(point: Pos2, a: Pos2, b: Pos2) -> f32 {
    let ab = b - a;
    let t = ((point - a).dot(ab) / ab.length_sq().max(1e-6)).clamp(0.0, 1.0);
    (a + ab * t).distance(point)
}
