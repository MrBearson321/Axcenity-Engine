//! Phase 60: sculpting. A face sculpted on a voxel-remeshed sphere with X
//! symmetry, masks holding vertices still under every brush, each brush's
//! way, the hierarchy's picking, chunks, and the 500k-triangle benchmark.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use glam::{IVec3, Vec3};

use crate::half_edge::{EditMesh, Polygon};
use crate::paint::BrushFalloff;
use crate::sculpt::{Sculpt, SculptDab, SculptTool, subdivide, voxel_remesh};
use crate::{PrimitiveSettings, PrimitiveShape, check_manifold};

/// A UV sphere 2 studs wide, remeshed into even quads.
fn remeshed_sphere() -> EditMesh {
    let sphere = PrimitiveSettings::of(PrimitiveShape::UvSphere).build();
    voxel_remesh(&sphere, 0.06).expect("a sphere remeshes")
}

/// A sphere of radius 1 made from a cube with `n` × `n` quads a side.
fn cube_sphere(n: i32) -> EditMesh {
    let mut ids: HashMap<IVec3, u32> = HashMap::new();
    let mut positions = Vec::new();
    let mut vertex = |p: IVec3| {
        *ids.entry(p).or_insert_with(|| {
            let cube = p.as_vec3() * (2.0 / n as f32) - Vec3::ONE;
            positions.push(cube.normalize());
            positions.len() as u32 - 1
        })
    };
    let mut polygons = Vec::new();
    for axis in 0..3 {
        for outward in [false, true] {
            let (mut u, mut v) = ((axis + 1) % 3, (axis + 2) % 3);
            if !outward {
                std::mem::swap(&mut u, &mut v);
            }
            for i in 0..n {
                for j in 0..n {
                    let corner = |di: i32, dj: i32| {
                        let mut p = IVec3::ZERO;
                        p[axis] = if outward { n } else { 0 };
                        p[u] = i + di;
                        p[v] = j + dj;
                        p
                    };
                    let corners = [corner(0, 0), corner(1, 0), corner(1, 1), corner(0, 1)]
                        .map(&mut vertex)
                        .to_vec();
                    polygons.push(Polygon {
                        corners,
                        uvs: Vec::new(),
                        uvs2: Vec::new(),
                        normals: Vec::new(),
                        material: 0,
                        smooth: true,
                    });
                }
            }
        }
    }
    EditMesh::new(positions, &polygons)
}

/// For each vertex, the one at its mirror image across X = 0.
fn mirror_pairs(positions: &[Vec3]) -> Vec<u32> {
    positions
        .iter()
        .map(|p| {
            let mirrored = Vec3::new(-p.x, p.y, p.z);
            let (best, distance) = positions
                .iter()
                .enumerate()
                .map(|(k, q)| (k as u32, q.distance(mirrored)))
                .min_by(|a, b| a.1.total_cmp(&b.1))
                .unwrap();
            assert!(distance < 1e-4, "{p} has no mirror image ({distance})");
            best
        })
        .collect()
}

/// The furthest any vertex is from its mirror image's mirror image.
fn asymmetry(positions: &[Vec3], pairs: &[u32]) -> f32 {
    positions
        .iter()
        .zip(pairs)
        .map(|(p, &other)| {
            let q = positions[other as usize];
            p.distance(Vec3::new(-q.x, q.y, q.z))
        })
        .fold(0.0, f32::max)
}

/// The settings of a stroke in these tests.
#[derive(Clone, Copy)]
struct Stroke {
    tool: SculptTool,
    radius: f32,
    strength: f32,
    invert: bool,
    symmetry: [bool; 3],
}

impl Stroke {
    fn of(tool: SculptTool) -> Stroke {
        Stroke {
            tool,
            radius: 0.2,
            strength: 0.6,
            invert: false,
            symmetry: [false; 3],
        }
    }
}

