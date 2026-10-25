//! The tutorials' pictures: small drawings of the editor and the game,
//! painted with egui so they follow the theme and accent and stay sharp at
//! any scale. Scene content (gizmo axes, parts, blocks, the game's screen)
//! keeps its own colours, as it does in the editor.

use eframe::egui::{self, Color32, FontId, Pos2, Rect, Shape, Stroke, Vec2, pos2, vec2};

use crate::lua_syntax;
use crate::theme::{self, Palette, icons, type_scale};

/// The widest a picture is drawn, in points.
const MAX_WIDTH: f32 = 620.0;
const RADIUS: u8 = 8;
/// The built-in ribbon tabs, in order.
const TABS: [&str; 7] = ["Home", "Model", "Terrain", "UI", "Script", "Test", "View"];
/// Gizmo axis colours, as the Move tool draws them.
const AXIS_X: Color32 = Color32::from_rgb(230, 72, 72);
const AXIS_Y: Color32 = Color32::from_rgb(96, 200, 96);
const AXIS_Z: Color32 = Color32::from_rgb(72, 132, 240);
const SELECTION: Color32 = Color32::from_rgb(25, 160, 255);

#[derive(Clone, Copy, Debug)]
pub(in crate::start) enum Picture {
    /// The start screen's Projects page, with Open Project under the pointer.
    StartScreen,
    /// A ribbon tab with one of its buttons under the pointer.
    Ribbon {
        tab: &'static str,
        buttons: &'static [(&'static str, &'static str)],
        pressed: usize,
    },
    /// Rows of a tree (depth, icon, name) under a title such as
    /// "Explorer", one of them selected.
    Tree {
        title: &'static str,
        rows: &'static [(u8, &'static str, &'static str)],
        selected: usize,
    },
    /// A script tab.
    Code {
        file: &'static str,
        code: &'static str,
    },
    /// Lines in the Console.
    Console(&'static [&'static str]),
    /// A selected block with the Move tool's arrows.
    MoveHandles,
    /// Steps (icon, words) joined by arrows.
    Flow(&'static [(&'static str, &'static str)]),
    /// A ball landing on a pad that lights up.
    Touch,
    /// A block script: an event with two blocks under it.
    Blocks,
    /// The game's screen, with a menu whose button is under the pointer.
    Menu,
}

/// Draws `picture` at the width there is, up to [`MAX_WIDTH`].
pub(in crate::start) fn draw(ui: &mut egui::Ui, picture: Picture) {
    let palette = theme::palette(ui.ctx());
    let width = ui.available_width().min(MAX_WIDTH);
    // Code is measured first, since its height is the code's.
    let code = match picture {
        Picture::Code { code, .. } => Some(lua_syntax::layout_job(
            code,
            FontId::monospace(12.0),
            ui.visuals(),
        )),
        _ => None,
    }
    .map(|job| ui.painter().layout_job(job));
    let height = match picture {
        Picture::StartScreen => 150.0,
        Picture::Ribbon { .. } => 112.0,
        Picture::Tree { rows, .. } => 40.0 + rows.len() as f32 * 22.0,
        Picture::Code { .. } => 56.0 + code.as_ref().map_or(0.0, |galley| galley.size().y),
        Picture::Console(lines) => 44.0 + lines.len() as f32 * 17.0,
        Picture::MoveHandles => 190.0,
        Picture::Flow(_) => 112.0,
        Picture::Touch => 160.0,
        Picture::Blocks => 150.0,
        Picture::Menu => 180.0,
    };
    let (rect, response) = ui.allocate_exact_size(vec2(width, height), egui::Sense::hover());
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Image, true, "Tutorial picture")
    });
    let painter = ui.painter_at(rect);
    painter.rect(
        rect,
        RADIUS,
        palette.inset,
        Stroke::new(1.0_f32, palette.border),
        egui::StrokeKind::Inside,
    );
    let pen = Pen {
        painter: &painter,
        palette: &palette,
    };
    match picture {
        Picture::StartScreen => pen.start_screen(rect),
        Picture::Ribbon {
            tab,
            buttons,
            pressed,
        } => pen.ribbon(rect, tab, buttons, pressed),
        Picture::Tree {
            title,
            rows,
            selected,
        } => pen.tree(rect, title, rows, selected),
        Picture::Code { file, .. } => {
            if let Some(galley) = code {
                pen.code(rect, file, galley);
            }
        }
        Picture::Console(lines) => pen.console(rect, lines),
        Picture::MoveHandles => pen.move_handles(rect),
        Picture::Flow(steps) => pen.flow(rect, steps),
        Picture::Touch => pen.touch(rect),
        Picture::Blocks => pen.blocks(rect),
        Picture::Menu => pen.menu(rect),
    }
}

