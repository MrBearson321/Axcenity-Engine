//! The UI Designer's Event Graph tab and its Live Preview.
//!
//! **Event Graph** edits the ScreenGui's UI Event Graph (the generic graph
//! editor in `node_graph.rs` with the UI nodes from
//! `axcenity_blocks::ui_graph`): what its buttons and text boxes do. The
//! graph is compiled as it's edited, against the objects in the
//! ScreenGui, so a node that can't run shows why on itself. **View Lua**
//! shows the LocalScript it becomes, which can't be edited: saving the
//! graph makes it again.
//!
//! **Live Preview** runs that Lua on a copy of the world (a sandbox: only
//! this graph's script runs, and nothing it does reaches the world being
//! edited), shown on the artboard. Clicking a button there fires its
//! `Activated`, so the graph can be tried without Play. It starts again
//! when the graph changes.

use std::collections::BTreeMap;

use axcenity_blocks::node_graph::NodeGraph;
use axcenity_blocks::ui_graph::{GraphError, UI_GRAPH_SCRIPT_NAME, UiNodes, compile_ui_graph};
use axcenity_script::{OutputKind, ScriptRuntime};

use super::*;
use crate::node_graph::{GraphContext, variables_panel};

/// The designer's two tabs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DesignerTab {
    #[default]
    Design,
    EventGraph,
    /// The string table: languages, keys and their text.
    Strings,
    /// Every ScreenGui as a card, and the picked one's PageLayouts and
    /// pages as a slideshow (`pages.rs`).
    Pages,
}

/// The shown ScreenGui's graph, lent by the app for a frame.
pub struct GraphPane<'a> {
    pub graph: &'a mut NodeGraph,
    /// Its file's project path, which the Lua's errors name.
    pub path: &'a str,
    /// Team Create: who else is editing it (it's read-only here).
    pub locked_by: Option<&'a str>,
}

/// The graph's Lua running on a copy of the world.
pub struct LivePreview {
    pub world: World,
    scripts: ScriptRuntime,
    gui: InstanceId,
    /// The graph it runs, to start again when that changes.
    graph: NodeGraph,
    hovered: Option<InstanceId>,
    /// What it printed, newest last, with errors marked.
    pub output: Vec<(bool, String)>,
}

impl LivePreview {
    /// Copies `world` and starts the graph's Lua there, as the ScreenGui
    /// `gui`'s LocalScript. Fails with the graph's errors.
    pub fn start(
        world: &World,
        gui: InstanceId,
        path: &str,
        graph: &NodeGraph,
        assets: &Assets,
    ) -> Result<Self, Vec<GraphError>> {
        let mut world = world.clone();
        let objects = axcenity_project::graph_objects(&world, gui);
        let compiled = compile_ui_graph(graph, &objects)?;
        // The loaded graph's own LocalScript, else a new one.
        let script = world
            .find_first_child(gui, UI_GRAPH_SCRIPT_NAME)
            .filter(|&id| {
                world
                    .get(id)
                    .is_some_and(|instance| instance.class() == ClassName::LocalScript)
            })
            .unwrap_or_else(|| axcenity_project::add_graph_script(&mut world, gui));
        let mut scripts = axcenity_sim::script_runtime();
        scripts.set_assets(assets.clone());
        scripts.start_script(&mut world, script, path, &compiled.source);
        let mut preview = Self {
            world,
            scripts,
            gui,
            graph: graph.clone(),
            hovered: None,
            output: Vec::new(),
        };
        preview.collect();
        Ok(preview)
    }

    /// Moves it on by `seconds`: waits, delays, tweens and Every Frame.
    pub fn step(&mut self, seconds: f64) {
        self.scripts.step(&mut self.world, seconds);
        self.scripts.heartbeat(&mut self.world, seconds);
        self.collect();
    }

    /// A click on `button`: fires its `Activated`.
    pub fn click(&mut self, button: InstanceId) {
        self.scripts.activate_ui_button(&mut self.world, button);
        self.collect();
    }

