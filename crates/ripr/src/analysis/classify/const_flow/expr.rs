//! A bounded reader and evaluator for the expressions the const value-flow
//! check (RIPR-SPEC-0048) is allowed to decide: a reader function's one tail
//! expression, a constant's initializer and a test's `assert!`/`assert_eq!`.
//!
//! Anything outside the small grammar below is unreadable, not approximated:
//! every entry point returns `None` and the caller keeps its abstention.

use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::analysis::classify) enum Value {
    Int(i64),
    Bool(bool),
    Str(String),
}

impl Value {
    /// The value as the test author would write it.
    pub(in crate::analysis::classify) fn render(&self) -> String {
        match self {
            Self::Int(value) => value.to_string(),
            Self::Bool(value) => value.to_string(),
            Self::Str(value) => format!("{value:?}"),
        }
    }

    fn display(&self) -> String {
        match self {
            Self::Str(value) => value.clone(),
            other => other.render(),
        }
    }
}

/// A declared type this evaluator understands. Integer ranges are clipped to
/// `i64`: a value outside it is unreadable rather than wrapped.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::analysis::classify) enum Ty {
    Int { min: i64, max: i64 },
    Bool,
    Str,
}

impl Ty {
    fn admits(self, value: &Value) -> bool {
        match (self, value) {
            (Self::Int { min, max }, Value::Int(v)) => (min..=max).contains(v),
            (Self::Bool, Value::Bool(_)) | (Self::Str, Value::Str(_)) => true,
            _ => false,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Token {
    Ident(String),
    Int(i64),
    Str(String),
    Lifetime,
    Punct(&'static str),
}

const PUNCT: [&str; 28] = [
    "&&", "||", "==", "!=", "<=", ">=", "->", "::", "(", ")", "[", "]", "{", "}", ",", ".", "+",
    "-", "*", "/", "%", "<", ">", "!", "&", ";", ":", "=",
];

fn tokenize(text: &str) -> Option<Vec<Token>> {
    let mut tokens = Vec::new();
    let mut rest = text;
    while let Some(ch) = rest.chars().next() {
        if ch.is_whitespace() {
            rest = &rest[ch.len_utf8()..];
        } else if let Some(after) = rest.strip_prefix("//") {
            rest = after.find('\n').map_or("", |at| &after[at..]);
        } else if let Some(after) = rest.strip_prefix("/*") {
            rest = &after[after.find("*/")? + 2..];
        } else if ch == '#' {
            // An attribute group is skipped; a `cfg` one makes the item
            // conditional, which this evaluator does not model.
            let after = rest.strip_prefix('#')?.trim_start();
            let after = after.strip_prefix('!').unwrap_or(after).strip_prefix('[')?;
            let close = after.find(']')?;
            if after[..close].contains("cfg") {
                return None;
            }
            rest = &after[close + 1..];
        } else if ch == '"' {
            let (value, after) = string_literal(&rest[1..])?;
            tokens.push(Token::Str(value));
            rest = after;
        } else if ch == '\'' {
            let after = &rest[1..];
            let end = after
                .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
                .unwrap_or(after.len());
            // `'static` is a lifetime; `'a'` is a char literal, unreadable.
            if end == 0 || after[end..].starts_with('\'') {
                return None;
            }
            tokens.push(Token::Lifetime);
            rest = &after[end..];
        } else if ch.is_ascii_digit() {
            let end = rest
                .find(|c: char| !(c.is_ascii_digit() || c == '_'))
                .unwrap_or(rest.len());
            let (digits, after) = rest.split_at(end);
            // A suffix (`1u32`), a radix prefix or a float is not read.
            if after.starts_with(|c: char| c.is_ascii_alphanumeric() || c == '.') {
                return None;
            }
            tokens.push(Token::Int(digits.replace('_', "").parse().ok()?));
            rest = after;
        } else if ch.is_ascii_alphabetic() || ch == '_' {
            let end = rest
                .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
                .unwrap_or(rest.len());
            let (word, after) = rest.split_at(end);
            // `r"raw"`, `b"bytes"`, `r#"..."#`: prefixed literals are not read.
            if after.starts_with(['"', '#']) && matches!(word, "r" | "b" | "br") {
                return None;
            }
            tokens.push(Token::Ident(word.to_string()));
            rest = after;
        } else {
            let punct = PUNCT.iter().find(|punct| rest.starts_with(**punct))?;
            tokens.push(Token::Punct(punct));
            rest = &rest[punct.len()..];
        }
    }
    Some(tokens)
}

/// The text of a string literal whose opening quote is already consumed, and
/// what follows its closing quote.
fn string_literal(text: &str) -> Option<(String, &str)> {
    let mut value = String::new();
    let mut chars = text.char_indices();
    while let Some((at, ch)) = chars.next() {
        match ch {
            '"' => return Some((value, &text[at + 1..])),
            '\\' => value.push(match chars.next()?.1 {
                'n' => '\n',
                't' => '\t',
                'r' => '\r',
                '\\' => '\\',
                '"' => '"',
                '\'' => '\'',
                _ => return None,
            }),
            other => value.push(other),
        }
    }
    None
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::analysis::classify) enum BinOp {
    Or,
    And,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    Add,
    Sub,
    Mul,
    Div,
    Rem,
}

impl BinOp {
    fn of(token: &Token) -> Option<(Self, u8)> {
        let Token::Punct(punct) = token else {
            return None;
        };
        Some(match *punct {
            "||" => (Self::Or, 1),
            "&&" => (Self::And, 2),
            "==" => (Self::Eq, 3),
            "!=" => (Self::Ne, 3),
            "<" => (Self::Lt, 3),
            "<=" => (Self::Le, 3),
            ">" => (Self::Gt, 3),
            ">=" => (Self::Ge, 3),
            "+" => (Self::Add, 4),
            "-" => (Self::Sub, 4),
            "*" => (Self::Mul, 5),
            "/" => (Self::Div, 5),
            "%" => (Self::Rem, 5),
            _ => return None,
        })
    }

    fn is_comparison(self) -> bool {
        matches!(
            self,
            Self::Eq | Self::Ne | Self::Lt | Self::Le | Self::Gt | Self::Ge
        )
    }
}

#[derive(Clone, Debug)]
pub(in crate::analysis::classify) enum Expr {
    Literal(Value),
    Var(String),
    Not(Box<Expr>),
    Neg(Box<Expr>),
    Bin(BinOp, Box<Expr>, Box<Expr>),
    Method(Box<Expr>, String, Vec<Expr>),
    Call(String, Vec<Expr>),
    Format(String, Vec<Expr>),
    /// `if cond { a } else { b }`: only a trait-default reader (the
    /// associated-constant extension) may contain one.
    If(Box<Expr>, Box<Expr>, Box<Expr>),
}

struct Parser<'a> {
    tokens: &'a [Token],
    at: usize,
    /// A trait default method's body: `Self::NAME` reads the constant and an
    /// `if`/`else` chain is evaluable. Off for every other expression.
    trait_body: bool,
}

impl Parser<'_> {
    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.at)
    }

