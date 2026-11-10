//! Reading a changed file again while it's in use (live asset reload).
//!
//! [`Assets::reload`] swaps in the new version of a model, texture,
//! sky image, reflection or light probe bake, material, data table or font file. The swapped-in model or texture is a new load
//! with a new key, so the renderer uploads it again and physics rebuilds
//! the part's hull. A file that now fails to load keeps the last good
//! version, and the failure is reported once, through
//! [`Assets::take_errors`]. A model or texture file whose size and
//! modification time are the same as when it was read is left alone. As
//! with a first load, the file is read with the lock released, so the last
//! good version stays in use until the new one is in.

use std::time::SystemTime;

use super::*;

/// A file's modification time (if the OS reports one) and size.
pub(crate) type Stamp = (Option<SystemTime>, u64);

/// The stamp of the file at `path` in `folder`, if it's there.
pub(crate) fn stamp(folder: &Path, path: &str) -> Option<Stamp> {
    let metadata = std::fs::metadata(folder.join(safe_path(path)?)).ok()?;
    Some((metadata.modified().ok(), metadata.len()))
}

/// Keeps the stamp of the file at `path` as it is now, as the one read.
pub(crate) fn remember_stamp(shared: &mut Shared, folder: &Path, path: &str) {
    keep_stamp(shared, path, stamp(folder, path));
}

/// Keeps `stamp` as the one read for the file at `path`.
pub(crate) fn keep_stamp(shared: &mut Shared, path: &str, stamp: Option<Stamp>) {
    match stamp {
        Some(stamp) => shared.stamps.insert(path.to_owned(), stamp),
        None => shared.stamps.remove(path),
    };
}

/// What's loaded from a file, noted under the lock so it can be read again
/// without it.
#[derive(Default)]
struct Uses {
    mesh_ids: Vec<String>,
    takes: bool,
    texture: bool,
    sky_image: bool,
    lookup: bool,
    palette: bool,
    probe: bool,
    light_probes: bool,
    materials: Vec<String>,
    table: bool,
}

impl Uses {
    fn of(shared: &Shared, file: &str) -> Self {
        Self {
            mesh_ids: shared
                .models
                .keys()
                .filter(|mesh_id| mesh_id.split('#').next() == Some(file))
                .cloned()
                .collect(),
            takes: shared.takes.contains_key(file),
            texture: shared.textures.contains_key(file),
            sky_image: shared.sky_images.contains_key(file),
            lookup: shared.lookups.contains_key(file),
            palette: shared.palettes.contains_key(file),
            probe: shared.probes.contains_key(file),
            light_probes: shared.light_probes.contains_key(file),
            materials: shared
                .materials
                .iter()
                .filter(|(material, resolved)| {
                    material.as_str() == file
                        || resolved
                            .as_ref()
                            .is_some_and(|resolved| resolved.files.iter().any(|used| used == file))
                })
                .map(|(material, _)| material.clone())
                .collect(),
            table: shared.tables.contains_key(file),
        }
    }

    fn any(&self) -> bool {
        !self.mesh_ids.is_empty()
            || self.takes
            || self.texture
            || self.sky_image
            || self.lookup
            || self.palette
            || self.probe
            || self.light_probes
            || !self.materials.is_empty()
            || self.table
    }
}

/// What [`Assets::reload`] did with a changed file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reload {
    /// Nothing loaded uses it, so there was nothing to swap.
    Unused,
    /// Its new version is in use.
    Reloaded,
    /// Its new version can't be loaded, so the last good one stays.
    Failed,
}

impl Assets {
    /// Reads the file at `path` (a project path such as
    /// `assets/models/chair.glb`) again if something loaded uses it: every
    /// model picked from it and its takes, a texture, every material made
    /// from it, or the project's fonts. A file a model reads besides its
    /// own (a glTF's buffer or a texture in the model's folder) reads the
    /// model again too. If any of that fails, everything keeps its last
    /// good version, and why goes to [`take_errors`](Self::take_errors)
    /// once.
    pub fn reload(&self, path: &str) -> Reload {
        let file = path.split('#').next().unwrap_or(path);
        let mut result = self.reload_file(file, false);
        for model in self.models_reading(file) {
            result = match (result, self.reload_file(&model, true)) {
                (Reload::Failed, _) | (_, Reload::Failed) => Reload::Failed,
                (Reload::Reloaded, _) | (_, Reload::Reloaded) => Reload::Reloaded,
                _ => Reload::Unused,
            };
        }
        result
    }

