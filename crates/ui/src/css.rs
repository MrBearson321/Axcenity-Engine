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

/// How a border is drawn (CSS `border-style`). Dashed, dotted and double
/// borders are drawn solid.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BorderStyle {
    None,
    Solid,
    Inset,
    Outset,
    Ridge,
    Groove,
}

impl BorderStyle {
    fn parse(word: &str) -> Option<BorderStyle> {
        Some(match word.to_ascii_lowercase().as_str() {
            "none" | "hidden" => BorderStyle::None,
            "solid" | "dashed" | "dotted" | "double" => BorderStyle::Solid,
            "inset" => BorderStyle::Inset,
            "outset" => BorderStyle::Outset,
            "ridge" => BorderStyle::Ridge,
            "groove" => BorderStyle::Groove,
            _ => return None,
        })
    }
}

/// One side of a border: its width in pixels, colour and opacity.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct BorderSide {
    pub(crate) width: f32,
    pub(crate) color: Color3,
    pub(crate) alpha: f32,
}

/// An element's border: its sides (top, right, bottom, left) and style.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Borders {
    pub(crate) sides: [BorderSide; 4],
    pub(crate) style: BorderStyle,
}

impl Borders {
    pub(crate) fn widths(&self) -> [f32; 4] {
        self.sides.map(|side| side.width)
    }

    /// The colours of the outer and inner halves of each side. They're
    /// the same but for the 3D styles, whose light and dark edges come
    /// from the border colour: [`lighter`] and [`darker`].
    pub(crate) fn colors(&self) -> ([Color3; 4], [Color3; 4]) {
        let color = self.sides.map(|side| side.color);
        // Top and left, then bottom and right, shaded.
        let shade = |top_left: fn(Color3) -> Color3, bottom_right: fn(Color3) -> Color3| {
            [
                top_left(color[0]),
                bottom_right(color[1]),
                bottom_right(color[2]),
                top_left(color[3]),
            ]
        };
        let sunk = shade(darker, lighter);
        let raised = shade(lighter, darker);
        match self.style {
            BorderStyle::None | BorderStyle::Solid => (color, color),
            BorderStyle::Inset => (sunk, sunk),
            BorderStyle::Outset => (raised, raised),
            BorderStyle::Groove => (sunk, raised),
            BorderStyle::Ridge => (raised, sunk),
        }
    }
}

/// A 3D border's lit edge: halfway from its colour to white.
pub(crate) fn lighter(color: Color3) -> Color3 {
    Color3::new(
        (color.r + 1.0) * 0.5,
        (color.g + 1.0) * 0.5,
        (color.b + 1.0) * 0.5,
    )
}

/// A 3D border's shaded edge: its colour at half the brightness.
pub(crate) fn darker(color: Color3) -> Color3 {
    Color3::new(color.r * 0.5, color.g * 0.5, color.b * 0.5)
}

/// The words of a CSS value, split at spaces outside brackets, so
/// `2px solid rgb(0, 0, 0)` is three words.
pub(super) fn css_words(value: &str) -> Vec<&str> {
    let mut words = Vec::new();
    let mut start = None;
    let mut depth = 0;
    for (index, c) in value.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => depth -= 1,
            _ => {}
        }
        if c.is_whitespace() && depth <= 0 {
            if let Some(from) = start.take() {
                words.push(&value[from..index]);
            }
        } else if start.is_none() {
            start = Some(index);
        }
    }
    if let Some(from) = start {
        words.push(&value[from..]);
    }
    words
}

/// A number of pixels: `4px`, or a bare number.
fn pixels(word: &str) -> Option<f32> {
    word.strip_suffix("px")
        .unwrap_or(word)
        .trim()
        .parse::<f32>()
        .ok()
        .filter(|n| n.is_finite())
}

/// A border width: pixels, or `thin`, `medium` or `thick`.
fn border_width(word: &str) -> Option<f32> {
    match word.to_ascii_lowercase().as_str() {
        "thin" => Some(1.0),
        "medium" => Some(3.0),
        "thick" => Some(5.0),
        _ => pixels(word).map(|width| width.max(0.0)),
    }
}

/// One to four values for top, right, bottom and left, as CSS spreads them.
fn four<T: Copy>(values: &[T]) -> Option<[T; 4]> {
    Some(match *values {
        [all] => [all; 4],
        [vertical, horizontal] => [vertical, horizontal, vertical, horizontal],
        [top, horizontal, bottom] => [top, horizontal, bottom, horizontal],
        [top, right, bottom, left] => [top, right, bottom, left],
        _ => return None,
    })
}