    fn eat(&mut self, punct: &str) -> bool {
        if matches!(self.peek(), Some(Token::Punct(p)) if *p == punct) {
            self.at += 1;
            true
        } else {
            false
        }
    }

    fn expression(&mut self, min_precedence: u8) -> Option<Expr> {
        let mut left = self.unary()?;
        loop {
            let Some((op, precedence)) = self.peek().and_then(BinOp::of) else {
                return Some(left);
            };
            if precedence < min_precedence {
                return Some(left);
            }
            self.at += 1;
            let right = self.expression(precedence + 1)?;
            // `a < b < c` does not type-check in Rust; do not guess a reading.
            let compared =
                |expr: &Expr| matches!(expr, Expr::Bin(inner, ..) if inner.is_comparison());
            if op.is_comparison() && (compared(&left) || compared(&right)) {
                return None;
            }
            left = Expr::Bin(op, Box::new(left), Box::new(right));
        }
    }

    fn unary(&mut self) -> Option<Expr> {
        if self.eat("!") {
            return Some(Expr::Not(Box::new(self.unary()?)));
        }
        if self.eat("-") {
            return Some(Expr::Neg(Box::new(self.unary()?)));
        }
        let mut expr = self.primary()?;
        while self.eat(".") {
            let Some(Token::Ident(method)) = self.peek().cloned() else {
                return None;
            };
            self.at += 1;
            if !self.eat("(") {
                return None;
            }
            let args = self.arguments()?;
            expr = Expr::Method(Box::new(expr), method, args);
        }
        Some(expr)
    }

    /// Comma-separated expressions up to the `)` that closes the group.
    fn arguments(&mut self) -> Option<Vec<Expr>> {
        let mut args = Vec::new();
        while !self.eat(")") {
            args.push(self.expression(1)?);
            if !self.eat(",") && self.peek() != Some(&Token::Punct(")")) {
                return None;
            }
        }
        Some(args)
    }

