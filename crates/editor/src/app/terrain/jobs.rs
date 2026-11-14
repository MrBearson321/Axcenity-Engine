use super::*;

impl EditorApp {
    /// Starts generating a landscape for the terrain being edited, to preview.
    pub(in crate::app) fn start_generate(&mut self) {
        let Some(terrain) = self.terrain.target else {
            return;
        };
        if let Some(running) = &self.terrain.job {
            // Started again once the old one stops.
            running.job.cancel();
            self.terrain.queued = Some(false);
            return;
        }
        let Some(placement) = self.world.get(terrain).and_then(Placement::of) else {
            return;
        };
        let new = self.terrain.new_terrain;
        let size_x = new.samples;
        let size_z = samples_along(size_x, new.width, new.length);
        let target = Target {
            size_x,
            size_z,
            world_size: Vec2::new(new.width, new.length),
        };
        let mut settings = self.terrain.generator.clone();
        let generator = match generate::check(&settings, target) {
            Ok(generator) => generator,
            Err(err) => {
                self.output
                    .error(format!("Could not generate the landscape: {err}"));
                return;
            }
        };
        settings.algorithm = generator.name().to_owned();
        settings.version = generator.version();
        let size = Vector3::new(new.width, new.height, new.length);
        let combine = self.terrain.combine;
        let current = self.world.terrain(terrain).cloned();
        let old_range = placement.size.y;
        let title = format!("Generate {}", settings.preset.label());
        let job = Job::spawn(move |control| {
            let heights = generator.generate(&settings, target, control)?;
            let mut data = match (combine, current) {
                (Combine::Add, Some(current)) => {
                    let mut data = current.resampled(size_x, size_z).ok()?;
                    // The new hills and dips, on top of the old ground in studs.
                    let base = settings.base_height;
                    let all = data.bounds();
                    for (old, new) in data.heights_mut_in(all).iter_mut().zip(&heights) {
                        *old = (*old * old_range + (new - base) * size.y) / size.y;
                    }
                    data.clamp_heights_in(all);
                    data.generation = None;
                    data
                }
                _ => {
                    let mut data = TerrainData::flat(size_x, size_z, 0.0).ok()?;
                    let all = data.bounds();
                    data.heights_mut_in(all).copy_from_slice(&heights);
                    data.clamp_heights_in(all);
                    data.generation = Some(settings.to_json());
                    data
                }
            };
            data.clamp_heights_in(data.bounds());
            Some(JobResult {
                data,
                size: Some(size),
            })
        });
        self.terrain.job = Some(RunningJob {
            terrain,
            label: "Generating",
            title,
            job,
            erosion: false,
        });
    }

    /// Starts eroding the preview (if one is showing for this terrain) or
    /// the terrain itself, to preview.
    pub(in crate::app) fn start_erode(&mut self) {
        let Some(terrain) = self.terrain.target else {
            return;
        };
        if let Some(running) = &self.terrain.job {
            running.job.cancel();
            self.terrain.queued = Some(true);
            return;
        }
        let Some(placement) = self.world.get(terrain).and_then(Placement::of) else {
            return;
        };
        let kind = self.terrain.erosion;
        let what = match kind {
            ErosionKind::Hydraulic => "Rain Erosion",
            ErosionKind::Thermal => "Scree Erosion",
        };
        let (source, size, title, alone) = match &self.terrain.preview {
            Some(preview) if preview.terrain == terrain => (
                preview.data.clone(),
                preview.size,
                format!("{} with {what}", preview.label),
                preview.erosion,
            ),
            _ => {
                let Some(data) = self.world.terrain(terrain).cloned() else {
                    return;
                };
                (data, None, format!("Apply {what}"), true)
            }
        };
        let studs = size.map_or(placement.size, |s| Vec3::new(s.x, s.y, s.z));
        let samples = source.size();
        let scale = Scale {
            cell: Vec2::new(
                studs.x / (samples.0 - 1) as f32,
                studs.z / (samples.1 - 1) as f32,
            ),
            height: studs.y,
        };
        let mut hydraulic = self.terrain.hydraulic.clone();
        hydraulic.droplets = HydraulicSettings::droplets_for(
            samples.0 * samples.1,
            self.terrain.quality.droplets_per_sample(),
        );
        let thermal = self.terrain.thermal.clone();
        let job = Job::spawn(move |control| {
            let heights = match kind {
                ErosionKind::Hydraulic => {
                    erosion::hydraulic(source.heights(), samples, scale, &hydraulic, control)?
                }
                ErosionKind::Thermal => {
                    erosion::thermal(source.heights(), samples, scale, &thermal, control)?
                }
            };
            let mut data = (*source).clone();
            let all = data.bounds();
            data.heights_mut_in(all).copy_from_slice(&heights);
            data.clamp_heights_in(all);
            Some(JobResult { data, size })
        });
        self.terrain.job = Some(RunningJob {
            terrain,
            label: "Eroding",
            title,
            job,
            erosion: alone,
        });
    }

