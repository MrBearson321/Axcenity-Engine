use super::*;

impl PropertyDoc {
    /// The toolbar's part: play/pause, back to the start, Stop and Loop.
    pub fn controls(&mut self, ui: &mut egui::Ui) {
        let (icon, hover) = if self.playing {
            (icons::PAUSE, "Pause")
        } else {
            (icons::PLAY, "Play")
        };
        if ui.button(icon).on_hover_text(hover).clicked() {
            if !self.playing && self.playhead >= self.file.duration {
                self.playhead = 0.0;
            }
            self.playing = !self.playing;
        }
        if ui
            .button(icons::SKIP_BACK)
            .on_hover_text("Back to the start")
            .clicked()
        {
            self.playhead = 0.0;
        }
        if ui
            .button(icons::STOP)
            .on_hover_text("Stop and go back to the start")
            .clicked()
        {
            self.playing = false;
            self.playhead = 0.0;
        }
        ui.checkbox(&mut self.file.looping, "Loop");
        ui.monospace(format!("{:.2} s", self.playhead));
    }

    /// The drawer: the copy's objects as a tree, each with its tracks, and
    /// under a skinned root its bones. Returns what was clicked.
    pub fn objects(&mut self, ui: &mut egui::Ui, preview: &ObjectPreview) -> Option<Drawn> {
        let Some(root) = preview.root() else {
            ui.weak("Select a Part, Model or ScreenGui in the Explorer.");
            return None;
        };
        let world = preview.world();
        let mut picked = None;
        let mut delete = None;
        let mut shown = BTreeSet::new();
        for (id, depth) in preview.tree() {
            let Some(instance) = world.get(id) else {
                continue;
            };
            let Some(target) = target_path(world, root, id) else {
                continue;
            };
            ui.horizontal(|ui| {
                ui.add_space(depth as f32 * 12.0);
                let text = format!(
                    "{} {}",
                    theme::class_icon(instance.class()),
                    instance.name()
                );
                let selected = preview.selected == Some(id) && preview.bones.selected.is_none();
                let label = egui::Button::selectable(selected, text).truncate();
                if ui.add(label).clicked() {
                    picked = Some(Drawn::Object(id));
                }
            });
            if id == root
                && let Some(rig) = preview.rig()
            {
                let paths = axcenity_anim::bone_paths(rig);
                for (bone, bone_depth) in super::super::posing::bone_rows(rig) {
                    let tracks: Vec<&PropertyTrack> = self
                        .file
                        .tracks
                        .iter()
                        .filter(|t| t.property == PropertyName::Bone)
                        .filter(|t| axcenity_anim::find_bone(rig, &paths, &t.target) == Some(bone))
                        .collect();
                    ui.horizontal(|ui| {
                        ui.add_space(depth as f32 * 12.0 + 12.0);
                        let keys = tracks.iter().map(|t| t.keys.len()).sum();
                        let row = super::super::posing::bone_row(
                            ui,
                            &rig.bones[bone].name,
                            bone_depth,
                            preview.bones.selected == Some(bone),
                            false,
                            keys,
                        );
                        if row.clicked() {
                            picked = Some(Drawn::Bone(bone));
                        }
                    });
                    for track in tracks {
                        shown.insert(track.id.clone());
                        ui.horizontal(|ui| {
                            ui.add_space(
                                depth as f32 * 12.0 + 12.0 + bone_depth as f32 * 8.0 + 18.0,
                            );
                            self.track_label(ui, track, &mut delete);
                        });
                    }
                }
            }
            for track in self.file.tracks.iter().filter(|t| t.target == target) {
                shown.insert(track.id.clone());
                ui.horizontal(|ui| {
                    ui.add_space(depth as f32 * 12.0 + 18.0);
                    self.track_label(ui, track, &mut delete);
                });
            }
        }
        let lost: Vec<&PropertyTrack> = self
            .file
            .tracks
            .iter()
            .filter(|t| !shown.contains(&t.id))
            .collect();
        if !lost.is_empty() {
            ui.add_space(4.0);
            ui.weak("Not in this object:");
            for track in lost {
                ui.horizontal(|ui| {
                    ui.add_space(6.0);
                    ui.weak(&track.target);
                    self.track_label(ui, track, &mut delete);
                });
            }
        }
        if let Some(id) = delete {
            self.delete_track(&id);
        }
        picked
    }

