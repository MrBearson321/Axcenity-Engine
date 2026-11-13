//! The Settings window (File > Settings…, the start screen's sidebar, or
//! Ctrl+, / Cmd+,): appearance, typography and density, the editor layout,
//! this computer's graphics, the keyboard shortcuts, accessibility, and (in
//! a debug editor run from the source tree) rebuilding the engine.
//! Changes apply at once and are saved between runs with the other settings.

use super::*;

use crate::shortcuts;
use crate::theme::{
    Accent, Appearance, Density, MenuIcons, Palette, RibbonAlign, ThemeChoice, space, type_scale,
};
use crate::widgets;
use axcenity_render::{
    AmbientOcclusion, AntiAliasing, Capabilities, CloudQuality, DepthOfFieldQuality,
    EffectiveGraphics, GraphicsSettings, MotionBlurQuality, Quality, RAY_TRACING_DISTANCE_RANGE,
    RENDER_SCALE_RANGE, RayTracedAmbientOcclusion, RayTracedGlobalIllumination, RayTracedShadows,
    ReflectionQuality, ShadowQuality, TEXTURE_BUDGET_RANGE, TextureQuality,
};

/// The file beside eframe's saved settings that says whether the GPU is
/// opened with hardware ray tracing: the device is made before the app (and
/// its settings) exists, so the choice is kept where `main` can read it.
fn ray_tracing_file() -> Option<std::path::PathBuf> {
    eframe::storage_dir(crate::APP_ID).map(|folder| folder.join("hardware-ray-tracing.txt"))
}

/// Whether Settings > Graphics > Hardware ray tracing was on when the
/// editor last saved. Off without the file.
pub(crate) fn saved_ray_tracing() -> bool {
    ray_tracing_file()
        .and_then(|file| std::fs::read_to_string(file).ok())
        .is_some_and(|text| text.trim() == "on")
}

/// Keeps `on` for the next start ([`saved_ray_tracing`]). Never in tests.
pub(super) fn remember_ray_tracing(on: bool) {
    if cfg!(test) || saved_ray_tracing() == on {
        return;
    }
    if let Some(file) = ray_tracing_file() {
        let _ = std::fs::create_dir_all(file.parent().unwrap_or(&file));
        if let Err(err) = std::fs::write(&file, if on { "on" } else { "off" }) {
            log::warn!("couldn't keep the hardware ray tracing setting: {err}");
        }
    }
}

/// The pages of the Settings window.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum SettingsPage {
    #[default]
    Appearance,
    Typography,
    Layout,
    /// AutoRecover (`app/autorecover.rs`).
    Saving,
    /// Programs the editor opens files in: Blender (`blender.rs`).
    Tools,
    /// What the Scene view draws while editing (the menu bar's Viewport).
    Viewport,
    Graphics,
    Shortcuts,
    Accessibility,
    /// Update checks (`app/updates.rs`).
    Updates,
    /// MCP clients working in this window (`app/mcp/live.rs`).
    Ai,
    /// Name, colour and invite defaults (`app/team_panel.rs`).
    TeamCreate,
    /// Only in a debug editor run from the source tree (`engine.rs`).
    Engine,
    /// The open project's own settings, which live in its folder. Listed
    /// apart from the rest, which are the user's.
    Project,
}

impl SettingsPage {
    const ALL: [SettingsPage; 14] = [
        SettingsPage::Appearance,
        SettingsPage::Typography,
        SettingsPage::Layout,
        SettingsPage::Saving,
        SettingsPage::Tools,
        SettingsPage::Viewport,
        SettingsPage::Graphics,
        SettingsPage::Shortcuts,
        SettingsPage::Accessibility,
        SettingsPage::Updates,
        SettingsPage::Ai,
        SettingsPage::TeamCreate,
        SettingsPage::Engine,
        SettingsPage::Project,
    ];

