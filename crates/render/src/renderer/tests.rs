use super::*;
use crate::draw_list::{
    AmbientLight, Camera, Channel, Cone, DrawItem, Light, Material, NormalMap, PostProcessing,
    Shadows, Sky, Sun,
};
use glam::Mat4;

#[test]
fn zero_sized_window_has_nothing_to_draw() {
    assert_eq!(drawable_size(0, 0), None);
    assert_eq!(drawable_size(0, 720), None);
    assert_eq!(drawable_size(1280, 0), None);
}

#[test]
fn normal_window_keeps_its_size() {
    assert_eq!(drawable_size(1280, 720), Some((1280, 720)));
    assert_eq!(drawable_size(1, 1), Some((1, 1)));
}

#[test]
fn srgb_to_linear_keeps_black_and_white_and_darkens_midtones() {
    assert_eq!(linear_color([0.0, 1.0, 0.5]).x, 0.0);
    assert!((linear_color([0.0, 1.0, 0.5]).y - 1.0).abs() < 1e-6);
    assert!((linear_color([0.0, 1.0, 0.5]).z - 0.214).abs() < 0.001);
}

#[test]
fn ui_rectangles_keep_rounded_border_data_and_allow_border_only_shapes() {
    let item = UiDrawItem::Rectangle(UiRectangle {
        position: [10.0, 20.0],
        size: [100.0, 50.0],
        color: [0.0, 0.0, 0.0, 0.0],
        border_color: [1.0, 0.0, 0.0, 1.0],
        border_width: 3.0,
        border_radius: 80.0,
        color_end: [0.0, 0.0, 0.0, 0.0],
        ..Default::default()
    });

    let vertices = ui_vertices(&[item], 0..1, &[], (200, 100));
    assert_eq!(vertices.len(), 6);
    assert_eq!(vertices[0].size, [100.0, 50.0]);
    assert_eq!(vertices[0].border, [25.0, UI_BOX]);
    assert_eq!(vertices[0].border_widths, [3.0; 4]);
    assert_eq!(vertices[0].border_color[3], 1.0);
}

fn ui_rect(position: [f32; 2], size: [f32; 2]) -> UiDrawItem {
    UiDrawItem::Rectangle(UiRectangle {
        position,
        size,
        color: [0.0, 0.0, 0.0, 1.0],
        border_color: [0.0; 4],
        border_width: 0.0,
        border_radius: 0.0,
        color_end: [0.0, 0.0, 0.0, 1.0],
        ..Default::default()
    })
}

fn ui_label(position: [f32; 2], size: [f32; 2]) -> UiDrawItem {
    UiDrawItem::Text(UiText {
        position,
        size,
        text: "Score".to_owned(),
        ..UiText::default()
    })
}

#[test]
fn ui_that_does_not_overlap_is_one_layer() {
    let items = [
        ui_rect([0.0, 0.0], [100.0, 40.0]),
        ui_label([0.0, 0.0], [100.0, 40.0]),
        ui_rect([300.0, 300.0], [100.0, 40.0]),
        ui_label([300.0, 300.0], [100.0, 40.0]),
    ];
    assert_eq!(ui_layers(&items), vec![0..4]);
}

#[test]
fn a_rectangle_over_earlier_text_starts_a_layer() {
    // A HUD label, then a full-screen loading screen with its own caption.
    let items = [
        ui_rect([10.0, 10.0], [200.0, 40.0]),
        ui_label([10.0, 10.0], [200.0, 40.0]),
        ui_rect([0.0, 0.0], [1280.0, 720.0]),
        ui_label([500.0, 400.0], [280.0, 30.0]),
    ];
    assert_eq!(ui_layers(&items), vec![0..2, 2..4]);
    assert!(ui_layers(&[]).is_empty());
}

#[test]
fn text_shadows_count_toward_overlap() {
    let mut label = UiText {
        position: [0.0, 0.0],
        size: [100.0, 20.0],
        ..UiText::default()
    };
    label.shadows.push(crate::draw_list::TextShadow {
        offset: [0.0, 12.0],
        blur: 0.0,
        color: [0.0, 0.0, 0.0, 1.0],
    });
    // Just under the box, but the shadow reaches it.
    let items = [UiDrawItem::Text(label), ui_rect([0.0, 26.0], [100.0, 10.0])];
    assert_eq!(ui_layers(&items), vec![0..1, 1..2]);
}

/// A list seen from the origin.
fn list_of(items: Vec<DrawItem>) -> DrawList {
    DrawList {
        sky: Sky {
            top_color: [0.0; 3],
            horizon_color: [0.0; 3],
            ground_color: [0.0; 3],
            environment_intensity: 0.0,
        },
        camera: Camera {
            position: Vec3::ZERO,
            look_at: Vec3::NEG_Z,
            fov_y_degrees: 70.0,
        },
        sun: Sun {
            direction: Vec3::Y,
            color: [1.0; 3],
            intensity: 1.0,
        },
        ambient: AmbientLight {
            color: [1.0; 3],
            intensity: 0.0,
        },
        items,
        sprites: Vec::new(),
        shadows: Shadows::default(),
        lights: Vec::new(),
        post: PostProcessing {
            enabled: false,
            ..PostProcessing::default()
        },
        ui: Vec::new(),
        fonts: Vec::new(),
    }
}

/// A unit-sized item at `position`.
fn item(shape: Shape, position: Vec3, opacity: f32) -> DrawItem {
    DrawItem {
        shape,
        transform: Mat4::from_translation(position),
        material: Material {
            opacity,
            ..Material::default()
        },
        uv_tile: 0.0,
        casts_shadow: true,
    }
}

