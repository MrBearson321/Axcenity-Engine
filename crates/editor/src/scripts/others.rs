//! Others' cursors in a live script or stylesheet tab (C8): each
//! collaborator's caret in their colour with their name above it, and a
//! light band over what they have selected.

use eframe::egui;

/// Where another collaborator's cursor is in a tab's text.
#[derive(Clone, Debug, PartialEq)]
pub struct OtherCursor {
    pub name: String,
    pub color: egui::Color32,
    /// The selection's fixed end, in characters.
    pub anchor: usize,
    /// Where the caret is, in characters.
    pub head: usize,
}

/// Paints `others` over the text box's laid-out text. `text` is what the
/// galley shows: indexes past its end are kept at the end.
pub(super) fn paint(
    ui: &egui::Ui,
    text: &str,
    output: &egui::text_edit::TextEditOutput,
    others: &[OtherCursor],
) {
    if others.is_empty() {
        return;
    }
    let chars = text.chars().count();
    let offset = output.galley_pos.to_vec2();
    let place = |index: usize| {
        output
            .galley
            .pos_from_cursor(egui::text::CCursor::new(index.min(chars)))
            .translate(offset)
    };
    let painter = ui.painter_at(output.response.rect.expand(2.0));
    let font = egui::FontId::proportional(10.0);
    for other in others {
        let (start, end) = if other.anchor <= other.head {
            (other.anchor, other.head)
        } else {
            (other.head, other.anchor)
        };
        if start < end {
            for band in selection_rows(text, start.min(chars), end.min(chars)) {
                let from = place(band.0);
                let to = place(band.1);
                // An empty row of a selection still shows a sliver.
                let right = to.min.x.max(from.min.x + 4.0);
                let rect = egui::Rect::from_min_max(from.min, egui::pos2(right, from.max.y));
                painter.rect_filled(rect, 0.0, other.color.gamma_multiply(0.22));
            }
        }
        let caret = place(other.head);
        painter.line_segment(
            [caret.center_top(), caret.center_bottom()],
            egui::Stroke::new(2.0, other.color),
        );
        // The name sits just above the caret, on a tag of their colour, or
        // below it on the top row.
        let galley = painter.layout_no_wrap(other.name.clone(), font.clone(), egui::Color32::WHITE);
        let size = galley.size() + egui::vec2(6.0, 2.0);
        let above = caret.min.y - size.y;
        let top = if above < output.response.rect.top() {
            caret.max.y
        } else {
            above
        };
        let tag = egui::Rect::from_min_size(egui::pos2(caret.center().x - 1.0, top), size);
        painter.rect_filled(tag, 2.0, other.color);
        painter.galley(tag.min + egui::vec2(3.0, 1.0), galley, egui::Color32::WHITE);
    }
}

/// The selection `start..end` (characters) cut at each line break: one
/// `(from, to)` per line it covers.
pub(super) fn selection_rows(text: &str, start: usize, end: usize) -> Vec<(usize, usize)> {
    let mut rows = Vec::new();
    let mut from = start;
    for (index, c) in text.chars().enumerate().skip(start).take(end - start) {
        if c == '\n' {
            rows.push((from, index));
            from = index + 1;
        }
    }
    if from <= end {
        rows.push((from, end));
    }
    rows
}
