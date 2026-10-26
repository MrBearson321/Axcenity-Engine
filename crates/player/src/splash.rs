//! The player's "Made with Axcenity" splash: a black screen, the white
//! wordmark, a white loading bar, and a short fade.
//!
//! The bar is timed, not a measure of remaining load. The clock starts on the
//! first drawn frame and only counts drawn frames (a slow one counts as at
//! most [`MAX_STEP`]), so loading can't use the splash up before it's seen.
//! Click or a key skips to the fade-out. A game that waits for a script
//! ([`Splash::hold`]) keeps the full bar up until it says it's ready
//! ([`Splash::release`]); only then can it fade or be skipped.

use std::f32::consts::TAU;
use std::sync::Arc;
use std::time::Instant;

use anyhow::Context;
use axcenity_render::{
    TextAlign, TextureSource, UiDrawItem, UiImage, UiRectangle, UiText, VerticalAlign,
};

const WORDMARK_PNG: &[u8] = include_bytes!("../../../assets/branding/wordmark-white.png");

/// Fade the plate in over this many seconds.
pub const INTRO: f32 = 0.5;
/// The bar starts filling after this.
pub const BAR_START: f32 = 0.3;
/// The bar is full, and the fade-out begins.
pub const BAR_END: f32 = 2.8;
/// The overlay is gone.
pub const DURATION: f32 = 3.3;
/// The most one frame moves the clock, in seconds.
pub const MAX_STEP: f32 = 0.05;

const BLACK: [f32; 3] = [0.027, 0.027, 0.031];
const BLACK_END: [f32; 3] = [0.055, 0.055, 0.06];
const WHITE: [f32; 3] = [0.96, 0.96, 0.96];
const TRACK: [f32; 3] = [0.22, 0.22, 0.23];
const RING: [f32; 3] = [0.78, 0.78, 0.8];
const BAR_HEIGHT: f32 = 4.0;

/// Built-in white wordmark, decoded once when the player starts.
struct PackedTexture {
    rgba: Vec<u8>,
    width: u32,
    height: u32,
}

impl TextureSource for PackedTexture {
    fn key(&self) -> u64 {
        0xA5CE_7174_0001
    }
    fn size(&self) -> (u32, u32) {
        (self.width, self.height)
    }
    fn rgba(&self) -> &[u8] {
        &self.rgba
    }
}

/// Owns the splash clock and the wordmark texture.
pub struct Splash {
    /// Seconds of splash shown so far, stopped at the full bar while held.
    elapsed: f32,
    /// Seconds shown, held or not: the rings and dots keep moving.
    motion: f32,
    /// Waiting for a script before the fade.
    held: bool,
    /// When the last frame was drawn; none until the first.
    last_frame: Option<Instant>,
    skipped_at: Option<f32>,
    texture: Option<Arc<dyn TextureSource>>,
    texture_size: [f32; 2],
}

impl Splash {
    pub fn new() -> Self {
        match load_wordmark() {
            Ok((texture, texture_size)) => Self {
                elapsed: 0.0,
                motion: 0.0,
                held: false,
                last_frame: None,
                skipped_at: None,
                texture: Some(texture),
                texture_size,
            },
            Err(err) => {
                log::warn!("no splash wordmark: {err:#}");
                Self {
                    elapsed: 0.0,
                    motion: 0.0,
                    held: false,
                    last_frame: None,
                    skipped_at: None,
                    texture: None,
                    texture_size: [1.0, 1.0],
                }
            }
        }
    }

    /// Moves the clock on by the time since the last frame, capped. Call
    /// once per drawn frame; the first call starts the clock.
    pub fn tick(&mut self) {
        let now = Instant::now();
        if let Some(last) = self.last_frame {
            let step = (now - last).as_secs_f32().min(MAX_STEP);
            self.motion += step;
            self.elapsed += step;
            if self.held {
                self.elapsed = self.elapsed.min(BAR_END);
            }
        }
        self.last_frame = Some(now);
    }

