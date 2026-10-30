//! Drawing a node: its header, fields, pins and the values typed into
//! them, and its errors; and the wires between nodes.

use super::*;

/// Makes the controls in `ui` draw at `zoom`.
pub(super) fn scale_style(ui: &mut egui::Ui, zoom: f32) {
    let mut style = (**ui.style()).clone();
    for font in style.text_styles.values_mut() {
        font.size *= zoom;
    }
    style.spacing.interact_size *= zoom;
    style.spacing.button_padding *= zoom;
    style.spacing.icon_width *= zoom;
    style.spacing.icon_width_inner *= zoom;
    style.spacing.icon_spacing *= zoom;
    style.spacing.item_spacing *= zoom;
    ui.set_style(style);
}

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

/// What `draw_node` needs about one node.
pub(super) struct NodeDraw<'a> {
    pub(super) spec: &'a NodeSpec,
    pub(super) shape: &'a NodeShape,
    pub(super) rect: Rect,
    pub(super) z: f32,
    pub(super) selected: bool,
    pub(super) interactive: bool,
    pub(super) wire_held: bool,
    /// Its wired pins: name and whether it's an output.
    pub(super) wired: &'a BTreeSet<(String, bool)>,
    pub(super) objects: &'a [GraphObject],
    pub(super) images: &'a [String],
    pub(super) variables: &'a [String],
    pub(super) key: &'a str,
}

