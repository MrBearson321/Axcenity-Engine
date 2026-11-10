//! The Explorer panel: the Instance tree, from `game` down, with branches
//! that open and close. Clicking a row selects that Instance. Ctrl (Cmd on
//! macOS) toggles it in the selection, and Shift selects every visible row
//! from the last one to the clicked one.
//!
//! Right-clicking a row opens a menu to insert, rename, duplicate or delete,
//! and dragging a row onto another moves it inside that one. Double-clicking
//! a Script or LocalScript opens it in a script tab, and a warning sign after one says
//! it uses something that only works on the other side. The Explorer
//! never changes the world itself: [`Explorer::show`] returns an [`Action`]
//! and the app carries it out, so it's recorded for undo.
//!
//! [`Explorer::rows`] works out which rows are visible without any UI code,
//! so tests can check the tree; [`Explorer::show`] only draws those rows.

use std::collections::{HashMap, HashSet};

use axcenity_core::{ClassName, InstanceId, Value, World};
use eframe::egui;

use crate::app::{DELETE_KEY_TEXT, DUPLICATE_SHORTCUT, RENAME_KEY};
use crate::properties;
use crate::theme::{self, icons};
use crate::tree::{self, Insertable};

/// How far each level of the tree is indented, in points.
const INDENT: f32 = 14.0;
/// The size of the open/close arrow's clickable square, in points.
const ARROW_SIZE: f32 = 16.0;
/// A row shows its class after its name only if the name still gets this
/// many points.
const MIN_NAME_WIDTH: f32 = 72.0;
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
             and open it. Switch it between Code, Blocks and Graph at the top of its tab."
        }
        _ => {
            "Make a new Script, which runs on the server, in ServerScriptService, with its \
             .server.lua file in the project's scripts folder, and open it. Switch it between \
             Code, Blocks and Graph at the top of its tab."
        }
    }
}
/// The hover text of Insert ▸ RemoteEvent.
const REMOTE_EVENT_HOVER: &str = "Make a RemoteEvent, for sending messages between Scripts and \
     LocalScripts. It goes in ReplicatedStorage, where both can find it, unless a Model or \
     Folder is selected.";
/// The hover text of Insert ▸ RemoteFunction, BindableEvent and BindableFunction.
fn messenger_hover(class: ClassName) -> &'static str {
    match class {
        ClassName::RemoteFunction => {
            "Make a RemoteFunction: a LocalScript's InvokeServer waits for what the server's \
             OnServerInvoke returns. It goes in ReplicatedStorage, unless a Model or Folder is \
             selected."
        }
        ClassName::BindableEvent => {
            "Make a BindableEvent, for scripts on the same side to tell each other something: \
             Fire it, and its Event fires."
        }
        _ => {
            "Make a BindableFunction, for scripts on the same side to ask each other: Invoke \
             calls its OnInvoke and returns what that returns."
        }
    }
}
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
    /// The hover text of the warning badge after each script that uses an API
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
    /// Turn `Locked` on or off for it (and the rest of the selection, if
    /// it's in one).
    SetLocked(InstanceId, bool),
    /// `id` was dropped onto `parent`'s row, so it becomes `parent`'s last child.
    MoveInto {
        id: InstanceId,
        parent: InstanceId,
    },
    /// Give `id` an Animator (if it has none), a new clip and an Animation
    /// playing it, and open it in the Animation tab.
    Animate(InstanceId),
    /// Save `id` as a prefab, with every asset it uses, in an asset package.
    ExportPackage(InstanceId),
    /// Save the Part `id`'s look as a material file.
    SaveMaterial(InstanceId),
    /// During Play, keep every change to `id` after Stop (see `play::kept`).
    KeepPlayChanges(InstanceId),
    /// Apply, revert or unpack a placed prefab copy (Phase 49).
    Prefab(crate::prefab_menu::PrefabAction),
}

