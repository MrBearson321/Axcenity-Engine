use super::*;

fn flat(height: f32) -> (TerrainData, Placement) {
    // 2 studs between samples, 100 studs of height, centered on the origin.
    let data = TerrainData::flat(65, 65, height).unwrap();
    let placement = Placement {
        position: Vec3::ZERO,
        size: Vec3::new(128.0, 100.0, 128.0),
    };
    (data, placement)
}

fn height_at(data: &TerrainData, placement: &Placement, x: f32, z: f32) -> f32 {
    placement.ground_y(data, x, z).unwrap()
}

#[test]
fn falloff_is_full_inside_and_fades_to_nothing_at_the_rim() {
    let brush = Brush {
        radius: 10.0,
        falloff: 0.5,
        ..Brush::default()
    };
    assert_eq!(brush.weight(Vec2::ZERO), 1.0);
    assert_eq!(brush.weight(Vec2::new(5.0, 0.0)), 1.0);
    assert_eq!(brush.weight(Vec2::new(7.5, 0.0)), 0.5);
    assert_eq!(brush.weight(Vec2::new(10.0, 0.0)), 0.0);
    assert!(brush.weight(Vec2::new(9.0, 0.0)) < brush.weight(Vec2::new(8.0, 0.0)));
    // A square brush reaches its corners.
    let square = Brush {
        shape: Shape::Square,
        falloff: 0.0,
        ..brush
    };
    assert_eq!(square.weight(Vec2::new(9.0, 9.0)), 1.0);
    let hard = Brush {
        falloff: 0.0,
        ..Brush::default()
    };
    assert_eq!(hard.weight(Vec2::new(11.9, 0.0)), 1.0);
    for curve in Curve::ALL {
        assert_eq!(curve.shape(0.0), 0.0);
        assert_eq!(curve.shape(1.0), 1.0);
    }
}

#[test]
fn raising_lifts_the_middle_most_and_leaves_the_rest_alone() {
    let (mut data, placement) = flat(0.2);
    let brush = Brush::default();
    let rect = apply(&mut data, &placement, &brush, &Operation::Raise, Vec3::ZERO);
    assert!(!rect.is_empty());
    let middle = height_at(&data, &placement, 0.0, 0.0);
    let edge = height_at(&data, &placement, 10.0, 0.0);
    assert!(
        middle > 20.0 && edge > 20.0 && edge < middle,
        "{middle} {edge}"
    );
    assert_eq!(height_at(&data, &placement, 40.0, 40.0), 20.0);
    // Only samples within the brush's reach changed.
    assert!(rect.width() <= 15 && rect.depth() <= 15, "{rect:?}");

    apply(&mut data, &placement, &brush, &Operation::Lower, Vec3::ZERO);
    assert!((height_at(&data, &placement, 0.0, 0.0) - 20.0).abs() < 1e-4);
}

#[test]
fn height_limits_stop_raising() {
    let (mut data, placement) = flat(0.2);
    let brush = Brush {
        strength: 1.0,
        height_limits: Some((0.0, 21.0)),
        ..Brush::default()
    };
    for _ in 0..20 {
        apply(&mut data, &placement, &brush, &Operation::Raise, Vec3::ZERO);
    }
    assert!((height_at(&data, &placement, 0.0, 0.0) - 21.0).abs() < 1e-3);
}

#[test]
fn smoothing_removes_a_spike_but_keeps_a_broad_hill() {
    let (mut data, placement) = flat(0.0);
    // A broad hill, and one spike on it.
    let all = data.bounds();
    let heights = data.heights_mut_in(all);
    for z in 0..65 {
        for x in 0..65 {
            let d = ((x as f32 - 32.0).powi(2) + (z as f32 - 32.0).powi(2)).sqrt();
            heights[z * 65 + x] = (0.3 - d * 0.01).max(0.0);
        }
    }
    heights[32 * 65 + 34] += 0.2;
    let hill_before = data.height(28, 32);
    let brush = Brush {
        radius: 16.0,
        strength: 1.0,
        ..Brush::default()
    };
    for _ in 0..4 {
        apply(
            &mut data,
            &placement,
            &brush,
            &Operation::Smooth,
            Vec3::ZERO,
        );
    }
    let spike = data.height(34, 32) - (data.height(33, 32) + data.height(35, 32)) / 2.0;
    // It started 0.2 above its neighbours.
    assert!(spike < 0.03, "the spike is mostly gone: {spike}");
    assert!(
        (data.height(28, 32) - hill_before).abs() < 0.01,
        "the hill keeps its height"
    );
}

