//! Screen-space layout for UI Instances.
//!
//! `layout_ui` and `layout_children` place elements with taffy. `paint_instance`
//! and the `css_*` helpers turn a placed element into draw items. Stylesheet
//! parsing, specificity and variables live in `style.rs`. `render` stays free
//! of Instances: this crate only returns draw-list types.

mod style;

mod accessibility;
mod css;
mod gamepad;
mod interact;
mod loading;
mod paint;
mod rich;
mod scroll;
mod transition;
mod turn;
mod widget;

pub use css::*;
pub use gamepad::{GamepadButton, Gamepads, NavRepeat, PadNavigation, nav_key, pad_key};
use interact::interactable;
pub use interact::{
    FOCUS_RING_COLOR, NavKey, SCROLL_DRAG_SLOP, TOOLTIP_DELAY, UiCursor, UiEvent, UiInput,
    UiInteraction, UiNavigated, UiScrolled, UiState,
};
pub use loading::LoadingGui;
use paint::*;
pub use rich::{RichSpan, RichStroke, RichStyle, parse_rich_text, rich_text_plain};
pub use scroll::{ScrollBar, UiScroll, scroll_bars};
pub use turn::UiTurn;
pub use widget::{DROPDOWN_ROWS, UiTrack};

use axcenity_core::{ClassName, Color3, Instance, InstanceId, UDim2, Value, World};
pub use axcenity_render::UiClip;
use axcenity_render::glam::Vec3;
use axcenity_render::glam::camera::rh;
use axcenity_render::{
    Camera, TextAlign, TextShadow, TextSpan, TextStroke, UiBorderSide, UiDrawItem, UiImage,
    UiRectangle, UiShadow, UiSlice, UiText, VerticalAlign,
};
use std::collections::HashMap;
use taffy::prelude::*;

pub use style::{
    CssDeclaration, SUPPORTED_PROPERTIES, StyleWarning, Stylesheet, applied_declaration,
    applied_declarations, has_rule, own_rules, parse_stylesheet, rule_declaration,
};
use style::{PseudoClasses, ResolvedStyles, resolve_styles, resolve_variables};

/// A rectangle in physical pixels, from the top left of the screen.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct UiRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl UiRect {
    pub fn contains(&self, x: f32, y: f32) -> bool {
        x >= self.x && x <= self.x + self.width && y >= self.y && y <= self.y + self.height
    }
}

/// Where one UI element ended up on screen.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UiElement {
    pub id: InstanceId,
    pub rect: UiRect,
    pub button: bool,
    /// True for a TextBox, which takes keyboard focus when clicked.
    pub text_box: bool,
    /// True when its parent is a flex container: the flex layout places it,
    /// and its `Position` is ignored.
    pub in_flow: bool,
    /// Screen pixels per authored pixel. `1` normally. A ScreenGui in Fit
    /// mode lays out at its design size and scales the result, so a drag
    /// divides screen pixels by this to get back to authored pixels.
    pub pixels_per_unit: f32,
    /// Its text's size in authored pixels, as it's drawn: CSS `font-size`,
    /// else `TextSize`. Times `pixels_per_unit` for screen pixels.
    pub font_size: f32,
    /// False when its `Interactable` is off: it takes no hover, clicks or
    /// typing, and what's under it doesn't either.
    pub interactable: bool,
    /// The rounded box an element around it cuts it to (`overflow:
    /// hidden`, or a ScrollingFrame): outside it, it isn't drawn and
    /// doesn't take the pointer.
    pub clip: Option<UiClip>,
    /// A ScrollingFrame's canvas and how far it's scrolled.
    pub scroll: Option<UiScroll>,
    /// How it's turned on screen, by its own `Rotation` and those of the
    /// elements around it. `rect` is the box before it's turned.
    pub turn: Option<UiTurn>,
    /// A Slider's track, which the pointer picks its `Value` along.
    pub track: Option<UiTrack>,
}

impl UiElement {
    /// Whether a point is on the element: in its turned box and not cut
    /// off by a clip.
    pub fn contains(&self, x: f32, y: f32) -> bool {
        let [lx, ly] = self.local([x, y]);
        self.rect.contains(lx, ly) && self.clip.is_none_or(|clip| clip.contains(x, y))
    }

    /// A point on the screen in the element's upright layout, where `rect` is.
    pub fn local(&self, point: [f32; 2]) -> [f32; 2] {
        self.turn.map_or(point, |turn| turn.invert(point))
    }

    /// Where a point of its upright layout is drawn on the screen.
    pub fn on_screen(&self, point: [f32; 2]) -> [f32; 2] {
        self.turn.map_or(point, |turn| turn.apply(point))
    }

    /// How far it's turned on screen, in degrees clockwise.
    pub fn degrees(&self) -> f32 {
        self.turn.map_or(0.0, |turn| turn.degrees)
    }
}

/// The UI's draw data, and each element drawn, in drawing order (so later
/// elements are on top).
#[derive(Debug, Default)]
pub struct UiLayout {
    pub draw_items: Vec<UiDrawItem>,
    pub elements: Vec<UiElement>,
    pub hovered: Option<InstanceId>,
    pub active: Option<InstanceId>,
    /// The mouse cursor to show for what's under the pointer (CSS `cursor`).
    /// Only [`UiInteraction::layout`] sets it.
    pub cursor: UiCursor,
    /// True while a CSS transition runs or a tooltip is due, so a host that
    /// only draws on input should draw again.
    pub animating: bool,
    /// Problems found while laying out, such as a PageLayout page name that
    /// matches no Frame. The same text can repeat every frame; callers
    /// should show each distinct line once.
    pub warnings: Vec<String>,
}

impl UiLayout {
    /// The topmost element at a point, in physical pixels.
    pub fn hit(&self, x: f32, y: f32) -> Option<InstanceId> {
        self.top_element(x, y).map(|element| element.id)
    }

    pub fn element(&self, id: InstanceId) -> Option<&UiElement> {
        self.elements.iter().find(|element| element.id == id)
    }

    /// The topmost element at a point that takes the pointer: what `:hover`
    /// and `MouseEnter` follow. `None` when the topmost is `Interactable` off.
    pub fn hover_target(&self, x: f32, y: f32) -> Option<InstanceId> {
        self.top_element(x, y)
            .and_then(|element| element.interactable.then_some(element.id))
    }

    /// The topmost visible, interactable button at a point in physical pixels.
    pub fn hit_button(&self, x: f32, y: f32) -> Option<InstanceId> {
        self.top_element(x, y)
            .and_then(|element| (element.button && element.interactable).then_some(element.id))
    }

    /// The topmost visible, interactable text box at a point in physical pixels.
    pub fn hit_text_box(&self, x: f32, y: f32) -> Option<InstanceId> {
        self.top_element(x, y)
            .and_then(|element| (element.text_box && element.interactable).then_some(element.id))
    }

    fn top_element(&self, x: f32, y: f32) -> Option<&UiElement> {
        self.elements
            .iter()
            .rev()
            .find(|element| element.contains(x, y))
    }
}

/// What a key did to a focused text box.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TextCommand {
    /// Letters, digits and punctuation. Control characters are ignored.
    Insert(String),
    Backspace,
    /// Enter. Finishes a one-line box, and starts a new line in a multi-line one.
    Submit,
}

