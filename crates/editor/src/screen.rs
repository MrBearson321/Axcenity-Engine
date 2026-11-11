//! The camera as it draws into the viewport's rectangle, for turning between
//! egui positions (points, Y down) and the 3D world.

use axcenity_render::Camera;
use axcenity_render::glam::{Vec2, Vec3};
use axcenity_scene::{CameraView, Ray};
use eframe::egui;

#[derive(Clone, Copy, Debug)]
pub struct Screen {
    view: CameraView,
    rect: egui::Rect,
}

impl Screen {
    pub fn new(camera: Camera, rect: egui::Rect) -> Self {
        // `max` keeps a zero-height (minimized) viewport from dividing by zero.
        let aspect = rect.width() / rect.height().max(1.0);
        Self {
            view: CameraView::new(camera, aspect),
            rect,
        }
    }

    /// The ray from the camera through a position in the viewport.
    pub fn ray(&self, point: egui::Pos2) -> Ray {
        self.view.ray(to_screen_point(self.rect, point))
    }

    /// Where a world point shows in the viewport, unless it's behind the camera.
    pub fn point(&self, world: Vec3) -> Option<egui::Pos2> {
        let screen = self.view.project_point(world)?;
        Some(to_ui_point(self.rect, screen))
    }

    /// Where a world line shows in the viewport, cut off at the camera.
    pub fn line(&self, a: Vec3, b: Vec3) -> Option<[egui::Pos2; 2]> {
        let (a, b) = self.view.project_line(a, b)?;
        Some([to_ui_point(self.rect, a), to_ui_point(self.rect, b)])
    }

    /// How many studs one point covers at `at`'s distance from the camera.
    pub fn studs_per_point(&self, at: Vec3) -> f32 {
        // Half the screen is half the viewport's height in points.
        self.view.studs_per_half_screen(at) / (self.rect.height().max(1.0) / 2.0)
    }
}

/// Turns a position in the viewport (egui points, Y down) into the
/// renderer's screen coordinates (-1 to 1, Y up).
fn to_screen_point(rect: egui::Rect, point: egui::Pos2) -> Vec2 {
    let across = (point - rect.min) / rect.size();
    Vec2::new(across.x * 2.0 - 1.0, 1.0 - across.y * 2.0)
}

