//! Screen-space layout for UI Instances.

use axcenity_core::{ClassName, Color3, Instance, InstanceId, UDim2, Value, World};
use axcenity_render::{UiDrawItem, UiRectangle, UiText};
use taffy::prelude::*;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct UiBounds {
    x: f32,
    y: f32,
    width: f32,
    height: f32,
}

/// Lays out enabled ScreenGuis into pixel-space render data.
///
/// Position and size values use scale relative to the parent plus a pixel
/// offset. Layout runs each frame so script changes and window resizing show
/// up immediately.
pub fn build_ui_draw_data(world: &World, width: u32, height: u32) -> Vec<UiDrawItem> {
    if width == 0 || height == 0 {
        return Vec::new();
    }

    let mut draw_items = Vec::new();
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
        layout_children(
            world,
            id,
            UiBounds {
                x: 0.0,
                y: 0.0,
                width: width as f32,
                height: height as f32,
            },
            &mut draw_items,
        );
    }
    draw_items
}

/// Uses Taffy's absolute-positioned nodes for scale-based layout, then adds
/// the UDim2 pixel offsets to their resulting rectangles.
fn layout_children(world: &World, parent: InstanceId, bounds: UiBounds, out: &mut Vec<UiDrawItem>) {
    let Some(parent_instance) = world.get(parent) else {
        return;
    };
    let children: Vec<_> = parent_instance
        .children()
        .iter()
        .copied()
        .filter(|&id| {
            world.get(id).is_some_and(|child| {
                matches!(child.class(), ClassName::Frame | ClassName::TextLabel)
            })
        })
        .collect();
    if children.is_empty() {
        return;
    }

    let mut tree = TaffyTree::<()>::new();
    let nodes: Vec<_> = children
        .iter()
        .map(|&id| {
            let instance = world.get(id).expect("filtered children are live");
            let position = ui_property(instance, "Position");
            let size = ui_property(instance, "Size");
            tree.new_leaf(Style {
                position: Position::Absolute,
                inset: Rect {
                    left: LengthPercentageAuto::Percent(finite(position.x.scale)),
                    top: LengthPercentageAuto::Percent(finite(position.y.scale)),
                    right: LengthPercentageAuto::Auto,
                    bottom: LengthPercentageAuto::Auto,
                },
                size: Size {
                    width: Dimension::Percent(finite(size.x.scale).max(0.0)),
                    height: Dimension::Percent(finite(size.y.scale).max(0.0)),
                },
                ..Style::default()
            })
            .expect("a UI node can be added to a fresh Taffy tree")
        })
        .collect();
    let root = tree
        .new_with_children(
            Style {
                display: Display::Flex,
                size: Size {
                    width: Dimension::Length(bounds.width),
                    height: Dimension::Length(bounds.height),
                },
                ..Style::default()
            },
            &nodes,
        )
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
        if !instance
            .property("Visible")
            .and_then(Value::as_bool)
            .expect("every UI element has a Visible property")
        {
            continue;
        }
        let layout = tree.layout(node).expect("each child has a computed layout");
        let position = ui_property(instance, "Position");
        let size = ui_property(instance, "Size");
        let rect = UiBounds {
            x: bounds.x + layout.location.x + finite(position.x.offset),
            y: bounds.y + layout.location.y + finite(position.y.offset),
            width: (layout.size.width + finite(size.x.offset)).max(0.0),
            height: (layout.size.height + finite(size.y.offset)).max(0.0),
        };

        paint_instance(instance, rect, out);
        layout_children(world, id, rect, out);
    }
}

fn paint_instance(instance: &Instance, rect: UiBounds, out: &mut Vec<UiDrawItem>) {
    let color = color_property(instance, "BackgroundColor3");
    let transparency = instance
        .property("BackgroundTransparency")
        .and_then(Value::as_number)
        .map(|n| finite(n as f32).clamp(0.0, 1.0))
        .expect("every UI element has a BackgroundTransparency property");
    if transparency < 1.0 {
        out.push(UiDrawItem::Rectangle(UiRectangle {
            position: [rect.x, rect.y],
            size: [rect.width, rect.height],
            color: [color.r, color.g, color.b, 1.0 - transparency],
        }));
    }

    if instance.class() == ClassName::TextLabel {
        let text = instance
            .property("Text")
            .and_then(Value::as_str)
            .expect("every TextLabel has a Text property");
        let font_size = instance
            .property("TextSize")
            .and_then(Value::as_number)
            .map(|n| finite(n as f32).max(1.0))
            .expect("every TextLabel has a TextSize property");
        let color = color_property(instance, "TextColor3");
        if !text.is_empty() && rect.width > 0.0 && rect.height > 0.0 {
            out.push(UiDrawItem::Text(UiText {
                position: [rect.x + 4.0, rect.y + 4.0],
                size: [(rect.width - 8.0).max(0.0), (rect.height - 8.0).max(0.0)],
                text: text.to_owned(),
                font_size,
                color: [color.r, color.g, color.b],
            }));
        }
    }
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

        let small = build_ui_draw_data(&world, 800, 600);
        let large = build_ui_draw_data(&world, 1200, 800);
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

        let first = build_ui_draw_data(&world, 640, 480);
        world
            .set_property(label, "Text", Value::String("After".to_owned()))
            .unwrap();
        let next = build_ui_draw_data(&world, 640, 480);

        assert!(first.iter().any(|item| matches!(
            item,
            UiDrawItem::Text(text) if text.text == "Before"
        )));
        assert!(next.iter().any(|item| matches!(
            item,
            UiDrawItem::Text(text) if text.text == "After"
        )));
    }
}
