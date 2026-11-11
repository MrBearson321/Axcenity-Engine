//! Layouts that adapt to a panel's shape, so a panel docked into a tall,
//! narrow column keeps everything inside it: a toolbar whose controls wrap
//! onto more rows, a two-column form whose values move under their names,
//! and two panes that sit side by side or stack.

use eframe::egui;

/// Below this width, a [`form`]'s values go under their names.
pub const FORM_STACK_BELOW: f32 = 280.0;
/// What a stacked form leaves beside each slider for its number.
const SLIDER_NUMBER_ROOM: f32 = 110.0;

/// Controls in a row that wrap onto the next row when they run out of room.
pub fn toolbar<R>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        add(ui)
    })
    .inner
}

/// In a [`toolbar`], starts the next row unless `width` points are left on
/// this one. Wrapping only moves single widgets, so a wide control built from
/// several (a combo box) needs this before it.
pub fn room_for(ui: &mut egui::Ui, width: f32) {
    if ui.available_size_before_wrap().x < width {
        ui.end_row();
    }
}

/// Names and their controls, two to a row (`rows` adds a name, then a
/// control, then calls `ui.end_row()`). When there's room they're a grid
/// whose name column shrinks with the panel; in a narrow panel each control
/// goes under its name, and sliders fill the width.
pub fn form<R>(
    ui: &mut egui::Ui,
    id_salt: impl std::hash::Hash + std::fmt::Debug,
    rows: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    let width = ui.available_width();
    if width < FORM_STACK_BELOW {
        // `end_row` does nothing outside a grid, so the same rows work here.
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 3.0;
            // Leaves room for the slider's number box beside it, with a
            // unit such as " studs".
            ui.spacing_mut().slider_width = (width - SLIDER_NUMBER_ROOM).max(60.0);
            rows(ui)
        })
        .inner
    } else {
        egui::Grid::new(id_salt)
            .num_columns(2)
            .min_col_width(name_width(width))
            .spacing([10.0, 5.0])
            .show(ui, rows)
            .inner
    }
}

/// The width of a [`form`]'s name column in a panel `width` wide.
pub fn name_width(width: f32) -> f32 {
    (width * 0.38).clamp(64.0, 150.0)
}

/// Two panes: side by side with pane 0 taking `share` of the width, or
/// pane 0 above pane 1 when the space is narrower than `stack_below`. A
/// separator goes between them either way.
///
/// `pane` draws the pane it's given the number of. It's one closure rather
/// than two so both panes can change the same things: two closures would
/// each need their own `&mut` borrow at once, which Rust doesn't allow.
pub fn split<R>(
    ui: &mut egui::Ui,
    stack_below: f32,
    share: f32,
    mut pane: impl FnMut(&mut egui::Ui, usize) -> R,
) -> [R; 2] {
    let width = ui.available_width();
    if width < stack_below {
        let first = ui.vertical(|ui| pane(ui, 0)).inner;
        ui.separator();
        let second = ui.vertical(|ui| pane(ui, 1)).inner;
        return [first, second];
    }
    ui.horizontal_top(|ui| {
        let height = ui.available_height();
        let top_down = egui::Layout::top_down(egui::Align::Min);
        let first = ui
            .allocate_ui_with_layout(egui::vec2(width * share, height), top_down, |ui| {
                pane(ui, 0)
            })
            .inner;
        ui.separator();
        let second = ui
            .allocate_ui_with_layout(egui::vec2(ui.available_width(), height), top_down, |ui| {
                pane(ui, 1)
            })
            .inner;
        [first, second]
    })
    .inner
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Draws `add` in a panel `width` wide and returns how wide it drew.
    fn drawn_width(width: f32, add: impl FnOnce(&mut egui::Ui)) -> (f32, f32) {
        let ctx = crate::theme::test_context();
        let mut add = Some(add);
        let mut out = (0.0, 0.0);
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(width, 600.0),
            )),
            ..Default::default()
        };
        let _ = crate::theme::run(&ctx, input, |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                let room = ui.max_rect().width();
                if let Some(add) = add.take() {
                    add(ui);
                }
                out = (ui.min_rect().width(), room);
            });
        });
        out
    }

    #[test]
    fn a_toolbar_wraps_instead_of_running_past_the_panel() {
        let (used, room) = drawn_width(200.0, |ui| {
            toolbar(ui, |ui| {
                for name in ["Import", "Save prefab", "Refresh", "Settings", "More"] {
                    let _ = ui.button(name);
                }
            });
        });
        assert!(used <= room + 0.5, "{used} > {room}");
    }

    #[test]
    fn a_form_stacks_when_narrow_and_two_panes_stack_too() {
        for width in [200.0, 600.0] {
            let (used, room) = drawn_width(width, |ui| {
                form(ui, "form", |ui| {
                    ui.label("Transparency");
                    let mut value = 0.5;
                    ui.add(egui::Slider::new(&mut value, 0.0..=1.0));
                    ui.end_row();
                });
                split(ui, 400.0, 0.5, |ui, pane| {
                    ui.label(format!("pane {pane}, with some words to take up room"))
                });
            });
            assert!(used <= room + 0.5, "at {width}: {used} > {room}");
        }
    }
}
