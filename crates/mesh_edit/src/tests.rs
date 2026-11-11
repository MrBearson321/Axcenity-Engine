use std::collections::HashMap;
use std::path::PathBuf;

use axcenity_assets::{Assets, ModelFormat, Pick, load_models};
use axcenity_core::{ClassName, PartType, Value, Vector3, World};
use glam::{Vec2, Vec3};

use crate::*;

/// True if every edge, matched by position, is used once each way: the
/// mesh is closed with its triangles facing one way.
fn is_closed(mesh: &Mesh) -> bool {
    let mut edges: HashMap<([u32; 3], [u32; 3]), i32> = HashMap::new();
    for triangle in &mesh.triangles {
        let [a, b, c] = triangle
            .corners
            .map(|corner| position_key(mesh.positions[corner as usize]));
        for (from, to) in [(a, b), (b, c), (c, a)] {
            if from != to {
                *edges.entry((from, to)).or_default() += 1;
            }
        }
    }
    edges
        .iter()
        .all(|(&(from, to), &count)| count == 1 && edges.get(&(to, from)) == Some(&1))
}

/// A solid made of unit cubes at `cells`, with only the faces between a
/// cube and empty space. Every corner is on the grid, so it's closed.
fn cubes(cells: &[[i32; 3]]) -> Mesh {
    let filled = |cell: [i32; 3]| cells.contains(&cell);
    let faces = [
        (Vec3::X, Vec3::Y, Vec3::Z),
        (Vec3::NEG_X, Vec3::Z, Vec3::Y),
        (Vec3::Y, Vec3::Z, Vec3::X),
        (Vec3::NEG_Y, Vec3::X, Vec3::Z),
        (Vec3::Z, Vec3::X, Vec3::Y),
        (Vec3::NEG_Z, Vec3::Y, Vec3::X),
    ];
    let mut mesh = Mesh::default();
    for &cell in cells {
        let middle = Vec3::new(cell[0] as f32, cell[1] as f32, cell[2] as f32) + 0.5;
        for (normal, u, v) in faces {
            let n = normal.as_ivec3();
            if filled([cell[0] + n.x, cell[1] + n.y, cell[2] + n.z]) {
                continue;
            }
            let first = mesh.positions.len() as u32;
            for corner in [-u - v, u - v, u + v, -u + v] {
                mesh.push_vertex(middle + (normal + corner) * 0.5, normal, Vec2::ZERO);
            }
            for corners in [[0, 1, 2], [0, 2, 3]] {
                mesh.triangles.push(Triangle {
                    corners: corners.map(|k| first + k),
                    surface: Surface::Outer(0),
                });
            }
        }
    }
    mesh
}

fn close(a: f32, b: f32, within: f32) -> bool {
    (a - b).abs() <= within * a.abs().max(b.abs()).max(1.0)
}

fn plane(normal: Vec3, offset: f32) -> Plane {
    Plane { normal, offset }
}

/// Both halves closed, and their volumes adding up to the whole's.
fn assert_cut_keeps_volume(mesh: &Mesh, plane: Plane) -> (Mesh, Mesh) {
    assert!(is_closed(mesh), "the mesh to cut should be closed");
    let (below, above) = mesh.cut(plane);
    let (below, above) = (below.expect("below"), above.expect("above"));
    assert!(is_closed(&below), "the half below should be closed");
    assert!(is_closed(&above), "the half above should be closed");
    let sum = below.volume() + above.volume();
    assert!(
        close(sum, mesh.volume(), 1e-4),
        "{} + {} should be {}",
        below.volume(),
        above.volume(),
        mesh.volume()
    );
    (below, above)
}

