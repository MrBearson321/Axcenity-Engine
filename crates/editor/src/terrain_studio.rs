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

use std::sync::Arc;

use axcenity_core::{ClassName, Color3, InstanceId, MAX_TERRAIN_LAYERS, TerrainData, Value, World};
use axcenity_render::TerrainView;
use axcenity_terrain::brush::{
    Brush, Curve, FlattenMode, PaintMode, SculptMode, Shape, Stamp, StampMode, StampShape, Symmetry,
};
use axcenity_terrain::erosion::{HydraulicSettings, ThermalSettings};
use axcenity_terrain::generate::{GeneratorSettings, Preset};
use axcenity_terrain::job::Job;
use eframe::egui;

use crate::theme::{self, icons};
use crate::widgets::{self, Role};

pub use presets::{LayerPreset, layers_for};

/// The resolutions offered for a terrain, in samples along a side.
pub const RESOLUTIONS: [usize; 5] = [129, 257, 513, 1025, 2049];

/// One of the Terrain Studio's tools.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TerrainTool {
    Raise,
    Smooth,
    Flatten,
    Sculpt,
    Stamp,
    Noise,
    Paint,
    Auto,
    Generate,
    Erode,
    Masks,
}

impl TerrainTool {
    pub const SCULPTING: [TerrainTool; 6] = [
        TerrainTool::Raise,
        TerrainTool::Smooth,
        TerrainTool::Flatten,
        TerrainTool::Sculpt,
        TerrainTool::Stamp,
        TerrainTool::Noise,
    ];
    pub const MATERIALS: [TerrainTool; 2] = [TerrainTool::Paint, TerrainTool::Auto];
    pub const WHOLE: [TerrainTool; 3] = [
        TerrainTool::Generate,
        TerrainTool::Erode,
        TerrainTool::Masks,
    ];

