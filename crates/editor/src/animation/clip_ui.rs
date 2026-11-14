use super::*;

impl AnimationStudio {
    /// The open clip: its toolbar, preview, drawer, timeline or dope sheet,
    /// and settings. `root` says what a property clip's preview shows, with
    /// the object the Explorer selection stands for.
    #[allow(clippy::too_many_arguments)] // Each is a different input to the tab.
    pub(super) fn clip_body(
        &mut self,
        ui: &mut egui::Ui,
        project: &Path,
        assets: &Assets,
        render_state: Option<&RenderState>,
        world: &mut World,
        (root, select): (Root, Option<InstanceId>),
        stylesheets: &[axcenity_ui::Stylesheet],
        output: &mut Output,
        actions: &mut Vec<StudioAction>,
    ) {
        // The transport's keys first, before any widget of the clip's can
        // take Space or an arrow.
        if self.wants_transport_keys(ui.ctx()) {
            let commands = ui.input_mut(transport::keys);
            if let Some(doc) = self.clip.playback() {
                for command in commands {
                    transport::apply(doc, command);
                }
            }
        }
        self.clip.drawn = Some((ui.max_rect(), ui.ctx().cumulative_pass_nr()));
        match root {
            Root::Followed(object) => self.clip.objects.follow_within(world, object, select),
            Root::Selected => self.clip.objects.follow(world, select),
        }
        let stacked = ui.available_width() < STACK_BELOW;
        let pick = egui::Panel::top("clip toolbar")
            .show(ui, |ui| self.clip_toolbar(ui, world, stacked, root))
            .inner;
        if let Some(pick) = pick {
            self.act(pick, project, assets, world, output, actions);
        }

        let dt = f64::from(ui.input(|input| input.unstable_dt)).min(MAX_FRAME_SECONDS)
            * self.clip.rate.0;
        let playing = match &mut self.clip.doc {
            Some(Clip::Take(doc)) => {
                doc.tick(dt);
                doc.playing
            }
            Some(Clip::Property(doc)) => {
                doc.tick(dt);
                doc.playing
            }
            None => false,
        };
        if playing {
            ui.ctx().request_repaint();
        }

        // A property clip poses its copy at the playhead (with moves not
        // yet keyed on top), before anything is drawn.
        let mut problem = None;
        let keys = self.wants_keys(ui.ctx());
        if let Some(Clip::Take(doc)) = &mut self.clip.doc
            && keys
            && let Some(bone) = self.clip.bones.selected
            && ui.input(|input| {
                crate::shortcuts::pressed(input, crate::shortcuts::Action::KeySelected)
            })
            && let Err(problem) = doc.key_selected(assets, bone)
        {
            output.warning(problem);
        }
        if let Some(Clip::Property(doc)) = &mut self.clip.doc
            && keys
            && doc.shortcuts(ui)
            && let Some(focus) = self.clip.objects.focus()
        {
            doc.key_focus(&focus);
        }
        if let Err(err) = self.pose_copy(assets) {
            problem = Some(err);
        }

        let mut rename = false;
        let mut convert = false;
        let mut events = Vec::new();
        if stacked {
            egui::CentralPanel::default().show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .id_salt("clip stacked")
                    .auto_shrink(false)
                    .show(ui, |ui| {
                        let height = (ui.available_width() * 0.75).clamp(160.0, 320.0);
                        let shown = ui.scope(|ui| {
                            self.clip_preview(
                                ui,
                                assets,
                                render_state,
                                stylesheets,
                                problem.as_deref(),
                                height,
                                output,
                            )
                        });
                        self.recording_frame(ui, shown.response.rect);
                        events.extend(shown.inner);
                        egui::CollapsingHeader::new(self.drawer_title())
                            .id_salt("clip drawer stacked")
                            .icon(widgets::chevron)
                            .show(ui, |ui| events.extend(self.clip_drawer(ui, assets)));
                        ui.separator();
                        convert |= self.clip_timeline(ui, assets);
                        ui.separator();
                        egui::CollapsingHeader::new(format!("{} Settings", icons::SLIDERS))
                            .id_salt("clip inspector stacked")
                            .default_open(true)
                            .icon(widgets::chevron)
                            .show(ui, |ui| {
                                rename = self.clip_inspector(ui, assets, project, output)
                            });
                    });
            });
        } else {
            // The dope sheet (a linked clip's under its timeline) grows
            // with its tracks, so it can be resized.
            egui::Panel::bottom("clip dope sheet")
                .resizable(true)
                .default_size(280.0)
                .min_size(80.0)
                .show(ui, |ui| {
                    egui::ScrollArea::vertical()
                        .id_salt("clip timeline scroll")
                        .show(ui, |ui| convert |= self.clip_timeline(ui, assets))
                });
            if !self.clip.hide_inspector {
                let room = ui.available_width();
                egui::Panel::right("clip inspector")
                    .resizable(true)
                    .default_size(260.0)
                    .max_size(room * 0.45)
                    .show(ui, |ui| {
                        egui::ScrollArea::vertical()
                            .id_salt("clip inspector scroll")
                            .show(ui, |ui| {
                                rename = self.clip_inspector(ui, assets, project, output)
                            })
                    });
            }
            if !self.clip.hide_drawer {
                let room = ui.available_width();
                egui::Panel::left("clip drawer")
                    .resizable(true)
                    .default_size(170.0)
                    .max_size(room * 0.4)
                    .show(ui, |ui| {
                        ui.strong(self.drawer_title());
                        egui::ScrollArea::vertical()
                            .id_salt("clip drawer scroll")
                            .show(ui, |ui| events.extend(self.clip_drawer(ui, assets)))
                    });
            }
            egui::CentralPanel::default().show(ui, |ui| {
                let height = ui.available_height();
                events.extend(self.clip_preview(
                    ui,
                    assets,
                    render_state,
                    stylesheets,
                    problem.as_deref(),
                    height,
                    output,
                ));
                self.recording_frame(ui, ui.max_rect());
            });
        }
        if convert && let Some(path) = self.clip_path() {
            self.act(
                Pick::ConvertToKeys(path),
                project,
                assets,
                world,
                output,
                actions,
            );
        }
        if rename && let Some(path) = self.clip_path() {
            self.dialog = Some(Dialog::Rename {
                name: axcenity_anim::files::clip_name(&path).to_owned(),
                path,
                error: None,
            });
        }

        if !events.is_empty() {
            ui.ctx().request_repaint();
        }
        self.preview_events(events);
    }

    /// Poses a property clip's copy at the playhead, with moves not yet
    /// keyed on top, and its skeleton if it's a skinned MeshPart. Fails,
    /// saying why, if the clip can't play.
    pub(super) fn pose_copy(&mut self, assets: &Assets) -> Result<(), String> {
        let Some(Clip::Property(doc)) = &mut self.clip.doc else {
            return Ok(());
        };
        let posed = doc
            .evaluated()
            .map(|evaluated| self.clip.objects.pose(&evaluated));
        let rigged = self.clip.objects.rigged(assets);
        doc.rig = rigged.as_ref().and_then(|model| model.rig.clone());
        let skeleton = rigged.map(|model| {
            let pose = doc.bone_pose(model.rig.as_deref().expect("rigged"));
            (model, pose)
        });
        self.clip.objects.pose_bones(skeleton);
        posed
    }

    /// Both previews hold and show the open clip's pins.
    pub(super) fn follow_pins(&mut self) {
        let path = self.clip_path().unwrap_or_default();
        self.clip.bones.for_clip(&path);
        self.clip.objects.bones.for_clip(&path);
    }

    /// What was done in a property clip's preview: a pick selects there; a
    /// move is keyed with Record on, or shown until the playhead moves.
    pub(super) fn preview_events(&mut self, events: Vec<PreviewEvent>) {
        if let Some(Clip::Property(doc)) = &mut self.clip.doc {
            for event in events {
                match event {
                    PreviewEvent::Select(id) => {
                        self.clip.objects.selected = id;
                        let focus = self.clip.objects.focus();
                        if let Some(track) =
                            doc.file.tracks.iter().find(|t| {
                                focus.as_ref().is_some_and(|focus| focus.target == t.target)
                            })
                        {
                            doc.selected_track = Some(track.id.clone());
                        }
                    }
                    PreviewEvent::Edit(edits) => {
                        let objects = &self.clip.objects;
                        doc.edit_from(edits, self.clip.record, |edit| objects.rest_value(edit));
                    }
                    // Said in Output where the preview is drawn.
                    PreviewEvent::Warn(_) => {}
                }
            }
        }
    }

    /// What's open's playback, the preview model ("Preview on…") or the
    /// copy shown, Record, the buttons that show or hide the drawer and the
    /// inspector, and the ⋯ menu. Returns what the ⋯ menu asked for.
    pub(super) fn clip_toolbar(
        &mut self,
        ui: &mut egui::Ui,
        world: &World,
        stacked: bool,
        root: Root,
    ) -> Option<Pick> {
        let open = self.clip_path();
        responsive::toolbar(ui, |ui| {
            let mut pick = None;
            let Some(doc) = &mut self.clip.doc else {
                return pick;
            };
            match doc {
                Clip::Take(doc) => {
                    doc.controls(ui, &self.files.models);
                    record_toggle(
                        ui,
                        &mut self.clip.record,
                        "On, turning a bone in the preview sets a key at the playhead",
                    );
                }
                Clip::Property(doc) => {
                    doc.controls(ui);
                    responsive::room_for(ui, 140.0);
                    let objects = &mut self.clip.objects;
                    let on = format!(
                        "{} Preview on {}",
                        icons::CUBE,
                        objects.root_name().unwrap_or("(select something)")
                    );
                    let hover = match root {
                        Root::Followed(_) => {
                            "The preview shows a copy of the object being animated"
                        }
                        Root::Selected => {
                            "The preview shows a copy of the Part, Model or ScreenGui selected in the Explorer"
                        }
                    };
                    ui.add(egui::Label::new(egui::RichText::new(on).weak()).truncate())
                        .on_hover_text(hover);
                    if objects.root().is_some()
                        && ui
                            .small_button(icons::ARROWS_CLOCKWISE)
                            .on_hover_text("Copy it again from the world, after editing it there")
                            .clicked()
                    {
                        objects.refresh(world);
                    }
                    record_toggle(
                        ui,
                        &mut self.clip.record,
                        "On, moving something in the preview sets a key at the playhead",
                    );
                }
            }
            ui.separator();
            if !stacked {
                let drawer = if matches!(doc, Clip::Take(_)) {
                    "Show or hide the bones"
                } else {
                    "Show or hide the objects"
                };
                if widgets::icon_button(ui, icons::SIDEBAR, drawer, !self.clip.hide_drawer)
                    .clicked()
                {
                    self.clip.hide_drawer = !self.clip.hide_drawer;
                }
                if widgets::icon_button(
                    ui,
                    icons::SLIDERS,
                    "Show or hide the settings",
                    !self.clip.hide_inspector,
                )
                .clicked()
                {
                    self.clip.hide_inspector = !self.clip.hide_inspector;
                }
            }
            let linked = matches!(doc, Clip::Take(_));
            if let Some(path) = &open {
                ui.menu_button(icons::DOTS_THREE, |ui| {
                    if linked {
                        if ui
                            .button(format!("{} Convert to keys", icons::KEY))
                            .on_hover_text(
                                "Bake the take into bone keys you can edit, and unlink the clip (one undo step)",
                            )
                            .clicked()
                        {
                            pick = Some(Pick::ConvertToKeys(path.clone()));
                            ui.close();
                        }
                        ui.separator();
                    }
                    asset_menu(ui, path, &mut pick)
                });
            }
            pick
        })
    }

    /// The drawer's heading: bones for a take clip, objects for a property clip.
    pub(super) fn drawer_title(&self) -> String {
        match &self.clip.doc {
            Some(Clip::Property(_)) => format!("{} Objects", icons::CUBE),
            _ => format!("{} Bones", icons::BONE),
        }
    }

    /// The bones, or the objects and their tracks. Picking an object
    /// selects it in the preview.
    pub(super) fn clip_drawer(
        &mut self,
        ui: &mut egui::Ui,
        assets: &Assets,
    ) -> Option<PreviewEvent> {
        match &mut self.clip.doc {
            Some(Clip::Take(doc)) => {
                doc.bones(
                    ui,
                    assets,
                    &mut self.clip.bones.selected,
                    &mut self.clip.folds,
                );
                None
            }
            Some(Clip::Property(doc)) => {
                match doc.objects(ui, &self.clip.objects, &mut self.clip.folds)? {
                    Drawn::Object(id) => {
                        self.clip.objects.select_bone(None);
                        Some(PreviewEvent::Select(Some(id)))
                    }
                    Drawn::Bone(bone) => {
                        self.clip.objects.select_bone(Some(bone));
                        Some(PreviewEvent::Select(self.clip.objects.root()))
                    }
                }
            }
            None => None,
        }
    }

    /// The open clip's preview, `height` points tall: the model posed at the
    /// playhead, or for a property clip, the copy of what it animates. A
    /// take clip's bones are posed here; a property clip's preview reports
    /// its edits.
    #[allow(clippy::too_many_arguments)] // Each is a different input to the preview.
    pub(super) fn clip_preview(
        &mut self,
        ui: &mut egui::Ui,
        assets: &Assets,
        render_state: Option<&RenderState>,
        stylesheets: &[axcenity_ui::Stylesheet],
        problem: Option<&str>,
        height: f32,
        output: &mut Output,
    ) -> Option<PreviewEvent> {
        self.follow_pins();
        match &mut self.clip.doc {
            Some(Clip::Take(doc)) => {
                show_model(&mut self.clip.preview, doc.preview_model.as_deref(), assets);
                let posing = match doc.pose(assets) {
                    Ok(posing) => Some(posing),
                    Err(problem) => {
                        ui.colored_label(ui.visuals().error_fg_color, problem);
                        None
                    }
                };
                let top = ui.cursor().top();
                if let Some(posing) = &posing
                    && self.clip.bones.selected.is_some()
                {
                    responsive::toolbar(ui, |ui| {
                        self.clip.bones.tools(ui, posing.rig(), &mut self.clip.pose)
                    });
                }
                let height = (height - (ui.cursor().top() - top))
                    .min(ui.available_height())
                    .max(80.0);
                let posed = posing
                    .as_ref()
                    .and_then(|posing| posing.model.pose(&posing.pose))
                    .map(std::sync::Arc::new);
                let bones = posing.as_ref().map(|posing| preview::Bones {
                    poser: &mut self.clip.bones,
                    settings: &self.clip.pose,
                    rig: posing.rig(),
                    pose: &posing.pose,
                    locked: &posing.locked,
                });
                let event =
                    self.clip
                        .preview
                        .show_posing(ui, render_state, assets, posed, height, bones);
                match (event, &posing) {
                    (Some(posing::BoneEvent::Pose(bones)), Some(posing)) => {
                        if let Err(problem) = doc.pose_bones(posing.rig(), &bones, self.clip.record)
                        {
                            output.warning(problem);
                        }
                    }
                    (Some(posing::BoneEvent::Warn(message)), _) => output.warning(message),
                    _ => {}
                }
                None
            }
            Some(Clip::Property(_)) => {
                let objects = &mut self.clip.objects;
                let settings = &mut self.clip.pose;
                let top = ui.cursor().top();
                responsive::toolbar(ui, |ui| {
                    objects.tools(ui, settings);
                    if let Some(problem) = problem {
                        ui.colored_label(ui.visuals().error_fg_color, problem);
                    }
                    let missing = objects.missing();
                    if !missing.is_empty() {
                        ui.colored_label(
                            theme::palette(ui.ctx()).warning,
                            format!(
                                "{} Not in this copy: {}",
                                icons::WARNING,
                                missing.join(", ")
                            ),
                        )
                        .on_hover_text("Tracks whose object or property this copy hasn't");
                    }
                });
                let used = ui.cursor().top() - top;
                let height = (height - used).min(ui.available_height()).max(80.0);
                match objects.show_ui(ui, render_state, assets, stylesheets, height, settings) {
                    Some(PreviewEvent::Warn(message)) => {
                        output.warning(message);
                        None
                    }
                    event => event,
                }
            }
            None => None,
        }
    }

    /// The transport, then the take clip's timeline with its imported keys
    /// under it, or the property clip's dope sheet. Returns true if Convert to keys was
    /// pressed.
    pub(super) fn clip_timeline(&mut self, ui: &mut egui::Ui, assets: &Assets) -> bool {
        let rate = &mut self.clip.rate;
        if let Some(doc) = self.clip.doc.as_mut().map(Clip::playback)
            && let Some(command) = transport::bar(ui, doc, rate)
        {
            transport::apply(doc, command);
        }
        self.key_status(ui, assets);
        match &mut self.clip.doc {
            Some(Clip::Take(doc)) => {
                doc.timeline(ui);
                ui.add_space(4.0);
                let convert = doc.imported(ui, assets);
                self.clip.new_keys = doc.new_keys;
                convert
            }
            Some(Clip::Property(doc)) => {
                let focus = self.clip.objects.focus();
                doc.recording = self.clip.record;
                if doc.keys(ui, focus.as_ref())
                    && let Some(focus) = &focus
                {
                    doc.key_focus(focus);
                }
                self.clip.new_keys = doc.new_keys;
                false
            }
            None => false,
        }
    }

    /// A red frame around the preview while Record is on, so it's plain
    /// that moving something there sets keys (as Unity tints its timeline).
    fn recording_frame(&self, ui: &egui::Ui, rect: egui::Rect) {
        let read_only = matches!(&self.clip.doc, Some(Clip::Property(doc)) if doc.read_only);
        if !self.clip.record || self.clip.doc.is_none() || read_only {
            return;
        }
        let color = theme::palette(ui.ctx()).error;
        ui.painter().rect_stroke(
            rect,
            4,
            egui::Stroke::new(2.0_f32, color),
            egui::StrokeKind::Inside,
        );
    }

    /// One line above the timeline saying what moving something in the
    /// preview does now: with Record on, that it sets a key (in red, like
    /// the frame around the preview); with it off and a move not keyed yet,
    /// that moving the playhead drops it, with a button that keys it. A
    /// read-only clip, or Record off with nothing moved, shows nothing.
    pub(super) fn key_status(&mut self, ui: &mut egui::Ui, assets: &Assets) {
        let palette = theme::palette(ui.ctx());
        let (playhead, unkeyed) = match &self.clip.doc {
            Some(Clip::Take(doc)) => (doc.playhead, doc.unkeyed()),
            Some(Clip::Property(doc)) if !doc.read_only => (doc.playhead, doc.unkeyed()),
            _ => return,
        };
        if self.clip.record {
            ui.label(
                egui::RichText::new(format!(
                    "{} Recording: moving something in the preview sets a key at {playhead:.2} s",
                    icons::RECORD
                ))
                .color(palette.error),
            )
            .on_hover_text("Turn Record off in the toolbar to try moves without keying them");
            return;
        }
        if !unkeyed {
            return;
        }
        let mut key = false;
        responsive::toolbar(ui, |ui| {
            ui.label(
                egui::RichText::new(format!(
                    "{} Not keyed yet: moving the playhead or pressing play drops this change.",
                    icons::WARNING
                ))
                .color(palette.warning),
            );
            key = ui
                .button(format!("{} Key it (K)", icons::KEY))
                .on_hover_text("Keep the change as a key at the playhead")
                .clicked();
        });
        if !key {
            return;
        }
        match &mut self.clip.doc {
            Some(Clip::Take(doc)) => {
                // With moves pending, the bones they turned are keyed and
                // the bone passed is unused.
                let bone = self.clip.bones.selected.unwrap_or_default();
                if let Err(problem) = doc.key_selected(assets, bone) {
                    ui.colored_label(palette.error, problem);
                }
            }
            Some(Clip::Property(doc)) => {
                if let Some(focus) = self.clip.objects.focus() {
                    doc.key_focus(&focus);
                }
            }
            None => {}
        }
    }

    /// Convert to keys (the linked clip's ⋯ menu): bakes its take into bone
    /// keys and unlinks it, as one undo step, then opens it as a keyed clip.
    pub(super) fn convert_to_keys(
        &mut self,
        project: &Path,
        assets: &Assets,
        output: &mut Output,
        actions: &mut Vec<StudioAction>,
    ) {
        actions.extend(self.write_all(project, output));
        let Some(Clip::Take(doc)) = &self.clip.doc else {
            return;
        };
        let path = doc.path.clone();
        let name = axcenity_anim::files::clip_name(&path).to_owned();
        let root_motion = doc.file.root_motion;
        let converted = doc.converted(assets).and_then(|file| {
            let before = std::fs::read_to_string(project.join(&path))
                .map_err(|err| format!("its file can't be read: {err}"))?;
            let after = file.to_json();
            axcenity_anim::files::write_file(&project.join(&path), &after)
                .map_err(|err| format!("its file can't be written: {err}"))?;
            Ok((before, after))
        });
        let (before, after) = match converted {
            Ok(files) => files,
            Err(err) => {
                output.error(format!("Could not convert {name} to keys: {err}"));
                return;
            }
        };
        remember(
            actions,
            format!("Convert {name} to keys"),
            vec![store::FileDiff {
                path: path.clone(),
                before: Some(before),
                after: Some(after),
            }],
            None,
        );
        self.badges.remove(&path);
        self.load_clip(project, assets, &path, output);
        output.info(if root_motion {
            format!(
                "Converted {name} to keys. Root motion isn't kept: the keyed clip plays in place."
            )
        } else {
            format!("Converted {name} to keys.")
        });
    }

    /// The open clip's settings. Returns true if Rename was pressed.
    pub(super) fn clip_inspector(
        &mut self,
        ui: &mut egui::Ui,
        assets: &Assets,
        project: &Path,
        output: &mut Output,
    ) -> bool {
        match &mut self.clip.doc {
            Some(Clip::Take(doc)) => {
                let rename = doc.inspector(ui);
                self.clip.retarget.show(ui, doc, assets, project, output);
                rename
            }
            Some(Clip::Property(doc)) => {
                let rename = doc.inspector(ui, self.clip.objects.root_name());
                doc.cleanup_section(ui, &assets.rig_definitions());
                rename
            }
            None => false,
        }
    }
}

/// The Record toggle: red while on.
fn record_toggle(ui: &mut egui::Ui, record: &mut bool, hover: &str) {
    let text = format!("{} Record", icons::RECORD);
    let text = if *record {
        egui::RichText::new(text).color(theme::palette(ui.ctx()).error)
    } else {
        egui::RichText::new(text)
    };
    if ui
        .selectable_label(*record, text)
        .on_hover_text(hover)
        .clicked()
    {
        *record = !*record;
    }
}
