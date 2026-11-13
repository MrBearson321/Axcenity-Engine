//! Bake Probes, a little each frame. A big bake (many light probe volumes,
//! up to 4,096 probes each, every probe six small pictures of the whole
//! scene) can take minutes, so the editor bakes for at most
//! [`FRAME_BUDGET`] a frame and draws in between. A progress window counts
//! the probes, says about how long is left and can stop the bake: volumes
//! already finished are kept, the one part way through is left as it was.
//!
//! The bake draws the world as it was when it started (its draw list and
//! fingerprint are taken then), so edits made meanwhile show the volumes as
//! out of date afterward. Files are written as each volume or reflection
//! probe finishes; the new `Bake` names are one undo step at the end. It
//! waits while Play runs, since `apply` would change the game then.

use std::collections::{HashSet, VecDeque};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use axcenity_render::glam::Vec3;
use axcenity_render::{
    LIGHT_PROBE_BAKE_SIZE, LightProbeBaker, PROBE_BAKE_SIZE, TextureRenderer, wgpu,
};

use super::probes::{ToBake, bake_path};
use super::*;

/// How long one frame may bake before the editor draws again. Each step
/// bakes at least one probe, so a very heavy scene still moves.
pub(super) const FRAME_BUDGET: Duration = Duration::from_millis(30);

/// From this many light probes, starting a bake says in Output that it may
/// take a while.
const BIG_BAKE_PROBES: usize = 1000;

/// A guess at how much longer a reflection probe (128² faces) takes than a
/// light probe (16² faces) until one has been timed.
const REFLECTION_GUESS: f64 = 4.0;

/// A LightProbeVolume being baked: where its probes stand and the light
/// baked so far.
struct VolumeBake {
    probe: ToBake,
    counts: [u32; 3],
    points: Vec<Vec3>,
    path: String,
    light: Vec<[[f32; 3]; 4]>,
}

/// A ReflectionProbe waiting to be baked.
struct ReflectionBake {
    probe: ToBake,
    center: Vec3,
    path: String,
}

/// A bake in progress.
pub(super) struct ProbeBake {
    folder: PathBuf,
    /// The world as it was when the bake started, as bakes see it.
    list: axcenity_render::DrawList,
    fingerprint: u64,
    volumes: VecDeque<VolumeBake>,
    reflections: VecDeque<ReflectionBake>,
    /// Kept between the light probes, so the scene is uploaded once.
    baker: Option<LightProbeBaker>,
    /// The `Bake` each finished probe gets, applied as one undo step.
    bakes: Vec<(InstanceId, String)>,
    volume_count: usize,
    volumes_baked: usize,
    pub(super) light_total: usize,
    pub(super) light_done: usize,
    reflection_total: usize,
    reflections_baked: usize,
    /// How many of `bakes` are reflection probes.
    reflections_saved: usize,
    /// Time spent drawing light probes and reflection probes, for the
    /// estimate of what's left (the scene's upload isn't counted).
    light_seconds: f64,
    reflection_seconds: f64,
    /// Asked to stop: finishes at the next step outside Play.
    pub(super) cancel: bool,
    /// The progress window is showing (its × hides it; Bake Probes shows
    /// it again).
    pub(super) window_open: bool,
}

impl ProbeBake {
    fn done(&self) -> bool {
        self.volumes.is_empty() && self.reflections.is_empty()
    }

    /// The share of the work done, 0 to 1.
    pub(super) fn fraction(&self) -> f32 {
        let total = self.light_total + self.reflection_total;
        if total == 0 {
            return 1.0;
        }
        (self.light_done + self.reflections_baked) as f32 / total as f32
    }

    /// About how many seconds are left, once a few probes have been timed.
    pub(super) fn seconds_left(&self) -> Option<f64> {
        let per_light = (self.light_done >= 4).then(|| self.light_seconds / self.light_done as f64);
        let per_reflection = if self.reflections_baked > 0 {
            Some(self.reflection_seconds / self.reflections_baked as f64)
        } else {
            per_light.map(|seconds| seconds * REFLECTION_GUESS)
        };
        let lights_left = self.light_total - self.light_done;
        let reflections_left = self.reflection_total - self.reflections_baked;
        let lights = if lights_left == 0 {
            0.0
        } else {
            per_light? * lights_left as f64
        };
        let reflections = if reflections_left == 0 {
            0.0
        } else {
            per_reflection? * reflections_left as f64
        };
        Some(lights + reflections)
    }

