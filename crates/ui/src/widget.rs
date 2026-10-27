//! The ready-made widgets: Slider, Toggle, Dropdown and ProgressBar.
//!
//! Each is one Instance. Its parts are drawn here, not child Instances, and
//! styled with pseudo-elements: a Slider's `::track`, `::fill` and
//! `::thumb`, a Toggle's `::track`, `::thumb` (a switch's knob) and
//! `::check` (a checkbox's tick), a ProgressBar's `::fill`, and an open
//! Dropdown's `::list` and each `::option` in it. What a part's CSS leaves
//! out comes from its defaults, which are written as CSS too
//! ([`with_defaults`]). [`UiInteraction`](crate::UiInteraction) changes
//! their `Value` from the pointer and draws an open Dropdown's list
//! ([`dropdown_list`]) over everything else.

use super::*;
use crate::style::{PseudoElement, resolve_part_styles};

/// The blue of a Slider's and a ProgressBar's fill, and of a Toggle that's on.
const ACCENT: &str = "#3b82f6";
/// The grey of an empty track.
const TRACK: &str = "#5b6170";

/// The most rows an open Dropdown's list shows before it scrolls.
pub const DROPDOWN_ROWS: usize = 6;

/// A Slider's track, as laid out: where the thumb's middle is at
/// `MinValue` (`from`) and at `MaxValue` (`to`), in authored pixels from
/// the element's start along `axis` (0 across, 1 down).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UiTrack {
    pub axis: usize,
    pub from: f32,
    pub to: f32,
}

impl UiTrack {
    /// How far along the track `point` is, from 0 at MinValue to 1 at
    /// MaxValue, for an element laid out at `rect`, `per_unit` pixels to an
    /// authored pixel. `point` is in the element's upright layout (see
    /// [`UiElement::local`]).
    pub fn fraction_at(&self, rect: UiRect, per_unit: f32, point: [f32; 2]) -> f32 {
        let start = if self.axis == 0 { rect.x } else { rect.y };
        let along = (point[self.axis] - start) / per_unit.max(1e-4);
        let span = self.to - self.from;
        if span.abs() < 1e-3 {
            return 0.0;
        }
        ((along - self.from) / span).clamp(0.0, 1.0)
    }
}

fn number(instance: &Instance, name: &str, default: f64) -> f64 {
    instance
        .property(name)
        .and_then(Value::as_number)
        .filter(|n| n.is_finite())
        .unwrap_or(default)
}

/// A Slider's or ProgressBar's `MinValue` and `MaxValue`, the max never
/// below the min.
fn value_range(instance: &Instance) -> (f64, f64) {
    let min = number(instance, "MinValue", 0.0);
    (min, number(instance, "MaxValue", 100.0).max(min))
}

/// How far `Value` is from MinValue to MaxValue, from 0 to 1.
pub(crate) fn value_fraction(instance: &Instance) -> f32 {
    let (min, max) = value_range(instance);
    if max - min <= 0.0 {
        return 0.0;
    }
    ((number(instance, "Value", min) - min) / (max - min)).clamp(0.0, 1.0) as f32
}

/// The Slider value `fraction` of the way along its track: a whole number
/// of Steps from MinValue (any number when Step is 0), kept in range.
pub(crate) fn slider_value_at(instance: &Instance, fraction: f32) -> f64 {
    let (min, max) = value_range(instance);
    let mut value = min + f64::from(fraction.clamp(0.0, 1.0)) * (max - min);
    let step = number(instance, "Step", 0.0);
    if step > 0.0 {
        value = min + ((value - min) / step).round() * step;
    }
    // Tidies the last digits a sum of steps leaves, as in 0.30000000000000004.
    ((value * 1e9).round() / 1e9).clamp(min, max)
}

