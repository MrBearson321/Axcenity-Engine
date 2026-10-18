//! Reading FBX models (binary or ASCII, any version) with `ufbx`, into the
//! same triangles and materials as glTF.
//!
//! `ufbx` converts the file to glTF's space as it loads: Y up, right-handed,
//! 1 unit = 1 meter (= 1 stud), whatever axes and units the file was saved
//! with. It reads only the bytes it's given; textures that aren't embedded
//! are looked for here, inside the model's folder.
//!
//! FBX files often name textures by a path on the machine they were made
//! on, or in a format the engine can't decode (`.psd`, `.dds`). Such a
//! texture is left out and noted in [`Model::warnings`]; the model still
//! loads, drawn with its materials' colors.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use glam::{DMat3, DMat4, DVec3, Vec3};
use ufbx::{CoordinateAxes, CoordinateAxis, ShaderType};

use super::objects::{self, Listed, Siblings};
use super::{
    AlphaMode, Channel, ChannelTexture, Image, MAX_NODE_DEPTH, Model, ModelError, ModelMaterial,
    ModelObject, Pick, Primitive, fit, missing, new_key, smooth_normals, want,
};

/// Reads an FBX file's `bytes`, making one model for each of `picks`.
/// Textures that aren't embedded in it are read from `folder`, the folder
/// the model is in.
pub(super) fn load(bytes: &[u8], folder: &Path, picks: &[Pick]) -> Result<Vec<Model>, ModelError> {
    let scene = parse(bytes)?;
    let mut reader = Reader {
        folder,
        images: HashMap::new(),
        primitives: Vec::new(),
        warnings: Vec::new(),
    };
    let mut models = Vec::new();
    for pick in picks {
        let wanted = pick.paths();
        let mut found = HashSet::new();
        reader.children(&scene.root_node, "", &wanted, &mut found);
        if let Some(path) = missing(&wanted, &found) {
            return Err(ModelError::NoObject(path));
        }
        let mut model = fit(std::mem::take(&mut reader.primitives))?;
        // A texture that can't be used is noted by the first model that has it.
        model.warnings = std::mem::take(&mut reader.warnings);
        models.push(model);
    }
    Ok(models)
}

/// The objects in an FBX file: its visible nodes with meshes, and the
/// nodes they're in.
pub(super) fn list(bytes: &[u8]) -> Result<Vec<ModelObject>, ModelError> {
    let scene = parse(bytes)?;
    fn children(node: &ufbx::Node, path: &str, depth: usize) -> Vec<Listed> {
        let mut siblings = Siblings::default();
        node.children
            .iter()
            .map(|child| {
                let name = node_name(child);
                let path = siblings.path(path, &name);
                let (triangles, materials) = match &child.mesh {
                    Some(mesh) if child.visible => {
                        (mesh.num_triangles, material_names(child, mesh))
                    }
                    _ => (0, Vec::new()),
                };
                let children = children(child, &path, depth + 1);
                Listed {
                    object: ModelObject {
                        path,
                        name,
                        depth,
                        triangles,
                        materials,
                    },
                    children,
                }
            })
            .collect()
    }
    Ok(objects::flatten(children(&scene.root_node, "", 0)))
}

/// A node's name, or "Object" if it has none.
fn node_name(node: &ufbx::Node) -> String {
    match node.element.name.trim() {
        "" => "Object".to_owned(),
        name => name.to_owned(),
    }
}

/// The names of the materials `mesh` uses, as placed by `node`.
fn material_names(node: &ufbx::Node, mesh: &ufbx::Mesh) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    for part in mesh
        .material_parts
        .iter()
        .filter(|part| part.num_triangles > 0)
    {
        let name = node
            .materials
            .get(part.index as usize)
            .or_else(|| mesh.materials.get(part.index as usize))
            .map_or("Default", |material| material.element.name.as_ref())
            .to_owned();
        if !names.contains(&name) {
            names.push(name);
        }
    }
    names
}

/// The texture files an FBX file in `folder` uses that are there, as paths
/// inside `folder`.
pub(super) fn files_used(bytes: &[u8], folder: &Path) -> Result<Vec<PathBuf>, ModelError> {
    let scene = parse(bytes)?;
    let mut files: Vec<PathBuf> = scene
        .textures
        .iter()
        .filter(|texture| texture.content.is_empty())
        .filter_map(|texture| texture_path(texture, folder))
        .collect();
    files.sort();
    files.dedup();
    Ok(files)
}

