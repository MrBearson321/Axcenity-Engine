//! The default chat window (Phase 67): the last few messages at the
//! bottom left of the screen, each "Name: text" (a system message has no
//! name), and, while the player types after pressing `/`, the line being
//! typed under them.
//!
//! It's styled like the leaderboard: `TextChatService` rules style the
//! window (`background-color`, `color`, `font-size`, `padding`,
//! `border-radius`, `width`, and `margin-left` and `margin-bottom` from the
//! screen's edges), `TextChatService::key` the names. What the CSS leaves
//! out comes from defaults written as CSS. With `TextChatService.ChatEnabled`
//! off it isn't drawn.

use super::*;
use crate::prompt::{text_item, text_width};
use crate::style::{PseudoElement, resolve_part_styles};
use crate::widget::{paint_part, px, with_defaults};

/// The window shows this many of the newest messages.
pub const CHAT_SHOWN: usize = 8;

fn pixels(styles: &ResolvedStyles, name: &str, default: f32) -> f32 {
    css_length_property(styles, name)
        .unwrap_or(default)
        .max(0.0)
}

/// Whether `TextChatService.ChatEnabled` is on.
pub fn chat_enabled(world: &World) -> bool {
    world
        .get(world.text_chat_service())
        .and_then(|service| service.property("ChatEnabled"))
        .and_then(Value::as_bool)
        != Some(false)
}

/// The draw items for the chat window in a `width` × `height` view, over
/// the rest of the UI: `lines` (who said each, `None` for a system
/// message, and what), the newest last, and `typing`, the line being typed
/// while the chat is open. Nothing while `ChatEnabled` is off, or with no
/// messages and the chat closed.
pub fn chat_items(
    world: &World,
    lines: &[(Option<String>, String)],
    typing: Option<&str>,
    stylesheets: &[Stylesheet],
    width: u32,
    height: u32,
) -> Vec<UiDrawItem> {
    let mut out = Vec::new();
    if !chat_enabled(world) || (lines.is_empty() && typing.is_none()) || width == 0 || height == 0 {
        return out;
    }
    let service = world.text_chat_service();
    let scale = world.ui_settings().text_scale as f32;
    let pseudo = PseudoClasses::default();
    let window_styles = with_defaults(
        resolve_styles(world, service, stylesheets, pseudo),
        &[
            ("background-color", "rgba(17, 17, 20, 0.6)".to_owned()),
            ("color", "#ffffff".to_owned()),
            ("border-radius", px(8.0)),
        ],
    );
    let name_styles = with_defaults(
        resolve_part_styles(world, service, stylesheets, pseudo, PseudoElement::Key),
        &[
            ("font-weight", "700".to_owned()),
            ("color", "#9ad1ff".to_owned()),
        ],
    );
    let opacity = css_number(&window_styles, "opacity")
        .unwrap_or(1.0)
        .clamp(0.0, 1.0);
    let padding = css_box_values(&window_styles, "padding").map_or([8.0; 4], |sides| {
        sides.map(|side| match side {
            taffy::LengthPercentage::Length(pixels) => pixels.max(0.0),
            taffy::LengthPercentage::Percent(_) => 0.0,
        })
    });
    let font_size = pixels(&window_styles, "font-size", 14.0).max(1.0) * scale;
    let left = pixels(&window_styles, "margin-left", 16.0);
    let bottom = pixels(&window_styles, "margin-bottom", 16.0);
    let window_width = pixels(&window_styles, "width", 380.0).min(width as f32 - left * 2.0);
    let line_height = font_size * 1.5;
    let shown = &lines[lines.len().saturating_sub(CHAT_SHOWN)..];
    let rows = shown.len() + usize::from(typing.is_some());
    let inner_height = rows as f32 * line_height;
    let rect = UiRect {
        x: left,
        y: height as f32 - bottom - inner_height - padding[0] - padding[2],
        width: window_width.max(0.0),
        height: inner_height + padding[0] + padding[2],
    };
    paint_part(rect, &window_styles, opacity, &mut out);
    let inner_width = (rect.width - padding[1] - padding[3]).max(0.0);
    let x = rect.x + padding[3];
    let mut y = rect.y + padding[0];
    for (from, text) in shown {
        let mut at = x;
        if let Some(from) = from {
            let name = format!("{from}:");
            let name_width = text_width(&name, font_size) + font_size * 0.4;
            out.push(text_item(
                name,
                UiRect {
                    x: at,
                    y,
                    width: name_width.min(inner_width),
                    height: line_height,
                },
                font_size,
                &name_styles,
                opacity,
                TextAlign::Left,
            ));
            at += name_width;
        }
        out.push(text_item(
            text.clone(),
            UiRect {
                x: at,
                y,
                width: (x + inner_width - at).max(0.0),
                height: line_height,
            },
            font_size,
            &window_styles,
            opacity,
            TextAlign::Left,
        ));
        y += line_height;
    }
    if let Some(typing) = typing {
        out.push(text_item(
            format!("> {typing}|"),
            UiRect {
                x,
                y,
                width: inner_width,
                height: line_height,
            },
            font_size,
            &window_styles,
            opacity,
            TextAlign::Left,
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(items: &[UiDrawItem]) -> Vec<String> {
        items
            .iter()
            .filter_map(|item| match item {
                UiDrawItem::Text(text) => Some(text.text.clone()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn the_chat_window_shows_the_newest_messages_and_the_line_being_typed() {
        let world = World::new();
        let lines: Vec<(Option<String>, String)> = (1..=10)
            .map(|n| (Some(format!("Player{n}")), format!("message {n}")))
            .chain([(None, "Welcome".to_owned())])
            .collect();
        let items = chat_items(&world, &lines, Some("hel"), &[], 800, 600);
        let shown = texts(&items);
        assert!(
            !shown.contains(&"message 3".to_owned()),
            "only the newest: {shown:?}"
        );
        assert!(shown.contains(&"Player10:".to_owned()), "{shown:?}");
        assert!(shown.contains(&"message 10".to_owned()));
        assert!(
            shown.contains(&"Welcome".to_owned()),
            "a system message has no name"
        );
        assert_eq!(shown.last().map(String::as_str), Some("> hel|"));
        // Nothing to show while it's closed and empty, or turned off.
        assert!(chat_items(&world, &[], None, &[], 800, 600).is_empty());
        let mut off = World::new();
        off.set_property(off.text_chat_service(), "ChatEnabled", Value::Bool(false))
            .unwrap();
        assert!(chat_items(&off, &lines, Some("hel"), &[], 800, 600).is_empty());
    }

    #[test]
    fn the_chat_window_is_styled_by_text_chat_service_rules() {
        let world = World::new();
        let (sheet, warnings) = parse_stylesheet(
            "ui.css",
            "TextChatService { background-color: #102030; font-size: 20px; } \
             TextChatService::key { color: #ff0000; }",
        );
        assert!(warnings.is_empty(), "{warnings:?}");
        let lines = [(Some("Player".to_owned()), "hi".to_owned())];
        let items = chat_items(&world, &lines, None, &[sheet], 800, 600);
        let name = items.iter().find_map(|item| match item {
            UiDrawItem::Text(text) if text.text == "Player:" => Some(text),
            _ => None,
        });
        let name = name.expect("the name is drawn");
        assert_eq!(name.font_size, 20.0);
        assert!(
            (name.color[0] - 1.0).abs() < 1e-3 && name.color[1] < 1e-3,
            "{:?}",
            name.color
        );
    }
}