/// A Slider's `Value` one press of an arrow key or the d-pad on from where
/// it is, up (`by` 1) or down (-1): a twentieth of its range, in whole
/// `Step`s and at least one, kept between MinValue and MaxValue.
pub(crate) fn slider_nudged(instance: &Instance, by: f64) -> f64 {
    let (min, max) = value_range(instance);
    let step = number(instance, "Step", 0.0);
    let mut amount = (max - min) / 20.0;
    if step > 0.0 {
        amount = (amount / step).round().max(1.0) * step;
    }
    let mut value = number(instance, "Value", min).clamp(min, max) + by * amount;
    if step > 0.0 {
        value = min + ((value - min) / step).round() * step;
    }
    ((value * 1e9).round() / 1e9).clamp(min, max)
}

pub(crate) fn vertical(instance: &Instance) -> bool {
    instance
        .property("Orientation")
        .and_then(Value::as_str)
        .is_some_and(|orientation| orientation.eq_ignore_ascii_case("Vertical"))
}

/// A Dropdown's `Options`: one per line, without blank lines or the
/// spaces around each.
pub(crate) fn options(instance: &Instance) -> Vec<String> {
    instance
        .property("Options")
        .and_then(Value::as_str)
        .unwrap_or("")
        .lines()
        .map(str::trim)
        .filter(|option| !option.is_empty())
        .map(str::to_owned)
        .collect()
}

fn px(pixels: f32) -> String {
    format!("{pixels}px")
}

/// A colour as CSS, for a default that's worked out from other styles.
fn css_rgba([r, g, b, a]: [f32; 4]) -> String {
    format!(
        "rgba({:.0}, {:.0}, {:.0}, {a})",
        r * 255.0,
        g * 255.0,
        b * 255.0
    )
}

/// `styles` with each of `defaults` (a property and its value) filled in
/// where its CSS doesn't set it.
fn with_defaults(mut styles: ResolvedStyles, defaults: &[(&str, String)]) -> ResolvedStyles {
    for (name, value) in defaults {
        styles
            .values
            .entry((*name).to_owned())
            .or_insert_with(|| value.clone());
    }
    styles
}

/// A part's box from its CSS: its background (or gradient), corners,
/// border and shadows, faded by `opacity` and its own CSS `opacity`.
fn paint_part(rect: UiRect, styles: &ResolvedStyles, opacity: f32, out: &mut Vec<UiDrawItem>) {
    let opacity = opacity * css_number(styles, "opacity").unwrap_or(1.0).clamp(0.0, 1.0);
    if opacity <= 0.0 {
        return;
    }
    let (fill, fill_end) = match css_gradient(styles) {
        Some(((start, start_alpha), (end, end_alpha))) => (
            [start.r, start.g, start.b, start_alpha * opacity],
            [end.r, end.g, end.b, end_alpha * opacity],
        ),
        None => {
            let (color, alpha) =
                css_color(styles, "background-color").unwrap_or((Color3::new(0.0, 0.0, 0.0), 0.0));
            let fill = [color.r, color.g, color.b, alpha * opacity];
            (fill, fill)
        }
    };
    let radius = css_length_property(styles, "border-radius")
        .unwrap_or(0.0)
        .max(0.0);
    paint_box(
        &BoxPaint {
            rect,
            fill,
            fill_end,
            radius,
            opacity,
            focus_ring: None,
            border: true,
        },
        styles,
        out,
    );
}

/// A line from `from` to `to`, `thickness` wide with round ends, painted
/// as a turned part (a checkbox's tick, a Dropdown's caret).
fn paint_stroke(
    from: [f32; 2],
    to: [f32; 2],
    thickness: f32,
    styles: &ResolvedStyles,
    opacity: f32,
    out: &mut Vec<UiDrawItem>,
) {
    let (dx, dy) = (to[0] - from[0], to[1] - from[1]);
    let length = dx.hypot(dy) + thickness;
    let middle = [(from[0] + to[0]) * 0.5, (from[1] + to[1]) * 0.5];
    let rect = UiRect {
        x: middle[0] - length * 0.5,
        y: middle[1] - thickness * 0.5,
        width: length,
        height: thickness,
    };
    let styles = with_defaults(styles.clone(), &[("border-radius", px(thickness * 0.5))]);
    let start = out.len();
    paint_part(rect, &styles, opacity, out);
    // Clockwise, on a screen whose y runs down.
    let degrees = dy.atan2(dx).to_degrees();
    for item in &mut out[start..] {
        if let UiDrawItem::Rectangle(rectangle) = item {
            rectangle.rotation_degrees += degrees;
        }
    }
}

