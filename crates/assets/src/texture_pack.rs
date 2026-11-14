//! Texture packs: the maps of one material that arrive as loose files (a
//! Poly Haven download's `_diff_2k`, `_nor_gl_2k`, `_rough_2k`…, an
//! ambientCG one's `_Color`, `_NormalGL`…), kept in a folder of their own
//! with a [texture set](crate::TextureSet), so the Assets panel shows one
//! tile per material instead of every map.
//!
//! Importing several maps of a material puts them in a pack folder
//! ([`import_textures`]); [`plan_pack_folders`] works out how to tidy maps
//! already in a project, which the editor moves with their references.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use crate::import::{ImportError, TEXTURE_EXTENSIONS, import_texture, safe_file_name};
use crate::model::{MapSlot, texture_pack_name, texture_roles};
use crate::texture_set::{
    TEXTURE_SET_SUFFIX, TextureSet, read_texture_set, texture_set_from_files, write_texture_set,
};

/// Files that are maps of one material.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PackGroup {
    /// The material's name, as the first file names it (`Moss002`).
    pub name: String,
    /// The files, as places in the list they came from, in its order.
    pub members: Vec<usize>,
}

/// Which of `names` (file names or paths) are maps of the same material:
/// files whose [`texture_pack_name`]s match (case ignored), at least two of
/// them, filling at least two different maps. So two unrelated pictures
/// that happen to end in `_a` and `_n` aren't put together, but a colour map
/// and its normal map are.
pub fn texture_packs(names: &[&str]) -> Vec<PackGroup> {
    let mut groups: Vec<(String, PackGroup)> = Vec::new();
    for (index, name) in names.iter().enumerate() {
        let file = name.rsplit(['/', '\\']).next().unwrap_or(name);
        let Some(pack) = texture_pack_name(file) else {
            continue;
        };
        let key = pack.to_lowercase();
        match groups.iter_mut().find(|(other, _)| *other == key) {
            Some((_, group)) => group.members.push(index),
            None => groups.push((
                key,
                PackGroup {
                    name: pack,
                    members: vec![index],
                },
            )),
        }
    }
    groups
        .into_iter()
        .map(|(_, group)| group)
        .filter(|group| {
            let mut slots: Vec<MapSlot> = Vec::new();
            for role in group.members.iter().flat_map(|&i| texture_roles(names[i])) {
                if !slots.contains(&role.slot) {
                    slots.push(role.slot);
                }
            }
            group.members.len() >= 2 && slots.len() >= 2
        })
        .collect()
}

/// The folder in the project folder `parent` (`assets/textures`) that the
/// pack `name`'s maps go in.
pub fn pack_folder(parent: &str, name: &str) -> String {
    let name = safe_file_name(name).unwrap_or_else(|| "Texture".to_owned());
    format!("{}/{name}", parent.trim_end_matches('/'))
}

/// The texture set file already in the project folder `folder`, if there
/// is one (the one named after the folder first).
pub fn pack_set_file(project: &Path, folder: &str) -> Option<String> {
    let own = folder.rsplit('/').next().unwrap_or(folder);
    let own = format!("{folder}/{own}{TEXTURE_SET_SUFFIX}");
    if project.join(&own).is_file() {
        return Some(own);
    }
    let mut found: Vec<String> = fs::read_dir(project.join(folder))
        .ok()?
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            name.to_lowercase()
                .ends_with(TEXTURE_SET_SUFFIX)
                .then(|| format!("{folder}/{name}"))
        })
        .collect();
    found.sort();
    found.into_iter().next()
}

/// The texture set for the pack `name` in the project folder `folder`: its
/// maps from the images already there and `images` (project paths, which
/// may not be there yet), keeping where a set already there came from.
/// Returns where it's saved and the set.
pub fn pack_texture_set(
    project: &Path,
    folder: &str,
    name: &str,
    images: &[String],
) -> (String, TextureSet) {
    let existing = pack_set_file(project, folder);
    let old = existing
        .as_ref()
        .and_then(|path| read_texture_set(&project.join(path)).ok());
    let mut paths: BTreeSet<String> = images.iter().cloned().collect();
    if let Ok(entries) = fs::read_dir(project.join(folder)) {
        for entry in entries.flatten() {
            let file = entry.file_name().to_string_lossy().into_owned();
            let extension = file
                .rsplit_once('.')
                .map(|(_, extension)| extension.to_lowercase())
                .unwrap_or_default();
            if entry.path().is_file() && TEXTURE_EXTENSIONS.contains(&extension.as_str()) {
                paths.insert(format!("{folder}/{file}"));
            }
        }
    }
    let files: Vec<(&str, &str)> = paths
        .iter()
        .map(|path| (path.rsplit('/').next().unwrap_or(path), path.as_str()))
        .collect();
    let (source, asset_id, link) = match &old {
        Some(set) => (
            set.source.as_str(),
            set.asset_id.as_str(),
            set.link.as_str(),
        ),
        None => ("", name, ""),
    };
    let mut set = texture_set_from_files(source, asset_id, link, &files);
    // Maps a set already there named from elsewhere stay.
    if let Some(old) = old {
        for map in old.maps {
            if !set.maps.iter().any(|kept| kept.slot == map.slot) {
                set.maps.push(map);
            }
        }
    }
    let path = existing.unwrap_or_else(|| {
        let file = folder.rsplit('/').next().unwrap_or(folder);
        format!("{folder}/{file}{TEXTURE_SET_SUFFIX}")
    });
    (path, set)
}

