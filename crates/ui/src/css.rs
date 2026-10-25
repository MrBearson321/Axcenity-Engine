use super::*;

pub(super) fn css_property(styles: &ResolvedStyles, name: &str) -> Option<String> {
    let value = styles.values.get(name)?;
    resolve_variables(value, &styles.variables)
}

pub(super) fn css_number(styles: &ResolvedStyles, name: &str) -> Option<f32> {
    let value = css_property(styles, name)?;
    value
        .trim()
        .parse::<f32>()
        .ok()
        .filter(|value| value.is_finite())
}

pub(super) fn css_length_property(styles: &ResolvedStyles, name: &str) -> Option<f32> {
    let value = css_property(styles, name)?;
    value
        .trim()
        .strip_suffix("px")?
        .trim()
        .parse::<f32>()
        .ok()
        .filter(|value| value.is_finite())
}

pub(super) fn css_dimension(styles: &ResolvedStyles, name: &str) -> Option<Dimension> {
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

pub(super) fn css_length(value: &str) -> Option<LengthPercentage> {
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

pub(super) fn css_box_values(styles: &ResolvedStyles, name: &str) -> Option<[LengthPercentage; 4]> {
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

pub(super) fn css_padding(styles: &ResolvedStyles) -> Rect<LengthPercentage> {
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

pub(super) fn css_margin(styles: &ResolvedStyles) -> Rect<LengthPercentageAuto> {
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

pub(super) fn css_side(styles: &ResolvedStyles, name: &str) -> Option<LengthPercentageAuto> {
    let value = css_property(styles, name)?;
    if value.trim() == "auto" {
        Some(LengthPercentageAuto::Auto)
    } else {
        css_length(&value).map(Into::into)
    }
}

pub(super) fn apply_container_style(style: &mut Style, styles: &ResolvedStyles) {
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

pub(super) fn css_color(styles: &ResolvedStyles, name: &str) -> Option<(Color3, f32)> {
    let value = css_property(styles, name)?;
    parse_color(&value)
}

/// A CSS colour (`#rgb`, `#rrggbb`, `rgb()`, `rgba()` or a name), as its
/// colour and opacity. The editor uses it to show colour swatches.
pub fn parse_css_color(value: &str) -> Option<(Color3, f32)> {
    parse_color(value)
}

pub(super) fn parse_color(value: &str) -> Option<(Color3, f32)> {
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

pub(super) fn css_border(styles: &ResolvedStyles) -> (Color3, f32, f32) {
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
pub(super) fn css_gradient(styles: &ResolvedStyles) -> Option<((Color3, f32), (Color3, f32))> {
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
pub(super) fn css_box_shadow(styles: &ResolvedStyles) -> Option<(f32, f32, f32, Color3, f32)> {
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

pub(super) fn split_shadow_color(value: &str) -> Option<(&str, (Color3, f32))> {
    if let Some(start) = value.find("rgb") {
        let color = parse_color(value[start..].trim())?;
        return Some((value[..start].trim(), color));
    }
    let (lengths, color) = value.rsplit_once(char::is_whitespace)?;
    Some((lengths.trim(), parse_color(color.trim())?))
}

pub(super) fn split_css_args(value: &str) -> Vec<&str> {
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
