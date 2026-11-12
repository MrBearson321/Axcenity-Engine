use axcenity_core::ClassName;
use axcenity_ui::UiLayout;

use super::*;

/// The 800×600 viewport `frame` lays out into.
fn viewport() -> UiRect {
    UiRect {
        x: 0.0,
        y: 0.0,
        width: 800.0,
        height: 600.0,
    }
}

/// A ScreenGui holding a Frame at (100, 50) sized 200×80 by its
/// properties, laid out in an 800×600 viewport, with `css` applied.
fn frame(css: &str) -> (World, Vec<Stylesheet>, Handles, UiLayout) {
    let mut world = World::new();
    let gui = world.create(ClassName::ScreenGui).unwrap();
    world.set_parent(gui, Some(world.workspace())).unwrap();
    let frame = world.create(ClassName::Frame).unwrap();
    world.set_parent(frame, Some(gui)).unwrap();
    world
        .set_property(frame, "Class", Value::String("hud".into()))
        .unwrap();
    world
        .set_property(
            frame,
            "Position",
            Value::UDim2(UDim2::new(0.0, 100.0, 0.0, 50.0)),
        )
        .unwrap();
    world
        .set_property(
            frame,
            "Size",
            Value::UDim2(UDim2::new(0.0, 200.0, 0.0, 80.0)),
        )
        .unwrap();
    let (stylesheet, _) = axcenity_ui::parse_stylesheet("ui.css", css);
    let stylesheets = vec![stylesheet];
    let layout = axcenity_ui::layout_ui(&world, 800, 600, &stylesheets);
    let element = *layout.element(frame).unwrap();
    let handles = Handles::new(element, &world, &stylesheets, egui::Pos2::ZERO, 1.0);
    (world, stylesheets, handles, layout)
}

fn moved(
    drag: &UiDrag,
    pointer: egui::Pos2,
    pixels_per_point: f32,
    world: &World,
    layout: &UiLayout,
) -> UiDragEdit {
    drag.update(
        pointer,
        pixels_per_point,
        world,
        &layout.elements,
        viewport(),
    )
}

fn udim2(edit: &UiDragEdit, name: &str) -> Option<UDim2> {
    edit.values
        .iter()
        .find(|(property, _)| *property == name)
        .and_then(|(_, value)| value.as_udim2())
}

#[test]
fn with_an_aspect_ratio_every_handle_resizes_the_width_and_the_height_follows() {
    // 200 wide at 2:1 is 100 tall, whatever Size says.
    let (world, _, handles, layout) = frame(".hud { aspect-ratio: 2 }");
    assert_eq!(handles.rect.height(), 100.0);
    let size = |grip: Grip, from: egui::Pos2, by: egui::Vec2| {
        let drag = UiDrag::start(&handles, grip, from, &world).unwrap();
        let edit = moved(&drag, from + by, 1.0, &world, &layout);
        (udim2(&edit, "Size"), udim2(&edit, "Position"))
    };
    // The bottom handle asks for 110 tall: 220 wide.
    let (bottom, _) = size(
        Grip { x: 0, y: 1 },
        egui::pos2(200.0, 150.0),
        egui::vec2(0.0, 10.0),
    );
    assert_eq!(bottom.unwrap().x.offset, 220.0);
    let (right, _) = size(
        Grip { x: 1, y: 0 },
        egui::pos2(300.0, 100.0),
        egui::vec2(20.0, 0.0),
    );
    assert_eq!(right.unwrap().x.offset, 220.0);
    // The top left corner keeps the bottom right where it is.
    let (corner, position) = size(
        Grip { x: -1, y: -1 },
        egui::pos2(100.0, 50.0),
        egui::vec2(-20.0, -3.0),
    );
    assert_eq!(corner.unwrap().x.offset, 220.0);
    let position = position.unwrap();
    assert_eq!((position.x.offset, position.y.offset), (80.0, 40.0));
}

