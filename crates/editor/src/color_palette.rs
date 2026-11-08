//! Color swatches everywhere a color is picked: a preset grid for fast
//! prototyping, and the user's own palettes, which are saved with the
//! editor's settings rather than in a project, so every project, and both
//! 3D parts and UI elements, share them.
//!
//! [`edit_srgb`] and [`edit_srgba`] replace egui's color buttons. The
//! [`Library`] lives behind a [`Handle`] that the app shares with egui's
//! memory, so a control anywhere can reach it without being passed it.

use std::sync::{Arc, Mutex};

use eframe::egui::{self, Color32, color_picker::Alpha};
use serde::{Deserialize, Serialize};

use crate::theme::{self, icons};

/// The preset swatches, as sRGB bytes, in rows of [`COLUMNS`].
pub const PRESETS: [[u8; 3]; 28] = [
    [248, 248, 248],
    [205, 205, 205],
    [163, 162, 165],
    [99, 95, 98],
    [27, 42, 53],
    [17, 17, 17],
    [215, 197, 154],
    [196, 40, 28],
    [255, 0, 0],
    [218, 133, 65],
    [245, 205, 48],
    [255, 255, 0],
    [160, 95, 53],
    [124, 92, 70],
    [75, 151, 75],
    [0, 255, 0],
    [39, 70, 45],
    [161, 196, 140],
    [234, 184, 146],
    [105, 64, 40],
    [204, 142, 105],
    [13, 105, 172],
    [0, 0, 255],
    [110, 153, 202],
    [0, 255, 255],
    [4, 175, 236],
    [107, 50, 124],
    [255, 0, 191],
];
const COLUMNS: usize = 7;
const SWATCH: f32 = 20.0;
const GAP: f32 = 3.0;
const FIRST_PALETTE: &str = "My Colors";
/// How many colors one palette can hold, so a slip can't grow one forever.
const MAX_COLORS: usize = 70;

/// A named set of colors the user saved.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Palette {
    pub name: String,
    pub colors: Vec<[u8; 3]>,
}

/// Every palette the user has, and the one showing.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Library {
    pub palettes: Vec<Palette>,
    pub active: usize,
}

impl Default for Library {
    fn default() -> Self {
        Self {
            palettes: vec![Palette {
                name: FIRST_PALETTE.to_owned(),
                colors: Vec::new(),
            }],
            active: 0,
        }
    }
}

impl Library {
    /// The library as text, for the editor's settings.
    pub fn to_text(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }

    /// A library saved by [`to_text`](Self::to_text); damaged text gives
    /// the starting one. There's always a palette to add colors to.
    pub fn from_text(text: &str) -> Self {
        let mut library: Self = serde_json::from_str(text).unwrap_or_default();
        if library.palettes.is_empty() {
            library = Self::default();
        }
        library.active = library.active.min(library.palettes.len() - 1);
        library
    }

    fn current(&self) -> &Palette {
        &self.palettes[self.active.min(self.palettes.len() - 1)]
    }

    /// Saves `color` in the palette showing. False if it's already there
    /// or the palette is full.
    pub fn add_color(&mut self, color: [u8; 3]) -> bool {
        let at = self.active.min(self.palettes.len() - 1);
        let colors = &mut self.palettes[at].colors;
        if colors.contains(&color) || colors.len() >= MAX_COLORS {
            return false;
        }
        colors.push(color);
        true
    }

    /// Takes `color` out of the palette showing.
    pub fn remove_color(&mut self, color: [u8; 3]) {
        let at = self.active.min(self.palettes.len() - 1);
        self.palettes[at].colors.retain(|saved| *saved != color);
    }

    /// Adds an empty palette called `name` and shows it. False if the name
    /// is blank or taken.
    pub fn add_palette(&mut self, name: &str) -> bool {
        let name = name.trim();
        if name.is_empty() || self.palettes.iter().any(|palette| palette.name == name) {
            return false;
        }
        self.palettes.push(Palette {
            name: name.to_owned(),
            colors: Vec::new(),
        });
        self.active = self.palettes.len() - 1;
        true
    }

