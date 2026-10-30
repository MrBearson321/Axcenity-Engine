//! The editor's design system: semantic colour tokens ([`Palette`]) for the
//! dark and light themes, a choice of accent colours, the type scale (Inter
//! for the UI, JetBrains Mono for code, Phosphor for icons), spacing for two
//! densities, motion, and an icon and colour for each class of Instance.
//!
//! Everything the editor draws takes its colours from [`palette`] or from
//! egui's `Visuals`, which [`apply`] fills from the same tokens. Only scene
//! content (gizmo axes, block categories, syntax colours) has its own.
//!
//! Icons are letters in the Phosphor icon font, so they go inside any text,
//! such as `format!("{} Save", icons::FLOPPY_DISK)`, and take its size and colour.

use std::sync::Arc;

use axcenity_core::{ClassName, PartType};
use eframe::egui::{
    self, Color32, CornerRadius, FontData, FontFamily, FontId, Shadow, Stroke, TextStyle,
};
use serde::{Deserialize, Serialize};

use crate::gizmo::Tool;
use crate::tree::Insertable;

/// The solid icons. They have the same letters as [`icons`], so they only
/// show as solid in the [`filled`] font family.
pub use egui_phosphor::fill as filled_icons;
/// The outline icons, used everywhere unless a solid one reads better.
pub use egui_phosphor::regular as icons;

/// The type scale, in points. Hierarchy comes from weight and colour more
/// than size, so most of the UI is [`BODY`](type_scale::BODY).
pub mod type_scale {
    /// Page titles on the start screen.
    pub const DISPLAY: f32 = 22.0;
    /// Dialog titles and section headings.
    pub const HEADING: f32 = 15.0;
    /// Labels, buttons and panel text.
    pub const BODY: f32 = 13.0;
    /// Descriptions and secondary text.
    pub const SMALL: f32 = 11.5;
    /// Captions, badges and status text.
    pub const CAPTION: f32 = 10.5;
}

/// The light or dark theme, or whichever the system uses.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ThemeChoice {
    #[default]
    Dark,
    Light,
    System,
}

impl ThemeChoice {
    pub const ALL: [ThemeChoice; 3] = [ThemeChoice::Dark, ThemeChoice::Light, ThemeChoice::System];

    pub fn label(self) -> &'static str {
        match self {
            ThemeChoice::Dark => "Dark",
            ThemeChoice::Light => "Light",
            ThemeChoice::System => "System",
        }
    }

    pub fn icon(self) -> &'static str {
        match self {
            ThemeChoice::Dark => icons::MOON,
            ThemeChoice::Light => icons::SUN,
            ThemeChoice::System => icons::DESKTOP,
        }
    }

    pub fn preference(self) -> egui::ThemePreference {
        match self {
            ThemeChoice::Dark => egui::ThemePreference::Dark,
            ThemeChoice::Light => egui::ThemePreference::Light,
            ThemeChoice::System => egui::ThemePreference::System,
        }
    }
}

/// The one colour used for emphasis: selection, focus, the active tab and
/// main buttons. The rest of the UI stays neutral whichever is picked.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Accent {
    #[default]
    Blue,
    Violet,
    Pink,
    Red,
    Orange,
    Yellow,
    Green,
    Teal,
}

impl Accent {
    /// In the order Settings shows them, around the colour wheel.
    pub const ALL: [Accent; 8] = [
        Accent::Blue,
        Accent::Violet,
        Accent::Pink,
        Accent::Red,
        Accent::Orange,
        Accent::Yellow,
        Accent::Green,
        Accent::Teal,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Accent::Blue => "Blue",
            Accent::Violet => "Violet",
            Accent::Pink => "Pink",
            Accent::Red => "Red",
            Accent::Orange => "Orange",
            Accent::Yellow => "Yellow",
            Accent::Green => "Green",
            Accent::Teal => "Teal",
        }
    }

    /// The accent as text and icons on panels, and the fill of main
    /// buttons (white text on it), for a dark or light theme. Every pair is
    /// at least 4.5:1 against what it sits on (WCAG AA).
    fn colors(self, dark: bool) -> (Color32, Color32) {
        let hex = |rgb: u32| Color32::from_rgb((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8);
        let (on_dark, solid_dark, on_light) = match self {
            Accent::Blue => (0x6C8CFF, 0x4562E6, 0x3451D1),
            Accent::Violet => (0xA48AFF, 0x7650E8, 0x6741D9),
            Accent::Pink => (0xF07AB8, 0xBE2F6E, 0xB52A68),
            Accent::Red => (0xF47A7A, 0xC63434, 0xC02E2E),
            Accent::Orange => (0xF2A15A, 0xBF5310, 0xA8460B),
            Accent::Yellow => (0xE8C547, 0x8A6A00, 0x806200),
            Accent::Green => (0x4CC38A, 0x1D7F51, 0x1C7547),
            Accent::Teal => (0x3EC6BE, 0x0F766E, 0x0F766E),
        };
        if dark {
            (hex(on_dark), hex(solid_dark))
        } else {
            (hex(on_light), hex(on_light))
        }
    }
}

