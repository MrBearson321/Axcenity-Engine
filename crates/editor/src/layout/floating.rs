//! Panels in their own OS windows, which can go anywhere, on any monitor.
//!
//! `egui_dock`'s floating surfaces live inside the main window, so each is
//! turned into a [`FloatingWindow`] the moment it appears, with a dock of its
//! own. A tab dragged past the edge of the window it's in tears off into a
//! new one that follows the mouse (`app/floating.rs` draws them all).

use eframe::egui;
use egui_dock::{DockState, NodeIndex, Surface, SurfaceIndex};
use serde::{Deserialize, Serialize};

use super::{Layout, Pane};

/// How big a window starts out when nothing says otherwise, in points.
pub const DEFAULT_SIZE: egui::Vec2 = egui::vec2(420.0, 520.0);
/// Where the mouse holds a window that's being torn off, from its top left,
/// in physical pixels.
const GRAB: egui::Vec2 = egui::vec2(60.0, 14.0);

/// Where the window is and how big, kept between runs. Physical pixels for
/// the place, as monitors can differ in scale, and points for the size.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Placement {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

/// A window following the mouse, torn off and not yet let go of.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Following {
    /// The window the drag began in, or `None` for the main one. Only it
    /// gets the mouse's events until the button comes up.
    pub source: Option<u64>,
    /// Where the mouse is on the screen, in physical pixels.
    pub pointer: egui::Pos2,
    pub held: bool,
}

/// One panel window.
pub struct FloatingWindow {
    pub id: u64,
    pub dock: DockState<Pane>,
    /// Where it was asked to open, which egui uses only on creation.
    pub opens_at: Option<Placement>,
    /// Where it is now, read from the OS each frame.
    pub placed: Option<Placement>,
    pub following: Option<Following>,
    /// Bring it to the front on the next frame.
    pub focus: bool,
    /// Which theme (dark or not) its OS title bar was last told to use.
    pub dark_title_bar: Option<bool>,
}

impl FloatingWindow {
    /// The panels in it.
    pub fn panes(&self) -> Vec<Pane> {
        self.dock.iter_all_tabs().map(|(_, pane)| *pane).collect()
    }

    pub fn holds(&self, pane: Pane) -> bool {
        self.dock.find_tab(&pane).is_some()
    }

    /// The panel it's named after: the active tab of its first leaf.
    pub fn front_pane(&self) -> Option<Pane> {
        self.dock
            .iter_leaves()
            .next()
            .and_then(|(_, leaf)| leaf.tabs().get(leaf.active.0).copied())
    }
}

/// How windows are written in the saved layout.
#[derive(Serialize, Deserialize)]
pub(super) struct SavedWindow {
    pub dock: DockState<Pane>,
    pub placed: Option<Placement>,
}

fn dock_of(tabs: Vec<Pane>) -> DockState<Pane> {
    DockState::new(tabs)
}

impl Layout {
    /// Opens a window holding `dock`.
    fn add_window(
        &mut self,
        dock: DockState<Pane>,
        opens_at: Option<Placement>,
        following: Option<Following>,
    ) -> u64 {
        self.next_window += 1;
        self.windows.push(FloatingWindow {
            id: self.next_window,
            dock,
            opens_at,
            placed: opens_at,
            following,
            focus: true,
            dark_title_bar: None,
        });
        self.next_window
    }

    pub(super) fn restore_window(&mut self, saved: SavedWindow) {
        if saved.dock.iter_all_tabs().next().is_some() {
            self.add_window(saved.dock, saved.placed, None);
            if let Some(window) = self.windows.last_mut() {
                window.focus = false;
            }
        }
    }

    /// The saved form of every window, with `keep` choosing the tabs saved.
    pub(super) fn saved_windows(&self, keep: impl Fn(&Pane) -> bool + Copy) -> Vec<SavedWindow> {
        self.windows
            .iter()
            .map(|window| SavedWindow {
                dock: window.dock.filter_tabs(keep),
                placed: window.placed,
            })
            .filter(|saved| saved.dock.iter_all_tabs().next().is_some())
            .collect()
    }

    /// Takes `pane` out of whichever dock it's in.
    fn take_pane(&mut self, pane: Pane) -> bool {
        if let Some(found) = self.dock.find_tab(&pane) {
            self.dock.remove_tab(found);
            return true;
        }
        for window in &mut self.windows {
            if let Some(found) = window.dock.find_tab(&pane) {
                window.dock.remove_tab(found);
                return true;
            }
        }
        false
    }