/// What the parts of one widget are styled from.
struct Parts<'a> {
    world: &'a World,
    id: InstanceId,
    stylesheets: &'a [Stylesheet],
    pseudo: PseudoClasses<'a>,
}

impl Parts<'_> {
    fn styles(&self, part: PseudoElement, defaults: &[(&str, String)]) -> ResolvedStyles {
        let styles = resolve_part_styles(self.world, self.id, self.stylesheets, self.pseudo, part);
        with_defaults(styles, defaults)
    }
}

/// Draws the parts of the widget `id` (a Slider, Toggle, Dropdown or
/// ProgressBar) laid out at `rect` with its own `styles`, over its
/// background. Returns a Slider's track.
#[allow(clippy::too_many_arguments)] // The widget's box and how it's drawn.
pub(crate) fn paint_widget(
    world: &World,
    id: InstanceId,
    rect: UiRect,
    styles: &ResolvedStyles,
    opacity: f32,
    stylesheets: &[Stylesheet],
    pseudo: PseudoClasses<'_>,
    out: &mut UiLayout,
) -> Option<UiTrack> {
    let instance = world.get(id)?;
    let parts = Parts {
        world,
        id,
        stylesheets,
        pseudo,
    };
    match instance.class() {
        ClassName::Slider => Some(paint_slider(&parts, instance, rect, opacity, out)),
        ClassName::Toggle => {
            paint_toggle(&parts, instance, rect, opacity, &mut out.draw_items);
            None
        }
        ClassName::Dropdown => {
            paint_dropdown(instance, rect, styles, opacity, out);
            None
        }
        ClassName::ProgressBar => {
            paint_progress(&parts, instance, rect, styles, opacity, &mut out.draw_items);
            None
        }
        _ => None,
    }
}

/// A track across the element (down it, when Vertical) with the fill up
/// to the thumb, and the thumb at `Value`.
fn paint_slider(
    parts: &Parts,
    instance: &Instance,
    rect: UiRect,
    opacity: f32,
    out: &mut UiLayout,
) -> UiTrack {
    let vertical = vertical(instance);
    let (length, cross) = if vertical {
        (rect.height, rect.width)
    } else {
        (rect.width, rect.height)
    };
    let (along_name, across_name) = if vertical {
        ("height", "width")
    } else {
        ("width", "height")
    };
    // A box `size` across, from `start` for `run` along the track, centred
    // across the element.
    let place = |start: f32, run: f32, size: f32| {
        if vertical {
            UiRect {
                x: rect.x + (cross - size) * 0.5,
                y: rect.y + start,
                width: size,
                height: run,
            }
        } else {
            UiRect {
                x: rect.x + start,
                y: rect.y + (cross - size) * 0.5,
                width: run,
                height: size,
            }
        }
    };

    let thumb_styles = parts.styles(PseudoElement::Thumb, &[]);
    let thumb = css_length_property(&thumb_styles, along_name)
        .unwrap_or(cross.min(20.0))
        .clamp(0.0, length);
    let thumb_across = css_length_property(&thumb_styles, across_name).unwrap_or(thumb);
    let thumb_styles = with_defaults(
        thumb_styles,
        &[
            ("background-color", "#ffffff".to_owned()),
            ("border-radius", px(thumb.min(thumb_across) * 0.5)),
            ("box-shadow", "0 1px 3px rgba(0, 0, 0, 0.4)".to_owned()),
        ],
    );
    let track_styles = parts.styles(PseudoElement::Track, &[]);
    let thickness = css_length_property(&track_styles, across_name)
        .unwrap_or(6.0)
        .clamp(0.0, cross);
    let radius = px(thickness * 0.5);
    let track_styles = with_defaults(
        track_styles,
        &[
            ("background-color", TRACK.to_owned()),
            ("border-radius", radius.clone()),
        ],
    );
    let fill_styles = parts.styles(
        PseudoElement::Fill,
        &[
            ("background-color", ACCENT.to_owned()),
            ("border-radius", radius),
        ],
    );

    // Vertical sliders have MinValue at the bottom.
    let (from, to) = if vertical {
        (length - thumb * 0.5, thumb * 0.5)
    } else {
        (thumb * 0.5, length - thumb * 0.5)
    };
    let centre = from + (to - from) * value_fraction(instance);
    let filled = if vertical {
        place(centre, length - centre, thickness)
    } else {
        place(0.0, centre, thickness)
    };
    let items = &mut out.draw_items;
    paint_part(place(0.0, length, thickness), &track_styles, opacity, items);
    paint_part(filled, &fill_styles, opacity, items);
    paint_part(
        place(centre - thumb * 0.5, thumb, thumb_across),
        &thumb_styles,
        opacity,
        items,
    );
    UiTrack {
        axis: usize::from(vertical),
        from,
        to,
    }
}

