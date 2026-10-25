//! An imported model's files, shown as one folder in the Assets panel.
//!
//! Importing a textured or animated model writes many files: the model and
//! its textures in `assets/models/<name>/`, and its clips and state machine
//! in `assets/animations/<name>/`. Listed one by one they bury everything
//! else, so the panel shows them as one folder tile named after the model,
//! which opens to show them. A search still finds each file on its own.
//!
//! This module decides what's in each folder and what the grid shows
//! ([`shown`]), and draws the folder tile and the row above an open folder.

use eframe::egui;

use super::{
    AssetBrowser, AssetEntry, AssetKind, AssetPayload, ItemMenu, LIST_PICTURE, LIST_ROW, PICTURE,
    TILE, global_badge,
};
use crate::theme::{self, icons};
use crate::widgets;

const MODELS: &str = "assets/models/";
const ANIMATIONS: &str = "assets/animations/";

/// The files of one imported model.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Package {
    /// The model's name, as its folder (or file) is called: `Walking`.
    pub name: String,
    /// Its model file, which dragging the folder places.
    pub model: AssetEntry,
    /// Every file in it: models, then state machines and clips, then
    /// textures, each in path order.
    pub members: Vec<AssetEntry>,
}

impl Package {
    /// What's in it, such as "1 model, 1 clip, 1 state machine, 25 textures".
    pub fn summary(&self) -> String {
        let count = |kinds: &[AssetKind]| {
            self.members
                .iter()
                .filter(|entry| kinds.contains(&entry.kind))
                .count()
        };
        let parts = [
            (count(&[AssetKind::Model]), "model"),
            (count(&[AssetKind::Animation]), "clip"),
            (count(&[AssetKind::Controller]), "state machine"),
            (
                count(&[AssetKind::Texture, AssetKind::TextureSet, AssetKind::Image]),
                "texture",
            ),
        ];
        let mut text: Vec<String> = parts
            .iter()
            .filter(|(n, _)| *n > 0)
            .map(|&(n, thing)| match n {
                1 => format!("1 {thing}"),
                n => format!("{n} {thing}s"),
            })
            .collect();
        let others = self.members.len() - parts.iter().map(|(n, _)| n).sum::<usize>();
        if others > 0 {
            text.push(format!("{others} more"));
        }
        text.join(", ")
    }

    /// True if `entry` is one of its files.
    pub fn holds(&self, entry: &AssetEntry) -> bool {
        self.members.iter().any(|member| member.path == entry.path)
    }
}

/// The package `path` (a file in the project) would belong to, by name in
/// lower case: the folder it's in under `assets/models` or
/// `assets/animations`, or for a model file right in `assets/models`, its
/// own name.
fn key(path: &str, kind: AssetKind) -> Option<String> {
    let lower = path.to_lowercase();
    for root in [MODELS, ANIMATIONS] {
        if let Some(rest) = lower.strip_prefix(root) {
            return match rest.split_once('/') {
                Some((folder, _)) => Some(folder.to_owned()),
                None if root == MODELS && kind == AssetKind::Model => {
                    rest.rsplit_once('.').map(|(stem, _)| stem.to_owned())
                }
                None => None,
            };
        }
    }
    None
}