/// Changes `selection` for a click. The last id is the primary one (Properties
/// and handles). Ctrl or Cmd toggles. Shift, with `order` (the visible rows),
/// selects the range from the primary to the clicked row; without `order` it
/// adds. A plain click replaces the selection. `None` clears it, unless Ctrl
/// or Shift is held. `keep_group` is a right-click on a row already selected:
/// the others stay, and that row becomes primary.
pub fn update_selection(
    selection: &mut Vec<InstanceId>,
    clicked: Option<InstanceId>,
    modifiers: egui::Modifiers,
    order: Option<&[InstanceId]>,
    keep_group: bool,
) {
    let toggle = modifiers.command || modifiers.ctrl || modifiers.mac_cmd;
    match clicked {
        None if toggle || modifiers.shift => {}
        None => selection.clear(),
        Some(id) if keep_group && selection.contains(&id) => {
            selection.retain(|other| *other != id);
            selection.push(id);
        }
        Some(id) if toggle => {
            if let Some(index) = selection.iter().position(|other| *other == id) {
                selection.remove(index);
            } else {
                selection.push(id);
            }
        }
        Some(id) if modifiers.shift => {
            if let Some(order) = order
                && let Some(anchor) = selection.last().copied().or(Some(id))
                && let (Some(from), Some(to)) = (
                    order.iter().position(|row| *row == anchor),
                    order.iter().position(|row| *row == id),
                )
            {
                let (lo, hi) = (from.min(to), from.max(to));
                *selection = order[lo..=hi].to_vec();
            } else if !selection.contains(&id) {
                selection.push(id);
            }
            if selection.last() != Some(&id) && selection.contains(&id) {
                selection.retain(|other| *other != id);
                selection.push(id);
            }
        }
        Some(id) => *selection = vec![id],
    }
}

