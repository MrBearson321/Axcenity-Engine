//! What the editor's Generate button on a Fracture calls: breaks the part
//! the Fracture is in and saves the pieces next to the project's models.
//!
//! The pieces stay in the part's own space. For a Part that's its shape at
//! its `Size`, centered on the origin; for a MeshPart, its model's file
//! units. Loading the whole file then gives a model whose size is the
//! source's, which the runtime uses to scale each piece to the part's
//! current `Size`.

use std::path::Path;

use axcenity_assets::{AlphaMode, Assets, MODELS_FOLDER, Model, safe_file_name, unique_file_name};
use axcenity_core::{ClassName, Color3, Instance, InstanceId, Material, PartType, Value, World};
use glam::Vec3;

use crate::{GlbMaterial, Mesh, Pattern, fracture, write_glb};

/// What a Fracture asks for, read from its properties.
#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    /// How many pieces (`Pieces`), 1 to 256.
    pub pieces: usize,
    /// Which pieces (`Seed`): the same seed always breaks the same way.
    pub seed: u64,
    pub pattern: Pattern,
    /// How the inside of the cuts looks (`InteriorMaterial`), in the
    /// part's color.
    pub interior: Material,
    /// The file made last time (`PiecesId`), overwritten when generating
    /// again; empty for none.
    pub pieces_id: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            pieces: 12,
            seed: 0,
            pattern: Pattern::Uniform,
            interior: Material::Plastic,
            pieces_id: String::new(),
        }
    }
}

impl Settings {
    /// The settings of `fracture`, with defaults for any it doesn't have.
    pub fn of(fracture: &Instance) -> Self {
        let defaults = Self::default();
        let number = |name| fracture.property(name).and_then(Value::as_number);
        let pattern = match fracture.property("Pattern") {
            Some(Value::EnumItem(item))
                if item.enum_name == "FracturePattern" && item.item == "Impact" =>
            {
                Pattern::Impact
            }
            _ => Pattern::Uniform,
        };
        Self {
            pieces: number("Pieces").map_or(defaults.pieces, |n| n.round().max(1.0) as usize),
            // `as` turns NaN into 0 and stops at the largest u64.
            seed: number("Seed").map_or(defaults.seed, |n| n.max(0.0) as u64),
            pattern,
            interior: fracture
                .property("InteriorMaterial")
                .and_then(Value::as_material)
                .unwrap_or(defaults.interior),
            pieces_id: fracture
                .property("PiecesId")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned(),
        }
    }
}

/// Cuts the part that the Fracture `fracture` is parented to into pieces by
/// the Fracture's `Pieces`, `Seed`, `Pattern` and `InteriorMaterial`, writes
/// `<name>.fracture.glb`, and returns its project-relative path (to be
/// stored in the Fracture's `PiecesId`).
pub fn generate(
    world: &World,
    assets: &Assets,
    project: &Path,
    fracture: InstanceId,
) -> Result<String, String> {
    let instance = world
        .get(fracture)
        .ok_or_else(|| "The Fracture isn't in the game any more".to_owned())?;
    let part = instance.parent().ok_or_else(not_in_a_part)?;
    generate_for(world, assets, project, part, &Settings::of(instance))
}

/// Cuts `part` (a Part or MeshPart) by `settings`, writes the pieces, and
/// returns the file's project-relative path. See [`generate`].
pub fn generate_for(
    world: &World,
    assets: &Assets,
    project: &Path,
    part: InstanceId,
    settings: &Settings,
) -> Result<String, String> {
    let instance = world.get(part).ok_or_else(not_in_a_part)?;
    let source = if instance.class() == ClassName::MeshPart {
        mesh_part(instance, assets, project)?
    } else if instance.class().is_base_part() {
        part_source(instance)?
    } else {
        return Err(not_in_a_part());
    };

    let pieces = fracture(
        &source.mesh,
        settings.pieces,
        settings.seed,
        settings.pattern,
    );
    if pieces.is_empty() {
        return Err(format!(
            "`{}` has nothing to break: its mesh has no volume",
            instance.name()
        ));
    }
    let interior = interior_material(settings.interior, &source.materials);
    let bytes = write_glb(&pieces, &source.materials, &interior);

    let path = file_path(instance, &source.folder, project, &settings.pieces_id);
    let full = project.join(&path);
    if let Some(folder) = full.parent() {
        std::fs::create_dir_all(folder)
            .map_err(|err| format!("Could not make the folder for `{path}`: {err}"))?;
    }
    std::fs::write(&full, bytes).map_err(|err| format!("Could not write `{path}`: {err}"))?;
    // A file written again must be read again, not taken from the cache.
    assets.forget(&path);
    Ok(path)
}

fn not_in_a_part() -> String {
    "A Fracture breaks the Part or MeshPart it's in".to_owned()
}

/// The mesh to break, its looks, and the folder its pieces go in.
struct Source {
    mesh: Mesh,
    materials: Vec<GlbMaterial>,
    /// Project-relative, with `/` between folders.
    folder: String,
}

/// A Part's shape at its size, in its one look.
fn part_source(part: &Instance) -> Result<Source, String> {
    Ok(Source {
        mesh: part_mesh(part, "fractured")?,
        materials: vec![part_look(part)],
        folder: MODELS_FOLDER.to_owned(),
    })
}

