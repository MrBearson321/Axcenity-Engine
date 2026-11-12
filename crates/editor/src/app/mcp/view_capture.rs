//! `view_capture`: a picture of the scene for models that can see images.
//! It's drawn offscreen with the engine's own renderer (the draw list the
//! player builds, without the editor's gizmos or selection outlines), from
//! the editor's camera, framing an object, or from a point the model
//! gives. The world shown is the viewport's: the game's during Play. It
//! needs a graphics adapter: the window's computer has one; a server
//! without a display may not (`docs/editor/mcp.md`).

use std::sync::OnceLock;

use axcenity_core::{InstanceId, Value, World};
use axcenity_mcp::{Args, Class, Code, Kind, Outcome, Param, Profile, ToolError, ToolSpec};
use axcenity_render::glam::Vec3;
use axcenity_render::wgpu;

use super::Host;
use super::refs::{label, resolve};

pub const SPEC: ToolSpec = ToolSpec {
    name: "view_capture",
    title: "Capture the view",
    summary: "A PNG of the scene from the editor's camera, framing an object, or from a point; costs image tokens.",
    profile: Profile::Standard,
    class: Class::Read,
    idempotent: true,
    params: &[
        Param::optional("id", Kind::String, "object to frame"),
        Param::optional("from", Kind::Vector3, "camera position"),
        Param::optional("at", Kind::Vector3, "point to look at"),
        Param::optional(
            "size",
            Kind::Enum(&["small", "medium", "large"]),
            "320x180, 640x360 (default) or 1024x576",
        ),
    ],
};

/// The vertical field of view pictures are drawn with, as the viewport's.
const FOV_Y_DEGREES: f32 = 70.0;

pub fn call(host: &mut Host, args: &Args) -> Outcome {
    let (width, height) = match args.str("size") {
        Some("small") => (320, 180),
        Some("large") => (1024, 576),
        _ => (640, 360),
    };
    let app = &*host.app;
    let world = super::super::shown(&app.world, &app.play);
    let from = args.vector3("from").map(vec3);
    let at = args.vector3("at").map(vec3);
    let (camera, seen) = match (args.str("id"), from, at) {
        (Some(_), Some(_), _) => {
            return Err(ToolError::bad_args(
                "give id (to frame an object) or from (a camera position), not both",
            ));
        }
        (Some(text), None, at) => {
            let id = resolve(world, text)?;
            let Some((center, radius)) = extent(world, id) else {
                return Err(ToolError::new(
                    Code::BadValue,
                    format!("{} has no parts to frame", label(world, id)),
                )
                .with_fix("give a part, a model or a folder of parts, or from and at"));
            };
            let look = at.unwrap_or(center);
            let distance = radius / (FOV_Y_DEGREES.to_radians() / 2.0).tan() * 1.3;
            let position = look + Vec3::new(1.0, 0.8, 1.0).normalize() * distance.max(4.0);
            (
                camera(position, look),
                format!("framing {}", label(world, id)),
            )
        }
        (None, Some(position), at) => {
            let look = at.unwrap_or(app.viewport.focus());
            if (look - position).length() < 0.01 {
                return Err(ToolError::bad_args("from and at are the same point"));
            }
            (camera(position, look), "from the given point".to_owned())
        }
        (None, None, Some(_)) => {
            return Err(ToolError::bad_args("at goes with id or from")
                .with_fix("give from too, or leave both out for the editor's camera"));
        }
        (None, None, None) => (
            app.viewport.render_camera(),
            "from the editor's camera".to_owned(),
        ),
    };
    let framed = args.str("id").is_some() || from.is_some();
    let Some((device, queue)) = gpu() else {
        return Err(ToolError::new(
            Code::Unsupported,
            "there's no graphics adapter to draw with on this computer",
        )
        .with_fix("run the editor with a GPU, or use scene_inspect and object_get instead"));
    };
    let mut renderer = axcenity_render::TextureRenderer::new(device, queue, width, height);
    // Models load the first time they're drawn, so draw twice.
    for _ in 0..2 {
        let list = axcenity_scene::build_draw_list(world, &app.assets, camera);
        renderer
            .render(&list)
            .map_err(|error| ToolError::internal(format!("could not draw: {error}")))?;
    }
    let png = encode(&renderer.read_pixels(), width, height)
        .map_err(|error| ToolError::internal(format!("could not encode the picture: {error}")))?;
    host.session.png = Some(png);
    Ok(format!(
        "{width}x{height} picture {seen}: camera at {} {} (Y is up)",
        point(camera.position),
        aim(&camera, framed)
    ))
}

