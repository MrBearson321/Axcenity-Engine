//! The Assets panel: the project's asset files (models, prefabs, images,
//! textures, sounds and particle textures) and the built-in props, as tiles
//! to drag into the viewport.
//!
//! Files are found by walking the project's `assets` folder, again every
//! couple of seconds while the panel is shown, so files copied in by other
//! programs turn up without reopening the project. What a file is comes
//! from its extension and folder:
//!
//! - `.gltf`, `.glb` and `.fbx` are models, `*.prefab.json` prefabs, and
//!   `.wav`, `.ogg`, `.mp3` and `.flac` sounds.
//! - Images are particle textures in `assets/particles`, textures in
//!   `assets/textures` or beside a model in `assets/models`, and UI images
//!   anywhere else.
//!
//! Dragging a tile sets an egui drag-and-drop payload ([`AssetPayload`]);
//! the app places it where it's let go over the viewport. Double-clicking a
//! tile places it at the camera's focus point.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use axcenity_assets::{AUDIO_EXTENSIONS, Assets, TEXTURE_EXTENSIONS};
use eframe::egui;

use crate::props::{PROPS, Prop};
use crate::theme::{self, icons};

/// Where prefabs are saved, in the project folder.
pub const PREFABS_FOLDER: &str = "assets/prefabs";
pub const PREFAB_SUFFIX: &str = ".prefab.json";
/// Where imported UI images and particle textures go.
pub const IMAGES_FOLDER: &str = "assets/images";
pub const PARTICLES_FOLDER: &str = "assets/particles";
const MODEL_EXTENSIONS: [&str; 3] = ["gltf", "glb", "fbx"];
/// How often the folder is walked again while the panel is shown.
const RESCAN: Duration = Duration::from_secs(2);
/// Image thumbnails are scaled down to at most this many pixels across.
const THUMBNAIL_PIXELS: u32 = 96;
const TILE: egui::Vec2 = egui::vec2(84.0, 96.0);
const PICTURE: f32 = 60.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AssetKind {
    Prop,
    Model,
    Prefab,
    Image,
    Texture,
    Audio,
    Particle,
    /// An animation clip (`*.anim.json`).
    Animation,
    /// An animator controller (`*.controller.json`).
    Controller,
}

impl AssetKind {
    fn icon(self) -> &'static str {
        match self {
            AssetKind::Prop => icons::SHAPES,
            AssetKind::Model => icons::CUBE_TRANSPARENT,
            AssetKind::Prefab => icons::PACKAGE,
            AssetKind::Image => icons::IMAGE,
            AssetKind::Texture => icons::IMAGE_SQUARE,
            AssetKind::Audio => icons::WAVEFORM,
            AssetKind::Particle => icons::SPARKLE,
            AssetKind::Animation => icons::FILM_SLATE,
            AssetKind::Controller => icons::FLOW_ARROW,
        }
    }

    fn color(self, visuals: &egui::Visuals) -> egui::Color32 {
        use axcenity_core::ClassName;
        match self {
            AssetKind::Prop | AssetKind::Model | AssetKind::Prefab => {
                theme::class_color(ClassName::Part, visuals)
            }
            AssetKind::Image | AssetKind::Texture => {
                theme::class_color(ClassName::SurfaceAppearance, visuals)
            }
            AssetKind::Audio => theme::class_color(ClassName::Sound, visuals),
            AssetKind::Particle => theme::class_color(ClassName::PointLight, visuals),
            AssetKind::Animation | AssetKind::Controller => {
                theme::class_color(ClassName::Animator, visuals)
            }
        }
    }

    /// What dropping one in the viewport does, for its tile's tooltip.
    fn hover(self) -> &'static str {
        match self {
            AssetKind::Prop | AssetKind::Prefab => {
                "Drag into the viewport to place a copy on the surface under the mouse."
            }
            AssetKind::Model => "Drag into the viewport to place it as a MeshPart.",
            AssetKind::Image => {
                "Drag onto an ImageLabel to show it there, or anywhere else to add an ImageLabel."
            }
            AssetKind::Texture => {
                "Drag onto a MeshPart to use it as the color map of its SurfaceAppearances."
            }
            AssetKind::Audio => {
                "Drag onto a part to put a Sound in it, or anywhere else for one in the Workspace."
            }
            AssetKind::Particle => {
                "Drag onto a part to give it a ParticleEmitter with this texture."
            }
            AssetKind::Animation => "Double-click to open it in the Animation panel.",
            AssetKind::Controller => {
                "Drag onto an animated MeshPart to give its Animator this controller."
            }
        }
    }
}

