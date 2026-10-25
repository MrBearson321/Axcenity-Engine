//! The property clip's preview: a copy of the chosen Part, Model or
//! ScreenGui in a world of its own, posed at the playhead, so keying never
//! touches the world being edited.
//!
//! The copy keeps every `InstanceId` (it's taken with `World::clone`), so
//! picking something in the Explorer picks the same thing here. Click a
//! part (or a UI element) to select it; the Move, Rotate and Resize handles
//! (or a UI element's handles) then move it, and [`PreviewEvent::Edit`]
//! says what it became. A Model's handles move its pivot.
//!
//! A skinned MeshPart also shows its bones as joints (see `posing.rs`):
//! click one to select it, and its gizmo turns it (or moves the root),
//! as a `Bone` edit.

use std::sync::Arc;

use axcenity_anim::{
    Applied, EvaluatedProperty, PropertyName, apply_evaluated, bone_numbers, bone_paths,
    numbers_of, target_path,
};
use axcenity_assets::{Assets, BoneTransform, Model};
use axcenity_core::{ClassName, InstanceId, Value, World};
use axcenity_render::Camera;
use axcenity_render::glam::Vec3;
use axcenity_ui::{Stylesheet, UiLayout, UiRect};
use eframe::egui;
use eframe::egui_wgpu::RenderState;

use super::posing::{BoneEvent, Poser, Skeleton};
use super::preview::{Canvas, add_floor};
use crate::gizmo::{self, Drag, Handle, Modes, Pose, Snap, Tool};
use crate::screen::Screen;
use crate::theme::{self, icons};
use crate::ui_handles::{Grip, Handles, UiDrag};

/// What happened in the preview this frame.
#[derive(Clone, Debug, PartialEq)]
pub enum PreviewEvent {
    /// Something was clicked (or nothing, to clear the selection).
    Select(Option<InstanceId>),
    /// A handle was dragged: the new values, by target path from the root.
    Edit(Vec<EvaluatedProperty>),
}

/// The selected object, as the dope sheet and Key selected need it.
#[derive(Clone, Debug, PartialEq)]
pub struct Focus {
    /// Its path from the root (`.` is the root).
    pub target: String,
    pub name: String,
    /// Every property of it that can be keyed, with its value now.
    pub values: Vec<EvaluatedProperty>,
    /// What Key selected keys when it has no tracks yet: where it is.
    pub transform: Vec<PropertyName>,
}

enum Dragging {
    Handle(Drag),
    Ui(Box<UiDrag>),
}

/// A copy of one object, posed, with its own camera and handles.
pub struct ObjectPreview {
    canvas: Canvas,
    /// What it shows, by its id in the world being edited.
    root: Option<InstanceId>,
    /// The Explorer selection last seen, so a change can be followed once.
    followed: Option<InstanceId>,
    /// The copy as taken, and posed at the playhead this frame.
    pristine: World,
    posed: World,
    applied: Applied,
    /// The root is a UI element: show its ScreenGui flat, not in 3D.
    flat: bool,
    pub selected: Option<InstanceId>,
    pub tool: Tool,
    drag: Option<Dragging>,
    yaw: f32,
    pitch: f32,
    zoom: f32,
    middle: Vec3,
    height: f32,
    /// The root's bones, when it's a skinned MeshPart: the selected bone
    /// and its gizmo.
    pub bones: Poser,
    /// The root's model and its bones' pose at the playhead, when it's a
    /// skinned MeshPart.
    skeleton: Option<(Arc<Model>, Vec<BoneTransform>)>,
}

impl Default for ObjectPreview {
    fn default() -> Self {
        Self {
            canvas: Canvas::default(),
            root: None,
            followed: None,
            pristine: World::new(),
            posed: World::new(),
            applied: Applied::default(),
            flat: false,
            selected: None,
            tool: Tool::Move,
            drag: None,
            yaw: 35.0,
            pitch: 20.0,
            zoom: 2.0,
            middle: Vec3::ZERO,
            height: 1.0,
            bones: Poser::default(),
            skeleton: None,
        }
    }
}

