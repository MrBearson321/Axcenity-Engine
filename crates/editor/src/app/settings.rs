//! The Settings window (File > Settings…, the start screen's sidebar, or
//! Ctrl+, / Cmd+,): appearance, typography and density, the editor layout,
//! the keyboard shortcuts, and accessibility. Changes apply at once and are
//! saved between runs with the other settings.

use super::*;

use crate::theme::{Accent, Appearance, Density, Palette, ThemeChoice, type_scale};
use crate::widgets;

/// Ctrl+, (Cmd+, on macOS) opens Settings, as in most desktop apps.
pub(super) const SETTINGS_SHORTCUT: egui::KeyboardShortcut =
    egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::Comma);

/// The pages of the Settings window.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum SettingsPage {
    #[default]
    Appearance,
    Typography,
    Layout,
    Shortcuts,
    Accessibility,
}

impl SettingsPage {
    const ALL: [SettingsPage; 5] = [
        SettingsPage::Appearance,
        SettingsPage::Typography,
        SettingsPage::Layout,
        SettingsPage::Shortcuts,
        SettingsPage::Accessibility,
    ];

    fn label(self) -> &'static str {
        match self {
            SettingsPage::Appearance => "Appearance",
            SettingsPage::Typography => "Text & density",
            SettingsPage::Layout => "Editor layout",
            SettingsPage::Shortcuts => "Shortcuts",
            SettingsPage::Accessibility => "Accessibility",
        }
    }

    fn icon(self) -> &'static str {
        match self {
            SettingsPage::Appearance => icons::PALETTE,
            SettingsPage::Typography => icons::TEXT_AA,
            SettingsPage::Layout => icons::LAYOUT,
            SettingsPage::Shortcuts => icons::KEYBOARD,
            SettingsPage::Accessibility => icons::EYE,
        }
    }
}

/// Whether Settings is open, and on which page.
#[derive(Default)]
pub(super) struct SettingsWindow {
    pub open: bool,
    pub page: SettingsPage,
}

const WINDOW_SIZE: egui::Vec2 = egui::vec2(700.0, 470.0);
const NAV_WIDTH: f32 = 170.0;

impl EditorApp {
    /// Changes the look to `appearance`, fading from the old background if
    /// the theme changed, and remembers it.
    pub(super) fn set_appearance(&mut self, ctx: &egui::Context, appearance: Appearance) {
        if appearance == self.appearance {
            return;
        }
        let old_background = theme::palette(ctx).panel;
        let theme_changed = appearance.theme != self.appearance.theme;
        self.appearance = appearance;
        theme::apply(ctx, &self.appearance);
        if theme_changed {
            theme::start_crossfade(ctx, old_background);
        }
    }