#[test]
fn flatten_pulls_toward_the_target_and_can_go_one_way_only() {
    let (mut data, placement) = flat(0.5);
    let brush = Brush {
        strength: 1.0,
        falloff: 0.0,
        ..Brush::default()
    };
    let down = Operation::Flatten {
        target: 30.0,
        mode: FlattenMode::Both,
    };
    for _ in 0..12 {
        apply(&mut data, &placement, &brush, &down, Vec3::ZERO);
    }
    assert!((height_at(&data, &placement, 0.0, 0.0) - 30.0).abs() < 0.05);
    let raise_only = Operation::Flatten {
        target: 10.0,
        mode: FlattenMode::RaiseOnly,
    };
    apply(&mut data, &placement, &brush, &raise_only, Vec3::ZERO);
    assert!((height_at(&data, &placement, 0.0, 0.0) - 30.0).abs() < 0.05);
}

#[test]
fn stamps_add_their_shape_and_symmetry_mirrors_it() {
    let (mut data, placement) = flat(0.2);
    let brush = Brush {
        radius: 20.0,
        strength: 0.5,
        symmetry: Symmetry::X,
        ..Brush::default()
    };
    let stamp = Operation::Stamp(Stamp::default());
    apply(
        &mut data,
        &placement,
        &brush,
        &stamp,
        Vec3::new(-30.0, 20.0, 0.0),
    );
    let left = height_at(&data, &placement, -30.0, 0.0);
    let right = height_at(&data, &placement, 30.0, 0.0);
    assert!(left > 40.0, "{left}");
    assert!((left - right).abs() < 1e-3);
    let crater = Operation::Stamp(Stamp {
        shape: StampShape::Crater,
        ..Stamp::default()
    });
    let brush = Brush {
        symmetry: Symmetry::None,
        ..brush
    };
    apply(
        &mut data,
        &placement,
        &brush,
        &crater,
        Vec3::new(0.0, 20.0, 30.0),
    );
    assert!(height_at(&data, &placement, 0.0, 30.0) < 20.0);
}

#[test]
fn paint_blends_toward_a_layer_keeps_locked_ones_and_erases_back_to_auto() {
    let unlocked = [false; MAX_TERRAIN_LAYERS];
    let paint = |layer| Operation::Paint {
        layer,
        mode: PaintMode::Blend,
        locked: unlocked,
    };
    let mut sample = [0; MAX_TERRAIN_LAYERS];
    sample = paint_sample(sample, &paint(2), 0.5);
    assert_eq!(sample[2], 128);
    sample = paint_sample(sample, &paint(2), 1.0);
    assert_eq!(sample[2], 255);
    sample = paint_sample(sample, &paint(1), 0.5);
    assert_eq!((sample[1], sample[2]), (128, 127));

    let mut locked = unlocked;
    locked[1] = true;
    let over = Operation::Paint {
        layer: 3,
        mode: PaintMode::Replace,
        locked,
    };
    let painted = paint_sample(sample, &over, 0.2);
    assert_eq!((painted[1], painted[2], painted[3]), (128, 0, 127));
    let erased = paint_sample(painted, &Operation::Erase { locked: unlocked }, 1.0);
    assert_eq!(erased, [0; MAX_TERRAIN_LAYERS]);
    for sample in [sample, painted, erased] {
        assert!(sample.iter().map(|&w| u32::from(w)).sum::<u32>() <= 255);
    }
}

#[test]
fn stamps_are_spaced_evenly_however_fast_the_mouse_moves() {
    let mut stroke = Stroke::new();
    assert_eq!(stroke.advance(Vec3::ZERO, 2.0, 0.0, None), vec![Vec3::ZERO]);
    // One big jump of 10 studs: stamps at 2, 4, 6, 8 and 10, no gaps.
    let stamps = stroke.advance(Vec3::new(10.0, 0.0, 0.0), 2.0, 0.016, None);
    let xs: Vec<f32> = stamps.iter().map(|p| p.x.round()).collect();
    assert_eq!(xs, vec![2.0, 4.0, 6.0, 8.0, 10.0]);
    // Small moves add up.
    assert!(
        stroke
            .advance(Vec3::new(11.0, 0.0, 0.0), 2.0, 0.016, None)
            .is_empty()
    );
    let next = stroke.advance(Vec3::new(12.5, 0.0, 0.0), 2.0, 0.016, None);
    assert_eq!(next.len(), 1);
    assert!((next[0].x - 12.0).abs() < 1e-5);
    // Held still, it repeats.
    assert!(
        stroke
            .advance(Vec3::new(12.5, 0.0, 0.0), 2.0, 0.03, Some(0.05))
            .is_empty()
    );
    assert_eq!(
        stroke
            .advance(Vec3::new(12.5, 0.0, 0.0), 2.0, 0.03, Some(0.05))
            .len(),
        1
    );
}

