//! Watching a replay on screen (Phase 81): a `.replay` file opened with
//! `axcenity-player --replay <file> <game>`, or the buffer a script's
//! `ReplayService:PlayBuffer()` shows over the game (a kill cam).
//!
//! Keys: Space pauses, Left and Right jump 5 seconds, Up and Down double
//! or halve the speed (¼× to 4×), Return goes back to the start, Tab
//! follows the next player through their character's eyes (then the free
//! camera again), and Escape ends a kill cam. The free camera is the
//! player's own fly camera. A timeline along the bottom shows where
//! playback is.

use axcenity_core::{InstanceId, KeyCode, World};
use axcenity_net::{BufferPlay, Playback, Replay, eyes_of};
use axcenity_render::glam::Vec3;
use axcenity_render::{TextAlign, UiDrawItem, UiRectangle, UiText, VerticalAlign};
use axcenity_scene::TerrainCache;

/// How far Left and Right jump, in seconds.
const JUMP: f64 = 5.0;
const SPEEDS: [f64; 5] = [0.25, 0.5, 1.0, 2.0, 4.0];

const TRACK: [f32; 4] = [1.0, 1.0, 1.0, 0.25];
const FILL: [f32; 4] = [0.95, 0.35, 0.3, 0.95];
const PANEL: [f32; 4] = [0.0, 0.0, 0.0, 0.45];
const CAPTION: [f32; 4] = [1.0, 1.0, 1.0, 0.95];

/// A replay on screen.
pub struct ReplayView {
    pub playback: Playback,
    /// The Player whose eyes the camera looks through; `None` is the free
    /// camera.
    pub follow: Option<InstanceId>,
    /// A kill cam: it ends at its last frame, back to the game. A file
    /// stays on its last frame.
    pub buffer: bool,
    /// Terrains' meshes for the replay's world, kept between frames.
    pub terrain: TerrainCache,
}

impl ReplayView {
    /// A `.replay` file's bytes, for `game`.
    pub fn open(bytes: &[u8], game: &World) -> Result<ReplayView, String> {
        let replay = Replay::from_bytes(bytes)?;
        Ok(ReplayView {
            playback: Playback::new(replay, game)?,
            follow: None,
            buffer: false,
            terrain: TerrainCache::default(),
        })
    }

    /// The buffer `PlayBuffer()` asked for, from its start.
    pub fn kill_cam(play: BufferPlay, game: &World) -> Result<ReplayView, String> {
        let mut playback = Playback::new(play.replay, game)?;
        playback.seek(play.start);
        Ok(ReplayView {
            playback,
            follow: play.follow,
            buffer: true,
            terrain: TerrainCache::default(),
        })
    }

    /// Plays on for a frame of `seconds`. False once a kill cam is over.
    pub fn step(&mut self, seconds: f64) -> bool {
        self.playback.step(seconds);
        !(self.buffer && self.playback.at_end())
    }

    /// Does what `key` asks; true if it was one of the replay's keys. A
    /// kill cam ends with Escape (the caller drops it on `Some(false)`).
    pub fn key(&mut self, key: KeyCode) -> Option<bool> {
        let playback = &mut self.playback;
        match key {
            KeyCode::Space => playback.paused = !playback.paused,
            KeyCode::Left => playback.seek(playback.time() - JUMP),
            KeyCode::Right => playback.seek(playback.time() + JUMP),
            KeyCode::Return => playback.seek(playback.span().0),
            KeyCode::Up | KeyCode::Down => {
                let at = SPEEDS
                    .iter()
                    .position(|&speed| speed >= playback.speed)
                    .unwrap_or(2);
                let at = if key == KeyCode::Up {
                    (at + 1).min(SPEEDS.len() - 1)
                } else {
                    at.saturating_sub(1)
                };
                playback.speed = SPEEDS[at];
            }
            KeyCode::Tab => self.follow = self.next_follow(),
            KeyCode::Escape if self.buffer => return Some(false),
            _ => return None,
        }
        Some(true)
    }

