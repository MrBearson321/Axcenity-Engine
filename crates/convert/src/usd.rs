//! USD (`.usd`, `.usda`, `.usdc`, `.usdz`) to glTF, with `openusd`.
//!
//! The stage is opened with its references and payloads loaded and its
//! variants as selected, then walked from the top: each `Mesh` prim
//! becomes a glTF node with a mesh, and the prims above it (`Xform`,
//! `Scope`, untyped) group nodes, named as in the file, so the Import
//! dialog shows the USD tree. Inactive, invisible, guide and proxy prims
//! are left out; instances are read through their proxies (each copy
//! becomes its own node).
//!
//! glTF is Y up, in meters: the stage's `upAxis` and `metersPerUnit` (USD's
//! fallbacks are Y and 0.01) are applied at the top. Polygons are
//! triangulated as fans, `leftHanded` meshes are turned round, and `st`
//! texture coordinates are flipped to glTF's top-left origin. Values that
//! only have time samples are read at the stage's start time.
//!
//! Materials are read from `UsdPreviewSurface` shaders (the portable one
//! every USD tool writes): diffuse color, metallic, roughness, normal,
//! occlusion, emissive and opacity, each a number or a `UsdUVTexture`
//! file. A mesh's `GeomSubset`s in the `materialBind` family give its
//! faces their own materials. A mesh with no material takes its
//! `displayColor`. Not converted: skeletons and animation (P7.6), lights,
//! cameras, points, curves and the implicit shapes (`Cube`, `Sphere`…).

use std::collections::{HashMap, HashSet};
use std::path::Path;

use glam::{DMat4, DVec3};
use openusd::{sdf, usd};
use openusd_schemas::geom::{StageMetadata, XformCache};
use openusd_schemas::shade;

use crate::glb::{self, Alpha, EmbeddedImage, Material, Node, Primitive, Scene};
use crate::textures::{self, Channel, Files};
use crate::{ConvertError, Converted};

/// Reads the USD file at `path` into a `.glb`.
pub(crate) fn convert(path: &Path) -> Result<Converted, ConvertError> {
    let text = path
        .to_str()
        .ok_or_else(|| ConvertError::Usd("its path isn't valid text".into()))?;
    let stage = usd::Stage::builder()
        .schema_registry(openusd_schemas::schema_registry())
        .open(text)
        .map_err(|err| ConvertError::Usd(err.to_string()))?;
    let mut reader = Reader::new(&stage, path);
    let root = stage
        .prim(sdf::Path::abs_root())
        .map_err(|err| ConvertError::Usd(err.to_string()))?;
    // The top nodes carry the change to glTF's units and axes.
    let top = reader.root;
    let roots = reader.children(&root, top, DMat4::IDENTITY);
    if reader.scene.meshes.is_empty() {
        // Often because the layer holding the meshes isn't there.
        return Err(match stage.composition_errors().first() {
            Some(error) => ConvertError::Usd(format!("a layer it uses couldn't be read: {error}")),
            None => ConvertError::NoMeshes,
        });
    }
    reader.scene.roots = roots;
    if !reader.skipped.is_empty() {
        let mut kinds: Vec<&str> = reader.skipped.iter().map(String::as_str).collect();
        kinds.sort_unstable();
        reader.warnings.push(format!(
            "left out what glTF import doesn't bring in: {}",
            kinds.join(", ")
        ));
    }
    for error in stage.composition_errors() {
        reader
            .warnings
            .push(format!("a layer it uses couldn't be read: {error}"));
    }
    Ok(Converted {
        glb: glb::write(&reader.scene),
        warnings: reader.warnings,
    })
}

struct Reader<'a> {
    stage: &'a usd::Stage,
    /// When values that only have time samples are read.
    time: usd::TimeCode,
    transforms: XformCache,
    files: Files,
    scene: Scene,
    /// Materials made, by their prim path (and the mesh's sides and color,
    /// which glTF keeps in the material).
    materials: HashMap<(String, bool, [u32; 3]), usize>,
    /// Images embedded, by what they were made from.
    images: HashMap<String, Option<usize>>,
    warnings: Vec<String>,
    warned: HashSet<String>,
    /// Prim types that were left out.
    skipped: HashSet<String>,
    /// From the stage's space to glTF's: its units and up axis.
    root: DMat4,
}

