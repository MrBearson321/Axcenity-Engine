//! Ray traced cut-outs: what lets rays through the holes of a cut-out item
//! (a leaf card's, a fence's, anything with an `alpha_cutoff`), so ray
//! traced shadows fall in the shape of the leaves rather than as solid
//! cards (`docs/rendering/ray-tracing.md`).
//!
//! A cut-out mesh's BLAS is built without the opaque flag
//! (`ray_tracing.rs`), so a ray query that doesn't force opacity sees each
//! of its triangles as a candidate and confirms it only where the texture
//! isn't cut out there (`cut_out_covers` in ray_shadows.wgsl). For that
//! the shaders read, by the TLAS slot a hit is in (its custom data):
//!
//! - the slot's [`CutOutRaw`]: its mesh's first triangle in the shared
//!   texture coordinate buffer, its texture's alpha layer and cutoff, and
//!   the material's texture coordinate turn, scale and offset;
//! - the mesh's texture coordinates, three a triangle, copied on the GPU
//!   from its vertex and index buffers once, when its BLAS is built
//!   (`ray_cut_out_uvs.wgsl`);
//! - the texture's alpha, shrunk to [`ALPHA_SIZE`] texels a side on the CPU
//!   from the base color texture, one layer of an array per texture.
//!
//! Items whose texture coordinates are tiled by their size (`uv_tile`) and
//! cut-outs without a texture are traced as solid, as before.

use bytemuck::{Pod, Zeroable};
use rustc_hash::FxHashMap;
use wgpu::util::DeviceExt;

use super::{GpuMesh, MeshKey};
use crate::draw_list::{DrawItem, Material};

/// How many texels a side each texture's alpha is kept at.
pub(super) const ALPHA_SIZE: u32 = 512;
const ALPHA_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::R8Unorm;

/// One TLAS slot's cut-out, as the shaders read it (`CutOut` in
/// ray_shadows.wgsl). All zeros for a slot without holes.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Pod, Zeroable)]
pub(super) struct CutOutRaw {
    /// x: its mesh's first triangle in the texture coordinate buffer, y:
    /// its alpha's layer plus 1 (0: no holes, every hit counts), z: the
    /// alpha cutoff's bits, w: unused.
    mesh: [u32; 4],
    /// The columns of the material's texture coordinate matrix
    /// (`lighting::uv_matrix`).
    uv_matrix: [f32; 4],
    /// xy: the material's texture coordinate offset.
    uv_offset: [f32; 4],
}

/// Must match `Extract` in ray_cut_out_uvs.wgsl.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct ExtractUniform {
    mesh: [u32; 4],
}

/// The cut-outs' tables on the GPU, kept with the ray traced scene.
pub(super) struct CutOuts {
    /// A [`CutOutRaw`] a TLAS slot.
    pub(super) info: wgpu::Buffer,
    info_capacity: usize,
    written: Vec<CutOutRaw>,
    /// Every cut-out mesh's texture coordinates, three a triangle.
    pub(super) uvs: wgpu::Buffer,
    uv_capacity: u32,
    next_triangle: u32,
    /// Each cut-out mesh's (and version's) first triangle in `uvs`.
    meshes: FxHashMap<(MeshKey, u64), u32>,
    alpha: wgpu::Texture,
    pub(super) alpha_view: wgpu::TextureView,
    alpha_capacity: u32,
    /// Each layer's alpha, kept to copy into a bigger array.
    layers: Vec<Vec<u8>>,
    /// Each base color texture's layer, by its key.
    textures: FxHashMap<u64, u32>,
    pub(super) sampler: wgpu::Sampler,
    extract: wgpu::ComputePipeline,
    extract_layout: wgpu::BindGroupLayout,
}

