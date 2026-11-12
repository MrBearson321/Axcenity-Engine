//! The editor's own title bar, like VS Code's: the OS title bar is turned off
//! and the header holds the menus, the window title and the window buttons.
//!
//! On Windows and Linux the OS frame is removed entirely, so this draws the
//! minimize, maximize and close buttons, and resize grips along the edges.
//! On macOS the frame stays and the content runs under a transparent title
//! bar, so the system's traffic lights sit at the header's left, and the
//! menus are in the system menu bar (`app/native_menu.rs`), so the header
//! keeps only the title and the status chips. In full screen the traffic
//! lights go and the system shows the menu bar and title bar when the
//! pointer reaches the top of the screen, as for any Mac app.

use eframe::egui::{
    self, Color32, CursorIcon, Rect, ResizeDirection, Sense, Stroke, StrokeKind, ViewportCommand,
    pos2, vec2,
};

use crate::theme;

/// The header's height, in points.
pub const HEIGHT: f32 = 34.0;

/// Whether this OS keeps its own window buttons (macOS's traffic lights).
const NATIVE_BUTTONS: bool = cfg!(target_os = "macos");

/// Whether the menus are in the system menu bar rather than the header.
/// Tests draw the header's menus on every OS, so they can click them.
pub const NATIVE_MENU: bool = cfg!(all(target_os = "macos", not(test)));

/// Room the traffic lights take at the header's left.
const TRAFFIC_LIGHTS: f32 = 76.0;

/// Room to leave free at the header's left: the traffic lights' on macOS,
/// except in full screen, where macOS hides them.
pub fn left_inset(ctx: &egui::Context) -> f32 {
    if NATIVE_BUTTONS && !fullscreen(ctx) {
        TRAFFIC_LIGHTS
    } else {
        0.0
    }
}

const BUTTON_WIDTH: f32 = 46.0;

/// A panel window's strip, in points. macOS needs room for its traffic lights.
const PANEL_STRIP_HEIGHT: f32 = 28.0;
const PANEL_BUTTON_WIDTH: f32 = 30.0;

/// How far in from the window's edge a drag resizes it, in points.
const GRIP: f32 = 5.0;
/// Corners grip a little further along each edge.
const CORNER: f32 = 14.0;

/// Turns off the OS title bar (see the module docs).
pub fn viewport(builder: egui::ViewportBuilder) -> egui::ViewportBuilder {
    if NATIVE_BUTTONS {
        builder
            .with_fullsize_content_view(true)
            .with_titlebar_shown(false)
            .with_title_shown(false)
    } else {
        builder.with_decorations(false)
    }
}

fn maximized(ctx: &egui::Context) -> bool {
    ctx.input(|input| input.viewport().maximized.unwrap_or(false))
}

fn fullscreen(ctx: &egui::Context) -> bool {
    ctx.input(|input| input.viewport().fullscreen.unwrap_or(false))
}

/// Makes `rect` move the window when dragged, and maximize or restore it when
/// double-clicked (on macOS, what System Settings says a double-click on a
/// title bar does). Call it before adding the header's widgets: egui gives
/// the pointer to the widget added last, so they stay clickable on top of it.
pub fn drag_region(ui: &mut egui::Ui, rect: Rect) {
    let response = ui.interact(rect, ui.id().with("drag window"), Sense::click_and_drag());
    if response.double_clicked() {
        match double_click() {
            DoubleClick::Zoom => {
                let maximized = maximized(ui.ctx());
                ui.ctx()
                    .send_viewport_cmd(ViewportCommand::Maximized(!maximized));
            }
            DoubleClick::Minimize => ui.ctx().send_viewport_cmd(ViewportCommand::Minimized(true)),
            DoubleClick::Nothing => {}
        }
    } else if response.drag_started_by(egui::PointerButton::Primary) {
        ui.ctx().send_viewport_cmd(ViewportCommand::StartDrag);
    }
}

