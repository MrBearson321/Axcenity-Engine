use super::*;

/// A dialog's title, with an optional line under it.
/// The name over a field in a dialog.
pub(super) fn field_label(ui: &mut egui::Ui, palette: &Palette, text: &str) {
    ui.label(
        RichText::new(text)
            .family(theme::medium_family())
            .size(type_scale::SMALL)
            .color(palette.text_secondary),
    );
    ui.add_space(theme::space::XXS);
}

/// A problem with what was typed, in red, with what to do about it.
pub(super) fn error_line(ui: &mut egui::Ui, palette: &Palette, error: Option<&str>) {
    if let Some(error) = error {
        ui.add_space(8.0);
        ui.add(
            egui::Label::new(
                RichText::new(theme::with_icon(icons::WARNING_CIRCLE, error)).color(palette.error),
            )
            .wrap(),
        );
    }
}

/// A dialog's buttons at its bottom right: Cancel, and the main one (its
/// icon, label, kind and whether it can be pressed) right of it. Returns
/// whether each was picked.
pub(super) fn dialog_buttons(
    ui: &mut egui::Ui,
    (icon, label, kind, enabled): (&'static str, &str, dialog::Kind, bool),
    enter: dialog::Enter,
) -> (bool, bool) {
    let picked = dialog::buttons(
        ui,
        &[
            dialog::Button::cancel(),
            dialog::Button::new(icon, label, kind).enabled(enabled),
        ],
        enter,
    );
    (picked == Some(1), picked == Some(0))
}

/// How big a template's tile is in the New Project dialog.
pub(super) const TEMPLATE_TILE: egui::Vec2 = vec2(148.0, 128.0);

/// A template to pick in the New Project dialog: its picture (or its icon,
/// for the blank templates), its name and a line of what it is.
pub(super) fn template_choice(
    ui: &mut egui::Ui,
    palette: &Palette,
    template: Template,
    picked: bool,
    picture: Option<&egui::TextureHandle>,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(TEMPLATE_TILE, Sense::click());
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::RadioButton, true, picked, template.name())
    });
    let hover = theme::ease(ui.ctx(), response.id.with("hover"), response.hovered());
    let fill = if picked {
        palette.accent_soft
    } else {
        palette.control.gamma_multiply(hover)
    };
    let stroke = if picked {
        palette.accent
    } else {
        theme::mix(palette.border, palette.border_strong, hover)
    };
    ui.painter().rect(
        rect,
        8,
        fill,
        Stroke::new(1.0_f32, stroke),
        egui::StrokeKind::Inside,
    );
    if response.has_focus() && !picked {
        ui.painter().rect_stroke(
            rect,
            8,
            Stroke::new(1.0_f32, palette.accent),
            egui::StrokeKind::Inside,
        );
    }
    let inner = rect.shrink(5.0);
    let image = Rect::from_min_size(inner.min, vec2(inner.width(), inner.width() * 9.0 / 16.0));
    template_picture(
        ui,
        palette,
        template,
        image,
        picture,
        CornerRadius::same(5),
        hover,
        egui::Vec2::ZERO,
    );
    let color = if picked {
        palette.accent_text
    } else {
        palette.text_strong
    };
    ui.painter().text(
        pos2(inner.left() + 2.0, image.bottom() + 12.0),
        egui::Align2::LEFT_CENTER,
        template.name(),
        FontId::new(type_scale::BODY, theme::medium_family()),
        color,
    );
    let line = widgets::elided(
        ui,
        template.description(),
        FontId::proportional(type_scale::CAPTION),
        palette.text_weak,
        inner.width() - 4.0,
        1,
    );
    ui.painter().galley(
        pos2(inner.left() + 2.0, image.bottom() + 22.0),
        line,
        palette.text_weak,
    );
    response
        .on_hover_text(template.description())
        .on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// A template's picture in `rect`: the built-in screenshot of a genre
