//! Rich text in the box typed over on the artboard: the source drawn with
//! its tags' styles (the tags themselves small and faint), and a selection
//! wrapped in tags by B, I, U, S, Ctrl+B, Ctrl+I, Ctrl+U or a colour.

use std::ops::Range;

use axcenity_ui::parse_rich_text;
use eframe::egui::{self, text::LayoutJob};

use super::Toggle;
use crate::theme;

/// What a selection is wrapped in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Toggle(Toggle),
    /// `<font color="#rrggbb">`.
    Color([u8; 3]),
}

/// Ctrl+B, Ctrl+I and Ctrl+U (Cmd on a Mac).
pub const SHORTCUTS: [(egui::Key, Toggle); 3] = [
    (egui::Key::B, Toggle::Bold),
    (egui::Key::I, Toggle::Italic),
    (egui::Key::U, Toggle::Underline),
];

/// The colours offered over the box: white, black, red, orange, yellow,
/// green, blue and purple.
pub const SWATCHES: [[u8; 3]; 8] = [
    [0xff, 0xff, 0xff],
    [0x00, 0x00, 0x00],
    [0xef, 0x47, 0x6f],
    [0xff, 0x9f, 0x1c],
    [0xff, 0xd1, 0x66],
    [0x06, 0xd6, 0xa0],
    [0x3a, 0x86, 0xff],
    [0x9b, 0x5d, 0xe5],
];

impl Format {
    fn tags(self) -> (String, &'static str) {
        match self {
            Format::Toggle(Toggle::Bold) => ("<b>".to_owned(), "</b>"),
            Format::Toggle(Toggle::Italic) => ("<i>".to_owned(), "</i>"),
            Format::Toggle(Toggle::Underline) => ("<u>".to_owned(), "</u>"),
            Format::Toggle(Toggle::Strikethrough) => ("<s>".to_owned(), "</s>"),
            Format::Color([r, g, b]) => (
                format!("<font color=\"#{r:02x}{g:02x}{b:02x}\">"),
                "</font>",
            ),
        }
    }
}

/// Wraps the characters `selection` of `text` in `format`'s tags, and
/// returns the new text and the characters to select in it: the text
/// inside the tags, so the same format again takes them off. A selection
/// already wrapped in just those tags is unwrapped instead (a colour is
/// changed). Tags inside the selection are left where they are and the
/// text between them is wrapped piece by piece, so tags stay nested.
/// `None` when the selection holds no text.
pub fn apply(
    text: &str,
    selection: Range<usize>,
    format: Format,
) -> Option<(String, Range<usize>)> {
    let byte = |chars: usize| {
        text.char_indices()
            .nth(chars)
            .map_or(text.len(), |(at, _)| at)
    };
    let (start, end) = (byte(selection.start), byte(selection.end));
    if start >= end {
        return None;
    }
    let (open, close) = format.tags();
    let (before, inside, after) = (&text[..start], &text[start..end], &text[end..]);

    // Already wrapped in exactly these tags: take them off (or recolour).
    if !inside.contains('<') && after.starts_with(close) {
        let opened = match format {
            // Only a tag of just a colour, as this makes.
            Format::Color(_) => before
                .rfind("<font color=")
                .filter(|&at| before[at..].len() == open.len() && before.ends_with("\">")),
            Format::Toggle(_) => before.ends_with(&open).then(|| start - open.len()),
        };
        if let Some(at) = opened {
            let rewrapped = match format {
                Format::Color(_) if before[at..] != open => format!("{open}{inside}{close}"),
                _ => inside.to_owned(),
            };
            let new = format!("{}{rewrapped}{}", &text[..at], &after[close.len()..]);
            let first = text[..at].chars().count();
            let selected = match rewrapped.len() == inside.len() {
                true => first..first + inside.chars().count(),
                false => {
                    first + open.chars().count()
                        ..first + open.chars().count() + inside.chars().count()
                }
            };
            return Some((new, selected));
        }
    }

    // The selection, less any tags, which are kept as they are. An end
    // that falls inside a tag is moved out of it.
    let tags = tag_ranges(text);
    let mut pieces: Vec<Range<usize>> = Vec::new();
    let mut at = start;
    for tag in tags.iter().filter(|tag| tag.end > start && tag.start < end) {
        if tag.start > at {
            pieces.push(at..tag.start);
        }
        at = at.max(tag.end);
    }
    if at < end {
        pieces.push(at..end);
    }
    pieces.retain(|piece| !text[piece.clone()].trim().is_empty());
    let (first, last) = (pieces.first()?.start, pieces.last()?.end);
    let mut new = text[..first].to_owned();
    let mut cursor = first;
    for piece in &pieces {
        new.push_str(&text[cursor..piece.start]);
        new.push_str(&open);
        new.push_str(&text[piece.clone()]);
        new.push_str(close);
        cursor = piece.end;
    }
    let mut selected = text[..first].chars().count()..new.chars().count();
    if pieces.len() == 1 {
        selected.start += open.chars().count();
        selected.end -= close.chars().count();
    }
    new.push_str(&text[last..]);
    Some((new, selected))
}

/// Where each `<…>` in `text` is, in bytes.
fn tag_ranges(text: &str) -> Vec<Range<usize>> {
    let mut tags = Vec::new();
    let mut from = 0;
    while let Some(open) = text[from..].find('<').map(|at| at + from) {
        let Some(close) = text[open..].find('>').map(|at| at + open) else {
            break;
        };
        tags.push(open..close + 1);
        from = close + 1;
    }
    tags
}

