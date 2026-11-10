//! Phase 61: retopology and baking. The sculpted face of Phase 60,
//! retopologised under 5k triangles and baked, drawn from its saved `.glb`
//! as a game does, against the sculpt's own normals; no seams where the
//! uvs don't overlap, and a warning naming the islands where they do;
//! DirectX normal maps; snapping, Poly Build and Decimate to a count.

use axcenity_assets::{Model, ModelFormat, ModelMaterial, Pick, Primitive};
use glam::{Vec2, Vec3};

use crate::bake::{BakeSettings, corner_tangents};
use crate::half_edge::EditMesh;
use crate::model_doc::{DocObject, ModelDoc};
use crate::paint::sample;
use crate::retopo::{PolyStep, Reference, decimate_to, poly_build, relax};
use crate::sculpt::bvh::closest_on_triangle;
use crate::sculpt::subdivide;
use crate::sculpt_tests::sculpted_face;
use crate::{PrimitiveSettings, PrimitiveShape, check_manifold, uv};

const CLAY: [f32; 4] = [0.6, 0.3, 0.2, 1.0];

fn object(name: &str, mesh: EditMesh, material: &str) -> DocObject {
    DocObject {
        name: name.to_owned(),
        depth: 0,
        mesh: Some(mesh),
        materials: vec![ModelMaterial {
            name: material.to_owned(),
            base_color: CLAY,
            roughness: 0.6,
            ..ModelMaterial::default()
        }],
        uvs: true,
        modifiers: Vec::new(),
        paint: Vec::new(),
        skin: None,
    }
}

/// Every face of `mesh`.
fn all_faces(mesh: &EditMesh) -> Vec<u32> {
    (0..mesh.face_count() as u32).collect()
}

/// The sculpted face made finer (Catmull-Clark, about 28k triangles), and
/// a copy of it decimated to `triangles`, relaxed on it and unwrapped by
/// Smart UV Project at 45° (as Make Low Poly does): the quick retopology.
fn face_doc(triangles: usize) -> ModelDoc {
    let high = subdivide(&sculpted_face()).unwrap();
    let mut low = decimate_to(&high, triangles);
    relax(&mut low, &Reference::new(&high), 3);
    let faces = all_faces(&low);
    uv::smart_project(&mut low, &faces, 45.0, 0.02, 0).unwrap();
    ModelDoc {
        objects: vec![
            object("Face", high, "Clay"),
            object("Face_Low", low, "Skin"),
        ],
        armature: None,
    }
}

fn triangle_count(mesh: &EditMesh) -> usize {
    (0..mesh.face_count() as u32)
        .map(|face| mesh.triangles(face).len())
        .sum()
}

/// A number from 0 to 1, the same each run.
struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> f32 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (self.0 >> 40) as f32 / (1u64 << 24) as f32
    }
}

/// The normal a normal map gives as the engine's shader works it out
/// (`surface_normal` in `shader.wgsl`): the blended normal `n`, tangent
/// and side, and the map's texel.
fn decode(n: Vec3, tangent: Vec3, side: f32, texel: [f32; 4], flip_green: bool) -> Vec3 {
    let n = n.normalize();
    let t = (tangent - n * n.dot(tangent)).normalize();
    let b = n.cross(t) * side;
    let mut v = Vec3::new(texel[0], texel[1], texel[2]) / 255.0 * 2.0 - 1.0;
    if flip_green {
        v.y = -v.y;
    }
    v.z = v.z.max(0.0);
    (t * v.x + b * v.y + n * v.z).normalize()
}

fn degrees(a: Vec3, b: Vec3) -> f32 {
    a.dot(b).clamp(-1.0, 1.0).acos().to_degrees()
}

