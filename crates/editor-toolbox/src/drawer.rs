//! A card's details drawer: who made the asset, its licence in plain words
//! (what it allows and asks), its size and maps, each package and its
//! download size, whether it's in the library or this project, and Get,
//! Favourite, Add to collection, Copy credit line and Make material.

use axcenity_toolbox::provenance::{self, Snapshot};

use super::*;

/// The open drawer.
pub(super) struct Drawer {
    pub(super) key: String,
    /// The full record once looked up (Poly Haven lists packages only there).
    record: AssetRecord,
    lookup: Option<Job<AssetRecord>>,
    favourite: bool,
    /// Packages already in the asset directory: label and bytes.
    downloaded: Vec<(String, u64)>,
    /// Files in this project that came from it.
    in_project: Vec<String>,
    new_collection: String,
    /// Said under the buttons after one is used ("Copied.").
    said: Option<String>,
}

/// Bytes as people read them: "640 KB", "12.5 MB", "1.2 GB".
pub fn size_text(bytes: u64) -> String {
    let bytes = bytes as f64;
    if bytes >= 1e9 {
        format!("{:.1} GB", bytes / 1e9)
    } else if bytes >= 1e6 {
        format!("{:.1} MB", bytes / 1e6)
    } else {
        format!("{:.0} KB", (bytes / 1e3).max(1.0))
    }
}

/// A physical size in metres: "2 × 2 m", "0.4 × 0.3 × 0.2 m".
pub(super) fn physical_text(size: &[f64]) -> Option<String> {
    if size.is_empty() || size.iter().any(|side| !side.is_finite() || *side <= 0.0) {
        return None;
    }
    let sides: Vec<String> = size
        .iter()
        .map(|side| {
            let text = format!("{side:.2}");
            text.trim_end_matches('0').trim_end_matches('.').to_owned()
        })
        .collect();
    Some(format!("{} m", sides.join(" × ")))
}

/// The texture set a texture asset installs as, in a project.
pub(super) fn texture_set_path(id: &str) -> String {
    format!("{PROJECT_TEXTURES_FOLDER}/{id}/{id}{TEXTURE_SET_SUFFIX}")
}

impl ToolboxPanel {
    /// Opens the drawer for the card with `key`, and marks it recently seen.
    pub(super) fn open_drawer(&mut self, ctx: &egui::Context, project: Option<&Path>, key: &str) {
        let Some(listing) = self.listing(key) else {
            return;
        };
        let record = listing.record.clone();
        let lookup = (listing.lookup)
            .then(|| axcenity_toolbox::source(&record.provider))
            .flatten()
            .map(|source| {
                let (tx, rx) = mpsc::channel();
                let id = record.id.clone();
                let ctx = ctx.clone();
                std::thread::spawn(move || {
                    let client = http::Client::new(Duration::from_secs(30));
                    let _ = tx.send(source.record(&client, &id).map_err(|err| err.to_string()));
                    ctx.request_repaint();
                });
                Job { rx }
            });
        self.drawer = Some(Drawer {
            key: key.to_owned(),
            record,
            lookup,
            favourite: false,
            downloaded: Vec::new(),
            in_project: Vec::new(),
            new_collection: String::new(),
            said: None,
        });
        self.shown_for = project.map(Path::to_owned);
        if let Ok((_, db)) = library()
            && let Some(drawer) = &self.drawer
        {
            // A search result is kept so it can be starred or collected.
            let _ = db.put_asset(&drawer.record);
            let _ = db.touch_recent(key);
        }
        self.refresh_drawer();
    }

    /// Reads the drawer's library and project state again.
    pub(super) fn refresh_drawer(&mut self) {
        let project = self.shown_for.clone();
        let Some(drawer) = &mut self.drawer else {
            return;
        };
        if let Ok((_, db)) = library() {
            drawer.favourite = db.is_favourite(&drawer.key).unwrap_or(false);
            drawer.downloaded = db
                .downloads_of(&drawer.key)
                .unwrap_or_default()
                .into_iter()
                .map(|download| (download.variant, download.bytes))
                .collect();
            drawer.in_project = match &project {
                Some(project) => {
                    let mut paths: Vec<String> = db
                        .installs(&provenance::project_key(project))
                        .unwrap_or_default()
                        .into_iter()
                        .filter(|install| install.key == drawer.key)
                        .map(|install| install.path)
                        .collect();
                    if let Ok(snapshot) = Snapshot::read(project) {
                        paths.extend(
                            snapshot
                                .files
                                .into_iter()
                                .filter(|file| file.key == drawer.key)
                                .map(|file| file.path),
                        );
                    }
                    paths.sort();
                    paths.dedup();
                    paths
                }
                None => Vec::new(),
            };
        }
    }

