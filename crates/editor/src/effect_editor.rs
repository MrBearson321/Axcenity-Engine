//! The Effect tab (roadmap R17.3, `docs/research/vfx-editor.md`): the
//! selected effect (a part, model, attachment or free emitter holding
//! ParticleEmitters) played alone in a preview of its own, beside a module
//! stack of each emitter's properties in the order a particle lives them
//! (D6). The stack edits the world through Properties' rows, so every edit
//! is an undo step; the preview copies the effect again as it changes,
//! keeping its particles while only values change. Counts and costs (D7)
//! come from the preview's own particles and renderer.

use std::time::Instant;

use axcenity_assets::Assets;
use axcenity_core::{ClassName, Color3, InstanceId, Value, World};
use axcenity_editor_tools::preview::{Canvas, add_floor};
use axcenity_render::glam::Vec3;
use axcenity_render::{Camera, FramePass};
use axcenity_scene::{Particles, part_box_edges};
use eframe::egui;
use eframe::egui_wgpu::RenderState;

use crate::properties::{self, Edit};
use crate::theme::{self, icons};
use crate::widgets;

/// The modules of an emitter's stack (D6): a title, an icon and the
/// properties it groups, in the order a particle lives them.
pub const MODULES: [(&str, &str, &[&str]); 5] = [
    (
        "Spawn",
        icons::PLUS_CIRCLE,
        &[
            "Enabled",
            "Rate",
            "Bursts",
            "Duration",
            "Lifetime",
            "Shape",
            "ShapeStyle",
            "ShapeInOut",
            "EmitOn",
            "EmitCount",
        ],
    ),
    (
        "Birth",
        icons::ARROW_FAT_UP,
        &[
            "Speed",
            "VelocityInheritance",
            "Direction",
            "SpreadAngle",
            "Size",
            "Color",
            "Transparency",
            "Rotation",
            "RotSpeed",
        ],
    ),
    (
        "Over life",
        icons::CHART_LINE,
        &[
            "SizeEnd",
            "SizeOverLife",
            "ColorEnd",
            "ColorOverLife",
            "TransparencyEnd",
            "TransparencyOverLife",
            "SpeedOverLife",
            "Acceleration",
            "Drag",
            "WindAffectsDrag",
        ],
    ),
    (
        "Collision",
        icons::ARROW_ELBOW_RIGHT_UP,
        &["Collides", "Bounce", "LifetimeLoss"],
    ),
    (
        "Render",
        icons::IMAGE,
        &[
            "ParticleType",
            "Texture",
            "FlipbookLayout",
            "FlipbookMode",
            "FlipbookFramerate",
            "FlipbookStartRandom",
            "LightEmission",
            "Brightness",
        ],
    ),
];

/// Preview speeds, for slow motion.
const SPEEDS: [f32; 4] = [0.1, 0.25, 0.5, 1.0];
/// The camera's field of view, degrees.
const FOV: f32 = 50.0;
/// The longest a preview step may be, so a stall doesn't burst particles.
const MAX_STEP: f32 = 0.1;

/// What the tab asks the app to do.
#[derive(Debug, PartialEq)]
pub enum EffectAction {
    /// Change one of an emitter's properties (one undo step).
    Edit(InstanceId, Edit),
    /// Select this Instance in the Explorer.
    Select(InstanceId),
    /// Save the effect at this root as an effect asset.
    Save(InstanceId),
}

/// What the preview shows of the world: the effect's root, its Instances
/// (so a change of shape is told from a change of values) and the world's
/// revision it was copied at.
#[derive(Clone, Debug, PartialEq)]
struct Copied {
    root: InstanceId,
    ids: Vec<(InstanceId, ClassName)>,
    revision: u64,
}

