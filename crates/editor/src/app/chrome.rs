//! Menus, the ribbon, dock panels and keyboard shortcuts.
//!
//! Drawing a panel may change the selection. The frame in `app.rs` applies
//! viewport results after the dock pass. These are methods of [`super::EditorApp`].

use super::*;

impl EditorApp {
    /// The Explorer panel, then whatever was clicked or done in it.
    pub(super) fn explorer_pane(&mut self, ui: &mut egui::Ui) {
        let editing = !self.playing();
        // The game's world during Play, otherwise the one being edited.
        let world = shown(&self.world, &self.play);
        let explorer = self.explorer.show(ui, world, &self.selection, editing);
        let rows: Vec<InstanceId> = if explorer.clicked.is_some() {
            self.explorer
                .rows(world)
                .into_iter()
                .map(|row| row.id)
                .collect()
        } else {
            Vec::new()
        };
        if explorer.clicked.is_some() || explorer.keep_group {
            self.select_clicked(
                explorer.clicked,
                explorer.modifiers,
                Some(&rows),
                explorer.keep_group,
            );
            ui.ctx().request_repaint();
        }
        if let Some(action) = explorer.action {
            self.act(action);
            // The Explorer was drawn before this change, so draw it again.
            ui.ctx().request_repaint();
        }
        if let Some(id) = explorer.open {
            self.open_script_instance(id);
        }
    }
    /// The Properties panel (with the Style section for UI elements), then
    /// the edit made in it.
    pub(super) fn properties_pane(&mut self, ui: &mut egui::Ui) {
        let editing = !self.playing();
        let world = shown(&self.world, &self.play);
        let stylesheets = self
            .project
            .as_ref()
            .map_or(&[][..], |project| project.stylesheets.as_slice());
        let selected = self.selected;
        let mut style_edit = None;
        let edit = properties::show(ui, world, selected, !editing, self.selection.len(), |ui| {
            // Stylesheets are edited, not the game, so not during Play.
            if editing {
                style_edit = style_panel::show(ui, world, selected, stylesheets);
            }
        });
        // Other panels may already be drawn this frame, so draw again to
        // show the change everywhere.
        if let (Some(id), Some(edit)) = (selected, edit) {
            self.apply(id, edit);
            ui.ctx().request_repaint();
        }
        if let (Some(id), Some(edit)) = (selected, style_edit) {
            self.apply_style(id, edit);
            ui.ctx().request_repaint();
        }
    }
    /// The Assets panel, then what was asked for in it.
    pub(super) fn assets_pane(&mut self, ui: &mut egui::Ui) {
        let can_save = assets::any_prefab(&self.world, &self.selection_roots());
        let action = self
            .asset_browser
            .show(ui, &self.assets, !self.playing(), can_save);
        match action {
            Some(BrowserAction::Place(entry)) => self.drop_asset(&entry, None),
            Some(BrowserAction::Import { image_folder }) => self.import_assets(image_folder),
            Some(BrowserAction::SavePrefab) => self.save_prefab(),
            Some(BrowserAction::RenamePrefab { path, name }) => self.rename_prefab(&path, &name),
            Some(BrowserAction::DuplicatePrefab { path }) => self.duplicate_prefab(&path),
            Some(BrowserAction::DeletePrefab { path }) => self.delete_prefab(&path),
            None => {}
        }
    }
    /// The Animation tab: clips (see `animation.rs`).
    pub(super) fn animation_pane(&mut self, ui: &mut egui::Ui, render_state: Option<&RenderState>) {
        let folder = self.project.as_ref().map(|project| project.folder.clone());
        let selected = self.selected_model();
        let preview_root = self.property_preview_root();
        let actions = self.animation.show_clips(
            ui,
            folder.as_deref(),
            &self.assets,
            render_state,
            selected.as_ref(),
            &mut self.world,
            preview_root,
            &mut self.output,
        );
        self.studio_actions(actions);
    }

    /// The Animator tab: controllers (see `animation.rs`).
    pub(super) fn animator_pane(&mut self, ui: &mut egui::Ui, render_state: Option<&RenderState>) {
        let folder = self.project.as_ref().map(|project| project.folder.clone());
        let selected = self.selected_model();
        let actions = self.animation.show_controllers(
            ui,
            folder.as_deref(),
            &self.assets,
            render_state,
            selected.as_ref(),
            &mut self.world,
            &mut self.output,
        );
        self.studio_actions(actions);
    }