    fn primary(&mut self) -> Option<Expr> {
        let token = self.peek()?.clone();
        self.at += 1;
        match token {
            Token::Int(value) => Some(Expr::Literal(Value::Int(value))),
            Token::Str(value) => Some(Expr::Literal(Value::Str(value))),
            Token::Punct("(") => {
                let inner = self.expression(1)?;
                self.eat(")").then_some(inner)
            }
            Token::Ident(word) if word == "true" || word == "false" => {
                Some(Expr::Literal(Value::Bool(word == "true")))
            }
            Token::Ident(word) if word == "if" && self.trait_body => self.if_expression(),
            Token::Ident(first) => {
                let first_word = first.clone();
                let mut name = first;
                let mut segments = 0usize;
                // A path reads as its last segment only for a call; a qualified
                // value (`Self::MAX`, `other::NAME`) is another item.
                let mut qualified = false;
                while self.eat("::") {
                    let Some(Token::Ident(next)) = self.peek().cloned() else {
                        return None;
                    };
                    self.at += 1;
                    name = next;
                    qualified = true;
                    segments += 1;
                }
                // `Self::NAME` in a trait default method names the constant
                // the impl binds; any longer or other-typed path stays unread.
                if self.trait_body
                    && qualified
                    && segments == 1
                    && first_word == "Self"
                    && self.peek() != Some(&Token::Punct("("))
                {
                    return Some(Expr::Var(name));
                }
                if self.eat("(") {
                    return Some(Expr::Call(name, self.arguments()?));
                }
                if qualified {
                    return None;
                }
                if name == "format" && self.eat("!") {
                    return self.format_macro();
                }
                Some(Expr::Var(name))
            }
            _ => None,
        }
    }

    /// The rest of an `if` whose keyword is consumed: `cond { a } else { b }`,
    /// the `else` branch another `if` or a block. A missing `else` is
    /// unreadable.
    fn if_expression(&mut self) -> Option<Expr> {
        let condition = self.expression(1)?;
        let then = self.block()?;
        if !matches!(self.peek(), Some(Token::Ident(word)) if word == "else") {
            return None;
        }
        self.at += 1;
        let otherwise = if matches!(self.peek(), Some(Token::Ident(word)) if word == "if") {
            self.at += 1;
            self.if_expression()?
        } else {
            self.block()?
        };
        Some(Expr::If(
            Box::new(condition),
            Box::new(then),
            Box::new(otherwise),
        ))
    }

    /// `{ expr }`.
    fn block(&mut self) -> Option<Expr> {
        if !self.eat("{") {
            return None;
        }
        let inner = self.expression(1)?;
        self.eat("}").then_some(inner)
    }

    fn format_macro(&mut self) -> Option<Expr> {
        if !self.eat("(") {
            return None;
        }
        let Some(Token::Str(template)) = self.peek().cloned() else {
            return None;
        };
        self.at += 1;
        let mut args = Vec::new();
        while !self.eat(")") {
            if !self.eat(",") {
                return None;
            }
            if self.eat(")") {
                break;
            }
            args.push(self.expression(1)?);
        }
        Some(Expr::Format(template, args))
    }
}

fn parse_tokens(tokens: &[Token]) -> Option<Expr> {
    parse_tokens_in(tokens, false)
}

fn parse_tokens_in(tokens: &[Token], trait_body: bool) -> Option<Expr> {
    let mut parser = Parser {
        tokens,
        at: 0,
        trait_body,
    };
    let expr = parser.expression(1)?;
    (parser.at == tokens.len()).then_some(expr)
}

impl Expr {
    /// Whether a reader body stays inside what `eval` computes: no calls, and
    /// only the methods it knows.
    fn is_closed(&self) -> bool {
        match self {
            Self::Literal(_) | Self::Var(_) => true,
            Self::Not(inner) | Self::Neg(inner) => inner.is_closed(),
            Self::Bin(_, left, right) => left.is_closed() && right.is_closed(),
            Self::Method(receiver, method, args) => {
                matches!(method.as_str(), "min" | "max" | "is_empty" | "len")
                    && receiver.is_closed()
                    && args.iter().all(Self::is_closed)
            }
            Self::Call(..) => false,
            Self::Format(_, args) => args.iter().all(Self::is_closed),
            Self::If(condition, then, otherwise) => {
                condition.is_closed() && then.is_closed() && otherwise.is_closed()
            }
        }
    }

    /// A helper or inlined initializer that `eval` can compute without a
    /// nested call, `format!`, or `if`. Format templates look up names in
    /// the caller's scope, which is not the helper's after inlining.
    fn is_value_closed(&self) -> bool {
        match self {
            Self::Literal(_) | Self::Var(_) => true,
            Self::Not(inner) | Self::Neg(inner) => inner.is_value_closed(),
            Self::Bin(_, left, right) => left.is_value_closed() && right.is_value_closed(),
            Self::Method(receiver, method, args) => {
                matches!(method.as_str(), "min" | "max" | "is_empty" | "len")
                    && receiver.is_value_closed()
                    && args.iter().all(Self::is_value_closed)
            }
            Self::Call(..) | Self::Format(..) | Self::If(..) => false,
        }
    }

