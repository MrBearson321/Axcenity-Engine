//! The canvas every graph view draws on (unified Phase 91): the node
//! graphs here (UI Event Graph, behavior trees, dialogues, material
//! graphs), a block script's Graph view and the Animation tab's state
//! machines. Each view keeps its own nodes and rules; the canvas gives them
//! the same background, pan and zoom, box selection, framing, minimap,
//! search menu, notices and keys.
//!
//! - Right- or middle-drag and the wheel pan; Ctrl + wheel (or pinching)
//!   zooms round the pointer.
//! - Left-drag on the background draws a selection box ([`BoxSelect`]).
//! - F frames the selection, or everything ([`Canvas::fit`]); Space opens
//!   the search menu at the pointer ([`SearchMenu`]); Delete, Ctrl+D,
//!   Ctrl+A, Escape and Ctrl+C, X and V are read the same way ([`keys`]).
//! - The minimap shows every node and the part in view once some are off
//!   screen; a click or drag on it moves the view there.

use std::ops::RangeInclusive;

use axcenity_editor_kit::theme::{self, icons};
use axcenity_editor_tools::shortcuts::{Action, pressed};
use eframe::egui::{self, Color32, FontId, Pos2, Rect, Sense, Stroke, Vec2, pos2, vec2};

/// The minimap's size on screen, and its gap from the canvas's corner.
const MINIMAP: Vec2 = Vec2::new(160.0, 110.0);
const MINIMAP_GAP: f32 = 10.0;

/// Where a graph view is looking, and what the canvas is doing for it.
/// Kept by the view between frames.
#[derive(Clone, Debug)]
pub struct Canvas {
    /// Where the graph's origin is, from the canvas's top left corner.
    pub pan: Vec2,
    pub zoom: f32,
    /// Where a box selection started, on screen.
    boxing: Option<Pos2>,
    /// A message at the bottom of the canvas, and until when.
    notice: Option<(String, f64)>,
}

impl Canvas {
    /// A canvas at zoom 1 with the graph's origin at `pan`.
    pub fn new(pan: Vec2) -> Self {
        Self {
            pan,
            zoom: 1.0,
            boxing: None,
            notice: None,
        }
    }

    /// Draws the background and grid in `rect` and pans and zooms (within
    /// `zooms`) from the mouse, unless `input` is false (a menu is open).
    pub fn begin(
        &mut self,
        ui: &mut egui::Ui,
        rect: Rect,
        id: egui::Id,
        zooms: RangeInclusive<f32>,
        input: bool,
    ) -> Frame {
        let colors = theme::canvas(ui.ctx());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0, colors.background);
        let back = ui.interact(rect, id, Sense::click_and_drag());
        if back.dragged_by(egui::PointerButton::Secondary)
            || back.dragged_by(egui::PointerButton::Middle)
        {
            self.pan += back.drag_delta();
        }
        let hovered = ui.rect_contains_pointer(rect);
        if hovered && input {
            let (scroll, zoom, pointer) =
                ui.input(|i| (i.smooth_scroll_delta, i.zoom_delta(), i.pointer.hover_pos()));
            self.pan += scroll;
            if zoom != 1.0
                && let Some(pointer) = pointer
            {
                let old = self.zoom;
                self.zoom = (old * zoom).clamp(*zooms.start(), *zooms.end());
                let from_origin = pointer - rect.min - self.pan;
                self.pan = pointer - rect.min - from_origin * (self.zoom / old);
            }
        }

