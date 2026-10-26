//! ScrollingFrames: the canvas their children are laid out on, how far
//! it's scrolled, and their scroll bars (`::scrollbar` and `::thumb`).
//!
//! The canvas is measured in authored pixels, like `Position` and `Size`.
//! Its bars are worked out from the element's laid-out box whenever
//! they're needed ([`scroll_bars`]), so they're right after a ScreenGui
//! is scaled or the artboard zoomed.

use super::*;
use crate::style::{PseudoElement, resolve_part_styles};

/// The shortest a thumb is drawn, in authored pixels, so a long canvas
/// still has one to grab.
const MIN_THUMB: f32 = 16.0;

/// A ScrollingFrame's canvas, as laid out.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct UiScroll {
    /// The canvas's width and height, in authored pixels: `CanvasSize`,
    /// grown to fit the children by `AutomaticCanvasSize`, and never
    /// smaller than the window onto it.
    pub canvas: [f32; 2],
    /// The window onto the canvas: the frame inside its border, in
    /// authored pixels.
    pub window: [f32; 2],
    /// Where the window starts in the frame: its left and top borders.
    pub inset: [f32; 2],
    /// How far it's scrolled across and down, kept between 0 and
    /// [`UiScroll::max`].
    pub position: [f32; 2],
    /// Whether it scrolls across and down: `ScrollingDirection` lets it,
    /// and the canvas is bigger than the window that way.
    pub scrolls: [bool; 2],
    /// `ScrollBarThickness`, in authored pixels.
    pub thickness: f32,
}

impl UiScroll {
    /// How far it can scroll across and down.
    pub fn max(&self) -> [f32; 2] {
        [0, 1].map(|axis| {
            if self.scrolls[axis] {
                (self.canvas[axis] - self.window[axis]).max(0.0)
            } else {
                0.0
            }
        })
    }

    /// `position` kept in range, and at 0 on an axis it doesn't scroll.
    pub fn clamp(&self, position: [f32; 2]) -> [f32; 2] {
        let max = self.max();
        [0, 1].map(|axis| {
            let at = finite(position[axis]);
            if self.scrolls[axis] {
                at.clamp(0.0, max[axis])
            } else {
                0.0
            }
        })
    }

    /// Where the canvas is, for an element laid out at `rect`.
    pub(crate) fn canvas_rect(&self, rect: UiRect) -> UiRect {
        UiRect {
            x: rect.x + self.inset[0] - self.position[0],
            y: rect.y + self.inset[1] - self.position[1],
            width: self.canvas[0],
            height: self.canvas[1],
        }
    }
}

/// One of a ScrollingFrame's scroll bars, in the pixels of its element's box.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScrollBar {
    /// 0 for the bar across the bottom, 1 for the one down the right.
    pub axis: usize,
    pub track: UiRect,
    /// The part dragged along the track: as long as the window is of the
    /// canvas, and as far along as the canvas is scrolled.
    pub thumb: UiRect,
}

impl ScrollBar {
    fn along(rect: UiRect, axis: usize) -> (f32, f32) {
        if axis == 0 {
            (rect.x, rect.width)
        } else {
            (rect.y, rect.height)
        }
    }

    /// The scroll position along its axis that puts the thumb's start
    /// at `start` (pixels, as the track is), kept in range.
    pub fn position_at(&self, scroll: &UiScroll, start: f32) -> f32 {
        let (track_start, track_length) = Self::along(self.track, self.axis);
        let (_, thumb_length) = Self::along(self.thumb, self.axis);
        let room = track_length - thumb_length;
        if room <= 0.0 {
            return 0.0;
        }
        ((start - track_start) / room).clamp(0.0, 1.0) * scroll.max()[self.axis]
    }

    /// Where the thumb starts along its axis.
    pub fn thumb_start(&self) -> f32 {
        Self::along(self.thumb, self.axis).0
    }

    /// How long the thumb is along its axis.
    pub fn thumb_length(&self) -> f32 {
        Self::along(self.thumb, self.axis).1
    }
}

/// The scroll bars of a ScrollingFrame laid out at `rect`, `per_unit`
/// pixels to an authored pixel: one for each way it scrolls, unless
/// `ScrollBarThickness` is 0. They sit over its content, inside its border.
pub fn scroll_bars(rect: UiRect, scroll: &UiScroll, per_unit: f32) -> Vec<ScrollBar> {
    let thickness = scroll.thickness.max(0.0) * per_unit;
    if thickness <= 0.0 {
        return Vec::new();
    }
    let window = UiRect {
        x: rect.x + scroll.inset[0] * per_unit,
        y: rect.y + scroll.inset[1] * per_unit,
        width: scroll.window[0] * per_unit,
        height: scroll.window[1] * per_unit,
    };
    let max = scroll.max();
    let both = scroll.scrolls[0] && scroll.scrolls[1];
    let mut bars = Vec::new();
    for axis in [1, 0] {
        if !scroll.scrolls[axis] {
            continue;
        }
        // The bars meet in the bottom right corner without overlapping.
        let corner = if both { thickness } else { 0.0 };
        let track = if axis == 1 {
            UiRect {
                x: window.x + window.width - thickness,
                y: window.y,
                width: thickness,
                height: (window.height - corner).max(0.0),
            }
        } else {
            UiRect {
                x: window.x,
                y: window.y + window.height - thickness,
                width: (window.width - corner).max(0.0),
                height: thickness,
            }
        };
        let (track_start, track_length) = ScrollBar::along(track, axis);
        let shown = (scroll.window[axis] / scroll.canvas[axis].max(1e-3)).clamp(0.0, 1.0);
        let length = (track_length * shown)
            .max((MIN_THUMB * per_unit).min(track_length))
            .min(track_length);
        let fraction = if max[axis] > 0.0 {
            scroll.position[axis] / max[axis]
        } else {
            0.0
        };
        let start = track_start + (track_length - length) * fraction;
        let thumb = if axis == 1 {
            UiRect {
                y: start,
                height: length,
                ..track
            }
        } else {
            UiRect {
                x: start,
                width: length,
                ..track
            }
        };
        bars.push(ScrollBar { axis, track, thumb });
    }
    bars
}

