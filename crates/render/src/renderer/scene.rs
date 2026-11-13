use super::*;
use rustc_hash::FxHashMap;

impl ScenePipeline {
    pub(super) fn new(
        device: wgpu::Device,
        queue: wgpu::Queue,
        color_format: wgpu::TextureFormat,
        width: u32,
        height: u32,
        capabilities: Capabilities,
    ) -> Self {
        let skinning = Skinning::new(&device);
        let frame_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("frame uniform"),
            size: size_of::<FrameUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let frame_entries = frame_layout_entries();
        let frame_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("frame"),
            entries: &frame_entries,
        });
        let light_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("lights"),
            size: (MAX_LIGHTS * size_of::<LightRaw>()) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let tile_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("light tiles"),
            size: (MAX_TILES * (LIGHTS_PER_TILE + 1) * size_of::<u32>()) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let shadow_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("shadow map"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            compare: Some(wgpu::CompareFunction::LessEqual),
            ..Default::default()
        });
        let (empty_shadow_view, empty_layers) = create_shadow_views(&device, "no shadow map", 1, 1);
        // Its one layer, as a plain 2D depth texture.
        let no_prepass_depth_view = empty_layers[0].clone();
        let no_occlusion_view = create_white_occlusion(&device, &queue);
        // The occlusion is drawn at half the scene's size, so it's
        // smoothed back up as it's read.
        let occlusion_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("ambient occlusion"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let environment = environment::Environment::new(&device, &queue);
        let (environment_view, sky_image_view) = environment.views();
        let probes = probes::Probes::new(&device, &queue);
        let light_probes = light_probes::LightProbes::new(&device);
        let no_ray_shadows = ray_shadows::stand_in(&device);
        let frame_bind_group = create_frame_bind_group(
            &device,
            &frame_layout,
            [
                &frame_buffer,
                &light_buffer,
                &tile_buffer,
                &skinning.joints,
                light_probes.buffer(),
                &no_ray_shadows,
            ],
            [
                &empty_shadow_view,
                &empty_shadow_view,
                &no_occlusion_view,
                environment_view,
                sky_image_view,
                probes.view(),
                &empty_shadow_view,
                &no_prepass_depth_view,
            ],
            [&shadow_sampler, &occlusion_sampler, &environment.sampler],
        );
        let prepass_frame_bind_group = frame_bind_group.clone();
        let cascade_size = size_of::<[[f32; 4]; 4]>() as u64;
        let cascade_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("shadow views"),
            size: CASCADE_UNIFORM_STRIDE
                * (SHADOW_CASCADES + MAX_SPOT_SHADOWS + MAX_POINT_SHADOW_VIEWS) as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        // Binding 5 matches `shadow_cascade` in shader.wgsl. Each pass picks
        // its cascade with an offset into the buffer (a "dynamic offset").
        // Bindings 0 and 9 are the frame's uniform (its wind and clock, for
        // swaying plants) and joints, as in the frame's group.
        let shadow_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("shadow pass"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 5,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: true,
                        min_binding_size: wgpu::BufferSize::new(cascade_size),
                    },
                    count: None,
                },
                skinning::joints_entry(),
            ],
        });
        let shadow_bind_group = create_shadow_bind_group(
            &device,
            &shadow_layout,
            &frame_buffer,
            &cascade_buffer,
            &skinning.joints,
        );
        let shadow_pipeline = create_shadow_pipeline(&device, &shadow_layout, &skinning);

        // Bindings 0 and 3–9 are the textures (see `material_bindings`).
        let texture_entry = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        };
        let mut entries = vec![
            texture_entry(0),
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
            // The vertex shader reads its texture coordinate transform.
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
        ];
        entries.extend((3..=9).map(texture_entry));
        let material_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("material"),
            entries: &entries,
        });
        // glTF's default: smooth, blending between mipmap levels, and
        // repeating past the edges.
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("material textures"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            address_mode_w: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            ..Default::default()
        });
        let (texture_bytes, mesh_bytes) = (GpuBytes::default(), GpuBytes::default());
        let white_texture = create_texture(
            &device,
            &queue,
            &texture_bytes,
            (1, 1, &[255; 4]),
            (0, capabilities.max_texture_size),
            0,
        );
        let plain_material = create_material(
            &device,
            &material_layout,
            &sampler,
            [&white_texture; MATERIAL_TEXTURES],
            maps_uniform_of(&MaterialKey::default()),
        );

        let settings = GraphicsSettings::default();
        let graphics = settings.effective(&capabilities);
        let samples = graphics.anti_aliasing.samples();
        let (sky_pipeline, item_pipelines, outline_pipeline, prefill_pipelines) = scene_pipelines(
            &device,
            [&frame_layout, &material_layout, &skinning.layout],
            samples,
        );
        let water = water::WaterPass::new(
            &device,
            [&frame_layout, &material_layout, &skinning.layout],
            samples,
        );
        let mut terrain =
            terrain::TerrainPass::new(&device, &frame_layout, texture_bytes.clone(), samples);
        terrain.set_skipped_levels(graphics.texture_quality.skipped_levels());
        let sprites = sprites::SpritePass::new(&device, &white_texture.srgb, samples);
        let grid_pass = grid::GridPass::new(&device, samples);
        let decal_pass =
            decals::DecalPass::new(&device, &frame_layout, &white_texture.srgb, samples);
        let post = PostPasses::new(&device, color_format);
        let occlusion =
            OcclusionPasses::new(&device, [&frame_layout, &material_layout, &skinning.layout]);
        let edge_pipeline = create_edge_pipeline(&device, &occlusion.layout, samples);
        let temporal = temporal::TemporalPasses::new(
            &device,
            [&frame_layout, &skinning.empty_layout, &skinning.layout],
        );
        let targets_alive = TargetsAlive::default();
        let frame_targets = FrameTargets::new(
            &device,
            (&post, &occlusion, &temporal),
            scene_size((width, height), &graphics, &capabilities),
            (width, height),
            targets_used(&graphics, false, false, false),
            &targets_alive,
        );
        let ui_pipeline = create_ui_pipeline(&device, color_format);
        let (ui_image_layout, ui_sampler, ui_image_pipeline, ui_sheet_pipeline) =
            create_ui_image_pipeline(&device, color_format);
        let meshes = Meshes {
            block: upload_mesh(&device, &mesh_bytes, "block", &mesh::block()),
            sphere: upload_mesh(&device, &mesh_bytes, "sphere", &mesh::sphere()),
            cylinder: upload_mesh(&device, &mesh_bytes, "cylinder", &mesh::cylinder()),
            wedge: upload_mesh(&device, &mesh_bytes, "wedge", &mesh::wedge()),
            corner_wedge: upload_mesh(&device, &mesh_bytes, "corner wedge", &mesh::corner_wedge()),
        };
        let instance_capacity = 64;
        let instance_buffer = create_instance_buffer(&device, instance_capacity);
        let ui_capacity = 128;
        let ui_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("UI vertices"),
            size: (ui_capacity * size_of::<UiVertex>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let mut font_system = FontSystem::new();
        for data in crate::fonts::built_in_data() {
            font_system.db_mut().load_font_data(data.to_vec());
        }
        let swash_cache = SwashCache::new();
        let glyph_cache = GlyphCache::new(&device);
        let text_viewport = TextViewport::new(&device, &glyph_cache);
        let mut text_atlas = TextAtlas::new(&device, &queue, &glyph_cache, color_format);
        let turned_text = TurnedText {
            renderer: GlyphTextRenderer::new(
                &mut text_atlas,
                &device,
                wgpu::MultisampleState::default(),
                None,
            ),
            viewport: TextViewport::new(&device, &glyph_cache),
            sheet: None,
        };
        let gpu_timer = gpu_timer::GpuTimer::new(&device, &queue);

        let mut pipeline = Self {
            device,
            queue,
            sky_pipeline,
            item_pipelines,
            prefill_pipelines,
            cascades_drawn: [None; SHADOW_CASCADES],
            casters_planned: 0,
            outline_pipeline,
            edge_pipeline,
            edges: false,
            decals: false,
            sun_rays: false,
            particle_collisions: false,
            sun_rays_pass: None,
            depth_of_field: false,
            motion_blur: false,
            temporal,
            history: Default::default(),
            surface_shaders: Default::default(),
            sprites,
            decal_pass,
            grid_pass,
            post,
            frame_targets,
            occlusion,
            atmosphere: Default::default(),
            environment,
            probes,
            light_probes,
            realtime_gi: None,
            volumetric_fog: None,
            gpu_particles: None,
            water,
            water_frame_bind_group: None,
            cloud_drift: Default::default(),
            frame_entries,
            sun_disc: true,
            sky_band0: glam::Vec3::ZERO,
            no_occlusion_view,
            occlusion_sampler,
            targets_alive,
            output_size: (width, height),
            light_buffer,
            tile_buffer,
            ui_pipeline,
            ui_image_pipeline,
            ui_sheet_pipeline,
            ui_image_layout,
            ui_sampler,
            ui_image_groups: FxHashMap::default(),
            ui_buffer,
            ui_capacity,
            font_system,
            loaded_fonts: HashSet::new(),
            fitted_sizes: HashMap::new(),
            swash_cache,
            text_viewport,
            text_atlas,
            text_renderers: Vec::new(),
            turned_text,
            color_format,
            frame_buffer,
            frame_bind_group,
            prepass_frame_bind_group,
            frame_layout,
            empty_shadow_view,
            no_prepass_depth_view,
            shadow_sampler,
            shadow_pipeline,
            cascade_buffer,
            shadow_layout,
            shadow_bind_group,
            skinning,
            shadow_map: None,
            shadow_maps_alive: Arc::new(()),
            spot_shadow_map: None,
            spot_shadow_maps_alive: Arc::new(()),
            point_shadow_map: None,
            point_shadow_maps_alive: Arc::new(()),
            graphics,
            settings,
            capabilities,
            meshes,
            instance_buffer,
            instance_capacity,
            material_layout,
            sampler,
            white_texture,
            plain_material,
            uploader: None,
            frame_clock: std::collections::VecDeque::new(),
            custom_meshes: FxHashMap::default(),
            textures: FxHashMap::default(),
            full_textures: FxHashMap::default(),
            materials: FxHashMap::default(),
            terrain,
            frame: 0,
            gpu_timer,
            time_gpu: true,
            counts: FrameCounts::default(),
            frozen_view: None,
            culling_bounds: Vec::new(),
            lods: LodStates::default(),
            ray_scene: None,
            ray_shadows: None,
            no_ray_shadows,
            ray_ao: None,
            ray_gi: None,
            #[cfg(feature = "debug-views")]
            debug_views: None,
            texture_bytes,
            mesh_bytes,
        };
        pipeline.bind_frame();
        pipeline
    }

    /// Uses `settings` from the next frame, lowered to what the device can
    /// do. A shadow map (the sun's, the spot lights' or the point lights')
    /// of the wrong size is
    /// freed here and remade at the right size when next drawn. A new render
    /// scale remakes the scene's targets, and a new texture quality frees
    /// every texture drawn in the scene (and the materials using them), so
    /// they're uploaded again at the new size. A new anti-aliasing remakes
    /// the targets and, if its samples per pixel changed, the scene pass's
    /// pipelines.
    pub(super) fn set_graphics(&mut self, settings: &GraphicsSettings) {
        if *settings != self.settings {
            self.apply_graphics(settings);
        }
    }

    /// Draws as if the device could do no more than `limits`: each value is
    /// the lower of the two, so nothing is raised. The settings are lowered
    /// again to fit, and textures are uploaded again within the new size.
    pub(super) fn limit_capabilities(&mut self, limits: &Capabilities) {
        let real = self.capabilities;
        self.capabilities = Capabilities {
            max_texture_size: real.max_texture_size.min(limits.max_texture_size),
            max_texture_layers: real.max_texture_layers.min(limits.max_texture_layers),
            timestamp_queries: real.timestamp_queries && limits.timestamp_queries,
            software: real.software || limits.software,
            msaa_samples: real.msaa_samples & limits.msaa_samples,
            compute_shaders: real.compute_shaders && limits.compute_shaders,
            ray_queries: real.ray_queries && limits.ray_queries,
        };
        let settings = self.settings.clone();
        self.apply_graphics(&settings);
        self.materials.clear();
        self.textures.clear();
        self.sprites.forget(|_| false);
        self.decal_pass.forget(|_| false);
    }

    /// Uses `settings` from the next frame (see [`Self::set_graphics`]).
    fn apply_graphics(&mut self, settings: &GraphicsSettings) {
        let old_quality = self.graphics.texture_quality;
        let old_samples = self.graphics.anti_aliasing.samples();
        self.settings = settings.clone();
        self.graphics = settings.effective(&self.capabilities);
        if !self.graphics.realtime_gi && self.realtime_gi.take().is_some() {
            // The probes traced while it was on give way to their bakes.
            self.light_probes.restore_bakes();
        }
        // A new tier (roadmap R18.4) is made afresh, keeping what the old
        // one traced until it's traced again.
        if self
            .realtime_gi
            .as_ref()
            .is_some_and(|gi| gi.tier != self.graphics.realtime_gi_tier)
        {
            self.realtime_gi = None;
        }
        if !ray_gi::traced(&self.graphics, 1) && self.ray_gi.take().is_some() {
            self.light_probes.restore_bakes();
        }
        if !self.graphics.volumetric_fog {
            self.volumetric_fog = None;
        }
        let quality = self.graphics.shadow_quality;
        if self.shadow_map.as_ref().map(|map| map.size) != quality.map() {
            self.shadow_map = None;
        }
        if self.spot_shadow_map.as_ref().map(|map| map.size) != quality.spot_map() {
            self.spot_shadow_map = None;
        }
        if self.point_shadow_map.as_ref().map(|map| map.size) != quality.point_map() {
            self.point_shadow_map = None;
        }
        self.bind_frame();
        let samples = self.graphics.anti_aliasing.samples();
        if samples != old_samples {
            let building = std::time::Instant::now();
            (
                self.sky_pipeline,
                self.item_pipelines,
                self.outline_pipeline,
                self.prefill_pipelines,
            ) = scene_pipelines(
                &self.device,
                [
                    &self.frame_layout,
                    &self.material_layout,
                    &self.skinning.layout,
                ],
                samples,
            );
            self.edge_pipeline =
                create_edge_pipeline(&self.device, &self.occlusion.layout, samples);
            self.terrain
                .set_samples(&self.device, &self.frame_layout, samples);
            self.water.set_samples(
                &self.device,
                [
                    &self.frame_layout,
                    &self.material_layout,
                    &self.skinning.layout,
                ],
                samples,
            );
            self.sprites.set_samples(&self.device, samples);
            self.decal_pass.set_samples(&self.device, samples);
            self.grid_pass.set_samples(&self.device, samples);
            self.surface_shaders.clear_pipelines();
            log::info!(
                "graphics pipelines rebuilt for {samples} samples per pixel in {} ms",
                building.elapsed().as_millis()
            );
        }
        self.fit_targets();
        if self.graphics.texture_quality != old_quality {
            self.materials.clear();
            self.textures.clear();
            self.sprites.forget(|_| false);
            self.decal_pass.forget(|_| false);
            self.terrain
                .set_skipped_levels(self.graphics.texture_quality.skipped_levels());
        }
    }

    /// Call when the target changes size. Sizes are in pixels and never zero.
    pub(super) fn resize(&mut self, width: u32, height: u32) {
        self.output_size = (width, height);
        self.fit_targets();
    }

    /// Remakes the scene's targets if they weren't made for this output
    /// size, render scale, anti-aliasing, ambient occlusion, contact
    /// shadows, volumetric fog, screen-space outlines, depth of field, TAA
    /// and motion blur. The old ones are freed, and TAA's history with them.
    fn fit_targets(&mut self) {
        let size = scene_size(self.output_size, &self.graphics, &self.capabilities);
        let used = targets_used(
            &self.graphics,
            self.edges || self.decals || self.sun_rays || self.particle_collisions,
            self.depth_of_field,
            self.motion_blur,
        );
        let targets = &self.frame_targets;
        let made_for = (
            targets.size,
            targets.output,
            (
                targets.samples,
                targets.fxaa.is_some(),
                targets.occlusion.is_some(),
                targets.depth_of_field.is_some(),
                targets
                    .temporal
                    .as_ref()
                    .map_or_else(Default::default, |temporal| temporal.used()),
            ),
        );
        if made_for != (size, self.output_size, used) {
            self.frame_targets = FrameTargets::new(
                &self.device,
                (&self.post, &self.occlusion, &self.temporal),
                size,
                self.output_size,
                used,
                &self.targets_alive,
            );
            self.history.valid = false;
            // The frame bindings held the old occlusion texture.
            self.bind_frame();
        }
    }

    /// Draws `list` into `view`, which must be the size last given to
    /// `new` or `resize`, and sends the work to the GPU.
    pub(super) fn draw(
        &mut self,
        list: &DrawList,
        view: &wgpu::TextureView,
        aspect: f32,
        viewport_size: (u32, u32),
    ) -> Result<()> {
        // Screen-space outlines keep the prepass's targets alive while the
        // world has them.
        // So does depth of field, with a target of its own, and motion blur,
        // with the motion targets.
        let depth_of_field = lens::wants_depth_of_field(&list.lens);
        let motion_blur = temporal::wants_motion_blur(&list.lens);
        // So do projected decals, while the settings draw any.
        let decals = !list.decals.is_empty() && self.graphics.decals.count > 0;
        if self.decals != decals {
            self.decals = decals;
            self.fit_targets();
            // The frame bindings hand decal.wgsl the prepass's depth only
            // while it's read.
            self.bind_frame();
        }
        // And sun rays, which tell sky from solid by its depth.
        let sun_rays = sun_rays::wanted(list, &self.graphics);
        // And GPU particles that land on what it shows (roadmap R17.4b).
        let particle_collisions = self.capabilities.compute_shaders
            && list
                .particle_emitters
                .iter()
                .any(|emitter| emitter.collision.is_some());
        if self.edges != list.edges.is_some()
            || self.sun_rays != sun_rays
            || self.particle_collisions != particle_collisions
            || self.depth_of_field != depth_of_field
            || self.motion_blur != motion_blur
        {
            self.edges = list.edges.is_some();
            self.sun_rays = sun_rays;
            self.particle_collisions = particle_collisions;
            self.depth_of_field = depth_of_field;
            self.motion_blur = motion_blur;
            self.fit_targets();
        }
        // Lights are planned in the scene's pixels, which the render scale
        // makes fewer or more than the output's. UI uses the output's.
        let quality = self.graphics.shadow_quality;
        let planned_lights = plan_lights(list, aspect, self.frame_targets.size, quality);
        // An atmosphere's sky is worked out, and the sky's maps baked, only
        // when the sky has changed.
        let maps = self.graphics.reflection_quality.map().is_some();
        let sky = self.atmosphere.look(&list.sky, maps);
        if self.environment.prepare(
            &self.device,
            &self.queue,
            &sky,
            self.graphics.reflection_quality,
            self.capabilities.max_texture_size,
        ) {
            self.bind_frame();
        }
        // So are the reflection probes, each only when it's new.
        if self.probes.prepare(
            (&self.device, &self.queue),
            &self.environment,
            &list.reflection_probes,
            list.camera.position,
            (
                self.graphics.reflection_quality,
                self.capabilities.max_texture_size,
            ),
        ) {
            self.bind_frame();
        }
        // And the light probe volumes, each only when it's new, with the
        // camera's own probes while ray traced GI is on.
        if self.light_probes.prepare(
            (&self.device, &self.queue),
            &list.light_probe_volumes,
            list.camera.position,
            ray_gi::wanted(&self.graphics),
        ) {
            self.bind_frame();
        }
        // With ray traced GI (renderer/ray_gi.rs), a batch of their probes
        // is traced with rays this frame, once the ray traced scene is up
        // to date; its pass needs the frame's bindings.
        let ray_gi = ray_gi::traced(&self.graphics, self.light_probes.count());
        if !ray_gi {
            if self.ray_gi.take().is_some() {
                // The probes traced while it was on give way to their bakes.
                self.light_probes.restore_bakes();
            }
        } else if self.ray_gi.is_none() {
            self.ray_gi = Some(Box::new(ray_gi::RayGlobalIllumination::new(
                &self.device,
                &self.frame_entries,
            )));
            self.bind_frame();
        }
        let ray_gi_batch = match &mut self.ray_gi {
            Some(gi) => gi.plan(&self.queue, &self.light_probes, &self.graphics),
            None => None,
        };
        // Otherwise, with real-time GI, a few of their probes are traced
        // again first by drawing what they see.
        if !ray_gi && self.graphics.realtime_gi && self.light_probes.count() > 0 {
            let gi = self.realtime_gi.get_or_insert_with(|| {
                Box::new(realtime_gi::RealtimeGi::new(
                    &self.device,
                    &self.queue,
                    list,
                    &self.settings,
                ))
            });
            gi.update(list, &self.light_probes, &self.settings)?;
        } else {
            self.realtime_gi = None;
        }
        let mut frame_uniform = frame_uniform(list, aspect, &self.graphics);
        // The time and wind plants sway by, kept for next frame's motion.
        let clock = frame_uniform.clock;
        // With TAA, the scene is drawn a fraction of a pixel to one side,
        // a different way each frame (renderer/temporal.rs).
        let view_proj = camera_view_projection(&list.camera, aspect);
        // Upscaling TAA steps through more places, since each output pixel
        // gets fewer of the scene's.
        let taa_upscales = self
            .frame_targets
            .temporal
            .as_ref()
            .is_some_and(|temporal| temporal.upscales());
        let jitter_phases = if taa_upscales {
            temporal::jitter_phases(
                self.frame_targets.size.0 as f32 / self.frame_targets.output.0.max(1) as f32,
            )
        } else {
            temporal::JITTER_PHASES
        };
        let jitter = self.history.jitter(jitter_phases);
        let drawn_view_proj = match &self.frame_targets.temporal {
            Some(temporal) if temporal.has_taa() => {
                self.history
                    .jittered(view_proj, self.frame_targets.size, jitter_phases)
            }
            _ => view_proj,
        };
        frame_uniform.view_proj = drawn_view_proj.to_cols_array_2d();
        frame_uniform.inv_view_proj = drawn_view_proj.inverse().to_cols_array_2d();
        (frame_uniform.environment, frame_uniform.sky_light) = self.environment.uniform();
        let sky_colors = [
            &mut frame_uniform.sky_top,
            &mut frame_uniform.sky_horizon,
            &mut frame_uniform.sky_ground,
        ];
        for (slot, color) in sky_colors.into_iter().zip(sky.colors) {
            slot[..3].copy_from_slice(&color.to_array());
        }
        let stars = if list.sky.stars.is_finite() {
            list.sky.stars.clamp(0.0, 1.0)
        } else {
            0.0
        };
        // A creator's sky picture dims at night as the colors do; an
        // atmosphere's works out its own light.
        let picture_light =
            if sky.image.is_some() && !sky.scattered && list.sky.daylight.is_finite() {
                list.sky.daylight.clamp(0.0, 1.0)
            } else {
                1.0
            };
        for coefficient in &mut frame_uniform.sky_light {
            for channel in &mut coefficient[..3] {
                *channel *= picture_light;
            }
        }
        self.sky_band0 = self.environment.sky_band0(sky.colors, picture_light);
        frame_uniform.night = [
            stars,
            f32::from(u8::from(sky.scattered)),
            picture_light,
            0.0,
        ];
        // Volumetric fog needs the prepass's depth, which its setting keeps.
        let volumetric = fog::volumetric_drawn(&list.fog, &self.graphics)
            && self.frame_targets.occlusion.is_some();
        if !volumetric {
            self.volumetric_fog = None;
        } else if self.volumetric_fog.is_none() {
            self.volumetric_fog = Some(Box::new(fog::VolumetricFog::new(
                &self.device,
                &self.frame_entries,
                &self.frame_layout,
                &self.targets_alive.volumetric_fog,
            )));
            self.bind_frame();
        }
        // The sky's light averaged over every way (as `sky_diffuse` sees it,
        // times the environment intensity), which lights dust where no
        // volumetric fog or light probes say how open it is.
        let sky_average = {
            let rgb = |slot: &[f32; 4]| glam::Vec3::from_slice(&slot[..3]);
            let average = if frame_uniform.environment[0] > 0.5 {
                rgb(&frame_uniform.sky_light[0]) * 0.282_095
            } else {
                let sky = rgb(&frame_uniform.sky_horizon).lerp(rgb(&frame_uniform.sky_top), 0.5);
                (sky + rgb(&frame_uniform.sky_ground)) * 0.5
            };
            average * frame_uniform.sky_top[3]
        };
        frame_uniform.fog = fog::FogRaw::new(
            &list.fog,
            sky.colors[1],
            sky_average,
            glam::Vec3::from_slice(&frame_uniform.sun_direction[..3]),
            glam::Vec3::from_slice(&frame_uniform.sun_radiance[..3]),
            if volumetric {
                fog::VOLUMETRIC_REACH
            } else {
                0.0
            },
        );
        let drift = self.cloud_drift.advance(&list.clouds, list.time);
        frame_uniform.clouds =
            clouds::CloudsRaw::new(&list.clouds, drift, self.graphics.cloud_quality);
        let (probes, probe_count) = self.probes.uniform();
        frame_uniform.probes = probes;
        frame_uniform.reflection_probes[0] = probe_count;
        // Water's copy of the solid scene exists only while it's used.
        let water_mode = water::WaterMode::of(list, &self.graphics);
        frame_uniform.reflection_probes[1..3].copy_from_slice(&water_mode.flags());
        let copy_size = water_mode.copies().then_some(self.frame_targets.size);
        if self
            .water
            .fit(&self.device, copy_size, &self.targets_alive.water)
        {
            self.bind_frame();
        }
        let (volumes, volume_count) = self.light_probes.uniform(self.sky_band0);
        frame_uniform.volumes = volumes;
        frame_uniform.light_probes = [volume_count, u32::from(!self.sun_disc), 0, 0];
        let (scene_width, scene_height) = self.frame_targets.size;
        frame_uniform.ambient_occlusion[2] = 1.0 / scene_width as f32;
        frame_uniform.ambient_occlusion[3] = 1.0 / scene_height as f32;
        frame_uniform.lights = [
            planned_lights.lights.len() as u32,
            planned_lights.tile_size,
            planned_lights.tiles_across,
            0,
        ];
        for (matrix, spot) in frame_uniform
            .spot_shadows
            .iter_mut()
            .zip(&planned_lights.spot_shadows)
        {
            *matrix = spot.view_proj.to_cols_array_2d();
        }
        let point_faces = planned_lights
            .point_shadows
            .iter()
            .flat_map(|point| point.faces);
        for (matrix, face) in frame_uniform.point_shadows.iter_mut().zip(point_faces) {
            *matrix = face.to_cols_array_2d();
        }
        // Far cascades of a heavy scene take turns being drawn: those not
        // drawn this frame keep what they last drew, and the matrix it was
        // drawn with goes in `frame_uniform`.
        // Ray traced sun shadows, while drawn (renderer/ray_shadows.rs):
        // their mask is bound in the frame's place 17.
        let ray_shadow_uniform = match ray_shadows::RayShadowPlan::of(list, &self.graphics)
            .filter(|_| self.frame_targets.occlusion.is_some())
        {
            None => {
                if self.ray_shadows.take().is_some() {
                    self.bind_frame();
                }
                None
            }
            Some(plan) => {
                let size = self.frame_targets.size;
                if !self
                    .ray_shadows
                    .as_ref()
                    .is_some_and(|shadows| shadows.fits(size, plan.shape()))
                {
                    self.ray_shadows = Some(Box::new(ray_shadows::RayShadows::new(
                        &self.device,
                        (size, plan.shape()),
                    )));
                    self.bind_frame();
                }
                let lamp_words = self
                    .ray_shadows
                    .as_ref()
                    .map_or(0, |shadows| shadows.lamp_words());
                frame_uniform.ray_shadows = [1.0, size.0 as f32, plan.reach, lamp_words as f32];
                // Under TAA the rays turn each frame, and TAA averages them.
                let taa = self
                    .frame_targets
                    .temporal
                    .as_ref()
                    .is_some_and(|temporal| temporal.has_taa());
                let turn = if taa { (self.frame % 64) as f32 } else { 0.0 };
                Some(ray_shadows::ray_shadow_uniform(
                    list,
                    &plan,
                    (
                        drawn_view_proj.inverse(),
                        glam::Mat4::from_cols_array_2d(&frame_uniform.view),
                    ),
                    (
                        size,
                        if planned_lights.lights.is_empty() {
                            (planned_lights.tile_size, 0)
                        } else {
                            (planned_lights.tile_size, planned_lights.tiles_across)
                        },
                    ),
                    turn,
                ))
            }
        };
        // Ray traced ambient occlusion, while drawn (renderer/ray_ao.rs).
        let ray_ao_uniform =
            if ray_ao::traced(&self.graphics) && self.frame_targets.occlusion.is_some() {
                if self.ray_ao.is_none() {
                    self.ray_ao = Some(Box::new(ray_ao::RayAmbientOcclusion::new(
                        &self.device,
                        &self.occlusion,
                    )));
                }
                let taa = self
                    .frame_targets
                    .temporal
                    .as_ref()
                    .is_some_and(|temporal| temporal.has_taa());
                let turn = if taa { (self.frame % 64) as f32 } else { 0.0 };
                let (view, _) = lighting::camera_matrices(&list.camera, aspect);
                Some(ray_ao::ray_ao_uniform(list, &self.graphics, view, turn))
            } else {
                self.ray_ao = None;
                None
            };
        let cascade_drawn = self.schedule_cascades(list, &mut frame_uniform);
        self.queue
            .write_buffer(&self.frame_buffer, 0, bytemuck::bytes_of(&frame_uniform));
        // The sun's cascades, then the spot lights' views, then the point
        // lights' faces.
        let shadow_views = frame_uniform
            .cascades
            .iter()
            .chain(&frame_uniform.spot_shadows)
            .chain(&frame_uniform.point_shadows);
        for (index, matrix) in shadow_views.enumerate() {
            self.queue.write_buffer(
                &self.cascade_buffer,
                index as u64 * CASCADE_UNIFORM_STRIDE,
                bytemuck::bytes_of(matrix),
            );
        }
        if !planned_lights.lights.is_empty() {
            self.queue.write_buffer(
                &self.light_buffer,
                0,
                bytemuck::cast_slice(&planned_lights.lights),
            );
            self.queue.write_buffer(
                &self.tile_buffer,
                0,
                bytemuck::cast_slice(&planned_lights.tiles),
            );
        }
        // Ambient occlusion is drawn screen-space, ray traced or both (the
        // rays over screen-space's, fading into it toward their reach).
        let screen_space_occlusion =
            self.graphics.ambient_occlusion != crate::AmbientOcclusion::Off;
        let ambient_occlusion = screen_space_occlusion || ray_ao_uniform.is_some();
        if ambient_occlusion && self.frame_targets.occlusion.is_some() {
            let uniform = occlusion_uniform(list, aspect, self.graphics.ambient_occlusion);
            self.queue
                .write_buffer(&self.occlusion.uniform, 0, bytemuck::bytes_of(&uniform));
        }
        if let Some(edges) = &list.edges {
            let uniform = edge_uniform(list, edges, aspect, self.frame_targets.size);
            self.queue.write_buffer(
                &self.occlusion.edge_uniform,
                0,
                bytemuck::bytes_of(&uniform),
            );
        }
        let post = post_drawn(&list.post, &self.graphics);
        let mut post_settings = post_uniform(&post);
        lens::add_lens(
            &mut post_settings,
            &list.lens,
            &self.graphics,
            (aspect, self.output_size.1),
            self.frame,
        );
        self.queue
            .write_buffer(&self.post.settings, 0, bytemuck::bytes_of(&post_settings));
        if self.frame_targets.depth_of_field.is_some() {
            self.post.depth_of_field.prepare(
                &self.queue,
                list,
                aspect,
                self.frame_targets.size.1,
                self.graphics.depth_of_field,
            );
        }
        self.post
            .grading
            .prepare(&self.device, &self.queue, list.grading.as_ref());

        self.frame += 1;
        // Each item's level of detail, which decides the meshes uploaded.
        // Everything the frame's views can see, or everything while a
        // frozen view is being looked at from outside.
        let reach = self.frozen_view.is_none().then(|| {
            let mut reach = Reach::default();
            reach
                .views
                .push(Frustum::of(camera_view_projection(&list.camera, aspect)));
            let cascades = shadows_drawn(list.shadows, &self.graphics)
                .map_or(0, |(_, size)| size.cascades as usize);
            reach.views.extend(
                frame_uniform.cascades[..cascades]
                    .iter()
                    .map(|matrix| Frustum::of(glam::Mat4::from_cols_array_2d(matrix))),
            );
            for spot in &planned_lights.spot_shadows {
                reach.lamps.push((spot.bounds, Frustum::of(spot.view_proj)));
            }
            for point in &planned_lights.point_shadows {
                for face in point.faces {
                    reach.lamps.push((point.bounds, Frustum::of(face)));
                }
            }
            reach
        });
        let Resolved {
            draws,
            culled: past_cull_distance,
            submitted: items_submitted,
        } = self.lods.resolve(list, self.frame, reach.as_ref());
        // Each draw's material binding, worked out once for the uploads
        // and the plan: it reads every map setting of the item.
        let uses = FrameUses::of(&draws);
        self.upload_new(list, &draws, &uses);
        // What the camera can see, or the frozen view's while it's set.
        let camera = self.frozen_view.as_ref().unwrap_or(&list.camera);
        let camera_view = Frustum::of(camera_view_projection(camera, aspect));
        let mut seen = std::mem::take(&mut self.culling_bounds);
        seen.clear();
        let DrawPlan {
            instances,
            batches,
            casters,
            culled,
            joints,
            skins,
            lod_items,
        } = {
            let (meshes, custom_meshes) = (&self.meshes, &self.custom_meshes);
            plan_draws(
                list,
                &draws,
                &uses,
                &camera_view,
                |mesh, skin| item_bounds(meshes, custom_meshes, mesh, skin),
                self.frozen_view.is_some().then_some(&mut seen),
            )
        };
        self.culling_bounds = seen;
        // What moved since last frame, for the motion vectors.
        let moved = self.frame_targets.temporal.as_ref().map(|_| {
            let moved = self.history.moved(&draws, &joints, &skins, clock);
            let blur = list
                .lens
                .motion_blur
                .filter(|share| share.is_finite())
                .map_or(0.0, |share| share.clamp(0.0, 1.0));
            self.temporal.prepare(
                (&self.device, &self.queue),
                &self.history,
                (view_proj, drawn_view_proj, jitter),
                (
                    self.frame_targets.size,
                    taa_upscales.then_some(self.frame_targets.output),
                ),
                (blur, self.graphics.motion_blur),
                clock,
                &moved,
            );
            moved
        });
        let joints_remade = self
            .skinning
            .write_joints(&self.device, &self.queue, &joints);
        // Shadow maps cost memory, so one exists only while shadows are on,
        // at the size the graphics settings give.
        let map_size = shadows_drawn(list.shadows, &self.graphics).map(|(_, size)| size);
        // Each shadow view's casters are copied after the scene's instances:
        // the sun's cascades, each a box reaching back toward the sun, then
        // the spot lights' views, each near enough to its beam, then each
        // point light's faces, each within its reach.
        let cascades = map_size.map_or(0, |size| size.cascades as usize);
        let mut shadow_view_list: Vec<ShadowView> = frame_uniform.cascades[..cascades]
            .iter()
            .zip(cascade_drawn)
            .map(|(matrix, drawn)| {
                if drawn {
                    ShadowView::Sun(Frustum::of(glam::Mat4::from_cols_array_2d(matrix)))
                } else {
                    ShadowView::Skipped
                }
            })
            .collect();
        for spot in &planned_lights.spot_shadows {
            shadow_view_list.push(ShadowView::Lamp(spot.bounds, Frustum::of(spot.view_proj)));
        }
        for point in &planned_lights.point_shadows {
            for face in point.faces {
                shadow_view_list.push(ShadowView::Lamp(point.bounds, Frustum::of(face)));
            }
        }
        self.casters_planned = casters.iter().map(Vec::len).sum();
        let view_casters = casters_in_views(&casters, &shadow_view_list);
        let mut instance_count = instances.iter().map(Vec::len).sum::<usize>() as u32;
        let mut pieces: Vec<&[InstanceRaw]> = instances.iter().map(Vec::as_slice).collect();
        let mut casters_by_view = Vec::with_capacity(view_casters.len());
        for view in &view_casters {
            let groups: Vec<_> = view
                .groups
                .iter()
                .map(|(mesh, range)| {
                    (
                        *mesh,
                        range.start + instance_count..range.end + instance_count,
                    )
                })
                .collect();
            casters_by_view.push((groups, view.left_out));
            pieces.extend(view.instances.iter().map(Vec::as_slice));
            instance_count += view.instances.iter().map(Vec::len).sum::<usize>() as u32;
        }
        self.ensure_instance_capacity(instance_count as usize);
        write_pieces(&self.queue, &self.instance_buffer, &pieces);
        for font in &list.fonts {
            if self.loaded_fonts.insert(font.path.clone()) {
                self.font_system
                    .db_mut()
                    .load_font_data(font.data.as_ref().clone());
                // A new face can change what fits.
                self.fitted_sizes.clear();
            }
        }
        // UI is drawn in its list's order, layer by layer (see `ui_layers`).
        let layer_ranges = ui_layers(&list.ui);
        let (text_buffers, text_lines, layers_with_text) =
            self.prepare_ui_text(&list.ui, &layer_ranges, viewport_size)?;
        let turned = self.prepare_turned_text(&list.ui, viewport_size)?;
        // The sheet's underlines go first in the buffer.
        let mut ui_vertices = turned.lines.clone();
        let sheet_lines = 0..ui_vertices.len() as u32;
        let mut ui_draws = Vec::with_capacity(layer_ranges.len());
        for (index, range) in layer_ranges.iter().enumerate() {
            let start = ui_vertices.len() as u32;
            ui_vertices.extend(super::ui_vertices(
                &list.ui,
                range.clone(),
                &text_lines,
                viewport_size,
            ));
            let rectangles = start..ui_vertices.len() as u32;
            let images = self.ui_image_draws(
                &list.ui,
                range.clone(),
                &turned.quads,
                viewport_size,
                &mut ui_vertices,
            );
            ui_draws.push(UiLayer {
                rectangles,
                images,
                text: layers_with_text[index],
            });
        }
        self.ensure_ui_capacity(ui_vertices.len());
        if !ui_vertices.is_empty() {
            self.queue
                .write_buffer(&self.ui_buffer, 0, bytemuck::cast_slice(&ui_vertices));
        }
        let (sprite_draws, sprites_culled, emitter_textures) = {
            let textures = &self.textures;
            self.sprites.prepare(
                &self.device,
                &self.queue,
                list,
                (drawn_view_proj, frame_uniform.fog),
                &camera_view,
                |key| {
                    textures
                        .get(&key)
                        .and_then(|cached| cached.gpu.as_ref())
                        .map(|gpu| &gpu.srgb)
                },
            )
        };
        let (decal_draws, decals_left_out) = {
            let textures = &self.textures;
            let normals = self
                .frame_targets
                .occlusion
                .as_ref()
                .map(|targets| &targets.normal_view);
            self.decal_pass.prepare(
                (&self.device, &self.queue),
                list,
                (self.graphics.decals, &camera_view),
                normals,
                |key| {
                    textures
                        .get(&key)
                        .and_then(|cached| cached.gpu.as_ref())
                        .map(|gpu| &gpu.srgb)
                },
            )
        };
        // GPU particles, where compute shaders run.
        if list.particle_emitters.is_empty() || !self.capabilities.compute_shaders {
            self.gpu_particles = None;
        } else {
            self.gpu_particles
                .get_or_insert_with(|| Box::new(gpu_particles::GpuParticles::new(&self.device)))
                .prepare(
                    (&self.device, &self.queue),
                    list,
                    &emitter_textures,
                    gpu_particles::PrepassView {
                        view_proj: glam::Mat4::from_cols_array_2d(&frame_uniform.view_proj),
                        inv_view_proj: glam::Mat4::from_cols_array_2d(&frame_uniform.inv_view_proj),
                        view: glam::Mat4::from_cols_array_2d(&frame_uniform.view),
                        size: self.frame_targets.size,
                    },
                    self.frame_targets
                        .occlusion
                        .as_ref()
                        .map(|targets| (&targets.depth_view, &targets.normal_view)),
                );
        }

        // The spot and point lights' shadow maps exist only while one of
        // them casts shadows.
        let spot_map = quality
            .spot_map()
            .filter(|_| !planned_lights.spot_shadows.is_empty());
        let point_map = quality
            .point_map()
            .filter(|_| !planned_lights.point_shadows.is_empty());
        let mut changed = false;
        if self.shadow_map.as_ref().map(|map| map.size) != map_size {
            self.shadow_map = map_size.map(|size| {
                // The shader holds at most `SHADOW_CASCADES` cascades.
                debug_assert!(
                    size.size <= SHADOW_MAP_SIZE && size.cascades as usize <= SHADOW_CASCADES
                );
                let (array, layers) =
                    create_shadow_views(&self.device, "shadow map", size.size, size.cascades);
                ShadowMap {
                    size,
                    _alive: self.shadow_maps_alive.clone(),
                    bytes: target_bytes((size.size, size.size, size.cascades), DEPTH_FORMAT, 1),
                    array,
                    layers,
                }
            });
            changed = true;
        }
        if self.spot_shadow_map.as_ref().map(|map| map.size) != spot_map {
            self.spot_shadow_map = spot_map.map(|size| {
                debug_assert!(size.lights as usize <= MAX_SPOT_SHADOWS);
                let (array, layers) =
                    create_shadow_views(&self.device, "spot shadow map", size.size, size.lights);
                ShadowMap {
                    size,
                    _alive: self.spot_shadow_maps_alive.clone(),
                    bytes: target_bytes((size.size, size.size, size.lights), DEPTH_FORMAT, 1),
                    array,
                    layers,
                }
            });
            changed = true;
        }
        if self.point_shadow_map.as_ref().map(|map| map.size) != point_map {
            self.point_shadow_map = point_map.map(|size| {
                debug_assert!(size.lights as usize <= MAX_POINT_SHADOWS);
                let count = size.lights * POINT_SHADOW_FACES;
                let (array, layers) =
                    create_shadow_views(&self.device, "point shadow map", size.size, count);
                ShadowMap {
                    size,
                    _alive: self.point_shadow_maps_alive.clone(),
                    bytes: target_bytes((size.size, size.size, count), DEPTH_FORMAT, 1),
                    array,
                    layers,
                }
            });
            changed = true;
        }
        if changed || joints_remade {
            self.bind_frame();
        }

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("frame"),
            });
        let traces_rays = self.traces_rays(
            ray_shadow_uniform.is_some() || ray_ao_uniform.is_some() || ray_gi_batch.is_some(),
        );
        // Times this frame unless the last measurement is still being read.
        let timed = match &mut self.gpu_timer {
            Some(timer) if self.time_gpu => timer.collect(&self.device),
            _ => false,
        };
        let timer = self.gpu_timer.as_ref().filter(|_| timed);
        let mark = |encoder: &mut wgpu::CommandEncoder, pass| {
            if let Some(timer) = timer {
                timer.mark(encoder, pass);
            }
        };
        mark(&mut encoder, FramePass::AccelerationStructures);
        let tracing = ray_tracing::prepare_scene(
            &mut self.ray_scene,
            (
                (&self.device, &self.queue),
                &self.meshes,
                &self.custom_meshes,
            ),
            &mut encoder,
            (
                traces_rays,
                self.frame,
                glam::Vec3::from_slice(&frame_uniform.camera_position[..3]),
                self.graphics.ray_tracing_distance,
                ray_gi_batch.is_some(),
            ),
            &draws,
        );
        mark(&mut encoder, FramePass::Shadows);
        let mut counts = FrameCounts {
            items_submitted,
            items_culled: culled + past_cull_distance,
            shadow_casters_culled: casters_by_view.iter().map(|(_, culled)| culled).sum(),
            sprites_culled,
            decals_drawn: list.decals.len() as u32 - decals_left_out,
            decals_left_out,
            lod_items,
            ..FrameCounts::default()
        };
        let mut shadow_casters_drawn = 0;
        let mut count = |mesh: &GpuMesh, instances: &Range<u32>| {
            let instances = u64::from(instances.end - instances.start);
            counts.draw_calls += 1;
            counts.triangles += u64::from(mesh.index_count / 3) * instances;
            counts.vertices += u64::from(mesh.vertex_count) * instances;
        };

        // One pass per cascade, each drawing into its own layer of the map,
        // then one per shadow-casting spot light and one per face of each
        // shadow-casting point light, each drawing the casters that can
        // reach its view. Each view's matrix is at its index in
        // `cascade_buffer`: the spot lights' after every cascade's, the
        // point lights' after every spot light's.
        let (cascade_casters, lamp_casters) = casters_by_view.split_at(cascades);
        let (spot_casters, point_casters) =
            lamp_casters.split_at(planned_lights.spot_shadows.len());
        let cascades = self
            .shadow_map
            .iter()
            .flat_map(|map| map.layers.iter().zip(cascade_casters).enumerate());
        let spots = self
            .spot_shadow_map
            .iter()
            .flat_map(|map| map.layers.iter().zip(spot_casters).enumerate())
            .map(|(index, view)| (SHADOW_CASCADES + index, view));
        let points = self
            .point_shadow_map
            .iter()
            .flat_map(|map| map.layers.iter().zip(point_casters).enumerate())
            .map(|(index, view)| (SHADOW_CASCADES + MAX_SPOT_SHADOWS + index, view));
        let mut cascades_timed = [false; SHADOW_CASCADES];
        let mut cascade_counts = [(0u32, 0u64); SHADOW_CASCADES];
        let mut after_cascades = false;
        for (index, (layer, (casters, _))) in cascades.chain(spots).chain(points) {
            // A cascade waiting its turn keeps what it drew: not even cleared.
            if index < SHADOW_CASCADES && !cascade_drawn[index] {
                continue;
            }
            if let Some(timer) = timer {
                if index < SHADOW_CASCADES {
                    timer.mark_cascade(&mut encoder, index);
                    cascades_timed[index] = true;
                } else if !after_cascades {
                    timer.mark_cascade(&mut encoder, SHADOW_CASCADES);
                    after_cascades = true;
                }
            }
            let (mut view_casters, mut view_triangles) = (0u32, 0u64);
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("shadow view"),
                color_attachments: &[],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: layer,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                ..Default::default()
            });
            pass.set_pipeline(&self.shadow_pipeline);
            let offset = (index as u64 * CASCADE_UNIFORM_STRIDE) as u32;
            pass.set_bind_group(0, &self.shadow_bind_group, &[offset]);
            pass.set_bind_group(1, &self.skinning.empty, &[]);
            pass.set_vertex_buffer(1, self.instance_buffer.slice(..));
            // See-through items, and parts with `CastShadow` off, cast no shadow.
            for (mesh, instances) in casters {
                let Some(mesh) = self.gpu_mesh(*mesh) else {
                    continue;
                };
                pass.set_bind_group(2, self.skin_group(mesh), &[]);
                pass.set_vertex_buffer(0, mesh.vertices.slice(..));
                pass.set_index_buffer(mesh.indices.slice(..), mesh.index_format);
                count(mesh, instances);
                shadow_casters_drawn += instances.end - instances.start;
                view_casters += instances.end - instances.start;
                view_triangles +=
                    u64::from(mesh.index_count / 3) * u64::from(instances.end - instances.start);
                pass.draw_indexed(0..mesh.index_count, 0, instances.clone());
            }
            drop(pass);
            if index < SHADOW_CASCADES {
                cascade_counts[index] = (view_casters, view_triangles);
            }
        }
        if let Some(timer) = timer
            && !after_cascades
        {
            timer.mark_cascade(&mut encoder, SHADOW_CASCADES);
        }
        let frame_bind_group = &self.frame_bind_group;
        mark(&mut encoder, FramePass::Prepass);
        let mut motion_draws = 0;

        // While ambient occlusion or contact shadows are on: solid items'
        // depth and normals, then (for ambient occlusion) how hidden each
        // pixel is by what's around it.
        if let Some(occlusion) = &self.frame_targets.occlusion {
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("depth and normal prepass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        depth_slice: None,
                        view: &occlusion.normal_view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                        view: &occlusion.depth_view,
                        depth_ops: Some(wgpu::Operations {
                            load: wgpu::LoadOp::Clear(1.0),
                            store: wgpu::StoreOp::Store,
                        }),
                        stencil_ops: None,
                    }),
                    ..Default::default()
                });
                pass.set_bind_group(0, &self.prepass_frame_bind_group, &[]);
                pass.set_vertex_buffer(1, self.instance_buffer.slice(..));
                for batch in batches.iter().filter(|batch| !batch.transparent) {
                    let Some(mesh) = self.bind_batch(&mut pass, batch, BatchPass::Prepass) else {
                        continue;
                    };
                    count(mesh, &batch.instances);
                    pass.draw_indexed(0..mesh.index_count, 0, batch.instances.clone());
                }
            }
            // The motion vectors, from what the prepass drew.
            if let (Some(targets), Some(moved)) = (&self.frame_targets.temporal, &moved) {
                motion_draws = self.temporal.draw_motion(
                    &mut encoder,
                    (targets, &self.history),
                    (
                        &occlusion.depth_view,
                        &self.prepass_frame_bind_group,
                        &self.skinning.empty,
                    ),
                    moved,
                    |pass, key| {
                        let mesh = self.gpu_mesh(key)?;
                        pass.set_bind_group(2, self.skin_group(mesh), &[]);
                        pass.set_vertex_buffer(0, mesh.vertices.slice(..));
                        pass.set_index_buffer(mesh.indices.slice(..), mesh.index_format);
                        Some(mesh.index_count)
                    },
                );
            }
            mark(&mut encoder, FramePass::AmbientOcclusion);
            if screen_space_occlusion {
                self.occlusion.draw_raw(&mut encoder, occlusion);
            }
            if let (Some(pass), Some(uniform), Some(rays)) =
                (&self.ray_ao, &ray_ao_uniform, &self.ray_scene)
            {
                pass.draw(
                    (&self.device, &self.queue),
                    &mut encoder,
                    (rays, occlusion),
                    uniform,
                    screen_space_occlusion,
                );
            }
            if ambient_occlusion {
                self.occlusion.blur(&mut encoder, occlusion);
            }
        } else {
            mark(&mut encoder, FramePass::AmbientOcclusion);
        }
        mark(&mut encoder, FramePass::RayTracedShadows);
        let ray_shadows_drawn = if let (Some(shadows), Some(uniform), Some(rays), Some(prepass)) = (
            &self.ray_shadows,
            &ray_shadow_uniform,
            &self.ray_scene,
            &self.frame_targets.occlusion,
        ) {
            shadows.draw(
                (&self.device, &self.queue),
                &mut encoder,
                (rays, prepass),
                (&self.light_buffer, &self.tile_buffer),
                uniform,
            );
            true
        } else {
            false
        };
        mark(&mut encoder, FramePass::RayTracedGi);
        // The camera's probes move along with it first.
        self.light_probes.follow_camera(&mut encoder);
        let ray_gi_drawn = if let (Some(gi), Some(batch), Some(rays)) =
            (&mut self.ray_gi, &ray_gi_batch, &self.ray_scene)
            && let Some(surfaces) = &rays.surfaces
        {
            gi.trace(
                (&self.device, &self.queue),
                &mut encoder,
                (rays.tlas(), &surfaces.buffer, &rays.cut_outs),
                batch,
                &self.light_probes,
            );
            true
        } else {
            false
        };
        mark(&mut encoder, FramePass::VolumetricFog);
        if let Some(fog) = &self.volumetric_fog {
            fog.scatter(&mut encoder);
        }
        mark(&mut encoder, FramePass::Particles);
        if let Some(particles) = &self.gpu_particles {
            particles.run(&mut encoder);
        }
        mark(&mut encoder, FramePass::Scene);
        let emitter_draws = self
            .gpu_particles
            .as_ref()
            .map_or_else(Vec::new, |particles| particles.draws());

        // The scene is drawn into the HDR texture; post-processing then brings
        // it to `view`. With MSAA it's drawn into the multisampled texture,
        // whose samples are averaged into the HDR one as the pass ends.
        // Solid items come first, then the sky only where none was drawn
        // (its depth test), so the sky and its clouds are worked out only
        // for the pixels that show them. Solid items' outlines come just
        // after them, and screen-space outlines after the sky. Then
        // see-through items over all that:
        // water first, then the rest, then sprites. While water sees
        // through itself, the pass stops before the see-through items so
        // the solid scene can be copied for it (renderer/water.rs), and a
        // second pass carries on where it left off.
        let see_through = batches
            .iter()
            .position(|batch| batch.transparent)
            .unwrap_or(batches.len());
        let (solid, see_through) = batches.split_at(see_through);
        let copied = self.water.copy.is_some() && see_through.iter().any(|batch| batch.water);
        let water_group = self
            .water_frame_bind_group
            .as_ref()
            .filter(|_| copied)
            .unwrap_or(frame_bind_group);
        let mut drawn = (0, 0);
        // The depth fill's draw calls, triangles and corners.
        let mut depth_fill = (0, 0, 0);
        let mut draw_batches = |pass: &mut wgpu::RenderPass<'_>, batches: &[Batch]| {
            let mut water_bound = false;
            for batch in batches {
                if batch.water != water_bound {
                    water_bound = batch.water;
                    let group = if water_bound {
                        water_group
                    } else {
                        frame_bind_group
                    };
                    pass.set_bind_group(0, group, &[]);
                }
                let Some(mesh) = self.bind_batch(pass, batch, BatchPass::Scene) else {
                    continue;
                };
                count(mesh, &batch.instances);
                drawn.0 += batch.instances.end - batch.instances.start;
                drawn.1 += 1;
                pass.draw_indexed(0..mesh.index_count, 0, batch.instances.clone());
            }
            if water_bound {
                pass.set_bind_group(0, frame_bind_group, &[]);
            }
            // Outlines (solid items only) go behind what was just drawn.
            for batch in batches.iter().filter(|batch| batch.outline) {
                let Some(mesh) = self.bind_batch(pass, batch, BatchPass::Scene) else {
                    continue;
                };
                pass.set_pipeline(&self.outline_pipeline);
                count(mesh, &batch.instances);
                pass.draw_indexed(0..mesh.index_count, 0, batch.instances.clone());
            }
        };
        let edges = self
            .frame_targets
            .occlusion
            .as_ref()
            .filter(|_| list.edges.is_some());
        if let Some(grid) = &list.grid {
            self.grid_pass.prepare(
                &self.queue,
                frame_uniform.view_proj,
                frame_uniform.camera_position,
                grid,
            );
        }
        // Each pass is a block so it ends (is dropped) before the next
        // begins and before `encoder.finish()`, which need the encoder back.
        {
            let mut pass = begin_scene_pass(&mut encoder, &self.frame_targets, false, copied);
            pass.set_bind_group(0, frame_bind_group, &[]);
            pass.set_vertex_buffer(1, self.instance_buffer.slice(..));
            // The cut-out surfaces' depth first, so they're lit only once
            // per pixel, and what's behind them not at all.
            for batch in solid.iter().filter(|batch| batch.prefilled) {
                let Some(mesh) = self.bind_batch(&mut pass, batch, BatchPass::DepthFill) else {
                    continue;
                };
                let instances = u64::from(batch.instances.end - batch.instances.start);
                depth_fill.0 += 1;
                depth_fill.1 += u64::from(mesh.index_count / 3) * instances;
                depth_fill.2 += u64::from(mesh.vertex_count) * instances;
                pass.draw_indexed(0..mesh.index_count, 0, batch.instances.clone());
            }
            draw_batches(&mut pass, solid);
            pass.set_pipeline(&self.sky_pipeline);
            pass.draw(0..3, 0..1);
            // Projected decals over the solid scene (the sky's pixels are
            // left alone by the prepass's depth).
            self.decal_pass.draw(&mut pass, &decal_draws);
            if list.grid.is_some() {
                self.grid_pass.draw(&mut pass);
                pass.set_bind_group(0, frame_bind_group, &[]);
            }
            // Screen-space outlines over the solid scene and the sky.
            if let Some(prepass) = edges {
                pass.set_pipeline(&self.edge_pipeline);
                pass.set_bind_group(0, &prepass.edge_group, &[]);
                pass.draw(0..3, 0..1);
                pass.set_bind_group(0, frame_bind_group, &[]);
            }
            if !copied {
                draw_batches(&mut pass, see_through);
                // Last, over everything, but hidden behind solid items.
                self.sprites.draw(&mut pass, &sprite_draws, &emitter_draws);
            }
        }
        if copied {
            let targets = &self.frame_targets;
            self.water.copy_scene(
                (&self.device, &self.queue),
                &mut encoder,
                (&targets.hdr_view, &targets.depth_view),
                water::CopyView {
                    inv_view_proj: frame_uniform.inv_view_proj,
                    camera_position: frame_uniform.camera_position,
                },
            );
            let mut pass = begin_scene_pass(&mut encoder, targets, true, false);
            pass.set_bind_group(0, frame_bind_group, &[]);
            pass.set_vertex_buffer(1, self.instance_buffer.slice(..));
            draw_batches(&mut pass, see_through);
            self.sprites.draw(&mut pass, &sprite_draws, &emitter_draws);
        }
        (counts.items_drawn, counts.batches) = drawn;
        counts.draw_calls += motion_draws + depth_fill.0 + decal_draws.len() as u32;
        counts.triangles += depth_fill.1;
        counts.vertices += depth_fill.2;
        // The light the volumetric fog scattered in front of what was drawn.
        if let (Some(fog), Some(occlusion)) = (&self.volumetric_fog, &self.frame_targets.occlusion)
        {
            fog.composite(
                &self.device,
                &mut encoder,
                frame_bind_group,
                &self.frame_targets.hdr_view,
                &occlusion.depth_view,
            );
        }
        // Sun rays streaming from the sky's gaps, added to the scene.
        // Under TAA their noise turns each frame, so TAA averages it away.
        let taa = self
            .frame_targets
            .temporal
            .as_ref()
            .is_some_and(|temporal| temporal.has_taa());
        let turn = if taa { (self.frame % 64) as f32 } else { 0.0 };
        if let (Some(rays), Some(occlusion)) = (
            sun_rays::uniform(list, &self.graphics, aspect, turn),
            &self.frame_targets.occlusion,
        ) {
            let pass = self
                .sun_rays_pass
                .get_or_insert_with(|| Box::new(sun_rays::SunRaysPass::new(&self.device)));
            pass.draw(
                &self.device,
                &mut encoder,
                &rays,
                &self.frame_targets.hdr_view,
                &occlusion.depth_view,
            );
        }
        mark(&mut encoder, FramePass::PostProcessing);

        // TAA, then motion blur, each copied over the scene, then depth of
        // field. While TAA upscales, it comes last: motion blur and depth of
        // field are drawn on the scene first, and TAA reads the result.
        let targets = &self.frame_targets;
        let temporal_targets = targets.temporal.as_ref().map(|t| (t, &self.history));
        if let Some(temporal) = temporal_targets
            && !taa_upscales
        {
            self.temporal.draw_taa(&mut encoder, temporal, &targets.hdr);
        }
        if let Some(temporal) = temporal_targets {
            self.temporal
                .draw_motion_blur(&mut encoder, temporal, &targets.hdr);
        }
        if let Some(target) = &targets.depth_of_field {
            self.post.depth_of_field.draw(&mut encoder, target);
        }
        if let Some(temporal) = temporal_targets
            && taa_upscales
        {
            self.temporal.draw_taa(&mut encoder, temporal, &targets.hdr);
        }
        self.post.draw(
            &mut encoder,
            &self.frame_targets,
            view,
            post.enabled && post.bloom_enabled && post.bloom_intensity > 0.0,
        );

        // The editor's debug view, over the finished frame and under the
        // UI (renderer/debug_views.rs).
        #[cfg(feature = "debug-views")]
        if let Some(mut debug) = self.debug_views.take() {
            debug.draw(self, &mut encoder, view, (&batches, &draws), view_proj);
            self.debug_views = Some(debug);
        }

        mark(&mut encoder, FramePass::Ui);
        if turned.drawn
            && let Some((sheet, _, _)) = &self.turned_text.sheet
        {
            let sheet_view = sheet.create_view(&wgpu::TextureViewDescriptor::default());
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("turned UI text"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    depth_slice: None,
                    view: &sheet_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                ..Default::default()
            });
            if !sheet_lines.is_empty() {
                pass.set_pipeline(&self.ui_pipeline);
                pass.set_vertex_buffer(0, self.ui_buffer.slice(..));
                pass.draw(sheet_lines.clone(), 0..1);
            }
            self.turned_text
                .renderer
                .render(&self.text_atlas, &self.turned_text.viewport, &mut pass)
                .context("could not render turned UI text")?;
        }
        let _ = &turned.buffers;

        let ui_drawn = !ui_vertices.is_empty() || !text_buffers.is_empty();
        if ui_drawn {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("UI overlay"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    depth_slice: None,
                    view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                ..Default::default()
            });
            for (index, layer) in ui_draws.iter().enumerate() {
                if !layer.rectangles.is_empty() {
                    pass.set_pipeline(&self.ui_pipeline);
                    pass.set_vertex_buffer(0, self.ui_buffer.slice(..));
                    pass.draw(layer.rectangles.clone(), 0..1);
                }
                if !layer.images.is_empty() {
                    pass.set_vertex_buffer(0, self.ui_buffer.slice(..));
                    for (key, start, count) in &layer.images {
                        let (pipeline, group) = if *key == SHEET_KEY {
                            let Some((_, group, _)) = &self.turned_text.sheet else {
                                continue;
                            };
                            (&self.ui_sheet_pipeline, group)
                        } else {
                            let Some(group) = self.ui_image_groups.get(key) else {
                                continue;
                            };
                            (&self.ui_image_pipeline, group)
                        };
                        pass.set_pipeline(pipeline);
                        pass.set_bind_group(0, group, &[]);
                        pass.draw(*start..*start + *count, 0..1);
                    }
                }
                if layer.text {
                    self.text_renderers[index]
                        .render(&self.text_atlas, &self.text_viewport, &mut pass)
                        .context("could not render UI text")?;
                }
            }
        }

        if let Some(timer) = timer {
            timer.end(&mut encoder);
        }
        self.queue.submit([encoder.finish()]);
        if timed && let Some(timer) = &mut self.gpu_timer {
            let shadows = self.shadow_map.is_some()
                || self.spot_shadow_map.is_some()
                || self.point_shadow_map.is_some();
            let prepass = self.frame_targets.occlusion.is_some();
            timer.submitted(
                [
                    tracing,
                    shadows,
                    prepass,
                    prepass && ambient_occlusion,
                    ray_shadows_drawn,
                    ray_gi_drawn,
                    self.volumetric_fog.is_some(),
                    self.gpu_particles.is_some(),
                    true,
                    true,
                    turned.drawn || ui_drawn,
                ],
                cascades_timed,
            );
        }
        counts.shadow_casters_drawn = shadow_casters_drawn;
        counts.cascade_casters = cascade_counts.map(|(casters, _)| casters);
        counts.cascade_triangles = cascade_counts.map(|(_, triangles)| triangles);
        self.counts = counts;
        match &self.frame_targets.temporal {
            Some(temporal) => self
                .history
                .finish(list, view_proj, clock, temporal.has_taa()),
            None => self.history.reset(),
        }
        self.forget_undrawn();
        Ok(())
    }

    /// Which of the sun's cascades this frame draws. In a heavy scene (more
    /// shadow casters than [`HEAVY_SHADOW_CASTERS`]) the two farthest take
    /// turns, so each frame draws one: they hold most of its shadow
    /// triangles and cover what's far away, where one frame's lag doesn't
    /// show. A cascade waiting its turn goes in `uniform` as it was last
    /// drawn, for the shaders to read it the way it was drawn.
    fn schedule_cascades(
        &mut self,
        list: &DrawList,
        uniform: &mut FrameUniform,
    ) -> [bool; SHADOW_CASCADES] {
        let mut drawn = [true; SHADOW_CASCADES];
        let Some((_, size)) = shadows_drawn(list.shadows, &self.graphics) else {
            return drawn;
        };
        let cascades = size.cascades as usize;
        let map_fits = self.shadow_map.as_ref().map(|map| map.size) == Some(size);
        let heavy = self.casters_planned > HEAVY_SHADOW_CASTERS;
        for (index, drawn) in drawn.iter_mut().enumerate().take(cascades) {
            let takes_turns = heavy && map_fits && cascades >= 3 && index >= cascades - 2;
            let its_turn = (self.frame as usize + index).is_multiple_of(2);
            // A cascade reaches half the map's width from the camera: it
            // isn't kept once the camera has gone a good way across it.
            let near = |kept: &DrawnCascade| {
                kept.camera.distance(list.camera.position) <= uniform.cascade_texels[index] * 256.0
            };
            match self.cascades_drawn[index] {
                Some(kept) if takes_turns && !its_turn && near(&kept) => {
                    *drawn = false;
                    uniform.cascades[index] = kept.matrix;
                    uniform.cascade_texels[index] = kept.texel;
                    uniform.cascade_depths[index] = kept.depth;
                }
                _ => {
                    self.cascades_drawn[index] = Some(DrawnCascade {
                        matrix: uniform.cascades[index],
                        texel: uniform.cascade_texels[index],
                        depth: uniform.cascade_depths[index],
                        camera: list.camera.position,
                    });
                }
            }
        }
        drawn
    }

    /// Sets `pass` up to draw `batch`, in the scene pass, the depth fill
    /// or the depth and normal prepass (`kind`): its pipeline, its material
    /// or terrain binding, and its mesh, which is returned. `None` skips
    /// it: a caller's mesh with broken data was never uploaded.
    fn bind_batch(
        &self,
        pass: &mut wgpu::RenderPass<'_>,
        batch: &Batch,
        kind: BatchPass,
    ) -> Option<&GpuMesh> {
        let prepass = kind == BatchPass::Prepass;
        let mesh = self.gpu_mesh(batch.mesh)?;
        if batch.material.terrain != terrain::TerrainKey::default() {
            let group = self.terrain.group(&batch.material.terrain)?;
            pass.set_pipeline(if prepass {
                &self.terrain.prepass
            } else {
                &self.terrain.pipeline
            });
            pass.set_bind_group(1, group, &[]);
        } else {
            let surface = batch.shader.and_then(|key| self.surface_shaders.get(key));
            let water = &self.water.pipelines;
            let water = if self.water.copy.is_some() {
                &water.refracted
            } else {
                &water.blended
            };
            let side = usize::from(batch.double_sided);
            let pipeline = match (prepass, batch.double_sided, surface) {
                _ if kind == BatchPass::DepthFill => &self.prefill_pipelines.depth[side],
                // Water is never in the prepass.
                _ if batch.water => water,
                (false, _, None) if batch.prefilled => &self.prefill_pipelines.lit[side],
                (false, _, Some(surface)) => surface.items.of(batch),
                (false, _, None) => self.item_pipelines.of(batch),
                (true, double_sided, Some(surface)) => surface.prepass(double_sided),
                (true, false, None) => &self.occlusion.prepass,
                (true, true, None) => &self.occlusion.double_sided_prepass,
            };
            let material = self
                .materials
                .get(&batch.material)
                .and_then(|cached| cached.gpu.as_ref())
                .unwrap_or(&self.plain_material);
            pass.set_pipeline(pipeline);
            pass.set_bind_group(1, &material.bind_group, &[]);
            pass.set_bind_group(2, self.skin_group(mesh), &[]);
        }
        pass.set_vertex_buffer(0, mesh.vertices.slice(..));
        pass.set_index_buffer(mesh.indices.slice(..), mesh.index_format);
        Some(mesh)
    }

    /// The skin binding `mesh` is drawn with: its joints and weights, or
    /// none for a mesh that isn't animated.
    pub(super) fn skin_group<'a>(&'a self, mesh: &'a GpuMesh) -> &'a wgpu::BindGroup {
        mesh.skin
            .as_ref()
            .map_or(&self.skinning.none, |skin| &skin.bind_group)
    }

    /// Remakes the frame bindings with the shadow maps, ambient occlusion
    /// and prepass depth alive, binding the empty stand-in for each that
    /// isn't, so a freed map or target isn't kept alive by the bindings,
    /// and with the frame's joints (the shadow passes' bindings too).
    /// A bind group of `layout`, which holds only `bindings` of the frame's
    /// (of its uniform 0, lamps 3, sky 10 to 12 and light probes 16): for
    /// compute passes that would pass eight storage buffers a stage with
    /// all of them.
    pub(super) fn frame_subset(
        &self,
        layout: &wgpu::BindGroupLayout,
        bindings: &[u32],
    ) -> wgpu::BindGroup {
        let (environment, sky_image) = self.environment.views();
        let entries: Vec<wgpu::BindGroupEntry> = bindings
            .iter()
            .map(|&binding| wgpu::BindGroupEntry {
                binding,
                resource: match binding {
                    0 => self.frame_buffer.as_entire_binding(),
                    3 => self.light_buffer.as_entire_binding(),
                    10 => wgpu::BindingResource::TextureView(environment),
                    11 => wgpu::BindingResource::Sampler(&self.environment.sampler),
                    12 => wgpu::BindingResource::TextureView(sky_image),
                    16 => self.light_probes.buffer().as_entire_binding(),
                    _ => unreachable!("frame binding {binding} isn't offered on its own"),
                },
            })
            .collect();
        self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("frame (part)"),
            layout,
            entries: &entries,
        })
    }

    pub(super) fn bind_frame(&mut self) {
        let empty = &self.empty_shadow_view;
        let sun = self.shadow_map.as_ref().map_or(empty, |map| &map.array);
        let spot = self
            .spot_shadow_map
            .as_ref()
            .map_or(empty, |map| &map.array);
        let point = self
            .point_shadow_map
            .as_ref()
            .map_or(empty, |map| &map.array);
        let prepass = self.frame_targets.occlusion.as_ref();
        let occlusion = prepass
            .filter(|_| {
                self.graphics.ambient_occlusion != crate::AmbientOcclusion::Off
                    || ray_ao::traced(&self.graphics)
            })
            .map_or(&self.no_occlusion_view, |targets| &targets.blurred_view);
        let prepass_depth = prepass
            .filter(|_| self.graphics.contact_shadows || self.decals)
            .map_or(&self.no_prepass_depth_view, |targets| &targets.depth_view);
        let (environment, sky_image) = self.environment.views();
        let probes = self.probes.view();
        let ray_shadow_mask = self
            .ray_shadows
            .as_ref()
            .map_or(&self.no_ray_shadows, |shadows| &shadows.mask);
        let bind = |layout, prepass_depth, occlusion| {
            create_frame_bind_group(
                &self.device,
                layout,
                [
                    &self.frame_buffer,
                    &self.light_buffer,
                    &self.tile_buffer,
                    &self.skinning.joints,
                    self.light_probes.buffer(),
                    ray_shadow_mask,
                ],
                [
                    sun,
                    spot,
                    occlusion,
                    environment,
                    sky_image,
                    probes,
                    point,
                    prepass_depth,
                ],
                [
                    &self.shadow_sampler,
                    &self.occlusion_sampler,
                    &self.environment.sampler,
                ],
            )
        };
        self.frame_bind_group = bind(&self.frame_layout, prepass_depth, occlusion);
        self.prepass_frame_bind_group =
            bind(&self.frame_layout, &self.no_prepass_depth_view, occlusion);
        // Water reads its copy of the scene in ambient occlusion's place.
        self.water_frame_bind_group = self
            .water
            .copy
            .as_ref()
            .map(|copy| bind(&self.frame_layout, prepass_depth, &copy.view));
        let fog_group = self
            .volumetric_fog
            .as_ref()
            .map(|fog| bind(&fog.frame_layout, prepass_depth, occlusion));
        let ray_gi_group = self
            .ray_gi
            .as_ref()
            .map(|gi| self.frame_subset(&gi.frame_layout, &ray_gi::FRAME_BINDINGS));
        if let Some(fog) = &mut self.volumetric_fog {
            fog.frame_group = fog_group;
        }
        if let Some(gi) = &mut self.ray_gi {
            gi.frame_group = ray_gi_group;
        }
        self.shadow_bind_group = create_shadow_bind_group(
            &self.device,
            &self.shadow_layout,
            &self.frame_buffer,
            &self.cascade_buffer,
            &self.skinning.joints,
        );
    }

    /// How many shadow maps are alive: 0 or 1, unless one leaked.
    #[cfg(test)]
    pub(super) fn shadow_maps_alive(&self) -> usize {
        Arc::strong_count(&self.shadow_maps_alive) - 1
    }

    /// How many spot shadow maps are alive: 0 or 1, unless one leaked.
    #[cfg(test)]
    pub(super) fn spot_shadow_maps_alive(&self) -> usize {
        Arc::strong_count(&self.spot_shadow_maps_alive) - 1
    }

    /// How many point shadow maps are alive: 0 or 1, unless one leaked.
    #[cfg(test)]
    pub(super) fn point_shadow_maps_alive(&self) -> usize {
        Arc::strong_count(&self.point_shadow_maps_alive) - 1
    }

    /// How many light probes real-time GI has traced, or `None` while it
    /// has nothing to trace (it's off, or no volumes are drawn).
    #[cfg(test)]
    pub(super) fn probes_traced(&self) -> Option<u32> {
        self.realtime_gi.as_ref().map(|gi| gi.traced)
    }

    /// How many probes ray traced GI has traced, while it's on.
    #[cfg(test)]
    pub(super) fn probes_ray_traced(&self) -> Option<u64> {
        self.ray_gi.as_ref().map(|gi| gi.probes_traced)
    }

    /// How many sets of scene targets are alive: 1, unless one leaked.
    #[cfg(test)]
    pub(super) fn targets_alive(&self) -> usize {
        Arc::strong_count(&self.targets_alive.sets) - 1
    }

    /// How many sets of prepass and ambient occlusion targets are alive: 0
    /// or 1, unless one leaked.
    #[cfg(test)]
    pub(super) fn occlusion_targets_alive(&self) -> usize {
        Arc::strong_count(&self.targets_alive.occlusion) - 1
    }

    /// How many of depth of field's blurred copies of the scene are alive:
    /// 0 or 1, unless one leaked.
    #[cfg(test)]
    pub(super) fn depth_of_field_targets_alive(&self) -> usize {
        Arc::strong_count(&self.targets_alive.depth_of_field) - 1
    }

    /// How many sets of volumetric fog grids are alive: 0 or 1, unless one
    /// leaked.
    #[cfg(test)]
    pub(super) fn volumetric_fogs_alive(&self) -> usize {
        Arc::strong_count(&self.targets_alive.volumetric_fog) - 1
    }

    /// How many GPU particle emitters have buffers.
    #[cfg(test)]
    pub(super) fn gpu_particle_emitters(&self) -> usize {
        self.gpu_particles.as_ref().map_or(0, |p| p.emitters())
    }

    /// How many of water's copies of the scene are alive: 0 or 1, unless
    /// one leaked.
    #[cfg(test)]
    pub(super) fn water_copies_alive(&self) -> usize {
        Arc::strong_count(&self.targets_alive.water) - 1
    }

    /// How many sets of motion targets, TAA histories and motion blur
    /// targets are alive: 0 or 1 each, unless one leaked.
    #[cfg(test)]
    pub(super) fn temporal_targets_alive(&self) -> (usize, usize, usize) {
        let alive = &self.targets_alive;
        (
            Arc::strong_count(&alive.motion) - 1,
            Arc::strong_count(&alive.taa) - 1,
            Arc::strong_count(&alive.motion_blur) - 1,
        )
    }

    /// How many multisampled and FXAA targets are alive: 0 or 1 each.
    #[cfg(test)]
    pub(super) fn anti_aliasing_targets_alive(&self) -> (usize, usize) {
        let alive = &self.targets_alive;
        (
            Arc::strong_count(&alive.msaa) - 1,
            Arc::strong_count(&alive.fxaa) - 1,
        )
    }

    pub(super) fn stats(&self) -> RenderStats {
        let counts = self.counts;
        let shadow_bytes = self.shadow_map.as_ref().map_or(0, |map| map.bytes)
            + self.spot_shadow_map.as_ref().map_or(0, |map| map.bytes)
            + self.point_shadow_map.as_ref().map_or(0, |map| map.bytes);
        let sheet_bytes = self.turned_text.sheet.as_ref().map_or(0, |(sheet, _, _)| {
            target_bytes((sheet.width(), sheet.height(), 1), sheet.format(), 1)
        });
        let buffers = [
            &self.instance_buffer,
            &self.skinning.joints,
            &self.ui_buffer,
            &self.light_buffer,
            &self.tile_buffer,
            &self.frame_buffer,
            &self.cascade_buffer,
            self.light_probes.buffer(),
        ];
        let (surface_shaders, broken_shaders) = self.surface_shaders.counts();
        RenderStats {
            draw_calls: counts.draw_calls,
            triangles: counts.triangles,
            vertices: counts.vertices,
            gpu_time: self
                .gpu_timer
                .as_ref()
                .filter(|_| self.time_gpu)
                .and_then(gpu_timer::GpuTimer::last),
            gpu_timing: self.gpu_timer.is_some() && self.time_gpu,
            gpu_readings: self
                .gpu_timer
                .as_ref()
                .map_or(0, gpu_timer::GpuTimer::readings),
            cascade_times: self
                .gpu_timer
                .as_ref()
                .filter(|_| self.time_gpu)
                .map_or([None; SHADOW_CASCADES], gpu_timer::GpuTimer::last_cascades),
            cascade_casters: counts.cascade_casters,
            cascade_triangles: counts.cascade_triangles,
            pass_times: self
                .gpu_timer
                .as_ref()
                .filter(|_| self.time_gpu)
                .map_or([None; FramePass::COUNT], gpu_timer::GpuTimer::last_passes),
            items_submitted: counts.items_submitted,
            items_drawn: counts.items_drawn,
            batches: counts.batches,
            items_culled: counts.items_culled,
            shadow_casters_drawn: counts.shadow_casters_drawn,
            shadow_casters_culled: counts.shadow_casters_culled,
            sprites_culled: counts.sprites_culled,
            decals_drawn: counts.decals_drawn,
            decals_left_out: counts.decals_left_out,
            lod_items: counts.lod_items,
            surface_shaders,
            broken_shaders,
            gpu_particles: self.gpu_particles.as_ref().map_or(0, |p| p.count()),
            ray_tracing: self
                .ray_scene
                .as_ref()
                .map_or_else(Default::default, |scene| scene.stats),
            texture_bytes: self.texture_bytes.total(),
            mesh_bytes: self.mesh_bytes.total(),
            target_bytes: self.frame_targets.bytes
                + shadow_bytes
                + sheet_bytes
                + self.environment.bytes()
                + self.probes.bytes()
                + self.realtime_gi.as_ref().map_or(0, |gi| gi.bytes())
                + self.ray_gi.as_ref().map_or(0, |gi| gi.bytes())
                + self.volumetric_fog.as_ref().map_or(0, |fog| fog.bytes())
                + self.water.copy.as_ref().map_or(0, |copy| copy.bytes),
            buffer_bytes: buffers.iter().map(|buffer| buffer.size()).sum::<u64>()
                + self.gpu_particles.as_ref().map_or(0, |p| p.bytes())
                + self
                    .ray_shadows
                    .as_ref()
                    .map_or(0, |shadows| shadows.bytes()),
        }
    }

    /// The box a mesh's corners span, for culling. A caller's mesh that
    /// was never uploaded isn't drawn, so the unit cube does for it.
    #[cfg(test)]
    pub(super) fn mesh_bounds(&self, key: MeshKey) -> Bounds {
        self.gpu_mesh(key).map_or(Bounds::UNIT, |mesh| mesh.bounds)
    }

    /// The box an item's mesh spans, in its own space, once its `skin` (if
    /// any) moves its corners.
    #[cfg(test)]
    pub(super) fn item_bounds(&self, key: MeshKey, skin: Option<&Skin>) -> Bounds {
        item_bounds(&self.meshes, &self.custom_meshes, key, skin)
    }

    /// The GPU copy of a mesh, or `None` for a caller's mesh that was never uploaded.
    pub(super) fn gpu_mesh(&self, key: MeshKey) -> Option<&GpuMesh> {
        gpu_mesh_in(&self.meshes, &self.custom_meshes, key)
    }

    /// Shapes and uploads the UI text in `ui`, with its rich text runs,
    /// shadows, outlines and faux bold, into one text renderer per layer.
    /// Returns the shaped text, which must live until it's drawn, its
    /// underlines and line-throughs by the index of the text in `ui`, and
    /// which `layers` have text.
    pub(super) fn prepare_ui_text(
        &mut self,
        ui: &[UiDrawItem],
        layers: &[Range<usize>],
        (width, height): (u32, u32),
    ) -> Result<(Vec<Buffer>, Vec<TextLine>, Vec<bool>)> {
        self.text_viewport
            .update(&self.queue, Resolution { width, height });

        let text_items: Vec<(usize, &UiText)> = ui
            .iter()
            .enumerate()
            .filter_map(|(index, item)| match item {
                // Turned text is drawn from the sheet instead.
                UiDrawItem::Text(text) if !is_turned(text.rotation_degrees) => Some((index, text)),
                UiDrawItem::Text(_) | UiDrawItem::Rectangle(_) | UiDrawItem::Image(_) => None,
            })
            .collect();
        if self.fitted_sizes.len() > 4096 {
            self.fitted_sizes.clear();
        }
        let mut buffers = Vec::new();
        // Each copy to draw: the text, where its lines start, the buffer
        // it's shaped in, and the pass.
        let mut placed = Vec::new();
        let mut lines = Vec::new();
        for &(index, item) in &text_items {
            let shaped = self.shape_ui_text(item, &mut buffers);
            lines.extend(shaped.lines.into_iter().map(|line| {
                (
                    index,
                    UiRectangle {
                        clip: item.clip,
                        ..line
                    },
                )
            }));
            placed.extend(
                shaped
                    .passes
                    .into_iter()
                    .map(|pass| (index, item, shaped.top, shaped.first + pass.layer, pass)),
            );
        }

        let mut with_text = vec![false; layers.len()];
        for (layer, range) in layers.iter().enumerate() {
            let in_layer: Vec<_> = placed
                .iter()
                .filter(|(index, ..)| range.contains(index))
                .collect();
            if in_layer.is_empty() {
                continue;
            }
            with_text[layer] = true;
            while self.text_renderers.len() <= layer {
                self.text_renderers.push(GlyphTextRenderer::new(
                    &mut self.text_atlas,
                    &self.device,
                    wgpu::MultisampleState::default(),
                    None,
                ));
            }
            let areas = in_layer.into_iter().map(|&(_, text, top, buffer, pass)| {
                let [dx, dy] = pass.offset;
                let mut bounds = text_bounds(text, pass.offset);
                // Cut to the clip too, which doesn't move with a shadow.
                if let Some(clip) = text.clip {
                    let [left, upper, right, lower] = super::ui_pass::clip_bounds(clip);
                    bounds.left = bounds.left.max(left.floor() as i32);
                    bounds.top = bounds.top.max(upper.floor() as i32);
                    bounds.right = bounds.right.min(right.ceil() as i32);
                    bounds.bottom = bounds.bottom.min(lower.ceil() as i32);
                    bounds.right = bounds.right.max(bounds.left);
                    bounds.bottom = bounds.bottom.max(bounds.top);
                }
                TextArea {
                    buffer: &buffers[buffer],
                    left: text.position[0] + dx,
                    top: top + dy,
                    scale: 1.0,
                    // Where it may show, in screen pixels (not relative to
                    // the text's position): its box, moved with the copy so
                    // a shadow is cut where its letters are.
                    bounds,
                    default_color: glyph_color(pass.color),
                    custom_glyphs: &[],
                }
            });
            self.text_renderers[layer]
                .prepare(
                    &self.device,
                    &self.queue,
                    &mut self.font_system,
                    &mut self.text_atlas,
                    &self.text_viewport,
                    areas,
                    &mut self.swash_cache,
                )
                .context("could not prepare UI text")?;
        }
        Ok((buffers, lines, with_text))
    }

    /// Shapes one UI text into `buffers`, one buffer per paint (see
    /// `text::passes`), and works out its copies and decorations.
    pub(super) fn shape_ui_text(&mut self, item: &UiText, buffers: &mut Vec<Buffer>) -> ShapedText {
        let styled = text::Styled::new(self.font_system.db(), item);
        let font_size = if item.scaled {
            let runs: Vec<_> = styled
                .runs
                .iter()
                .map(|run| (&run.text, &run.font_family, run.font_size))
                .collect();
            let key = format!(
                "{:?}{:?}{:?}{:?}",
                styled.faces,
                runs,
                item.size,
                (
                    &item.font_family,
                    item.font_size,
                    item.letter_spacing,
                    item.line_height,
                    item.wrap,
                )
            );
            match self.fitted_sizes.get(&key) {
                Some(size) => *size,
                None => {
                    let size = text::fitted_size(&mut self.font_system, &styled);
                    self.fitted_sizes.insert(key, size);
                    size
                }
            }
        } else {
            item.font_size.max(1.0)
        };
        let (layers, passes) = text::passes(&styled, font_size);
        let first = buffers.len();
        for paint in &layers {
            buffers.push(text::shape(
                &mut self.font_system,
                &styled,
                font_size,
                paint,
            ));
        }
        let top = item.position[1] + text::vertical_offset(item, &buffers[first]);
        let lines = text::decorations(&styled, &buffers[first], font_size, [item.position[0], top]);
        ShapedText {
            first,
            top,
            passes,
            lines,
        }
    }

    /// Uploads the callers' meshes in `draws` (each item's level of
    /// detail) and textures in `list` that aren't on the GPU yet, and notes
    /// that the rest were drawn this frame. Textures are fitted to the
    /// texture budget before the materials using them are bound.
    /// `uses` says what `draws` use.
    pub(super) fn upload_new(&mut self, list: &DrawList, draws: &[Draw], uses: &FrameUses) {
        let frame = self.frame;
        let arrived = self.finish_background_uploads();
        // Thousands of draws share a few meshes and textures: each is
        // looked up once rather than once per draw.
        for &source in &uses.meshes {
            match self.custom_meshes.entry(source.key()) {
                Entry::Occupied(mut cached) => {
                    let cached = cached.get_mut();
                    cached.last_drawn = frame;
                    // A posed (animated) mesh has new corners: rewrite them.
                    if let Some(gpu) = &mut cached.gpu
                        && gpu.version != source.version()
                    {
                        update_custom_mesh(
                            &self.device,
                            &self.queue,
                            (&self.mesh_bytes, &self.skinning),
                            gpu,
                            source.as_ref(),
                        );
                    }
                }
                Entry::Vacant(slot) => {
                    if let Some(uploader) = &mut self.uploader {
                        // Not drawn until its thread is done with it.
                        uploader.mesh(source);
                        continue;
                    }
                    slot.insert(Cached {
                        gpu: upload_custom_mesh(
                            &self.device,
                            &self.mesh_bytes,
                            &self.skinning,
                            source.as_ref(),
                        ),
                        last_drawn: frame,
                    });
                }
            }
        }
        for draw in draws {
            if let Some(shader) = &draw.item.material.surface_shader {
                self.prepare_surface_shader(shader);
            }
        }
        for item in &list.items {
            if let Some(surface) = &item.material.terrain {
                for weights in &surface.weights {
                    self.note_texture(weights, frame, true);
                }
                let weights = surface.weights.each_ref().map(|source| {
                    self.full_textures
                        .get(&source.key())
                        .and_then(|cached| cached.gpu.as_ref())
                });
                self.terrain.prepare(
                    &self.device,
                    &self.queue,
                    surface,
                    weights,
                    &self.white_texture,
                    frame,
                );
                continue;
            }
            for source in material_textures(item).into_iter().flatten() {
                self.note_texture(source, frame, false);
            }
        }
        // Each material's textures are noted once. The list's own items
        // are done above: this is for what's in its groups.
        for (key, &first) in uses.materials.iter().zip(&uses.first_draw) {
            if *key == MaterialKey::default() {
                continue;
            }
            for source in material_textures(draws[first as usize].item)
                .into_iter()
                .flatten()
            {
                self.note_texture(source, frame, false);
            }
        }
        for item in &list.ui {
            if let UiDrawItem::Image(image) = item
                && let Some(source) = &image.texture
            {
                self.note_texture(source, frame, true);
            }
        }
        let emitter_textures = list.particle_emitters.iter().map(|e| &e.look.texture);
        let sprite_textures = list.sprites.iter().map(|s| &s.texture);
        let decal_textures = list.decals.iter().map(|d| &d.texture);
        for source in sprite_textures
            .chain(emitter_textures)
            .chain(decal_textures)
            .flatten()
        {
            self.note_texture(source, frame, false);
        }
        let mut replaced = self.fit_texture_budget(list);
        replaced.extend(arrived);
        if !replaced.is_empty() {
            self.materials.retain(|key, _| {
                !key.textures
                    .iter()
                    .flatten()
                    .any(|texture| replaced.contains(texture))
            });
            self.sprites.forget(|key| !replaced.contains(&key));
            self.decal_pass.forget(|key| !replaced.contains(&key));
        }
        for (key, &first) in uses.materials.iter().zip(&uses.first_draw) {
            let key = *key;
            if key == MaterialKey::default() {
                continue;
            }
            let item = draws[first as usize].item;
            if item.material.terrain.is_some() {
                continue;
            }
            let sources = material_textures(item);
            match self.materials.entry(key) {
                Entry::Occupied(mut cached) => cached.get_mut().last_drawn = frame,
                Entry::Vacant(slot) => {
                    // A texture with broken data is drawn as white instead.
                    let textures = sources.map(|source| {
                        source
                            .and_then(|source| self.textures.get(&source.key())?.gpu.as_ref())
                            .unwrap_or(&self.white_texture)
                    });
                    slot.insert(Cached {
                        gpu: Some(create_material(
                            &self.device,
                            &self.material_layout,
                            &self.sampler,
                            textures,
                            maps_uniform_of(&key),
                        )),
                        last_drawn: frame,
                    });
                }
            }
        }
    }

    /// Appends the quads of the UI images in `ui[range]`, and of its turned
    /// text (`turned`, by index in `ui`), and remembers which texture each
    /// uses: [`SHEET_KEY`] for turned text.
    pub(super) fn ui_image_draws(
        &mut self,
        ui: &[UiDrawItem],
        range: Range<usize>,
        turned: &HashMap<usize, Vec<UiVertex>>,
        viewport: (u32, u32),
        vertices: &mut Vec<UiVertex>,
    ) -> Vec<(u64, u32, u32)> {
        let mut draws = Vec::new();
        for index in range {
            if let Some(quad) = turned.get(&index) {
                let start = vertices.len() as u32;
                vertices.extend_from_slice(quad);
                draws.push((SHEET_KEY, start, quad.len() as u32));
                continue;
            }
            let UiDrawItem::Image(image) = &ui[index] else {
                continue;
            };
            let Some(source) = &image.texture else {
                continue;
            };
            let key = source.key();
            if self
                .full_textures
                .get(&key)
                .and_then(|cached| cached.gpu.as_ref())
                .is_none()
            {
                continue;
            }
            if !self.ui_image_groups.contains_key(&key) {
                let group = {
                    let gpu = self
                        .full_textures
                        .get(&key)
                        .and_then(|cached| cached.gpu.as_ref())
                        .expect("checked just above");
                    self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                        label: Some("UI image"),
                        layout: &self.ui_image_layout,
                        entries: &[
                            wgpu::BindGroupEntry {
                                binding: 0,
                                resource: wgpu::BindingResource::TextureView(&gpu.srgb),
                            },
                            wgpu::BindGroupEntry {
                                binding: 1,
                                resource: wgpu::BindingResource::Sampler(&self.ui_sampler),
                            },
                        ],
                    })
                };
                self.ui_image_groups.insert(key, group);
            }
            let quad = image_quad(image, viewport);
            if quad.is_empty() {
                continue;
            }
            let start = vertices.len() as u32;
            let count = quad.len() as u32;
            vertices.extend(quad);
            draws.push((key, start, count));
        }
        draws
    }

    /// Uploads `source` the first time, and marks it drawn this frame. A
    /// `full` texture (a UI image or terrain weights) keeps its full size;
    /// others are sized by the texture quality.
    pub(super) fn note_texture(&mut self, source: &Arc<dyn TextureSource>, frame: u64, full: bool) {
        let (cache, skipped_levels) = if full {
            (&mut self.full_textures, 0)
        } else {
            (
                &mut self.textures,
                self.graphics.texture_quality.skipped_levels(),
            )
        };
        match cache.entry(source.key()) {
            Entry::Occupied(mut cached) => cached.get_mut().last_drawn = frame,
            Entry::Vacant(slot) => {
                let sizing = (skipped_levels, self.capabilities.max_texture_size);
                // Terrain weights and UI images are small: done right away.
                if !full && let Some(uploader) = &mut self.uploader {
                    uploader.texture(source, sizing);
                    return;
                }
                slot.insert(Cached {
                    gpu: upload_texture(
                        &self.device,
                        &self.queue,
                        &self.texture_bytes,
                        source.as_ref(),
                        sizing,
                        0,
                        // UI images and terrain weights stay exact.
                        !full,
                    ),
                    last_drawn: frame,
                });
            }
        }
    }

    /// See [`Renderer::set_background_uploads`](super::Renderer::set_background_uploads).
    pub(super) fn set_background_uploads(&mut self, on: bool) {
        if on && self.uploader.is_none() {
            self.uploader = Some(super::uploads::Uploader::start(
                self.device.clone(),
                self.queue.clone(),
                (self.mesh_bytes.clone(), self.texture_bytes.clone()),
                self.skinning.clone(),
            ));
        } else if !on {
            self.uploader = None;
        }
    }

    /// Asks the upload thread for every mesh (with its levels of detail)
    /// and color texture of `list`'s items, drawn this frame or not, so
    /// turning the camera later finds them on the GPU. Only while
    /// background uploads are on.
    pub(super) fn preload(&mut self, list: &DrawList) {
        if self.uploader.is_none() {
            return;
        }
        let frame = self.frame;
        let chunked = list.chunks.iter().flat_map(|chunk| chunk.items.iter());
        for item in list.items.iter().chain(chunked) {
            if item.material.terrain.is_some() {
                continue;
            }
            if let Shape::Mesh(source) = &item.shape {
                let levels = (1..=source.lod_count()).filter_map(|level| source.lod(level));
                for mesh in std::iter::once(source).chain(levels) {
                    if !self.custom_meshes.contains_key(&mesh.key())
                        && let Some(uploader) = &mut self.uploader
                    {
                        uploader.mesh(mesh);
                    }
                }
            }
            for source in material_textures(item).into_iter().flatten() {
                self.note_texture(source, frame, false);
            }
        }
    }

    /// How many background uploads are still on their way.
    pub(super) fn pending_uploads(&self) -> usize {
        self.uploader
            .as_ref()
            .map_or(0, |uploader| uploader.pending())
    }

    /// Takes in what the background uploads finished, and returns the keys
    /// of the textures among them: materials that drew white for them are
    /// made again.
    fn finish_background_uploads(&mut self) -> Vec<u64> {
        let Some(uploader) = &mut self.uploader else {
            return Vec::new();
        };
        let (meshes, textures) = uploader.take_finished();
        let frame = self.frame;
        for (key, gpu) in meshes {
            self.custom_meshes.insert(
                key,
                Cached {
                    gpu,
                    last_drawn: frame,
                },
            );
        }
        let arrived = textures.iter().map(|(key, _)| *key).collect();
        for (key, gpu) in textures {
            self.textures.insert(
                key,
                Cached {
                    gpu,
                    last_drawn: frame,
                },
            );
        }
        arrived
    }

    /// Frees the callers' meshes, textures and materials that haven't been
    /// drawn lately.
    pub(super) fn forget_undrawn(&mut self) {
        let frame = self.frame;
        // The first frame of the last `KEEP_UNDRAWN_SECONDS`.
        let now = std::time::Instant::now();
        self.frame_clock.push_back((frame, now));
        while self
            .frame_clock
            .front()
            .is_some_and(|&(_, at)| now.duration_since(at).as_secs_f32() > KEEP_UNDRAWN_SECONDS)
        {
            self.frame_clock.pop_front();
        }
        let since = self.frame_clock.front().map_or(frame, |&(first, _)| first);
        let recent =
            |last_drawn: u64| frame - last_drawn < KEEP_UNDRAWN_FRAMES || last_drawn >= since;
        self.custom_meshes
            .retain(|_, cached| recent(cached.last_drawn));
        self.textures.retain(|_, cached| recent(cached.last_drawn));
        self.full_textures
            .retain(|_, cached| recent(cached.last_drawn));
        self.ui_image_groups
            .retain(|key, _| self.full_textures.contains_key(key));
        self.sprites.forget(|key| self.textures.contains_key(&key));
        self.decal_pass
            .forget(|key| self.textures.contains_key(&key));
        self.materials.retain(|_, cached| recent(cached.last_drawn));
        self.terrain.forget(frame);
        self.surface_shaders.forget(frame);
    }

    /// Replaces the instance buffer with a bigger one if `count` won't fit.
    pub(super) fn ensure_instance_capacity(&mut self, count: usize) {
        if count <= self.instance_capacity {
            return;
        }
        self.instance_capacity = count.next_power_of_two();
        self.instance_buffer = create_instance_buffer(&self.device, self.instance_capacity);
    }

    pub(super) fn ensure_ui_capacity(&mut self, count: usize) {
        if count <= self.ui_capacity {
            return;
        }
        self.ui_capacity = count.next_power_of_two();
        self.ui_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("UI vertices"),
            size: (self.ui_capacity * size_of::<UiVertex>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
    }
}