/// A switch (a rounded track with a knob at the left when off and the
/// right when on), or a checkbox (a square at the left, ticked when on).
fn paint_toggle(
    parts: &Parts,
    instance: &Instance,
    rect: UiRect,
    opacity: f32,
    out: &mut Vec<UiDrawItem>,
) {
    let on = instance.property("Value").and_then(Value::as_bool) == Some(true);
    let checkbox = instance
        .property("Style")
        .and_then(Value::as_str)
        .is_some_and(|style| style.eq_ignore_ascii_case("Checkbox"));
    if checkbox {
        let side = rect.width.min(rect.height).max(0.0);
        let square = UiRect {
            x: rect.x,
            y: rect.y + (rect.height - side) * 0.5,
            width: side,
            height: side,
        };
        let mut defaults = vec![("border-radius", px(side * 0.2))];
        if on {
            defaults.push(("background-color", ACCENT.to_owned()));
        } else {
            defaults.push(("background-color", "#f3f4f6".to_owned()));
            defaults.push(("border", "2px solid #9ca3af".to_owned()));
        }
        paint_part(
            square,
            &parts.styles(PseudoElement::Track, &defaults),
            opacity,
            out,
        );
        if on {
            let check = parts.styles(
                PseudoElement::Check,
                &[("background-color", "#ffffff".to_owned())],
            );
            let at = |x: f32, y: f32| [square.x + side * x, square.y + side * y];
            let thickness = (side * 0.13).max(1.5);
            paint_stroke(
                at(0.25, 0.52),
                at(0.43, 0.7),
                thickness,
                &check,
                opacity,
                out,
            );
            paint_stroke(
                at(0.43, 0.7),
                at(0.76, 0.32),
                thickness,
                &check,
                opacity,
                out,
            );
        }
        return;
    }
    let track = parts.styles(
        PseudoElement::Track,
        &[
            (
                "background-color",
                if on { ACCENT } else { TRACK }.to_owned(),
            ),
            ("border-radius", px(rect.height * 0.5)),
        ],
    );
    paint_part(rect, &track, opacity, out);
    let thumb = parts.styles(PseudoElement::Thumb, &[]);
    let knob = css_length_property(&thumb, "width")
        .unwrap_or(rect.height - 6.0)
        .clamp(0.0, rect.width.min(rect.height));
    let inset = (rect.height - knob) * 0.5;
    let thumb = with_defaults(
        thumb,
        &[
            ("background-color", "#ffffff".to_owned()),
            ("border-radius", px(knob * 0.5)),
            ("box-shadow", "0 1px 3px rgba(0, 0, 0, 0.4)".to_owned()),
        ],
    );
    let x = if on {
        rect.x + rect.width - inset - knob
    } else {
        rect.x + inset
    };
    let knob_box = UiRect {
        x,
        y: rect.y + inset,
        width: knob,
        height: knob,
    };
    paint_part(knob_box, &thumb, opacity, out);
}