/// UI elements (and ScreenGuis), which preview flat.
fn is_ui(class: ClassName) -> bool {
    matches!(
        class,
        ClassName::ScreenGui
            | ClassName::Frame
            | ClassName::TextLabel
            | ClassName::TextButton
            | ClassName::TextBox
            | ClassName::ImageLabel
            | ClassName::PageLayout
            | ClassName::Table
    )
}

impl ObjectPreview {
    pub fn root(&self) -> Option<InstanceId> {
        self.root
    }

    pub fn root_name(&self) -> Option<&str> {
        Some(self.pristine.get(self.root?)?.name())
    }

    /// The copy as taken, for the drawer's tree.
    pub fn world(&self) -> &World {
        &self.pristine
    }

    /// Follows the Explorer: picking something new shows it, and picking
    /// something inside what's shown selects it here.
    pub fn follow(&mut self, world: &World, selection: Option<InstanceId>) {
        if self.root.is_some_and(|root| !world.contains(root)) {
            self.forget();
        }
        if selection == self.followed {
            return;
        }
        self.followed = selection;
        let Some(id) = selection else {
            return;
        };
        if let Some(root) = self.root
            && (id == root || world.is_descendant_of(id, root))
        {
            self.selected = Some(id);
            return;
        }
        self.show(world, id);
    }

    /// Shows a copy of `root` (taking one if it isn't shown), and follows
    /// the Explorer inside it: picking something inside selects it here.
    /// Picking something outside changes nothing.
    pub fn follow_within(
        &mut self,
        world: &World,
        root: InstanceId,
        selection: Option<InstanceId>,
    ) {
        if self.root != Some(root) || !world.contains(root) {
            self.show_root(world, root);
        }
        if selection == self.followed {
            return;
        }
        self.followed = selection;
        if let Some(id) = selection
            && (id == root || world.is_descendant_of(id, root))
        {
            self.selected = Some(id);
        }
    }

    /// Shows a fresh copy of `root`.
    pub fn show_root(&mut self, world: &World, root: InstanceId) {
        self.show(world, root);
    }

    /// Shows nothing until something is picked again.
    pub fn forget(&mut self) {
        self.root = None;
        self.selected = None;
        self.drag = None;
        self.bones.selected = None;
        self.skeleton = None;
        self.pristine = World::new();
        self.posed = World::new();
    }

    /// Takes a new copy of the root from `world` (after it was edited).
    pub fn refresh(&mut self, world: &World) {
        if let Some(root) = self.root {
            let selected = self.selected;
            self.show(world, root);
            self.selected = selected.filter(|&id| self.pristine.contains(id));
        }
    }

    /// Copies `root` from `world` into a world of its own: alone in the
    /// Workspace on a floor, or for a UI element, its whole ScreenGui.
    fn show(&mut self, world: &World, root: InstanceId) {
        let Some(class) = world.get(root).map(|i| i.class()) else {
            return;
        };
        let mut copy = world.clone();
        let workspace = copy.workspace();
        let flat = is_ui(class);
        let mut keep = root;
        if flat {
            let mut at = Some(root);
            while let Some(id) = at {
                if copy
                    .get(id)
                    .is_some_and(|i| i.class() == ClassName::ScreenGui)
                {
                    keep = id;
                    break;
                }
                at = copy.get(id).and_then(|i| i.parent());
            }
        }
        if copy.set_parent(keep, Some(workspace)).is_err() {
            return;
        }
        let others: Vec<InstanceId> = copy
            .get(workspace)
            .map(|w| w.children().to_vec())
            .unwrap_or_default();
        for child in others.into_iter().filter(|&child| child != keep) {
            let _ = copy.destroy(child);
        }
        if !flat {
            let (min, max) = bounds(&copy, root).unwrap_or((Vec3::splat(-1.0), Vec3::splat(1.0)));
            let size = (max - min).max(Vec3::splat(0.1));
            self.height = size.max_element();
            self.middle = (min + max) / 2.0;
            let tile = (self.height / 3.0).max(0.25);
            add_floor(
                &mut copy,
                Vec3::new(self.middle.x, 0.0, self.middle.z),
                min.y,
                tile,
            );
        }
        self.flat = flat;
        if self.root != Some(root) {
            self.bones.selected = None;
        }
        self.root = Some(root);
        self.selected = Some(root);
        self.drag = None;
        self.posed = copy.clone();
        self.pristine = copy;
        self.applied = Applied::default();
    }