    /// The Settings window, while it's open.
    pub(super) fn settings_window(&mut self, ctx: &egui::Context) {
        if !self.settings.open {
            return;
        }
        let mut open = true;
        let mut appearance = self.appearance.clone();
        let screen = ctx.screen_rect();
        egui::Window::new(theme::with_icon(icons::GEAR_SIX, "Settings"))
            .id(egui::Id::new("settings window"))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .fixed_size(WINDOW_SIZE)
            .default_pos(screen.center() - WINDOW_SIZE / 2.0)
            .constrain(true)
            .show(ctx, |ui| {
                ui.horizontal_top(|ui| {
                    ui.vertical(|ui| {
                        ui.set_width(NAV_WIDTH);
                        ui.spacing_mut().item_spacing.y = 2.0;
                        for page in SettingsPage::ALL {
                            let selected = self.settings.page == page;
                            if widgets::nav_item(ui, page.icon(), page.label(), None, selected)
                                .clicked()
                            {
                                self.settings.page = page;
                            }
                        }
                    });
                    ui.separator();
                    egui::ScrollArea::vertical()
                        .id_salt("settings page")
                        .max_height(WINDOW_SIZE.y)
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            ui.set_min_height(WINDOW_SIZE.y);
                            ui.set_width(ui.available_width());
                            ui.add_space(2.0);
                            ui.vertical(|ui| match self.settings.page {
                                SettingsPage::Appearance => appearance_page(ui, &mut appearance),
                                SettingsPage::Typography => typography_page(ui, &mut appearance),
                                SettingsPage::Layout => self.layout_page(ui),
                                SettingsPage::Shortcuts => shortcuts_page(ui),
                                SettingsPage::Accessibility => {
                                    accessibility_page(ui, &mut appearance);
                                }
                            });
                        });
                });
            });
        self.set_appearance(ctx, appearance);
        if !open || ctx.input(|input| input.key_pressed(egui::Key::Escape)) {
            self.settings.open = false;
        }
    }

    fn layout_page(&mut self, ui: &mut egui::Ui) {
        page_title(ui, "Editor layout", "Which panels show, and where they go.");
        widgets::section_label(ui, "Panels");
        for pane in Pane::CLOSABLE {
            let mut open = self.layout.is_open(pane);
            if ui
                .checkbox(&mut open, theme::with_icon(pane.icon(), pane.label()))
                .changed()
            {
                self.layout.toggle(pane);
            }
        }
        ui.add_space(6.0);
        let palette = theme::palette(ui.ctx());
        if ui
            .add(widgets::button(
                widgets::Role::Secondary,
                Some(icons::ARROW_COUNTER_CLOCKWISE),
                "Reset Layout",
                &palette,
            ))
            .on_hover_text("Put every panel back where it starts.")
            .clicked()
        {
            self.layout.reset();
        }
        ui.add_space(16.0);
        widgets::section_label(ui, "Viewport");
        ui.checkbox(
            &mut self.light_cones,
            theme::with_icon(icons::FLASHLIGHT, "Show every light's cone"),
        )
        .on_hover_text("Off, only the selected light (or the part it's in) shows where it shines.");
        ui.add_space(16.0);
        widgets::section_label(ui, "Ribbon");
        ui.label(
            egui::RichText::new(
                "Make your own ribbon tab from any of the ribbon's groups. Right-click a tab \
                 you made to change or delete it.",
            )
            .color(palette.text_secondary),
        );
        if ui
            .add(widgets::button(
                widgets::Role::Secondary,
                Some(icons::PLUS),
                "New Ribbon Tab…",
                &palette,
            ))
            .clicked()
        {
            self.ribbon_tabs.edit(None);
        }
    }
}

/// A page's title and a line saying what's on it.
fn page_title(ui: &mut egui::Ui, title: &str, subtitle: &str) {
    let palette = theme::palette(ui.ctx());
    ui.label(
        egui::RichText::new(title)
            .family(theme::heading_family())
            .size(type_scale::HEADING + 2.0)
            .color(palette.text_strong),
    );
    ui.label(egui::RichText::new(subtitle).color(palette.text_secondary));
    ui.add_space(14.0);
}

fn appearance_page(ui: &mut egui::Ui, appearance: &mut Appearance) {
    page_title(
        ui,
        "Appearance",
        "The theme, the accent colour and how big everything is.",
    );
    widgets::section_label(ui, "Theme");
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 12.0;
        for choice in ThemeChoice::ALL {
            if theme_preview(ui, choice, appearance.accent, appearance.theme == choice).clicked() {
                appearance.theme = choice;
            }
        }
    });
    ui.add_space(16.0);
    widgets::section_label(ui, "Accent");
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 10.0;
        for accent in Accent::ALL {
            if accent_swatch(ui, accent, appearance.accent == accent).clicked() {
                appearance.accent = accent;
            }
        }
        ui.add_space(4.0);
        let palette = theme::palette(ui.ctx());
        ui.label(egui::RichText::new(appearance.accent.label()).color(palette.text_secondary));
    });
    ui.add_space(16.0);
    widgets::section_label(ui, "Interface scale");
    let options: Vec<(f32, Option<&str>, String)> = Appearance::UI_SCALES
        .iter()
        .map(|&scale| (scale, None, format!("{}%", (scale * 100.0).round())))
        .collect();
    let options: Vec<(f32, Option<&str>, &str)> = options
        .iter()
        .map(|(scale, icon, label)| (*scale, *icon, label.as_str()))
        .collect();
    widgets::segmented(ui, &mut appearance.ui_scale, &options);
}

