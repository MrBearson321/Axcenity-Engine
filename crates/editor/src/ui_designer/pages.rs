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
//! **Pages** (the designer's fourth tab) is a slideshow of every page of
//! every ScreenGui (a ScreenGui without a PageLayout is one slide), each
//! drawn as it looks in the game at its design size: one large, with ◀ and
//! ▶ (or the arrow keys), and all of them as thumbnails below. A click on a
//! thumbnail shows it large; **Edit page** (or a double-click) opens it in
//! the Design tab with the page shown and selected.

use std::collections::{HashMap, HashSet};

use axcenity_core::{ClassName, InstanceId, World};
use axcenity_render::glam::Vec3;
use axcenity_render::{Camera, UiDrawItem, UiRectangle};
use axcenity_ui::DesignerOverrides;
use eframe::egui;
use eframe::egui_wgpu::RenderState;

use super::{DesignerAction, DesignerScene, DesignerTab, UiDesigner, screen_guis, sketch};
use crate::animation::preview::Canvas;
use crate::theme::{self, icons};
use crate::widgets;

/// A thumbnail's width, in points.
const THUMBNAIL: f32 = 200.0;
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

/// Every slide: each ScreenGui in the Workspace in tree order, one per
/// page of each of its PageLayouts (in tree order), or one for the
/// ScreenGui when it has no pages.
pub fn slides(world: &World) -> Vec<Slide> {
    let mut slides = Vec::new();
    for gui in screen_guis(world) {
        let gui_name = world.get(gui).map_or("", |instance| instance.name());
        let layouts: Vec<InstanceId> = super::hierarchy(world, gui)
            .into_iter()
            .map(|(id, _)| id)
            .filter(|&id| {
                world
                    .get(id)
                    .is_some_and(|instance| instance.class() == ClassName::PageLayout)
            })
            .collect();
        let mut pages = 0;
        for &layout in &layouts {
            let Some(instance) = world.get(layout) else {
                continue;
            };
            for &page in instance.children() {
                let Some(frame) = world.get(page) else {
                    continue;
                };
                if frame.class() != ClassName::Frame {
                    continue;
                }
                let title = if layouts.len() > 1 {
                    format!("{gui_name} › {} › {}", instance.name(), frame.name())
                } else {
                    format!("{gui_name} › {}", frame.name())
                };
                slides.push(Slide {
                    gui,
                    page: Some((layout, page)),
                    title,
                });
                pages += 1;
            }
        }
        if pages == 0 {
            slides.push(Slide {
                gui,
                page: None,
                title: gui_name.to_owned(),
            });
        }
    }
    slides
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
            palette.text_muted
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

    /// The slide shown large in the Pages tab.
    #[cfg(test)]
    pub fn slide(&self) -> usize {
        self.slide
    }

    /// The Pages tab: the slideshow, then every slide as a thumbnail.
    pub(super) fn pages_tab(
        &mut self,
        ui: &mut egui::Ui,
        scene: &DesignerScene,
        render_state: Option<&RenderState>,
        actions: &mut Vec<DesignerAction>,
    ) {
        let world = scene.world;
        let slides = slides(world);
        if slides.is_empty() {
            ui.weak("There's no ScreenGui yet.");
            return;
        }
        self.slide = self.slide.min(slides.len() - 1);
        // The arrow keys move through the slides while nothing is typed in.
        if !ui.ctx().wants_keyboard_input() {
            let (back, on) = ui.input(|input| {
                (
                    input.key_pressed(egui::Key::ArrowLeft),
                    input.key_pressed(egui::Key::ArrowRight),
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
        let current = slides[self.slide].clone();

        // The bar over the large slide.
        ui.horizontal(|ui| {
            if widgets::icon_button(ui, icons::CARET_LEFT, "Previous page (←)", false).clicked() {
                self.slide = self.slide.saturating_sub(1);
            }
            ui.label(format!("{} / {}", self.slide + 1, slides.len()));
            if widgets::icon_button(ui, icons::CARET_RIGHT, "Next page (→)", false).clicked() {
                self.slide = (self.slide + 1).min(slides.len() - 1);
            }
            ui.strong(&current.title);
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
        let large_height = (room.y - THUMBNAIL * 0.75 - 40.0)
            .max(room.y * 0.5)
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

        // Every slide, as thumbnails that wrap onto rows.
        egui::ScrollArea::vertical()
            .id_salt("ui designer pages")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    for (index, slide) in slides.iter().enumerate() {
                        let (width, height) = axcenity_ui::design_size(world, slide.gui);
                        let size =
                            egui::vec2(THUMBNAIL, THUMBNAIL * height as f32 / width.max(1) as f32);
                        let response = ui
                            .vertical(|ui| {
                                ui.set_width(THUMBNAIL);
                                let (rect, response) =
                                    ui.allocate_exact_size(size, egui::Sense::click());
                                self.draw_slide(ui, rect, scene, slide, index + 1, render_state);
                                let palette = theme::palette(ui.ctx());
                                let chosen = index == self.slide;
                                if chosen || response.hovered() {
                                    ui.painter().rect_stroke(
                                        rect,
                                        3,
                                        egui::Stroke::new(
                                            if chosen { 2.0_f32 } else { 1.0 },
                                            palette.accent,
                                        ),
                                        egui::StrokeKind::Outside,
                                    );
                                }
                                ui.add(
                                    egui::Label::new(egui::RichText::new(&slide.title).small())
                                        .truncate(),
                                );
                                response
                            })
                            .inner
                            .on_hover_text(format!(
                                "{}: click to show it above, double-click to edit it",
                                slide.title
                            ));
                        if response.clicked() {
                            self.slide = index;
                        }
                        if response.double_clicked() {
                            edit = Some(index);
                        }
                    }
                });
            });

        if let Some(index) = edit {
            self.edit_slide(world, &slides[index], actions);
        }
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