fn batch(mesh: MeshKey, transparent: bool, instances: Range<u32>) -> Batch {
    Batch {
        mesh,
        material: MaterialKey::default(),
        transparent,
        casts_shadow: true,
        instances,
    }
}

/// A caller's mesh with no triangles, for planning tests (it's never uploaded).
struct TestMesh(u64);

impl MeshSource for TestMesh {
    fn key(&self) -> u64 {
        self.0
    }
    fn positions(&self) -> &[[f32; 3]] {
        &[]
    }
    fn normals(&self) -> &[[f32; 3]] {
        &[]
    }
    fn uvs(&self) -> &[[f32; 2]] {
        &[]
    }
    fn indices(&self) -> &[u32] {
        &[]
    }
}

/// One white pixel.
struct TestTexture(u64);

impl TextureSource for TestTexture {
    fn key(&self) -> u64 {
        self.0
    }
    fn size(&self) -> (u32, u32) {
        (1, 1)
    }
    fn rgba(&self) -> &[u8] {
        &[255; 4]
    }
}

fn textured(shape: Shape, texture: Option<u64>) -> DrawItem {
    let mut item = item(shape, Vec3::ZERO, 1.0);
    item.material.base_color_texture =
        texture.map(|key| Arc::new(TestTexture(key)) as Arc<dyn TextureSource>);
    item
}

#[test]
fn callers_meshes_are_grouped_by_mesh_then_texture() {
    let mesh = |key| Shape::Mesh(Arc::new(TestMesh(key)));
    let list = list_of(vec![
        textured(mesh(7), Some(2)),
        textured(mesh(5), None),
        textured(mesh(7), Some(1)),
        textured(mesh(7), Some(2)),
        textured(Shape::Block, Some(1)),
    ]);
    let (_, batches) = plan_draws(&list);
    let textures: Vec<(MeshKey, Option<u64>, Range<u32>)> = batches
        .into_iter()
        .map(|batch| (batch.mesh, batch.material.textures[0], batch.instances))
        .collect();
    assert_eq!(
        textures,
        vec![
            (MeshKey::Block, Some(1), 0..1),
            (MeshKey::Custom(5), None, 1..2),
            (MeshKey::Custom(7), Some(1), 2..3),
            (MeshKey::Custom(7), Some(2), 3..5),
        ]
    );
}

#[test]
fn maps_and_their_settings_pick_the_material_binding() {
    let mesh = Shape::Mesh(Arc::new(TestMesh(1)));
    let texture = |key| Arc::new(TestTexture(key)) as Arc<dyn TextureSource>;
    let mut packed = textured(mesh.clone(), None);
    // One ORM texture: occlusion in red, roughness in green, metalness in blue.
    let channel = |channel| {
        Some(ChannelMap {
            texture: texture(9),
            channel,
            invert: false,
        })
    };
    packed.material.maps.occlusion = channel(Channel::Red);
    packed.material.maps.roughness = channel(Channel::Green);
    packed.material.maps.metalness = channel(Channel::Blue);
    let key = material_key(&packed);
    assert_eq!(
        key.textures,
        [None, None, Some(9), Some(9), Some(9), None, None, None]
    );
    let maps = maps_uniform_of(&key);
    assert_eq!(maps.metalness, [0.0, 0.0, 1.0, 0.0]);
    assert_eq!(maps.roughness, [0.0, 1.0, 0.0, 0.0]);
    assert_eq!(maps.settings, [0.0, 1.0, 1.0, 0.0]);

    // The same textures read another way need another binding.
    let mut glossy = packed.clone();
    glossy.material.maps.roughness.as_mut().unwrap().invert = true;
    assert_ne!(material_key(&glossy), key);
    assert_eq!(maps_uniform_of(&material_key(&glossy)).invert[1], 1.0);
    // Built-in shapes have no texture coordinates, so no maps.
    let mut block = packed.clone();
    block.shape = Shape::Block;
    assert_eq!(material_key(&block), MaterialKey::default());
}

#[test]
fn mipmaps_halve_down_to_one_pixel_averaging_each_square() {
    // 3×2: an odd width, so the last column is averaged with itself.
    let rgba: Vec<u8> = [0, 100, 200, 40, 80, 120]
        .iter()
        .flat_map(|&v| [v, v, v, 255])
        .collect();
    let levels = mip_levels(3, 2, &rgba);
    let sizes: Vec<(u32, u32)> = levels.iter().map(|(w, h, _)| (*w, *h)).collect();
    assert_eq!(sizes, [(3, 2), (1, 1)]);
    // The top-left 2×2: (0 + 100 + 40 + 80) / 4 = 55.
    assert_eq!(levels[1].2[..4], [55, 55, 55, 255]);
}

#[test]
fn flat_and_invisible_shapes_are_skipped() {
    let mut flat = item(Shape::Block, Vec3::ZERO, 1.0);
    flat.transform = Mat4::from_scale(Vec3::new(1.0, 0.0, 1.0));
    let list = list_of(vec![
        item(Shape::Block, Vec3::ZERO, 1.0),
        flat,
        item(Shape::Block, Vec3::ZERO, 0.0),
    ]);
    let (instances, batches) = plan_draws(&list);
    assert_eq!(instances.len(), 1);
    assert_eq!(batches, vec![batch(MeshKey::Block, false, 0..1)]);
}