/// Prim types that hold no geometry for us, or nothing under them that's
/// drawn: their subtrees are skipped without a word.
const NOT_DRAWN: [&str; 9] = [
    "Material",
    "Shader",
    "NodeGraph",
    "GeomSubset",
    "Camera",
    "Skeleton",
    "SkelAnimation",
    "BlendShape",
    "RenderSettings",
];

impl<'a> Reader<'a> {
    fn new(stage: &'a usd::Stage, path: &Path) -> Self {
        let time = usd::TimeCode::new(stage.start_time_code());
        let meters = stage
            .meters_per_unit()
            .ok()
            .flatten()
            .filter(|meters| *meters > 0.0)
            // USD's fallback: centimeters.
            .unwrap_or(0.01);
        let z_up = stage
            .up_axis()
            .ok()
            .flatten()
            .is_some_and(|axis| axis.as_str().eq_ignore_ascii_case("z"));
        // Z up to Y up: (x, y, z) -> (x, z, -y).
        let axes = if z_up {
            DMat4::from_rotation_x(-std::f64::consts::FRAC_PI_2)
        } else {
            DMat4::IDENTITY
        };
        let root = axes * DMat4::from_scale(DVec3::splat(meters));
        Self {
            stage,
            time,
            transforms: XformCache::new(time),
            files: Files::new(path, &stage.layer_identifiers()),
            scene: Scene::default(),
            materials: HashMap::new(),
            images: HashMap::new(),
            warnings: Vec::new(),
            warned: HashSet::new(),
            skipped: HashSet::new(),
            root,
        }
    }

    fn warn_once(&mut self, key: String, warning: String) {
        if self.warned.insert(key) {
            self.warnings.push(warning);
        }
    }

    /// The nodes made for `prim`'s children. `world` is where `prim` is in
    /// glTF's space (column vectors), and `node_world` where its nearest
    /// node is, which the new nodes' matrices are relative to.
    fn children(&mut self, prim: &usd::Prim, world: DMat4, node_world: DMat4) -> Vec<usize> {
        let Ok(children) = prim.children() else {
            return Vec::new();
        };
        let mut nodes = Vec::new();
        for child in children {
            if let Some(node) = self.prim(&child, world, node_world) {
                nodes.push(node);
            }
        }
        nodes
    }

    /// The node made for `prim` (with what's under it), if it has anything
    /// that's drawn.
    fn prim(&mut self, prim: &usd::Prim, parent: DMat4, parent_node: DMat4) -> Option<usize> {
        if !prim.is_active().unwrap_or(false)
            || prim.is_abstract().unwrap_or(true)
            || !prim.is_defined().unwrap_or(false)
        {
            return None;
        }
        let kind = prim
            .type_name()
            .ok()
            .flatten()
            .map(|name| name.as_str().to_owned())
            .unwrap_or_default();
        if NOT_DRAWN.contains(&kind.as_str()) || kind.ends_with("Light") {
            return None;
        }
        if token(&prim.attribute("visibility"), None).as_deref() == Some("invisible") {
            return None;
        }
        if matches!(
            token(&prim.attribute("purpose"), None).as_deref(),
            Some("guide" | "proxy")
        ) {
            return None;
        }
        let world = match self.transforms.local_transformation(prim) {
            Ok((local, resets)) => {
                let local = DMat4::from_cols_array(&local.0);
                if resets { self.root * local } else { parent * local }
            }
            Err(_) => parent,
        };
        let mesh = match kind.as_str() {
            "Mesh" => self.mesh(prim),
            "" | "Xform" | "Scope" | "SkelRoot" => None,
            other => {
                self.skipped.insert(other.to_owned());
                None
            }
        };
        let children = self.children(prim, world, world);
        if mesh.is_none() && children.is_empty() {
            return None;
        }
        let relative = parent_node.inverse() * world;
        let name = prim.path().name().unwrap_or("Object").to_owned();
        self.scene.nodes.push(Node {
            name,
            matrix: relative.as_mat4().to_cols_array(),
            mesh,
            children,
        });
        Some(self.scene.nodes.len() - 1)
    }