    fn subst(&self, vars: &BTreeMap<&str, &Expr>) -> Expr {
        match self {
            Self::Literal(value) => Self::Literal(value.clone()),
            Self::Var(name) => vars
                .get(name.as_str())
                .map(|expr| (*expr).clone())
                .unwrap_or_else(|| self.clone()),
            Self::Not(inner) => Self::Not(Box::new(inner.subst(vars))),
            Self::Neg(inner) => Self::Neg(Box::new(inner.subst(vars))),
            Self::Bin(op, left, right) => {
                Self::Bin(*op, Box::new(left.subst(vars)), Box::new(right.subst(vars)))
            }
            Self::Method(receiver, method, args) => Self::Method(
                Box::new(receiver.subst(vars)),
                method.clone(),
                args.iter().map(|arg| arg.subst(vars)).collect(),
            ),
            Self::Call(name, args) => Self::Call(
                name.clone(),
                args.iter().map(|arg| arg.subst(vars)).collect(),
            ),
            Self::Format(template, args) => Self::Format(
                template.clone(),
                args.iter().map(|arg| arg.subst(vars)).collect(),
            ),
            Self::If(condition, then, otherwise) => Self::If(
                Box::new(condition.subst(vars)),
                Box::new(then.subst(vars)),
                Box::new(otherwise.subst(vars)),
            ),
        }
    }

    fn inline_helpers(&self, helpers: &BTreeMap<String, Reader>) -> Option<Expr> {
        match self {
            Self::Literal(_) | Self::Var(_) => Some(self.clone()),
            Self::Not(inner) => Some(Self::Not(Box::new(inner.inline_helpers(helpers)?))),
            Self::Neg(inner) => Some(Self::Neg(Box::new(inner.inline_helpers(helpers)?))),
            Self::Bin(op, left, right) => Some(Self::Bin(
                *op,
                Box::new(left.inline_helpers(helpers)?),
                Box::new(right.inline_helpers(helpers)?),
            )),
            Self::Method(receiver, method, args) => {
                let args: Option<Vec<Expr>> =
                    args.iter().map(|arg| arg.inline_helpers(helpers)).collect();
                Some(Self::Method(
                    Box::new(receiver.inline_helpers(helpers)?),
                    method.clone(),
                    args?,
                ))
            }
            Self::Call(name, args) => {
                let helper = helpers.get(name)?;
                let args: Option<Vec<Expr>> =
                    args.iter().map(|arg| arg.inline_helpers(helpers)).collect();
                helper.bind(&args?)
            }
            Self::Format(..) | Self::If(..) => None,
        }
    }

    /// The first call in source order, as its own expression.
    pub(in crate::analysis::classify) fn first_call(&self) -> Option<&Self> {
        match self {
            Self::Literal(_) | Self::Var(_) => None,
            Self::Not(inner) | Self::Neg(inner) => inner.first_call(),
            Self::Bin(_, left, right) => left.first_call().or_else(|| right.first_call()),
            Self::Method(receiver, _, args) => receiver
                .first_call()
                .or_else(|| args.iter().find_map(Self::first_call)),
            Self::Call(..) => Some(self),
            Self::Format(_, args) => args.iter().find_map(Self::first_call),
            Self::If(condition, then, otherwise) => condition
                .first_call()
                .or_else(|| then.first_call())
                .or_else(|| otherwise.first_call()),
        }
    }

    /// No call and no variable anywhere: the expression is its own literal value.
    fn is_literal_only(&self) -> bool {
        match self {
            Self::Literal(_) => true,
            Self::Var(_) | Self::Call(..) | Self::If(..) => false,
            Self::Not(inner) | Self::Neg(inner) => inner.is_literal_only(),
            Self::Bin(_, left, right) => left.is_literal_only() && right.is_literal_only(),
            Self::Method(receiver, _, args) => {
                receiver.is_literal_only() && args.iter().all(Self::is_literal_only)
            }
            Self::Format(_, args) => args.iter().all(Self::is_literal_only),
        }
    }

    /// A boolean assertion body in which each comparison has a literal side,
    /// joined by `&&`, `||` and `!`; a bare reader call is a boolean verdict.
    fn is_pinned(&self) -> bool {
        match self {
            Self::Not(inner) => inner.is_pinned(),
            Self::Bin(BinOp::And | BinOp::Or, left, right) => {
                (left.is_pinned() || left.is_literal_only())
                    && (right.is_pinned() || right.is_literal_only())
            }
            Self::Bin(
                BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge,
                left,
                right,
            ) => compares_to_literal(left, right),
            Self::Call(..) | Self::Method(..) => self.first_call().is_some(),
            _ => false,
        }
    }

    /// Every called name, in source order.
    pub(in crate::analysis::classify) fn call_names(&self, names: &mut Vec<String>) {
        match self {
            Self::Literal(_) | Self::Var(_) => {}
            Self::Not(inner) | Self::Neg(inner) => inner.call_names(names),
            Self::Bin(_, left, right) => {
                left.call_names(names);
                right.call_names(names);
            }
            Self::Method(receiver, _, args) => {
                receiver.call_names(names);
                args.iter().for_each(|arg| arg.call_names(names));
            }
            Self::Call(name, args) => {
                names.push(name.clone());
                args.iter().for_each(|arg| arg.call_names(names));
            }
            Self::Format(_, args) => args.iter().for_each(|arg| arg.call_names(names)),
            Self::If(condition, then, otherwise) => {
                condition.call_names(names);
                then.call_names(names);
                otherwise.call_names(names);
            }
        }
    }
}

