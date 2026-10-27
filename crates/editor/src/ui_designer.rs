//! The UI Designer tab: one ScreenGui on an artboard, like a design tool's
//! frame, with zoom, pan and rulers.
//!
//! The ScreenGui is designed at `DesignWidth` × `DesignHeight`, and the
//! artboard previews it on any screen: the design size, or a preset such
//! as an ultrawide monitor or a phone. A ScreenGui in Scale mode (the
//! default for new ones) is scaled evenly to each screen and laid out over
//! all of it, so with its elements pinned to the right edges and corners
//! (the anchor picker) it looks the same everywhere.
//!
//! Around the artboard are a palette of UI elements to drag onto it, the
//! ScreenGui's hierarchy, and an inspector with the element's name, text,
//! anchor and font, the states editor (Normal, Hover, Pressed and Disabled,
//! `states.rs`) and the same Style (CSS) section as Properties. The
//! selected element gets the viewport's handles, snapping and distances in
//! pixels (`ui_handles.rs`), and also snaps to guides dragged out of the
//! rulers. Double-click text on the artboard to type over it. Several
//! elements can be selected (Shift- and Ctrl-click, or a box dragged on
//! empty artboard), and get one box of handles around them all
//! (`group.rs`); the arrange bar aligns, distributes, copies and pastes
//! styles and duplicates them, and the arrow keys nudge them
//! (`arrange.rs`, `selection.rs`).
//!
//! The designer only reads the world. Everything done in it comes back as
//! [`DesignerAction`]s, which the app applies through `EditorApp::change`,
//! the tree edits and the stylesheet tabs, so each is an undo step and the
//! Scene View and Play show the same UI. The artboard is laid out with
//! `axcenity_ui::layout_artboard`: exactly what a screen of that size shows
//! in Play, scaled by the zoom.

mod anchor_handles;
mod anchors;
pub(crate) mod arrange;
mod artboard;
pub(crate) mod bevel;
mod event_graph;
mod group;
mod navigation;
pub(crate) mod order;
mod panels;
mod rich_text;
mod rotate;
mod rulers;
mod selection;
mod slice;
mod states;
mod text_toolbar;
mod widget_section;

pub use anchors::Pin;
pub use arrange::UiClipboard;
pub use event_graph::{DesignerTab, GraphPane, LivePreview};
pub use states::state_selector;
pub use text_toolbar::Toggle;

use std::collections::HashMap;

use axcenity_assets::Assets;
use axcenity_core::{ClassName, InstanceId, UDim2, Value, World};
use axcenity_ui::{Stylesheet, UiLayout, UiState};
use eframe::egui;
use eframe::egui_wgpu::RenderState;

use crate::animation::preview::Canvas;
use crate::node_graph::GraphView;
use crate::properties::Edit;
use crate::responsive;
use crate::style_panel::StyleEdit;
use crate::theme::{self, icons};
use crate::tree::Insertable;
use crate::ui_handles::{SnapGuide, UiDrag, UiDragEdit};
use crate::widgets;
use group::GroupDrag;

/// Screens to design at, or preview on: name, width and height.
pub const SCREENS: [(&str, u32, u32); 8] = [
    ("Desktop 1920 × 1080", 1920, 1080),
    ("Desktop 1280 × 720", 1280, 720),
    ("Desktop 2560 × 1440", 2560, 1440),
    ("Ultrawide 3440 × 1440", 3440, 1440),
    ("Old monitor 1024 × 768", 1024, 768),
    ("Tablet 1366 × 1024", 1366, 1024),
    ("Phone landscape 844 × 390", 844, 390),
    ("Phone portrait 390 × 844", 390, 844),
];

/// The unsafe edges of the phone screens in [`SCREENS`] (a notch or
/// camera, and the home bar), in pixels: top, right, bottom, left. A
/// ScreenGui with `ScreenInsets` set to `DeviceSafeInsets` keeps inside
/// them, and the artboard hatches them. Other screens have none.
pub fn safe_insets((width, height): (u32, u32)) -> [f32; 4] {
    match (width, height) {
        (390, 844) => [47.0, 0.0, 34.0, 0.0],
        (844, 390) => [0.0, 47.0, 21.0, 47.0],
        _ => [0.0; 4],
    }
}

/// What the palette offers, with its name there.
pub const PALETTE: [(Insertable, &str); 13] = [
    (Insertable::Frame, "Frame"),
    (Insertable::TextLabel, "Text"),
    (Insertable::TextButton, "Button"),
    (Insertable::ImageLabel, "Image"),
    (Insertable::TextBox, "TextBox"),
    (Insertable::PageLayout, "Pages"),
    (Insertable::Page, "Page"),
    (Insertable::Table, "Table"),
    (Insertable::ScrollingFrame, "Scroll"),
    (Insertable::Slider, "Slider"),
    (Insertable::Toggle, "Toggle"),
    (Insertable::Dropdown, "Dropdown"),
    (Insertable::ProgressBar, "Progress"),
];

/// The scale modes, as the toolbar names them.
pub const SCALE_MODES: [&str; 3] = ["Fixed", "Fit", "Scale"];

