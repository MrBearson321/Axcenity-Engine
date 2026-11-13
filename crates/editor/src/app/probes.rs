//! Baking probes (Bake Probes on the ribbon): each LightProbeVolume's grid
//! of probes and each ReflectionProbe's view from the middle of its part is
//! drawn on the GPU, saved in `assets/probes`, and named by its `Bake`, all
//! one undo step. A light probe bake keeps a fingerprint of the world, so a
//! volume whose bake is out of date gets a warning in the Explorer, and the
//! probes that stood inside a solid part or under the ground, which surfaces
//! leave out (roadmap R18.2).

use std::collections::HashSet;
use std::path::Path;

use axcenity_render::{LIGHT_PROBE_BAKE_SIZE, PROBE_BAKE_SIZE, TextureRenderer, wgpu};

use super::*;

/// A probe to bake: the Instance, its part's name and its `Bake` now.
struct ToBake {
    id: InstanceId,
    part_name: String,
    bake: String,
}

impl EditorApp {
    /// Bakes every LightProbeVolume, then every ReflectionProbe (so their
    /// reflections show the volumes' light), inside a part in the Workspace,
    /// as it is now, on `device`. A probe keeps its bake file if it has one
    /// in `assets/probes`; otherwise one is named after its part. The new
    /// `Bake` values are one undo step (the files stay).
    pub(super) fn bake_probes(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) {
        if self.playing() {
            return;
        }
        let Some(folder) = self.project.as_ref().map(|p| p.folder.clone()) else {
            return;
        };
        let volumes = self.probes_to_bake(ClassName::LightProbeVolume);
        let probes = self.probes_to_bake(ClassName::ReflectionProbe);
        if volumes.is_empty() && probes.is_empty() {
            self.output.warning(
                "There are no LightProbeVolumes or ReflectionProbes in parts to bake. \
                 Insert one with Light Probes or Reflection Probe on the ribbon.",
            );
            return;
        }
        if self.assets.texture_progress().is_some() {
            self.output.warning(
                "Some textures are still loading, so the bakes are missing them. \
                 Bake again once they've loaded.",
            );
        }
        let mut used = HashSet::new();
        let volumes = self.bake_light_probes(device, queue, &folder, volumes, &mut used);
        let probes = self.bake_reflection_probes(device, queue, &folder, probes, &mut used);
        self.history.seal();
        // Baking again into the same files changes no `Bake`, so no step.
        self.stale_light_probes = None;
        let mut baked = Vec::new();
        if volumes > 0 {
            let s = if volumes == 1 { "" } else { "s" };
            baked.push(format!("{volumes} light probe volume{s}"));
        }
        if probes > 0 {
            let s = if probes == 1 { "" } else { "s" };
            baked.push(format!("{probes} reflection probe{s}"));
        }
        if !baked.is_empty() {
            let baked = baked.join(" and ");
            self.toasts.success(format!("Baked {baked}"));
            self.output.info(format!(
                "Baked {baked} into {}.",
                axcenity_assets::PROBES_FOLDER
            ));
        }
    }

    /// Every Instance of `class` inside a Part or MeshPart in the Workspace.
    fn probes_to_bake(&self, class: ClassName) -> Vec<ToBake> {
        self.world
            .descendants(self.world.workspace())
            .into_iter()
            .filter_map(|id| {
                let probe = self.world.get(id)?;
                if probe.class() != class {
                    return None;
                }
                let part = self.world.get(probe.parent()?)?;
                if !part.class().is_base_part() {
                    return None;
                }
                let bake = probe
                    .property("Bake")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned();
                Some(ToBake {
                    id,
                    part_name: part.name().to_owned(),
                    bake,
                })
            })
            .collect()
    }

