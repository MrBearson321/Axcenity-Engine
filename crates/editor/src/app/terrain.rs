//! The Terrain Studio's side of the app: carrying out the panel's actions
//! as undo steps, brush strokes from the viewport, and the background jobs
//! (generating, eroding) whose results are previewed until applied. These
//! are methods of [`super::EditorApp`]; the panel is `terrain_studio.rs`.
//!
//! A stroke changes the world as it goes (so the viewport shows it at
//! once) and becomes one undo step when the button is let go: only the
//! chunks it touched are kept. Whole new landscapes (generating, erosion,
//! importing, resampling) are one step holding the landscape before and after.

use std::sync::Arc;
use std::time::Duration;

use axcenity_core::{
    MAX_TERRAIN_LAYERS, MAX_TERRAIN_SAMPLES, MIN_TERRAIN_SAMPLES, TerrainData, TextureChannel,
};
use axcenity_render::glam::{Vec2, Vec3};
use axcenity_terrain::Placement;
use axcenity_terrain::brush::{
    self, EditRecorder, Operation, SculptMode, Shape, StampImage, Stroke,
};
use axcenity_terrain::erosion::{self, HydraulicSettings, Scale};
use axcenity_terrain::files;
use axcenity_terrain::generate::{self, GeneratorSettings, Preset, Target};
use axcenity_terrain::job::Job;
use axcenity_terrain::materials::{self, MaterialRules};

use super::*;
use crate::history::TerrainChange;
use crate::terrain_studio::{
    ActiveStroke, Combine, ErosionKind, JobResult, LayerPreset, Preview, RunningJob, StudioContext,
    TerrainAction, TerrainTool, layers_for,
};
use crate::viewport::{TerrainBrush, TerrainPointer};

/// How long generator settings must stay still before a new preview is made.
const LIVE_PREVIEW_DELAY: f64 = 0.35;
/// Held still, a stroke stamps again this often (seconds).
const STROKE_REPEAT: f32 = 0.05;
/// How much `[` and `]` change the brush's size.
const BRUSH_STEP: f32 = 1.15;
/// Where stamp images picked from outside the project are copied to.
const STAMP_FOLDER: &str = "assets/terrain/stamps";

impl EditorApp {
    /// The Terrain panel.
    pub(super) fn terrain_pane(&mut self, ui: &mut egui::Ui) {
        let editing = self.project.is_some() && !self.playing();
        let time = ui.input(|input| input.time);
        let actions = self.terrain.show(
            ui,
            &StudioContext {
                world: &self.world,
                selected: self.selected,
                editing,
                time,
            },
        );
        for action in actions {
            self.terrain_action(action);
        }
    }

    /// Runs every frame, before the panels are drawn: finished jobs become
    /// previews, the viewport is told what to draw, and the studio's keys.
    pub(super) fn terrain_frame(&mut self, ctx: &egui::Context) {
        if self.playing() || self.project.is_none() {
            self.viewport.terrain.clear_overrides();
            return;
        }
        // Closing the panel puts the tool down.
        if !self.layout.is_open(Pane::Terrain) {
            self.terrain.tool = None;
        }
        self.terrain.resolve_target(&self.world, self.selected);
        // A preview or stroke for a terrain that's gone (deleted, or an insert undone).
        let alive = |id: InstanceId| crate::terrain_studio::terrains(&self.world).contains(&id);
        if self
            .terrain
            .preview
            .as_ref()
            .is_some_and(|p| !alive(p.terrain))
        {
            self.terrain.preview = None;
        }
        if self
            .terrain
            .stroke
            .as_ref()
            .is_some_and(|s| !alive(s.terrain))
        {
            self.terrain.stroke = None;
        }
        self.poll_terrain_job(ctx);

        // Generator settings changed: make the preview again once they settle.
        if let Some(changed) = self.terrain.changed_at {
            let now = ctx.input(|input| input.time);
            let regenerating = self.terrain.tool == Some(TerrainTool::Generate)
                && self.terrain.live_preview
                && (self.terrain.job.as_ref().is_some_and(|job| !job.erosion)
                    || self.terrain.preview.as_ref().is_some_and(|p| !p.erosion));
            if !regenerating {
                self.terrain.changed_at = None;
            } else if now - changed >= LIVE_PREVIEW_DELAY {
                self.terrain.changed_at = None;
                self.start_generate();
            } else {
                ctx.request_repaint_after(Duration::from_secs_f64(
                    LIVE_PREVIEW_DELAY - (now - changed),
                ));
            }
        }

        let shown = self
            .terrain
            .preview
            .as_ref()
            .filter(|_| !self.terrain.comparing)
            .map(|p| (p.terrain, p.data.clone(), p.size));
        self.viewport.terrain.clear_overrides();
        if let Some((id, data, size)) = shown {
            let size = size.map(|s| Vec3::new(s.x, s.y, s.z));
            self.viewport.terrain.set_override(id, Some(data), size);
        }
        self.viewport.terrain.view = self.terrain.view;
        self.terrain_keys(ctx);
    }

    /// 1 to 0 pick tools, `[` and `]` size the brush, Escape puts it down.
    fn terrain_keys(&mut self, ctx: &egui::Context) {
        if self.terrain.tool.is_none() || ctx.wants_keyboard_input() {
            return;
        }
        let none = egui::Modifiers::NONE;
        let (tool, smaller, bigger, escape) = ctx.input_mut(|input| {
            let tool = TerrainTool::all()
                .find(|tool| tool.key().is_some_and(|key| input.consume_key(none, key)));
            (
                tool,
                input.consume_key(none, egui::Key::OpenBracket),
                input.consume_key(none, egui::Key::CloseBracket),
                input.consume_key(none, egui::Key::Escape),
            )
        });
        if let Some(tool) = tool {
            self.terrain.tool = Some(tool);
        }
        let brush = &mut self.terrain.brush;
        if smaller {
            brush.radius = (brush.radius / BRUSH_STEP).max(0.5);
        }
        if bigger {
            brush.radius = (brush.radius * BRUSH_STEP).min(256.0);
        }
        if escape {
            self.terrain.tool = None;
        }
    }

    /// The brush to show in the viewport, while one is being used.
    pub(super) fn terrain_brush_view(&self, ctx: &egui::Context) -> Option<TerrainBrush> {
        if self.playing() || !self.terrain.brushing() {
            return None;
        }
        let terrain = self.terrain.target?;
        self.world.get(terrain)?;
        let brush = &self.terrain.brush;
        Some(TerrainBrush {
            terrain,
            radius: brush.radius,
            falloff: brush.falloff,
            square: brush.shape == Shape::Square,
            color: self
                .terrain
                .brush_color(&self.world, theme::palette(ctx).accent),
        })
    }