/// A `border` (or `border-top`, …) shorthand's width, style and colour.
type BorderParts = (Option<f32>, Option<BorderStyle>, Option<(Color3, f32)>);

fn border_parts(value: &str) -> BorderParts {
    let (mut width, mut style, mut color) = (None, None, None);
    for word in css_words(value) {
        if let Some(parsed) = border_width(word) {
            width = Some(parsed);
        } else if let Some(parsed) = BorderStyle::parse(word) {
            style = Some(parsed);
        } else if let Some(parsed) = parse_color(word) {
            color = Some(parsed);
        }
    }
    (width, style, color)
}

/// Each side's `border-{side}`, `-width` and `-color` property names,
/// written out so they aren't formatted for every element (M14).
const SIDES: [[&str; 3]; 4] = [
    ["border-top", "border-top-width", "border-top-color"],
    ["border-right", "border-right-width", "border-right-color"],
    [
        "border-bottom",
        "border-bottom-width",
        "border-bottom-color",
    ],
    ["border-left", "border-left-width", "border-left-color"],
];

/// The element's border, from `border`, `border-width`, `border-color`,
/// `border-style` and each side's `border-top` (and so on),
/// `border-top-width` and `border-top-color`; the more specific win. The
/// style defaults to solid, so a width alone draws a border, and the
/// colour to black.
pub(crate) fn css_borders(styles: &ResolvedStyles) -> Borders {
    let mut sides = [BorderSide {
        width: 0.0,
        color: Color3::new(0.0, 0.0, 0.0),
        alpha: 1.0,
    }; 4];
    let mut style = BorderStyle::Solid;
    let set_color = |side: &mut BorderSide, (color, alpha): (Color3, f32)| {
        side.color = color;
        side.alpha = alpha;
    };
    if let Some(border) = css_property(styles, "border") {
        let (width, parsed_style, color) = border_parts(&border);
        for side in &mut sides {
            side.width = width.unwrap_or(side.width);
            if let Some(color) = color {
                set_color(side, color);
            }
        }
        style = parsed_style.unwrap_or(style);
        if parsed_style == Some(BorderStyle::None) {
            sides.iter_mut().for_each(|side| side.width = 0.0);
        }
    }
    if let Some(widths) = css_property(styles, "border-width").and_then(|value| {
        let widths: Option<Vec<f32>> = css_words(&value).into_iter().map(border_width).collect();
        four(&widths?)
    }) {
        for (side, width) in sides.iter_mut().zip(widths) {
            side.width = width;
        }
    }
    if let Some(colors) = css_property(styles, "border-color").and_then(|value| {
        let colors: Option<Vec<_>> = css_words(&value).into_iter().map(parse_color).collect();
        four(&colors?)
    }) {
        for (side, color) in sides.iter_mut().zip(colors) {
            set_color(side, color);
        }
    }
    if let Some(parsed) = css_property(styles, "border-style").and_then(|value| {
        css_words(&value)
            .first()
            .and_then(|word| BorderStyle::parse(word))
    }) {
        style = parsed;
    }
    for (side, [shorthand, width, color]) in sides.iter_mut().zip(SIDES) {
        if let Some(value) = css_property(styles, shorthand) {
            let (width, side_style, color) = border_parts(&value);
            side.width = width.unwrap_or(side.width);
            if let Some(color) = color {
                set_color(side, color);
            }
            if side_style == Some(BorderStyle::None) {
                side.width = 0.0;
            }
        }
        if let Some(width) = css_property(styles, width).and_then(|v| border_width(v.trim())) {
            side.width = width;
        }
        if let Some(color) = css_color(styles, color) {
            set_color(side, color);
        }
    }
    if style == BorderStyle::None {
        sides.iter_mut().for_each(|side| side.width = 0.0);
    }
    Borders { sides, style }
}

/// CSS `border-image`: a picture cut into nine pieces and drawn as the
/// element's frame, its corners kept their size (`ui::paint`).
#[derive(Clone, Debug, PartialEq)]
pub struct BorderImage {
    /// The picture, a project path, from `url(…)`.
    pub path: String,
    /// Where it's cut, in its own pixels: top, right, bottom and left.
    /// The corners are drawn this many design pixels big.
    pub slices: [f32; 4],
    /// Whether the middle piece is drawn (`fill`).
    pub fill: bool,
    /// Whether the edges and middle repeat (`repeat`, `round` or `space`)
    /// rather than stretch (`stretch`, the default).
    pub repeat: bool,
}