    /// A finished job becomes the preview; a job asked for while it was
    /// stopping starts.
    pub(super) fn poll_terrain_job(&mut self, ctx: &egui::Context) {
        if let Some(running) = &mut self.terrain.job {
            match running.job.try_take() {
                None => {
                    // Still working: keep the progress bar moving.
                    ctx.request_repaint_after(Duration::from_millis(100));
                    return;
                }
                Some(result) => {
                    let running = self.terrain.job.take().expect("checked above");
                    if let Some(result) = result {
                        self.terrain.preview = Some(Preview {
                            terrain: running.terrain,
                            data: Arc::new(result.data),
                            size: result.size,
                            label: running.title,
                            erosion: running.erosion,
                        });
                    }
                    ctx.request_repaint();
                }
            }
        }
        match self.terrain.queued.take() {
            Some(true) => self.start_erode(),
            Some(false) => self.start_generate(),
            None => {}
        }
    }

    /// Puts the preview into the world, as one undo step, with layers that
    /// suit it if the terrain has none (and that was asked for).
    pub(super) fn apply_preview(&mut self) {
        let Some(preview) = self.terrain.preview.take() else {
            return;
        };
        if self.world.get(preview.terrain).is_none() {
            return;
        }
        if self.apply_world_preview(&preview) {
            return;
        }
        self.replace_terrain(
            preview.terrain,
            preview.label.clone(),
            &preview.data,
            preview.size,
        );
        let preset = preview
            .data
            .generation
            .as_deref()
            .and_then(|text| GeneratorSettings::from_json(text).ok())
            .map(|settings| settings.preset);
        if self.terrain.setup_materials
            && let Some(preset) = preset
            && materials::layers(&self.world, preview.terrain).is_empty()
        {
            let label = format!("Add {} Layers", preset.label());
            self.add_terrain_layers(preview.terrain, &layers_for(preset), label);
        }
    }

    /// Replaces `terrain`'s whole landscape (and its Size, if given) with
    /// `data`, as one undo step named `label`.
    pub(in crate::app) fn replace_terrain(
        &mut self,
        terrain: InstanceId,
        label: String,
        data: &TerrainData,
        size: Option<Vector3>,
    ) {
        let Some(before) = self.world.terrain(terrain).cloned() else {
            return;
        };
        let fields_before = history::snapshot(&self.world, terrain);
        if let Some(size) = size
            && let Err(err) = self
                .world
                .set_property(terrain, "Size", Value::Vector3(size))
        {
            self.output
                .error(format!("Could not resize the terrain: {err}"));
            return;
        }
        if let Some(current) = self.world.terrain_mut(terrain) {
            current.replace_with(data);
        }
        let Some(after) = self.world.terrain(terrain).cloned() else {
            return;
        };
        let fields_after = history::snapshot(&self.world, terrain);
        self.history.record_terrain(
            &mut self.world,
            terrain,
            label,
            TerrainChange::Replace { before, after },
            fields_before,
            fields_after,
        );
    }

