//! Screen-space rectangles, images and the UI and sky pipelines.
//!
//! Rectangle vertices are pixels from the top left. Glyphs are drawn by glyphon
//! inside `ScenePipeline`; this module builds the quads and the pipelines
//! (`ui.wgsl`, and the sky entry of `shader.wgsl`, after `lighting.wgsl`).

use std::ops::Range;

use glyphon::Color as GlyphColor;

use crate::draw_list::{UiDrawItem, UiImage, UiRectangle, UiText};

use super::{DEPTH_FORMAT, HDR_FORMAT, UiVertex};

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
/// and the box it can draw in: left, top, right, bottom.
fn kind_and_bounds(item: &UiDrawItem) -> (u8, [f32; 4]) {
    let (kind, [x, y], [w, h], margin) = match item {
        UiDrawItem::Rectangle(rectangle) => (0, rectangle.position, rectangle.size, 0.0),
        UiDrawItem::Image(image) => (1, image.position, image.size, 0.0),
        UiDrawItem::Text(text) => (2, text.position, text.size, text_spill(text)),
    };
    (
        kind,
        [x - margin, y - margin, x + w + margin, y + h + margin],
    )
}

/// How far a text's outlines and shadows can reach past its box. Each copy
/// is cut to the box moved with it.
fn text_spill(text: &UiText) -> f32 {
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
        border_color: [br, bg, bb, ba],
        border_width,
        border_radius,
    } = rectangle;
    {
        if ![
            x,
            y,
            w,
            h,
            r,
            g,
            b,
            alpha,
            br,
            bg,
            bb,
            ba,
            border_width,
            border_radius,
            er,
            eg,
            eb,
            ea,
        ]
        .into_iter()
        .all(|component| component.is_finite())
            || *w <= 0.0
            || *h <= 0.0
            || (*alpha <= 0.0 && *ea <= 0.0 && (*ba <= 0.0 || *border_width <= 0.0))
        {
            return;
        }
        let point = |px: f32, py: f32| [px / width * 2.0 - 1.0, 1.0 - py / height * 2.0];
        let top_left = point(*x, *y);
        let top_right = point(*x + *w, *y);
        let bottom_left = point(*x, *y + *h);
        let bottom_right = point(*x + *w, *y + *h);
        let color = linear_color([*r, *g, *b]);
        let color = [color.x, color.y, color.z, alpha.clamp(0.0, 1.0)];
        let border_color = linear_color([*br, *bg, *bb]);
        let border_color = [
            border_color.x,
            border_color.y,
            border_color.z,
            ba.clamp(0.0, 1.0),
        ];
        let end = linear_color([*er, *eg, *eb]);
        let color_end = [end.x, end.y, end.z, ea.clamp(0.0, 1.0)];
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
                size: [*w, *h],
                border_color,
                border: [
                    border_radius.clamp(0.0, w.min(*h) * 0.5),
                    border_width.clamp(0.0, w.min(*h) * 0.5),
                ],
                color_end,
            });
        }
    }
}

pub(super) fn image_quad(image: &UiImage, (width, height): (u32, u32)) -> Vec<UiVertex> {
    if width == 0 || height == 0 {
        return Vec::new();
    }
    let [x, y] = image.position;
    let [w, h] = image.size;
    let [r, g, b, alpha] = image.tint;
    if ![x, y, w, h, r, g, b, alpha, image.border_radius]
        .into_iter()
        .all(f32::is_finite)
        || w <= 0.0
        || h <= 0.0
        || alpha <= 0.0
    {
        return Vec::new();
    }
    let (width, height) = (width as f32, height as f32);
    let point = |px: f32, py: f32| [px / width * 2.0 - 1.0, 1.0 - py / height * 2.0];
    let color = linear_color([r, g, b]);
    let color = [color.x, color.y, color.z, alpha.clamp(0.0, 1.0)];
    let radius = image.border_radius.clamp(0.0, w.min(h) * 0.5);
    [
        (point(x, y), [0.0, 0.0]),
        (point(x, y + h), [0.0, 1.0]),
        (point(x + w, y), [1.0, 0.0]),
        (point(x + w, y), [1.0, 0.0]),
        (point(x, y + h), [0.0, 1.0]),
        (point(x + w, y + h), [1.0, 1.0]),
    ]
    .into_iter()
    .map(|(position, uv)| UiVertex {
        position,
        color,
        uv,
        size: [w, h],
        border_color: [0.0; 4],
        border: [radius, 0.0],
        color_end: color,
    })
    .collect()
}

pub(super) fn glyph_color([r, g, b, a]: [f32; 4]) -> GlyphColor {
    let byte = |channel: f32| {
        (if channel.is_finite() { channel } else { 1.0 })
            .clamp(0.0, 1.0)
            .mul_add(255.0, 0.5) as u8
    };
    GlyphColor::rgba(byte(r), byte(g), byte(b), byte(a))
}

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
        attributes: &wgpu::vertex_attr_array![
            0 => Float32x2, 1 => Float32x4, 2 => Float32x2,
            3 => Float32x2, 4 => Float32x4, 5 => Float32x2, 6 => Float32x4
        ],
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
/// [`create_ui_pipeline`]; the tint is the vertex color.
pub(super) fn create_ui_image_pipeline(
    device: &wgpu::Device,
    color_format: wgpu::TextureFormat,
) -> (wgpu::BindGroupLayout, wgpu::Sampler, wgpu::RenderPipeline) {
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
    let vertex_layout = wgpu::VertexBufferLayout {
        array_stride: size_of::<UiVertex>() as u64,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &wgpu::vertex_attr_array![
            0 => Float32x2, 1 => Float32x4, 2 => Float32x2,
            3 => Float32x2, 4 => Float32x4, 5 => Float32x2, 6 => Float32x4
        ],
    };
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
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
            entry_point: Some("fs_image"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format: color_format,
                blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview: None,
        cache: None,
    });
    (bind_layout, sampler, pipeline)
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