/// A Part's shape at its `Size`, centered on the origin. `verb` says what
/// a Capsule can't be yet ("fractured").
pub(crate) fn part_mesh(part: &Instance, verb: &str) -> Result<Mesh, String> {
    let size = part
        .property("Size")
        .and_then(Value::as_vector3)
        .map_or(Vec3::new(4.0, 1.0, 2.0), |s| Vec3::new(s.x, s.y, s.z))
        .abs();
    let shape = part
        .property("Shape")
        .and_then(Value::as_part_type)
        .unwrap_or(PartType::Block);
    Ok(match shape {
        PartType::Block => Mesh::block(size),
        PartType::Wedge => Mesh::wedge(size),
        PartType::Ball => Mesh::ball(size),
        PartType::Cylinder => Mesh::cylinder(size),
        PartType::Capsule => {
            return Err(format!(
                "Capsules can't be {verb} yet; use a Block, Wedge, Ball or Cylinder"
            ));
        }
    })
}

/// A Part's one look: its `Color`, `Metalness`, `Roughness`, and its
/// Material's opacity and glow.
pub(crate) fn part_look(part: &Instance) -> GlbMaterial {
    let color = part
        .property("Color")
        .and_then(Value::as_color3)
        .unwrap_or(Color3::new(0.64, 0.64, 0.64));
    let material = part
        .property("Material")
        .and_then(Value::as_material)
        .unwrap_or(Material::Plastic);
    let preset = material.preset();
    let number = |name, default: f64| {
        part.property(name)
            .and_then(Value::as_number)
            .unwrap_or(default) as f32
    };
    let linear = [color.r, color.g, color.b].map(srgb_to_linear);
    GlbMaterial {
        name: material.as_str().to_owned(),
        base_color: [linear[0], linear[1], linear[2], preset.opacity],
        metallic: number("Metalness", preset.metalness),
        roughness: number("Roughness", preset.roughness),
        emissive: if preset.glows { linear } else { [0.0; 3] },
        blend: preset.opacity < 1.0,
        base_color_texture: None,
    }
}

/// A MeshPart's model in its file's units, with the model's materials. The
/// pieces go next to the model, so its textures' paths still work.
fn mesh_part(part: &Instance, assets: &Assets, project: &Path) -> Result<Source, String> {
    let mesh_id = part
        .property("MeshId")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if mesh_id.is_empty() {
        return Err(format!("`{}` has no model to break", part.name()));
    }
    let model = assets
        .model_blocking(mesh_id)
        .ok_or_else(|| format!("Could not load the model `{mesh_id}`"))?;
    let file = mesh_id.split('#').next().unwrap_or(mesh_id);
    let folder = file.rsplit_once('/').map_or("", |(folder, _)| folder);
    let mesh = Mesh::from_model(&model);
    Ok(Source {
        mesh,
        materials: model_materials(&model, &project.join(folder)),
        folder: folder.to_owned(),
    })
}

/// A material for each of `model`'s primitives. A texture is kept only if
/// its file is in `folder`, where the pieces' file will look for it.
pub(crate) fn model_materials(model: &Model, folder: &Path) -> Vec<GlbMaterial> {
    model
        .primitives
        .iter()
        .map(|primitive| {
            let material = &primitive.material;
            let texture = material
                .base_color_texture
                .as_ref()
                .and_then(|image| image.file.clone())
                .filter(|file| {
                    axcenity_assets::safe_path(file).is_some_and(|file| folder.join(file).is_file())
                });
            GlbMaterial {
                name: material.name.clone(),
                base_color: material.base_color,
                metallic: material.metalness,
                roughness: material.roughness,
                emissive: material.emissive,
                blend: material.alpha_mode == AlphaMode::Blend,
                base_color_texture: texture,
            }
        })
        .collect()
}

/// The inside of the cuts: `material`'s preset in the color of the first
/// look (the part's color).
fn interior_material(material: Material, looks: &[GlbMaterial]) -> GlbMaterial {
    let preset = material.preset();
    let [r, g, b, _] = looks.first().map_or([1.0; 4], |look| look.base_color);
    GlbMaterial {
        name: format!("Interior {}", material.as_str()),
        base_color: [r, g, b, preset.opacity],
        metallic: preset.metalness as f32,
        roughness: preset.roughness as f32,
        emissive: if preset.glows { [r, g, b] } else { [0.0; 3] },
        blend: preset.opacity < 1.0,
        base_color_texture: None,
    }
}

/// Where the pieces go: `pieces_id` again if it's a `.glb` in the project,
/// so generating again replaces the old file; otherwise
/// `<folder>/<part name>.fracture.glb`, numbered if that's taken.
fn file_path(part: &Instance, folder: &str, project: &Path, pieces_id: &str) -> String {
    let file = pieces_id.split('#').next().unwrap_or_default();
    if file.to_ascii_lowercase().ends_with(".glb") && axcenity_assets::safe_path(file).is_some() {
        return file.to_owned();
    }
    let stem = safe_file_name(part.name()).unwrap_or_else(|| "Part".to_owned());
    let name = unique_file_name(&format!("{stem}.fracture.glb"), |name| {
        project.join(folder).join(name).exists()
    });
    if folder.is_empty() {
        name
    } else {
        format!("{folder}/{name}")
    }
}

/// A color channel as people pick it (sRGB) to the linear value glTF wants.
fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}