    /// Does what the Animation or Animator tab asked for.
    fn studio_actions(&mut self, actions: Vec<StudioAction>) {
        for action in actions {
            match action {
                StudioAction::ImportModel => self.import_model_with_dialog(),
                StudioAction::Play => self.play(),
                StudioAction::Show(pane) => self.layout.open(pane),
                StudioAction::SetController {
                    part,
                    animator,
                    path,
                } => self.set_controller(part, animator, path),
                StudioAction::RecordFile {
                    path,
                    before,
                    after,
                    label,
                } => {
                    self.history.record_files(
                        &mut self.world,
                        label,
                        vec![crate::history::TextChange {
                            path,
                            before: Some(before),
                            after: Some(after),
                        }],
                    );
                }
                StudioAction::RecordFiles {
                    label,
                    files,
                    rebind_controller,
                } => self.record_asset_files(label, files, rebind_controller),
            }
        }
    }

    /// A Part, MeshPart or Model selected for property-clip preview.
    fn property_preview_root(&self) -> Option<InstanceId> {
        let id = self.selected?;
        let instance = self.world.get(id)?;
        match instance.class() {
            ClassName::Part | ClassName::MeshPart | ClassName::Model => Some(id),
            ClassName::Animation => instance.parent(),
            _ => None,
        }
    }

    /// The MeshPart selected (or the one around a selected Animator), for
    /// the Animation and Animator tabs, with its Animator if it has one.
    fn selected_model(&self) -> Option<SelectedModel> {
        let world = &self.world;
        let mut id = self.selected?;
        let mut instance = world.get(id)?;
        if instance.class() == ClassName::Animator {
            id = instance.parent()?;
            instance = world.get(id)?;
        }
        if instance.class() != ClassName::MeshPart {
            return None;
        }
        let mesh_id = instance
            .property("MeshId")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        let animated = self
            .assets
            .model(&mesh_id)
            .is_some_and(|model| model.rig.is_some());
        let animator = instance.children().iter().copied().find(|&child| {
            world
                .get(child)
                .is_some_and(|c| c.class() == ClassName::Animator)
        });
        let controller = animator
            .and_then(|a| world.get(a))
            .and_then(|a| a.property("Controller").and_then(Value::as_str))
            .unwrap_or_default()
            .to_owned();
        Some(SelectedModel {
            part: id,
            name: instance.name().to_owned(),
            mesh_id,
            animated,
            animator,
            controller,
        })
    }

