//! The visual editor for persisted block scripts and their generated Lua.

use axcenity_blocks::{
    Block, BlockKind, BlockScript, Comparison, Condition, Expression, translate,
};
use eframe::egui;

pub fn show(
    ui: &mut egui::Ui,
    script: &mut BlockScript,
    client: bool,
    highlighted: Option<u64>,
) -> bool {
    let before = script.clone();
    ui.horizontal(|ui| {
        ui.heading("Blocks");
        ui.weak(if client {
            "Client script"
        } else {
            "Server script"
        });
    });
    ui.label("Blocks snap into statement sequences and translate to Lua when the game runs.");
    ui.separator();
    ui.horizontal(|ui| {
        if ui.button("When game starts").clicked() {
            script
                .blocks
                .push(Block::new(BlockKind::WhenStarted { body: Vec::new() }));
        }
        if ui.button("When part is touched").clicked() {
            script.blocks.push(Block::new(BlockKind::WhenTouched {
                part: "Part".to_owned(),
                body: Vec::new(),
            }));
        }
    });
    egui::ScrollArea::vertical().show(ui, |ui| {
        show_events(ui, &mut script.blocks, highlighted);
        ui.collapsing("View as Lua", |ui| match translate(script) {
            Ok(translation) => {
                let mut source = translation.source;
                ui.add(
                    egui::TextEdit::multiline(&mut source)
                        .font(egui::TextStyle::Monospace)
                        .desired_width(f32::INFINITY)
                        .interactive(false),
                );
            }
            Err(err) => {
                ui.colored_label(ui.visuals().error_fg_color, err.to_string());
            }
        });
    });
    *script != before
}

fn show_events(ui: &mut egui::Ui, blocks: &mut Vec<Block>, highlighted: Option<u64>) {
    let mut remove = None;
    for (index, block) in blocks.iter_mut().enumerate() {
        let title = match &block.kind {
            BlockKind::WhenStarted { .. } => "When game starts".to_owned(),
            BlockKind::WhenTouched { part, .. } => format!("When {part} is touched"),
            _ => "Invalid event block".to_owned(),
        };
        block_frame(ui, block.id, highlighted, |ui| {
            ui.horizontal(|ui| {
                ui.strong(title);
                if ui.small_button("Remove").clicked() {
                    remove = Some(index);
                }
            });
            match &mut block.kind {
                BlockKind::WhenStarted { body } => show_body(ui, body, highlighted),
                BlockKind::WhenTouched { part, body } => {
                    ui.horizontal(|ui| {
                        ui.label("Part path");
                        ui.text_edit_singleline(part);
                    });
                    show_body(ui, body, highlighted);
                }
                _ => {
                    ui.colored_label(ui.visuals().error_fg_color, "Only event blocks fit here.");
                }
            }
        });
    }
    if let Some(index) = remove {
        blocks.remove(index);
    }
}

fn show_body(ui: &mut egui::Ui, blocks: &mut Vec<Block>, highlighted: Option<u64>) {
    let mut remove = None;
    let mut move_by = None;
    let len = blocks.len();
    for (index, block) in blocks.iter_mut().enumerate() {
        let title = block_title(&block.kind);
        block_frame(ui, block.id, highlighted, |ui| {
            ui.horizontal(|ui| {
                ui.strong(title);
                if ui.small_button("↑").clicked() && index > 0 {
                    move_by = Some((index, index - 1));
                }
                if ui.small_button("↓").clicked() && index + 1 < len {
                    move_by = Some((index, index + 1));
                }
                if ui.small_button("Remove").clicked() {
                    remove = Some(index);
                }
            });
            show_statement(ui, block, highlighted);
        });
    }
    if let Some(index) = remove {
        blocks.remove(index);
    } else if let Some((from, to)) = move_by {
        blocks.swap(from, to);
    }
    if let Some(block) = add_statement(ui) {
        blocks.push(block);
    }
}

