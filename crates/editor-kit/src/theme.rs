//! The editor's design system: semantic colour tokens ([`Palette`]) for the
//! dark and light themes, a choice of accent colours, the type scale (Inter
//! for the UI, Plus Jakarta Sans for page and dialog titles, JetBrains Mono
//! for code and micro-labels, Phosphor for icons), spacing for two
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

/// The solid icons. They have the same letters as [`icons`], so they only
/// show as solid in the [`filled`] font family.
pub use egui_phosphor::fill as filled_icons;
/// The outline icons, used everywhere unless a solid one reads better.
pub use egui_phosphor::regular as icons;

/// The type scale, in points. Hierarchy comes from weight and colour more
/// than size, so most of the UI is [`BODY`](type_scale::BODY).
pub mod type_scale {
    /// Page titles on the start screen.
    pub const DISPLAY: f32 = 26.0;
    /// Dialog titles and section headings.
    pub const HEADING: f32 = 15.0;
    /// Labels, buttons and panel text.
    pub const BODY: f32 = 13.0;
    /// Descriptions and secondary text.
    pub const SMALL: f32 = 11.5;
    /// Captions, badges and status text.
    pub const CAPTION: f32 = 10.5;
}

/// Corner radii, in points. There are only three, so shapes that mean the
/// same thing look the same everywhere.
pub mod radius {
    /// Panels, tabs, windows and menus: square and flat.
    pub const NONE: u8 = 0;
    /// Fields, buttons, chips and list highlights.
    pub const CONTROL: u8 = 4;
    /// Cards, toasts, popovers and dialogs.
    pub const CARD: u8 = 6;
    /// Every radius the editor's shared pieces may use.
    #[cfg(any(test, feature = "test-support"))]
    pub const ALL: [u8; 3] = [NONE, CONTROL, CARD];
}

/// Spacing steps, in points, on the 2-point grid most of the editor already
/// follows. Use these for `add_space` and margins instead of new numbers.
pub mod space {
    pub const XXS: f32 = 2.0;
    pub const XS: f32 = 4.0;
    pub const S: f32 = 8.0;
    pub const M: f32 = 12.0;
    pub const L: f32 = 16.0;
    pub const XL: f32 = 24.0;

    /// A margin of `x` points left and right and `y` above and below,
    /// for frames: `space::margin(space::M, space::S)`.
    pub fn margin(x: f32, y: f32) -> eframe::egui::Margin {
        eframe::egui::Margin::symmetric(x as i8, y as i8)
    }
}

/// Sizes of icons and rows, in points.
pub mod size {
    use super::Density;

    /// Icons inside small text, such as badges and captions.
    pub const ICON_SMALL: f32 = 13.0;
    /// Icons beside body text and on icon buttons.
    pub const ICON_MEDIUM: f32 = 15.0;
    /// Icons in headers and empty states.
    pub const ICON_LARGE: f32 = 18.0;
    /// Main buttons ([`crate::widgets::button`]).
    pub const BUTTON_HEIGHT: f32 = 28.0;

    /// The height of a list or form row at a density, the same as egui's
    /// `interact_size.y` after [`super::apply`].
    pub fn row(density: Density) -> f32 {
        match density {
            Density::Comfortable => 24.0,
            Density::Compact => 20.0,
        }
    }
}

/// Colours of guides, selection and handles drawn over 3D or UI content.
/// They don't follow the theme: they sit on scenes, skies and pictures and
/// have to read on any of them. One meaning, one colour, in the viewport,
/// Model Studio, splines, Constraints and the UI Designer.
pub mod overlay {
    use eframe::egui::Color32;

    /// What's selected in a 3D view: outlines, picked points and attachments.
    pub const SELECTED: Color32 = Color32::from_rgb(25, 160, 255);
    /// The gizmo handle or point under the pointer.
    pub const HOVER: Color32 = Color32::from_rgb(255, 215, 0);
    /// Snap and alignment lines, shown while something is dragged into
    /// line, in the viewport and the UI Designer alike.
    pub const SNAP: Color32 = Color32::from_rgb(232, 64, 176);
    /// Guides the user placed from the UI Designer's rulers: a cyan that
    /// reads apart from the magenta snap lines that land on them.
    pub const GUIDE: Color32 = Color32::from_rgb(64, 200, 230);
    /// What a tool will do if you click: a Part being placed, a loop cut
    /// or knife line in Model Studio.
    pub const PREVIEW: Color32 = Color32::from_rgb(255, 196, 64);
    /// Attachments and other points Parts are joined at, while not selected.
    pub const ATTACHMENT: Color32 = Color32::from_rgb(255, 196, 64);
    /// Handles that shape something (spline tangents and similar).
    pub const HANDLE: Color32 = Color32::from_rgb(200, 140, 255);
    /// Something wrong or blocked, such as a ray that hit or a seam.
    pub const WARNING: Color32 = Color32::from_rgb(255, 90, 90);
    /// Measurements and their numbers.
    pub const MEASURE: Color32 = Color32::from_rgb(245, 245, 248);
    /// The plate behind numbers drawn over the scene.
    pub const LABEL_BACK: Color32 = Color32::from_rgb(30, 30, 33);
    /// The X, Y and Z axes: the gizmo, the view cube and joint axes.
    pub const AXIS_X: Color32 = Color32::from_rgb(235, 65, 65);
    pub const AXIS_Y: Color32 = Color32::from_rgb(70, 205, 70);
    pub const AXIS_Z: Color32 = Color32::from_rgb(55, 125, 245);
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

/// The editor's category colours, first made for Block Coding's palette
/// and shared so other tools group things the same way (cohesion pass
/// CW2.2): one hue each, a step apart around the wheel, all deep enough for
/// white labels at 3:1 (4.5:1 with Increase contrast, through
/// [`Palette::under_white_text`]). Use them for a category's swatch,
/// header or accent strip, not for whole panels.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Hue {
    Amber,
    Orange,
    Red,
    Magenta,
    Purple,
    Violet,
    Indigo,
    Blue,
    Cyan,
    Teal,
    Green,
    Slate,
}

