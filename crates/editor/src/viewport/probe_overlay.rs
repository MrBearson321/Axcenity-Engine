//! The Light Probes overlay (roadmap R18.1, `docs/research/global-illumination.md`):
//! every LightProbeVolume's probes as small discs filled with the light the
//! bake gives a surface facing the camera, so a dark or leaking probe shows
//! where it stands. Volumes with no bake, a bake that can't be read or one
//! made from a different world are outlined in their own colour, and a
//! legend counts probes, bakes and the renderer's buffer. The probes'
//! places are `scene::light_probe_points`, the same the bake uses, and
//! their light is the bake's (`docs/rendering/sky.md`); with real-time GI on,
//! the renderer's live values aren't read back.

use axcenity_assets::{Assets, LightProbes, ProbeLight};
use axcenity_core::{ClassName, InstanceId, Value, World};
use axcenity_render::glam::Vec3;
use eframe::egui::{self, Color32, RichText};

use super::legend::thousands;
use super::overlays::bounds_edges;
use crate::screen::Screen;

/// A volume whose bake matches the world.
const BAKED_COLOR: Color32 = Color32::from_rgb(110, 220, 120);
/// A volume whose bake was made from a different world.
const STALE_COLOR: Color32 = Color32::from_rgb(255, 170, 60);
/// A volume with no bake yet: the sky lights it.
const UNBAKED_COLOR: Color32 = Color32::from_rgb(150, 150, 165);
/// A volume whose `Bake` names a file that can't be read.
const MISSING_COLOR: Color32 = Color32::from_rgb(240, 80, 80);
/// At most this many probes are drawn, the nearest first.
const MAX_PROBES: usize = 4000;
/// A probe's disc, in studs across, before it's projected.
const PROBE_SIZE: f32 = 0.6;
/// The renderer keeps the nearest this many volumes' probes
/// (`render`'s `MAX_VOLUMES`).
const DRAWN_VOLUMES: usize = 8;
/// Bytes each probe takes in the renderer's buffer: four `vec4`s.
const PROBE_BYTES: u64 = 64;

/// What a volume's bake is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum BakeState {
    /// Baked from this world.
    Baked,
    /// Baked from a different world (the Explorer badges it too).
    Stale,
    /// Never baked: the sky lights it.
    Unbaked,
    /// `Bake` names a file that can't be read, or whose grid doesn't fit.
    Missing,
}

impl BakeState {
    fn color(self) -> Color32 {
        match self {
            BakeState::Baked => BAKED_COLOR,
            BakeState::Stale => STALE_COLOR,
            BakeState::Unbaked => UNBAKED_COLOR,
            BakeState::Missing => MISSING_COLOR,
        }
    }
}

/// One volume as the overlay sees it.
pub(super) struct VolumeView {
    pub id: InstanceId,
    pub state: BakeState,
    /// Each probe's place in the world.
    pub points: Vec<Vec3>,
    /// Each probe's baked light, when the bake fits the grid.
    pub light: Option<std::sync::Arc<LightProbes>>,
}

/// Every LightProbeVolume in the Workspace with its probes and bake.
/// `stale` is the editor's list of volumes baked from another world.
pub(super) fn volumes(world: &World, assets: &Assets, stale: &[InstanceId]) -> Vec<VolumeView> {
    world
        .descendants(world.workspace())
        .into_iter()
        .filter_map(|id| {
            let instance = world.get(id)?;
            if instance.class() != ClassName::LightProbeVolume {
                return None;
            }
            let grid = axcenity_scene::light_probe_points(world, id)?;
            let bake = match instance.property("Bake") {
                Some(Value::String(path)) if !path.is_empty() => Some(assets.light_probes(path)),
                _ => None,
            };
            let (state, light) = match bake {
                None => (BakeState::Unbaked, None),
                Some(Some(light)) if light.counts == grid.counts => {
                    let state = if stale.contains(&id) {
                        BakeState::Stale
                    } else {
                        BakeState::Baked
                    };
                    (state, Some(light))
                }
                Some(_) => (BakeState::Missing, None),
            };
            Some(VolumeView {
                id,
                state,
                points: grid.points,
                light,
            })
        })
        .collect()
}

/// The light a bake gives a white matte surface facing `normal`: the same
/// sum `lighting.wgsl` makes (`c0·0.282095 + 0.488603·(c1 n.y + c2 n.z + c3 n.x)`).
pub(super) fn probe_light(probe: &ProbeLight, normal: Vec3) -> Vec3 {
    let [c0, c1, c2, c3] = probe.map(Vec3::from_array);
    (c0 * 0.282_095 + (c1 * normal.y + c2 * normal.z + c3 * normal.x) * 0.488_603).max(Vec3::ZERO)
}

/// Linear light as a colour to draw: Reinhard, then sRGB's curve.
pub(super) fn display_color(light: Vec3) -> Color32 {
    let mapped = light / (Vec3::ONE + light);
    let [r, g, b] = mapped.to_array().map(|channel| {
        let srgb = if channel <= 0.003_130_8 {
            channel * 12.92
        } else {
            1.055 * channel.powf(1.0 / 2.4) - 0.055
        };
        (srgb.clamp(0.0, 1.0) * 255.0).round() as u8
    });
    Color32::from_rgb(r, g, b)
}

