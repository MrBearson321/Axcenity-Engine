//! The Mixer panel (Phase 52): the SoundGroups as a tree, each with a live
//! level meter and Mute and Solo buttons, under the game's whole output. Meters
//! move and the buttons work during Play; they change what's heard, never
//! the world, and Stop clears them. Levels come from the Play session's
//! `GameAudio::take_levels`, and mute and solo go back with
//! `GameAudio::set_monitor`.

use std::collections::{HashMap, HashSet};

use axcenity_audio::Levels;
use axcenity_audio::Monitor;
use axcenity_audio::mix::Group;
use axcenity_core::InstanceId;
use eframe::egui::{self, RichText};

use crate::theme::{self, icons};
use crate::widgets;

/// How fast a meter falls once the sound behind it drops, in decibels a
/// second, so a peak stays long enough to read.
const FALL_DB_PER_SECOND: f32 = 30.0;
/// The quietest level a meter shows; anything below is empty.
const FLOOR_DB: f32 = -60.0;
/// Pixels each level of nesting indents a group.
const INDENT: f32 = 12.0;
const METER_WIDTH: f32 = 64.0;

/// The panel's state: the meters as drawn, and mute and solo.
#[derive(Default)]
pub struct Mixer {
    pub monitor: Monitor,
    /// Each group's meter, in decibels.
    meters: HashMap<InstanceId, f32>,
    master: f32,
}

impl Mixer {
    /// Takes one Play frame's peak levels: a meter jumps up to a louder
    /// peak and falls back slowly.
    pub fn record(&mut self, levels: &Levels, seconds: f64) {
        let fall = FALL_DB_PER_SECOND * seconds as f32;
        let step = |shown: f32, peak: f32| to_db(peak).max(shown - fall).max(FLOOR_DB);
        self.master = step(self.master, levels.master);
        self.meters.retain(|id, _| levels.groups.contains_key(id));
        for (&id, &peak) in &levels.groups {
            let shown = self.meters.entry(id).or_insert(FLOOR_DB);
            *shown = step(*shown, peak);
        }
    }

    /// Play stopped: the meters empty, and mute and solo go.
    pub fn stop(&mut self) {
        self.meters.clear();
        self.master = FLOOR_DB;
        self.monitor = Monitor::default();
    }

    /// Mutes `group`, or unmutes it.
    pub fn toggle_mute(&mut self, group: InstanceId) {
        toggle(&mut self.monitor.muted, group);
    }

    /// Solos `group`, or stops soloing it. Several can be soloed at once.
    pub fn toggle_solo(&mut self, group: InstanceId) {
        toggle(&mut self.monitor.soloed, group);
    }

