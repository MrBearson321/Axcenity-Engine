//! The Toolbox's library views: favourites, recently used, everything
//! downloaded on this computer, what this project uses, and each
//! collection. They read the metadata database (and the project's
//! `.axcenity/asset-sources.json`) when picked and after each Get, so they
//! work offline.

use axcenity_toolbox::library::Filter;
use axcenity_toolbox::provenance::Snapshot;

use super::*;

/// What the panel lists.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum View {
    /// Search results from the chosen source.
    Search,
    Favourites,
    Recent,
    /// Every asset downloaded into the asset directory.
    Library,
    /// The assets this project's files came from.
    Project,
    Collection(String),
}

impl View {
    pub fn label(&self) -> String {
        match self {
            View::Search => "Search".to_owned(),
            View::Favourites => "Favourites".to_owned(),
            View::Recent => "Recent".to_owned(),
            View::Library => "Downloaded".to_owned(),
            View::Project => "This project".to_owned(),
            View::Collection(name) => format!("Collection: {name}"),
        }
    }

    /// What an empty list says.
    pub(super) fn empty_text(&self, noun: &str) -> String {
        match self {
            View::Search => format!("No {noun} match that."),
            View::Favourites => format!("No favourite {noun} yet. Star one in its details."),
            View::Recent => format!("No {noun} used yet."),
            View::Library => format!("No {noun} downloaded on this computer yet."),
            View::Project => format!("No {noun} in this project came from the Toolbox."),
            View::Collection(name) => format!("No {noun} in {name} yet."),
        }
    }
}

/// The records a library view lists, of one kind.
pub(super) fn records(
    db: &Database,
    view: &View,
    kind: Kind,
    project: Option<&Path>,
) -> Result<Vec<AssetRecord>, String> {
    let text = |err: SourceError| err.to_string();
    let mut records = match view {
        View::Search => Vec::new(),
        View::Favourites => db.favourites().map_err(text)?,
        View::Recent => db.recents().map_err(text)?,
        View::Library => db
            .assets(&Filter {
                kind: Some(kind),
                downloaded: true,
                ..Filter::default()
            })
            .map_err(text)?,
        View::Project => match project {
            Some(project) => {
                let snapshot = Snapshot::read(project).map_err(text)?;
                let mut keys: Vec<String> = snapshot.files.iter().map(|f| f.key.clone()).collect();
                let installs = db
                    .installs(&axcenity_toolbox::provenance::project_key(project))
                    .map_err(text)?;
                keys.extend(installs.into_iter().map(|install| install.key));
                keys.sort();
                keys.dedup();
                let mut found = db.assets_by_key(&keys).map_err(text)?;
                // Records only this project's snapshot knows (made elsewhere).
                for key in &keys {
                    if !found.iter().any(|record| &record.key() == key)
                        && let Some(record) = snapshot.assets.get(key)
                    {
                        found.push(record.clone());
                    }
                }
                found
            }
            None => Vec::new(),
        },
        View::Collection(name) => match db.collection(name).map_err(text)? {
            Some(collection) => {
                let keys: Vec<String> = collection.items.into_iter().map(|item| item.key).collect();
                db.assets_by_key(&keys).map_err(text)?
            }
            None => Vec::new(),
        },
    };
    records.retain(|record| record.kind == kind);
    Ok(records)
}

impl ToolboxPanel {
    /// The view menu: Search, then the library's lists and collections.
    pub(super) fn view_choice(&mut self, ui: &mut egui::Ui) {
        let collections = self.collections.clone();
        let mut picked = None;
        egui::ComboBox::from_id_salt("toolbox view")
            .selected_text(self.view.label())
            .show_ui(ui, |ui| {
                let mut views = vec![
                    View::Search,
                    View::Favourites,
                    View::Recent,
                    View::Library,
                    View::Project,
                ];
                views.extend(collections.into_iter().map(View::Collection));
                for view in views {
                    if ui
                        .selectable_label(self.view == view, view.label())
                        .clicked()
                        && self.view != view
                    {
                        picked = Some(view);
                    }
                }
                ui.separator();
                if ui
                    .button(theme::with_icon(
                        icons::TRAY_ARROW_DOWN,
                        "Downloaded / Imported Assets…",
                    ))
                    .on_hover_text(
                        "Every asset downloaded or imported, of every kind, in a tab of its own.",
                    )
                    .clicked()
                {
                    self.open_inventory = true;
                }
            })
            .response
            .on_hover_text("Search a source, or list what's in your library.");
        if let Some(view) = picked {
            self.view = view;
            self.drawer = None;
            if self.view == View::Search {
                self.start_search(ui.ctx(), 1);
            } else {
                self.load_view(ui.ctx());
            }
        }
    }