/// Where a ray from far out toward the middle meets the surface, and the
/// normal there toward the outside.
fn on_surface(sculpt: &Sculpt, toward: Vec3) -> Option<(Vec3, Vec3)> {
    let origin = toward.normalize() * 4.0;
    let direction = -toward.normalize();
    let (triangle, t) = sculpt.ray(origin, direction)?;
    let mut normal = sculpt.triangle_normal(triangle);
    if normal.dot(direction) > 0.0 {
        normal = -normal;
    }
    Some((origin + direction * t, normal))
}

/// A stroke from `from` to `to` (directions from the middle), a dab every
/// 0.02 studs. Grab and Snake Hook drag along the line from where they
/// first touch; the others follow the surface. True if a dab changed it.
fn stroke(sculpt: &mut Sculpt, settings: Stroke, from: Vec3, to: Vec3) -> bool {
    sculpt.begin_stroke();
    let steps = ((to - from).length() / 0.02).ceil().max(1.0) as usize;
    let mut changed = false;
    let mut last: Option<Vec3> = None;
    let start = on_surface(sculpt, from).expect("the stroke starts on the sphere");
    for k in 0..=steps {
        let along = from.lerp(to, k as f32 / steps as f32);
        let (center, normal) = if settings.tool.drags() {
            (start.0 + (along - from), start.1)
        } else {
            match on_surface(sculpt, along) {
                Some(hit) => hit,
                None => continue,
            }
        };
        let dab = SculptDab {
            center,
            normal,
            radius: settings.radius,
            strength: settings.strength,
            motion: last.map_or(Vec3::ZERO, |last| center - last),
            invert: settings.invert,
        };
        changed |= sculpt.dab(settings.tool, BrushFalloff::Smooth, settings.symmetry, &dab);
        last = Some(center);
    }
    changed
}

/// The strokes that sculpt a face looking along +Z on a sphere of radius
/// 1, with X symmetry.
fn face_strokes() -> [(Stroke, Vec3, Vec3); 5] {
    let with = |tool| Stroke {
        symmetry: [true, false, false],
        ..Stroke::of(tool)
    };
    // The face looks along +Z. A brow and a nose (Draw: the nose runs down
    // the middle, where both sides' dabs overlap), cheeks (Clay Strips),
    // the brow softened (Smooth), and the mouth (Crease).
    [
        (
            with(SculptTool::Draw),
            Vec3::new(0.05, 0.35, 0.94),
            Vec3::new(0.5, 0.35, 0.8),
        ),
        (
            with(SculptTool::Draw),
            Vec3::new(0.0, 0.2, 1.0),
            Vec3::new(0.0, -0.15, 1.0),
        ),
        (
            with(SculptTool::ClayStrips),
            Vec3::new(0.3, -0.05, 0.95),
            Vec3::new(0.55, -0.2, 0.8),
        ),
        (
            with(SculptTool::Smooth),
            Vec3::new(0.0, 0.35, 0.94),
            Vec3::new(0.5, 0.35, 0.8),
        ),
        (
            with(SculptTool::Crease),
            Vec3::new(0.0, -0.45, 0.89),
            Vec3::new(0.35, -0.42, 0.83),
        ),
    ]
}

/// The face of `a_face_sculpted_on_a_remeshed_sphere_with_x_symmetry_…`
/// as a mesh (Phase 61 retopologises and bakes it).
pub(crate) fn sculpted_face() -> EditMesh {
    let mut mesh = remeshed_sphere();
    let mut sculpt = Sculpt::new(&mesh);
    for (settings, from, to) in face_strokes() {
        stroke(&mut sculpt, settings, from, to);
    }
    sculpt.write_back(&mut mesh);
    mesh
}