        // A grid, which moves with the nodes.
        let spacing = 32.0 * self.zoom;
        let line = Stroke::new(1.0_f32, colors.grid);
        let mut x = rect.left() + self.pan.x.rem_euclid(spacing);
        while x < rect.right() {
            painter.vline(x, rect.y_range(), line);
            x += spacing;
        }
        let mut y = rect.top() + self.pan.y.rem_euclid(spacing);
        while y < rect.bottom() {
            painter.hline(rect.x_range(), y, line);
            y += spacing;
        }
        Frame {
            rect,
            back,
            hovered,
            pointer: ui.input(|i| i.pointer.interact_pos()),
            zoom: self.zoom,
            origin: rect.min + self.pan,
        }
    }

    /// A screen point in a canvas drawn in `rect`, as a graph point.
    pub fn to_graph(&self, rect: Rect, at: Pos2) -> Pos2 {
        let origin = rect.min + self.pan;
        pos2((at.x - origin.x) / self.zoom, (at.y - origin.y) / self.zoom)
    }

    /// A graph point as a screen point in a canvas drawn in `rect`.
    pub fn to_screen(&self, rect: Rect, at: Pos2) -> Pos2 {
        rect.min + self.pan + at.to_vec2() * self.zoom
    }

    /// Fits `area` (in graph units) into `rect`, with a margin, zoomed no
    /// further in than `max_zoom` nor out than `min_zoom`. Nothing to fit
    /// puts the origin near the top left at zoom 1.
    pub fn fit(&mut self, rect: Rect, area: Option<Rect>, min_zoom: f32, max_zoom: f32) {
        let Some(area) = area else {
            self.pan = vec2(40.0, 40.0);
            self.zoom = 1.0;
            return;
        };
        let room = (rect.size() - Vec2::splat(80.0)).max(vec2(1.0, 1.0));
        self.zoom = (room.x / area.width().max(1.0))
            .min(room.y / area.height().max(1.0))
            .clamp(min_zoom, max_zoom);
        self.pan = rect.size() * 0.5 - area.center().to_vec2() * self.zoom;
    }

    /// The selection box: started by a left-drag on the background (when
    /// `enabled`), drawn while dragged, and its area once let go.
    pub fn box_select(&mut self, ui: &egui::Ui, frame: &Frame, enabled: bool) -> BoxSelect {
        let mut result = BoxSelect::None;
        if enabled && frame.back.drag_started_by(egui::PointerButton::Primary) {
            self.boxing = frame.back.interact_pointer_pos();
            let add = ui.input(|i| i.modifiers.shift || i.modifiers.command);
            result = BoxSelect::Started { add };
        }
        if let (Some(start), Some(pointer)) = (self.boxing, frame.pointer) {
            let area = Rect::from_two_pos(start, pointer);
            let accent = ui.visuals().selection.bg_fill;
            ui.painter_at(frame.rect).rect(
                area,
                2,
                accent.gamma_multiply(0.15),
                Stroke::new(1.0_f32, accent),
                egui::StrokeKind::Inside,
            );
            if frame.back.drag_stopped() {
                self.boxing = None;
                result = BoxSelect::Done(area);
            }
        }
        if !ui.input(|i| i.pointer.primary_down()) {
            self.boxing = None;
        }
        result
    }

    /// Shows `text` at the bottom of the canvas for four seconds.
    pub fn say(&mut self, ctx: &egui::Context, text: impl Into<String>) {
        let now = ctx.input(|i| i.time);
        self.notice = Some((text.into(), now + 4.0));
    }

    /// Draws the notice, if there's one.
    pub fn show_notice(&mut self, ui: &egui::Ui, frame: &Frame) {
        let now = ui.input(|i| i.time);
        let Some((text, until)) = &self.notice else {
            return;
        };
        if now >= *until {
            self.notice = None;
            return;
        }
        let painter = ui.painter_at(frame.rect);
        let at = pos2(frame.rect.center().x, frame.rect.bottom() - 24.0);
        let galley =
            painter.layout_no_wrap(text.clone(), FontId::proportional(13.0), Color32::WHITE);
        let back = Rect::from_center_size(at, galley.size() + vec2(20.0, 10.0));
        painter.rect_filled(back, 6, Color32::from_rgba_unmultiplied(160, 40, 40, 230));
        painter.galley(back.min + vec2(10.0, 5.0), galley, Color32::WHITE);
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(250));
    }

    /// The notice showing, if any (for tests).
    pub fn notice(&self) -> Option<&str> {
        self.notice.as_ref().map(|(text, _)| text.as_str())
    }

    /// The minimap, in the canvas's bottom right corner, once any of
    /// `nodes` (in graph units) is out of view. A click or drag on it
    /// centres the view there. Returns whether it has the pointer.
    pub fn minimap(&mut self, ui: &mut egui::Ui, frame: &Frame, nodes: &[Rect]) -> bool {
        let Some(area) = nodes.iter().copied().reduce(|a, b| a.union(b)) else {
            return false;
        };
        let view = Rect::from_min_max(
            self.to_graph(frame.rect, frame.rect.min),
            self.to_graph(frame.rect, frame.rect.max),
        );
        let small = frame.rect.width() < MINIMAP.x * 2.5 || frame.rect.height() < MINIMAP.y * 2.5;
        if small || nodes.iter().all(|node| view.contains_rect(*node)) {
            return false;
        }
        let map = Rect::from_min_size(frame.rect.max - MINIMAP - Vec2::splat(MINIMAP_GAP), MINIMAP);
        let shown = area.union(view);
        let scale = ((MINIMAP.x - 8.0) / shown.width().max(1.0))
            .min((MINIMAP.y - 8.0) / shown.height().max(1.0));
        let offset = map.center() - shown.center().to_vec2() * scale;
        let to_map = |at: Pos2| offset + at.to_vec2() * scale;
        let colors = theme::canvas(ui.ctx());
        let painter = ui.painter_at(frame.rect);
        painter.rect(
            map,
            4,
            colors.background.gamma_multiply(0.92),
            Stroke::new(1.0_f32, colors.node_edge),
            egui::StrokeKind::Inside,
        );
        for node in nodes {
            painter.rect_filled(
                Rect::from_min_max(to_map(node.min), to_map(node.max)),
                1,
                colors.node_text_weak,
            );
        }
        painter.rect_stroke(
            Rect::from_min_max(to_map(view.min), to_map(view.max)).intersect(map),
            1,
            Stroke::new(1.5_f32, ui.visuals().selection.bg_fill),
            egui::StrokeKind::Inside,
        );
        let response = ui
            .interact(map, frame.back.id.with("minimap"), Sense::click_and_drag())
            .on_hover_text("Click or drag to look there");
        if (response.clicked() || response.dragged())
            && let Some(at) = response.interact_pointer_pos()
        {
            let target = ((at - offset) / scale).to_pos2();
            self.pan = frame.rect.size() * 0.5 - target.to_vec2() * self.zoom;
        }
        response.hovered() || response.dragged()
    }
}

