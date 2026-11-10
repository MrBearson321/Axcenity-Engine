//! Reading the Workspace's models and textures before the game shows
//! (roadmap I11).
//!
//! [`Preload::start`] lists every file the first drawing reads
//! (`scene::asset_files`) and requests them from the asset workers
//! (`Assets::request_model` and friends) while the player opens its window
//! and starts its scripts, so a 4K texture set or a big FBX never stalls a
//! frame. The splash, the game's own loading screen, or a plain progress
//! screen stays up until everything is in, so the world doesn't appear with
//! its textures popping in one by one.
//!
//! A file needed later (a level loaded by a script, a model cloned from
//! storage) is still read the first time it's drawn: textures in the
//! background, models in the frame that needs them. A script can load those
//! ahead with `ContentProvider:PreloadAsync`.

use axcenity_assets::{Assets, LoadTicket};
use axcenity_core::World;
use axcenity_render::{TextAlign, UiDrawItem, UiRectangle, UiText, VerticalAlign};
use axcenity_scene::asset_files;

/// The Workspace's files being read ahead of the first frame.
pub struct Preload {
    /// One for each file, or for a material and its textures.
    tickets: Vec<LoadTicket>,
    /// Set once every file is in.
    finished: bool,
}

/// How far a [`Preload`] has got, for its screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Progress {
    pub done: usize,
    pub total: usize,
}

impl Preload {
    /// Starts reading every file drawing `world`'s Workspace reads, and
    /// returns at once. Turns on the asset workers for textures, which stay
    /// on for the rest of the game: a texture first seen later loads in the
    /// background too.
    pub fn start(world: &World, assets: &Assets) -> Self {
        // The player draws every frame, so it needs no wake-up call when a
        // texture is ready.
        assets.load_textures_in_background(|| {});
        let tickets = asset_files(world)
            .iter()
            .map(|file| file.request(assets))
            .collect();
        Self {
            tickets,
            finished: false,
        }
    }

    /// Checks on the files once a frame. True once every file is in (or
    /// has failed); stays true.
    pub fn update(&mut self) -> bool {
        self.finished = self.finished || self.tickets.iter().all(LoadTicket::is_finished);
        self.finished
    }

    /// Files read so far, of how many.
    pub fn progress(&self) -> Progress {
        let total = self.tickets.len();
        let done = if self.finished {
            total
        } else {
            self.tickets
                .iter()
                .filter(|ticket| ticket.is_finished())
                .count()
        };
        Progress { done, total }
    }
}

impl Progress {
    /// Such as "Loading 12 of 40", or "Loading" before anything is counted.
    pub fn caption(&self) -> String {
        if self.total == 0 {
            "Loading".to_owned()
        } else {
            format!("Loading {} of {}", self.done.min(self.total), self.total)
        }
    }

    /// From 0 to 1.
    pub fn fraction(&self) -> f32 {
        if self.total == 0 {
            0.0
        } else {
            (self.done as f32 / self.total as f32).clamp(0.0, 1.0)
        }
    }
}

const BACKDROP: [f32; 4] = [0.027, 0.027, 0.031, 1.0];
const TRACK: [f32; 4] = [0.22, 0.22, 0.23, 1.0];
const FILL: [f32; 4] = [0.96, 0.96, 0.96, 1.0];
const CAPTION: [f32; 4] = [0.72, 0.72, 0.74, 1.0];
const BAR_HEIGHT: f32 = 4.0;