/// Saves `doc` (and its baked `files`) to a new folder and loads object
/// `name` from the `.glb` as a MeshPart picking it does.
fn load_saved(doc: &ModelDoc, files: &[(String, Vec<u8>)], name: &str, tag: &str) -> Model {
    let folder = std::env::temp_dir().join(format!("axcenity-bake-{tag}-{}", std::process::id()));
    let models = folder.join("assets/models");
    std::fs::create_dir_all(models.join("textures")).unwrap();
    for (path, bytes) in files {
        std::fs::write(folder.join(path), bytes).unwrap();
    }
    let glb = doc.write_glb();
    let pick = Pick::Objects(vec![name.to_owned()]);
    let model = axcenity_assets::load_models(&glb, &models, ModelFormat::Gltf, &[pick])
        .unwrap()
        .remove(0);
    let _ = std::fs::remove_dir_all(&folder);
    model
}

/// One corner of a loaded primitive, back in the file's units (a loaded
/// model is fitted into the 1×1×1 cube; a MeshPart its size undoes that).
struct Corner {
    position: Vec3,
    normal: Vec3,
    tangent: Vec3,
    side: f32,
    uv: Vec2,
}

fn corner(model: &Model, primitive: &Primitive, index: u32) -> Corner {
    let size = Vec3::from(model.size);
    let k = index as usize;
    let tangent = primitive.tangents[k];
    Corner {
        position: Vec3::from(primitive.positions[k]) * size + Vec3::from(model.center),
        // Normals turn the other way to positions under a stretch.
        normal: (Vec3::from(primitive.normals[k]) / size).normalize(),
        tangent: (Vec3::from_slice(&tangent[..3]) * size).normalize(),
        side: tangent[3],
        uv: Vec2::from(primitive.uvs[k]),
    }
}