#[test]
fn a_voxel_remeshed_sphere_is_closed_outward_even_and_mirrored() {
    let mesh = remeshed_sphere();
    check_manifold(&mesh, true).expect("closed and manifold");
    let sculpt = Sculpt::new(&mesh);
    let volume: f32 = sculpt
        .triangles()
        .iter()
        .map(|t| {
            let [a, b, c] = t.map(|v| sculpt.positions()[v as usize]);
            a.dot(b.cross(c)) / 6.0
        })
        .sum();
    // The UV sphere's 32 × 16 faces hold a little less than a ball.
    let ball = 4.0 / 3.0 * std::f32::consts::PI;
    assert!(volume > 0.95 * ball && volume < ball, "outward: {volume}");
    // Even: every edge about a voxel long.
    for t in sculpt.triangles() {
        for (a, b) in [(t[0], t[1]), (t[1], t[2])] {
            let length = sculpt.positions()[a as usize].distance(sculpt.positions()[b as usize]);
            assert!(length > 0.005 && length < 0.12, "edge {length}");
        }
    }
    mirror_pairs(sculpt.positions());
    assert!(mesh.face_count() > 2000, "{} faces", mesh.face_count());
}

#[test]
fn a_voxel_remesh_too_fine_is_refused() {
    let sphere = PrimitiveSettings::of(PrimitiveShape::UvSphere).build();
    let error = voxel_remesh(&sphere, 0.001).unwrap_err();
    assert!(error.contains("voxels"), "{error}");
    assert!(voxel_remesh(&sphere, 0.0).is_err());
}

#[test]
fn subdivide_cuts_every_quad_into_four() {
    let mesh = remeshed_sphere();
    let finer = subdivide(&mesh).unwrap();
    assert_eq!(finer.face_count(), mesh.face_count() * 4);
    check_manifold(&finer, true).unwrap();
}

#[test]
fn a_face_sculpted_on_a_remeshed_sphere_with_x_symmetry_matches_on_both_sides() {
    let mut mesh = remeshed_sphere();
    let mut sculpt = Sculpt::new(&mesh);
    let pairs = mirror_pairs(sculpt.positions());
    let start = sculpt.positions().to_vec();
    let strokes = face_strokes();
    for (settings, from, to) in strokes {
        assert!(
            stroke(&mut sculpt, settings, from, to),
            "{} sculpted",
            settings.tool.label()
        );
        let off = asymmetry(sculpt.positions(), &pairs);
        assert!(
            off < 1e-4,
            "{} left the sides {off} apart",
            settings.tool.label()
        );
    }
    // It's a face now, not a sphere: the brow stands out and the mouth is in.
    let moved = sculpt
        .positions()
        .iter()
        .zip(&start)
        .map(|(p, q)| p.distance(*q))
        .fold(0.0, f32::max);
    assert!(moved > 0.03, "moved {moved}");
    let height = |toward: Vec3| on_surface(&sculpt, toward).unwrap().0.length();
    assert!(height(Vec3::new(0.25, 0.35, 0.9)) > 1.01, "a brow");
    assert!(height(Vec3::new(-0.25, 0.35, 0.9)) > 1.01, "both brows");
    assert!(height(Vec3::new(0.15, -0.44, 0.88)) < 0.995, "a mouth");
    assert!(height(Vec3::new(-0.15, -0.44, 0.88)) < 0.995, "all of it");

    // Back in the mesh, still closed and still mirrored.
    assert!(sculpt.write_back(&mut mesh));
    check_manifold(&mesh, true).unwrap();
    assert!(asymmetry(mesh.positions(), &pairs) < 1e-4);
    assert!(!sculpt.write_back(&mut mesh), "nothing moved since");
}

