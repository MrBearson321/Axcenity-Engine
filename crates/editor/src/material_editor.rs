//! The Material tab: a material file (`*.material.json`) with every value
//! and map, its chain of bases, a live preview, presets, and Apply to the
//! selected parts.
//!
//! A value the file sets is marked, and its reset button leaves it to the
//! base again (or to the default material, with no base). A value it leaves
//! out shows the base's, dimmed. Each edit is one step of the material's
//! own undo, used while the pointer is over the tab; dragging a slider or
//! typing is one step however many frames it takes.
//!
//! The Shader section edits the material's surface shader (WGSL, see
//! `docs/rendering/models.md`). Typing goes into a draft, which becomes the
//! material's code once typing pauses for [`SHADER_PAUSE`] seconds or the
//! box loses focus, so the scene isn't compiled again on every key. The
//! code is checked with `naga` here, and its error shown on its line.
//!
//! The Graph view edits the material's graph instead (`material_editor/graph.rs`),
//! which makes its shader, maps and parameters.
//!
//! Materials are written with the project's Save. Each edit is also handed
//! to the app ([`MaterialStudio::take_changed`]), which gives it to the
//! shared `Assets`, so every part using the material, or a material built
//! on it, is drawn with the edit at once, in the viewport and in Play.

mod graph;

use std::collections::HashMap;
use std::path::Path;

use axcenity_assets::{
    Assets, Channel, MATERIAL_SUFFIX, MATERIALS_FOLDER, MapFile, MaterialChain, MaterialFile,
    OutlineFile, Shading, StandardMaterial, StandardOutline, StandardWater, ToonFile, WaterFile,
    read_material_file, write_material_file,
};
use axcenity_core::{ClassName, Color3, InstanceId, PartType, Value, Vector3, World};
use axcenity_render::glam::Vec3;
use axcenity_render::{Camera, DrawList, ShaderError, SurfaceShader};
use eframe::egui;
use eframe::egui_wgpu::RenderState;

use crate::animation::preview::{Canvas, add_floor};
use crate::node_graph::GraphView;
use crate::responsive;
use crate::theme::{self, icons};
use crate::widgets::{self, Role};

/// Below this width, the preview goes above the values.
const STACK_BELOW: f32 = 620.0;
/// The preview's height when it's above the values.
const STACKED_PREVIEW_HEIGHT: f32 = 220.0;
/// The size of the preview's sphere and cube, in studs.
const PREVIEW_SIZE: f32 = 4.0;
/// How long typing in the shader must pause before the code is used.
const SHADER_PAUSE: f64 = 0.6;

/// The code a new surface shader starts with: the material's own look,
/// scrolling.
pub const STARTER_SHADER: &str = "\
// Works out each pixel's surface. `input` has the pixel's uv,
// world_position, normal, tangent and time (in seconds), and the part's
// color, opacity, metalness, roughness and emissive (colors are linear).
// standard_surface(input) is this material's own look at input.uv.
fn surface(input: SurfaceInput) -> Surface {
    var moved = input;
    // Scroll the maps a tenth of the texture each second.
    moved.uv = input.uv + vec2<f32>(input.time * 0.1, 0.0);
    return standard_surface(moved);
}
";

/// One material file, as edited.
#[derive(Debug)]
pub struct MaterialDocument {
    pub path: String,
    pub file: MaterialFile,
    /// The file as last read or written; `None` before it has a file.
    saved: Option<MaterialFile>,
    /// The file before each edit, newest last, and the ones undone.
    undo: Vec<MaterialFile>,
    redo: Vec<MaterialFile>,
}

impl MaterialDocument {
    fn unsaved(&self) -> bool {
        self.saved.as_ref() != Some(&self.file)
    }
}

/// Which of a material's views the tab shows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MaterialView {
    /// Every value and map.
    #[default]
    Values,
    /// Its graph.
    Graph,
}

/// What the preview shows the material on.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PreviewShape {
    #[default]
    Sphere,
    Cube,
    /// The first selected part, with its shape, size and color.
    Selected,
}

/// The first selected part's look, for the preview's Selected shape.
#[derive(Clone, Debug, PartialEq)]
pub struct SelectedLook {
    /// A MeshPart's model, or `None` for a Part.
    pub mesh_id: Option<String>,
    pub shape: PartType,
    pub size: Vector3,
    /// Its `Color`, which tints the material.
    pub color: Color3,
}

impl SelectedLook {
    /// The look of the Part or MeshPart `id` in `world`.
    pub fn of(world: &World, id: InstanceId) -> Option<Self> {
        let instance = world.get(id)?;
        let mesh_id = match instance.class() {
            ClassName::Part
            | ClassName::Seat
            | ClassName::VehicleSeat
            | ClassName::DamageZone
            | ClassName::ReverbZone => None,
            ClassName::MeshPart => Some(instance.property("MeshId")?.as_str()?.to_owned()),
            _ => return None,
        };
        Some(Self {
            mesh_id,
            shape: instance
                .property("Shape")
                .and_then(Value::as_part_type)
                .unwrap_or(PartType::Block),
            size: instance.property("Size").and_then(Value::as_vector3)?,
            color: instance
                .property("Color")
                .and_then(Value::as_color3)
                .unwrap_or(Color3::new(1.0, 1.0, 1.0)),
        })
    }
}

/// What the tab is shown with.
pub struct MaterialScene<'a> {
    /// The material files in the project, sorted.
    pub files: &'a [String],
    /// The project's images, for the maps' pickers.
    pub images: &'a [String],
    pub assets: &'a Assets,
    /// The first selected part's look, if one is a Part or MeshPart.
    pub selected: Option<SelectedLook>,
    pub render_state: Option<&'a RenderState>,
    /// False with no project: shown but not edited.
    pub editable: bool,
}

/// What was asked for in the tab, for the app to do.
#[derive(Clone, Debug, PartialEq)]
pub enum MaterialAction {
    Show(String),
    /// Make a material with this name.
    New(String),
    /// Use the material at this path on the selected parts.
    Apply(String),
}

/// A starting point for a material's look: its color, metalness, roughness
/// and opacity. Its maps are kept.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Preset {
    pub name: &'static str,
    pub color: [f32; 3],
    pub metalness: f32,
    pub roughness: f32,
    pub opacity: f32,
}

pub const PRESETS: [Preset; 6] = [
    Preset {
        name: "Plastic",
        color: [0.8, 0.8, 0.8],
        metalness: 0.0,
        roughness: 0.5,
        opacity: 1.0,
    },
    Preset {
        name: "Metal",
        color: [0.78, 0.78, 0.8],
        metalness: 1.0,
        roughness: 0.3,
        opacity: 1.0,
    },
    Preset {
        name: "Glass",
        color: [0.9, 0.95, 1.0],
        metalness: 0.0,
        roughness: 0.05,
        opacity: 0.3,
    },
    Preset {
        name: "Wood",
        color: [0.55, 0.36, 0.2],
        metalness: 0.0,
        roughness: 0.75,
        opacity: 1.0,
    },
    Preset {
        name: "Stone",
        color: [0.5, 0.5, 0.48],
        metalness: 0.0,
        roughness: 0.9,
        opacity: 1.0,
    },
    Preset {
        name: "Fabric",
        color: [0.6, 0.22, 0.22],
        metalness: 0.0,
        roughness: 1.0,
        opacity: 1.0,
    },
];

