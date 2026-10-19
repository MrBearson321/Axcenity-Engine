//! The editor's look: the brand colours, the fonts (Montserrat for the UI,
//! JetBrains Mono for code, Phosphor for icons), dark and light styles, and
//! an icon and colour for each class of Instance.
//!
//! Icons are letters in the Phosphor icon font, so they go inside any text,
//! such as `format!("{} Save", icons::FLOPPY_DISK)`, and take its size and colour.

use std::sync::Arc;

use axcenity_core::{ClassName, PartType};
use eframe::egui::{self, Color32, CornerRadius, FontData, FontFamily, FontId, Stroke, TextStyle};

use crate::gizmo::Tool;
use crate::tree::Insertable;

/// The solid icons. They have the same letters as [`icons`], so they only
/// show as solid in the [`filled`] font family.
pub use egui_phosphor::fill as filled_icons;
/// The outline icons, used everywhere unless a solid one reads better.
pub use egui_phosphor::regular as icons;

/// The logo's colours (see `assets/branding/README.md`).
pub const BRAND_BLUE: Color32 = Color32::from_rgb(0, 74, 173);
/// The blue of the dark-theme logo: brand blue is too dark on a dark background.
pub const BRAND_BLUE_BRIGHT: Color32 = Color32::from_rgb(77, 134, 232);
pub const BRAND_YELLOW: Color32 = Color32::from_rgb(249, 191, 59);

/// Play's green and Stop's red, readable in both themes.
pub const PLAY_GREEN: Color32 = Color32::from_rgb(46, 175, 110);
pub const STOP_RED: Color32 = Color32::from_rgb(226, 78, 78);

/// The font family of headings and panel titles (Montserrat SemiBold).
pub fn heading_family() -> FontFamily {
    FontFamily::Name("heading".into())
}

/// The font family that draws [`filled_icons`] solid.
pub fn filled() -> FontFamily {
    FontFamily::Name("filled icons".into())
}

/// Sets the fonts and both themes' styles. Called once, when the window opens.
pub fn install(ctx: &egui::Context) {
    ctx.set_fonts(fonts());
    for (theme, visuals) in [
        (egui::Theme::Dark, dark_visuals()),
        (egui::Theme::Light, light_visuals()),
    ] {
        ctx.set_visuals_of(theme, visuals);
        ctx.style_mut_of(theme, set_sizes);
    }
}

/// Montserrat and JetBrains Mono first, with Phosphor after each for icons,
/// then egui's own fonts for anything they don't have (such as ⌫).
fn fonts() -> egui::FontDefinitions {
    // `static` font files are built into the program, like the window icon.
    let font = |bytes: &'static [u8]| Arc::new(FontData::from_static(bytes));
    let mut fonts = egui::FontDefinitions::default();
    for (name, data) in [
        (
            "Montserrat",
            font(include_bytes!(
                "../../../assets/fonts/Montserrat-Regular.ttf"
            )),
        ),
        (
            "Montserrat SemiBold",
            font(include_bytes!(
                "../../../assets/fonts/Montserrat-SemiBold.ttf"
            )),
        ),
        (
            "JetBrains Mono",
            font(include_bytes!(
                "../../../assets/fonts/JetBrainsMono-Regular.ttf"
            )),
        ),
        (
            "Phosphor",
            Arc::new(egui_phosphor::Variant::Regular.font_data()),
        ),
        (
            "Phosphor Fill",
            Arc::new(egui_phosphor::Variant::Fill.font_data()),
        ),
    ] {
        fonts.font_data.insert(name.to_string(), data);
    }

    let fallbacks = fonts.families[&FontFamily::Proportional].clone();
    let family = |first: &str| {
        let mut names = vec![first.to_string(), "Phosphor".to_string()];
        names.extend(fallbacks.iter().cloned());
        names
    };
    fonts
        .families
        .insert(FontFamily::Proportional, family("Montserrat"));
    fonts
        .families
        .insert(FontFamily::Monospace, family("JetBrains Mono"));
    fonts
        .families
        .insert(heading_family(), family("Montserrat SemiBold"));
    // Every family needs a font with `◻` or `?` in it, which egui draws
    // for characters no font has; without one it warns at startup.
    fonts.families.insert(filled(), family("Phosphor Fill"));
    fonts
}