    /// Poses the copy: `evaluated` applied to the copy as it was taken.
    pub fn pose(&mut self, evaluated: &[EvaluatedProperty]) {
        let Some(root) = self.root else {
            return;
        };
        self.posed = self.pristine.clone();
        self.applied = Applied::default();
        apply_evaluated(&mut self.posed, root, evaluated, &mut self.applied);
    }

    /// The root's model, if it's a MeshPart whose model has a skeleton: the
    /// one its clip's bone tracks pose.
    pub fn rigged(&self, assets: &Assets) -> Option<Arc<Model>> {
        let root = self.pristine.get(self.root?)?;
        if root.class() != ClassName::MeshPart || self.flat {
            return None;
        }
        let model = assets.model(root.property("MeshId").and_then(Value::as_str)?)?;
        model.rig.is_some().then_some(model)
    }

    /// Poses the root's bones (`None` when it has none).
    pub fn pose_bones(&mut self, skeleton: Option<(Arc<Model>, Vec<BoneTransform>)>) {
        if skeleton.is_none() {
            self.bones.selected = None;
        }
        self.skeleton = skeleton;
    }

    /// The root's bones as posed, for tests.
    #[cfg(test)]
    pub fn bone_pose(&self) -> Option<&[BoneTransform]> {
        self.skeleton.as_ref().map(|(_, pose)| &pose[..])
    }

    /// Selects `bone` of the root's skeleton (and the root).
    pub fn select_bone(&mut self, bone: Option<usize>) {
        self.bones.selected = bone;
        if bone.is_some() {
            self.selected = self.root;
        }
    }

