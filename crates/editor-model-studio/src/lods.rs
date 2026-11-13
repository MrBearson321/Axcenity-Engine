//! The Model Studio's Levels of detail fold (roadmap R15.3): the active
//! object's `_LOD1` and `_LOD2` copies, each level's triangle count and
//! share, the ratios Generate LODs uses, Regenerate and Remove LODs, and
//! Preview LOD 0–2, which draws every object as its copy at that level.
//! The renderer still picks the level a game draws by size on screen
//! (decision D3 in `docs/research/model-studio-r15.md`).

use axcenity_mesh_edit::{LOD_RATIOS, MIN_LOD_RATIO};
use eframe::egui;

use super::{Mode, StudioDoc};
use crate::theme::{self, icons};
use crate::widgets::{self, Role};

/// The fold's header.
pub const LODS_HEADER: &str = "Levels of detail";

/// One level of the active object: 0 is the object itself.
#[derive(Clone, Debug, PartialEq)]
pub struct LodRow {
    pub level: usize,
    /// The object that holds it.
    pub object: usize,
    pub triangles: usize,
    /// The share of the triangles it was generated with (a copy's
    /// Decimate), if it says.
    pub ratio: Option<f32>,
}

/// The ratios Generate LODs starts with.
pub fn default_ratios() -> [f32; 2] {
    LOD_RATIOS
}

impl StudioDoc {
    /// Generate LODs for the active object with [`StudioDoc::lod_ratios`],
    /// in place of copies it has (Regenerate), as one undo step.
    pub fn generate_lods(&mut self) -> Result<(), String> {
        let before = self.snapshot();
        let ratios = self.lod_ratios;
        let had = !self.model.lod_copies(self.active).is_empty();
        let index = self
            .model
            .generate_lods_with(self.active, &ratios)
            .ok_or("Generate LODs needs a full object with a mesh, not a group or a copy")?;
        self.active = index;
        self.objects = [index].into();
        self.mode = Mode::Object;
        self.selection.clear();
        self.record(
            if had {
                "Regenerate LODs"
            } else {
                "Generate LODs"
            },
            before,
        );
        Ok(())
    }

    /// Remove LODs: the active object's copies taken out, as one undo
    /// step. Nothing happens, and nothing is recorded, without copies.
    pub fn remove_lods(&mut self) -> Result<(), String> {
        if self.model.lod_copies(self.active).is_empty() {
            return Err("It has no LODs to remove".to_owned());
        }
        let before = self.snapshot();
        let index = self
            .model
            .remove_lods(self.active)
            .ok_or("Remove LODs needs a full object with a mesh")?;
        self.active = index;
        self.objects = [index].into();
        self.selection.clear();
        self.record("Remove LODs", before);
        Ok(())
    }

    /// The active object's levels: itself, then each copy, with their
    /// triangle counts. Worked out again only after a change.
    pub fn lod_rows(&mut self) -> Vec<LodRow> {
        let key = (self.revision, self.active);
        if let Some((kept, rows)) = &self.lod_rows
            && *kept == key
        {
            return rows.clone();
        }
        let active = self.active;
        let rows: Vec<LodRow> = std::iter::once(active)
            .chain(self.model.lod_copies(active))
            .enumerate()
            .map(|(level, object)| LodRow {
                level,
                object,
                triangles: self.model.triangles(object),
                ratio: (level > 0).then(|| self.model.lod_ratio(object)).flatten(),
            })
            .collect();
        self.lod_rows = Some((key, rows.clone()));
        rows
    }
}

