use super::*;

impl EditorApp {
    /// The header, which is also the window's title bar, like VS Code's (see
    /// `window_frame.rs`): the logo and menus, the window title in the
    /// middle, then Play's badge, Settings and the window buttons. Its empty
    /// space drags the window.
    pub(in crate::app) fn title_bar(&mut self, ui: &mut egui::Ui) {
        let bar = ui.max_rect();
        // First, so the menus and buttons added after it sit on top.
        window_frame::drag_region(ui, bar);
        if window_frame::buttons(ui, bar) {
            self.request_and_go_ahead(ui.ctx(), Discard::Quit);
        }
        let row_height = ui.spacing().interact_size.y;
        let row = egui::Rect::from_x_y_ranges(
            bar.left() + 10.0 + window_frame::LEFT_INSET
                ..=bar.right() - window_frame::buttons_width() - 6.0,
            bar.center().y - row_height * 0.5..=bar.center().y + row_height * 0.5,
        );
        let free = ui
            .scope_builder(
                egui::UiBuilder::new()
                    .max_rect(row)
                    .layout(egui::Layout::left_to_right(egui::Align::Center)),
                |ui| {
                    // The menus act on the open project, so not while another loads.
                    if self.loading.is_some() {
                        ui.disable();
                    }
                    self.menu_bar(ui)
                },
            )
            .inner;

        // Centred on the window, but kept clear of the menus and buttons.
        let span = free.shrink2(egui::vec2(16.0, 0.0));
        if span.width() > 48.0 {
            let palette = theme::palette(ui.ctx());
            let title = widgets::elided(
                ui,
                self.title(),
                egui::FontId::new(12.5, theme::medium_family()),
                palette.text_secondary,
                span.width(),
                1,
            );
            let size = title.size();
            let x = (bar.center().x - size.x * 0.5).clamp(span.left(), span.right() - size.x);
            ui.painter().galley(
                egui::pos2(x, bar.center().y - size.y * 0.5),
                title,
                palette.text_secondary,
            );
        }
    }

