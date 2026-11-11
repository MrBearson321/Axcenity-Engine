use std::path::Path;

use super::*;

fn demo_world() -> World {
    let folder = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/hello-touched");
    axcenity_project::load(&folder).unwrap().world
}

#[test]
fn ctrl_toggles_and_shift_selects_the_rows_between() {
    let mut world = World::new();
    let a = world.create(ClassName::Part).unwrap();
    let b = world.create(ClassName::Part).unwrap();
    let c = world.create(ClassName::Part).unwrap();
    let order = [a, b, c];
    let mut selection = vec![a];
    update_selection(
        &mut selection,
        Some(c),
        egui::Modifiers {
            shift: true,
            ..Default::default()
        },
        Some(&order),
        false,
    );
    assert_eq!(selection, vec![a, b, c]);
    update_selection(
        &mut selection,
        Some(b),
        egui::Modifiers {
            ctrl: true,
            ..Default::default()
        },
        Some(&order),
        false,
    );
    assert_eq!(selection, vec![a, c]);
    update_selection(
        &mut selection,
        None,
        egui::Modifiers::default(),
        None,
        false,
    );
    assert!(selection.is_empty());
}

/// Each visible row as indentation plus name, for easy comparing.
fn outline(explorer: &Explorer, world: &World) -> Vec<String> {
    explorer
        .rows(world)
        .iter()
        .map(|row| format!("{}{}", "  ".repeat(row.depth), row.name))
        .collect()
}

#[test]
fn explorer_starts_with_game_and_workspace_open() {
    let world = demo_world();
    let explorer = Explorer::new(&world);
    let rows = explorer.rows(&world);

    assert_eq!(rows[0].name, "Game");
    assert_eq!(rows[0].class, ClassName::DataModel);
    assert!(rows[0].expanded);
    assert_eq!(rows[1].name, "Workspace");
    assert_eq!(rows[1].depth, 1);
    assert!(rows[1].expanded);
    // Every part in the Workspace is listed right under it.
    let workspace = world.get(world.workspace()).unwrap();
    for (i, &child) in workspace.children().iter().enumerate() {
        assert_eq!(rows[2 + i].id, child);
        assert_eq!(rows[2 + i].depth, 2);
    }
    // The other services follow, closed, so the Script inside
    // ServerScriptService isn't shown.
    let services: Vec<&str> = rows[2 + workspace.children().len()..]
        .iter()
        .map(|row| row.name.as_str())
        .collect();
    assert_eq!(
        services,
        [
            "Players",
            "Lighting",
            "ReplicatedStorage",
            "ServerScriptService",
            "ServerStorage",
            "StarterPlayer",
            "Teams",
            "SoundService",
            "TextChatService"
        ]
    );
    let scripts = &rows[rows.len() - 6];
    assert!(scripts.has_children && !scripts.expanded);
}

#[test]
fn a_closed_branch_shows_the_warning_badge_of_a_script_inside() {
    let world = demo_world();
    let scripts = world.server_script_service();
    let pad = world.find_first_child(scripts, "pad").unwrap();
    let mut explorer = Explorer::new(&world);
    explorer
        .warnings
        .insert(pad, "Line 1: a warning".to_string());
    let badge = |explorer: &Explorer, id| {
        let rows = explorer.rows(&world);
        let row = rows.iter().find(|row| row.id == id)?;
        warning_badge(&explorer.warnings, &world, row)
    };

    let closed = badge(&explorer, scripts).unwrap();
    assert!(closed.contains("warnings of pad"), "{closed}");
    assert_eq!(badge(&explorer, world.workspace()), None);
    // Opened, the script shows its own badge, and the branch none.
    explorer.toggle(scripts);
    assert_eq!(badge(&explorer, scripts), None);
    assert_eq!(badge(&explorer, pad).unwrap(), "Line 1: a warning");
}

