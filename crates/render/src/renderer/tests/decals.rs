//! Decals: a projected decal lies on curved surfaces with no gaps and no
//! fighting for depth, faces the right way, can leave characters bare,
//! follows the quality preset (none at Low, and no prepass kept for it),
//! and a Part's face decal shows over its face even far away.

use super::*;
use crate::graphics::{AmbientOcclusion, Quality};
use crate::{Decal, DecalRole, GraphicsSettings, Shading, decal_quad};

const SIZE: u32 = 128;

/// High, with nothing else that keeps the prepass alive.
fn settings(quality: Quality) -> GraphicsSettings {
    GraphicsSettings {
        ambient_occlusion: Some(AmbientOcclusion::Off),
        contact_shadows: Some(false),
        volumetric_fog: Some(false),
        ..GraphicsSettings::preset(quality)
    }
}

fn renderer(device: &wgpu::Device, queue: &wgpu::Queue, quality: Quality) -> TextureRenderer {
    let mut renderer = TextureRenderer::new(device, queue, SIZE, SIZE);
    renderer.set_graphics(&settings(quality));
    renderer
}

fn pixel(pixels: &[u8], x: u32, y: u32) -> [u8; 3] {
    pixels[((y * SIZE + x) * 4) as usize..][..3]
        .try_into()
        .unwrap()
}

/// An unlit (so evenly colored) item of `color`.
fn unlit(shape: Shape, color: [f32; 3], transform: Mat4) -> DrawItem {
    DrawItem {
        shape,
        transform,
        material: Material {
            base_color: color,
            shading: Shading::Unlit,
            ..Material::default()
        },
        ..big_block()
    }
}

/// A list lit from straight above by a white sun and white ambient light,
/// so a white decal on a dark surface is bright wherever it lies.
fn lit_list(items: Vec<DrawItem>, eye: Vec3, look_at: Vec3) -> DrawList {
    let mut list = list_of(items);
    list.camera = Camera {
        position: eye,
        look_at,
        fov_y_degrees: 70.0,
    };
    list.ambient.intensity = 1.0;
    list
}

/// A white decal `size` studs across, at `center`, facing up (its picture
/// lies flat, with its top toward +Z).
fn facing_up(center: Vec3, size: Vec3) -> Decal {
    Decal {
        transform: Mat4::from_scale_rotation_translation(
            size,
            glam::Quat::from_rotation_x(std::f32::consts::FRAC_PI_2),
            center,
        ),
        texture: None,
        color: [1.0; 3],
        opacity: 1.0,
        fade_angle: 90.0,
        ignore_characters: true,
    }
}

/// The big dark ball's radius and middle: its top is at the origin.
const BALL_RADIUS: f32 = 20.0;
const BALL_CENTER: Vec3 = Vec3::new(0.0, -20.0, 0.0);

/// Where the camera's ray through pixel (x, y)'s middle first meets the
/// ball, if it does.
fn ball_hit(camera: &Camera, x: u32, y: u32) -> Option<Vec3> {
    let forward = (camera.look_at - camera.position).normalize();
    let right = forward.cross(Vec3::Y).normalize();
    let up = right.cross(forward);
    let half = (camera.fov_y_degrees.to_radians() * 0.5).tan();
    let ndc_x = (x as f32 + 0.5) / SIZE as f32 * 2.0 - 1.0;
    let ndc_y = 1.0 - (y as f32 + 0.5) / SIZE as f32 * 2.0;
    let dir = (forward + right * ndc_x * half + up * ndc_y * half).normalize();
    let from_center = camera.position - BALL_CENTER;
    let b = from_center.dot(dir);
    let c = from_center.length_squared() - BALL_RADIUS * BALL_RADIUS;
    let discriminant = b * b - c;
    (discriminant >= 0.0).then(|| camera.position + dir * (-b - discriminant.sqrt()))
}

