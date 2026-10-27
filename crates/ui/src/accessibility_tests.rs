//! The game UI settings (`World::ui_settings`): text scale and higher
//! contrast change what's drawn, reduced motion stops transitions, and
//! navigation reads each control's accessible name.

use axcenity_core::UiSettings;

use super::*;

fn add(world: &mut World, parent: InstanceId, class: ClassName, name: &str) -> InstanceId {
    let id = world.create(class).unwrap();
    world.set_name(id, name).unwrap();
    world.set_parent(id, Some(parent)).unwrap();
    world
        .set_property(id, "Id", Value::String(name.to_owned()))
        .unwrap();
    id
}

fn set(world: &mut World, id: InstanceId, name: &str, value: Value) {
    world.set_property(id, name, value).unwrap();
}

/// A HUD: a half see-through panel holding a light 20-pixel label, a dark
/// label, a label whose text fits its box, and a button with a shadow.
fn hud() -> (World, InstanceId, [InstanceId; 5]) {
    let mut world = World::new();
    let workspace = world.workspace();
    let gui = add(&mut world, workspace, ClassName::ScreenGui, "Hud");
    let panel = add(&mut world, gui, ClassName::Frame, "panel");
    set(
        &mut world,
        panel,
        "BackgroundTransparency",
        Value::Number(0.5),
    );
    set(
        &mut world,
        panel,
        "Size",
        Value::UDim2(UDim2::new(0.0, 400.0, 0.0, 300.0)),
    );
    let label = |world: &mut World, name: &str, y: f32, color: Color3| {
        let id = add(world, panel, ClassName::TextLabel, name);
        set(world, id, "Text", Value::String(name.to_owned()));
        set(world, id, "TextSize", Value::Number(20.0));
        set(world, id, "TextColor3", Value::Color3(color));
        set(world, id, "BackgroundTransparency", Value::Number(1.0));
        set(
            world,
            id,
            "Position",
            Value::UDim2(UDim2::new(0.0, 0.0, 0.0, y)),
        );
        set(
            world,
            id,
            "Size",
            Value::UDim2(UDim2::new(0.0, 300.0, 0.0, 40.0)),
        );
        id
    };
    let light = label(&mut world, "light", 0.0, Color3::new(0.7, 0.75, 0.8));
    let dark = label(&mut world, "dark", 50.0, Color3::new(0.3, 0.2, 0.25));
    let fits = label(&mut world, "fits", 100.0, Color3::new(1.0, 1.0, 1.0));
    set(&mut world, fits, "TextScaled", Value::Bool(true));
    let button = add(&mut world, panel, ClassName::TextButton, "play");
    set(&mut world, button, "Text", Value::String("Play".into()));
    set(
        &mut world,
        button,
        "Position",
        Value::UDim2(UDim2::new(0.0, 0.0, 0.0, 150.0)),
    );
    set(
        &mut world,
        button,
        "Size",
        Value::UDim2(UDim2::new(0.0, 120.0, 0.0, 40.0)),
    );
    (world, gui, [panel, light, dark, fits, button])
}

fn sheets() -> Vec<Stylesheet> {
    let (sheet, warnings) = parse_stylesheet(
        "ui.css",
        "#play { box-shadow: 0 4px 8px rgba(0, 0, 0, 0.5) }",
    );
    assert!(warnings.is_empty(), "{warnings:?}");
    vec![sheet]
}

fn text<'a>(layout: &'a UiLayout, shown: &str) -> &'a UiText {
    layout
        .draw_items
        .iter()
        .find_map(|item| match item {
            UiDrawItem::Text(text) if text.text == shown => Some(text),
            _ => None,
        })
        .unwrap_or_else(|| panic!("{shown} is drawn"))
}

fn rectangles(layout: &UiLayout) -> Vec<UiRectangle> {
    layout
        .draw_items
        .iter()
        .filter_map(|item| match item {
            UiDrawItem::Rectangle(rectangle) => Some(*rectangle),
            _ => None,
        })
        .collect()
}

fn with(world: &mut World, change: impl FnOnce(&mut UiSettings)) {
    let mut settings = world.ui_settings();
    change(&mut settings);
    world.set_ui_settings(settings);
}

#[test]
fn raising_text_scale_draws_text_bigger_in_the_same_boxes() {
    let (mut world, gui, [_, light, _, fits, _]) = hud();
    let sheets = sheets();
    let before = layout_ui(&world, 1280, 720, &sheets);
    assert_eq!(text(&before, "light").font_size, 20.0);
    with(&mut world, |settings| settings.text_scale = 1.5);
    let after = layout_ui(&world, 1280, 720, &sheets);
    assert_eq!(text(&after, "light").font_size, 30.0);
    assert_eq!(after.element(light).unwrap().font_size, 30.0);
    // Text that fits its box keeps fitting it.
    assert_eq!(
        text(&after, "fits").font_size,
        text(&before, "fits").font_size
    );
    assert!(text(&after, "fits").scaled);
    // Boxes, and so clicks, stay where they were.
    for (old, new) in before.elements.iter().zip(&after.elements) {
        assert_eq!(old.rect, new.rect);
    }
    assert_eq!(text(&after, "light").size, text(&before, "light").size);
    assert!(before.element(fits).is_some());

    // The UI Designer's artboard shows it too.
    let artboard = layout_artboard(&world, gui, &sheets, (1280, 720), 0.5, [0.0, 0.0]);
    assert_eq!(text(&artboard, "light").font_size, 15.0);
}