#[test]
fn shapes_are_closed_and_face_outward() {
    let size = Vec3::new(2.0, 3.0, 4.0);
    let block = Mesh::block(size);
    assert!(close(block.volume(), 24.0, 1e-5));
    assert_eq!(block.bounds(), (-size / 2.0, size / 2.0));
    // Half the box.
    assert!(close(Mesh::wedge(size).volume(), 12.0, 1e-5));
    // A pyramid on the whole bottom: a third of the box.
    assert!(close(Mesh::corner_wedge(size).volume(), 8.0, 1e-5));
    assert_eq!(Mesh::corner_wedge(size).bounds(), (-size / 2.0, size / 2.0));
    // Slightly less than a true sphere's or cylinder's, being faceted.
    let ball = Mesh::ball(size).volume();
    assert!(ball > 0.9 * 4.0 / 3.0 * std::f32::consts::PI && ball < 4.19);
    let cylinder = Mesh::cylinder(size).volume();
    let round = std::f32::consts::PI * 1.5 * 1.5 * 2.0;
    assert!(cylinder > 0.97 * round && cylinder < round);
    assert_eq!(Mesh::cylinder(size).bounds().1, Vec3::new(1.0, 1.5, 1.5));
    for mesh in [
        block,
        Mesh::wedge(size),
        Mesh::corner_wedge(size),
        Mesh::ball(size),
        Mesh::cylinder(size),
    ] {
        assert!(is_closed(&mesh));
    }
}

#[test]
fn cutting_a_block_gives_two_closed_boxes() {
    let block = Mesh::block(Vec3::new(2.0, 4.0, 6.0));
    let (below, above) = assert_cut_keeps_volume(&block, plane(Vec3::Y, 0.5));
    assert!(close(below.volume(), 2.0 * 2.5 * 6.0, 1e-5));
    assert!(close(above.volume(), 2.0 * 1.5 * 6.0, 1e-5));
    for (half, outward) in [(&below, Vec3::Y), (&above, Vec3::NEG_Y)] {
        let interior: Vec<_> = half
            .triangles
            .iter()
            .filter(|t| t.surface == Surface::Interior)
            .collect();
        assert!(!interior.is_empty());
        let mut area = 0.0;
        for triangle in interior {
            let [a, b, c] = triangle.corners.map(|k| half.positions[k as usize]);
            for p in [a, b, c] {
                assert!((p.y - 0.5).abs() < 1e-6, "{p} should be on the cut");
            }
            let facing = (b - a).cross(c - a);
            assert!(facing.dot(outward) >= 0.0, "the cap should face out");
            area += facing.length() / 2.0;
            assert_eq!(half.normals[triangle.corners[0] as usize], outward);
        }
        assert!(close(area, 12.0, 1e-5), "the cap covers the cut: {area}");
        let (min, max) = half.bounds();
        assert!(min.y >= -2.0 && max.y <= 2.0);
    }
    assert_eq!(below.bounds().1.y, 0.5);
    assert_eq!(above.bounds().0.y, 0.5);
}

#[test]
fn cutting_slanted_keeps_the_volume() {
    let slant = plane(Vec3::new(0.3, 1.0, -0.5), 0.2);
    let size = Vec3::new(2.0, 3.0, 4.0);
    for mesh in [
        Mesh::block(size),
        Mesh::wedge(size),
        Mesh::corner_wedge(size),
        Mesh::ball(size),
        Mesh::cylinder(size),
    ] {
        assert_cut_keeps_volume(&mesh, slant);
    }
}

#[test]
fn cutting_through_a_corner_keeps_the_volume() {
    // The plane passes exactly through block corners and along edges.
    let block = Mesh::block(Vec3::splat(2.0));
    assert_cut_keeps_volume(&block, plane(Vec3::new(1.0, 1.0, 0.0), 0.0));
    assert_cut_keeps_volume(&block, plane(Vec3::new(1.0, 1.0, 1.0), 1.0));
}

#[test]
fn cutting_an_l_across_fills_a_concave_cap() {
    // An L lying flat, cut through its thickness: the cap is the L itself.
    let l = cubes(&[[0, 0, 0], [1, 0, 0], [2, 0, 0], [0, 1, 0], [0, 2, 0]]);
    let (below, above) = assert_cut_keeps_volume(&l, plane(Vec3::Z, 0.5));
    assert!(close(below.volume(), 2.5, 1e-5));
    assert!(close(above.volume(), 2.5, 1e-5));
}

