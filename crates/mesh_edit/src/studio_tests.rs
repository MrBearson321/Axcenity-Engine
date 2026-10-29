//! Phase 55: the Model Studio's editable meshes: New shapes, selection,
//! loops, moves, and opening and saving `.glb` files.

use std::path::{Path, PathBuf};

use axcenity_assets::{Model, ModelFormat, load_model};
use glam::{Mat4, Vec3};

use crate::{EditMesh, ModelDoc, PrimitiveSettings, PrimitiveShape, SelectMode, Selection};

fn shape(shape: PrimitiveShape) -> EditMesh {
    PrimitiveSettings::of(shape).build()
}

/// The space a closed mesh encloses (positive when its faces face out).
fn volume(mesh: &EditMesh) -> f32 {
    (0..mesh.face_count() as u32)
        .flat_map(|face| mesh.triangles(face))
        .map(|t| {
            let [a, b, c] = t.map(|e| mesh.position(mesh.half_edge(e).vertex));
            a.dot(b.cross(c)) / 6.0
        })
        .sum()
}

#[test]
fn every_closed_new_shape_is_watertight_and_faces_out() {
    for kind in PrimitiveShape::ALL {
        let mesh = shape(kind);
        if kind == PrimitiveShape::Plane {
            continue;
        }
        assert!(
            mesh.half_edges().iter().all(|edge| edge.twin.is_some()),
            "{kind:?} has an open edge"
        );
        assert!(volume(&mesh) > 0.0, "{kind:?} faces inward");
        // Euler's formula for a sphere-like shape, or a torus (V - E + F = 0).
        let euler =
            mesh.vertex_count() as i64 - mesh.edges().len() as i64 + mesh.face_count() as i64;
        let expected = if kind == PrimitiveShape::Torus { 0 } else { 2 };
        assert_eq!(euler, expected, "{kind:?}");
    }
}

#[test]
fn new_shapes_have_their_sizes_and_segments() {
    let cube = shape(PrimitiveShape::Cube);
    assert_eq!(
        (cube.vertex_count(), cube.edges().len(), cube.face_count()),
        (8, 12, 6)
    );
    assert!((volume(&cube) - 8.0).abs() < 1e-4);

    let plane = shape(PrimitiveShape::Plane);
    assert_eq!(plane.face_count(), 1);
    assert!(plane.face_normal(0).abs_diff_eq(Vec3::Y, 1e-6));

    let mut settings = PrimitiveSettings::of(PrimitiveShape::Cylinder);
    settings.segments = 12;
    settings.rings = 3;
    let cylinder = settings.build();
    // 3 bands of 12 quads, and two 12-sided caps.
    assert_eq!(cylinder.face_count(), 3 * 12 + 2);
    assert_eq!(cylinder.vertex_count(), 4 * 12);

    let sphere = shape(PrimitiveShape::UvSphere);
    let corner = sphere
        .positions()
        .iter()
        .map(|p| p.length())
        .fold(0.0, f32::max);
    assert!((corner - 1.0).abs() < 1e-5);
    let ico = shape(PrimitiveShape::IcoSphere);
    assert_eq!(ico.face_count(), 20 * 4 * 4);
}

/// The cylinder's edges running round it at height `y`.
fn ring_edges(mesh: &EditMesh, y: f32) -> Vec<u32> {
    mesh.edges()
        .into_iter()
        .filter(|&edge| {
            let (a, b) = mesh.ends(edge);
            (mesh.position(a).y - y).abs() < 1e-5 && (mesh.position(b).y - y).abs() < 1e-5
        })
        .collect()
}

#[test]
fn loop_select_picks_a_whole_edge_loop_on_a_cylinder() {
    let mut settings = PrimitiveSettings::of(PrimitiveShape::Cylinder);
    settings.rings = 3;
    let cylinder = settings.build();
    let mut middle = ring_edges(&cylinder, -1.0 / 3.0);
    assert_eq!(middle.len(), 32);

    let mut selection = Selection {
        mode: SelectMode::Edge,
        ..Selection::default()
    };
    selection.select_loop(&cylinder, middle[5], false);
    middle.sort_unstable();
    assert_eq!(selection.edges.iter().copied().collect::<Vec<_>>(), middle);

    // The rim, where the side meets the cap, is a loop too.
    let mut rim = ring_edges(&cylinder, 1.0);
    rim.sort_unstable();
    selection.select_loop(&cylinder, rim[0], false);
    assert_eq!(selection.edges.iter().copied().collect::<Vec<_>>(), rim);

    // Up the side, the loop runs from cap to cap: one edge per band.
    let side = cylinder
        .edges()
        .into_iter()
        .find(|&edge| {
            let (a, b) = cylinder.ends(edge);
            (cylinder.position(a).y - cylinder.position(b).y).abs() > 0.1
        })
        .unwrap();
    selection.select_loop(&cylinder, side, false);
    assert_eq!(selection.edges.len(), 3);

    // In Vertex mode the loop's vertices; in Face mode the faces it crosses.
    selection.mode = SelectMode::Vertex;
    selection.select_loop(&cylinder, middle[0], false);
    assert_eq!(selection.vertices.len(), 32);
    selection.mode = SelectMode::Face;
    selection.select_loop(&cylinder, side, false);
    assert_eq!(selection.faces.len(), 32, "a ring of side faces");
}

