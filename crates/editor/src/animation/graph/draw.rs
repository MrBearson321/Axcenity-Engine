//! How the state machine graph looks: nodes and pins like the script Graph
//! view's, Bezier wires, and the labels on them.

use axcenity_anim::files::{Motion, TransitionDef};
use axcenity_anim::{ANY_STATE, StateMachineFile};
use eframe::egui::{self, Color32, CornerRadius, FontId, Pos2, Rect, Shape, Stroke, Vec2, vec2};

use super::{Node, ParameterKind, STATE, words};
use crate::theme::icons;

/// A node's title band, at zoom 1.
pub(super) const HEADER: f32 = 24.0;
const PIN: f32 = 5.0;

pub(super) const SELECTED: Color32 = Color32::from_rgb(255, 176, 46);
pub(super) const LIVE: Color32 = Color32::from_rgb(90, 210, 120);
pub(super) const WIRE: Color32 = Color32::from_rgb(200, 204, 214);
pub(super) const ENTRY_WIRE: Color32 = Color32::from_rgb(110, 190, 130);
const NODE_BODY: Color32 = Color32::from_rgb(30, 32, 40);
const NODE_EDGE: Color32 = Color32::from_rgb(12, 13, 17);
const PIN_TEXT: Color32 = Color32::from_rgb(214, 218, 228);
const LABEL_FILL: Color32 = Color32::from_rgb(40, 43, 54);
const NEVER: Color32 = Color32::from_rgb(236, 150, 90);
const DEFAULT_HEADER: Color32 = Color32::from_rgb(196, 118, 40);
const STATE_HEADER: Color32 = Color32::from_rgb(64, 92, 140);
const ANY_HEADER: Color32 = Color32::from_rgb(40, 140, 150);
const ENTRY_HEADER: Color32 = Color32::from_rgb(58, 140, 82);

/// The grid behind the graph, so panning reads as movement.
pub(super) fn draw_grid(
    painter: &egui::Painter,
    rect: Rect,
    offset: Vec2,
    zoom: f32,
    visuals: &egui::Visuals,
) {
    let step = 40.0 * zoom;
    let grid = Stroke::new(1.0_f32, visuals.weak_text_color().gamma_multiply(0.15));
    let mut x = rect.left() + offset.x.rem_euclid(step);
    while x < rect.right() {
        painter.line_segment(
            [Pos2::new(x, rect.top()), Pos2::new(x, rect.bottom())],
            grid,
        );
        x += step;
    }
    let mut y = rect.top() + offset.y.rem_euclid(step);
    while y < rect.bottom() {
        painter.line_segment(
            [Pos2::new(rect.left(), y), Pos2::new(rect.right(), y)],
            grid,
        );
        y += step;
    }
}

/// A wire from an output pin at `start` to an input pin at `end`, leaving
/// and arriving sideways, bowed `bow` points down (up if negative).
pub(super) fn wire_points(start: Pos2, end: Pos2, bow: f32, zoom: f32) -> [Pos2; 4] {
    let reach = ((end.x - start.x).abs() * 0.5).max(50.0 * zoom);
    [
        start,
        start + vec2(reach, bow * 4.0 / 3.0),
        end + vec2(-reach, bow * 4.0 / 3.0),
        end,
    ]
}

/// The point `t` (0 to 1) along a cubic Bezier.
pub(super) fn bezier_at([a, b, c, d]: [Pos2; 4], t: f32) -> Pos2 {
    let u = 1.0 - t;
    let mix = |a: f32, b: f32, c: f32, d: f32| {
        u * u * u * a + 3.0 * u * u * t * b + 3.0 * u * t * t * c + t * t * t * d
    };
    Pos2::new(mix(a.x, b.x, c.x, d.x), mix(a.y, b.y, c.y, d.y))
}

/// A wire as points along it, for drawing dashes and finding the nearest.
pub(super) fn sample_wire(points: [Pos2; 4]) -> Vec<Pos2> {
    (0..=32)
        .map(|step| bezier_at(points, step as f32 / 32.0))
        .collect()
}