impl Preset {
    fn apply(&self, file: &mut MaterialFile) {
        file.color = Some(self.color);
        file.metalness = Some(self.metalness);
        file.roughness = Some(self.roughness);
        file.opacity = Some(self.opacity);
    }
}

/// One change to a material, and the value it changes: changes to the same
/// value in a row are one undo step.
type Change = (&'static str, Box<dyn FnOnce(&mut MaterialFile)>);

/// The open materials, and the one shown.
#[derive(Default)]
pub struct MaterialStudio {
    documents: Vec<MaterialDocument>,
    current: Option<String>,
    /// Materials edited since the app last took them.
    changed: Vec<String>,
    /// The value being changed: its next change joins the same step.
    open_field: Option<&'static str>,
    hovered: bool,
    pub shape: PreviewShape,
    preview: MaterialPreview,
    /// The New material box's name, while it's open.
    naming: Option<String>,
    /// The shader code being typed, before it's used.
    shader_draft: Option<ShaderDraft>,
    /// The last code checked, and what the check found.
    shader_check: Option<(String, Result<(), ShaderError>)>,
    pub view: MaterialView,
    /// What each material's graph last compiled to, by path.
    builds: HashMap<String, graph::GraphBuild>,
    /// Where each material's graph is looked at, by path.
    graph_views: HashMap<String, GraphView>,
}

/// Shader code being typed in the Material tab.
#[derive(Debug)]
struct ShaderDraft {
    /// The material it's for.
    path: String,
    text: String,
    /// The material's code when the draft started or was last used: when
    /// the code changes otherwise (undo, a reload), the draft starts again.
    from: String,
    /// When it was last typed in (egui's time, in seconds).
    typed_at: f64,
}

impl MaterialStudio {
    pub fn unsaved(&self) -> bool {
        self.documents.iter().any(MaterialDocument::unsaved)
    }

    pub fn has_pointer(&self) -> bool {
        self.hovered
    }

    pub fn current(&self) -> Option<&MaterialDocument> {
        self.document(self.current.as_deref()?)
    }

    pub fn document(&self, path: &str) -> Option<&MaterialDocument> {
        self.documents.iter().find(|doc| doc.path == path)
    }

    fn document_mut(&mut self, path: &str) -> Option<&mut MaterialDocument> {
        self.documents.iter_mut().find(|doc| doc.path == path)
    }

    /// The materials edited since the last call, as they are now.
    pub fn take_changed(&mut self) -> Vec<(String, MaterialFile)> {
        let mut paths = std::mem::take(&mut self.changed);
        paths.sort();
        paths.dedup();
        paths
            .into_iter()
            .filter_map(|path| Some((path.clone(), self.document(&path)?.file.clone())))
            .collect()
    }

    /// Opens the material file at `path` inside `folder` (or shows it, if
    /// it's open), or says why it can't be read.
    pub fn open(&mut self, folder: &Path, path: &str) -> Result<(), String> {
        if self.document(path).is_none() {
            let file = read_material_file(&folder.join(path)).map_err(|err| with_causes(&err))?;
            self.documents.push(MaterialDocument {
                path: path.to_owned(),
                file: file.clone(),
                saved: Some(file),
                undo: Vec::new(),
                redo: Vec::new(),
            });
        }
        self.current = Some(path.to_owned());
        self.open_field = None;
        Ok(())
    }

    /// Starts a material at `path`, written with the next save, and shows it.
    pub fn create(&mut self, path: &str, file: MaterialFile) {
        self.documents.push(MaterialDocument {
            path: path.to_owned(),
            file,
            saved: None,
            undo: Vec::new(),
            redo: Vec::new(),
        });
        self.current = Some(path.to_owned());
        self.open_field = None;
        self.changed.push(path.to_owned());
    }

    /// The file at `path` changed on disk: an open material with no
    /// unsaved edits reads it again; one with edits keeps them.
    pub fn file_changed(&mut self, folder: &Path, path: &str) {
        let Some(doc) = self.document(path) else {
            return;
        };
        if doc.unsaved() {
            return;
        }
        let disk = folder.join(path);
        if !disk.exists() {
            self.documents.retain(|doc| doc.path != path);
            if self.current.as_deref() == Some(path) {
                self.current = None;
            }
            self.changed.retain(|changed| changed != path);
            return;
        }
        if let Ok(file) = read_material_file(&disk)
            && let Some(doc) = self.document_mut(path)
            && doc.file != file
        {
            doc.file = file.clone();
            doc.saved = Some(file);
            self.changed.push(path.to_owned());
        }
    }

    /// Writes every material with changes into `folder`. Returns the paths
    /// written, or why one couldn't be.
    pub fn save(&mut self, folder: &Path) -> Result<Vec<String>, String> {
        // A graph's newest code is saved with it.
        self.sync_graphs(None);
        let mut written = Vec::new();
        for doc in &mut self.documents {
            if !doc.unsaved() {
                continue;
            }
            write_material_file(&folder.join(&doc.path), &doc.file).map_err(|err| {
                format!(
                    "Could not save the material `{}`: {}",
                    doc.path,
                    with_causes(&err)
                )
            })?;
            doc.saved = Some(doc.file.clone());
            written.push(doc.path.clone());
        }
        Ok(written)
    }

    /// Changes the shown material by `change`, as one undo step (a change to
    /// `field` joins the step before while that value is still being
    /// changed). False when there's nothing to change.
    pub fn edit(&mut self, field: &'static str, change: impl FnOnce(&mut MaterialFile)) -> bool {
        let Some(path) = self.current.clone() else {
            return false;
        };
        let same_step = self.open_field == Some(field);
        let Some(doc) = self.document_mut(&path) else {
            return false;
        };
        let mut file = doc.file.clone();
        change(&mut file);
        if file == doc.file {
            return false;
        }
        let before = std::mem::replace(&mut doc.file, file);
        if !same_step {
            doc.undo.push(before);
            doc.redo.clear();
        }
        self.open_field = Some(field);
        self.changed.push(path);
        true
    }

    /// The error the Shader section last showed, if any.
    #[cfg(test)]
    pub fn shader_error(&self) -> Option<&ShaderError> {
        self.shader_check.as_ref()?.1.as_ref().err()
    }

    /// Ends the value being changed, so its next change is a new step.
    #[cfg(test)]
    pub fn finish_edit(&mut self) {
        self.open_field = None;
    }

    /// Undoes the last edit of the shown material. False when there was none.
    pub fn undo(&mut self) -> bool {
        self.history(true)
    }

    pub fn redo(&mut self) -> bool {
        self.history(false)
    }

    fn history(&mut self, undo: bool) -> bool {
        self.open_field = None;
        let Some(path) = self.current.clone() else {
            return false;
        };
        let Some(doc) = self.document_mut(&path) else {
            return false;
        };
        let (from, to) = if undo {
            (&mut doc.undo, &mut doc.redo)
        } else {
            (&mut doc.redo, &mut doc.undo)
        };
        let Some(file) = from.pop() else {
            return false;
        };
        to.push(std::mem::replace(&mut doc.file, file));
        self.changed.push(path);
        true
    }

    /// The draw list the preview shows the material at `path` in, from its
    /// camera: what `scene` makes of a part using it, as the viewport and
    /// Play draw it.
    #[cfg(test)]
    pub fn preview_list(
        &mut self,
        path: &str,
        selected: Option<&SelectedLook>,
        assets: &Assets,
    ) -> DrawList {
        self.preview.set(path, self.shape, selected);
        self.preview.list(assets)
    }

    /// The tab, then what was asked for in it.
    pub fn show(&mut self, ui: &mut egui::Ui, scene: &MaterialScene) -> Vec<MaterialAction> {
        let mut actions = Vec::new();
        self.hovered = ui.rect_contains_pointer(ui.max_rect());
        // A drag ended, or typing stopped: the next change is a new step.
        let (pointer_up, focused) = (
            ui.input(|input| !input.pointer.any_down()),
            ui.memory(|memory| memory.focused().is_some()),
        );
        if pointer_up && !focused {
            self.open_field = None;
        }
        let now = ui.input(|input| input.time);
        if self.sync_graphs(Some(now)) {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_secs_f64(graph::GRAPH_PAUSE));
        }
        self.toolbar(ui, scene, &mut actions);
        let mut view = self.view;
        widgets::segmented(
            ui,
            &mut view,
            &[
                (MaterialView::Values, Some(icons::LIST), "Values"),
                (MaterialView::Graph, Some(icons::GRAPH), "Graph"),
            ],
        );
        self.view = view;
        ui.add_space(4.0);
        let Some(path) = self.current.clone() else {
            ui.weak("Open a material from the list above, or make a new one.");
            return actions;
        };
        let Some(file) = self.document(&path).map(|doc| doc.file.clone()) else {
            return actions;
        };
        let chain = file
            .base
            .as_deref()
            .filter(|base| !base.is_empty())
            .and_then(|base| scene.assets.material_chain(base));
        let mut changes: Vec<Change> = Vec::new();
        let stacked = ui.available_width() < STACK_BELOW;
        let graph_view = self.view == MaterialView::Graph;
        let share = if graph_view { 0.3 } else { 0.42 };
        responsive::split(ui, STACK_BELOW, share, |ui, pane| {
            if pane == 0 {
                self.preview_pane(ui, scene, &path, stacked);
            } else if graph_view {
                self.graph_pane(ui, scene, &path, &file, &mut changes, &mut actions);
            } else {
                egui::ScrollArea::vertical()
                    .id_salt("material values")
                    .show(ui, |ui| {
                        let (on_nodes, on_graph) = self.graph_errors(&path);
                        if !on_nodes.is_empty() || !on_graph.is_empty() {
                            ui.colored_label(
                                theme::palette(ui.ctx()).warning,
                                "Its graph has errors: see the Graph view.",
                            );
                        }
                        values(ui, scene, &path, &file, chain.as_ref(), &mut changes);
                        ui.add_space(6.0);
                        if file.graph.is_some() {
                            graph::graph_code(ui, &file);
                        } else {
                            self.shader_section(ui, &path, &file, chain.as_ref(), &mut changes);
                        }
                        ui.add_space(8.0);
                        let apply = ui
                            .add_enabled(
                                scene.editable && scene.selected.is_some(),
                                widgets::button(
                                    Role::Primary,
                                    Some(icons::PAINT_BUCKET),
                                    "Apply to selected parts",
                                    &theme::palette(ui.ctx()),
                                ),
                            )
                            .on_disabled_hover_text("Select a Part or MeshPart first.");
                        if apply.clicked() {
                            actions.push(MaterialAction::Apply(path.clone()));
                        }
                    });
            }
        });
        if scene.editable {
            for (field, change) in changes {
                self.edit(field, change);
            }
        }
        actions
    }

    /// The shader's code, with its line numbers and its first error.
    fn shader_section(
        &mut self,
        ui: &mut egui::Ui,
        path: &str,
        file: &MaterialFile,
        chain: Option<&MaterialChain>,
        changes: &mut Vec<Change>,
    ) {
        let palette = theme::palette(ui.ctx());
        let inherited = chain
            .and_then(|chain| chain.merged.shader.clone())
            .unwrap_or_default();
        let code = file.shader.clone().unwrap_or_else(|| inherited.clone());
        widgets::section_label(ui, "Shader");
        responsive::toolbar(ui, |ui| {
            value_label(ui, "Surface shader", file.shader.is_some());
            if code.is_empty() {
                if ui
                    .button("Add shader")
                    .on_hover_text("Work out the surface in WGSL, starting from an example")
                    .clicked()
                {
                    changes.push((
                        "shader",
                        Box::new(|file| file.shader = Some(STARTER_SHADER.to_owned())),
                    ));
                }
            } else if ui
                .button("Remove shader")
                .on_hover_text("Draw the standard surface")
                .clicked()
            {
                let empty = (!inherited.is_empty()).then(String::new);
                changes.push(("shader", Box::new(move |file| file.shader = empty)));
            }
            reset_button(ui, file.shader.is_some(), "shader", changes, |file| {
                file.shader = None;
            });
        });
        if code.is_empty() {
            self.shader_draft = None;
            self.shader_check = None;
            return;
        }

        let now = ui.input(|input| input.time);
        let draft = match &mut self.shader_draft {
            Some(draft) if draft.path == path && draft.from == code => draft,
            _ => self.shader_draft.insert(ShaderDraft {
                path: path.to_owned(),
                text: code.clone(),
                from: code.clone(),
                typed_at: now,
            }),
        };
        let check = match &self.shader_check {
            Some((checked, result)) if *checked == code => result.clone(),
            _ => {
                let result = SurfaceShader::new(path, &code).check();
                self.shader_check = Some((code.clone(), result.clone()));
                result
            }
        };
        let error_line = check.as_ref().err().and_then(|error| error.line);
        let lines = draft.text.split('\n').count();
        let response = ui
            .horizontal_top(|ui| {
                ui.vertical(|ui| {
                    // The box's own margin, so the numbers line up.
                    ui.add_space(2.0);
                    ui.spacing_mut().item_spacing.y = 0.0;
                    for line in 1..=lines {
                        let color = if error_line == Some(line as u32) {
                            palette.error
                        } else {
                            palette.text_muted
                        };
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(format!("{line:>3}"))
                                    .monospace()
                                    .color(color),
                            )
                            .selectable(false),
                        );
                    }
                });
                ui.add(
                    egui::TextEdit::multiline(&mut draft.text)
                        .code_editor()
                        .desired_rows(8)
                        .desired_width(ui.available_width()),
                )
            })
            .inner;
        if response.changed() {
            draft.typed_at = now;
        }
        let waiting = draft.text != code;
        if waiting && (response.lost_focus() || now - draft.typed_at >= SHADER_PAUSE) {
            let text = draft.text.clone();
            draft.from = text.clone();
            changes.push(("shader", Box::new(move |file| file.shader = Some(text))));
        } else if waiting {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_secs_f64(SHADER_PAUSE));
        }
        if file.shader.is_none() {
            ui.label(
                egui::RichText::new("From its base: typing here makes it this material's own.")
                    .color(palette.text_secondary),
            );
        }
        if let Err(error) = check {
            let place = error
                .line
                .map_or(String::new(), |line| format!("Line {line}: "));
            ui.colored_label(palette.error, format!("{place}{}", error.message));
            ui.label(
                egui::RichText::new("It draws in magenta checks until it's fixed.")
                    .color(palette.text_secondary),
            );
        }
    }

    fn toolbar(
        &mut self,
        ui: &mut egui::Ui,
        scene: &MaterialScene,
        actions: &mut Vec<MaterialAction>,
    ) {
        responsive::toolbar(ui, |ui| {
            let shown = self.current.as_deref().map_or("Choose a material", |path| {
                path.rsplit('/').next().unwrap_or(path)
            });
            let unsaved = self.current().is_some_and(MaterialDocument::unsaved);
            let label = if unsaved {
                format!("{shown} ●")
            } else {
                shown.to_owned()
            };
            egui::ComboBox::from_id_salt("material file")
                .selected_text(label)
                .width(ui.available_width().min(220.0) - 24.0)
                .show_ui(ui, |ui| {
                    let mut listed: Vec<&str> = scene.files.iter().map(String::as_str).collect();
                    for doc in &self.documents {
                        if !listed.contains(&doc.path.as_str()) {
                            listed.push(&doc.path);
                        }
                    }
                    if listed.is_empty() {
                        ui.weak("No materials yet");
                    }
                    for path in listed {
                        let chosen = self.current.as_deref() == Some(path);
                        if ui.selectable_label(chosen, path).clicked() && !chosen {
                            actions.push(MaterialAction::Show(path.to_owned()));
                        }
                    }
                });
            if scene.editable
                && ui
                    .add(widgets::button(
                        Role::Secondary,
                        Some(icons::PLUS),
                        "New material",
                        &theme::palette(ui.ctx()),
                    ))
                    .clicked()
            {
                self.naming = Some(String::new());
            }
            if let Some(name) = &mut self.naming {
                let field = ui.add(
                    egui::TextEdit::singleline(name)
                        .hint_text("Name")
                        .desired_width(140.0),
                );
                field.request_focus();
                let enter = ui.input(|input| input.key_pressed(egui::Key::Enter));
                let valid = !name.trim().is_empty();
                if (ui.add_enabled(valid, egui::Button::new("Make")).clicked() || enter) && valid {
                    actions.push(MaterialAction::New(name.trim().to_owned()));
                    self.naming = None;
                } else if ui.button("Cancel").clicked()
                    || ui.input(|input| input.key_pressed(egui::Key::Escape))
                {
                    self.naming = None;
                }
            }
        });
    }

    /// The preview's shapes and picture: under the values when `stacked`,
    /// otherwise as tall as the tab.
    fn preview_pane(
        &mut self,
        ui: &mut egui::Ui,
        scene: &MaterialScene,
        path: &str,
        stacked: bool,
    ) {
        let mut shape = self.shape;
        widgets::segmented(
            ui,
            &mut shape,
            &[
                (PreviewShape::Sphere, Some(icons::SPHERE), "Sphere"),
                (PreviewShape::Cube, Some(icons::CUBE), "Cube"),
                (PreviewShape::Selected, Some(icons::CURSOR), "Part"),
            ],
        );
        self.shape = shape;
        let height = if stacked {
            STACKED_PREVIEW_HEIGHT
        } else {
            ui.available_height().max(60.0)
        };
        self.preview.set(path, self.shape, scene.selected.as_ref());
        self.preview
            .show(ui, scene.render_state, scene.assets, height);
    }
}