/// What happened in the Explorer this frame.
#[derive(Debug, Default, PartialEq)]
pub struct ExplorerOutput {
    /// A row that was clicked (with either button), to select.
    pub clicked: Option<InstanceId>,
    /// Ctrl, Shift and the other modifier keys held for [`clicked`](Self::clicked).
    pub modifiers: egui::Modifiers,
    /// A right-click on a row that is already selected should keep the others.
    pub keep_group: bool,
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
        // A UnionOperation's parts are kept inside it for Separate, out of
        // sight (Phase 50), and what a SplineMesh or SplineScatter makes is
        // made again from its settings (Phase 74).
        let union = instance.class() == ClassName::UnionOperation
            || axcenity_core::spline::is_generator(instance.class());
        rows.push(Row {
            id,
            name: instance.name().to_string(),
            class: instance.class(),
            depth,
            has_children: !union && !instance.children().is_empty(),
            expanded,
        });
        if union {
            return;
        }
        if expanded {
            for &child in instance.children() {
                self.add_rows(world, child, depth + 1, rows);
            }
        }
    }

    /// Draws the tree with every id in `selected` highlighted. Returns the row
    /// that was clicked, and anything to do to the tree. When not `editable`
    /// (during Play), rows can still be clicked and opened, and the
    /// right-click menu only has Keep changes, but there's no renaming or
    /// dragging. Ctrl (or Cmd) toggles a row
    /// and Shift selects the range; see [`update_selection`].
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        world: &World,
        selected: &[InstanceId],
        editable: bool,
    ) -> ExplorerOutput {
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
        let palette = theme::palette(ui.ctx());
        // Only scrolls up and down: long names end in "…" instead of
        // running past the panel.
        egui::ScrollArea::vertical()
            .auto_shrink(false)
            .show(ui, |ui| {
                // Rows touch, so their highlights join into one column.
                ui.spacing_mut().item_spacing.y = 0.0;
                let row_height = ui.spacing().interact_size.y;
                for row in self.rows(world) {
                    // Filled in once the row is drawn and its size is known, but
                    // painted behind it.
                    let background = ui.painter().add(egui::Shape::Noop);
                    let is_selected = selected.contains(&row.id);
                    let drawn = ui.horizontal(|ui| {
                        ui.set_min_height(row_height);
                        ui.add_space(4.0 + row.depth as f32 * INDENT);
                        let arrow_sense = if row.has_children {
                            egui::Sense::click()
                        } else {
                            egui::Sense::hover()
                        };
                        let (_, arrow) =
                            ui.allocate_exact_size(egui::Vec2::splat(ARROW_SIZE), arrow_sense);
                        if row.has_children {
                            let caret = if row.expanded {
                                icons::CARET_DOWN
                            } else {
                                icons::CARET_RIGHT
                            };
                            let color = if arrow.hovered() {
                                palette.text_strong
                            } else {
                                palette.text_weak
                            };
                            ui.painter().text(
                                arrow.rect.center(),
                                egui::Align2::CENTER_CENTER,
                                caret,
                                egui::FontId::proportional(11.0),
                                color,
                            );
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
                        let icon = egui::RichText::new(row_icon(world, &row))
                            .color(theme::class_color(row.class, ui.visuals()));
                        // The class (when the name doesn't already say it) and
                        // any warning keep their room after the name, unless the
                        // row is too narrow for them; the name ends in "…".
                        let warning = warning_badge(&self.warnings, world, &row);
                        let spacing = ui.spacing().item_spacing.x;
                        let class = (row.name != row.class.as_str()).then(|| {
                            ui.painter().layout_no_wrap(
                                row.class.as_str().to_owned(),
                                egui::FontId::proportional(theme::type_scale::SMALL),
                                palette.text_muted,
                            )
                        });
                        let badge_room = if warning.is_some() {
                            16.0 + spacing
                        } else {
                            0.0
                        };
                        let class_room = class.as_ref().map_or(0.0, |c| c.size().x + spacing);
                        let room = ui.available_width() - badge_room;
                        let class = class.filter(|_| room - class_room >= MIN_NAME_WIDTH);
                        let name_room = if class.is_some() {
                            room - class_room
                        } else {
                            room
                        };
                        // The row's background shows the selection and hover,
                        // so the label itself has no frame.
                        let label = ui
                            .scope(|ui| {
                                ui.set_max_width(name_room.max(ARROW_SIZE));
                                ui.add(
                                    egui::Button::new((icon, row.name.as_str()))
                                        .selected(is_selected)
                                        .frame(false)
                                        .truncate()
                                        .sense(egui::Sense::click_and_drag()),
                                )
                            })
                            .inner
                            .on_hover_text(format!("{}\n{}", row.name, row.class.as_str()));
                        #[cfg(test)]
                        self.drawn.push((row.id, label.rect));
                        // A right-click selects too, so the menu acts on what's selected.
                        if label.secondary_clicked() {
                            output.clicked = Some(row.id);
                            output.keep_group = true;
                        } else if label.clicked() {
                            output.clicked = Some(row.id);
                            output.modifiers = ui.input(|input| input.modifiers);
                        }
                        if scroll_to == Some(row.id) {
                            label.scroll_to_me(Some(egui::Align::Center));
                        }
                        if let Some(class) = class {
                            ui.label(class);
                        }
                        if row.has_children && label.double_clicked() {
                            toggled = Some(row.id);
                        }
                        if let Some(text) = warning {
                            ui.colored_label(ui.visuals().warn_fg_color, icons::WARNING)
                                .on_hover_text(text);
                        }
                        if world.is_locked(row.id) {
                            ui.weak(icons::LOCK)
                                .on_hover_text("Locked: the viewport can't select or change it.");
                        }
                        if row.class.is_script() && label.double_clicked() {
                            output.open = Some(row.id);
                        }

                        if !editable {
                            label.context_menu(|ui| {
                                if play_row_menu(ui, &row.name) {
                                    output.action = Some(Action::KeepPlayChanges(row.id));
                                }
                            });
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
                    // The row's full width: the selection or a hover fill, and
                    // a faint guide for each level it's nested in.
                    let clip = ui.clip_rect();
                    let row_rect =
                        egui::Rect::from_x_y_ranges(clip.x_range(), drawn.response.rect.y_range());
                    let hovered = ui.rect_contains_pointer(row_rect) && dragged.is_none();
                    let hover =
                        theme::ease(ui.ctx(), egui::Id::new(("explorer row", row.id)), hovered);
                    let fill = if is_selected {
                        palette.accent_soft
                    } else {
                        palette.control.gamma_multiply(hover)
                    };
                    ui.painter().set(
                        background,
                        egui::Shape::rect_filled(row_rect.shrink2(egui::vec2(2.0, 0.0)), 4, fill),
                    );
                    let left = drawn.response.rect.left() + 4.0;
                    for depth in 1..=row.depth {
                        let x = left + (depth - 1) as f32 * INDENT + ARROW_SIZE / 2.0;
                        ui.painter().vline(
                            x.round() + 0.5,
                            row_rect.y_range(),
                            egui::Stroke::new(1.0_f32, palette.border),
                        );
                    }
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

/// The icon in front of `row`'s name: its class's, or for a part its shape's,
/// and an open folder for an open Folder.
fn row_icon(world: &World, row: &Row) -> &'static str {
    match row.class {
        ClassName::Part => match world.get(row.id).and_then(|part| part.property("Shape")) {
            Some(&Value::PartType(shape)) => theme::part_icon(shape),
            _ => theme::class_icon(row.class),
        },
        ClassName::Folder if row.expanded => icons::FOLDER_OPEN,
        class => theme::class_icon(class),
    }
}

/// The hover text of `row`'s warning badge, if it has one: its own warnings, or,
/// on a closed branch, a pointer to what's inside with warnings (scripts,
/// or out-of-date light probe volumes), so they aren't hidden.
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
        .filter_map(|&id| world.get(id).map(|inside| inside.name()))
        .collect();
    inside.sort_unstable();
    (!inside.is_empty()).then(|| format!("Open this to see the warnings of {}.", inside.join(", ")))
}

/// The text box a row turns into while it's renamed. Returns `None` while
/// typing, then `Some(Some(name))` to rename or `Some(None)` to cancel.
fn rename_box(ui: &mut egui::Ui, renaming: &mut Renaming) -> Option<Option<String>> {
    let text_box = ui.add(
        egui::TextEdit::singleline(&mut renaming.text)
            .desired_width(ui.available_width().min(160.0)),
    );
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

/// The right-click menu of `name`'s row during Play. True when Keep
/// changes was picked.
fn play_row_menu(ui: &mut egui::Ui, name: &str) -> bool {
    ui.button(theme::with_icon(
        icons::PUSH_PIN,
        &format!("Keep changes to {name}"),
    ))
    .on_hover_text(
        "At Stop, copy this object's values as the game has them into the world you're \
         editing, as one undo step. Other changes made during Play are still undone.",
    )
    .clicked()
}

/// A choice in a row's right-click menu.
#[derive(Clone, Debug, PartialEq)]
enum RowCommand {
    Insert(Insertable),
    InsertScript(ClassName),
    /// Open a script in a tab.
    Open,
    /// Animate a Part, Model or UI element (see [`Action::Animate`]).
    Animate,
    /// Export it as a prefab in an asset package.
    ExportPackage,
    /// Save a Part's look as a material file.
    SaveMaterial,
    /// From the Prefab ▸ submenu of a placed copy.
    Prefab(crate::prefab_menu::PrefabAction),
    Rename,
    Duplicate,
    Delete,
    /// Turn `Locked` on or off.
    SetLocked(bool),
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
            RowCommand::Animate => Some(Action::Animate(id)),
            RowCommand::ExportPackage => Some(Action::ExportPackage(id)),
            RowCommand::SaveMaterial => Some(Action::SaveMaterial(id)),
            RowCommand::Prefab(action) => Some(Action::Prefab(action)),
            RowCommand::Duplicate => Some(Action::Duplicate(id)),
            RowCommand::Delete => Some(Action::Delete(id)),
            RowCommand::SetLocked(on) => Some(Action::SetLocked(id, on)),
        }
    }
}

/// The right-click menu of `id`'s row: Open (for scripts) or Animate (for
/// what can animate), Insert ▸, then Rename, Duplicate and Delete, greyed
/// out (with the reason on hover) where they can't be done.
fn row_menu(ui: &mut egui::Ui, world: &World, id: InstanceId) -> Option<RowCommand> {
    let mut chosen = None;
    let is_script = world
        .get(id)
        .is_some_and(|instance| instance.class().is_script());
    if is_script
        && ui
            .button(theme::with_icon(icons::ARROW_SQUARE_OUT, "Open"))
            .clicked()
    {
        chosen = Some(RowCommand::Open);
    }
    let can_animate = world
        .get(id)
        .is_some_and(|instance| crate::animation::can_animate(instance.class()));
    if can_animate {
        let icon = theme::AnimKind::Animator.rich_icon(ui.visuals());
        if ui
            .button((icon, "  Animate"))
            .on_hover_text(
                "Give it an Animator, a new clip and an Animation playing it, and open the clip with Record on",
            )
            .clicked()
        {
            chosen = Some(RowCommand::Animate);
        }
    }
    ui.menu_button(theme::with_icon(icons::PLUS, "Insert"), |ui| {
        // Buttons for `items`, noting the one clicked in `chosen`.
        let items = |ui: &mut egui::Ui,
                     chosen: &mut Option<RowCommand>,
                     items: &mut dyn Iterator<Item = Insertable>| {
            for what in items {
                if ui
                    .button(insert_item(ui, what))
                    .on_hover_text(insert_hint(what))
                    .clicked()
                {
                    *chosen = Some(RowCommand::Insert(what));
                }
            }
        };
        // Parts and groups are used most, so they're first and one click
        // away; everything else is in a submenu by kind.
        items(ui, &mut chosen, &mut Insertable::PARTS.into_iter());
        ui.separator();
        items(
            ui,
            &mut chosen,
            &mut Insertable::GROUPS
                .into_iter()
                .chain(Insertable::CONSTRAINTS),
        );
        ui.separator();
        ui.menu_button(theme::with_icon(icons::SUN, "Lighting & Sky"), |ui| {
            items(
                ui,
                &mut chosen,
                &mut Insertable::LIGHTS
                    .into_iter()
                    .chain(Insertable::PROBES)
                    .chain(Insertable::SKY),
            );
            ui.separator();
            items(ui, &mut chosen, &mut Insertable::LENS.into_iter());
            ui.separator();
            items(ui, &mut chosen, &mut Insertable::VOLUMES.into_iter());
        });
        ui.menu_button(theme::with_icon(icons::SPARKLE, "Effects"), |ui| {
            items(
                ui,
                &mut chosen,
                &mut Insertable::EFFECTS
                    .into_iter()
                    .chain(Insertable::BREAKING)
                    .chain(Insertable::ANIMATION),
            );
        });
        ui.menu_button(theme::with_icon(icons::SPEAKER_HIGH, "Audio"), |ui| {
            items(ui, &mut chosen, &mut Insertable::AUDIO.into_iter());
            ui.menu_button("Sound effect", |ui| {
                items(ui, &mut chosen, &mut Insertable::SOUND_EFFECTS.into_iter());
            });
        });
        ui.menu_button(theme::with_icon(icons::GAME_CONTROLLER, "Gameplay"), |ui| {
            items(
                ui,
                &mut chosen,
                &mut Insertable::HUMANOIDS
                    .into_iter()
                    .chain(Insertable::CHARACTERS)
                    .chain(Insertable::HEALTH)
                    .chain(Insertable::INTERACTION)
                    .chain(Insertable::SEATS),
            );
            ui.separator();
            items(
                ui,
                &mut chosen,
                &mut Insertable::SPAWNING
                    .into_iter()
                    .chain(Insertable::TOOLS)
                    .chain(Insertable::VEHICLES)
                    .chain(Insertable::AGENTS)
                    .chain(Insertable::SPLINES)
                    .chain(Insertable::CHECKPOINTS)
                    .chain(Insertable::CAMERAS),
            );
        });
        ui.menu_button(theme::with_icon(icons::LAYOUT, "UI"), |ui| {
            items(ui, &mut chosen, &mut Insertable::UI.into_iter());
        });
        ui.menu_button(theme::with_icon(icons::HASH, "Value"), |ui| {
            items(ui, &mut chosen, &mut Insertable::VALUES.into_iter());
        });
        ui.separator();
        for class in SCRIPT_CLASSES {
            let icon = egui::RichText::new(theme::class_icon(class))
                .color(theme::class_color(class, ui.visuals()));
            if ui
                .button((icon, class.as_str()))
                .on_hover_text(script_hover(class))
                .clicked()
            {
                chosen = Some(RowCommand::InsertScript(class));
            }
        }
        ui.menu_button(theme::with_icon(icons::BROADCAST, "Events"), |ui| {
            let remote = Insertable::RemoteEvent;
            if ui
                .button(insert_item(ui, remote))
                .on_hover_text(REMOTE_EVENT_HOVER)
                .clicked()
            {
                chosen = Some(RowCommand::Insert(remote));
            }
            for what in Insertable::MESSENGERS {
                if ui
                    .button(insert_item(ui, what))
                    .on_hover_text(messenger_hover(what.class()))
                    .clicked()
                {
                    chosen = Some(RowCommand::Insert(what));
                }
            }
        });
    });
    if ui
        .add_enabled(
            tree::can_be_prefab(world, id),
            egui::Button::new(theme::with_icon(icons::PACKAGE, "Export Package…")),
        )
        .on_hover_text(
            "Save it as a prefab, with every model, texture, sound and clip it uses, in one \
             .axpkg file to import into another project.",
        )
        .on_disabled_hover_text("Only parts, Models and Folders in the Workspace can be exported.")
        .clicked()
    {
        chosen = Some(RowCommand::ExportPackage);
    }
    let is_part = world
        .get(id)
        .is_some_and(|instance| instance.class().is_part());
    if is_part
        && ui
            .button(theme::with_icon(icons::SPHERE, "Save as Material"))
            .on_hover_text(
                "Save its color, material and SurfaceAppearance as a material file in \
                 assets/materials, to drag onto other parts.",
            )
            .clicked()
    {
        chosen = Some(RowCommand::SaveMaterial);
    }
    if let Some(action) = crate::prefab_menu::row_menu(ui, world, id) {
        chosen = Some(RowCommand::Prefab(action));
    }
    ui.separator();

    let renamable = world.get(id).is_some_and(properties::can_rename);
    let rename = egui::Button::new(theme::with_icon(icons::PENCIL_SIMPLE, "Rename"))
        .shortcut_text(RENAME_KEY.name());
    if ui
        .add_enabled(renamable, rename)
        .on_disabled_hover_text("Services keep their names.")
        .clicked()
    {
        chosen = Some(RowCommand::Rename);
    }
    if let Some(instance) = world.get(id).filter(|i| i.class().can_lock()) {
        let locked = instance.property("Locked") == Some(&Value::Bool(true));
        let (icon, label, hint) = if locked {
            (
                icons::LOCK_OPEN,
                "Unlock",
                "Let the viewport select and change it again.",
            )
        } else {
            (
                icons::LOCK,
                "Lock",
                "Stop the viewport selecting or changing it. It stays here in the Explorer.",
            )
        };
        if ui
            .button(theme::with_icon(icon, label))
            .on_hover_text(hint)
            .clicked()
        {
            chosen = Some(RowCommand::SetLocked(!locked));
        }
    }
    let editable = tree::check_editable(world, id);
    let reason = editable.err().unwrap_or_default();
    let duplicate = egui::Button::new(theme::with_icon(icons::COPY, "Duplicate"))
        .shortcut_text(ui.ctx().format_shortcut(&DUPLICATE_SHORTCUT));
    if ui
        .add_enabled(editable.is_ok(), duplicate)
        .on_disabled_hover_text(reason)
        .clicked()
    {
        chosen = Some(RowCommand::Duplicate);
    }
    let deletable = tree::check_delete(world, id);
    let delete =
        egui::Button::new(theme::with_icon(icons::TRASH, "Delete")).shortcut_text(DELETE_KEY_TEXT);
    if ui
        .add_enabled(deletable.is_ok(), delete)
        .on_disabled_hover_text(deletable.err().unwrap_or_default())
        .clicked()
    {
        chosen = Some(RowCommand::Delete);
    }
    chosen
}

/// An Insert menu item: the icon, in its class's colour, then the name.
pub(crate) fn insert_item(ui: &egui::Ui, what: Insertable) -> (egui::RichText, &'static str) {
    let icon = egui::RichText::new(theme::insert_icon(what))
        .color(theme::class_color(what.class(), ui.visuals()));
    (icon, what.label())
}

#[cfg(test)]
mod tests;

/// What inserting `what` does, for its tooltip in the ribbon's menus and
/// the Explorer's Insert menu alike.
pub fn insert_hint(what: Insertable) -> String {
    let text = match what {
        Insertable::Part(_) => {
            return format!(
                "Insert a {} part at the camera's focus point, inside the selected Model or \
                 Folder if there is one.",
                what.label().to_lowercase()
            );
        }
        Insertable::Sound => "Insert a Sound into the selected part or the Workspace.",
        Insertable::SoundGroup => {
            "Insert a SoundGroup (an audio bus) into the selected SoundGroup or SoundService. \
             Set a Sound's SoundGroup to play it through one."
        }
        Insertable::ReverbZone => {
            "Insert a Reverb Zone: while the camera is inside it, spatial Sounds take on its \
             reverb."
        }
        Insertable::Decal => {
            "Insert a Decal into the selected part: a picture on one of its faces (Face). \
             Pick its Texture, and set StudsPerTileU and StudsPerTileV to repeat it."
        }
        Insertable::ProjectedDecal => {
            "Insert a Projected Decal at the camera's focus point: a box that lays its picture \
             on whatever is inside it, following its shape, for puddles, road markings and dirt. \
             Characters walking through stay bare (IgnoreCharacters)."
        }
        Insertable::ParticleEmitter => {
            "Insert a ParticleEmitter into the selected part. With no part selected it stands \
             on its own at the camera's focus point, shown as an icon you can drag."
        }
        Insertable::LensEffect(ClassName::DepthOfFieldEffect) => {
            "Insert a DepthOfFieldEffect into Lighting: what's nearer or farther than its \
             FocusDistance blurs, as through a camera's lens."
        }
        Insertable::LensEffect(ClassName::VignetteEffect) => {
            "Insert a VignetteEffect into Lighting: the picture darkens (or takes its Color) \
             toward the corners."
        }
        Insertable::LensEffect(ClassName::FilmGrainEffect) => {
            "Insert a FilmGrainEffect into Lighting: moving noise over the picture, as on film."
        }
        Insertable::LensEffect(ClassName::ChromaticAberrationEffect) => {
            "Insert a ChromaticAberrationEffect into Lighting: red and blue drift apart toward \
             the picture's edges."
        }
        Insertable::LensEffect(ClassName::MotionBlurEffect) => {
            "Insert a MotionBlurEffect into Lighting: what moves across the view streaks along \
             its motion, as through a camera's shutter."
        }
        Insertable::LensEffect(_) => {
            "Insert a SharpenEffect into Lighting: crisper edges between colors, useful with a \
             render scale below 100%."
        }
        Insertable::GraphicsVolume => {
            "Insert a Graphics Volume: an invisible box that changes the look while the camera \
             is inside it, fading in over its BlendDistance. Put Exposure, Fog, Bloom, Color \
             Correction or an Atmosphere inside it; where volumes overlap, the higher Priority \
             wins."
        }
        Insertable::ExposureEffect => {
            "Insert an ExposureEffect into the selected Graphics Volume (or a new one): the \
             exposure inside it, in place of Lighting's."
        }
        Insertable::FogEffect => {
            "Insert a FogEffect into the selected Graphics Volume (or a new one): distance fog \
             inside it, in place of the world's."
        }
        Insertable::BloomEffect => {
            "Insert a BloomEffect into the selected Graphics Volume (or a new one): how bright \
             light blooms inside it, in place of Lighting's bloom."
        }
        Insertable::PointLight | Insertable::SpotLight => {
            "Insert a light into the selected part. With no part selected it stands on its own \
             at the camera's focus point, as in Unreal: no lamp, just an icon you can drag."
        }
        Insertable::RectLight => {
            "Insert a RectLight: light from a rectangle, out of its front only, like a window \
             or a softbox. In a part it sits on the side it faces; with no part selected it \
             stands on its own at the camera's focus point."
        }
        Insertable::ReflectionProbe => {
            "Insert a ReflectionProbe into the selected part: shiny things in the part's box \
             reflect what's around them there, once baked. With no part selected, an invisible \
             room-sized box is made for it."
        }
        Insertable::LightProbeVolume => {
            "Insert a LightProbeVolume into the selected part: a grid of probes in the part's \
             box lights everything there, still or moving, with the light bounced around it, \
             once baked. With no part selected, an invisible room-sized box is made for it."
        }
        Insertable::Clouds => {
            "Insert Clouds into Lighting: a layer of cloud high in the sky, lit by the sun, \
             whose shadows fall on the world. Workspace's GlobalWind carries them along."
        }
        Insertable::OutlineEffect => {
            "Insert an OutlineEffect into Lighting: lines along the edges of everything solid, \
             around silhouettes and along creases, with no setup per part. A material file's \
             outline draws one around its parts alone."
        }
        Insertable::ColorCorrectionEffect => {
            "Insert a ColorCorrectionEffect into Lighting (or the selected Graphics Volume): \
             changes the finished picture's colors (brightness, contrast, saturation and a \
             tint), and can look them up in a .cube table, posterize them or snap them to a \
             palette."
        }
        Insertable::Weather => {
            "Insert Weather into Lighting: rain or snow falling around the camera, wetting \
             surfaces or covering their tops in snow. Changes ease in over its TransitionTime, \
             and Workspace's GlobalWind blows the rain and snow."
        }
        Insertable::Atmosphere => {
            "Insert an Atmosphere into Lighting: the sky is drawn as sunlight scattered by air, \
             reddening toward sunset and darkening at night as Lighting's ClockTime goes round, \
             and it colors the sun's light."
        }
        Insertable::ThirdPersonController | Insertable::FirstPersonController => {
            return format!(
                "Insert a {} on the selected model or part. With nothing selected, or on a \
                 character that already has a controller, makes a simple character.",
                what.class()
            );
        }
        Insertable::Health => {
            "Insert a Health into the selected character's Model: how much it can take. \
             Scripts call TakeDamage and Heal, and it fires Died once at 0. Turn on FallDamage \
             to hurt it when it lands hard."
        }
        Insertable::Ability => {
            "Insert an Ability into the selected character's Model: something it can do. Set \
             its Cooldown, its cost (CostAttribute and CostAmount, such as Mana 20), the input \
             action that activates it and the tags that block it; a Script on its Activated \
             does the rest. The ability bar shows it during Play."
        }
        Insertable::Effect => {
            "Insert an Effect: a change to an attribute, Health or Humanoid property of the \
             Model it's in, at once, for a Duration or every Period. Keep it in \
             ReplicatedStorage or ServerStorage as a template and put copies on with \
             model:ApplyEffect(effect)."
        }
        Insertable::DamageZone => {
            "Insert a Damage Zone: a see-through part that takes DamagePerSecond from every \
             Health whose Model is inside it during Play."
        }
        Insertable::Seat => {
            "Insert a Seat: a part a character sits on when it walks into it during Play. Jump \
             stands it up."
        }
        Insertable::ProximityPrompt => {
            "Insert a Prompt into the selected part or Attachment. During Play, walking up to \
             it shows ActionText with the Interact key, and holding that for HoldDuration fires \
             Triggered."
        }
        Insertable::ClickDetector => {
            "Insert a Click Detector into the selected part. During Play, clicking the part \
             fires MouseClick, and the cursor changes over it."
        }
        Insertable::Character => {
            "Insert a Character: six welded parts with a Humanoid, a third-person controller, \
             Health, Inventory and an Animator with Idle, Walk, Run, Jump, Fall and Sit states."
        }
        Insertable::Humanoid => {
            "Insert a Humanoid into the selected character's Model: Roblox-style properties \
             and methods over its controller, Health, Inventory and Animator."
        }
        Insertable::SpawnLocation => {
            "Insert a SpawnLocation: a pad players' characters spawn on during Play, and \
             respawn on after dying. Turn Neutral off and set its TeamColor to keep it for one \
             team."
        }
        Insertable::Team => {
            "Insert a Team into Teams: players join the AutoAssignable team with the fewest \
             players, and spawn on SpawnLocations of its TeamColor."
        }
        Insertable::Tool => {
            "Insert a Tool: a Handle part with a Grip. During Play, a character with an \
             Inventory picks it up by touching it, number keys equip it in the right hand, and \
             clicking fires Activated."
        }
        Insertable::Inventory => {
            "Insert an Inventory into the selected character's Model: the Tools it carries, \
             shown in the hotbar during Play."
        }
        Insertable::Vehicle => {
            "Insert a ready-to-drive car. During Play, walk into its seat on the left to drive \
             with the move keys or a stick; Jump gets out."
        }
        Insertable::VehicleSeat => {
            "Insert a Vehicle Seat: weld it to a car's chassis to drive it, on wheels inserted \
             into the chassis."
        }
        Insertable::VehicleWheel => {
            "Insert a Wheel into the selected part: a spring and tyre that a Vehicle Seat \
             welded to that part drives on."
        }
        Insertable::NavigationAgent => {
            "Insert a NavigationAgent on the selected model or part, which walks it along the \
             navigation mesh to its Destination during Play. With nothing selected, makes a \
             simple character for it."
        }
        Insertable::Spline => {
            "Insert a Spline in front of you: a curve through its points, for a Spline Follower \
             to carry a part along, an agent to patrol or a cutscene camera to ride. Click it to \
             drag its points; Alt-drag a point to add one after it."
        }
        Insertable::SplineFollower => {
            "Insert a Spline Follower into the selected part or model: during Play it carries \
             it along its Spline at Speed, there and back by default. Characters standing on \
             it ride along."
        }
        Insertable::SplineMesh => {
            "Insert a Spline Mesh into the selected Spline (or a new one): it bends a model \
             along the curve, repeated to fill it, for roads, pipes, fences and rails. Click a \
             model in the Assets panel while it's selected to use it."
        }
        Insertable::SplineScatter => {
            "Insert a Spline Scatter into the selected Spline (or a new one): it places models \
             along the curve, or inside a closed one, from a seed. Click models in the Assets \
             panel while it's selected to add them."
        }
        Insertable::Checkpoint => {
            "Insert a Checkpoint on top of the selected part: a place \
             SaveService:RestoreCheckpoint (or the \"go back to checkpoint\" block) puts the \
             player character back at."
        }
        Insertable::Camera => {
            "Insert a Camera where you're looking from: a named viewpoint a LocalScript cuts to \
             with workspace.CurrentCamera.CFrame = viewpoint.CFrame. It isn't drawn in the game."
        }
        Insertable::Attachment => {
            "Insert an Attachment in the selected part, at its middle: a point and direction on \
             the part that moves with it."
        }
        Insertable::ScreenGui => "Insert a ScreenGui: a layer of UI over the game.",
        Insertable::PageLayout => {
            "Insert a PageLayout. Its child Frames are pages; a script or the show-page block \
             sets CurrentPage."
        }
        Insertable::Page => {
            "Insert a page Frame into the selected PageLayout (a PageLayout and a ScreenGui are \
             made if there isn't one)."
        }
        Insertable::BillboardGui => {
            "Insert a BillboardGui into the selected part. It floats in the world and is drawn \
             with the screen UI."
        }
        Insertable::HealthBar => {
            "Insert health bars: one floating over the nearest character with a Health, and \
             one at the top left of the screen. Both follow that Health with no script, styled \
             by health-bar.css."
        }
        _ if Insertable::UI.contains(&what) => {
            return format!(
                "Insert a {} into the selected ScreenGui or Frame (or the first ScreenGui; one is \
                 made if there's none). Drag its handles in the viewport, and style it with CSS \
                 in the Style section of Properties.",
                what.label()
            );
        }
        _ => return format!("Insert a {}.", what.label()),
    };
    text.to_owned()
}