    /// Bakes each light probe volume's grid. Returns how many were baked.
    fn bake_light_probes(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        folder: &Path,
        volumes: Vec<ToBake>,
        used: &mut HashSet<String>,
    ) -> usize {
        let fingerprint = axcenity_scene::light_bake_fingerprint(&self.world);
        let list = self.bake_scene();
        let mut baked = 0;
        for volume in volumes {
            let Some(grid) = axcenity_scene::light_probe_points(&self.world, volume.id) else {
                continue;
            };
            let path = bake_path(
                folder,
                &volume,
                used,
                axcenity_assets::is_light_probes_file,
                axcenity_assets::light_probes_path,
                "LightProbeVolume",
            );
            let part_name = &volume.part_name;
            let probes = match TextureRenderer::bake_light_probes(
                device,
                queue,
                &list,
                &grid.points,
                LIGHT_PROBE_BAKE_SIZE,
                &self.graphics,
            ) {
                Ok(probes) => probes,
                Err(err) => {
                    self.output.error(format!(
                        "Could not bake the light probes in {part_name}: {err:#}"
                    ));
                    continue;
                }
            };
            let invalid = invalid_probes(&self.world, &self.assets, volume.id, &grid.points);
            let Some(probes) = axcenity_assets::LightProbes::new(grid.counts, fingerprint, probes)
                .and_then(|probes| probes.with_invalid(invalid))
            else {
                self.output.error(format!(
                    "Could not bake the light probes in {part_name}: the light came back \
                     as something that isn't a number."
                ));
                continue;
            };
            if let Err(err) = axcenity_assets::write_light_probes(folder, &path, &probes) {
                self.output
                    .error(format!("Could not save the bake `{path}`: {err}"));
                continue;
            }
            self.assets.put_light_probes(&path, probes);
            if volume.bake != path {
                self.apply(
                    volume.id,
                    Edit::Property("Bake", Value::String(path.clone())),
                );
            }
            used.insert(path);
            baked += 1;
        }
        baked
    }

    /// Bakes each reflection probe's view. Returns how many were baked.
    fn bake_reflection_probes(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        folder: &Path,
        probes: Vec<ToBake>,
        used: &mut HashSet<String>,
    ) -> usize {
        let list = self.bake_scene();
        let mut baked = 0;
        for probe in probes {
            let center = self
                .world
                .get(probe.id)
                .and_then(|probe| self.world.get(probe.parent()?))
                .and_then(|part| part.property("Position")?.as_vector3());
            let Some(center) = center else {
                continue;
            };
            let path = bake_path(
                folder,
                &probe,
                used,
                axcenity_assets::is_probe_file,
                axcenity_assets::probe_path,
                "ReflectionProbe",
            );
            let center = axcenity_render::glam::Vec3::new(center.x, center.y, center.z);
            let rgb = match TextureRenderer::bake_reflection_probe(
                device,
                queue,
                &list,
                center,
                PROBE_BAKE_SIZE,
                &self.graphics,
            ) {
                Ok(rgb) => rgb,
                Err(err) => {
                    self.output.error(format!(
                        "Could not bake the probe in {}: {err:#}",
                        probe.part_name
                    ));
                    continue;
                }
            };
            if let Err(err) = axcenity_assets::write_probe(folder, &path, PROBE_BAKE_SIZE, &rgb) {
                self.output
                    .error(format!("Could not save the bake `{path}`: {err}"));
                continue;
            }
            if let Some(image) = axcenity_assets::ProbeImage::new(PROBE_BAKE_SIZE, rgb) {
                self.assets.put_reflection_probe(&path, image);
            }
            if probe.bake != path {
                self.apply(
                    probe.id,
                    Edit::Property("Bake", Value::String(path.clone())),
                );
            }
            used.insert(path);
            baked += 1;
        }
        baked
    }

    /// The world as bakes see it (each bake places its own cameras).
    fn bake_scene(&self) -> axcenity_render::DrawList {
        axcenity_scene::build_draw_list(
            &self.world,
            &self.assets,
            axcenity_render::Camera {
                position: axcenity_render::glam::Vec3::ZERO,
                look_at: axcenity_render::glam::Vec3::NEG_Z,
                fov_y_degrees: 90.0,
            },
        )
    }

    /// The LightProbeVolumes whose bake was made from a different world
    /// than this one (by its fingerprint), worked out again only after the
    /// history changes. Unbaked volumes and missing bakes aren't counted:
    /// those light with the sky, and a missing file is reported in Output.
    pub(super) fn stale_light_probe_volumes(&mut self) -> Vec<InstanceId> {
        if self.playing() {
            return self
                .stale_light_probes
                .as_ref()
                .map(|(_, stale)| stale.clone())
                .unwrap_or_default();
        }
        let revision = self.history.revision();
        if let Some((checked, stale)) = &self.stale_light_probes
            && *checked == revision
        {
            return stale.clone();
        }
        let baked: Vec<(InstanceId, u64)> = self
            .probes_to_bake(ClassName::LightProbeVolume)
            .into_iter()
            .filter_map(|volume| {
                let bake = self.assets.light_probes(&volume.bake)?;
                Some((volume.id, bake.fingerprint))
            })
            .collect();
        let stale = if baked.is_empty() {
            Vec::new()
        } else {
            let now = axcenity_scene::light_bake_fingerprint(&self.world);
            baked
                .into_iter()
                .filter(|&(_, fingerprint)| fingerprint != now)
                .map(|(id, _)| id)
                .collect()
        };
        self.stale_light_probes = Some((revision, stale.clone()));
        stale
    }
}

