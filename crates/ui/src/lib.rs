//! Screen-space layout for UI Instances.
//!
//! `layout_ui` and `layout_children` place elements with taffy. `paint_instance`
//! and the `css_*` helpers turn a placed element into draw items. Stylesheet
//! parsing, specificity and variables live in `style.rs`. `render` stays free
//! of Instances: this crate only returns draw-list types.

mod style;

mod css;
mod paint;

pub use css::*;
use paint::*;

use axcenity_core::{ClassName, Color3, Instance, InstanceId, UDim2, Value, World};
use axcenity_render::glam::Vec3;
use axcenity_render::glam::camera::rh;
use axcenity_render::{
    Camera, TextAlign, TextShadow, TextStroke, UiDrawItem, UiImage, UiRectangle, UiText,
    VerticalAlign,
};
use taffy::prelude::*;

pub use style::{
    CssDeclaration, SUPPORTED_PROPERTIES, StyleWarning, Stylesheet, applied_declaration,
    applied_declarations, parse_stylesheet,
};
use style::{PseudoClasses, ResolvedStyles, resolve_styles, resolve_variables};

/// A rectangle in physical pixels, from the top left of the screen.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct UiRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl UiRect {
    pub fn contains(&self, x: f32, y: f32) -> bool {
        x >= self.x && x <= self.x + self.width && y >= self.y && y <= self.y + self.height
    }
}

/// Where one UI element ended up on screen.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UiElement {
    pub id: InstanceId,
    pub rect: UiRect,
    pub button: bool,
    /// True for a TextBox, which takes keyboard focus when clicked.
    pub text_box: bool,
    /// True when its parent is a flex container: the flex layout places it,
    /// and its `Position` is ignored.
    pub in_flow: bool,
    /// Screen pixels per authored pixel. `1` normally. A ScreenGui in Fit
    /// mode lays out at its design size and scales the result, so a drag
    /// divides screen pixels by this to get back to authored pixels.
    pub pixels_per_unit: f32,
    /// Its text's size in authored pixels, as it's drawn: CSS `font-size`,
    /// else `TextSize`. Times `pixels_per_unit` for screen pixels.
    pub font_size: f32,
}

/// The UI's draw data, and each element drawn, in drawing order (so later
/// elements are on top).
#[derive(Debug, Default)]
pub struct UiLayout {
    pub draw_items: Vec<UiDrawItem>,
    pub elements: Vec<UiElement>,
    pub hovered: Option<InstanceId>,
    pub active: Option<InstanceId>,
    /// Problems found while laying out, such as a PageLayout page name that
    /// matches no Frame. The same text can repeat every frame; callers
    /// should show each distinct line once.
    pub warnings: Vec<String>,
}

impl UiLayout {
    /// The topmost element at a point, in physical pixels.
    pub fn hit(&self, x: f32, y: f32) -> Option<InstanceId> {
        self.elements
            .iter()
            .rev()
            .find(|element| element.rect.contains(x, y))
            .map(|element| element.id)
    }

    pub fn element(&self, id: InstanceId) -> Option<&UiElement> {
        self.elements.iter().find(|element| element.id == id)
    }

    /// The topmost visible button at a point in physical pixels.
    pub fn hit_button(&self, x: f32, y: f32) -> Option<InstanceId> {
        self.elements
            .iter()
            .rev()
            .find(|element| element.rect.contains(x, y))
            .and_then(|element| element.button.then_some(element.id))
    }

    /// The topmost visible text box at a point in physical pixels.
    pub fn hit_text_box(&self, x: f32, y: f32) -> Option<InstanceId> {
        self.elements
            .iter()
            .rev()
            .find(|element| element.rect.contains(x, y))
            .and_then(|element| element.text_box.then_some(element.id))
    }
}

/// What a key did to a focused text box.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TextCommand {
    /// Letters, digits and punctuation. Control characters are ignored.
    Insert(String),
    Backspace,
    /// Enter. Finishes a one-line box, and starts a new line in a multi-line one.
    Submit,
}