/// Draws the overlay over `rect` for a camera at `eye`, and its legend.
pub(super) fn paint(
    ui: &egui::Ui,
    rect: egui::Rect,
    screen: &Screen,
    world: &World,
    assets: &Assets,
    stale: &[InstanceId],
    eye: Vec3,
) {
    let volumes = volumes(world, assets, stale);
    let painter = ui.painter_at(rect);
    for volume in &volumes {
        let Some(edges) = world
            .get(volume.id)
            .and_then(|instance| instance.parent())
            .and_then(|parent| world.get(parent))
            .and_then(bounds_edges)
        else {
            continue;
        };
        let stroke = egui::Stroke::new(1.5_f32, volume.state.color());
        for (a, b) in edges {
            if let Some(line) = screen.line(a, b) {
                painter.line_segment(line, stroke);
            }
        }
    }
    let mut probes: Vec<(f32, usize, usize)> = volumes
        .iter()
        .enumerate()
        .flat_map(|(v, volume)| {
            volume
                .points
                .iter()
                .enumerate()
                .map(move |(p, point)| (point.distance_squared(eye), v, p))
        })
        .collect();
    probes.sort_by(|a, b| a.0.total_cmp(&b.0));
    // Farthest first, so nearer discs cover farther ones.
    for &(_, v, p) in probes.iter().take(MAX_PROBES).rev() {
        let volume = &volumes[v];
        let point = volume.points[p];
        let Some(center) = screen.point(point) else {
            continue;
        };
        let radius = (PROBE_SIZE / 2.0 / screen.studs_per_point(point)).clamp(2.0, 14.0);
        let fill = volume.light.as_ref().and_then(|light| light.probes.get(p));
        match fill {
            Some(probe) => {
                let toward = (eye - point).normalize_or_zero();
                painter.circle(
                    center,
                    radius,
                    display_color(probe_light(probe, toward)),
                    egui::Stroke::new(1.0_f32, Color32::from_black_alpha(160)),
                );
            }
            None => {
                painter.circle_stroke(
                    center,
                    radius,
                    egui::Stroke::new(1.0_f32, volume.state.color()),
                );
            }
        }
    }
    show_legend(ui, rect, &legend_lines(&volumes, eye));
}

/// The legend's lines: each kind of volume and how many, the probes, and
/// what the renderer keeps.
pub(super) fn legend_lines(volumes: &[VolumeView], eye: Vec3) -> Vec<(Option<Color32>, String)> {
    if volumes.is_empty() {
        return vec![(
            None,
            "No light probe volumes. Insert one with Light Probes on the ribbon, then Bake Probes."
                .to_owned(),
        )];
    }
    let count = |state: BakeState| volumes.iter().filter(|v| v.state == state).count();
    let probes: usize = volumes.iter().map(|volume| volume.points.len()).sum();
    let mut lines = Vec::new();
    for (state, text) in [
        (BakeState::Baked, "baked"),
        (
            BakeState::Stale,
            "baked from a different world: Bake Probes again",
        ),
        (BakeState::Unbaked, "not baked: the sky lights them"),
        (BakeState::Missing, "bake file missing or the wrong size"),
    ] {
        let n = count(state);
        if n > 0 {
            lines.push((
                Some(state.color()),
                format!("{} {} {text}", n, if n == 1 { "volume" } else { "volumes" }),
            ));
        }
    }
    // The renderer keeps the nearest baked volumes' probes, by their box's
    // middle (close enough for the count).
    let mut baked: Vec<(f32, usize)> = volumes
        .iter()
        .filter(|volume| volume.light.is_some())
        .map(|volume| {
            let middle = volume.points.iter().copied().sum::<Vec3>() / volume.points.len() as f32;
            (middle.distance_squared(eye), volume.points.len())
        })
        .collect();
    baked.sort_by(|a, b| a.0.total_cmp(&b.0));
    let kept: usize = baked.iter().take(DRAWN_VOLUMES).map(|&(_, n)| n).sum();
    lines.push((
        None,
        format!(
            "{} probes. The renderer keeps the nearest {} baked volumes here: {} probes, {} KB.",
            thousands(probes as u64),
            DRAWN_VOLUMES.min(baked.len()),
            thousands(kept as u64),
            thousands((kept as u64 * PROBE_BYTES).div_ceil(1024))
        ),
    ));
    if probes > MAX_PROBES {
        lines.push((
            None,
            format!("The nearest {} are drawn.", thousands(MAX_PROBES as u64)),
        ));
    }
    lines
}