/// The fold, under the Modifiers panel.
pub(super) fn panel(
    ui: &mut egui::Ui,
    doc: &mut StudioDoc,
    editable: bool,
    notices: &mut Vec<String>,
) {
    let Some(object) = doc.model.objects.get(doc.active) else {
        return;
    };
    if object.mesh.is_none() || doc.model.is_lod_copy(doc.active) {
        return;
    }
    ui.add_space(8.0);
    let mut header = egui::CollapsingHeader::new(LODS_HEADER)
        .id_salt("studio lods")
        .default_open(false);
    // Previewing a level keeps the fold open, so it can be turned off.
    if doc.lod_preview > 0 {
        header = header.open(Some(true));
    }
    header
        .show(ui, |ui| sections(ui, doc, editable, notices))
        .header_response
        .on_hover_text(
            "Simpler copies the game draws when the model is small on screen: \
             their sizes, how much each keeps, and a preview",
        );
}

fn sections(ui: &mut egui::Ui, doc: &mut StudioDoc, editable: bool, notices: &mut Vec<String>) {
    let palette = theme::palette(ui.ctx());
    let rows = doc.lod_rows();
    let full = rows.first().map_or(0, |row| row.triangles).max(1);
    egui::Grid::new("studio lod rows")
        .num_columns(3)
        .spacing([12.0, 2.0])
        .show(ui, |ui| {
            for row in &rows {
                ui.label(format!("LOD {}", row.level));
                ui.label(format!("{} triangles", thousands(row.triangles)));
                if row.level == 0 {
                    ui.weak("full");
                } else {
                    let share = row.triangles as f32 / full as f32 * 100.0;
                    let text = match row.ratio {
                        Some(ratio) => format!("{share:.0}% (asked {:.0}%)", ratio * 100.0),
                        None => format!("{share:.0}%"),
                    };
                    ui.weak(text);
                }
                ui.end_row();
            }
        });
    if rows.len() == 1 {
        ui.weak("No copies yet: the game simplifies it to a half and a quarter itself.");
    }

    ui.add_space(4.0);
    ui.add_enabled_ui(editable, |ui| {
        for (k, ratio) in doc.lod_ratios.iter_mut().enumerate() {
            ui.horizontal(|ui| {
                ui.label(format!("LOD {} keeps", k + 1));
                let mut percent = *ratio * 100.0;
                if ui
                    .add(
                        egui::DragValue::new(&mut percent)
                            .range(MIN_LOD_RATIO * 100.0..=100.0)
                            .speed(0.5)
                            .suffix("%"),
                    )
                    .on_hover_text("The share of the triangles this copy keeps when generated")
                    .changed()
                {
                    *ratio = percent / 100.0;
                }
            });
        }
        let has_copies = rows.len() > 1;
        ui.horizontal_wrapped(|ui| {
            let label = if has_copies {
                "Regenerate"
            } else {
                "Generate LODs"
            };
            if ui
                .add(widgets::button(
                    Role::Secondary,
                    Some(icons::STACK),
                    label,
                    &palette,
                ))
                .on_hover_text(
                    "Add _LOD1 and _LOD2 copies with a Decimate to these shares, \
                     in place of copies already there",
                )
                .clicked()
                && let Err(why) = doc.generate_lods()
            {
                notices.push(why);
            }
            if ui
                .add_enabled(
                    has_copies,
                    widgets::button(Role::Secondary, Some(icons::TRASH), "Remove LODs", &palette),
                )
                .on_hover_text("Take the copies out; the game simplifies it itself again")
                .clicked()
                && let Err(why) = doc.remove_lods()
            {
                notices.push(why);
            }
        });
    });

    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.label("Preview");
        for level in 0..=LOD_RATIOS.len() {
            ui.selectable_value(&mut doc.lod_preview, level, format!("LOD {level}"))
                .on_hover_text(if level == 0 {
                    "Draw the full objects"
                } else {
                    "Draw every object as its copy at this level, as the game does far away"
                });
        }
    });
}

/// `n` with a comma between thousands.
fn thousands(n: usize) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (k, digit) in digits.chars().enumerate() {
        if k > 0 && (digits.len() - k).is_multiple_of(3) {
            out.push(',');
        }
        out.push(digit);
    }
    out
}
