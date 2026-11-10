//! SplineMeshes and SplineScatters in the editor (Phase 74): what they
//! make is made again each frame their spline or settings changed
//! (`generate_frame`, never an undo step: only their settings are), Insert
//! puts one in a Spline, the Assets panel picks their models, Bake keeps
//! what they made as ordinary Instances, and Flatten Terrain Along levels
//! the ground under a spline. Bake and Flatten are one undo step each.

use std::sync::Arc;

use axcenity_core::spline::{SplineCurve, is_generator};
use axcenity_core::{TerrainData, Value, Vector3};
use axcenity_terrain::Placement;

use super::*;

/// Studs of shoulder Flatten Terrain Along adds on each side of a road,
/// and the width it flattens under a spline with no SplineMesh.
const SHOULDER: f32 = 1.0;
const PLAIN_WIDTH: f32 = 8.0;

impl EditorApp {
    /// Makes again what the edit world's SplineMeshes and SplineScatters
    /// make, where their splines or settings changed. Not during Play: the
    /// game's world has its own (in its script runtime).
    pub(super) fn generate_frame(&mut self) {
        if self.play.is_some() {
            return;
        }
        for problem in self.generators.update(&mut self.world, &self.assets) {
            self.output.warning(problem);
        }
    }

    /// Insert > Spline Mesh or Spline Scatter: into the Spline `near` is
    /// (or is in), or a new Spline in front of the camera (its own undo
    /// step), selected so the Assets panel picks its models.
    pub(super) fn insert_spline_generator(&mut self, what: Insertable, near: Option<InstanceId>) {
        let spline = match near.and_then(|id| spline_context(&self.world, id)) {
            Some(spline) => spline,
            None => {
                self.insert_spline();
                let Some(spline) = self.selected else {
                    return;
                };
                spline
            }
        };
        self.insert_named(what, Some(spline), what.class().as_str(), Vec::new());
        self.generate_frame();
    }

    /// The SplineMesh or SplineScatter selected while editing, and the
    /// models it uses, for the Assets panel to pick.
    pub(super) fn spline_models_picking(&self) -> Option<(InstanceId, Vec<String>)> {
        if self.playing() {
            return None;
        }
        let id = self.selected?;
        let instance = self.world.get(id)?;
        let models = match instance.class() {
            ClassName::SplineMesh => instance.property("MeshId")?.as_str()?.trim().to_owned(),
            ClassName::SplineScatter => instance.property("Models")?.as_str()?.to_owned(),
            _ => return None,
        };
        Some((id, split_models(&models)))
    }

    /// A model clicked in the Assets panel while `generator` is selected:
    /// a SplineMesh bends it (sized as the model is long and wide, unless
    /// it had one already), a SplineScatter adds it or takes it out. One
    /// undo step.
    pub(super) fn pick_spline_model(&mut self, generator: InstanceId, path: &str) {
        let Some(instance) = self.world.get(generator) else {
            return;
        };
        let name = instance.name().to_owned();
        let mut edits = Vec::new();
        match instance.class() {
            ClassName::SplineMesh => {
                let had = instance
                    .property("MeshId")
                    .and_then(Value::as_str)
                    .is_some_and(|id| !id.trim().is_empty());
                edits.push(Edit::Property("MeshId", Value::String(path.to_owned())));
                if !had && let Some(model) = self.assets.model_blocking(path) {
                    let [x, y, z] = model.size.map(|v| if v > 0.0 { v } else { 1.0 });
                    edits.push(Edit::Property(
                        "Size",
                        Value::Vector3(Vector3::new(x, y, z)),
                    ));
                }
            }
            ClassName::SplineScatter => {
                let mut models = split_models(
                    instance
                        .property("Models")
                        .and_then(Value::as_str)
                        .unwrap_or_default(),
                );
                match models.iter().position(|model| model == path) {
                    Some(index) => {
                        models.remove(index);
                    }
                    None => models.push(path.to_owned()),
                }
                edits.push(Edit::Property("Models", Value::String(models.join("; "))));
            }
            _ => return,
        }
        self.change(
            generator,
            Field::Property("MeshId"),
            format!("Change models of {name}"),
            edits,
        );
        self.history.seal();
        self.generate_frame();
    }