#[test]
fn solid_items_are_grouped_by_shape() {
    let list = list_of(vec![
        item(Shape::Wedge, Vec3::ZERO, 1.0),
        item(Shape::Block, Vec3::ZERO, 1.0),
        item(Shape::Wedge, Vec3::ZERO, 1.0),
        item(Shape::Block, Vec3::ZERO, 1.0),
    ]);
    let (_, batches) = plan_draws(&list);
    assert_eq!(
        batches,
        vec![
            batch(MeshKey::Block, false, 0..2),
            batch(MeshKey::Wedge, false, 2..4)
        ]
    );
}

#[test]
fn see_through_items_come_last_and_farthest_first() {
    let near_glass = item(Shape::Sphere, Vec3::new(0.0, 0.0, -2.0), 0.5);
    let far_glass = item(Shape::Sphere, Vec3::new(0.0, 0.0, -9.0), 0.5);
    let middle_glass = item(Shape::Block, Vec3::new(0.0, 0.0, -5.0), 0.5);
    let solid = item(Shape::Sphere, Vec3::new(0.0, 0.0, -1.0), 1.0);
    let list = list_of(vec![near_glass, far_glass, solid, middle_glass]);

    let (instances, batches) = plan_draws(&list);
    assert_eq!(
        batches,
        vec![
            batch(MeshKey::Sphere, false, 0..1),
            batch(MeshKey::Sphere, true, 1..2),
            batch(MeshKey::Block, true, 2..3),
            batch(MeshKey::Sphere, true, 3..4),
        ]
    );
    let z = |i: usize| instances[i].model[3][2];
    assert_eq!([z(1), z(2), z(3)], [-9.0, -5.0, -2.0]);
}

/// A GPU device for tests, or `None` on machines without one (some CI
/// runners have no graphics adapter at all).
fn test_device() -> Option<(wgpu::Device, wgpu::Queue)> {
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
    let adapter = pollster::block_on(instance.request_adapter(&Default::default())).ok()?;
    pollster::block_on(adapter.request_device(&Default::default())).ok()
}

fn point_light(position: Vec3, range: f32) -> Light {
    Light {
        position,
        color: [1.0; 3],
        brightness: 1.0,
        range,
        cone: None,
    }
}

#[test]
fn lights_are_listed_only_in_the_tiles_they_can_reach() {
    let mut list = list_of(vec![]);
    // Straight ahead and small: the middle tile only.
    list.lights
        .push(point_light(Vec3::new(0.0, 0.0, -50.0), 1.0));
    // Behind the camera: not drawn at all.
    list.lights
        .push(point_light(Vec3::new(0.0, 0.0, 50.0), 1.0));
    // Not a real light.
    list.lights.push(point_light(Vec3::ZERO, 0.0));
    let planned = plan_lights(&list, 1.0, (320, 320));
    assert_eq!(planned.lights.len(), 1);
    assert_eq!((planned.tile_size, planned.tiles_across), (32, 10));
    let count = |x: usize, y: usize| planned.tiles[(y * 10 + x) * (LIGHTS_PER_TILE + 1)];
    assert_eq!(count(5, 5), 1);
    assert_eq!(count(0, 0), 0);
    assert_eq!(count(9, 9), 0);
}

#[test]
fn a_light_around_the_camera_reaches_every_tile_and_the_nearest_are_kept() {
    let mut list = list_of(vec![]);
    list.lights
        .push(point_light(Vec3::new(0.0, 0.0, 0.1), 100.0));
    let planned = plan_lights(&list, 1.0, (64, 64));
    assert!(
        planned
            .tiles
            .chunks(LIGHTS_PER_TILE + 1)
            .all(|tile| tile[0] == 1)
    );

    list.lights = (0..MAX_LIGHTS + 10)
        .map(|i| point_light(Vec3::new(0.0, 0.0, -1.0 - i as f32), 5.0))
        .collect();
    let planned = plan_lights(&list, 1.0, (64, 64));
    assert_eq!(planned.lights.len(), MAX_LIGHTS);
    let farthest = planned
        .lights
        .iter()
        .map(|l| l.position_range[2])
        .fold(0.0, f32::min);
    assert_eq!(farthest, -(MAX_LIGHTS as f32));
}

#[test]
fn spot_lights_describe_their_cone() {
    let raw = light_raw(&Light {
        cone: Some(Cone {
            direction: Vec3::new(0.0, -2.0, 0.0),
            angle_degrees: 90.0,
        }),
        ..point_light(Vec3::ZERO, 10.0)
    });
    assert_eq!(raw.direction_inner[..3], [0.0, -1.0, 0.0]);
    assert!((raw.radiance_cone[3] - 45f32.to_radians().cos()).abs() < 1e-6);
    assert!(raw.direction_inner[3] > raw.radiance_cone[3]);
    assert_eq!(
        light_raw(&point_light(Vec3::ZERO, 1.0)).radiance_cone[3],
        -2.0
    );
}

/// The middle pixel of a `size` × `size` texture drawn from `list`.
fn middle_pixel(device: &wgpu::Device, queue: &wgpu::Queue, list: &DrawList) -> [u8; 4] {
    let mut renderer = TextureRenderer::new(device, queue, 64, 64);
    renderer.render(list).unwrap();
    let pixels = renderer.read_pixels();
    pixels[(32 * 64 + 32) * 4..][..4].try_into().unwrap()
}

