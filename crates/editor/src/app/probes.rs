//! Baking reflection probes (Bake Probes on the ribbon): each
//! ReflectionProbe's view from the middle of its part is drawn on the GPU,
//! saved in `assets/probes`, and named by its `Bake`, all one undo step.

use std::collections::HashSet;

use axcenity_render::{PROBE_BAKE_SIZE, TextureRenderer, wgpu};

use super::*;

impl EditorApp {
    /// Bakes every ReflectionProbe inside a part in the Workspace, as it
    /// is now, on `device`. A probe keeps its bake file if it has one in
    /// `assets/probes`; otherwise one is named after its part. The
    /// probes' new `Bake` values are one undo step (the files stay).
    pub(super) fn bake_probes(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) {
        if self.playing() {
            return;
        }
        let Some(folder) = self.project.as_ref().map(|p| p.folder.clone()) else {
            return;
        };
        let probes: Vec<(InstanceId, Vector3, String, String)> = self
            .world
            .descendants(self.world.workspace())
            .into_iter()
            .filter_map(|id| {
                let probe = self.world.get(id)?;
                if probe.class() != ClassName::ReflectionProbe {
                    return None;
                }
                let part = self.world.get(probe.parent()?)?;
                if !matches!(part.class(), ClassName::Part | ClassName::MeshPart) {
                    return None;
                }
                let center = part.property("Position")?.as_vector3()?;
                let bake = probe
                    .property("Bake")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned();
                Some((id, center, part.name().to_owned(), bake))
            })
            .collect();
        if probes.is_empty() {
            self.output.warning(
                "There are no ReflectionProbes in parts to bake. Insert one with \
                 Reflection Probe on the ribbon.",
            );
            return;
        }
        if self.assets.texture_progress().is_some() {
            self.output.warning(
                "Some textures are still loading, so the bakes are missing them. \
                 Bake again once they've loaded.",
            );
        }
        let list = axcenity_scene::build_draw_list(
            &self.world,
            &self.assets,
            axcenity_render::Camera {
                position: axcenity_render::glam::Vec3::ZERO,
                look_at: axcenity_render::glam::Vec3::NEG_Z,
                fov_y_degrees: 90.0,
            },
        );
        let mut used = HashSet::new();
        let mut baked = 0;
        for (id, center, part_name, bake) in probes {
            // Its own file again, unless another probe here already took it.
            let path = if axcenity_assets::is_probe_file(&bake) && !used.contains(&bake) {
                bake.clone()
            } else {
                let stem = axcenity_assets::safe_file_name(&part_name)
                    .unwrap_or_else(|| "ReflectionProbe".to_owned());
                let name = axcenity_assets::unique_file_name(&stem, |name| {
                    let path = axcenity_assets::probe_path(name);
                    used.contains(&path) || folder.join(&path).exists()
                });
                axcenity_assets::probe_path(&name)
            };
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
                    self.output
                        .error(format!("Could not bake the probe in {part_name}: {err:#}"));
                    continue;
                }
            };
            if let Err(err) = axcenity_assets::write_probe(&folder, &path, PROBE_BAKE_SIZE, &rgb) {
                self.output
                    .error(format!("Could not save the bake `{path}`: {err}"));
                continue;
            }
            if let Some(image) = axcenity_assets::ProbeImage::new(PROBE_BAKE_SIZE, rgb) {
                self.assets.put_reflection_probe(&path, image);
            }
            if bake != path {
                self.apply(id, Edit::Property("Bake", Value::String(path.clone())));
            }
            used.insert(path);
            baked += 1;
        }
        self.history.seal();
        if baked > 0 {
            let probes = if baked == 1 { "probe" } else { "probes" };
            self.toasts
                .success(format!("Baked {baked} reflection {probes}"));
            self.output.info(format!(
                "Baked {baked} reflection {probes} into {}.",
                axcenity_assets::PROBES_FOLDER
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_device() -> Option<(wgpu::Device, wgpu::Queue)> {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
        let adapter = pollster::block_on(instance.request_adapter(&Default::default())).ok()?;
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
}