#[test]
fn grow_shrink_linked_and_mode_switches() {
    let cube = shape(PrimitiveShape::Cube);
    let mut selection = Selection::default();
    selection.click(Some(0), false);
    selection.grow(&cube);
    // The three faces around a corner reach seven of the eight corners.
    assert_eq!(selection.vertices.len(), 7);
    selection.shrink(&cube);
    assert_eq!(
        selection.vertices.iter().copied().collect::<Vec<_>>(),
        vec![0]
    );

    selection.linked(&cube);
    assert_eq!(selection.vertices.len(), 8);
    selection.convert(&cube, SelectMode::Face);
    assert_eq!(selection.faces.len(), 6);
    selection.click(Some(2), false);
    selection.convert(&cube, SelectMode::Edge);
    assert_eq!(selection.edges.len(), 4);
    selection.convert(&cube, SelectMode::Vertex);
    assert_eq!(selection.vertices.len(), 4);

    // Shift-click adds, and takes off again.
    let other = (0..8).find(|v| !selection.vertices.contains(v)).unwrap();
    selection.click(Some(other), true);
    assert!(selection.vertices.contains(&other));
    assert_eq!(selection.vertices.len(), 5);
    selection.click(Some(other), true);
    assert!(!selection.vertices.contains(&other));
}

#[test]
fn scaling_a_face_moves_its_corners_and_reshades_its_neighbours() {
    let mut cube = shape(PrimitiveShape::Cube);
    let top = (0..6).find(|&f| cube.face_normal(f).y > 0.9).unwrap();
    let selection = Selection {
        mode: SelectMode::Face,
        faces: [top].into(),
        ..Selection::default()
    };
    let center = selection.center(&cube).unwrap();
    assert!(center.abs_diff_eq(Vec3::Y, 1e-6));
    let scale = Mat4::from_translation(center)
        * Mat4::from_scale(Vec3::splat(2.0))
        * Mat4::from_translation(-center);
    cube.transform(&selection.vertices_of(&cube), scale);
    let max = cube.positions().iter().fold(Vec3::ZERO, |m, p| m.max(*p));
    assert!(max.abs_diff_eq(Vec3::new(2.0, 1.0, 2.0), 1e-5));
    // The sides now lean out, and their corners' normals say so.
    let side = (0..6).find(|&f| cube.face_normal(f).x > 0.5).unwrap();
    let normal = cube.face_normal(side);
    assert!(normal.y < -0.1);
    for edge in cube.face_half_edges(side) {
        assert!(cube.half_edge(edge).normal.abs_diff_eq(normal, 1e-5));
    }
}