    /// Draws the panel for `groups` (parents first, as `mix::groups` lists
    /// them). The buttons only work while `playing`.
    pub fn show(&mut self, ui: &mut egui::Ui, groups: &[Group], playing: bool) {
        let palette = theme::palette(ui.ctx());
        if groups.is_empty() {
            ui.add_space(16.0);
            ui.vertical_centered(|ui| {
                ui.label(
                    RichText::new(icons::FADERS)
                        .size(22.0)
                        .color(palette.text_muted),
                );
                ui.label(
                    RichText::new(
                        "No SoundGroups yet. Insert ▸ SoundGroup puts one in SoundService; set a \
                         Sound's SoundGroup to play it through one.",
                    )
                    .color(palette.text_weak),
                );
            });
            return;
        }
        if !playing {
            ui.label(
                RichText::new("Press Play to see levels, and to mute and solo.")
                    .color(palette.text_weak),
            );
            ui.add_space(4.0);
        }
        let row_height = ui.spacing().interact_size.y;
        let master = self.master;
        // Not "Master": that's what most games call their top SoundGroup,
        // which is listed below it.
        meter_row(
            ui,
            row_height,
            0.0,
            icons::SPEAKER_HIGH,
            (
                "Output",
                Some("Everything the game plays, after all its SoundGroups"),
            ),
            master,
            |_| {},
        );
        widgets::divider(ui);
        egui::ScrollArea::vertical()
            .auto_shrink(false)
            .show(ui, |ui| {
                for group in groups {
                    let depth = depth(groups, group);
                    let level = self.meters.get(&group.id).copied().unwrap_or(FLOOR_DB);
                    let muted = self.monitor.muted.contains(&group.id);
                    let soloed = self.monitor.soloed.contains(&group.id);
                    let mut clicked = (false, false);
                    meter_row(
                        ui,
                        row_height,
                        depth as f32 * INDENT,
                        icons::FADERS,
                        (&group.name, None),
                        level,
                        |ui| {
                            ui.add_enabled_ui(playing, |ui| {
                                let solo = format!("Solo {}", group.name);
                                clicked.1 =
                                    widgets::icon_button(ui, icons::HEADPHONES, &solo, soloed)
                                        .on_disabled_hover_text("Press Play to solo.")
                                        .clicked();
                                let (icon, tip) = if muted {
                                    (icons::SPEAKER_SLASH, format!("Unmute {}", group.name))
                                } else {
                                    (icons::SPEAKER_HIGH, format!("Mute {}", group.name))
                                };
                                clicked.0 = widgets::icon_button(ui, icon, &tip, muted)
                                    .on_disabled_hover_text("Press Play to mute.")
                                    .clicked();
                            });
                        },
                    );
                    if clicked.0 {
                        self.toggle_mute(group.id);
                    }
                    if clicked.1 {
                        self.toggle_solo(group.id);
                    }
                }
            });
    }
}

fn toggle(set: &mut HashSet<InstanceId>, id: InstanceId) {
    if !set.remove(&id) {
        set.insert(id);
    }
}

/// How many groups `group` is inside.
fn depth(groups: &[Group], group: &Group) -> usize {
    let mut depth = 0;
    let mut parent = group.parent;
    while let Some(id) = parent {
        depth += 1;
        parent = groups.iter().find(|g| g.id == id).and_then(|g| g.parent);
    }
    depth
}

/// A peak amplitude in decibels, at least the floor.
fn to_db(peak: f32) -> f32 {
    if peak > 0.0 {
        (20.0 * peak.log10()).max(FLOOR_DB)
    } else {
        FLOOR_DB
    }
}

/// One row: `buttons` from the right, then the meter, then the icon and
/// name (cut short with "…" when the column is narrow), `indent` in.
fn meter_row(
    ui: &mut egui::Ui,
    height: f32,
    indent: f32,
    icon: &str,
    (name, hint): (&str, Option<&str>),
    level_db: f32,
    buttons: impl FnOnce(&mut egui::Ui),
) {
    let palette = theme::palette(ui.ctx());
    let row = egui::Layout::right_to_left(egui::Align::Center);
    ui.allocate_ui_with_layout(egui::vec2(ui.available_width(), height), row, |ui| {
        buttons(ui);
        let (rect, response) =
            ui.allocate_exact_size(egui::vec2(METER_WIDTH, 8.0), egui::Sense::hover());
        let painter = ui.painter();
        painter.rect_filled(rect, 3.0, palette.inset);
        let fill = ((level_db - FLOOR_DB) / -FLOOR_DB).clamp(0.0, 1.0);
        if fill > 0.0 {
            let color = if level_db > -1.0 {
                palette.error
            } else if level_db > -6.0 {
                palette.warning
            } else {
                palette.success
            };
            let mut lit = rect;
            lit.set_width(rect.width() * fill);
            painter.rect_filled(lit, 3.0, color);
        }
        response.on_hover_text(if level_db <= FLOOR_DB {
            "Silent".to_owned()
        } else {
            format!("{level_db:.0} dB peak")
        });
        ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
            ui.add_space(indent);
            ui.label(RichText::new(icon).color(palette.text_weak));
            let label = ui.add(egui::Label::new(name).truncate());
            if let Some(hint) = hint {
                label.on_hover_text(hint);
            }
        });
    });
}

