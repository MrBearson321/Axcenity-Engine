//! The Explorer panel: the Instance tree, from `game` down, with branches
//! that open and close. Clicking a row selects that Instance.
//!
//! Right-clicking a row opens a menu to insert, rename, duplicate or delete,
//! and dragging a row onto another moves it inside that one. Double-clicking
//! a Script or LocalScript opens it in a script tab, and a ⚠ after one says
//! it uses something that only works on the other side. The Explorer
//! never changes the world itself: [`Explorer::show`] returns an [`Action`]
//! and the app carries it out, so it's recorded for undo.
//!
//! [`Explorer::rows`] works out which rows are visible without any UI code,
//! so tests can check the tree; [`Explorer::show`] only draws those rows.

use std::collections::{HashMap, HashSet};

use axcenity_core::{ClassName, InstanceId, World};
use eframe::egui;

use crate::app::{DELETE_KEY_TEXT, DUPLICATE_SHORTCUT, RENAME_KEY};
use crate::properties;
use crate::tree::{self, Insertable};

/// How far each level of the tree is indented, in points.
const INDENT: f32 = 14.0;
/// The size of the open/close arrow's clickable square, in points.
const ARROW_SIZE: f32 = 16.0;
/// The kinds of script the Insert menus offer.
pub const SCRIPT_CLASSES: [ClassName; 3] = [
    ClassName::Script,
    ClassName::LocalScript,
    ClassName::ModuleScript,
];

/// The hover text of every button and menu item that inserts a script of `class`.
pub fn script_hover(class: ClassName) -> &'static str {
    match class {
        ClassName::ModuleScript => {
            "Make a new ModuleScript, code that other scripts load with \
             require(game.ReplicatedStorage.Name), in ReplicatedStorage, with its .module.lua \
             file in the project's scripts folder, and open it."
        }
        ClassName::LocalScript => {
            "Make a new LocalScript, which runs on the player's side (keyboard, camera), in \
             StarterPlayerScripts, with its .client.lua file in the project's scripts folder, \
             and open it."
        }
        _ => {
            "Make a new Script, which runs on the server, in ServerScriptService, with its \
             .server.lua file in the project's scripts folder, and open it."
        }
    }
}
/// The hover text of Insert ▸ RemoteEvent.
const REMOTE_EVENT_HOVER: &str = "Make a RemoteEvent, for sending messages between Scripts and \
     LocalScripts. It goes in ReplicatedStorage, where both can find it, unless a Model or \
     Folder is selected.";
/// The outline around the row something would be dropped into: the same
/// blue as the viewport's selection box.
const DROP_COLOR: egui::Color32 = egui::Color32::from_rgb(25, 160, 255);

/// Which branches are open, and the rename in progress. IDs of Instances
/// that no longer exist do no harm: they simply never match a row again.
pub struct Explorer {
    expanded: HashSet<InstanceId>,
    /// A row to scroll into view on the next frame, after something was
    /// selected in the viewport.
    scroll_to: Option<InstanceId>,
    /// The row being renamed, which shows a text box instead of its name.
    renaming: Option<Renaming>,
    /// The hover text of the ⚠ badge after each script that uses an API
    /// from the other side. The app fills it in each frame.
    pub warnings: HashMap<InstanceId, String>,
    /// Where each row's name was drawn last frame, so tests can point at rows.
    #[cfg(test)]
    drawn: Vec<(InstanceId, egui::Rect)>,
}

struct Renaming {
    id: InstanceId,
    /// The name as typed so far.
    text: String,
    /// True until the text box has been given the keyboard, on its first frame.
    needs_focus: bool,
}

/// Something done in the Explorer, for the app to carry out.
#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    /// Insert `what`, going by the rule in [`tree::insert_parent`] with
    /// `near` as the selection.
    Insert {
        what: Insertable,
        near: Option<InstanceId>,
    },
    /// Make a new script file and its Script (in ServerScriptService) or
    /// LocalScript (in StarterPlayerScripts).
    InsertScript(ClassName),
    Rename(InstanceId, String),
    Duplicate(InstanceId),
    Delete(InstanceId),
    /// `id` was dropped onto `parent`'s row, so it becomes `parent`'s last child.
    MoveInto {
        id: InstanceId,
        parent: InstanceId,
    },
}