/// The wire nearest `point`, and how far it is.
pub(super) fn nearest_wire(
    curves: &[(usize, [Pos2; 4], Vec<Pos2>)],
    point: Pos2,
) -> Option<(usize, f32)> {
    curves
        .iter()
        .map(|(index, _, samples)| {
            let distance = samples
                .windows(2)
                .map(|pair| distance_to_segment(point, pair[0], pair[1]))
                .fold(f32::INFINITY, f32::min);
            (*index, distance)
        })
        .min_by(|x, y| x.1.total_cmp(&y.1))
}

fn distance_to_segment(point: Pos2, a: Pos2, b: Pos2) -> f32 {
    let ab = b - a;
    let t = ((point - a).dot(ab) / ab.length_sq().max(1e-6)).clamp(0.0, 1.0);
    (a + ab * t).distance(point)
}

pub(super) fn draw_wire(
    painter: &egui::Painter,
    points: [Pos2; 4],
    samples: &[Pos2],
    color: Color32,
    width: f32,
    dashed: bool,
) {
    let stroke = Stroke::new(width, color);
    if dashed {
        painter.extend(Shape::dashed_line(samples, stroke, 8.0, 5.0));
    } else {
        painter.add(egui::epaint::CubicBezierShape::from_points_stroke(
            points,
            false,
            Color32::TRANSPARENT,
            stroke,
        ));
    }
}

/// What `draw_node` needs about one node.
pub(super) struct NodeLook<'a> {
    pub node: Node,
    pub file: &'a StateMachineFile,
    pub rect: Rect,
    pub zoom: f32,
    pub selected: bool,
    pub hovered: bool,
    /// How far through its pass the state playing in the preview is.
    pub playing: Option<f32>,
    pub wired_in: bool,
    pub wired_out: bool,
}

/// What a state plays, in a few words: its clip's name, or "blend tree ·
/// Speed".
pub(super) fn plays(motion: &Motion) -> String {
    match motion {
        Motion::Clip(path) if path.is_empty() => "(no clip)".to_owned(),
        Motion::Clip(path) => axcenity_anim::files::clip_name(path).to_owned(),
        Motion::BlendTree(tree) => format!("blend tree · {}", tree.parameter),
        Motion::BlendSpace(space) => {
            format!(
                "blend space · {} × {}",
                space.parameter_x, space.parameter_y
            )
        }
    }
}

