//! The node menu: every palette block and reporter by category, with a
//! search box. Opened from a wire, it offers only the nodes that wire fits,
//! and the new node is wired to it.

use super::*;

/// The open node menu.
#[derive(Clone, Debug)]
pub(super) struct Menu {
    /// Where it opened, on screen and in the graph (where the node goes).
    pub(super) screen: Pos2,
    pub(super) graph: Pos2,
    /// The pin a wire was dropped from, to wire the new node to.
    pub(super) from: Option<Pin>,
    search: String,
    /// The menu opened this frame: give the search box focus.
    fresh: bool,
}

impl Menu {
    pub(super) fn new(screen: Pos2, graph: Pos2, from: Option<Pin>) -> Self {
        Self {
            screen,
            graph,
            from,
            search: String::new(),
            fresh: true,
        }
    }
}

pub(super) struct MenuItem {
    pub(super) category: Category,
    pub(super) title: String,
    pub(super) item: PaletteItem,
    /// What the search box finds it by: its title, and the words and Lua
    /// the Blocks palette's search uses.
    words: String,
}

pub(super) enum MenuChoice {
    Open,
    Close,
    Pick(usize),
}

/// Whether a node made from `item` can be wired to `from`.
fn fits(script: &BlockScript, item: &PaletteItem, from: Option<&Pin>) -> bool {
    match (from, item) {
        (_, PaletteItem::NewVariable) => false,
        (None, _) => true,
        (Some(Pin::ExecOut(..)), PaletteItem::Block(kind)) => !kind.is_event(),
        (Some(Pin::ExecIn(_)), PaletteItem::Block(kind)) => !exec_outs(kind).is_empty(),
        (Some(Pin::Input(at)), PaletteItem::Reporter(expression)) => {
            slot_kind(script, at).is_some_and(|kind| kind.accepts(expression.shape()))
        }
        (Some(Pin::Output(at)), item) => {
            let Some(shape) = value(script, at).map(Expression::shape) else {
                return false;
            };
            let slots = match item {
                PaletteItem::Block(kind) => block_slots(kind),
                PaletteItem::Reporter(expression) => value_slots(expression),
                PaletteItem::NewVariable => Vec::new(),
            };
            slots.iter().any(|kind| kind.accepts(shape))
        }
        _ => false,
    }
}

/// What the menu offers, in palette order.
pub(super) fn menu_items(
    script: &BlockScript,
    from: Option<&Pin>,
    context: &BlockContext,
    variables: &[String],
) -> Vec<MenuItem> {
    let mut items = Vec::new();
    for category in Category::ALL {
        for item in palette(category, context.client, context.objects, variables) {
            if !fits(script, &item, from) {
                continue;
            }
            let title = match &item {
                PaletteItem::Block(kind) => block_names(kind).0.to_owned(),
                PaletteItem::Reporter(expression) => value_names(expression).0,
                PaletteItem::NewVariable => continue,
            };
            let words = format!(
                "{} {}",
                title.to_lowercase(),
                search_text(&item, category, context.objects)
            );
            items.push(MenuItem {
                category,
                title,
                item,
                words,
            });
        }
    }
    items
}

/// Draws the menu. Escape or a click outside closes it; Enter picks the
/// first match.
pub(super) fn show_menu(
    ui: &mut egui::Ui,
    menu: &mut Menu,
    items: &[MenuItem],
    key: &str,
) -> MenuChoice {
    let mut choice = MenuChoice::Open;
    let matches: Vec<usize> = items
        .iter()
        .enumerate()
        .filter(|(_, item)| matches_search(&item.words, &menu.search))
        .map(|(index, _)| index)
        .collect();
    let area = egui::Area::new(egui::Id::new(("graph menu", key)))
        .order(egui::Order::Foreground)
        .fixed_pos(menu.screen)
        .constrain(true)
        .show(ui.ctx(), |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                ui.set_width(250.0);
                let title = if menu.from.is_some() {
                    "Nodes for this wire"
                } else {
                    "Add a node"
                };
                ui.label(egui::RichText::new(title).strong());
                let search_box = ui.add(
                    egui::TextEdit::singleline(&mut menu.search)
                        .hint_text(theme::with_icon(icons::MAGNIFYING_GLASS, "Search"))
                        .desired_width(f32::INFINITY),
                );
                if menu.fresh {
                    search_box.request_focus();
                }
                if search_box.lost_focus()
                    && ui.input(|i| i.key_pressed(egui::Key::Enter))
                    && let Some(first) = matches.first()
                {
                    choice = MenuChoice::Pick(*first);
                }
                ui.separator();
                egui::ScrollArea::vertical()
                    .max_height(340.0)
                    .show(ui, |ui| {
                        if matches.is_empty() {
                            ui.weak("No nodes match.");
                        }
                        let mut shown = None;
                        for &index in &matches {
                            let item = &items[index];
                            if shown != Some(item.category) {
                                shown = Some(item.category);
                                ui.label(
                                    egui::RichText::new(item.category.name())
                                        .color(item.category.fill())
                                        .strong()
                                        .size(theme::type_scale::SMALL),
                                );
                            }
                            if ui
                                .selectable_label(false, &item.title)
                                .on_hover_ui(|ui| {
                                    lua_hint(ui, "Click to add it.", item_lua(&item.item))
                                })
                                .clicked()
                            {
                                choice = MenuChoice::Pick(index);
                            }
                        }
                    });
            });
        });
    let fresh = std::mem::replace(&mut menu.fresh, false);
    let (escape, pressed, pointer) = ui.input(|i| {
        (
            i.key_pressed(egui::Key::Escape),
            i.pointer.any_pressed(),
            i.pointer.interact_pos(),
        )
    });
    let outside = pressed && pointer.is_some_and(|pointer| !area.response.rect.contains(pointer));
    if matches!(choice, MenuChoice::Open) && (escape || (outside && !fresh)) {
        choice = MenuChoice::Close;
    }
    choice
}