/// What happened in the Explorer this frame.
#[derive(Debug, Default, PartialEq)]
pub struct ExplorerOutput {
    /// A row that was clicked (with either button), to select.
    pub clicked: Option<InstanceId>,
    pub action: Option<Action>,
    /// A Script to open in a tab: its row was double-clicked, or Open was
    /// picked in its menu. Works during Play too.
    pub open: Option<InstanceId>,
}

/// What egui's drag and drop carries while a row is being dragged.
#[derive(Clone, Copy, Debug)]
struct DraggedRow(InstanceId);

/// One visible line in the Explorer.
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub id: InstanceId,
    pub name: String,
    pub class: ClassName,
    /// How many levels below `game` it is (`game` itself is 0).
    pub depth: usize,
    pub has_children: bool,
    pub expanded: bool,
}

impl Explorer {
    /// Starts with the root (`game` in scripts, named `Game`) and `Workspace` open, so the parts are in view.
    pub fn new(world: &World) -> Self {
        Self {
            expanded: HashSet::from([world.root(), world.workspace()]),
            scroll_to: None,
            renaming: None,
            warnings: HashMap::new(),
            #[cfg(test)]
            drawn: Vec::new(),
        }
    }

    /// Turns `id`'s row into a text box to type a new name in, if it can be
    /// renamed. Enter or clicking elsewhere keeps the name; Escape cancels.
    pub fn start_rename(&mut self, world: &World, id: InstanceId) {
        let Some(instance) = world.get(id) else {
            return;
        };
        if !properties::can_rename(instance) {
            return;
        }
        self.reveal(world, id);
        self.renaming = Some(Renaming {
            id,
            text: instance.name().to_string(),
            needs_focus: true,
        });
    }

    /// The Instance being renamed, if any.
    #[cfg(test)]
    pub fn renaming(&self) -> Option<InstanceId> {
        self.renaming.as_ref().map(|renaming| renaming.id)
    }

    /// Opens every branch above `id`, so its row is visible, and scrolls to
    /// it on the next frame. Used when a part is selected in the viewport.
    pub fn reveal(&mut self, world: &World, id: InstanceId) {
        let mut parent = world.get(id).and_then(|instance| instance.parent());
        while let Some(next) = parent {
            self.expanded.insert(next);
            parent = world.get(next).and_then(|instance| instance.parent());
        }
        self.scroll_to = Some(id);
    }

    /// Opens a closed branch, or closes an open one.
    pub fn toggle(&mut self, id: InstanceId) {
        // `insert` returns false when the ID was already there, which means
        // the branch was open: close it instead.
        if !self.expanded.insert(id) {
            self.expanded.remove(&id);
        }
    }

    /// The visible rows, top to bottom: every Instance whose ancestors are
    /// all open, with children in their stored order.
    pub fn rows(&self, world: &World) -> Vec<Row> {
        let mut rows = Vec::new();
        self.add_rows(world, world.root(), 0, &mut rows);
        rows
    }

    /// Adds the row for `id`, then (if it's open) its children's rows.
    fn add_rows(&self, world: &World, id: InstanceId, depth: usize, rows: &mut Vec<Row>) {
        let Some(instance) = world.get(id) else {
            return;
        };
        let expanded = self.expanded.contains(&id);
        rows.push(Row {
            id,
            name: instance.name().to_string(),
            class: instance.class(),
            depth,
            has_children: !instance.children().is_empty(),
            expanded,
        });
        if expanded {
            for &child in instance.children() {
                self.add_rows(world, child, depth + 1, rows);
            }
        }
    }