    /// The logo and the menus, then Play's badge and Settings at the right.
    /// Returns the space left between them, for the window title.
    pub(super) fn menu_bar(&mut self, ui: &mut egui::Ui) -> egui::Rect {
        let palette = theme::palette(ui.ctx());
        egui::MenuBar::new().ui(ui, |ui| {
            let (mark, _) = ui.allocate_exact_size(egui::vec2(18.0, 18.0), egui::Sense::hover());
            if let Some(brand) = &self.brand {
                widgets::paint_mark(
                    ui.painter(),
                    mark.shrink(1.0),
                    brand.mark(),
                    palette.text_strong,
                );
            }
            ui.add_space(4.0);
            ui.menu_button("File", |ui| {
                let editing = self.project.is_some() && !self.playing();
                let not_editing = if self.playing() {
                    "Press Stop to edit."
                } else {
                    "Open a project first."
                };
                widgets::section_label(ui, "Project");
                if ui
                    .button(theme::with_icon(icons::FILE_PLUS, "New Project…"))
                    .on_hover_text("Close this project and make a new one.")
                    .clicked()
                {
                    self.request_and_go_ahead(ui.ctx(), Discard::NewProject);
                }
                if ui
                    .button(theme::with_icon(icons::FOLDER_OPEN, "Open Project…"))
                    .clicked()
                {
                    self.request_and_go_ahead(ui.ctx(), Discard::OpenAnother);
                }
                let save = egui::Button::new(theme::with_icon(icons::FLOPPY_DISK, "Save"))
                    .shortcut_text(ui.ctx().format_shortcut(&SAVE_SHORTCUT));
                if ui.add_enabled(self.project.is_some(), save).clicked() {
                    self.save();
                }
                if ui
                    .add_enabled(
                        editing,
                        egui::Button::new(theme::with_icon(
                            icons::CLOCK_COUNTER_CLOCKWISE,
                            "Version History…",
                        )),
                    )
                    .clicked()
                {
                    self.versions_open = true;
                }
                if ui
                    .add_enabled(
                        editing,
                        egui::Button::new(theme::with_icon(icons::HOUSE, "Close Project")),
                    )
                    .clicked()
                {
                    self.request_and_go_ahead(ui.ctx(), Discard::CloseProject);
                }

                ui.separator();
                widgets::section_label(ui, "Import");
                let import = egui::Button::new(theme::with_icon(icons::FILE_ARROW_DOWN, "Import…"));
                if ui
                    .add_enabled(editing, import)
                    .on_hover_text(IMPORT_HOVER)
                    .on_disabled_hover_text(not_editing)
                    .clicked()
                {
                    self.import_with_dialog(axcenity_assets::PROJECT_TEXTURES_FOLDER);
                }

                ui.separator();
                widgets::section_label(ui, "Publish");
                self.build_game_menu(ui, editing);

                ui.separator();
                if ui
                    .add_enabled(editing, egui::Button::new("Open Stylesheet…"))
                    .clicked()
                {
                    self.open_stylesheet_with_dialog();
                }
                if ui
                    .add_enabled(editing, egui::Button::new("New Stylesheet"))
                    .clicked()
                {
                    self.new_stylesheet();
                }
                if ui
                    .add_enabled(
                        self.project.is_some(),
                        egui::Button::new(theme::with_icon(
                            icons::MAGNIFYING_GLASS,
                            "Check Assets",
                        )),
                    )
                    .on_hover_text(
                        "Scan the open project for missing files, invalid references, \
                         import failures, and broken dependencies.",
                    )
                    .on_disabled_hover_text("Open a project first.")
                    .clicked()
                {
                    self.run_asset_check();
                }
                if ui
                    .add_enabled(
                        self.project.is_some(),
                        egui::Button::new(theme::with_icon(
                            icons::SPARKLE,
                            "Set Up VS Code Autocomplete",
                        )),
                    )
                    .on_hover_text(
                        "Add the engine's API to this project for luau-lsp, so VS Code \
                         autocompletes it and checks scripts as Luau.",
                    )
                    .on_disabled_hover_text("Open a project first.")
                    .clicked()
                {
                    self.set_up_autocomplete();
                }
                ui.separator();
                let settings = egui::Button::new(theme::with_icon(icons::GEAR_SIX, "Settings…"))
                    .shortcut_text(ui.ctx().format_shortcut(&settings::SETTINGS_SHORTCUT));
                if ui.add(settings).clicked() {
                    self.settings.open = true;
                }
                ui.separator();
                if ui
                    .button(theme::with_icon(icons::SIGN_OUT, "Quit"))
                    .clicked()
                {
                    self.request_and_go_ahead(ui.ctx(), Discard::Quit);
                }
            });
            ui.menu_button("Edit", |ui| {
                // Nothing can be edited mid-drag, or while the game plays.
                let busy = self.viewport.is_dragging() || self.playing();
                // "Undo Move Pad", so it's clear what will be undone.
                let undo_label = self.history.undo_label().map(str::to_string);
                let undo = egui::Button::new(theme::with_icon(
                    icons::ARROW_COUNTER_CLOCKWISE,
                    &match &undo_label {
                        Some(label) => format!("Undo {label}"),
                        None => "Undo".to_string(),
                    },
                ))
                .shortcut_text(ui.ctx().format_shortcut(&UNDO_SHORTCUT));
                if ui
                    .add_enabled(undo_label.is_some() && !busy, undo)
                    .clicked()
                {
                    self.undo();
                }
                let redo_label = self.history.redo_label().map(str::to_string);
                let redo = egui::Button::new(theme::with_icon(
                    icons::ARROW_CLOCKWISE,
                    &match &redo_label {
                        Some(label) => format!("Redo {label}"),
                        None => "Redo".to_string(),
                    },
                ))
                .shortcut_text(ui.ctx().format_shortcut(&REDO_SHORTCUT));
                if ui
                    .add_enabled(redo_label.is_some() && !busy, redo)
                    .clicked()
                {
                    self.redo();
                }
                ui.separator();

                // These act on the selection, the same as its right-click menu in the Explorer.
                let selected = self.selected;
                let renamable = selected
                    .and_then(|id| self.world.get(id))
                    .is_some_and(properties::can_rename)
                    && !self.playing();
                let editable = selected
                    .is_some_and(|id| tree::check_editable(&self.world, id).is_ok())
                    && !busy;
                let rename = egui::Button::new(theme::with_icon(icons::PENCIL_SIMPLE, "Rename"))
                    .shortcut_text(RENAME_KEY.name());
                if ui.add_enabled(renamable, rename).clicked()
                    && let Some(id) = selected
                {
                    self.explorer.start_rename(&self.world, id);
                }
                let duplicate = egui::Button::new(theme::with_icon(icons::COPY, "Duplicate"))
                    .shortcut_text(ui.ctx().format_shortcut(&DUPLICATE_SHORTCUT));
                if ui.add_enabled(editable, duplicate).clicked() && selected.is_some() {
                    self.duplicate_selected();
                }
                let deletable =
                    selected.is_some_and(|id| tree::check_delete(&self.world, id).is_ok()) && !busy;
                let delete = egui::Button::new(theme::with_icon(icons::TRASH, "Delete"))
                    .shortcut_text(DELETE_KEY_TEXT);
                if ui.add_enabled(deletable, delete).clicked() && selected.is_some() {
                    self.delete_selected();
                }
            });
            ui.menu_button("View", |ui| {
                ui.weak("Theme");
                for choice in theme::ThemeChoice::ALL {
                    let label = match choice {
                        theme::ThemeChoice::System => "Match System",
                        other => other.label(),
                    };
                    let item = egui::Button::selectable(
                        self.appearance.theme == choice,
                        theme::with_icon(choice.icon(), label),
                    );
                    if ui.add(item).clicked() {
                        let appearance = theme::Appearance {
                            theme: choice,
                            ..self.appearance.clone()
                        };
                        self.set_appearance(ui.ctx(), appearance);
                    }
                }
                if ui
                    .button(theme::with_icon(icons::PALETTE, "Appearance…"))
                    .on_hover_text("Accent colour, density, text size and motion.")
                    .clicked()
                {
                    self.settings.open = true;
                    self.settings.page = settings::SettingsPage::Appearance;
                }
                ui.separator();
                ui.weak("Panels");
                for pane in Pane::CLOSABLE {
                    let item = egui::Button::selectable(
                        self.layout.is_open(pane),
                        theme::with_icon(pane.icon(), pane.label()),
                    );
                    if ui
                        .add(item)
                        .on_hover_text("Show or hide it. Drag a panel by its tab to move it.")
                        .clicked()
                    {
                        self.layout.toggle(pane);
                    }
                }
                if ui
                    .button(theme::with_icon(icons::ARROW_COUNTER_CLOCKWISE, "Reset Layout"))
                    .on_hover_text("Put every panel back where it starts.")
                    .clicked()
                {
                    self.layout.reset();
                }
                ui.separator();
                let cones = egui::Button::selectable(
                    self.light_cones,
                    theme::with_icon(icons::FLASHLIGHT, "Light Cones"),
                );
                if ui
                    .add(cones)
                    .on_hover_text(
                        "Show where every light shines in the viewport. Off, only the selected light (or the part it's in) shows its cone.",
                    )
                    .clicked()
                {
                    self.light_cones = !self.light_cones;
                }
                ui.weak("Overlays");
                overlay_menu_item(
                    ui,
                    &mut self.overlays.colliders,
                    icons::CUBE,
                    "Colliders",
                    "The shape each part collides with, including sensors and terrain.",
                );
                overlay_menu_item(
                    ui,
                    &mut self.overlays.raycasts,
                    icons::LINE_SEGMENT,
                    "Raycasts",
                    "Recent workspace:Raycast calls during Play.",
                );
                overlay_menu_item(
                    ui,
                    &mut self.overlays.bounds,
                    icons::SQUARE,
                    "Bounds",
                    "Each part's Size box.",
                );
                overlay_menu_item(
                    ui,
                    &mut self.overlays.emitters,
                    icons::SPEAKER_HIGH,
                    "Emitters",
                    "Where each spatial Sound is heard from.",
                );
                ui.separator();
                if ui
                    .button(theme::with_icon(icons::STAR, "New Ribbon Tab…"))
                    .on_hover_text("Make your own ribbon tab from any of the ribbon's groups.")
                    .clicked()
                {
                    self.ribbon_tabs.edit(None);
                }
            });
            // Only in debug builds (`cargo run`), never in a release.
            if cfg!(debug_assertions) {
                ui.menu_button("Debug", |ui| {
                    let panic =
                        egui::Button::new(theme::with_icon(icons::WARNING, "Panic during Play"));
                    if ui
                        .add_enabled(self.playing(), panic)
                        .on_hover_text(
                            "Makes the engine panic on the next frame, to check that \
                             Play stops and reports it instead of closing the editor.",
                        )
                        .on_disabled_hover_text("Press Play first.")
                        .clicked()
                        && let Some(session) = &mut self.play
                    {
                        session.panic_next_frame();
                    }
                });
            }
            ui.menu_button("Help", |ui| {
                if ui
                    .add_enabled(
                        self.project.is_some(),
                        egui::Button::selectable(
                            self.learn_open,
                            theme::with_icon(icons::BOOK_OPEN, "Learn"),
                        ),
                    )
                    .on_hover_text("Tutorials and the API Reference, beside your project")
                    .on_disabled_hover_text("Learn is on the start screen until a project is open.")
                    .clicked()
                {
                    self.learn_open = !self.learn_open;
                }
                if ui
                    .button(theme::with_icon(icons::INFO, "About Axcenity"))
                    .clicked()
                {
                    self.about_open = true;
                }
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 10.0;
                if widgets::icon_button(ui, icons::GEAR_SIX, "Settings", self.settings.open)
                    .clicked()
                {
                    self.settings.open = !self.settings.open;
                }
                if self.playing() {
                    playing_badge(ui, self.paused());
                }
                ui.available_rect_before_wrap()
            })
            .inner
        })
        .inner
    }