/// The values of `file` (the material at `path`), each beside its base's
/// (`chain`), adding a [`Change`] for each one edited.
fn values(
    ui: &mut egui::Ui,
    scene: &MaterialScene,
    path: &str,
    file: &MaterialFile,
    chain: Option<&MaterialChain>,
    changes: &mut Vec<Change>,
) {
    let inherited_file = chain.map(|chain| chain.merged.clone()).unwrap_or_default();
    let base = inherited_file.resolve();
    let palette = theme::palette(ui.ctx());

    widgets::section_label(ui, "Base");
    responsive::form(ui, "material base", |ui| {
        ui.label("Builds on");
        let shown = file.base.as_deref().filter(|base| !base.is_empty());
        egui::ComboBox::from_id_salt("material base")
            .selected_text(shown.map_or("Nothing".to_owned(), short_name))
            .width(ui.available_width().min(220.0) - 24.0)
            .show_ui(ui, |ui| {
                if ui.selectable_label(shown.is_none(), "Nothing").clicked() {
                    changes.push(("base", Box::new(|file| file.base = None)));
                }
                for other in scene.files.iter().filter(|other| *other != path) {
                    if ui
                        .selectable_label(shown == Some(other.as_str()), other)
                        .clicked()
                    {
                        let other = other.clone();
                        changes.push(("base", Box::new(move |file| file.base = Some(other))));
                    }
                }
            });
        ui.end_row();
    });
    let mut names = vec![short_name(path)];
    match (chain, file.base.as_deref().filter(|base| !base.is_empty())) {
        (Some(chain), _) => names.extend(chain.files.iter().map(|file| short_name(file))),
        (None, Some(base)) => names.push(format!("{} (can't be read)", short_name(base))),
        (None, None) => {}
    }
    ui.label(
        egui::RichText::new(format!("{} → default", names.join(" → ")))
            .color(palette.text_secondary),
    )
    .on_hover_text("Each value this material leaves out comes from the next one along.");
    let unread = file.base.as_deref().is_some_and(|base| !base.is_empty()) && chain.is_none();
    if unread || chain.is_some_and(|chain| chain.broken) {
        ui.colored_label(
            palette.warning,
            "A base is missing or leads back around: the default material stands in for it.",
        );
    }

    ui.add_space(6.0);
    widgets::section_label(ui, "Presets");
    responsive::toolbar(ui, |ui| {
        for preset in PRESETS {
            if ui.button(preset.name).clicked() {
                changes.push(("preset", Box::new(move |file| preset.apply(file))));
            }
        }
    });

    ui.add_space(6.0);
    widgets::section_label(ui, "Surface");
    responsive::form(ui, "material surface", |ui| {
        color_row(
            ui,
            "Color",
            file.color,
            base.color,
            "color",
            changes,
            |file, value| {
                file.color = value;
            },
        );
        number_row(
            ui,
            ("Metalness", "metalness"),
            (file.metalness, base.metalness),
            0.0..=1.0,
            changes,
            |file, value| file.metalness = value,
        );
        number_row(
            ui,
            ("Roughness", "roughness"),
            (file.roughness, base.roughness),
            0.0..=1.0,
            changes,
            |file, value| file.roughness = value,
        );
        number_row(
            ui,
            ("Opacity", "opacity"),
            (file.opacity, base.opacity),
            0.0..=1.0,
            changes,
            |file, value| file.opacity = value,
        );
        number_row(
            ui,
            ("Cut out below", "alpha cutoff"),
            (file.alpha_cutoff, base.alpha_cutoff),
            0.0..=1.0,
            changes,
            |file, value| file.alpha_cutoff = value,
        );
        color_row(
            ui,
            "Glow color",
            file.emissive_color,
            base.emissive_color,
            "emissive color",
            changes,
            |file, value| file.emissive_color = value,
        );
        number_row(
            ui,
            ("Glow strength", "emissive strength"),
            (file.emissive_strength, base.emissive_strength),
            0.0..=20.0,
            changes,
            |file, value| file.emissive_strength = value,
        );
        let label = value_label(ui, "Double-sided", file.double_sided.is_some());
        label.on_hover_text("Both sides of each face are drawn and lit.");
        ui.horizontal(|ui| {
            let mut on = file.double_sided.unwrap_or(base.double_sided);
            if ui.checkbox(&mut on, "").changed() {
                changes.push((
                    "double sided",
                    Box::new(move |file| file.double_sided = Some(on)),
                ));
            }
            reset_button(
                ui,
                file.double_sided.is_some(),
                "double sided",
                changes,
                |file| {
                    file.double_sided = None;
                },
            );
        });
        ui.end_row();
        number_row(
            ui,
            ("Sway", "sway"),
            (file.sway, base.sway),
            0.0..=2.0,
            changes,
            |file, value| file.sway = value,
        );
        pair_row(
            ui,
            ("Tiling", "tiling"),
            (file.tiling, base.tiling),
            changes,
            |file, value| file.tiling = value,
        );
        pair_row(
            ui,
            ("Offset", "offset"),
            (file.offset, base.offset),
            changes,
            |file, value| file.offset = value,
        );
    });

    shading_rows(ui, scene, file, &base, changes);

    outline_rows(ui, file, &base, changes);

    water_rows(ui, file, base.water, changes);

    graph::parameter_rows(ui, scene, path, file, chain, changes);

    ui.add_space(6.0);
    widgets::section_label(ui, "Maps");
    // A graph's textures are its other maps.
    let graph = file.graph.is_some();
    if graph {
        ui.label(
            egui::RichText::new("Its other maps are the images its graph reads.")
                .color(palette.text_secondary),
        );
    }
    for slot in MapSlot::ALL {
        if !graph || matches!(slot, MapSlot::Occlusion | MapSlot::Specular) {
            map_rows(ui, scene, slot, file, &inherited_file, changes);
        }
    }
}

