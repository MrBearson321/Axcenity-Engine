//! The Import dialog's preview: the file's model as it is in the file
//! (its own materials), on a checkered floor, playing the clip picked in
//! the Animation tab. It has its own `Assets` holding only that model, so
//! nothing is read again and the project's assets are never touched.
//!
//! It also gives the Materials tab small pictures of each map: those
//! the model uses from its already-decoded images, picked ones read once.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use axcenity_anim::Clip;
use axcenity_assets::{Assets, Image, Model, Take};
use eframe::egui;
use eframe::egui_wgpu::RenderState;

use crate::anim_import::{AnimationImport, ClipRow};
use crate::animation::preview::Preview;
use crate::surface::MapFile;
use crate::theme::{self, icons};

/// The longest side of a map's picture, in pixels.
const THUMBNAIL: u32 = 64;

pub struct ImportPreview {
    assets: Assets,
    /// What the model is called in `assets`: the file's name.
    mesh_id: String,
    model: Arc<Model>,
    takes: Arc<Vec<Take>>,
    view: Preview,
    playing: bool,
    /// Seconds into the clip shown.
    time: f64,
    /// The clip row it was bound for, its length in seconds, and the
    /// clip, so it's bound again only when the row changes.
    bound: Option<(ClipRow, f64, Result<Clip, String>)>,
    /// The model's images by their file (as in [`MapFile::Model`]).
    images: HashMap<String, Arc<Image>>,
    /// Pictures made so far, by map file; `None` if it can't be read.
    thumbnails: HashMap<MapFile, Option<egui::TextureHandle>>,
}

impl ImportPreview {
    /// The preview of `model`, loaded from `file`, with the file's `takes`.
    pub fn new(file: &Path, model: Model, takes: Vec<Take>) -> Self {
        let folder = file.parent().unwrap_or(Path::new("."));
        let mesh_id = file
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        let assets = Assets::new(folder);
        let model = assets.insert(&mesh_id, model);
        let mut images = HashMap::new();
        for primitive in &model.primitives {
            let material = &primitive.material;
            let channels = [
                &material.metalness_texture,
                &material.roughness_texture,
                &material.occlusion_texture,
                &material.opacity_texture,
            ];
            let all = [
                &material.base_color_texture,
                &material.normal_texture,
                &material.emissive_texture,
                &material.specular_texture,
            ]
            .into_iter()
            .flatten()
            .chain(channels.into_iter().flatten().map(|channel| &channel.image));
            for image in all {
                if let Some(file) = &image.file {
                    images.entry(file.clone()).or_insert_with(|| image.clone());
                }
            }
        }
        let mut view = Preview::default();
        view.set_model(&mesh_id, &model);
        Self {
            assets,
            mesh_id,
            model,
            takes: Arc::new(takes),
            view,
            playing: true,
            time: 0.0,
            bound: None,
            images,
            thumbnails: HashMap::new(),
        }
    }

    /// The pose of clip `index` of `animation` now, moving the time on
    /// while it plays. `None` shows the model as it's bound (no clip, or a
    /// clip that can't be played on it).
    fn posed(
        &mut self,
        ui: &egui::Ui,
        animation: Option<&AnimationImport>,
        index: usize,
    ) -> Option<Arc<axcenity_assets::Posed>> {
        let animation = animation?;
        let rig = self.model.rig.clone()?;
        let row = animation.clips.get(index)?;
        if self.bound.as_ref().is_none_or(|(bound, _, _)| bound != row) {
            let file = animation.clip_file(index, &self.mesh_id);
            let duration = file.as_ref().map_or(0.0, |file| file.duration);
            let clip = file
                .ok_or_else(|| "the clip's take isn't in the file".to_owned())
                .and_then(|file| Clip::bind(file, Some(self.takes.clone()), Some(&rig)));
            self.bound = Some((row.clone(), duration, clip));
            self.time = 0.0;
        }
        let Some((_, duration, Ok(clip))) = &self.bound else {
            return None;
        };
        if self.playing {
            // Read in `input`, used outside it (see the crate's CLAUDE.md).
            let dt = ui.input(|input| input.stable_dt).min(0.1);
            self.time += f64::from(dt);
            ui.ctx().request_repaint();
        }
        // Always round and round in the preview, even for a clip that
        // doesn't loop in the game.
        let phase = (self.time / duration.max(1e-3)).rem_euclid(1.0);
        let mut pose = rig.rest_pose();
        clip.sample(phase, &rig, &mut pose);
        self.model.pose(&pose).map(Arc::new)
    }

