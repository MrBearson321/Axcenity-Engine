//! The wgpu side: GPU setup, the window surface, buffers and drawing.

use std::borrow::Cow;
use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::ops::Range;
use std::sync::Arc;

use anyhow::{Context, Result};
use bytemuck::{Pod, Zeroable};
use glam::camera::rh;
use glam::{Mat3, Vec3};
use wgpu::util::DeviceExt;

use crate::draw_list::{ChannelMap, DrawItem, DrawList, MeshSource, Shape, TextureSource};
use crate::mesh::{self, MeshData, Vertex};

const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
/// A [`TextureRenderer`]'s texture, as shown (see `create_target_texture`).
const TARGET_SHOW_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
/// A [`TextureRenderer`]'s texture, as drawn into.
const TARGET_DRAW_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;
/// Textures are stored as plain bytes. Color textures are read through an
/// sRGB view of them ([`TEXTURE_SRGB_FORMAT`]), so sampling gives linear light.
const TEXTURE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
const TEXTURE_SRGB_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;
const NEAR_PLANE: f32 = 0.1;
const FAR_PLANE: f32 = 5000.0;
/// A caller's mesh or texture not drawn for this many frames is taken off
/// the GPU (about 10 seconds at 60 fps). Drawn again, it's uploaded again.
const KEEP_UNDRAWN_FRAMES: u64 = 600;

/// Per-frame values for the shader. Must match `Frame` in shader.wgsl.
/// Every field is 16 bytes or a multiple, which is what uniform buffers need.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct FrameUniform {
    view_proj: [[f32; 4]; 4],
    camera_position: [f32; 4],
    sun_direction: [f32; 4],
    sun_radiance: [f32; 4],
    ambient: [f32; 4],
}

/// One draw item as the GPU reads it. Must match `InstanceIn` in shader.wgsl.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct InstanceRaw {
    model: [[f32; 4]; 4],
    /// Turns surface directions to world space. Unlike `model`, it keeps them
    /// perpendicular to the surface when a shape is stretched.
    normal_matrix: [[f32; 3]; 3],
    base_color_opacity: [f32; 4],
    emissive: [f32; 3],
    /// Metalness, roughness and alpha cutoff.
    surface: [f32; 3],
}

/// A mesh uploaded to the GPU.
struct GpuMesh {
    vertices: wgpu::Buffer,
    indices: wgpu::Buffer,
    index_count: u32,
    /// 16-bit for the built-in shapes, 32-bit for callers' meshes, which
    /// can have more than 65,536 corners.
    index_format: wgpu::IndexFormat,
}

/// How a material's texture maps are read. Must match `Maps` in
/// shader.wgsl. Each field is 16 bytes, as uniform buffers need.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
struct MapsUniform {
    /// Which channel holds each map's value: 1 in that one, 0 in the others.
    metalness: [f32; 4],
    roughness: [f32; 4],
    occlusion: [f32; 4],
    opacity: [f32; 4],
    /// 1 to invert metalness, roughness, occlusion and opacity.
    invert: [f32; 4],
    /// Normal map scale (0: none), green's sign, occlusion strength (0:
    /// none), and 1 if opacity has its own map.
    settings: [f32; 4],
    /// 1 if there's a specular map, then unused.
    specular: [f32; 4],
}

/// A texture uploaded to the GPU, with a view for each way it's read.
struct GpuTexture {
    /// Kept so it lives as long as the bind groups that use it.
    _texture: wgpu::Texture,
    /// Converts sRGB to linear light, for colors.
    srgb: wgpu::TextureView,
    /// Gives the bytes as they are (128 is 0.5), for values.
    linear: wgpu::TextureView,
}

/// A material's textures and [`MapsUniform`], bound together for the shader.
struct GpuMaterial {
    bind_group: wgpu::BindGroup,
    /// Kept so it lives as long as the bind group.
    _maps: wgpu::Buffer,
}

/// The number of textures a material can have: base color, normal,
/// metalness, roughness, occlusion, emissive, opacity and specular.
const MATERIAL_TEXTURES: usize = 8;

/// Which material binding an item uses: its textures' keys and its
/// [`MapsUniform`] (as bits, so it can be compared and sorted). Items with
/// the same key share one binding.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
struct MaterialKey {
    textures: [Option<u64>; MATERIAL_TEXTURES],
    maps: [u32; 28],
}

/// Something of a caller's on the GPU, and the frame it was last drawn in.
/// `None` if its data was broken, so that's only reported once.
struct Cached<T> {
    gpu: Option<T>,
    last_drawn: u64,
}

/// Which mesh a batch draws: a built-in shape, or a caller's by its key.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum MeshKey {
    Block,
    Sphere,
    Cylinder,
    Wedge,
    Custom(u64),
}

impl MeshKey {
    fn of(shape: &Shape) -> Self {
        match shape {
            Shape::Block => MeshKey::Block,
            Shape::Sphere => MeshKey::Sphere,
            Shape::Cylinder => MeshKey::Cylinder,
            Shape::Wedge => MeshKey::Wedge,
            Shape::Mesh(mesh) => MeshKey::Custom(mesh.key()),
        }
    }
}

/// One uploaded mesh per [`Shape`].
struct Meshes {
    block: GpuMesh,
    sphere: GpuMesh,
    cylinder: GpuMesh,
    wedge: GpuMesh,
}

impl Meshes {
    /// The built-in mesh for `key`, or `None` for a caller's mesh.
    fn get(&self, key: MeshKey) -> Option<&GpuMesh> {
        match key {
            MeshKey::Block => Some(&self.block),
            MeshKey::Sphere => Some(&self.sphere),
            MeshKey::Cylinder => Some(&self.cylinder),
            MeshKey::Wedge => Some(&self.wedge),
            MeshKey::Custom(_) => None,
        }
    }
}

/// Consecutive instances with the same mesh, the same material textures
/// and the same kind of drawing (solid or see-through), drawn with one
/// draw call.
#[derive(Clone, Debug, PartialEq)]
struct Batch {
    mesh: MeshKey,
    material: MaterialKey,
    transparent: bool,
    instances: Range<u32>,
}