/// How much room controls get.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Density {
    #[default]
    Comfortable,
    Compact,
}

impl Density {
    pub const ALL: [Density; 2] = [Density::Comfortable, Density::Compact];

    pub fn label(self) -> &'static str {
        match self {
            Density::Comfortable => "Comfortable",
            Density::Compact => "Compact",
        }
    }
}

/// Everything about the editor's look that people can change in Settings.
/// Saved between runs as JSON; fields missing from an older save keep
/// their defaults.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Appearance {
    pub theme: ThemeChoice,
    pub accent: Accent,
    pub density: Density,
    /// Turns transitions off: hover fades, panel and popup fades, smooth scrolling.
    pub reduce_motion: bool,
    /// The size of code and Output text, in points.
    pub code_font_size: f32,
    /// How big the whole interface is drawn (egui's zoom factor).
    pub ui_scale: f32,
}

impl Default for Appearance {
    fn default() -> Self {
        Self {
            theme: ThemeChoice::Dark,
            accent: Accent::Blue,
            density: Density::Comfortable,
            reduce_motion: false,
            code_font_size: 13.0,
            ui_scale: 1.0,
        }
    }
}

impl Appearance {
    pub const CODE_FONT_SIZES: std::ops::RangeInclusive<f32> = 10.0..=20.0;
    pub const UI_SCALES: [f32; 5] = [0.9, 1.0, 1.1, 1.25, 1.5];

    pub fn to_text(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }

    /// Settings saved by [`to_text`](Self::to_text). Damaged text gives the defaults.
    pub fn from_text(text: &str) -> Self {
        let mut appearance: Self = serde_json::from_str(text).unwrap_or_default();
        appearance.code_font_size = appearance
            .code_font_size
            .clamp(*Self::CODE_FONT_SIZES.start(), *Self::CODE_FONT_SIZES.end());
        if !Self::UI_SCALES.contains(&appearance.ui_scale) {
            appearance.ui_scale = 1.0;
        }
        appearance
    }
}

/// The semantic colours of one theme. Draw with these (through [`palette`])
/// rather than with colours of your own, so both themes and every accent work.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Palette {
    pub dark: bool,
    /// Behind everything: the header, toolbars and tab bars.
    pub app_bg: Color32,
    /// Panel bodies: the Explorer, Properties, Output and pages.
    pub panel: Color32,
    /// Cards and other secondary surfaces on a panel.
    pub surface: Color32,
    /// Menus, popovers and dialogs.
    pub elevated: Color32,
    /// Text fields and other wells.
    pub inset: Color32,
    /// Buttons and other controls, idle, hovered and pressed.
    pub control: Color32,
    pub control_hover: Color32,
    pub control_active: Color32,
    /// Separators and outlines, and stronger ones for hovered things.
    pub border: Color32,
    pub border_strong: Color32,
    /// Text: body, the strongest (titles, hovered), secondary (labels,
    /// descriptions), weak (metadata, still AA) and muted (disabled and
    /// decoration only: below AA).
    pub text: Color32,
    pub text_strong: Color32,
    pub text_secondary: Color32,
    pub text_weak: Color32,
    pub text_muted: Color32,
    /// The accent for text, icons and indicators on panels.
    pub accent: Color32,
    /// The fill of main buttons, with [`on_accent`](Self::on_accent) text.
    pub accent_solid: Color32,
    pub on_accent: Color32,
    /// The background of selected rows and items, and the text on it.
    pub accent_soft: Color32,
    pub accent_text: Color32,
    pub success: Color32,
    pub warning: Color32,
    pub error: Color32,
}

impl Palette {
    pub fn new(dark: bool, accent: Accent) -> Self {
        let hex = |rgb: u32| Color32::from_rgb((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8);
        let (accent_fg, accent_solid) = accent.colors(dark);
        if dark {
            let panel = hex(0x1E1E1E);
            Self {
                dark,
                app_bg: hex(0x171717),
                panel,
                surface: hex(0x242424),
                elevated: hex(0x292929),
                inset: hex(0x191919),
                control: hex(0x2A2A2A),
                control_hover: hex(0x323232),
                control_active: hex(0x3A3A3A),
                border: hex(0x303030),
                border_strong: hex(0x424242),
                text: hex(0xE8E8EA),
                text_strong: hex(0xFAFAFA),
                text_secondary: hex(0xA1A1AA),
                text_weak: hex(0x8E8E96),
                text_muted: hex(0x63636B),
                accent: accent_fg,
                accent_solid,
                on_accent: Color32::WHITE,
                accent_soft: mix(panel, accent_fg, 0.24),
                accent_text: mix(accent_fg, Color32::WHITE, 0.45),
                success: hex(0x4CC38A),
                warning: hex(0xE5B454),
                error: hex(0xF07178),
            }
        } else {
            let panel = Color32::WHITE;
            Self {
                dark,
                app_bg: hex(0xF4F4F5),
                panel,
                surface: hex(0xF7F7F8),
                elevated: Color32::WHITE,
                inset: Color32::WHITE,
                control: hex(0xF1F1F3),
                control_hover: hex(0xE9E9EC),
                control_active: hex(0xE0E0E4),
                border: hex(0xE4E4E7),
                border_strong: hex(0xD1D1D6),
                text: hex(0x27272A),
                text_strong: hex(0x18181B),
                text_secondary: hex(0x52525B),
                text_weak: hex(0x6B6B73),
                text_muted: hex(0xA1A1AA),
                accent: accent_fg,
                accent_solid,
                on_accent: Color32::WHITE,
                accent_soft: mix(panel, accent_fg, 0.12),
                accent_text: mix(accent_fg, Color32::BLACK, 0.15),
                success: hex(0x1C7547),
                warning: hex(0x8F5F00),
                error: hex(0xC42B2B),
            }
        }
    }
}

/// `a` moved toward `b` by `t` (0 to 1), channel by channel.
pub fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let channel = |x: u8, y: u8| (f32::from(x) + (f32::from(y) - f32::from(x)) * t).round() as u8;
    Color32::from_rgb(
        channel(a.r(), b.r()),
        channel(a.g(), b.g()),
        channel(a.b(), b.b()),
    )
}

