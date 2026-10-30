//! The wgpu side: GPU setup, the window surface, buffers and drawing.
//!
//! `renderer.rs` owns the GPU types and draws a frame. `lighting.rs` plans
//! uniforms, tiles, shadows and batches. `post.rs` blooms, tone-maps and runs FXAA.
//! `ui_pass.rs` builds screen-space quads and the UI and sky pipelines.
//! `gpu.rs` uploads meshes, textures and materials. `sprites.rs` draws particles.
//! `terrain.rs` draws terrain chunks with their blended layers.
//! `occlusion.rs` draws the depth and normal prepass and ambient occlusion.
//! `environment.rs` bakes the sky into environment maps for reflections and
//! sky light.
//! `skinning.rs` uploads animated meshes' joints and weights, and each
//! frame's joints, so the vertex shader poses them.
//! `realtime_gi.rs` traces light probes again a few each frame, while
//! real-time GI is on.
//! `fog.rs` packs the frame's fog and, while volumetric fog is on, works
//! out the sun's and lamps' light in it.
//! `clouds.rs` packs the frame's clouds and keeps how far the wind has
//! carried them.
//! `culling.rs` tests bounds against each view, so what a view can't see
//! isn't drawn.
//! `gpu_timer.rs` measures the GPU's time per frame and per pass, for the
//! editor's profiler and the benchmarks.

use std::collections::hash_map::Entry;
use std::collections::{HashMap, HashSet};
use std::ops::Range;
use std::time::Duration;

use crate::draw_list::{
    DrawList, Shape, Skin, TextureSource, UiDrawItem, UiRectangle, UiText, is_turned,
};
use crate::graphics::{
    Capabilities, EffectiveGraphics, GraphicsSettings, POINT_SHADOW_FACES, ShadowMapSize,
};
use crate::mesh::{self};
use anyhow::{Context, Result};
use bytemuck::{Pod, Zeroable};
pub use culling::CullingBounds;
use glyphon::{
    Buffer, Cache as GlyphCache, FontSystem, Resolution, SwashCache, TextArea, TextAtlas,
    TextBounds, TextRenderer as GlyphTextRenderer, Viewport as TextViewport,
};
pub use gpu_timer::FramePass;
pub use probe_bake::{LIGHT_PROBE_BAKE_SIZE, PROBE_BAKE_SIZE};
pub use surface_shaders::{ShaderError, SurfaceShader};

mod atmosphere_sky;
mod clouds;
mod culling;
mod environment;
mod fog;
mod gpu;
mod gpu_timer;
mod grading;
mod lens;
mod light_probes;
mod lighting;
mod lod;
mod occlusion;
mod post;
mod probe_bake;
mod probes;
mod realtime_gi;
mod skinning;
mod sprites;
mod surface_shaders;
mod temporal;
mod terrain;
pub(crate) mod text;
mod texture_budget;
mod ui_pass;
mod water;

#[cfg(test)]
use crate::draw_list::{ChannelMap, MeshSource};
use culling::{Bounds, Frustum, spheres_touch};
#[cfg(test)]
use glam::Vec3;
#[cfg(test)]
use gpu::mip_levels;
use gpu::{
    create_frame_bind_group, create_instance_buffer, create_material, create_pipeline,
    create_shadow_pipeline, create_shadow_views, create_target_texture, create_texture,
    drawable_size, shader_module, target_bytes, update_custom_mesh, upload_custom_mesh,
    upload_mesh, upload_texture,
};
use lighting::{
    DrawPlan, camera_view_projection, casters_in, frame_uniform, linear_color, maps_uniform_of,
    material_key, material_textures, plan_draws, plan_lights, post_drawn, post_uniform,
    shadows_drawn,
};
#[cfg(test)]
use lighting::{cascade_ends, light_raw, uv_matrix, view_projection};
use lod::{Draw, LodStates, Resolved};
use occlusion::{create_edge_pipeline, edge_uniform, occlusion_uniform};
use skinning::Skinning;
use std::sync::Arc;

mod scene;
mod turned_text;
mod types;

use text::TextLine;
use turned_text::{SHEET_KEY, ShapedText};
use types::*;
use ui_pass::{
    UiLayer, create_sky_pipeline, create_ui_image_pipeline, create_ui_pipeline, glyph_color,
    image_quad, ui_layers, ui_vertices,
};

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