    pub(super) fn track_label(
        &self,
        ui: &mut egui::Ui,
        track: &PropertyTrack,
        delete: &mut Option<String>,
    ) {
        let selected = self.selected_track.as_deref() == Some(track.id.as_str());
        let text = format!("{} {}", icons::KEY, track.property.as_str());
        ui.add(egui::Button::selectable(selected, text).truncate())
            .on_hover_text(format!("{} keys", track.keys.len()))
            .context_menu(|ui| {
                if ui
                    .button(format!("{} Delete track", icons::TRASH))
                    .clicked()
                {
                    *delete = Some(track.id.clone());
                    ui.close();
                }
            });
    }

    /// Add track, Key selected, copy, paste, delete and snap, then the
    /// dope sheet. Returns true if Key selected was pressed.
    pub fn keys(&mut self, ui: &mut egui::Ui, focus: Option<&Focus>) -> bool {
        let mut key = false;
        crate::responsive::toolbar(ui, |ui| {
            let addable: Vec<&EvaluatedProperty> = focus
                .map(|focus| {
                    focus
                        .values
                        .iter()
                        .filter(|v| {
                            !self
                                .file
                                .tracks
                                .iter()
                                .any(|t| t.target == v.target && t.property == v.property)
                        })
                        .collect()
                })
                .unwrap_or_default();
            ui.add_enabled_ui(!addable.is_empty(), |ui| {
                ui.menu_button(format!("{} Add track", icons::PLUS), |ui| {
                    for value in &addable {
                        if ui.button(value.property.as_str()).clicked() {
                            self.key_values(std::slice::from_ref(*value));
                            ui.close();
                        }
                    }
                })
                .response
                .on_disabled_hover_text("Select something in the preview to add its tracks");
            });
            if ui
                .add_enabled(
                    focus.is_some(),
                    egui::Button::new(format!("{} Key selected", icons::KEY)),
                )
                .on_hover_text("Key the selected object at the playhead (K)")
                .clicked()
            {
                key = true;
            }
            let any = !self.selected.is_empty();
            if ui
                .add_enabled(any, egui::Button::new(icons::COPY))
                .on_hover_text("Copy the selected keys (Ctrl+C)")
                .clicked()
            {
                self.copy();
            }
            if ui
                .add_enabled(self.can_paste(), egui::Button::new(icons::CLIPBOARD))
                .on_hover_text("Paste at the playhead (Ctrl+V)")
                .clicked()
            {
                self.paste();
            }
            if ui
                .add_enabled(any, egui::Button::new(icons::TRASH))
                .on_hover_text("Delete the selected keys (Delete)")
                .clicked()
            {
                self.delete_selected();
            }
            ui.checkbox(&mut self.snap, "Snap")
                .on_hover_text("Put keys and the playhead on whole frames");
            if self.snap {
                ui.add(
                    egui::DragValue::new(&mut self.fps)
                        .range(1.0..=240.0)
                        .speed(1.0)
                        .suffix(" fps"),
                );
            }
        });
        self.dope_sheet(ui);
        key
    }

    /// Delete, K, Ctrl+C, Ctrl+X and Ctrl+V, while the mouse is over the
    /// Animation tab. Returns true if K (Key selected) was pressed.
    pub fn shortcuts(&mut self, ui: &egui::Ui) -> bool {
        let (delete, key, copy, cut, paste) = ui.input(|input| {
            let command = input.modifiers.command;
            let plain = input.modifiers.is_none();
            (
                plain
                    && (input.key_pressed(egui::Key::Delete)
                        || input.key_pressed(egui::Key::Backspace)),
                plain && input.key_pressed(egui::Key::K),
                command && input.key_pressed(egui::Key::C),
                command && input.key_pressed(egui::Key::X),
                command && input.key_pressed(egui::Key::V),
            )
        });
        if copy || cut {
            self.copy();
        }
        if delete || cut {
            self.delete_selected();
        }
        if paste {
            self.paste();
        }
        key
    }