#[test]
fn higher_contrast_draws_text_black_or_white_and_see_through_backgrounds_solid() {
    let (mut world, _, _) = hud();
    let sheets = sheets();
    let before = layout_ui(&world, 1280, 720, &sheets);
    let panel_fill = |layout: &UiLayout| {
        rectangles(layout)
            .into_iter()
            .find(|r| r.shadow.is_none() && r.size == [400.0, 300.0])
            .unwrap()
            .color
    };
    assert_eq!(panel_fill(&before)[3], 0.5);
    let shadow = |layout: &UiLayout| {
        rectangles(layout)
            .into_iter()
            .find(|r| r.shadow.is_some())
            .unwrap()
            .color
    };
    with(&mut world, |settings| settings.high_contrast = true);
    let after = layout_ui(&world, 1280, 720, &sheets);
    assert_eq!(text(&after, "light").color, [1.0, 1.0, 1.0, 1.0]);
    assert_eq!(text(&after, "dark").color, [0.0, 0.0, 0.0, 1.0]);
    assert_eq!(panel_fill(&after)[3], 1.0);
    assert_eq!(panel_fill(&after)[..3], panel_fill(&before)[..3]);
    // A shadow stays soft, and a clear background stays clear.
    assert_eq!(shadow(&after), shadow(&before));
    assert_eq!(rectangles(&after).len(), rectangles(&before).len());
    assert!(
        rectangles(&after)
            .iter()
            .filter(|r| r.size == [300.0, 40.0])
            .all(|r| r.color[3] == 0.0)
    );
    // Nothing moves.
    for (old, new) in before.elements.iter().zip(&after.elements) {
        assert_eq!(old.rect, new.rect);
    }
}

#[test]
fn reduced_motion_shows_a_new_style_at_once_instead_of_easing() {
    let (mut world, _, [.., button]) = hud();
    let (sheet, _) = parse_stylesheet(
        "ui.css",
        "#play { background-color: #000000; transition: background-color 1s linear } \
         #play:hover { background-color: #ffffff }",
    );
    let sheets = vec![sheet];
    let over = |world: &World| {
        let element = layout_ui(world, 1280, 720, &sheets);
        let rect = element.element(button).unwrap().rect;
        UiInput {
            pointer: Some([rect.x + 10.0, rect.y + 10.0]),
            ..UiInput::default()
        }
    };
    let fill = |layout: &UiLayout| {
        rectangles(layout)
            .into_iter()
            .find(|r| r.size == [120.0, 40.0])
            .unwrap()
            .color
    };
    let pointer = over(&world);
    let mut ui = UiInteraction::default();
    ui.layout(
        &world,
        (1280, 720),
        &sheets,
        UiInput::default(),
        None,
        0.016,
    );
    ui.layout(&world, (1280, 720), &sheets, pointer, None, 0.0);
    let easing = ui.layout(&world, (1280, 720), &sheets, pointer, None, 0.5);
    assert!((fill(&easing)[0] - 0.5).abs() < 0.01, "{:?}", fill(&easing));

    // Turned on halfway, it ends there and then; the next change snaps.
    with(&mut world, |settings| settings.reduced_motion = true);
    let now = ui.layout(&world, (1280, 720), &sheets, pointer, None, 0.016);
    assert_eq!(fill(&now), [1.0, 1.0, 1.0, 1.0]);
    assert!(!now.animating);
    let away = ui.layout(
        &world,
        (1280, 720),
        &sheets,
        UiInput::default(),
        None,
        0.016,
    );
    assert_eq!(fill(&away), [0.0, 0.0, 0.0, 1.0]);
    assert!(!away.animating);
}

#[test]
fn navigation_reads_each_controls_accessible_name_in_the_tab_order() {
    let mut world = World::new();
    let workspace = world.workspace();
    let gui = add(&mut world, workspace, ClassName::ScreenGui, "Menu");
    let button = |world: &mut World, name: &str, y: f32| {
        let id = add(world, gui, ClassName::TextButton, name);
        set(
            world,
            id,
            "Position",
            Value::UDim2(UDim2::new(0.0, 0.0, 0.0, y)),
        );
        set(
            world,
            id,
            "Size",
            Value::UDim2(UDim2::new(0.0, 200.0, 0.0, 40.0)),
        );
        id
    };
    let play = button(&mut world, "PlayButton", 0.0);
    let options = button(&mut world, "OptionsButton", 50.0);
    let quit = button(&mut world, "QuitButton", 100.0);
    set(
        &mut world,
        options,
        "AccessibleName",
        Value::String("Settings".into()),
    );
    let sheets = Vec::new();
    let mut ui = UiInteraction::default();
    let frame = |ui: &mut UiInteraction, world: &World| {
        ui.layout(world, (1280, 720), &sheets, UiInput::default(), None, 0.016);
    };
    frame(&mut ui, &world);
    assert_eq!(ui.selected_name(&world), None);
    let mut heard = Vec::new();
    let mut order = Vec::new();
    for _ in 0..4 {
        ui.navigate(&world, NavKey::Next).apply(&mut world);
        frame(&mut ui, &world);
        order.push(ui.selected().unwrap());
        heard.push(ui.selected_name(&world).unwrap().to_owned());
    }
    assert_eq!(order, [play, options, quit, play], "the Phase 19 order");
    assert_eq!(
        heard,
        ["PlayButton", "Settings", "QuitButton", "PlayButton"]
    );

    // The settings don't change the order.
    with(&mut world, |settings| {
        *settings = UiSettings {
            text_scale: 2.0,
            high_contrast: true,
            reduced_motion: true,
        }
    });
    ui.navigate(&world, NavKey::Next).apply(&mut world);
    frame(&mut ui, &world);
    assert_eq!(ui.selected(), Some(options));
}