fn show_legend(ui: &egui::Ui, rect: egui::Rect, lines: &[(Option<Color32>, String)]) {
    egui::Area::new(egui::Id::new("light probe overlay legend"))
        .order(egui::Order::Foreground)
        .pivot(egui::Align2::RIGHT_BOTTOM)
        .fixed_pos(rect.right_bottom() + egui::vec2(-10.0, -10.0))
        .constrain_to(rect)
        .show(ui.ctx(), |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                ui.set_max_width(300.0);
                ui.spacing_mut().item_spacing.y = 2.0;
                ui.label(RichText::new("Light probes").strong());
                for (swatch, text) in lines {
                    ui.horizontal(|ui| {
                        if let Some(color) = swatch {
                            let (square, _) = ui
                                .allocate_exact_size(egui::vec2(12.0, 12.0), egui::Sense::hover());
                            ui.painter().rect_filled(square, 2.0, *color);
                        }
                        ui.add(egui::Label::new(RichText::new(text).small()).wrap());
                    });
                }
            });
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    use axcenity_core::Vector3;

    /// A 8×4×4 part at the origin holding a LightProbeVolume (Spacing 4, so
    /// 2×1×1 probes) whose `Bake` is `bake`.
    fn volume(world: &mut World, name: &str, bake: &str) -> InstanceId {
        let part = world.create(ClassName::Part).unwrap();
        world
            .set_property(part, "Size", Value::Vector3(Vector3::new(8.0, 4.0, 4.0)))
            .unwrap();
        world.set_parent(part, Some(world.workspace())).unwrap();
        let volume = world.create(ClassName::LightProbeVolume).unwrap();
        world.set_name(volume, name).unwrap();
        world
            .set_property(volume, "Bake", Value::String(bake.to_owned()))
            .unwrap();
        world.set_parent(volume, Some(part)).unwrap();
        volume
    }

    #[test]
    fn a_probe_lights_a_surface_as_the_shader_does_and_draws_tone_mapped() {
        // Only the constant term: the same light every way.
        let flat: ProbeLight = [[1.0 / 0.282_095; 3], [0.0; 3], [0.0; 3], [0.0; 3]];
        let light = probe_light(&flat, Vec3::Y);
        assert!((light - Vec3::ONE).abs().max_element() < 1e-4, "{light}");
        // c1 pairs with y: brighter facing up, and never below black.
        let up: ProbeLight = [[0.0; 3], [1.0, 0.0, 0.0], [0.0; 3], [0.0; 3]];
        assert!((probe_light(&up, Vec3::Y).x - 0.488_603).abs() < 1e-5);
        assert_eq!(probe_light(&up, Vec3::NEG_Y), Vec3::ZERO);
        assert_eq!(display_color(Vec3::ZERO), Color32::BLACK);
        // 1 maps to a half in linear light, about 188 in sRGB.
        assert_eq!(display_color(Vec3::ONE), Color32::from_rgb(188, 188, 188));
        assert_eq!(display_color(Vec3::splat(1e9)), Color32::WHITE);
    }

    #[test]
    fn each_volume_is_baked_stale_unbaked_or_missing_and_the_legend_counts_them() {
        let folder =
            std::env::temp_dir().join(format!("axcenity-probe-overlay-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        let bake = LightProbes::new([2, 1, 1], 7, vec![[[0.5; 3]; 4]; 2]).unwrap();
        axcenity_assets::write_light_probes(&folder, "assets/probes/a.lightprobes.json", &bake)
            .unwrap();
        axcenity_assets::write_light_probes(&folder, "assets/probes/b.lightprobes.json", &bake)
            .unwrap();
        let assets = Assets::new(&folder);
        let mut world = World::new();
        let baked = volume(&mut world, "Baked", "assets/probes/a.lightprobes.json");
        let stale = volume(&mut world, "Stale", "assets/probes/b.lightprobes.json");
        let unbaked = volume(&mut world, "Unbaked", "");
        let missing = volume(&mut world, "Missing", "assets/probes/gone.lightprobes.json");

        let views = volumes(&world, &assets, &[stale]);
        let state = |id| views.iter().find(|view| view.id == id).unwrap().state;
        assert_eq!(state(baked), BakeState::Baked);
        assert_eq!(state(stale), BakeState::Stale);
        assert_eq!(state(unbaked), BakeState::Unbaked);
        assert_eq!(state(missing), BakeState::Missing);
        let first = views.iter().find(|view| view.id == baked).unwrap();
        assert_eq!(first.points.len(), 2);
        assert_eq!(first.light.as_ref().unwrap().probes.len(), 2);

        let lines = legend_lines(&views, Vec3::ZERO);
        let texts: Vec<&str> = lines.iter().map(|(_, text)| text.as_str()).collect();
        assert_eq!(
            texts,
            [
                "1 volume baked",
                "1 volume baked from a different world: Bake Probes again",
                "1 volume not baked: the sky lights them",
                "1 volume bake file missing or the wrong size",
                "8 probes. The renderer keeps the nearest 2 baked volumes here: 4 probes, 1 KB.",
            ]
        );
        assert_eq!(lines[1].0, Some(STALE_COLOR));
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn with_no_volumes_the_legend_says_how_to_add_one() {
        let lines = legend_lines(&[], Vec3::ZERO);
        assert!(
            lines[0].1.starts_with("No light probe volumes."),
            "{lines:?}"
        );
    }
}