    /// Gives the terrain `samples` samples along X (and as many along Z as
    /// keep the cells square), blending the landscape and paint across.
    pub(super) fn resample_terrain(&mut self, samples: usize) {
        let Some(terrain) = self.terrain.target else {
            return;
        };
        let Some(size) = self
            .world
            .get(terrain)
            .and_then(Placement::of)
            .map(|placement| placement.size)
        else {
            return;
        };
        let Some(data) = self.world.terrain(terrain) else {
            return;
        };
        let samples_z = samples_along(samples, size.x, size.z);
        match data.resampled(samples, samples_z) {
            Ok(resampled) => self.replace_terrain(
                terrain,
                format!("Resample Terrain to {samples} Samples"),
                &resampled,
                None,
            ),
            Err(err) => self
                .output
                .error(format!("Could not resample the terrain: {err}")),
        }
    }

    /// Makes `settings`' landscape now, on this thread, `size` studs
    /// across: on `terrain` (keeping its samples across, its layers and
    /// where it is) or, with none, on a new Terrain with the preset's
    /// layers. One undo step, `label`. For callers that wait (MCP).
    pub(in crate::app) fn generate_now(
        &mut self,
        terrain: Option<InstanceId>,
        mut settings: GeneratorSettings,
        size: Vector3,
        label: String,
    ) -> Result<InstanceId, String> {
        let samples_x = terrain
            .and_then(|id| self.world.terrain(id))
            .map_or(self.terrain.new_terrain.samples, |data| data.size().0);
        let samples_z = samples_along(samples_x, size.x, size.z);
        let target = Target {
            size_x: samples_x,
            size_z: samples_z,
            world_size: Vec2::new(size.x, size.z),
        };
        let generator = generate::check(&settings, target).map_err(|err| err.to_string())?;
        settings.algorithm = generator.name().to_owned();
        settings.version = generator.version();
        let heights = generator
            .generate(&settings, target, &axcenity_terrain::job::JobControl::new())
            .ok_or("it was cancelled")?;
        let mut data =
            TerrainData::flat(samples_x, samples_z, 0.0).map_err(|err| err.to_string())?;
        let all = data.bounds();
        data.heights_mut_in(all).copy_from_slice(&heights);
        data.clamp_heights_in(all);
        data.generation = Some(settings.to_json());
        match terrain {
            Some(id) => {
                let revision = self.history.revision();
                self.replace_terrain(id, label, &data, Some(size));
                if self.history.revision() == revision {
                    return Err("the terrain didn't change".to_owned());
                }
                Ok(id)
            }
            None => self
                .insert_terrain(size, data, settings.preset, Some(label))
                .ok_or_else(|| "no terrain could be made".to_owned()),
        }
    }

    /// Wears `terrain` down with rain or scree now, on this thread, at the
    /// Terrain Studio's settings, as one undo step `label`.
    pub(in crate::app) fn erode_now(
        &mut self,
        terrain: InstanceId,
        kind: ErosionKind,
        label: String,
    ) -> Result<(), String> {
        let placement = self
            .world
            .get(terrain)
            .and_then(Placement::of)
            .ok_or("it isn't a Terrain")?;
        let source = self
            .world
            .terrain(terrain)
            .cloned()
            .ok_or("it has no landscape")?;
        let samples = source.size();
        let studs = placement.size;
        let scale = Scale {
            cell: Vec2::new(
                studs.x / (samples.0 - 1) as f32,
                studs.z / (samples.1 - 1) as f32,
            ),
            height: studs.y,
        };
        let control = axcenity_terrain::job::JobControl::new();
        let heights = match kind {
            ErosionKind::Hydraulic => {
                let mut hydraulic = self.terrain.hydraulic.clone();
                hydraulic.droplets = HydraulicSettings::droplets_for(
                    samples.0 * samples.1,
                    self.terrain.quality.droplets_per_sample(),
                );
                erosion::hydraulic(source.heights(), samples, scale, &hydraulic, &control)
            }
            ErosionKind::Thermal => erosion::thermal(
                source.heights(),
                samples,
                scale,
                &self.terrain.thermal,
                &control,
            ),
        }
        .ok_or("it was cancelled")?;
        let mut data = (*source).clone();
        let all = data.bounds();
        data.heights_mut_in(all).copy_from_slice(&heights);
        data.clamp_heights_in(all);
        self.replace_terrain(terrain, label, &data, None);
        Ok(())
    }
}