/// The result of [`edit_text_box`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextOutcome {
    /// The text changed and the box is still focused.
    Edited,
    /// The player finished typing. The host should fire `FocusLost`.
    Finished,
    /// `id` is not a text box, or the command changed nothing.
    Ignored,
}

/// Applies `command` to a TextBox's `Text`. A one-line box finishes on
/// Enter. A multi-line box inserts a newline instead. Clicking away is the
/// host's job: it drops focus and fires `FocusLost` itself.
pub fn edit_text_box(world: &mut World, id: InstanceId, command: TextCommand) -> TextOutcome {
    let Some(instance) = world.get(id) else {
        return TextOutcome::Ignored;
    };
    if instance.class() != ClassName::TextBox {
        return TextOutcome::Ignored;
    }
    let multiline = instance
        .property("MultiLine")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let mut text = instance
        .property("Text")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned();
    match command {
        TextCommand::Submit if !multiline => return TextOutcome::Finished,
        TextCommand::Submit => text.push('\n'),
        TextCommand::Backspace => {
            text.pop();
        }
        TextCommand::Insert(extra) => {
            let extra: String = extra
                .chars()
                .filter(|c| *c == '\n' || !c.is_control())
                .collect();
            if extra.is_empty() {
                return TextOutcome::Ignored;
            }
            if !multiline {
                text.push_str(&extra.replace(['\n', '\r'], ""));
            } else {
                text.push_str(&extra);
            }
        }
    }
    if world.set_property(id, "Text", Value::String(text)).is_err() {
        return TextOutcome::Ignored;
    }
    TextOutcome::Edited
}

/// Lays out enabled ScreenGuis into pixel-space render data.
///
/// Position and size values use scale relative to the parent plus a pixel
/// offset. Layout runs each frame so script changes and window resizing show
/// up immediately.
pub fn build_ui_draw_data(
    world: &World,
    width: u32,
    height: u32,
    stylesheets: &[Stylesheet],
) -> Vec<UiDrawItem> {
    layout_ui(world, width, height, stylesheets).draw_items
}

/// How `id`'s text is styled, as it's drawn at its authored size, but not
/// placed: what the UI Designer's text toolbar shows. `None` for an element
/// without text.
pub fn text_style(world: &World, id: InstanceId, stylesheets: &[Stylesheet]) -> Option<UiText> {
    let instance = world.get(id)?;
    let text = instance.property("Text").and_then(Value::as_str)?;
    let styles = resolve_styles(world, id, stylesheets, PseudoClasses::default());
    Some(styled_text(instance, &styles, 1.0, false, text))
}

/// [`build_ui_draw_data`], also saying where each element went, for picking
/// and handles in the editor.
pub fn layout_ui(world: &World, width: u32, height: u32, stylesheets: &[Stylesheet]) -> UiLayout {
    layout_ui_viewed(world, width, height, stylesheets, None, None)
}

/// [`layout_ui`], also placing BillboardGuis from `camera` and drawing a
/// focus ring on `focused` (a TextBox).
pub fn layout_ui_viewed(
    world: &World,
    width: u32,
    height: u32,
    stylesheets: &[Stylesheet],
    focused: Option<InstanceId>,
    camera: Option<&Camera>,
) -> UiLayout {
    layout_ui_with_pseudo(
        world,
        width,
        height,
        stylesheets,
        PseudoClasses {
            hovered: None,
            active: None,
            focused,
        },
        camera,
    )
}

/// Lays out UI with CSS pointer states. The active element is the button held
/// from the mouse-down event; it only gets `:active` while still hovered.
#[allow(clippy::too_many_arguments)] // Each is a different input to layout.
pub fn layout_ui_interactive(
    world: &World,
    width: u32,
    height: u32,
    stylesheets: &[Stylesheet],
    pointer: Option<[f32; 2]>,
    pressed_button: Option<InstanceId>,
    focused: Option<InstanceId>,
    camera: Option<&Camera>,
) -> UiLayout {
    let base = layout_ui_viewed(world, width, height, stylesheets, focused, camera);
    let hovered = pointer.and_then(|[x, y]| base.hit(x, y));
    let active = pressed_button.filter(|button| hovered == Some(*button));
    let mut layout = layout_ui_with_pseudo(
        world,
        width,
        height,
        stylesheets,
        PseudoClasses {
            hovered,
            active,
            focused,
        },
        camera,
    );
    layout.hovered = hovered;
    layout.active = active;
    layout
}

