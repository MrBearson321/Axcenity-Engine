//! The Terrain Studio: a dockable panel for shaping and painting Terrains.
//!
//! At the top, the terrain being edited (or a way to make one). Then the
//! tool palette, in three rows: sculpting brushes (Raise / Lower, Smooth,
//! Flatten, Sculpt, Stamp, Noise), materials (Paint, Auto Materials) and
//! whole-terrain tools (Generate, Erosion, Masks). Under it, only the
//! settings of the tool picked, with the rarely needed ones folded away,
//! then the terrain's layers and how the viewport shows the terrain.
//!
//! With a brush picked, the viewport's left button sculpts or paints (see
//! `viewport.rs`), Shift does the opposite (lowers, erases), and Ctrl+click
//! samples (Flatten's height, Paint's layer). `[` and `]` change the size.
//! Escape puts the brush down.
//!
//! The panel doesn't change the world. [`TerrainStudio::show`] returns
//! [`TerrainAction`]s, and the app (`app/terrain.rs`) carries them out, as
//! undo steps. Generating and eroding run on another thread; their result
//! is previewed in the viewport (without touching the world) until Apply.

mod presets;

pub(crate) mod biomes;
mod layers;
pub(crate) mod scatter;
mod tool_settings;
mod water;
pub(crate) mod world_presets;

use layers::*;

use std::collections::HashMap;
use std::sync::Arc;

use axcenity_core::{
    ClassName, Color3, FoliagePlant, FoliageProp, FoliageSource, InstanceId, MAX_TERRAIN_LAYERS,
    TerrainData, Value, World,
};
use axcenity_render::TerrainView;
use axcenity_terrain::brush::{
    Brush, Curve, FlattenMode, PaintMode, SculptMode, Shape, Stamp, StampMode, StampShape, Symmetry,
};
use axcenity_terrain::erosion::{HydraulicSettings, ThermalSettings};
use axcenity_terrain::generate::{GeneratorSettings, Preset};
use axcenity_terrain::job::Job;
use eframe::egui;

use crate::asset_browser::{AssetEntry, AssetKind, AssetPayload, TexturePick};
use crate::responsive;
use crate::theme::{self, icons};
use crate::widgets::{self, Role};

pub use axcenity_editor_tools::terrain_tool::TerrainTool;
pub use biomes::{BiomePreset, biome_of};
pub use presets::{LayerPreset, layers_for};
pub use scatter::{RulePreset, scatter_rules};
pub use world_presets::built_in_world_presets;

/// A TerrainLayer's map properties, with what the studio calls them.
pub const LAYER_MAPS: [(&str, &str); 6] = [
    ("ColorMap", "Color"),
    ("NormalMap", "Normal"),
    ("RoughnessMap", "Roughness"),
    ("MetalnessMap", "Metalness"),
    ("AmbientOcclusionMap", "Occlusion"),
    ("HeightMap", "Height"),
];

/// From this panel width (in points), the tool palette names its tools
/// beside their icons instead of only on hover.
const CAPTIONS_WIDTH: f32 = 400.0;

/// The resolutions offered for a terrain, in samples along a side.
pub const RESOLUTIONS: [usize; 5] = [129, 257, 513, 1025, 2049];

/// How a generated landscape joins the terrain.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Combine {
    /// It becomes the terrain.
    #[default]
    Replace,
    /// Its hills and dips are added to the ground already there.
    Add,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ErosionKind {
    #[default]
    Hydraulic,
    Thermal,
}

/// How many raindrops hydraulic erosion runs, per sample.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Quality {
    Draft,
    #[default]
    Normal,
    High,
}

impl Quality {
    pub fn droplets_per_sample(self) -> f32 {
        match self {
            Quality::Draft => 0.25,
            Quality::Normal => 0.8,
            Quality::High => 2.0,
        }
    }
}

