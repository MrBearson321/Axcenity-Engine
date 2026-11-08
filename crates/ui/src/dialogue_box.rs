//! The dialogue box and the quest tracker (Phase 77), drawn over the game
//! UI from what the `script` crate works out (in the player's language).
//!
//! **The dialogue box** sits at the bottom middle of the screen: the
//! speaker's name, the line (wrapped), and the choices under it, the one
//! the keyboard or controller is on highlighted. `DialogueBox` rules style
//! the box (`background-color`, `color`, `font-*`, `padding`, `border`,
//! `border-radius`, `width`, `margin-bottom`, `opacity`),
//! `DialogueBox::key` the speaker's name, `DialogueBox::option` each choice
//! and `DialogueBox::option:focus` the chosen one.
//!
//! **The quest tracker** lists the active quests at the top left: each
//! title, then its objectives as "Apples 3/5". `QuestTracker` rules style
//! the panel (and `width`, `margin-left`, `margin-top`),
//! `QuestTracker::key` the titles and `QuestTracker::check` the objectives
//! that are done. What the CSS leaves out comes from defaults written as
//! CSS.

use super::*;
use crate::prompt::text_item;
use crate::style::{PseudoElement, Widget, resolve_widget_styles};
use crate::widget::{paint_part, px, with_defaults};
use axcenity_core::dialogue::{DialogueView, QuestView};

fn pixels(styles: &ResolvedStyles, name: &str, default: f32) -> f32 {
    css_length_property(styles, name)
        .unwrap_or(default)
        .max(0.0)
}

/// Top, right, bottom and left padding.
fn padding(styles: &ResolvedStyles, default: f32) -> [f32; 4] {
    css_box_values(styles, "padding").map_or([default; 4], |sides| {
        sides.map(|side| match side {
            taffy::LengthPercentage::Length(pixels) => pixels.max(0.0),
            taffy::LengthPercentage::Percent(_) => 0.0,
        })
    })
}

/// About how many lines `text` wraps to in `width` at `font_size`.
fn wrapped_lines(text: &str, font_size: f32, width: f32) -> usize {
    let per_line = (width / (font_size * 0.52)).floor().max(1.0) as usize;
    text.split('\n')
        .map(|line| line.chars().count().div_ceil(per_line).max(1))
        .sum()
}

