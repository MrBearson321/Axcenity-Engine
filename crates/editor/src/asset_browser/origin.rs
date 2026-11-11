//! Where each asset in the Assets panel came from, and the asset it
//! belongs under: what the panel's default view groups by.
//!
//! An asset's origin is worked out from what the project already records,
//! never guessed from its name:
//!
//! - **Built-in**: the engine's props.
//! - **A Toolbox source** (Poly Haven, ambientCG, Openverse): the project's
//!   `.axcenity/asset-sources.json` names the file or the folder it's in.
//! - **Imported**: a model, texture, sky, sound or image copied in from this
//!   computer (Import, or dropped into `assets/`), and the clips an
//!   animated model's import wrote into `assets/animations/<model>/`.
//! - **Made in Axcenity**: prefabs, materials, clips, state machines,
//!   sequences, behavior trees and data tables made in the editor.
//! - **Global assets**: global assets this project hasn't got a copy of.
//!
//! An imported model's folder (`packages.rs`) takes its model's origin, so
//! a Poly Haven model's textures stay with it. A material made from a
//! texture set (Toolbox **Make material**, or by hand) belongs under that
//! set: it takes the set's origin and is listed right after it.

use std::collections::HashMap;
use std::path::Path;

use axcenity_toolbox::provenance::{self, Snapshot};

use super::packages::Package;
use super::{AssetEntry, AssetKind};
use crate::theme::icons;

const ANIMATIONS: &str = "assets/animations/";

/// Where an asset came from, in the order the panel's sections go.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Origin {
    BuiltIn,
    Made,
    Imported,
    /// A Toolbox source, by its key (`polyhaven`).
    Toolbox(String),
    /// A global asset this project hasn't got yet.
    Global,
}

impl Origin {
    /// The section's heading and the Source menu's entry.
    pub fn label(&self) -> String {
        match self {
            Origin::BuiltIn => "Built-in".to_owned(),
            Origin::Made => "Made in Axcenity".to_owned(),
            Origin::Imported => "Imported".to_owned(),
            Origin::Toolbox(key) => match provenance::source_name(key) {
                name if name.is_empty() => "Toolbox".to_owned(),
                name => name,
            },
            Origin::Global => "Global assets".to_owned(),
        }
    }

    pub fn icon(&self) -> &'static str {
        match self {
            Origin::BuiltIn => icons::SHAPES,
            Origin::Made => icons::PENCIL_SIMPLE,
            Origin::Imported => icons::FILE_ARROW_DOWN,
            Origin::Toolbox(_) => icons::CLOUD_ARROW_DOWN,
            Origin::Global => icons::GLOBE,
        }
    }

    /// What the section holds, for its heading's tooltip.
    pub fn hover(&self) -> String {
        match self {
            Origin::BuiltIn => "The engine's own props, in every project.".to_owned(),
            Origin::Made => {
                "Made in the editor: prefabs, materials, clips, state machines, sequences, \
                 behavior trees and data tables."
                    .to_owned()
            }
            Origin::Imported => {
                "Copied in from this computer: with Import, or into the assets folder.".to_owned()
            }
            Origin::Toolbox(_) => format!(
                "Got from {} through the Toolbox. Each one's licence is in its tooltip.",
                self.label()
            ),
            Origin::Global => {
                "Global assets this project hasn't used yet: each is copied in the first time \
                 it's used."
                    .to_owned()
            }
        }
    }
}

/// Every listed asset's origin, and the parent of each one listed under
/// another (a material under its texture set).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Origins {
    by_path: HashMap<String, Origin>,
    /// A material's path to its texture set's.
    parents: HashMap<String, String>,
}