fn int_type(word: &str) -> Option<Ty> {
    let (min, max) = match word {
        "u8" => (0, i64::from(u8::MAX)),
        "u16" => (0, i64::from(u16::MAX)),
        "u32" => (0, i64::from(u32::MAX)),
        "u64" | "usize" => (0, i64::MAX),
        "i8" => (i64::from(i8::MIN), i64::from(i8::MAX)),
        "i16" => (i64::from(i16::MIN), i64::from(i16::MAX)),
        "i32" => (i64::from(i32::MIN), i64::from(i32::MAX)),
        "i64" | "isize" => (i64::MIN, i64::MAX),
        _ => return None,
    };
    Some(Ty::Int { min, max })
}

fn parse_type(tokens: &[Token]) -> Option<Ty> {
    match tokens {
        [Token::Ident(word)] if word == "bool" => Some(Ty::Bool),
        [Token::Ident(word)] => int_type(word),
        [Token::Punct("&"), Token::Ident(word)] if word == "str" => Some(Ty::Str),
        [Token::Punct("&"), Token::Lifetime, Token::Ident(word)] if word == "str" => Some(Ty::Str),
        _ => None,
    }
}

/// A free function of one parameter list and one tail expression.
#[derive(Clone, Debug)]
pub(in crate::analysis::classify) struct Reader {
    pub(in crate::analysis::classify) name: String,
    params: Vec<(String, Ty)>,
    body: Expr,
}

impl Reader {
    pub(in crate::analysis::classify) fn from_parts(
        name: String,
        params: Vec<(String, Ty)>,
        body: Expr,
    ) -> Option<Self> {
        body.is_value_closed().then(|| Self { name, params, body })
    }

    pub(in crate::analysis::classify) fn is_value_closed(&self) -> bool {
        self.body.is_value_closed()
    }

    fn bind(&self, args: &[Expr]) -> Option<Expr> {
        if self.params.len() != args.len() || !self.body.is_value_closed() {
            return None;
        }
        let vars: BTreeMap<&str, &Expr> = self
            .params
            .iter()
            .map(|(name, _)| name.as_str())
            .zip(args.iter())
            .collect();
        let bound = self.body.subst(&vars);
        bound.is_value_closed().then_some(bound)
    }
}

struct FnSig {
    name: String,
    params: Vec<(String, Ty)>,
    body_open: usize,
}

fn parse_fn_sig(tokens: &[Token]) -> Option<FnSig> {
    let mut at = 0;
    if matches!(tokens.get(at), Some(Token::Ident(word)) if word == "pub") {
        at += 1;
        if tokens.get(at) == Some(&Token::Punct("(")) {
            at += 3;
        }
    }
    if !matches!(tokens.get(at), Some(Token::Ident(word)) if word == "fn") {
        return None;
    }
    let Some(Token::Ident(name)) = tokens.get(at + 1) else {
        return None;
    };
    if tokens.get(at + 2) != Some(&Token::Punct("(")) {
        return None;
    }
    let close = tokens
        .iter()
        .skip(at + 3)
        .position(|t| *t == Token::Punct(")"))?
        + at
        + 3;
    let mut params = Vec::new();
    let list = tokens.get(at + 3..close)?;
    for parameter in list.split(|t| *t == Token::Punct(",")) {
        match parameter {
            [] => {}
            [Token::Ident(param), Token::Punct(":"), ty @ ..] if param != "self" => {
                params.push((param.clone(), parse_type(ty)?));
            }
            _ => return None,
        }
    }
    let open = tokens
        .iter()
        .skip(close + 1)
        .position(|t| *t == Token::Punct("{"))?
        + close
        + 1;
    // The return type is any short run of tokens: it only has to be present
    // or absent, never read.
    if open - close > 5 {
        return None;
    }
    Some(FnSig {
        name: name.clone(),
        params,
        body_open: open,
    })
}

/// `fn name(a: T, ...) -> R {` including a `pub` prefix. The body may contain
/// statements; only the signature is read.
pub(in crate::analysis::classify) fn parse_fn_header(
    text: &str,
) -> Option<(String, Vec<(String, Ty)>)> {
    let tokens = tokenize(text)?;
    let sig = parse_fn_sig(&tokens)?;
    Some((sig.name, sig.params))
}

/// Parse `fn name(a: T, ...) -> R { expr }` (attributes and a `pub` prefix
/// allowed). Control flow, statements, generics, `self` and any other shape
/// are unreadable.
pub(in crate::analysis::classify) fn parse_reader(text: &str) -> Option<Reader> {
    parse_reader_in(text, false)
}