/// An error and its causes on one line.
fn with_causes(err: &dyn std::error::Error) -> String {
    let mut text = err.to_string();
    let mut source = err.source();
    while let Some(cause) = source {
        text.push_str(&format!(": {cause}"));
        source = cause.source();
    }
    text
}

/// The file name of `path` without its folder or `.material.json`.
fn short_name(path: &str) -> String {
    let name = path.rsplit('/').next().unwrap_or(path);
    name.strip_suffix(MATERIAL_SUFFIX)
        .unwrap_or(name)
        .to_owned()
}

/// A value's name, marked when this material sets it.
fn value_label(ui: &mut egui::Ui, name: &str, set: bool) -> egui::Response {
    let palette = theme::palette(ui.ctx());
    if set {
        ui.label(egui::RichText::new(format!("● {name}")).color(palette.accent))
            .on_hover_text("Set by this material")
    } else {
        ui.label(egui::RichText::new(name).color(palette.text_secondary))
            .on_hover_text("From its base (or the default material)")
    }
}

/// A reset button, shown when this material sets the value, that leaves it
/// to the base again.
fn reset_button(
    ui: &mut egui::Ui,
    set: bool,
    field: &'static str,
    changes: &mut Vec<Change>,
    reset: impl FnOnce(&mut MaterialFile) + 'static,
) {
    if set
        && widgets::icon_button(
            ui,
            icons::ARROW_COUNTER_CLOCKWISE,
            "Reset to the base",
            false,
        )
        .clicked()
    {
        changes.push((field, Box::new(reset)));
    }
}

