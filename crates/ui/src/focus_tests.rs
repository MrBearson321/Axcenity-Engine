//! Keyboard and controller navigation: focus, `:focus`, `NextSelection*`,
//! and the widgets, text boxes and ScrollingFrames under focus.

// `press` applies what each key did; most steps only look at focus after.
#![allow(unused_must_use)]

use super::*;

fn screen_gui(world: &mut World) -> InstanceId {
    let gui = world.create(ClassName::ScreenGui).unwrap();
    world.set_parent(gui, Some(world.workspace())).unwrap();
    gui
}

/// A `class` named and with `Id` `name`, at `x`, `y` pixels, `w` × `h` big.
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
    set(world, id, "Id", Value::String(name.to_owned()));
    set(
        world,
        id,
        "Position",
        Value::UDim2(UDim2::new(0.0, x, 0.0, y)),
    );
    set(world, id, "Size", Value::UDim2(UDim2::new(0.0, w, 0.0, h)));
    id
}

fn set(world: &mut World, id: InstanceId, name: &str, value: Value) {
    world.set_property(id, name, value).unwrap();
}

fn css(source: &str) -> Vec<Stylesheet> {
    let (sheet, warnings) = parse_stylesheet("ui.css", source);
    assert!(warnings.is_empty(), "{warnings:?}");
    vec![sheet]
}

fn frame(ui: &mut UiInteraction, world: &World, sheets: &[Stylesheet]) -> UiLayout {
    ui.layout(
        world,
        (1280, 720),
        sheets,
        UiInput::default(),
        None,
        1.0 / 60.0,
    )
}

/// Presses `key` and lays the UI out again, as a host does between frames.
fn press(
    ui: &mut UiInteraction,
    world: &mut World,
    sheets: &[Stylesheet],
    key: NavKey,
) -> UiNavigated {
    let done = ui.navigate(world, key);
    done.apply(world);
    frame(ui, world, sheets);
    done
}

fn rings(layout: &UiLayout) -> Vec<UiRectangle> {
    layout
        .draw_items
        .iter()
        .filter_map(|item| match item {
            UiDrawItem::Rectangle(r) if r.border_color == FOCUS_RING_COLOR => Some(*r),
            _ => None,
        })
        .collect()
}

/// A title, a name box, three buttons and a volume slider, in a column.
struct Menu {
    world: World,
    title: InstanceId,
    name: InstanceId,
    play: InstanceId,
    options: InstanceId,
    volume: InstanceId,
    quit: InstanceId,
}

fn menu() -> Menu {
    let mut world = World::new();
    let gui = screen_gui(&mut world);
    let x = 440.0;
    let title = element(
        &mut world,
        gui,
        ClassName::TextLabel,
        "title",
        [x, 40.0, 400.0, 40.0],
    );
    let name = element(
        &mut world,
        gui,
        ClassName::TextBox,
        "name",
        [x, 100.0, 400.0, 40.0],
    );
    let play = element(
        &mut world,
        gui,
        ClassName::TextButton,
        "play",
        [x, 160.0, 400.0, 48.0],
    );
    let options = element(
        &mut world,
        gui,
        ClassName::TextButton,
        "options",
        [x, 220.0, 400.0, 48.0],
    );
    let volume = element(
        &mut world,
        gui,
        ClassName::Slider,
        "volume",
        [x, 288.0, 400.0, 24.0],
    );
    set(&mut world, volume, "Step", Value::Number(5.0));
    let quit = element(
        &mut world,
        gui,
        ClassName::TextButton,
        "quit",
        [x, 330.0, 400.0, 48.0],
    );
    Menu {
        world,
        title,
        name,
        play,
        options,
        volume,
        quit,
    }
}