#[test]
fn a_turned_elements_handles_turn_with_it_and_drag_along_its_own_axes() {
    let mut world = World::new();
    let gui = world.create(ClassName::ScreenGui).unwrap();
    world.set_parent(gui, Some(world.workspace())).unwrap();
    let frame = world.create(ClassName::Frame).unwrap();
    world.set_parent(frame, Some(gui)).unwrap();
    world
        .set_property(
            frame,
            "Position",
            Value::UDim2(UDim2::new(0.0, 100.0, 0.0, 50.0)),
        )
        .unwrap();
    world
        .set_property(
            frame,
            "Size",
            Value::UDim2(UDim2::new(0.0, 200.0, 0.0, 100.0)),
        )
        .unwrap();
    world
        .set_property(frame, "Rotation", Value::Number(90.0))
        .unwrap();
    let layout = axcenity_ui::layout_ui(&world, 800, 600, &[]);
    let element = *layout.element(frame).unwrap();
    let handles = Handles::new(element, &world, &[], egui::Pos2::ZERO, 1.0);
    // Turned a quarter around (200, 100), its right edge faces down.
    let right = Grip { x: 1, y: 0 };
    assert_eq!(handles.grip_at(egui::pos2(200.0, 200.0)), Some(right));
    assert_eq!(handles.grip_at(egui::pos2(300.0, 100.0)), None);
    assert!(handles.centre().distance(egui::pos2(200.0, 100.0)) < 1e-3);
    // Dragging it down makes it wider.
    let drag = UiDrag::start(&handles, right, egui::pos2(200.0, 200.0), &world).unwrap();
    let edit = moved(&drag, egui::pos2(200.0, 230.0), 1.0, &world, &layout);
    assert_eq!(udim2(&edit, "Size").unwrap().x.offset, 230.0);
    // Its knobs are just outside its turned corners.
    let corner = handles.on_screen(handles.rect.right_bottom());
    assert!(
        handles.rotate_at(corner + egui::vec2(-8.0, 8.0)),
        "{corner:?}"
    );
    assert!(!handles.rotate_at(egui::pos2(200.0, 100.0)));
}

#[test]
fn handles_sit_on_the_corners_and_edges_and_the_body_is_inside() {
    let (_, _, handles, _) = frame("");
    assert_eq!(
        handles.rect,
        egui::Rect::from_min_size(egui::pos2(100.0, 50.0), egui::vec2(200.0, 80.0))
    );
    assert_eq!(
        handles.grip_at(egui::pos2(300.0, 130.0)),
        Some(Grip { x: 1, y: 1 })
    );
    assert_eq!(
        handles.grip_at(egui::pos2(102.0, 90.0)),
        Some(Grip { x: -1, y: 0 })
    );
    assert_eq!(handles.grip_at(egui::pos2(200.0, 90.0)), Some(Grip::BODY));
    assert_eq!(handles.grip_at(egui::pos2(500.0, 500.0)), None);
}

#[test]
fn dragging_the_body_moves_and_the_left_edge_resizes_and_moves() {
    let (world, _, handles, layout) = frame("");
    let start = egui::pos2(200.0, 90.0);
    let drag = UiDrag::start(&handles, Grip::BODY, start, &world).unwrap();
    let edit = moved(&drag, start + egui::vec2(10.4, -5.0), 1.0, &world, &layout);
    assert!(!edit.resize);
    assert_eq!(
        udim2(&edit, "Position"),
        Some(UDim2::new(0.0, 110.0, 0.0, 45.0))
    );

    let start = egui::pos2(100.0, 90.0);
    let drag = UiDrag::start(&handles, Grip { x: -1, y: 0 }, start, &world).unwrap();
    let edit = moved(&drag, start + egui::vec2(-20.0, 30.0), 1.0, &world, &layout);
    assert!(edit.resize && edit.css.is_empty());
    assert_eq!(
        udim2(&edit, "Size"),
        Some(UDim2::new(0.0, 220.0, 0.0, 80.0))
    );
    assert_eq!(
        udim2(&edit, "Position"),
        Some(UDim2::new(0.0, 80.0, 0.0, 50.0))
    );
}