#[test]
fn a_recorded_stroke_undoes_and_redoes_and_keeps_only_the_chunks_it_changed() {
    let (mut data, placement) = flat(0.2);
    let original = data.heights().to_vec();
    let brush = Brush::default();
    let mut recorder = EditRecorder::new();
    let mut stroke = Stroke::new();
    for x in [-20.0, -10.0, 0.0] {
        for point in stroke.advance(Vec3::new(x, 20.0, 0.0), brush.spacing_studs(), 0.0, None) {
            let rect = footprint(&data, &placement, Vec2::new(point.x, point.z), brush.radius);
            recorder.before_change(&data, rect);
            apply(&mut data, &placement, &brush, &Operation::Raise, point);
        }
    }
    let edited = data.heights().to_vec();
    let edit = recorder.finish(&data).unwrap();
    // 65 samples is 2 × 2 chunks; the stroke stays in the middle row.
    assert!(edit.before.len() <= 4);
    edit.undo(&mut data);
    assert_eq!(data.heights(), original.as_slice());
    edit.redo(&mut data);
    assert_eq!(data.heights(), edited.as_slice());

    // A stroke that changes nothing leaves nothing to undo.
    let mut recorder = EditRecorder::new();
    recorder.before_change(&data, SampleRect::new(0, 0, 3, 3));
    assert!(recorder.finish(&data).is_none());
}

#[test]
fn foliage_scatters_at_density_and_undo_removes_the_stroke() {
    let (mut data, placement) = flat(0.2);
    let brush = Brush {
        radius: 16.0,
        strength: 1.0,
        falloff: 0.0,
        ..Brush::default()
    };
    let plant = Operation::Foliage {
        source: FoliageSource::Plant(axcenity_core::FoliagePlant::Grass),
        density: 0.2,
        erase: false,
        collide: true,
    };
    let mut recorder = EditRecorder::new();
    recorder.before_change(
        &data,
        footprint(&data, &placement, Vec2::ZERO, brush.radius),
    );
    apply(&mut data, &placement, &brush, &plant, Vec3::ZERO);
    let count = data.foliage().len();
    assert!(count > 5, "scattered {count} plants");
    assert!(
        data.foliage()
            .iter()
            .all(|i| i.source == FoliageSource::Plant(axcenity_core::FoliagePlant::Grass))
    );
    let heights = data.heights().to_vec();
    let edit = recorder.finish(&data).unwrap();
    edit.undo(&mut data);
    assert!(data.foliage().is_empty());
    assert_eq!(data.heights(), heights.as_slice());
    edit.redo(&mut data);
    assert_eq!(data.foliage().len(), count);
    assert!(data.foliage().iter().all(|i| i.collide));

    let crate_op = Operation::Foliage {
        source: FoliageSource::Prop(axcenity_core::FoliageProp::Crate),
        density: 0.05,
        erase: false,
        collide: false,
    };
    apply(
        &mut data,
        &placement,
        &brush,
        &crate_op,
        Vec3::new(20.0, 0.0, 0.0),
    );
    assert!(
        data.foliage()
            .iter()
            .any(|i| i.source == FoliageSource::Prop(axcenity_core::FoliageProp::Crate))
    );
    let erase = Operation::Foliage {
        source: FoliageSource::Plant(axcenity_core::FoliagePlant::Grass),
        density: 0.2,
        erase: true,
        collide: false,
    };
    apply(&mut data, &placement, &brush, &erase, Vec3::ZERO);
    assert!(
        data.foliage()
            .iter()
            .all(|i| i.source == FoliageSource::Prop(axcenity_core::FoliageProp::Crate))
    );
}
