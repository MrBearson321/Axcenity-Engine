//! Rich text: the tags a text element with `RichText` on reads in its
//! `Text`, with Roblox-familiar names. `<b>`, `<i>`, `<u>`, `<s>`,
//! `<font color size face weight transparency>`, `<stroke color thickness
//! transparency>`, `<uc>` (or `<uppercase>`) and `<br/>`, with `&lt;`,
//! `&gt;`, `&amp;`, `&quot;` and `&apos;` for the characters tags use.

use std::ops::Range;

use axcenity_core::Color3;

use crate::parse_color;

/// A piece of rich text and the style its tags give it.
#[derive(Clone, Debug, PartialEq)]
pub struct RichSpan {
    /// What it says, with escapes read (`&lt;` is `<`) and `<br/>` a new line.
    pub text: String,
    /// Where it is in the source, in bytes. Empty for a `<br/>`.
    pub source: Range<usize>,
    pub style: RichStyle,
}

/// What the tags around a piece of rich text change. `None` and `false`
/// keep the element's own style.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RichStyle {
    /// `<b>` is 700; `<font weight>` sets any.
    pub weight: Option<u16>,
    pub italic: bool,
    pub underline: bool,
    pub strikethrough: bool,
    pub uppercase: bool,
    /// The colour and its opacity (from `transparency`).
    pub color: Option<(Color3, f32)>,
    /// `transparency` without a colour, which fades the element's colour.
    pub opacity: Option<f32>,
    /// In the text's own pixels, like `TextSize`.
    pub size: Option<f32>,
    pub face: Option<String>,
    pub stroke: Option<RichStroke>,
}

/// `<stroke>`: an outline around the letters.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RichStroke {
    pub color: Color3,
    pub thickness: f32,
    /// 1 is solid.
    pub opacity: f32,
}

/// Reads the tags in `source`. A tag rich text doesn't know, one left
/// open, a closing tag with nothing to close and a bad attribute are each
/// an error, which says what's wrong: the caller draws the source as plain
/// text instead.
pub fn parse_rich_text(source: &str) -> Result<Vec<RichSpan>, String> {
    let mut spans = Vec::new();
    // Each open tag's name and the style from before it.
    let mut open: Vec<(String, RichStyle)> = Vec::new();
    let mut style = RichStyle::default();
    let mut text = String::new();
    let mut start = 0;
    let mut at = 0;
    while let Some(ch) = source[at..].chars().next() {
        match ch {
            '<' => {
                let Some(length) = source[at..].find('>') else {
                    return Err(format!("`{}` has no closing `>`", excerpt(&source[at..])));
                };
                let tag = &source[at + 1..at + length];
                push(&mut spans, &mut text, start..at, &style);
                at += length + 1;
                start = at;
                read_tag(tag, &mut open, &mut style, &mut spans, at)?;
            }
            '&' => match escape(&source[at..]) {
                Some((ch, length)) => {
                    text.push(ch);
                    at += length;
                }
                None => {
                    text.push('&');
                    at += 1;
                }
            },
            _ => {
                text.push(ch);
                at += ch.len_utf8();
            }
        }
    }
    push(&mut spans, &mut text, start..at, &style);
    if let Some((name, _)) = open.last() {
        return Err(format!("<{name}> is never closed with </{name}>"));
    }
    Ok(spans)
}

/// The text with its tags taken out and escapes read: what rich text says.
pub fn rich_text_plain(source: &str) -> Option<String> {
    parse_rich_text(source)
        .ok()
        .map(|spans| spans.into_iter().map(|span| span.text).collect())
}

/// Ends the span being read, if it says anything.
fn push(spans: &mut Vec<RichSpan>, text: &mut String, source: Range<usize>, style: &RichStyle) {
    if text.is_empty() {
        return;
    }
    spans.push(RichSpan {
        text: std::mem::take(text),
        source,
        style: style.clone(),
    });
}

/// The start of `text`, to quote in an error.
fn excerpt(text: &str) -> String {
    let mut short: String = text.chars().take(16).collect();
    if short.len() < text.len() {
        short.push('…');
    }
    short
}

/// The character an escape at the start of `text` stands for, and its
/// length. An `&` that starts no escape is itself.
fn escape(text: &str) -> Option<(char, usize)> {
    [
        ("&lt;", '<'),
        ("&gt;", '>'),
        ("&amp;", '&'),
        ("&quot;", '"'),
        ("&apos;", '\''),
    ]
    .into_iter()
    .find(|(escape, _)| text.starts_with(escape))
    .map(|(escape, ch)| (ch, escape.len()))
}