/// What the app should do, from the panel.
#[derive(Clone, Debug, PartialEq)]
pub enum TerrainAction {
    /// A new flat terrain this many studs across, tall and long, with
    /// `samples` samples along X.
    Create {
        width: f32,
        length: f32,
        height: f32,
        samples: usize,
    },
    /// Edit this terrain (and select it).
    Target(InstanceId),
    AddLayer(LayerPreset),
    /// Add the layers that suit a landscape preset.
    AddLayerSet(Preset),
    RemoveLayer(InstanceId),
    /// Move a layer up (true) or down the list.
    MoveLayer(InstanceId, bool),
    /// Change a layer's property, as an undo step.
    SetLayer(InstanceId, &'static str, Value),
    /// Change the terrain's own property (its water), as an undo step.
    SetTerrain(InstanceId, &'static str, Value),
    /// Pick a texture file for a layer's map.
    PickLayerTexture(InstanceId, &'static str),
    /// Fill every map of a layer from a texture set in the project.
    UseTextureSet(InstanceId, AssetEntry),
    /// Opens the Toolbox on Textures; the next texture got fills the layer.
    FromToolbox(InstanceId),
    /// Set a layer's roughness, metalness and normal strength to suit its maps.
    AutoLayerSettings(InstanceId),
    /// Pick several texture files, sorted into the layer's maps by name.
    ImportTextureSet(InstanceId),
    /// Copy the selected SurfaceAppearance's maps into the layer.
    CopySurfaceAppearance(InstanceId),
    Select(InstanceId),
    /// Start making a preview (generated or eroded).
    StartGenerate,
    StartErode,
    ApplyPreview,
    CancelPreview,
    CancelJob,
    /// Take away all hand-painted materials.
    ClearPaint,
    /// Add ScatterRules from these presets to the end, as one undo step
    /// with this label.
    AddScatterRules(Vec<RulePreset>, String),
    /// Generate the landscape of world preset N (of the context's list);
    /// applying the preview also sets up its objects and water.
    GenerateWorldPreset(usize),
    /// Replace the terrain's layers, biomes, rules and water with world
    /// preset N's, keeping its shape, as one undo step.
    ApplyWorldPreset(usize),
    /// Save the terrain's setup as a world preset with this name in the
    /// project's presets folder.
    SaveWorldPreset(String),
    /// Add a Biome from this preset (with its ground layer and rules), as
    /// one undo step.
    AddBiome(BiomePreset),
    /// Turn what the scatter rules placed into painted foliage and switch
    /// the rules off, as one undo step.
    BakeScatter,
    /// Change the terrain's resolution (samples along X).
    Resample(usize),
    ImportHeightmap,
    ExportHeightmap,
    PickStampImage,
}

/// Work running for the studio.
pub struct RunningJob {
    pub terrain: InstanceId,
    /// What it's doing, such as "Generating".
    pub label: &'static str,
    /// The undo step its result makes when applied, such as "Generate Island".
    pub title: String,
    pub job: Job<JobResult>,
    /// True for erosion alone (not on top of a generated preview).
    pub erosion: bool,
}

/// What a job made.
pub struct JobResult {
    pub data: TerrainData,
    /// A new Size for the terrain, if generating changed it.
    pub size: Option<axcenity_core::Vector3>,
}

/// A landscape shown in the viewport but not yet in the world.
pub struct Preview {
    pub terrain: InstanceId,
    pub data: Arc<TerrainData>,
    pub size: Option<axcenity_core::Vector3>,
    /// The undo step's name, such as "Generate Mountain Range".
    pub label: String,
    pub erosion: bool,
}

/// A stroke under way.
pub struct ActiveStroke {
    pub terrain: InstanceId,
    pub operation: axcenity_terrain::brush::Operation,
    pub stroke: axcenity_terrain::brush::Stroke,
    pub recorder: axcenity_terrain::brush::EditRecorder,
    pub label: String,
    /// Stamps happen once per click.
    pub once: bool,
}

/// The size of a new terrain.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NewTerrain {
    pub width: f32,
    pub length: f32,
    pub height: f32,
    pub samples: usize,
}

impl Default for NewTerrain {
    fn default() -> Self {
        Self {
            width: 512.0,
            length: 512.0,
            height: 128.0,
            samples: 257,
        }
    }
}

/// The Terrain Studio's settings and what's under way. The settings are
/// preferences: they stay when another project is opened.
pub struct TerrainStudio {
    /// The tool picked, or `None` when the terrain isn't being edited.
    pub tool: Option<TerrainTool>,
    /// The terrain being edited.
    pub target: Option<InstanceId>,
    pub brush: Brush,
    pub sculpt: SculptMode,
    pub terrace_step: f32,
    pub flatten_mode: FlattenMode,
    /// Flatten's height (world Y), used when not taken from the stroke.
    pub flatten_height: f32,
    pub flatten_from_stroke: bool,
    pub noise_amplitude: f32,
    pub noise_scale: f32,
    pub noise_seed: u64,
    pub stamp: Stamp,
    /// The custom stamp's image in the project, if one was picked.
    pub stamp_image: Option<String>,
    /// The layer Paint paints (an index into the terrain's layers).
    pub paint_layer: usize,
    pub paint_mode: PaintMode,
    /// The plant, prop, or model Foliage scatters (picked from Assets).
    pub foliage_source: FoliageSource,
    /// Instances per square stud at full strength.
    pub foliage_density: f32,
    /// Whether new strokes collide with parts.
    pub foliage_collide: bool,
    /// Layers painting doesn't take from.
    pub locked: [bool; MAX_TERRAIN_LAYERS],
    /// The layer whose material is shown for editing.
    pub open_layer: Option<InstanceId>,
    /// What the open layer's texture pickers are searching for.
    texture_search: String,
    pub view: TerrainView,
    pub generator: GeneratorSettings,
    pub new_terrain: NewTerrain,
    pub combine: Combine,
    pub setup_materials: bool,
    /// Make a new preview by itself when a setting changes.
    pub live_preview: bool,
    pub erosion: ErosionKind,
    pub hydraulic: HydraulicSettings,
    pub thermal: ThermalSettings,
    pub quality: Quality,
    pub job: Option<RunningJob>,
    /// A job asked for while another was still stopping.
    pub queued: Option<bool>,
    pub preview: Option<Preview>,
    /// True while "Compare" is held: the terrain is shown as it is.
    pub comparing: bool,
    /// When a generator setting last changed (egui seconds), to make a
    /// new preview once they stop changing.
    pub changed_at: Option<f64>,
    pub stroke: Option<ActiveStroke>,
    /// The world preset picked in the Generate tool (of the context's list).
    pub world_preset: usize,
    /// The world preset whose objects and water the preview's Apply puts
    /// in, if the preview was made from one.
    pub pending_world: Option<axcenity_project::WorldPreset>,
    /// The name Save as preset uses.
    pub preset_name: String,
}

impl Default for TerrainStudio {
    fn default() -> Self {
        Self {
            tool: None,
            target: None,
            brush: Brush::default(),
            sculpt: SculptMode::Hill,
            terrace_step: 4.0,
            flatten_mode: FlattenMode::Both,
            flatten_height: 0.0,
            flatten_from_stroke: true,
            noise_amplitude: 4.0,
            noise_scale: 12.0,
            noise_seed: 1,
            stamp: Stamp::default(),
            stamp_image: None,
            paint_layer: 0,
            paint_mode: PaintMode::Blend,
            foliage_source: FoliageSource::Prop(FoliageProp::Tree),
            foliage_density: 0.08,
            foliage_collide: false,
            locked: [false; MAX_TERRAIN_LAYERS],
            open_layer: None,
            texture_search: String::new(),
            view: TerrainView::Shaded,
            generator: Preset::RollingHills.settings(),
            new_terrain: NewTerrain::default(),
            combine: Combine::Replace,
            setup_materials: true,
            live_preview: true,
            erosion: ErosionKind::Hydraulic,
            hydraulic: HydraulicSettings::default(),
            thermal: ThermalSettings::default(),
            quality: Quality::Normal,
            job: None,
            queued: None,
            preview: None,
            comparing: false,
            changed_at: None,
            stroke: None,
            world_preset: 0,
            pending_world: None,
            preset_name: String::new(),
        }
    }
}

/// What the panel needs to know about the editor.
pub struct StudioContext<'a> {
    pub world: &'a World,
    pub selected: Option<InstanceId>,
    /// False during Play, or with no project open: nothing can be changed.
    pub editing: bool,
    /// The time (egui seconds), for previews that wait for settings to settle.
    pub time: f64,
    /// The project's texture sets and textures, for an open layer's
    /// pickers. Empty while no layer is open.
    pub textures: &'a [TexturePick],
    /// Thumbnails of the layers' maps by path, once read: every layer's
    /// color map, and all the open layer's maps.
    pub pictures: &'a HashMap<String, egui::TextureHandle>,
    /// The world presets the Generate tool offers: the built-in ones, then
    /// the project's own.
    pub world_presets: &'a [axcenity_project::WorldPreset],
}

/// Every Terrain in the Workspace, in tree order.
pub fn terrains(world: &World) -> Vec<InstanceId> {
    world
        .descendants(world.workspace())
        .into_iter()
        .filter(|&id| world.get(id).map(|i| i.class()) == Some(ClassName::Terrain))
        .collect()
}

/// The Terrain `id` is, or is inside (a selected TerrainLayer's terrain).
pub fn terrain_of(world: &World, id: InstanceId) -> Option<InstanceId> {
    let instance = world.get(id)?;
    match instance.class() {
        ClassName::Terrain => Some(id),
        ClassName::TerrainLayer | ClassName::ScatterRule | ClassName::Biome => {
            let parent = instance.parent()?;
            match world.get(parent)?.class() {
                ClassName::Terrain => Some(parent),
                // A rule inside a Biome.
                ClassName::Biome if instance.class() == ClassName::ScatterRule => {
                    terrain_of(world, parent)
                }
                _ => None,
            }
        }
        _ => None,
    }
}

impl TerrainStudio {
    /// The terrain to edit: the one picked, if it's still in the world, or
    /// the selected one, or the first.
    pub fn resolve_target(
        &mut self,
        world: &World,
        selected: Option<InstanceId>,
    ) -> Option<InstanceId> {
        let all = terrains(world);
        if let Some(from_selection) = selected.and_then(|id| terrain_of(world, id))
            && all.contains(&from_selection)
        {
            self.target = Some(from_selection);
        }
        if self.target.is_none_or(|target| !all.contains(&target)) {
            self.target = all.first().copied();
        }
        self.target
    }