/// The reverse of [`to_screen_point`].
fn to_ui_point(rect: egui::Rect, screen: Vec2) -> egui::Pos2 {
    rect.min + egui::vec2((screen.x + 1.0) / 2.0, (1.0 - screen.y) / 2.0) * rect.size()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn viewport_corners_map_to_screen_corners_and_back() {
        let rect = egui::Rect::from_min_size(egui::pos2(100.0, 50.0), egui::vec2(800.0, 600.0));
        assert_eq!(to_screen_point(rect, rect.left_top()), Vec2::new(-1.0, 1.0));
        assert_eq!(
            to_screen_point(rect, rect.right_bottom()),
            Vec2::new(1.0, -1.0)
        );
        assert_eq!(to_screen_point(rect, rect.center()), Vec2::ZERO);
        let point = egui::pos2(250.0, 470.0);
        assert_eq!(to_ui_point(rect, to_screen_point(rect, point)), point);
    }

    #[test]
    fn a_world_point_on_a_ray_shows_where_the_ray_started() {
        let camera = Camera {
            position: Vec3::new(12.0, 8.0, 16.0),
            look_at: Vec3::ZERO,
            fov_y_degrees: 50.0,
        };
        let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1280.0, 720.0));
        let screen = Screen::new(camera, rect);
        let pointer = egui::pos2(300.0, 500.0);
        let shown = screen.point(screen.ray(pointer).at(20.0)).unwrap();
        assert!((shown - pointer).length() < 1e-2, "{shown:?}");
    }

    /// Picking goes through `Screen`, which knows only the viewport's
    /// rectangle, so it must still match what's drawn when the scene is
    /// drawn at half size and scaled up to the viewport.
    #[test]
    fn picking_hits_the_part_drawn_under_the_pointer_at_half_render_scale() {
        use axcenity_render::{GraphicsSettings, TextureRenderer, wgpu};
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let Ok(adapter) = pollster::block_on(instance.request_adapter(&Default::default())) else {
            return;
        };
        let Ok((device, queue)) = pollster::block_on(adapter.request_device(&Default::default()))
        else {
            return;
        };
        let folder =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/hello-graphics");
        let world = axcenity_project::load(&folder).unwrap().world;
        let assets = axcenity_assets::Assets::new(&folder);
        let camera = axcenity_scene::FlyCamera::looking_at(Vec3::new(12.0, 8.0, 16.0), Vec3::ZERO)
            .to_render_camera();
        let (width, height) = (320, 180);
        let draw = |render_scale| {
            let mut renderer = TextureRenderer::new(&device, &queue, width, height);
            renderer.set_graphics(&GraphicsSettings {
                render_scale: Some(render_scale),
                ..GraphicsSettings::default()
            });
            // Models load the first time they're drawn, so draw twice.
            for _ in 0..2 {
                let list = axcenity_scene::build_draw_list(&world, &assets, camera);
                renderer.render(&list).unwrap();
            }
            renderer.read_pixels()
        };
        let (full, half) = (draw(1.0), draw(0.5));
        let difference = |at: usize| -> u64 {
            (0..3)
                .map(|channel| u64::from(full[at + channel].abs_diff(half[at + channel])))
                .sum()
        };

        let rect =
            egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(width as f32, height as f32));
        let screen = Screen::new(camera, rect);
        let picked = |x: i32, y: i32| {
            let pointer = egui::pos2(x as f32 + 0.5, y as f32 + 0.5);
            axcenity_scene::pick(&world, screen.ray(pointer))
        };
        let mut parts = std::collections::HashSet::new();
        let (mut compared, mut total) = (0u64, 0u64);
        for y in (4..height as i32 - 4).step_by(6) {
            for x in (4..width as i32 - 4).step_by(6) {
                let Some(part) = picked(x, y) else {
                    continue;
                };
                // Only well inside a part, away from its edges.
                let inside = [(-3, 0), (3, 0), (0, -3), (0, 3)]
                    .iter()
                    .all(|(dx, dy)| picked(x + dx, y + dy) == Some(part));
                if !inside {
                    continue;
                }
                parts.insert(part);
                compared += 1;
                total += difference((y as usize * width as usize + x as usize) * 4);
            }
        }
        assert!(parts.len() >= 5, "only {} parts picked", parts.len());
        // Where picking finds a part, both frames show that part.
        let average = total as f64 / (compared * 3) as f64;
        assert!(
            average < 6.0,
            "picked points differ by {average:.1} per channel"
        );
        let whole = (0..full.len()).step_by(4).map(difference).sum::<u64>() as f64
            / (u64::from(width * height) * 3) as f64;
        assert!(whole < 6.0, "the frames differ by {whole:.1} per channel");
    }

    #[test]
    fn studs_per_point_matches_what_the_camera_shows() {
        // Looking down -Z from the origin, a point 10 studs away and
        // `studs_per_point · 100` studs up shows 100 points above the middle.
        let camera = Camera {
            position: Vec3::ZERO,
            look_at: Vec3::NEG_Z,
            fov_y_degrees: 50.0,
        };
        let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1280.0, 720.0));
        let screen = Screen::new(camera, rect);
        let ahead = Vec3::new(0.0, 0.0, -10.0);
        let up = Vec3::Y * screen.studs_per_point(ahead) * 100.0;
        let shown = screen.point(ahead + up).unwrap();
        assert!((shown - (rect.center() - egui::vec2(0.0, 100.0))).length() < 1e-2);
    }
}
