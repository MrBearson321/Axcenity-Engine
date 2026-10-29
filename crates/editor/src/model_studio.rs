//! The Model Studio tab (Phase 55): opening a `.glb`, `.gltf` or `.fbx`
//! (or making a New shape), editing its meshes in Object and Edit mode, and
//! saving it as a `.glb` that MeshParts using it reload.
//!
//! - **Object mode** picks whole objects; G, R and S move them.
//! - **Edit mode** (Tab) picks the active object's vertices, edges or faces
//!   (1, 2, 3): click, Shift-click, box (drag), loop (Alt-click), linked
//!   (L), all (A), grow and shrink (Ctrl + and Ctrl −).
//! - G, R and S (`modal.rs`) or the editor's handles (Move, Rotate and
//!   Scale in the Studio's toolbar) move what's picked, with the toolbar's
//!   snapping and Global or Local axes.
//!
//! The Studio keeps its own undo history while the mouse is over it; each
//! edit is one step. Save writes the file and records it as one step of the
//! editor's history (`app/model_studio.rs`). The 3D view, picking and the
//! overlay are `view.rs`.

mod modal;
mod view;

pub use modal::{Modal, ModalKind};

use std::collections::BTreeSet;

use axcenity_mesh_edit::{ModelDoc, PrimitiveSettings, PrimitiveShape, SelectMode, Selection};
use axcenity_render::glam::{Mat4, Vec3};
use eframe::egui;
use eframe::egui_wgpu::RenderState;

use crate::gizmo::{MoveAxes, Snap, Tool};
use crate::responsive;
use crate::theme::{self, icons};
use crate::widgets::{self, Role};

/// Object mode or Edit mode.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Mode {
    #[default]
    Object,
    Edit,
}

/// What the tab asks the app to do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StudioAction {
    /// Open the model file at this project path.
    Open(String),
    /// Save the open model.
    Save,
}

/// Everything an undo step puts back.
#[derive(Clone, Debug)]
struct Snapshot {
    model: ModelDoc,
    mode: Mode,
    active: usize,
    objects: BTreeSet<usize>,
    selection: Selection,
}

/// A model open in the Studio.
pub struct StudioDoc {
    /// The project path Save writes (a `.glb`).
    pub path: String,
    pub model: ModelDoc,
    pub mode: Mode,
    /// The object Edit mode edits (an index into `model.objects`; one with
    /// a mesh).
    pub active: usize,
    /// The objects picked in Object mode.
    pub objects: BTreeSet<usize>,
    /// What's picked of the active object in Edit mode.
    pub selection: Selection,
    undo: Vec<(String, Snapshot)>,
    redo: Vec<(String, Snapshot)>,
    /// Goes up with every change to the meshes, so the view knows to draw
    /// them again.
    pub revision: u64,
    /// The revision last saved.
    saved: u64,
}

impl StudioDoc {
    /// `model`, to be saved at `path`. `saved` says whether the file
    /// already holds it.
    pub fn new(path: String, model: ModelDoc, saved: bool) -> StudioDoc {
        let active = model
            .objects
            .iter()
            .position(|object| object.mesh.is_some())
            .unwrap_or(0);
        StudioDoc {
            path,
            model,
            mode: Mode::Object,
            active,
            objects: BTreeSet::from([active]),
            selection: Selection::default(),
            undo: Vec::new(),
            redo: Vec::new(),
            revision: 1,
            saved: if saved { 1 } else { 0 },
        }
    }

    pub fn unsaved(&self) -> bool {
        self.revision != self.saved
    }

    pub fn mark_saved(&mut self) {
        self.saved = self.revision;
    }

    /// Its file's name, for the tab.
    pub fn name(&self) -> &str {
        self.path.rsplit('/').next().unwrap_or(&self.path)
    }

    fn snapshot(&self) -> Snapshot {
        Snapshot {
            model: self.model.clone(),
            mode: self.mode,
            active: self.active,
            objects: self.objects.clone(),
            selection: self.selection.clone(),
        }
    }

    fn restore(&mut self, snapshot: Snapshot) {
        self.model = snapshot.model;
        self.mode = snapshot.mode;
        self.active = snapshot.active;
        self.objects = snapshot.objects;
        self.selection = snapshot.selection;
        self.revision += 1;
    }