impl CutOuts {
    pub(super) fn new(device: &wgpu::Device) -> Self {
        let info_capacity = 256;
        let uv_capacity = 4096;
        let alpha_capacity = 4;
        let alpha = alpha_array(device, alpha_capacity);
        let storage = |binding, read_only| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Storage { read_only },
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        };
        let extract_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("ray traced cut-outs: texture coordinates"),
            entries: &[
                storage(0, true),
                storage(1, true),
                storage(2, false),
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let shader = super::gpu::shader_module(
            device,
            "ray traced cut-outs",
            include_str!("../ray_cut_out_uvs.wgsl"),
        );
        let extract = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("ray traced cut-outs: texture coordinates"),
            layout: Some(
                &device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some("ray traced cut-outs: texture coordinates"),
                    bind_group_layouts: &[Some(&extract_layout)],
                    immediate_size: 0,
                }),
            ),
            module: &shader,
            entry_point: Some("cs_extract"),
            compilation_options: Default::default(),
            cache: None,
        });
        Self {
            info: info_buffer(device, info_capacity),
            info_capacity,
            written: Vec::new(),
            uvs: uv_buffer(device, uv_capacity),
            uv_capacity,
            next_triangle: 0,
            meshes: FxHashMap::default(),
            alpha_view: alpha_view(&alpha),
            alpha,
            alpha_capacity,
            layers: Vec::new(),
            textures: FxHashMap::default(),
            sampler: device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some("ray traced cut-outs"),
                address_mode_u: wgpu::AddressMode::Repeat,
                address_mode_v: wgpu::AddressMode::Repeat,
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                ..Default::default()
            }),
            extract,
            extract_layout,
        }
    }

    /// Records the copy of `gpu`'s texture coordinates (the mesh `key`'s)
    /// into the shared buffer, if they aren't there yet, growing it to fit.
    pub(super) fn add_mesh(
        &mut self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        key: MeshKey,
        gpu: &GpuMesh,
    ) {
        if self.meshes.contains_key(&(key, gpu.version)) {
            return;
        }
        let triangles = gpu.index_count / 3;
        let first = self.next_triangle;
        let end = first + triangles;
        if end > self.uv_capacity {
            let capacity = end.next_power_of_two().max(self.uv_capacity * 2);
            let bigger = uv_buffer(device, capacity);
            encoder.copy_buffer_to_buffer(&self.uvs, 0, &bigger, 0, self.uvs.size());
            self.uvs = bigger;
            self.uv_capacity = capacity;
        }
        self.next_triangle = end;
        self.meshes.insert((key, gpu.version), first);
        if triangles == 0 {
            return;
        }
        let wide = gpu.index_format == wgpu::IndexFormat::Uint32;
        let uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("ray traced cut-outs: texture coordinates"),
            contents: bytemuck::bytes_of(&ExtractUniform {
                mesh: [first, triangles, u32::from(wide), 0],
            }),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("ray traced cut-outs: texture coordinates"),
            layout: &self.extract_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: gpu.vertices.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: gpu.indices.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: self.uvs.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: uniform.as_entire_binding(),
                },
            ],
        });
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("ray traced cut-outs: texture coordinates"),
            timestamp_writes: None,
        });
        pass.set_pipeline(&self.extract);
        pass.set_bind_group(0, &group, &[]);
        pass.dispatch_workgroups((triangles * 3).div_ceil(64), 1, 1);
    }

    /// What a slot holding `item` (drawn with the mesh `key`, at `version`)
    /// tells the shaders: its holes, or nothing for an item traced as solid.
    pub(super) fn slot(
        &mut self,
        (device, queue): (&wgpu::Device, &wgpu::Queue),
        item: &DrawItem,
        key: MeshKey,
        version: u64,
    ) -> CutOutRaw {
        let material = &item.material;
        let tiled = item.uv_tile.is_finite() && item.uv_tile > 0.0;
        let (Some(&first), Some(texture)) = (
            self.meshes.get(&(key, version)),
            material.base_color_texture.as_ref(),
        ) else {
            return CutOutRaw::default();
        };
        if !has_holes(material) || tiled {
            return CutOutRaw::default();
        }
        let layer = match self.textures.get(&texture.key()) {
            Some(&layer) => layer,
            None => {
                let (width, height) = texture.size();
                let alpha = shrunk_alpha(texture.rgba(), width, height);
                let layer = self.layers.len() as u32;
                if layer >= self.alpha_capacity {
                    self.grow_alpha(device, queue, (layer + 1).next_power_of_two());
                }
                write_layer(queue, &self.alpha, layer, &alpha);
                self.layers.push(alpha);
                self.textures.insert(texture.key(), layer);
                layer
            }
        };
        let cutoff = material.alpha_cutoff.clamp(0.0, 1.0);
        let [ox, oy] = material.uv_offset;
        CutOutRaw {
            mesh: [first, layer + 1, cutoff.to_bits(), 0],
            uv_matrix: super::lighting::uv_matrix(material),
            uv_offset: [ox, oy, 0.0, 0.0],
        }
    }

    /// Writes `raw` (one a TLAS slot) if it changed, growing the buffer.
    pub(super) fn write(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        raw: Vec<CutOutRaw>,
    ) {
        if raw.len() > self.info_capacity {
            self.info_capacity = raw.len().next_power_of_two();
            self.info = info_buffer(device, self.info_capacity);
            self.written.clear();
        }
        if raw != self.written && !raw.is_empty() {
            queue.write_buffer(&self.info, 0, bytemuck::cast_slice(&raw));
        }
        self.written = raw;
    }

    /// Makes the alpha array `capacity` layers deep, with the layers so far.
    fn grow_alpha(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, capacity: u32) {
        self.alpha = alpha_array(device, capacity);
        self.alpha_view = alpha_view(&self.alpha);
        self.alpha_capacity = capacity;
        for (layer, alpha) in self.layers.iter().enumerate() {
            write_layer(queue, &self.alpha, layer as u32, alpha);
        }
    }
}

/// Whether `material` cuts holes rays should pass through.
pub(super) fn has_holes(material: &Material) -> bool {
    material.alpha_cutoff > 0.0 && material.opacity >= 1.0 && material.water.is_none()
}