/// The tab's state (see the module docs).
pub struct EffectEditor {
    canvas: Canvas,
    copied: Option<Copied>,
    /// The effect copied alone, with a floor and lighting: what's drawn.
    shown: World,
    /// Each copied Instance in `shown`, in the order of `Copied::ids`.
    shown_ids: Vec<InstanceId>,
    particles: Particles,
    pub paused: bool,
    /// How fast the preview runs, from [`SPEEDS`].
    pub speed: f32,
    /// Starts the effect again every [`EffectEditor::loop_seconds`].
    pub looping: bool,
    /// Seconds of the effect played since it (re)started.
    pub clock: f32,
    pub floor: bool,
    /// Off: a dark backdrop, to judge glowing particles.
    pub sky: bool,
    middle: Vec3,
    reach: f32,
    yaw: f32,
    pitch: f32,
    zoom: f32,
    last_frame: Option<Instant>,
    /// The last CPU step's milliseconds, eased.
    pub step_ms: f32,
}

impl Default for EffectEditor {
    fn default() -> Self {
        Self {
            canvas: Canvas::default(),
            copied: None,
            shown: World::new(),
            shown_ids: Vec::new(),
            particles: Particles::new(),
            paused: false,
            speed: 1.0,
            looping: true,
            clock: 0.0,
            floor: true,
            sky: true,
            middle: Vec3::ZERO,
            reach: 4.0,
            yaw: 35.0,
            pitch: 15.0,
            zoom: 1.0,
            last_frame: None,
            step_ms: 0.0,
        }
    }
}

/// The effect that `selected` belongs to: the selection itself if it holds
/// a ParticleEmitter (a part, model, attachment or folder), or an emitter's
/// part (an Attachment's part for an emitter in one), or a sub-emitter's
/// parent's effect. A free emitter, in the Workspace, is its own effect. `None` when there's no emitter there.
pub fn effect_root(world: &World, selected: Option<InstanceId>) -> Option<InstanceId> {
    let id = selected?;
    let instance = world.get(id)?;
    let root = if instance.class() == ClassName::ParticleEmitter {
        let parent = instance.parent().and_then(|p| world.get(p).map(|i| (p, i)));
        match parent {
            // A sub-emitter (R17.4) belongs to its parent emitter's effect.
            Some((parent, holder)) if holder.class() == ClassName::ParticleEmitter => {
                return effect_root(world, Some(parent));
            }
            Some((part, holder)) if holder.class().is_base_part() => part,
            Some((attachment, holder)) if holder.class() == ClassName::Attachment => holder
                .parent()
                .filter(|&part| world.get(part).is_some_and(|i| i.class().is_base_part()))
                .unwrap_or(attachment),
            _ => id,
        }
    } else {
        id
    };
    (!emitters(world, root).is_empty()).then_some(root)
}

/// The ParticleEmitters of the effect at `root`: itself and everything
/// inside it, in tree order.
pub fn emitters(world: &World, root: InstanceId) -> Vec<InstanceId> {
    std::iter::once(root)
        .chain(world.descendants(root))
        .filter(|&id| world.get(id).map(|i| i.class()) == Some(ClassName::ParticleEmitter))
        .collect()
}

/// How long a loop of the effect at `root` is: its longest burst
/// `Duration` or particle `Lifetime`, plus a `Lifetime` to fade out, 1 to
/// 20 seconds.
pub fn loop_seconds(world: &World, root: InstanceId) -> f32 {
    let number = |id: InstanceId, name: &str| {
        world
            .get(id)
            .and_then(|i| i.property(name))
            .and_then(Value::as_number)
            .unwrap_or(0.0) as f32
    };
    let (mut duration, mut lifetime) = (0.0_f32, 0.0_f32);
    for id in emitters(world, root) {
        duration = duration.max(number(id, "Duration"));
        lifetime = lifetime.max(number(id, "Lifetime"));
    }
    (duration.max(lifetime) + lifetime).clamp(1.0, 20.0)
}

impl EffectEditor {
    /// The effect shown, as its root in the edited world.
    pub fn root(&self) -> Option<InstanceId> {
        self.copied.as_ref().map(|copied| copied.root)
    }

    /// Particles alive in the preview, on the CPU.
    #[cfg(test)]
    fn particle_count(&self) -> usize {
        self.particles.count()
    }

    /// Starts the effect again from nothing.
    pub fn restart(&mut self) {
        self.particles.clear();
        self.clock = 0.0;
    }