fn typography_page(ui: &mut egui::Ui, appearance: &mut Appearance) {
    page_title(
        ui,
        "Text & density",
        "How much room controls get, and the size of code.",
    );
    widgets::section_label(ui, "Density");
    widgets::segmented(
        ui,
        &mut appearance.density,
        &Density::ALL.map(|density| (density, None, density.label())),
    );
    let palette = theme::palette(ui.ctx());
    ui.label(
        egui::RichText::new(match appearance.density {
            Density::Comfortable => "Roomier rows and controls, easier to hit.",
            Density::Compact => "Tighter rows and controls, to see more at once.",
        })
        .color(palette.text_secondary),
    );
    ui.add_space(16.0);
    widgets::section_label(ui, "Code and Output");
    ui.add(
        egui::Slider::new(&mut appearance.code_font_size, Appearance::CODE_FONT_SIZES)
            .step_by(0.5)
            .suffix(" pt"),
    );
    egui::Frame::new()
        .fill(palette.inset)
        .stroke(egui::Stroke::new(1.0_f32, palette.border))
        .corner_radius(6)
        .inner_margin(egui::Margin::symmetric(12, 10))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            let font = egui::FontId::monospace(appearance.code_font_size);
            ui.label(
                egui::RichText::new("local part = workspace.Pad")
                    .font(font.clone())
                    .color(palette.text),
            );
            ui.label(
                egui::RichText::new("part.Touched:Connect(onTouched) -- 0O 1lI")
                    .font(font)
                    .color(palette.text_secondary),
            );
        });
    ui.add_space(16.0);
    widgets::section_label(ui, "Interface font");
    ui.label(
        egui::RichText::new("Inter for the interface and JetBrains Mono for code, both built in.")
            .color(palette.text_secondary),
    );
}

/// The keyboard shortcuts, as a table. Shortcuts are fixed for now, so this
/// is a reference.
fn shortcuts_page(ui: &mut egui::Ui) {
    page_title(ui, "Shortcuts", "The keys the editor knows.");
    let ctx = ui.ctx().clone();
    let format = |shortcut: &egui::KeyboardShortcut| ctx.format_shortcut(shortcut);
    let mut groups: Vec<(&str, Vec<(String, String)>)> = vec![
        (
            "General",
            vec![
                ("Save".into(), format(&SAVE_SHORTCUT)),
                ("Settings".into(), format(&SETTINGS_SHORTCUT)),
                ("Play".into(), format(&PLAY_SHORTCUT)),
                ("Stop".into(), format(&STOP_SHORTCUT)),
            ],
        ),
        (
            "Editing",
            vec![
                ("Undo".into(), format(&UNDO_SHORTCUT)),
                (
                    "Redo".into(),
                    format!(
                        "{}  or  {}",
                        format(&REDO_SHORTCUT),
                        format(&REDO_SHIFT_SHORTCUT)
                    ),
                ),
                ("Duplicate".into(), format(&DUPLICATE_SHORTCUT)),
                ("Rename".into(), RENAME_KEY.name().to_owned()),
                ("Delete".into(), DELETE_KEY_TEXT.to_owned()),
            ],
        ),
    ];
    let tools = Tool::ALL
        .into_iter()
        .filter_map(|tool| {
            let keys: Vec<String> = tool
                .shortcut()
                .map(|shortcut| format(&shortcut))
                .into_iter()
                .chain(tool.key().map(|key| key.name().to_owned()))
                .collect();
            (!keys.is_empty()).then(|| (format!("{} tool", tool.label()), keys.join("  or  ")))
        })
        .collect();
    groups.push(("Tools", tools));
    groups.push((
        "Viewport camera",
        vec![
            ("Look around".into(), "Right-drag".into()),
            ("Fly".into(), "W A S D, E, Q".into()),
            ("Fly slower".into(), "Shift".into()),
            ("Pan".into(), "Middle-drag".into()),
            ("Orbit".into(), "Alt + left-drag".into()),
            ("Fly speed".into(), "Shift + wheel".into()),
        ],
    ));
    let palette = theme::palette(ui.ctx());
    for (title, rows) in groups {
        widgets::section_label(ui, title);
        egui::Grid::new(title)
            .num_columns(2)
            .min_col_width(200.0)
            .spacing([12.0, 6.0])
            .show(ui, |ui| {
                for (action, keys) in rows {
                    ui.label(egui::RichText::new(action).color(palette.text_secondary));
                    keycap(ui, &keys);
                    ui.end_row();
                }
            });
        ui.add_space(12.0);
    }
}

/// Keys drawn like a key on a keyboard.
fn keycap(ui: &mut egui::Ui, keys: &str) {
    let palette = theme::palette(ui.ctx());
    egui::Frame::new()
        .fill(palette.control)
        .stroke(egui::Stroke::new(1.0_f32, palette.border))
        .corner_radius(4)
        .inner_margin(egui::Margin::symmetric(6, 1))
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new(keys)
                    .monospace()
                    .size(type_scale::SMALL)
                    .color(palette.text),
            );
        });
}