    /// The mouse over the terrain while a brush is held: starts, continues
    /// and ends strokes, and Ctrl+click samples.
    pub(super) fn terrain_pointer(&mut self, pointer: TerrainPointer) {
        if self.playing() {
            return;
        }
        let (Some(terrain), Some(tool)) = (self.terrain.target, self.terrain.tool) else {
            return;
        };
        if pointer.pressed && pointer.modifiers.command {
            if let Some(point) = pointer.point {
                self.sample_terrain(terrain, tool, point);
            }
            return;
        }
        if pointer.pressed
            && self.terrain.stroke.is_none()
            && let Some(point) = pointer.point
            && let Some((operation, label)) =
                self.stroke_operation(terrain, tool, point, pointer.modifiers.shift)
        {
            self.terrain.stroke = Some(ActiveStroke {
                terrain,
                once: matches!(operation, Operation::Stamp(_)),
                operation,
                stroke: Stroke::new(),
                recorder: EditRecorder::new(),
                label,
            });
            self.stroke_to(point, pointer.seconds);
        } else if pointer.down
            && let Some(point) = pointer.point
        {
            self.stroke_to(point, pointer.seconds);
        }
        if pointer.released || !pointer.down {
            self.finish_stroke();
        }
    }

    /// What a stroke of `tool` does, starting at `point`, and its undo
    /// step's name. Shift does the opposite.
    fn stroke_operation(
        &mut self,
        terrain: InstanceId,
        tool: TerrainTool,
        point: Vec3,
        shift: bool,
    ) -> Option<(Operation, String)> {
        let studio = &self.terrain;
        Some(match tool {
            TerrainTool::Raise if shift => (Operation::Lower, "Lower Terrain".to_owned()),
            TerrainTool::Raise => (Operation::Raise, "Raise Terrain".to_owned()),
            TerrainTool::Smooth => (Operation::Smooth, "Smooth Terrain".to_owned()),
            TerrainTool::Flatten => {
                let target = if studio.flatten_from_stroke {
                    point.y
                } else {
                    studio.flatten_height
                };
                (
                    Operation::Flatten {
                        target,
                        mode: studio.flatten_mode,
                    },
                    "Flatten Terrain".to_owned(),
                )
            }
            TerrainTool::Sculpt => {
                let mode = match (studio.sculpt, shift) {
                    (SculptMode::Hill, true) => SculptMode::Basin,
                    (SculptMode::Basin, true) => SculptMode::Hill,
                    (SculptMode::Ridge, true) => SculptMode::Valley,
                    (SculptMode::Valley, true) => SculptMode::Ridge,
                    (mode, _) => mode,
                };
                (
                    Operation::Sculpt {
                        mode,
                        step: studio.terrace_step,
                    },
                    format!("Sculpt {}", mode.label()),
                )
            }
            TerrainTool::Stamp => {
                if studio.stamp_image.is_some() && studio.stamp.image.is_none() {
                    self.load_stamp_image();
                }
                let mut stamp = self.terrain.stamp.clone();
                if shift {
                    stamp.height = -stamp.height;
                }
                (Operation::Stamp(stamp), "Stamp Terrain".to_owned())
            }
            TerrainTool::Noise => {
                let sign = if shift { -1.0 } else { 1.0 };
                (
                    Operation::Noise {
                        amplitude: studio.noise_amplitude * sign,
                        scale: studio.noise_scale,
                        seed: studio.noise_seed,
                    },
                    "Roughen Terrain".to_owned(),
                )
            }
            TerrainTool::Paint if shift => (
                Operation::Erase {
                    locked: studio.locked,
                },
                "Erase Terrain Paint".to_owned(),
            ),
            TerrainTool::Paint => {
                let layers = materials::layers(&self.world, terrain);
                let Some(&layer) = layers.get(studio.paint_layer) else {
                    self.output
                        .info("Add a layer to the terrain first: painting puts a layer down.");
                    return None;
                };
                let name = self
                    .world
                    .get(layer)
                    .map_or("Layer".to_owned(), |l| l.name().to_string());
                (
                    Operation::Paint {
                        layer: studio.paint_layer,
                        mode: studio.paint_mode,
                        locked: studio.locked,
                    },
                    format!("Paint {name}"),
                )
            }
            TerrainTool::Auto | TerrainTool::Generate | TerrainTool::Erode | TerrainTool::Masks => {
                return None;
            }
        })
    }

    /// Moves the stroke under way to `point`, stamping along the way.
    fn stroke_to(&mut self, point: Vec3, seconds: f32) {
        let Some(mut active) = self.terrain.stroke.take() else {
            return;
        };
        let brush = self.terrain.brush.clone();
        let placement = self.world.get(active.terrain).and_then(Placement::of);
        let stamps = if active.once {
            // A stamp happens once, where the click was.
            let first = active.stroke.advance(point, f32::MAX, seconds, None);
            first.into_iter().take(1).collect()
        } else {
            active
                .stroke
                .advance(point, brush.spacing_studs(), seconds, Some(STROKE_REPEAT))
        };
        if let (Some(placement), Some(data)) = (placement, self.world.terrain_mut(active.terrain)) {
            for center in stamps {
                for rect in brush::affected_rects(data, &placement, &brush, center) {
                    active.recorder.before_change(data, rect);
                }
                brush::apply(data, &placement, &brush, &active.operation, center);
            }
        }
        self.terrain.stroke = Some(active);
    }

    /// Ends the stroke under way, as one undo step if it changed anything.
    fn finish_stroke(&mut self) {
        let Some(active) = self.terrain.stroke.take() else {
            return;
        };
        let Some(edit) = self
            .world
            .terrain(active.terrain)
            .and_then(|data| active.recorder.finish(data))
        else {
            return;
        };
        let fields = history::snapshot(&self.world, active.terrain);
        self.history.record_terrain(
            &mut self.world,
            active.terrain,
            active.label,
            TerrainChange::Stroke(edit),
            fields.clone(),
            fields,
        );
    }

    /// Ctrl+click: Flatten takes the height there, Paint the layer showing.
    fn sample_terrain(&mut self, terrain: InstanceId, tool: TerrainTool, point: Vec3) {
        match tool {
            TerrainTool::Flatten => {
                self.terrain.flatten_height = point.y;
                self.terrain.flatten_from_stroke = false;
            }
            TerrainTool::Paint => {
                let Some(placement) = self.world.get(terrain).and_then(Placement::of) else {
                    return;
                };
                let Some(data) = self.world.terrain(terrain) else {
                    return;
                };
                let rules = MaterialRules::of(&self.world, terrain);
                if let Some(layer) =
                    materials::dominant_layer(data, &placement, &rules, point.x, point.z)
                {
                    self.terrain.paint_layer = layer;
                }
            }
            _ => {}
        }
    }