fn layout_ui_with_pseudo(
    world: &World,
    width: u32,
    height: u32,
    stylesheets: &[Stylesheet],
    pseudo: PseudoClasses,
    camera: Option<&Camera>,
) -> UiLayout {
    let mut layout = UiLayout::default();
    if width == 0 || height == 0 {
        return layout;
    }

    let workspace = world.workspace();
    for id in world.descendants(workspace) {
        let Some(instance) = world.get(id) else {
            continue;
        };
        if instance.class() != ClassName::ScreenGui
            || !instance
                .property("Enabled")
                .and_then(Value::as_bool)
                .expect("every ScreenGui has an Enabled property")
        {
            continue;
        }
        layout_screen_gui(world, id, width, height, stylesheets, pseudo, &mut layout);
    }
    if let Some(camera) = camera {
        layout_billboards(
            world,
            camera,
            width,
            height,
            &mut layout,
            stylesheets,
            pseudo,
        );
    }
    layout
}

/// Lays out one ScreenGui (`gui`) on a `width`×`height` screen, adding to `layout`.
fn layout_screen_gui(
    world: &World,
    gui: InstanceId,
    width: u32,
    height: u32,
    stylesheets: &[Stylesheet],
    pseudo: PseudoClasses,
    layout: &mut UiLayout,
) {
    let Some(instance) = world.get(gui) else {
        return;
    };
    let styles = resolve_styles(world, gui, stylesheets, pseudo);
    if css_property(&styles, "display").is_some_and(|display| display == "none") {
        return;
    }
    let opacity = css_number(&styles, "opacity")
        .unwrap_or(1.0)
        .clamp(0.0, 1.0);
    let fit = fit_transform(instance, width, height);
    let bounds = fit.map_or(
        UiRect {
            x: 0.0,
            y: 0.0,
            width: width as f32,
            height: height as f32,
        },
        |fit| UiRect {
            x: 0.0,
            y: 0.0,
            width: fit.design_width,
            height: fit.design_height,
        },
    );
    let draws = layout.draw_items.len();
    let elements = layout.elements.len();
    layout_children(world, gui, bounds, layout, stylesheets, opacity, pseudo);
    if let Some(fit) = fit {
        apply_fit(layout, draws, elements, fit);
    }
}

/// The design size of a ScreenGui: its `DesignWidth` × `DesignHeight`, in
/// whole pixels and at least 1 each (1920×1080 if they're missing).
pub fn design_size(world: &World, gui: InstanceId) -> (u32, u32) {
    let number = |name, default: f64| {
        world
            .get(gui)
            .and_then(|instance| instance.property(name))
            .and_then(Value::as_number)
            .unwrap_or(default)
            .round()
            .max(1.0) as u32
    };
    (
        number("DesignWidth", 1920.0),
        number("DesignHeight", 1080.0),
    )
}

/// One ScreenGui on a `screen` (width and height in pixels), for the UI
/// Designer's artboard: laid out exactly as [`layout_ui`] lays it out on a
/// screen that size, then scaled by `scale` and moved by `origin` (pixels).
/// It's drawn even when `Enabled` is off, since it's being designed. Each
/// element's `pixels_per_unit` is `scale` times what it would be on that
/// screen, so a drag converts back to authored pixels.
pub fn layout_artboard(
    world: &World,
    gui: InstanceId,
    stylesheets: &[Stylesheet],
    screen: (u32, u32),
    scale: f32,
    origin: [f32; 2],
) -> UiLayout {
    let mut layout = UiLayout::default();
    let (width, height) = screen;
    layout_screen_gui(
        world,
        gui,
        width,
        height,
        stylesheets,
        PseudoClasses {
            hovered: None,
            active: None,
            focused: None,
        },
        &mut layout,
    );
    let place = Fit {
        scale,
        origin_x: origin[0],
        origin_y: origin[1],
        design_width: width as f32,
        design_height: height as f32,
    };
    for item in &mut layout.draw_items {
        scale_draw_item(item, place);
    }
    for element in &mut layout.elements {
        element.rect = scale_rect(element.rect, place);
        element.pixels_per_unit *= scale;
    }
    layout
}