const MIN_ZOOM: f32 = 0.05;
const MAX_ZOOM: f32 = 8.0;
/// The space left around the artboard when it's fitted into view, in points.
const FIT_MARGIN: f32 = 24.0;
/// Below this width the artboard goes on top and the panels under it.
const STACK_BELOW: f32 = 640.0;
/// Where a palette item clicked (not dragged) goes in its parent, in design pixels.
const CLICK_INSERT_AT: [f32; 2] = [24.0, 24.0];

/// Something done in the designer, for the app to apply.
#[derive(Clone, Debug, PartialEq)]
pub enum DesignerAction {
    /// Selects this (or nothing).
    Select(Option<InstanceId>),
    /// Selects these, the last one the one the inspector shows: Shift- and
    /// Ctrl-clicks, and a box dragged on the artboard.
    SelectMany(Vec<InstanceId>),
    /// Sets the ScreenGui's `DesignWidth` and `DesignHeight`.
    DesignSize {
        gui: InstanceId,
        width: u32,
        height: u32,
    },
    /// Sets the ScreenGui's `ScaleMode` (one of [`SCALE_MODES`]).
    ScaleMode(InstanceId, &'static str),
    /// Puts a new `what` into `parent`, its top left `at` design pixels
    /// from the parent's.
    Insert {
        what: Insertable,
        parent: InstanceId,
        at: [f32; 2],
    },
    /// A handle dragged on the artboard.
    Drag(UiDragEdit),
    /// The box around several elements dragged (see `group.rs`): every
    /// element's edit this frame, and every frame of one drag makes one
    /// undo step.
    GroupDrag {
        resize: bool,
        edits: Vec<UiDragEdit>,
    },
    /// Align, distribute and the arrow keys: each element's new
    /// `Position`, as one undo step named `label`.
    Arrange {
        label: String,
        positions: Vec<(InstanceId, UDim2)>,
    },
    /// Paste style: writes `declarations` into each element's own `#Id`
    /// rule (an element without an `Id` gets one), as one undo step.
    PasteStyle {
        ids: Vec<InstanceId>,
        declarations: Vec<(String, String)>,
    },
    /// Duplicates the selection (as Ctrl+D does).
    Duplicate,
    /// Pastes copied elements into `parent`, with fresh `Id`s and copies
    /// of their own rules, as one undo step.
    Paste {
        parent: InstanceId,
        clipboard: UiClipboard,
    },
    /// The inspector's name or text, or text typed on the artboard.
    Edit(InstanceId, Edit),
    /// The inspector's Style section.
    Style(InstanceId, StyleEdit),
    /// Pins an element: its new `Position` and `Size` (see `anchors.rs`).
    Anchor {
        id: InstanceId,
        position: UDim2,
        size: UDim2,
    },
    /// Points the element's `property` (`NextSelectionDown` and so on) at
    /// `target`, or clears it, from dragging a focus arrow: one undo step.
    NextSelection {
        id: InstanceId,
        property: &'static str,
        target: Option<InstanceId>,
    },
    /// Like [`Anchor`](Self::Anchor), from dragging the anchor's diamonds:
    /// every frame of one drag makes one undo step.
    MoveAnchor {
        id: InstanceId,
        position: UDim2,
        size: UDim2,
    },
    /// Sets the element's CSS `font-family`.
    Font { id: InstanceId, family: String },
    /// Sets one CSS property in the element's own `#Id` rule, from the text
    /// toolbar. Every frame of one `dragging` change makes one undo step.
    TextStyle {
        id: InstanceId,
        property: &'static str,
        value: String,
        dragging: bool,
    },
    /// Like [`TextStyle`](Self::TextStyle), in the rule for one of the
    /// element's states (`#Id:hover`, `#Id:active` or `#Id:disabled`),
    /// from the states editor or the text toolbar in a state's tab.
    StateStyle {
        id: InstanceId,
        state: UiState,
        property: &'static str,
        value: String,
        dragging: bool,
    },
    /// The Bevel: `box-shadow` for the element's own `#Id` rule and, for a
    /// button, `#Id:active`, as one undo step (or part of a drag's).
    Bevel {
        id: InstanceId,
        normal: String,
        pressed: Option<String>,
        dragging: bool,
    },
    /// Bring to Front and the rest: the new z of each element that changes
    /// (see `order.rs`), as one undo step. Each is its `ZIndex`, or its
    /// CSS `z-index` when that's what places it.
    Restack(Vec<(InstanceId, f64)>),
    /// Makes a ScreenGui, when there's none to design.
    NewScreenGui,
    /// An edit of the shown ScreenGui's Event Graph finished: the graph as
    /// it was before, for undo.
    GraphStep(axcenity_blocks::node_graph::NodeGraph),
    /// What the Live Preview printed (true for an error), for Output.
    PreviewOutput(Vec<(bool, String)>),
}

/// The action that sets `property` for `id` in `state`: in its own `#Id`
/// rule for Normal, else in that state's rule.
pub fn state_style_action(
    id: InstanceId,
    state: UiState,
    property: &'static str,
    value: String,
    dragging: bool,
) -> DesignerAction {
    match state {
        UiState::Normal => DesignerAction::TextStyle {
            id,
            property,
            value,
            dragging,
        },
        _ => DesignerAction::StateStyle {
            id,
            state,
            property,
            value,
            dragging,
        },
    }
}

/// What's dragged out of the palette.
#[derive(Clone, Copy, Debug)]
pub struct PaletteItem(pub Insertable);

/// What the designer needs from the app to draw.
pub struct DesignerScene<'a> {
    pub world: &'a World,
    pub selected: Option<InstanceId>,
    /// Everything selected; `selected` is the last of it.
    pub selection: &'a [InstanceId],
    pub stylesheets: &'a [Stylesheet],
    pub assets: &'a Assets,
    /// False during Play: the designer shows the game, but doesn't edit it.
    pub editing: bool,
}

