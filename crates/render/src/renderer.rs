//! The wgpu side: GPU setup, the window surface, buffers and drawing.

use std::ops::Range;

use anyhow::{Context, Result};
use bytemuck::{Pod, Zeroable};
use glam::camera::rh;
use glam::{Mat3, Vec3};
use wgpu::util::DeviceExt;

use crate::draw_list::{DrawItem, DrawList, Shape};
use crate::mesh::{self, MeshData, Vertex};

const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
const NEAR_PLANE: f32 = 0.1;
const FAR_PLANE: f32 = 5000.0;

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
    metalness_roughness: [f32; 2],
}

/// A mesh uploaded to the GPU.
struct GpuMesh {
    vertices: wgpu::Buffer,
    indices: wgpu::Buffer,
    index_count: u32,
}

/// One uploaded mesh per [`Shape`].
struct Meshes {
    block: GpuMesh,
    sphere: GpuMesh,
    cylinder: GpuMesh,
    wedge: GpuMesh,
}

impl Meshes {
    fn get(&self, shape: Shape) -> &GpuMesh {
        match shape {
            Shape::Block => &self.block,
            Shape::Sphere => &self.sphere,
            Shape::Cylinder => &self.cylinder,
            Shape::Wedge => &self.wedge,
        }
    }
}

/// Consecutive instances with the same shape and the same kind of drawing
/// (solid or see-through), drawn with one draw call.
#[derive(Clone, Debug, PartialEq)]
struct Batch {
    shape: Shape,
    transparent: bool,
    instances: Range<u32>,
}

pub struct Renderer {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    /// True while the window is minimized (zero-sized), when there's nothing to draw.
    minimized: bool,
    depth_view: wgpu::TextureView,
    opaque_pipeline: wgpu::RenderPipeline,
    transparent_pipeline: wgpu::RenderPipeline,
    frame_buffer: wgpu::Buffer,
    frame_bind_group: wgpu::BindGroup,
    meshes: Meshes,
    instance_buffer: wgpu::Buffer,
    /// How many instances fit in `instance_buffer` before it must grow.
    instance_capacity: usize,
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

        let opaque_pipeline = create_pipeline(&device, &frame_layout, config.format, false);
        let transparent_pipeline = create_pipeline(&device, &frame_layout, config.format, true);
        let meshes = Meshes {
            block: upload_mesh(&device, "block", &mesh::block()),
            sphere: upload_mesh(&device, "sphere", &mesh::sphere()),
            cylinder: upload_mesh(&device, "cylinder", &mesh::cylinder()),
            wedge: upload_mesh(&device, "wedge", &mesh::wedge()),
        };
        let instance_capacity = 64;
        let instance_buffer = create_instance_buffer(&device, instance_capacity);

        Ok(Self {
            surface,
            device,
            queue,
            config,
            minimized: false,
            depth_view,
            opaque_pipeline,
            transparent_pipeline,
            frame_buffer,
            frame_bind_group,
            meshes,
            instance_buffer,
            instance_capacity,
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
        self.surface.configure(&self.device, &self.config);
        self.depth_view = create_depth_view(&self.device, width, height);
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
                self.surface.configure(&self.device, &self.config);
                return Ok(());
            }
            // The GPU took too long to hand over a frame. Skip this one.
            Err(wgpu::SurfaceError::Timeout) => return Ok(()),
            Err(err) => return Err(err).context("could not get the next frame to draw into"),
        };

        let aspect = self.config.width as f32 / self.config.height as f32;
        let frame_uniform = frame_uniform(list, aspect);
        self.queue
            .write_buffer(&self.frame_buffer, 0, bytemuck::bytes_of(&frame_uniform));

        let (instances, batches) = plan_draws(list);
        self.ensure_instance_capacity(instances.len());
        self.queue
            .write_buffer(&self.instance_buffer, 0, bytemuck::cast_slice(&instances));

        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
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
                    view: &view,
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
                let mesh = self.meshes.get(batch.shape);
                pass.set_pipeline(pipeline);
                pass.set_vertex_buffer(0, mesh.vertices.slice(..));
                pass.set_index_buffer(mesh.indices.slice(..), wgpu::IndexFormat::Uint16);
                pass.draw_indexed(0..mesh.index_count, 0, batch.instances);
            }
        }

        self.queue.submit([encoder.finish()]);
        before_present();
        frame.present();
        Ok(())
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
/// Solid items come first, grouped by shape. See-through items come last,
/// farthest from the camera first, so each one blends over what's behind it.
/// They're sorted by their centers, so two that overlap can still blend in the
/// wrong order; that's the usual trade-off for this simple approach.
fn plan_draws(list: &DrawList) -> (Vec<InstanceRaw>, Vec<Batch>) {
    let (mut transparent, mut opaque): (Vec<&DrawItem>, Vec<&DrawItem>) = list
        .items
        .iter()
        .filter(|item| item.material.opacity > 0.0)
        .partition(|item| item.material.opacity < 1.0);

    opaque.sort_by_key(|item| item.shape);
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
        match batches.last_mut() {
            Some(last) if last.shape == item.shape && last.transparent == transparent => {
                last.instances.end = index + 1;
            }
            _ => batches.push(Batch {
                shape: item.shape,
                transparent,
                instances: index..index + 1,
            }),
        }
    }
    (instances, batches)
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
        metalness_roughness: [material.metalness, material.roughness],
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
    color_format: wgpu::TextureFormat,
    transparent: bool,
) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::include_wgsl!("shader.wgsl"));
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("scene"),
        bind_group_layouts: &[frame_layout],
        push_constant_ranges: &[],
    });

    let vertex_layout = wgpu::VertexBufferLayout {
        array_stride: size_of::<Vertex>() as u64,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3],
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
            11 => Float32x2,
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

fn create_instance_buffer(device: &wgpu::Device, capacity: usize) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("instances"),
        size: (capacity * size_of::<InstanceRaw>()) as u64,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

fn upload_mesh(device: &wgpu::Device, name: &str, mesh: &MeshData) -> GpuMesh {
    let vertices = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some(name),
        contents: bytemuck::cast_slice(&mesh.vertices),
        usage: wgpu::BufferUsages::VERTEX,
    });
    let indices = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some(name),
        contents: bytemuck::cast_slice(&mesh.indices),
        usage: wgpu::BufferUsages::INDEX,
    });
    GpuMesh {
        vertices,
        indices,
        index_count: mesh.indices.len() as u32,
    }
}

/// Returns the size to draw at, or `None` when the window has no area
/// (minimized windows report 0×0, and a GPU surface can't be zero-sized).
fn drawable_size(width: u32, height: u32) -> Option<(u32, u32)> {
    (width > 0 && height > 0).then_some((width, height))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::draw_list::{AmbientLight, Camera, DrawItem, Material, Sun};
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

    fn batch(shape: Shape, transparent: bool, instances: Range<u32>) -> Batch {
        Batch {
            shape,
            transparent,
            instances,
        }
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
        assert_eq!(batches, vec![batch(Shape::Block, false, 0..1)]);
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
                batch(Shape::Block, false, 0..2),
                batch(Shape::Wedge, false, 2..4)
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
                batch(Shape::Sphere, false, 0..1),
                batch(Shape::Sphere, true, 1..2),
                batch(Shape::Block, true, 2..3),
                batch(Shape::Sphere, true, 3..4),
            ]
        );
        let z = |i: usize| instances[i].model[3][2];
        assert_eq!([z(1), z(2), z(3)], [-9.0, -5.0, -2.0]);
    }
}
