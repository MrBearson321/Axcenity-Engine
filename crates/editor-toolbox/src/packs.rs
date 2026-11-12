//! Asset packs (`*.axcenity-pack.json`, the Toolbox website's export): Import
//! pack… keeps a pack as a collection and shows what each item is, its
//! licence and whether it can come in; Get all fetches the ones a Toolbox
//! source offers, one after another, through the normal Get. Export
//! collection… writes a collection as a pack.
//!
//! A pack is a stranger's file, so its addresses are never downloaded: each
//! item is looked up again at its source by `provider:id`, and an item no
//! source offers is listed with a link to its page.

use axcenity_toolbox::pack::{self, Fetch, Planned};

use super::*;

/// The pack being reviewed.
pub(super) struct Review {
    /// The collection the pack was kept as.
    pub(super) collection: String,
    pub(super) planned: Vec<Planned>,
}

/// Reads a pack, keeps it as a collection in `db` and plans each item.
pub(super) fn import_pack(db: &Database, text: &str) -> Result<Review, String> {
    let manifest = pack::parse(text).map_err(|err| err.to_string())?;
    let planned = pack::plan(&manifest);
    let collection = manifest
        .save_as_collection(db)
        .map_err(|err| err.to_string())?;
    Ok(Review {
        collection,
        planned,
    })
}

/// Whether a Toolbox source can fetch this item (looked up again by id).
fn gettable(planned: &Planned) -> bool {
    !matches!(planned.fetch, Fetch::No(_))
        && planned
            .key
            .split_once(':')
            .and_then(|(provider, _)| axcenity_toolbox::source(provider))
            .is_some()
}

/// The items Get all fetches, as listings that look themselves up at their
/// source. Models are left out: each opens the Import dialog, so they're got
/// one at a time from their details.
pub(super) fn queue(db: &Database, review: &Review) -> Vec<Listing> {
    review
        .planned
        .iter()
        .filter(|planned| gettable(planned) && planned.kind != Some(Kind::Model))
        .filter_map(|planned| db.asset(&planned.key).ok().flatten())
        .map(|mut record| {
            // The pack's own file list is never trusted.
            record.variants.clear();
            let mut listing = Listing::from_record(record);
            listing.lookup = true;
            listing
        })
        .collect()
}

/// A collection's file name: `castle-kit.axcenity-pack.json`.
fn pack_file_name(collection: &str) -> String {
    let slug: String = collection
        .to_lowercase()
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|word| !word.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    let slug = if slug.is_empty() {
        "pack".to_owned()
    } else {
        slug
    };
    format!("{slug}.axcenity-pack.json")
}

/// What the Packs menu asked for beyond asset packs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PackAction {
    /// Install a `.axcenity-plugin` into the open project's `plugins/`.
    ImportPlugin(std::path::PathBuf),
}

impl ToolboxPanel {
    /// The Packs menu: Import pack…, Import plugin…, and Export collection…
    /// in a collection. Returns a plugin import when the creator picked one.
    pub(super) fn packs_menu(&mut self, ui: &mut egui::Ui) -> Option<PackAction> {
        let mut action = None;
        axcenity_editor_kit::widgets::menu_button(ui, None, "Packs", |ui| {
            if ui
                .button("Import pack…")
                .on_hover_text(
                    "Keep an asset pack from the Toolbox website as a collection, \
                     and get its assets.",
                )
                .clicked()
            {
                ui.close();
                if let Some(file) = rfd::FileDialog::new()
                    .add_filter("Asset pack", &["json"])
                    .pick_file()
                {
                    self.import_pack_file(ui.ctx(), &file);
                }
            }
            if ui
                .button("Import plugin…")
                .on_hover_text(
                    "Install a .axcenity-plugin shared by another creator into this \
                     project's plugins folder (or your Plugins folder with no project).",
                )
                .clicked()
            {
                ui.close();
                if let Some(file) = rfd::FileDialog::new()
                    .add_filter("Axcenity plugin", &["axcenity-plugin"])
                    .pick_file()
                {
                    action = Some(PackAction::ImportPlugin(file));
                }
            }
            if let View::Collection(name) = self.view.clone()
                && ui.button("Export collection…").clicked()
            {
                ui.close();
                if let Some(file) = rfd::FileDialog::new()
                    .set_file_name(pack_file_name(&name))
                    .add_filter("Asset pack", &["json"])
                    .save_file()
                {
                    self.export_collection(&name, &file);
                }
            }
        });
        action
    }

    fn import_pack_file(&mut self, ctx: &egui::Context, file: &Path) {
        let read = fs::read_to_string(file).map_err(|err| err.to_string());
        let review = read.and_then(|text| {
            let (_, db) = library()?;
            import_pack(&db, &text)
        });
        match review {
            Ok(review) => {
                self.error = None;
                self.drawer = None;
                self.view = View::Collection(review.collection.clone());
                self.pack = Some(review);
                self.refresh_collections();
                self.load_view(ctx);
            }
            Err(err) => self.error = Some(format!("Could not import that pack: {err}")),
        }
    }

