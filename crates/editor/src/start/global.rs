//! The start screen's Global Assets page: what's in the global assets
//! folder (see `global_assets.rs`), which every project's Assets panel
//! shows too. An imported model's files are one card. An asset can be
//! taken out of global assets here; making one global is done from a
//! project's Assets panel, where its files are.

use std::time::{Duration, Instant};

use super::*;
use crate::asset_browser::packages;
use crate::asset_browser::thumbnails::Thumbnails;
use crate::asset_browser::{self, AssetEntry, AssetKind};
use crate::global_assets;

/// How often the folder is looked at again while the page is shown.
const RESCAN: Duration = Duration::from_secs(2);
const CARD: egui::Vec2 = vec2(136.0, 158.0);
const CARD_PICTURE: f32 = 96.0;

#[derive(Default)]
pub(super) struct GlobalPage {
    library: Option<PathBuf>,
    files: Vec<AssetEntry>,
    scanned_at: Option<Instant>,
    thumbnails: Thumbnails,
    search: String,
    /// The card asked about in "Remove from global assets?", with a
    /// problem removing it.
    removing: Option<(Card, Option<String>)>,
    /// The last thing done, or a problem (`true`).
    notice: Option<(bool, String)>,
}

/// One card: a file, or an imported model's files.
#[derive(Clone)]
pub(super) struct Card {
    /// The file, or the model of a folder.
    entry: AssetEntry,
    pub(super) name: String,
    /// What's in a folder; a file's kind.
    detail: String,
    folder: bool,
}

impl GlobalPage {
    pub(super) fn set_library(&mut self, library: Option<PathBuf>) {
        self.thumbnails.open(library.as_deref());
        self.library = library;
        self.scanned_at = None;
    }

    fn rescan_if_due(&mut self) {
        if self.scanned_at.is_some_and(|at| at.elapsed() < RESCAN) {
            return;
        }
        self.scanned_at = Some(Instant::now());
        self.files = self
            .library
            .as_deref()
            .map(asset_browser::scan)
            .unwrap_or_default();
        self.thumbnails.forget_missing();
    }

    /// The cards, matching `search` (in lower case) if there is one: each
    /// imported model's folder, then the files in no folder. A search
    /// matches each file on its own.
    pub(super) fn cards(&mut self, search: &str) -> Vec<Card> {
        self.rescan_if_due();
        let file = |entry: &AssetEntry| Card {
            entry: entry.clone(),
            name: entry.name.clone(),
            detail: entry.kind.label().to_owned(),
            folder: false,
        };
        if !search.is_empty() {
            return self
                .files
                .iter()
                .filter(|entry| entry.path.to_lowercase().contains(search))
                .map(file)
                .collect();
        }
        let packages = packages::packages(&self.files);
        let mut cards: Vec<Card> = packages
            .iter()
            .map(|package| Card {
                entry: package.model.clone(),
                name: package.name.clone(),
                detail: package.summary(),
                folder: true,
            })
            .collect();
        cards.extend(
            self.files
                .iter()
                .filter(|entry| !packages.iter().any(|package| package.holds(entry)))
                .map(file),
        );
        cards
    }