/// How far in from a Dropdown's left its text starts, and how much room
/// its caret takes on the right, in authored pixels.
const DROPDOWN_INSET: f32 = 10.0;
const DROPDOWN_CARET: f32 = 26.0;

/// The chosen option (or the Placeholder, faint) and a caret on the right.
fn paint_dropdown(
    instance: &Instance,
    rect: UiRect,
    styles: &ResolvedStyles,
    opacity: f32,
    out: &mut UiLayout,
) {
    let value = instance
        .property("Value")
        .and_then(Value::as_str)
        .unwrap_or("");
    let placeholder = value.is_empty();
    let text = if placeholder {
        instance
            .property("Placeholder")
            .and_then(Value::as_str)
            .unwrap_or("")
    } else {
        value
    };
    let mut styled = styled_text(
        instance,
        styles,
        opacity,
        placeholder,
        text,
        &mut out.warnings,
    );
    if css_property(styles, "vertical-align").is_none() {
        styled.vertical_align = VerticalAlign::Middle;
    }
    styled.wrap = false;
    if !text.is_empty() && rect.width > 0.0 && rect.height > 0.0 {
        out.draw_items.push(UiDrawItem::Text(UiText {
            position: [rect.x + DROPDOWN_INSET, rect.y],
            size: [
                (rect.width - DROPDOWN_INSET - DROPDOWN_CARET).max(0.0),
                rect.height,
            ],
            ..styled
        }));
    }
    let (color, alpha) =
        css_color(styles, "color").unwrap_or_else(|| (color_property(instance, "TextColor3"), 1.0));
    let caret = with_defaults(
        ResolvedStyles::default(),
        &[(
            "background-color",
            css_rgba([color.r, color.g, color.b, alpha]),
        )],
    );
    let (x, y) = (rect.x + rect.width - 15.0, rect.y + rect.height * 0.5);
    let items = &mut out.draw_items;
    paint_stroke(
        [x - 4.0, y - 2.0],
        [x, y + 2.0],
        1.75,
        &caret,
        opacity,
        items,
    );
    paint_stroke(
        [x, y + 2.0],
        [x + 4.0, y - 2.0],
        1.75,
        &caret,
        opacity,
        items,
    );
}

/// The fill inside the border, from the left (the bottom, when Vertical)
/// as far as `Value` goes.
fn paint_progress(
    parts: &Parts,
    instance: &Instance,
    rect: UiRect,
    styles: &ResolvedStyles,
    opacity: f32,
    out: &mut Vec<UiDrawItem>,
) {
    let fraction = value_fraction(instance);
    if fraction <= 0.0 {
        return;
    }
    let [top, right, bottom, left] = css_borders(styles).widths();
    let inside = UiRect {
        x: rect.x + left,
        y: rect.y + top,
        width: (rect.width - left - right).max(0.0),
        height: (rect.height - top - bottom).max(0.0),
    };
    let widest = top.max(right).max(bottom).max(left);
    let radius = (css_length_property(styles, "border-radius").unwrap_or(0.0) - widest).max(0.0);
    let filled = if vertical(instance) {
        let height = inside.height * fraction;
        UiRect {
            y: inside.y + inside.height - height,
            height,
            ..inside
        }
    } else {
        UiRect {
            width: inside.width * fraction,
            ..inside
        }
    };
    let fill = parts.styles(
        PseudoElement::Fill,
        &[
            ("background-color", ACCENT.to_owned()),
            ("border-radius", px(radius)),
        ],
    );
    paint_part(filled, &fill, opacity, out);
}

