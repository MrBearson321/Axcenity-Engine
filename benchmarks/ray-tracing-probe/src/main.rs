//! Measures what hardware ray queries cost through wgpu on this machine:
//! bottom-level (BLAS) builds, top-level (TLAS) rebuilds as instances move,
//! and one ray per pixel for the effects `docs/research/hardware-ray-tracing.md`
//! weighs (sun shadows, ambient occlusion, reflections, diffuse GI).
//!
//! Run from this folder: `cargo run --release` (add `-- --help` for options).
//! `WGPU_BACKEND=dx12` or `vulkan` picks the API, as in the engine.

use std::time::Instant;

use wgpu::util::DeviceExt;

struct Options {
    width: u32,
    height: u32,
    cubes: u32,
    spheres: u32,
    frames: u32,
}

impl Options {
    fn parse() -> Self {
        let mut options = Options {
            width: 2560,
            height: 1440,
            cubes: 10_000,
            spheres: 200,
            frames: 30,
        };
        let args: Vec<String> = std::env::args().skip(1).collect();
        let mut i = 0;
        while i < args.len() {
            let value = || -> u32 {
                args.get(i + 1)
                    .and_then(|v| v.parse().ok())
                    .unwrap_or_else(|| panic!("{} needs a number", args[i]))
            };
            match args[i].as_str() {
                "--width" => options.width = value(),
                "--height" => options.height = value(),
                "--cubes" => options.cubes = value(),
                "--spheres" => options.spheres = value(),
                "--frames" => options.frames = value(),
                _ => {
                    println!(
                        "ray-tracing-probe [--width 2560] [--height 1440] [--cubes 10000] \
                         [--spheres 200] [--frames 30]"
                    );
                    std::process::exit(0);
                }
            }
            i += 2;
        }
        options
    }
}

/// Vertices (xyz) and triangle indices of one mesh.
struct Mesh {
    vertices: Vec<[f32; 3]>,
    indices: Vec<u32>,
}

fn ground() -> Mesh {
    Mesh {
        vertices: vec![
            [-0.5, 0.0, -0.5],
            [0.5, 0.0, -0.5],
            [0.5, 0.0, 0.5],
            [-0.5, 0.0, 0.5],
        ],
        indices: vec![0, 2, 1, 0, 3, 2],
    }
}

fn cube() -> Mesh {
    let mut vertices = Vec::new();
    for i in 0..8u32 {
        vertices.push([
            (i & 1) as f32 - 0.5,
            ((i >> 1) & 1) as f32 - 0.5,
            ((i >> 2) & 1) as f32 - 0.5,
        ]);
    }
    let faces = [
        [0, 2, 3, 1],
        [4, 5, 7, 6],
        [0, 1, 5, 4],
        [2, 6, 7, 3],
        [0, 4, 6, 2],
        [1, 3, 7, 5],
    ];
    let mut indices = Vec::new();
    for [a, b, c, d] in faces {
        indices.extend_from_slice(&[a, b, c, a, c, d]);
    }
    Mesh { vertices, indices }
}

/// A unit-radius UV sphere with about `triangles` triangles.
fn sphere(triangles: u32) -> Mesh {
    let n = ((triangles as f32 / 2.0).sqrt().round() as u32).max(3);
    let mut vertices = Vec::new();
    for stack in 0..=n {
        let theta = std::f32::consts::PI * stack as f32 / n as f32;
        for slice in 0..=n {
            let phi = std::f32::consts::TAU * slice as f32 / n as f32;
            vertices.push([theta.sin() * phi.cos(), theta.cos(), theta.sin() * phi.sin()]);
        }
    }
    let mut indices = Vec::new();
    for stack in 0..n {
        for slice in 0..n {
            let a = stack * (n + 1) + slice;
            let b = a + n + 1;
            indices.extend_from_slice(&[a, b, a + 1, a + 1, b, b + 1]);
        }
    }
    Mesh { vertices, indices }
}

/// A row-major 3x4 transform: rotation about Y, scale, then translation.
fn transform(position: [f32; 3], scale: [f32; 3], yaw: f32) -> [f32; 12] {
    let (s, c) = yaw.sin_cos();
    [
        c * scale[0],
        0.0,
        s * scale[2],
        position[0],
        0.0,
        scale[1],
        0.0,
        position[1],
        -s * scale[0],
        0.0,
        c * scale[2],
        position[2],
    ]
}