/// One frame of a canvas: where it is, its background's response, and how
/// graph points map to the screen.
pub struct Frame {
    pub rect: Rect,
    /// The background, under the nodes.
    pub back: egui::Response,
    /// Whether the pointer is over the canvas.
    pub hovered: bool,
    pub pointer: Option<Pos2>,
    pub zoom: f32,
    origin: Pos2,
}

impl Frame {
    pub fn to_screen(&self, at: Pos2) -> Pos2 {
        self.origin + at.to_vec2() * self.zoom
    }

    pub fn to_graph(&self, at: Pos2) -> Pos2 {
        pos2(
            (at.x - self.origin.x) / self.zoom,
            (at.y - self.origin.y) / self.zoom,
        )
    }

    /// The pointer as a graph point, or the canvas's middle without one.
    pub fn pointer_in_graph(&self) -> Pos2 {
        self.to_graph(
            self.pointer
                .filter(|at| self.rect.contains(*at))
                .unwrap_or(self.rect.center()),
        )
    }
}

/// What the selection box did this frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BoxSelect {
    None,
    /// A box began: the selection is cleared first unless `add` (Shift or
    /// Ctrl was held).
    Started {
        add: bool,
    },
    /// The box was let go over this area, on screen.
    Done(Rect),
}

/// A hint in the middle of an empty canvas.
pub fn empty_hint(ui: &egui::Ui, rect: Rect, text: &str) {
    ui.painter_at(rect).text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        text,
        FontId::proportional(14.0),
        ui.visuals().weak_text_color(),
    );
}

/// The keys every graph view reads, the same way, while the pointer is
/// over it and nothing is being typed.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Keys {
    pub delete: bool,
    pub duplicate: bool,
    pub select_all: bool,
    pub escape: bool,
    /// F: frame the selection, or everything.
    pub frame: bool,
    /// Space: open the search menu at the pointer.
    pub search: bool,
    pub copy: bool,
    pub cut: bool,
    /// Text pasted (Ctrl+V).
    pub paste: Option<String>,
}

/// The graph keys pressed this frame, or none while a text box has the
/// keyboard.
pub fn keys(ctx: &egui::Context) -> Keys {
    if ctx.egui_wants_keyboard_input() || ctx.memory(|memory| memory.focused().is_some()) {
        return Keys::default();
    }
    ctx.input(|i| {
        let mut keys = Keys {
            delete: pressed(i, Action::DeleteNodes),
            duplicate: pressed(i, Action::DuplicateNodes),
            select_all: pressed(i, Action::SelectAllNodes),
            escape: pressed(i, Action::ClearNodeSelection),
            frame: pressed(i, Action::FrameNodes),
            search: pressed(i, Action::SearchNodes),
            ..Keys::default()
        };
        for event in &i.events {
            match event {
                egui::Event::Copy => keys.copy = true,
                egui::Event::Cut => keys.cut = true,
                egui::Event::Paste(text) => keys.paste = Some(text.clone()),
                _ => {}
            }
        }
        keys
    })
}