    /// Gives `part`'s Animator (making one if `animator` is `None`) the
    /// controller at `path`, as one undo step.
    pub(super) fn set_controller(
        &mut self,
        part: InstanceId,
        animator: Option<InstanceId>,
        path: String,
    ) {
        if self.playing() {
            return;
        }
        match animator {
            Some(animator) => {
                let name = self
                    .world
                    .get(animator)
                    .map_or("Animator".to_owned(), |a| a.name().to_owned());
                self.change(
                    animator,
                    Field::Property("Controller"),
                    format!("Set Controller of {name}"),
                    vec![Edit::Property("Controller", Value::String(path))],
                );
                self.history.seal();
            }
            None => {
                let id = self
                    .world
                    .create(ClassName::Animator)
                    .expect("not a service");
                self.world
                    .set_property(id, "Controller", Value::String(path))
                    .expect("every Animator has a Controller");
                self.place_new(part, vec![id], "Insert Animator".to_owned());
            }
        }
    }
    /// The Scene View panel: the viewport.
    pub(super) fn scene_pane(
        &mut self,
        ui: &mut egui::Ui,
        render_state: Option<&RenderState>,
    ) -> ViewportOutput {
        let ctx = ui.ctx().clone();
        let world = shown(&self.world, &self.play);
        let stylesheets = self
            .project
            .as_ref()
            .map_or(&[][..], |project| project.stylesheets.as_slice());
        // No tool during Play, so nothing can be dragged.
        let tool = (!self.playing()).then_some(self.tool);
        let preview = self.angular_preview();
        let terrain_brush = self.terrain_brush_view(&ctx);
        let emits = std::mem::take(&mut self.pending_emits);
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show_inside(ui, |ui| match render_state {
                Some(render_state) => {
                    self.viewport_rect = Some(ui.available_rect_before_wrap());
                    self.viewport.show(
                        ui,
                        render_state,
                        ViewportScene {
                            world,
                            assets: &self.assets,
                            selected: self.selected,
                            selection: &self.selection,
                            tool,
                            snap: self.snap,
                            modes: self.modes,
                            preview: &preview,
                            emits: &emits,
                            stylesheets,
                            focused_text: self.focused_text,
                            light_cones: self.light_cones,
                            poses: self.play.as_ref().map(Session::poses),
                            terrain_brush,
                        },
                    )
                }
                // Can't happen: main.rs always picks the wgpu renderer.
                None => {
                    ui.label("The viewport needs the wgpu renderer.");
                    Default::default()
                }
            })
            .inner
    }
    /// A script's dock tab (see `scripts.rs`): Code/Blocks for a Script or
    /// LocalScript, then its code or blocks. `id` is its tab's id.
    pub(super) fn script_pane(&mut self, ui: &mut egui::Ui, id: u64) {
        let Some(path) = self.tabs.path_of(id).map(str::to_owned) else {
            return;
        };
        let ctx = ui.ctx().clone();
        let palette = theme::palette(&ctx);
        // Clicking or typing in a tab makes it the script used last.
        if ui.ui_contains_pointer() && ctx.input(|input| input.pointer.any_pressed()) {
            self.tabs.activate(id);
        }
        // Scripts and LocalScripts switch between Code and Blocks here.
        if axcenity_project::switched_script_path(&path).is_some() {
            let blocks = axcenity_project::is_block_script(&path);
            let playing = self.playing();
            let switch = egui::TopBottomPanel::top(egui::Id::new(("script mode", id)))
                .frame(
                    egui::Frame::new()
                        .fill(palette.panel)
                        .inner_margin(egui::Margin::symmetric(10, 5)),
                )
                .show_inside(ui, |ui| scripts::mode_bar(ui, blocks, !playing))
                .inner;
            if switch {
                self.switch_script_mode(&path);
                ctx.request_repaint();
                return;
            }
        }

        if axcenity_project::is_block_script(&path) {
            let mut changed = false;
            let folder = self.project.as_ref().map(|project| project.folder.clone());
            let animation = &mut self.animation;
            let objects = block_objects(&self.world, |controller| match &folder {
                Some(folder) => animation.controller_names(folder, controller),
                None => Vec::new(),
            });
            egui::CentralPanel::default()
                .frame(egui::Frame::NONE)
                .show_inside(ui, |ui| {
                    if let Some(document) = self.project.as_mut().and_then(|project| {
                        project
                            .block_scripts
                            .iter_mut()
                            .find(|document| document.path == path)
                    }) {
                        changed = block_editor::show(
                            ui,
                            &mut document.script,
                            block_editor::BlockContext {
                                key: &path,
                                client: axcenity_project::script_class(&path)
                                    == ClassName::LocalScript,
                                highlighted: document.highlighted,
                                objects: &objects,
                            },
                        );
                        if changed {
                            document.highlighted = None;
                        }
                    }
                });
            if changed {
                self.update_block_lua(&path);
            }
            return;
        }

        let stylesheet = axcenity_project::is_stylesheet(&path);
        let previous_source = stylesheet
            .then(|| self.tabs.get(&path).map(|tab| tab.text.clone()))
            .flatten();
        egui::CentralPanel::default().show_inside(ui, |ui| self.tabs.show(ui, &path));
        if let Some(previous_source) = previous_source {
            let current_source = self
                .tabs
                .get(&path)
                .map(|tab| tab.text.clone())
                .unwrap_or_default();
            if current_source != previous_source {
                self.refresh_stylesheet(&path, &current_source);
                ctx.request_repaint();
            }
        }
    }
    /// The header, which is also the window's title bar, like VS Code's (see
    /// `window_frame.rs`): the logo and menus, the window title in the
    /// middle, then Play's badge, Settings and the window buttons. Its empty
    /// space drags the window.
    pub(super) fn title_bar(&mut self, ui: &mut egui::Ui) {
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
    fn menu_bar(&mut self, ui: &mut egui::Ui) -> egui::Rect {
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
                if ui
                    .button(theme::with_icon(icons::FOLDER_OPEN, "Open Project…"))
                    .clicked()
                {
                    self.request_and_go_ahead(ui.ctx(), Discard::OpenAnother);
                }
                if ui
                    .add_enabled(
                        self.project.is_some() && !self.playing(),
                        egui::Button::new(theme::with_icon(icons::HOUSE, "Close Project")),
                    )
                    .clicked()
                {
                    self.request_and_go_ahead(ui.ctx(), Discard::CloseProject);
                }
                if ui
                    .add_enabled(
                        self.project.is_some() && !self.playing(),
                        egui::Button::new("Open Stylesheet…"),
                    )
                    .clicked()
                {
                    self.open_stylesheet_with_dialog();
                }
                if ui
                    .add_enabled(
                        self.project.is_some() && !self.playing(),
                        egui::Button::new("New Stylesheet"),
                    )
                    .clicked()
                {
                    self.new_stylesheet();
                }
                let save = egui::Button::new(theme::with_icon(icons::FLOPPY_DISK, "Save"))
                    .shortcut_text(ui.ctx().format_shortcut(&SAVE_SHORTCUT));
                if ui.add_enabled(self.project.is_some(), save).clicked() {
                    self.save();
                }
                if ui
                    .add_enabled(
                        self.project.is_some() && !self.playing(),
                        egui::Button::new(theme::with_icon(
                            icons::CLOCK_COUNTER_CLOCKWISE,
                            "Version History…",
                        )),
                    )
                    .clicked()
                {
                    self.versions_open = true;
                }
                let export_enabled = self.project.is_some() && !self.playing();
                ui.menu_button(
                    theme::with_icon(icons::FILE_ARROW_UP, "Export Game"),
                    |ui| {
                        if ui
                            .add_enabled(export_enabled, egui::Button::new("For this OS"))
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
                            if ui
                                .add_enabled(export_enabled, egui::Button::new(label))
                                .clicked()
                            {
                                self.export_game_for_with_dialog(target);
                                ui.close();
                            }
                        }
                        if !export_enabled {
                            ui.weak(if self.playing() {
                                "Press Stop before exporting."
                            } else {
                                "Open a project first."
                            });
                        }
                        ui.separator();
                        let mut show_splash = self
                            .project
                            .as_ref()
                            .is_some_and(|project| project.show_splash);
                        if ui
                            .add_enabled(
                                self.project.is_some(),
                                egui::Checkbox::new(
                                    &mut show_splash,
                                    "Show splash in exported games",
                                ),
                            )
                            .changed()
                        {
                            self.set_show_splash(show_splash);
                        }
                    },
                );
                ui.separator();
                let import =
                    egui::Button::new(theme::with_icon(icons::FILE_ARROW_DOWN, "Import Model…"));
                if ui
                    .add_enabled(self.project.is_some() && !self.playing(), import)
                    .on_hover_text(IMPORT_HOVER)
                    .on_disabled_hover_text(if self.playing() {
                        "Press Stop to edit."
                    } else {
                        "Open a project first."
                    })
                    .clicked()
                {
                    self.import_model_with_dialog();
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
                        "Add the engine's API to this project for Lua Language Server, \
                         so VS Code autocompletes it in scripts.",
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
                    playing_badge(ui);
                }
                ui.available_rect_before_wrap()
            })
            .inner
        })
        .inner
    }
    /// The ribbon under the menu bar: Play and Stop, the tools, the snapping
    /// settings, then buttons that insert each primitive and each kind of script.
    /// The ribbon's tabs, on the app background like the header, over the
    /// ribbon itself, a shade lighter with a hairline above and below, so
    /// the tabs don't run into its buttons.
    pub(super) fn toolbar(&mut self, ui: &mut egui::Ui) {
        let palette = theme::palette(ui.ctx());
        ui.spacing_mut().item_spacing.y = 0.0;
        let active_tab = egui::Frame::new()
            .inner_margin(egui::Margin {
                left: 8,
                right: 8,
                top: 2,
                bottom: 0,
            })
            .show(ui, |ui| self.ribbon_tabs.tab_bar(ui))
            .inner;
        let mut clicks = RibbonClicks::default();
        let groups = self.ribbon_tabs.groups();
        let ribbon = egui::Frame::new()
            .fill(palette.panel)
            .inner_margin(egui::Margin::symmetric(8, 6))
            .show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                egui::ScrollArea::horizontal().show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 6.0;
                        for (index, &group) in groups.iter().enumerate() {
                            if index > 0 {
                                ui.add(egui::Separator::default().vertical().spacing(10.0));
                            }
                            self.ribbon_group(ui, group, &mut clicks);
                        }
                    });
                });
            })
            .response
            .rect;
        let line = egui::Stroke::new(1.0_f32, palette.border);
        let painter = ui.painter();
        painter.hline(ribbon.x_range(), ribbon.top() + 0.5, line);
        painter.hline(ribbon.x_range(), ribbon.bottom() - 0.5, line);
        // The active tab is filled like the ribbon and joins it, over the line.
        if let Some(tab) = active_tab {
            painter.rect_filled(
                egui::Rect::from_min_max(
                    egui::pos2(tab.left() + 1.0, ribbon.top() - 0.5),
                    egui::pos2(tab.right() - 1.0, ribbon.top() + 1.0),
                ),
                0,
                palette.panel,
            );
        }
        if let Some(what) = clicks.insert {
            let near = self.selected;
            self.act(Action::Insert { what, near });
        }
        if let Some(class) = clicks.insert_script {
            self.act(Action::InsertScript(class));
        }
        if clicks.import {
            self.import_model_with_dialog();
        }
        if clicks.play {
            self.play();
        }
        if clicks.stop {
            self.stop();
        }
    }
    /// The ribbon buttons for `tools`. Clicking one picks it.
    fn tool_buttons(&mut self, ui: &mut egui::Ui, tools: &[Tool]) {
        for tool in tools {
            let keys: Vec<String> = tool
                .shortcut()
                .map(|shortcut| ui.ctx().format_shortcut(&shortcut))
                .into_iter()
                .chain(tool.key().map(|key| key.name().to_owned()))
                .collect();
            let hover = match keys.as_slice() {
                [] => format!("{} tool", tool.label()),
                keys => format!("{} tool ({})", tool.label(), keys.join(" or ")),
            };
            let button = RibbonButton::new(theme::tool_icon(*tool), tool.label())
                .selected(self.tool == *tool);
            if ui.add(button).on_hover_text(hover).clicked() {
                self.tool = *tool;
            }
        }
    }

    /// Draws one of the ribbon's groups. Buttons that act on the whole app
    /// are noted in `clicks`, and the toolbar acts on them once it's drawn.
    pub(super) fn ribbon_group(
        &mut self,
        ui: &mut egui::Ui,
        group: RibbonGroup,
        clicks: &mut RibbonClicks,
    ) {
        let playing = self.playing();
        let can_play = self.project.is_some();
        match group {
            RibbonGroup::Game => ribbon::group(ui, "Game", |ui| {
                let palette = theme::palette(ui.ctx());
                let play_button = RibbonButton::new(filled_icons::PLAY, "Play")
                    .family(theme::filled())
                    .color(palette.success);
                clicks.play = ui
                    .add_enabled(!playing && can_play, play_button)
                    .on_hover_text(format!(
                        "Save, then run the game here ({})",
                        ui.ctx().format_shortcut(&PLAY_SHORTCUT)
                    ))
                    .on_disabled_hover_text(if playing {
                        "The game is playing."
                    } else {
                        "Open a project first."
                    })
                    .clicked();
                let stop_button = RibbonButton::new(filled_icons::STOP, "Stop")
                    .family(theme::filled())
                    .color(palette.error);
                clicks.stop = ui
                    .add_enabled(playing, stop_button)
                    .on_hover_text(format!(
                        "Stop the game and go back to editing ({})",
                        ui.ctx().format_shortcut(&STOP_SHORTCUT)
                    ))
                    .clicked();
            }),
            RibbonGroup::Tools => ribbon::group(ui, "Tools", |ui| {
                self.tool_buttons(ui, &Tool::TRANSFORM);
            }),
            RibbonGroup::Build => ribbon::group(ui, "Build", |ui| {
                self.tool_buttons(ui, &Tool::BUILD);
            }),
            RibbonGroup::Snapping => ribbon::group(ui, "Snapping", |ui| {
                let snap = &mut self.snap;
                egui::Grid::new("snapping")
                    .num_columns(2)
                    .spacing([6.0, 4.0])
                    .show(ui, |ui| {
                        ui.checkbox(
                            &mut snap.move_on,
                            theme::with_icon(icons::GRID_FOUR, "Move"),
                        )
                        .on_hover_text("Move and scale in steps of this many studs.");
                        ui.add_enabled(
                            snap.move_on,
                            egui::DragValue::new(&mut snap.move_step)
                                .range(Snap::MIN_MOVE_STEP..=1000.0)
                                .speed(0.05)
                                .max_decimals(3)
                                .suffix(" studs"),
                        );
                        ui.end_row();
                        ui.checkbox(
                            &mut snap.rotate_on,
                            theme::with_icon(icons::ANGLE, "Rotate"),
                        )
                        .on_hover_text("Rotate in steps of this many degrees.");
                        ui.add_enabled(
                            snap.rotate_on,
                            egui::DragValue::new(&mut snap.rotate_step)
                                .range(Snap::MIN_ROTATE_STEP..=180.0)
                                .speed(0.5)
                                .max_decimals(2)
                                .suffix("°"),
                        );
                        ui.end_row();
                    });
            }),
            RibbonGroup::Parts => ribbon::group(ui, "Parts", |ui| {
                for what in Insertable::PARTS {
                    let hover = format!(
                        "Insert a {} part at the camera's focus point, inside the selected \
                         Model or Folder if there is one.",
                        what.label().to_lowercase()
                    );
                    let button = RibbonButton::new(theme::insert_icon(what), what.label());
                    if ui
                        .add_enabled(!playing, button)
                        .on_hover_text(hover)
                        .on_disabled_hover_text("Press Stop to edit.")
                        .clicked()
                    {
                        clicks.insert = Some(what);
                    }
                }
            }),
            RibbonGroup::Effects => ribbon::group(ui, "Effects", |ui| {
                for what in Insertable::AUDIO {
                    let button = RibbonButton::new(theme::insert_icon(what), what.label());
                    if ui
                        .add_enabled(!playing, button)
                        .on_hover_text("Insert a Sound into the selected part or the Workspace.")
                        .on_disabled_hover_text("Press Stop to edit.")
                        .clicked()
                    {
                        clicks.insert = Some(what);
                    }
                }
                for what in Insertable::EFFECTS {
                    let button = RibbonButton::new(theme::insert_icon(what), what.label());
                    if ui
                        .add_enabled(!playing, button)
                        .on_hover_text(
                            "Insert a ParticleEmitter into the selected part (an invisible \
                             holder part is made if none is selected).",
                        )
                        .on_disabled_hover_text("Press Stop to edit.")
                        .clicked()
                    {
                        clicks.insert = Some(what);
                    }
                }
                for what in Insertable::LIGHTS {
                    let button = RibbonButton::new(theme::insert_icon(what), what.label());
                    if ui
                        .add_enabled(!playing, button)
                        .on_hover_text("Insert a light into the selected part.")
                        .on_disabled_hover_text("Press Stop to edit.")
                        .clicked()
                    {
                        clicks.insert = Some(what);
                    }
                }
            }),
            RibbonGroup::Import => ribbon::group(ui, "Import", |ui| {
                let button = RibbonButton::new(icons::FILE_ARROW_DOWN, "Import");
                clicks.import = ui
                    .add_enabled(!playing, button)
                    .on_hover_text(IMPORT_HOVER)
                    .on_disabled_hover_text("Press Stop to edit.")
                    .clicked();
            }),
            RibbonGroup::Ui => ribbon::group(ui, "UI", |ui| {
                for what in Insertable::UI {
                    let label = match what {
                        Insertable::ScreenGui => "Screen",
                        Insertable::Frame => "Frame",
                        Insertable::TextLabel => "Text",
                        Insertable::TextButton => "Button",
                        Insertable::ImageLabel => "Image",
                        Insertable::TextBox => "TextBox",
                        Insertable::PageLayout => "Pages",
                        Insertable::Page => "Page",
                        Insertable::Table => "Table",
                        Insertable::BillboardGui => "Billboard",
                        _ => what.label(),
                    };
                    let hover = match what {
                        Insertable::ScreenGui => {
                            "Insert a ScreenGui: a layer of UI over the game.".to_owned()
                        }
                        Insertable::PageLayout => {
                            "Insert a PageLayout. Its child Frames are pages; a script or the \
                             show-page block sets CurrentPage."
                                .to_owned()
                        }
                        Insertable::Page => {
                            "Insert a page Frame into the selected PageLayout (a PageLayout \
                             and a ScreenGui are made if there isn't one)."
                                .to_owned()
                        }
                        Insertable::BillboardGui => {
                            "Insert a BillboardGui into the selected part. It floats in the \
                             world and is drawn with the screen UI."
                                .to_owned()
                        }
                        _ => format!(
                            "Insert a {} into the selected ScreenGui or Frame (or the first \
                             ScreenGui; one is made if there's none). Drag its handles in the \
                             viewport, and style it with CSS in the Style section of Properties.",
                            what.label()
                        ),
                    };
                    let button = RibbonButton::new(theme::insert_icon(what), label);
                    if ui
                        .add_enabled(!playing, button)
                        .on_hover_text(hover)
                        .on_disabled_hover_text("Press Stop to edit.")
                        .clicked()
                    {
                        clicks.insert = Some(what);
                    }
                }
            }),
            RibbonGroup::Scripts => ribbon::group(ui, "Scripts", |ui| {
                for class in explorer::SCRIPT_CLASSES {
                    let button = RibbonButton::new(theme::class_icon(class), class.as_str());
                    if ui
                        .add_enabled(!playing && can_play, button)
                        .on_hover_text(explorer::script_hover(class))
                        .on_disabled_hover_text(if playing {
                            "Press Stop to edit."
                        } else {
                            "Open a project first."
                        })
                        .clicked()
                    {
                        clicks.insert_script = Some(class);
                    }
                }
            }),
            RibbonGroup::Terrain => ribbon::group(ui, "Terrain", |ui| {
                use crate::terrain_studio::TerrainTool;
                let editor = RibbonButton::new(icons::MOUNTAINS, "Terrain")
                    .selected(self.layout.is_open(Pane::Terrain));
                if ui
                    .add_enabled(can_play, editor)
                    .on_hover_text(
                        "Open the Terrain Studio: make, sculpt, paint and generate terrain.",
                    )
                    .on_disabled_hover_text("Open a project first.")
                    .clicked()
                {
                    self.layout.open(Pane::Terrain);
                }
                for tool in [
                    TerrainTool::Raise,
                    TerrainTool::Paint,
                    TerrainTool::Generate,
                ] {
                    let button = RibbonButton::new(tool.icon(), tool.label())
                        .selected(self.terrain.tool == Some(tool));
                    if ui
                        .add_enabled(!playing && can_play, button)
                        .on_hover_text(tool.hint())
                        .on_disabled_hover_text(if playing {
                            "Press Stop to edit."
                        } else {
                            "Open a project first."
                        })
                        .clicked()
                    {
                        self.terrain.tool = Some(tool);
                        self.layout.open(Pane::Terrain);
                    }
                }
            }),
            RibbonGroup::Panels => ribbon::group(ui, "Panels", |ui| {
                for pane in Pane::CLOSABLE {
                    let button = RibbonButton::new(pane.icon(), pane.label())
                        .selected(self.layout.is_open(pane));
                    if ui
                        .add(button)
                        .on_hover_text(format!(
                            "Show or hide the {} panel. Drag a panel by its tab to move it.",
                            pane.label()
                        ))
                        .clicked()
                    {
                        self.layout.toggle(pane);
                    }
                }
                let reset = RibbonButton::new(icons::ARROW_COUNTER_CLOCKWISE, "Reset");
                if ui
                    .add(reset)
                    .on_hover_text("Put every panel back where it starts.")
                    .clicked()
                {
                    self.layout.reset();
                }
            }),
        }
    }
    /// The keyboard shortcuts for undo, redo, the tools, and renaming,
    /// duplicating and deleting the selection.
    pub(super) fn shortcuts(&mut self, ctx: &egui::Context) {
        // Stop first: egui counts Shift+F5 as F5 too.
        let (stop, play) = ctx.input_mut(|input| {
            let stop = input.consume_shortcut(&STOP_SHORTCUT);
            (stop, input.consume_shortcut(&PLAY_SHORTCUT))
        });
        if stop {
            self.stop();
        } else if play {
            self.play();
        }

        // A text box being typed in has its own undo, and uses Delete and
        // Backspace itself, so leave those keys to it then.
        let typing = ctx.wants_keyboard_input();
        let playing = self.playing();
        let studio_tool = |input: &mut egui::InputState| {
            Tool::ALL.into_iter().find(|tool| {
                tool.shortcut()
                    .is_some_and(|shortcut| input.consume_shortcut(&shortcut))
            })
        };
        // F3X's single keys only while editing, so a game gets every key.
        let f3x_tool = |input: &mut egui::InputState| {
            if playing {
                return None;
            }
            Tool::ALL.into_iter().find(|tool| {
                tool.key()
                    .is_some_and(|key| input.consume_key(egui::Modifiers::NONE, key))
            })
        };
        let keys = ctx.input_mut(|input| {
            if typing {
                return Keys {
                    tool: studio_tool(input),
                    ..Keys::default()
                };
            }
            // Redo first: egui counts Ctrl+Shift+Z as Ctrl+Z too.
            let redo = input.consume_shortcut(&REDO_SHIFT_SHORTCUT)
                || input.consume_shortcut(&REDO_SHORTCUT);
            let none = egui::Modifiers::NONE;
            Keys {
                redo,
                undo: input.consume_shortcut(&UNDO_SHORTCUT),
                tool: studio_tool(input).or_else(|| f3x_tool(input)),
                toggle: !playing && input.consume_key(none, egui::Key::Enter),
                // Taken before the viewport sees them, so Ctrl+D doesn't
                // also fly the camera right.
                duplicate: input.consume_shortcut(&DUPLICATE_SHORTCUT),
                rename: input.consume_key(none, RENAME_KEY),
                delete: input.consume_key(none, egui::Key::Delete)
                    || (cfg!(target_os = "macos") && input.consume_key(none, egui::Key::Backspace)),
            }
        });
        if keys.redo {
            self.redo();
        }
        if keys.undo {
            self.undo();
        }
        if let Some(tool) = keys.tool {
            self.tool = tool;
        }
        if keys.toggle && self.tool == Tool::Angular {
            self.extend_angular();
        } else if keys.toggle && matches!(self.tool, Tool::Anchor | Tool::Collision) {
            let property = if self.tool == Tool::Anchor {
                "Anchored"
            } else {
                "CanCollide"
            };
            let parts = self.tool_parts();
            let all_on = parts.iter().all(|&id| {
                self.world
                    .get(id)
                    .and_then(|i| i.property(property))
                    .and_then(Value::as_bool)
                    != Some(false)
            });
            self.set_on_parts(&parts, property, Value::Bool(!all_on));
            self.history.seal();
        }
        // Undo, duplicate and delete check for Play themselves; renaming
        // only opens a text box, so it's checked here.
        if let Some(id) = self.selected {
            if keys.rename && !self.playing() {
                self.explorer.start_rename(&self.world, id);
            }
            if keys.duplicate {
                self.duplicate_selected();
            }
            if keys.delete {
                self.delete_selected();
            }
        }
    }
    /// File > Version History: the saves kept for this project, and restoring one.
    pub(super) fn version_window(&mut self, ctx: &egui::Context) {
        let Some(folder) = self.project.as_ref().map(|project| project.folder.clone()) else {
            self.versions_open = false;
            return;
        };
        let mut open = self.versions_open;
        let mut restore = None;
        let mut checkpoint = false;
        egui::Window::new("Version History")
            .open(&mut open)
            .resizable(true)
            .default_width(420.0)
            .show(ctx, |ui| {
                ui.weak(
                    "Each save keeps the world, scripts and styles. Restoring puts that \
                     version back. Assets and the project name stay as they are.",
                );
                ui.add_space(6.0);
                if ui.button("Save a version now").clicked() {
                    checkpoint = true;
                }
                ui.add_space(6.0);
                let versions = axcenity_project::list_versions(&folder);
                if versions.is_empty() {
                    ui.weak("No versions yet. Save the project to keep the first one.");
                }
                egui::ScrollArea::vertical()
                    .max_height(320.0)
                    .show(ui, |ui| {
                        for version in &versions {
                            ui.horizontal(|ui| {
                                ui.vertical(|ui| {
                                    ui.label(version.label.as_str());
                                    ui.weak(version.when());
                                });
                                if ui.button("Restore").clicked() {
                                    restore = Some(version.id.clone());
                                }
                            });
                            ui.separator();
                        }
                    });
            });
        self.versions_open = open;
        if checkpoint {
            if self.unsaved() {
                self.save();
            } else {
                match axcenity_project::record_version(&folder, "Checkpoint") {
                    Ok(true) => self
                        .output
                        .info("Kept a version of the world, scripts and styles."),
                    Ok(false) => self
                        .output
                        .info("The newest version already matches these files."),
                    Err(err) => self
                        .output
                        .warning(format!("Could not keep a version: {err}")),
                }
            }
        }
        if let Some(id) = restore {
            self.restore_version(&id);
        }
    }
    /// Puts version `id` back, keeping the current files as a version first.
    pub(super) fn restore_version(&mut self, id: &str) {
        let Some(folder) = self.project.as_ref().map(|project| project.folder.clone()) else {
            return;
        };
        if self.unsaved() && !self.save() {
            return;
        }
        if let Err(err) = axcenity_project::record_version(&folder, "Before restore") {
            self.output.warning(format!(
                "Could not keep the current files before restoring: {err}"
            ));
        }
        match axcenity_project::restore_version(&folder, id) {
            Ok(()) => {
                self.open(&folder);
                self.output.info("Restored that version. The files it replaced were kept as a version named \"Before restore\".");
                self.versions_open = true;
            }
            Err(err) => self
                .output
                .error(format!("Could not restore that version: {err}")),
        }
    }
}