    pub(super) fn label(self) -> &'static str {
        match self {
            SettingsPage::Appearance => "Appearance",
            SettingsPage::Typography => "Text & density",
            SettingsPage::Layout => "Editor layout",
            SettingsPage::Saving => "Saving",
            SettingsPage::Tools => "External tools",
            SettingsPage::Viewport => "Viewport",
            SettingsPage::Graphics => "Graphics",
            SettingsPage::Shortcuts => "Shortcuts",
            SettingsPage::Accessibility => "Accessibility",
            SettingsPage::Updates => "Updates",
            SettingsPage::Ai => "AI connections",
            SettingsPage::TeamCreate => "Collaborate",
            SettingsPage::Engine => "Engine",
            SettingsPage::Project => "This project",
        }
    }

    pub(super) fn icon(self) -> &'static str {
        match self {
            SettingsPage::Appearance => icons::PALETTE,
            SettingsPage::Typography => icons::TEXT_AA,
            SettingsPage::Layout => icons::LAYOUT,
            SettingsPage::Saving => icons::FLOPPY_DISK,
            SettingsPage::Tools => icons::WRENCH,
            SettingsPage::Viewport => icons::CUBE_FOCUS,
            SettingsPage::Graphics => icons::MONITOR,
            SettingsPage::Shortcuts => icons::KEYBOARD,
            SettingsPage::Accessibility => icons::EYE,
            SettingsPage::Updates => icons::ARROW_CIRCLE_UP,
            SettingsPage::Ai => icons::ROBOT,
            SettingsPage::TeamCreate => icons::USERS_THREE,
            SettingsPage::Engine => icons::HAMMER,
            SettingsPage::Project => icons::FOLDER_SIMPLE,
        }
    }
}

mod appearance;
mod graphics;
mod memory_budget;
mod search;

use appearance::*;
use graphics::*;
use search::*;
pub(super) use search::{SEARCHABLE, find_setting, section};

/// Whether Settings is open, and on which page.
#[derive(Default)]
pub(super) struct SettingsWindow {
    pub open: bool,
    pub page: SettingsPage,
}

const WINDOW_SIZE: egui::Vec2 = egui::vec2(700.0, 470.0);
const NAV_WIDTH: f32 = 170.0;

impl EditorApp {
    /// Changes the look to `appearance`, fading from the old background if
    /// the theme changed, and remembers it.
    pub(super) fn set_appearance(&mut self, ctx: &egui::Context, appearance: Appearance) {
        if appearance == self.appearance {
            return;
        }
        let old_background = theme::palette(ctx).panel;
        let theme_changed = appearance.theme != self.appearance.theme;
        self.appearance = appearance;
        theme::apply(ctx, &self.appearance);
        if theme_changed {
            theme::start_crossfade(ctx, old_background);
        }
    }

