//! The inspector's Slice editor, for a Frame, TextButton or ImageLabel:
//! a picture drawn as the element's frame (CSS `border-image`), cut into
//! nine pieces whose corners keep their size. It shows the picture with
//! the four cuts as lines to drag, and writes `border-image` into the
//! element's own `#Id` rule.

use std::sync::Arc;

use axcenity_core::{ClassName, InstanceId};
use axcenity_ui::{BorderImage, UiState};
use eframe::egui;

use super::{DesignerAction, DesignerScene, UiDesigner};
use crate::responsive;
use crate::theme;
use crate::widgets;

/// The preview's largest side, in points.
const PREVIEW: f32 = 200.0;
/// How near a line the pointer picks it up, in points.
const GRAB: f32 = 6.0;
/// The sides, in `BorderImage::slices` order.
const SIDES: [&str; 4] = ["Top", "Right", "Bottom", "Left"];

/// Whether `class` can have a sliced frame.
pub fn sliceable(class: ClassName) -> bool {
    matches!(
        class,
        ClassName::Frame | ClassName::TextButton | ClassName::ImageLabel
    )
}

/// The cut nearest `pointer` within [`GRAB`] points, of a picture
/// `image` pixels big drawn on `preview`: 0 top, 1 right, 2 bottom, 3 left.
pub fn line_at(
    preview: egui::Rect,
    image: [f32; 2],
    slices: [f32; 4],
    pointer: egui::Pos2,
) -> Option<usize> {
    let scale = preview.width() / image[0].max(1.0);
    let [top, right, bottom, left] = slices.map(|slice| slice * scale);
    [
        (pointer.y - (preview.top() + top)).abs(),
        (pointer.x - (preview.right() - right)).abs(),
        (pointer.y - (preview.bottom() - bottom)).abs(),
        (pointer.x - (preview.left() + left)).abs(),
    ]
    .into_iter()
    .enumerate()
    .filter(|(_, distance)| *distance <= GRAB)
    .min_by(|a, b| a.1.total_cmp(&b.1))
    .map(|(side, _)| side)
}

/// `slices` with cut `side` moved to `pointer`, in whole image pixels,
/// kept inside the picture and short of the opposite cut.
pub fn dragged_slices(
    preview: egui::Rect,
    image: [f32; 2],
    mut slices: [f32; 4],
    side: usize,
    pointer: egui::Pos2,
) -> [f32; 4] {
    let scale = preview.width() / image[0].max(1.0);
    let (along, length) = match side {
        0 => (pointer.y - preview.top(), image[1]),
        1 => (preview.right() - pointer.x, image[0]),
        2 => (preview.bottom() - pointer.y, image[1]),
        _ => (pointer.x - preview.left(), image[0]),
    };
    let opposite = slices[(side + 2) % 4];
    slices[side] = (along / scale)
        .round()
        .clamp(0.0, (length - opposite).max(0.0));
    slices
}