    /// The player after the one followed, then the free camera.
    fn next_follow(&self) -> Option<InstanceId> {
        let players: Vec<InstanceId> = self
            .playback
            .players()
            .into_iter()
            .filter(|&player| eyes_of(self.playback.world(), player).is_some())
            .collect();
        match self
            .follow
            .and_then(|f| players.iter().position(|&p| p == f))
        {
            Some(at) => players.get(at + 1).copied(),
            None => players.first().copied(),
        }
    }

    /// The camera through the followed player's eyes, else `free`.
    pub fn camera(&self, free: axcenity_render::Camera) -> axcenity_render::Camera {
        let eyes = self
            .follow
            .and_then(|player| eyes_of(self.playback.world(), player));
        match eyes {
            Some((from, toward)) => axcenity_render::Camera {
                position: Vec3::new(from.x, from.y, from.z),
                look_at: Vec3::new(toward.x, toward.y, toward.z),
                ..free
            },
            None => free,
        }
    }

    /// The timeline along the bottom of a `width` × `height` screen: a
    /// bar filled to where playback is, and the time, speed and who's
    /// followed.
    pub fn timeline_items(&self, width: f32, height: f32) -> Vec<UiDrawItem> {
        let margin = 24.0;
        let bar_w = (width - margin * 2.0).max(40.0);
        let bar_y = height - 36.0;
        let (start, end) = self.playback.span();
        let length = (end - start).max(1e-6);
        let done = ((self.playback.time() - start) / length).clamp(0.0, 1.0) as f32;
        let rectangle = |position: [f32; 2], size: [f32; 2], color: [f32; 4]| {
            UiDrawItem::Rectangle(UiRectangle {
                position,
                size,
                color,
                color_end: color,
                border_radius: size[1].min(6.0) * 0.5,
                ..Default::default()
            })
        };
        let mut items = vec![
            rectangle([0.0, height - 64.0], [width, 64.0], PANEL),
            rectangle([margin, bar_y], [bar_w, 6.0], TRACK),
        ];
        if done > 0.0 {
            items.push(rectangle([margin, bar_y], [bar_w * done, 6.0], FILL));
        }
        items.push(rectangle(
            [margin + bar_w * done - 6.0, bar_y - 3.0],
            [12.0, 12.0],
            FILL,
        ));
        items.push(UiDrawItem::Text(UiText {
            position: [margin, height - 60.0],
            size: [bar_w, 20.0],
            text: self.caption(),
            font_size: 13.0,
            font_family: "Montserrat".to_owned(),
            color: CAPTION,
            align: TextAlign::Left,
            vertical_align: VerticalAlign::Middle,
            ..UiText::default()
        }));
        items
    }

    /// "Replay 0:12 / 2:00 · 2× · paused · following Player2".
    pub fn caption(&self) -> String {
        let (start, end) = self.playback.span();
        let clock = |seconds: f64| {
            let seconds = seconds.max(0.0).round() as u64;
            format!("{}:{:02}", seconds / 60, seconds % 60)
        };
        let mut caption = format!(
            "{} {} / {}",
            if self.buffer { "Kill cam" } else { "Replay" },
            clock(self.playback.time() - start),
            clock(end - start)
        );
        if self.playback.speed != 1.0 {
            caption.push_str(&format!(" · {}×", self.playback.speed));
        }
        if self.playback.paused {
            caption.push_str(" · paused");
        }
        let world = self.playback.world();
        match self.follow.and_then(|player| world.get(player)) {
            Some(player) => caption.push_str(&format!(" · {}", player.name())),
            None => caption.push_str(" · free camera"),
        }
        caption
    }
}

#[cfg(test)]
mod tests {
    use axcenity_core::{ClassName, Value, Vector3};
    use axcenity_net::{GameInfo, Recorder};

    use super::*;