    /// The skeleton as posed, on the root in the posed copy.
    pub fn skeleton(&self) -> Option<Skeleton<'_>> {
        let (model, pose) = self.skeleton.as_ref()?;
        Skeleton::on(&self.posed, self.root?, model.rig.as_deref()?, pose, &[])
    }

    /// The root's bone paths, as bone tracks name them.
    pub fn bone_paths(&self) -> Vec<String> {
        self.rig().map(bone_paths).unwrap_or_default()
    }

    /// The root's rig, when it has bones.
    pub fn rig(&self) -> Option<&axcenity_assets::Rig> {
        self.skeleton.as_ref()?.0.rig.as_deref()
    }

    /// Targets and properties the clip has that the copy hasn't.
    pub fn missing(&self) -> Vec<String> {
        self.applied.missing.iter().cloned().collect()
    }

    /// The value of `property` on `id` as posed: a Model's pivot for its
    /// Position and Orientation.
    fn value(&self, id: InstanceId, property: PropertyName) -> Option<Vec<f32>> {
        let instance = self.posed.get(id)?;
        if instance.class() == ClassName::Model {
            let pivot = self.applied.pivot(&self.posed, id)?;
            let (position, orientation) = pivot.numbers();
            return match property {
                PropertyName::Position => Some(position.to_vec()),
                PropertyName::Orientation => Some(orientation.to_vec()),
                _ => None,
            };
        }
        let numbers = numbers_of(instance.property(property.as_str())?)?;
        property
            .widths()
            .contains(&numbers.len())
            .then_some(numbers)
    }

    /// The selected object, with what can be keyed on it: the selected
    /// bone, if one is.
    pub fn focus(&self) -> Option<Focus> {
        if let Some(bone) = self.bones.selected
            && let Some((model, pose)) = &self.skeleton
            && let Some(rig) = model.rig.as_deref()
            && let (Some(path), Some(transform)) = (bone_paths(rig).get(bone), pose.get(bone))
        {
            return Some(Focus {
                target: path.clone(),
                name: rig.bones[bone].name.clone(),
                values: vec![EvaluatedProperty {
                    target: path.clone(),
                    property: PropertyName::Bone,
                    value: bone_numbers(transform),
                }],
                transform: vec![PropertyName::Bone],
            });
        }
        let root = self.root?;
        let id = self.selected?;
        let instance = self.posed.get(id)?;
        let target = target_path(&self.posed, root, id)?;
        let values = PropertyName::ALL
            .into_iter()
            .filter_map(|property| {
                Some(EvaluatedProperty {
                    target: target.clone(),
                    property,
                    value: self.value(id, property)?,
                })
            })
            .collect();
        let transform = if is_ui(instance.class()) {
            vec![PropertyName::Position, PropertyName::Size]
        } else {
            vec![PropertyName::Position, PropertyName::Orientation]
        };
        Some(Focus {
            target,
            name: instance.name().to_owned(),
            values,
            transform,
        })
    }

    fn camera(&self) -> Camera {
        let (yaw, pitch) = (self.yaw.to_radians(), self.pitch.to_radians());
        let direction = Vec3::new(
            yaw.sin() * pitch.cos(),
            pitch.sin(),
            yaw.cos() * pitch.cos(),
        );
        Camera {
            position: self.middle + direction * self.height.max(0.5) * self.zoom,
            look_at: self.middle,
            fov_y_degrees: 50.0,
        }
    }

    /// Where the selected object's handles go: a part's pose, or a Model's
    /// pivot (which can't be resized).
    fn selected_pose(&self) -> Option<(InstanceId, Pose)> {
        let id = self.selected?;
        let instance = self.posed.get(id)?;
        if instance.class() == ClassName::Model {
            let pivot = self.applied.pivot(&self.posed, id)?;
            return Some((
                id,
                Pose {
                    position: pivot.position,
                    rotation: pivot.rotation,
                    size: Vec3::ONE,
                },
            ));
        }
        Some((id, Pose::of(instance)?))
    }

    /// The Move / Rotate / Resize buttons, shown above a 3D preview.
    pub fn tools(&mut self, ui: &mut egui::Ui) {
        if self.flat || self.root.is_none() {
            return;
        }
        if self.bones.selected.is_some()
            && let Some((model, _)) = &self.skeleton
            && let Some(rig) = model.rig.as_deref()
        {
            self.bones.tools(ui, rig);
            return;
        }
        for (tool, icon) in [
            (Tool::Move, icons::ARROWS_OUT_CARDINAL),
            (Tool::Rotate, icons::ARROW_CLOCKWISE),
            (Tool::Scale, icons::ARROWS_OUT),
        ] {
            if crate::widgets::icon_button(ui, icon, tool.label(), self.tool == tool).clicked() {
                self.tool = tool;
            }
        }
    }

    /// Draws the posed copy `height` points tall, and handles clicks and
    /// drags on it.
    pub fn show_ui(
        &mut self,
        ui: &mut egui::Ui,
        render_state: Option<&RenderState>,
        assets: &Assets,
        stylesheets: &[Stylesheet],
        height: f32,
    ) -> Option<PreviewEvent> {
        let size = egui::vec2(ui.available_width(), height.max(80.0));
        let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click_and_drag());
        let painter = ui.painter_at(rect);
        let message = match (render_state, self.root) {
            (None, _) => Some("The preview needs the wgpu renderer."),
            (_, None) => Some(
                "Select a Part, Model or ScreenGui in the Explorer to animate a copy of it here.",
            ),
            _ => None,
        };
        let (Some(render_state), None) = (render_state, message) else {
            painter.rect_filled(rect, 4.0, ui.visuals().extreme_bg_color);
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                message.unwrap_or_default(),
                egui::FontId::proportional(13.0),
                ui.visuals().weak_text_color(),
            );
            return None;
        };
        if self.flat {
            self.show_flat(ui, rect, &response, render_state, assets, stylesheets)
        } else {
            self.show_3d(ui, rect, &response, render_state, assets)
        }
    }

    fn show_3d(
        &mut self,
        ui: &mut egui::Ui,
        rect: egui::Rect,
        response: &egui::Response,
        render_state: &RenderState,
        assets: &Assets,
    ) -> Option<PreviewEvent> {
        let screen = Screen::new(self.camera(), rect);
        let tool = self.tool;
        // With a bone selected, its gizmo shows instead of the part's.
        let pose = self
            .selected_pose()
            .filter(|_| self.bones.selected.is_none() || self.skeleton.is_none());
        let model = self
            .selected
            .and_then(|id| self.posed.get(id))
            .is_some_and(|i| i.class() == ClassName::Model);
        let modes = Modes::default();
        let frame = pose
            .filter(|_| !(model && tool == Tool::Scale))
            .and_then(|(_, pose)| gizmo::handle_frame(tool, modes, &[pose]));
        let shapes = frame.map_or_else(Vec::new, |frame| gizmo::shapes(tool, frame, &screen));
        let (pressed, down, pointer) = ui.input(|input| {
            (
                input.pointer.primary_pressed(),
                input.pointer.primary_down(),
                input.pointer.latest_pos(),
            )
        });
        let mut bones = std::mem::take(&mut self.bones);
        let posing = self
            .skeleton()
            .map(|skeleton| bones.interact(ui, response, &screen, &skeleton));
        let captured = posing.as_ref().is_some_and(|done| done.captured);
        let hovered = match pointer {
            Some(pointer) if response.contains_pointer() && self.drag.is_none() && !captured => {
                gizmo::hovered(&shapes, pointer)
            }
            _ => None,
        };
        if pressed
            && let (Some(handle), Some(pointer), Some(frame), Some(part)) =
                (hovered, pointer, frame, pose)
        {
            self.drag = Drag::start_many(handle, frame, vec![part], modes, screen.ray(pointer))
                .map(Dragging::Handle);
        }

        let mut event = None;
        let highlighted: Option<Handle> = match &self.drag {
            Some(Dragging::Handle(drag)) => Some(drag.handle),
            _ => hovered,
        };
        if let Some(Dragging::Handle(drag)) = &mut self.drag {
            if !down {
                self.drag = None;
            } else if let Some(pointer) = pointer
                && let Some(parts) = drag.update_all(screen.ray(pointer), &Snap::default())
            {
                event = self.edits(parts.into_iter().flat_map(|(id, values)| {
                    values
                        .into_iter()
                        .map(move |(name, value)| (id, name, value))
                }));
            }
        } else if response.dragged() && !captured {
            let delta = response.drag_delta();
            self.yaw -= delta.x * 0.4;
            self.pitch = (self.pitch + delta.y * 0.3).clamp(-10.0, 85.0);
        }
        if response.hovered() {
            let scroll = ui.input(|input| input.smooth_scroll_delta.y);
            if scroll != 0.0 {
                self.zoom = (self.zoom * (1.0 - scroll * 0.002)).clamp(0.4, 12.0);
            }
        }
        match posing.as_ref().and_then(|done| done.event.clone()) {
            Some(BoneEvent::Select(_)) => {
                self.selected = self.root;
                event = Some(PreviewEvent::Select(self.root));
            }
            Some(BoneEvent::Pose(bone, transform)) => {
                event = self.bone_edit(bone, &transform);
            }
            None => {}
        }
        if response.clicked()
            && hovered.is_none()
            && !captured
            && let Some(pointer) = response.interact_pointer_pos()
        {
            let hit = axcenity_scene::pick(&self.posed, screen.ray(pointer))
                .filter(|&id| self.inside(id));
            self.selected = hit;
            bones.selected = None;
            event = Some(PreviewEvent::Select(hit));
        }
        if hovered.is_some() || self.drag.is_some() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
        }

        let mut poses = axcenity_scene::Poses::new();
        if let (Some(root), Some((model, pose))) = (self.root, &self.skeleton)
            && let Some(posed) = model.pose(pose)
        {
            poses.insert(root, Arc::new(posed));
        }
        let list = axcenity_scene::build_draw_list_posed(
            &self.posed,
            assets,
            self.camera(),
            &poses,
            &mut axcenity_scene::TerrainCache::new(),
        );
        self.canvas.paint(ui, rect, render_state, &list);
        let painter = ui.painter_at(rect);
        if let (Some(done), Some(skeleton)) = (&posing, self.skeleton()) {
            bones.paint(&painter, &screen, &skeleton, done);
        }
        self.bones = bones;
        if let Some((_, pose)) = pose {
            outline(
                &painter,
                &screen,
                pose,
                model,
                theme::palette(ui.ctx()).accent,
            );
        }
        gizmo::paint(&painter, &shapes, highlighted);
        if response.hovered() && self.drag.is_none() {
            hint(
                &painter,
                rect,
                if self.skeleton.is_some() {
                    "Click a joint or part to select it · drag to turn · scroll to zoom"
                } else {
                    "Click to select · drag to turn · scroll to zoom"
                },
            );
        }
        event
    }

    fn show_flat(
        &mut self,
        ui: &mut egui::Ui,
        rect: egui::Rect,
        response: &egui::Response,
        render_state: &RenderState,
        assets: &Assets,
        stylesheets: &[Stylesheet],
    ) -> Option<PreviewEvent> {
        let ppp = ui.ctx().pixels_per_point();
        let (width, height) = Canvas::pixels(ui, rect);
        let layout: UiLayout = axcenity_ui::layout_ui(&self.posed, width, height, stylesheets);
        // The handles ignore stylesheets: a key sets the properties, never CSS.
        let handles = self
            .selected
            .and_then(|id| layout.element(id))
            .map(|&element| Handles::new(element, &self.posed, &[], rect.min, ppp));
        let (pressed, down, pointer) = ui.input(|input| {
            (
                input.pointer.primary_pressed(),
                input.pointer.primary_down(),
                input.pointer.latest_pos(),
            )
        });
        let grip = match (pointer, &handles) {
            (Some(pointer), Some(handles)) if response.contains_pointer() => {
                handles.grip_at(pointer)
            }
            _ => None,
        };
        if pressed
            && self.drag.is_none()
            && let (Some(grip), Some(pointer), Some(handles)) = (grip, pointer, &handles)
        {
            self.drag = UiDrag::start(handles, grip, pointer, &self.posed)
                .map(|drag| Dragging::Ui(Box::new(drag)));
        }
        let mut event = None;
        if let Some(Dragging::Ui(drag)) = &self.drag {
            if !down {
                self.drag = None;
            } else if let Some(pointer) = pointer {
                let viewport = UiRect {
                    x: 0.0,
                    y: 0.0,
                    width: width as f32,
                    height: height as f32,
                };
                let edit = drag.update(pointer, ppp, &self.posed, &layout.elements, viewport);
                let id = edit.id;
                event = self.edits(
                    edit.values
                        .into_iter()
                        .map(|(name, value)| (id, name, value)),
                );
            }
        }
        if response.clicked()
            && let Some(pointer) = response.interact_pointer_pos()
        {
            let at = (pointer - rect.min) * ppp;
            let hit = layout.hit(at.x, at.y).filter(|&id| self.inside(id));
            if hit != self.selected {
                self.selected = hit;
                event = Some(PreviewEvent::Select(hit));
            }
        }
        match (&self.drag, &handles) {
            (Some(Dragging::Ui(drag)), Some(handles)) => {
                ui.ctx().set_cursor_icon(handles.cursor(drag.grip));
            }
            (None, Some(handles)) if grip.is_some_and(|g| g != Grip::BODY) => {
                ui.ctx()
                    .set_cursor_icon(handles.cursor(grip.expect("checked")));
            }
            _ => {}
        }

        let mut list = axcenity_scene::build_draw_list_posed(
            &self.posed,
            assets,
            self.camera(),
            &axcenity_scene::Poses::new(),
            &mut axcenity_scene::TerrainCache::new(),
        );
        list.ui = layout.draw_items;
        self.canvas.paint(ui, rect, render_state, &list);
        let painter = ui.painter_at(rect);
        if let Some(handles) = &handles {
            let highlighted = match &self.drag {
                Some(Dragging::Ui(drag)) => Some(drag.grip),
                _ => grip,
            };
            handles.paint(&painter, theme::palette(ui.ctx()).accent, highlighted);
        }
        if response.hovered() && self.drag.is_none() {
            hint(
                &painter,
                rect,
                "Click an element to select it · drag it or its handles",
            );
        }
        event
    }

    /// A bone turned or moved in the preview, as an edit of its `Bone`.
    pub(super) fn bone_edit(&self, bone: usize, transform: &BoneTransform) -> Option<PreviewEvent> {
        let path = self.bone_paths().get(bone)?.clone();
        Some(PreviewEvent::Edit(vec![EvaluatedProperty {
            target: path,
            property: PropertyName::Bone,
            value: bone_numbers(transform),
        }]))
    }

    /// Whether `id` is the root or inside it (not the floor).
    fn inside(&self, id: InstanceId) -> bool {
        self.root
            .is_some_and(|root| id == root || self.posed.is_descendant_of(id, root))
    }

    /// Dragged values as an edit, by target path. Values the clip can't
    /// key are left out.
    fn edits(
        &self,
        values: impl Iterator<Item = (InstanceId, &'static str, Value)>,
    ) -> Option<PreviewEvent> {
        let root = self.root?;
        let edits: Vec<EvaluatedProperty> = values
            .filter_map(|(id, name, value)| {
                Some(EvaluatedProperty {
                    target: target_path(&self.posed, root, id)?,
                    property: PropertyName::from_name(name)?,
                    value: numbers_of(&value)?,
                })
            })
            .collect();
        (!edits.is_empty()).then_some(PreviewEvent::Edit(edits))
    }

    /// The copy's tree under the root, parents first: each object with its
    /// depth.
    pub fn tree(&self) -> Vec<(InstanceId, usize)> {
        let Some(root) = self.root else {
            return Vec::new();
        };
        let mut out = Vec::new();
        let mut stack = vec![(root, 0)];
        while let Some((id, depth)) = stack.pop() {
            let Some(instance) = self.pristine.get(id) else {
                continue;
            };
            out.push((id, depth));
            for &child in instance.children().iter().rev() {
                stack.push((child, depth + 1));
            }
        }
        out
    }
}

