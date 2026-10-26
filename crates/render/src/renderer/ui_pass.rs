//! Screen-space rectangles, images and the UI and sky pipelines.
//!
//! Rectangle vertices are pixels from the top left. Glyphs are drawn by glyphon
//! inside `ScenePipeline`; this module builds the quads and the pipelines
//! (`ui.wgsl`, and the sky entry of `shader.wgsl`, after `lighting.wgsl`).

use std::ops::Range;

use glyphon::Color as GlyphColor;

use crate::draw_list::{
    UiBorderSide, UiClip, UiDrawItem, UiImage, UiRectangle, UiShadow, UiSlice, UiText, is_turned,
    turn_point, turned_bounds,
};

use super::{DEPTH_FORMAT, HDR_FORMAT, NO_CLIP, UI_BOX, UI_INSET_SHADOW, UI_SHADOW, UiVertex};

use super::lighting::linear_color;

/// One layer of UI, as drawn: its rectangles (a range of the UI vertex
/// buffer), then its images (texture key, first vertex, vertex count), then
/// its text, if it has any.
pub(super) struct UiLayer {
    pub rectangles: Range<u32>,
    pub images: Vec<(u64, u32, u32)>,
    pub text: bool,
}

/// Splits `items` into layers drawn one after another. A layer draws its
/// rectangles, then its images, then its text, so a layer ends before an
/// item that would otherwise go under something earlier that it overlaps:
/// a Frame over a TextLabel, or a loading screen over the game's UI.
/// Items that don't overlap share a layer whatever their order.
pub(super) fn ui_layers(items: &[UiDrawItem]) -> Vec<Range<usize>> {
    let mut layers = Vec::new();
    let mut start = 0;
    // The images and text in this layer: their kind and box.
    let mut above: Vec<(u8, [f32; 4])> = Vec::new();
    for (index, item) in items.iter().enumerate() {
        let (kind, bounds) = kind_and_bounds(item);
        if above
            .iter()
            .any(|&(earlier, other)| earlier > kind && overlaps(other, bounds))
        {
            layers.push(start..index);
            start = index;
            above.clear();
        }
        if kind > 0 {
            above.push((kind, bounds));
        }
    }
    if start < items.len() {
        layers.push(start..items.len());
    }
    layers
}

/// Where a kind of item comes in a layer (rectangles 0, images 1, text 2),
/// and the box it can draw in: left, top, right, bottom. Turned text is
/// drawn as a picture (see [`TurnedText`](super::TurnedText)), so it comes
/// with the images.
pub(super) fn kind_and_bounds(item: &UiDrawItem) -> (u8, [f32; 4]) {
    let (kind, [x, y], [w, h], margin, turn) = match item {
        UiDrawItem::Rectangle(rectangle) => (
            0,
            rectangle.position,
            rectangle.size,
            shadow_spill(rectangle),
            rectangle.rotation_degrees,
        ),
        UiDrawItem::Image(image) => (1, image.position, image.size, 0.0, image.rotation_degrees),
        UiDrawItem::Text(text) => (
            if is_turned(text.rotation_degrees) {
                1
            } else {
                2
            },
            text.position,
            text.size,
            text_spill(text),
            text.rotation_degrees,
        ),
    };
    let mut bounds = turned_bounds(
        [x - margin, y - margin, w + margin * 2.0, h + margin * 2.0],
        turn,
    );
    // Nothing is drawn outside its clip.
    if let Some(clip) = item_clip(item) {
        let [left, top, right, bottom] = clip_bounds(clip);
        bounds[0] = bounds[0].max(left);
        bounds[1] = bounds[1].max(top);
        bounds[2] = bounds[2].min(right);
        bounds[3] = bounds[3].min(bottom);
    }
    (kind, bounds)
}

/// The upright box (left, top, right, bottom) around a clip, turned or not.
pub(super) fn clip_bounds(clip: UiClip) -> [f32; 4] {
    turned_bounds(
        [
            clip.position[0],
            clip.position[1],
            clip.size[0],
            clip.size[1],
        ],
        clip.rotation_degrees,
    )
}

/// The box an item is cut to, if it's cut.
pub(super) fn item_clip(item: &UiDrawItem) -> Option<UiClip> {
    match item {
        UiDrawItem::Rectangle(rectangle) => rectangle.clip,
        UiDrawItem::Image(image) => image.clip,
        UiDrawItem::Text(text) => text.clip,
    }
}