/// What the last frame drawn cost, for the editor's profiler.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RenderStats {
    /// Indexed draws, including the shadow passes'.
    pub draw_calls: u32,
    /// Triangles those draws sent, counting every instance.
    pub triangles: u64,
    /// The GPU's time for a recent frame (a frame or two behind), or `None`
    /// before the first reading or when it can't be measured.
    pub gpu_time: Option<Duration>,
    /// False when the device can't take timestamps, so `gpu_time` and
    /// `pass_times` stay `None`.
    pub gpu_timing: bool,
    /// The GPU's time for each pass of the frame `gpu_time` measured, in
    /// [`FramePass::ALL`]'s order: `None` for a pass that frame didn't draw
    /// (see [`RenderStats::pass_time`]).
    pub pass_times: [Option<Duration>; FramePass::COUNT],
    /// Items in the draw list.
    pub items_submitted: u32,
    /// Items the scene pass drew: those submitted in the camera's view,
    /// less any whose mesh couldn't be uploaded.
    pub items_drawn: u32,
    /// Instanced draws in the scene pass, one per mesh and material.
    pub batches: u32,
    /// Items outside the camera's view, so not drawn by the scene pass.
    pub items_culled: u32,
    /// Casters drawn into the shadow views (each cascade and shadowing spot
    /// light), counted once per view.
    pub shadow_casters_drawn: u32,
    /// Casters left out of a shadow view they couldn't reach, counted once
    /// per view.
    pub shadow_casters_culled: u32,
    /// Sprites (particles) outside the camera's view.
    pub sprites_culled: u32,
    /// Bytes of textures on the GPU, every mip level included, counted when
    /// uploaded and when freed.
    pub texture_bytes: u64,
    /// Bytes of mesh vertices and indices on the GPU, counted the same way.
    pub mesh_bytes: u64,
    /// Bytes of the frame's own targets: HDR, depth, MSAA, bloom, FXAA,
    /// prepass and ambient occlusion targets, the shadow maps, the sky's
    /// environment maps and sky image, and the turned-text sheet.
    pub target_bytes: u64,
    /// Bytes of buffers: instances, lights, tiles, uniforms and UI vertices.
    pub buffer_bytes: u64,
}

impl RenderStats {
    /// The GPU's time for `pass`, if it was measured and drawn.
    pub fn pass_time(&self, pass: FramePass) -> Option<Duration> {
        self.pass_times[pass as usize]
    }
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
        let capabilities = Capabilities::read(&device, Some(&adapter.get_info()));

        Ok(Self {
            surface,
            minimized: false,
            scene: ScenePipeline::new(device, queue, config.format, width, height, capabilities),
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

    /// What the GPU can do, which the graphics settings are lowered to fit.
    pub fn capabilities(&self) -> &Capabilities {
        &self.scene.capabilities
    }

    /// Uses `settings` from the next frame (see [`GraphicsSettings::effective`]).
    pub fn set_graphics(&mut self, settings: &GraphicsSettings) {
        self.scene.set_graphics(settings);
    }

    /// The graphics values in use, and why any were lowered.
    pub fn graphics(&self) -> &EffectiveGraphics {
        &self.scene.graphics
    }

    /// The materials' surface shaders that didn't compile since the last
    /// call, each once. Their items draw in glowing magenta checks.
    pub fn take_shader_errors(&mut self) -> Vec<ShaderError> {
        self.scene.surface_shaders.take_errors()
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
        self.scene
            .draw(list, &view, aspect, (self.config.width, self.config.height))?;
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
            Capabilities::read(device, None),
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
    pub fn render(&mut self, list: &DrawList) -> Result<()> {
        let (width, height) = self.size();
        self.scene.draw(
            list,
            &self.draw_view,
            width as f32 / height as f32,
            (width, height),
        )
    }

    /// What the last frame drawn cost, for the editor's profiler.
    pub fn stats(&self) -> RenderStats {
        self.scene.stats()
    }

    /// What the GPU can do, which the graphics settings are lowered to fit.
    pub fn capabilities(&self) -> &Capabilities {
        &self.scene.capabilities
    }

    /// Uses `settings` from the next frame (see [`GraphicsSettings::effective`]).
    pub fn set_graphics(&mut self, settings: &GraphicsSettings) {
        self.scene.set_graphics(settings);
    }

    /// For the editor's culling overlay: while `camera` is set, items and
    /// sprites are culled as if seen from it (at this texture's aspect)
    /// rather than from the draw list's camera, and each item's bounds are
    /// kept for [`culling_bounds`](Self::culling_bounds). Moving the real
    /// camera away then shows what culling left out. `None` culls normally.
    pub fn freeze_culling(&mut self, camera: Option<crate::Camera>) {
        self.scene.frozen_view = camera;
        if camera.is_none() {
            self.scene.culling_bounds = Vec::new();
        }
    }

    /// Each item's bounding sphere in the last frame, and whether it was
    /// drawn, while culling is frozen (see [`freeze_culling`](Self::freeze_culling)).
    /// Empty otherwise.
    pub fn culling_bounds(&self) -> &[CullingBounds] {
        &self.scene.culling_bounds
    }

    /// Draws as if the GPU could do no more than `limits` (each value the
    /// lower of the two, so nothing is raised), with the settings lowered
    /// to fit and noted: for checking fallbacks on a GPU that can do more.
    pub fn limit_capabilities(&mut self, limits: &Capabilities) {
        self.scene.limit_capabilities(limits);
    }

    /// The graphics values in use, and why any were lowered.
    pub fn graphics(&self) -> &EffectiveGraphics {
        &self.scene.graphics
    }

    /// The materials' surface shaders that didn't compile since the last
    /// call, each once. Their items draw in glowing magenta checks.
    pub fn take_shader_errors(&mut self) -> Vec<ShaderError> {
        self.scene.surface_shaders.take_errors()
    }

    /// Reads the texture back to the CPU as rows of RGBA bytes. Slow: for
    /// tests and screenshots.
    pub fn read_pixels(&self) -> Vec<u8> {
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

#[cfg(test)]
mod tests;