    /// Records `before` (taken with [`snapshot`](Self::snapshot) before an
    /// edit) as an undo step named `label`.
    fn record(&mut self, label: &str, before: Snapshot) {
        self.undo.push((label.to_owned(), before));
        self.redo.clear();
        self.revision += 1;
    }

    /// Puts the last edit back. False if there's none.
    pub fn undo(&mut self) -> bool {
        let Some((label, before)) = self.undo.pop() else {
            return false;
        };
        let now = self.snapshot();
        self.restore(before);
        self.redo.push((label, now));
        true
    }

    /// Does the last undone edit again. False if there's none.
    pub fn redo(&mut self) -> bool {
        let Some((label, after)) = self.redo.pop() else {
            return false;
        };
        let now = self.snapshot();
        self.restore(after);
        self.undo.push((label, now));
        true
    }

    pub fn undo_label(&self) -> Option<&str> {
        self.undo.last().map(|(label, _)| label.as_str())
    }

    pub fn redo_label(&self) -> Option<&str> {
        self.redo.last().map(|(label, _)| label.as_str())
    }

    /// The active object's mesh.
    pub fn mesh(&self) -> Option<&axcenity_mesh_edit::EditMesh> {
        self.model.objects.get(self.active)?.mesh.as_ref()
    }

    /// Tab: into Edit mode on the active object, or back to Object mode.
    pub fn toggle_mode(&mut self) {
        self.mode = match self.mode {
            Mode::Object if self.mesh().is_some() => Mode::Edit,
            _ => Mode::Object,
        };
    }

    /// 1, 2 or 3: what Edit mode picks, keeping what's covered.
    pub fn set_select_mode(&mut self, mode: SelectMode) {
        let Some(mesh) = self
            .model
            .objects
            .get(self.active)
            .and_then(|o| o.mesh.as_ref())
        else {
            return;
        };
        self.selection.convert(mesh, mode);
    }

    /// Makes `object` the active one and picks it (or with `extend` adds
    /// or takes it off).
    pub fn click_object(&mut self, object: Option<usize>, extend: bool) {
        match (object, extend) {
            (Some(object), true) => {
                if !self.objects.remove(&object) {
                    self.objects.insert(object);
                }
            }
            (Some(object), false) => self.objects = BTreeSet::from([object]),
            (None, false) => self.objects.clear(),
            (None, true) => {}
        }
        if let Some(object) = object
            && self.objects.contains(&object)
            && self.model.objects[object].mesh.is_some()
            && object != self.active
        {
            self.active = object;
            self.selection.clear();
        }
    }

    /// A selection command on the active mesh (picking, loops, linked,
    /// grow, shrink), which isn't an undo step.
    pub fn select(&mut self, change: impl FnOnce(&mut Selection, &axcenity_mesh_edit::EditMesh)) {
        let Some(mesh) = self
            .model
            .objects
            .get(self.active)
            .and_then(|o| o.mesh.as_ref())
        else {
            return;
        };
        change(&mut self.selection, mesh);
    }

    /// What a move moves: each object and its vertices.
    pub fn targets(&self) -> Vec<(usize, Vec<u32>)> {
        match self.mode {
            Mode::Edit => self
                .mesh()
                .map(|mesh| vec![(self.active, self.selection.vertices_of(mesh))])
                .unwrap_or_default()
                .into_iter()
                .filter(|(_, vertices)| !vertices.is_empty())
                .collect(),
            Mode::Object => self
                .objects
                .iter()
                .filter_map(|&object| {
                    let mesh = self.model.objects.get(object)?.mesh.as_ref()?;
                    Some((object, (0..mesh.vertex_count() as u32).collect()))
                })
                .collect(),
        }
    }