/// Measures the canvas of the ScrollingFrame `id`, laid out at `rect`:
/// its size, and how far it's scrolled (the designer's own position for
/// it, when it has one, else `CanvasPosition`).
pub(crate) fn measure(
    world: &World,
    id: InstanceId,
    instance: &Instance,
    styles: &ResolvedStyles,
    rect: UiRect,
    stylesheets: &[Stylesheet],
    pseudo: PseudoClasses<'_>,
) -> UiScroll {
    let [top, right, bottom, left] = css_borders(styles).widths();
    let window = [
        (rect.width - left - right).max(0.0),
        (rect.height - top - bottom).max(0.0),
    ];
    let size = instance
        .property("CanvasSize")
        .and_then(Value::as_udim2)
        .unwrap_or_default();
    let mut canvas = [
        (finite(size.x.scale) * window[0] + finite(size.x.offset)).max(window[0]),
        (finite(size.y.scale) * window[1] + finite(size.y.offset)).max(window[1]),
    ];
    let axes = |name: &str, default: &str| {
        let value = instance
            .property(name)
            .and_then(Value::as_str)
            .unwrap_or(default)
            .to_ascii_uppercase();
        [value.contains('X'), value.contains('Y')]
    };
    let grow = axes("AutomaticCanvasSize", "None");
    if grow[0] || grow[1] {
        // Lay the children out once on the canvas to see how far they reach.
        let mut trial = UiLayout::default();
        let bounds = UiRect {
            x: 0.0,
            y: 0.0,
            width: canvas[0],
            height: canvas[1],
        };
        let quiet = PseudoClasses {
            transitions: None,
            ..pseudo
        };
        let inherited = Inherited {
            opacity: 1.0,
            clip: None,
        };
        layout_children(world, id, bounds, &mut trial, stylesheets, inherited, quiet);
        for element in &trial.elements {
            let reach = [
                element.rect.x + element.rect.width,
                element.rect.y + element.rect.height,
            ];
            for axis in [0, 1] {
                if grow[axis] && reach[axis].is_finite() {
                    canvas[axis] = canvas[axis].max(reach[axis]);
                }
            }
        }
    }
    let direction = axes("ScrollingDirection", "XY");
    let mut scroll = UiScroll {
        canvas,
        window,
        inset: [left, top],
        position: [0.0; 2],
        scrolls: [0, 1].map(|axis| direction[axis] && canvas[axis] > window[axis] + 0.5),
        thickness: instance
            .property("ScrollBarThickness")
            .and_then(Value::as_number)
            .map_or(12.0, |n| finite(n as f32).max(0.0)),
    };
    let wanted = pseudo
        .scrolled
        .and_then(|scrolled| scrolled.get(&id).copied())
        .or_else(|| {
            instance
                .property("CanvasPosition")
                .and_then(Value::as_vector2)
                .map(|at| [at.x, at.y])
        })
        .unwrap_or([0.0; 2]);
    scroll.position = scroll.clamp(wanted);
    scroll
}

/// Draws the ScrollingFrame `id`'s bars over its content: each track
/// (`::scrollbar`, clear unless styled) and its thumb (`::thumb`, a grey
/// pill unless styled).
#[allow(clippy::too_many_arguments)] // The frame's box and how it's drawn.
pub(crate) fn paint_bars(
    world: &World,
    id: InstanceId,
    rect: UiRect,
    scroll: &UiScroll,
    inherited: Inherited,
    stylesheets: &[Stylesheet],
    pseudo: PseudoClasses<'_>,
    out: &mut Vec<UiDrawItem>,
) {
    let bars = scroll_bars(rect, scroll, 1.0);
    if bars.is_empty() || inherited.opacity <= 0.0 {
        return;
    }
    let track_styles =
        resolve_part_styles(world, id, stylesheets, pseudo, PseudoElement::Scrollbar);
    let thumb_styles = resolve_part_styles(world, id, stylesheets, pseudo, PseudoElement::Thumb);
    let start = out.len();
    for bar in bars {
        for (part, styles, default) in [
            (bar.track, &track_styles, (Color3::new(0.0, 0.0, 0.0), 0.0)),
            (bar.thumb, &thumb_styles, (Color3::new(0.5, 0.5, 0.5), 0.8)),
        ] {
            let opacity =
                inherited.opacity * css_number(styles, "opacity").unwrap_or(1.0).clamp(0.0, 1.0);
            let (color, alpha) = css_color(styles, "background-color").unwrap_or(default);
            let fill = [color.r, color.g, color.b, alpha * opacity];
            let radius = css_length_property(styles, "border-radius")
                .unwrap_or(part.width.min(part.height) * 0.5)
                .max(0.0);
            paint::paint_box(
                &paint::BoxPaint {
                    rect: part,
                    fill,
                    fill_end: fill,
                    radius,
                    opacity,
                    focus_ring: None,
                    border: true,
                },
                styles,
                out,
            );
        }
    }
    for item in &mut out[start..] {
        set_clip(item, inherited.clip);
    }
}