    /// The Settings window, while it's open.
    pub(super) fn settings_window(&mut self, ctx: &egui::Context) {
        if !self.settings.open {
            return;
        }
        let mut open = true;
        let mut restart = false;
        let mut appearance = self.appearance.clone();
        widgets::tool_window(icons::GEAR_SIX, "Settings")
            .id("settings window")
            .fixed(WINDOW_SIZE)
            .show(ctx, &mut open, |ui| {
                ui.horizontal_top(|ui| {
                    ui.vertical(|ui| {
                        ui.set_width(NAV_WIDTH);
                        ui.spacing_mut().item_spacing.y = 2.0;
                        let palette = theme::palette(ui.ctx());
                        ui.label(widgets::micro("Editor", palette.text_weak))
                            .on_hover_text("Yours: saved on this computer, for every project.");
                        for page in SettingsPage::ALL {
                            if page == SettingsPage::Engine && self.rebuilds.source.is_none() {
                                continue;
                            }
                            if page == SettingsPage::Project {
                                ui.add_space(space::M);
                                ui.label(widgets::micro("Project", palette.text_weak))
                                    .on_hover_text(
                                        "Saved in the project's folder, for everyone who opens it.",
                                    );
                            }
                            let selected = self.settings.page == page;
                            if widgets::nav_item(ui, page.icon(), page.label(), None, selected)
                                .clicked()
                            {
                                self.settings.page = page;
                            }
                        }
                    });
                    ui.separator();
                    egui::ScrollArea::vertical()
                        .id_salt("settings page")
                        .max_height(WINDOW_SIZE.y)
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            ui.set_min_height(WINDOW_SIZE.y);
                            ui.set_width(ui.available_width());
                            ui.add_space(space::XXS);
                            ui.vertical(|ui| match self.settings.page {
                                SettingsPage::Appearance => appearance_page(ui, &mut appearance),
                                SettingsPage::Typography => typography_page(ui, &mut appearance),
                                SettingsPage::Layout => self.layout_page(ui),
                                SettingsPage::Saving => self.saving_page(ui),
                                SettingsPage::Tools => self.tools_page(ui),
                                SettingsPage::Viewport => self.viewport_page(ui),
                                SettingsPage::Graphics => {
                                    let capabilities = self.capabilities.unwrap_or_default();
                                    restart |= graphics_page(ui, &mut self.graphics, &capabilities);
                                }
                                SettingsPage::Shortcuts => shortcuts_page(ui),
                                SettingsPage::Accessibility => {
                                    accessibility_page(ui, &mut appearance);
                                }
                                SettingsPage::Updates => self.updates_page(ui),
                                SettingsPage::Ai => self.ai_page(ui),
                                SettingsPage::TeamCreate => self.team_settings_page(ui),
                                SettingsPage::Engine => self.engine_page(ui),
                                SettingsPage::Project => self.project_page(ui),
                            });
                        });
                });
            });
        self.set_appearance(ctx, appearance);
        if !open {
            self.settings.open = false;
        }
        if restart {
            self.restart_editor();
        }
    }

    fn layout_page(&mut self, ui: &mut egui::Ui) {
        page_title(ui, "Editor layout", "Which panels show, and where they go.");
        section(ui, "Panels");
        for pane in Pane::CLOSABLE {
            let mut open = self.layout.is_open(pane);
            if ui
                .checkbox(&mut open, theme::with_icon(pane.icon(), pane.label()))
                .changed()
            {
                self.layout.toggle(pane);
            }
        }
        ui.add_space(6.0);
        let palette = theme::palette(ui.ctx());
        if ui
            .add(widgets::button(
                widgets::Role::Secondary,
                Some(icons::ARROW_COUNTER_CLOCKWISE),
                "Reset Layout",
                &palette,
            ))
            .on_hover_text("Put every panel back where it starts.")
            .clicked()
        {
            self.layout.reset();
        }
        ui.add_space(space::L);
        // The presets and saved layouts of View › Layout (design system V17).
        section(ui, "Layouts");
        ui.horizontal_wrapped(|ui| {
            for preset in crate::layout::Preset::ALL {
                if ui
                    .button(theme::with_icon(preset.icon(), preset.label()))
                    .on_hover_text(preset.about())
                    .clicked()
                {
                    self.layout.use_preset(preset);
                }
            }
        });
        let mut delete = None;
        let mut switch = None;
        for (name, _) in &self.saved_layouts.0 {
            ui.horizontal(|ui| {
                if ui
                    .button(theme::with_icon(icons::BOOKMARK_SIMPLE, name))
                    .on_hover_text("Switch to this saved layout.")
                    .clicked()
                {
                    switch = Some(name.clone());
                }
                if widgets::icon_button(ui, icons::TRASH, "Delete this saved layout", false)
                    .clicked()
                {
                    delete = Some(name.clone());
                }
            });
        }
        if let Some(name) = switch {
            self.use_saved_layout(&name);
        }
        if let Some(name) = delete {
            self.saved_layouts.delete(&name);
        }
        if ui
            .add(widgets::button(
                widgets::Role::Secondary,
                Some(icons::FLOPPY_DISK),
                "Save Current Layout…",
                &palette,
            ))
            .on_hover_text("Keep where the panels are now, by name, to switch back to later.")
            .clicked()
        {
            self.layout_name = Some(Default::default());
        }
        ui.add_space(space::L);
        // The start screen's navigation (`start/nav.rs`), saved per user.
        section(ui, "Start screen navigation");
        let mut nav = self.start.nav();
        widgets::segmented_tips(
            ui,
            &mut nav.layout,
            &[
                (
                    crate::start::NavLayout::Sidebar,
                    Some(crate::start::NavLayout::Sidebar.icon()),
                    crate::start::NavLayout::Sidebar.label(),
                ),
                (
                    crate::start::NavLayout::TopBar,
                    Some(crate::start::NavLayout::TopBar.icon()),
                    crate::start::NavLayout::TopBar.label(),
                ),
            ],
            &[
                "Projects, Learn and the other pages down the left side.",
                "The pages as tabs across the top, leaving the page the full width.",
            ],
        );
        let folded = ui
            .add_enabled(
                nav.layout == crate::start::NavLayout::Sidebar,
                egui::Checkbox::new(&mut nav.folded, "Fold the sidebar to its icons"),
            )
            .on_hover_text(
                "Only the pages' icons show, each with a tooltip. Gives the page more room.",
            );
        found(ui, &folded, "Fold the sidebar to its icons");
        if nav != self.start.nav() {
            self.start.set_nav(nav);
        }
        ui.add_space(space::L);
        section(ui, "Ribbon");
        ui.label(
            egui::RichText::new(
                "Make your own ribbon tab from any of the ribbon's groups. Right-click a tab \
                 you made to change or delete it.",
            )
            .color(palette.text_secondary),
        );
        if ui
            .add(widgets::button(
                widgets::Role::Secondary,
                Some(icons::PLUS),
                "New Ribbon Tab…",
                &palette,
            ))
            .clicked()
        {
            self.ribbon_tabs.edit(None);
        }
    }

    /// What the Scene view draws over the world: the same toggles as the
    /// menu bar's Viewport menu, and the grid's strength.
    fn viewport_page(&mut self, ui: &mut egui::Ui) {
        page_title(
            ui,
            "Viewport",
            "What the Scene view draws while you edit. Yours, for every project.",
        );
        section(ui, "Guides");
        let row = ui
            .checkbox(
                &mut self.light_cones,
                theme::with_icon(icons::FLASHLIGHT, "Show every light's cone"),
            )
            .on_hover_text(
                "Off, only the selected light (or the part it's in) shows where it shines.",
            );
        found(ui, &row, "Show every light's cone");
        let row = ui
            .checkbox(
                &mut self.grid,
                theme::with_icon(icons::GRID_NINE, "Show the measuring grid"),
            )
            .on_hover_text("Lines and distances in studs on the baseplate, while editing.");
        found(ui, &row, "Show the measuring grid");
        let row = ui
            .add_enabled(
                self.grid,
                egui::Slider::new(
                    &mut self.grid_strength,
                    MIN_GRID_STRENGTH..=MAX_GRID_STRENGTH,
                )
                .text("Grid strength")
                .custom_formatter(|value, _| format!("{:.0}%", value * 100.0)),
            )
            .on_hover_text("How solid the grid's lines are. Lower is easier on the eyes.");
        found(ui, &row, "Grid strength");
        let row = ui.checkbox(
            &mut self.view_cube,
            theme::with_icon(icons::CUBE_FOCUS, "Show the view cube"),
        )
        .on_hover_text(
            "The cube in the viewport's corner that turns the camera to a face, edge or corner.",
        );
        found(ui, &row, "Show the view cube");
        let row = ui
            .checkbox(
                &mut self.move_guide,
                theme::with_icon(icons::ARROWS_OUT_CARDINAL, "Show the movement guide"),
            )
            .on_hover_text(MOVE_GUIDE_HINT);
        found(ui, &row, "Show the movement guide");
        ui.add_space(space::L);
        section(ui, "Overlays");
        let row = ui
            .checkbox(
                &mut self.overlays.colliders,
                theme::with_icon(icons::CUBE, "Show colliders"),
            )
            .on_hover_text("The shape each part collides with, including sensors and terrain.");
        found(ui, &row, "Show colliders");
        let row = ui
            .checkbox(
                &mut self.overlays.raycasts,
                theme::with_icon(icons::LINE_SEGMENT, "Show raycasts"),
            )
            .on_hover_text("Recent workspace:Raycast calls during Play.");
        found(ui, &row, "Show raycasts");
        let row = ui
            .checkbox(
                &mut self.overlays.bounds,
                theme::with_icon(icons::SQUARE, "Show bounds"),
            )
            .on_hover_text("Each part's Size box.");
        found(ui, &row, "Show bounds");
        let row = ui
            .checkbox(
                &mut self.overlays.emitters,
                theme::with_icon(icons::SPEAKER_HIGH, "Show audio emitters"),
            )
            .on_hover_text("Where each spatial Sound is heard from.");
        found(ui, &row, "Show audio emitters");
        let row = ui
            .checkbox(
                &mut self.overlays.nav_mesh,
                theme::with_icon(icons::FOOTPRINTS, "Show navigation mesh"),
            )
            .on_hover_text("Where characters can walk, coloured by connected ground.");
        found(ui, &row, "Show navigation mesh");
        let row = ui
            .checkbox(
                &mut self.overlays.agent_paths,
                theme::with_icon(icons::PATH, "Show agent paths"),
            )
            .on_hover_text("The path each NavigationAgent is walking, during Play.");
        found(ui, &row, "Show agent paths");
        let row = ui
            .checkbox(
                &mut self.overlays.constraints,
                theme::with_icon(icons::LINK_SIMPLE, "Show constraints"),
            )
            .on_hover_text("Welds as lines between their parts, and every Attachment.");
        found(ui, &row, "Show constraints");
        let row = ui
            .checkbox(
                &mut self.overlays.contacts,
                theme::with_icon(icons::ARROWS_IN_LINE_VERTICAL, "Show contacts"),
            )
            .on_hover_text("Where parts touch and how hard they push, during Play.");
        found(ui, &row, "Show contacts");
        let row = ui
            .checkbox(
                &mut self.overlays.bodies,
                theme::with_icon(icons::CROSSHAIR, "Show bodies"),
            )
            .on_hover_text("Each moving body's center of mass, awake or asleep, during Play.");
        found(ui, &row, "Show bodies");
        let row = ui
            .checkbox(
                &mut self.overlays.streaming,
                theme::with_icon(icons::BROADCAST, "Show streaming"),
            )
            .on_hover_text("What streams around the camera, with Workspace streaming on.");
        found(ui, &row, "Show streaming");
        let row = ui
            .checkbox(
                &mut self.overlays.light_probes,
                theme::with_icon(icons::LIGHTBULB, "Show light probes"),
            )
            .on_hover_text(
                "Each light probe volume's probes, filled with their baked light, and which bakes are missing or out of date.",
            );
        found(ui, &row, "Show light probes");
        let row = ui
            .checkbox(
                &mut self.overlays.culling,
                theme::with_icon(icons::FRAME_CORNERS, "Show culling"),
            )
            .on_hover_text(
                "Holds what's drawn to the current view, and outlines what's drawn and culled. \
             Not saved between runs.",
            );
        found(ui, &row, "Show culling");
    }

    fn saving_page(&mut self, ui: &mut egui::Ui) {
        page_title(
            ui,
            "Saving",
            "How your unsaved work is kept safe. Yours, for every project.",
        );
        section(ui, "AutoRecover");
        let row = ui
            .horizontal(|ui| {
                ui.label(theme::with_icon(
                    icons::CLOCK_COUNTER_CLOCKWISE,
                    "Keep a copy of unsaved work",
                ));
                egui::ComboBox::from_id_salt("autorecover minutes")
                    .selected_text(autorecover::minutes_label(self.autorecover.minutes))
                    .show_ui(ui, |ui| {
                        for minutes in autorecover::MINUTE_CHOICES {
                            ui.selectable_value(
                                &mut self.autorecover.minutes,
                                minutes,
                                autorecover::minutes_label(minutes),
                            );
                        }
                    });
            })
            .response
            .on_hover_text(
                "While a project has unsaved changes, a copy of them is written to its \
                 .axcenity/autorecover folder this often. If the editor crashes or the \
                 computer loses power, opening the project offers to recover them. Saving \
                 removes the copy.",
            );
        found(ui, &row, "Keep a copy of unsaved work");
        ui.add_space(space::S);
        ui.label(
            egui::RichText::new(
                "Kept: the world, script tabs and block scripts. Not yet kept: UI graphs, \
                 behavior trees, dialogues, data tables, materials and terrain sculpting, so \
                 save those yourself. A Collaborate session keeps its own copy.",
            )
            .color(theme::palette(ui.ctx()).text_weak),
        );
    }

    /// Programs the editor opens files in (R15.4): Blender's path.
    fn tools_page(&mut self, ui: &mut egui::Ui) {
        page_title(
            ui,
            "External tools",
            "Programs the editor opens your files in. Yours, for every project.",
        );
        section(ui, "Blender");
        let blender = crate::blender::find_blender(&self.blender_path);
        let row = ui
            .horizontal(|ui| {
                ui.label(theme::with_icon(icons::ARROW_SQUARE_OUT, "Blender"));
                let width = (ui.available_width() - 90.0).max(120.0);
                ui.add(
                    egui::TextEdit::singleline(&mut self.blender_path)
                        .desired_width(width)
                        .hint_text("Found by itself"),
                );
                if ui.button("Browse…").clicked()
                    && let Some(file) = rfd::FileDialog::new()
                        .set_title("Where is Blender?")
                        .pick_file()
                {
                    self.blender_path = file.to_string_lossy().into_owned();
                }
            })
            .response
            .on_hover_text(
                "The Blender program Edit in Blender opens models in. Leave it empty to use the \
                 one on PATH or in the usual install folder.",
            );
        found(ui, &row, "Blender");
        ui.add_space(space::S);
        let palette = theme::palette(ui.ctx());
        match blender {
            Some(path) => ui.label(
                egui::RichText::new(format!("Uses {}", path.display())).color(palette.text_weak),
            ),
            None => ui.colored_label(
                palette.warning,
                "Blender wasn't found. Install it from blender.org, or browse to it.",
            ),
        };
        ui.label(
            egui::RichText::new(
                "Edit in Blender (on a model in Assets, a MeshPart's Properties or the Model \
                 Studio) opens the .glb there. Save in Blender (Ctrl+S) to send it back: \
                 MeshParts using it reload, and the Model Studio offers to load it again. \
                 Nothing is installed into Blender.",
            )
            .color(palette.text_weak),
        );
    }

    /// The open project's settings: what they are and where each is
    /// changed. They're saved in the project, so they aren't kept with the
    /// user's own on the other pages.
    fn project_page(&mut self, ui: &mut egui::Ui) {
        page_title(
            ui,
            "This project",
            "Saved in the project's folder and shared with everyone who opens it. Everything \
             on the other pages is yours, saved on this computer for every project.",
        );
        let Some(project) = &self.project else {
            ui.weak("Open a project to change its settings.");
            return;
        };
        let palette = theme::palette(ui.ctx());
        ui.label(
            egui::RichText::new(&project.name)
                .size(type_scale::HEADING)
                .color(palette.text_strong),
        );
        ui.label(
            egui::RichText::new(project.folder.display().to_string())
                .size(type_scale::SMALL)
                .color(palette.text_weak),
        );
        ui.add_space(space::L);
        section(ui, "Project settings");
        let editing = !self.playing();
        let row = |ui: &mut egui::Ui, icon: &str, name: &'static str, about: &str| {
            // The button, then what it's for under it, so a narrow window
            // wraps the words instead of cutting them off.
            let button = ui
                .add_enabled(
                    editing,
                    widgets::button(widgets::Role::Secondary, Some(icon), name, &palette),
                )
                .on_disabled_hover_text("Press Stop to change it.");
            found(ui, &button, name);
            ui.label(
                egui::RichText::new(about)
                    .size(type_scale::SMALL)
                    .color(palette.text_secondary),
            );
            ui.add_space(space::S);
            button.clicked()
        };
        if row(
            ui,
            icons::GAME_CONTROLLER,
            "Input…",
            "The game's actions and the keys and buttons bound to them.",
        ) {
            self.input_settings.open = true;
        }
        if row(
            ui,
            icons::INTERSECT,
            "Collision Groups…",
            "Which groups of parts pass through each other.",
        ) {
            self.collision_groups.open = true;
        }
        if row(
            ui,
            icons::GLOBE_HEMISPHERE_WEST,
            "Workspace",
            "Gravity, physics and streaming: select it to see them in Properties.",
        ) {
            let workspace = self.world.workspace();
            self.select_from_viewport(Some(workspace));
        }
        if row(
            ui,
            icons::SUN,
            "Lighting",
            "The game's sky, time of day, fog and post effects, in Properties.",
        ) {
            let lighting = self.world.lighting();
            self.select_from_viewport(Some(lighting));
        }
        ui.add_space(space::S);
        section(ui, "Memory budget");
        self.memory_budget_settings(ui);
        ui.add_space(space::S);
        section(ui, "Branding");
        self.branding_settings(ui);
        ui.label(
            egui::RichText::new(
                "The name, description and splash are on the start screen: open the project's \
                 menu there and pick Settings. Build options are in File > Build Game.",
            )
            .size(type_scale::SMALL)
            .color(palette.text_weak),
        );
    }
}