fn number_row(
    ui: &mut egui::Ui,
    (name, field): (&str, &'static str),
    (own, base): (Option<f32>, f32),
    range: std::ops::RangeInclusive<f32>,
    changes: &mut Vec<Change>,
    set: impl Fn(&mut MaterialFile, Option<f32>) + Copy + 'static,
) {
    value_label(ui, name, own.is_some());
    ui.horizontal(|ui| {
        let mut value = own.unwrap_or(base);
        if ui.add(egui::Slider::new(&mut value, range)).changed() {
            changes.push((field, Box::new(move |file| set(file, Some(value)))));
        }
        reset_button(ui, own.is_some(), field, changes, move |file| {
            set(file, None)
        });
    });
    ui.end_row();
}

fn color_row(
    ui: &mut egui::Ui,
    name: &str,
    own: Option<[f32; 3]>,
    base: [f32; 3],
    field: &'static str,
    changes: &mut Vec<Change>,
    set: impl Fn(&mut MaterialFile, Option<[f32; 3]>) + Copy + 'static,
) {
    value_label(ui, name, own.is_some());
    ui.horizontal(|ui| {
        // Material colors are sRGB, like the picker's bytes.
        let mut bytes = own
            .unwrap_or(base)
            .map(|channel| (channel.clamp(0.0, 1.0) * 255.0).round() as u8);
        if ui.color_edit_button_srgb(&mut bytes).changed() {
            let color = bytes.map(|byte| f32::from(byte) / 255.0);
            changes.push((field, Box::new(move |file| set(file, Some(color)))));
        }
        reset_button(ui, own.is_some(), field, changes, move |file| {
            set(file, None)
        });
    });
    ui.end_row();
}

fn pair_row(
    ui: &mut egui::Ui,
    (name, field): (&str, &'static str),
    (own, base): (Option<[f32; 2]>, [f32; 2]),
    changes: &mut Vec<Change>,
    set: impl Fn(&mut MaterialFile, Option<[f32; 2]>) + Copy + 'static,
) {
    value_label(ui, name, own.is_some());
    ui.horizontal(|ui| {
        let mut value = own.unwrap_or(base);
        let across = ui.add(egui::DragValue::new(&mut value[0]).speed(0.01).prefix("U "));
        let down = ui.add(egui::DragValue::new(&mut value[1]).speed(0.01).prefix("V "));
        if across.changed() || down.changed() {
            changes.push((field, Box::new(move |file| set(file, Some(value)))));
        }
        reset_button(ui, own.is_some(), field, changes, move |file| {
            set(file, None)
        });
    });
    ui.end_row();
}

/// The Shading section: how the material is lit, and for Toon its bands,
/// ramp, softness, rim and highlight (`docs/rendering/stylized.md`).
/// `base` is what its bases give.
fn shading_rows(
    ui: &mut egui::Ui,
    scene: &MaterialScene,
    file: &MaterialFile,
    base: &StandardMaterial,
    changes: &mut Vec<Change>,
) {
    ui.add_space(6.0);
    widgets::section_label(ui, "Shading");
    let shading = file.shading.unwrap_or(base.shading);
    responsive::form(ui, "material shading", |ui| {
        value_label(ui, "Shading", file.shading.is_some()).on_hover_text(
            "Standard is physically based. Unlit shows its color whatever the light. Toon \
             lights it in flat bands, with a rim light and a sharp highlight.",
        );
        ui.horizontal(|ui| {
            egui::ComboBox::from_id_salt("material shading")
                .selected_text(shading_name(shading))
                .show_ui(ui, |ui| {
                    for choice in [Shading::Standard, Shading::Unlit, Shading::Toon] {
                        if ui
                            .selectable_label(shading == choice, shading_name(choice))
                            .clicked()
                        {
                            changes.push((
                                "shading",
                                Box::new(move |file| file.shading = Some(choice)),
                            ));
                        }
                    }
                });
            reset_button(ui, file.shading.is_some(), "shading", changes, |file| {
                file.shading = None
            });
        });
        ui.end_row();
        if shading != Shading::Toon {
            return;
        }
        // Each value this file leaves out shows its base's (or the default).
        let below = &base.toon;
        let own = file.toon.clone().unwrap_or_default();
        value_label(ui, "Bands", own.steps.is_some())
            .on_hover_text("How many bands of light from dark to fully lit.");
        ui.horizontal(|ui| {
            let mut steps = own.steps.unwrap_or(below.steps);
            if ui.add(egui::Slider::new(&mut steps, 1..=8)).changed() {
                changes.push((
                    "toon steps",
                    Box::new(move |file| toon_of(file).steps = Some(steps)),
                ));
            }
            reset_button(ui, own.steps.is_some(), "toon steps", changes, |file| {
                toon_of(file).steps = None
            });
        });
        ui.end_row();
        value_label(ui, "Ramp", own.ramp.is_some()).on_hover_text(
            "An image whose colors, left to right, are the light from none to full, in place \
             of the bands.",
        );
        ui.horizontal(|ui| {
            let image = own
                .ramp
                .clone()
                .or_else(|| below.ramp.clone())
                .unwrap_or_default();
            let text = if image.is_empty() {
                "None".to_owned()
            } else {
                image.rsplit('/').next().unwrap_or(&image).to_owned()
            };
            egui::ComboBox::from_id_salt("toon ramp")
                .selected_text(text)
                .width((ui.available_width() - 40.0).clamp(60.0, 180.0))
                .show_ui(ui, |ui| {
                    for candidate in
                        std::iter::once("").chain(scene.images.iter().map(String::as_str))
                    {
                        let name = if candidate.is_empty() {
                            "None"
                        } else {
                            candidate
                        };
                        if ui.selectable_label(candidate == image, name).clicked() {
                            let candidate = candidate.to_owned();
                            changes.push((
                                "toon ramp",
                                Box::new(move |file| toon_of(file).ramp = Some(candidate)),
                            ));
                        }
                    }
                });
            reset_button(ui, own.ramp.is_some(), "toon ramp", changes, |file| {
                toon_of(file).ramp = None
            });
        });
        ui.end_row();
        let mut row = |name, field, (own, below), range, set: ToonSetter| {
            number_row(
                ui,
                (name, field),
                (own, below),
                range,
                changes,
                move |file, value| set(toon_of(file), value),
            );
        };
        row(
            "Softness",
            "toon softness",
            (own.softness, below.softness),
            0.0..=1.0,
            |toon, value| toon.softness = value,
        );
        row(
            "Rim light",
            "toon rim",
            (own.rim, below.rim),
            0.0..=2.0,
            |toon, value| toon.rim = value,
        );
        row(
            "Highlight",
            "toon highlight",
            (own.highlight, below.highlight),
            0.0..=1.0,
            |toon, value| toon.highlight = value,
        );
    });
}

fn shading_name(shading: Shading) -> &'static str {
    match shading {
        Shading::Standard => "Standard",
        Shading::Unlit => "Unlit",
        Shading::Toon => "Toon",
    }
}

/// Sets one of toon shading's numbers (`None` leaves it to the base).
type ToonSetter = fn(&mut ToonFile, Option<f32>);

/// `file`'s toon values, made if it had none.
fn toon_of(file: &mut MaterialFile) -> &mut ToonFile {
    file.toon.get_or_insert_with(ToonFile::default)
}

/// The Outline section: the line drawn around its parts' silhouettes, its
/// width (0 for none) and color (`docs/rendering/stylized.md`). `base` is
/// what its bases give.
fn outline_rows(
    ui: &mut egui::Ui,
    file: &MaterialFile,
    base: &StandardMaterial,
    changes: &mut Vec<Change>,
) {
    ui.add_space(6.0);
    widgets::section_label(ui, "Outline");
    let below = base.outline.unwrap_or(StandardOutline {
        width: 0.0,
        ..StandardOutline::default()
    });
    let own = file.outline.clone().unwrap_or_default();
    // An outline block that sets no width anywhere draws the default one.
    let width = own
        .width
        .unwrap_or(if base.outline.is_none() && file.outline.is_some() {
            StandardOutline::default().width
        } else {
            below.width
        });
    // Its width is in pixels at 1080 pixels tall, the same near or far.
    responsive::form(ui, "material outline", |ui| {
        number_row(
            ui,
            ("Width", "outline width"),
            (own.width, width),
            0.0..=10.0,
            changes,
            |file, value| outline_of(file).width = value,
        );
        if width > 0.0 {
            color_row(
                ui,
                "Color",
                own.color,
                below.color,
                "outline color",
                changes,
                |file, value| outline_of(file).color = value,
            );
        }
    });
}

/// `file`'s outline values, made if it had none.
fn outline_of(file: &mut MaterialFile) -> &mut OutlineFile {
    file.outline.get_or_insert_with(OutlineFile::default)
}

/// The Water section: whether the material is water, and its waves, color
/// and foam (`docs/rendering/water.md`). `base` is what its bases give.
fn water_rows(
    ui: &mut egui::Ui,
    file: &MaterialFile,
    base: Option<StandardWater>,
    changes: &mut Vec<Change>,
) {
    ui.add_space(6.0);
    widgets::section_label(ui, "Water");
    let own = file.water.clone();
    responsive::form(ui, "material water", |ui| {
        let label = value_label(ui, "Water", own.is_some());
        label.on_hover_text(
            "Draws a part with this material as water: its box's top, with waves, what's \
             under seen through it, reflections and foam.",
        );
        ui.horizontal(|ui| {
            let mut on = own.is_some() || base.is_some();
            if ui.checkbox(&mut on, "").changed() {
                changes.push((
                    "water",
                    Box::new(move |file| file.water = on.then(WaterFile::default)),
                ));
            }
            reset_button(ui, own.is_some(), "water", changes, |file| {
                file.water = None
            });
        });
        ui.end_row();
        if own.is_none() && base.is_none() {
            return;
        }
        // Each value this file leaves out shows its base's (or the default).
        let below = base.unwrap_or_default();
        let own = own.unwrap_or_default();
        color_row(
            ui,
            "Deep color",
            own.deep_color,
            below.deep_color,
            "water deep color",
            changes,
            |file, value| water_of(file).deep_color = value,
        );
        let mut row = |name, field, (own, below), range, set: WaterSetter| {
            number_row(
                ui,
                (name, field),
                (own, below),
                range,
                changes,
                move |file, value| set(water_of(file), value),
            );
        };
        row(
            "Clarity",
            "water clarity",
            (own.clarity, below.clarity),
            0.1..=50.0,
            |water, value| water.clarity = value,
        );
        row(
            "Wave height",
            "water wave height",
            (own.wave_height, below.wave_height),
            0.0..=5.0,
            |water, value| water.wave_height = value,
        );
        row(
            "Wave length",
            "water wave length",
            (own.wave_length, below.wave_length),
            1.0..=200.0,
            |water, value| water.wave_length = value,
        );
        row(
            "Wave speed",
            "water wave speed",
            (own.wave_speed, below.wave_speed),
            0.0..=4.0,
            |water, value| water.wave_speed = value,
        );
        row(
            "Wave direction",
            "water wave direction",
            (own.wave_direction, below.wave_direction),
            0.0..=360.0,
            |water, value| water.wave_direction = value,
        );
        row(
            "Foam",
            "water foam",
            (own.foam, below.foam),
            0.0..=4.0,
            |water, value| water.foam = value,
        );
        row(
            "Refraction",
            "water refraction",
            (own.refraction, below.refraction),
            0.0..=2.0,
            |water, value| water.refraction = value,
        );
    });
}

/// Sets one of a water material's values (`None` leaves it to the base).
type WaterSetter = fn(&mut WaterFile, Option<f32>);

/// `file`'s water values, made water if it wasn't.
fn water_of(file: &mut MaterialFile) -> &mut WaterFile {
    file.water.get_or_insert_with(WaterFile::default)
}

/// One of a material's texture maps.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MapSlot {
    Color,
    Normal,
    Metalness,
    Roughness,
    Occlusion,
    Emissive,
    Opacity,
    Specular,
}