#[test]
fn masked_vertices_dont_move_under_any_brush() {
    let mesh = remeshed_sphere();
    let mut sculpt = Sculpt::new(&mesh);
    // Mask the front with the Mask brush.
    let mask = Stroke {
        radius: 0.45,
        strength: 1.0,
        ..Stroke::of(SculptTool::Mask)
    };
    assert!(stroke(
        &mut sculpt,
        mask,
        Vec3::new(-0.1, 0.0, 1.0),
        Vec3::new(0.1, 0.0, 1.0)
    ));
    let masked: Vec<usize> = (0..sculpt.vertex_count())
        .filter(|&v| sculpt.mask()[v] == 1.0)
        .collect();
    assert!(masked.len() > 50, "{} masked", masked.len());
    let before = sculpt.positions().to_vec();
    for tool in SculptTool::ALL {
        if tool == SculptTool::Mask {
            continue;
        }
        // Across the masked front and out past it, both ways.
        let at = sculpt.positions().to_vec();
        for invert in [false, true] {
            let settings = Stroke {
                radius: 0.3,
                strength: 1.0,
                invert,
                symmetry: [true, false, false],
                ..Stroke::of(tool)
            };
            // Grab and Snake Hook take hold where the stroke starts: at
            // the mask's edge, so they hold some of each.
            stroke(
                &mut sculpt,
                settings,
                Vec3::new(0.3, 0.05, 1.0),
                Vec3::new(0.7, 0.05, 0.7),
            );
        }
        for &v in &masked {
            assert_eq!(
                sculpt.positions()[v],
                before[v],
                "{} moved a masked vertex",
                tool.label()
            );
        }
        let free_moved = (0..sculpt.vertex_count())
            .any(|v| sculpt.mask()[v] < 1.0 && sculpt.positions()[v] != at[v]);
        assert!(free_moved, "{} moved what isn't masked", tool.label());
    }

    // Invert frees the front and holds the rest; clear frees everything.
    sculpt.invert_mask();
    assert!(masked.iter().all(|&v| sculpt.mask()[v] == 0.0));
    assert!(sculpt.mask().iter().filter(|&&m| m == 1.0).count() > masked.len());
    sculpt.clear_mask();
    assert!(sculpt.mask().iter().all(|&m| m == 0.0));

    // Hide masked: brushes and rays pass over the hidden front.
    stroke(
        &mut sculpt,
        mask,
        Vec3::new(-0.1, 0.0, 1.0),
        Vec3::new(0.1, 0.0, 1.0),
    );
    assert!(sculpt.hide_masked());
    assert!(sculpt.hidden_count() > 0);
    let (hit, _) = on_surface(&sculpt, Vec3::Z).unwrap();
    assert!(hit.z < 0.0, "the ray goes through to the back: {hit}");
    let drawn: usize = sculpt
        .chunks()
        .iter()
        .map(|c| c.clay.len() + c.masked.len())
        .sum();
    assert_eq!(drawn / 3, sculpt.triangle_count() - sculpt.hidden_count());
    sculpt.reveal();
    assert_eq!(sculpt.hidden_count(), 0);
    assert!(on_surface(&sculpt, Vec3::Z).unwrap().0.z > 0.9);
}

#[test]
fn each_brush_moves_the_surface_its_way() {
    let mesh = remeshed_sphere();
    let front = |sculpt: &Sculpt| on_surface(sculpt, Vec3::Z).unwrap().0.z;
    let run = |settings: Stroke| {
        let mut sculpt = Sculpt::new(&mesh);
        assert!(stroke(
            &mut sculpt,
            settings,
            Vec3::new(-0.05, 0.0, 1.0),
            Vec3::new(0.05, 0.0, 1.0)
        ));
        sculpt
    };
    let of = Stroke::of;
    let untouched = front(&Sculpt::new(&mesh));
    for tool in [
        SculptTool::Draw,
        SculptTool::Inflate,
        SculptTool::ClayStrips,
    ] {
        assert!(
            front(&run(of(tool))) > untouched + 0.005,
            "{} builds",
            tool.label()
        );
        let inverted = Stroke {
            invert: true,
            ..of(tool)
        };
        assert!(
            front(&run(inverted)) < untouched - 0.005,
            "{} digs",
            tool.label()
        );
    }
    assert!(
        front(&run(of(SculptTool::Crease))) < untouched - 0.002,
        "crease"
    );
    assert!(
        front(&run(of(SculptTool::Flatten))) < untouched - 0.002,
        "flatten"
    );

    // Pinch pulls the sides of the stroke in toward it.
    let pinched = run(of(SculptTool::Pinch));
    let side = Sculpt::new(&mesh)
        .positions()
        .iter()
        .zip(pinched.positions())
        .filter(|(p, _)| p.z > 0.9 && p.y > 0.05 && p.y < 0.15)
        .map(|(p, q)| q.y - p.y)
        .sum::<f32>();
    assert!(side < 0.0, "pinched toward the stroke: {side}");

    // Grab and Snake Hook drag the front up, the way the mouse went.
    for tool in [SculptTool::Grab, SculptTool::SnakeHook] {
        let mut sculpt = Sculpt::new(&mesh);
        let settings = Stroke {
            radius: 0.3,
            ..of(tool)
        };
        assert!(stroke(
            &mut sculpt,
            settings,
            Vec3::new(0.0, 0.0, 1.0),
            Vec3::new(0.0, 0.3, 1.0)
        ));
        let lifted = Sculpt::new(&mesh)
            .positions()
            .iter()
            .zip(sculpt.positions())
            .map(|(p, q)| q.y - p.y)
            .fold(0.0, f32::max);
        assert!(lifted > 0.1, "{} lifted {lifted}", tool.label());
    }

    // Smooth takes a bump back down.
    let mut bumped = run(Stroke {
        strength: 1.0,
        radius: 0.1,
        ..of(SculptTool::Draw)
    });
    let peak = front(&bumped);
    stroke(
        &mut bumped,
        Stroke {
            strength: 1.0,
            ..of(SculptTool::Smooth)
        },
        Vec3::new(-0.05, 0.0, 1.0),
        Vec3::new(0.05, 0.0, 1.0),
    );
    assert!(front(&bumped) < peak - 0.002, "smoothed");
}

