use super::*;

impl ScenePipeline {
    pub(super) fn new(
        device: wgpu::Device,
        queue: wgpu::Queue,
        color_format: wgpu::TextureFormat,
        width: u32,
        height: u32,
        capabilities: Capabilities,
    ) -> Self {
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
        let skinning = Skinning::new(&device);
        let frame_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("frame uniform"),
            size: size_of::<FrameUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let frame_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("frame"),
            entries: &[
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
            ],
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
        let (empty_shadow_view, _) = create_shadow_views(&device, "no shadow map", 1, 1);
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
        let frame_bind_group = create_frame_bind_group(
            &device,
            &frame_layout,
            [&frame_buffer, &light_buffer, &tile_buffer, &skinning.joints],
            [
                &empty_shadow_view,
                &empty_shadow_view,
                &no_occlusion_view,
                environment_view,
                sky_image_view,
                probes.view(),
            ],
            [&shadow_sampler, &occlusion_sampler, &environment.sampler],
        );
        let cascade_size = size_of::<[[f32; 4]; 4]>() as u64;
        let cascade_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("shadow views"),
            size: CASCADE_UNIFORM_STRIDE * (SHADOW_CASCADES + MAX_SPOT_SHADOWS) as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        // Binding 5 matches `shadow_cascade` in shader.wgsl. Each pass picks
        // its cascade with an offset into the buffer (a "dynamic offset").
        // Binding 9 is the frame's joints, as in the frame's group.
        let shadow_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("shadow pass"),
            entries: &[
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
        let shadow_bind_group =
            create_shadow_bind_group(&device, &shadow_layout, &cascade_buffer, &skinning.joints);
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
            mipmap_filter: wgpu::FilterMode::Linear,
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
        let (sky_pipeline, item_pipelines) = scene_pipelines(
            &device,
            [&frame_layout, &material_layout, &skinning.layout],
            samples,
        );
        let mut terrain =
            terrain::TerrainPass::new(&device, &frame_layout, texture_bytes.clone(), samples);
        terrain.set_skipped_levels(graphics.texture_quality.skipped_levels());
        let sprites = sprites::SpritePass::new(&device, &white_texture.srgb, samples);
        let post = PostPasses::new(&device, color_format);
        let occlusion =
            OcclusionPasses::new(&device, [&frame_layout, &material_layout, &skinning.layout]);
        let targets_alive = TargetsAlive::default();
        let frame_targets = FrameTargets::new(
            &device,
            (&post, &occlusion),
            scene_size((width, height), &graphics, &capabilities),
            (width, height),
            targets_used(&graphics),
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
            sprites,
            post,
            frame_targets,
            occlusion,
            environment,
            probes,
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
            ui_image_groups: HashMap::new(),
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
            frame_layout,
            empty_shadow_view,
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
            custom_meshes: HashMap::new(),
            textures: HashMap::new(),
            full_textures: HashMap::new(),
            materials: HashMap::new(),
            terrain,
            frame: 0,
            gpu_timer,
            counts: FrameCounts::default(),
            frozen_view: None,
            culling_bounds: Vec::new(),
            lods: LodStates::default(),
            texture_bytes,
            mesh_bytes,
        };
        pipeline.bind_frame();
        pipeline
    }

    /// Uses `settings` from the next frame, lowered to what the device can
    /// do. A shadow map (the sun's or the spot lights') of the wrong size is
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
        };
        let settings = self.settings.clone();
        self.apply_graphics(&settings);
        self.materials.clear();
        self.textures.clear();
        self.sprites.forget(|_| false);
    }

    /// Uses `settings` from the next frame (see [`Self::set_graphics`]).
    fn apply_graphics(&mut self, settings: &GraphicsSettings) {
        let old_quality = self.graphics.texture_quality;
        let old_samples = self.graphics.anti_aliasing.samples();
        self.settings = settings.clone();
        self.graphics = settings.effective(&self.capabilities);
        let quality = self.graphics.shadow_quality;
        if self.shadow_map.as_ref().map(|map| map.size) != quality.map() {
            self.shadow_map = None;
        }
        if self.spot_shadow_map.as_ref().map(|map| map.size) != quality.spot_map() {
            self.spot_shadow_map = None;
        }
        self.bind_frame();
        let samples = self.graphics.anti_aliasing.samples();
        if samples != old_samples {
            (self.sky_pipeline, self.item_pipelines) = scene_pipelines(
                &self.device,
                [
                    &self.frame_layout,
                    &self.material_layout,
                    &self.skinning.layout,
                ],
                samples,
            );
            self.terrain
                .set_samples(&self.device, &self.frame_layout, samples);
            self.sprites.set_samples(&self.device, samples);
        }
        self.fit_targets();
        if self.graphics.texture_quality != old_quality {
            self.materials.clear();
            self.textures.clear();
            self.sprites.forget(|_| false);
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
    /// size, render scale, anti-aliasing and ambient occlusion. The old ones
    /// are freed.
    fn fit_targets(&mut self) {
        let size = scene_size(self.output_size, &self.graphics, &self.capabilities);
        let used = targets_used(&self.graphics);
        let targets = &self.frame_targets;
        let made_for = (
            targets.size,
            targets.output,
            (
                targets.samples,
                targets.fxaa.is_some(),
                targets.occlusion.is_some(),
            ),
        );
        if made_for != (size, self.output_size, used) {
            self.frame_targets = FrameTargets::new(
                &self.device,
                (&self.post, &self.occlusion),
                size,
                self.output_size,
                used,
                &self.targets_alive,
            );
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
        // Lights are planned in the scene's pixels, which the render scale
        // makes fewer or more than the output's. UI uses the output's.
        let spot_map = self.graphics.shadow_quality.spot_map();
        let planned_lights = plan_lights(list, aspect, self.frame_targets.size, spot_map);
        // The sky's maps are baked here only when the sky has changed.
        if self.environment.prepare(
            &self.device,
            &self.queue,
            &list.sky,
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
        let mut frame_uniform = frame_uniform(list, aspect, &self.graphics);
        (frame_uniform.environment, frame_uniform.sky_light) = self.environment.uniform();
        let (probes, probe_count) = self.probes.uniform();
        frame_uniform.probes = probes;
        frame_uniform.reflection_probes[0] = probe_count;
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
        self.queue
            .write_buffer(&self.frame_buffer, 0, bytemuck::bytes_of(&frame_uniform));
        // The sun's cascades, then the spot lights' views.
        let shadow_views = frame_uniform
            .cascades
            .iter()
            .chain(&frame_uniform.spot_shadows);
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
        if self.frame_targets.occlusion.is_some() {
            let uniform = occlusion_uniform(list, aspect, self.graphics.ambient_occlusion);
            self.queue
                .write_buffer(&self.occlusion.uniform, 0, bytemuck::bytes_of(&uniform));
        }
        let post = post_drawn(&list.post, &self.graphics);
        self.queue.write_buffer(
            &self.post.settings,
            0,
            bytemuck::bytes_of(&post_uniform(&post)),
        );

        self.frame += 1;
        // Each item's level of detail, which decides the meshes uploaded.
        let Resolved {
            draws,
            culled: past_cull_distance,
        } = self.lods.resolve(list, self.frame);
        self.upload_new(list, &draws);
        // What the camera can see, or the frozen view's while it's set.
        let camera = self.frozen_view.as_ref().unwrap_or(&list.camera);
        let camera_view = Frustum::of(camera_view_projection(camera, aspect));
        let mut seen = std::mem::take(&mut self.culling_bounds);
        seen.clear();
        let DrawPlan {
            mut instances,
            batches,
            casters,
            culled,
            joints,
        } = plan_draws(
            list,
            &draws,
            &camera_view,
            |mesh, skin| self.item_bounds(mesh, skin),
            self.frozen_view.is_some().then_some(&mut seen),
        );
        self.culling_bounds = seen;
        let joints_remade = self
            .skinning
            .write_joints(&self.device, &self.queue, &joints);
        // Shadow maps cost memory, so one exists only while shadows are on,
        // at the size the graphics settings give.
        let map_size = shadows_drawn(list.shadows, &self.graphics).map(|(_, size)| size);
        // Each shadow view's casters are copied after the scene's instances:
        // the sun's cascades, each a box reaching back toward the sun, then
        // the spot lights' views, each near enough to its beam.
        let cascades = map_size.map_or(0, |size| size.cascades as usize);
        let mut casters_by_view: Vec<_> = frame_uniform.cascades[..cascades]
            .iter()
            .map(|matrix| {
                let view = Frustum::of(glam::Mat4::from_cols_array_2d(matrix));
                casters_in(&mut instances, &casters, |sphere| view.touches(sphere))
            })
            .collect();
        for spot in &planned_lights.spot_shadows {
            let view = Frustum::of(spot.view_proj);
            casters_by_view.push(casters_in(&mut instances, &casters, |sphere| {
                spheres_touch(spot.bounds, sphere) && view.touches(sphere)
            }));
        }
        self.ensure_instance_capacity(instances.len());
        self.queue
            .write_buffer(&self.instance_buffer, 0, bytemuck::cast_slice(&instances));
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
        let (sprite_draws, sprites_culled) = {
            let textures = &self.textures;
            self.sprites.prepare(
                &self.device,
                &self.queue,
                list,
                view_projection(list, aspect),
                &camera_view,
                |key| {
                    textures
                        .get(&key)
                        .and_then(|cached| cached.gpu.as_ref())
                        .map(|gpu| &gpu.srgb)
                },
            )
        };

        // The spot lights' shadow map exists only while one of them casts
        // shadows.
        let spot_map = spot_map.filter(|_| !planned_lights.spot_shadows.is_empty());
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
        if changed || joints_remade {
            self.bind_frame();
        }

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("frame"),
            });
        // Times this frame unless the last measurement is still being read.
        let timed = match &mut self.gpu_timer {
            Some(timer) => timer.collect(&self.device),
            None => false,
        };
        let timer = self.gpu_timer.as_ref().filter(|_| timed);
        let mark = |encoder: &mut wgpu::CommandEncoder, pass| {
            if let Some(timer) = timer {
                timer.mark(encoder, pass);
            }
        };
        mark(&mut encoder, FramePass::Shadows);
        let mut counts = FrameCounts {
            items_submitted: list.items.len() as u32,
            items_culled: culled + past_cull_distance,
            shadow_casters_culled: casters_by_view.iter().map(|(_, culled)| culled).sum(),
            sprites_culled,
            ..FrameCounts::default()
        };
        let mut shadow_casters_drawn = 0;
        let mut count = |index_count: u32, instances: &Range<u32>| {
            counts.draw_calls += 1;
            counts.triangles +=
                u64::from(index_count / 3) * u64::from(instances.end - instances.start);
        };

        // One pass per cascade, each drawing into its own layer of the map,
        // then one per shadow-casting spot light, each drawing the casters
        // that can reach its view. Each view's matrix is at its index in
        // `cascade_buffer`, the spot lights' after every cascade's.
        let (cascade_casters, spot_casters) = casters_by_view.split_at(cascades);
        let cascades = self
            .shadow_map
            .iter()
            .flat_map(|map| map.layers.iter().zip(cascade_casters).enumerate());
        let spots = self
            .spot_shadow_map
            .iter()
            .flat_map(|map| map.layers.iter().zip(spot_casters).enumerate())
            .map(|(index, view)| (SHADOW_CASCADES + index, view));
        for (index, (layer, (casters, _))) in cascades.chain(spots) {
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
                count(mesh.index_count, instances);
                shadow_casters_drawn += instances.end - instances.start;
                pass.draw_indexed(0..mesh.index_count, 0, instances.clone());
            }
        }
        let frame_bind_group = &self.frame_bind_group;
        mark(&mut encoder, FramePass::Prepass);

        // While ambient occlusion is on: solid items' depth and normals,
        // then how hidden each pixel is by what's around it.
        if let Some(occlusion) = &self.frame_targets.occlusion {
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("depth and normal prepass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
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
                pass.set_bind_group(0, frame_bind_group, &[]);
                pass.set_vertex_buffer(1, self.instance_buffer.slice(..));
                for batch in batches.iter().filter(|batch| !batch.transparent) {
                    let Some(mesh) = self.bind_batch(&mut pass, batch, true) else {
                        continue;
                    };
                    count(mesh.index_count, &batch.instances);
                    pass.draw_indexed(0..mesh.index_count, 0, batch.instances.clone());
                }
            }
            mark(&mut encoder, FramePass::AmbientOcclusion);
            self.occlusion.draw(&mut encoder, occlusion);
        } else {
            mark(&mut encoder, FramePass::AmbientOcclusion);
        }
        mark(&mut encoder, FramePass::Scene);

        // The scene is drawn into the HDR texture; post-processing then brings
        // it to `view`. With MSAA it's drawn into the multisampled texture,
        // whose samples are averaged into the HDR one as the pass ends. The
        // pass is a block so it ends (is dropped) before `encoder.finish()`,
        // which needs the encoder back.
        {
            let targets = &self.frame_targets;
            let color_attachment = match &targets.msaa {
                Some(msaa) => wgpu::RenderPassColorAttachment {
                    view: &msaa.view,
                    resolve_target: Some(&targets.hdr_view),
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        // Only the averaged result is needed.
                        store: wgpu::StoreOp::Discard,
                    },
                },
                None => wgpu::RenderPassColorAttachment {
                    view: &targets.hdr_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                },
            };
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("scene"),
                color_attachments: &[Some(color_attachment)],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.frame_targets.depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Discard,
                    }),
                    stencil_ops: None,
                }),
                ..Default::default()
            });

            pass.set_bind_group(0, frame_bind_group, &[]);
            pass.set_pipeline(&self.sky_pipeline);
            pass.draw(0..3, 0..1);
            pass.set_vertex_buffer(1, self.instance_buffer.slice(..));
            let mut drawn = (0, 0);
            for batch in batches {
                let Some(mesh) = self.bind_batch(&mut pass, &batch, false) else {
                    continue;
                };
                count(mesh.index_count, &batch.instances);
                drawn.0 += batch.instances.end - batch.instances.start;
                drawn.1 += 1;
                pass.draw_indexed(0..mesh.index_count, 0, batch.instances);
            }
            (counts.items_drawn, counts.batches) = drawn;
            // Last, over everything, but hidden behind solid items.
            self.sprites.draw(&mut pass, &sprite_draws);
        }
        mark(&mut encoder, FramePass::PostProcessing);

        self.post.draw(
            &mut encoder,
            &self.frame_targets,
            view,
            post.enabled && post.bloom_enabled && post.bloom_intensity > 0.0,
        );

        mark(&mut encoder, FramePass::Ui);
        if turned.drawn
            && let Some((sheet, _, _)) = &self.turned_text.sheet
        {
            let sheet_view = sheet.create_view(&wgpu::TextureViewDescriptor::default());
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("turned UI text"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
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
            let shadows = self.shadow_map.is_some() || self.spot_shadow_map.is_some();
            let occlusion = self.frame_targets.occlusion.is_some();
            timer.submitted([
                shadows,
                occlusion,
                occlusion,
                true,
                true,
                turned.drawn || ui_drawn,
            ]);
        }
        counts.shadow_casters_drawn = shadow_casters_drawn;
        self.counts = counts;
        self.forget_undrawn();
        Ok(())
    }

    /// Sets `pass` up to draw `batch`, in the scene pass or (`prepass`) the
    /// depth and normal prepass: its pipeline, its material or terrain
    /// binding, and its mesh, which is returned. `None` skips it: a
    /// caller's mesh with broken data was never uploaded.
    fn bind_batch(
        &self,
        pass: &mut wgpu::RenderPass<'_>,
        batch: &Batch,
        prepass: bool,
    ) -> Option<&GpuMesh> {
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
            let pipeline = match (prepass, batch.double_sided) {
                (false, _) => self.item_pipelines.of(batch),
                (true, false) => &self.occlusion.prepass,
                (true, true) => &self.occlusion.double_sided_prepass,
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
    fn skin_group<'a>(&'a self, mesh: &'a GpuMesh) -> &'a wgpu::BindGroup {
        mesh.skin
            .as_ref()
            .map_or(&self.skinning.none, |skin| &skin.bind_group)
    }

    /// Remakes the frame bindings with the shadow maps and ambient
    /// occlusion alive, binding the empty stand-in for each that isn't, so
    /// a freed map or target isn't kept alive by the bindings, and with
    /// the frame's joints (the shadow passes' bindings too).
    fn bind_frame(&mut self) {
        let empty = &self.empty_shadow_view;
        let sun = self.shadow_map.as_ref().map_or(empty, |map| &map.array);
        let spot = self
            .spot_shadow_map
            .as_ref()
            .map_or(empty, |map| &map.array);
        let occlusion = self
            .frame_targets
            .occlusion
            .as_ref()
            .map_or(&self.no_occlusion_view, |targets| &targets.blurred_view);
        let (environment, sky_image) = self.environment.views();
        let probes = self.probes.view();
        self.frame_bind_group = create_frame_bind_group(
            &self.device,
            &self.frame_layout,
            [
                &self.frame_buffer,
                &self.light_buffer,
                &self.tile_buffer,
                &self.skinning.joints,
            ],
            [sun, spot, occlusion, environment, sky_image, probes],
            [
                &self.shadow_sampler,
                &self.occlusion_sampler,
                &self.environment.sampler,
            ],
        );
        self.shadow_bind_group = create_shadow_bind_group(
            &self.device,
            &self.shadow_layout,
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
            + self.spot_shadow_map.as_ref().map_or(0, |map| map.bytes);
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
        ];
        RenderStats {
            draw_calls: counts.draw_calls,
            triangles: counts.triangles,
            gpu_time: self.gpu_timer.as_ref().and_then(gpu_timer::GpuTimer::last),
            gpu_timing: self.gpu_timer.is_some(),
            pass_times: self
                .gpu_timer
                .as_ref()
                .map_or([None; FramePass::COUNT], gpu_timer::GpuTimer::last_passes),
            items_submitted: counts.items_submitted,
            items_drawn: counts.items_drawn,
            batches: counts.batches,
            items_culled: counts.items_culled,
            shadow_casters_drawn: counts.shadow_casters_drawn,
            shadow_casters_culled: counts.shadow_casters_culled,
            sprites_culled: counts.sprites_culled,
            texture_bytes: self.texture_bytes.total(),
            mesh_bytes: self.mesh_bytes.total(),
            target_bytes: self.frame_targets.bytes
                + shadow_bytes
                + sheet_bytes
                + self.environment.bytes()
                + self.probes.bytes(),
            buffer_bytes: buffers.iter().map(|buffer| buffer.size()).sum(),
        }
    }

    /// The box a mesh's corners span, for culling. A caller's mesh that
    /// was never uploaded isn't drawn, so the unit cube does for it.
    pub(super) fn mesh_bounds(&self, key: MeshKey) -> Bounds {
        self.gpu_mesh(key).map_or(Bounds::UNIT, |mesh| mesh.bounds)
    }

    /// The box an item's mesh spans, in its own space, once its `skin` (if
    /// any) moves its corners.
    pub(super) fn item_bounds(&self, key: MeshKey, skin: Option<&Skin>) -> Bounds {
        match (self.gpu_mesh(key), skin) {
            (
                Some(GpuMesh {
                    skin: Some(gpu), ..
                }),
                Some(skin),
            ) => gpu.bounds(skin),
            _ => self.mesh_bounds(key),
        }
    }

    /// The GPU copy of a mesh, or `None` for a caller's mesh that was never uploaded.
    pub(super) fn gpu_mesh(&self, key: MeshKey) -> Option<&GpuMesh> {
        match key {
            MeshKey::Custom(key) => self
                .custom_meshes
                .get(&key)
                .and_then(|cached| cached.gpu.as_ref()),
            builtin => self.meshes.get(builtin),
        }
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
    pub(super) fn upload_new(&mut self, list: &DrawList, draws: &[Draw]) {
        let frame = self.frame;
        for source in draws.iter().filter_map(Draw::source) {
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
        for item in &list.items {
            if let Some(surface) = &item.material.terrain {
                for weights in &surface.weights {
                    self.note_texture(weights.as_ref(), frame, true);
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
            for source in material_textures(item).iter().flatten() {
                self.note_texture(source.as_ref(), frame, false);
            }
        }
        for item in &list.ui {
            if let UiDrawItem::Image(image) = item
                && let Some(source) = &image.texture
            {
                self.note_texture(source.as_ref(), frame, true);
            }
        }
        for sprite in &list.sprites {
            if let Some(source) = &sprite.texture {
                self.note_texture(source.as_ref(), frame, false);
            }
        }
        let replaced = self.fit_texture_budget(list);
        if !replaced.is_empty() {
            self.materials.retain(|key, _| {
                !key.textures
                    .iter()
                    .flatten()
                    .any(|texture| replaced.contains(texture))
            });
            self.sprites.forget(|key| !replaced.contains(&key));
        }
        for item in &list.items {
            if item.material.terrain.is_some() {
                continue;
            }
            let sources = material_textures(item);
            let key = material_key(item);
            if key == MaterialKey::default() {
                continue;
            }
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
    pub(super) fn note_texture(&mut self, source: &dyn TextureSource, frame: u64, full: bool) {
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
                slot.insert(Cached {
                    gpu: upload_texture(
                        &self.device,
                        &self.queue,
                        &self.texture_bytes,
                        source,
                        (skipped_levels, self.capabilities.max_texture_size),
                        0,
                    ),
                    last_drawn: frame,
                });
            }
        }
    }

    /// Frees the callers' meshes, textures and materials that haven't been
    /// drawn lately.
    pub(super) fn forget_undrawn(&mut self) {
        let frame = self.frame;
        let recent = |last_drawn: u64| frame - last_drawn < KEEP_UNDRAWN_FRAMES;
        self.custom_meshes
            .retain(|_, cached| recent(cached.last_drawn));
        self.textures.retain(|_, cached| recent(cached.last_drawn));
        self.full_textures
            .retain(|_, cached| recent(cached.last_drawn));
        self.ui_image_groups
            .retain(|key, _| self.full_textures.contains_key(key));
        self.sprites.forget(|key| self.textures.contains_key(&key));
        self.materials.retain(|_, cached| recent(cached.last_drawn));
        self.terrain.forget(frame);
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

/// The scene pass's sky pipeline and its pipelines for items, for drawing
/// with `samples` per pixel.
/// The sky's and items' pipelines, with `layouts` the frame's, the
/// material's and the skin's bindings.
fn scene_pipelines(
    device: &wgpu::Device,
    layouts: [&wgpu::BindGroupLayout; 3],
    samples: u32,
) -> (wgpu::RenderPipeline, ItemPipelines) {
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
    (
        create_sky_pipeline(device, frame_layout, samples),
        ItemPipelines {
            opaque: pipeline(false, false),
            transparent: pipeline(true, false),
            double_sided: pipeline(false, true),
            double_sided_transparent: pipeline(true, true),
        },
    )
}

/// The scene's samples per pixel, whether FXAA needs its target, and
/// whether ambient occlusion needs the prepass's and its own.
fn targets_used(graphics: &EffectiveGraphics) -> (u32, bool, bool) {
    let anti_aliasing = graphics.anti_aliasing;
    (
        anti_aliasing.samples(),
        anti_aliasing == crate::AntiAliasing::Fxaa,
        graphics.ambient_occlusion != crate::AmbientOcclusion::Off,
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

/// The shadow passes' bindings: each view's matrix in `cascades` (picked
/// by a dynamic offset), and the frame's `joints`.
fn create_shadow_bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    cascades: &wgpu::Buffer,
    joints: &wgpu::Buffer,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("shadow pass"),
        layout,
        entries: &[
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
