//! Screen-space layout for UI Instances.
//!
//! `layout_ui` and `layout_children` place elements with taffy. `paint_instance`
//! and the `css_*` helpers turn a placed element into draw items. Stylesheet
//! parsing, specificity and variables live in `style.rs`. `render` stays free
//! of Instances: this crate only returns draw-list types.

mod style;

use axcenity_core::{ClassName, Color3, Instance, InstanceId, UDim2, Value, World};
use axcenity_render::glam::Vec3;
use axcenity_render::glam::camera::rh;
use axcenity_render::{Camera, TextAlign, UiDrawItem, UiImage, UiRectangle, UiText};
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
        let styles = resolve_styles(world, id, stylesheets, pseudo);
        if css_property(&styles, "display").is_some_and(|display| display == "none") {
            continue;
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
        layout_children(world, id, bounds, &mut layout, stylesheets, opacity, pseudo);
        if let Some(fit) = fit {
            apply_fit(&mut layout, draws, elements, fit);
        }
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

/// `Some` when this ScreenGui's `ScaleMode` is `Fit`.
fn fit_transform(instance: &Instance, width: u32, height: u32) -> Option<Fit> {
    let mode = instance.property("ScaleMode").and_then(Value::as_str)?;
    if !mode.eq_ignore_ascii_case("Fit") {
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

fn paint_instance(
    instance: &Instance,
    rect: UiRect,
    styles: &ResolvedStyles,
    opacity: f32,
    focused: bool,
    out: &mut Vec<UiDrawItem>,
) {
    let css_background = css_color(styles, "background-color");
    let color = css_background
        .map(|(color, _)| color)
        .unwrap_or_else(|| color_property(instance, "BackgroundColor3"));
    let transparency = if css_background.is_some() {
        0.0
    } else {
        instance
            .property("BackgroundTransparency")
            .and_then(Value::as_number)
            .map(|n| finite(n as f32).clamp(0.0, 1.0))
            .expect("every UI element has a BackgroundTransparency property")
    };
    let (border_color, mut border_width, border_opacity) = css_border(styles);
    let mut border_tint = border_color;
    if focused && border_width <= 0.0 {
        border_width = 2.0;
        border_tint = color_property(instance, "TextColor3");
    }
    let gradient = css_gradient(styles);
    let fill_alpha = (1.0 - transparency) * opacity * css_background.map_or(1.0, |(_, a)| a);
    let (color_end, end_alpha) = gradient.map_or(
        ([color.r, color.g, color.b, fill_alpha], fill_alpha),
        |(start, end)| {
            // A gradient replaces the flat background. `color` below is
            // overwritten by the start color when one was parsed.
            let _ = start;
            (
                [end.0.r, end.0.g, end.0.b, opacity * end.1],
                opacity * end.1,
            )
        },
    );
    let (fill, fill_alpha) = if let Some((start, _)) = gradient {
        ([start.0.r, start.0.g, start.0.b], opacity * start.1)
    } else {
        ([color.r, color.g, color.b], fill_alpha)
    };
    if let Some((ox, oy, blur, shadow, shadow_alpha)) = css_box_shadow(styles) {
        let spread = blur.max(0.0) * 0.5;
        out.push(UiDrawItem::Rectangle(UiRectangle {
            position: [rect.x + ox - spread, rect.y + oy - spread],
            size: [rect.width + spread * 2.0, rect.height + spread * 2.0],
            color: [shadow.r, shadow.g, shadow.b, opacity * shadow_alpha * 0.85],
            color_end: [shadow.r, shadow.g, shadow.b, opacity * shadow_alpha * 0.85],
            border_color: [0.0; 4],
            border_width: 0.0,
            border_radius: css_length_property(styles, "border-radius")
                .unwrap_or(0.0)
                .max(0.0)
                + spread,
        }));
    }
    let radius = css_length_property(styles, "border-radius")
        .unwrap_or(0.0)
        .max(0.0);
    if (fill_alpha > 0.0 && opacity > 0.0) || end_alpha > 0.0 || border_width > 0.0 {
        out.push(UiDrawItem::Rectangle(UiRectangle {
            position: [rect.x, rect.y],
            size: [rect.width, rect.height],
            color: [fill[0], fill[1], fill[2], fill_alpha],
            color_end,
            border_color: [
                border_tint.r,
                border_tint.g,
                border_tint.b,
                opacity
                    * if focused && css_border(styles).1 <= 0.0 {
                        1.0
                    } else {
                        border_opacity
                    },
            ],
            border_width,
            border_radius: radius,
        }));
    }

    let image_path = instance
        .property("Image")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned();
    if !image_path.is_empty()
        && matches!(
            instance.class(),
            ClassName::ImageLabel | ClassName::TextButton
        )
    {
        let image_alpha = 1.0
            - instance
                .property("ImageTransparency")
                .and_then(Value::as_number)
                .map(|n| finite(n as f32).clamp(0.0, 1.0))
                .unwrap_or(0.0);
        if image_alpha > 0.0 && opacity > 0.0 {
            out.push(UiDrawItem::Image(UiImage {
                position: [rect.x, rect.y],
                size: [rect.width, rect.height],
                tint: [1.0, 1.0, 1.0, image_alpha * opacity],
                border_radius: radius,
                path: image_path,
                texture: None,
            }));
        }
    }

    if matches!(
        instance.class(),
        ClassName::TextLabel | ClassName::TextButton | ClassName::TextBox
    ) {
        let typed = instance
            .property("Text")
            .and_then(Value::as_str)
            .expect("every text element has a Text property");
        let placeholder = typed.is_empty() && instance.class() == ClassName::TextBox;
        let text = if placeholder {
            instance
                .property("Placeholder")
                .and_then(Value::as_str)
                .unwrap_or("")
        } else {
            typed
        };
        let font_size = css_length_property(styles, "font-size")
            .or_else(|| {
                instance
                    .property("TextSize")
                    .and_then(Value::as_number)
                    .map(|n| n as f32)
            })
            .map(finite)
            .unwrap_or(16.0)
            .max(1.0);
        let css_text_color = css_color(styles, "color");
        let color = css_text_color
            .map(|(color, _)| color)
            .unwrap_or_else(|| color_property(instance, "TextColor3"));
        let font_family = css_property(styles, "font-family")
            .map(|family| font_family_name(&family))
            .unwrap_or_else(|| "Montserrat".to_owned());
        // Buttons center their text, as in Roblox; labels start at the left.
        let button = instance.class() == ClassName::TextButton;
        let align = match css_property(styles, "text-align").as_deref().map(str::trim) {
            Some("center") => TextAlign::Center,
            Some("right" | "end") => TextAlign::Right,
            Some(_) => TextAlign::Left,
            None if button => TextAlign::Center,
            None => TextAlign::Left,
        };
        if !text.is_empty() && rect.width > 0.0 && rect.height > 0.0 {
            out.push(UiDrawItem::Text(UiText {
                position: [rect.x + 4.0, rect.y + 4.0],
                size: [(rect.width - 8.0).max(0.0), (rect.height - 8.0).max(0.0)],
                text: text.to_owned(),
                font_size,
                color: [
                    color.r,
                    color.g,
                    color.b,
                    opacity
                        * css_text_color.map_or(1.0, |(_, a)| a)
                        * if placeholder { 0.45 } else { 1.0 },
                ],
                font_family,
                align,
                center_vertically: button,
            }));
        }
    }
}

fn css_property(styles: &ResolvedStyles, name: &str) -> Option<String> {
    let value = styles.values.get(name)?;
    resolve_variables(value, &styles.variables)
}

fn css_number(styles: &ResolvedStyles, name: &str) -> Option<f32> {
    let value = css_property(styles, name)?;
    value
        .trim()
        .parse::<f32>()
        .ok()
        .filter(|value| value.is_finite())
}

fn css_length_property(styles: &ResolvedStyles, name: &str) -> Option<f32> {
    let value = css_property(styles, name)?;
    value
        .trim()
        .strip_suffix("px")?
        .trim()
        .parse::<f32>()
        .ok()
        .filter(|value| value.is_finite())
}

fn css_dimension(styles: &ResolvedStyles, name: &str) -> Option<Dimension> {
    let value = css_property(styles, name)?;
    let value = value.trim();
    if value == "auto" {
        return Some(Dimension::Auto);
    }
    if let Some(percent) = value.strip_suffix('%') {
        return percent
            .trim()
            .parse::<f32>()
            .ok()
            .filter(|value| value.is_finite())
            .map(|value| Dimension::Percent((value / 100.0).max(0.0)));
    }
    let pixels = value.strip_suffix("px")?;
    pixels
        .trim()
        .parse::<f32>()
        .ok()
        .filter(|value| value.is_finite())
        .map(|value| Dimension::Length(value.max(0.0)))
}

fn css_length(value: &str) -> Option<LengthPercentage> {
    let value = value.trim();
    if let Some(percent) = value.strip_suffix('%') {
        return percent
            .trim()
            .parse::<f32>()
            .ok()
            .filter(|value| value.is_finite())
            .map(|value| LengthPercentage::Percent(value / 100.0));
    }
    let pixels = value.strip_suffix("px")?;
    pixels
        .trim()
        .parse::<f32>()
        .ok()
        .filter(|value| value.is_finite())
        .map(LengthPercentage::Length)
}

fn css_box_values(styles: &ResolvedStyles, name: &str) -> Option<[LengthPercentage; 4]> {
    let value = css_property(styles, name)?;
    let values: Vec<_> = value.split_whitespace().filter_map(css_length).collect();
    if values.is_empty() {
        return None;
    }
    Some(match values.as_slice() {
        [top] => [*top, *top, *top, *top],
        [vertical, horizontal] => [*vertical, *horizontal, *vertical, *horizontal],
        [top, horizontal, bottom] => [*top, *horizontal, *bottom, *horizontal],
        [top, right, bottom, left] => [*top, *right, *bottom, *left],
        _ => return None,
    })
}

fn css_padding(styles: &ResolvedStyles) -> Rect<LengthPercentage> {
    let mut sides = css_box_values(styles, "padding").unwrap_or([LengthPercentage::Length(0.0); 4]);
    for (index, property) in [
        "padding-top",
        "padding-right",
        "padding-bottom",
        "padding-left",
    ]
    .into_iter()
    .enumerate()
    {
        if let Some(value) = css_property(styles, property).and_then(|value| css_length(&value)) {
            sides[index] = value;
        }
    }
    Rect {
        top: sides[0],
        right: sides[1],
        bottom: sides[2],
        left: sides[3],
    }
}

fn css_margin(styles: &ResolvedStyles) -> Rect<LengthPercentageAuto> {
    let mut sides = css_box_values(styles, "margin")
        .map(|[top, right, bottom, left]| [top.into(), right.into(), bottom.into(), left.into()])
        .unwrap_or([LengthPercentageAuto::Length(0.0); 4]);
    for (index, property) in ["margin-top", "margin-right", "margin-bottom", "margin-left"]
        .into_iter()
        .enumerate()
    {
        if let Some(value) = css_side(styles, property) {
            sides[index] = value;
        }
    }
    Rect {
        top: sides[0],
        right: sides[1],
        bottom: sides[2],
        left: sides[3],
    }
}

fn css_side(styles: &ResolvedStyles, name: &str) -> Option<LengthPercentageAuto> {
    let value = css_property(styles, name)?;
    if value.trim() == "auto" {
        Some(LengthPercentageAuto::Auto)
    } else {
        css_length(&value).map(Into::into)
    }
}

fn apply_container_style(style: &mut Style, styles: &ResolvedStyles) {
    if let Some(direction) = css_property(styles, "flex-direction") {
        style.flex_direction = match direction.as_str() {
            "row" => FlexDirection::Row,
            "row-reverse" => FlexDirection::RowReverse,
            "column-reverse" => FlexDirection::ColumnReverse,
            _ => FlexDirection::Column,
        };
    }
    if let Some(justify) = css_property(styles, "justify-content") {
        style.justify_content = Some(match justify.as_str() {
            "center" => JustifyContent::Center,
            "flex-end" | "end" => JustifyContent::FlexEnd,
            "space-between" => JustifyContent::SpaceBetween,
            "space-around" => JustifyContent::SpaceAround,
            "space-evenly" => JustifyContent::SpaceEvenly,
            _ => JustifyContent::FlexStart,
        });
    }
    if let Some(align) = css_property(styles, "align-items") {
        style.align_items = Some(match align.as_str() {
            "center" => AlignItems::Center,
            "flex-end" | "end" => AlignItems::FlexEnd,
            "stretch" => AlignItems::Stretch,
            _ => AlignItems::FlexStart,
        });
    }
    if let Some(gap) = css_property(styles, "gap").and_then(|value| css_length(&value)) {
        style.gap = Size {
            width: gap,
            height: gap,
        };
    }
    style.padding = css_padding(styles);
}

fn css_color(styles: &ResolvedStyles, name: &str) -> Option<(Color3, f32)> {
    let value = css_property(styles, name)?;
    parse_color(&value)
}

/// A CSS colour (`#rgb`, `#rrggbb`, `rgb()`, `rgba()` or a name), as its
/// colour and opacity. The editor uses it to show colour swatches.
pub fn parse_css_color(value: &str) -> Option<(Color3, f32)> {
    parse_color(value)
}

fn parse_color(value: &str) -> Option<(Color3, f32)> {
    let value = value.trim().to_ascii_lowercase();
    if let Some(hex) = value.strip_prefix('#') {
        let digits = match hex.len() {
            3 => hex
                .chars()
                .flat_map(|channel| [channel, channel])
                .collect::<String>(),
            4 => hex
                .chars()
                .take(3)
                .flat_map(|channel| [channel, channel])
                .collect(),
            6 | 8 => hex[..6].to_owned(),
            _ => return None,
        };
        let alpha = match hex.len() {
            4 => f32::from(u8::from_str_radix(&hex[3..4].repeat(2), 16).ok()?) / 255.0,
            8 => f32::from(u8::from_str_radix(&hex[6..8], 16).ok()?) / 255.0,
            _ => 1.0,
        };
        let channel = |start| u8::from_str_radix(&digits[start..start + 2], 16).ok();
        return Some((
            Color3::new(
                f32::from(channel(0)?) / 255.0,
                f32::from(channel(2)?) / 255.0,
                f32::from(channel(4)?) / 255.0,
            ),
            alpha,
        ));
    }
    let (named, alpha) = match value.as_str() {
        "black" => ([0.0, 0.0, 0.0], 1.0),
        "white" => ([1.0, 1.0, 1.0], 1.0),
        "red" => ([1.0, 0.0, 0.0], 1.0),
        "green" => ([0.0, 0.5, 0.0], 1.0),
        "blue" => ([0.0, 0.0, 1.0], 1.0),
        "yellow" => ([1.0, 1.0, 0.0], 1.0),
        "orange" => ([1.0, 0.647, 0.0], 1.0),
        "purple" => ([0.5, 0.0, 0.5], 1.0),
        "gray" | "grey" => ([0.5, 0.5, 0.5], 1.0),
        "transparent" => ([0.0, 0.0, 0.0], 0.0),
        _ => {
            let is_rgba = value.starts_with("rgba(");
            let channels = value
                .strip_prefix("rgb(")
                .or_else(|| value.strip_prefix("rgba("))?
                .strip_suffix(')')?;
            let components: Vec<_> = channels
                .split(',')
                .take(3)
                .map(|component| {
                    let component = component.trim();
                    if let Some(percent) = component.strip_suffix('%') {
                        percent.parse::<f32>().ok().map(|value| value / 100.0)
                    } else {
                        component.parse::<f32>().ok().map(|value| value / 255.0)
                    }
                })
                .collect::<Option<Vec<_>>>()?;
            if components.len() != 3 {
                return None;
            }
            let alpha = if is_rgba {
                channels
                    .rsplit_once(',')?
                    .1
                    .trim()
                    .parse::<f32>()
                    .ok()?
                    .clamp(0.0, 1.0)
            } else {
                1.0
            };
            (
                [
                    components[0].clamp(0.0, 1.0),
                    components[1].clamp(0.0, 1.0),
                    components[2].clamp(0.0, 1.0),
                ],
                alpha,
            )
        }
    };
    Some((Color3::new(named[0], named[1], named[2]), alpha))
}

fn css_border(styles: &ResolvedStyles) -> (Color3, f32, f32) {
    if let Some(border) = css_property(styles, "border") {
        let mut width = None;
        let mut color = None;
        for component in border.split_whitespace() {
            if let Some(pixels) = component.strip_suffix("px") {
                width = pixels.parse::<f32>().ok();
            } else if let Some(parsed) = parse_color(component) {
                color = Some(parsed);
            }
        }
        let color = color.or_else(|| css_color(styles, "border-color"));
        return (
            color
                .map(|(color, _)| color)
                .unwrap_or(Color3::new(0.0, 0.0, 0.0)),
            width
                .or_else(|| css_number(styles, "border-width"))
                .unwrap_or(0.0)
                .max(0.0),
            color.map_or(1.0, |(_, alpha)| alpha),
        );
    }
    let color = css_color(styles, "border-color");
    (
        color
            .map(|(color, _)| color)
            .unwrap_or(Color3::new(0.0, 0.0, 0.0)),
        css_number(styles, "border-width").unwrap_or(0.0).max(0.0),
        color.map_or(1.0, |(_, alpha)| alpha),
    )
}

/// `linear-gradient(to bottom, #fff, #000)`, two colors, top to bottom.
/// A leading `to …` direction is accepted and ignored: the blend is always
/// top to bottom.
fn css_gradient(styles: &ResolvedStyles) -> Option<((Color3, f32), (Color3, f32))> {
    let value = css_property(styles, "background-image")?;
    let inner = value
        .trim()
        .strip_prefix("linear-gradient(")?
        .strip_suffix(')')?;
    let mut colors = Vec::new();
    for part in split_css_args(inner) {
        let part = part.trim();
        if part.starts_with("to ") {
            continue;
        }
        if let Some(color) = parse_color(part) {
            colors.push(color);
        }
    }
    (colors.len() >= 2).then(|| (colors[0], colors[1]))
}

/// `box-shadow: <x> <y> <blur> <color>`, lengths in px. The blur is drawn
/// as a larger, softer rectangle behind the element.
fn css_box_shadow(styles: &ResolvedStyles) -> Option<(f32, f32, f32, Color3, f32)> {
    let value = css_property(styles, "box-shadow")?;
    let value = value.trim();
    if value.is_empty() || value == "none" {
        return None;
    }
    let (lengths, color) = split_shadow_color(value)?;
    let nums: Vec<f32> = lengths
        .split_whitespace()
        .filter_map(|token| {
            token
                .strip_suffix("px")
                .unwrap_or(token)
                .parse::<f32>()
                .ok()
                .filter(|n| n.is_finite())
        })
        .collect();
    let (x, y, blur) = match nums.as_slice() {
        [x, y, blur, ..] => (*x, *y, *blur),
        [x, y] => (*x, *y, 0.0),
        _ => return None,
    };
    Some((x, y, blur.max(0.0), color.0, color.1))
}

fn split_shadow_color(value: &str) -> Option<(&str, (Color3, f32))> {
    if let Some(start) = value.find("rgb") {
        let color = parse_color(value[start..].trim())?;
        return Some((value[..start].trim(), color));
    }
    let (lengths, color) = value.rsplit_once(char::is_whitespace)?;
    Some((lengths.trim(), parse_color(color.trim())?))
}

fn split_css_args(value: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut start = 0;
    let mut depth = 0;
    for (index, c) in value.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => depth -= 1,
            ',' if depth == 0 => {
                parts.push(&value[start..index]);
                start = index + 1;
            }
            _ => {}
        }
    }
    parts.push(&value[start..]);
    parts
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
mod tests {
    use super::*;

    #[test]
    fn scale_and_offset_layout_is_recomputed_for_the_viewport_size() {
        let mut world = World::new();
        let gui = world.create(ClassName::ScreenGui).unwrap();
        world.set_parent(gui, Some(world.workspace())).unwrap();
        let frame = world.create(ClassName::Frame).unwrap();
        world.set_parent(frame, Some(gui)).unwrap();
        world
            .set_property(
                frame,
                "Position",
                Value::UDim2(UDim2::new(0.5, 10.0, 0.25, 5.0)),
            )
            .unwrap();
        world
            .set_property(
                frame,
                "Size",
                Value::UDim2(UDim2::new(0.5, -20.0, 0.5, -10.0)),
            )
            .unwrap();

        let small = build_ui_draw_data(&world, 800, 600, &[]);
        let large = build_ui_draw_data(&world, 1200, 800, &[]);
        let rectangle = |items: &[UiDrawItem]| match &items[0] {
            UiDrawItem::Rectangle(rect) => *rect,
            UiDrawItem::Text(_) | UiDrawItem::Image(_) => {
                panic!("frame layout starts with its rectangle")
            }
        };
        assert_eq!(rectangle(&small).position, [410.0, 155.0]);
        assert_eq!(rectangle(&small).size, [380.0, 290.0]);
        assert_eq!(rectangle(&large).position, [610.0, 205.0]);
        assert_eq!(rectangle(&large).size, [580.0, 390.0]);
    }

    #[test]
    fn fit_mode_scales_uniformly_and_centers_on_a_different_aspect() {
        let mut world = World::new();
        let gui = world.create(ClassName::ScreenGui).unwrap();
        world.set_parent(gui, Some(world.workspace())).unwrap();
        world
            .set_property(gui, "ScaleMode", Value::String("Fit".to_owned()))
            .unwrap();
        world
            .set_property(gui, "DesignWidth", Value::Number(1920.0))
            .unwrap();
        world
            .set_property(gui, "DesignHeight", Value::Number(1080.0))
            .unwrap();
        let frame = world.create(ClassName::Frame).unwrap();
        world.set_parent(frame, Some(gui)).unwrap();
        world
            .set_property(
                frame,
                "Position",
                Value::UDim2(UDim2::new(0.0, 100.0, 0.0, 40.0)),
            )
            .unwrap();
        world
            .set_property(
                frame,
                "Size",
                Value::UDim2(UDim2::new(0.0, 200.0, 0.0, 80.0)),
            )
            .unwrap();

        // Half the design width, same height: scale is limited by the width
        // (0.5) and the spare height is split above and below.
        let layout = layout_ui(&world, 960, 1080, &[]);
        let rect = layout.element(frame).unwrap().rect;
        assert_eq!(
            rect,
            UiRect {
                x: 50.0,
                y: 290.0,
                width: 100.0,
                height: 40.0
            }
        );
        assert!((layout.element(frame).unwrap().pixels_per_unit - 0.5).abs() < 0.001);
    }

    #[test]
    fn changing_a_text_label_updates_the_draw_data() {
        let mut world = World::new();
        let gui = world.create(ClassName::ScreenGui).unwrap();
        world.set_parent(gui, Some(world.workspace())).unwrap();
        let label = world.create(ClassName::TextLabel).unwrap();
        world.set_parent(label, Some(gui)).unwrap();
        world
            .set_property(label, "Text", Value::String("Before".to_owned()))
            .unwrap();

        let first = build_ui_draw_data(&world, 640, 480, &[]);
        world
            .set_property(label, "Text", Value::String("After".to_owned()))
            .unwrap();
        let next = build_ui_draw_data(&world, 640, 480, &[]);

        assert!(first.iter().any(|item| matches!(
            item,
            UiDrawItem::Text(text) if text.text == "Before"
        )));
        assert!(next.iter().any(|item| matches!(
            item,
            UiDrawItem::Text(text) if text.text == "After"
        )));
    }

    #[test]
    fn buttons_draw_text_track_pointer_states_and_ignore_hidden_hit_targets() {
        let mut world = World::new();
        let gui = world.create(ClassName::ScreenGui).unwrap();
        world.set_parent(gui, Some(world.workspace())).unwrap();
        let button = world.create(ClassName::TextButton).unwrap();
        world.set_parent(button, Some(gui)).unwrap();
        world
            .set_property(button, "Class", Value::String("start".into()))
            .unwrap();
        world
            .set_property(button, "Text", Value::String("Start".into()))
            .unwrap();
        let (sheet, warnings) = parse_stylesheet(
            "ui.css",
            ".start { width: 120px; height: 40px; background-color: #000000 } \
             .start:hover { background-color: #00ff00 } \
             .start:active { background-color: #ff0000 }",
        );
        assert!(warnings.is_empty());
        let hovered = layout_ui_interactive(
            &world,
            640,
            480,
            std::slice::from_ref(&sheet),
            Some([20.0, 20.0]),
            None,
            None,
            None,
        );
        assert_eq!(hovered.hovered, Some(button));
        assert_eq!(hovered.hit_button(20.0, 20.0), Some(button));
        assert!(hovered.draw_items.iter().any(|item| matches!(
            item,
            UiDrawItem::Text(text) if text.text == "Start"
        )));
        let active = layout_ui_interactive(
            &world,
            640,
            480,
            &[sheet],
            Some([20.0, 20.0]),
            Some(button),
            None,
            None,
        );
        assert_eq!(active.active, Some(button));
        let Some(UiDrawItem::Rectangle(rectangle)) = active.draw_items.first() else {
            panic!("the button background should be drawn first");
        };
        assert_eq!(rectangle.color[..3], [1.0, 0.0, 0.0]);

        world
            .set_property(button, "Visible", Value::Bool(false))
            .unwrap();
        let hidden = layout_ui_interactive(
            &world,
            640,
            480,
            &[],
            Some([20.0, 20.0]),
            Some(button),
            None,
            None,
        );
        assert_eq!(hidden.hit_button(20.0, 20.0), None);
        assert_eq!(hidden.active, None);
    }

    #[test]
    fn css_applies_variables_colors_dimensions_borders_and_text_styles() {
        let mut world = World::new();
        let gui = world.create(ClassName::ScreenGui).unwrap();
        world.set_parent(gui, Some(world.workspace())).unwrap();
        let frame = world.create(ClassName::Frame).unwrap();
        world.set_parent(frame, Some(gui)).unwrap();
        world
            .set_property(frame, "Class", Value::String("hud".to_owned()))
            .unwrap();
        let label = world.create(ClassName::TextLabel).unwrap();
        world.set_parent(label, Some(frame)).unwrap();
        world
            .set_property(label, "Class", Value::String("score".to_owned()))
            .unwrap();
        let (stylesheet, warnings) = parse_stylesheet(
            "hud.css",
            ".hud { --panel: #123456; width: 50%; height: 30px; background-color: var(--panel); border: 2px solid #ffffff; border-radius: 8px; opacity: .5 } .score { color: #ff0000; font-size: 20px; font-family: Arial }",
        );
        assert!(warnings.is_empty());

        let items = build_ui_draw_data(&world, 800, 600, &[stylesheet]);
        let Some(UiDrawItem::Rectangle(frame)) = items.first() else {
            panic!("the frame background should be drawn first");
        };
        assert_eq!(frame.size, [400.0, 30.0]);
        assert!((frame.color[0] - 0x12 as f32 / 255.0).abs() < 0.001);
        assert_eq!(frame.color[3], 0.5);
        assert_eq!(frame.border_width, 2.0);
        assert_eq!(frame.border_radius, 8.0);
        assert!(items.iter().any(|item| matches!(
            item,
            UiDrawItem::Text(text)
                if text.font_size == 20.0
                    && text.font_family == "Arial"
                    && text.color == [1.0, 0.0, 0.0, 0.5]
        )));
    }

    #[test]
    fn flex_containers_flow_children_with_gap_and_padding() {
        let mut world = World::new();
        let gui = world.create(ClassName::ScreenGui).unwrap();
        world.set_parent(gui, Some(world.workspace())).unwrap();
        let parent = world.create(ClassName::Frame).unwrap();
        world.set_parent(parent, Some(gui)).unwrap();
        world
            .set_property(parent, "Class", Value::String("row".to_owned()))
            .unwrap();
        let first = world.create(ClassName::Frame).unwrap();
        world.set_parent(first, Some(parent)).unwrap();
        world
            .set_property(first, "Class", Value::String("item".to_owned()))
            .unwrap();
        let second = world.create(ClassName::Frame).unwrap();
        world.set_parent(second, Some(parent)).unwrap();
        world
            .set_property(second, "Class", Value::String("item".to_owned()))
            .unwrap();
        let (stylesheet, warnings) = parse_stylesheet(
            "hud.css",
            ".row { display: flex; flex-direction: row; gap: 10px; padding: 10px; width: 400px; height: 50px } .item { width: 100px; height: 20px; margin: 5px; flex-grow: 1 }",
        );
        assert!(warnings.is_empty());

        let items = build_ui_draw_data(&world, 800, 600, &[stylesheet]);
        let rectangles: Vec<_> = items
            .iter()
            .filter_map(|item| match item {
                UiDrawItem::Rectangle(rectangle) => Some(rectangle),
                UiDrawItem::Text(_) | UiDrawItem::Image(_) => None,
            })
            .collect();
        assert_eq!(rectangles.len(), 3);
        assert_eq!(rectangles[1].position, [15.0, 15.0]);
        assert_eq!(rectangles[2].position, [210.0, 15.0]);
        assert_eq!(rectangles[1].size, [175.0, 20.0]);
        assert_eq!(rectangles[2].size, [175.0, 20.0]);
    }

    #[test]
    fn the_health_hud_example_uses_only_supported_css() {
        let (stylesheet, warnings) =
            parse_stylesheet("ui.css", include_str!("../../../examples/hello-css/ui.css"));
        assert!(warnings.is_empty(), "{warnings:#?}");
        let mut world = World::new();
        let gui = world.create(ClassName::ScreenGui).unwrap();
        world.set_parent(gui, Some(world.workspace())).unwrap();
        let panel = world.create(ClassName::Frame).unwrap();
        world.set_parent(panel, Some(gui)).unwrap();
        world
            .set_property(panel, "Class", Value::String("hud".to_owned()))
            .unwrap();

        let items = build_ui_draw_data(&world, 800, 600, &[stylesheet]);
        let Some(UiDrawItem::Rectangle(panel)) = items.first() else {
            panic!("the health HUD panel should be drawn");
        };
        assert_eq!(panel.size, [280.0, 90.0]);
        assert_eq!(panel.color, [20.0 / 255.0, 34.0 / 255.0, 56.0 / 255.0, 1.0]);
    }

    #[test]
    fn a_pixel_size_without_css_is_that_many_pixels() {
        let mut world = World::new();
        let gui = world.create(ClassName::ScreenGui).unwrap();
        world.set_parent(gui, Some(world.workspace())).unwrap();
        let frame = world.create(ClassName::Frame).unwrap();
        world.set_parent(frame, Some(gui)).unwrap();
        world
            .set_property(
                frame,
                "Size",
                Value::UDim2(UDim2::new(0.0, 120.0, 0.0, 40.0)),
            )
            .unwrap();

        let layout = layout_ui(&world, 800, 600, &[]);
        assert_eq!(layout.element(frame).unwrap().rect.width, 120.0);
        assert_eq!(layout.element(frame).unwrap().rect.height, 40.0);
    }

    #[test]
    fn hits_find_the_topmost_element_and_flex_children_are_in_flow() {
        let mut world = World::new();
        let gui = world.create(ClassName::ScreenGui).unwrap();
        world.set_parent(gui, Some(world.workspace())).unwrap();
        let row = world.create(ClassName::Frame).unwrap();
        world.set_parent(row, Some(gui)).unwrap();
        world
            .set_property(row, "Class", Value::String("row".to_owned()))
            .unwrap();
        let item = world.create(ClassName::Frame).unwrap();
        world.set_parent(item, Some(row)).unwrap();
        let (stylesheet, _) = parse_stylesheet(
            "ui.css",
            ".row { display: flex; padding: 10px; width: 200px; height: 100px } Frame Frame { width: 50px; height: 50px }",
        );

        let layout = layout_ui(&world, 800, 600, &[stylesheet]);
        assert_eq!(layout.hit(30.0, 30.0), Some(item));
        assert_eq!(layout.hit(150.0, 80.0), Some(row));
        assert_eq!(layout.hit(500.0, 500.0), None);
        assert!(!layout.element(row).unwrap().in_flow);
        assert!(layout.element(item).unwrap().in_flow);
    }

    #[test]
    fn invisible_flex_children_take_no_space_and_cannot_be_hit() {
        let mut world = World::new();
        let gui = world.create(ClassName::ScreenGui).unwrap();
        world.set_parent(gui, Some(world.workspace())).unwrap();
        let row = world.create(ClassName::Frame).unwrap();
        world.set_parent(row, Some(gui)).unwrap();
        world
            .set_property(row, "Class", Value::String("row".to_owned()))
            .unwrap();
        let hidden = world.create(ClassName::Frame).unwrap();
        world.set_parent(hidden, Some(row)).unwrap();
        world
            .set_property(hidden, "Visible", Value::Bool(false))
            .unwrap();
        let visible = world.create(ClassName::Frame).unwrap();
        world.set_parent(visible, Some(row)).unwrap();
        let (stylesheet, warnings) = parse_stylesheet(
            "ui.css",
            ".row { display: flex; width: 200px; height: 60px; gap: 10px } Frame Frame { width: 50px; height: 50px }",
        );
        assert!(warnings.is_empty());

        let layout = layout_ui(&world, 200, 60, &[stylesheet]);
        let element = layout.element(visible).expect("visible child is laid out");
        assert_eq!(element.rect.x, 0.0);
        assert_eq!(layout.element(hidden), None);
        assert_eq!(layout.hit(25.0, 25.0), Some(visible));
    }

    #[test]
    fn css_lengths_are_resized_in_their_own_units() {
        assert_eq!(
            resized_css_length("280px", 280.0, 312.4),
            Some("312px".to_owned())
        );
        assert_eq!(
            resized_css_length(" 72% ", 180.0, 90.0),
            Some("36%".to_owned())
        );
        assert_eq!(resized_css_length("50%", 0.0, 90.0), None);
        assert_eq!(resized_css_length("var(--width)", 10.0, 20.0), None);
        assert_eq!(resized_css_length("auto", 10.0, 20.0), None);
        assert_eq!(
            resized_css_length("10px", 10.0, -5.0),
            Some("0px".to_owned())
        );
    }
}