#[test]
fn the_hierarchy_picks_what_testing_every_triangle_picks() {
    let mesh = remeshed_sphere();
    let sculpt = Sculpt::new(&mesh);
    let mut seed = 7u32;
    let mut next = || {
        // A small linear congruential generator, for a test that's the
        // same every run.
        seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        (seed >> 8) as f32 / (1 << 24) as f32 * 2.0 - 1.0
    };
    for _ in 0..200 {
        let origin = Vec3::new(next(), next(), next()) * 3.0;
        let direction = (Vec3::new(next(), next(), next()) * 0.6 - origin).normalize();
        let brute = sculpt
            .triangles()
            .iter()
            .enumerate()
            .filter_map(|(t, corners)| {
                let corners = corners.map(|v| sculpt.positions()[v as usize]);
                Some((t as u32, crate::bvh::hit(origin, direction, corners)?))
            })
            .min_by(|a, b| a.1.total_cmp(&b.1));
        let fast = sculpt.ray(origin, direction);
        match (brute, fast) {
            (Some((_, a)), Some((_, b))) => assert!((a - b).abs() < 1e-5),
            (None, None) => {}
            other => panic!("{other:?}"),
        }
    }
}

#[test]
fn a_dab_marks_only_the_chunks_it_touched() {
    let mesh = cube_sphere(64);
    let mut sculpt = Sculpt::new(&mesh);
    let chunks = sculpt.chunks().len();
    assert!(chunks >= 12, "{chunks} chunks");
    assert!(
        sculpt
            .chunks()
            .iter()
            .all(|c| c.clay.len() / 3 <= crate::sculpt::CHUNK_TRIANGLES)
    );
    assert_eq!(sculpt.take_dirty_chunks().len(), chunks, "all drawn first");
    assert!(sculpt.take_dirty_chunks().is_empty());
    stroke(
        &mut sculpt,
        Stroke {
            radius: 0.1,
            ..Stroke::of(SculptTool::Draw)
        },
        Vec3::Z,
        Vec3::Z,
    );
    let dirty = sculpt.take_dirty_chunks();
    assert!(!dirty.is_empty() && dirty.len() <= chunks / 4, "{dirty:?}");
    let (positions, normals) = sculpt.chunk_corners(dirty[0]);
    assert_eq!(positions.len(), sculpt.chunks()[dirty[0]].vertices.len());
    assert_eq!(normals.len(), positions.len());
}

