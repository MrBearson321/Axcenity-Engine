//! The Lua of the blocks for Luau itself: locals, calls, functions, tables
//! and so on, which is what any typed script becomes (see `axcenity_blocks::from_lua`).

use super::*;

/// How tightly unary operators hold their value.
pub(super) const UNARY: u8 = 12;

/// A binary operator's left and right priorities (Lua's own table, which
/// Luau keeps): the higher, the tighter it holds.
pub fn binary_priority(op: &str) -> Option<(u8, u8)> {
    Some(match op {
        "or" => (1, 1),
        "and" => (2, 2),
        "<" | ">" | "<=" | ">=" | "~=" | "==" => (3, 3),
        ".." => (9, 8),
        "+" | "-" => (10, 10),
        "*" | "/" | "//" | "%" => (11, 11),
        "^" => (14, 13),
        _ => return None,
    })
}

/// Whether a child operator (with priorities `child`, `unary` if it's written
/// in front of its value) needs brackets on the `right` or left of `parent`.
pub fn needs_brackets(child: (u8, u8), unary: bool, parent: (u8, u8), right: bool) -> bool {
    if right {
        !unary && child.0 <= parent.1
    } else {
        parent.0 > child.1
    }
}

/// Whether `name` is a name, or a name and fields.
fn valid_path(name: &str) -> bool {
    name.split('.').all(valid_identifier)
}

/// Each line of `text` one level further in, past any block marks.
fn indented(text: &str, unit: &str) -> String {
    text.split('\n')
        .map(|line| {
            let bare = without_marks(line);
            format!("{}{unit}{bare}", &line[..line.len() - bare.len()])
        })
        .collect::<Vec<_>>()
        .join("\n")
}

impl Writer {
    fn list(&self, values: &[Expression]) -> Result<String, TranslateError> {
        Ok(values
            .iter()
            .map(|value| self.expression(value))
            .collect::<Result<Vec<_>, _>>()?
            .join(", "))
    }

    pub(super) fn write_connect(
        &mut self,
        signal: &Expression,
        method: &str,
        params: &str,
        body: &[Block],
        ids: &mut HashSet<u64>,
    ) -> Result<(), TranslateError> {
        if !valid_identifier(method) {
            return Err(TranslateError::InvalidProperty(method.to_owned()));
        }
        let signal = self.receiver(signal)?;
        self.line(format!("{signal}:{method}(function({params})"));
        self.nested(body, ids)?;
        self.line("end)");
        Ok(())
    }