impl MapSlot {
    const ALL: [MapSlot; 8] = [
        MapSlot::Color,
        MapSlot::Normal,
        MapSlot::Metalness,
        MapSlot::Roughness,
        MapSlot::Occlusion,
        MapSlot::Emissive,
        MapSlot::Opacity,
        MapSlot::Specular,
    ];

    fn label(self) -> &'static str {
        match self {
            MapSlot::Color => "Color map",
            MapSlot::Normal => "Normal map",
            MapSlot::Metalness => "Metalness map",
            MapSlot::Roughness => "Roughness map",
            MapSlot::Occlusion => "Occlusion map",
            MapSlot::Emissive => "Glow map",
            MapSlot::Opacity => "Opacity map",
            MapSlot::Specular => "Specular map",
        }
    }

    /// The undo step's name for this map.
    fn field(self) -> &'static str {
        match self {
            MapSlot::Color => "color map",
            MapSlot::Normal => "normal map",
            MapSlot::Metalness => "metalness map",
            MapSlot::Roughness => "roughness map",
            MapSlot::Occlusion => "occlusion map",
            MapSlot::Emissive => "glow map",
            MapSlot::Opacity => "opacity map",
            MapSlot::Specular => "specular map",
        }
    }

    fn of(self, file: &MaterialFile) -> &Option<MapFile> {
        let maps = &file.maps;
        match self {
            MapSlot::Color => &maps.color,
            MapSlot::Normal => &maps.normal,
            MapSlot::Metalness => &maps.metalness,
            MapSlot::Roughness => &maps.roughness,
            MapSlot::Occlusion => &maps.occlusion,
            MapSlot::Emissive => &maps.emissive,
            MapSlot::Opacity => &maps.opacity,
            MapSlot::Specular => &maps.specular,
        }
    }

    fn of_mut(self, file: &mut MaterialFile) -> &mut Option<MapFile> {
        let maps = &mut file.maps;
        match self {
            MapSlot::Color => &mut maps.color,
            MapSlot::Normal => &mut maps.normal,
            MapSlot::Metalness => &mut maps.metalness,
            MapSlot::Roughness => &mut maps.roughness,
            MapSlot::Occlusion => &mut maps.occlusion,
            MapSlot::Emissive => &mut maps.emissive,
            MapSlot::Opacity => &mut maps.opacity,
            MapSlot::Specular => &mut maps.specular,
        }
    }

    /// The channel a map reads its value from, or `None` for a map read in
    /// color (color, normal, glow and specular maps).
    fn default_channel(self) -> Option<Channel> {
        match self {
            MapSlot::Metalness | MapSlot::Roughness | MapSlot::Occlusion => Some(Channel::Red),
            MapSlot::Opacity => Some(Channel::Alpha),
            _ => None,
        }
    }

    fn has_strength(self) -> bool {
        matches!(self, MapSlot::Normal | MapSlot::Occlusion)
    }
}

