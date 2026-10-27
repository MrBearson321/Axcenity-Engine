//! Scene pipelines, meshes, textures and materials on the GPU.
//!
//! A caller's mesh or texture is uploaded the first time it is drawn and
//! dropped after `KEEP_UNDRAWN_FRAMES` frames without a draw. Color textures
//! are stored as bytes and sampled through an sRGB view.

use std::borrow::Cow;

use glam::Vec3;
use wgpu::util::DeviceExt;

use crate::draw_list::{MeshSource, TextureSource};
use crate::mesh::{self, MeshData, Vertex};

use super::skinning::{JOINTS_BINDING, Skinning};
use super::{
    Bounds, DEPTH_FORMAT, GpuBytes, GpuMaterial, GpuMesh, GpuTexture, InstanceRaw,
    MATERIAL_TEXTURES, MapsUniform, TARGET_DRAW_FORMAT, TARGET_SHOW_FORMAT, TEXTURE_FORMAT,
    TEXTURE_SRGB_FORMAT,
};

/// The shader for parts and models, and the shadow and sky passes: the
/// sky's colors and the shared lighting, then `shader.wgsl`.
pub(super) const SCENE_SHADER: &str = concat!(
    include_str!("../sky.wgsl"),
    include_str!("../lighting.wgsl"),
    include_str!("../shader.wgsl")
);
/// The shader for terrain chunks: the sky's colors and the shared
/// lighting, then `terrain.wgsl`.
pub(super) const TERRAIN_SHADER: &str = concat!(
    include_str!("../sky.wgsl"),
    include_str!("../lighting.wgsl"),
    include_str!("../terrain.wgsl")
);
/// The shader that bakes the sky's environment maps: the sky's colors,
/// then `environment.wgsl`.
pub(super) const ENVIRONMENT_SHADER: &str = concat!(
    include_str!("../sky.wgsl"),
    include_str!("../environment.wgsl")
);

pub(super) fn shader_module(
    device: &wgpu::Device,
    label: &str,
    source: &'static str,
) -> wgpu::ShaderModule {
    device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some(label),
        source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(source)),
    })
}

/// A pipeline that draws meshes with their instances into the HDR scene:
/// solid items, see-through ones when `transparent` is true, or terrain
/// chunks, by `shader`'s `entry_points` (vertex, fragment). `layouts` are
/// its bind groups: the frame's, then the material's. `samples` per pixel
/// must match the scene's targets. `double_sided` draws back faces too.
pub(super) fn create_pipeline(
    device: &wgpu::Device,
    shader: &wgpu::ShaderModule,
    (vertex_entry, fragment_entry): (&str, &str),
    layouts: &[&wgpu::BindGroupLayout],
    (color_format, samples): (wgpu::TextureFormat, u32),
    (transparent, double_sided): (bool, bool),
) -> wgpu::RenderPipeline {
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("scene"),
        bind_group_layouts: layouts,
        push_constant_ranges: &[],
    });

    let vertex_layout = wgpu::VertexBufferLayout {
        array_stride: size_of::<Vertex>() as u64,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &wgpu::vertex_attr_array![
            0 => Float32x3, 1 => Float32x3, 12 => Float32x2, 13 => Float32x4
        ],
    };
    // Advances once per drawn shape instead of once per corner.
    let instance_layout = wgpu::VertexBufferLayout {
        array_stride: size_of::<InstanceRaw>() as u64,
        step_mode: wgpu::VertexStepMode::Instance,
        attributes: &wgpu::vertex_attr_array![
            2 => Float32x4, 3 => Float32x4, 4 => Float32x4, 5 => Float32x4,
            6 => Float32x3, 7 => Float32x3, 8 => Float32x3,
            9 => Float32x4,
            10 => Float32x4,
            11 => Float32x4,
            14 => Uint32x2,
        ],
    };

    let blend = if transparent {
        wgpu::BlendState {
            // Mix the surface's color over what's already drawn, by its opacity.
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::SrcAlpha,
                dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                operation: wgpu::BlendOperation::Add,
            },
            // Keep the window's own alpha at 1. Some desktops would otherwise
            // show what's behind the window through see-through parts.
            alpha: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::Zero,
                dst_factor: wgpu::BlendFactor::One,
                operation: wgpu::BlendOperation::Add,
            },
        }
    } else {
        wgpu::BlendState::REPLACE
    };

    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(if transparent {
            "transparent"
        } else {
            fragment_entry
        }),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some(vertex_entry),
            compilation_options: Default::default(),
            buffers: &[vertex_layout, instance_layout],
        },
        primitive: wgpu::PrimitiveState {
            // Counter-clockwise triangles face the camera; the backs are
            // skipped unless the material is double-sided.
            front_face: wgpu::FrontFace::Ccw,
            cull_mode: (!double_sided).then_some(wgpu::Face::Back),
            ..Default::default()
        },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: DEPTH_FORMAT,
            // See-through items are hidden behind solid ones, but don't hide
            // other see-through items behind them.
            depth_write_enabled: !transparent,
            depth_compare: wgpu::CompareFunction::Less,
            stencil: Default::default(),
            bias: Default::default(),
        }),
        multisample: wgpu::MultisampleState {
            count: samples,
            ..Default::default()
        },
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some(fragment_entry),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format: color_format,
                blend: Some(blend),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview: None,
        cache: None,
    })
}