/// template, or a tile with the template's icon.
/// A template's picture, or its icon on a tile when it has none. `float`
/// is the hover's parallax: the picture pans inside its frame (zoomed in a
/// little so no edge shows) and the icon drifts, both toward the pointer.
#[allow(clippy::too_many_arguments)]
pub(super) fn template_picture(
    ui: &egui::Ui,
    palette: &Palette,
    template: Template,
    rect: Rect,
    picture: Option<&egui::TextureHandle>,
    radius: CornerRadius,
    hover: f32,
    float: egui::Vec2,
) {
    match picture {
        Some(texture) => {
            let zoom = 1.0 + 0.06 * hover;
            let half = 0.5 / zoom;
            let room = 0.5 - half;
            let pan = -float / rect.size() / zoom;
            let center = egui::pos2(
                0.5 + pan.x.clamp(-room, room),
                0.5 + pan.y.clamp(-room, room),
            );
            egui::Image::new(texture)
                .corner_radius(radius)
                .uv(Rect::from_center_size(center, vec2(2.0 * half, 2.0 * half)))
                .paint_at(ui, rect);
        }
        None => {
            ui.painter().rect(
                rect,
                radius,
                theme::mix(palette.surface, palette.control_hover, hover),
                Stroke::new(1.0_f32, palette.border),
                egui::StrokeKind::Inside,
            );
            icon_glow(
                ui,
                rect.center() + float * 2.0,
                rect.height() * 0.45,
                hover,
                palette,
            );
            ui.painter().text(
                rect.center() + float * 2.0,
                egui::Align2::CENTER_CENTER,
                template_icon(template),
                FontId::proportional(rect.height() * (0.36 + 0.03 * hover)),
                theme::mix(palette.text_secondary, palette.text_strong, hover),
            );
        }
    }
}

pub(super) fn template_icon(template: Template) -> &'static str {
    match template {
        Template::Baseplate => icons::SQUARE,
        Template::Empty => icons::FILE_DASHED,
        Template::Starter => icons::SPARKLE,
        Template::FirstPerson => icons::CROSSHAIR,
        Template::ThirdPerson => icons::PERSON_SIMPLE_RUN,
        Template::Vehicle => icons::CAR_PROFILE,
        Template::TopDown => icons::MAP_TRIFOLD,
        Template::Arena => icons::USERS_THREE,
    }
}

/// A card to start a new project from `template`: an icon tile, the name
/// and what it starts with.
pub(super) fn template_card(ui: &mut egui::Ui, template: Template, width: f32) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(vec2(width, 68.0), Sense::click());
    response
        .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, template.name()));
    let palette = theme::palette(ui.ctx());
    let lean = tilt::lean(ui, rect, response.id, response.hovered());
    let hover = lean.hover;
    tilt::leaning(ui, rect, CARD_RADIUS, lean, |ui| {
        card_surface(ui, rect, &response, hover, &palette);
        // The icon floats above the card: it drifts further than the card
        // leans, and glows in the accent while hovered.
        let tile = Rect::from_min_size(rect.min + vec2(14.0, 14.0), vec2(40.0, 40.0));
        let float = lean.drift(3.0);
        icon_glow(ui, tile.center() + float * 1.5, 30.0, hover, &palette);
        ui.painter().rect(
            tile.translate(float),
            8,
            theme::mix(palette.surface, palette.control_hover, hover),
            Stroke::new(
                1.0_f32,
                theme::mix(palette.border, palette.accent.gamma_multiply(0.6), hover),
            ),
            egui::StrokeKind::Inside,
        );
        ui.painter().text(
            tile.center() + float * 1.6,
            egui::Align2::CENTER_CENTER,
            template_icon(template),
            FontId::proportional(theme::size::ICON_LARGE + 2.0 * hover),
            theme::mix(palette.text_secondary, palette.accent, hover),
        );
        let left = tile.right() + 12.0;
        ui.painter().text(
            pos2(left, rect.top() + 23.0),
            egui::Align2::LEFT_CENTER,
            template.name(),
            FontId::new(type_scale::BODY, theme::medium_family()),
            palette.text_strong,
        );
        let description = widgets::elided(
            ui,
            template.description(),
            FontId::proportional(type_scale::SMALL),
            palette.text_weak,
            rect.right() - left - 12.0,
            2,
        );
        ui.painter().galley(
            pos2(left, rect.top() + 33.0),
            description,
            palette.text_weak,
        );
    });
    response
        .on_hover_text(format!("New project: {}", template.description()))
        .on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// A card to start a new project from a genre template: its picture, its