fn big_block() -> DrawItem {
    DrawItem {
        shape: Shape::Block,
        transform: Mat4::from_scale_rotation_translation(
            Vec3::splat(4.0),
            glam::Quat::IDENTITY,
            Vec3::new(0.0, 0.0, -5.0),
        ),
        material: Material::default(),
        uv_tile: 0.0,
        casts_shadow: true,
    }
}

#[test]
fn parts_shadow_others_unless_they_dont_cast_shadows() {
    let Some((device, queue)) = test_device() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    let block = |size: Vec3, position: Vec3| DrawItem {
        transform: Mat4::from_scale_rotation_translation(size, glam::Quat::IDENTITY, position),
        ..big_block()
    };
    // A wall facing the camera, lit from the front right. The small
    // block's shadow falls on the middle of the wall, beside the block.
    let wall = block(Vec3::new(30.0, 30.0, 1.0), Vec3::new(0.0, 0.0, -10.0));
    let blocker = block(Vec3::splat(2.0), Vec3::new(4.0, 0.0, -6.0));
    let mut list = list_of(vec![wall, blocker]);
    list.sun.direction = Vec3::new(1.0, 0.0, 1.0);
    list.shadows.softness = 0.0;

    let shaded = middle_pixel(&device, &queue, &list);
    list.items[1].casts_shadow = false;
    let lit = middle_pixel(&device, &queue, &list);
    assert!(
        lit[0] > shaded[0] + 40,
        "the block's shadow is gone: {shaded:?} vs {lit:?}"
    );

    list.items[1].casts_shadow = true;
    list.shadows.enabled = false;
    assert_eq!(middle_pixel(&device, &queue, &list), lit, "shadows off");
}

#[test]
fn cascades_end_further_out_each_time_and_the_last_at_the_distance() {
    let ends = cascade_ends(150.0);
    assert!(ends[0] > 1.0 && ends[0] < ends[1] && ends[1] < ends[2]);
    assert!((ends[SHADOW_CASCADES - 1] - 150.0).abs() < 1e-3);
    // The nearest cascade is small, so it's the sharpest.
    assert!(ends[0] < 25.0, "{ends:?}");
}

#[test]
fn a_point_light_lights_the_part_in_front_of_it() {
    let Some((device, queue)) = test_device() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    let mut list = list_of(vec![big_block()]);
    list.sun.intensity = 0.0;
    let dark = middle_pixel(&device, &queue, &list);
    assert_eq!(dark[..3], [0, 0, 0], "no light, no color");

    list.lights
        .push(point_light(Vec3::new(0.0, 0.0, -1.0), 10.0));
    let lit = middle_pixel(&device, &queue, &list);
    assert!(lit[0] > 40, "the front face is lit: {lit:?}");

    // Out of reach, the light changes nothing.
    list.lights[0].range = 1.0;
    assert_eq!(middle_pixel(&device, &queue, &list)[..3], [0, 0, 0]);
}

#[test]
fn a_spot_light_lights_only_inside_its_cone() {
    let Some((device, queue)) = test_device() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    let mut list = list_of(vec![big_block()]);
    list.sun.intensity = 0.0;
    let mut light = point_light(Vec3::new(0.0, 0.0, -1.0), 10.0);
    light.cone = Some(Cone {
        direction: Vec3::NEG_Z,
        angle_degrees: 40.0,
    });
    list.lights.push(light);
    assert!(
        middle_pixel(&device, &queue, &list)[0] > 40,
        "inside the beam"
    );

    list.lights[0].cone = Some(Cone {
        direction: Vec3::X,
        angle_degrees: 40.0,
    });
    assert_eq!(middle_pixel(&device, &queue, &list)[..3], [0, 0, 0]);
}

#[test]
fn a_metal_part_reflects_the_sky() {
    let Some((device, queue)) = test_device() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    let mut block = big_block();
    block.material = Material {
        base_color: [1.0; 3],
        metalness: 1.0,
        roughness: 0.1,
        ..Material::default()
    };
    let mut list = list_of(vec![block]);
    list.sun.intensity = 0.0;
    assert_eq!(middle_pixel(&device, &queue, &list)[..3], [0, 0, 0]);
    list.sky = Sky::default();
    let reflecting = middle_pixel(&device, &queue, &list);
    assert!(
        reflecting[2] > 60,
        "the block shows the sky: {reflecting:?}"
    );
}

fn red_sprite(z: f32, light_emission: f32) -> crate::Sprite {
    crate::Sprite {
        position: Vec3::new(0.0, 0.0, z),
        size: 2.0,
        rotation_degrees: 0.0,
        color: [1.0, 0.0, 0.0],
        opacity: 1.0,
        brightness: 1.0,
        light_emission,
        texture: None,
    }
}

#[test]
fn sprites_show_in_front_and_hide_behind_solid_items() {
    let Some((device, queue)) = test_device() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    let mut list = list_of(Vec::new());
    list.sprites = vec![red_sprite(-3.0, 0.0)];
    let shown = middle_pixel(&device, &queue, &list);
    assert!(shown[0] > 200 && shown[1] < 40, "{shown:?}");

    // Additive: black behind plus red light is still red.
    list.sprites = vec![red_sprite(-3.0, 1.0)];
    let added = middle_pixel(&device, &queue, &list);
    assert!(added[0] > 200, "{added:?}");

    // Behind a solid block, it's hidden.
    list.items = vec![big_block()];
    list.sprites = vec![red_sprite(-9.0, 0.0)];
    let block_only = {
        let mut plain = list_of(vec![big_block()]);
        plain.sprites.clear();
        middle_pixel(&device, &queue, &plain)
    };
    assert_eq!(middle_pixel(&device, &queue, &list), block_only);
}