    /// Deletes palette `index`. The last one left stays.
    pub fn remove_palette(&mut self, index: usize) {
        if self.palettes.len() > 1 && index < self.palettes.len() {
            self.palettes.remove(index);
            self.active = self.active.min(self.palettes.len() - 1);
        }
    }
}

/// The library, shared between the app (which saves it) and the controls.
#[derive(Clone, Default)]
pub struct Handle(Arc<Mutex<Library>>);

impl Handle {
    pub fn new(library: Library) -> Self {
        Self(Arc::new(Mutex::new(library)))
    }

    /// A copy of the library as it is now.
    pub fn get(&self) -> Library {
        self.lock().clone()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Library> {
        // A panic while it was held can't leave a library half-changed in
        // a way that matters here.
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

fn memory_id() -> egui::Id {
    egui::Id::new("color palette library")
}

/// Makes `handle` the library the controls in `ctx` use.
pub fn install(ctx: &egui::Context, handle: &Handle) {
    ctx.data_mut(|data| data.insert_temp(memory_id(), handle.clone()));
}

/// The library the controls use.
fn handle(ctx: &egui::Context) -> Handle {
    ctx.data_mut(|data| data.get_temp_mut_or_default::<Handle>(memory_id()).clone())
}

/// One swatch, ringed when it's the current color or the mouse is on it.
fn swatch(ui: &mut egui::Ui, rgb: [u8; 3], current: [u8; 3]) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::Vec2::splat(SWATCH), egui::Sense::click());
    let palette = theme::palette(ui.ctx());
    let [r, g, b] = rgb;
    let stroke = if rgb == current || response.hovered() {
        egui::Stroke::new(2.0_f32, palette.text_strong)
    } else {
        egui::Stroke::new(1.0_f32, palette.border_strong)
    };
    ui.painter()
        .rect_filled(rect, 3.0, Color32::from_rgb(r, g, b));
    ui.painter()
        .rect_stroke(rect, 3.0, stroke, egui::StrokeKind::Inside);
    response
}

/// Wraps swatches into rows of [`COLUMNS`].
fn in_rows(ui: &mut egui::Ui, count: usize, mut add: impl FnMut(&mut egui::Ui, usize)) {
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing = egui::Vec2::splat(GAP);
        for row in 0..count.div_ceil(COLUMNS) {
            ui.horizontal(|ui| {
                for index in (row * COLUMNS)..((row + 1) * COLUMNS).min(count) {
                    add(ui, index);
                }
            });
        }
    });
}

/// The preset swatches and the user's palettes, changing `current` to the
/// one clicked. True if it changed.
pub fn swatches(ui: &mut egui::Ui, current: &mut [u8; 3]) -> bool {
    let mut picked = None;
    let shown = *current;
    in_rows(ui, PRESETS.len(), |ui, index| {
        if swatch(ui, PRESETS[index], shown).clicked() {
            picked = Some(PRESETS[index]);
        }
    });
    ui.add_space(6.0);
    if let Some(color) = saved_palettes(ui, shown) {
        picked = Some(color);
    }
    match picked {
        Some(color) if color != *current => {
            *current = color;
            true
        }
        _ => false,
    }
}

/// What the Add Palette box is holding, kept between frames.
#[derive(Clone, Default)]
struct NewPalette {
    open: bool,
    name: String,
}