    fn export_collection(&mut self, name: &str, file: &Path) {
        let written = library().and_then(|(_, db)| {
            let manifest = pack::export(&db, name, &pack::timestamp(db::now()))
                .map_err(|err| err.to_string())?;
            let text = serde_json::to_string_pretty(&manifest).map_err(|err| err.to_string())?;
            fs::write(file, text + "\n").map_err(|err| err.to_string())
        });
        match written {
            Ok(()) => {
                self.error = None;
                self.status = Some(format!("Exported {name} to {}.", file.display()));
            }
            Err(err) => self.error = Some(format!("Could not export {name}: {err}")),
        }
    }

    /// The pack's items: licence, size, and whether each can come in.
    pub(super) fn pack_review(
        &mut self,
        ui: &mut egui::Ui,
        project: Option<&Path>,
        can_edit: bool,
    ) {
        let Some(review) = &self.pack else {
            return;
        };
        let palette = theme::palette(ui.ctx());
        let mut close = false;
        let mut get_all = false;
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.strong(format!("Pack: {}", review.collection));
                let count = review
                    .planned
                    .iter()
                    .filter(|planned| gettable(planned) && planned.kind != Some(Kind::Model))
                    .count();
                if ui
                    .add_enabled(
                        count > 0 && project.is_some() && can_edit && self.download.is_none(),
                        widgets::button(
                            widgets::Role::Primary,
                            Some(icons::DOWNLOAD_SIMPLE),
                            &format!("Get all ({count})"),
                            &palette,
                        ),
                    )
                    .on_hover_text(
                        "Get each texture, sky and sound from its source into the project. \
                         Get models one at a time from their details.",
                    )
                    .clicked()
                {
                    get_all = true;
                }
                if ui.small_button("Close").clicked() {
                    close = true;
                }
            });
            egui::ScrollArea::vertical()
                .id_salt("toolbox pack review")
                .max_height(160.0)
                .show(ui, |ui| {
                    for planned in &review.planned {
                        ui.horizontal_wrapped(|ui| {
                            ui.label(&planned.name);
                            ui.label(egui::RichText::new(&planned.license).weak());
                            if let Some(size) = planned.size {
                                ui.label(egui::RichText::new(drawer::size_text(size)).weak());
                            }
                            match &planned.fetch {
                                Fetch::No(why) => {
                                    ui.colored_label(
                                        ui.visuals().error_fg_color,
                                        format!("Can't come in: {why}."),
                                    );
                                }
                                _ if !gettable(planned) => {
                                    ui.label(
                                        egui::RichText::new(
                                            "Not a Toolbox source: get it from its page.",
                                        )
                                        .weak(),
                                    );
                                }
                                _ => {}
                            }
                            if let Some(why) = planned.blocked {
                                ui.colored_label(palette.warning, why);
                            }
                        });
                    }
                });
        });
        if get_all && let Ok((_, db)) = library() {
            let listings = queue(&db, review);
            self.queue.extend(listings);
        }
        if close {
            self.pack = None;
        }
    }

    /// Starts the next queued Get once the last one is done.
    pub(super) fn advance_queue(&mut self, ctx: &egui::Context, project: Option<&Path>) {
        if self.download.is_some() {
            return;
        }
        let Some(project) = project else {
            self.queue.clear();
            return;
        };
        if let Some(listing) = self.queue.pop_front() {
            self.start_listing_download(ctx, project, &listing);
            if !self.queue.is_empty()
                && let Some(status) = &mut self.status
            {
                status.push_str(&format!(" ({} more after this)", self.queue.len()));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GOLDEN: &str = include_str!("../../toolbox/tests/fixtures/pack_manifest.json");

    #[test]
    fn an_imported_pack_is_kept_as_a_collection_and_planned() {
        let db = Database::in_memory().unwrap();
        let review = import_pack(&db, GOLDEN).unwrap();
        let collection = db.collection(&review.collection).unwrap().unwrap();
        assert_eq!(collection.items.len(), review.planned.len());
        assert!(!review.planned.is_empty());
    }

    #[test]
    fn get_all_looks_each_item_up_at_its_source_and_skips_models() {
        let db = Database::in_memory().unwrap();
        let review = import_pack(&db, GOLDEN).unwrap();
        let listings = queue(&db, &review);
        assert!(!listings.is_empty());
        for listing in &listings {
            assert!(listing.lookup, "{}", listing.name);
            assert!(listing.packages.is_empty(), "{}", listing.name);
            assert_ne!(listing.record.kind, Kind::Model);
            assert!(axcenity_toolbox::source(&listing.record.provider).is_some());
        }
    }

    #[test]
    fn a_damaged_pack_is_refused_and_nothing_is_kept() {
        let db = Database::in_memory().unwrap();
        assert!(import_pack(&db, "{ not a pack").is_err());
        assert!(db.collections().unwrap().is_empty());
    }

    #[test]
    fn a_collection_exports_under_a_plain_file_name() {
        assert_eq!(
            pack_file_name("Castle Kit!"),
            "castle-kit.axcenity-pack.json"
        );
        assert_eq!(pack_file_name("???"), "pack.axcenity-pack.json");
    }
}