    /// Draws the tree with the `selected` row highlighted. Returns the row
    /// that was clicked, and anything to do to the tree. When not `editable`
    /// (during Play), rows can still be clicked and opened, but there's no
    /// right-click menu, renaming or dragging.
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        world: &World,
        selected: Option<InstanceId>,
        editable: bool,
    ) -> ExplorerOutput {
        ui.strong("Explorer");
        ui.separator();

        // A rename of something that has left the tree (undone, say) can't
        // finish, and nothing is renamed during Play.
        if self
            .renaming
            .as_ref()
            .is_some_and(|renaming| !world.is_descendant_of(renaming.id, world.root()))
            || !editable
        {
            self.renaming = None;
        }
        // Changing which rows there are while looping over them would change
        // what's being looped over, so remember what happened and do it afterward.
        let mut toggled = None;
        let mut start_rename = None;
        let mut rename_done = None;
        let mut output = ExplorerOutput::default();
        let scroll_to = self.scroll_to.take();
        let dragged = egui::DragAndDrop::payload::<DraggedRow>(ui.ctx()).map(|row| row.0);
        #[cfg(test)]
        self.drawn.clear();
        egui::ScrollArea::both().auto_shrink(false).show(ui, |ui| {
            for row in self.rows(world) {
                ui.horizontal(|ui| {
                    ui.add_space(row.depth as f32 * INDENT);
                    let arrow_sense = if row.has_children {
                        egui::Sense::click()
                    } else {
                        egui::Sense::hover()
                    };
                    let (_, arrow) =
                        ui.allocate_exact_size(egui::Vec2::splat(ARROW_SIZE), arrow_sense);
                    if row.has_children {
                        let openness = if row.expanded { 1.0 } else { 0.0 };
                        egui::collapsing_header::paint_default_icon(ui, openness, &arrow);
                    }
                    if arrow.clicked() {
                        toggled = Some(row.id);
                    }

                    // `filter` keeps the rename only if it's for this row.
                    if let Some(renaming) = self.renaming.as_mut().filter(|r| r.id == row.id) {
                        if let Some(done) = rename_box(ui, renaming) {
                            rename_done = Some(done);
                        }
                        return;
                    }

                    // Clicks select; dragging moves the row onto another.
                    let label = ui
                        .add(
                            egui::Button::selectable(selected == Some(row.id), &row.name)
                                .sense(egui::Sense::click_and_drag()),
                        )
                        .on_hover_text(row.class.as_str());
                    #[cfg(test)]
                    self.drawn.push((row.id, label.rect));
                    // A right-click selects too, so the menu acts on what's selected.
                    if label.clicked() || label.secondary_clicked() {
                        output.clicked = Some(row.id);
                    }
                    if scroll_to == Some(row.id) {
                        label.scroll_to_me(Some(egui::Align::Center));
                    }
                    // Show the class too when the name doesn't already say it.
                    if row.name != row.class.as_str() {
                        ui.weak(row.class.as_str());
                    }
                    if row.has_children && label.double_clicked() {
                        toggled = Some(row.id);
                    }
                    if let Some(text) = warning_badge(&self.warnings, world, &row) {
                        ui.colored_label(ui.visuals().warn_fg_color, "⚠")
                            .on_hover_text(text);
                    }
                    if row.class.is_script() && label.double_clicked() {
                        output.open = Some(row.id);
                    }

                    if !editable {
                        return;
                    }
                    if tree::check_editable(world, row.id).is_ok() {
                        label.dnd_set_drag_payload(DraggedRow(row.id));
                    }
                    if let Some(action) = drop_onto(ui, &label, world, dragged, row.id) {
                        output.action = Some(action);
                    }
                    label.context_menu(|ui| match row_menu(ui, world, row.id) {
                        Some(RowCommand::Rename) => start_rename = Some(row.id),
                        Some(RowCommand::Open) => output.open = Some(row.id),
                        Some(command) => output.action = command.action(row.id),
                        None => {}
                    });
                });
            }
        });

        if let Some(id) = toggled {
            self.toggle(id);
        }
        if let Some(id) = start_rename {
            self.start_rename(world, id);
        }
        if let Some(done) = rename_done
            && let Some(renaming) = self.renaming.take()
            && let Some(name) = done
        {
            output.action = Some(Action::Rename(renaming.id, name));
        }
        if let Some(id) = dragged {
            paint_dragged_name(ui, world, id);
        }
        output
    }
}