impl BorderImage {
    /// Reads `url(path) <slices> [fill] [stretch | repeat]`, with one to
    /// four slices in image pixels. `None` for `none` or anything else.
    pub fn parse(value: &str) -> Option<BorderImage> {
        let mut path = None;
        let mut slices = Vec::new();
        let mut fill = false;
        let mut repeat = false;
        for word in css_words(value) {
            let lower = word.to_ascii_lowercase();
            if lower.starts_with("url(") && word.ends_with(')') {
                let inner = word[4..word.len() - 1].trim();
                path = Some(inner.trim_matches(|c| c == '"' || c == '\'').to_owned());
            } else if lower == "fill" {
                fill = true;
            } else if lower == "stretch" {
                repeat = false;
            } else if matches!(lower.as_str(), "repeat" | "round" | "space") {
                repeat = true;
            } else {
                // Slices come after the picture.
                let slice = pixels(word).filter(|_| path.is_some())?;
                slices.push(slice.max(0.0));
            }
        }
        let path = path.filter(|path| !path.is_empty())?;
        Some(BorderImage {
            path,
            slices: four(&slices)?,
            fill,
            repeat,
        })
    }

    /// As CSS, the way [`parse`](Self::parse) reads it: one slice when
    /// they're all the same.
    pub fn to_css(&self) -> String {
        let number = |value: f32| {
            let text = format!("{:.2}", value.max(0.0));
            text.trim_end_matches('0').trim_end_matches('.').to_owned()
        };
        let [top, right, bottom, left] = self.slices;
        let slices = if [right, bottom, left].iter().all(|&side| side == top) {
            number(top)
        } else {
            self.slices.map(number).join(" ")
        };
        let mut css = format!("url(\"{}\") {slices}", self.path);
        if self.fill {
            css.push_str(" fill");
        }
        if self.repeat {
            css.push_str(" repeat");
        }
        css
    }
}

pub(super) fn css_border_image(styles: &ResolvedStyles) -> Option<BorderImage> {
    BorderImage::parse(&css_property(styles, "border-image")?)
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

/// One `box-shadow`, in pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BoxShadow {
    pub inset: bool,
    pub offset: [f32; 2],
    pub blur: f32,
    pub spread: f32,
    pub color: Color3,
    pub alpha: f32,
}

impl BoxShadow {
    /// One shadow of a `box-shadow` list (see [`split_css_list`]).
    pub fn parse(text: &str) -> Option<BoxShadow> {
        parse_box_shadow(text)
    }
}

/// A CSS list (`box-shadow`, `transition`) split at the commas that
/// aren't inside brackets.
pub fn split_css_list(value: &str) -> Vec<&str> {
    split_css_args(value)
}

/// `box-shadow`: shadows separated by commas, the first on top, each
/// `[inset] <x> <y> [<blur> [<spread>]] [<color>]` (the colour may come
/// first; black if it's left out). `none` is none.
pub(crate) fn css_box_shadows(styles: &ResolvedStyles) -> Vec<BoxShadow> {
    let Some(value) = css_property(styles, "box-shadow") else {
        return Vec::new();
    };
    split_css_args(&value)
        .into_iter()
        .filter_map(parse_box_shadow)
        .collect()
}