/// Draws a scene into a window. The player uses this.
pub struct Renderer {
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    /// True while the window is minimized (zero-sized), when there's nothing to draw.
    minimized: bool,
    scene: ScenePipeline,
}

/// Draws a scene into a texture instead of a window, using a GPU device that
/// someone else set up. The editor uses this for its viewport: egui owns the
/// window and shows the texture as an image.
pub struct TextureRenderer {
    scene: ScenePipeline,
    texture: wgpu::Texture,
    /// The texture seen as sRGB, so drawing converts linear light to sRGB
    /// the same way a window surface does.
    draw_view: wgpu::TextureView,
    /// The same texture seen as plain bytes, for others to show.
    view: wgpu::TextureView,
}

/// The GPU objects for drawing a scene, whatever it's drawn into. Both
/// [`Renderer`] and [`TextureRenderer`] own one.
struct ScenePipeline {
    device: wgpu::Device,
    queue: wgpu::Queue,
    depth_view: wgpu::TextureView,
    opaque_pipeline: wgpu::RenderPipeline,
    transparent_pipeline: wgpu::RenderPipeline,
    frame_buffer: wgpu::Buffer,
    frame_bind_group: wgpu::BindGroup,
    meshes: Meshes,
    instance_buffer: wgpu::Buffer,
    /// How many instances fit in `instance_buffer` before it must grow.
    instance_capacity: usize,
    material_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    /// One white pixel, for maps an item doesn't have: it changes nothing.
    white_texture: GpuTexture,
    /// For items with no textures, such as every built-in shape.
    plain_material: GpuMaterial,
    /// Callers' meshes and textures, by key, and the materials using them.
    custom_meshes: HashMap<u64, Cached<GpuMesh>>,
    textures: HashMap<u64, Cached<GpuTexture>>,
    materials: HashMap<MaterialKey, Cached<GpuMaterial>>,
    /// How many frames have been drawn, to tell what wasn't drawn lately.
    frame: u64,
}

impl Renderer {
    /// Sets up the GPU to draw into `target`, which is usually a window.
    /// `width` and `height` are the target's size in physical pixels.
    pub async fn new(
        target: impl Into<wgpu::SurfaceTarget<'static>>,
        width: u32,
        height: u32,
    ) -> Result<Self> {
        // `Instance` picks a graphics API: DirectX 12 or Vulkan on Windows,
        // Metal on macOS, Vulkan on Linux.
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
        let surface = instance
            .create_surface(target)
            .context("could not create a drawing surface for the window")?;

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .context("no compatible graphics card found")?;
        log::info!("using graphics adapter: {:?}", adapter.get_info());

        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("axcenity device"),
                ..Default::default()
            })
            .await
            .context("could not open the graphics device")?;

        let (width, height) = drawable_size(width, height).unwrap_or((1, 1));
        let mut config = surface
            .get_default_config(&adapter, width, height)
            .context("the graphics card cannot draw to this window")?;
        // An sRGB surface converts the shader's linear colors for the screen.
        let formats = surface.get_capabilities(&adapter).formats;
        match formats.iter().find(|format| format.is_srgb()) {
            Some(&format) => config.format = format,
            None => log::warn!(
                "no sRGB surface format available ({formats:?}); colors will look too dark"
            ),
        }
        surface.configure(&device, &config);

        Ok(Self {
            surface,
            minimized: false,
            scene: ScenePipeline::new(device, queue, config.format, width, height),
            config,
        })
    }

    /// Call when the window changes size. Sizes are in physical pixels.
    pub fn resize(&mut self, width: u32, height: u32) {
        let Some((width, height)) = drawable_size(width, height) else {
            self.minimized = true;
            return;
        };
        self.minimized = false;
        self.config.width = width;
        self.config.height = height;
        self.surface.configure(&self.scene.device, &self.config);
        self.scene.resize(width, height);
    }

    /// Draws one frame. `before_present` runs just before the frame is shown;
    /// windowing libraries use that moment to pace frames (for example
    /// `winit`'s `Window::pre_present_notify`).
    pub fn render(&mut self, list: &DrawList, before_present: impl FnOnce()) -> Result<()> {
        if self.minimized {
            return Ok(());
        }

        let frame = match self.surface.get_current_texture() {
            Ok(frame) => frame,
            // The surface no longer matches the window (for example, it moved
            // to another monitor). Reconfigure and draw on the next frame.
            Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                self.surface.configure(&self.scene.device, &self.config);
                return Ok(());
            }
            // The GPU took too long to hand over a frame. Skip this one.
            Err(wgpu::SurfaceError::Timeout) => return Ok(()),
            Err(err) => return Err(err).context("could not get the next frame to draw into"),
        };

        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let aspect = self.config.width as f32 / self.config.height as f32;
        self.scene.draw(list, &view, aspect);
        before_present();
        frame.present();
        Ok(())
    }
}

