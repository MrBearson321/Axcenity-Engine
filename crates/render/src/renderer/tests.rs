use super::*;
use crate::ToneMapping;
use crate::draw_list::{
    AmbientLight, Camera, Channel, Cone, DrawItem, Light, Material, NormalMap, PostProcessing,
    RectArea, Shadows, Sky, Sun,
};
use crate::graphics::ShadowQuality;
use glam::Mat4;

mod ambient_occlusion;
mod anti_aliasing;
mod clouds;
mod color_grading;
mod cooked_textures;
#[cfg(feature = "debug-views")]
mod debug_views;
mod decals;
mod environment_maps;
mod fog;
mod frame_planning;
mod frustum_culling;
mod gpu_particles;
mod grid;
mod lens;
mod level_of_detail;
mod light_probe_volumes;
mod outlines;
mod point_and_contact_shadows;
mod ray_tracing;
mod realtime_gi;
mod reflection_probes;
mod shading;
mod shadows;
mod skinning;
mod streaming;
mod surface_shaders;
mod temporal;
mod texture_budget;
mod ui;
mod upscaling;
mod water;
mod weather;
mod wind;

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

/// A list seen from the origin.
fn list_of(items: Vec<DrawItem>) -> DrawList {
    DrawList {
        sky: Sky {
            top_color: [0.0; 3],
            horizon_color: [0.0; 3],
            ground_color: [0.0; 3],
            environment_intensity: 0.0,
            image: None,
            ..Sky::default()
        },
        fog: crate::Fog::default(),
        clouds: crate::Clouds::default(),
        weather: crate::Weather::default(),
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
            occlusion: 1.0,
        },
        items,
        chunks: Vec::new(),
        sprites: Vec::new(),
        particle_emitters: Vec::new(),
        shadows: Shadows::default(),
        lights: Vec::new(),
        post: PostProcessing {
            enabled: false,
            ..PostProcessing::default()
        },
        ui: Vec::new(),
        fonts: Vec::new(),
        reflection_probes: Vec::new(),
        light_probe_volumes: Vec::new(),
        time: 0.0,
        grid: None,
        wind: Vec3::ZERO,
        edges: None,
        grading: None,
        lens: crate::LensEffects::default(),
        decals: Vec::new(),
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
        id: 0,
        cull_distance: 0.0,
        skin: None,
        sway: None,
        decal_role: Default::default(),
    }
}

fn batch(mesh: MeshKey, transparent: bool, instances: Range<u32>) -> Batch {
    Batch {
        mesh,
        prefilled: false,
        material: MaterialKey::default(),
        transparent,
        double_sided: false,
        casts_shadow: true,
        shader: None,
        water: false,
        outline: false,
        instances,
    }
}

/// Plans `list` as its camera sees it on a square target, with every shape
/// in the unit cube.
fn plan(list: &DrawList) -> DrawPlan {
    plan_with(list, |_| Bounds::UNIT)
}

