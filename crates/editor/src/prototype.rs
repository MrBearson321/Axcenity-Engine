//! Kenney's prototype textures (CC0, `assets/textures/prototype`), built
//! into the editor. Picking one in the Material tool copies its image into
//! the project and writes a material file for it there, so the project
//! stays self-contained and the player needs nothing special to draw it.

use std::collections::HashMap;
use std::io;
use std::path::Path;

use axcenity_assets::{MapFile, MaterialFile, material_json};
use eframe::egui;

/// The pack's color families, in the order the Material tool lists them.
pub const PALETTES: [&str; 6] = ["Dark", "Light", "Green", "Orange", "Purple", "Red"];
/// How many textures each family has, numbered from 1.
pub const COUNT: usize = 13;
/// How many pixels wide a thumbnail in the Material tool is.
const THUMBNAIL: u32 = 64;

macro_rules! palette {
    ($name:literal) => {
        palette!(@list $name, "01", "02", "03", "04", "05", "06", "07", "08", "09", "10", "11", "12", "13")
    };
    (@list $name:literal, $($number:literal),*) => {
        [$(include_bytes!(concat!(
            "../../../assets/textures/prototype/", $name, "/texture_", $number, ".png"
        )) as &[u8]),*]
    };
}

static IMAGES: [[&[u8]; COUNT]; PALETTES.len()] = [
    palette!("Dark"),
    palette!("Light"),
    palette!("Green"),
    palette!("Orange"),
    palette!("Purple"),
    palette!("Red"),
];

/// Where a prototype texture's image goes in a project.
pub fn image_path(palette: usize, number: usize) -> String {
    format!(
        "assets/textures/prototype/{}/texture_{number:02}.png",
        PALETTES[palette]
    )
}

/// Where a prototype texture's material file goes in a project.
pub fn material_path(palette: usize, number: usize) -> String {
    format!(
        "assets/materials/prototype/{} {number:02}.material.json",
        PALETTES[palette]
    )
}

/// What the Material tool and undo call a prototype texture.
pub fn name(palette: usize, number: usize) -> String {
    format!("Prototype {} {number:02}", PALETTES[palette])
}

/// The material file of a prototype texture.
fn material(palette: usize, number: usize) -> MaterialFile {
    MaterialFile {
        color: Some([1.0, 1.0, 1.0]),
        metalness: Some(0.0),
        roughness: Some(0.8),
        maps: axcenity_assets::MaterialMaps {
            color: Some(MapFile {
                path: Some(image_path(palette, number)),
                ..Default::default()
            }),
            ..Default::default()
        },
        ..Default::default()
    }
}

/// Puts a prototype texture into the project at `folder`, keeping whatever
/// is already at its paths. Gives the material file's path.
pub fn install(folder: &Path, palette: usize, number: usize) -> io::Result<String> {
    let bytes = IMAGES
        .get(palette)
        .and_then(|images| images.get(number.wrapping_sub(1)))
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no such prototype texture"))?;
    let write_new = |path: &str, contents: &[u8]| -> io::Result<()> {
        let file = folder.join(path);
        if file.exists() {
            return Ok(());
        }
        if let Some(parent) = file.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(file, contents)
    };
    write_new(&image_path(palette, number), bytes)?;
    let path = material_path(palette, number);
    write_new(&path, material_json(&material(palette, number)).as_bytes())?;
    Ok(path)
}

/// The thumbnails decoded so far, kept in egui's memory.
#[derive(Clone, Default)]
struct Thumbnails(HashMap<(usize, usize), egui::TextureHandle>);

/// The thumbnail of a prototype texture, if it's ready. Each is decoded the
/// first time it's wanted; `budget` is how many may still be decoded this
/// frame, so opening the tool never stalls.
pub fn thumbnail(
    ctx: &egui::Context,
    palette: usize,
    number: usize,
    budget: &mut u32,
) -> Option<egui::TextureId> {
    let memory = egui::Id::new("prototype thumbnails");
    let known = ctx.data_mut(|data| {
        data.get_temp_mut_or_default::<Thumbnails>(memory)
            .0
            .get(&(palette, number))
            .map(egui::TextureHandle::id)
    });
    if known.is_some() {
        return known;
    }
    if *budget == 0 {
        ctx.request_repaint();
        return None;
    }
    *budget -= 1;
    let bytes = IMAGES[palette][number - 1];
    let image = image::load_from_memory(bytes)
        .ok()?
        .thumbnail(THUMBNAIL, THUMBNAIL);
    let rgba = image.to_rgba8();
    let size = [rgba.width() as usize, rgba.height() as usize];
    let color = egui::ColorImage::from_rgba_unmultiplied(size, rgba.as_raw());
    let handle = ctx.load_texture(
        format!("prototype {palette} {number}"),
        color,
        egui::TextureOptions::LINEAR,
    );
    let id = handle.id();
    ctx.data_mut(|data| {
        data.get_temp_mut_or_default::<Thumbnails>(memory)
            .0
            .insert((palette, number), handle);
    });
    Some(id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_prototype_texture_is_a_readable_image() {
        for images in &IMAGES {
            for bytes in images {
                assert!(image::load_from_memory(bytes).is_ok());
            }
        }
    }

    fn scratch(test: &str) -> std::path::PathBuf {
        let folder =
            std::env::temp_dir().join(format!("axcenity-prototype-{test}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(&folder).unwrap();
        folder
    }

    #[test]
    fn installing_writes_the_image_and_a_material_naming_it_and_keeps_edits() {
        let dir = scratch("install");
        let path = install(&dir, 3, 5).unwrap();
        assert_eq!(path, "assets/materials/prototype/Orange 05.material.json");
        let file = axcenity_assets::read_material_file(&dir.join(&path)).unwrap();
        let image = file.maps.color.unwrap().path.unwrap();
        assert_eq!(image, "assets/textures/prototype/Orange/texture_05.png");
        assert!(dir.join(&image).is_file());

        // A material the user has since changed isn't written over.
        std::fs::write(dir.join(&path), "{}").unwrap();
        install(&dir, 3, 5).unwrap();
        assert_eq!(std::fs::read_to_string(dir.join(&path)).unwrap(), "{}");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_number_outside_the_pack_is_refused() {
        let dir = scratch("outside");
        assert!(install(&dir, 0, 0).is_err());
        assert!(install(&dir, 0, COUNT + 1).is_err());
        let _ = std::fs::remove_dir_all(dir);
    }
}