    /// Bake: what the SplineMesh or SplineScatter `generator` made becomes
    /// ordinary Instances in a Model named after it, beside its Spline, and
    /// it's removed, as one undo step. A SplineMesh's bent pieces are
    /// saved as `assets/models/<name>.spline.glb` first (kept on undo, as
    /// a Fracture's pieces are).
    pub(super) fn bake_spline(&mut self, generator: InstanceId) {
        if self.playing() {
            return;
        }
        self.generate_frame();
        let Some(instance) = self.world.get(generator) else {
            return;
        };
        if !is_generator(instance.class()) {
            return;
        }
        let name = instance.name().to_owned();
        let class = instance.class();
        let results = instance.children().to_vec();
        let Some(spline) = instance.parent() else {
            return;
        };
        if results.is_empty() {
            self.output.warning(format!(
                "{name} hasn't made anything to bake: give it a model and a Spline with points."
            ));
            return;
        }
        let mut pieces_file = None;
        if class == ClassName::SplineMesh {
            let Some(folder) = self.project.as_ref().map(|project| project.folder.clone()) else {
                self.output
                    .warning("Open or save a project first: the bent pieces are saved in it.");
                return;
            };
            let models: Vec<_> = self
                .generators
                .pieces(generator)
                .iter()
                .filter_map(|id| self.assets.model_blocking(id))
                .collect();
            if models.len() != results.len() {
                self.output
                    .warning(format!("{name}'s pieces aren't ready to bake yet."));
                return;
            }
            let models: Vec<&axcenity_assets::Model> = models.iter().map(Arc::as_ref).collect();
            match axcenity_mesh_edit::save_pieces(&folder, &self.assets, &name, "spline", &models) {
                Ok(path) => pieces_file = Some(path),
                Err(problem) => {
                    self.output.warning(problem);
                    return;
                }
            }
        }
        let before = self.world.clone();
        let holder = self
            .world
            .get(spline)
            .and_then(|s| s.parent())
            .unwrap_or(self.world.workspace());
        let model = self
            .world
            .create(ClassName::Model)
            .expect("a Model can be made");
        let unique = tree::unique_name(&self.world, holder, &name);
        self.world.set_name(model, unique).expect("just made");
        for (i, &id) in results.iter().enumerate() {
            if let Some(file) = &pieces_file {
                let mesh_id = Value::String(format!("{file}#Piece{}", i + 1));
                let _ = self.world.set_property(id, "MeshId", mesh_id);
            }
            let _ = self.world.set_parent(id, Some(model));
        }
        let index = self
            .world
            .get(holder)
            .and_then(|h| h.children().iter().position(|&c| c == spline))
            .map_or(0, |i| i + 1);
        let _ = self.world.set_parent_at(model, holder, index);
        let _ = self.world.destroy(generator);
        self.history
            .record_world(&mut self.world, format!("Bake {name}"), before);
        self.select_from_viewport(Some(model));
        let count = results.len();
        self.output.info(match pieces_file {
            Some(file) => format!("Baked {name}: {count} pieces, saved in {file}."),
            None => format!("Baked {name}: {count} models."),
        });
    }

    /// Flatten Terrain Along: the first Terrain under the Spline `id` is
    /// (or is in) brought to the curve's height in a strip as wide as its
    /// first SplineMesh's road plus a shoulder, as one undo step.
    pub(super) fn flatten_terrain_along(&mut self, id: InstanceId) {
        if self.playing() {
            return;
        }
        let Some(spline) = spline_context(&self.world, id) else {
            return;
        };
        let name = self.world.get(spline).map_or("", |s| s.name()).to_owned();
        let Some(curve) = SplineCurve::of(&self.world, spline) else {
            self.output.warning(format!(
                "{name} has no points to flatten the terrain along."
            ));
            return;
        };
        let terrain = self
            .world
            .descendants(self.world.workspace())
            .into_iter()
            .find(|&id| self.world.terrain(id).is_some());
        let (Some(terrain), Some(placement)) = (
            terrain,
            terrain
                .and_then(|id| self.world.get(id))
                .and_then(Placement::of),
        ) else {
            self.output.error(
                "Insert a Terrain first: Flatten Terrain Along levels the ground under the spline.",
            );
            return;
        };
        let road = self.world.get(spline).and_then(|s| {
            s.children().iter().find_map(|&child| {
                let child = self.world.get(child)?;
                (child.class() == ClassName::SplineMesh)
                    .then(|| child.property("Size")?.as_vector3())
                    .flatten()
            })
        });
        let width = road.map_or(PLAIN_WIDTH, |size| size.x.abs() + 2.0 * SHOULDER);
        let Some(mut data) = self
            .world
            .terrain(terrain)
            .map(|data| TerrainData::clone(data))
        else {
            return;
        };
        let before = self.world.clone();
        let set = axcenity_procedural::flatten_along(&mut data, &placement, &curve, width);
        if set == 0 {
            self.output.warning(format!(
                "{name} isn't over the Terrain, so nothing was changed."
            ));
            return;
        }
        self.world.set_terrain(terrain, Arc::new(data));
        self.history.record_world(
            &mut self.world,
            format!("Flatten Terrain Along {name}"),
            before,
        );
        self.output.info(format!(
            "Flattened {set} terrain samples along {name}, {width} studs wide."
        ));
    }
}

/// The Spline `id` is, or the one it's in (a SplinePoint, SplineMesh or
/// SplineScatter's).
fn spline_context(world: &World, id: InstanceId) -> Option<InstanceId> {
    let instance = world.get(id)?;
    if instance.class() == ClassName::Spline {
        return Some(id);
    }
    crate::splines::spline_of(world, id)
}

/// Model paths separated by `;`, without blanks.
fn split_models(text: &str) -> Vec<String> {
    text.split(';')
        .map(str::trim)
        .filter(|model| !model.is_empty())
        .map(str::to_owned)
        .collect()
}