    /// The pointer is over `target` now: `MouseLeave` and `MouseEnter`
    /// when that changed.
    pub fn hover(&mut self, target: Option<InstanceId>) {
        if target == self.hovered {
            return;
        }
        if let Some(left) = self.hovered {
            self.scripts
                .fire_ui_event(&mut self.world, left, "MouseLeave");
        }
        if let Some(entered) = target {
            self.scripts
                .fire_ui_event(&mut self.world, entered, "MouseEnter");
        }
        self.hovered = target;
        self.collect();
    }

    fn collect(&mut self) {
        for line in self.scripts.take_output() {
            self.output
                .push((line.kind == OutputKind::Error, line.text));
        }
    }

    /// Whether it runs `graph` for `gui`.
    fn runs(&self, gui: InstanceId, graph: &NodeGraph) -> bool {
        self.gui == gui && self.graph == *graph
    }
}

impl UiDesigner {
    /// Which tab shows.
    #[cfg(any(test, feature = "test-support"))]
    pub fn tab(&self) -> DesignerTab {
        self.tab
    }

    /// Shows the Event Graph tab (as opening the graph's LocalScript does).
    pub fn show_event_graph(&mut self, gui: Option<InstanceId>) {
        self.tab = DesignerTab::EventGraph;
        if gui.is_some() {
            self.gui = gui;
            self.followed = gui;
        }
    }

    /// The ScreenGui the designer shows.
    pub fn current_gui(&self) -> Option<InstanceId> {
        self.gui
    }

    /// Whether the Event Graph had the pointer last frame, so Ctrl+Z
    /// undoes in the graph.
    pub fn graph_has_pointer(&self) -> bool {
        self.tab == DesignerTab::EventGraph && self.graph_hovered
    }

    /// The Live Preview, while it runs.
    #[cfg(any(test, feature = "test-support"))]
    pub fn live(&self) -> Option<&LivePreview> {
        self.live.as_ref()
    }

    /// Starts (or stops) the Live Preview on the next frame, as its button does.
    #[cfg(any(test, feature = "test-support"))]
    pub fn toggle_live_preview(&mut self) {
        self.live_wanted = true;
    }

    /// Picks a tab, as clicking it does.
    #[cfg(any(test, feature = "test-support"))]
    pub fn set_tab(&mut self, tab: DesignerTab) {
        self.tab = tab;
    }

    /// Design · Event Graph, and Live Preview, at the start of the toolbar.
    pub(super) fn tabs(
        &mut self,
        ui: &mut egui::Ui,
        scene: &DesignerScene,
        gui: InstanceId,
        graph: Option<&GraphPane>,
        actions: &mut Vec<DesignerAction>,
    ) {
        // Below 1200 points the icons go, so the toolbar takes fewer rows,
        // and in a column too narrow for four names Event Graph is Graph.
        let width = ui.max_rect().width();
        let narrow = width < 1200.0;
        let mut tab = self.tab;
        let icon = |icon| (!narrow).then_some(icon);
        let graph_name = if width < 400.0 {
            "Graph"
        } else {
            "Event Graph"
        };
        let options = [
            (DesignerTab::Design, icon(icons::LAYOUT), "Design"),
            (DesignerTab::EventGraph, icon(icons::GRAPH), graph_name),
            (DesignerTab::Strings, icon(icons::TRANSLATE), "Strings"),
            (DesignerTab::Pages, icon(icons::SLIDESHOW), "Pages"),
        ];
        if width < 400.0 {
            // Two halves, which wrap onto their own rows if they must.
            for half in options.chunks(2) {
                responsive::room_for(ui, 140.0);
                if widgets::segmented(ui, &mut tab, half) {
                    self.tab = tab;
                }
            }
        } else {
            responsive::room_for(ui, if narrow { 260.0 } else { 400.0 });
            if widgets::segmented(ui, &mut tab, &options) {
                self.tab = tab;
            }
        }
        responsive::room_for(ui, if narrow { 40.0 } else { 150.0 });
        let live = self.live.is_some();
        let (icon, label, tip) = if live {
            (
                icons::STOP,
                "Stop Preview",
                "Back to designing. Nothing the preview did is kept.",
            )
        } else {
            (
                icons::PLAY,
                "Live Preview",
                "Runs this ScreenGui's Event Graph on a copy of the world: click its \
                 buttons on the artboard to try it, without Play",
            )
        };
        // Narrow, it's its icon alone (its name is in the tooltip).
        let text = if narrow {
            icon.to_owned()
        } else {
            theme::with_icon(icon, label)
        };
        let button = egui::Button::new(text).selected(live);
        let clicked = ui
            .add_enabled(scene.editing && graph.is_some(), button)
            .on_hover_text(format!("{label}: {tip}"))
            .clicked();
        if clicked || std::mem::take(&mut self.live_wanted) {
            if live {
                self.live = None;
            } else if let Some(pane) = graph {
                self.start_live(scene, gui, pane, actions);
            }
        }
    }

