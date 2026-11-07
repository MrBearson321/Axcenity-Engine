//! The ability bar (Phase 76): a row of slots at the bottom right of the
//! screen, one per Ability the player holds with an `ActivationKey`, each
//! with its `Icon` (or its Name) and its key. While an ability cools down,
//! a dark sweep covers its slot from the top, shrinking as the cooldown
//! runs out, with the seconds left over it.
//!
//! It's styled like the hotbar: `Ability` rules style each slot (`width`,
//! `height`, `background-color`, `color`, `border-radius` and the rest; an
//! Ability's own `Class` or `Id` picks one out), and `Ability::key` the
//! key's text. What the CSS leaves out comes from defaults written as CSS.

use super::*;
use crate::prompt::{text_item, text_width};
use crate::style::{PseudoElement, resolve_part_styles};
use crate::widget::{paint_part, px, with_defaults};
use axcenity_core::ability::AbilitySlot;

/// Space between slots and from the screen's edges, in pixels.
const GAP: f32 = 8.0;
const MARGIN: f32 = 16.0;
/// How dark the cooldown sweep is.
const SWEEP_COLOR: [f32; 4] = [0.0, 0.0, 0.0, 0.6];

fn pixels(styles: &ResolvedStyles, name: &str, default: f32) -> f32 {
    css_length_property(styles, name)
        .unwrap_or(default)
        .max(0.0)
}

/// The draw items for the ability bar's `slots` in a `width` × `height`
/// view, to go over the rest of the UI. Nothing when there are none.
pub fn ability_bar_items(
    world: &World,
    slots: &[AbilitySlot],
    stylesheets: &[Stylesheet],
    width: u32,
    height: u32,
) -> Vec<UiDrawItem> {
    let mut out = Vec::new();
    let scale = world.ui_settings().text_scale as f32;
    let pseudo = PseudoClasses::default();
    let styled: Vec<(&AbilitySlot, ResolvedStyles, [f32; 2])> = slots
        .iter()
        .filter(|slot| world.contains(slot.ability))
        .map(|slot| {
            let styles = with_defaults(
                resolve_styles(world, slot.ability, stylesheets, pseudo),
                &[
                    ("background-color", "rgba(17, 17, 20, 0.75)".to_owned()),
                    ("color", "#ffffff".to_owned()),
                    ("border-radius", px(8.0)),
                    ("font-weight", "600".to_owned()),
                ],
            );
            let size = [
                pixels(&styles, "width", 64.0).max(1.0),
                pixels(&styles, "height", 64.0).max(1.0),
            ];
            (slot, styles, size)
        })
        .collect();
    if styled.is_empty() {
        return out;
    }
    let total: f32 =
        styled.iter().map(|(_, _, size)| size[0]).sum::<f32>() + GAP * (styled.len() - 1) as f32;
    let tallest = styled
        .iter()
        .map(|(_, _, size)| size[1])
        .fold(0.0, f32::max);
    let mut x = width as f32 - MARGIN - total;
    let bottom = height as f32 - MARGIN;
    for (slot, styles, size) in styled {
        let rect = UiRect {
            x,
            y: bottom - tallest + (tallest - size[1]),
            width: size[0],
            height: size[1],
        };
        x += size[0] + GAP;
        paint_slot(world, slot, rect, &styles, stylesheets, scale, &mut out);
    }
    out
}