/// Sets the map in `slot` of the material to its own values with `change`
/// made, filling in a map the material didn't set (so it overrides only
/// what's changed).
fn change_map(
    slot: MapSlot,
    change: impl FnOnce(&mut MapFile) + 'static,
) -> Box<dyn FnOnce(&mut MaterialFile)> {
    Box::new(move |file| {
        let map = slot.of_mut(file).get_or_insert_with(MapFile::default);
        change(map);
    })
}

/// A map's rows: its image, and (as the map has them) its channel,
/// inversion, strength and DirectX switch.
fn map_rows(
    ui: &mut egui::Ui,
    scene: &MaterialScene,
    slot: MapSlot,
    file: &MaterialFile,
    inherited: &MaterialFile,
    changes: &mut Vec<Change>,
) {
    let own = slot.of(file).clone();
    let below = slot.of(inherited).clone().unwrap_or_default();
    let shown = own.as_ref().map_or(below.clone(), |own| {
        let mut merged = MaterialFile::default();
        *slot.of_mut(&mut merged) = Some(own.clone());
        let mut base = MaterialFile::default();
        *slot.of_mut(&mut base) = Some(below.clone());
        slot.of(&merged.over(&base)).clone().unwrap_or_default()
    });
    let field = slot.field();
    responsive::form(ui, ("material map", field), |ui| {
        value_label(ui, slot.label(), own.is_some());
        ui.horizontal(|ui| {
            let image = shown.path.clone().unwrap_or_default();
            let text = if image.is_empty() {
                "None".to_owned()
            } else {
                image.rsplit('/').next().unwrap_or(&image).to_owned()
            };
            egui::ComboBox::from_id_salt(("map image", field))
                .selected_text(text)
                .width((ui.available_width() - 40.0).clamp(60.0, 180.0))
                .show_ui(ui, |ui| {
                    if ui.selectable_label(image.is_empty(), "None").clicked() {
                        changes.push((
                            field,
                            change_map(slot, |map| map.path = Some(String::new())),
                        ));
                    }
                    for candidate in scene.images {
                        if ui
                            .selectable_label(*candidate == image, candidate)
                            .clicked()
                        {
                            let candidate = candidate.clone();
                            changes.push((
                                field,
                                change_map(slot, move |map| map.path = Some(candidate)),
                            ));
                        }
                    }
                });
            reset_button(ui, own.is_some(), field, changes, move |file| {
                *slot.of_mut(file) = None;
            });
        });
        ui.end_row();
        if shown.path.as_deref().is_none_or(str::is_empty) {
            return;
        }
        if let Some(default) = slot.default_channel() {
            ui.label("    Channel");
            ui.horizontal(|ui| {
                let before = shown.channel.unwrap_or(default);
                let mut channel = before;
                egui::ComboBox::from_id_salt(("map channel", field))
                    .selected_text(channel_name(channel))
                    .show_ui(ui, |ui| {
                        for choice in [Channel::Red, Channel::Green, Channel::Blue, Channel::Alpha]
                        {
                            ui.selectable_value(&mut channel, choice, channel_name(choice));
                        }
                    });
                if channel != before {
                    changes.push((
                        field,
                        change_map(slot, move |map| map.channel = Some(channel)),
                    ));
                }
                let mut invert = shown.invert.unwrap_or(false);
                if ui.checkbox(&mut invert, "Inverted").changed() {
                    changes.push((
                        field,
                        change_map(slot, move |map| map.invert = Some(invert)),
                    ));
                }
            });
            ui.end_row();
        }
        if slot.has_strength() {
            ui.label("    Strength");
            let mut strength = shown.strength.unwrap_or(1.0);
            if ui
                .add(egui::Slider::new(&mut strength, 0.0..=2.0))
                .changed()
            {
                changes.push((
                    field,
                    change_map(slot, move |map| map.strength = Some(strength)),
                ));
            }
            ui.end_row();
        }
        if slot == MapSlot::Normal {
            ui.label("    DirectX");
            let mut direct_x = shown.direct_x.unwrap_or(false);
            if ui.checkbox(&mut direct_x, "Green points down").changed() {
                changes.push((
                    field,
                    change_map(slot, move |map| map.direct_x = Some(direct_x)),
                ));
            }
            ui.end_row();
        }
    });
}