/// What [`import_textures`] did.
#[derive(Debug, Default)]
pub struct TexturesImport {
    /// Each file, with its path in the project or why it couldn't be
    /// copied, in the order given.
    pub files: Vec<(PathBuf, Result<String, ImportError>)>,
    /// The texture sets written for packs (project paths), or why one
    /// couldn't be.
    pub sets: Vec<Result<String, String>>,
}

/// Imports the texture `files`, as [`import_texture`] does each, into the
/// project folder `into`, except that the maps of one material from
/// outside the project ([`texture_packs`]) go in a folder of their own in
/// `packs_into` ([`pack_folder`]) with a texture set naming them. A map
/// whose pack already has its folder and set there joins it. Files already
/// in the project stay where they are.
pub fn import_textures(
    project: &Path,
    files: &[PathBuf],
    into: &str,
    packs_into: &str,
) -> TexturesImport {
    let outside = |file: &Path| match (project.canonicalize(), file.canonicalize()) {
        (Ok(project), Ok(file)) => !file.starts_with(project),
        _ => true,
    };
    let names: Vec<String> = files
        .iter()
        .map(|file| {
            file.file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default()
        })
        .collect();
    // Each file's pack folder, if it goes in one.
    let mut folders: Vec<Option<(String, String)>> = vec![None; files.len()];
    let candidates: Vec<usize> = (0..files.len())
        .filter(|&index| outside(&files[index]))
        .collect();
    let candidate_names: Vec<&str> = candidates
        .iter()
        .map(|&index| names[index].as_str())
        .collect();
    for group in texture_packs(&candidate_names) {
        let folder = pack_folder(packs_into, &group.name);
        for member in group.members {
            folders[candidates[member]] = Some((folder.clone(), group.name.clone()));
        }
    }
    for &index in &candidates {
        if folders[index].is_some() {
            continue;
        }
        if let Some(name) = texture_pack_name(&names[index]) {
            let folder = pack_folder(packs_into, &name);
            if pack_set_file(project, &folder).is_some() {
                folders[index] = Some((folder, name));
            }
        }
    }

    let mut import = TexturesImport::default();
    // Each pack folder (in first-seen order), its name and the maps copied in.
    let mut packs: Vec<(String, String, Vec<String>)> = Vec::new();
    for (file, pack) in files.iter().zip(&folders) {
        let result = match pack {
            Some((folder, _)) => import_texture(project, file, folder),
            None => import_texture(project, file, into),
        };
        if let (Some((folder, name)), Ok(path)) = (pack, &result) {
            match packs.iter_mut().find(|(other, _, _)| other == folder) {
                Some((_, _, maps)) => maps.push(path.clone()),
                None => packs.push((folder.clone(), name.clone(), vec![path.clone()])),
            }
        }
        import.files.push((file.clone(), result));
    }
    for (folder, name, maps) in packs {
        let (path, set) = pack_texture_set(project, &folder, &name, &maps);
        import.sets.push(
            write_texture_set(&project.join(&path), &set)
                .map(|()| path.clone())
                .map_err(|err| format!("could not write `{path}`: {err}")),
        );
    }
    import
}

/// One pack's maps to move into a folder of their own.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PackMove {
    /// The material's name (`brown_mud_leaves_01`).
    pub name: String,
    /// The folder they go in (`assets/textures/brown_mud_leaves_01`).
    pub folder: String,
    /// Each map's path now and in the folder.
    pub moves: Vec<(String, String)>,
}