/// What a double-click on the title bar does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
enum DoubleClick {
    /// Maximize or restore (on macOS, zoom or fill).
    Zoom,
    Minimize,
    Nothing,
}

/// On macOS, System Settings > Desktop & Dock > "Double-click a window's
/// title bar to": Fill or Zoom (the default), Minimize, or Do Nothing.
#[cfg(target_os = "macos")]
fn double_click() -> DoubleClick {
    use objc2_foundation::{NSUserDefaults, ns_string};
    let defaults = NSUserDefaults::standardUserDefaults();
    let action = defaults
        .stringForKey(ns_string!("AppleActionOnDoubleClick"))
        .map(|action| action.to_string());
    match action.as_deref() {
        Some("Minimize") => DoubleClick::Minimize,
        Some("None") => DoubleClick::Nothing,
        _ => DoubleClick::Zoom,
    }
}

#[cfg(not(target_os = "macos"))]
fn double_click() -> DoubleClick {
    DoubleClick::Zoom
}

/// What a panel window's buttons asked for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PanelAction {
    /// Put its panels back where they were docked.
    DockBack,
    Close,
}

/// How tall the row holding a panel window's buttons is.
const PANEL_ROW_HEIGHT: f32 = if NATIVE_BUTTONS {
    PANEL_STRIP_HEIGHT
} else {
    crate::layout::TAB_BAR_HEIGHT
};

/// Lets a panel window be moved by its tab row (empty space there: its tabs
/// stay on top and drag as tabs). On macOS, which keeps its traffic lights,
/// it's a slim strip above the tabs instead. Call before the panels are drawn.
pub fn panel_title_area(ui: &mut egui::Ui) {
    let ctx = &ui.ctx().clone();
    if NATIVE_BUTTONS {
        let palette = theme::palette(ctx);
        egui::Panel::top("panel window strip")
            .exact_size(PANEL_STRIP_HEIGHT)
            .frame(egui::Frame::new().fill(palette.app_bg))
            .show_separator_line(false)
            .show(ui, |ui| {
                let bar = ui.max_rect();
                drag_region(ui, bar);
            });
        return;
    }
    let screen = ctx.content_rect();
    let row = Rect::from_min_size(screen.min, vec2(screen.width(), PANEL_ROW_HEIGHT));
    let mut ui = egui::Ui::new(
        ctx.clone(),
        egui::Id::new("panel window title"),
        egui::UiBuilder::new()
            .layer_id(egui::LayerId::background())
            .max_rect(row),
    );
    drag_region(&mut ui, row);
}

/// A panel window's buttons, at the right end of its tab row: dock back,
/// then close (macOS keeps its own close). Call after the panels are drawn
/// and after [`edges`], so they stay clickable in the window's corner.
pub fn panel_buttons(ctx: &egui::Context) -> Option<PanelAction> {
    let screen = ctx.content_rect();
    let cell = |from_right: f32| {
        Rect::from_min_size(
            pos2(
                screen.right() - PANEL_BUTTON_WIDTH * from_right,
                screen.top(),
            ),
            vec2(PANEL_BUTTON_WIDTH, PANEL_ROW_HEIGHT),
        )
    };
    let (dock, close) = if NATIVE_BUTTONS {
        (cell(1.0), None)
    } else {
        (cell(2.0), Some(cell(1.0)))
    };
    let id = egui::Id::new("panel window buttons");
    let layer = egui::LayerId::new(egui::Order::Foreground, id);
    let mut ui = egui::Ui::new(
        ctx.clone(),
        id,
        egui::UiBuilder::new().layer_id(layer).max_rect(screen),
    );
    // Over tabs that reach this far, so the buttons always read clearly.
    let backing = close.map_or(dock, |close| dock.union(close));
    ui.painter()
        .rect_filled(backing, 0, theme::palette(ctx).app_bg);
    let mut action = None;
    if button(&mut ui, dock, Glyph::DockBack, "Dock Back").clicked() {
        action = Some(PanelAction::DockBack);
    }
    if let Some(close) = close
        && button(&mut ui, close, Glyph::Close, "Close").clicked()
    {
        action = Some(PanelAction::Close);
    }
    action
}