impl TextureRenderer {
    /// Sets up drawing into a `width` × `height` texture on an existing
    /// device. Sizes of zero are treated as 1, since a texture can't be empty.
    ///
    /// `wgpu::Device` and `wgpu::Queue` are cheap handles to shared GPU
    /// objects, so cloning them here doesn't copy anything big.
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, width: u32, height: u32) -> Self {
        let (width, height) = (width.max(1), height.max(1));
        let scene = ScenePipeline::new(
            device.clone(),
            queue.clone(),
            TARGET_DRAW_FORMAT,
            width,
            height,
        );
        let (texture, draw_view, view) = create_target_texture(device, width, height);
        Self {
            scene,
            texture,
            draw_view,
            view,
        }
    }

    /// The texture's size in pixels.
    pub fn size(&self) -> (u32, u32) {
        (self.texture.width(), self.texture.height())
    }

    /// Replaces the texture with one of a new size, unless it's already that
    /// size. The old texture view stops being updated, so anything showing it
    /// (such as an egui image) must switch to [`TextureRenderer::view`] again.
    pub fn resize(&mut self, width: u32, height: u32) {
        let (width, height) = (width.max(1), height.max(1));
        if self.size() == (width, height) {
            return;
        }
        (self.texture, self.draw_view, self.view) =
            create_target_texture(&self.scene.device, width, height);
        self.scene.resize(width, height);
    }

    /// The finished picture, for showing it (for example as an egui image).
    ///
    /// Its format is `Rgba8Unorm` holding sRGB-encoded colors, the same as
    /// egui's own textures: sampling it gives the colors as they'd appear on
    /// screen, not linear light.
    pub fn view(&self) -> &wgpu::TextureView {
        &self.view
    }

    /// Draws one frame into the texture. The GPU runs it in order with
    /// anything submitted afterward, such as egui drawing the texture.
    pub fn render(&mut self, list: &DrawList) {
        let (width, height) = self.size();
        self.scene
            .draw(list, &self.draw_view, width as f32 / height as f32);
    }

    /// Reads the texture back to the CPU as rows of RGBA bytes. Slow; for tests.
    #[cfg(test)]
    fn read_pixels(&self) -> Vec<u8> {
        let (width, height) = self.size();
        let device = &self.scene.device;
        // Each copied row must be a multiple of 256 bytes long.
        let row_bytes = (width * 4).next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("read back"),
            size: u64::from(row_bytes * height),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = device.create_command_encoder(&Default::default());
        encoder.copy_texture_to_buffer(
            self.texture.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(row_bytes),
                    rows_per_image: None,
                },
            },
            self.texture.size(),
        );
        self.scene.queue.submit([encoder.finish()]);
        buffer.slice(..).map_async(wgpu::MapMode::Read, |result| {
            result.expect("could not read the texture back");
        });
        device.poll(wgpu::PollType::Wait).unwrap();
        let mapped = buffer.slice(..).get_mapped_range();
        mapped
            .chunks(row_bytes as usize)
            .flat_map(|row| &row[..(width * 4) as usize])
            .copied()
            .collect()
    }
}

impl ScenePipeline {
    fn new(
        device: wgpu::Device,
        queue: wgpu::Queue,
        color_format: wgpu::TextureFormat,
        width: u32,
        height: u32,
    ) -> Self {
        let depth_view = create_depth_view(&device, width, height);
        let frame_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("frame uniform"),
            size: size_of::<FrameUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let frame_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("frame"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let frame_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("frame"),
            layout: &frame_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: frame_buffer.as_entire_binding(),
            }],
        });

        // Bindings 0 and 3–9 are the textures (see `material_bindings`).
        let texture_entry = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        };
        let mut entries = vec![
            texture_entry(0),
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
        ];
        entries.extend((3..=9).map(texture_entry));
        let material_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("material"),
            entries: &entries,
        });
        // glTF's default: smooth, blending between mipmap levels, and
        // repeating past the edges.
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("material textures"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            address_mode_w: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let white_texture = create_texture(&device, &queue, 1, 1, &[255; 4]);
        let plain_material = create_material(
            &device,
            &material_layout,
            &sampler,
            [&white_texture; MATERIAL_TEXTURES],
            maps_uniform_of(&MaterialKey::default()),
        );

        let opaque_pipeline = create_pipeline(
            &device,
            &frame_layout,
            &material_layout,
            color_format,
            false,
        );
        let transparent_pipeline =
            create_pipeline(&device, &frame_layout, &material_layout, color_format, true);
        let meshes = Meshes {
            block: upload_mesh(&device, "block", &mesh::block()),
            sphere: upload_mesh(&device, "sphere", &mesh::sphere()),
            cylinder: upload_mesh(&device, "cylinder", &mesh::cylinder()),
            wedge: upload_mesh(&device, "wedge", &mesh::wedge()),
        };
        let instance_capacity = 64;
        let instance_buffer = create_instance_buffer(&device, instance_capacity);

        Self {
            device,
            queue,
            depth_view,
            opaque_pipeline,
            transparent_pipeline,
            frame_buffer,
            frame_bind_group,
            meshes,
            instance_buffer,
            instance_capacity,
            material_layout,
            sampler,
            white_texture,
            plain_material,
            custom_meshes: HashMap::new(),
            textures: HashMap::new(),
            materials: HashMap::new(),
            frame: 0,
        }
    }

    /// Call when the target changes size. Sizes are in pixels and never zero.
    fn resize(&mut self, width: u32, height: u32) {
        self.depth_view = create_depth_view(&self.device, width, height);
    }

    /// Draws `list` into `view`, which must be the size last given to
    /// `new` or `resize`, and sends the work to the GPU.
    fn draw(&mut self, list: &DrawList, view: &wgpu::TextureView, aspect: f32) {
        let frame_uniform = frame_uniform(list, aspect);
        self.queue
            .write_buffer(&self.frame_buffer, 0, bytemuck::bytes_of(&frame_uniform));

        self.frame += 1;
        self.upload_new(list);
        let (instances, batches) = plan_draws(list);
        self.ensure_instance_capacity(instances.len());
        self.queue
            .write_buffer(&self.instance_buffer, 0, bytemuck::cast_slice(&instances));

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("frame"),
            });

        // The pass is a block so it ends (is dropped) before `encoder.finish()`,
        // which needs the encoder back.
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("scene"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(sky_clear_color(list.sky_color)),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Discard,
                    }),
                    stencil_ops: None,
                }),
                ..Default::default()
            });

            pass.set_bind_group(0, &self.frame_bind_group, &[]);
            pass.set_vertex_buffer(1, self.instance_buffer.slice(..));
            for batch in batches {
                let pipeline = if batch.transparent {
                    &self.transparent_pipeline
                } else {
                    &self.opaque_pipeline
                };
                let mesh = match batch.mesh {
                    MeshKey::Custom(key) => self
                        .custom_meshes
                        .get(&key)
                        .and_then(|cached| cached.gpu.as_ref()),
                    builtin => self.meshes.get(builtin),
                };
                // A caller's mesh with broken data was never uploaded.
                let Some(mesh) = mesh else {
                    continue;
                };
                let material = self
                    .materials
                    .get(&batch.material)
                    .and_then(|cached| cached.gpu.as_ref())
                    .unwrap_or(&self.plain_material);
                pass.set_pipeline(pipeline);
                pass.set_bind_group(1, &material.bind_group, &[]);
                pass.set_vertex_buffer(0, mesh.vertices.slice(..));
                pass.set_index_buffer(mesh.indices.slice(..), mesh.index_format);
                pass.draw_indexed(0..mesh.index_count, 0, batch.instances);
            }
        }

        self.queue.submit([encoder.finish()]);
        self.forget_undrawn();
    }

    /// Uploads the callers' meshes and textures in `list` that aren't on
    /// the GPU yet, and notes that the rest were drawn this frame.
    fn upload_new(&mut self, list: &DrawList) {
        let frame = self.frame;
        for item in &list.items {
            if let Shape::Mesh(source) = &item.shape {
                match self.custom_meshes.entry(source.key()) {
                    Entry::Occupied(mut cached) => cached.get_mut().last_drawn = frame,
                    Entry::Vacant(slot) => {
                        slot.insert(Cached {
                            gpu: upload_custom_mesh(&self.device, source.as_ref()),
                            last_drawn: frame,
                        });
                    }
                }
            }
            let sources = material_textures(item);
            for source in sources.iter().flatten() {
                match self.textures.entry(source.key()) {
                    Entry::Occupied(mut cached) => cached.get_mut().last_drawn = frame,
                    Entry::Vacant(slot) => {
                        slot.insert(Cached {
                            gpu: upload_texture(&self.device, &self.queue, source.as_ref()),
                            last_drawn: frame,
                        });
                    }
                }
            }
            let key = material_key(item);
            if key == MaterialKey::default() {
                continue;
            }
            match self.materials.entry(key) {
                Entry::Occupied(mut cached) => cached.get_mut().last_drawn = frame,
                Entry::Vacant(slot) => {
                    // A texture with broken data is drawn as white instead.
                    let textures = sources.map(|source| {
                        source
                            .and_then(|source| self.textures.get(&source.key())?.gpu.as_ref())
                            .unwrap_or(&self.white_texture)
                    });
                    slot.insert(Cached {
                        gpu: Some(create_material(
                            &self.device,
                            &self.material_layout,
                            &self.sampler,
                            textures,
                            maps_uniform_of(&key),
                        )),
                        last_drawn: frame,
                    });
                }
            }
        }
    }

    /// Frees the callers' meshes, textures and materials that haven't been
    /// drawn lately.
    fn forget_undrawn(&mut self) {
        let frame = self.frame;
        let recent = |last_drawn: u64| frame - last_drawn < KEEP_UNDRAWN_FRAMES;
        self.custom_meshes
            .retain(|_, cached| recent(cached.last_drawn));
        self.textures.retain(|_, cached| recent(cached.last_drawn));
        self.materials.retain(|_, cached| recent(cached.last_drawn));
    }

    /// Replaces the instance buffer with a bigger one if `count` won't fit.
    fn ensure_instance_capacity(&mut self, count: usize) {
        if count <= self.instance_capacity {
            return;
        }
        self.instance_capacity = count.next_power_of_two();
        self.instance_buffer = create_instance_buffer(&self.device, self.instance_capacity);
    }
}

