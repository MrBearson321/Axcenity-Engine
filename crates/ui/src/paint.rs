use super::*;

/// Its text's size in authored pixels: CSS `font-size`, else `TextSize`.
pub(super) fn font_size(instance: &Instance, styles: &ResolvedStyles) -> f32 {
    css_length_property(styles, "font-size")
        .or_else(|| {
            instance
                .property("TextSize")
                .and_then(Value::as_number)
                .map(|n| n as f32)
        })
        .map(finite)
        .unwrap_or(16.0)
        .max(1.0)
}

pub(super) fn paint_instance(
    instance: &Instance,
    rect: UiRect,
    styles: &ResolvedStyles,
    opacity: f32,
    focused: bool,
    out: &mut Vec<UiDrawItem>,
    warnings: &mut Vec<String>,
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
    let (border_color, mut border_width, border_opacity) = css_border(styles);
    let mut border_tint = border_color;
    if focused && border_width <= 0.0 {
        border_width = 2.0;
        border_tint = color_property(instance, "TextColor3");
    }
    let gradient = css_gradient(styles);
    let fill_alpha = (1.0 - transparency) * opacity * css_background.map_or(1.0, |(_, a)| a);
    let (color_end, end_alpha) = gradient.map_or(
        ([color.r, color.g, color.b, fill_alpha], fill_alpha),
        |(start, end)| {
            // A gradient replaces the flat background. `color` below is
            // overwritten by the start color when one was parsed.
            let _ = start;
            (
                [end.0.r, end.0.g, end.0.b, opacity * end.1],
                opacity * end.1,
            )
        },
    );
    let (fill, fill_alpha) = if let Some((start, _)) = gradient {
        ([start.0.r, start.0.g, start.0.b], opacity * start.1)
    } else {
        ([color.r, color.g, color.b], fill_alpha)
    };
    if let Some((ox, oy, blur, shadow, shadow_alpha)) = css_box_shadow(styles) {
        let spread = blur.max(0.0) * 0.5;
        out.push(UiDrawItem::Rectangle(UiRectangle {
            position: [rect.x + ox - spread, rect.y + oy - spread],
            size: [rect.width + spread * 2.0, rect.height + spread * 2.0],
            color: [shadow.r, shadow.g, shadow.b, opacity * shadow_alpha * 0.85],
            color_end: [shadow.r, shadow.g, shadow.b, opacity * shadow_alpha * 0.85],
            border_color: [0.0; 4],
            border_width: 0.0,
            border_radius: css_length_property(styles, "border-radius")
                .unwrap_or(0.0)
                .max(0.0)
                + spread,
        }));
    }
    let radius = css_length_property(styles, "border-radius")
        .unwrap_or(0.0)
        .max(0.0);
    if (fill_alpha > 0.0 && opacity > 0.0) || end_alpha > 0.0 || border_width > 0.0 {
        out.push(UiDrawItem::Rectangle(UiRectangle {
            position: [rect.x, rect.y],
            size: [rect.width, rect.height],
            color: [fill[0], fill[1], fill[2], fill_alpha],
            color_end,
            border_color: [
                border_tint.r,
                border_tint.g,
                border_tint.b,
                opacity
                    * if focused && css_border(styles).1 <= 0.0 {
                        1.0
                    } else {
                        border_opacity
                    },
            ],
            border_width,
            border_radius: radius,
        }));
    }

    let image_path = instance
        .property("Image")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned();
    if !image_path.is_empty()
        && matches!(
            instance.class(),
            ClassName::ImageLabel | ClassName::TextButton
        )
    {
        let image_alpha = 1.0
            - instance
                .property("ImageTransparency")
                .and_then(Value::as_number)
                .map(|n| finite(n as f32).clamp(0.0, 1.0))
                .unwrap_or(0.0);
        if image_alpha > 0.0 && opacity > 0.0 {
            out.push(UiDrawItem::Image(UiImage {
                position: [rect.x, rect.y],
                size: [rect.width, rect.height],
                tint: [1.0, 1.0, 1.0, image_alpha * opacity],
                border_radius: radius,
                path: image_path,
                texture: None,
            }));
        }
    }

    if matches!(
        instance.class(),
        ClassName::TextLabel | ClassName::TextButton | ClassName::TextBox
    ) {
        let typed = instance
            .property("Text")
            .and_then(Value::as_str)
            .expect("every text element has a Text property");
        let placeholder = typed.is_empty() && instance.class() == ClassName::TextBox;
        let text = if placeholder {
            instance
                .property("Placeholder")
                .and_then(Value::as_str)
                .unwrap_or("")
        } else {
            typed
        };
        if !text.is_empty() && rect.width > 0.0 && rect.height > 0.0 {
            out.push(UiDrawItem::Text(UiText {
                position: [rect.x + 4.0, rect.y + 4.0],
                size: [(rect.width - 8.0).max(0.0), (rect.height - 8.0).max(0.0)],
                ..styled_text(instance, styles, opacity, placeholder, text, warnings)
            }));
        }
    }
}

