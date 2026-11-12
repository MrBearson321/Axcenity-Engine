//! Drawing the Assets panel: the toolbar (chips, Source menu, search and
//! buttons), the tiles or list grouped by source, item menus, and the
//! dialogs they open. The browser's state, scanning and file operations are
//! in `asset_browser.rs`.

use super::*;

impl AssetBrowser {
    /// Draws the panel. `can_edit` is false during Play, and `can_save` is
    /// true when something that can be a prefab is selected. With
    /// `picking`, a click on a prop or model picks it ([`Picking`]).
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        can_edit: bool,
        can_save: bool,
        picking: Option<&Picking>,
    ) -> Option<BrowserAction> {
        let mut action = None;
        self.thumbnails.poll(ui.ctx());
        self.global_thumbnails.poll(ui.ctx());
        let width = ui.available_width();
        if width >= ONE_ROW_FROM {
            // Everything on one row: the chips, then the rest at the right.
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                self.chips(ui);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    if let Some(done) = self.buttons(ui, can_edit, can_save, true) {
                        action = Some(done);
                    }
                    self.refresh_button(ui);
                    widgets::search_field(ui, &mut self.search, "Search assets", 170.0);
                });
            });
        } else {
            // The search on its own row, with the buttons beside it when
            // there's room, and the chips wrapping under it.
            let buttons_beside = width >= BUTTONS_BESIDE_FROM;
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                let search_width = if buttons_beside {
                    200.0
                } else {
                    // The width is the text's: the field adds its margins
                    // (for the magnifying glass and the clear button).
                    ui.available_width()
                        - ui.spacing().interact_size.y
                        - 6.0
                        - widgets::SEARCH_FIELD_MARGINS
                };
                widgets::search_field(ui, &mut self.search, "Search assets", search_width);
                self.refresh_button(ui);
                if buttons_beside {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if let Some(done) = self.buttons(ui, can_edit, can_save, true) {
                            action = Some(done);
                        }
                    });
                }
            });
            ui.add_space(2.0);
            responsive::toolbar(ui, |ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                self.chips(ui);
            });
            if !buttons_beside {
                ui.add_space(2.0);
                responsive::toolbar(ui, |ui| {
                    if let Some(done) = self.buttons(ui, can_edit, can_save, false) {
                        action = Some(done);
                    }
                });
            }
        }
        ui.add_space(4.0);
        widgets::divider(ui);
        ui.add_space(4.0);

        let search = self.search.trim().to_lowercase();
        let entries: Vec<AssetEntry> = self
            .entries()
            .into_iter()
            .filter(|entry| self.category.holds(entry.kind))
            .filter(|entry| self.category != Category::Global || self.is_global(&entry.path))
            .filter(|entry| {
                self.source_filter
                    .as_ref()
                    .is_none_or(|origin| self.origins.of(entry) == *origin)
            })
            .filter(|entry| {
                // A source's name finds its assets too ("poly haven").
                search.is_empty()
                    || search_matches(entry, &search)
                    || self
                        .origins
                        .of(entry)
                        .label()
                        .to_lowercase()
                        .contains(&search)
            })
            .filter(|entry| {
                !self.unused_only
                    || self
                        .unused
                        .as_ref()
                        .is_some_and(|unused| unused.contains(&entry.path))
            })
            .collect();
        if self.unused_only && self.unused.is_none() && action.is_none() {
            // Worked out again after each change; nothing shows meanwhile.
            let candidates: Vec<String> = self
                .files
                .iter()
                .filter(|entry| entry.kind != AssetKind::Prop)
                .filter(|entry| !axcenity_project::is_model_part(&entry.path))
                .map(|entry| entry.path.clone())
                .collect();
            self.unused = Some(HashSet::new());
            action = Some(BrowserAction::FindUnused(candidates));
        }
        let packages = packages::packages(&self.files_and_shared());
        // A folder that's gone (deleted, or another project) closes.
        let open = self.open_package.as_ref().and_then(|name| {
            packages
                .iter()
                .find(|package| package.name.eq_ignore_ascii_case(name))
        });
        if open.is_none() {
            self.open_package = None;
        }
        let in_folder = open.is_some() && search.is_empty();
        let searching = !search.is_empty() || self.unused_only;
        let shown = if self.flat {
            shown(entries, &packages, open, searching)
        } else {
            grouped(entries, &packages, open, searching, &self.origins)
        };
        if let Some(package) = open
            && search.is_empty()
            && self.folder_header(ui, package)
        {
            self.open_package = None;
        }
        egui::ScrollArea::vertical()
            .auto_shrink(false)
            .show(ui, |ui| {
                if shown.is_empty() {
                    let (title, sentence) = if in_folder {
                        ("Empty folder", "Nothing of this kind in this folder.")
                    } else if search.is_empty() && self.source_filter.is_some() {
                        (
                            "Nothing from here",
                            "No asset of this kind came from this source. Pick All sources \
                             in the Source menu to see the rest.",
                        )
                    } else if search.is_empty() && self.category == Category::Global {
                        (
                            "No global assets",
                            "Right-click an asset and tick Global asset to use it in every \
                             project.",
                        )
                    } else if search.is_empty() {
                        (
                            "No assets yet",
                            "Import files, or copy them into the project's assets folder.",
                        )
                    } else {
                        ("No matches", "No asset's path has that in it.")
                    };
                    let icon = if search.is_empty() {
                        icons::FOLDER_DASHED
                    } else {
                        icons::MAGNIFYING_GLASS
                    };
                    widgets::empty_state(ui, icon, title, sentence, None);
                    return;
                }
                // A readable list in a narrow column, tiles where there's room.
                let list = ui.available_width() < LIST_BELOW;
                for block in shown {
                    if let Some(origin) = &block.origin {
                        let open = !self.folded.contains(origin);
                        if self.source_heading(ui, origin, block.items.len(), open) {
                            if open {
                                self.folded.insert(origin.clone());
                            } else {
                                self.folded.remove(origin);
                            }
                        }
                        if !open {
                            continue;
                        }
                    } else if let Some(heading) = &block.heading {
                        ui.add_space(4.0);
                        widgets::section_label(ui, heading);
                        ui.add_space(2.0);
                    }
                    let items = |ui: &mut egui::Ui| {
                        for item in block.items {
                            let entry = match item {
                                Shown::Asset(entry) => entry,
                                Shown::Folder(package) => {
                                    let (opened, menu) =
                                        self.folder_item(ui, &package, can_edit, list);
                                    if opened {
                                        self.open_package = Some(package.name.clone());
                                    }
                                    if let Some(done) = self.follow_menu(menu) {
                                        action = Some(done);
                                    }
                                    continue;
                                }
                            };
                            let (place, pick, menu) =
                                self.item(ui, &entry, can_edit, list, picking);
                            if pick {
                                action = Some(BrowserAction::Pick(entry.clone()));
                            } else if place {
                                action = Some(BrowserAction::Place(entry.clone()));
                            }
                            if let Some(done) = self.follow_menu(menu) {
                                action = Some(done);
                            }
                        }
                    };
                    if list {
                        ui.vertical(|ui| {
                            ui.spacing_mut().item_spacing.y = 1.0;
                            items(ui);
                        });
                    } else {
                        ui.horizontal_wrapped(|ui| {
                            ui.spacing_mut().item_spacing = egui::vec2(6.0, 6.0);
                            items(ui);
                        });
                    }
                }
            });
        if let Some(done) = self.asset_dialog(ui.ctx()) {
            action = Some(done);
        }
        action
    }

    /// Opens Rename or Move… for the project file at `path`, such as the
    /// code of a script inside an object, which has no tile here.
    pub fn start_move(&mut self, path: String) {
        self.dialog = Some(AssetDialog::Move {
            to: path.clone(),
            from: path,
        });
    }

    /// Does what a tile's right-click menu asked for, or opens its dialog.
    pub(super) fn follow_menu(&mut self, menu: Option<ItemMenu>) -> Option<BrowserAction> {
        match menu? {
            ItemMenu::Global { path, global } => {
                let (error, text) = match self.set_global(&path, global) {
                    Ok(text) => (false, text),
                    Err(problem) => (true, format!("Global assets: {problem}")),
                };
                return Some(BrowserAction::Notice { error, text });
            }
            ItemMenu::Rename { path, name } => {
                self.dialog = Some(AssetDialog::Rename {
                    path,
                    name,
                    error: None,
                });
            }
            ItemMenu::NewVariant { path } => {
                return Some(BrowserAction::NewVariant { path });
            }
            ItemMenu::Duplicate { path } => {
                return Some(BrowserAction::DuplicatePrefab { path });
            }
            ItemMenu::Delete { name, paths } => {
                let files = self.files_to_delete(&paths);
                if files.is_empty() {
                    return Some(BrowserAction::Notice {
                        error: true,
                        text: format!("Could not delete {name}: its files aren't there."),
                    });
                }
                self.dialog = Some(AssetDialog::Delete {
                    name,
                    global: paths.iter().any(|path| self.is_global(path)),
                    files,
                });
            }
            ItemMenu::ExportPackage { name, paths } => {
                return Some(BrowserAction::ExportPackage { name, paths });
            }
            ItemMenu::EditMaterial { path } => return Some(BrowserAction::EditMaterial(path)),
            ItemMenu::EditModel { path } => return Some(BrowserAction::EditModel(path)),
            ItemMenu::FindReferences { path } => return Some(BrowserAction::FindReferences(path)),
            ItemMenu::Move { path } => {
                self.dialog = Some(AssetDialog::Move {
                    to: path.clone(),
                    from: path,
                });
            }
        }
        None
    }

    /// The project files deleting the assets at `paths` removes: each one's
    /// own files (see `global_assets::own_files`: a model's folder and its
    /// clips, a texture set's folder, or the one file), sorted.
    pub fn files_to_delete(&self, paths: &[String]) -> Vec<String> {
        let Some(folder) = &self.folder else {
            return Vec::new();
        };
        let files: BTreeSet<String> = paths
            .iter()
            .flat_map(|path| global_assets::own_files(folder, path))
            .collect();
        files.into_iter().collect()
    }

    /// A click on the asset at `path`: picks only it, or with `adding`
    /// (Ctrl or Cmd held) picks or unpicks it along with the others.
    pub(super) fn pick_click(&mut self, path: &str, adding: bool) {
        match self.picked.iter().position(|picked| picked == path) {
            Some(at) if adding => {
                self.picked.remove(at);
            }
            None if adding => self.picked.push(path.to_owned()),
            _ => self.picked = vec![path.to_owned()],
        }
    }

    /// The "Global asset" checkbox of the asset at `path`'s right-click
    /// menu. Off while there's no folder for global assets.
    pub(crate) fn global_checkbox(&self, ui: &mut egui::Ui, path: &str) -> Option<ItemMenu> {
        let mut global = self.is_global(path);
        let response = ui
            .add_enabled(
                self.library.is_some() && self.folder.is_some(),
                egui::Checkbox::new(&mut global, theme::with_icon(icons::GLOBE, "Global asset")),
            )
            .on_hover_text(if global {
                "Untick to take it out of global assets. Projects that used it keep their copy."
            } else {
                "Tick to use it in every project. It's copied into a project the first time \
                 it's used there."
            })
            .on_disabled_hover_text("There's no folder for global assets on this computer.");
        if response.changed() {
            ui.close();
            return Some(ItemMenu::Global {
                path: path.to_owned(),
                global,
            });
        }
        None
    }

    /// A source's section heading: its fold caret, icon, name and how many
    /// things it holds. True when clicked (to fold or unfold it).
    pub(super) fn source_heading(
        &self,
        ui: &mut egui::Ui,
        origin: &Origin,
        count: usize,
        open: bool,
    ) -> bool {
        let palette = theme::palette(ui.ctx());
        ui.add_space(4.0);
        let caret = if open {
            icons::CARET_DOWN
        } else {
            icons::CARET_RIGHT
        };
        let text = egui::RichText::new(format!(
            "{caret} {} {}  {count}",
            origin.icon(),
            origin.label()
        ))
        .size(theme::type_scale::SMALL)
        .color(palette.text_secondary);
        let clicked = ui
            .add(egui::Label::new(text).sense(egui::Sense::click()))
            .on_hover_text(format!(
                "{}\nClick to {} this section.",
                origin.hover(),
                if open { "fold" } else { "unfold" }
            ))
            .clicked();
        ui.add_space(2.0);
        clicked
    }

    /// The Source menu: every source, or one; and whether the grid is in a
    /// section per source.
    pub(super) fn source_menu(&mut self, ui: &mut egui::Ui) {
        let all = self.entries();
        let present = self.origins.present(&all);
        let selected = self
            .source_filter
            .as_ref()
            .map_or("All sources".to_owned(), Origin::label);
        let mut grouped = !self.flat;
        egui::ComboBox::from_id_salt("assets source")
            .selected_text(theme::with_icon(icons::FUNNEL, &selected))
            .show_ui(ui, |ui| {
                if ui
                    .selectable_label(self.source_filter.is_none(), "All sources")
                    .clicked()
                {
                    self.source_filter = None;
                }
                for origin in present {
                    let on = self.source_filter.as_ref() == Some(&origin);
                    if ui
                        .selectable_label(on, theme::with_icon(origin.icon(), &origin.label()))
                        .on_hover_text(origin.hover())
                        .clicked()
                    {
                        self.source_filter = Some(origin);
                    }
                }
                ui.separator();
                ui.checkbox(&mut grouped, "Group by source")
                    .on_hover_text("Show the assets in a section for each place they came from.");
            })
            .response
            .on_hover_text("Show the assets from one place: built-in, made here, imported, a Toolbox source or global.");
        self.flat = !grouped;
    }

    /// The category chips, which pick what's shown.
    pub(super) fn chips(&mut self, ui: &mut egui::Ui) {
        for category in Category::ALL {
            if widgets::chip(ui, category.label(), self.category == category).clicked() {
                self.category = category;
            }
        }
        if widgets::chip(ui, "Unused", self.unused_only)
            .on_hover_text(
                "Only the assets nothing uses: no Instance, script (by its path), material, \
                 prefab, level or data table names them.",
            )
            .clicked()
        {
            self.unused_only = !self.unused_only;
            self.unused = None;
        }
        responsive::room_for(ui, 140.0);
        self.source_menu(ui);
    }

    pub(super) fn refresh_button(&mut self, ui: &mut egui::Ui) {
        if widgets::icon_button(ui, icons::ARROW_CLOCKWISE, "Look for new files now", false)
            .clicked()
        {
            self.refresh();
        }
    }

    /// Import and Save prefab, added last-first when `right_to_left`, so
    /// they read the same way in either layout.
    pub(super) fn buttons(
        &mut self,
        ui: &mut egui::Ui,
        can_edit: bool,
        can_save: bool,
        right_to_left: bool,
    ) -> Option<BrowserAction> {
        let palette = theme::palette(ui.ctx());
        let mut action = None;
        let mut toolbox = false;
        let mut get_texture = |ui: &mut egui::Ui| {
            if ui
                .add_enabled(
                    self.folder.is_some(),
                    widgets::button(
                        widgets::Role::Secondary,
                        Some(icons::TOOLBOX),
                        "Toolbox",
                        &palette,
                    )
                    .min_size(egui::vec2(0.0, 24.0)),
                )
                .on_hover_text(
                    "Find textures, skies, models and sounds on ambientCG, Poly Haven and \
                     Openverse, and get them into the project.",
                )
                .clicked()
            {
                toolbox = true;
            }
        };
        let mut import = |ui: &mut egui::Ui| {
            if ui
                .add_enabled(
                    can_edit && self.folder.is_some(),
                    widgets::button(
                        widgets::Role::Secondary,
                        Some(icons::PLUS),
                        "Import",
                        &palette,
                    )
                    .min_size(egui::vec2(0.0, 24.0)),
                )
                .on_hover_text(
                    "Copy models, images, sounds or prefabs into the project's assets folder.",
                )
                .clicked()
            {
                action = Some(BrowserAction::Import {
                    image_folder: self.category.image_folder(),
                });
            }
        };
        let save = |ui: &mut egui::Ui| {
            ui.add_enabled(
                can_edit && can_save,
                widgets::button(
                    widgets::Role::Secondary,
                    Some(icons::FLOPPY_DISK),
                    "Save prefab",
                    &palette,
                )
                .min_size(egui::vec2(0.0, 24.0)),
            )
            .on_hover_text(
                "Save the selected parts, Models or UI as a prefab in assets/prefabs, to place \
                 copies of from here.",
            )
            .on_disabled_hover_text("Select parts, Models or UI to save as a prefab.")
            .clicked()
        };
        let saved = if right_to_left {
            let saved = save(ui);
            import(ui);
            get_texture(ui);
            saved
        } else {
            get_texture(ui);
            import(ui);
            save(ui)
        };
        if saved {
            action = Some(BrowserAction::SavePrefab);
        }
        if toolbox {
            action = Some(BrowserAction::OpenToolbox);
        }
        action
    }

    /// The rename question for a prefab tile, and the delete question for
    /// any project asset.
    pub(super) fn asset_dialog(&mut self, ctx: &egui::Context) -> Option<BrowserAction> {
        let folder = self.folder.clone();
        let dialog = self.dialog.as_mut()?;
        let mut action = None;
        let mut close = false;
        let ((), dismissed) = dialog::modal(ctx, "asset dialog", 380.0, |ui| {
            let palette = theme::palette(ui.ctx());
            match dialog {
                AssetDialog::Rename { path, name, error } => {
                    dialog::header(
                        ui,
                        icons::PENCIL_SIMPLE,
                        Tone::Question,
                        "Rename prefab",
                        None,
                    );
                    ui.add(dialog::text_field(name));
                    if let Some(problem) = error {
                        ui.colored_label(palette.error, problem.as_str());
                    }
                    let picked = dialog::buttons(
                        ui,
                        &[
                            Button::cancel(),
                            Button::new(icons::CHECK, "Rename", Kind::Main),
                        ],
                        Enter::FromField,
                    );
                    match picked {
                        Some(1) => match check_prefab_rename(folder.as_deref(), path, name) {
                            Ok(name) => {
                                action = Some(BrowserAction::RenamePrefab {
                                    path: path.clone(),
                                    name,
                                });
                                close = true;
                            }
                            Err(problem) => *error = Some(problem),
                        },
                        Some(_) => close = true,
                        None => {}
                    }
                }
                AssetDialog::Move { from, to } => {
                    let objects = format!("{}/", axcenity_project::OBJECTS_FOLDER);
                    let message = if from.starts_with(&objects) {
                        format!(
                            "A new name, or a path under {objects} to move `{from}`. Every \
                             script inside an object that runs it is changed to match, as one \
                             undo step."
                        )
                    } else {
                        format!(
                            "A new name, or a path under assets/ to move `{from}`. Every \
                             Instance, script, material, prefab, level and data table that \
                             uses it is changed to match, as one undo step."
                        )
                    };
                    dialog::header(
                        ui,
                        icons::PENCIL_SIMPLE,
                        Tone::Question,
                        "Rename or move",
                        Some(&message),
                    );
                    ui.add(dialog::text_field(to));
                    let target = to.trim();
                    // A bare name stays in its folder.
                    let target = if target.contains('/') {
                        target.to_owned()
                    } else {
                        match from.rsplit_once('/') {
                            Some((folder, _)) => format!("{folder}/{target}"),
                            None => target.to_owned(),
                        }
                    };
                    let changed = !target.is_empty() && target != *from;
                    let picked = dialog::buttons(
                        ui,
                        &[
                            Button::cancel(),
                            Button::new(icons::CHECK, "Rename", Kind::Main).enabled(changed),
                        ],
                        Enter::FromField,
                    );
                    match picked {
                        Some(1) => {
                            action = Some(BrowserAction::RenameAsset {
                                from: from.clone(),
                                to: target,
                            });
                            close = true;
                        }
                        Some(_) => close = true,
                        None => {}
                    }
                }
                AssetDialog::Delete {
                    name,
                    files,
                    global,
                } => {
                    let mut message = match files.as_slice() {
                        [only] => format!("This deletes `{only}` from the project."),
                        files => format!("This deletes {} files from the project.", files.len()),
                    };
                    message.push_str(
                        " Anything that uses it shows it as missing (File > Check Assets \
                         lists those). Undo puts the files back.",
                    );
                    if *global {
                        message.push_str(
                            " The global copy stays. Untick Global asset to take it out of \
                             every project.",
                        );
                    }
                    dialog::header(
                        ui,
                        icons::TRASH,
                        Tone::Danger,
                        &format!("Delete {name}?"),
                        Some(&message),
                    );
                    if files.len() > 1 {
                        egui::ScrollArea::vertical()
                            .max_height(120.0)
                            .show(ui, |ui| {
                                for file in files.iter() {
                                    ui.label(
                                        egui::RichText::new(file)
                                            .size(theme::type_scale::SMALL)
                                            .color(palette.text_weak),
                                    );
                                }
                            });
                    }
                    let picked = dialog::buttons(
                        ui,
                        &[
                            Button::cancel(),
                            Button::new(icons::TRASH, "Delete", Kind::Danger),
                        ],
                        Enter::Main,
                    );
                    match picked {
                        Some(1) => {
                            action = Some(BrowserAction::DeleteAssets {
                                name: name.clone(),
                                files: files.clone(),
                            });
                            close = true;
                        }
                        Some(_) => close = true,
                        None => {}
                    }
                }
            }
        });
        close |= dismissed;
        if close {
            self.dialog = None;
        }
        if matches!(action, Some(BrowserAction::DeleteAssets { .. })) {
            self.picked.clear();
        }
        action
    }

    /// Draws one asset: a tile, or a row of the list when `list`. The first
    /// bool is a double-click (place); the second is a pick ([`Picking`]).
    pub(super) fn item(
        &mut self,
        ui: &mut egui::Ui,
        entry: &AssetEntry,
        can_edit: bool,
        list: bool,
        picking: Option<&Picking>,
    ) -> (bool, bool, Option<ItemMenu>) {
        let size = if list {
            egui::vec2(ui.available_width(), LIST_ROW)
        } else {
            TILE
        };
        let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click_and_drag());
        let id = response.id;
        let takes = picking.filter(|picking| picking.takes(entry));
        let selected = picking.is_some_and(|picking| picking.has(entry));
        let global = self.is_global(&entry.path);
        let mut hover = if let Some(picking) = takes {
            format!("{}\n{}", entry.path, picking.hover())
        } else {
            match entry.prop() {
                Some(prop) => format!("{}\n{}", prop.hover, entry.kind.hover()),
                None => format!("{}\n{}", entry.path, entry.kind.hover()),
            }
        };
        if let Some(line) = self.source_line(&entry.path) {
            hover.push('\n');
            hover.push_str(&line);
        }
        if let Some(parent) = self.origins.parent(&entry.path) {
            hover.push_str(&format!(
                "\nMade from the texture set {}.",
                display_name(parent)
            ));
        }
        if global && !self.is_local(&entry.path) {
            hover.push_str("\nGlobal asset: copied into this project the first time it's used.");
        } else if global {
            hover.push_str("\nGlobal asset: in every project.");
        }
        // Only tiles on screen ask for their thumbnail, so a long list
        // loads what's visible first.
        let thumbnail = (ui.is_rect_visible(rect)
            && matches!(
                entry.kind,
                AssetKind::Image
                    | AssetKind::Texture
                    | AssetKind::TextureSet
                    | AssetKind::Particle
                    | AssetKind::Sky
            ))
        .then(|| self.thumbnail_of(ui.ctx(), entry))
        .flatten();
        // A picture's tooltip shows it larger, to see the whole texture.
        let response = match &thumbnail {
            Some(texture) => response.on_hover_ui(|ui| {
                ui.add(egui::Image::new(texture).max_size(egui::Vec2::splat(HOVER_PICTURE)));
                ui.label(hover);
            }),
            None => response.on_hover_text(hover),
        };
        if can_edit && response.drag_started() {
            egui::DragAndDrop::set_payload(ui.ctx(), AssetPayload(entry.clone()));
        }
        let palette = theme::palette(ui.ctx());
        let hover = theme::ease(
            ui.ctx(),
            id.with("hover"),
            response.hovered() || response.dragged(),
        );
        let painter = ui.painter();
        painter.rect_filled(rect, 6.0, palette.control_hover.gamma_multiply(hover));
        let picked = self.picked.contains(&entry.path);
        if picked {
            painter.rect_filled(rect, 6.0, palette.control_active);
        }
        if response.has_focus() || selected || picked {
            painter.rect_stroke(
                rect,
                6.0,
                egui::Stroke::new(1.0_f32, palette.accent),
                egui::StrokeKind::Inside,
            );
        }
        let picture = if list {
            egui::Rect::from_center_size(
                egui::pos2(rect.min.x + 6.0 + LIST_PICTURE / 2.0, rect.center().y),
                egui::vec2(LIST_PICTURE, LIST_PICTURE),
            )
        } else {
            egui::Rect::from_center_size(
                egui::pos2(rect.center().x, rect.min.y + 6.0 + PICTURE / 2.0),
                egui::vec2(PICTURE, PICTURE),
            )
        };
        self.paint_picture(ui, picture, entry, thumbnail.as_ref());
        if global {
            global_badge(ui, picture);
        }
        let text_color = if response.hovered() {
            palette.text_strong
        } else {
            palette.text
        };
        if list {
            // The kind at the right when there's room, then the name in
            // what's left, ending in "…" if it's too long.
            let mut name_right = rect.max.x - 6.0;
            if rect.width() >= 200.0 {
                let kind = ui.painter().text(
                    rect.right_center() - egui::vec2(8.0, 0.0),
                    egui::Align2::RIGHT_CENTER,
                    entry.kind.label(),
                    egui::FontId::proportional(theme::type_scale::SMALL),
                    palette.text_weak,
                );
                name_right = kind.min.x - 8.0;
            }
            let left = picture.max.x + 8.0;
            let galley = widgets::elided(
                ui,
                entry.name.clone(),
                egui::FontId::proportional(theme::type_scale::BODY),
                text_color,
                (name_right - left).max(1.0),
                1,
            );
            ui.painter().galley(
                egui::pos2(left, rect.center().y - galley.size().y / 2.0),
                galley,
                text_color,
            );
        } else {
            // Long names show two lines, then "…".
            let galley = widgets::elided(
                ui,
                entry.name.clone(),
                egui::FontId::proportional(theme::type_scale::SMALL),
                text_color,
                TILE.x - 8.0,
                2,
            );
            ui.painter().galley(
                egui::pos2(rect.center().x - galley.size().x / 2.0, picture.max.y + 5.0),
                galley,
                text_color,
            );
        }

        // While dragged, a copy of the picture follows the mouse.
        if response.dragged()
            && let Some(pointer) = ui.ctx().pointer_latest_pos()
        {
            let layer = egui::LayerId::new(egui::Order::Tooltip, id.with("dragged"));
            let ghost = egui::Rect::from_center_size(
                pointer + egui::vec2(18.0, 18.0),
                egui::vec2(40.0, 40.0),
            );
            let painter = ui.ctx().layer_painter(layer);
            painter.rect_filled(ghost.expand(3.0), 6.0, ui.visuals().window_fill);
            match &thumbnail {
                Some(texture) => {
                    painter.image(texture.id(), ghost, uv(), egui::Color32::WHITE);
                }
                None => {
                    painter.text(
                        ghost.center(),
                        egui::Align2::CENTER_CENTER,
                        icon_of(entry),
                        egui::FontId::proportional(28.0),
                        entry.kind.color(ui.visuals()),
                    );
                }
            }
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
        }
        let mut menu = None;
        // Built-in props are in every project already.
        if entry.kind != AssetKind::Prop {
            let path = entry.path.clone();
            let name = entry.name.clone();
            let local = self.is_local(&path);
            let prefab = entry.kind == AssetKind::Prefab;
            let material = entry.kind == AssetKind::Material;
            let model = entry.kind == AssetKind::Model;
            // The picked assets if this is one of them, or else just this.
            let export = if picked {
                self.picked.clone()
            } else {
                vec![path.clone()]
            };
            // Only the project's own files: built-in props and global
            // assets it hasn't got have none to delete.
            let deletable: Vec<String> = export
                .iter()
                .filter(|path| self.is_local(path))
                .cloned()
                .collect();
            response.context_menu(|ui| {
                if local
                    && ui
                        .button(format!("{} Find References", icons::MAGNIFYING_GLASS))
                        .on_hover_text(
                            "List every Instance, script, material, prefab, level and data \
                             table that uses it.",
                        )
                        .clicked()
                {
                    menu = Some(ItemMenu::FindReferences { path: path.clone() });
                    ui.close();
                }
                if let Some(picked) = self.global_checkbox(ui, &path) {
                    menu = Some(picked);
                }
                if local && let Some(chosen) = export_package_button(ui, &name, &export) {
                    menu = Some(chosen);
                }
                if !can_edit || !local {
                    return;
                }
                ui.separator();
                if material
                    && ui
                        .button(format!("{} Edit material", icons::PENCIL_SIMPLE))
                        .on_hover_text("Open it in the Material tab.")
                        .clicked()
                {
                    menu = Some(ItemMenu::EditMaterial { path: path.clone() });
                    ui.close();
                }
                if model
                    && ui
                        .button(format!("{} Edit in Model Studio", icons::POLYGON))
                        .on_hover_text(
                            "Edit its meshes in the Model Studio tab. Saving writes a .glb.",
                        )
                        .clicked()
                {
                    menu = Some(ItemMenu::EditModel { path: path.clone() });
                    ui.close();
                }
                if !prefab
                    && ui
                        .button(format!("{} Rename or Move…", icons::PENCIL_SIMPLE))
                        .on_hover_text(
                            "Give it another name or folder. Everything that uses it is \
                             changed to match, as one undo step.",
                        )
                        .clicked()
                {
                    menu = Some(ItemMenu::Move { path: path.clone() });
                    ui.close();
                }
                if prefab {
                    if ui
                        .button(format!("{} Rename", icons::PENCIL_SIMPLE))
                        .clicked()
                    {
                        menu = Some(ItemMenu::Rename {
                            path: path.clone(),
                            name: name.clone(),
                        });
                        ui.close();
                    }
                    if ui.button(format!("{} Duplicate", icons::COPY)).clicked() {
                        menu = Some(ItemMenu::Duplicate { path: path.clone() });
                        ui.close();
                    }
                    if ui
                        .button(format!("{} New variant", icons::GIT_BRANCH))
                        .on_hover_text(
                            "Make a prefab built on this one, such as a Red Enemy from an \
                             Enemy. Its changes are its own; the rest follows this prefab.",
                        )
                        .clicked()
                    {
                        menu = Some(ItemMenu::NewVariant { path: path.clone() });
                        ui.close();
                    }
                }
                if let Some(chosen) = delete_button(ui, &name, &deletable) {
                    menu = Some(chosen);
                }
            });
        }
        let pick = can_edit && takes.is_some() && response.clicked();
        let place = can_edit && !pick && response.double_clicked();
        if response.clicked() && !pick {
            let modifiers = ui.input(|input| input.modifiers);
            self.pick_click(&entry.path, modifiers.command || modifiers.ctrl);
        }
        (place, pick, menu)
    }

    pub(super) fn paint_picture(
        &self,
        ui: &egui::Ui,
        picture: egui::Rect,
        entry: &AssetEntry,
        thumbnail: Option<&egui::TextureHandle>,
    ) {
        let painter = ui.painter();
        let palette = theme::palette(ui.ctx());
        painter.rect(
            picture,
            5.0,
            palette.inset,
            egui::Stroke::new(1.0_f32, palette.border),
            egui::StrokeKind::Inside,
        );
        let side = picture.width();
        match thumbnail {
            Some(texture) => {
                // Fitted inside the square, keeping its shape.
                let [w, h] = texture.size().map(|n| n as f32);
                let scale = (side / w).min(side / h);
                let fitted =
                    egui::Rect::from_center_size(picture.center(), egui::vec2(w, h) * scale);
                painter.image(texture.id(), fitted, uv(), egui::Color32::WHITE);
            }
            None => {
                painter.text(
                    picture.center(),
                    egui::Align2::CENTER_CENTER,
                    icon_of(entry),
                    egui::FontId::proportional(side * 0.47),
                    entry.kind.color(ui.visuals()),
                );
            }
        }
    }
}