    /// Copies the effect at `root` of `world` (as of `revision`) into the
    /// preview: in full when it's another effect or its Instances changed,
    /// otherwise only its values, so particles keep flying while a slider
    /// moves. With `root` `None` the preview empties.
    pub fn follow(&mut self, world: &World, root: Option<InstanceId>, revision: u64) {
        let Some(root) = root.filter(|&root| world.get(root).is_some()) else {
            if self.copied.take().is_some() {
                self.shown = World::new();
                self.shown_ids.clear();
                self.restart();
            }
            return;
        };
        let ids: Vec<(InstanceId, ClassName)> = std::iter::once(root)
            .chain(world.descendants(root))
            .filter_map(|id| Some((id, world.get(id)?.class())))
            .collect();
        match &self.copied {
            Some(copied) if copied.root == root && copied.revision == revision => {}
            Some(copied) if copied.root == root && copied.ids == ids => {
                for (&(id, _), &shown) in ids.iter().zip(&self.shown_ids) {
                    let Some(instance) = world.get(id) else {
                        continue;
                    };
                    let values: Vec<(&'static str, Value)> = instance
                        .properties()
                        .map(|(name, value)| (name, value.clone()))
                        .collect();
                    for (name, value) in values {
                        let _ = self.shown.set_property(shown, name, value);
                    }
                }
                self.copied = Some(Copied {
                    root,
                    ids,
                    revision,
                });
            }
            _ => {
                self.copy(world, root);
                self.copied = Some(Copied {
                    root,
                    ids,
                    revision,
                });
            }
        }
    }

    /// Copies the effect at `root` alone into a new preview world, frames it
    /// and starts it again.
    fn copy(&mut self, world: &World, root: InstanceId) {
        let mut shown = World::new();
        let workspace = shown.workspace();
        let Ok(copy) = shown.copy_from(world, root) else {
            return;
        };
        let _ = shown.set_parent(copy, Some(workspace));
        self.shown_ids = std::iter::once(copy)
            .chain(shown.descendants(copy))
            .collect();
        // Its parts' box, or where its free emitters stand.
        let mut low = Vec3::splat(f32::INFINITY);
        let mut high = Vec3::splat(f32::NEG_INFINITY);
        for &id in &self.shown_ids {
            for (a, b) in part_box_edges(&shown, id).into_iter().flatten() {
                low = low.min(a).min(b);
                high = high.max(a).max(b);
            }
            if let Some(Value::Vector3(at)) = shown
                .get(id)
                .filter(|i| i.class() == ClassName::ParticleEmitter)
                .and_then(|i| i.property("Position"))
                && !shown
                    .get(id)
                    .and_then(|i| i.parent())
                    .and_then(|p| shown.get(p))
                    .is_some_and(|p| p.class().is_base_part() || p.class() == ClassName::Attachment)
            {
                let at = Vec3::new(at.x, at.y, at.z);
                low = low.min(at);
                high = high.max(at);
            }
        }
        if low.x > high.x {
            (low, high) = (Vec3::ZERO, Vec3::ZERO);
        }
        // Particles fly past the box: room for a few studs round it.
        self.middle = (low + high) / 2.0 + Vec3::Y * 2.0;
        self.reach = ((high - low).length() / 2.0 + 4.0).min(200.0);
        self.zoom = 1.0;
        if self.floor {
            let tile = (self.reach / 3.0).max(0.5);
            add_floor(
                &mut shown,
                Vec3::new(self.middle.x, 0.0, self.middle.z),
                low.y.min(self.middle.y - 2.0),
                tile,
            );
        }
        let lighting = shown.lighting();
        let backdrop = Value::Color3(Color3::new(0.05, 0.05, 0.07));
        let sky: [(&str, Value); 3] = if self.sky {
            [
                ("ClockTime", Value::Number(14.0)),
                ("SkyTopColor", Value::Color3(Color3::new(0.25, 0.5, 0.9))),
                (
                    "SkyHorizonColor",
                    Value::Color3(Color3::new(0.63, 0.8, 0.95)),
                ),
            ]
        } else {
            [
                ("ClockTime", Value::Number(0.0)),
                ("SkyTopColor", backdrop.clone()),
                ("SkyHorizonColor", backdrop),
            ]
        };
        for (name, value) in sky {
            let _ = shown.set_property(lighting, name, value);
        }
        self.shown = shown;
        self.restart();
    }

    /// Plays `seconds` of the effect (at the preview's speed, unless
    /// paused), starting it again at the end of each loop. Colliding
    /// particles land on the preview's floor.
    pub fn step(&mut self, assets: &Assets, seconds: f32, loop_seconds: f32) {
        if self.paused || self.copied.is_none() {
            return;
        }
        let seconds = (seconds * self.speed).min(MAX_STEP);
        if self.looping && self.clock + seconds > loop_seconds {
            self.restart();
        }
        let start = Instant::now();
        let shown = &self.shown;
        self.particles
            .update_against(shown, seconds, &mut |skip, paths| {
                crate::play::particle_surfaces(shown, assets, skip, paths)
            });
        let ms = start.elapsed().as_secs_f32() * 1000.0;
        self.step_ms = if self.step_ms == 0.0 {
            ms
        } else {
            self.step_ms * 0.9 + ms * 0.1
        };
        self.clock += seconds;
    }

    fn camera(&self) -> Camera {
        let (yaw, pitch) = (self.yaw.to_radians(), self.pitch.to_radians());
        let direction = Vec3::new(
            yaw.sin() * pitch.cos(),
            pitch.sin(),
            yaw.cos() * pitch.cos(),
        );
        let distance = self.reach * self.zoom / (FOV.to_radians() / 2.0).sin();
        Camera {
            position: self.middle + direction * distance,
            look_at: self.middle,
            fov_y_degrees: FOV,
        }
    }

    /// Draws the tab for the effect `selected` belongs to in `world` and
    /// returns what it asks of the app.
    pub fn ui(
        &mut self,
        ui: &mut egui::Ui,
        world: &World,
        assets: &Assets,
        selected: Option<InstanceId>,
        revision: u64,
        render_state: Option<&RenderState>,
    ) -> Option<EffectAction> {
        let root = effect_root(world, selected).or_else(|| {
            // Keeps showing the last effect while something else is
            // selected, if it's still there.
            self.root()
                .filter(|&root| !emitters(world, root).is_empty())
        });
        self.follow(world, root, revision);
        let Some(root) = root else {
            widgets::empty_state(
                ui,
                icons::SPARKLE,
                "No effect selected",
                "Select a ParticleEmitter, or the part or model holding one, to play it here \
                 and edit its modules.",
                None,
            );
            return None;
        };
        let now = Instant::now();
        let seconds = self
            .last_frame
            .map_or(0.0, |last| now.duration_since(last).as_secs_f32());
        self.last_frame = Some(now);
        let looped = loop_seconds(world, root);
        self.step(assets, seconds, looped);

        let mut action = None;
        let name = world.get(root).map_or("", |i| i.name()).to_owned();
        self.toolbar(ui, &name, looped, root, &mut action);
        let height = (ui.available_height() * 0.5).clamp(140.0, 420.0);
        let (rect, response) = ui.allocate_exact_size(
            egui::vec2(ui.available_width().max(1.0), height),
            egui::Sense::click_and_drag(),
        );
        if response.dragged_by(egui::PointerButton::Primary) {
            let delta = response.drag_delta();
            self.yaw -= delta.x * 0.4;
            self.pitch = (self.pitch + delta.y * 0.3).clamp(-85.0, 85.0);
        }
        if response.hovered() {
            let scroll = ui.input(|input| input.smooth_scroll_delta.y);
            self.zoom = (self.zoom * (1.0 - scroll * 0.002)).clamp(0.2, 10.0);
        }
        if response.double_clicked() {
            self.zoom = 1.0;
        }
        match render_state {
            Some(render_state) => {
                if let Some(compute) = self.canvas.compute_shaders() {
                    self.particles.set_gpu(compute);
                }
                let mut list = axcenity_scene::build_draw_list(&self.shown, assets, self.camera());
                self.particles.draw(&mut list, &self.shown, assets);
                self.canvas.paint(ui, rect, render_state, &list);
                if !self.paused {
                    ui.ctx().request_repaint();
                }
            }
            None => {
                ui.painter_at(rect)
                    .rect_filled(rect, 0.0, ui.visuals().extreme_bg_color);
            }
        }
        self.costs(ui);
        widgets::divider(ui);
        egui::ScrollArea::vertical()
            .auto_shrink(false)
            .show(ui, |ui| {
                for emitter in emitters(world, root) {
                    if let Some(made) = stack(ui, world, assets, emitter) {
                        action = Some(made);
                    }
                }
            });
        action
    }

    fn toolbar(
        &mut self,
        ui: &mut egui::Ui,
        name: &str,
        looped: f32,
        root: InstanceId,
        action: &mut Option<EffectAction>,
    ) {
        ui.horizontal_wrapped(|ui| {
            ui.label(egui::RichText::new(name).strong());
            let (icon, tip) = if self.paused {
                (icons::PLAY, "Play")
            } else {
                (icons::PAUSE, "Pause")
            };
            if widgets::icon_button(ui, icon, tip, false).clicked() {
                self.paused = !self.paused;
            }
            if widgets::icon_button(ui, icons::ARROW_COUNTER_CLOCKWISE, "Restart", false).clicked()
            {
                self.restart();
            }
            let tip = format!("Loop: start again every {looped:.1} s");
            if widgets::icon_button(ui, icons::REPEAT, &tip, self.looping).clicked() {
                self.looping = !self.looping;
            }
            let mut speed = self.speed;
            widgets::menu_button(ui, Some(icons::GAUGE), &format!("{speed}×"), |ui| {
                for choice in SPEEDS {
                    if ui
                        .selectable_label(speed == choice, format!("{choice}×"))
                        .clicked()
                    {
                        speed = choice;
                        ui.close();
                    }
                }
            });
            self.speed = speed;
            let (mut floor, mut sky) = (self.floor, self.sky);
            ui.checkbox(&mut floor, "Floor");
            ui.checkbox(&mut sky, "Sky");
            if (floor, sky) != (self.floor, self.sky) {
                (self.floor, self.sky) = (floor, sky);
                // Copied again with the new floor and backdrop.
                if let Some(copied) = &mut self.copied {
                    copied.ids.clear();
                }
            }
            if widgets::icon_button(
                ui,
                icons::FLOPPY_DISK,
                "Save as Effect: an effect asset in assets/effects, placed like a prefab",
                false,
            )
            .clicked()
            {
                *action = Some(EffectAction::Save(root));
            }
        });
    }

    /// The counts and costs line (D7).
    fn costs(&self, ui: &mut egui::Ui) {
        let gpu = self.particles.gpu_emitters();
        let pass = self
            .canvas
            .stats()
            .and_then(|stats| stats.pass_time(FramePass::Particles))
            .map_or("–".to_owned(), |time| {
                format!("{:.2} ms", time.as_secs_f64() * 1000.0)
            });
        let text = format!(
            "{} particles on the CPU · {} emitters on the GPU · CPU step {:.2} ms · GPU particles {}",
            self.particles.count(),
            gpu,
            self.step_ms,
            pass,
        );
        ui.label(
            egui::RichText::new(text)
                .small()
                .color(theme::palette(ui.ctx()).text_weak),
        )
        .on_hover_text(
            "What this effect costs in the preview: particles moved on the CPU, emitters big \
             enough to move on the GPU (Rate × Lifetime from 3,000), the CPU's step and the GPU's \
             particle pass (timed only where the graphics card has timestamps).",
        );
    }
}

/// One emitter's module stack: its name (click to select it) and a
/// collapsible group of rows per module.
fn stack(
    ui: &mut egui::Ui,
    world: &World,
    assets: &Assets,
    emitter: InstanceId,
) -> Option<EffectAction> {
    let instance = world.get(emitter)?;
    let mut action = None;
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(theme::class_icon(ClassName::ParticleEmitter))
                .color(theme::class_color(ClassName::ParticleEmitter, ui.visuals())),
        );
        if ui
            .link(egui::RichText::new(instance.name()).strong())
            .on_hover_text("Select it")
            .clicked()
        {
            action = Some(EffectAction::Select(emitter));
        }
    });
    for (title, icon, names) in MODULES {
        let salt = format!("effect {emitter:?} {title}");
        widgets::section(icon, title)
            .id_salt(&salt)
            .default_open(true)
            .show(ui, |ui| {
                if let Some(edit) = properties::property_rows(ui, instance, assets, names, &salt) {
                    action = Some(EffectAction::Edit(emitter, edit));
                }
            });
    }
    ui.add_space(6.0);
    action
}