/// A road marking 2 studs wide and 16 long down a ball's curved top: every
/// pixel of the ball inside the strip shows the marking and every one
/// clearly outside shows the ball, from a camera moved a little between
/// frames, so there are no gaps and nothing fights for depth.
#[test]
fn a_road_marking_lies_on_a_curved_surface_with_no_gaps_or_fighting() {
    let Some((device, queue)) = test_device() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    let ball = unlit(
        Shape::Sphere,
        [0.1, 0.1, 0.1],
        Mat4::from_scale_rotation_translation(
            Vec3::splat(BALL_RADIUS * 2.0),
            glam::Quat::IDENTITY,
            BALL_CENTER,
        ),
    );
    for quality in [Quality::Medium, Quality::High] {
        let mut renderer = renderer(&device, &queue, quality);
        for step in 0..3 {
            let eye = Vec3::new(0.3 * step as f32, 14.0, 14.0 - 0.2 * step as f32);
            let mut list = lit_list(vec![ball.clone()], eye, Vec3::new(0.0, -2.0, 0.0));
            list.decals = vec![facing_up(Vec3::ZERO, Vec3::new(2.0, 16.0, 12.0))];
            renderer.render(&list).unwrap();
            let pixels = renderer.read_pixels();
            assert_eq!(renderer.stats().decals_drawn, 1);
            let (mut inside, mut outside) = (0, 0);
            // One pixel at most 0.25 studs across, nearby: keep half a
            // stud clear of the strip's edges, and of the ball's outline.
            for y in 0..SIZE {
                for x in 0..SIZE {
                    let Some(hit) = ball_hit(&list.camera, x, y) else {
                        continue;
                    };
                    let seen = pixel(&pixels, x, y);
                    let neighbors_on_ball = [(x + 1, y), (x.wrapping_sub(1), y), (x, y + 1)]
                        .iter()
                        .all(|&(nx, ny)| {
                            nx < SIZE && ny < SIZE && ball_hit(&list.camera, nx, ny).is_some()
                        });
                    if !neighbors_on_ball {
                        continue;
                    }
                    if hit.x.abs() < 0.5 && hit.z.abs() < 7.5 {
                        inside += 1;
                        assert!(
                            seen.iter().all(|&c| c > 150),
                            "{quality:?} step {step}: a gap at ({x}, {y}), {hit}: {seen:?}"
                        );
                    } else if hit.x.abs() > 1.5 || hit.z.abs() > 8.5 {
                        outside += 1;
                        assert!(
                            seen.iter().all(|&c| c < 100),
                            "{quality:?} step {step}: marked off the strip at ({x}, {y}), \
                             {hit}: {seen:?}"
                        );
                    }
                }
            }
            assert!(inside > 200 && outside > 2000, "{inside} in, {outside} out");
        }
    }
}

/// 16 × 4 pixels: red on the left half, see-through on the right (big
/// enough that Low's quarter-size textures still keep the halves apart).
struct HalfRed(Vec<u8>);

impl HalfRed {
    fn new() -> Self {
        let mut rgba = Vec::new();
        for _ in 0..4 {
            for x in 0..16 {
                rgba.extend(if x < 8 { [255, 0, 0, 255] } else { [0; 4] });
            }
        }
        Self(rgba)
    }
}

impl TextureSource for HalfRed {
    fn key(&self) -> u64 {
        0xdeca1
    }
    fn size(&self) -> (u32, u32) {
        (16, 4)
    }
    fn rgba(&self) -> &[u8] {
        &self.0
    }
}

/// A decal's picture is seen as drawn (not mirrored) from the way it
/// faces, and see-through where the picture is.
#[test]
fn a_decal_faces_its_look_vector_unmirrored_and_shows_its_pictures_alpha() {
    let Some((device, queue)) = test_device() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    let floor = unlit(
        Shape::Block,
        [0.1, 0.1, 0.1],
        Mat4::from_scale_rotation_translation(
            Vec3::new(40.0, 1.0, 40.0),
            glam::Quat::IDENTITY,
            Vec3::new(0.0, -0.5, 0.0),
        ),
    );
    // From above, with +Z (the picture's top) up the screen, so +X is on
    // the left.
    let mut list = lit_list(vec![floor], Vec3::new(0.0, 12.0, -0.01), Vec3::ZERO);
    let mut decal = facing_up(Vec3::ZERO, Vec3::new(8.0, 8.0, 2.0));
    decal.texture = Some(Arc::new(HalfRed::new()));
    list.decals = vec![decal];
    let mut renderer = renderer(&device, &queue, Quality::High);
    renderer.render(&list).unwrap();
    let pixels = renderer.read_pixels();
    let middle = SIZE / 2;
    let left = pixel(&pixels, middle - 16, middle);
    let right = pixel(&pixels, middle + 16, middle);
    assert!(left[0] > 150 && left[1] < 60, "red on the left: {left:?}");
    assert!(
        right.iter().all(|&c| c < 60),
        "the floor on the right: {right:?}"
    );
}

