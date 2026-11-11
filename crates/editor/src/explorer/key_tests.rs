//! The Explorer's keys (design system V6): arrows, Shift+arrows, Home and
//! End once it's clicked or tabbed into, and nothing before.

use super::*;

const SIZE: egui::Vec2 = egui::vec2(800.0, 600.0);

/// The Explorer in a 300-point column on the left, with the app's part
/// played here: it takes the keys first when the Explorer has them, and
/// applies a click or a move to `selection`.
struct Panel {
    ctx: egui::Context,
    world: World,
    explorer: Explorer,
    selection: Vec<InstanceId>,
}

impl Panel {
    fn new(world: World) -> Self {
        let explorer = Explorer::new(&world);
        let mut panel = Self {
            ctx: theme::test_context(),
            world,
            explorer,
            selection: Vec::new(),
        };
        panel.frame(Vec::new());
        panel
    }

    fn frame(&mut self, events: Vec<egui::Event>) {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, SIZE)),
            events,
            ..Default::default()
        };
        let Self {
            ctx,
            world,
            explorer,
            selection,
        } = self;
        let mut output = ExplorerOutput::default();
        let _ = crate::theme::run(ctx, input, |ui| {
            let ctx = &ui.ctx().clone();
            let typing = ctx.egui_wants_keyboard_input() && !explorer.has_focus(ctx);
            if !typing && explorer.has_keys(ctx) {
                ctx.input_mut(|input| explorer.take_keys(input));
            }
            egui::Panel::left("explorer")
                .exact_size(300.0)
                .show(ui, |ui| output = explorer.show(ui, world, selection, true));
            egui::CentralPanel::default().show(ui, |_| {});
        });
        if let Some(moved) = output.select {
            *selection = moved;
        } else if output.clicked.is_some() {
            let order: Vec<_> = explorer.rows(world).iter().map(|row| row.id).collect();
            update_selection(
                selection,
                output.clicked,
                output.modifiers,
                Some(&order),
                false,
            );
        }
    }

    fn press(&mut self, key: egui::Key, shift: bool) {
        let modifiers = if shift {
            egui::Modifiers::SHIFT
        } else {
            egui::Modifiers::NONE
        };
        self.frame(vec![egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        }]);
        // A branch opened or closed shows next frame.
        self.frame(Vec::new());
    }

    fn click_at(&mut self, at: egui::Pos2) {
        self.frame(vec![egui::Event::PointerMoved(at)]);
        for pressed in [true, false] {
            self.frame(vec![egui::Event::PointerButton {
                pos: at,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            }]);
        }
    }

    fn click(&mut self, id: InstanceId) {
        let (_, rect) = *self
            .explorer
            .drawn
            .iter()
            .find(|(row, _)| *row == id)
            .expect("the row is drawn");
        self.click_at(rect.center());
    }

    fn has_keys(&self) -> bool {
        self.explorer.has_keys(&self.ctx)
    }

    fn names(&self) -> Vec<String> {
        self.selection
            .iter()
            .map(|id| self.world.get(*id).unwrap().name().to_string())
            .collect()
    }
}

/// game → Workspace → Car (a Model) → Wheel, and a Folder after the Car.
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
fn the_arrow_keys_move_open_and_close_once_the_explorer_is_clicked() {
    let (world, car, wheel, props) = car_world();
    let mut panel = Panel::new(world);
    assert!(!panel.has_keys(), "not until it's clicked");
    panel.press(egui::Key::ArrowDown, false);
    assert!(panel.selection.is_empty());

    panel.click(car);
    assert_eq!(panel.selection, [car]);
    assert!(panel.has_keys());
    panel.press(egui::Key::ArrowDown, false);
    assert_eq!(panel.selection, [props], "Down: the next row");
    panel.press(egui::Key::ArrowUp, false);
    assert_eq!(panel.selection, [car], "Up: back");

    panel.press(egui::Key::ArrowRight, false);
    assert!(panel.explorer.expanded.contains(&car), "Right opens");
    assert_eq!(panel.selection, [car]);
    panel.press(egui::Key::ArrowRight, false);
    assert_eq!(panel.selection, [wheel], "Right again: the first child");
    panel.press(egui::Key::ArrowLeft, false);
    assert_eq!(panel.selection, [car], "Left: the parent");
    panel.press(egui::Key::ArrowLeft, false);
    assert!(!panel.explorer.expanded.contains(&car), "Left again closes");
    assert_eq!(panel.selection, [car]);

    let rows = panel.explorer.rows(&panel.world);
    panel.press(egui::Key::End, false);
    assert_eq!(
        panel.selection,
        [rows.last().unwrap().id],
        "End: the last row"
    );
    panel.press(egui::Key::Home, false);
    assert_eq!(panel.selection, [panel.world.root()], "Home: game");
    panel.press(egui::Key::ArrowUp, false);
    assert_eq!(panel.selection, [panel.world.root()], "Up stops at the top");

    // A click anywhere else takes the keys away.
    panel.click_at(egui::pos2(600.0, 300.0));
    assert!(!panel.has_keys());
    panel.press(egui::Key::ArrowDown, false);
    assert_eq!(panel.selection, [panel.world.root()]);
}