/// A tile in the panel.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AssetEntry {
    pub kind: AssetKind,
    /// The file's path in the project, with `/` between folders, or a
    /// prop's name.
    pub path: String,
    /// What the tile says: the file name without its extension.
    pub name: String,
}

impl AssetEntry {
    /// The built-in prop this tile is, if it is one.
    pub fn prop(&self) -> Option<&'static Prop> {
        (self.kind == AssetKind::Prop)
            .then(|| PROPS.iter().find(|prop| prop.name == self.path))
            .flatten()
    }
}

/// What's dragged from the panel to the viewport.
#[derive(Clone, Debug)]
pub struct AssetPayload(pub AssetEntry);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Category {
    #[default]
    All,
    Props,
    Models,
    Prefabs,
    Images,
    Textures,
    Audio,
    Particles,
    Animations,
}

impl Category {
    const ALL: [Category; 9] = [
        Category::All,
        Category::Props,
        Category::Models,
        Category::Prefabs,
        Category::Images,
        Category::Textures,
        Category::Audio,
        Category::Particles,
        Category::Animations,
    ];

    fn label(self) -> &'static str {
        match self {
            Category::All => "All",
            Category::Props => "Props",
            Category::Models => "Models",
            Category::Prefabs => "Prefabs",
            Category::Images => "Images",
            Category::Textures => "Textures",
            Category::Audio => "Audio",
            Category::Particles => "Particles",
            Category::Animations => "Animations",
        }
    }

    fn holds(self, kind: AssetKind) -> bool {
        match self {
            Category::All => true,
            Category::Props => kind == AssetKind::Prop,
            Category::Models => kind == AssetKind::Model,
            Category::Prefabs => kind == AssetKind::Prefab,
            Category::Images => kind == AssetKind::Image,
            Category::Textures => kind == AssetKind::Texture,
            Category::Audio => kind == AssetKind::Audio,
            Category::Particles => kind == AssetKind::Particle,
            Category::Animations => {
                matches!(kind, AssetKind::Animation | AssetKind::Controller)
            }
        }
    }

    /// Where files imported while this category is shown go, for images.
    pub fn image_folder(self) -> &'static str {
        match self {
            Category::Textures => axcenity_assets::PROJECT_TEXTURES_FOLDER,
            Category::Particles => PARTICLES_FOLDER,
            _ => IMAGES_FOLDER,
        }
    }
}

/// What the panel asks the app to do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BrowserAction {
    /// Place this at the camera's focus point (a double-click).
    Place(AssetEntry),
    /// Copy files into the project; images go in this folder.
    Import { image_folder: &'static str },
    /// Save the selection as a prefab.
    SavePrefab,
}

#[derive(Default)]
pub struct AssetBrowser {
    category: Category,
    search: String,
    folder: Option<PathBuf>,
    files: Vec<AssetEntry>,
    scanned_at: Option<Instant>,
    /// Image thumbnails by path, `None` for an image that can't be read.
    thumbnails: HashMap<String, Option<egui::TextureHandle>>,
}

impl AssetBrowser {
    /// Forgets the old project's files, for a newly opened (or closed) one.
    pub fn open(&mut self, folder: Option<&Path>) {
        self.folder = folder.map(Path::to_owned);
        self.files.clear();
        self.thumbnails.clear();
        self.scanned_at = None;
    }

    /// Walks the folder again on the next frame the panel is shown.
    pub fn refresh(&mut self) {
        self.scanned_at = None;
    }

    /// Every tile: the props, then the files in path order.
    pub fn entries(&mut self) -> Vec<AssetEntry> {
        self.rescan_if_due();
        let props = PROPS.iter().map(|prop| AssetEntry {
            kind: AssetKind::Prop,
            path: prop.name.to_owned(),
            name: prop.name.to_owned(),
        });
        props.chain(self.files.iter().cloned()).collect()
    }

    fn rescan_if_due(&mut self) {
        if self.scanned_at.is_some_and(|at| at.elapsed() < RESCAN) {
            return;
        }
        self.scanned_at = Some(Instant::now());
        self.files = match &self.folder {
            Some(folder) => scan(folder),
            None => Vec::new(),
        };
        let files = &self.files;
        self.thumbnails
            .retain(|path, _| files.iter().any(|entry| entry.path == *path));
    }