pub(super) fn draw_node(
    ui: &mut egui::Ui,
    painter: &egui::Painter,
    draw: NodeDraw,
    node: &mut GraphNode,
    events: &mut Vec<Event>,
) {
    let NodeDraw {
        spec,
        shape,
        rect,
        z,
        selected,
        interactive,
        wire_held,
        wired,
        objects,
        images,
        variables,
        key,
    } = draw;
    let id = egui::Id::new(("node graph node", key, node.id));
    let s = |x: f32| x * z;

    // The node itself: click to select, drag to move. Its controls are
    // made after it, so they take their own clicks.
    if interactive {
        let response = ui
            .interact(rect, id, Sense::click_and_drag())
            .on_hover_cursor(CursorIcon::Grab)
            .on_hover_text(spec.help);
        if response.clicked() {
            let add = ui.input(|i| i.modifiers.shift || i.modifiers.command);
            events.push(Event::Select(node.id, add));
        }
        if response.drag_started_by(egui::PointerButton::Primary) {
            events.push(Event::MoveStart(node.id));
        }
        if response.dragged_by(egui::PointerButton::Primary) {
            events.push(Event::Move(response.drag_delta()));
        }
        if response.drag_stopped() {
            events.push(Event::MoveEnd);
        }
        response.context_menu(|ui| {
            if ui
                .button(theme::with_icon(icons::COPY_SIMPLE, "Duplicate"))
                .clicked()
            {
                events.push(Event::Duplicate(node.id));
                ui.close();
            }
            if ui.button(theme::with_icon(icons::COPY, "Copy")).clicked() {
                events.push(Event::Copy(node.id));
                ui.close();
            }
            if ui
                .button(theme::with_icon(icons::TRASH, "Delete"))
                .clicked()
            {
                events.push(Event::Delete(node.id));
                ui.close();
            }
        });
    }

    let corner = s(6.0);
    let has_errors = !shape.errors.is_empty();
    painter.rect_filled(
        rect.translate(vec2(0.0, s(3.0))),
        corner,
        Color32::from_black_alpha(70),
    );
    painter.rect_filled(rect, corner, NODE_BODY);
    let header = Rect::from_min_size(rect.min, vec2(rect.width(), s(HEADER)));
    let r = corner.round().clamp(0.0, 255.0) as u8;
    painter.rect_filled(
        header,
        CornerRadius {
            nw: r,
            ne: r,
            sw: 0,
            se: 0,
        },
        category_color(spec.category).gamma_multiply(0.9),
    );
    painter.text(
        pos2(header.left() + s(10.0), header.center().y),
        egui::Align2::LEFT_CENTER,
        spec.title,
        FontId::proportional(13.0 * z),
        Color32::WHITE,
    );
    let outline = if has_errors {
        Stroke::new(2.5 * z.max(0.8), ERROR)
    } else if selected {
        Stroke::new(2.0 * z.max(0.8), SELECTED)
    } else {
        Stroke::new(1.0_f32, NODE_EDGE)
    };
    painter.rect_stroke(rect, corner, outline, egui::StrokeKind::Outside);
    if has_errors {
        painter.text(
            pos2(header.right() - s(10.0), header.center().y),
            egui::Align2::RIGHT_CENTER,
            icons::WARNING,
            FontId::proportional(14.0 * z),
            Color32::WHITE,
        );
    }

    let widgets = z >= WIDGETS_ZOOM && interactive;
    let font = FontId::proportional(12.0 * z);
    let mut y = rect.top() + s(HEADER + 4.0);
    // Fields: a label and its control.
    for (field, &field_width) in shape.fields.iter().zip(&shape.field_widths) {
        let middle = y + s(ROW) * 0.5;
        painter.text(
            pos2(rect.left() + s(10.0), middle),
            egui::Align2::LEFT_CENTER,
            field.name,
            font.clone(),
            PIN_TEXT,
        );
        let control = Rect::from_min_size(
            pos2(
                rect.right() - s(field_width + 10.0),
                middle - s(ROW - 6.0) * 0.5,
            ),
            vec2(s(field_width), s(ROW - 6.0)),
        );
        let mut value = node.value(field.name).to_owned();
        if widgets {
            field_control(
                ui,
                control,
                field,
                &mut value,
                (objects, images, variables),
                id.with(field.name),
            );
            set_value(node, field.name, value);
        } else {
            let shown = shown_value(field, &value);
            painter
                .with_clip_rect(painter.clip_rect().intersect(control))
                .text(
                    control.left_center(),
                    egui::Align2::LEFT_CENTER,
                    shown,
                    font.clone(),
                    Color32::from_gray(190),
                );
        }
        y += s(ROW);
    }
    // Pins: inputs on the left (with a box for a typed value while it
    // isn't wired), outputs on the right.
    for (row, (input, output)) in shape.pin_rows.iter().enumerate() {
        let middle = rect.top() + s(shape.row_middle(row));
        if let Some(input) = input {
            let at = pos2(rect.left() + s(PIN + 6.0), middle);
            let connected = wired.contains(&(input.name.to_owned(), false));
            draw_pin(painter, at, z, input.ty, connected);
            let text_x = rect.left() + s(PIN_SPACE);
            painter.text(
                pos2(text_x, middle),
                egui::Align2::LEFT_CENTER,
                input.name,
                font.clone(),
                PIN_TEXT,
            );
            if input.ty != PinType::Exec && input.typed && !connected {
                let left = text_x + text_width(ui.ctx(), input.name, 12.0) * z + s(6.0);
                let mut value = node.value(input.name).to_owned();
                let boxed = Rect::from_min_size(
                    pos2(left, middle - s(ROW - 8.0) * 0.5),
                    vec2(s(value_width(ui.ctx(), input.ty, &value)), s(ROW - 8.0)),
                );
                if widgets {
                    value_control(ui, boxed, input.ty, &mut value, id.with(input.name));
                    set_value(node, input.name, value);
                } else {
                    painter
                        .with_clip_rect(painter.clip_rect().intersect(boxed))
                        .text(
                            boxed.left_center(),
                            egui::Align2::LEFT_CENTER,
                            value,
                            font.clone(),
                            Color32::from_gray(190),
                        );
                }
            }
            if interactive {
                pin_response(
                    ui,
                    painter,
                    PinRef::new(node.id, input.name),
                    false,
                    at,
                    z,
                    wire_held,
                    events,
                    key,
                );
            }
        }
        if let Some(output) = output {
            let at = pos2(rect.right() - s(PIN + 6.0), middle);
            let connected = wired.contains(&(output.name.to_owned(), true));
            draw_pin(painter, at, z, output.ty, connected);
            painter.text(
                pos2(rect.right() - s(PIN_SPACE), middle),
                egui::Align2::RIGHT_CENTER,
                output.name,
                font.clone(),
                PIN_TEXT,
            );
            if interactive {
                pin_response(
                    ui,
                    painter,
                    PinRef::new(node.id, output.name),
                    true,
                    at,
                    z,
                    wire_held,
                    events,
                    key,
                );
            }
        }
    }
    // Errors, under the rows, on red.
    if has_errors {
        let area = Rect::from_min_max(
            pos2(rect.left(), rect.bottom() - s(shape.errors_height)),
            rect.max,
        );
        painter.rect_filled(
            area.shrink(s(3.0)),
            s(4.0),
            Color32::from_rgba_unmultiplied(150, 30, 30, 200),
        );
        let mut y = area.top() + s(6.0);
        for error in &shape.errors {
            let galley = painter.layout(
                error.clone(),
                FontId::proportional(11.5 * z),
                Color32::WHITE,
                area.width() - s(16.0),
            );
            let height = galley.size().y;
            painter.galley(pos2(area.left() + s(8.0), y), galley, Color32::WHITE);
            y += height + s(4.0);
        }
    }
}

