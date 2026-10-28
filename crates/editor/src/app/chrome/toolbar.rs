use super::*;

impl EditorApp {
    /// The ribbon under the menu bar: Play and Stop, the tools, the snapping
    /// settings, then buttons that insert each primitive and each kind of script.
    /// The ribbon's tabs, on the app background like the header, over the
    /// ribbon itself, a shade lighter with a hairline above and below, so
    /// the tabs don't run into its buttons.
    pub(in crate::app) fn toolbar(&mut self, ui: &mut egui::Ui) {
        let palette = theme::palette(ui.ctx());
        ui.spacing_mut().item_spacing.y = 0.0;
        let active_tab = egui::Frame::new()
            .inner_margin(egui::Margin {
                left: 8,
                right: 8,
                top: 2,
                bottom: 0,
            })
            .show(ui, |ui| self.ribbon_tabs.tab_bar(ui))
            .inner;
        let mut clicks = RibbonClicks::default();
        let groups = self.ribbon_tabs.groups();
        let ribbon = egui::Frame::new()
            .fill(palette.panel)
            .inner_margin(egui::Margin::symmetric(8, 6))
            .show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                egui::ScrollArea::horizontal().show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 6.0;
                        for (index, &group) in groups.iter().enumerate() {
                            if index > 0 {
                                ui.add(egui::Separator::default().vertical().spacing(10.0));
                            }
                            self.ribbon_group(ui, group, &mut clicks);
                        }
                    });
                });
            })
            .response
            .rect;
        let line = egui::Stroke::new(1.0_f32, palette.border);
        let painter = ui.painter();
        painter.hline(ribbon.x_range(), ribbon.top() + 0.5, line);
        painter.hline(ribbon.x_range(), ribbon.bottom() - 0.5, line);
        // The active tab is filled like the ribbon and joins it, over the line.
        if let Some(tab) = active_tab {
            painter.rect_filled(
                egui::Rect::from_min_max(
                    egui::pos2(tab.left() + 1.0, ribbon.top() - 0.5),
                    egui::pos2(tab.right() - 1.0, ribbon.top() + 1.0),
                ),
                0,
                palette.panel,
            );
        }
        if let Some(what) = clicks.insert {
            let near = self.selected;
            self.act(Action::Insert { what, near });
        }
        if let Some(class) = clicks.insert_script {
            self.act(Action::InsertScript(class));
        }
        if clicks.insert_controls {
            self.insert_controls_ui();
        }
        if clicks.weld {
            self.weld_selected();
        }
        if let Some(class) = clicks.constraint {
            self.pick_constraint_tool(class);
        }
        if clicks.bake_probes {
            self.bake_probes_asked = true;
        }
        if clicks.import {
            self.import_with_dialog(axcenity_assets::PROJECT_TEXTURES_FOLDER);
        }
        if clicks.check_assets {
            self.run_asset_check();
        }
        if clicks.play {
            if self.paused() {
                self.continue_play();
            } else {
                self.play();
            }
        }
        if clicks.stop {
            self.stop();
        }
    }

    /// The ribbon buttons for `tools`. Clicking one picks it.
    pub(super) fn tool_buttons(&mut self, ui: &mut egui::Ui, tools: &[Tool]) {
        for tool in tools {
            let keys: Vec<String> = tool
                .shortcut()
                .map(|shortcut| ui.ctx().format_shortcut(&shortcut))
                .into_iter()
                .chain(tool.key().map(|key| key.name().to_owned()))
                .collect();
            let hover = match keys.as_slice() {
                [] => format!("{} tool", tool.label()),
                keys => format!("{} tool ({})", tool.label(), keys.join(" or ")),
            };
            let button = RibbonButton::new(theme::tool_icon(*tool), tool.label())
                .selected(self.tool == *tool);
            if ui.add(button).on_hover_text(hover).clicked() {
                self.tool = *tool;
            }
        }
    }

    /// Draws one of the ribbon's groups. Buttons that act on the whole app
    /// are noted in `clicks`, and the toolbar acts on them once it's drawn.
    pub(in crate::app) fn ribbon_group(
        &mut self,
        ui: &mut egui::Ui,
        group: RibbonGroup,
        clicks: &mut RibbonClicks,
    ) {
        let playing = self.playing();
        let paused = self.paused();
        let can_play = self.project.is_some();
        match group {
            RibbonGroup::Game => ribbon::group(ui, "Game", |ui| {
                let palette = theme::palette(ui.ctx());
                // Paused at a breakpoint, Play is Continue, as in Studio.
                let (label, hover) = if paused {
                    ("Continue", "Continue the game from the breakpoint")
                } else {
                    ("Play", "Save, then run the game here")
                };
                let play_button = RibbonButton::new(filled_icons::PLAY, label)
                    .family(theme::filled())
                    .color(palette.success);
                clicks.play = ui
                    .add_enabled((!playing || paused) && can_play, play_button)
                    .on_hover_text(format!(
                        "{hover} ({})",
                        ui.ctx().format_shortcut(&PLAY_SHORTCUT)
                    ))
                    .on_disabled_hover_text(if playing {
                        "The game is playing."
                    } else {
                        "Open a project first."
                    })
                    .clicked();
                let stop_button = RibbonButton::new(filled_icons::STOP, "Stop")
                    .family(theme::filled())
                    .color(palette.error);
                clicks.stop = ui
                    .add_enabled(playing, stop_button)
                    .on_hover_text(format!(
                        "Stop the game and go back to editing ({})",
                        ui.ctx().format_shortcut(&STOP_SHORTCUT)
                    ))
                    .clicked();
            }),
            RibbonGroup::Check => ribbon::group(ui, "Check", |ui| {
                let button = RibbonButton::new(icons::MAGNIFYING_GLASS, "Assets");
                clicks.check_assets = ui
                    .add_enabled(can_play, button)
                    .on_hover_text(
                        "Scan the open project for missing files, invalid references, \
                         import failures, and broken dependencies.",
                    )
                    .on_disabled_hover_text("Open a project first.")
                    .clicked();
            }),
            RibbonGroup::Tools => ribbon::group(ui, "Tools", |ui| {
                self.tool_buttons(ui, &Tool::TRANSFORM);
            }),
            RibbonGroup::Build => ribbon::group(ui, "Build", |ui| {
                self.tool_buttons(ui, &Tool::BUILD);
                let button = RibbonButton::new(icons::INTERSECT, "Collision Groups");
                if ui
                    .add(button)
                    .on_hover_text(
                        "Name collision groups and choose which of them collide: parts in                          groups that don't pass through each other.",
                    )
                    .clicked()
                {
                    self.collision_groups.open = true;
                }
            }),
            RibbonGroup::Snapping => ribbon::group(ui, "Snapping", |ui| {
                let snap = &mut self.snap;
                egui::Grid::new("snapping")
                    .num_columns(2)
                    .spacing([6.0, 4.0])
                    .show(ui, |ui| {
                        ui.checkbox(
                            &mut snap.move_on,
                            theme::with_icon(icons::GRID_FOUR, "Move"),
                        )
                        .on_hover_text("Move and scale in steps of this many studs.");
                        ui.add_enabled(
                            snap.move_on,
                            egui::DragValue::new(&mut snap.move_step)
                                .range(Snap::MIN_MOVE_STEP..=1000.0)
                                .speed(0.05)
                                .max_decimals(3)
                                .suffix(" studs"),
                        );
                        ui.end_row();
                        ui.checkbox(
                            &mut snap.rotate_on,
                            theme::with_icon(icons::ANGLE, "Rotate"),
                        )
                        .on_hover_text("Rotate in steps of this many degrees.");
                        ui.add_enabled(
                            snap.rotate_on,
                            egui::DragValue::new(&mut snap.rotate_step)
                                .range(Snap::MIN_ROTATE_STEP..=180.0)
                                .speed(0.5)
                                .max_decimals(2)
                                .suffix("°"),
                        );
                        ui.end_row();
                    });
            }),
            RibbonGroup::Parts => ribbon::group(ui, "Parts", |ui| {
                for what in Insertable::PARTS {
                    let hover = format!(
                        "Insert a {} part at the camera's focus point, inside the selected \
                         Model or Folder if there is one.",
                        what.label().to_lowercase()
                    );
                    let button = RibbonButton::new(theme::insert_icon(what), what.label());
                    if ui
                        .add_enabled(!playing, button)
                        .on_hover_text(hover)
                        .on_disabled_hover_text("Press Stop to edit.")
                        .clicked()
                    {
                        clicks.insert = Some(what);
                    }
                }
            }),
            RibbonGroup::Effects => ribbon::group(ui, "Effects", |ui| {
                for what in Insertable::AUDIO {
                    let button = RibbonButton::new(theme::insert_icon(what), what.label());
                    if ui
                        .add_enabled(!playing, button)
                        .on_hover_text("Insert a Sound into the selected part or the Workspace.")
                        .on_disabled_hover_text("Press Stop to edit.")
                        .clicked()
                    {
                        clicks.insert = Some(what);
                    }
                }
                for what in Insertable::EFFECTS {
                    let button = RibbonButton::new(theme::insert_icon(what), what.label());
                    if ui
                        .add_enabled(!playing, button)
                        .on_hover_text(
                            "Insert a ParticleEmitter into the selected part (an invisible \
                             holder part is made if none is selected).",
                        )
                        .on_disabled_hover_text("Press Stop to edit.")
                        .clicked()
                    {
                        clicks.insert = Some(what);
                    }
                }
                for what in Insertable::LIGHTS {
                    let button = RibbonButton::new(theme::insert_icon(what), what.label());
                    if ui
                        .add_enabled(!playing, button)
                        .on_hover_text("Insert a light into the selected part.")
                        .on_disabled_hover_text("Press Stop to edit.")
                        .clicked()
                    {
                        clicks.insert = Some(what);
                    }
                }
                for what in Insertable::PROBES {
                    let button = RibbonButton::new(theme::insert_icon(what), what.label());
                    let hover = match what {
                        Insertable::ReflectionProbe => {
                            "Insert a ReflectionProbe into the selected part: shiny things in \
                             the part's box reflect what's around them there, once baked. With \
                             no part selected, an invisible room-sized box is made for it."
                        }
                        _ => {
                            "Insert a LightProbeVolume into the selected part: a grid of probes \
                             in the part's box lights everything there, still or moving, with \
                             the light bounced around it, once baked. With no part selected, an \
                             invisible room-sized box is made for it."
                        }
                    };
                    if ui
                        .add_enabled(!playing, button)
                        .on_hover_text(hover)
                        .on_disabled_hover_text("Press Stop to edit.")
                        .clicked()
                    {
                        clicks.insert = Some(what);
                    }
                }
                let button = RibbonButton::new(icons::CAMERA_ROTATE, "Bake Probes");
                if ui
                    .add_enabled(!playing, button)
                    .on_hover_text(
                        "Bake every LightProbeVolume and ReflectionProbe: the light around \
                         each probe is saved in assets/probes. Bake again after changing the \
                         world: an out-of-date volume has a warning in the Explorer.",
                    )
                    .on_disabled_hover_text("Press Stop to edit.")
                    .clicked()
                {
                    clicks.bake_probes = true;
                }
                for what in Insertable::SKY {
                    let button = RibbonButton::new(theme::insert_icon(what), what.label());
                    let hover = if what == Insertable::Clouds {
                        "Insert Clouds into Lighting: a layer of cloud high in the sky, lit by                          the sun, whose shadows fall on the world. Workspace's GlobalWind                          carries them along."
                    } else {
                        "Insert an Atmosphere into Lighting: the sky is drawn as sunlight                          scattered by air, reddening toward sunset and darkening at night                          as Lighting's ClockTime goes round, and it colors the sun's light."
                    };
                    if ui
                        .add_enabled(!playing, button)
                        .on_hover_text(hover)
                        .on_disabled_hover_text("Press Stop to edit.")
                        .clicked()
                    {
                        clicks.insert = Some(what);
                    }
                }
                for what in Insertable::CHARACTERS {
                    let button = RibbonButton::new(theme::insert_icon(what), what.label());
                    if ui
                        .add_enabled(!playing, button)
                        .on_hover_text(format!(
                            "Insert a {} on the selected model or part. With nothing \
                             selected, or on a character that already has a controller, \
                             makes a simple character.",
                            what.class()
                        ))
                        .on_disabled_hover_text("Press Stop to edit.")
                        .clicked()
                    {
                        clicks.insert = Some(what);
                    }
                }
                for what in Insertable::HEALTH {
                    let hint = if what == Insertable::Health {
                        "Insert a Health into the selected character's Model: how much it \
                         can take. Scripts call TakeDamage and Heal, and it fires Died once \
                         at 0. Turn on FallDamage to hurt it when it lands hard."
                    } else {
                        "Insert a Damage Zone: a see-through part that takes DamagePerSecond \
                         from every Health whose Model is inside it during Play."
                    };
                    let button = RibbonButton::new(theme::insert_icon(what), what.label());
                    if ui
                        .add_enabled(!playing, button)
                        .on_hover_text(hint)
                        .on_disabled_hover_text("Press Stop to edit.")
                        .clicked()
                    {
                        clicks.insert = Some(what);
                    }
                }
                for what in Insertable::SEATS {
                    let button = RibbonButton::new(theme::insert_icon(what), what.label());
                    if ui
                        .add_enabled(!playing, button)
                        .on_hover_text(
                            "Insert a Seat: a part a character sits on when it walks into \
                             it during Play. Jump stands it up.",
                        )
                        .on_disabled_hover_text("Press Stop to edit.")
                        .clicked()
                    {
                        clicks.insert = Some(what);
                    }
                }
                for what in Insertable::INTERACTION {
                    let hint = if what == Insertable::ProximityPrompt {
                        "Insert a Prompt into the selected part or Attachment. During Play, \
                         walking up to it shows ActionText with the Interact key, and \
                         holding that for HoldDuration fires Triggered."
                    } else {
                        "Insert a Click Detector into the selected part. During Play, \
                         clicking the part fires MouseClick, and the cursor changes over it."
                    };
                    let button = RibbonButton::new(theme::insert_icon(what), what.label());
                    if ui
                        .add_enabled(!playing, button)
                        .on_hover_text(hint)
                        .on_disabled_hover_text("Press Stop to edit.")
                        .clicked()
                    {
                        clicks.insert = Some(what);
                    }
                }
                for what in Insertable::HUMANOIDS {
                    let hint = if what == Insertable::Character {
                        "Insert a Character: six welded parts with a Humanoid, a \
                         third-person controller, Health, Inventory and an Animator \
                         with Idle, Walk, Run, Jump, Fall and Sit states."
                    } else {
                        "Insert a Humanoid into the selected character's Model: \
                         Roblox-style properties and methods over its controller, \
                         Health, Inventory and Animator."
                    };
                    let button = RibbonButton::new(theme::insert_icon(what), what.label());
                    if ui
                        .add_enabled(!playing, button)
                        .on_hover_text(hint)
                        .on_disabled_hover_text("Press Stop to edit.")
                        .clicked()
                    {
                        clicks.insert = Some(what);
                    }
                }
                for what in Insertable::SPAWNING {
                    let hint = if what == Insertable::SpawnLocation {
                        "Insert a SpawnLocation: a pad players' characters spawn on during \
                         Play, and respawn on after dying. Turn Neutral off and set its \
                         TeamColor to keep it for one team."
                    } else {
                        "Insert a Team into Teams: players join the AutoAssignable team with \
                         the fewest players, and spawn on SpawnLocations of its TeamColor."
                    };
                    let button = RibbonButton::new(theme::insert_icon(what), what.label());
                    if ui
                        .add_enabled(!playing, button)
                        .on_hover_text(hint)
                        .on_disabled_hover_text("Press Stop to edit.")
                        .clicked()
                    {
                        clicks.insert = Some(what);
                    }
                }
                for what in Insertable::TOOLS {
                    let hint = if what == Insertable::Tool {
                        "Insert a Tool: a Handle part with a Grip. During Play, a character \
                         with an Inventory picks it up by touching it, number keys equip it \
                         in the right hand, and clicking fires Activated."
                    } else {
                        "Insert an Inventory into the selected character's Model: the \
                         Tools it carries, shown in the hotbar during Play."
                    };
                    let button = RibbonButton::new(theme::insert_icon(what), what.label());
                    if ui
                        .add_enabled(!playing, button)
                        .on_hover_text(hint)
                        .on_disabled_hover_text("Press Stop to edit.")
                        .clicked()
                    {
                        clicks.insert = Some(what);
                    }
                }
                for what in Insertable::VEHICLES {
                    let hint = match what {
                        Insertable::Vehicle => {
                            "Insert a ready-to-drive car. During Play, walk into its seat on \
                             the left to drive with the move keys or a stick; Jump gets out."
                        }
                        Insertable::VehicleSeat => {
                            "Insert a Vehicle Seat: weld it to a car's chassis to drive it, \
                             on wheels inserted into the chassis."
                        }
                        _ => {
                            "Insert a Wheel into the selected part: a spring and tyre that \
                             a Vehicle Seat welded to that part drives on."
                        }
                    };
                    let button = RibbonButton::new(theme::insert_icon(what), what.label());
                    if ui
                        .add_enabled(!playing, button)
                        .on_hover_text(hint)
                        .on_disabled_hover_text("Press Stop to edit.")
                        .clicked()
                    {
                        clicks.insert = Some(what);
                    }
                }
                for what in Insertable::AGENTS {
                    let button = RibbonButton::new(theme::insert_icon(what), what.label());
                    if ui
                        .add_enabled(!playing, button)
                        .on_hover_text(
                            "Insert a NavigationAgent on the selected model or part, which \
                             walks it along the navigation mesh to its Destination during Play. \
                             With nothing selected, makes a simple character for it.",
                        )
                        .on_disabled_hover_text("Press Stop to edit.")
                        .clicked()
                    {
                        clicks.insert = Some(what);
                    }
                }
                for what in Insertable::CHECKPOINTS {
                    let button = RibbonButton::new(theme::insert_icon(what), what.label());
                    if ui
                        .add_enabled(!playing, button)
                        .on_hover_text(
                            "Insert a Checkpoint on top of the selected part: a place \
                             SaveService:RestoreCheckpoint (or the \"go back to checkpoint\" \
                             block) puts the player character back at.",
                        )
                        .on_disabled_hover_text("Press Stop to edit.")
                        .clicked()
                    {
                        clicks.insert = Some(what);
                    }
                }
                for what in Insertable::CAMERAS {
                    let button = RibbonButton::new(theme::insert_icon(what), what.label());
                    if ui
                        .add_enabled(!playing, button)
                        .on_hover_text(
                            "Insert a Camera where you're looking from: a named viewpoint \
                             a LocalScript cuts to with workspace.CurrentCamera.CFrame = \
                             viewpoint.CFrame. It isn't drawn in the game.",
                        )
                        .on_disabled_hover_text("Press Stop to edit.")
                        .clicked()
                    {
                        clicks.insert = Some(what);
                    }
                }
            }),
            RibbonGroup::Constraints => ribbon::group(ui, "Constraints", |ui| {
                let can_weld =
                    !crate::constraints::weld_pairs(&self.world, &self.tool_parts()).is_empty();
                let button = RibbonButton::new(icons::LINK_SIMPLE, "Weld");
                clicks.weld = ui
                    .add_enabled(!playing && can_weld, button)
                    .on_hover_text(
                        "Weld the selected parts to the largest of them, where they are now: \
                         unanchored, they move as one body in the game.",
                    )
                    .on_disabled_hover_text(if playing {
                        "Press Stop to edit."
                    } else {
                        "Select two or more parts to weld."
                    })
                    .clicked();
                let active = self.constraint_tool.map(|tool| tool.class);
                let icon = active.map_or(icons::DOOR_OPEN, theme::class_icon);
                let button = RibbonButton::new(icon, "Constraint").selected(active.is_some());
                let button = ui
                    .add_enabled(!playing, button)
                    .on_hover_text(
                        "Join two parts with a hinge, ball socket, slider, spring, rope or rod: \
                         pick one, click a surface on one part, then a surface on another. \
                         Or push and turn a part with a mover: click a surface on it (an \
                         align then takes a surface on the part it follows). The \
                         Attachments and the constraint are one undo step. Esc stops.",
                    )
                    .on_disabled_hover_text("Press Stop to edit.");
                egui::Popup::menu(&button).show(|ui| {
                    let groups = [&crate::constraints::JOINTS[..], &crate::constraints::MOVERS];
                    for (i, classes) in groups.into_iter().enumerate() {
                        if i > 0 {
                            ui.separator();
                        }
                        for &class in classes {
                            let label = format!("{} {}", theme::class_icon(class), class);
                            if ui.selectable_label(active == Some(class), label).clicked() {
                                clicks.constraint = Some(Some(class));
                            }
                        }
                    }
                    if active.is_some() && ui.button("Stop").clicked() {
                        clicks.constraint = Some(None);
                    }
                });
                for what in Insertable::CONSTRAINTS {
                    let button = RibbonButton::new(theme::insert_icon(what), what.label());
                    if ui
                        .add_enabled(!playing, button)
                        .on_hover_text(
                            "Insert an Attachment in the selected part, at its middle: a \
                             point and direction on the part that moves with it.",
                        )
                        .on_disabled_hover_text("Press Stop to edit.")
                        .clicked()
                    {
                        clicks.insert = Some(what);
                    }
                }
            }),
            RibbonGroup::Import => ribbon::group(ui, "Import", |ui| {
                let button = RibbonButton::new(icons::FILE_ARROW_DOWN, "Import…");
                clicks.import = ui
                    .add_enabled(!playing, button)
                    .on_hover_text(IMPORT_HOVER)
                    .on_disabled_hover_text("Press Stop to edit.")
                    .clicked();
            }),
            RibbonGroup::Ui => ribbon::group(ui, "UI", |ui| {
                for what in Insertable::UI {
                    let label = match what {
                        Insertable::ScreenGui => "Screen",
                        Insertable::Frame => "Frame",
                        Insertable::TextLabel => "Text",
                        Insertable::TextButton => "Button",
                        Insertable::ImageLabel => "Image",
                        Insertable::TextBox => "TextBox",
                        Insertable::PageLayout => "Pages",
                        Insertable::Page => "Page",
                        Insertable::Table => "Table",
                        Insertable::ScrollingFrame => "Scroll",
                        Insertable::ProgressBar => "Progress",
                        Insertable::BillboardGui => "Billboard",
                        Insertable::HealthBar => "Health Bar",
                        _ => what.label(),
                    };
                    let hover = match what {
                        Insertable::ScreenGui => {
                            "Insert a ScreenGui: a layer of UI over the game.".to_owned()
                        }
                        Insertable::PageLayout => {
                            "Insert a PageLayout. Its child Frames are pages; a script or the \
                             show-page block sets CurrentPage."
                                .to_owned()
                        }
                        Insertable::Page => {
                            "Insert a page Frame into the selected PageLayout (a PageLayout \
                             and a ScreenGui are made if there isn't one)."
                                .to_owned()
                        }
                        Insertable::BillboardGui => {
                            "Insert a BillboardGui into the selected part. It floats in the \
                             world and is drawn with the screen UI."
                                .to_owned()
                        }
                        Insertable::HealthBar => {
                            "Insert health bars: one floating over the nearest character with \
                             a Health, and one at the top left of the screen. Both follow that \
                             Health with no script, styled by health-bar.css."
                                .to_owned()
                        }
                        _ => format!(
                            "Insert a {} into the selected ScreenGui or Frame (or the first \
                             ScreenGui; one is made if there's none). Drag its handles in the \
                             viewport, and style it with CSS in the Style section of Properties.",
                            what.label()
                        ),
                    };
                    let button = RibbonButton::new(theme::insert_icon(what), label);
                    if ui
                        .add_enabled(!playing, button)
                        .on_hover_text(hover)
                        .on_disabled_hover_text("Press Stop to edit.")
                        .clicked()
                    {
                        clicks.insert = Some(what);
                    }
                }
                let button = RibbonButton::new(icons::GAME_CONTROLLER, "Controls");
                clicks.insert_controls = ui
                    .add_enabled(!playing && can_play, button)
                    .on_hover_text(
                        "Insert a ready-made Controls menu: a ScreenGui listing the input \
                         map's actions, and a LocalScript that rebinds one to the next key \
                         or button pressed after its button is clicked. In the game, Menu \
                         (Escape, or Start on a controller) shows it.",
                    )
                    .on_disabled_hover_text("Open a project, and press Stop to edit.")
                    .clicked();
            }),
            RibbonGroup::Scripts => ribbon::group(ui, "Scripts", |ui| {
                for class in explorer::SCRIPT_CLASSES {
                    let button = RibbonButton::new(theme::class_icon(class), class.as_str());
                    if ui
                        .add_enabled(!playing && can_play, button)
                        .on_hover_text(explorer::script_hover(class))
                        .on_disabled_hover_text(if playing {
                            "Press Stop to edit."
                        } else {
                            "Open a project first."
                        })
                        .clicked()
                    {
                        clicks.insert_script = Some(class);
                    }
                }
            }),
            RibbonGroup::Terrain => ribbon::group(ui, "Terrain", |ui| {
                use crate::terrain_studio::TerrainTool;
                let editor = RibbonButton::new(icons::MOUNTAINS, "Terrain")
                    .selected(self.layout.is_open(Pane::Terrain));
                if ui
                    .add_enabled(can_play, editor)
                    .on_hover_text(
                        "Open the Terrain Studio: make, sculpt, paint, scatter foliage and generate terrain.",
                    )
                    .on_disabled_hover_text("Open a project first.")
                    .clicked()
                {
                    self.layout.open(Pane::Terrain);
                }
                for tool in [
                    TerrainTool::Raise,
                    TerrainTool::Paint,
                    TerrainTool::Foliage,
                    TerrainTool::Generate,
                ] {
                    let button = RibbonButton::new(tool.icon(), tool.label())
                        .selected(self.terrain.tool == Some(tool));
                    if ui
                        .add_enabled(!playing && can_play, button)
                        .on_hover_text(tool.hint())
                        .on_disabled_hover_text(if playing {
                            "Press Stop to edit."
                        } else {
                            "Open a project first."
                        })
                        .clicked()
                    {
                        self.terrain.tool = Some(tool);
                        self.layout.open(Pane::Terrain);
                    }
                }
            }),
            RibbonGroup::Panels => ribbon::group(ui, "Panels", |ui| {
                for pane in Pane::CLOSABLE {
                    let button = RibbonButton::new(pane.icon(), pane.label())
                        .selected(self.layout.is_open(pane));
                    if ui
                        .add(button)
                        .on_hover_text(format!(
                            "Show or hide the {} panel. Drag a panel by its tab to move it.",
                            pane.label()
                        ))
                        .clicked()
                    {
                        self.layout.toggle(pane);
                    }
                }
                let reset = RibbonButton::new(icons::ARROW_COUNTER_CLOCKWISE, "Reset");
                if ui
                    .add(reset)
                    .on_hover_text("Put every panel back where it starts.")
                    .clicked()
                {
                    self.layout.reset();
                }
            }),
            RibbonGroup::Overlays => ribbon::group(ui, "Overlays", |ui| {
                overlay_button(
                    ui,
                    &mut self.overlays.colliders,
                    icons::CUBE,
                    "Colliders",
                    "Draw the shape each part actually collides with, including sensors and terrain chunks.",
                );
                overlay_button(
                    ui,
                    &mut self.overlays.raycasts,
                    icons::LINE_SEGMENT,
                    "Raycasts",
                    "Draw recent workspace:Raycast calls during Play.",
                );
                overlay_button(
                    ui,
                    &mut self.overlays.bounds,
                    icons::SQUARE,
                    "Bounds",
                    "Draw each part's Size box.",
                );
                overlay_button(
                    ui,
                    &mut self.overlays.emitters,
                    icons::SPEAKER_HIGH,
                    "Emitters",
                    "Mark where each spatial Sound is heard from.",
                );
                overlay_button(
                    ui,
                    &mut self.overlays.nav_mesh,
                    icons::FOOTPRINTS,
                    "Nav Mesh",
                    "Show where characters can walk. Each colour is ground that connects; it rebuilds after you move parts or sculpt terrain.",
                );
                overlay_button(
                    ui,
                    &mut self.overlays.agent_paths,
                    icons::PATH,
                    "Agent Paths",
                    "During Play, draw the path each NavigationAgent is walking, from its feet to its Destination.",
                );
                overlay_button(
                    ui,
                    &mut self.overlays.constraints,
                    icons::LINK_SIMPLE,
                    "Constraints",
                    "Draw each WeldConstraint as a line between its parts (grey when disabled) and every Attachment as a small gizmo.",
                );
                overlay_button(
                    ui,
                    &mut self.overlays.culling,
                    icons::FRAME_CORNERS,
                    "Culling",
                    "Hold what's drawn to where the camera is now, then fly away to see it: each part's bounds in green where it's drawn and red where it's culled, and the view it was held at.",
                );
            }),
        }
    }
}