    /// The glTF mesh of a `Mesh` prim, if it has triangles.
    fn mesh(&mut self, prim: &usd::Prim) -> Option<usize> {
        let name = prim.path().as_str().to_owned();
        let points = vec3s(&self.value(&prim.attribute("points"))?)?;
        let counts = ints(&self.value(&prim.attribute("faceVertexCounts"))?)?;
        let corners = ints(&self.value(&prim.attribute("faceVertexIndices"))?)?;
        let total: i64 = counts.iter().map(|&count| i64::from(count.max(0))).sum();
        if total != corners.len() as i64
            || corners
                .iter()
                .any(|&corner| corner < 0 || corner as usize >= points.len())
        {
            self.warn_once(
                name.clone(),
                format!("its mesh `{name}` lists corners that aren't there, so it's left out"),
            );
            return None;
        }
        let left_handed = token(&prim.attribute("orientation"), None).as_deref() == Some("leftHanded");
        let double_sided = matches!(
            self.value(&prim.attribute("doubleSided")),
            Some(sdf::Value::Bool(true))
        );
        let normals = self.normals(prim);
        let uvs = self.uvs(prim);

        // Which faces each material subset owns.
        let faces = counts.len();
        let mut face_material = vec![None::<usize>; faces];
        let display = self.display_color(prim);
        let own = self.bound_material(prim);
        let own = self.material(own, display, double_sided);
        for subset in self.material_subsets(prim) {
            let material = self.bound_material(&subset.prim);
            let material = self.material(material, display, double_sided);
            for face in subset.faces {
                if let Some(slot) = face_material.get_mut(face) {
                    *slot = material.or(own);
                }
            }
        }

        // One primitive per material, each with only its own corners.
        let mut groups: Vec<(Option<usize>, Builder)> = Vec::new();
        let mut first_corner = 0usize;
        for (face, &count) in counts.iter().enumerate() {
            let count = count.max(0) as usize;
            let material = face_material[face].or(own);
            let group = match groups.iter().position(|(m, _)| *m == material) {
                Some(group) => group,
                None => {
                    groups.push((material, Builder::default()));
                    groups.len() - 1
                }
            };
            let builder = &mut groups[group].1;
            let vertex = |builder: &mut Builder, corner: usize| {
                let point = corners[corner] as usize;
                builder.vertex(
                    &points,
                    point,
                    normals.as_ref().map(|n| n.pick(point, corner, face)),
                    uvs.as_ref().map(|uv| uv.pick(point, corner, face)),
                    normals.as_ref(),
                    uvs.as_ref(),
                )
            };
            if count >= 3 {
                let a = vertex(builder, first_corner);
                for i in 1..count - 1 {
                    let b = vertex(builder, first_corner + i);
                    let c = vertex(builder, first_corner + i + 1);
                    if left_handed {
                        builder.indices.extend([a, c, b]);
                    } else {
                        builder.indices.extend([a, b, c]);
                    }
                }
            }
            first_corner += count;
        }
        let primitives: Vec<Primitive> = groups
            .into_iter()
            .filter(|(_, builder)| !builder.indices.is_empty())
            .map(|(material, builder)| Primitive {
                positions: builder.positions,
                normals: if normals.is_some() { builder.normals } else { Vec::new() },
                uvs: if uvs.is_some() { builder.uvs } else { Vec::new() },
                indices: builder.indices,
                material,
            })
            .collect();
        if primitives.is_empty() {
            return None;
        }
        self.scene.meshes.push(glb::Mesh { primitives });
        Some(self.scene.meshes.len() - 1)
    }

    /// An attribute's value: its default, or else its value at the start.
    fn value(&self, attribute: &usd::Attribute) -> Option<sdf::Value> {
        match attribute.get::<sdf::Value>() {
            Ok(Some(value)) => Some(value),
            _ => attribute.get_at::<sdf::Value>(self.time).ok().flatten(),
        }
    }