impl Origins {
    /// Works out the origins of the project's `files` (in `folder`, whose
    /// material files are read to find their texture sets) and of the
    /// global assets in `shared`, from the project's Toolbox `sources`.
    pub fn work_out(
        folder: Option<&Path>,
        files: &[AssetEntry],
        shared: &[AssetEntry],
        packages: &[Package],
        sources: &Snapshot,
    ) -> Self {
        let mut origins = Origins::default();
        let toolbox = |path: &str| {
            sources
                .source_of(path)
                .map(|record| Origin::Toolbox(record.provider.clone()))
        };
        for entry in shared {
            origins.by_path.insert(entry.path.clone(), Origin::Global);
        }
        // A model's folder goes where its model came from.
        for package in packages {
            if origins.by_path.contains_key(&package.model.path) {
                continue;
            }
            let origin = toolbox(&package.model.path).unwrap_or(Origin::Imported);
            for member in &package.members {
                origins.by_path.insert(member.path.clone(), origin.clone());
            }
        }
        let sets: Vec<&AssetEntry> = files
            .iter()
            .filter(|entry| entry.kind == AssetKind::TextureSet)
            .collect();
        for entry in files {
            if origins.by_path.contains_key(&entry.path) {
                continue;
            }
            let origin = match toolbox(&entry.path) {
                Some(origin) => origin,
                None => own_origin(entry),
            };
            origins.by_path.insert(entry.path.clone(), origin);
        }
        // Materials made from a texture set go with it.
        for entry in files.iter().filter(|e| e.kind == AssetKind::Material) {
            let Some(folder) = folder else {
                break;
            };
            let Some(set) = texture_set_of(folder, &entry.path, &sets) else {
                continue;
            };
            if let Some(origin) = origins.by_path.get(&set.path).cloned() {
                origins.by_path.insert(entry.path.clone(), origin);
            }
            origins.parents.insert(entry.path.clone(), set.path.clone());
        }
        origins
    }

    /// Where `entry` came from. A file not seen yet counts as imported.
    pub fn of(&self, entry: &AssetEntry) -> Origin {
        if entry.kind == AssetKind::Prop {
            return Origin::BuiltIn;
        }
        self.by_path
            .get(&entry.path)
            .cloned()
            .unwrap_or(Origin::Imported)
    }

    /// The path of the asset `path` is listed under, if any.
    pub fn parent(&self, path: &str) -> Option<&str> {
        self.parents.get(path).map(String::as_str)
    }

    /// The origins of `entries` (with the built-in props), in section
    /// order, each once.
    pub fn present(&self, entries: &[AssetEntry]) -> Vec<Origin> {
        let mut found: Vec<Origin> = entries.iter().map(|entry| self.of(entry)).collect();
        found.sort();
        found.dedup();
        found
    }
}

/// The origin of a project file the Toolbox didn't bring, from its kind.
fn own_origin(entry: &AssetEntry) -> Origin {
    match entry.kind {
        AssetKind::Prop => Origin::BuiltIn,
        // An animated model's import writes its clips and state machine
        // into a folder of its own under assets/animations.
        AssetKind::Animation | AssetKind::Controller
            if entry
                .path
                .to_lowercase()
                .strip_prefix(ANIMATIONS)
                .is_some_and(|rest| rest.contains('/')) =>
        {
            Origin::Imported
        }
        AssetKind::Prefab
        | AssetKind::Material
        | AssetKind::Animation
        | AssetKind::Controller
        | AssetKind::Sequence
        | AssetKind::BehaviorTree
        | AssetKind::DataTable => Origin::Made,
        AssetKind::Model
        | AssetKind::Image
        | AssetKind::Texture
        | AssetKind::TextureSet
        | AssetKind::Audio
        | AssetKind::Particle
        | AssetKind::Sky => Origin::Imported,
    }
}