/// A guide dragged out of a ruler: a line elements snap to, in design
/// pixels from the ScreenGui's top left. Kept per ScreenGui while the
/// editor runs.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Guide {
    /// True for an up-and-down line (from the left ruler), at an x.
    pub vertical: bool,
    pub at: f32,
}

/// Text being typed over on the artboard.
struct TextEditing {
    id: InstanceId,
    /// What it said before, for Escape.
    original: String,
    text: String,
    /// Focus is asked for on the first frame only.
    focused: bool,
    /// The characters selected in the box, as last seen.
    selection: Option<std::ops::Range<usize>>,
}

/// The designer's view and the drag under way. Kept while the editor runs.
pub struct UiDesigner {
    /// The ScreenGui on the artboard.
    gui: Option<InstanceId>,
    /// The selection last followed, so a change is followed once.
    followed: Option<InstanceId>,
    /// The screen previewed, or `None` for the design size.
    preview: Option<(u32, u32)>,
    /// Points per screen pixel.
    pub zoom: f32,
    /// The artboard's top left, from the canvas's top left, in points.
    pan: egui::Vec2,
    /// What was fitted into view last: the ScreenGui and the screen size.
    /// Another one (or a new size) is fitted again.
    fitted: Option<(InstanceId, (u32, u32))>,
    /// The canvas size it was fitted into. While the view hasn't been
    /// zoomed or panned by hand, a resized canvas fits it again.
    fitted_room: egui::Vec2,
    /// The view was zoomed or panned by hand since it was fitted.
    moved: bool,
    /// A zoom asked for by the toolbar, about the canvas's middle.
    zoom_by: Option<f32>,
    /// Set by Fit, to fit the artboard on the next frame.
    fit_next: bool,
    canvas: Canvas,
    drag: Option<UiDrag>,
    /// The box around several selected elements being dragged.
    group_drag: Option<GroupDrag>,
    /// A box being dragged on empty artboard to select what's inside it.
    marquee: Option<selection::Marquee>,
    /// What Copy style took, for Paste style.
    copied_style: Option<Vec<(String, String)>>,
    /// What Ctrl+C copied, for Ctrl+V.
    clipboard: Option<UiClipboard>,
    /// The anchor's diamonds being dragged.
    anchor_drag: Option<anchor_handles::AnchorDrag>,
    /// The selection being turned by a rotate knob.
    rotate_drag: Option<rotate::RotateDrag>,
    /// A focus arrow being dragged onto another element.
    focus_drag: Option<navigation::FocusDrag>,
    /// The diamonds' right-click menu is open.
    anchor_menu: bool,
    /// Where the pointer went down for the drag, until it has moved: a
    /// click that doesn't move changes nothing (not even a snap).
    drag_held: Option<egui::Pos2>,
    /// The lines the drag under way snapped to.
    snaps: Vec<SnapGuide>,
    /// Ruler guides, by ScreenGui.
    guides: HashMap<InstanceId, Vec<Guide>>,
    /// A guide being dragged: which one (`None` for a new one) and its
    /// direction.
    guide_drag: Option<(Option<usize>, bool)>,
    text_editing: Option<TextEditing>,
    /// The project's font families, and the font list they were read from.
    project_fonts: Option<(usize, Vec<String>)>,
    /// The states editor's tab: which state the inspector edits.
    state: UiState,
    /// The eye: the selected element is shown in `state` on the artboard.
    show_state: bool,
    /// The Slice editor's picture path as typed, for the element it's typed for.
    slice_path: Option<(InstanceId, String)>,
    /// The Slice editor's picture, and the image it was made from.
    slice_image: Option<(usize, egui::TextureHandle)>,
    /// The cut being dragged in the Slice editor: 0 top, 1 right, 2 bottom, 3 left.
    slice_drag: Option<usize>,
    /// Where the artboard was drawn last, in points.
    board: Option<egui::Rect>,
    /// The part of the canvas the artboard showed last, in points: where
    /// its layout's pixel 0, 0 is.
    board_visible: Option<egui::Rect>,
    /// How far each ScrollingFrame is scrolled on the artboard, in design
    /// pixels. The designer's own: its saved `CanvasPosition` doesn't change.
    scrolled: HashMap<InstanceId, [f32; 2]>,
    /// Scrolls ScrollingFrames on the artboard by the wheel and their bars.
    scroller: axcenity_ui::UiInteraction,
    /// The element whose layer order the artboard's right-click menu is for.
    order_target: Option<InstanceId>,
    /// The canvas inside the rulers, as drawn last.
    canvas_rect: Option<egui::Rect>,
    /// Nothing but sky, for the renderer's 3D part (the artboard covers it).
    empty: World,
    /// Design or Event Graph.
    tab: DesignerTab,
    /// Each ScreenGui's Event Graph view: where it looks, what's selected.
    graph_views: HashMap<InstanceId, GraphView>,
    /// The Event Graph had the pointer last frame.
    graph_hovered: bool,
    /// The Event Graph's Lua shows beside it.
    show_lua: bool,
    /// The Event Graph running on a copy of the world.
    live: Option<LivePreview>,
    /// The artboard is showing the Live Preview: clicks fire buttons.
    previewing: bool,
    /// Buttons clicked on the Live Preview's artboard this frame.
    preview_clicks: Vec<InstanceId>,
    /// What the pointer is over on the Live Preview's artboard.
    preview_hover: Option<InstanceId>,
    /// Start the Live Preview on the next frame, as its button does.
    live_wanted: bool,
}

