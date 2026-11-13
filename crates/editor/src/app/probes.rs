//! Baking probes (Bake Probes on the ribbon): each LightProbeVolume's grid
//! of probes and each ReflectionProbe's view from the middle of its part is
//! drawn on the GPU, saved in `assets/probes`, and named by its `Bake`, all
//! one undo step. The bake itself runs a little each frame
//! (`app/probe_bake.rs`); this file finds what to bake and where. A light
//! probe bake keeps a fingerprint of the world, so a volume whose bake is
//! out of date gets a warning in the Explorer.

use std::collections::HashSet;
use std::path::Path;

use super::*;

/// A probe to bake: the Instance, its part's name and its `Bake` now.
pub(super) struct ToBake {
    pub(super) id: InstanceId,
    pub(super) part_name: String,
    pub(super) bake: String,
}

impl EditorApp {
    /// Every Instance of `class` inside a Part or MeshPart in the Workspace.
    pub(super) fn probes_to_bake(&self, class: ClassName) -> Vec<ToBake> {
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

    /// The world as bakes see it (each bake places its own cameras).
    pub(super) fn bake_scene(&self) -> axcenity_render::DrawList {
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
pub(super) fn bake_path(
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

#[cfg(test)]
mod tests {
    use axcenity_render::{PROBE_BAKE_SIZE, wgpu};

    use super::*;

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

    /// The bake that froze the editor: it now runs a probe or so a frame,
    /// counts its progress, and Cancel keeps the volumes already finished
    /// and leaves the one part way through as it was.
    #[test]
    fn a_bake_runs_a_little_each_frame_and_cancel_keeps_finished_volumes() {
        let Some((device, queue)) = test_device() else {
            eprintln!("skipped: no graphics adapter on this machine");
            return;
        };
        let folder = scratch_copy("light-steps");
        let mut app = EditorApp::new(Some(&folder));
        app.selected = None;
        app.insert(Insertable::LightProbeVolume, None);
        let first = app.selected.expect("the first volume");
        app.selected = None;
        app.insert(Insertable::LightProbeVolume, None);
        let second = app.selected.expect("the second volume");
        app.history.seal();
        let revision = app.history.revision();

        app.start_probe_bake();
        let bake = app.probe_bake.as_ref().expect("started");
        assert_eq!(
            (bake.light_done, bake.light_total),
            (0, 96),
            "two 4×3×4 grids"
        );
        assert!(bake.seconds_left().is_none(), "nothing timed yet");
        // A zero budget still bakes one probe a step.
        for done in 1..=50 {
            app.step_probe_bake(&device, &queue, std::time::Duration::ZERO);
            let bake = app.probe_bake.as_ref().expect("still baking");
            assert_eq!(bake.light_done, done);
        }
        let bake = app.probe_bake.as_ref().unwrap();
        assert!(bake.seconds_left().is_some_and(|seconds| seconds > 0.0));
        assert!((bake.fraction() - 50.0 / 96.0).abs() < 1e-6);
        assert!(bake.now().contains("2 of 2 volumes"), "{}", bake.now());
        // The first volume's file is written; nothing is named until the end.
        let path = axcenity_assets::light_probes_path("LightProbeBox");
        assert!(folder.join(&path).exists());
        assert_eq!(bake_of(&app.world, first), "");

        app.probe_bake.as_mut().unwrap().cancel = true;
        app.step_probe_bake(&device, &queue, std::time::Duration::ZERO);
        assert!(app.probe_bake.is_none(), "stopped");
        assert_eq!(
            bake_of(&app.world, first),
            path,
            "the finished volume is kept"
        );
        assert_eq!(bake_of(&app.world, second), "", "the half-baked one isn't");
        // One undo step takes the name back.
        assert_ne!(app.history.revision(), revision);
        app.undo();
        assert_eq!(bake_of(&app.world, first), "");
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