/// The search menu every graph view opens to add a node (Space, a
/// right-click, or a wire let go on the background): its entries by
/// group, with a search box.
#[derive(Clone, Debug)]
pub struct SearchMenu {
    /// Where it opened, on screen and in the graph (where the node goes).
    pub screen: Pos2,
    pub graph: Pos2,
    search: String,
    /// It opened this frame: give the search box focus.
    fresh: bool,
}

/// One thing the search menu offers.
pub struct MenuEntry<'a> {
    /// The group it's listed under, and the group's colour.
    pub group: &'a str,
    pub color: Color32,
    pub title: &'a str,
    /// What the search box finds it by, lower case (its title at least).
    pub words: &'a str,
}

/// What the search menu did this frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuChoice {
    Open,
    Close,
    /// The entry at this index was picked.
    Pick(usize),
}

/// Draws an entry's help (by its index) in the menu's hover tooltip.
pub type EntryHelp<'a> = &'a dyn Fn(&mut egui::Ui, usize);

impl SearchMenu {
    pub fn new(screen: Pos2, graph: Pos2) -> Self {
        Self {
            screen,
            graph,
            search: String::new(),
            fresh: true,
        }
    }

    /// Draws the menu titled `title`. `hover`, if given, shows an entry's
    /// help.
    /// Escape or a click outside closes it; Enter picks the first match.
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        id: egui::Id,
        title: &str,
        entries: &[MenuEntry],
        hover: Option<EntryHelp>,
    ) -> MenuChoice {
        let mut choice = MenuChoice::Open;
        let matches: Vec<usize> = entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| matches_search(entry.words, &self.search))
            .map(|(index, _)| index)
            .collect();
        let area = egui::Area::new(id)
            .order(egui::Order::Foreground)
            .fixed_pos(self.screen)
            .constrain(true)
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_width(250.0);
                    ui.label(egui::RichText::new(title).strong());
                    let search_box = ui.add(
                        egui::TextEdit::singleline(&mut self.search)
                            .hint_text(theme::with_icon(icons::MAGNIFYING_GLASS, "Search"))
                            .desired_width(f32::INFINITY),
                    );
                    if self.fresh {
                        search_box.request_focus();
                    }
                    if search_box.lost_focus()
                        && ui.input(|i| i.key_pressed(egui::Key::Enter))
                        && let Some(first) = matches.first()
                    {
                        choice = MenuChoice::Pick(*first);
                    }
                    ui.separator();
                    egui::ScrollArea::vertical()
                        .max_height(340.0)
                        .show(ui, |ui| {
                            if matches.is_empty() {
                                ui.weak("Nothing matches.");
                            }
                            let mut shown = None;
                            for &index in &matches {
                                let entry = &entries[index];
                                if shown != Some(entry.group) && !entry.group.is_empty() {
                                    ui.label(
                                        egui::RichText::new(entry.group)
                                            .color(entry.color)
                                            .strong()
                                            .size(theme::type_scale::SMALL),
                                    );
                                }
                                shown = Some(entry.group);
                                let mut item = ui.selectable_label(false, entry.title);
                                if let Some(hover) = hover {
                                    item = item.on_hover_ui(|ui| hover(ui, index));
                                }
                                if item.clicked() {
                                    choice = MenuChoice::Pick(index);
                                }
                            }
                        });
                });
            });
        let fresh = std::mem::replace(&mut self.fresh, false);
        let (escape, pressed, pointer) = ctx.input(|i| {
            (
                i.key_pressed(egui::Key::Escape),
                i.pointer.any_pressed(),
                i.pointer.interact_pos(),
            )
        });
        let outside =
            pressed && pointer.is_some_and(|pointer| !area.response.rect.contains(pointer));
        if choice == MenuChoice::Open && (escape || (outside && !fresh)) {
            choice = MenuChoice::Close;
        }
        choice
    }
}

/// Whether every word of `search` is in `words` (lower case).
pub fn matches_search(words: &str, search: &str) -> bool {
    search
        .split_whitespace()
        .all(|word| words.contains(&word.to_lowercase()))
}

/// A pin's radius at zoom 1.
pub const PIN_RADIUS: f32 = 5.0;

