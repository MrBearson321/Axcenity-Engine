//! What the artboard shows while designing, and the Pages tab.
//!
//! **Visibility**: each row of the hierarchy has an eye. On a page (a Frame
//! in a PageLayout) it picks the page the artboard shows, so pages can be
//! worked on one after another whatever `CurrentPage` says; on anything
//! else it hides the element (and what's in it) on the artboard. Neither is
//! saved or an undo step, and Play, the Live Preview and the Scene View
//! show the game's own `Visible` and `CurrentPage`. Selecting an element on
//! another page shows that page.
//!
//! **Pages** (the designer's fourth tab) lists every ScreenGui as a card;
//! the picked one (the designer's ScreenGui) shows its pages, each drawn as
//! it looks in the game at its design size: one large, with ◀ and ▶ (or
//! the arrow keys), then each of its PageLayouts with its pages as
//! thumbnails, the one the game starts on flagged. A click on a thumbnail
//! shows it large; **Edit page** (or a double-click) opens it in the Design
//! tab with the page shown and selected. A ScreenGui without a PageLayout
//! is one slide.

use std::collections::{HashMap, HashSet};

use axcenity_core::{ClassName, InstanceId, Value, World};
use axcenity_render::glam::Vec3;
use axcenity_render::{Camera, UiDrawItem, UiRectangle};
use axcenity_ui::DesignerOverrides;
use eframe::egui;
use eframe::egui_wgpu::RenderState;

use super::{DesignerAction, DesignerScene, DesignerTab, UiDesigner, screen_guis, sketch};
use crate::edit::Edit;
use crate::preview::Canvas;
use crate::theme::{self, icons};
use crate::tree::Insertable;
use crate::widgets;

/// A page thumbnail's width, in points.
const THUMBNAIL: f32 = 200.0;
/// A ScreenGui card's width, in points.
const CARD: f32 = 160.0;
/// The width of the column of ScreenGui cards, in points.
const SCREEN_LIST: f32 = 184.0;
/// What's behind a slide's UI, as on the artboard.
const BACKDROP: [f32; 4] = [0.16, 0.17, 0.19, 1.0];

/// One slide: a ScreenGui, showing one page of one of its PageLayouts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Slide {
    pub gui: InstanceId,
    /// The PageLayout and its page, or `None` for a ScreenGui with none.
    pub page: Option<(InstanceId, InstanceId)>,
    pub title: String,
}

/// Every slide: each ScreenGui in the Workspace in tree order, then its
/// [`screen_slides`].
#[cfg(any(test, feature = "test-support"))]
pub fn slides(world: &World) -> Vec<Slide> {
    screen_guis(world)
        .into_iter()
        .flat_map(|gui| screen_slides(world, gui))
        .collect()
}

/// A ScreenGui's PageLayouts, at any depth in tree order, each with its
/// pages (the Frames in it).
pub fn page_layouts(world: &World, gui: InstanceId) -> Vec<(InstanceId, Vec<InstanceId>)> {
    super::hierarchy(world, gui)
        .into_iter()
        .filter_map(|(id, _)| {
            let instance = world.get(id)?;
            (instance.class() == ClassName::PageLayout).then(|| {
                let pages = instance
                    .children()
                    .iter()
                    .copied()
                    .filter(|&page| page_layout_of(world, page) == Some(id))
                    .collect();
                (id, pages)
            })
        })
        .collect()
}

/// One ScreenGui's slides: one per page of each of its PageLayouts, or
/// one for the ScreenGui when it has no pages.
pub fn screen_slides(world: &World, gui: InstanceId) -> Vec<Slide> {
    let name = |id| world.get(id).map_or("", |instance| instance.name());
    let layouts = page_layouts(world, gui);
    let mut slides = Vec::new();
    for (layout, pages) in &layouts {
        for &page in pages {
            let title = if layouts.len() > 1 {
                format!("{} › {} › {}", name(gui), name(*layout), name(page))
            } else {
                format!("{} › {}", name(gui), name(page))
            };
            slides.push(Slide {
                gui,
                page: Some((*layout, page)),
                title,
            });
        }
    }
    if slides.is_empty() {
        slides.push(Slide {
            gui,
            page: None,
            title: name(gui).to_owned(),
        });
    }
    slides
}