fn frame_uniform(list: &DrawList, aspect: f32) -> FrameUniform {
    let camera = &list.camera;
    let view = rh::view::look_at_mat4(camera.position, camera.look_at, Vec3::Y);
    // wgpu uses DirectX-style clip space: depth from 0 to 1, Y up.
    let projection = rh::proj::directx::perspective(
        camera.fov_y_degrees.to_radians(),
        aspect,
        NEAR_PLANE,
        FAR_PLANE,
    );
    let sun_direction = list.sun.direction.normalize_or(Vec3::Y);
    let sun_radiance = linear_color(list.sun.color) * list.sun.intensity;
    let ambient = linear_color(list.ambient.color) * list.ambient.intensity;

    FrameUniform {
        view_proj: (projection * view).to_cols_array_2d(),
        camera_position: camera.position.extend(1.0).to_array(),
        sun_direction: sun_direction.extend(0.0).to_array(),
        sun_radiance: sun_radiance.extend(0.0).to_array(),
        ambient: ambient.extend(0.0).to_array(),
    }
}

/// Puts the visible items in drawing order and groups them into batches.
///
/// Solid items come first, grouped by mesh and texture. See-through items come last,
/// farthest from the camera first, so each one blends over what's behind it.
/// They're sorted by their centers, so two that overlap can still blend in the
/// wrong order; that's the usual trade-off for this simple approach.
fn plan_draws(list: &DrawList) -> (Vec<InstanceRaw>, Vec<Batch>) {
    let (mut transparent, mut opaque): (Vec<&DrawItem>, Vec<&DrawItem>) = list
        .items
        .iter()
        .filter(|item| item.material.opacity > 0.0)
        .partition(|item| item.material.opacity < 1.0);

    opaque.sort_by_key(|item| (MeshKey::of(&item.shape), material_key(item)));
    let distance = |item: &DrawItem| {
        item.transform
            .w_axis
            .truncate()
            .distance_squared(list.camera.position)
    };
    // `total_cmp` orders floats even when one is NaN, so sorting can't panic.
    transparent.sort_by(|a, b| distance(b).total_cmp(&distance(a)));

    let mut instances = Vec::new();
    let mut batches: Vec<Batch> = Vec::new();
    let ordered = opaque
        .into_iter()
        .map(|item| (item, false))
        .chain(transparent.into_iter().map(|item| (item, true)));
    for (item, transparent) in ordered {
        let Some(raw) = instance_raw(item) else {
            continue;
        };
        let index = instances.len() as u32;
        instances.push(raw);
        let (mesh, material) = (MeshKey::of(&item.shape), material_key(item));
        match batches.last_mut() {
            Some(last)
                if last.mesh == mesh
                    && last.material == material
                    && last.transparent == transparent =>
            {
                last.instances.end = index + 1;
            }
            _ => batches.push(Batch {
                mesh,
                material,
                transparent,
                instances: index..index + 1,
            }),
        }
    }
    (instances, batches)
}