#[test]
fn a_css_width_is_rewritten_and_a_variable_locks_its_axis() {
    let css = ".hud { width: 200px; height: var(--tall, 80px) }";
    let (world, _, handles, layout) = frame(css);
    assert!(matches!(handles.width, Axis::Css(_)));
    assert!(matches!(handles.height, Axis::Locked(_)));
    assert!(!handles.can_drag(Grip { x: 0, y: 1 }));
    assert!(handles.can_drag(Grip { x: 1, y: 1 }));

    let start = egui::pos2(300.0, 130.0);
    let drag = UiDrag::start(&handles, Grip { x: 1, y: 1 }, start, &world).unwrap();
    // Two pixels per point: 25 points is 50 pixels.
    let edit = moved(&drag, start + egui::vec2(25.0, 25.0), 2.0, &world, &layout);
    assert_eq!(
        udim2(&edit, "Size"),
        None,
        "CSS decides the width, and the height is locked"
    );
    assert_eq!(edit.css.len(), 1);
    assert_eq!(
        apply_css_edits(css, edit.css.iter()),
        ".hud { width: 250px; height: var(--tall, 80px) }"
    );
}

#[test]
fn several_edits_to_one_text_keep_their_places() {
    let text = "a { width: 10px; height: 20px }";
    let edit = |span: Range<usize>, value: &str| CssEdit {
        path: "ui.css".into(),
        span,
        value: value.into(),
    };
    let edits = [edit(11..15, "100px"), edit(25..29, "5px")];
    assert_eq!(
        apply_css_edits(text, edits.iter()),
        "a { width: 100px; height: 5px }"
    );
}

fn add_frame(
    world: &mut World,
    parent: InstanceId,
    name: &str,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
) -> InstanceId {
    let frame = world.create(ClassName::Frame).unwrap();
    world.set_parent(frame, Some(parent)).unwrap();
    world.set_name(frame, name).unwrap();
    world
        .set_property(frame, "Position", Value::UDim2(UDim2::new(0.0, x, 0.0, y)))
        .unwrap();
    world
        .set_property(
            frame,
            "Size",
            Value::UDim2(UDim2::new(0.0, width, 0.0, height)),
        )
        .unwrap();
    frame
}

/// A ScreenGui of absolutely positioned frames, laid out in the test viewport.
fn screen_with(frames: &[(&str, f32, f32, f32, f32)]) -> (World, UiLayout) {
    let mut world = World::new();
    let gui = world.create(ClassName::ScreenGui).unwrap();
    world.set_parent(gui, Some(world.workspace())).unwrap();
    for &(name, x, y, width, height) in frames {
        add_frame(&mut world, gui, name, x, y, width, height);
    }
    let layout = axcenity_ui::layout_ui(&world, 800, 600, &[]);
    (world, layout)
}

fn drag_named(
    world: &World,
    layout: &UiLayout,
    name: &str,
    grip: Grip,
    by: egui::Vec2,
) -> UiDragEdit {
    let id = layout
        .elements
        .iter()
        .find(|element| world.get(element.id).unwrap().name() == name)
        .unwrap()
        .id;
    let element = *layout.element(id).unwrap();
    let handles = Handles::new(element, world, &[], egui::Pos2::ZERO, 1.0);
    let start = handles.rect.center();
    let drag = UiDrag::start(&handles, grip, start, world).unwrap();
    drag.update(start + by, 1.0, world, &layout.elements, viewport())
}

#[test]
fn dragging_snaps_to_another_elements_edge_and_draws_a_guide() {
    // A is 100,50 200×80, so its right edge is x=300. B is 320,200 40×40.
    // Dragging B left 12px puts its left edge at 308, within 8px of A's right.
    let (world, layout) = screen_with(&[
        ("A", 100.0, 50.0, 200.0, 80.0),
        ("B", 320.0, 200.0, 40.0, 40.0),
    ]);
    let edit = drag_named(&world, &layout, "B", Grip::BODY, egui::vec2(-12.0, 0.0));
    assert_eq!(
        udim2(&edit, "Position"),
        Some(UDim2::new(0.0, 300.0, 0.0, 200.0))
    );
    assert_eq!(
        edit.guides,
        vec![SnapGuide {
            horizontal: false,
            at: 300.0,
            from: 50.0,
            to: 240.0,
        }]
    );

    // 16px away is outside the snap distance, so B follows the pointer.
    let edit = drag_named(&world, &layout, "B", Grip::BODY, egui::vec2(-4.0, 0.0));
    assert_eq!(
        udim2(&edit, "Position"),
        Some(UDim2::new(0.0, 316.0, 0.0, 200.0))
    );
    assert!(edit.guides.is_empty());
}