/// How a `width`×`height` screen shows the ScreenGui `gui`: screen pixels
/// per design pixel, and where design pixel (0, 0) lands on the screen.
/// Fixed is 1 at the corner; Fit and Scale scale the design size to fit.
pub fn screen_mapping(world: &World, gui: InstanceId, width: u32, height: u32) -> (f32, [f32; 2]) {
    world
        .get(gui)
        .and_then(|instance| fit_transform(instance, width, height))
        .map_or((1.0, [0.0, 0.0]), |fit| {
            (fit.scale, [fit.origin_x, fit.origin_y])
        })
}

/// How a ScreenGui in Fit mode maps its design rectangle onto the viewport:
/// uniform scale, centered, so the design stays proportional and inside the screen.
#[derive(Clone, Copy)]
struct Fit {
    scale: f32,
    origin_x: f32,
    origin_y: f32,
    design_width: f32,
    design_height: f32,
}

/// `Some` when this ScreenGui's `ScaleMode` is `Fit` or `Scale`.
///
/// Both scale the design size evenly by as much as fits the screen. Fit
/// lays out on the design rectangle and centres it, leaving empty bars on
/// another aspect ratio. Scale lays out on the whole screen, measured in
/// design pixels, so what's placed against an edge stays against it.
fn fit_transform(instance: &Instance, width: u32, height: u32) -> Option<Fit> {
    let mode = instance.property("ScaleMode").and_then(Value::as_str)?;
    let whole_screen = mode.eq_ignore_ascii_case("Scale");
    if !whole_screen && !mode.eq_ignore_ascii_case("Fit") {
        return None;
    }
    let design_width = instance
        .property("DesignWidth")
        .and_then(Value::as_number)
        .unwrap_or(1920.0)
        .max(1.0) as f32;
    let design_height = instance
        .property("DesignHeight")
        .and_then(Value::as_number)
        .unwrap_or(1080.0)
        .max(1.0) as f32;
    let scale = (width as f32 / design_width).min(height as f32 / design_height);
    if !scale.is_finite() || scale <= 0.0 {
        return None;
    }
    if whole_screen {
        return Some(Fit {
            scale,
            origin_x: 0.0,
            origin_y: 0.0,
            design_width: width as f32 / scale,
            design_height: height as f32 / scale,
        });
    }
    Some(Fit {
        scale,
        origin_x: (width as f32 - design_width * scale) * 0.5,
        origin_y: (height as f32 - design_height * scale) * 0.5,
        design_width,
        design_height,
    })
}

/// Scales everything `layout_children` just appended, from design pixels into the viewport.
fn apply_fit(layout: &mut UiLayout, draws: usize, elements: usize, fit: Fit) {
    for item in &mut layout.draw_items[draws..] {
        scale_draw_item(item, fit);
    }
    for element in &mut layout.elements[elements..] {
        element.rect = scale_rect(element.rect, fit);
        element.pixels_per_unit = fit.scale;
    }
}

fn scale_rect(rect: UiRect, fit: Fit) -> UiRect {
    UiRect {
        x: fit.origin_x + rect.x * fit.scale,
        y: fit.origin_y + rect.y * fit.scale,
        width: rect.width * fit.scale,
        height: rect.height * fit.scale,
    }
}

