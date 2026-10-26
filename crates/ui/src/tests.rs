use super::*;

#[test]
fn scale_and_offset_layout_is_recomputed_for_the_viewport_size() {
    let mut world = World::new();
    let gui = world.create(ClassName::ScreenGui).unwrap();
    world.set_parent(gui, Some(world.workspace())).unwrap();
    let frame = world.create(ClassName::Frame).unwrap();
    world.set_parent(frame, Some(gui)).unwrap();
    world
        .set_property(
            frame,
            "Position",
            Value::UDim2(UDim2::new(0.5, 10.0, 0.25, 5.0)),
        )
        .unwrap();
    world
        .set_property(
            frame,
            "Size",
            Value::UDim2(UDim2::new(0.5, -20.0, 0.5, -10.0)),
        )
        .unwrap();

    let small = build_ui_draw_data(&world, 800, 600, &[]);
    let large = build_ui_draw_data(&world, 1200, 800, &[]);
    let rectangle = |items: &[UiDrawItem]| match &items[0] {
        UiDrawItem::Rectangle(rect) => *rect,
        UiDrawItem::Text(_) | UiDrawItem::Image(_) => {
            panic!("frame layout starts with its rectangle")
        }
    };
    assert_eq!(rectangle(&small).position, [410.0, 155.0]);
    assert_eq!(rectangle(&small).size, [380.0, 290.0]);
    assert_eq!(rectangle(&large).position, [610.0, 205.0]);
    assert_eq!(rectangle(&large).size, [580.0, 390.0]);
}

#[test]
fn fit_mode_scales_uniformly_and_centers_on_a_different_aspect() {
    let mut world = World::new();
    let gui = world.create(ClassName::ScreenGui).unwrap();
    world.set_parent(gui, Some(world.workspace())).unwrap();
    world
        .set_property(gui, "ScaleMode", Value::String("Fit".to_owned()))
        .unwrap();
    world
        .set_property(gui, "DesignWidth", Value::Number(1920.0))
        .unwrap();
    world
        .set_property(gui, "DesignHeight", Value::Number(1080.0))
        .unwrap();
    let frame = world.create(ClassName::Frame).unwrap();
    world.set_parent(frame, Some(gui)).unwrap();
    world
        .set_property(
            frame,
            "Position",
            Value::UDim2(UDim2::new(0.0, 100.0, 0.0, 40.0)),
        )
        .unwrap();
    world
        .set_property(
            frame,
            "Size",
            Value::UDim2(UDim2::new(0.0, 200.0, 0.0, 80.0)),
        )
        .unwrap();

    // Half the design width, same height: scale is limited by the width
    // (0.5) and the spare height is split above and below.
    let layout = layout_ui(&world, 960, 1080, &[]);
    let rect = layout.element(frame).unwrap().rect;
    assert_eq!(
        rect,
        UiRect {
            x: 50.0,
            y: 290.0,
            width: 100.0,
            height: 40.0
        }
    );
    assert!((layout.element(frame).unwrap().pixels_per_unit - 0.5).abs() < 0.001);
}

#[test]
fn an_artboard_is_one_screen_gui_at_its_design_size_scaled() {
    let mut world = World::new();
    let mut frames = Vec::new();
    for (mode, enabled) in [("Fixed", false), ("Fit", true)] {
        let gui = world.create(ClassName::ScreenGui).unwrap();
        world.set_parent(gui, Some(world.workspace())).unwrap();
        for (name, value) in [
            ("ScaleMode", Value::String(mode.to_owned())),
            ("Enabled", Value::Bool(enabled)),
            ("DesignWidth", Value::Number(400.0)),
            ("DesignHeight", Value::Number(300.0)),
        ] {
            world.set_property(gui, name, value).unwrap();
        }
        let frame = world.create(ClassName::Frame).unwrap();
        world.set_parent(frame, Some(gui)).unwrap();
        world
            .set_property(
                frame,
                "Position",
                Value::UDim2(UDim2::new(0.5, -50.0, 0.0, 20.0)),
            )
            .unwrap();
        frames.push((gui, frame));
    }
    assert_eq!(design_size(&world, frames[0].0), (400, 300));

    for &(gui, frame) in &frames {
        // At scale 1 it matches a 400×300 screen with only this layer on.
        let artboard = layout_artboard(&world, gui, &[], 1.0, [0.0, 0.0]);
        assert_eq!(artboard.elements.len(), 1, "only this ScreenGui");
        assert_eq!(artboard.elements[0].rect.x, 150.0);
        assert_eq!(artboard.elements[0].rect.y, 20.0);

        let zoomed = layout_artboard(&world, gui, &[], 0.5, [10.0, -4.0]);
        let element = zoomed.element(frame).unwrap();
        assert_eq!(element.rect.x, 10.0 + 75.0);
        assert_eq!(element.rect.y, -4.0 + 10.0);
        assert!((element.pixels_per_unit - 0.5).abs() < 1e-6);
    }
    // The enabled Fit layer, on a 400×300 screen, draws what its artboard draws.
    let screen = layout_ui(&world, 400, 300, &[]);
    let artboard = layout_artboard(&world, frames[1].0, &[], 1.0, [0.0, 0.0]);
    assert_eq!(
        format!("{:?}", screen.draw_items),
        format!("{:?}", artboard.draw_items)
    );
}