fn temp_folder(name: &str) -> PathBuf {
    let folder =
        std::env::temp_dir().join(format!("axcenity-studio-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&folder);
    std::fs::create_dir_all(&folder).unwrap();
    folder
}

/// Every triangle corner of `model` in its file's units: position, uv and
/// normal, rounded so float noise doesn't count, sorted.
/// A triangle corner's position, uv and normal, rounded.
type Corner = ([i64; 3], [i64; 2], [i64; 3]);

fn corners(model: &Model) -> Vec<Vec<Corner>> {
    let size = Vec3::from(model.size);
    let scale = Vec3::select(size.cmpgt(Vec3::ZERO), size, Vec3::ONE);
    let center = Vec3::from(model.center);
    model
        .primitives
        .iter()
        .map(|primitive| {
            let mut corners: Vec<_> = primitive
                .indices
                .iter()
                .map(|&k| {
                    let k = k as usize;
                    let p = Vec3::from(primitive.positions[k]) * scale + center;
                    let uv = primitive.uvs.get(k).copied().unwrap_or_default();
                    let n = Vec3::from(primitive.normals[k]);
                    (
                        (p * 1e4).round().as_i64vec3().to_array(),
                        uv.map(|c| (c * 1e4).round() as i64),
                        (n * 1e3).round().as_i64vec3().to_array(),
                    )
                })
                .collect();
            corners.sort_unstable();
            corners
        })
        .collect()
}

#[test]
fn a_new_cube_saves_as_a_glb_that_loads_back_the_same() {
    let doc = ModelDoc::primitive(&PrimitiveSettings::default());
    let bytes = doc.write_glb();
    let folder = temp_folder("cube");
    let model = load_model(&bytes, &folder, ModelFormat::Gltf).unwrap();
    assert_eq!(model.size, [2.0, 2.0, 2.0]);
    assert_eq!(model.primitives.len(), 1);
    assert_eq!(model.primitives[0].indices.len(), 12 * 3);
    assert_eq!(model.primitives[0].material.name, "Material");
    let objects = axcenity_assets::list_objects(&bytes, ModelFormat::Gltf).unwrap();
    assert_eq!(objects[0].path, "Cube");

    // Opened and saved again, nothing changes.
    let reopened = ModelDoc::open(&bytes, &folder, ModelFormat::Gltf).unwrap();
    let again = load_model(&reopened.write_glb(), &folder, ModelFormat::Gltf).unwrap();
    assert_eq!(corners(&again), corners(&model));
    let _ = std::fs::remove_dir_all(&folder);
}

fn example(path: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(path)
}

#[test]
fn a_blender_model_opened_and_saved_unchanged_keeps_its_geometry_uvs_and_materials() {
    // DamagedHelmet was exported from Blender with its textures embedded;
    // importing copies them out, as the editor does.
    let project = temp_folder("helmet");
    let source = example("examples/hello-pbr/assets/models/DamagedHelmet/DamagedHelmet.glb");
    let (mesh_id, original) = axcenity_assets::import_model(&project, &source).unwrap();
    let path = project.join(&mesh_id);
    let folder = path.parent().unwrap();
    let bytes = std::fs::read(&path).unwrap();

    let doc = ModelDoc::open(&bytes, folder, ModelFormat::Gltf).unwrap();
    let saved = doc.write_glb();
    let model = load_model(&saved, folder, ModelFormat::Gltf).unwrap();

    assert_eq!(corners(&model), corners(&original));
    for (a, b) in model.primitives.iter().zip(&original.primitives) {
        let (a, b) = (&a.material, &b.material);
        assert_eq!(a.name, b.name);
        assert_eq!(a.base_color, b.base_color);
        assert_eq!((a.metalness, a.roughness), (b.metalness, b.roughness));
        assert_eq!(a.emissive, b.emissive);
        assert_eq!(a.alpha_mode, b.alpha_mode);
        assert_eq!(a.double_sided, b.double_sided);
        let file = |image: Option<&std::sync::Arc<axcenity_assets::Image>>| {
            image.and_then(|image| image.file.clone())
        };
        assert!(file(b.base_color_texture.as_ref()).is_some());
        assert_eq!(
            file(a.base_color_texture.as_ref()),
            file(b.base_color_texture.as_ref())
        );
        assert_eq!(
            file(a.normal_texture.as_ref()),
            file(b.normal_texture.as_ref())
        );
        assert_eq!(
            file(a.emissive_texture.as_ref()),
            file(b.emissive_texture.as_ref())
        );
        let channel = |c: &Option<axcenity_assets::ChannelTexture>| {
            c.as_ref()
                .and_then(|c| c.image.file.clone().map(|f| (f, c.channel)))
        };
        assert_eq!(channel(&a.roughness_texture), channel(&b.roughness_texture));
        assert_eq!(channel(&a.metalness_texture), channel(&b.metalness_texture));
        assert_eq!(channel(&a.occlusion_texture), channel(&b.occlusion_texture));
    }
    let _ = std::fs::remove_dir_all(&project);
}

#[test]
fn groups_are_saved_so_object_paths_still_pick_the_same_objects() {
    let mut doc = ModelDoc::primitive(&PrimitiveSettings::default());
    let mut sphere = ModelDoc::primitive(&PrimitiveSettings::of(PrimitiveShape::UvSphere)).objects;
    sphere[0].depth = 1;
    doc.objects[0].mesh = None;
    doc.objects[0].name = "Group".to_owned();
    doc.objects.extend(sphere);
    let bytes = doc.write_glb();
    let paths: Vec<String> = axcenity_assets::list_objects(&bytes, ModelFormat::Gltf)
        .unwrap()
        .into_iter()
        .map(|object| object.path)
        .collect();
    assert_eq!(paths, vec!["Group".to_owned(), "Group/UVSphere".to_owned()]);
}