impl Hue {
    pub const ALL: [Hue; 12] = [
        Hue::Amber,
        Hue::Orange,
        Hue::Red,
        Hue::Magenta,
        Hue::Purple,
        Hue::Violet,
        Hue::Indigo,
        Hue::Blue,
        Hue::Cyan,
        Hue::Teal,
        Hue::Green,
        Hue::Slate,
    ];

    /// The hue at full strength, before Increase contrast.
    pub fn base(self) -> Color32 {
        match self {
            Hue::Amber => Color32::from_rgb(190, 128, 18),
            Hue::Orange => Color32::from_rgb(212, 98, 40),
            Hue::Red => Color32::from_rgb(206, 62, 62),
            Hue::Magenta => Color32::from_rgb(200, 56, 128),
            Hue::Purple => Color32::from_rgb(166, 70, 196),
            Hue::Violet => Color32::from_rgb(118, 88, 224),
            Hue::Indigo => Color32::from_rgb(72, 96, 214),
            Hue::Blue => Color32::from_rgb(30, 116, 206),
            Hue::Cyan => Color32::from_rgb(18, 130, 168),
            Hue::Teal => Color32::from_rgb(22, 138, 112),
            Hue::Green => Color32::from_rgb(64, 138, 58),
            Hue::Slate => Color32::from_rgb(92, 100, 120),
        }
    }

    /// A fill that white text reads on, darkened with Increase contrast.
    pub fn fill(self, palette: &Palette) -> Color32 {
        palette.under_white_text(self.base())
    }

    /// The darker shade under a filled shape (its edge and shadow).
    pub fn edge(self, palette: &Palette) -> Color32 {
        let darker = |value: u8| (f32::from(value) * 0.78).round() as u8;
        let base = self.base();
        palette.under_white_text(Color32::from_rgb(
            darker(base.r()),
            darker(base.g()),
            darker(base.b()),
        ))
    }

    /// The hue as text or a thin mark on the panel: mixed toward the
    /// theme's text so it reads on light and dark panels alike.
    pub fn ink(self, palette: &Palette) -> Color32 {
        let toward = if palette.dark {
            Color32::WHITE
        } else {
            Color32::BLACK
        };
        mix(self.base(), toward, if palette.dark { 0.28 } else { 0.12 })
    }
}

/// Where a menu item's icon sits in the column before the labels
/// (Settings > Appearance > Menu icons). Either way every label in a menu
/// starts at the same place, with an icon or without one.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum MenuIcons {
    /// Against the left edge, as in Visual Studio and Unity.
    #[default]
    Left,
    /// In the middle of the icon column, lined up like a toolbar's icons.
    Centered,
}

impl MenuIcons {
    pub const ALL: [MenuIcons; 2] = [MenuIcons::Left, MenuIcons::Centered];

    pub fn label(self) -> &'static str {
        match self {
            MenuIcons::Left => "Left",
            MenuIcons::Centered => "Centered",
        }
    }
}

/// Where the ribbon's buttons sit across the window (Settings > Appearance >
/// Ribbon): from the left edge, or in the middle under the centred tabs.
/// When they don't fit, they start at the left and scroll either way.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum RibbonAlign {
    /// From the left edge, as in Visual Studio.
    Left,
    /// In the middle of the window, under the tabs.
    #[default]
    Centered,
}

impl RibbonAlign {
    pub const ALL: [RibbonAlign; 2] = [RibbonAlign::Left, RibbonAlign::Centered];

    pub fn label(self) -> &'static str {
        match self {
            RibbonAlign::Left => "Left",
            RibbonAlign::Centered => "Centered",
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
    /// Stronger outlines, borders, muted text and focus ring, and block
    /// colours dark enough for their white labels (design system V18).
    pub increase_contrast: bool,
    /// The size of code and Output text, in points.
    pub code_font_size: f32,
    /// How big the whole interface is drawn (egui's zoom factor).
    pub ui_scale: f32,
    /// Draws continuously while the window has focus, so a variable refresh
    /// rate display never drops out of its rate when the editor goes idle.
    pub hold_refresh: bool,
    /// While frames are being kept steady, present them on a fixed grid of
    /// this many a second, so a variable refresh rate display sees one
    /// rate, not every frame's own length. 0 is the display's own rate.
    pub steady_hz: u32,
    /// Where icons sit in menus.
    pub menu_icons: MenuIcons,
    /// Where the ribbon's buttons sit across the window.
    pub ribbon: RibbonAlign,
}

impl Default for Appearance {
    fn default() -> Self {
        Self {
            theme: ThemeChoice::Dark,
            accent: Accent::Blue,
            density: Density::Comfortable,
            reduce_motion: false,
            increase_contrast: false,
            code_font_size: 13.0,
            ui_scale: 1.0,
            hold_refresh: false,
            steady_hz: 60,
            menu_icons: MenuIcons::Left,
            ribbon: RibbonAlign::Centered,
        }
    }
}

impl Appearance {
    pub const CODE_FONT_SIZES: std::ops::RangeInclusive<f32> = 10.0..=20.0;
    pub const UI_SCALES: [f32; 5] = [0.9, 1.0, 1.1, 1.25, 1.5];
    /// The steady frame rates to choose from; 0 is the display's own rate.
    pub const STEADY_RATES: [u32; 4] = [0, 120, 60, 30];

