//! Phase 59: texture painting: layers, brushes through the texel map, a
//! stroke across a seam, the maps saved as PNGs named by role, and layers
//! through the sidecar folder.

use std::collections::BTreeMap;
use std::sync::Arc;

use axcenity_assets::{ModelFormat, ModelMaterial, texture_roles};
use glam::{Vec2, Vec3};

use crate::paint::{
    BlendMode, Brush, BrushFalloff, Dab, MapKind, PaintSet, PaintTool, StampView, Target, TexelMap,
    normals_from_height, sample, stamp, uv_touches,
};
use crate::{EditMesh, ModelDoc, PrimitiveSettings, PrimitiveShape, uv};

const RUST: [u8; 3] = [150, 70, 30];

/// A 2 × 2 plane facing up, in a 4 × 4 grid, cut down the middle (x = 0)
/// by a seam and unwrapped into two islands.
fn seamed_plane() -> EditMesh {
    let mut mesh = PrimitiveSettings {
        segments: 4,
        ..PrimitiveSettings::of(PrimitiveShape::Plane)
    }
    .build();
    let middle: Vec<[u32; 2]> = mesh
        .edges()
        .into_iter()
        .map(|e| mesh.ends(e))
        .filter(|&(a, b)| mesh.position(a).x.abs() < 1e-5 && mesh.position(b).x.abs() < 1e-5)
        .map(|(a, b)| [a.min(b), a.max(b)])
        .collect();
    assert_eq!(middle.len(), 4, "a line of edges down the middle");
    mesh.set_seams(middle, true);
    let faces: Vec<u32> = (0..mesh.face_count() as u32).collect();
    uv::unwrap(&mut mesh, &faces, 0.05, 0).unwrap();
    assert_eq!(uv::uv_islands(&mesh, &faces, 0).len(), 2);
    mesh
}

fn grey_material() -> ModelMaterial {
    ModelMaterial {
        name: "Wood".to_owned(),
        base_color: [0.5, 0.5, 0.5, 1.0],
        roughness: 0.4,
        ..ModelMaterial::default()
    }
}

fn draw(color: [u8; 3]) -> Brush {
    Brush {
        color,
        strength: 1.0,
        ..Brush::default()
    }
}

/// Paints `dab` (and nothing else) onto `kind` of `set` with `brush`.
fn paint(set: &mut PaintSet, texels: &TexelMap, kind: MapKind, dab: Dab, brush: &Brush) {
    let touches = texels.touches(&dab, brush.falloff, &|_| Some(Vec2::ZERO));
    assert!(!touches.is_empty(), "the dab reaches the surface");
    let material = grey_material();
    let map = set.ensure_map(kind, &material);
    let view = StampView {
        center: Vec2::ZERO,
        radius: 1.0,
        clone: None,
    };
    assert!(stamp(map, Target::Layer, &touches, brush, &view));
}

fn dab_at(center: Vec3, radius: f32) -> Dab {
    Dab {
        center,
        normal: Vec3::Y,
        radius,
        strength: 1.0,
        motion: Vec3::ZERO,
        mirror: [false; 3],
    }
}

/// `face`'s color at `point` on it, as the GPU would read the map.
fn color_at(mesh: &EditMesh, texels: &TexelMap, flat: &[u8], point: Vec3) -> [f32; 4] {
    let face = (0..mesh.face_count() as u32)
        .find(|&f| {
            let vertices = mesh.face_vertices(f);
            let (min, max) = vertices
                .iter()
                .map(|&v| mesh.position(v))
                .fold((Vec3::INFINITY, Vec3::NEG_INFINITY), |(min, max), p| {
                    (min.min(p), max.max(p))
                });
            point.cmpge(min - 1e-4).all() && point.cmple(max + 1e-4).all()
        })
        .unwrap();
    let uv = texels.uv_at(face, point).unwrap();
    sample(flat, texels.size(), texels.size(), uv, true)
}

#[test]
fn painting_across_a_uv_seam_leaves_no_line() {
    let mesh = seamed_plane();
    let size = 256;
    let texels = TexelMap::build(&mesh, 0, size);
    let mut set = PaintSet::new(0, size, &grey_material());
    // Off the seam's middle, so its two sides get different amounts.
    paint(
        &mut set,
        &texels,
        MapKind::BaseColor,
        dab_at(Vec3::new(0.2, 0.0, 0.1), 0.7),
        &draw(RUST),
    );
    let flat = set.map(MapKind::BaseColor).unwrap().flat();
    let mut painted = 0;
    for step in 0..=20 {
        let z = -0.9 + 1.8 * step as f32 / 20.0;
        // Each side's face, a hair from the seam, read through its own
        // island's uvs.
        let left = color_at(&mesh, &texels, flat, Vec3::new(-1e-3, 0.0, z));
        let right = color_at(&mesh, &texels, flat, Vec3::new(1e-3, 0.0, z));
        for c in 0..3 {
            assert!(
                (left[c] - right[c]).abs() <= 4.0,
                "at z = {z}: {left:?} one side, {right:?} the other"
            );
        }
        if (left[0] - 128.0).abs() > 10.0 {
            painted += 1;
        }
    }
    assert!(painted > 5, "the stroke crosses the seam");
}