#[test]
fn the_sculpted_face_retopologised_under_5k_triangles_and_baked_shows_the_sculpts_normals_in_the_game()
 {
    let mut doc = face_doc(4000);
    let high = doc.objects[0].mesh.clone().unwrap();
    let low = doc.objects[1].mesh.clone().unwrap();
    assert!(triangle_count(&high) > 25_000, "{}", triangle_count(&high));
    let low_triangles = triangle_count(&low);
    assert!(
        low_triangles < 5000 && low_triangles > 3000,
        "{low_triangles} triangles"
    );
    assert!(
        uv::overlapping_islands(&low, &all_faces(&low), 0).is_empty(),
        "Smart UV Project packs without overlaps"
    );

    let settings = BakeSettings {
        size: 1024,
        occlusion_rays: 8,
        ..BakeSettings::default()
    };
    let report = doc.bake("assets/models/Face.glb", 1, 0, &settings).unwrap();
    let names: Vec<&str> = report.files.iter().map(|(path, _)| path.as_str()).collect();
    assert_eq!(
        names,
        [
            "assets/models/textures/Face_Face_Low_Skin_BaseColor.png",
            "assets/models/textures/Face_Face_Low_Skin_Normal.png",
            "assets/models/textures/Face_Face_Low_Skin_Occlusion.png",
            "assets/models/textures/Face_Face_Low_Skin_Curvature.png",
        ]
    );
    assert!(report.overlaps.is_empty());
    assert!(report.warnings().is_empty(), "{:?}", report.warnings());

    // Loaded from the saved `.glb` as a game loads it.
    let model = load_saved(&doc, &report.files, "Face_Low", "face");
    assert_eq!(model.primitives.len(), 1);
    let primitive = &model.primitives[0];
    let material = &primitive.material;
    let normal_map = material.normal_texture.as_ref().expect("the normal map");
    assert!(!material.normal_flip_green);
    assert!(material.occlusion_texture.is_some());
    assert!(material.base_color_texture.is_some());
    assert_eq!(primitive.tangents.len(), primitive.positions.len());

    // At points spread over the low-poly surface: the normal the game
    // draws there, against the sculpt's own (at its nearest point).
    let reference = Reference::new(&high);
    let mut random = Lcg(61);
    let (mut baked_error, mut plain_error, mut worst) = (0.0, 0.0, Vec::new());
    let samples = 3000;
    for _ in 0..samples {
        let triangle = (random.next() * (primitive.indices.len() / 3) as f32) as usize;
        let (mut a, mut b) = (random.next(), random.next());
        if a + b > 1.0 {
            (a, b) = (1.0 - a, 1.0 - b);
        }
        let weights = [1.0 - a - b, a, b];
        let corners =
            [0, 1, 2].map(|k| corner(&model, primitive, primitive.indices[triangle * 3 + k]));
        let blend3 = |f: &dyn Fn(&Corner) -> Vec3| {
            corners
                .iter()
                .zip(weights)
                .map(|(c, w)| f(c) * w)
                .sum::<Vec3>()
        };
        let position = blend3(&|c| c.position);
        let normal = blend3(&|c| c.normal).normalize();
        let tangent = blend3(&|c| c.tangent);
        let side: f32 = corners.iter().zip(weights).map(|(c, w)| c.side * w).sum();
        let uv: Vec2 = corners.iter().zip(weights).map(|(c, w)| c.uv * w).sum();
        let texel = sample(
            normal_map.rgba(),
            normal_map.width,
            normal_map.height,
            uv,
            true,
        );
        let drawn = decode(normal, tangent, side, texel, false);
        let sculpt = reference.normal(&reference.nearest(position).unwrap());
        let error = degrees(drawn, sculpt);
        baked_error += error;
        plain_error += degrees(normal, sculpt);
        worst.push(error);
    }
    let (baked_error, plain_error) = (baked_error / samples as f32, plain_error / samples as f32);
    worst.sort_by(f32::total_cmp);
    let p95 = worst[samples * 95 / 100];
    println!(
        "mean error: baked {baked_error:.2}°, low-poly normals alone {plain_error:.2}°; 95th percentile baked {p95:.2}°"
    );
    assert!(baked_error < 2.5, "baked normals {baked_error}° off");
    assert!(p95 < 6.0, "95% within {p95}°");
    assert!(
        baked_error < plain_error * 0.5,
        "the bake adds the detail: {baked_error}° against {plain_error}°"
    );

    // The other maps: the brow bulges (brighter curvature) and the mouth
    // is a crease (darker curvature, more occluded) than the forehead.
    let map = |role: &str| -> (Vec<u8>, u32) {
        let (_, bytes) = report
            .files
            .iter()
            .find(|(path, _)| path.ends_with(&format!("_{role}.png")))
            .unwrap();
        let image = image::load_from_memory(bytes).unwrap().into_rgba8();
        (image.as_raw().clone(), image.width())
    };
    let at = |role: &str, toward: Vec3| -> f32 {
        let (pixels, size) = map(role);
        let (on, _) = reference
            .ray(toward.normalize() * 3.0, -toward.normalize())
            .unwrap();
        sample(&pixels, size, size, uv_near(&low, on.position), true)[0]
    };
    let mouth = Vec3::new(0.15, -0.44, 0.88);
    let forehead = Vec3::new(0.0, 0.7, 0.7);
    let brow = Vec3::new(0.25, 0.35, 0.9);
    assert!(
        at("Curvature", mouth) + 10.0 < at("Curvature", forehead),
        "the mouth's crease is darker: {} against {}",
        at("Curvature", mouth),
        at("Curvature", forehead)
    );
    assert!(at("Curvature", brow) > 128.0, "the brow bulges");
    assert!(
        at("Occlusion", mouth) < at("Occlusion", forehead),
        "the mouth is more occluded"
    );
    // The sculpt's clay color, with nothing painted.
    let color = at("BaseColor", forehead);
    assert!((color - 203.0).abs() < 2.0, "base color red {color}");
}

/// The uv on `mesh` nearest `point`.
fn uv_near(mesh: &EditMesh, point: Vec3) -> Vec2 {
    let mut best = (f32::INFINITY, Vec2::ZERO);
    for face in 0..mesh.face_count() as u32 {
        for triangle in mesh.triangles(face) {
            let corners = triangle.map(|e| mesh.half_edge(e));
            let (on, weights) =
                closest_on_triangle(point, corners.map(|c| mesh.position(c.vertex)));
            let distance = on.distance(point);
            if distance < best.0 {
                let uv = (0..3).map(|k| corners[k].uv * weights[k]).sum();
                best = (distance, uv);
            }
        }
    }
    best.1
}

