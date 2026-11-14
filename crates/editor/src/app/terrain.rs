//! The Terrain Studio's side of the app: carrying out the panel's actions
//! as undo steps, brush strokes from the viewport, and the background jobs
//! (generating, eroding) whose results are previewed until applied. These
//! are methods of [`super::EditorApp`]; the panel is `terrain_studio.rs`.
//!
//! A stroke changes the world as it goes (so the viewport shows it at
//! once) and becomes one undo step when the button is let go: only the
//! chunks it touched are kept. Whole new landscapes (generating, erosion,
//! importing, resampling) are one step holding the landscape before and after.

use std::collections::HashMap;
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

mod disk;
mod jobs;
mod layers;
mod scatter;
mod strokes;

pub(super) use disk::terrain_revisions;
use disk::*;
use layers::*;
use strokes::*;

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
        // Only an open layer has texture pickers, so only then are the
        // project's textures (and their thumbnails) looked up.
        let textures = if self.terrain.open_layer.is_some() {
            self.asset_browser.textures(ui.ctx())
        } else {
            Vec::new()
        };
        let pictures = self.layer_pictures(ui.ctx());
        let actions = self.terrain.show(
            ui,
            &StudioContext {
                world: &self.world,
                selected: self.selected,
                editing,
                time,
                textures: &textures,
                pictures: &pictures,
            },
        );
        for action in actions {
            self.terrain_action(action);
        }
    }

    /// Thumbnails of every terrain layer's color map, and of all the open
    /// layer's maps, by path: what the studio's layer list and open layer
    /// show.
    fn layer_pictures(&mut self, ctx: &egui::Context) -> HashMap<String, egui::TextureHandle> {
        let mut paths = Vec::new();
        for terrain in crate::terrain_studio::terrains(&self.world) {
            for layer in axcenity_terrain::materials::layers(&self.world, terrain) {
                let open = self.terrain.open_layer == Some(layer);
                let Some(instance) = self.world.get(layer) else {
                    continue;
                };
                for (map, _) in crate::terrain_studio::LAYER_MAPS {
                    if (open || map == "ColorMap")
                        && let Some(path) = instance.property(map).and_then(Value::as_str)
                        && !path.is_empty()
                    {
                        paths.push(path.to_owned());
                    }
                }
            }
        }
        paths
            .into_iter()
            .filter_map(|path| Some((path.clone(), self.asset_browser.thumbnail(ctx, &path)?)))
            .collect()
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

    /// Carries out one of the panel's actions.
    pub(super) fn terrain_action(&mut self, action: TerrainAction) {
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
            TerrainAction::SetLayer(layer, name, value)
            | TerrainAction::SetTerrain(layer, name, value) => {
                // A texture dropped on a map may be a global one.
                if let Value::String(path) = &value
                    && !self.use_global_asset(path)
                {
                    return;
                }
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
            TerrainAction::FromToolbox(layer) => {
                self.open_toolbox(Some(crate::toolbox_panel::Target::TerrainLayer(layer)));
            }
            TerrainAction::UseTextureSet(layer, entry) => {
                if !self.use_global_asset(&entry.path) {
                    return;
                }
                self.apply_texture_set(&entry, Some(layer));
            }
            TerrainAction::AutoLayerSettings(layer) => self.auto_layer_settings(layer),
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
            TerrainAction::AddScatterRules(presets, label) => {
                if let Some(terrain) = target {
                    self.add_scatter_rules(terrain, &presets, label);
                }
            }
            TerrainAction::BakeScatter => {
                if let Some(terrain) = target {
                    self.bake_scatter(terrain);
                }
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
        let size = Vector3::new(width, height, length);
        if let Some(id) = self.insert_terrain(size, data, Preset::RollingHills, None) {
            self.select_from_viewport(Some(id));
            self.terrain.target = Some(id);
            self.terrain.tool.get_or_insert(TerrainTool::Raise);
            self.layout.open(Pane::Terrain);
        }
    }

    /// Puts a new Terrain of `size` studs with landscape `data` at the end
    /// of the Workspace, with `preset`'s layers inside, as one undo step
    /// (`label`, or `Insert <name>`). Its flat ground (a quarter of the way
    /// up) sits at Y = 0.
    pub(in crate::app) fn insert_terrain(
        &mut self,
        size: Vector3,
        data: TerrainData,
        preset: Preset,
        label: Option<String>,
    ) -> Option<InstanceId> {
        let folder = self.project.as_ref()?.folder.clone();
        let id = self.world.create(ClassName::Terrain).ok()?;
        let workspace = self.world.workspace();
        let name = tree::unique_name(&self.world, workspace, "Terrain");
        let taken = terrain_folders(&self.world);
        let data_folder = axcenity_project::new_terrain_folder(&folder, &name, &taken);
        let properties = [
            ("Size", Value::Vector3(size)),
            (
                "Position",
                Value::Vector3(Vector3::new(0.0, -0.25 * size.y, 0.0)),
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
        for preset in layers_for(preset) {
            if let Some(layer) = new_layer(&mut self.world, id, &preset, size.y) {
                let _ = self.world.set_parent(layer, Some(id));
            }
        }
        let end = self.world.get(workspace).map_or(0, |w| w.children().len());
        let place = Place {
            parent: workspace,
            index: end,
        };
        let label = label.unwrap_or_else(|| format!("Insert {name}"));
        self.place(id, Some(place), label).then_some(id)
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
}

#[cfg(test)]
mod scatter_tests;
#[cfg(test)]
mod tests;