/// A decal that ignores characters leaves their items bare and marks the
/// ground under them; one that doesn't marks both.
#[test]
fn ignore_characters_leaves_characters_bare() {
    let Some((device, queue)) = test_device() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    let ground = unlit(
        Shape::Block,
        [0.1, 0.1, 0.1],
        Mat4::from_scale_rotation_translation(
            Vec3::new(40.0, 1.0, 40.0),
            glam::Quat::IDENTITY,
            Vec3::new(0.0, -0.5, 0.0),
        ),
    );
    let mut character = unlit(
        Shape::Block,
        [0.1, 0.1, 0.1],
        Mat4::from_scale_rotation_translation(
            Vec3::new(2.0, 1.0, 2.0),
            glam::Quat::IDENTITY,
            Vec3::new(2.0, 0.5, 0.0),
        ),
    );
    character.decal_role = DecalRole::Character;
    let mut list = lit_list(
        vec![ground, character],
        Vec3::new(0.0, 12.0, -0.01),
        Vec3::ZERO,
    );
    let mut renderer = renderer(&device, &queue, Quality::High);
    let middle = SIZE / 2;
    // Seen from above with +X on the left: the character's top is on the
    // left, bare ground on the right.
    let (on_character, on_ground) = ((middle - 20, middle), (middle + 20, middle));
    for ignore_characters in [true, false] {
        let mut decal = facing_up(Vec3::ZERO, Vec3::new(10.0, 10.0, 6.0));
        decal.ignore_characters = ignore_characters;
        list.decals = vec![decal];
        renderer.render(&list).unwrap();
        let pixels = renderer.read_pixels();
        let character = pixel(&pixels, on_character.0, on_character.1);
        let ground = pixel(&pixels, on_ground.0, on_ground.1);
        assert!(ground.iter().all(|&c| c > 150), "{ground:?}");
        assert_eq!(
            character.iter().all(|&c| c < 60),
            ignore_characters,
            "ignoring characters: {ignore_characters}, {character:?}"
        );
    }
}

/// At Low no projected decal is drawn and the prepass isn't kept for them;
/// at Medium and up they are, and their prepass goes when they do.
#[test]
fn projected_decals_follow_the_quality_preset_and_are_off_at_low() {
    let Some((device, queue)) = test_device() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    let floor = unlit(
        Shape::Block,
        [0.1, 0.1, 0.1],
        Mat4::from_scale_rotation_translation(
            Vec3::new(40.0, 1.0, 40.0),
            glam::Quat::IDENTITY,
            Vec3::new(0.0, -0.5, 0.0),
        ),
    );
    let mut list = lit_list(vec![floor], Vec3::new(0.0, 12.0, 0.01), Vec3::ZERO);
    list.decals = vec![facing_up(Vec3::ZERO, Vec3::new(8.0, 8.0, 2.0))];
    let middle = SIZE / 2;
    for quality in Quality::ALL {
        let mut renderer = renderer(&device, &queue, quality);
        renderer.render(&list).unwrap();
        let pixels = renderer.read_pixels();
        let seen = pixel(&pixels, middle, middle);
        let drawn = quality != Quality::Low;
        assert_eq!(
            seen.iter().all(|&c| c > 150),
            drawn,
            "{quality:?}: {seen:?}"
        );
        let stats = renderer.stats();
        assert_eq!(stats.decals_drawn, u32::from(drawn), "{quality:?}");
        assert_eq!(stats.decals_left_out, u32::from(!drawn), "{quality:?}");
        if quality == Quality::Low {
            assert_eq!(renderer.scene.occlusion_targets_alive(), 0);
        } else {
            assert_eq!(renderer.scene.occlusion_targets_alive(), 1);
            // Without decals the prepass isn't kept for them.
            let mut bare = list.clone();
            bare.decals.clear();
            renderer.render(&bare).unwrap();
            assert_eq!(renderer.scene.occlusion_targets_alive(), 0, "{quality:?}");
        }
    }
}