/// The chips that pick a palette, its colors, and the buttons that add to
/// them. Gives a saved color that was clicked.
fn saved_palettes(ui: &mut egui::Ui, current: [u8; 3]) -> Option<[u8; 3]> {
    let handle = handle(ui.ctx());
    let library = handle.get();
    let mut picked = None;
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = egui::Vec2::splat(GAP);
        for (index, palette) in library.palettes.iter().enumerate() {
            let chip = ui
                .selectable_label(index == library.active, palette.name.as_str())
                .on_hover_text("Right-click to delete this palette");
            if chip.clicked() {
                handle.lock().active = index;
            }
            chip.context_menu(|ui| {
                let can_delete = library.palettes.len() > 1;
                if ui
                    .add_enabled(can_delete, egui::Button::new("Delete palette"))
                    .clicked()
                {
                    handle.lock().remove_palette(index);
                    ui.close();
                }
            });
        }
    });
    let form_id = ui.id().with("new palette");
    let mut form: NewPalette = ui.data_mut(|data| data.get_temp(form_id).unwrap_or_default());
    let colors = &library.current().colors;
    in_rows(ui, colors.len() + 1, |ui, index| {
        let Some(&color) = colors.get(index) else {
            let add = ui
                .add_sized(
                    egui::Vec2::splat(SWATCH),
                    egui::Button::new(icons::PLUS).frame(true),
                )
                .on_hover_text("Save the current color in this palette");
            if add.clicked() {
                handle.lock().add_color(current);
            }
            return;
        };
        let saved = swatch(ui, color, current).on_hover_text("Right-click to remove");
        if saved.clicked() {
            picked = Some(color);
        }
        saved.context_menu(|ui| {
            if ui.button("Remove from palette").clicked() {
                handle.lock().remove_color(color);
                ui.close();
            }
        });
    });
    ui.horizontal(|ui| {
        if form.open {
            let field = ui.add(
                egui::TextEdit::singleline(&mut form.name)
                    .hint_text("Palette name")
                    .desired_width(110.0),
            );
            let entered = field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            if (ui.button("Add").clicked() || entered) && handle.lock().add_palette(&form.name) {
                form = NewPalette::default();
            }
            if ui.button(icons::X).on_hover_text("Cancel").clicked() {
                form = NewPalette::default();
            }
        } else if ui
            .small_button(theme::with_icon(icons::PLUS, "New palette"))
            .clicked()
        {
            form.open = true;
        }
    });
    ui.data_mut(|data| data.insert_temp(form_id, form));
    picked
}

/// A color button whose popup has the swatches above a color picker.
fn button(ui: &mut egui::Ui, color: &mut Color32, alpha: Alpha) -> egui::Response {
    let size = ui.spacing().interact_size;
    let (rect, mut response) = ui.allocate_exact_size(size, egui::Sense::click());
    if ui.is_rect_visible(rect) {
        egui::color_picker::show_color_at(ui.painter(), *color, rect);
        // A thin outline, so white shows on a white panel and black on black.
        ui.painter().rect_stroke(
            rect,
            2,
            egui::Stroke::new(1.0_f32, theme::palette(ui.ctx()).border_strong),
            egui::StrokeKind::Inside,
        );
    }
    let mut changed = false;
    egui::Popup::from_toggle_button_response(&response)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .show(|ui| {
            ui.set_max_width(COLUMNS as f32 * (SWATCH + GAP) + 80.0);
            let [r, g, b, a] = color.to_srgba_unmultiplied();
            let mut rgb = [r, g, b];
            if swatches(ui, &mut rgb) {
                *color = Color32::from_rgba_unmultiplied(rgb[0], rgb[1], rgb[2], a);
                changed = true;
            }
            ui.separator();
            changed |= egui::color_picker::color_picker_color32(ui, color, alpha);
        });
    if changed {
        response.mark_changed();
    }
    response
}

/// An opaque color, as sRGB bytes.
pub fn edit_srgb(ui: &mut egui::Ui, rgb: &mut [u8; 3]) -> egui::Response {
    let mut color = Color32::from_rgb(rgb[0], rgb[1], rgb[2]);
    let response = button(ui, &mut color, Alpha::Opaque);
    if response.changed() {
        let [r, g, b, _] = color.to_srgba_unmultiplied();
        *rgb = [r, g, b];
    }
    response
}