    /// Carries out one of the panel's actions.
    fn terrain_action(&mut self, action: TerrainAction) {
        if self.playing() {
            return;
        }
        let target = self.terrain.target;
        match action {
            TerrainAction::Create {
                width,
                length,
                height,
                samples,
            } => self.create_terrain(width, length, height, samples),
            TerrainAction::Target(id) => {
                self.terrain.target = Some(id);
                self.select_from_viewport(Some(id));
            }
            TerrainAction::AddLayer(preset) => {
                if let Some(terrain) = target {
                    self.add_terrain_layers(
                        terrain,
                        &[preset],
                        format!("Add {} Layer", preset.name),
                    );
                }
            }
            TerrainAction::AddLayerSet(preset) => {
                if let Some(terrain) = target {
                    let label = format!("Add {} Layers", preset.label());
                    self.add_terrain_layers(terrain, &layers_for(preset), label);
                }
            }
            TerrainAction::RemoveLayer(layer) => self.delete(layer),
            TerrainAction::MoveLayer(layer, up) => self.move_terrain_layer(layer, up),
            TerrainAction::SetLayer(layer, name, value) => {
                let Some(instance) = self.world.get(layer) else {
                    return;
                };
                let label = format!("Change {name} of {}", instance.name());
                self.change(
                    layer,
                    Field::Property(name),
                    label,
                    vec![Edit::Property(name, value)],
                );
            }
            TerrainAction::PickLayerTexture(layer, name) => {
                self.apply(layer, Edit::PickTexture(name));
            }
            TerrainAction::ImportTextureSet(layer) => self.import_texture_set(layer),
            TerrainAction::CopySurfaceAppearance(layer) => self.copy_surface_appearance(layer),
            TerrainAction::Select(id) => self.select_from_viewport(Some(id)),
            TerrainAction::StartGenerate => self.start_generate(),
            TerrainAction::StartErode => self.start_erode(),
            TerrainAction::ApplyPreview => self.apply_preview(),
            TerrainAction::CancelPreview => self.terrain.preview = None,
            TerrainAction::CancelJob => {
                if let Some(running) = &self.terrain.job {
                    running.job.cancel();
                }
                self.terrain.queued = None;
            }
            TerrainAction::ClearPaint => {
                let Some(terrain) = target else {
                    return;
                };
                let Some(data) = self.world.terrain(terrain) else {
                    return;
                };
                if data
                    .paint()
                    .iter()
                    .all(|weights| *weights == [0; MAX_TERRAIN_LAYERS])
                {
                    self.output
                        .info("The terrain has no painted materials to clear.");
                    return;
                }
                let mut cleared = (**data).clone();
                let all = cleared.bounds();
                cleared.paint_mut_in(all).fill([0; MAX_TERRAIN_LAYERS]);
                self.replace_terrain(
                    terrain,
                    "Clear Painted Materials".to_owned(),
                    &cleared,
                    None,
                );
            }
            TerrainAction::Resample(samples) => self.resample_terrain(samples),
            TerrainAction::ImportHeightmap => self.import_heightmap(),
            TerrainAction::ExportHeightmap => self.export_heightmap(),
            TerrainAction::PickStampImage => self.pick_stamp_image(),
        }
    }

    /// A new flat terrain in the Workspace, with grass, dirt and rock
    /// layers, as one undo step. It's selected and ready to sculpt.
    fn create_terrain(&mut self, width: f32, length: f32, height: f32, samples: usize) {
        let Some(folder) = self.project.as_ref().map(|project| project.folder.clone()) else {
            return;
        };
        let samples_z = samples_along(samples, width, length);
        let data =
            match TerrainData::flat(samples, samples_z, axcenity_core::DEFAULT_TERRAIN_HEIGHT) {
                Ok(data) => data,
                Err(err) => {
                    self.output
                        .error(format!("Could not make the terrain: {err}"));
                    return;
                }
            };
        let Ok(id) = self.world.create(ClassName::Terrain) else {
            return;
        };
        let workspace = self.world.workspace();
        let name = tree::unique_name(&self.world, workspace, "Terrain");
        let taken = terrain_folders(&self.world);
        let data_folder = axcenity_project::new_terrain_folder(&folder, &name, &taken);
        // The flat ground (a quarter of the way up) sits at Y = 0.
        let properties = [
            ("Size", Value::Vector3(Vector3::new(width, height, length))),
            (
                "Position",
                Value::Vector3(Vector3::new(0.0, -0.25 * height, 0.0)),
            ),
            ("DataFolder", Value::String(data_folder)),
        ];
        for (property, value) in properties {
            self.world
                .set_property(id, property, value)
                .expect("a Terrain has these properties");
        }
        self.world.set_name(id, &name).expect("just made");
        self.world.set_terrain(id, Arc::new(data));
        // Its layers go inside it before it's placed, so one step adds all.
        for preset in layers_for(Preset::RollingHills) {
            if let Some(layer) = new_layer(&mut self.world, id, &preset, height) {
                let _ = self.world.set_parent(layer, Some(id));
            }
        }
        let end = self.world.get(workspace).map_or(0, |w| w.children().len());
        let place = Place {
            parent: workspace,
            index: end,
        };
        if self.place(id, Some(place), format!("Insert {name}")) {
            self.select_from_viewport(Some(id));
            self.terrain.target = Some(id);
            self.terrain.tool.get_or_insert(TerrainTool::Raise);
            self.layout.open(Pane::Terrain);
        }
    }

    /// Adds layers to the end of `terrain`'s, as one undo step. Layers past
    /// the most a terrain draws aren't added.
    fn add_terrain_layers(&mut self, terrain: InstanceId, presets: &[LayerPreset], label: String) {
        let Some(instance) = self.world.get(terrain) else {
            return;
        };
        let height = instance
            .property("Size")
            .and_then(Value::as_vector3)
            .map_or(128.0, |size| size.y);
        let room = MAX_TERRAIN_LAYERS.saturating_sub(materials::layers(&self.world, terrain).len());
        if presets.len() > room {
            self.output.warning(format!(
                "A terrain draws up to {MAX_TERRAIN_LAYERS} layers, so only {room} more {} added.",
                if room == 1 { "was" } else { "were" }
            ));
        }
        let mut moves = Vec::new();
        for preset in presets.iter().take(room) {
            let Some(layer) = new_layer(&mut self.world, terrain, preset, height) else {
                continue;
            };
            let index = self.world.get(terrain).map_or(0, |t| t.children().len());
            let place = Place {
                parent: terrain,
                index,
            };
            if history::put(&mut self.world, layer, Some(place)).is_ok() {
                moves.push(Move {
                    id: layer,
                    before: None,
                    after: Some(place),
                });
            }
        }
        if !moves.is_empty() {
            self.history.record_tree(&mut self.world, label, moves);
        }
    }