pub(super) fn draw_node(painter: &egui::Painter, look: NodeLook) {
    let NodeLook {
        node,
        file,
        rect,
        zoom: z,
        ..
    } = look;
    let corner = 6.0 * z;
    let title_font = FontId::proportional(13.0 * z.clamp(0.7, 1.5));
    let body_font = FontId::proportional(11.0 * z.clamp(0.7, 1.5));
    painter.rect_filled(
        rect.translate(vec2(0.0, 3.0 * z)),
        corner,
        Color32::from_black_alpha(70),
    );
    let (header, title) = match node {
        Node::State(index) => {
            let state = &file.states[index];
            let default = file
                .starting_state()
                .is_some_and(|start| start.name == state.name);
            (
                if default {
                    DEFAULT_HEADER
                } else {
                    STATE_HEADER
                },
                state.name.clone(),
            )
        }
        Node::Any => (ANY_HEADER, format!("{} {ANY_STATE}", icons::ASTERISK)),
        Node::Entry => (ENTRY_HEADER, format!("{} Entry", icons::SIGN_IN)),
    };
    let clipped = painter.with_clip_rect(painter.clip_rect().intersect(rect.expand(2.0)));
    match node {
        Node::State(index) => {
            painter.rect_filled(rect, corner, NODE_BODY);
            let band = Rect::from_min_size(rect.min, vec2(rect.width(), HEADER * z));
            let r = corner.round().clamp(0.0, 255.0) as u8;
            painter.rect_filled(
                band,
                CornerRadius {
                    nw: r,
                    ne: r,
                    sw: 0,
                    se: 0,
                },
                header,
            );
            clipped.text(
                Pos2::new(band.left() + 10.0 * z, band.center().y),
                egui::Align2::LEFT_CENTER,
                &title,
                title_font,
                Color32::WHITE,
            );
            let body_y = band.bottom() + (rect.bottom() - band.bottom()) * 0.5;
            let icon = match &file.states[index].motion {
                Motion::Clip(_) => icons::FILM_STRIP,
                Motion::BlendTree(_) => icons::SLIDERS_HORIZONTAL,
                Motion::BlendSpace(_) => icons::GRID_FOUR,
            };
            clipped.text(
                Pos2::new(rect.left() + 22.0 * z, body_y),
                egui::Align2::LEFT_CENTER,
                format!("{icon} {}", plays(&file.states[index].motion)),
                body_font,
                PIN_TEXT,
            );
            if let Some(progress) = look.playing {
                let bar = Rect::from_min_size(
                    Pos2::new(rect.left(), rect.bottom() - 3.0 * z),
                    vec2(rect.width() * progress.clamp(0.0, 1.0), 3.0 * z),
                );
                painter.rect_filled(bar, 0.0, LIVE);
            }
            pin(
                painter,
                Pos2::new(rect.left() + super::PIN_INSET * z, body_y),
                z,
                look.wired_in,
            );
            pin(
                painter,
                Pos2::new(rect.right() - super::PIN_INSET * z, body_y),
                z,
                look.wired_out,
            );
        }
        Node::Any | Node::Entry => {
            painter.rect_filled(rect, corner, header);
            clipped.text(
                Pos2::new(rect.left() + 10.0 * z, rect.center().y),
                egui::Align2::LEFT_CENTER,
                &title,
                body_font,
                Color32::WHITE,
            );
            pin(
                painter,
                Pos2::new(rect.right() - super::PIN_INSET * z, rect.center().y),
                z,
                look.wired_out,
            );
        }
    }
    let outline = if look.selected {
        Stroke::new(2.0 * z.max(0.8), SELECTED)
    } else if look.playing.is_some() {
        Stroke::new(2.0 * z.max(0.8), LIVE)
    } else if look.hovered {
        Stroke::new(1.5_f32, Color32::WHITE.gamma_multiply(0.6))
    } else {
        Stroke::new(1.0_f32, NODE_EDGE)
    };
    painter.rect_stroke(rect, corner, outline, egui::StrokeKind::Outside);
}

/// Where a state's pins sit up and down: the middle of its body, under
/// the title.
pub(super) fn state_pin_y(rect: Rect, zoom: f32) -> f32 {
    rect.top() + (HEADER + (STATE.y - HEADER) * 0.5) * zoom
}

/// A pin: a circle, filled when it's wired.
fn pin(painter: &egui::Painter, at: Pos2, z: f32, wired: bool) {
    let radius = PIN * z;
    if wired {
        painter.circle_filled(at, radius, PIN_TEXT);
    } else {
        painter.circle(
            at,
            radius,
            NODE_BODY,
            Stroke::new(1.5 * z.max(0.7), PIN_TEXT),
        );
    }
}

/// A wire's label at `at`: when its transition happens. Returns where it
/// was drawn.
pub(super) fn draw_label(
    painter: &egui::Painter,
    at: Pos2,
    transition: &TransitionDef,
    z: f32,
    selected: bool,
) -> Rect {
    let never = words::never(transition);
    let color = if never {
        NEVER
    } else if transition.generated {
        PIN_TEXT.gamma_multiply(0.7)
    } else {
        PIN_TEXT
    };
    let galley = painter.layout_no_wrap(
        words::when_words(transition),
        FontId::proportional(11.0 * z.clamp(0.7, 1.4)),
        color,
    );
    let rect = Rect::from_center_size(at, galley.size() + vec2(12.0, 6.0) * z.clamp(0.7, 1.4));
    painter.rect_filled(rect, rect.height() * 0.5, LABEL_FILL);
    let edge = if selected {
        Stroke::new(1.5_f32, SELECTED)
    } else if never {
        Stroke::new(1.0_f32, NEVER)
    } else {
        Stroke::new(1.0_f32, NODE_EDGE)
    };
    painter.rect_stroke(rect, rect.height() * 0.5, edge, egui::StrokeKind::Outside);
    painter.galley(rect.center() - galley.size() * 0.5, galley, color);
    rect
}