    pub fn label(self) -> &'static str {
        match self {
            TerrainTool::Raise => "Raise / Lower",
            TerrainTool::Smooth => "Smooth",
            TerrainTool::Flatten => "Flatten",
            TerrainTool::Sculpt => "Sculpt",
            TerrainTool::Stamp => "Stamp",
            TerrainTool::Noise => "Noise",
            TerrainTool::Paint => "Paint",
            TerrainTool::Auto => "Auto Materials",
            TerrainTool::Generate => "Generate",
            TerrainTool::Erode => "Erosion",
            TerrainTool::Masks => "Masks",
        }
    }

    pub fn icon(self) -> &'static str {
        match self {
            TerrainTool::Raise => icons::ARROWS_DOWN_UP,
            TerrainTool::Smooth => icons::WAVES,
            TerrainTool::Flatten => icons::EQUALS,
            TerrainTool::Sculpt => icons::MOUNTAINS,
            TerrainTool::Stamp => icons::STAMP,
            TerrainTool::Noise => icons::DOTS_NINE,
            TerrainTool::Paint => icons::PAINT_BRUSH,
            TerrainTool::Auto => icons::MAGIC_WAND,
            TerrainTool::Generate => icons::PLANET,
            TerrainTool::Erode => icons::CLOUD_RAIN,
            TerrainTool::Masks => icons::FUNNEL,
        }
    }

    /// Its key while the Terrain Studio has a tool: 1 to 9, then 0.
    pub fn key(self) -> Option<egui::Key> {
        Some(match self {
            TerrainTool::Raise => egui::Key::Num1,
            TerrainTool::Smooth => egui::Key::Num2,
            TerrainTool::Flatten => egui::Key::Num3,
            TerrainTool::Sculpt => egui::Key::Num4,
            TerrainTool::Stamp => egui::Key::Num5,
            TerrainTool::Noise => egui::Key::Num6,
            TerrainTool::Paint => egui::Key::Num7,
            TerrainTool::Auto => egui::Key::Num8,
            TerrainTool::Generate => egui::Key::Num9,
            TerrainTool::Erode => egui::Key::Num0,
            TerrainTool::Masks => return None,
        })
    }

    pub fn all() -> impl Iterator<Item = TerrainTool> {
        Self::SCULPTING
            .into_iter()
            .chain(Self::MATERIALS)
            .chain(Self::WHOLE)
    }

    /// True for the tools used by brushing the terrain in the viewport.
    pub fn is_brush(self) -> bool {
        Self::SCULPTING.contains(&self) || self == TerrainTool::Paint
    }

    /// One line on how to use it.
    pub fn hint(self) -> &'static str {
        match self {
            TerrainTool::Raise => "Drag to raise the ground. Hold Shift to lower it.",
            TerrainTool::Smooth => "Drag to even out bumps. Big shapes stay.",
            TerrainTool::Flatten => {
                "Drag to level the ground to the height where you start. Ctrl+click picks a height."
            }
            TerrainTool::Sculpt => "Drag to shape hills, ridges, valleys and cliffs.",
            TerrainTool::Stamp => "Click to stamp a shape. Shift stamps it upside down.",
            TerrainTool::Noise => "Drag to roughen the ground. Shift flips the bumps.",
            TerrainTool::Paint => {
                "Drag to paint the chosen layer. Shift erases back to the automatic materials. Ctrl+click picks a layer."
            }
            TerrainTool::Auto => "Lay layers down by height and slope. Painting still wins.",
            TerrainTool::Generate => "Make a landscape from a preset and a seed.",
            TerrainTool::Erode => "Wear the landscape down with rain or gravity.",
            TerrainTool::Masks => "Limit every brush to some heights or slopes.",
        }
    }
}

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
    /// Pick a texture file for a layer's map.
    PickLayerTexture(InstanceId, &'static str),
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
    /// Layers painting doesn't take from.
    pub locked: [bool; MAX_TERRAIN_LAYERS],
    /// The layer whose material is shown for editing.
    pub open_layer: Option<InstanceId>,
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
            locked: [false; MAX_TERRAIN_LAYERS],
            open_layer: None,
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
        ClassName::TerrainLayer => instance
            .parent()
            .filter(|&parent| world.get(parent).map(|p| p.class()) == Some(ClassName::Terrain)),
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
                        self.layers_section(ui, cx.world, terrain, false, &mut actions);
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
                    self.layers_section(ui, cx.world, terrain, painting, &mut actions);
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
        egui::Grid::new("terrain size")
            .num_columns(2)
            .spacing([10.0, 5.0])
            .show(ui, |ui| {
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

    /// The tool palette: three rows of icon buttons.
    fn palette(&mut self, ui: &mut egui::Ui) {
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
                for &tool in tools {
                    let tooltip = match tool.key() {
                        Some(key) => format!("{} ({})\n{}", tool.label(), key.name(), tool.hint()),
                        None => format!("{}\n{}", tool.label(), tool.hint()),
                    };
                    let picked = self.tool == Some(tool);
                    if widgets::icon_button(ui, tool.icon(), &tooltip, picked).clicked() {
                        self.tool = if picked { None } else { Some(tool) };
                    }
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
                ui.horizontal(|ui| {
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

    /// Size, strength and falloff, then the rest folded away. `extra` adds
    /// rows to the main grid.
    fn brush_settings(&mut self, ui: &mut egui::Ui, extra: Option<&mut dyn FnMut(&mut egui::Ui)>) {
        let brush = &mut self.brush;
        section(ui, "Brush", |ui| {
            egui::Grid::new("brush")
                .num_columns(2)
                .spacing([10.0, 5.0])
                .show(ui, |ui| {
                    field(ui, "Size").on_hover_text("The brush's radius. [ and ] change it.");
                    ui.add(
                        egui::Slider::new(&mut brush.radius, 0.5..=256.0)
                            .logarithmic(true)
                            .suffix(" studs"),
                    );
                    ui.end_row();
                    field(ui, "Strength");
                    ui.add(egui::Slider::new(&mut brush.strength, 0.01..=1.0));
                    ui.end_row();
                    field(ui, "Falloff")
                        .on_hover_text("How much of the brush fades toward its rim.");
                    ui.add(egui::Slider::new(&mut brush.falloff, 0.0..=1.0));
                    ui.end_row();
                    if let Some(extra) = extra {
                        extra(ui);
                    }
                });
            egui::CollapsingHeader::new("More brush settings")
                .id_salt("brush more")
                .default_open(false)
                .show(ui, |ui| {
                    egui::Grid::new("brush more grid")
                        .num_columns(2)
                        .spacing([10.0, 5.0])
                        .show(ui, |ui| {
                            field(ui, "Fade");
                            egui::ComboBox::from_id_salt("brush curve")
                                .selected_text(brush.curve.label())
                                .show_ui(ui, |ui| {
                                    for curve in Curve::ALL {
                                        ui.selectable_value(&mut brush.curve, curve, curve.label());
                                    }
                                });
                            ui.end_row();
                            field(ui, "Shape");
                            widgets::segmented(
                                ui,
                                &mut brush.shape,
                                &[
                                    (Shape::Circle, Some(icons::CIRCLE), "Circle"),
                                    (Shape::Square, Some(icons::SQUARE), "Square"),
                                ],
                            );
                            ui.end_row();
                            field(ui, "Spacing").on_hover_text(
                                "How far apart stamps are along a stroke, as a part of the size.",
                            );
                            ui.add(egui::Slider::new(&mut brush.spacing, 0.05..=1.0));
                            ui.end_row();
                            field(ui, "Symmetry")
                                .on_hover_text("Mirror every stamp across the terrain's middle.");
                            egui::ComboBox::from_id_salt("brush symmetry")
                                .selected_text(brush.symmetry.label())
                                .show_ui(ui, |ui| {
                                    for symmetry in Symmetry::ALL {
                                        ui.selectable_value(
                                            &mut brush.symmetry,
                                            symmetry,
                                            symmetry.label(),
                                        );
                                    }
                                });
                            ui.end_row();
                        });
                    masks_summary(ui, brush);
                });
        });
    }

    fn flatten_settings(&mut self, ui: &mut egui::Ui) {
        let mut mode = self.flatten_mode;
        let mut from_stroke = self.flatten_from_stroke;
        let mut height = self.flatten_height;
        self.brush_settings(
            ui,
            Some(&mut |ui: &mut egui::Ui| {
                field(ui, "Direction");
                egui::ComboBox::from_id_salt("flatten mode")
                    .selected_text(mode.label())
                    .show_ui(ui, |ui| {
                        for option in FlattenMode::ALL {
                            ui.selectable_value(&mut mode, option, option.label());
                        }
                    });
                ui.end_row();
                field(ui, "Height");
                ui.vertical(|ui| {
                    ui.checkbox(&mut from_stroke, "Where the stroke starts");
                    ui.add_enabled(
                        !from_stroke,
                        egui::DragValue::new(&mut height).speed(0.25).suffix(" Y"),
                    )
                    .on_hover_text(
                        "The world height to level to. Ctrl+click the ground to pick it.",
                    );
                });
                ui.end_row();
            }),
        );
        self.flatten_mode = mode;
        self.flatten_from_stroke = from_stroke;
        self.flatten_height = height;
    }

    fn sculpt_settings(&mut self, ui: &mut egui::Ui) {
        let mut mode = self.sculpt;
        let mut step = self.terrace_step;
        self.brush_settings(
            ui,
            Some(&mut |ui: &mut egui::Ui| {
                field(ui, "Shape");
                egui::ComboBox::from_id_salt("sculpt mode")
                    .selected_text(mode.label())
                    .show_ui(ui, |ui| {
                        for option in SculptMode::ALL {
                            ui.selectable_value(&mut mode, option, option.label());
                        }
                    });
                ui.end_row();
                if mode == SculptMode::Terrace {
                    field(ui, "Step");
                    ui.add(studs(&mut step, 0.5..=256.0));
                    ui.end_row();
                }
            }),
        );
        self.sculpt = mode;
        self.terrace_step = step;
    }

    fn stamp_settings(&mut self, ui: &mut egui::Ui, actions: &mut Vec<TerrainAction>) {
        let mut stamp = self.stamp.clone();
        let mut image = self.stamp_image.clone();
        let mut pick = false;
        self.brush_settings(
            ui,
            Some(&mut |ui: &mut egui::Ui| {
                field(ui, "Shape");
                ui.vertical(|ui| {
                    egui::ComboBox::from_id_salt("stamp shape")
                        .selected_text(if image.is_some() {
                            "Image"
                        } else {
                            stamp.shape.label()
                        })
                        .show_ui(ui, |ui| {
                            for shape in StampShape::ALL {
                                if ui
                                    .selectable_label(
                                        image.is_none() && stamp.shape == shape,
                                        shape.label(),
                                    )
                                    .clicked()
                                {
                                    stamp.shape = shape;
                                    image = None;
                                }
                            }
                        });
                    ui.horizontal(|ui| {
                        if ui
                            .small_button(theme::with_icon(icons::IMAGE, "Heightmap…"))
                            .on_hover_text("Stamp a greyscale PNG: white is high.")
                            .clicked()
                        {
                            pick = true;
                        }
                        if let Some(path) = &image {
                            ui.weak(path.rsplit('/').next().unwrap_or(path));
                        }
                    });
                });
                ui.end_row();
                field(ui, "Height");
                ui.add(studs(&mut stamp.height, -512.0..=512.0));
                ui.end_row();
                field(ui, "Turn");
                ui.add(egui::Slider::new(&mut stamp.rotation, -180.0..=180.0).suffix("°"));
                ui.end_row();
                field(ui, "Combine");
                egui::ComboBox::from_id_salt("stamp mode")
                    .selected_text(stamp.mode.label())
                    .show_ui(ui, |ui| {
                        for mode in StampMode::ALL {
                            ui.selectable_value(&mut stamp.mode, mode, mode.label());
                        }
                    });
                ui.end_row();
            }),
        );
        if image.is_none() {
            stamp.image = None;
        }
        self.stamp = stamp;
        self.stamp_image = image;
        if pick {
            actions.push(TerrainAction::PickStampImage);
        }
    }

    fn noise_settings(&mut self, ui: &mut egui::Ui) {
        let (mut amplitude, mut scale, mut seed) =
            (self.noise_amplitude, self.noise_scale, self.noise_seed);
        self.brush_settings(
            ui,
            Some(&mut |ui: &mut egui::Ui| {
                field(ui, "Bumps");
                ui.add(studs(&mut amplitude, 0.1..=128.0));
                ui.end_row();
                field(ui, "Bump size");
                ui.add(studs(&mut scale, 0.5..=512.0));
                ui.end_row();
                field(ui, "Seed");
                ui.horizontal(|ui| {
                    ui.add(egui::DragValue::new(&mut seed));
                    if ui
                        .small_button(icons::DICE_FIVE)
                        .on_hover_text("Another pattern")
                        .clicked()
                    {
                        seed = random_seed();
                    }
                });
                ui.end_row();
            }),
        );
        self.noise_amplitude = amplitude;
        self.noise_scale = scale;
        self.noise_seed = seed;
    }

    fn paint_settings(&mut self, ui: &mut egui::Ui) {
        let mut mode = self.paint_mode;
        self.brush_settings(
            ui,
            Some(&mut |ui: &mut egui::Ui| {
                field(ui, "Mode");
                widgets::segmented(
                    ui,
                    &mut mode,
                    &[
                        (PaintMode::Blend, None, "Blend"),
                        (PaintMode::Replace, None, "Replace"),
                    ],
                );
                ui.end_row();
            }),
        );
        self.paint_mode = mode;
    }

    fn mask_settings(&mut self, ui: &mut egui::Ui) {
        let brush = &mut self.brush;
        section(ui, "Masks", |ui| {
            ui.label(
                egui::RichText::new("Every brush only changes ground that passes these.")
                    .size(theme::type_scale::SMALL)
                    .color(theme::palette(ui.ctx()).text_weak),
            );
            let mut heights = brush.height_limits.is_some();
            let (mut low, mut high) = brush.height_limits.unwrap_or((0.0, 64.0));
            ui.checkbox(&mut heights, "Only between heights")
                .on_hover_text("Studs above the terrain's bottom. Raising stops at the top.");
            if heights {
                ui.horizontal(|ui| {
                    ui.add(egui::DragValue::new(&mut low).speed(0.5).suffix(" studs"));
                    ui.label("to");
                    ui.add(egui::DragValue::new(&mut high).speed(0.5).suffix(" studs"));
                });
            }
            brush.height_limits = heights.then_some((low.min(high), high.max(low)));
            let mut slopes = brush.slope_limits.is_some();
            let (mut low, mut high) = brush.slope_limits.unwrap_or((0.0, 30.0));
            ui.checkbox(&mut slopes, "Only between slopes")
                .on_hover_text("0° is flat, 90° a wall. Show the Slope view to see them.");
            if slopes {
                ui.horizontal(|ui| {
                    ui.add(egui::DragValue::new(&mut low).range(0.0..=90.0).suffix("°"));
                    ui.label("to");
                    ui.add(
                        egui::DragValue::new(&mut high)
                            .range(0.0..=90.0)
                            .suffix("°"),
                    );
                });
            }
            brush.slope_limits = slopes.then_some((low.min(high), high.max(low)));
        });
        masks_summary(ui, brush);
    }

    fn auto_settings(
        &mut self,
        ui: &mut egui::Ui,
        world: &World,
        terrain: InstanceId,
        actions: &mut Vec<TerrainAction>,
    ) {
        let palette = theme::palette(ui.ctx());
        section(ui, "Rules", |ui| {
            ui.label(
                egui::RichText::new(
                    "The first layer covers everything. Each later layer whose rule holds \
                     covers the layers above it in this list. Painting still wins, and \
                     erasing paint shows the rules again.",
                )
                .size(theme::type_scale::SMALL)
                .color(palette.text_weak),
            );
            let layers = axcenity_terrain::materials::layers(world, terrain);
            if layers.is_empty() {
                ui.weak("Add a layer below first.");
            }
            for (index, &layer) in layers.iter().enumerate() {
                let Some(instance) = world.get(layer) else {
                    continue;
                };
                ui.add_space(4.0);
                card(ui, |ui| {
                    rule_editor(ui, layer, instance, index == 0, actions);
                });
            }
        });
    }

    fn generate_settings(
        &mut self,
        ui: &mut egui::Ui,
        cx: &StudioContext,
        terrain: InstanceId,
        actions: &mut Vec<TerrainAction>,
    ) {
        let palette = theme::palette(ui.ctx());
        let before = self.generator.clone();
        let settings = &mut self.generator;
        section(ui, "Landscape", |ui| {
            egui::Grid::new("generate main")
                .num_columns(2)
                .spacing([10.0, 5.0])
                .show(ui, |ui| {
                    field(ui, "Preset");
                    let mut preset = settings.preset;
                    egui::ComboBox::from_id_salt("generator preset")
                        .selected_text(preset.label())
                        .show_ui(ui, |ui| {
                            for option in Preset::ALL {
                                ui.selectable_value(&mut preset, option, option.label())
                                    .on_hover_text(option.description());
                            }
                        });
                    if preset != settings.preset {
                        let seed = settings.seed;
                        *settings = GeneratorSettings {
                            seed,
                            ..preset.settings()
                        };
                    }
                    ui.end_row();
                    field(ui, "Seed");
                    ui.horizontal(|ui| {
                        ui.add(egui::DragValue::new(&mut settings.seed));
                        if ui
                            .small_button(icons::DICE_FIVE)
                            .on_hover_text("Randomize the seed: another landscape like it")
                            .clicked()
                        {
                            settings.seed = random_seed();
                        }
                    });
                    ui.end_row();
                    field(ui, "Height");
                    ui.add(egui::Slider::new(&mut settings.amplitude, 0.0..=1.0))
                        .on_hover_text(
                            "How much the land rises and falls, as part of the height range.",
                        );
                    ui.end_row();
                    field(ui, "Base elevation");
                    ui.add(egui::Slider::new(&mut settings.base_height, 0.0..=1.0));
                    ui.end_row();
                    field(ui, "Feature size");
                    ui.add(
                        egui::Slider::new(&mut settings.feature_size, 8.0..=4096.0)
                            .logarithmic(true)
                            .suffix(" studs"),
                    );
                    ui.end_row();
                    field(ui, "Mountains");
                    ui.add(egui::Slider::new(&mut settings.mountains, 0.0..=1.0));
                    ui.end_row();
                    field(ui, "Valleys");
                    ui.add(egui::Slider::new(&mut settings.valleys, 0.0..=1.0));
                    ui.end_row();
                    field(ui, "Roughness");
                    ui.add(egui::Slider::new(&mut settings.roughness, 0.0..=1.0));
                    ui.end_row();
                    field(ui, "Island");
                    ui.add(egui::Slider::new(&mut settings.island, 0.0..=1.0));
                    ui.end_row();
                    field(ui, "Plains");
                    ui.add(egui::Slider::new(&mut settings.plains, 0.0..=1.0))
                        .on_hover_text("Flattens the lowlands.");
                    ui.end_row();
                });
            egui::CollapsingHeader::new("Advanced")
                .id_salt("generate advanced")
                .default_open(false)
                .show(ui, |ui| {
                    egui::Grid::new("generate advanced grid")
                        .num_columns(2)
                        .spacing([10.0, 5.0])
                        .show(ui, |ui| {
                            field(ui, "Octaves");
                            ui.add(egui::Slider::new(&mut settings.octaves, 1..=10));
                            ui.end_row();
                            field(ui, "Persistence");
                            ui.add(egui::Slider::new(&mut settings.persistence, 0.1..=0.9));
                            ui.end_row();
                            field(ui, "Lacunarity");
                            ui.add(egui::Slider::new(&mut settings.lacunarity, 1.5..=3.5));
                            ui.end_row();
                            field(ui, "Warp");
                            ui.add(egui::Slider::new(&mut settings.warp, 0.0..=1.0))
                                .on_hover_text("Bends shapes so they swirl and branch.");
                            ui.end_row();
                            field(ui, "Height curve");
                            ui.add(egui::Slider::new(&mut settings.curve, 0.3..=3.0))
                                .on_hover_text("Above 1: broad valleys and steep peaks.");
                            ui.end_row();
                            field(ui, "Terraces");
                            ui.add(egui::Slider::new(&mut settings.terraces, 0..=24));
                            ui.end_row();
                            field(ui, "Terrace sharpness");
                            ui.add(egui::Slider::new(&mut settings.terrace_strength, 0.0..=1.0));
                            ui.end_row();
                            field(ui, "Dunes");
                            ui.add(egui::Slider::new(&mut settings.dunes, 0.0..=1.0));
                            ui.end_row();
                            field(ui, "Dune direction");
                            ui.add(
                                egui::Slider::new(&mut settings.dune_direction, 0.0..=180.0)
                                    .suffix("°"),
                            );
                            ui.end_row();
                            field(ui, "Crater");
                            ui.add(egui::Slider::new(&mut settings.crater, 0.0..=1.0));
                            ui.end_row();
                            field(ui, "Lowest");
                            ui.add(egui::Slider::new(&mut settings.min_height, 0.0..=1.0));
                            ui.end_row();
                            field(ui, "Highest");
                            ui.add(egui::Slider::new(&mut settings.max_height, 0.0..=1.0));
                            ui.end_row();
                        });
                });
        });
        // Changing anything but the seed makes the settings your own.
        let only_seed = GeneratorSettings {
            seed: self.generator.seed,
            ..before.clone()
        };
        if self.generator.preset == before.preset && self.generator != only_seed {
            self.generator.preset = Preset::Custom;
        }
        if self.generator != before {
            self.changed_at = Some(cx.time);
        }
        section(ui, "Size", |ui| {
            ui.label(
                egui::RichText::new("The terrain gets this size when the landscape is applied.")
                    .size(theme::type_scale::SMALL)
                    .color(palette.text_weak),
            );
            let size_before = self.new_terrain;
            self.size_controls(ui);
            if self.new_terrain != size_before {
                self.changed_at = Some(cx.time);
            }
            ui.horizontal(|ui| {
                if ui
                    .small_button("Use the terrain's size")
                    .on_hover_text("Keep the terrain's current size and resolution.")
                    .clicked()
                    && let (Some(instance), Some(data)) =
                        (cx.world.get(terrain), cx.world.terrain(terrain))
                    && let Some(size) = instance.property("Size").and_then(Value::as_vector3)
                {
                    self.new_terrain = NewTerrain {
                        width: size.x,
                        length: size.z,
                        height: size.y,
                        samples: data.size().0,
                    };
                    self.changed_at = Some(cx.time);
                }
            });
        });
        section(ui, "Apply", |ui| {
            ui.horizontal(|ui| {
                field(ui, "Result");
                if widgets::segmented(
                    ui,
                    &mut self.combine,
                    &[
                        (Combine::Replace, None, "Replaces the ground"),
                        (Combine::Add, None, "Adds to it"),
                    ],
                ) {
                    self.changed_at = Some(cx.time);
                }
            });
            ui.checkbox(
                &mut self.setup_materials,
                "Add matching layers if the terrain has none",
            )
            .on_hover_text(
                "Grass, rock, snow and so on, with rules that lay them down by themselves.",
            );
            ui.checkbox(
                &mut self.live_preview,
                "Update the preview as settings change",
            );
            ui.horizontal(|ui| {
                let label = if self.preview.is_some() {
                    "Regenerate"
                } else {
                    "Preview"
                };
                if ui
                    .add(widgets::button(
                        Role::Primary,
                        Some(icons::SPARKLE),
                        label,
                        &palette,
                    ))
                    .clicked()
                {
                    actions.push(TerrainAction::StartGenerate);
                }
                if ui
                    .add(widgets::button(
                        Role::Ghost,
                        Some(icons::ARROW_COUNTER_CLOCKWISE),
                        "Reset",
                        &palette,
                    ))
                    .on_hover_text("Back to the preset's own settings")
                    .clicked()
                {
                    let preset = match self.generator.preset {
                        Preset::Custom => Preset::RollingHills,
                        preset => preset,
                    };
                    self.generator = GeneratorSettings {
                        seed: self.generator.seed,
                        ..preset.settings()
                    };
                    self.changed_at = Some(cx.time);
                }
            });
            // The settings this terrain was last made with, to make it again.
            let saved = cx
                .world
                .terrain(terrain)
                .and_then(|data| data.generation.clone())
                .and_then(|text| GeneratorSettings::from_json(&text).ok());
            if let Some(saved) = saved {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(format!(
                            "Made with {} v{}, seed {}.",
                            saved.algorithm, saved.version, saved.seed
                        ))
                        .size(theme::type_scale::SMALL)
                        .color(palette.text_weak),
                    );
                    if ui
                        .small_button("Load its settings")
                        .on_hover_text("Make the same landscape again, or change it a little.")
                        .clicked()
                    {
                        self.generator = saved;
                        self.changed_at = Some(cx.time);
                    }
                });
            }
        });
    }

    fn erosion_settings(&mut self, ui: &mut egui::Ui, actions: &mut Vec<TerrainAction>) {
        let palette = theme::palette(ui.ctx());
        section(ui, "Erosion", |ui| {
            widgets::segmented(
                ui,
                &mut self.erosion,
                &[
                    (ErosionKind::Hydraulic, Some(icons::DROP), "Rain"),
                    (ErosionKind::Thermal, Some(icons::MOUNTAINS), "Scree"),
                ],
            );
            ui.label(
                egui::RichText::new(match self.erosion {
                    ErosionKind::Hydraulic => {
                        "Raindrops run downhill, carving gullies and filling valley floors."
                    }
                    ErosionKind::Thermal => {
                        "Loose ground slides off slopes steeper than the talus angle."
                    }
                })
                .size(theme::type_scale::SMALL)
                .color(palette.text_weak),
            );
            egui::Grid::new("erosion")
                .num_columns(2)
                .spacing([10.0, 5.0])
                .show(ui, |ui| match self.erosion {
                    ErosionKind::Hydraulic => {
                        let settings = &mut self.hydraulic;
                        field(ui, "Quality");
                        widgets::segmented(
                            ui,
                            &mut self.quality,
                            &[
                                (Quality::Draft, None, "Draft"),
                                (Quality::Normal, None, "Normal"),
                                (Quality::High, None, "High"),
                            ],
                        );
                        ui.end_row();
                        field(ui, "Erosion");
                        ui.add(egui::Slider::new(&mut settings.erosion, 0.0..=1.0));
                        ui.end_row();
                        field(ui, "Deposition");
                        ui.add(egui::Slider::new(&mut settings.deposition, 0.0..=1.0));
                        ui.end_row();
                        field(ui, "Evaporation");
                        ui.add(egui::Slider::new(&mut settings.evaporation, 0.0..=0.2));
                        ui.end_row();
                        field(ui, "Capacity").on_hover_text("How much soil water carries.");
                        ui.add(egui::Slider::new(&mut settings.capacity, 0.5..=16.0));
                        ui.end_row();
                        field(ui, "Inertia");
                        ui.add(egui::Slider::new(&mut settings.inertia, 0.0..=0.5));
                        ui.end_row();
                        field(ui, "Radius");
                        ui.add(egui::Slider::new(&mut settings.radius, 1..=6).suffix(" samples"));
                        ui.end_row();
                        field(ui, "Drop lifetime");
                        ui.add(egui::Slider::new(&mut settings.lifetime, 8..=96).suffix(" steps"));
                        ui.end_row();
                        field(ui, "Seed");
                        ui.horizontal(|ui| {
                            ui.add(egui::DragValue::new(&mut settings.seed));
                            if ui.small_button(icons::DICE_FIVE).clicked() {
                                settings.seed = random_seed();
                            }
                        });
                        ui.end_row();
                    }
                    ErosionKind::Thermal => {
                        let settings = &mut self.thermal;
                        field(ui, "Talus angle");
                        ui.add(
                            egui::Slider::new(&mut settings.talus_angle, 5.0..=80.0).suffix("°"),
                        );
                        ui.end_row();
                        field(ui, "Strength");
                        ui.add(egui::Slider::new(&mut settings.strength, 0.05..=1.0));
                        ui.end_row();
                        field(ui, "Iterations");
                        ui.add(egui::Slider::new(&mut settings.iterations, 1..=400));
                        ui.end_row();
                    }
                });
            if ui
                .add(widgets::button(
                    Role::Primary,
                    Some(icons::CLOUD_RAIN),
                    "Preview erosion",
                    &palette,
                ))
                .on_hover_text("Runs on a copy: the terrain only changes when you apply it.")
                .clicked()
            {
                actions.push(TerrainAction::StartErode);
            }
        });
    }

    /// The terrain's layers, and the open one's material.
    fn layers_section(
        &mut self,
        ui: &mut egui::Ui,
        world: &World,
        terrain: InstanceId,
        painting: bool,
        actions: &mut Vec<TerrainAction>,
    ) {
        let palette = theme::palette(ui.ctx());
        let layers = axcenity_terrain::materials::layers(world, terrain);
        let all_layers: Vec<InstanceId> = world
            .get(terrain)
            .map(|t| t.children().to_vec())
            .unwrap_or_default()
            .into_iter()
            .filter(|&c| world.get(c).map(|i| i.class()) == Some(ClassName::TerrainLayer))
            .collect();
        if self.paint_layer >= layers.len() {
            self.paint_layer = 0;
        }
        if self.open_layer.is_some_and(|open| !layers.contains(&open)) {
            self.open_layer = None;
        }
        section(ui, "Layers", |ui| {
            if layers.is_empty() {
                ui.label(
                    egui::RichText::new(
                        "No layers: the ground is plain grey. Add one to give it a material.",
                    )
                    .size(theme::type_scale::SMALL)
                    .color(palette.text_weak),
                );
            }
            for (index, &layer) in layers.iter().enumerate() {
                let Some(instance) = world.get(layer) else {
                    continue;
                };
                let open = self.open_layer == Some(layer);
                let chosen = painting && self.paint_layer == index;
                ui.horizontal(|ui| {
                    let color = instance
                        .property("Color")
                        .and_then(Value::as_color3)
                        .unwrap_or(Color3::new(0.5, 0.5, 0.5));
                    let (rect, _) = ui.allocate_exact_size(egui::vec2(16.0, 16.0), egui::Sense::hover());
                    ui.painter().rect_filled(
                        rect,
                        4,
                        egui::Color32::from_rgb(
                            (color.r * 255.0) as u8,
                            (color.g * 255.0) as u8,
                            (color.b * 255.0) as u8,
                        ),
                    );
                    if instance.property("ColorMap").and_then(Value::as_str).is_some_and(|p| !p.is_empty()) {
                        ui.painter().text(
                            rect.center(),
                            egui::Align2::CENTER_CENTER,
                            icons::IMAGE,
                            egui::FontId::proportional(10.0),
                            egui::Color32::from_white_alpha(200),
                        );
                    }
                    let name = format!("{}. {}", index + 1, instance.name());
                    let row = ui.selectable_label(chosen || open, name);
                    if row.clicked() {
                        if painting {
                            self.paint_layer = index;
                        }
                        self.open_layer = if open { None } else { Some(layer) };
                    }
                    row.context_menu(|ui| {
                        if ui.button("Select in Explorer").clicked() {
                            actions.push(TerrainAction::Select(layer));
                            ui.close();
                        }
                        if ui.button("Move up").clicked() {
                            actions.push(TerrainAction::MoveLayer(layer, true));
                            ui.close();
                        }
                        if ui.button("Move down").clicked() {
                            actions.push(TerrainAction::MoveLayer(layer, false));
                            ui.close();
                        }
                        if ui.button("Delete").clicked() {
                            actions.push(TerrainAction::RemoveLayer(layer));
                            ui.close();
                        }
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if painting {
                            let locked = &mut self.locked[index];
                            let (icon, tip) = if *locked {
                                (icons::LOCK, "Locked: painting doesn't take this layer away. Click to unlock.")
                            } else {
                                (icons::LOCK_OPEN, "Lock this layer, so painting doesn't take it away.")
                            };
                            if widgets::icon_button(ui, icon, tip, *locked).clicked() {
                                *locked = !*locked;
                            }
                        }
                        if instance.property("AutoEnabled").and_then(Value::as_bool) == Some(true) {
                            ui.label(egui::RichText::new(icons::MAGIC_WAND).color(palette.text_weak))
                                .on_hover_text("Laid down by its rule");
                        }
                    });
                });
                if open {
                    card(ui, |ui| layer_editor(ui, layer, instance, actions));
                }
            }
            if all_layers.len() > MAX_TERRAIN_LAYERS {
                ui.colored_label(
                    palette.warning,
                    format!("Only the first {MAX_TERRAIN_LAYERS} layers are drawn."),
                );
            }
            ui.horizontal(|ui| {
                ui.menu_button(theme::with_icon(icons::PLUS, "Add layer"), |ui| {
                    for preset in presets::ALL {
                        if ui.button(preset.name).clicked() {
                            actions.push(TerrainAction::AddLayer(preset));
                            ui.close();
                        }
                    }
                    ui.separator();
                    ui.label(egui::RichText::new("A set for a landscape").color(palette.text_weak));
                    for preset in Preset::ALL {
                        if preset == Preset::Custom {
                            continue;
                        }
                        if ui.button(preset.label()).clicked() {
                            actions.push(TerrainAction::AddLayerSet(preset));
                            ui.close();
                        }
                    }
                });
            });
        });
    }

    fn view_section(&mut self, ui: &mut egui::Ui) {
        section(ui, "View", |ui| {
            widgets::segmented(
                ui,
                &mut self.view,
                &[
                    (TerrainView::Shaded, None, "Shaded"),
                    (TerrainView::Height, None, "Height"),
                    (TerrainView::Slope, None, "Slope"),
                ],
            );
            widgets::segmented(
                ui,
                &mut self.view,
                &[
                    (TerrainView::Weights, None, "Layers"),
                    (TerrainView::Grid, None, "Grid"),
                ],
            );
        });
    }

    /// Resolution, heightmaps and clearing paint, folded away.
    fn data_section(
        &mut self,
        ui: &mut egui::Ui,
        world: &World,
        terrain: InstanceId,
        actions: &mut Vec<TerrainAction>,
    ) {
        egui::CollapsingHeader::new(theme::with_icon(icons::GEAR, "Terrain data"))
            .id_salt("terrain data")
            .default_open(false)
            .show(ui, |ui| {
                let current = world.terrain(terrain).map_or(0, |data| data.size().0);
                ui.horizontal(|ui| {
                    field(ui, "Resolution");
                    let mut samples = current;
                    egui::ComboBox::from_id_salt("terrain resample")
                        .selected_text(format!("{samples} samples"))
                        .show_ui(ui, |ui| {
                            for option in RESOLUTIONS {
                                ui.selectable_value(&mut samples, option, format!("{option} samples"));
                            }
                        });
                    if samples != current {
                        actions.push(TerrainAction::Resample(samples));
                    }
                });
                ui.horizontal(|ui| {
                    if ui
                        .button(theme::with_icon(icons::DOWNLOAD, "Import heightmap…"))
                        .on_hover_text("A greyscale PNG (8 or 16-bit): black is the bottom of the height range, white the top.")
                        .clicked()
                    {
                        actions.push(TerrainAction::ImportHeightmap);
                    }
                    if ui
                        .button(theme::with_icon(icons::UPLOAD, "Export…"))
                        .on_hover_text("Save the heights as a 16-bit greyscale PNG.")
                        .clicked()
                    {
                        actions.push(TerrainAction::ExportHeightmap);
                    }
                });
                if ui
                    .button(theme::with_icon(icons::ERASER, "Clear painted materials"))
                    .on_hover_text("Takes away all painting, leaving the automatic rules. You can undo it.")
                    .clicked()
                {
                    actions.push(TerrainAction::ClearPaint);
                }
            });
    }
}

/// One layer's material: color, maps and how they tile.
fn layer_editor(
    ui: &mut egui::Ui,
    layer: InstanceId,
    instance: &axcenity_core::Instance,
    actions: &mut Vec<TerrainAction>,
) {
    let number = |name| {
        instance
            .property(name)
            .and_then(Value::as_number)
            .unwrap_or(0.0)
    };
    let flag = |name| instance.property(name).and_then(Value::as_bool) == Some(true);
    egui::Grid::new(("layer", layer))
        .num_columns(2)
        .spacing([10.0, 5.0])
        .show(ui, |ui| {
            field(ui, "Tint");
            if let Some(color) = instance.property("Color").and_then(Value::as_color3)
                && let Some(color) = crate::properties::color_control(ui, color)
            {
                actions.push(TerrainAction::SetLayer(
                    layer,
                    "Color",
                    Value::Color3(color),
                ));
            }
            ui.end_row();
            for (name, label) in [
                ("ColorMap", "Color"),
                ("NormalMap", "Normal"),
                ("RoughnessMap", "Roughness"),
                ("MetalnessMap", "Metalness"),
                ("AmbientOcclusionMap", "Occlusion"),
                ("HeightMap", "Height"),
            ] {
                field(ui, label);
                ui.horizontal(|ui| {
                    if ui
                        .small_button(icons::FOLDER_OPEN)
                        .on_hover_text("Choose a texture (PNG, JPEG, TGA or BMP)")
                        .clicked()
                    {
                        actions.push(TerrainAction::PickLayerTexture(layer, name));
                    }
                    let path = instance
                        .property(name)
                        .and_then(Value::as_str)
                        .unwrap_or("");
                    if path.is_empty() {
                        ui.weak("None");
                    } else {
                        if ui
                            .small_button(icons::X)
                            .on_hover_text("Use no map")
                            .clicked()
                        {
                            actions.push(TerrainAction::SetLayer(
                                layer,
                                name,
                                Value::String(String::new()),
                            ));
                        }
                        ui.label(path.rsplit('/').next().unwrap_or(path))
                            .on_hover_text(path);
                    }
                });
                ui.end_row();
            }
            let mut slider = |ui: &mut egui::Ui,
                              name: &'static str,
                              label: &str,
                              range: std::ops::RangeInclusive<f64>,
                              suffix: &str| {
                field(ui, label);
                let mut value = number(name);
                if ui
                    .add(egui::Slider::new(&mut value, range).suffix(suffix))
                    .changed()
                {
                    actions.push(TerrainAction::SetLayer(layer, name, Value::Number(value)));
                }
                ui.end_row();
            };
            slider(ui, "Roughness", "Roughness", 0.0..=1.0, "");
            slider(ui, "Metalness", "Metalness", 0.0..=1.0, "");
            slider(ui, "NormalStrength", "Normal strength", 0.0..=3.0, "");
            slider(ui, "TileSize", "Tile size", 0.25..=256.0, " studs");
            slider(ui, "TextureRotation", "Turn", -180.0..=180.0, "°");
            slider(ui, "AntiTiling", "Anti-tiling", 0.0..=1.0, "");
            slider(ui, "HeightBlend", "Height blend", 0.0..=1.0, "");
            field(ui, "Triplanar");
            let mut triplanar = flag("Triplanar");
            if ui
                .checkbox(&mut triplanar, "Project from the sides too")
                .on_hover_text("For cliffs and rock: textures aren't stretched on steep ground.")
                .changed()
            {
                actions.push(TerrainAction::SetLayer(
                    layer,
                    "Triplanar",
                    Value::Bool(triplanar),
                ));
            }
            ui.end_row();
        });
    ui.horizontal_wrapped(|ui| {
        if ui
            .button(theme::with_icon(icons::DOWNLOAD, "Import texture set…"))
            .on_hover_text(
                "Pick a material's textures together (BaseColor, Normal, Roughness, ORM…): \
                 each goes to its map by its name.",
            )
            .clicked()
        {
            actions.push(TerrainAction::ImportTextureSet(layer));
        }
        if ui
            .button(theme::with_icon(icons::IMAGE, "From SurfaceAppearance"))
            .on_hover_text("Copy the maps of the SurfaceAppearance selected in the Explorer.")
            .clicked()
        {
            actions.push(TerrainAction::CopySurfaceAppearance(layer));
        }
    });
}

/// "Covers the layers above where the height is … and the slope is …".
fn rule_editor(
    ui: &mut egui::Ui,
    layer: InstanceId,
    instance: &axcenity_core::Instance,
    first: bool,
    actions: &mut Vec<TerrainAction>,
) {
    let palette = theme::palette(ui.ctx());
    let number = |name| {
        instance
            .property(name)
            .and_then(Value::as_number)
            .unwrap_or(0.0)
    };
    let mut enabled = instance.property("AutoEnabled").and_then(Value::as_bool) == Some(true);
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(instance.name())
                .family(theme::medium_family())
                .color(palette.text_strong),
        );
        if first {
            ui.label(
                egui::RichText::new("covers everything underneath")
                    .size(theme::type_scale::SMALL)
                    .color(palette.text_weak),
            );
            return;
        }
        if ui.checkbox(&mut enabled, "automatic").changed() {
            actions.push(TerrainAction::SetLayer(
                layer,
                "AutoEnabled",
                Value::Bool(enabled),
            ));
        }
    });
    if first || !enabled {
        return;
    }
    let mut value = |ui: &mut egui::Ui,
                     name: &'static str,
                     speed: f64,
                     range: std::ops::RangeInclusive<f64>,
                     suffix: &str| {
        let mut current = number(name);
        if ui
            .add(
                egui::DragValue::new(&mut current)
                    .speed(speed)
                    .range(range)
                    .suffix(suffix),
            )
            .changed()
        {
            actions.push(TerrainAction::SetLayer(layer, name, Value::Number(current)));
        }
    };
    ui.horizontal_wrapped(|ui| {
        ui.label("where the height is");
        value(ui, "MinHeight", 0.5, -100000.0..=100000.0, "");
        ui.label("to");
        value(ui, "MaxHeight", 0.5, -100000.0..=100000.0, " studs,");
        ui.label("fading over");
        value(ui, "HeightFade", 0.25, 0.0..=10000.0, " studs");
    });
    ui.horizontal_wrapped(|ui| {
        ui.label("and the slope is");
        value(ui, "MinSlope", 0.5, 0.0..=90.0, "°");
        ui.label("to");
        value(ui, "MaxSlope", 0.5, 0.0..=90.0, "°,");
        ui.label("fading over");
        value(ui, "SlopeFade", 0.25, 0.0..=90.0, "°");
    });
    ui.horizontal_wrapped(|ui| {
        ui.label("covering");
        value(ui, "RuleStrength", 0.01, 0.0..=1.0, "");
        ui.label("with noise");
        value(ui, "RuleNoise", 0.01, 0.0..=1.0, "");
        ui.label("in patches");
        value(ui, "NoiseScale", 0.5, 0.1..=10000.0, " studs across");
    });
}