#[test]
fn dragging_snaps_centers_and_resizing_snaps_the_moving_edge() {
    // A's center is x=200. B's center starts at 340. A drag of -136
    // leaves it at 204, four pixels off, so it snaps the rest of the way.
    let (world, layout) = screen_with(&[
        ("A", 100.0, 50.0, 200.0, 80.0),
        ("B", 320.0, 200.0, 40.0, 40.0),
    ]);
    let edit = drag_named(&world, &layout, "B", Grip::BODY, egui::vec2(-136.0, 0.0));
    assert_eq!(
        udim2(&edit, "Position"),
        Some(UDim2::new(0.0, 180.0, 0.0, 200.0))
    );

    // B's right edge is 290. Pulling it 6px toward A's right edge at 300 snaps.
    let (world, layout) = screen_with(&[
        ("A", 100.0, 50.0, 200.0, 80.0),
        ("B", 250.0, 200.0, 40.0, 40.0),
    ]);
    let edit = drag_named(
        &world,
        &layout,
        "B",
        Grip { x: 1, y: 0 },
        egui::vec2(6.0, 0.0),
    );
    assert_eq!(udim2(&edit, "Size"), Some(UDim2::new(0.0, 50.0, 0.0, 40.0)));
    assert_eq!(udim2(&edit, "Position"), None);
    assert_eq!(edit.guides.len(), 1);
    assert_eq!(edit.guides[0].at, 300.0);
    assert!(!edit.guides[0].horizontal);
}

#[test]
fn a_drag_snaps_to_a_ruler_guide() {
    // A's left edge is 100. A guide at x=130 is in reach after a 26px
    // drag (126), and pulls it the rest of the way.
    let (world, layout) = screen_with(&[("A", 100.0, 50.0, 200.0, 80.0)]);
    let id = layout.elements[0].id;
    let element = *layout.element(id).unwrap();
    let handles = Handles::new(element, &world, &[], egui::Pos2::ZERO, 1.0);
    let start = handles.rect.center();
    let drag = UiDrag::start(&handles, Grip::BODY, start, &world).unwrap();
    let guide = UiRect {
        x: 130.0,
        y: 0.0,
        width: 0.0,
        height: 600.0,
    };
    let pointer = start + egui::vec2(26.0, 0.0);
    let edit =
        drag.update_with_guides(pointer, 1.0, &world, &layout.elements, viewport(), &[guide]);
    assert_eq!(
        udim2(&edit, "Position"),
        Some(UDim2::new(0.0, 130.0, 0.0, 50.0))
    );
    assert_eq!(edit.guides[0].at, 130.0);
    let unguided = drag.update(pointer, 1.0, &world, &layout.elements, viewport());
    assert_eq!(
        udim2(&unguided, "Position"),
        Some(UDim2::new(0.0, 126.0, 0.0, 50.0))
    );
}

#[test]
fn a_drag_snaps_to_the_screen_but_not_to_its_own_children() {
    // Center starts at x=200. +196 puts it at 396, four pixels left of
    // the screen's center.
    let (world, layout) = screen_with(&[("A", 100.0, 50.0, 200.0, 80.0)]);
    let edit = drag_named(&world, &layout, "A", Grip::BODY, egui::vec2(196.0, 0.0));
    assert_eq!(
        udim2(&edit, "Position"),
        Some(UDim2::new(0.0, 300.0, 0.0, 50.0))
    );

    let mut world = World::new();
    let gui = world.create(ClassName::ScreenGui).unwrap();
    world.set_parent(gui, Some(world.workspace())).unwrap();
    let parent = add_frame(&mut world, gui, "Parent", 100.0, 100.0, 200.0, 200.0);
    add_frame(&mut world, parent, "Child", 0.0, 0.0, 20.0, 20.0);
    let layout = axcenity_ui::layout_ui(&world, 800, 600, &[]);
    let child = layout
        .elements
        .iter()
        .find(|element| world.get(element.id).unwrap().name() == "Child")
        .unwrap();
    assert_eq!(
        child.rect.x, 100.0,
        "the child shares the parent's left edge"
    );
    // The child's left edge sits on the parent's. Snapping to it would
    // hold the parent still; five pixels of drag should stick.
    let edit = drag_named(&world, &layout, "Parent", Grip::BODY, egui::vec2(5.0, 0.0));
    assert_eq!(
        udim2(&edit, "Position"),
        Some(UDim2::new(0.0, 105.0, 0.0, 100.0))
    );
}
