//! The Settings window (File > Settings…, the start screen's sidebar, or
//! Ctrl+, / Cmd+,): appearance, typography and density, the editor layout,
//! this computer's graphics, the keyboard shortcuts, accessibility, and (in
//! a debug editor run from the source tree) rebuilding the engine.
//! Changes apply at once and are saved between runs with the other settings.

use super::*;

use crate::shortcuts;
use crate::theme::{Accent, Appearance, Density, Palette, ThemeChoice, space, type_scale};
use crate::widgets;
use axcenity_render::{
    AmbientOcclusion, AntiAliasing, Capabilities, CloudQuality, DepthOfFieldQuality,
    GraphicsSettings, MotionBlurQuality, Quality, RENDER_SCALE_RANGE, ReflectionQuality,
    ShadowQuality, TEXTURE_BUDGET_RANGE, TextureQuality,
};

/// The pages of the Settings window.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum SettingsPage {
    #[default]
    Appearance,
    Typography,
    Layout,
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
    const ALL: [SettingsPage; 12] = [
        SettingsPage::Appearance,
        SettingsPage::Typography,
        SettingsPage::Layout,
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
            SettingsPage::Viewport => "Viewport",
            SettingsPage::Graphics => "Graphics",
            SettingsPage::Shortcuts => "Shortcuts",
            SettingsPage::Accessibility => "Accessibility",
            SettingsPage::Updates => "Updates",
            SettingsPage::Ai => "AI connections",
            SettingsPage::TeamCreate => "Team Create",
            SettingsPage::Engine => "Engine",
            SettingsPage::Project => "This project",
        }
    }

    pub(super) fn icon(self) -> &'static str {
        match self {
            SettingsPage::Appearance => icons::PALETTE,
            SettingsPage::Typography => icons::TEXT_AA,
            SettingsPage::Layout => icons::LAYOUT,
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
                                SettingsPage::Viewport => self.viewport_page(ui),
                                SettingsPage::Graphics => {
                                    let capabilities = self.capabilities.unwrap_or_default();
                                    graphics_page(ui, &mut self.graphics, &capabilities);
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
                &mut self.overlays.culling,
                theme::with_icon(icons::FRAME_CORNERS, "Show culling"),
            )
            .on_hover_text(
                "Holds what's drawn to the current view, and outlines what's drawn and culled. \
             Not saved between runs.",
            );
        found(ui, &row, "Show culling");
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

/// A row of the Settings window that universal search lists
/// (`app/search.rs`): picking it opens its page, scrolls to the row and
/// outlines it for a moment ([`find_setting`]).
pub(super) struct SearchableSetting {
    pub page: SettingsPage,
    /// What the row says: the label, section or checkbox it's found by.
    pub name: &'static str,
    /// Other words for it.
    pub aliases: &'static [&'static str],
    pub description: &'static str,
}

const fn setting(
    page: SettingsPage,
    name: &'static str,
    aliases: &'static [&'static str],
    description: &'static str,
) -> SearchableSetting {
    SearchableSetting {
        page,
        name,
        aliases,
        description,
    }
}

