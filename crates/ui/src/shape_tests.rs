//! `Rotation`, the three transparencies, CSS `aspect-ratio` and
//! `ScreenInsets`.

use super::*;

fn screen_gui(world: &mut World) -> InstanceId {
    let gui = world.create(ClassName::ScreenGui).unwrap();
    world.set_parent(gui, Some(world.workspace())).unwrap();
    gui
}

/// A `class` named `name` in `parent`, at `x`, `y` pixels, `w` × `h` big.
fn element(
    world: &mut World,
    parent: InstanceId,
    class: ClassName,
    name: &str,
    [x, y, w, h]: [f32; 4],
) -> InstanceId {
    let id = world.create(class).unwrap();
    world.set_name(id, name).unwrap();
    world.set_parent(id, Some(parent)).unwrap();
    world
        .set_property(id, "Position", Value::UDim2(UDim2::new(0.0, x, 0.0, y)))
        .unwrap();
    world
        .set_property(id, "Size", Value::UDim2(UDim2::new(0.0, w, 0.0, h)))
        .unwrap();
    id
}

fn css(source: &str) -> Vec<Stylesheet> {
    vec![parse_stylesheet("ui.css", source).0]
}

/// The first rectangle drawn for an element laid out at `rect`, before
/// it's turned: its middle and its turn.
fn drawn_box(layout: &UiLayout, size: [f32; 2]) -> Vec<([f32; 2], f32)> {
    layout
        .draw_items
        .iter()
        .filter_map(|item| match item {
            UiDrawItem::Rectangle(r) if r.size == size => Some((
                [
                    r.position[0] + r.size[0] * 0.5,
                    r.position[1] + r.size[1] * 0.5,
                ],
                r.rotation_degrees,
            )),
            _ => None,
        })
        .collect()
}

fn close(a: [f32; 2], b: [f32; 2]) -> bool {
    (a[0] - b[0]).abs() < 0.01 && (a[1] - b[1]).abs() < 0.01
}

#[test]
fn a_rotated_frame_draws_turned_and_clicks_hit_its_turned_shape() {
    let mut world = World::new();
    let gui = screen_gui(&mut world);
    let frame = element(
        &mut world,
        gui,
        ClassName::Frame,
        "Panel",
        [100.0, 100.0, 200.0, 100.0],
    );
    world
        .set_property(frame, "Rotation", Value::Number(30.0))
        .unwrap();
    let button = element(
        &mut world,
        frame,
        ClassName::TextButton,
        "Ok",
        [0.0, 0.0, 50.0, 20.0],
    );

    let layout = layout_ui(&world, 800, 600, &[]);
    // The frame turns around its own middle, which stays put.
    let frame_box = drawn_box(&layout, [200.0, 100.0]);
    assert_eq!(frame_box.len(), 1);
    assert!(close(frame_box[0].0, [200.0, 150.0]));
    assert_eq!(frame_box[0].1, 30.0);
    // Its child turns with it, around the frame's middle, not its own.
    let (sin, cos) = 30f32.to_radians().sin_cos();
    let on_screen = |[x, y]: [f32; 2]| {
        let [dx, dy] = [x - 200.0, y - 150.0];
        [200.0 + dx * cos - dy * sin, 150.0 + dx * sin + dy * cos]
    };
    let button_box = drawn_box(&layout, [50.0, 20.0]);
    assert!(close(button_box[0].0, on_screen([125.0, 110.0])));
    assert_eq!(button_box[0].1, 30.0);
    // Its text turns too.
    assert!(layout.draw_items.iter().any(|item| matches!(
        item,
        UiDrawItem::Text(text) if text.rotation_degrees == 30.0
    )));
    // Layout doesn't change: the boxes are where they'd be upright.
    assert_eq!(
        layout.element(frame).unwrap().rect,
        UiRect {
            x: 100.0,
            y: 100.0,
            width: 200.0,
            height: 100.0
        }
    );

    // Clicks hit the turned shape: a point the turn brought below the
    // upright box hits the frame, and the upright box's corner it turned
    // away from doesn't.
    let [x, y] = on_screen([295.0, 195.0]);
    assert!(y > 200.0);
    assert_eq!(layout.hit(x, y), Some(frame));
    assert_eq!(layout.hit(295.0, 105.0), None);
    // The button's middle, turned above the frame's upright top edge.
    let [bx, by] = on_screen([125.0, 110.0]);
    assert!(by < 100.0);
    assert_eq!(layout.hit_button(bx, by), Some(button));
    assert_eq!(layout.hover_target(bx, by), Some(button));

    // The same in a scaled ScreenGui (Play at twice the design size) and
    // on the designer's artboard (half size, moved).
    world
        .set_property(gui, "ScaleMode", Value::String("Scale".into()))
        .unwrap();
    world
        .set_property(gui, "DesignWidth", Value::Number(800.0))
        .unwrap();
    world
        .set_property(gui, "DesignHeight", Value::Number(600.0))
        .unwrap();
    let play = layout_ui(&world, 1600, 1200, &[]);
    assert_eq!(play.hit_button(bx * 2.0, by * 2.0), Some(button));
    assert_eq!(play.hit(295.0 * 2.0, 105.0 * 2.0), None);
    let board = layout_artboard(&world, gui, &[], (800, 600), 0.5, [40.0, 30.0]);
    assert_eq!(
        board.hit_button(40.0 + bx * 0.5, 30.0 + by * 0.5),
        Some(button)
    );
    assert_eq!(board.hit(40.0 + x * 0.5, 30.0 + y * 0.5), Some(frame));
    let turned = board.element(button).unwrap();
    assert!((turned.degrees() - 30.0).abs() < 1e-4);
    assert!(close(
        turned.on_screen([40.0 + 125.0 * 0.5, 30.0 + 110.0 * 0.5]),
        [40.0 + bx * 0.5, 30.0 + by * 0.5]
    ));
}