/// The alpha of `rgba` (`width` × `height`), averaged into
/// [`ALPHA_SIZE`] squares a side (each the mean of the pixels it covers,
/// or the nearest one for a smaller texture).
fn shrunk_alpha(rgba: &[u8], width: u32, height: u32) -> Vec<u8> {
    let size = ALPHA_SIZE as usize;
    let (width, height) = (width.max(1) as usize, height.max(1) as usize);
    if rgba.len() < width * height * 4 {
        return vec![255; size * size];
    }
    let mut out = vec![0u8; size * size];
    for ty in 0..size {
        let y0 = ty * height / size;
        let y1 = ((ty + 1) * height / size).max(y0 + 1).min(height);
        for tx in 0..size {
            let x0 = tx * width / size;
            let x1 = ((tx + 1) * width / size).max(x0 + 1).min(width);
            let mut sum = 0u32;
            for y in y0..y1 {
                for x in x0..x1 {
                    sum += u32::from(rgba[(y * width + x) * 4 + 3]);
                }
            }
            let count = ((y1 - y0) * (x1 - x0)) as u32;
            out[ty * size + tx] = (sum / count.max(1)) as u8;
        }
    }
    out
}

fn write_layer(queue: &wgpu::Queue, texture: &wgpu::Texture, layer: u32, alpha: &[u8]) {
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture,
            mip_level: 0,
            origin: wgpu::Origin3d {
                x: 0,
                y: 0,
                z: layer,
            },
            aspect: wgpu::TextureAspect::All,
        },
        alpha,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(ALPHA_SIZE),
            rows_per_image: Some(ALPHA_SIZE),
        },
        wgpu::Extent3d {
            width: ALPHA_SIZE,
            height: ALPHA_SIZE,
            depth_or_array_layers: 1,
        },
    );
}

fn alpha_array(device: &wgpu::Device, layers: u32) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("ray traced cut-outs: alpha"),
        size: wgpu::Extent3d {
            width: ALPHA_SIZE,
            height: ALPHA_SIZE,
            depth_or_array_layers: layers,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: ALPHA_FORMAT,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    })
}

fn alpha_view(texture: &wgpu::Texture) -> wgpu::TextureView {
    texture.create_view(&wgpu::TextureViewDescriptor {
        dimension: Some(wgpu::TextureViewDimension::D2Array),
        ..Default::default()
    })
}

fn info_buffer(device: &wgpu::Device, slots: usize) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("ray traced cut-outs"),
        size: (slots.max(1) * size_of::<CutOutRaw>()) as u64,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

fn uv_buffer(device: &wgpu::Device, triangles: u32) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("ray traced cut-outs: texture coordinates"),
        // Three corners of two floats a triangle.
        size: u64::from(triangles.max(1)) * 24,
        usage: wgpu::BufferUsages::STORAGE
            | wgpu::BufferUsages::COPY_SRC
            | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

/// The four bindings a pass that lets rays through cut-outs' holes reads
/// (`ray_cut_outs.wgsl`), from `first`: the slots' [`CutOutRaw`]s, the
/// meshes' texture coordinates, the alpha array and its sampler.
pub(super) fn layout_entries(
    first: u32,
    visibility: wgpu::ShaderStages,
) -> [wgpu::BindGroupLayoutEntry; 4] {
    let storage = |binding| wgpu::BindGroupLayoutEntry {
        binding,
        visibility,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Storage { read_only: true },
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    };
    [
        storage(first),
        storage(first + 1),
        wgpu::BindGroupLayoutEntry {
            binding: first + 2,
            visibility,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: wgpu::TextureViewDimension::D2Array,
                multisampled: false,
            },
            count: None,
        },
        wgpu::BindGroupLayoutEntry {
            binding: first + 3,
            visibility,
            ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
            count: None,
        },
    ]
}

impl CutOuts {
    /// What [`layout_entries`] lays out, bound from `first`.
    pub(super) fn entries(&self, first: u32) -> [wgpu::BindGroupEntry<'_>; 4] {
        [
            wgpu::BindGroupEntry {
                binding: first,
                resource: self.info.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: first + 1,
                resource: self.uvs.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: first + 2,
                resource: wgpu::BindingResource::TextureView(&self.alpha_view),
            },
            wgpu::BindGroupEntry {
                binding: first + 3,
                resource: wgpu::BindingResource::Sampler(&self.sampler),
            },
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alpha_is_averaged_over_the_pixels_each_texel_covers() {
        // A 1024-wide texture whose left half is opaque and right half clear.
        let (width, height) = (1024, 4);
        let mut rgba = vec![0u8; width * height * 4];
        for y in 0..height {
            for x in 0..width / 2 {
                rgba[(y * width + x) * 4 + 3] = 255;
            }
        }
        let alpha = shrunk_alpha(&rgba, width as u32, height as u32);
        let size = ALPHA_SIZE as usize;
        assert_eq!(alpha.len(), size * size);
        assert_eq!(alpha[0], 255, "the left edge is solid");
        assert_eq!(alpha[size - 1], 0, "the right edge is a hole");
        assert_eq!(alpha[size / 2 - 1], 255);
        assert_eq!(alpha[size / 2], 0);
    }

    #[test]
    fn a_texture_without_its_pixels_counts_as_solid() {
        assert!(shrunk_alpha(&[], 8, 8).iter().all(|&a| a == 255));
    }
}