fn parse(bytes: &[u8]) -> Result<ufbx::SceneRoot, ModelError> {
    let opts = ufbx::LoadOpts {
        // Only the bytes given, and only as FBX (not OBJ).
        load_external_files: false,
        file_format: ufbx::FileFormat::Fbx,
        ignore_animation: true,
        generate_missing_normals: true,
        // Blender saves its Principled BSDF's metalness and roughness in
        // its own properties.
        use_blender_pbr_material: true,
        node_depth_limit: MAX_NODE_DEPTH as u32,
        // glTF's space: +X right, +Y up, +Z front, in meters.
        target_axes: CoordinateAxes {
            right: CoordinateAxis::PositiveX,
            up: CoordinateAxis::PositiveY,
            front: CoordinateAxis::PositiveZ,
        },
        target_unit_meters: 1.0,
        ..Default::default()
    };
    ufbx::load_memory(bytes, opts).map_err(|err| {
        let mut message = err.description.to_string();
        let info = err.info();
        if !info.is_empty() {
            message.push_str(&format!(" ({info})"));
        }
        ModelError::Fbx(message)
    })
}

/// Collects each mesh's triangles in model space.
struct Reader<'a> {
    folder: &'a Path,
    /// Textures decoded so far, by element ID, so shared ones decode once.
    /// `None` means it couldn't be, and was noted.
    images: HashMap<u32, Option<Arc<Image>>>,
    primitives: Vec<Primitive>,
    warnings: Vec<String>,
}

