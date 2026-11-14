use super::*;
use axcenity_core::Vector3;

fn door_world() -> (World, InstanceId, InstanceId, InstanceId) {
    let mut world = World::new();
    let door = world.create(ClassName::Model).unwrap();
    world.set_name(door, "Door").unwrap();
    world.set_parent(door, Some(world.workspace())).unwrap();
    let hinge = world.create(ClassName::Part).unwrap();
    world.set_name(hinge, "Hinge").unwrap();
    world.set_parent(hinge, Some(door)).unwrap();
    let panel = world.create(ClassName::Part).unwrap();
    world.set_name(panel, "Panel").unwrap();
    world
        .set_property(
            panel,
            "Position",
            Value::Vector3(Vector3::new(2.0, 0.0, 0.0)),
        )
        .unwrap();
    world.set_parent(panel, Some(hinge)).unwrap();
    let other = world.create(ClassName::Part).unwrap();
    world.set_parent(other, Some(world.workspace())).unwrap();
    (world, door, hinge, panel)
}

#[test]
fn the_copy_has_only_the_root_and_keeps_its_ids() {
    let (world, door, hinge, panel) = door_world();
    let mut preview = ObjectPreview::default();
    preview.follow(&world, Some(door));
    assert_eq!(preview.root(), Some(door));
    let copy = preview.world();
    let shown: Vec<InstanceId> = copy
        .get(copy.workspace())
        .unwrap()
        .children()
        .iter()
        .copied()
        .filter(|&id| copy.get(id).unwrap().class() != ClassName::Part)
        .collect();
    assert_eq!(shown, [door]);
    assert!(copy.is_descendant_of(panel, hinge));

    // Picking the hinge in the Explorer selects it in the copy.
    preview.follow(&world, Some(hinge));
    assert_eq!(preview.root(), Some(door));
    assert_eq!(preview.selected, Some(hinge));
    let focus = preview.focus().unwrap();
    assert_eq!(focus.target, "Hinge");
    assert!(
        focus
            .values
            .iter()
            .any(|v| v.property == PropertyName::Orientation)
    );
}

#[test]
fn posing_never_touches_the_edited_world() {
    let (world, door, _hinge, panel) = door_world();
    let mut preview = ObjectPreview::default();
    preview.follow(&world, Some(door));
    preview.pose(&[EvaluatedProperty {
        target: "Hinge".into(),
        property: PropertyName::Orientation,
        value: vec![0.0, 90.0, 0.0],
    }]);
    let moved = preview
        .posed
        .get(panel)
        .unwrap()
        .property("Position")
        .cloned();
    assert_ne!(
        moved,
        world.get(panel).unwrap().property("Position").cloned()
    );
    // A Model's pivot keys as its Position and Orientation.
    preview.selected = Some(door);
    let focus = preview.focus().unwrap();
    assert_eq!(focus.target, ".");
    let keyed: Vec<PropertyName> = focus.values.iter().map(|v| v.property).collect();
    assert_eq!(keyed, [PropertyName::Position, PropertyName::Orientation]);
}

#[test]
fn a_ui_element_shows_its_screen_gui() {
    let mut world = World::new();
    let gui = world.create(ClassName::ScreenGui).unwrap();
    world.set_parent(gui, Some(world.workspace())).unwrap();
    let frame = world.create(ClassName::Frame).unwrap();
    world.set_parent(frame, Some(gui)).unwrap();
    let mut preview = ObjectPreview::default();
    preview.follow(&world, Some(frame));
    assert!(preview.flat);
    assert!(preview.world().contains(gui));
    let focus = preview.focus().unwrap();
    let position = focus
        .values
        .iter()
        .find(|v| v.property == PropertyName::Position)
        .unwrap();
    assert_eq!(position.value.len(), 4);
    assert_eq!(
        focus.transform,
        [PropertyName::Position, PropertyName::Size]
    );
}

/// The plan's door: turn the Hinge in the preview with Record on, and
/// the game puts the Panel where the preview showed it.
#[test]
fn a_hinge_turn_recorded_in_the_preview_plays_the_same_in_the_game() {
    use super::super::property_editor::PropertyDoc;
    let (mut world, door, hinge, panel) = door_world();
    let mut preview = ObjectPreview::default();
    preview.follow(&world, Some(door));
    preview.follow(&world, Some(hinge));
    let path = "assets/animations/Door.anim.json";
    let mut doc = PropertyDoc::open(path.into(), axcenity_anim::ClipFile::new(1.0));
    preview.pose(&doc.evaluated().unwrap());
    doc.key_focus(&preview.focus().unwrap());
    doc.playhead = 1.0;
    let turn = EvaluatedProperty {
        target: "Hinge".into(),
        property: PropertyName::Orientation,
        value: vec![0.0, 90.0, 0.0],
    };
    doc.edit(vec![turn], true);
    preview.pose(&doc.evaluated().unwrap());
    let shown = preview
        .posed
        .get(panel)
        .unwrap()
        .property("Position")
        .cloned();

    let folder = std::env::temp_dir().join(format!(
        "axcenity-door-preview-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    std::fs::create_dir_all(folder.join("assets/animations")).unwrap();
    std::fs::write(folder.join(path), doc.file.to_json()).unwrap();
    let assets = Assets::new(&folder);
    let animation = world.create(ClassName::Animation).unwrap();
    world.set_parent(animation, Some(door)).unwrap();
    world
        .set_property(animation, "Clip", Value::String(path.into()))
        .unwrap();
    world.property_animation_mut(animation).unwrap().play = true;
    let mut players = axcenity_anim::Animators::new();
    players.update(&mut world, &assets, 0.0);
    players.update(&mut world, &assets, 1.0);
    assert!(players.take_errors().is_empty());
    let played = world.get(panel).unwrap().property("Position").cloned();
    assert_eq!(played, shown);
    assert_eq!(
        shown.and_then(|v| v.as_vector3()).map(|v| v.z.round()),
        Some(-2.0)
    );
    std::fs::remove_dir_all(folder).unwrap();
}

#[test]
fn bounds_cover_rotated_parts() {
    let (world, door, ..) = door_world();
    let (min, max) = bounds(&world, door).unwrap();
    assert!(min.x < 0.0 && max.x > 2.0);
}

#[test]
fn a_face_controls_offers_its_controls_to_key() {
    let (mut world, door, hinge, _) = door_world();
    let face = world.create(ClassName::FaceControls).unwrap();
    world.set_parent(face, Some(hinge)).unwrap();
    world
        .set_property(face, "Jaw_Open", Value::Number(0.5))
        .unwrap();
    let mut preview = ObjectPreview::default();
    preview.follow(&world, Some(door));
    preview.follow(&world, Some(face));
    let focus = preview.focus().unwrap();
    assert_eq!(focus.target, "Hinge/FaceControls");
    assert_eq!(focus.values.len(), axcenity_core::face::FACE_CONTROLS.len());
    let jaw = focus
        .values
        .iter()
        .find(|v| v.property.as_str() == "Jaw_Open")
        .unwrap();
    assert_eq!(jaw.value, [0.5]);
}