/// What the masks are set to, so a brush that does nothing explains why.
fn masks_summary(ui: &mut egui::Ui, brush: &Brush) {
    let mut parts = Vec::new();
    if let Some((low, high)) = brush.height_limits {
        parts.push(format!("heights {low:.0}–{high:.0} studs"));
    }
    if let Some((low, high)) = brush.slope_limits {
        parts.push(format!("slopes {low:.0}–{high:.0}°"));
    }
    if !parts.is_empty() {
        let palette = theme::palette(ui.ctx());
        ui.label(
            egui::RichText::new(theme::with_icon(
                icons::FUNNEL,
                &format!("Masked: only {}.", parts.join(" and ")),
            ))
            .size(theme::type_scale::SMALL)
            .color(palette.warning),
        );
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
mod tests {
    use super::*;

    #[test]
    fn the_target_follows_the_selection_and_falls_back_to_the_first_terrain() {
        let mut world = World::new();
        let make = |world: &mut World| {
            let id = world.create(ClassName::Terrain).unwrap();
            world.set_parent(id, Some(world.workspace())).unwrap();
            id
        };
        let a = make(&mut world);
        let b = make(&mut world);
        let layer = world.create(ClassName::TerrainLayer).unwrap();
        world.set_parent(layer, Some(b)).unwrap();
        let mut studio = TerrainStudio::default();
        assert_eq!(studio.resolve_target(&world, None), Some(a));
        // Selecting a layer edits its terrain, and it stays picked after.
        assert_eq!(studio.resolve_target(&world, Some(layer)), Some(b));
        assert_eq!(studio.resolve_target(&world, None), Some(b));
        world.destroy(b).unwrap();
        assert_eq!(studio.resolve_target(&world, None), Some(a));
    }

    #[test]
    fn only_sculpting_and_painting_tools_brush_the_viewport() {
        let brushes: Vec<TerrainTool> = TerrainTool::all().filter(|t| t.is_brush()).collect();
        assert_eq!(brushes.len(), 7);
        assert!(!TerrainTool::Generate.is_brush());
        let keys: Vec<_> = TerrainTool::all().filter_map(TerrainTool::key).collect();
        let mut unique = keys.clone();
        unique.dedup();
        assert_eq!(keys, unique);
    }

    #[test]
    fn the_panel_draws_with_and_without_a_terrain() {
        let ctx = crate::theme::test_context();
        let mut world = World::new();
        let mut studio = TerrainStudio::default();
        let draw = |studio: &mut TerrainStudio, world: &World| {
            let mut actions = Vec::new();
            let _ = ctx.run(Default::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    actions = studio.show(
                        ui,
                        &StudioContext {
                            world,
                            selected: None,
                            editing: true,
                            time: 0.0,
                        },
                    );
                });
            });
            actions
        };
        assert!(draw(&mut studio, &world).is_empty());
        let terrain = world.create(ClassName::Terrain).unwrap();
        world.set_parent(terrain, Some(world.workspace())).unwrap();
        let layer = world.create(ClassName::TerrainLayer).unwrap();
        world.set_parent(layer, Some(terrain)).unwrap();
        studio.open_layer = Some(layer);
        for tool in TerrainTool::all() {
            studio.tool = Some(tool);
            draw(&mut studio, &world);
        }
    }

    /// With `AXCENITY_SNAPSHOTS` set, saves the panel with each of a few
    /// tools picked, to look at.
    #[test]
    fn the_panel_snapshots_with_its_main_tools() {
        let Some(folder) = std::env::var_os("AXCENITY_SNAPSHOTS") else {
            return;
        };
        let mut world = World::new();
        let terrain = world.create(ClassName::Terrain).unwrap();
        world.set_parent(terrain, Some(world.workspace())).unwrap();
        for preset in layers_for(Preset::MountainRange) {
            let layer = world.create(ClassName::TerrainLayer).unwrap();
            for (name, value) in preset.properties(128.0) {
                world.set_property(layer, name, value).unwrap();
            }
            world.set_name(layer, preset.name).unwrap();
            world.set_parent(layer, Some(terrain)).unwrap();
        }
        for (tool, file) in [
            (None, "terrain-none.png"),
            (Some(TerrainTool::Raise), "terrain-raise.png"),
            (Some(TerrainTool::Paint), "terrain-paint.png"),
            (Some(TerrainTool::Generate), "terrain-generate.png"),
            (Some(TerrainTool::Erode), "terrain-erode.png"),
        ] {
            let mut studio = TerrainStudio {
                tool,
                ..TerrainStudio::default()
            };
            crate::snapshot::snapshot(
                &std::path::PathBuf::from(&folder).join(file),
                egui::vec2(340.0, 900.0),
                vec![Vec::new(); 3],
                |ctx| {
                    egui::SidePanel::right("terrain")
                        .default_width(320.0)
                        .show(ctx, |ui| {
                            studio.show(
                                ui,
                                &StudioContext {
                                    world: &world,
                                    selected: None,
                                    editing: true,
                                    time: 0.0,
                                },
                            );
                        });
                },
            );
        }
    }
}
