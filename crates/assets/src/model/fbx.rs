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
use ufbx::{CoordinateAxes, CoordinateAxis};

use super::objects::{self, Listed, Siblings};
use super::{
    AlphaMode, Image, MAX_NODE_DEPTH, Model, ModelError, ModelMaterial, ModelObject, Pick,
    Primitive, fit, missing, new_key, smooth_normals, want,
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
                None => plain_material(),
            };
            let want_uvs = material.base_color_texture.is_some() && mesh.vertex_uv.exists;

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
                material.base_color_texture = None;
            }
            self.primitives.push(Primitive {
                key: new_key(),
                positions,
                normals,
                uvs,
                indices,
                material,
            });
        }
    }

    /// An FBX material as metallic-roughness. `ufbx` works out PBR values
    /// for every kind of FBX material, including the old Lambert and Phong.
    fn material(&mut self, material: &ufbx::Material) -> ModelMaterial {
        let pbr = &material.pbr;
        let base = color(&pbr.base_color, Vec3::ONE) * number(&pbr.base_factor, 1.0);
        let emissive = color(&pbr.emission_color, Vec3::ZERO) * number(&pbr.emission_factor, 1.0);
        let opacity = number(&pbr.opacity, 1.0);
        let texture = pbr
            .base_color
            .texture
            .as_ref()
            .filter(|_| pbr.base_color.texture_enabled)
            .and_then(|texture| self.image(texture));
        // FBX has no alpha mode. A see-through material blends; an opacity
        // map cuts out (leaves, fences) by the base texture's alpha.
        let alpha_mode = if opacity < 1.0 {
            AlphaMode::Blend
        } else if pbr.opacity.texture.is_some() && texture.is_some() {
            AlphaMode::Mask { cutoff: 0.5 }
        } else {
            AlphaMode::Opaque
        };
        ModelMaterial {
            base_color: [base.x, base.y, base.z, opacity].map(unit),
            metalness: unit(number(&pbr.metalness, 0.0)),
            roughness: unit(number(&pbr.roughness, 1.0)),
            emissive: emissive.to_array().map(unit),
            alpha_mode,
            base_color_texture: texture,
        }
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

/// The material of a mesh that has none: plain white plastic.
fn plain_material() -> ModelMaterial {
    ModelMaterial {
        base_color: [1.0; 4],
        metalness: 0.0,
        roughness: 1.0,
        emissive: [0.0; 3],
        alpha_mode: AlphaMode::Opaque,
        base_color_texture: None,
    }
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
        let (texture_object, texture_connection) = match texture {
            Some(path) => (
                format!(
                    "\tTexture: 4000, \"Texture::Checker\", \"\" {{\n\
                     \t\tType: \"TextureVideoClip\"\n\
                     \t\tVersion: 202\n\
                     \t\tTextureName: \"Texture::Checker\"\n\
                     \t\tFileName: \"C:/Users/artist/Desktop/{path}\"\n\
                     \t\tRelativeFilename: \"{path}\"\n\
                     \t}}\n"
                ),
                "\tC: \"OP\",4000,3000, \"DiffuseColor\"\n",
            ),
            None => (String::new(), ""),
        };
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