/// The result of [`edit_text_box`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextOutcome {
    /// The text changed and the box is still focused.
    Edited,
    /// The player finished typing. The host should fire `FocusLost`.
    Finished,
    /// `id` is not a text box, or the command changed nothing.
    Ignored,
}

/// Applies `command` to a TextBox's `Text`. A one-line box finishes on
/// Enter. A multi-line box inserts a newline instead. Clicking away is the
/// host's job: it drops focus and fires `FocusLost` itself.
pub fn edit_text_box(world: &mut World, id: InstanceId, command: TextCommand) -> TextOutcome {
    let Some(instance) = world.get(id) else {
        return TextOutcome::Ignored;
    };
    if instance.class() != ClassName::TextBox {
        return TextOutcome::Ignored;
    }
    let multiline = instance
        .property("MultiLine")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let mut text = instance
        .property("Text")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned();
    match command {
        TextCommand::Submit if !multiline => return TextOutcome::Finished,
        TextCommand::Submit => text.push('\n'),
        TextCommand::Backspace => {
            text.pop();
        }
        TextCommand::Insert(extra) => {
            let extra: String = extra
                .chars()
                .filter(|c| *c == '\n' || !c.is_control())
                .collect();
            if extra.is_empty() {
                return TextOutcome::Ignored;
            }
            if !multiline {
                text.push_str(&extra.replace(['\n', '\r'], ""));
            } else {
                text.push_str(&extra);
            }
        }
    }
    if world.set_property(id, "Text", Value::String(text)).is_err() {
        return TextOutcome::Ignored;
    }
    TextOutcome::Edited
}

/// Lays out enabled ScreenGuis into pixel-space render data.
///
/// Position and size values use scale relative to the parent plus a pixel
/// offset. Layout runs each frame so script changes and window resizing show
/// up immediately.
pub fn build_ui_draw_data(
    world: &World,
    width: u32,
    height: u32,
    stylesheets: &[Stylesheet],
) -> Vec<UiDrawItem> {
    layout_ui(world, width, height, stylesheets).draw_items
}

/// How `id`'s text is styled, as it's drawn at its authored size, but not
/// placed: what the UI Designer's text toolbar shows. `None` for an element
/// without text.
pub fn text_style(world: &World, id: InstanceId, stylesheets: &[Stylesheet]) -> Option<UiText> {
    text_style_in_state(world, id, stylesheets, UiState::Normal)
}

/// The value `property` has for `id` in `state`, with `var()`s filled in,
/// whichever rule it comes from: what the states editor shows. `None` when
/// no CSS sets it.
pub fn style_value_in_state(
    world: &World,
    id: InstanceId,
    stylesheets: &[Stylesheet],
    state: UiState,
    property: &str,
) -> Option<String> {
    let pseudo = PseudoClasses {
        forced: Some((id, state)),
        ..PseudoClasses::default()
    };
    css_property(&resolve_styles(world, id, stylesheets, pseudo), property)
}

/// The text an element shows: its `TextKey`'s value in the world's active
/// language when the string table has one, else its own `Text` (the
/// source text). `None` for an element without `Text`.
pub fn shown_text<'a>(world: &'a World, instance: &'a Instance) -> Option<&'a str> {
    let text = instance.property("Text").and_then(Value::as_str)?;
    let translated = instance
        .property("TextKey")
        .and_then(Value::as_str)
        .filter(|key| !key.is_empty())
        .and_then(|key| world.translate(key));
    Some(translated.unwrap_or(text))
}

/// [`text_style`] with `id` in `state`, for the states editor's tabs.
pub fn text_style_in_state(
    world: &World,
    id: InstanceId,
    stylesheets: &[Stylesheet],
    state: UiState,
) -> Option<UiText> {
    let instance = world.get(id)?;
    let text = shown_text(world, instance)?;
    let pseudo = PseudoClasses {
        forced: Some((id, state)),
        ..PseudoClasses::default()
    };
    let styles = resolve_styles(world, id, stylesheets, pseudo);
    Some(styled_text(
        instance,
        &styles,
        1.0,
        false,
        text,
        &mut Vec::new(),
    ))
}

/// [`build_ui_draw_data`], also saying where each element went, for picking
/// and handles in the editor.
pub fn layout_ui(world: &World, width: u32, height: u32, stylesheets: &[Stylesheet]) -> UiLayout {
    layout_ui_viewed(world, width, height, stylesheets, None, None)
}

/// [`layout_ui`], also placing BillboardGuis from `camera` and drawing a
/// focus ring on `focused` (a TextBox).
pub fn layout_ui_viewed(
    world: &World,
    width: u32,
    height: u32,
    stylesheets: &[Stylesheet],
    focused: Option<InstanceId>,
    camera: Option<&Camera>,
) -> UiLayout {
    layout_ui_with_pseudo(
        world,
        width,
        height,
        stylesheets,
        PseudoClasses {
            focused,
            ..PseudoClasses::default()
        },
        camera,
        None,
    )
}

/// Lays out UI with CSS pointer states. The active element is the button held
/// from the mouse-down event (`input.pressed`); it only gets `:active` while
/// still hovered.
pub fn layout_ui_interactive(
    world: &World,
    width: u32,
    height: u32,
    stylesheets: &[Stylesheet],
    input: UiInput,
    camera: Option<&Camera>,
) -> UiLayout {
    let focused = input.focused;
    let base = layout_ui_with_pseudo(
        world,
        width,
        height,
        stylesheets,
        PseudoClasses {
            focused,
            ..PseudoClasses::default()
        },
        camera,
        input.on_top,
    );
    let hovered = input.pointer.and_then(|[x, y]| base.hover_target(x, y));
    let active = input.pressed.filter(|button| hovered == Some(*button));
    let mut layout = layout_ui_with_pseudo(
        world,
        width,
        height,
        stylesheets,
        PseudoClasses {
            hovered,
            active,
            focused,
            ..PseudoClasses::default()
        },
        camera,
        input.on_top,
    );
    layout.hovered = hovered;
    layout.active = active;
    layout
}

/// Lays out the enabled ScreenGuis by `DisplayOrder` (ties in tree
/// order), then the BillboardGuis, then `on_top` (see [`UiInput::on_top`]).
pub(crate) fn layout_ui_with_pseudo(
    world: &World,
    width: u32,
    height: u32,
    stylesheets: &[Stylesheet],
    pseudo: PseudoClasses<'_>,
    camera: Option<&Camera>,
    on_top: Option<InstanceId>,
) -> UiLayout {
    let mut layout = UiLayout::default();
    if width == 0 || height == 0 {
        return layout;
    }

    let workspace = world.workspace();
    let mut guis: Vec<(f64, InstanceId)> = world
        .descendants(workspace)
        .into_iter()
        .filter(|&id| Some(id) != on_top && enabled_screen_gui(world, id))
        .map(|id| (number_property(world, id, "DisplayOrder", 0.0), id))
        .collect();
    // A stable sort, so equal orders stay in tree order.
    guis.sort_by(|a, b| a.0.total_cmp(&b.0));
    // The desktop player's screen has no unsafe edges.
    let insets = [0.0; 4];
    for (_, id) in guis {
        let screen = (width, height, insets);
        layout_screen_gui(world, id, screen, stylesheets, pseudo, &mut layout);
    }
    if let Some(camera) = camera {
        let (draws, elements) = (layout.draw_items.len(), layout.elements.len());
        layout_billboards(
            world,
            camera,
            width,
            height,
            &mut layout,
            stylesheets,
            pseudo,
        );
        accessibility::apply(world, &mut layout, draws, elements);
    }
    if let Some(id) = on_top
        && enabled_screen_gui(world, id)
    {
        let screen = (width, height, insets);
        layout_screen_gui(world, id, screen, stylesheets, pseudo, &mut layout);
    }
    layout
}