/// The world-aligned box around `root` and the parts inside it.
fn bounds(world: &World, root: InstanceId) -> Option<(Vec3, Vec3)> {
    let mut min = Vec3::splat(f32::INFINITY);
    let mut max = Vec3::splat(f32::NEG_INFINITY);
    for id in std::iter::once(root).chain(world.descendants(root)) {
        let Some(instance) = world.get(id) else {
            continue;
        };
        let Some(pose) = Pose::of(instance) else {
            continue;
        };
        let reach = [Vec3::X, Vec3::Y, Vec3::Z]
            .into_iter()
            .zip(pose.size.to_array())
            .fold(Vec3::ZERO, |sum, (axis, length)| {
                sum + (pose.rotation * axis).abs() * length / 2.0
            });
        min = min.min(pose.position - reach);
        max = max.max(pose.position + reach);
    }
    min.x.is_finite().then_some((min, max))
}

/// A box around a selected part, or a small cross on a Model's pivot.
fn outline(
    painter: &egui::Painter,
    screen: &Screen,
    pose: Pose,
    pivot: bool,
    color: egui::Color32,
) {
    let stroke = egui::Stroke::new(1.5_f32, color);
    if pivot {
        let length = screen.studs_per_point(pose.position) * 10.0;
        for axis in [Vec3::X, Vec3::Y, Vec3::Z] {
            let along = pose.rotation * axis * length;
            if let Some(line) = screen.line(pose.position - along, pose.position + along) {
                painter.line_segment(line, stroke);
            }
        }
        return;
    }
    let half = pose.size / 2.0;
    let corner = |x: f32, y: f32, z: f32| {
        pose.position + pose.rotation * Vec3::new(x * half.x, y * half.y, z * half.z)
    };
    let signs = [-1.0, 1.0];
    for a in signs {
        for b in signs {
            for (from, to) in [
                (corner(-1.0, a, b), corner(1.0, a, b)),
                (corner(a, -1.0, b), corner(a, 1.0, b)),
                (corner(a, b, -1.0), corner(a, b, 1.0)),
            ] {
                if let Some(line) = screen.line(from, to) {
                    painter.line_segment(line, stroke);
                }
            }
        }
    }
}

fn hint(painter: &egui::Painter, rect: egui::Rect, text: &str) {
    painter.text(
        rect.left_bottom() + egui::vec2(8.0, -6.0),
        egui::Align2::LEFT_BOTTOM,
        text,
        egui::FontId::proportional(11.0),
        egui::Color32::from_white_alpha(160),
    );
}

#[cfg(test)]
mod tests;