/// Every searchable row, by page. `every_searchable_setting_is_found_on_its_page`
/// checks each name is drawn (and highlighted) on its page.
pub(super) const SEARCHABLE: &[SearchableSetting] = {
    use SettingsPage::*;
    &[
        setting(
            Appearance,
            "Theme",
            &["Dark mode", "Light mode", "Match system", "Colors"],
            "Dark, light, or match the computer's setting.",
        ),
        setting(
            Appearance,
            "Accent",
            &["Accent colour", "Highlight colour", "Selection colour"],
            "The colour of selections, focus rings and primary buttons.",
        ),
        setting(
            Appearance,
            "Interface scale",
            &["Zoom", "UI size", "DPI", "Bigger text"],
            "How big the whole editor is drawn, 90% to 150%.",
        ),
        setting(
            Typography,
            "Density",
            &["Compact", "Comfortable", "Spacing", "Row height"],
            "How much room rows and controls get.",
        ),
        setting(
            Typography,
            "Code and Output",
            &[
                "Code font size",
                "Console font",
                "Monospace",
                "Script text size",
            ],
            "The size of code in scripts and the Console.",
        ),
        setting(
            Typography,
            "Interface font",
            &["Font", "Typeface"],
            "The fonts the editor uses.",
        ),
        setting(
            Layout,
            "Panels",
            &["Show panel", "Hide panel", "Docking", "Windows"],
            "Which panels show.",
        ),
        setting(
            Layout,
            "Layouts",
            &[
                "Layout presets",
                "Workspaces",
                "Save layout",
                "Scripting layout",
                "Animation layout",
                "Modeling layout",
            ],
            "Switch to a preset for a task or a layout you saved, or save this one.",
        ),
        setting(
            Layout,
            "Start screen navigation",
            &[
                "Sidebar",
                "Top bar",
                "Navigation",
                "Home screen",
                "Collapse sidebar",
            ],
            "The start screen's pages down the left side, or as tabs across the top.",
        ),
        setting(
            Layout,
            "Fold the sidebar to its icons",
            &["Collapse sidebar", "Compact sidebar", "Icons only"],
            "The start screen's sidebar shows only its icons, each with a tooltip.",
        ),
        setting(
            Viewport,
            "Show every light's cone",
            &["Light cones", "Spotlight"],
            "Off, only the selected light shows where it shines.",
        ),
        setting(
            Viewport,
            "Show the measuring grid",
            &["Grid", "Ruler", "Studs"],
            "Lines and distances in studs on the baseplate, while editing.",
        ),
        setting(
            Viewport,
            "Grid strength",
            &["Grid opacity", "Grid lines"],
            "How solid the grid's lines are.",
        ),
        setting(
            Viewport,
            "Show the view cube",
            &["View cube", "Navigation cube", "Gizmo"],
            "The cube in the viewport's corner that turns the camera to a face.",
        ),
        setting(
            Viewport,
            "Show the movement guide",
            &[
                "Movement guide",
                "Move guide",
                "Snap points",
                "Distance",
                "Drag guide",
            ],
            "The line, snap points and distance drawn while moving parts.",
        ),
        setting(
            Viewport,
            "Show colliders",
            &["Collision shapes", "Physics", "Hitboxes"],
            "The shape each part collides with.",
        ),
        setting(
            Viewport,
            "Show raycasts",
            &["Rays", "Physics debug"],
            "Recent workspace:Raycast calls during Play.",
        ),
        setting(
            Viewport,
            "Show bounds",
            &["Bounding boxes", "Size box"],
            "Each part's Size box.",
        ),
        setting(
            Viewport,
            "Show audio emitters",
            &["Sound", "Audio debug"],
            "Where each spatial Sound is heard from.",
        ),
        setting(
            Viewport,
            "Show navigation mesh",
            &["Nav mesh", "Pathfinding", "Walkable"],
            "Where characters can walk.",
        ),
        setting(
            Viewport,
            "Show agent paths",
            &["Pathfinding", "AI paths"],
            "The path each NavigationAgent is walking, during Play.",
        ),
        setting(
            Viewport,
            "Show constraints",
            &["Welds", "Attachments", "Joints"],
            "Welds as lines between their parts, and every Attachment.",
        ),
        setting(
            Viewport,
            "Show culling",
            &["Frustum", "Occlusion", "Culled objects"],
            "What's drawn and what's culled for the current view.",
        ),
        setting(
            Layout,
            "Ribbon",
            &["Custom ribbon tab", "Toolbar"],
            "Make your own ribbon tab from any of the ribbon's groups.",
        ),
        setting(
            Project,
            "Input…",
            &["Key bindings", "Controls", "Actions", "Project settings"],
            "The game's actions and the keys bound to them, saved in the project.",
        ),
        setting(
            Project,
            "Collision Groups…",
            &["Collision", "Physics groups", "Project settings"],
            "Which groups of parts pass through each other, saved in the project.",
        ),
        setting(
            Graphics,
            "Quality",
            &["Graphics preset", "Ultra", "High", "Medium", "Low"],
            "This computer's quality preset for the viewport and Play.",
        ),
        setting(
            Graphics,
            "Shadow quality",
            &["Shadow resolution", "Shadow map"],
            "How sharp and far-reaching shadows are.",
        ),
        setting(
            Graphics,
            "Shadow distance",
            &["Shadow range", "Cascades"],
            "How far from the camera shadows are drawn.",
        ),
        setting(
            Graphics,
            "Post-processing",
            &["Bloom", "Tone mapping", "Effects"],
            "Bloom, colour correction and the other screen effects.",
        ),
        setting(
            Graphics,
            "Render scale",
            &["Resolution", "Upscaling", "Supersampling"],
            "How many pixels are drawn, before scaling to the window.",
        ),
        setting(
            Graphics,
            "Texture quality",
            &["Texture resolution", "Mipmaps"],
            "How detailed textures are.",
        ),
        setting(
            Graphics,
            "Texture memory",
            &["VRAM", "Texture budget", "GPU memory"],
            "How much graphics memory textures may use.",
        ),
        setting(
            Graphics,
            "Anti-aliasing",
            &["AA", "MSAA", "TAA", "FXAA", "Jaggies"],
            "Smoothing jagged edges.",
        ),
        setting(
            Graphics,
            "Ambient occlusion",
            &["AO", "SSAO", "GTAO"],
            "Soft shadows in corners and creases.",
        ),
        setting(
            Graphics,
            "Reflection quality",
            &["Reflections", "SSR"],
            "How reflections are drawn.",
        ),
        setting(
            Graphics,
            "Cloud quality",
            &["Clouds", "Sky"],
            "How detailed volumetric clouds are.",
        ),
        setting(
            Graphics,
            "Contact shadows",
            &["Small shadows"],
            "Short screen-space shadows where things touch.",
        ),
        setting(
            Graphics,
            "Real-time GI",
            &["Global illumination", "Bounce light", "GI"],
            "Light bouncing off surfaces, worked out as you go.",
        ),
        setting(
            Graphics,
            "Volumetric fog",
            &["Fog", "God rays", "Light shafts"],
            "Fog that lights and shadows shine through.",
        ),
        setting(
            Graphics,
            "Depth of field",
            &["DOF", "Blur", "Focus"],
            "Blurring what's out of focus.",
        ),
        setting(
            Graphics,
            "Motion blur",
            &["Blur"],
            "Blurring fast movement.",
        ),
        setting(
            Graphics,
            "Vignette",
            &["Dark corners"],
            "Darkening the screen's corners.",
        ),
        setting(
            Graphics,
            "Film grain",
            &["Noise", "Grain"],
            "A film-like noise over the picture.",
        ),
        setting(
            Graphics,
            "Chromatic aberration",
            &["Colour fringing", "Lens effect"],
            "Colour fringes towards the screen's edges.",
        ),
        setting(
            Graphics,
            "Sharpen",
            &["Sharpness", "Sharpening"],
            "Crisper edges after scaling.",
        ),
        setting(
            Graphics,
            "In use",
            &["Effective settings", "GPU limits"],
            "What's in use once the graphics card's limits are applied, and why.",
        ),
        setting(
            Shortcuts,
            "Shortcuts",
            &["Keyboard shortcuts", "Hotkeys", "Keybindings", "Keys"],
            "The keys the editor knows.",
        ),
        setting(
            Accessibility,
            "Reduce motion",
            &["Animations", "Fades", "Motion sickness"],
            "Turns off hover fades, popup fades and smooth scrolling.",
        ),
        setting(
            Accessibility,
            "Increase contrast",
            &["High contrast", "Contrast", "Low vision", "Outlines"],
            "Stronger outlines, borders, faint text and focus rings.",
        ),
        setting(
            Accessibility,
            "Steady frame rate",
            &[
                "FPS cap",
                "VSync",
                "Refresh rate",
                "OLED flicker",
                "FreeSync",
                "G-Sync",
                "VRR",
            ],
            "Shows frames at a fixed rate so a variable-refresh display doesn't flicker.",
        ),
        setting(
            Accessibility,
            "Hold the refresh rate steady",
            &["Flicker", "OLED", "Idle"],
            "Keeps drawing while the editor has focus, for displays that flicker as the rate drops.",
        ),
        setting(
            Accessibility,
            "Readability",
            &["Contrast", "Text size"],
            "Contrast in both themes, and how to make text larger.",
        ),
        setting(
            Accessibility,
            "Keyboard",
            &["Tab navigation", "Focus ring"],
            "Moving between controls with Tab.",
        ),
        setting(
            Updates,
            "Update channel",
            &["Stable", "Beta", "Development", "Alpha", "Pre-release"],
            "Which releases the editor tells you about.",
        ),
        setting(
            Updates,
            "Automatic checks",
            &["Check for updates", "Update check", "New version"],
            "How often the editor looks for a new release, and Check now.",
        ),
        setting(
            Ai,
            "Allow AI connections",
            &["MCP", "Claude Code", "Cursor", "Codex", "AI agent"],
            "Let AI clients work in this window instead of a copy without one.",
        ),
        setting(
            TeamCreate,
            "Your name and colour",
            &[
                "Team Create name",
                "Collaborator colour",
                "Identity",
                "Device key",
            ],
            "How others see you in a Team Create session.",
        ),
        setting(
            TeamCreate,
            "New invites",
            &["Invite link", "Invite expiry", "Default invite role"],
            "The role new Team Create invites give, and how long they work.",
        ),
    ]
};