#[test]
fn a_dab_paints_only_surfaces_facing_its_way_and_its_mirror_image() {
    let mesh = PrimitiveSettings::default().build();
    let texels = TexelMap::build(&mesh, 0, 128);
    // On the top, near the +X side: the side faces away from it.
    let dab = Dab {
        normal: Vec3::Y,
        ..dab_at(Vec3::new(0.9, 1.0, 0.0), 0.5)
    };
    let touches = texels.touches(&dab, BrushFalloff::Smooth, &|_| Some(Vec2::ZERO));
    let faces: std::collections::BTreeSet<u32> = touches
        .iter()
        .filter(|t| t.weight > 0.0)
        .filter_map(|t| texels.face_of(t.texel))
        .collect();
    assert_eq!(faces.len(), 1, "only the top");
    let mirrored = dab.with_symmetry([true, false, false]);
    assert_eq!(mirrored.len(), 2);
    assert_eq!(mirrored[1].center, Vec3::new(-0.9, 1.0, 0.0));
    assert_eq!(dab.with_symmetry([true, true, true]).len(), 8);
}

#[test]
fn layers_blend_and_a_hidden_or_masked_layer_leaves_the_flattened_map() {
    let mut set = PaintSet::new(0, 4, &grey_material());
    let map = set.map_mut(MapKind::BaseColor).unwrap();
    // The material's grey, in sRGB.
    assert_eq!(map.fill, [188, 188, 188, 255]);
    assert_eq!(map.layers.len(), 1, "no texture: one layer to paint on");
    let all: Vec<_> = (0..16)
        .map(|texel| crate::paint::Touch {
            texel,
            weight: 1.0,
            at: Vec2::ZERO,
            smear: Vec2::ZERO,
        })
        .collect();
    let view = StampView {
        center: Vec2::ZERO,
        radius: 1.0,
        clone: None,
    };
    stamp(map, Target::Layer, &all, &draw([255, 0, 0]), &view);
    assert_eq!(&map.flat()[..4], &[255, 0, 0, 255]);
    map.add_layer();
    assert_eq!(map.active, 1);
    stamp(map, Target::Layer, &all, &draw([0, 0, 255]), &view);
    assert_eq!(&map.flat()[..4], &[0, 0, 255, 255]);
    map.layers[1].blend = BlendMode::Screen;
    map.refresh_all();
    assert_eq!(&map.flat()[..4], &[255, 0, 255, 255]);
    map.layers[1].opacity = 0.5;
    map.layers[1].blend = BlendMode::Normal;
    map.refresh_all();
    assert_eq!(&map.flat()[..4], &[128, 0, 128, 255]);
    map.layers[1].visible = false;
    map.refresh_all();
    assert_eq!(&map.flat()[..4], &[255, 0, 0, 255]);

    // A mask: Erase hides the layer where it's painted.
    map.layers[1].visible = true;
    map.layers[1].opacity = 1.0;
    let erase = Brush {
        tool: PaintTool::Erase,
        ..draw(RUST)
    };
    stamp(map, Target::Mask, &all[..1], &erase, &view);
    map.refresh_all();
    assert_eq!(&map.flat()[..4], &[255, 0, 0, 255], "masked off");
    assert_eq!(&map.flat()[4..8], &[0, 0, 255, 255], "not masked");
    assert!(
        !map.layers[1].is_empty(),
        "the layer itself keeps its paint"
    );
}