/// A plain loading screen, for a game with no splash and no loading screen
/// of its own: a dark backdrop, a bar filled by `progress`, and its count.
pub fn screen_items(width: f32, height: f32, progress: Progress) -> Vec<UiDrawItem> {
    let bar_w = (width * 0.3).clamp(160.0, 360.0);
    let bar_x = (width - bar_w) * 0.5;
    let bar_y = height * 0.5;
    let rectangle = |position, size, color| {
        UiDrawItem::Rectangle(UiRectangle {
            position,
            size,
            color,
            color_end: color,
            border_color: [0.0; 4],
            border_width: 0.0,
            border_radius: BAR_HEIGHT * 0.5,
            ..Default::default()
        })
    };
    let mut items = vec![
        UiDrawItem::Rectangle(UiRectangle {
            position: [0.0, 0.0],
            size: [width, height],
            color: BACKDROP,
            color_end: BACKDROP,
            ..Default::default()
        }),
        rectangle([bar_x, bar_y], [bar_w, BAR_HEIGHT], TRACK),
    ];
    let fill = progress.fraction() * bar_w;
    if fill > 0.5 {
        items.push(rectangle([bar_x, bar_y], [fill, BAR_HEIGHT], FILL));
    }
    items.push(UiDrawItem::Text(UiText {
        position: [bar_x, bar_y + 16.0],
        size: [bar_w, 22.0],
        text: progress.caption(),
        font_size: 13.0,
        font_family: "Montserrat".to_owned(),
        color: CAPTION,
        align: TextAlign::Center,
        vertical_align: VerticalAlign::Middle,
        ..UiText::default()
    }));
    items
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use axcenity_core::{ClassName, Value};

    use super::*;

    /// A 2×2 white PNG.
    fn png() -> Vec<u8> {
        let mut bytes = Vec::new();
        let mut encoder = png::Encoder::new(&mut bytes, 2, 2);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().unwrap();
        writer.write_image_data(&[255; 16]).unwrap();
        writer.finish().unwrap();
        bytes
    }

    fn world_using(maps: &[&str], mesh_id: &str) -> World {
        let mut world = World::new();
        let part = world.create(ClassName::MeshPart).unwrap();
        world
            .set_property(part, "MeshId", Value::String(mesh_id.into()))
            .unwrap();
        world.set_parent(part, Some(world.workspace())).unwrap();
        let look = world.create(ClassName::SurfaceAppearance).unwrap();
        for (property, path) in ["ColorMap", "NormalMap"].iter().zip(maps) {
            world
                .set_property(look, property, Value::String((*path).into()))
                .unwrap();
        }
        world.set_parent(look, Some(part)).unwrap();
        world
    }

    #[test]
    fn the_workspace_assets_load_ahead_and_textures_stay_in_the_background() {
        let folder = std::env::temp_dir().join(format!(
            "axcenity-player-preload-test-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(folder.join("assets/textures")).unwrap();
        std::fs::write(folder.join("assets/textures/a.png"), png()).unwrap();
        std::fs::write(folder.join("assets/textures/b.png"), png()).unwrap();
        let world = world_using(
            &["assets/textures/a.png", "assets/textures/b.png"],
            "assets/models/missing.glb",
        );
        let assets = Assets::new(&folder);
        let mut preload = Preload::start(&world, &assets);
        assert!(assets.loads_textures_in_background());

        let started = Instant::now();
        while !preload.update() {
            assert!(started.elapsed() < Duration::from_secs(10), "never done");
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(preload.update(), "done stays done");
        let progress = preload.progress();
        assert_eq!(progress.done, progress.total);
        assert!(progress.total >= 1, "the model counts: {progress:?}");
        // Read by the preload: drawing takes them without reading again.
        assert!(assets.texture("assets/textures/a.png").is_some());
        assert!(assets.texture("assets/textures/b.png").is_some());
        assert!(assets.texture_progress().is_none());
        // A model that isn't there fails once, and is reported.
        assert!(assets.model_blocking("assets/models/missing.glb").is_none());
        let errors = assets.take_errors();
        assert!(
            errors.iter().any(|error| error.contains("missing.glb")),
            "{errors:?}"
        );
        // A texture first seen later loads in the background too.
        std::fs::write(folder.join("assets/textures/c.png"), png()).unwrap();
        assert!(assets.texture("assets/textures/c.png").is_none());
        let started = Instant::now();
        while assets.texture("assets/textures/c.png").is_none() {
            assert!(started.elapsed() < Duration::from_secs(10), "never read");
            std::thread::sleep(Duration::from_millis(5));
        }
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn an_empty_workspace_is_done_at_once() {
        let assets = Assets::new(std::env::temp_dir());
        let mut preload = Preload::start(&World::new(), &assets);
        let started = Instant::now();
        while !preload.update() {
            assert!(started.elapsed() < Duration::from_secs(10), "never done");
            std::thread::sleep(Duration::from_millis(1));
        }
        assert_eq!(preload.progress(), Progress { done: 0, total: 0 });
    }

    #[test]
    fn the_progress_caption_and_bar_say_how_many_are_in() {
        let half = Progress { done: 3, total: 6 };
        assert_eq!(half.caption(), "Loading 3 of 6");
        assert_eq!(half.fraction(), 0.5);
        assert_eq!(Progress { done: 0, total: 0 }.caption(), "Loading");
        assert_eq!(Progress { done: 9, total: 6 }.fraction(), 1.0);

        let items = screen_items(1280.0, 720.0, half);
        let Some(UiDrawItem::Rectangle(backdrop)) = items.first() else {
            panic!("the backdrop comes first");
        };
        assert_eq!(backdrop.size, [1280.0, 720.0]);
        assert_eq!(backdrop.color[3], 1.0, "the world doesn't show through");
        let fills: Vec<f32> = items
            .iter()
            .filter_map(|item| match item {
                UiDrawItem::Rectangle(rect) if rect.size[1] == BAR_HEIGHT => Some(rect.size[0]),
                _ => None,
            })
            .collect();
        assert_eq!(fills.len(), 2, "the track and its fill");
        assert!((fills[1] - fills[0] * 0.5).abs() < 0.01);
        assert!(
            items.iter().any(
                |item| matches!(item, UiDrawItem::Text(text) if text.text == "Loading 3 of 6")
            )
        );
    }
}