/// Begins a scene pass into `targets`: clearing them, or (`load`) carrying
/// on with what an earlier scene pass drew. With MSAA its samples are
/// averaged into the HDR texture as it ends. `keep` keeps the samples and
/// depth for a later pass (water's copy reads the depth); otherwise they're
/// thrown away.
/// More shadow casters a frame than this and the far cascades take turns
/// (see [`ScenePipeline::schedule_cascades`]).
const HEAVY_SHADOW_CASTERS: usize = 12000;

/// Which pass a batch is being drawn in (see `ScenePipeline::bind_batch`).
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum BatchPass {
    Scene,
    /// The cut-out surfaces' depth, drawn first in the scene pass.
    DepthFill,
    Prepass,
}

/// The GPU copy of a mesh among `meshes` and `custom_meshes`, or `None` for
/// a caller's mesh that was never uploaded.
pub(super) fn gpu_mesh_in<'a>(
    meshes: &'a Meshes,
    custom_meshes: &'a FxHashMap<u64, Cached<GpuMesh>>,
    key: MeshKey,
) -> Option<&'a GpuMesh> {
    match key {
        MeshKey::Custom(key) => custom_meshes
            .get(&key)
            .and_then(|cached| cached.gpu.as_ref()),
        builtin => meshes.get(builtin),
    }
}