/// Stores a value edited in a control, only when it changed, so drawing
/// a node never changes the graph.
fn set_value(node: &mut GraphNode, name: &str, value: String) {
    if value != node.value(name) {
        node.values.insert(name.to_owned(), value);
    }
}

/// How a field's value reads when there's no control for it.
pub(super) fn shown_value(field: &FieldSpec, value: &str) -> String {
    match field.kind {
        FieldKind::Object {
            root: ObjectRoot::Gui,
            classes,
        } if value.is_empty() && (classes.is_empty() || classes.contains(&"ScreenGui")) => {
            "(this ScreenGui)".to_owned()
        }
        _ if value.is_empty() => "(none)".to_owned(),
        _ => value.to_owned(),
    }
}

/// The control for a field: a menu of choices, objects, images or
/// variables, a color button, or a text box.
fn field_control(
    ui: &mut egui::Ui,
    rect: Rect,
    field: &FieldSpec,
    value: &mut String,
    (objects, images, variables): (&[GraphObject], &[String], &[String]),
    id: egui::Id,
) {
    let choices: Vec<(String, String)> = match field.kind {
        FieldKind::Text => {
            ui.put(rect, egui::TextEdit::singleline(value).id(id));
            return;
        }
        FieldKind::Color => {
            let mut bytes = super::parse_numbers::<3>(value)
                .unwrap_or([1.0; 3])
                .map(|channel| (channel.clamp(0.0, 1.0) * 255.0).round() as u8);
            let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect));
            if child.color_edit_button_srgb(&mut bytes).changed() {
                *value = super::numbers_text(&bytes.map(|byte| f32::from(byte) / 255.0));
            }
            return;
        }
        FieldKind::Image => images
            .iter()
            .map(|path| {
                let name = path.rsplit('/').next().unwrap_or(path);
                (path.clone(), name.to_owned())
            })
            .collect(),
        FieldKind::Choice(choices) => choices
            .iter()
            .map(|choice| ((*choice).to_owned(), (*choice).to_owned()))
            .collect(),
        FieldKind::Object { root, classes } => objects
            .iter()
            .filter(|object| {
                object.root == root
                    && (classes.is_empty() || classes.contains(&object.class.as_str()))
            })
            .map(|object| {
                let shown = if object.path.is_empty() {
                    "(this ScreenGui)".to_owned()
                } else {
                    object.path.clone()
                };
                (object.path.clone(), shown)
            })
            .collect(),
        FieldKind::Variable => variables
            .iter()
            .map(|name| (name.clone(), name.clone()))
            .collect(),
    };
    let selected = choices
        .iter()
        .find(|(choice, _)| choice == value)
        .map_or_else(|| shown_value(field, value), |(_, shown)| shown.clone());
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect));
    // A value too long for the node is cut short (with …), not let out.
    egui::ComboBox::from_id_salt(id)
        .width(rect.width())
        .truncate()
        .selected_text(selected)
        .show_ui(&mut child, |ui| {
            if choices.is_empty() {
                ui.weak(match field.kind {
                    FieldKind::Variable => "Add a variable beside the graph first.",
                    FieldKind::Image => "Add an image to the project's assets first.",
                    _ => "There's nothing of this kind.",
                });
            }
            for (choice, shown) in &choices {
                if ui.selectable_label(choice == value, shown).clicked() {
                    *value = choice.clone();
                }
            }
        });
}