fn block_frame(
    ui: &mut egui::Ui,
    id: u64,
    highlighted: Option<u64>,
    contents: impl FnOnce(&mut egui::Ui),
) {
    let frame = egui::Frame::group(ui.style()).fill(if highlighted == Some(id) {
        ui.visuals().error_fg_color.gamma_multiply(0.22)
    } else {
        ui.visuals().faint_bg_color
    });
    frame.show(ui, |ui| ui.push_id(id, contents));
}

fn show_statement(ui: &mut egui::Ui, block: &mut Block, highlighted: Option<u64>) {
    match &mut block.kind {
        BlockKind::SetProperty {
            target,
            property,
            value,
        } => {
            ui.horizontal(|ui| {
                ui.label("Object");
                ui.text_edit_singleline(target);
                ui.label("Property");
                ui.text_edit_singleline(property);
            });
            ui.horizontal(|ui| {
                ui.label("Value");
                show_expression(ui, value);
            });
        }
        BlockKind::Wait { seconds } => {
            ui.horizontal(|ui| {
                ui.label("Wait seconds");
                show_expression(ui, seconds);
            });
        }
        BlockKind::MoveTo { target, position } => {
            ui.horizontal(|ui| {
                ui.label("Move object");
                ui.text_edit_singleline(target);
                ui.label("to");
                show_expression(ui, position);
            });
        }
        BlockKind::IfElse {
            condition,
            then_body,
            else_body,
        } => {
            ui.horizontal(|ui| {
                ui.label("If");
                show_expression(ui, &mut condition.left);
                comparison(ui, block.id, &mut condition.operator);
                show_expression(ui, &mut condition.right);
            });
            ui.label("Then");
            show_body(ui, then_body, highlighted);
            ui.label("Else");
            show_body(ui, else_body, highlighted);
        }
        BlockKind::Repeat { count, body } => {
            ui.horizontal(|ui| {
                ui.label("Repeat");
                show_expression(ui, count);
                ui.label("times");
            });
            show_body(ui, body, highlighted);
        }
        BlockKind::SetVariable { name, value } => {
            ui.horizontal(|ui| {
                ui.label("Set variable");
                ui.text_edit_singleline(name);
                ui.label("to");
                show_expression(ui, value);
            });
        }
        BlockKind::PlaySound { target } => {
            ui.horizontal(|ui| {
                ui.label("Play sound");
                ui.text_edit_singleline(target);
            });
        }
        BlockKind::StopSound { target } => {
            ui.horizontal(|ui| {
                ui.label("Stop sound");
                ui.text_edit_singleline(target);
            });
        }
        BlockKind::WhenStarted { .. } | BlockKind::WhenTouched { .. } => {
            ui.colored_label(
                ui.visuals().error_fg_color,
                "Event blocks only fit at the top level.",
            );
        }
    }
}

fn comparison(ui: &mut egui::Ui, id: u64, operator: &mut Comparison) {
    egui::ComboBox::from_id_salt(("comparison", id))
        .selected_text(match operator {
            Comparison::Equal => "equals",
            Comparison::NotEqual => "does not equal",
            Comparison::Less => "less than",
            Comparison::LessOrEqual => "at most",
            Comparison::Greater => "greater than",
            Comparison::GreaterOrEqual => "at least",
        })
        .show_ui(ui, |ui| {
            for (choice, title) in [
                (Comparison::Equal, "equals"),
                (Comparison::NotEqual, "does not equal"),
                (Comparison::Less, "less than"),
                (Comparison::LessOrEqual, "at most"),
                (Comparison::Greater, "greater than"),
                (Comparison::GreaterOrEqual, "at least"),
            ] {
                ui.selectable_value(operator, choice, title);
            }
        });
}

