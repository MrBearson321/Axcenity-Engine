//! Scratch-style script data and its one-way translation to Lua.

use std::collections::HashSet;
use std::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Serialize};
use thiserror::Error;

static NEXT_BLOCK_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlockShape {
    Event,
    Statement,
    Value,
    Boolean,
}

impl BlockShape {
    pub fn accepts(self, child: BlockShape) -> bool {
        matches!(
            (self, child),
            (BlockShape::Event, BlockShape::Event)
                | (BlockShape::Statement, BlockShape::Statement)
                | (BlockShape::Value, BlockShape::Value)
                | (BlockShape::Boolean, BlockShape::Boolean)
        )
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BlockScript {
    pub blocks: Vec<Block>,
}

pub fn reserve_block_ids(script: &BlockScript) {
    fn maximum(blocks: &[Block]) -> u64 {
        blocks
            .iter()
            .map(|block| {
                let child_max = match &block.kind {
                    BlockKind::WhenStarted { body }
                    | BlockKind::WhenTouched { body, .. }
                    | BlockKind::Repeat { body, .. } => maximum(body),
                    BlockKind::IfElse {
                        then_body,
                        else_body,
                        ..
                    } => maximum(then_body).max(maximum(else_body)),
                    _ => 0,
                };
                block.id.max(child_max)
            })
            .max()
            .unwrap_or(0)
    }

    NEXT_BLOCK_ID.fetch_max(maximum(&script.blocks).saturating_add(1), Ordering::Relaxed);
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Block {
    pub id: u64,
    pub kind: BlockKind,
}

impl Block {
    pub fn new(kind: BlockKind) -> Self {
        Self {
            id: NEXT_BLOCK_ID.fetch_add(1, Ordering::Relaxed),
            kind,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum BlockKind {
    WhenStarted {
        body: Vec<Block>,
    },
    WhenTouched {
        part: String,
        body: Vec<Block>,
    },
    SetProperty {
        target: String,
        property: String,
        value: Expression,
    },
    Wait {
        seconds: Expression,
    },
    MoveTo {
        target: String,
        position: Expression,
    },
    IfElse {
        condition: Condition,
        then_body: Vec<Block>,
        else_body: Vec<Block>,
    },
    Repeat {
        count: Expression,
        body: Vec<Block>,
    },
    SetVariable {
        name: String,
        value: Expression,
    },
    PlaySound {
        target: String,
    },
    StopSound {
        target: String,
    },
}

impl BlockKind {
    pub fn shape(&self) -> BlockShape {
        match self {
            BlockKind::WhenStarted { .. } | BlockKind::WhenTouched { .. } => BlockShape::Event,
            _ => BlockShape::Statement,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Expression {
    Number { value: f64 },
    Text { value: String },
    Boolean { value: bool },
    Variable { name: String },
    Vector3 { x: f64, y: f64, z: f64 },
    Color3 { r: f64, g: f64, b: f64 },
}

impl Expression {
    pub fn shape(&self) -> BlockShape {
        match self {
            Expression::Boolean { .. } => BlockShape::Boolean,
            _ => BlockShape::Value,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Condition {
    pub left: Expression,
    pub operator: Comparison,
    pub right: Expression,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Comparison {
    Equal,
    NotEqual,
    Less,
    LessOrEqual,
    Greater,
    GreaterOrEqual,
}

impl Comparison {
    fn lua(self) -> &'static str {
        match self {
            Self::Equal => "==",
            Self::NotEqual => "~=",
            Self::Less => "<",
            Self::LessOrEqual => "<=",
            Self::Greater => ">",
            Self::GreaterOrEqual => ">=",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Translation {
    pub source: String,
    /// Generated Lua line to source block ID, with lines numbered from 1.
    pub block_lines: Vec<(usize, u64)>,
}

#[derive(Debug, Error, PartialEq)]
pub enum TranslateError {
    #[error("block ID {0} appears more than once")]
    DuplicateId(u64),
    #[error(
        "variable name `{0}` must start with a letter or underscore and contain only letters, digits or underscores"
    )]
    InvalidVariable(String),
    #[error("number blocks must contain finite numbers")]
    InvalidNumber,
    #[error("Instance target `{0}` must be a non-empty dotted path of child names")]
    InvalidTarget(String),
    #[error("only event blocks can be at the top level, and events cannot be nested")]
    InvalidEventPlacement,
}

pub fn translate(script: &BlockScript) -> Result<Translation, TranslateError> {
    let mut state = Writer::default();
    let mut ids = HashSet::new();
    for block in &script.blocks {
        if !matches!(
            block.kind,
            BlockKind::WhenStarted { .. } | BlockKind::WhenTouched { .. }
        ) {
            return Err(TranslateError::InvalidEventPlacement);
        }
        state.write_event(block, &mut ids)?;
    }
    Ok(Translation {
        source: state.lines.join("\n") + if state.lines.is_empty() { "" } else { "\n" },
        block_lines: state.block_lines,
    })
}

#[derive(Default)]
struct Writer {
    lines: Vec<String>,
    block_lines: Vec<(usize, u64)>,
    indent: usize,
    repeat_id: u64,
}

impl Writer {
    fn line(&mut self, text: impl Into<String>) {
        self.lines
            .push(format!("{}{}", "    ".repeat(self.indent), text.into()));
    }

    fn tag(&mut self, id: u64, ids: &mut HashSet<u64>) -> Result<(), TranslateError> {
        if !ids.insert(id) {
            return Err(TranslateError::DuplicateId(id));
        }
        self.line(format!("--[[axcenity:block:{id}]]"));
        self.block_lines.push((self.lines.len(), id));
        Ok(())
    }

    fn write_event(&mut self, block: &Block, ids: &mut HashSet<u64>) -> Result<(), TranslateError> {
        self.tag(block.id, ids)?;
        match &block.kind {
            BlockKind::WhenStarted { body } => {
                self.line("task.spawn(function()");
                self.indent += 1;
                self.write_body(body, ids)?;
                self.indent -= 1;
                self.line("end)");
            }
            BlockKind::WhenTouched { part, body } => {
                let part = target(part)?;
                self.line(format!("{}.Touched:Connect(function(otherPart)", part));
                self.indent += 1;
                self.write_body(body, ids)?;
                self.indent -= 1;
                self.line("end)");
            }
            _ => return Err(TranslateError::InvalidEventPlacement),
        }
        Ok(())
    }

    fn write_body(&mut self, body: &[Block], ids: &mut HashSet<u64>) -> Result<(), TranslateError> {
        for block in body {
            if block.kind.shape() != BlockShape::Statement {
                return Err(TranslateError::InvalidEventPlacement);
            }
            self.tag(block.id, ids)?;
            match &block.kind {
                BlockKind::SetProperty {
                    target: object,
                    property,
                    value,
                } => {
                    self.line(format!(
                        "{}[{}] = {}",
                        target(object)?,
                        quote(property),
                        expression(value)?
                    ));
                }
                BlockKind::Wait { seconds } => {
                    self.line(format!("task.wait({})", expression(seconds)?));
                }
                BlockKind::MoveTo {
                    target: object,
                    position,
                } => {
                    self.line(format!(
                        "{}.Position = {}",
                        target(object)?,
                        expression(position)?
                    ));
                }
                BlockKind::IfElse {
                    condition,
                    then_body,
                    else_body,
                } => {
                    self.line(format!(
                        "if {} {} {} then",
                        expression(&condition.left)?,
                        condition.operator.lua(),
                        expression(&condition.right)?
                    ));
                    self.indent += 1;
                    self.write_body(then_body, ids)?;
                    self.indent -= 1;
                    if else_body.is_empty() {
                        self.line("end");
                    } else {
                        self.line("else");
                        self.indent += 1;
                        self.write_body(else_body, ids)?;
                        self.indent -= 1;
                        self.line("end");
                    }
                }
                BlockKind::Repeat { count, body } => {
                    self.repeat_id += 1;
                    let iterator = format!("__block_repeat_{}", self.repeat_id);
                    self.line(format!(
                        "for {iterator} = 1, math.max(0, math.floor({})) do",
                        expression(count)?
                    ));
                    self.indent += 1;
                    self.write_body(body, ids)?;
                    self.indent -= 1;
                    self.line("end");
                }
                BlockKind::SetVariable { name, value } => {
                    if !valid_identifier(name) {
                        return Err(TranslateError::InvalidVariable(name.clone()));
                    }
                    self.line(format!("local {name} = {}", expression(value)?));
                }
                BlockKind::PlaySound { target: object } => {
                    self.line(format!("{}:Play()", target(object)?));
                }
                BlockKind::StopSound { target: object } => {
                    self.line(format!("{}:Stop()", target(object)?));
                }
                BlockKind::WhenStarted { .. } | BlockKind::WhenTouched { .. } => {
                    return Err(TranslateError::InvalidEventPlacement);
                }
            }
        }
        Ok(())
    }
}

fn expression(expression: &Expression) -> Result<String, TranslateError> {
    match expression {
        Expression::Number { value } if value.is_finite() => Ok(value.to_string()),
        Expression::Number { .. } => Err(TranslateError::InvalidNumber),
        Expression::Text { value } => Ok(quote(value)),
        Expression::Boolean { value } => Ok(value.to_string()),
        Expression::Variable { name } if valid_identifier(name) => Ok(name.clone()),
        Expression::Variable { name } => Err(TranslateError::InvalidVariable(name.clone())),
        Expression::Vector3 { x, y, z } if [x, y, z].iter().all(|value| value.is_finite()) => {
            Ok(format!("Vector3.new({x}, {y}, {z})"))
        }
        Expression::Vector3 { .. } => Err(TranslateError::InvalidNumber),
        Expression::Color3 { r, g, b } if [r, g, b].iter().all(|value| value.is_finite()) => {
            Ok(format!("Color3.new({r}, {g}, {b})"))
        }
        Expression::Color3 { .. } => Err(TranslateError::InvalidNumber),
    }
}

fn target(path: &str) -> Result<String, TranslateError> {
    let parts = path.split('.').collect::<Vec<_>>();
    if parts.iter().any(|part| part.is_empty()) {
        return Err(TranslateError::InvalidTarget(path.to_owned()));
    }
    Ok(parts
        .into_iter()
        .fold("workspace".to_owned(), |prefix, part| {
            format!("{prefix}[{}]", quote(part))
        }))
}

fn quote(text: &str) -> String {
    let mut escaped = String::from("\"");
    for character in text.chars() {
        match character {
            '\\' => escaped.push_str("\\\\"),
            '"' => escaped.push_str("\\\""),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            '\0' => escaped.push_str("\\000"),
            character if character.is_control() => {
                escaped.push_str(&format!("\\{:03}", character as u32));
            }
            character => escaped.push(character),
        }
    }
    escaped.push('"');
    escaped
}

fn valid_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|first| first == '_' || first.is_ascii_alphabetic())
        && chars.all(|character| character == '_' || character.is_ascii_alphanumeric())
        && !matches!(
            name,
            "and"
                | "break"
                | "do"
                | "else"
                | "elseif"
                | "end"
                | "false"
                | "for"
                | "function"
                | "goto"
                | "if"
                | "in"
                | "local"
                | "nil"
                | "not"
                | "or"
                | "repeat"
                | "return"
                | "then"
                | "true"
                | "until"
                | "while"
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn translates_events_actions_and_sound_blocks_to_lua() {
        let script = BlockScript {
            blocks: vec![Block {
                id: 1,
                kind: BlockKind::WhenTouched {
                    part: "Pad".to_owned(),
                    body: vec![
                        Block {
                            id: 2,
                            kind: BlockKind::SetProperty {
                                target: "Pad".to_owned(),
                                property: "Color".to_owned(),
                                value: Expression::Color3 {
                                    r: 1.0,
                                    g: 0.0,
                                    b: 0.0,
                                },
                            },
                        },
                        Block {
                            id: 3,
                            kind: BlockKind::PlaySound {
                                target: "Pad.Chime".to_owned(),
                            },
                        },
                        Block {
                            id: 4,
                            kind: BlockKind::StopSound {
                                target: "Pad.Chime".to_owned(),
                            },
                        },
                    ],
                },
            }],
        };
        let translated = translate(&script).unwrap();
        assert!(
            translated
                .source
                .contains("workspace[\"Pad\"].Touched:Connect")
        );
        assert!(translated.source.contains("workspace[\"Pad\"][\"Color\"]"));
        assert!(
            translated
                .source
                .contains("workspace[\"Pad\"][\"Chime\"]:Play()")
        );
        assert!(
            translated
                .source
                .contains("workspace[\"Pad\"][\"Chime\"]:Stop()")
        );
        assert_eq!(translated.block_lines, [(1, 1), (3, 2), (5, 3), (7, 4)]);
    }

    #[test]
    fn emits_readable_conditionals_repeats_and_variables() {
        let script = BlockScript {
            blocks: vec![Block {
                id: 1,
                kind: BlockKind::WhenStarted {
                    body: vec![
                        Block {
                            id: 2,
                            kind: BlockKind::SetVariable {
                                name: "count".to_owned(),
                                value: Expression::Number { value: 3.0 },
                            },
                        },
                        Block {
                            id: 3,
                            kind: BlockKind::Wait {
                                seconds: Expression::Number { value: 0.5 },
                            },
                        },
                        Block {
                            id: 4,
                            kind: BlockKind::MoveTo {
                                target: "Pad".to_owned(),
                                position: Expression::Vector3 {
                                    x: 1.0,
                                    y: 2.0,
                                    z: 3.0,
                                },
                            },
                        },
                        Block {
                            id: 5,
                            kind: BlockKind::IfElse {
                                condition: Condition {
                                    left: Expression::Variable {
                                        name: "count".to_owned(),
                                    },
                                    operator: Comparison::Greater,
                                    right: Expression::Number { value: 2.0 },
                                },
                                then_body: vec![Block {
                                    id: 6,
                                    kind: BlockKind::SetProperty {
                                        target: "Pad".to_owned(),
                                        property: "Transparency".to_owned(),
                                        value: Expression::Number { value: 0.5 },
                                    },
                                }],
                                else_body: vec![Block {
                                    id: 7,
                                    kind: BlockKind::PlaySound {
                                        target: "Chime".to_owned(),
                                    },
                                }],
                            },
                        },
                        Block {
                            id: 8,
                            kind: BlockKind::Repeat {
                                count: Expression::Number { value: 2.0 },
                                body: vec![Block {
                                    id: 9,
                                    kind: BlockKind::Wait {
                                        seconds: Expression::Number { value: 1.0 },
                                    },
                                }],
                            },
                        },
                    ],
                },
            }],
        };
        let translated = translate(&script).unwrap();
        assert!(translated.source.contains("local count = 3"));
        assert!(translated.source.contains("task.wait(0.5)"));
        assert!(
            translated
                .source
                .contains("workspace[\"Pad\"].Position = Vector3.new(1, 2, 3)")
        );
        assert!(translated.source.contains("if count > 2 then"));
        assert!(translated.source.contains("else"));
        assert!(translated.source.contains("for __block_repeat_1 = 1"));
        assert!(translated.source.contains("task.wait(1)"));
        assert!(translated.source.contains("workspace[\"Chime\"]:Play()"));
    }

    #[test]
    fn rejects_duplicate_ids_invalid_names_and_event_blocks_in_slots() {
        let repeated = Block {
            id: 2,
            kind: BlockKind::Wait {
                seconds: Expression::Number { value: 1.0 },
            },
        };
        let script = BlockScript {
            blocks: vec![Block {
                id: 1,
                kind: BlockKind::WhenStarted {
                    body: vec![repeated.clone(), repeated],
                },
            }],
        };
        assert_eq!(translate(&script), Err(TranslateError::DuplicateId(2)));
        assert!(!BlockShape::Statement.accepts(BlockShape::Event));
        assert_eq!(quote("sound\"path"), "\"sound\\\"path\"");
    }
}