/// A cheap deterministic random number in 0..1.
fn hash(i: u32, salt: u32) -> f32 {
    let mut x = i.wrapping_mul(0x9E37_79B9) ^ salt.wrapping_mul(0x85EB_CA6B);
    x ^= x >> 16;
    x = x.wrapping_mul(0x7FEB_352D);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846C_A68B);
    x ^= x >> 16;
    x as f32 / u32::MAX as f32
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Params {
    cam_pos: [f32; 4],
    cam_forward: [f32; 4],
    cam_right: [f32; 4],
    cam_up: [f32; 4],
    sun: [f32; 4],
    size: [u32; 4],
}

fn normalize(v: [f32; 3]) -> [f32; 3] {
    let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    [v[0] / l, v[1] / l, v[2] / l]
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn median(mut values: Vec<f64>) -> f64 {
    if values.is_empty() {
        return f64::NAN;
    }
    values.sort_by(|a, b| a.partial_cmp(b).unwrap());
    values[values.len() / 2]
}

/// GPU timestamps, read back after each submit.
struct Timer {
    set: wgpu::QuerySet,
    resolve: wgpu::Buffer,
    readback: wgpu::Buffer,
    period_ns: f64,
}

const TIMER_SLOTS: u32 = 32;

impl Timer {
    fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        let size = TIMER_SLOTS as u64 * 8;
        Timer {
            set: device.create_query_set(&wgpu::QuerySetDescriptor {
                label: Some("timestamps"),
                ty: wgpu::QueryType::Timestamp,
                count: TIMER_SLOTS,
            }),
            resolve: device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("timestamps resolve"),
                size,
                usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
                mapped_at_creation: false,
            }),
            readback: device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("timestamps readback"),
                size,
                usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
            period_ns: queue.get_timestamp_period() as f64,
        }
    }

    fn resolve(&self, encoder: &mut wgpu::CommandEncoder, used: u32) {
        encoder.resolve_query_set(&self.set, 0..used, &self.resolve, 0);
        encoder.copy_buffer_to_buffer(&self.resolve, 0, &self.readback, 0, used as u64 * 8);
    }

    /// Milliseconds between each pair of slots (0-1, 2-3, ...).
    fn read(&self, device: &wgpu::Device, pairs: u32) -> Vec<f64> {
        let slice = self.readback.slice(0..pairs as u64 * 16);
        slice.map_async(wgpu::MapMode::Read, |r| r.unwrap());
        device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
        let data = slice.get_mapped_range().unwrap();
        let ticks: &[u64] = bytemuck::cast_slice(&data);
        let out = (0..pairs as usize)
            .map(|i| ticks[2 * i + 1].wrapping_sub(ticks[2 * i]) as f64 * self.period_ns / 1e6)
            .collect();
        drop(data);
        self.readback.unmap();
        out
    }
}

fn wait(device: &wgpu::Device) {
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
}