/// Groups `files` into packages: one per model in `assets/models` (in a
/// folder of its own, or a file on its own), holding every file in its
/// folder and in `assets/animations/<name>/`. Only packages of more than
/// one file are made; a model on its own stays a plain tile.
pub fn packages(files: &[AssetEntry]) -> Vec<Package> {
    let mut packages: Vec<(String, Package)> = Vec::new();
    for entry in files.iter().filter(|entry| entry.kind == AssetKind::Model) {
        let Some(key) = key(&entry.path, entry.kind) else {
            continue;
        };
        if packages.iter().any(|(known, _)| *known == key) {
            continue;
        }
        let name = match entry.path[MODELS.len()..].split_once('/') {
            Some((folder, _)) => folder.to_owned(),
            None => entry.name.clone(),
        };
        packages.push((
            key,
            Package {
                name,
                model: entry.clone(),
                members: Vec::new(),
            },
        ));
    }
    for entry in files {
        let Some(key) = key(&entry.path, entry.kind) else {
            continue;
        };
        if let Some((_, package)) = packages.iter_mut().find(|(known, _)| *known == key) {
            package.members.push(entry.clone());
        }
    }
    let mut packages: Vec<Package> = packages
        .into_iter()
        .map(|(_, package)| package)
        .filter(|package| package.members.len() > 1)
        .collect();
    for package in &mut packages {
        // Stable, so each kind keeps path order.
        package.members.sort_by_key(|entry| order(entry.kind));
    }
    packages.sort_by_key(|package| package.name.to_lowercase());
    packages
}

/// Where a kind of file comes in a package: what's used most first.
pub fn order(kind: AssetKind) -> u8 {
    match kind {
        AssetKind::Model | AssetKind::Prefab | AssetKind::Prop => 0,
        AssetKind::Controller => 1,
        AssetKind::Animation => 2,
        AssetKind::TextureSet => 3,
        AssetKind::Texture | AssetKind::Image | AssetKind::Particle => 4,
        AssetKind::Audio => 5,
    }
}

/// The heading for a kind's files inside an open package.
pub fn section(kind: AssetKind) -> &'static str {
    match order(kind) {
        0 => "Model",
        1 | 2 => "Animation",
        3 | 4 => "Textures",
        _ => "Other",
    }
}

/// One thing in the panel's grid or list.
pub(super) enum Shown {
    Asset(AssetEntry),
    /// An imported model's files, as one folder.
    Folder(Package),
}

/// A run of things in the grid, under a heading inside an open folder.
pub(super) struct Block {
    pub(super) heading: Option<&'static str>,
    pub(super) items: Vec<Shown>,
}

/// What the panel shows, in order, from the `entries` the category and
/// search let through. With a search, every match on its own. Inside the
/// `open` folder, its files under headings. Otherwise the props, then a
/// folder for each imported model holding anything shown, then the files
/// not in one.
pub(super) fn shown(
    entries: Vec<AssetEntry>,
    packages: &[Package],
    open: Option<&Package>,
    searching: bool,
) -> Vec<Block> {
    let block = |items| Block {
        heading: None,
        items,
    };
    if searching {
        return vec![block(entries.into_iter().map(Shown::Asset).collect())];
    }
    if let Some(package) = open {
        let mut blocks: Vec<Block> = Vec::new();
        for member in &package.members {
            if !entries.iter().any(|entry| entry.path == member.path) {
                continue;
            }
            let heading = section(member.kind);
            match blocks.last_mut() {
                Some(last) if last.heading == Some(heading) => {
                    last.items.push(Shown::Asset(member.clone()));
                }
                _ => blocks.push(Block {
                    heading: Some(heading),
                    items: vec![Shown::Asset(member.clone())],
                }),
            }
        }
        return blocks;
    }
    let (props, files): (Vec<AssetEntry>, Vec<AssetEntry>) = entries
        .into_iter()
        .partition(|entry| entry.kind == AssetKind::Prop);
    let mut shown: Vec<Shown> = props.into_iter().map(Shown::Asset).collect();
    for package in packages {
        if files.iter().any(|entry| package.holds(entry)) {
            shown.push(Shown::Folder(package.clone()));
        }
    }
    shown.extend(
        files
            .into_iter()
            .filter(|entry| !packages.iter().any(|package| package.holds(entry)))
            .map(Shown::Asset),
    );
    if shown.is_empty() {
        Vec::new()
    } else {
        vec![block(shown)]
    }
}