#[test]
fn a_bake_without_overlapping_uvs_shows_no_seams() {
    let mut doc = face_doc(4000);
    let low = doc.objects[1].mesh.clone().unwrap();
    let settings = BakeSettings {
        size: 1024,
        occlusion: false,
        curvature: false,
        base_color: false,
        ..BakeSettings::default()
    };
    let report = doc.bake("assets/models/Face.glb", 1, 0, &settings).unwrap();
    assert!(report.overlaps.is_empty());
    let normal_map = &report.files[0].1;
    let pixels = image::load_from_memory(normal_map).unwrap().into_rgba8();
    let tangents = corner_tangents(&low, 0);
    // The normal drawn at `s` along half-edge `edge` of its face.
    let drawn = |edge: u32, s: f32| {
        let (from, to) = (edge, low.half_edge(edge).next);
        let mix = |a: Vec3, b: Vec3| a.lerp(b, s);
        let (a, b) = (low.half_edge(from), low.half_edge(to));
        let tangent = mix(
            Vec3::from_slice(&tangents[from as usize][..3]),
            Vec3::from_slice(&tangents[to as usize][..3]),
        );
        let side = tangents[from as usize][3] * (1.0 - s) + tangents[to as usize][3] * s;
        let uv = a.uv.lerp(b.uv, s);
        let texel = sample(pixels.as_raw(), 1024, 1024, uv, true);
        decode(mix(a.normal, b.normal), tangent, side, texel, false)
    };
    // Along each seam (an edge whose sides have different uvs), the two
    // sides draw the same normal.
    let mut seams = 0;
    let mut differences = Vec::new();
    for edge in low.edges() {
        let Some(twin) = low.half_edge(edge).twin else {
            continue;
        };
        let (a, b) = (low.half_edge(edge), low.half_edge(twin));
        let (a_next, b_next) = (low.half_edge(a.next), low.half_edge(b.next));
        if a.uv.distance(b_next.uv) < 1e-5 && a_next.uv.distance(b.uv) < 1e-5 {
            continue;
        }
        seams += 1;
        for s in [0.2, 0.5, 0.8] {
            // The twin runs the other way.
            differences.push(degrees(drawn(edge, s), drawn(twin, 1.0 - s)));
        }
    }
    differences.sort_by(f32::total_cmp);
    let mean = differences.iter().sum::<f32>() / differences.len() as f32;
    let at = |share: usize| differences[differences.len() * share / 100];
    println!(
        "{seams} seam edges: {mean:.2}° apart on average, {:.2}° at the median, 95% within {:.2}°, worst {:.2}°",
        at(50),
        at(95),
        differences.last().unwrap()
    );
    assert!(seams > 50, "Smart UV Project cut seams: {seams}");
    assert!(mean < 1.0, "seams {mean}° apart on average");
    assert!(at(50) < 0.5, "half the seam within {}°", at(50));
    assert!(at(95) < 1.0, "95% of the seam within {}°", at(95));
    // The worst are faces Smart UV Project squashed to a texel or less
    // across, as far off inside as at their edges.
    let worst = *differences.last().unwrap();
    assert!(worst < 5.0, "the worst seam point {worst}° apart");
}