    /// File > Build Game ▸: a game anyone can run without the editor, for
    /// this OS or another, and whether it shows the splash. `enabled` is
    /// false with no project open, or during Play.
    fn build_game_menu(&mut self, ui: &mut egui::Ui, enabled: bool) {
        ui.menu_button(theme::with_icon(icons::FILE_ARROW_UP, "Build Game"), |ui| {
            if ui
                .add_enabled(enabled, egui::Button::new("For this OS"))
                .clicked()
            {
                self.export_game_with_dialog();
                ui.close();
            }
            ui.separator();
            for (target, label) in [
                (files::ExportTarget::Windows, "For Windows"),
                (files::ExportTarget::MacOS, "For macOS"),
                (files::ExportTarget::Linux, "For Linux"),
            ] {
                if ui.add_enabled(enabled, egui::Button::new(label)).clicked() {
                    self.export_game_for_with_dialog(target);
                    ui.close();
                }
            }
            if !enabled {
                ui.weak(if self.playing() {
                    "Press Stop before building."
                } else {
                    "Open a project first."
                });
            }
            ui.separator();
            self.loading_screen_menu(ui);
        });
    }

    /// The Build Game menu's loading screen: the Axcenity splash, none, or
    /// one of the Workspace's ScreenGuis, for how long and whether it waits
    /// for a script.
    fn loading_screen_menu(&mut self, ui: &mut egui::Ui) {
        let Some(project) = &self.project else {
            ui.weak("Loading screen: open a project first.");
            return;
        };
        let (show_splash, mut loading) = (project.show_splash, project.loading_screen.clone());
        ui.label("Loading screen");
        #[derive(PartialEq)]
        enum Choice {
            Splash,
            Nothing,
            Gui(String),
        }
        let current = match loading.screen_gui.as_str() {
            "" if show_splash => Choice::Splash,
            "" => Choice::Nothing,
            name => Choice::Gui(name.to_owned()),
        };
        let mut choices = vec![
            (Choice::Splash, "Axcenity splash".to_owned()),
            (Choice::Nothing, "None".to_owned()),
        ];
        let workspace = self.world.workspace();
        for id in self.world.descendants(workspace) {
            if let Some(gui) = self.world.get(id)
                && gui.class() == ClassName::ScreenGui
                && !choices
                    .iter()
                    .any(|(choice, _)| *choice == Choice::Gui(gui.name().to_owned()))
            {
                choices.push((Choice::Gui(gui.name().to_owned()), gui.name().to_owned()));
            }
        }
        // A saved name with no ScreenGui left: still shown, so it can be changed.
        if let Choice::Gui(name) = &current
            && !choices.iter().any(|(choice, _)| choice == &current)
        {
            choices.push((Choice::Gui(name.clone()), format!("{name} (missing)")));
        }
        let mut picked = None;
        for (choice, label) in choices {
            if ui.radio(choice == current, label).clicked() && choice != current {
                picked = Some(choice);
            }
        }
        let none_shown = picked.as_ref().unwrap_or(&current) == &Choice::Nothing;
        match picked {
            Some(Choice::Splash) => {
                loading.screen_gui.clear();
                self.set_loading_screen(loading.clone());
                self.set_show_splash(true);
            }
            Some(Choice::Nothing) => {
                loading.screen_gui.clear();
                self.set_loading_screen(loading.clone());
                self.set_show_splash(false);
            }
            Some(Choice::Gui(name)) => {
                loading.screen_gui = name;
                self.set_loading_screen(loading.clone());
            }
            None => {}
        }
        if !loading.screen_gui.is_empty() {
            let mut seconds = loading.min_seconds;
            let response = ui
                .horizontal(|ui| {
                    ui.label("Show for at least");
                    ui.add(
                        egui::DragValue::new(&mut seconds)
                            .range(0.0..=60.0)
                            .speed(0.1)
                            .fixed_decimals(1)
                            .suffix(" s"),
                    )
                })
                .inner;
            // Saved when a drag ends or a typed value is in, not every frame.
            loading.min_seconds = seconds;
            if let Some(project) = &mut self.project {
                project.loading_screen.min_seconds = seconds;
            }
            if response.drag_stopped() || (response.changed() && !response.dragged()) {
                self.set_loading_screen(loading.clone());
            }
        }
        if !none_shown {
            let mut wait = loading.wait_for_script;
            if ui
                .checkbox(&mut wait, "Wait for a script")
                .on_hover_text(
                    "Stays up until a LocalScript calls \
                     game:GetService(\"ReplicatedFirst\"):RemoveDefaultLoadingScreen()",
                )
                .changed()
            {
                loading.wait_for_script = wait;
                self.set_loading_screen(loading);
            }
        }
    }
}
