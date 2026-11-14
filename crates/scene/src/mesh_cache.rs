//! What MeshParts' draw items share from frame to frame (I8): each model's
//! meshes and file materials, and each animated MeshPart's [`Skin`].
//!
//! Without it every MeshPart made a new mesh per primitive each frame (and
//! with it new copies for each level of detail, and new textures for its
//! material), so 2,000 copies of one model handed the renderer 2,000
//! meshes it had to tell apart. Now they all share one.

use std::collections::HashMap;
use std::sync::Arc;

use axcenity_assets::{Assets, Model, Posed, Slot};
use axcenity_core::InstanceId;
use axcenity_render::{Shape, Skin};
use rustc_hash::FxHashSet;

use crate::draw_list::{ModelPiece, model_pieces};

/// Kept by the host in its [`TerrainCache`](crate::TerrainCache), so it
/// lives as long as the host draws the same project. Models and skins are
/// apart so a MeshPart can hold its model's pieces while it gets its skin.
#[derive(Default)]
pub(crate) struct MeshCache {
    pub(crate) models: Models,
    pub(crate) skins: Skins,
}

impl MeshCache {
    /// Call once before drawing a frame.
    pub(crate) fn start_frame(&mut self) {
        self.models.frame += 1;
        self.skins.frame += 1;
    }

    /// Call once after drawing a frame: forgets models and poses nothing
    /// drew, so a deleted MeshPart's or a replaced model's memory goes.
    pub(crate) fn end_frame(&mut self) {
        let frame = self.models.frame;
        self.models
            .by_id
            .retain(|_, cached| cached.checked == frame);
        let frame = self.skins.frame;
        self.skins.by_part.retain(|_, cached| cached.used == frame);
        self.forget_rest_skins();
    }

    /// Forgets the rest skins of models no longer kept.
    fn forget_rest_skins(&mut self) {
        let models = &self.models.by_id;
        self.skins
            .rest
            .retain(|key, _| models.values().any(|cached| cached.model.key() == *key));
    }

    /// Call once after drawing a frame when draw items are kept between
    /// frames: a kept item doesn't ask for its model again, so what
    /// nothing used this frame says nothing about models. Skins are always
    /// asked for each frame. `models_in_use` is `Some` when the models in
    /// use may have changed.
    pub(crate) fn end_frame_retained(&mut self, models_in_use: Option<FxHashSet<usize>>) {
        let frame = self.skins.frame;
        self.skins.by_part.retain(|_, cached| cached.used == frame);
        if let Some(in_use) = models_in_use {
            self.models.by_id.retain(|_, cached| {
                cached
                    .pieces
                    .first()
                    .and_then(|piece| mesh_address(&piece.shape))
                    .is_some_and(|address| in_use.contains(&address))
            });
        }
        self.forget_rest_skins();
    }
}

/// Which mesh `shape` is, by the address of the one copy every draw item
/// showing it shares. `None` for a built-in shape.
pub(crate) fn mesh_address(shape: &Shape) -> Option<usize> {
    match shape {
        Shape::Mesh(mesh) => Some(Arc::as_ptr(mesh).cast::<()>() as usize),
        _ => None,
    }
}

/// Models and what their primitives share, by `MeshId`.
#[derive(Default)]
pub(crate) struct Models {
    by_id: HashMap<String, CachedModel>,
    /// Counts the frames drawn, to tell what this one used.
    frame: u64,
}

/// What drawing finds for a `MeshId` this frame.
pub(crate) enum ModelLookup<'a> {
    Ready(&'a Arc<Model>, &'a [ModelPiece]),
    /// On its way: nothing is drawn for it yet.
    Loading,
    /// No file, or one that can't be loaded.
    Failed,
}

struct CachedModel {
    model: Arc<Model>,
    pieces: Vec<ModelPiece>,
    /// The last frame that checked `model` against the assets'.
    checked: u64,
}