#[test]
fn a_rotated_clipping_frame_cuts_its_children_to_its_turned_box() {
    let mut world = World::new();
    let gui = screen_gui(&mut world);
    let frame = element(
        &mut world,
        gui,
        ClassName::Frame,
        "Window",
        [100.0, 100.0, 100.0, 100.0],
    );
    world
        .set_property(frame, "Id", Value::String("Window".into()))
        .unwrap();
    world
        .set_property(frame, "Rotation", Value::Number(45.0))
        .unwrap();
    let wide = element(
        &mut world,
        frame,
        ClassName::TextButton,
        "Wide",
        [-50.0, 40.0, 200.0, 20.0],
    );
    let layout = layout_ui(&world, 400, 400, &css("#Window { overflow: hidden }"));
    let child = layout.element(wide).unwrap();
    let clip = child.clip.unwrap();
    assert_eq!(clip.rotation_degrees, 45.0);
    // The button's middle is inside; its far end, past the frame's turned
    // edge, is cut away and takes no click.
    assert_eq!(layout.hit_button(150.0, 150.0), Some(wide));
    let end = child.on_screen([-40.0 + 100.0, 150.0]);
    assert_eq!(layout.hit_button(end[0], end[1]), None);
}

#[test]
fn background_text_and_image_transparency_each_change_only_their_own_part() {
    let mut world = World::new();
    let gui = screen_gui(&mut world);
    let button = element(
        &mut world,
        gui,
        ClassName::TextButton,
        "Play",
        [0.0, 0.0, 160.0, 48.0],
    );
    world
        .set_property(button, "Image", Value::String("icon.png".into()))
        .unwrap();
    // Background, image and text alpha.
    let alphas = |world: &World| {
        let layout = layout_ui(world, 400, 300, &[]);
        let mut found = [None; 3];
        for item in &layout.draw_items {
            match item {
                UiDrawItem::Rectangle(r) => found[0] = Some(r.color[3]),
                UiDrawItem::Image(image) => found[1] = Some(image.tint[3]),
                UiDrawItem::Text(text) => found[2] = Some(text.color[3]),
            }
        }
        found.map(|alpha| alpha.unwrap_or(0.0))
    };
    assert_eq!(alphas(&world), [1.0, 1.0, 1.0]);
    for (index, name) in [
        "BackgroundTransparency",
        "ImageTransparency",
        "TextTransparency",
    ]
    .into_iter()
    .enumerate()
    {
        world
            .set_property(button, name, Value::Number(0.75))
            .unwrap();
        let mut expected = [1.0; 3];
        expected[index] = 0.25;
        assert_eq!(alphas(&world), expected, "{name} at 0.75");
        world
            .set_property(button, name, Value::Number(0.0))
            .unwrap();
    }
    // CSS opacity still fades the whole element.
    world
        .set_property(button, "Id", Value::String("Play".into()))
        .unwrap();
    world
        .set_property(button, "TextTransparency", Value::Number(0.5))
        .unwrap();
    let layout = layout_ui(&world, 400, 300, &css("#Play { opacity: 0.5 }"));
    let text = layout.draw_items.iter().find_map(|item| match item {
        UiDrawItem::Text(text) => Some(text.color[3]),
        _ => None,
    });
    assert_eq!(text, Some(0.25));
    // Fully transparent text isn't drawn at all.
    world
        .set_property(button, "TextTransparency", Value::Number(1.0))
        .unwrap();
    assert_eq!(alphas(&world)[2], 0.0);
}