/// What [`find_setting`] asked for: the row, when, and whether it was
/// scrolled to yet.
#[derive(Clone)]
struct Finding {
    name: &'static str,
    since: f64,
    scrolled: bool,
}

/// How long a found row stays outlined, in seconds.
const FOUND_SECONDS: f64 = 2.0;

fn finding_id() -> egui::Id {
    egui::Id::new("settings find")
}

/// Scrolls the Settings window to the row called `name` the next time its
/// page is drawn, and outlines it for a moment.
pub(super) fn find_setting(ctx: &egui::Context, name: &'static str) {
    let since = ctx.input(|input| input.time);
    ctx.data_mut(|data| {
        data.insert_temp(
            finding_id(),
            Finding {
                name,
                since,
                scrolled: false,
            },
        );
    });
}

/// A section label that search can find.
pub(super) fn section(ui: &mut egui::Ui, name: &str) {
    let palette = theme::palette(ui.ctx());
    let row = ui.label(widgets::micro(name, palette.text_weak));
    found(ui, &row, name);
}

/// A row's label that search can find.
fn found_label(ui: &mut egui::Ui, name: &str) {
    let row = ui.label(name);
    found(ui, &row, name);
}

/// If search asked for the row `name` (drawn as `row`), scrolls to it once
/// and outlines it in the accent, fading.
fn found(ui: &egui::Ui, row: &egui::Response, name: &str) {
    let Some(mut finding) = ui.data(|data| data.get_temp::<Finding>(finding_id())) else {
        return;
    };
    if finding.name != name {
        return;
    }
    let age = ui.input(|input| input.time) - finding.since;
    if age > FOUND_SECONDS {
        ui.data_mut(|data| data.remove::<Finding>(finding_id()));
        return;
    }
    if !finding.scrolled {
        row.scroll_to_me(Some(egui::Align::Center));
        finding.scrolled = true;
        ui.data_mut(|data| data.insert_temp(finding_id(), finding));
        #[cfg(test)]
        ui.data_mut(|data| data.insert_temp(egui::Id::new("settings found"), name.to_owned()));
    }
    let palette = theme::palette(ui.ctx());
    let strength = (1.0 - age / FOUND_SECONDS).clamp(0.0, 1.0) as f32;
    let rect = row.rect.expand2(egui::vec2(6.0, 3.0));
    ui.painter().rect(
        rect,
        theme::radius::CONTROL,
        palette.accent_soft.gamma_multiply(0.5 * strength),
        egui::Stroke::new(1.5_f32, palette.accent.gamma_multiply(strength)),
        egui::StrokeKind::Outside,
    );
    ui.ctx().request_repaint();
}

fn appearance_page(ui: &mut egui::Ui, appearance: &mut Appearance) {
    page_title(
        ui,
        "Appearance",
        "The theme, the accent colour and how big everything is.",
    );
    section(ui, "Theme");
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 12.0;
        for choice in ThemeChoice::ALL {
            if theme_preview(ui, choice, appearance.accent, appearance.theme == choice).clicked() {
                appearance.theme = choice;
            }
        }
    });
    ui.add_space(space::L);
    section(ui, "Accent");
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 10.0;
        for accent in Accent::ALL {
            if accent_swatch(ui, accent, appearance.accent == accent).clicked() {
                appearance.accent = accent;
            }
        }
        ui.add_space(space::XS);
        let palette = theme::palette(ui.ctx());
        ui.label(egui::RichText::new(appearance.accent.label()).color(palette.text_secondary));
    });
    ui.add_space(space::L);
    section(ui, "Interface scale");
    let options: Vec<(f32, Option<&str>, String)> = Appearance::UI_SCALES
        .iter()
        .map(|&scale| (scale, None, format!("{}%", (scale * 100.0).round())))
        .collect();
    let options: Vec<(f32, Option<&str>, &str)> = options
        .iter()
        .map(|(scale, icon, label)| (*scale, *icon, label.as_str()))
        .collect();
    widgets::segmented(ui, &mut appearance.ui_scale, &options);
}

fn typography_page(ui: &mut egui::Ui, appearance: &mut Appearance) {
    page_title(
        ui,
        "Text & density",
        "How much room controls get, and the size of code.",
    );
    section(ui, "Density");
    widgets::segmented(
        ui,
        &mut appearance.density,
        &Density::ALL.map(|density| (density, None, density.label())),
    );
    let palette = theme::palette(ui.ctx());
    ui.label(
        egui::RichText::new(match appearance.density {
            Density::Comfortable => "Roomier rows and controls, easier to hit.",
            Density::Compact => "Tighter rows and controls, to see more at once.",
        })
        .color(palette.text_secondary),
    );
    ui.add_space(space::L);
    section(ui, "Code and Output");
    ui.add(
        egui::Slider::new(&mut appearance.code_font_size, Appearance::CODE_FONT_SIZES)
            .step_by(0.5)
            .suffix(" pt"),
    );
    egui::Frame::new()
        .fill(palette.inset)
        .stroke(egui::Stroke::new(1.0_f32, palette.border))
        .corner_radius(theme::radius::CARD)
        .inner_margin(egui::Margin::symmetric(12, 10))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            let font = egui::FontId::monospace(appearance.code_font_size);
            ui.label(
                egui::RichText::new("local part = workspace.Pad")
                    .font(font.clone())
                    .color(palette.text),
            );
            ui.label(
                egui::RichText::new("part.Touched:Connect(onTouched) -- 0O 1lI")
                    .font(font)
                    .color(palette.text_secondary),
            );
        });
    ui.add_space(space::L);
    section(ui, "Interface font");
    ui.label(
        egui::RichText::new("Inter for the interface and JetBrains Mono for code, both built in.")
            .color(palette.text_secondary),
    );
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