    /// The model files loaded from the folder `file` is in, or a folder
    /// above it, which may read it: `file` isn't a model, clip or other
    /// project file itself.
    fn models_reading(&self, file: &str) -> Vec<String> {
        let own_file = ModelFormat::of(Path::new(file)).is_some()
            || file.ends_with(".json")
            || is_font_file(file);
        if own_file {
            return Vec::new();
        }
        let shared = self.lock();
        let mut models: Vec<String> = shared
            .models
            .keys()
            .map(String::as_str)
            .chain(loading::models_being_read(&shared))
            .filter_map(|mesh_id| mesh_id.split('#').next())
            .filter(|model| {
                let folder = model.rsplit_once('/').map_or("", |(folder, _)| folder);
                file.starts_with(&format!("{folder}/"))
            })
            .map(str::to_owned)
            .collect();
        models.sort();
        models.dedup();
        models
    }

    /// Reads `file` again, as [`reload`](Self::reload) says, unless it's the
    /// same as when it was read (`changed` says it is, for a model whose
    /// other files changed).
    fn reload_file(&self, file: &str, changed: bool) -> Reload {
        if is_font_file(file) {
            let mut shared = self.lock();
            loading::reread_loads(&mut shared, |key| key.kind == Kind::Fonts);
            if shared.fonts.is_none() {
                return Reload::Unused;
            }
            shared.fonts = None;
            return Reload::Reloaded;
        }
        let Some(folder) = self.folder() else {
            return Reload::Unused;
        };
        let now = stamp(&folder, file);
        let (uses, edited, gate) = {
            let mut shared = self.lock();
            if !changed && now.is_some() && shared.stamps.get(file) == now.as_ref() {
                // Written by the host itself after it was read, or touched.
                return Reload::Unused;
            }
            loading::reread_loads(&mut shared, |key| {
                key.file() == file || key.kind == Kind::Material
            });
            (
                Uses::of(&shared, file),
                shared.edited_materials.clone(),
                shared.gate.clone(),
            )
        };
        if !uses.any() {
            return Reload::Unused;
        }
        if let Some(gate) = gate {
            gate(file);
        }

        // Read everything with the lock released; the old versions stay in
        // use meanwhile. Then all of it goes in, or none.
        let mut problems = Vec::new();

        // Every model picked from the file, and its takes: all new, or none.
        let mut models = Vec::new();
        for mesh_id in &uses.mesh_ids {
            if let Some(model) = kept(&mut problems, read_model(&folder, mesh_id)) {
                models.push((mesh_id.clone(), model));
            }
        }
        let takes = uses
            .takes
            .then(|| read_takes(&folder, file))
            .and_then(|read| kept(&mut problems, read));
        let texture = uses
            .texture
            .then(|| read_texture(&folder, file))
            .and_then(|read| kept(&mut problems, read));
        let sky_image = uses
            .sky_image
            .then(|| read_sky_image(&folder, file))
            .and_then(|read| kept(&mut problems, read));
        let lookup = uses
            .lookup
            .then(|| crate::read_color_lookup(&folder, file))
            .and_then(|read| kept(&mut problems, read));
        let palette = uses
            .palette
            .then(|| crate::read_palette(&folder, file).map(|(read, _)| read))
            .and_then(|read| kept(&mut problems, read));
        let probe = uses
            .probe
            .then(|| read_probe(&folder, file))
            .and_then(|read| kept(&mut problems, read));
        let light_probes = uses
            .light_probes
            .then(|| read_light_probes(&folder, file))
            .and_then(|read| kept(&mut problems, read));

        // Every material made from the file, put together again.
        let mut materials = Vec::new();
        for material in &uses.materials {
            let mut errors = Vec::new();
            match material::resolve(&folder, material, &edited, &mut errors) {
                Some(resolved) if !resolved.broken => materials.push((material.clone(), resolved)),
                _ => problems.extend(errors),
            }
        }
        let table = uses
            .table
            .then(|| read_table(&folder, file))
            .and_then(|read| kept(&mut problems, read));

        let mut shared = self.lock();
        keep_stamp(&mut shared, file, now);
        if let Some(problem) = problems.into_iter().next() {
            if shared.reload_failed.insert(file.to_owned()) {
                shared.errors.push(format!(
                    "Could not reload `{file}`: {problem}. The last good version stays."
                ));
            }
            return Reload::Failed;
        }
        shared.reload_failed.remove(file);
        shared.generation += 1;
        for (mesh_id, model) in models {
            for warning in &model.warnings {
                shared
                    .errors
                    .push(format!("The model `{mesh_id}` {warning}."));
            }
            shared.models.insert(mesh_id, Some(Arc::new(model)));
        }
        if let Some(takes) = takes {
            shared.takes.insert(file.to_owned(), Some(Arc::new(takes)));
        }
        if let Some(image) = texture {
            shared
                .textures
                .insert(file.to_owned(), Some(Arc::new(image)));
        }
        if let Some(image) = sky_image {
            shared
                .sky_images
                .insert(file.to_owned(), Some(Arc::new(image)));
        }
        if let Some(read) = lookup {
            shared.lookups.insert(file.to_owned(), Some(Arc::new(read)));
        }
        if let Some(read) = palette {
            shared
                .palettes
                .insert(file.to_owned(), Some(Arc::new(read)));
        }
        if let Some(image) = probe {
            shared.probes.insert(file.to_owned(), Some(Arc::new(image)));
        }
        if let Some(probes) = light_probes {
            shared
                .light_probes
                .insert(file.to_owned(), Some(Arc::new(probes)));
        }
        for (material, resolved) in materials {
            shared.materials.insert(material, Some(resolved));
        }
        if let Some(table) = table {
            shared.tables.insert(file.to_owned(), Ok(Arc::new(table)));
        }
        Reload::Reloaded
    }
}