    /// Shows `record` (from the Downloaded / Imported Assets tab): its kind's
    /// tab, the Downloaded view (This project when it's not `downloaded`),
    /// and its details, when next drawn.
    pub fn show_record(&mut self, project: Option<&Path>, record: AssetRecord, downloaded: bool) {
        if self.kind != record.kind && Kind::ALL.contains(&record.kind) {
            self.set_kind(record.kind);
        }
        self.view = if downloaded {
            View::Library
        } else {
            View::Project
        };
        self.shown_for = project.map(Path::to_owned);
        self.reveal = Some(record);
    }

    /// Reads the collections' names for the menus.
    pub(super) fn refresh_collections(&mut self) {
        self.collections = library()
            .ok()
            .and_then(|(_, db)| db.collections().ok())
            .unwrap_or_default()
            .into_iter()
            .map(|collection| collection.name)
            .collect();
    }

    /// Reads the current library view into the page.
    pub(super) fn load_view(&mut self, ctx: &egui::Context) {
        if self.view == View::Search {
            return;
        }
        self.generation += 1;
        self.status = None;
        self.error = None;
        let project = self.shown_for.clone();
        let found =
            library().and_then(|(_, db)| records(&db, &self.view, self.kind, project.as_deref()));
        match found {
            Ok(records) => {
                let listings: Vec<Listing> =
                    records.into_iter().map(Listing::from_record).collect();
                self.fetch_previews_for(ctx, &listings);
                self.page = Some(Page {
                    listings,
                    number: 1,
                    more: Vec::new(),
                });
            }
            Err(err) => {
                self.page = None;
                self.error = Some(format!("Could not read the asset library: {err}"));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axcenity_toolbox::provenance;

    fn record(provider: &str, id: &str, kind: Kind) -> AssetRecord {
        AssetRecord {
            provider: provider.into(),
            id: id.into(),
            kind,
            name: id.into(),
            license: "CC0".into(),
            ..AssetRecord::default()
        }
    }

    #[test]
    fn each_view_lists_its_records_of_the_tabs_kind() {
        let db = Database::in_memory().unwrap();
        let brick = record("polyhaven", "brick", Kind::Texture);
        let rain = record("openverse", "rain", Kind::Sound);
        let rock = record("ambientcg", "Rock030", Kind::Texture);
        for each in [&brick, &rain, &rock] {
            db.put_asset(each).unwrap();
        }
        db.set_favourite(&brick.key(), true).unwrap();
        db.set_favourite(&rain.key(), true).unwrap();
        db.touch_recent(&rock.key()).unwrap();
        db.add_to_collection("Castle", &rock.key(), None).unwrap();
        let ids = |view: View, kind| -> Vec<String> {
            records(&db, &view, kind, None)
                .unwrap()
                .into_iter()
                .map(|record| record.id)
                .collect()
        };
        assert_eq!(ids(View::Favourites, Kind::Texture), ["brick"]);
        assert_eq!(ids(View::Favourites, Kind::Sound), ["rain"]);
        assert_eq!(ids(View::Recent, Kind::Texture), ["Rock030"]);
        assert_eq!(
            ids(View::Collection("Castle".into()), Kind::Texture),
            ["Rock030"]
        );
        assert!(ids(View::Collection("Nope".into()), Kind::Texture).is_empty());
        assert!(ids(View::Project, Kind::Texture).is_empty());
    }

    #[test]
    fn this_project_lists_installs_and_the_snapshots_own_records() {
        let project = std::env::temp_dir().join(format!(
            "axcenity-toolbox-view-project-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&project);
        fs::create_dir_all(project.join("assets/sky")).unwrap();
        fs::write(project.join("assets/sky/sunset_2k.hdr"), "x").unwrap();
        // Installed on another computer: only the snapshot knows it.
        let elsewhere = Database::in_memory().unwrap();
        let sunset = record("polyhaven", "sunset", Kind::Hdri);
        elsewhere.put_asset(&sunset).unwrap();
        elsewhere
            .record_install(&Install {
                project: provenance::project_key(&project),
                path: "assets/sky/sunset_2k.hdr".into(),
                key: sunset.key(),
                variant: "2K HDR".into(),
                uid: None,
                installed_at: 1,
            })
            .unwrap();
        provenance::refresh(&elsewhere, &project).unwrap();
        let here = Database::in_memory().unwrap();
        let listed = records(&here, &View::Project, Kind::Hdri, Some(&project)).unwrap();
        assert_eq!(listed, [sunset]);
        let _ = fs::remove_dir_all(&project);
    }
}