fn scale_draw_item(item: &mut UiDrawItem, fit: Fit) {
    let place = |position: &mut [f32; 2], size: &mut [f32; 2]| {
        position[0] = fit.origin_x + position[0] * fit.scale;
        position[1] = fit.origin_y + position[1] * fit.scale;
        size[0] *= fit.scale;
        size[1] *= fit.scale;
    };
    match item {
        UiDrawItem::Rectangle(rectangle) => {
            place(&mut rectangle.position, &mut rectangle.size);
            rectangle.border_width *= fit.scale;
            rectangle.border_radius *= fit.scale;
        }
        UiDrawItem::Image(image) => {
            place(&mut image.position, &mut image.size);
            image.border_radius *= fit.scale;
        }
        UiDrawItem::Text(text) => {
            place(&mut text.position, &mut text.size);
            text.font_size *= fit.scale;
            text.letter_spacing *= fit.scale;
            if let Some(height) = &mut text.line_height {
                *height *= fit.scale;
            }
            if let Some(stroke) = &mut text.stroke {
                stroke.width *= fit.scale;
            }
            for shadow in &mut text.shadows {
                shadow.offset = shadow.offset.map(|d| d * fit.scale);
                shadow.blur *= fit.scale;
            }
        }
    }
}

/// Lays out the UI children of `parent` inside `bounds` with Taffy.
///
/// Outside a flex container, children are absolutely positioned: scale is a
/// percentage of `bounds` and the UDim2 pixel offsets are added on top. In a
/// flex container (`display: flex`), Taffy flows them and `Position` is
/// ignored. Either way, CSS `width`/`height` replace `Size`.
fn layout_children(
    world: &World,
    parent: InstanceId,
    bounds: UiRect,
    out: &mut UiLayout,
    stylesheets: &[Stylesheet],
    inherited_opacity: f32,
    pseudo: PseudoClasses,
) {
    let Some(parent_instance) = world.get(parent) else {
        return;
    };
    let children: Vec<_> = parent_instance
        .children()
        .iter()
        .copied()
        .filter(|&id| included_child(world, parent, id, out))
        .collect();
    if children.is_empty() {
        return;
    }
    if parent_instance.class() == ClassName::Table {
        layout_table(
            world,
            &children,
            bounds,
            out,
            stylesheets,
            inherited_opacity,
            pseudo,
        );
        return;
    }

    let parent_styles = resolve_styles(world, parent, stylesheets, pseudo);
    let flow_children =
        css_property(&parent_styles, "display").is_some_and(|display| display == "flex");
    let mut root_style = Style {
        display: Display::Flex,
        size: Size {
            width: Dimension::Length(bounds.width),
            height: Dimension::Length(bounds.height),
        },
        ..Style::default()
    };
    if flow_children {
        apply_container_style(&mut root_style, &parent_styles);
    }
    // The space that `Size` scale is a fraction of: inside the padding of a
    // flex container, or all of `bounds` for absolutely positioned children.
    let padding = root_style.padding;
    let container = Size {
        width: bounds.width
            - resolve(padding.left, bounds.width)
            - resolve(padding.right, bounds.width),
        height: bounds.height
            - resolve(padding.top, bounds.width)
            - resolve(padding.bottom, bounds.width),
    };

    let mut tree = TaffyTree::<()>::new();
    let nodes: Vec<_> = children
        .iter()
        .map(|&id| {
            let instance = world.get(id).expect("filtered children are live");
            let position = ui_property(instance, "Position");
            let size = ui_property(instance, "Size");
            let styles = resolve_styles(world, id, stylesheets, pseudo);
            let hidden = css_property(&styles, "display").is_some_and(|display| display == "none");
            let node_style = Style {
                position: if flow_children {
                    Position::Relative
                } else {
                    Position::Absolute
                },
                inset: if flow_children {
                    Rect::auto()
                } else {
                    Rect {
                        left: LengthPercentageAuto::Percent(finite(position.x.scale)),
                        top: LengthPercentageAuto::Percent(finite(position.y.scale)),
                        right: LengthPercentageAuto::Auto,
                        bottom: LengthPercentageAuto::Auto,
                    }
                },
                size: Size {
                    width: css_dimension(&styles, "width")
                        .unwrap_or_else(|| udim_length(size.x, container.width)),
                    height: css_dimension(&styles, "height")
                        .unwrap_or_else(|| udim_length(size.y, container.height)),
                },
                flex_grow: css_number(&styles, "flex-grow").unwrap_or(0.0).max(0.0),
                margin: css_margin(&styles),
                display: if hidden || !visible(instance) {
                    Display::None
                } else {
                    Display::Flex
                },
                ..Style::default()
            };
            tree.new_leaf(node_style)
                .expect("a UI node can be added to a fresh Taffy tree")
        })
        .collect();
    let root = tree
        .new_with_children(root_style, &nodes)
        .expect("UI nodes form a valid Taffy tree");
    tree.compute_layout(
        root,
        Size {
            width: AvailableSpace::Definite(bounds.width),
            height: AvailableSpace::Definite(bounds.height),
        },
    )
    .expect("the UI root has definite dimensions");

    for (&id, node) in children.iter().zip(nodes) {
        let Some(instance) = world.get(id) else {
            continue;
        };
        let styles = resolve_styles(world, id, stylesheets, pseudo);
        let hidden = css_property(&styles, "display").is_some_and(|display| display == "none");
        if hidden || !visible(instance) {
            continue;
        }
        let opacity = inherited_opacity
            * css_number(&styles, "opacity")
                .unwrap_or(1.0)
                .clamp(0.0, 1.0);
        let layout = tree.layout(node).expect("each child has a computed layout");
        let position = ui_property(instance, "Position");
        let (offset_x, offset_y) = if flow_children {
            (0.0, 0.0)
        } else {
            (finite(position.x.offset), finite(position.y.offset))
        };
        let rect = UiRect {
            x: bounds.x + layout.location.x + offset_x,
            y: bounds.y + layout.location.y + offset_y,
            width: layout.size.width.max(0.0),
            height: layout.size.height.max(0.0),
        };

        out.elements.push(UiElement {
            id,
            rect,
            button: instance.class() == ClassName::TextButton,
            text_box: instance.class() == ClassName::TextBox,
            in_flow: flow_children,
            pixels_per_unit: 1.0,
            font_size: paint::font_size(instance, &styles),
        });
        paint_instance(
            instance,
            rect,
            &styles,
            opacity,
            pseudo.focused == Some(id),
            &mut out.draw_items,
        );
        layout_children(world, id, rect, out, stylesheets, opacity, pseudo);
    }
}