    /// Closes windows left with no panels in them.
    pub fn drop_empty_windows(&mut self) {
        self.windows
            .retain(|window| window.dock.iter_all_tabs().next().is_some());
    }

    /// Puts `pane` in a window of its own at `opens_at`. The Scene stays in
    /// the main window, and so does a panel that isn't open.
    pub fn float(&mut self, pane: Pane, opens_at: Option<Placement>) -> Option<u64> {
        if pane == Pane::Scene || !self.take_pane(pane) {
            return None;
        }
        self.drop_empty_windows();
        Some(self.add_window(dock_of(vec![pane]), opens_at, None))
    }

    /// Tears `pane` off into a window following the mouse at `pointer`
    /// (physical pixels), for a drag that began in window `source`. Not if
    /// it's the only panel of its window: that window can just be moved.
    pub fn tear_off(
        &mut self,
        pane: Pane,
        source: Option<u64>,
        pointer: egui::Pos2,
        size: egui::Vec2,
    ) -> Option<u64> {
        if let Some(from) = source
            && self
                .windows
                .iter()
                .find(|window| window.id == from)
                .is_none_or(|window| window.panes().len() < 2)
        {
            return None;
        }
        let at = pointer - GRAB;
        let opens_at = Placement {
            x: at.x,
            y: at.y,
            width: size.x,
            height: size.y,
        };
        let id = self.float(pane, Some(opens_at))?;
        // egui_dock remembers the drag by the tab's old place, and would look
        // for it there until the button comes up: start its memory afresh.
        self.dock_generation += 1;
        if let Some(window) = self.windows.last_mut() {
            window.following = Some(Following {
                source,
                pointer,
                held: true,
            });
        }
        Some(id)
    }

    /// Where a following window goes for the mouse at `pointer`.
    pub fn follow_target(pointer: egui::Pos2) -> egui::Pos2 {
        pointer - GRAB
    }

    /// Moves every floating surface of `dock` (the main dock or a window's)
    /// into a window of its own, opening where the mouse is (`pointer`,
    /// physical pixels on the screen).
    pub fn absorb_floating(&mut self, dock: &mut DockState<Pane>, pointer: Option<egui::Pos2>) {
        let surfaces: Vec<SurfaceIndex> = dock
            .iter_surfaces()
            .enumerate()
            .filter(|(_, surface)| matches!(surface, Surface::Window(..)))
            .map(|(index, _)| SurfaceIndex(index))
            .collect();
        for index in surfaces {
            let Some(Surface::Window(tree, _)) = dock.remove_surface(index) else {
                continue;
            };
            let opens_at = pointer.map(|pointer| {
                let at = pointer - GRAB;
                Placement {
                    x: at.x,
                    y: at.y,
                    width: DEFAULT_SIZE.x,
                    height: DEFAULT_SIZE.y,
                }
            });
            let mut own = dock_of(Vec::new());
            *own.main_surface_mut() = tree;
            self.add_window(own, opens_at, None);
        }
    }

    /// Puts `pane` back in the main window, where it starts out.
    pub fn dock_in_main(&mut self, pane: Pane) {
        if self.take_pane(pane) {
            self.drop_empty_windows();
            self.open(pane);
        }
    }

    /// Closes window `id`, putting its panels back in the main window, so
    /// closing one never loses a panel.
    pub fn dock_window_in_main(&mut self, id: u64) {
        let Some(index) = self.windows.iter().position(|window| window.id == id) else {
            return;
        };
        let window = self.windows.remove(index);
        for pane in window.panes() {
            self.open(pane);
        }
    }

    /// True while any window is following the mouse.
    pub fn is_following(&self) -> bool {
        self.windows.iter().any(|window| window.following.is_some())
    }

    /// What a mouse in window `source` is doing this frame: moves the
    /// windows it's dragging along, and, if the button has come up, lets go
    /// of them. A panel dragged out of a window and let go over the main
    /// one (`main`, physical pixels) goes back in it.
    pub fn update_following(
        &mut self,
        source: Option<u64>,
        pointer: Option<egui::Pos2>,
        held: bool,
        main: Option<egui::Rect>,
    ) {
        let mut dropped = Vec::new();
        for window in &mut self.windows {
            let Some(follow) = window.following.as_mut().filter(|f| f.source == source) else {
                continue;
            };
            if let Some(pointer) = pointer {
                follow.pointer = pointer;
            }
            follow.held = held;
            if !held {
                let over_main =
                    source.is_some() && main.is_some_and(|main| main.contains(follow.pointer));
                window.following = None;
                if over_main {
                    dropped.push(window.id);
                }
            }
        }
        for id in dropped {
            self.dock_window_in_main(id);
        }
    }