    /// True while a brush is in the viewport's hands.
    pub fn brushing(&self) -> bool {
        self.tool.is_some_and(TerrainTool::is_brush)
            && self.target.is_some()
            && self.preview.is_none()
    }

    /// The brush's color in the viewport: the accent for sculpting, the
    /// layer's own color for painting.
    pub fn brush_color(&self, world: &World, accent: egui::Color32) -> egui::Color32 {
        if self.tool == Some(TerrainTool::Foliage) {
            let color = axcenity_scene::foliage_brush_color(&self.foliage_source);
            return egui::Color32::from_rgb(
                (color[0] * 255.0) as u8,
                (color[1] * 255.0) as u8,
                (color[2] * 255.0) as u8,
            );
        }
        if self.tool != Some(TerrainTool::Paint) {
            return accent;
        }
        let layer = self.target.and_then(|terrain| {
            axcenity_terrain::materials::layers(world, terrain)
                .get(self.paint_layer)
                .copied()
        });
        layer
            .and_then(|layer| world.get(layer)?.property("Color")?.as_color3())
            .map_or(accent, |c| {
                egui::Color32::from_rgb(
                    (c.r * 255.0) as u8,
                    (c.g * 255.0) as u8,
                    (c.b * 255.0) as u8,
                )
            })
    }