#[test]
fn erase_soften_smear_clone_and_fill_change_the_layer() {
    let size = 16;
    let mut set = PaintSet::new(0, size, &grey_material());
    let map = set.map_mut(MapKind::BaseColor).unwrap();
    let to_view = |uv: Vec2| uv;
    let view = StampView {
        center: Vec2::ZERO,
        radius: 1.0,
        clone: None,
    };
    // A red dot in the middle of the left half.
    let dot = uv_touches(
        size,
        Vec2::new(0.25, 0.5),
        0.1,
        BrushFalloff::Constant,
        1.0,
        Vec2::ZERO,
        &to_view,
    );
    stamp(map, Target::Layer, &dot, &draw([255, 0, 0]), &view);
    let texel = |uv: Vec2| ((uv.y * 16.0) as usize * 16 + (uv.x * 16.0) as usize) * 4;
    let middle = texel(Vec2::new(0.25, 0.5));
    assert_eq!(map.flat()[middle], 255);

    // Clone copies it half the texture to the right.
    let source = map.flat_shared();
    let clone = Brush {
        tool: PaintTool::Clone,
        ..draw(RUST)
    };
    let right = uv_touches(
        size,
        Vec2::new(0.75, 0.5),
        0.1,
        BrushFalloff::Constant,
        1.0,
        Vec2::ZERO,
        &to_view,
    );
    let cloned = StampView {
        clone: Some((&source, Vec2::new(-0.5, 0.0))),
        ..view
    };
    stamp(map, Target::Layer, &right, &clone, &cloned);
    assert_eq!(map.flat()[texel(Vec2::new(0.75, 0.5))], 255);
    assert!(
        !stamp(map, Target::Layer, &right, &clone, &view),
        "no source"
    );

    // Soften spreads the dot's edge; Smear drags it.
    let edge = texel(Vec2::new(0.25 + 0.15, 0.5));
    let before = map.flat()[edge];
    let wide = uv_touches(
        size,
        Vec2::new(0.25, 0.5),
        0.3,
        BrushFalloff::Constant,
        1.0,
        Vec2::ZERO,
        &to_view,
    );
    let soften = Brush {
        tool: PaintTool::Soften,
        ..draw(RUST)
    };
    stamp(map, Target::Layer, &wide, &soften, &view);
    assert!(map.flat()[edge] > before, "softened outward");
    let smear = Brush {
        tool: PaintTool::Smear,
        ..draw(RUST)
    };
    let below = texel(Vec2::new(0.25, 0.5 + 0.25));
    let dragged = uv_touches(
        size,
        Vec2::new(0.25, 0.75),
        0.08,
        BrushFalloff::Constant,
        1.0,
        Vec2::new(0.0, 0.25),
        &to_view,
    );
    stamp(map, Target::Layer, &dragged, &smear, &view);
    assert!(map.flat()[below] > 200, "smeared down");

    // Erase takes the layer off; Fill paints every texel given.
    let erase = Brush {
        tool: PaintTool::Erase,
        ..draw(RUST)
    };
    let everything: Vec<_> = uv_touches(
        size,
        Vec2::splat(0.5),
        1.0,
        BrushFalloff::Constant,
        1.0,
        Vec2::ZERO,
        &to_view,
    );
    stamp(map, Target::Layer, &everything, &erase, &view);
    assert!(map.layers[0].is_empty());
    let fill = Brush {
        tool: PaintTool::Fill,
        ..draw([0, 255, 0])
    };
    stamp(map, Target::Layer, &everything, &fill, &view);
    assert!(map.flat().chunks(4).all(|p| p == [0, 255, 0, 255]));
}

#[test]
fn a_height_bump_makes_normals_tipped_away_from_it() {
    let size = 32;
    let mut height = vec![128u8; 32 * 32 * 4];
    for (k, pixel) in height.chunks_mut(4).enumerate() {
        let (x, y) = ((k % 32) as f32 - 16.0, (k / 32) as f32 - 16.0);
        let h = (128.0 + 100.0 * (-(x * x + y * y) / 20.0).exp()) as u8;
        pixel[0] = h;
    }
    let normals = normals_from_height(&height, size, 4.0);
    let at = |x: usize, y: usize| {
        let k = (y * 32 + x) * 4;
        [normals[k], normals[k + 1], normals[k + 2]].map(|c| c as f32 / 127.5 - 1.0)
    };
    // Right of the peak it tips right (+x); below it (down the texture),
    // it tips down: green, which points up the texture, goes negative.
    assert!(at(19, 16)[0] > 0.2);
    assert!(at(13, 16)[0] < -0.2);
    assert!(at(16, 19)[1] < -0.2);
    assert!(at(16, 13)[1] > 0.2);
    assert!(at(0, 0)[2] > 0.99, "level far away");
}

/// The cube, its map painted rust and its roughness painted rough, as a
/// model.
fn painted_cube() -> ModelDoc {
    let mut doc = ModelDoc::primitive(&PrimitiveSettings::default());
    doc.objects[0].materials[0].name = "Wood".to_owned();
    let mesh = doc.objects[0].mesh.clone().unwrap();
    let texels = TexelMap::build(&mesh, 0, 64);
    let material = doc.objects[0].materials[0].clone();
    let mut set = PaintSet::new(0, 64, &material);
    // Every face of the cube has the whole texture, so the dab paints
    // the middle of it.
    let top = dab_at(Vec3::new(0.0, 1.0, 0.0), 0.3);
    paint(&mut set, &texels, MapKind::BaseColor, top, &draw(RUST));
    let rough = Brush {
        value: 1.0,
        ..draw(RUST)
    };
    paint(&mut set, &texels, MapKind::Roughness, top, &rough);
    set.apply_to(&mut doc.objects[0].materials[0]);
    doc.objects[0].paint.push(set);
    doc
}

