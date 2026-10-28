//! The ProximityPrompt's default UI (Phase 44): a box over each prompt
//! that shows, with the key or button that uses it in a round cap, the
//! hold ring round the cap, and "ActionText · ObjectText".
//!
//! It's styled like a widget: `ProximityPrompt` rules style the box and
//! its text (`background-color`, `color`, `font-size`, `padding`,
//! `border-radius`, `opacity` and the rest), `ProximityPrompt::key` the
//! cap and its letter, and `ProximityPrompt::ring` the ring, whose `color`
//! is the part held so far, `background-color` the rest, and `width` its
//! thickness. What the CSS leaves out comes from defaults written as CSS.
//! Every prompt shares these rules: prompts have no `Id` or `Class`.

use super::*;
use crate::style::{PseudoElement, resolve_part_styles};
use crate::widget::{paint_part, px, with_defaults};

/// One prompt that shows, as the host's script runtime found it.
#[derive(Clone, Debug, PartialEq)]
pub struct PromptView {
    pub prompt: InstanceId,
    /// Where it is in the world; the box is drawn centred over it.
    pub position: [f32; 3],
    /// How far the hold is, 0 to 1.
    pub progress: f32,
    /// The key or button that uses it, such as `E` or `X`.
    pub key: String,
}

/// How many pieces the ring is drawn in.
const RING_PIECES: usize = 48;

/// The pixel `point` is at in a `width` × `height` view from `camera`, or
/// `None` behind it.
fn project(camera: &Camera, width: u32, height: u32, point: Vec3) -> Option<[f32; 2]> {
    if width == 0 || height == 0 {
        return None;
    }
    let aspect = width as f32 / height as f32;
    let view = rh::view::look_at_mat4(camera.position, camera.look_at, Vec3::Y);
    let tan_half = (camera.fov_y_degrees.to_radians() * 0.5).tan();
    let seen = view.transform_point3(point);
    if seen.z >= -0.05 {
        return None;
    }
    let depth = -seen.z;
    let x = seen.x / (depth * tan_half * aspect);
    let y = seen.y / (depth * tan_half);
    (x.is_finite() && y.is_finite()).then_some([
        (x + 1.0) * 0.5 * width as f32,
        (1.0 - y) * 0.5 * height as f32,
    ])
}

/// What the prompt says: `ActionText · ObjectText`, or `ActionText` alone
/// when there's no ObjectText.
pub fn prompt_text(world: &World, prompt: InstanceId) -> String {
    let text = |name| {
        world
            .get(prompt)
            .and_then(|instance| instance.property(name))
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim()
            .to_owned()
    };
    let (action, object) = (text("ActionText"), text("ObjectText"));
    match (action.is_empty(), object.is_empty()) {
        (_, true) => action,
        (true, false) => object,
        (false, false) => format!("{action} · {object}"),
    }
}

/// The draw items for every prompt in `prompts`, seen from `camera` in a
/// `width` × `height` view, to go over the rest of the UI.
pub fn prompt_items(
    world: &World,
    prompts: &[PromptView],
    stylesheets: &[Stylesheet],
    camera: &Camera,
    width: u32,
    height: u32,
) -> Vec<UiDrawItem> {
    let mut items = Vec::new();
    for prompt in prompts {
        let point = Vec3::from_array(prompt.position);
        if let Some(at) = project(camera, width, height, point)
            && world.contains(prompt.prompt)
        {
            paint_prompt(world, prompt, stylesheets, at, &mut items);
        }
    }
    items
}

/// A text item `text`, in `rect`, from `styles`.
pub(crate) fn text_item(
    text: String,
    rect: UiRect,
    font_size: f32,
    styles: &ResolvedStyles,
    opacity: f32,
    align: TextAlign,
) -> UiDrawItem {
    let (color, alpha) = css_color(styles, "color").unwrap_or((Color3::new(1.0, 1.0, 1.0), 1.0));
    UiDrawItem::Text(UiText {
        position: [rect.x, rect.y],
        size: [rect.width, rect.height],
        text,
        font_size,
        font_family: css_property(styles, "font-family")
            .map(|family| font_family_name(&family))
            .unwrap_or_else(|| "Montserrat".to_owned()),
        color: [color.r, color.g, color.b, alpha * opacity],
        align,
        vertical_align: VerticalAlign::Middle,
        weight: css_font_weight(styles),
        wrap: false,
        ..UiText::default()
    })
}

/// About how wide `text` is at `font_size`: a little wider than tooltips
/// reckon, since the prompt's text is bolder and never wraps.
pub(crate) fn text_width(text: &str, font_size: f32) -> f32 {
    text.chars().count() as f32 * font_size * 0.6
}

