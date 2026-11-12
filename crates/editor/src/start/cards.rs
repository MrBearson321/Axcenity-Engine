use super::*;

/// What the page header's buttons asked for.
pub(super) enum HeaderAction {
    New,
    Open,
    Join,
}

/// The page's title and a line about the engine, with New Project, Open
/// Project and Join Session at the right.
pub(super) fn page_header(
    ui: &mut egui::Ui,
    palette: &Palette,
    first_time: bool,
) -> Option<HeaderAction> {
    let mut action = None;
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.label(
                RichText::new(if first_time {
                    "Welcome to Axcenity"
                } else {
                    "Projects"
                })
                .family(theme::display_family())
                .size(type_scale::DISPLAY)
                .extra_letter_spacing(theme::DISPLAY_TRACKING)
                .color(palette.text_strong),
            );
            ui.label(
                RichText::new(
                    "Build worlds from parts, script them in Lua or blocks, and press Play.",
                )
                .color(palette.text_secondary),
            );
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            let new = widgets::button(Role::Primary, Some(icons::PLUS), "New Project", palette)
                .min_size(vec2(0.0, 32.0));
            if ui
                .add(new)
                .on_hover_text("Start a project from a template")
                .clicked()
            {
                action = Some(HeaderAction::New);
            }
            let open = widgets::button(
                Role::Secondary,
                Some(icons::FOLDER_OPEN),
                "Open Project…",
                palette,
            )
            .min_size(vec2(0.0, 32.0));
            if ui
                .add(open)
                .on_hover_text("Open a project folder")
                .clicked()
            {
                action = Some(HeaderAction::Open);
            }
            let join = widgets::button(Role::Secondary, Some(icons::SIGN_IN), "Join…", palette)
                .min_size(vec2(0.0, 32.0));
            if ui
                .add(join)
                .on_hover_text("Join someone's Collaborate session with their invite link")
                .clicked()
            {
                action = Some(HeaderAction::Join);
            }
        });
    });
    action
}

/// A card's surface with a spotlight under the pointer, for cards that
/// don't lean ([`tilt`]).
pub(super) fn card_frame(
    ui: &egui::Ui,
    rect: Rect,
    response: &egui::Response,
    hover: f32,
    palette: &Palette,
) {
    card_surface(ui, rect, response, hover, palette);
    widgets::spotlight(ui, rect, response.id, hover);
}

/// A card's surface: a thin outline that strengthens on hover (`hover`
/// from 0 to 1) toward the accent, a faint fill, and the focus ring.
pub(super) fn card_surface(
    ui: &egui::Ui,
    rect: Rect,
    response: &egui::Response,
    hover: f32,
    palette: &Palette,
) {
    ui.painter().rect(
        rect,
        CARD_RADIUS,
        theme::mix(palette.panel, palette.surface, 0.6 + 0.4 * hover),
        Stroke::new(
            1.0_f32,
            theme::mix(
                palette.border,
                theme::mix(palette.border_strong, palette.accent, 0.45),
                hover,
            ),
        ),
        egui::StrokeKind::Inside,
    );
    if response.has_focus() {
        ui.painter().rect_stroke(
            rect.expand(2.0),
            CARD_RADIUS + 2,
            Stroke::new(1.5_f32, palette.accent),
            egui::StrokeKind::Inside,
        );
    }
}

/// A soft accent glow behind an icon at `center`, `radius` wide, by the
/// hover's fade.
pub(super) fn icon_glow(
    ui: &egui::Ui,
    center: egui::Pos2,
    radius: f32,
    hover: f32,
    palette: &Palette,
) {
    if hover <= 0.0 {
        return;
    }
    let alpha = if palette.dark { 0.30 } else { 0.20 };
    widgets::soft_pool(
        ui.painter(),
        center,
        radius,
        palette.accent.gamma_multiply(alpha * hover),
    );
}

/// A project's picture: its thumbnail, or a quiet placeholder with its
/// initials on a faint grid. `float` is the hover's parallax: the picture
/// pans inside its frame, zoomed in by as much so no edge shows, and the
/// initials drift.
pub(super) fn thumbnail_image(
    ui: &egui::Ui,
    rect: Rect,
    radius: CornerRadius,
    name: &str,
    thumbnail: Option<&egui::TextureHandle>,
    float: egui::Vec2,
) {
    let palette = theme::palette(ui.ctx());
    match thumbnail {
        Some(texture) => {
            let room = (float.abs() / rect.size()).max_elem().min(0.2);
            let half = 0.5 - room;
            let pan = -float / rect.size();
            let center = pos2(
                0.5 + pan.x.clamp(-room, room),
                0.5 + pan.y.clamp(-room, room),
            );
            egui::Image::new(texture)
                .corner_radius(radius)
                .uv(Rect::from_center_size(center, vec2(2.0 * half, 2.0 * half)))
                .paint_at(ui, rect);
        }
        None => {
            let painter = ui.painter().with_clip_rect(rect);
            painter.rect_filled(rect, radius, palette.surface);
            let step = (rect.height() / 6.0).max(8.0);
            let line = Stroke::new(1.0_f32, palette.border.gamma_multiply(0.7));
            let mut x = rect.left() + step;
            while x < rect.right() {
                painter.vline(x, rect.y_range(), line);
                x += step;
            }
            let mut y = rect.top() + step;
            while y < rect.bottom() {
                painter.hline(rect.x_range(), y, line);
                y += step;
            }
            let initials: String = name
                .split_whitespace()
                .filter_map(|word| word.chars().next())
                .take(2)
                .flat_map(char::to_uppercase)
                .collect();
            painter.text(
                rect.center() + float * 1.5,
                egui::Align2::CENTER_CENTER,
                if initials.is_empty() {
                    "?".to_owned()
                } else {
                    initials
                },
                FontId::new(rect.height() * 0.26, theme::heading_family()),
                palette.text_weak,
            );
        }
    }
}