    /// A primvar (or `normals`): its values, flattened through its indices,
    /// and how they're spread over the mesh.
    fn primvar(&self, prim: &usd::Prim, name: &str) -> Option<(sdf::Value, Spread, Option<Vec<i32>>)> {
        let attribute = prim.attribute(name);
        let values = self.value(&attribute)?;
        let spread = match attribute.get_metadata::<sdf::Value>("interpolation") {
            Ok(Some(value)) => Spread::of(value.as_str().unwrap_or_default()),
            _ => Spread::Vertex,
        };
        let indices = self
            .value(&prim.attribute(format!("{name}:indices")))
            .and_then(|value| ints(&value));
        Some((values, spread, indices))
    }

    fn normals(&self, prim: &usd::Prim) -> Option<PerCorner<[f32; 3]>> {
        let (values, spread, indices) = self
            .primvar(prim, "primvars:normals")
            .or_else(|| self.primvar(prim, "normals"))?;
        PerCorner::new(vec3s(&values)?, spread, indices)
    }

    fn uvs(&self, prim: &usd::Prim) -> Option<PerCorner<[f32; 2]>> {
        let named = ["st", "st0", "st_0", "UVMap", "map1", "uv", "UV"]
            .into_iter()
            .find_map(|name| self.primvar(prim, &format!("primvars:{name}")));
        let found = named.or_else(|| {
            // Any other two-number primvar a tool named its own way.
            let attributes = prim.attributes_in_namespace("primvars").ok()?;
            attributes.iter().find_map(|attribute| {
                let name = attribute.path().name()?.to_owned();
                if name.ends_with(":indices") {
                    return None;
                }
                let found = self.primvar(prim, &name)?;
                vec2s(&found.0).map(|_| found)
            })
        })?;
        let (values, spread, indices) = found;
        // USD's st origin is bottom left, glTF's top left.
        let flipped = vec2s(&values)?
            .into_iter()
            .map(|[u, v]| [u, 1.0 - v])
            .collect();
        PerCorner::new(flipped, spread, indices)
    }

    fn display_color(&self, prim: &usd::Prim) -> Option<[f32; 3]> {
        let (values, _, _) = self.primvar(prim, "primvars:displayColor")?;
        vec3s(&values)?.first().copied()
    }

    /// The faces of each `materialBind` subset of `prim`.
    fn material_subsets(&self, prim: &usd::Prim) -> Vec<Subset> {
        let Ok(children) = prim.children() else {
            return Vec::new();
        };
        children
            .into_iter()
            .filter(|child| {
                child
                    .type_name()
                    .ok()
                    .flatten()
                    .is_some_and(|kind| kind.as_str() == "GeomSubset")
            })
            .filter(|child| {
                let family = token(&child.attribute("familyName"), None);
                let element = token(&child.attribute("elementType"), None);
                family.as_deref().is_none_or(|family| family == "materialBind")
                    && element.as_deref().is_none_or(|element| element == "face")
            })
            .filter_map(|child| {
                let faces = ints(&self.value(&child.attribute("indices"))?)?
                    .into_iter()
                    .filter_map(|face| usize::try_from(face).ok())
                    .collect();
                Some(Subset { prim: child, faces })
            })
            .collect()
    }

    /// The material bound to `prim` (or above it), if any.
    fn bound_material(&self, prim: &usd::Prim) -> Option<sdf::Path> {
        if let Ok(Some(api)) = shade::MaterialBindingAPI::get(self.stage, prim.path().clone())
            && let Ok(Some(material)) = api.compute_bound_material("")
        {
            return Some(material);
        }
        // Many files bind without applying the API: read the relationship,
        // on the prim and then up its ancestors.
        let mut at = Some(prim.clone());
        while let Some(prim) = at {
            if let Ok(targets) = prim.relationship("material:binding").targets()
                && let Some(target) = targets.into_iter().next()
            {
                return Some(target);
            }
            at = prim.parent();
        }
        None
    }