    /// Swaps a layer with the one before (`up`) or after it.
    fn move_terrain_layer(&mut self, layer: InstanceId, up: bool) {
        let Some(place) = history::place_of(&self.world, layer) else {
            return;
        };
        let Some(siblings) = self.world.get(place.parent).map(|p| p.children().to_vec()) else {
            return;
        };
        let is_layer = |id: &InstanceId| {
            self.world.get(*id).map(|i| i.class()) == Some(ClassName::TerrainLayer)
        };
        // Taking the layer out first moves later siblings back one, so
        // going down, the next layer's index is where it ends up after it.
        let index = if up {
            siblings[..place.index].iter().rposition(is_layer)
        } else {
            siblings[place.index + 1..]
                .iter()
                .position(is_layer)
                .map(|offset| place.index + 1 + offset)
        };
        let Some(index) = index else {
            return;
        };
        let name = self
            .world
            .get(layer)
            .map_or(String::new(), |l| l.name().to_string());
        let label = format!("Move {name} {}", if up { "Up" } else { "Down" });
        self.place(
            layer,
            Some(Place {
                parent: place.parent,
                index,
            }),
            label,
        );
    }

    /// Picks texture files and puts each in the layer's maps its name says
    /// (BaseColor, Normal, ORM…), as one undo step.
    fn import_texture_set(&mut self, layer: InstanceId) {
        let Some(folder) = self.project.as_ref().map(|project| project.folder.clone()) else {
            return;
        };
        let Some(files) = rfd::FileDialog::new()
            .set_title("Choose a material's textures")
            .add_filter("Texture", &axcenity_assets::TEXTURE_EXTENSIONS)
            .set_directory(&folder)
            .pick_files()
        else {
            return;
        };
        let mut edits = Vec::new();
        let mut unknown = Vec::new();
        for file in files {
            let file_name = file
                .file_name()
                .map_or(String::new(), |name| name.to_string_lossy().into_owned());
            let roles = axcenity_assets::texture_roles(&file_name);
            if roles.is_empty() {
                unknown.push(file_name);
                continue;
            }
            let path = match axcenity_assets::import_texture(
                &folder,
                &file,
                axcenity_assets::PROJECT_TEXTURES_FOLDER,
            ) {
                Ok(path) => path,
                Err(err) => {
                    self.output.error(format!(
                        "Could not use `{}`: {}",
                        file.display(),
                        super::files::chain(&err)
                    ));
                    continue;
                }
            };
            edits.extend(layer_map_edits(&roles, &path));
        }
        if !unknown.is_empty() {
            self.output.warning(format!(
                "Skipped {}: their names don't say which map they are (such as `Rock_Normal.png`).",
                unknown.join(", ")
            ));
        }
        if edits.is_empty() {
            return;
        }
        let name = self
            .world
            .get(layer)
            .map_or(String::new(), |l| l.name().to_string());
        self.change(
            layer,
            Field::Property("ColorMap"),
            format!("Import Textures into {name}"),
            edits,
        );
        self.history.seal();
    }

    /// Copies the selected SurfaceAppearance's maps into the layer.
    fn copy_surface_appearance(&mut self, layer: InstanceId) {
        let source = self.selection.iter().copied().find(|&id| {
            self.world.get(id).map(|i| i.class()) == Some(ClassName::SurfaceAppearance)
        });
        let Some(source) = source.and_then(|id| self.world.get(id)) else {
            self.output.info(
                "Select a SurfaceAppearance in the Explorer first (Ctrl+click it to keep the layer \
                 open), then copy its maps.",
            );
            return;
        };
        const COPIED: [&str; 12] = [
            "Color",
            "ColorMap",
            "NormalMap",
            "RoughnessMap",
            "MetalnessMap",
            "AmbientOcclusionMap",
            "NormalStrength",
            "NormalMapDirectX",
            "RoughnessChannel",
            "InvertRoughness",
            "MetalnessChannel",
            "AmbientOcclusionChannel",
        ];
        let edits: Vec<Edit> = COPIED
            .into_iter()
            .filter_map(|name| Some(Edit::Property(name, source.property(name)?.clone())))
            .collect();
        let name = self
            .world
            .get(layer)
            .map_or(String::new(), |l| l.name().to_string());
        self.change(
            layer,
            Field::Property("ColorMap"),
            format!("Copy Material into {name}"),
            edits,
        );
        self.history.seal();
    }

    /// Starts generating a landscape for the terrain being edited, to preview.
    pub(super) fn start_generate(&mut self) {
        let Some(terrain) = self.terrain.target else {
            return;
        };
        if let Some(running) = &self.terrain.job {
            // Started again once the old one stops.
            running.job.cancel();
            self.terrain.queued = Some(false);
            return;
        }
        let Some(placement) = self.world.get(terrain).and_then(Placement::of) else {
            return;
        };
        let new = self.terrain.new_terrain;
        let size_x = new.samples;
        let size_z = samples_along(size_x, new.width, new.length);
        let target = Target {
            size_x,
            size_z,
            world_size: Vec2::new(new.width, new.length),
        };
        let mut settings = self.terrain.generator.clone();
        let generator = match generate::check(&settings, target) {
            Ok(generator) => generator,
            Err(err) => {
                self.output
                    .error(format!("Could not generate the landscape: {err}"));
                return;
            }
        };
        settings.algorithm = generator.name().to_owned();
        settings.version = generator.version();
        let size = Vector3::new(new.width, new.height, new.length);
        let combine = self.terrain.combine;
        let current = self.world.terrain(terrain).cloned();
        let old_range = placement.size.y;
        let title = format!("Generate {}", settings.preset.label());
        let job = Job::spawn(move |control| {
            let heights = generator.generate(&settings, target, control)?;
            let mut data = match (combine, current) {
                (Combine::Add, Some(current)) => {
                    let mut data = current.resampled(size_x, size_z).ok()?;
                    // The new hills and dips, on top of the old ground in studs.
                    let base = settings.base_height;
                    let all = data.bounds();
                    for (old, new) in data.heights_mut_in(all).iter_mut().zip(&heights) {
                        *old = (*old * old_range + (new - base) * size.y) / size.y;
                    }
                    data.clamp_heights_in(all);
                    data.generation = None;
                    data
                }
                _ => {
                    let mut data = TerrainData::flat(size_x, size_z, 0.0).ok()?;
                    let all = data.bounds();
                    data.heights_mut_in(all).copy_from_slice(&heights);
                    data.clamp_heights_in(all);
                    data.generation = Some(settings.to_json());
                    data
                }
            };
            data.clamp_heights_in(data.bounds());
            Some(JobResult {
                data,
                size: Some(size),
            })
        });
        self.terrain.job = Some(RunningJob {
            terrain,
            label: "Generating",
            title,
            job,
            erosion: false,
        });
    }