    /// Draws the panel and returns what was asked for.
    pub fn show(&mut self, ui: &mut egui::Ui, cx: &StudioContext) -> Vec<TerrainAction> {
        let mut actions = Vec::new();
        let target = self.resolve_target(cx.world, cx.selected);
        egui::ScrollArea::vertical()
            .auto_shrink(false)
            .show(ui, |ui| {
                ui.add_enabled_ui(cx.editing, |ui| {
                    self.header(ui, cx, target, &mut actions);
                    let Some(terrain) = target else {
                        self.empty_state(ui, &mut actions);
                        return;
                    };
                    ui.add_space(6.0);
                    self.palette(ui);
                    self.status(ui, &mut actions);
                    let Some(tool) = self.tool else {
                        hint(ui, "Pick a tool to start shaping the terrain.");
                        self.layers_section(ui, cx, terrain, false, &mut actions);
                        self.biomes_section(ui, cx, terrain, &mut actions);
                        self.scatter_section(ui, cx, terrain, &mut actions);
                        self.water_section(ui, cx.world, terrain, &mut actions);
                        self.view_section(ui);
                        return;
                    };
                    hint(ui, tool.hint());
                    match tool {
                        TerrainTool::Raise => self.brush_settings(ui, None),
                        TerrainTool::Smooth => self.brush_settings(ui, None),
                        TerrainTool::Flatten => self.flatten_settings(ui),
                        TerrainTool::Sculpt => self.sculpt_settings(ui),
                        TerrainTool::Stamp => self.stamp_settings(ui, &mut actions),
                        TerrainTool::Noise => self.noise_settings(ui),
                        TerrainTool::Paint => self.paint_settings(ui),
                        TerrainTool::Foliage => self.foliage_settings(ui),
                        TerrainTool::Auto => {
                            self.auto_settings(ui, cx.world, terrain, &mut actions)
                        }
                        TerrainTool::Generate => {
                            self.generate_settings(ui, cx, terrain, &mut actions)
                        }
                        TerrainTool::Erode => self.erosion_settings(ui, &mut actions),
                        TerrainTool::Masks => self.mask_settings(ui),
                    }
                    let painting = matches!(tool, TerrainTool::Paint | TerrainTool::Auto);
                    self.layers_section(ui, cx, terrain, painting, &mut actions);
                    self.biomes_section(ui, cx, terrain, &mut actions);
                    self.scatter_section(ui, cx, terrain, &mut actions);
                    self.water_section(ui, cx.world, terrain, &mut actions);
                    self.view_section(ui);
                    self.data_section(ui, cx.world, terrain, &mut actions);
                });
            });
        actions
    }