fn show_expression(ui: &mut egui::Ui, expression: &mut Expression) {
    let kind = match expression {
        Expression::Number { .. } => 0,
        Expression::Text { .. } => 1,
        Expression::Boolean { .. } => 2,
        Expression::Variable { .. } => 3,
        Expression::Vector3 { .. } => 4,
        Expression::Color3 { .. } => 5,
    };
    let mut selected = kind;
    egui::ComboBox::from_id_salt(ui.next_auto_id())
        .selected_text(["Number", "Text", "Boolean", "Variable", "Vector3", "Color3"][kind])
        .show_ui(ui, |ui| {
            for (value, title) in [
                (0, "Number"),
                (1, "Text"),
                (2, "Boolean"),
                (3, "Variable"),
                (4, "Vector3"),
                (5, "Color3"),
            ] {
                ui.selectable_value(&mut selected, value, title);
            }
        });
    if selected != kind {
        *expression = match selected {
            0 => Expression::Number { value: 0.0 },
            1 => Expression::Text {
                value: String::new(),
            },
            2 => Expression::Boolean { value: false },
            3 => Expression::Variable {
                name: "value".to_owned(),
            },
            4 => Expression::Vector3 {
                x: 0.0,
                y: 0.0,
                z: 0.0,
            },
            _ => Expression::Color3 {
                r: 1.0,
                g: 1.0,
                b: 1.0,
            },
        };
    }
    match expression {
        Expression::Number { value } => {
            ui.add(egui::DragValue::new(value).speed(0.1).max_decimals(3));
        }
        Expression::Text { value } | Expression::Variable { name: value } => {
            ui.text_edit_singleline(value);
        }
        Expression::Boolean { value } => {
            ui.checkbox(value, "");
        }
        Expression::Vector3 { x, y, z } => {
            for value in [x, y, z] {
                ui.add(egui::DragValue::new(value).speed(0.1).max_decimals(3));
            }
        }
        Expression::Color3 { r, g, b } => {
            for value in [r, g, b] {
                ui.add(
                    egui::DragValue::new(value)
                        .speed(0.01)
                        .range(0.0..=1.0)
                        .max_decimals(3),
                );
            }
        }
    }
}

fn add_statement(ui: &mut egui::Ui) -> Option<Block> {
    let mut chosen = None;
    ui.menu_button("Add statement block", |ui| {
        for (title, kind) in [
            (
                "Set property",
                BlockKind::SetProperty {
                    target: "Part".to_owned(),
                    property: "Color".to_owned(),
                    value: Expression::Color3 {
                        r: 1.0,
                        g: 0.0,
                        b: 0.0,
                    },
                },
            ),
            (
                "Wait",
                BlockKind::Wait {
                    seconds: Expression::Number { value: 1.0 },
                },
            ),
            (
                "Move to",
                BlockKind::MoveTo {
                    target: "Part".to_owned(),
                    position: Expression::Vector3 {
                        x: 0.0,
                        y: 5.0,
                        z: 0.0,
                    },
                },
            ),
            (
                "If / else",
                BlockKind::IfElse {
                    condition: Condition {
                        left: Expression::Number { value: 0.0 },
                        operator: Comparison::Equal,
                        right: Expression::Number { value: 0.0 },
                    },
                    then_body: Vec::new(),
                    else_body: Vec::new(),
                },
            ),
            (
                "Repeat",
                BlockKind::Repeat {
                    count: Expression::Number { value: 5.0 },
                    body: Vec::new(),
                },
            ),
            (
                "Set variable",
                BlockKind::SetVariable {
                    name: "score".to_owned(),
                    value: Expression::Number { value: 0.0 },
                },
            ),
            (
                "Play sound",
                BlockKind::PlaySound {
                    target: "Sound".to_owned(),
                },
            ),
            (
                "Stop sound",
                BlockKind::StopSound {
                    target: "Sound".to_owned(),
                },
            ),
        ] {
            if ui.button(title).clicked() {
                chosen = Some(Block::new(kind));
                ui.close();
            }
        }
    });
    chosen
}

fn block_title(kind: &BlockKind) -> &'static str {
    match kind {
        BlockKind::WhenStarted { .. } => "When game starts",
        BlockKind::WhenTouched { .. } => "When part is touched",
        BlockKind::SetProperty { .. } => "Set property",
        BlockKind::Wait { .. } => "Wait",
        BlockKind::MoveTo { .. } => "Move to",
        BlockKind::IfElse { .. } => "If / else",
        BlockKind::Repeat { .. } => "Repeat",
        BlockKind::SetVariable { .. } => "Set variable",
        BlockKind::PlaySound { .. } => "Play sound",
        BlockKind::StopSound { .. } => "Stop sound",
    }
}