/// A square depth texture with one layer per shadow cascade, which the
/// sun's views are drawn into and the scene pass then reads: a view of the
/// whole array for reading, and one view per layer for drawing.
/// A `size`² depth array with `layers` layers: a view of all of them, for
/// the shader, and one of each, to draw into.
pub(super) fn create_shadow_views(
    device: &wgpu::Device,
    label: &str,
    size: u32,
    layers: u32,
) -> (wgpu::TextureView, Vec<wgpu::TextureView>) {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width: size,
            height: size,
            depth_or_array_layers: layers,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: DEPTH_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let array = texture.create_view(&wgpu::TextureViewDescriptor {
        label: Some(label),
        dimension: Some(wgpu::TextureViewDimension::D2Array),
        ..Default::default()
    });
    let layers = (0..layers)
        .map(|layer| {
            texture.create_view(&wgpu::TextureViewDescriptor {
                label: Some(label),
                dimension: Some(wgpu::TextureViewDimension::D2),
                base_array_layer: layer,
                array_layer_count: Some(1),
                ..Default::default()
            })
        })
        .collect();
    (array, layers)
}

/// The frame's bindings (group 0 in lighting.wgsl): its uniform, lights
/// and tiles, the sun's and spot lights' shadow maps, and the ambient
/// occlusion, with a sampler for each kind of map.
pub(super) fn create_frame_bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    [frame_buffer, light_buffer, tile_buffer, joint_buffer]: [&wgpu::Buffer; 4],
    [
        shadow_view,
        spot_shadow_view,
        occlusion_view,
        environment_view,
        sky_image_view,
    ]: [&wgpu::TextureView; 5],
    [shadow_sampler, occlusion_sampler, environment_sampler]: [&wgpu::Sampler; 3],
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("frame"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: frame_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(shadow_view),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::Sampler(shadow_sampler),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: light_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 4,
                resource: tile_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: JOINTS_BINDING,
                resource: joint_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 6,
                resource: wgpu::BindingResource::TextureView(spot_shadow_view),
            },
            wgpu::BindGroupEntry {
                binding: 7,
                resource: wgpu::BindingResource::TextureView(occlusion_view),
            },
            wgpu::BindGroupEntry {
                binding: 8,
                resource: wgpu::BindingResource::Sampler(occlusion_sampler),
            },
            wgpu::BindGroupEntry {
                binding: 10,
                resource: wgpu::BindingResource::TextureView(environment_view),
            },
            wgpu::BindGroupEntry {
                binding: 11,
                resource: wgpu::BindingResource::Sampler(environment_sampler),
            },
            wgpu::BindGroupEntry {
                binding: 12,
                resource: wgpu::BindingResource::TextureView(sky_image_view),
            },
        ],
    })
}

