//! The default leaderboard (Phase 47): a table at the top right of the
//! screen with a row for each Player that has a `leaderstats` child (a
//! Folder of Value objects, such as an IntValue named Coins), its name and
//! each stat's value, under a header of the stats' names. Rows are sorted
//! by the first stat, highest first.
//!
//! It's styled like the hotbar: `Players` rules style the board (its
//! `background-color`, `color`, `font-size`, `padding`, `border-radius`,
//! and `margin-top` and `margin-right` from the screen's edges),
//! `Players::key` the header, and `Player` rules each row (`height`,
//! `background-color`, `color`; a Player's own `Class` or `Id` picks one
//! out). What the CSS leaves out comes from defaults written as CSS. With
//! `Players.LeaderboardEnabled` off, it isn't drawn; hosts hide it while
//! the player has toggled it off with Tab.

use super::*;
use crate::prompt::{text_item, text_width};
use crate::style::{PseudoElement, resolve_part_styles};
use crate::widget::{paint_part, px, with_defaults};

/// The name of the child of a Player whose Value objects the leaderboard
/// shows.
pub const LEADERSTATS: &str = "leaderstats";

/// Gap between columns, in pixels at text scale 1.
const COLUMN_GAP: f32 = 18.0;

fn pixels(styles: &ResolvedStyles, name: &str, default: f32) -> f32 {
    css_length_property(styles, name)
        .unwrap_or(default)
        .max(0.0)
}

/// The text a stat shows: an IntValue's whole number, a NumberValue's
/// number (without a trailing `.0`), a StringValue's text or a BoolValue's
/// `true` or `false`. `None` for anything else.
fn stat_text(world: &World, stat: InstanceId) -> Option<String> {
    let instance = world.get(stat)?;
    let value = instance.property("Value")?;
    match instance.class() {
        ClassName::IntValue | ClassName::NumberValue => {
            let n = value.as_number()?;
            Some(if n.fract() == 0.0 && n.abs() < 1e15 {
                format!("{}", n as i64)
            } else {
                format!("{n}")
            })
        }
        ClassName::StringValue => value.as_str().map(str::to_owned),
        ClassName::BoolValue => value.as_bool().map(|b| b.to_string()),
        _ => None,
    }
}

/// One stat of a Player's: its name, the text it shows, and its number
/// for sorting (the lowest there is when it isn't a number).
struct Stat {
    name: String,
    text: String,
    number: f64,
}

/// Each Player with a `leaderstats` child, and its stats.
fn rows(world: &World) -> Vec<(InstanceId, Vec<Stat>)> {
    let Some(players) = world.get(world.players()) else {
        return Vec::new();
    };
    players
        .children()
        .iter()
        .copied()
        .filter(|&player| {
            world
                .get(player)
                .is_some_and(|p| p.class() == ClassName::Player)
        })
        .filter_map(|player| {
            let stats = world.find_first_child(player, LEADERSTATS)?;
            let values = world
                .get(stats)?
                .children()
                .iter()
                .filter_map(|&stat| {
                    let text = stat_text(world, stat)?;
                    let number = world
                        .get(stat)
                        .and_then(|s| s.property("Value"))
                        .and_then(Value::as_number)
                        .unwrap_or(f64::NEG_INFINITY);
                    Some(Stat {
                        name: world.get(stat)?.name().to_owned(),
                        text,
                        number,
                    })
                })
                .collect();
            Some((player, values))
        })
        .collect()
}