/// Text sizes, and roomier spacing than egui's, so things are easier to see and hit.
fn set_sizes(style: &mut egui::Style) {
    style.text_styles = [
        (TextStyle::Small, FontId::proportional(10.5)),
        (TextStyle::Body, FontId::proportional(13.0)),
        (TextStyle::Button, FontId::proportional(13.0)),
        (TextStyle::Heading, FontId::new(19.0, heading_family())),
        (TextStyle::Monospace, FontId::monospace(13.0)),
    ]
    .into();
    let spacing = &mut style.spacing;
    spacing.item_spacing = egui::vec2(8.0, 5.0);
    spacing.button_padding = egui::vec2(8.0, 3.0);
    spacing.interact_size.y = 22.0;
    spacing.menu_margin = egui::Margin::same(6);
    spacing.window_margin = egui::Margin::same(14);
    spacing.indent = 16.0;
}

/// The colours of one theme's widgets: idle, hovered and pressed.
struct WidgetColors {
    idle: Color32,
    hovered: Color32,
    pressed: Color32,
    open: Color32,
    /// The outline of hovered and pressed widgets.
    outline: Color32,
    /// Lines between things, and text that isn't on a widget.
    line: Color32,
    text: Color32,
    strong_text: Color32,
}

fn set_widgets(visuals: &mut egui::Visuals, colors: WidgetColors) {
    let radius = CornerRadius::same(6);
    let widgets = &mut visuals.widgets;
    widgets.noninteractive.bg_fill = visuals.panel_fill;
    widgets.noninteractive.weak_bg_fill = visuals.panel_fill;
    widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, colors.line);
    widgets.noninteractive.fg_stroke = Stroke::new(1.0_f32, colors.text);
    widgets.noninteractive.corner_radius = radius;
    for (state, fill, outline) in [
        (&mut widgets.inactive, colors.idle, Color32::TRANSPARENT),
        (&mut widgets.hovered, colors.hovered, colors.outline),
        (&mut widgets.active, colors.pressed, colors.outline),
        (&mut widgets.open, colors.open, Color32::TRANSPARENT),
    ] {
        state.bg_fill = fill;
        state.weak_bg_fill = fill;
        state.bg_stroke = Stroke::new(1.0_f32, outline);
        state.corner_radius = radius;
    }
    widgets.inactive.fg_stroke = Stroke::new(1.0_f32, colors.text);
    widgets.open.fg_stroke = Stroke::new(1.0_f32, colors.text);
    widgets.hovered.fg_stroke = Stroke::new(1.5_f32, colors.strong_text);
    widgets.active.fg_stroke = Stroke::new(2.0_f32, colors.strong_text);
    visuals.window_corner_radius = CornerRadius::same(10);
    visuals.menu_corner_radius = CornerRadius::same(8);
    visuals.slider_trailing_fill = true;
}

/// The dark theme: blue-grey panels, so the viewport stands out.
fn dark_visuals() -> egui::Visuals {
    let rgb = Color32::from_rgb;
    let mut visuals = egui::Visuals::dark();
    visuals.panel_fill = rgb(30, 32, 39);
    visuals.window_fill = rgb(37, 40, 49);
    visuals.window_stroke = Stroke::new(1.0_f32, rgb(55, 60, 73));
    visuals.extreme_bg_color = rgb(21, 23, 28);
    visuals.text_edit_bg_color = Some(rgb(23, 25, 31));
    visuals.faint_bg_color = rgb(35, 38, 46);
    visuals.code_bg_color = rgb(26, 28, 34);
    visuals.hyperlink_color = rgb(110, 162, 255);
    visuals.warn_fg_color = BRAND_YELLOW;
    visuals.error_fg_color = rgb(255, 110, 110);
    visuals.selection.bg_fill = rgb(37, 84, 168);
    visuals.selection.stroke = Stroke::new(1.0_f32, rgb(225, 235, 255));
    visuals.window_shadow = egui::Shadow {
        offset: [0, 8],
        blur: 28,
        spread: 0,
        color: Color32::from_black_alpha(120),
    };
    set_widgets(
        &mut visuals,
        WidgetColors {
            idle: rgb(44, 48, 58),
            hovered: rgb(54, 59, 72),
            pressed: rgb(63, 69, 85),
            open: rgb(48, 52, 64),
            outline: rgb(84, 112, 170),
            line: rgb(48, 52, 62),
            text: rgb(205, 209, 219),
            strong_text: rgb(245, 247, 252),
        },
    );
    visuals
}