    /// Starts the Live Preview, or shows the graph's errors when it can't.
    fn start_live(
        &mut self,
        scene: &DesignerScene,
        gui: InstanceId,
        pane: &GraphPane,
        actions: &mut Vec<DesignerAction>,
    ) {
        match LivePreview::start(scene.world, gui, pane.path, pane.graph, scene.assets) {
            Ok(live) => {
                self.live = Some(live);
                self.tab = DesignerTab::Design;
            }
            Err(errors) => {
                self.tab = DesignerTab::EventGraph;
                actions.push(DesignerAction::PreviewOutput(vec![(
                    true,
                    format!(
                        "it didn't start: the Event Graph has {} {}, shown on its nodes.",
                        errors.len(),
                        if errors.len() == 1 { "error" } else { "errors" }
                    ),
                )]));
            }
        }
    }

    /// Keeps the Live Preview running: it moves on with time, starts
    /// again on a changed graph, and stops for another ScreenGui or Play.
    pub(super) fn run_live(
        &mut self,
        ui: &egui::Ui,
        scene: &DesignerScene,
        gui: InstanceId,
        graph: Option<&GraphPane>,
        actions: &mut Vec<DesignerAction>,
    ) {
        let Some(live) = &mut self.live else {
            return;
        };
        let Some(pane) = graph.filter(|_| scene.editing && live.gui == gui) else {
            self.live = None;
            return;
        };
        if !live.runs(gui, pane.graph) {
            self.live = None;
            self.start_live(scene, gui, pane, actions);
            return;
        }
        let seconds = ui.input(|i| i.stable_dt).clamp(0.0, 0.1);
        live.step(f64::from(seconds));
        let printed = std::mem::take(&mut live.output);
        if !printed.is_empty() {
            actions.push(DesignerAction::PreviewOutput(printed));
        }
        ui.ctx().request_repaint();
    }

    /// The artboard showing the Live Preview's world: clicks fire buttons,
    /// and nothing is selected or moved.
    pub(super) fn live_artboard(
        &mut self,
        ui: &mut egui::Ui,
        rect: egui::Rect,
        scene: &DesignerScene,
        gui: InstanceId,
        render_state: Option<&RenderState>,
    ) {
        let Some(mut live) = self.live.take() else {
            return;
        };
        let live_scene = DesignerScene {
            world: &live.world,
            selected: None,
            selection: &[],
            stylesheets: scene.stylesheets,
            assets: scene.assets,
            images: &[],
            scripts: &[],
            editing: false,
        };
        self.previewing = true;
        let mut ignored = Vec::new();
        self.artboard(ui, rect, &live_scene, gui, render_state, &mut ignored);
        self.previewing = false;
        for button in std::mem::take(&mut self.preview_clicks) {
            live.click(button);
        }
        live.hover(self.preview_hover.take());
        self.live = Some(live);
    }

