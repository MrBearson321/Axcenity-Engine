//! The Material tab's Graph view: a material's graph on the node-graph
//! editor, compiled to its shader, maps and parameters as it changes
//! (`material_graph.rs`); and the Parameters section of its values.
//!
//! The graph is kept in the file (`MaterialFile::graph`) and edited like
//! any other value, so undo covers it. What it makes is put into the file
//! by [`MaterialStudio::sync_graphs`] without an undo step of its own (it
//! only ever follows the graph): new maps and parameters at once, new
//! shader code once the graph has been still for [`GRAPH_PAUSE`] seconds,
//! so a value being dragged doesn't compile the scene's pipelines again on
//! every frame. A graph with errors keeps the shader it last made.

use std::collections::BTreeMap;

use axcenity_assets::{MaterialChain, MaterialFile, ParamValue};
use axcenity_render::{ShaderError, SurfaceShader};
use eframe::egui;

use super::{Change, MaterialAction, MaterialScene, MaterialStudio, reset_button, value_label};
use crate::material_graph::{
    CompiledGraph, GraphError, MaterialLibrary, graph_of, graph_value, set_parameter, starter_graph,
};
use crate::node_graph::GraphContext;
use crate::responsive;
use crate::theme::{self, icons};
use crate::widgets::{self, Role};

/// How long a graph must be still before its new shader code is used.
pub const GRAPH_PAUSE: f64 = 0.6;

/// What a material's graph made when it was last compiled.
#[derive(Debug)]
pub(super) struct GraphBuild {
    /// The graph, as the file keeps it.
    graph: serde_json::Value,
    result: Result<CompiledGraph, Vec<GraphError>>,
    /// When the graph last changed (egui's time, in seconds).
    changed_at: f64,
}

impl GraphBuild {
    fn of(graph: &serde_json::Value, path: &str, now: f64) -> Self {
        let result = graph_of(graph)
            .map_err(|why| {
                vec![GraphError {
                    node: None,
                    message: format!("The graph can't be read: {why}"),
                }]
            })
            .and_then(|graph| crate::material_graph::compile(&graph))
            .and_then(|compiled| {
                // The graph's code should always check; if it doesn't, say
                // so rather than draw magenta.
                match SurfaceShader::new(path, &compiled.shader).check() {
                    Ok(()) => Ok(compiled),
                    Err(ShaderError { message, .. }) => Err(vec![GraphError {
                        node: None,
                        message: format!("The graph made code that doesn't compile: {message}"),
                    }]),
                }
            });
        Self {
            graph: graph.clone(),
            result,
            changed_at: now,
        }
    }
}

impl MaterialStudio {
    /// Puts what each open material's graph makes into its file. `now` is
    /// egui's time; `None` puts new code in at once (for saving and
    /// tests). True while new code waits for the graph to be still.
    pub fn sync_graphs(&mut self, now: Option<f64>) -> bool {
        let mut waiting = false;
        for index in 0..self.documents.len() {
            let doc = &self.documents[index];
            let Some(graph) = doc.file.graph.clone() else {
                self.builds.remove(&doc.path);
                continue;
            };
            let path = doc.path.clone();
            let build = match self.builds.get(&path) {
                Some(build) if build.graph == graph => build,
                _ => {
                    let build = GraphBuild::of(&graph, &path, now.unwrap_or(0.0));
                    self.builds
                        .entry(path.clone())
                        .insert_entry(build)
                        .into_mut()
                }
            };
            let Ok(compiled) = &build.result else {
                continue;
            };
            let mut made = doc.file.clone();
            compiled.apply_to(&mut made);
            if made == doc.file {
                continue;
            }
            let new_code = made.shader != doc.file.shader;
            if new_code && now.is_some_and(|now| now - build.changed_at < GRAPH_PAUSE) {
                waiting = true;
                continue;
            }
            self.documents[index].file = made;
            self.changed.push(path);
        }
        waiting
    }

    /// The errors of the material at `path`'s graph, by node, and the ones
    /// on the whole graph.
    pub fn graph_errors(&self, path: &str) -> (BTreeMap<u32, Vec<String>>, Vec<String>) {
        let mut on_nodes: BTreeMap<u32, Vec<String>> = BTreeMap::new();
        let mut on_graph = Vec::new();
        if let Some(GraphBuild {
            result: Err(errors),
            ..
        }) = self.builds.get(path)
        {
            for error in errors {
                match error.node {
                    Some(node) => on_nodes
                        .entry(node)
                        .or_default()
                        .push(error.message.clone()),
                    None => on_graph.push(error.message.clone()),
                }
            }
        }
        (on_nodes, on_graph)
    }

