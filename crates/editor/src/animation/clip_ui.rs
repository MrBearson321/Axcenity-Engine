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
        self.clip.drawn = Some((ui.max_rect(), ui.ctx().cumulative_pass_nr()));
        match root {
            Root::Followed(object) => self.clip.objects.follow_within(world, object, select),
            Root::Selected => self.clip.objects.follow(world, select),
        }
        let stacked = ui.available_width() < STACK_BELOW;
        let pick = egui::TopBottomPanel::top("clip toolbar")
            .show_inside(ui, |ui| self.clip_toolbar(ui, world, stacked, root))
            .inner;
        if let Some(pick) = pick {
            self.act(pick, project, assets, world, output, actions);
        }

        let dt = f64::from(ui.input(|input| input.unstable_dt)).min(MAX_FRAME_SECONDS);
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
        if let Some(Clip::Property(doc)) = &mut self.clip.doc {
            if keys
                && doc.shortcuts(ui)
                && let Some(focus) = self.clip.objects.focus()
            {
                doc.key_focus(&focus);
            }
            match doc.evaluated() {
                Ok(evaluated) => self.clip.objects.pose(&evaluated),
                Err(err) => problem = Some(err),
            }
        }

        let mut rename = false;
        let property = matches!(self.clip.doc, Some(Clip::Property(_)));
        let mut events = Vec::new();
        if stacked {
            egui::CentralPanel::default().show_inside(ui, |ui| {
                egui::ScrollArea::vertical()
                    .id_salt("clip stacked")
                    .auto_shrink(false)
                    .show(ui, |ui| {
                        let height = (ui.available_width() * 0.75).clamp(160.0, 320.0);
                        events.extend(self.clip_preview(
                            ui,
                            assets,
                            render_state,
                            stylesheets,
                            problem.as_deref(),
                            height,
                        ));
                        egui::CollapsingHeader::new(self.drawer_title())
                            .id_salt("clip drawer stacked")
                            .icon(widgets::chevron)
                            .show(ui, |ui| events.extend(self.clip_drawer(ui, assets)));
                        ui.separator();
                        self.clip_timeline(ui);
                        ui.separator();
                        egui::CollapsingHeader::new(format!("{} Settings", icons::SLIDERS))
                            .id_salt("clip inspector stacked")
                            .default_open(true)
                            .icon(widgets::chevron)
                            .show(ui, |ui| rename = self.clip_inspector(ui));
                    });
            });
        } else {
            // The dope sheet grows with its tracks, so it can be resized;
            // the timeline is one strip.
            let bottom = if property {
                egui::TopBottomPanel::bottom("clip dope sheet")
                    .resizable(true)
                    .default_height(170.0)
                    .min_height(80.0)
            } else {
                egui::TopBottomPanel::bottom("clip timeline").resizable(false)
            };
            bottom.show_inside(ui, |ui| {
                egui::ScrollArea::vertical()
                    .id_salt("clip timeline scroll")
                    .show(ui, |ui| self.clip_timeline(ui))
            });
            if !self.clip.hide_inspector {
                let room = ui.available_width();
                egui::SidePanel::right("clip inspector")
                    .resizable(true)
                    .default_width(260.0)
                    .max_width(room * 0.45)
                    .show_inside(ui, |ui| {
                        egui::ScrollArea::vertical()
                            .id_salt("clip inspector scroll")
                            .show(ui, |ui| rename = self.clip_inspector(ui))
                    });
            }
            if !self.clip.hide_drawer {
                let room = ui.available_width();
                egui::SidePanel::left("clip drawer")
                    .resizable(true)
                    .default_width(170.0)
                    .max_width(room * 0.4)
                    .show_inside(ui, |ui| {
                        ui.strong(self.drawer_title());
                        egui::ScrollArea::vertical()
                            .id_salt("clip drawer scroll")
                            .show(ui, |ui| events.extend(self.clip_drawer(ui, assets)))
                    });
            }
            egui::CentralPanel::default().show_inside(ui, |ui| {
                let height = ui.available_height();
                events.extend(self.clip_preview(
                    ui,
                    assets,
                    render_state,
                    stylesheets,
                    problem.as_deref(),
                    height,
                ));
            });
        }
        if rename && let Some(path) = self.clip_path() {
            self.dialog = Some(Dialog::Rename {
                name: axcenity_anim::files::clip_name(&path).to_owned(),
                path,
                error: None,
            });
        }

        // What was done in the preview: a pick selects there; a move is
        // keyed with Record on, or shown until the playhead moves.
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
                    PreviewEvent::Edit(edits) => doc.edit(edits, self.clip.record),
                }
                ui.ctx().request_repaint();
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
                Clip::Take(doc) => doc.controls(ui, &self.files.models),
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
                    let record = &mut self.clip.record;
                    let text = format!("{} Record", icons::RECORD);
                    let text = if *record {
                        egui::RichText::new(text).color(theme::palette(ui.ctx()).error)
                    } else {
                        egui::RichText::new(text)
                    };
                    if ui
                        .selectable_label(*record, text)
                        .on_hover_text(
                            "On, moving something in the preview sets a key at the playhead",
                        )
                        .clicked()
                    {
                        *record = !*record;
                    }
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
            if let Some(path) = &open {
                ui.menu_button(icons::DOTS_THREE, |ui| asset_menu(ui, path, &mut pick));
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
                doc.bones(ui, assets);
                None
            }
            Some(Clip::Property(doc)) => doc
                .objects(ui, &self.clip.objects)
                .map(|id| PreviewEvent::Select(Some(id))),
            None => None,
        }
    }

    /// The open clip's preview, `height` points tall: the model posed at the
    /// playhead, or for a property clip, the copy of what it animates.
    pub(super) fn clip_preview(
        &mut self,
        ui: &mut egui::Ui,
        assets: &Assets,
        render_state: Option<&RenderState>,
        stylesheets: &[axcenity_ui::Stylesheet],
        problem: Option<&str>,
        height: f32,
    ) -> Option<PreviewEvent> {
        match &mut self.clip.doc {
            Some(Clip::Take(doc)) => {
                show_model(&mut self.clip.preview, doc.preview_model.as_deref(), assets);
                let posed = match doc.pose(assets) {
                    Ok(posed) => Some(posed),
                    Err(problem) => {
                        ui.colored_label(ui.visuals().error_fg_color, problem);
                        None
                    }
                };
                let height = height.min(ui.available_height()).max(80.0);
                self.clip
                    .preview
                    .show(ui, render_state, assets, posed, height);
                None
            }
            Some(Clip::Property(_)) => {
                let objects = &mut self.clip.objects;
                let top = ui.cursor().top();
                responsive::toolbar(ui, |ui| {
                    objects.tools(ui);
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
                objects.show_ui(ui, render_state, assets, stylesheets, height)
            }
            None => None,
        }
    }

    /// The take clip's timeline, or the property clip's dope sheet.
    pub(super) fn clip_timeline(&mut self, ui: &mut egui::Ui) {
        match &mut self.clip.doc {
            Some(Clip::Take(doc)) => doc.timeline(ui),
            Some(Clip::Property(doc)) => {
                let focus = self.clip.objects.focus();
                if doc.keys(ui, focus.as_ref())
                    && let Some(focus) = &focus
                {
                    doc.key_focus(focus);
                }
            }
            None => {}
        }
    }

    /// The open clip's settings. Returns true if Rename was pressed.
    pub(super) fn clip_inspector(&mut self, ui: &mut egui::Ui) -> bool {
        match &mut self.clip.doc {
            Some(Clip::Take(doc)) => doc.inspector(ui),
            Some(Clip::Property(doc)) => doc.inspector(ui, self.clip.objects.root_name()),
            None => false,
        }
    }
}
