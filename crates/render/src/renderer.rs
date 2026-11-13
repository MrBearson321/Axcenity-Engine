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
    read_capabilities,
};
use crate::mesh::{self};
use anyhow::{Context, Result};
use bytemuck::{Pod, Zeroable};
pub use culling::CullingBounds;
#[cfg(feature = "debug-views")]
pub use debug_views::DebugView;
use glyphon::{
    Buffer, Cache as GlyphCache, FontSystem, Resolution, SwashCache, TextArea, TextAtlas,
    TextBounds, TextRenderer as GlyphTextRenderer, Viewport as TextViewport,
};
pub use gpu_particles::MAX_GPU_PARTICLES;
pub use gpu_timer::FramePass;
pub use probe_bake::{BakedProbe, LIGHT_PROBE_BAKE_SIZE, LightProbeBaker, PROBE_BAKE_SIZE};
pub use ray_tracing::RayTracingStats;
pub use surface_shaders::{ShaderError, SurfaceShader};

mod atmosphere_sky;
mod clouds;
mod culling;
#[cfg(feature = "debug-views")]
mod debug_views;
mod decals;
mod environment;
mod fog;
mod gpu;
mod gpu_particles;
mod gpu_timer;
mod grading;
mod grid;
mod lens;
mod light_probes;
mod lighting;
mod lod;
mod occlusion;
mod parallel;
mod post;
mod probe_bake;
mod probes;
mod ray_ao;
mod ray_gi;
mod ray_shadows;
mod ray_tracing;
mod realtime_gi;
mod skinning;
mod sprites;
mod surface_shaders;
mod temporal;
mod terrain;
pub(crate) mod text;
mod texture_budget;
mod ui_pass;
mod uploads;
mod water;

#[cfg(test)]
use crate::draw_list::{ChannelMap, MeshSource};
use culling::{Bounds, Frustum, Reach, ShadowView};
#[cfg(test)]
use glam::Vec3;
#[cfg(test)]
use gpu::mip_levels;
use gpu::{
    DepthUse, create_frame_bind_group, create_instance_buffer, create_material, create_pipeline,
    create_pipeline_for, create_shadow_pipeline, create_shadow_views, create_target_texture,
    create_texture, drawable_size, shader_module, target_bytes, update_custom_mesh,
    upload_custom_mesh, upload_mesh, upload_texture,
};
use lighting::{
    DrawPlan, FrameUses, camera_view_projection, casters_in_views, frame_uniform, linear_color,
    maps_uniform_of, material_textures, plan_draws, plan_lights, post_drawn, post_uniform,
    shadows_drawn,
};
#[cfg(test)]
use lighting::{cascade_ends, light_raw, material_key, uv_matrix, view_projection};
use lod::{Draw, LodStates, Resolved};
use occlusion::{create_edge_pipeline, edge_uniform, occlusion_uniform};
use parallel::write_pieces;
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
    /// How long the last frame was blocked getting its picture from the
    /// window and handing it back (vertical sync, or the GPU behind).
    present_wait: Duration,
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

/// How many levels of detail an item can be drawn at, its own mesh included
/// (see [`RenderStats::lod_items`]).
pub const LOD_LEVELS: usize = lod::LOD_SCREEN_SIZES.len() + 1;

/// The most cascades the sun's shadows are drawn in (Ultra's four).
pub const SHADOW_CASCADE_COUNT: usize = gpu_timer::CASCADES;

/// Below these sizes on screen (the share of the view's height an item's
/// bounding sphere covers), an item draws its first, then second, simpler
/// copy; it switches back once 15% past the threshold.
pub const LOD_SCREEN_SIZES: [f32; LOD_LEVELS - 1] = lod::LOD_SCREEN_SIZES;