/// This computer's graphics settings: a preset, its overrides, and what's
/// in use once the GPU's limits are applied, with why anything was lowered.
fn graphics_page(ui: &mut egui::Ui, settings: &mut GraphicsSettings, capabilities: &Capabilities) {
    page_title(
        ui,
        "Graphics",
        "This computer's quality, for the viewport and Play. Not saved in projects.",
    );
    let palette = theme::palette(ui.ctx());
    section(ui, "Quality");
    widgets::segmented(
        ui,
        &mut settings.quality,
        &Quality::ALL.map(|quality| (quality, None, quality.label())),
    );
    let preset = GraphicsSettings::preset(settings.quality);
    ui.add_space(space::L);
    section(ui, "Overrides");
    egui::Grid::new("graphics overrides")
        .num_columns(2)
        .spacing([12.0, 8.0])
        .show(ui, |ui| {
            found_label(ui, "Shadow quality");
            let label = |quality: Option<ShadowQuality>| match quality {
                Some(quality) => quality.label().to_owned(),
                None => format!("Preset ({})", preset.shadow_quality().label()),
            };
            egui::ComboBox::from_id_salt("shadow quality")
                .selected_text(label(settings.shadow_quality))
                .show_ui(ui, |ui| {
                    for choice in std::iter::once(None).chain(ShadowQuality::ALL.map(Some)) {
                        ui.selectable_value(&mut settings.shadow_quality, choice, label(choice));
                    }
                });
            ui.end_row();

            found_label(ui, "Shadow distance");
            ui.horizontal(|ui| {
                let mut limited = settings.shadow_distance.is_some();
                if ui
                    .checkbox(&mut limited, "")
                    .on_hover_text("Off, the preset's distance is used.")
                    .changed()
                {
                    settings.shadow_distance = limited.then(|| preset.shadow_distance());
                }
                match &mut settings.shadow_distance {
                    Some(distance) => {
                        ui.add(
                            egui::DragValue::new(distance)
                                .range(0.0..=5000.0)
                                .speed(1.0)
                                .suffix(" studs"),
                        );
                    }
                    None => {
                        ui.label(
                            egui::RichText::new(format!(
                                "Preset ({} studs)",
                                preset.shadow_distance()
                            ))
                            .color(palette.text_secondary),
                        );
                    }
                }
            });
            ui.end_row();

            found_label(ui, "Post-processing");
            let label = |on: Option<bool>| match on {
                Some(true) => "On".to_owned(),
                Some(false) => "Off".to_owned(),
                None => format!(
                    "Preset ({})",
                    if preset.post_processing() {
                        "On"
                    } else {
                        "Off"
                    }
                ),
            };
            egui::ComboBox::from_id_salt("post-processing")
                .selected_text(label(settings.post_processing))
                .show_ui(ui, |ui| {
                    for choice in [None, Some(true), Some(false)] {
                        ui.selectable_value(&mut settings.post_processing, choice, label(choice));
                    }
                });
            ui.end_row();

            found_label(ui, "Render scale");
            ui.horizontal(|ui| {
                let mut custom = settings.render_scale.is_some();
                if ui
                    .checkbox(&mut custom, "")
                    .on_hover_text("Off, the preset's scale is used.")
                    .changed()
                {
                    settings.render_scale = custom.then(|| preset.render_scale());
                }
                match &mut settings.render_scale {
                    Some(scale) => {
                        let mut percent = (*scale * 100.0).round();
                        let range = RENDER_SCALE_RANGE;
                        if ui
                            .add(
                                egui::DragValue::new(&mut percent)
                                    .range(range.start() * 100.0..=range.end() * 100.0)
                                    .speed(1.0)
                                    .suffix("%"),
                            )
                            .changed()
                        {
                            *scale = percent / 100.0;
                        }
                    }
                    None => {
                        ui.label(
                            egui::RichText::new(format!(
                                "Preset ({}%)",
                                (preset.render_scale() * 100.0).round()
                            ))
                            .color(palette.text_secondary),
                        );
                    }
                }
            });
            ui.end_row();

            found_label(ui, "Texture quality");
            let label = |quality: Option<TextureQuality>| match quality {
                Some(quality) => quality.label().to_owned(),
                None => format!("Preset ({})", preset.texture_quality().label()),
            };
            egui::ComboBox::from_id_salt("texture quality")
                .selected_text(label(settings.texture_quality))
                .show_ui(ui, |ui| {
                    for choice in std::iter::once(None).chain(TextureQuality::ALL.map(Some)) {
                        ui.selectable_value(&mut settings.texture_quality, choice, label(choice));
                    }
                });
            ui.end_row();

            found_label(ui, "Texture memory");
            ui.horizontal(|ui| {
                let mut custom = settings.texture_budget.is_some();
                if ui
                    .checkbox(&mut custom, "")
                    .on_hover_text("Off, the preset's budget is used.")
                    .changed()
                {
                    settings.texture_budget = custom.then(|| preset.texture_budget());
                }
                match &mut settings.texture_budget {
                    Some(budget) => {
                        ui.add(
                            egui::DragValue::new(budget)
                                .range(TEXTURE_BUDGET_RANGE)
                                .speed(4.0)
                                .suffix(" MB"),
                        );
                    }
                    None => {
                        ui.label(
                            egui::RichText::new(format!("Preset ({} MB)", preset.texture_budget()))
                                .color(palette.text_secondary),
                        );
                    }
                }
            });
            ui.end_row();

            found_label(ui, "Anti-aliasing");
            let label = |mode: Option<AntiAliasing>| match mode {
                Some(mode) => mode.label().to_owned(),
                None => format!("Preset ({})", preset.anti_aliasing().label()),
            };
            egui::ComboBox::from_id_salt("anti-aliasing")
                .selected_text(label(settings.anti_aliasing))
                .show_ui(ui, |ui| {
                    for choice in std::iter::once(None).chain(AntiAliasing::ALL.map(Some)) {
                        ui.selectable_value(&mut settings.anti_aliasing, choice, label(choice));
                    }
                });
            ui.end_row();

            found_label(ui, "Ambient occlusion");
            let label = |mode: Option<AmbientOcclusion>| match mode {
                Some(mode) => mode.label().to_owned(),
                None => format!("Preset ({})", preset.ambient_occlusion().label()),
            };
            egui::ComboBox::from_id_salt("ambient occlusion")
                .selected_text(label(settings.ambient_occlusion))
                .show_ui(ui, |ui| {
                    for choice in std::iter::once(None).chain(AmbientOcclusion::ALL.map(Some)) {
                        ui.selectable_value(&mut settings.ambient_occlusion, choice, label(choice));
                    }
                });
            ui.end_row();

            found_label(ui, "Reflection quality");
            let label = |quality: Option<ReflectionQuality>| match quality {
                Some(quality) => quality.label().to_owned(),
                None => format!("Preset ({})", preset.reflection_quality().label()),
            };
            egui::ComboBox::from_id_salt("reflection quality")
                .selected_text(label(settings.reflection_quality))
                .show_ui(ui, |ui| {
                    for choice in std::iter::once(None).chain(ReflectionQuality::ALL.map(Some)) {
                        ui.selectable_value(
                            &mut settings.reflection_quality,
                            choice,
                            label(choice),
                        );
                    }
                });
            ui.end_row();

            found_label(ui, "Cloud quality");
            let label = |quality: Option<CloudQuality>| match quality {
                Some(quality) => quality.label().to_owned(),
                None => format!("Preset ({})", preset.cloud_quality().label()),
            };
            egui::ComboBox::from_id_salt("cloud quality")
                .selected_text(label(settings.cloud_quality))
                .show_ui(ui, |ui| {
                    for choice in std::iter::once(None).chain(CloudQuality::ALL.map(Some)) {
                        ui.selectable_value(&mut settings.cloud_quality, choice, label(choice));
                    }
                });
            ui.end_row();

            found_label(ui, "Contact shadows");
            let on_off = |on: bool| if on { "On" } else { "Off" };
            let label = |on: Option<bool>| match on {
                Some(on) => on_off(on).to_owned(),
                None => format!("Preset ({})", on_off(preset.contact_shadows())),
            };
            egui::ComboBox::from_id_salt("contact shadows")
                .selected_text(label(settings.contact_shadows))
                .show_ui(ui, |ui| {
                    for choice in [None, Some(true), Some(false)] {
                        ui.selectable_value(&mut settings.contact_shadows, choice, label(choice));
                    }
                });
            ui.end_row();

            found_label(ui, "Real-time GI");
            let label = |on: Option<bool>| match on {
                Some(on) => on_off(on).to_owned(),
                None => format!("Preset ({})", on_off(preset.realtime_gi())),
            };
            egui::ComboBox::from_id_salt("realtime gi")
                .selected_text(label(settings.realtime_gi))
                .show_ui(ui, |ui| {
                    for choice in [None, Some(true), Some(false)] {
                        ui.selectable_value(&mut settings.realtime_gi, choice, label(choice));
                    }
                });
            ui.end_row();

            found_label(ui, "Volumetric fog");
            let label = |on: Option<bool>| match on {
                Some(on) => on_off(on).to_owned(),
                None => format!("Preset ({})", on_off(preset.volumetric_fog())),
            };
            egui::ComboBox::from_id_salt("volumetric fog")
                .selected_text(label(settings.volumetric_fog))
                .show_ui(ui, |ui| {
                    for choice in [None, Some(true), Some(false)] {
                        ui.selectable_value(&mut settings.volumetric_fog, choice, label(choice));
                    }
                });
            ui.end_row();

            found_label(ui, "Depth of field");
            let label = |quality: Option<DepthOfFieldQuality>| match quality {
                Some(quality) => quality.label().to_owned(),
                None => format!("Preset ({})", preset.depth_of_field().label()),
            };
            egui::ComboBox::from_id_salt("depth of field")
                .selected_text(label(settings.depth_of_field))
                .show_ui(ui, |ui| {
                    for choice in std::iter::once(None).chain(DepthOfFieldQuality::ALL.map(Some)) {
                        ui.selectable_value(&mut settings.depth_of_field, choice, label(choice));
                    }
                });
            ui.end_row();

            found_label(ui, "Motion blur");
            let label = |quality: Option<MotionBlurQuality>| match quality {
                Some(quality) => quality.label().to_owned(),
                None => format!("Preset ({})", preset.motion_blur().label()),
            };
            egui::ComboBox::from_id_salt("motion blur")
                .selected_text(label(settings.motion_blur))
                .show_ui(ui, |ui| {
                    for choice in std::iter::once(None).chain(MotionBlurQuality::ALL.map(Some)) {
                        ui.selectable_value(&mut settings.motion_blur, choice, label(choice));
                    }
                });
            ui.end_row();

            let switches = [
                ("Vignette", preset.vignette(), &mut settings.vignette),
                ("Film grain", preset.film_grain(), &mut settings.film_grain),
                (
                    "Chromatic aberration",
                    preset.chromatic_aberration(),
                    &mut settings.chromatic_aberration,
                ),
                ("Sharpen", preset.sharpen(), &mut settings.sharpen),
            ];
            for (name, preset_on, value) in switches {
                found_label(ui, name);
                let label = |on: Option<bool>| match on {
                    Some(on) => on_off(on).to_owned(),
                    None => format!("Preset ({})", on_off(preset_on)),
                };
                egui::ComboBox::from_id_salt(name)
                    .selected_text(label(*value))
                    .show_ui(ui, |ui| {
                        for choice in [None, Some(true), Some(false)] {
                            ui.selectable_value(value, choice, label(choice));
                        }
                    });
                ui.end_row();
            }
        });
    ui.add_space(space::L);
    section(ui, "In use");
    let effective = settings.effective(capabilities);
    let shadows = match effective.shadow_quality.map() {
        Some(map) => format!(
            "Shadows: {}, a {}² map with {} cascades, up to {} studs",
            effective.shadow_quality.label(),
            map.size,
            map.cascades,
            effective.shadow_distance
        ),
        None => "Shadows: off".to_owned(),
    };
    ui.label(shadows);
    ui.label(if effective.post_processing {
        "Post-processing: on (bloom and tone mapping)"
    } else {
        "Post-processing: off"
    });
    ui.label(format!(
        "Scene drawn at {}% of the view's size; the UI always at 100%",
        (effective.render_scale * 100.0).round()
    ));
    ui.label(format!(
        "Textures: {} size, within {} MB (the least recently drawn lose detail first)",
        effective.texture_quality.label().to_lowercase(),
        effective.texture_budget >> 20
    ));
    ui.label(format!(
        "Anti-aliasing: {}",
        match effective.anti_aliasing {
            AntiAliasing::Off => "off".to_owned(),
            mode => mode.label().to_owned(),
        }
    ));
    ui.label(match effective.ambient_occlusion {
        AmbientOcclusion::Off => "Ambient occlusion: off".to_owned(),
        mode => format!(
            "Ambient occlusion: {}, {} samples per pixel",
            mode.label(),
            mode.samples()
        ),
    });
    ui.label(match effective.reflection_quality.map() {
        Some(map) => format!(
            "Reflections: {}, the sky baked into a {}² cubemap ({} samples)",
            effective.reflection_quality.label(),
            map.size,
            map.samples
        ),
        None => "Reflections: Low, from the sky's gradient (no sky image)".to_owned(),
    });
    ui.label(if effective.contact_shadows {
        "Contact shadows: on (small gaps the shadow maps miss)"
    } else {
        "Contact shadows: off"
    });
    ui.label(if effective.realtime_gi {
        "Real-time GI: on (light probe volumes traced again, two probes a frame)"
    } else {
        "Real-time GI: off (light probe volumes keep their bakes)"
    });
    ui.label(match effective.cloud_quality.steps() {
        (0, _) => "Clouds: Low, a flat sheet".to_owned(),
        (steps, _) => format!(
            "Clouds: {}, looked through in {steps} steps",
            effective.cloud_quality.label()
        ),
    });
    ui.label(if effective.volumetric_fog {
        "Volumetric fog: on (sun shafts and lamp light in an Atmosphere's fog)"
    } else {
        "Volumetric fog: off (fog without shadows in it)"
    });
    ui.label(match effective.depth_of_field {
        DepthOfFieldQuality::Off => "Depth of field: off".to_owned(),
        quality => format!(
            "Depth of field: {}, {} samples per pixel",
            quality.label(),
            quality.samples()
        ),
    });
    ui.label(match effective.motion_blur {
        MotionBlurQuality::Off => "Motion blur: off".to_owned(),
        quality => format!(
            "Motion blur: {}, {} samples per pixel",
            quality.label(),
            quality.samples()
        ),
    });
    let lens: Vec<&str> = [
        (effective.vignette, "vignette"),
        (effective.film_grain, "film grain"),
        (effective.chromatic_aberration, "chromatic aberration"),
        (effective.sharpen, "sharpening"),
    ]
    .into_iter()
    .filter_map(|(on, name)| on.then_some(name))
    .collect();
    ui.label(if lens.is_empty() {
        "Lens effects: none".to_owned()
    } else {
        format!("Lens effects: {}", lens.join(", "))
    });
    for note in &effective.notes {
        ui.label(egui::RichText::new(note).color(palette.warning));
    }
    ui.label(
        egui::RichText::new(
            "A world's Lighting can ask for less (ShadowsEnabled, ShadowDistance, \
             PostProcessingEnabled), never more.",
        )
        .color(palette.text_secondary),
    );
}

