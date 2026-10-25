use super::*;

/// Every Terrain's data folder, to keep new ones apart.
pub(super) fn terrain_folders(world: &World) -> Vec<String> {
    crate::terrain_studio::terrains(world)
        .into_iter()
        .chain(storage_terrains(world))
        .filter_map(|id| axcenity_project::data_folder(world, id))
        .collect()
}

/// Terrains kept in ReplicatedStorage (saved, but not drawn).
pub(super) fn storage_terrains(world: &World) -> Vec<InstanceId> {
    world
        .descendants(world.replicated_storage())
        .into_iter()
        .filter(|&id| world.get(id).map(|i| i.class()) == Some(ClassName::Terrain))
        .collect()
}

/// The saved landscapes of `world`'s terrains, to tell later changes apart.
pub(in crate::app) fn terrain_revisions(world: &World) -> HashMap<InstanceId, (u64, u64)> {
    crate::terrain_studio::terrains(world)
        .into_iter()
        .chain(storage_terrains(world))
        .filter_map(|id| {
            let data = world.terrain(id)?;
            Some((id, (data.uid(), data.revision())))
        })
        .collect()
}

impl EditorApp {
    /// Picks a PNG and makes it the terrain's heights: its size becomes the
    /// terrain's resolution. Paint is cleared (it wouldn't line up).
    pub(super) fn import_heightmap(&mut self) {
        let Some(terrain) = self.terrain.target else {
            return;
        };
        let Some(folder) = self.project.as_ref().map(|project| project.folder.clone()) else {
            return;
        };
        let Some(file) = rfd::FileDialog::new()
            .set_title("Import a heightmap")
            .add_filter("PNG image", &["png"])
            .set_directory(&folder)
            .pick_file()
        else {
            return;
        };
        let image = std::fs::read(&file)
            .map_err(|err| err.to_string())
            .and_then(|bytes| files::decode_image(&bytes));
        let data = image.and_then(|image| {
            let mut data =
                TerrainData::flat(image.width, image.height, 0.0).map_err(|err| err.to_string())?;
            let all = data.bounds();
            data.heights_mut_in(all).copy_from_slice(&image.values);
            data.clamp_heights_in(all);
            Ok(data)
        });
        match data {
            Ok(data) => {
                let label = format!(
                    "Import Heightmap {}",
                    file.file_name()
                        .map_or(String::new(), |n| n.to_string_lossy().into_owned())
                );
                self.replace_terrain(terrain, label, &data, None);
            }
            Err(err) => self
                .output
                .error(format!("Could not import `{}`: {err}", file.display())),
        }
    }

    /// Saves the terrain's heights as a 16-bit greyscale PNG.
    pub(super) fn export_heightmap(&mut self) {
        let Some(terrain) = self.terrain.target else {
            return;
        };
        let Some(folder) = self.project.as_ref().map(|project| project.folder.clone()) else {
            return;
        };
        let Some(data) = self.world.terrain(terrain).cloned() else {
            return;
        };
        let name = self
            .world
            .get(terrain)
            .map_or("Terrain".to_owned(), |t| t.name().to_string());
        let Some(file) = rfd::FileDialog::new()
            .set_title("Export the heightmap")
            .add_filter("PNG image", &["png"])
            .set_directory(&folder)
            .set_file_name(format!("{name}-heights.png"))
            .save_file()
        else {
            return;
        };
        let (size_x, size_z) = data.size();
        let bytes = files::encode_heights(size_x, size_z, data.heights());
        match std::fs::write(&file, bytes) {
            Ok(()) => self.output.info(format!(
                "Exported {name}'s heights ({size_x} × {size_z}) to `{}`.",
                file.display()
            )),
            Err(err) => self
                .output
                .error(format!("Could not write `{}`: {err}", file.display())),
        }
    }

    /// Picks a greyscale PNG for the Stamp tool (copied into the project).
    pub(super) fn pick_stamp_image(&mut self) {
        let Some(path) = self.pick_texture(STAMP_FOLDER) else {
            return;
        };
        self.terrain.stamp_image = Some(path);
        self.terrain.stamp.image = None;
        self.load_stamp_image();
    }

    /// Reads the Stamp tool's image. A bad one is reported and dropped.
    pub(super) fn load_stamp_image(&mut self) {
        let (Some(folder), Some(path)) = (
            self.project.as_ref().map(|project| project.folder.clone()),
            self.terrain.stamp_image.clone(),
        ) else {
            return;
        };
        let image = std::fs::read(folder.join(&path))
            .map_err(|err| err.to_string())
            .and_then(|bytes| files::decode_image(&bytes));
        match image {
            Ok(image) => {
                self.terrain.stamp.image = Some(Arc::new(StampImage {
                    width: image.width,
                    height: image.height,
                    values: image.values,
                }));
            }
            Err(err) => {
                self.output
                    .error(format!("Could not use `{path}` as a stamp: {err}"));
                self.terrain.stamp_image = None;
                self.terrain.stamp.image = None;
            }
        }
    }

    /// Writes every terrain whose landscape changed since it was loaded or
    /// last saved. A terrain without a data folder, or sharing one (a
    /// duplicate), gets its own first. Returns false, with the reason in
    /// Output, if one couldn't be written.
    pub(in crate::app) fn save_terrains(&mut self) -> bool {
        let Some(project) = &mut self.project else {
            return false;
        };
        let folder = project.folder.clone();
        let mut used: Vec<String> = Vec::new();
        for id in crate::terrain_studio::terrains(&self.world)
            .into_iter()
            .chain(storage_terrains(&self.world))
        {
            let Some(data) = self.world.terrain(id).cloned() else {
                continue;
            };
            let mut data_folder = axcenity_project::data_folder(&self.world, id);
            let shared = data_folder
                .as_ref()
                .is_some_and(|f| used.iter().any(|u| u.eq_ignore_ascii_case(f)));
            if data_folder.is_none() || shared {
                let name = self
                    .world
                    .get(id)
                    .map_or("Terrain".to_owned(), |t| t.name().to_string());
                let mut taken = terrain_folders(&self.world);
                taken.extend(used.iter().cloned());
                let fresh = axcenity_project::new_terrain_folder(&folder, &name, &taken);
                // Part of the world being saved, not an edit to undo.
                let _ = self
                    .world
                    .set_property(id, "DataFolder", Value::String(fresh.clone()));
                data_folder = Some(fresh);
            }
            let data_folder = data_folder.expect("given one above");
            used.push(data_folder.clone());
            let saved = (data.uid(), data.revision());
            if project.saved_terrains.get(&id) == Some(&saved)
                && folder.join(&data_folder).exists()
                && !shared
            {
                continue;
            }
            if let Some(at) = project
                .damaged_terrains
                .iter()
                .position(|damaged| *damaged == data_folder)
            {
                match axcenity_project::keep_terrain_folder(&folder, &data_folder) {
                    Ok(Some(kept)) => self.output.info(format!(
                        "Kept the terrain files the editor couldn't load as `{kept}`."
                    )),
                    Ok(None) => {}
                    Err(err) => {
                        self.output.error(format!(
                            "Could not save: {}",
                            super::super::files::chain(&err)
                        ));
                        return false;
                    }
                }
                project.damaged_terrains.remove(at);
            }
            if let Err(err) = axcenity_project::save_terrain(&folder, &data_folder, &data) {
                self.output.error(format!(
                    "Could not save: {}",
                    super::super::files::chain(&err)
                ));
                return false;
            }
            project.saved_terrains.insert(id, saved);
        }
        true
    }
}