fn main() {
    let options = Options::parse();
    let instance = wgpu::Instance::new(
        wgpu::InstanceDescriptor::new_without_display_handle_from_env(),
    );
    let adapters = pollster::block_on(instance.enumerate_adapters(wgpu::Backends::all()));
    let adapter = adapters
        .into_iter()
        .filter(|a| a.features().contains(wgpu::Features::EXPERIMENTAL_RAY_QUERY))
        .max_by_key(|a| {
            let info = a.get_info();
            let kind = match info.device_type {
                wgpu::DeviceType::DiscreteGpu => 3,
                wgpu::DeviceType::IntegratedGpu => 2,
                wgpu::DeviceType::Cpu => 0,
                _ => 1,
            };
            let api = match info.backend {
                wgpu::Backend::Dx12 if cfg!(windows) => 3,
                wgpu::Backend::Vulkan | wgpu::Backend::Metal => 2,
                _ => 0,
            };
            (kind, api)
        })
        .unwrap_or_else(|| {
            eprintln!("No adapter here offers wgpu's EXPERIMENTAL_RAY_QUERY.");
            std::process::exit(1);
        });
    let info = adapter.get_info();
    let wanted = wgpu::Features::EXPERIMENTAL_RAY_QUERY
        | (adapter.features()
            & (wgpu::Features::TIMESTAMP_QUERY | wgpu::Features::TIMESTAMP_QUERY_INSIDE_ENCODERS));
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("probe"),
        required_features: wanted,
        required_limits: adapter.limits(),
        // SAFETY: ray queries are an experimental wgpu feature; this probe
        // only measures them and accepts that they may misbehave.
        experimental_features: unsafe { wgpu::ExperimentalFeatures::enabled() },
        memory_hints: wgpu::MemoryHints::Performance,
        trace: wgpu::Trace::Off,
    }))
    .expect("device with ray queries");
    let gpu_timing = device.features().contains(wgpu::Features::TIMESTAMP_QUERY);
    let inside = device
        .features()
        .contains(wgpu::Features::TIMESTAMP_QUERY_INSIDE_ENCODERS);
    println!("# Ray tracing probe");
    println!(
        "adapter: {} ({:?}, {:?}), driver {} {}",
        info.name, info.backend, info.device_type, info.driver, info.driver_info
    );
    println!(
        "timestamps: passes {gpu_timing}, builds {inside} (CPU wall time otherwise)",
    );
    let limits = device.limits();
    println!(
        "limits: max_tlas_instance_count {}, max_blas_primitive_count {}",
        limits.max_tlas_instance_count, limits.max_blas_primitive_count
    );
    let timer = gpu_timing.then(|| Timer::new(&device, &queue));

    // Meshes: 0 ground, 1 cube, 2 sphere (in the scene), 3 big sphere
    // (only built, for BLAS timings).
    let meshes = [ground(), cube(), sphere(100_000), sphere(1_000_000)];
    let mut vertices: Vec<[f32; 3]> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();
    let mut ranges = Vec::new();
    for mesh in &meshes {
        ranges.push([indices.len() as u32, vertices.len() as u32, 0, 0]);
        vertices.extend_from_slice(&mesh.vertices);
        indices.extend_from_slice(&mesh.indices);
    }
    let input = wgpu::BufferUsages::BLAS_INPUT | wgpu::BufferUsages::STORAGE;
    let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("vertices"),
        contents: bytemuck::cast_slice(&vertices),
        usage: input,
    });
    let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("indices"),
        contents: bytemuck::cast_slice(&indices),
        usage: input,
    });
    let mesh_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("meshes"),
        contents: bytemuck::cast_slice(&ranges),
        usage: wgpu::BufferUsages::STORAGE,
    });
    let sizes: Vec<wgpu::BlasTriangleGeometrySizeDescriptor> = meshes
        .iter()
        .map(|m| wgpu::BlasTriangleGeometrySizeDescriptor {
            vertex_format: wgpu::VertexFormat::Float32x3,
            vertex_count: m.vertices.len() as u32,
            index_format: Some(wgpu::IndexFormat::Uint32),
            index_count: Some(m.indices.len() as u32),
            flags: wgpu::AccelerationStructureGeometryFlags::OPAQUE,
        })
        .collect();
    let make_blas = |mesh: usize, flags: wgpu::AccelerationStructureFlags| {
        device.create_blas(
            &wgpu::CreateBlasDescriptor {
                label: None,
                flags,
                update_mode: wgpu::AccelerationStructureUpdateMode::Build,
            },
            wgpu::BlasGeometrySizeDescriptors::Triangles {
                descriptors: vec![sizes[mesh].clone()],
            },
        )
    };

    // --- BLAS builds -----------------------------------------------------
    println!();
    println!("## BLAS builds (median of 5)");
    println!("| mesh | triangles | flags | GPU ms | CPU wall ms |");
    println!("|---|---|---|---|---|");
    let fast_trace = wgpu::AccelerationStructureFlags::PREFER_FAST_TRACE;
    let fast_build = wgpu::AccelerationStructureFlags::PREFER_FAST_BUILD;
    for (mesh, name) in [(1, "cube"), (2, "sphere 100k"), (3, "sphere 1M")] {
        for (flags, flag_name) in [(fast_trace, "fast trace"), (fast_build, "fast build")] {
            let mut gpu = Vec::new();
            let mut wall = Vec::new();
            for _ in 0..5 {
                let blas = make_blas(mesh, flags);
                let start = Instant::now();
                let mut encoder = device.create_command_encoder(&Default::default());
                if let (Some(t), true) = (&timer, inside) {
                    encoder.write_timestamp(&t.set, 0);
                }
                encoder.build_acceleration_structures(
                    std::iter::once(&wgpu::BlasBuildEntry {
                        blas: &blas,
                        geometry: wgpu::BlasGeometries::TriangleGeometries(vec![
                            wgpu::BlasTriangleGeometry {
                                size: &sizes[mesh],
                                vertex_buffer: &vertex_buffer,
                                first_vertex: ranges[mesh][1],
                                vertex_stride: 12,
                                index_buffer: Some(&index_buffer),
                                first_index: Some(ranges[mesh][0]),
                                transform_buffer: None,
                                transform_buffer_offset: None,
                            },
                        ]),
                    }),
                    std::iter::empty(),
                );
                if let (Some(t), true) = (&timer, inside) {
                    encoder.write_timestamp(&t.set, 1);
                    t.resolve(&mut encoder, 2);
                }
                queue.submit(Some(encoder.finish()));
                wait(&device);
                wall.push(start.elapsed().as_secs_f64() * 1e3);
                if let (Some(t), true) = (&timer, inside) {
                    gpu.push(t.read(&device, 1)[0]);
                }
            }
            println!(
                "| {name} | {} | {flag_name} | {:.3} | {:.3} |",
                meshes[mesh].indices.len() / 3,
                median(gpu),
                median(wall)
            );
        }
    }

    // The scene's BLASes, built once (fast trace, as static meshes would be).
    let scene_blas: Vec<wgpu::Blas> = (0..3).map(|m| make_blas(m, fast_trace)).collect();
    {
        let entries: Vec<wgpu::BlasBuildEntry> = (0..3)
            .map(|m| wgpu::BlasBuildEntry {
                blas: &scene_blas[m],
                geometry: wgpu::BlasGeometries::TriangleGeometries(vec![
                    wgpu::BlasTriangleGeometry {
                        size: &sizes[m],
                        vertex_buffer: &vertex_buffer,
                        first_vertex: ranges[m][1],
                        vertex_stride: 12,
                        index_buffer: Some(&index_buffer),
                        first_index: Some(ranges[m][0]),
                        transform_buffer: None,
                        transform_buffer_offset: None,
                    },
                ]),
            })
            .collect();
        let mut encoder = device.create_command_encoder(&Default::default());
        encoder.build_acceleration_structures(entries.iter(), std::iter::empty());
        queue.submit(Some(encoder.finish()));
        wait(&device);
    }

    // Instances: the ground, `cubes` cubes, `spheres` spheres, over a
    // 400 x 400 stud area.
    let place = |tlas: &mut wgpu::Tlas, frame: u32, count: u32| {
        tlas[0] = Some(wgpu::TlasInstance::new(
            &scene_blas[0],
            transform([0.0, 0.0, 0.0], [1000.0, 1.0, 1000.0], 0.0),
            0,
            0xff,
        ));
        let drift = frame as f32 * 0.01;
        for i in 1..count {
            // The last `spheres` instances are spheres.
            let sphere = i >= count - options.spheres.min(count - 1);
            let x = (hash(i, 1) - 0.5) * 400.0;
            let z = (hash(i, 2) - 0.5) * 400.0;
            let (mesh, scale, y) = if sphere {
                let r = 2.0 + hash(i, 3) * 4.0;
                (2, [r, r, r], r)
            } else {
                let s = [1.0 + hash(i, 3) * 6.0, 1.0 + hash(i, 4) * 10.0, 1.0 + hash(i, 5) * 6.0];
                (1, s, s[1] * 0.5)
            };
            tlas[i as usize] = Some(wgpu::TlasInstance::new(
                &scene_blas[mesh],
                transform([x, y, z], scale, hash(i, 6) * 6.28 + drift),
                mesh as u32,
                0xff,
            ));
        }
    };

    // --- TLAS rebuilds ---------------------------------------------------
    println!();
    println!("## TLAS rebuild with every instance moved (median of 10)");
    println!(
        "| instances | CPU placing (probe's own loop) ms | CPU wgpu encode + submit ms | GPU build ms | CPU wall ms |"
    );
    println!("|---|---|---|---|---|");
    for count in [1_000u32, 10_000, 100_000] {
        if count > limits.max_tlas_instance_count {
            println!("| {count} | over the device limit | | | |");
            continue;
        }
        let mut tlas = device.create_tlas(&wgpu::CreateTlasDescriptor {
            label: None,
            max_instances: count,
            flags: fast_build,
            update_mode: wgpu::AccelerationStructureUpdateMode::Build,
        });
        let (mut placing, mut cpu, mut gpu, mut wall) =
            (Vec::new(), Vec::new(), Vec::new(), Vec::new());
        for frame in 0..10 {
            let start = Instant::now();
            place(&mut tlas, frame, count);
            let placed = Instant::now();
            placing.push((placed - start).as_secs_f64() * 1e3);
            let mut encoder = device.create_command_encoder(&Default::default());
            if let (Some(t), true) = (&timer, inside) {
                encoder.write_timestamp(&t.set, 0);
            }
            encoder.build_acceleration_structures(std::iter::empty(), std::iter::once(&tlas));
            if let (Some(t), true) = (&timer, inside) {
                encoder.write_timestamp(&t.set, 1);
                t.resolve(&mut encoder, 2);
            }
            queue.submit(Some(encoder.finish()));
            cpu.push(placed.elapsed().as_secs_f64() * 1e3);
            wait(&device);
            wall.push(start.elapsed().as_secs_f64() * 1e3);
            if let (Some(t), true) = (&timer, inside) {
                gpu.push(t.read(&device, 1)[0]);
            }
        }
        println!(
            "| {count} | {:.3} | {:.3} | {:.3} | {:.3} |",
            median(placing),
            median(cpu),
            median(gpu),
            median(wall)
        );
    }

    // --- Rays ------------------------------------------------------------
    let count = 1 + options.cubes + options.spheres;
    let mut tlas = device.create_tlas(&wgpu::CreateTlasDescriptor {
        label: Some("scene"),
        max_instances: count,
        flags: fast_trace,
        update_mode: wgpu::AccelerationStructureUpdateMode::Build,
    });
    place(&mut tlas, 0, count);
    let pixels = options.width as u64 * options.height as u64;
    let storage = |label: &str, size: u64| {
        device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(label),
            size,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        })
    };
    let hits = storage("hits", pixels * 16);
    let normals = storage("normals", pixels * 16);
    let results = storage("results", pixels * 4);
    let forward = normalize([0.0, -0.18, 1.0]);
    let right = normalize(cross([0.0, 1.0, 0.0], forward));
    let up = cross(forward, right);
    let sun = normalize([0.4, 0.8, 0.3]);
    let params = |frame: u32| Params {
        cam_pos: [0.0, 30.0, -230.0, 0.0],
        cam_forward: [forward[0], forward[1], forward[2], 0.0],
        cam_right: [right[0], right[1], right[2], 0.0],
        cam_up: [up[0], up[1], up[2], 0.0],
        sun: [sun[0], sun[1], sun[2], 1.5f32.to_radians().tan()],
        size: [options.width, options.height, frame, 0],
    };
    let params_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("params"),
        contents: bytemuck::bytes_of(&params(0)),
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
    });
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("probe.wgsl"),
        source: wgpu::ShaderSource::Wgsl(include_str!("probe.wgsl").into()),
    });
    let compute = wgpu::ShaderStages::COMPUTE;
    let buffer_entry = |binding: u32, read_only: bool| wgpu::BindGroupLayoutEntry {
        binding,
        visibility: compute,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Storage { read_only },
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    };
    let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("probe"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: compute,
                ty: wgpu::BindingType::AccelerationStructure {
                    vertex_return: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: compute,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            buffer_entry(2, false),
            buffer_entry(3, false),
            buffer_entry(4, false),
            buffer_entry(5, true),
            buffer_entry(6, true),
            buffer_entry(7, true),
        ],
    });
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("probe"),
        bind_group_layouts: &[Some(&layout)],
        immediate_size: 0,
    });
    // (entry point, rays per pixel as a fraction of the full resolution)
    let passes: [(&str, f64, bool); 6] = [
        ("primary", 1.0, false),
        ("shadow_hard", 1.0, false),
        ("shadow_soft", 1.0, false),
        ("ambient_occlusion", 1.0, false),
        ("reflection", 0.25, true),
        ("diffuse_gi", 0.25, true),
    ];
    let pipelines: Vec<wgpu::ComputePipeline> = passes
        .iter()
        .map(|(name, _, _)| {
            device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some(name),
                layout: Some(&pipeline_layout),
                module: &shader,
                entry_point: Some(name),
                compilation_options: Default::default(),
                cache: None,
            })
        })
        .collect();

    let mut encoder = device.create_command_encoder(&Default::default());
    encoder.build_acceleration_structures(std::iter::empty(), std::iter::once(&tlas));
    queue.submit(Some(encoder.finish()));
    wait(&device);

    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("probe"),
        layout: &layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: tlas.as_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: params_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: hits.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: normals.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 4,
                resource: results.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 5,
                resource: vertex_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 6,
                resource: index_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 7,
                resource: mesh_buffer.as_entire_binding(),
            },
        ],
    });

    let mut times: Vec<Vec<f64>> = vec![Vec::new(); passes.len()];
    let mut walls = Vec::new();
    for frame in 0..options.frames {
        queue.write_buffer(&params_buffer, 0, bytemuck::bytes_of(&params(frame)));
        let start = Instant::now();
        let mut encoder = device.create_command_encoder(&Default::default());
        for (p, (_, _, half)) in passes.iter().enumerate() {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some(passes[p].0),
                timestamp_writes: timer.as_ref().map(|t| wgpu::ComputePassTimestampWrites {
                    query_set: &t.set,
                    beginning_of_pass_write_index: Some(2 * p as u32),
                    end_of_pass_write_index: Some(2 * p as u32 + 1),
                }),
            });
            pass.set_pipeline(&pipelines[p]);
            pass.set_bind_group(0, &bind_group, &[]);
            let (w, h) = if *half {
                (options.width.div_ceil(2), options.height.div_ceil(2))
            } else {
                (options.width, options.height)
            };
            pass.dispatch_workgroups(w.div_ceil(8), h.div_ceil(8), 1);
        }
        if let Some(t) = &timer {
            t.resolve(&mut encoder, 2 * passes.len() as u32);
        }
        queue.submit(Some(encoder.finish()));
        wait(&device);
        walls.push(start.elapsed().as_secs_f64() * 1e3);
        if let Some(t) = &timer {
            for (p, ms) in t.read(&device, passes.len() as u32).into_iter().enumerate() {
                times[p].push(ms);
            }
        }
    }

    // A sanity check: what share of the screen the primary rays hit, and
    // how much of that the sun reaches (neither should be 0% or 100%).
    let (hit_share, lit_share) = {
        let readback = |buffer: &wgpu::Buffer, size: u64| -> Vec<u8> {
            let staging = device.create_buffer(&wgpu::BufferDescriptor {
                label: None,
                size,
                usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let mut encoder = device.create_command_encoder(&Default::default());
            encoder.copy_buffer_to_buffer(buffer, 0, &staging, 0, size);
            queue.submit(Some(encoder.finish()));
            let slice = staging.slice(..);
            slice.map_async(wgpu::MapMode::Read, |r| r.unwrap());
            wait(&device);
            let data = slice.get_mapped_range().unwrap().to_vec();
            data
        };
        // Run primary and hard shadows once more so `results` holds shadows.
        let mut encoder = device.create_command_encoder(&Default::default());
        for p in 0..2 {
            let mut pass = encoder.begin_compute_pass(&Default::default());
            pass.set_pipeline(&pipelines[p]);
            pass.set_bind_group(0, &bind_group, &[]);
            pass.dispatch_workgroups(options.width.div_ceil(8), options.height.div_ceil(8), 1);
        }
        queue.submit(Some(encoder.finish()));
        wait(&device);
        let hit_data = readback(&hits, pixels * 16);
        let hit_floats: &[[f32; 4]] = bytemuck::cast_slice(&hit_data);
        let result_data = readback(&results, pixels * 4);
        let lit: &[u32] = bytemuck::cast_slice(&result_data);
        let mut hit_count = 0u64;
        let mut lit_count = 0u64;
        for (h, l) in hit_floats.iter().zip(lit) {
            if h[3] != 0.0 {
                hit_count += 1;
                lit_count += (*l == 1) as u64;
            }
        }
        (
            hit_count as f64 / pixels as f64,
            lit_count as f64 / hit_count.max(1) as f64,
        )
    };

    println!();
    println!(
        "## Rays: {}x{}, {} instances ({} cubes, {} spheres of {} triangles), median of {} frames",
        options.width,
        options.height,
        count,
        options.cubes,
        options.spheres,
        meshes[2].indices.len() / 3,
        options.frames
    );
    println!(
        "sanity: primary rays hit {:.1}% of pixels; {:.1}% of those see the sun",
        hit_share * 100.0,
        lit_share * 100.0
    );
    println!("| pass | rays | GPU ms | Mrays/s |");
    println!("|---|---|---|---|");
    for (p, (name, share, _)) in passes.iter().enumerate() {
        let rays = pixels as f64 * share * if *name == "diffuse_gi" { 2.0 } else { 1.0 };
        let ms = median(times[p].clone());
        println!(
            "| {name} | {:.2} M | {:.3} | {:.0} |",
            rays / 1e6,
            ms,
            rays / 1e6 / (ms / 1e3)
        );
    }
    println!(
        "CPU wall per frame (all passes, submit to idle): {:.3} ms",
        median(walls)
    );
}