    /// The middle of what a move moves, and its own axes' Y (the picked
    /// faces' normal in Edit mode).
    pub fn pivot(&self) -> Option<(Vec3, Vec3)> {
        if self.mode == Mode::Edit {
            let mesh = self.mesh()?;
            return Some((self.selection.center(mesh)?, self.selection.normal(mesh)));
        }
        let (sum, count) = self
            .targets()
            .iter()
            .flat_map(|(object, vertices)| {
                let mesh = self.model.objects[*object].mesh.as_ref();
                vertices
                    .iter()
                    .filter_map(move |&v| Some(mesh?.position(v)))
            })
            .fold((Vec3::ZERO, 0), |(sum, count), p| (sum + p, count + 1));
        (count > 0).then(|| (sum / count as f32, Vec3::ZERO))
    }

    /// The positions of `targets` now, to move from or put back.
    fn positions_of(&self, targets: &[(usize, Vec<u32>)]) -> Vec<Vec<Vec3>> {
        targets
            .iter()
            .map(|(object, vertices)| {
                let mesh = self.model.objects[*object].mesh.as_ref();
                vertices
                    .iter()
                    .map(|&v| mesh.map_or(Vec3::ZERO, |mesh| mesh.position(v)))
                    .collect()
            })
            .collect()
    }

    /// Puts `targets` where `matrix` takes them from `start`.
    fn place(&mut self, targets: &[(usize, Vec<u32>)], start: &[Vec<Vec3>], matrix: Mat4) {
        for ((object, vertices), start) in targets.iter().zip(start) {
            if let Some(mesh) = self.model.objects[*object].mesh.as_mut() {
                let moved: Vec<Vec3> = start.iter().map(|&p| matrix.transform_point3(p)).collect();
                mesh.set_positions(vertices, &moved);
            }
        }
        self.revision += 1;
    }

    /// Moves what's picked by `matrix` at once, as one undo step named
    /// `label`.
    #[cfg(test)]
    pub fn transform(&mut self, label: &str, matrix: Mat4) -> bool {
        let targets = self.targets();
        if targets.is_empty() {
            return false;
        }
        let before = self.snapshot();
        let start = self.positions_of(&targets);
        self.place(&targets, &start, matrix);
        self.record(label, before);
        true
    }
}

/// A move being dragged (a handle or a key's): what it moves, from where,
/// and the state to undo to.
struct Moving {
    targets: Vec<(usize, Vec<u32>)>,
    start: Vec<Vec<Vec3>>,
    before: Snapshot,
}

impl Moving {
    fn begin(doc: &StudioDoc) -> Option<Moving> {
        let targets = doc.targets();
        if targets.is_empty() {
            return None;
        }
        Some(Moving {
            start: doc.positions_of(&targets),
            targets,
            before: doc.snapshot(),
        })
    }

    /// Shows the move as `matrix` takes it.
    fn update(&self, doc: &mut StudioDoc, matrix: Mat4) {
        doc.place(&self.targets, &self.start, matrix);
    }

    /// Keeps the move as one undo step named `label`.
    fn finish(self, doc: &mut StudioDoc, label: &str) {
        doc.record(label, self.before);
    }

    /// Puts everything back where it started.
    fn cancel(self, doc: &mut StudioDoc) {
        doc.place(&self.targets, &self.start, Mat4::IDENTITY);
    }
}

/// The Model Studio tab.
pub struct ModelStudio {
    pub doc: Option<StudioDoc>,
    /// The shape and sizes New makes.
    pub new_settings: PrimitiveSettings,
    /// Which handles the view shows (Select shows none).
    pub tool: Tool,
    /// Global or Local axes, for the handles.
    pub axes: MoveAxes,
    /// Draw each face's normal.
    pub show_normals: bool,
    /// A keyboard move under way, and what it moves.
    modal: Option<(Modal, Moving)>,
    view: view::View,
    /// The mouse was over the tab last frame, so undo and keys are its.
    hovered: bool,
    /// A model waiting to replace one with unsaved changes.
    pending: Option<StudioDoc>,
    /// Lines for the Console.
    pub notices: Vec<String>,
}

impl Default for ModelStudio {
    fn default() -> Self {
        Self {
            doc: None,
            new_settings: PrimitiveSettings::default(),
            tool: Tool::Select,
            axes: MoveAxes::Global,
            show_normals: false,
            modal: None,
            view: view::View::default(),
            hovered: false,
            pending: None,
            notices: Vec::new(),
        }
    }
}