/// A clip as the shader reads it: its box, and its radius and turn (in
/// radians).
fn clip_numbers(clip: Option<UiClip>) -> ([f32; 4], [f32; 2]) {
    match clip {
        Some(clip)
            if clip
                .position
                .iter()
                .chain(&clip.size)
                .all(|n| n.is_finite())
                && clip.radius.is_finite()
                && clip.rotation_degrees.is_finite() =>
        {
            (
                [
                    clip.position[0],
                    clip.position[1],
                    clip.size[0],
                    clip.size[1],
                ],
                [clip.radius.max(0.0), clip.rotation_degrees.to_radians()],
            )
        }
        Some(_) => ([0.0; 4], [0.0; 2]),
        None => (NO_CLIP, [0.0; 2]),
    }
}

/// Turns `[px, py]` (screen pixels) `degrees` around `pivot`, then puts it
/// in clip space for a `width` × `height` target.
fn screen_point(
    [px, py]: [f32; 2],
    (pivot, degrees): ([f32; 2], f32),
    (width, height): (f32, f32),
) -> [f32; 2] {
    let [px, py] = if is_turned(degrees) {
        turn_point([px, py], pivot, degrees)
    } else {
        [px, py]
    };
    [px / width * 2.0 - 1.0, 1.0 - py / height * 2.0]
}

/// How far a text's outlines and shadows can reach past its box. Each copy
/// is cut to the box moved with it.
pub(super) fn text_spill(text: &UiText) -> f32 {
    let stroke = text
        .spans
        .iter()
        .filter_map(|span| span.stroke)
        .chain(text.stroke)
        .map(|stroke| stroke.width)
        .fold(0.0, f32::max);
    let shadow = text
        .shadows
        .iter()
        .map(|shadow| shadow.offset[0].abs().max(shadow.offset[1].abs()) + shadow.blur)
        .fold(0.0, f32::max);
    // Faux bold and italics draw a pixel or two outside the box's letters.
    stroke + shadow + 2.0
}

/// How far an outer shadow can reach past its rectangle.
fn shadow_spill(rectangle: &UiRectangle) -> f32 {
    match rectangle.shadow {
        Some(shadow) if !shadow.inset => {
            shadow.offset[0].abs().max(shadow.offset[1].abs())
                + shadow.spread.max(0.0)
                + shadow.blur.max(0.0)
        }
        _ => 0.0,
    }
}

fn overlaps(a: [f32; 4], b: [f32; 4]) -> bool {
    // NaN compares false, so a broken box never splits a layer.
    a[0] < b[2] && b[0] < a[2] && a[1] < b[3] && b[1] < a[3]
}

/// The quads of the rectangles in `items[range]`, in order. `lines` are
/// text decorations (underline and line-through), each drawn after the item
/// at its index in `items`; they're sorted by it.
pub(super) fn ui_vertices(
    items: &[UiDrawItem],
    range: Range<usize>,
    lines: &[(usize, UiRectangle)],
    (width, height): (u32, u32),
) -> Vec<UiVertex> {
    if width == 0 || height == 0 {
        return Vec::new();
    }
    let size = (width as f32, height as f32);
    let mut vertices = Vec::new();
    let mut lines = lines.iter().filter(|(at, _)| range.contains(at)).peekable();
    for index in range.clone() {
        if let UiDrawItem::Rectangle(rectangle) = &items[index] {
            push_rectangle(rectangle, size, &mut vertices);
        }
        while let Some((_, line)) = lines.next_if(|(at, _)| *at <= index) {
            push_rectangle(line, size, &mut vertices);
        }
    }
    vertices
}

/// The quads of `rectangles`, in order, on a `width` × `height` target.
pub(super) fn rectangle_vertices(
    rectangles: &[UiRectangle],
    (width, height): (u32, u32),
) -> Vec<UiVertex> {
    let mut vertices = Vec::new();
    if width > 0 && height > 0 {
        for rectangle in rectangles {
            push_rectangle(rectangle, (width as f32, height as f32), &mut vertices);
        }
    }
    vertices
}