/// A trait default method with no receiver: `Self::NAME` is the constant and
/// its body may be an `if`/`else` chain (RIPR-SPEC-0048, associated-constant
/// extension).
pub(in crate::analysis::classify) fn parse_trait_reader(text: &str) -> Option<Reader> {
    parse_reader_in(text, true)
}

fn parse_reader_in(text: &str, trait_body: bool) -> Option<Reader> {
    let tokens = tokenize(text)?;
    let sig = parse_fn_sig(&tokens)?;
    let body = tokens.get(sig.body_open + 1..tokens.len().checked_sub(1)?)?;
    if tokens.last() != Some(&Token::Punct("}")) {
        return None;
    }
    let body = parse_tokens_in(body, trait_body)?;
    body.is_closed().then(|| Reader {
        name: sig.name,
        params: sig.params,
        body,
    })
}

/// `[pub] const|static NAME: T = INIT;` on one line.
pub(in crate::analysis::classify) struct ConstDecl {
    pub(in crate::analysis::classify) name: String,
    pub(in crate::analysis::classify) ty: Ty,
    pub(in crate::analysis::classify) init: Expr,
}

pub(in crate::analysis::classify) fn parse_const_decl(text: &str) -> Option<ConstDecl> {
    let tokens = tokenize(text)?;
    let mut at = 0;
    if matches!(tokens.get(at), Some(Token::Ident(word)) if word == "pub") {
        at += 1;
        if tokens.get(at) == Some(&Token::Punct("(")) {
            at += 3;
        }
    }
    if !matches!(tokens.get(at), Some(Token::Ident(word)) if word == "const" || word == "static") {
        return None;
    }
    let Some(Token::Ident(name)) = tokens.get(at + 1) else {
        return None;
    };
    if tokens.get(at + 2) != Some(&Token::Punct(":")) {
        return None;
    }
    let equals = tokens.iter().position(|t| *t == Token::Punct("="))?;
    let ty = parse_type(tokens.get(at + 3..equals)?)?;
    let init = tokens.get(equals + 1..tokens.len().checked_sub(1)?)?;
    if tokens.last() != Some(&Token::Punct(";")) {
        return None;
    }
    Some(ConstDecl {
        name: name.clone(),
        ty,
        init: parse_tokens(init)?,
    })
}

/// A parsed test assertion.
pub(in crate::analysis::classify) enum Assertion {
    True(Expr),
    Eq(Expr, Expr),
    Ne(Expr, Expr),
}

impl Assertion {
    /// Every comparison pits a reader's result against an expected value
    /// written out in the test: a literal-only expression, with no call and
    /// no constant. `assert_eq!(read(), read())` and `assert_eq!(read(), LIMIT)`
    /// move with the edit and pin nothing.
    pub(in crate::analysis::classify) fn has_literal_expectation(&self) -> bool {
        match self {
            Self::True(expr) => expr.is_pinned(),
            Self::Eq(left, right) | Self::Ne(left, right) => compares_to_literal(left, right),
        }
    }

    pub(in crate::analysis::classify) fn call_names(&self) -> Vec<String> {
        let mut names = Vec::new();
        match self {
            Self::True(expr) => expr.call_names(&mut names),
            Self::Eq(left, right) | Self::Ne(left, right) => {
                left.call_names(&mut names);
                right.call_names(&mut names);
            }
        }
        names
    }
}

fn compares_to_literal(left: &Expr, right: &Expr) -> bool {
    (left.is_literal_only() && right.first_call().is_some())
        || (right.is_literal_only() && left.first_call().is_some())
}

/// `assert!(e)`, `assert_eq!(a, b)` or `assert_ne!(a, b)`, with no message.
pub(in crate::analysis::classify) fn parse_assertion(text: &str) -> Option<Assertion> {
    let tokens = tokenize(text)?;
    let Some(Token::Ident(macro_name)) = tokens.first() else {
        return None;
    };
    if tokens.get(1) != Some(&Token::Punct("!")) {
        return None;
    }
    let mut end = tokens.len();
    if tokens.last() == Some(&Token::Punct(";")) {
        end -= 1;
    }
    let close = match tokens.get(2)? {
        Token::Punct("(") => Token::Punct(")"),
        Token::Punct("[") => Token::Punct("]"),
        _ => return None,
    };
    if tokens.get(end.checked_sub(1)?) != Some(&close) {
        return None;
    }
    let inner = tokens.get(3..end - 1)?;
    let mut args: Vec<&[Token]> = Vec::new();
    let mut depth = 0usize;
    let mut start = 0;
    for (at, token) in inner.iter().enumerate() {
        match token {
            Token::Punct("(" | "[" | "{") => depth += 1,
            Token::Punct(")" | "]" | "}") => depth = depth.checked_sub(1)?,
            Token::Punct(",") if depth == 0 => {
                args.push(inner.get(start..at)?);
                start = at + 1;
            }
            _ => {}
        }
    }
    let last = inner.get(start..)?;
    if !last.is_empty() {
        args.push(last);
    }
    let exprs: Option<Vec<Expr>> = args.into_iter().map(parse_tokens).collect();
    let mut exprs = exprs?.into_iter();
    match (
        macro_name.as_str(),
        exprs.next(),
        exprs.next(),
        exprs.next(),
    ) {
        ("assert", Some(a), None, None) => Some(Assertion::True(a)),
        ("assert_eq", Some(a), Some(b), None) => Some(Assertion::Eq(a, b)),
        ("assert_ne", Some(a), Some(b), None) => Some(Assertion::Ne(a, b)),
        _ => None,
    }
}