fn accent_id() -> egui::Id {
    egui::Id::new("axcenity accent")
}

fn density_id() -> egui::Id {
    egui::Id::new("axcenity density")
}

/// The density picked in Settings, for widgets that change shape with it
/// (such as the ribbon, which drops its captions when compact).
pub fn density(ctx: &egui::Context) -> Density {
    ctx.data(|data| data.get_temp::<Density>(density_id()))
        .unwrap_or_default()
}

/// The colours of the theme showing now, with the picked accent.
pub fn palette(ctx: &egui::Context) -> Palette {
    let accent = ctx
        .data(|data| data.get_temp::<Accent>(accent_id()))
        .unwrap_or_default();
    Palette::new(ctx.style().visuals.dark_mode, accent)
}

/// The font family of headings, panel titles and emphasis (Inter SemiBold).
pub fn heading_family() -> FontFamily {
    FontFamily::Name("heading".into())
}

/// The font family of tabs, buttons that need weight, and labels that lead
/// a group (Inter Medium).
pub fn medium_family() -> FontFamily {
    FontFamily::Name("medium".into())
}

/// The font family that draws [`filled_icons`] solid.
pub fn filled() -> FontFamily {
    FontFamily::Name("filled icons".into())
}

/// Sets the fonts and the default look. Called once, when the window opens;
/// [`apply`] then changes the look as Settings change.
pub fn install(ctx: &egui::Context) {
    ctx.set_fonts(fonts());
    apply(ctx, &Appearance::default());
}

/// Sets both themes' colours and sizes from `appearance`, and shows the
/// theme it asks for. Cheap enough to call whenever a setting changes.
pub fn apply(ctx: &egui::Context, appearance: &Appearance) {
    ctx.data_mut(|data| {
        data.insert_temp(accent_id(), appearance.accent);
        data.insert_temp(density_id(), appearance.density);
    });
    for theme in [egui::Theme::Dark, egui::Theme::Light] {
        let palette = Palette::new(theme == egui::Theme::Dark, appearance.accent);
        ctx.set_visuals_of(theme, visuals(&palette));
        ctx.style_mut_of(theme, |style| set_sizes(style, appearance));
    }
    ctx.set_theme(appearance.theme.preference());
    if (ctx.zoom_factor() - appearance.ui_scale).abs() > f32::EPSILON {
        ctx.set_zoom_factor(appearance.ui_scale);
    }
}