    /// Starts eroding the preview (if one is showing for this terrain) or
    /// the terrain itself, to preview.
    pub(super) fn start_erode(&mut self) {
        let Some(terrain) = self.terrain.target else {
            return;
        };
        if let Some(running) = &self.terrain.job {
            running.job.cancel();
            self.terrain.queued = Some(true);
            return;
        }
        let Some(placement) = self.world.get(terrain).and_then(Placement::of) else {
            return;
        };
        let kind = self.terrain.erosion;
        let what = match kind {
            ErosionKind::Hydraulic => "Rain Erosion",
            ErosionKind::Thermal => "Scree Erosion",
        };
        let (source, size, title, alone) = match &self.terrain.preview {
            Some(preview) if preview.terrain == terrain => (
                preview.data.clone(),
                preview.size,
                format!("{} with {what}", preview.label),
                preview.erosion,
            ),
            _ => {
                let Some(data) = self.world.terrain(terrain).cloned() else {
                    return;
                };
                (data, None, format!("Apply {what}"), true)
            }
        };
        let studs = size.map_or(placement.size, |s| Vec3::new(s.x, s.y, s.z));
        let samples = source.size();
        let scale = Scale {
            cell: Vec2::new(
                studs.x / (samples.0 - 1) as f32,
                studs.z / (samples.1 - 1) as f32,
            ),
            height: studs.y,
        };
        let mut hydraulic = self.terrain.hydraulic.clone();
        hydraulic.droplets = HydraulicSettings::droplets_for(
            samples.0 * samples.1,
            self.terrain.quality.droplets_per_sample(),
        );
        let thermal = self.terrain.thermal.clone();
        let job = Job::spawn(move |control| {
            let heights = match kind {
                ErosionKind::Hydraulic => {
                    erosion::hydraulic(source.heights(), samples, scale, &hydraulic, control)?
                }
                ErosionKind::Thermal => {
                    erosion::thermal(source.heights(), samples, scale, &thermal, control)?
                }
            };
            let mut data = (*source).clone();
            let all = data.bounds();
            data.heights_mut_in(all).copy_from_slice(&heights);
            data.clamp_heights_in(all);
            Some(JobResult { data, size })
        });
        self.terrain.job = Some(RunningJob {
            terrain,
            label: "Eroding",
            title,
            job,
            erosion: alone,
        });
    }

    /// A finished job becomes the preview; a job asked for while it was
    /// stopping starts.
    fn poll_terrain_job(&mut self, ctx: &egui::Context) {
        if let Some(running) = &mut self.terrain.job {
            match running.job.try_take() {
                None => {
                    // Still working: keep the progress bar moving.
                    ctx.request_repaint_after(Duration::from_millis(100));
                    return;
                }
                Some(result) => {
                    let running = self.terrain.job.take().expect("checked above");
                    if let Some(result) = result {
                        self.terrain.preview = Some(Preview {
                            terrain: running.terrain,
                            data: Arc::new(result.data),
                            size: result.size,
                            label: running.title,
                            erosion: running.erosion,
                        });
                    }
                    ctx.request_repaint();
                }
            }
        }
        match self.terrain.queued.take() {
            Some(true) => self.start_erode(),
            Some(false) => self.start_generate(),
            None => {}
        }
    }

    /// Puts the preview into the world, as one undo step, with layers that
    /// suit it if the terrain has none (and that was asked for).
    fn apply_preview(&mut self) {
        let Some(preview) = self.terrain.preview.take() else {
            return;
        };
        if self.world.get(preview.terrain).is_none() {
            return;
        }
        self.replace_terrain(
            preview.terrain,
            preview.label.clone(),
            &preview.data,
            preview.size,
        );
        let preset = preview
            .data
            .generation
            .as_deref()
            .and_then(|text| GeneratorSettings::from_json(text).ok())
            .map(|settings| settings.preset);
        if self.terrain.setup_materials
            && let Some(preset) = preset
            && materials::layers(&self.world, preview.terrain).is_empty()
        {
            let label = format!("Add {} Layers", preset.label());
            self.add_terrain_layers(preview.terrain, &layers_for(preset), label);
        }
    }

    /// Replaces `terrain`'s whole landscape (and its Size, if given) with
    /// `data`, as one undo step named `label`.
    fn replace_terrain(
        &mut self,
        terrain: InstanceId,
        label: String,
        data: &TerrainData,
        size: Option<Vector3>,
    ) {
        let Some(before) = self.world.terrain(terrain).cloned() else {
            return;
        };
        let fields_before = history::snapshot(&self.world, terrain);
        if let Some(size) = size
            && let Err(err) = self
                .world
                .set_property(terrain, "Size", Value::Vector3(size))
        {
            self.output
                .error(format!("Could not resize the terrain: {err}"));
            return;
        }
        if let Some(current) = self.world.terrain_mut(terrain) {
            current.replace_with(data);
        }
        let Some(after) = self.world.terrain(terrain).cloned() else {
            return;
        };
        let fields_after = history::snapshot(&self.world, terrain);
        self.history.record_terrain(
            &mut self.world,
            terrain,
            label,
            TerrainChange::Replace { before, after },
            fields_before,
            fields_after,
        );
    }

    /// Gives the terrain `samples` samples along X (and as many along Z as
    /// keep the cells square), blending the landscape and paint across.
    fn resample_terrain(&mut self, samples: usize) {
        let Some(terrain) = self.terrain.target else {
            return;
        };
        let Some(size) = self
            .world
            .get(terrain)
            .and_then(Placement::of)
            .map(|placement| placement.size)
        else {
            return;
        };
        let Some(data) = self.world.terrain(terrain) else {
            return;
        };
        let samples_z = samples_along(samples, size.x, size.z);
        match data.resampled(samples, samples_z) {
            Ok(resampled) => self.replace_terrain(
                terrain,
                format!("Resample Terrain to {samples} Samples"),
                &resampled,
                None,
            ),
            Err(err) => self
                .output
                .error(format!("Could not resample the terrain: {err}")),
        }
    }