impl Reader<'_> {
    /// Collects the wanted meshes inside `node` (whose path is `path`),
    /// adding their paths to `found`. Every one if `wanted` is `None`.
    fn children(
        &mut self,
        node: &ufbx::Node,
        path: &str,
        wanted: &Option<HashSet<String>>,
        found: &mut HashSet<String>,
    ) {
        let mut siblings = Siblings::default();
        for child in &node.children {
            let child_path = siblings.path(path, &node_name(child));
            // Hidden in the program it was made in, so hidden here too.
            if let Some(mesh) = &child.mesh
                && child.visible
                && want(wanted, found, &child_path)
            {
                self.mesh(child, mesh);
            }
            self.children(child, &child_path, wanted, found);
        }
    }

    /// One primitive per material the mesh uses, placed by `node`.
    fn mesh(&mut self, node: &ufbx::Node, mesh: &ufbx::Mesh) {
        let transform = mat4(&node.geometry_to_world);
        // Normals are turned by the inverse transpose, so they stay
        // perpendicular to stretched surfaces.
        let normal_matrix = DMat3::from_mat4(transform).inverse().transpose();
        // A mirrored node turns triangles inside out: swapping two corners
        // turns them back.
        let mirrored = transform.determinant() < 0.0;
        // Room for the triangles of the biggest face (polygon).
        let mut corners = vec![0u32; mesh.max_face_triangles * 3];

        for part in &mesh.material_parts {
            if part.num_triangles == 0 {
                continue;
            }
            // An FBX material belongs to the node, so each use of a mesh
            // can have its own; the mesh's are the first node's.
            let material = node
                .materials
                .get(part.index as usize)
                .or_else(|| mesh.materials.get(part.index as usize));
            let mut material = match material {
                Some(material) => self.material(material),
                None => ModelMaterial::default(),
            };
            let want_uvs = material.has_textures() && mesh.vertex_uv.exists;

            let mut positions = Vec::new();
            let mut normals = Vec::new();
            let mut uvs = Vec::new();
            let mut indices = Vec::new();
            // Corners with the same position, normal and UV become one
            // vertex. The bit patterns compare exactly.
            let mut vertices: HashMap<[u32; 8], u32> = HashMap::new();
            for &face in &part.face_indices {
                let Some(&face) = mesh.faces.get(face as usize) else {
                    continue;
                };
                let triangles = ufbx::triangulate_face(&mut corners, mesh, face) as usize;
                for &index in &corners[..triangles * 3] {
                    let index = index as usize;
                    let position = transform
                        .transform_point3(dvec3(mesh.vertex_position[index]))
                        .as_vec3();
                    let normal = if mesh.vertex_normal.exists {
                        (normal_matrix * dvec3(mesh.vertex_normal[index]))
                            .normalize_or_zero()
                            .as_vec3()
                    } else {
                        Vec3::ZERO
                    };
                    // FBX's V runs up from the bottom of the texture.
                    let uv = if want_uvs {
                        let uv = mesh.vertex_uv[index];
                        [uv.x as f32, 1.0 - uv.y as f32]
                    } else {
                        [0.0; 2]
                    };
                    let mut key = [0; 8];
                    for (slot, value) in key.iter_mut().zip(
                        position
                            .to_array()
                            .into_iter()
                            .chain(normal.to_array())
                            .chain(uv),
                    ) {
                        *slot = value.to_bits();
                    }
                    let next = positions.len() as u32;
                    let vertex = *vertices.entry(key).or_insert_with(|| {
                        positions.push(position.to_array());
                        normals.push(normal.to_array());
                        if want_uvs {
                            uvs.push(uv);
                        }
                        next
                    });
                    indices.push(vertex);
                }
            }
            if mirrored {
                for triangle in indices.as_chunks_mut::<3>().0 {
                    triangle.swap(1, 2);
                }
            }
            if !mesh.vertex_normal.exists {
                normals = smooth_normals(&positions, &indices);
            }
            // A texture needs coordinates to be placed by.
            if uvs.is_empty() {
                material.remove_textures();
            }
            self.primitives.push(Primitive {
                key: new_key(),
                positions,
                normals,
                uvs,
                tangents: Vec::new(),
                indices,
                material,
            });
        }
    }

    /// An FBX material as metallic-roughness. `ufbx` works out PBR values
    /// for every kind of FBX material, including the old Lambert and Phong.
    /// A value with a map is the map's alone: FBX programs use a map
    /// instead of the value, not multiplied by it.
    fn material(&mut self, material: &ufbx::Material) -> ModelMaterial {
        let pbr = &material.pbr;
        let fbx = &material.fbx;
        // Lambert and Phong, the FBX format's own materials, rather than a
        // PBR one from a renderer's plug-in.
        let classic = matches!(
            material.shader_type,
            ShaderType::Unknown
                | ShaderType::FbxLambert
                | ShaderType::FbxPhong
                | ShaderType::BlenderPhong
        );
        let base = color(&pbr.base_color, Vec3::ONE) * number(&pbr.base_factor, 1.0);
        let base_color_texture = self.map_image(&pbr.base_color);

        let normal_texture = match self
            .map_image(&pbr.normal_map)
            .or_else(|| self.map_image(&fbx.normal_map))
        {
            Some(image) => Some(image),
            // Many exporters put normal maps in the bump slot; a real bump
            // (height) map is grey, and can't be used.
            None => self
                .map_image(&fbx.bump)
                .filter(|image| looks_like_normal_map(image)),
        };

        let metalness_texture = self
            .map_image(&pbr.metalness)
            .map(|image| ChannelTexture::new(image, Channel::Red));
        // Phong's shininess map, and maps the program says are
        // glossiness, are smooth where they're white.
        let roughness_texture = match self.map_image(&pbr.glossiness) {
            Some(image) => Some((image, true)),
            None => self.map_image(&pbr.roughness).map(|image| (image, classic)),
        }
        .map(|(image, invert)| ChannelTexture {
            invert,
            ..ChannelTexture::new(image, Channel::Red)
        });
        let occlusion_texture = self
            .map_image(&pbr.ambient_occlusion)
            .map(|image| ChannelTexture::new(image, Channel::Red));

        let emissive_texture = self.map_image(&pbr.emission_color);
        let mut emissive =
            color(&pbr.emission_color, Vec3::ZERO) * number(&pbr.emission_factor, 1.0);
        // An emission map with no emission color set glows in its own colors.
        if emissive_texture.is_some() && emissive.max_element() <= 0.0 {
            emissive = Vec3::ONE;
        }

        // Opacity is the base texture's alpha, or a map of its own (white
        // is solid). Classic materials keep it in "transparent color".
        let opacity_image = self
            .map_image(&pbr.opacity)
            .or_else(|| self.map_image(&fbx.transparency_color))
            .or_else(|| self.map_image(&fbx.transparency_factor));
        let same_as_base = |image: &Arc<Image>| {
            base_color_texture
                .as_ref()
                .is_some_and(|base| Arc::ptr_eq(base, image))
        };
        let opacity_texture = opacity_image
            .as_ref()
            .filter(|image| !same_as_base(image))
            .map(|image| {
                let channel = if image
                    .rgba
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .any(|pixel| pixel[3] < 255)
                {
                    Channel::Alpha
                } else {
                    Channel::Red
                };
                ChannelTexture::new(image.clone(), channel)
            });
        let opacity = number(&pbr.opacity, 1.0);
        // FBX has no alpha mode. A see-through material blends; an opacity
        // map cuts out (leaves, fences).
        let alpha_mode = if opacity < 1.0 {
            AlphaMode::Blend
        } else if opacity_image.is_some() {
            AlphaMode::Mask { cutoff: 0.5 }
        } else {
            AlphaMode::Opaque
        };

        // A classic material's specular map says how strongly it shines.
        let specular_texture = if classic {
            self.map_image(&pbr.specular_color)
                .or_else(|| self.map_image(&fbx.specular_color))
        } else {
            None
        };

        ModelMaterial {
            name: material.element.name.to_string(),
            base_color: [base.x, base.y, base.z, opacity].map(unit),
            metalness: match metalness_texture {
                Some(_) => 1.0,
                None => unit(number(&pbr.metalness, 0.0)),
            },
            roughness: match roughness_texture {
                Some(_) => 1.0,
                None => unit(number(&pbr.roughness, 1.0)),
            },
            emissive: emissive.to_array().map(unit),
            alpha_mode,
            base_color_texture,
            normal_texture,
            normal_scale: 1.0,
            metalness_texture,
            roughness_texture,
            occlusion_texture,
            occlusion_strength: 1.0,
            emissive_texture,
            opacity_texture,
            specular_texture,
        }
    }

    /// The image of a material value's map, if it has one that can be used.
    fn map_image(&mut self, map: &ufbx::MaterialMap) -> Option<Arc<Image>> {
        map.texture
            .as_ref()
            .filter(|_| map.texture_enabled)
            .and_then(|texture| self.image(texture))
    }

    /// The texture decoded, from the FBX file if it's embedded or from its
    /// file in the model's folder, or `None` (noted once) if it can't be.
    fn image(&mut self, texture: &ufbx::Texture) -> Option<Arc<Image>> {
        // A layered texture is made of file textures; the first is the base.
        let texture = texture
            .file_textures
            .first()
            .map_or(texture, |file| &**file);
        let id = texture.element.element_id;
        if let Some(image) = self.images.get(&id) {
            return image.clone();
        }
        let name = texture_name(texture);
        let image = if texture.content.is_empty() {
            match texture_path(texture, self.folder) {
                Some(path) => match std::fs::read(self.folder.join(&path)) {
                    Ok(bytes) => decode(&bytes, &name),
                    Err(err) => Err(err.to_string()),
                },
                None => Err("it isn't there; put it next to the model".to_owned()),
            }
        } else {
            decode(&texture.content, &name)
        };
        let image = match image {
            Ok(image) => Some(Arc::new(image)),
            Err(reason) => {
                self.warnings
                    .push(format!("is drawn without its texture `{name}`: {reason}"));
                None
            }
        };
        self.images.insert(id, image.clone());
        image
    }
}