#[test]
fn arrows_and_tab_move_focus_through_a_menu_in_the_same_order_and_confirm_clicks() {
    let Menu {
        mut world,
        title,
        name,
        play,
        options,
        volume,
        quit,
    } = menu();
    let sheets = Vec::new();
    let column = [name, play, options, volume, quit];

    // Down from nothing starts at the top, then goes down the column; the
    // label isn't selectable.
    let mut ui = UiInteraction::default();
    frame(&mut ui, &world, &sheets);
    let mut by_arrows = Vec::new();
    for _ in 0..column.len() {
        assert!(press(&mut ui, &mut world, &sheets, NavKey::Down).taken);
        by_arrows.extend(ui.selected());
    }
    assert_eq!(by_arrows, column);
    assert!(!by_arrows.contains(&title));
    // Nothing further down: focus stays.
    assert!(press(&mut ui, &mut world, &sheets, NavKey::Down).taken);
    assert_eq!(ui.selected(), Some(quit));
    // And back up past the slider (up and down don't move a horizontal one).
    press(&mut ui, &mut world, &sheets, NavKey::Up);
    press(&mut ui, &mut world, &sheets, NavKey::Up);
    assert_eq!(ui.selected(), Some(options));

    // Tab goes the same way, in the tree's order, and wraps round.
    let mut ui = UiInteraction::default();
    frame(&mut ui, &world, &sheets);
    let mut by_tab = Vec::new();
    for _ in 0..column.len() {
        press(&mut ui, &mut world, &sheets, NavKey::Next);
        by_tab.extend(ui.selected());
    }
    assert_eq!(by_tab, column);
    press(&mut ui, &mut world, &sheets, NavKey::Next);
    assert_eq!(ui.selected(), Some(name));
    press(&mut ui, &mut world, &sheets, NavKey::Previous);
    assert_eq!(ui.selected(), Some(quit));

    // Confirm on a button is a click: it fires Activated.
    ui.select(Some(play));
    frame(&mut ui, &world, &sheets);
    let done = press(&mut ui, &mut world, &sheets, NavKey::Confirm);
    assert_eq!(done.activated, Some(play));
    // Back drops focus.
    press(&mut ui, &mut world, &sheets, NavKey::Back);
    assert_eq!(ui.selected(), None);
}

#[test]
fn the_focused_button_shows_its_focus_rule_and_others_get_the_default_ring() {
    let Menu {
        mut world,
        play,
        options,
        ..
    } = menu();
    let sheets =
        css("#play:focus { background-color: #ff0000 } #play { background-color: #222222 }");
    let mut ui = UiInteraction::default();
    let before = frame(&mut ui, &world, &sheets);
    assert!(rings(&before).is_empty());

    ui.select(Some(play));
    let focused = frame(&mut ui, &world, &sheets);
    let element = *focused.element(play).unwrap();
    let red = focused.draw_items.iter().any(|item| {
        matches!(item, UiDrawItem::Rectangle(r)
            if r.position == [element.rect.x, element.rect.y] && r.color[..3] == [1.0, 0.0, 0.0])
    });
    assert!(red, "#play:focus paints it red");
    assert!(
        rings(&focused).is_empty(),
        "a :focus rule replaces the ring"
    );

    // Options has no :focus rule, so it gets the ring, just outside it.
    press(&mut ui, &mut world, &sheets, NavKey::Down);
    assert_eq!(ui.selected(), Some(options));
    let layout = frame(&mut ui, &world, &sheets);
    let ring = rings(&layout);
    assert_eq!(ring.len(), 1);
    let rect = layout.element(options).unwrap().rect;
    assert!(ring[0].position[0] < rect.x && ring[0].position[1] < rect.y);
    assert!(ring[0].size[0] > rect.width && ring[0].border_width > 0.0);
}