    /// Writes the `elseif`s at the start of `else_body`, and returns what's
    /// left for the `else`.
    pub(super) fn write_elseifs<'b>(
        &mut self,
        else_body: &'b [Block],
        ids: &mut HashSet<u64>,
    ) -> Result<&'b [Block], TranslateError> {
        let mut rest = else_body;
        while let [block] = rest
            && block.chained
        {
            let (condition, then_body, otherwise): (_, _, &[Block]) = match &block.kind {
                BlockKind::If { condition, body } => (condition, body, &[]),
                BlockKind::IfElse {
                    condition,
                    then_body,
                    else_body,
                } => (condition, then_body, else_body),
                _ => break,
            };
            self.tag(block.id, ids)?;
            let condition = self.expression(condition)?;
            self.line(format!("elseif {condition} then"));
            self.nested(then_body, ids)?;
            rest = otherwise;
        }
        Ok(rest)
    }

    /// A line that starts with `(` would read as a call of what's before
    /// it, so the statement before it gets a `;`.
    pub(super) fn separate_from_previous(&mut self, first_line: usize) {
        let starts_bracketed = self
            .lines
            .get(first_line)
            .is_some_and(|line| line.trim_start().starts_with('('));
        if !starts_bracketed {
            return;
        }
        let previous = self.lines[..first_line]
            .iter_mut()
            .rev()
            .find(|line| !line.trim().is_empty() && !line.trim_start().starts_with("--"));
        if let Some(previous) = previous {
            previous.push(';');
        }
    }

    pub(super) fn write_luau(
        &mut self,
        kind: &BlockKind,
        ids: &mut HashSet<u64>,
    ) -> Result<(), TranslateError> {
        match kind {
            BlockKind::Comment { text } => self.code(text),
            BlockKind::Local { names, values } if values.is_empty() => {
                self.line(format!("local {names}"));
            }
            BlockKind::Local { names, values } => {
                let values = self.list(values)?;
                self.line(format!("local {names} = {values}"));
            }
            BlockKind::Assign { targets, values } => {
                let (targets, values) = (self.list(targets)?, self.list(values)?);
                self.line(format!("{targets} = {values}"));
            }
            BlockKind::CompoundAssign { target, op, value } => {
                if !LUA_COMPOUND_OPS.contains(&op.as_str()) {
                    return Err(TranslateError::InvalidOperator(op.clone()));
                }
                let (target, value) = (self.expression(target)?, self.expression(value)?);
                self.line(format!("{target} {op}= {value}"));
            }
            BlockKind::CallFunction { function, args } => {
                let (function, args) = (self.receiver(function)?, self.list(args)?);
                self.line(format!("{function}({args})"));
            }
            BlockKind::CallMethod {
                object,
                method,
                args,
            } => {
                if !valid_identifier(method) {
                    return Err(TranslateError::InvalidProperty(method.clone()));
                }
                let (object, args) = (self.receiver(object)?, self.list(args)?);
                self.line(format!("{object}:{method}({args})"));
            }
            BlockKind::DefineFunction {
                name,
                local,
                params,
                generics,
                returns,
                attributes,
                body,
            } => {
                let mut opening = String::new();
                if !attributes.is_empty() {
                    opening.push_str(attributes);
                    opening.push(' ');
                }
                if *local {
                    opening.push_str("local ");
                }
                opening.push_str(&format!("function {name}{generics}({params}){returns}"));
                self.line(opening);
                self.nested(body, ids)?;
                self.line("end");
            }
            BlockKind::While { condition, body } => {
                let condition = self.expression(condition)?;
                self.line(format!("while {condition} do"));
                self.nested(body, ids)?;
                self.line("end");
            }
            BlockKind::RepeatLoop { body, condition } => {
                self.line("repeat");
                self.nested(body, ids)?;
                let condition = self.expression(condition)?;
                self.line(format!("until {condition}"));
            }
            BlockKind::NumericFor {
                variable,
                start,
                stop,
                step,
                body,
            } => {
                let (start, stop) = (self.expression(start)?, self.expression(stop)?);
                let step = match step {
                    Some(step) => format!(", {}", self.expression(step)?),
                    None => String::new(),
                };
                self.line(format!("for {variable} = {start}, {stop}{step} do"));
                self.nested(body, ids)?;
                self.line("end");
            }
            BlockKind::GenericFor {
                names,
                values,
                body,
            } => {
                let values = self.list(values)?;
                self.line(format!("for {names} in {values} do"));
                self.nested(body, ids)?;
                self.line("end");
            }
            BlockKind::DoBlock { body } => {
                self.line("do");
                self.nested(body, ids)?;
                self.line("end");
            }
            BlockKind::Break => self.line("break"),
            BlockKind::Continue => self.line("continue"),
            BlockKind::Return { values } if values.is_empty() => self.line("return"),
            BlockKind::Return { values } => {
                let values = self.list(values)?;
                self.line(format!("return {values}"));
            }
            _ => return Err(TranslateError::InvalidEventPlacement),
        }
        Ok(())
    }

    /// The Lua of the values that are Luau's own.
    pub(super) fn luau_value(&self, value: &Expression) -> Result<String, TranslateError> {
        Ok(match value {
            Expression::Nil => "nil".to_owned(),
            Expression::Vararg => "...".to_owned(),
            Expression::Name { name } if valid_path(name) => name.clone(),
            Expression::Name { name } => return Err(TranslateError::InvalidName(name.clone())),
            Expression::Field { object, name } => {
                if !valid_identifier(name) {
                    return Err(TranslateError::InvalidProperty(name.clone()));
                }
                format!("{}.{name}", self.receiver(object)?)
            }
            Expression::Index { object, key } => {
                format!("{}[{}]", self.receiver(object)?, self.expression(key)?)
            }
            Expression::Call { function, args } => {
                format!("{}({})", self.receiver(function)?, self.list(args)?)
            }
            Expression::MethodCall {
                object,
                method,
                args,
            } => {
                if !valid_identifier(method) {
                    return Err(TranslateError::InvalidProperty(method.clone()));
                }
                format!("{}:{method}({})", self.receiver(object)?, self.list(args)?)
            }
            Expression::Closure {
                params,
                generics,
                returns,
                body,
            } => self.closure(params, generics, returns, body)?,
            Expression::Table { fields, multiline } => self.table(fields, *multiline)?,
            Expression::Binary { op, left, right } => {
                if !LUA_BINARY_OPS.contains(&op.as_str()) {
                    return Err(TranslateError::InvalidOperator(op.clone()));
                }
                format!(
                    "{} {op} {}",
                    self.operand(left, op, false)?,
                    self.operand(right, op, true)?
                )
            }
            Expression::Unary { op, value } => {
                if !LUA_UNARY_OPS.contains(&op.as_str()) {
                    return Err(TranslateError::InvalidOperator(op.clone()));
                }
                let inner = self.operand(value, op, true)?;
                // `--` would start a comment.
                if op == "not" || (op == "-" && inner.starts_with('-')) {
                    format!("{op} {inner}")
                } else {
                    format!("{op}{inner}")
                }
            }
            Expression::Paren { value } => format!("({})", self.expression(value)?),
            Expression::Interpolated { parts, values } => {
                let mut text = String::from("`");
                for (index, part) in parts.iter().enumerate() {
                    text.push_str(part);
                    if let Some(value) = values.get(index) {
                        text.push('{');
                        text.push_str(&self.expression(value)?);
                        text.push('}');
                    }
                }
                text.push('`');
                text
            }
            Expression::IfValue { parts } => {
                if parts.len() < 3 || parts.len().is_multiple_of(2) {
                    return Err(TranslateError::InvalidIfValue);
                }
                let mut text = String::new();
                let (pairs, _) = parts.as_chunks::<2>();
                for (index, [condition, value]) in pairs.iter().enumerate() {
                    let word = if index == 0 { "if" } else { " elseif" };
                    text.push_str(&format!(
                        "{word} {} then {}",
                        self.expression(condition)?,
                        self.expression(value)?
                    ));
                }
                let otherwise = parts.last().ok_or(TranslateError::InvalidIfValue)?;
                text.push_str(&format!(" else {}", self.expression(otherwise)?));
                text
            }
            Expression::Cast { value, annotation } => {
                format!("{} :: {annotation}", self.expression(value)?)
            }
            _ => return Err(TranslateError::InvalidEventPlacement),
        })
    }

    /// A function, with marks on the lines its blocks start on.
    fn closure(
        &self,
        params: &str,
        generics: &str,
        returns: &str,
        body: &[Block],
    ) -> Result<String, TranslateError> {
        let mut inner = Writer {
            indent: 1,
            in_touched: self.in_touched,
            tabs: self.tabs,
            ..Default::default()
        };
        inner.write_body(body, &mut HashSet::new())?;
        let mut text = format!("function{generics}({params}){returns}");
        for (index, line) in inner.lines.iter().enumerate() {
            text.push('\n');
            for (_, id) in inner.block_lines.iter().filter(|(at, _)| *at == index + 1) {
                text.push(LINE_MARK);
                text.push_str(&id.to_string());
                text.push(LINE_MARK_END);
            }
            text.push_str(line);
        }
        text.push_str("\nend");
        Ok(text)
    }

    fn table(&self, fields: &[TableField], multiline: bool) -> Result<String, TranslateError> {
        if fields.is_empty() {
            return Ok("{}".to_owned());
        }
        let written = fields
            .iter()
            .map(|field| {
                Ok(match field {
                    TableField::Named { name, value } => {
                        if !valid_identifier(name) {
                            return Err(TranslateError::InvalidProperty(name.clone()));
                        }
                        format!("{name} = {}", self.expression(value)?)
                    }
                    TableField::Keyed { key, value } => {
                        format!("[{}] = {}", self.expression(key)?, self.expression(value)?)
                    }
                    TableField::Item { value } => self.expression(value)?,
                })
            })
            .collect::<Result<Vec<_>, TranslateError>>()?;
        if !multiline {
            return Ok(format!("{{ {} }}", written.join(", ")));
        }
        let mut text = String::from("{\n");
        for field in written {
            text.push_str(&indented(&field, self.indent_unit()));
            text.push_str(",\n");
        }
        text.push('}');
        Ok(text)
    }
}
