//! Bloom, tone mapping, color grading and FXAA (`post.wgsl`), and the
//! frame's targets.
//!
//! The scene is drawn into an HDR target. Bloom blurs the bright parts, then
//! the composite pass writes the window or the editor's viewport texture, or
//! with FXAA on an LDR target that FXAA then smooths into the output. While
//! the render scale is below 1, both stay at the scene's size and an
//! FSR 1-style upscale and sharpen (`upscale.wgsl`) take the picture to the
//! output's. With TAA on, TAA upscales instead (`temporal.rs`): bloom and the
//! composite pass read its output-sized picture, and only the sharpen runs.

use super::gpu::{create_depth_view, target_bytes};
use super::grading::GradingPass;
use super::lens::{DepthOfFieldPass, DepthOfFieldTarget};
use super::temporal::{SceneViews, TemporalPasses, TemporalTargets, TemporalUse};
use super::{
    BLOOM_LEVELS, DEPTH_FORMAT, FXAA_FORMAT, FrameTargets, FxaaTarget, HDR_FORMAT, MsaaTarget,
    OcclusionPasses, OcclusionTargets, PostPasses, PostUniform, TargetsAlive, UPSCALE_FORMAT,
    UpscaleTarget,
};

impl PostPasses {
    pub(super) fn new(device: &wgpu::Device, color_format: wgpu::TextureFormat) -> Self {
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
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("bloom"),
            entries: &entries,
        });
        entries.push(texture_entry(3));
        let composite_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("composite"),
            entries: &entries,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("post-processing"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let settings = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("post-processing settings"),
            size: size_of::<PostUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let upscale_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("upscale"),
            entries: &[texture_entry(0)],
        });

        let grading = GradingPass::new(device);
        let post_shader = device.create_shader_module(wgpu::include_wgsl!("../post.wgsl"));
        let upscale_shader = device.create_shader_module(wgpu::include_wgsl!("../upscale.wgsl"));
        let pipeline_of = |shader: &wgpu::ShaderModule,
                           label,
                           layouts: &[&wgpu::BindGroupLayout],
                           entry,
                           format,
                           blend| {
            let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some(label),
                bind_group_layouts: &super::gpu::some(layouts),
                immediate_size: 0,
            });
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: shader,
                    entry_point: Some("vs_main"),
                    compilation_options: Default::default(),
                    buffers: &[],
                },
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: None,
                multisample: Default::default(),
                fragment: Some(wgpu::FragmentState {
                    module: shader,
                    entry_point: Some(entry),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        let pipeline = |label, layouts: &[&wgpu::BindGroupLayout], entry, format, blend| {
            pipeline_of(&post_shader, label, layouts, entry, format, blend)
        };
        let upscale = |label, entry, format| {
            pipeline_of(
                &upscale_shader,
                label,
                &[&upscale_layout],
                entry,
                format,
                None,
            )
        };
        let add = wgpu::BlendComponent {
            src_factor: wgpu::BlendFactor::One,
            dst_factor: wgpu::BlendFactor::One,
            operation: wgpu::BlendOperation::Add,
        };
        Self {
            prefilter: pipeline(
                "bloom prefilter",
                &[&layout],
                "fs_prefilter",
                HDR_FORMAT,
                None,
            ),
            downsample: pipeline(
                "bloom downsample",
                &[&layout],
                "fs_downsample",
                HDR_FORMAT,
                None,
            ),
            upsample: pipeline(
                "bloom upsample",
                &[&layout],
                "fs_upsample",
                HDR_FORMAT,
                Some(wgpu::BlendState {
                    color: add,
                    alpha: add,
                }),
            ),
            composite: pipeline(
                "composite",
                &[&composite_layout, &grading.layout],
                "fs_composite",
                color_format,
                None,
            ),
            composite_for_fxaa: pipeline(
                "composite for FXAA",
                &[&composite_layout, &grading.layout],
                "fs_composite",
                FXAA_FORMAT,
                None,
            ),
            fxaa: pipeline("FXAA", &[&layout], "fs_fxaa", color_format, None),
            fxaa_for_upscale: pipeline(
                "FXAA for upscaling",
                &[&layout],
                "fs_fxaa",
                FXAA_FORMAT,
                None,
            ),
            easu: upscale("upscale", "fs_easu", UPSCALE_FORMAT),
            rcas: upscale("sharpen upscaled", "fs_rcas", color_format),
            upscale_layout,
            grading,
            depth_of_field: DepthOfFieldPass::new(device),
            sampler,
            layout,
            composite_layout,
            settings,
        }
    }

    /// Adds bloom (if `bloom`) to the scene and draws the result, tone
    /// mapped, into `target`: through FXAA if `targets` has its target.
    pub(super) fn draw(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        targets: &FrameTargets,
        target: &wgpu::TextureView,
        bloom: bool,
    ) {
        let levels = &targets.levels;
        if bloom {
            fullscreen_pass(
                encoder,
                "bloom prefilter",
                &levels[0],
                wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                &self.prefilter,
                &[&targets.prefilter_group],
            );
            for (level, target) in levels.iter().enumerate().skip(1) {
                fullscreen_pass(
                    encoder,
                    "bloom downsample",
                    target,
                    wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    &self.downsample,
                    &[&targets.downsample_groups[level]],
                );
            }
            for level in (1..levels.len()).rev() {
                fullscreen_pass(
                    encoder,
                    "bloom upsample",
                    &levels[level - 1],
                    wgpu::LoadOp::Load,
                    &self.upsample,
                    &[&targets.upsample_groups[level]],
                );
            }
        }
        let clear = wgpu::LoadOp::Clear(wgpu::Color::BLACK);
        let groups = [&targets.composite_group, self.grading.group()];
        let upscale = targets.upscale.as_ref();
        // The finished picture goes to the output, or while the scene is
        // smaller, to be upscaled into it.
        let (finished, composite, fxaa_pipeline) = match upscale {
            Some(upscale) => (
                &upscale.view,
                &self.composite_for_fxaa,
                &self.fxaa_for_upscale,
            ),
            None => (target, &self.composite, &self.fxaa),
        };
        match &targets.fxaa {
            None => fullscreen_pass(encoder, "composite", finished, clear, composite, &groups),
            Some(fxaa) => {
                fullscreen_pass(
                    encoder,
                    "composite",
                    &fxaa.view,
                    clear,
                    &self.composite_for_fxaa,
                    &groups,
                );
                fullscreen_pass(
                    encoder,
                    "FXAA",
                    finished,
                    clear,
                    fxaa_pipeline,
                    &[&fxaa.group],
                );
            }
        }
        if let Some(upscale) = upscale {
            // TAA upscaled already: only the sharpen is left.
            let sharpened = match &upscale.upscaled {
                Some((upscaled, group)) => {
                    fullscreen_pass(
                        encoder,
                        "upscale",
                        upscaled,
                        clear,
                        &self.easu,
                        &[&upscale.source_group],
                    );
                    group
                }
                None => &upscale.source_group,
            };
            fullscreen_pass(
                encoder,
                "sharpen upscaled",
                target,
                clear,
                &self.rcas,
                &[sharpened],
            );
        }
    }
}