/// What the last frame drawn cost, for the editor's profiler.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RenderStats {
    /// Indexed draws, including the shadow passes'.
    pub draw_calls: u32,
    /// Triangles those draws sent, counting every instance.
    pub triangles: u64,
    /// The corners of the meshes those draws sent, counting every instance.
    pub vertices: u64,
    /// The GPU's time for a recent frame (a frame or two behind), or `None`
    /// before the first reading or when it can't be measured.
    pub gpu_time: Option<Duration>,
    /// False when the device can't take timestamps, so `gpu_time` and
    /// `pass_times` stay `None`.
    pub gpu_timing: bool,
    /// How many GPU measurements have come back so far: a reader that keeps
    /// every frame's time (`--bench`) takes `gpu_time` only when this grows.
    pub gpu_readings: u64,
    /// The GPU's time for each of the sun's shadow cascades (nearest
    /// first), within the Shadows pass: `None` for one that frame didn't
    /// draw (off, or kept from an earlier frame while the far cascades take
    /// turns).
    pub cascade_times: [Option<Duration>; SHADOW_CASCADE_COUNT],
    /// Casters and triangles drawn into each cascade this frame.
    pub cascade_casters: [u32; SHADOW_CASCADE_COUNT],
    pub cascade_triangles: [u64; SHADOW_CASCADE_COUNT],
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
    /// Projected decals drawn, and those left out: outside the view, past
    /// the graphics settings' count or distance, or all of them at Low.
    pub decals_drawn: u32,
    pub decals_left_out: u32,
    /// Items in the camera's view whose mesh has levels of detail, by the
    /// level they're drawn at: 0 is the mesh itself, then its simpler
    /// copies. An item cross-fading between two counts at the one coming in.
    pub lod_items: [u32; LOD_LEVELS],
    /// Materials' surface shaders compiled into pipelines, and those that
    /// don't compile (drawn in the error color).
    pub surface_shaders: u32,
    pub broken_shaders: u32,
    /// Particles kept on the GPU: every GPU particle emitter's capacity
    /// (alive or not, as nothing is read back).
    pub gpu_particles: u32,
    /// Bytes of textures on the GPU, every mip level included, counted when
    /// uploaded and when freed.
    pub texture_bytes: u64,
    /// Bytes of mesh vertices and indices on the GPU, counted the same way.
    pub mesh_bytes: u64,
    /// Bytes of the frame's own targets: HDR, depth, MSAA, bloom, FXAA,
    /// prepass and ambient occlusion targets, the shadow maps, the sky's
    /// environment maps and sky image, and the turned-text sheet.
    pub target_bytes: u64,
    /// Bytes of buffers: instances, lights, tiles, uniforms, UI vertices
    /// and GPU particles.
    pub buffer_bytes: u64,
    /// Hardware ray tracing's acceleration structures this frame: all zero
    /// while nothing traces rays.
    pub ray_tracing: RayTracingStats,
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
        // DirectX 12 on Windows, Metal on macOS, Vulkan on Linux
        // (`adapters`).
        let starting = std::time::Instant::now();
        let instance = crate::adapters::instance();
        log::debug!(
            "graphics instance made in {} ms",
            starting.elapsed().as_millis()
        );
        let surface = instance
            .create_surface(target)
            .context("could not create a drawing surface for the window")?;
        let adapter = crate::adapters::adapter(&instance, Some(&surface))
            .await
            .context("no compatible graphics card found")?;
        log::info!(
            "using graphics adapter: {:?} ({} ms since the instance)",
            adapter.get_info(),
            starting.elapsed().as_millis()
        );

        let mut descriptor = wgpu::DeviceDescriptor {
            label: Some("axcenity device"),
            // Cooked textures go up as BC7 where the GPU reads it
            // (every desktop GPU); otherwise they're decoded. Timestamps
            // are asked for where the GPU has them, for the frame-rate
            // readout and `--bench`; frames are only timed while
            // `set_gpu_timing` is on.
            required_features: adapter.features()
                & (wgpu::Features::TEXTURE_COMPRESSION_BC | wgpu::Features::TIMESTAMP_QUERY),
            ..Default::default()
        };
        // Hardware ray tracing where the GPU has it, for the optional ray
        // traced effects.
        crate::adapters::with_ray_queries(&adapter, &mut descriptor);
        let (device, queue) = adapter
            .request_device(&descriptor)
            .await
            .context("could not open the graphics device")?;
        log::debug!(
            "graphics device open {} ms since the instance",
            starting.elapsed().as_millis()
        );

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
        // `AXCENITY_VSYNC=off` draws as fast as the GPU can (no wait for the
        // display), for benchmarks such as the cross-engine XB10k scene.
        if vsync_off(std::env::var("AXCENITY_VSYNC").ok().as_deref()) {
            config.present_mode = wgpu::PresentMode::AutoNoVsync;
        }
        surface.configure(&device, &config);
        let capabilities = read_capabilities(&device, Some(&adapter.get_info()));
        let building = std::time::Instant::now();
        let mut scene =
            ScenePipeline::new(device, queue, config.format, width, height, capabilities);
        scene.time_gpu = false;
        log::info!(
            "graphics pipelines ready in {} ms",
            building.elapsed().as_millis()
        );

        Ok(Self {
            surface,
            minimized: false,
            scene,
            config,
            present_wait: Duration::ZERO,
        })
    }

    /// Times the GPU's frame and passes (in [`Renderer::stats`]) while on,
    /// where the GPU can take timestamps. Off at first: a timed frame
    /// costs a few tiny GPU passes and a readback.
    pub fn set_gpu_timing(&mut self, on: bool) {
        self.scene.time_gpu = on;
    }

    /// Whether the GPU can be timed at all.
    pub fn can_time_gpu(&self) -> bool {
        self.scene.gpu_timer.is_some()
    }

    /// Waits for the display's refresh before showing each frame (on, the
    /// default), or shows frames as soon as they're drawn. `--bench` turns
    /// it off so frame times are the engine's, not the display's.
    pub fn set_vsync(&mut self, on: bool) {
        let mode = if on {
            wgpu::PresentMode::AutoVsync
        } else {
            wgpu::PresentMode::AutoNoVsync
        };
        if self.config.present_mode != mode {
            self.config.present_mode = mode;
            if !self.minimized {
                self.surface.configure(&self.scene.device, &self.config);
            }
        }
    }

    /// What the last frame drawn cost: counts, memory and, while
    /// [`Renderer::set_gpu_timing`] is on, the GPU's times.
    pub fn stats(&self) -> RenderStats {
        self.scene.stats()
    }

    /// How long the last frame was blocked on the window: getting a picture
    /// to draw into and presenting it. Vertical sync, or a GPU frames
    /// behind, shows up here rather than as work.
    pub fn present_wait(&self) -> Duration {
        self.present_wait
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

    /// While on (world streaming, Phase 80), meshes and color textures not
    /// yet on the GPU are uploaded on a thread of their own, and appear a
    /// few frames later, instead of stalling the frame that first draws
    /// them. Off by default.
    pub fn set_background_uploads(&mut self, on: bool) {
        self.scene.set_background_uploads(on);
    }

    /// How many background uploads are still on their way.
    pub fn pending_uploads(&self) -> usize {
        self.scene.pending_uploads()
    }

    /// The graphics values in use, and why any were lowered.
    pub fn graphics(&self) -> &EffectiveGraphics {
        &self.scene.graphics
    }

    /// Shows `view` in place of the lit scene from the next frame, or the
    /// lit scene for `None`: the player's console `wireframe` (Phase 71).
    /// Built with the `debug-views` feature, as on [`TextureRenderer`].
    #[cfg(feature = "debug-views")]
    pub fn set_debug_view(&mut self, view: Option<DebugView>) {
        self.scene.set_debug_view(view);
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
            self.present_wait = Duration::ZERO;
            return Ok(());
        }

        let acquiring = std::time::Instant::now();
        let acquired = self.surface.get_current_texture();
        self.present_wait = acquiring.elapsed();
        let frame = match acquired {
            wgpu::CurrentSurfaceTexture::Success(frame) => frame,
            // The surface no longer matches the window (for example, it moved
            // to another monitor). Reconfigure and draw on the next frame.
            wgpu::CurrentSurfaceTexture::Lost | wgpu::CurrentSurfaceTexture::Outdated => {
                self.surface.configure(&self.scene.device, &self.config);
                return Ok(());
            }
            // Still drawable, though no longer an exact match for the window
            // (wgpu 25 handed these over as `Ok` too). A resize reconfigures.
            wgpu::CurrentSurfaceTexture::Suboptimal(frame) => frame,
            // The GPU took too long to hand over a frame, or the window is
            // hidden. Skip this one.
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {
                return Ok(());
            }
            wgpu::CurrentSurfaceTexture::Validation => {
                anyhow::bail!("could not get the next frame to draw into (validation error)")
            }
        };

        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let aspect = self.config.width as f32 / self.config.height as f32;
        self.scene
            .draw(list, &view, aspect, (self.config.width, self.config.height))?;
        before_present();
        let presenting = std::time::Instant::now();
        self.scene.queue.present(frame);
        self.present_wait += presenting.elapsed();
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
            read_capabilities(device, None),
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

    /// The sky's average light in the last frame drawn (linear red, green
    /// and blue; the first of its spherical harmonics): what a light probe
    /// bake's [`ProbeSky::band0`](crate::ProbeSky::band0) is measured as.
    pub fn sky_band0(&self) -> [f32; 3] {
        self.scene.sky_band0.to_array()
    }

    /// Uses `settings` from the next frame (see [`GraphicsSettings::effective`]).
    pub fn set_graphics(&mut self, settings: &GraphicsSettings) {
        self.scene.set_graphics(settings);
    }

    /// See [`Renderer::set_background_uploads`].
    pub fn set_background_uploads(&mut self, on: bool) {
        self.scene.set_background_uploads(on);
    }

    /// See [`Renderer::pending_uploads`].
    pub fn pending_uploads(&self) -> usize {
        self.scene.pending_uploads()
    }

    /// Asks for every mesh (with its levels of detail) and color texture of
    /// `list`'s items to be uploaded, seen or not, so that looking around
    /// later doesn't wait for them: the editor does this behind its loading
    /// screen. Does nothing unless background uploads are on
    /// ([`set_background_uploads`](Self::set_background_uploads)); count
    /// them with [`pending_uploads`](Self::pending_uploads).
    pub fn preload(&mut self, list: &DrawList) {
        self.scene.preload(list);
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

    /// The editor's LOD preview: draws the item with this
    /// [`crate::DrawItem::id`] (`scene::item_id`) at this level whatever its
    /// size, or every item as usual for `None`. Only what's drawn changes.
    pub fn preview_lod(&mut self, item: Option<(u64, usize)>) {
        self.scene.lods.forced = item;
    }

    /// The level of detail the item with this [`crate::DrawItem::id`] was
    /// last drawn at, if its mesh has levels and it was drawn lately.
    pub fn lod_level_of(&self, id: u64) -> Option<usize> {
        self.scene.lods.level_of(id)
    }

    /// Shows `view` in place of the lit scene from the next frame, or the
    /// lit scene for `None` (see [`DebugView`]). Editor only: built with
    /// the `debug-views` feature. Nothing is kept for the debug views while
    /// the lit scene is shown.
    #[cfg(feature = "debug-views")]
    pub fn set_debug_view(&mut self, view: Option<DebugView>) {
        self.scene.set_debug_view(view);
    }

    /// Whether anything is kept for a debug view: only while one is shown.
    #[cfg(all(test, feature = "debug-views"))]
    pub(crate) fn debug_view_alive(&self) -> bool {
        self.scene.debug_views.is_some()
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
        device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
        let mapped = buffer
            .slice(..)
            .get_mapped_range()
            .expect("the buffer is mapped");
        mapped
            .chunks(row_bytes as usize)
            .flat_map(|row| &row[..(width * 4) as usize])
            .copied()
            .collect()
    }
}

#[cfg(test)]
mod tests;

/// Whether `AXCENITY_VSYNC`'s value asks for no vsync.
fn vsync_off(value: Option<&str>) -> bool {
    value.is_some_and(|v| v.eq_ignore_ascii_case("off") || v == "0")
}

#[cfg(test)]
mod vsync_tests {
    use super::vsync_off;

    #[test]
    fn only_off_or_zero_turns_vsync_off() {
        assert!(vsync_off(Some("off")));
        assert!(vsync_off(Some("OFF")));
        assert!(vsync_off(Some("0")));
        assert!(!vsync_off(Some("on")));
        assert!(!vsync_off(None));
    }
}