fn visible(instance: &Instance) -> bool {
    instance
        .property("Visible")
        .and_then(Value::as_bool)
        .expect("every UI element has a Visible property")
}

fn is_widget(class: ClassName) -> bool {
    matches!(
        class,
        ClassName::Frame
            | ClassName::TextLabel
            | ClassName::TextButton
            | ClassName::ImageLabel
            | ClassName::TextBox
            | ClassName::PageLayout
            | ClassName::Table
    )
}

/// Whether `id` is drawn under `parent`. A PageLayout shows one Frame, the
/// page named by `CurrentPage`, or the first Frame when that name is empty
/// or missing.
fn included_child(world: &World, parent: InstanceId, id: InstanceId, out: &mut UiLayout) -> bool {
    let Some(child) = world.get(id) else {
        return false;
    };
    if !is_widget(child.class()) {
        return false;
    }
    let Some(parent_instance) = world.get(parent) else {
        return false;
    };
    if parent_instance.class() != ClassName::PageLayout {
        return true;
    }
    if child.class() != ClassName::Frame {
        out.warnings.push(format!(
            "PageLayout \"{}\" only switches Frames. \"{}\" stays visible on every page.",
            parent_instance.name(),
            child.name()
        ));
        return true;
    }
    current_page(world, parent, out) == Some(id)
}

fn current_page(world: &World, parent: InstanceId, out: &mut UiLayout) -> Option<InstanceId> {
    let layout = world.get(parent)?;
    let frames: Vec<InstanceId> = layout
        .children()
        .iter()
        .copied()
        .filter(|&id| {
            world
                .get(id)
                .is_some_and(|child| child.class() == ClassName::Frame)
        })
        .collect();
    let name = layout
        .property("CurrentPage")
        .and_then(Value::as_str)
        .unwrap_or("");
    if !name.is_empty() {
        if let Some(id) = frames
            .iter()
            .copied()
            .find(|&id| world.get(id).is_some_and(|child| child.name() == name))
        {
            return Some(id);
        }
        out.warnings.push(format!(
            "PageLayout \"{}\" has no page named \"{name}\", so it shows the first page.",
            layout.name()
        ));
    }
    frames
        .into_iter()
        .find(|&id| world.get(id).is_some_and(visible))
}