/// A number property of `id`, or `default` when it has none (or it isn't finite).
fn number_property(world: &World, id: InstanceId, name: &str, default: f64) -> f64 {
    world
        .get(id)
        .and_then(|instance| instance.property(name))
        .and_then(Value::as_number)
        .filter(|n| n.is_finite())
        .unwrap_or(default)
}

fn enabled_screen_gui(world: &World, id: InstanceId) -> bool {
    world.get(id).is_some_and(|instance| {
        instance.class() == ClassName::ScreenGui
            && instance
                .property("Enabled")
                .and_then(Value::as_bool)
                .expect("every ScreenGui has an Enabled property")
    })
}

/// A screen's edges a device covers or rounds off (a phone's notch and
/// home bar), in pixels: top, right, bottom, left.
pub type SafeInsets = [f32; 4];

/// Lays out one ScreenGui (`gui`) on a `width`×`height` screen with
/// `insets`, adding to `layout`.
fn layout_screen_gui(
    world: &World,
    gui: InstanceId,
    (width, height, insets): (u32, u32, SafeInsets),
    stylesheets: &[Stylesheet],
    pseudo: PseudoClasses<'_>,
    layout: &mut UiLayout,
) {
    let Some(instance) = world.get(gui) else {
        return;
    };
    let styles = resolve_styles(world, gui, stylesheets, pseudo);
    if css_property(&styles, "display").is_some_and(|display| display == "none") {
        return;
    }
    let opacity = css_number(&styles, "opacity")
        .unwrap_or(1.0)
        .clamp(0.0, 1.0);
    let (fit, bounds) = gui_area(instance, width, height, insets);
    let draws = layout.draw_items.len();
    let elements = layout.elements.len();
    let inherited = Inherited {
        opacity,
        clip: None,
    };
    layout_children(world, gui, bounds, layout, stylesheets, inherited, pseudo);
    if let Some(fit) = fit {
        apply_fit(layout, draws, elements, fit);
    }
    accessibility::apply(world, layout, draws, elements);
}

/// Whether a ScreenGui keeps out of the device's unsafe edges: its
/// `ScreenInsets` is `DeviceSafeInsets`.
fn uses_safe_insets(instance: &Instance) -> bool {
    instance
        .property("ScreenInsets")
        .and_then(Value::as_str)
        .is_some_and(|insets| insets.eq_ignore_ascii_case("DeviceSafeInsets"))
}

/// Where a ScreenGui is laid out on a `width`×`height` screen with
/// `insets`: the area its children are laid out in, and how that's scaled
/// and moved onto the screen (`None` for as it is). With `DeviceSafeInsets`
/// the screen is what's inside the insets.
fn gui_area(
    instance: &Instance,
    width: u32,
    height: u32,
    insets: SafeInsets,
) -> (Option<Fit>, UiRect) {
    let [top, right, bottom, left] = if uses_safe_insets(instance) {
        insets.map(|inset| {
            if inset.is_finite() {
                inset.max(0.0)
            } else {
                0.0
            }
        })
    } else {
        [0.0; 4]
    };
    let safe_width = (width as f32 - left - right).max(1.0);
    let safe_height = (height as f32 - top - bottom).max(1.0);
    let (width, height) = (safe_width.round() as u32, safe_height.round() as u32);
    let fit = match fit_transform(instance, width, height) {
        Some(fit) => Some(Fit {
            origin_x: fit.origin_x + left,
            origin_y: fit.origin_y + top,
            ..fit
        }),
        None if left != 0.0 || top != 0.0 => Some(Fit {
            scale: 1.0,
            origin_x: left,
            origin_y: top,
            design_width: width as f32,
            design_height: height as f32,
        }),
        None => None,
    };
    let (area_width, area_height) = fit.map_or((width as f32, height as f32), |fit| {
        (fit.design_width, fit.design_height)
    });
    let bounds = UiRect {
        x: 0.0,
        y: 0.0,
        width: area_width,
        height: area_height,
    };
    (fit, bounds)
}

/// The design size of a ScreenGui: its `DesignWidth` × `DesignHeight`, in
/// whole pixels and at least 1 each (1920×1080 if they're missing).
pub fn design_size(world: &World, gui: InstanceId) -> (u32, u32) {
    let number = |name, default: f64| {
        world
            .get(gui)
            .and_then(|instance| instance.property(name))
            .and_then(Value::as_number)
            .unwrap_or(default)
            .round()
            .max(1.0) as u32
    };
    (
        number("DesignWidth", 1920.0),
        number("DesignHeight", 1080.0),
    )
}

/// One ScreenGui on a `screen` (width and height in pixels), for the UI
/// Designer's artboard: laid out exactly as [`layout_ui`] lays it out on a
/// screen that size, then scaled by `scale` and moved by `origin` (pixels).
/// It's drawn even when `Enabled` is off, since it's being designed. Each
/// element's `pixels_per_unit` is `scale` times what it would be on that
/// screen, so a drag converts back to authored pixels.
pub fn layout_artboard(
    world: &World,
    gui: InstanceId,
    stylesheets: &[Stylesheet],
    screen: (u32, u32),
    scale: f32,
    origin: [f32; 2],
) -> UiLayout {
    layout_artboard_in_state(world, gui, stylesheets, screen, scale, origin, None)
}

/// [`layout_artboard`], with one element shown in a state (hovered,
/// pressed or disabled) as the states editor's preview does.
pub fn layout_artboard_in_state(
    world: &World,
    gui: InstanceId,
    stylesheets: &[Stylesheet],
    screen: (u32, u32),
    scale: f32,
    origin: [f32; 2],
    shown: Option<(InstanceId, UiState)>,
) -> UiLayout {
    let view = ArtboardView {
        screen,
        scale,
        origin,
        shown,
        scrolled: None,
        safe_insets: [0.0; 4],
    };
    layout_artboard_view(world, gui, stylesheets, view)
}

/// How the UI Designer shows a ScreenGui on its artboard: see
/// [`layout_artboard`] and [`layout_artboard_in_state`].
#[derive(Clone, Copy, Debug)]
pub struct ArtboardView<'a> {
    /// The screen it's laid out on, in pixels.
    pub screen: (u32, u32),
    /// Artboard pixels per screen pixel.
    pub scale: f32,
    /// Where the screen's corner is on the artboard, in its pixels.
    pub origin: [f32; 2],
    /// One element shown in a state, whatever the pointer does.
    pub shown: Option<(InstanceId, UiState)>,
    /// ScrollingFrames shown scrolled this far instead of to their own
    /// `CanvasPosition`, so the designer can scroll them without changing it.
    pub scrolled: Option<&'a HashMap<InstanceId, [f32; 2]>>,
    /// The previewed device's unsafe edges, in screen pixels: a ScreenGui
    /// with `DeviceSafeInsets` keeps inside them.
    pub safe_insets: SafeInsets,
}

