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
        let depth_view = create_depth_view(&device, width, height);
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
        let (empty_shadow_view, _) = create_shadow_views(
            &device,
            ShadowMapSize {
                size: 1,
                cascades: 1,
            },
        );
        // The bind group keeps the stand-in 1×1 map alive while shadows are off.
        let frame_bind_group = create_frame_bind_group(
            &device,
            &frame_layout,
            [&frame_buffer, &light_buffer, &tile_buffer],
            &empty_shadow_view,
            &shadow_sampler,
        );
        let cascade_size = size_of::<[[f32; 4]; 4]>() as u64;
        let cascade_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("shadow cascades"),
            size: CASCADE_UNIFORM_STRIDE * SHADOW_CASCADES as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        // Binding 5 matches `shadow_cascade` in shader.wgsl. Each pass picks
        // its cascade with an offset into the buffer (a "dynamic offset").
        let shadow_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("shadow pass"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 5,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: true,
                    min_binding_size: wgpu::BufferSize::new(cascade_size),
                },
                count: None,
            }],
        });
        let shadow_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("shadow pass"),
            layout: &shadow_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 5,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: &cascade_buffer,
                    offset: 0,
                    size: wgpu::BufferSize::new(cascade_size),
                }),
            }],
        });
        let shadow_pipeline = create_shadow_pipeline(&device, &shadow_layout);

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
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::FRAGMENT,
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
        let white_texture = create_texture(&device, &queue, 1, 1, &[255; 4]);
        let plain_material = create_material(
            &device,
            &material_layout,
            &sampler,
            [&white_texture; MATERIAL_TEXTURES],
            maps_uniform_of(&MaterialKey::default()),
        );

        let scene_shader = shader_module(&device, "scene", gpu::SCENE_SHADER);
        let entry_points = ("vs_main", "fs_main");
        let layouts = [&frame_layout, &material_layout];
        let opaque_pipeline = create_pipeline(
            &device,
            &scene_shader,
            entry_points,
            &layouts,
            HDR_FORMAT,
            false,
        );
        let transparent_pipeline = create_pipeline(
            &device,
            &scene_shader,
            entry_points,
            &layouts,
            HDR_FORMAT,
            true,
        );
        let terrain = terrain::TerrainPass::new(&device, &frame_layout);
        let sky_pipeline = create_sky_pipeline(&device, &frame_layout);
        let sprites = sprites::SpritePass::new(&device, &white_texture.srgb);
        let post = PostPasses::new(&device, color_format);
        let post_targets = PostTargets::new(&device, &post, width, height);
        let ui_pipeline = create_ui_pipeline(&device, color_format);
        let (ui_image_layout, ui_sampler, ui_image_pipeline, ui_sheet_pipeline) =
            create_ui_image_pipeline(&device, color_format);
        let meshes = Meshes {
            block: upload_mesh(&device, "block", &mesh::block()),
            sphere: upload_mesh(&device, "sphere", &mesh::sphere()),
            cylinder: upload_mesh(&device, "cylinder", &mesh::cylinder()),
            wedge: upload_mesh(&device, "wedge", &mesh::wedge()),
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

        Self {
            device,
            queue,
            depth_view,
            sky_pipeline,
            opaque_pipeline,
            transparent_pipeline,
            sprites,
            post,
            post_targets,
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
            shadow_sampler,
            shadow_pipeline,
            cascade_buffer,
            shadow_bind_group,
            shadow_map: None,
            shadow_maps_alive: Arc::new(()),
            graphics: GraphicsSettings::default().effective(&capabilities),
            settings: GraphicsSettings::default(),
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
            materials: HashMap::new(),
            terrain,
            frame: 0,
            gpu_timer,
            draws: (0, 0),
        }
    }

    /// Uses `settings` from the next frame, lowered to what the device can
    /// do. A shadow map of the wrong size is freed here and remade at the
    /// right size when next drawn.
    pub(super) fn set_graphics(&mut self, settings: &GraphicsSettings) {
        if *settings == self.settings {
            return;
        }
        self.settings = settings.clone();
        self.graphics = settings.effective(&self.capabilities);
        let wanted = self.graphics.shadow_quality.map();
        if self.shadow_map.as_ref().map(|map| map.size) != wanted {
            self.shadow_map = None;
        }
    }

    /// Call when the target changes size. Sizes are in pixels and never zero.
    pub(super) fn resize(&mut self, width: u32, height: u32) {
        self.depth_view = create_depth_view(&self.device, width, height);
        self.post_targets = PostTargets::new(&self.device, &self.post, width, height);
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
        let planned_lights = plan_lights(list, aspect, viewport_size);
        let mut frame_uniform = frame_uniform(list, aspect, &self.graphics);
        frame_uniform.lights = [
            planned_lights.lights.len() as u32,
            planned_lights.tile_size,
            planned_lights.tiles_across,
            0,
        ];
        self.queue
            .write_buffer(&self.frame_buffer, 0, bytemuck::bytes_of(&frame_uniform));
        for (index, cascade) in frame_uniform.cascades.iter().enumerate() {
            self.queue.write_buffer(
                &self.cascade_buffer,
                index as u64 * CASCADE_UNIFORM_STRIDE,
                bytemuck::bytes_of(cascade),
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
        let post = post_drawn(&list.post, &self.graphics);
        self.queue.write_buffer(
            &self.post.settings,
            0,
            bytemuck::bytes_of(&post_uniform(&post)),
        );

        self.frame += 1;
        self.upload_new(list);
        let (instances, batches) = plan_draws(list);
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
        let sprite_draws = {
            let textures = &self.textures;
            self.sprites.prepare(
                &self.device,
                &self.queue,
                list,
                view_projection(list, aspect),
                |key| {
                    textures
                        .get(&key)
                        .and_then(|cached| cached.gpu.as_ref())
                        .map(|gpu| &gpu.srgb)
                },
            )
        };

        // Shadow maps cost memory, so one exists only while shadows are on,
        // at the size the graphics settings give.
        let map_size = shadows_drawn(list.shadows, &self.graphics).map(|(_, size)| size);
        if self.shadow_map.as_ref().map(|map| map.size) != map_size {
            self.shadow_map = None;
        }
        if let Some(size) = map_size
            && self.shadow_map.is_none()
        {
            let (view, layers) = create_shadow_views(&self.device, size);
            let frame_bind_group = create_frame_bind_group(
                &self.device,
                &self.frame_layout,
                [&self.frame_buffer, &self.light_buffer, &self.tile_buffer],
                &view,
                &self.shadow_sampler,
            );
            self.shadow_map = Some(ShadowMap {
                size,
                _alive: self.shadow_maps_alive.clone(),
                layers,
                frame_bind_group,
            });
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
        if timed && let Some(timer) = &self.gpu_timer {
            timer.begin(&mut encoder);
        }
        let mut draws = (0u32, 0u64);
        let mut count = |index_count: u32, instances: &Range<u32>| {
            draws.0 += 1;
            draws.1 += u64::from(index_count / 3) * u64::from(instances.end - instances.start);
        };

        // One pass per cascade, each drawing into its own layer of the map.
        let layers = self.shadow_map.iter().flat_map(|map| &map.layers);
        for (index, layer) in layers.enumerate() {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("shadow cascade"),
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
            pass.set_vertex_buffer(1, self.instance_buffer.slice(..));
            // See-through items, and parts with `CastShadow` off, cast no shadow.
            for batch in batches
                .iter()
                .filter(|batch| !batch.transparent && batch.casts_shadow)
            {
                let Some(mesh) = self.gpu_mesh(batch.mesh) else {
                    continue;
                };
                pass.set_vertex_buffer(0, mesh.vertices.slice(..));
                pass.set_index_buffer(mesh.indices.slice(..), mesh.index_format);
                count(mesh.index_count, &batch.instances);
                pass.draw_indexed(0..mesh.index_count, 0, batch.instances.clone());
            }
        }
        let frame_bind_group = self
            .shadow_map
            .as_ref()
            .map_or(&self.frame_bind_group, |map| &map.frame_bind_group);

        // The scene is drawn into the HDR texture; post-processing then brings
        // it to `view`. The pass is a block so it ends (is dropped) before
        // `encoder.finish()`, which needs the encoder back.
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("scene"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.post_targets.hdr_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth_view,
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
            for batch in batches {
                // A caller's mesh with broken data was never uploaded.
                let Some(mesh) = self.gpu_mesh(batch.mesh) else {
                    continue;
                };
                if batch.material.terrain != terrain::TerrainKey::default() {
                    let Some(group) = self.terrain.group(&batch.material.terrain) else {
                        continue;
                    };
                    pass.set_pipeline(&self.terrain.pipeline);
                    pass.set_bind_group(1, group, &[]);
                } else {
                    let pipeline = if batch.transparent {
                        &self.transparent_pipeline
                    } else {
                        &self.opaque_pipeline
                    };
                    let material = self
                        .materials
                        .get(&batch.material)
                        .and_then(|cached| cached.gpu.as_ref())
                        .unwrap_or(&self.plain_material);
                    pass.set_pipeline(pipeline);
                    pass.set_bind_group(1, &material.bind_group, &[]);
                }
                pass.set_vertex_buffer(0, mesh.vertices.slice(..));
                pass.set_index_buffer(mesh.indices.slice(..), mesh.index_format);
                count(mesh.index_count, &batch.instances);
                pass.draw_indexed(0..mesh.index_count, 0, batch.instances);
            }
            // Last, over everything, but hidden behind solid items.
            self.sprites.draw(&mut pass, &sprite_draws);
        }

        self.post.draw(
            &mut encoder,
            &self.post_targets,
            view,
            post.enabled && post.bloom_enabled && post.bloom_intensity > 0.0,
        );

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

        if !ui_vertices.is_empty() || !text_buffers.is_empty() {
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

        if timed && let Some(timer) = &self.gpu_timer {
            timer.end(&mut encoder);
        }
        self.queue.submit([encoder.finish()]);
        if timed && let Some(timer) = &mut self.gpu_timer {
            timer.submitted();
        }
        self.draws = draws;
        self.forget_undrawn();
        Ok(())
    }

    /// How many shadow maps are alive: 0 or 1, unless one leaked.
    #[cfg(test)]
    pub(super) fn shadow_maps_alive(&self) -> usize {
        Arc::strong_count(&self.shadow_maps_alive) - 1
    }

    pub(super) fn stats(&self) -> RenderStats {
        RenderStats {
            draw_calls: self.draws.0,
            triangles: self.draws.1,
            gpu_time: self.gpu_timer.as_ref().and_then(gpu_timer::GpuTimer::last),
            gpu_timing: self.gpu_timer.is_some(),
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

    /// Uploads the callers' meshes and textures in `list` that aren't on
    /// the GPU yet, and notes that the rest were drawn this frame.
    pub(super) fn upload_new(&mut self, list: &DrawList) {
        let frame = self.frame;
        for item in &list.items {
            if let Shape::Mesh(source) = &item.shape {
                match self.custom_meshes.entry(source.key()) {
                    Entry::Occupied(mut cached) => {
                        let cached = cached.get_mut();
                        cached.last_drawn = frame;
                        // A posed (animated) mesh has new corners: rewrite them.
                        if let Some(gpu) = &mut cached.gpu
                            && gpu.version != source.version()
                        {
                            update_custom_mesh(&self.device, &self.queue, gpu, source.as_ref());
                        }
                    }
                    Entry::Vacant(slot) => {
                        slot.insert(Cached {
                            gpu: upload_custom_mesh(&self.device, source.as_ref()),
                            last_drawn: frame,
                        });
                    }
                }
            }
            if let Some(surface) = &item.material.terrain {
                for weights in &surface.weights {
                    self.note_texture(weights.as_ref(), frame);
                }
                let weights = surface.weights.each_ref().map(|source| {
                    self.textures
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
            let sources = material_textures(item);
            for source in sources.iter().flatten() {
                self.note_texture(source.as_ref(), frame);
            }
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
        for item in &list.ui {
            if let UiDrawItem::Image(image) = item
                && let Some(source) = &image.texture
            {
                self.note_texture(source.as_ref(), frame);
            }
        }
        for sprite in &list.sprites {
            if let Some(source) = &sprite.texture {
                self.note_texture(source.as_ref(), frame);
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
                .textures
                .get(&key)
                .and_then(|cached| cached.gpu.as_ref())
                .is_none()
            {
                continue;
            }
            if !self.ui_image_groups.contains_key(&key) {
                let group = {
                    let gpu = self
                        .textures
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

    /// Uploads `source` the first time, and marks it drawn this frame.
    pub(super) fn note_texture(&mut self, source: &dyn TextureSource, frame: u64) {
        match self.textures.entry(source.key()) {
            Entry::Occupied(mut cached) => cached.get_mut().last_drawn = frame,
            Entry::Vacant(slot) => {
                slot.insert(Cached {
                    gpu: upload_texture(&self.device, &self.queue, source),
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
        self.ui_image_groups
            .retain(|key, _| self.textures.contains_key(key));
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