impl Models {
    /// The model at `mesh_id` and what its primitives share, from
    /// [`Assets::model_slot`], so a frame never waits for a file. The assets
    /// are asked once a frame for each model, and when they hand back
    /// another one (it was reloaded) its pieces are made again.
    pub(crate) fn get(&mut self, assets: &Assets, mesh_id: &str) -> ModelLookup<'_> {
        let frame = self.frame;
        // Looked up by `&str` first, so a hit doesn't allocate a key.
        let fresh = match self.by_id.get_mut(mesh_id) {
            Some(cached) if cached.checked == frame => false,
            Some(cached) => {
                match assets.model_slot(mesh_id) {
                    Slot::Ready(model) => {
                        if !Arc::ptr_eq(&model, &cached.model) {
                            cached.pieces = model_pieces(&model);
                            cached.model = model;
                        }
                    }
                    // Being read again: the last version stays on screen.
                    Slot::Loading => {}
                    Slot::Failed => return ModelLookup::Failed,
                }
                cached.checked = frame;
                false
            }
            None => true,
        };
        if fresh {
            let model = match assets.model_slot(mesh_id) {
                Slot::Ready(model) => model,
                Slot::Loading => return ModelLookup::Loading,
                Slot::Failed => return ModelLookup::Failed,
            };
            let pieces = model_pieces(&model);
            self.by_id.insert(
                mesh_id.to_owned(),
                CachedModel {
                    model,
                    pieces,
                    checked: frame,
                },
            );
        }
        match self.by_id.get(mesh_id) {
            Some(cached) => ModelLookup::Ready(&cached.model, &cached.pieces),
            None => ModelLookup::Failed,
        }
    }
}

/// Animated MeshParts' skins, by MeshPart.
#[derive(Default)]
pub(crate) struct Skins {
    by_part: HashMap<InstanceId, CachedSkin>,
    /// Each model's skin of its blend shapes' default weights, by model
    /// key, `None` when they're all 0.
    rest: HashMap<u64, Option<Arc<Skin>>>,
    frame: u64,
}

struct CachedSkin {
    /// The [`Posed::version`] `skin` holds, if any.
    version: Option<u64>,
    skin: Arc<Skin>,
    used: u64,
}

impl Skins {
    /// The MeshPart `id`'s skin for `posed`: the same one while the pose
    /// is the same, and otherwise last frame's buffers changed to the new
    /// pose when nothing else holds them any more.
    pub(crate) fn get(&mut self, id: InstanceId, posed: &Posed) -> Arc<Skin> {
        let frame = self.frame;
        let cached = self.by_part.entry(id).or_insert_with(|| CachedSkin {
            version: None,
            skin: Arc::new(Skin::default()),
            used: frame,
        });
        cached.used = frame;
        if cached.version != Some(posed.version) {
            cached.version = Some(posed.version);
            // Still held (by last frame's draw list, say): a new one.
            match Arc::get_mut(&mut cached.skin) {
                Some(skin) => {
                    skin.set_pose(posed.joints(), posed.normal_matrices());
                    skin.set_morph_weights(posed.morph_weights());
                }
                None => {
                    let mut skin = Skin::default();
                    skin.set_pose(posed.joints(), posed.normal_matrices());
                    skin.set_morph_weights(posed.morph_weights());
                    cached.skin = Arc::new(skin);
                }
            }
        }
        cached.skin.clone()
    }

    /// The skin of `model` at rest when its file leans it toward some of
    /// its blend shapes (default weights): no joints, only those weights.
    /// `None` for a model whose shapes all rest at 0.
    pub(crate) fn rest(&mut self, model: &Model) -> Option<Arc<Skin>> {
        self.rest
            .entry(model.key())
            .or_insert_with(|| {
                let weights: Vec<f32> = model
                    .morph_targets
                    .iter()
                    .map(|target| target.default_weight)
                    .collect();
                weights.iter().any(|weight| *weight != 0.0).then(|| {
                    let mut skin = Skin::default();
                    skin.set_morph_weights(&weights);
                    Arc::new(skin)
                })
            })
            .clone()
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};
    use std::sync::mpsc;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use axcenity_assets::{Assets, LoadStatus, Reload};
    use axcenity_core::{ClassName, InstanceId, Value, Vector3, World};
    use axcenity_render::glam::Vec3;
    use axcenity_render::{Camera, DrawItem, MeshSource, Shape};

    use crate::{Poses, TerrainCache, build_draw_list_posed};

    const PYRAMID: &str = "assets/models/pyramid.gltf";
    /// Only turns a load that never comes into a failed test.
    const LIMIT: Duration = Duration::from_secs(30);

