//! The game UI settings' look (`World::ui_settings`): text scale and higher
//! contrast change the draw items a layout makes, after it's laid out, so
//! boxes, clicks and the 3D world stay where they were. Reduced motion is
//! in `transition.rs` (and the script and animation crates).

use axcenity_core::{UiSettings, Value, World};
use axcenity_render::UiDrawItem;

use crate::UiLayout;

/// Applies `world`'s game UI settings to what `layout` added from `draws`
/// and `elements` on.
pub(crate) fn apply(world: &World, layout: &mut UiLayout, draws: usize, elements: usize) {
    let settings = world.ui_settings();
    if settings == UiSettings::default() {
        return;
    }
    let scale = settings.text_scale as f32;
    if scale != 1.0 {
        for element in &mut layout.elements[elements..] {
            let fits = world
                .get(element.id)
                .and_then(|instance| instance.property("TextScaled"))
                .and_then(Value::as_bool)
                == Some(true);
            if !fits {
                element.font_size *= scale;
            }
        }
    }
    adjust_items(settings, &mut layout.draw_items[draws..]);
}

/// Applies `settings` to draw items that aren't an element's: a tooltip or
/// an open list.
pub(crate) fn adjust_items(settings: UiSettings, items: &mut [UiDrawItem]) {
    let scale = settings.text_scale as f32;
    for item in items {
        match item {
            UiDrawItem::Text(text) => {
                if !text.scaled && scale != 1.0 {
                    text.font_size *= scale;
                    text.letter_spacing *= scale;
                    if let Some(height) = &mut text.line_height {
                        *height *= scale;
                    }
                    for span in &mut text.spans {
                        span.font_size *= scale;
                    }
                }
                if settings.high_contrast {
                    contrast(&mut text.color);
                    for span in &mut text.spans {
                        contrast(&mut span.color);
                    }
                }
            }
            // A shadow stays soft; a fill that shows at all is solid.
            UiDrawItem::Rectangle(rectangle)
                if settings.high_contrast && rectangle.shadow.is_none() =>
            {
                for color in [&mut rectangle.color, &mut rectangle.color_end] {
                    if color[3] > 0.0 {
                        color[3] = 1.0;
                    }
                }
            }
            _ => {}
        }
    }
}

/// `color` as black or white, whichever is nearer it by brightness. Its
/// alpha stays, so placeholder text is still faint.
fn contrast(color: &mut [f32; 4]) {
    let [r, g, b, _] = *color;
    let light = 0.2126 * r + 0.7152 * g + 0.0722 * b >= 0.5;
    let level = if light { 1.0 } else { 0.0 };
    color[..3].fill(level);
}