#[test]
fn saving_flattens_each_map_to_a_png_named_by_its_role_that_the_glb_uses() {
    let mut doc = painted_cube();
    let files = doc.bake_paint("assets/models/Cube.glb");
    let names: Vec<&str> = files.iter().map(|(path, _)| path.as_str()).collect();
    assert_eq!(
        names,
        [
            "assets/models/textures/Cube_Cube_Wood_BaseColor.png",
            "assets/models/textures/Cube_Cube_Wood_MetallicRoughness.png",
        ]
    );
    // Named so `assets` reads them as what they are.
    let slots = |path: &str| {
        texture_roles(path)
            .into_iter()
            .map(|r| r.slot)
            .collect::<Vec<_>>()
    };
    use axcenity_assets::MapSlot;
    assert_eq!(slots(names[0]), [MapSlot::BaseColor]);
    assert_eq!(slots(names[1]), [MapSlot::Roughness, MapSlot::Metalness]);

    // Loaded from the `.glb` as a game does, the material uses them.
    let folder = std::env::temp_dir().join(format!("axcenity-paint-{}", std::process::id()));
    let models = folder.join("assets/models");
    std::fs::create_dir_all(models.join("textures")).unwrap();
    for (path, bytes) in &files {
        std::fs::write(folder.join(path), bytes).unwrap();
    }
    let glb = doc.write_glb();
    let model = axcenity_assets::load_model(&glb, &models, ModelFormat::Gltf).unwrap();
    let material = &model.primitives[0].material;
    let set = &doc.objects[0].paint[0];
    let color = material
        .base_color_texture
        .as_ref()
        .expect("base color map");
    assert_eq!(color.rgba(), set.map(MapKind::BaseColor).unwrap().flat());
    assert_eq!(material.base_color, [1.0; 4]);
    let rough = material.roughness_texture.as_ref().expect("roughness map");
    assert_eq!(rough.channel, axcenity_assets::Channel::Green);
    let painted: Vec<u8> = rough.image.rgba().chunks(4).map(|p| p[1]).collect();
    let wanted: Vec<u8> = set
        .map(MapKind::Roughness)
        .unwrap()
        .flat()
        .chunks(4)
        .map(|p| p[0])
        .collect();
    assert_eq!(painted, wanted);
    assert!(
        wanted.iter().any(|&v| v > 240) && wanted.contains(&102),
        "rough paint on 0.4"
    );
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn layers_go_through_the_sidecar_folder_hidden_and_all() {
    let mut doc = painted_cube();
    {
        let map = doc.objects[0].paint[0].map_mut(MapKind::BaseColor).unwrap();
        map.add_layer();
        map.layers[1].pixels = Arc::new(vec![200; 64 * 64 * 4]);
        map.layers[1].visible = false;
        map.layers[1].blend = BlendMode::Multiply;
        map.layers[1].mask = Some(Arc::new(vec![7; 64 * 64]));
        map.refresh_all();
    }
    let glb = doc.write_glb();
    let sidecar = doc.write_sidecar(&glb).expect("painted objects need it");
    let files: BTreeMap<String, Vec<u8>> = doc.paint_files().into_iter().collect();
    assert!(files.contains_key("0-0-BaseColor-1.png"));
    assert!(files.contains_key("0-0-BaseColor-1-mask.png"));

    let mut reopened = ModelDoc::open(&glb, std::path::Path::new("."), ModelFormat::Gltf).unwrap();
    reopened
        .read_sidecar_with(&sidecar, &glb, &mut |name| files.get(name).cloned())
        .unwrap();
    let before = &doc.objects[0].paint[0];
    let after = &reopened.objects[0].paint[0];
    for kind in [MapKind::BaseColor, MapKind::Roughness, MapKind::Metalness] {
        let (a, b) = (before.map(kind).unwrap(), after.map(kind).unwrap());
        assert_eq!(a.layers, b.layers, "{kind:?}");
        assert_eq!(a.flat(), b.flat());
        assert_eq!(a.fill, b.fill);
    }
    let layer = &after.map(MapKind::BaseColor).unwrap().layers[1];
    assert!(!layer.visible && layer.blend == BlendMode::Multiply);
    // And it's on the material again.
    let material = &reopened.objects[0].materials[0];
    assert_eq!(
        material.base_color_texture.as_ref().unwrap().rgba(),
        after.map(MapKind::BaseColor).unwrap().flat()
    );
}