/// The hover text of `row`'s ⚠ badge, if it has one: its own warnings, or,
/// on a closed branch, a pointer to the scripts inside with warnings, so
/// they aren't hidden.
fn warning_badge(
    warnings: &HashMap<InstanceId, String>,
    world: &World,
    row: &Row,
) -> Option<String> {
    if let Some(text) = warnings.get(&row.id) {
        return Some(text.clone());
    }
    if row.expanded {
        return None;
    }
    let mut inside: Vec<&str> = warnings
        .keys()
        .filter(|&&id| world.is_descendant_of(id, row.id))
        .filter_map(|&id| world.get(id).map(|script| script.name()))
        .collect();
    inside.sort_unstable();
    (!inside.is_empty()).then(|| {
        format!(
            "Open this to see the warnings of {}: they use something that only works on the \
             other side.",
            inside.join(", ")
        )
    })
}

/// The text box a row turns into while it's renamed. Returns `None` while
/// typing, then `Some(Some(name))` to rename or `Some(None)` to cancel.
fn rename_box(ui: &mut egui::Ui, renaming: &mut Renaming) -> Option<Option<String>> {
    let text_box = ui.add(egui::TextEdit::singleline(&mut renaming.text).desired_width(160.0));
    if renaming.needs_focus {
        text_box.request_focus();
        renaming.needs_focus = false;
        return None;
    }
    if !text_box.lost_focus() {
        return None;
    }
    // Escape takes the keyboard away from the text box too, so check which it was.
    let cancelled = ui.input(|input| input.key_pressed(egui::Key::Escape));
    let name = renaming.text.trim();
    if cancelled || name.is_empty() {
        Some(None)
    } else {
        Some(Some(name.to_string()))
    }
}

/// While a row is dragged over `target`'s row: outlines it if the row can be
/// dropped there, or shows a "not allowed" cursor and the reason. Returns the
/// move if the row was let go over it.
fn drop_onto(
    ui: &egui::Ui,
    label: &egui::Response,
    world: &World,
    dragged: Option<InstanceId>,
    target: InstanceId,
) -> Option<Action> {
    let dragged = dragged?;
    // Over its own row is where every drag starts, so say nothing there.
    if dragged == target || label.dnd_hover_payload::<DraggedRow>().is_none() {
        return None;
    }
    let allowed = tree::check_move(world, dragged, target);
    match allowed {
        Ok(()) => {
            let stroke = egui::Stroke::new(1.5_f32, DROP_COLOR);
            ui.painter()
                .rect_stroke(label.rect, 2.0, stroke, egui::StrokeKind::Outside);
        }
        Err(reason) => {
            ui.ctx().set_cursor_icon(egui::CursorIcon::NotAllowed);
            label.show_tooltip_text(reason);
        }
    }
    let dropped = label.dnd_release_payload::<DraggedRow>().is_some();
    (dropped && allowed.is_ok()).then_some(Action::MoveInto {
        id: dragged,
        parent: target,
    })
}

/// Shows the name of the row being dragged next to the mouse.
fn paint_dragged_name(ui: &egui::Ui, world: &World, id: InstanceId) {
    let (Some(instance), Some(pointer)) = (world.get(id), ui.ctx().pointer_latest_pos()) else {
        return;
    };
    // The tooltip layer is drawn over every panel.
    let layer = egui::LayerId::new(egui::Order::Tooltip, egui::Id::new("dragged explorer row"));
    let painter = ui.ctx().layer_painter(layer);
    let galley = painter.layout_no_wrap(
        instance.name().to_string(),
        egui::TextStyle::Body.resolve(ui.style()),
        ui.visuals().strong_text_color(),
    );
    let at = pointer + egui::vec2(14.0, -galley.size().y / 2.0);
    let background = egui::Rect::from_min_size(at, galley.size()).expand(4.0);
    painter.rect_filled(background, 3.0, ui.visuals().window_fill);
    painter.galley(at, galley, egui::Color32::WHITE);
}