/// One ScreenGui on the artboard, as `view` says.
pub fn layout_artboard_view(
    world: &World,
    gui: InstanceId,
    stylesheets: &[Stylesheet],
    view: ArtboardView<'_>,
) -> UiLayout {
    let ArtboardView {
        screen,
        scale,
        origin,
        shown,
        scrolled,
        safe_insets,
    } = view;
    let mut layout = UiLayout::default();
    let (width, height) = screen;
    layout_screen_gui(
        world,
        gui,
        (width, height, safe_insets),
        stylesheets,
        PseudoClasses {
            forced: shown,
            scrolled,
            ..PseudoClasses::default()
        },
        &mut layout,
    );
    let place = Fit {
        scale,
        origin_x: origin[0],
        origin_y: origin[1],
        design_width: width as f32,
        design_height: height as f32,
    };
    for item in &mut layout.draw_items {
        scale_draw_item(item, place);
    }
    for element in &mut layout.elements {
        scale_element(element, place);
        element.pixels_per_unit *= scale;
    }
    layout
}

/// How a `width`×`height` screen shows the ScreenGui `gui`: screen pixels
/// per design pixel, and where design pixel (0, 0) lands on the screen.
/// Fixed is 1 at the corner; Fit and Scale scale the design size to fit.
pub fn screen_mapping(world: &World, gui: InstanceId, width: u32, height: u32) -> (f32, [f32; 2]) {
    let (scale, origin, _) = safe_screen_mapping(world, gui, width, height, [0.0; 4]);
    (scale, origin)
}

/// [`screen_mapping`] on a screen with unsafe edges (`insets`), which a
/// ScreenGui with `DeviceSafeInsets` keeps inside, and the size of the
/// area it's laid out in, in design pixels.
pub fn safe_screen_mapping(
    world: &World,
    gui: InstanceId,
    width: u32,
    height: u32,
    insets: SafeInsets,
) -> (f32, [f32; 2], [f32; 2]) {
    let Some(instance) = world.get(gui) else {
        return (1.0, [0.0, 0.0], [width as f32, height as f32]);
    };
    let (fit, area) = gui_area(instance, width, height, insets);
    let (scale, origin) = fit.map_or((1.0, [0.0, 0.0]), |fit| {
        (fit.scale, [fit.origin_x, fit.origin_y])
    });
    (scale, origin, [area.width, area.height])
}

/// How a ScreenGui in Fit mode maps its design rectangle onto the viewport:
/// uniform scale, centered, so the design stays proportional and inside the screen.
#[derive(Clone, Copy)]
struct Fit {
    scale: f32,
    origin_x: f32,
    origin_y: f32,
    design_width: f32,
    design_height: f32,
}

/// `Some` when this ScreenGui's `ScaleMode` is `Fit` or `Scale`.
///
/// Both scale the design size evenly by as much as fits the screen. Fit
/// lays out on the design rectangle and centres it, leaving empty bars on
/// another aspect ratio. Scale lays out on the whole screen, measured in
/// design pixels, so what's placed against an edge stays against it.
fn fit_transform(instance: &Instance, width: u32, height: u32) -> Option<Fit> {
    let mode = instance.property("ScaleMode").and_then(Value::as_str)?;
    let whole_screen = mode.eq_ignore_ascii_case("Scale");
    if !whole_screen && !mode.eq_ignore_ascii_case("Fit") {
        return None;
    }
    let design_width = instance
        .property("DesignWidth")
        .and_then(Value::as_number)
        .unwrap_or(1920.0)
        .max(1.0) as f32;
    let design_height = instance
        .property("DesignHeight")
        .and_then(Value::as_number)
        .unwrap_or(1080.0)
        .max(1.0) as f32;
    let scale = (width as f32 / design_width).min(height as f32 / design_height);
    if !scale.is_finite() || scale <= 0.0 {
        return None;
    }
    if whole_screen {
        return Some(Fit {
            scale,
            origin_x: 0.0,
            origin_y: 0.0,
            design_width: width as f32 / scale,
            design_height: height as f32 / scale,
        });
    }
    Some(Fit {
        scale,
        origin_x: (width as f32 - design_width * scale) * 0.5,
        origin_y: (height as f32 - design_height * scale) * 0.5,
        design_width,
        design_height,
    })
}

/// Scales everything `layout_children` just appended, from design pixels into the viewport.
fn apply_fit(layout: &mut UiLayout, draws: usize, elements: usize, fit: Fit) {
    for item in &mut layout.draw_items[draws..] {
        scale_draw_item(item, fit);
    }
    for element in &mut layout.elements[elements..] {
        scale_element(element, fit);
        element.pixels_per_unit = fit.scale;
    }
}

/// An element's box, clip and turn, scaled and moved by `fit`.
fn scale_element(element: &mut UiElement, fit: Fit) {
    element.rect = scale_rect(element.rect, fit);
    element.clip = element.clip.map(|clip| scale_clip(clip, fit));
    element.turn = element
        .turn
        .map(|turn| turn.scaled(fit.scale, [fit.origin_x, fit.origin_y]));
}

fn scale_clip(clip: UiClip, fit: Fit) -> UiClip {
    let rect = scale_rect(
        UiRect {
            x: clip.position[0],
            y: clip.position[1],
            width: clip.size[0],
            height: clip.size[1],
        },
        fit,
    );
    UiClip {
        position: [rect.x, rect.y],
        size: [rect.width, rect.height],
        radius: clip.radius * fit.scale,
        rotation_degrees: clip.rotation_degrees,
    }
}

/// Cuts a draw item to `clip`, inside any clip it already has.
pub(crate) fn set_clip(item: &mut UiDrawItem, clip: Option<UiClip>) {
    let Some(clip) = clip else {
        return;
    };
    let slot = match item {
        UiDrawItem::Rectangle(rectangle) => &mut rectangle.clip,
        UiDrawItem::Image(image) => &mut image.clip,
        UiDrawItem::Text(text) => &mut text.clip,
    };
    *slot = Some(slot.map_or(clip, |own| own.intersect(clip)));
}

fn scale_rect(rect: UiRect, fit: Fit) -> UiRect {
    UiRect {
        x: fit.origin_x + rect.x * fit.scale,
        y: fit.origin_y + rect.y * fit.scale,
        width: rect.width * fit.scale,
        height: rect.height * fit.scale,
    }
}

fn scale_draw_item(item: &mut UiDrawItem, fit: Fit) {
    transform_draw_item(
        item,
        Affine {
            scale: [fit.scale, fit.scale],
            offset: [fit.origin_x, fit.origin_y],
        },
    );
}

/// A 2D scale then move: a point `p` goes to `p * scale + offset`.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Affine {
    scale: [f32; 2],
    offset: [f32; 2],
}

impl Affine {
    const IDENTITY: Affine = Affine {
        scale: [1.0, 1.0],
        offset: [0.0, 0.0],
    };