#[test]
fn bloom_makes_bright_parts_glow_past_their_edges() {
    let Some((device, queue)) = test_device() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    let glowing = DrawItem {
        material: Material {
            base_color: [0.0; 3],
            emissive: [4.0, 0.5, 0.5],
            ..Material::default()
        },
        transform: Mat4::from_scale_rotation_translation(
            Vec3::splat(2.0),
            glam::Quat::IDENTITY,
            Vec3::new(0.0, 0.0, -5.0),
        ),
        ..big_block()
    };
    let mut list = list_of(vec![glowing]);
    // The block covers pixels 21 to 43 across; pixel 14 is in the sky
    // just outside it.
    let outside = |list: &DrawList| {
        let mut renderer = TextureRenderer::new(&device, &queue, 64, 64);
        renderer.render(list).unwrap();
        renderer.read_pixels()[(32 * 64 + 14) * 4..][..4].to_vec()
    };
    list.post.enabled = true;
    list.post.bloom_enabled = false;
    let without = outside(&list);
    list.post.bloom_enabled = true;
    let with = outside(&list);
    assert!(
        with[0] > without[0],
        "bloom adds glow: {without:?} vs {with:?}"
    );

    // Tone mapping keeps the block's bright color from clipping to flat white.
    let middle = middle_pixel(&device, &queue, &list);
    assert!(
        middle[0] > middle[1],
        "still red, not clipped white: {middle:?}"
    );
}

#[test]
fn stats_count_draws_and_triangles_and_time_the_gpu_where_it_can() {
    let Some((device, queue)) = test_device() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    let mut renderer = TextureRenderer::new(&device, &queue, 32, 32);
    renderer.render(&list_of(vec![])).unwrap();
    assert_eq!(renderer.stats().draw_calls, 0);
    let mut two = list_of(vec![big_block(), big_block()]);
    two.shadows.enabled = false;
    renderer.render(&two).unwrap();
    let stats = renderer.stats();
    // Both blocks share a mesh: one draw of two instances.
    assert_eq!(stats.draw_calls, 1);
    assert_eq!(stats.triangles, 2 * 12, "a block is 12 triangles");
    // Each shadow cascade draws them again.
    two.shadows.enabled = true;
    renderer.render(&two).unwrap();
    assert!(renderer.stats().draw_calls > 1);
    // A device opened without timestamps never times anything.
    assert!(!stats.gpu_timing && stats.gpu_time.is_none());

    // With them (where the adapter has them), a reading arrives a frame
    // or two after the GPU finishes, without waiting for it.
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
    let Ok(adapter) = pollster::block_on(instance.request_adapter(&Default::default())) else {
        return;
    };
    if !adapter.features().contains(wgpu::Features::TIMESTAMP_QUERY) {
        eprintln!("skipped GPU timing: the adapter has no timestamps");
        return;
    }
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        required_features: wgpu::Features::TIMESTAMP_QUERY,
        ..Default::default()
    }))
    .unwrap();
    let mut renderer = TextureRenderer::new(&device, &queue, 32, 32);
    let list = list_of(vec![big_block()]);
    for _ in 0..10 {
        renderer.render(&list).unwrap();
        device.poll(wgpu::PollType::Wait).unwrap();
    }
    let stats = renderer.stats();
    assert!(stats.gpu_timing);
    assert!(stats.gpu_time.is_some(), "{stats:?}");
}

#[test]
fn texture_renderer_draws_the_sky_and_a_part_into_a_texture() {
    let Some((device, queue)) = test_device() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    // Pure red is 255 whether or not the GPU sRGB-encodes the background
    // clear (some drivers don't when drawing through an sRGB view).
    let red = [255, 0, 0, 255];
    let mut list = list_of(vec![]);
    list.sky.top_color = [1.0, 0.0, 0.0];
    list.sky.horizon_color = [1.0, 0.0, 0.0];
    list.sky.ground_color = [1.0, 0.0, 0.0];
    let mut renderer = TextureRenderer::new(&device, &queue, 64, 32);
    renderer.render(&list).unwrap();
    let pixels = renderer.read_pixels();
    assert_eq!(pixels.len(), 64 * 32 * 4);
    assert_eq!(pixels[..4], red);

    // A big glowing block straight ahead covers the middle of the view,
    // after resizing to a different shape. With no sun or ambient light,
    // its pixels are exactly its glow color.
    list.sun.intensity = 0.0;
    list.items.push(DrawItem {
        shape: Shape::Block,
        transform: Mat4::from_scale_rotation_translation(
            Vec3::splat(4.0),
            glam::Quat::IDENTITY,
            Vec3::new(0.0, 0.0, -5.0),
        ),
        material: Material {
            base_color: [0.0; 3],
            emissive: [0.63, 0.8, 0.95],
            ..Material::default()
        },
        uv_tile: 0.0,
        casts_shadow: true,
    });
    renderer.resize(64, 64);
    assert_eq!(renderer.size(), (64, 64));
    renderer.render(&list).unwrap();
    let pixels = renderer.read_pixels();
    let pixel = |x: usize, y: usize| &pixels[(y * 64 + x) * 4..][..4];
    assert_eq!(pixel(0, 0), red, "the corner shows the sky");
    // The texture holds the sRGB color as picked (0.63 × 255 ≈ 161), the
    // way egui expects, not linear light (which would be about 90).
    assert_eq!(
        pixel(32, 32),
        [161, 204, 242, 255],
        "the middle shows the block"
    );
}