/// The green "Playing" badge in the header during Play, with a dot that
/// pulses gently (steady when motion is reduced).
pub(super) fn playing_badge(ui: &mut egui::Ui) {
    let palette = theme::palette(ui.ctx());
    let green = palette.success;
    let pulse = if ui.ctx().style().animation_time > 0.0 {
        let time = ui.input(|input| input.time);
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(50));
        0.6 + 0.4 * (time * 2.5).sin().abs() as f32
    } else {
        1.0
    };
    egui::Frame::new()
        .fill(green.gamma_multiply(0.14))
        .corner_radius(9)
        .inner_margin(egui::Margin::symmetric(8, 1))
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.x = 5.0;
            ui.label(
                egui::RichText::new(filled_icons::CIRCLE)
                    .family(theme::filled())
                    .size(7.0)
                    .color(green.gamma_multiply(pulse)),
            );
            ui.label(
                egui::RichText::new("Playing")
                    .family(theme::medium_family())
                    .size(theme::type_scale::SMALL)
                    .color(green),
            );
        })
        .response
        .on_hover_text(
            "Only the Properties panel can change the game, until you press \
             Stop. Those changes are undone at Stop.",
        );
}

/// Every object in the Workspace, for the block editor's object menus: its
/// path of names below the Workspace (`Pad.Chime`) and its class.
/// Every object under the Workspace, by path, for the blocks' menus.
/// Animators also get their controller's names (from `names_of`, given the
/// `Controller` path).
fn block_objects(
    world: &World,
    mut names_of: impl FnMut(&str) -> Vec<(block_editor::NameKind, String)>,
) -> Vec<block_editor::ObjectInfo> {
    let workspace = world.workspace();
    world
        .descendants(workspace)
        .into_iter()
        .filter_map(|id| {
            let instance = world.get(id)?;
            let controller_names = match instance.class() {
                ClassName::Animator => instance
                    .property("Controller")
                    .and_then(Value::as_str)
                    .filter(|path| !path.is_empty())
                    .map(&mut names_of)
                    .unwrap_or_default(),
                _ => Vec::new(),
            };
            let mut names = vec![instance.name().to_owned()];
            let mut parent = instance.parent();
            while let Some(above) = parent.filter(|&above| above != workspace) {
                let above = world.get(above)?;
                names.push(above.name().to_owned());
                parent = above.parent();
            }
            names.reverse();
            Some(block_editor::ObjectInfo {
                path: names.join("."),
                class: instance.class(),
                names: controller_names,
            })
        })
        .collect()
}
