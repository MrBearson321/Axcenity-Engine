//! The connection to the graphics card and the window's drawing surface.
//!
//! This is temporary scaffolding: in Phase 2 drawing moves into the `render`
//! crate, which takes a draw list instead of knowing about windows.

use std::sync::Arc;

use anyhow::{Context, Result};
use winit::window::Window;

/// The color the window is cleared to each frame (linear color space).
const SKY_COLOR: wgpu::Color = wgpu::Color {
    r: 0.35,
    g: 0.6,
    b: 0.9,
    a: 1.0,
};

pub struct Gpu {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    /// True while the window is minimized (zero-sized), when there's nothing to draw.
    minimized: bool,
}

impl Gpu {
    pub async fn new(window: Arc<Window>) -> Result<Self> {
        let size = window.inner_size();

        // `Instance` picks a graphics API: DirectX 12 or Vulkan on Windows,
        // Metal on macOS, Vulkan on Linux.
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
        let surface = instance
            .create_surface(window)
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

        let (width, height) = drawable_size(size.width, size.height).unwrap_or((1, 1));
        let config = surface
            .get_default_config(&adapter, width, height)
            .context("the graphics card cannot draw to this window")?;
        surface.configure(&device, &config);

        Ok(Self {
            surface,
            device,
            queue,
            config,
            minimized: false,
        })
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        let Some((width, height)) = drawable_size(width, height) else {
            self.minimized = true;
            return;
        };
        self.minimized = false;
        self.config.width = width;
        self.config.height = height;
        self.surface.configure(&self.device, &self.config);
    }

    pub fn render(&mut self, window: &Window) -> Result<()> {
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

        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("frame"),
            });

        // A render pass that only clears the screen. It ends when dropped,
        // which `drop` makes explicit before the encoder is finished.
        let clear_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("clear to sky"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(SKY_COLOR),
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..Default::default()
        });
        drop(clear_pass);

        self.queue.submit([encoder.finish()]);
        window.pre_present_notify();
        frame.present();
        Ok(())
    }
}

/// Returns the size to draw at, or `None` when the window has no area
/// (minimized windows report 0×0, and a GPU surface can't be zero-sized).
fn drawable_size(width: u32, height: u32) -> Option<(u32, u32)> {
    (width > 0 && height > 0).then_some((width, height))
}

#[cfg(test)]
mod tests {
    use super::drawable_size;

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
}