/// A page's title and a line saying what's on it.
pub(super) fn page_title(ui: &mut egui::Ui, title: &str, subtitle: &str) {
    let palette = theme::palette(ui.ctx());
    let row = ui.label(
        egui::RichText::new(title)
            .family(theme::display_family())
            .size(type_scale::HEADING + 2.0)
            .extra_letter_spacing(theme::TITLE_TRACKING)
            .color(palette.text_strong),
    );
    found(ui, &row, title);
    ui.label(egui::RichText::new(subtitle).color(palette.text_secondary));
    ui.add_space(14.0);
}

/// The keyboard shortcuts, as a table. Shortcuts are fixed for now, so this
/// is a reference.
fn shortcuts_page(ui: &mut egui::Ui) {
    page_title(
        ui,
        "Shortcuts",
        "Every key the editor knows, by where it works.",
    );
    let palette = theme::palette(ui.ctx());
    let actions = shortcuts::Action::all();
    // One width for every group's names, so the keys line up down the page;
    // a longer name wraps.
    let name_width = (ui.available_width() * 0.55).clamp(160.0, 320.0);
    for context in shortcuts::Context::ALL {
        widgets::section_label(ui, context.label());
        ui.label(
            egui::RichText::new(context.about())
                .size(type_scale::SMALL)
                .color(palette.text_weak),
        );
        ui.add_space(space::XS);
        egui::Grid::new(("shortcuts", context.label()))
            .num_columns(2)
            .min_col_width(name_width)
            .spacing([12.0, 6.0])
            .show(ui, |ui| {
                for action in actions.iter().filter(|action| action.context() == context) {
                    let name = egui::RichText::new(action.label()).color(palette.text_secondary);
                    ui.vertical(|ui| {
                        ui.set_max_width(name_width);
                        ui.add(egui::Label::new(name).wrap());
                    });
                    keycap(ui, &action.text(ui.ctx()));
                    ui.end_row();
                }
            });
        ui.add_space(space::M);
    }
    ui.label(
        egui::RichText::new(
            "In any field, menu or dialog, Enter confirms and Esc cancels or closes.",
        )
        .color(palette.text_weak),
    );
}

/// Keys drawn like a key on a keyboard.
fn keycap(ui: &mut egui::Ui, keys: &str) {
    let palette = theme::palette(ui.ctx());
    egui::Frame::new()
        .fill(palette.control)
        .stroke(egui::Stroke::new(1.0_f32, palette.border))
        .corner_radius(theme::radius::CONTROL)
        .inner_margin(egui::Margin::symmetric(6, 1))
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new(keys)
                    .monospace()
                    .size(type_scale::SMALL)
                    .color(palette.text),
            );
        });
}

#[cfg(test)]
mod tests;
