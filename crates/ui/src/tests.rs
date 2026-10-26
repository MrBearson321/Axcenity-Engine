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