/// Applies the tag between `<` and `>`, which ended at `at`.
fn read_tag(
    tag: &str,
    open: &mut Vec<(String, RichStyle)>,
    style: &mut RichStyle,
    spans: &mut Vec<RichSpan>,
    at: usize,
) -> Result<(), String> {
    let tag = tag.trim();
    if let Some(name) = tag.strip_prefix('/') {
        let name = name.trim().to_ascii_lowercase();
        return match open.pop() {
            Some((open_name, before)) if open_name == name => {
                *style = before;
                Ok(())
            }
            Some((open_name, _)) => Err(format!("</{name}> closes <{open_name}>")),
            None => Err(format!("</{name}> has no <{name}> to close")),
        };
    }
    let (tag, self_closing) = match tag.strip_suffix('/') {
        Some(tag) => (tag.trim_end(), true),
        None => (tag, false),
    };
    let (name, attributes) = tag
        .split_once(char::is_whitespace)
        .map_or((tag, ""), |(name, rest)| (name, rest));
    let name = name.to_ascii_lowercase();
    if name == "br" {
        spans.push(RichSpan {
            text: "\n".to_owned(),
            source: at..at,
            style: style.clone(),
        });
        return Ok(());
    }
    if self_closing {
        return Err(format!("<{name}/> can't close itself; only <br/> can"));
    }
    let before = style.clone();
    let attributes = read_attributes(&name, attributes)?;
    match name.as_str() {
        "b" | "i" | "u" | "s" | "uc" | "uppercase" if !attributes.is_empty() => {
            return Err(format!("<{name}> has no attributes"));
        }
        "b" => style.weight = Some(700),
        "i" => style.italic = true,
        "u" => style.underline = true,
        "s" => style.strikethrough = true,
        "uc" | "uppercase" => style.uppercase = true,
        "font" => read_font(&attributes, style)?,
        "stroke" => read_stroke(&attributes, style)?,
        "" => return Err("`<>` is an empty tag (write &lt; for <)".to_owned()),
        _ => return Err(format!("<{name}> isn't a rich text tag")),
    }
    open.push((name, before));
    Ok(())
}

/// `name="value"` pairs, separated by spaces. Names are lowercased.
fn read_attributes(tag: &str, mut text: &str) -> Result<Vec<(String, String)>, String> {
    let mut attributes = Vec::new();
    loop {
        text = text.trim_start();
        if text.is_empty() {
            return Ok(attributes);
        }
        let bad = || format!("<{tag}> has a bad attribute at `{}`", excerpt(text));
        let (name, rest) = text.split_once('=').ok_or_else(bad)?;
        let name = name.trim();
        if name.is_empty() || name.contains(char::is_whitespace) {
            return Err(bad());
        }
        let rest = rest.trim_start();
        let quote = rest
            .chars()
            .next()
            .filter(|quote| matches!(quote, '"' | '\''))
            .ok_or_else(bad)?;
        let (value, after) = rest[1..].split_once(quote).ok_or_else(bad)?;
        attributes.push((name.to_ascii_lowercase(), value.to_owned()));
        text = after;
    }
}

fn read_font(attributes: &[(String, String)], style: &mut RichStyle) -> Result<(), String> {
    let mut transparency = None;
    for (name, value) in attributes {
        let bad = || format!("<font {name}=\"{value}\"> isn't a {name} rich text knows");
        match name.as_str() {
            "color" => {
                let (color, alpha) = parse_color(value).ok_or_else(bad)?;
                style.color = Some((color, alpha));
            }
            "size" => {
                let size = number(value).filter(|size| *size > 0.0).ok_or_else(bad)?;
                style.size = Some(size);
            }
            "face" | "family" => {
                let face = value.trim();
                if face.is_empty() {
                    return Err(bad());
                }
                style.face = Some(face.to_owned());
            }
            "weight" => style.weight = Some(weight(value).ok_or_else(bad)?),
            "transparency" => transparency = Some(fraction(value).ok_or_else(bad)?),
            _ => return Err(format!("<font> has no `{name}` attribute")),
        }
    }
    if let Some(transparency) = transparency {
        match &mut style.color {
            Some((_, alpha)) => *alpha *= 1.0 - transparency,
            None => style.opacity = Some(1.0 - transparency),
        }
    }
    Ok(())
}