impl UiDesigner {
    /// The Slice section.
    pub(super) fn slice_editor(
        &mut self,
        ui: &mut egui::Ui,
        scene: &DesignerScene,
        id: InstanceId,
        actions: &mut Vec<DesignerAction>,
    ) {
        let current = axcenity_ui::style_value_in_state(
            scene.world,
            id,
            scene.stylesheets,
            UiState::Normal,
            "border-image",
        )
        .and_then(|value| BorderImage::parse(&value));
        ui.add_space(4.0);
        widgets::section_label(ui, "Slice");
        let mut write = |value: String, dragging: bool| {
            actions.push(DesignerAction::TextStyle {
                id,
                property: "border-image",
                value,
                dragging,
            });
        };

        // The picture's path, typed: written when the box is left. While
        // it isn't being typed in, it shows what the CSS says.
        let edit_id = egui::Id::new(("ui designer slice path", id));
        let typing = ui.memory(|memory| memory.has_focus(edit_id));
        let shown = current
            .as_ref()
            .map_or(String::new(), |image| image.path.clone());
        let typed = match &mut self.slice_path {
            Some((for_id, path)) if *for_id == id && typing => path,
            draft => &mut draft.insert((id, shown)).1,
        };
        let mut chosen = None;
        responsive::form(ui, ("ui designer slice", id), |ui| {
            ui.label("Image");
            let response = ui.add(
                egui::TextEdit::singleline(typed)
                    .id(edit_id)
                    .hint_text("assets/ui/frame.png")
                    .desired_width(f32::INFINITY),
            );
            if response.lost_focus() {
                chosen = Some(typed.trim().to_owned());
            }
            ui.end_row();
        });
        let picture = |path: &str| scene.assets.texture(path);
        if let Some(path) = chosen
            && current.as_ref().map(|image| image.path.as_str()) != Some(path.as_str())
        {
            if path.is_empty() {
                write("none".to_owned(), false);
            } else {
                // A new picture is cut a quarter of the way in, middle drawn.
                let slices = current.as_ref().map_or_else(
                    || {
                        let quarter = picture(&path).map_or(16.0, |image| {
                            (image.width.min(image.height) as f32 / 4.0).round()
                        });
                        [quarter; 4]
                    },
                    |image| image.slices,
                );
                write(
                    BorderImage {
                        path,
                        slices,
                        fill: current.as_ref().is_none_or(|image| image.fill),
                        repeat: current.as_ref().is_some_and(|image| image.repeat),
                    }
                    .to_css(),
                    false,
                );
            }
            return;
        }
        let Some(mut frame) = current else {
            ui.add(
                egui::Label::new(
                    egui::RichText::new(
                        "A picture to draw as its frame. Its corners keep their size and the \
                         rest stretches (CSS border-image).",
                    )
                    .weak(),
                )
                .wrap(),
            );
            return;
        };

        // The picture with its four cuts.
        let image = picture(&frame.path);
        match &image {
            Some(image) => {
                let key = Arc::as_ptr(image) as usize;
                if self
                    .slice_image
                    .as_ref()
                    .is_none_or(|(seen, _)| *seen != key)
                {
                    let pixels = egui::ColorImage::from_rgba_unmultiplied(
                        [image.width as usize, image.height as usize],
                        image.rgba(),
                    );
                    let handle = ui.ctx().load_texture(
                        "ui designer slice",
                        pixels,
                        egui::TextureOptions::LINEAR,
                    );
                    self.slice_image = Some((key, handle));
                }
                let size = [image.width as f32, image.height as f32];
                if let Some(slices) = self.slice_preview(ui, size, frame.slices) {
                    frame.slices = slices.0;
                    write(frame.to_css(), slices.1);
                    return;
                }
            }
            None => {
                ui.label(egui::RichText::new("The picture can't be read yet.").weak());
            }
        }

        let mut edited = None;
        responsive::form(ui, ("ui designer slice cuts", id), |ui| {
            let most = image
                .as_ref()
                .map_or(4096.0, |image| image.width.max(image.height) as f32);
            for (side, label) in SIDES.into_iter().enumerate() {
                ui.label(label);
                let response = ui.add(
                    egui::DragValue::new(&mut frame.slices[side])
                        .range(0.0..=most)
                        .speed(0.5)
                        .suffix(" px"),
                );
                if response.changed() {
                    edited = Some(response.dragged());
                }
                ui.end_row();
            }
        });
        ui.horizontal_wrapped(|ui| {
            if ui
                .checkbox(&mut frame.fill, "Fill middle")
                .on_hover_text("Draw the middle piece too (CSS fill)")
                .changed()
            {
                edited = Some(false);
            }
            for (label, repeat, tip) in [
                ("Stretch", false, "Edges and middle stretch to fit"),
                (
                    "Repeat",
                    true,
                    "Edges and middle are tiled at the corners' scale",
                ),
            ] {
                if ui
                    .selectable_label(frame.repeat == repeat, label)
                    .on_hover_text(tip)
                    .clicked()
                    && frame.repeat != repeat
                {
                    frame.repeat = repeat;
                    edited = Some(false);
                }
            }
            if ui
                .button("Remove")
                .on_hover_text("No sliced frame (border-image: none)")
                .clicked()
            {
                write("none".to_owned(), false);
            }
        });
        if let Some(dragging) = edited {
            write(frame.to_css(), dragging);
        }
    }