/// Draws solid items' depth as the sun or a spot light sees them. No
/// color, no materials.
pub(super) fn create_shadow_pipeline(
    device: &wgpu::Device,
    shadow_layout: &wgpu::BindGroupLayout,
    skinning: &Skinning,
) -> wgpu::RenderPipeline {
    let shader = shader_module(device, "shadow", SCENE_SHADER);
    // No material (group 1), but the skin (group 2) moves the corners.
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("shadow"),
        bind_group_layouts: &[shadow_layout, &skinning.empty_layout, &skinning.layout],
        push_constant_ranges: &[],
    });
    // Only the position, the model matrix and the skin matter here.
    let vertex_layout = wgpu::VertexBufferLayout {
        array_stride: size_of::<Vertex>() as u64,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &wgpu::vertex_attr_array![0 => Float32x3],
    };
    let instance_layout = wgpu::VertexBufferLayout {
        array_stride: size_of::<InstanceRaw>() as u64,
        step_mode: wgpu::VertexStepMode::Instance,
        attributes: &[
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x4,
                offset: 0,
                shader_location: 2,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x4,
                offset: 16,
                shader_location: 3,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x4,
                offset: 32,
                shader_location: 4,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x4,
                offset: 48,
                shader_location: 5,
            },
            // The skin is last, past the attributes left out.
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Uint32x2,
                offset: std::mem::offset_of!(InstanceRaw, skin) as u64,
                shader_location: 14,
            },
        ],
    };
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("shadow"),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_shadow"),
            compilation_options: Default::default(),
            buffers: &[vertex_layout, instance_layout],
        },
        primitive: wgpu::PrimitiveState {
            front_face: wgpu::FrontFace::Ccw,
            // Both sides are drawn, so thin and open shapes still cast shadows.
            cull_mode: None,
            ..Default::default()
        },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: DEPTH_FORMAT,
            depth_write_enabled: true,
            depth_compare: wgpu::CompareFunction::Less,
            stencil: Default::default(),
            // Pushes steep surfaces' depth back a little, so they don't
            // shadow themselves in stripes ("shadow acne").
            bias: wgpu::DepthBiasState {
                constant: 0,
                slope_scale: 1.5,
                clamp: 0.0,
            },
        }),
        multisample: Default::default(),
        fragment: None,
        multiview: None,
        cache: None,
    })
}

/// The scene's depth, with `samples` per pixel like its color.
/// Bytes of a `width` × `height` × `layers` target of `format` with
/// `samples` per pixel and one mip level.
pub(super) fn target_bytes(
    (width, height, layers): (u32, u32, u32),
    format: wgpu::TextureFormat,
    samples: u32,
) -> u64 {
    let texel = format
        .block_copy_size(None)
        .or_else(|| format.block_copy_size(Some(wgpu::TextureAspect::DepthOnly)))
        .unwrap_or(4);
    u64::from(width) * u64::from(height) * u64::from(layers) * u64::from(texel * samples)
}