fn accessibility_page(ui: &mut egui::Ui, appearance: &mut Appearance) {
    page_title(
        ui,
        "Accessibility",
        "Motion, contrast, and other ways to make the editor easier to use.",
    );
    let palette = theme::palette(ui.ctx());
    section(ui, "Motion");
    let row = ui.checkbox(&mut appearance.reduce_motion, "Reduce motion");
    found(ui, &row, "Reduce motion");
    ui.label(
        egui::RichText::new(
            "Turns off hover fades, popup and window fades, smooth scrolling and the theme \
             crossfade. The viewport camera is never animated by the interface.",
        )
        .color(palette.text_secondary),
    );
    ui.add_space(space::L);
    section(ui, "Contrast");
    let row = ui.checkbox(&mut appearance.increase_contrast, "Increase contrast");
    found(ui, &row, "Increase contrast");
    ui.label(
        egui::RichText::new(
            "Stronger outlines around buttons and fields, stronger borders and faint text, a \
             thicker focus ring, and block colours dark enough for their white labels.",
        )
        .color(palette.text_secondary),
    );
    ui.add_space(space::L);
    section(ui, "Display");
    ui.horizontal(|ui| {
        found_label(ui, "Steady frame rate");
        egui::ComboBox::from_id_salt("steady frame rate")
            .selected_text(Appearance::steady_rate_name(appearance.steady_hz))
            .show_ui(ui, |ui| {
                for hz in Appearance::STEADY_RATES {
                    ui.selectable_value(
                        &mut appearance.steady_hz,
                        hz,
                        Appearance::steady_rate_name(hz),
                    );
                }
            });
    });
    ui.label(
        egui::RichText::new(
            "While you work, frames are shown at this fixed rate rather than as fast as each \
             one is ready, so a FreeSync or G-Sync display holds one refresh rate. OLED and \
             VA monitors flicker when the rate swings; if yours still does, pick a lower rate. \
             The display's own rate has no limit.",
        )
        .color(palette.text_secondary),
    );
    ui.add_space(space::S);
    let row = ui.checkbox(&mut appearance.hold_refresh, "Hold the refresh rate steady");
    found(ui, &row, "Hold the refresh rate steady");
    ui.label(
        egui::RichText::new(
            "Keeps drawing at the steady frame rate while the editor has focus instead of \
             stopping when it's idle, for displays that flicker as the rate drops. It uses \
             more of the graphics card.",
        )
        .color(palette.text_secondary),
    );
    ui.add_space(space::L);
    section(ui, "Readability");
    ui.label(
        egui::RichText::new(
            "Text and accent colours meet WCAG AA contrast in both themes. For larger text, \
             raise the interface scale in Appearance, or press Ctrl and + (Cmd and + on macOS).",
        )
        .color(palette.text_secondary),
    );
    ui.add_space(space::L);
    section(ui, "Keyboard");
    ui.label(
        egui::RichText::new(
            "Tab moves between controls and shows a focus ring in the accent colour; Space or \
             Enter presses the focused control.",
        )
        .color(palette.text_secondary),
    );
}