/// A project's options, for its ⋯ button and its right-click menu.
pub(super) fn project_menu(ui: &mut egui::Ui, action: &mut Option<CardAction>) {
    let palette = theme::palette(ui.ctx());
    let mut item = |ui: &mut egui::Ui, icon: &str, label: &str, picked: CardAction| {
        if ui.button(theme::with_icon(icon, label)).clicked() {
            *action = Some(picked);
        }
    };
    item(ui, icons::FOLDER_OPEN, "Open", CardAction::Open);
    item(ui, icons::GEAR, "Settings…", CardAction::Settings);
    item(
        ui,
        icons::CLOCK_COUNTER_CLOCKWISE,
        "Version History…",
        CardAction::History,
    );
    item(
        ui,
        icons::ARROW_SQUARE_OUT,
        "Show in File Manager",
        CardAction::Reveal,
    );
    if ui
        .button(theme::with_icon(icons::X_CIRCLE, "Remove from List"))
        .on_hover_text("Stops tracking it here. The files are kept.")
        .clicked()
    {
        *action = Some(CardAction::Forget);
    }
    ui.separator();
    let delete = RichText::new(theme::with_icon(icons::TRASH, "Delete…")).color(palette.error);
    if ui.button(delete).clicked() {
        *action = Some(CardAction::Delete);
    }
}

/// The ⋯ button of a card or row, shown while `visible` (the card is
/// hovered) or while its menu is open.
pub(super) fn card_menu(
    ui: &mut egui::Ui,
    rect: Rect,
    id: egui::Id,
    visible: bool,
) -> Option<CardAction> {
    let response = ui.interact(rect, id, Sense::click());
    let open = egui::Popup::is_id_open(ui.ctx(), egui::Popup::default_response_id(&response));
    let shown = theme::ease(
        ui.ctx(),
        id.with("shown"),
        visible || open || response.has_focus(),
    );
    if shown > 0.0 {
        let palette = theme::palette(ui.ctx());
        let fill = if response.hovered() || open {
            palette.control_active
        } else {
            palette.elevated
        };
        ui.painter().rect(
            rect,
            6,
            fill.gamma_multiply(shown),
            Stroke::new(1.0_f32, palette.border.gamma_multiply(shown)),
            egui::StrokeKind::Inside,
        );
        ui.painter().text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            icons::DOTS_THREE,
            FontId::proportional(16.0),
            palette.text_strong.gamma_multiply(shown),
        );
    }
    let response = response.on_hover_text("More");
    let mut action = None;
    egui::Popup::menu(&response).show(|ui| project_menu(ui, &mut action));
    action
}