    /// The glTF material for a bound USD material (or none, then a plain
    /// one in `display` color), made once.
    fn material(
        &mut self,
        path: Option<sdf::Path>,
        display: Option<[f32; 3]>,
        double_sided: bool,
    ) -> Option<usize> {
        if path.is_none() && display.is_none() && !double_sided {
            return None;
        }
        let color_key = display.unwrap_or([0.18; 3]).map(f32::to_bits);
        let key = (
            path.as_ref().map(|path| path.as_str().to_owned()).unwrap_or_default(),
            double_sided,
            color_key,
        );
        if let Some(&index) = self.materials.get(&key) {
            return Some(index);
        }
        let mut material = Material {
            name: path
                .as_ref()
                .and_then(|path| path.name().map(str::to_owned))
                .unwrap_or_else(|| "Display color".into()),
            double_sided,
            ..Material::default()
        };
        if let Some([r, g, b]) = display {
            material.base_color = [r, g, b, 1.0];
        }
        if let Some(path) = &path {
            match self.surface(path) {
                Some(surface) => self.apply(&mut material, &surface),
                None => {
                    let shown = path.as_str().to_owned();
                    self.warn_once(
                        format!("surface {shown}"),
                        format!(
                            "its material `{shown}` isn't a UsdPreviewSurface, so it's drawn in its display color"
                        ),
                    );
                }
            }
        }
        self.scene.materials.push(material);
        let index = self.scene.materials.len() - 1;
        self.materials.insert(key, index);
        Some(index)
    }

    /// The `UsdPreviewSurface` inputs of the material at `path`.
    fn surface(&self, path: &sdf::Path) -> Option<Surface> {
        let material = shade::Material::get(self.stage, path.clone()).ok()??;
        let terminal = match material.compute_surface_source(&[]).ok()? {
            Some(terminal) => terminal,
            None => {
                // A material with only a render-context output (`outputs:mtlx:surface`).
                let mut contexts: Vec<String> = material
                    .surface_outputs()
                    .ok()?
                    .iter()
                    .filter_map(|output| output.base_name().strip_suffix(":surface").map(String::from))
                    .collect();
                contexts.sort();
                let contexts: Vec<&str> = contexts.iter().map(String::as_str).collect();
                material.compute_surface_source(&contexts).ok()??
            }
        };
        let shader = terminal.sources().first()?.shader()?;
        let surface = shade::nodes::PreviewSurface::from_shader(shader.clone()).ok()??;
        Some(Surface {
            diffuse: input(&surface.diffuse_color_input()),
            emissive: input(&surface.emissive_color_input()),
            metallic: input(&surface.metallic_input()),
            roughness: input(&surface.roughness_input()),
            opacity: input(&surface.opacity_input()),
            opacity_threshold: input(&surface.opacity_threshold_input()),
            normal: input(&surface.normal_input()),
            occlusion: input(&surface.occlusion_input()),
        })
    }

