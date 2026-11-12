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
            bar.left() + 10.0 + window_frame::left_inset(ui.ctx())
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
    pub(in crate::app) fn menu_bar(&mut self, ui: &mut egui::Ui) -> egui::Rect {
        let palette = theme::palette(ui.ctx());
        egui::MenuBar::new()
            .ui(ui, |ui| {
                let (mark, _) =
                    ui.allocate_exact_size(egui::vec2(18.0, 18.0), egui::Sense::hover());
                if let Some(brand) = &self.brand {
                    widgets::paint_mark(
                        ui.painter(),
                        mark.shrink(1.0),
                        brand.mark(),
                        palette.text_strong,
                    );
                }
                ui.add_space(4.0);
                // On macOS they're in the system menu bar instead (`native_menu.rs`).
                if !window_frame::NATIVE_MENU {
                    self.header_menus(ui);
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = 10.0;
                    if widgets::icon_button(ui, icons::GEAR_SIX, "Settings", self.settings.open)
                        .clicked()
                    {
                        self.settings.open = !self.settings.open;
                    }
                    self.update_chip(ui);
                    self.team_chip(ui);
                    self.repo_chip(ui);
                    if self.playing() {
                        playing_badge(ui, self.paused());
                    }
                    ui.available_rect_before_wrap()
                })
                .inner
            })
            .inner
    }

    /// File, Edit, View, Viewport, Debug and Help, drawn in the header on
    /// Windows and Linux. macOS shows the same items in its menu bar
    /// (`app/native_menu.rs`); a change here needs one there too.
    fn header_menus(&mut self, ui: &mut egui::Ui) {
        ui.menu_button("File", |ui| {
            let editing = self.project.is_some() && !self.playing();
            let not_editing = if self.playing() {
                "Press Stop to edit."
            } else {
                "Open a project first."
            };
            widgets::section_label(ui, "Project");
            if ui
                .button(item(ui, icons::FILE_PLUS, "New Project…"))
                .on_hover_text("Close this project and make a new one.")
                .clicked()
            {
                self.request_and_go_ahead(ui.ctx(), Discard::NewProject);
            }
            if ui
                .button(item(ui, icons::FOLDER_OPEN, "Open Project…"))
                .clicked()
            {
                self.request_and_go_ahead(ui.ctx(), Discard::OpenAnother);
            }
            let save = egui::Button::new(item(ui, icons::FLOPPY_DISK, "Save"))
                .shortcut_text(crate::shortcuts::Action::Save.menu_text(ui.ctx()));
            if ui.add_enabled(self.project.is_some(), save).clicked() {
                self.save();
            }
            if ui
                .add_enabled(
                    editing,
                    egui::Button::new(item(ui, icons::CLOCK_COUNTER_CLOCKWISE, "Version History…")),
                )
                .clicked()
            {
                self.version_history.open = true;
            }
            if ui
                .add_enabled(
                    editing,
                    egui::Button::new(item(ui, icons::PUSH_PIN, "Save Checkpoint…")),
                )
                .on_hover_text(
                    "Save with a message, and keep that version however many saves follow.",
                )
                .clicked()
            {
                self.version_history.open_for_checkpoint();
            }
            if ui
                .add_enabled(
                    editing,
                    egui::Button::new(item(ui, icons::ARCHIVE, "Back Up Project")),
                )
                .on_hover_text(
                    "Write a .zip of the project to its backup folder (asked the first time).",
                )
                .clicked()
            {
                self.back_up_project(false);
            }
            if ui
                .add_enabled(
                    self.project.is_some(),
                    egui::Button::new(item(ui, icons::GAME_CONTROLLER, "Input…")),
                )
                .on_hover_text(
                    "The game's actions (Move, Jump, Interact…) and the keys, mouse \
                         buttons and controller buttons bound to each.",
                )
                .on_disabled_hover_text("Open a project first.")
                .clicked()
            {
                self.input_settings.open = true;
            }
            if ui
                .add_enabled(
                    editing,
                    egui::Button::new(item(ui, icons::HOUSE, "Close Project")),
                )
                .clicked()
            {
                self.request_and_go_ahead(ui.ctx(), Discard::CloseProject);
            }

            ui.separator();
            widgets::section_label(ui, "Import");
            let import = egui::Button::new(item(ui, icons::FILE_ARROW_DOWN, "Import…"));
            if ui
                .add_enabled(editing, import)
                .on_hover_text(IMPORT_HOVER)
                .on_disabled_hover_text(not_editing)
                .clicked()
            {
                self.import_with_dialog(axcenity_assets::PROJECT_TEXTURES_FOLDER);
            }

            ui.separator();
            widgets::section_label(ui, "Collaborate");
            if ui
                .add_enabled(
                    editing && self.team.is_none(),
                    egui::Button::new(item(ui, icons::SHARE_NETWORK, "Start Collaborate Session…")),
                )
                .on_hover_text("Host a session others can join with an invite link.")
                .clicked()
            {
                self.share_with_team_create();
            }
            if ui
                .add_enabled(
                    editing && self.team.is_none(),
                    egui::Button::new(item(ui, icons::SIGN_IN, "Join Collaborate Session…")),
                )
                .on_hover_text("Join someone's session with their invite link.")
                .clicked()
            {
                self.join_team_create();
            }
            if ui
                .button(item(ui, icons::GEAR, "Set Up Collaborate…"))
                .on_hover_text("Get Tailscale ready so people anywhere can join your sessions.")
                .clicked()
            {
                self.set_up_collaborate();
            }

            ui.separator();
            widgets::section_label(ui, "Publish");
            self.build_game_menu(ui, editing);

            ui.separator();
            if ui
                .add_enabled(editing, egui::Button::new(plain(ui, "Open Stylesheet…")))
                .clicked()
            {
                self.open_stylesheet_with_dialog();
            }
            if ui
                .add_enabled(editing, egui::Button::new(plain(ui, "New Stylesheet")))
                .clicked()
            {
                self.new_stylesheet();
            }
            if ui
                .add_enabled(
                    self.project.is_some(),
                    egui::Button::new(item(ui, icons::MAGNIFYING_GLASS, "Check Assets")),
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
                    egui::Button::new(item(ui, icons::SPARKLE, "Set Up VS Code Autocomplete")),
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
            let settings = egui::Button::new(item(ui, icons::GEAR_SIX, "Settings…"))
                .shortcut_text(crate::shortcuts::Action::Settings.menu_text(ui.ctx()));
            if ui.add(settings).clicked() {
                self.settings.open = true;
            }
            ui.separator();
            if ui.button(item(ui, icons::SIGN_OUT, "Quit")).clicked() {
                self.request_and_go_ahead(ui.ctx(), Discard::Quit);
            }
        });
        ui.menu_button("Edit", |ui| {
            // Nothing can be edited mid-drag, or while the game plays.
            let busy = self.viewport.is_dragging() || self.playing();
            // "Undo Move Pad", so it's clear what will be undone.
            let undo_label = self.history.undo_label().map(str::to_string);
            let undo = egui::Button::new(item(
                ui,
                icons::ARROW_COUNTER_CLOCKWISE,
                &match &undo_label {
                    Some(label) => format!("Undo {label}"),
                    None => "Undo".to_string(),
                },
            ))
            .shortcut_text(crate::shortcuts::Action::Undo.menu_text(ui.ctx()));
            if ui
                .add_enabled(undo_label.is_some() && !busy, undo)
                .clicked()
            {
                self.undo();
            }
            let redo_label = self.history.redo_label().map(str::to_string);
            let redo = egui::Button::new(item(
                ui,
                icons::ARROW_CLOCKWISE,
                &match &redo_label {
                    Some(label) => format!("Redo {label}"),
                    None => "Redo".to_string(),
                },
            ))
            .shortcut_text(crate::shortcuts::Action::Redo.menu_text(ui.ctx()));
            if ui
                .add_enabled(redo_label.is_some() && !busy, redo)
                .clicked()
            {
                self.redo();
            }
            ui.add_enabled_ui(!busy, |ui| {
                crate::widgets::menu_button(
                    ui,
                    Some(icons::CLOCK_COUNTER_CLOCKWISE),
                    "Undo History",
                    |ui| {
                        if let Some(jump) = undo_history(ui, &self.history) {
                            self.jump_in_history(jump);
                            ui.close();
                        }
                    },
                );
            });
            ui.separator();

            // These act on the selection, the same as its right-click menu in the Explorer.
            let selected = self.selected;
            let renamable = selected
                .and_then(|id| self.world.get(id))
                .is_some_and(properties::can_rename)
                && !self.playing();
            let editable =
                selected.is_some_and(|id| tree::check_editable(&self.world, id).is_ok()) && !busy;
            let rename = egui::Button::new(item(ui, icons::PENCIL_SIMPLE, "Rename"))
                .shortcut_text(crate::shortcuts::Action::Rename.menu_text(ui.ctx()));
            if ui.add_enabled(renamable, rename).clicked()
                && let Some(id) = selected
            {
                self.explorer.start_rename(&self.world, id);
            }
            let duplicate = egui::Button::new(item(ui, icons::COPY, "Duplicate"))
                .shortcut_text(crate::shortcuts::Action::Duplicate.menu_text(ui.ctx()));
            if ui.add_enabled(editable, duplicate).clicked() && selected.is_some() {
                self.duplicate_selected();
            }
            let deletable =
                selected.is_some_and(|id| tree::check_delete(&self.world, id).is_ok()) && !busy;
            let delete = egui::Button::new(item(ui, icons::TRASH, "Delete"))
                .shortcut_text(crate::shortcuts::Action::Delete.menu_text(ui.ctx()));
            if ui.add_enabled(deletable, delete).clicked() && selected.is_some() {
                self.delete_selected();
            }
            ui.separator();
            let search = egui::Button::new(item(ui, icons::MAGNIFYING_GLASS, "Search Everything…"))
                .shortcut_text(crate::shortcuts::Action::Search.menu_text(ui.ctx()));
            if ui.add(search).clicked() {
                self.palette.toggle();
            }
            let palette = egui::Button::new(item(ui, icons::COMMAND, "Command Palette"))
                .shortcut_text(crate::shortcuts::Action::Palette.menu_text(ui.ctx()));
            if ui.add(palette).clicked() {
                self.palette.toggle_commands();
            }
            let quick = egui::Button::new(item(ui, icons::FILE_MAGNIFYING_GLASS, "Quick Open…"))
                .shortcut_text(crate::shortcuts::Action::QuickOpen.menu_text(ui.ctx()));
            if ui.add_enabled(self.project.is_some(), quick).clicked() {
                self.palette.toggle();
            }
            let find = egui::Button::new(item(ui, icons::MAGNIFYING_GLASS, "Find in Scripts…"))
                .shortcut_text(crate::shortcuts::Action::FindInScripts.menu_text(ui.ctx()));
            if ui.add_enabled(self.project.is_some(), find).clicked() {
                self.find.show_and_focus();
                self.search_scripts();
            }
        });
        // View: the editor's own windows and how they're laid out. The
        // look of the whole editor (theme, accent, text size) is in
        // Settings, one click away on the gear at the right.
        ui.menu_button("View", |ui| {
            widgets::section_label(ui, "Panels");
            for pane in Pane::CLOSABLE {
                let item = egui::Button::selectable(
                    self.layout.is_open(pane),
                    item(ui, pane.icon(), pane.label()),
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
                .button(item(ui, icons::ARROW_COUNTER_CLOCKWISE, "Reset Layout"))
                .on_hover_text("Put every panel back where it starts.")
                .clicked()
            {
                self.layout.reset();
            }
            crate::widgets::menu_button(ui, Some(icons::LAYOUT), "Layout", |ui| {
                self.layout_menu(ui);
            });
            ui.separator();
            if ui
                .button(item(ui, icons::STAR, "New Ribbon Tab…"))
                .on_hover_text("Make your own ribbon tab from any of the ribbon's groups.")
                .clicked()
            {
                self.ribbon_tabs.edit(None);
            }
        });
        // Viewport: what the Scene view draws over the world while editing.
        ui.menu_button("Viewport", |ui| self.viewport_menu(ui));
        ui.menu_button("Debug", |ui| {
            if ui
                .button(item(ui, icons::GAUGE, "Graphics Benchmark…"))
                .on_hover_text(
                    "Measure how this computer draws the viewport and get recommended \
                         editor graphics settings. Changes nothing until you apply them.",
                )
                .clicked()
            {
                self.open_graphics_benchmark();
            }
            // Only in debug builds (`cargo run`), never in a release.
            if cfg!(debug_assertions) {
                ui.separator();
                let panic = egui::Button::new(item(ui, icons::WARNING, "Panic during Play"));
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
            }
        });
        ui.menu_button("Help", |ui| {
            if ui
                .add_enabled(
                    self.project.is_some(),
                    egui::Button::selectable(self.learn_open, item(ui, icons::BOOK_OPEN, "Learn")),
                )
                .on_hover_text("Tutorials and the API Reference, beside your project")
                .on_disabled_hover_text("Learn is on the start screen until a project is open.")
                .clicked()
            {
                self.learn_open = !self.learn_open;
            }
            if ui
                .button(item(ui, icons::GIT_BRANCH, "Version Explorer"))
                .on_hover_text(
                    "This build's version, the releases, features, known issues, \
                         and what changed between two versions",
                )
                .clicked()
            {
                self.layout.open(Pane::VersionExplorer);
            }
            if ui
                .button(item(ui, icons::ARROW_CIRCLE_UP, "Check for Updates…"))
                .clicked()
            {
                self.open_updates();
            }
            if ui.button(item(ui, icons::INFO, "About Axcenity")).clicked() {
                self.about_open = true;
            }
        });
    }

    /// Menu bar > Viewport: the guides drawn over the Scene view while
    /// editing, what it shows (View Mode) and the debug overlays. These are
    /// the user's own and saved between runs; Settings > Viewport has them
    /// all too, with the grid's strength.
    pub(in crate::app) fn viewport_menu(&mut self, ui: &mut egui::Ui) {
        widgets::section_label(ui, "Guides");
        overlay_menu_item(
            ui,
            &mut self.grid,
            icons::GRID_NINE,
            "Grid",
            "The measuring grid on the baseplate, with its distances in studs.",
        );
        overlay_menu_item(
            ui,
            &mut self.view_cube,
            icons::CUBE_FOCUS,
            "View Cube",
            "The cube in the viewport's corner: click a face, edge or corner to look from there.",
        );
        overlay_menu_item(
            ui,
            &mut self.light_cones,
            icons::FLASHLIGHT,
            "Light Cones",
            "Where every light shines. Off, only the selected light (or the part it's in) shows its cone.",
        );
        overlay_menu_item(
            ui,
            &mut self.move_guide,
            icons::ARROWS_OUT_CARDINAL,
            "Movement Guide",
            MOVE_GUIDE_HINT,
        );
        ui.separator();
        crate::widgets::menu_button(ui, Some(icons::EYE), "View Mode", |ui| {
            for mode in ViewMode::all() {
                let item = egui::Button::selectable(self.view_mode == mode, mode.label());
                if ui.add(item).on_hover_text(mode.hint()).clicked() {
                    self.view_mode = mode;
                    ui.close();
                }
            }
        });
        widgets::section_label(ui, "Overlays");
        self.overlay_items(ui);
        ui.separator();
        if ui
            .button(theme::menu_item(
                ui.ctx(),
                icons::SLIDERS,
                "Viewport Settings…",
            ))
            .on_hover_text("All of these, and the grid's strength, in Settings.")
            .clicked()
        {
            self.settings.open = true;
            self.settings.page = settings::SettingsPage::Viewport;
        }
    }

    /// File > Build Game ▸: a game anyone can run without the editor, for
    /// this OS or another, and whether it shows the splash. `enabled` is
    /// false with no project open, or during Play.
    fn build_game_menu(&mut self, ui: &mut egui::Ui, enabled: bool) {
        crate::widgets::menu_button(ui, Some(icons::FILE_ARROW_UP), "Build Game", |ui| {
            if ui
                .add_enabled(enabled, egui::Button::new(plain(ui, "For this OS")))
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
            ui.separator();
            if let Some(project) = &self.project {
                let mut on = project.developer_console;
                if ui
                    .checkbox(&mut on, "Developer Console (F9)")
                    .on_hover_text(
                        "Built games open the Developer Console with F9: the log, stats and \
                         commands. Play in the editor and --dev have it anyway.",
                    )
                    .changed()
                {
                    self.set_developer_console(on);
                }
            }
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

/// How many steps to undo (negative) or redo (positive), picked in
/// Edit ▸ Undo History.
pub(in crate::app) type HistoryJump = isize;

/// The newest steps that can be undone and redone, at most this many of each.
pub(in crate::app) const HISTORY_ROWS: usize = 25;

/// Edit ▸ Undo History: the steps that can be redone (dimmed, the furthest
/// at the top), then the steps that can be undone, newest first. Clicking
/// a step undoes back to before it, or redoes up to it.
fn undo_history(ui: &mut egui::Ui, history: &crate::history::History) -> Option<HistoryJump> {
    let undo = history.undo_labels();
    let redo = history.redo_labels();
    if undo.is_empty() && redo.is_empty() {
        ui.weak("Nothing to undo yet.");
        return None;
    }
    let mut jump = None;
    let palette = theme::palette(ui.ctx());
    for (index, label) in redo.iter().enumerate().take(HISTORY_ROWS).rev() {
        let text = egui::RichText::new(theme::with_icon(icons::ARROW_CLOCKWISE, label))
            .color(palette.text_weak);
        if ui
            .button(text)
            .on_hover_text("Undone. Click to redo every step up to this one.")
            .clicked()
        {
            jump = Some(index as isize + 1);
        }
    }
    if !redo.is_empty() && !undo.is_empty() {
        ui.separator();
    }
    for (index, label) in undo.iter().enumerate().take(HISTORY_ROWS) {
        if ui
            .button(theme::menu_item(
                ui.ctx(),
                icons::ARROW_COUNTER_CLOCKWISE,
                *label,
            ))
            .on_hover_text("Click to undo this step and every step after it.")
            .clicked()
        {
            jump = Some(-(index as isize + 1));
        }
    }
    if undo.len() > HISTORY_ROWS {
        ui.weak(format!("and {} older", undo.len() - HISTORY_ROWS));
    }
    jump
}

/// A menu item with `icon` in the menu's icon column (`theme::menu_item`).
fn item(ui: &egui::Ui, icon: &str, text: impl Into<String>) -> egui::Atoms<'static> {
    theme::menu_item(ui.ctx(), icon, text)
}

/// A menu item without an icon, its label lined up with those that have one.
fn plain(ui: &egui::Ui, text: impl Into<String>) -> egui::Atoms<'static> {
    theme::menu_entry(ui.ctx(), None, text)
}
