//! The terrain volume spike (PW8): storage, brushes, the mesher, and the
//! benchmark whose numbers decide whether to build volumes fully.

use std::collections::HashMap;
use std::time::Instant;

use glam::{IVec3, Vec3};

use super::*;
use crate::noise::{Noise, Rng};

/// Every edge of a closed surface is used as often one way as the other:
/// no gaps, no doubled faces, consistent winding. (Surface nets puts one
/// vertex in a cell even where two pieces of surface pass through it, so
/// an edge there is shared by four triangles, two each way: a pinch, still
/// closed, and a few cells in a row can make it six.)
fn assert_closed(meshes: &HashMap<IVec3, ChunkMesh>) {
    // Chunks share border vertices by position, not by index.
    let key = |p: [f32; 3]| p.map(|v| (v * 1024.0).round() as i64);
    let mut edges: HashMap<([i64; 3], [i64; 3]), i32> = HashMap::new();
    for mesh in meshes.values() {
        for triangle in mesh.indices.chunks(3) {
            for k in 0..3 {
                let a = key(mesh.positions[triangle[k] as usize]);
                let b = key(mesh.positions[triangle[(k + 1) % 3] as usize]);
                *edges.entry((a, b)).or_default() += 1;
            }
        }
    }
    for (&(a, b), &count) in &edges {
        let back = edges.get(&(b, a)).copied().unwrap_or(0);
        assert_ne!(a, b, "a triangle with no area");
        assert!(
            count == back && count <= 3,
            "edge {a:?} -> {b:?}: {count} and {back}"
        );
    }
}

fn triangles(meshes: &HashMap<IVec3, ChunkMesh>) -> usize {
    meshes.values().map(|m| m.indices.len() / 3).sum()
}

#[test]
fn values_round_trip_and_plain_chunks_cost_nothing() {
    let mut volume = VolumeData::new(IVec3::new(70, 40, 70), 1.0);
    assert_eq!(volume.chunks(), IVec3::new(3, 2, 3));
    assert_eq!(volume.dense_chunks(), 0);
    assert_eq!(volume.get(IVec3::new(5, 5, 5)), AIR);
    assert_eq!(volume.get(IVec3::new(-1, 5, 5)), AIR, "outside is air");
    // Rock below y = 10.
    volume.fill(|p| p.y - 10.0);
    assert!((volume.distance(IVec3::new(3, 8, 3)) + 2.0).abs() < 0.1);
    assert!((volume.distance(IVec3::new(3, 12, 3)) - 2.0).abs() < 0.1);
    // Far from the ground the chunks are plain air or rock.
    assert!(!volume.chunk_is_dense(IVec3::new(0, 1, 0)));
    assert_eq!(volume.dense_chunks(), 9, "only the bottom layer");
    let before = volume.memory_bytes();
    assert!(before < 9 * 33_000 + 4_000, "{before}");
}

#[test]
fn a_ball_meshes_closed_across_chunk_borders() {
    let mut volume = VolumeData::new(IVec3::splat(64), 1.0);
    // Centred on the corner where eight chunks meet.
    volume.sphere(Vec3::splat(32.0), 12.0, false);
    let meshes = mesh_all(&volume);
    assert_eq!(meshes.len(), 8, "every chunk round the corner has a piece");
    assert_closed(&meshes);
    // Its vertices sit on the ball and face out of it.
    for mesh in meshes.values() {
        for (p, n) in mesh.positions.iter().zip(&mesh.normals) {
            let p = Vec3::from_array(*p) - Vec3::splat(32.0);
            assert!((p.length() - 12.0).abs() < 0.6, "{p:?}");
            assert!(Vec3::from_array(*n).dot(p.normalize()) > 0.8);
        }
    }
}

#[test]
fn ground_against_the_edge_of_the_volume_is_closed_there() {
    let mut volume = VolumeData::new(IVec3::new(40, 24, 40), 0.5);
    volume.fill(|p| p.y - 8.0);
    let meshes = mesh_all(&volume);
    assert_closed(&meshes);
    // The top is at y = 8 voxels, half a stud apart.
    let top = meshes
        .values()
        .flat_map(|m| &m.positions)
        .map(|p| p[1])
        .fold(f32::MIN, f32::max);
    assert!((top - 4.0).abs() < 0.05, "{top}");
}

#[test]
fn carving_a_tunnel_under_a_hill_leaves_an_overhang_and_changes_only_its_chunks() {
    let mut volume = VolumeData::new(IVec3::new(96, 48, 64), 1.0);
    volume.fill(|p| p.y - 30.0);
    let before = mesh_all(&volume);
    let versions: Vec<u64> = volume
        .chunk_coords()
        .map(|c| volume.chunk_version(c))
        .collect();
    // A tunnel along X at y = 12, reaching the first two chunks (a ball
    // changes samples up to 8 voxels past its edge).
    let mut touched = Vec::new();
    for x in (4..50).step_by(2) {
        touched.extend(volume.sphere(Vec3::new(x as f32, 12.0, 20.0), 5.0, true));
    }
    touched.sort_by_key(|c| (c.x, c.y, c.z));
    touched.dedup();
    for (chunk, old) in volume.chunk_coords().zip(versions) {
        assert_eq!(
            volume.chunk_version(chunk) != old,
            touched.contains(&chunk),
            "{chunk}"
        );
    }
    assert!(!touched.contains(&IVec3::new(2, 0, 0)), "the far end stays");
    let after = mesh_all(&volume);
    assert_closed(&after);
    assert!(triangles(&after) > triangles(&before) + 1_000);
    // Over the tunnel's middle: rock, then air, then rock again, then air.
    let column: Vec<bool> = (0..48)
        .map(|y| volume.distance(IVec3::new(30, y, 20)) < 0.0)
        .collect();
    let changes = column.windows(2).filter(|w| w[0] != w[1]).count();
    assert_eq!(changes, 3, "{column:?}");
    // Filling it back in closes it again.
    for x in (4..50).step_by(2) {
        volume.sphere(Vec3::new(x as f32, 12.0, 20.0), 5.5, false);
    }
    assert!(volume.distance(IVec3::new(30, 12, 20)) < 0.0);
}