/// An open Dropdown's list as the last layout drew it, in screen pixels.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DropdownList {
    pub(crate) id: InstanceId,
    /// The list's box.
    pub(crate) rect: UiRect,
    /// The space above the first row and below the last.
    pub(crate) pad: f32,
    pub(crate) row_height: f32,
    /// How far it's scrolled, in authored pixels, and how far it can be.
    pub(crate) scroll: f32,
    pub(crate) max_scroll: f32,
    pub(crate) per_unit: f32,
    pub(crate) options: Vec<String>,
}

impl DropdownList {
    /// The option under `point`, if any.
    pub(crate) fn option_at(&self, [x, y]: [f32; 2]) -> Option<usize> {
        let inside = self.rect.contains(x, y)
            && y >= self.rect.y + self.pad
            && y <= self.rect.y + self.rect.height - self.pad;
        if !inside || self.row_height <= 0.0 {
            return None;
        }
        let along = y - self.rect.y - self.pad + self.scroll * self.per_unit;
        let index = (along / self.row_height).floor();
        (index >= 0.0 && (index as usize) < self.options.len()).then_some(index as usize)
    }
}

/// Lays out and draws the list of the open Dropdown laid out as `element`,
/// on a screen `screen` pixels big: just below it (above, when there's
/// more room there), as wide as it, showing up to [`DROPDOWN_ROWS`] rows
/// scrolled `scroll` authored pixels (`None` scrolls to the chosen
/// option), with the option `cursor` (keyboard or controller) lit, else
/// the one under `pointer`. `None` when it has no options.
pub(crate) fn dropdown_list(
    world: &World,
    element: &UiElement,
    stylesheets: &[Stylesheet],
    screen: (f32, f32),
    scroll: Option<f32>,
    pointer: Option<[f32; 2]>,
    cursor: Option<usize>,
) -> Option<(DropdownList, Vec<UiDrawItem>)> {
    let instance = world.get(element.id)?;
    let options = options(instance);
    if options.is_empty() {
        return None;
    }
    let per_unit = element.pixels_per_unit.max(1e-4);
    let pseudo = PseudoClasses::default();
    let own = resolve_styles(world, element.id, stylesheets, pseudo);
    // Rows take the Dropdown's text styles unless `::option` sets its own.
    let inherited: Vec<(&str, String)> = [
        "color",
        "font-family",
        "font-size",
        "font-weight",
        "font-style",
        "letter-spacing",
        "text-transform",
    ]
    .into_iter()
    .filter_map(|name| css_property(&own, name).map(|value| (name, value)))
    .collect();
    let option_styles = with_defaults(
        resolve_part_styles(
            world,
            element.id,
            stylesheets,
            pseudo,
            PseudoElement::Option,
        ),
        &inherited,
    );
    let list_styles = with_defaults(
        resolve_part_styles(world, element.id, stylesheets, pseudo, PseudoElement::List),
        &[
            ("background-color", "#ffffff".to_owned()),
            ("border", "1px solid rgba(0, 0, 0, 0.12)".to_owned()),
            ("border-radius", "6px".to_owned()),
            ("box-shadow", "0 4px 12px rgba(0, 0, 0, 0.3)".to_owned()),
        ],
    );

    // The Dropdown's box on the screen, around it if it's turned.
    let corners = [
        [element.rect.x, element.rect.y],
        [element.rect.x + element.rect.width, element.rect.y],
        [element.rect.x, element.rect.y + element.rect.height],
        [
            element.rect.x + element.rect.width,
            element.rect.y + element.rect.height,
        ],
    ]
    .map(|corner| element.on_screen(corner));
    let left = corners.iter().map(|c| c[0]).fold(f32::INFINITY, f32::min);
    let top = corners.iter().map(|c| c[1]).fold(f32::INFINITY, f32::min);
    let bottom = corners
        .iter()
        .map(|c| c[1])
        .fold(f32::NEG_INFINITY, f32::max);

    let pad = 4.0;
    let gap = 4.0 * per_unit;
    let row = css_length_property(&option_styles, "height")
        .unwrap_or(element.rect.height / per_unit)
        .max(1.0);
    let rows = options.len().min(DROPDOWN_ROWS);
    let width = element.rect.width / per_unit;
    let height = rows as f32 * row + pad * 2.0;
    let max_scroll = ((options.len() - rows) as f32 * row).max(0.0);
    let chosen = instance
        .property("Value")
        .and_then(Value::as_str)
        .and_then(|value| options.iter().position(|option| option == value));
    let scroll = scroll
        .unwrap_or_else(|| {
            chosen.map_or(0.0, |index| (index + 1).saturating_sub(rows) as f32 * row)
        })
        .clamp(0.0, max_scroll);

    let (screen_width, screen_height) = screen;
    let below = bottom + gap;
    let y = if below + height * per_unit > screen_height && top - gap - height * per_unit >= 0.0 {
        top - gap - height * per_unit
    } else {
        below
    };
    let x = left.min(screen_width - width * per_unit).max(0.0);
    let list = DropdownList {
        id: element.id,
        rect: UiRect {
            x,
            y,
            width: width * per_unit,
            height: height * per_unit,
        },
        pad: pad * per_unit,
        row_height: row * per_unit,
        scroll,
        max_scroll,
        per_unit,
        options: options.clone(),
    };
    let lit = cursor.or_else(|| pointer.and_then(|point| list.option_at(point)));

    // Drawn in authored pixels from the list's corner, then scaled onto the screen.
    let mut items = Vec::new();
    let whole = UiRect {
        x: 0.0,
        y: 0.0,
        width,
        height,
    };
    paint_part(whole, &list_styles, 1.0, &mut items);
    let rows_start = items.len();
    let highlight = |color: &str| {
        with_defaults(
            ResolvedStyles::default(),
            &[("background-color", color.to_owned())],
        )
    };
    for (index, option) in options.iter().enumerate() {
        let row_box = UiRect {
            x: 0.0,
            y: pad + index as f32 * row - scroll,
            width,
            height: row,
        };
        // Rows scrolled out of the list are cut away whole.
        if row_box.y + row <= pad || row_box.y >= height - pad {
            continue;
        }
        paint_part(row_box, &option_styles, 1.0, &mut items);
        if chosen == Some(index) {
            paint_part(
                row_box,
                &highlight("rgba(59, 130, 246, 0.18)"),
                1.0,
                &mut items,
            );
        }
        if lit == Some(index) {
            paint_part(row_box, &highlight("rgba(0, 0, 0, 0.08)"), 1.0, &mut items);
        }
        let mut text = styled_text(
            instance,
            &option_styles,
            1.0,
            false,
            option,
            &mut Vec::new(),
        );
        if css_property(&option_styles, "vertical-align").is_none() {
            text.vertical_align = VerticalAlign::Middle;
        }
        text.wrap = false;
        items.push(UiDrawItem::Text(UiText {
            position: [DROPDOWN_INSET, row_box.y],
            size: [(width - DROPDOWN_INSET * 2.0).max(0.0), row],
            ..text
        }));
    }
    if max_scroll > 0.0 {
        let room = height - pad * 2.0;
        let length = (room * rows as f32 / options.len() as f32)
            .max(12.0)
            .min(room);
        let thumb = UiRect {
            x: width - 6.0,
            y: pad + (room - length) * scroll / max_scroll,
            width: 4.0,
            height: length,
        };
        paint_part(thumb, &highlight("rgba(0, 0, 0, 0.3)"), 1.0, &mut items);
        let rounded = items.len() - 1;
        if let UiDrawItem::Rectangle(rectangle) = &mut items[rounded] {
            rectangle.border_radius = 2.0;
        }
    }
    let rows_clip = UiClip {
        position: [0.0, pad],
        size: [width, height - pad * 2.0],
        radius: 0.0,
        rotation_degrees: 0.0,
    };
    for item in &mut items[rows_start..] {
        set_clip(item, Some(rows_clip));
    }
    let place = Affine {
        scale: [per_unit, per_unit],
        offset: [x, y],
    };
    for item in &mut items {
        transform_draw_item(item, place);
    }
    Some((list, items))
}