#[test]
fn changing_a_text_label_updates_the_draw_data() {
    let mut world = World::new();
    let gui = world.create(ClassName::ScreenGui).unwrap();
    world.set_parent(gui, Some(world.workspace())).unwrap();
    let label = world.create(ClassName::TextLabel).unwrap();
    world.set_parent(label, Some(gui)).unwrap();
    world
        .set_property(label, "Text", Value::String("Before".to_owned()))
        .unwrap();

    let first = build_ui_draw_data(&world, 640, 480, &[]);
    world
        .set_property(label, "Text", Value::String("After".to_owned()))
        .unwrap();
    let next = build_ui_draw_data(&world, 640, 480, &[]);

    assert!(first.iter().any(|item| matches!(
        item,
        UiDrawItem::Text(text) if text.text == "Before"
    )));
    assert!(next.iter().any(|item| matches!(
        item,
        UiDrawItem::Text(text) if text.text == "After"
    )));
}

#[test]
fn buttons_draw_text_track_pointer_states_and_ignore_hidden_hit_targets() {
    let mut world = World::new();
    let gui = world.create(ClassName::ScreenGui).unwrap();
    world.set_parent(gui, Some(world.workspace())).unwrap();
    let button = world.create(ClassName::TextButton).unwrap();
    world.set_parent(button, Some(gui)).unwrap();
    world
        .set_property(button, "Class", Value::String("start".into()))
        .unwrap();
    world
        .set_property(button, "Text", Value::String("Start".into()))
        .unwrap();
    let (sheet, warnings) = parse_stylesheet(
        "ui.css",
        ".start { width: 120px; height: 40px; background-color: #000000 } \
         .start:hover { background-color: #00ff00 } \
         .start:active { background-color: #ff0000 }",
    );
    assert!(warnings.is_empty());
    let hovered = layout_ui_interactive(
        &world,
        640,
        480,
        std::slice::from_ref(&sheet),
        Some([20.0, 20.0]),
        None,
        None,
        None,
    );
    assert_eq!(hovered.hovered, Some(button));
    assert_eq!(hovered.hit_button(20.0, 20.0), Some(button));
    assert!(hovered.draw_items.iter().any(|item| matches!(
        item,
        UiDrawItem::Text(text) if text.text == "Start"
    )));
    let active = layout_ui_interactive(
        &world,
        640,
        480,
        &[sheet],
        Some([20.0, 20.0]),
        Some(button),
        None,
        None,
    );
    assert_eq!(active.active, Some(button));
    let Some(UiDrawItem::Rectangle(rectangle)) = active.draw_items.first() else {
        panic!("the button background should be drawn first");
    };
    assert_eq!(rectangle.color[..3], [1.0, 0.0, 0.0]);

    world
        .set_property(button, "Visible", Value::Bool(false))
        .unwrap();
    let hidden = layout_ui_interactive(
        &world,
        640,
        480,
        &[],
        Some([20.0, 20.0]),
        Some(button),
        None,
        None,
    );
    assert_eq!(hidden.hit_button(20.0, 20.0), None);
    assert_eq!(hidden.active, None);
}