#[test]
fn branches_expand_and_collapse() {
    let world = demo_world();
    let mut explorer = Explorer::new(&world);
    let before = outline(&explorer, &world);

    explorer.toggle(world.server_script_service());
    let opened = outline(&explorer, &world);
    assert_eq!(opened.len(), before.len() + 1);
    assert_eq!(opened[opened.len() - 6], "    pad");

    explorer.toggle(world.workspace());
    let parts = world.get(world.workspace()).unwrap().children().len();
    assert_eq!(outline(&explorer, &world).len(), opened.len() - parts);

    explorer.toggle(world.workspace());
    explorer.toggle(world.server_script_service());
    assert_eq!(outline(&explorer, &world), before);
}

#[test]
fn closing_a_branch_hides_everything_inside_it_and_reopening_restores_it() {
    // game → Workspace → Car → Wheel, three levels deep. (Model doesn't
    // exist yet, so a Part holds the other Part.)
    let mut world = World::new();
    let car = world.create(ClassName::Part).unwrap();
    world.set_name(car, "Car").unwrap();
    world.set_parent(car, Some(world.workspace())).unwrap();
    let wheel = world.create(ClassName::Part).unwrap();
    world.set_name(wheel, "Wheel").unwrap();
    world.set_parent(wheel, Some(car)).unwrap();
    let mut explorer = Explorer::new(&world);
    explorer.toggle(car);
    let open = [
        "Game",
        "  Workspace",
        "    Car",
        "      Wheel",
        "  Players",
        "  Lighting",
        "  ReplicatedStorage",
        "  ServerScriptService",
        "  ServerStorage",
        "  StarterPlayer",
        "  Teams",
        "  SoundService",
        "  TextChatService",
    ];
    assert_eq!(outline(&explorer, &world), open);

    explorer.toggle(world.root());
    assert_eq!(outline(&explorer, &world), ["Game"]);

    // The Model inside stays open, so it all comes back as it was.
    explorer.toggle(world.root());
    assert_eq!(outline(&explorer, &world), open);
}

/// Runs the Explorer for one egui frame in an 800×600 window, with these
/// input events. No window or GPU needed.
fn frame(
    ctx: &egui::Context,
    explorer: &mut Explorer,
    world: &World,
    events: Vec<egui::Event>,
) -> ExplorerOutput {
    frame_while(ctx, explorer, world, events, true)
}

/// `frame`, with the Explorer `editable` or not (during Play).
fn frame_while(
    ctx: &egui::Context,
    explorer: &mut Explorer,
    world: &World,
    events: Vec<egui::Event>,
    editable: bool,
) -> ExplorerOutput {
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(800.0, 600.0),
        )),
        events,
        ..Default::default()
    };
    let mut output = ExplorerOutput::default();
    let _ = ctx.run(input, |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            output = explorer.show(ui, world, &[], editable);
        });
    });
    output
}

fn center_of(explorer: &Explorer, id: InstanceId) -> egui::Pos2 {
    let (_, rect) = explorer.drawn.iter().find(|(row, _)| *row == id).unwrap();
    rect.center()
}

/// Drags the name of `from`'s row with the mouse and lets go over `to`'s
/// row. Returns every action the Explorer asked for along the way.
fn drag_row(
    explorer: &mut Explorer,
    world: &World,
    from: InstanceId,
    to: InstanceId,
) -> Vec<Action> {
    drag_row_while(explorer, world, from, to, true)
}

/// `drag_row`, with the Explorer `editable` or not (during Play).
fn drag_row_while(
    explorer: &mut Explorer,
    world: &World,
    from: InstanceId,
    to: InstanceId,
    editable: bool,
) -> Vec<Action> {
    let ctx = theme::test_context();
    frame_while(&ctx, explorer, world, Vec::new(), editable);
    let (start, end) = (center_of(explorer, from), center_of(explorer, to));
    let button = |pos, pressed| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    let mut frames = vec![
        vec![egui::Event::PointerMoved(start)],
        vec![button(start, true)],
    ];
    for step in 1..=5 {
        frames.push(vec![egui::Event::PointerMoved(
            start.lerp(end, step as f32 / 5.0),
        )]);
    }
    frames.push(vec![button(end, false)]);
    frames.push(Vec::new());
    frames
        .into_iter()
        .filter_map(|events| frame_while(&ctx, explorer, world, events, editable).action)
        .collect()
}