#[test]
fn shift_arrows_extend_the_selection_from_where_the_keys_started() {
    let (world, car, _, props) = car_world();
    let workspace = world.workspace();
    let mut panel = Panel::new(world);
    panel.click(workspace);
    panel.press(egui::Key::ArrowDown, true);
    panel.press(egui::Key::ArrowDown, true);
    assert_eq!(
        panel.selection,
        [workspace, car, props],
        "the last is primary"
    );
    panel.press(egui::Key::ArrowUp, true);
    assert_eq!(panel.selection, [workspace, car]);
    // Above the start, the range turns round.
    panel.press(egui::Key::ArrowUp, true);
    panel.press(egui::Key::ArrowUp, true);
    assert_eq!(panel.names(), ["Workspace", "Game"]);
    // A plain arrow starts again from one row.
    panel.press(egui::Key::ArrowDown, false);
    assert_eq!(panel.selection, [workspace]);
}

#[test]
fn tab_reaches_the_explorer_after_its_filter() {
    let (world, ..) = car_world();
    let mut panel = Panel::new(world);
    panel.press(egui::Key::Tab, false);
    assert!(panel.ctx.egui_wants_keyboard_input(), "Filter first");
    assert!(!panel.explorer.has_focus(&panel.ctx));
    panel.press(egui::Key::Tab, false);
    assert!(panel.explorer.has_focus(&panel.ctx), "then the rows");
    assert!(panel.explorer.ring, "shown with the focus ring");
    panel.press(egui::Key::ArrowDown, false);
    assert_eq!(panel.selection, [panel.world.root()], "Down: the first row");
    panel.press(egui::Key::ArrowDown, false);
    assert_eq!(panel.names(), ["Workspace"]);
    assert!(
        panel.explorer.has_focus(&panel.ctx),
        "arrows don't move focus"
    );
}

#[test]
fn the_keys_scroll_just_far_enough_to_show_the_row() {
    let mut world = World::new();
    let folder = world.create(ClassName::Folder).unwrap();
    world.set_parent(folder, Some(world.workspace())).unwrap();
    for _ in 0..1_000 {
        let part = world.create(ClassName::Part).unwrap();
        world.set_parent(part, Some(folder)).unwrap();
    }
    let mut panel = Panel::new(world);
    panel.explorer.toggle(folder);
    panel.frame(Vec::new());
    panel.click(folder);
    let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, SIZE);
    let shown = |panel: &Panel, id: InstanceId| {
        panel
            .explorer
            .drawn
            .iter()
            .find(|(row, _)| *row == id)
            .is_some_and(|(_, rect)| screen.contains_rect(*rect))
    };
    panel.press(egui::Key::End, false);
    let last = panel.selection[0];
    assert!(shown(&panel, last), "End shows the last row");
    let bottom = panel
        .explorer
        .drawn
        .iter()
        .find(|(row, _)| *row == last)
        .unwrap()
        .1;
    assert!(bottom.bottom() > SIZE.y - 40.0, "at the bottom: {bottom:?}");
    panel.press(egui::Key::ArrowUp, false);
    let up = panel.selection[0];
    let rect = panel
        .explorer
        .drawn
        .iter()
        .find(|(row, _)| *row == up)
        .unwrap()
        .1;
    assert!(
        rect.bottom() < bottom.bottom(),
        "already in view: no scrolling"
    );
    panel.press(egui::Key::Home, false);
    assert!(
        shown(&panel, panel.world.root()),
        "Home goes back to the top"
    );
}

#[test]
fn arrows_in_the_filter_move_its_cursor() {
    let (world, ..) = car_world();
    let mut panel = Panel::new(world);
    panel.click_at(egui::pos2(100.0, 15.0));
    assert!(
        panel.ctx.egui_wants_keyboard_input(),
        "the filter has the keyboard"
    );
    panel.frame(vec![egui::Event::Text("ab".into())]);
    panel.press(egui::Key::ArrowLeft, false);
    panel.frame(vec![egui::Event::Text("x".into())]);
    assert_eq!(panel.explorer.filter, "axb");
    assert!(panel.selection.is_empty());
}

/// With `AXCENITY_SNAPSHOTS`, saves the Explorer after the keys moved the
/// selection, with its focus ring (`explorer-keys.png`).
#[test]
fn snapshot_of_the_focus_ring() {
    let Some(folder) = std::env::var_os("AXCENITY_SNAPSHOTS") else {
        return;
    };
    let (world, car, ..) = car_world();
    let mut panel = Panel::new(world);
    panel.click(car);
    panel.press(egui::Key::ArrowRight, false);
    panel.press(egui::Key::ArrowDown, false);
    let Panel {
        world,
        explorer,
        selection,
        ..
    } = &mut panel;
    crate::snapshot::snapshot(
        &std::path::PathBuf::from(folder).join("explorer-keys.png"),
        egui::vec2(260.0, 220.0),
        vec![Vec::new(); 3],
        |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                let _ = explorer.show(ui, world, selection, true);
            });
        },
    );
}