fn layout_table(
    world: &World,
    children: &[InstanceId],
    bounds: UiRect,
    out: &mut UiLayout,
    stylesheets: &[Stylesheet],
    inherited_opacity: f32,
    pseudo: PseudoClasses,
) {
    let shown: Vec<InstanceId> = children
        .iter()
        .copied()
        .filter(|&id| {
            world.get(id).is_some_and(|child| {
                let styles = resolve_styles(world, id, stylesheets, pseudo);
                visible(child)
                    && !css_property(&styles, "display").is_some_and(|display| display == "none")
            })
        })
        .collect();
    if shown.is_empty() || bounds.width <= 0.0 || bounds.height <= 0.0 {
        return;
    }
    let columns = world
        .get(children[0])
        .and_then(|child| child.parent())
        .and_then(|parent| world.get(parent))
        .and_then(|table| table.property("Columns"))
        .and_then(Value::as_number)
        .map(|n| (n as isize).max(1) as usize)
        .unwrap_or(2);
    let gap = 8.0_f32;
    let rows = shown.len().div_ceil(columns).max(1);
    let cell_w =
        ((bounds.width - gap * (columns.saturating_sub(1) as f32)) / columns as f32).max(0.0);
    let cell_h = ((bounds.height - gap * (rows.saturating_sub(1) as f32)) / rows as f32).max(0.0);
    for (index, id) in shown.into_iter().enumerate() {
        let Some(instance) = world.get(id) else {
            continue;
        };
        let styles = resolve_styles(world, id, stylesheets, pseudo);
        let opacity = inherited_opacity
            * css_number(&styles, "opacity")
                .unwrap_or(1.0)
                .clamp(0.0, 1.0);
        let column = index % columns;
        let row = index / columns;
        let rect = UiRect {
            x: bounds.x + column as f32 * (cell_w + gap),
            y: bounds.y + row as f32 * (cell_h + gap),
            width: cell_w,
            height: cell_h,
        };
        out.elements.push(UiElement {
            id,
            rect,
            button: instance.class() == ClassName::TextButton,
            text_box: instance.class() == ClassName::TextBox,
            in_flow: true,
            pixels_per_unit: 1.0,
            font_size: paint::font_size(instance, &styles),
        });
        paint_instance(
            instance,
            rect,
            &styles,
            opacity,
            pseudo.focused == Some(id),
            &mut out.draw_items,
        );
        layout_children(world, id, rect, out, stylesheets, opacity, pseudo);
    }
}

fn layout_billboards(
    world: &World,
    camera: &Camera,
    width: u32,
    height: u32,
    out: &mut UiLayout,
    stylesheets: &[Stylesheet],
    pseudo: PseudoClasses,
) {
    if width == 0 || height == 0 {
        return;
    }
    let aspect = width as f32 / height as f32;
    let view = rh::view::look_at_mat4(camera.position, camera.look_at, Vec3::Y);
    let tan_half = (camera.fov_y_degrees.to_radians() * 0.5).tan();
    for id in world.descendants(world.workspace()) {
        let Some(instance) = world.get(id) else {
            continue;
        };
        if instance.class() != ClassName::BillboardGui
            || instance.property("Enabled").and_then(Value::as_bool) != Some(true)
        {
            continue;
        }
        let Some(position) = instance
            .parent()
            .and_then(|parent| world.get(parent))
            .and_then(|parent| parent.property("Position"))
            .and_then(Value::as_vector3)
        else {
            continue;
        };
        let offset = instance
            .property("StudsOffset")
            .and_then(Value::as_vector3)
            .unwrap_or(axcenity_core::Vector3::new(0.0, 0.0, 0.0));
        let anchor = Vec3::new(
            position.x + offset.x,
            position.y + offset.y,
            position.z + offset.z,
        );
        let max_distance = instance
            .property("MaxDistance")
            .and_then(Value::as_number)
            .unwrap_or(0.0) as f32;
        if max_distance > 0.0 && camera.position.distance(anchor) > max_distance {
            continue;
        }
        let camera_space = view.transform_point3(anchor);
        if camera_space.z >= -0.05 {
            continue;
        }
        let depth = -camera_space.z;
        let ndc_x = camera_space.x / (depth * tan_half * aspect);
        let ndc_y = camera_space.y / (depth * tan_half);
        if !ndc_x.is_finite() || !ndc_y.is_finite() {
            continue;
        }
        let size = instance
            .property("Size")
            .and_then(Value::as_udim2)
            .unwrap_or_default();
        let w = finite(size.x.scale) * width as f32 + finite(size.x.offset);
        let h = finite(size.y.scale) * height as f32 + finite(size.y.offset);
        let rect = UiRect {
            x: (ndc_x + 1.0) * 0.5 * width as f32 - w * 0.5,
            y: (1.0 - ndc_y) * 0.5 * height as f32 - h * 0.5,
            width: w.max(0.0),
            height: h.max(0.0),
        };
        layout_children(world, id, rect, out, stylesheets, 1.0, pseudo);
    }
}

