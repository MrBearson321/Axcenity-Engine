//! Solid modeling (Phase 50): the booleans and combining parts.

use std::f32::consts::PI;

use axcenity_assets::Assets;
use axcenity_core::{ClassName, Color3, InstanceId, PartType, Value, Vector3, World};
use glam::{Quat, Vec3};

use crate::*;

fn close(a: f32, b: f32, within: f32) -> bool {
    (a - b).abs() <= within * a.abs().max(b.abs()).max(1.0)
}

fn moved(mut mesh: Mesh, by: Vec3) -> Mesh {
    for position in &mut mesh.positions {
        *position += by;
    }
    mesh
}

fn with_surface(mut mesh: Mesh, surface: u32) -> Mesh {
    for triangle in &mut mesh.triangles {
        triangle.surface = Surface::Outer(surface);
    }
    mesh
}

#[test]
fn a_union_of_overlapping_blocks_is_closed_and_counts_the_overlap_once() {
    let a = Mesh::block(Vec3::splat(2.0));
    let b = moved(Mesh::block(Vec3::splat(2.0)), Vec3::new(1.0, 0.0, 0.0));
    let union = boolean(&a, &b, Operation::Union);
    assert_eq!(union.open_edges(), 0);
    // 8 + 8 - the 1×2×2 overlap.
    assert!(close(union.volume(), 12.0, 1e-4), "{}", union.volume());
    assert_eq!(
        union.bounds(),
        (Vec3::new(-1.0, -1.0, -1.0), Vec3::new(2.0, 1.0, 1.0))
    );
}

#[test]
fn subtracting_a_block_through_a_wall_leaves_a_doorway() {
    let wall = with_surface(Mesh::block(Vec3::new(12.0, 8.0, 1.0)), 0);
    // 4 wide, 6 tall, standing on the wall's bottom and sticking out of
    // both faces.
    let door = with_surface(
        moved(
            Mesh::block(Vec3::new(4.0, 6.0, 3.0)),
            Vec3::new(0.0, -1.0, 0.0),
        ),
        1,
    );
    let result = boolean(&wall, &door, Operation::Subtract);
    assert_eq!(result.open_edges(), 0);
    assert!(
        close(result.volume(), 96.0 - 24.0, 1e-4),
        "{}",
        result.volume()
    );
    // The doorway's sides are the door's faces, turned to face into it.
    let sides: Vec<&Triangle> = result
        .triangles
        .iter()
        .filter(|t| t.surface == Surface::Outer(1))
        .collect();
    assert!(!sides.is_empty());
    for triangle in sides {
        let [a, b, c] = triangle.corners.map(|k| result.positions[k as usize]);
        let normal = (b - a).cross(c - a).normalize();
        let middle = (a + b + c) / 3.0;
        // Toward the doorway's middle line, x = 0, y = -1.
        let inward = Vec3::new(-middle.x, -1.0 - middle.y, 0.0);
        assert!(normal.dot(inward) > 0.0, "{normal} at {middle}");
    }

    // The result can be combined again: cutting a window through it too.
    let window = moved(
        Mesh::block(Vec3::new(2.0, 2.0, 3.0)),
        Vec3::new(4.0, 1.0, 0.0),
    );
    let again = boolean(&result, &window, Operation::Subtract);
    assert_eq!(again.open_edges(), 0);
    assert!(
        close(again.volume(), 72.0 - 4.0, 1e-4),
        "{}",
        again.volume()
    );
}

#[test]
fn an_intersection_keeps_only_the_overlap() {
    let a = Mesh::block(Vec3::splat(2.0));
    let b = moved(Mesh::block(Vec3::splat(2.0)), Vec3::new(1.0, 1.0, 0.0));
    let both = boolean(&a, &b, Operation::Intersect);
    assert_eq!(both.open_edges(), 0);
    assert!(close(both.volume(), 2.0, 1e-4), "{}", both.volume());

    let far = moved(Mesh::block(Vec3::splat(2.0)), Vec3::new(5.0, 0.0, 0.0));
    assert!(boolean(&a, &far, Operation::Intersect).is_empty());
}

#[test]
fn subtracting_a_ball_from_a_block_makes_a_crater() {
    let block = Mesh::block(Vec3::new(8.0, 4.0, 8.0));
    // A ball of radius 2 centered on the top face: half of it is cut out.
    let ball = moved(Mesh::ball(Vec3::splat(4.0)), Vec3::new(0.0, 2.0, 0.0));
    let crater = boolean(&block, &ball, Operation::Subtract);
    assert_eq!(crater.open_edges(), 0);
    let half_ball = ball.volume() / 2.0;
    assert!(
        close(crater.volume(), 256.0 - half_ball, 1e-3),
        "{} should be {}",
        crater.volume(),
        256.0 - half_ball
    );
    assert!(half_ball > 0.9 * 2.0 / 3.0 * PI * 8.0);
}