/// A Part's face decal, lying exactly on its face, shows over it near and
/// far (it never fights the face for depth), at every quality, and is
/// see-through where its picture is.
#[test]
fn a_face_decal_shows_over_its_face_near_and_far_at_every_quality() {
    let Some((device, queue)) = test_device() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    for distance in [6.0, 60.0, 600.0] {
        let size = distance * 0.4;
        // A wall facing the camera (+Z), its front face at z = -distance.
        let wall = unlit(
            Shape::Block,
            [0.1, 0.1, 0.1],
            Mat4::from_scale_rotation_translation(
                Vec3::splat(size),
                glam::Quat::IDENTITY,
                Vec3::new(0.0, 0.0, -distance - size * 0.5),
            ),
        );
        let mut decal = unlit(
            Shape::Mesh(decal_quad()),
            [1.0, 1.0, 1.0],
            Mat4::from_scale_rotation_translation(
                Vec3::new(size, size, 1.0),
                glam::Quat::IDENTITY,
                Vec3::new(0.0, 0.0, -distance),
            ),
        );
        decal.decal_role = DecalRole::Decal;
        decal.casts_shadow = false;
        decal.material.base_color_texture = Some(Arc::new(HalfRed::new()));
        let mut list = list_of(vec![wall, decal]);
        list.camera.look_at = Vec3::new(0.0, 0.0, -1.0);
        for quality in Quality::ALL {
            let mut renderer = renderer(&device, &queue, quality);
            renderer.render(&list).unwrap();
            let pixels = renderer.read_pixels();
            let middle = SIZE / 2;
            // The quad spans about 0.4 / 1.4 of the view's height: a third
            // of it in from the middle stays well inside each half.
            let reach = (SIZE as f32 * 0.4 / 1.4 / 4.0) as u32;
            let left = pixel(&pixels, middle - reach, middle);
            let right = pixel(&pixels, middle + reach, middle);
            assert!(
                left[0] > 200 && left[1] < 40,
                "{distance} studs, {quality:?}: the decal's red: {left:?}"
            );
            assert!(
                right.iter().all(|&c| c < 50),
                "{distance} studs, {quality:?}: the wall through it: {right:?}"
            );
        }
    }
}

/// A decal box the camera stands inside still marks what's inside it (its
/// back faces are drawn, with no depth test).
#[test]
fn a_decal_shows_with_the_camera_inside_its_box() {
    let Some((device, queue)) = test_device() else {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    };
    let floor = unlit(
        Shape::Block,
        [0.1, 0.1, 0.1],
        Mat4::from_scale_rotation_translation(
            Vec3::new(40.0, 1.0, 40.0),
            glam::Quat::IDENTITY,
            Vec3::new(0.0, -0.5, 0.0),
        ),
    );
    let mut list = lit_list(vec![floor], Vec3::new(0.0, 12.0, -0.01), Vec3::ZERO);
    list.decals = vec![facing_up(Vec3::ZERO, Vec3::new(30.0, 30.0, 40.0))];
    let mut renderer = renderer(&device, &queue, Quality::High);
    renderer.render(&list).unwrap();
    let pixels = renderer.read_pixels();
    for (x, y) in [(SIZE / 2, SIZE / 2), (10, 10), (SIZE - 10, SIZE - 10)] {
        let seen = pixel(&pixels, x, y);
        assert!(seen.iter().all(|&c| c > 150), "({x}, {y}): {seen:?}");
    }
}