/// The textures an item is drawn with, in binding order (see
/// [`MATERIAL_TEXTURES`]). Only callers' meshes have texture coordinates,
/// so the built-in shapes never use any.
fn material_textures(item: &DrawItem) -> [Option<&Arc<dyn TextureSource>>; MATERIAL_TEXTURES] {
    if !matches!(item.shape, Shape::Mesh(_)) {
        return [None; MATERIAL_TEXTURES];
    }
    let material = &item.material;
    let maps = &material.maps;
    [
        material.base_color_texture.as_ref(),
        maps.normal.as_ref().map(|map| &map.texture),
        maps.metalness.as_ref().map(|map| &map.texture),
        maps.roughness.as_ref().map(|map| &map.texture),
        maps.occlusion.as_ref().map(|map| &map.texture),
        maps.emissive.as_ref(),
        maps.opacity.as_ref().map(|map| &map.texture),
        maps.specular.as_ref(),
    ]
}

/// The material binding an item is drawn with (see [`MaterialKey`]).
fn material_key(item: &DrawItem) -> MaterialKey {
    let textures = material_textures(item).map(|source| source.map(|source| source.key()));
    if textures.iter().all(Option::is_none) {
        return MaterialKey::default();
    }
    let maps = &item.material.maps;
    let mask = |map: &Option<ChannelMap>| {
        let mut mask = [0.0; 4];
        mask[map.as_ref().map_or(0, |map| map.channel as usize)] = 1.0;
        mask
    };
    let inverted = |map: &Option<ChannelMap>| f32::from(map.as_ref().is_some_and(|map| map.invert));
    let used = |used: bool| f32::from(used);
    let uniform = MapsUniform {
        metalness: mask(&maps.metalness),
        roughness: mask(&maps.roughness),
        occlusion: mask(&maps.occlusion),
        opacity: mask(&maps.opacity),
        invert: [
            inverted(&maps.metalness),
            inverted(&maps.roughness),
            inverted(&maps.occlusion),
            inverted(&maps.opacity),
        ],
        settings: [
            maps.normal.as_ref().map_or(0.0, |map| map.scale),
            if maps.normal.as_ref().is_some_and(|map| map.flip_green) {
                -1.0
            } else {
                1.0
            },
            if maps.occlusion.is_some() {
                maps.occlusion_strength.clamp(0.0, 1.0)
            } else {
                0.0
            },
            used(maps.opacity.is_some()),
        ],
        specular: [used(maps.specular.is_some()), 0.0, 0.0, 0.0],
    };
    MaterialKey {
        textures,
        maps: bytemuck::cast(uniform),
    }
}

/// The [`MapsUniform`] a key was made from. The default key's is the plain
/// material's: no maps.
fn maps_uniform_of(key: &MaterialKey) -> MapsUniform {
    if *key == MaterialKey::default() {
        let red = [1.0, 0.0, 0.0, 0.0];
        return MapsUniform {
            metalness: red,
            roughness: red,
            occlusion: red,
            opacity: red,
            invert: [0.0; 4],
            settings: [0.0, 1.0, 0.0, 0.0],
            specular: [0.0; 4],
        };
    }
    bytemuck::cast(key.maps)
}

/// The GPU data for one item, or `None` if it can't be seen.
fn instance_raw(item: &DrawItem) -> Option<InstanceRaw> {
    let model3 = Mat3::from_mat4(item.transform);
    // A shape squashed flat (size 0 on some axis) has no inverse, and can't be seen anyway.
    if model3.determinant().abs() < 1e-12 {
        return None;
    }
    let material = &item.material;
    let base_color = linear_color(material.base_color);
    Some(InstanceRaw {
        model: item.transform.to_cols_array_2d(),
        normal_matrix: model3.inverse().transpose().to_cols_array_2d(),
        base_color_opacity: base_color.extend(material.opacity).to_array(),
        emissive: linear_color(material.emissive).to_array(),
        surface: [
            material.metalness,
            material.roughness,
            material.alpha_cutoff,
        ],
    })
}

/// Converts an sRGB color (as people pick it) to linear light (what shading
/// math needs).
fn linear_color([r, g, b]: [f32; 3]) -> Vec3 {
    fn channel(c: f32) -> f32 {
        if c <= 0.04045 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    }
    Vec3::new(channel(r), channel(g), channel(b))
}

fn sky_clear_color(srgb: [f32; 3]) -> wgpu::Color {
    let linear = linear_color(srgb);
    wgpu::Color {
        r: linear.x.into(),
        g: linear.y.into(),
        b: linear.z.into(),
        a: 1.0,
    }
}