    /// Picks a PNG and makes it the terrain's heights: its size becomes the
    /// terrain's resolution. Paint is cleared (it wouldn't line up).
    fn import_heightmap(&mut self) {
        let Some(terrain) = self.terrain.target else {
            return;
        };
        let Some(folder) = self.project.as_ref().map(|project| project.folder.clone()) else {
            return;
        };
        let Some(file) = rfd::FileDialog::new()
            .set_title("Import a heightmap")
            .add_filter("PNG image", &["png"])
            .set_directory(&folder)
            .pick_file()
        else {
            return;
        };
        let image = std::fs::read(&file)
            .map_err(|err| err.to_string())
            .and_then(|bytes| files::decode_image(&bytes));
        let data = image.and_then(|image| {
            let mut data =
                TerrainData::flat(image.width, image.height, 0.0).map_err(|err| err.to_string())?;
            let all = data.bounds();
            data.heights_mut_in(all).copy_from_slice(&image.values);
            data.clamp_heights_in(all);
            Ok(data)
        });
        match data {
            Ok(data) => {
                let label = format!(
                    "Import Heightmap {}",
                    file.file_name()
                        .map_or(String::new(), |n| n.to_string_lossy().into_owned())
                );
                self.replace_terrain(terrain, label, &data, None);
            }
            Err(err) => self
                .output
                .error(format!("Could not import `{}`: {err}", file.display())),
        }
    }

    /// Saves the terrain's heights as a 16-bit greyscale PNG.
    fn export_heightmap(&mut self) {
        let Some(terrain) = self.terrain.target else {
            return;
        };
        let Some(folder) = self.project.as_ref().map(|project| project.folder.clone()) else {
            return;
        };
        let Some(data) = self.world.terrain(terrain).cloned() else {
            return;
        };
        let name = self
            .world
            .get(terrain)
            .map_or("Terrain".to_owned(), |t| t.name().to_string());
        let Some(file) = rfd::FileDialog::new()
            .set_title("Export the heightmap")
            .add_filter("PNG image", &["png"])
            .set_directory(&folder)
            .set_file_name(format!("{name}-heights.png"))
            .save_file()
        else {
            return;
        };
        let (size_x, size_z) = data.size();
        let bytes = files::encode_heights(size_x, size_z, data.heights());
        match std::fs::write(&file, bytes) {
            Ok(()) => self.output.info(format!(
                "Exported {name}'s heights ({size_x} × {size_z}) to `{}`.",
                file.display()
            )),
            Err(err) => self
                .output
                .error(format!("Could not write `{}`: {err}", file.display())),
        }
    }

    /// Picks a greyscale PNG for the Stamp tool (copied into the project).
    fn pick_stamp_image(&mut self) {
        let Some(path) = self.pick_texture(STAMP_FOLDER) else {
            return;
        };
        self.terrain.stamp_image = Some(path);
        self.terrain.stamp.image = None;
        self.load_stamp_image();
    }

    /// Reads the Stamp tool's image. A bad one is reported and dropped.
    fn load_stamp_image(&mut self) {
        let (Some(folder), Some(path)) = (
            self.project.as_ref().map(|project| project.folder.clone()),
            self.terrain.stamp_image.clone(),
        ) else {
            return;
        };
        let image = std::fs::read(folder.join(&path))
            .map_err(|err| err.to_string())
            .and_then(|bytes| files::decode_image(&bytes));
        match image {
            Ok(image) => {
                self.terrain.stamp.image = Some(Arc::new(StampImage {
                    width: image.width,
                    height: image.height,
                    values: image.values,
                }));
            }
            Err(err) => {
                self.output
                    .error(format!("Could not use `{path}` as a stamp: {err}"));
                self.terrain.stamp_image = None;
                self.terrain.stamp.image = None;
            }
        }
    }

    /// Play starts: strokes, previews and jobs belong to editing.
    pub(super) fn put_terrain_tools_down(&mut self) {
        self.finish_stroke();
        self.terrain.preview = None;
        if let Some(running) = self.terrain.job.take() {
            running.job.cancel();
        }
        self.terrain.queued = None;
        self.terrain.changed_at = None;
        self.viewport.terrain.clear_overrides();
    }

    /// Another world was loaded: nothing under way belongs to it.
    pub(super) fn forget_terrain_work(&mut self) {
        self.terrain.stroke = None;
        self.put_terrain_tools_down();
        self.terrain.target = None;
        self.terrain.open_layer = None;
        self.viewport.terrain.clear();
    }

    /// Writes every terrain whose landscape changed since it was loaded or
    /// last saved. A terrain without a data folder, or sharing one (a
    /// duplicate), gets its own first. Returns false, with the reason in
    /// Output, if one couldn't be written.
    pub(super) fn save_terrains(&mut self) -> bool {
        let Some(project) = &mut self.project else {
            return false;
        };
        let folder = project.folder.clone();
        let mut used: Vec<String> = Vec::new();
        for id in crate::terrain_studio::terrains(&self.world)
            .into_iter()
            .chain(storage_terrains(&self.world))
        {
            let Some(data) = self.world.terrain(id).cloned() else {
                continue;
            };
            let mut data_folder = axcenity_project::data_folder(&self.world, id);
            let shared = data_folder
                .as_ref()
                .is_some_and(|f| used.iter().any(|u| u.eq_ignore_ascii_case(f)));
            if data_folder.is_none() || shared {
                let name = self
                    .world
                    .get(id)
                    .map_or("Terrain".to_owned(), |t| t.name().to_string());
                let mut taken = terrain_folders(&self.world);
                taken.extend(used.iter().cloned());
                let fresh = axcenity_project::new_terrain_folder(&folder, &name, &taken);
                // Part of the world being saved, not an edit to undo.
                let _ = self
                    .world
                    .set_property(id, "DataFolder", Value::String(fresh.clone()));
                data_folder = Some(fresh);
            }
            let data_folder = data_folder.expect("given one above");
            used.push(data_folder.clone());
            let saved = (data.uid(), data.revision());
            if project.saved_terrains.get(&id) == Some(&saved)
                && folder.join(&data_folder).exists()
                && !shared
            {
                continue;
            }
            if let Some(at) = project
                .damaged_terrains
                .iter()
                .position(|damaged| *damaged == data_folder)
            {
                match axcenity_project::keep_terrain_folder(&folder, &data_folder) {
                    Ok(Some(kept)) => self.output.info(format!(
                        "Kept the terrain files the editor couldn't load as `{kept}`."
                    )),
                    Ok(None) => {}
                    Err(err) => {
                        self.output
                            .error(format!("Could not save: {}", super::files::chain(&err)));
                        return false;
                    }
                }
                project.damaged_terrains.remove(at);
            }
            if let Err(err) = axcenity_project::save_terrain(&folder, &data_folder, &data) {
                self.output
                    .error(format!("Could not save: {}", super::files::chain(&err)));
                return false;
            }
            project.saved_terrains.insert(id, saved);
        }
        true
    }
}