/// One constant bound to a value, and the readers a test may call.
pub(in crate::analysis::classify) struct Machine<'a> {
    pub(in crate::analysis::classify) readers: &'a [Reader],
    pub(in crate::analysis::classify) constant: &'a str,
    pub(in crate::analysis::classify) constant_ty: Ty,
    pub(in crate::analysis::classify) constant_value: &'a Value,
}

/// Names bound while evaluating, and the integer range every intermediate
/// result must stay inside (the tightest declared type involved).
struct Scope<'a> {
    vars: BTreeMap<&'a str, &'a Value>,
    lo: i64,
    hi: i64,
    calls: bool,
}

pub(in crate::analysis::classify) fn evaluate_initializer(expr: &Expr, ty: Ty) -> Option<Value> {
    let scope = Scope {
        vars: BTreeMap::new(),
        lo: i64::MIN,
        hi: i64::MAX,
        calls: false,
    };
    let value = eval(expr, &scope, None)?;
    ty.admits(&value).then_some(value)
}

impl Machine<'_> {
    /// Whether the assertion holds when the constant has this machine's value,
    /// or `None` when any part of it is unreadable.
    pub(in crate::analysis::classify) fn holds(&self, assertion: &Assertion) -> Option<bool> {
        let scope = Scope {
            vars: BTreeMap::new(),
            lo: i64::MIN,
            hi: i64::MAX,
            calls: true,
        };
        match assertion {
            Assertion::True(expr) => match eval(expr, &scope, Some(self))? {
                Value::Bool(holds) => Some(holds),
                _ => None,
            },
            Assertion::Eq(left, right) | Assertion::Ne(left, right) => {
                let left = eval(left, &scope, Some(self))?;
                let right = eval(right, &scope, Some(self))?;
                if std::mem::discriminant(&left) != std::mem::discriminant(&right) {
                    return None;
                }
                Some((left == right) == matches!(assertion, Assertion::Eq(..)))
            }
        }
    }

    /// The value the assertion's first reader call returns, for the
    /// explanation (not the value of the whole comparison). `None` when
    /// there is no call or it does not evaluate.
    pub(in crate::analysis::classify) fn call_result(
        &self,
        assertion: &Assertion,
    ) -> Option<Value> {
        let scope = Scope {
            vars: BTreeMap::new(),
            lo: i64::MIN,
            hi: i64::MAX,
            calls: true,
        };
        let expr = match assertion {
            Assertion::True(expr) | Assertion::Eq(expr, _) | Assertion::Ne(expr, _) => expr,
        };
        eval(expr.first_call()?, &scope, Some(self))
    }
}