/// The phase's target: a 500k-triangle mesh stays above 30 frames a second
/// while sculpting. Each frame is what the Studio does on the CPU for one
/// frame of a stroke: pick the surface under the mouse, dab (with X
/// symmetry), and rebuild the corners of the chunks it touched to send to
/// the GPU (which writes them over the old ones in place).
#[test]
fn sculpting_a_500k_triangle_mesh_stays_above_30_frames_a_second() {
    let mesh = cube_sphere(205);
    let built = Instant::now();
    let mut sculpt = Sculpt::new(&mesh);
    let build_time = built.elapsed();
    assert!(
        sculpt.triangle_count() >= 500_000,
        "{}",
        sculpt.triangle_count()
    );
    let budget = Duration::from_secs_f64(1.0 / 30.0);
    for tool in [
        SculptTool::Draw,
        SculptTool::ClayStrips,
        SculptTool::Smooth,
        SculptTool::Crease,
        SculptTool::Grab,
        SculptTool::SnakeHook,
        SculptTool::Mask,
        // R15.1's.
        SculptTool::Layer,
        SculptTool::Blob,
        SculptTool::Scrape,
        SculptTool::Fill,
        SculptTool::DrawSharp,
        SculptTool::Nudge,
        SculptTool::ElasticGrab,
    ] {
        sculpt.begin_stroke();
        let mut frames = Vec::new();
        let mut last: Option<Vec3> = None;
        let mut uploaded = 0;
        for k in 0..60 {
            let toward = Vec3::new(-0.4 + k as f32 * 0.012, 0.2, 1.0);
            let frame = Instant::now();
            let (center, normal) = on_surface(&sculpt, toward).unwrap();
            let dab = SculptDab {
                center,
                normal,
                radius: 0.3,
                strength: 0.5,
                motion: last.map_or(Vec3::ZERO, |last| center - last),
                invert: false,
            };
            sculpt.dab(tool, BrushFalloff::Smooth, [true, false, false], &dab);
            for chunk in sculpt.take_dirty_chunks() {
                let corners = sculpt.chunk_corners(chunk);
                uploaded += corners.0.len();
                std::hint::black_box(corners);
            }
            frames.push(frame.elapsed());
            last = Some(center);
        }
        frames.sort();
        let mean = frames.iter().sum::<Duration>() / frames.len() as u32;
        let slow = frames[frames.len() * 95 / 100];
        println!(
            "{}: {mean:?} a frame on average, 95% within {slow:?}, {uploaded} corners sent",
            tool.label()
        );
        assert!(uploaded > 0, "{} touched chunks", tool.label());
        assert!(mean < budget, "{}: {mean:?} a frame", tool.label());
        assert!(slow < budget, "{}: 95% within {slow:?}", tool.label());
    }
    // R15.1: a Mesh Filter over all of it is one click, not a frame each,
    // but shouldn't stall the window either.
    for filter in crate::sculpt::MeshFilter::ALL {
        let started = Instant::now();
        assert!(sculpt.filter(filter, 0.2));
        let took = started.elapsed();
        println!("Mesh Filter {}: {took:?}", filter.label());
        assert!(
            took < Duration::from_secs(1),
            "{}: {took:?}",
            filter.label()
        );
    }
    let mut mesh = mesh;
    let written = Instant::now();
    sculpt.write_back(&mut mesh);
    println!(
        "built in {build_time:?}; the stroke's vertices back in the mesh in {:?}",
        written.elapsed()
    );
}

/// How far each vertex of `after` moved from `before`.
fn moves(before: &Sculpt, after: &Sculpt) -> Vec<Vec3> {
    before
        .positions()
        .iter()
        .zip(after.positions())
        .map(|(p, q)| *q - *p)
        .collect()
}