/// The source of rich text laid out in the typing box at `size` points: each
/// piece in its tags' style (`base_size` is the element's own text size, which
/// `<font size>` is measured against), and the tags small and faint. Text that
/// can't be read is laid out plain.
pub fn layout_job(
    source: &str,
    size: f32,
    base_size: f32,
    visuals: &egui::Visuals,
    wrap_width: f32,
) -> LayoutJob {
    let plain = egui::TextFormat::simple(egui::FontId::proportional(size), visuals.text_color());
    let mut job = LayoutJob::default();
    job.wrap.max_width = wrap_width;
    let Ok(spans) = parse_rich_text(source) else {
        job.append(source, 0.0, plain);
        return job;
    };
    let tag = egui::TextFormat::simple(
        egui::FontId::proportional((size * 0.6).max(6.0)),
        visuals.weak_text_color().gamma_multiply(0.7),
    );
    let mut at = 0;
    for span in spans.iter().filter(|span| !span.source.is_empty()) {
        if span.source.start > at {
            job.append(&source[at..span.source.start], 0.0, tag.clone());
        }
        let style = &span.style;
        let span_size = style
            .size
            .map_or(size, |points| size * points / base_size.max(1.0))
            .clamp(6.0, 256.0);
        let family = if style.weight.is_some_and(|weight| weight >= 600) {
            theme::heading_family()
        } else {
            egui::FontFamily::Proportional
        };
        let color = style.color.map_or(visuals.text_color(), |(color, alpha)| {
            let byte = |channel: f32| (channel.clamp(0.0, 1.0) * 255.0).round() as u8;
            egui::Color32::from_rgba_unmultiplied(
                byte(color.r),
                byte(color.g),
                byte(color.b),
                byte(alpha),
            )
        });
        let line = |on: bool| {
            if on {
                egui::Stroke::new((span_size / 14.0).max(1.0), color)
            } else {
                egui::Stroke::NONE
            }
        };
        let format = egui::TextFormat {
            font_id: egui::FontId::new(span_size, family),
            color,
            italics: style.italic,
            underline: line(style.underline),
            strikethrough: line(style.strikethrough),
            ..Default::default()
        };
        job.append(&source[span.source.clone()], 0.0, format);
        at = span.source.end;
    }
    if at < source.len() {
        job.append(&source[at..], 0.0, tag);
    }
    job
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bold(text: &str, selection: Range<usize>) -> (String, Range<usize>) {
        apply(text, selection, Format::Toggle(Toggle::Bold)).unwrap()
    }

    #[test]
    fn a_selected_word_is_wrapped_and_unwrapped_again() {
        let (text, selected) = bold("Press E to open", 6..7);
        assert_eq!(text, "Press <b>E</b> to open");
        assert_eq!(selected, 9..10);
        // Bold again takes the tags off.
        let (text, selected) = bold(&text, selected);
        assert_eq!(text, "Press E to open");
        assert_eq!(selected, 6..7);
        let (red, _) = apply("to open", 3..7, Format::Color([255, 0, 0])).unwrap();
        assert_eq!(red, "to <font color=\"#ff0000\">open</font>");
        // Another colour on the same word changes the colour.
        let (blue, selected) = apply(&red, 25..29, Format::Color([0, 0, 255])).unwrap();
        assert_eq!(blue, "to <font color=\"#0000ff\">open</font>");
        assert_eq!(&blue[selected], "open");
        assert!(apply("abc", 1..1, Format::Toggle(Toggle::Italic)).is_none());
    }

    #[test]
    fn tags_in_the_selection_stay_nested() {
        // "E</b> to" across the end of a bold tag.
        let text = "Press <b>E</b> to open";
        let (italic, _) = apply(text, 9..17, Format::Toggle(Toggle::Italic)).unwrap();
        assert_eq!(italic, "Press <b><i>E</i></b><i> to</i> open");
        assert!(parse_rich_text(&italic).is_ok());
        // Ends inside a tag are moved out of it.
        let (under, _) = apply(text, 7..12, Format::Toggle(Toggle::Underline)).unwrap();
        assert!(parse_rich_text(&under).is_ok(), "{under}");
        assert_eq!(under, "Press <b><u>E</u></b> to open");
        // Non-ASCII text is counted in characters.
        let (text, selected) = bold("héllo wörld", 6..11);
        assert_eq!(text, "héllo <b>wörld</b>");
        assert_eq!(selected, 9..14);
    }

    #[test]
    fn the_source_is_laid_out_in_its_tags_styles() {
        let visuals = egui::Visuals::dark();
        let source = "a <b>b</b> <font color=\"#ff0000\" size=\"32\">c</font>";
        let job = layout_job(source, 16.0, 16.0, &visuals, f32::INFINITY);
        let text_of = |section: &egui::text::LayoutSection| &source[section.byte_range.clone()];
        let section = |text: &str| {
            job.sections
                .iter()
                .find(|section| text_of(section) == text)
                .unwrap()
        };
        assert_eq!(section("b").format.font_id.family, theme::heading_family());
        assert_eq!(
            section("c").format.color,
            egui::Color32::from_rgb(255, 0, 0)
        );
        assert_eq!(section("c").format.font_id.size, 32.0);
        assert!(section("<b>").format.font_id.size < 16.0);
        assert_eq!(job.text, source);
        // Unreadable rich text is laid out as it is.
        let job = layout_job("<b>open", 16.0, 16.0, &visuals, f32::INFINITY);
        assert_eq!(job.sections.len(), 1);
    }
}
