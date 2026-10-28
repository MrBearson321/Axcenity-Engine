//! The default hotbar (Phase 45): a row of slots at the bottom middle of
//! the screen, one per Tool in the player character's Inventory, each with
//! its number and its `TextureId` image (or its Name), and the Tool in
//! hand's `ToolTip` over the bar.
//!
//! It's styled like a widget: `Inventory` rules style the bar (its
//! `background-color`, `padding`, `gap` between slots, `margin-bottom`
//! from the screen's edge, and the tooltip's `color` and `font-size`),
//! `Tool` rules each slot
//! (`width`, `height`, `background-color`, `color`, `border-radius`,
//! `border` and the rest; `Tool:checked` is the one in hand, and a Tool's
//! own `Class` or `Id` picks one out), and `Tool::key` the slot's number.
//! What the CSS leaves out comes from defaults written as CSS. With the
//! Inventory's `HotbarEnabled` off, it isn't drawn.

use super::*;
use crate::prompt::{text_item, text_width};
use crate::style::{PseudoElement, resolve_part_styles};
use crate::widget::{paint_part, px, with_defaults};

fn text<'a>(world: &'a World, id: InstanceId, name: &str) -> &'a str {
    world
        .get(id)
        .and_then(|instance| instance.property(name))
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
}

/// The Tools in `inventory`, in slot order.
fn tools(world: &World, inventory: InstanceId) -> Vec<InstanceId> {
    world.get(inventory).map_or_else(Vec::new, |instance| {
        instance
            .children()
            .iter()
            .copied()
            .filter(|&child| {
                world
                    .get(child)
                    .is_some_and(|c| c.class() == ClassName::Tool)
            })
            .collect()
    })
}

fn pixels(styles: &ResolvedStyles, name: &str, default: f32) -> f32 {
    css_length_property(styles, name)
        .unwrap_or(default)
        .max(0.0)
}

/// The draw items for `inventory`'s hotbar in a `width` × `height` view, to
/// go over the rest of the UI. Nothing when it has no Tools or its
/// `HotbarEnabled` is off.
pub fn hotbar_items(
    world: &World,
    inventory: InstanceId,
    stylesheets: &[Stylesheet],
    width: u32,
    height: u32,
) -> Vec<UiDrawItem> {
    let mut out = Vec::new();
    let Some(bar) = world.get(inventory) else {
        return out;
    };
    let tools = tools(world, inventory);
    if bar.property("HotbarEnabled").and_then(Value::as_bool) == Some(false) || tools.is_empty() {
        return out;
    }
    let scale = world.ui_settings().text_scale as f32;
    let pseudo = PseudoClasses::default();
    let bar_styles = with_defaults(
        resolve_styles(world, inventory, stylesheets, pseudo),
        &[
            ("color", "#ffffff".to_owned()),
            ("font-weight", "600".to_owned()),
        ],
    );
    let opacity = css_number(&bar_styles, "opacity")
        .unwrap_or(1.0)
        .clamp(0.0, 1.0);
    let padding = css_box_values(&bar_styles, "padding").map_or([0.0; 4], |sides| {
        sides.map(|side| match side {
            taffy::LengthPercentage::Length(pixels) => pixels.max(0.0),
            taffy::LengthPercentage::Percent(_) => 0.0,
        })
    });
    let gap = pixels(&bar_styles, "gap", 8.0);
    let bottom = pixels(&bar_styles, "margin-bottom", 16.0);

    let slots: Vec<(InstanceId, ResolvedStyles, [f32; 2])> = tools
        .iter()
        .map(|&tool| {
            let in_hand = axcenity_core::tool::equipped(world, inventory) == Some(tool);
            let (background, color) = if in_hand {
                ("rgba(255, 255, 255, 0.9)", "#111114")
            } else {
                ("rgba(17, 17, 20, 0.75)", "#ffffff")
            };
            let styles = with_defaults(
                resolve_styles(world, tool, stylesheets, pseudo),
                &[
                    ("background-color", background.to_owned()),
                    ("color", color.to_owned()),
                    ("border-radius", px(8.0)),
                    ("font-weight", "600".to_owned()),
                ],
            );
            let size = [
                pixels(&styles, "width", 64.0).max(1.0),
                pixels(&styles, "height", 64.0).max(1.0),
            ];
            (tool, styles, size)
        })
        .collect();
    let inner_width: f32 =
        slots.iter().map(|(_, _, size)| size[0]).sum::<f32>() + gap * (slots.len() - 1) as f32;
    let inner_height = slots.iter().map(|(_, _, size)| size[1]).fold(0.0, f32::max);
    let rect = UiRect {
        x: (width as f32 - inner_width - padding[1] - padding[3]) * 0.5,
        y: height as f32 - bottom - inner_height - padding[0] - padding[2],
        width: inner_width + padding[1] + padding[3],
        height: inner_height + padding[0] + padding[2],
    };
    paint_part(rect, &bar_styles, opacity, &mut out);

    let mut x = rect.x + padding[3];
    for (index, (tool, styles, size)) in slots.iter().enumerate() {
        let slot = UiRect {
            x,
            y: rect.y + padding[0] + (inner_height - size[1]),
            width: size[0],
            height: size[1],
        };
        x += size[0] + gap;
        paint_slot(
            world,
            *tool,
            index,
            slot,
            styles,
            stylesheets,
            scale,
            opacity,
            &mut out,
        );
    }

    // The Tool in hand's ToolTip, over the bar.
    if let Some(tool) = axcenity_core::tool::equipped(world, inventory) {
        let tip = text(world, tool, "ToolTip");
        if !tip.is_empty() {
            let font_size = pixels(&bar_styles, "font-size", 15.0).max(1.0) * scale;
            let tip_width = text_width(tip, font_size);
            let tip_rect = UiRect {
                x: (width as f32 - tip_width) * 0.5,
                y: rect.y - font_size * 1.6,
                width: tip_width,
                height: font_size * 1.4,
            };
            out.push(text_item(
                tip.to_owned(),
                tip_rect,
                font_size,
                &bar_styles,
                opacity,
                TextAlign::Center,
            ));
        }
    }
    out
}