/// game → Workspace → Car (a Model, open) → Wheel, and a Folder.
fn car_world() -> (World, InstanceId, InstanceId, InstanceId) {
    let mut world = World::new();
    let workspace = world.workspace();
    let mut add = |class, name, parent| {
        let id = world.create(class).unwrap();
        world.set_name(id, name).unwrap();
        world.set_parent(id, Some(parent)).unwrap();
        id
    };
    let car = add(ClassName::Model, "Car", workspace);
    let wheel = add(ClassName::Part, "Wheel", car);
    let props = add(ClassName::Folder, "Props", workspace);
    (world, car, wheel, props)
}

#[test]
fn dragging_a_row_onto_another_asks_to_move_it_inside() {
    let (world, car, wheel, props) = car_world();
    let mut explorer = Explorer::new(&world);
    explorer.toggle(car);
    assert_eq!(
        drag_row(&mut explorer, &world, wheel, props),
        [Action::MoveInto {
            id: wheel,
            parent: props
        }]
    );
}

#[test]
fn dragging_a_row_onto_its_own_child_or_a_service_outside_the_workspace_does_nothing() {
    let (world, car, wheel, _) = car_world();
    let mut explorer = Explorer::new(&world);
    explorer.toggle(car);
    assert_eq!(drag_row(&mut explorer, &world, car, wheel), []);
    let scripts = world.server_script_service();
    assert_eq!(drag_row(&mut explorer, &world, car, scripts), []);
    // Services can't be dragged at all.
    assert_eq!(drag_row(&mut explorer, &world, world.workspace(), car), []);
}

#[test]
fn while_playing_rows_can_be_clicked_but_not_dragged_or_renamed() {
    let (world, car, wheel, props) = car_world();
    let mut explorer = Explorer::new(&world);
    explorer.toggle(car);
    assert_eq!(
        drag_row_while(&mut explorer, &world, wheel, props, false),
        []
    );

    // A rename under way when Play starts is dropped.
    let ctx = theme::test_context();
    explorer.start_rename(&world, props);
    frame_while(&ctx, &mut explorer, &world, Vec::new(), false);
    assert_eq!(explorer.renaming(), None);

    // Clicking still selects, to inspect the running game.
    let at = center_of(&explorer, wheel);
    let button = |pressed| egui::Event::PointerButton {
        pos: at,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    frame_while(
        &ctx,
        &mut explorer,
        &world,
        vec![egui::Event::PointerMoved(at)],
        false,
    );
    frame_while(&ctx, &mut explorer, &world, vec![button(true)], false);
    let clicked = frame_while(&ctx, &mut explorer, &world, vec![button(false)], false);
    assert_eq!(clicked.clicked, Some(wheel));
}

fn key(key: egui::Key) -> egui::Event {
    egui::Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    }
}

#[test]
fn renaming_in_the_explorer_ends_with_enter_and_escape_cancels() {
    let (world, _, _, props) = car_world();
    let mut explorer = Explorer::new(&world);
    let ctx = theme::test_context();

    explorer.start_rename(&world, props);
    frame(&ctx, &mut explorer, &world, Vec::new());
    assert_eq!(explorer.renaming(), Some(props));
    let typed = vec![egui::Event::Text("2".to_string())];
    assert_eq!(frame(&ctx, &mut explorer, &world, typed).action, None);
    let entered = frame(&ctx, &mut explorer, &world, vec![key(egui::Key::Enter)]);
    // Where the cursor starts is up to egui, so the 2 is at one end or the other.
    let Some(Action::Rename(id, name)) = entered.action else {
        panic!("expected a rename, got {:?}", entered.action);
    };
    assert_eq!(id, props);
    assert!(name == "Props2" || name == "2Props", "{name}");
    assert_eq!(explorer.renaming(), None);

    explorer.start_rename(&world, props);
    frame(&ctx, &mut explorer, &world, Vec::new());
    let typed = vec![egui::Event::Text("x".to_string())];
    frame(&ctx, &mut explorer, &world, typed);
    let escaped = frame(&ctx, &mut explorer, &world, vec![key(egui::Key::Escape)]);
    assert_eq!(escaped.action, None);
    assert_eq!(explorer.renaming(), None);

    // Services keep their names.
    explorer.start_rename(&world, world.workspace());
    assert_eq!(explorer.renaming(), None);
}