    /// The Graph view of the material at `path`.
    pub(super) fn graph_pane(
        &mut self,
        ui: &mut egui::Ui,
        scene: &MaterialScene,
        path: &str,
        file: &MaterialFile,
        changes: &mut Vec<Change>,
        actions: &mut Vec<MaterialAction>,
    ) {
        let palette = theme::palette(ui.ctx());
        let Some(value) = &file.graph else {
            ui.add_space(8.0);
            ui.label("This material has no graph.");
            ui.label(
                egui::RichText::new(
                    "A graph works out its surface from nodes (textures, noise, math and \
                     color), and makes its shader. It replaces any shader code it has.",
                )
                .color(palette.text_secondary),
            );
            ui.add_space(4.0);
            responsive::toolbar(ui, |ui| {
                let make = ui.add_enabled(
                    scene.editable,
                    widgets::button(Role::Primary, Some(icons::GRAPH), "Make a graph", &palette),
                );
                if make.clicked() {
                    let graph = graph_value(&starter_graph(file));
                    changes.push(("graph", Box::new(move |file| file.graph = Some(graph))));
                }
                if let Some(base) = file.base.as_deref().filter(|base| !base.is_empty())
                    && ui
                        .button("Open its base")
                        .on_hover_text("An instance's look, its graph's too, is its base's")
                        .clicked()
                {
                    actions.push(MaterialAction::Show(base.to_owned()));
                }
            });
            return;
        };
        let Ok(mut graph) = graph_of(value) else {
            let (_, on_graph) = self.graph_errors(path);
            for error in on_graph {
                ui.colored_label(palette.error, error);
            }
            return;
        };
        let (on_nodes, on_graph) = self.graph_errors(path);
        responsive::toolbar(ui, |ui| {
            if on_nodes.is_empty() && on_graph.is_empty() {
                ui.label(
                    egui::RichText::new("Each change shows in the preview.")
                        .color(palette.text_secondary),
                );
            } else {
                ui.colored_label(
                    palette.error,
                    "Fix the errors on the nodes: until then it keeps its last shader.",
                );
            }
            if ui
                .add_enabled(scene.editable, egui::Button::new("Remove graph"))
                .on_hover_text("Keep the shader it made as code to edit by hand")
                .clicked()
            {
                changes.push(("graph", Box::new(|file| file.graph = None)));
            }
        });
        for error in &on_graph {
            ui.colored_label(palette.error, error);
        }
        let mut rect = ui.available_rect_before_wrap();
        rect.set_height(rect.height().max(260.0));
        let view = self.graph_views.entry(path.to_owned()).or_default();
        let response = view.show(
            ui,
            rect,
            &mut graph,
            &GraphContext {
                library: &MaterialLibrary,
                objects: &[],
                images: scene.images,
                errors: &on_nodes,
                editable: scene.editable,
                key: path,
            },
        );
        if response.changed {
            let graph = graph_value(&graph);
            changes.push(("graph", Box::new(move |file| file.graph = Some(graph))));
        }
    }
}

/// The Parameters section: each value the material's shader reads by name,
/// marked when this material sets it. On a material with a graph, a change
/// goes to its parameter node, so the graph keeps it.
pub(super) fn parameter_rows(
    ui: &mut egui::Ui,
    scene: &MaterialScene,
    path: &str,
    file: &MaterialFile,
    chain: Option<&MaterialChain>,
    changes: &mut Vec<Change>,
) {
    let Some(resolved) = scene.assets.material(path) else {
        return;
    };
    if resolved.parameters.is_empty() {
        return;
    }
    let below = chain
        .map(|chain| chain.merged.parameters.clone())
        .unwrap_or_default();
    let has_graph = file.graph.is_some();
    ui.add_space(6.0);
    widgets::section_label(ui, "Parameters");
    responsive::form(ui, "material parameters", |ui| {
        for (name, _) in &resolved.parameters {
            let own = file.parameters.get(name);
            let Some(value) = own.or_else(|| below.get(name)).cloned() else {
                continue;
            };
            let set = own.is_some();
            value_label(ui, name, set).on_hover_text(if has_graph {
                "A parameter of this material's graph, which instances can change"
            } else {
                "Set by this material, over its base's"
            });
            ui.horizontal(|ui| {
                if let Some(changed) = parameter_control(ui, &value) {
                    let name = name.clone();
                    changes.push((
                        "parameter",
                        Box::new(move |file| {
                            if let Some(mut graph) =
                                file.graph.as_ref().and_then(|v| graph_of(v).ok())
                                && set_parameter(&mut graph, &name, &changed)
                            {
                                file.graph = Some(graph_value(&graph));
                            }
                            file.parameters.insert(name, changed);
                        }),
                    ));
                }
                let name = name.clone();
                reset_button(ui, set && !has_graph, "parameter", changes, move |file| {
                    file.parameters.remove(&name);
                });
            });
            ui.end_row();
        }
    });
}

/// A parameter's control: a color button for three numbers, otherwise a
/// box per number. The new value when it changed.
fn parameter_control(ui: &mut egui::Ui, value: &ParamValue) -> Option<ParamValue> {
    match value {
        ParamValue::Number(number) => {
            let mut number = *number;
            ui.add(egui::DragValue::new(&mut number).speed(0.01))
                .changed()
                .then_some(ParamValue::Number(number))
        }
        ParamValue::Vector(numbers) if numbers.len() == 3 => {
            let mut bytes = [0, 1, 2].map(|i| (numbers[i].clamp(0.0, 1.0) * 255.0).round() as u8);
            ui.color_edit_button_srgb(&mut bytes)
                .changed()
                .then(|| ParamValue::Vector(bytes.map(|byte| f32::from(byte) / 255.0).to_vec()))
        }
        ParamValue::Vector(numbers) => {
            let mut numbers = numbers.clone();
            let mut changed = false;
            for number in &mut numbers {
                changed |= ui.add(egui::DragValue::new(number).speed(0.01)).changed();
            }
            changed.then_some(ParamValue::Vector(numbers))
        }
    }
}

/// In place of the Shader section, for a material with a graph: its code
/// is the graph's, to read but not edit.
pub(super) fn graph_code(ui: &mut egui::Ui, file: &MaterialFile) {
    let palette = theme::palette(ui.ctx());
    widgets::section_label(ui, "Shader");
    ui.label(
        egui::RichText::new("Made by its graph (the Graph view), and made again as it changes.")
            .color(palette.text_secondary),
    );
    let mut code = file.shader.clone().unwrap_or_default();
    egui::CollapsingHeader::new("Show the code")
        .id_salt("material graph code")
        .show(ui, |ui| {
            ui.add(
                egui::TextEdit::multiline(&mut code)
                    .code_editor()
                    .interactive(false)
                    .desired_width(ui.available_width()),
            );
        });
}
