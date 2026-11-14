//! The Skeletons overlay (AP6): every rigged MeshPart's bones as lines from
//! each bone to its parent, with a dot at each joint. During Play and in
//! the Sequencer's preview it follows the pose the Animators made; while
//! editing it shows the rest pose. The selected thing's skeleton is drawn
//! brighter.

use axcenity_assets::{Assets, PosedBone};
use axcenity_core::{ClassName, InstanceId, Value, World};
use axcenity_render::glam::{Mat4, Vec3};
use eframe::egui;

use crate::gizmo::Pose;
use crate::screen::Screen;

const BONE_COLOR: egui::Color32 = egui::Color32::from_rgb(255, 170, 60);
const SELECTED_BONE_COLOR: egui::Color32 = egui::Color32::from_rgb(255, 230, 120);
/// Skeletons farther than this from the camera aren't drawn.
const SKELETON_DISTANCE: f32 = 250.0;

/// One skeleton to draw, in world space.
pub(super) struct Drawn {
    pub part: InstanceId,
    /// Each bone's world position and its parent's index.
    pub bones: Vec<(Vec3, Option<usize>)>,
}

/// Every rigged MeshPart's skeleton within reach of `eye`: posed where
/// `poses` has the part, else at rest.
pub(super) fn skeletons(
    world: &World,
    assets: &Assets,
    poses: &axcenity_scene::Poses,
    eye: Vec3,
) -> Vec<Drawn> {
    let mut drawn = Vec::new();
    for id in world.descendants(world.workspace()) {
        let Some(instance) = world.get(id) else {
            continue;
        };
        if instance.class() != ClassName::MeshPart {
            continue;
        }
        let Some(at) = Pose::of(instance) else {
            continue;
        };
        if (at.position - eye).length() > SKELETON_DISTANCE + at.size.length() {
            continue;
        }
        let bones: Vec<PosedBone> = match poses.get(&id) {
            Some(posed) => posed.bones().to_vec(),
            None => {
                let mesh = instance
                    .property("MeshId")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                let Some(model) = (!mesh.is_empty())
                    .then(|| assets.model_ready(mesh))
                    .flatten()
                else {
                    continue;
                };
                let Some(rig) = model.rig.as_deref() else {
                    continue;
                };
                rig.skeleton(&rig.rest_pose())
            }
        };
        if bones.is_empty() {
            continue;
        }
        let place = Mat4::from_scale_rotation_translation(at.size, at.rotation, at.position);
        drawn.push(Drawn {
            part: id,
            bones: bones
                .iter()
                .map(|bone| (place.transform_point3(bone.position), bone.parent))
                .collect(),
        });
    }
    drawn
}

/// Draws `skeletons`, the ones in or holding the `selection` brighter.
pub(super) fn paint(
    painter: &egui::Painter,
    screen: &Screen,
    world: &World,
    skeletons: &[Drawn],
    selection: &[InstanceId],
) {
    for skeleton in skeletons {
        let selected = selection.iter().any(|&chosen| {
            chosen == skeleton.part
                || world.is_descendant_of(skeleton.part, chosen)
                || world.is_descendant_of(chosen, skeleton.part)
        });
        let color = if selected {
            SELECTED_BONE_COLOR
        } else {
            BONE_COLOR.gamma_multiply(0.8)
        };
        let width = if selected { 2.0 } else { 1.25 };
        let stroke = egui::Stroke::new(width, color);
        for &(position, parent) in &skeleton.bones {
            if let Some(&(from, _)) = parent.and_then(|parent| skeleton.bones.get(parent))
                && let Some(line) = screen.line(from, position)
            {
                painter.line_segment(line, stroke);
            }
        }
        for &(position, _) in &skeleton.bones {
            if let Some(at) = screen.point(position) {
                painter.circle_filled(at, width + 1.0, color);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use axcenity_core::Vector3;

    use super::*;

    fn noodle() -> (World, Assets, InstanceId) {
        let assets = Assets::new(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/hello-animation"),
        );
        assets.model_blocking("assets/models/noodle.fbx").unwrap();
        let mut world = World::new();
        let part = world.create(ClassName::MeshPart).unwrap();
        world
            .set_property(
                part,
                "MeshId",
                Value::String("assets/models/noodle.fbx".into()),
            )
            .unwrap();
        world
            .set_property(part, "Size", Value::Vector3(Vector3::new(1.0, 3.8, 1.0)))
            .unwrap();
        world
            .set_property(
                part,
                "Position",
                Value::Vector3(Vector3::new(10.0, 0.0, 0.0)),
            )
            .unwrap();
        let workspace = world.workspace();
        world.set_parent(part, Some(workspace)).unwrap();
        (world, assets, part)
    }

    #[test]
    fn a_rigged_part_shows_its_rest_skeleton_where_it_stands() {
        let (world, assets, part) = noodle();
        let drawn = skeletons(&world, &assets, &axcenity_scene::Poses::new(), Vec3::ZERO);
        assert_eq!(drawn.len(), 1);
        assert_eq!(drawn[0].part, part);
        let model = assets.model_blocking("assets/models/noodle.fbx").unwrap();
        let rig = model.rig.as_deref().unwrap();
        assert_eq!(drawn[0].bones.len(), rig.bones.len());
        // The column stands 3.8 tall around x = 10: Top is above Root.
        let at = |name| drawn[0].bones[rig.bone(name).unwrap()].0;
        assert!((at("Root").x - 10.0).abs() < 0.6, "{}", at("Root"));
        assert!(at("Top").y > at("Root").y + 1.0);
        // Too far away to draw.
        let far = skeletons(
            &world,
            &assets,
            &axcenity_scene::Poses::new(),
            Vec3::new(10_000.0, 0.0, 0.0),
        );
        assert!(far.is_empty());
    }

    #[test]
    fn a_posed_part_shows_the_pose() {
        let (world, assets, part) = noodle();
        let model = assets.model_blocking("assets/models/noodle.fbx").unwrap();
        let rig = model.rig.as_deref().unwrap();
        let mut pose = rig.rest_pose();
        let root = rig.bone("Root").unwrap();
        pose[root].translation.z += 2.0;
        let mut poses = axcenity_scene::Poses::new();
        poses.insert(part, std::sync::Arc::new(model.pose(&pose).unwrap()));
        let rest = skeletons(&world, &assets, &axcenity_scene::Poses::new(), Vec3::ZERO);
        let posed = skeletons(&world, &assets, &poses, Vec3::ZERO);
        assert!(posed[0].bones[root].0.z > rest[0].bones[root].0.z + 0.1);
    }
}