fn push_rectangle(
    rectangle: &UiRectangle,
    (width, height): (f32, f32),
    vertices: &mut Vec<UiVertex>,
) {
    let UiRectangle {
        position: [x, y],
        size: [w, h],
        color: [r, g, b, alpha],
        color_end: [er, eg, eb, ea],
        border_color,
        border_width,
        border_radius,
        border_sides,
        shadow,
        clip,
        rotation_degrees,
    } = *rectangle;
    // A shadow turns around its element's middle, not its own.
    let turn = ([x + w * 0.5, y + h * 0.5], rotation_degrees);
    let (clip, clip_shape) = clip_numbers(clip);
    if clip[2] <= 0.0 || clip[3] <= 0.0 {
        return;
    }
    let sides = border_sides.unwrap_or(
        [UiBorderSide {
            width: border_width,
            color: border_color,
        }; 4],
    );
    let shadow = shadow.unwrap_or_default();
    let numbers = [
        x,
        y,
        w,
        h,
        r,
        g,
        b,
        alpha,
        border_radius,
        er,
        eg,
        eb,
        ea,
        shadow.offset[0],
        shadow.offset[1],
        shadow.blur,
        shadow.spread,
        rotation_degrees,
    ];
    if !numbers
        .into_iter()
        .chain(
            sides
                .iter()
                .flat_map(|side| side.color.into_iter().chain([side.width])),
        )
        .all(f32::is_finite)
        || w <= 0.0
        || h <= 0.0
    {
        return;
    }
    let border_shows = sides
        .iter()
        .any(|side| side.width > 0.0 && side.color[3] > 0.0);
    let kind = match rectangle.shadow {
        Some(UiShadow { inset: true, .. }) => UI_INSET_SHADOW,
        Some(_) => UI_SHADOW,
        None => UI_BOX,
    };
    if alpha <= 0.0 && (kind != UI_BOX || (ea <= 0.0 && !border_shows)) {
        return;
    }
    // An outer shadow is its box moved and grown by the spread, drawn on a
    // quad bigger by the blur on every side.
    let (x, y, w, h, radius) = if kind == UI_SHADOW {
        let blur = shadow.blur.max(0.0);
        let spread = shadow.spread.max(-w.min(h) * 0.5);
        let grow = spread + blur;
        (
            x + shadow.offset[0] - grow,
            y + shadow.offset[1] - grow,
            w + grow * 2.0,
            h + grow * 2.0,
            (border_radius.clamp(0.0, w.min(h) * 0.5) + spread).max(0.0),
        )
    } else {
        (x, y, w, h, border_radius.clamp(0.0, w.min(h) * 0.5))
    };
    if w <= 0.0 || h <= 0.0 {
        return;
    }
    let point = |px: f32, py: f32| screen_point([px, py], turn, (width, height));
    let top_left = point(x, y);
    let top_right = point(x + w, y);
    let bottom_left = point(x, y + h);
    let bottom_right = point(x + w, y + h);
    let linear = |[r, g, b, a]: [f32; 4]| {
        let color = linear_color([r, g, b]);
        [color.x, color.y, color.z, a.clamp(0.0, 1.0)]
    };
    let color = linear([r, g, b, alpha]);
    let color_end = linear([er, eg, eb, ea]);
    let half = w.min(h) * 0.5;
    let widths = sides.map(|side| side.width.clamp(0.0, half));
    let [top, right, bottom, left] = sides.map(|side| linear(side.color));
    for (position, uv) in [
        (top_left, [0.0, 0.0]),
        (bottom_left, [0.0, 1.0]),
        (top_right, [1.0, 0.0]),
        (top_right, [1.0, 0.0]),
        (bottom_left, [0.0, 1.0]),
        (bottom_right, [1.0, 1.0]),
    ] {
        vertices.push(UiVertex {
            position,
            color,
            uv,
            size: [w, h],
            border_color: top,
            border: [radius, kind],
            color_end,
            border_widths: if kind == UI_BOX { widths } else { [0.0; 4] },
            border_right: right,
            border_bottom: bottom,
            border_left: left,
            shadow: [
                shadow.offset[0],
                shadow.offset[1],
                shadow.blur.max(0.0),
                shadow.spread,
            ],
            clip,
            clip_shape,
        });
    }
}