#[test]
fn css_applies_variables_colors_dimensions_borders_and_text_styles() {
    let mut world = World::new();
    let gui = world.create(ClassName::ScreenGui).unwrap();
    world.set_parent(gui, Some(world.workspace())).unwrap();
    let frame = world.create(ClassName::Frame).unwrap();
    world.set_parent(frame, Some(gui)).unwrap();
    world
        .set_property(frame, "Class", Value::String("hud".to_owned()))
        .unwrap();
    let label = world.create(ClassName::TextLabel).unwrap();
    world.set_parent(label, Some(frame)).unwrap();
    world
        .set_property(label, "Class", Value::String("score".to_owned()))
        .unwrap();
    let (stylesheet, warnings) = parse_stylesheet(
        "hud.css",
        ".hud { --panel: #123456; width: 50%; height: 30px; background-color: var(--panel); border: 2px solid #ffffff; border-radius: 8px; opacity: .5 } .score { color: #ff0000; font-size: 20px; font-family: Arial }",
    );
    assert!(warnings.is_empty());

    let items = build_ui_draw_data(&world, 800, 600, &[stylesheet]);
    let Some(UiDrawItem::Rectangle(frame)) = items.first() else {
        panic!("the frame background should be drawn first");
    };
    assert_eq!(frame.size, [400.0, 30.0]);
    assert!((frame.color[0] - 0x12 as f32 / 255.0).abs() < 0.001);
    assert_eq!(frame.color[3], 0.5);
    assert_eq!(frame.border_width, 2.0);
    assert_eq!(frame.border_radius, 8.0);
    assert!(items.iter().any(|item| matches!(
        item,
        UiDrawItem::Text(text)
            if text.font_size == 20.0
                && text.font_family == "Arial"
                && text.color == [1.0, 0.0, 0.0, 0.5]
    )));
}

#[test]
fn flex_containers_flow_children_with_gap_and_padding() {
    let mut world = World::new();
    let gui = world.create(ClassName::ScreenGui).unwrap();
    world.set_parent(gui, Some(world.workspace())).unwrap();
    let parent = world.create(ClassName::Frame).unwrap();
    world.set_parent(parent, Some(gui)).unwrap();
    world
        .set_property(parent, "Class", Value::String("row".to_owned()))
        .unwrap();
    let first = world.create(ClassName::Frame).unwrap();
    world.set_parent(first, Some(parent)).unwrap();
    world
        .set_property(first, "Class", Value::String("item".to_owned()))
        .unwrap();
    let second = world.create(ClassName::Frame).unwrap();
    world.set_parent(second, Some(parent)).unwrap();
    world
        .set_property(second, "Class", Value::String("item".to_owned()))
        .unwrap();
    let (stylesheet, warnings) = parse_stylesheet(
        "hud.css",
        ".row { display: flex; flex-direction: row; gap: 10px; padding: 10px; width: 400px; height: 50px } .item { width: 100px; height: 20px; margin: 5px; flex-grow: 1 }",
    );
    assert!(warnings.is_empty());

    let items = build_ui_draw_data(&world, 800, 600, &[stylesheet]);
    let rectangles: Vec<_> = items
        .iter()
        .filter_map(|item| match item {
            UiDrawItem::Rectangle(rectangle) => Some(rectangle),
            UiDrawItem::Text(_) | UiDrawItem::Image(_) => None,
        })
        .collect();
    assert_eq!(rectangles.len(), 3);
    assert_eq!(rectangles[1].position, [15.0, 15.0]);
    assert_eq!(rectangles[2].position, [210.0, 15.0]);
    assert_eq!(rectangles[1].size, [175.0, 20.0]);
    assert_eq!(rectangles[2].size, [175.0, 20.0]);
}

#[test]
fn the_health_hud_example_uses_only_supported_css() {
    let (stylesheet, warnings) =
        parse_stylesheet("ui.css", include_str!("../../../examples/hello-css/ui.css"));
    assert!(warnings.is_empty(), "{warnings:#?}");
    let mut world = World::new();
    let gui = world.create(ClassName::ScreenGui).unwrap();
    world.set_parent(gui, Some(world.workspace())).unwrap();
    let panel = world.create(ClassName::Frame).unwrap();
    world.set_parent(panel, Some(gui)).unwrap();
    world
        .set_property(panel, "Class", Value::String("hud".to_owned()))
        .unwrap();

    let items = build_ui_draw_data(&world, 800, 600, &[stylesheet]);
    let Some(UiDrawItem::Rectangle(panel)) = items.first() else {
        panic!("the health HUD panel should be drawn");
    };
    assert_eq!(panel.size, [280.0, 90.0]);
    assert_eq!(panel.color, [20.0 / 255.0, 34.0 / 255.0, 56.0 / 255.0, 1.0]);
}