fn channel_name(channel: Channel) -> &'static str {
    match channel {
        Channel::Red => "Red",
        Channel::Green => "Green",
        Channel::Blue => "Blue",
        Channel::Alpha => "Alpha",
    }
}

/// The path a new material named `name` is saved at.
pub fn material_path(name: &str) -> String {
    format!("{MATERIALS_FOLDER}/{name}{MATERIAL_SUFFIX}")
}

/// The look a new material starts with: the default material's, written
/// out so each value can be seen and changed.
pub fn new_material() -> MaterialFile {
    let default = StandardMaterial::default();
    MaterialFile {
        color: Some(default.color),
        metalness: Some(default.metalness),
        roughness: Some(default.roughness),
        ..MaterialFile::default()
    }
}

/// The material on a sphere, a cube or the selected part's shape, on a
/// checkered floor, with an orbiting camera.
struct MaterialPreview {
    canvas: Canvas,
    world: World,
    /// What the world was made for: the material, the shape, and the
    /// selected part's look.
    made_for: Option<(String, PreviewShape, Option<SelectedLook>)>,
    /// Half the part's height, the point the camera looks at, and its size.
    middle: Vec3,
    extent: f32,
    yaw: f32,
    pitch: f32,
    zoom: f32,
}

impl Default for MaterialPreview {
    fn default() -> Self {
        Self {
            canvas: Canvas::default(),
            world: World::new(),
            made_for: None,
            middle: Vec3::ZERO,
            extent: PREVIEW_SIZE,
            yaw: 35.0,
            pitch: 20.0,
            zoom: 1.9,
        }
    }
}

impl MaterialPreview {
    /// Shows the material at `path` on `shape`, making the world again if
    /// any of them changed.
    fn set(&mut self, path: &str, shape: PreviewShape, selected: Option<&SelectedLook>) {
        let selected = match shape {
            PreviewShape::Selected => selected.cloned(),
            _ => None,
        };
        let wanted = (path.to_owned(), shape, selected);
        if self.made_for.as_ref() == Some(&wanted) {
            return;
        }
        let (part_shape, size, color, mesh_id) = match &wanted.2 {
            Some(look) => (
                look.shape,
                Vec3::new(look.size.x, look.size.y, look.size.z),
                look.color,
                look.mesh_id.clone(),
            ),
            None => (
                if shape == PreviewShape::Cube {
                    PartType::Block
                } else {
                    PartType::Ball
                },
                Vec3::splat(PREVIEW_SIZE),
                Color3::new(1.0, 1.0, 1.0),
                None,
            ),
        };
        let mut world = World::new();
        let extent = size.max_element().max(0.1);
        add_floor(&mut world, Vec3::ZERO, 0.0, (extent / 3.0).max(0.25));
        let class = if mesh_id.is_some() {
            ClassName::MeshPart
        } else {
            ClassName::Part
        };
        let part = world.create(class).expect("not a service");
        let mut properties = vec![
            ("Anchored", Value::Bool(true)),
            ("Size", Value::Vector3(Vector3::new(size.x, size.y, size.z))),
            (
                "Position",
                Value::Vector3(Vector3::new(0.0, size.y / 2.0, 0.0)),
            ),
            ("Color", Value::Color3(color)),
            ("MaterialVariant", Value::String(path.to_owned())),
        ];
        match mesh_id {
            Some(mesh_id) => properties.push(("MeshId", Value::String(mesh_id))),
            None => properties.push(("Shape", Value::PartType(part_shape))),
        }
        for (name, value) in properties {
            world
                .set_property(part, name, value)
                .expect("parts have these");
        }
        world
            .set_parent(part, Some(world.workspace()))
            .expect("parts go in the Workspace");
        self.world = world;
        self.middle = Vec3::new(0.0, size.y / 2.0, 0.0);
        self.extent = extent;
        self.made_for = Some(wanted);
    }

    fn camera(&self) -> Camera {
        let (yaw, pitch) = (self.yaw.to_radians(), self.pitch.to_radians());
        let direction = Vec3::new(
            yaw.sin() * pitch.cos(),
            pitch.sin(),
            yaw.cos() * pitch.cos(),
        );
        Camera {
            position: self.middle + direction * self.extent * self.zoom,
            look_at: self.middle,
            fov_y_degrees: 50.0,
        }
    }

    fn list(&self, assets: &Assets) -> DrawList {
        axcenity_scene::build_draw_list(&self.world, assets, self.camera())
    }

    /// Draws the preview, `height` points tall. Dragging turns the camera
    /// around the part, and the wheel zooms.
    fn show(
        &mut self,
        ui: &mut egui::Ui,
        render_state: Option<&RenderState>,
        assets: &Assets,
        height: f32,
    ) {
        let size = egui::vec2(ui.available_width(), height);
        let (rect, response) = ui.allocate_exact_size(size, egui::Sense::drag());
        if response.dragged() {
            let delta = response.drag_delta();
            self.yaw -= delta.x * 0.4;
            self.pitch = (self.pitch + delta.y * 0.3).clamp(-10.0, 85.0);
        }
        if response.hovered() {
            let scroll = ui.input(|input| input.smooth_scroll_delta.y);
            if scroll != 0.0 {
                self.zoom = (self.zoom * (1.0 - scroll * 0.002)).clamp(0.8, 8.0);
            }
        }
        let Some(render_state) = render_state else {
            ui.painter_at(rect).text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "The preview needs the wgpu renderer.",
                egui::FontId::proportional(13.0),
                ui.visuals().weak_text_color(),
            );
            return;
        };
        let mut list = self.list(assets);
        list.time = crate::viewport::shader_time(ui, &list);
        self.canvas.paint(ui, rect, render_state, &list);
        if response.hovered() {
            ui.painter_at(rect).text(
                rect.left_bottom() + egui::vec2(8.0, -6.0),
                egui::Align2::LEFT_BOTTOM,
                "Drag to turn · scroll to zoom",
                egui::FontId::proportional(11.0),
                egui::Color32::from_white_alpha(160),
            );
        }
    }
}