    /// The Event Graph tab: the graph, with its variables, its errors and
    /// its Lua beside it.
    pub(super) fn event_graph(
        &mut self,
        ui: &mut egui::Ui,
        scene: &DesignerScene,
        gui: InstanceId,
        pane: Option<GraphPane>,
        actions: &mut Vec<DesignerAction>,
    ) {
        let Some(pane) = pane else {
            ui.weak("Open a project to give this ScreenGui an Event Graph.");
            return;
        };
        if let Some(who) = pane.locked_by {
            crate::widgets::locked_banner(ui, who);
        }
        let editable = scene.editing && pane.locked_by.is_none();
        let objects = axcenity_project::graph_objects(scene.world, gui);
        let compiled = compile_ui_graph(pane.graph, &objects);
        let errors = compiled.as_ref().err().cloned().unwrap_or_default();
        let by_node = axcenity_blocks::ui_graph::errors_by_node(&errors);
        let width = ui.available_width();
        let height = ui.available_height();
        let side = if width < STACK_BELOW {
            0.0
        } else {
            (width * 0.26).clamp(200.0, 320.0)
        };
        let graph_height = if side == 0.0 {
            (height * 0.6).max(160.0)
        } else {
            height
        };
        let (rect, _) =
            ui.allocate_exact_size(egui::vec2(width - side, graph_height), egui::Sense::hover());
        let before = pane.graph.clone();
        let key = format!("ui graph {gui:?}");
        let view = self.graph_views.entry(gui).or_default();
        let response = view.show(
            ui,
            rect,
            pane.graph,
            &GraphContext {
                library: &UiNodes,
                objects: &objects,
                images: &[],
                errors: &by_node,
                editable,
                key: &key,
            },
        );
        self.graph_hovered = response.hovered;
        let mut side_panel = |ui: &mut egui::Ui, graph: &mut NodeGraph| {
            self.graph_side(ui, graph, &errors, compiled.as_ref().ok(), editable);
        };
        if side == 0.0 {
            egui::ScrollArea::vertical()
                .id_salt("ui graph side")
                .show(ui, |ui| side_panel(ui, pane.graph));
        } else {
            let side_rect = egui::Rect::from_min_max(
                egui::pos2(rect.right(), rect.top()),
                egui::pos2(rect.right() + side, rect.bottom()),
            );
            let mut column = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(side_rect.shrink2(egui::vec2(8.0, 6.0)))
                    .layout(egui::Layout::top_down(egui::Align::Min)),
            );
            egui::ScrollArea::vertical()
                .id_salt("ui graph side")
                .show(&mut column, |ui| side_panel(ui, pane.graph));
        }
        // A variable changed beside the graph is a step of its own.
        let step = response
            .step
            .or_else(|| (before != *pane.graph && !response.changed).then_some(before));
        if let Some(before) = step {
            actions.push(DesignerAction::GraphStep(before));
        }
    }

    /// Beside the graph: its variables, what's wrong with it, and its Lua.
    fn graph_side(
        &mut self,
        ui: &mut egui::Ui,
        graph: &mut NodeGraph,
        errors: &[GraphError],
        compiled: Option<&axcenity_blocks::ui_graph::CompiledGraph>,
        editable: bool,
    ) {
        let palette = theme::palette(ui.ctx());
        variables_panel(ui, graph, editable);
        ui.separator();
        if errors.is_empty() {
            ui.label(
                egui::RichText::new(theme::with_icon(icons::CHECK_CIRCLE, "Ready to run"))
                    .color(palette.success),
            );
        } else {
            let on_nodes: BTreeMap<u32, usize> = errors.iter().filter_map(|error| error.node).fold(
                BTreeMap::new(),
                |mut count, node| {
                    *count.entry(node).or_default() += 1;
                    count
                },
            );
            ui.label(
                egui::RichText::new(theme::with_icon(
                    icons::WARNING,
                    &format!(
                        "{} {} on {} {}: it won't run until they're fixed.",
                        errors.len(),
                        if errors.len() == 1 {
                            "problem"
                        } else {
                            "problems"
                        },
                        on_nodes.len(),
                        if on_nodes.len() == 1 { "node" } else { "nodes" },
                    ),
                ))
                .color(palette.error),
            );
            for error in errors.iter().filter(|error| error.node.is_none()) {
                ui.colored_label(palette.error, &error.message);
            }
        }
        ui.separator();
        ui.horizontal(|ui| {
            ui.strong("Lua");
            ui.toggle_value(&mut self.show_lua, "View").on_hover_text(
                "The LocalScript this graph becomes, under the ScreenGui. It's made \
                     again each time the graph is saved, so it can't be edited here.",
            );
        });
        if self.show_lua {
            match compiled {
                Some(compiled) => {
                    let mut text = compiled.source.as_str();
                    ui.add(
                        egui::TextEdit::multiline(&mut text)
                            .code_editor()
                            .desired_width(f32::INFINITY),
                    );
                }
                None => {
                    ui.weak("Fix the problems above to see its Lua.");
                }
            }
        }
    }
}