/// The draw items for the dialogue box showing `view` in a `width` ×
/// `height` view, over the rest of the UI.
pub fn dialogue_box_items(
    world: &World,
    view: &DialogueView,
    stylesheets: &[Stylesheet],
    width: u32,
    height: u32,
) -> Vec<UiDrawItem> {
    let mut out = Vec::new();
    if width == 0 || height == 0 {
        return out;
    }
    let scale = world.ui_settings().text_scale as f32;
    let box_styles = with_defaults(
        resolve_widget_styles(stylesheets, Widget::DialogueBox, None, false),
        &[
            ("background-color", "rgba(17, 17, 20, 0.85)".to_owned()),
            ("color", "#ffffff".to_owned()),
            ("border-radius", px(10.0)),
        ],
    );
    let name_styles = with_defaults(
        resolve_widget_styles(
            stylesheets,
            Widget::DialogueBox,
            Some(PseudoElement::Key),
            false,
        ),
        &[
            ("font-weight", "700".to_owned()),
            ("color", "#ffd27a".to_owned()),
        ],
    );
    let option_styles = |focus: bool| {
        let defaults = if focus {
            vec![
                ("background-color", "rgba(255, 210, 122, 0.25)".to_owned()),
                ("color", "#ffffff".to_owned()),
                ("border-radius", px(6.0)),
            ]
        } else {
            vec![("color", "#d8d8e0".to_owned()), ("border-radius", px(6.0))]
        };
        with_defaults(
            resolve_widget_styles(
                stylesheets,
                Widget::DialogueBox,
                Some(PseudoElement::Option),
                focus,
            ),
            &defaults,
        )
    };
    let opacity = css_number(&box_styles, "opacity")
        .unwrap_or(1.0)
        .clamp(0.0, 1.0);
    let pad = padding(&box_styles, 16.0);
    let font_size = pixels(&box_styles, "font-size", 18.0).max(1.0) * scale;
    let name_size = pixels(&name_styles, "font-size", 15.0).max(1.0) * scale;
    let option_size = pixels(&option_styles(false), "font-size", 17.0).max(1.0) * scale;
    let bottom = pixels(&box_styles, "margin-bottom", 24.0);
    let box_width = pixels(&box_styles, "width", 720.0)
        .min(width as f32 - 32.0)
        .max(1.0);
    let inner_width = (box_width - pad[1] - pad[3]).max(1.0);
    let line_height = font_size * 1.4;
    let name_height = if view.speaker.is_empty() {
        0.0
    } else {
        name_size * 1.6
    };
    let text_height = wrapped_lines(&view.text, font_size, inner_width) as f32 * line_height;
    let option_height = option_size * 1.9;
    let options_height = if view.choices.is_empty() {
        0.0
    } else {
        font_size * 0.5 + view.choices.len() as f32 * option_height
    };
    let inner_height = name_height + text_height + options_height;
    let rect = UiRect {
        x: (width as f32 - box_width) * 0.5,
        y: height as f32 - bottom - inner_height - pad[0] - pad[2],
        width: box_width,
        height: inner_height + pad[0] + pad[2],
    };
    paint_part(rect, &box_styles, opacity, &mut out);
    let x = rect.x + pad[3];
    let mut y = rect.y + pad[0];
    if !view.speaker.is_empty() {
        out.push(text_item(
            view.speaker.clone(),
            UiRect {
                x,
                y,
                width: inner_width,
                height: name_height,
            },
            name_size,
            &name_styles,
            opacity,
            TextAlign::Left,
        ));
        y += name_height;
    }
    let mut text = text_item(
        view.text.clone(),
        UiRect {
            x,
            y,
            width: inner_width,
            height: text_height,
        },
        font_size,
        &box_styles,
        opacity,
        TextAlign::Left,
    );
    if let UiDrawItem::Text(text) = &mut text {
        text.wrap = true;
        text.vertical_align = VerticalAlign::Top;
        text.line_height = Some(line_height);
    }
    out.push(text);
    y += text_height;
    if view.choices.is_empty() {
        return out;
    }
    y += font_size * 0.5;
    for (index, choice) in view.choices.iter().enumerate() {
        let focus = index == view.selected;
        let styles = option_styles(focus);
        let row = UiRect {
            x,
            y,
            width: inner_width,
            height: option_height - 4.0,
        };
        paint_part(row, &styles, opacity, &mut out);
        out.push(text_item(
            format!("{}. {choice}", index + 1),
            UiRect {
                x: row.x + 10.0,
                y: row.y,
                width: (row.width - 20.0).max(1.0),
                height: row.height,
            },
            option_size,
            &styles,
            opacity,
            TextAlign::Left,
        ));
        y += option_height;
    }
    out
}

