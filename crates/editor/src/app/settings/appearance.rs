//! The Appearance, Typography and Accessibility pages, and the small theme
//! preview they share.

use super::*;

pub(super) fn appearance_page(ui: &mut egui::Ui, appearance: &mut Appearance) {
    page_title(
        ui,
        "Appearance",
        "The theme, the accent colour and how big everything is.",
    );
    section(ui, "Theme");
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 12.0;
        for choice in ThemeChoice::ALL {
            if theme_preview(ui, choice, appearance.accent, appearance.theme == choice).clicked() {
                appearance.theme = choice;
            }
        }
    });
    ui.add_space(space::L);
    section(ui, "Accent");
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 10.0;
        for accent in Accent::ALL {
            if accent_swatch(ui, accent, appearance.accent == accent).clicked() {
                appearance.accent = accent;
            }
        }
        ui.add_space(space::XS);
        let palette = theme::palette(ui.ctx());
        ui.label(egui::RichText::new(appearance.accent.label()).color(palette.text_secondary));
    });
    ui.add_space(space::L);
    section(ui, "Interface scale");
    let options: Vec<(f32, Option<&str>, String)> = Appearance::UI_SCALES
        .iter()
        .map(|&scale| (scale, None, format!("{}%", (scale * 100.0).round())))
        .collect();
    let options: Vec<(f32, Option<&str>, &str)> = options
        .iter()
        .map(|(scale, icon, label)| (*scale, *icon, label.as_str()))
        .collect();
    widgets::segmented(ui, &mut appearance.ui_scale, &options);
    ui.add_space(space::L);
    section(ui, "Ribbon");
    widgets::segmented(
        ui,
        &mut appearance.ribbon,
        &RibbonAlign::ALL.map(|place| (place, None, place.label())),
    );
    let palette = theme::palette(ui.ctx());
    ui.label(
        egui::RichText::new(
            "Where the ribbon's buttons sit across the window: from the left edge, or in the \
             middle under the tabs.",
        )
        .color(palette.text_secondary),
    );
    ui.add_space(space::L);
    section(ui, "Menu icons");
    widgets::segmented(
        ui,
        &mut appearance.menu_icons,
        &MenuIcons::ALL.map(|place| (place, None, place.label())),
    );
    let palette = theme::palette(ui.ctx());
    ui.label(
        egui::RichText::new(
            "Where icons sit in their column in menus. Labels line up either way, with an icon \
             or without.",
        )
        .color(palette.text_secondary),
    );
}

pub(super) fn typography_page(ui: &mut egui::Ui, appearance: &mut Appearance) {
    page_title(
        ui,
        "Text & density",
        "How much room controls get, and the size of code.",
    );
    section(ui, "Density");
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
    ui.add_space(space::L);
    section(ui, "Code and Output");
    ui.add(
        egui::Slider::new(&mut appearance.code_font_size, Appearance::CODE_FONT_SIZES)
            .step_by(0.5)
            .suffix(" pt"),
    );
    egui::Frame::new()
        .fill(palette.inset)
        .stroke(egui::Stroke::new(1.0_f32, palette.border))
        .corner_radius(theme::radius::CARD)
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
    ui.add_space(space::L);
    section(ui, "Interface font");
    ui.label(
        egui::RichText::new("Inter for the interface and JetBrains Mono for code, both built in.")
            .color(palette.text_secondary),
    );
}

pub(super) fn accessibility_page(ui: &mut egui::Ui, appearance: &mut Appearance) {
    page_title(
        ui,
        "Accessibility",
        "Motion, contrast, and other ways to make the editor easier to use.",
    );
    let palette = theme::palette(ui.ctx());
    section(ui, "Motion");
    let row = ui.checkbox(&mut appearance.reduce_motion, "Reduce motion");
    found(ui, &row, "Reduce motion");
    ui.label(
        egui::RichText::new(
            "Turns off hover fades, popup and window fades, smooth scrolling and the theme \
             crossfade. The viewport camera is never animated by the interface.",
        )
        .color(palette.text_secondary),
    );
    ui.add_space(space::L);
    section(ui, "Contrast");
    let row = ui.checkbox(&mut appearance.increase_contrast, "Increase contrast");
    found(ui, &row, "Increase contrast");
    ui.label(
        egui::RichText::new(
            "Stronger outlines around buttons and fields, stronger borders and faint text, a \
             thicker focus ring, and block colours dark enough for their white labels.",
        )
        .color(palette.text_secondary),
    );
    ui.add_space(space::L);
    section(ui, "Display");
    ui.horizontal(|ui| {
        found_label(ui, "Steady frame rate");
        egui::ComboBox::from_id_salt("steady frame rate")
            .selected_text(Appearance::steady_rate_name(appearance.steady_hz))
            .show_ui(ui, |ui| {
                for hz in Appearance::STEADY_RATES {
                    ui.selectable_value(
                        &mut appearance.steady_hz,
                        hz,
                        Appearance::steady_rate_name(hz),
                    );
                }
            });
    });
    ui.label(
        egui::RichText::new(
            "While you work, frames are shown at this fixed rate rather than as fast as each \
             one is ready, so a FreeSync or G-Sync display holds one refresh rate. OLED and \
             VA monitors flicker when the rate swings; if yours still does, pick a lower rate. \
             The display's own rate has no limit.",
        )
        .color(palette.text_secondary),
    );
    ui.add_space(space::S);
    let row = ui.checkbox(&mut appearance.hold_refresh, "Hold the refresh rate steady");
    found(ui, &row, "Hold the refresh rate steady");
    ui.label(
        egui::RichText::new(
            "Keeps drawing at the steady frame rate while the editor has focus instead of \
             stopping when it's idle, for displays that flicker as the rate drops. It uses \
             more of the graphics card.",
        )
        .color(palette.text_secondary),
    );
    ui.add_space(space::L);
    section(ui, "Readability");
    ui.label(
        egui::RichText::new(
            "Text and accent colours meet WCAG AA contrast in both themes. For larger text, \
             raise the interface scale in Appearance, or press Ctrl and + (Cmd and + on macOS).",
        )
        .color(palette.text_secondary),
    );
    ui.add_space(space::L);
    section(ui, "Keyboard");
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
pub(super) fn theme_preview(
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
pub(super) fn mini_editor(ui: &egui::Ui, rect: egui::Rect, p: &Palette) {
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
pub(super) fn accent_swatch(ui: &mut egui::Ui, accent: Accent, selected: bool) -> egui::Response {
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