/// Width the window buttons take at the header's right.
pub fn buttons_width() -> f32 {
    if NATIVE_BUTTONS {
        0.0
    } else {
        BUTTON_WIDTH * 3.0
    }
}

/// Draws minimize, maximize (or restore) and close at the right of `bar`,
/// each the bar's full height. Returns true when close was clicked; the
/// caller decides whether the window may close.
pub fn buttons(ui: &mut egui::Ui, bar: Rect) -> bool {
    if NATIVE_BUTTONS {
        return false;
    }
    let ctx = ui.ctx().clone();
    let maximized = maximized(&ctx);
    let mut x = bar.right() - buttons_width();
    let mut next = || {
        let rect = Rect::from_min_size(pos2(x, bar.top()), vec2(BUTTON_WIDTH, bar.height()));
        x += BUTTON_WIDTH;
        rect
    };
    let (minimize, toggle, close) = (next(), next(), next());

    if button(ui, minimize, Glyph::Minimize, "Minimize").clicked() {
        ctx.send_viewport_cmd(ViewportCommand::Minimized(true));
    }
    let (glyph, tip) = if maximized {
        (Glyph::Restore, "Restore Down")
    } else {
        (Glyph::Maximize, "Maximize")
    };
    if button(ui, toggle, glyph, tip).clicked() {
        ctx.send_viewport_cmd(ViewportCommand::Maximized(!maximized));
    }
    button(ui, close, Glyph::Close, "Close").clicked()
}

#[derive(Clone, Copy, PartialEq)]
enum Glyph {
    Minimize,
    Maximize,
    Restore,
    Close,
    DockBack,
}

/// One window button: a flat cell that fills on hover, with a thin
/// line-drawn glyph like the OS's own.
fn button(ui: &mut egui::Ui, rect: Rect, glyph: Glyph, tooltip: &str) -> egui::Response {
    let response = ui.interact(rect, ui.id().with(tooltip), Sense::click());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, tooltip));
    let palette = theme::palette(ui.ctx());
    let active = response.hovered() || response.is_pointer_button_down_on();
    // Close turns the OS's own red, in both themes, as on Windows and in VS Code.
    let (fill, ink) = match (glyph, active) {
        (Glyph::Close, true) => (Color32::from_rgb(0xC4, 0x2B, 0x1C), Color32::WHITE),
        (_, true) if response.is_pointer_button_down_on() => {
            (palette.control_active, palette.text_strong)
        }
        (_, true) => (palette.control_hover, palette.text_strong),
        (_, false) => (Color32::TRANSPARENT, palette.text_secondary),
    };
    let painter = ui.painter();
    painter.rect_filled(rect, 0, fill);
    let stroke = Stroke::new(1.0_f32, ink);
    let c = rect.center();
    let s = 5.0;
    match glyph {
        Glyph::Minimize => {
            painter.hline((c.x - s)..=(c.x + s), c.y, stroke);
        }
        Glyph::Maximize => {
            painter.rect_stroke(
                Rect::from_center_size(c, vec2(2.0 * s, 2.0 * s)),
                1,
                stroke,
                StrokeKind::Inside,
            );
        }
        Glyph::Restore => {
            // A square with another peeking out behind its top-right corner.
            let front = Rect::from_min_size(pos2(c.x - s, c.y - s + 2.0), vec2(8.0, 8.0));
            painter.rect_stroke(front, 1, stroke, StrokeKind::Inside);
            painter.line(
                vec![
                    pos2(c.x - s + 2.0, c.y - s + 2.0),
                    pos2(c.x - s + 2.0, c.y - s),
                    pos2(c.x + s, c.y - s),
                    pos2(c.x + s, c.y + s - 2.0),
                    pos2(c.x + s - 2.0, c.y + s - 2.0),
                ],
                stroke,
            );
        }
        Glyph::Close => {
            painter.line_segment([pos2(c.x - s, c.y - s), pos2(c.x + s, c.y + s)], stroke);
            painter.line_segment([pos2(c.x + s, c.y - s), pos2(c.x - s, c.y + s)], stroke);
        }
        Glyph::DockBack => {
            painter.text(
                c,
                egui::Align2::CENTER_CENTER,
                theme::icons::ARROW_SQUARE_IN,
                egui::FontId::proportional(14.0),
                ink,
            );
        }
    }
    response.on_hover_text(tooltip)
}