/// The texture set a material file's maps all come from: the set whose
/// folder holds every map the material names. `None` for a material with
/// no maps, maps from several places, or a file that can't be read.
fn texture_set_of<'a>(
    folder: &Path,
    material: &str,
    sets: &[&'a AssetEntry],
) -> Option<&'a AssetEntry> {
    let file = axcenity_assets::read_material_file(&folder.join(material)).ok()?;
    let maps = &file.maps;
    let paths: Vec<String> = [
        &maps.color,
        &maps.normal,
        &maps.metalness,
        &maps.roughness,
        &maps.occlusion,
        &maps.emissive,
        &maps.opacity,
        &maps.specular,
    ]
    .into_iter()
    .flatten()
    .filter_map(|map| map.path.as_deref())
    .filter(|path| !path.is_empty())
    .map(|path| path.trim_start_matches("./").to_lowercase())
    .collect();
    if paths.is_empty() {
        return None;
    }
    sets.iter().copied().find(|set| {
        let Some((set_folder, _)) = set.path.rsplit_once('/') else {
            return false;
        };
        let prefix = format!("{}/", set_folder.to_lowercase());
        paths.iter().all(|path| path.starts_with(&prefix))
    })
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use axcenity_assets::{MapFile, MaterialFile, MaterialMaps, material_json};
    use axcenity_toolbox::AssetRecord;
    use axcenity_toolbox::provenance::SourcedFile;
    use eframe::egui;

    use super::super::packages::{Block, Shown, grouped, packages};
    use super::super::{AssetBrowser, scan};
    use super::*;
    use crate::theme;

    /// A project with assets from everywhere: an imported model's folder,
    /// a Poly Haven model's folder, an ambientCG texture set and the
    /// material made from it, an Openverse sound, an imported sound, clips
    /// made here and one an animated import wrote, a prefab, and a material
    /// with no maps.
    fn project(name: &str) -> PathBuf {
        let folder =
            std::env::temp_dir().join(format!("axcenity-origins-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        for file in [
            "assets/models/Hero/Hero.fbx",
            "assets/models/Hero/textures/Hero_Diffuse.png",
            "assets/models/rock_01/rock_01.gltf",
            "assets/models/rock_01/textures/rock_01_diff_1k.jpg",
            "assets/textures/Rock030/Rock030.texture.json",
            "assets/textures/Rock030/Rock030_Color.jpg",
            "assets/audio/rain-a1b2.mp3",
            "assets/audio/ding.wav",
            "assets/animations/Walk.anim.json",
            "assets/animations/Dance/Dance.anim.json",
            "assets/prefabs/Lamp.prefab.json",
            "assets/materials/Plain.material.json",
        ] {
            let path = folder.join(file);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, b"{}").unwrap();
        }
        let material = MaterialFile {
            maps: MaterialMaps {
                color: Some(MapFile::of("assets/textures/Rock030/Rock030_Color.jpg")),
                ..MaterialMaps::default()
            },
            ..MaterialFile::default()
        };
        std::fs::write(
            folder.join("assets/materials/Rock030.material.json"),
            material_json(&material),
        )
        .unwrap();
        let mut snapshot = Snapshot::default();
        for (path, provider, id) in [
            ("assets/models/rock_01", "polyhaven", "rock_01"),
            ("assets/textures/Rock030", "ambientcg", "Rock030"),
            ("assets/audio/rain-a1b2.mp3", "openverse", "a1b2"),
        ] {
            let record = AssetRecord {
                provider: provider.into(),
                id: id.into(),
                name: id.into(),
                license: "CC0".into(),
                ..AssetRecord::default()
            };
            snapshot.files.push(SourcedFile {
                path: path.into(),
                key: record.key(),
                variant: "1K".into(),
                uid: None,
            });
            snapshot.assets.insert(record.key(), record);
        }
        snapshot.write(&folder).unwrap();
        folder
    }

    fn describe(blocks: Vec<Block>) -> Vec<String> {
        let mut lines = Vec::new();
        for block in blocks {
            lines.extend(block.heading.map(|heading| format!("# {heading}")));
            for item in block.items {
                match item {
                    Shown::Asset(entry) if entry.kind == AssetKind::Prop => {}
                    Shown::Asset(entry) => lines.push(format!("{} ({:?})", entry.name, entry.kind)),
                    Shown::Folder(package) => lines.push(format!("[{}]", package.name)),
                }
            }
        }
        lines
    }

    #[test]
    fn assets_are_grouped_by_where_they_came_from_with_their_parents() {
        let folder = project("grouped");
        let files = scan(&folder);
        let packages = packages(&files);
        let sources = Snapshot::read(&folder).unwrap();
        let origins = Origins::work_out(Some(&folder), &files, &[], &packages, &sources);
        assert_eq!(
            describe(grouped(files.clone(), &packages, None, false, &origins)),
            [
                "# Made in Axcenity",
                "Walk (Animation)",
                "Plain (Material)",
                "Lamp (Prefab)",
                "# Imported",
                "[Hero]",
                "Dance (Animation)",
                "ding (Audio)",
                "# ambientCG",
                // The material made from the set comes right after it.
                "Rock030 (TextureSet)",
                "Rock030 (Material)",
                "# Openverse",
                "rain-a1b2 (Audio)",
                "# Poly Haven",
                "[rock_01]",
            ]
        );
        assert_eq!(
            origins.parent("assets/materials/Rock030.material.json"),
            Some("assets/textures/Rock030/Rock030.texture.json")
        );
        // A Poly Haven model's own textures stay in its folder, under it.
        let texture = files
            .iter()
            .find(|entry| entry.path.ends_with("rock_01_diff_1k.jpg"))
            .unwrap();
        assert_eq!(origins.of(texture), Origin::Toolbox("polyhaven".into()));
        // Searching still sections the matches, each on its own.
        let found: Vec<AssetEntry> = files
            .iter()
            .filter(|entry| entry.path.contains("Rock030") || entry.path.contains("Hero_"))
            .cloned()
            .collect();
        assert_eq!(
            describe(grouped(found, &packages, None, true, &origins)),
            [
                "# Imported",
                "Hero_Diffuse (Texture)",
                "# ambientCG",
                "Rock030 (TextureSet)",
                "Rock030 (Material)",
            ]
        );
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn global_assets_and_props_have_their_own_sections() {
        let origins = Origins::default();
        let prop = AssetEntry {
            kind: AssetKind::Prop,
            path: "Crate".into(),
            name: "Crate".into(),
        };
        assert_eq!(origins.of(&prop), Origin::BuiltIn);
        let shared = AssetEntry {
            kind: AssetKind::Audio,
            path: "assets/audio/theme.ogg".into(),
            name: "theme".into(),
        };
        let origins = Origins::work_out(
            None,
            &[],
            std::slice::from_ref(&shared),
            &[],
            &Snapshot::default(),
        );
        assert_eq!(origins.of(&shared), Origin::Global);
        assert_eq!(
            origins.present(&[shared, prop]),
            [Origin::BuiltIn, Origin::Global]
        );
        assert_eq!(Origin::Toolbox("polyhaven".into()).label(), "Poly Haven");
    }

    /// The panel draws its sections, filters to one source, folds a
    /// section, and turns grouping off (PNGs with `AXCENITY_SNAPSHOTS`).
    #[test]
    fn the_panel_shows_a_section_per_source_and_filters_to_one() {
        let folder = project("panel");
        let mut browser = AssetBrowser::default();
        browser.open(Some(&folder));
        let ctx = theme::test_context();
        let text = |browser: &mut AssetBrowser, width: f32, name: &str| -> String {
            let size = egui::vec2(width, 1400.0);
            let draw = |browser: &mut AssetBrowser, ui: &mut egui::Ui| {
                egui::CentralPanel::default().show(ui, |ui| browser.show(ui, true, false, None));
            };
            let mut drawn = String::new();
            for _ in 0..2 {
                let input = egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                    ..Default::default()
                };
                let output = crate::theme::run(&ctx, input, |ui| draw(browser, ui));
                drawn = format!("{:?}", output.shapes);
            }
            if let Some(snapshots) = std::env::var_os("AXCENITY_SNAPSHOTS") {
                crate::snapshot::snapshot(
                    &PathBuf::from(snapshots).join(format!("assets-sources-{name}-{width}.png")),
                    size,
                    vec![Vec::new(); 4],
                    |ui| draw(browser, ui),
                );
            }
            drawn
        };
        for width in [300.0, 760.0, 1320.0] {
            let drawn = text(&mut browser, width, "all");
            for heading in [
                "Built-in",
                "Made in Axcenity",
                "Imported",
                "ambientCG",
                "Poly Haven",
            ] {
                assert!(drawn.contains(heading), "{heading} at {width}");
            }
        }
        browser.source_filter = Some(Origin::Toolbox("ambientcg".into()));
        let drawn = text(&mut browser, 760.0, "ambientcg");
        assert!(drawn.contains("Rock030"));
        assert!(!drawn.contains("Hero"));
        browser.source_filter = None;
        browser.folded.insert(Origin::BuiltIn);
        browser.flat = true;
        let drawn = text(&mut browser, 760.0, "flat");
        assert!(!drawn.contains("Made in Axcenity"));
        assert!(drawn.contains("Hero"));
        let _ = std::fs::remove_dir_all(&folder);
    }
}