/// Where `texture`'s file is, as a path inside `folder`, trying in turn:
/// the path the file gives relative to the model, the file's name next to
/// the model, and its name in a `textures` folder next to the model. `None`
/// if it isn't in any of them.
fn texture_path(texture: &ufbx::Texture, folder: &Path) -> Option<PathBuf> {
    let name = file_name(texture);
    let candidates = [
        crate::safe_path(&texture.relative_filename),
        crate::safe_path(name),
        (!name.is_empty())
            .then(|| crate::safe_path(&format!("textures/{name}")))
            .flatten(),
    ];
    candidates
        .into_iter()
        .flatten()
        .find(|path| folder.join(path).is_file())
}

/// The last part of the texture's path, with `/` or `\` between folders.
fn file_name(texture: &ufbx::Texture) -> &str {
    [
        &texture.relative_filename,
        &texture.filename,
        &texture.absolute_filename,
    ]
    .into_iter()
    .map(|path| path.rsplit(['/', '\\']).next().unwrap_or_default())
    .find(|name| !name.is_empty())
    .unwrap_or_default()
}

/// How the texture is named in messages.
fn texture_name(texture: &ufbx::Texture) -> String {
    match file_name(texture) {
        "" if !texture.content.is_empty() => "embedded texture".to_owned(),
        "" => texture.element.name.to_string(),
        name => name.to_owned(),
    }
}

/// Decodes a PNG, JPEG, TGA or BMP texture. TGA files have no signature to
/// recognize, so the name's extension decides when the content can't.
fn decode(bytes: &[u8], name: &str) -> Result<Image, String> {
    let format = image::guess_format(bytes)
        .or_else(|_| image::ImageFormat::from_path(name))
        .map_err(|_| "only PNG, JPEG, TGA and BMP textures can be used".to_owned())?;
    let rgba = image::load_from_memory_with_format(bytes, format)
        .map_err(|err| match err {
            image::ImageError::Unsupported(_) => {
                "only PNG, JPEG, TGA and BMP textures can be used".to_owned()
            }
            err => err.to_string(),
        })?
        .into_rgba8();
    if rgba.width() == 0 || rgba.height() == 0 {
        return Err("it has no pixels".to_owned());
    }
    Ok(Image {
        key: new_key(),
        width: rgba.width(),
        height: rgba.height(),
        rgba: rgba.into_raw(),
    })
}

/// True if `image` looks like a tangent-space normal map rather than a
/// bump (height) map: mostly blue (surfaces facing out), with red and
/// green around the middle. A bump map is grey.
fn looks_like_normal_map(image: &Image) -> bool {
    let pixels = image.rgba.len() / 4;
    // Every pixel of a small image, or about 4,096 spread over a big one.
    let step = (pixels / 4096).max(1);
    let mut sums = [0u64; 3];
    let mut count = 0u64;
    for pixel in image.rgba.as_chunks::<4>().0.iter().step_by(step) {
        for (sum, &value) in sums.iter_mut().zip(pixel) {
            *sum += u64::from(value);
        }
        count += 1;
    }
    if count == 0 {
        return false;
    }
    let [red, green, blue] = sums.map(|sum| sum as f32 / count as f32);
    blue > 180.0 && (red - 128.0).abs() < 40.0 && (green - 128.0).abs() < 40.0
}