    /// `other` first, then `self`.
    fn after(self, other: Affine) -> Affine {
        Affine {
            scale: [
                self.scale[0] * other.scale[0],
                self.scale[1] * other.scale[1],
            ],
            offset: [
                self.scale[0] * other.offset[0] + self.offset[0],
                self.scale[1] * other.offset[1] + self.offset[1],
            ],
        }
    }

    fn moved(x: f32, y: f32) -> Affine {
        Affine {
            scale: [1.0, 1.0],
            offset: [x, y],
        }
    }
}

/// CSS `transform` on an element at `rect`, around its centre: `scale`,
/// `scaleX`, `scaleY`, `translate`, `translateX` and `translateY` (px, or %
/// of the element's own size), applied right to left as in CSS. `None` for
/// `none`, nothing it knows, or no change. Scales are taken as positive.
fn css_transform(styles: &ResolvedStyles, rect: UiRect) -> Option<Affine> {
    let value = css_property(styles, "transform")?;
    let mut transform = Affine::IDENTITY;
    let mut rest = value.trim();
    while let Some(open) = rest.find('(') {
        let name = rest[..open].trim().to_ascii_lowercase();
        let close = open + rest[open..].find(')')?;
        let arguments: Vec<&str> = rest[open + 1..close].split(',').map(str::trim).collect();
        rest = &rest[close + 1..];
        let number = |text: &str| {
            text.parse::<f32>()
                .ok()
                .filter(|n| n.is_finite())
                .map(f32::abs)
        };
        let length = |text: &str, along: f32| {
            let pixels = if let Some(percent) = text.strip_suffix('%') {
                percent
                    .trim()
                    .parse::<f32>()
                    .ok()
                    .map(|p| p / 100.0 * along)
            } else {
                text.strip_suffix("px")
                    .unwrap_or(text)
                    .trim()
                    .parse::<f32>()
                    .ok()
            };
            pixels.filter(|n| n.is_finite())
        };
        let step = match (name.as_str(), arguments.as_slice()) {
            ("scale", [s]) => Affine {
                scale: [number(s)?; 2],
                offset: [0.0, 0.0],
            },
            ("scale", [x, y]) => Affine {
                scale: [number(x)?, number(y)?],
                offset: [0.0, 0.0],
            },
            ("scalex", [x]) => Affine {
                scale: [number(x)?, 1.0],
                offset: [0.0, 0.0],
            },
            ("scaley", [y]) => Affine {
                scale: [1.0, number(y)?],
                offset: [0.0, 0.0],
            },
            ("translate", [x]) => Affine::moved(length(x, rect.width)?, 0.0),
            ("translate", [x, y]) => Affine::moved(length(x, rect.width)?, length(y, rect.height)?),
            ("translatex", [x]) => Affine::moved(length(x, rect.width)?, 0.0),
            ("translatey", [y]) => Affine::moved(0.0, length(y, rect.height)?),
            _ => return None,
        };
        transform = transform.after(step);
    }
    if transform == Affine::IDENTITY {
        return None;
    }
    let centre = [rect.x + rect.width * 0.5, rect.y + rect.height * 0.5];
    Some(
        Affine::moved(centre[0], centre[1])
            .after(transform)
            .after(Affine::moved(-centre[0], -centre[1])),
    )
}

fn transform_draw_item(item: &mut UiDrawItem, affine: Affine) {
    let [sx, sy] = affine.scale;
    // Lengths that aren't along one axis (corners, borders, font sizes).
    let uniform = (sx.abs() * sy.abs()).sqrt();
    let place = |position: &mut [f32; 2], size: &mut [f32; 2]| {
        position[0] = affine.offset[0] + position[0] * sx;
        position[1] = affine.offset[1] + position[1] * sy;
        size[0] *= sx;
        size[1] *= sy;
    };
    let clip = match item {
        UiDrawItem::Rectangle(rectangle) => &mut rectangle.clip,
        UiDrawItem::Image(image) => &mut image.clip,
        UiDrawItem::Text(text) => &mut text.clip,
    };
    if let Some(clip) = clip {
        place(&mut clip.position, &mut clip.size);
        clip.radius *= uniform;
    }
    match item {
        UiDrawItem::Rectangle(rectangle) => {
            place(&mut rectangle.position, &mut rectangle.size);
            rectangle.border_width *= uniform;
            rectangle.border_radius *= uniform;
            if let Some(sides) = &mut rectangle.border_sides {
                for side in sides {
                    side.width *= uniform;
                }
            }
            if let Some(shadow) = &mut rectangle.shadow {
                shadow.offset = [shadow.offset[0] * sx, shadow.offset[1] * sy];
                shadow.blur *= uniform;
                shadow.spread *= uniform;
            }
        }
        UiDrawItem::Image(image) => {
            place(&mut image.position, &mut image.size);
            image.border_radius *= uniform;
            if let Some(slice) = &mut image.slice {
                let [top, right, bottom, left] = slice.widths;
                slice.widths = [top * sy, right * sx, bottom * sy, left * sx];
            }
        }
        UiDrawItem::Text(text) => {
            place(&mut text.position, &mut text.size);
            text.font_size *= uniform;
            text.letter_spacing *= uniform;
            if let Some(height) = &mut text.line_height {
                *height *= uniform;
            }
            if let Some(stroke) = &mut text.stroke {
                stroke.width *= uniform;
            }
            for span in &mut text.spans {
                span.font_size *= uniform;
                if let Some(stroke) = &mut span.stroke {
                    stroke.width *= uniform;
                }
            }
            for shadow in &mut text.shadows {
                shadow.offset = [shadow.offset[0] * sx, shadow.offset[1] * sy];
                shadow.blur *= uniform;
            }
        }
    }
}

/// What an element passes down to its children: how see-through it and
/// its ancestors make them, and the box they're cut to.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Inherited {
    pub(crate) opacity: f32,
    pub(crate) clip: Option<UiClip>,
}

