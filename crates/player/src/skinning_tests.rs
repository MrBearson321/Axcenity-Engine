//! Animated characters skinned on the GPU draw as they did skinned on the
//! CPU (graphics Phase 14).

use std::sync::Arc;

use axcenity_assets::PosedPrimitive;
use axcenity_render::{DrawList, MeshSource, Shape, TextureRenderer, wgpu};

use super::tests::start_example_in;
use super::*;

/// A primitive skinned on the CPU (`Posed::primitive`), as characters were
/// drawn before they were skinned on the GPU.
struct CpuPosed {
    key: u64,
    corners: PosedPrimitive,
    uvs: Vec<[f32; 2]>,
    indices: Vec<u32>,
}

impl MeshSource for CpuPosed {
    fn key(&self) -> u64 {
        self.key
    }
    fn positions(&self) -> &[[f32; 3]] {
        &self.corners.positions
    }
    fn normals(&self) -> &[[f32; 3]] {
        &self.corners.normals
    }
    fn uvs(&self) -> &[[f32; 2]] {
        &self.uvs
    }
    fn tangents(&self) -> &[[f32; 4]] {
        &self.corners.tangents
    }
    fn indices(&self) -> &[u32] {
        &self.indices
    }
}

fn gpu() -> Option<(wgpu::Device, wgpu::Queue)> {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = pollster::block_on(instance.request_adapter(&Default::default())).ok()?;
    pollster::block_on(adapter.request_device(&Default::default())).ok()
}

/// Draws `list` (twice, so its meshes are uploaded) and reads it back.
fn draw(renderer: &mut TextureRenderer, list: &DrawList) -> Vec<u8> {
    renderer.render(list).unwrap();
    renderer.render(list).unwrap();
    renderer.read_pixels()
}

/// How many pixels of `a` and `b` differ by more than 8 levels in a channel.
fn pixels_apart(a: &[u8], b: &[u8]) -> usize {
    a.chunks(4)
        .zip(b.chunks(4))
        .filter(|(a, b)| (0..3).any(|channel| a[channel].abs_diff(b[channel]) > 8))
        .count()
}

#[test]
fn a_posed_character_looks_the_same_skinned_on_the_gpu_as_on_the_cpu() {
    let Some((device, queue)) = gpu() else {
        return;
    };
    let (mut world, _scripts, folder) = start_example_in("hello-animation");
    let noodle = world.find_first_child(world.workspace(), "Noodle").unwrap();
    let assets = Assets::new(&folder);
    let mut animators = Animators::new();
    // Into its idle sway, away from where the model rests.
    for _ in 0..45 {
        animators.update(&mut world, &assets, 1.0 / 60.0);
    }
    assert!(animators.take_errors().is_empty());
    let posed = animators.poses()[&noodle].clone();
    let model = assets.model_blocking("assets/models/noodle.fbx").unwrap();

    let camera = axcenity_render::Camera {
        position: Vec3::new(2.0, 5.0, 9.0),
        look_at: Vec3::new(0.0, 3.8, 0.0),
        fov_y_degrees: 50.0,
    };
    let mut cache = axcenity_scene::TerrainCache::new();
    let on_gpu = axcenity_scene::build_draw_list_posed(
        &world,
        &assets,
        camera,
        animators.poses(),
        &mut cache,
    );
    let skinned = on_gpu
        .items
        .iter()
        .filter(|item| item.skin.is_some())
        .count();
    assert_eq!(skinned, model.primitives.len());
    // The same items, skinned on the CPU instead, and not posed at all.
    let mut on_cpu = on_gpu.clone();
    let mut at_rest = on_gpu.clone();
    for item in &mut on_cpu.items {
        let Shape::Mesh(mesh) = &item.shape else {
            continue;
        };
        let Some(index) = model.primitives.iter().position(|p| p.key() == mesh.key()) else {
            continue;
        };
        let primitive = &model.primitives[index];
        item.shape = Shape::Mesh(Arc::new(CpuPosed {
            key: mesh.key() | 1 << 63,
            corners: posed.primitive(&model, index, 0).clone(),
            uvs: primitive.uvs.clone(),
            indices: primitive.indices.clone(),
        }));
        item.skin = None;
    }
    for item in &mut at_rest.items {
        item.skin = None;
    }

    let (width, height) = (160, 160);
    let mut renderer = TextureRenderer::new(&device, &queue, width, height);
    let gpu_pixels = draw(&mut renderer, &on_gpu);
    let cpu_pixels = draw(&mut renderer, &on_cpu);
    let rest_pixels = draw(&mut renderer, &at_rest);
    let total = (width * height) as usize;
    // The pose moves the character visibly...
    let moved = pixels_apart(&gpu_pixels, &rest_pixels);
    assert!(moved > total / 200, "the pose moves only {moved} pixels");
    // ...and the GPU puts it where the CPU did, shaded the same.
    let apart = pixels_apart(&gpu_pixels, &cpu_pixels);
    assert!(
        apart <= total / 2000,
        "{apart} pixels differ between GPU and CPU skinning ({moved} moved)"
    );
}