#[test]
fn baking_onto_overlapping_uvs_warns_which_islands_overlap() {
    // A New cube's six faces share one square of uvs.
    let cube = PrimitiveSettings::of(PrimitiveShape::Cube).build();
    let mut doc = ModelDoc {
        objects: vec![
            object("Detail", cube.clone(), "Clay"),
            object("Box", cube.clone(), "Paint"),
        ],
        armature: None,
    };
    let settings = BakeSettings {
        size: 64,
        occlusion: false,
        curvature: false,
        ..BakeSettings::default()
    };
    let report = doc.bake("Box.glb", 1, 0, &settings).unwrap();
    let islands = uv::uv_islands(&cube, &all_faces(&cube), 0);
    assert_eq!(islands.len(), 6);
    assert_eq!(report.overlaps.len(), 15, "every pair of the six squares");
    let warnings = report.warnings();
    assert_eq!(warnings.len(), 15);
    assert!(
        warnings[0].starts_with("UV islands 1 and 2 overlap (1 and 1 faces)"),
        "{}",
        warnings[0]
    );
    for overlap in &report.overlaps {
        let [a, b] = overlap.islands;
        assert_eq!(overlap.faces, [islands[a].clone(), islands[b].clone()]);
    }
    // Unwrapped without overlaps, it doesn't warn.
    let mesh = doc.objects[1].mesh.as_mut().unwrap();
    let faces = all_faces(mesh);
    uv::smart_project(mesh, &faces, 66.0, 0.02, 0).unwrap();
    let report = doc.bake("Box.glb", 1, 0, &settings).unwrap();
    assert!(report.overlaps.is_empty());
    assert!(report.warnings().is_empty());
}

#[test]
fn a_directx_normal_map_is_named_so_and_read_back_with_its_green_flipped() {
    let mut doc = face_doc(1500);
    let gl = BakeSettings {
        size: 128,
        occlusion: false,
        curvature: false,
        base_color: false,
        ..BakeSettings::default()
    };
    let dx = BakeSettings {
        flip_green: true,
        ..gl.clone()
    };
    let gl_report = doc.bake("assets/models/Face.glb", 1, 0, &gl).unwrap();
    let dx_report = doc.bake("assets/models/Face.glb", 1, 0, &dx).unwrap();
    assert_eq!(
        dx_report.files[0].0,
        "assets/models/textures/Face_Face_Low_Skin_NormalDX.png"
    );
    let read = |bytes: &[u8]| image::load_from_memory(bytes).unwrap().into_rgba8();
    let (gl_map, dx_map) = (read(&gl_report.files[0].1), read(&dx_report.files[0].1));
    for (a, b) in gl_map.pixels().zip(dx_map.pixels()) {
        assert_eq!(a[0], b[0]);
        assert!(
            (i32::from(a[1]) + i32::from(b[1]) - 255).abs() <= 1,
            "green flipped"
        );
        assert_eq!(a[2], b[2]);
    }
    let material = &doc.objects[1].materials[0];
    assert!(material.normal_flip_green);
    // A game loading the `.glb` reads it as DirectX's by its name.
    let model = load_saved(&doc, &dx_report.files, "Face_Low", "dx");
    assert!(model.primitives[0].material.normal_flip_green);
}

#[test]
fn baking_needs_uvs_and_two_objects() {
    let mut doc = face_doc(1000);
    doc.objects[1].uvs = false;
    let settings = BakeSettings::default();
    let error = doc.bake("Face.glb", 1, 0, &settings).unwrap_err();
    assert!(error.contains("unwrap"), "{error}");
    assert!(doc.bake("Face.glb", 0, 0, &settings).is_err());
}

#[test]
fn the_reference_finds_what_testing_every_triangle_finds() {
    let reference = Reference::new(&sculpted_face());
    let mut random = Lcg(7);
    for _ in 0..200 {
        let p = Vec3::new(random.next(), random.next(), random.next()) * 3.0 - 1.5;
        let on = reference.nearest(p).unwrap();
        let brute = (0..reference.triangle_count() as u32)
            .map(|t| closest_on_triangle(p, reference.corners(t)).0.distance(p))
            .fold(f32::INFINITY, f32::min);
        assert!((on.position.distance(p) - brute).abs() < 1e-5);
        let direction = (-p).normalize();
        let (_, t) = reference.ray(p * 2.0, direction).unwrap();
        let brute = reference.ray_by_brute_force(p * 2.0, direction).unwrap();
        assert!((t - brute).abs() < 1e-4, "{t} against {brute}");
    }
}