    fn header(
        &mut self,
        ui: &mut egui::Ui,
        cx: &StudioContext,
        target: Option<InstanceId>,
        actions: &mut Vec<TerrainAction>,
    ) {
        let palette = theme::palette(ui.ctx());
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new(theme::with_icon(icons::MOUNTAINS, "Terrain Studio"))
                    .family(theme::heading_family())
                    .color(palette.text_strong),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if self.tool.is_some()
                    && widgets::icon_button(ui, icons::X, "Put the tool down (Esc)", false)
                        .clicked()
                {
                    self.tool = None;
                }
            });
        });
        let all = terrains(cx.world);
        if all.is_empty() {
            return;
        }
        ui.horizontal(|ui| {
            let name = |id: InstanceId| cx.world.get(id).map_or("?", |i| i.name()).to_owned();
            let mut chosen = target;
            egui::ComboBox::from_id_salt("terrain target")
                .selected_text(target.map(name).unwrap_or_default())
                .width(ui.available_width() - 70.0)
                .show_ui(ui, |ui| {
                    for &id in &all {
                        ui.selectable_value(&mut chosen, Some(id), name(id));
                    }
                });
            if chosen != target
                && let Some(id) = chosen
            {
                self.target = Some(id);
                actions.push(TerrainAction::Target(id));
            }
            if ui
                .add(widgets::button(
                    Role::Ghost,
                    Some(icons::PLUS),
                    "New",
                    &palette,
                ))
                .on_hover_text("Make another terrain")
                .clicked()
            {
                let size = self.new_terrain;
                actions.push(TerrainAction::Create {
                    width: size.width,
                    length: size.length,
                    height: size.height,
                    samples: size.samples,
                });
            }
        });
        if let Some(target) = target
            && let (Some(instance), Some(data)) = (cx.world.get(target), cx.world.terrain(target))
            && let Some(size) = instance.property("Size").and_then(Value::as_vector3)
        {
            let (x, z) = data.size();
            ui.label(
                egui::RichText::new(format!(
                    "{} × {} studs, {} tall · {x} × {z} samples · {:.2} studs apart",
                    size.x,
                    size.z,
                    size.y,
                    size.x / (x - 1) as f32
                ))
                .size(theme::type_scale::SMALL)
                .color(palette.text_weak),
            );
        }
    }

    fn empty_state(&mut self, ui: &mut egui::Ui, actions: &mut Vec<TerrainAction>) {
        let palette = theme::palette(ui.ctx());
        ui.add_space(12.0);
        card(ui, |ui| {
            ui.label(
                egui::RichText::new("No terrain yet")
                    .family(theme::heading_family())
                    .color(palette.text_strong),
            );
            ui.label(
                egui::RichText::new(
                    "A terrain is ground you can sculpt, paint and generate: hills, \
                     mountains, islands. Pick a size to begin.",
                )
                .color(palette.text_secondary),
            );
            ui.add_space(6.0);
            self.size_controls(ui);
            ui.add_space(6.0);
            let size = self.new_terrain;
            if ui
                .add(widgets::button(
                    Role::Primary,
                    Some(icons::PLUS),
                    "Create terrain",
                    &palette,
                ))
                .clicked()
            {
                actions.push(TerrainAction::Create {
                    width: size.width,
                    length: size.length,
                    height: size.height,
                    samples: size.samples,
                });
                self.tool = Some(TerrainTool::Generate);
            }
        });
    }

    /// Width, length, height and resolution of a new (or regenerated) terrain.
    fn size_controls(&mut self, ui: &mut egui::Ui) {
        let size = &mut self.new_terrain;
        responsive::form(ui, "terrain size", |ui| {
            field(ui, "Width");
            ui.add(studs(&mut size.width, 16.0..=16384.0));
            ui.end_row();
            field(ui, "Length");
            ui.add(studs(&mut size.length, 16.0..=16384.0));
            ui.end_row();
            field(ui, "Height range");
            ui.add(studs(&mut size.height, 4.0..=4096.0));
            ui.end_row();
            field(ui, "Resolution");
            egui::ComboBox::from_id_salt("terrain resolution")
                .selected_text(format!("{0} × {0}", size.samples))
                .show_ui(ui, |ui| {
                    for samples in RESOLUTIONS {
                        ui.selectable_value(
                            &mut size.samples,
                            samples,
                            format!("{samples} × {samples}"),
                        );
                    }
                });
            ui.end_row();
        });
        let samples = size.samples * size.samples;
        // Heights (4 bytes) and paint (8), plus the copies undo keeps.
        let megabytes = (samples * 12) as f32 / (1024.0 * 1024.0);
        let spacing = size.width / (size.samples - 1) as f32;
        let palette = theme::palette(ui.ctx());
        let mut note = format!("{spacing:.2} studs between samples, about {megabytes:.1} MB.");
        if size.samples > 1025 {
            note.push_str(" Large: generating and eroding take a while.");
        }
        ui.label(
            egui::RichText::new(note)
                .size(theme::type_scale::SMALL)
                .color(if size.samples > 1025 {
                    palette.warning
                } else {
                    palette.text_weak
                }),
        );
    }

    /// The tool palette: three rows of icon buttons, with each tool's name
    /// beside its icon when the panel is wide enough for it.
    fn palette(&mut self, ui: &mut egui::Ui) {
        let captions = ui.available_width() >= CAPTIONS_WIDTH;
        let rows: [(&str, &[TerrainTool]); 3] = [
            ("Sculpt", &TerrainTool::SCULPTING),
            ("Materials", &TerrainTool::MATERIALS),
            ("Terrain", &TerrainTool::WHOLE),
        ];
        for (title, tools) in rows {
            ui.horizontal_wrapped(|ui| {
                ui.add_sized(
                    egui::vec2(64.0, ui.spacing().interact_size.y),
                    egui::Label::new(
                        egui::RichText::new(title)
                            .size(theme::type_scale::SMALL)
                            .color(theme::palette(ui.ctx()).text_weak),
                    ),
                );
                let mut buttons = |ui: &mut egui::Ui| {
                    for &tool in tools {
                        let tooltip = match tool.key() {
                            Some(key) => {
                                format!("{} ({})\n{}", tool.label(), key.name(), tool.hint())
                            }
                            None => format!("{}\n{}", tool.label(), tool.hint()),
                        };
                        let picked = self.tool == Some(tool);
                        let button = if captions {
                            let palette = theme::palette(ui.ctx());
                            ui.add(
                                widgets::button(
                                    Role::Ghost,
                                    Some(tool.icon()),
                                    tool.label(),
                                    &palette,
                                )
                                .selected(picked)
                                .frame(picked),
                            )
                            .on_hover_text(&tooltip)
                        } else {
                            widgets::icon_button(ui, tool.icon(), &tooltip, picked)
                        };
                        if button.clicked() {
                            self.tool = if picked { None } else { Some(tool) };
                        }
                    }
                };
                // Named buttons that wrap stay to the right of the row's title.
                if captions {
                    ui.horizontal_wrapped(buttons);
                } else {
                    buttons(ui);
                }
            });
        }
        if let Some(tool) = self.tool {
            let palette = theme::palette(ui.ctx());
            ui.label(
                egui::RichText::new(tool.label())
                    .family(theme::medium_family())
                    .color(palette.text_strong),
            );
        }
    }

    /// A running job's progress, and a waiting preview's Apply and Cancel.
    fn status(&mut self, ui: &mut egui::Ui, actions: &mut Vec<TerrainAction>) {
        let palette = theme::palette(ui.ctx());
        if let Some(job) = &self.job {
            card(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(format!("{}…", job.label));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .add(widgets::button(Role::Ghost, None, "Cancel", &palette))
                            .clicked()
                        {
                            actions.push(TerrainAction::CancelJob);
                        }
                    });
                });
                ui.add(egui::ProgressBar::new(job.job.progress()).show_percentage());
            });
        }
        let mut comparing = false;
        if let Some(preview) = &self.preview {
            card(ui, |ui| {
                ui.label(
                    egui::RichText::new(theme::with_icon(icons::EYE, "Previewing"))
                        .family(theme::medium_family())
                        .color(palette.accent),
                );
                ui.label(
                    egui::RichText::new(format!(
                        "{}. The terrain isn't changed until you apply it.",
                        preview.label
                    ))
                    .size(theme::type_scale::SMALL)
                    .color(palette.text_secondary),
                );
                responsive::toolbar(ui, |ui| {
                    if ui
                        .add(widgets::button(
                            Role::Primary,
                            Some(icons::CHECK),
                            "Apply",
                            &palette,
                        ))
                        .clicked()
                    {
                        actions.push(TerrainAction::ApplyPreview);
                    }
                    if ui
                        .add(widgets::button(Role::Secondary, None, "Cancel", &palette))
                        .clicked()
                    {
                        actions.push(TerrainAction::CancelPreview);
                    }
                    let compare = ui
                        .add(widgets::button(
                            Role::Ghost,
                            Some(icons::SCALES),
                            "Hold to compare",
                            &palette,
                        ))
                        .on_hover_text("Shows the terrain as it is now while held.");
                    comparing = compare.is_pointer_button_down_on();
                });
            });
        }
        self.comparing = comparing;
    }
}