/// Lays out the UI children of `parent` inside `bounds` with Taffy.
///
/// Outside a flex container, children are absolutely positioned: scale is a
/// percentage of `bounds` and the UDim2 pixel offsets are added on top. In a
/// flex container (`display: flex`), Taffy flows them and `Position` is
/// ignored. Either way, CSS `width`/`height` replace `Size`. They're laid out
/// in tree order but drawn (and so hit) in [`z_order`].
pub(crate) fn layout_children(
    world: &World,
    parent: InstanceId,
    bounds: UiRect,
    out: &mut UiLayout,
    stylesheets: &[Stylesheet],
    inherited: Inherited,
    pseudo: PseudoClasses<'_>,
) {
    let Some(parent_instance) = world.get(parent) else {
        return;
    };
    let children: Vec<_> = parent_instance
        .children()
        .iter()
        .copied()
        .filter(|&id| included_child(world, parent, id, out))
        .collect();
    if children.is_empty() {
        return;
    }
    if parent_instance.class() == ClassName::Table {
        layout_table(
            world,
            &children,
            bounds,
            out,
            stylesheets,
            inherited,
            pseudo,
        );
        return;
    }

    let parent_styles = resolve_styles(world, parent, stylesheets, pseudo);
    let flow_children =
        css_property(&parent_styles, "display").is_some_and(|display| display == "flex");
    let mut root_style = Style {
        display: Display::Flex,
        size: Size {
            width: Dimension::Length(bounds.width),
            height: Dimension::Length(bounds.height),
        },
        ..Style::default()
    };
    if flow_children {
        apply_container_style(&mut root_style, &parent_styles);
    }
    // The space that `Size` scale is a fraction of: inside the padding of a
    // flex container, or all of `bounds` for absolutely positioned children.
    let padding = root_style.padding;
    let container = Size {
        width: bounds.width
            - resolve(padding.left, bounds.width)
            - resolve(padding.right, bounds.width),
        height: bounds.height
            - resolve(padding.top, bounds.width)
            - resolve(padding.bottom, bounds.width),
    };

    let styles: Vec<ResolvedStyles> = children
        .iter()
        .map(|&id| resolve_styles(world, id, stylesheets, pseudo))
        .collect();
    let mut tree = TaffyTree::<()>::new();
    let nodes: Vec<_> = children
        .iter()
        .zip(&styles)
        .map(|(&id, styles)| {
            let instance = world.get(id).expect("filtered children are live");
            let position = ui_property(instance, "Position");
            let size = ui_property(instance, "Size");
            let hidden = css_property(styles, "display").is_some_and(|display| display == "none");
            let node_style = Style {
                position: if flow_children {
                    Position::Relative
                } else {
                    Position::Absolute
                },
                inset: if flow_children {
                    Rect::auto()
                } else {
                    Rect {
                        left: LengthPercentageAuto::Percent(finite(position.x.scale)),
                        top: LengthPercentageAuto::Percent(finite(position.y.scale)),
                        right: LengthPercentageAuto::Auto,
                        bottom: LengthPercentageAuto::Auto,
                    }
                },
                // With `aspect-ratio`, the width decides the height.
                aspect_ratio: css_aspect_ratio(styles),
                size: Size {
                    width: css_dimension(styles, "width")
                        .unwrap_or_else(|| udim_length(size.x, container.width)),
                    height: if css_aspect_ratio(styles).is_some() {
                        Dimension::Auto
                    } else {
                        css_dimension(styles, "height")
                            .unwrap_or_else(|| udim_length(size.y, container.height))
                    },
                },
                flex_grow: css_number(styles, "flex-grow").unwrap_or(0.0).max(0.0),
                margin: css_margin(styles),
                display: if hidden || !visible(instance) {
                    Display::None
                } else {
                    Display::Flex
                },
                ..Style::default()
            };
            tree.new_leaf(node_style)
                .expect("a UI node can be added to a fresh Taffy tree")
        })
        .collect();
    let root = tree
        .new_with_children(root_style, &nodes)
        .expect("UI nodes form a valid Taffy tree");
    tree.compute_layout(
        root,
        Size {
            width: AvailableSpace::Definite(bounds.width),
            height: AvailableSpace::Definite(bounds.height),
        },
    )
    .expect("the UI root has definite dimensions");

    for index in z_order(world, &children, &styles) {
        let (id, styles, node) = (children[index], &styles[index], nodes[index]);
        let Some(instance) = world.get(id) else {
            continue;
        };
        let hidden = css_property(styles, "display").is_some_and(|display| display == "none");
        if hidden || !visible(instance) {
            continue;
        }
        let opacity =
            inherited.opacity * css_number(styles, "opacity").unwrap_or(1.0).clamp(0.0, 1.0);
        let layout = tree.layout(node).expect("each child has a computed layout");
        let position = ui_property(instance, "Position");
        let (offset_x, offset_y) = if flow_children {
            (0.0, 0.0)
        } else {
            (finite(position.x.offset), finite(position.y.offset))
        };
        let rect = UiRect {
            x: bounds.x + layout.location.x + offset_x,
            y: bounds.y + layout.location.y + offset_y,
            width: layout.size.width.max(0.0),
            height: layout.size.height.max(0.0),
        };
        let placed = Placed {
            rect,
            inherited: Inherited {
                opacity,
                clip: inherited.clip,
            },
            in_flow: flow_children,
        };
        place_element(world, id, styles, placed, out, stylesheets, pseudo);
    }
}

/// The order `children` are drawn in, as indices into it: lowest
/// [`z_index`] first, and equal ones in tree order, so the last is on top.
fn z_order(world: &World, children: &[InstanceId], styles: &[ResolvedStyles]) -> Vec<usize> {
    let z: Vec<f64> = children
        .iter()
        .zip(styles)
        .map(|(&id, styles)| {
            world
                .get(id)
                .map_or(1.0, |instance| z_index(instance, styles))
        })
        .collect();
    let mut order: Vec<usize> = (0..children.len()).collect();
    // `sort_by` is stable: ties keep their order.
    order.sort_by(|&a, &b| z[a].total_cmp(&z[b]));
    order
}

/// CSS `aspect-ratio`: width over height, as `16 / 9`, `1.5` or `auto 4 / 3`
/// (`auto` alone, or nothing sensible, is `None`).
pub(crate) fn css_aspect_ratio(styles: &ResolvedStyles) -> Option<f32> {
    let value = css_property(styles, "aspect-ratio")?;
    let ratio = value
        .split_whitespace()
        .filter(|word| !word.eq_ignore_ascii_case("auto"))
        .collect::<String>();
    let number = |text: &str| {
        text.trim()
            .parse::<f32>()
            .ok()
            .filter(|n| n.is_finite() && *n > 0.0)
    };
    match ratio.split_once('/') {
        Some((width, height)) => Some(number(width)? / number(height)?),
        None => number(&ratio),
    }
}

/// `id`'s CSS `aspect-ratio` (width over height), if it keeps one.
pub fn aspect_ratio_of(world: &World, id: InstanceId, stylesheets: &[Stylesheet]) -> Option<f32> {
    css_aspect_ratio(&resolve_styles(
        world,
        id,
        stylesheets,
        PseudoClasses::default(),
    ))
}

/// An element's place among its siblings: CSS `z-index` when it's set to
/// a number, else its `ZIndex` (1 by default).
pub fn z_index_of(world: &World, id: InstanceId, stylesheets: &[Stylesheet]) -> f64 {
    let styles = resolve_styles(world, id, stylesheets, PseudoClasses::default());
    world
        .get(id)
        .map_or(1.0, |instance| z_index(instance, &styles))
}

/// Whether CSS `z-index` (rather than `ZIndex`) places `id`.
pub fn css_sets_z_index(world: &World, id: InstanceId, stylesheets: &[Stylesheet]) -> bool {
    let styles = resolve_styles(world, id, stylesheets, PseudoClasses::default());
    css_number(&styles, "z-index").is_some()
}

fn z_index(instance: &Instance, styles: &ResolvedStyles) -> f64 {
    css_number(styles, "z-index")
        .map(f64::from)
        .or_else(|| instance.property("ZIndex").and_then(Value::as_number))
        .filter(|z| z.is_finite())
        .unwrap_or(1.0)
}

/// Where layout put an element, what its ancestors pass down to it, and
/// whether a flex layout placed it.
#[derive(Clone, Copy)]
struct Placed {
    rect: UiRect,
    inherited: Inherited,
    in_flow: bool,
}

/// Whether an element cuts its children to its box: CSS `overflow` other
/// than `visible`, or a ScrollingFrame, which always does.
fn clips_children(instance: &Instance, styles: &ResolvedStyles) -> bool {
    instance.class() == ClassName::ScrollingFrame
        || css_keyword(styles, "overflow").is_some_and(|overflow| {
            matches!(overflow.as_str(), "hidden" | "clip" | "scroll" | "auto")
        })
}

