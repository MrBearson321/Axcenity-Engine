//! The node menu: every node in the library by category, with a search
//! box. Opened from a wire, it offers only the nodes that wire fits, and
//! the new node is wired to it.

use super::*;

/// The open node menu.
#[derive(Clone, Debug)]
pub(super) struct NodeMenu {
    /// Where it opened, on screen and in the graph (where the node goes).
    pub(super) screen: Pos2,
    pub(super) graph: Pos2,
    /// The pin a wire was dropped from (an output when true), to wire the
    /// new node to.
    pub(super) from: Option<(PinRef, bool)>,
    search: String,
    /// The menu opened this frame: give the search box focus.
    fresh: bool,
}

impl NodeMenu {
    pub(super) fn new(screen: Pos2, graph: Pos2, from: Option<(PinRef, bool)>) -> Self {
        Self {
            screen,
            graph,
            from,
            search: String::new(),
            fresh: true,
        }
    }
}

pub(super) enum MenuChoice {
    Open,
    Close,
    Pick(usize),
}

/// The pin of a node of `spec` that a wire from `from` would join: the
/// first input that fits an output, or the first output that fits an input.
fn fitting_pin(spec: &NodeSpec, from_type: PinType, from_output: bool) -> Option<&'static str> {
    let pins = if from_output {
        spec.inputs
    } else {
        spec.outputs
    };
    pins.iter()
        .find(|pin| {
            if from_output {
                pin.ty.accepts(from_type)
            } else {
                from_type.accepts(pin.ty)
            }
        })
        .map(|pin| pin.name)
}

/// What the menu offers for a wire from `from` (everything, without one),
/// in the library's order.
pub(super) fn fitting<'a>(
    graph: &NodeGraph,
    library: &'a dyn NodeLibrary,
    from: Option<&(PinRef, bool)>,
) -> Vec<&'a NodeSpec> {
    let from_type = from.and_then(|(pin, output)| graph.pin_type(library, pin, *output));
    library
        .specs()
        .iter()
        .filter(|spec| match (from, from_type) {
            (None, _) => true,
            (Some((_, output)), Some(ty)) => fitting_pin(spec, ty, *output).is_some(),
            (Some(_), None) => false,
        })
        .collect()
}

/// Wires the node `id` just made from the menu to the pin its wire came from.
pub(super) fn wire_new(
    graph: &mut NodeGraph,
    library: &dyn NodeLibrary,
    from: &PinRef,
    output: bool,
    id: u32,
) -> Result<(), &'static str> {
    let ty = graph
        .pin_type(library, from, output)
        .ok_or("That node is gone.")?;
    let spec = library
        .spec(&graph.node(id).ok_or("That node is gone.")?.kind)
        .ok_or("That node is gone.")?;
    let pin = fitting_pin(spec, ty, output).ok_or("It has no pin this wire fits.")?;
    if output {
        graph.connect(library, from.clone(), PinRef::new(id, pin))
    } else {
        graph.connect(library, PinRef::new(id, pin), from.clone())
    }
}

/// Draws the menu. Escape or a click outside closes it; Enter picks the
/// first match.
pub(super) fn show(
    ui: &mut egui::Ui,
    menu: &mut NodeMenu,
    specs: &[&NodeSpec],
    key: &str,
) -> MenuChoice {
    let mut choice = MenuChoice::Open;
    let search = menu.search.to_lowercase();
    let matches: Vec<usize> = specs
        .iter()
        .enumerate()
        .filter(|(_, spec)| {
            search.is_empty()
                || spec.title.to_lowercase().contains(&search)
                || spec.category.to_lowercase().contains(&search)
        })
        .map(|(index, _)| index)
        .collect();
    let area = egui::Area::new(egui::Id::new(("node graph menu", key)))
        .order(egui::Order::Foreground)
        .fixed_pos(menu.screen)
        .constrain(true)
        .show(ui.ctx(), |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                ui.set_width(240.0);
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
                            let spec = specs[index];
                            if shown != Some(spec.category) {
                                shown = Some(spec.category);
                                ui.label(
                                    egui::RichText::new(spec.category)
                                        .color(category_color(spec.category))
                                        .strong()
                                        .size(theme::type_scale::SMALL),
                                );
                            }
                            if ui
                                .selectable_label(false, spec.title)
                                .on_hover_text(spec.help)
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