#[test]
fn revealing_a_deep_part_opens_every_branch_above_it() {
    let mut world = World::new();
    let car = world.create(ClassName::Part).unwrap();
    world.set_parent(car, Some(world.workspace())).unwrap();
    let wheel = world.create(ClassName::Part).unwrap();
    world.set_parent(wheel, Some(car)).unwrap();
    let mut explorer = Explorer::new(&world);
    explorer.toggle(world.workspace());
    assert!(!explorer.rows(&world).iter().any(|row| row.id == wheel));

    explorer.reveal(&world, wheel);
    let rows = explorer.rows(&world);
    assert!(rows.iter().any(|row| row.id == wheel));
    // The wheel itself has no children, and the Script service stays closed.
    assert!(!rows.iter().find(|row| row.id == wheel).unwrap().expanded);
    assert_eq!(explorer.scroll_to, Some(wheel));
}

#[test]
fn the_filter_shows_what_matches_and_the_branches_above_it() {
    let mut world = World::new();
    let make = |world: &mut World, class, name: &str, parent| {
        let id = world.create(class).unwrap();
        world.set_name(id, name).unwrap();
        world.set_parent(id, Some(parent)).unwrap();
        id
    };
    let workspace = world.workspace();
    let car = make(&mut world, ClassName::Model, "Car", workspace);
    let wheel = make(&mut world, ClassName::Part, "Front Wheel", car);
    make(&mut world, ClassName::Part, "Door", car);
    let light = make(&mut world, ClassName::PointLight, "Headlight", wheel);
    make(&mut world, ClassName::Part, "Floor", workspace);
    let mut explorer = Explorer::new(&world);
    // Car is closed, so its parts aren't shown.
    assert!(!outline(&explorer, &world).contains(&"      Front Wheel".to_owned()));

    // Case and spaces don't matter, and closed branches open.
    explorer.filter = " frontwheel ".to_owned();
    assert_eq!(
        outline(&explorer, &world),
        ["Game", "  Workspace", "    Car", "      Front Wheel"].map(str::to_owned)
    );
    // Only the match is drawn bright; the branches above it are dimmed.
    let bright: Vec<bool> = explorer
        .rows(&world)
        .iter()
        .map(|row| row.matches)
        .collect();
    assert_eq!(bright, [false, false, false, true]);

    // A class matches too.
    explorer.filter = "PointLight".to_owned();
    let rows = explorer.rows(&world);
    assert_eq!(rows.last().map(|row| row.id), Some(light));
    assert!(rows.last().unwrap().matches);

    // Nothing matching shows nothing, and a cleared filter puts back the
    // branches as they were.
    explorer.filter = "zzz".to_owned();
    assert!(explorer.rows(&world).is_empty());
    explorer.filter.clear();
    assert!(!explorer.rows(&world).iter().any(|row| row.id == wheel));
}

#[test]
fn clicking_an_arrow_while_filtering_leaves_the_branches_alone() {
    let world = demo_world();
    let ctx = crate::theme::test_context();
    let click_workspace_arrow = |explorer: &mut Explorer| {
        frame(&ctx, explorer, &world, Vec::new());
        let name = explorer
            .drawn
            .iter()
            .find(|(id, _)| *id == world.workspace())
            .unwrap()
            .1;
        // The arrow is just left of the name.
        let arrow = name.left_center() - egui::vec2(ARROW_SIZE / 2.0 + 2.0, 0.0);
        let press = |pressed| egui::Event::PointerButton {
            pos: arrow,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        for events in [
            vec![egui::Event::PointerMoved(arrow)],
            vec![press(true)],
            vec![press(false)],
        ] {
            frame(&ctx, explorer, &world, events);
        }
    };
    let mut explorer = Explorer::new(&world);
    explorer.filter = "pad".to_owned();
    click_workspace_arrow(&mut explorer);
    assert!(explorer.expanded.contains(&world.workspace()));
    // The same click without a filter closes it.
    explorer.filter.clear();
    click_workspace_arrow(&mut explorer);
    assert!(!explorer.expanded.contains(&world.workspace()));
}

#[test]
fn snapshot_of_the_filter() {
    let Some(folder) = std::env::var_os("AXCENITY_SNAPSHOTS") else {
        return;
    };
    let world = demo_world();
    let mut explorer = Explorer::new(&world);
    explorer.filter = "pad".to_owned();
    crate::snapshot::snapshot(
        &std::path::PathBuf::from(folder).join("explorer-filter.png"),
        egui::vec2(260.0, 220.0),
        vec![Vec::new(); 3],
        |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                let _ = explorer.show(ui, &world, &[], true);
            });
        },
    );
}

