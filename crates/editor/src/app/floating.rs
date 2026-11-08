//! Drawing panels in their own OS windows (`layout/floating.rs`): each is an
//! immediate egui viewport with a dock area of its own. The panels draw from
//! the app's state as they do when docked, so nothing is lost by moving one.

use eframe::egui;
use eframe::egui_wgpu::RenderState;
use egui_dock::DockState;

use super::{DockViewer, EditorApp};
use crate::layout::floating::{self, DEFAULT_SIZE, Placement};
use crate::layout::{self, Layout, Pane};
use crate::theme;
use crate::viewport::ViewportOutput;
use crate::window_frame;

/// The main window's dock area.
pub(super) fn main_area(generation: u64) -> egui::Id {
    egui::Id::new(("main dock area", generation))
}

fn area_of(id: u64, generation: u64) -> egui::Id {
    egui::Id::new(("floating dock area", id, generation))
}

fn viewport_of(id: u64) -> egui::ViewportId {
    egui::ViewportId::from_hash_of(("floating panel window", id))
}

/// Turns points in the viewport `ctx` is drawing into physical pixels on
/// the screen, once the OS has said where that viewport is.
fn screen_pixels(ctx: &egui::Context) -> Option<impl Fn(egui::Pos2) -> egui::Pos2 + use<>> {
    let origin = ctx.input(|input| input.viewport().inner_rect)?.min;
    let scale = ctx.pixels_per_point();
    Some(move |at: egui::Pos2| ((origin.to_vec2() + at.to_vec2()) * scale).to_pos2())
}

/// What the mouse did in a viewport this frame.
struct MouseReport {
    /// Where it is on the screen, in physical pixels.
    screen: Option<egui::Pos2>,
    held: bool,
    /// The tab being dragged, if the mouse has gone outside the viewport.
    torn: Option<Pane>,
}

fn mouse_report(ctx: &egui::Context, area: egui::Id, dock: &DockState<Pane>) -> MouseReport {
    let (pointer, held) =
        ctx.input(|input| (input.pointer.latest_pos(), input.pointer.primary_down()));
    let to_screen = screen_pixels(ctx);
    let screen = pointer.zip(to_screen.as_ref()).map(|(at, to)| to(at));
    let outside = pointer.is_some_and(|at| !ctx.screen_rect().contains(at));
    MouseReport {
        screen,
        held,
        torn: floating::dragged_tab(ctx, area, dock).filter(|_| outside && held),
    }
}

/// What a window's name in the OS should say.
fn window_title(app: &EditorApp, pane: Option<Pane>) -> String {
    let name = match pane {
        Some(Pane::Script(id)) => app
            .tabs
            .path_of(id)
            .map_or("Script".to_owned(), str::to_owned),
        Some(Pane::Plugin(id)) => app.plugin_pane_title(id),
        Some(pane) => pane.label().to_owned(),
        None => "Panel".to_owned(),
    };
    format!("{name} - Axcenity Editor")
}

impl EditorApp {
    /// The icon panel windows show in their title bars and on the taskbar.
    pub fn set_window_icon(&mut self, icon: egui::IconData) {
        self.window_icon = Some(std::sync::Arc::new(icon));
    }

    fn builder_for(&self, index: usize) -> egui::ViewportBuilder {
        let window = &self.layout.windows[index];
        let mut builder = window_frame::viewport(egui::ViewportBuilder::default())
            .with_title(window_title(self, window.front_pane()))
            .with_min_inner_size([200.0, 140.0])
            .with_inner_size(
                window
                    .opens_at
                    .map_or(DEFAULT_SIZE, |at| egui::vec2(at.width, at.height)),
            );
        if let Some(icon) = &self.window_icon {
            builder = builder.with_icon(icon.clone());
        }
        builder
    }