/// Inter and JetBrains Mono first, with Phosphor after each for icons,
/// then egui's own fonts for anything they don't have (such as ⌫).
fn fonts() -> egui::FontDefinitions {
    // `static` font files are built into the program, like the window icon.
    let font = |bytes: &'static [u8]| Arc::new(FontData::from_static(bytes));
    let mut fonts = egui::FontDefinitions::default();
    for (name, data) in [
        (
            "Inter",
            font(include_bytes!("../../../assets/fonts/Inter-Regular.ttf")),
        ),
        (
            "Inter Medium",
            font(include_bytes!("../../../assets/fonts/Inter-Medium.ttf")),
        ),
        (
            "Inter SemiBold",
            font(include_bytes!("../../../assets/fonts/Inter-SemiBold.ttf")),
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
        .insert(FontFamily::Proportional, family("Inter"));
    fonts
        .families
        .insert(FontFamily::Monospace, family("JetBrains Mono"));
    fonts
        .families
        .insert(heading_family(), family("Inter SemiBold"));
    fonts
        .families
        .insert(medium_family(), family("Inter Medium"));
    // Every family needs a font with `◻` or `?` in it, which egui draws
    // for characters no font has; without one it warns at startup.
    fonts.families.insert(filled(), family("Phosphor Fill"));
    fonts
}

/// Text sizes, spacing for the density, and motion.
fn set_sizes(style: &mut egui::Style, appearance: &Appearance) {
    style.text_styles = [
        (TextStyle::Small, FontId::proportional(type_scale::SMALL)),
        (TextStyle::Body, FontId::proportional(type_scale::BODY)),
        (TextStyle::Button, FontId::proportional(type_scale::BODY)),
        (
            TextStyle::Heading,
            FontId::new(type_scale::HEADING, heading_family()),
        ),
        (
            TextStyle::Monospace,
            FontId::monospace(appearance.code_font_size),
        ),
    ]
    .into();
    let spacing = &mut style.spacing;
    match appearance.density {
        Density::Comfortable => {
            spacing.item_spacing = egui::vec2(8.0, 6.0);
            spacing.button_padding = egui::vec2(10.0, 4.0);
            spacing.interact_size.y = 24.0;
            spacing.menu_margin = egui::Margin::same(6);
            spacing.window_margin = egui::Margin::same(16);
            spacing.indent = 16.0;
        }
        Density::Compact => {
            spacing.item_spacing = egui::vec2(6.0, 3.0);
            spacing.button_padding = egui::vec2(7.0, 2.0);
            spacing.interact_size.y = 20.0;
            spacing.menu_margin = egui::Margin::same(4);
            spacing.window_margin = egui::Margin::same(12);
            spacing.indent = 13.0;
        }
    }
    spacing.menu_spacing = 2.0;
    spacing.combo_width = 120.0;
    // Thin scroll bars over the content, which widen under the mouse.
    spacing.scroll = egui::style::ScrollStyle::floating();
    style.interaction.tooltip_delay = 0.35;
    // Hovers, fades and opening sections take this long; none of it ever
    // waits for input.
    if appearance.reduce_motion {
        style.animation_time = 0.0;
        style.scroll_animation = egui::style::ScrollAnimation::none();
    } else {
        style.animation_time = MOTION_SECONDS;
        style.scroll_animation = egui::style::ScrollAnimation::default();
    }
}

/// How long micro-interactions take (hover, press, fades), in seconds.
pub const MOTION_SECONDS: f32 = 0.16;

/// egui's visuals for one theme, from its palette.
fn visuals(p: &Palette) -> egui::Visuals {
    let mut v = if p.dark {
        egui::Visuals::dark()
    } else {
        egui::Visuals::light()
    };
    v.panel_fill = p.panel;
    v.window_fill = p.elevated;
    v.window_stroke = Stroke::new(1.0_f32, p.border);
    v.extreme_bg_color = p.inset;
    v.text_edit_bg_color = Some(p.inset);
    v.faint_bg_color = p.surface;
    v.code_bg_color = p.inset;
    v.hyperlink_color = p.accent;
    v.warn_fg_color = p.warning;
    v.error_fg_color = p.error;
    v.weak_text_color = Some(p.text_weak);
    // Selected rows and items. The stroke is also the focus ring of text
    // fields and the text on a selected item.
    v.selection.bg_fill = p.accent_soft;
    v.selection.stroke = Stroke::new(1.0_f32, p.accent_text);
    v.text_cursor.stroke = Stroke::new(2.0_f32, p.accent);
    let shadow = |y: i8, blur: u8, alpha: u8| Shadow {
        offset: [0, y],
        blur,
        spread: 0,
        color: Color32::from_black_alpha(alpha),
    };
    v.window_shadow = if p.dark {
        shadow(12, 36, 120)
    } else {
        shadow(12, 36, 30)
    };
    v.popup_shadow = if p.dark {
        shadow(6, 18, 90)
    } else {
        shadow(6, 18, 24)
    };
    v.window_corner_radius = CornerRadius::same(10);
    v.menu_corner_radius = CornerRadius::same(8);
    v.collapsing_header_frame = false;
    v.indent_has_left_vline = false;
    v.striped = false;
    v.slider_trailing_fill = true;
    v.handle_shape = egui::style::HandleShape::Circle;

    // Small: fields and checkboxes stay square-cornered enough to read as
    // boxes. Buttons the editor draws itself round a little more.
    let radius = CornerRadius::same(4);
    let w = &mut v.widgets;
    w.noninteractive.bg_fill = p.panel;
    w.noninteractive.weak_bg_fill = p.panel;
    w.noninteractive.bg_stroke = Stroke::new(1.0_f32, p.border);
    w.noninteractive.fg_stroke = Stroke::new(1.0_f32, p.text);
    for (state, fill, outline, text) in [
        (&mut w.inactive, p.control, p.border, p.text),
        (
            &mut w.hovered,
            p.control_hover,
            p.border_strong,
            p.text_strong,
        ),
        // Pressed, and focused from the keyboard: the accent ring shows focus.
        (&mut w.active, p.control_active, p.accent, p.text_strong),
        (&mut w.open, p.control_hover, p.border, p.text),
    ] {
        state.bg_fill = fill;
        state.weak_bg_fill = fill;
        state.bg_stroke = Stroke::new(1.0_f32, outline);
        state.fg_stroke = Stroke::new(1.0_f32, text);
        state.corner_radius = radius;
        // No growing on hover: it shifts edges by a pixel and looks jittery.
        state.expansion = 0.0;
    }
    w.noninteractive.corner_radius = radius;
    v
}

/// How far `id` is through its hover (or any on/off) transition, from 0 to
/// 1, eased. Instant when motion is reduced.
pub fn ease(ctx: &egui::Context, id: egui::Id, on: bool) -> f32 {
    let time = ctx.style().animation_time;
    let t = ctx.animate_bool_with_time(id, on, time);
    // Ease out: quick to respond, gentle to settle.
    1.0 - (1.0 - t) * (1.0 - t)
}

/// Fades the whole window in from the old background after the theme
/// changes, so switching doesn't flash. Call [`start_crossfade`] when it
/// changes and [`paint_crossfade`] every frame.
pub fn start_crossfade(ctx: &egui::Context, from: Color32) {
    if ctx.style().animation_time <= 0.0 {
        return;
    }
    let now = ctx.input(|input| input.time);
    ctx.data_mut(|data| data.insert_temp(crossfade_id(), (from, now)));
}

pub fn paint_crossfade(ctx: &egui::Context) {
    const SECONDS: f64 = 0.24;
    let Some((from, start)) = ctx.data(|data| data.get_temp::<(Color32, f64)>(crossfade_id()))
    else {
        return;
    };
    let elapsed = ctx.input(|input| input.time) - start;
    if elapsed >= SECONDS {
        ctx.data_mut(|data| data.remove::<(Color32, f64)>(crossfade_id()));
        return;
    }
    let t = (elapsed / SECONDS) as f32;
    let alpha = (1.0 - t) * (1.0 - t) * 0.9;
    // Painted only: clicks and keys go through to the UI underneath.
    ctx.layer_painter(egui::LayerId::new(egui::Order::Tooltip, crossfade_id()))
        .rect_filled(ctx.screen_rect(), 0, from.gamma_multiply(alpha));
    ctx.request_repaint();
}

fn crossfade_id() -> egui::Id {
    egui::Id::new("theme crossfade")
}

/// The four kinds of animation thing. Each has one icon and one colour,
/// used in the Animation tab's outline, the Explorer, the Assets panel and
/// the pickers, so a kind looks the same everywhere.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AnimKind {
    /// The Instance that plays animation on its parent.
    Animator,
    /// A `*.statemachine.json` file.
    StateMachine,
    /// The Instance that points at a clip.
    Animation,
    /// A `*.anim.json` file.
    Clip,
}

impl AnimKind {
    pub fn icon(self) -> &'static str {
        match self {
            AnimKind::Animator => icons::PERSON_SIMPLE_RUN,
            AnimKind::StateMachine => icons::FLOW_ARROW,
            AnimKind::Animation => icons::FILM_SLATE,
            AnimKind::Clip => icons::FILM_STRIP,
        }
    }

    pub fn color(self, visuals: &egui::Visuals) -> Color32 {
        let dark = visuals.dark_mode;
        let pick = |on_dark: (u8, u8, u8), on_light: (u8, u8, u8)| {
            let (r, g, b) = if dark { on_dark } else { on_light };
            mix(Color32::from_rgb(r, g, b), visuals.text_color(), 0.2)
        };
        match self {
            AnimKind::Animator => pick((122, 205, 150), (30, 130, 70)),
            AnimKind::StateMachine => pick((238, 170, 90), (175, 100, 0)),
            AnimKind::Animation => pick((110, 190, 175), (25, 120, 105)),
            AnimKind::Clip => pick((205, 140, 235), (130, 55, 170)),
        }
    }

    /// What it's called in the editor.
    pub fn label(self) -> &'static str {
        match self {
            AnimKind::Animator => "Animator",
            AnimKind::StateMachine => "State Machine",
            AnimKind::Animation => "Animation",
            AnimKind::Clip => "Clip",
        }
    }

    /// Its icon in its colour, to start a label with.
    pub fn rich_icon(self, visuals: &egui::Visuals) -> egui::RichText {
        egui::RichText::new(self.icon()).color(self.color(visuals))
    }

    /// The kind of a file by its path, if it's a clip or a state machine.
    pub fn of_path(path: &str) -> Option<AnimKind> {
        if path.ends_with(axcenity_anim::CLIP_EXTENSION) {
            Some(AnimKind::Clip)
        } else if path.ends_with(axcenity_anim::STATE_MACHINE_EXTENSION) {
            Some(AnimKind::StateMachine)
        } else {
            None
        }
    }
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
        ClassName::ServerStorage => icons::ARCHIVE,
        ClassName::StarterPlayer => icons::USER_CIRCLE,
        ClassName::StarterPlayerScripts => icons::USER_GEAR,
        ClassName::Teams => icons::USERS_THREE,
        ClassName::SoundService => icons::SPEAKER_HIFI,
        ClassName::TextChatService => icons::CHAT_CIRCLE_TEXT,
        ClassName::SoundGroup => icons::FADERS,
        ClassName::ReverbSoundEffect => icons::WAVEFORM,
        ClassName::EqualizerSoundEffect => icons::EQUALIZER,
        ClassName::CompressorSoundEffect => icons::ARROWS_IN_LINE_VERTICAL,
        ClassName::EchoSoundEffect => icons::REPEAT,
        ClassName::DistortionSoundEffect => icons::WAVE_SQUARE,
        ClassName::PitchShiftSoundEffect => icons::WAVE_SINE,
        ClassName::LowPassSoundEffect | ClassName::HighPassSoundEffect => icons::FUNNEL,
        ClassName::ReverbZone => icons::EAR,
        ClassName::GraphicsVolume => icons::BOUNDING_BOX,
        ClassName::ExposureEffect => icons::APERTURE,
        ClassName::FogEffect => icons::CLOUD_FOG,
        ClassName::BloomEffect => icons::STAR_FOUR,
        ClassName::Team => icons::FLAG_BANNER,
        ClassName::SpawnLocation => icons::MAP_PIN,
        ClassName::ForceField => icons::SHIELD,
        ClassName::Part => icons::CUBE,
        ClassName::MeshPart => icons::CUBE_TRANSPARENT,
        ClassName::Seat => icons::CHAIR,
        ClassName::VehicleSeat => icons::STEERING_WHEEL,
        ClassName::VehicleWheel => icons::TIRE,
        ClassName::Ragdoll => icons::PERSON_SIMPLE,
        ClassName::Fracture => icons::HAMMER,
        ClassName::UnionOperation => icons::UNITE,
        ClassName::NegateOperation => icons::SUBTRACT,
        ClassName::Health => icons::HEART,
        ClassName::ProximityPrompt => icons::HAND_TAP,
        ClassName::ClickDetector => icons::CURSOR_CLICK,
        ClassName::Tool => icons::SWORD,
        ClassName::Inventory => icons::BACKPACK,
        ClassName::Humanoid => icons::PERSON,
        ClassName::DamageZone => icons::FIRE,
        ClassName::Script => icons::FILE_CODE,
        ClassName::LocalScript => icons::CODE,
        ClassName::ModuleScript => icons::PUZZLE_PIECE,
        ClassName::RemoteEvent => icons::BROADCAST,
        ClassName::RemoteFunction => icons::PHONE_CALL,
        ClassName::BindableEvent => icons::LIGHTNING,
        ClassName::BindableFunction => icons::FUNCTION,
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
        ClassName::ScrollingFrame => icons::SCROLL,
        ClassName::Slider => icons::SLIDERS_HORIZONTAL,
        ClassName::Toggle => icons::TOGGLE_RIGHT,
        ClassName::Dropdown => icons::CARET_CIRCLE_DOWN,
        ClassName::ProgressBar => icons::CHART_BAR_HORIZONTAL,
        ClassName::BillboardGui => icons::FLAG,
        ClassName::Sound => icons::MUSIC_NOTE,
        ClassName::PointLight => icons::LIGHTBULB,
        ClassName::SpotLight => icons::FLASHLIGHT,
        ClassName::ReflectionProbe => icons::SPHERE,
        ClassName::LightProbeVolume => icons::DOTS_NINE,
        ClassName::Atmosphere => icons::CLOUD_SUN,
        ClassName::Clouds => icons::CLOUD,
        ClassName::Weather => icons::CLOUD_RAIN,
        ClassName::OutlineEffect => icons::PEN_NIB,
        ClassName::ColorCorrectionEffect => icons::SWATCHES,
        ClassName::DepthOfFieldEffect => icons::BINOCULARS,
        ClassName::VignetteEffect => icons::VIGNETTE,
        ClassName::FilmGrainEffect => icons::FILM_STRIP,
        ClassName::ChromaticAberrationEffect => icons::RAINBOW,
        ClassName::SharpenEffect => icons::DIAMOND,
        ClassName::MotionBlurEffect => icons::WIND,
        ClassName::ParticleEmitter => icons::SPARKLE,
        ClassName::Animator => AnimKind::Animator.icon(),
        ClassName::Animation => AnimKind::Animation.icon(),
        ClassName::AnimationTrack => icons::PLAY_CIRCLE,
        ClassName::Sequence => icons::FILM_SLATE,
        ClassName::Terrain => icons::MOUNTAINS,
        ClassName::TerrainLayer => icons::STACK,
        ClassName::ThirdPersonController => icons::PERSON_SIMPLE_WALK,
        ClassName::FirstPersonController => icons::EYE,
        ClassName::NavigationAgent => icons::ROBOT,
        ClassName::Checkpoint => icons::FLAG_PENNANT,
        ClassName::Camera => icons::VIDEO_CAMERA,
        ClassName::Attachment => icons::CROSSHAIR_SIMPLE,
        ClassName::WeldConstraint => icons::LINK_SIMPLE,
        ClassName::HingeConstraint => icons::DOOR_OPEN,
        ClassName::BallSocketConstraint => icons::CIRCLE_NOTCH,
        ClassName::PrismaticConstraint => icons::ARROWS_HORIZONTAL,
        ClassName::CylindricalConstraint => icons::CYLINDER,
        ClassName::SpringConstraint => icons::SCRIBBLE_LOOP,
        ClassName::RopeConstraint => icons::SCRIBBLE,
        ClassName::RodConstraint => icons::LINE_SEGMENT,
        ClassName::LinearVelocity => icons::NAVIGATION_ARROW,
        ClassName::AngularVelocity => icons::ARROWS_CLOCKWISE,
        ClassName::VectorForce => icons::ARROW_FAT_UP,
        ClassName::Torque => icons::ARROW_ARC_RIGHT,
        ClassName::AlignPosition => icons::CROSSHAIR,
        ClassName::AlignOrientation => icons::COMPASS,
        ClassName::IntValue | ClassName::NumberValue => icons::HASH,
        ClassName::StringValue => icons::TEXT_AA,
        ClassName::BoolValue => icons::TOGGLE_LEFT,
        ClassName::Vector3Value => icons::ARROWS_OUT_CARDINAL,
        ClassName::Color3Value => icons::PALETTE,
        ClassName::CFrameValue => icons::COMPASS,
        ClassName::ObjectValue => icons::LINK,
    }
}