impl ModelStudio {
    /// True while the mouse is over the tab: undo, redo and the Studio's
    /// keys are its, not the world's.
    pub fn has_pointer(&self) -> bool {
        self.hovered
    }

    /// Until the tab is drawn again, the mouse isn't over it.
    pub fn forget_pointer(&mut self) {
        self.hovered = false;
    }

    /// As if the mouse were over the tab (or not), for tests.
    #[cfg(test)]
    pub fn set_pointer(&mut self, over: bool) {
        self.hovered = over;
    }

    /// True while a move is being dragged, which undo mustn't interrupt.
    pub fn is_dragging(&self) -> bool {
        self.modal.is_some() || self.view.dragging()
    }

    /// Shows `doc`, unless the open one has unsaved changes: then it waits
    /// for an answer.
    pub fn open(&mut self, doc: StudioDoc) {
        if self.doc.as_ref().is_some_and(StudioDoc::unsaved) {
            self.pending = Some(doc);
        } else {
            self.show_doc(doc);
        }
    }

    fn show_doc(&mut self, doc: StudioDoc) {
        self.modal = None;
        self.view.frame_doc(&doc);
        self.doc = Some(doc);
    }

    /// Undoes the open model's last edit. False if there was none.
    pub fn undo(&mut self) -> bool {
        if self.is_dragging() {
            return true;
        }
        self.doc.as_mut().is_some_and(StudioDoc::undo)
    }

    pub fn redo(&mut self) -> bool {
        if self.is_dragging() {
            return true;
        }
        self.doc.as_mut().is_some_and(StudioDoc::redo)
    }