/// `text` as `instance` draws it, with its styles, but not yet placed.
/// `placeholder` text is drawn faint. With `RichText` on, its tags are read
/// into spans; tags that can't be read are drawn as they are, with a line
/// in `warnings`.
pub(super) fn styled_text(
    instance: &Instance,
    styles: &ResolvedStyles,
    opacity: f32,
    placeholder: bool,
    text: &str,
    warnings: &mut Vec<String>,
) -> UiText {
    let font_size = font_size(instance, styles);
    let css_text_color = css_color(styles, "color");
    let color = css_text_color
        .map(|(color, _)| color)
        .unwrap_or_else(|| color_property(instance, "TextColor3"));
    let font_family = css_property(styles, "font-family")
        .map(|family| font_family_name(&family))
        .unwrap_or_else(|| "Montserrat".to_owned());
    // Buttons center their text, as in Roblox; labels start at the left.
    let button = instance.class() == ClassName::TextButton;
    let align = match css_property(styles, "text-align").as_deref().map(str::trim) {
        Some("center") => TextAlign::Center,
        Some("right" | "end") => TextAlign::Right,
        Some(_) => TextAlign::Left,
        None if button => TextAlign::Center,
        None => TextAlign::Left,
    };
    let vertical_align = match css_keyword(styles, "vertical-align").as_deref() {
        Some("top") => VerticalAlign::Top,
        Some("middle") => VerticalAlign::Middle,
        Some("bottom") => VerticalAlign::Bottom,
        _ if button => VerticalAlign::Middle,
        _ => VerticalAlign::Top,
    };
    let decoration = css_keyword(styles, "text-decoration").unwrap_or_default();
    let transform = css_keyword(styles, "text-transform");
    let rich = !placeholder && instance.property("RichText").and_then(Value::as_bool) == Some(true);
    let spans = if rich {
        parse_rich_text(text)
            .map_err(|problem| {
                warnings.push(format!(
                    "\"{}\" has rich text that can't be read ({problem}), so it's drawn as plain text.",
                    instance.name()
                ));
            })
            .ok()
    } else {
        None
    };
    let mut word_start = true;
    let mut transformed = |text: &str, uppercase: bool| match transform.as_deref() {
        _ if uppercase => text.to_uppercase(),
        Some("uppercase") => text.to_uppercase(),
        Some("lowercase") => text.to_lowercase(),
        Some("capitalize") => capitalize(text, &mut word_start),
        _ => text.to_owned(),
    };
    let alpha =
        opacity * css_text_color.map_or(1.0, |(_, a)| a) * if placeholder { 0.45 } else { 1.0 };
    let mut styled = UiText {
        position: [0.0; 2],
        size: [0.0; 2],
        text: String::new(),
        font_size,
        color: [color.r, color.g, color.b, alpha],
        font_family,
        align,
        vertical_align,
        weight: css_font_weight(styles),
        italic: matches!(
            css_keyword(styles, "font-style").as_deref(),
            Some("italic" | "oblique")
        ),
        underline: decoration.contains("underline"),
        line_through: decoration.contains("line-through"),
        stroke: css_text_stroke(styles, opacity),
        shadows: css_text_shadows(styles, opacity),
        letter_spacing: css_letter_spacing(styles, font_size),
        line_height: css_line_height(styles, font_size),
        wrap: !matches!(
            css_keyword(styles, "white-space").as_deref(),
            Some("nowrap" | "pre")
        ),
        scaled: instance.property("TextScaled").and_then(Value::as_bool) == Some(true),
        spans: Vec::new(),
    };
    match spans {
        Some(spans) => {
            styled.spans = spans
                .into_iter()
                .map(|span| {
                    let text = transformed(&span.text, span.style.uppercase);
                    rich_span(&styled, text, &span.style, opacity)
                })
                .collect();
            styled.text = styled.spans.iter().map(|span| span.text.as_str()).collect();
        }
        None => styled.text = transformed(text, false),
    }
    styled
}

/// A rich text span in `base`'s style, changed by its tags.
fn rich_span(base: &UiText, text: String, style: &RichStyle, opacity: f32) -> TextSpan {
    let color = match (style.color, style.opacity) {
        (Some((color, alpha)), _) => [color.r, color.g, color.b, alpha * opacity],
        (None, Some(fade)) => {
            let [r, g, b, a] = base.color;
            [r, g, b, a * fade]
        }
        (None, None) => base.color,
    };
    TextSpan {
        text,
        font_family: style
            .face
            .as_deref()
            .map_or_else(|| base.font_family.clone(), font_family_name),
        font_size: style.size.unwrap_or(base.font_size),
        weight: style.weight.unwrap_or(base.weight),
        italic: base.italic || style.italic,
        underline: base.underline || style.underline,
        line_through: base.line_through || style.strikethrough,
        color,
        stroke: style.stroke.map_or(base.stroke, |stroke| {
            Some(TextStroke {
                width: stroke.thickness,
                color: [
                    stroke.color.r,
                    stroke.color.g,
                    stroke.color.b,
                    stroke.opacity * opacity,
                ],
            })
        }),
    }
}

/// Each word's first letter in upper case (CSS `text-transform: capitalize`).
/// `word_start` carries on from one span to the next.
fn capitalize(text: &str, word_start: &mut bool) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        if *word_start {
            out.extend(ch.to_uppercase());
        } else {
            out.push(ch);
        }
        *word_start = ch.is_whitespace();
    }
    out
}