    /// Lets go of windows whose drag began in a window that's gone.
    pub fn forget_lost_follows(&mut self) {
        let ids: Vec<u64> = self.windows.iter().map(|window| window.id).collect();
        for window in &mut self.windows {
            if let Some(follow) = window.following
                && follow.source.is_some_and(|source| !ids.contains(&source))
            {
                window.following = None;
            }
        }
    }
}

/// The panel being dragged by its tab in `dock`, drawn with the dock area
/// `area`. egui_dock doesn't say; its tabs' ids are what it drags by.
pub fn dragged_tab(ctx: &egui::Context, area: egui::Id, dock: &DockState<Pane>) -> Option<Pane> {
    let dragged = ctx.dragged_id()?;
    for (s, surface) in dock.iter_surfaces().enumerate() {
        let Some(tree) = surface.node_tree() else {
            continue;
        };
        for (n, node) in tree.iter().enumerate() {
            for (t, pane) in node.tabs().unwrap_or_default().iter().enumerate() {
                let id = area
                    .with((SurfaceIndex(s), "surface"))
                    .with((NodeIndex(n), "node"))
                    .with((t, "tab"));
                if id == dragged {
                    return Some(*pane);
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(x: f32, y: f32) -> Placement {
        Placement {
            x,
            y,
            width: 300.0,
            height: 200.0,
        }
    }

    fn windowed(layout: &Layout, pane: Pane) -> bool {
        layout.windows.iter().any(|window| window.holds(pane))
    }

    #[test]
    fn a_floated_panel_stays_open_in_its_own_window_and_leaves_the_dock() {
        let mut layout = Layout::default();
        let id = layout
            .float(Pane::Explorer, Some(at(4000.0, 80.0)))
            .unwrap();
        assert!(layout.is_open(Pane::Explorer));
        assert!(layout.dock.find_tab(&Pane::Explorer).is_none());
        assert_eq!(layout.windows[0].id, id);
        assert_eq!(layout.windows[0].opens_at.unwrap().x, 4000.0);
    }

    #[test]
    fn the_scene_never_leaves_the_main_window() {
        let mut layout = Layout::default();
        assert_eq!(layout.float(Pane::Scene, None), None);
        assert!(layout.windows.is_empty());
    }

    #[test]
    fn panels_are_found_shown_and_closed_wherever_they_are() {
        let mut layout = Layout::default();
        layout.float(Pane::Console, None);
        layout.windows[0].focus = false;
        layout.open(Pane::Console);
        assert!(layout.windows[0].focus, "showing it brings its window up");
        assert!(windowed(&layout, Pane::Console), "and doesn't move it");
        layout.toggle(Pane::Console);
        assert!(!layout.is_open(Pane::Console));
        assert!(layout.windows.is_empty(), "its empty window closes");
    }

    #[test]
    fn closing_a_window_puts_its_panels_back_in_the_main_window() {
        let mut layout = Layout::default();
        let id = layout.float(Pane::Properties, None).unwrap();
        layout.windows[0].dock.push_to_first_leaf(Pane::Profiler);
        layout.dock.retain_tabs(|pane| *pane != Pane::Profiler);
        layout.dock_window_in_main(id);
        assert!(layout.windows.is_empty());
        assert!(layout.dock.find_tab(&Pane::Properties).is_some());
        assert!(layout.dock.find_tab(&Pane::Profiler).is_some());
    }

    #[test]
    fn a_tab_torn_off_follows_the_mouse_until_let_go() {
        let mut layout = Layout::default();
        let pointer = egui::pos2(3000.0, 500.0);
        layout
            .tear_off(Pane::Assets, None, pointer, DEFAULT_SIZE)
            .unwrap();
        assert!(layout.is_following());
        let first = layout.windows[0].opens_at.unwrap();
        assert_eq!((first.x, first.y), (pointer.x - GRAB.x, pointer.y - GRAB.y));

        let later = egui::pos2(5100.0, 700.0);
        layout.update_following(None, Some(later), true, None);
        assert_eq!(layout.windows[0].following.unwrap().pointer, later);
        layout.update_following(None, Some(later), false, None);
        assert!(!layout.is_following(), "letting go stops it following");
        assert!(windowed(&layout, Pane::Assets), "and it stays out there");
    }

    #[test]
    fn a_panel_dragged_out_of_a_window_and_dropped_on_the_main_one_docks_again() {
        let mut layout = Layout::default();
        let source = layout.float(Pane::Explorer, None).unwrap();
        let tree_pane = Pane::Terrain;
        layout.dock.retain_tabs(|pane| *pane != tree_pane);
        layout.windows[0].dock.push_to_first_leaf(tree_pane);

        let main = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(1000.0, 800.0));
        let over_main = egui::pos2(500.0, 400.0);
        layout
            .tear_off(tree_pane, Some(source), over_main, DEFAULT_SIZE)
            .unwrap();
        layout.update_following(Some(source), Some(over_main), false, Some(main));
        assert!(layout.dock.find_tab(&tree_pane).is_some());
        assert_eq!(layout.windows.len(), 1, "only the source window is left");
    }

    #[test]
    fn the_only_panel_of_a_window_is_not_torn_off() {
        let mut layout = Layout::default();
        let source = layout.float(Pane::Explorer, None).unwrap();
        let pointer = egui::pos2(0.0, 0.0);
        assert_eq!(
            layout.tear_off(Pane::Explorer, Some(source), pointer, DEFAULT_SIZE),
            None
        );
    }

    fn detached_console() -> (Layout, DockState<Pane>) {
        let mut layout = Layout::default();
        let found = layout.dock.find_tab(&Pane::Console).unwrap();
        layout.dock.detach_tab(
            found,
            egui::Rect::from_min_size(egui::pos2(100.0, 50.0), DEFAULT_SIZE),
        );
        let dock = std::mem::replace(&mut layout.dock, dock_of(Vec::new()));
        (layout, dock)
    }

    #[test]
    fn a_surface_egui_dock_floated_becomes_an_os_window_where_the_mouse_let_it_go() {
        let (mut layout, mut dock) = detached_console();
        let pointer = egui::pos2(2100.0, 300.0);
        layout.absorb_floating(&mut dock, Some(pointer));
        layout.dock = dock;
        assert!(windowed(&layout, Pane::Console));
        assert_eq!(
            layout
                .dock
                .iter_surfaces()
                .filter(|s| !s.is_empty())
                .count(),
            1
        );
        let opens = layout.windows[0].opens_at.unwrap();
        assert_eq!((opens.x, opens.y), (pointer.x - GRAB.x, pointer.y - GRAB.y));
    }

    #[test]
    fn a_surface_floated_without_a_mouse_opens_where_the_os_likes() {
        let (mut layout, mut dock) = detached_console();
        layout.absorb_floating(&mut dock, None);
        assert_eq!(layout.windows[0].opens_at, None);
    }

    #[test]
    fn windows_and_where_they_are_are_saved_between_runs() {
        let mut layout = Layout::default();
        layout.float(Pane::Explorer, Some(at(-1600.0, 200.0)));
        layout.windows[0].placed = Some(at(-1500.0, 240.0));
        let loaded = Layout::from_text(&layout.to_text());
        assert_eq!(loaded.windows.len(), 1);
        assert!(loaded.is_open(Pane::Explorer));
        assert!(windowed(&loaded, Pane::Explorer));
        assert_eq!(loaded.windows[0].opens_at, Some(at(-1500.0, 240.0)));
    }

    #[test]
    fn a_layout_saved_before_windows_existed_still_loads() {
        let layout = Layout::default();
        let old = serde_json::to_string(&layout.dock).unwrap();
        let loaded = Layout::from_text(&old);
        assert!(loaded.windows.is_empty());
        assert!(loaded.is_open(Pane::Explorer));
    }

    #[test]
    fn script_tabs_are_not_saved_and_a_window_of_only_scripts_is_left_out() {
        let mut layout = Layout::default();
        layout.sync_scripts(&[7], None, None);
        layout.float(Pane::Script(7), None);
        assert_eq!(layout.windows.len(), 1);
        assert!(Layout::from_text(&layout.to_text()).windows.is_empty());
    }

    #[test]
    fn script_tabs_in_a_window_close_with_their_script() {
        let mut layout = Layout::default();
        layout.sync_scripts(&[7], None, None);
        layout.float(Pane::Script(7), None);
        layout.sync_scripts(&[7], None, None);
        assert_eq!(layout.windows.len(), 1, "an open script isn't added twice");
        layout.sync_scripts(&[], None, None);
        assert!(layout.windows.is_empty());
    }

    #[test]
    fn reset_layout_brings_every_panel_home() {
        let mut layout = Layout::default();
        layout.float(Pane::Explorer, None);
        layout.reset();
        assert!(layout.windows.is_empty());
        assert!(layout.dock.find_tab(&Pane::Explorer).is_some());
    }
}