fn parse_box_shadow(text: &str) -> Option<BoxShadow> {
    let mut inset = false;
    let mut lengths = Vec::new();
    let mut color = None;
    for word in css_words(text) {
        if word.eq_ignore_ascii_case("inset") {
            inset = true;
        } else if let Some(length) = pixels(word) {
            lengths.push(length);
        } else {
            color = Some(parse_color(word)?);
        }
    }
    let (x, y, blur, spread) = match lengths.as_slice() {
        [x, y] => (*x, *y, 0.0, 0.0),
        [x, y, blur] => (*x, *y, *blur, 0.0),
        [x, y, blur, spread] => (*x, *y, *blur, *spread),
        _ => return None,
    };
    let (color, alpha) = color.unwrap_or((Color3::new(0.0, 0.0, 0.0), 1.0));
    Some(BoxShadow {
        inset,
        offset: [x, y],
        blur: blur.max(0.0),
        spread,
        color,
        alpha,
    })
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

/// A keyword value, trimmed and in lower case, such as `bold`.
pub(super) fn css_keyword(styles: &ResolvedStyles, name: &str) -> Option<String> {
    css_property(styles, name).map(|value| value.trim().to_ascii_lowercase())
}

/// `font-weight`: `normal` is 400, `bold` 700, or a number from 1 to 1000.
pub(super) fn css_font_weight(styles: &ResolvedStyles) -> u16 {
    match css_keyword(styles, "font-weight").as_deref() {
        Some("bold" | "bolder") => 700,
        Some("lighter") => 300,
        Some(number) => number
            .parse::<f32>()
            .ok()
            .filter(|weight| weight.is_finite())
            .map_or(400, |weight| weight.clamp(1.0, 1000.0) as u16),
        None => 400,
    }
}

/// A length for text, in pixels: `px`, `em` (of `font_size`) or a bare `0`.
fn text_length(value: &str, font_size: f32) -> Option<f32> {
    let value = value.trim();
    let number = |text: &str| text.trim().parse::<f32>().ok().filter(|n| n.is_finite());
    if let Some(pixels) = value.strip_suffix("px") {
        number(pixels)
    } else if let Some(ems) = value.strip_suffix("rem") {
        number(ems).map(|ems| ems * 16.0)
    } else if let Some(ems) = value.strip_suffix("em") {
        number(ems).map(|ems| ems * font_size)
    } else {
        number(value).filter(|n| *n == 0.0)
    }
}

/// `letter-spacing` in pixels. `normal` is 0.
pub(super) fn css_letter_spacing(styles: &ResolvedStyles, font_size: f32) -> f32 {
    css_property(styles, "letter-spacing")
        .and_then(|value| text_length(&value, font_size))
        .unwrap_or(0.0)
}

/// `line-height` in pixels: a length, a percentage, or a bare number times
/// the font size. `normal` is `None`, which the renderer draws at 1.2.
pub(super) fn css_line_height(styles: &ResolvedStyles, font_size: f32) -> Option<f32> {
    let value = css_keyword(styles, "line-height")?;
    let height = if let Some(percent) = value.strip_suffix('%') {
        percent
            .trim()
            .parse::<f32>()
            .ok()
            .map(|p| p / 100.0 * font_size)
    } else if let Ok(times) = value.parse::<f32>() {
        Some(times * font_size)
    } else {
        text_length(&value, font_size)
    };
    height.filter(|height| height.is_finite() && *height > 0.0)
}

/// `-webkit-text-stroke` (or `text-stroke`): a width and a colour, such as
/// `2px #000`. The colour defaults to black.
pub(super) fn css_text_stroke(styles: &ResolvedStyles, opacity: f32) -> Option<TextStroke> {
    let value = css_property(styles, "-webkit-text-stroke")
        .or_else(|| css_property(styles, "text-stroke"))?;
    let value = value.trim();
    let (width, color) = value.split_once(char::is_whitespace).unwrap_or((value, ""));
    let width = text_length(width, 16.0)?;
    let (color, alpha) = parse_color(color.trim()).unwrap_or((Color3::new(0.0, 0.0, 0.0), 1.0));
    (width > 0.0).then_some(TextStroke {
        width,
        color: [color.r, color.g, color.b, alpha * opacity],
    })
}

/// `text-shadow`: shadows separated by commas, each `<x> <y> [<blur>]
/// <color>` in pixels (the colour may come first instead). `none` is none.
pub(super) fn css_text_shadows(styles: &ResolvedStyles, opacity: f32) -> Vec<TextShadow> {
    let Some(value) = css_property(styles, "text-shadow") else {
        return Vec::new();
    };
    split_css_args(&value)
        .into_iter()
        .filter_map(|shadow| {
            let shadow = shadow.trim();
            if shadow.is_empty() || shadow.eq_ignore_ascii_case("none") {
                return None;
            }
            let (lengths, (color, alpha)) = split_shadow_color(shadow).or_else(|| {
                let (color, lengths) = shadow.split_once(char::is_whitespace)?;
                Some((lengths.trim(), parse_color(color)?))
            })?;
            let numbers: Vec<f32> = lengths
                .split_whitespace()
                .map(|length| text_length(length, 16.0))
                .collect::<Option<_>>()?;
            let (x, y, blur) = match numbers.as_slice() {
                [x, y] => (*x, *y, 0.0),
                [x, y, blur] => (*x, *y, blur.max(0.0)),
                _ => return None,
            };
            Some(TextShadow {
                offset: [x, y],
                blur,
                color: [color.r, color.g, color.b, alpha * opacity],
            })
        })
        .collect()
}