fn point(p: Vec3) -> String {
    format!("[{},{},{}]", n(p.x), n(p.y), n(p.z))
}

/// Where the camera looks, in words: the point it was aimed at; for the
/// editor's own camera (whose look-at is just a step ahead of it), where
/// its view meets the ground (Y 0), else which way it looks.
fn aim(camera: &axcenity_render::Camera, framed: bool) -> String {
    if framed {
        return format!("looking at {}", point(camera.look_at));
    }
    let along = (camera.look_at - camera.position).normalize_or_zero();
    if along.y < -0.01 {
        let ground = camera.position + along * (-camera.position.y / along.y);
        if camera.position.y > 0.0 && ground.distance(camera.position) < 10_000.0 {
            return format!("looking at {} on the ground", point(ground));
        }
    }
    let rounded = |x: f32| (x * 100.0).round() / 100.0;
    format!(
        "looking along {}",
        point(Vec3::new(
            rounded(along.x),
            rounded(along.y),
            rounded(along.z)
        ))
    )
}

fn vec3([x, y, z]: [f64; 3]) -> Vec3 {
    Vec3::new(x as f32, y as f32, z as f32)
}

fn n(x: f32) -> String {
    axcenity_mcp::text::number(f64::from(x))
}

fn camera(position: Vec3, look_at: Vec3) -> axcenity_render::Camera {
    axcenity_render::Camera {
        position,
        look_at,
        fov_y_degrees: FOV_Y_DEGREES,
    }
}

/// The middle of `id` and its parts and how far they reach from it (the
/// sphere around their boxes, rotation left out), or `None` when there's no
/// part in it.
fn extent(world: &World, id: InstanceId) -> Option<(Vec3, f32)> {
    let (mut low, mut high) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
    let mut any = false;
    for part in std::iter::once(id).chain(world.descendants_iter(id)) {
        let Some(instance) = world.get(part) else {
            continue;
        };
        let (Some(Value::Vector3(p)), Some(Value::Vector3(s))) =
            (instance.property("Position"), instance.property("Size"))
        else {
            continue;
        };
        let (p, half) = (Vec3::new(p.x, p.y, p.z), Vec3::new(s.x, s.y, s.z) / 2.0);
        low = low.min(p - half);
        high = high.max(p + half);
        any = true;
    }
    any.then(|| ((low + high) / 2.0, ((high - low) / 2.0).length().max(0.5)))
}

/// The device pictures are drawn on, made the first time one is asked for
/// and kept: `None` when the computer has no adapter.
fn gpu() -> Option<&'static (wgpu::Device, wgpu::Queue)> {
    static GPU: OnceLock<Option<(wgpu::Device, wgpu::Queue)>> = OnceLock::new();
    GPU.get_or_init(|| {
        let instance = axcenity_render::adapters::instance();
        let adapter = block_on(axcenity_render::adapters::adapter(&instance, None)).ok()?;
        block_on(adapter.request_device(&Default::default())).ok()
    })
    .as_ref()
}

/// RGBA rows as a PNG file's bytes.
fn encode(pixels: &[u8], width: u32, height: u32) -> Result<Vec<u8>, png::EncodingError> {
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.write_header()?.write_image_data(pixels)?;
    }
    Ok(bytes)
}

/// Runs `future` to its end on this thread. wgpu's native adapter and
/// device requests are ready at once, so this only polls.
fn block_on<F: std::future::Future>(future: F) -> F::Output {
    let mut future = std::pin::pin!(future);
    let mut context = std::task::Context::from_waker(std::task::Waker::noop());
    loop {
        if let std::task::Poll::Ready(value) = future.as_mut().poll(&mut context) {
            return value;
        }
        std::thread::yield_now();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_editors_camera_says_where_its_view_meets_the_ground() {
        // The editor's camera looks a step ahead of itself.
        let down = camera(Vec3::new(12.0, 8.0, 16.0), Vec3::new(11.4, 7.6, 15.2));
        assert_eq!(aim(&down, false), "looking at [0,0,0] on the ground");
        let level = camera(Vec3::new(0.0, 5.0, 0.0), Vec3::new(0.0, 5.0, -1.0));
        assert_eq!(aim(&level, false), "looking along [0,0,-1]");
        // A point the client gave is said as it is.
        let given = camera(Vec3::new(0.0, 20.0, 30.0), Vec3::ZERO);
        assert_eq!(aim(&given, true), "looking at [0,0,0]");
    }
}