#[test]
fn a_frame_with_aspect_ratio_one_stays_square_whatever_the_screen() {
    let mut world = World::new();
    let gui = screen_gui(&mut world);
    let square = element(&mut world, gui, ClassName::Frame, "Box", [0.0; 4]);
    world
        .set_property(
            square,
            "Size",
            Value::UDim2(UDim2::new(0.25, 0.0, 0.5, 0.0)),
        )
        .unwrap();
    world
        .set_property(square, "Id", Value::String("Box".into()))
        .unwrap();
    let sheets = css("#Box { aspect-ratio: 1 }");
    for (width, height) in [(1920, 1080), (800, 1200), (3440, 1440)] {
        let layout = layout_ui(&world, width, height, &sheets);
        let rect = layout.element(square).unwrap().rect;
        assert_eq!(rect.width, width as f32 * 0.25, "on {width} × {height}");
        assert_eq!(rect.height, rect.width, "square on {width} × {height}");
    }
    // A ratio is width over height; `auto` alone keeps the Size.
    let layout = layout_ui(&world, 1600, 900, &css("#Box { aspect-ratio: 16 / 9 }"));
    let rect = layout.element(square).unwrap().rect;
    assert_eq!((rect.width, rect.height), (400.0, 225.0));
    let layout = layout_ui(&world, 1600, 900, &css("#Box { aspect-ratio: auto }"));
    assert_eq!(layout.element(square).unwrap().rect.height, 450.0);
    assert_eq!(
        aspect_ratio_of(&world, square, &css("#Box { aspect-ratio: auto 4/3 }")),
        Some(4.0 / 3.0)
    );
    assert_eq!(aspect_ratio_of(&world, square, &[]), None);
    let (_, warnings) = parse_stylesheet("ui.css", "#Box { aspect-ratio: 1 }");
    assert!(warnings.is_empty(), "{warnings:?}");
}