    /// Draws the panel. `can_edit` is false during Play, and `can_save` is
    /// true when something that can be a prefab is selected.
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        assets: &Assets,
        can_edit: bool,
        can_save: bool,
    ) -> Option<BrowserAction> {
        let mut action = None;
        ui.horizontal(|ui| {
            for category in Category::ALL {
                if ui
                    .selectable_label(self.category == category, category.label())
                    .clicked()
                {
                    self.category = category;
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .add_enabled(
                        can_edit && can_save,
                        egui::Button::new(theme::with_icon(icons::FLOPPY_DISK, "Save prefab")),
                    )
                    .on_hover_text(
                        "Save the selected parts and Models as a prefab in assets/prefabs, to \
                         place copies of from here.",
                    )
                    .on_disabled_hover_text("Select parts or Models to save as a prefab.")
                    .clicked()
                {
                    action = Some(BrowserAction::SavePrefab);
                }
                if ui
                    .add_enabled(
                        can_edit && self.folder.is_some(),
                        egui::Button::new(theme::with_icon(icons::PLUS, "Import")),
                    )
                    .on_hover_text(
                        "Copy models, images, sounds or prefabs into the project's assets folder.",
                    )
                    .clicked()
                {
                    action = Some(BrowserAction::Import {
                        image_folder: self.category.image_folder(),
                    });
                }
                if ui
                    .small_button(icons::ARROW_CLOCKWISE)
                    .on_hover_text("Look for new files now")
                    .clicked()
                {
                    self.refresh();
                }
                ui.add(
                    egui::TextEdit::singleline(&mut self.search)
                        .hint_text(format!("{} Search", icons::MAGNIFYING_GLASS))
                        .desired_width(160.0),
                );
            });
        });
        ui.separator();

        let search = self.search.trim().to_lowercase();
        let entries: Vec<AssetEntry> = self
            .entries()
            .into_iter()
            .filter(|entry| self.category.holds(entry.kind))
            .filter(|entry| search.is_empty() || entry.path.to_lowercase().contains(&search))
            .collect();
        egui::ScrollArea::vertical()
            .auto_shrink(false)
            .show(ui, |ui| {
                if entries.is_empty() {
                    ui.weak(
                        "Nothing here yet. Import files, or copy them into the project's assets \
                         folder.",
                    );
                    return;
                }
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = egui::vec2(6.0, 6.0);
                    for entry in entries {
                        if self.tile(ui, assets, &entry, can_edit) {
                            action = Some(BrowserAction::Place(entry));
                        }
                    }
                });
            });
        action
    }

    /// Draws one tile. Returns true if it was double-clicked.
    fn tile(
        &mut self,
        ui: &mut egui::Ui,
        assets: &Assets,
        entry: &AssetEntry,
        can_edit: bool,
    ) -> bool {
        let (rect, response) = ui.allocate_exact_size(TILE, egui::Sense::click_and_drag());
        let id = response.id;
        let hover = match entry.prop() {
            Some(prop) => format!("{}\n{}", prop.hover, entry.kind.hover()),
            None => format!("{}\n{}", entry.path, entry.kind.hover()),
        };
        let response = response.on_hover_text(hover);
        if can_edit && response.drag_started() {
            egui::DragAndDrop::set_payload(ui.ctx(), AssetPayload(entry.clone()));
        }
        let visuals = ui.visuals();
        let fill = if response.hovered() || response.dragged() {
            visuals.widgets.hovered.weak_bg_fill
        } else {
            visuals.faint_bg_color
        };
        let painter = ui.painter();
        painter.rect_filled(rect, 6.0, fill);
        let picture = egui::Rect::from_center_size(
            egui::pos2(rect.center().x, rect.min.y + 6.0 + PICTURE / 2.0),
            egui::vec2(PICTURE, PICTURE),
        );
        let thumbnail = matches!(
            entry.kind,
            AssetKind::Image | AssetKind::Texture | AssetKind::Particle
        )
        .then(|| self.thumbnail(ui.ctx(), assets, &entry.path))
        .flatten();
        self.paint_picture(ui, picture, entry, thumbnail.as_ref());
        let text_color = ui.visuals().text_color();
        let galley = ui.painter().layout(
            entry.name.clone(),
            egui::FontId::proportional(11.5),
            text_color,
            TILE.x - 6.0,
        );
        // Long names show their first two lines.
        let text_rect = egui::Rect::from_min_size(
            egui::pos2(rect.min.x + 3.0, picture.max.y + 4.0),
            egui::vec2(TILE.x - 6.0, rect.max.y - picture.max.y - 6.0),
        );
        ui.painter().with_clip_rect(text_rect).galley(
            egui::pos2(rect.center().x - galley.size().x / 2.0, text_rect.min.y),
            galley,
            text_color,
        );

        // While dragged, a copy of the picture follows the mouse.
        if response.dragged()
            && let Some(pointer) = ui.ctx().pointer_latest_pos()
        {
            let layer = egui::LayerId::new(egui::Order::Tooltip, id.with("dragged"));
            let ghost = egui::Rect::from_center_size(
                pointer + egui::vec2(18.0, 18.0),
                egui::vec2(40.0, 40.0),
            );
            let painter = ui.ctx().layer_painter(layer);
            painter.rect_filled(ghost.expand(3.0), 6.0, ui.visuals().window_fill);
            match &thumbnail {
                Some(texture) => {
                    painter.image(texture.id(), ghost, uv(), egui::Color32::WHITE);
                }
                None => {
                    painter.text(
                        ghost.center(),
                        egui::Align2::CENTER_CENTER,
                        icon_of(entry),
                        egui::FontId::proportional(28.0),
                        entry.kind.color(ui.visuals()),
                    );
                }
            }
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
        }
        can_edit && response.double_clicked()
    }

    fn paint_picture(
        &self,
        ui: &egui::Ui,
        picture: egui::Rect,
        entry: &AssetEntry,
        thumbnail: Option<&egui::TextureHandle>,
    ) {
        let painter = ui.painter();
        match thumbnail {
            Some(texture) => {
                // Fitted inside the square, keeping its shape.
                let [w, h] = texture.size().map(|n| n as f32);
                let scale = (PICTURE / w).min(PICTURE / h);
                let fitted =
                    egui::Rect::from_center_size(picture.center(), egui::vec2(w, h) * scale);
                // Checkers behind see-through images, such as particles.
                painter.rect_filled(picture, 4.0, egui::Color32::from_gray(40));
                painter.image(texture.id(), fitted, uv(), egui::Color32::WHITE);
            }
            None => {
                painter.text(
                    picture.center(),
                    egui::Align2::CENTER_CENTER,
                    icon_of(entry),
                    egui::FontId::proportional(34.0),
                    entry.kind.color(ui.visuals()),
                );
            }
        }
    }

    /// The thumbnail of the image at `path`, made the first time.
    fn thumbnail(
        &mut self,
        ctx: &egui::Context,
        assets: &Assets,
        path: &str,
    ) -> Option<egui::TextureHandle> {
        self.thumbnails
            .entry(path.to_owned())
            .or_insert_with(|| {
                let image = assets.texture(path)?;
                let color = shrink(image.width, image.height, &image.rgba, THUMBNAIL_PIXELS)?;
                Some(ctx.load_texture(
                    format!("asset thumbnail {path}"),
                    color,
                    egui::TextureOptions::LINEAR,
                ))
            })
            .clone()
    }
}