/// The pipeline for solid items, or for see-through ones when `transparent`
/// is true. Both use the same shader.
fn create_pipeline(
    device: &wgpu::Device,
    frame_layout: &wgpu::BindGroupLayout,
    material_layout: &wgpu::BindGroupLayout,
    color_format: wgpu::TextureFormat,
    transparent: bool,
) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::include_wgsl!("shader.wgsl"));
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("scene"),
        bind_group_layouts: &[frame_layout, material_layout],
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
            10 => Float32x3,
            11 => Float32x3,
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
        label: Some(if transparent { "transparent" } else { "opaque" }),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            compilation_options: Default::default(),
            buffers: &[vertex_layout, instance_layout],
        },
        primitive: wgpu::PrimitiveState {
            // Counter-clockwise triangles face the camera; the backs are skipped.
            front_face: wgpu::FrontFace::Ccw,
            cull_mode: Some(wgpu::Face::Back),
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
        multisample: Default::default(),
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
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

fn create_depth_view(device: &wgpu::Device, width: u32, height: u32) -> wgpu::TextureView {
    device
        .create_texture(&wgpu::TextureDescriptor {
            label: Some("depth"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
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
fn create_target_texture(
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

fn create_instance_buffer(device: &wgpu::Device, capacity: usize) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("instances"),
        size: (capacity * size_of::<InstanceRaw>()) as u64,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

fn upload_mesh(device: &wgpu::Device, name: &str, mesh: &MeshData) -> GpuMesh {
    upload_buffers(
        device,
        name,
        &mesh.vertices,
        bytemuck::cast_slice(&mesh.indices),
        mesh.indices.len(),
        wgpu::IndexFormat::Uint16,
    )
}

/// Uploads a caller's mesh, or `None` (with a warning) if its data doesn't
/// fit together, such as an index past the last corner.
fn upload_custom_mesh(device: &wgpu::Device, source: &dyn MeshSource) -> Option<GpuMesh> {
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
    let vertices: Vec<Vertex> = (0..count)
        .map(|i| Vertex {
            position: positions[i],
            normal: normals[i],
            uv: uvs.get(i).copied().unwrap_or_default(),
            tangent: tangents.get(i).copied().unwrap_or(mesh::NO_TANGENT),
        })
        .collect();
    Some(upload_buffers(
        device,
        "mesh",
        &vertices,
        bytemuck::cast_slice(indices),
        indices.len(),
        wgpu::IndexFormat::Uint32,
    ))
}

fn upload_buffers(
    device: &wgpu::Device,
    name: &str,
    vertices: &[Vertex],
    indices: &[u8],
    index_count: usize,
    index_format: wgpu::IndexFormat,
) -> GpuMesh {
    let vertices = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some(name),
        contents: bytemuck::cast_slice(vertices),
        usage: wgpu::BufferUsages::VERTEX,
    });
    let indices = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some(name),
        contents: indices,
        usage: wgpu::BufferUsages::INDEX,
    });
    GpuMesh {
        vertices,
        indices,
        index_count: index_count as u32,
        index_format,
    }
}

/// Uploads a caller's texture, or `None` (with a warning) if it has no
/// pixels or the wrong number of them.
fn upload_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    source: &dyn TextureSource,
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
    Some(create_texture(device, queue, width, height, rgba))
}

/// Binds a material's `textures` (in [`MATERIAL_TEXTURES`] order) and
/// `maps` for the shader. Colors are read through sRGB views, the rest as
/// plain values.
fn create_material(
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

/// Uploads RGBA pixels with a full set of mipmaps (smaller copies, so far
/// away surfaces don't shimmer), with an sRGB and a plain view.
fn create_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    width: u32,
    height: u32,
    rgba: &[u8],
) -> GpuTexture {
    let mut levels = mip_levels(width, height, rgba);
    // Leave out levels too big for this GPU, so a huge texture still shows,
    // just less sharp. The last level is 1×1, so one is always left.
    let max = device.limits().max_texture_dimension_2d;
    levels.retain(|(width, height, _)| *width <= max && *height <= max);
    let (width, height) = (levels[0].0, levels[0].1);
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("material texture"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: levels.len() as u32,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: TEXTURE_FORMAT,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[TEXTURE_SRGB_FORMAT],
    });
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
    let linear = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let srgb = texture.create_view(&wgpu::TextureViewDescriptor {
        format: Some(TEXTURE_SRGB_FORMAT),
        ..Default::default()
    });
    GpuTexture {
        _texture: texture,
        srgb,
        linear,
    }
}