/// The icon of a tool in the toolbar.
pub fn tool_icon(tool: Tool) -> &'static str {
    match tool {
        Tool::Select => icons::CURSOR,
        Tool::Move => icons::ARROWS_OUT_CARDINAL,
        Tool::Scale => icons::ARROWS_OUT,
        Tool::Rotate => icons::ARROWS_CLOCKWISE,
        Tool::Transform => icons::BOUNDING_BOX,
        Tool::Paint => icons::PAINT_BUCKET,
        Tool::Material => icons::DROP,
        Tool::Anchor => icons::ANCHOR,
        Tool::Lighting => icons::LIGHTBULB,
        Tool::Emission => icons::SUN_DIM,
        Tool::Angular => icons::SPIRAL,
    }
}

/// The icon of something to insert: parts by their shape.
pub fn insert_icon(what: Insertable) -> &'static str {
    match what {
        Insertable::Part(shape) => part_icon(shape),
        Insertable::Vehicle => icons::CAR,
        Insertable::Character => icons::PERSON_ARMS_SPREAD,
        Insertable::HealthBar => icons::HEARTBEAT,
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
        PartType::Capsule => icons::PILL,
    }
}

/// The colour of `class`'s icon in the Explorer and tabs, so kinds of
/// things are easy to tell apart at a glance. The hues are softened toward
/// the text colour so the tree stays calm; services are grey.
pub fn class_color(class: ClassName, visuals: &egui::Visuals) -> Color32 {
    let dark = visuals.dark_mode;
    let pick = |on_dark: (u8, u8, u8), on_light: (u8, u8, u8)| {
        let (r, g, b) = if dark { on_dark } else { on_light };
        mix(Color32::from_rgb(r, g, b), visuals.text_color(), 0.2)
    };
    match class {
        ClassName::Part
        | ClassName::MeshPart
        | ClassName::Seat
        | ClassName::VehicleSeat
        | ClassName::DamageZone
        | ClassName::ReverbZone
        | ClassName::GraphicsVolume
        | ClassName::Model => pick((120, 160, 240), (40, 90, 190)),
        ClassName::Folder => pick((222, 182, 92), (160, 115, 10)),
        ClassName::SurfaceAppearance => pick((214, 130, 204), (150, 50, 140)),
        ClassName::ScreenGui
        | ClassName::Frame
        | ClassName::TextLabel
        | ClassName::TextButton
        | ClassName::ImageLabel
        | ClassName::TextBox
        | ClassName::PageLayout
        | ClassName::Table
        | ClassName::ScrollingFrame
        | ClassName::Slider
        | ClassName::Toggle
        | ClassName::Dropdown
        | ClassName::ProgressBar
        | ClassName::BillboardGui => pick((125, 185, 240), (20, 105, 170)),
        class
            if class == ClassName::Sound
                || class == ClassName::SoundGroup
                || class.is_sound_effect() =>
        {
            pick((226, 162, 80), (165, 100, 10))
        }
        ClassName::PointLight
        | ClassName::SpotLight
        | ClassName::ReflectionProbe
        | ClassName::LightProbeVolume
        | ClassName::Atmosphere
        | ClassName::Clouds
        | ClassName::Weather
        | ClassName::OutlineEffect
        | ClassName::ColorCorrectionEffect
        | ClassName::DepthOfFieldEffect
        | ClassName::VignetteEffect
        | ClassName::FilmGrainEffect
        | ClassName::ChromaticAberrationEffect
        | ClassName::SharpenEffect
        | ClassName::MotionBlurEffect
        | ClassName::ExposureEffect
        | ClassName::FogEffect
        | ClassName::BloomEffect => pick((232, 205, 110), (150, 115, 0)),
        ClassName::ParticleEmitter => pick((232, 150, 108), (175, 80, 30)),
        ClassName::Animator => AnimKind::Animator.color(visuals),
        ClassName::Animation | ClassName::AnimationTrack => AnimKind::Animation.color(visuals),
        ClassName::Terrain | ClassName::TerrainLayer => pick((140, 200, 110), (60, 125, 35)),
        ClassName::ThirdPersonController
        | ClassName::FirstPersonController
        | ClassName::NavigationAgent
        | ClassName::Checkpoint
        | ClassName::Camera => pick((90, 200, 160), (20, 130, 100)),
        class
            if class == ClassName::Attachment
                || class == ClassName::WeldConstraint
                || class.is_joint()
                || class.is_mover() =>
        {
            pick((200, 170, 120), (130, 95, 40))
        }
        ClassName::Script => pick((226, 145, 82), (185, 95, 10)),
        ClassName::LocalScript => pick((90, 190, 175), (10, 120, 110)),
        ClassName::ModuleScript => pick((170, 145, 240), (105, 75, 190)),
        ClassName::RemoteEvent
        | ClassName::RemoteFunction
        | ClassName::BindableEvent
        | ClassName::BindableFunction => pick((232, 130, 170), (175, 50, 105)),
        ClassName::Player | ClassName::Team => pick((125, 195, 125), (45, 125, 55)),
        _ => visuals.weak_text_color(),
    }
}