/// "3 pages", "1 page" or "No pages", for a ScreenGui's card.
fn page_count(world: &World, gui: InstanceId) -> String {
    match page_layouts(world, gui)
        .iter()
        .map(|(_, pages)| pages.len())
        .sum::<usize>()
    {
        0 => "No pages".to_owned(),
        1 => "1 page".to_owned(),
        count => format!("{count} pages"),
    }
}

/// The PageLayout `id` is a page of, when it's one (a Frame in a PageLayout).
pub fn page_layout_of(world: &World, id: InstanceId) -> Option<InstanceId> {
    let instance = world.get(id)?;
    let parent = instance.parent()?;
    (instance.class() == ClassName::Frame
        && world
            .get(parent)
            .is_some_and(|parent| parent.class() == ClassName::PageLayout))
    .then_some(parent)
}

/// Sets `pages` so `id` is on a shown page: each page around it (and `id`
/// itself, if it's a page) becomes the one its PageLayout shows.
pub fn reveal(world: &World, id: InstanceId, pages: &mut HashMap<InstanceId, InstanceId>) {
    let mut current = Some(id);
    while let Some(id) = current {
        if let Some(layout) = page_layout_of(world, id) {
            pages.insert(layout, id);
        }
        current = world.get(id).and_then(|instance| instance.parent());
    }
}

impl UiDesigner {
    /// The page the artboard shows for the PageLayout `layout`: the one
    /// picked with its eye, else the game's.
    pub fn artboard_page(&self, world: &World, layout: InstanceId) -> Option<InstanceId> {
        self.pages
            .get(&layout)
            .copied()
            .filter(|&page| page_layout_of(world, page) == Some(layout))
            .or_else(|| axcenity_ui::shown_page(world, layout))
    }

    /// Whether `id` is hidden on the artboard by its eye or an eye around it.
    pub fn hidden_on_artboard(&self, world: &World, id: InstanceId) -> bool {
        let mut current = Some(id);
        while let Some(id) = current {
            if self.hidden.contains(&id) {
                return true;
            }
            current = world.get(id).and_then(|instance| instance.parent());
        }
        false
    }

    /// Whether every page around `id` (and `id`, if it's a page) is the
    /// one the artboard shows.
    pub fn on_shown_page(&self, world: &World, id: InstanceId) -> bool {
        let mut current = Some(id);
        while let Some(id) = current {
            if let Some(layout) = page_layout_of(world, id)
                && self.artboard_page(world, layout) != Some(id)
            {
                return false;
            }
            current = world.get(id).and_then(|instance| instance.parent());
        }
        true
    }

    /// Clicks the eye of `id`, as the hierarchy's eye does: shows that
    /// page, or hides or shows the element on the artboard.
    pub fn toggle_eye(&mut self, world: &World, id: InstanceId) {
        if let Some(layout) = page_layout_of(world, id) {
            self.pages.insert(layout, id);
        } else if !self.hidden.remove(&id) {
            self.hidden.insert(id);
        }
    }

    /// Shows the page the selection is on, when the selection changes.
    pub(super) fn follow_page(&mut self, world: &World, selected: Option<InstanceId>) {
        if selected == self.page_followed {
            return;
        }
        self.page_followed = selected;
        if let Some(id) = selected {
            reveal(world, id, &mut self.pages);
        }
    }