    /// Fills `material` from a preview surface's inputs.
    fn apply(&mut self, material: &mut Material, surface: &Surface) {
        if let Some([r, g, b]) = surface.diffuse.value.as_ref().and_then(vec3) {
            material.base_color = [r, g, b, 1.0];
        }
        if let Some(metallic) = surface.metallic.value.as_ref().and_then(number) {
            material.metallic = metallic as f32;
        }
        if let Some(roughness) = surface.roughness.value.as_ref().and_then(number) {
            material.roughness = roughness as f32;
        }
        if let Some(emissive) = surface.emissive.value.as_ref().and_then(vec3) {
            material.emissive = emissive;
        }
        let opacity = surface
            .opacity
            .value
            .as_ref()
            .and_then(number)
            .unwrap_or(1.0) as f32;
        material.base_color[3] = opacity;
        let threshold = surface
            .opacity_threshold
            .value
            .as_ref()
            .and_then(number)
            .unwrap_or(0.0) as f32;

        let diffuse = self.texture_bytes(&surface.diffuse);
        let opacity_map = self.texture_bytes(&surface.opacity);
        // Base color, with opacity in its alpha when it's a map.
        if let Some(opacity_bytes) = &opacity_map {
            let base = diffuse.as_ref().map(|(_, bytes, _)| bytes.as_slice());
            let name = diffuse
                .as_ref()
                .map_or_else(|| opacity_bytes.0.clone(), |(name, _, _)| name.clone());
            let color = [material.base_color[0], material.base_color[1], material.base_color[2]];
            let key = format!("opacity {:?} {:?} {color:?}", surface.diffuse.texture, surface.opacity.texture);
            material.base_color_texture = self.image(key, || {
                textures::with_opacity(
                    &textures::with_extension(&name, "png"),
                    base,
                    color,
                    (&opacity_bytes.1, opacity_bytes.2),
                )
            });
            if diffuse.is_some() {
                material.base_color = [1.0, 1.0, 1.0, 1.0];
            } else {
                material.base_color[3] = 1.0;
            }
        } else if let Some((name, bytes, _)) = diffuse {
            let key = as_is(&surface.diffuse);
            material.base_color_texture = self.image(key, || textures::embed(&name, bytes));
            if material.base_color_texture.is_some() {
                let alpha = material.base_color[3];
                material.base_color = [1.0, 1.0, 1.0, alpha];
            }
        }
        material.alpha = if threshold > 0.0 {
            Alpha::Mask(threshold)
        } else if opacity < 1.0 || opacity_map.is_some() {
            Alpha::Blend
        } else {
            Alpha::Opaque
        };

        // Metalness and roughness in one image.
        let metal = self.texture_bytes(&surface.metallic);
        let rough = self.texture_bytes(&surface.roughness);
        if metal.is_some() || rough.is_some() {
            let name = metal
                .as_ref()
                .or(rough.as_ref())
                .map(|(name, _, _)| textures::with_extension(name, "png"))
                .unwrap_or_default();
            // Already glTF's layout (an ORM or ARM map): used as it is.
            let packed = matches!(
                (&metal, &rough, &surface.metallic.texture, &surface.roughness.texture),
                (Some((_, _, Channel::Blue)), Some((_, _, Channel::Green)), Some((a, _)), Some((b, _))) if a == b
            );
            let key = if packed {
                as_is(&surface.metallic)
            } else {
                format!("mr {:?} {:?}", surface.metallic.texture, surface.roughness.texture)
            };
            let whole = packed.then(|| metal.clone()).flatten();
            material.metallic_roughness_texture = self.image(key, || {
                if let Some((name, bytes, _)) = whole {
                    return textures::embed(&name, bytes);
                }
                textures::pack_metal_rough(
                    &format!("metallic_roughness_{name}"),
                    metal.as_ref().map(|(_, bytes, channel)| (bytes.as_slice(), *channel)),
                    rough.as_ref().map(|(_, bytes, channel)| (bytes.as_slice(), *channel)),
                )
            });
            if metal.is_some() {
                material.metallic = 1.0;
            }
            if rough.is_some() {
                material.roughness = 1.0;
            }
        }
        if let Some((name, bytes, _)) = self.texture_bytes(&surface.normal) {
            let key = as_is(&surface.normal);
            material.normal_texture = self.image(key, || textures::embed(&name, bytes));
        }
        if let Some((name, bytes, channel)) = self.texture_bytes(&surface.occlusion) {
            let key = match channel {
                Channel::Red | Channel::Color => as_is(&surface.occlusion),
                _ => format!("occlusion {:?}", surface.occlusion.texture),
            };
            material.occlusion_texture = self.image(key, || match channel {
                Channel::Red | Channel::Color => textures::embed(&name, bytes),
                channel => textures::single_channel(&textures::with_extension(&name, "png"), &bytes, channel),
            });
        }
        if let Some((name, bytes, _)) = self.texture_bytes(&surface.emissive) {
            let key = as_is(&surface.emissive);
            material.emissive_texture = self.image(key, || textures::embed(&name, bytes));
            if material.emissive_texture.is_some() {
                material.emissive = [1.0; 3];
            }
        }
    }

    /// The file an input's texture names (its name, bytes and channel), or
    /// `None` without one; a texture that isn't found is warned about once.
    fn texture_bytes(&mut self, input: &Input) -> Option<(String, Vec<u8>, Channel)> {
        let (asset, channel) = input.texture.as_ref()?;
        if asset.contains("<UDIM>") {
            self.warn_once(
                asset.clone(),
                format!("its texture `{asset}` is a UDIM set, which glTF can't hold, so it's left out"),
            );
            return None;
        }
        match self.files.read(asset) {
            Some((name, bytes)) => Some((name, bytes, *channel)),
            None => {
                self.warn_once(
                    asset.clone(),
                    format!("its texture `{asset}` isn't there, so it's drawn without it"),
                );
                None
            }
        }
    }