#[test]
fn a_top_bar_sits_below_a_phones_notch_with_device_safe_insets() {
    let mut world = World::new();
    let gui = screen_gui(&mut world);
    let bar = element(&mut world, gui, ClassName::Frame, "TopBar", [0.0; 4]);
    world
        .set_property(bar, "Size", Value::UDim2(UDim2::new(1.0, 0.0, 0.0, 60.0)))
        .unwrap();
    let phone = [47.0, 0.0, 34.0, 0.0];
    let on = |world: &World, insets: SafeInsets| {
        let view = ArtboardView {
            screen: (390, 844),
            scale: 1.0,
            origin: [0.0, 0.0],
            shown: None,
            scrolled: None,
            safe_insets: insets,
            overrides: DesignerOverrides::default(),
        };
        layout_artboard_view(world, gui, &[], view)
            .element(bar)
            .unwrap()
            .rect
    };
    // `None` (the default) lays out over the whole screen, notch and all.
    assert_eq!(on(&world, phone).y, 0.0);
    world
        .set_property(
            gui,
            "ScreenInsets",
            Value::String("DeviceSafeInsets".into()),
        )
        .unwrap();
    let rect = on(&world, phone);
    assert_eq!((rect.y, rect.width), (47.0, 390.0));
    // A desktop screen has no insets: the bar is at the top, in the
    // designer and in Play.
    assert_eq!(on(&world, [0.0; 4]).y, 0.0);
    assert_eq!(
        layout_ui(&world, 1920, 1080, &[])
            .element(bar)
            .unwrap()
            .rect
            .y,
        0.0
    );

    // Scaled from its design size, it keeps inside the insets too.
    world
        .set_property(gui, "ScaleMode", Value::String("Scale".into()))
        .unwrap();
    world
        .set_property(gui, "DesignWidth", Value::Number(390.0))
        .unwrap();
    world
        .set_property(gui, "DesignHeight", Value::Number(844.0))
        .unwrap();
    let rect = on(&world, phone);
    assert_eq!(rect.y, 47.0);
    assert!(rect.y + rect.height <= 844.0 - 34.0);
    let (scale, origin, area) = safe_screen_mapping(&world, gui, 390, 844, phone);
    assert_eq!(origin, [0.0, 47.0]);
    assert!((scale - (844.0 - 81.0) / 844.0).abs() < 1e-4);
    assert!((area[1] * scale - (844.0 - 81.0)).abs() < 0.01, "{area:?}");
}

#[test]
fn the_designer_can_hide_elements_pick_pages_and_show_only_an_enabled_screen_gui() {
    let mut world = World::new();
    let gui = screen_gui(&mut world);
    let pages = element(
        &mut world,
        gui,
        ClassName::PageLayout,
        "Pages",
        [0.0, 0.0, 400.0, 300.0],
    );
    let home = element(
        &mut world,
        pages,
        ClassName::Frame,
        "Home",
        [0.0, 0.0, 400.0, 300.0],
    );
    let settings = element(
        &mut world,
        pages,
        ClassName::Frame,
        "Settings",
        [0.0, 0.0, 400.0, 300.0],
    );
    let back = element(
        &mut world,
        settings,
        ClassName::TextButton,
        "Back",
        [10.0, 10.0, 80.0, 30.0],
    );
    let badge = element(
        &mut world,
        gui,
        ClassName::Frame,
        "Badge",
        [500.0, 0.0, 50.0, 50.0],
    );
    let board = |world: &World, overrides: DesignerOverrides| {
        layout_artboard_view(
            world,
            gui,
            &[],
            ArtboardView {
                screen: (800, 600),
                scale: 1.0,
                origin: [0.0, 0.0],
                shown: None,
                scrolled: None,
                safe_insets: [0.0; 4],
                overrides,
            },
        )
    };
    // As the game shows it: the first page, and the badge.
    let game = board(&world, DesignerOverrides::default());
    assert!(game.element(home).is_some() && game.element(back).is_none());
    assert_eq!(shown_page(&world, pages), Some(home));

    // The designer shows Settings, and hides the badge, without changing them.
    let picked = HashMap::from([(pages, settings)]);
    let hidden = HashSet::from([badge]);
    let designing = board(
        &world,
        DesignerOverrides {
            hidden: Some(&hidden),
            pages: Some(&picked),
            only_enabled: false,
        },
    );
    assert!(designing.element(home).is_none());
    assert!(designing.element(back).is_some());
    assert!(designing.element(badge).is_none());
    assert_eq!(shown_page(&world, pages), Some(home));

    // A disabled ScreenGui is still designed, but the Live Preview shows
    // nothing, as Play does.
    world
        .set_property(gui, "Enabled", Value::Bool(false))
        .unwrap();
    assert!(
        board(&world, DesignerOverrides::default())
            .element(home)
            .is_some()
    );
    let preview = board(
        &world,
        DesignerOverrides {
            only_enabled: true,
            ..DesignerOverrides::default()
        },
    );
    assert!(preview.elements.is_empty() && preview.draw_items.is_empty());
}