/// The box a mesh spans, in its own space, once its `skin` (if any) moves
/// its corners. A caller's mesh that was never uploaded isn't drawn, so the
/// unit cube does for it.
fn item_bounds(
    meshes: &Meshes,
    custom_meshes: &FxHashMap<u64, Cached<GpuMesh>>,
    key: MeshKey,
    skin: Option<&Skin>,
) -> Bounds {
    let mesh = gpu_mesh_in(meshes, custom_meshes, key);
    match (mesh, skin) {
        (
            Some(GpuMesh {
                skin: Some(gpu), ..
            }),
            Some(skin),
        ) => gpu.bounds(skin),
        _ => mesh.map_or(Bounds::UNIT, |mesh| mesh.bounds),
    }
}

fn begin_scene_pass<'a>(
    encoder: &'a mut wgpu::CommandEncoder,
    targets: &'a FrameTargets,
    load: bool,
    keep: bool,
) -> wgpu::RenderPass<'a> {
    let (color_load, depth_load) = if load {
        (wgpu::LoadOp::Load, wgpu::LoadOp::Load)
    } else {
        (
            wgpu::LoadOp::Clear(wgpu::Color::BLACK),
            wgpu::LoadOp::Clear(1.0),
        )
    };
    let kept = if keep {
        wgpu::StoreOp::Store
    } else {
        wgpu::StoreOp::Discard
    };
    let color_attachment = match &targets.msaa {
        Some(msaa) => wgpu::RenderPassColorAttachment {
            depth_slice: None,
            view: &msaa.view,
            resolve_target: Some(&targets.hdr_view),
            ops: wgpu::Operations {
                load: color_load,
                // Only the averaged result is needed, unless a later pass
                // carries on drawing.
                store: kept,
            },
        },
        None => wgpu::RenderPassColorAttachment {
            depth_slice: None,
            view: &targets.hdr_view,
            resolve_target: None,
            ops: wgpu::Operations {
                load: color_load,
                store: wgpu::StoreOp::Store,
            },
        },
    };
    encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("scene"),
        color_attachments: &[Some(color_attachment)],
        depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
            view: &targets.depth_view,
            depth_ops: Some(wgpu::Operations {
                load: depth_load,
                store: kept,
            }),
            stencil_ops: None,
        }),
        ..Default::default()
    })
}

