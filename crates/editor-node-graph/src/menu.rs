//! The node menu: every node in the library by category, with a search
//! box. Opened from a wire, it offers only the nodes that wire fits, and
//! the new node is wired to it.

use super::*;
use crate::canvas::{MenuEntry, SearchMenu};

/// The open node menu.
#[derive(Clone, Debug)]
pub(super) struct NodeMenu {
    pub(super) menu: SearchMenu,
    /// The pin a wire was dropped from (an output when true), to wire the
    /// new node to.
    pub(super) from: Option<(PinRef, bool)>,
}

impl NodeMenu {
    pub(super) fn new(screen: Pos2, graph: Pos2, from: Option<(PinRef, bool)>) -> Self {
        Self {
            menu: SearchMenu::new(screen, graph),
            from,
        }
    }
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
    let words: Vec<String> = specs
        .iter()
        .map(|spec| format!("{} {}", spec.title, spec.category).to_lowercase())
        .collect();
    let entries: Vec<MenuEntry> = specs
        .iter()
        .zip(&words)
        .map(|(spec, words)| MenuEntry {
            group: spec.category,
            color: category_color(spec.category),
            title: spec.title,
            words,
        })
        .collect();
    let title = if menu.from.is_some() {
        "Nodes for this wire"
    } else {
        "Add a node"
    };
    menu.menu.show(
        ui.ctx(),
        egui::Id::new(("node graph menu", key)),
        title,
        &entries,
        Some(&|ui: &mut egui::Ui, index: usize| {
            ui.label(specs[index].help);
        }),
    )
}