    /// Draws the model `height` points tall, playing clip `clip` of
    /// `animation` if it's animated, with play/pause under it.
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        render_state: Option<&RenderState>,
        animation: Option<&AnimationImport>,
        clip: usize,
        height: f32,
    ) {
        let posed = self.posed(ui, animation, clip);
        let playing_clip = posed
            .is_some()
            .then(|| animation.and_then(|animation| animation.clips.get(clip)))
            .flatten()
            .map(|row| row.name.clone());
        self.view
            .show(ui, render_state, &self.assets, posed, height);
        let palette = theme::palette(ui.ctx());
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            match playing_clip {
                Some(name) => {
                    let (icon, hover) = if self.playing {
                        (icons::PAUSE, "Pause")
                    } else {
                        (icons::PLAY, "Play")
                    };
                    if ui.small_button(icon).on_hover_text(hover).clicked() {
                        self.playing = !self.playing;
                    }
                    ui.label(
                        egui::RichText::new(format!("Playing {name}"))
                            .size(theme::type_scale::SMALL)
                            .color(palette.text_weak),
                    );
                }
                None => {
                    ui.label(
                        egui::RichText::new("As it is in the file, with its own materials")
                            .size(theme::type_scale::SMALL)
                            .color(palette.text_weak),
                    );
                }
            }
        });
    }

    /// A small picture of the map `file`, made the first time.
    pub fn thumbnail(
        &mut self,
        ctx: &egui::Context,
        file: &MapFile,
    ) -> Option<egui::TextureHandle> {
        if let Some(made) = self.thumbnails.get(file) {
            return made.clone();
        }
        let image = match file {
            MapFile::Model(path) => self.images.get(path).cloned(),
            // A picked file is read now, once: the user just chose it.
            MapFile::Picked(path) => path
                .parent()
                .zip(path.file_name())
                .and_then(|(folder, name)| {
                    axcenity_assets::read_texture(folder, &name.to_string_lossy()).ok()
                })
                .map(Arc::new),
        };
        let made = image.map(|image| {
            ctx.load_texture(
                format!("import map {file:?}"),
                shrink(&image),
                egui::TextureOptions::LINEAR,
            )
        });
        self.thumbnails.insert(file.clone(), made.clone());
        made
    }
}

/// `image` at most [`THUMBNAIL`] pixels on its longest side, taking the
/// nearest pixel: it's only a hint of what the map looks like.
fn shrink(image: &Image) -> egui::ColorImage {
    let (width, height) = (image.width.max(1), image.height.max(1));
    let scale = (THUMBNAIL as f32 / width.max(height) as f32).min(1.0);
    let w = ((width as f32 * scale).round() as u32).max(1);
    let h = ((height as f32 * scale).round() as u32).max(1);
    let mut rgba = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h {
        let sy = (y * height / h).min(height - 1);
        for x in 0..w {
            let sx = (x * width / w).min(width - 1);
            let at = ((sy * width + sx) * 4) as usize;
            match image.rgba().get(at..at + 4) {
                Some(pixel) => rgba.extend_from_slice(pixel),
                None => rgba.extend_from_slice(&[0, 0, 0, 255]),
            }
        }
    }
    egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], &rgba)
}