/// The quads of `image`: one, or its nine pieces (more when they repeat)
/// when it's sliced and its texture's size is known.
pub(super) fn image_quad(image: &UiImage, (width, height): (u32, u32)) -> Vec<UiVertex> {
    if width == 0 || height == 0 {
        return Vec::new();
    }
    let [x, y] = image.position;
    let [w, h] = image.size;
    let [r, g, b, alpha] = image.tint;
    if ![
        x,
        y,
        w,
        h,
        r,
        g,
        b,
        alpha,
        image.border_radius,
        image.rotation_degrees,
    ]
    .into_iter()
    .all(f32::is_finite)
        || w <= 0.0
        || h <= 0.0
        || alpha <= 0.0
    {
        return Vec::new();
    }
    let color = linear_color([r, g, b]);
    let color = [color.x, color.y, color.z, alpha.clamp(0.0, 1.0)];
    let screen = (width as f32, height as f32);
    let texture_size = image.texture.as_ref().map(|texture| texture.size());
    let pieces = match (image.slice, texture_size) {
        (Some(slice), Some((tw, th))) if tw > 0 && th > 0 => {
            slice_pieces(&slice, [x, y, w, h], [tw as f32, th as f32])
        }
        _ => vec![([x, y, w, h], [0.0, 0.0, 1.0, 1.0])],
    };
    let radius = if image.slice.is_some() {
        0.0
    } else {
        image.border_radius.clamp(0.0, w.min(h) * 0.5)
    };
    let clip = clip_numbers(image.clip);
    if clip.0[2] <= 0.0 || clip.0[3] <= 0.0 {
        return Vec::new();
    }
    // Every piece turns around the whole picture's middle.
    let turn = ([x + w * 0.5, y + h * 0.5], image.rotation_degrees);
    let mut vertices = Vec::with_capacity(pieces.len() * 6);
    for (rect, uv) in pieces {
        push_image_piece(rect, uv, (color, radius, clip), turn, screen, &mut vertices);
    }
    vertices
}

/// The quad of turned `text`, drawn from where it was put on the sheet:
/// `slot` (left, top, width, height in the sheet's pixels), which holds
/// the text's box grown by `margin` on every side.
pub(super) fn turned_text_quad(
    text: &UiText,
    slot: [f32; 4],
    margin: f32,
    (sheet_width, sheet_height): (u32, u32),
    (width, height): (u32, u32),
) -> Vec<UiVertex> {
    let [x, y] = text.position;
    let [w, h] = text.size;
    if width == 0 || height == 0 || sheet_width == 0 || sheet_height == 0 {
        return Vec::new();
    }
    let clip = clip_numbers(text.clip);
    if clip.0[2] <= 0.0 || clip.0[3] <= 0.0 {
        return Vec::new();
    }
    let [sx, sy, sw, sh] = slot;
    let (tw, th) = (sheet_width as f32, sheet_height as f32);
    let mut vertices = Vec::with_capacity(6);
    push_image_piece(
        [x - margin, y - margin, sw, sh],
        [sx / tw, sy / th, (sx + sw) / tw, (sy + sh) / th],
        ([1.0; 4], 0.0, clip),
        ([x + w * 0.5, y + h * 0.5], text.rotation_degrees),
        (width as f32, height as f32),
        &mut vertices,
    );
    vertices
}

/// One textured quad: `[x, y, w, h]` on screen showing `[u0, v0, u1, v1]`,
/// turned `turn.1` degrees around `turn.0`.
fn push_image_piece(
    [x, y, w, h]: [f32; 4],
    [u0, v0, u1, v1]: [f32; 4],
    (color, radius, (clip, clip_shape)): ([f32; 4], f32, ([f32; 4], [f32; 2])),
    turn: ([f32; 2], f32),
    (width, height): (f32, f32),
    vertices: &mut Vec<UiVertex>,
) {
    if w <= 0.0 || h <= 0.0 {
        return;
    }
    let point = |px: f32, py: f32| screen_point([px, py], turn, (width, height));
    for (position, uv) in [
        (point(x, y), [u0, v0]),
        (point(x, y + h), [u0, v1]),
        (point(x + w, y), [u1, v0]),
        (point(x + w, y), [u1, v0]),
        (point(x, y + h), [u0, v1]),
        (point(x + w, y + h), [u1, v1]),
    ] {
        vertices.push(UiVertex {
            position,
            color,
            uv,
            size: [w, h],
            border_color: [0.0; 4],
            border: [radius, UI_BOX],
            color_end: color,
            border_widths: [0.0; 4],
            border_right: [0.0; 4],
            border_bottom: [0.0; 4],
            border_left: [0.0; 4],
            shadow: [0.0; 4],
            clip,
            clip_shape,
        });
    }
}