/// The box an element at `rect` cuts its children to: inside its border,
/// with its corners rounded as its border's inside is.
fn inner_clip(rect: UiRect, styles: &ResolvedStyles) -> UiClip {
    let [top, right, bottom, left] = css_borders(styles).widths();
    let radius = css_length_property(styles, "border-radius")
        .unwrap_or(0.0)
        .max(0.0);
    let widest = top.max(right).max(bottom).max(left);
    UiClip {
        position: [rect.x + left, rect.y + top],
        size: [
            (rect.width - left - right).max(0.0),
            (rect.height - top - bottom).max(0.0),
        ],
        radius: (radius - widest).max(0.0),
        rotation_degrees: 0.0,
    }
}

/// Records and paints one element laid out at `placed`, then its
/// children. Its CSS `transform` moves and scales what it and its children
/// draw, but not where they are for layout and clicks; its `Rotation`
/// turns both, around its middle. A ScrollingFrame's children go on its
/// canvas, under its bars. A widget's parts go over its background.
fn place_element(
    world: &World,
    id: InstanceId,
    styles: &ResolvedStyles,
    placed: Placed,
    out: &mut UiLayout,
    stylesheets: &[Stylesheet],
    pseudo: PseudoClasses<'_>,
) {
    let Some(instance) = world.get(id) else {
        return;
    };
    let Placed {
        rect,
        inherited,
        in_flow,
    } = placed;
    let draws = out.draw_items.len();
    let elements = out.elements.len();
    let rotation = turn::rotation(instance);
    // A turned element's subtree is cut to the clip around it only once
    // it's turned, so that clip isn't turned with it.
    let outer_clip = inherited.clip;
    let clip = if rotation.is_some() { None } else { outer_clip };
    let scroll = (instance.class() == ClassName::ScrollingFrame)
        .then(|| scroll::measure(world, id, instance, styles, rect, stylesheets, pseudo));
    out.elements.push(UiElement {
        id,
        rect,
        button: instance.class() == ClassName::TextButton,
        text_box: instance.class() == ClassName::TextBox,
        in_flow,
        pixels_per_unit: 1.0,
        font_size: paint::font_size(instance, styles),
        interactable: interactable(instance),
        clip,
        scroll,
        turn: None,
        track: None,
    });
    paint_instance(
        world,
        instance,
        rect,
        styles,
        inherited.opacity,
        // A TextBox being typed in has a ring in its text colour, unless a
        // `:focus` rule styles it.
        pseudo.focused == Some(id) && !style::focus_rule_applies(world, id, stylesheets, pseudo),
        &mut out.draw_items,
        &mut out.warnings,
    );
    if instance.class().is_value_widget() {
        let opacity = inherited.opacity;
        out.elements[elements].track =
            widget::paint_widget(world, id, rect, styles, opacity, stylesheets, pseudo, out);
    }
    for item in &mut out.draw_items[draws..] {
        set_clip(item, clip);
    }
    let children = Inherited {
        opacity: inherited.opacity,
        clip: if clips_children(instance, styles) {
            let own = inner_clip(rect, styles);
            Some(clip.map_or(own, |outer| own.intersect(outer)))
        } else {
            clip
        },
    };
    let canvas = scroll.map_or(rect, |scroll| scroll.canvas_rect(rect));
    layout_children(world, id, canvas, out, stylesheets, children, pseudo);
    if let Some(scroll) = &scroll {
        scroll::paint_bars(
            world,
            id,
            rect,
            scroll,
            children,
            stylesheets,
            pseudo,
            &mut out.draw_items,
        );
    }
    if let Some(transform) = css_transform(styles, rect) {
        for item in &mut out.draw_items[draws..] {
            transform_draw_item(item, transform);
        }
    }
    if let Some(degrees) = rotation {
        let turn = UiTurn::around(
            degrees,
            [rect.x + rect.width * 0.5, rect.y + rect.height * 0.5],
        );
        for item in &mut out.draw_items[draws..] {
            turn::turn_draw_item(item, turn);
            set_clip(item, outer_clip);
        }
        for element in &mut out.elements[elements..] {
            element.turn = Some(element.turn.map_or(turn, |inner| inner.then(turn)));
            element.clip = element
                .clip
                .map(|clip| turn::turn_clip(clip, turn))
                .map(|clip| outer_clip.map_or(clip, |outer| clip.intersect(outer)))
                .or(outer_clip);
        }
    }
}

fn visible(instance: &Instance) -> bool {
    instance
        .property("Visible")
        .and_then(Value::as_bool)
        .expect("every UI element has a Visible property")
}

fn is_widget(class: ClassName) -> bool {
    matches!(
        class,
        ClassName::Frame
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
    )
}

/// Whether `id` is drawn under `parent`. A PageLayout shows one Frame, the
/// page named by `CurrentPage`, or the first Frame when that name is empty
/// or missing.
fn included_child(world: &World, parent: InstanceId, id: InstanceId, out: &mut UiLayout) -> bool {
    let Some(child) = world.get(id) else {
        return false;
    };
    if !is_widget(child.class()) {
        return false;
    }
    let Some(parent_instance) = world.get(parent) else {
        return false;
    };
    if parent_instance.class() != ClassName::PageLayout {
        return true;
    }
    if child.class() != ClassName::Frame {
        out.warnings.push(format!(
            "PageLayout \"{}\" only switches Frames. \"{}\" stays visible on every page.",
            parent_instance.name(),
            child.name()
        ));
        return true;
    }
    current_page(world, parent, out) == Some(id)
}

fn current_page(world: &World, parent: InstanceId, out: &mut UiLayout) -> Option<InstanceId> {
    let layout = world.get(parent)?;
    let frames: Vec<InstanceId> = layout
        .children()
        .iter()
        .copied()
        .filter(|&id| {
            world
                .get(id)
                .is_some_and(|child| child.class() == ClassName::Frame)
        })
        .collect();
    let name = layout
        .property("CurrentPage")
        .and_then(Value::as_str)
        .unwrap_or("");
    if !name.is_empty() {
        if let Some(id) = frames
            .iter()
            .copied()
            .find(|&id| world.get(id).is_some_and(|child| child.name() == name))
        {
            return Some(id);
        }
        out.warnings.push(format!(
            "PageLayout \"{}\" has no page named \"{name}\", so it shows the first page.",
            layout.name()
        ));
    }
    frames
        .into_iter()
        .find(|&id| world.get(id).is_some_and(visible))
}