    pub fn steady_rate_name(hz: u32) -> String {
        if hz == 0 {
            "Display's rate".to_owned()
        } else {
            format!("{hz} a second")
        }
    }

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
        if !Self::STEADY_RATES.contains(&appearance.steady_hz) {
            appearance.steady_hz = 60;
        }
        appearance
    }
}

/// The semantic colours of one theme. Draw with these (through [`palette`])
/// rather than with colours of your own, so both themes and every accent work.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Palette {
    pub dark: bool,
    /// Settings › Accessibility › Increase contrast is on
    /// ([`Palette::with_contrast`]).
    pub high_contrast: bool,
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
                high_contrast: false,
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
                high_contrast: false,
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

impl Palette {
    /// This palette with Increase contrast on (`high`): borders one step
    /// stronger, outlines of controls and fields at 3:1 or more against the
    /// panel, muted text raised to AA (4.5:1) and a 2-point focus ring.
    pub fn with_contrast(mut self, high: bool) -> Self {
        if !high {
            return self;
        }
        let hex = |rgb: u32| Color32::from_rgb((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8);
        self.high_contrast = true;
        self.border = self.border_strong;
        if self.dark {
            self.border_strong = hex(0x76767E);
            self.text_muted = hex(0x8E8E96);
        } else {
            self.border_strong = hex(0x86868E);
            self.text_muted = hex(0x6B6B73);
        }
        self
    }

    /// The outline of a control or field: the border, or with Increase
    /// contrast the strong border, so it reaches 3:1.
    pub fn control_outline(&self) -> Color32 {
        if self.high_contrast {
            self.border_strong
        } else {
            self.border
        }
    }

    /// The accent ring around what has the keyboard: 2 points with
    /// Increase contrast.
    pub fn focus_stroke(&self) -> Stroke {
        let width = if self.high_contrast { 2.0_f32 } else { 1.0 };
        Stroke::new(width, self.accent)
    }

    /// A saturated colour (a block category's) with white text on it: as
    /// it is, or with Increase contrast darkened until the white reaches
    /// 4.5:1.
    pub fn under_white_text(&self, fill: Color32) -> Color32 {
        if !self.high_contrast {
            return fill;
        }
        let mut t = 0.0;
        let mut darker = fill;
        while contrast(Color32::WHITE, darker) < 4.5 && t < 1.0 {
            t += 0.04;
            darker = mix(fill, Color32::BLACK, t);
        }
        darker
    }
}

/// The colours of node-graph canvases (Material, UI Event Graph, Behavior
/// Tree, Dialogue, block Graph view and Animation state machines), built from
/// the [`Palette`] so graphs follow the theme and select in the accent like
/// every panel beside them. Category and pin colours are content and stay
/// the same in both themes; [`Canvas::header`] darkens a category colour
/// enough for white header text.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Canvas {
    pub background: Color32,
    /// Grid lines, which move with the nodes.
    pub grid: Color32,
    pub node_body: Color32,
    pub node_edge: Color32,
    /// Text and pin labels on node bodies.
    pub node_text: Color32,
    /// Titles on canvas cards, such as the Animation graph's guide.
    pub node_text_strong: Color32,
    /// Quieter text on node bodies, such as an unconnected pin's name.
    pub node_text_weak: Color32,
    /// Text on category headers, which are always saturated colours.
    pub header_text: Color32,
    /// Execution and transition wires.
    pub wire: Color32,
    /// The pill behind a label on a wire.
    pub label_fill: Color32,
    /// Comment boxes behind nodes.
    pub comment_fill: Color32,
    pub comment_header: Color32,
    pub comment_edge: Color32,
    pub comment_text: Color32,
    /// Selected nodes, wires and comments: the accent.
    pub selected: Color32,
    pub error: Color32,
    /// What's running now (the live state of an Animator).
    pub live: Color32,
    pub warning: Color32,
}

impl Canvas {
    pub fn new(p: &Palette) -> Self {
        let rgb = Color32::from_rgb;
        if p.dark {
            Self {
                background: rgb(22, 24, 29),
                grid: Color32::from_white_alpha(10),
                node_body: rgb(30, 32, 40),
                node_edge: rgb(12, 13, 17),
                node_text: rgb(214, 218, 228),
                node_text_strong: Color32::WHITE,
                node_text_weak: rgb(150, 154, 166),
                header_text: Color32::WHITE,
                wire: rgb(200, 204, 214),
                label_fill: rgb(40, 43, 54),
                comment_fill: Color32::from_rgba_unmultiplied(120, 130, 150, 40),
                comment_header: Color32::from_rgba_unmultiplied(120, 130, 150, 90),
                comment_edge: Color32::from_rgba_unmultiplied(160, 170, 190, 90),
                comment_text: rgb(214, 218, 228),
                selected: p.accent,
                error: p.error,
                live: p.success,
                warning: p.warning,
            }
        } else {
            Self {
                background: rgb(236, 238, 242),
                grid: Color32::from_black_alpha(14),
                node_body: Color32::WHITE,
                node_edge: rgb(196, 199, 207),
                node_text: p.text,
                node_text_strong: p.text_strong,
                node_text_weak: p.text_weak,
                header_text: Color32::WHITE,
                wire: rgb(70, 72, 80),
                label_fill: Color32::WHITE,
                comment_fill: Color32::from_rgba_unmultiplied(110, 120, 140, 30),
                comment_header: Color32::from_rgba_unmultiplied(110, 120, 140, 56),
                comment_edge: Color32::from_rgba_unmultiplied(110, 120, 140, 120),
                comment_text: p.text,
                selected: p.accent,
                error: p.error,
                live: p.success,
                warning: p.warning,
            }
        }
    }