    /// Stay on the full bar until [`Self::release`].
    pub fn hold(&mut self) {
        self.held = true;
    }

    /// The game is ready: the splash may fade once its bar is full.
    pub fn release(&mut self) {
        self.held = false;
    }

    /// Jump to the fade-out, if the splash is still covering the game and
    /// isn't waiting for a script.
    pub fn skip(&mut self) {
        if !self.held
            && self.last_frame.is_some()
            && self.skipped_at.is_none()
            && self.elapsed < BAR_END
        {
            self.skipped_at = Some(self.elapsed);
        }
    }

    /// True while the overlay should eat clicks and keys.
    pub fn covers(&self) -> bool {
        self.visual_t().is_some()
    }

    /// Screen-space items for this frame, or none when the splash is over.
    pub fn items(&self, width: f32, height: f32, caption: &str) -> Vec<UiDrawItem> {
        let Some(t) = self.visual_t() else {
            return Vec::new();
        };
        overlay(
            t,
            self.motion,
            [width, height],
            self.texture.clone(),
            self.texture_size,
            caption,
        )
    }

    fn visual_t(&self) -> Option<f32> {
        self.last_frame?;
        let t = match self.skipped_at {
            Some(t0) => BAR_END + (self.elapsed - t0),
            None => self.elapsed,
        };
        (t < DURATION).then_some(t)
    }
}

fn load_wordmark() -> anyhow::Result<(Arc<dyn TextureSource>, [f32; 2])> {
    let (rgba, width, height) =
        crate::icon::decode(WORDMARK_PNG).context("the splash wordmark is not a PNG")?;
    Ok((
        Arc::new(PackedTexture {
            rgba,
            width,
            height,
        }),
        [width as f32, height as f32],
    ))
}

fn ease_out_cubic(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    1.0 - (1.0 - t).powi(3)
}

fn alpha_at(t: f32) -> f32 {
    if t < INTRO {
        (t / INTRO).clamp(0.0, 1.0)
    } else if t < BAR_END {
        1.0
    } else {
        (1.0 - (t - BAR_END) / (DURATION - BAR_END)).clamp(0.0, 1.0)
    }
}

fn bar_at(t: f32) -> f32 {
    if t <= BAR_START {
        0.0
    } else {
        ease_out_cubic((t - BAR_START) / (BAR_END - BAR_START))
    }
}

fn rgba(rgb: [f32; 3], a: f32) -> [f32; 4] {
    [rgb[0], rgb[1], rgb[2], a]
}

