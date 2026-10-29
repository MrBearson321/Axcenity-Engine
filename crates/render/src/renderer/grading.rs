//! Color grading (`DrawList::grading`), drawn by the composite pass in
//! post.wgsl after tone mapping: the values ride in one uniform, and a
//! lookup table in a 3D texture that exists only while one is used.

use bytemuck::{Pod, Zeroable};

use crate::{ColorGrading, ColorLookup, MAX_PALETTE_COLORS};

/// Color grading for the shader. Must match `Grading` in post.wgsl.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(super) struct GradingUniform {
    /// Brightness, contrast, saturation and posterize levels (0 none).
    values: [f32; 4],
    /// The tint (sRGB), and 1 if grading is on.
    tint: [f32; 4],
    /// The lookup table's size (0 none) and how many palette colors.
    counts: [f32; 4],
    /// sRGB.
    palette: [[f32; 4]; MAX_PALETTE_COLORS],
}

/// `grading` as the shader reads it, with numbers that aren't numbers
/// changing nothing. `lookup_size` is the table bound (0 for none).
pub(super) fn grading_uniform(grading: Option<&ColorGrading>, lookup_size: u32) -> GradingUniform {
    let mut uniform = GradingUniform::zeroed();
    let Some(grading) = grading else {
        return uniform;
    };
    let number = |value: f32| if value.is_finite() { value } else { 0.0 };
    let tint = grading
        .tint
        .map(|c| if c.is_finite() { c.max(0.0) } else { 1.0 });
    let palette = &grading.palette[..grading.palette.len().min(MAX_PALETTE_COLORS)];
    uniform.values = [
        number(grading.brightness),
        number(grading.contrast).max(-1.0),
        number(grading.saturation).max(-1.0),
        if grading.posterize >= 2 {
            grading.posterize.min(256) as f32
        } else {
            0.0
        },
    ];
    uniform.tint = [tint[0], tint[1], tint[2], 1.0];
    uniform.counts = [lookup_size as f32, palette.len() as f32, 0.0, 0.0];
    for (slot, color) in uniform.palette.iter_mut().zip(palette) {
        *slot = [color[0], color[1], color[2], 0.0];
    }
    uniform
}

/// Color grading's bind group for the composite pass, and the lookup table
/// it shows.
pub(super) struct GradingPass {
    pub(super) layout: wgpu::BindGroupLayout,
    uniform: wgpu::Buffer,
    /// Shows a 1×1×1 table, never read, while no table is used.
    without_lookup: wgpu::BindGroup,
    /// The table in use (by its key) and the bind group showing it.
    lookup: Option<(u64, u32, wgpu::BindGroup)>,
}

impl GradingPass {
    pub(super) fn new(device: &wgpu::Device) -> Self {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("color grading"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    // Read texel by texel and blended in the shader, so a
                    // 32-bit float table needs no filtering.
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D3,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("color grading"),
            size: size_of::<GradingUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let placeholder = lookup_texture(device, 1);
        let without_lookup = group(device, &layout, &uniform, &placeholder);
        Self {
            layout,
            uniform,
            without_lookup,
            lookup: None,
        }
    }

    /// Writes `grading` for this frame's composite pass, uploading its
    /// lookup table if it's new and freeing the last one if it's unused.
    pub(super) fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        grading: Option<&ColorGrading>,
    ) {
        let table = grading
            .and_then(|grading| grading.lookup.as_ref())
            .filter(|table| usable(device, table));
        match table {
            None => self.lookup = None,
            Some(table)
                if self
                    .lookup
                    .as_ref()
                    .is_some_and(|(key, ..)| *key == table.key) => {}
            Some(table) => {
                let texture = lookup_texture(device, table.size);
                let texels: Vec<[f32; 4]> = table
                    .rgb
                    .as_chunks::<3>()
                    .0
                    .iter()
                    .map(|&[r, g, b]| [r, g, b, 1.0])
                    .collect();
                queue.write_texture(
                    texture.as_image_copy(),
                    bytemuck::cast_slice(&texels),
                    wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(table.size * 16),
                        rows_per_image: Some(table.size),
                    },
                    texture.size(),
                );
                let group = group(device, &self.layout, &self.uniform, &texture);
                self.lookup = Some((table.key, table.size, group));
            }
        }
        let size = self.lookup.as_ref().map_or(0, |&(_, size, _)| size);
        queue.write_buffer(
            &self.uniform,
            0,
            bytemuck::bytes_of(&grading_uniform(grading, size)),
        );
    }

    pub(super) fn group(&self) -> &wgpu::BindGroup {
        self.lookup
            .as_ref()
            .map_or(&self.without_lookup, |(_, _, group)| group)
    }

    /// Whether a lookup table's texture is alive.
    #[cfg(test)]
    pub(super) fn has_lookup(&self) -> bool {
        self.lookup.is_some()
    }
}

/// Whether `table` is whole and fits the GPU. One that isn't is left out.
fn usable(device: &wgpu::Device, table: &ColorLookup) -> bool {
    let size = table.size as usize;
    table.size >= 2
        && table.size <= device.limits().max_texture_dimension_3d
        && table.rgb.len() == size * size * size * 3
}

fn lookup_texture(device: &wgpu::Device, size: u32) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("color lookup table"),
        size: wgpu::Extent3d {
            width: size,
            height: size,
            depth_or_array_layers: size,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D3,
        format: wgpu::TextureFormat::Rgba32Float,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    })
}

fn group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    uniform: &wgpu::Buffer,
    texture: &wgpu::Texture,
) -> wgpu::BindGroup {
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("color grading"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: uniform.as_entire_binding(),
            },
        ],
    })
}
