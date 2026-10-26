//! Turned UI text. glyphon only draws text upright, so each turned text is
//! drawn upright into its own place on one sheet, with its outline, shadows
//! and underlines, and the sheet is then drawn as a turned picture where
//! the text goes (`fs_sheet` in `ui.wgsl`).

use super::*;

/// The image key that stands for the turned-text sheet in a layer's images.
pub(super) const SHEET_KEY: u64 = u64::MAX;
/// Space left between texts on the sheet, in pixels, so smoothing doesn't
/// pick up the next one.
const SHEET_GAP: u32 = 2;
/// The sheet is at least this wide, so a few texts share a row.
const SHEET_MIN_WIDTH: u32 = 1024;

/// One UI text, shaped (see [`ScenePipeline::shape_ui_text`]).
pub(super) struct ShapedText {
    /// Its first buffer, in the list it was shaped into.
    pub(super) first: usize,
    /// Where its first line starts, in pixels from the top.
    pub(super) top: f32,
    pub(super) passes: Vec<text::Pass>,
    /// Its underlines and line-throughs.
    pub(super) lines: Vec<UiRectangle>,
}

/// What [`ScenePipeline::prepare_turned_text`] made ready.
#[derive(Default)]
pub(super) struct TurnedPrep {
    /// The shaped text, which must live until it's drawn.
    pub(super) buffers: Vec<Buffer>,
    /// The quad of each turned text, by its index in the UI list.
    pub(super) quads: HashMap<usize, Vec<UiVertex>>,
    /// Underlines and line-throughs, drawn onto the sheet before the text.
    pub(super) lines: Vec<UiVertex>,
    /// Whether anything is drawn on the sheet this frame.
    pub(super) drawn: bool,
}

impl ScenePipeline {
    /// Places each turned text in `ui` on the sheet, shapes it there and
    /// readies the sheet's text renderer, growing the sheet if it's too
    /// small. A text too big for the biggest sheet isn't drawn.
    pub(super) fn prepare_turned_text(
        &mut self,
        ui: &[UiDrawItem],
        viewport: (u32, u32),
    ) -> Result<TurnedPrep> {
        let turned: Vec<(usize, &UiText)> = ui
            .iter()
            .enumerate()
            .filter_map(|(index, item)| match item {
                UiDrawItem::Text(text)
                    if is_turned(text.rotation_degrees)
                        && text.size.iter().all(|n| n.is_finite() && *n > 0.0) =>
                {
                    Some((index, text))
                }
                _ => None,
            })
            .collect();
        let mut prep = TurnedPrep::default();
        if turned.is_empty() {
            return Ok(prep);
        }

        // Rows across the sheet, each as tall as its tallest text.
        let biggest = self.device.limits().max_texture_dimension_2d.min(8192);
        let slots: Vec<(f32, [u32; 2])> = turned
            .iter()
            .map(|(_, text)| {
                let margin = super::ui_pass::text_spill(text).ceil();
                let size = [
                    (text.size[0] + margin * 2.0).ceil() as u32,
                    (text.size[1] + margin * 2.0).ceil() as u32,
                ];
                (margin, size)
            })
            .collect();
        let width = slots
            .iter()
            .map(|(_, [w, _])| *w + SHEET_GAP)
            .max()
            .unwrap_or(0)
            .max(SHEET_MIN_WIDTH)
            .min(biggest);
        let (mut x, mut y, mut row) = (0u32, 0u32, 0u32);
        let mut placed = Vec::new();
        for (&(index, text), &(margin, [w, h])) in turned.iter().zip(&slots) {
            if w + SHEET_GAP > width || h + SHEET_GAP > biggest {
                continue;
            }
            if x + w + SHEET_GAP > width {
                x = 0;
                y += row;
                row = 0;
            }
            if y + h + SHEET_GAP > biggest {
                continue;
            }
            placed.push((index, text, margin, [x, y, w, h]));
            x += w + SHEET_GAP;
            row = row.max(h + SHEET_GAP);
        }
        let height = (y + row).max(1);
        let size = self.sheet_of_at_least((width, height), biggest);

        let mut areas = Vec::new();
        for &(index, text, margin, [sx, sy, w, h]) in &placed {
            let upright = UiText {
                position: [sx as f32 + margin, sy as f32 + margin],
                clip: None,
                rotation_degrees: 0.0,
                ..text.clone()
            };
            let shaped = self.shape_ui_text(&upright, &mut prep.buffers);
            prep.lines
                .extend(super::ui_pass::rectangle_vertices(&shaped.lines, size));
            for pass in shaped.passes {
                areas.push((upright.clone(), shaped.top, shaped.first + pass.layer, pass));
            }
            let slot = [sx as f32, sy as f32, w as f32, h as f32];
            let quad = super::ui_pass::turned_text_quad(text, slot, margin, size, viewport);
            if !quad.is_empty() {
                prep.quads.insert(index, quad);
            }
        }
        if areas.is_empty() && prep.lines.is_empty() {
            return Ok(prep);
        }
        prep.drawn = true;
        let (width, height) = size;
        self.turned_text
            .viewport
            .update(&self.queue, Resolution { width, height });
        let buffers = &prep.buffers;
        let text_areas = areas.iter().map(|(text, top, buffer, pass)| {
            let [dx, dy] = pass.offset;
            TextArea {
                buffer: &buffers[*buffer],
                left: text.position[0] + dx,
                top: top + dy,
                scale: 1.0,
                bounds: super::scene::text_bounds(text, pass.offset),
                default_color: glyph_color(pass.color),
                custom_glyphs: &[],
            }
        });
        self.turned_text
            .renderer
            .prepare(
                &self.device,
                &self.queue,
                &mut self.font_system,
                &mut self.text_atlas,
                &self.turned_text.viewport,
                text_areas,
                &mut self.swash_cache,
            )
            .context("could not prepare turned UI text")?;
        Ok(prep)
    }

    /// The sheet's size, after making one at least `needed` big (and at
    /// most `biggest` on a side) if the one there is smaller.
    fn sheet_of_at_least(&mut self, needed: (u32, u32), biggest: u32) -> (u32, u32) {
        if let Some((_, _, size)) = &self.turned_text.sheet
            && size.0 >= needed.0
            && size.1 >= needed.1
        {
            return *size;
        }
        let old = self
            .turned_text
            .sheet
            .as_ref()
            .map_or((0, 0), |sheet| sheet.2);
        let grow = |needed: u32, old: u32| needed.max(old).next_power_of_two().min(biggest);
        let size = (grow(needed.0, old.0), grow(needed.1, old.1));
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("turned UI text"),
            size: wgpu::Extent3d {
                width: size.0,
                height: size.1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: self.color_format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("turned UI text"),
            layout: &self.ui_image_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.ui_sampler),
                },
            ],
        });
        self.turned_text.sheet = Some((texture, group, size));
        size
    }
}