impl Default for UiDesigner {
    fn default() -> Self {
        Self {
            gui: None,
            followed: None,
            preview: None,
            zoom: 0.5,
            pan: egui::vec2(FIT_MARGIN, FIT_MARGIN),
            fitted: None,
            fitted_room: egui::Vec2::ZERO,
            moved: false,
            zoom_by: None,
            fit_next: true,
            canvas: Canvas::default(),
            drag: None,
            group_drag: None,
            marquee: None,
            copied_style: None,
            clipboard: None,
            anchor_drag: None,
            rotate_drag: None,
            focus_drag: None,
            anchor_menu: false,
            drag_held: None,
            snaps: Vec::new(),
            guides: HashMap::new(),
            guide_drag: None,
            text_editing: None,
            project_fonts: None,
            state: UiState::Normal,
            show_state: true,
            slice_path: None,
            slice_image: None,
            slice_drag: None,
            board: None,
            board_visible: None,
            scrolled: HashMap::new(),
            scroller: axcenity_ui::UiInteraction::default(),
            order_target: None,
            canvas_rect: None,
            empty: World::new(),
            tab: DesignerTab::Design,
            graph_views: HashMap::new(),
            graph_hovered: false,
            show_lua: false,
            live: None,
            previewing: false,
            preview_clicks: Vec::new(),
            preview_hover: None,
            live_wanted: false,
        }
    }
}

/// The ScreenGui `id` is or is inside, if it's in the Workspace.
pub fn screen_gui_of(world: &World, id: Option<InstanceId>) -> Option<InstanceId> {
    let mut current = id;
    while let Some(id) = current {
        let instance = world.get(id)?;
        if instance.class() == ClassName::ScreenGui {
            return world.is_descendant_of(id, world.workspace()).then_some(id);
        }
        current = instance.parent();
    }
    None
}

/// Every ScreenGui in the Workspace, in tree order.
fn screen_guis(world: &World) -> Vec<InstanceId> {
    world
        .descendants(world.workspace())
        .into_iter()
        .filter(|&id| {
            world
                .get(id)
                .is_some_and(|instance| instance.class() == ClassName::ScreenGui)
        })
        .collect()
}

/// True for the classes a dropped element can go inside.
fn is_container(class: ClassName) -> bool {
    matches!(
        class,
        ClassName::ScreenGui
            | ClassName::Frame
            | ClassName::PageLayout
            | ClassName::Table
            | ClassName::ScrollingFrame
    )
}

/// The container a new element goes in when `id` is under the pointer or
/// selected: `id` itself, or the nearest container around it, within `gui`.
pub fn container_for(world: &World, gui: InstanceId, id: Option<InstanceId>) -> InstanceId {
    let mut current = id;
    while let Some(id) = current {
        if id == gui {
            break;
        }
        if !world.is_descendant_of(id, gui) {
            return gui;
        }
        let Some(instance) = world.get(id) else {
            break;
        };
        if is_container(instance.class()) {
            return id;
        }
        current = instance.parent();
    }
    gui
}

/// Where a drop at `pixel` (artboard layout pixels) lands in `parent`, in
/// design pixels from its top left. `gui_corner` and `gui_per_unit` are
/// where the ScreenGui's own corner is and its layout pixels per design
/// pixel, for a drop straight into it.
pub fn drop_offset(
    layout: &UiLayout,
    parent: InstanceId,
    pixel: [f32; 2],
    gui_corner: [f32; 2],
    gui_per_unit: f32,
) -> [f32; 2] {
    let (corner, per_unit) = layout
        .element(parent)
        .map_or((gui_corner, gui_per_unit), |element| {
            ([element.rect.x, element.rect.y], element.pixels_per_unit)
        });
    let per_unit = per_unit.max(1e-4);
    [
        ((pixel[0] - corner[0]) / per_unit).round(),
        ((pixel[1] - corner[1]) / per_unit).round(),
    ]
}

/// One row of the hierarchy: an element and how deep it is under the ScreenGui.
fn hierarchy(world: &World, gui: InstanceId) -> Vec<(InstanceId, usize)> {
    let mut rows = Vec::new();
    let mut stack = vec![(gui, 0)];
    while let Some((id, depth)) = stack.pop() {
        let Some(instance) = world.get(id) else {
            continue;
        };
        rows.push((id, depth));
        for &child in instance.children().iter().rev() {
            stack.push((child, depth + 1));
        }
    }
    rows
}