/// The light theme: soft grey panels on white.
fn light_visuals() -> egui::Visuals {
    let rgb = Color32::from_rgb;
    let mut visuals = egui::Visuals::light();
    visuals.panel_fill = rgb(243, 244, 247);
    visuals.window_fill = Color32::WHITE;
    visuals.window_stroke = Stroke::new(1.0_f32, rgb(212, 216, 224));
    visuals.extreme_bg_color = Color32::WHITE;
    visuals.text_edit_bg_color = Some(Color32::WHITE);
    visuals.faint_bg_color = rgb(235, 237, 241);
    visuals.code_bg_color = rgb(236, 238, 243);
    visuals.hyperlink_color = BRAND_BLUE;
    visuals.warn_fg_color = rgb(168, 100, 0);
    visuals.error_fg_color = rgb(200, 40, 40);
    visuals.selection.bg_fill = rgb(194, 214, 246);
    visuals.selection.stroke = Stroke::new(1.0_f32, rgb(0, 50, 125));
    visuals.window_shadow = egui::Shadow {
        offset: [0, 8],
        blur: 28,
        spread: 0,
        color: Color32::from_black_alpha(40),
    };
    set_widgets(
        &mut visuals,
        WidgetColors {
            idle: rgb(228, 231, 237),
            hovered: rgb(216, 221, 230),
            pressed: rgb(202, 210, 224),
            open: rgb(222, 226, 234),
            outline: rgb(120, 150, 205),
            line: rgb(218, 221, 228),
            text: rgb(40, 43, 51),
            strong_text: rgb(10, 12, 18),
        },
    );
    visuals
}

/// The icon of `class` in the Explorer, the tabs and the Insert buttons.
pub fn class_icon(class: ClassName) -> &'static str {
    match class {
        ClassName::DataModel => icons::GAME_CONTROLLER,
        ClassName::Workspace => icons::GLOBE_HEMISPHERE_WEST,
        ClassName::Players => icons::USERS,
        ClassName::Lighting => icons::SUN,
        ClassName::ReplicatedStorage => icons::ARROWS_LEFT_RIGHT,
        ClassName::ServerScriptService => icons::HARD_DRIVES,
        ClassName::StarterPlayer => icons::USER_CIRCLE,
        ClassName::StarterPlayerScripts => icons::USER_GEAR,
        ClassName::Part => icons::CUBE,
        ClassName::MeshPart => icons::CUBE_TRANSPARENT,
        ClassName::Script => icons::FILE_CODE,
        ClassName::LocalScript => icons::CODE,
        ClassName::ModuleScript => icons::PUZZLE_PIECE,
        ClassName::RemoteEvent => icons::BROADCAST,
        ClassName::Player => icons::USER,
        ClassName::Model => icons::PACKAGE,
        ClassName::Folder => icons::FOLDER,
        ClassName::SurfaceAppearance => icons::IMAGE,
        ClassName::ScreenGui => icons::GAME_CONTROLLER,
        ClassName::Frame => icons::CUBE,
        ClassName::TextLabel => icons::FILE_CODE,
        ClassName::TextButton => icons::CURSOR,
        ClassName::ImageLabel => icons::IMAGE,
        ClassName::TextBox => icons::TEXT_T,
        ClassName::PageLayout => icons::STACK,
        ClassName::Table => icons::TABLE,
        ClassName::BillboardGui => icons::FLAG,
        ClassName::Sound => icons::MUSIC_NOTE,
        ClassName::PointLight => icons::LIGHTBULB,
        ClassName::SpotLight => icons::FLASHLIGHT,
    }
}

/// The icon of a tool in the toolbar.
pub fn tool_icon(tool: Tool) -> &'static str {
    match tool {
        Tool::Select => icons::CURSOR,
        Tool::Move => icons::ARROWS_OUT_CARDINAL,
        Tool::Scale => icons::ARROWS_OUT,
        Tool::Rotate => icons::ARROWS_CLOCKWISE,
    }
}

/// The icon of something to insert: parts by their shape.
pub fn insert_icon(what: Insertable) -> &'static str {
    match what {
        Insertable::Part(shape) => part_icon(shape),
        _ => class_icon(what.class()),
    }
}

/// The icon of a part with this shape.
pub fn part_icon(shape: PartType) -> &'static str {
    match shape {
        PartType::Block => icons::CUBE,
        PartType::Ball => icons::SPHERE,
        PartType::Cylinder => icons::CYLINDER,
        PartType::Wedge => icons::TRIANGLE,
    }
}

