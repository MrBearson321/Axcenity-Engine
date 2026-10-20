//! A small Lua 5.4 reader: tokens, then a syntax tree with each node's
//! place in the source. [`crate::from_lua`] uses it to turn code back into
//! blocks. It reads all of Lua, but only keeps the detail the blocks need;
//! everything else is kept as the source text it came from.

use std::ops::Range;

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Token {
    Name(String),
    Keyword(&'static str),
    /// `None` for numbers the blocks can't hold, such as hex floats.
    Number(Option<f64>),
    Str(String),
    Symbol(&'static str),
    Eof,
}

#[derive(Clone, Debug)]
pub(crate) struct Spanned {
    pub token: Token,
    pub span: Range<usize>,
}

/// Where in the code reading it went wrong, as a 1-based line.
#[derive(Clone, Debug, PartialEq)]
pub struct SyntaxError {
    pub line: usize,
    pub message: String,
}

const KEYWORDS: [&str; 22] = [
    "and", "break", "do", "else", "elseif", "end", "false", "for", "function", "goto", "if", "in",
    "local", "nil", "not", "or", "repeat", "return", "then", "true", "until", "while",
];

/// Longest first, so `..` isn't read as two dots.
const SYMBOLS: [&str; 33] = [
    "...", "..", "==", "~=", "<=", ">=", "//", "::", "<<", ">>", "+", "-", "*", "/", "%", "^", "#",
    "&", "~", "|", "<", ">", "=", "(", ")", "{", "}", "[", "]", ";", ":", ",", ".",
];

fn line_of(source: &str, at: usize) -> usize {
    source[..at.min(source.len())].matches('\n').count() + 1
}

fn error<T>(source: &str, at: usize, message: impl Into<String>) -> Result<T, SyntaxError> {
    Err(SyntaxError {
        line: line_of(source, at),
        message: message.into(),
    })
}

/// The tokens of `source`, ending with [`Token::Eof`], and the spans of its
/// comments.
pub(crate) fn tokenize(source: &str) -> Result<(Vec<Spanned>, Vec<Range<usize>>), SyntaxError> {
    let bytes = source.as_bytes();
    let mut tokens = Vec::new();
    let mut comments = Vec::new();
    let mut at = 0;
    while at < bytes.len() {
        let start = at;
        let byte = bytes[at];
        if byte.is_ascii_whitespace() {
            at += 1;
            continue;
        }
        if source[at..].starts_with("--") {
            at += 2;
            if let Some(level) = long_bracket(&source[at..]) {
                let close = format!("]{}]", "=".repeat(level));
                let open_len = level + 2;
                match source[at + open_len..].find(&close) {
                    Some(end) => at += open_len + end + close.len(),
                    None => return error(source, start, "unfinished long comment"),
                }
            } else {
                at = source[at..].find('\n').map_or(bytes.len(), |end| at + end);
            }
            comments.push(start..at);
            continue;
        }
        let token = if byte == b'_' || byte.is_ascii_alphabetic() {
            while at < bytes.len() && (bytes[at] == b'_' || bytes[at].is_ascii_alphanumeric()) {
                at += 1;
            }
            let word = &source[start..at];
            match KEYWORDS.iter().find(|keyword| **keyword == word) {
                Some(keyword) => Token::Keyword(keyword),
                None => Token::Name(word.to_owned()),
            }
        } else if byte.is_ascii_digit()
            || (byte == b'.' && bytes.get(at + 1).is_some_and(u8::is_ascii_digit))
        {
            let (value, end) = number(source, at);
            at = end;
            Token::Number(value)
        } else if byte == b'"' || byte == b'\'' {
            let (value, end) = quoted(source, at)?;
            at = end;
            Token::Str(value)
        } else if let Some(level) = long_bracket(&source[at..]) {
            let close = format!("]{}]", "=".repeat(level));
            let body_start = at + level + 2;
            let Some(end) = source[body_start..].find(&close) else {
                return error(source, start, "unfinished long string");
            };
            let mut text = &source[body_start..body_start + end];
            // A newline straight after the opening bracket isn't part of it.
            if let Some(rest) = text
                .strip_prefix("\r\n")
                .or_else(|| text.strip_prefix('\n'))
            {
                text = rest;
            }
            at = body_start + end + close.len();
            Token::Str(text.to_owned())
        } else if let Some(symbol) = SYMBOLS
            .iter()
            .find(|symbol| source[at..].starts_with(**symbol))
        {
            at += symbol.len();
            Token::Symbol(symbol)
        } else {
            let character = source[at..].chars().next().unwrap_or('?');
            return error(source, start, format!("unexpected character `{character}`"));
        };
        tokens.push(Spanned {
            token,
            span: start..at,
        });
    }
    tokens.push(Spanned {
        token: Token::Eof,
        span: bytes.len()..bytes.len(),
    });
    Ok((tokens, comments))
}

/// If `text` starts with `[[`, `[=[` and so on, how many `=` there are.
fn long_bracket(text: &str) -> Option<usize> {
    let rest = text.strip_prefix('[')?;
    let level = rest.bytes().take_while(|byte| *byte == b'=').count();
    (rest.as_bytes().get(level) == Some(&b'[')).then_some(level)
}

fn number(source: &str, start: usize) -> (Option<f64>, usize) {
    let bytes = source.as_bytes();
    let mut at = start;
    let hex = source[at..].starts_with("0x") || source[at..].starts_with("0X");
    if hex {
        at += 2;
    }
    while at < bytes.len() {
        let byte = bytes[at];
        let exponent = if hex {
            matches!(byte, b'p' | b'P')
        } else {
            matches!(byte, b'e' | b'E')
        };
        if exponent && matches!(bytes.get(at + 1), Some(b'+' | b'-')) {
            at += 2;
        } else if byte.is_ascii_alphanumeric() || byte == b'.' || byte == b'_' {
            at += 1;
        } else {
            break;
        }
    }
    let text = &source[start..at];
    let value = if hex {
        i64::from_str_radix(&text[2..], 16)
            .ok()
            .map(|value| value as f64)
    } else {
        text.parse::<f64>().ok()
    };
    (value.filter(|value| value.is_finite()), at)
}

fn quoted(source: &str, start: usize) -> Result<(String, usize), SyntaxError> {
    let quote = source.as_bytes()[start] as char;
    let mut value = String::new();
    let mut chars = source[start + 1..].char_indices().peekable();
    while let Some((offset, character)) = chars.next() {
        let at = start + 1 + offset;
        match character {
            '\n' => return error(source, at, "unfinished string"),
            '\\' => {
                let Some((_, escape)) = chars.next() else {
                    break;
                };
                match escape {
                    'n' | '\n' => value.push('\n'),
                    't' => value.push('\t'),
                    'r' => value.push('\r'),
                    'a' => value.push('\u{7}'),
                    'b' => value.push('\u{8}'),
                    'f' => value.push('\u{c}'),
                    'v' => value.push('\u{b}'),
                    '\\' | '"' | '\'' => value.push(escape),
                    'z' => {
                        while chars.peek().is_some_and(|(_, next)| next.is_whitespace()) {
                            chars.next();
                        }
                    }
                    'x' => {
                        let digits: String =
                            (0..2).filter_map(|_| chars.next()).map(|c| c.1).collect();
                        match u8::from_str_radix(&digits, 16) {
                            Ok(byte) => value.push(byte as char),
                            Err(_) => return error(source, at, "bad \\x escape"),
                        }
                    }
                    'u' => {
                        let mut digits = String::new();
                        if chars.next().map(|c| c.1) != Some('{') {
                            return error(source, at, "bad \\u escape");
                        }
                        for (_, next) in chars.by_ref() {
                            if next == '}' {
                                break;
                            }
                            digits.push(next);
                        }
                        match u32::from_str_radix(&digits, 16)
                            .ok()
                            .and_then(char::from_u32)
                        {
                            Some(character) => value.push(character),
                            None => return error(source, at, "bad \\u escape"),
                        }
                    }
                    digit if digit.is_ascii_digit() => {
                        let mut code = digit.to_digit(10).unwrap_or(0);
                        for _ in 0..2 {
                            match chars.peek() {
                                Some((_, next)) if next.is_ascii_digit() => {
                                    code = code * 10 + next.to_digit(10).unwrap_or(0);
                                    chars.next();
                                }
                                _ => break,
                            }
                        }
                        match char::from_u32(code) {
                            Some(character) if code < 256 => value.push(character),
                            _ => return error(source, at, "bad escape"),
                        }
                    }
                    _ => return error(source, at, format!("bad escape `\\{escape}`")),
                }
            }
            character if character == quote => return Ok((value, at + 1)),
            character => value.push(character),
        }
    }
    error(source, start, "unfinished string")
}

// ---- The syntax tree --------------------------------------------------------

#[derive(Clone, Debug)]
pub(crate) struct Expr {
    pub kind: ExprKind,
    pub span: Range<usize>,
}

#[derive(Clone, Debug)]
pub(crate) enum ExprKind {
    Nil,
    True,
    False,
    Vararg,
    Number(Option<f64>),
    Str(String),
    Name(String),
    Field(Box<Expr>, String),
    Index(Box<Expr>, Box<Expr>),
    Call(Box<Expr>, Vec<Expr>),
    Method(Box<Expr>, String, Vec<Expr>),
    Function(FunctionBody),
    Table,
    Binary(&'static str, Box<Expr>, Box<Expr>),
    Unary(&'static str, Box<Expr>),
    Paren(Box<Expr>),
}

#[derive(Clone, Debug)]
pub(crate) struct FunctionBody {
    pub params: Vec<String>,
    pub vararg: bool,
    pub body: Vec<Stat>,
}

#[derive(Clone, Debug)]
pub(crate) struct Stat {
    pub kind: StatKind,
    pub span: Range<usize>,
}

#[derive(Clone, Debug)]
pub(crate) enum StatKind {
    Comment,
    Local {
        names: Vec<String>,
        values: Vec<Expr>,
    },
    Assign {
        targets: Vec<Expr>,
        values: Vec<Expr>,
    },
    Call(Expr),
    If {
        arms: Vec<(Expr, Vec<Stat>)>,
        otherwise: Option<Vec<Stat>>,
    },
    While {
        condition: Expr,
        body: Vec<Stat>,
    },
    NumericFor {
        variable: String,
        start: Expr,
        stop: Expr,
        step: Option<Expr>,
        body: Vec<Stat>,
    },
    Do(Vec<Stat>),
    Return(Vec<Expr>),
    /// Anything the blocks have no use for the inside of: `repeat`, a
    /// generic `for`, function statements, `goto`, labels, `break`, `;`.
    Other,
}

/// Reads a whole chunk of Lua.
pub(crate) fn parse(source: &str) -> Result<Vec<Stat>, SyntaxError> {
    let (tokens, comments) = tokenize(source)?;
    let mut parser = Parser {
        source,
        tokens,
        at: 0,
        comments,
        next_comment: 0,
    };
    let body = parser.block()?;
    if parser.peek() != &Token::Eof {
        return parser.fail("expected the end of the script");
    }
    Ok(body)
}

struct Parser<'a> {
    source: &'a str,
    tokens: Vec<Spanned>,
    at: usize,
    comments: Vec<Range<usize>>,
    next_comment: usize,
}

/// Binary operators: their left and right priorities (Lua's own table).
fn binary(token: &Token) -> Option<(&'static str, u8, u8)> {
    let (op, left, right) = match token {
        Token::Keyword("or") => ("or", 1, 1),
        Token::Keyword("and") => ("and", 2, 2),
        Token::Symbol(op @ ("<" | ">" | "<=" | ">=" | "~=" | "==")) => (*op, 3, 3),
        Token::Symbol("|") => ("|", 4, 4),
        Token::Symbol("~") => ("~", 5, 5),
        Token::Symbol("&") => ("&", 6, 6),
        Token::Symbol(op @ ("<<" | ">>")) => (*op, 7, 7),
        Token::Symbol("..") => ("..", 9, 8),
        Token::Symbol(op @ ("+" | "-")) => (*op, 10, 10),
        Token::Symbol(op @ ("*" | "/" | "//" | "%")) => (*op, 11, 11),
        Token::Symbol("^") => ("^", 14, 13),
        _ => return None,
    };
    Some((op, left, right))
}

const UNARY_PRIORITY: u8 = 12;

impl Parser<'_> {
    fn peek(&self) -> &Token {
        &self.tokens[self.at].token
    }

    fn peek_at(&self, ahead: usize) -> &Token {
        let index = (self.at + ahead).min(self.tokens.len() - 1);
        &self.tokens[index].token
    }

    fn start(&self) -> usize {
        self.tokens[self.at].span.start
    }

    /// Where the last token taken ends.
    fn end(&self) -> usize {
        self.at
            .checked_sub(1)
            .map_or(0, |index| self.tokens[index].span.end)
    }

    fn next(&mut self) -> Token {
        let token = self.tokens[self.at].token.clone();
        if token != Token::Eof {
            self.at += 1;
        }
        token
    }

    fn fail<T>(&self, message: &str) -> Result<T, SyntaxError> {
        let found = match self.peek() {
            Token::Eof => "the end of the script".to_owned(),
            _ => format!("`{}`", &self.source[self.tokens[self.at].span.clone()]),
        };
        error(
            self.source,
            self.start(),
            format!("{message}, found {found}"),
        )
    }

    fn check(&self, symbol: &str) -> bool {
        matches!(self.peek(), Token::Symbol(found) | Token::Keyword(found) if *found == symbol)
    }

    fn accept(&mut self, symbol: &str) -> bool {
        let found = self.check(symbol);
        if found {
            self.next();
        }
        found
    }

    fn expect(&mut self, symbol: &str) -> Result<(), SyntaxError> {
        if self.accept(symbol) {
            Ok(())
        } else {
            self.fail(&format!("expected `{symbol}`"))
        }
    }

    fn name(&mut self) -> Result<String, SyntaxError> {
        match self.peek().clone() {
            Token::Name(name) => {
                self.next();
                Ok(name)
            }
            _ => self.fail("expected a name"),
        }
    }

    /// Comments before the next token, as statements of their own.
    fn comments_before_next(&mut self, body: &mut Vec<Stat>) {
        let before = self.start();
        while let Some(comment) = self.comments.get(self.next_comment) {
            if comment.start >= before {
                break;
            }
            body.push(Stat {
                kind: StatKind::Comment,
                span: comment.clone(),
            });
            self.next_comment += 1;
        }
    }

    fn block_ends(&self) -> bool {
        matches!(
            self.peek(),
            Token::Eof | Token::Keyword("end" | "else" | "elseif" | "until")
        )
    }

    fn block(&mut self) -> Result<Vec<Stat>, SyntaxError> {
        let mut body = Vec::new();
        loop {
            self.comments_before_next(&mut body);
            if self.block_ends() {
                return Ok(body);
            }
            if self.check("return") {
                let start = self.start();
                self.next();
                let mut values = Vec::new();
                if !self.block_ends() && !self.check(";") {
                    values = self.expression_list()?;
                }
                self.accept(";");
                body.push(Stat {
                    kind: StatKind::Return(values),
                    span: start..self.end(),
                });
                self.comments_before_next(&mut body);
                if !self.block_ends() {
                    return self.fail("`return` must be the last thing in its block");
                }
                return Ok(body);
            }
            let stat = self.statement()?;
            body.push(stat);
        }
    }

    fn statement(&mut self) -> Result<Stat, SyntaxError> {
        let start = self.start();
        let kind = match self.peek().clone() {
            Token::Symbol(";") => {
                self.next();
                StatKind::Other
            }
            Token::Symbol("::") => {
                self.next();
                self.name()?;
                self.expect("::")?;
                StatKind::Other
            }
            Token::Keyword("break") => {
                self.next();
                StatKind::Other
            }
            Token::Keyword("goto") => {
                self.next();
                self.name()?;
                StatKind::Other
            }
            Token::Keyword("do") => {
                self.next();
                let body = self.block()?;
                self.expect("end")?;
                StatKind::Do(body)
            }
            Token::Keyword("while") => {
                self.next();
                let condition = self.expression()?;
                self.expect("do")?;
                let body = self.block()?;
                self.expect("end")?;
                StatKind::While { condition, body }
            }
            Token::Keyword("repeat") => {
                self.next();
                self.block()?;
                self.expect("until")?;
                self.expression()?;
                StatKind::Other
            }
            Token::Keyword("if") => {
                self.next();
                let mut arms = Vec::new();
                let condition = self.expression()?;
                self.expect("then")?;
                arms.push((condition, self.block()?));
                let mut otherwise = None;
                loop {
                    if self.accept("elseif") {
                        let condition = self.expression()?;
                        self.expect("then")?;
                        arms.push((condition, self.block()?));
                    } else if self.accept("else") {
                        otherwise = Some(self.block()?);
                        self.expect("end")?;
                        break;
                    } else {
                        self.expect("end")?;
                        break;
                    }
                }
                StatKind::If { arms, otherwise }
            }
            Token::Keyword("for") => {
                self.next();
                let first = self.name()?;
                if self.accept("=") {
                    let start = self.expression()?;
                    self.expect(",")?;
                    let stop = self.expression()?;
                    let step = if self.accept(",") {
                        Some(self.expression()?)
                    } else {
                        None
                    };
                    self.expect("do")?;
                    let body = self.block()?;
                    self.expect("end")?;
                    StatKind::NumericFor {
                        variable: first,
                        start,
                        stop,
                        step,
                        body,
                    }
                } else {
                    while self.accept(",") {
                        self.name()?;
                    }
                    self.expect("in")?;
                    self.expression_list()?;
                    self.expect("do")?;
                    self.block()?;
                    self.expect("end")?;
                    StatKind::Other
                }
            }
            Token::Keyword("function") => {
                self.next();
                self.name()?;
                while self.accept(".") {
                    self.name()?;
                }
                if self.accept(":") {
                    self.name()?;
                }
                self.function_body()?;
                StatKind::Other
            }
            Token::Keyword("local") => {
                self.next();
                if self.accept("function") {
                    self.name()?;
                    self.function_body()?;
                    StatKind::Other
                } else {
                    let mut names = Vec::new();
                    let mut attributes = false;
                    loop {
                        names.push(self.name()?);
                        if self.accept("<") {
                            self.name()?;
                            self.expect(">")?;
                            attributes = true;
                        }
                        if !self.accept(",") {
                            break;
                        }
                    }
                    let values = if self.accept("=") {
                        self.expression_list()?
                    } else {
                        Vec::new()
                    };
                    if attributes {
                        StatKind::Other
                    } else {
                        StatKind::Local { names, values }
                    }
                }
            }
            _ => {
                let first = self.suffixed()?;
                if self.check("=") || self.check(",") {
                    let mut targets = vec![first];
                    while self.accept(",") {
                        targets.push(self.suffixed()?);
                    }
                    for target in &targets {
                        if !matches!(
                            target.kind,
                            ExprKind::Name(_) | ExprKind::Field(..) | ExprKind::Index(..)
                        ) {
                            return error(self.source, target.span.start, "can't assign to this");
                        }
                    }
                    self.expect("=")?;
                    let values = self.expression_list()?;
                    StatKind::Assign { targets, values }
                } else if matches!(first.kind, ExprKind::Call(..) | ExprKind::Method(..)) {
                    StatKind::Call(first)
                } else {
                    return error(
                        self.source,
                        first.span.start,
                        "expected a statement, such as a function call or an assignment",
                    );
                }
            }
        };
        Ok(Stat {
            kind,
            span: start..self.end(),
        })
    }

    fn function_body(&mut self) -> Result<FunctionBody, SyntaxError> {
        self.expect("(")?;
        let mut params = Vec::new();
        let mut vararg = false;
        if !self.check(")") {
            loop {
                if self.accept("...") {
                    vararg = true;
                    break;
                }
                params.push(self.name()?);
                if !self.accept(",") {
                    break;
                }
            }
        }
        self.expect(")")?;
        let body = self.block()?;
        self.expect("end")?;
        Ok(FunctionBody {
            params,
            vararg,
            body,
        })
    }

    fn expression_list(&mut self) -> Result<Vec<Expr>, SyntaxError> {
        let mut list = vec![self.expression()?];
        while self.accept(",") {
            list.push(self.expression()?);
        }
        Ok(list)
    }

    fn expression(&mut self) -> Result<Expr, SyntaxError> {
        self.subexpression(0)
    }

    fn subexpression(&mut self, limit: u8) -> Result<Expr, SyntaxError> {
        let start = self.start();
        let unary = match self.peek() {
            Token::Keyword("not") => Some("not"),
            Token::Symbol("-") => Some("-"),
            Token::Symbol("#") => Some("#"),
            Token::Symbol("~") => Some("~"),
            _ => None,
        };
        let mut left = if let Some(op) = unary {
            self.next();
            let operand = self.subexpression(UNARY_PRIORITY)?;
            Expr {
                kind: ExprKind::Unary(op, Box::new(operand)),
                span: start..self.end(),
            }
        } else {
            self.simple()?
        };
        while let Some((op, left_priority, right_priority)) = binary(self.peek()) {
            if left_priority <= limit {
                break;
            }
            self.next();
            let right = self.subexpression(right_priority)?;
            left = Expr {
                kind: ExprKind::Binary(op, Box::new(left), Box::new(right)),
                span: start..self.end(),
            };
        }
        Ok(left)
    }

    fn simple(&mut self) -> Result<Expr, SyntaxError> {
        let start = self.start();
        let kind = match self.peek().clone() {
            Token::Number(value) => ExprKind::Number(value),
            Token::Str(value) => ExprKind::Str(value),
            Token::Keyword("nil") => ExprKind::Nil,
            Token::Keyword("true") => ExprKind::True,
            Token::Keyword("false") => ExprKind::False,
            Token::Symbol("...") => ExprKind::Vararg,
            Token::Symbol("{") => {
                self.table()?;
                return Ok(Expr {
                    kind: ExprKind::Table,
                    span: start..self.end(),
                });
            }
            Token::Keyword("function") => {
                self.next();
                let body = self.function_body()?;
                return Ok(Expr {
                    kind: ExprKind::Function(body),
                    span: start..self.end(),
                });
            }
            _ => return self.suffixed(),
        };
        self.next();
        Ok(Expr {
            kind,
            span: start..self.end(),
        })
    }

    fn table(&mut self) -> Result<(), SyntaxError> {
        self.expect("{")?;
        while !self.check("}") {
            if self.accept("[") {
                self.expression()?;
                self.expect("]")?;
                self.expect("=")?;
                self.expression()?;
            } else if matches!(self.peek(), Token::Name(_))
                && self.peek_at(1) == &Token::Symbol("=")
            {
                self.next();
                self.next();
                self.expression()?;
            } else {
                self.expression()?;
            }
            if !self.accept(",") && !self.accept(";") {
                break;
            }
        }
        self.expect("}")
    }

    fn primary(&mut self) -> Result<Expr, SyntaxError> {
        let start = self.start();
        match self.peek().clone() {
            Token::Name(name) => {
                self.next();
                Ok(Expr {
                    kind: ExprKind::Name(name),
                    span: start..self.end(),
                })
            }
            Token::Symbol("(") => {
                self.next();
                let inner = self.expression()?;
                self.expect(")")?;
                Ok(Expr {
                    kind: ExprKind::Paren(Box::new(inner)),
                    span: start..self.end(),
                })
            }
            _ => self.fail("expected a value"),
        }
    }

    fn suffixed(&mut self) -> Result<Expr, SyntaxError> {
        let start = self.start();
        let mut expr = self.primary()?;
        loop {
            let kind = match self.peek().clone() {
                Token::Symbol(".") => {
                    self.next();
                    ExprKind::Field(Box::new(expr), self.name()?)
                }
                Token::Symbol("[") => {
                    self.next();
                    let key = self.expression()?;
                    self.expect("]")?;
                    ExprKind::Index(Box::new(expr), Box::new(key))
                }
                Token::Symbol(":") => {
                    self.next();
                    let method = self.name()?;
                    let args = self.arguments()?;
                    ExprKind::Method(Box::new(expr), method, args)
                }
                Token::Symbol("(" | "{") | Token::Str(_) => {
                    let args = self.arguments()?;
                    ExprKind::Call(Box::new(expr), args)
                }
                _ => return Ok(expr),
            };
            expr = Expr {
                kind,
                span: start..self.end(),
            };
        }
    }

    fn arguments(&mut self) -> Result<Vec<Expr>, SyntaxError> {
        let start = self.start();
        match self.peek().clone() {
            Token::Str(value) => {
                self.next();
                Ok(vec![Expr {
                    kind: ExprKind::Str(value),
                    span: start..self.end(),
                }])
            }
            Token::Symbol("{") => {
                self.table()?;
                Ok(vec![Expr {
                    kind: ExprKind::Table,
                    span: start..self.end(),
                }])
            }
            Token::Symbol("(") => {
                self.next();
                let args = if self.check(")") {
                    Vec::new()
                } else {
                    self.expression_list()?
                };
                self.expect(")")?;
                Ok(args)
            }
            _ => self.fail("expected function arguments"),
        }
    }
}

/// Whether the Lua in `code` uses the name `name` anywhere (not counting
/// fields, as in `a.name`).
pub(crate) fn mentions(code: &str, name: &str) -> bool {
    let Ok((tokens, _)) = tokenize(code) else {
        // Not whole Lua: look for the word.
        return code.contains(name);
    };
    tokens.iter().enumerate().any(|(index, token)| {
        token.token == Token::Name(name.to_owned())
            && !(index > 0 && matches!(tokens[index - 1].token, Token::Symbol("." | ":")))
    })
}