    /// Three seconds of a game where Player1's head walks along X.
    fn recorded() -> (World, Replay) {
        let mut world = World::new();
        let player = world.create(ClassName::Player).unwrap();
        world.set_name(player, "Player1").unwrap();
        world.set_parent(player, Some(world.players())).unwrap();
        let character = world.create(ClassName::Model).unwrap();
        world
            .set_parent(character, Some(world.workspace()))
            .unwrap();
        let head = world.create(ClassName::Part).unwrap();
        world.set_name(head, "Head").unwrap();
        world.set_parent(head, Some(character)).unwrap();
        world
            .set_property(player, "Character", Value::Instance(Some(character)))
            .unwrap();
        let game = world.clone();
        let mut recorder = Recorder::new();
        for frame in 0..=180 {
            let x = frame as f32 / 60.0;
            world
                .set_property(head, "Position", Value::Vector3(Vector3::new(x, 5.0, 0.0)))
                .unwrap();
            recorder.frame(&world, 1.0 / 60.0);
        }
        let info = GameInfo {
            name: "Walk".to_owned(),
            version: "1.0".to_owned(),
        };
        (game, recorder.replay(&info))
    }

    #[test]
    fn the_keys_pause_jump_change_speed_and_follow_a_player() {
        let (game, replay) = recorded();
        let mut view = ReplayView::open(&replay.to_bytes(), &game).unwrap();
        assert!(
            view.caption().starts_with("Replay 0:00 / 0:03"),
            "{}",
            view.caption()
        );
        assert_eq!(view.key(KeyCode::Space), Some(true));
        assert!(view.playback.paused);
        view.step(1.0);
        assert_eq!(view.playback.time(), view.playback.span().0, "paused");
        view.key(KeyCode::Space);
        view.key(KeyCode::Up);
        assert_eq!(view.playback.speed, 2.0);
        view.step(0.5);
        let start = view.playback.span().0;
        assert!((view.playback.time() - start - 1.0).abs() < 1e-9);
        view.key(KeyCode::Right);
        assert!(view.playback.at_end(), "5 seconds on is the end");
        assert!(view.step(1.0), "a file stays on its last frame");
        view.key(KeyCode::Return);
        assert_eq!(view.playback.time(), start);
        for _ in 0..6 {
            view.key(KeyCode::Down);
        }
        assert_eq!(view.playback.speed, 0.25);
        assert_eq!(
            view.key(KeyCode::Escape),
            None,
            "only a kill cam ends with Escape"
        );
        assert_eq!(view.key(KeyCode::A), None);

        // Tab: through Player1's eyes, then the free camera again.
        let free = axcenity_render::Camera {
            position: Vec3::new(0.0, 50.0, 50.0),
            look_at: Vec3::ZERO,
            fov_y_degrees: 70.0,
        };
        view.key(KeyCode::Right);
        view.key(KeyCode::Tab);
        assert!(view.caption().ends_with("· Player1"), "{}", view.caption());
        let camera = view.camera(free);
        assert!(
            (camera.position - Vec3::new(3.0, 5.0, 0.0)).length() < 1e-4,
            "{camera:?}"
        );
        assert!(
            (camera.look_at - Vec3::new(3.0, 5.0, -1.0)).length() < 1e-4,
            "{camera:?}"
        );
        assert_eq!(camera.fov_y_degrees, 70.0);
        view.key(KeyCode::Tab);
        assert_eq!(view.camera(free).position, free.position);
        assert!(view.caption().ends_with("· free camera"));
        assert_eq!(view.timeline_items(800.0, 600.0).len(), 5);
    }

    #[test]
    fn a_kill_cam_ends_with_its_last_frame_or_escape() {
        let (game, replay) = recorded();
        let follow = replay_follow(&game);
        let play = BufferPlay {
            start: replay.span().1 - 2.0,
            replay,
            follow,
        };
        let mut view = ReplayView::kill_cam(play.clone(), &game).unwrap();
        assert!(
            view.caption().starts_with("Kill cam 0:01"),
            "{}",
            view.caption()
        );
        assert!(view.step(1.0));
        assert!(!view.step(1.5), "over, back to the game");
        let mut view = ReplayView::kill_cam(play, &game).unwrap();
        assert_eq!(view.key(KeyCode::Escape), Some(false));
        assert!(ReplayView::open(b"not a replay", &game).is_err());
    }

    fn replay_follow(game: &World) -> Option<InstanceId> {
        game.get(game.players())?.children().first().copied()
    }
}
