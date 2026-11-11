//! The LOD Inspector panel: the selected MeshPart's levels of detail, which
//! one it's drawn at and why, what each costs, warnings about them
//! (`assets::LodReport`), and a preview that draws it at one level in the
//! viewport whatever its size. Diagnostic only: nothing in the world or
//! the model changes. How levels are picked: `docs/rendering/performance.md`.

use axcenity_assets::{Assets, LodReport};
use axcenity_core::{ClassName, InstanceId, Value, World};
use axcenity_render::{Camera, LOD_SCREEN_SIZES};
use eframe::egui::{self, RichText};

use crate::responsive;

/// The share of the screen's height a size is shown as.
fn percent(share: f32) -> String {
    if share.is_finite() {
        format!("{:.0}%", share * 100.0)
    } else {
        "fills the view".to_owned()
    }
}

/// What the panel shows for one MeshPart.
pub struct Inspection {
    pub name: String,
    pub mesh_id: String,
    /// `None` while the model isn't loaded (or can't be).
    pub report: Option<LodReport>,
    /// From the camera to its middle, in studs.
    pub distance: f32,
    /// The share of the view's height its bounding sphere covers.
    pub screen_size: f32,
    /// The level its size picks with no history (the renderer also waits
    /// 15% past a threshold before switching back).
    pub expected_level: usize,
    /// The level the renderer last drew it at, if it has levels.
    pub drawn_level: Option<usize>,
}

/// The selected MeshPart `id`, seen from `camera`; `None` for anything
/// else. `drawn_level` asks the renderer.
pub fn inspect(
    world: &World,
    assets: &Assets,
    id: InstanceId,
    camera: Camera,
    drawn_level: impl Fn(u64) -> Option<usize>,
) -> Option<Inspection> {
    let instance = world.get(id)?;
    if instance.class() != ClassName::MeshPart {
        return None;
    }
    let mesh_id = instance
        .property("MeshId")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    let (position, size) = match (instance.property("Position"), instance.property("Size")) {
        (Some(Value::Vector3(position)), Some(Value::Vector3(size))) => (*position, *size),
        _ => return None,
    };
    let center = axcenity_render::glam::Vec3::new(position.x, position.y, position.z);
    // As the renderer does: the sphere around the part's box.
    let radius = axcenity_render::glam::Vec3::new(size.x, size.y, size.z).length() * 0.5;
    let distance = center.distance(camera.position);
    let tan_half = (camera.fov_y_degrees.to_radians() * 0.5).tan().max(1e-4);
    let screen_size = if distance <= radius {
        f32::INFINITY
    } else {
        radius / (distance * tan_half)
    };
    let report = assets
        .model_ready(&mesh_id)
        .map(|model| LodReport::of(&model));
    let levels = report.as_ref().map_or(1, LodReport::level_count);
    let expected_level = LOD_SCREEN_SIZES[..levels.saturating_sub(1).min(LOD_SCREEN_SIZES.len())]
        .iter()
        .filter(|&&threshold| screen_size < threshold)
        .count();
    let item_id = axcenity_scene::item_id(id);
    Some(Inspection {
        name: instance.name().to_owned(),
        mesh_id,
        report,
        distance,
        screen_size,
        expected_level,
        drawn_level: drawn_level(item_id),
    })
}