fn layout_table(
    world: &World,
    children: &[InstanceId],
    bounds: UiRect,
    out: &mut UiLayout,
    stylesheets: &[Stylesheet],
    inherited: Inherited,
    pseudo: PseudoClasses<'_>,
) {
    let shown: Vec<InstanceId> = children
        .iter()
        .copied()
        .filter(|&id| {
            world.get(id).is_some_and(|child| {
                let styles = resolve_styles(world, id, stylesheets, pseudo);
                visible(child)
                    && !css_property(&styles, "display").is_some_and(|display| display == "none")
            })
        })
        .collect();
    if shown.is_empty() || bounds.width <= 0.0 || bounds.height <= 0.0 {
        return;
    }
    let columns = world
        .get(children[0])
        .and_then(|child| child.parent())
        .and_then(|parent| world.get(parent))
        .and_then(|table| table.property("Columns"))
        .and_then(Value::as_number)
        .map(|n| (n as isize).max(1) as usize)
        .unwrap_or(2);
    let gap = 8.0_f32;
    let rows = shown.len().div_ceil(columns).max(1);
    let cell_w =
        ((bounds.width - gap * (columns.saturating_sub(1) as f32)) / columns as f32).max(0.0);
    let cell_h = ((bounds.height - gap * (rows.saturating_sub(1) as f32)) / rows as f32).max(0.0);
    let styles: Vec<ResolvedStyles> = shown
        .iter()
        .map(|&id| resolve_styles(world, id, stylesheets, pseudo))
        .collect();
    for index in z_order(world, &shown, &styles) {
        let (id, styles) = (shown[index], &styles[index]);
        let opacity =
            inherited.opacity * css_number(styles, "opacity").unwrap_or(1.0).clamp(0.0, 1.0);
        let column = index % columns;
        let row = index / columns;
        let rect = UiRect {
            x: bounds.x + column as f32 * (cell_w + gap),
            y: bounds.y + row as f32 * (cell_h + gap),
            width: cell_w,
            height: cell_h,
        };
        let placed = Placed {
            rect,
            inherited: Inherited {
                opacity,
                clip: inherited.clip,
            },
            in_flow: true,
        };
        place_element(world, id, styles, placed, out, stylesheets, pseudo);
    }
}

fn layout_billboards(
    world: &World,
    camera: &Camera,
    width: u32,
    height: u32,
    out: &mut UiLayout,
    stylesheets: &[Stylesheet],
    pseudo: PseudoClasses<'_>,
) {
    if width == 0 || height == 0 {
        return;
    }
    let aspect = width as f32 / height as f32;
    let view = rh::view::look_at_mat4(camera.position, camera.look_at, Vec3::Y);
    let tan_half = (camera.fov_y_degrees.to_radians() * 0.5).tan();
    for id in world.descendants(world.workspace()) {
        let Some(instance) = world.get(id) else {
            continue;
        };
        if instance.class() != ClassName::BillboardGui
            || instance.property("Enabled").and_then(Value::as_bool) != Some(true)
        {
            continue;
        }
        let Some(position) = instance
            .parent()
            .and_then(|parent| world.get(parent))
            .and_then(|parent| parent.property("Position"))
            .and_then(Value::as_vector3)
        else {
            continue;
        };
        let offset = instance
            .property("StudsOffset")
            .and_then(Value::as_vector3)
            .unwrap_or(axcenity_core::Vector3::new(0.0, 0.0, 0.0));
        let anchor = Vec3::new(
            position.x + offset.x,
            position.y + offset.y,
            position.z + offset.z,
        );
        let max_distance = instance
            .property("MaxDistance")
            .and_then(Value::as_number)
            .unwrap_or(0.0) as f32;
        if max_distance > 0.0 && camera.position.distance(anchor) > max_distance {
            continue;
        }
        let camera_space = view.transform_point3(anchor);
        if camera_space.z >= -0.05 {
            continue;
        }
        let depth = -camera_space.z;
        let ndc_x = camera_space.x / (depth * tan_half * aspect);
        let ndc_y = camera_space.y / (depth * tan_half);
        if !ndc_x.is_finite() || !ndc_y.is_finite() {
            continue;
        }
        let size = instance
            .property("Size")
            .and_then(Value::as_udim2)
            .unwrap_or_default();
        let w = finite(size.x.scale) * width as f32 + finite(size.x.offset);
        let h = finite(size.y.scale) * height as f32 + finite(size.y.offset);
        let rect = UiRect {
            x: (ndc_x + 1.0) * 0.5 * width as f32 - w * 0.5,
            y: (1.0 - ndc_y) * 0.5 * height as f32 - h * 0.5,
            width: w.max(0.0),
            height: h.max(0.0),
        };
        let inherited = Inherited {
            opacity: 1.0,
            clip: None,
        };
        layout_children(world, id, rect, out, stylesheets, inherited, pseudo);
    }
}

fn font_family_name(family: &str) -> String {
    let name = family
        .split(',')
        .next()
        .unwrap_or("Montserrat")
        .trim()
        .trim_matches(['\'', '"']);
    match name.to_ascii_lowercase().as_str() {
        "monospace" | "jetbrains mono" | "jetbrainsmono" => "JetBrains Mono".to_owned(),
        "sans-serif" | "montserrat" | "" => "Montserrat".to_owned(),
        "serif" => "Lora".to_owned(),
        _ => name.to_owned(),
    }
}

/// A UDim's length in pixels: `scale` of `container` plus `offset`.
fn udim_length(value: axcenity_core::UDim, container: f32) -> Dimension {
    Dimension::Length((finite(value.scale) * container + finite(value.offset)).max(0.0))
}

/// A padding length in pixels. Percentages are of the width, as in CSS.
fn resolve(length: LengthPercentage, width: f32) -> f32 {
    match length {
        LengthPercentage::Length(pixels) => pixels,
        LengthPercentage::Percent(fraction) => fraction * width,
    }
}

/// `value`, a CSS length as written (`280px` or `72%`), changed for an
/// element that goes from `old` to `new` pixels on that axis. Pixels are
/// rounded to whole ones, and percentages scale with the size. `None` if the
/// value isn't a plain length (such as `var(--width)` or `auto`), or a
/// percentage of nothing.
pub fn resized_css_length(value: &str, old: f32, new: f32) -> Option<String> {
    let value = value.trim();
    if let Some(percent) = value.strip_suffix('%') {
        let percent: f32 = percent
            .trim()
            .parse()
            .ok()
            .filter(|p: &f32| p.is_finite())?;
        if old <= 0.0 {
            return None;
        }
        let resized = (percent * new.max(0.0) / old * 10.0).round() / 10.0;
        return Some(format!("{resized}%"));
    }
    let is_pixels = value
        .strip_suffix("px")
        .and_then(|pixels| pixels.trim().parse::<f32>().ok())
        .is_some_and(f32::is_finite);
    is_pixels.then(|| format!("{}px", new.max(0.0).round()))
}

fn ui_property(instance: &Instance, name: &str) -> UDim2 {
    instance
        .property(name)
        .and_then(Value::as_udim2)
        .expect("every UI element has UDim2 position and size properties")
}

fn color_property(instance: &Instance, name: &str) -> Color3 {
    instance
        .property(name)
        .and_then(Value::as_color3)
        .map(|color| {
            Color3::new(
                finite(color.r).clamp(0.0, 1.0),
                finite(color.g).clamp(0.0, 1.0),
                finite(color.b).clamp(0.0, 1.0),
            )
        })
        .expect("every UI element has the requested color property")
}

fn finite(value: f32) -> f32 {
    if value.is_finite() { value } else { 0.0 }
}

#[cfg(test)]
mod accessibility_tests;
#[cfg(test)]
mod focus_tests;
#[cfg(test)]
mod layering_tests;
#[cfg(test)]
mod shape_tests;
#[cfg(test)]
mod strings_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod widget_tests;