/// Resize grips along the window's edges and corners, and a 1-point border
/// around the window, since the OS frame that had both is off. Nothing on
/// macOS, or while the window is maximized or fullscreen.
pub fn edges(ctx: &egui::Context) {
    if NATIVE_BUTTONS || maximized(ctx) || fullscreen(ctx) {
        return;
    }
    let screen = ctx.content_rect();
    let id = egui::Id::new("window edges");
    // Above the panels, so the grips win where they overlap them.
    let layer = egui::LayerId::new(egui::Order::Foreground, id);
    let ui = egui::Ui::new(
        ctx.clone(),
        id,
        egui::UiBuilder::new().layer_id(layer).max_rect(screen),
    );
    let palette = theme::palette(ctx);
    ui.painter().rect_stroke(
        screen,
        0,
        Stroke::new(ctx.pixels_per_point().max(1.0).recip(), palette.border),
        StrokeKind::Inside,
    );

    let (l, r, t, b) = (screen.left(), screen.right(), screen.top(), screen.bottom());
    // Edges first, then corners, so a corner wins where they meet.
    let grips = [
        (
            ResizeDirection::North,
            Rect::from_x_y_ranges(l..=r, t..=t + GRIP),
        ),
        (
            ResizeDirection::South,
            Rect::from_x_y_ranges(l..=r, b - GRIP..=b),
        ),
        (
            ResizeDirection::West,
            Rect::from_x_y_ranges(l..=l + GRIP, t..=b),
        ),
        (
            ResizeDirection::East,
            Rect::from_x_y_ranges(r - GRIP..=r, t..=b),
        ),
        (
            ResizeDirection::NorthWest,
            Rect::from_min_size(pos2(l, t), vec2(CORNER, CORNER)),
        ),
        (
            ResizeDirection::NorthEast,
            Rect::from_min_size(pos2(r - CORNER, t), vec2(CORNER, CORNER)),
        ),
        (
            ResizeDirection::SouthWest,
            Rect::from_min_size(pos2(l, b - CORNER), vec2(CORNER, CORNER)),
        ),
        (
            ResizeDirection::SouthEast,
            Rect::from_min_size(pos2(r - CORNER, b - CORNER), vec2(CORNER, CORNER)),
        ),
    ];
    // Read once, outside any `input` closure that touches the responses.
    let pressed = ctx.input(|input| input.pointer.primary_pressed());
    for (direction, rect) in grips {
        let response = ui
            .interact(rect, id.with(format!("{direction:?}")), Sense::drag())
            .on_hover_cursor(cursor(direction));
        if pressed && response.is_pointer_button_down_on() {
            ctx.send_viewport_cmd(ViewportCommand::BeginResize(direction));
        }
    }
}

fn cursor(direction: ResizeDirection) -> CursorIcon {
    match direction {
        ResizeDirection::North => CursorIcon::ResizeNorth,
        ResizeDirection::South => CursorIcon::ResizeSouth,
        ResizeDirection::East => CursorIcon::ResizeEast,
        ResizeDirection::West => CursorIcon::ResizeWest,
        ResizeDirection::NorthEast => CursorIcon::ResizeNorthEast,
        ResizeDirection::SouthEast => CursorIcon::ResizeSouthEast,
        ResizeDirection::NorthWest => CursorIcon::ResizeNorthWest,
        ResizeDirection::SouthWest => CursorIcon::ResizeSouthWest,
    }
}