/// The panel. `preview` is the level being previewed (`None` for the
/// renderer's own choice); returns the new one when a button changes it.
pub fn show(
    ui: &mut egui::Ui,
    inspection: Option<&Inspection>,
    preview: Option<usize>,
) -> Option<Option<usize>> {
    let Some(inspection) = inspection else {
        ui.label("Select a MeshPart to see its levels of detail.");
        return None;
    };
    let mut changed = None;
    ui.label(RichText::new(&inspection.name).strong());
    ui.label(RichText::new(&inspection.mesh_id).small());
    responsive::form(ui, "lod inspector facts", |ui| {
        let mut row = |label: &str, value: String| {
            ui.label(label);
            ui.label(value);
            ui.end_row();
        };
        row(
            "Camera distance",
            format!("{:.1} studs", inspection.distance),
        );
        row("Size on screen", percent(inspection.screen_size));
        let thresholds: Vec<String> = LOD_SCREEN_SIZES
            .iter()
            .enumerate()
            .map(|(index, size)| format!("LOD{} below {}", index + 1, percent(*size)))
            .collect();
        row("Thresholds", thresholds.join(", "));
        row(
            "Drawn at",
            match (preview, inspection.drawn_level) {
                (Some(level), _) => format!("LOD{level} (preview)"),
                (None, Some(level)) => format!("LOD{level}"),
                (None, None) => "its own mesh (no simpler copies)".to_owned(),
            },
        );
        row(
            "Its size picks",
            format!("LOD{}", inspection.expected_level),
        );
    });
    let Some(report) = &inspection.report else {
        ui.label("The model isn't loaded yet.");
        return None;
    };
    ui.separator();
    let full = report.triangles_at(0).max(1);
    egui::Grid::new("lod inspector levels")
        .striped(true)
        .show(ui, |ui| {
            for title in ["Level", "Triangles", "Vertices", "Of LOD0", "Memory"] {
                ui.label(RichText::new(title).small().strong());
            }
            ui.end_row();
            for level in 0..report.level_count().max(1) {
                let triangles = report.triangles_at(level);
                let vertices: usize = report
                    .primitives
                    .iter()
                    .filter_map(|p| p.levels.get(level).or(p.levels.last()))
                    .map(|l| l.vertices)
                    .sum();
                let bytes: u64 = report
                    .primitives
                    .iter()
                    .filter_map(|p| p.levels.get(level))
                    .map(|l| l.bytes)
                    .sum();
                ui.label(format!("LOD{level}"));
                ui.label(triangles.to_string());
                ui.label(vertices.to_string());
                ui.label(format!("{:.0}%", triangles as f64 * 100.0 / full as f64));
                ui.label(format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0)));
                ui.end_row();
            }
        });
    ui.label(
        RichText::new(format!(
            "{} primitives; levels are kept per primitive and switch together.",
            report.primitives.len()
        ))
        .small(),
    );
    ui.separator();
    ui.label("Preview in the viewport");
    ui.horizontal_wrapped(|ui| {
        if ui.selectable_label(preview.is_none(), "Auto").clicked() && preview.is_some() {
            changed = Some(None);
        }
        for level in 0..report.level_count().max(1) {
            if ui
                .selectable_label(preview == Some(level), format!("LOD{level}"))
                .clicked()
                && preview != Some(level)
            {
                changed = Some(Some(level));
            }
        }
    });
    if !report.warnings.is_empty() {
        ui.separator();
        for warning in &report.warnings {
            ui.label(RichText::new(format!("⚠ {}", warning.text())).small());
        }
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;
    use axcenity_core::Vector3;

    #[test]
    fn a_mesh_part_is_inspected_from_the_camera_and_anything_else_is_not() {
        let mut world = World::new();
        let workspace = world.workspace();
        let part = world.create(ClassName::MeshPart).unwrap();
        world.set_parent(part, Some(workspace)).unwrap();
        world
            .set_property(part, "Size", Value::Vector3(Vector3::new(4.0, 4.0, 4.0)))
            .unwrap();
        world
            .set_property(
                part,
                "Position",
                Value::Vector3(Vector3::new(0.0, 0.0, -100.0)),
            )
            .unwrap();
        let block = world.create(ClassName::Part).unwrap();
        world.set_parent(block, Some(workspace)).unwrap();
        let assets = Assets::default();
        let camera = Camera {
            position: axcenity_render::glam::Vec3::ZERO,
            look_at: axcenity_render::glam::Vec3::NEG_Z,
            fov_y_degrees: 70.0,
        };
        assert!(inspect(&world, &assets, block, camera, |_| None).is_none());
        let seen = inspect(&world, &assets, part, camera, |_| Some(2)).unwrap();
        assert!((seen.distance - 100.0).abs() < 1e-3);
        // A 4-stud box's sphere (radius √12) at 100 studs with a 70° view.
        let expected = 12f32.sqrt() / (100.0 * 35f32.to_radians().tan());
        assert!((seen.screen_size - expected).abs() < 1e-4);
        assert_eq!(seen.drawn_level, Some(2));
        assert!(seen.report.is_none(), "no model loaded");
    }
}