/// The samples along a side `length` studs long that keep cells square
/// with `samples` across `width` studs.
fn samples_along(samples: usize, width: f32, length: f32) -> usize {
    if width <= 0.0 || !width.is_finite() || !length.is_finite() {
        return samples;
    }
    let cells = (samples - 1) as f32 * length / width;
    (cells.round() as usize + 1).clamp(MIN_TERRAIN_SAMPLES, MAX_TERRAIN_SAMPLES)
}

/// Every Terrain's data folder, to keep new ones apart.
fn terrain_folders(world: &World) -> Vec<String> {
    crate::terrain_studio::terrains(world)
        .into_iter()
        .chain(storage_terrains(world))
        .filter_map(|id| axcenity_project::data_folder(world, id))
        .collect()
}

/// Terrains kept in ReplicatedStorage (saved, but not drawn).
fn storage_terrains(world: &World) -> Vec<InstanceId> {
    world
        .descendants(world.replicated_storage())
        .into_iter()
        .filter(|&id| world.get(id).map(|i| i.class()) == Some(ClassName::Terrain))
        .collect()
}

/// The saved landscapes of `world`'s terrains, to tell later changes apart.
pub(super) fn terrain_revisions(world: &World) -> HashMap<InstanceId, (u64, u64)> {
    crate::terrain_studio::terrains(world)
        .into_iter()
        .chain(storage_terrains(world))
        .filter_map(|id| {
            let data = world.terrain(id)?;
            Some((id, (data.uid(), data.revision())))
        })
        .collect()
}

/// A new TerrainLayer like `preset`, outside the tree, named so no child
/// of `terrain` has its name.
fn new_layer(
    world: &mut World,
    terrain: InstanceId,
    preset: &LayerPreset,
    height: f32,
) -> Option<InstanceId> {
    let layer = world.create(ClassName::TerrainLayer).ok()?;
    for (property, value) in preset.properties(height) {
        world
            .set_property(layer, property, value)
            .expect("presets set TerrainLayer properties");
    }
    let name = tree::unique_name(world, terrain, preset.name);
    world.set_name(layer, name).expect("just made");
    Some(layer)
}