pub(super) fn create_depth_view(
    device: &wgpu::Device,
    width: u32,
    height: u32,
    samples: u32,
) -> wgpu::TextureView {
    device
        .create_texture(&wgpu::TextureDescriptor {
            label: Some("depth"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: samples,
            dimension: wgpu::TextureDimension::D2,
            format: DEPTH_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        })
        .create_view(&wgpu::TextureViewDescriptor::default())
}

/// The texture a [`TextureRenderer`] draws into, with its two views: one to
/// draw through and one to show. Others can sample it (`TEXTURE_BINDING`),
/// and tests can copy it back (`COPY_SRC`).
///
/// The texture itself is plain `Rgba8Unorm`, which egui expects. Drawing goes
/// through an `Rgba8UnormSrgb` view of the same pixels, so the GPU encodes the
/// shader's linear colors as sRGB on the way in. (Sampling an sRGB view would
/// decode them back to linear, which egui would then show far too dark.)
pub(super) fn create_target_texture(
    device: &wgpu::Device,
    width: u32,
    height: u32,
) -> (wgpu::Texture, wgpu::TextureView, wgpu::TextureView) {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("scene texture"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: TARGET_SHOW_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_SRC,
        // Views may reinterpret the pixels in these formats too.
        view_formats: &[TARGET_DRAW_FORMAT],
    });
    let draw_view = texture.create_view(&wgpu::TextureViewDescriptor {
        format: Some(TARGET_DRAW_FORMAT),
        ..Default::default()
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    (texture, draw_view, view)
}

pub(super) fn create_instance_buffer(device: &wgpu::Device, capacity: usize) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("instances"),
        size: (capacity * size_of::<InstanceRaw>()) as u64,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

pub(super) fn upload_mesh(
    device: &wgpu::Device,
    bytes: &GpuBytes,
    name: &str,
    mesh: &MeshData,
) -> GpuMesh {
    upload_buffers(
        device,
        bytes,
        name,
        &mesh.vertices,
        bytemuck::cast_slice(&mesh.indices),
        mesh.indices.len(),
        wgpu::IndexFormat::Uint16,
    )
}

/// Uploads a caller's mesh, or `None` (with a warning) if its data doesn't
/// fit together, such as an index past the last corner.
pub(super) fn upload_custom_mesh(
    device: &wgpu::Device,
    bytes: &GpuBytes,
    skinning: &Skinning,
    source: &dyn MeshSource,
) -> Option<GpuMesh> {
    let vertices = custom_vertices(source)?;
    let indices = source.indices();
    let mut mesh = upload_buffers(
        device,
        bytes,
        "mesh",
        &vertices,
        bytemuck::cast_slice(indices),
        indices.len(),
        wgpu::IndexFormat::Uint32,
    );
    mesh.version = source.version();
    mesh.skin = skinning.upload(device, bytes, source);
    Some(mesh)
}

/// Brings `mesh` up to date with a new [`MeshSource::version`] of the same
/// mesh: its corners are written over the old ones, or, if their number
/// changed, the whole mesh is uploaded again. Broken data keeps the old.
pub(super) fn update_custom_mesh(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    (bytes, skinning): (&GpuBytes, &Skinning),
    mesh: &mut GpuMesh,
    source: &dyn MeshSource,
) {
    mesh.version = source.version();
    let Some(vertices) = custom_vertices(source) else {
        return;
    };
    let corners: &[u8] = bytemuck::cast_slice(&vertices);
    if corners.len() as u64 == mesh.vertices.size()
        && source.indices().len() == mesh.index_count as usize
    {
        queue.write_buffer(&mesh.vertices, 0, corners);
        mesh.bounds = vertex_bounds(&vertices);
    } else if let Some(fresh) = upload_custom_mesh(device, bytes, skinning, source) {
        *mesh = fresh;
    }
}

/// A caller's mesh as the GPU's corners, or `None` (with a warning) if
/// its data doesn't fit together.
fn custom_vertices(source: &dyn MeshSource) -> Option<Vec<Vertex>> {
    let positions = source.positions();
    let normals = source.normals();
    let uvs = source.uvs();
    let tangents = source.tangents();
    let indices = source.indices();
    let count = positions.len();
    let fits = count > 0
        && normals.len() == count
        && (uvs.is_empty() || uvs.len() == count)
        && (tangents.is_empty() || tangents.len() == count)
        && !indices.is_empty()
        && indices.len().is_multiple_of(3)
        && indices.iter().all(|&index| (index as usize) < count);
    if !fits {
        log::warn!("mesh #{} has broken data, so it isn't drawn", source.key());
        return None;
    }
    Some(
        (0..count)
            .map(|i| Vertex {
                position: positions[i],
                normal: normals[i],
                uv: uvs.get(i).copied().unwrap_or_default(),
                tangent: tangents.get(i).copied().unwrap_or(mesh::NO_TANGENT),
            })
            .collect(),
    )
}

fn vertex_bounds(vertices: &[Vertex]) -> Bounds {
    Bounds::around(vertices.iter().map(|vertex| Vec3::from(vertex.position)))
}

pub(super) fn upload_buffers(
    device: &wgpu::Device,
    bytes: &GpuBytes,
    name: &str,
    vertices: &[Vertex],
    indices: &[u8],
    index_count: usize,
    index_format: wgpu::IndexFormat,
) -> GpuMesh {
    let corners = vertices;
    let vertices = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some(name),
        contents: bytemuck::cast_slice(corners),
        // COPY_DST so a posed mesh's corners can be rewritten in place.
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
    });
    let indices = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some(name),
        contents: indices,
        usage: wgpu::BufferUsages::INDEX,
    });
    GpuMesh {
        bounds: vertex_bounds(corners),
        _bytes: bytes.count(vertices.size() + indices.size()),
        vertices,
        indices,
        index_count: index_count as u32,
        index_format,
        version: 0,
        skin: None,
    }
}