#[test]
fn booleans_are_deterministic() {
    let block = Mesh::block(Vec3::new(8.0, 4.0, 8.0));
    let ball = moved(Mesh::ball(Vec3::splat(5.0)), Vec3::new(1.0, 2.0, 0.5));
    let first = boolean(&block, &ball, Operation::Subtract);
    let second = boolean(&block, &ball, Operation::Subtract);
    assert_eq!(first, second);
}

#[test]
fn a_mesh_with_a_hole_has_open_edges() {
    let mut block = Mesh::block(Vec3::ONE);
    assert_eq!(block.open_edges(), 0);
    block.triangles.pop();
    assert_eq!(block.open_edges(), 3);
}

fn part(world: &mut World, name: &str, class: ClassName, size: Vec3, at: Vec3) -> InstanceId {
    let id = world.create(class).unwrap();
    world.set_name(id, name).unwrap();
    let v = |v: Vec3| Value::Vector3(Vector3::new(v.x, v.y, v.z));
    world.set_property(id, "Size", v(size)).unwrap();
    world.set_property(id, "Position", v(at)).unwrap();
    world.set_parent(id, Some(world.workspace())).unwrap();
    id
}

fn temp_project(name: &str) -> std::path::PathBuf {
    let folder = std::env::temp_dir().join(format!("axcenity-csg-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&folder);
    std::fs::create_dir_all(&folder).expect("a temp folder");
    folder
}

#[test]
fn combining_parts_cuts_out_the_negated_ones_in_the_first_part_s_frame() {
    let project = temp_project("combine");
    let assets = Assets::new(&project);
    let mut world = World::new();
    let wall = part(
        &mut world,
        "Wall",
        ClassName::Part,
        Vec3::new(12.0, 8.0, 1.0),
        Vec3::new(10.0, 4.0, 0.0),
    );
    world
        .set_property(
            wall,
            "Orientation",
            Value::Vector3(Vector3::new(0.0, 90.0, 0.0)),
        )
        .unwrap();
    world
        .set_property(wall, "Color", Value::Color3(Color3::new(1.0, 0.0, 0.0)))
        .unwrap();
    // The doorway, in world space: the wall runs along Z once turned.
    let door = part(
        &mut world,
        "Door",
        ClassName::NegateOperation,
        Vec3::new(3.0, 6.0, 4.0),
        Vec3::new(10.0, 3.0, 0.0),
    );
    let combined =
        combine_parts(&world, &assets, &[wall, door], Operation::Union).expect("a union");
    assert_eq!(combined.orientation(), Vector3::new(0.0, 90.0, 0.0));
    assert!((combined.position - Vec3::new(10.0, 4.0, 0.0)).length() < 1e-4);
    assert!((combined.size - Vec3::new(12.0, 8.0, 1.0)).length() < 1e-4);
    assert!(close(combined.mesh.volume(), 96.0 - 24.0, 1e-4));
    // A look for the wall and one for the doorway's sides.
    assert_eq!(combined.materials.len(), 2);

    // Saved, it loads at its size, and combines again as a model.
    let path = save_union(&project, &assets, "Wall", &combined).unwrap();
    assert_eq!(path, "assets/models/Wall.union.glb");
    let model = assets.model_blocking(&path).expect("it loads");
    let size = Vec3::from(model.size);
    assert!((size - combined.size).length() < 1e-4, "{size}");
    let union = part(
        &mut world,
        "Union",
        ClassName::UnionOperation,
        combined.size,
        combined.position,
    );
    world
        .set_property(union, "MeshId", Value::String(path))
        .unwrap();
    world
        .set_property(union, "Orientation", Value::Vector3(combined.orientation()))
        .unwrap();
    let solid = solid_of(&world, &assets, union).expect("the union is closed");
    assert!(close(solid.mesh.volume(), 72.0, 1e-4));
    assert_eq!(solid.rotation, Quat::from_rotation_y(PI / 2.0));

    let _ = std::fs::remove_dir_all(&project);
}

#[test]
fn a_part_that_isn_t_closed_or_can_t_be_meshed_is_refused_with_why() {
    let project = temp_project("refused");
    let assets = Assets::new(&project);
    // An open box: one face missing.
    let mut open = Mesh::block(Vec3::ONE);
    open.triangles.truncate(10);
    std::fs::create_dir_all(project.join("assets/models")).unwrap();
    let plain = GlbMaterial {
        name: "Plain".into(),
        base_color: [1.0; 4],
        metallic: 0.0,
        roughness: 0.5,
        emissive: [0.0; 3],
        blend: false,
        base_color_texture: None,
    };
    let bytes = write_glb(&[open], std::slice::from_ref(&plain), &plain);
    std::fs::write(project.join("assets/models/open.glb"), bytes).unwrap();

    let mut world = World::new();
    let block = part(
        &mut world,
        "Block",
        ClassName::Part,
        Vec3::ONE * 2.0,
        Vec3::ZERO,
    );
    let mesh = part(
        &mut world,
        "Cup",
        ClassName::MeshPart,
        Vec3::ONE,
        Vec3::ZERO,
    );
    world
        .set_property(
            mesh,
            "MeshId",
            Value::String("assets/models/open.glb".into()),
        )
        .unwrap();
    let err = combine_parts(&world, &assets, &[block, mesh], Operation::Union).unwrap_err();
    assert!(err.contains("`Cup` isn't a closed solid"), "{err}");

    world
        .set_property(block, "Shape", Value::PartType(PartType::Capsule))
        .unwrap();
    let err = combine_parts(&world, &assets, &[block], Operation::Union).unwrap_err();
    assert!(err.contains("Capsules can't be combined"), "{err}");

    let _ = std::fs::remove_dir_all(&project);
}

/// How many distinct positions `mesh`'s triangles use.
fn points_used(mesh: &Mesh) -> usize {
    let mut points: Vec<[u32; 3]> = mesh
        .triangles
        .iter()
        .flat_map(|t| t.corners.map(|c| position_key(mesh.positions[c as usize])))
        .collect();
    points.sort_unstable();
    points.dedup();
    points.len()
}

#[test]
fn two_blocks_side_by_side_with_one_look_union_into_a_plain_box() {
    let block = |at: Vec3| Solid {
        name: "Block".into(),
        mesh: Mesh::block(Vec3::splat(2.0)),
        materials: vec![crate::glb::plain_material()],
        position: at,
        rotation: Quat::IDENTITY,
    };
    let combined = combine(
        &[block(Vec3::ZERO), block(Vec3::new(2.0, 0.0, 0.0))],
        &[],
        Operation::Union,
    )
    .unwrap();
    let mesh = &combined.mesh;
    // A 4×2×2 box: 8 corners and two triangles a side, with no seam
    // where the blocks met.
    assert_eq!(mesh.open_edges(), 0);
    assert_eq!(points_used(mesh), 8);
    assert_eq!(mesh.triangles.len(), 12);
    assert!(close(mesh.volume(), 16.0, 1e-4));
    // Each side's uvs run from 0 to 1 across it again.
    for uv in &mesh.uvs {
        assert!(
            uv.min_element() >= -1e-4 && uv.max_element() <= 1.0 + 1e-4,
            "{uv}"
        );
    }
}

#[test]
fn a_cut_keeps_the_corners_a_shape_needs_and_no_more() {
    // Overlapping blocks: the corners of the L they make, and no others.
    let a = Mesh::block(Vec3::splat(2.0));
    let b = moved(Mesh::block(Vec3::splat(2.0)), Vec3::new(1.0, 0.5, 0.3));
    let union = boolean(&a, &b, Operation::Union);
    assert_eq!(points_used(&union), 20);
    // A cylinder bored through a block: the hole's rim on each side and the
    // block's corners. Cut apart, each of the cylinder's side quads used to
    // add a corner where its diagonal crossed the block's faces.
    let rod = Mesh::cylinder(Vec3::new(3.0, 1.0, 1.0));
    let sides = rod.triangles.len() / 4;
    let bored = boolean(&a, &rod, Operation::Subtract);
    assert_eq!(bored.open_edges(), 0);
    assert_eq!(points_used(&bored), 8 + 2 * sides);
}

#[test]
fn flat_faces_are_filled_without_needle_thin_triangles_where_they_can_be() {
    // A block with a ball scooped out of a corner, and a wall with a door:
    // the flat faces left have no triangle thinner than it must be.
    let block = Mesh::block(Vec3::splat(2.0));
    let ball = moved(Mesh::ball(Vec3::splat(2.0)), Vec3::ONE);
    let wall = Mesh::block(Vec3::new(12.0, 8.0, 1.0));
    let door = moved(
        Mesh::block(Vec3::new(4.0, 6.0, 3.0)),
        Vec3::new(0.0, -1.0, 0.0),
    );
    for mesh in [
        boolean(&block, &ball, Operation::Subtract),
        boolean(&wall, &door, Operation::Subtract),
    ] {
        for triangle in &mesh.triangles {
            let [a, b, c] = triangle.corners.map(|k| mesh.positions[k as usize]);
            let area = (b - a).cross(c - a).length() / 2.0;
            let longest = (b - a).length().max((c - b).length()).max((a - c).length());
            assert!(area / (longest * longest) > 0.02, "{a} {b} {c}");
        }
    }
}