#[test]
fn next_selection_down_is_followed_and_falls_back_when_its_element_is_hidden() {
    let Menu {
        mut world,
        name,
        play,
        quit,
        ..
    } = menu();
    let sheets = Vec::new();
    set(
        &mut world,
        quit,
        "NextSelectionDown",
        Value::String("name".into()),
    );
    set(
        &mut world,
        name,
        "NextSelectionDown",
        Value::String("quit".into()),
    );
    let mut ui = UiInteraction::default();
    ui.select(Some(quit));
    frame(&mut ui, &world, &sheets);
    press(&mut ui, &mut world, &sheets, NavKey::Down);
    assert_eq!(
        ui.selected(),
        Some(name),
        "Quit's override wraps to the top"
    );

    // Quit hidden: Name's override is skipped for the nearest one down.
    set(&mut world, quit, "Visible", Value::Bool(false));
    frame(&mut ui, &world, &sheets);
    press(&mut ui, &mut world, &sheets, NavKey::Down);
    assert_eq!(ui.selected(), Some(play));
}

#[test]
fn a_focused_slider_moves_with_left_and_right_in_steps() {
    let Menu {
        mut world, volume, ..
    } = menu();
    let sheets = Vec::new();
    let mut ui = UiInteraction::default();
    ui.select(Some(volume));
    frame(&mut ui, &world, &sheets);
    let value = |world: &World| world.get(volume).unwrap().property("Value").cloned();

    // A twentieth of 0 to 100, in whole steps of 5.
    let done = press(&mut ui, &mut world, &sheets, NavKey::Right);
    assert_eq!(done.value, Some((volume, Value::Number(55.0))));
    assert_eq!(value(&world), Some(Value::Number(55.0)));
    press(&mut ui, &mut world, &sheets, NavKey::Left);
    press(&mut ui, &mut world, &sheets, NavKey::Left);
    assert_eq!(value(&world), Some(Value::Number(45.0)));
    assert_eq!(
        ui.selected(),
        Some(volume),
        "left and right don't move focus"
    );

    // At the end it stays there.
    set(&mut world, volume, "Value", Value::Number(100.0));
    let done = press(&mut ui, &mut world, &sheets, NavKey::Right);
    assert!(done.taken && done.value.is_none());

    // A vertical slider moves with up and down instead.
    set(
        &mut world,
        volume,
        "Orientation",
        Value::String("Vertical".into()),
    );
    press(&mut ui, &mut world, &sheets, NavKey::Down);
    assert_eq!(value(&world), Some(Value::Number(95.0)));
}

#[test]
fn unselectable_and_turned_off_elements_are_skipped() {
    let Menu {
        mut world,
        name,
        play,
        options,
        ..
    } = menu();
    let sheets = Vec::new();
    set(&mut world, play, "Selectable", Value::Bool(false));
    set(&mut world, options, "Interactable", Value::Bool(false));
    let mut ui = UiInteraction::default();
    ui.select(Some(name));
    frame(&mut ui, &world, &sheets);
    press(&mut ui, &mut world, &sheets, NavKey::Down);
    let volume = world.find_first_descendant(world.workspace(), "volume");
    assert_eq!(ui.selected(), volume);

    // Focus goes when its element is turned off.
    set(
        &mut world,
        volume.unwrap(),
        "Interactable",
        Value::Bool(false),
    );
    frame(&mut ui, &world, &sheets);
    assert_eq!(ui.selected(), None);
}