fn read_stroke(attributes: &[(String, String)], style: &mut RichStyle) -> Result<(), String> {
    let mut stroke = RichStroke {
        color: Color3::new(0.0, 0.0, 0.0),
        thickness: 1.0,
        opacity: 1.0,
    };
    for (name, value) in attributes {
        let bad = || format!("<stroke {name}=\"{value}\"> isn't a {name} rich text knows");
        match name.as_str() {
            "color" => {
                let (color, alpha) = parse_color(value).ok_or_else(bad)?;
                stroke.color = color;
                stroke.opacity *= alpha;
            }
            "thickness" => {
                stroke.thickness = number(value).filter(|n| *n >= 0.0).ok_or_else(bad)?;
            }
            "transparency" => stroke.opacity *= 1.0 - fraction(value).ok_or_else(bad)?,
            // Roblox's corner styles: every outline here is round.
            "joins" => {}
            _ => return Err(format!("<stroke> has no `{name}` attribute")),
        }
    }
    style.stroke = Some(stroke);
    Ok(())
}

fn number(value: &str) -> Option<f32> {
    value
        .trim()
        .trim_end_matches("px")
        .parse::<f32>()
        .ok()
        .filter(|n| n.is_finite())
}

fn fraction(value: &str) -> Option<f32> {
    number(value).filter(|n| (0.0..=1.0).contains(n))
}

/// A weight by name (as Roblox's `Enum.FontWeight` and CSS say them) or
/// from 100 to 900.
fn weight(value: &str) -> Option<u16> {
    let value = value.trim().to_ascii_lowercase();
    Some(match value.as_str() {
        "thin" => 100,
        "extralight" | "extra-light" | "ultralight" => 200,
        "light" => 300,
        "normal" | "regular" => 400,
        "medium" => 500,
        "semibold" | "semi-bold" | "demibold" => 600,
        "bold" => 700,
        "extrabold" | "extra-bold" | "ultrabold" => 800,
        "heavy" | "black" => 900,
        _ => {
            let weight = value.parse::<f32>().ok()?;
            if !(1.0..=1000.0).contains(&weight) {
                return None;
            }
            weight as u16
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(spans: &[RichSpan]) -> Vec<&str> {
        spans.iter().map(|span| span.text.as_str()).collect()
    }

    #[test]
    fn tags_style_their_text_and_nest() {
        let source = r##"Press <b>E</b> to <font color="#ff0000">open</font>"##;
        let spans = parse_rich_text(source).unwrap();
        assert_eq!(texts(&spans), ["Press ", "E", " to ", "open"]);
        assert_eq!(spans[1].style.weight, Some(700));
        assert_eq!(spans[0].style, RichStyle::default());
        assert_eq!(
            spans[3].style.color,
            Some((Color3::new(1.0, 0.0, 0.0), 1.0))
        );
        assert_eq!(&source[spans[3].source.clone()], "open");

        let spans = parse_rich_text(
            "<i>a<u>b<s>c</s></u></i><uc>d</uc><font size='24' face=\"Bebas Neue\" \
             weight=\"semibold\">e</font><stroke color=\"#000\" thickness=\"2\">f</stroke>",
        )
        .unwrap();
        assert_eq!(texts(&spans), ["a", "b", "c", "d", "e", "f"]);
        assert!(spans[2].style.italic && spans[2].style.underline && spans[2].style.strikethrough);
        assert!(!spans[3].style.italic && spans[3].style.uppercase);
        assert_eq!(spans[4].style.size, Some(24.0));
        assert_eq!(spans[4].style.face.as_deref(), Some("Bebas Neue"));
        assert_eq!(spans[4].style.weight, Some(600));
        assert_eq!(spans[5].style.stroke.unwrap().thickness, 2.0);
    }

    #[test]
    fn line_breaks_and_escapes_are_read() {
        let spans = parse_rich_text("1 &lt; 2 &amp;&amp; 3 &gt; 2<br/>next<br />&nbsp;").unwrap();
        assert_eq!(
            texts(&spans),
            ["1 < 2 && 3 > 2", "\n", "next", "\n", "&nbsp;"]
        );
        assert!(spans[1].source.is_empty());
        assert_eq!(rich_text_plain("<b>a</b>&lt;b").as_deref(), Some("a<b"));
    }

    #[test]
    fn malformed_tags_are_errors_that_say_what_is_wrong() {
        for (source, says) in [
            ("<b>open", "<b> is never closed"),
            ("open</b>", "</b> has no <b>"),
            ("<b><i>x</b></i>", "</b> closes <i>"),
            ("<blink>x</blink>", "<blink> isn't a rich text tag"),
            ("a < b", "has no closing"),
            ("<font colour=\"red\">x</font>", "no `colour` attribute"),
            ("<font color=\"nope\">x</font>", "isn't a color"),
            ("<font size=24>x</font>", "bad attribute"),
            ("<b/>", "can't close itself"),
        ] {
            let error = parse_rich_text(source).unwrap_err();
            assert!(error.contains(says), "{source}: {error}");
        }
    }
}