    /// The fill of a node's header in a category's colour: a little darker,
    /// and darker still for light colours (Scratch's yellow), until its
    /// white title is readable (WCAG AA, 4.5:1) in both themes.
    pub fn header(&self, category: Color32) -> Color32 {
        let mut t = 0.12;
        let mut fill = mix(category, Color32::BLACK, t);
        while contrast(self.header_text, fill) < 4.5 && t < 1.0 {
            t += 0.04;
            fill = mix(category, Color32::BLACK, t);
        }
        fill
    }
}

/// A count that goes up once per frame of the whole editor, the same in
/// its floating windows (each OS window has its own pass count): for "was
/// this drawn last frame?".
pub fn frame_number(ctx: &egui::Context) -> u64 {
    ctx.cumulative_pass_nr_for(egui::ViewportId::ROOT)
}

/// The graph canvas colours of the theme showing now.
pub fn canvas(ctx: &egui::Context) -> Canvas {
    Canvas::new(&palette(ctx))
}

/// The WCAG contrast ratio of two colours, from 1 (the same) to 21 (black
/// on white). Text needs 4.5 (AA), large text and outlines 3.
pub fn contrast(a: Color32, b: Color32) -> f32 {
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

fn menu_icons_id() -> egui::Id {
    egui::Id::new("axcenity menu icons")
}

/// Where icons sit in menus, as picked in Settings.
pub fn menu_icons(ctx: &egui::Context) -> MenuIcons {
    ctx.data(|data| data.get_temp::<MenuIcons>(menu_icons_id()))
        .unwrap_or_default()
}

/// A menu item's icon and label as one button's contents: the icon in a
/// column of its own (so labels line up whether or not an item has an
/// icon), placed as Settings > Appearance > Menu icons says. Use it for
/// every item in a menu: `ui.button(theme::menu_entry(ui.ctx(), Some(icon), "Save"))`.
pub fn menu_entry(
    ctx: &egui::Context,
    icon: Option<&str>,
    text: impl Into<String>,
) -> egui::Atoms<'static> {
    menu_entry_tinted(ctx, icon, None, text)
}

/// [`menu_item`] with the icon drawn in `color`, such as an Insert menu's
/// class colours.
pub fn menu_item_tinted(
    ctx: &egui::Context,
    icon: &str,
    color: Color32,
    text: impl Into<String>,
) -> egui::Atoms<'static> {
    menu_entry_tinted(ctx, Some(icon), Some(color), text)
}

fn menu_entry_tinted(
    ctx: &egui::Context,
    icon: Option<&str>,
    color: Option<Color32>,
    text: impl Into<String>,
) -> egui::Atoms<'static> {
    use egui::{AtomExt as _, IntoAtoms as _};
    let icon = icon.unwrap_or("");
    let font = egui::FontId::proportional(size::ICON_MEDIUM);
    // egui paints a sized atom's text at the atom's left edge whatever its
    // `atom_align` says (the atom's cell is exactly its size), so the
    // column's left room is put in front of the glyph as leading space.
    let indent = match menu_icons(ctx) {
        MenuIcons::Left => 0.0,
        MenuIcons::Centered if icon.is_empty() => 0.0,
        MenuIcons::Centered => {
            let width = ctx.fonts_mut(|fonts| {
                fonts
                    .layout_no_wrap(icon.to_owned(), font.clone(), Color32::PLACEHOLDER)
                    .rect
                    .width()
            });
            ((MENU_ICON_COLUMN - width) / 2.0).max(0.0)
        }
    };
    let mut job = egui::text::LayoutJob::default();
    job.append(
        icon,
        indent,
        egui::TextFormat::simple(font, color.unwrap_or(Color32::PLACEHOLDER)),
    );
    let icon = egui::WidgetText::from(job)
        .atom_size(egui::vec2(MENU_ICON_COLUMN, size::ICON_MEDIUM))
        .atom_align(egui::Align2::LEFT_CENTER);
    let label: String = text.into();
    (icon, label).into_atoms()
}

/// [`menu_entry`] for an item that has an icon.
pub fn menu_item(ctx: &egui::Context, icon: &str, text: impl Into<String>) -> egui::Atoms<'static> {
    menu_entry(ctx, Some(icon), text)
}

/// How wide a menu's icon column is, in points.
pub const MENU_ICON_COLUMN: f32 = 22.0;