/// A choice in a row's right-click menu.
#[derive(Clone, Copy, Debug, PartialEq)]
enum RowCommand {
    Insert(Insertable),
    InsertScript(ClassName),
    /// Open a script in a tab.
    Open,
    Rename,
    Duplicate,
    Delete,
}

impl RowCommand {
    /// The tree action for this command on `id`'s row. Renaming isn't one
    /// yet: it starts a text box, and the rename comes when it's done.
    /// Opening a script doesn't change the tree.
    fn action(self, id: InstanceId) -> Option<Action> {
        match self {
            RowCommand::Insert(what) => Some(Action::Insert {
                what,
                near: Some(id),
            }),
            RowCommand::InsertScript(class) => Some(Action::InsertScript(class)),
            RowCommand::Open | RowCommand::Rename => None,
            RowCommand::Duplicate => Some(Action::Duplicate(id)),
            RowCommand::Delete => Some(Action::Delete(id)),
        }
    }
}

/// The right-click menu of `id`'s row: Open (for scripts), Insert ▸, then
/// Rename, Duplicate and Delete, greyed out (with the reason on hover) where
/// they can't be done.
fn row_menu(ui: &mut egui::Ui, world: &World, id: InstanceId) -> Option<RowCommand> {
    let mut chosen = None;
    let is_script = world
        .get(id)
        .is_some_and(|instance| instance.class().is_script());
    if is_script && ui.button("Open").clicked() {
        chosen = Some(RowCommand::Open);
    }
    ui.menu_button("Insert", |ui| {
        for what in Insertable::PARTS {
            if ui.button(what.label()).clicked() {
                chosen = Some(RowCommand::Insert(what));
            }
        }
        ui.separator();
        for what in Insertable::GROUPS {
            if ui.button(what.label()).clicked() {
                chosen = Some(RowCommand::Insert(what));
            }
        }
        ui.separator();
        for class in SCRIPT_CLASSES {
            if ui
                .button(class.as_str())
                .on_hover_text(script_hover(class))
                .clicked()
            {
                chosen = Some(RowCommand::InsertScript(class));
            }
        }
        let remote = Insertable::RemoteEvent;
        if ui
            .button(remote.label())
            .on_hover_text(REMOTE_EVENT_HOVER)
            .clicked()
        {
            chosen = Some(RowCommand::Insert(remote));
        }
    });
    ui.separator();

    let renamable = world.get(id).is_some_and(properties::can_rename);
    let rename = egui::Button::new("Rename").shortcut_text(RENAME_KEY.name());
    if ui
        .add_enabled(renamable, rename)
        .on_disabled_hover_text("Services keep their names.")
        .clicked()
    {
        chosen = Some(RowCommand::Rename);
    }
    let editable = tree::check_editable(world, id);
    let reason = editable.err().unwrap_or_default();
    let duplicate =
        egui::Button::new("Duplicate").shortcut_text(ui.ctx().format_shortcut(&DUPLICATE_SHORTCUT));
    if ui
        .add_enabled(editable.is_ok(), duplicate)
        .on_disabled_hover_text(reason)
        .clicked()
    {
        chosen = Some(RowCommand::Duplicate);
    }
    let deletable = tree::check_delete(world, id);
    let delete = egui::Button::new("Delete").shortcut_text(DELETE_KEY_TEXT);
    if ui
        .add_enabled(deletable.is_ok(), delete)
        .on_disabled_hover_text(deletable.err().unwrap_or_default())
        .clicked()
    {
        chosen = Some(RowCommand::Delete);
    }
    chosen
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    fn demo_world() -> World {
        let folder = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/hello-touched");
        axcenity_project::load(&folder).unwrap().world
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
                "ReplicatedStorage",
                "ServerScriptService",
                "StarterPlayer"
            ]
        );
        let scripts = &rows[rows.len() - 2];
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
        assert_eq!(opened[opened.len() - 2], "    pad");

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
            "  ReplicatedStorage",
            "  ServerScriptService",
            "  StarterPlayer",
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
                output = explorer.show(ui, world, None, editable);
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
        let ctx = egui::Context::default();
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
        let ctx = egui::Context::default();
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
        let ctx = egui::Context::default();

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
}