impl AssetBrowser {
    /// The row above an open folder's files: back to every asset, then the
    /// folder's name and what's in it. True when back is clicked.
    pub(super) fn folder_header(&self, ui: &mut egui::Ui, package: &Package) -> bool {
        let palette = theme::palette(ui.ctx());
        let back = ui
            .horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                let back = ui
                    .add(
                        widgets::button(
                            widgets::Role::Ghost,
                            Some(icons::ARROW_LEFT),
                            "All assets",
                            &palette,
                        )
                        .min_size(egui::vec2(0.0, 24.0)),
                    )
                    .on_hover_text("Back to every asset")
                    .clicked();
                ui.label(egui::RichText::new(icons::CARET_RIGHT).color(palette.text_muted));
                ui.label(
                    egui::RichText::new(theme::with_icon(icons::FOLDER_SIMPLE, &package.name))
                        .color(palette.text_strong),
                );
                ui.label(
                    egui::RichText::new(package.summary())
                        .size(theme::type_scale::SMALL)
                        .color(palette.text_weak),
                );
                back
            })
            .inner;
        ui.add_space(4.0);
        back
    }

    /// Draws an imported model's folder: a tile, or a row of the list when
    /// `list`. Clicking opens it, and dragging it places its model. True
    /// when it's opened, and what its right-click menu asked for.
    pub(super) fn folder_item(
        &mut self,
        ui: &mut egui::Ui,
        package: &Package,
        can_edit: bool,
        list: bool,
    ) -> (bool, Option<ItemMenu>) {
        let size = if list {
            egui::vec2(ui.available_width(), LIST_ROW)
        } else {
            TILE
        };
        let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click_and_drag());
        let id = response.id;
        let global = self.is_global(&package.model.path);
        let response = response.on_hover_text(format!(
            "{}\n{}, from one import.\nClick to open. Drag into the viewport to place the model.{}",
            package.name,
            package.summary(),
            if global {
                "\nGlobal asset: in every project."
            } else {
                ""
            }
        ));
        let mut menu = None;
        response.context_menu(|ui| {
            menu = self.global_checkbox(ui, &package.model.path);
        });
        if can_edit && response.drag_started() {
            egui::DragAndDrop::set_payload(ui.ctx(), AssetPayload(package.model.clone()));
        }
        let palette = theme::palette(ui.ctx());
        let hover = theme::ease(
            ui.ctx(),
            id.with("hover"),
            response.hovered() || response.dragged(),
        );
        let painter = ui.painter();
        painter.rect_filled(rect, 6.0, palette.control_hover.gamma_multiply(hover));
        if response.has_focus() {
            painter.rect_stroke(
                rect,
                6.0,
                egui::Stroke::new(1.0_f32, palette.accent),
                egui::StrokeKind::Inside,
            );
        }
        let (picture, side) = if list {
            let picture = egui::Rect::from_center_size(
                egui::pos2(rect.min.x + 6.0 + LIST_PICTURE / 2.0, rect.center().y),
                egui::vec2(LIST_PICTURE, LIST_PICTURE),
            );
            (picture, LIST_PICTURE)
        } else {
            let picture = egui::Rect::from_center_size(
                egui::pos2(rect.center().x, rect.min.y + 6.0 + PICTURE / 2.0),
                egui::vec2(PICTURE, PICTURE),
            );
            (picture, PICTURE)
        };
        let color = AssetKind::Model.color(ui.visuals());
        painter.rect(
            picture,
            5.0,
            palette.inset,
            egui::Stroke::new(1.0_f32, palette.border),
            egui::StrokeKind::Inside,
        );
        painter.text(
            picture.center(),
            egui::Align2::CENTER_CENTER,
            icons::FOLDER_SIMPLE,
            egui::FontId::proportional(side * 0.6),
            color,
        );
        // The model's icon on the folder, so it reads as "a model's files".
        painter.text(
            picture.center() + egui::vec2(0.0, side * 0.06),
            egui::Align2::CENTER_CENTER,
            icons::CUBE_TRANSPARENT,
            egui::FontId::proportional(side * 0.26),
            color,
        );
        if global {
            global_badge(ui, picture);
        }
        if !list {
            // How many files, in a badge at the bottom right.
            let count = package.members.len().to_string();
            let galley = painter.layout_no_wrap(
                count,
                egui::FontId::proportional(theme::type_scale::CAPTION),
                palette.text_strong,
            );
            let badge = egui::Rect::from_min_size(
                picture.right_bottom() - galley.size() - egui::vec2(10.0, 6.0),
                galley.size() + egui::vec2(8.0, 2.0),
            );
            painter.rect_filled(badge, 6.0, palette.control_active);
            painter.galley(
                badge.min + egui::vec2(4.0, 1.0),
                galley,
                palette.text_strong,
            );
        }
        let text_color = if response.hovered() {
            palette.text_strong
        } else {
            palette.text
        };
        if list {
            let mut name_right = rect.max.x - 6.0;
            if rect.width() >= 200.0 {
                let kind = ui.painter().text(
                    rect.right_center() - egui::vec2(8.0, 0.0),
                    egui::Align2::RIGHT_CENTER,
                    format!("Folder · {}", package.members.len()),
                    egui::FontId::proportional(theme::type_scale::SMALL),
                    palette.text_weak,
                );
                name_right = kind.min.x - 8.0;
            }
            let left = picture.max.x + 8.0;
            let galley = widgets::elided(
                ui,
                package.name.clone(),
                egui::FontId::proportional(theme::type_scale::BODY),
                text_color,
                (name_right - left).max(1.0),
                1,
            );
            ui.painter().galley(
                egui::pos2(left, rect.center().y - galley.size().y / 2.0),
                galley,
                text_color,
            );
        } else {
            let galley = widgets::elided(
                ui,
                package.name.clone(),
                egui::FontId::proportional(theme::type_scale::SMALL),
                text_color,
                TILE.x - 8.0,
                2,
            );
            ui.painter().galley(
                egui::pos2(rect.center().x - galley.size().x / 2.0, picture.max.y + 5.0),
                galley,
                text_color,
            );
        }
        // While dragged, the model's icon follows the mouse.
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
            painter.text(
                ghost.center(),
                egui::Align2::CENTER_CENTER,
                AssetKind::Model.icon(),
                egui::FontId::proportional(28.0),
                color,
            );
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
        }
        let opened = !response.dragged() && (response.clicked() || response.double_clicked());
        (opened, menu)
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::super::{display_name, kind_of, scan};
    use super::*;

    fn entry(path: &str) -> AssetEntry {
        AssetEntry {
            kind: kind_of(path).unwrap(),
            path: path.to_owned(),
            name: display_name(path),
        }
    }

    #[test]
    fn an_imported_models_files_are_one_package() {
        let files: Vec<AssetEntry> = [
            "assets/animations/Door/Open.anim.json",
            "assets/animations/Walking/Walking.anim.json",
            "assets/animations/Walking/Walking.statemachine.json",
            "assets/audio/ding.wav",
            "assets/models/chair.glb",
            "assets/models/noodle.fbx",
            "assets/animations/noodle/Idle.anim.json",
            "assets/models/Walking/Walking.fbx",
            "assets/models/Walking/textures/Body_Diffuse.png",
            "assets/models/Walking/textures/Body_Normal.png",
        ]
        .iter()
        .map(|path| entry(path))
        .collect();
        let packages = packages(&files);
        let names: Vec<&str> = packages.iter().map(|p| p.name.as_str()).collect();
        // The chair is on its own, and Door's clip has no model.
        assert_eq!(names, ["noodle", "Walking"]);

        let walking = &packages[1];
        assert_eq!(walking.model.path, "assets/models/Walking/Walking.fbx");
        let paths: Vec<&str> = walking.members.iter().map(|e| e.path.as_str()).collect();
        assert_eq!(
            paths,
            [
                "assets/models/Walking/Walking.fbx",
                "assets/animations/Walking/Walking.statemachine.json",
                "assets/animations/Walking/Walking.anim.json",
                "assets/models/Walking/textures/Body_Diffuse.png",
                "assets/models/Walking/textures/Body_Normal.png",
            ]
        );
        assert_eq!(
            walking.summary(),
            "1 model, 1 clip, 1 state machine, 2 textures"
        );
        assert!(walking.holds(&files[1]));
        assert!(!walking.holds(&files[0]));

        // A model file on its own in assets/models, with clips of its name.
        assert_eq!(packages[0].members.len(), 2);
        assert_eq!(section(walking.members[1].kind), "Animation");
    }

    /// An imported Hero (model, two textures, a clip and a state machine)
    /// and a sound: the grid shows one Hero folder and the sound; opened,
    /// the folder shows Hero's files under headings, and a search finds
    /// them one by one. Both are drawn (with `AXCENITY_SNAPSHOTS`, saved as
    /// `assets-panel-<view>.png`).
    #[test]
    fn an_imported_models_files_show_as_one_folder_that_opens() {
        let folder =
            std::env::temp_dir().join(format!("axcenity-asset-folders-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        for file in [
            "assets/models/Hero/Hero.fbx",
            "assets/models/Hero/textures/Hero_Diffuse.png",
            "assets/models/Hero/textures/Hero_Normal.png",
            "assets/animations/Hero/Walk.anim.json",
            "assets/animations/Hero/Hero.statemachine.json",
            "assets/audio/ding.wav",
        ] {
            let path = folder.join(file);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, b"").unwrap();
        }
        let files = scan(&folder);
        let packages = packages(&files);
        let describe = |blocks: Vec<Block>| -> Vec<String> {
            let mut lines = Vec::new();
            for block in blocks {
                lines.extend(block.heading.map(|heading| format!("# {heading}")));
                for item in block.items {
                    match item {
                        Shown::Asset(entry) if entry.kind == AssetKind::Prop => {}
                        Shown::Asset(entry) => lines.push(entry.name),
                        Shown::Folder(package) => lines.push(format!("[{}]", package.name)),
                    }
                }
            }
            lines
        };
        assert_eq!(
            describe(shown(files.clone(), &packages, None, false)),
            ["[Hero]", "ding"]
        );
        assert_eq!(
            describe(shown(files.clone(), &packages, Some(&packages[0]), false)),
            [
                "# Model",
                "Hero",
                "# Animation",
                "Hero",
                "Walk",
                "# Textures",
                "Hero_Diffuse",
                "Hero_Normal"
            ]
        );
        let found: Vec<AssetEntry> = files
            .iter()
            .filter(|entry| entry.path.contains("Normal"))
            .cloned()
            .collect();
        assert_eq!(
            describe(shown(found, &packages, None, true)),
            ["Hero_Normal"]
        );

        let mut browser = AssetBrowser::default();
        browser.open(Some(&folder));
        let ctx = theme::test_context();
        let size = egui::vec2(720.0, 320.0);
        let draw = |browser: &mut AssetBrowser, ctx: &egui::Context| {
            egui::CentralPanel::default().show(ctx, |ui| browser.show(ui, true, false, None));
        };
        for view in ["all", "folder"] {
            if view == "folder" {
                browser.open_package = Some("hero".to_owned());
            }
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                ..Default::default()
            };
            let _ = ctx.run(input, |ctx| draw(&mut browser, ctx));
            if let Some(snapshots) = std::env::var_os("AXCENITY_SNAPSHOTS") {
                crate::snapshot::snapshot(
                    &PathBuf::from(snapshots).join(format!("assets-panel-{view}.png")),
                    size,
                    vec![Vec::new(); 4],
                    |ctx| draw(&mut browser, ctx),
                );
            }
        }
        // The open folder is found by its name in any case.
        assert_eq!(browser.open_package.as_deref(), Some("hero"));
        let _ = std::fs::remove_dir_all(&folder);
    }
}