/// The draw items for the leaderboard in a `width` × `height` view, to go
/// over the rest of the UI. Nothing while `Players.LeaderboardEnabled` is
/// off or no Player has a `leaderstats` with a stat in it.
pub fn leaderboard_items(
    world: &World,
    stylesheets: &[Stylesheet],
    width: u32,
    height: u32,
) -> Vec<UiDrawItem> {
    let mut out = Vec::new();
    let service = world.players();
    let enabled = world
        .get(service)
        .and_then(|players| players.property("LeaderboardEnabled"))
        .and_then(Value::as_bool)
        != Some(false);
    let mut rows = rows(world);
    // The columns: every stat's name, in the order they're first seen.
    let mut columns: Vec<String> = Vec::new();
    for (_, stats) in &rows {
        for stat in stats {
            if !columns.contains(&stat.name) {
                columns.push(stat.name.clone());
            }
        }
    }
    if !enabled || columns.is_empty() || height == 0 {
        return out;
    }
    let first = columns[0].clone();
    let key = |stats: &[Stat]| {
        stats
            .iter()
            .find(|stat| stat.name == first)
            .map_or(f64::NEG_INFINITY, |stat| stat.number)
    };
    rows.sort_by(|a, b| key(&b.1).total_cmp(&key(&a.1)));

    let scale = world.ui_settings().text_scale as f32;
    let pseudo = PseudoClasses::default();
    let board_styles = with_defaults(
        resolve_styles(world, service, stylesheets, pseudo),
        &[
            ("background-color", "rgba(17, 17, 20, 0.75)".to_owned()),
            ("color", "#ffffff".to_owned()),
            ("border-radius", px(8.0)),
            ("font-weight", "500".to_owned()),
        ],
    );
    let header_styles = with_defaults(
        resolve_part_styles(world, service, stylesheets, pseudo, PseudoElement::Key),
        &[
            ("font-weight", "700".to_owned()),
            (
                "color",
                css_property(&board_styles, "color").unwrap_or_else(|| "#ffffff".to_owned()),
            ),
        ],
    );
    let opacity = css_number(&board_styles, "opacity")
        .unwrap_or(1.0)
        .clamp(0.0, 1.0);
    let padding = css_box_values(&board_styles, "padding").map_or([8.0; 4], |sides| {
        sides.map(|side| match side {
            taffy::LengthPercentage::Length(pixels) => pixels.max(0.0),
            taffy::LengthPercentage::Percent(_) => 0.0,
        })
    });
    let font_size = pixels(&board_styles, "font-size", 14.0).max(1.0) * scale;
    let header_size = pixels(&header_styles, "font-size", 13.0).max(1.0) * scale;
    let top = pixels(&board_styles, "margin-top", 16.0);
    let right = pixels(&board_styles, "margin-right", 16.0);
    let gap = COLUMN_GAP * scale;

    let row_styles: Vec<ResolvedStyles> = rows
        .iter()
        .map(|(player, _)| {
            with_defaults(
                resolve_styles(world, *player, stylesheets, pseudo),
                &[(
                    "color",
                    css_property(&board_styles, "color").unwrap_or_else(|| "#ffffff".to_owned()),
                )],
            )
        })
        .collect();
    let row_height = |styles: &ResolvedStyles| pixels(styles, "height", font_size * 1.7).max(1.0);
    let header_height = header_size * 1.8;

    // Each column as wide as its widest text.
    let names: Vec<&str> = rows
        .iter()
        .map(|(player, _)| world.get(*player).map_or("", |p| p.name()))
        .collect();
    let name_width = names
        .iter()
        .map(|name| text_width(name, font_size))
        .fold(text_width("Player", header_size), f32::max);
    let texts: Vec<Vec<String>> = rows
        .iter()
        .map(|(_, stats)| {
            columns
                .iter()
                .map(|column| {
                    stats
                        .iter()
                        .find(|stat| stat.name == *column)
                        .map_or_else(String::new, |stat| stat.text.clone())
                })
                .collect()
        })
        .collect();
    let widths: Vec<f32> = columns
        .iter()
        .enumerate()
        .map(|(index, column)| {
            texts
                .iter()
                .map(|row| text_width(&row[index], font_size))
                .fold(text_width(column, header_size), f32::max)
        })
        .collect();
    let inner_width = name_width + widths.iter().map(|w| w + gap).sum::<f32>();
    let inner_height = header_height + row_styles.iter().map(row_height).sum::<f32>();
    let rect = UiRect {
        x: width as f32 - right - inner_width - padding[1] - padding[3],
        y: top,
        width: inner_width + padding[1] + padding[3],
        height: inner_height + padding[0] + padding[2],
    };
    paint_part(rect, &board_styles, opacity, &mut out);

    let left = rect.x + padding[3];
    let mut y = rect.y + padding[0];
    // A row: its first text in the name column, the rest in the stats'.
    let cells = |out: &mut Vec<UiDrawItem>,
                 first: String,
                 rest: Vec<String>,
                 y: f32,
                 row: f32,
                 size: f32,
                 styles: &ResolvedStyles| {
        out.push(text_item(
            first,
            UiRect {
                x: left,
                y,
                width: name_width,
                height: row,
            },
            size,
            styles,
            opacity,
            TextAlign::Left,
        ));
        let mut x = left + name_width;
        for (text, column) in rest.into_iter().zip(&widths) {
            x += gap;
            out.push(text_item(
                text,
                UiRect {
                    x,
                    y,
                    width: *column,
                    height: row,
                },
                size,
                styles,
                opacity,
                TextAlign::Right,
            ));
            x += column;
        }
    };
    // The header: "Player", then each stat's name.
    cells(
        &mut out,
        "Player".to_owned(),
        columns.clone(),
        y,
        header_height,
        header_size,
        &header_styles,
    );
    y += header_height;
    for ((name, row), styles) in names.iter().zip(texts).zip(&row_styles) {
        let height = row_height(styles);
        let strip = UiRect {
            x: rect.x,
            y,
            width: rect.width,
            height,
        };
        paint_part(strip, styles, opacity, &mut out);
        let size = pixels(styles, "font-size", font_size / scale).max(1.0) * scale;
        cells(&mut out, (*name).to_owned(), row, y, height, size, styles);
        y += height;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn css(source: &str) -> Vec<Stylesheet> {
        let (sheet, warnings) = parse_stylesheet("ui.css", source);
        assert!(warnings.is_empty(), "{warnings:?}");
        vec![sheet]
    }

    /// A Player with a `leaderstats` Folder holding an IntValue Coins.
    fn player_with_coins(world: &mut World, coins: f64) -> InstanceId {
        let player = world.create(ClassName::Player).unwrap();
        world.set_parent(player, Some(world.players())).unwrap();
        let stats = world.create(ClassName::Folder).unwrap();
        world.set_name(stats, LEADERSTATS).unwrap();
        world.set_parent(stats, Some(player)).unwrap();
        let value = world.create(ClassName::IntValue).unwrap();
        world.set_name(value, "Coins").unwrap();
        world.set_parent(value, Some(stats)).unwrap();
        world
            .set_property(value, "Value", Value::Number(coins))
            .unwrap();
        value
    }

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
    fn coins_in_leaderstats_show_and_follow_the_value() {
        let mut world = World::new();
        assert!(leaderboard_items(&world, &[], 800, 600).is_empty());
        let coins = player_with_coins(&mut world, 5.0);
        let items = leaderboard_items(&world, &[], 800, 600);
        assert_eq!(texts(&items), ["Player", "Coins", "Player", "5"]);
        // At the top right.
        let UiDrawItem::Text(value) = items.last().unwrap() else {
            panic!("the value is text");
        };
        assert!(value.position[0] > 600.0 && value.position[1] < 100.0);

        world
            .set_property(coins, "Value", Value::Number(12.0))
            .unwrap();
        assert_eq!(
            texts(&leaderboard_items(&world, &[], 800, 600))
                .last()
                .unwrap(),
            "12"
        );
    }

    #[test]
    fn it_can_be_turned_off_and_styled_in_css() {
        let mut world = World::new();
        player_with_coins(&mut world, 3.0);
        let sheets = css("Players { background-color: #ff0000; } Player { color: #00ff00; }");
        let items = leaderboard_items(&world, &sheets, 800, 600);
        let red_board = items.iter().any(|item| {
            matches!(item, UiDrawItem::Rectangle(rect) if rect.color[0] > 0.99 && rect.color[1] < 0.01)
        });
        assert!(red_board, "{items:?}");
        let green_row = items.iter().any(|item| {
            matches!(item, UiDrawItem::Text(text) if text.text == "3" && text.color[1] > 0.99 && text.color[0] < 0.01)
        });
        assert!(green_row, "{items:?}");

        world
            .set_property(world.players(), "LeaderboardEnabled", Value::Bool(false))
            .unwrap();
        assert!(leaderboard_items(&world, &sheets, 800, 600).is_empty());
    }
}