/// `read`'s value, or `None` with why added to `problems`.
fn kept<T>(problems: &mut Vec<String>, read: Result<T, String>) -> Option<T> {
    match read {
        Ok(value) => Some(value),
        Err(message) => {
            problems.push(message);
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    fn project(name: &str) -> PathBuf {
        let folder =
            std::env::temp_dir().join(format!("axcenity-reload-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&folder);
        fs::create_dir_all(folder.join("assets/models")).unwrap();
        fs::create_dir_all(folder.join("assets/textures")).unwrap();
        folder
    }

    /// A one-triangle glTF `width` studs wide, its buffer in a `.bin` file.
    fn write_triangle(folder: &Path, width: f32) {
        let corners: [f32; 9] = [0.0, 0.0, 0.0, width, 0.0, 0.0, 0.0, 1.0, 0.0];
        let bytes: Vec<u8> = corners.iter().flat_map(|c| c.to_le_bytes()).collect();
        let models = folder.join("assets/models");
        fs::write(models.join("tri.bin"), bytes).unwrap();
        let gltf = format!(
            r#"{{
                "asset": {{ "version": "2.0" }},
                "buffers": [ {{ "uri": "tri.bin", "byteLength": 36 }} ],
                "bufferViews": [ {{ "buffer": 0, "byteLength": 36 }} ],
                "accessors": [ {{ "bufferView": 0, "componentType": 5126, "count": 3,
                                  "type": "VEC3", "min": [0, 0, 0], "max": [{width}, 1, 0] }} ],
                "meshes": [ {{ "primitives": [ {{ "attributes": {{ "POSITION": 0 }} }} ] }} ]
            }}"#
        );
        fs::write(models.join("tri.gltf"), gltf).unwrap();
    }

    const TRIANGLE: &str = "assets/models/tri.gltf";

    #[test]
    fn a_changed_model_is_swapped_in_with_a_new_key() {
        let folder = project("model");
        write_triangle(&folder, 1.0);
        let assets = Assets::new(&folder);
        let old = assets.model_blocking(TRIANGLE).unwrap();
        assert_eq!(old.size, [1.0, 1.0, 0.0]);

        write_triangle(&folder, 2.5);
        assert_eq!(assets.reload(TRIANGLE), Reload::Reloaded);
        let new = assets.model_blocking(TRIANGLE).unwrap();
        assert_eq!(new.size, [2.5, 1.0, 0.0]);
        assert_ne!(new.key(), old.key());
        assert!(assets.take_errors().is_empty());
        let _ = fs::remove_dir_all(&folder);
    }

    #[test]
    fn a_changed_buffer_beside_a_model_reads_the_model_again() {
        let folder = project("buffer");
        write_triangle(&folder, 1.0);
        let assets = Assets::new(&folder);
        let old = assets.model_blocking(TRIANGLE).unwrap();
        // Only the buffer changes: the triangle's corners move.
        let corners: [f32; 9] = [0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.5, 0.0];
        let bytes: Vec<u8> = corners.iter().flat_map(|c| c.to_le_bytes()).collect();
        fs::write(folder.join("assets/models/tri.bin"), bytes).unwrap();
        assert_eq!(assets.reload("assets/models/tri.bin"), Reload::Reloaded);
        let new = assets.model_blocking(TRIANGLE).unwrap();
        assert_ne!(old.key(), new.key());
        // A model or clip beside it is its own file, so it's left alone.
        assert!(assets.models_reading("assets/models/other.glb").is_empty());
        assert!(
            assets
                .models_reading("assets/models/Walk.anim.json")
                .is_empty()
        );
        let _ = fs::remove_dir_all(&folder);
    }

    #[test]
    fn a_broken_model_keeps_the_last_good_one_and_is_reported_once() {
        let folder = project("broken-model");
        write_triangle(&folder, 1.0);
        let assets = Assets::new(&folder);
        let good = assets.model_blocking(TRIANGLE).unwrap();

        fs::write(folder.join(TRIANGLE), "{ not a model").unwrap();
        assert_eq!(assets.reload(TRIANGLE), Reload::Failed);
        let kept = assets.model_blocking(TRIANGLE).unwrap();
        assert!(Arc::ptr_eq(&good, &kept));
        let errors = assets.take_errors();
        assert_eq!(errors.len(), 1, "{errors:?}");
        assert!(errors[0].starts_with("Could not reload `assets/models/tri.gltf`"));
        assert!(errors[0].ends_with("The last good version stays."));
        // Asking for it again doesn't report it again.
        assets.model_blocking(TRIANGLE);
        assert!(assets.take_errors().is_empty());
        // Nor does saving it broken again before it's fixed.
        fs::write(folder.join(TRIANGLE), "{ still not a model").unwrap();
        assert_eq!(assets.reload(TRIANGLE), Reload::Failed);
        assert!(assets.take_errors().is_empty());

        // Fixed, it's swapped in.
        write_triangle(&folder, 3.75);
        assert_eq!(assets.reload(TRIANGLE), Reload::Reloaded);
        assert_eq!(
            assets.model_blocking(TRIANGLE).unwrap().size,
            [3.75, 1.0, 0.0]
        );
        let _ = fs::remove_dir_all(&folder);
    }

    #[test]
    fn a_changed_texture_is_read_again_and_a_broken_one_is_kept() {
        let folder = project("texture");
        let path = "assets/textures/checker.png";
        fs::write(folder.join(path), model::checker_png()).unwrap();
        let assets = Assets::new(&folder);
        let old = assets.texture(path).unwrap();
        // Not changed since it was read: nothing to do.
        assert_eq!(assets.reload(path), Reload::Unused);

        let mut bigger = Vec::new();
        image::RgbaImage::from_pixel(4, 2, image::Rgba([255, 0, 0, 255]))
            .write_to(
                &mut std::io::Cursor::new(&mut bigger),
                image::ImageFormat::Png,
            )
            .unwrap();
        fs::write(folder.join(path), bigger).unwrap();
        assert_eq!(assets.reload(path), Reload::Reloaded);
        let new = assets.texture(path).unwrap();
        assert_ne!(new.key(), old.key());
        assert_eq!((new.width, new.height), (4, 2));

        fs::write(folder.join(path), b"not a png").unwrap();
        assert_eq!(assets.reload(path), Reload::Failed);
        assert!(Arc::ptr_eq(&assets.texture(path).unwrap(), &new));
        assert_eq!(assets.take_errors().len(), 1);
        let _ = fs::remove_dir_all(&folder);
    }

    #[test]
    fn a_model_the_host_wrote_and_inserted_itself_isnt_read_again() {
        let folder = project("inserted");
        write_triangle(&folder, 1.0);
        let assets = Assets::new(&folder);
        let model = read_model(&folder, TRIANGLE).unwrap();
        let inserted = assets.insert(TRIANGLE, model);
        assert_eq!(assets.reload(TRIANGLE), Reload::Unused);
        assert!(Arc::ptr_eq(
            &assets.model_blocking(TRIANGLE).unwrap(),
            &inserted
        ));
        let _ = fs::remove_dir_all(&folder);
    }

    #[test]
    fn a_file_nothing_has_loaded_is_left_alone() {
        let folder = project("unused");
        write_triangle(&folder, 1.0);
        let assets = Assets::new(&folder);
        assert_eq!(assets.reload(TRIANGLE), Reload::Unused);
        assert_eq!(assets.reload("assets/fonts/Title.ttf"), Reload::Unused);
        assets.fonts();
        assert_eq!(assets.reload("assets/fonts/Title.ttf"), Reload::Reloaded);
        let _ = fs::remove_dir_all(&folder);
    }

    #[test]
    fn a_changed_data_table_is_swapped_in_and_a_broken_one_is_kept() {
        let folder = project("table");
        let path = "assets/data/weapons.data.json";
        let write = |damage: f64| {
            let mut table = DataTable::default();
            table.add_column("Damage", ColumnType::Number).unwrap();
            let sword = table.add_row("sword", None);
            table
                .set_cell(&sword, "Damage", Cell::Number(damage))
                .unwrap();
            write_data_table(&folder.join(path), &table).unwrap();
        };
        write(10.0);
        let assets = Assets::new(&folder);
        let damage = |assets: &Assets| {
            let table = assets.data_table("weapons").unwrap();
            table.row("sword").unwrap().cells["Damage"].clone()
        };
        assert_eq!(damage(&assets), Cell::Number(10.0));
        write(12.0);
        assert_eq!(assets.reload(path), Reload::Reloaded);
        assert_eq!(damage(&assets), Cell::Number(12.0));

        fs::write(folder.join(path), "{").unwrap();
        assert_eq!(assets.reload(path), Reload::Failed);
        assert_eq!(damage(&assets), Cell::Number(12.0));
        assert_eq!(assets.take_errors().len(), 1);

        // A missing table says which file it looked for.
        let missing = assets.data_table("shields").unwrap_err();
        assert!(
            missing.contains("assets/data/shields.data.json"),
            "{missing}"
        );
        let _ = fs::remove_dir_all(&folder);
    }

    #[test]
    fn a_changed_material_file_is_put_together_again() {
        let folder = project("material");
        fs::create_dir_all(folder.join("assets/materials")).unwrap();
        let path = "assets/materials/red.material.json";
        let write = |metalness: f32| {
            let file = MaterialFile {
                metalness: Some(metalness),
                ..MaterialFile::default()
            };
            fs::write(folder.join(path), material_json(&file)).unwrap()
        };
        write(1.0);
        let assets = Assets::new(&folder);
        let old = assets.material(path).unwrap();
        write(0.5);
        assert_eq!(assets.reload(path), Reload::Reloaded);
        let new = assets.material(path).unwrap();
        assert!(!Arc::ptr_eq(&old, &new));
        assert_eq!((old.metalness, new.metalness), (1.0, 0.5));

        fs::write(folder.join(path), "{").unwrap();
        assert_eq!(assets.reload(path), Reload::Failed);
        assert!(Arc::ptr_eq(&assets.material(path).unwrap(), &new));
        let _ = fs::remove_dir_all(&folder);
    }
}