/// The image and each half-size copy of it, down to 1×1, as (width,
/// height, pixels). `Cow` lets the first be the caller's pixels, uncopied.
fn mip_levels(width: u32, height: u32, rgba: &[u8]) -> Vec<(u32, u32, Cow<'_, [u8]>)> {
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
fn drawable_size(width: u32, height: u32) -> Option<(u32, u32)> {
    (width > 0 && height > 0).then_some((width, height))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::draw_list::{AmbientLight, Camera, Channel, DrawItem, Material, NormalMap, Sun};
    use glam::Mat4;

    #[test]
    fn zero_sized_window_has_nothing_to_draw() {
        assert_eq!(drawable_size(0, 0), None);
        assert_eq!(drawable_size(0, 720), None);
        assert_eq!(drawable_size(1280, 0), None);
    }

    #[test]
    fn normal_window_keeps_its_size() {
        assert_eq!(drawable_size(1280, 720), Some((1280, 720)));
        assert_eq!(drawable_size(1, 1), Some((1, 1)));
    }

    #[test]
    fn srgb_to_linear_keeps_black_and_white_and_darkens_midtones() {
        assert_eq!(linear_color([0.0, 1.0, 0.5]).x, 0.0);
        assert!((linear_color([0.0, 1.0, 0.5]).y - 1.0).abs() < 1e-6);
        assert!((linear_color([0.0, 1.0, 0.5]).z - 0.214).abs() < 0.001);
    }

    /// A list seen from the origin.
    fn list_of(items: Vec<DrawItem>) -> DrawList {
        DrawList {
            sky_color: [0.0; 3],
            camera: Camera {
                position: Vec3::ZERO,
                look_at: Vec3::NEG_Z,
                fov_y_degrees: 70.0,
            },
            sun: Sun {
                direction: Vec3::Y,
                color: [1.0; 3],
                intensity: 1.0,
            },
            ambient: AmbientLight {
                color: [1.0; 3],
                intensity: 0.0,
            },
            items,
        }
    }

    /// A unit-sized item at `position`.
    fn item(shape: Shape, position: Vec3, opacity: f32) -> DrawItem {
        DrawItem {
            shape,
            transform: Mat4::from_translation(position),
            material: Material {
                opacity,
                ..Material::default()
            },
        }
    }

    fn batch(mesh: MeshKey, transparent: bool, instances: Range<u32>) -> Batch {
        Batch {
            mesh,
            material: MaterialKey::default(),
            transparent,
            instances,
        }
    }

    /// A caller's mesh with no triangles, for planning tests (it's never uploaded).
    struct TestMesh(u64);

    impl MeshSource for TestMesh {
        fn key(&self) -> u64 {
            self.0
        }
        fn positions(&self) -> &[[f32; 3]] {
            &[]
        }
        fn normals(&self) -> &[[f32; 3]] {
            &[]
        }
        fn uvs(&self) -> &[[f32; 2]] {
            &[]
        }
        fn indices(&self) -> &[u32] {
            &[]
        }
    }

    /// One white pixel.
    struct TestTexture(u64);

    impl TextureSource for TestTexture {
        fn key(&self) -> u64 {
            self.0
        }
        fn size(&self) -> (u32, u32) {
            (1, 1)
        }
        fn rgba(&self) -> &[u8] {
            &[255; 4]
        }
    }

    fn textured(shape: Shape, texture: Option<u64>) -> DrawItem {
        let mut item = item(shape, Vec3::ZERO, 1.0);
        item.material.base_color_texture =
            texture.map(|key| Arc::new(TestTexture(key)) as Arc<dyn TextureSource>);
        item
    }

    #[test]
    fn callers_meshes_are_grouped_by_mesh_then_texture() {
        let mesh = |key| Shape::Mesh(Arc::new(TestMesh(key)));
        let list = list_of(vec![
            textured(mesh(7), Some(2)),
            textured(mesh(5), None),
            textured(mesh(7), Some(1)),
            textured(mesh(7), Some(2)),
            // Built-in shapes have no texture coordinates, so no texture.
            textured(Shape::Block, Some(1)),
        ]);
        let (_, batches) = plan_draws(&list);
        let textures: Vec<(MeshKey, Option<u64>, Range<u32>)> = batches
            .into_iter()
            .map(|batch| (batch.mesh, batch.material.textures[0], batch.instances))
            .collect();
        assert_eq!(
            textures,
            vec![
                (MeshKey::Block, None, 0..1),
                (MeshKey::Custom(5), None, 1..2),
                (MeshKey::Custom(7), Some(1), 2..3),
                (MeshKey::Custom(7), Some(2), 3..5),
            ]
        );
    }

    #[test]
    fn maps_and_their_settings_pick_the_material_binding() {
        let mesh = Shape::Mesh(Arc::new(TestMesh(1)));
        let texture = |key| Arc::new(TestTexture(key)) as Arc<dyn TextureSource>;
        let mut packed = textured(mesh.clone(), None);
        // One ORM texture: occlusion in red, roughness in green, metalness in blue.
        let channel = |channel| {
            Some(ChannelMap {
                texture: texture(9),
                channel,
                invert: false,
            })
        };
        packed.material.maps.occlusion = channel(Channel::Red);
        packed.material.maps.roughness = channel(Channel::Green);
        packed.material.maps.metalness = channel(Channel::Blue);
        let key = material_key(&packed);
        assert_eq!(
            key.textures,
            [None, None, Some(9), Some(9), Some(9), None, None, None]
        );
        let maps = maps_uniform_of(&key);
        assert_eq!(maps.metalness, [0.0, 0.0, 1.0, 0.0]);
        assert_eq!(maps.roughness, [0.0, 1.0, 0.0, 0.0]);
        assert_eq!(maps.settings, [0.0, 1.0, 1.0, 0.0]);

        // The same textures read another way need another binding.
        let mut glossy = packed.clone();
        glossy.material.maps.roughness.as_mut().unwrap().invert = true;
        assert_ne!(material_key(&glossy), key);
        assert_eq!(maps_uniform_of(&material_key(&glossy)).invert[1], 1.0);
        // Built-in shapes have no texture coordinates, so no maps.
        let mut block = packed.clone();
        block.shape = Shape::Block;
        assert_eq!(material_key(&block), MaterialKey::default());
    }

    #[test]
    fn mipmaps_halve_down_to_one_pixel_averaging_each_square() {
        // 3×2: an odd width, so the last column is averaged with itself.
        let rgba: Vec<u8> = [0, 100, 200, 40, 80, 120]
            .iter()
            .flat_map(|&v| [v, v, v, 255])
            .collect();
        let levels = mip_levels(3, 2, &rgba);
        let sizes: Vec<(u32, u32)> = levels.iter().map(|(w, h, _)| (*w, *h)).collect();
        assert_eq!(sizes, [(3, 2), (1, 1)]);
        // The top-left 2×2: (0 + 100 + 40 + 80) / 4 = 55.
        assert_eq!(levels[1].2[..4], [55, 55, 55, 255]);
    }

    #[test]
    fn flat_and_invisible_shapes_are_skipped() {
        let mut flat = item(Shape::Block, Vec3::ZERO, 1.0);
        flat.transform = Mat4::from_scale(Vec3::new(1.0, 0.0, 1.0));
        let list = list_of(vec![
            item(Shape::Block, Vec3::ZERO, 1.0),
            flat,
            item(Shape::Block, Vec3::ZERO, 0.0),
        ]);
        let (instances, batches) = plan_draws(&list);
        assert_eq!(instances.len(), 1);
        assert_eq!(batches, vec![batch(MeshKey::Block, false, 0..1)]);
    }

    #[test]
    fn solid_items_are_grouped_by_shape() {
        let list = list_of(vec![
            item(Shape::Wedge, Vec3::ZERO, 1.0),
            item(Shape::Block, Vec3::ZERO, 1.0),
            item(Shape::Wedge, Vec3::ZERO, 1.0),
            item(Shape::Block, Vec3::ZERO, 1.0),
        ]);
        let (_, batches) = plan_draws(&list);
        assert_eq!(
            batches,
            vec![
                batch(MeshKey::Block, false, 0..2),
                batch(MeshKey::Wedge, false, 2..4)
            ]
        );
    }

    #[test]
    fn see_through_items_come_last_and_farthest_first() {
        let near_glass = item(Shape::Sphere, Vec3::new(0.0, 0.0, -2.0), 0.5);
        let far_glass = item(Shape::Sphere, Vec3::new(0.0, 0.0, -9.0), 0.5);
        let middle_glass = item(Shape::Block, Vec3::new(0.0, 0.0, -5.0), 0.5);
        let solid = item(Shape::Sphere, Vec3::new(0.0, 0.0, -1.0), 1.0);
        let list = list_of(vec![near_glass, far_glass, solid, middle_glass]);

        let (instances, batches) = plan_draws(&list);
        assert_eq!(
            batches,
            vec![
                batch(MeshKey::Sphere, false, 0..1),
                batch(MeshKey::Sphere, true, 1..2),
                batch(MeshKey::Block, true, 2..3),
                batch(MeshKey::Sphere, true, 3..4),
            ]
        );
        let z = |i: usize| instances[i].model[3][2];
        assert_eq!([z(1), z(2), z(3)], [-9.0, -5.0, -2.0]);
    }

    /// A GPU device for tests, or `None` on machines without one (some CI
    /// runners have no graphics adapter at all).
    fn test_device() -> Option<(wgpu::Device, wgpu::Queue)> {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
        let adapter = pollster::block_on(instance.request_adapter(&Default::default())).ok()?;
        pollster::block_on(adapter.request_device(&Default::default())).ok()
    }

    #[test]
    fn texture_renderer_draws_the_sky_and_a_part_into_a_texture() {
        let Some((device, queue)) = test_device() else {
            eprintln!("skipped: no graphics adapter on this machine");
            return;
        };
        // Pure red is 255 whether or not the GPU sRGB-encodes the background
        // clear (some drivers don't when drawing through an sRGB view).
        let red = [255, 0, 0, 255];
        let mut list = list_of(vec![]);
        list.sky_color = [1.0, 0.0, 0.0];
        let mut renderer = TextureRenderer::new(&device, &queue, 64, 32);
        renderer.render(&list);
        let pixels = renderer.read_pixels();
        assert_eq!(pixels.len(), 64 * 32 * 4);
        assert_eq!(pixels[..4], red);

        // A big glowing block straight ahead covers the middle of the view,
        // after resizing to a different shape. With no sun or ambient light,
        // its pixels are exactly its glow color.
        list.sun.intensity = 0.0;
        list.items.push(DrawItem {
            shape: Shape::Block,
            transform: Mat4::from_scale_rotation_translation(
                Vec3::splat(4.0),
                glam::Quat::IDENTITY,
                Vec3::new(0.0, 0.0, -5.0),
            ),
            material: Material {
                base_color: [0.0; 3],
                emissive: [0.63, 0.8, 0.95],
                ..Material::default()
            },
        });
        renderer.resize(64, 64);
        assert_eq!(renderer.size(), (64, 64));
        renderer.render(&list);
        let pixels = renderer.read_pixels();
        let pixel = |x: usize, y: usize| &pixels[(y * 64 + x) * 4..][..4];
        assert_eq!(pixel(0, 0), red, "the corner shows the sky");
        // The texture holds the sRGB color as picked (0.63 × 255 ≈ 161), the
        // way egui expects, not linear light (which would be about 90).
        assert_eq!(
            pixel(32, 32),
            [161, 204, 242, 255],
            "the middle shows the block"
        );
    }

    /// A square facing +Z, filling the unit cube's front, with texture
    /// coordinates and tangents.
    struct Square;

    impl MeshSource for Square {
        fn key(&self) -> u64 {
            1
        }
        fn positions(&self) -> &[[f32; 3]] {
            &[
                [-0.5, -0.5, 0.0],
                [0.5, -0.5, 0.0],
                [0.5, 0.5, 0.0],
                [-0.5, 0.5, 0.0],
            ]
        }
        fn normals(&self) -> &[[f32; 3]] {
            &[[0.0, 0.0, 1.0]; 4]
        }
        fn uvs(&self) -> &[[f32; 2]] {
            &[[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]]
        }
        fn tangents(&self) -> &[[f32; 4]] {
            &[[1.0, 0.0, 0.0, 1.0]; 4]
        }
        fn indices(&self) -> &[u32] {
            &[0, 1, 2, 0, 2, 3]
        }
    }

    /// One pixel of `rgba`.
    struct Pixel(u64, [u8; 4]);

    impl TextureSource for Pixel {
        fn key(&self) -> u64 {
            self.0
        }
        fn size(&self) -> (u32, u32) {
            (1, 1)
        }
        fn rgba(&self) -> &[u8] {
            &self.1
        }
    }

    #[test]
    fn texture_renderer_draws_a_mesh_with_texture_maps() {
        let Some((device, queue)) = test_device() else {
            eprintln!("skipped: no graphics adapter on this machine");
            return;
        };
        let mut list = list_of(vec![]);
        list.sun.intensity = 0.0;
        let pixel = |key, rgba| Arc::new(Pixel(key, rgba)) as Arc<dyn TextureSource>;
        let mut material = Material {
            base_color: [0.0; 3],
            emissive: [1.0; 3],
            ..Material::default()
        };
        // Glows only in the emissive map's green; the other maps are bound
        // but change nothing without light.
        material.maps.emissive = Some(pixel(2, [0, 255, 0, 255]));
        material.maps.normal = Some(NormalMap {
            texture: pixel(3, [128, 128, 255, 255]),
            scale: 1.0,
            flip_green: true,
        });
        material.maps.roughness = Some(ChannelMap {
            texture: pixel(4, [0, 0, 0, 255]),
            channel: Channel::Alpha,
            invert: true,
        });
        list.items.push(DrawItem {
            shape: Shape::Mesh(Arc::new(Square)),
            transform: Mat4::from_scale_rotation_translation(
                Vec3::splat(4.0),
                glam::Quat::IDENTITY,
                Vec3::new(0.0, 0.0, -3.0),
            ),
            material,
        });
        let mut renderer = TextureRenderer::new(&device, &queue, 32, 32);
        renderer.render(&list);
        let pixels = renderer.read_pixels();
        assert_eq!(pixels[(16 * 32 + 16) * 4..][..4], [0, 255, 0, 255]);
    }

    #[test]
    fn texture_renderer_never_makes_an_empty_texture() {
        let Some((device, queue)) = test_device() else {
            eprintln!("skipped: no graphics adapter on this machine");
            return;
        };
        let mut renderer = TextureRenderer::new(&device, &queue, 0, 0);
        assert_eq!(renderer.size(), (1, 1));
        renderer.resize(300, 0);
        assert_eq!(renderer.size(), (300, 1));
    }
}
