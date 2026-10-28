//! The Material tab (`material_editor.rs`) in the app: what it asks for,
//! saving materials with the project, and handing each edit to the shared
//! `Assets`, so every part using the material (or one built on it) is drawn
//! with it at once, in the viewport and in Play. These are methods of
//! [`super::EditorApp`].

use axcenity_assets::{MATERIALS_FOLDER, is_material_file};
use eframe::egui;
use eframe::egui_wgpu::RenderState;

use super::*;
use crate::asset_browser::{AssetKind, display_name};
use crate::material_editor::{
    MaterialAction, MaterialScene, SelectedLook, material_path, new_material,
};

impl EditorApp {
    /// The Material tab, then what was asked for in it.
    pub(in crate::app) fn material_pane(
        &mut self,
        ui: &mut egui::Ui,
        render_state: Option<&RenderState>,
    ) {
        let entries = self.asset_browser.entries();
        let files: Vec<String> = entries
            .iter()
            .filter(|entry| entry.kind == AssetKind::Material)
            .map(|entry| entry.path.clone())
            .collect();
        let images: Vec<String> = entries
            .iter()
            .filter(|entry| matches!(entry.kind, AssetKind::Texture | AssetKind::Image))
            .map(|entry| entry.path.clone())
            .collect();
        let selected = self
            .selection
            .iter()
            .find_map(|&id| SelectedLook::of(&self.world, id));
        let scene = MaterialScene {
            files: &files,
            images: &images,
            assets: &self.assets,
            selected,
            render_state,
            editable: self.project.is_some(),
        };
        let actions = self.materials.show(ui, &scene);
        self.hand_over_materials();
        for action in actions {
            self.material_action(action);
        }
    }

    pub(in crate::app) fn material_action(&mut self, action: MaterialAction) {
        match action {
            MaterialAction::Show(path) => self.open_material(&path),
            MaterialAction::New(name) => self.new_material(&name),
            MaterialAction::Apply(path) => {
                if self.playing() {
                    self.output
                        .info("Stop the game to change which material parts use.");
                    return;
                }
                let name = display_name(&path);
                self.use_material(&path, &name, self.selection.clone());
            }
        }
    }

    /// Gives the materials edited in the tab to the shared `Assets`, so
    /// the parts using them are drawn with the edit.
    pub(in crate::app) fn hand_over_materials(&mut self) {
        for (path, file) in self.materials.take_changed() {
            self.assets.put_material_file(&path, Some(file));
        }
    }

    /// Starts the material `name` (`assets/materials/<name>.material.json`)
    /// and shows it.
    pub(in crate::app) fn new_material(&mut self, name: &str) {
        let Some(folder) = self.project.as_ref().map(|project| project.folder.clone()) else {
            return;
        };
        let stem = crate::asset_browser::prefab_file_stem(name.trim());
        let path = material_path(&stem);
        if folder.join(&path).exists() || self.materials.document(&path).is_some() {
            self.output
                .error(format!("There's already a material named `{stem}`."));
            return;
        }
        self.materials.create(&path, new_material());
        self.hand_over_materials();
        self.layout.open(Pane::Material);
        self.output.info(format!(
            "Made the material `{path}`. It's written when the project is saved; select \
             parts and press Apply, or drag it from the Assets panel onto a part."
        ));
    }

    /// Shows the material file at `path` in the Material tab.
    pub(in crate::app) fn open_material(&mut self, path: &str) {
        let Some(folder) = self.project.as_ref().map(|project| project.folder.clone()) else {
            return;
        };
        if !is_material_file(path) {
            self.output.error(format!(
                "`{path}` isn't a material (a .material.json file, such as in {MATERIALS_FOLDER})."
            ));
            return;
        }
        match self.materials.open(&folder, path) {
            Ok(()) => {
                self.hand_over_materials();
                self.layout.open(Pane::Material);
            }
            Err(why) => self.output.error(format!("Could not open `{path}`: {why}")),
        }
    }