/// A painter clipped to the picture, and the theme's colours.
struct Pen<'a> {
    painter: &'a egui::Painter,
    palette: &'a Palette,
}

impl Pen<'_> {
    fn text(&self, at: Pos2, align: egui::Align2, text: &str, size: f32, color: Color32) -> Rect {
        self.painter
            .text(at, align, text, FontId::proportional(size), color)
    }

    fn medium(&self, at: Pos2, align: egui::Align2, text: &str, size: f32, color: Color32) -> Rect {
        self.painter.text(
            at,
            align,
            text,
            FontId::new(size, theme::medium_family()),
            color,
        )
    }

    /// The mouse pointer, clicking near the bottom right of `target`, so
    /// it covers little of its words.
    fn click(&self, target: Rect) {
        self.pointer(target.right_bottom() - vec2(9.0, 7.0));
    }

    /// The mouse pointer, its tip at `tip`.
    fn pointer(&self, tip: Pos2) {
        let points = vec![tip, tip + vec2(0.0, 17.0), tip + vec2(12.0, 12.0)];
        self.painter.add(Shape::convex_polygon(
            points,
            Color32::WHITE,
            Stroke::new(1.2_f32, Color32::from_gray(20)),
        ));
    }

    /// A caption at the top left, like a panel's title.
    fn header(&self, rect: Rect, icon: &str, title: &str) {
        self.text(
            rect.left_top() + vec2(14.0, 16.0),
            egui::Align2::LEFT_CENTER,
            &theme::with_icon(icon, title),
            type_scale::CAPTION,
            self.palette.text_weak,
        );
    }

    /// An arrow from `from` to `to`, its head at `to`.
    fn arrow(&self, from: Pos2, to: Pos2, width: f32, color: Color32) {
        let along = (to - from).normalized();
        let side = along.rot90();
        let head = 4.0 + width * 2.0;
        self.painter
            .line_segment([from, to - along * head * 0.8], Stroke::new(width, color));
        self.painter.add(Shape::convex_polygon(
            vec![
                to,
                to - along * head + side * head * 0.6,
                to - along * head - side * head * 0.6,
            ],
            color,
            Stroke::NONE,
        ));
    }

    fn start_screen(&self, rect: Rect) {
        let palette = self.palette;
        let sidebar = Rect::from_min_size(rect.min, vec2(130.0, rect.height()));
        self.painter.rect_filled(
            sidebar.shrink(1.0),
            egui::CornerRadius {
                nw: RADIUS - 1,
                sw: RADIUS - 1,
                ..Default::default()
            },
            palette.app_bg,
        );
        self.painter.vline(
            sidebar.right(),
            rect.y_range(),
            Stroke::new(1.0_f32, palette.border),
        );
        for (row, (icon, name)) in [
            (icons::SQUARES_FOUR, "Projects"),
            (icons::BOOK_OPEN, "Learn"),
            (icons::BOOKS, "API Reference"),
        ]
        .into_iter()
        .enumerate()
        {
            let row_rect = Rect::from_min_size(
                pos2(sidebar.left() + 8.0, rect.top() + 18.0 + row as f32 * 26.0),
                vec2(sidebar.width() - 16.0, 22.0),
            );
            if row == 0 {
                self.painter
                    .rect_filled(row_rect, 5, palette.control_active);
            }
            self.text(
                row_rect.left_center() + vec2(8.0, 0.0),
                egui::Align2::LEFT_CENTER,
                &theme::with_icon(icon, name),
                type_scale::SMALL,
                if row == 0 {
                    palette.text_strong
                } else {
                    palette.text_secondary
                },
            );
        }
        let left = sidebar.right() + 20.0;
        self.painter.text(
            pos2(left, rect.top() + 30.0),
            egui::Align2::LEFT_CENTER,
            "Projects",
            FontId::new(type_scale::HEADING + 2.0, theme::heading_family()),
            palette.text_strong,
        );
        let right = rect.right() - 16.0;
        let open = Rect::from_min_max(
            pos2(right - 104.0, rect.top() + 18.0),
            pos2(right, rect.top() + 42.0),
        );
        let new = open.translate(vec2(-112.0, 0.0));
        self.painter.rect(
            new,
            6,
            palette.control,
            Stroke::new(1.0_f32, palette.border),
            egui::StrokeKind::Inside,
        );
        self.medium(
            new.center(),
            egui::Align2::CENTER_CENTER,
            &theme::with_icon(icons::PLUS, "New Project"),
            type_scale::SMALL,
            palette.text,
        );
        self.painter.rect_filled(open, 6, palette.accent_solid);
        self.medium(
            open.center(),
            egui::Align2::CENTER_CENTER,
            &theme::with_icon(icons::FOLDER_OPEN, "Open Project"),
            type_scale::SMALL,
            palette.on_accent,
        );
        // Recent projects, as quiet cards.
        let top = rect.top() + 62.0;
        let card_width = ((right - left) - 2.0 * 12.0) / 3.0;
        for column in 0..3 {
            let card = Rect::from_min_size(
                pos2(left + column as f32 * (card_width + 12.0), top),
                vec2(card_width, rect.bottom() - 16.0 - top),
            );
            self.painter.rect(
                card,
                6,
                palette.surface,
                Stroke::new(1.0_f32, palette.border),
                egui::StrokeKind::Inside,
            );
            let picture = Rect::from_min_max(
                card.min + vec2(6.0, 6.0),
                pos2(card.right() - 6.0, card.bottom() - 26.0),
            );
            self.painter.rect_filled(picture, 4, palette.control);
            self.painter.rect_filled(
                Rect::from_min_size(
                    pos2(card.left() + 8.0, card.bottom() - 17.0),
                    vec2(card_width * 0.5, 6.0),
                ),
                3,
                palette.border_strong,
            );
        }
        self.click(open);
    }

    fn ribbon(&self, rect: Rect, tab: &str, buttons: &[(&str, &str)], pressed: usize) {
        let palette = self.palette;
        // The tab row.
        let mut x = rect.left() + 16.0;
        let row_y = rect.top() + 18.0;
        for name in TABS {
            let label = self.text(
                pos2(x, row_y),
                egui::Align2::LEFT_CENTER,
                name,
                type_scale::SMALL,
                if name == tab {
                    palette.text_strong
                } else {
                    palette.text_weak
                },
            );
            if name == tab {
                self.painter.hline(
                    label.x_range(),
                    label.bottom() + 3.0,
                    Stroke::new(2.0_f32, palette.accent),
                );
            }
            x = label.right() + 16.0;
        }
        // The ribbon under it, with the buttons.
        let band = Rect::from_min_max(
            pos2(rect.left() + 1.0, rect.top() + 34.0),
            pos2(rect.right() - 1.0, rect.bottom() - 10.0),
        );
        self.painter.rect_filled(band, 0, palette.panel);
        for y in [band.top(), band.bottom()] {
            self.painter
                .hline(band.x_range(), y, Stroke::new(1.0_f32, palette.border));
        }
        let size = vec2(62.0, 56.0);
        for (index, (icon, label)) in buttons.iter().enumerate() {
            let button = Rect::from_min_size(
                pos2(
                    band.left() + 12.0 + index as f32 * (size.x + 4.0),
                    band.top() + 6.0,
                ),
                size,
            );
            let held = index == pressed;
            if held {
                self.painter.rect(
                    button,
                    6,
                    palette.accent_soft,
                    Stroke::new(1.0_f32, palette.accent),
                    egui::StrokeKind::Inside,
                );
            }
            let color = match *icon {
                icons::PLAY => palette.success,
                icons::STOP => palette.error,
                _ if held => palette.accent_text,
                _ => palette.text_secondary,
            };
            self.text(
                button.center_top() + vec2(0.0, 20.0),
                egui::Align2::CENTER_CENTER,
                icon,
                20.0,
                color,
            );
            self.text(
                button.center_top() + vec2(0.0, 44.0),
                egui::Align2::CENTER_CENTER,
                label,
                type_scale::CAPTION,
                if held {
                    palette.text_strong
                } else {
                    palette.text_secondary
                },
            );
            if held {
                self.click(button);
            }
        }
    }

    fn tree(&self, rect: Rect, title: &str, rows: &[(u8, &str, &str)], selected: usize) {
        let palette = self.palette;
        self.header(rect, icons::TREE_STRUCTURE, title);
        for (index, (depth, icon, name)) in rows.iter().enumerate() {
            let row = Rect::from_min_size(
                pos2(rect.left() + 8.0, rect.top() + 30.0 + index as f32 * 22.0),
                vec2(rect.width() - 16.0, 22.0),
            );
            let chosen = index == selected;
            if chosen {
                self.painter.rect_filled(row, 5, palette.accent_soft);
            }
            let left = row.left() + 10.0 + f32::from(*depth) * 18.0;
            // Indent guides, like the Explorer's.
            for level in 0..*depth {
                let x = row.left() + 15.0 + f32::from(level) * 18.0;
                self.painter
                    .vline(x, row.y_range(), Stroke::new(1.0_f32, palette.border));
            }
            self.text(
                pos2(left, row.center().y),
                egui::Align2::LEFT_CENTER,
                icon,
                14.0,
                if chosen {
                    palette.accent_text
                } else {
                    palette.text_secondary
                },
            );
            self.text(
                pos2(left + 20.0, row.center().y),
                egui::Align2::LEFT_CENTER,
                name,
                type_scale::SMALL + 0.5,
                if chosen {
                    palette.text_strong
                } else {
                    palette.text
                },
            );
        }
    }

    fn code(&self, rect: Rect, file: &str, galley: std::sync::Arc<egui::Galley>) {
        let palette = self.palette;
        // The tab, joined to the code under it.
        let tab_title = theme::with_icon(icons::FILE_CODE, file);
        let title = self.painter.layout_no_wrap(
            tab_title,
            FontId::proportional(type_scale::SMALL),
            palette.text_strong,
        );
        let tab = Rect::from_min_size(
            rect.left_top() + vec2(10.0, 8.0),
            vec2(title.size().x + 24.0, 24.0),
        );
        self.painter.rect_filled(
            tab,
            egui::CornerRadius {
                nw: 5,
                ne: 5,
                ..Default::default()
            },
            palette.panel,
        );
        self.painter.hline(
            tab.x_range(),
            tab.top(),
            Stroke::new(2.0_f32, palette.accent),
        );
        self.painter.galley(
            tab.left_center() + vec2(12.0, -title.size().y / 2.0),
            title,
            palette.text_strong,
        );
        let body = Rect::from_min_max(
            pos2(rect.left() + 1.0, tab.bottom()),
            pos2(rect.right() - 1.0, rect.bottom() - 1.0),
        );
        self.painter.rect_filled(
            body,
            egui::CornerRadius {
                sw: RADIUS - 1,
                se: RADIUS - 1,
                ..Default::default()
            },
            palette.panel,
        );
        let lines = galley.job.text.lines().count().max(1);
        let numbers = (1..=lines)
            .map(|n| n.to_string())
            .collect::<Vec<_>>()
            .join("\n");
        let top = body.top() + 12.0;
        let numbers =
            self.painter
                .layout_no_wrap(numbers, FontId::monospace(12.0), palette.text_muted);
        let gutter = body.left() + 34.0;
        self.painter.galley(
            pos2(gutter - 8.0 - numbers.size().x, top),
            numbers,
            palette.text_muted,
        );
        self.painter
            .galley(pos2(gutter + 8.0, top), galley, palette.text);
    }

    fn console(&self, rect: Rect, lines: &[&str]) {
        let palette = self.palette;
        self.header(rect, icons::TERMINAL_WINDOW, "Console");
        for (index, line) in lines.iter().enumerate() {
            let y = rect.top() + 36.0 + index as f32 * 17.0;
            self.text(
                pos2(rect.left() + 14.0, y),
                egui::Align2::LEFT_CENTER,
                icons::INFO,
                type_scale::SMALL,
                palette.text_weak,
            );
            self.painter.text(
                pos2(rect.left() + 32.0, y),
                egui::Align2::LEFT_CENTER,
                *line,
                FontId::monospace(12.0),
                palette.text,
            );
        }
    }

    fn move_handles(&self, rect: Rect) {
        let palette = self.palette;
        let scale = 17.0;
        let center = rect.center() + vec2(-20.0, 18.0);
        // A simple isometric view: +X goes right and down, +Z left and down.
        let at = |x: f32, y: f32, z: f32| center + vec2((x - z) * 0.87, (x + z) * 0.5 - y) * scale;
        let (hx, hy, hz) = (2.0, 0.5, 1.0);
        // The floor grid.
        for step in -6..=6 {
            let s = step as f32;
            let grid = Stroke::new(1.0_f32, palette.border);
            self.painter
                .line_segment([at(s, -hy, -6.0), at(s, -hy, 6.0)], grid);
            self.painter
                .line_segment([at(-6.0, -hy, s), at(6.0, -hy, s)], grid);
        }
        // The block's three faces that face the camera, shaded.
        let part = Color32::from_rgb(196, 70, 60);
        let shade = |light: f32| theme::mix(Color32::BLACK, part, light);
        let faces = [
            (
                [
                    at(-hx, hy, -hz),
                    at(hx, hy, -hz),
                    at(hx, hy, hz),
                    at(-hx, hy, hz),
                ],
                1.0,
            ),
            (
                [
                    at(hx, hy, -hz),
                    at(hx, -hy, -hz),
                    at(hx, -hy, hz),
                    at(hx, hy, hz),
                ],
                0.78,
            ),
            (
                [
                    at(-hx, hy, hz),
                    at(hx, hy, hz),
                    at(hx, -hy, hz),
                    at(-hx, -hy, hz),
                ],
                0.62,
            ),
        ];
        for (corners, light) in faces {
            self.painter.add(Shape::convex_polygon(
                corners.to_vec(),
                shade(light),
                Stroke::NONE,
            ));
        }
        // The selection box, every edge.
        let corner = |i: usize| {
            at(
                if i & 1 == 0 { -hx } else { hx },
                if i & 2 == 0 { -hy } else { hy },
                if i & 4 == 0 { -hz } else { hz },
            )
        };
        for a in 0..8 {
            for bit in [1, 2, 4] {
                if a & bit == 0 {
                    self.painter.line_segment(
                        [corner(a), corner(a | bit)],
                        Stroke::new(1.2_f32, SELECTION),
                    );
                }
            }
        }
        // The Move tool's arrows, one per world axis.
        let origin = at(0.0, 0.0, 0.0);
        let tips = [
            (at(4.4, 0.0, 0.0), AXIS_X, "X"),
            (at(0.0, 4.0, 0.0), AXIS_Y, "Y"),
            (at(0.0, 0.0, 4.4), AXIS_Z, "Z"),
        ];
        for (tip, color, name) in tips {
            self.arrow(origin, tip, 3.0, color);
            let away = (tip - origin).normalized();
            self.medium(
                tip + away * 12.0,
                egui::Align2::CENTER_CENTER,
                name,
                type_scale::SMALL,
                color,
            );
        }
        self.pointer(tips[0].0 - (tips[0].0 - origin).normalized() * 8.0);
        self.text(
            rect.left_bottom() + vec2(14.0, -14.0),
            egui::Align2::LEFT_CENTER,
            &theme::with_icon(
                icons::ARROWS_OUT_CARDINAL,
                "Move  ·  Ctrl+2  ·  Snap 1 stud",
            ),
            type_scale::CAPTION,
            palette.text_weak,
        );
    }

    fn flow(&self, rect: Rect, steps: &[(&str, &str)]) {
        let palette = self.palette;
        let count = steps.len().max(1) as f32;
        let gap = 34.0;
        let inner = rect.shrink2(vec2(16.0, 16.0));
        let box_width = (inner.width() - gap * (count - 1.0)) / count;
        for (index, (icon, words)) in steps.iter().enumerate() {
            let card = Rect::from_min_size(
                pos2(inner.left() + index as f32 * (box_width + gap), inner.top()),
                vec2(box_width, inner.height()),
            );
            self.painter.rect(
                card,
                6,
                palette.surface,
                Stroke::new(1.0_f32, palette.border),
                egui::StrokeKind::Inside,
            );
            self.text(
                card.center_top() + vec2(0.0, 22.0),
                egui::Align2::CENTER_CENTER,
                icon,
                20.0,
                palette.accent_text,
            );
            let mut job = egui::text::LayoutJob::simple(
                (*words).to_owned(),
                FontId::proportional(type_scale::SMALL),
                palette.text,
                box_width - 14.0,
            );
            job.halign = egui::Align::Center;
            let galley = self.painter.layout_job(job);
            // A centred galley is laid out around x = 0.
            self.painter.galley(
                pos2(card.center().x, card.top() + 40.0),
                galley,
                palette.text,
            );
            if index + 1 < steps.len() {
                let y = card.center().y;
                self.arrow(
                    pos2(card.right() + 6.0, y),
                    pos2(card.right() + gap - 6.0, y),
                    1.5,
                    palette.text_weak,
                );
            }
        }
    }

    fn touch(&self, rect: Rect) {
        let palette = self.palette;
        let floor_y = rect.bottom() - 34.0;
        self.painter.rect_filled(
            Rect::from_min_max(
                pos2(rect.left() + 1.0, floor_y),
                rect.max - Vec2::splat(1.0),
            ),
            egui::CornerRadius {
                sw: RADIUS - 1,
                se: RADIUS - 1,
                ..Default::default()
            },
            palette.control,
        );
        let middle = rect.center().x - 30.0;
        let pad = Rect::from_center_size(pos2(middle, floor_y - 5.0), vec2(130.0, 10.0));
        let green = Color32::from_rgb(80, 220, 100);
        // A soft glow: the pad is Neon for a moment.
        for (grow, alpha) in [(14.0, 0.08), (8.0, 0.14), (3.0, 0.25)] {
            self.painter
                .rect_filled(pad.expand(grow), 8, green.gamma_multiply(alpha));
        }
        self.painter.rect_filled(pad, 2, green);
        // The ball, where it was a moment ago, and now.
        let red = Color32::from_rgb(220, 64, 52);
        let radius = 14.0;
        let ball = pos2(middle, pad.top() - radius);
        for (rise, alpha) in [(66.0, 0.15), (36.0, 0.3)] {
            self.painter
                .circle_filled(ball - vec2(0.0, rise), radius, red.gamma_multiply(alpha));
        }
        self.painter.circle_filled(ball, radius, red);
        // The contact.
        for (dx, dy) in [(-1.0, -0.6), (1.0, -0.6), (-1.0, 0.1), (1.0, 0.1)] {
            let from = pos2(middle + dx * 26.0, pad.top() + dy * 10.0);
            self.painter.line_segment(
                [from, from + vec2(dx * 12.0, dy * 10.0 - 3.0)],
                Stroke::new(2.0_f32, palette.warning),
            );
        }
        let label = pos2(pad.right() + 34.0, pad.top() - 26.0);
        self.painter.text(
            label,
            egui::Align2::LEFT_CENTER,
            "Pad.Touched",
            FontId::monospace(12.0),
            palette.accent_text,
        );
        self.text(
            label + vec2(0.0, 16.0),
            egui::Align2::LEFT_CENTER,
            "fires with the Ball",
            type_scale::SMALL,
            palette.text_secondary,
        );
    }

    fn blocks(&self, rect: Rect) {
        let palette = self.palette;
        let left = rect.left() + 20.0;
        let mut y = rect.top() + 20.0;
        let event = Color32::from_rgb(232, 178, 44);
        let looks = Color32::from_rgb(138, 92, 220);
        let control = Color32::from_rgb(232, 128, 44);
        let rows = [
            (0.0, event, "when Pad touched", Color32::from_gray(30)),
            (16.0, looks, "set Pad Color to  ■", Color32::WHITE),
            (16.0, control, "wait  0.5  seconds", Color32::WHITE),
        ];
        for (indent, fill, words, ink) in rows {
            let block = Rect::from_min_size(pos2(left + indent, y), vec2(200.0, 30.0));
            self.painter.rect_filled(block, 5, fill);
            // The notch that a block above snaps into.
            self.painter.rect_filled(
                Rect::from_min_size(block.left_top() + vec2(14.0, -3.0), vec2(18.0, 4.0)),
                1,
                fill,
            );
            self.medium(
                block.left_center() + vec2(12.0, 0.0),
                egui::Align2::LEFT_CENTER,
                words,
                type_scale::SMALL,
                ink,
            );
            y = block.bottom() + 4.0;
        }
        // The event's arm, wrapping the blocks inside it.
        self.painter.rect_filled(
            Rect::from_min_max(pos2(left, rect.top() + 46.0), pos2(left + 16.0, y + 10.0)),
            0,
            event,
        );
        self.painter.rect_filled(
            Rect::from_min_size(pos2(left, y + 2.0), vec2(80.0, 12.0)),
            egui::CornerRadius {
                sw: 5,
                se: 5,
                ne: 5,
                ..Default::default()
            },
            event,
        );
        let button = Rect::from_min_size(pos2(left + 250.0, rect.top() + 20.0), vec2(112.0, 28.0));
        if button.right() < rect.right() - 8.0 {
            self.painter.rect(
                button,
                6,
                palette.control,
                Stroke::new(1.0_f32, palette.border),
                egui::StrokeKind::Inside,
            );
            self.medium(
                button.center(),
                egui::Align2::CENTER_CENTER,
                &theme::with_icon(icons::CODE, "View as Lua"),
                type_scale::SMALL,
                palette.text,
            );
            self.click(button);
        }
    }

    fn menu(&self, rect: Rect) {
        let palette = self.palette;
        // The game's screen: sky over a floor.
        let screen = rect.shrink(10.0);
        self.painter
            .rect_filled(screen, 6, Color32::from_rgb(120, 170, 220));
        self.painter.rect_filled(
            Rect::from_min_max(pos2(screen.left(), screen.bottom() - 46.0), screen.max),
            egui::CornerRadius {
                sw: 6,
                se: 6,
                ..Default::default()
            },
            Color32::from_rgb(86, 120, 80),
        );
        let menu = Rect::from_center_size(screen.center(), vec2(210.0, 118.0));
        self.painter
            .rect_filled(menu, 10, Color32::from_rgba_unmultiplied(20, 22, 28, 230));
        self.medium(
            menu.center_top() + vec2(0.0, 22.0),
            egui::Align2::CENTER_CENTER,
            "Main Menu",
            type_scale::HEADING,
            Color32::WHITE,
        );
        let button = Rect::from_center_size(menu.center() + vec2(0.0, 6.0), vec2(120.0, 30.0));
        self.painter.rect_filled(button, 6, palette.accent_solid);
        self.medium(
            button.center(),
            egui::Align2::CENTER_CENTER,
            "Start",
            type_scale::BODY,
            palette.on_accent,
        );
        self.text(
            menu.center_bottom() - vec2(0.0, 18.0),
            egui::Align2::CENTER_CENTER,
            "Button activated!",
            type_scale::SMALL,
            Color32::from_gray(200),
        );
        self.click(button);
    }
}