/// The scene pass's sky pipeline, its pipelines for items and for their
/// outlines, for drawing with `samples` per pixel, with `layouts` the
/// frame's, the material's and the skin's bindings.
fn scene_pipelines(
    device: &wgpu::Device,
    layouts: [&wgpu::BindGroupLayout; 3],
    samples: u32,
) -> (
    wgpu::RenderPipeline,
    ItemPipelines,
    wgpu::RenderPipeline,
    PrefillPipelines,
) {
    let frame_layout = layouts[0];
    let shader = shader_module(device, "scene", gpu::SCENE_SHADER);
    let pipeline = |transparent, double_sided| {
        create_pipeline(
            device,
            &shader,
            ("vs_main", "fs_main"),
            &layouts,
            (HDR_FORMAT, samples),
            (transparent, double_sided),
        )
    };
    let prefill = |fragment, double_sided, depth| {
        create_pipeline_for(
            device,
            &shader,
            ("vs_main", fragment),
            &layouts,
            (HDR_FORMAT, samples),
            (false, double_sided),
            depth,
        )
    };
    // With one sample per pixel there's nothing for coverage to share out.
    let depth_fill = if samples > 1 {
        "fs_depth_coverage"
    } else {
        "fs_depth_only"
    };
    // Built at once on threads of their own: each compiles its shaders for
    // the GPU (on DirectX 12 through DXC, which takes a while for this
    // big shader), and wgpu lets pipelines be made from any thread.
    let [
        sky,
        opaque,
        transparent,
        double_sided,
        double_sided_transparent,
        outline,
        depth,
        depth_double,
        lit,
        lit_double,
    ] = gpu::in_parallel([
        &|| create_sky_pipeline(device, frame_layout, samples),
        &|| pipeline(false, false),
        &|| pipeline(true, false),
        &|| pipeline(false, true),
        &|| pipeline(true, true),
        &|| {
            create_pipeline(
                device,
                &shader,
                ("vs_outline", "fs_outline"),
                &layouts,
                (HDR_FORMAT, samples),
                (false, true),
            )
        },
        &|| prefill(depth_fill, false, DepthUse::Fill),
        &|| prefill(depth_fill, true, DepthUse::Fill),
        &|| prefill("fs_prefilled", false, DepthUse::Filled),
        &|| prefill("fs_prefilled", true, DepthUse::Filled),
    ]);
    (
        sky,
        ItemPipelines {
            opaque,
            transparent,
            double_sided,
            double_sided_transparent,
        },
        outline,
        PrefillPipelines {
            depth: [depth, depth_double],
            lit: [lit, lit_double],
        },
    )
}