    /// What's being baked now, such as "Light probes in Hall (2 of 5
    /// volumes)".
    pub(super) fn now(&self) -> String {
        if let Some(volume) = self.volumes.front() {
            format!(
                "Light probes in {} ({} of {} volumes)",
                volume.probe.part_name,
                self.volumes_baked + 1,
                self.volume_count
            )
        } else if let Some(reflection) = self.reflections.front() {
            format!(
                "Reflection probe in {} ({} of {})",
                reflection.probe.part_name,
                self.reflections_baked + 1,
                self.reflection_total
            )
        } else {
            "Finishing".to_owned()
        }
    }
}

/// `seconds` as "about 2 min left" or "less than a minute left".
pub(super) fn time_left(seconds: f64) -> String {
    let minutes = (seconds / 60.0).round();
    if seconds < 45.0 {
        "less than a minute left".to_owned()
    } else if minutes < 90.0 {
        format!("about {minutes} min left")
    } else {
        format!("about {:.1} hours left", seconds / 3600.0)
    }
}

impl EditorApp {
    /// Starts baking every LightProbeVolume, then every ReflectionProbe (so
    /// their reflections show the volumes' light), inside a part in the
    /// Workspace, as it is now. A probe keeps its bake file if it has one
    /// in `assets/probes`; otherwise one is named after its part. A bake
    /// already running just shows its window again.
    pub(super) fn start_probe_bake(&mut self) {
        if self.playing() {
            return;
        }
        if let Some(bake) = &mut self.probe_bake {
            bake.window_open = true;
            return;
        }
        let Some(folder) = self.project.as_ref().map(|p| p.folder.clone()) else {
            return;
        };
        let volumes = self.probes_to_bake(ClassName::LightProbeVolume);
        let reflections = self.probes_to_bake(ClassName::ReflectionProbe);
        if volumes.is_empty() && reflections.is_empty() {
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
        let volumes: VecDeque<VolumeBake> = volumes
            .into_iter()
            .filter_map(|probe| {
                let grid = axcenity_scene::light_probe_points(&self.world, probe.id)?;
                let path = bake_path(
                    &folder,
                    &probe,
                    &used,
                    axcenity_assets::is_light_probes_file,
                    axcenity_assets::light_probes_path,
                    "LightProbeVolume",
                );
                used.insert(path.clone());
                Some(VolumeBake {
                    probe,
                    counts: grid.counts,
                    light: Vec::with_capacity(grid.points.len()),
                    points: grid.points,
                    path,
                })
            })
            .collect();
        let reflections: VecDeque<ReflectionBake> = reflections
            .into_iter()
            .filter_map(|probe| {
                let center = self
                    .world
                    .get(probe.id)
                    .and_then(|instance| self.world.get(instance.parent()?))
                    .and_then(|part| part.property("Position")?.as_vector3())?;
                let path = bake_path(
                    &folder,
                    &probe,
                    &used,
                    axcenity_assets::is_probe_file,
                    axcenity_assets::probe_path,
                    "ReflectionProbe",
                );
                used.insert(path.clone());
                Some(ReflectionBake {
                    probe,
                    center: Vec3::new(center.x, center.y, center.z),
                    path,
                })
            })
            .collect();
        let light_total: usize = volumes.iter().map(|volume| volume.points.len()).sum();
        if light_total >= BIG_BAKE_PROBES {
            let count = volumes.len();
            let s = if count == 1 { "" } else { "s" };
            self.output.warning(format!(
                "Baking {light_total} light probes in {count} volume{s}. This can take \
                 several minutes; the editor stays usable, and the Baking Probes window \
                 shows how long is left and can stop it. Fewer probes bake faster: raise \
                 a volume's Spacing."
            ));
        }
        self.probe_bake = Some(ProbeBake {
            folder,
            list: self.bake_scene(),
            fingerprint: axcenity_scene::light_bake_fingerprint(&self.world),
            volume_count: volumes.len(),
            reflection_total: reflections.len(),
            volumes,
            reflections,
            baker: None,
            bakes: Vec::new(),
            volumes_baked: 0,
            light_total,
            light_done: 0,
            reflections_baked: 0,
            reflections_saved: 0,
            light_seconds: 0.0,
            reflection_seconds: 0.0,
            cancel: false,
            window_open: true,
        });
    }

    /// Bakes for about `budget` (at least one probe), and finishes the bake
    /// when it's done or was asked to stop. Waits during Play.
    pub(super) fn step_probe_bake(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        budget: Duration,
    ) {
        if self.playing() {
            return;
        }
        let Some(mut bake) = self.probe_bake.take() else {
            return;
        };
        // Another project was opened: its Instances aren't this bake's.
        if self.project.as_ref().map(|p| &p.folder) != Some(&bake.folder) {
            return;
        }
        let start = Instant::now();
        while !bake.cancel && !bake.done() {
            self.bake_next(&mut bake, device, queue);
            if start.elapsed() >= budget {
                break;
            }
        }
        if bake.cancel || bake.done() {
            self.finish_probe_bake(bake);
        } else {
            self.probe_bake = Some(bake);
        }
    }

    /// Bakes one light probe or one reflection probe, saving a volume or
    /// reflection probe once it's finished.
    fn bake_next(&mut self, bake: &mut ProbeBake, device: &wgpu::Device, queue: &wgpu::Queue) {
        if let Some(volume) = bake.volumes.front_mut() {
            let baker = bake.baker.get_or_insert_with(|| {
                LightProbeBaker::new(
                    device,
                    queue,
                    &bake.list,
                    LIGHT_PROBE_BAKE_SIZE,
                    &self.graphics,
                )
            });
            let started = Instant::now();
            let point = volume.points[volume.light.len()];
            match baker.bake(point) {
                Ok(light) => {
                    volume.light.push(light);
                    bake.light_done += 1;
                    bake.light_seconds += started.elapsed().as_secs_f64();
                }
                Err(err) => {
                    self.output.error(format!(
                        "Could not bake the light probes in {}: {err:#}",
                        volume.probe.part_name
                    ));
                    bake.light_done += volume.points.len() - volume.light.len();
                    bake.volumes.pop_front();
                    bake.volumes_baked += 1;
                    return;
                }
            }
            if volume.light.len() == volume.points.len() {
                let volume = bake.volumes.pop_front().expect("the front volume");
                bake.volumes_baked += 1;
                self.save_light_probes(bake, volume);
            }
            if bake.volumes.is_empty() {
                // Frees the bake's copy of the scene on the GPU.
                bake.baker = None;
            }
            return;
        }
        let Some(reflection) = bake.reflections.pop_front() else {
            return;
        };
        let started = Instant::now();
        let result = TextureRenderer::bake_reflection_probe(
            device,
            queue,
            &bake.list,
            reflection.center,
            PROBE_BAKE_SIZE,
            &self.graphics,
        );
        bake.reflection_seconds += started.elapsed().as_secs_f64();
        bake.reflections_baked += 1;
        let part_name = &reflection.probe.part_name;
        let path = &reflection.path;
        let rgb = match result {
            Ok(rgb) => rgb,
            Err(err) => {
                self.output
                    .error(format!("Could not bake the probe in {part_name}: {err:#}"));
                return;
            }
        };
        if let Err(err) = axcenity_assets::write_probe(&bake.folder, path, PROBE_BAKE_SIZE, &rgb) {
            self.output
                .error(format!("Could not save the bake `{path}`: {err}"));
            return;
        }
        if let Some(image) = axcenity_assets::ProbeImage::new(PROBE_BAKE_SIZE, rgb) {
            self.assets.put_reflection_probe(path, image);
        }
        bake.bakes.push((reflection.probe.id, path.clone()));
        bake.reflections_saved += 1;
    }

    /// Saves a finished volume's bake and hands it to the viewport.
    fn save_light_probes(&mut self, bake: &mut ProbeBake, volume: VolumeBake) {
        let part_name = &volume.probe.part_name;
        let path = &volume.path;
        let Some(probes) =
            axcenity_assets::LightProbes::new(volume.counts, bake.fingerprint, volume.light)
        else {
            self.output.error(format!(
                "Could not bake the light probes in {part_name}: the light came back \
                 as something that isn't a number."
            ));
            return;
        };
        if let Err(err) = axcenity_assets::write_light_probes(&bake.folder, path, &probes) {
            self.output
                .error(format!("Could not save the bake `{path}`: {err}"));
            return;
        }
        self.assets.put_light_probes(path, probes);
        bake.bakes.push((volume.probe.id, path.clone()));
    }

    /// Names the finished bakes in their probes' `Bake`, one undo step, and
    /// says what was baked.
    fn finish_probe_bake(&mut self, bake: ProbeBake) {
        self.history.seal();
        for (id, path) in &bake.bakes {
            let current = self
                .world
                .get(*id)
                .and_then(|probe| probe.property("Bake"))
                .and_then(Value::as_str)
                .map(str::to_owned);
            // Deleted meanwhile, or already named so (baking into the same
            // files changes nothing, so no step).
            if current.is_some_and(|current| current != *path) {
                self.apply(*id, Edit::Property("Bake", Value::String(path.clone())));
            }
        }
        self.history.seal();
        self.stale_light_probes = None;
        let reflections = bake.reflections_saved;
        let volumes = bake.bakes.len() - reflections;
        let mut baked = Vec::new();
        if volumes > 0 {
            let s = if volumes == 1 { "" } else { "s" };
            baked.push(format!("{volumes} light probe volume{s}"));
        }
        if reflections > 0 {
            let s = if reflections == 1 { "" } else { "s" };
            baked.push(format!("{reflections} reflection probe{s}"));
        }
        let baked = baked.join(" and ");
        if bake.cancel {
            let kept = if baked.is_empty() {
                "nothing was finished".to_owned()
            } else {
                format!("kept {baked}")
            };
            self.toasts.success(format!("Stopped baking: {kept}"));
            self.output.info(format!(
                "Stopped baking probes: {kept}. The rest keep their old bakes."
            ));
        } else if !baked.is_empty() {
            self.toasts.success(format!("Baked {baked}"));
            self.output.info(format!(
                "Baked {baked} into {}.",
                axcenity_assets::PROBES_FOLDER
            ));
        }
    }

    /// The Baking Probes window: what's being baked, a progress bar, about
    /// how long is left, and Cancel.
    pub(super) fn probe_bake_window(&mut self, ctx: &egui::Context) {
        let playing = self.playing();
        let Some(bake) = &mut self.probe_bake else {
            return;
        };
        // Keeps frames coming, so the bake goes on without input.
        ctx.request_repaint();
        let palette = theme::palette(ctx);
        let mut open = bake.window_open;
        widgets::tool_window(icons::LIGHTBULB, "Baking Probes")
            .fit(380.0)
            .show(ctx, &mut open, |ui| {
                ui.label(bake.now());
                let mut text = if bake.light_total > 0 {
                    format!("{} of {} light probes", bake.light_done, bake.light_total)
                } else {
                    format!(
                        "{} of {} reflection probes",
                        bake.reflections_baked, bake.reflection_total
                    )
                };
                match bake.seconds_left() {
                    Some(seconds) => text.push_str(&format!(" · {}", time_left(seconds))),
                    None => text.push_str(" · timing the first probes"),
                }
                ui.add(
                    egui::ProgressBar::new(bake.fraction())
                        .text(text)
                        .desired_width(ui.available_width()),
                );
                let note = if bake.cancel {
                    "Stopping after Play ends."
                } else if playing {
                    "Paused while Play runs; it goes on when you stop."
                } else {
                    "Big bakes can take several minutes. You can keep working: the bake \
                     uses the world as it was when it started. Cancel keeps the volumes \
                     already finished."
                };
                ui.label(egui::RichText::new(note).color(palette.text_weak));
                ui.add_space(theme::space::S);
                let cancel =
                    widgets::button(widgets::Role::Secondary, Some(icons::X), "Cancel", &palette);
                if ui.add_enabled(!bake.cancel, cancel).clicked() {
                    bake.cancel = true;
                }
            });
        bake.window_open = open;
    }

    /// Bakes everything now, start to end (tests; the editor bakes a little
    /// each frame).
    #[cfg(test)]
    pub(super) fn bake_probes(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) {
        self.start_probe_bake();
        while self.probe_bake.is_some() {
            self.step_probe_bake(device, queue, Duration::MAX);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn time_left_reads_in_minutes_or_hours() {
        assert_eq!(time_left(10.0), "less than a minute left");
        assert_eq!(time_left(130.0), "about 2 min left");
        assert_eq!(time_left(3.0 * 3600.0), "about 3.0 hours left");
    }
}