fn uv() -> egui::Rect {
    egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0))
}

fn icon_of(entry: &AssetEntry) -> &'static str {
    entry.prop().map_or(entry.kind.icon(), |prop| prop.icon)
}

/// `rgba` (`width` × `height`) scaled down to at most `most` pixels across,
/// by taking the nearest pixel. `None` for an empty or short image.
fn shrink(width: u32, height: u32, rgba: &[u8], most: u32) -> Option<egui::ColorImage> {
    if width == 0 || height == 0 || rgba.len() < (width * height * 4) as usize {
        return None;
    }
    let scale = (most as f32 / width.max(height) as f32).min(1.0);
    let (w, h) = (
        ((width as f32 * scale).round() as u32).max(1),
        ((height as f32 * scale).round() as u32).max(1),
    );
    let mut pixels = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h {
        let sy = (y * height / h).min(height - 1);
        for x in 0..w {
            let sx = (x * width / w).min(width - 1);
            let i = ((sy * width + sx) * 4) as usize;
            pixels.extend_from_slice(&rgba[i..i + 4]);
        }
    }
    Some(egui::ColorImage::from_rgba_unmultiplied(
        [w as usize, h as usize],
        &pixels,
    ))
}

/// What kind of asset the file at `path` (in the project, with `/`
/// between folders) is, if it's one.
pub fn kind_of(path: &str) -> Option<AssetKind> {
    let lower = path.to_lowercase();
    if lower.ends_with(PREFAB_SUFFIX) {
        return Some(AssetKind::Prefab);
    }
    if lower.ends_with(axcenity_anim::CLIP_EXTENSION) {
        return Some(AssetKind::Animation);
    }
    if lower.ends_with(axcenity_anim::CONTROLLER_EXTENSION) {
        return Some(AssetKind::Controller);
    }
    let extension = lower.rsplit_once('.')?.1;
    if MODEL_EXTENSIONS.contains(&extension) {
        Some(AssetKind::Model)
    } else if AUDIO_EXTENSIONS.contains(&extension) {
        Some(AssetKind::Audio)
    } else if TEXTURE_EXTENSIONS.contains(&extension) {
        Some(if lower.starts_with(&format!("{PARTICLES_FOLDER}/")) {
            AssetKind::Particle
        } else if lower.starts_with("assets/textures/") || lower.starts_with("assets/models/") {
            AssetKind::Texture
        } else {
            AssetKind::Image
        })
    } else {
        None
    }
}