    /// The clip's settings: its name, duration, the selected key and the
    /// events. Returns true if Rename was pressed.
    pub fn inspector(&mut self, ui: &mut egui::Ui, root: Option<&str>) -> bool {
        let mut rename = false;
        ui.horizontal_wrapped(|ui| {
            ui.heading(axcenity_anim::files::clip_name(&self.path));
            if ui
                .small_button(icons::PENCIL_SIMPLE)
                .on_hover_text("Rename this clip (its file)")
                .clicked()
            {
                rename = true;
            }
        });
        ui.weak(&self.path);
        if root.is_none() {
            ui.weak("Select a Part, Model or ScreenGui in the Explorer to animate a copy of it.");
        }
        ui.add_space(4.0);
        crate::responsive::form(ui, "property clip settings", |ui| {
            ui.label("Duration");
            let changed = ui
                .add(
                    egui::DragValue::new(&mut self.file.duration)
                        .speed(0.05)
                        .range(0.0..=3600.0)
                        .suffix(" s"),
                )
                .changed();
            if changed {
                // Keys past the new end move to it.
                let end = self.file.duration;
                for key in self.file.tracks.iter_mut().flat_map(|t| &mut t.keys) {
                    key.time = key.time.min(end);
                }
                for event in &mut self.file.events {
                    event.time = event.time.min(end);
                }
            }
            ui.end_row();
        });

        let one = (self.selected.len() == 1)
            .then(|| self.selected.first().cloned())
            .flatten();
        if let Some((track_id, key_id)) = one
            && let Some(track) = self.file.tracks.iter_mut().find(|t| t.id == track_id)
            && let Some(key) = track.keys.iter_mut().find(|k| k.id == key_id)
        {
            ui.separator();
            ui.strong(format!("Key · {}", track.property.as_str()));
            ui.weak(&track.target);
            let duration = self.file.duration;
            let property = track.property;
            crate::responsive::form(ui, "property key", |ui| {
                ui.label("Time");
                ui.add(egui::DragValue::new(&mut key.time).speed(0.01).suffix(" s"));
                key.time = key.time.clamp(0.0, duration);
                ui.end_row();
                for (i, label) in value_labels(property, key.value.len()).iter().enumerate() {
                    ui.label(*label);
                    let unit = matches!(
                        property,
                        PropertyName::Color
                            | PropertyName::BackgroundColor3
                            | PropertyName::TextColor3
                            | PropertyName::Transparency
                            | PropertyName::BackgroundTransparency
                            | PropertyName::ImageTransparency
                    );
                    let mut drag = egui::DragValue::new(&mut key.value[i]).speed(0.1);
                    if unit {
                        drag = drag.speed(0.01).range(0.0..=1.0);
                    }
                    ui.add(drag);
                    ui.end_row();
                }
                ui.label("Interpolation");
                egui::ComboBox::from_id_salt("property key interpolation")
                    .selected_text(match key.interp {
                        Interp::Step => "Step",
                        Interp::Linear => "Linear",
                    })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut key.interp, Interp::Step, "Step");
                        ui.selectable_value(&mut key.interp, Interp::Linear, "Linear");
                    });
                ui.end_row();
            });
        } else if self.selected.len() > 1 {
            ui.separator();
            ui.weak(format!(
                "{} keys selected. Drag one to move them all.",
                self.selected.len()
            ));
        }

        ui.separator();
        ui.strong("Events");
        let mut remove = None;
        for (i, event) in self.file.events.iter_mut().enumerate() {
            ui.horizontal_wrapped(|ui| {
                ui.add(
                    egui::DragValue::new(&mut event.time)
                        .speed(0.01)
                        .suffix(" s"),
                );
                ui.add(egui::TextEdit::singleline(&mut event.name).desired_width(110.0));
                if ui
                    .small_button(icons::TRASH)
                    .on_hover_text("Remove")
                    .clicked()
                {
                    remove = Some(i);
                }
            });
        }
        if let Some(i) = remove {
            self.file.events.remove(i);
        }
        ui.horizontal_wrapped(|ui| {
            ui.add(egui::TextEdit::singleline(&mut self.new_event).desired_width(110.0));
            if ui
                .add_enabled(
                    !self.new_event.trim().is_empty(),
                    egui::Button::new(format!("{} Add at playhead", icons::PLUS)),
                )
                .clicked()
            {
                self.file.events.push(axcenity_anim::ClipEvent {
                    time: self.playhead.clamp(0.0, self.file.duration),
                    name: self.new_event.trim().to_owned(),
                });
            }
        });

        ui.separator();
        ui.horizontal_wrapped(|ui| {
            if self.unwritten() {
                ui.weak("Saving…");
            } else {
                ui.weak(format!("{} Saved", icons::CHECK));
            }
            if ui
                .add_enabled(self.file != self.opened, egui::Button::new("Revert"))
                .on_hover_text("Put the clip back as it was when it was opened")
                .clicked()
            {
                self.file = self.opened.clone();
                self.selected.clear();
                self.playhead = 0.0;
            }
        });
        rename
    }

    pub(super) fn dope_sheet(&mut self, ui: &mut egui::Ui) {
        let palette = theme::palette(ui.ctx());
        let duration = self.file.duration.max(0.001);
        let rows = self.file.tracks.len().max(1);
        let height = RULER + ROW * rows as f32 + 4.0;
        let (rect, response) = ui.allocate_exact_size(
            egui::vec2(ui.available_width(), height),
            egui::Sense::click_and_drag(),
        );
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, palette.inset);

        let label_w = (rect.width() * 0.3).clamp(70.0, 160.0);
        let timeline = egui::Rect::from_min_max(
            egui::pos2(rect.left() + label_w + 6.0, rect.top()),
            egui::pos2(rect.right() - 8.0, rect.bottom()),
        );
        let width = timeline.width().max(1.0);
        let x_of = |t: f64| timeline.left() + (t / duration) as f32 * width;
        let t_of =
            |x: f32| (((x - timeline.left()) / width) as f64 * duration).clamp(0.0, duration);
        let rows_used = self.file.tracks.len();
        let row_of = |y: f32| {
            let row = ((y - rect.top() - RULER) / ROW).floor();
            (row >= 0.0 && (row as usize) < rows_used).then_some(row as usize)
        };
        let key_at = |tracks: &[PropertyTrack], pos: egui::Pos2| -> Option<KeyRef> {
            let track = tracks.get(row_of(pos.y)?)?;
            track
                .keys
                .iter()
                .map(|k| (k, (x_of(k.time) - pos.x).abs()))
                .filter(|(_, dx)| *dx <= GRAB)
                .min_by(|a, b| a.1.total_cmp(&b.1))
                .map(|(k, _)| (track.id.clone(), k.id.clone()))
        };

        // Ruler: a tick every "nice" step, and frames when they're far apart.
        let step = nice_step(duration, width);
        let mut t = 0.0;
        while t <= duration + 1e-9 {
            let x = x_of(t);
            painter.line_segment(
                [
                    egui::pos2(x, rect.top() + 12.0),
                    egui::pos2(x, rect.bottom()),
                ],
                egui::Stroke::new(1.0_f32, palette.border),
            );
            painter.text(
                egui::pos2(x + 2.0, rect.top() + 1.0),
                egui::Align2::LEFT_TOP,
                format!("{t:.2}")
                    .trim_end_matches('0')
                    .trim_end_matches('.')
                    .to_owned(),
                egui::FontId::proportional(10.0),
                palette.text_weak,
            );
            t += step;
        }
        if self.snap && self.fps > 0.0 && width / (duration * self.fps) as f32 > 6.0 {
            let frames = (duration * self.fps).floor() as usize;
            for frame in 0..=frames {
                let x = x_of(frame as f64 / self.fps);
                painter.line_segment(
                    [
                        egui::pos2(x, rect.top() + RULER - 4.0),
                        egui::pos2(x, rect.top() + RULER),
                    ],
                    egui::Stroke::new(1.0_f32, palette.border_strong),
                );
            }
        }

        for (row, track) in self.file.tracks.iter().enumerate() {
            let y = rect.top() + RULER + row as f32 * ROW;
            let row_rect = egui::Rect::from_min_size(
                egui::pos2(rect.left(), y),
                egui::vec2(rect.width(), ROW),
            );
            if self.selected_track.as_deref() == Some(track.id.as_str()) {
                painter.rect_filled(row_rect, 0.0, palette.accent_soft);
            } else if row % 2 == 1 {
                painter.rect_filled(row_rect, 0.0, palette.surface.gamma_multiply(0.4));
            }
            let name = match track.target.as_str() {
                "." | "" => "(root)",
                path => path.rsplit('/').next().unwrap_or(path),
            };
            let label_rect = egui::Rect::from_min_size(
                egui::pos2(rect.left() + 4.0, y),
                egui::vec2(label_w, ROW),
            );
            painter.with_clip_rect(label_rect).text(
                egui::pos2(label_rect.left(), y + ROW / 2.0),
                egui::Align2::LEFT_CENTER,
                format!("{name} · {}", track.property.as_str()),
                egui::FontId::proportional(12.0),
                palette.text,
            );
            for key in &track.keys {
                let center = egui::pos2(x_of(key.time), y + ROW / 2.0);
                let selected = self.selected.contains(&(track.id.clone(), key.id.clone()));
                let (fill, r) = if selected {
                    (palette.accent, 6.0)
                } else {
                    (palette.text_secondary, 5.0)
                };
                let diamond = vec![
                    center + egui::vec2(0.0, -r),
                    center + egui::vec2(r, 0.0),
                    center + egui::vec2(0.0, r),
                    center + egui::vec2(-r, 0.0),
                ];
                painter.add(egui::Shape::convex_polygon(
                    diamond,
                    fill,
                    egui::Stroke::new(1.0_f32, palette.inset),
                ));
            }
        }
        if self.file.tracks.is_empty() {
            painter.text(
                egui::pos2(timeline.left(), rect.top() + RULER + ROW / 2.0),
                egui::Align2::LEFT_CENTER,
                "No tracks yet: select something in the preview, move it, and press K.",
                egui::FontId::proportional(12.0),
                palette.text_weak,
            );
        }

        // Interaction.
        let (modifiers, origin) = ui.input(|i| (i.modifiers, i.pointer.press_origin()));
        let add = modifiers.shift || modifiers.command;
        if response.drag_started()
            && let Some(origin) = origin
        {
            self.gesture = if origin.y < rect.top() + RULER {
                Some(Gesture::Scrub)
            } else if let Some(hit) = key_at(&self.file.tracks, origin) {
                if !self.selected.contains(&hit) {
                    if !add {
                        self.selected.clear();
                    }
                    self.selected.insert(hit.clone());
                }
                let start = self.selected_keys();
                let anchor = start
                    .iter()
                    .find(|(key, _)| *key == hit)
                    .map_or(0.0, |(_, time)| *time);
                Some(Gesture::Keys {
                    from: t_of(origin.x),
                    anchor,
                    start,
                })
            } else if origin.x >= timeline.left() {
                Some(Gesture::Box { from: origin, add })
            } else {
                None
            };
        }
        if let Some(pointer) = response.interact_pointer_pos() {
            match &self.gesture {
                Some(Gesture::Scrub) if response.dragged() => {
                    self.playhead = self.snapped(t_of(pointer.x));
                    self.playing = false;
                }
                Some(Gesture::Keys {
                    from,
                    anchor,
                    start,
                }) if response.dragged() => {
                    let moved = self.snapped(anchor + t_of(pointer.x) - from) - anchor;
                    let end = self.file.duration;
                    for ((track_id, key_id), time) in start.clone() {
                        if let Some(key) = self
                            .file
                            .tracks
                            .iter_mut()
                            .find(|t| t.id == track_id)
                            .and_then(|t| t.keys.iter_mut().find(|k| k.id == key_id))
                        {
                            key.time = (time + moved).clamp(0.0, end);
                        }
                    }
                }
                Some(Gesture::Box { from, .. }) => {
                    let area = egui::Rect::from_two_pos(*from, pointer);
                    painter.rect(
                        area,
                        0.0,
                        palette.accent_soft,
                        egui::Stroke::new(1.0_f32, palette.accent),
                        egui::StrokeKind::Inside,
                    );
                }
                _ => {}
            }
        }
        if response.drag_stopped() {
            match self.gesture.take() {
                Some(Gesture::Keys { .. }) => self.tidy(),
                Some(Gesture::Box { from, add }) => {
                    if let Some(to) = response.interact_pointer_pos() {
                        let area = egui::Rect::from_two_pos(from, to);
                        if !add {
                            self.selected.clear();
                        }
                        for (row, track) in self.file.tracks.iter().enumerate() {
                            let y = rect.top() + RULER + row as f32 * ROW + ROW / 2.0;
                            for key in &track.keys {
                                if area.contains(egui::pos2(x_of(key.time), y)) {
                                    self.selected.insert((track.id.clone(), key.id.clone()));
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        if response.clicked()
            && let Some(pos) = response.interact_pointer_pos()
        {
            if let Some(row) = row_of(pos.y) {
                self.selected_track = Some(self.file.tracks[row].id.clone());
            }
            match key_at(&self.file.tracks, pos) {
                Some(hit) if add => {
                    if !self.selected.remove(&hit) {
                        self.selected.insert(hit);
                    }
                }
                Some(hit) => {
                    self.selected = BTreeSet::from([hit]);
                }
                None => {
                    if !add {
                        self.selected.clear();
                    }
                    if pos.x >= timeline.left() {
                        self.playhead = self.snapped(t_of(pos.x));
                        self.playing = false;
                    }
                }
            }
        }

        // Playhead.
        let px = x_of(self.playhead.clamp(0.0, duration));
        painter.line_segment(
            [egui::pos2(px, rect.top()), egui::pos2(px, rect.bottom())],
            egui::Stroke::new(2.0_f32, palette.error),
        );
    }
}