/// A color with transparency.
pub fn edit_srgba(ui: &mut egui::Ui, color: &mut Color32) -> egui::Response {
    button(ui, color, Alpha::OnlyBlend)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saved_colors_and_palettes_survive_saving() {
        let mut library = Library::default();
        assert!(library.add_color([10, 20, 30]));
        assert!(!library.add_color([10, 20, 30]), "already saved");
        assert!(library.add_palette("Forest"));
        assert!(library.add_color([1, 80, 2]));
        let loaded = Library::from_text(&library.to_text());
        assert_eq!(loaded, library);
        assert_eq!(loaded.current().name, "Forest");
        assert_eq!(loaded.palettes[0].colors, [[10, 20, 30]]);
    }

    #[test]
    fn damaged_text_still_gives_a_palette_to_save_into() {
        for text in ["not json", r#"{"palettes":[],"active":4}"#] {
            let mut library = Library::from_text(text);
            assert!(library.add_color([1, 2, 3]));
        }
    }

    #[test]
    fn a_blank_or_taken_name_is_refused_and_the_last_palette_stays() {
        let mut library = Library::default();
        assert!(!library.add_palette("  "));
        assert!(!library.add_palette(FIRST_PALETTE));
        library.remove_palette(0);
        assert_eq!(library.palettes.len(), 1);
        library.add_palette("B");
        library.remove_palette(1);
        assert_eq!(library.active, 0);
    }

    #[test]
    fn a_palette_holds_a_limited_number_of_colors() {
        let mut library = Library::default();
        for i in 0..MAX_COLORS {
            assert!(library.add_color([i as u8, 0, 0]));
        }
        assert!(!library.add_color([255, 255, 255]));
        library.remove_color([0, 0, 0]);
        assert_eq!(library.current().colors.len(), MAX_COLORS - 1);
    }

    /// Clicks at `at` over a few frames, drawing `add` each time.
    fn click(ctx: &egui::Context, at: egui::Pos2, mut add: impl FnMut(&mut egui::Ui)) {
        let press = |pressed| egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        };
        for events in [
            vec![egui::Event::PointerMoved(at)],
            vec![press(true)],
            vec![press(false)],
            Vec::new(),
        ] {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(800.0, 600.0),
                )),
                events,
                ..Default::default()
            };
            let _ = ctx.run(input, |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| add(ui));
            });
        }
    }

    #[test]
    fn clicking_a_preset_swatch_picks_its_color() {
        let ctx = theme::test_context();
        let mut color = [1, 2, 3];
        let mut changed = false;
        let mut top_left = egui::Pos2::ZERO;
        click(&ctx, egui::pos2(-10.0, -10.0), |ui| {
            top_left = ui.cursor().min;
        });
        // The second swatch of the first row.
        let at = top_left + egui::vec2(SWATCH + GAP + SWATCH / 2.0, SWATCH / 2.0);
        click(&ctx, at, |ui| changed |= swatches(ui, &mut color));
        assert!(changed);
        assert_eq!(color, PRESETS[1]);
    }

    #[test]
    fn a_color_buttons_popup_offers_the_presets_and_picking_one_changes_it() {
        // Where the popup's swatches land depends on the fonts and margins,
        // so try spots below the button until one is a swatch.
        for y in (20..90).step_by(4) {
            for x in (8..60).step_by(4) {
                let ctx = theme::test_context();
                let mut color = [1, 2, 3];
                let mut changed = false;
                let mut button_at = egui::Pos2::ZERO;
                click(&ctx, egui::pos2(-10.0, -10.0), |ui| {
                    button_at = ui.cursor().min + egui::vec2(4.0, 4.0);
                    changed |= edit_srgb(ui, &mut color).changed();
                });
                click(&ctx, button_at, |ui| {
                    changed |= edit_srgb(ui, &mut color).changed();
                });
                assert!(egui::Popup::is_any_open(&ctx), "the click opens it");
                assert!(!changed, "opening it changes nothing");
                click(&ctx, egui::pos2(x as f32, y as f32), |ui| {
                    changed |= edit_srgb(ui, &mut color).changed();
                });
                if changed && PRESETS.contains(&color) {
                    assert!(egui::Popup::is_any_open(&ctx), "and it stays open");
                    return;
                }
            }
        }
        panic!("no preset swatch could be clicked");
    }

    #[test]
    fn a_color_button_is_marked_changed_only_once_a_color_is_picked() {
        let ctx = theme::test_context();
        let mut color = [9, 9, 9];
        let mut changed = false;
        click(&ctx, egui::pos2(-10.0, -10.0), |ui| {
            changed |= edit_srgb(ui, &mut color).changed();
        });
        assert!(!changed);
    }
}