/// R15.1: the brushes added after Phase 60, each its own way.
#[test]
fn the_r15_brushes_move_the_surface_their_ways() {
    let mesh = remeshed_sphere();
    let untouched = Sculpt::new(&mesh);
    let run = |settings: Stroke, from: Vec3, to: Vec3| {
        let mut sculpt = Sculpt::new(&mesh);
        assert!(
            stroke(&mut sculpt, settings, from, to),
            "{} changed nothing",
            settings.tool.label()
        );
        sculpt
    };
    let across = (Vec3::new(-0.05, 0.0, 1.0), Vec3::new(0.05, 0.0, 1.0));
    let of = Stroke::of;

    // Layer rises to one height however many dabs go over a vertex.
    let layer = of(SculptTool::Layer);
    let goal = layer.radius * 0.1 * layer.strength;
    let layered = run(layer, across.0, across.1);
    let highest = moves(&untouched, &layered)
        .iter()
        .map(|m| m.length())
        .fold(0.0, f32::max);
    assert!(highest > goal * 0.5, "rose {highest} of {goal}");
    assert!(
        highest <= goal * 1.001,
        "rose past its height: {highest} > {goal}"
    );
    let lowered = run(
        Stroke {
            invert: true,
            ..layer
        },
        across.0,
        across.1,
    );
    assert!(
        moves(&untouched, &lowered)
            .iter()
            .any(|m| m.z < -goal * 0.5)
    );

    // Blob and Draw Sharp build, and dig with Ctrl; Draw Sharp's line is
    // narrower than Draw's.
    let front = |sculpt: &Sculpt| on_surface(sculpt, Vec3::Z).unwrap().0.z;
    for tool in [SculptTool::Blob, SculptTool::DrawSharp] {
        assert!(front(&run(of(tool), across.0, across.1)) > front(&untouched) + 0.002);
        let dug = run(
            Stroke {
                invert: true,
                ..of(tool)
            },
            across.0,
            across.1,
        );
        assert!(
            front(&dug) < front(&untouched) - 0.002,
            "{} digs",
            tool.label()
        );
    }
    let spread = |sculpt: &Sculpt| {
        moves(&untouched, sculpt)
            .iter()
            .filter(|m| m.length() > 1e-4)
            .count()
    };
    let total = |sculpt: &Sculpt| {
        moves(&untouched, sculpt)
            .iter()
            .map(|m| m.length())
            .sum::<f32>()
    };
    let draw = run(of(SculptTool::Draw), across.0, across.1);
    let sharp = run(of(SculptTool::DrawSharp), across.0, across.1);
    assert!(spread(&sharp) < spread(&draw), "sharp is narrower");
    assert!(total(&sharp) < total(&draw));

    // Scrape only takes down what stands above the plane under it (on a
    // sphere, the middle: in toward the centre); Fill only brings up
    // what's below (the rim: out). Ctrl swaps them.
    for (tool, invert, inward) in [
        (SculptTool::Scrape, false, true),
        (SculptTool::Fill, false, false),
        (SculptTool::Scrape, true, false),
        (SculptTool::Fill, true, true),
    ] {
        let sculpt = run(Stroke { invert, ..of(tool) }, across.0, across.1);
        let mut moved = 0;
        for (p, q) in untouched.positions().iter().zip(sculpt.positions()) {
            if p.distance(*q) < 1e-6 {
                continue;
            }
            moved += 1;
            let went_in = q.length() < p.length();
            assert_eq!(went_in, inward, "{} (Ctrl {invert}) at {p}", tool.label());
        }
        assert!(moved > 0, "{} moved nothing", tool.label());
    }

    // Nudge pushes the surface the way the stroke goes.
    let nudged = run(
        of(SculptTool::Nudge),
        Vec3::new(-0.15, 0.0, 1.0),
        Vec3::new(0.15, 0.0, 1.0),
    );
    let dx: f32 = moves(&untouched, &nudged).iter().map(|m| m.x).sum();
    assert!(dx > 0.0, "nudged along +X: {dx}");

    // Elastic Grab drags like Grab, and reaches past the radius.
    let settings = |tool| Stroke {
        radius: 0.2,
        ..of(tool)
    };
    let lift = (Vec3::new(0.0, 0.0, 1.0), Vec3::new(0.0, 0.3, 1.0));
    let grab = run(settings(SculptTool::Grab), lift.0, lift.1);
    let elastic = run(settings(SculptTool::ElasticGrab), lift.0, lift.1);
    let far_moved = |sculpt: &Sculpt| {
        untouched
            .positions()
            .iter()
            .zip(sculpt.positions())
            .any(|(p, q)| p.distance(Vec3::Z) > 0.21 && p.distance(*q) > 1e-3)
    };
    assert!(!far_moved(&grab), "Grab stays inside its radius");
    assert!(far_moved(&elastic), "Elastic Grab reaches past it");
    let lifted = moves(&untouched, &elastic)
        .iter()
        .map(|m| m.y)
        .fold(0.0, f32::max);
    assert!(lifted > 0.1, "lifted {lifted}");
}