    /// What to do once the main dock has been drawn: floating surfaces become
    /// windows, and a tab dragged off the edge tears off.
    pub(super) fn after_main_dock(&mut self, ctx: &egui::Context) {
        let mut dock = std::mem::replace(&mut self.layout.dock, DockState::new(Vec::new()));
        let report = mouse_report(ctx, main_area(self.layout.dock_generation), &dock);
        self.layout.absorb_floating(&mut dock, report.screen);
        self.layout.dock = dock;
        if let (Some(pane), Some(screen)) = (report.torn, report.screen) {
            self.layout.tear_off(pane, None, screen, DEFAULT_SIZE);
        }
        let main = ctx.input(|input| input.viewport().inner_rect).map(|rect| {
            egui::Rect::from_min_max(
                (rect.min.to_vec2() * ctx.pixels_per_point()).to_pos2(),
                (rect.max.to_vec2() * ctx.pixels_per_point()).to_pos2(),
            )
        });
        self.main_window_pixels = main;
        self.layout
            .update_following(None, report.screen, report.held, main);
    }

    /// Draws every panel window.
    pub(super) fn floating_windows(
        &mut self,
        ctx: &egui::Context,
        render_state: Option<&RenderState>,
        palette: &theme::Palette,
    ) {
        let ids: Vec<u64> = self.layout.windows.iter().map(|window| window.id).collect();
        for id in ids {
            self.floating_window(ctx, render_state, palette, id);
        }
        self.layout.forget_lost_follows();
        if self.layout.is_following() {
            ctx.request_repaint();
        }
    }

    fn floating_window(
        &mut self,
        ctx: &egui::Context,
        render_state: Option<&RenderState>,
        palette: &theme::Palette,
        id: u64,
    ) {
        let Some(index) = self.layout.windows.iter().position(|w| w.id == id) else {
            return;
        };
        let builder = self.builder_for(index);
        let viewport = viewport_of(id);
        let area = area_of(id, self.layout.dock_generation);
        let window = &mut self.layout.windows[index];
        if std::mem::take(&mut window.focus) {
            ctx.send_viewport_cmd_to(viewport, egui::ViewportCommand::Focus);
        }
        let following = window.following;
        let opens_at = window.opens_at.filter(|_| window.placed == window.opens_at);
        // Taken out while it's drawn, so the panels can borrow the app.
        let mut dock = std::mem::replace(&mut window.dock, DockState::new(Vec::new()));
        dock.translations.tab_context_menu.eject_button = "Float in New Window".to_owned();

        let title = window_title(self, floating_front(&dock));
        let viewport_builder = match opens_at {
            // Points of the window it opens from: egui scales it back up.
            Some(at) => builder.with_position(egui::pos2(
                at.x / ctx.pixels_per_point(),
                at.y / ctx.pixels_per_point(),
            )),
            None => builder,
        };
        let outcome = ctx.show_viewport_immediate(viewport, viewport_builder, |ctx, class| {
            // Its own chrome, unless it's drawn inside the main window.
            let own_window = class != egui::ViewportClass::Embedded;
            let strip_closed = own_window && window_frame::panel_strip(ctx);
            let style = layout::style(&ctx.style(), palette, ctx.pixels_per_point());
            let mut placeholder = ViewportOutput::default();
            let visuals = ctx.style().visuals.clone();
            let mut viewer = DockViewer {
                app: self,
                render_state,
                viewport: &mut placeholder,
                visuals,
                ctx: ctx.clone(),
                floating: Some(id),
            };
            let dock_area = egui_dock::DockArea::new(&mut dock)
                .id(area)
                .style(style)
                .show_leaf_collapse_buttons(false)
                .show_leaf_close_all_buttons(false);
            if class == egui::ViewportClass::Embedded {
                // A platform that can't open more windows.
                egui::Window::new(title.as_str())
                    .id(area.with("embedded"))
                    .default_size(DEFAULT_SIZE)
                    .show(ctx, |ui| dock_area.show_inside(ui, &mut viewer));
            } else {
                dock_area.show(ctx, &mut viewer);
            }
            // Keyboard shortcuts go to the window with focus.
            if ctx.input(|input| input.viewport().focused == Some(true)) {
                self.shortcuts(ctx);
            }
            if own_window {
                window_frame::edges(ctx);
            }
            let closed = strip_closed || ctx.input(|input| input.viewport().close_requested());
            // Read first: egui's Context mustn't be called inside `input`.
            let scale = ctx.pixels_per_point();
            let placed = ctx.input(|input| {
                let viewport = input.viewport();
                let outer = viewport.outer_rect?;
                let inner = viewport.inner_rect?;
                Some(Placement {
                    x: outer.min.x * scale,
                    y: outer.min.y * scale,
                    width: inner.width(),
                    height: inner.height(),
                })
            });
            if let Some(pointer) = following.filter(|follow| follow.held).map(|f| f.pointer) {
                let target = Layout::follow_target(pointer);
                let points = (target.to_vec2() / ctx.pixels_per_point()).to_pos2();
                ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(points));
            }
            let mouse = mouse_report(ctx, area, &dock);
            self.layout.absorb_floating(&mut dock, mouse.screen);
            (closed, placed, mouse)
        });
        let (closed, placed, mouse) = outcome;