/// Draws one full-screen triangle with `pipeline` into `target`, with
/// `bind_groups` from group 0 on.
pub(super) fn fullscreen_pass(
    encoder: &mut wgpu::CommandEncoder,
    label: &str,
    target: &wgpu::TextureView,
    load: wgpu::LoadOp<wgpu::Color>,
    pipeline: &wgpu::RenderPipeline,
    bind_groups: &[&wgpu::BindGroup],
) {
    let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some(label),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            depth_slice: None,
            view: target,
            resolve_target: None,
            ops: wgpu::Operations {
                load,
                store: wgpu::StoreOp::Store,
            },
        })],
        depth_stencil_attachment: None,
        ..Default::default()
    });
    pass.set_pipeline(pipeline);
    for (index, group) in bind_groups.iter().enumerate() {
        pass.set_bind_group(index as u32, *group, &[]);
    }
    pass.draw(0..3, 0..1);
}

impl FrameTargets {
    /// Makes the HDR and depth textures for a `width` × `height` scene with
    /// `samples` per pixel and the bloom levels below it, down to
    /// [`BLOOM_LEVELS`] halvings of the `output`'s size, with the bind
    /// groups that connect them, FXAA's target if `fxaa`, the prepass's and
    /// ambient occlusion's if `occlusion`, depth of field's if
    /// `depth_of_field` (which needs the prepass's), and the motion targets
    /// `temporal` uses (which need the prepass's too). Each target holds a
    /// clone of its counter in `alive`.
    pub(super) fn new(
        device: &wgpu::Device,
        (post, occlusion_passes, temporal_passes): (&PostPasses, &OcclusionPasses, &TemporalPasses),
        (width, height): (u32, u32),
        output: (u32, u32),
        (samples, fxaa, occlusion, depth_of_field, temporal): (u32, bool, bool, bool, TemporalUse),
        alive: &TargetsAlive,
    ) -> Self {
        // Bytes of every texture made, for `RenderStats::target_bytes`.
        let bytes = std::cell::Cell::new(0);
        let raw_texture = |label, (width, height): (u32, u32), format, samples| {
            let (width, height) = (width.max(1), height.max(1));
            bytes.set(bytes.get() + target_bytes((width, height, 1), format, samples));
            // A multisampled texture can't be sampled or copied, only
            // resolved. The others can be copied out (a reflection probe's
            // bake reads the scene's) and into (TAA and motion blur copy
            // their results over the scene's).
            let usage = if samples > 1 {
                wgpu::TextureUsages::RENDER_ATTACHMENT
            } else {
                wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING
                    | wgpu::TextureUsages::COPY_SRC
                    | wgpu::TextureUsages::COPY_DST
            };
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: samples,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage,
                view_formats: &[],
            })
        };
        let texture = |label, size, format, samples| {
            raw_texture(label, size, format, samples)
                .create_view(&wgpu::TextureViewDescriptor::default())
        };
        let make = |label, width, height| texture(label, (width, height), HDR_FORMAT, 1);
        let hdr = raw_texture("scene (HDR)", (width, height), HDR_FORMAT, 1);
        let hdr_view = hdr.create_view(&wgpu::TextureViewDescriptor::default());
        let msaa = (samples > 1).then(|| MsaaTarget {
            view: texture("scene (MSAA)", (width, height), HDR_FORMAT, samples),
            _alive: alive.msaa.clone(),
        });
        // Upscaled only when the scene is smaller than the output; a render
        // scale above 1 is scaled down by the composite pass's sampler. With
        // TAA, TAA upscales (G9 stage 2), and what comes after it (bloom, the
        // composite pass) works at the output's size.
        let upscaled = width < output.0 && height < output.1;
        let taa_upscales = upscaled && occlusion && temporal.taa;
        let source = if taa_upscales {
            output
        } else {
            (width, height)
        };
        let mut levels = Vec::new();
        let (mut level_width, mut level_height) = ((source.0 / 2).max(1), (source.1 / 2).max(1));
        loop {
            levels.push(make("bloom level", level_width, level_height));
            let last = (output.0 >> BLOOM_LEVELS, output.1 >> BLOOM_LEVELS);
            if level_width <= last.0
                || level_height <= last.1
                || level_width == 1
                || level_height == 1
            {
                break;
            }
            level_width = (level_width / 2).max(1);
            level_height = (level_height / 2).max(1);
        }

        let group = |layout: &wgpu::BindGroupLayout, source: &wgpu::TextureView, bloom| {
            let mut entries = vec![
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(source),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&post.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: post.settings.as_entire_binding(),
                },
            ];
            if let Some(bloom) = bloom {
                entries.push(wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(bloom),
                });
            }
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("post-processing"),
                layout,
                entries: &entries,
            })
        };
        let occlusion = occlusion.then(|| {
            OcclusionTargets::new(device, occlusion_passes, (width, height), &alive.occlusion)
        });
        let depth_of_field = occlusion
            .as_ref()
            .filter(|_| depth_of_field)
            .map(|prepass| {
                DepthOfFieldTarget::new(
                    device,
                    &post.depth_of_field,
                    (&hdr_view, &prepass.depth_view),
                    make("depth of field", width, height),
                    &alive.depth_of_field,
                )
            });
        // While TAA upscales, depth of field is drawn before it, on the
        // scene, and TAA reads its blurred copy.
        let taa_source = depth_of_field
            .as_ref()
            .filter(|_| taa_upscales)
            .map_or(&hdr_view, |target| &target.view);
        let temporal = occlusion
            .as_ref()
            .filter(|_| temporal.any())
            .map(|prepass| {
                TemporalTargets::new(
                    device,
                    temporal_passes,
                    ((width, height), taa_upscales.then_some(output)),
                    SceneViews {
                        hdr: &hdr_view,
                        depth: &prepass.depth_view,
                        taa_source,
                    },
                    temporal,
                    alive,
                )
            });
        // What bloom and the composite pass read: the scene, or with depth
        // of field its blurred copy, or TAA's upscaled picture.
        let scene = match temporal.as_ref().and_then(TemporalTargets::upscaled_view) {
            Some(upscaled) => upscaled,
            None => depth_of_field
                .as_ref()
                .map_or(&hdr_view, |target| &target.view),
        };
        let prefilter_group = group(&post.layout, scene, None);
        // Index 0 is never used, but keeps the lists indexed by level.
        let downsample_groups = (0..levels.len())
            .map(|level| group(&post.layout, &levels[level.saturating_sub(1)], None))
            .collect();
        let upsample_groups = (0..levels.len())
            .map(|level| group(&post.layout, &levels[level], None))
            .collect();
        let composite_group = group(&post.composite_layout, scene, Some(&levels[0]));
        let taa_upscales = temporal.as_ref().is_some_and(TemporalTargets::upscales);
        // FXAA smooths the picture at the size it's drawn at.
        let finished_size = if upscaled && !taa_upscales {
            (width, height)
        } else {
            output
        };
        let fxaa = fxaa.then(|| {
            let view = texture("tone mapped (FXAA)", finished_size, FXAA_FORMAT, 1);
            FxaaTarget {
                group: group(&post.layout, &view, None),
                view,
                _alive: alive.fxaa.clone(),
            }
        });
        let upscale = upscaled.then(|| {
            let one_texture = |view: &wgpu::TextureView| {
                device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("upscale"),
                    layout: &post.upscale_layout,
                    entries: &[wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(view),
                    }],
                })
            };
            let target =
                |label, (width, height): (u32, u32), view_formats: &[wgpu::TextureFormat]| {
                    bytes.set(bytes.get() + target_bytes((width, height, 1), UPSCALE_FORMAT, 1));
                    device.create_texture(&wgpu::TextureDescriptor {
                        label: Some(label),
                        size: wgpu::Extent3d {
                            width: width.max(1),
                            height: height.max(1),
                            depth_or_array_layers: 1,
                        },
                        mip_level_count: 1,
                        sample_count: 1,
                        dimension: wgpu::TextureDimension::D2,
                        format: UPSCALE_FORMAT,
                        usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                            | wgpu::TextureUsages::TEXTURE_BINDING,
                        view_formats,
                    })
                };
            let finished = target("finished (to upscale)", finished_size, &[FXAA_FORMAT]);
            let view = finished.create_view(&wgpu::TextureViewDescriptor {
                format: Some(FXAA_FORMAT),
                ..Default::default()
            });
            let source = finished.create_view(&wgpu::TextureViewDescriptor::default());
            let upscaled = (!taa_upscales).then(|| {
                let upscaled = target("upscaled", output, &[])
                    .create_view(&wgpu::TextureViewDescriptor::default());
                let group = one_texture(&upscaled);
                (upscaled, group)
            });
            UpscaleTarget {
                view,
                source_group: one_texture(&source),
                upscaled,
                _alive: alive.upscale.clone(),
            }
        });
        let depth_bytes = target_bytes((width, height, 1), DEPTH_FORMAT, samples);
        Self {
            size: (width, height),
            output,
            samples,
            bytes: bytes.get()
                + depth_bytes
                + occlusion.as_ref().map_or(0, |o| o.bytes)
                + temporal.as_ref().map_or(0, |t| t.bytes),
            _alive: alive.sets.clone(),
            hdr,
            hdr_view,
            msaa,
            depth_view: create_depth_view(device, width, height, samples),
            fxaa,
            upscale,
            occlusion,
            depth_of_field,
            temporal,
            levels,
            prefilter_group,
            downsample_groups,
            upsample_groups,
            composite_group,
        }
    }
}
