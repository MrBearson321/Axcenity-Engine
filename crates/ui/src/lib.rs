//! Screen-space layout for UI Instances.

mod style;

use axcenity_core::{ClassName, Color3, Instance, InstanceId, UDim2, Value, World};
use axcenity_render::{UiDrawItem, UiRectangle, UiText};
use taffy::prelude::*;

pub use style::{
    CssDeclaration, StyleWarning, Stylesheet, applied_declaration, applied_declarations,
    parse_stylesheet,
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
    /// True when its parent is a flex container: the flex layout places it,
    /// and its `Position` is ignored.
    pub in_flow: bool,
}

/// The UI's draw data, and each element drawn, in drawing order (so later
/// elements are on top).
#[derive(Debug, Default)]
pub struct UiLayout {
    pub draw_items: Vec<UiDrawItem>,
    pub elements: Vec<UiElement>,
    pub hovered: Option<InstanceId>,
    pub active: Option<InstanceId>,
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
    layout_ui_with_pseudo(world, width, height, stylesheets, PseudoClasses::default())
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
) -> UiLayout {
    let base = layout_ui(world, width, height, stylesheets);
    let hovered = pointer.and_then(|[x, y]| base.hit(x, y));
    let active = pressed_button.filter(|button| hovered == Some(*button));
    let mut layout = layout_ui_with_pseudo(
        world,
        width,
        height,
        stylesheets,
        PseudoClasses { hovered, active },
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
        layout_children(
            world,
            id,
            UiRect {
                x: 0.0,
                y: 0.0,
                width: width as f32,
                height: height as f32,
            },
            &mut layout,
            stylesheets,
            opacity,
            pseudo,
        );
    }
    layout
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
        .filter(|&id| {
            world.get(id).is_some_and(|child| {
                matches!(
                    child.class(),
                    ClassName::Frame
                        | ClassName::TextLabel
                        | ClassName::TextButton
                        | ClassName::ImageLabel
                )
            })
        })
        .collect();
    if children.is_empty() {
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
            in_flow: flow_children,
        });
        paint_instance(instance, rect, &styles, opacity, &mut out.draw_items);
        layout_children(world, id, rect, out, stylesheets, opacity, pseudo);
    }
}

fn visible(instance: &Instance) -> bool {
    instance
        .property("Visible")
        .and_then(Value::as_bool)
        .expect("every UI element has a Visible property")
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
    let (border_color, border_width, border_opacity) = css_border(styles);
    if (transparency < 1.0 && opacity > 0.0) || border_width > 0.0 {
        out.push(UiDrawItem::Rectangle(UiRectangle {
            position: [rect.x, rect.y],
            size: [rect.width, rect.height],
            color: [
                color.r,
                color.g,
                color.b,
                (1.0 - transparency) * opacity * css_background.map_or(1.0, |(_, a)| a),
            ],
            border_color: [
                border_color.r,
                border_color.g,
                border_color.b,
                opacity * border_opacity,
            ],
            border_width,
            border_radius: css_length_property(styles, "border-radius")
                .unwrap_or(0.0)
                .max(0.0),
        }));
    }

    if matches!(
        instance.class(),
        ClassName::TextLabel | ClassName::TextButton
    ) {
        let text = instance
            .property("Text")
            .and_then(Value::as_str)
            .expect("every TextLabel has a Text property");
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
            .map(|family| {
                family
                    .split(',')
                    .next()
                    .unwrap_or("Montserrat")
                    .trim()
                    .trim_matches(['\'', '"'])
                    .to_owned()
            })
            .unwrap_or_else(|| "Montserrat".to_owned());
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
                    opacity * css_text_color.map_or(1.0, |(_, a)| a),
                ],
                font_family,
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
            UiDrawItem::Text(_) => panic!("frame layout starts with its rectangle"),
        };
        assert_eq!(rectangle(&small).position, [410.0, 155.0]);
        assert_eq!(rectangle(&small).size, [380.0, 290.0]);
        assert_eq!(rectangle(&large).position, [610.0, 205.0]);
        assert_eq!(rectangle(&large).size, [580.0, 390.0]);
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
        );
        assert_eq!(hovered.hovered, Some(button));
        assert_eq!(hovered.hit_button(20.0, 20.0), Some(button));
        assert!(hovered.draw_items.iter().any(|item| matches!(
            item,
            UiDrawItem::Text(text) if text.text == "Start"
        )));
        let active =
            layout_ui_interactive(&world, 640, 480, &[sheet], Some([20.0, 20.0]), Some(button));
        assert_eq!(active.active, Some(button));
        let Some(UiDrawItem::Rectangle(rectangle)) = active.draw_items.first() else {
            panic!("the button background should be drawn first");
        };
        assert_eq!(rectangle.color[..3], [1.0, 0.0, 0.0]);

        world
            .set_property(button, "Visible", Value::Bool(false))
            .unwrap();
        let hidden = layout_ui_interactive(&world, 640, 480, &[], Some([20.0, 20.0]), Some(button));
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
                UiDrawItem::Text(_) => None,
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