    /// The scene's image made by `make`, made once per `key`.
    fn image(&mut self, key: String, make: impl FnOnce() -> Option<EmbeddedImage>) -> Option<usize> {
        if let Some(found) = self.images.get(&key) {
            return *found;
        }
        let made = make().map(|image| {
            self.scene.images.push(image);
            self.scene.images.len() - 1
        });
        if made.is_none() {
            self.warn_once(
                format!("decode {key}"),
                "one of its textures couldn't be read as an image, so it's drawn without it".into(),
            );
        }
        self.images.insert(key, made);
        made
    }
}

/// The image key of a texture file embedded as it is, so maps that share a
/// file (an ARM map's occlusion and metal-roughness) share one image.
fn as_is(input: &Input) -> String {
    let asset = input.texture.as_ref().map_or("", |(asset, _)| asset.as_str());
    format!("file {asset}")
}

/// A `GeomSubset` and the faces it holds.
struct Subset {
    prim: usd::Prim,
    faces: Vec<usize>,
}

/// One `UsdPreviewSurface` input: an authored value, or a texture file and
/// the channel read from it.
#[derive(Default)]
struct Input {
    value: Option<sdf::Value>,
    texture: Option<(String, Channel)>,
}

struct Surface {
    diffuse: Input,
    emissive: Input,
    metallic: Input,
    roughness: Input,
    opacity: Input,
    opacity_threshold: Input,
    normal: Input,
    occlusion: Input,
}

/// Reads a shader input: the `UsdUVTexture` it's connected to (through
/// node graphs too), or else its own value.
fn input(input: &shade::Input) -> Input {
    if let Some(texture) = connected_texture(input) {
        return Input {
            value: None,
            texture: Some(texture),
        };
    }
    Input {
        value: input.get::<sdf::Value>().ok().flatten(),
        texture: None,
    }
}

fn connected_texture(input: &shade::Input) -> Option<(String, Channel)> {
    let produced = input
        .value_producing_attributes(shade::ProducerFilter::ShaderOutputsOnly)
        .ok()?;
    let source = produced.first()?;
    let channel = Channel::from_output(source.path().name().unwrap_or_default());
    let shader = shade::Shader::get(input.attribute().stage(), source.path().prim_path()).ok()??;
    let texture = shade::nodes::UVTexture::from_shader(shader).ok()??;
    let file = texture.file_input();
    let produced = file
        .value_producing_attributes(shade::ProducerFilter::Any)
        .ok()?;
    let value = produced.first()?.attribute().get::<sdf::Value>().ok()??;
    Some((value.as_str()?.to_owned(), channel))
}

/// How a primvar's values are spread over a mesh.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Spread {
    /// One value for the whole mesh.
    Constant,
    /// One per face.
    Uniform,
    /// One per point (`vertex` and `varying`).
    Vertex,
    /// One per face corner.
    FaceVarying,
}

impl Spread {
    fn of(token: &str) -> Self {
        match token {
            "constant" => Spread::Constant,
            "uniform" => Spread::Uniform,
            "faceVarying" => Spread::FaceVarying,
            _ => Spread::Vertex,
        }
    }
}

/// Values for a mesh's corners, picked by how they're spread.
struct PerCorner<T> {
    values: Vec<T>,
    spread: Spread,
    indices: Option<Vec<i32>>,
}

impl<T: Copy> PerCorner<T> {
    fn new(values: Vec<T>, spread: Spread, indices: Option<Vec<i32>>) -> Option<Self> {
        (!values.is_empty()).then_some(Self {
            values,
            spread,
            indices,
        })
    }

    /// The number of the value for a corner (of `point`, the mesh's
    /// `corner`th, on `face`), before indices: what makes two corners share
    /// a vertex.
    fn pick(&self, point: usize, corner: usize, face: usize) -> u32 {
        let at = match self.spread {
            Spread::Constant => 0,
            Spread::Uniform => face,
            Spread::Vertex => point,
            Spread::FaceVarying => corner,
        };
        let at = match &self.indices {
            Some(indices) => indices.get(at).map_or(0, |&index| index.max(0) as usize),
            None => at,
        };
        at as u32
    }