/// The box for a value typed into an unwired input.
fn value_control(ui: &mut egui::Ui, rect: Rect, ty: PinType, value: &mut String, id: egui::Id) {
    match ty {
        PinType::Number => {
            let mut number = value.trim().parse::<f64>().unwrap_or(0.0);
            if ui
                .put(rect, egui::DragValue::new(&mut number).speed(0.1))
                .changed()
            {
                *value = number.to_string();
            }
        }
        PinType::Bool => {
            let mut on = value == "true";
            let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect));
            if child.checkbox(&mut on, "").changed() {
                *value = on.to_string();
            }
        }
        PinType::Vector2 | PinType::Vector3 => {
            let hint = if ty == PinType::Vector2 {
                "x, y"
            } else {
                "x, y, z"
            };
            ui.put(
                rect,
                egui::TextEdit::singleline(value).hint_text(hint).id(id),
            );
        }
        _ => {
            let hint = if ty == PinType::Any { "value" } else { "" };
            ui.put(
                rect,
                egui::TextEdit::singleline(value).hint_text(hint).id(id),
            );
        }
    }
}

/// A pin's pointer handling: drag to wire, Alt-click or right-click to
/// break its wires.
#[allow(clippy::too_many_arguments)] // One pin's whole state.
fn pin_response(
    ui: &mut egui::Ui,
    painter: &egui::Painter,
    pin: PinRef,
    output: bool,
    at: Pos2,
    z: f32,
    wire_held: bool,
    events: &mut Vec<Event>,
    key: &str,
) {
    let reach = (PIN + 5.0) * z;
    let rect = Rect::from_center_size(at, Vec2::splat(reach * 2.0));
    let response = ui
        .interact(
            rect,
            egui::Id::new(("node graph pin", key, pin.clone(), output)),
            Sense::click_and_drag(),
        )
        .on_hover_cursor(CursorIcon::Crosshair);
    if response.hovered() && !wire_held {
        painter.circle_stroke(
            at,
            reach,
            Stroke::new(1.0_f32, Color32::from_white_alpha(90)),
        );
    }
    if response.drag_started_by(egui::PointerButton::Primary) {
        events.push(Event::WireFrom(pin.clone(), output));
    }
    if response.clicked() && ui.input(|i| i.modifiers.alt) {
        events.push(Event::Break(pin.clone(), output));
    }
    response.context_menu(|ui| {
        if ui
            .button(theme::with_icon(icons::LINK_BREAK, "Break links"))
            .clicked()
        {
            events.push(Event::Break(pin.clone(), output));
            ui.close();
        }
    });
}

/// An exec pin is Unreal's white arrow, a value pin a circle in its type's
/// colour; each is filled when it's wired.
fn draw_pin(painter: &egui::Painter, at: Pos2, z: f32, ty: PinType, connected: bool) {
    if ty == PinType::Exec {
        let s = |x: f32, y: f32| at + vec2(x * z, y * z);
        let points = vec![
            s(-5.0, -6.0),
            s(1.0, -6.0),
            s(6.0, 0.0),
            s(1.0, 6.0),
            s(-5.0, 6.0),
        ];
        let fill = if connected {
            Color32::WHITE
        } else {
            Color32::TRANSPARENT
        };
        painter.add(egui::Shape::convex_polygon(
            points,
            fill,
            Stroke::new(1.5 * z.max(0.7), Color32::WHITE),
        ));
    } else if connected {
        painter.circle_filled(at, PIN * z, pin_color(ty));
    } else {
        painter.circle(
            at,
            PIN * z,
            NODE_BODY,
            Stroke::new(1.5 * z.max(0.7), pin_color(ty)),
        );
    }
}