#[test]
fn a_pixel_size_without_css_is_that_many_pixels() {
    let mut world = World::new();
    let gui = world.create(ClassName::ScreenGui).unwrap();
    world.set_parent(gui, Some(world.workspace())).unwrap();
    let frame = world.create(ClassName::Frame).unwrap();
    world.set_parent(frame, Some(gui)).unwrap();
    world
        .set_property(
            frame,
            "Size",
            Value::UDim2(UDim2::new(0.0, 120.0, 0.0, 40.0)),
        )
        .unwrap();

    let layout = layout_ui(&world, 800, 600, &[]);
    assert_eq!(layout.element(frame).unwrap().rect.width, 120.0);
    assert_eq!(layout.element(frame).unwrap().rect.height, 40.0);
}

#[test]
fn hits_find_the_topmost_element_and_flex_children_are_in_flow() {
    let mut world = World::new();
    let gui = world.create(ClassName::ScreenGui).unwrap();
    world.set_parent(gui, Some(world.workspace())).unwrap();
    let row = world.create(ClassName::Frame).unwrap();
    world.set_parent(row, Some(gui)).unwrap();
    world
        .set_property(row, "Class", Value::String("row".to_owned()))
        .unwrap();
    let item = world.create(ClassName::Frame).unwrap();
    world.set_parent(item, Some(row)).unwrap();
    let (stylesheet, _) = parse_stylesheet(
        "ui.css",
        ".row { display: flex; padding: 10px; width: 200px; height: 100px } Frame Frame { width: 50px; height: 50px }",
    );

    let layout = layout_ui(&world, 800, 600, &[stylesheet]);
    assert_eq!(layout.hit(30.0, 30.0), Some(item));
    assert_eq!(layout.hit(150.0, 80.0), Some(row));
    assert_eq!(layout.hit(500.0, 500.0), None);
    assert!(!layout.element(row).unwrap().in_flow);
    assert!(layout.element(item).unwrap().in_flow);
}

#[test]
fn invisible_flex_children_take_no_space_and_cannot_be_hit() {
    let mut world = World::new();
    let gui = world.create(ClassName::ScreenGui).unwrap();
    world.set_parent(gui, Some(world.workspace())).unwrap();
    let row = world.create(ClassName::Frame).unwrap();
    world.set_parent(row, Some(gui)).unwrap();
    world
        .set_property(row, "Class", Value::String("row".to_owned()))
        .unwrap();
    let hidden = world.create(ClassName::Frame).unwrap();
    world.set_parent(hidden, Some(row)).unwrap();
    world
        .set_property(hidden, "Visible", Value::Bool(false))
        .unwrap();
    let visible = world.create(ClassName::Frame).unwrap();
    world.set_parent(visible, Some(row)).unwrap();
    let (stylesheet, warnings) = parse_stylesheet(
        "ui.css",
        ".row { display: flex; width: 200px; height: 60px; gap: 10px } Frame Frame { width: 50px; height: 50px }",
    );
    assert!(warnings.is_empty());

    let layout = layout_ui(&world, 200, 60, &[stylesheet]);
    let element = layout.element(visible).expect("visible child is laid out");
    assert_eq!(element.rect.x, 0.0);
    assert_eq!(layout.element(hidden), None);
    assert_eq!(layout.hit(25.0, 25.0), Some(visible));
}

#[test]
fn css_lengths_are_resized_in_their_own_units() {
    assert_eq!(
        resized_css_length("280px", 280.0, 312.4),
        Some("312px".to_owned())
    );
    assert_eq!(
        resized_css_length(" 72% ", 180.0, 90.0),
        Some("36%".to_owned())
    );
    assert_eq!(resized_css_length("50%", 0.0, 90.0), None);
    assert_eq!(resized_css_length("var(--width)", 10.0, 20.0), None);
    assert_eq!(resized_css_length("auto", 10.0, 20.0), None);
    assert_eq!(
        resized_css_length("10px", 10.0, -5.0),
        Some("0px".to_owned())
    );
}
