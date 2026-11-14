//! The terrain volume spike (PW8): a volume's chunk meshes as triangle
//! mesh colliders hold a ball up in a carved cave, under an overhang.

use std::time::Instant;

use axcenity_terrain::volume::{VolumeData, mesh_all};
use glam::IVec3;
use rapier3d::glamx::Vec3;
use rapier3d::prelude::*;

use crate::triangle_mesh;

struct Scene {
    bodies: RigidBodySet,
    colliders: ColliderSet,
}

impl Scene {
    fn of(volume: &VolumeData) -> Self {
        let mut colliders = ColliderSet::new();
        for mesh in mesh_all(volume).values() {
            let collider = triangle_mesh(&mesh.positions, &mesh.indices).expect("a mesh");
            colliders.insert(collider);
        }
        Scene {
            bodies: RigidBodySet::new(),
            colliders,
        }
    }

    /// Drops a ball of `radius` at `at` and lets it settle for `seconds`;
    /// where it ends up. Once a scene: its broad phase is its own.
    fn drop_ball(mut self, at: Vec3, radius: f32, seconds: f32) -> Vec3 {
        let ball = self.bodies.insert(
            RigidBodyBuilder::dynamic()
                .translation(at)
                .ccd_enabled(true),
        );
        self.colliders
            .insert_with_parent(ColliderBuilder::ball(radius), ball, &mut self.bodies);
        let mut pipeline = PhysicsPipeline::new();
        let parameters = IntegrationParameters::default();
        let mut islands = IslandManager::new();
        let mut broad = BroadPhaseBvh::new();
        let mut narrow = NarrowPhase::new();
        let mut impulse = ImpulseJointSet::new();
        let mut multibody = MultibodyJointSet::new();
        let mut soft = SoftBodySet::new();
        let mut ccd = CCDSolver::new();
        for _ in 0..(seconds * 60.0) as usize {
            pipeline.step(
                Vec3::new(0.0, -196.2, 0.0),
                &parameters,
                &mut islands,
                &mut broad,
                &mut narrow,
                &mut self.bodies,
                &mut self.colliders,
                &mut impulse,
                &mut multibody,
                &mut soft,
                &mut ccd,
                &(),
                &(),
            );
        }
        self.bodies[ball].translation()
    }
}

#[test]
fn a_ball_rests_on_a_cave_floor_under_rock_and_on_top_of_the_hill() {
    // Rock up to y = 30 studs, a tunnel along X at y = 12, radius 5.
    let mut volume = VolumeData::new(IVec3::new(64, 40, 40), 1.0);
    volume.fill(|p| p.y - 30.0);
    for x in (-8..72).step_by(2) {
        volume.sphere(glam::Vec3::new(x as f32, 12.0, 20.0), 5.0, true);
    }
    // Dropped in at the open end, it stays in the tunnel: on its floor,
    // under 13 studs of rock.
    let inside = Scene::of(&volume).drop_ball(Vec3::new(2.0, 12.0, 20.0), 1.0, 2.0);
    assert!((inside.y - 8.0).abs() < 0.6, "{inside:?}");
    // Dropped from above the hill, the top holds it.
    let on_top = Scene::of(&volume).drop_ball(Vec3::new(30.0, 36.0, 20.0), 1.0, 2.0);
    assert!((on_top.y - 31.0).abs() < 0.6, "{on_top:?}");
}

/// Builds every chunk's collider for a large cave landscape.
/// `cargo test --release -p axcenity-physics -- --ignored --nocapture bench_volume_colliders`.
#[test]
#[ignore]
fn bench_volume_colliders() {
    let mut volume = VolumeData::new(IVec3::new(512, 128, 512), 1.0);
    volume.fill(|p| p.y - 64.0 - 8.0 * (p.x / 40.0).sin() * (p.z / 50.0).cos());
    for x in (0..512).step_by(3) {
        let z = 256.0 + 80.0 * (x as f32 / 90.0).sin();
        volume.sphere(glam::Vec3::new(x as f32, 40.0, z), 5.0, true);
    }
    let meshes = mesh_all(&volume);
    let started = Instant::now();
    let mut triangles = 0;
    for mesh in meshes.values() {
        triangles += mesh.indices.len() / 3;
        std::hint::black_box(triangle_mesh(&mesh.positions, &mesh.indices).map(|b| b.build()));
    }
    let built = started.elapsed();
    println!(
        "{} chunk colliders, {triangles} triangles, built in {built:.2?} ({:.2} ms a chunk)",
        meshes.len(),
        built.as_secs_f64() * 1e3 / meshes.len().max(1) as f64
    );
}