/// A small picture of the editor in `choice`'s colours, to pick a theme by.
/// System shows half dark, half light.
fn theme_preview(
    ui: &mut egui::Ui,
    choice: ThemeChoice,
    accent: Accent,
    selected: bool,
) -> egui::Response {
    let size = egui::vec2(148.0, 112.0);
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
    response.widget_info(|| {
        egui::WidgetInfo::selected(
            egui::WidgetType::RadioButton,
            true,
            selected,
            choice.label(),
        )
    });
    let current = theme::palette(ui.ctx());
    let picture = egui::Rect::from_min_size(rect.min, egui::vec2(size.x, 84.0));
    match choice {
        ThemeChoice::Dark => mini_editor(ui, picture, &Palette::new(true, accent)),
        ThemeChoice::Light => mini_editor(ui, picture, &Palette::new(false, accent)),
        ThemeChoice::System => {
            let half = picture.center().x;
            let mut left = ui.new_child(egui::UiBuilder::new().max_rect(picture));
            left.set_clip_rect(picture.with_max_x(half));
            mini_editor(&left, picture, &Palette::new(true, accent));
            let mut right = ui.new_child(egui::UiBuilder::new().max_rect(picture));
            right.set_clip_rect(picture.with_min_x(half));
            mini_editor(&right, picture, &Palette::new(false, accent));
        }
    }
    let t = theme::ease(
        ui.ctx(),
        response.id.with("ring"),
        selected || response.hovered(),
    );
    let ring = if selected {
        current.accent
    } else {
        theme::mix(current.border, current.border_strong, t)
    };
    ui.painter().rect_stroke(
        picture,
        8,
        egui::Stroke::new(if selected { 2.0_f32 } else { 1.0 }, ring),
        egui::StrokeKind::Outside,
    );
    let label_color = if selected {
        current.text_strong
    } else {
        current.text_secondary
    };
    ui.painter().text(
        egui::pos2(rect.left() + 2.0, picture.bottom() + 16.0),
        egui::Align2::LEFT_CENTER,
        theme::with_icon(choice.icon(), choice.label()),
        egui::FontId::new(type_scale::BODY, theme::medium_family()),
        label_color,
    );
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Paints a tiny editor: header, Explorer with a selected row, a viewport
/// and an accent button.
fn mini_editor(ui: &egui::Ui, rect: egui::Rect, p: &Palette) {
    let painter = ui.painter();
    painter.rect_filled(rect, 8, p.app_bg);
    let header = egui::Rect::from_min_size(rect.min, egui::vec2(rect.width(), 12.0));
    painter.circle_filled(
        header.left_center() + egui::vec2(8.0, 0.0),
        2.5,
        p.text_muted,
    );
    for (x, w) in [(16.0, 14.0), (34.0, 12.0), (50.0, 12.0)] {
        painter.rect_filled(
            egui::Rect::from_min_size(header.min + egui::vec2(x, 5.0), egui::vec2(w, 2.5)),
            1,
            p.text_weak,
        );
    }
    let body = egui::Rect::from_min_max(
        header.left_bottom() + egui::vec2(4.0, 0.0),
        rect.max - egui::vec2(4.0, 4.0),
    );
    let explorer = egui::Rect::from_min_size(body.min, egui::vec2(42.0, body.height()));
    painter.rect_filled(explorer, 4, p.panel);
    for row in 0..5 {
        let y = explorer.top() + 7.0 + row as f32 * 10.0;
        let line = egui::Rect::from_min_size(
            egui::pos2(explorer.left() + 5.0 + (row % 2) as f32 * 5.0, y),
            egui::vec2(24.0, 3.0),
        );
        if row == 2 {
            painter.rect_filled(
                egui::Rect::from_min_size(
                    egui::pos2(explorer.left() + 2.0, y - 3.0),
                    egui::vec2(38.0, 9.0),
                ),
                2,
                p.accent_soft,
            );
            painter.rect_filled(line, 1, p.accent_text);
        } else {
            painter.rect_filled(line, 1, p.text_muted);
        }
    }
    let viewport =
        egui::Rect::from_min_max(egui::pos2(explorer.right() + 4.0, body.top()), body.max);
    painter.rect_filled(viewport, 4, theme::mix(p.app_bg, p.panel, 0.5));
    let ground = egui::Rect::from_min_max(
        egui::pos2(viewport.left() + 14.0, viewport.center().y + 6.0),
        egui::pos2(viewport.right() - 14.0, viewport.center().y + 14.0),
    );
    painter.rect_filled(ground, 2, p.border_strong);
    painter.rect_filled(
        egui::Rect::from_center_size(
            viewport.center() - egui::vec2(0.0, 2.0),
            egui::vec2(14.0, 14.0),
        ),
        2,
        p.text_weak,
    );
    painter.rect_filled(
        egui::Rect::from_min_size(
            viewport.right_top() + egui::vec2(-26.0, 5.0),
            egui::vec2(20.0, 8.0),
        ),
        2,
        p.accent_solid,
    );
}

/// A round swatch of `accent`, ringed while it's picked.
fn accent_swatch(ui: &mut egui::Ui, accent: Accent, selected: bool) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(26.0, 26.0), egui::Sense::click());
    response.widget_info(|| {
        egui::WidgetInfo::selected(
            egui::WidgetType::RadioButton,
            true,
            selected,
            accent.label(),
        )
    });
    let current = theme::palette(ui.ctx());
    let color = Palette::new(current.dark, accent).accent;
    let center = rect.center();
    let grow = theme::ease(ui.ctx(), response.id.with("hover"), response.hovered());
    ui.painter().circle_filled(center, 9.0 + grow, color);
    if selected {
        ui.painter().circle_stroke(
            center,
            12.5,
            egui::Stroke::new(1.5_f32, current.text_strong),
        );
    }
    if response.has_focus() {
        ui.painter()
            .circle_stroke(center, 13.0, egui::Stroke::new(1.0_f32, current.accent));
    }
    response
        .on_hover_text(accent.label())
        .on_hover_cursor(egui::CursorIcon::PointingHand)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_settings_page_draws_and_changes_apply_to_the_look() {
        let ctx = theme::test_context();
        let mut app = EditorApp::new(None);
        app.settings.open = true;
        let snapshots = std::env::var_os("AXCENITY_SNAPSHOTS").map(PathBuf::from);
        for page in SettingsPage::ALL {
            app.settings.page = page;
            let mut draw = |ui: &mut egui::Ui| {
                let ctx = &ui.ctx().clone();
                egui::CentralPanel::default().show(ui, |_| {});
                app.settings_window(ctx);
            };
            match &snapshots {
                Some(folder)
                    if matches!(
                        page,
                        SettingsPage::Appearance
                            | SettingsPage::Graphics
                            | SettingsPage::Updates
                            | SettingsPage::Ai
                    ) =>
                {
                    crate::snapshot::snapshot(
                        &folder.join(format!("settings-{}.png", page.label().to_lowercase())),
                        egui::vec2(900.0, 640.0),
                        vec![Vec::new(); 3],
                        &mut draw,
                    )
                }
                _ => {
                    let _ = crate::theme::run(&ctx, Default::default(), |ui| draw(ui));
                }
            }
        }
        assert!(app.settings.open);

        let light = Appearance {
            theme: ThemeChoice::Light,
            accent: Accent::Orange,
            ..Appearance::default()
        };
        let _ = crate::theme::run(&ctx, Default::default(), |ui| {
            let ctx = &ui.ctx().clone();
            app.set_appearance(ctx, light.clone())
        });
        assert_eq!(app.appearance, light);
        assert!(!ctx.global_style().visuals.dark_mode);
        assert_eq!(
            theme::palette(&ctx).accent,
            Palette::new(false, Accent::Orange).accent
        );
    }

    /// eframe's saved settings, in memory.
    #[derive(Default)]
    struct MemoryStorage(std::collections::HashMap<String, String>);

    impl eframe::Storage for MemoryStorage {
        fn get_string(&self, key: &str) -> Option<String> {
            self.0.get(key).cloned()
        }
        fn set_string(&mut self, key: &str, value: String) {
            self.0.insert(key.to_owned(), value);
        }
        fn remove_string(&mut self, key: &str) {
            self.0.remove(key);
        }
        fn flush(&mut self) {}
    }

    /// Layouts saved by name (design system V17) are there after a restart.
    #[test]
    fn saved_layouts_survive_a_restart() {
        let ctx = theme::test_context();
        let mut app = EditorApp::new(None);
        app.layout.use_preset(crate::layout::Preset::Modeling);
        app.save_layout("Props").unwrap();
        let mut storage = MemoryStorage::default();
        eframe::App::save(&mut app, &mut storage);

        let mut restarted = EditorApp::new(None);
        assert!(restarted.saved_layouts.0.is_empty());
        restarted.load_settings(&ctx, Some(&storage));
        assert_eq!(restarted.saved_layouts, app.saved_layouts);
        restarted.use_saved_layout("Props");
        assert!(restarted.layout.is_open(Pane::ModelStudio));
    }

    /// The movement guide is on until it's turned off, and stays off after
    /// a restart; the other viewport guides keep their own choices.
    #[test]
    fn the_movement_guide_choice_survives_a_restart() {
        let ctx = theme::test_context();
        let mut app = EditorApp::new(None);
        assert!(app.move_guide, "on by default");
        app.move_guide = false;
        let mut storage = MemoryStorage::default();
        eframe::App::save(&mut app, &mut storage);

        let mut restarted = EditorApp::new(None);
        restarted.load_settings(&ctx, Some(&storage));
        assert!(!restarted.move_guide);
        assert!(restarted.grid && restarted.view_cube);

        restarted.move_guide = true;
        eframe::App::save(&mut restarted, &mut storage);
        let mut again = EditorApp::new(None);
        again.load_settings(&ctx, Some(&storage));
        assert!(again.move_guide);
    }

    #[test]
    fn graphics_settings_survive_a_restart() {
        let ctx = theme::test_context();
        let mut app = EditorApp::new(None);
        app.graphics = GraphicsSettings {
            quality: Quality::Medium,
            automatic: false,
            shadow_quality: Some(ShadowQuality::Off),
            shadow_distance: Some(60.0),
            post_processing: None,
            render_scale: Some(0.5),
            texture_quality: Some(TextureQuality::Half),
            texture_budget: Some(128),
            anti_aliasing: Some(AntiAliasing::Taa),
            ambient_occlusion: Some(AmbientOcclusion::Low),
            reflection_quality: Some(ReflectionQuality::Medium),
            contact_shadows: Some(false),
            realtime_gi: Some(true),
            volumetric_fog: Some(true),
            cloud_quality: Some(CloudQuality::Ultra),
            depth_of_field: Some(DepthOfFieldQuality::Off),
            vignette: Some(false),
            film_grain: Some(true),
            chromatic_aberration: None,
            sharpen: Some(false),
            motion_blur: Some(MotionBlurQuality::Low),
        };
        let mut storage = MemoryStorage::default();
        eframe::App::save(&mut app, &mut storage);

        let mut restarted = EditorApp::new(None);
        assert_eq!(restarted.graphics, GraphicsSettings::default());
        restarted.load_settings(&ctx, Some(&storage));
        assert_eq!(restarted.graphics, app.graphics);
    }

    #[test]
    fn changing_the_graphics_setting_changes_the_viewport_at_once() {
        let mut app = EditorApp::new(Some(&super::super::tests::example("hello-graphics")));
        let mut frame = 0;
        let mut seen = Vec::new();
        crate::snapshot::snapshot_rendered(
            &std::env::temp_dir().join(format!("axcenity-graphics-{}.png", std::process::id())),
            egui::vec2(320.0, 200.0),
            vec![Vec::new(); 3],
            |ui, render_state| {
                match frame {
                    1 => app.graphics.shadow_quality = Some(ShadowQuality::Off),
                    2 => app.graphics.anti_aliasing = Some(AntiAliasing::Fxaa),
                    _ => {}
                }
                frame += 1;
                egui::CentralPanel::default().show(ui, |ui| {
                    app.scene_pane(ui, Some(render_state));
                });
                seen.push(
                    app.viewport
                        .graphics()
                        .map(|used| (used.shadow_quality, used.anti_aliasing)),
                );
            },
        );
        if seen.is_empty() {
            return; // No GPU.
        }
        let (high, off) = (ShadowQuality::High, ShadowQuality::Off);
        assert_eq!(
            seen,
            [
                Some((high, AntiAliasing::Msaa4)),
                Some((off, AntiAliasing::Msaa4)),
                Some((off, AntiAliasing::Fxaa))
            ]
        );
    }

    /// Lighting's Exposure and ToneMapping, set as the Properties panel sets
    /// them, change what the viewport draws.
    #[test]
    fn exposure_and_tone_mapping_set_in_properties_change_the_viewport() {
        use axcenity_core::{ToneMapping, Value};
        let mut app = EditorApp::new(Some(&super::super::tests::example("hello-graphics")));
        let lighting = app.world.lighting();
        let mut frame = 0;
        let mut drawn = Vec::new();
        crate::snapshot::snapshot_rendered(
            &std::env::temp_dir().join(format!("axcenity-look-{}.png", std::process::id())),
            egui::vec2(320.0, 200.0),
            vec![Vec::new(); 3],
            |ui, render_state| {
                match frame {
                    1 => app.apply(lighting, Edit::Property("Exposure", Value::Number(2.0))),
                    2 => app.apply(
                        lighting,
                        Edit::Property("ToneMapping", Value::ToneMapping(ToneMapping::Filmic)),
                    ),
                    _ => {}
                }
                frame += 1;
                egui::CentralPanel::default().show(ui, |ui| {
                    app.scene_pane(ui, Some(render_state));
                });
                drawn.extend(app.viewport.pixels());
            },
        );
        if drawn.is_empty() {
            return; // No GPU.
        }
        let [before, brighter, filmic] = &drawn[..] else {
            panic!("drew {} frames, not 3", drawn.len());
        };
        let changed = |a: &[u8], b: &[u8]| {
            a.chunks(4)
                .zip(b.chunks(4))
                .filter(|(a, b)| a[..3] != b[..3])
                .count()
        };
        let pixels = before.len() / 4;
        let exposed = changed(before, brighter);
        assert!(
            exposed > pixels / 4,
            "exposure changed {exposed} of {pixels}"
        );
        let curved = changed(brighter, filmic);
        assert!(
            curved > pixels / 4,
            "tone mapping changed {curved} of {pixels}"
        );
    }

    /// hello-graphics' LocalScript steps the quality on G. In Play that
    /// changes the viewport, starting from the editor's own settings, and
    /// Stop puts them back without saving the script's choice.
    #[test]
    fn a_local_scripts_graphics_change_lasts_until_stop() {
        use axcenity_script::KeyEvent;
        let mut app = EditorApp::new(Some(&super::super::tests::example("hello-graphics")));
        app.play();
        let press = KeyEvent {
            key: axcenity_core::KeyCode::G,
            pressed: true,
        };
        let mut frame = 0;
        let mut seen = Vec::new();
        crate::snapshot::snapshot_rendered(
            &std::env::temp_dir()
                .join(format!("axcenity-play-graphics-{}.png", std::process::id())),
            egui::vec2(320.0, 200.0),
            vec![Vec::new(); 3],
            |ui, render_state| {
                match frame {
                    1 => app.play_frame(1.0 / 60.0, &[press.into()], None, None),
                    2 => app.stop(),
                    _ => app.play_frame(1.0 / 60.0, &[], None, None),
                }
                frame += 1;
                egui::CentralPanel::default().show(ui, |ui| {
                    app.scene_pane(ui, Some(render_state));
                });
                seen.push(app.viewport.graphics().map(|used| used.shadow_quality));
            },
        );
        if seen.is_empty() {
            return; // No GPU.
        }
        // The editor's High, then Ultra (the one after it), then High again.
        assert_eq!(
            seen,
            [
                Some(ShadowQuality::High),
                Some(ShadowQuality::Ultra),
                Some(ShadowQuality::High)
            ]
        );
        assert_eq!(app.graphics, GraphicsSettings::default());
        assert!(
            app.output
                .lines()
                .iter()
                .any(|line| line.text == "Graphics quality: Ultra"),
            "{:?}",
            app.output.lines()
        );
    }
}