/// Where `probe` is baked: its own file again (if `is_bake` says its
/// `Bake` is one and no other probe here took it), or a new file named
/// after its part (`path_of` a free name; `fallback` if the part's name
/// can't be a file name).
fn bake_path(
    folder: &Path,
    probe: &ToBake,
    used: &HashSet<String>,
    is_bake: fn(&str) -> bool,
    path_of: fn(&str) -> String,
    fallback: &str,
) -> String {
    if is_bake(&probe.bake) && !used.contains(&probe.bake) {
        return probe.bake.clone();
    }
    let stem =
        axcenity_assets::safe_file_name(&probe.part_name).unwrap_or_else(|| fallback.to_owned());
    let name = axcenity_assets::unique_file_name(&stem, |name| {
        let path = path_of(name);
        used.contains(&path) || folder.join(&path).exists()
    });
    path_of(&name)
}

/// The probes of `volume` standing at `points` inside a solid: a colliding
/// part (not the volume's own) or under a Terrain's ground (roadmap R18.2).
fn invalid_probes(
    world: &World,
    assets: &Assets,
    volume: InstanceId,
    points: &[axcenity_render::glam::Vec3],
) -> Vec<u32> {
    let own: Vec<InstanceId> = world
        .get(volume)
        .and_then(|v| v.parent())
        .into_iter()
        .collect();
    let points: Vec<Vector3> = points
        .iter()
        .map(|point| Vector3::new(point.x, point.y, point.z))
        .collect();
    axcenity_physics::inside_solids(world, assets, &points, &own)
        .into_iter()
        .zip(0u32..)
        .filter_map(|(inside, index)| inside.then_some(index))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// R18.2: a wall through a volume makes the probes inside it invalid;
    /// the volume's own part, colliding or not, doesn't.
    #[test]
    fn probes_inside_a_wall_are_invalid_and_the_volumes_own_part_is_not() {
        let mut world = World::new();
        let workspace = world.workspace();
        let room = world.create(ClassName::Part).unwrap();
        world.set_parent(room, Some(workspace)).unwrap();
        let set = |world: &mut World, id, name: &str, value| {
            world.set_property(id, name, value).unwrap();
        };
        set(
            &mut world,
            room,
            "Size",
            Value::Vector3(Vector3::new(12.0, 4.0, 4.0)),
        );
        set(
            &mut world,
            room,
            "Position",
            Value::Vector3(Vector3::new(0.0, 2.0, 0.0)),
        );
        set(&mut world, room, "Anchored", Value::Bool(true));
        let volume = world.create(ClassName::LightProbeVolume).unwrap();
        world.set_parent(volume, Some(room)).unwrap();
        // Probes 4 apart: three along X, at -4, 0 and 4.
        let wall = world.create(ClassName::Part).unwrap();
        world.set_parent(wall, Some(workspace)).unwrap();
        set(
            &mut world,
            wall,
            "Size",
            Value::Vector3(Vector3::new(1.0, 10.0, 10.0)),
        );
        set(
            &mut world,
            wall,
            "Position",
            Value::Vector3(Vector3::new(0.0, 2.0, 0.0)),
        );
        set(&mut world, wall, "Anchored", Value::Bool(true));
        let grid = axcenity_scene::light_probe_points(&world, volume).unwrap();
        assert_eq!(grid.counts, [3, 1, 1]);
        let assets = Assets::default();
        assert_eq!(invalid_probes(&world, &assets, volume, &grid.points), [1]);
        set(&mut world, wall, "CanCollide", Value::Bool(false));
        assert!(invalid_probes(&world, &assets, volume, &grid.points).is_empty());
    }

    fn test_device() -> Option<(wgpu::Device, wgpu::Queue)> {
        let instance = axcenity_render::adapters::instance();
        let adapter =
            pollster::block_on(axcenity_render::adapters::adapter(&instance, None)).ok()?;
        pollster::block_on(adapter.request_device(&Default::default())).ok()
    }

    /// `hello-graphics` copied to a temporary folder, so bakes don't land
    /// in the example.
    fn scratch_copy(name: &str) -> std::path::PathBuf {
        let folder = std::env::temp_dir().join(format!(
            "axcenity-editor-probes-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&folder);
        let source = super::super::tests::example("hello-graphics");
        for entry in walk(&source) {
            let to = folder.join(entry.strip_prefix(&source).unwrap());
            std::fs::create_dir_all(to.parent().unwrap()).unwrap();
            std::fs::copy(&entry, &to).unwrap();
        }
        folder
    }

    fn walk(folder: &std::path::Path) -> Vec<std::path::PathBuf> {
        let mut files = Vec::new();
        for entry in std::fs::read_dir(folder).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() {
                files.extend(walk(&path));
            } else {
                files.push(path);
            }
        }
        files
    }

    fn bake_of(world: &World, probe: InstanceId) -> String {
        world
            .get(probe)
            .and_then(|probe| probe.property("Bake"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned()
    }

    #[test]
    fn a_probe_with_no_part_gets_an_invisible_box_and_bakes_into_assets() {
        let Some((device, queue)) = test_device() else {
            eprintln!("skipped: no graphics adapter on this machine");
            return;
        };
        let folder = scratch_copy("bake");
        let mut app = EditorApp::new(Some(&folder));
        app.selected = None;
        app.insert(Insertable::ReflectionProbe, None);
        let probe = app.selected.expect("the probe is selected");
        let holder = app.world.get(probe).and_then(|p| p.parent()).unwrap();
        let holder = app.world.get(holder).unwrap();
        assert_eq!(holder.class(), ClassName::Part);
        assert_eq!(
            holder.property("Transparency").and_then(Value::as_number),
            Some(1.0)
        );

        app.bake_probes(&device, &queue);
        let path = bake_of(&app.world, probe);
        assert_eq!(path, "assets/probes/ReflectionProbeBox.probe.hdr");
        let image = axcenity_assets::read_probe(&folder, &path).unwrap();
        assert_eq!(image.size, PROBE_BAKE_SIZE);
        assert!(app.assets.reflection_probe(&path).is_some());

        // Baking again keeps the file; undo takes the name back, not the file.
        app.bake_probes(&device, &queue);
        assert_eq!(bake_of(&app.world, probe), path);
        app.undo();
        assert_eq!(bake_of(&app.world, probe), "");
        assert!(folder.join(&path).exists());
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn a_baked_probe_saves_reloads_and_is_packed_into_the_game() {
        let Some((device, queue)) = test_device() else {
            eprintln!("skipped: no graphics adapter on this machine");
            return;
        };
        let folder = scratch_copy("save");
        let mut app = EditorApp::new(Some(&folder));
        app.selected = None;
        app.insert(Insertable::ReflectionProbe, None);
        app.bake_probes(&device, &queue);
        let path = axcenity_assets::probe_path("ReflectionProbeBox");
        assert!(app.save());

        // Reopened, the world still names the bake, and the scene hands it
        // to the renderer.
        let reopened = EditorApp::new(Some(&folder));
        let probe = reopened
            .world
            .descendants(reopened.world.workspace())
            .into_iter()
            .find(|&id| {
                reopened.world.get(id).map(|i| i.class()) == Some(ClassName::ReflectionProbe)
            })
            .expect("the probe was saved");
        assert_eq!(bake_of(&reopened.world, probe), path);
        let list = axcenity_scene::build_draw_list(
            &reopened.world,
            &axcenity_assets::Assets::new(&folder),
            axcenity_render::Camera {
                position: axcenity_render::glam::Vec3::ZERO,
                look_at: axcenity_render::glam::Vec3::NEG_Z,
                fov_y_degrees: 70.0,
            },
        );
        assert_eq!(list.reflection_probes.len(), 1);
        assert_eq!(list.reflection_probes[0].image.size, PROBE_BAKE_SIZE);

        // A built game carries the bake.
        let packed = axcenity_project::pack_game(&folder).unwrap();
        let unpacked = folder.join("unpacked");
        axcenity_project::unpack_game(&packed, &unpacked).unwrap();
        assert!(axcenity_assets::read_probe(&unpacked, &path).is_ok());
        let _ = std::fs::remove_dir_all(&folder);
    }

    /// The LightProbeVolume in `app`'s Workspace.
    fn light_probe_volume(app: &EditorApp) -> InstanceId {
        app.world
            .descendants(app.world.workspace())
            .into_iter()
            .find(|&id| app.world.get(id).map(|i| i.class()) == Some(ClassName::LightProbeVolume))
            .expect("a LightProbeVolume")
    }

    fn flagged(app: &mut EditorApp, volume: InstanceId) -> bool {
        app.update_warnings();
        app.explorer.warnings.contains_key(&volume)
    }

    #[test]
    fn a_light_probe_volume_bakes_repeatably_and_is_flagged_once_the_world_changes() {
        let Some((device, queue)) = test_device() else {
            eprintln!("skipped: no graphics adapter on this machine");
            return;
        };
        let folder = scratch_copy("light");
        let mut app = EditorApp::new(Some(&folder));
        app.selected = None;
        app.insert(Insertable::LightProbeVolume, None);
        let volume = app.selected.expect("the volume is selected");
        let holder = app.world.get(volume).and_then(|v| v.parent()).unwrap();
        let holder = app.world.get(holder).unwrap();
        assert_eq!(holder.name(), "LightProbeBox");
        assert_eq!(
            holder.property("Transparency").and_then(Value::as_number),
            Some(1.0)
        );
        assert!(!flagged(&mut app, volume), "not baked: nothing to flag");

        app.bake_probes(&device, &queue);
        let path = bake_of(&app.world, volume);
        assert_eq!(path, "assets/probes/LightProbeBox.lightprobes.json");
        let first = axcenity_assets::read_light_probes(&folder, &path).unwrap();
        // 16 × 10 × 16 studs, a probe about every 4.
        assert_eq!(first.counts, [4, 3, 4]);
        assert!(app.assets.light_probes(&path).is_some());
        assert!(!flagged(&mut app, volume), "just baked");

        // Baking the same world again gives the same light, in the same file.
        app.bake_probes(&device, &queue);
        assert_eq!(bake_of(&app.world, volume), path);
        let again = axcenity_assets::read_light_probes(&folder, &path).unwrap();
        assert_eq!(again.fingerprint, first.fingerprint);
        for (a, b) in first
            .probes
            .iter()
            .flatten()
            .flatten()
            .zip(again.probes.iter().flatten().flatten())
        {
            assert!((a - b).abs() < 1e-4, "{a} then {b}");
        }

        // A new part changes the world, so the bake is out of date; undoing
        // it puts the world back as it was baked.
        app.selected = None;
        app.insert(Insertable::Part(PartType::Block), None);
        app.history.seal();
        assert!(flagged(&mut app, volume), "the world changed");
        assert!(
            app.explorer.warnings[&volume].contains("Bake Probes"),
            "{}",
            app.explorer.warnings[&volume]
        );
        app.undo();
        assert!(!flagged(&mut app, volume), "back as baked");
        app.redo();
        assert!(flagged(&mut app, volume));
        app.bake_probes(&device, &queue);
        assert!(!flagged(&mut app, volume), "baked again");
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn a_baked_light_probe_volume_saves_reopens_up_to_date_and_is_packed_into_the_game() {
        let Some((device, queue)) = test_device() else {
            eprintln!("skipped: no graphics adapter on this machine");
            return;
        };
        let folder = scratch_copy("light-save");
        let mut app = EditorApp::new(Some(&folder));
        app.selected = None;
        app.insert(Insertable::LightProbeVolume, None);
        app.bake_probes(&device, &queue);
        let path = axcenity_assets::light_probes_path("LightProbeBox");
        assert!(app.save());

        // Reopened, the bake is still up to date (the fingerprint survives
        // saving), and the scene hands it to the renderer.
        let mut reopened = EditorApp::new(Some(&folder));
        let volume = light_probe_volume(&reopened);
        assert_eq!(bake_of(&reopened.world, volume), path);
        assert!(!flagged(&mut reopened, volume), "saved as baked");
        let list = axcenity_scene::build_draw_list(
            &reopened.world,
            &axcenity_assets::Assets::new(&folder),
            axcenity_render::Camera {
                position: axcenity_render::glam::Vec3::ZERO,
                look_at: axcenity_render::glam::Vec3::NEG_Z,
                fov_y_degrees: 70.0,
            },
        );
        assert_eq!(list.light_probe_volumes.len(), 1);
        assert_eq!(list.light_probe_volumes[0].probes.counts, [4, 3, 4]);

        // A built game carries the bake.
        let packed = axcenity_project::pack_game(&folder).unwrap();
        let unpacked = folder.join("unpacked");
        axcenity_project::unpack_game(&packed, &unpacked).unwrap();
        assert!(axcenity_assets::read_light_probes(&unpacked, &path).is_ok());
        let _ = std::fs::remove_dir_all(&folder);
    }
}