/// Uploads a caller's texture without its `skipped_levels` largest mip
/// levels, or any wider or taller than `max_size`, and then without
/// `dropped` more (see [`create_texture`]), or `None` (with a warning) if
/// it has no pixels or the wrong number of them.
pub(super) fn upload_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    bytes: &GpuBytes,
    source: &dyn TextureSource,
    (skipped_levels, max_size): (u32, u32),
    dropped: u32,
) -> Option<GpuTexture> {
    let (width, height) = source.size();
    let rgba = source.rgba();
    if width == 0 || height == 0 || rgba.len() != width as usize * height as usize * 4 {
        log::warn!(
            "texture #{} has broken data, so it isn't drawn",
            source.key()
        );
        return None;
    }
    Some(create_texture(
        device,
        queue,
        bytes,
        (width, height, rgba),
        (skipped_levels, max_size),
        dropped,
    ))
}

/// Binds a material's `textures` (in [`MATERIAL_TEXTURES`] order) and
/// `maps` for the shader. Colors are read through sRGB views, the rest as
/// plain values.
pub(super) fn create_material(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    sampler: &wgpu::Sampler,
    textures: [&GpuTexture; MATERIAL_TEXTURES],
    maps: MapsUniform,
) -> GpuMaterial {
    let maps = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("material maps"),
        contents: bytemuck::bytes_of(&maps),
        usage: wgpu::BufferUsages::UNIFORM,
    });
    let [
        base,
        normal,
        metalness,
        roughness,
        occlusion,
        emissive,
        opacity,
        specular,
    ] = textures;
    let view = |binding, view| wgpu::BindGroupEntry {
        binding,
        resource: wgpu::BindingResource::TextureView(view),
    };
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("material"),
        layout,
        entries: &[
            view(0, &base.srgb),
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: maps.as_entire_binding(),
            },
            view(3, &normal.linear),
            view(4, &metalness.linear),
            view(5, &roughness.linear),
            view(6, &occlusion.linear),
            view(7, &emissive.srgb),
            view(8, &opacity.linear),
            view(9, &specular.linear),
        ],
    });
    GpuMaterial {
        bind_group,
        _maps: maps,
    }
}

/// Uploads RGBA pixels (width, height, pixels) with a full set of mipmaps
/// (smaller copies, so far away surfaces don't shimmer), with an sRGB and a
/// plain view. The `skipped_levels` largest are left out (the texture
/// quality), and so are any wider or taller than `max_size` (the GPU's
/// largest), so a huge texture still shows, just less sharp. What's left is
/// the texture's base size, and `dropped` more levels are left out for the
/// texture budget.
pub(super) fn create_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    bytes: &GpuBytes,
    (width, height, rgba): (u32, u32, &[u8]),
    (skipped_levels, max_size): (u32, u32),
    dropped: u32,
) -> GpuTexture {
    let mut levels = mip_levels(width, height, rgba);
    let max = max_size.min(device.limits().max_texture_dimension_2d);
    let too_big = levels
        .iter()
        .take_while(|(width, height, _)| *width > max || *height > max)
        .count();
    // The last level is 1×1, so one is always left.
    let skipped = too_big.max(skipped_levels as usize).min(levels.len() - 1);
    levels.drain(..skipped);
    let base = (levels[0].0, levels[0].1, levels.len() as u32);
    let dropped = dropped.min(base.2 - 1);
    levels.drain(..dropped as usize);
    let texture = empty_texture(device, base, dropped);
    for (level, (width, height, pixels)) in levels.iter().enumerate() {
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: level as u32,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            pixels,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(width * 4),
                rows_per_image: Some(*height),
            },
            wgpu::Extent3d {
                width: *width,
                height: *height,
                depth_or_array_layers: 1,
            },
        );
    }
    finish_texture(texture, bytes, base, dropped)
}