/// The text of a menu item with an icon in front, such as "💾 Save".
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
                    medium_family(),
                    filled(),
                ] {
                    assert!(
                        fonts.has_glyph(&FontId::new(13.0, family.clone()), '?'),
                        "{family:?}"
                    );
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

    #[test]
    fn icons_come_from_phosphor_not_the_text_font() {
        // Inter has its own private-use glyphs where Phosphor's icons are;
        // the bundled copy is subset without them (see assets/fonts). An
        // icon drawn from Inter would be a different width from Phosphor's.
        let ctx = test_context();
        let _ = ctx.run(Default::default(), |ctx| {
            ctx.fonts(|fonts| {
                let mut icons_used: Vec<&str> =
                    ClassName::ALL.into_iter().map(class_icon).collect();
                icons_used.extend([
                    icons::FLAG,
                    icons::CARET_DOWN,
                    icons::CARET_RIGHT,
                    icons::GEAR_SIX,
                    icons::MAGNIFYING_GLASS,
                ]);
                for icon in icons_used {
                    let width = |family: FontFamily| {
                        fonts
                            .layout_no_wrap(
                                icon.to_owned(),
                                FontId::new(20.0, family),
                                Color32::WHITE,
                            )
                            .size()
                            .x
                    };
                    assert_eq!(width(FontFamily::Proportional), width(filled()), "{icon:?}");
                }
            });
        });
    }

    /// WCAG relative luminance and contrast ratio.
    fn contrast(a: Color32, b: Color32) -> f32 {
        let luminance = |c: Color32| {
            let linear = |x: u8| {
                let x = f32::from(x) / 255.0;
                if x <= 0.04045 {
                    x / 12.92
                } else {
                    ((x + 0.055) / 1.055).powf(2.4)
                }
            };
            0.2126 * linear(c.r()) + 0.7152 * linear(c.g()) + 0.0722 * linear(c.b())
        };
        let (la, lb) = (luminance(a), luminance(b));
        (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
    }

    #[test]
    fn text_and_every_accent_meet_wcag_aa_in_both_themes() {
        for dark in [true, false] {
            for accent in Accent::ALL {
                let p = Palette::new(dark, accent);
                let label = format!("{accent:?}, dark {dark}");
                for background in [p.app_bg, p.panel, p.surface, p.elevated] {
                    for text in [p.text, p.text_strong, p.text_secondary] {
                        assert!(contrast(text, background) >= 4.5, "{label}");
                    }
                }
                assert!(contrast(p.text_weak, p.panel) >= 4.5, "{label}");
                assert!(contrast(p.accent, p.panel) >= 4.5, "{label}");
                assert!(contrast(p.on_accent, p.accent_solid) >= 4.5, "{label}");
                assert!(contrast(p.accent_text, p.accent_soft) >= 4.5, "{label}");
                for status in [p.success, p.warning, p.error] {
                    assert!(contrast(status, p.panel) >= 4.5, "{label}");
                }
            }
        }
    }

    #[test]
    fn appearance_survives_saving_and_bad_values_are_fixed() {
        let appearance = Appearance {
            theme: ThemeChoice::Light,
            accent: Accent::Violet,
            density: Density::Compact,
            reduce_motion: true,
            code_font_size: 15.0,
            ui_scale: 1.25,
        };
        assert_eq!(Appearance::from_text(&appearance.to_text()), appearance);
        assert_eq!(Appearance::from_text("not json"), Appearance::default());
        let odd = Appearance::from_text(r#"{"code_font_size": 99, "ui_scale": 7}"#);
        assert_eq!(odd.code_font_size, 20.0);
        assert_eq!(odd.ui_scale, 1.0);
        assert_eq!(odd.accent, Accent::Blue);
    }

    #[test]
    fn applying_settings_changes_the_accent_density_and_motion() {
        let ctx = test_context();
        let appearance = Appearance {
            accent: Accent::Green,
            density: Density::Compact,
            reduce_motion: true,
            ..Appearance::default()
        };
        apply(&ctx, &appearance);
        assert_eq!(
            palette(&ctx).accent,
            Palette::new(true, Accent::Green).accent
        );
        assert_eq!(ctx.style().spacing.interact_size.y, 20.0);
        assert_eq!(ctx.style().animation_time, 0.0);
        // Reduced motion makes transitions instant.
        assert_eq!(ease(&ctx, egui::Id::new("x"), true), 1.0);
    }
}