/// One slot: its box, its image or name, and its number.
#[allow(clippy::too_many_arguments)]
fn paint_slot(
    world: &World,
    tool: InstanceId,
    index: usize,
    slot: UiRect,
    styles: &ResolvedStyles,
    stylesheets: &[Stylesheet],
    scale: f32,
    opacity: f32,
    out: &mut Vec<UiDrawItem>,
) {
    paint_part(slot, styles, opacity, out);
    let image = text(world, tool, "TextureId");
    let inset = slot.width.min(slot.height) * 0.15;
    if image.is_empty() {
        let font_size = pixels(styles, "font-size", 13.0).max(1.0) * scale;
        out.push(text_item(
            world.get(tool).map_or("", |t| t.name()).to_owned(),
            UiRect {
                x: slot.x + 4.0,
                y: slot.y + inset,
                width: (slot.width - 8.0).max(1.0),
                height: (slot.height - inset * 2.0).max(1.0),
            },
            font_size,
            styles,
            opacity,
            TextAlign::Center,
        ));
    } else {
        out.push(UiDrawItem::Image(UiImage {
            position: [slot.x + inset, slot.y + inset],
            size: [slot.width - inset * 2.0, slot.height - inset * 2.0],
            tint: [1.0, 1.0, 1.0, opacity],
            border_radius: 0.0,
            path: image.to_owned(),
            texture: None,
            slice: None,
            clip: None,
            rotation_degrees: 0.0,
        }));
    }
    // 1 to 9, then 0 for the tenth.
    let number = if index < 9 {
        (index + 1).to_string()
    } else if index == 9 {
        "0".to_owned()
    } else {
        return;
    };
    let key_styles = with_defaults(
        resolve_part_styles(
            world,
            tool,
            stylesheets,
            PseudoClasses::default(),
            PseudoElement::Key,
        ),
        &[("font-weight", "700".to_owned())],
    );
    let key_styles = match css_property(&key_styles, "color") {
        Some(_) => key_styles,
        None => with_defaults(
            key_styles,
            &[(
                "color",
                css_property(styles, "color").unwrap_or_else(|| "#ffffff".to_owned()),
            )],
        ),
    };
    let font_size = pixels(&key_styles, "font-size", 12.0).max(1.0) * scale;
    out.push(text_item(
        number,
        UiRect {
            x: slot.x + 5.0,
            y: slot.y + 3.0,
            width: font_size * 1.5,
            height: font_size * 1.3,
        },
        font_size,
        &key_styles,
        opacity,
        TextAlign::Left,
    ));
}