/// A square facing +Z, filling the unit cube's front, with texture
/// coordinates and tangents.
struct Square;

impl MeshSource for Square {
    fn key(&self) -> u64 {
        1
    }
    fn positions(&self) -> &[[f32; 3]] {
        &[
            [-0.5, -0.5, 0.0],
            [0.5, -0.5, 0.0],
            [0.5, 0.5, 0.0],
            [-0.5, 0.5, 0.0],
        ]
    }
    fn normals(&self) -> &[[f32; 3]] {
        &[[0.0, 0.0, 1.0]; 4]
    }
    fn uvs(&self) -> &[[f32; 2]] {
        &[[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]]
    }
    fn tangents(&self) -> &[[f32; 4]] {
        &[[1.0, 0.0, 0.0, 1.0]; 4]
    }
    fn indices(&self) -> &[u32] {
        &[0, 1, 2, 0, 2, 3]
    }
}

/// One pixel of `rgba`.
struct Pixel(u64, [u8; 4]);

impl TextureSource for Pixel {
    fn key(&self) -> u64 {
        self.0
    }
    fn size(&self) -> (u32, u32) {
        (1, 1)
    }
    fn rgba(&self) -> &[u8] {
        &self.1
    }
}

#[test]
fn texture_renderer_draws_a_mesh_with_texture_maps() {
    let Some((device, queue)) = test_device() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    let mut list = list_of(vec![]);
    list.sun.intensity = 0.0;
    let pixel = |key, rgba| Arc::new(Pixel(key, rgba)) as Arc<dyn TextureSource>;
    let mut material = Material {
        base_color: [0.0; 3],
        emissive: [1.0; 3],
        ..Material::default()
    };
    // Glows only in the emissive map's green; the other maps are bound
    // but change nothing without light.
    material.maps.emissive = Some(pixel(2, [0, 255, 0, 255]));
    material.maps.normal = Some(NormalMap {
        texture: pixel(3, [128, 128, 255, 255]),
        scale: 1.0,
        flip_green: true,
    });
    material.maps.roughness = Some(ChannelMap {
        texture: pixel(4, [0, 0, 0, 255]),
        channel: Channel::Alpha,
        invert: true,
    });
    list.items.push(DrawItem {
        shape: Shape::Mesh(Arc::new(Square)),
        transform: Mat4::from_scale_rotation_translation(
            Vec3::splat(4.0),
            glam::Quat::IDENTITY,
            Vec3::new(0.0, 0.0, -3.0),
        ),
        material,
        uv_tile: 0.0,
        casts_shadow: true,
    });
    let mut renderer = TextureRenderer::new(&device, &queue, 32, 32);
    renderer.render(&list).unwrap();
    let pixels = renderer.read_pixels();
    assert_eq!(pixels[(16 * 32 + 16) * 4..][..4], [0, 255, 0, 255]);
}

#[test]
fn texture_renderer_never_makes_an_empty_texture() {
    let Some((device, queue)) = test_device() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    let mut renderer = TextureRenderer::new(&device, &queue, 0, 0);
    assert_eq!(renderer.size(), (1, 1));
    renderer.resize(300, 0);
    assert_eq!(renderer.size(), (300, 1));
}

/// Flat ground 20 studs across, as a terrain chunk would be.
struct Ground;

impl MeshSource for Ground {
    fn key(&self) -> u64 {
        9001
    }
    fn positions(&self) -> &[[f32; 3]] {
        &[
            [-10.0, 0.0, -10.0],
            [-10.0, 0.0, 10.0],
            [10.0, 0.0, -10.0],
            [10.0, 0.0, 10.0],
        ]
    }
    fn normals(&self) -> &[[f32; 3]] {
        &[[0.0, 1.0, 0.0]; 4]
    }
    fn uvs(&self) -> &[[f32; 2]] {
        &[[0.0, 0.0], [0.0, 1.0], [1.0, 0.0], [1.0, 1.0]]
    }
    fn indices(&self) -> &[u32] {
        &[0, 1, 2, 1, 3, 2]
    }
}

/// The same weights everywhere.
struct Weights(u64, [u8; 4]);

impl TextureSource for Weights {
    fn key(&self) -> u64 {
        self.0
    }
    fn size(&self) -> (u32, u32) {
        (1, 1)
    }
    fn rgba(&self) -> &[u8] {
        &self.1
    }
}