/// Pins `id` across by `x` and down by `y` where it is now (see
/// `anchors.rs`), as the anchor picker does. `None` when it isn't on the
/// artboard or a flex layout places it.
pub fn anchor_action(
    world: &World,
    stylesheets: &[Stylesheet],
    gui: InstanceId,
    id: InstanceId,
    x: Pin,
    y: Pin,
) -> Option<DesignerAction> {
    let instance = world.get(id)?;
    let position = instance.property("Position").and_then(Value::as_udim2)?;
    let size = instance.property("Size").and_then(Value::as_udim2)?;
    let (placement, in_flow) = panels::placement(world, stylesheets, gui, id)?;
    if in_flow {
        return None;
    }
    let (position, size) = anchors::pin(placement, position, size, Some(x), Some(y));
    Some(DesignerAction::Anchor { id, position, size })
}

/// Anchors `id` back at its parent's top left, not stretched, where it is
/// now: the Anchor section's Reset and the diamonds' Reset anchor.
pub fn reset_anchor_action(
    world: &World,
    stylesheets: &[Stylesheet],
    gui: InstanceId,
    id: InstanceId,
) -> Option<DesignerAction> {
    anchor_action(world, stylesheets, gui, id, Pin::Start, Pin::Start)
}

/// The ScreenGui's `ScaleMode`, as one of [`SCALE_MODES`].
fn scale_mode(world: &World, gui: InstanceId) -> &'static str {
    let mode = world
        .get(gui)
        .and_then(|instance| instance.property("ScaleMode"))
        .and_then(Value::as_str)
        .unwrap_or_default();
    SCALE_MODES
        .into_iter()
        .find(|known| mode.eq_ignore_ascii_case(known))
        .unwrap_or("Fixed")
}

impl UiDesigner {
    /// The ScreenGui on the artboard.
    #[cfg(test)]
    pub fn gui(&self) -> Option<InstanceId> {
        self.gui
    }

    /// The canvas inside the rulers, as drawn last, in points.
    #[cfg(test)]
    pub fn canvas(&self) -> Option<egui::Rect> {
        self.canvas_rect
    }

    /// Where the artboard was drawn last, in points, and the zoom.
    #[cfg(test)]
    pub fn board(&self) -> Option<(egui::Rect, f32)> {
        Some((self.board?, self.zoom))
    }

    /// Previews the ScreenGui on a `width`×`height` screen (`None` for its
    /// design size).
    #[cfg(test)]
    pub fn preview_at(&mut self, screen: Option<(u32, u32)>) {
        self.preview = screen;
    }

    /// How far the artboard shows a ScrollingFrame scrolled, when it's
    /// been scrolled there.
    #[cfg(test)]
    pub fn scrolled(&self, id: InstanceId) -> Option<[f32; 2]> {
        self.scrolled.get(&id).copied()
    }

    /// Picks the states editor's tab and its eye, as clicking them does.
    #[cfg(test)]
    pub fn show_in_state(&mut self, state: UiState, show: bool) {
        self.state = state;
        self.show_state = show;
    }

    /// The state the inspector edits `id` in: the states editor's tab, or
    /// Normal for an element without states.
    fn state_for(&self, scene: &DesignerScene, id: InstanceId) -> UiState {
        if states::has_states(scene.world, id, scene.stylesheets) {
            self.state
        } else {
            UiState::Normal
        }
    }

    /// The selected element and the state the eye shows it in on the
    /// artboard, when that isn't Normal.
    fn shown_state(&self, scene: &DesignerScene) -> Option<(InstanceId, UiState)> {
        let id = scene.selected?;
        let state = self.state_for(scene, id);
        (self.show_state && state != UiState::Normal).then_some((id, state))
    }

    /// The ruler guides on `gui`.
    pub fn guides(&self, gui: InstanceId) -> &[Guide] {
        self.guides.get(&gui).map_or(&[], Vec::as_slice)
    }

    /// Adds a ruler guide to `gui`.
    pub fn add_guide(&mut self, gui: InstanceId, guide: Guide) {
        self.guides.entry(gui).or_default().push(guide);
    }

    /// The screen the artboard shows `gui` on.
    fn screen(&self, world: &World, gui: InstanceId) -> (u32, u32) {
        self.preview
            .unwrap_or_else(|| axcenity_ui::design_size(world, gui))
    }

    /// Shows the ScreenGui the selection is in, when the selection changes,
    /// and keeps showing it while something outside UI is selected. With
    /// none shown (or it's gone), shows the Workspace's first.
    pub fn follow(&mut self, world: &World, selected: Option<InstanceId>) {
        if selected != self.followed {
            self.followed = selected;
            if let Some(gui) = screen_gui_of(world, selected) {
                self.gui = Some(gui);
            }
        }
        if screen_gui_of(world, self.gui) != self.gui || self.gui.is_none() {
            self.gui = screen_guis(world).first().copied();
        }
    }

    /// Fits the artboard into view on the next frame.
    pub fn fit(&mut self) {
        self.fit_next = true;
    }

    /// True while a handle, an anchor, a guide or a scroll bar is being dragged.
    pub fn is_dragging(&self) -> bool {
        self.drag.is_some()
            || self.group_drag.is_some()
            || self.marquee.is_some()
            || self.anchor_drag.is_some()
            || self.rotate_drag.is_some()
            || self.guide_drag.is_some()
            || self.scroller.scrolling()
    }