    /// A finished look-up fills the drawer and the card's packages.
    pub(super) fn poll_drawer(&mut self) {
        let Some(drawer) = &mut self.drawer else {
            return;
        };
        let Some(job) = drawer.lookup.take() else {
            return;
        };
        match job.rx.try_recv() {
            Ok(Ok(full)) => {
                if let Ok((_, db)) = library() {
                    let _ = db.put_asset(&full);
                }
                drawer.record = full.clone();
                let key = drawer.key.clone();
                if let Some(page) = &mut self.page
                    && let Some(listing) = page.listings.iter_mut().find(|l| l.key() == key)
                {
                    *listing = Listing::from_record(full);
                }
            }
            Ok(Err(err)) => drawer.said = Some(format!("Could not look it up: {err}")),
            Err(TryRecvError::Empty) => drawer.lookup = Some(job),
            Err(TryRecvError::Disconnected) => {}
        }
    }

    /// Draws the drawer.
    pub(super) fn drawer_pane(
        &mut self,
        ui: &mut egui::Ui,
        project: Option<&Path>,
        can_edit: bool,
        actions: &mut Vec<ToolboxAction>,
    ) {
        let palette = theme::palette(ui.ctx());
        let Some(drawer) = &self.drawer else {
            return;
        };
        let key = drawer.key.clone();
        let record = drawer.record.clone();
        let preview = self.previews.get(&key).cloned().flatten();
        let plan = self
            .listing(&key)
            .and_then(|listing| self.package(listing).map(|choice| choice.describe()));
        let can_get = can_edit && project.is_some() && self.download.is_none();
        let source_name = provenance::source_name(&record.provider);
        let mut close = false;
        let mut get = false;
        let mut toggle_favourite = false;
        let mut collect: Option<String> = None;
        let mut copy = false;
        let mut make_material = false;
        let has_set = record.kind == Kind::Texture
            && project.is_some_and(|project| {
                project
                    .join(texture_set_path(&record.id).replace('/', std::path::MAIN_SEPARATOR_STR))
                    .is_file()
            });
        let collections = self.collections.clone();
        egui::ScrollArea::vertical()
            .id_salt("toolbox drawer")
            .auto_shrink([false, true])
            .show(ui, |ui| {
                let Some(drawer) = &mut self.drawer else {
                    return;
                };
                ui.horizontal(|ui| {
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(&record.name)
                                .strong()
                                .size(theme::type_scale::BODY + 2.0),
                        )
                        .wrap(),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                        if ui
                            .small_button(icons::X)
                            .on_hover_text("Close the details")
                            .clicked()
                        {
                            close = true;
                        }
                    });
                });
                if let Some(texture) = &preview {
                    ui.add(egui::Image::new(texture).max_size(egui::Vec2::splat(160.0)));
                }
                let authors: Vec<&str> = record
                    .authors
                    .iter()
                    .map(String::as_str)
                    .filter(|author| !author.trim().is_empty())
                    .collect();
                ui.horizontal_wrapped(|ui| {
                    if !authors.is_empty() {
                        ui.label(format!("By {}", authors.join(", ")));
                    }
                    if record.url.starts_with("https://") {
                        ui.hyperlink_to(
                            format!("{} {source_name}", icons::ARROW_SQUARE_OUT),
                            &record.url,
                        );
                    } else if !source_name.is_empty() {
                        ui.label(format!("From {source_name}"));
                    }
                });
                ui.add_space(4.0);
                let badge = license::display(&record.license, &record.license_version);
                ui.horizontal_wrapped(|ui| {
                    ui.label(egui::RichText::new(badge).strong());
                    if record.license_url.starts_with("https://") {
                        ui.hyperlink_to("Licence", &record.license_url);
                    }
                });
                ui.add(egui::Label::new(license::plain_words(&record.license)).wrap());
                if let Some(why) = license::commercial_block(&record.license) {
                    ui.colored_label(
                        palette.warning,
                        format!("{} Not for a commercial game: {why}.", icons::WARNING),
                    );
                }
                ui.add_space(4.0);
                if let Some(size) = record.physical_size.as_deref().and_then(physical_text) {
                    ui.label(format!("Real size: {size}"));
                }
                if !record.maps.is_empty() {
                    let maps: Vec<&str> = record.maps.iter().map(|map| map_label(map)).collect();
                    ui.label(format!("Maps: {}", maps.join(", ")));
                }
                if record.polycount > 0 {
                    ui.label(format!("Triangles: {}", record.polycount));
                }
                ui.add_space(4.0);
                ui.label(egui::RichText::new("Packages").strong());
                let packages: Vec<&Variant> = record
                    .variants
                    .iter()
                    .filter(|variant| usable(record.kind, variant))
                    .collect();
                if drawer.lookup.is_some() {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label("Looking up its packages…");
                    });
                } else if packages.is_empty() {
                    ui.weak("None the engine can use.");
                } else {
                    for variant in packages {
                        let size = variant
                            .total_size()
                            .map(|bytes| format!(" · {}", size_text(bytes)))
                            .unwrap_or_default();
                        let here = drawer
                            .downloaded
                            .iter()
                            .any(|(label, _)| label == &variant.label);
                        let mark = if here { " · in library" } else { "" };
                        ui.label(format!("{}{size}{mark}", variant.label));
                    }
                }
                if !drawer.downloaded.is_empty() {
                    let labels: Vec<String> = drawer
                        .downloaded
                        .iter()
                        .map(|(label, bytes)| format!("{label} ({})", size_text(*bytes)))
                        .collect();
                    ui.label(format!("In your library: {}", labels.join(", ")));
                }
                if drawer.in_project.is_empty() {
                    ui.weak("Not in this project.");
                } else {
                    ui.label(format!("In this project: {}", drawer.in_project.join(", ")));
                }
                ui.add_space(6.0);
                responsive::toolbar(ui, |ui| {
                    let button = ui.add_enabled(
                        can_get && (plan.is_some() || record.variants.is_empty()),
                        widgets::button(
                            widgets::Role::Primary,
                            Some(icons::DOWNLOAD_SIMPLE),
                            "Get",
                            &palette,
                        ),
                    );
                    let button = match &plan {
                        Some(plan) => button.on_hover_text(plan),
                        None => button.on_hover_text(
                            "Gets the nearest package to the resolution and format chosen.",
                        ),
                    };
                    get = button.clicked();
                    let star = if drawer.favourite {
                        theme::filled_icons::STAR
                    } else {
                        icons::STAR
                    };
                    toggle_favourite = ui
                        .button(star)
                        .on_hover_text(if drawer.favourite {
                            "Remove from favourites"
                        } else {
                            "Add to favourites"
                        })
                        .clicked();
                    ui.menu_button(icons::FOLDER_PLUS, |ui| {
                        for name in &collections {
                            if ui.button(name).clicked() {
                                collect = Some(name.clone());
                                ui.close();
                            }
                        }
                        if !collections.is_empty() {
                            ui.separator();
                        }
                        ui.horizontal(|ui| {
                            ui.add(
                                egui::TextEdit::singleline(&mut drawer.new_collection)
                                    .hint_text("New collection")
                                    .desired_width(120.0),
                            );
                            if ui
                                .add_enabled(
                                    !drawer.new_collection.trim().is_empty(),
                                    egui::Button::new("Add"),
                                )
                                .clicked()
                            {
                                collect = Some(drawer.new_collection.trim().to_owned());
                                drawer.new_collection.clear();
                                ui.close();
                            }
                        });
                    })
                    .response
                    .on_hover_text("Add to a collection");
                    copy = ui
                        .button(icons::COPY)
                        .on_hover_text("Copy its credit line")
                        .clicked();
                    if record.kind == Kind::Texture {
                        make_material = ui
                            .add_enabled(
                                can_edit && has_set,
                                widgets::button(
                                    widgets::Role::Secondary,
                                    Some(icons::SPHERE),
                                    "Make material",
                                    &palette,
                                ),
                            )
                            .on_hover_text("Write a material file from its maps and open it.")
                            .on_disabled_hover_text("Get it into this project first.")
                            .clicked();
                    }
                });
                if let Some(said) = &drawer.said {
                    ui.weak(said);
                }
            });
        if close {
            self.drawer = None;
            return;
        }
        if get && let Some(project) = project {
            self.start_download(ui.ctx(), project, &key);
        }
        let said = (|| {
            if copy {
                ui.ctx()
                    .copy_text(license::credit_line(&record, &source_name));
                return Some("Credit line copied.".to_owned());
            }
            if make_material {
                actions.push(ToolboxAction::MakeMaterial {
                    set: texture_set_path(&record.id),
                    name: record.id.clone(),
                });
                return None;
            }
            let (_, db) = match library() {
                Ok(open) => open,
                Err(err) if toggle_favourite || collect.is_some() => return Some(err),
                Err(_) => return None,
            };
            if toggle_favourite {
                let favourite = !db.is_favourite(&key).unwrap_or(false);
                return Some(match db.set_favourite(&key, favourite) {
                    Ok(()) if favourite => "Added to favourites.".to_owned(),
                    Ok(()) => "Removed from favourites.".to_owned(),
                    Err(err) => err.to_string(),
                });
            }
            if let Some(name) = &collect {
                return Some(match db.add_to_collection(name, &key, None) {
                    Ok(()) => format!("Added to {name}."),
                    Err(err) => err.to_string(),
                });
            }
            None
        })();
        if said.is_some() {
            if collect.is_some() {
                self.refresh_collections();
            }
            self.refresh_drawer();
            if let Some(drawer) = &mut self.drawer {
                drawer.said = said;
            }
            if matches!(self.view, View::Favourites | View::Collection(_)) {
                self.load_view(ui.ctx());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_read_plainly() {
        assert_eq!(size_text(640_000), "640 KB");
        assert_eq!(size_text(12), "1 KB");
        assert_eq!(size_text(12_500_000), "12.5 MB");
        assert_eq!(size_text(1_250_000_000), "1.2 GB");
        assert_eq!(physical_text(&[2.0, 2.0]).as_deref(), Some("2 × 2 m"));
        assert_eq!(
            physical_text(&[0.4, 0.25, 0.2]).as_deref(),
            Some("0.4 × 0.25 × 0.2 m")
        );
        assert_eq!(physical_text(&[]), None);
        assert_eq!(physical_text(&[0.0, 1.0]), None);
        assert_eq!(
            texture_set_path("Rock030"),
            "assets/textures/Rock030/Rock030.texture.json"
        );
    }
}