    /// What the artboard shows differently from the game.
    pub(super) fn overrides(&self) -> DesignerOverrides<'_> {
        if self.previewing {
            // The Live Preview shows the game: its pages and what's visible.
            DesignerOverrides {
                only_enabled: true,
                ..DesignerOverrides::default()
            }
        } else {
            DesignerOverrides {
                hidden: Some(&self.hidden),
                pages: Some(&self.pages),
                only_enabled: false,
            }
        }
    }

    /// The eye at the right end of a hierarchy row (`row`), for `id`.
    pub(super) fn eye(
        &mut self,
        ui: &mut egui::Ui,
        world: &World,
        id: InstanceId,
        row: egui::Rect,
    ) {
        let palette = theme::palette(ui.ctx());
        let rect = egui::Rect::from_center_size(
            egui::pos2(row.right() - 12.0, row.center().y),
            egui::vec2(20.0, row.height()),
        );
        let page = page_layout_of(world, id);
        let (shown, tip) = match page {
            Some(layout) => (
                self.artboard_page(world, layout) == Some(id),
                "Show this page on the artboard. Only while designing: the game starts on \
                 the PageLayout's CurrentPage.",
            ),
            None => (
                !self.hidden.contains(&id),
                "Hide on the artboard while designing. Not saved: the game uses Visible.",
            ),
        };
        let response = ui
            .interact(
                rect,
                egui::Id::new(("ui designer eye", id)),
                egui::Sense::click(),
            )
            .on_hover_text(tip);
        let icon = if shown { icons::EYE } else { icons::EYE_SLASH };
        let color = if response.hovered() {
            palette.text_strong
        } else if shown {
            palette.text_secondary
        } else {
            palette.text_weak
        };
        ui.painter().text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            icon,
            egui::FontId::proportional(theme::type_scale::BODY),
            color,
        );
        if response.clicked() {
            self.toggle_eye(world, id);
        }
    }

    /// The slide shown large in the Pages tab, among its ScreenGui's.
    #[cfg(any(test, feature = "test-support"))]
    pub fn slide(&self) -> usize {
        self.slide
    }

    /// Forgets the slides' canvases, for drawing with another renderer.
    #[cfg(any(test, feature = "test-support"))]
    pub fn forget_canvases(&mut self) {
        self.slide_canvases.clear();
        self.canvas = Canvas::default();
    }

    /// Picks the ScreenGui whose pages the Pages tab shows, as clicking its
    /// card does: it becomes the designer's ScreenGui, and is selected.
    pub fn pick_screen(&mut self, gui: InstanceId, actions: &mut Vec<DesignerAction>) {
        if self.gui != Some(gui) {
            self.slide = 0;
        }
        self.gui = Some(gui);
        actions.push(DesignerAction::Select(Some(gui)));
    }

    /// The Pages tab: every ScreenGui as a card (a column at the left, or a
    /// row on top when narrow), and the picked one's pages.
    pub(super) fn pages_tab(
        &mut self,
        ui: &mut egui::Ui,
        scene: &DesignerScene,
        render_state: Option<&RenderState>,
        actions: &mut Vec<DesignerAction>,
    ) {
        let guis = screen_guis(scene.world);
        let Some(&first) = guis.first() else {
            ui.weak("There's no ScreenGui yet.");
            return;
        };
        let gui = self.gui.filter(|gui| guis.contains(gui)).unwrap_or(first);
        // Canvas 0 is the large slide, then one per card, then the pages.
        let first_page_canvas = guis.len() + 1;
        let mut cards = |designer: &mut Self, ui: &mut egui::Ui| {
            for (index, &id) in guis.iter().enumerate() {
                designer.screen_card(ui, scene, id, id == gui, index + 1, render_state, actions);
            }
        };
        if ui.available_width() >= super::STACK_BELOW {
            let height = ui.available_height();
            let top_down = egui::Layout::top_down(egui::Align::Min);
            let mut pages = Vec::new();
            ui.horizontal_top(|ui| {
                ui.allocate_ui_with_layout(egui::vec2(SCREEN_LIST, height), top_down, |ui| {
                    ui.strong("Screens");
                    egui::ScrollArea::vertical()
                        .id_salt("ui designer screens")
                        .auto_shrink([false, false])
                        .show(ui, |ui| cards(self, ui));
                });
                ui.separator();
                ui.allocate_ui_with_layout(
                    egui::vec2(ui.available_width(), height),
                    top_down,
                    |ui| {
                        self.screen_pages(
                            ui,
                            scene,
                            gui,
                            first_page_canvas,
                            render_state,
                            &mut pages,
                        )
                    },
                );
            });
            actions.extend(pages);
        } else {
            egui::ScrollArea::horizontal()
                .id_salt("ui designer screens")
                .show(ui, |ui| ui.horizontal_top(|ui| cards(self, ui)));
            ui.separator();
            self.screen_pages(ui, scene, gui, first_page_canvas, render_state, actions);
        }
    }

    /// A ScreenGui's card: how it looks, its name and how many pages it
    /// has. A click shows its pages; a double-click opens it to design.
    #[allow(clippy::too_many_arguments)]
    fn screen_card(
        &mut self,
        ui: &mut egui::Ui,
        scene: &DesignerScene,
        gui: InstanceId,
        chosen: bool,
        canvas: usize,
        render_state: Option<&RenderState>,
        actions: &mut Vec<DesignerAction>,
    ) {
        let world = scene.world;
        let (width, height) = axcenity_ui::design_size(world, gui);
        let size = egui::vec2(CARD, CARD * height as f32 / width.max(1) as f32);
        let name = world.get(gui).map_or("", |instance| instance.name());
        let slide = Slide {
            gui,
            page: None,
            title: name.to_owned(),
        };
        let response = ui
            .vertical(|ui| {
                ui.set_width(CARD);
                let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
                self.draw_slide(ui, rect, scene, &slide, canvas, render_state);
                let palette = theme::palette(ui.ctx());
                if chosen || response.hovered() {
                    ui.painter().rect_stroke(
                        rect,
                        3,
                        egui::Stroke::new(if chosen { 2.0_f32 } else { 1.0 }, palette.accent),
                        egui::StrokeKind::Outside,
                    );
                }
                ui.add(egui::Label::new(egui::RichText::new(name).strong()).truncate());
                ui.label(
                    egui::RichText::new(page_count(world, gui))
                        .small()
                        .color(palette.text_weak),
                );
                ui.add_space(6.0);
                response
            })
            .inner
            .on_hover_text(format!(
                "{name}: click to see its pages, double-click to design it"
            ));
        if response.clicked() {
            self.pick_screen(gui, actions);
        }
        if response.double_clicked() {
            self.edit_slide(world, &slide, actions);
        }
    }

    /// One ScreenGui's pages: the slideshow over all of them, then each
    /// PageLayout with its pages as thumbnails, its start page flagged.
    fn screen_pages(
        &mut self,
        ui: &mut egui::Ui,
        scene: &DesignerScene,
        gui: InstanceId,
        first_canvas: usize,
        render_state: Option<&RenderState>,
        actions: &mut Vec<DesignerAction>,
    ) {
        let world = scene.world;
        let name = |id| world.get(id).map_or("", |instance| instance.name());
        let slides = screen_slides(world, gui);
        self.slide = self.slide.min(slides.len() - 1);
        // The arrow keys move through the slides while nothing is typed in.
        if !ui.ctx().egui_wants_keyboard_input() {
            let (back, on) = ui.input(|input| {
                use crate::shortcuts::{Action, pressed};
                (
                    pressed(input, Action::PreviousSlide),
                    pressed(input, Action::NextSlide),
                )
            });
            if back {
                self.slide = self.slide.saturating_sub(1);
            }
            if on {
                self.slide = (self.slide + 1).min(slides.len() - 1);
            }
        }
        let mut edit = None;

        // The bar over the large slide.
        ui.horizontal_wrapped(|ui| {
            if widgets::icon_button(ui, icons::CARET_LEFT, "Previous page (←)", false).clicked() {
                self.slide = self.slide.saturating_sub(1);
            }
            ui.label(format!("{} / {}", self.slide + 1, slides.len()));
            if widgets::icon_button(ui, icons::CARET_RIGHT, "Next page (→)", false).clicked() {
                self.slide = (self.slide + 1).min(slides.len() - 1);
            }
            ui.strong(&slides[self.slide].title);
            if ui
                .button(theme::with_icon(icons::PENCIL_SIMPLE, "Edit page"))
                .on_hover_text("Open this page in the Design tab")
                .clicked()
            {
                edit = Some(self.slide);
            }
        });
        let current = slides[self.slide].clone();
        // The large slide: as much of the room as leaves a row of thumbnails.
        let room = ui.available_size();
        let large_height = (room.y - THUMBNAIL * 0.75 - 70.0)
            .max(room.y * 0.45)
            .max(80.0);
        let (large, response) =
            ui.allocate_exact_size(egui::vec2(room.x, large_height), egui::Sense::click());
        let fitted = self.draw_slide(ui, large, scene, &current, 0, render_state);
        if response.double_clicked()
            && fitted.is_some_and(|rect| response.hover_pos().is_some_and(|at| rect.contains(at)))
        {
            edit = Some(self.slide);
        }
        ui.separator();

        let layouts = page_layouts(world, gui);
        egui::ScrollArea::vertical()
            .id_salt(("ui designer pages", gui))
            .auto_shrink([false, false])
            .show(ui, |ui| {
                if layouts.is_empty() {
                    ui.weak(
                        "This ScreenGui has no pages: it's one screen. Add pages to switch \
                         between screens in it, such as a menu's Home and Settings.",
                    );
                    if ui
                        .add_enabled(
                            scene.editing,
                            egui::Button::new(theme::with_icon(icons::PLUS, "Add pages")),
                        )
                        .on_hover_text("Put a PageLayout with its first page in this ScreenGui")
                        .clicked()
                    {
                        actions.push(DesignerAction::Insert {
                            what: Insertable::Page,
                            parent: gui,
                            at: [0.0, 0.0],
                        });
                    }
                }
                let mut index = 0;
                for (layout, pages) in &layouts {
                    let start = axcenity_ui::shown_page(world, *layout);
                    ui.horizontal_wrapped(|ui| {
                        ui.label(icons::STACK);
                        ui.strong(name(*layout));
                        let count = match pages.len() {
                            1 => "1 page".to_owned(),
                            count => format!("{count} pages"),
                        };
                        let starts = start.map_or(String::new(), |start| {
                            format!(" · the game starts on {}", name(start))
                        });
                        ui.weak(format!("PageLayout · {count}{starts}"));
                    });
                    ui.horizontal_wrapped(|ui| {
                        for &page in pages {
                            let slide_index = index;
                            index += 1;
                            let response = self.page_thumbnail(
                                ui,
                                scene,
                                &slides[slide_index],
                                slide_index == self.slide,
                                start == Some(page),
                                first_canvas + slide_index,
                                render_state,
                            );
                            if response.clicked() {
                                self.slide = slide_index;
                            }
                            if response.double_clicked() {
                                edit = Some(slide_index);
                            }
                            response.context_menu(|ui| {
                                if ui.button("Edit page").clicked() {
                                    edit = Some(slide_index);
                                    ui.close();
                                }
                                let starts_here = start == Some(page);
                                if ui
                                    .add_enabled(
                                        scene.editing && !starts_here,
                                        egui::Button::new("Start on this page"),
                                    )
                                    .on_hover_text("Set the PageLayout's CurrentPage to it")
                                    .clicked()
                                {
                                    actions.push(DesignerAction::Edit(
                                        *layout,
                                        Edit::Property(
                                            "CurrentPage",
                                            Value::String(name(page).to_owned()),
                                        ),
                                    ));
                                    ui.close();
                                }
                            });
                        }
                        if ui
                            .add_enabled(
                                scene.editing,
                                egui::Button::new(theme::with_icon(icons::PLUS, "Page")),
                            )
                            .on_hover_text(format!("Add a page to {}", name(*layout)))
                            .clicked()
                        {
                            actions.push(DesignerAction::Insert {
                                what: Insertable::Page,
                                parent: *layout,
                                at: [0.0, 0.0],
                            });
                        }
                    });
                    ui.add_space(8.0);
                }
            });

        if let Some(index) = edit {
            self.edit_slide(world, &slides[index], actions);
        }
    }

    /// A page's thumbnail with its name under it (flagged when the game
    /// starts on it), outlined when it's the one shown large.
    #[allow(clippy::too_many_arguments)]
    fn page_thumbnail(
        &mut self,
        ui: &mut egui::Ui,
        scene: &DesignerScene,
        slide: &Slide,
        chosen: bool,
        start: bool,
        canvas: usize,
        render_state: Option<&RenderState>,
    ) -> egui::Response {
        let world = scene.world;
        let (width, height) = axcenity_ui::design_size(world, slide.gui);
        let size = egui::vec2(THUMBNAIL, THUMBNAIL * height as f32 / width.max(1) as f32);
        let page_name = slide
            .page
            .and_then(|(_, page)| world.get(page))
            .map_or("", |page| page.name());
        ui.vertical(|ui| {
            ui.set_width(THUMBNAIL);
            let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
            self.draw_slide(ui, rect, scene, slide, canvas, render_state);
            let palette = theme::palette(ui.ctx());
            if chosen || response.hovered() {
                ui.painter().rect_stroke(
                    rect,
                    3,
                    egui::Stroke::new(if chosen { 2.0_f32 } else { 1.0 }, palette.accent),
                    egui::StrokeKind::Outside,
                );
            }
            let label = if start {
                format!("{} {page_name}", icons::FLAG)
            } else {
                page_name.to_owned()
            };
            ui.add(egui::Label::new(egui::RichText::new(label).small()).truncate());
            response
        })
        .inner
        .on_hover_text(if start {
            format!("{}: the game starts on this page. Click to show it above, double-click to edit it, right-click for more", slide.title)
        } else {
            format!("{}: click to show it above, double-click to edit it, right-click for more", slide.title)
        })
    }

    /// Opens `slide` in the Design tab: its ScreenGui, with its page shown
    /// and selected.
    pub fn edit_slide(&mut self, world: &World, slide: &Slide, actions: &mut Vec<DesignerAction>) {
        self.gui = Some(slide.gui);
        self.tab = DesignerTab::Design;
        self.fit();
        let selected = slide.page.map_or(slide.gui, |(_, page)| page);
        reveal(world, selected, &mut self.pages);
        // The page is shown already, so following the selection is done.
        self.page_followed = Some(selected);
        self.followed = Some(selected);
        actions.push(DesignerAction::Select(Some(selected)));
    }

    /// Draws `slide` fitted into `room` at its design size, with the game's
    /// renderer when there is one (`canvas` picks which of the tab's
    /// canvases draws it). Returns where it went.
    fn draw_slide(
        &mut self,
        ui: &egui::Ui,
        room: egui::Rect,
        scene: &DesignerScene,
        slide: &Slide,
        canvas: usize,
        render_state: Option<&RenderState>,
    ) -> Option<egui::Rect> {
        let world = scene.world;
        let screen = axcenity_ui::design_size(world, slide.gui);
        let zoom = (room.width() / screen.0 as f32).min(room.height() / screen.1 as f32);
        if !zoom.is_finite() || zoom <= 0.0 {
            return None;
        }
        let size = egui::vec2(screen.0 as f32, screen.1 as f32) * zoom;
        let rect = egui::Rect::from_center_size(room.center(), size);
        let ppp = ui.ctx().pixels_per_point();
        // Only what's in view is drawn (a thumbnail can be scrolled half
        // out), laid out from the corner of that.
        let visible = rect.intersect(ui.clip_rect());
        if visible.width() < 1.0 || visible.height() < 1.0 {
            return Some(rect);
        }
        let origin = (rect.min - visible.min) * ppp;
        // The slide's page, and the pages around it, shown.
        let mut pages = self.pages.clone();
        if let Some((_, page)) = slide.page {
            reveal(world, page, &mut pages);
        }
        let hidden = HashSet::new();
        let layout = axcenity_ui::layout_artboard_view(
            world,
            slide.gui,
            scene.stylesheets,
            axcenity_ui::ArtboardView {
                screen,
                scale: zoom * ppp,
                origin: [origin.x, origin.y],
                shown: None,
                scrolled: None,
                safe_insets: super::safe_insets(screen),
                overrides: DesignerOverrides {
                    hidden: Some(&hidden),
                    pages: Some(&pages),
                    only_enabled: false,
                },
            },
        );
        let backdrop = UiDrawItem::Rectangle(UiRectangle {
            position: [origin.x, origin.y],
            size: [size.x * ppp, size.y * ppp],
            color: BACKDROP,
            color_end: BACKDROP,
            border_color: [0.0; 4],
            border_width: 0.0,
            border_radius: 0.0,
            ..Default::default()
        });
        match render_state {
            Some(render_state) => {
                let camera = Camera {
                    position: Vec3::new(0.0, 5.0, 10.0),
                    look_at: Vec3::ZERO,
                    fov_y_degrees: 70.0,
                };
                let mut list = axcenity_scene::build_draw_list(&self.empty, scene.assets, camera);
                list.ui = std::iter::once(backdrop).chain(layout.draw_items).collect();
                axcenity_scene::attach_ui_textures(&mut list.ui, scene.assets);
                if self.slide_canvases.len() <= canvas {
                    self.slide_canvases.resize_with(canvas + 1, Canvas::default);
                }
                self.slide_canvases[canvas].paint(ui, visible, render_state, &list);
            }
            None => sketch(
                &ui.painter_at(visible),
                visible.min,
                ppp,
                std::iter::once(&backdrop).chain(&layout.draw_items),
            ),
        }
        Some(rect)
    }
}