/// R15.1: mesh filters change the whole sculpt, held back by the mask.
#[test]
fn mesh_filters_change_the_whole_sculpt_but_what_is_masked() {
    use crate::sculpt::MeshFilter;
    let mesh = remeshed_sphere();
    let untouched = Sculpt::new(&mesh);
    let mean_radius = |sculpt: &Sculpt| {
        sculpt.positions().iter().map(|p| p.length()).sum::<f32>() / sculpt.vertex_count() as f32
    };
    let filtered = |filter, amount| {
        let mut sculpt = Sculpt::new(&mesh);
        assert!(
            sculpt.filter(filter, amount),
            "{} moved nothing",
            filter.label()
        );
        sculpt
    };
    let r = mean_radius(&untouched);
    assert!(mean_radius(&filtered(MeshFilter::Inflate, 1.0)) > r + 0.01);
    assert!(mean_radius(&filtered(MeshFilter::Inflate, -1.0)) < r - 0.01);
    let grown = mean_radius(&filtered(MeshFilter::Scale, 1.0));
    assert!((grown / r - 1.1).abs() < 0.01, "scaled {}", grown / r);
    // Smooth shrinks a sphere a little (its points go toward chords) and
    // Sharpen does the opposite.
    assert!(mean_radius(&filtered(MeshFilter::Smooth, 0.5)) < r);
    assert!(mean_radius(&filtered(MeshFilter::Sharpen, 0.5)) > r);
    // Random is the same each time, in and out.
    let a = filtered(MeshFilter::Random, 1.0);
    let b = filtered(MeshFilter::Random, 1.0);
    assert_eq!(a.positions(), b.positions());
    let radial: Vec<f32> = moves(&untouched, &a)
        .iter()
        .zip(untouched.positions())
        .map(|(m, p)| m.dot(p.normalize()))
        .collect();
    assert!(radial.iter().any(|&d| d > 1e-4) && radial.iter().any(|&d| d < -1e-4));
    assert!(!Sculpt::new(&mesh).filter(MeshFilter::Smooth, 0.0));

    // The masked front and hidden parts don't move.
    let mut sculpt = Sculpt::new(&mesh);
    let mask = Stroke {
        radius: 0.45,
        strength: 1.0,
        ..Stroke::of(SculptTool::Mask)
    };
    stroke(
        &mut sculpt,
        mask,
        Vec3::new(-0.1, 0.0, 1.0),
        Vec3::new(0.1, 0.0, 1.0),
    );
    let held: Vec<usize> = (0..sculpt.vertex_count())
        .filter(|&v| sculpt.mask()[v] == 1.0)
        .collect();
    assert!(!held.is_empty());
    for filter in MeshFilter::ALL {
        let before = sculpt.positions().to_vec();
        assert!(sculpt.filter(filter, 0.5), "{}", filter.label());
        for &v in &held {
            assert_eq!(
                sculpt.positions()[v],
                before[v],
                "{} moved a masked vertex",
                filter.label()
            );
        }
    }
    let mut hidden = Sculpt::new(&mesh);
    stroke(
        &mut hidden,
        mask,
        Vec3::new(-0.1, 0.0, 1.0),
        Vec3::new(0.1, 0.0, 1.0),
    );
    hidden.hide_masked();
    hidden.clear_mask();
    let before = hidden.positions().to_vec();
    assert!(hidden.filter(MeshFilter::Inflate, 1.0));
    let front_still = (0..hidden.vertex_count())
        .filter(|&v| before[v].z > 0.99)
        .all(|v| hidden.positions()[v] == before[v]);
    assert!(front_still, "hidden parts stay");
}