/// The frame's bindings (group 0 in lighting.wgsl), as the scene's shaders
/// see them.
fn frame_layout_entries() -> Vec<wgpu::BindGroupLayoutEntry> {
    let storage_entry = |binding| wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Storage { read_only: true },
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    };
    vec![
        wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        },
        wgpu::BindGroupLayoutEntry {
            binding: 1,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Depth,
                view_dimension: wgpu::TextureViewDimension::D2Array,
                multisampled: false,
            },
            count: None,
        },
        wgpu::BindGroupLayoutEntry {
            binding: 2,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Comparison),
            count: None,
        },
        storage_entry(3),
        storage_entry(4),
        storage_entry(16),
        storage_entry(17),
        skinning::joints_entry(),
        wgpu::BindGroupLayoutEntry {
            binding: 6,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Depth,
                view_dimension: wgpu::TextureViewDimension::D2Array,
                multisampled: false,
            },
            count: None,
        },
        wgpu::BindGroupLayoutEntry {
            binding: 7,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        },
        wgpu::BindGroupLayoutEntry {
            binding: 8,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
            count: None,
        },
        wgpu::BindGroupLayoutEntry {
            binding: 10,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: wgpu::TextureViewDimension::Cube,
                multisampled: false,
            },
            count: None,
        },
        wgpu::BindGroupLayoutEntry {
            binding: 11,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
            count: None,
        },
        wgpu::BindGroupLayoutEntry {
            binding: 12,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        },
        wgpu::BindGroupLayoutEntry {
            binding: 13,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: wgpu::TextureViewDimension::CubeArray,
                multisampled: false,
            },
            count: None,
        },
        wgpu::BindGroupLayoutEntry {
            binding: 14,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Depth,
                view_dimension: wgpu::TextureViewDimension::D2Array,
                multisampled: false,
            },
            count: None,
        },
        wgpu::BindGroupLayoutEntry {
            binding: 15,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Depth,
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        },
    ]
}