    fn examples() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples")
    }

    fn mesh_part(world: &mut World, mesh_id: &str) -> InstanceId {
        let part = world.create(ClassName::MeshPart).unwrap();
        world.set_parent(part, Some(world.workspace())).unwrap();
        world
            .set_property(part, "MeshId", Value::String(mesh_id.to_owned()))
            .unwrap();
        world
            .set_property(part, "Size", Value::Vector3(Vector3::new(4.0, 4.0, 4.0)))
            .unwrap();
        part
    }

    fn draw(
        world: &World,
        assets: &Assets,
        poses: &Poses,
        cache: &mut TerrainCache,
    ) -> Vec<DrawItem> {
        let camera = Camera {
            position: Vec3::new(0.0, 10.0, 20.0),
            look_at: Vec3::ZERO,
            fov_y_degrees: 60.0,
        };
        build_draw_list_posed(world, assets, camera, poses, cache).items
    }

    fn mesh(item: &DrawItem) -> &Arc<dyn MeshSource> {
        match &item.shape {
            Shape::Mesh(mesh) => mesh,
            shape => panic!("drawn as {shape:?}"),
        }
    }

    #[test]
    fn mesh_parts_of_one_model_share_its_mesh_and_textures_frame_after_frame() {
        let mut world = World::new();
        mesh_part(&mut world, PYRAMID);
        mesh_part(&mut world, PYRAMID);
        let assets = Assets::new(examples().join("hello-mesh"));
        let mut cache = TerrainCache::new();
        let first = draw(&world, &assets, &Poses::new(), &mut cache);
        let second = draw(&world, &assets, &Poses::new(), &mut cache);
        assert_eq!(first.len(), 2);
        let shared = mesh(&first[0]);
        let texture = first[0].material.base_color_texture.as_ref().unwrap();
        for item in first.iter().chain(&second) {
            assert!(Arc::ptr_eq(mesh(item), shared));
            let other = item.material.base_color_texture.as_ref().unwrap();
            assert!(Arc::ptr_eq(other, texture));
        }
        // Each part still has its own id, for its own level of detail.
        assert_ne!(first[0].id, first[1].id);
        assert!(assets.take_errors().is_empty());
    }

    #[test]
    fn a_reloaded_model_is_drawn_with_new_meshes_and_the_old_one_is_let_go() {
        let folder =
            std::env::temp_dir().join(format!("axcenity-mesh-cache-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(folder.join("assets/models")).unwrap();
        let file = std::fs::read(examples().join("hello-mesh").join(PYRAMID)).unwrap();
        std::fs::write(folder.join(PYRAMID), &file).unwrap();
        let mut world = World::new();
        let part = mesh_part(&mut world, PYRAMID);
        let assets = Assets::new(&folder);
        let mut cache = TerrainCache::new();
        let before = mesh(&draw(&world, &assets, &Poses::new(), &mut cache)[0]).clone();

        // The same model again, a byte longer so it's seen to change.
        std::fs::write(folder.join(PYRAMID), [&file[..], b"\n"].concat()).unwrap();
        assert_eq!(assets.reload(PYRAMID), Reload::Reloaded);
        let after = mesh(&draw(&world, &assets, &Poses::new(), &mut cache)[0]).clone();
        assert!(!Arc::ptr_eq(&before, &after));
        // Only this test still holds the old mesh, not the cache.
        assert_eq!(Arc::strong_count(&before), 1);

        // A MeshPart that's gone takes its model out of the cache.
        world.destroy(part).unwrap();
        draw(&world, &assets, &Poses::new(), &mut cache);
        assert_eq!(Arc::strong_count(&after), 1);
        assert!(assets.take_errors().is_empty());
        let _ = std::fs::remove_dir_all(&folder);
    }

    /// Holds every load until the first sender is sent to, and says on the
    /// receiver when one has got that far.
    fn gate_loads(assets: &Assets) -> (mpsc::Sender<()>, mpsc::Receiver<()>) {
        let (release, released) = mpsc::channel::<()>();
        let released = Mutex::new(released);
        let (reached, reaches) = mpsc::channel();
        let reached = Mutex::new(reached);
        assets.set_load_gate(move |_| {
            let _ = reached.lock().unwrap().send(());
            let _ = released.lock().unwrap().recv_timeout(LIMIT);
        });
        (release, reaches)
    }

    #[test]
    fn a_model_still_loading_draws_nothing_and_doesnt_hold_up_the_frame() {
        let mut world = World::new();
        mesh_part(&mut world, PYRAMID);
        let assets = Assets::new(examples().join("hello-mesh"));
        assets.load_textures_in_background(|| {});
        let (release, reached) = gate_loads(&assets);
        let mut cache = TerrainCache::new();
        let poses = Poses::new();

        // The file is held open below, so a frame that waited for it would
        // sit here until the gate gave up, and then find the model.
        assert!(draw(&world, &assets, &poses, &mut cache).is_empty());
        reached.recv_timeout(LIMIT).expect("the load started");
        assert!(
            draw(&world, &assets, &poses, &mut cache).is_empty(),
            "not even the grey box of a model that can't be loaded"
        );

        release.send(()).unwrap();
        assert_eq!(assets.request_model(PYRAMID).wait(), LoadStatus::Ready);
        let items = draw(&world, &assets, &poses, &mut cache);
        let model = assets.model_ready(PYRAMID).unwrap();
        assert_eq!(items.len(), model.primitives.len(), "drawn once it's in");
        assert!(
            items
                .iter()
                .all(|item| matches!(item.shape, Shape::Mesh(_)))
        );
    }

    #[test]
    fn a_model_that_cant_load_is_the_grey_box_once_that_is_known() {
        const GONE: &str = "assets/models/gone.glb";
        let mut world = World::new();
        mesh_part(&mut world, GONE);
        let assets = Assets::new(examples().join("hello-mesh"));
        assets.load_textures_in_background(|| {});
        let (release, reached) = gate_loads(&assets);
        let mut cache = TerrainCache::new();
        assert!(draw(&world, &assets, &Poses::new(), &mut cache).is_empty());
        reached.recv_timeout(LIMIT).expect("the load started");

        release.send(()).unwrap();
        assert_eq!(assets.request_model(GONE).wait(), LoadStatus::Failed);
        let items = draw(&world, &assets, &Poses::new(), &mut cache);
        assert_eq!(items.len(), 1);
        assert!(matches!(items[0].shape, Shape::Block));
    }

    #[test]
    fn a_model_being_read_again_stays_on_screen() {
        let mut world = World::new();
        mesh_part(&mut world, PYRAMID);
        let assets = Assets::new(examples().join("hello-mesh"));
        assets.load_textures_in_background(|| {});
        assert_eq!(assets.request_model(PYRAMID).wait(), LoadStatus::Ready);
        let mut cache = TerrainCache::new();
        let before = draw(&world, &assets, &Poses::new(), &mut cache).len();
        assert!(before > 0);

        let (release, reached) = gate_loads(&assets);
        assets.forget(PYRAMID);
        assert_eq!(
            draw(&world, &assets, &Poses::new(), &mut cache).len(),
            before
        );
        reached.recv_timeout(LIMIT).expect("the load started");
        assert_eq!(
            draw(&world, &assets, &Poses::new(), &mut cache).len(),
            before
        );
        release.send(()).unwrap();
    }

    #[test]
    fn an_animated_mesh_part_keeps_its_skin_until_its_pose_changes() {
        const NOODLE: &str = "assets/models/noodle.fbx";
        let mut world = World::new();
        let part = mesh_part(&mut world, NOODLE);
        let assets = Assets::new(examples().join("hello-animation"));
        let model = assets.model_blocking(NOODLE).unwrap();
        let rig = model.rig.clone().unwrap();
        let rest = Arc::new(model.pose(&rig.rest_pose()).unwrap());
        let mut bent_pose = rig.rest_pose();
        bent_pose[0].rotation = axcenity_render::glam::Quat::from_rotation_z(0.5);
        let bent = Arc::new(model.pose(&bent_pose).unwrap());
        let mut cache = TerrainCache::new();
        let skin_of = |poses: &Poses, cache: &mut TerrainCache| {
            draw(&world, &assets, poses, cache)[0].skin.clone().unwrap()
        };

        let resting = Poses::from([(part, rest.clone())]);
        let first = skin_of(&resting, &mut cache);
        let again = skin_of(&resting, &mut cache);
        assert!(Arc::ptr_eq(&first, &again), "the same pose, the same skin");
        assert_eq!(first.joints(), rest.joints());

        drop((first, again));
        let moved = skin_of(&Poses::from([(part, bent.clone())]), &mut cache);
        assert_eq!(moved.joints(), bent.joints());
        assert_ne!(moved.joints(), rest.joints());
        assert!(assets.take_errors().is_empty());
    }
}