fn font_family_name(family: &str) -> String {
    let name = family
        .split(',')
        .next()
        .unwrap_or("Montserrat")
        .trim()
        .trim_matches(['\'', '"']);
    match name.to_ascii_lowercase().as_str() {
        "monospace" | "jetbrains mono" | "jetbrainsmono" => "JetBrains Mono".to_owned(),
        "sans-serif" | "montserrat" | "" => "Montserrat".to_owned(),
        "serif" => "Lora".to_owned(),
        _ => name.to_owned(),
    }
}

/// A UDim's length in pixels: `scale` of `container` plus `offset`.
fn udim_length(value: axcenity_core::UDim, container: f32) -> Dimension {
    Dimension::Length((finite(value.scale) * container + finite(value.offset)).max(0.0))
}

/// A padding length in pixels. Percentages are of the width, as in CSS.
fn resolve(length: LengthPercentage, width: f32) -> f32 {
    match length {
        LengthPercentage::Length(pixels) => pixels,
        LengthPercentage::Percent(fraction) => fraction * width,
    }
}

/// `value`, a CSS length as written (`280px` or `72%`), changed for an
/// element that goes from `old` to `new` pixels on that axis. Pixels are
/// rounded to whole ones, and percentages scale with the size. `None` if the
/// value isn't a plain length (such as `var(--width)` or `auto`), or a
/// percentage of nothing.
pub fn resized_css_length(value: &str, old: f32, new: f32) -> Option<String> {
    let value = value.trim();
    if let Some(percent) = value.strip_suffix('%') {
        let percent: f32 = percent
            .trim()
            .parse()
            .ok()
            .filter(|p: &f32| p.is_finite())?;
        if old <= 0.0 {
            return None;
        }
        let resized = (percent * new.max(0.0) / old * 10.0).round() / 10.0;
        return Some(format!("{resized}%"));
    }
    let is_pixels = value
        .strip_suffix("px")
        .and_then(|pixels| pixels.trim().parse::<f32>().ok())
        .is_some_and(f32::is_finite);
    is_pixels.then(|| format!("{}px", new.max(0.0).round()))
}

fn ui_property(instance: &Instance, name: &str) -> UDim2 {
    instance
        .property(name)
        .and_then(Value::as_udim2)
        .expect("every UI element has UDim2 position and size properties")
}

fn color_property(instance: &Instance, name: &str) -> Color3 {
    instance
        .property(name)
        .and_then(Value::as_color3)
        .map(|color| {
            Color3::new(
                finite(color.r).clamp(0.0, 1.0),
                finite(color.g).clamp(0.0, 1.0),
                finite(color.b).clamp(0.0, 1.0),
            )
        })
        .expect("every UI element has the requested color property")
}

fn finite(value: f32) -> f32 {
    if value.is_finite() { value } else { 0.0 }
}

#[cfg(test)]
mod tests;