fn accessibility_page(ui: &mut egui::Ui, appearance: &mut Appearance) {
    page_title(
        ui,
        "Accessibility",
        "Motion, and other ways to make the editor easier to use.",
    );
    let palette = theme::palette(ui.ctx());
    widgets::section_label(ui, "Motion");
    ui.checkbox(&mut appearance.reduce_motion, "Reduce motion");
    ui.label(
        egui::RichText::new(
            "Turns off hover fades, popup and window fades, smooth scrolling and the theme \
             crossfade. The viewport camera is never animated by the interface.",
        )
        .color(palette.text_secondary),
    );
    ui.add_space(16.0);
    widgets::section_label(ui, "Readability");
    ui.label(
        egui::RichText::new(
            "Text and accent colours meet WCAG AA contrast in both themes. For larger text, \
             raise the interface scale in Appearance, or press Ctrl and + (Cmd and + on macOS).",
        )
        .color(palette.text_secondary),
    );
    ui.add_space(16.0);
    widgets::section_label(ui, "Keyboard");
    ui.label(
        egui::RichText::new(
            "Tab moves between controls and shows a focus ring in the accent colour; Space or \
             Enter presses the focused control.",
        )
        .color(palette.text_secondary),
    );
}

/// A small picture of the editor in `choice`'s colours, to pick a theme by.
/// System shows half dark, half light.
fn theme_preview(
    ui: &mut egui::Ui,
    choice: ThemeChoice,
    accent: Accent,
    selected: bool,
) -> egui::Response {
    let size = egui::vec2(148.0, 112.0);
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
    response.widget_info(|| {
        egui::WidgetInfo::selected(
            egui::WidgetType::RadioButton,
            true,
            selected,
            choice.label(),
        )
    });
    let current = theme::palette(ui.ctx());
    let picture = egui::Rect::from_min_size(rect.min, egui::vec2(size.x, 84.0));
    match choice {
        ThemeChoice::Dark => mini_editor(ui, picture, &Palette::new(true, accent)),
        ThemeChoice::Light => mini_editor(ui, picture, &Palette::new(false, accent)),
        ThemeChoice::System => {
            let half = picture.center().x;
            let mut left = ui.new_child(egui::UiBuilder::new().max_rect(picture));
            left.set_clip_rect(picture.with_max_x(half));
            mini_editor(&left, picture, &Palette::new(true, accent));
            let mut right = ui.new_child(egui::UiBuilder::new().max_rect(picture));
            right.set_clip_rect(picture.with_min_x(half));
            mini_editor(&right, picture, &Palette::new(false, accent));
        }
    }
    let t = theme::ease(
        ui.ctx(),
        response.id.with("ring"),
        selected || response.hovered(),
    );
    let ring = if selected {
        current.accent
    } else {
        theme::mix(current.border, current.border_strong, t)
    };
    ui.painter().rect_stroke(
        picture,
        8,
        egui::Stroke::new(if selected { 2.0_f32 } else { 1.0 }, ring),
        egui::StrokeKind::Outside,
    );
    let label_color = if selected {
        current.text_strong
    } else {
        current.text_secondary
    };
    ui.painter().text(
        egui::pos2(rect.left() + 2.0, picture.bottom() + 16.0),
        egui::Align2::LEFT_CENTER,
        theme::with_icon(choice.icon(), choice.label()),
        egui::FontId::new(type_scale::BODY, theme::medium_family()),
        label_color,
    );
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Paints a tiny editor: header, Explorer with a selected row, a viewport
/// and an accent button.
fn mini_editor(ui: &egui::Ui, rect: egui::Rect, p: &Palette) {
    let painter = ui.painter();
    painter.rect_filled(rect, 8, p.app_bg);
    let header = egui::Rect::from_min_size(rect.min, egui::vec2(rect.width(), 12.0));
    painter.circle_filled(
        header.left_center() + egui::vec2(8.0, 0.0),
        2.5,
        p.text_muted,
    );
    for (x, w) in [(16.0, 14.0), (34.0, 12.0), (50.0, 12.0)] {
        painter.rect_filled(
            egui::Rect::from_min_size(header.min + egui::vec2(x, 5.0), egui::vec2(w, 2.5)),
            1,
            p.text_weak,
        );
    }
    let body = egui::Rect::from_min_max(
        header.left_bottom() + egui::vec2(4.0, 0.0),
        rect.max - egui::vec2(4.0, 4.0),
    );
    let explorer = egui::Rect::from_min_size(body.min, egui::vec2(42.0, body.height()));
    painter.rect_filled(explorer, 4, p.panel);
    for row in 0..5 {
        let y = explorer.top() + 7.0 + row as f32 * 10.0;
        let line = egui::Rect::from_min_size(
            egui::pos2(explorer.left() + 5.0 + (row % 2) as f32 * 5.0, y),
            egui::vec2(24.0, 3.0),
        );
        if row == 2 {
            painter.rect_filled(
                egui::Rect::from_min_size(
                    egui::pos2(explorer.left() + 2.0, y - 3.0),
                    egui::vec2(38.0, 9.0),
                ),
                2,
                p.accent_soft,
            );
            painter.rect_filled(line, 1, p.accent_text);
        } else {
            painter.rect_filled(line, 1, p.text_muted);
        }
    }
    let viewport =
        egui::Rect::from_min_max(egui::pos2(explorer.right() + 4.0, body.top()), body.max);
    painter.rect_filled(viewport, 4, theme::mix(p.app_bg, p.panel, 0.5));
    let ground = egui::Rect::from_min_max(
        egui::pos2(viewport.left() + 14.0, viewport.center().y + 6.0),
        egui::pos2(viewport.right() - 14.0, viewport.center().y + 14.0),
    );
    painter.rect_filled(ground, 2, p.border_strong);
    painter.rect_filled(
        egui::Rect::from_center_size(
            viewport.center() - egui::vec2(0.0, 2.0),
            egui::vec2(14.0, 14.0),
        ),
        2,
        p.text_weak,
    );
    painter.rect_filled(
        egui::Rect::from_min_size(
            viewport.right_top() + egui::vec2(-26.0, 5.0),
            egui::vec2(20.0, 8.0),
        ),
        2,
        p.accent_solid,
    );
}

/// A round swatch of `accent`, ringed while it's picked.
fn accent_swatch(ui: &mut egui::Ui, accent: Accent, selected: bool) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(26.0, 26.0), egui::Sense::click());
    response.widget_info(|| {
        egui::WidgetInfo::selected(
            egui::WidgetType::RadioButton,
            true,
            selected,
            accent.label(),
        )
    });
    let current = theme::palette(ui.ctx());
    let color = Palette::new(current.dark, accent).accent;
    let center = rect.center();
    let grow = theme::ease(ui.ctx(), response.id.with("hover"), response.hovered());
    ui.painter().circle_filled(center, 9.0 + grow, color);
    if selected {
        ui.painter().circle_stroke(
            center,
            12.5,
            egui::Stroke::new(1.5_f32, current.text_strong),
        );
    }
    if response.has_focus() {
        ui.painter()
            .circle_stroke(center, 13.0, egui::Stroke::new(1.0_f32, current.accent));
    }
    response
        .on_hover_text(accent.label())
        .on_hover_cursor(egui::CursorIcon::PointingHand)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_settings_page_draws_and_changes_apply_to_the_look() {
        let ctx = theme::test_context();
        let mut app = EditorApp::new(None);
        app.settings.open = true;
        let snapshots = std::env::var_os("AXCENITY_SNAPSHOTS").map(PathBuf::from);
        for page in SettingsPage::ALL {
            app.settings.page = page;
            let mut draw = |ctx: &egui::Context| {
                egui::CentralPanel::default().show(ctx, |_| {});
                app.settings_window(ctx);
            };
            match &snapshots {
                Some(folder) if page == SettingsPage::Appearance => crate::snapshot::snapshot(
                    &folder.join("settings-appearance.png"),
                    egui::vec2(900.0, 640.0),
                    vec![Vec::new(); 3],
                    &mut draw,
                ),
                _ => {
                    let _ = ctx.run(Default::default(), &mut draw);
                }
            }
        }
        assert!(app.settings.open);

        let light = Appearance {
            theme: ThemeChoice::Light,
            accent: Accent::Orange,
            ..Appearance::default()
        };
        let _ = ctx.run(Default::default(), |ctx| {
            app.set_appearance(ctx, light.clone())
        });
        assert_eq!(app.appearance, light);
        assert!(!ctx.style().visuals.dark_mode);
        assert_eq!(
            theme::palette(&ctx).accent,
            Palette::new(false, Accent::Orange).accent
        );
    }
}
