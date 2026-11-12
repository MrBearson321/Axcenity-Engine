//! The rows universal search can find in the Settings window, and how a
//! found row is scrolled to and outlined.

use super::*;

/// A row of the Settings window that universal search lists
/// (`app/search.rs`): picking it opens its page, scrolls to the row and
/// outlines it for a moment ([`find_setting`]).
pub(in crate::app) struct SearchableSetting {
    pub page: SettingsPage,
    /// What the row says: the label, section or checkbox it's found by.
    pub name: &'static str,
    /// Other words for it.
    pub aliases: &'static [&'static str],
    pub description: &'static str,
}

pub(super) const fn setting(
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
pub(in crate::app) const SEARCHABLE: &[SearchableSetting] = {
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
            Appearance,
            "Menu icons",
            &["Icon alignment", "Centered icons", "Left icons", "Menus"],
            "Whether menu icons sit at the left of their column or in its middle.",
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
            Saving,
            "Keep a copy of unsaved work",
            &[
                "AutoRecover",
                "Autosave",
                "Auto save",
                "Crash recovery",
                "Power outage",
            ],
            "How often unsaved work is copied, so a crash or power cut doesn't lose it.",
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
            "Show contacts",
            &["Physics debug", "Collisions", "Contact points"],
            "Where parts touch and how hard they push, during Play.",
        ),
        setting(
            Viewport,
            "Show bodies",
            &["Physics debug", "Center of mass", "Sleeping"],
            "Each moving body's center of mass, awake or asleep, during Play.",
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
            Project,
            "Memory budget",
            &[
                "Memory limit",
                "RAM budget",
                "Lua memory",
                "Profiler warning",
            ],
            "Limits the Profiler compares the playing game's memory with, saved in the project.",
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
                "Your name in Collaborate",
                "Collaborator colour",
                "Identity",
                "Device key",
            ],
            "How others see you in a Collaborate session.",
        ),
        setting(
            TeamCreate,
            "New invites",
            &["Invite link", "Invite expiry", "Default invite role"],
            "The role new Collaborate invites give, and how long they work.",
        ),
    ]
};

/// What [`find_setting`] asked for: the row, when, and whether it was
/// scrolled to yet.
#[derive(Clone)]
pub(super) struct Finding {
    name: &'static str,
    since: f64,
    scrolled: bool,
}

/// How long a found row stays outlined, in seconds.
pub(super) const FOUND_SECONDS: f64 = 2.0;

pub(super) fn finding_id() -> egui::Id {
    egui::Id::new("settings find")
}

/// Scrolls the Settings window to the row called `name` the next time its
/// page is drawn, and outlines it for a moment.
pub(in crate::app) fn find_setting(ctx: &egui::Context, name: &'static str) {
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
pub(in crate::app) fn section(ui: &mut egui::Ui, name: &str) {
    let palette = theme::palette(ui.ctx());
    let row = ui.label(widgets::micro(name, palette.text_weak));
    found(ui, &row, name);
}

/// A row's label that search can find.
pub(super) fn found_label(ui: &mut egui::Ui, name: &str) {
    let row = ui.label(name);
    found(ui, &row, name);
}

/// If search asked for the row `name` (drawn as `row`), scrolls to it once
/// and outlines it in the accent, fading.
pub(super) fn found(ui: &egui::Ui, row: &egui::Response, name: &str) {
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