    /// Draws the designer and returns what was done in it.
    #[cfg(test)]
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        scene: &DesignerScene,
        render_state: Option<&RenderState>,
    ) -> Vec<DesignerAction> {
        self.show_with_graph(ui, scene, render_state, None)
    }

    /// [`Self::show`], with the shown ScreenGui's Event Graph (see
    /// `event_graph.rs`), which the Event Graph tab edits in place and the
    /// Live Preview runs.
    pub fn show_with_graph(
        &mut self,
        ui: &mut egui::Ui,
        scene: &DesignerScene,
        render_state: Option<&RenderState>,
        graph: Option<GraphPane>,
    ) -> Vec<DesignerAction> {
        let mut actions = Vec::new();
        self.follow(scene.world, scene.selected);
        let Some(gui) = self.gui else {
            self.drag = None;
            self.group_drag = None;
            self.marquee = None;
            self.anchor_drag = None;
            self.rotate_drag = None;
            self.live = None;
            ui.label("There's no ScreenGui to design yet.");
            if ui
                .add_enabled(
                    scene.editing,
                    widgets::button(
                        widgets::Role::Primary,
                        Some(icons::PLUS),
                        "New ScreenGui",
                        &theme::palette(ui.ctx()),
                    ),
                )
                .clicked()
            {
                actions.push(DesignerAction::NewScreenGui);
            }
            return actions;
        };
        self.toolbar(ui, scene, gui, graph.as_ref(), &mut actions);
        self.run_live(ui, scene, gui, graph.as_ref(), &mut actions);
        if self.tab == DesignerTab::EventGraph {
            self.event_graph(ui, scene, gui, graph, &mut actions);
            return actions;
        }
        self.graph_hovered = false;
        if self.live.is_some() {
            ui.weak(
                "Live Preview: click the buttons to try the Event Graph. It runs on a copy \
                 of the world, so nothing it does is kept.",
            );
            let size = ui.available_size().max(egui::vec2(80.0, 80.0));
            let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
            self.live_artboard(ui, rect, scene, gui, render_state);
            return actions;
        }
        self.arrange_bar(ui, scene, gui, &mut actions);
        self.notes(ui, scene, gui, &mut actions);
        self.text_shortcuts(ui.ctx(), scene, gui, &mut actions);
        self.selection_keys(ui.ctx(), scene, gui, &mut actions);
        ui.add_space(2.0);

        let width = ui.available_width();
        if width < STACK_BELOW {
            // Tall and narrow: the artboard on top, the panels under it.
            let height = (ui.available_height() * 0.45).clamp(140.0, 420.0);
            let (rect, _) = ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::hover());
            self.artboard(ui, rect, scene, gui, render_state, &mut actions);
            ui.separator();
            egui::ScrollArea::vertical()
                .id_salt("ui designer panels")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    self.palette(ui, scene, gui, &mut actions);
                    ui.separator();
                    self.hierarchy(ui, scene, gui, &mut actions);
                    ui.separator();
                    self.inspector(ui, scene, gui, &mut actions);
                });
            return actions;
        }
        // Wide: palette and hierarchy, the artboard, then the inspector.
        let height = ui.available_height();
        let side = (width * 0.2).clamp(150.0, 220.0);
        let inspector = (width * 0.26).clamp(200.0, 300.0);
        ui.horizontal_top(|ui| {
            let top_down = egui::Layout::top_down(egui::Align::Min);
            ui.allocate_ui_with_layout(egui::vec2(side, height), top_down, |ui| {
                egui::ScrollArea::vertical()
                    .id_salt("ui designer left")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        self.palette(ui, scene, gui, &mut actions);
                        ui.separator();
                        self.hierarchy(ui, scene, gui, &mut actions);
                    });
            });
            ui.separator();
            let middle =
                (ui.available_width() - inspector - ui.spacing().item_spacing.x * 2.0).max(80.0);
            let (rect, _) =
                ui.allocate_exact_size(egui::vec2(middle, height), egui::Sense::hover());
            self.artboard(ui, rect, scene, gui, render_state, &mut actions);
            ui.separator();
            ui.allocate_ui_with_layout(egui::vec2(ui.available_width(), height), top_down, |ui| {
                egui::ScrollArea::vertical()
                    .id_salt("ui designer right")
                    .auto_shrink([false, false])
                    .show(ui, |ui| self.inspector(ui, scene, gui, &mut actions));
            });
        });
        actions
    }

    /// Ctrl+B, Ctrl+I and Ctrl+U toggle bold, italic and underline on the
    /// selected text element, unless something is being typed in.
    fn text_shortcuts(
        &self,
        ctx: &egui::Context,
        scene: &DesignerScene,
        gui: InstanceId,
        actions: &mut Vec<DesignerAction>,
    ) {
        if !scene.editing || self.text_editing.is_some() || ctx.wants_keyboard_input() {
            return;
        }
        let Some(id) = scene
            .selected
            .filter(|&id| scene.world.is_descendant_of(id, gui))
        else {
            return;
        };
        let state = self.state_for(scene, id);
        let Some(style) =
            axcenity_ui::text_style_in_state(scene.world, id, scene.stylesheets, state)
        else {
            return;
        };
        for (key, toggle) in rich_text::SHORTCUTS {
            let shortcut = egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, key);
            if ctx.input_mut(|input| input.consume_shortcut(&shortcut)) {
                let (property, value) = toggle.flipped(&style);
                actions.push(state_style_action(id, state, property, value, false));
            }
        }
    }

    /// The ScreenGui picker, its scale mode and design size, the screen to
    /// preview on, and the zoom.
    fn toolbar(
        &mut self,
        ui: &mut egui::Ui,
        scene: &DesignerScene,
        gui: InstanceId,
        graph: Option<&GraphPane>,
        actions: &mut Vec<DesignerAction>,
    ) {
        let world = scene.world;
        let (width, height) = axcenity_ui::design_size(world, gui);
        responsive::toolbar(ui, |ui| {
            self.tabs(ui, scene, gui, graph, actions);
            let guis = screen_guis(world);
            if guis.len() > 1 {
                responsive::room_for(ui, 130.0);
                let name = |id| world.get(id).map_or("", |instance| instance.name());
                egui::ComboBox::from_id_salt("ui designer screen gui")
                    .width(110.0)
                    .selected_text(name(gui))
                    .show_ui(ui, |ui| {
                        for id in guis {
                            if ui.selectable_label(id == gui, name(id)).clicked() {
                                self.gui = Some(id);
                                actions.push(DesignerAction::Select(Some(id)));
                            }
                        }
                    });
            }
            // The rest is about the artboard.
            if self.tab == DesignerTab::EventGraph {
                return;
            }
            // The scale mode and design size wrap inside their own group,
            // so it starts a row unless it fits whole.
            responsive::room_for(ui, 380.0);
            ui.add_enabled_ui(scene.editing, |ui| {
                responsive::room_for(ui, 170.0);
                let mut mode = scale_mode(world, gui);
                let options = SCALE_MODES.map(|mode| (mode, None, mode));
                if widgets::segmented(ui, &mut mode, &options) {
                    actions.push(DesignerAction::ScaleMode(gui, mode));
                }
                // The design size: typed, dragged or picked.
                responsive::room_for(ui, 190.0);
                ui.label("Design");
                let (mut w, mut h) = (width, height);
                let changed_w = ui
                    .add(egui::DragValue::new(&mut w).range(1..=8192).speed(2.0))
                    .on_hover_text("Design width, in pixels: the size this UI is laid out at")
                    .changed();
                ui.label("×");
                let changed_h = ui
                    .add(egui::DragValue::new(&mut h).range(1..=8192).speed(2.0))
                    .on_hover_text("Design height, in pixels")
                    .changed();
                if changed_w || changed_h {
                    actions.push(DesignerAction::DesignSize {
                        gui,
                        width: w,
                        height: h,
                    });
                }
                ui.menu_button(icons::CARET_DOWN, |ui| {
                    for &(name, w, h) in &SCREENS {
                        if ui
                            .selectable_label((w, h) == (width, height), name)
                            .clicked()
                        {
                            actions.push(DesignerAction::DesignSize {
                                gui,
                                width: w,
                                height: h,
                            });
                            ui.close();
                        }
                    }
                })
                .response
                .on_hover_text("Design at a common screen size");
            });
            self.preview_picker(ui, (width, height));
            responsive::room_for(ui, 130.0);
            if widgets::icon_button(ui, icons::MAGNIFYING_GLASS_MINUS, "Zoom out", false).clicked()
            {
                self.zoom_by = Some(1.0 / 1.25);
            }
            ui.label(format!("{:.0}%", self.zoom * 100.0));
            if widgets::icon_button(ui, icons::MAGNIFYING_GLASS_PLUS, "Zoom in", false).clicked() {
                self.zoom_by = Some(1.25);
            }
            if widgets::icon_button(ui, icons::CORNERS_OUT, "Fit the artboard into view", false)
                .clicked()
            {
                self.fit();
            }
        });
    }

    /// The screen the artboard previews on: the design size, a preset, or
    /// a custom size.
    fn preview_picker(&mut self, ui: &mut egui::Ui, design: (u32, u32)) {
        responsive::room_for(ui, 200.0);
        ui.label("Preview");
        let shown = self.preview.unwrap_or(design);
        let name = if self.preview.is_none() {
            "Design size".to_owned()
        } else {
            SCREENS
                .iter()
                .find(|(_, w, h)| (*w, *h) == shown)
                .map_or_else(|| "Custom".to_owned(), |(name, _, _)| (*name).to_owned())
        };
        egui::ComboBox::from_id_salt("ui designer preview")
            .width(150.0)
            .selected_text(name)
            .show_ui(ui, |ui| {
                if ui
                    .selectable_label(self.preview.is_none(), "Design size")
                    .clicked()
                {
                    self.preview = None;
                }
                for &(name, w, h) in &SCREENS {
                    if ui
                        .selectable_label(self.preview == Some((w, h)), name)
                        .clicked()
                    {
                        self.preview = Some((w, h));
                    }
                }
                if ui
                    .selectable_label(false, "Custom…")
                    .on_hover_text("Type the screen size beside this")
                    .clicked()
                {
                    self.preview = Some(shown);
                }
            })
            .response
            .on_hover_text(
                "The screen to preview this UI on. It looks as it will in Play on that screen.",
            );
        if let Some((mut w, mut h)) = self.preview {
            let changed = ui
                .add(egui::DragValue::new(&mut w).range(1..=8192).speed(2.0))
                .changed()
                | {
                    ui.label("×");
                    ui.add(egui::DragValue::new(&mut h).range(1..=8192).speed(2.0))
                        .changed()
                };
            if changed {
                self.preview = Some((w, h));
            }
        }
    }

    /// What will differ in Play: a hidden layer, or one that doesn't scale.
    fn notes(
        &mut self,
        ui: &mut egui::Ui,
        scene: &DesignerScene,
        gui: InstanceId,
        actions: &mut Vec<DesignerAction>,
    ) {
        let Some(instance) = scene.world.get(gui) else {
            return;
        };
        if instance.property("Enabled").and_then(Value::as_bool) == Some(false) {
            ui.weak("Enabled is off: this ScreenGui is hidden in Play.");
        }
        let note = match scale_mode(scene.world, gui) {
            "Fixed" => {
                "Fixed: Play lays this out at the window's size, unscaled, so it's smaller on \
                 big screens. Scale makes it look the same on every screen."
            }
            "Fit" => {
                "Fit: screens of another shape get empty bars at the sides. Scale fills them, \
                 keeping anchored elements against their edges."
            }
            _ => return,
        };
        ui.horizontal_wrapped(|ui| {
            ui.weak(note);
            if ui
                .add_enabled(scene.editing, egui::Button::new("Use Scale"))
                .clicked()
            {
                actions.push(DesignerAction::ScaleMode(gui, "Scale"));
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use axcenity_core::UDim2;

    use super::*;

    /// A ScreenGui designed at 400×300 with a Frame at 40, 30 holding a
    /// TextLabel, and a Part.
    fn menu() -> (World, InstanceId, InstanceId, InstanceId, InstanceId) {
        let mut world = World::new();
        let gui = world.create(ClassName::ScreenGui).unwrap();
        world.set_parent(gui, Some(world.workspace())).unwrap();
        world
            .set_property(gui, "DesignWidth", Value::Number(400.0))
            .unwrap();
        world
            .set_property(gui, "DesignHeight", Value::Number(300.0))
            .unwrap();
        let frame = world.create(ClassName::Frame).unwrap();
        world.set_parent(frame, Some(gui)).unwrap();
        world
            .set_property(
                frame,
                "Position",
                Value::UDim2(UDim2::new(0.0, 40.0, 0.0, 30.0)),
            )
            .unwrap();
        let label = world.create(ClassName::TextLabel).unwrap();
        world.set_parent(label, Some(frame)).unwrap();
        let part = world.create(ClassName::Part).unwrap();
        world.set_parent(part, Some(world.workspace())).unwrap();
        (world, gui, frame, label, part)
    }

    #[test]
    fn the_designer_follows_the_selection_into_its_screen_gui() {
        let (mut world, gui, _, label, part) = menu();
        let other = world.create(ClassName::ScreenGui).unwrap();
        world.set_parent(other, Some(world.workspace())).unwrap();
        let mut designer = UiDesigner::default();
        designer.follow(&world, None);
        assert_eq!(designer.gui(), Some(gui), "the first ScreenGui");
        designer.follow(&world, Some(other));
        assert_eq!(designer.gui(), Some(other));
        designer.follow(&world, Some(label));
        assert_eq!(designer.gui(), Some(gui), "the one around the label");
        designer.follow(&world, Some(part));
        assert_eq!(designer.gui(), Some(gui), "kept while a Part is selected");
        world.set_parent(gui, None).unwrap();
        designer.follow(&world, Some(part));
        assert_eq!(designer.gui(), Some(other), "a removed one isn't shown");
    }

    #[test]
    fn a_drop_goes_into_the_container_under_it_at_design_pixels() {
        let (world, gui, frame, label, _) = menu();
        assert_eq!(container_for(&world, gui, Some(label)), frame);
        assert_eq!(container_for(&world, gui, Some(frame)), frame);
        assert_eq!(container_for(&world, gui, None), gui);

        // At half size, 10 pixels in from the artboard's corner.
        let layout = axcenity_ui::layout_artboard(&world, gui, &[], (400, 300), 0.5, [10.0, 10.0]);
        let element = layout.element(frame).unwrap();
        assert_eq!([element.rect.x, element.rect.y], [30.0, 25.0]);
        // 20 pixels into the frame on screen is 40 design pixels.
        assert_eq!(
            drop_offset(&layout, frame, [50.0, 35.0], [10.0, 10.0], 0.5),
            [40.0, 20.0]
        );
        assert_eq!(
            drop_offset(&layout, gui, [50.0, 35.0], [10.0, 10.0], 0.5),
            [80.0, 50.0]
        );
    }

    #[test]
    fn the_hierarchy_lists_the_screen_gui_and_its_elements_by_depth() {
        let (world, gui, frame, label, _) = menu();
        assert_eq!(
            hierarchy(&world, gui),
            vec![(gui, 0), (frame, 1), (label, 2)]
        );
    }

    #[test]
    fn the_scale_mode_is_read_as_the_toolbar_names_it() {
        let (mut world, gui, ..) = menu();
        assert_eq!(scale_mode(&world, gui), "Fixed");
        world
            .set_property(gui, "ScaleMode", Value::String("scale".into()))
            .unwrap();
        assert_eq!(scale_mode(&world, gui), "Scale");
    }
}