/// The colour of `class`'s icon, so kinds of things are easy to tell apart
/// at a glance. Services are grey: they are always there.
pub fn class_color(class: ClassName, visuals: &egui::Visuals) -> Color32 {
    let dark = visuals.dark_mode;
    let pick = |on_dark: (u8, u8, u8), on_light: (u8, u8, u8)| {
        let (r, g, b) = if dark { on_dark } else { on_light };
        Color32::from_rgb(r, g, b)
    };
    match class {
        ClassName::Part | ClassName::MeshPart | ClassName::Model => {
            pick((110, 160, 245), (0, 74, 173))
        }
        ClassName::Folder => pick((249, 191, 59), (190, 130, 0)),
        ClassName::SurfaceAppearance => pick((236, 120, 220), (160, 40, 150)),
        ClassName::ScreenGui
        | ClassName::Frame
        | ClassName::TextLabel
        | ClassName::TextButton
        | ClassName::ImageLabel
        | ClassName::TextBox
        | ClassName::PageLayout
        | ClassName::Table
        | ClassName::BillboardGui => {
            pick((120, 190, 255), (0, 105, 180))
        }
        ClassName::Sound => pick((245, 166, 35), (175, 105, 0)),
        ClassName::PointLight | ClassName::SpotLight => pick((255, 220, 90), (170, 130, 0)),
        ClassName::Script => pick((241, 137, 45), (200, 95, 0)),
        ClassName::LocalScript => pick((64, 200, 180), (0, 128, 118)),
        ClassName::ModuleScript => pick((180, 145, 255), (110, 70, 200)),
        ClassName::RemoteEvent => pick((255, 125, 170), (190, 40, 100)),
        ClassName::Player => pick((120, 205, 120), (40, 130, 50)),
        _ => visuals.weak_text_color(),
    }
}

/// A panel's title, such as "Explorer", with its icon.
pub fn panel_title(ui: &mut egui::Ui, icon: &str, title: &str) {
    let color = ui.visuals().strong_text_color();
    ui.label(
        egui::RichText::new(format!("{icon}  {title}"))
            .family(heading_family())
            .size(13.5)
            .color(color),
    );
}

/// The text of a menu item with an icon in front, such as "💾 Save".
/// How big to draw a button's icon (1 is its normal size), animated: it
/// grows a little under the mouse, dips while pressed, and pops back with a
/// small bounce after a click.
pub fn icon_scale(ui: &egui::Ui, response: &egui::Response) -> f32 {
    const POP_SECONDS: f64 = 0.35;
    let ctx = ui.ctx();
    let hover = ctx.animate_bool_with_time(
        response.id.with("icon hover"),
        response.hovered() && ui.is_enabled(),
        0.12,
    );
    let pressed = ctx.animate_bool_with_time(
        response.id.with("icon press"),
        response.is_pointer_button_down_on(),
        0.06,
    );
    let clicked_at = response.id.with("icon clicked");
    let now = ctx.input(|input| input.time);
    if response.clicked() {
        ctx.data_mut(|data| data.insert_temp(clicked_at, now));
    }
    let since = ctx
        .data(|data| data.get_temp::<f64>(clicked_at))
        .map(|at| now - at);
    let pop = match since {
        Some(since) if since < POP_SECONDS => {
            ctx.request_repaint();
            let t = (since / POP_SECONDS) as f32;
            (t * std::f32::consts::PI).sin() * 0.25 * (1.0 - t)
        }
        _ => 0.0,
    };
    1.0 + 0.12 * hover - 0.12 * pressed + pop
}

pub fn with_icon(icon: &str, text: &str) -> String {
    format!("{icon}  {text}")
}

/// An egui context with the editor's fonts and styles, for tests that draw.
#[cfg(test)]
pub fn test_context() -> egui::Context {
    let ctx = egui::Context::default();
    install(&ctx);
    ctx
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_class_has_an_icon_and_every_text_style_a_font() {
        let ctx = test_context();
        let _ = ctx.run(Default::default(), |ctx| {
            ctx.fonts(|fonts| {
                for class in ClassName::ALL {
                    let icon = class_icon(class).chars().next().unwrap();
                    assert!(
                        fonts.has_glyph(&FontId::proportional(13.0), icon),
                        "{class:?}"
                    );
                }
                // Each family can draw the stand-in for missing characters.
                for family in [
                    FontFamily::Proportional,
                    FontFamily::Monospace,
                    heading_family(),
                    filled(),
                ] {
                    assert!(fonts.has_glyph(&FontId::new(13.0, family.clone()), '?'), "{family:?}");
                }
            });
            for theme in [egui::Theme::Dark, egui::Theme::Light] {
                ctx.set_theme(theme);
                let style = ctx.style();
                assert_eq!(
                    style.text_styles[&TextStyle::Heading].family,
                    heading_family()
                );
                assert_eq!(
                    style.text_styles[&TextStyle::Monospace].family,
                    FontFamily::Monospace
                );
            }
        });
    }
}