#[test]
fn a_ball_of_rock_added_over_ground_is_an_arch_when_carved_through() {
    let mut volume = VolumeData::new(IVec3::splat(48), 1.0);
    volume.fill(|p| p.y - 6.0);
    volume.sphere(Vec3::new(24.0, 14.0, 24.0), 10.0, false);
    volume.sphere(Vec3::new(24.0, 8.0, 24.0), 6.0, true);
    // Air under the rock at the middle: an arch you can see through.
    assert!(volume.distance(IVec3::new(24, 8, 24)) > 0.0);
    assert!(volume.distance(IVec3::new(24, 20, 24)) < 0.0);
    assert_closed(&mesh_all(&volume));
}

/// A landscape with caves: rock up to an fbm heightfield, then worms of
/// carved balls.
fn caves(size: IVec3, seed: u64) -> VolumeData {
    let mut volume = VolumeData::new(size, 1.0);
    let noise = Noise::new(seed);
    let height = size.y as f32;
    volume.fill(|p| {
        let ground = height * (0.45 + 0.25 * noise.fbm(p.x / 160.0, p.z / 160.0, 5, 0.5, 2.0));
        p.y - ground
    });
    let mut rng = Rng::new(seed);
    let worms = (size.x * size.z / 8192).max(1);
    for _ in 0..worms {
        let mut at = Vec3::new(
            rng.next_f32() * size.x as f32,
            height * (0.15 + 0.2 * rng.next_f32()),
            rng.next_f32() * size.z as f32,
        );
        let mut heading = rng.next_f32() * std::f32::consts::TAU;
        for _ in 0..40 {
            volume.sphere(at, 3.0 + 2.0 * rng.next_f32(), true);
            heading += (rng.next_f32() - 0.5) * 0.8;
            at += Vec3::new(heading.cos(), (rng.next_f32() - 0.5) * 0.4, heading.sin()) * 3.0;
        }
    }
    volume
}

#[test]
fn a_small_cave_landscape_meshes_closed() {
    let volume = caves(IVec3::new(96, 64, 96), 3);
    assert_closed(&mesh_all(&volume));
}

/// The spike's numbers: memory, meshing time per chunk, and a brush stroke
/// remade. `AXCENITY_VOLUME_SIZE=512` (samples across; height a quarter)
/// sets the size, and `AXCENITY_VOLUME_OBJ=<file>` writes the surface as an
/// OBJ file to look at. Run in release on the PC:
/// `cargo test --release -p axcenity-terrain -- --ignored --nocapture bench_volume_spike`.
#[test]
#[ignore]
fn bench_volume_spike() {
    let across: i32 = std::env::var("AXCENITY_VOLUME_SIZE")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(512);
    let size = IVec3::new(across, (across / 4).max(64), across);
    let started = Instant::now();
    let mut volume = caves(size, 7);
    let made = started.elapsed();
    let chunks = volume.chunks();
    let started = Instant::now();
    let meshes = mesh_all(&volume);
    let meshed = started.elapsed();
    let surface_chunks = meshes.len();
    let started = Instant::now();
    let middle = size.as_vec3() * Vec3::new(0.5, 0.3, 0.5);
    let mut touched = Vec::new();
    for step in 0..20 {
        touched.extend(volume.sphere(middle + Vec3::X * step as f32, 4.0, true));
    }
    touched.sort_by_key(|c| (c.x, c.y, c.z));
    touched.dedup();
    for &chunk in &touched {
        std::hint::black_box(volume.mesh_chunk(chunk));
    }
    let stroke = started.elapsed();
    if let Some(path) = std::env::var_os("AXCENITY_VOLUME_OBJ") {
        let mut obj = String::new();
        let mut first = 1;
        for mesh in mesh_all(&volume).values() {
            for p in &mesh.positions {
                obj.push_str(&format!("v {} {} {}\n", p[0], p[1], p[2]));
            }
            for t in mesh.indices.chunks(3) {
                let [a, b, c] = [t[0], t[1], t[2]].map(|i| i as usize + first);
                obj.push_str(&format!("f {a} {b} {c}\n"));
            }
            first += mesh.positions.len();
        }
        std::fs::write(path, obj).unwrap();
    }
    println!(
        "volume {size}: {} chunks ({} dense, {} with surface), {:.1} MB samples, \
         {} triangles; made in {made:.2?}, meshed in {meshed:.2?} ({:.2} ms per surface chunk); \
         a 20-ball carve stroke with its {} chunks remeshed: {stroke:.2?}",
        chunks.x * chunks.y * chunks.z,
        volume.dense_chunks(),
        surface_chunks,
        volume.memory_bytes() as f64 / 1e6,
        triangles(&meshes),
        meshed.as_secs_f64() * 1e3 / surface_chunks.max(1) as f64,
        touched.len(),
    );
}