/// Plans `list` as its camera sees it on a square target, at each item's
/// level of detail on a first frame, with each mesh's box from `bounds`.
fn plan_with(list: &DrawList, bounds: impl Fn(MeshKey) -> Bounds + Sync) -> DrawPlan {
    let view = Frustum::of(view_projection(list, 1.0));
    let draws = LodStates::default().resolve(list, 1, None).draws;
    let uses = FrameUses::of(&draws);
    plan_draws(list, &draws, &uses, &view, |mesh, _| bounds(mesh), None)
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
    let batches = plan(&list).batches;
    let textures: Vec<(MeshKey, Option<u64>, Range<u32>)> = batches
        .into_iter()
        .map(|batch| (batch.mesh, batch.material.textures[0], batch.instances))
        .collect();
    // Within a mesh, materials come in the order the list first uses them.
    assert_eq!(
        textures,
        vec![
            (MeshKey::Block, Some(1), 0..1),
            (MeshKey::Custom(5), None, 1..2),
            (MeshKey::Custom(7), Some(2), 2..4),
            (MeshKey::Custom(7), Some(1), 4..5),
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
    // A SurfaceAppearance in a Part draws its maps on built-in shapes too.
    let mut block = packed.clone();
    block.shape = Shape::Block;
    assert_eq!(material_key(&block), key);
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
    let DrawPlan {
        instances, batches, ..
    } = plan(&list);
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
    let batches = plan(&list).batches;
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

    let DrawPlan {
        instances, batches, ..
    } = plan(&list);
    assert_eq!(
        batches,
        vec![
            batch(MeshKey::Sphere, false, 0..1),
            batch(MeshKey::Sphere, true, 1..2),
            batch(MeshKey::Block, true, 2..3),
            batch(MeshKey::Sphere, true, 3..4),
        ]
    );
    let instances: Vec<_> = instances.iter().flatten().collect();
    let z = |i: usize| instances[i].model[3][2];
    assert_eq!([z(1), z(2), z(3)], [-9.0, -5.0, -2.0]);
}

/// A GPU device for tests, or `None` on machines without one (some CI
/// runners have no graphics adapter at all).
pub(super) fn test_device() -> Option<(wgpu::Device, wgpu::Queue)> {
    let instance = crate::adapters::instance();
    let adapter = pollster::block_on(crate::adapters::adapter(&instance, None)).ok()?;
    pollster::block_on(adapter.request_device(&Default::default())).ok()
}

/// A GPU device with hardware ray tracing (wgpu's ray queries), or `None`
/// where the GPU (or its driver) has none. Lavapipe, Mesa's software
/// Vulkan, has them, so these run on machines without a GPU too.
pub(super) fn ray_test_device() -> Option<(wgpu::Device, wgpu::Queue)> {
    let instance = crate::adapters::instance();
    let adapter = pollster::block_on(crate::adapters::adapter(&instance, None)).ok()?;
    let mut descriptor = wgpu::DeviceDescriptor::default();
    crate::adapters::with_ray_queries(&adapter, &mut descriptor);
    if !descriptor
        .required_features
        .contains(wgpu::Features::EXPERIMENTAL_RAY_QUERY)
    {
        return None;
    }
    pollster::block_on(adapter.request_device(&descriptor)).ok()
}

fn point_light(position: Vec3, range: f32) -> Light {
    Light {
        position,
        color: [1.0; 3],
        brightness: 1.0,
        range,
        cone: None,
        rect: None,
        point_shadows: None,
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
    let planned = plan_lights(&list, 1.0, (320, 320), ShadowQuality::Off);
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
    let planned = plan_lights(&list, 1.0, (64, 64), ShadowQuality::Off);
    assert!(
        planned
            .tiles
            .chunks(LIGHTS_PER_TILE + 1)
            .all(|tile| tile[0] == 1)
    );

    list.lights = (0..MAX_LIGHTS + 10)
        .map(|i| point_light(Vec3::new(0.0, 0.0, -1.0 - i as f32), 5.0))
        .collect();
    let planned = plan_lights(&list, 1.0, (64, 64), ShadowQuality::Off);
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
            shadows: None,
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
        id: 0,
        cull_distance: 0.0,
        skin: None,
        sway: None,
        decal_role: Default::default(),
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
    let ends = cascade_ends(150.0, 3);
    assert!(ends[0] > 1.0 && ends[0] < ends[1] && ends[1] < ends[2]);
    assert!((ends[2] - 150.0).abs() < 1e-3);
    let two = cascade_ends(150.0, 2);
    assert!(two[0] < two[1] && (two[1] - 150.0).abs() < 1e-3);
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
        shadows: None,
    });
    list.lights.push(light);
    assert!(
        middle_pixel(&device, &queue, &list)[0] > 40,
        "inside the beam"
    );

    list.lights[0].cone = Some(Cone {
        direction: Vec3::X,
        angle_degrees: 40.0,
        shadows: None,
    });
    assert_eq!(middle_pixel(&device, &queue, &list)[..3], [0, 0, 0]);
}

#[test]
fn a_rect_light_lights_what_is_in_front_of_it_from_its_whole_face() {
    let raw = light_raw(&Light {
        rect: Some(RectArea {
            direction: Vec3::new(0.0, 0.0, 3.0),
            right: Vec3::new(2.0, 0.0, 0.5),
            width: 4.0,
            height: 2.0,
        }),
        ..point_light(Vec3::ZERO, 10.0)
    });
    assert_eq!(raw.radiance_cone[3], -3.0);
    assert_eq!(raw.direction_inner[..3], [0.0, 0.0, 1.0]);
    // Its right is made square to the way it faces, half the width long.
    assert_eq!(raw.rect, [2.0, 0.0, 0.0, 1.0]);

    let Some((device, queue)) = test_device() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    let mut list = list_of(vec![big_block()]);
    list.sun.intensity = 0.0;
    // Facing the block's front, between it and the camera.
    let mut light = point_light(Vec3::new(0.0, 0.0, -1.0), 10.0);
    light.rect = Some(RectArea {
        direction: Vec3::NEG_Z,
        right: Vec3::X,
        width: 2.0,
        height: 2.0,
    });
    list.lights.push(light);
    let lit = middle_pixel(&device, &queue, &list);
    assert!(lit[0] > 40, "in front of it: {lit:?}");
    // Turned away, its back gives no light.
    list.lights[0].rect.as_mut().unwrap().direction = Vec3::Z;
    assert_eq!(middle_pixel(&device, &queue, &list)[..3], [0, 0, 0]);
    // Moved off to one side but still facing the same way, a wide one still
    // reaches the block from the near end of its face.
    list.lights[0].position = Vec3::new(6.0, 0.0, -1.0);
    list.lights[0].rect = Some(RectArea {
        direction: Vec3::NEG_Z,
        right: Vec3::X,
        width: 1.0,
        height: 1.0,
    });
    let narrow = middle_pixel(&device, &queue, &list);
    list.lights[0].rect.as_mut().unwrap().width = 12.0;
    let wide = middle_pixel(&device, &queue, &list);
    assert!(wide[0] > narrow[0] + 20, "{narrow:?} vs {wide:?}");
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

/// Draw-list colors are sRGB: the renderer turns them linear to shade and
/// the sRGB target turns them back. A matte grey card lit by flat white
/// light alone (no sun, sky or post-processing) comes back as its own
/// color. A black specular map leaves out the 4% a plastic reflects, so
/// only the diffuse color is left.
#[test]
fn a_grey_part_under_flat_white_light_comes_back_as_its_srgb_color() {
    let Some((device, queue)) = test_device() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    let no_reflection: Arc<dyn TextureSource> = Arc::new(Painted::new(1, |_, _| [0, 0, 0, 255]));
    for grey in [0.25, 0.5, 0.75] {
        let mut card = big_block();
        card.material = Material {
            base_color: [grey; 3],
            roughness: 1.0,
            maps: crate::draw_list::TextureMaps {
                specular: Some(no_reflection.clone()),
                ..Default::default()
            },
            ..Material::default()
        };
        let mut list = list_of(vec![card]);
        list.sun.intensity = 0.0;
        list.ambient.intensity = 1.0;
        let pixel = middle_pixel(&device, &queue, &list);
        let expected = (grey * 255.0).round() as u8;
        for channel in &pixel[..3] {
            assert!(
                channel.abs_diff(expected) <= 2,
                "grey {grey} came back as {pixel:?}, not {expected}"
            );
        }
    }
}

/// The normal matrix is the model matrix's inverse transpose, so a block
/// stretched along X shades its top face like an unstretched one.
#[test]
fn a_stretched_block_shades_its_top_face_like_a_unit_block() {
    let Some((device, queue)) = test_device() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    let block = |size: Vec3| DrawItem {
        transform: Mat4::from_scale_rotation_translation(
            size,
            glam::Quat::from_rotation_y(30f32.to_radians()),
            Vec3::ZERO,
        ),
        ..big_block()
    };
    // Looking down at the middle of the top face, lit from a slant.
    let mut list = list_of(vec![block(Vec3::splat(2.0))]);
    list.camera = Camera {
        position: Vec3::new(0.0, 5.0, 3.0),
        look_at: Vec3::new(0.0, 1.0, 0.0),
        fov_y_degrees: 30.0,
    };
    list.sun.direction = Vec3::new(0.4, 1.0, 0.6);
    list.ambient.intensity = 0.2;
    list.shadows.enabled = false;
    let unit = middle_pixel(&device, &queue, &list);
    list.items = vec![block(Vec3::new(8.0, 2.0, 2.0))];
    let stretched = middle_pixel(&device, &queue, &list);
    assert!(unit[0] > 40, "the top face is lit: {unit:?}");
    for channel in 0..3 {
        assert!(
            unit[channel].abs_diff(stretched[channel]) <= 1,
            "unit {unit:?} vs stretched {stretched:?}"
        );
    }
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
    // Ambient occlusion's prepass draws solid items again; see
    // `with_ambient_occlusion_off_no_prepass_or_its_targets_exist`.
    renderer.set_graphics(&with_occlusion(crate::AmbientOcclusion::Off));
    renderer.render(&list_of(vec![])).unwrap();
    assert_eq!(renderer.stats().draw_calls, 0);
    let mut two = list_of(vec![big_block(), big_block()]);
    two.shadows.enabled = false;
    renderer.render(&two).unwrap();
    let stats = renderer.stats();
    // Both blocks share a mesh: one draw of two instances.
    assert_eq!(stats.draw_calls, 1);
    assert_eq!(stats.triangles, 2 * 12, "a block is 12 triangles");
    let corners = crate::mesh::block().vertices.len() as u64;
    assert_eq!(stats.vertices, 2 * corners);
    // Each shadow cascade draws them again.
    two.shadows.enabled = true;
    renderer.render(&two).unwrap();
    assert!(renderer.stats().draw_calls > 1);
    // A device opened without timestamps never times anything.
    assert!(!stats.gpu_timing && stats.gpu_time.is_none());

    // With them (where the adapter has them), a reading arrives a frame
    // or two after the GPU finishes, without waiting for it.
    let instance = crate::adapters::instance();
    let Ok(adapter) = pollster::block_on(crate::adapters::adapter(&instance, None)) else {
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
        device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    }
    let stats = renderer.stats();
    assert!(stats.gpu_timing);
    assert!(stats.gpu_time.is_some(), "{stats:?}");
}

#[test]
fn stats_count_items_batches_and_gpu_bytes_by_kind() {
    let Some((device, queue)) = test_device() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    let mut renderer = TextureRenderer::new(&device, &queue, 64, 32);
    let mut settings = with_occlusion(crate::AmbientOcclusion::Off);
    settings.anti_aliasing = Some(crate::AntiAliasing::Off);
    renderer.set_graphics(&settings);
    let mut sphere = big_block();
    sphere.shape = Shape::Sphere;
    // Two blocks share a batch; the sphere is its own.
    let mut list = list_of(vec![big_block(), big_block(), sphere]);
    list.shadows.enabled = false;
    renderer.render(&list).unwrap();
    let stats = renderer.stats();
    assert_eq!(
        (stats.items_submitted, stats.items_drawn, stats.batches),
        (3, 3, 2)
    );
    assert!(stats.buffer_bytes > 0);
    let plain = stats.target_bytes;
    // At least the HDR target and the depth: 8 and 4 bytes a pixel.
    assert!(plain >= 64 * 32 * 12, "{plain}");

    // Shadows add their map, and MSAA its multisampled color and depth.
    list.shadows.enabled = true;
    renderer.render(&list).unwrap();
    let shadowed = renderer.stats().target_bytes;
    assert!(shadowed > plain + 1024 * 1024, "{plain} then {shadowed}");
    settings.anti_aliasing = Some(crate::AntiAliasing::Msaa4);
    renderer.set_graphics(&settings);
    renderer.render(&list).unwrap();
    if renderer.graphics().anti_aliasing == crate::AntiAliasing::Msaa4 {
        let multisampled = renderer.stats().target_bytes;
        assert!(
            multisampled >= shadowed + 64 * 32 * 12 * 3,
            "{multisampled}"
        );
    }
}

/// With timestamps, each pass a frame draws gets a time, and the passes it
/// skips don't.
#[test]
fn stats_time_each_pass_the_frame_draws() {
    let instance = crate::adapters::instance();
    let Ok(adapter) = pollster::block_on(crate::adapters::adapter(&instance, None)) else {
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
    let mut renderer = TextureRenderer::new(&device, &queue, 64, 32);
    let draw = |renderer: &mut TextureRenderer, list: &DrawList| {
        for _ in 0..10 {
            renderer.render(list).unwrap();
            device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
        }
        renderer.stats()
    };

    // Shadows and ambient occlusion on, no UI, fog or GPU particles.
    renderer.set_graphics(&with_occlusion(crate::AmbientOcclusion::High));
    let mut list = list_of(vec![big_block()]);
    list.shadows.enabled = true;
    let stats = draw(&mut renderer, &list);
    for pass in FramePass::ALL {
        let expected = !matches!(
            pass,
            FramePass::AccelerationStructures
                | FramePass::Ui
                | FramePass::VolumetricFog
                | FramePass::Particles
        );
        assert_eq!(stats.pass_time(pass).is_some(), expected, "{pass:?}");
    }
    let passes: Duration = stats.pass_times.iter().flatten().sum();
    assert!(passes <= stats.gpu_time.unwrap(), "{stats:?}");
    // Each cascade the preset draws is timed within the shadow pass, and
    // counts what it drew.
    let cascades = renderer
        .graphics()
        .shadow_quality
        .map()
        .map_or(0, |size| size.cascades as usize);
    assert!(cascades > 0);
    for cascade in 0..crate::SHADOW_CASCADE_COUNT {
        assert_eq!(
            stats.cascade_times[cascade].is_some(),
            cascade < cascades,
            "{cascade}"
        );
    }
    // Passes with nothing between them can overlap on the GPU, so a
    // cascade's time is only bounded by the frame's.
    for time in stats.cascade_times.iter().flatten() {
        assert!(*time <= stats.gpu_time.unwrap(), "{stats:?}");
    }
    assert!(
        stats.cascade_casters[0] > 0 && stats.cascade_triangles[0] > 0,
        "{stats:?}"
    );

    // Both off: no shadow, prepass or occlusion time.
    renderer.set_graphics(&with_occlusion(crate::AmbientOcclusion::Off));
    list.shadows.enabled = false;
    let stats = draw(&mut renderer, &list);
    let timed: Vec<_> = FramePass::ALL
        .into_iter()
        .filter(|&pass| stats.pass_time(pass).is_some())
        .collect();
    assert_eq!(timed, [FramePass::Scene, FramePass::PostProcessing]);
    assert!(stats.cascade_times.iter().all(Option::is_none));
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
        id: 0,
        cull_distance: 0.0,
        skin: None,
        sway: None,
        decal_role: Default::default(),
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
        id: 0,
        cull_distance: 0.0,
        skin: None,
        sway: None,
        decal_role: Default::default(),
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
            id: 0,
            cull_distance: 0.0,
            skin: None,
            sway: None,
            decal_role: Default::default(),
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

/// A wall facing the camera, lit from the front right, with a small block
/// whose shadow falls on the middle of the wall, about 10 studs away.
fn shadowed_wall() -> DrawList {
    let block = |size: Vec3, position: Vec3| DrawItem {
        transform: Mat4::from_scale_rotation_translation(size, glam::Quat::IDENTITY, position),
        ..big_block()
    };
    let wall = block(Vec3::new(30.0, 30.0, 1.0), Vec3::new(0.0, 0.0, -10.0));
    let blocker = block(Vec3::splat(2.0), Vec3::new(4.0, 0.0, -6.0));
    let mut list = list_of(vec![wall, blocker]);
    list.sun.direction = Vec3::new(1.0, 0.0, 1.0);
    list.shadows.softness = 0.0;
    list
}

fn middle_of(renderer: &mut TextureRenderer, list: &DrawList) -> [u8; 4] {
    renderer.render(list).unwrap();
    let (width, height) = renderer.size();
    let pixels = renderer.read_pixels();
    let middle = ((height / 2) * width + width / 2) as usize * 4;
    pixels[middle..][..4].try_into().unwrap()
}

fn with_shadows(quality: ShadowQuality) -> GraphicsSettings {
    GraphicsSettings {
        shadow_quality: Some(quality),
        ..GraphicsSettings::default()
    }
}

/// A point light at `position` that casts shadows, starting right at it.
fn shadowing_point_light(position: Vec3, range: f32) -> Light {
    Light {
        brightness: 4.0,
        point_shadows: Some(crate::PointShadows { near: [0.0; 6] }),
        ..point_light(position, range)
    }
}

/// A spot light at `position` shining along `direction` in a 90° beam,
/// casting shadows if `shadows`.
fn spot_light(position: Vec3, direction: Vec3, range: f32, shadows: bool) -> Light {
    Light {
        brightness: 4.0,
        cone: Some(Cone {
            direction,
            angle_degrees: 90.0,
            shadows: shadows.then_some(crate::SpotShadows { near: 0.0 }),
        }),
        ..point_light(position, range)
    }
}

/// The pixel of a 64 × 64 frame of `list` that `point` is drawn at.
fn pixel_at(renderer: &mut TextureRenderer, list: &DrawList, point: Vec3) -> [u8; 4] {
    renderer.render(list).unwrap();
    let clip = view_projection(list, 1.0) * point.extend(1.0);
    let ndc = clip.truncate() / clip.w;
    let x = ((ndc.x * 0.5 + 0.5) * 64.0) as usize;
    let y = ((0.5 - ndc.y * 0.5) * 64.0) as usize;
    let pixels = renderer.read_pixels();
    pixels[(y * 64 + x) * 4..][..4].try_into().unwrap()
}

#[test]
fn post_processing_off_in_the_settings_turns_the_worlds_off() {
    let world = PostProcessing::default();
    let off = GraphicsSettings {
        post_processing: Some(false),
        ..GraphicsSettings::default()
    }
    .effective(&Capabilities::default());
    assert!(!post_drawn(&world, &off).enabled);
    assert!(post_drawn(&world, &EffectiveGraphics::default()).enabled);
    let world_off = PostProcessing {
        enabled: false,
        ..world
    };
    assert!(!post_drawn(&world_off, &EffectiveGraphics::default()).enabled);
}

fn at_scale(render_scale: f32) -> GraphicsSettings {
    GraphicsSettings {
        render_scale: Some(render_scale),
        ..GraphicsSettings::default()
    }
}

/// A lit block over a sky, with a UI panel and its label at the top left.
fn scene_with_ui() -> DrawList {
    let mut list = list_of(vec![big_block()]);
    list.sky = Sky::default();
    list.ui = vec![
        ui_rect([4.0, 4.0], [120.0, 40.0]),
        ui_label([8.0, 8.0], [110.0, 30.0]),
    ];
    list
}

#[test]
fn half_render_scale_halves_the_scene_targets_and_keeps_ui_text_sharp() {
    let Some((device, queue)) = test_device() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    let list = scene_with_ui();
    let (width, height) = (160, 96);
    let mut renderer = TextureRenderer::new(&device, &queue, width, height);
    renderer.render(&list).unwrap();
    assert_eq!(renderer.scene.frame_targets.size, (160, 96));
    let full = renderer.read_pixels();

    renderer.set_graphics(&at_scale(0.5));
    renderer.render(&list).unwrap();
    assert_eq!(renderer.scene.frame_targets.size, (80, 48), "half each way");
    assert_eq!(renderer.size(), (160, 96), "the output keeps its size");
    assert_eq!(renderer.scene.targets_alive(), 1);
    let half = renderer.read_pixels();

    // The UI is drawn at the output's size either way, so the panel and its
    // text come out the same, pixel for pixel.
    let panel = |pixels: &[u8]| -> Vec<u8> {
        (4..44)
            .flat_map(|y| (4..124).map(move |x| (y * width + x) as usize * 4))
            .flat_map(|at| pixels[at..at + 4].to_vec())
            .collect()
    };
    assert_eq!(panel(&full), panel(&half));
    assert!(
        panel(&full).chunks(4).any(|pixel| pixel[0] > 128),
        "the label's text is drawn on the panel"
    );
    // The scene is drawn at half size and scaled up, so it's still where it was.
    let middle = |pixels: &[u8]| -> [u8; 4] {
        let at = ((height / 2) * width + width / 2) as usize * 4;
        pixels[at..at + 4].try_into().unwrap()
    };
    let (a, b) = (middle(&full), middle(&half));
    assert!(
        a.iter().zip(b).all(|(a, b)| a.abs_diff(b) <= 2),
        "{a:?} vs {b:?}"
    );
}

/// A `size` × `size` texture in a checker of grey shades.
struct Checker(u64, u32, Vec<u8>);

impl Checker {
    fn texture(key: u64, size: u32) -> Arc<dyn TextureSource> {
        let pixels = (0..size * size)
            .flat_map(|i| {
                let shade = if (i % size + i / size).is_multiple_of(2) {
                    60
                } else {
                    200
                };
                [shade, shade, shade, 255]
            })
            .collect();
        Arc::new(Checker(key, size, pixels))
    }
}

impl TextureSource for Checker {
    fn key(&self) -> u64 {
        self.0
    }
    fn size(&self) -> (u32, u32) {
        (self.1, self.1)
    }
    fn rgba(&self) -> &[u8] {
        &self.2
    }
}

fn textured_block(texture: Arc<dyn TextureSource>) -> DrawItem {
    let mut block = big_block();
    block.material.base_color_texture = Some(texture);
    block
}

fn with_textures(quality: crate::TextureQuality) -> GraphicsSettings {
    GraphicsSettings {
        texture_quality: Some(quality),
        ..GraphicsSettings::default()
    }
}

#[test]
fn half_texture_quality_uploads_a_1024_texture_at_512_and_a_quarter_of_the_bytes() {
    let Some((device, queue)) = test_device() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    let mut list = list_of(vec![textured_block(Checker::texture(7, 1024))]);
    let mut renderer = TextureRenderer::new(&device, &queue, 32, 32);
    let empty = renderer.stats().texture_bytes;
    renderer.render(&list).unwrap();
    let uploaded = |renderer: &TextureRenderer| {
        let texture = &renderer.scene.textures[&7].gpu.as_ref().unwrap().texture;
        (texture.width(), texture.height(), texture.mip_level_count())
    };
    assert_eq!(uploaded(&renderer), (1024, 1024, 11));
    let full = renderer.stats().texture_bytes - empty;
    assert_eq!(
        full,
        (0..=10)
            .map(|level| (1024u64 >> level).pow(2) * 4)
            .sum::<u64>()
    );

    renderer.set_graphics(&with_textures(crate::TextureQuality::Half));
    assert!(
        renderer.scene.textures.is_empty(),
        "emptied for uploading again"
    );
    assert!(renderer.scene.materials.is_empty());
    assert_eq!(
        renderer.stats().texture_bytes,
        empty,
        "the full copy is freed"
    );
    renderer.render(&list).unwrap();
    assert_eq!(uploaded(&renderer), (512, 512, 10), "with its smaller mips");
    let half = renderer.stats().texture_bytes - empty;
    let ratio = half as f64 / full as f64;
    assert!((0.24..0.26).contains(&ratio), "{half} of {full} bytes");

    // UI images keep their full size.
    list.ui.push(UiDrawItem::Image(crate::UiImage {
        position: [0.0; 2],
        size: [16.0, 16.0],
        tint: [1.0; 4],
        border_radius: 0.0,
        path: "checker.png".to_owned(),
        texture: Some(Checker::texture(8, 1024)),
        slice: None,
        clip: None,
        rotation_degrees: 0.0,
    }));
    renderer.render(&list).unwrap();
    let image = &renderer.scene.full_textures[&8]
        .gpu
        .as_ref()
        .unwrap()
        .texture;
    assert_eq!(image.width(), 1024);
}

#[test]
fn changing_render_scale_and_texture_quality_at_run_time_draws_the_next_frame_right() {
    let Some((device, queue)) = test_device() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    let mut list = scene_with_ui();
    list.items = vec![textured_block(Checker::texture(9, 64))];
    let (width, height) = (96, 64);
    // A renderer that starts with `settings`, for comparing against.
    let fresh = |settings: &GraphicsSettings| {
        let mut renderer = TextureRenderer::new(&device, &queue, width, height);
        renderer.set_graphics(settings);
        renderer.render(&list).unwrap();
        (renderer.read_pixels(), renderer.stats().texture_bytes)
    };
    let mut renderer = TextureRenderer::new(&device, &queue, width, height);
    let scales = [0.5, 2.0, 1.0, 0.75, 1.5];
    let qualities = crate::TextureQuality::ALL;
    for step in 0..20 {
        let settings = GraphicsSettings {
            render_scale: Some(scales[step % scales.len()]),
            texture_quality: Some(qualities[step % qualities.len()]),
            ..GraphicsSettings::default()
        };
        renderer.set_graphics(&settings);
        renderer.render(&list).unwrap();
        let expected = scene::scene_size(
            (width, height),
            renderer.graphics(),
            renderer.capabilities(),
        );
        assert_eq!(renderer.scene.frame_targets.size, expected);
        assert_eq!(renderer.scene.targets_alive(), 1, "step {step}");
        if step % 7 == 0 {
            let (pixels, bytes) = fresh(&settings);
            assert_eq!(renderer.read_pixels(), pixels, "step {step}: {settings:?}");
            assert_eq!(renderer.stats().texture_bytes, bytes, "step {step}");
        }
    }
    renderer.set_graphics(&GraphicsSettings::default());
    renderer.render(&list).unwrap();
    let (pixels, bytes) = fresh(&GraphicsSettings::default());
    assert_eq!(renderer.read_pixels(), pixels);
    assert_eq!(renderer.stats().texture_bytes, bytes, "nothing leaked");
    assert_eq!(renderer.scene.targets_alive(), 1);
}

/// A glowing white block turned 20° over a black sky: with no lighting
/// and post-processing off, every pixel is wholly one or the other unless
/// its edge is smoothed.
fn tilted_block() -> DrawList {
    let block = DrawItem {
        transform: Mat4::from_scale_rotation_translation(
            Vec3::splat(2.0),
            glam::Quat::from_rotation_z(20f32.to_radians()),
            Vec3::new(0.0, 0.0, -5.0),
        ),
        material: Material {
            base_color: [0.0; 3],
            emissive: [1.0; 3],
            ..Material::default()
        },
        ..big_block()
    };
    let mut list = list_of(vec![block]);
    list.shadows.enabled = false;
    list
}

/// A 2 × 1 texture: opaque white on the left, see-through on the right.
struct HalfCut;

impl TextureSource for HalfCut {
    fn key(&self) -> u64 {
        4242
    }
    fn size(&self) -> (u32, u32) {
        (2, 1)
    }
    fn rgba(&self) -> &[u8] {
        &[255, 255, 255, 255, 255, 255, 255, 0]
    }
}

/// A pixel to look at, by what should be drawn there.
type Probe = (&'static str, (u32, u32));

/// One of each kind of drawing over the default sky, at 96 × 64: a solid
/// block, a see-through one and a cut-out one in a row, a sprite above,
/// and terrain below, with the pixels to look at.
fn one_of_everything() -> (DrawList, Vec<Probe>) {
    let block = |x: f32, material: Material| DrawItem {
        transform: Mat4::from_scale_rotation_translation(
            Vec3::splat(3.0),
            glam::Quat::IDENTITY,
            Vec3::new(x, 1.0, -10.0),
        ),
        material,
        ..big_block()
    };
    let solid = block(
        -6.0,
        Material {
            base_color: [0.8, 0.1, 0.1],
            ..Material::default()
        },
    );
    let see_through = block(
        0.0,
        Material {
            base_color: [0.1, 0.1, 0.9],
            opacity: 0.5,
            ..Material::default()
        },
    );
    let cut_out = block(
        6.0,
        Material {
            base_color: [0.1, 0.8, 0.1],
            base_color_texture: Some(Arc::new(HalfCut)),
            alpha_cutoff: 0.5,
            ..Material::default()
        },
    );
    let layers = Arc::new(crate::TerrainLayers {
        id: 3,
        key: 3,
        layers: vec![crate::TerrainLayer {
            color: [0.6, 0.5, 0.2],
            ..terrain::tests::layer()
        }],
        view: crate::TerrainView::Shaded,
        height_range: [0.0, 1.0],
        grid_origin: [-10.0, -10.0],
        grid_cell: [1.0, 1.0],
    });
    let ground = DrawItem {
        shape: Shape::Mesh(Arc::new(Ground)),
        transform: Mat4::from_translation(Vec3::new(0.0, -2.0, 0.0)),
        material: Material {
            terrain: Some(crate::TerrainSurface {
                layers,
                weights: [
                    Arc::new(Weights(40, [255, 0, 0, 0])),
                    Arc::new(Weights(41, [0; 4])),
                ],
            }),
            ..Material::default()
        },
        uv_tile: 0.0,
        casts_shadow: true,
        id: 0,
        cull_distance: 0.0,
        skin: None,
        sway: None,
        decal_role: Default::default(),
    };
    let mut list = list_of(vec![solid, see_through, cut_out, ground]);
    list.sky = Sky::default();
    list.ambient.intensity = 0.3;
    list.sprites = vec![crate::Sprite {
        position: Vec3::new(0.0, 4.5, -10.0),
        ..red_sprite(0.0, 0.0)
    }];
    let probes = vec![
        ("sky", (4, 4)),
        ("solid", (16, 26)),
        ("see-through", (48, 26)),
        ("cut-out left", (76, 26)),
        ("cut-out right", (84, 26)),
        ("sprite", (48, 11)),
        ("terrain", (48, 56)),
    ];
    (list, probes)
}

/// A glowing block filling the middle of the view, lit by nothing else, so
/// the light reaching tone mapping is its emissive color times exposure.
fn glowing_block(color: [f32; 3], post: PostProcessing) -> DrawList {
    let mut block = big_block();
    block.material = Material {
        base_color: [0.0; 3],
        emissive: color,
        ..Material::default()
    };
    let mut list = list_of(vec![block]);
    list.sun.intensity = 0.0;
    list.post = post;
    list
}

/// What an sRGB target stores for linear `light` from 0 to 1.
fn srgb_byte(light: f32) -> u8 {
    let c = if light <= 0.0031308 {
        light * 12.92
    } else {
        1.055 * light.powf(1.0 / 2.4) - 0.055
    };
    (c * 255.0).round() as u8
}

#[test]
fn the_shaders_tone_mapping_curves_match_their_cpu_copies() {
    let Some((device, queue)) = test_device() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    let mut renderer = TextureRenderer::new(&device, &queue, 64, 64);
    for curve in [
        ToneMapping::Classic,
        ToneMapping::Neutral,
        ToneMapping::Filmic,
    ] {
        for color in [[1.0; 3], [1.0, 0.5, 0.2], [0.2, 0.6, 1.0]] {
            for exposure in [0.05, 0.3, 1.0, 2.5, 8.0] {
                let post = PostProcessing {
                    exposure,
                    tone_mapping: curve,
                    bloom_enabled: false,
                    ..PostProcessing::default()
                };
                let pixel = middle_of(&mut renderer, &glowing_block(color, post));
                let light = (lighting::linear_color(color) * exposure).to_array();
                let expected = curve.apply(light).map(srgb_byte);
                for channel in 0..3 {
                    assert!(
                        pixel[channel].abs_diff(expected[channel]) <= 2,
                        "{curve:?} of {color:?} × {exposure}: GPU {pixel:?}, CPU {expected:?}"
                    );
                }
            }
        }
    }
}

#[test]
fn with_post_processing_off_every_curve_is_a_plain_clamp() {
    let Some((device, queue)) = test_device() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    let mut renderer = TextureRenderer::new(&device, &queue, 64, 64);
    for curve in [
        ToneMapping::Classic,
        ToneMapping::Neutral,
        ToneMapping::Filmic,
    ] {
        let post = PostProcessing {
            enabled: false,
            exposure: 3.0,
            tone_mapping: curve,
            ..PostProcessing::default()
        };
        let pixel = middle_of(&mut renderer, &glowing_block([1.0, 0.5, 0.2], post));
        let expected = [255, 128, 51];
        for channel in 0..3 {
            assert!(
                pixel[channel].abs_diff(expected[channel]) <= 1,
                "{curve:?}: {pixel:?}"
            );
        }
    }
}

/// A plastic part reflects its color plus a few percent of white light,
/// which Neutral takes back off: under flat white light, a lit grey Plastic
/// part (roughness 0.5) comes out closer to its picked sRGB color than with
/// a plain clamp. It reflects a little under the 4% Neutral expects, so
/// mid-grey lands within 3 levels rather than exactly on it.
#[test]
fn neutral_tone_mapping_brings_a_lit_grey_part_back_toward_its_color() {
    let Some((device, queue)) = test_device() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    let mut renderer = TextureRenderer::new(&device, &queue, 64, 64);
    for grey in [0.25, 0.5, 0.75] {
        let mut card = big_block();
        card.material = Material {
            base_color: [grey; 3],
            roughness: 0.5,
            ..Material::default()
        };
        let mut list = list_of(vec![card]);
        list.sun.intensity = 0.0;
        list.ambient.intensity = 1.0;
        list.post = PostProcessing {
            tone_mapping: ToneMapping::Neutral,
            bloom_enabled: false,
            ..PostProcessing::default()
        };
        let neutral = middle_of(&mut renderer, &list)[0];
        list.post.enabled = false;
        let clamped = middle_of(&mut renderer, &list)[0];
        let picked = (grey * 255.0).round() as u8;
        assert!(
            neutral.abs_diff(picked) < clamped.abs_diff(picked),
            "grey {picked}: Neutral {neutral}, clamped {clamped}"
        );
        if grey >= 0.5 {
            assert!(neutral.abs_diff(picked) <= 3, "grey {picked}: {neutral}");
        }
    }
}

#[test]
fn double_sided_items_get_their_own_batch_and_texture_transforms_their_own_binding() {
    let list = list_of(vec![
        item(Shape::Block, Vec3::ZERO, 1.0),
        Material {
            double_sided: true,
            ..Material::default()
        }
        .on(item(Shape::Block, Vec3::ZERO, 1.0)),
        item(Shape::Block, Vec3::ZERO, 1.0),
    ]);
    let batches = plan(&list).batches;
    let sides: Vec<(bool, Range<u32>)> = batches
        .into_iter()
        .map(|batch| (batch.double_sided, batch.instances))
        .collect();
    assert_eq!(sides, vec![(false, 0..2), (true, 2..3)]);

    let plain = textured(Shape::Block, Some(1));
    let mut tiled = plain.clone();
    tiled.material.uv_scale = [2.0, 2.0];
    let mut shifted = plain.clone();
    shifted.material.uv_offset = [0.5, 0.0];
    let keys = [&plain, &tiled, &shifted].map(material_key);
    assert_ne!(keys[0], keys[1]);
    assert_ne!(keys[0], keys[2]);
    assert_eq!(maps_uniform_of(&keys[0]).uv_matrix, [1.0, 0.0, 0.0, 1.0]);
    assert_eq!(maps_uniform_of(&keys[1]).uv_matrix, [2.0, 0.0, 0.0, 2.0]);
    assert_eq!(maps_uniform_of(&keys[2]).uv_offset, [0.5, 0.0, 0.0, 0.0]);
}

impl Material {
    /// `item` drawn with this material.
    fn on(self, item: DrawItem) -> DrawItem {
        DrawItem {
            material: self,
            ..item
        }
    }
}

/// glTF's `KHR_texture_transform` turns coordinates counter-clockwise in a
/// texture whose V runs down: a quarter turn takes U's direction to -V.
#[test]
fn a_quarter_turn_takes_u_to_minus_v_after_scaling() {
    let material = Material {
        uv_scale: [2.0, 3.0],
        uv_rotation: std::f32::consts::FRAC_PI_2,
        ..Material::default()
    };
    let [a, b, c, d] = uv_matrix(&material);
    let turn = |[u, v]: [f32; 2]| [a * u + c * v, b * u + d * v];
    let close =
        |[x, y]: [f32; 2], [ex, ey]: [f32; 2]| (x - ex).abs() < 1e-6 && (y - ey).abs() < 1e-6;
    assert!(
        close(turn([1.0, 0.0]), [0.0, -2.0]),
        "{:?}",
        turn([1.0, 0.0])
    );
    assert!(
        close(turn([0.0, 1.0]), [3.0, 0.0]),
        "{:?}",
        turn([0.0, 1.0])
    );
}

/// Whether the texture shows red (`R`) or blue (`B`) at a quarter, three
/// eighths, five eighths and seven eighths of the way across the block's
/// face (U runs left to right on it).
fn colors_across(renderer: &mut TextureRenderer, material: &Material) -> String {
    let texture: Arc<dyn TextureSource> = Arc::new(Painted::new(64, |x, _| {
        if x < 32 {
            [255, 0, 0, 255]
        } else {
            [0, 0, 255, 255]
        }
    }));
    let mut block = big_block();
    block.material = Material {
        base_color: [1.0; 3],
        base_color_texture: Some(texture),
        ..material.clone()
    };
    let mut list = list_of(vec![block]);
    list.sun.intensity = 0.0;
    list.ambient.intensity = 1.0;
    renderer.render(&list).unwrap();
    let pixels = renderer.read_pixels();
    // The face is 4 studs wide, 3 in front of a 70° camera.
    let face = 64.0 * 4.0 / (2.0 * 3.0 * 35f32.to_radians().tan());
    [0.125, 0.375, 0.625, 0.875]
        .into_iter()
        .map(|u| {
            let x = (32.0 + (u - 0.5) * face) as usize;
            let pixel: [u8; 3] = pixels[(32 * 64 + x) * 4..][..3].try_into().unwrap();
            let [red, _, blue] = pixel.map(u16::from);
            if red > 3 * blue {
                'R'
            } else if blue > 3 * red {
                'B'
            } else {
                '?'
            }
        })
        .collect()
}

#[test]
fn tiling_repeats_a_texture_and_an_offset_shifts_it() {
    let Some((device, queue)) = test_device() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    let mut renderer = TextureRenderer::new(&device, &queue, 64, 64);
    let plain = Material::default();
    assert_eq!(colors_across(&mut renderer, &plain), "RRBB");
    let tiled = Material {
        uv_scale: [2.0, 1.0],
        ..Material::default()
    };
    assert_eq!(colors_across(&mut renderer, &tiled), "RBRB");
    let shifted = Material {
        uv_offset: [0.5, 0.0],
        ..Material::default()
    };
    assert_eq!(colors_across(&mut renderer, &shifted), "BBRR");
}

/// A double-sided square seen from behind is drawn, and lit as if its back
/// were its front, solid or see-through. A one-sided one isn't drawn at all.
#[test]
fn a_double_sided_square_shows_its_back_lit_like_its_front() {
    let Some((device, queue)) = test_device() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    let mut renderer = TextureRenderer::new(&device, &queue, 64, 64);
    let mut frame = |turned: bool, double_sided: bool, opacity: f32| {
        let turn = if turned {
            glam::Quat::from_rotation_y(std::f32::consts::PI)
        } else {
            glam::Quat::IDENTITY
        };
        let square = DrawItem {
            shape: Shape::Mesh(Arc::new(Square)),
            transform: Mat4::from_scale_rotation_translation(
                Vec3::splat(4.0),
                turn,
                Vec3::new(0.0, 0.0, -5.0),
            ),
            material: Material {
                base_color: [0.8; 3],
                roughness: 1.0,
                opacity,
                double_sided,
                ..Material::default()
            },
            uv_tile: 0.0,
            casts_shadow: true,
            id: 0,
            cull_distance: 0.0,
            skin: None,
            sway: None,
            decal_role: Default::default(),
        };
        let mut list = list_of(vec![square]);
        // Lit from behind the camera.
        list.sun.direction = Vec3::Z;
        list.shadows.enabled = false;
        middle_of(&mut renderer, &list)
    };
    for opacity in [1.0, 0.6] {
        let front = frame(false, true, opacity);
        assert!(front[0] > 60, "the front is lit: {front:?}");
        let back = frame(true, true, opacity);
        for channel in 0..3 {
            assert!(
                front[channel].abs_diff(back[channel]) <= 2,
                "opacity {opacity}: front {front:?}, back {back:?}"
            );
        }
        let one_sided = frame(true, false, opacity);
        assert_eq!(
            one_sided[..3],
            [0, 0, 0],
            "opacity {opacity}: no back drawn"
        );
        assert_eq!(frame(false, false, opacity), front);
    }
}

/// High with ambient occlusion at `mode`, and contact shadows off: they
/// draw the prepass too.
fn with_occlusion(mode: crate::AmbientOcclusion) -> GraphicsSettings {
    GraphicsSettings {
        ambient_occlusion: Some(mode),
        contact_shadows: Some(false),
        ..GraphicsSettings::default()
    }
}

/// A 4-stud block resting on a wide floor, seen from above and in front,
/// lit only by flat ambient light.
fn block_on_floor() -> DrawList {
    let block = |size: Vec3, position: Vec3| DrawItem {
        transform: Mat4::from_scale_rotation_translation(size, glam::Quat::IDENTITY, position),
        material: Material {
            base_color: [0.8; 3],
            roughness: 1.0,
            ..Material::default()
        },
        ..big_block()
    };
    let floor = block(Vec3::new(60.0, 1.0, 60.0), Vec3::new(0.0, -0.5, -10.0));
    let cube = block(Vec3::splat(4.0), Vec3::new(0.0, 2.0, -10.0));
    let mut list = list_of(vec![floor, cube]);
    list.camera = Camera {
        position: Vec3::new(0.0, 7.0, 2.0),
        look_at: Vec3::new(0.0, 0.0, -10.0),
        fov_y_degrees: 70.0,
    };
    list.sun.intensity = 0.0;
    list.ambient.intensity = 0.6;
    list.shadows.enabled = false;
    list
}

const PROBE_SIZE: u32 = 128;

/// The colors `points` are drawn in, in a frame of `list` drawn with
/// ambient occlusion at `mode`.
fn probe(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    list: &DrawList,
    mode: crate::AmbientOcclusion,
    points: &[Vec3],
) -> Vec<[u8; 3]> {
    let mut renderer = TextureRenderer::new(device, queue, PROBE_SIZE, PROBE_SIZE);
    renderer.set_graphics(&with_occlusion(mode));
    renderer.render(list).unwrap();
    let pixels = renderer.read_pixels();
    points
        .iter()
        .map(|point| {
            let clip = view_projection(list, 1.0) * point.extend(1.0);
            let ndc = clip.truncate() / clip.w;
            let x = ((ndc.x * 0.5 + 0.5) * PROBE_SIZE as f32) as usize;
            let y = ((0.5 - ndc.y * 0.5) * PROBE_SIZE as f32) as usize;
            let at = (y * PROBE_SIZE as usize + x) * 4;
            [pixels[at], pixels[at + 1], pixels[at + 2]]
        })
        .collect()
}

fn within(a: [u8; 3], b: [u8; 3], levels: u8) -> bool {
    a.iter().zip(&b).all(|(a, b)| a.abs_diff(*b) <= levels)
}

/// Every graphics setting through every value, one at a time from High,
/// then back to High: the settings the resource test switches through.
fn every_setting_and_value() -> Vec<GraphicsSettings> {
    use crate::{AmbientOcclusion, AntiAliasing, Quality, ReflectionQuality, TextureQuality};
    let high = GraphicsSettings::default();
    let mut all: Vec<GraphicsSettings> = Quality::ALL
        .map(GraphicsSettings::preset)
        .into_iter()
        .collect();
    let with = |change: &dyn Fn(&mut GraphicsSettings)| {
        let mut settings = high.clone();
        change(&mut settings);
        settings
    };
    for quality in ShadowQuality::ALL {
        all.push(with(&|s| s.shadow_quality = Some(quality)));
    }
    for distance in [0.0, 20.0, 500.0] {
        all.push(with(&|s| s.shadow_distance = Some(distance)));
    }
    for on in [false, true] {
        all.push(with(&|s| s.post_processing = Some(on)));
    }
    for scale in [0.5, 0.75, 1.5, 2.0] {
        all.push(with(&|s| s.render_scale = Some(scale)));
    }
    for quality in TextureQuality::ALL {
        all.push(with(&|s| s.texture_quality = Some(quality)));
    }
    for mode in AntiAliasing::ALL {
        all.push(with(&|s| s.anti_aliasing = Some(mode)));
    }
    for mode in AmbientOcclusion::ALL {
        all.push(with(&|s| s.ambient_occlusion = Some(mode)));
    }
    for quality in ReflectionQuality::ALL {
        all.push(with(&|s| s.reflection_quality = Some(quality)));
    }
    for on in [false, true] {
        all.push(with(&|s| s.contact_shadows = Some(on)));
    }
    for on in [true, false] {
        all.push(with(&|s| s.realtime_gi = Some(on)));
    }
    // Volumetric fog alone still draws the prepass, for its depth.
    for on in [true, false] {
        all.push(with(&|s| {
            s.ambient_occlusion = Some(AmbientOcclusion::Off);
            s.contact_shadows = Some(false);
            s.volumetric_fog = Some(on);
        }));
    }
    // Contact shadows alone still draw the prepass.
    all.push(with(&|s| {
        s.ambient_occlusion = Some(AmbientOcclusion::Off);
        s.contact_shadows = Some(true);
    }));
    // So does depth of field, with its own target.
    for quality in crate::DepthOfFieldQuality::ALL {
        all.push(with(&|s| {
            s.ambient_occlusion = Some(AmbientOcclusion::Off);
            s.contact_shadows = Some(false);
            s.depth_of_field = Some(quality);
        }));
    }
    for on in [false, true] {
        all.push(with(&|s| {
            s.vignette = Some(on);
            s.film_grain = Some(on);
            s.chromatic_aberration = Some(on);
            s.sharpen = Some(on);
        }));
    }
    // TAA and motion blur alone still draw the prepass, with the motion
    // targets.
    for quality in crate::MotionBlurQuality::ALL {
        all.push(with(&|s| {
            s.ambient_occlusion = Some(AmbientOcclusion::Off);
            s.contact_shadows = Some(false);
            s.depth_of_field = Some(crate::DepthOfFieldQuality::Off);
            s.motion_blur = Some(quality);
        }));
    }
    all.push(with(&|s| {
        s.ambient_occlusion = Some(AmbientOcclusion::Off);
        s.contact_shadows = Some(false);
        s.depth_of_field = Some(crate::DepthOfFieldQuality::Off);
        s.motion_blur = Some(crate::MotionBlurQuality::Off);
        s.anti_aliasing = Some(AntiAliasing::Taa);
    }));
    all.push(high);
    all
}

#[test]
fn switching_every_setting_and_dropping_renderers_leaves_no_resources_behind() {
    // The instance is kept, so its report can count what's still alive.
    let instance = crate::adapters::instance();
    let Ok(adapter) = pollster::block_on(crate::adapters::adapter(&instance, None)) else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    let Ok((device, queue)) = pollster::block_on(adapter.request_device(&Default::default()))
    else {
        return;
    };
    // The GPU objects the program holds, of each kind that could leak.
    let held = || {
        device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
        let report = instance.generate_report().expect("a native backend");
        let hub = report.hub_report();
        [
            ("textures", hub.textures.num_kept_from_user),
            ("texture views", hub.texture_views.num_kept_from_user),
            ("buffers", hub.buffers.num_kept_from_user),
            ("bind groups", hub.bind_groups.num_kept_from_user),
            ("pipelines", hub.render_pipelines.num_kept_from_user),
            (
                "compute pipelines",
                hub.compute_pipelines.num_kept_from_user,
            ),
            ("samplers", hub.samplers.num_kept_from_user),
            ("query sets", hub.query_sets.num_kept_from_user),
        ]
    };
    let before = held();

    let (mut list, _) = one_of_everything();
    list.shadows = Shadows {
        enabled: true,
        ..Shadows::default()
    };
    list.lights.push(spot_light(
        Vec3::new(0.0, 6.0, -8.0),
        Vec3::NEG_Y,
        20.0,
        true,
    ));
    list.lights
        .push(shadowing_point_light(Vec3::new(2.0, 4.0, -8.0), 20.0));
    // Fog for volumetric fog to light.
    list.fog = crate::Fog {
        color: None,
        density: 0.01,
        sun_scattering: 0.8,
        forward_scattering: 0.6,
        ..crate::Fog::default()
    };
    // A baked volume, for real-time GI to trace.
    list.light_probe_volumes
        .push(crate::draw_list::LightProbeVolume {
            center: Vec3::new(0.0, 2.0, -8.0),
            rotation: glam::Quat::IDENTITY,
            half_size: Vec3::splat(4.0),
            blend_distance: 1.0,
            probes: crate::draw_list::LightProbeGrid {
                key: 1,
                counts: [2, 1, 2],
                probes: Arc::new(vec![[[0.5; 3]; 4]; 4]),
            },
        });
    // Every lens effect, for their settings to turn on and off.
    list.lens = crate::LensEffects {
        depth_of_field: Some(crate::DepthOfField {
            focus_distance: 4.0,
            in_focus_radius: 1.0,
            near_intensity: 1.0,
            far_intensity: 1.0,
        }),
        vignette: Some(crate::Vignette {
            intensity: 0.5,
            size: 0.5,
            color: [0.0; 3],
        }),
        film_grain: Some(crate::FilmGrain {
            intensity: 0.2,
            size: 1.0,
        }),
        chromatic_aberration: Some(0.5),
        sharpen: Some(0.5),
        motion_blur: Some(0.5),
    };
    let settings = every_setting_and_value();
    // What a renderer holds: its caches and its targets.
    let counts = |renderer: &TextureRenderer| {
        let scene = &renderer.scene;
        let stats = renderer.stats();
        (
            (
                scene.textures.len(),
                scene.materials.len(),
                scene.custom_meshes.len(),
            ),
            (stats.texture_bytes, stats.mesh_bytes),
            (
                scene.shadow_maps_alive(),
                (
                    scene.spot_shadow_maps_alive(),
                    scene.point_shadow_maps_alive(),
                ),
                scene.targets_alive(),
                scene.occlusion_targets_alive(),
                scene.anti_aliasing_targets_alive(),
            ),
            scene.environment.bytes(),
            scene.probes_traced().is_some(),
            (
                scene.volumetric_fogs_alive(),
                scene.depth_of_field_targets_alive(),
                scene.temporal_targets_alive(),
            ),
        )
    };
    for round in 0..3 {
        let mut renderer = TextureRenderer::new(&device, &queue, 96, 64);
        renderer.render(&list).unwrap();
        let start = counts(&renderer);
        // The report does see a renderer's objects.
        let alive = held();
        assert!(alive.iter().zip(&before).all(|(now, then)| now.1 >= then.1));
        assert!(
            alive[0].1 > before[0].1 && alive[3].1 > before[3].1,
            "{alive:?}"
        );
        for (step, settings) in settings.iter().enumerate() {
            renderer.set_graphics(settings);
            renderer.render(&list).unwrap();
            let used = renderer.graphics().clone();
            let (
                _,
                _,
                (sun, (spot, point), targets, occlusion, (msaa, fxaa)),
                environment,
                gi,
                (fogs, depth_of_field, (motion, taa, motion_blur)),
            ) = counts(&renderer);
            let context = format!("round {round}, step {step}: {settings:?}");
            // A ceiling of 0 studs draws no sun shadows, so needs no map.
            let sun_drawn = used.shadow_quality.map().is_some() && used.shadow_distance > 0.0;
            assert_eq!(sun, usize::from(sun_drawn), "{context}");
            assert_eq!(
                spot,
                usize::from(used.shadow_quality.spot_map().is_some()),
                "{context}"
            );
            assert_eq!(
                point,
                usize::from(used.shadow_quality.point_map().is_some()),
                "{context}"
            );
            assert_eq!(targets, 1, "{context}");
            // The prepass is drawn for ambient occlusion, contact shadows,
            // volumetric fog or depth of field.
            let blurred = used.depth_of_field != crate::DepthOfFieldQuality::Off;
            assert_eq!(depth_of_field, usize::from(blurred), "{context}");
            // TAA and motion blur keep the motion targets, each with its own.
            let jittered = used.anti_aliasing == crate::AntiAliasing::Taa;
            let streaked = used.motion_blur != crate::MotionBlurQuality::Off;
            assert_eq!(taa, usize::from(jittered), "{context}");
            assert_eq!(motion_blur, usize::from(streaked), "{context}");
            assert_eq!(motion, usize::from(jittered || streaked), "{context}");
            let prepass = used.ambient_occlusion != crate::AmbientOcclusion::Off
                || used.contact_shadows
                || used.volumetric_fog
                || blurred
                || jittered
                || streaked;
            assert_eq!(occlusion, usize::from(prepass), "{context}");
            assert_eq!(
                msaa,
                usize::from(used.anti_aliasing.samples() > 1),
                "{context}"
            );
            assert_eq!(
                fxaa,
                usize::from(used.anti_aliasing == crate::AntiAliasing::Fxaa),
                "{context}"
            );
            // The environment maps exist only above Low reflections.
            assert_eq!(
                environment > 0,
                used.reflection_quality.map().is_some(),
                "{context}"
            );
            assert_eq!(gi, used.realtime_gi, "{context}");
            assert_eq!(fogs, usize::from(used.volumetric_fog), "{context}");
        }
        // Back on High, as it started.
        assert_eq!(counts(&renderer), start, "round {round}");
        drop(renderer);
        assert_eq!(
            held(),
            before,
            "round {round}: a dropped renderer left GPU objects"
        );
    }
}