#[cfg(test)]
mod tests {
    use axcenity_core::{ClassName, World};

    use super::*;

    /// Master in SoundService, with Music and SFX inside it.
    fn groups() -> (Vec<Group>, [InstanceId; 3]) {
        let mut world = World::new();
        let service = world.sound_service();
        let mut add = |name: &str, parent| {
            let id = world.create(ClassName::SoundGroup).unwrap();
            world.set_name(id, name).unwrap();
            world.set_parent(id, Some(parent)).unwrap();
            id
        };
        let master = add("Master", service);
        let music = add("Music", master);
        let sfx = add("SFX", master);
        (axcenity_audio::mix::groups(&world), [master, music, sfx])
    }

    #[test]
    fn meters_jump_to_a_peak_and_fall_back_slowly() {
        let (_, [_, music, sfx]) = groups();
        let mut mixer = Mixer::default();
        let mut levels = Levels {
            master: 1.0,
            groups: HashMap::from([(music, 0.5), (sfx, 0.0)]),
        };
        mixer.record(&levels, 1.0 / 60.0);
        assert!((mixer.meters[&music] + 6.02).abs() < 0.01);
        assert_eq!(mixer.meters[&sfx], FLOOR_DB);
        assert_eq!(mixer.master, 0.0);
        levels.groups.insert(music, 0.0);
        mixer.record(&levels, 0.1);
        assert!(
            (mixer.meters[&music] + 9.02).abs() < 0.01,
            "falls 3 dB in 0.1 s"
        );
    }

    #[test]
    fn mute_and_solo_toggle_and_stop_clears_them() {
        let (_, [master, music, _]) = groups();
        let mut mixer = Mixer::default();
        mixer.toggle_mute(music);
        mixer.toggle_solo(master);
        assert!(mixer.monitor.muted.contains(&music));
        assert!(mixer.monitor.soloed.contains(&master));
        mixer.toggle_mute(music);
        assert!(mixer.monitor.muted.is_empty());
        mixer.stop();
        assert_eq!(mixer.monitor, Monitor::default());
    }

    #[test]
    fn the_mixer_draws_the_group_tree_in_a_narrow_column() {
        let (groups, [_, music, _]) = groups();
        let mut mixer = Mixer::default();
        mixer.record(
            &Levels {
                master: 0.8,
                groups: HashMap::from([(music, 0.6)]),
            },
            1.0 / 60.0,
        );
        mixer.toggle_mute(music);
        let size = egui::vec2(260.0, 200.0);
        let mut fit = None;
        let mut draw = |ctx: &egui::Context| {
            let palette = theme::palette(ctx);
            let frame = egui::Frame::new()
                .fill(palette.panel)
                .inner_margin(egui::Margin::symmetric(8, 6));
            egui::CentralPanel::default().frame(frame).show(ctx, |ui| {
                let room = ui.max_rect();
                mixer.show(ui, &groups, true);
                fit = Some((ui.min_rect(), room));
            });
        };
        let ctx = theme::test_context();
        let mut shown = String::new();
        for _ in 0..3 {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                ..Default::default()
            };
            shown = format!("{:?}", ctx.run(input, &mut draw).shapes);
        }
        for name in ["Output", "Master", "Music", "SFX"] {
            assert!(shown.contains(name), "{name} isn't drawn");
        }
        assert_eq!(
            shown.matches("\"Master\"").count(),
            shown.matches("\"Music\"").count(),
            "the whole output isn't called Master as well as the Master group"
        );
        if let Some(folder) = std::env::var_os("AXCENITY_SNAPSHOTS").map(std::path::PathBuf::from) {
            crate::snapshot::snapshot(
                &folder.join("panel-mixer.png"),
                size,
                vec![Vec::new(); 3],
                &mut draw,
            );
        }
        let (used, room) = fit.unwrap();
        assert!(used.width() <= room.width() + 0.5);
    }
}