/// At most this many tiles across or down one piece of a repeated slice.
const MAX_TILES: usize = 256;

/// The pieces of a sliced image on `[x, y, w, h]`, each as its box on
/// screen and the part of the texture (`[u0, v0, u1, v1]`) it shows. The
/// corners are `slice.widths` big, scaled down together when they don't
/// fit, as CSS does; the edges and middle stretch, or repeat from the top
/// left at the corners' scale.
pub(super) fn slice_pieces(
    slice: &UiSlice,
    [x, y, w, h]: [f32; 4],
    [tw, th]: [f32; 2],
) -> Vec<([f32; 4], [f32; 4])> {
    let clean = |value: f32| {
        if value.is_finite() {
            value.max(0.0)
        } else {
            0.0
        }
    };
    // The cuts, in texture pixels, kept inside the texture.
    let [mut it, mut ir, mut ib, mut il] = slice.insets.map(clean);
    if it + ib > th {
        let scale = th / (it + ib);
        it *= scale;
        ib *= scale;
    }
    if il + ir > tw {
        let scale = tw / (il + ir);
        il *= scale;
        ir *= scale;
    }
    let [mut top, mut right, mut bottom, mut left] = slice.widths.map(clean);
    let fit = (w / (left + right)).min(h / (top + bottom)).min(1.0);
    if fit.is_finite() {
        [top, right, bottom, left] = [top, right, bottom, left].map(|side| side * fit);
    }
    // Across: the left column, the middle and the right column, each as
    // its span on screen and in the texture.
    let columns = [
        (x, left, 0.0, il),
        (x + left, w - left - right, il, tw - ir),
        (x + w - right, right, tw - ir, tw),
    ];
    let rows = [
        (y, top, 0.0, it),
        (y + top, h - top - bottom, it, th - ib),
        (y + h - bottom, bottom, th - ib, th),
    ];
    // How big a repeated tile is: the texture's middle at the scale of the
    // corners beside it.
    let scale_x = if it > 0.0 {
        top / it
    } else if ib > 0.0 {
        bottom / ib
    } else {
        1.0
    };
    let scale_y = if il > 0.0 {
        left / il
    } else if ir > 0.0 {
        right / ir
    } else {
        1.0
    };
    let mut pieces = Vec::new();
    for (row, &(sy, sh, v0, v1)) in rows.iter().enumerate() {
        for (column, &(sx, sw, u0, u1)) in columns.iter().enumerate() {
            if row == 1 && column == 1 && !slice.fill {
                continue;
            }
            if sw <= 0.0 || sh <= 0.0 || u1 <= u0 || v1 <= v0 {
                continue;
            }
            // Only the middle row and column repeat; corners never do.
            let tile_w = if slice.repeat && column == 1 {
                (u1 - u0) * scale_x
            } else {
                sw
            };
            let tile_h = if slice.repeat && row == 1 {
                (v1 - v0) * scale_y
            } else {
                sh
            };
            // Sampled half a texel inside each cut, so smoothing doesn't
            // bleed the piece next to it in.
            let inside = |t0: f32, t1: f32, length: f32| {
                let half = (0.5f32).min((t1 - t0) * 0.5);
                (
                    if t0 > 0.0 { t0 + half } else { t0 },
                    if t1 < length { t1 - half } else { t1 },
                )
            };
            let (u0, u1) = inside(u0, u1, tw);
            let (v0, v1) = inside(v0, v1, th);
            for (px, pw, pu0, pu1) in tiles(sx, sw, tile_w, u0, u1) {
                for (py, ph, pv0, pv1) in tiles(sy, sh, tile_h, v0, v1) {
                    pieces.push(([px, py, pw, ph], [pu0 / tw, pv0 / th, pu1 / tw, pv1 / th]));
                }
            }
        }
    }
    pieces
}

/// `length` from `start` cut into tiles `tile` long, the last one cut
/// short, each with its span of the texture from `t0` to `t1`.
fn tiles(start: f32, length: f32, tile: f32, t0: f32, t1: f32) -> Vec<(f32, f32, f32, f32)> {
    if tile.is_nan() || tile <= 0.0 || tile >= length || length / tile > MAX_TILES as f32 {
        return vec![(start, length, t0, t1)];
    }
    let mut out = Vec::new();
    let mut at = 0.0;
    while at < length - 0.001 {
        let piece = tile.min(length - at);
        out.push((start + at, piece, t0, t0 + (t1 - t0) * piece / tile));
        at += tile;
    }
    out
}

