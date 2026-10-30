use super::ribbon_menus::{insert_hint, insert_items, insert_menu};
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
        if let Some(action) = clicks.solid {
            self.solid_action(action);
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
            let key = match tool {
                Tool::Select => Some(egui::Key::Escape),
                _ => tool.key(),
            };
            let hover = match key {
                Some(key) if *tool == Tool::Select => format!("Select tool ({})", key.name()),
                Some(key) => format!(
                    "{} tool ({}). Click again to go back to Select.",
                    tool.label(),
                    key.name()
                ),
                None => format!("{} tool", tool.label()),
            };
            let button = RibbonButton::new(theme::tool_icon(*tool), tool.label())
                .selected(self.tool == *tool);
            if ui.add(button).on_hover_text(hover).clicked() {
                self.toggle_tool(*tool);
            }
        }
    }

    /// Picks `tool`, or, if it's already picked, goes back to Select: a
    /// tool's button (or key) turns it off again, as well as on.
    pub(super) fn toggle_tool(&mut self, tool: Tool) {
        self.tool = if self.tool == tool {
            Tool::Select
        } else {
            tool
        };
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
            RibbonGroup::Clients => ribbon::group(ui, "Clients", |ui| {
                self.play_settings_controls(ui, playing);
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
            RibbonGroup::Plugins => self.plugin_ribbon_groups(ui),
            RibbonGroup::Tools => ribbon::group(ui, "Tools", |ui| {
                self.tool_buttons(ui, &Tool::TRANSFORM);
            }),
            RibbonGroup::Build => ribbon::group(ui, "Build", |ui| {
                self.tool_buttons(ui, &Tool::BUILD);
            }),
            RibbonGroup::Snapping => ribbon::group(ui, "Snapping", |ui| {
                let snap = &mut self.snap;
                let mut open_groups = false;
                egui::Grid::new("snapping")
                    .num_columns(4)
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
                        ui.checkbox(
                            &mut snap.surfaces,
                            theme::with_icon(icons::MAGNET, "Surfaces"),
                        )
                        .on_hover_text(
                            "Moved and resized parts snap flush against, or in line with, the \
                             faces of parts near them.",
                        );
                        ui.checkbox(
                            &mut snap.join,
                            theme::with_icon(icons::LINK_SIMPLE, "Join Surfaces"),
                        )
                        .on_hover_text(
                            "When you let go of a part you moved, weld it to the parts it's \
                             touching, so they move as one in the game.",
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
                        ui.checkbox(
                            &mut snap.collisions,
                            theme::with_icon(icons::CUBE, "Collisions"),
                        )
                        .on_hover_text(
                            "Moved, resized and turned parts stop at solid parts (CanCollide) \
                             instead of going into them.",
                        );
                        open_groups = ui
                            .button(theme::with_icon(icons::INTERSECT, "Collision Groups…"))
                            .on_hover_text(
                                "Name collision groups and choose which of them collide: parts \
                                 in groups that don't pass through each other.",
                            )
                            .clicked();
                        ui.end_row();
                    });
                if open_groups {
                    self.collision_groups.open = true;
                }
            }),
            RibbonGroup::Parts => ribbon::group(ui, "Parts", |ui| {
                for what in Insertable::PARTS {
                    let button = RibbonButton::new(theme::insert_icon(what), what.label());
                    if ui
                        .add_enabled(!playing, button)
                        .on_hover_text(insert_hint(what))
                        .on_disabled_hover_text("Press Stop to edit.")
                        .clicked()
                    {
                        clicks.insert = Some(what);
                    }
                }
            }),
            RibbonGroup::Effects => ribbon::group(ui, "Effects", |ui| {
                insert_menu(
                    ui,
                    (icons::SPEAKER_HIGH, "Sound"),
                    "Insert a Sound, a SoundGroup or a Reverb Zone.",
                    &[&Insertable::AUDIO],
                    playing,
                    &mut clicks.insert,
                );
                let particles = Insertable::ParticleEmitter;
                let button = RibbonButton::new(theme::insert_icon(particles), "Particles");
                if ui
                    .add_enabled(!playing, button)
                    .on_hover_text(insert_hint(particles))
                    .on_disabled_hover_text("Press Stop to edit.")
                    .clicked()
                {
                    clicks.insert = Some(particles);
                }
                insert_menu(
                    ui,
                    (icons::APERTURE, "Post FX"),
                    "Insert a lens effect over the whole picture, or a Graphics Volume that \
                     changes the look inside it.",
                    &[&Insertable::LENS, &Insertable::VOLUMES],
                    playing,
                    &mut clicks.insert,
                );
            }),
            RibbonGroup::Lights => ribbon::group(ui, "Lights & Sky", |ui| {
                let button = ui
                    .add_enabled(
                        !playing,
                        RibbonButton::new(icons::LIGHTBULB, "Light").dropdown(),
                    )
                    .on_hover_text("Insert a light or a probe, or bake the probes.")
                    .on_disabled_hover_text("Press Stop to edit.");
                egui::Popup::menu(&button).show(|ui| {
                    insert_items(
                        ui,
                        &[&Insertable::LIGHTS, &Insertable::PROBES],
                        &mut clicks.insert,
                    );
                    ui.separator();
                    if ui
                        .button(theme::with_icon(icons::CAMERA_ROTATE, "Bake Probes"))
                        .on_hover_text(
                            "Bake every LightProbeVolume and ReflectionProbe: the light around \
                             each probe is saved in assets/probes. Bake again after changing \
                             the world: an out-of-date volume has a warning in the Explorer.",
                        )
                        .clicked()
                    {
                        clicks.bake_probes = true;
                    }
                });
                insert_menu(
                    ui,
                    (icons::CLOUD_SUN, "Sky"),
                    "Insert an Atmosphere, Clouds, Weather, outlines or color correction.",
                    &[&Insertable::SKY],
                    playing,
                    &mut clicks.insert,
                );
            }),
            RibbonGroup::Gameplay => ribbon::group(ui, "Gameplay", |ui| {
                insert_menu(
                    ui,
                    (icons::PERSON_SIMPLE, "Character"),
                    "Insert a ready character, or the pieces of one: a Humanoid, a controller, \
                     Health, Tools and an Inventory, or an agent that walks it.",
                    &[
                        &Insertable::HUMANOIDS,
                        &Insertable::CHARACTERS,
                        &Insertable::HEALTH,
                        &Insertable::TOOLS,
                        &Insertable::AGENTS,
                    ],
                    playing,
                    &mut clicks.insert,
                );
                insert_menu(
                    ui,
                    (icons::HAND_POINTING, "Interact"),
                    "Insert a Prompt, a Click Detector or a Seat.",
                    &[&Insertable::INTERACTION, &Insertable::SEATS],
                    playing,
                    &mut clicks.insert,
                );
                insert_menu(
                    ui,
                    (icons::FLAG, "Spawn"),
                    "Insert a SpawnLocation, a Team, a Checkpoint or a Camera viewpoint.",
                    &[
                        &Insertable::SPAWNING,
                        &Insertable::CHECKPOINTS,
                        &Insertable::CAMERAS,
                    ],
                    playing,
                    &mut clicks.insert,
                );
                insert_menu(
                    ui,
                    (icons::CAR, "Vehicle"),
                    "Insert a ready-to-drive car, or a Vehicle Seat and Wheels to build one.",
                    &[&Insertable::VEHICLES],
                    playing,
                    &mut clicks.insert,
                );
            }),
            RibbonGroup::Solid => ribbon::group(ui, "Solid Modeling", |ui| {
                use crate::app::solid::SolidAction;
                let parts = self.solid_parts();
                for action in SolidAction::ALL {
                    let usable = action.usable(&self.world, &parts);
                    let button = RibbonButton::new(action.icon(), action.label());
                    if ui
                        .add_enabled(!playing && usable, button)
                        .on_hover_text(action.hover())
                        .on_disabled_hover_text(if playing {
                            "Press Stop to edit."
                        } else {
                            action.needs()
                        })
                        .clicked()
                    {
                        clicks.solid = Some(action);
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
                        .on_hover_text(insert_hint(what))
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
                let screen = Insertable::ScreenGui;
                let button = RibbonButton::new(theme::insert_icon(screen), "Screen");
                if ui
                    .add_enabled(!playing, button)
                    .on_hover_text(insert_hint(screen))
                    .on_disabled_hover_text("Press Stop to edit.")
                    .clicked()
                {
                    clicks.insert = Some(screen);
                }
                insert_menu(
                    ui,
                    (icons::SQUARES_FOUR, "Layout"),
                    "Insert a Frame, a scrolling frame, pages, a table or a BillboardGui.",
                    &[
                        &[
                            Insertable::Frame,
                            Insertable::ScrollingFrame,
                            Insertable::Table,
                        ],
                        &[Insertable::PageLayout, Insertable::Page],
                        &[Insertable::BillboardGui],
                    ],
                    playing,
                    &mut clicks.insert,
                );
                insert_menu(
                    ui,
                    (icons::TEXT_T, "Elements"),
                    "Insert text, buttons, images, inputs and bars.",
                    &[
                        &[
                            Insertable::TextLabel,
                            Insertable::TextButton,
                            Insertable::ImageLabel,
                        ],
                        &[
                            Insertable::TextBox,
                            Insertable::Slider,
                            Insertable::Toggle,
                            Insertable::Dropdown,
                        ],
                        &[Insertable::ProgressBar, Insertable::HealthBar],
                    ],
                    playing,
                    &mut clicks.insert,
                );
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
                let active = self.terrain.tool;
                let icon = active.map_or(icons::PAINT_BRUSH_BROAD, TerrainTool::icon);
                let button = RibbonButton::new(icon, "Brush")
                    .selected(active.is_some())
                    .dropdown();
                let button = ui
                    .add_enabled(!playing && can_play, button)
                    .on_hover_text("Pick a terrain tool: sculpt, paint, foliage or generate.")
                    .on_disabled_hover_text(if playing {
                        "Press Stop to edit."
                    } else {
                        "Open a project first."
                    });
                egui::Popup::menu(&button).show(|ui| {
                    for tool in [
                        TerrainTool::Raise,
                        TerrainTool::Paint,
                        TerrainTool::Foliage,
                        TerrainTool::Generate,
                    ] {
                        let item = egui::Button::selectable(
                            active == Some(tool),
                            theme::with_icon(tool.icon(), tool.label()),
                        );
                        if ui.add(item).on_hover_text(tool.hint()).clicked() {
                            self.terrain.tool = Some(tool);
                            self.layout.open(Pane::Terrain);
                        }
                    }
                });
            }),
            RibbonGroup::Panels => ribbon::group(ui, "Panels", |ui| {
                for pane in [Pane::Explorer, Pane::Properties] {
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
                let more = RibbonButton::new(icons::SIDEBAR, "Panels").dropdown();
                let more = ui.add(more).on_hover_text(
                    "Show or hide any panel, or put them all back where they start.",
                );
                egui::Popup::menu(&more)
                    .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
                    .show(|ui| {
                        for pane in Pane::CLOSABLE {
                            let item = egui::Button::selectable(
                                self.layout.is_open(pane),
                                theme::with_icon(pane.icon(), pane.label()),
                            );
                            if ui.add(item).clicked() {
                                self.layout.toggle(pane);
                            }
                        }
                        ui.separator();
                        if ui
                            .button(theme::with_icon(
                                icons::ARROW_COUNTER_CLOCKWISE,
                                "Reset Layout",
                            ))
                            .on_hover_text("Put every panel back where it starts.")
                            .clicked()
                        {
                            self.layout.reset();
                            ui.close();
                        }
                    });
            }),
            RibbonGroup::Overlays => ribbon::group(ui, "View", |ui| {
                let cube =
                    RibbonButton::new(icons::CUBE_FOCUS, "View Cube").selected(self.view_cube);
                if ui
                    .add(cube)
                    .on_hover_text(
                        "Show or hide the view cube in the viewport's corner: click a face, \
                         edge or corner to look from there, or the house for the starting view.",
                    )
                    .clicked()
                {
                    self.view_cube = !self.view_cube;
                }
                let any = self.overlays.any() || self.light_cones;
                let button = RibbonButton::new(icons::STACK_SIMPLE, "Overlays")
                    .selected(any)
                    .dropdown();
                let button = ui.add(button).on_hover_text(
                    "Debug drawings over the viewport: colliders, raycasts, the nav mesh and more.",
                );
                egui::Popup::menu(&button)
                    .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
                    .show(|ui| self.overlay_items(ui));
                let mode = RibbonButton::new(icons::EYE, "View Mode")
                    .selected(self.view_mode != ViewMode::Lit)
                    .dropdown();
                let mode = ui
                    .add(mode)
                    .on_hover_text("Show the scene lit, or one of the renderer's debug views.");
                egui::Popup::menu(&mode).show(|ui| {
                    for mode in ViewMode::all() {
                        let item = egui::Button::selectable(self.view_mode == mode, mode.label());
                        if ui.add(item).on_hover_text(mode.hint()).clicked() {
                            self.view_mode = mode;
                        }
                    }
                });
            }),
        }
    }
}

impl EditorApp {
    /// The overlay toggles, for the ribbon's Overlays menu.
    fn overlay_items(&mut self, ui: &mut egui::Ui) {
        let overlays = &mut self.overlays;
        let items: [(&mut bool, &'static str, &'static str, &'static str); 8] = [
            (
                &mut overlays.colliders,
                icons::CUBE,
                "Colliders",
                "Draw the shape each part actually collides with, including sensors and terrain chunks.",
            ),
            (
                &mut overlays.raycasts,
                icons::LINE_SEGMENT,
                "Raycasts",
                "Draw recent workspace:Raycast calls during Play.",
            ),
            (
                &mut overlays.bounds,
                icons::SQUARE,
                "Bounds",
                "Draw each part's Size box.",
            ),
            (
                &mut overlays.emitters,
                icons::SPEAKER_HIGH,
                "Emitters",
                "Mark where each spatial Sound is heard from.",
            ),
            (
                &mut overlays.nav_mesh,
                icons::FOOTPRINTS,
                "Nav Mesh",
                "Show where characters can walk. Each colour is ground that connects; it rebuilds after you move parts or sculpt terrain.",
            ),
            (
                &mut overlays.agent_paths,
                icons::PATH,
                "Agent Paths",
                "During Play, draw the path each NavigationAgent is walking, from its feet to its Destination.",
            ),
            (
                &mut overlays.constraints,
                icons::LINK_SIMPLE,
                "Constraints",
                "Draw each WeldConstraint as a line between its parts (grey when disabled) and every Attachment as a small gizmo.",
            ),
            (
                &mut overlays.culling,
                icons::FRAME_CORNERS,
                "Culling",
                "Hold what's drawn to where the camera is now, then fly away to see it: each part's bounds in green where it's drawn and red where it's culled, and the view it was held at.",
            ),
        ];
        for (on, icon, label, hint) in items {
            overlay_menu_item(ui, on, icon, label, hint);
        }
        ui.separator();
        overlay_menu_item(
            ui,
            &mut self.light_cones,
            icons::FLASHLIGHT,
            "Light Cones",
            "Show where every light shines. Off, only the selected light (or the part it's in) shows its cone.",
        );
    }

    /// Beside Play and Stop (Phases 64 and 67): how many clients Play
    /// runs, the network emulated between them, and, while several play,
    /// which one the viewport shows.
    fn play_settings_controls(&mut self, ui: &mut egui::Ui, playing: bool) {
        ui.vertical(|ui| {
            ui.horizontal(|ui| {
                ui.label("Clients");
                let mut clients = self.play_settings.clients;
                ui.add_enabled(
                    !playing,
                    egui::DragValue::new(&mut clients).range(1..=crate::play::multi::MAX_CLIENTS),
                )
                .on_hover_text(
                    "How many players Play runs, each a client with its own world. The first \
                     plays on the server; the others join it, as players on other computers do.",
                )
                .on_disabled_hover_text("Stop the game to change how many clients play.");
                self.play_settings.clients = clients;
            });
            ui.menu_button(theme::with_icon(icons::WIFI_HIGH, "Network"), |ui| {
                let mut conditions = self.play_settings.conditions;
                let mut latency = conditions.latency.as_secs_f64() * 1000.0;
                let mut jitter = conditions.jitter.as_secs_f64() * 1000.0;
                let mut loss = conditions.loss * 100.0;
                crate::responsive::form(ui, "play network", |ui| {
                    ui.label("Latency");
                    ui.add(
                        egui::DragValue::new(&mut latency)
                            .range(0.0..=1000.0)
                            .suffix(" ms"),
                    )
                    .on_hover_text("Added to every packet each way between the clients.");
                    ui.end_row();
                    ui.label("Jitter");
                    ui.add(
                        egui::DragValue::new(&mut jitter)
                            .range(0.0..=500.0)
                            .suffix(" ms"),
                    )
                    .on_hover_text("Each packet is up to this much earlier or later.");
                    ui.end_row();
                    ui.label("Packet loss");
                    ui.add(
                        egui::DragValue::new(&mut loss)
                            .range(0.0..=50.0)
                            .suffix(" %"),
                    )
                    .on_hover_text("This share of packets never arrives.");
                    ui.end_row();
                });
                ui.weak("Between Play's clients, with 2 or more.");
                conditions.latency = std::time::Duration::from_secs_f64(latency / 1000.0);
                conditions.jitter = std::time::Duration::from_secs_f64(jitter / 1000.0);
                conditions.loss = loss / 100.0;
                if conditions != self.play_settings.conditions {
                    self.play_settings.conditions = conditions;
                    if let Some(play) = &mut self.play {
                        play.emulate(conditions);
                    }
                }
            });
            if let Some(play) = &mut self.play
                && play.clients() > 1
            {
                let views = play.views();
                let shown = play.view();
                let name = views
                    .iter()
                    .find(|(view, _)| *view == shown)
                    .map(|(_, name)| name.clone())
                    .unwrap_or_default();
                egui::ComboBox::from_id_salt("play view")
                    .selected_text(name)
                    .show_ui(ui, |ui| {
                        for (view, name) in views {
                            if ui.selectable_label(view == shown, name).clicked() {
                                play.set_view(view);
                            }
                        }
                    })
                    .response
                    .on_hover_text(
                        "Which device the viewport, Explorer and Properties show: the server, \
                         from the fly camera, or a client's game, which gets the keys and mouse.",
                    );
            }
        });
    }
}