/// The scene's samples per pixel, whether FXAA needs its target, whether
/// ambient occlusion needs the prepass's and its own (contact shadows and
/// volumetric fog need only the prepass's depth, and screen-space outlines,
/// the draw list's `edges`, its depth and normals), and whether depth of
/// field (`wants_depth_of_field`, if its setting allows) needs its target
/// and the prepass's depth, and which motion targets TAA and motion blur
/// (`wants_motion_blur`, if its setting allows) need, with the prepass's.
fn targets_used(
    graphics: &EffectiveGraphics,
    edges: bool,
    wants_depth_of_field: bool,
    wants_motion_blur: bool,
) -> (u32, bool, bool, bool, temporal::TemporalUse) {
    let anti_aliasing = graphics.anti_aliasing;
    let depth_of_field =
        wants_depth_of_field && graphics.depth_of_field != crate::DepthOfFieldQuality::Off;
    let temporal = temporal::TemporalUse::of(graphics, wants_motion_blur);
    (
        anti_aliasing.samples(),
        anti_aliasing == crate::AntiAliasing::Fxaa,
        graphics.ambient_occlusion != crate::AmbientOcclusion::Off
            || ray_ao::traced(graphics)
            || graphics.contact_shadows
            || graphics.volumetric_fog
            || graphics.ray_traced_shadows != crate::RayTracedShadows::Off
            || edges
            || depth_of_field
            || temporal.any(),
        depth_of_field,
        temporal,
    )
}