#[test]
fn terrain_chunks_show_the_layer_their_weights_pick() {
    let Some((device, queue)) = test_device() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    let layer = |color| crate::TerrainLayer {
        color,
        ..terrain::tests::layer()
    };
    let layers = Arc::new(crate::TerrainLayers {
        id: 1,
        key: 1,
        layers: vec![layer([1.0, 0.0, 0.0]), layer([0.0, 0.0, 1.0])],
        view: crate::TerrainView::Shaded,
        height_range: [0.0, 1.0],
        grid_origin: [-10.0, -10.0],
        grid_cell: [1.0, 1.0],
    });
    let ground = |weights: [u8; 4], key| {
        let mut list = list_of(vec![DrawItem {
            shape: Shape::Mesh(Arc::new(Ground)),
            transform: Mat4::from_translation(Vec3::new(0.0, -2.0, 0.0)),
            material: Material {
                terrain: Some(crate::TerrainSurface {
                    layers: layers.clone(),
                    weights: [
                        Arc::new(Weights(key, weights)),
                        Arc::new(Weights(key + 1, [0; 4])),
                    ],
                }),
                ..Material::default()
            },
            uv_tile: 0.0,
            casts_shadow: true,
        }]);
        // Looking straight down at it, in sunlight.
        list.camera.look_at = Vec3::new(0.0, -1.0, -0.001);
        list.ambient.intensity = 0.2;
        list
    };
    let [red, _, blue, _] = middle_pixel(&device, &queue, &ground([255, 0, 0, 0], 10));
    // Some of the blue is the sun's white highlight.
    assert!(
        red > 100 && red > blue * 3,
        "the first layer: {red}, {blue}"
    );
    let [red, _, blue, _] = middle_pixel(&device, &queue, &ground([0, 255, 0, 0], 20));
    assert!(
        blue > 100 && blue > red * 3,
        "the second layer: {red}, {blue}"
    );
    let [red, _, blue, _] = middle_pixel(&device, &queue, &ground([128, 127, 0, 0], 30));
    assert!(red > 40 && blue > 40, "half and half: {red}, {blue}");
}

#[test]
fn a_sliced_image_keeps_its_corners_whatever_its_size() {
    let slice = crate::UiSlice {
        insets: [10.0; 4],
        widths: [10.0; 4],
        fill: true,
        repeat: false,
    };
    for [w, h] in [[100.0, 100.0], [600.0, 200.0]] {
        let pieces = super::ui_pass::slice_pieces(&slice, [0.0, 0.0, w, h], [30.0, 30.0]);
        assert_eq!(pieces.len(), 9);
        // The top left corner is 10 × 10 and shows the texture's corner,
        // up to half a texel inside the cut.
        assert_eq!(
            pieces[0],
            ([0.0, 0.0, 10.0, 10.0], [0.0, 0.0, 9.5 / 30.0, 9.5 / 30.0])
        );
        // The bottom right one too, at the far corner.
        assert_eq!(pieces[8].0, [w - 10.0, h - 10.0, 10.0, 10.0]);
        // The middle stretches over the rest.
        assert_eq!(pieces[4].0, [10.0, 10.0, w - 20.0, h - 20.0]);
    }
    // Without fill there's no middle; repeating tiles the edges at the
    // corners' scale, the last tile cut short.
    let tiled = crate::UiSlice {
        fill: false,
        repeat: true,
        ..slice
    };
    let pieces = super::ui_pass::slice_pieces(&tiled, [0.0, 0.0, 45.0, 30.0], [30.0, 30.0]);
    let top: Vec<_> = pieces.iter().filter(|(rect, _)| rect[1] == 0.0).collect();
    assert_eq!(
        top.iter().map(|(rect, _)| rect[2]).collect::<Vec<_>>(),
        [10.0, 10.0, 10.0, 5.0, 10.0]
    );
    assert!(
        !pieces
            .iter()
            .any(|(rect, _)| rect[0] == 10.0 && rect[1] == 10.0)
    );
    // Corners too big for the box shrink together.
    let pieces = super::ui_pass::slice_pieces(&slice, [0.0, 0.0, 10.0, 40.0], [30.0, 30.0]);
    assert_eq!(pieces[0].0, [0.0, 0.0, 5.0, 5.0]);
}

/// A texture of `size` × `size` pixels, each the colour `color(x, y)` gives.
struct Painted {
    size: u32,
    rgba: Vec<u8>,
}

impl Painted {
    fn new(size: u32, color: impl Fn(u32, u32) -> [u8; 4]) -> Self {
        let rgba = (0..size)
            .flat_map(|y| (0..size).map(move |x| (x, y)))
            .flat_map(|(x, y)| color(x, y))
            .collect();
        Painted { size, rgba }
    }
}

impl TextureSource for Painted {
    fn key(&self) -> u64 {
        0x5_11ce
    }
    fn size(&self) -> (u32, u32) {
        (self.size, self.size)
    }
    fn rgba(&self) -> &[u8] {
        &self.rgba
    }
}

/// The pixels of a `width` × `height` frame showing only `ui` over black.
fn ui_pixels(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    ui: Vec<UiDrawItem>,
    (width, height): (u32, u32),
) -> impl Fn(u32, u32) -> [u8; 4] {
    let mut list = list_of(Vec::new());
    list.ui = ui;
    let mut renderer = TextureRenderer::new(device, queue, width, height);
    renderer.render(&list).unwrap();
    let pixels = renderer.read_pixels();
    move |x, y| {
        pixels[((y * width + x) * 4) as usize..][..4]
            .try_into()
            .unwrap()
    }
}