/// A parameter chip: its type's icon and its name in a pill.
/// A parameter chip's room left and right of its name.
pub(super) const CHIP_MARGIN_X: f32 = 8.0;

pub(super) fn draw_chip(ui: &mut egui::Ui, kind: ParameterKind, name: &str) {
    let color = kind.color();
    egui::Frame::new()
        .fill(color.gamma_multiply(0.18))
        .stroke(Stroke::new(1.0_f32, color))
        .corner_radius(10)
        .inner_margin(egui::Margin::symmetric(CHIP_MARGIN_X as i8, 3))
        .show(ui, |ui| {
            let text = egui::RichText::new(format!("{} {name}", kind.icon()))
                .color(ui.visuals().text_color());
            // Always one line: the row it's in starts a new one first if
            // it doesn't fit (`parameters_panel`).
            ui.add(
                egui::Label::new(text)
                    .selectable(false)
                    .wrap_mode(egui::TextWrapMode::Extend),
            );
        });
}

/// One line of help at the graph's top left.
pub(super) fn hint(painter: &egui::Painter, rect: Rect, text: &str) {
    painter.text(
        rect.left_top() + vec2(10.0, 10.0),
        egui::Align2::LEFT_TOP,
        text,
        FontId::proportional(12.0),
        SELECTED,
    );
}

/// A graph shorter than this shows only the guide's next step.
const GUIDE_IN_FULL: f32 = 260.0;

/// The three steps to a working state machine, shown until it has a
/// transition, each ticked once it's done.
pub(super) fn draw_guide(painter: &egui::Painter, rect: Rect, file: &StateMachineFile) {
    let steps = [
        (
            !file.states.is_empty(),
            "Add states: right-click the graph, or drop a clip on it.",
        ),
        (
            !file.transitions.is_empty(),
            "Wire them: drag from a state's right pin to another state.",
        ),
        (false, "Say when: click the label on the wire."),
    ];
    // At the top right, where the first states usually aren't, wrapped to
    // fit a narrow graph.
    let width = (rect.width() - 20.0).clamp(80.0, 300.0);
    let wrap = width - 20.0;
    let font = FontId::proportional(12.0);
    // A short graph shows only the next step, so the card doesn't cover it.
    let short = rect.height() < GUIDE_IN_FULL;
    let next = steps.iter().position(|(done, _)| !done).unwrap_or(2);
    let title = painter.layout(
        if short {
            format!("Step {} of 3", next + 1)
        } else {
            "Make it work in three steps".to_owned()
        },
        FontId::proportional(12.5),
        Color32::WHITE,
        wrap,
    );
    let galleys: Vec<_> = steps
        .iter()
        .enumerate()
        .filter(|(n, _)| !short || *n == next)
        .map(|(n, (done, text))| {
            let mark = if *done {
                icons::CHECK.to_owned()
            } else {
                format!("{}", n + 1)
            };
            let color = if *done {
                PIN_TEXT.gamma_multiply(0.6)
            } else {
                PIN_TEXT
            };
            // The short card's title already numbers its one step.
            let line = if short {
                (*text).to_owned()
            } else {
                format!("{mark}  {text}")
            };
            painter.layout(line, font.clone(), color, wrap)
        })
        .collect();
    let height = title.size().y + galleys.iter().map(|g| g.size().y + 4.0).sum::<f32>() + 22.0;
    let panel = Rect::from_min_size(
        Pos2::new(rect.right() - width - 10.0, rect.top() + 10.0),
        vec2(width, height),
    );
    painter.rect_filled(panel, 6.0, LABEL_FILL.gamma_multiply(0.85));
    let mut y = panel.top() + 8.0;
    y += title.size().y + 6.0;
    painter.galley(
        Pos2::new(panel.left() + 10.0, panel.top() + 8.0),
        title,
        Color32::WHITE,
    );
    for galley in galleys {
        let height = galley.size().y;
        painter.galley(Pos2::new(panel.left() + 10.0, y), galley, PIN_TEXT);
        y += height + 4.0;
    }
}