#[test]
fn confirm_flips_a_toggle_and_opens_a_dropdown_whose_list_up_and_down_choose_in() {
    let mut world = World::new();
    let gui = screen_gui(&mut world);
    let toggle = element(
        &mut world,
        gui,
        ClassName::Toggle,
        "t",
        [100.0, 100.0, 52.0, 28.0],
    );
    let dropdown = element(
        &mut world,
        gui,
        ClassName::Dropdown,
        "d",
        [100.0, 160.0, 200.0, 36.0],
    );
    let sheets = Vec::new();
    let mut ui = UiInteraction::default();
    ui.select(Some(toggle));
    frame(&mut ui, &world, &sheets);

    let done = press(&mut ui, &mut world, &sheets, NavKey::Confirm);
    assert_eq!(done.value, Some((toggle, Value::Bool(true))));

    press(&mut ui, &mut world, &sheets, NavKey::Down);
    assert_eq!(ui.selected(), Some(dropdown));
    press(&mut ui, &mut world, &sheets, NavKey::Confirm);
    assert_eq!(ui.open_dropdown(), Some(dropdown));
    // The list starts on the first option (nothing is chosen yet); down
    // moves to Medium, and the lit row follows.
    let lit = |layout: &UiLayout| {
        layout
            .draw_items
            .iter()
            .filter(
                |item| matches!(item, UiDrawItem::Rectangle(r) if (r.color[3] - 0.08).abs() < 1e-3),
            )
            .count()
    };
    assert_eq!(lit(&frame(&mut ui, &world, &sheets)), 1);
    press(&mut ui, &mut world, &sheets, NavKey::Down);
    press(&mut ui, &mut world, &sheets, NavKey::Down);
    press(&mut ui, &mut world, &sheets, NavKey::Up);
    let done = press(&mut ui, &mut world, &sheets, NavKey::Confirm);
    assert_eq!(done.value, Some((dropdown, Value::String("Medium".into()))));
    assert_eq!(ui.open_dropdown(), None);
    assert_eq!(ui.selected(), Some(dropdown), "focus stays on the dropdown");

    // Back closes the list without choosing.
    press(&mut ui, &mut world, &sheets, NavKey::Confirm);
    press(&mut ui, &mut world, &sheets, NavKey::Down);
    let done = press(&mut ui, &mut world, &sheets, NavKey::Back);
    assert!(done.taken && done.value.is_none());
    assert_eq!(ui.open_dropdown(), None);
    assert_eq!(ui.selected(), Some(dropdown));
}

#[test]
fn confirm_on_a_text_box_starts_typing_in_it() {
    let Menu {
        mut world, name, ..
    } = menu();
    let sheets = Vec::new();
    let mut ui = UiInteraction::default();
    ui.select(Some(name));
    frame(&mut ui, &world, &sheets);
    let done = press(&mut ui, &mut world, &sheets, NavKey::Confirm);
    assert_eq!(done.text_box, Some(name));
    assert_eq!(done.activated, None);
}

#[test]
fn a_scrolling_frame_scrolls_to_keep_the_focused_child_in_view() {
    let mut world = World::new();
    let gui = screen_gui(&mut world);
    let list = element(
        &mut world,
        gui,
        ClassName::ScrollingFrame,
        "list",
        [100.0, 100.0, 300.0, 200.0],
    );
    set(
        &mut world,
        list,
        "CanvasSize",
        Value::UDim2(UDim2::new(0.0, 0.0, 0.0, 1000.0)),
    );
    let buttons: Vec<_> = (0..20)
        .map(|i| {
            let name = format!("b{i}");
            let y = i as f32 * 50.0;
            element(
                &mut world,
                list,
                ClassName::TextButton,
                &name,
                [0.0, y, 280.0, 44.0],
            )
        })
        .collect();
    let sheets = Vec::new();
    let mut ui = UiInteraction::default();
    frame(&mut ui, &world, &sheets);
    let mut scrolled = false;
    for _ in 0..6 {
        scrolled |= press(&mut ui, &mut world, &sheets, NavKey::Down)
            .moved
            .is_some();
    }
    assert_eq!(ui.selected(), Some(buttons[5]));
    assert!(scrolled);
    let layout = frame(&mut ui, &world, &sheets);
    let shown = layout.element(list).unwrap().rect;
    let focused = layout.element(buttons[5]).unwrap().rect;
    assert!(focused.y >= shown.y - 0.5);
    assert!(
        focused.y + focused.height <= shown.y + shown.height + 0.5,
        "{focused:?} in {shown:?}"
    );

    // Back up to the top scrolls back.
    for _ in 0..5 {
        press(&mut ui, &mut world, &sheets, NavKey::Up);
    }
    let canvas = world.get(list).unwrap().property("CanvasPosition").cloned();
    assert_eq!(
        canvas,
        Some(Value::Vector2(axcenity_core::Vector2::new(0.0, 0.0)))
    );
}