fn paint_prompt(
    world: &World,
    prompt: &PromptView,
    stylesheets: &[Stylesheet],
    at: [f32; 2],
    out: &mut Vec<UiDrawItem>,
) {
    let id = prompt.prompt;
    let scale = world.ui_settings().text_scale as f32;
    let pseudo = PseudoClasses::default();
    let styles = with_defaults(
        resolve_styles(world, id, stylesheets, pseudo),
        &[
            ("background-color", "rgba(17, 17, 20, 0.8)".to_owned()),
            ("border-radius", px(10.0)),
            ("color", "#ffffff".to_owned()),
            ("font-weight", "600".to_owned()),
        ],
    );
    let part = |element, defaults: &[(&str, String)]| {
        let styles = resolve_part_styles(world, id, stylesheets, pseudo, element);
        with_defaults(styles, defaults)
    };
    let opacity = css_number(&styles, "opacity")
        .unwrap_or(1.0)
        .clamp(0.0, 1.0);
    let font_size = css_length_property(&styles, "font-size")
        .unwrap_or(16.0)
        .max(1.0)
        * scale;
    let padding = css_box_values(&styles, "padding").map_or([8.0, 14.0, 8.0, 8.0], |sides| {
        sides.map(|side| match side {
            taffy::LengthPercentage::Length(pixels) => pixels.max(0.0),
            taffy::LengthPercentage::Percent(_) => 0.0,
        })
    });

    // The key cap: a circle, or a pill for a longer name such as Shift.
    let key_styles = part(
        PseudoElement::Key,
        &[
            ("background-color", "#ffffff".to_owned()),
            ("color", "#111114".to_owned()),
            ("font-weight", "700".to_owned()),
        ],
    );
    let key_font = css_length_property(&key_styles, "font-size")
        .map(|size| size.max(1.0) * scale)
        .unwrap_or(if prompt.key.chars().count() > 2 {
            11.0 * scale
        } else {
            16.0 * scale
        });
    let key_height = css_length_property(&key_styles, "height")
        .unwrap_or(32.0 * scale)
        .max(1.0);
    let key_width = css_length_property(&key_styles, "width")
        .unwrap_or_else(|| key_height.max(text_width(&prompt.key, key_font) + 12.0))
        .max(1.0);
    let key_styles = with_defaults(key_styles, &[("border-radius", px(key_height * 0.5))]);

    let ring_styles = part(
        PseudoElement::Ring,
        &[
            ("color", "#ffffff".to_owned()),
            ("background-color", "rgba(255, 255, 255, 0.25)".to_owned()),
        ],
    );
    let thickness = css_length_property(&ring_styles, "width")
        .unwrap_or(3.0)
        .max(0.0);
    // The ring sits just outside the cap.
    let ring_gap = 3.0;
    let cap = [
        key_width + 2.0 * (ring_gap + thickness),
        key_height + 2.0 * (ring_gap + thickness),
    ];

    let label = prompt_text(world, id);
    let gap = if label.is_empty() { 0.0 } else { 10.0 };
    let label_width = text_width(&label, font_size);
    let inner_height = cap[1].max(font_size * 1.25);
    let size = [
        padding[3] + cap[0] + gap + label_width + padding[1],
        padding[0] + inner_height + padding[2],
    ];
    let rect = UiRect {
        x: at[0] - size[0] * 0.5,
        y: at[1] - size[1] * 0.5,
        width: size[0],
        height: size[1],
    };
    paint_part(rect, &styles, opacity, out);

    let cap_rect = UiRect {
        x: rect.x + padding[3],
        y: rect.y + padding[0] + (inner_height - cap[1]) * 0.5,
        width: cap[0],
        height: cap[1],
    };
    let middle = [
        cap_rect.x + cap_rect.width * 0.5,
        cap_rect.y + cap_rect.height * 0.5,
    ];
    paint_ring(
        middle,
        (cap[0].min(cap[1]) - thickness) * 0.5,
        thickness,
        prompt.progress,
        &ring_styles,
        opacity,
        out,
    );
    let key_rect = UiRect {
        x: middle[0] - key_width * 0.5,
        y: middle[1] - key_height * 0.5,
        width: key_width,
        height: key_height,
    };
    paint_part(key_rect, &key_styles, opacity, out);
    out.push(text_item(
        prompt.key.clone(),
        key_rect,
        key_font,
        &key_styles,
        opacity,
        TextAlign::Center,
    ));
    if !label.is_empty() {
        let label_rect = UiRect {
            x: cap_rect.x + cap_rect.width + gap,
            y: rect.y + padding[0],
            width: label_width,
            height: inner_height,
        };
        out.push(text_item(
            label,
            label_rect,
            font_size,
            &styles,
            opacity,
            TextAlign::Left,
        ));
    }
}

/// The hold ring round `middle`, `radius` to the middle of its line: the
/// whole ring in `background-color`, then `progress` of it from the top,
/// clockwise, in `color`.
fn paint_ring(
    middle: [f32; 2],
    radius: f32,
    thickness: f32,
    progress: f32,
    styles: &ResolvedStyles,
    opacity: f32,
    out: &mut Vec<UiDrawItem>,
) {
    if thickness <= 0.0 || radius <= 0.0 {
        return;
    }
    let piece = std::f32::consts::TAU / RING_PIECES as f32;
    // Each piece is a short straight line along the ring, a little longer
    // than its share so the pieces meet.
    let length = 2.0 * radius * (piece * 0.5).sin() + 0.5;
    let mut track = styles.clone();
    track.values.remove("border-radius");
    let mut fill = track.clone();
    if let Some(color) = css_property(styles, "color") {
        fill.values.insert("background-color".to_owned(), color);
    }
    let filled = (progress.clamp(0.0, 1.0) * RING_PIECES as f32).round() as usize;
    for (index, styles) in (0..RING_PIECES)
        .map(|index| (index, &track))
        .chain((0..filled).map(|index| (index, &fill)))
    {
        // From the top (-90°), clockwise on a screen whose y runs down.
        let angle = -std::f32::consts::FRAC_PI_2 + (index as f32 + 0.5) * piece;
        let centre = [
            middle[0] + radius * angle.cos(),
            middle[1] + radius * angle.sin(),
        ];
        let rect = UiRect {
            x: centre[0] - length * 0.5,
            y: centre[1] - thickness * 0.5,
            width: length,
            height: thickness,
        };
        let start = out.len();
        paint_part(rect, styles, opacity, out);
        // Along the ring: at right angles to the way out from the middle.
        let degrees = angle.to_degrees() + 90.0;
        for item in &mut out[start..] {
            if let UiDrawItem::Rectangle(rectangle) = item {
                rectangle.rotation_degrees += degrees;
            }
        }
    }
}