/// A material value that's one number, or `default` if the file has none.
fn number(map: &ufbx::MaterialMap, default: f32) -> f32 {
    if map.has_value {
        map.value_vec4.x as f32
    } else {
        default
    }
}

/// A material value that's a color, or `default` if the file has none.
fn color(map: &ufbx::MaterialMap, default: Vec3) -> Vec3 {
    if map.has_value {
        let v = map.value_vec4;
        DVec3::new(v.x, v.y, v.z).as_vec3()
    } else {
        default
    }
}

/// `value` clamped to 0–1, with anything not a number as 0.
fn unit(value: f32) -> f32 {
    if value.is_finite() {
        value.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

fn dvec3(v: ufbx::Vec3) -> DVec3 {
    DVec3::new(v.x, v.y, v.z)
}

/// `ufbx`'s 3×4 matrix (columns: X, Y and Z axes, then the translation).
fn mat4(m: &ufbx::Matrix) -> DMat4 {
    DMat4::from_cols_array(&[
        m.m00, m.m10, m.m20, 0.0, //
        m.m01, m.m11, m.m21, 0.0, //
        m.m02, m.m12, m.m22, 0.0, //
        m.m03, m.m13, m.m23, 1.0,
    ])
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::model::{ModelFormat, Pick, load_model};

    /// An ASCII FBX file of a square pyramid 4 wide and 3 tall, with one
    /// red Phong material. `z_up` saves it in centimeters with Z up (as
    /// 3ds Max does), otherwise in meters with Y up. With a `texture` path,
    /// the material's diffuse color uses it.
    pub(crate) fn pyramid_fbx(z_up: bool, texture: Option<&str>) -> String {
        let maps: Vec<(&str, &str)> = texture
            .map(|path| ("DiffuseColor", path))
            .into_iter()
            .collect();
        pyramid_fbx_with_maps(z_up, &maps)
    }

    /// Like [`pyramid_fbx`], with each `(property, path)` of `maps` a
    /// texture linked to that property of the material ("DiffuseColor",
    /// "NormalMap", "SpecularColor"…).
    pub(crate) fn pyramid_fbx_with_maps(z_up: bool, maps: &[(&str, &str)]) -> String {
        let (up_axis, front_axis, front_sign, unit, vertices) = if z_up {
            (
                2,
                1,
                -1,
                1,
                "-200,200,0, 200,200,0, 200,-200,0, -200,-200,0, 0,0,300",
            )
        } else {
            (1, 2, 1, 100, "-2,0,-2, 2,0,-2, 2,0,2, -2,0,2, 0,3,0")
        };
        let mut texture_object = String::new();
        let mut texture_connection = String::new();
        for (i, (property, path)) in maps.iter().enumerate() {
            let id = 4000 + i;
            texture_object.push_str(&format!(
                "\tTexture: {id}, \"Texture::Map{i}\", \"\" {{\n\
                 \t\tType: \"TextureVideoClip\"\n\
                 \t\tVersion: 202\n\
                 \t\tTextureName: \"Texture::Map{i}\"\n\
                 \t\tFileName: \"C:/Users/artist/Desktop/{path}\"\n\
                 \t\tRelativeFilename: \"{path}\"\n\
                 \t}}\n"
            ));
            texture_connection.push_str(&format!("\tC: \"OP\",{id},3000, \"{property}\"\n"));
        }
        format!(
            r#"; FBX 7.4.0 project file
FBXHeaderExtension:  {{
	FBXHeaderVersion: 1003
	FBXVersion: 7400
}}
GlobalSettings:  {{
	Version: 1000
	Properties70:  {{
		P: "UpAxis", "int", "Integer", "",{up_axis}
		P: "UpAxisSign", "int", "Integer", "",1
		P: "FrontAxis", "int", "Integer", "",{front_axis}
		P: "FrontAxisSign", "int", "Integer", "",{front_sign}
		P: "CoordAxis", "int", "Integer", "",0
		P: "CoordAxisSign", "int", "Integer", "",1
		P: "UnitScaleFactor", "double", "Number", "",{unit}
	}}
}}
Objects:  {{
	Geometry: 1000, "Geometry::Pyramid", "Mesh" {{
		Vertices: *15 {{
			a: {vertices}
		}}
		PolygonVertexIndex: *16 {{
			a: 0,1,2,-4,0,4,-2,1,4,-3,2,4,-4,3,4,-1
		}}
		GeometryVersion: 124
		LayerElementUV: 0 {{
			Version: 101
			Name: "UVMap"
			MappingInformationType: "ByPolygonVertex"
			ReferenceInformationType: "IndexToDirect"
			UV: *10 {{
				a: 0,0,1,0,1,1,0,1,0.5,0.5
			}}
			UVIndex: *16 {{
				a: 0,1,2,3,0,4,1,1,4,2,2,4,3,3,4,0
			}}
		}}
		LayerElementMaterial: 0 {{
			Version: 101
			Name: ""
			MappingInformationType: "AllSame"
			ReferenceInformationType: "IndexToDirect"
			Materials: *1 {{
				a: 0
			}}
		}}
		Layer: 0 {{
			Version: 100
			LayerElement:  {{
				Type: "LayerElementUV"
				TypedIndex: 0
			}}
			LayerElement:  {{
				Type: "LayerElementMaterial"
				TypedIndex: 0
			}}
		}}
	}}
	Model: 2000, "Model::Pyramid", "Mesh" {{
		Version: 232
		Properties70:  {{
		}}
	}}
	Material: 3000, "Material::Red", "" {{
		Version: 102
		ShadingModel: "phong"
		Properties70:  {{
			P: "DiffuseColor", "Color", "", "A",1,0,0
		}}
	}}
{texture_object}}}
Connections:  {{
	C: "OO",1000,2000
	C: "OO",2000,0
	C: "OO",3000,2000
{texture_connection}}}
"#
        )
    }

    /// An ASCII FBX file, in meters with Y up, of two copies of a 2×2×2
    /// cube: `Street/Bench` (inside an empty `Street` node, 10 m along X)
    /// and `Lamp` (at the origin, with a second, empty `Lamp` after it).
    pub(crate) fn two_cubes_fbx() -> String {
        r#"; FBX 7.4.0 project file
FBXHeaderExtension:  {
	FBXHeaderVersion: 1003
	FBXVersion: 7400
}
GlobalSettings:  {
	Version: 1000
	Properties70:  {
		P: "UpAxis", "int", "Integer", "",1
		P: "UpAxisSign", "int", "Integer", "",1
		P: "FrontAxis", "int", "Integer", "",2
		P: "FrontAxisSign", "int", "Integer", "",1
		P: "CoordAxis", "int", "Integer", "",0
		P: "CoordAxisSign", "int", "Integer", "",1
		P: "UnitScaleFactor", "double", "Number", "",100
	}
}
Objects:  {
	Geometry: 1000, "Geometry::Cube", "Mesh" {
		Vertices: *24 {
			a: -1,-1,-1, 1,-1,-1, 1,1,-1, -1,1,-1, -1,-1,1, 1,-1,1, 1,1,1, -1,1,1
		}
		PolygonVertexIndex: *24 {
			a: 0,3,2,-2,4,5,6,-8,0,1,5,-5,2,3,7,-7,1,2,6,-6,0,4,7,-4
		}
		GeometryVersion: 124
	}
	Model: 2000, "Model::Street", "Null" {
		Version: 232
	}
	Model: 2001, "Model::Bench", "Mesh" {
		Version: 232
		Properties70:  {
			P: "Lcl Translation", "Lcl Translation", "", "A",10,0,0
		}
	}
	Model: 2002, "Model::Lamp", "Mesh" {
		Version: 232
	}
	Model: 2003, "Model::Lamp", "Null" {
		Version: 232
	}
	Material: 3000, "Material::Stone", "" {
		Version: 102
		ShadingModel: "phong"
	}
}
Connections:  {
	C: "OO",2000,0
	C: "OO",2001,2000
	C: "OO",2002,0
	C: "OO",2003,0
	C: "OO",1000,2001
	C: "OO",1000,2002
	C: "OO",3000,2001
	C: "OO",3000,2002
}
"#
        .to_owned()
    }

    /// A 2×2 checkered PNG.
    pub(crate) fn checker_png() -> Vec<u8> {
        let image = image::RgbaImage::from_fn(2, 2, |x, y| match (x + y) % 2 {
            0 => image::Rgba([255, 255, 255, 255]),
            _ => image::Rgba([0, 0, 0, 255]),
        });
        let mut png = std::io::Cursor::new(Vec::new());
        image.write_to(&mut png, image::ImageFormat::Png).unwrap();
        png.into_inner()
    }

    fn temp_folder(name: &str) -> PathBuf {
        let folder = std::env::temp_dir().join(format!("axcenity-fbx-{name}"));
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(&folder).unwrap();
        folder
    }

    fn load_fbx(text: &str, folder: &Path) -> Result<Model, ModelError> {
        load_model(text.as_bytes(), folder, ModelFormat::Fbx)
    }

    fn assert_size(model: &Model, expected: [f32; 3]) {
        for (size, expected) in model.size.iter().zip(expected) {
            assert!((size - expected).abs() < 1e-4, "{:?}", model.size);
        }
    }

    #[test]
    fn an_fbx_pyramid_loads_fitted_into_the_unit_cube() {
        let folder = temp_folder("pyramid");
        let model = load_fbx(&pyramid_fbx(false, None), &folder).unwrap();
        assert_size(&model, [4.0, 3.0, 4.0]);
        assert_eq!(model.primitives.len(), 1);
        let primitive = &model.primitives[0];
        // The square base is two triangles, plus four sides.
        assert_eq!(primitive.indices.len(), 6 * 3);
        assert_eq!(primitive.normals.len(), primitive.positions.len());
        for corner in &primitive.positions {
            assert!(corner.iter().all(|c| c.abs() <= 0.5 + 1e-6), "{corner:?}");
        }
        assert_eq!(model.hull_points.len(), 5);
        let material = &primitive.material;
        assert_eq!(material.base_color, [1.0, 0.0, 0.0, 1.0]);
        assert_eq!(material.alpha_mode, AlphaMode::Opaque);
        assert!(material.base_color_texture.is_none());
        assert!(primitive.uvs.is_empty());
        assert!(model.warnings.is_empty());
    }

    #[test]
    fn other_axes_and_units_are_converted_to_studs_with_y_up() {
        let folder = temp_folder("z-up");
        let model = load_fbx(&pyramid_fbx(true, None), &folder).unwrap();
        assert_size(&model, [4.0, 3.0, 4.0]);
    }

    #[test]
    fn textures_are_found_next_to_the_model() {
        let folder = temp_folder("texture");
        std::fs::create_dir_all(folder.join("textures")).unwrap();
        std::fs::write(folder.join("textures/checker.png"), checker_png()).unwrap();
        // By its relative path, and by its name in `textures` when the
        // relative path leads outside the folder.
        for path in ["textures/checker.png", "../../maps/checker.png"] {
            let model = load_fbx(&pyramid_fbx(false, Some(path)), &folder).unwrap();
            assert!(model.warnings.is_empty(), "{:?}", model.warnings);
            let primitive = &model.primitives[0];
            let texture = primitive.material.base_color_texture.as_ref().unwrap();
            assert_eq!((texture.width, texture.height), (2, 2));
            assert_eq!(primitive.uvs.len(), primitive.positions.len());
            // V is flipped: FBX's (0, 0) is the bottom left.
            assert!(primitive.uvs.contains(&[0.0, 1.0]), "{:?}", primitive.uvs);
        }
    }

    /// A 2×2 PNG all of one color.
    fn solid_png(rgba: [u8; 4]) -> Vec<u8> {
        let image = image::RgbaImage::from_pixel(2, 2, image::Rgba(rgba));
        let mut png = std::io::Cursor::new(Vec::new());
        image.write_to(&mut png, image::ImageFormat::Png).unwrap();
        png.into_inner()
    }

    #[test]
    fn a_phong_materials_maps_become_pbr_maps() {
        let folder = temp_folder("phong-maps");
        let files = [
            ("color.png", [200, 100, 50, 255]),
            ("normal.png", [128, 128, 255, 255]),
            ("gloss.png", [255, 255, 255, 255]),
            ("spec.png", [64, 64, 64, 255]),
            ("glow.png", [255, 0, 0, 255]),
            ("cutout.png", [255, 255, 255, 0]),
        ];
        for (name, rgba) in files {
            std::fs::write(folder.join(name), solid_png(rgba)).unwrap();
        }
        let fbx = pyramid_fbx_with_maps(
            false,
            &[
                ("DiffuseColor", "color.png"),
                ("NormalMap", "normal.png"),
                ("ShininessExponent", "gloss.png"),
                ("SpecularColor", "spec.png"),
                ("EmissiveColor", "glow.png"),
                ("TransparentColor", "cutout.png"),
            ],
        );
        let model = load_fbx(&fbx, &folder).unwrap();
        assert!(model.warnings.is_empty(), "{:?}", model.warnings);
        let primitive = &model.primitives[0];
        let material = &primitive.material;
        assert_eq!(material.name, "Red");
        assert!(material.base_color_texture.is_some());
        assert!(material.normal_texture.is_some());
        // Every corner gets a tangent for the normal map.
        assert_eq!(primitive.tangents.len(), primitive.positions.len());
        // Shininess is glossiness: inverted, and the map alone sets it.
        let roughness = material.roughness_texture.as_ref().unwrap();
        assert!(roughness.invert);
        assert_eq!(material.roughness, 1.0);
        assert!(material.specular_texture.is_some());
        // An emission map with no emission color glows in its own colors.
        assert!(material.emissive_texture.is_some());
        assert_eq!(material.emissive, [1.0; 3]);
        // The cut-out map has alpha, so that's the channel used.
        let opacity = material.opacity_texture.as_ref().unwrap();
        assert_eq!(opacity.channel, Channel::Alpha);
        assert_eq!(material.alpha_mode, AlphaMode::Mask { cutoff: 0.5 });
    }

    #[test]
    fn a_bump_map_is_used_only_if_it_is_really_a_normal_map() {
        let folder = temp_folder("bump");
        std::fs::write(folder.join("normal.png"), solid_png([120, 135, 250, 255])).unwrap();
        std::fs::write(folder.join("height.png"), solid_png([90, 90, 90, 255])).unwrap();
        let normal = load_fbx(
            &pyramid_fbx_with_maps(false, &[("Bump", "normal.png")]),
            &folder,
        );
        assert!(
            normal.unwrap().primitives[0]
                .material
                .normal_texture
                .is_some()
        );
        let height = load_fbx(
            &pyramid_fbx_with_maps(false, &[("Bump", "height.png")]),
            &folder,
        );
        let height = height.unwrap();
        assert!(height.primitives[0].material.normal_texture.is_none());
        // With no texture left, it needs no texture coordinates.
        assert!(height.primitives[0].uvs.is_empty());
    }

    #[test]
    fn a_missing_or_unreadable_texture_is_a_warning_not_an_error() {
        let folder = temp_folder("no-texture");
        std::fs::write(folder.join("paint.psd"), b"8BPS not really").unwrap();
        let missing = load_fbx(&pyramid_fbx(false, Some("gone.png")), &folder).unwrap();
        assert!(missing.primitives[0].material.base_color_texture.is_none());
        assert_eq!(missing.warnings.len(), 1);
        assert!(
            missing.warnings[0].contains("`gone.png`: it isn't there"),
            "{:?}",
            missing.warnings
        );
        let unreadable = load_fbx(&pyramid_fbx(false, Some("paint.psd")), &folder).unwrap();
        assert!(
            unreadable.warnings[0].contains("only PNG, JPEG, TGA and BMP"),
            "{:?}",
            unreadable.warnings
        );
    }

    #[test]
    fn the_objects_in_an_fbx_are_listed_as_a_tree() {
        let objects =
            crate::model::list_objects(two_cubes_fbx().as_bytes(), ModelFormat::Fbx).unwrap();
        let rows: Vec<(&str, usize, usize)> = objects
            .iter()
            .map(|object| (object.path.as_str(), object.depth, object.triangles))
            .collect();
        // The empty second Lamp has nothing to import, so it isn't listed.
        assert_eq!(
            rows,
            [("Street", 0, 0), ("Street/Bench", 1, 12), ("Lamp", 0, 12)]
        );
        assert_eq!(objects[1].materials, ["Stone"]);
    }

    #[test]
    fn picked_fbx_objects_load_on_their_own_and_keep_their_places() {
        let folder = temp_folder("picks");
        let picks = [
            Pick::Objects(vec!["Street/Bench".into()]),
            Pick::Objects(vec!["Lamp".into()]),
            Pick::Whole,
        ];
        let models = crate::model::load_models(
            two_cubes_fbx().as_bytes(),
            &folder,
            ModelFormat::Fbx,
            &picks,
        )
        .unwrap();
        let [bench, lamp, whole] = &models[..] else {
            panic!("one model per pick");
        };
        assert_size(bench, [2.0, 2.0, 2.0]);
        assert!((bench.center[0] - 10.0).abs() < 1e-4, "{:?}", bench.center);
        assert!(
            lamp.center.iter().all(|c| c.abs() < 1e-4),
            "{:?}",
            lamp.center
        );
        assert_size(whole, [12.0, 2.0, 2.0]);
        assert_eq!(whole.primitives.len(), 2);

        let missing = crate::model::load_models(
            two_cubes_fbx().as_bytes(),
            &folder,
            ModelFormat::Fbx,
            &[Pick::Objects(vec!["Street".into()])],
        );
        // A group has no mesh of its own.
        assert_eq!(
            missing.unwrap_err().to_string(),
            "it has no object `Street`"
        );
    }

    #[test]
    fn broken_fbx_files_give_errors_not_crashes() {
        let folder = temp_folder("broken");
        let error = |bytes: &[u8]| {
            load_model(bytes, &folder, ModelFormat::Fbx)
                .unwrap_err()
                .to_string()
        };
        assert!(error(b"").contains("isn't a valid FBX file"));
        assert!(error(b"Kaydara FBX Binary  \0\x1a\0").contains("isn't a valid FBX file"));
        // Cut off halfway.
        let text = pyramid_fbx(false, None);
        assert!(load_fbx(&text[..text.len() / 2], &folder).is_err());
        // Valid, but nothing to draw.
        let empty = "; FBX 7.4.0 project file\nFBXHeaderExtension:  {\n\tFBXVersion: 7400\n}\n";
        assert_eq!(error(empty.as_bytes()), "it has no triangles to draw");
    }
}
