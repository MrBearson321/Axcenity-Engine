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
        let artboard = layout_artboard(&world, gui, &[], (400, 300), 1.0, [0.0, 0.0]);
        assert_eq!(artboard.elements.len(), 1, "only this ScreenGui");
        assert_eq!(artboard.elements[0].rect.x, 150.0);
        assert_eq!(artboard.elements[0].rect.y, 20.0);

        let zoomed = layout_artboard(&world, gui, &[], (400, 300), 0.5, [10.0, -4.0]);
        let element = zoomed.element(frame).unwrap();
        assert_eq!(element.rect.x, 10.0 + 75.0);
        assert_eq!(element.rect.y, -4.0 + 10.0);
        assert!((element.pixels_per_unit - 0.5).abs() < 1e-6);
    }
    // The enabled Fit layer, on a 400×300 screen, draws what its artboard draws.
    let screen = layout_ui(&world, 400, 300, &[]);
    let artboard = layout_artboard(&world, frames[1].0, &[], (400, 300), 1.0, [0.0, 0.0]);
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
        UiInput {
            pointer: Some([20.0, 20.0]),
            pressed: None,
            ..UiInput::default()
        },
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
        UiInput {
            pointer: Some([20.0, 20.0]),
            pressed: Some(button),
            ..UiInput::default()
        },
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
        UiInput {
            pointer: Some([20.0, 20.0]),
            pressed: Some(button),
            ..UiInput::default()
        },
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
fn an_element_knows_its_text_size_from_css_or_text_size() {
    let mut world = World::new();
    let gui = world.create(ClassName::ScreenGui).unwrap();
    world.set_parent(gui, Some(world.workspace())).unwrap();
    let styled = world.create(ClassName::TextLabel).unwrap();
    world.set_parent(styled, Some(gui)).unwrap();
    world
        .set_property(styled, "Class", Value::String("title".to_owned()))
        .unwrap();
    world
        .set_property(styled, "TextSize", Value::Number(12.0))
        .unwrap();
    let plain = world.create(ClassName::TextLabel).unwrap();
    world.set_parent(plain, Some(gui)).unwrap();
    world
        .set_property(plain, "TextSize", Value::Number(12.0))
        .unwrap();
    let (stylesheet, _) = parse_stylesheet("ui.css", ".title { font-size: 26px }");

    let layout = layout_artboard(&world, gui, &[stylesheet], (800, 600), 2.0, [0.0, 0.0]);
    let styled = layout.element(styled).unwrap();
    assert_eq!(styled.font_size, 26.0);
    assert_eq!(styled.pixels_per_unit, 2.0);
    assert_eq!(layout.element(plain).unwrap().font_size, 12.0);
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

#[test]
fn scale_mode_scales_evenly_and_keeps_edges_on_the_screen() {
    let mut world = World::new();
    let gui = world.create(ClassName::ScreenGui).unwrap();
    world.set_parent(gui, Some(world.workspace())).unwrap();
    for (name, value) in [
        ("ScaleMode", Value::String("Scale".to_owned())),
        ("DesignWidth", Value::Number(1280.0)),
        ("DesignHeight", Value::Number(720.0)),
    ] {
        world.set_property(gui, name, value).unwrap();
    }
    // 100×40 pinned 20 pixels in from the bottom right corner.
    let corner = world.create(ClassName::Frame).unwrap();
    world.set_parent(corner, Some(gui)).unwrap();
    world
        .set_property(
            corner,
            "Position",
            Value::UDim2(UDim2::new(1.0, -120.0, 1.0, -60.0)),
        )
        .unwrap();
    world
        .set_property(
            corner,
            "Size",
            Value::UDim2(UDim2::new(0.0, 100.0, 0.0, 40.0)),
        )
        .unwrap();

    // At the design size it's exactly as authored.
    let rect = layout_ui(&world, 1280, 720, &[])
        .element(corner)
        .unwrap()
        .rect;
    assert_eq!([rect.x, rect.y, rect.width], [1160.0, 660.0, 100.0]);

    // Twice the size: twice as big, still 40 pixels from the corner.
    let rect = layout_ui(&world, 2560, 1440, &[])
        .element(corner)
        .unwrap()
        .rect;
    assert_eq!([rect.x, rect.y, rect.width], [2320.0, 1320.0, 200.0]);

    // An ultrawide screen: scaled by the height, and the corner element
    // follows the real right edge instead of a centred 16:9 box.
    let layout = layout_ui(&world, 3440, 1440, &[]);
    let element = layout.element(corner).unwrap();
    assert_eq!(element.pixels_per_unit, 2.0);
    assert_eq!(element.rect.x + element.rect.width, 3440.0 - 40.0);
    assert_eq!(screen_mapping(&world, gui, 3440, 1440), (2.0, [0.0, 0.0]));

    // The artboard previews any screen the way Play shows it.
    let artboard = layout_artboard(&world, gui, &[], (3440, 1440), 0.25, [5.0, 5.0]);
    let small = artboard.element(corner).unwrap().rect;
    assert_eq!(small.x, 5.0 + element.rect.x * 0.25);
    assert_eq!(small.width, element.rect.width * 0.25);
}

/// The text drawn for the first text element in `items`.
fn first_text(items: &[UiDrawItem]) -> &UiText {
    items
        .iter()
        .find_map(|item| match item {
            UiDrawItem::Text(text) => Some(text),
            _ => None,
        })
        .expect("some text is drawn")
}

#[test]
fn text_styles_are_parsed_into_the_text_drawn() {
    let mut world = World::new();
    let gui = world.create(ClassName::ScreenGui).unwrap();
    world.set_parent(gui, Some(world.workspace())).unwrap();
    let label = world.create(ClassName::TextLabel).unwrap();
    world.set_parent(label, Some(gui)).unwrap();
    world
        .set_property(label, "Id", Value::String("Title".to_owned()))
        .unwrap();
    world
        .set_property(label, "Text", Value::String("play now".to_owned()))
        .unwrap();
    let (stylesheet, warnings) = parse_stylesheet(
        "ui.css",
        "#Title { font-size: 20px; font-weight: bold; font-style: italic; \
         text-decoration: underline line-through; text-transform: capitalize; \
         -webkit-text-stroke: 2px #ff0000; \
         text-shadow: 1px 2px #000, 3px 4px 5px rgba(0, 0, 255, 0.5); \
         letter-spacing: 0.1em; line-height: 1.5; vertical-align: bottom; white-space: nowrap }",
    );
    assert!(warnings.is_empty(), "{warnings:?}");
    let items = build_ui_draw_data(&world, 800, 600, &[stylesheet]);
    let text = first_text(&items);
    assert_eq!(text.text, "Play Now");
    assert_eq!(text.weight, 700);
    assert!(text.italic && text.underline && text.line_through);
    assert_eq!(
        text.stroke,
        Some(TextStroke {
            width: 2.0,
            color: [1.0, 0.0, 0.0, 1.0],
        })
    );
    assert_eq!(text.shadows.len(), 2);
    assert_eq!(text.shadows[0].offset, [1.0, 2.0]);
    assert_eq!(text.shadows[0].blur, 0.0);
    assert_eq!(text.shadows[1].offset, [3.0, 4.0]);
    assert_eq!(text.shadows[1].blur, 5.0);
    assert_eq!(text.shadows[1].color, [0.0, 0.0, 1.0, 0.5]);
    assert!((text.letter_spacing - 2.0).abs() < 1e-5);
    assert_eq!(text.line_height, Some(30.0));
    assert_eq!(text.vertical_align, VerticalAlign::Bottom);
    assert!(!text.wrap && !text.scaled);

    // Unstyled: normal, upright, wrapping text at the top of a label and
    // in the middle of a button.
    let plain = build_ui_draw_data(&world, 800, 600, &[]);
    let text = first_text(&plain);
    assert_eq!((text.weight, text.italic, text.wrap), (400, false, true));
    assert_eq!(text.text, "play now");
    assert_eq!(text.vertical_align, VerticalAlign::Top);
    assert!(text.stroke.is_none() && text.shadows.is_empty());
    let button = world.create(ClassName::TextButton).unwrap();
    world.set_parent(button, Some(gui)).unwrap();
    world.destroy(label).unwrap();
    let items = build_ui_draw_data(&world, 800, 600, &[]);
    assert_eq!(first_text(&items).vertical_align, VerticalAlign::Middle);
}

#[test]
fn text_styles_and_text_scaled_draw_the_same_on_the_artboard_and_in_play() {
    let mut world = World::new();
    let gui = world.create(ClassName::ScreenGui).unwrap();
    world.set_parent(gui, Some(world.workspace())).unwrap();
    for (name, value) in [
        ("ScaleMode", Value::String("Scale".into())),
        ("DesignWidth", Value::Number(800.0)),
        ("DesignHeight", Value::Number(600.0)),
    ] {
        world.set_property(gui, name, value).unwrap();
    }
    let label = world.create(ClassName::TextLabel).unwrap();
    world.set_parent(label, Some(gui)).unwrap();
    world
        .set_property(label, "Class", Value::String("fancy".to_owned()))
        .unwrap();
    world
        .set_property(label, "TextScaled", Value::Bool(true))
        .unwrap();
    let (stylesheet, _) = parse_stylesheet(
        "ui.css",
        ".fancy { -webkit-text-stroke: 2px black; text-shadow: 2px 2px 4px #000; \
         letter-spacing: 3px; line-height: 24px; vertical-align: middle; \
         white-space: nowrap; text-transform: uppercase }",
    );
    let sheets = [stylesheet];
    let play = layout_ui(&world, 800, 600, &sheets);
    let board = layout_artboard(&world, gui, &sheets, (800, 600), 1.0, [0.0, 0.0]);
    let in_play = first_text(&play.draw_items);
    assert_eq!(in_play, first_text(&board.draw_items));
    assert!(in_play.scaled);
    assert_eq!(in_play.text, "LABEL");

    // On a screen twice the design size, every length doubles.
    let big = layout_ui(&world, 1600, 1200, &sheets);
    let doubled = first_text(&big.draw_items);
    assert_eq!(doubled.font_size, in_play.font_size * 2.0);
    assert_eq!(doubled.letter_spacing, 6.0);
    assert_eq!(doubled.line_height, Some(48.0));
    assert_eq!(doubled.stroke.unwrap().width, 4.0);
    assert_eq!(doubled.shadows[0].offset, [4.0, 4.0]);
    assert_eq!(doubled.shadows[0].blur, 8.0);
}

#[test]
fn text_style_properties_are_supported_and_text_stroke_has_both_names() {
    let (stylesheet, warnings) = parse_stylesheet(
        "ui.css",
        "#A { text-stroke: 1px white; font-weight: 300 } #B { font-weight: nonsense }",
    );
    assert!(warnings.is_empty(), "{warnings:?}");
    let mut world = World::new();
    let gui = world.create(ClassName::ScreenGui).unwrap();
    world.set_parent(gui, Some(world.workspace())).unwrap();
    let a = world.create(ClassName::TextLabel).unwrap();
    world.set_parent(a, Some(gui)).unwrap();
    world
        .set_property(a, "Id", Value::String("A".to_owned()))
        .unwrap();
    let items = build_ui_draw_data(&world, 800, 600, std::slice::from_ref(&stylesheet));
    let text = first_text(&items);
    assert_eq!(text.stroke.unwrap().width, 1.0);
    assert_eq!(text.weight, 300);
    world
        .set_property(a, "Id", Value::String("B".to_owned()))
        .unwrap();
    let items = build_ui_draw_data(&world, 800, 600, &[stylesheet]);
    assert_eq!(first_text(&items).weight, 400);
}

/// A label in a ScreenGui designed at 800×600 in Scale, with `RichText` on
/// and `text`.
fn rich_label(text: &str) -> (World, InstanceId, InstanceId) {
    let mut world = World::new();
    let gui = world.create(ClassName::ScreenGui).unwrap();
    world.set_parent(gui, Some(world.workspace())).unwrap();
    for (name, value) in [
        ("ScaleMode", Value::String("Scale".into())),
        ("DesignWidth", Value::Number(800.0)),
        ("DesignHeight", Value::Number(600.0)),
    ] {
        world.set_property(gui, name, value).unwrap();
    }
    let label = world.create(ClassName::TextLabel).unwrap();
    world.set_parent(label, Some(gui)).unwrap();
    world.set_name(label, "Prompt").unwrap();
    world
        .set_property(label, "RichText", Value::Bool(true))
        .unwrap();
    world
        .set_property(label, "Text", Value::String(text.into()))
        .unwrap();
    (world, gui, label)
}

#[test]
fn rich_text_tags_become_styled_spans_in_play_and_on_the_artboard() {
    let (mut world, gui, label) =
        rich_label(r##"Press <b>E</b> to <font color="#ff0000">open</font>"##);
    let (stylesheet, _) = parse_stylesheet(
        "ui.css",
        "TextLabel { color: #ffffff; font-size: 20px; text-transform: capitalize }",
    );
    let sheets = [stylesheet];
    let play = layout_ui(&world, 800, 600, &sheets);
    assert!(play.warnings.is_empty(), "{:?}", play.warnings);
    let text = first_text(&play.draw_items);
    // Capitalized across the spans: the tags don't start new words.
    assert_eq!(text.text, "Press E To Open");
    let spans: Vec<&str> = text.spans.iter().map(|span| span.text.as_str()).collect();
    assert_eq!(spans, ["Press ", "E", " To ", "Open"]);
    assert_eq!(text.spans[1].weight, 700);
    assert_eq!(text.spans[0].weight, 400);
    assert_eq!(text.spans[3].color, [1.0, 0.0, 0.0, 1.0]);
    assert_eq!(text.spans[2].color, [1.0, 1.0, 1.0, 1.0]);
    assert!(text.spans.iter().all(|span| span.font_size == 20.0));
    let board = layout_artboard(&world, gui, &sheets, (800, 600), 1.0, [0.0, 0.0]);
    assert_eq!(text, first_text(&board.draw_items));

    // On a screen twice the size, the spans' sizes and outlines double too.
    world
        .set_property(
            label,
            "Text",
            Value::String(
                "<font size=\"30\" face=\"Bebas Neue\"><stroke thickness=\"2\">Big</stroke></font>"
                    .into(),
            ),
        )
        .unwrap();
    let big = layout_ui(&world, 1600, 1200, &sheets);
    let span = &first_text(&big.draw_items).spans[0];
    assert_eq!(span.font_size, 60.0);
    assert_eq!(span.font_family, "Bebas Neue");
    assert_eq!(span.stroke.unwrap().width, 4.0);

    // With RichText off, the tags are just text.
    world
        .set_property(label, "RichText", Value::Bool(false))
        .unwrap();
    world
        .set_property(label, "Text", Value::String("<b>E</b>".into()))
        .unwrap();
    let plain = layout_ui(&world, 800, 600, &[]);
    let text = first_text(&plain.draw_items);
    assert_eq!(text.text, "<b>E</b>");
    assert!(text.spans.is_empty());
}

#[test]
fn malformed_rich_text_draws_as_plain_text_with_a_warning() {
    let (world, _, _) = rich_label("<b>open");
    let layout = layout_ui(&world, 800, 600, &[]);
    let text = first_text(&layout.draw_items);
    assert_eq!(text.text, "<b>open");
    assert!(text.spans.is_empty());
    assert_eq!(text.weight, 400);
    assert_eq!(layout.warnings.len(), 1, "{:?}", layout.warnings);
    let warning = &layout.warnings[0];
    assert!(warning.contains("\"Prompt\""), "{warning}");
    assert!(warning.contains("<b> is never closed"), "{warning}");
    // The same line each frame, which hosts show once.
    assert_eq!(layout_ui(&world, 800, 600, &[]).warnings, layout.warnings);
}

/// A ScreenGui with a 160×48 TextButton `#play` at the top left, and a
/// stylesheet for it.
fn play_button(css: &str) -> (World, InstanceId, Vec<Stylesheet>) {
    let mut world = World::new();
    let gui = world.create(ClassName::ScreenGui).unwrap();
    world.set_parent(gui, Some(world.workspace())).unwrap();
    let button = world.create(ClassName::TextButton).unwrap();
    world.set_parent(button, Some(gui)).unwrap();
    world
        .set_property(button, "Id", Value::String("play".into()))
        .unwrap();
    let (sheet, warnings) = parse_stylesheet("ui.css", css);
    assert!(warnings.is_empty(), "{warnings:?}");
    (world, button, vec![sheet])
}

/// The first rectangle drawn: the button's background.
fn background(layout: &UiLayout) -> UiRectangle {
    layout
        .draw_items
        .iter()
        .find_map(|item| match item {
            UiDrawItem::Rectangle(rectangle) => Some(*rectangle),
            _ => None,
        })
        .expect("the button has a background")
}

const OVER: UiInput = UiInput {
    pointer: Some([80.0, 24.0]),
    pressed: None,
    focused: None,
    on_top: None,
};
const AWAY: UiInput = UiInput {
    pointer: Some([500.0, 400.0]),
    pressed: None,
    focused: None,
    on_top: None,
};

#[test]
fn a_disabled_button_shows_its_disabled_style_and_takes_no_hover_or_click() {
    let (mut world, button, sheets) = play_button(
        "#play { background-color: #808080 } #play:hover { background-color: #00ff00 } \
         #play:disabled { background-color: #ff0000; cursor: not-allowed }",
    );
    world
        .set_property(button, "Interactable", Value::Bool(false))
        .unwrap();
    let mut ui = UiInteraction::default();
    let layout = ui.layout(&world, (640, 480), &sheets, OVER, None, 0.016);
    assert_eq!(background(&layout).color, [1.0, 0.0, 0.0, 1.0]);
    assert_eq!(layout.hovered, None);
    assert_eq!(layout.hit_button(80.0, 24.0), None);
    assert_eq!(layout.cursor, UiCursor::NotAllowed);
    assert!(ui.take_events().is_empty(), "no MouseEnter");
    assert!(!ui.activate(&world, button), "no Activated");

    world
        .set_property(button, "Interactable", Value::Bool(true))
        .unwrap();
    let layout = ui.layout(&world, (640, 480), &sheets, OVER, None, 0.016);
    assert_eq!(background(&layout).color, [0.0, 1.0, 0.0, 1.0]);
    assert_eq!(layout.hit_button(80.0, 24.0), Some(button));
    assert_eq!(
        layout.cursor,
        UiCursor::Pointer,
        "a button's hand by default"
    );
    assert!(ui.activate(&world, button));
}

#[test]
fn the_designer_shows_an_element_in_a_state_whatever_the_pointer_does() {
    let (world, button, sheets) = play_button(
        "#play { background-color: #808080 } #play:hover { background-color: #00ff00 } \
         #play:active { transform: scale(0.5) } #play:disabled { background-color: #ff0000 }",
    );
    let gui = world.get(button).unwrap().parent().unwrap();
    let shown = |state| {
        layout_artboard_in_state(
            &world,
            gui,
            &sheets,
            (640, 480),
            1.0,
            [0.0, 0.0],
            Some((button, state)),
        )
    };
    let rgb = |layout: &UiLayout| background(layout).color;
    assert_eq!(rgb(&shown(UiState::Normal))[..3], [128.0 / 255.0; 3]);
    assert_eq!(rgb(&shown(UiState::Hover)), [0.0, 1.0, 0.0, 1.0]);
    let pressed = shown(UiState::Pressed);
    assert_eq!(rgb(&pressed), [0.0, 1.0, 0.0, 1.0], ":hover still applies");
    assert_eq!(background(&pressed).size, [80.0, 24.0]);
    assert_eq!(
        background(&pressed).position,
        [40.0, 12.0],
        "around its centre"
    );
    assert_eq!(rgb(&shown(UiState::Disabled)), [1.0, 0.0, 0.0, 1.0]);
}

#[test]
fn a_transform_scales_and_moves_the_drawing_but_not_the_layout() {
    let (world, button, sheets) =
        play_button("#play { transform: translate(10px, 50%) scale(0.5, 2) }");
    let layout = layout_ui(&world, 640, 480, &sheets);
    let rectangle = background(&layout);
    // Centre (80, 24); scaled to 80×96 around it, then moved 10 across and
    // half its 48 down.
    assert_eq!(rectangle.size, [80.0, 96.0]);
    assert_eq!(rectangle.position, [50.0, 0.0]);
    let text = first_text(&layout.draw_items);
    assert_eq!(text.size[0], (160.0 - 8.0) * 0.5);
    let element = layout.element(button).unwrap();
    assert_eq!(
        (element.rect.width, element.rect.height),
        (160.0, 48.0),
        "clicks and handles use the laid-out box"
    );
}

#[test]
fn a_transition_eases_the_hover_colour_and_scale_instead_of_snapping() {
    let (world, _, sheets) = play_button(
        "#play { background-color: #000000; transition: all 0.2s linear } \
         #play:hover { background-color: #ffffff; transform: scale(0.9) }",
    );
    let mut ui = UiInteraction::default();
    let start = ui.layout(&world, (640, 480), &sheets, AWAY, None, 0.016);
    assert_eq!(background(&start).color, [0.0, 0.0, 0.0, 1.0]);
    assert!(!start.animating);

    // The pointer comes on: nothing has moved yet.
    let entered = ui.layout(&world, (640, 480), &sheets, OVER, None, 0.0);
    assert_eq!(background(&entered).color, [0.0, 0.0, 0.0, 1.0]);
    assert!(entered.animating);
    // Half way.
    let half = ui.layout(&world, (640, 480), &sheets, OVER, None, 0.1);
    let color = background(&half).color;
    assert!((color[0] - 0.5).abs() < 0.01, "{color:?}");
    assert!((background(&half).size[0] - 160.0 * 0.95).abs() < 0.1);
    // Done.
    let done = ui.layout(&world, (640, 480), &sheets, OVER, None, 0.15);
    assert_eq!(background(&done).color, [1.0, 1.0, 1.0, 1.0]);
    assert!((background(&done).size[0] - 144.0).abs() < 1e-3);
    assert!(!done.animating);

    // Leaving eases back from where it is.
    let leaving = ui.layout(&world, (640, 480), &sheets, AWAY, None, 0.05);
    assert_eq!(background(&leaving).color, [1.0, 1.0, 1.0, 1.0]);
    let back = ui.layout(&world, (640, 480), &sheets, AWAY, None, 0.05);
    assert!((background(&back).color[0] - 0.75).abs() < 0.01);

    // Without `transition`, a hover snaps.
    let (world, _, sheets) = play_button(
        "#play { background-color: #000000 } #play:hover { background-color: #ffffff }",
    );
    let mut ui = UiInteraction::default();
    ui.layout(&world, (640, 480), &sheets, AWAY, None, 0.016);
    let snapped = ui.layout(&world, (640, 480), &sheets, OVER, None, 0.0);
    assert_eq!(background(&snapped).color, [1.0, 1.0, 1.0, 1.0]);
}

#[test]
fn a_transition_from_an_element_colour_blends_with_its_background_property() {
    let (world, _, sheets) = play_button(
        "#play { transition: background-color 1s linear } #play:hover { background-color: #ffffff }",
    );
    let mut ui = UiInteraction::default();
    ui.layout(&world, (640, 480), &sheets, AWAY, None, 0.0);
    ui.layout(&world, (640, 480), &sheets, OVER, None, 0.0);
    let half = ui.layout(&world, (640, 480), &sheets, OVER, None, 0.5);
    // BackgroundColor3 is 0.2 grey.
    let color = background(&half).color;
    assert!((color[0] - 0.6).abs() < 0.01, "{color:?}");
}

#[test]
fn mouse_enter_and_leave_fire_once_each_with_the_button_sounds() {
    let (mut world, button, sheets) = play_button("");
    world
        .set_property(
            button,
            "HoverSound",
            Value::String("sounds/tick.wav".into()),
        )
        .unwrap();
    world
        .set_property(
            button,
            "ClickSound",
            Value::String("sounds/click.wav".into()),
        )
        .unwrap();
    let mut ui = UiInteraction::default();
    ui.layout(&world, (640, 480), &sheets, AWAY, None, 0.016);
    assert!(ui.take_events().is_empty());
    for _ in 0..3 {
        ui.layout(&world, (640, 480), &sheets, OVER, None, 0.016);
    }
    assert_eq!(
        ui.take_events(),
        [
            UiEvent::MouseEnter(button),
            UiEvent::PlaySound("sounds/tick.wav".into())
        ]
    );
    assert!(ui.activate(&world, button));
    assert_eq!(
        ui.take_events(),
        [UiEvent::PlaySound("sounds/click.wav".into())]
    );
    ui.layout(&world, (640, 480), &sheets, AWAY, None, 0.016);
    ui.layout(&world, (640, 480), &sheets, UiInput::default(), None, 0.016);
    assert_eq!(ui.take_events(), [UiEvent::MouseLeave(button)]);
}

#[test]
fn a_tooltip_shows_after_a_short_hover_in_its_tooltip_style() {
    let (mut world, button, sheets) =
        play_button("#play Tooltip { background-color: #ff0000; color: #00ff00; font-size: 20px }");
    world
        .set_property(button, "Tooltip", Value::String("Start the game".into()))
        .unwrap();
    let mut ui = UiInteraction::default();
    let texts = |layout: &UiLayout| -> Vec<UiText> {
        layout
            .draw_items
            .iter()
            .filter_map(|item| match item {
                UiDrawItem::Text(text) => Some(text.clone()),
                _ => None,
            })
            .collect()
    };
    let soon = ui.layout(&world, (640, 480), &sheets, OVER, None, 0.016);
    assert_eq!(texts(&soon).len(), 1, "only the button's text");
    assert!(soon.animating, "waiting for the tooltip");
    let later = ui.layout(&world, (640, 480), &sheets, OVER, None, 0.6);
    let shown = texts(&later);
    let tip = shown
        .iter()
        .find(|text| text.text == "Start the game")
        .expect("the tooltip is drawn");
    assert_eq!(tip.font_size, 20.0);
    assert_eq!(tip.color, [0.0, 1.0, 0.0, 1.0]);
    assert!(
        tip.position[0] > 80.0 && tip.position[1] > 24.0,
        "below right of the pointer"
    );
    let UiDrawItem::Rectangle(box_) = later.draw_items[later.draw_items.len() - 2] else {
        panic!("the tooltip's box is under its text");
    };
    assert_eq!(box_.color, [1.0, 0.0, 0.0, 1.0]);
    let away = ui.layout(&world, (640, 480), &sheets, AWAY, None, 0.016);
    assert_eq!(texts(&away).len(), 1, "gone when the pointer leaves");
}

#[test]
fn the_on_top_screen_gui_is_laid_out_last_and_takes_the_pointer_first() {
    let (mut world, button, sheets) = play_button("#play { background-color: #808080 }");
    // A full-screen loading screen, earlier in the tree than the button.
    let loading = world.create(ClassName::ScreenGui).unwrap();
    world.set_parent_at(loading, world.workspace(), 0).unwrap();
    let cover = world.create(ClassName::Frame).unwrap();
    world.set_parent(cover, Some(loading)).unwrap();
    world
        .set_property(cover, "Size", Value::UDim2(UDim2::new(1.0, 0.0, 1.0, 0.0)))
        .unwrap();

    let mut ui = UiInteraction::default();
    let normal = ui.layout(&world, (800, 600), &sheets, OVER, None, 0.0);
    assert_eq!(normal.hit(80.0, 24.0), Some(button));

    let input = UiInput {
        on_top: Some(loading),
        ..OVER
    };
    let covered = ui.layout(&world, (800, 600), &sheets, input, None, 0.0);
    assert_eq!(covered.hit(80.0, 24.0), Some(cover));
    assert_eq!(
        covered.elements.last().map(|element| element.id),
        Some(cover)
    );
}

/// A ScreenGui with one `class` element 100 × 100 at (10, 10), `Id` "box",
/// and its draw items under `css` on an 800 × 600 screen.
fn box_drawn(class: ClassName, css: &str) -> Vec<UiDrawItem> {
    let mut world = World::new();
    let gui = world.create(ClassName::ScreenGui).unwrap();
    world.set_parent(gui, Some(world.workspace())).unwrap();
    let element = world.create(class).unwrap();
    world.set_parent(element, Some(gui)).unwrap();
    for (name, value) in [
        ("Id", Value::String("box".to_owned())),
        ("Position", Value::UDim2(UDim2::new(0.0, 10.0, 0.0, 10.0))),
        ("Size", Value::UDim2(UDim2::new(0.0, 100.0, 0.0, 100.0))),
    ] {
        world.set_property(element, name, value).unwrap();
    }
    let (stylesheet, warnings) = parse_stylesheet("ui.css", css);
    assert!(warnings.is_empty(), "{warnings:?}");
    build_ui_draw_data(&world, 800, 600, &[stylesheet])
}

fn rectangles(items: &[UiDrawItem]) -> Vec<UiRectangle> {
    items
        .iter()
        .filter_map(|item| match item {
            UiDrawItem::Rectangle(rectangle) => Some(*rectangle),
            _ => None,
        })
        .collect()
}

#[test]
fn inset_and_outer_shadows_mix_and_inset_ones_sit_inside_the_border() {
    let items = box_drawn(
        ClassName::Frame,
        "#box { background-color: #808080; border: 4px solid #ffffff; border-radius: 12px; \
         box-shadow: 0 2px 6px 1px rgba(0, 0, 0, 0.5), inset 2px 2px 3px #ffffff, \
         inset -2px -2px 3px #000000 }",
    );
    let drawn = rectangles(&items);
    assert_eq!(drawn.len(), 4);
    // Behind: the outer shadow, its blur and spread kept for the renderer.
    let outer = drawn[0].shadow.unwrap();
    assert!(!outer.inset);
    assert_eq!(
        (outer.offset, outer.blur, outer.spread),
        ([0.0, 2.0], 6.0, 1.0)
    );
    assert_eq!(drawn[0].color[3], 0.5);
    // Then the box with its border.
    assert_eq!(drawn[1].shadow, None);
    assert_eq!(drawn[1].border_width, 4.0);
    // Then the inset shadows inside the border, the first on top, rounded
    // less by the border's width.
    for inset in &drawn[2..] {
        assert!(inset.shadow.unwrap().inset);
        assert_eq!(inset.position, [14.0, 14.0]);
        assert_eq!(inset.size, [92.0, 92.0]);
        assert_eq!(inset.border_radius, 8.0);
    }
    assert_eq!(drawn[2].color[..3], [0.0, 0.0, 0.0]);
    assert_eq!(drawn[3].color[..3], [1.0, 1.0, 1.0]);
    assert_eq!(drawn[3].shadow.unwrap().offset, [2.0, 2.0]);
}

#[test]
fn classic_borders_shade_their_sides_from_the_border_colour() {
    let sides = |css: &str| {
        rectangles(&box_drawn(ClassName::Frame, css))
            .iter()
            .map(|rectangle| rectangle.border_sides)
            .collect::<Vec<_>>()
    };
    let colors = |sides: Option<[UiBorderSide; 4]>| sides.unwrap().map(|side| side.color);
    // Inset: shaded top and left, lit bottom and right; outset the other way.
    let inset = sides("#box { border: 4px inset #808080 }");
    assert_eq!(inset.len(), 1);
    let half = |[r, g, b, a]: [f32; 4]| [r, g, b, a].map(|c| (c * 1000.0).round() / 1000.0);
    let rounded = |sides: Option<[UiBorderSide; 4]>| colors(sides).map(half);
    let expect = |top_left: [f32; 4], bottom_right: [f32; 4]| {
        [top_left, bottom_right, bottom_right, top_left].map(half)
    };
    let mid = 128.0 / 255.0;
    let lit = [(mid + 1.0) * 0.5, (mid + 1.0) * 0.5, (mid + 1.0) * 0.5, 1.0];
    let shaded = [mid * 0.5, mid * 0.5, mid * 0.5, 1.0];
    assert_eq!(rounded(inset[0]), expect(shaded, lit));
    let outset = sides("#box { border: 4px outset #808080 }");
    assert_eq!(rounded(outset[0]), expect(lit, shaded));
    // Ridge and groove: two halves, the inner one the other way round.
    let ridge = sides("#box { border-width: 4px; border-style: ridge; border-color: #808080 }");
    assert_eq!(ridge.len(), 2);
    assert_eq!(rounded(ridge[0]), expect(lit, shaded));
    assert_eq!(rounded(ridge[1]), expect(shaded, lit));
    assert_eq!(ridge[1].unwrap()[0].width, 2.0);
    let groove = sides("#box { border: 4px groove #808080 }");
    assert_eq!(rounded(groove[0]), expect(shaded, lit));
    assert_eq!(rounded(groove[1]), expect(lit, shaded));
    // Each side's own width and colour.
    let own = sides(
        "#box { border: 2px solid #000; border-width: 1px 2px 3px 4px; \
         border-left-color: #ff0000; border-top: 6px solid #00ff00; border-bottom-width: thick }",
    );
    let own = own[0].unwrap();
    assert_eq!(own.map(|side| side.width), [6.0, 2.0, 5.0, 4.0]);
    assert_eq!(own[0].color, [0.0, 1.0, 0.0, 1.0]);
    assert_eq!(own[3].color, [1.0, 0.0, 0.0, 1.0]);
    assert_eq!(own[1].color, [0.0, 0.0, 0.0, 1.0]);
    // `none` draws none: only the background.
    let none = rectangles(&box_drawn(
        ClassName::Frame,
        "#box { border: 4px none #000 }",
    ));
    assert_eq!(none.len(), 1);
    assert_eq!((none[0].border_width, none[0].border_sides), (0.0, None));
}

#[test]
fn border_image_draws_nine_slices_whose_corners_scale_with_the_screen_gui() {
    let slice = |items: &[UiDrawItem]| {
        items.iter().find_map(|item| match item {
            UiDrawItem::Image(image) if image.slice.is_some() => Some(image.clone()),
            _ => None,
        })
    };
    let css = "#box { border-image: url(\"assets/ui/frame.png\") 10 20 fill repeat }";
    let image = slice(&box_drawn(ClassName::Frame, css)).expect("a Frame draws its border image");
    assert_eq!(image.path, "assets/ui/frame.png");
    assert_eq!(image.position, [10.0, 10.0]);
    assert_eq!(image.size, [100.0, 100.0]);
    let cut = image.slice.unwrap();
    assert_eq!(cut.insets, [10.0, 20.0, 10.0, 20.0]);
    assert_eq!(cut.widths, cut.insets);
    assert!(cut.fill && cut.repeat);
    assert!(slice(&box_drawn(ClassName::TextButton, css)).is_some());
    assert!(slice(&box_drawn(ClassName::ImageLabel, css)).is_some());
    assert!(slice(&box_drawn(ClassName::TextLabel, css)).is_none());

    // On a screen twice the design size, the corners are twice as big.
    let mut world = World::new();
    let gui = world.create(ClassName::ScreenGui).unwrap();
    world.set_parent(gui, Some(world.workspace())).unwrap();
    for (name, value) in [
        ("ScaleMode", Value::String("Scale".to_owned())),
        ("DesignWidth", Value::Number(400.0)),
        ("DesignHeight", Value::Number(300.0)),
    ] {
        world.set_property(gui, name, value).unwrap();
    }
    let frame = world.create(ClassName::Frame).unwrap();
    world.set_parent(frame, Some(gui)).unwrap();
    world
        .set_property(frame, "Id", Value::String("box".to_owned()))
        .unwrap();
    let (stylesheet, _) = parse_stylesheet("ui.css", css);
    let image = slice(&build_ui_draw_data(&world, 800, 600, &[stylesheet])).unwrap();
    assert_eq!(image.slice.unwrap().widths, [20.0, 40.0, 20.0, 40.0]);
}

#[test]
fn border_image_values_read_and_write_back() {
    let read = BorderImage::parse("url(frame.png) 12 fill").unwrap();
    assert_eq!(
        read,
        BorderImage {
            path: "frame.png".to_owned(),
            slices: [12.0; 4],
            fill: true,
            repeat: false,
        }
    );
    assert_eq!(read.to_css(), "url(\"frame.png\") 12 fill");
    let uneven = BorderImage {
        slices: [1.0, 2.0, 3.5, 4.0],
        repeat: true,
        fill: false,
        ..read
    };
    assert_eq!(uneven.to_css(), "url(\"frame.png\") 1 2 3.5 4 repeat");
    assert_eq!(BorderImage::parse(&uneven.to_css()), Some(uneven));
    assert_eq!(BorderImage::parse("none"), None);
    assert_eq!(BorderImage::parse("url(a.png) 1 2 3 4 5"), None);
}

#[test]
fn rule_sets_group_each_simple_selector_with_its_states_and_parts() {
    let (sheet, _) = parse_stylesheet(
        "styles.css",
        ".card { background-color: #222; border-radius: 8px; }\n\
         .card:hover { background-color: #333; }\n\
         TextButton { color: white; }\n\
         .card::thumb { width: 8px; }\n\
         .card { background-color: #111; }\n\
         .menu .card, #a { color: black; }\n\
         .empty { }\n",
    );
    let pair = |property: &str, value: &str| (property.to_owned(), value.to_owned());
    assert_eq!(
        rule_sets(&sheet),
        vec![
            RuleSet {
                selector: ".card".into(),
                rules: vec![
                    (
                        String::new(),
                        vec![
                            pair("background-color", "#111"),
                            pair("border-radius", "8px")
                        ]
                    ),
                    (":hover".into(), vec![pair("background-color", "#333")]),
                    ("::thumb".into(), vec![pair("width", "8px")]),
                ],
            },
            RuleSet {
                selector: "TextButton".into(),
                rules: vec![(String::new(), vec![pair("color", "white")])],
            },
        ]
    );
}

#[test]
fn own_rules_are_an_elements_id_rule_and_its_states_and_parts_alone() {
    let (sheet, _) = parse_stylesheet(
        "ui.css",
        "#play { color: red; border-radius: 4px; }\n\
         #play:hover { color: blue; }\n\
         #play::thumb { width: 8px; }\n\
         #play { color: green; }\n\
         #play .label, #play2 { color: black; }\n\
         #player { color: white; }\n",
    );
    let pair = |property: &str, value: &str| (property.to_owned(), value.to_owned());
    assert_eq!(
        own_rules(&[sheet], "play"),
        vec![
            (
                String::new(),
                vec![pair("color", "green"), pair("border-radius", "4px")]
            ),
            (":hover".to_owned(), vec![pair("color", "blue")]),
            ("::thumb".to_owned(), vec![pair("width", "8px")]),
        ]
    );
}