/// name and what it is.
pub(super) fn game_template_card(
    ui: &mut egui::Ui,
    template: Template,
    picture: Option<&egui::TextureHandle>,
    width: f32,
) -> egui::Response {
    let image_height = (width - 16.0) * 9.0 / 16.0;
    let (rect, response) = ui.allocate_exact_size(vec2(width, image_height + 74.0), Sense::click());
    response
        .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, template.name()));
    let palette = theme::palette(ui.ctx());
    let lean = tilt::lean(ui, rect, response.id, response.hovered());
    let hover = lean.hover;
    tilt::leaning(ui, rect, CARD_RADIUS, lean, |ui| {
        card_surface(ui, rect, &response, hover, &palette);
        let image =
            Rect::from_min_size(rect.min + vec2(8.0, 8.0), vec2(width - 16.0, image_height));
        template_picture(
            ui,
            &palette,
            template,
            image,
            picture,
            CornerRadius::same(6),
            hover,
            lean.drift(4.0),
        );
        let left = rect.left() + 12.0;
        ui.painter().text(
            pos2(left, image.bottom() + 18.0),
            egui::Align2::LEFT_CENTER,
            template.name(),
            FontId::new(type_scale::BODY, theme::medium_family()),
            palette.text_strong,
        );
        let description = widgets::elided(
            ui,
            template.description(),
            FontId::proportional(type_scale::SMALL),
            palette.text_weak,
            rect.right() - left - 12.0,
            2,
        );
        ui.painter().galley(
            pos2(left, image.bottom() + 30.0),
            description,
            palette.text_weak,
        );
    });
    response
        .on_hover_text(format!("New project: {}", template.description()))
        .on_hover_cursor(egui::CursorIcon::PointingHand)
}