    /// The tab: its toolbar, object list and 3D view. `models` are the
    /// project's model files, for Open.
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        render_state: Option<&RenderState>,
        models: &[String],
        snap: Snap,
        editable: bool,
    ) -> Vec<StudioAction> {
        let mut actions = Vec::new();
        self.hovered = ui.rect_contains_pointer(ui.max_rect());
        self.pending_bar(ui);
        self.toolbar(ui, models, editable, &mut actions);
        ui.separator();
        let Some(doc) = &self.doc else {
            ui.add_space(12.0);
            ui.vertical_centered(|ui| {
                ui.weak("Open a model, or make a New shape, to edit it here.");
                ui.weak("Edit in Model Studio on a MeshPart or a model in Assets opens it too.");
            });
            return actions;
        };
        let has_groups = doc.model.objects.len() > 1;
        if has_groups {
            responsive::split(ui, 420.0, 0.24, |ui, pane| {
                if pane == 0 {
                    self.object_list(ui);
                } else {
                    self.viewport(ui, render_state, snap, editable);
                }
            });
        } else {
            self.viewport(ui, render_state, snap, editable);
        }
        actions
    }

    /// Asks before a model replaces one with unsaved changes.
    fn pending_bar(&mut self, ui: &mut egui::Ui) {
        let Some(pending) = &self.pending else {
            return;
        };
        let current = self.doc.as_ref().map_or("", StudioDoc::name).to_owned();
        let palette = theme::palette(ui.ctx());
        let mut answer = None;
        responsive::toolbar(ui, |ui| {
            ui.colored_label(
                palette.warning,
                format!(
                    "{current} has unsaved changes. Open {} anyway?",
                    pending.name()
                ),
            );
            if ui
                .add(widgets::button(
                    Role::Danger,
                    None,
                    "Discard and open",
                    &palette,
                ))
                .clicked()
            {
                answer = Some(true);
            }
            if ui.button("Keep editing").clicked() {
                answer = Some(false);
            }
        });
        match answer {
            Some(true) => {
                let doc = self.pending.take().expect("checked above");
                self.show_doc(doc);
            }
            Some(false) => self.pending = None,
            None => {}
        }
    }

    fn toolbar(
        &mut self,
        ui: &mut egui::Ui,
        models: &[String],
        editable: bool,
        actions: &mut Vec<StudioAction>,
    ) {
        let palette = theme::palette(ui.ctx());
        responsive::toolbar(ui, |ui| {
            ui.add_enabled_ui(editable, |ui| {
                ui.menu_button(theme::with_icon(icons::PLUS, "New"), |ui| {
                    if self.new_menu(ui) {
                        let doc = StudioDoc::new(
                            String::new(),
                            ModelDoc::primitive(&self.new_settings),
                            false,
                        );
                        self.open(doc);
                        ui.close();
                    }
                });
                ui.menu_button(theme::with_icon(icons::FOLDER_OPEN, "Open"), |ui| {
                    if models.is_empty() {
                        ui.weak("The project has no models yet.");
                    }
                    egui::ScrollArea::vertical()
                        .max_height(320.0)
                        .show(ui, |ui| {
                            for path in models {
                                if ui.button(path).clicked() {
                                    actions.push(StudioAction::Open(path.clone()));
                                    ui.close();
                                }
                            }
                        });
                });
            });
            let Some(doc) = &mut self.doc else {
                return;
            };
            let label = if doc.unsaved() { "Save*" } else { "Save" };
            if ui
                .add_enabled(
                    editable,
                    widgets::button(Role::Primary, Some(icons::FLOPPY_DISK), label, &palette),
                )
                .on_hover_text(format!(
                    "Write {} as a .glb. MeshParts using it reload.",
                    save_name(doc)
                ))
                .clicked()
            {
                actions.push(StudioAction::Save);
            }
            ui.separator();
            let mut mode = doc.mode;
            if widgets::segmented(
                ui,
                &mut mode,
                &[
                    (Mode::Object, Some(icons::CUBE), "Object"),
                    (Mode::Edit, Some(icons::POLYGON), "Edit"),
                ],
            ) && mode != doc.mode
            {
                doc.toggle_mode();
            }
            if doc.mode == Mode::Edit {
                let mut select = doc.selection.mode;
                if widgets::segmented(
                    ui,
                    &mut select,
                    &[
                        (SelectMode::Vertex, Some(icons::DOT_OUTLINE), "Vertex"),
                        (SelectMode::Edge, Some(icons::LINE_SEGMENT), "Edge"),
                        (SelectMode::Face, Some(icons::SELECTION), "Face"),
                    ],
                ) {
                    doc.set_select_mode(select);
                }
            }
            ui.separator();
            for (tool, icon, name) in [
                (Tool::Select, icons::CURSOR, "Select (no handles)"),
                (Tool::Move, icons::ARROWS_OUT_CARDINAL, "Move handles (G)"),
                (Tool::Rotate, icons::ARROWS_CLOCKWISE, "Rotate handles (R)"),
                (Tool::Scale, icons::ARROWS_OUT, "Scale handles (S)"),
            ] {
                if widgets::icon_button(ui, icon, name, self.tool == tool).clicked() {
                    self.tool = tool;
                }
            }
            let mut local = self.axes == MoveAxes::Local;
            if widgets::icon_button(
                ui,
                icons::GLOBE,
                "Local axes: the selection's own (the picked faces' normal is Y)",
                local,
            )
            .clicked()
            {
                local = !local;
                self.axes = if local {
                    MoveAxes::Local
                } else {
                    MoveAxes::Global
                };
            }
            if widgets::icon_button(ui, icons::ARROW_UP_RIGHT, "Show normals", self.show_normals)
                .clicked()
            {
                self.show_normals = !self.show_normals;
            }
            ui.separator();
            let undo = doc
                .undo_label()
                .map(|label| format!("Undo {label} (Ctrl+Z)"));
            if ui
                .add_enabled(undo.is_some(), egui::Button::new(icons::ARROW_U_UP_LEFT))
                .on_hover_text(undo.unwrap_or_else(|| "Nothing to undo".to_owned()))
                .clicked()
            {
                doc.undo();
            }
            let redo = doc
                .redo_label()
                .map(|label| format!("Redo {label} (Ctrl+Y)"));
            if ui
                .add_enabled(redo.is_some(), egui::Button::new(icons::ARROW_U_UP_RIGHT))
                .on_hover_text(redo.unwrap_or_else(|| "Nothing to redo".to_owned()))
                .clicked()
            {
                doc.redo();
            }
        });
    }

    /// New's menu: the shape and its sizes. True when Create is clicked.
    fn new_menu(&mut self, ui: &mut egui::Ui) -> bool {
        let settings = &mut self.new_settings;
        let mut shape = settings.shape;
        egui::ComboBox::from_id_salt("studio new shape")
            .selected_text(shape.label())
            .show_ui(ui, |ui| {
                for option in PrimitiveShape::ALL {
                    ui.selectable_value(&mut shape, option, option.label());
                }
            });
        if shape != settings.shape {
            *settings = PrimitiveSettings::of(shape);
        }
        let (height, thickness, segments, rings, subdivisions) = shape.uses();
        let width_name = match shape {
            PrimitiveShape::Cube | PrimitiveShape::Plane => "Size",
            _ => "Width",
        };
        egui::Grid::new("studio new sizes")
            .num_columns(2)
            .show(ui, |ui| {
                let row = |ui: &mut egui::Ui, name: &str, add: &mut dyn FnMut(&mut egui::Ui)| {
                    ui.label(name);
                    add(ui);
                    ui.end_row();
                };
                row(ui, width_name, &mut |ui| {
                    ui.add(
                        egui::DragValue::new(&mut settings.size)
                            .speed(0.05)
                            .range(0.01..=1000.0)
                            .suffix(" studs"),
                    );
                });
                if height {
                    row(ui, "Height", &mut |ui| {
                        ui.add(
                            egui::DragValue::new(&mut settings.height)
                                .speed(0.05)
                                .range(0.01..=1000.0)
                                .suffix(" studs"),
                        );
                    });
                }
                if thickness {
                    row(ui, "Thickness", &mut |ui| {
                        ui.add(
                            egui::DragValue::new(&mut settings.thickness)
                                .speed(0.02)
                                .range(0.01..=1000.0)
                                .suffix(" studs"),
                        );
                    });
                }
                if segments {
                    let name = match shape {
                        PrimitiveShape::Cube | PrimitiveShape::Plane => "Cuts",
                        _ => "Segments",
                    };
                    row(ui, name, &mut |ui| {
                        ui.add(egui::DragValue::new(&mut settings.segments).range(1..=256));
                    });
                }
                if rings {
                    let name = match shape {
                        PrimitiveShape::Torus => "Ring steps",
                        PrimitiveShape::UvSphere => "Rings",
                        _ => "Height cuts",
                    };
                    row(ui, name, &mut |ui| {
                        ui.add(egui::DragValue::new(&mut settings.rings).range(1..=256));
                    });
                }
                if subdivisions {
                    row(ui, "Subdivisions", &mut |ui| {
                        ui.add(egui::DragValue::new(&mut settings.subdivisions).range(0..=5));
                    });
                }
            });
        let palette = theme::palette(ui.ctx());
        ui.add(widgets::button(
            Role::Primary,
            Some(icons::PLUS),
            &format!("Create {}", shape.label()),
            &palette,
        ))
        .clicked()
    }

    /// The file's objects, indented inside their groups. Clicking one picks
    /// it and makes it the one Edit mode edits.
    fn object_list(&mut self, ui: &mut egui::Ui) {
        let Some(doc) = &mut self.doc else {
            return;
        };
        widgets::section_label(ui, "Objects");
        let mut clicked = None;
        egui::ScrollArea::vertical()
            .id_salt("studio objects")
            .show(ui, |ui| {
                for (index, object) in doc.model.objects.iter().enumerate() {
                    ui.horizontal(|ui| {
                        ui.add_space(object.depth as f32 * 12.0);
                        let icon = if object.mesh.is_some() {
                            icons::CUBE
                        } else {
                            icons::FOLDER_OPEN
                        };
                        let mut text = theme::with_icon(icon, &object.name);
                        if index == doc.active && object.mesh.is_some() {
                            text.push_str("  ·  active");
                        }
                        let response = ui.selectable_label(doc.objects.contains(&index), text);
                        if response.clicked() && object.mesh.is_some() {
                            clicked = Some((index, ui.input(|i| i.modifiers.shift)));
                        }
                    });
                }
            });
        if let Some((index, extend)) = clicked {
            doc.click_object(Some(index), extend);
        }
    }

    /// The 3D view and what the mouse and keys do in it.
    fn viewport(
        &mut self,
        ui: &mut egui::Ui,
        render_state: Option<&RenderState>,
        snap: Snap,
        editable: bool,
    ) {
        let size = ui.available_size().max(egui::vec2(64.0, 64.0));
        let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click_and_drag());
        let context = view::Frame {
            rect,
            response: &response,
            snap,
            tool: if editable { self.tool } else { Tool::Select },
            axes: self.axes,
            show_normals: self.show_normals,
        };
        if editable && response.contains_pointer() {
            self.keys(ui, &context);
        }
        let Some(doc) = &mut self.doc else {
            return;
        };
        let modal = self.modal.as_mut().map(|(modal, _)| &*modal);
        let event = self.view.show(ui, &context, render_state, doc, modal);
        self.handle(event, ui, &context);
    }

    /// The Studio's keys, while the mouse is over its view.
    fn keys(&mut self, ui: &mut egui::Ui, frame: &view::Frame) {
        let pointer = ui
            .input(|input| input.pointer.latest_pos())
            .unwrap_or(frame.rect.center());
        if let Some((modal, _)) = &mut self.modal {
            let (axes, typed, back, enter, escape) = ui.input_mut(|input| {
                let mut axes = Vec::new();
                for (axis, key) in [egui::Key::X, egui::Key::Y, egui::Key::Z]
                    .into_iter()
                    .enumerate()
                {
                    if input.consume_key(egui::Modifiers::NONE, key) {
                        axes.push(axis);
                    }
                }
                let typed: Vec<char> = input
                    .events
                    .iter()
                    .filter_map(|event| match event {
                        egui::Event::Text(text) => Some(text.chars().collect::<Vec<_>>()),
                        _ => None,
                    })
                    .flatten()
                    .collect();
                (
                    axes,
                    typed,
                    input.consume_key(egui::Modifiers::NONE, egui::Key::Backspace),
                    input.consume_key(egui::Modifiers::NONE, egui::Key::Enter),
                    input.consume_key(egui::Modifiers::NONE, egui::Key::Escape),
                )
            });
            for axis in axes {
                modal.lock = modal.lock.press(axis);
            }
            for c in typed {
                modal.type_char(c);
            }
            if back {
                modal.typed.pop();
            }
            if enter {
                self.finish_modal();
            } else if escape {
                self.cancel_modal();
            }
            return;
        }
        let none = egui::Modifiers::NONE;
        let command = egui::Modifiers::COMMAND;
        let pressed = ui.input_mut(|input| StudioKeys {
            tab: input.consume_key(none, egui::Key::Tab),
            modes: [egui::Key::Num1, egui::Key::Num2, egui::Key::Num3]
                .map(|key| input.consume_key(none, key)),
            grab: input.consume_key(none, egui::Key::G),
            rotate: input.consume_key(none, egui::Key::R),
            scale: input.consume_key(none, egui::Key::S),
            linked: input.consume_key(none, egui::Key::L),
            all: input.consume_key(none, egui::Key::A),
            grow: input.consume_key(command, egui::Key::Plus)
                || input.consume_key(command, egui::Key::Equals),
            shrink: input.consume_key(command, egui::Key::Minus),
        });
        let Some(doc) = &mut self.doc else {
            return;
        };
        if pressed.tab {
            doc.toggle_mode();
        }
        if doc.mode == Mode::Edit {
            for (pressed, mode) in pressed.modes.into_iter().zip([
                SelectMode::Vertex,
                SelectMode::Edge,
                SelectMode::Face,
            ]) {
                if pressed {
                    doc.set_select_mode(mode);
                }
            }
            if pressed.linked {
                doc.select(|selection, mesh| selection.linked(mesh));
            }
            if pressed.all {
                doc.select(|selection, mesh| selection.all(mesh));
            }
            if pressed.grow {
                doc.select(|selection, mesh| selection.grow(mesh));
            }
            if pressed.shrink {
                doc.select(|selection, mesh| selection.shrink(mesh));
            }
        } else if pressed.all {
            doc.objects = (0..doc.model.objects.len())
                .filter(|&k| doc.model.objects[k].mesh.is_some())
                .collect();
        }
        let kind = [
            (pressed.grab, ModalKind::Grab),
            (pressed.rotate, ModalKind::Rotate),
            (pressed.scale, ModalKind::Scale),
        ]
        .into_iter()
        .find_map(|(pressed, kind)| pressed.then_some(kind));
        if let Some(kind) = kind {
            self.begin_modal(kind, pointer);
        }
    }

    /// Starts a keyboard move of what's picked, from the mouse at `at`.
    pub fn begin_modal(&mut self, kind: ModalKind, at: egui::Pos2) -> bool {
        let Some(doc) = &self.doc else {
            return false;
        };
        let (Some((center, normal)), Some(moving)) = (doc.pivot(), Moving::begin(doc)) else {
            return false;
        };
        self.modal = Some((
            Modal::new(kind, at, center, modal::frame_of(normal)),
            moving,
        ));
        true
    }

    /// Shows the keyboard move with the mouse at `mouse`.
    pub fn update_modal(&mut self, screen: &crate::screen::Screen, mouse: egui::Pos2, snap: &Snap) {
        let (Some((modal, moving)), Some(doc)) = (&self.modal, &mut self.doc) else {
            return;
        };
        moving.update(doc, modal.matrix(screen, mouse, snap));
    }

    /// Keeps the keyboard move as one undo step.
    pub fn finish_modal(&mut self) {
        if let (Some((modal, moving)), Some(doc)) = (self.modal.take(), &mut self.doc) {
            moving.finish(doc, modal.kind.label());
        }
    }

    /// Puts back what the keyboard move moved.
    pub fn cancel_modal(&mut self) {
        if let (Some((_, moving)), Some(doc)) = (self.modal.take(), &mut self.doc) {
            moving.cancel(doc);
        }
    }

    /// Acts on what happened in the view.
    fn handle(&mut self, event: view::Event, ui: &egui::Ui, frame: &view::Frame) {
        let screen = event.screen;
        if self.modal.is_some() {
            if let Some(mouse) = ui.input(|input| input.pointer.latest_pos()) {
                let snap = if ui.input(|input| input.modifiers.command) {
                    frame.snap.flipped()
                } else {
                    frame.snap
                };
                self.update_modal(&screen, mouse, &snap);
            }
            if event.clicked {
                self.finish_modal();
            } else if event.secondary_clicked {
                self.cancel_modal();
            }
            return;
        }
        let Some(doc) = &mut self.doc else {
            return;
        };
        match event.action {
            view::ViewAction::None => {}
            view::ViewAction::PickObject { object, extend } => doc.click_object(object, extend),
            view::ViewAction::Pick { element, extend } => {
                doc.select(|selection, _| selection.click(element, extend));
            }
            view::ViewAction::Loop { edge, extend } => {
                doc.select(|selection, mesh| selection.select_loop(mesh, edge, extend));
            }
            view::ViewAction::Box { elements, extend } => doc.select(|selection, _| {
                let mode = selection.mode;
                let set = selection.set_mut(mode);
                if !extend {
                    set.clear();
                }
                set.extend(elements);
            }),
            view::ViewAction::BeginHandle => {
                self.view.moving = Moving::begin(doc);
            }
            view::ViewAction::Handle(matrix) => {
                if let Some(moving) = &self.view.moving {
                    moving.update(doc, matrix);
                }
            }
            view::ViewAction::EndHandle(label) => {
                if let Some(moving) = self.view.moving.take() {
                    moving.finish(doc, label);
                }
            }
        }
    }
}

/// The Studio keys pressed this frame.
struct StudioKeys {
    tab: bool,
    modes: [bool; 3],
    grab: bool,
    rotate: bool,
    scale: bool,
    linked: bool,
    all: bool,
    grow: bool,
    shrink: bool,
}

/// Where Save writes `doc`, as the tooltip names it.
fn save_name(doc: &StudioDoc) -> String {
    if doc.path.is_empty() {
        let name = doc
            .model
            .objects
            .first()
            .map_or("Model", |o| o.name.as_str());
        format!("assets/models/{name}.glb")
    } else {
        doc.path.clone()
    }
}

#[cfg(test)]
mod tests;