    /// Writes every material with changes. Adds what it wrote to `saved`.
    /// False when one couldn't be written.
    pub(super) fn save_materials(&mut self, saved: &mut Vec<String>) -> bool {
        let Some(folder) = self.project.as_ref().map(|project| project.folder.clone()) else {
            return true;
        };
        match self.materials.save(&folder) {
            Ok(written) => {
                if !written.is_empty() {
                    self.asset_browser.refresh();
                }
                saved.extend(written);
                true
            }
            Err(why) => {
                self.output.error(why);
                false
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use axcenity_core::{InstanceId, Value, World};
    use axcenity_render::glam::Vec3;
    use axcenity_render::{Camera, DrawList};

    use super::*;
    use crate::app::tests::{copy_of_example, part};
    use crate::material_editor::PreviewShape;

    /// What a draw-list material looks like, without its textures: base
    /// color, metalness, roughness, glow, opacity, cutoff, tiling, offset
    /// and double-sided.
    type Look = (
        [f32; 3],
        f32,
        f32,
        [f32; 3],
        f32,
        f32,
        [f32; 2],
        [f32; 2],
        bool,
    );

    fn look_of(material: &axcenity_render::Material) -> Look {
        (
            material.base_color,
            material.metalness,
            material.roughness,
            material.emissive,
            material.opacity,
            material.alpha_cutoff,
            material.uv_scale,
            material.uv_offset,
            material.double_sided,
        )
    }

    fn camera() -> Camera {
        Camera {
            position: Vec3::new(0.0, 40.0, 40.0),
            look_at: Vec3::ZERO,
            fov_y_degrees: 70.0,
        }
    }

    /// How the part `id` of `world` is drawn: the item standing where it is.
    fn drawn(world: &World, id: InstanceId, list: &DrawList) -> Look {
        let at = world
            .get(id)
            .unwrap()
            .property("Position")
            .and_then(Value::as_vector3)
            .unwrap();
        let at = Vec3::new(at.x, at.y, at.z);
        let item = list
            .items
            .iter()
            .find(|item| item.transform.w_axis.truncate().distance(at) < 1e-3)
            .expect("the part is drawn");
        look_of(&item.material)
    }

    fn viewport(app: &EditorApp, id: InstanceId) -> Look {
        drawn(
            &app.world,
            id,
            &axcenity_scene::build_draw_list(&app.world, &app.assets, camera()),
        )
    }

    /// How the preview draws the material at `path` on its sphere, or on
    /// the selected part `on`.
    fn preview(app: &mut EditorApp, path: &str, on: Option<InstanceId>) -> Look {
        let selected = on.and_then(|id| SelectedLook::of(&app.world, id));
        app.materials.shape = match selected {
            Some(_) => PreviewShape::Selected,
            None => PreviewShape::Sphere,
        };
        let list = app
            .materials
            .preview_list(path, selected.as_ref(), &app.assets);
        // The sphere or the part stands over the middle of the floor.
        let item = list
            .items
            .iter()
            .find(|item| item.transform.w_axis.x == 0.0 && item.transform.w_axis.z == 0.0)
            .expect("the preview draws the part");
        look_of(&item.material)
    }

    #[test]
    fn a_material_made_edited_saved_reopened_and_assigned_looks_the_same_in_the_preview_the_viewport_and_play()
     {
        let folder = copy_of_example("hello-touched", "material-tab");
        let mut app = EditorApp::new(Some(&folder));
        app.new_material("Brick");
        let path = "assets/materials/Brick.material.json";
        assert_eq!(app.materials.current().unwrap().path, path);
        assert!(app.layout.is_open(Pane::Material));
        assert!(app.unsaved(), "a new material waits for Save");
        app.materials
            .edit("color", |file| file.color = Some([0.7, 0.25, 0.1]));
        app.materials
            .edit("color", |file| file.color = Some([0.72, 0.2, 0.1]));
        app.materials.finish_edit();
        app.materials
            .edit("metalness", |file| file.metalness = Some(0.4));
        app.materials.finish_edit();
        app.materials
            .edit("roughness", |file| file.roughness = Some(0.8));
        app.materials.finish_edit();
        app.materials
            .edit("tiling", |file| file.tiling = Some([2.0, 3.0]));
        app.hand_over_materials();
        // Dragging the color was one step.
        assert!(app.materials.undo() && app.materials.undo() && app.materials.undo());
        assert_eq!(
            app.materials.current().unwrap().file.color,
            Some([0.72, 0.2, 0.1])
        );
        assert!(app.materials.undo());
        assert_eq!(
            app.materials.current().unwrap().file.color,
            Some([1.0, 1.0, 1.0])
        );
        assert!(!app.materials.undo());
        for _ in 0..4 {
            assert!(app.materials.redo());
        }
        assert!(!app.materials.redo());
        app.hand_over_materials();
        let edited = app.materials.current().unwrap().file.clone();
        let before = preview(&mut app, path, None);

        assert!(app.save());
        assert!(!app.materials.unsaved());
        let written = axcenity_assets::read_material_file(&folder.join(path)).unwrap();
        assert_eq!(written, edited);

        let mut app = EditorApp::new(Some(&folder));
        app.open_material(path);
        assert_eq!(app.materials.current().unwrap().file, edited);
        let pad = part(&app, "Pad");
        app.selection = vec![pad];
        app.material_action(MaterialAction::Apply(path.to_owned()));
        assert_eq!(
            app.world.get(pad).unwrap().property("MaterialVariant"),
            Some(&Value::String(path.to_owned()))
        );

        let on_sphere = preview(&mut app, path, None);
        assert_eq!(on_sphere, before, "reopened, the preview is the same");
        let in_viewport = viewport(&app, pad);
        let on_pad = preview(&mut app, path, Some(pad));
        assert_eq!(on_pad, in_viewport, "the preview of the pad is the pad");
        assert_eq!(on_sphere, in_viewport, "and the sphere is drawn alike");
        assert!((in_viewport.1 - 0.4).abs() < 1e-6 && (in_viewport.2 - 0.8).abs() < 1e-6);

        app.play();
        assert!(app.playing());
        let game = app.play.as_ref().unwrap().world();
        let in_play = drawn(
            game,
            pad,
            &axcenity_scene::build_draw_list(game, &app.assets, camera()),
        );
        assert_eq!(in_play, in_viewport, "Play draws it the same");
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn editing_a_base_updates_its_instances_previews_and_parts_except_their_overrides() {
        let folder = copy_of_example("hello-touched", "material-base");
        let mut app = EditorApp::new(Some(&folder));
        let (stone, mossy) = (
            "assets/materials/Stone.material.json",
            "assets/materials/Mossy.material.json",
        );
        app.new_material("Stone");
        app.materials
            .edit("roughness", |file| file.roughness = Some(0.9));
        app.new_material("Mossy");
        // An instance: everything but its color from Stone.
        app.materials.edit("base", |file| {
            *file = axcenity_assets::MaterialFile {
                base: Some(stone.to_owned()),
                color: Some([0.2, 0.6, 0.2]),
                ..Default::default()
            }
        });
        app.hand_over_materials();
        assert!(app.save());
        let (pad, ball) = (part(&app, "Pad"), part(&app, "Ball"));
        app.use_material(stone, "Stone", vec![pad]);
        app.use_material(mossy, "Mossy", vec![ball]);
        assert!(
            (viewport(&app, ball).2 - 0.9).abs() < 1e-6,
            "the instance's roughness is its base's"
        );
        let green = viewport(&app, ball).0;

        app.open_material(stone);
        app.materials
            .edit("roughness", |file| file.roughness = Some(0.3));
        app.materials.finish_edit();
        app.materials
            .edit("color", |file| file.color = Some([0.9, 0.1, 0.1]));
        app.hand_over_materials();

        let (base_part, instance_part) = (viewport(&app, pad), viewport(&app, ball));
        assert!((base_part.2 - 0.3).abs() < 1e-6, "{base_part:?}");
        assert!(
            (instance_part.2 - 0.3).abs() < 1e-6,
            "the instance follows its base"
        );
        assert_eq!(instance_part.0, green, "but keeps its own color");
        let red = base_part.0;
        assert!(red[0] > red[1], "the base's part is red: {red:?}");
        let mossy_preview = preview(&mut app, mossy, None);
        assert!((mossy_preview.2 - 0.3).abs() < 1e-6, "{mossy_preview:?}");
        assert_eq!(
            preview(&mut app, mossy, Some(ball)),
            instance_part,
            "the instance's preview on its part is the part"
        );

        // Undo puts the base back, for every part using it.
        assert!(app.materials.undo() && app.materials.undo());
        app.hand_over_materials();
        assert!((viewport(&app, pad).2 - 0.9).abs() < 1e-6);
        assert!((viewport(&app, ball).2 - 0.9).abs() < 1e-6);
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn the_preview_draws_the_material_on_its_sphere() {
        use axcenity_render::wgpu;
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
        let Ok(adapter) = pollster::block_on(instance.request_adapter(&Default::default())) else {
            eprintln!("skipped: no graphics adapter on this machine");
            return;
        };
        let Ok((device, queue)) = pollster::block_on(adapter.request_device(&Default::default()))
        else {
            return;
        };
        let folder = copy_of_example("hello-touched", "material-preview");
        let mut app = EditorApp::new(Some(&folder));
        app.new_material("Red");
        let path = "assets/materials/Red.material.json";
        let look = |app: &mut EditorApp, color: [f32; 3]| {
            app.materials.edit("color", |file| file.color = Some(color));
            app.hand_over_materials();
            let list = app.materials.preview_list(path, None, &app.assets);
            let (width, height) = (160, 120);
            let mut renderer =
                axcenity_render::TextureRenderer::new(&device, &queue, width, height);
            renderer.render(&list).unwrap();
            let pixels = renderer.read_pixels();
            if let Some(snapshots) = std::env::var_os("AXCENITY_SNAPSHOTS") {
                let name = format!(
                    "material-preview-{}.png",
                    color.map(|c| c.to_string()).join("-")
                );
                image::save_buffer(
                    std::path::Path::new(&snapshots).join(name),
                    &pixels,
                    width,
                    height,
                    image::ExtendedColorType::Rgba8,
                )
                .unwrap();
            }
            // The middle of the picture is the sphere.
            let at = ((height / 2 * width + width / 2) * 4) as usize;
            [0, 1, 2].map(|channel| i32::from(pixels[at + channel]))
        };
        let [r, g, b] = look(&mut app, [0.9, 0.1, 0.1]);
        assert!(r > g + 40 && r > b + 40, "a red sphere: {r} {g} {b}");
        let [r, g, b] = look(&mut app, [0.1, 0.2, 0.9]);
        assert!(b > r + 40 && b > g + 20, "turned blue at once: {r} {g} {b}");
        let _ = std::fs::remove_dir_all(&folder);
    }
}
