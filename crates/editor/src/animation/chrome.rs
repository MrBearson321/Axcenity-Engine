use super::*;

impl AnimationStudio {
    /// Whether the Animation tab takes K, Delete and Ctrl+C/X/V this frame:
    /// a property clip is shown and the mouse is over it, as drawn last
    /// pass. The app leaves those keys alone then.
    pub fn wants_keys(&self, ctx: &egui::Context) -> bool {
        let Some((rect, pass)) = self.clip.drawn else {
            return false;
        };
        let keyed = match &self.clip.doc {
            Some(Clip::Property(_)) => true,
            Some(Clip::Take(_)) => self.clip.bones.selected.is_some(),
            None => false,
        };
        self.view == View::Clip
            && keyed
            && ctx.cumulative_pass_nr() <= pass + 1
            && ctx
                .input(|input| input.pointer.latest_pos())
                .is_some_and(|pointer| rect.contains(pointer))
            && !ctx.wants_keyboard_input()
    }

    /// Draws the Animation tab. `selected` is the Explorer's primary
    /// selection, which the tab follows (see [`AnimationStudio::follow`]).
    #[allow(clippy::too_many_arguments)] // Each is a different input to the tab.
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        project: Option<&Path>,
        assets: &Assets,
        render_state: Option<&RenderState>,
        selected: Option<InstanceId>,
        world: &mut World,
        stylesheets: &[axcenity_ui::Stylesheet],
        output: &mut Output,
    ) -> Vec<StudioAction> {
        let mut actions = Vec::new();
        let Some(project) = project else {
            ui.weak("Open a project to animate its models, Parts and UI.");
            return actions;
        };
        self.rescan_if_due(project);
        self.follow(project, assets, world, selected, output, &mut actions);
        let subject = self.subject.and_then(|id| Subject::of(world, id));
        if let Some(subject) = &subject {
            self.read_outline(project, subject);
        }
        // Show what's open if the view's own document isn't.
        self.view = match self.view {
            View::Clip if self.clip.doc.is_none() && self.machine.doc.is_some() => {
                View::StateMachine
            }
            View::StateMachine if self.machine.doc.is_none() => View::Clip,
            view => view,
        };
        if self.view == View::StateMachine {
            self.clip.drawn = None;
        }

        let narrow = ui.available_width() < OUTLINE_BELOW;
        let (mut go, pick, new) = egui::TopBottomPanel::top("animation breadcrumb")
            .show_inside(ui, |ui| self.breadcrumb(ui, subject.as_ref(), narrow))
            .inner;
        if !narrow && let Some(subject) = &subject {
            let room = ui.available_width();
            let clicked = egui::SidePanel::left("animation outline")
                .resizable(true)
                .default_width(OUTLINE_WIDTH)
                .max_width(room * 0.35)
                .show_inside(ui, |ui| {
                    egui::ScrollArea::vertical()
                        .id_salt("animation outline scroll")
                        .show(ui, |ui| self.outline(ui, subject))
                        .inner
                })
                .inner;
            go = go.or(clicked);
        }
        if let Some(pick) = pick {
            self.act(pick, project, assets, world, output, &mut actions);
        }
        match new {
            Some(New::Clip) => self.new_clip_dialog(assets),
            Some(New::PropertyClip) => self.new_property_clip_dialog(subject.as_ref()),
            Some(New::StateMachine) => {
                self.dialog = Some(new_controller_dialog(subject.as_ref()));
            }
            None => {}
        }

        // What Animate would animate when nothing is open.
        let animatable = subject
            .as_ref()
            .map(|s| (s.object, s.name.clone()))
            .or_else(|| {
                let id = selected.and_then(|id| animatable_of(world, id))?;
                Some((id, world.get(id)?.name().to_owned()))
            });
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show_inside(ui, |ui| {
                let open = match self.view {
                    View::Clip => self.clip.doc.is_some(),
                    View::StateMachine => self.machine.doc.is_some(),
                };
                if !open {
                    egui::ScrollArea::vertical()
                        .id_salt("animation empty")
                        .show(ui, |ui| {
                            if let Some(asked) = empty_state(ui, animatable.as_ref()) {
                                go = Some(asked);
                            }
                        });
                    return;
                }
                match self.view {
                    View::Clip => {
                        let root = match &subject {
                            Some(subject) => Root::Followed(subject.object),
                            None => Root::Selected,
                        };
                        let select = selected.and_then(|id| animatable_of(world, id));
                        self.clip_body(
                            ui,
                            project,
                            assets,
                            render_state,
                            world,
                            (root, select),
                            stylesheets,
                            output,
                            &mut actions,
                        );
                    }
                    View::StateMachine => {
                        if let Some(asked) = self.machine_body(
                            ui,
                            project,
                            assets,
                            render_state,
                            world,
                            output,
                            &mut actions,
                        ) {
                            go = Some(asked);
                        }
                    }
                }
            });
        if let Some(go) = go {
            // "Animate" with nothing followed animates what's selected.
            if go == Go::Animate && self.subject.is_none() {
                if let Some((object, _)) = animatable {
                    actions.push(StudioAction::Animate(object));
                }
            } else {
                self.go(go, project, assets, world, output, &mut actions);
            }
            ui.ctx().request_repaint();
        }
        self.finish(
            ui,
            project,
            assets,
            subject.as_ref(),
            world,
            output,
            &mut actions,
        );
        actions
    }

    /// The breadcrumb, then the library picker and New. When `narrow`, the
    /// first crumb drops down the outline. Returns where a crumb or the
    /// outline went, what was picked, and what New was asked to make.
    pub(super) fn breadcrumb(
        &mut self,
        ui: &mut egui::Ui,
        subject: Option<&Subject>,
        narrow: bool,
    ) -> (Option<Go>, Option<Pick>, Option<New>) {
        let crumbs = self.crumbs(subject);
        let mut go = None;
        let mut new = None;
        let pick = responsive::toolbar(ui, |ui| {
            let visuals = ui.visuals().clone();
            let palette = theme::palette(ui.ctx());
            for (index, crumb) in crumbs.iter().enumerate() {
                if index > 0 {
                    ui.label(egui::RichText::new(icons::CARET_RIGHT).color(palette.text_muted));
                }
                let last = index + 1 == crumbs.len();
                let text = egui::RichText::new(&crumb.text);
                let text = if last { text.strong() } else { text };
                if index == 0
                    && narrow
                    && let Some(subject) = subject
                {
                    // The outline, folded into the first crumb.
                    let icon = crumb
                        .kind
                        .map_or_else(|| egui::RichText::new(""), |kind| kind.rich_icon(&visuals));
                    let menu = ui.menu_button((icon, text, icons::CARET_DOWN), |ui| {
                        ui.set_min_width(220.0);
                        let clicked = self.outline(ui, subject);
                        if clicked.is_some() {
                            ui.close();
                        }
                        clicked
                    });
                    if let Some(Some(clicked)) = menu.inner {
                        go = Some(clicked);
                    }
                    menu.response.on_hover_text("What its Animator has");
                    continue;
                }
                if let Some(kind) = crumb.kind {
                    ui.label(kind.rich_icon(&visuals));
                }
                match &crumb.go {
                    Some(target) => {
                        if ui
                            .add(egui::Button::new(text).frame(false))
                            .on_hover_text("Go back up to it")
                            .clicked()
                        {
                            go = Some(target.clone());
                        }
                    }
                    None => {
                        ui.add(egui::Label::new(text).truncate());
                    }
                }
            }
            if let Some(path) = self.clip_path().filter(|_| self.view == View::Clip) {
                self.badge(&path).show(ui);
            }
            ui.separator();
            let open = match self.view {
                View::Clip => self.clip_path(),
                View::StateMachine => self.controller_path(),
            };
            let mut paths = self.files.controllers.clone();
            paths.extend(self.files.clips.iter().cloned());
            let pick = picker(ui, &mut self.search, &paths, open.as_deref());
            ui.menu_button(format!("{} New", icons::PLUS), |ui| {
                let visuals = ui.visuals().clone();
                let item =
                    |kind: AnimKind, text: &str| (kind.rich_icon(&visuals), format!("  {text}"));
                if ui
                    .button(item(AnimKind::Clip, "Keyed clip…"))
                    .on_hover_text(
                        "Make a clip that keys Parts, Models and UI elements (doors, props, menus)",
                    )
                    .clicked()
                {
                    new = Some(New::PropertyClip);
                    ui.close();
                }
                if ui
                    .button(item(AnimKind::Clip, "Clip from a take…"))
                    .on_hover_text("Make a clip from a take of a model in the project")
                    .clicked()
                {
                    new = Some(New::Clip);
                    ui.close();
                }
                if ui
                    .button(item(AnimKind::StateMachine, "State machine…"))
                    .on_hover_text("Make a state machine: states, transitions and parameters")
                    .clicked()
                {
                    new = Some(New::StateMachine);
                    ui.close();
                }
            });
            pick
        });
        (go, pick, new)
    }

    /// The outline of what the followed object's Animator has. Returns
    /// where a click went.
    pub(super) fn outline(&self, ui: &mut egui::Ui, subject: &Subject) -> Option<Go> {
        let mut go = None;
        let visuals = ui.visuals().clone();
        let row = |ui: &mut egui::Ui, kind: AnimKind, text: &str, selected: bool| {
            ui.add(
                egui::Button::selectable(selected, (kind.rich_icon(&visuals), format!("  {text}")))
                    .truncate(),
            )
        };
        ui.add(egui::Label::new(egui::RichText::new(&subject.name).strong()).truncate());
        let Some(_) = subject.animator else {
            ui.weak("No Animator yet.");
            if ui
                .button((
                    AnimKind::Animator.rich_icon(&visuals),
                    format!("  Animate {}", subject.name),
                ))
                .on_hover_text("Give it an Animator, a clip and an Animation playing it")
                .clicked()
            {
                go = Some(Go::Animate);
            }
            return go;
        };
        ui.horizontal(|ui| {
            ui.label(AnimKind::Animator.rich_icon(&visuals));
            ui.weak(AnimKind::Animator.label());
        });
        let clip = self.clip_path();
        let showing_clip = self.view == View::Clip;
        if !subject.state_machine.is_empty() {
            let open = self.view == View::StateMachine
                && self.controller_path().as_deref() == Some(subject.state_machine.as_str());
            if row(
                ui,
                AnimKind::StateMachine,
                AnimKind::StateMachine.label(),
                open,
            )
            .on_hover_text(&subject.state_machine)
            .clicked()
            {
                go = Some(Go::StateMachine);
            }
            ui.indent("outline states", |ui| {
                for (state, clips) in self.states_of(&subject.state_machine) {
                    let first = clips.first().cloned();
                    let open = showing_clip
                        && first.is_some()
                        && clip == first
                        && self.from_state.as_deref() == Some(state.as_str());
                    let response = row(ui, AnimKind::Clip, &state, open);
                    match &first {
                        Some(path) => {
                            let response = response.on_hover_text(path);
                            self.badge(path).show(ui);
                            if response.clicked() {
                                go = Some(Go::State(state.clone(), path.clone()));
                            }
                        }
                        None => {
                            response.on_hover_text("This state plays no clip");
                        }
                    }
                }
            });
        }
        ui.add_space(4.0);
        ui.weak("Animations");
        for animation in &subject.animations {
            let open = showing_clip
                && !animation.clip.is_empty()
                && clip.as_deref() == Some(animation.clip.as_str())
                && self.from_state.is_none();
            let response = row(ui, AnimKind::Animation, &animation.name, open);
            if animation.clip.is_empty() {
                response.on_hover_text("It has no Clip: pick one in Properties");
                continue;
            }
            let response = response.on_hover_text(&animation.clip);
            ui.indent(("outline animation", animation.id), |ui| {
                self.badge(&animation.clip).show(ui);
            });
            if response.clicked() {
                go = Some(Go::Animation(animation.clip.clone()));
            }
        }
        if subject.animations.is_empty() {
            ui.weak("None yet.");
        }
        if ui
            .button(format!("{} New Animation", icons::PLUS))
            .on_hover_text("Make another clip for it, and an Animation playing it")
            .clicked()
        {
            go = Some(Go::Animate);
        }
        go
    }
}