        let Some(index) = self.layout.windows.iter().position(|w| w.id == id) else {
            return;
        };
        let window = &mut self.layout.windows[index];
        window.dock = dock;
        if placed.is_some() {
            window.placed = placed;
        }
        if let (Some(pane), Some(screen)) = (mouse.torn, mouse.screen) {
            self.layout.tear_off(pane, Some(id), screen, DEFAULT_SIZE);
        }
        let main = self.main_window_pixels;
        self.layout
            .update_following(Some(id), mouse.screen, mouse.held, main);
        if let Some(pane) = self.pending_main_dock.take() {
            self.layout.dock_in_main(pane);
        }
        self.layout.drop_empty_windows();
        if closed {
            self.layout.dock_window_in_main(id);
        }
    }
}

/// The panel a dock's window is named after.
fn floating_front(dock: &DockState<Pane>) -> Option<Pane> {
    dock.iter_leaves()
        .next()
        .and_then(|(_, leaf)| leaf.tabs().get(leaf.active.0).copied())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::tests::copy_of_example;

    fn a_frame(app: &mut EditorApp, ctx: &egui::Context) {
        let _ = ctx.run(Default::default(), |ctx| {
            let palette = theme::palette(ctx);
            app.floating_windows(ctx, None, &palette);
        });
    }

    #[test]
    fn a_floated_panel_draws_and_keeps_its_place_where_more_windows_are_not_possible() {
        let folder = copy_of_example("hello-touched", "floating");
        let mut app = EditorApp::new(Some(&folder));
        let id = app.layout.float(Pane::Explorer, None).unwrap();
        let ctx = crate::theme::test_context();
        for _ in 0..3 {
            a_frame(&mut app, &ctx);
        }
        assert!(app.layout.is_open(Pane::Explorer));
        assert_eq!(app.layout.windows[0].id, id);
        let _ = std::fs::remove_dir_all(folder);
    }

    /// Draws nothing in each tab.
    struct NoPanels;

    impl egui_dock::TabViewer for NoPanels {
        type Tab = Pane;
        fn title(&mut self, tab: &mut Pane) -> egui::WidgetText {
            tab.label().into()
        }
        fn ui(&mut self, _ui: &mut egui::Ui, _tab: &mut Pane) {}
    }

    /// One frame of `dock` with these mouse events, and what the mouse did.
    fn frame_with(
        ctx: &egui::Context,
        dock: &mut DockState<Pane>,
        area: egui::Id,
        events: Vec<egui::Event>,
    ) -> MouseReport {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(800.0, 600.0),
            )),
            events,
            ..Default::default()
        };
        let mut report = None;
        let _ = ctx.run(input, |ctx| {
            egui_dock::DockArea::new(dock)
                .id(area)
                .show(ctx, &mut NoPanels);
            report = Some(mouse_report(ctx, area, dock));
        });
        report.unwrap()
    }

    fn press(at: egui::Pos2, pressed: bool) -> egui::Event {
        egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        }
    }

    /// A dock of two tabs with the second being dragged over the panel, so
    /// egui_dock has started its own drag and drop.
    fn second_tab_dragged(area: egui::Id) -> (egui::Context, DockState<Pane>) {
        let moved = |to| vec![egui::Event::PointerMoved(to)];
        // Across the tab bar until the mouse is on the second tab.
        for x in (40..500).step_by(15) {
            let ctx = egui::Context::default();
            let mut dock = DockState::new(vec![Pane::Scene, Pane::Console]);
            let grab = egui::pos2(x as f32, 12.0);
            frame_with(&ctx, &mut dock, area, moved(grab));
            frame_with(&ctx, &mut dock, area, vec![press(grab, true)]);
            for at in [egui::pos2(300.0, 250.0), egui::pos2(310.0, 260.0)] {
                frame_with(&ctx, &mut dock, area, moved(at));
            }
            if floating::dragged_tab(&ctx, area, &dock) == Some(Pane::Console) {
                return (ctx, dock);
            }
        }
        panic!("the second tab couldn't be grabbed");
    }

    #[test]
    fn tearing_off_a_tab_mid_drag_leaves_the_dock_it_came_from_drawing() {
        let mut layout = Layout::default();
        let first = main_area(layout.dock_generation);
        let (ctx, dock) = second_tab_dragged(first);
        layout.dock = dock;
        let moved = |to| vec![egui::Event::PointerMoved(to)];
        let out = frame_with(
            &ctx,
            &mut layout.dock,
            first,
            moved(egui::pos2(1400.0, 200.0)),
        );
        assert_eq!(out.torn, Some(Pane::Console));

        layout
            .tear_off(Pane::Console, None, egui::pos2(1400.0, 200.0), DEFAULT_SIZE)
            .unwrap();

        // Drawing it again used to panic with an index out of bounds.
        let next = main_area(layout.dock_generation);
        for at in [egui::pos2(300.0, 250.0), egui::pos2(320.0, 260.0)] {
            frame_with(&ctx, &mut layout.dock, next, moved(at));
        }
        assert!(layout.windows[0].holds(Pane::Console));
        assert_eq!(floating::dragged_tab(&ctx, next, &layout.dock), None);
    }

    #[test]
    fn a_panel_window_has_no_os_frame_and_its_strip_closes_it() {
        let folder = copy_of_example("hello-touched", "panel-strip");
        let mut app = EditorApp::new(Some(&folder));
        app.layout.float(Pane::Explorer, None).unwrap();
        if !cfg!(target_os = "macos") {
            assert_eq!(app.builder_for(0).decorations, Some(false));
        }
        let _ = std::fs::remove_dir_all(folder);

        let ctx = crate::theme::test_context();
        let close = egui::pos2(790.0, 10.0);
        let frames = [
            vec![egui::Event::PointerMoved(close)],
            vec![press(close, true)],
            vec![press(close, false)],
        ];
        let mut clicked = false;
        for events in frames {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(800.0, 600.0),
                )),
                events,
                ..Default::default()
            };
            let _ = ctx.run(input, |ctx| {
                clicked |= window_frame::panel_strip(ctx);
            });
        }
        assert_eq!(clicked, !cfg!(target_os = "macos"));
    }

    #[test]
    fn a_tab_dragged_past_the_edge_of_the_window_is_torn_off() {
        let ctx = egui::Context::default();
        let mut dock = DockState::new(vec![Pane::Explorer, Pane::Console]);
        let area = egui::Id::new("test area");
        let grab = egui::pos2(30.0, 12.0);
        let moved = |to| vec![egui::Event::PointerMoved(to)];

        frame_with(&ctx, &mut dock, area, moved(grab));
        frame_with(&ctx, &mut dock, area, vec![press(grab, true)]);
        let inside = frame_with(&ctx, &mut dock, area, moved(egui::pos2(120.0, 200.0)));
        assert_eq!(inside.torn, None, "still inside the window");
        assert_eq!(
            floating::dragged_tab(&ctx, area, &dock),
            Some(Pane::Explorer),
            "the tab under the mouse is the one dragged"
        );

        let outside = frame_with(&ctx, &mut dock, area, moved(egui::pos2(1400.0, 200.0)));
        assert_eq!(outside.torn, Some(Pane::Explorer));
    }
}