/// The property edits that put the texture at `path` in a layer's maps.
fn layer_map_edits(roles: &[axcenity_assets::TextureRole], path: &str) -> Vec<Edit> {
    use axcenity_assets::MapSlot;
    let file = || Value::String(path.to_owned());
    let channel = |role: &axcenity_assets::TextureRole| {
        Value::TextureChannel(match role.channel {
            axcenity_assets::Channel::Red => TextureChannel::Red,
            axcenity_assets::Channel::Green => TextureChannel::Green,
            axcenity_assets::Channel::Blue => TextureChannel::Blue,
            axcenity_assets::Channel::Alpha => TextureChannel::Alpha,
        })
    };
    let mut edits = Vec::new();
    for role in roles {
        match role.slot {
            MapSlot::BaseColor => {
                edits.push(Edit::Property("ColorMap", file()));
                // The map is the color: no tint over it.
                edits.push(Edit::Property(
                    "Color",
                    Value::Color3(Color3::new(1.0, 1.0, 1.0)),
                ));
            }
            MapSlot::Normal => {
                edits.push(Edit::Property("NormalMap", file()));
                edits.push(Edit::Property(
                    "NormalMapDirectX",
                    Value::Bool(role.flip_green),
                ));
            }
            MapSlot::Roughness => {
                edits.push(Edit::Property("RoughnessMap", file()));
                edits.push(Edit::Property("RoughnessChannel", channel(role)));
                edits.push(Edit::Property("InvertRoughness", Value::Bool(role.invert)));
            }
            MapSlot::Metalness => {
                edits.push(Edit::Property("MetalnessMap", file()));
                edits.push(Edit::Property("MetalnessChannel", channel(role)));
            }
            MapSlot::Occlusion => {
                edits.push(Edit::Property("AmbientOcclusionMap", file()));
                edits.push(Edit::Property("AmbientOcclusionChannel", channel(role)));
            }
            MapSlot::Height => edits.push(Edit::Property("HeightMap", file())),
            // Terrain layers have no glow, see-through or specular maps.
            MapSlot::Emissive | MapSlot::Opacity | MapSlot::Specular => {}
        }
    }
    edits
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app_with_project(name: &str) -> (EditorApp, PathBuf) {
        let parent = std::env::temp_dir().join(format!(
            "axcenity-editor-terrain-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&parent);
        std::fs::create_dir_all(&parent).unwrap();
        let folder = axcenity_project::create_project(&parent, "Terrain Test").unwrap();
        let app = EditorApp::new(Some(&folder));
        assert!(app.project.is_some(), "the project opens");
        (app, parent)
    }

    fn create(app: &mut EditorApp) -> InstanceId {
        app.terrain_action(TerrainAction::Create {
            width: 128.0,
            length: 64.0,
            height: 32.0,
            samples: 65,
        });
        app.terrain.target.expect("the new terrain is the target")
    }

    fn pointer(point: Vec3, pressed: bool, down: bool, released: bool) -> TerrainPointer {
        TerrainPointer {
            point: Some(point),
            pressed,
            down,
            released,
            seconds: 1.0 / 60.0,
            ..Default::default()
        }
    }

    #[test]
    fn a_new_terrain_is_one_undo_step_with_square_cells_and_starter_layers() {
        let (mut app, folder) = app_with_project("create");
        let terrain = create(&mut app);
        assert_eq!(app.world.terrain(terrain).unwrap().size(), (65, 33));
        assert_eq!(materials::layers(&app.world, terrain).len(), 3);
        assert_eq!(app.selected, Some(terrain));
        assert_eq!(
            axcenity_project::data_folder(&app.world, terrain).as_deref(),
            Some("assets/terrain/Terrain")
        );
        // The flat ground sits at Y = 0.
        let placement = Placement::of(app.world.get(terrain).unwrap()).unwrap();
        let data = app.world.terrain(terrain).unwrap();
        assert!(placement.ground_y(data, 0.0, 0.0).unwrap().abs() < 1e-4);
        app.undo();
        assert!(crate::terrain_studio::terrains(&app.world).is_empty());
        app.redo();
        assert_eq!(crate::terrain_studio::terrains(&app.world), vec![terrain]);
        std::fs::remove_dir_all(folder).unwrap();
    }

    #[test]
    fn a_stroke_raises_the_ground_and_undoes_as_one_step() {
        let (mut app, folder) = app_with_project("stroke");
        let terrain = create(&mut app);
        app.terrain.tool = Some(TerrainTool::Raise);
        app.terrain.brush.radius = 8.0;
        let ground = |app: &EditorApp, x: f32| {
            let placement = Placement::of(app.world.get(terrain).unwrap()).unwrap();
            placement
                .ground_y(app.world.terrain(terrain).unwrap(), x, 0.0)
                .unwrap()
        };
        app.terrain_pointer(pointer(Vec3::new(-20.0, 0.0, 0.0), true, true, false));
        for step in 1..=20 {
            let x = -20.0 + step as f32 * 2.0;
            app.terrain_pointer(pointer(Vec3::new(x, 0.0, 0.0), false, true, false));
        }
        app.terrain_pointer(pointer(Vec3::new(20.0, 0.0, 0.0), false, false, true));
        assert!(app.terrain.stroke.is_none());
        // Raised all along the path, with no gaps.
        for x in [-20.0, -10.0, 0.0, 10.0, 20.0] {
            assert!(ground(&app, x) > 0.2, "raised at {x}: {}", ground(&app, x));
        }
        assert!(ground(&app, 50.0).abs() < 1e-4, "far away stays");
        assert!(!app.history.is_saved());
        app.undo();
        assert!(
            ground(&app, 0.0).abs() < 1e-4,
            "one undo takes the stroke away"
        );
        app.redo();
        assert!(ground(&app, 0.0) > 0.2);
        // Shift lowers.
        app.terrain_pointer(TerrainPointer {
            modifiers: egui::Modifiers::SHIFT,
            ..pointer(Vec3::new(40.0, 0.0, 0.0), true, true, false)
        });
        app.terrain_pointer(pointer(Vec3::new(40.0, 0.0, 0.0), false, false, true));
        assert!(ground(&app, 40.0) < -0.01);
        std::fs::remove_dir_all(folder).unwrap();
    }

    #[test]
    fn painting_needs_a_layer_and_ctrl_click_picks_one() {
        let (mut app, folder) = app_with_project("paint");
        let terrain = create(&mut app);
        app.terrain.tool = Some(TerrainTool::Paint);
        app.terrain.paint_layer = 2;
        app.terrain.paint_mode = axcenity_terrain::brush::PaintMode::Replace;
        app.terrain.brush.radius = 6.0;
        app.terrain_pointer(pointer(Vec3::ZERO, true, true, false));
        app.terrain_pointer(pointer(Vec3::ZERO, false, false, true));
        let data = app.world.terrain(terrain).unwrap();
        let (x, z) = (data.size().0 / 2, data.size().1 / 2);
        assert!(data.paint()[data.index(x, z)][2] > 200);
        assert_eq!(app.history.undo_label(), Some("Paint Rock"));
        // Ctrl+click on the painted spot picks Rock again.
        app.terrain.paint_layer = 0;
        app.terrain_pointer(TerrainPointer {
            modifiers: egui::Modifiers::COMMAND,
            ..pointer(Vec3::ZERO, true, true, false)
        });
        assert_eq!(app.terrain.paint_layer, 2);
        std::fs::remove_dir_all(folder).unwrap();
    }

    #[test]
    fn a_generated_preview_changes_nothing_until_applied_then_saves_and_reloads() {
        let (mut app, folder) = app_with_project("generate");
        let terrain = create(&mut app);
        app.terrain.new_terrain.samples = 65;
        app.terrain.new_terrain.width = 128.0;
        app.terrain.new_terrain.length = 128.0;
        app.terrain.generator = Preset::MountainRange.settings();
        app.terrain.generator.seed = 7;
        app.start_generate();
        let ctx = egui::Context::default();
        while app.terrain.job.is_some() {
            std::thread::sleep(Duration::from_millis(5));
            app.poll_terrain_job(&ctx);
        }
        let preview = app.terrain.preview.as_ref().expect("a preview");
        assert_eq!(preview.data.size(), (65, 65));
        assert_eq!(
            app.world.terrain(terrain).unwrap().size(),
            (65, 33),
            "not applied yet"
        );
        let made = preview.data.heights().to_vec();

        app.terrain_action(TerrainAction::ApplyPreview);
        let data = app.world.terrain(terrain).unwrap().clone();
        assert_eq!(data.heights(), made.as_slice());
        let settings = GeneratorSettings::from_json(data.generation.as_deref().unwrap()).unwrap();
        assert_eq!(
            (settings.seed, settings.version),
            (7, generate::GENERATOR_VERSION)
        );

        assert!(app.save());
        let project = app.project.as_ref().unwrap().folder.clone();
        let reopened = EditorApp::new(Some(&project));
        let id = crate::terrain_studio::terrains(&reopened.world)[0];
        let loaded = reopened.world.terrain(id).unwrap();
        assert_eq!(loaded.size(), (65, 65));
        let worst = loaded
            .heights()
            .iter()
            .zip(&made)
            .map(|(a, b)| (a - b).abs())
            .fold(0.0, f32::max);
        assert!(worst < 1e-4, "saved to 16 bits: off by {worst}");

        // Undo puts the old landscape and size back.
        app.undo();
        assert_eq!(app.world.terrain(terrain).unwrap().size(), (65, 33));
        std::fs::remove_dir_all(folder).unwrap();
    }

    #[test]
    fn texture_names_fill_the_right_maps() {
        let roles = axcenity_assets::texture_roles("Rock_ORM.png");
        let edits = layer_map_edits(&roles, "assets/textures/Rock_ORM.png");
        assert!(edits.contains(&Edit::Property(
            "RoughnessChannel",
            Value::TextureChannel(TextureChannel::Green)
        )));
        assert!(edits.contains(&Edit::Property(
            "AmbientOcclusionMap",
            Value::String("assets/textures/Rock_ORM.png".to_owned())
        )));
    }

    #[test]
    fn layers_move_past_each_other() {
        let (mut app, folder) = app_with_project("layers");
        let terrain = create(&mut app);
        let names = |app: &EditorApp| -> Vec<String> {
            materials::layers(&app.world, terrain)
                .into_iter()
                .map(|id| app.world.get(id).unwrap().name().to_string())
                .collect()
        };
        assert_eq!(names(&app), ["Grass", "Dirt", "Rock"]);
        let rock = materials::layers(&app.world, terrain)[2];
        app.terrain_action(TerrainAction::MoveLayer(rock, true));
        assert_eq!(names(&app), ["Grass", "Rock", "Dirt"]);
        app.terrain_action(TerrainAction::MoveLayer(rock, false));
        assert_eq!(names(&app), ["Grass", "Dirt", "Rock"]);
        app.terrain_action(TerrainAction::AddLayer(
            crate::terrain_studio::LayerPreset {
                ..layers_for(Preset::Island)[0]
            },
        ));
        assert_eq!(names(&app).len(), 4);
        std::fs::remove_dir_all(folder).unwrap();
    }
}