    fn get(&self, picked: u32) -> T {
        self.values
            .get(picked as usize)
            .or(self.values.first())
            .copied()
            .expect("PerCorner always has a value")
    }
}

/// One primitive's vertices, shared where a point, its normal and its
/// texture coordinate are all the same.
#[derive(Default)]
struct Builder {
    positions: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    uvs: Vec<[f32; 2]>,
    indices: Vec<u32>,
    made: HashMap<(usize, Option<u32>, Option<u32>), u32>,
}

impl Builder {
    fn vertex(
        &mut self,
        points: &[[f32; 3]],
        point: usize,
        normal: Option<u32>,
        uv: Option<u32>,
        normals: Option<&PerCorner<[f32; 3]>>,
        uvs: Option<&PerCorner<[f32; 2]>>,
    ) -> u32 {
        if let Some(&index) = self.made.get(&(point, normal, uv)) {
            return index;
        }
        let index = self.positions.len() as u32;
        self.positions.push(points[point]);
        if let (Some(normals), Some(normal)) = (normals, normal) {
            self.normals.push(normals.get(normal));
        }
        if let (Some(uvs), Some(uv)) = (uvs, uv) {
            self.uvs.push(uvs.get(uv));
        }
        self.made.insert((point, normal, uv), index);
        index
    }
}

/// A token attribute's value.
fn token(attribute: &usd::Attribute, _time: Option<usd::TimeCode>) -> Option<String> {
    let value = attribute.get::<sdf::Value>().ok()??;
    value.as_str().map(str::to_owned)
}

fn number(value: &sdf::Value) -> Option<f64> {
    match value {
        sdf::Value::Float(value) => Some(f64::from(*value)),
        sdf::Value::Double(value) => Some(*value),
        sdf::Value::Half(value) => Some(f64::from(value.to_f32())),
        sdf::Value::Int(value) => Some(f64::from(*value)),
        _ => None,
    }
}

fn vec3(value: &sdf::Value) -> Option<[f32; 3]> {
    match value {
        sdf::Value::Vec3f(v) => Some([v.x, v.y, v.z]),
        sdf::Value::Vec3d(v) => Some([v.x as f32, v.y as f32, v.z as f32]),
        sdf::Value::Vec3h(v) => Some([v.x.to_f32(), v.y.to_f32(), v.z.to_f32()]),
        _ => None,
    }
}

fn vec3s(value: &sdf::Value) -> Option<Vec<[f32; 3]>> {
    match value {
        sdf::Value::Vec3fVec(values) => Some(values.iter().map(|v| [v.x, v.y, v.z]).collect()),
        sdf::Value::Vec3dVec(values) => Some(
            values
                .iter()
                .map(|v| [v.x as f32, v.y as f32, v.z as f32])
                .collect(),
        ),
        sdf::Value::Vec3hVec(values) => Some(
            values
                .iter()
                .map(|v| [v.x.to_f32(), v.y.to_f32(), v.z.to_f32()])
                .collect(),
        ),
        _ => None,
    }
}

fn vec2s(value: &sdf::Value) -> Option<Vec<[f32; 2]>> {
    match value {
        sdf::Value::Vec2fVec(values) => Some(values.iter().map(|v| [v.x, v.y]).collect()),
        sdf::Value::Vec2dVec(values) => {
            Some(values.iter().map(|v| [v.x as f32, v.y as f32]).collect())
        }
        sdf::Value::Vec2hVec(values) => {
            Some(values.iter().map(|v| [v.x.to_f32(), v.y.to_f32()]).collect())
        }
        _ => None,
    }
}

fn ints(value: &sdf::Value) -> Option<Vec<i32>> {
    match value {
        sdf::Value::IntVec(values) => Some(values.clone()),
        sdf::Value::UintVec(values) => Some(values.iter().map(|&v| v as i32).collect()),
        sdf::Value::Int64Vec(values) => Some(values.iter().map(|&v| v as i32).collect()),
        _ => None,
    }
}

#[cfg(test)]
mod tests;