fn contrast_id() -> egui::Id {
    egui::Id::new("axcenity increase contrast")
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
    let high = ctx
        .data(|data| data.get_temp::<bool>(contrast_id()))
        .unwrap_or(false);
    Palette::new(ctx.global_style().visuals.dark_mode, accent).with_contrast(high)
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

/// The font family of page and dialog titles and the wordmark (Plus Jakarta
/// Sans Bold). Pair it with [`DISPLAY_TRACKING`] at display sizes.
pub fn display_family() -> FontFamily {
    FontFamily::Name("display".into())
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
        data.insert_temp(contrast_id(), appearance.increase_contrast);
        data.insert_temp(menu_icons_id(), appearance.menu_icons);
    });
    for theme in [egui::Theme::Dark, egui::Theme::Light] {
        let palette = Palette::new(theme == egui::Theme::Dark, appearance.accent)
            .with_contrast(appearance.increase_contrast);
        ctx.set_visuals_of(theme, visuals(&palette));
        ctx.style_mut_of(theme, |style| set_sizes(style, appearance));
    }
    ctx.set_theme(appearance.theme.preference());
    if (ctx.zoom_factor() - appearance.ui_scale).abs() > f32::EPSILON {
        ctx.set_zoom_factor(appearance.ui_scale);
    }
}