/// A 1×1 white occlusion texture: nothing hidden.
fn create_white_occlusion(device: &wgpu::Device, queue: &wgpu::Queue) -> wgpu::TextureView {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("no ambient occlusion"),
        size: wgpu::Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: OCCLUSION_FORMAT,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    queue.write_texture(
        texture.as_image_copy(),
        &[255],
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(1),
            rows_per_image: Some(1),
        },
        wgpu::Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 1,
        },
    );
    texture.create_view(&wgpu::TextureViewDescriptor::default())
}

/// The scene's size in pixels for an `output` of that size: scaled by the
/// render scale, at least 1, and at most what the GPU's textures can hold.
pub(super) fn scene_size(
    (width, height): (u32, u32),
    graphics: &EffectiveGraphics,
    capabilities: &Capabilities,
) -> (u32, u32) {
    let scaled = |pixels: u32| {
        ((pixels as f32 * graphics.render_scale).round() as u32)
            .clamp(1, capabilities.max_texture_size.max(1))
    };
    (scaled(width), scaled(height))
}

/// The glyphon bounds of one copy of `text`, `offset` pixels from it: its
/// box, moved with the copy so a shadow is cut where its letters are.
pub(super) fn text_bounds(text: &UiText, [dx, dy]: [f32; 2]) -> TextBounds {
    TextBounds {
        left: (text.position[0] + dx).floor() as i32,
        top: (text.position[1] + dy).floor() as i32,
        right: (text.position[0] + dx + text.size[0].max(0.0)).ceil() as i32,
        bottom: (text.position[1] + dy + text.size[1].max(0.0)).ceil() as i32,
    }
}

/// The shadow passes' bindings: the `frame` uniform, each view's matrix in
/// `cascades` (picked by a dynamic offset), and the frame's `joints`.
fn create_shadow_bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    frame: &wgpu::Buffer,
    cascades: &wgpu::Buffer,
    joints: &wgpu::Buffer,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("shadow pass"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: frame.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 5,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: cascades,
                    offset: 0,
                    size: wgpu::BufferSize::new(size_of::<[[f32; 4]; 4]>() as u64),
                }),
            },
            wgpu::BindGroupEntry {
                binding: skinning::JOINTS_BINDING,
                resource: joints.as_entire_binding(),
            },
        ],
    })
}