    /// Draws the picture (`size` pixels) with its cuts, and returns new
    /// cuts while one is dragged, with whether the drag goes on.
    fn slice_preview(
        &mut self,
        ui: &mut egui::Ui,
        size: [f32; 2],
        slices: [f32; 4],
    ) -> Option<([f32; 4], bool)> {
        let (_, texture) = self.slice_image.as_ref()?;
        let width = ui.available_width().min(PREVIEW);
        let scale = (width / size[0]).min(PREVIEW / size[1]).max(0.01);
        let (rect, response) = ui.allocate_exact_size(
            egui::vec2(size[0] * scale, size[1] * scale),
            egui::Sense::drag(),
        );
        response.widget_info(|| {
            egui::WidgetInfo::labeled(egui::WidgetType::Other, true, "Slice preview")
        });
        let palette = theme::palette(ui.ctx());
        let painter = ui.painter_at(rect.expand(1.0));
        painter.rect_filled(rect, 0.0, palette.inset);
        painter.image(
            texture.id(),
            rect,
            egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
            egui::Color32::WHITE,
        );
        let [top, right, bottom, left] = slices.map(|slice| slice * scale);
        let stroke = egui::Stroke::new(1.5_f32, palette.accent);
        painter.hline(rect.x_range(), rect.top() + top, stroke);
        painter.hline(rect.x_range(), rect.bottom() - bottom, stroke);
        painter.vline(rect.left() + left, rect.y_range(), stroke);
        painter.vline(rect.right() - right, rect.y_range(), stroke);

        let pointer = response.interact_pointer_pos().or(response.hover_pos());
        if response.drag_started()
            && let Some(at) = response.interact_pointer_pos()
        {
            self.slice_drag = line_at(rect, size, slices, at);
        }
        let hovered = self
            .slice_drag
            .or_else(|| pointer.and_then(|at| line_at(rect, size, slices, at)));
        if let Some(side) = hovered {
            ui.ctx().set_cursor_icon(if side % 2 == 0 {
                egui::CursorIcon::ResizeVertical
            } else {
                egui::CursorIcon::ResizeHorizontal
            });
        }
        let side = self.slice_drag?;
        if response.drag_stopped() {
            self.slice_drag = None;
        }
        let at = pointer?;
        let moved = dragged_slices(rect, size, slices, side, at);
        (moved != slices).then_some((moved, self.slice_drag.is_some()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_cut_lines_are_picked_up_and_dragged_in_image_pixels() {
        // A 100 × 100 picture drawn 200 points across: 2 points a pixel.
        let preview = egui::Rect::from_min_size(egui::pos2(10.0, 10.0), egui::vec2(200.0, 200.0));
        let image = [100.0, 100.0];
        let slices = [10.0, 20.0, 30.0, 40.0];
        assert_eq!(
            line_at(preview, image, slices, egui::pos2(100.0, 31.0)),
            Some(0)
        );
        assert_eq!(
            line_at(preview, image, slices, egui::pos2(170.0, 100.0)),
            Some(1)
        );
        assert_eq!(
            line_at(preview, image, slices, egui::pos2(100.0, 150.0)),
            Some(2)
        );
        assert_eq!(
            line_at(preview, image, slices, egui::pos2(90.0, 100.0)),
            Some(3)
        );
        assert_eq!(
            line_at(preview, image, slices, egui::pos2(120.0, 120.0)),
            None
        );
        assert_eq!(
            dragged_slices(preview, image, slices, 0, egui::pos2(0.0, 60.0)),
            [25.0, 20.0, 30.0, 40.0]
        );
        // Not past the opposite cut, nor outside the picture.
        assert_eq!(
            dragged_slices(preview, image, slices, 3, egui::pos2(500.0, 0.0)),
            [10.0, 20.0, 30.0, 80.0]
        );
        assert_eq!(
            dragged_slices(preview, image, slices, 1, egui::pos2(500.0, 0.0)),
            [10.0, 0.0, 30.0, 40.0]
        );
    }
}