#[test]
fn a_nine_slice_frame_draws_its_corners_unstretched_on_the_gpu() {
    let Some((device, queue)) = test_device() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    // 30 × 30: red corners, green edges and a blue middle, 10 pixels each.
    let texture = Arc::new(Painted::new(30, |x, y| {
        match ((x / 10) == 1, (y / 10) == 1) {
            (false, false) => [255, 0, 0, 255],
            (true, true) => [0, 0, 255, 255],
            _ => [0, 255, 0, 255],
        }
    }));
    let image = |position: [f32; 2], size: [f32; 2]| {
        UiDrawItem::Image(crate::UiImage {
            position,
            size,
            tint: [1.0; 4],
            border_radius: 0.0,
            path: "frame.png".to_owned(),
            texture: Some(texture.clone()),
            slice: Some(crate::UiSlice {
                insets: [10.0; 4],
                widths: [10.0; 4],
                fill: true,
                repeat: false,
            }),
        })
    };
    let red = [255, 0, 0, 255];
    let green = [0, 255, 0, 255];
    let blue = [0, 0, 255, 255];
    for [w, h] in [[100u32, 100u32], [600, 200]] {
        let pixel = ui_pixels(
            &device,
            &queue,
            vec![image([20.0, 20.0], [w as f32, h as f32])],
            (w + 40, h + 40),
        );
        for (x, y) in [(23, 23), (w + 16, 23), (23, h + 16), (w + 16, h + 16)] {
            assert_eq!(pixel(x, y), red, "a corner at {x}, {y} on {w} × {h}");
        }
        // Just past a 10 pixel corner is an edge, at either size.
        assert_eq!(pixel(33, 23), green, "the top edge on {w} × {h}");
        assert_eq!(pixel(23, 33), green, "the left edge on {w} × {h}");
        assert_eq!(
            pixel(20 + w / 2, 20 + h / 2),
            blue,
            "the middle on {w} × {h}"
        );
    }
}

#[test]
fn borders_and_inset_shadows_keep_inside_rounded_corners_on_the_gpu() {
    let Some((device, queue)) = test_device() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    let side = |color: [f32; 4]| crate::UiBorderSide { width: 6.0, color };
    let red = [1.0, 0.0, 0.0, 1.0];
    let blue = [0.0, 0.0, 1.0, 1.0];
    let rounded = UiRectangle {
        position: [8.0, 8.0],
        size: [48.0, 48.0],
        color: [0.5, 0.5, 0.5, 1.0],
        color_end: [0.5, 0.5, 0.5, 1.0],
        border_radius: 16.0,
        ..UiRectangle::default()
    };
    // An inset-style border: lit top and left, shaded bottom and right.
    let bordered = UiRectangle {
        border_sides: Some([side(red), side(blue), side(blue), side(red)]),
        ..rounded
    };
    // A ridge's inner half, over it.
    let inner = UiRectangle {
        position: [11.0, 11.0],
        size: [42.0, 42.0],
        color: [0.0; 4],
        color_end: [0.0; 4],
        border_radius: 13.0,
        border_sides: Some(
            [crate::UiBorderSide {
                width: 3.0,
                color: blue,
            }; 4],
        ),
        ..UiRectangle::default()
    };
    let shadow = UiRectangle {
        color: [1.0, 1.0, 1.0, 1.0],
        color_end: [1.0, 1.0, 1.0, 1.0],
        shadow: Some(crate::UiShadow {
            offset: [6.0, 6.0],
            blur: 0.0,
            spread: 0.0,
            inset: true,
        }),
        ..rounded
    };
    let black = [0, 0, 0, 255];
    for (name, items) in [
        ("border", vec![UiDrawItem::Rectangle(bordered)]),
        (
            "ridge",
            vec![
                UiDrawItem::Rectangle(bordered),
                UiDrawItem::Rectangle(inner),
            ],
        ),
        (
            "inset shadow",
            vec![
                UiDrawItem::Rectangle(rounded),
                UiDrawItem::Rectangle(shadow),
            ],
        ),
    ] {
        let pixel = ui_pixels(&device, &queue, items, (64, 64));
        // The square's corner, outside its rounding, stays clear.
        for (x, y) in [(9, 9), (54, 9), (9, 54), (54, 54)] {
            assert_eq!(pixel(x, y), black, "{name}: the corner at {x}, {y}");
        }
        // The middle of the top edge has the border or shadow.
        assert_ne!(pixel(32, 9), black, "{name}: the top edge");
    }
    let pixel = ui_pixels(
        &device,
        &queue,
        vec![UiDrawItem::Rectangle(bordered)],
        (64, 64),
    );
    assert_eq!(pixel(32, 9)[..3], [255, 0, 0], "lit top");
    assert_eq!(pixel(54, 32)[..3], [0, 0, 255], "shaded right");
    // The inset shadow lights the top and left edges only.
    let pixel = ui_pixels(
        &device,
        &queue,
        vec![
            UiDrawItem::Rectangle(rounded),
            UiDrawItem::Rectangle(shadow),
        ],
        (64, 64),
    );
    assert_eq!(pixel(32, 10)[..3], [255, 255, 255], "the shadow's top");
    assert_ne!(
        pixel(32, 53)[..3],
        [255, 255, 255],
        "no shadow at the bottom"
    );
}

#[test]
fn an_outer_shadow_with_blur_reaches_past_its_box_and_counts_toward_overlap() {
    let shadow = UiDrawItem::Rectangle(UiRectangle {
        position: [0.0, 0.0],
        size: [20.0, 20.0],
        color: [0.0, 0.0, 0.0, 0.5],
        shadow: Some(crate::UiShadow {
            offset: [0.0, 4.0],
            blur: 6.0,
            spread: 2.0,
            inset: false,
        }),
        ..UiRectangle::default()
    });
    let vertices = ui_vertices(std::slice::from_ref(&shadow), 0..1, &[], (100, 100));
    // Its quad is the box moved 4 down, grown by the spread and the blur.
    assert_eq!(vertices[0].size, [36.0, 36.0]);
    assert_eq!(vertices[0].border[1], UI_SHADOW);
    // Text just below the box is under the shadow, so the shadow goes in
    // an earlier layer.
    let items = [ui_label([0.0, 24.0], [20.0, 10.0]), shadow];
    assert_eq!(ui_layers(&items), vec![0..1, 1..2]);
}