/// The draw items for the quest tracker listing `quests` in a `width` ×
/// `height` view. Nothing with no quests.
pub fn quest_tracker_items(
    world: &World,
    quests: &[QuestView],
    stylesheets: &[Stylesheet],
    width: u32,
    height: u32,
) -> Vec<UiDrawItem> {
    let mut out = Vec::new();
    if quests.is_empty() || width == 0 || height == 0 {
        return out;
    }
    let scale = world.ui_settings().text_scale as f32;
    let panel_styles = with_defaults(
        resolve_widget_styles(stylesheets, Widget::QuestTracker, None, false),
        &[
            ("background-color", "rgba(17, 17, 20, 0.6)".to_owned()),
            ("color", "#e8e8ee".to_owned()),
            ("border-radius", px(8.0)),
        ],
    );
    let title_styles = with_defaults(
        resolve_widget_styles(
            stylesheets,
            Widget::QuestTracker,
            Some(PseudoElement::Key),
            false,
        ),
        &[
            ("font-weight", "700".to_owned()),
            ("color", "#ffd27a".to_owned()),
        ],
    );
    let done_styles = with_defaults(
        resolve_widget_styles(
            stylesheets,
            Widget::QuestTracker,
            Some(PseudoElement::Check),
            false,
        ),
        &[("color", "#8fd18f".to_owned())],
    );
    let opacity = css_number(&panel_styles, "opacity")
        .unwrap_or(1.0)
        .clamp(0.0, 1.0);
    let pad = padding(&panel_styles, 10.0);
    let font_size = pixels(&panel_styles, "font-size", 14.0).max(1.0) * scale;
    let title_size = pixels(&title_styles, "font-size", 15.0).max(1.0) * scale;
    let left = pixels(&panel_styles, "margin-left", 16.0);
    let top = pixels(&panel_styles, "margin-top", 16.0);
    let panel_width = pixels(&panel_styles, "width", 280.0)
        .min(width as f32 - left * 2.0)
        .max(1.0);
    let line = font_size * 1.5;
    let title_line = title_size * 1.6;
    let inner_height: f32 = quests
        .iter()
        .map(|quest| title_line + quest.objectives.len() as f32 * line)
        .sum::<f32>()
        + (quests.len() - 1) as f32 * font_size * 0.5;
    let rect = UiRect {
        x: left,
        y: top,
        width: panel_width,
        height: inner_height + pad[0] + pad[2],
    };
    paint_part(rect, &panel_styles, opacity, &mut out);
    let x = rect.x + pad[3];
    let inner_width = (rect.width - pad[1] - pad[3]).max(1.0);
    let mut y = rect.y + pad[0];
    for (index, quest) in quests.iter().enumerate() {
        if index > 0 {
            y += font_size * 0.5;
        }
        out.push(text_item(
            quest.title.clone(),
            UiRect {
                x,
                y,
                width: inner_width,
                height: title_line,
            },
            title_size,
            &title_styles,
            opacity,
            TextAlign::Left,
        ));
        y += title_line;
        for objective in &quest.objectives {
            let (text, styles) = if objective.done() {
                (format!("{} (done)", objective.text), &done_styles)
            } else if objective.count > 1 {
                (
                    format!(
                        "{} {}/{}",
                        objective.text, objective.progress, objective.count
                    ),
                    &panel_styles,
                )
            } else {
                (objective.text.clone(), &panel_styles)
            };
            out.push(text_item(
                text,
                UiRect {
                    x: x + 8.0,
                    y,
                    width: (inner_width - 8.0).max(1.0),
                    height: line,
                },
                font_size,
                styles,
                opacity,
                TextAlign::Left,
            ));
            y += line;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::style::parse_stylesheet;
    use axcenity_core::dialogue::ObjectiveView;

    fn texts(items: &[UiDrawItem]) -> Vec<(&str, [f32; 4])> {
        items
            .iter()
            .filter_map(|item| match item {
                UiDrawItem::Text(text) => Some((text.text.as_str(), text.color)),
                _ => None,
            })
            .collect()
    }

    fn view() -> DialogueView {
        DialogueView {
            speaker: "Farmer".to_owned(),
            text: "Could you pick me some apples?".to_owned(),
            choices: vec!["Yes".to_owned(), "No".to_owned()],
            selected: 1,
        }
    }

    #[test]
    fn the_box_shows_the_speaker_the_line_and_numbered_choices_and_css_styles_them() {
        let world = World::new();
        let (sheet, warnings) = parse_stylesheet(
            "ui/dialogue.css",
            "DialogueBox { color: #ff0000; width: 400px }\n\
             DialogueBox::key { color: #00ff00 }\n\
             DialogueBox::option { color: #0000ff }\n\
             DialogueBox::option:focus { color: #ffffff; background-color: #123456 }",
        );
        assert!(warnings.is_empty(), "{warnings:?}");
        let items = dialogue_box_items(&world, &view(), &[sheet], 800, 600);
        let found = texts(&items);
        let color = |text: &str| found.iter().find(|(t, _)| *t == text).map(|(_, c)| *c);
        assert_eq!(color("Farmer"), Some([0.0, 1.0, 0.0, 1.0]));
        assert_eq!(
            color("Could you pick me some apples?"),
            Some([1.0, 0.0, 0.0, 1.0])
        );
        assert_eq!(color("1. Yes"), Some([0.0, 0.0, 1.0, 1.0]));
        assert_eq!(color("2. No"), Some([1.0, 1.0, 1.0, 1.0]), "the chosen one");
        // The box is 400 wide, in the middle, at the bottom.
        let UiDrawItem::Rectangle(panel) = &items[0] else {
            panic!("the box first: {:?}", items[0]);
        };
        assert_eq!(panel.size[0], 400.0);
        assert_eq!(panel.position[0], 200.0);
        assert!(panel.position[1] + panel.size[1] <= 600.0 - 23.0);
    }

    #[test]
    fn the_tracker_lists_active_quests_with_their_progress() {
        let world = World::new();
        let quests = [QuestView {
            quest: "apples".to_owned(),
            title: "Apples for the Farmer".to_owned(),
            objectives: vec![
                ObjectiveView {
                    text: "Apples".to_owned(),
                    progress: 3,
                    count: 5,
                },
                ObjectiveView {
                    text: "Back to the Farmer".to_owned(),
                    progress: 1,
                    count: 1,
                },
            ],
        }];
        let items = quest_tracker_items(&world, &quests, &[], 800, 600);
        let found: Vec<&str> = texts(&items).into_iter().map(|(text, _)| text).collect();
        assert_eq!(
            found,
            [
                "Apples for the Farmer",
                "Apples 3/5",
                "Back to the Farmer (done)"
            ]
        );
        assert!(quest_tracker_items(&world, &[], &[], 800, 600).is_empty());
    }
}