/// A Folder of `count` Parts in the Workspace, open.
fn big_folder(count: usize) -> (World, Explorer, InstanceId, Vec<InstanceId>) {
    let mut world = World::new();
    let folder = world.create(ClassName::Folder).unwrap();
    world.set_parent(folder, Some(world.workspace())).unwrap();
    let parts = (0..count)
        .map(|_| {
            let part = world.create(ClassName::Part).unwrap();
            world.set_parent(part, Some(folder)).unwrap();
            part
        })
        .collect();
    let mut explorer = Explorer::new(&world);
    explorer.toggle(folder);
    (world, explorer, folder, parts)
}

/// With 10,000 rows only the ones on screen are laid out, and revealing
/// one far down scrolls it into the middle of the view.
#[test]
fn only_rows_on_screen_are_drawn_and_reveal_scrolls_to_any_row() {
    let (world, mut explorer, _, parts) = big_folder(10_000);
    let ctx = theme::test_context();
    frame(&ctx, &mut explorer, &world, Vec::new());
    assert!(
        explorer.drawn.len() < 60,
        "{} rows laid out",
        explorer.drawn.len()
    );
    let far = parts[6_000];
    explorer.reveal(&world, far);
    for _ in 0..2 {
        frame(&ctx, &mut explorer, &world, Vec::new());
    }
    let (_, rect) = explorer
        .drawn
        .iter()
        .find(|(id, _)| *id == far)
        .expect("the revealed row is drawn");
    assert!(
        (100.0..500.0).contains(&rect.center().y),
        "near the middle of the 600-point window: {rect:?}"
    );
}

/// The rows are made again only when the tree or the open branches
/// change, and a rename shows without that.
#[test]
fn rows_are_kept_until_the_tree_or_the_open_branches_change() {
    let (mut world, mut explorer, folder, parts) = big_folder(3);
    let ctx = theme::test_context();
    frame(&ctx, &mut explorer, &world, Vec::new());
    let key = explorer.cache.as_ref().unwrap().key;
    frame(&ctx, &mut explorer, &world, Vec::new());
    assert_eq!(explorer.cache.as_ref().unwrap().key, key, "nothing changed");

    world.set_name(parts[0], "Renamed").unwrap();
    let mut drawn_names = Vec::new();
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(800.0, 600.0),
        )),
        ..Default::default()
    };
    let output = ctx.run(input, |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            explorer.show(ui, &world, &[], true);
        });
    });
    for clipped in &output.shapes {
        if let egui::Shape::Text(text) = &clipped.shape {
            drawn_names.push(text.galley.text().to_owned());
        }
    }
    assert!(
        drawn_names.iter().any(|text| text.contains("Renamed")),
        "the new name is drawn: {drawn_names:?}"
    );

    // A new part, a closed branch and a reorder each make new rows.
    let added = world.create(ClassName::Part).unwrap();
    world.set_parent(added, Some(folder)).unwrap();
    frame(&ctx, &mut explorer, &world, Vec::new());
    assert!(explorer.drawn.iter().any(|(id, _)| *id == added));
    explorer.toggle(folder);
    frame(&ctx, &mut explorer, &world, Vec::new());
    assert!(!explorer.drawn.iter().any(|(id, _)| *id == added));
    explorer.toggle(folder);
    world.set_parent_at(added, folder, 0).unwrap();
    frame(&ctx, &mut explorer, &world, Vec::new());
    let order: Vec<InstanceId> = explorer.drawn.iter().map(|(id, _)| *id).collect();
    let at = order.iter().position(|id| *id == added).unwrap();
    assert_eq!(order[at + 1], parts[0], "moved to the front of the folder");
}