#[test]
fn poly_build_puts_quads_on_the_surface_and_joins_them() {
    let reference = Reference::new(&sculpted_face());
    let snap = |p: Vec3| reference.snap(p);
    let on = |p: Vec3| reference.nearest(p).unwrap().position.distance(p) < 1e-5;
    // The first quad: four points above the forehead, which snap down.
    let points = [
        Vec3::new(-0.1, 0.6, 0.9),
        Vec3::new(0.1, 0.6, 0.9),
        Vec3::new(0.1, 0.8, 0.7),
        Vec3::new(-0.1, 0.8, 0.7),
    ];
    let (mesh, face) = poly_build(
        &EditMesh::default(),
        &PolyStep::Quad {
            points,
            facing: Vec3::new(0.0, 0.7, 0.7),
        },
        0.01,
        &snap,
    )
    .unwrap();
    assert_eq!((mesh.vertex_count(), mesh.face_count()), (4, 1));
    assert!(mesh.positions().iter().all(|&p| on(p)));
    assert!(
        mesh.face_normal(face).dot(Vec3::new(0.0, 0.7, 0.7)) > 0.5,
        "faces out"
    );

    // Out from its right edge, toward the temple.
    let right = |mesh: &EditMesh| {
        (0..mesh.half_edges().len() as u32)
            .find(|&e| {
                let (a, b) = mesh.ends(e);
                mesh.half_edge(e).twin.is_none()
                    && mesh.position(a).x > 0.05
                    && mesh.position(b).x > 0.05
            })
            .unwrap()
    };
    let (mesh, face) = poly_build(
        &mesh,
        &PolyStep::FromEdge {
            edge: right(&mesh),
            to: Vec3::new(0.3, 0.7, 0.8),
        },
        0.01,
        &snap,
    )
    .unwrap();
    assert_eq!((mesh.vertex_count(), mesh.face_count()), (6, 2));
    assert!(mesh.positions().iter().all(|&p| on(p)));
    check_manifold(&mesh, false).expect("the two quads agree");
    assert!(mesh.face_normal(face).z > 0.3);

    // A quad whose corners land on vertices already there joins them.
    let corners = mesh.face_vertices(face);
    let back: [Vec3; 4] = [0, 1, 2, 3].map(|k| mesh.position(corners[k]) + Vec3::splat(0.004));
    let (joined, _) = poly_build(
        &mesh,
        &PolyStep::Quad {
            points: back,
            facing: Vec3::Z,
        },
        0.02,
        &snap,
    )
    .unwrap();
    assert_eq!(joined.vertex_count(), 6, "no new vertices");
}

#[test]
fn decimate_to_a_count_comes_in_under_it() {
    let high = subdivide(&sculpted_face()).unwrap();
    for target in [500, 2000, 4999] {
        let low = decimate_to(&high, target);
        let count = triangle_count(&low);
        assert!(
            count <= target && count > target * 8 / 10,
            "{count} for {target}"
        );
    }
    assert_eq!(decimate_to(&high, usize::MAX), high);
}

#[test]
fn relaxing_a_decimated_mesh_on_the_reference_opens_out_its_folds() {
    let high = subdivide(&sculpted_face()).unwrap();
    let reference = Reference::new(&high);
    // Faces turned more than about 45° from the sculpt under them.
    let folds = |mesh: &EditMesh| {
        (0..mesh.face_count() as u32)
            .filter(|&face| {
                let at = reference.nearest(mesh.face_center(face)).unwrap();
                mesh.face_normal(face).dot(reference.normal(&at)) < 0.7
            })
            .count()
    };
    let mut low = decimate_to(&high, 4000);
    let before = folds(&low);
    relax(&mut low, &reference, 3);
    let after = folds(&low);
    println!("{before} folded faces decimated, {after} relaxed");
    assert!(after * 3 <= before, "{before} to {after}");
    assert_eq!(low.face_count(), decimate_to(&high, 4000).face_count());
    for &p in low.positions() {
        assert!(reference.nearest(p).unwrap().position.distance(p) < 1e-5);
    }
}