#[cfg(test)]
mod tests {
    use axcenity_core::Vector3;

    use super::*;

    fn emitter_in(world: &mut World, parent: InstanceId) -> InstanceId {
        let emitter = world.create(ClassName::ParticleEmitter).unwrap();
        world.set_parent(emitter, Some(parent)).unwrap();
        emitter
    }

    #[test]
    fn every_emitter_property_is_in_one_module() {
        let mut names: Vec<&str> = MODULES
            .iter()
            .flat_map(|(_, _, names)| names.iter().copied())
            .collect();
        let count = names.len();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), count, "no property in two modules");
        for (name, _) in ClassName::ParticleEmitter.default_properties() {
            let elsewhere = ["Name", "Locked", "Position", "Orientation"];
            if elsewhere.contains(&name) {
                continue;
            }
            assert!(names.contains(&name), "{name} is in a module");
        }
        for name in names {
            assert!(
                ClassName::ParticleEmitter
                    .default_properties()
                    .iter()
                    .any(|(property, _)| *property == name),
                "{name} is an emitter property"
            );
        }
    }

    #[test]
    fn the_effect_is_the_part_holding_the_selected_emitter() {
        let mut world = World::new();
        let workspace = world.workspace();
        let model = world.create(ClassName::Model).unwrap();
        world.set_parent(model, Some(workspace)).unwrap();
        let part = world.create(ClassName::Part).unwrap();
        world.set_parent(part, Some(model)).unwrap();
        let in_part = emitter_in(&mut world, part);
        let attachment = world.create(ClassName::Attachment).unwrap();
        world.set_parent(attachment, Some(part)).unwrap();
        let in_attachment = emitter_in(&mut world, attachment);
        let free = emitter_in(&mut world, workspace);

        assert_eq!(effect_root(&world, Some(in_part)), Some(part));
        assert_eq!(effect_root(&world, Some(in_attachment)), Some(part));
        assert_eq!(effect_root(&world, Some(free)), Some(free));
        assert_eq!(effect_root(&world, Some(model)), Some(model));
        assert_eq!(emitters(&world, model), vec![in_part, in_attachment]);
        // A sub-emitter (R17.4) is in its parent emitter's effect.
        let sub = emitter_in(&mut world, in_part);
        let deeper = emitter_in(&mut world, free);
        assert_eq!(effect_root(&world, Some(sub)), Some(part));
        assert_eq!(effect_root(&world, Some(deeper)), Some(free));
        // A part with no emitter isn't an effect.
        let plain = world.create(ClassName::Part).unwrap();
        world.set_parent(plain, Some(workspace)).unwrap();
        assert_eq!(effect_root(&world, Some(plain)), None);
        assert_eq!(effect_root(&world, None), None);
    }

    #[test]
    fn a_loop_lasts_the_longest_burst_or_life_and_a_life_to_fade() {
        let mut world = World::new();
        let workspace = world.workspace();
        let part = world.create(ClassName::Part).unwrap();
        world.set_parent(part, Some(workspace)).unwrap();
        let a = emitter_in(&mut world, part);
        let b = emitter_in(&mut world, part);
        world
            .set_property(a, "Lifetime", Value::Number(1.5))
            .unwrap();
        world
            .set_property(b, "Duration", Value::Number(4.0))
            .unwrap();
        world
            .set_property(b, "Lifetime", Value::Number(0.5))
            .unwrap();
        // Duration 4 (longer than any life) plus the longest life, 1.5.
        assert_eq!(loop_seconds(&world, part), 5.5);
        world
            .set_property(b, "Duration", Value::Number(90.0))
            .unwrap();
        assert_eq!(loop_seconds(&world, part), 20.0);
    }

    /// A part with an emitter in the Workspace, and the world's revision.
    fn fountain() -> (World, InstanceId, InstanceId) {
        let mut world = World::new();
        let workspace = world.workspace();
        let part = world.create(ClassName::Part).unwrap();
        world
            .set_property(
                part,
                "Position",
                Value::Vector3(Vector3::new(30.0, 2.0, 0.0)),
            )
            .unwrap();
        world.set_parent(part, Some(workspace)).unwrap();
        let emitter = emitter_in(&mut world, part);
        (world, part, emitter)
    }

    #[test]
    fn the_preview_plays_a_copy_and_keeps_its_particles_while_values_change() {
        let (mut world, part, emitter) = fountain();
        let before = world.descendants(world.root()).len();
        let mut editor = EffectEditor::default();
        editor.follow(&world, effect_root(&world, Some(emitter)), 1);
        assert_eq!(editor.root(), Some(part));
        for _ in 0..30 {
            editor.step(&Assets::default(), 1.0 / 30.0, 10.0);
        }
        let alive = editor.particle_count();
        assert!(alive > 0, "it plays");
        assert_eq!(
            world.descendants(world.root()).len(),
            before,
            "the world is untouched"
        );
        // A value changes: the copy follows and the particles stay.
        world
            .set_property(emitter, "Rate", Value::Number(80.0))
            .unwrap();
        editor.follow(&world, Some(part), 2);
        assert!(editor.particle_count() >= alive / 2, "kept flying");
        let copied_rate = editor
            .shown
            .get(editor.shown_ids[1])
            .and_then(|i| i.property("Rate"))
            .cloned();
        assert_eq!(copied_rate, Some(Value::Number(80.0)));
        // An emitter added: copied again, from nothing.
        emitter_in(&mut world, part);
        editor.follow(&world, Some(part), 3);
        assert_eq!(editor.particle_count(), 0);
        assert_eq!(editor.shown_ids.len(), 3);
        // Nothing selected and the effect gone: the preview empties.
        editor.follow(&world, None, 4);
        assert_eq!(editor.root(), None);
    }

    /// R17.4: a colliding emitter's particles land on the preview's floor
    /// (its top at the part's bottom, 1.5) instead of falling through.
    #[test]
    fn colliding_particles_land_on_the_preview_floor() {
        let (mut world, part, emitter) = fountain();
        for (name, value) in [
            ("Collides", Value::Bool(true)),
            ("Bounce", Value::Number(0.0)),
            ("Speed", Value::Number(0.0)),
            ("Lifetime", Value::Number(20.0)),
            (
                "Acceleration",
                Value::Vector3(Vector3::new(0.0, -50.0, 0.0)),
            ),
        ] {
            world.set_property(emitter, name, value).unwrap();
        }
        let mut editor = EffectEditor::default();
        editor.follow(&world, Some(part), 1);
        for _ in 0..60 {
            editor.step(&Assets::default(), 1.0 / 30.0, 30.0);
        }
        let sprites = editor.particles.sprites(&editor.shown, &Assets::default());
        assert!(sprites.len() > 20);
        for sprite in &sprites {
            assert!(sprite.position.y > 1.49, "{:?}", sprite.position);
        }
        assert!(sprites.iter().any(|sprite| sprite.position.y < 1.6));
    }

    #[test]
    fn pause_slow_motion_and_loop_steer_the_clock() {
        let (world, part, _) = fountain();
        let mut editor = EffectEditor::default();
        editor.follow(&world, Some(part), 1);
        editor.speed = 0.25;
        editor.step(&Assets::default(), 0.08, 10.0);
        assert!((editor.clock - 0.02).abs() < 1e-6, "{}", editor.clock);
        editor.paused = true;
        editor.step(&Assets::default(), 0.08, 10.0);
        assert!((editor.clock - 0.02).abs() < 1e-6, "paused");
        editor.paused = false;
        editor.speed = 1.0;
        for _ in 0..20 {
            editor.step(&Assets::default(), 0.05, 0.5);
        }
        assert!(editor.clock <= 0.5, "looped: {}", editor.clock);
    }
}