#[test]
fn cutting_both_arms_of_a_u_fills_two_caps() {
    let u = cubes(&[
        [0, 0, 0],
        [1, 0, 0],
        [2, 0, 0],
        [0, 1, 0],
        [2, 1, 0],
        [0, 2, 0],
        [2, 2, 0],
    ]);
    let (below, above) = assert_cut_keeps_volume(&u, plane(Vec3::Y, 1.5));
    assert!(close(below.volume(), 4.0, 1e-5));
    assert!(close(above.volume(), 3.0, 1e-5));
    // Two separate caps on each side, one per arm.
    let cap_corners = above
        .triangles
        .iter()
        .filter(|t| t.surface == Surface::Interior)
        .count();
    assert!(cap_corners >= 4);
}

#[test]
fn cutting_a_ring_across_fills_a_cap_with_a_hole() {
    // A 3×3 square with the middle missing, cut through its thickness.
    let mut cells = Vec::new();
    for x in 0..3 {
        for y in 0..3 {
            if (x, y) != (1, 1) {
                cells.push([x, y, 0]);
            }
        }
    }
    let ring = cubes(&cells);
    let (below, _) = assert_cut_keeps_volume(&ring, plane(Vec3::Z, 0.25));
    assert!(close(below.volume(), 2.0, 1e-5));
    // The cap covers the ring only, not the hole in the middle.
    let cap: f32 = below
        .triangles
        .iter()
        .filter(|t| t.surface == Surface::Interior)
        .map(|t| {
            let [a, b, c] = t.corners.map(|k| below.positions[k as usize]);
            (b - a).cross(c - a).length() / 2.0
        })
        .sum();
    assert!(close(cap, 8.0, 1e-5), "{cap}");
}

#[test]
fn a_plane_missing_the_mesh_keeps_it_whole() {
    let block = Mesh::block(Vec3::ONE);
    assert_eq!(block.cut(plane(Vec3::X, 5.0)), (Some(block.clone()), None));
    assert_eq!(block.cut(plane(Vec3::X, -5.0)), (None, Some(block.clone())));
    // Touching a face isn't cutting it.
    assert_eq!(block.cut(plane(Vec3::Y, 0.5)), (Some(block.clone()), None));
}

/// The plane of each triangle that isn't a sliver (whose normal f32 can't
/// tell well): its normal and a point on it.
fn face_planes(piece: &Mesh) -> Vec<(Vec3, Vec3)> {
    piece
        .triangles
        .iter()
        .filter_map(|t| {
            let [a, b, c] = t.corners.map(|k| piece.positions[k as usize]);
            let normal = (b - a).cross(c - a);
            let longest = (b - a).length().max((c - b).length()).max((a - c).length());
            (normal.length() > 1e-2 * longest * longest).then(|| (normal.normalize(), a))
        })
        .collect()
}

/// A few hundred points spread over the box from `min` to `max`.
fn sample_points(min: Vec3, max: Vec3) -> Vec<Vec3> {
    let mut state = 12345u32;
    let mut next = || {
        state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        (state >> 8) as f32 / (1 << 24) as f32
    };
    (0..600)
        .map(|_| min + (max - min) * Vec3::new(next(), next(), next()))
        .collect()
}

#[test]
fn voronoi_pieces_tile_a_block_with_convex_pieces() {
    let size = Vec3::new(4.0, 6.0, 0.2);
    let block = Mesh::block(size);
    let pieces = fracture(&block, 30, 7, Pattern::Uniform);
    assert_eq!(pieces.len(), 30);
    let total: f32 = pieces.iter().map(Mesh::volume).sum();
    assert!(close(total, block.volume(), 1e-3), "{total}");

    let planes: Vec<_> = pieces.iter().map(face_planes).collect();
    for (piece, planes) in pieces.iter().zip(&planes) {
        assert!(!piece.is_empty() && piece.volume() > 0.0);
        assert!(is_closed(piece));
        for &(normal, on) in planes {
            for &p in &piece.positions {
                assert!((p - on).dot(normal) <= 1e-4, "a piece isn't convex");
            }
        }
    }

    // No gaps, no overlaps: each point is in exactly one piece. A point
    // right on a cut may be in two by a hair, so only clearly inside counts
    // toward an overlap.
    let inside = |planes: &[(Vec3, Vec3)], p: Vec3, margin: f32| {
        planes
            .iter()
            .all(|&(normal, on)| (p - on).dot(normal) <= margin)
    };
    for p in sample_points(-size / 2.0, size / 2.0) {
        let clearly = planes.iter().filter(|pl| inside(pl, p, -1e-5)).count();
        let roughly = planes.iter().filter(|pl| inside(pl, p, 1e-5)).count();
        assert!(clearly <= 1, "{p} is in {clearly} pieces");
        assert!(roughly >= 1, "{p} is in no piece");
    }
}