pub(super) fn glyph_color([r, g, b, a]: [f32; 4]) -> GlyphColor {
    let byte = |channel: f32| {
        (if channel.is_finite() { channel } else { 1.0 })
            .clamp(0.0, 1.0)
            .mul_add(255.0, 0.5) as u8
    };
    GlyphColor::rgba(byte(r), byte(g), byte(b), byte(a))
}

/// [`UiVertex`]'s fields, in order, as `ui.wgsl` reads them.
const UI_VERTEX_ATTRIBUTES: [wgpu::VertexAttribute; 14] = wgpu::vertex_attr_array![
    0 => Float32x2, 1 => Float32x4, 2 => Float32x2,
    3 => Float32x2, 4 => Float32x4, 5 => Float32x2, 6 => Float32x4,
    7 => Float32x4, 8 => Float32x4, 9 => Float32x4, 10 => Float32x4, 11 => Float32x4,
    12 => Float32x4, 13 => Float32x2
];

pub(super) fn create_ui_pipeline(
    device: &wgpu::Device,
    color_format: wgpu::TextureFormat,
) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::include_wgsl!("../ui.wgsl"));
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("UI overlay"),
        bind_group_layouts: &[],
        push_constant_ranges: &[],
    });
    let vertex_layout = wgpu::VertexBufferLayout {
        array_stride: size_of::<UiVertex>() as u64,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &UI_VERTEX_ATTRIBUTES,
    };
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("UI overlay"),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            compilation_options: Default::default(),
            buffers: &[vertex_layout],
        },
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: Default::default(),
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format: color_format,
                blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview: None,
        cache: None,
    })
}

/// A second UI pipeline that samples one texture. Rounded corners match
/// [`create_ui_pipeline`]; the tint is the vertex color. Also returns a
/// third, the same but for the turned-text sheet (`fs_sheet`).
pub(super) fn create_ui_image_pipeline(
    device: &wgpu::Device,
    color_format: wgpu::TextureFormat,
) -> (
    wgpu::BindGroupLayout,
    wgpu::Sampler,
    wgpu::RenderPipeline,
    wgpu::RenderPipeline,
) {
    let bind_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("UI image"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
        ],
    });
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("UI image sampler"),
        address_mode_u: wgpu::AddressMode::ClampToEdge,
        address_mode_v: wgpu::AddressMode::ClampToEdge,
        address_mode_w: wgpu::AddressMode::ClampToEdge,
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        ..Default::default()
    });
    let shader = device.create_shader_module(wgpu::include_wgsl!("../ui.wgsl"));
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("UI image"),
        bind_group_layouts: &[&bind_layout],
        push_constant_ranges: &[],
    });
    let pipeline = |entry_point: &str| {
        let vertex_layout = wgpu::VertexBufferLayout {
            array_stride: size_of::<UiVertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &UI_VERTEX_ATTRIBUTES,
        };
        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("UI image"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[vertex_layout],
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some(entry_point),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: color_format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview: None,
            cache: None,
        })
    };
    let image = pipeline("fs_image");
    let sheet = pipeline("fs_sheet");
    (bind_layout, sampler, image, sheet)
}

/// Draws the sky as a full-screen triangle at the far plane, before any item.
pub(super) fn create_sky_pipeline(
    device: &wgpu::Device,
    frame_layout: &wgpu::BindGroupLayout,
) -> wgpu::RenderPipeline {
    let shader = super::gpu::shader_module(device, "sky", super::gpu::SCENE_SHADER);
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("sky"),
        bind_group_layouts: &[frame_layout],
        push_constant_ranges: &[],
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("sky"),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_sky"),
            compilation_options: Default::default(),
            buffers: &[],
        },
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: Some(wgpu::DepthStencilState {
            format: DEPTH_FORMAT,
            depth_write_enabled: false,
            depth_compare: wgpu::CompareFunction::Always,
            stencil: Default::default(),
            bias: Default::default(),
        }),
        multisample: Default::default(),
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_sky"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format: HDR_FORMAT,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview: None,
        cache: None,
    })
}