impl StartScreen {
    /// Draws the open dialog, if any. Returns the folder of a project that
    /// was just created, to open it.
    pub(super) fn dialogs(
        &mut self,
        ctx: &egui::Context,
        projects: &mut Vec<ProjectLink>,
    ) -> Option<PathBuf> {
        // The pictures first: the dialog borrows `self.dialog` from here on.
        let pictures: Vec<_> = if matches!(self.dialog, Some(Dialog::New { .. })) {
            Template::ALL
                .into_iter()
                .map(|template| self.picture(ctx, template))
                .collect()
        } else {
            Vec::new()
        };
        let dialog = self.dialog.as_mut()?;
        let palette = theme::palette(ctx);
        let mut close = false;
        let mut created = None;
        let mut deleted = None;
        let mut picked_parent = None;
        let mut restored = None;
        let ((), dismissed) = dialog::modal(ctx, "start dialog", 400.0, |ui| {
            match dialog {
                Dialog::New {
                    name,
                    parent,
                    template,
                    error,
                } => {
                    let columns = 4.0;
                    ui.set_width(TEMPLATE_TILE.x * columns + 8.0 * (columns - 1.0));
                    dialog::header(ui, icons::PLUS, dialog::Tone::Question, "New Project", None);
                    field_label(ui, &palette, "Template");
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing = vec2(8.0, 8.0);
                        for (choice, picture) in Template::ALL.into_iter().zip(&pictures) {
                            let picked = *template == choice;
                            if template_choice(ui, &palette, choice, picked, picture.as_ref())
                                .clicked()
                            {
                                *template = choice;
                            }
                        }
                    });
                    ui.label(
                        RichText::new(template.description())
                            .size(type_scale::SMALL)
                            .color(palette.text_weak),
                    );
                    ui.add_space(10.0);
                    field_label(ui, &palette, "Name");
                    let name_field = ui.add(dialog::text_field(name).hint_text("My Game"));
                    if name.is_empty() && !name_field.has_focus() && error.is_none() {
                        name_field.request_focus();
                    }
                    ui.add_space(10.0);
                    field_label(ui, &palette, "Location");
                    ui.horizontal(|ui| {
                        if ui
                            .add(widgets::button(
                                Role::Secondary,
                                Some(icons::FOLDER_OPEN),
                                "Browse…",
                                &palette,
                            ))
                            .clicked()
                            && let Some(folder) = rfd::FileDialog::new()
                                .set_title("Choose where to make the project")
                                .pick_folder()
                        {
                            *parent = Some(folder);
                        }
                        match parent {
                            Some(parent) => ui.add(
                                egui::Label::new(
                                    RichText::new(parent.display().to_string())
                                        .color(palette.text_secondary),
                                )
                                .truncate(),
                            ),
                            None => {
                                ui.label(RichText::new("No folder chosen").color(palette.text_weak))
                            }
                        };
                    });
                    if let Some(parent) = parent.as_deref()
                        && !name.trim().is_empty()
                    {
                        ui.label(
                            RichText::new(format!(
                                "Makes a new folder: {}",
                                parent.join(name.trim()).display()
                            ))
                            .size(type_scale::SMALL)
                            .color(palette.text_weak),
                        );
                    }
                    error_line(ui, &palette, error.as_deref());
                    let ready = parent.is_some() && !name.trim().is_empty();
                    // The name is the only thing typed, so Enter there creates.
                    let (create, cancel) = dialog_buttons(
                        ui,
                        (icons::PLUS, "Create Project", dialog::Kind::Main, ready),
                        dialog::Enter::FromField,
                    );
                    if create && let Some(parent) = parent.as_deref() {
                        match axcenity_project::create_project_from(parent, name, *template) {
                            Ok(folder) => {
                                picked_parent = Some(parent.to_owned());
                                created = Some(folder);
                                close = true;
                            }
                            Err(err) if err.already_exists() => {
                                *error = Some(
                                    "A folder with that name is already there. Pick \
                                         another name or location."
                                        .into(),
                                );
                            }
                            Err(err) => *error = Some(format!("{err}: {}", err.source)),
                        }
                    }
                    close |= cancel;
                }
                Dialog::Settings {
                    folder,
                    name,
                    description,
                    show_splash,
                    loading_gui,
                    error,
                } => {
                    dialog::header(
                        ui,
                        icons::GEAR_SIX,
                        dialog::Tone::Question,
                        "Project Settings",
                        Some(&folder.display().to_string()),
                    );
                    field_label(ui, &palette, "Name");
                    ui.add(dialog::text_field(name));
                    ui.add_space(10.0);
                    field_label(ui, &palette, "Description");
                    ui.add(
                        egui::TextEdit::multiline(description)
                            .desired_rows(3)
                            .margin(theme::space::margin(theme::space::S, 6.0))
                            .desired_width(f32::INFINITY),
                    );
                    ui.add_space(10.0);
                    ui.checkbox(show_splash, "Show the Axcenity splash in built games");
                    if !loading_gui.is_empty() {
                        ui.label(
                            egui::RichText::new(format!(
                                "Built games show the ScreenGui \"{loading_gui}\" as their \
                                     loading screen instead (File > Build Game in the editor)."
                            ))
                            .color(palette.text_weak),
                        );
                    }
                    error_line(ui, &palette, error.as_deref());
                    let (save, cancel) = dialog_buttons(
                        ui,
                        (
                            icons::FLOPPY_DISK,
                            "Save",
                            dialog::Kind::Main,
                            !name.trim().is_empty(),
                        ),
                        dialog::Enter::Main,
                    );
                    if save {
                        let saved = axcenity_project::set_name(folder, name)
                            .and_then(|()| axcenity_project::set_description(folder, description))
                            .and_then(|()| axcenity_project::set_show_splash(folder, *show_splash));
                        match saved {
                            Ok(()) => {
                                if let Some(link) =
                                    projects.iter_mut().find(|link| link.folder == *folder)
                                {
                                    link.name = name.trim().to_owned();
                                    link.description = description.clone();
                                }
                                close = true;
                            }
                            Err(err) => *error = Some(format!("{err}: {}", err.source)),
                        }
                    }
                    close |= cancel;
                }
                Dialog::Delete { link, error } => {
                    let message = format!(
                        "\"{}\" and everything in its folder will be permanently \
                             deleted. This can't be undone.",
                        link.name
                    );
                    dialog::header(
                        ui,
                        icons::TRASH,
                        dialog::Tone::Danger,
                        "Delete project?",
                        Some(&message),
                    );
                    ui.label(
                        RichText::new(link.folder.display().to_string())
                            .size(type_scale::SMALL)
                            .color(palette.text_weak),
                    );
                    error_line(ui, &palette, error.as_deref());
                    let (delete, cancel) = dialog_buttons(
                        ui,
                        (icons::TRASH, "Delete Project", dialog::Kind::Danger, true),
                        dialog::Enter::Main,
                    );
                    if delete {
                        match delete_project(&link.folder) {
                            Ok(()) => {
                                deleted = Some(link.folder.clone());
                                close = true;
                            }
                            Err(err) => *error = Some(err),
                        }
                    }
                    close |= cancel;
                }
                Dialog::History {
                    folder,
                    error,
                    notice,
                    confirm,
                } => {
                    ui.set_width(460.0);
                    dialog::header(
                        ui,
                        icons::CLOCK_COUNTER_CLOCKWISE,
                        dialog::Tone::Question,
                        "Version History",
                        Some(
                            "Each save keeps the world, scripts and styles; pinned ones are \
                                 never dropped. Restoring puts that version back and keeps the \
                                 current files as a version first. Assets and the project name \
                                 stay as they are.",
                        ),
                    );
                    let versions = axcenity_project::list_versions(folder);
                    if versions.is_empty() {
                        ui.label(
                            RichText::new(
                                "No versions yet. Open the project and save it to keep one.",
                            )
                            .color(palette.text_weak),
                        );
                    }
                    egui::Frame::new()
                        .stroke(Stroke::new(1.0_f32, palette.border))
                        .corner_radius(8)
                        .show(ui, |ui| {
                            egui::ScrollArea::vertical()
                                .max_height(280.0)
                                .auto_shrink([false, true])
                                .show(ui, |ui| {
                                    for (index, version) in versions.iter().enumerate() {
                                        if index > 0 {
                                            widgets::divider(ui);
                                        }
                                        egui::Frame::new()
                                            .inner_margin(egui::Margin::symmetric(12, 8))
                                            .show(ui, |ui| {
                                                ui.horizontal(|ui| {
                                                    ui.vertical(|ui| {
                                                        let label = if version.pinned {
                                                            theme::with_icon(
                                                                theme::filled_icons::PUSH_PIN,
                                                                &version.label,
                                                            )
                                                        } else {
                                                            version.label.clone()
                                                        };
                                                        ui.label(
                                                            RichText::new(label)
                                                                .family(theme::medium_family()),
                                                        );
                                                        ui.label(
                                                            RichText::new(version.when())
                                                                .size(type_scale::SMALL)
                                                                .color(palette.text_weak),
                                                        );
                                                    });
                                                    ui.with_layout(
                                                        egui::Layout::right_to_left(
                                                            egui::Align::Center,
                                                        ),
                                                        |ui| {
                                                            let restore = ui.add(widgets::button(
                                                                Role::Secondary,
                                                                Some(
                                                                    icons::CLOCK_COUNTER_CLOCKWISE,
                                                                ),
                                                                "Restore",
                                                                &palette,
                                                            ));
                                                            if restore.clicked() {
                                                                *confirm = Some(version.id.clone());
                                                            }
                                                        },
                                                    );
                                                });
                                            });
                                    }
                                });
                        });
                    // Restore asks first.
                    if let Some(version) = confirm
                        .clone()
                        .and_then(|id| versions.iter().find(|version| version.id == id).cloned())
                    {
                        ui.add_space(8.0);
                        ui.label(format!(
                            "Restore \"{}\" ({})? The current files are kept first as a \
                                 pinned version named \"Before restore\".",
                            version.label,
                            version.when()
                        ));
                        ui.horizontal(|ui| {
                            if ui
                                .add(widgets::button(
                                    Role::Primary,
                                    Some(icons::CLOCK_COUNTER_CLOCKWISE),
                                    "Restore",
                                    &palette,
                                ))
                                .clicked()
                            {
                                *confirm = None;
                                match axcenity_project::restore_version(folder, &version.id) {
                                    Ok(()) => {
                                        *notice = Some(format!(
                                            "Restored the version from {}.",
                                            version.when()
                                        ));
                                        *error = None;
                                        restored = Some(folder.clone());
                                    }
                                    Err(err) => *error = Some(err.to_string()),
                                }
                            }
                            if ui
                                .add(widgets::button(Role::Secondary, None, "Cancel", &palette))
                                .clicked()
                            {
                                *confirm = None;
                            }
                        });
                    }
                    if let Some(notice) = notice {
                        ui.add_space(6.0);
                        ui.label(
                            RichText::new(theme::with_icon(icons::CHECK_CIRCLE, notice))
                                .color(palette.success),
                        );
                    }
                    error_line(ui, &palette, error.as_deref());
                    close |= dialog::buttons(
                        ui,
                        &[dialog::Button::new(icons::X, "Close", dialog::Kind::Plain)],
                        dialog::Enter::Main,
                    )
                    .is_some();
                }
            }
        });
        if let Some(parent) = picked_parent {
            self.last_parent = Some(parent);
        }
        if let Some(folder) = deleted {
            self.thumbnails.remove(&folder);
            projects.retain(|link| link.folder != folder);
        }
        if let Some(folder) = restored {
            self.edited.remove(&folder);
        }
        if close || dismissed {
            self.dialog = None;
        }
        created
    }
}