/// Records copying `old` without its `more` largest levels into a new
/// texture, for the texture budget: the smaller levels are already on the
/// GPU, so nothing is read from the caller. At least one level is kept.
pub(super) fn shrink_texture(
    device: &wgpu::Device,
    encoder: &mut wgpu::CommandEncoder,
    bytes: &GpuBytes,
    old: &GpuTexture,
    more: u32,
) -> GpuTexture {
    let more = more.min(old.texture.mip_level_count() - 1);
    let dropped = old.dropped + more;
    let texture = empty_texture(device, old.base, dropped);
    let at = |texture, mip_level| wgpu::TexelCopyTextureInfo {
        texture,
        mip_level,
        origin: wgpu::Origin3d::ZERO,
        aspect: wgpu::TextureAspect::All,
    };
    for level in 0..texture.mip_level_count() {
        let (width, height) = level_size(old.base, dropped + level);
        encoder.copy_texture_to_texture(
            at(&old.texture, level + more),
            at(&texture, level),
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
    }
    finish_texture(texture, bytes, old.base, dropped)
}

/// The width and height of mip `level` of a texture of `base` size (see
/// [`GpuTexture::base`]): each level is half the last, at least 1.
pub(super) fn level_size((width, height, _): (u32, u32, u32), level: u32) -> (u32, u32) {
    ((width >> level).max(1), (height >> level).max(1))
}

/// The bytes of a texture of `base` size without its `dropped` largest
/// levels (at least one is always kept).
pub(super) fn texture_bytes(base: (u32, u32, u32), dropped: u32) -> u64 {
    (dropped.min(base.2 - 1)..base.2)
        .map(|level| {
            let (width, height) = level_size(base, level);
            u64::from(width) * u64::from(height) * 4
        })
        .sum()
}

/// A material texture of `base` size without its `dropped` largest levels,
/// with nothing in it yet.
fn empty_texture(device: &wgpu::Device, base: (u32, u32, u32), dropped: u32) -> wgpu::Texture {
    let (width, height) = level_size(base, dropped);
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("material texture"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: base.2 - dropped,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: TEXTURE_FORMAT,
        // Copied from when the texture budget shrinks it.
        usage: wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_DST
            | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[TEXTURE_SRGB_FORMAT],
    })
}

/// Makes `texture`'s two views and counts its bytes.
fn finish_texture(
    texture: wgpu::Texture,
    bytes: &GpuBytes,
    base: (u32, u32, u32),
    dropped: u32,
) -> GpuTexture {
    let linear = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let srgb = texture.create_view(&wgpu::TextureViewDescriptor {
        format: Some(TEXTURE_SRGB_FORMAT),
        ..Default::default()
    });
    GpuTexture {
        texture,
        srgb,
        linear,
        base,
        dropped,
        bytes: bytes.count(texture_bytes(base, dropped)),
    }
}

/// The image and each half-size copy of it, down to 1×1, as (width,
/// height, pixels). `Cow` lets the first be the caller's pixels, uncopied.
pub(super) fn mip_levels(width: u32, height: u32, rgba: &[u8]) -> Vec<(u32, u32, Cow<'_, [u8]>)> {
    let mut levels = vec![(width, height, Cow::Borrowed(rgba))];
    loop {
        let (width, height, pixels) = levels.last().expect("starts with one");
        if *width == 1 && *height == 1 {
            return levels;
        }
        let half = half_size(*width, *height, pixels);
        levels.push(half);
    }
}

/// An image at half the size (at least 1 pixel), each pixel the average of
/// the 2×2 it covers. Averaging the sRGB values rather than linear light is
/// slightly off, but not visibly for textures.
fn half_size(width: u32, height: u32, rgba: &[u8]) -> (u32, u32, Cow<'static, [u8]>) {
    let (half_width, half_height) = ((width / 2).max(1), (height / 2).max(1));
    let mut half = Vec::with_capacity(half_width as usize * half_height as usize * 4);
    for y in 0..half_height {
        for x in 0..half_width {
            let mut sum = [0u32; 4];
            for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                // An odd last row or column is averaged with itself.
                let from_x = (x * 2 + dx).min(width - 1);
                let from_y = (y * 2 + dy).min(height - 1);
                let at = (from_y as usize * width as usize + from_x as usize) * 4;
                for (channel, total) in sum.iter_mut().enumerate() {
                    *total += u32::from(rgba[at + channel]);
                }
            }
            half.extend(sum.map(|total| ((total + 2) / 4) as u8));
        }
    }
    (half_width, half_height, Cow::Owned(half))
}

/// Returns the size to draw at, or `None` when the window has no area
/// (minimized windows report 0×0, and a GPU surface can't be zero-sized).
pub(super) fn drawable_size(width: u32, height: u32) -> Option<(u32, u32)> {
    (width > 0 && height > 0).then_some((width, height))
}
