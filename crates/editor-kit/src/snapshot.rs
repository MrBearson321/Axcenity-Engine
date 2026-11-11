//! Draws editor UI offscreen and saves it as a PNG, to look at in tests.

use std::sync::Arc;

use eframe::egui::{self, Pos2, Rect, Vec2};
use eframe::egui_wgpu::{RenderState, Renderer, ScreenDescriptor, wgpu};

/// Runs `frames` (each a list of input events and a UI function) and saves
/// the last frame drawn at `path`. Does nothing without a GPU.
pub fn snapshot(
    path: &std::path::Path,
    size: Vec2,
    frames: Vec<Vec<egui::Event>>,
    mut show: impl FnMut(&mut egui::Ui),
) {
    snapshot_rendered(path, size, frames, |ui, _| show(ui));
}

/// [`snapshot`], also handing `show` a GPU render state, so what draws with
/// the game's renderer (previews, the UI Designer's artboard) draws for real.
pub fn snapshot_rendered(
    path: &std::path::Path,
    size: Vec2,
    frames: Vec<Vec<egui::Event>>,
    show: impl FnMut(&mut egui::Ui, &RenderState),
) {
    // Half a second a frame, so fades and other transitions have finished
    // by the frame that's saved.
    let timed = frames
        .into_iter()
        .enumerate()
        .map(|(index, events)| (index as f64 * 0.5, events))
        .collect();
    snapshot_timed(path, size, timed, show);
}

/// [`snapshot_rendered`] with each frame's time given, to catch an
/// animation part way through.
pub fn snapshot_timed(
    path: &std::path::Path,
    size: Vec2,
    frames: Vec<(f64, Vec<egui::Event>)>,
    mut show: impl FnMut(&mut egui::Ui, &RenderState),
) {
    let instance = axcenity_render::adapters::instance();
    let Ok(adapter) = pollster::block_on(axcenity_render::adapters::adapter(&instance, None))
    else {
        eprintln!("skipped: no graphics adapter");
        return;
    };
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    let ctx = crate::theme::test_context();
    let renderer = Renderer::new(
        &device,
        wgpu::TextureFormat::Rgba8Unorm,
        eframe::egui_wgpu::RendererOptions {
            msaa_samples: 1,
            depth_stencil_format: None,
            dithering: false,
            predictable_texture_filtering: false,
        },
    );
    let render_state = RenderState {
        adapter: adapter.clone(),
        available_adapters: Vec::new(),
        instance: instance.clone(),
        device: device.clone(),
        queue: queue.clone(),
        target_format: wgpu::TextureFormat::Rgba8Unorm,
        renderer: Arc::new(egui::mutex::RwLock::new(renderer)),
        surface_config: eframe::egui_wgpu::SurfaceConfig::LOW_LATENCY,
    };
    let mut last = None;
    for (time, events) in frames {
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
            events,
            time: Some(time),
            ..Default::default()
        };
        let mut output = ctx.run_ui(input, |ui| show(ui, &render_state));
        let mut renderer = render_state.renderer.write();
        for (id, deltas) in output.textures_delta.set.drain() {
            for delta in deltas {
                renderer.update_texture(&device, &queue, id, &delta);
            }
        }
        for id in output.textures_delta.free.drain() {
            renderer.free_texture(&id);
        }
        last = Some(output);
    }
    let output = last.expect("at least one frame");
    let ppp = output.pixels_per_point;
    let jobs = ctx.tessellate(output.shapes, ppp);
    let (width, height) = ((size.x * ppp) as u32, (size.y * ppp) as u32);
    let screen = ScreenDescriptor {
        size_in_pixels: [width, height],
        pixels_per_point: ppp,
    };
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = texture.create_view(&Default::default());
    let mut encoder = device.create_command_encoder(&Default::default());
    let mut renderer = render_state.renderer.write();
    let commands = renderer.update_buffers(&device, &queue, &mut encoder, &jobs, &screen);
    {
        let pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: None,
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                depth_slice: None,
                view: &view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..Default::default()
        });
        renderer.render(&mut pass.forget_lifetime(), &jobs, &screen);
    }
    let row = (width * 4).next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: u64::from(row * height),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    encoder.copy_texture_to_buffer(
        texture.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(row),
                rows_per_image: None,
            },
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );
    queue.submit(commands.into_iter().chain([encoder.finish()]));
    buffer.slice(..).map_async(wgpu::MapMode::Read, |_| {});
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    let mapped = buffer
        .slice(..)
        .get_mapped_range()
        .expect("the buffer is mapped");
    let pixels: Vec<u8> = mapped
        .chunks(row as usize)
        .flat_map(|line| &line[..(width * 4) as usize])
        .copied()
        .collect();
    save_png(path, (width, height), &pixels);
}

/// Saves rows of RGBA `pixels`, `width` × `height`, as a PNG at `path`.
pub fn save_png(path: &std::path::Path, (width, height): (u32, u32), pixels: &[u8]) {
    let file = std::fs::File::create(path).unwrap();
    let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder
        .write_header()
        .unwrap()
        .write_image_data(pixels)
        .unwrap();
}