fn eval(expr: &Expr, scope: &Scope<'_>, machine: Option<&Machine<'_>>) -> Option<Value> {
    let in_range = |value: i64| (scope.lo..=scope.hi).contains(&value);
    match expr {
        Expr::Literal(value) => Some(value.clone()),
        Expr::Var(name) => scope.vars.get(name.as_str()).map(|value| (*value).clone()),
        Expr::Not(inner) => match eval(inner, scope, machine)? {
            Value::Bool(value) => Some(Value::Bool(!value)),
            _ => None,
        },
        Expr::Neg(inner) => match eval(inner, scope, machine)? {
            Value::Int(value) => Some(Value::Int(value.checked_neg().filter(|v| in_range(*v))?)),
            _ => None,
        },
        Expr::Bin(op, left, right) => {
            let left = eval(left, scope, machine)?;
            if matches!(op, BinOp::And | BinOp::Or) {
                let Value::Bool(left) = left else {
                    return None;
                };
                if (*op == BinOp::And) != left {
                    return Some(Value::Bool(left));
                }
                return match eval(right, scope, machine)? {
                    Value::Bool(value) => Some(Value::Bool(value)),
                    _ => None,
                };
            }
            let right = eval(right, scope, machine)?;
            binary(*op, left, right).filter(|value| match value {
                Value::Int(v) => in_range(*v),
                _ => true,
            })
        }
        Expr::Method(receiver, method, args) => {
            let receiver = eval(receiver, scope, machine)?;
            let args: Option<Vec<Value>> =
                args.iter().map(|arg| eval(arg, scope, machine)).collect();
            match (receiver, method.as_str(), args?.as_slice()) {
                (Value::Int(a), "min", [Value::Int(b)]) => Some(Value::Int(a.min(*b))),
                (Value::Int(a), "max", [Value::Int(b)]) => Some(Value::Int(a.max(*b))),
                (Value::Str(text), "is_empty", []) => Some(Value::Bool(text.is_empty())),
                (Value::Str(text), "len", []) => Some(Value::Int(i64::try_from(text.len()).ok()?)),
                _ => None,
            }
        }
        Expr::Call(name, args) => {
            let machine = machine.filter(|_| scope.calls)?;
            let reader = machine.readers.iter().find(|reader| reader.name == *name)?;
            if reader.params.len() != args.len() {
                return None;
            }
            let values: Option<Vec<Value>> =
                args.iter().map(|arg| eval(arg, scope, None)).collect();
            let values = values?;
            let mut vars: BTreeMap<&str, &Value> = BTreeMap::new();
            let (mut lo, mut hi) = (i64::MIN, i64::MAX);
            let mut tighten = |ty: Ty| {
                if let Ty::Int { min, max } = ty {
                    lo = lo.max(min);
                    hi = hi.min(max);
                }
            };
            for ((param, ty), value) in reader.params.iter().zip(&values) {
                if !ty.admits(value) {
                    return None;
                }
                tighten(*ty);
                vars.insert(param.as_str(), value);
            }
            tighten(machine.constant_ty);
            // A parameter spelled like the constant is a shadow, not a read.
            if vars.contains_key(machine.constant) {
                return None;
            }
            vars.insert(machine.constant, machine.constant_value);
            let inner = Scope {
                vars,
                lo,
                hi,
                calls: false,
            };
            eval(&reader.body, &inner, None)
        }
        Expr::If(condition, then, otherwise) => match eval(condition, scope, machine)? {
            Value::Bool(true) => eval(then, scope, machine),
            Value::Bool(false) => eval(otherwise, scope, machine),
            _ => None,
        },
        Expr::Format(template, args) => {
            let args: Option<Vec<Value>> =
                args.iter().map(|arg| eval(arg, scope, machine)).collect();
            let args = args?;
            let mut positional = args.iter();
            let mut out = String::new();
            let mut chars = template.chars().peekable();
            while let Some(ch) = chars.next() {
                match (ch, chars.peek()) {
                    ('{', Some('{')) | ('}', Some('}')) => {
                        out.push(ch);
                        chars.next();
                    }
                    ('{', _) => {
                        let mut name = String::new();
                        loop {
                            match chars.next()? {
                                '}' => break,
                                c if c.is_ascii_alphanumeric() || c == '_' => name.push(c),
                                _ => return None,
                            }
                        }
                        if name.is_empty() {
                            out.push_str(&positional.next()?.display());
                        } else {
                            out.push_str(&scope.vars.get(name.as_str())?.display());
                        }
                    }
                    ('}', _) => return None,
                    _ => out.push(ch),
                }
            }
            // A surplus argument is a compile error in Rust: do not read it.
            positional.next().is_none().then_some(Value::Str(out))
        }
    }
}

fn binary(op: BinOp, left: Value, right: Value) -> Option<Value> {
    match (left, right) {
        (Value::Int(a), Value::Int(b)) => Some(match op {
            BinOp::Add => Value::Int(a.checked_add(b)?),
            BinOp::Sub => Value::Int(a.checked_sub(b)?),
            BinOp::Mul => Value::Int(a.checked_mul(b)?),
            BinOp::Div => Value::Int(a.checked_div(b)?),
            BinOp::Rem => Value::Int(a.checked_rem(b)?),
            BinOp::Eq => Value::Bool(a == b),
            BinOp::Ne => Value::Bool(a != b),
            BinOp::Lt => Value::Bool(a < b),
            BinOp::Le => Value::Bool(a <= b),
            BinOp::Gt => Value::Bool(a > b),
            BinOp::Ge => Value::Bool(a >= b),
            BinOp::And | BinOp::Or => return None,
        }),
        (Value::Str(a), Value::Str(b)) => match op {
            BinOp::Eq => Some(Value::Bool(a == b)),
            BinOp::Ne => Some(Value::Bool(a != b)),
            _ => None,
        },
        (Value::Bool(a), Value::Bool(b)) => match op {
            BinOp::Eq => Some(Value::Bool(a == b)),
            BinOp::Ne => Some(Value::Bool(a != b)),
            _ => None,
        },
        _ => None,
    }
}

pub(in crate::analysis::classify) fn parse_expression(text: &str) -> Option<Expr> {
    parse_tokens(&tokenize(text)?)
}

pub(in crate::analysis::classify) fn inline_helpers(
    expr: &Expr,
    helpers: &BTreeMap<String, Reader>,
) -> Option<Expr> {
    expr.inline_helpers(helpers)
}