/// Inter, Plus Jakarta Sans and JetBrains Mono first, with Phosphor after each for icons,
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
            "Plus Jakarta Sans",
            font(include_bytes!(
                "../../../assets/fonts/PlusJakartaSans-Bold.ttf"
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
    fonts
        .families
        .insert(display_family(), family("Plus Jakarta Sans"));
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
            spacing.interact_size.y = size::row(Density::Comfortable);
            spacing.menu_margin = egui::Margin::same(6);
            spacing.window_margin = egui::Margin::same(16);
            spacing.indent = 16.0;
        }
        Density::Compact => {
            spacing.item_spacing = egui::vec2(6.0, 3.0);
            spacing.button_padding = egui::vec2(7.0, 2.0);
            spacing.interact_size.y = size::row(Density::Compact);
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

/// Letter spacing of display-size titles, in points: tight, as big type wants.
pub const DISPLAY_TRACKING: f32 = -0.55;
/// Letter spacing of dialog and settings titles, in points.
pub const TITLE_TRACKING: f32 = -0.25;
/// Letter spacing of uppercase mono micro-labels, in points.
pub const MICRO_TRACKING: f32 = 1.45;

/// Timing curves for entrances, as CSS `cubic-bezier` control points.
pub mod curve {
    /// Quick to arrive, gentle to settle: pages and cards.
    pub const SETTLE: (f32, f32, f32, f32) = (0.2, 0.8, 0.2, 1.0);
    /// Slow at both ends: the logo's cut-in and wipe.
    pub const CUT: (f32, f32, f32, f32) = (0.65, 0.0, 0.35, 1.0);
    /// The loader's sweep across the mark.
    pub const SWEEP: (f32, f32, f32, f32) = (0.4, 0.0, 0.2, 1.0);
}

/// `t` (0 to 1) along a CSS-style timing curve. The curve is a function of
/// its x, so x is found by bisection and y read there.
pub fn bezier((x1, y1, x2, y2): (f32, f32, f32, f32), t: f32) -> f32 {
    if t <= 0.0 {
        return 0.0;
    }
    if t >= 1.0 {
        return 1.0;
    }
    let at = |a: f32, b: f32, u: f32| {
        let v = 1.0 - u;
        3.0 * v * v * u * a + 3.0 * v * u * u * b + u * u * u
    };
    let (mut low, mut high) = (0.0_f32, 1.0_f32);
    for _ in 0..24 {
        let middle = (low + high) / 2.0;
        if at(x1, x2, middle) < t {
            low = middle;
        } else {
            high = middle;
        }
    }
    at(y1, y2, (low + high) / 2.0)
}

/// How far an entrance that began at `started` (egui's clock) has got, from
/// 0 to 1, after waiting `delay` seconds. 1 at once with Reduce motion on,
/// so everything shows at rest. Only ever runs after something happened:
/// an animation on a timer would wake an idle window at uneven intervals,
/// which variable refresh rate displays show as flicker.
pub fn arrival(
    ctx: &egui::Context,
    started: f64,
    delay: f32,
    seconds: f32,
    timing: (f32, f32, f32, f32),
) -> f32 {
    if ctx.global_style().animation_time <= 0.0 {
        return 1.0;
    }
    let elapsed = (ctx.input(|input| input.time) - started) as f32 - delay;
    if elapsed >= seconds {
        return 1.0;
    }
    ctx.request_repaint();
    bezier(timing, elapsed.max(0.0) / seconds)
}

/// egui's visuals for one theme, from its palette.
pub fn visuals(p: &Palette) -> egui::Visuals {
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
    v.selection.stroke = Stroke::new(p.focus_stroke().width, p.accent_text);
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
    // Panels, windows and menus are square and flat.
    v.window_corner_radius = CornerRadius::same(radius::NONE);
    v.menu_corner_radius = CornerRadius::same(radius::NONE);
    v.collapsing_header_frame = false;
    v.indent_has_left_vline = false;
    v.striped = false;
    v.slider_trailing_fill = true;
    v.handle_shape = egui::style::HandleShape::Circle;

    // Fields, checkboxes and buttons share one small corner.
    let corner = CornerRadius::same(radius::CONTROL);
    let w = &mut v.widgets;
    w.noninteractive.bg_fill = p.panel;
    w.noninteractive.weak_bg_fill = p.panel;
    w.noninteractive.bg_stroke = Stroke::new(1.0_f32, p.border);
    w.noninteractive.fg_stroke = Stroke::new(1.0_f32, p.text);
    for (state, fill, outline, text) in [
        (&mut w.inactive, p.control, p.control_outline(), p.text),
        (
            &mut w.hovered,
            p.control_hover,
            p.border_strong,
            p.text_strong,
        ),
        // Pressed, and focused from the keyboard: the accent ring shows focus.
        (&mut w.active, p.control_active, p.accent, p.text_strong),
        (&mut w.open, p.control_hover, p.control_outline(), p.text),
    ] {
        state.bg_fill = fill;
        state.weak_bg_fill = fill;
        state.bg_stroke = Stroke::new(1.0_f32, outline);
        state.fg_stroke = Stroke::new(1.0_f32, text);
        state.corner_radius = corner;
        // No growing on hover: it shifts edges by a pixel and looks jittery.
        state.expansion = 0.0;
    }
    // Pressed or focused: the ring is thicker with Increase contrast.
    w.active.bg_stroke.width = p.focus_stroke().width;
    w.noninteractive.corner_radius = corner;
    v
}

/// How far `id` is through its hover (or any on/off) transition, from 0 to
/// 1, eased. Instant when motion is reduced.
pub fn ease(ctx: &egui::Context, id: egui::Id, on: bool) -> f32 {
    let time = ctx.global_style().animation_time;
    let t = ctx.animate_bool_with_time(id, on, time);
    // Ease out: quick to respond, gentle to settle.
    1.0 - (1.0 - t) * (1.0 - t)
}

/// Fades the whole window in from the old background after the theme
/// changes, so switching doesn't flash. Call [`start_crossfade`] when it
/// changes and [`paint_crossfade`] every frame.
pub fn start_crossfade(ctx: &egui::Context, from: Color32) {
    if ctx.global_style().animation_time <= 0.0 {
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
        .rect_filled(ctx.content_rect(), 0, from.gamma_multiply(alpha));
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
        ClassName::DataLayerService => icons::STACK_SIMPLE,
        ClassName::DataLayer => icons::SQUARES_FOUR,
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
        ClassName::Ragdoll | ClassName::IKControl => icons::PERSON_SIMPLE,
        ClassName::FaceControls => icons::SMILEY,
        ClassName::Fracture => icons::HAMMER,
        ClassName::UnionOperation => icons::UNITE,
        ClassName::NegateOperation => icons::SUBTRACT,
        ClassName::Health => icons::HEART,
        ClassName::Ability => icons::LIGHTNING,
        ClassName::Effect => icons::FLASK,
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
        ClassName::RectLight => icons::FRAME_CORNERS,
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
        ClassName::SunRaysEffect => icons::SUN,
        ClassName::MotionBlurEffect => icons::WIND,
        ClassName::ParticleEmitter => icons::SPARKLE,
        ClassName::Animator => AnimKind::Animator.icon(),
        ClassName::Animation => AnimKind::Animation.icon(),
        ClassName::AnimationTrack => icons::PLAY_CIRCLE,
        ClassName::Sequence => icons::FILM_SLATE,
        ClassName::Spline => icons::BEZIER_CURVE,
        ClassName::SplinePoint => icons::DOT_OUTLINE,
        ClassName::SplineFollower => icons::TRAIN,
        ClassName::SplineMesh => icons::ROAD_HORIZON,
        ClassName::SplineScatter => icons::TREE_EVERGREEN,
        ClassName::Decal => icons::STICKER,
        ClassName::ProjectedDecal => icons::PROJECTOR_SCREEN,
        ClassName::Terrain => icons::MOUNTAINS,
        ClassName::TerrainLayer => icons::STACK,
        ClassName::ScatterRule => icons::TREE_STRUCTURE,
        ClassName::Biome => icons::MOUNTAINS,
        ClassName::TerrainPath => icons::PATH,
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

/// The icon of a part with this shape.
pub fn part_icon(shape: PartType) -> &'static str {
    match shape {
        PartType::Block => icons::CUBE,
        PartType::Ball => icons::SPHERE,
        PartType::Cylinder => icons::CYLINDER,
        PartType::Wedge => icons::TRIANGLE,
        PartType::Capsule => icons::PILL,
        // Phosphor has no pyramid; its polygon reads as a faceted solid.
        PartType::CornerWedge => icons::POLYGON,
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
        | ClassName::RectLight
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
        | ClassName::SunRaysEffect
        | ClassName::MotionBlurEffect
        | ClassName::ExposureEffect
        | ClassName::FogEffect
        | ClassName::BloomEffect => pick((232, 205, 110), (150, 115, 0)),
        ClassName::ParticleEmitter => pick((232, 150, 108), (175, 80, 30)),
        ClassName::Animator => AnimKind::Animator.color(visuals),
        ClassName::Animation | ClassName::AnimationTrack => AnimKind::Animation.color(visuals),
        ClassName::Terrain
        | ClassName::TerrainLayer
        | ClassName::ScatterRule
        | ClassName::Biome => pick((140, 200, 110), (60, 125, 35)),
        ClassName::ThirdPersonController
        | ClassName::FirstPersonController
        | ClassName::NavigationAgent
        | ClassName::Checkpoint
        | ClassName::Camera
        | ClassName::Spline
        | ClassName::SplinePoint
        | ClassName::SplineFollower
        | ClassName::SplineMesh
        | ClassName::SplineScatter
        | ClassName::TerrainPath => pick((90, 200, 160), (20, 130, 100)),
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
#[cfg(any(test, feature = "test-support"))]
pub fn test_context() -> egui::Context {
    let ctx = egui::Context::default();
    install(&ctx);
    ctx
}

/// Runs one frame of `ctx` without a GPU, for tests.
///
/// Each frame's input holds its own modifier keys, as egui's did before 0.36:
/// with no `ModifiersChanged` event, none are held (egui itself now keeps
/// the last ones until told otherwise). And since egui 0.36 a frame's texture
/// changes must reach a renderer or be cleared before they're dropped (a
/// debug check panics otherwise); with no renderer, this clears them.
#[cfg(any(test, feature = "test-support"))]
pub fn run(
    ctx: &egui::Context,
    mut input: egui::RawInput,
    ui: impl FnMut(&mut egui::Ui),
) -> egui::FullOutput {
    let held = input
        .events
        .iter()
        .any(|event| matches!(event, egui::Event::ModifiersChanged(_)));
    if !held && !ctx.input(|input| input.modifiers.is_none()) {
        input
            .events
            .insert(0, egui::Event::ModifiersChanged(egui::Modifiers::NONE));
    }
    let mut output = ctx.run_ui(input, ui);
    output.textures_delta.clear();
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_class_has_an_icon_and_every_text_style_a_font() {
        let ctx = test_context();
        let _ = crate::theme::run(&ctx, Default::default(), |ui| {
            let ctx = &ui.ctx().clone();
            ctx.fonts_mut(|fonts| {
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
                    display_family(),
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
                let style = ctx.global_style();
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
        let _ = crate::theme::run(&ctx, Default::default(), |ui| {
            let ctx = &ui.ctx().clone();
            ctx.fonts_mut(|fonts| {
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
                    let mut width = |family: FontFamily| {
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
                    assert_eq!(width(display_family()), width(filled()), "{icon:?}");
                }
            });
        });
    }

    #[test]
    fn text_and_every_accent_meet_wcag_aa_in_both_themes() {
        for (dark, high) in [(true, false), (false, false), (true, true), (false, true)] {
            for accent in Accent::ALL {
                let p = Palette::new(dark, accent).with_contrast(high);
                let label = format!("{accent:?}, dark {dark}, increased contrast {high}");
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

    /// Increase contrast (design system V18): outlines of controls and
    /// fields reach 3:1 against every surface they sit on, muted text
    /// reaches AA, the focus ring is 2 points, and the default look is
    /// left alone when it's off.
    #[test]
    fn increase_contrast_reaches_3_to_1_outlines_and_aa_muted_text() {
        for dark in [true, false] {
            let quiet = Palette::new(dark, Accent::Blue);
            assert_eq!(quiet.with_contrast(false), quiet);
            let p = quiet.with_contrast(true);
            for background in [p.app_bg, p.panel, p.surface, p.elevated, p.inset] {
                assert!(
                    contrast(p.control_outline(), background) >= 3.0,
                    "dark {dark}: {:.2}",
                    contrast(p.control_outline(), background)
                );
            }
            assert!(contrast(p.text_muted, p.panel) >= 4.5, "dark {dark}");
            assert!(contrast(p.border, p.panel) > contrast(quiet.border, quiet.panel));
            assert_eq!(p.focus_stroke().width, 2.0);
            assert_eq!(quiet.focus_stroke().width, 1.0);
            let visuals = visuals(&p);
            assert_eq!(visuals.widgets.inactive.bg_stroke.color, p.border_strong);
        }
    }

    #[test]
    fn appearance_saved_before_increase_contrast_loads_with_it_off() {
        let old = Appearance::from_text(r#"{"theme":"Light","reduce_motion":true}"#);
        assert!(old.reduce_motion);
        assert!(!old.increase_contrast);
    }

    #[test]
    fn graph_text_meets_wcag_aa_on_nodes_and_on_every_header() {
        for dark in [true, false] {
            for accent in Accent::ALL {
                let c = Canvas::new(&Palette::new(dark, accent));
                let label = format!("{accent:?}, dark {dark}");
                for text in [c.node_text, c.node_text_strong, c.node_text_weak] {
                    assert!(contrast(text, c.node_body) >= 4.5, "{label}");
                    assert!(contrast(text, c.label_fill) >= 4.5, "{label}");
                }
                assert!(contrast(c.warning, c.label_fill) >= 4.5, "{label}");
                assert!(contrast(c.comment_text, c.background) >= 4.5, "{label}");
                // Outlines and wires: 3:1 against the canvas.
                for line in [c.selected, c.wire, c.error] {
                    assert!(contrast(line, c.background) >= 3.0, "{label}");
                }
                for r in (0..=255).step_by(51) {
                    for g in (0..=255).step_by(51) {
                        for b in (0..=255).step_by(51) {
                            let fill = c.header(Color32::from_rgb(r, g, b));
                            assert!(contrast(c.header_text, fill) >= 4.5, "{r} {g} {b}");
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn category_hues_read_in_both_themes_and_stay_apart() {
        for dark in [true, false] {
            for high in [false, true] {
                let palette = Palette::new(dark, Accent::Blue).with_contrast(high);
                for hue in Hue::ALL {
                    let label = format!("{hue:?} dark {dark} high {high}");
                    // As a caption tint or a thin mark on the panel.
                    assert!(contrast(hue.ink(&palette), palette.panel) >= 3.0, "{label}");
                    if high {
                        assert!(
                            contrast(Color32::WHITE, hue.fill(&palette)) >= 4.5,
                            "{label}"
                        );
                        assert!(
                            contrast(Color32::WHITE, hue.edge(&palette)) >= 4.5,
                            "{label}"
                        );
                    }
                }
            }
        }
        for (index, a) in Hue::ALL.iter().enumerate() {
            for b in &Hue::ALL[index + 1..] {
                assert_ne!(a.base(), b.base(), "{a:?} and {b:?}");
            }
        }
    }

    #[test]
    fn menu_labels_start_at_the_same_place_with_or_without_an_icon() {
        let mut placed = Vec::new();
        for place in MenuIcons::ALL {
            let ctx = test_context();
            let appearance = Appearance {
                menu_icons: place,
                ..Default::default()
            };
            let mut output = None;
            for _ in 0..2 {
                output = Some(run(&ctx, Default::default(), |ui| {
                    apply(ui.ctx(), &appearance);
                    assert_eq!(menu_icons(ui.ctx()), place);
                    ui.vertical(|ui| {
                        let _ = ui.button(menu_item(ui.ctx(), icons::FLOPPY_DISK, "Save"));
                        let _ = ui.button(menu_entry(ui.ctx(), None, "Settings"));
                        let _ = ui.button(menu_item(ui.ctx(), icons::GEAR_SIX, "Gear"));
                    });
                }));
            }
            let mut starts = std::collections::HashMap::new();
            let mut icon_centres = Vec::new();
            for clipped in &output.unwrap().shapes {
                if let egui::Shape::Text(text) = &clipped.shape {
                    let rect = text.visual_bounding_rect();
                    match text.galley.text() {
                        "Save" | "Settings" | "Gear" => {
                            starts.insert(text.galley.text().to_owned(), rect.left());
                        }
                        text if !text.trim().is_empty() => icon_centres.push(rect.center().x),
                        _ => {}
                    }
                }
            }
            assert_eq!(starts.len(), 3, "{place:?}: {starts:?}");
            let save = starts["Save"];
            for (label, left) in &starts {
                // Glyphs' own side bearings differ by a point or so.
                assert!(
                    (left - save).abs() <= 1.5,
                    "{place:?}: {label} at {left}, Save at {save}"
                );
            }
            assert_eq!(icon_centres.len(), 2, "{place:?}");
            placed.push((icon_centres[0], save));
        }
        // Centered moves the icons to the middle of their column (they're
        // narrower than it), and the labels stay where they were.
        let [(left_icon, left_label), (centered_icon, centered_label)] = placed[..] else {
            unreachable!()
        };
        assert!(
            centered_icon > left_icon + 2.0,
            "Centered icon at {centered_icon}, Left at {left_icon}"
        );
        assert!(
            (centered_label - left_label).abs() < 0.5,
            "labels at {left_label} and {centered_label}"
        );
    }

    #[test]
    fn appearance_survives_saving_and_bad_values_are_fixed() {
        let appearance = Appearance {
            theme: ThemeChoice::Light,
            accent: Accent::Violet,
            density: Density::Compact,
            reduce_motion: true,
            increase_contrast: true,
            code_font_size: 15.0,
            ui_scale: 1.25,
            hold_refresh: true,
            steady_hz: 30,
            menu_icons: MenuIcons::Centered,
            ribbon: RibbonAlign::Left,
        };
        assert_eq!(Appearance::from_text(&appearance.to_text()), appearance);
        assert_eq!(Appearance::from_text("not json"), Appearance::default());
        let odd = Appearance::from_text(r#"{"code_font_size": 99, "ui_scale": 7}"#);
        assert_eq!(odd.code_font_size, 20.0);
        assert_eq!(odd.ui_scale, 1.0);
        assert_eq!(odd.accent, Accent::Blue);
        let odd = Appearance::from_text(r#"{"steady_hz": 7}"#);
        assert_eq!(odd.steady_hz, 60);
    }

    #[test]
    fn timing_curves_start_end_and_only_rise() {
        for timing in [curve::SETTLE, curve::CUT, curve::SWEEP] {
            assert_eq!(bezier(timing, 0.0), 0.0);
            assert_eq!(bezier(timing, 1.0), 1.0);
            let mut last = 0.0;
            for step in 1..100 {
                let y = bezier(timing, step as f32 / 100.0);
                assert!(y >= last - 0.001, "{timing:?} at {step}");
                last = y;
            }
        }
        // SETTLE is most of the way there early on.
        assert!(bezier(curve::SETTLE, 0.3) > 0.7);
    }

    #[test]
    fn entrances_finish_and_reduced_motion_skips_them() {
        let ctx = test_context();
        let at = |time: f64, ctx: &egui::Context| {
            let mut got = 0.0;
            let input = egui::RawInput {
                time: Some(time),
                ..Default::default()
            };
            let _ = crate::theme::run(ctx, input, |ui| {
                let ctx = &ui.ctx().clone();
                got = arrival(ctx, 1.0, 0.1, 0.5, curve::SETTLE);
            });
            got
        };
        assert_eq!(at(1.0, &ctx), 0.0, "still waiting out the delay");
        let middle = at(1.3, &ctx);
        assert!(middle > 0.0 && middle < 1.0, "{middle}");
        assert_eq!(at(1.7, &ctx), 1.0);
        apply(
            &ctx,
            &Appearance {
                reduce_motion: true,
                ..Appearance::default()
            },
        );
        assert_eq!(at(1.0, &ctx), 1.0);
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
        assert_eq!(ctx.global_style().spacing.interact_size.y, 20.0);
        assert_eq!(ctx.global_style().animation_time, 0.0);
        // Reduced motion makes transitions instant.
        assert_eq!(ease(&ctx, egui::Id::new("x"), true), 1.0);
    }
}