#[test]
fn round_shapes_break_into_closed_pieces() {
    let size = Vec3::new(3.0, 2.0, 2.0);
    for mesh in [
        Mesh::ball(size),
        Mesh::cylinder(size),
        Mesh::wedge(size),
        Mesh::corner_wedge(size),
    ] {
        for pattern in [Pattern::Uniform, Pattern::Impact] {
            let pieces = fracture(&mesh, 20, 3, pattern);
            assert!(pieces.len() >= 15, "{} pieces", pieces.len());
            let total: f32 = pieces.iter().map(Mesh::volume).sum();
            assert!(close(total, mesh.volume(), 1e-3), "{total}");
            assert!(pieces.iter().all(is_closed));
        }
    }
}

#[test]
fn seeds_stay_in_the_bounds() {
    let block = Mesh::block(Vec3::new(4.0, 2.0, 1.0));
    for pattern in [Pattern::Uniform, Pattern::Impact] {
        let seeds = seed_points(&block, 50, 9, pattern);
        assert_eq!(seeds.len(), 50);
        let (min, max) = block.bounds();
        assert!(
            seeds
                .iter()
                .all(|s| s.cmpge(min).all() && s.cmple(max).all())
        );
    }
    // Impact crowds seeds toward the middle.
    let spread = |pattern| -> f32 {
        let seeds = seed_points(&block, 100, 1, pattern);
        seeds
            .iter()
            .map(|s| (*s / Vec3::new(2.0, 1.0, 0.5)).length())
            .sum()
    };
    assert!(spread(Pattern::Impact) < spread(Pattern::Uniform));
}

#[test]
fn the_same_seed_breaks_the_same_way() {
    let block = Mesh::block(Vec3::new(4.0, 6.0, 0.2));
    let first = fracture(&block, 12, 42, Pattern::Impact);
    assert_eq!(first, fracture(&block, 12, 42, Pattern::Impact));
    assert_ne!(first, fracture(&block, 12, 43, Pattern::Impact));
    assert_eq!(
        fracture(&block, 1, 42, Pattern::Uniform),
        vec![block.clone()]
    );
    assert_eq!(fracture(&block, 0, 42, Pattern::Uniform), vec![block]);
}

fn test_materials() -> (Vec<GlbMaterial>, GlbMaterial) {
    let outer = GlbMaterial {
        name: "Glass".into(),
        base_color: [0.2, 0.5, 0.8, 0.3],
        metallic: 0.0,
        roughness: 0.05,
        emissive: [0.0; 3],
        blend: true,
        base_color_texture: None,
    };
    let interior = GlbMaterial {
        name: "Inside".into(),
        blend: false,
        ..outer.clone()
    };
    (vec![outer], interior)
}

fn assert_near(a: [f32; 3], b: Vec3) {
    assert!(Vec3::from(a).abs_diff_eq(b, 1e-5), "{a:?} should be {b}");
}

#[test]
fn written_pieces_load_back_where_they_were() {
    let size = Vec3::new(4.0, 6.0, 0.2);
    let pieces = fracture(&Mesh::block(size), 10, 5, Pattern::Uniform);
    let (materials, interior) = test_materials();
    let bytes = write_glb(&pieces, &materials, &interior);
    assert_eq!(&bytes[..4], b"glTF");
    assert_eq!(bytes.len() % 4, 0);

    let picks = [Pick::Objects(vec!["Piece3".into()]), Pick::Whole];
    let models = load_models(&bytes, &std::env::temp_dir(), ModelFormat::Gltf, &picks)
        .expect("the pieces load");
    let (min, max) = pieces[2].bounds();
    assert_near(models[0].size, max - min);
    assert_near(models[0].center, (min + max) / 2.0);
    // Its outside and its inside.
    assert_eq!(models[0].primitives.len(), 2);
    let blends = models[0]
        .primitives
        .iter()
        .filter(|p| p.material.alpha_mode == axcenity_assets::AlphaMode::Blend)
        .count();
    assert_eq!(blends, 1);
    assert_near(models[1].size, size);
    assert_near(models[1].center, Vec3::ZERO);
}

