//! The wgpu side: GPU setup, the window surface, buffers and drawing.
//!
//! `renderer.rs` owns the GPU types and draws a frame. `lighting.rs` plans
//! uniforms, tiles, shadows and batches. `post.rs` blooms and tone-maps.
//! `ui_pass.rs` builds screen-space quads and the UI and sky pipelines.
//! `gpu.rs` uploads meshes, textures and materials. `sprites.rs` draws particles.
//! `terrain.rs` draws terrain chunks with their blended layers.
//! `gpu_timer.rs` measures the GPU's time per frame, for the editor's profiler.

use std::collections::hash_map::Entry;
use std::collections::{HashMap, HashSet};
use std::ops::Range;
use std::time::Duration;

use crate::draw_list::{
    DrawList, Shape, TextureSource, UiDrawItem, UiRectangle, UiText, is_turned,
};
use crate::graphics::{Capabilities, EffectiveGraphics, GraphicsSettings, ShadowMapSize};
use crate::mesh::{self};
use anyhow::{Context, Result};
use bytemuck::{Pod, Zeroable};
use glyphon::{
    Buffer, Cache as GlyphCache, FontSystem, Resolution, SwashCache, TextArea, TextAtlas,
    TextBounds, TextRenderer as GlyphTextRenderer, Viewport as TextViewport,
};

mod gpu;
mod gpu_timer;
mod lighting;
mod post;
mod sprites;
mod terrain;
pub(crate) mod text;
mod ui_pass;

#[cfg(test)]
use crate::draw_list::{ChannelMap, MeshSource};
#[cfg(test)]
use glam::Vec3;
#[cfg(test)]
use gpu::mip_levels;
use gpu::{
    create_depth_view, create_frame_bind_group, create_instance_buffer, create_material,
    create_pipeline, create_shadow_pipeline, create_shadow_views, create_target_texture,
    create_texture, drawable_size, shader_module, update_custom_mesh, upload_custom_mesh,
    upload_mesh, upload_texture,
};
#[cfg(test)]
use lighting::{cascade_ends, light_raw};
use lighting::{
    frame_uniform, linear_color, maps_uniform_of, material_key, material_textures, plan_draws,
    plan_lights, post_drawn, post_uniform, shadows_drawn, view_projection,
};
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
    /// False when the device can't take timestamps, so `gpu_time` stays `None`.
    pub gpu_timing: bool,
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

    /// The graphics values in use, and why any were lowered.
    pub fn graphics(&self) -> &EffectiveGraphics {
        &self.scene.graphics
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