/// One slot: its box, its icon or name, its cooldown sweep and its key.
fn paint_slot(
    world: &World,
    slot: &AbilitySlot,
    rect: UiRect,
    styles: &ResolvedStyles,
    stylesheets: &[Stylesheet],
    scale: f32,
    out: &mut Vec<UiDrawItem>,
) {
    paint_part(rect, styles, 1.0, out);
    let inset = rect.width.min(rect.height) * 0.15;
    let icon = world
        .get(slot.ability)
        .and_then(|ability| ability.property("Icon"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim();
    if icon.is_empty() {
        let font_size = pixels(styles, "font-size", 13.0).max(1.0) * scale;
        out.push(text_item(
            world
                .get(slot.ability)
                .map_or("", |ability| ability.name())
                .to_owned(),
            UiRect {
                x: rect.x + 4.0,
                y: rect.y + inset,
                width: (rect.width - 8.0).max(1.0),
                height: (rect.height - inset * 2.0).max(1.0),
            },
            font_size,
            styles,
            1.0,
            TextAlign::Center,
        ));
    } else {
        out.push(UiDrawItem::Image(UiImage {
            position: [rect.x + inset, rect.y + inset],
            size: [rect.width - inset * 2.0, rect.height - inset * 2.0],
            tint: [1.0, 1.0, 1.0, 1.0],
            border_radius: 0.0,
            path: icon.to_owned(),
            texture: None,
            slice: None,
            clip: None,
            rotation_degrees: 0.0,
        }));
    }
    let sweep = slot.sweep();
    if sweep > 0.0 {
        out.push(UiDrawItem::Rectangle(UiRectangle {
            position: [rect.x, rect.y],
            size: [rect.width, rect.height * sweep],
            color: SWEEP_COLOR,
            color_end: SWEEP_COLOR,
            ..UiRectangle::default()
        }));
        let font_size = 18.0 * scale;
        let seconds = if slot.cooldown_left >= 10.0 {
            format!("{:.0}", slot.cooldown_left.ceil())
        } else {
            format!("{:.1}", slot.cooldown_left)
        };
        let text_w = text_width(&seconds, font_size);
        out.push(text_item(
            seconds,
            UiRect {
                x: rect.x + (rect.width - text_w) * 0.5,
                y: rect.y + (rect.height - font_size * 1.3) * 0.5,
                width: text_w,
                height: font_size * 1.3,
            },
            font_size,
            styles,
            1.0,
            TextAlign::Center,
        ));
    }
    if slot.key.is_empty() {
        return;
    }
    let key_styles = with_defaults(
        resolve_part_styles(
            world,
            slot.ability,
            stylesheets,
            PseudoClasses::default(),
            PseudoElement::Key,
        ),
        &[
            ("font-weight", "700".to_owned()),
            (
                "color",
                css_property(styles, "color").unwrap_or_else(|| "#ffffff".to_owned()),
            ),
        ],
    );
    let font_size = pixels(&key_styles, "font-size", 12.0).max(1.0) * scale;
    out.push(text_item(
        slot.key.clone(),
        UiRect {
            x: rect.x + 5.0,
            y: rect.y + 3.0,
            width: text_width(&slot.key, font_size).max(font_size),
            height: font_size * 1.3,
        },
        font_size,
        &key_styles,
        1.0,
        TextAlign::Left,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bar_world() -> (World, InstanceId) {
        let mut world = World::new();
        let ability = world.create(ClassName::Ability).unwrap();
        world.set_name(ability, "Fireball").unwrap();
        world.set_parent(ability, Some(world.workspace())).unwrap();
        (world, ability)
    }

    fn rectangles(items: &[UiDrawItem]) -> Vec<UiRectangle> {
        items
            .iter()
            .filter_map(|item| match item {
                UiDrawItem::Rectangle(rect) => Some(*rect),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn a_cooling_ability_has_a_sweep_that_shrinks_as_the_cooldown_runs_out() {
        let (world, ability) = bar_world();
        let slot = |left: f32| AbilitySlot {
            ability,
            key: "Q".to_owned(),
            cooldown_left: left,
            cooldown: 3.0,
        };
        let sweep_height = |left: f32| {
            let items = ability_bar_items(&world, &[slot(left)], &[], 800, 600);
            rectangles(&items)
                .into_iter()
                .filter(|rect| rect.color == SWEEP_COLOR)
                .map(|rect| rect.size[1])
                .next()
        };
        assert_eq!(sweep_height(0.0), None, "ready: no sweep");
        let full = sweep_height(3.0).expect("a sweep as it starts");
        let half = sweep_height(1.5).expect("a sweep halfway");
        assert!((full - 64.0).abs() < 0.01, "{full}");
        assert!((half - 32.0).abs() < 0.01, "{half}");
        let items = ability_bar_items(&world, &[slot(1.5)], &[], 800, 600);
        let texts: Vec<&str> = items
            .iter()
            .filter_map(|item| match item {
                UiDrawItem::Text(text) => Some(text.text.as_str()),
                _ => None,
            })
            .collect();
        assert!(texts.contains(&"1.5"), "the seconds left: {texts:?}");
        assert!(texts.contains(&"Q"), "the key: {texts:?}");
        assert!(texts.contains(&"Fireball"), "its name: {texts:?}");
    }

    #[test]
    fn no_slots_draw_nothing() {
        let (world, _) = bar_world();
        assert!(ability_bar_items(&world, &[], &[], 800, 600).is_empty());
    }
}