/// A wire from an output at `start` to an input at `end`, as a curve that
/// leaves and arrives level.
pub fn draw_wire(painter: &egui::Painter, start: Pos2, end: Pos2, color: Color32, width: f32) {
    let reach = ((end.x - start.x).abs() * 0.5).max(40.0);
    let bezier = egui::epaint::CubicBezierShape::from_points_stroke(
        [start, start + vec2(reach, 0.0), end - vec2(reach, 0.0), end],
        false,
        Color32::TRANSPARENT,
        Stroke::new(width, color),
    );
    painter.add(bezier);
}

/// An exec pin: Unreal's white arrow, filled when it's wired.
pub fn draw_exec_pin(
    painter: &egui::Painter,
    at: Pos2,
    z: f32,
    connected: bool,
    colors: &theme::Canvas,
) {
    let s = |x: f32, y: f32| at + vec2(x * z, y * z);
    let points = vec![
        s(-5.0, -6.0),
        s(1.0, -6.0),
        s(6.0, 0.0),
        s(1.0, 6.0),
        s(-5.0, 6.0),
    ];
    let fill = if connected {
        colors.wire
    } else {
        Color32::TRANSPARENT
    };
    painter.add(egui::Shape::convex_polygon(
        points,
        fill,
        Stroke::new(1.5 * z.max(0.7), colors.wire),
    ));
}

/// A value pin: a circle in its type's colour, filled when it's wired.
pub fn draw_value_pin(
    painter: &egui::Painter,
    at: Pos2,
    z: f32,
    color: Color32,
    connected: bool,
    colors: &theme::Canvas,
) {
    let radius = PIN_RADIUS * z;
    if connected {
        painter.circle_filled(at, radius, color);
    } else {
        painter.circle(
            at,
            radius,
            colors.node_body,
            Stroke::new(1.5 * z.max(0.7), color),
        );
    }
}

/// What the pointer did to a pin.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PinAction {
    None,
    /// A wire is being dragged out of it.
    WireFrom,
    /// Alt-click, or Break links in its menu: its wires go.
    Break,
}

/// A pin's pointer handling, the same in every graph: drag to wire,
/// Alt-click or right-click > Break links to break its wires. A ring shows
/// on hover while no wire is held.
pub fn pin_interact(
    ui: &mut egui::Ui,
    painter: &egui::Painter,
    id: egui::Id,
    at: Pos2,
    z: f32,
    wire_held: bool,
) -> PinAction {
    let reach = (PIN_RADIUS + 5.0) * z;
    let rect = Rect::from_center_size(at, Vec2::splat(reach * 2.0));
    let response = ui
        .interact(rect, id, Sense::click_and_drag())
        .on_hover_cursor(egui::CursorIcon::Crosshair);
    if response.hovered() && !wire_held {
        let ring = theme::canvas(ui.ctx()).node_text.gamma_multiply(0.35);
        painter.circle_stroke(at, reach, Stroke::new(1.0_f32, ring));
    }
    let mut action = PinAction::None;
    if response.drag_started_by(egui::PointerButton::Primary) {
        action = PinAction::WireFrom;
    }
    if response.clicked() && ui.input(|i| i.modifiers.alt) {
        action = PinAction::Break;
    }
    response.context_menu(|ui| {
        if ui
            .button(theme::with_icon(icons::LINK_BREAK, "Break links"))
            .clicked()
        {
            action = PinAction::Break;
            ui.close();
        }
    });
    action
}

/// Zoom out, the zoom as a percentage (click for 100%), zoom in and Frame
/// all, right to left in a toolbar. Returns true when Frame all was clicked.
pub fn zoom_buttons(ui: &mut egui::Ui, canvas: &mut Canvas, zooms: RangeInclusive<f32>) -> bool {
    let frame = ui
        .button(icons::CORNERS_OUT)
        .on_hover_text("Frame all: fit every node into view (F frames the selection)")
        .clicked();
    if ui
        .button(icons::MAGNIFYING_GLASS_PLUS)
        .on_hover_text("Zoom in")
        .clicked()
    {
        canvas.zoom = (canvas.zoom * 1.2).min(*zooms.end());
    }
    if ui
        .button(format!("{:.0}%", canvas.zoom * 100.0))
        .on_hover_text("Back to 100%")
        .clicked()
    {
        canvas.zoom = 1.0;
    }
    if ui
        .button(icons::MAGNIFYING_GLASS_MINUS)
        .on_hover_text("Zoom out")
        .clicked()
    {
        canvas.zoom = (canvas.zoom / 1.2).max(*zooms.start());
    }
    frame
}