/// An empty folder of its own for a test project.
fn temp_project(name: &str) -> PathBuf {
    let folder =
        std::env::temp_dir().join(format!("axcenity-mesh-edit-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&folder);
    std::fs::create_dir_all(&folder).expect("a temp folder");
    folder
}

#[test]
fn generate_writes_a_part_s_pieces() {
    let project = temp_project("generate");
    let assets = Assets::new(&project);
    let mut world = World::new();
    let pane = world.create(ClassName::Part).unwrap();
    world.set_name(pane, "Pane").unwrap();
    world
        .set_property(pane, "Size", Value::Vector3(Vector3::new(4.0, 6.0, 0.2)))
        .unwrap();
    world.set_parent(pane, Some(world.workspace())).unwrap();
    let settings = Settings {
        pieces: 8,
        ..Settings::default()
    };

    let path = generate_for(&world, &assets, &project, pane, &settings).unwrap();
    assert_eq!(path, "assets/models/Pane.fracture.glb");
    let piece = assets
        .model_blocking("assets/models/Pane.fracture.glb#Piece1")
        .expect("a piece loads");
    assert!(piece.size.iter().all(|&s| s > 0.0));
    let whole = assets.model_blocking(&path).expect("the whole file loads");
    assert_near(whole.size, Vec3::new(4.0, 6.0, 0.2));

    // Generating again overwrites the same file; without a PiecesId, it
    // picks a new name.
    let again = Settings {
        pieces_id: path.clone(),
        ..settings.clone()
    };
    assert_eq!(
        generate_for(&world, &assets, &project, pane, &again),
        Ok(path)
    );
    assert_eq!(
        generate_for(&world, &assets, &project, pane, &settings),
        Ok("assets/models/Pane.fracture2.glb".into())
    );

    // `generate` reads the settings of whatever it's given, and breaks its parent.
    let fracture = world.create(ClassName::Folder).unwrap();
    world.set_parent(fracture, Some(pane)).unwrap();
    assert_eq!(
        generate(&world, &assets, &project, fracture),
        Ok("assets/models/Pane.fracture3.glb".into())
    );
    let twelve = assets.model_blocking("assets/models/Pane.fracture3.glb#Piece12");
    assert!(twelve.is_some(), "12 pieces by default");

    // Only parts break.
    let folder = world.create(ClassName::Folder).unwrap();
    assert!(generate_for(&world, &assets, &project, folder, &settings).is_err());
    world
        .set_property(pane, "Shape", Value::PartType(PartType::Capsule))
        .unwrap();
    assert!(generate_for(&world, &assets, &project, pane, &settings).is_err());

    let _ = std::fs::remove_dir_all(&project);
}

#[test]
fn generate_breaks_a_mesh_part_next_to_its_model() {
    let project = temp_project("mesh-part");
    // A model made of the pieces of a block, so its own file.
    let (materials, interior) = test_materials();
    let source = write_glb(
        &[Mesh::block(Vec3::new(2.0, 1.0, 1.0))],
        &materials,
        &interior,
    );
    std::fs::create_dir_all(project.join("assets/models/props")).unwrap();
    std::fs::write(project.join("assets/models/props/crate.glb"), source).unwrap();

    let assets = Assets::new(&project);
    let mut world = World::new();
    let part = world.create(ClassName::MeshPart).unwrap();
    world.set_name(part, "Crate").unwrap();
    world
        .set_property(
            part,
            "MeshId",
            Value::String("assets/models/props/crate.glb".into()),
        )
        .unwrap();
    let path = generate_for(&world, &assets, &project, part, &Settings::default()).unwrap();
    assert_eq!(path, "assets/models/props/Crate.fracture.glb");
    let whole = assets.model_blocking(&path).expect("the pieces load");
    assert_near(whole.size, Vec3::new(2.0, 1.0, 1.0));

    let _ = std::fs::remove_dir_all(&project);
}