/// How to tidy the loose maps among `images` (project paths of the images
/// in `project` that aren't in a texture set's folder): each pack's maps
/// ([`texture_packs`], among the images in the same folder) moved into a
/// folder of their own beside them. A pack whose folder is taken by
/// something other than a texture set is left as it is, as is a map whose
/// name is taken in the folder. Writes nothing.
pub fn plan_pack_folders(project: &Path, images: &[String]) -> Vec<PackMove> {
    let parent = |path: &str| {
        path.rsplit_once('/')
            .map_or("", |(parent, _)| parent)
            .to_owned()
    };
    let mut parents: Vec<String> = images.iter().map(|path| parent(path)).collect();
    parents.sort_by_key(|folder| folder.to_lowercase());
    parents.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
    let mut plan = Vec::new();
    for folder in parents {
        let here: Vec<&str> = images
            .iter()
            .filter(|path| parent(path).eq_ignore_ascii_case(&folder))
            .map(String::as_str)
            .collect();
        for group in texture_packs(&here) {
            let target = pack_folder(&folder, &group.name);
            let disk = project.join(&target);
            if disk.exists() && (!disk.is_dir() || pack_set_file(project, &target).is_none()) {
                continue;
            }
            let moves: Vec<(String, String)> = group
                .members
                .iter()
                .map(|&index| {
                    let from = here[index];
                    let file = from.rsplit('/').next().unwrap_or(from);
                    (from.to_owned(), format!("{target}/{file}"))
                })
                .filter(|(_, to)| !project.join(to).exists())
                .collect();
            if !moves.is_empty() {
                plan.push(PackMove {
                    name: group.name,
                    folder: target,
                    moves,
                });
            }
        }
    }
    plan
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The maps Paul's Abandoned Farmhouse terrain came with (Poly Haven
    /// and ambientCG names), plus a picture of nothing in particular.
    const FARMHOUSE: [&str; 13] = [
        "brown_mud_leaves_01_ao_2k.jpg",
        "brown_mud_leaves_01_diff_2k.jpg",
        "brown_mud_leaves_01_disp_2k.jpg",
        "brown_mud_leaves_01_nor_gl_2k.jpg",
        "brown_mud_leaves_01_rough_2k.jpg",
        "leafy_grass_ao_2k.jpg",
        "leafy_grass_nor_gl_2k.jpg",
        "Moss002_2K-JPG_AmbientOcclusion.jpg",
        "Moss002_2K-JPG_Color.jpg",
        "Moss002_2K-JPG_Displacement.jpg",
        "Moss002_2K-JPG_NormalGL.jpg",
        "Moss002_2K-JPG_Roughness.jpg",
        "logo.png",
    ];

    #[test]
    fn a_maps_pack_name_is_its_name_without_the_map_size_and_format() {
        let cases = [
            ("brown_mud_leaves_01_nor_gl_2k.jpg", "brown_mud_leaves_01"),
            ("brown_mud_leaves_01_diff_2k.jpg", "brown_mud_leaves_01"),
            ("dry_decay_leaves_disp_2k.jpg", "dry_decay_leaves"),
            ("forest_leaves_04_arm_4k.png", "forest_leaves_04"),
            ("Moss002_2K-JPG_AmbientOcclusion.jpg", "Moss002"),
            ("Moss002_2K-JPG_NormalDX.jpg", "Moss002"),
            ("Fabric086_4K-PNG_Color.png", "Fabric086"),
            ("T_Bench_N.tga", "T_Bench"),
            ("Rock_BaseColor_4K.png", "Rock"),
            ("C:/Downloads/wall-normal.png", "wall"),
            ("leaves_AO_01.png", "leaves"),
        ];
        for (file, pack) in cases {
            assert_eq!(texture_pack_name(file).as_deref(), Some(pack), "{file}");
        }
        for file in ["logo.png", "diffuse.png", "n.png", "Color_rock.png", ""] {
            assert_eq!(texture_pack_name(file), None, "{file}");
        }
    }

    #[test]
    fn maps_of_one_material_are_one_pack() {
        let packs = texture_packs(&FARMHOUSE);
        let names: Vec<&str> = packs.iter().map(|pack| pack.name.as_str()).collect();
        assert_eq!(names, ["brown_mud_leaves_01", "leafy_grass", "Moss002"]);
        assert_eq!(packs[0].members, [0, 1, 2, 3, 4]);
        assert_eq!(packs[2].members, [7, 8, 9, 10, 11]);
    }

    #[test]
    fn two_files_for_the_same_map_are_not_a_pack() {
        // Two colour pictures that happen to be named alike stay loose.
        assert!(texture_packs(&["tree_a.png", "tree_d.png"]).is_empty());
        assert!(texture_packs(&["moss_color.png"]).is_empty());
    }

    fn temp_project(name: &str) -> PathBuf {
        let folder = std::env::temp_dir().join(format!(
            "axcenity-texture-pack-{name}-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&folder);
        fs::create_dir_all(&folder).unwrap();
        folder
    }

    #[test]
    fn importing_a_downloads_maps_puts_them_in_a_folder_with_a_texture_set() {
        let project = temp_project("import");
        let downloads = project.with_extension("downloads");
        let _ = fs::remove_dir_all(&downloads);
        fs::create_dir_all(&downloads).unwrap();
        let files: Vec<PathBuf> = FARMHOUSE
            .iter()
            .map(|name| {
                let file = downloads.join(name);
                fs::write(&file, name.as_bytes()).unwrap();
                file
            })
            .collect();

        let import = import_textures(&project, &files, "assets/images", "assets/textures");
        let paths: Vec<String> = import
            .files
            .iter()
            .map(|(_, result)| result.as_ref().unwrap().clone())
            .collect();
        assert_eq!(
            paths[3],
            "assets/textures/brown_mud_leaves_01/brown_mud_leaves_01_nor_gl_2k.jpg"
        );
        assert_eq!(paths[8], "assets/textures/Moss002/Moss002_2K-JPG_Color.jpg");
        // A file that isn't a map goes where images go.
        assert_eq!(paths[12], "assets/images/logo.png");
        let sets: Vec<String> = import.sets.into_iter().map(Result::unwrap).collect();
        assert_eq!(
            sets,
            [
                "assets/textures/brown_mud_leaves_01/brown_mud_leaves_01.texture.json",
                "assets/textures/leafy_grass/leafy_grass.texture.json",
                "assets/textures/Moss002/Moss002.texture.json",
            ]
        );
        let set = read_texture_set(&project.join(&sets[2])).unwrap();
        assert_eq!(set.asset_id, "Moss002");
        assert_eq!(set.source, "");
        assert_eq!(
            set.path(MapSlot::Normal),
            Some("assets/textures/Moss002/Moss002_2K-JPG_NormalGL.jpg")
        );
        assert_eq!(set.maps.len(), 5);

        // A map of the same material brought in later joins its folder.
        let more = downloads.join("leafy_grass_diff_2k.jpg");
        fs::write(&more, b"colour").unwrap();
        let import = import_textures(&project, &[more], "assets/images", "assets/textures");
        assert_eq!(
            import.files[0].1.as_ref().unwrap(),
            "assets/textures/leafy_grass/leafy_grass_diff_2k.jpg"
        );
        let set =
            read_texture_set(&project.join("assets/textures/leafy_grass/leafy_grass.texture.json"))
                .unwrap();
        assert_eq!(set.maps.len(), 3);
        assert!(set.path(MapSlot::BaseColor).is_some());

        let _ = fs::remove_dir_all(&project);
        let _ = fs::remove_dir_all(&downloads);
    }

    #[test]
    fn loose_maps_already_in_a_project_are_planned_into_folders_beside_them() {
        let project = temp_project("plan");
        let folder = "assets/textures/abandoned_farmhouse/terrain";
        fs::create_dir_all(project.join(folder)).unwrap();
        let images: Vec<String> = FARMHOUSE
            .iter()
            .map(|name| format!("{folder}/{name}"))
            .collect();
        // Something else already called `leafy_grass` there: that pack stays.
        fs::write(project.join(format!("{folder}/leafy_grass")), b"x").unwrap();

        let plan = plan_pack_folders(&project, &images);
        let names: Vec<&str> = plan.iter().map(|pack| pack.name.as_str()).collect();
        assert_eq!(names, ["brown_mud_leaves_01", "Moss002"]);
        assert_eq!(plan[0].folder, format!("{folder}/brown_mud_leaves_01"));
        assert_eq!(
            plan[0].moves[0],
            (
                format!("{folder}/brown_mud_leaves_01_ao_2k.jpg"),
                format!("{folder}/brown_mud_leaves_01/brown_mud_leaves_01_ao_2k.jpg"),
            )
        );
        assert_eq!(plan[1].moves.len(), 5);

        let (path, set) = pack_texture_set(
            &project,
            &plan[1].folder,
            &plan[1].name,
            &plan[1]
                .moves
                .iter()
                .map(|(_, to)| to.clone())
                .collect::<Vec<_>>(),
        );
        assert_eq!(path, format!("{folder}/Moss002/Moss002.texture.json"));
        assert_eq!(set.maps.len(), 5);
        let _ = fs::remove_dir_all(&project);
    }
}