    pub(super) fn show(&mut self, ui: &mut egui::Ui) {
        let palette = theme::palette(ui.ctx());
        self.thumbnails.poll(ui.ctx());
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label(
                    RichText::new("Global Assets")
                        .family(theme::display_family())
                        .size(type_scale::DISPLAY)
                        .extra_letter_spacing(theme::DISPLAY_TRACKING)
                        .color(palette.text_strong),
                );
                ui.add(
                    egui::Label::new(
                        RichText::new(
                            "Shared by every project on this computer. To add one, right-click \
                             it in a project's Assets panel and tick Global asset.",
                        )
                        .color(palette.text_secondary),
                    )
                    .wrap(),
                );
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let show = widgets::button(
                    Role::Secondary,
                    Some(icons::FOLDER_OPEN),
                    "Show Folder",
                    &palette,
                )
                .min_size(vec2(0.0, 32.0));
                if let Some(library) = self.library.clone()
                    && ui
                        .add(show)
                        .on_hover_text(library.display().to_string())
                        .clicked()
                {
                    let shown = std::fs::create_dir_all(library.join("assets"))
                        .and_then(|()| reveal(&library));
                    if let Err(err) = shown {
                        self.notice = Some((true, format!("Could not show the folder: {err}")));
                    }
                }
            });
        });
        if let Some((error, text)) = &self.notice {
            ui.add_space(10.0);
            let (icon, color) = if *error {
                (icons::WARNING_CIRCLE, palette.error)
            } else {
                (icons::CHECK_CIRCLE, palette.success)
            };
            ui.label(RichText::new(theme::with_icon(icon, text)).color(color));
        }

        ui.add_space(28.0);
        let mut search = std::mem::take(&mut self.search);
        ui.horizontal(|ui| {
            widgets::section_label(ui, "In every project");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                widgets::search_field(ui, &mut search, "Search global assets", 200.0);
            });
        });
        self.search = search;
        ui.add_space(8.0);
        let cards = self.cards(&self.search.trim().to_lowercase());
        if cards.is_empty() {
            ui.add_space(theme::space::XL);
            ui.vertical_centered(|ui| {
                ui.label(
                    RichText::new(icons::GLOBE)
                        .size(32.0)
                        .color(palette.text_muted),
                );
                ui.label(
                    RichText::new(if self.search.trim().is_empty() {
                        "No global assets yet. Mark one global in a project's Assets panel \
                         and it shows here, and in every project."
                    } else {
                        "No global asset's path has that in it."
                    })
                    .color(palette.text_weak),
                );
            });
        } else {
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = vec2(12.0, 12.0);
                for card in cards {
                    if self.card(ui, &card) {
                        self.removing = Some((card, None));
                    }
                }
            });
        }
        self.remove_dialog(ui.ctx());
    }

    /// Draws one card. True when Remove is clicked.
    fn card(&mut self, ui: &mut egui::Ui, card: &Card) -> bool {
        let palette = theme::palette(ui.ctx());
        let (rect, response) = ui.allocate_exact_size(CARD, Sense::click());
        let response = response.on_hover_text(format!(
            "{}\nIn every project's Assets panel. A project gets its own copy the first time \
             it uses it.",
            card.entry.path
        ));
        let hovered = ui.rect_contains_pointer(rect);
        let painter = ui.painter();
        painter.rect(
            rect,
            CARD_RADIUS,
            if hovered {
                palette.control_hover
            } else {
                palette.surface
            },
            Stroke::new(1.0_f32, palette.border),
            egui::StrokeKind::Inside,
        );
        let picture = Rect::from_center_size(
            pos2(rect.center().x, rect.top() + 8.0 + CARD_PICTURE / 2.0),
            vec2(CARD_PICTURE, CARD_PICTURE),
        );
        painter.rect_filled(picture, 6, palette.inset);
        let thumbnail = (!card.folder
            && ui.is_rect_visible(rect)
            && matches!(
                card.entry.kind,
                AssetKind::Image | AssetKind::Texture | AssetKind::TextureSet | AssetKind::Particle
            ))
        .then(|| {
            self.thumbnails.get(
                ui.ctx(),
                &card.entry.path,
                card.entry.kind == AssetKind::TextureSet,
            )
        })
        .flatten();
        let painter = ui.painter();
        match &thumbnail {
            Some(texture) => {
                let [w, h] = texture.size().map(|n| n as f32);
                let scale = (CARD_PICTURE / w).min(CARD_PICTURE / h);
                let fitted = Rect::from_center_size(picture.center(), vec2(w, h) * scale);
                let uv = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
                painter.image(texture.id(), fitted, uv, egui::Color32::WHITE);
            }
            None => {
                let icon = if card.folder {
                    icons::FOLDER_SIMPLE
                } else {
                    asset_browser::icon_of(&card.entry)
                };
                painter.text(
                    picture.center(),
                    egui::Align2::CENTER_CENTER,
                    icon,
                    FontId::proportional(CARD_PICTURE * 0.45),
                    card.entry.kind.color(ui.visuals()),
                );
            }
        }
        let name = widgets::elided(
            ui,
            card.name.clone(),
            FontId::proportional(type_scale::SMALL + 0.5),
            palette.text_strong,
            CARD.x - 16.0,
            1,
        );
        let name_top = picture.bottom() + 8.0;
        ui.painter().galley(
            pos2(rect.left() + 8.0, name_top),
            name.clone(),
            palette.text_strong,
        );
        let detail = widgets::elided(
            ui,
            card.detail.clone(),
            FontId::proportional(type_scale::CAPTION),
            palette.text_weak,
            CARD.x - 16.0,
            1,
        );
        ui.painter().galley(
            pos2(rect.left() + 8.0, name_top + name.size().y + 2.0),
            detail,
            palette.text_weak,
        );
        let mut remove = false;
        response.context_menu(|ui| {
            if ui
                .button(theme::with_icon(icons::TRASH, "Remove from global assets"))
                .clicked()
            {
                remove = true;
                ui.close();
            }
        });
        // The remove button shows over the card's corner while it's hovered.
        if hovered {
            let corner = Rect::from_min_size(rect.right_top() + vec2(-30.0, 6.0), vec2(24.0, 24.0));
            let button = ui.put(corner, egui::Button::new(icons::TRASH).frame(true));
            if button.on_hover_text("Remove from global assets").clicked() {
                remove = true;
            }
        }
        remove
    }

    /// "Remove from global assets?", while it's asked.
    fn remove_dialog(&mut self, ctx: &egui::Context) {
        let Some((card, error)) = &mut self.removing else {
            return;
        };
        let palette = theme::palette(ctx);
        let mut close = false;
        let mut removed = None;
        let frame = egui::Frame::popup(&ctx.style())
            .corner_radius(12)
            .inner_margin(egui::Margin::same(20));
        let modal = egui::Modal::new(egui::Id::new("remove global asset"))
            .frame(frame)
            .show(ctx, |ui| {
                ui.set_width(400.0);
                dialog_title(ui, &palette, "Remove from global assets?", None);
                ui.add(
                    egui::Label::new(
                        RichText::new(format!(
                            "\"{}\" is deleted from global assets, so new projects won't have \
                             it. Projects that already use it keep their own copy.",
                            card.name
                        ))
                        .color(palette.text_secondary),
                    )
                    .wrap(),
                );
                error_line(ui, &palette, error.as_deref());
                let (remove, cancel) =
                    dialog_buttons(ui, &palette, ("Remove", Role::Danger, true), "Cancel");
                if remove {
                    let result = match &self.library {
                        Some(library) => global_assets::remove(library, &card.entry.path),
                        None => Err("there's no folder for global assets".to_owned()),
                    };
                    match result {
                        Ok(_) => {
                            removed = Some(card.name.clone());
                            close = true;
                        }
                        Err(problem) => *error = Some(problem),
                    }
                }
                close |= cancel;
            });
        if modal.should_close() {
            close = true;
        }
        if let Some(name) = removed {
            self.notice = Some((false, format!("Removed {name} from global assets.")));
            self.scanned_at = None;
        }
        if close {
            self.removing = None;
        }
    }
}