#[test]
fn a_hud_that_isnt_navigated_draws_and_clicks_as_before() {
    let Menu {
        world, play, quit, ..
    } = menu();
    let sheets = css("#play { background-color: #335577; border-radius: 8px }");
    let mut ui = UiInteraction::default();
    let interactive = frame(&mut ui, &world, &sheets);
    let plain = layout_ui(&world, 1280, 720, &sheets);
    assert_eq!(
        format!("{:?}", interactive.draw_items),
        format!("{:?}", plain.draw_items)
    );
    assert!(rings(&interactive).is_empty());

    // Scaled from its design size onto a bigger screen, the same too.
    let mut world = world;
    let gui = world.get(play).unwrap().parent().unwrap();
    set(&mut world, gui, "ScaleMode", Value::String("Scale".into()));
    set(&mut world, gui, "DesignWidth", Value::Number(1280.0));
    set(&mut world, gui, "DesignHeight", Value::Number(720.0));
    let mut scaled_ui = UiInteraction::default();
    let input = UiInput::default();
    let scaled = scaled_ui.layout(&world, (1920, 1080), &sheets, input, None, 0.0);
    let plain_scaled = layout_ui(&world, 1920, 1080, &sheets);
    assert_eq!(
        format!("{:?}", scaled.draw_items),
        format!("{:?}", plain_scaled.draw_items)
    );
    let big = scaled.element(play).unwrap().rect;
    let small = interactive.element(play).unwrap().rect;
    assert!(
        (big.width - small.width * 1.5).abs() < 0.01,
        "{big:?} {small:?}"
    );
    let world = world;

    // The game's keys aren't taken until something is focused, and Tab
    // is taken only when there's something to focus.
    for key in [NavKey::Up, NavKey::Down, NavKey::Confirm, NavKey::Back] {
        assert!(!ui.takes(&world, key), "{key:?}");
    }
    assert!(ui.takes(&world, NavKey::Next));
    let mut hud = World::new();
    let gui = screen_gui(&mut hud);
    element(
        &mut hud,
        gui,
        ClassName::TextLabel,
        "score",
        [0.0, 0.0, 100.0, 30.0],
    );
    let mut hud_ui = UiInteraction::default();
    frame(&mut hud_ui, &hud, &sheets);
    assert!(!hud_ui.takes(&hud, NavKey::Next));
    assert!(!hud_ui.navigate(&hud, NavKey::Next).taken);

    // A click still finds the button, and the mouse drops focus.
    let rect = interactive.element(quit).unwrap().rect;
    let at = [rect.x + 5.0, rect.y + 5.0];
    ui.select(Some(play));
    frame(&mut ui, &world, &sheets);
    let _ = ui.press(&world, at);
    assert_eq!(ui.selected(), None);
    let layout = frame(&mut ui, &world, &sheets);
    assert_eq!(layout.hit_button(at[0], at[1]), Some(quit));
    assert!(ui.activate(&world, quit));
}

#[test]
fn a_text_box_with_a_focus_rule_has_no_typing_ring() {
    let Menu { world, name, .. } = menu();
    let typing = |sheets: &[Stylesheet]| {
        layout_ui_viewed(&world, 1280, 720, sheets, Some(name), None)
            .draw_items
            .iter()
            .filter(|item| matches!(item, UiDrawItem::Rectangle(r) if r.border_width >= 2.0 || r.border_sides.is_some()))
            .count()
    };
    assert_eq!(typing(&[]), 1, "the default ring in its text colour");
    let styled = css("TextBox:focus { background-color: #ffffcc }");
    assert_eq!(typing(&styled), 0);
}