/// One project as a card: its picture, name and when it was edited. Clicking
/// it opens it; ⋯ (shown on hover) and right-click have its options.
pub(super) fn project_card(
    ui: &mut egui::Ui,
    link: &ProjectLink,
    thumbnail: Option<&egui::TextureHandle>,
    edited: Option<String>,
    width: f32,
    section: &str,
) -> Option<CardAction> {
    let picture_height = (width * 9.0 / 16.0).round();
    let size = vec2(width, picture_height + 54.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, &link.name));
    let palette = theme::palette(ui.ctx());
    let hovered = ui.rect_contains_pointer(rect);
    let lean = tilt::lean(ui, rect, response.id, hovered);
    let hover = lean.hover;
    let picture = Rect::from_min_size(rect.min, vec2(width, picture_height));
    let mut action = tilt::leaning(ui, picture, CARD_RADIUS, lean, |ui| {
        thumbnail_image(
            ui,
            picture,
            CornerRadius::same(CARD_RADIUS),
            &link.name,
            thumbnail,
            lean.drift(4.0),
        );
        ui.painter().rect_stroke(
            picture,
            CARD_RADIUS,
            Stroke::new(
                1.0_f32,
                theme::mix(
                    palette.border,
                    theme::mix(palette.border_strong, palette.accent, 0.45),
                    hover,
                ),
            ),
            egui::StrokeKind::Inside,
        );
        if response.has_focus() {
            ui.painter().rect_stroke(
                picture.expand(2.0),
                CARD_RADIUS + 2,
                Stroke::new(1.5_f32, palette.accent),
                egui::StrokeKind::Inside,
            );
        }
        let menu = Rect::from_min_size(
            pos2(picture.right() - 36.0, picture.top() + 8.0),
            vec2(28.0, 24.0),
        );
        card_menu(
            ui,
            menu,
            egui::Id::new(("card menu", section, &link.folder)),
            hovered,
        )
    });

    // The name and details sit under the picture, lifting with it.
    let rise = if lean.motion { -2.0 * hover } else { 0.0 };
    let text_left = rect.left() + 2.0;
    let text_width = width - 4.0;
    let name = widgets::elided(
        ui,
        link.name.clone(),
        FontId::new(type_scale::BODY + 0.5, theme::medium_family()),
        palette.text_strong,
        text_width,
        1,
    );
    let name_top = picture.bottom() + 10.0 + rise;
    ui.painter()
        .galley(pos2(text_left, name_top), name, palette.text_strong);
    let detail = match &edited {
        Some(edited) => format!("Edited {edited}"),
        None => link.folder.display().to_string(),
    };
    let detail = match link
        .description
        .lines()
        .next()
        .filter(|line| !line.is_empty())
    {
        Some(description) => format!("{detail}  ·  {description}"),
        None => detail,
    };
    let detail = widgets::elided(
        ui,
        detail,
        FontId::proportional(type_scale::SMALL),
        palette.text_weak,
        text_width,
        1,
    );
    ui.painter()
        .galley(pos2(text_left, name_top + 20.0), detail, palette.text_weak);

    let response = response
        .on_hover_text(format!("Open {}", link.folder.display()))
        .on_hover_cursor(egui::CursorIcon::PointingHand);
    response.context_menu(|ui| project_menu(ui, &mut action));
    if action.is_none() && response.clicked() {
        action = Some(CardAction::Open);
    }
    action
}

/// One project as a row of the list view.
pub(super) fn project_row(
    ui: &mut egui::Ui,
    link: &ProjectLink,
    thumbnail: Option<&egui::TextureHandle>,
    edited: Option<String>,
) -> Option<CardAction> {
    let size = vec2(ui.available_width(), 52.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, &link.name));
    let palette = theme::palette(ui.ctx());
    let hovered = ui.rect_contains_pointer(rect);
    widgets::paint_hover(ui, rect, &response, false, theme::radius::CARD);
    if hovered && !response.hovered() {
        // Over the ⋯ button: keep the row lit.
        ui.painter().rect_filled(rect, 8, palette.control_hover);
    }
    let picture = Rect::from_min_size(rect.min + vec2(8.0, 8.0), vec2(36.0 * 16.0 / 9.0, 36.0));
    thumbnail_image(
        ui,
        picture,
        CornerRadius::same(5),
        &link.name,
        thumbnail,
        egui::Vec2::ZERO,
    );
    ui.painter().rect_stroke(
        picture,
        5,
        Stroke::new(1.0_f32, palette.border),
        egui::StrokeKind::Inside,
    );
    let left = picture.right() + 14.0;
    let edited_width = 150.0;
    let text_width = (rect.right() - edited_width - 50.0 - left).max(40.0);
    let name = widgets::elided(
        ui,
        link.name.clone(),
        FontId::new(type_scale::BODY, theme::medium_family()),
        palette.text_strong,
        text_width,
        1,
    );
    ui.painter()
        .galley(pos2(left, rect.top() + 9.0), name, palette.text_strong);
    let path = widgets::elided(
        ui,
        link.folder.display().to_string(),
        FontId::proportional(type_scale::SMALL),
        palette.text_weak,
        text_width,
        1,
    );
    ui.painter()
        .galley(pos2(left, rect.top() + 28.0), path, palette.text_weak);
    if let Some(edited) = edited {
        ui.painter().text(
            rect.right_center() - vec2(50.0, 0.0),
            egui::Align2::RIGHT_CENTER,
            format!("Edited {edited}"),
            FontId::proportional(type_scale::SMALL),
            palette.text_weak,
        );
    }
    let menu = Rect::from_center_size(rect.right_center() - vec2(22.0, 0.0), vec2(28.0, 24.0));
    let mut action = card_menu(ui, menu, egui::Id::new(("row menu", &link.folder)), hovered);
    let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
    response.context_menu(|ui| project_menu(ui, &mut action));
    if action.is_none() && response.clicked() {
        action = Some(CardAction::Open);
    }
    action
}

/// A quiet note in place of the project cards.
pub(super) fn empty_state(ui: &mut egui::Ui, icon: &str, title: &str, text: &str) {
    let palette = theme::palette(ui.ctx());
    ui.add_space(8.0);
    egui::Frame::new()
        .stroke(Stroke::new(1.0_f32, palette.border))
        .corner_radius(CARD_RADIUS)
        .inner_margin(egui::Margin::symmetric(16, 32))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.vertical_centered(|ui| {
                ui.label(RichText::new(icon).size(24.0).color(palette.text_weak));
                ui.add_space(4.0);
                ui.label(
                    RichText::new(title)
                        .family(theme::medium_family())
                        .color(palette.text_strong),
                );
                ui.label(RichText::new(text).color(palette.text_weak));
            });
        });
    ui.add_space(8.0);
}