/// The overlay at time `t` (seconds from start, after skip mapping). The
/// rings and dots move with `motion`, which goes on while the bar is held.
pub fn overlay(
    t: f32,
    motion: f32,
    size: [f32; 2],
    texture: Option<Arc<dyn TextureSource>>,
    texture_size: [f32; 2],
    caption: &str,
) -> Vec<UiDrawItem> {
    if t.is_nan() || t >= DURATION {
        return Vec::new();
    }
    let a = alpha_at(t);
    // The backdrop is solid from the first frame, so the game never shows
    // through while the logo fades in.
    let backdrop = alpha_at(t.max(INTRO));
    if backdrop <= 0.0 {
        return Vec::new();
    }
    let [width, height] = size;
    let cx = width * 0.5;
    let cy = height * 0.47;
    let logo_w = (width * 0.34).clamp(220.0, 460.0);
    let aspect = if texture_size[0] > 0.0 {
        texture_size[1] / texture_size[0]
    } else {
        0.3
    };
    let logo_h = logo_w * aspect;
    let logo_x = cx - logo_w * 0.5;
    let logo_y = cy - logo_h * 0.5;
    let intro = ease_out_cubic((t / INTRO).min(1.0));
    let rise = (1.0 - intro) * 18.0;

    let mut items = Vec::new();
    items.push(UiDrawItem::Rectangle(UiRectangle {
        position: [0.0, 0.0],
        size: [width, height],
        color: rgba(BLACK, backdrop),
        color_end: rgba(BLACK_END, backdrop),
        border_color: [0.0; 4],
        border_width: 0.0,
        border_radius: 0.0,
        ..Default::default()
    }));

    // Rings start just outside the wordmark's corners and fade as they grow.
    for (phase, scale) in [(0.0, 1.0), (0.45, 1.12)] {
        let pulse = ((motion * 0.85 + phase).fract()).clamp(0.0, 1.0);
        let radius = (logo_w * 0.6 + pulse * logo_w * 0.3) * scale;
        let ring_a = (1.0 - pulse) * 0.18 * a;
        items.push(UiDrawItem::Rectangle(UiRectangle {
            position: [cx - radius, cy - radius + rise],
            size: [radius * 2.0, radius * 2.0],
            color: [0.0, 0.0, 0.0, 0.0],
            color_end: [0.0, 0.0, 0.0, 0.0],
            border_color: rgba(RING, ring_a),
            border_width: 1.5,
            border_radius: radius,
            ..Default::default()
        }));
    }

    for i in 0..6 {
        let angle = motion * 0.55 + i as f32 * TAU / 6.0;
        let orbit = logo_w * 0.64 + (motion * 1.7 + i as f32).sin() * 8.0;
        let x = cx + angle.cos() * orbit;
        let y = cy + angle.sin() * orbit * 0.42 + rise;
        let r = 2.0 + (motion * 2.0 + i as f32).sin().abs() * 1.2;
        items.push(UiDrawItem::Rectangle(UiRectangle {
            position: [x - r, y - r],
            size: [r * 2.0, r * 2.0],
            color: rgba(WHITE, 0.35 * a),
            color_end: rgba(WHITE, 0.35 * a),
            border_color: [0.0; 4],
            border_width: 0.0,
            border_radius: r,
            ..Default::default()
        }));
    }

    if let Some(texture) = texture {
        items.push(UiDrawItem::Image(UiImage {
            position: [logo_x, logo_y + rise],
            size: [logo_w, logo_h],
            tint: [1.0, 1.0, 1.0, a],
            border_radius: 0.0,
            path: String::new(),
            texture: Some(texture),
            slice: None,
        }));
    } else {
        items.push(UiDrawItem::Text(UiText {
            position: [logo_x, logo_y + rise],
            size: [logo_w, logo_h],
            text: "Axcenity".to_owned(),
            font_size: logo_h * 0.6,
            font_family: "Montserrat".to_owned(),
            color: rgba(WHITE, a),
            align: TextAlign::Center,
            vertical_align: VerticalAlign::Middle,
            ..UiText::default()
        }));
    }

    let bar_w = logo_w * 0.6;
    let bar_h = BAR_HEIGHT;
    let bar_x = cx - bar_w * 0.5;
    let bar_y = logo_y + logo_h + rise + 44.0;
    items.push(UiDrawItem::Rectangle(UiRectangle {
        position: [bar_x, bar_y],
        size: [bar_w, bar_h],
        color: rgba(TRACK, a),
        color_end: rgba(TRACK, a),
        border_color: [0.0; 4],
        border_width: 0.0,
        border_radius: bar_h * 0.5,
        ..Default::default()
    }));
    let fill = bar_at(t) * bar_w;
    if fill > 0.5 {
        items.push(UiDrawItem::Rectangle(UiRectangle {
            position: [bar_x, bar_y],
            size: [fill, bar_h],
            color: rgba(WHITE, a),
            color_end: rgba(WHITE, a),
            border_color: [0.0; 4],
            border_width: 0.0,
            border_radius: bar_h * 0.5,
            ..Default::default()
        }));
        // A brighter leading edge, sweeping with the fill.
        let shimmer_w = (18.0_f32).min(fill);
        items.push(UiDrawItem::Rectangle(UiRectangle {
            position: [bar_x + fill - shimmer_w, bar_y],
            size: [shimmer_w, bar_h],
            color: rgba([1.0, 1.0, 1.0], a),
            color_end: rgba(WHITE, a * 0.4),
            border_color: [0.0; 4],
            border_width: 0.0,
            border_radius: bar_h * 0.5,
            ..Default::default()
        }));
    }

    items.push(UiDrawItem::Text(UiText {
        position: [cx - bar_w * 0.5, bar_y + 16.0],
        size: [bar_w, 22.0],
        text: caption.to_owned(),
        font_size: 13.0,
        font_family: "Montserrat".to_owned(),
        color: rgba([0.72, 0.72, 0.74], a),
        align: TextAlign::Center,
        vertical_align: VerticalAlign::Middle,
        ..UiText::default()
    }));
    items
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bar_width(items: &[UiDrawItem]) -> Option<f32> {
        items.iter().find_map(|item| {
            let UiDrawItem::Rectangle(rect) = item else {
                return None;
            };
            (rect.size[1] == BAR_HEIGHT && rect.color[0] > 0.8).then_some(rect.size[0])
        })
    }

    #[test]
    fn the_overlay_is_gone_after_the_duration() {
        assert!(
            overlay(
                DURATION,
                DURATION,
                [1280.0, 720.0],
                None,
                [1000.0, 500.0],
                "Made with Axcenity",
            )
            .is_empty()
        );
        assert!(
            overlay(
                DURATION + 1.0,
                DURATION + 1.0,
                [1280.0, 720.0],
                None,
                [1000.0, 500.0],
                "x"
            )
            .is_empty()
        );
    }

    #[test]
    fn the_bar_grows_while_the_splash_runs() {
        let early = overlay(
            0.4,
            0.4,
            [1280.0, 720.0],
            None,
            [1000.0, 500.0],
            "Made with Axcenity",
        );
        let mid = overlay(
            1.6,
            1.6,
            [1280.0, 720.0],
            None,
            [1000.0, 500.0],
            "Made with Axcenity",
        );
        let late = overlay(
            2.7,
            2.7,
            [1280.0, 720.0],
            None,
            [1000.0, 500.0],
            "Made with Axcenity",
        );
        let a = bar_width(&early).unwrap_or(0.0);
        let b = bar_width(&mid).expect("the mid splash has a bar");
        let c = bar_width(&late).expect("the late splash has a bar");
        assert!(b > a);
        assert!(c > b);
        assert!(!early.is_empty());
    }

    #[test]
    fn skip_starts_the_fade() {
        let mut splash = Splash::new();
        assert!(!splash.covers(), "nothing is shown before the first frame");
        splash.tick();
        splash.elapsed = 0.8;
        assert!(splash.covers());
        splash.skip();
        let t = splash.visual_t().expect("still fading");
        assert!(t >= BAR_END);
    }

    #[test]
    fn the_first_frame_already_hides_the_game() {
        let items = overlay(0.0, 0.0, [1280.0, 720.0], None, [1000.0, 300.0], "x");
        let Some(UiDrawItem::Rectangle(backdrop)) = items.first() else {
            panic!("the backdrop comes first");
        };
        assert_eq!(backdrop.size, [1280.0, 720.0]);
        assert_eq!(backdrop.color[3], 1.0);
    }

    #[test]
    fn a_held_splash_waits_on_the_full_bar_until_released() {
        let mut splash = Splash::new();
        splash.hold();
        splash.tick();
        splash.elapsed = BAR_END;
        splash.last_frame = Some(Instant::now() - std::time::Duration::from_secs(1));
        splash.tick();
        assert_eq!(splash.elapsed, BAR_END);
        assert!(splash.motion > 0.0, "the rings keep moving");
        splash.skip();
        assert!(
            splash.covers(),
            "a click doesn't skip a game that isn't ready"
        );
        splash.release();
        splash.skip();
        assert!(splash.visual_t().expect("fading") >= BAR_END);
    }

    #[test]
    fn a_slow_frame_moves_the_clock_by_at_most_one_step() {
        let mut splash = Splash::new();
        splash.tick();
        splash.last_frame = Some(Instant::now() - std::time::Duration::from_secs(5));
        splash.tick();
        assert!(splash.elapsed <= MAX_STEP + f32::EPSILON);
        assert!(splash.covers());
    }
}