/// A file's name without its extension (or `.prefab.json`).
pub fn display_name(path: &str) -> String {
    let file = path.rsplit('/').next().unwrap_or(path);
    let lower = file.to_lowercase();
    for suffix in [
        PREFAB_SUFFIX,
        axcenity_anim::CLIP_EXTENSION,
        axcenity_anim::CONTROLLER_EXTENSION,
    ] {
        if lower.ends_with(suffix) {
            return file[..file.len() - suffix.len()].to_owned();
        }
    }
    file.rsplit_once('.')
        .map_or(file, |(stem, _)| stem)
        .to_owned()
}

/// Every asset file in `folder`'s `assets` folder, sorted by path.
fn scan(folder: &Path) -> Vec<AssetEntry> {
    let mut found = Vec::new();
    walk(&folder.join("assets"), "assets", &mut found);
    found.sort_by_key(|entry| entry.path.to_lowercase());
    found
}

fn walk(directory: &Path, prefix: &str, found: &mut Vec<AssetEntry>) {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        let path = format!("{prefix}/{name}");
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_dir() {
            walk(&entry.path(), &path, found);
        } else if let Some(kind) = kind_of(&path) {
            found.push(AssetEntry {
                kind,
                name: display_name(&path),
                path,
            });
        }
    }
}

/// A file name for a prefab called `name`: letters, digits, spaces, `-`
/// and `_` are kept, anything else becomes `_`.
pub fn prefab_file_stem(name: &str) -> String {
    let cleaned: String = name
        .trim()
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || matches!(c, ' ' | '-' | '_') {
                c
            } else {
                '_'
            }
        })
        .collect();
    match cleaned.trim() {
        "" => "Prefab".to_owned(),
        stem => stem.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn files_are_sorted_into_kinds_by_extension_and_folder() {
        assert_eq!(kind_of("assets/models/chair.glb"), Some(AssetKind::Model));
        assert_eq!(
            kind_of("assets/models/robot/robot.FBX"),
            Some(AssetKind::Model)
        );
        assert_eq!(
            kind_of("assets/prefabs/Lamp.prefab.json"),
            Some(AssetKind::Prefab)
        );
        assert_eq!(kind_of("assets/audio/ding.ogg"), Some(AssetKind::Audio));
        assert_eq!(
            kind_of("assets/particles/spark_01.png"),
            Some(AssetKind::Particle)
        );
        assert_eq!(
            kind_of("assets/textures/wood.jpg"),
            Some(AssetKind::Texture)
        );
        assert_eq!(
            kind_of("assets/models/tree/textures/bark.png"),
            Some(AssetKind::Texture)
        );
        assert_eq!(kind_of("assets/images/logo.png"), Some(AssetKind::Image));
        assert_eq!(kind_of("assets/notes.txt"), None);
        assert_eq!(kind_of("assets/data.json"), None);
    }

    #[test]
    fn names_drop_the_extension() {
        assert_eq!(
            display_name("assets/prefabs/Street Lamp.prefab.json"),
            "Street Lamp"
        );
        assert_eq!(display_name("assets/models/chair.glb"), "chair");
        assert_eq!(prefab_file_stem("Lamp: big/small"), "Lamp_ big_small");
        assert_eq!(prefab_file_stem("  "), "Prefab");
    }

    #[test]
    fn the_folder_is_walked_for_assets() {
        let folder = std::env::temp_dir().join(format!("axcenity-assets-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        for file in [
            "assets/models/a.glb",
            "assets/audio/b.wav",
            "assets/particles/c.png",
            "assets/.hidden/d.png",
            "assets/readme.md",
        ] {
            let path = folder.join(file);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, b"").unwrap();
        }
        let paths: Vec<String> = scan(&folder).into_iter().map(|entry| entry.path).collect();
        assert_eq!(
            paths,
            [
                "assets/audio/b.wav",
                "assets/models/a.glb",
                "assets/particles/c.png"
            ]
        );
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn thumbnails_shrink_and_keep_their_shape() {
        let rgba = vec![255; 200 * 100 * 4];
        let image = shrink(200, 100, &rgba, 96).unwrap();
        assert_eq!(image.size, [96, 48]);
        assert!(shrink(0, 10, &[], 96).is_none());
    }
}