/// A collapsible group, like the Properties panel's.
fn section(ui: &mut egui::Ui, title: &str, body: impl FnOnce(&mut egui::Ui)) {
    let palette = theme::palette(ui.ctx());
    ui.add_space(4.0);
    widgets::divider(ui);
    egui::CollapsingHeader::new(
        egui::RichText::new(title)
            .family(theme::medium_family())
            .color(palette.text_secondary),
    )
    .id_salt(("terrain section", title))
    .default_open(true)
    .icon(widgets::chevron)
    .show_unindented(ui, body);
}

/// A quiet box around related controls.
fn card(ui: &mut egui::Ui, body: impl FnOnce(&mut egui::Ui)) {
    let palette = theme::palette(ui.ctx());
    egui::Frame::new()
        .fill(palette.surface)
        .stroke(egui::Stroke::new(1.0_f32, palette.border))
        .corner_radius(6)
        .inner_margin(egui::Margin::same(8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            body(ui);
        });
    ui.add_space(4.0);
}

fn hint(ui: &mut egui::Ui, text: &str) {
    let palette = theme::palette(ui.ctx());
    ui.label(
        egui::RichText::new(text)
            .size(theme::type_scale::SMALL)
            .color(palette.text_weak),
    );
}

fn foliage_icon(source: &FoliageSource) -> &'static str {
    match source {
        FoliageSource::Plant(FoliagePlant::Grass) => icons::LEAF,
        FoliageSource::Plant(FoliagePlant::Bush) => icons::PLANT,
        FoliageSource::Plant(FoliagePlant::Flower) => icons::FLOWER,
        FoliageSource::Plant(FoliagePlant::Tree) | FoliageSource::Prop(FoliageProp::Tree) => {
            icons::TREE
        }
        FoliageSource::Prop(FoliageProp::Crate) => icons::PACKAGE,
        FoliageSource::Prop(FoliageProp::Lamp) => icons::LAMP,
        FoliageSource::Prop(FoliageProp::Table) => icons::TABLE,
        FoliageSource::Prop(FoliageProp::Floor) => icons::SQUARE,
        FoliageSource::Prop(FoliageProp::Wall) => icons::WALL,
        FoliageSource::Prop(FoliageProp::Stairs) => icons::STEPS,
        FoliageSource::Prop(FoliageProp::Ramp) => icons::TRIANGLE,
        FoliageSource::Model(_) => icons::CUBE_TRANSPARENT,
    }
}

/// A name in a settings grid's left column.
fn field(ui: &mut egui::Ui, name: &str) -> egui::Response {
    let palette = theme::palette(ui.ctx());
    ui.add(egui::Label::new(egui::RichText::new(name).color(palette.text_secondary)).extend())
}

/// A number of studs, dragged or typed.
fn studs(value: &mut f32, range: std::ops::RangeInclusive<f32>) -> egui::DragValue<'_> {
    egui::DragValue::new(value)
        .range(range)
        .speed(0.5)
        .max_decimals(2)
        .suffix(" studs")
}

/// A seed from the clock: different each time, any number allowed.
pub fn random_seed() -> u64 {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |time| time.as_nanos() as u64);
    axcenity_terrain::noise::mix(nanos) % 1_000_000
}

#[cfg(test)]
mod tests;
