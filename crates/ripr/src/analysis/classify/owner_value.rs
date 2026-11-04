//! #12716: a pinned literal that tells neither version of the owner apart.
//!
//! An owner-return pin credits a test that compares the owner's call with a
//! literal. A literal pinned to the value the owner returned before the diff
//! discriminates the change: the test passes before and fails after. So does a
//! literal pinned to the value it returns after. A literal that is neither,
//! `assert_eq!(checked_div(17, 5), Ok(9))` against a tail that moved from
//! `Ok(4)` to `Ok(3)`, fails on both versions and cannot notice the change.
//!
//! This module answers one question over a deliberately closed subset: does
//! the owner, before and after the diff, run on the call's literal arguments,
//! provably return something other than the pinned value? Anything it cannot
//! evaluate, including a diff with no old line, answers "no evidence" and
//! leaves today's reading untouched, so the actionability flip stays
//! fail-closed and no shape it cannot judge gains a refusal.
//!
//! The subset:
//! - the owner is `fn name(p: int, ..) -> .. { guards; tail }` where every
//!   parameter is a plain `ident: <one integer type>`;
//! - a guard is `if COND { return RET; }`, a `COND` being comparisons of
//!   integer expressions joined by `&&` / `||`;
//! - an integer expression is decimal literals and parameters joined by
//!   `+ - * / %`, unary `-` and parentheses, checked in the parameter type;
//! - `RET` and the tail are an integer expression, `Ok(expr)`, `Some(expr)`,
//!   `Err(..)` or `None`.
//!
//! An overflow, a division by zero, a suffixed or non-decimal literal, a
//! statement of any other kind, a name that is not a parameter and any call
//! argument that is not a plain decimal literal make the answer "no evidence".
//! The shared `value_transfer` evaluator is untyped and credit-only, so it
//! cannot say that `u8` arithmetic overflowed; this one is typed.

/// A value the closed subset can compute.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Outcome {
    Plain(i128),
    Ok(i128),
    Some(i128),
    /// `Err(..)`: the payload is not evaluated.
    Err,
    None,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Token {
    Ident(String),
    Int(String),
    Sym(&'static str),
}

const SYMBOLS: [&str; 20] = [
    "==", "!=", "<=", ">=", "&&", "||", "(", ")", "{", "}", "<", ">", "+", "-", "*", "/", "%", ",",
    ";", ":",
];

fn lex(text: &str) -> Option<Vec<Token>> {
    let bytes = text.as_bytes();
    let mut tokens = Vec::new();
    let mut at = 0;
    while at < bytes.len() {
        let byte = bytes[at];
        if byte.is_ascii_whitespace() {
            at += 1;
        } else if byte.is_ascii_alphabetic() || byte == b'_' {
            let start = at;
            while at < bytes.len() && (bytes[at].is_ascii_alphanumeric() || bytes[at] == b'_') {
                at += 1;
            }
            tokens.push(Token::Ident(text[start..at].to_string()));
        } else if byte.is_ascii_digit() {
            let start = at;
            while at < bytes.len() && (bytes[at].is_ascii_digit() || bytes[at] == b'_') {
                at += 1;
            }
            // A suffix (`3u8`), a radix prefix (`0x1`) or a float tail
            // changes the type or the value this reader assumes.
            if at < bytes.len() && (bytes[at].is_ascii_alphanumeric() || bytes[at] == b'.') {
                return None;
            }
            let digits = &text[start..at];
            if digits.contains('_') {
                return None;
            }
            tokens.push(Token::Int(digits.to_string()));
        } else {
            let rest = &text[at..];
            // `->` is the one two-byte symbol only the signature uses.
            if rest.starts_with("->") {
                tokens.push(Token::Sym("->"));
                at += 2;
                continue;
            }
            let symbol = SYMBOLS.iter().find(|symbol| rest.starts_with(**symbol))?;
            tokens.push(Token::Sym(symbol));
            at += symbol.len();
        }
    }
    Some(tokens)
}

/// The inclusive range of a built-in integer type. `usize` / `isize` take the
/// 32-bit range so a result beyond it, which another target could wrap, is not
/// evaluated; `u128` stops at `i128::MAX` for the same reason.
fn range_of(ty: &str) -> Option<(i128, i128)> {
    Some(match ty {
        "i8" => (i8::MIN.into(), i8::MAX.into()),
        "i16" => (i16::MIN.into(), i16::MAX.into()),
        "i32" | "isize" => (i32::MIN.into(), i32::MAX.into()),
        "i64" => (i64::MIN.into(), i64::MAX.into()),
        "i128" => (i128::MIN, i128::MAX),
        "u8" => (0, u8::MAX.into()),
        "u16" => (0, u16::MAX.into()),
        "u32" | "usize" => (0, u32::MAX.into()),
        "u64" => (0, u64::MAX.into()),
        "u128" => (0, i128::MAX),
        _ => return None,
    })
}

#[derive(Clone, Debug)]
enum Expr {
    Literal(i128),
    Param(usize),
    Negate(Box<Expr>),
    Binary(Box<Expr>, char, Box<Expr>),
}

#[derive(Clone, Debug)]
enum Cond {
    Compare(Expr, &'static str, Expr),
    And(Box<Cond>, Box<Cond>),
    Or(Box<Cond>, Box<Cond>),
}

#[derive(Clone, Debug)]
enum Ret {
    Plain(Expr),
    Ok(Expr),
    Some(Expr),
    Err,
    None,
}

/// The owner as the closed subset reads it.
#[derive(Clone, Debug)]
pub(super) struct ClosedOwner {
    parameters: Vec<String>,
    range: (i128, i128),
    guards: Vec<(Cond, Ret)>,
    tail: Ret,
}

struct Parser<'a> {
    tokens: &'a [Token],
    at: usize,
    parameters: &'a [String],
}

impl<'a> Parser<'a> {
    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.at)
    }

    fn eat_sym(&mut self, symbol: &str) -> bool {
        if matches!(self.peek(), Some(Token::Sym(found)) if *found == symbol) {
            self.at += 1;
            true
        } else {
            false
        }
    }

    fn eat_ident(&mut self, word: &str) -> bool {
        if matches!(self.peek(), Some(Token::Ident(found)) if found == word) {
            self.at += 1;
            true
        } else {
            false
        }
    }

    fn expr(&mut self) -> Option<Expr> {
        let mut left = self.term()?;
        loop {
            let operator = if self.eat_sym("+") {
                '+'
            } else if self.eat_sym("-") {
                '-'
            } else {
                return Some(left);
            };
            left = Expr::Binary(Box::new(left), operator, Box::new(self.term()?));
        }
    }

    fn term(&mut self) -> Option<Expr> {
        let mut left = self.unary()?;
        loop {
            let operator = if self.eat_sym("*") {
                '*'
            } else if self.eat_sym("/") {
                '/'
            } else if self.eat_sym("%") {
                '%'
            } else {
                return Some(left);
            };
            left = Expr::Binary(Box::new(left), operator, Box::new(self.unary()?));
        }
    }

    fn unary(&mut self) -> Option<Expr> {
        if self.eat_sym("-") {
            return Some(Expr::Negate(Box::new(self.unary()?)));
        }
        if self.eat_sym("(") {
            let inner = self.expr()?;
            return self.eat_sym(")").then_some(inner);
        }
        let token = self.peek()?.clone();
        self.at += 1;
        match token {
            Token::Int(digits) => digits.parse().ok().map(Expr::Literal),
            Token::Ident(name) => self
                .parameters
                .iter()
                .position(|parameter| *parameter == name)
                .map(Expr::Param),
            Token::Sym(_) => None,
        }
    }

    fn compare(&mut self) -> Option<Cond> {
        let left = self.expr()?;
        let operator = ["==", "!=", "<=", ">=", "<", ">"]
            .into_iter()
            .find(|operator| self.eat_sym(operator))?;
        Some(Cond::Compare(left, operator, self.expr()?))
    }

    fn and(&mut self) -> Option<Cond> {
        let mut left = self.compare()?;
        while self.eat_sym("&&") {
            left = Cond::And(Box::new(left), Box::new(self.compare()?));
        }
        Some(left)
    }

    fn cond(&mut self) -> Option<Cond> {
        let mut left = self.and()?;
        while self.eat_sym("||") {
            left = Cond::Or(Box::new(left), Box::new(self.and()?));
        }
        Some(left)
    }

    /// The balanced `( .. )` that follows, skipped; every token in it was
    /// lexed, so a string or a macro inside already refused the owner.
    fn skip_group(&mut self) -> Option<()> {
        if !self.eat_sym("(") {
            return None;
        }
        let mut depth = 1usize;
        while depth > 0 {
            match self.peek()? {
                Token::Sym("(") => depth += 1,
                Token::Sym(")") => depth -= 1,
                _ => {}
            }
            self.at += 1;
        }
        Some(())
    }

    fn ret(&mut self) -> Option<Ret> {
        if self.eat_ident("Err") {
            self.skip_group()?;
            return Some(Ret::Err);
        }
        if self.eat_ident("None") {
            return Some(Ret::None);
        }
        for wrapper in ["Ok", "Some"] {
            if self.eat_ident(wrapper) {
                if !self.eat_sym("(") {
                    return None;
                }
                let payload = self.expr()?;
                if !self.eat_sym(")") {
                    return None;
                }
                return Some(if wrapper == "Ok" {
                    Ret::Ok(payload)
                } else {
                    Ret::Some(payload)
                });
            }
        }
        self.expr().map(Ret::Plain)
    }
}

impl ClosedOwner {
    /// Read `definition` (the owner's `fn` text) as the closed subset, or
    /// `None` when any part of it is outside it.
    pub(super) fn read(definition: &str, name: &str) -> Option<Self> {
        let masked = crate::analysis::extract::mask_comments_and_strings(definition);
        // Attributes and visibility before `fn` are not part of the subset's
        // concern: the reading starts at the `fn <name>` item itself.
        let item = masked.find(&format!("fn {name}"))?;
        let masked = &masked[item..];
        let open = masked.find('{')?;
        let close = masked.rfind('}')?;
        // Mismatched braces (a `}` before the first `{`) leave no body to slice.
        if close <= open {
            return None;
        }
        let signature = lex(&masked[..open])?;
        let fn_at = 0;
        if signature.get(fn_at + 1) != Some(&Token::Ident(name.to_string()))
            || signature.get(fn_at + 2) != Some(&Token::Sym("("))
        {
            return None;
        }
        let mut at = fn_at + 3;
        let mut parameters = Vec::new();
        let mut parameter_type: Option<String> = None;
        while signature.get(at) != Some(&Token::Sym(")")) {
            let (Some(Token::Ident(parameter)), Some(Token::Sym(":")), Some(Token::Ident(ty))) = (
                signature.get(at),
                signature.get(at + 1),
                signature.get(at + 2),
            ) else {
                return None;
            };
            range_of(ty)?;
            // Mixed integer types need casts, which the subset does not read.
            if parameter_type.as_ref().is_some_and(|seen| seen != ty) || parameter == "self" {
                return None;
            }
            parameter_type = Some(ty.clone());
            parameters.push(parameter.clone());
            at += 3;
            match signature.get(at) {
                Some(Token::Sym(",")) => at += 1,
                Some(Token::Sym(")")) => {}
                _ => return None,
            }
        }
        let range = range_of(parameter_type.as_deref()?)?;
        // Only `-> <type>` may follow the parameter list: no generics, no
        // `where`, no lifetimes.
        if signature.get(at + 1) != Some(&Token::Sym("->")) {
            return None;
        }
        let body = lex(&masked[open + 1..close])?;
        let mut parser = Parser {
            tokens: &body,
            at: 0,
            parameters: &parameters,
        };
        let mut guards = Vec::new();
        while parser.eat_ident("if") {
            let cond = parser.cond()?;
            if !(parser.eat_sym("{") && parser.eat_ident("return")) {
                return None;
            }
            let ret = parser.ret()?;
            if !(parser.eat_sym(";") && parser.eat_sym("}")) {
                return None;
            }
            guards.push((cond, ret));
        }
        let tail = parser.ret()?;
        (parser.at == body.len()).then_some(Self {
            parameters,
            range,
            guards,
            tail,
        })
    }

    fn eval(&self, expr: &Expr, arguments: &[i128]) -> Option<i128> {
        let (low, high) = self.range;
        let value = match expr {
            Expr::Literal(value) => *value,
            Expr::Param(position) => *arguments.get(*position)?,
            Expr::Negate(inner) => self.eval(inner, arguments)?.checked_neg()?,
            Expr::Binary(left, operator, right) => {
                let (left, right) = (self.eval(left, arguments)?, self.eval(right, arguments)?);
                match operator {
                    '+' => left.checked_add(right)?,
                    '-' => left.checked_sub(right)?,
                    '*' => left.checked_mul(right)?,
                    '/' => left.checked_div(right)?,
                    '%' => left.checked_rem(right)?,
                    _ => return None,
                }
            }
        };
        (low..=high).contains(&value).then_some(value)
    }

    fn holds(&self, cond: &Cond, arguments: &[i128]) -> Option<bool> {
        Some(match cond {
            Cond::Compare(left, operator, right) => {
                let (left, right) = (self.eval(left, arguments)?, self.eval(right, arguments)?);
                match *operator {
                    "==" => left == right,
                    "!=" => left != right,
                    "<=" => left <= right,
                    ">=" => left >= right,
                    "<" => left < right,
                    ">" => left > right,
                    _ => return None,
                }
            }
            // Both sides are evaluated even when the first decides: a side
            // the subset cannot read must not be skipped over.
            Cond::And(left, right) => {
                let (left, right) = (self.holds(left, arguments)?, self.holds(right, arguments)?);
                left && right
            }
            Cond::Or(left, right) => {
                let (left, right) = (self.holds(left, arguments)?, self.holds(right, arguments)?);
                left || right
            }
        })
    }

    fn outcome(&self, ret: &Ret, arguments: &[i128]) -> Option<Outcome> {
        Some(match ret {
            Ret::Plain(expr) => Outcome::Plain(self.eval(expr, arguments)?),
            Ret::Ok(expr) => Outcome::Ok(self.eval(expr, arguments)?),
            Ret::Some(expr) => Outcome::Some(self.eval(expr, arguments)?),
            Ret::Err => Outcome::Err,
            Ret::None => Outcome::None,
        })
    }

    fn run(&self, arguments: &[i128]) -> Option<Outcome> {
        if arguments.len() != self.parameters.len()
            || arguments
                .iter()
                .any(|argument| !(self.range.0..=self.range.1).contains(argument))
        {
            return None;
        }
        for (cond, ret) in &self.guards {
            if self.holds(cond, arguments)? {
                return self.outcome(ret, arguments);
            }
        }
        self.outcome(&self.tail, arguments)
    }

    /// Whether the owner, called as `call` (`name(17, 5)`), provably returns
    /// something other than the pinned `expected` (`Ok(4)`). `false` is "no
    /// evidence": the call or the expected value is outside the subset, or
    /// they agree.
    pub(super) fn provably_differs(&self, name: &str, call: &str, expected: &str) -> bool {
        let Some(arguments) = call_arguments(call, name) else {
            return false;
        };
        let (Some(returned), Some(pinned)) = (self.run(&arguments), literal_outcome(expected))
        else {
            return false;
        };
        match (returned, pinned) {
            (Outcome::Ok(a), Outcome::Ok(b))
            | (Outcome::Some(a), Outcome::Some(b))
            | (Outcome::Plain(a), Outcome::Plain(b)) => a != b,
            // The owner's own exit is the other variant of the pinned one.
            (Outcome::Err | Outcome::None, Outcome::Ok(_) | Outcome::Some(_)) => true,
            (Outcome::Ok(_), Outcome::Some(_))
            | (Outcome::Some(_), Outcome::Ok(_))
            | (Outcome::Plain(_), _)
            | (_, Outcome::Plain(_)) => false,
            // The payload of an `Err(..)` is not evaluated, and a variant the
            // owner returns matches a pin on that variant.
            (Outcome::Err | Outcome::None, Outcome::Err | Outcome::None)
            | (Outcome::Ok(_) | Outcome::Some(_), Outcome::Err | Outcome::None) => false,
        }
    }
}

/// The owner before and after the diff's changed line.
#[derive(Clone, Debug)]
pub(super) struct ClosedChange {
    after: ClosedOwner,
    before: ClosedOwner,
}

impl ClosedChange {
    /// Both versions of `definition`, the second with line `changed_line`
    /// (0-based within the definition) replaced by the diff's old text.
    pub(super) fn read(
        definition: &str,
        name: &str,
        changed_line: usize,
        before: &str,
    ) -> Option<Self> {
        // One old line replaces one physical line; a multi-line old text could
        // build an owner unrelated to the real one.
        if before.trim().is_empty() || before.contains('\n') {
            return None;
        }
        let mut lines: Vec<&str> = definition.lines().collect();
        *lines.get_mut(changed_line)? = before.trim_end();
        Some(Self {
            after: ClosedOwner::read(definition, name)?,
            before: ClosedOwner::read(&lines.join("\n"), name)?,
        })
    }

    /// #12394 item 5: the two versions of one struct field's initializer,
    /// each read as a scalar owner over the same integer parameters, so a
    /// pinned value for `field` that neither version computes for `call` is
    /// provably unpinned. `field_ty` is the field's declared integer type; the
    /// parameters must have the same range, since the subset checks arithmetic
    /// in one type. The initializer is the text after `field:` on the changed
    /// line (`before` for the old version); anything outside the closed subset
    /// answers `None` and keeps today's reading.
    pub(super) fn read_field(
        definition: &str,
        name: &str,
        changed_line: usize,
        before: &str,
        field: &str,
        field_ty: &str,
    ) -> Option<Self> {
        if before.trim().is_empty() || before.contains('\n') {
            return None;
        }
        let after_line = definition.lines().nth(changed_line)?;
        Some(Self {
            after: scalar_field_owner(definition, name, after_line, field, field_ty)?,
            before: scalar_field_owner(definition, name, before, field, field_ty)?,
        })
    }

    /// Whether `expected` is a value neither version provably returns for
    /// `call`. `false` is "no evidence", or a version agrees with the pin.
    pub(super) fn provably_unpinned(&self, name: &str, call: &str, expected: &str) -> bool {
        self.after.provably_differs(name, call, expected)
            && self.before.provably_differs(name, call, expected)
    }
}

/// The owner `fn name(<its parameters>) -> field_ty { <initializer of field on
/// line> }`: the struct field read as the scalar the subset evaluates.
fn scalar_field_owner(
    definition: &str,
    name: &str,
    line: &str,
    field: &str,
    field_ty: &str,
) -> Option<ClosedOwner> {
    let range = range_of(field_ty)?;
    let initializer = field_initializer(line, field)?;
    let masked = crate::analysis::extract::mask_comments_and_strings(definition);
    let item = masked.find(&format!("fn {name}"))?;
    let signature_end = masked[item..].find(')')? + item;
    let source = format!(
        "{} ) -> {field_ty} {{ {initializer} }}",
        masked[item..signature_end].trim_end()
    );
    let owner = ClosedOwner::read(&source, name)?;
    (owner.range == range && owner.guards.is_empty() && matches!(owner.tail, Ret::Plain(_)))
        .then_some(owner)
}

/// The text of `field`'s initializer on `line`: after `field:` up to the
/// top-level `,` or the bracket that closes the literal. A line with a string,
/// a char, a comment or a second mention of the field is not read.
fn field_initializer(line: &str, field: &str) -> Option<String> {
    if line.contains(['"', '\'']) || line.contains("//") || line.contains("/*") {
        return None;
    }
    let bytes = line.as_bytes();
    let mut found = None;
    let mut from = 0;
    while let Some(offset) = line[from..].find(field) {
        let start = from + offset;
        let end = start + field.len();
        from = end;
        let before_ok =
            start == 0 || !(bytes[start - 1].is_ascii_alphanumeric() || bytes[start - 1] == b'_');
        let rest = line[end..].trim_start();
        if before_ok && rest.starts_with(':') && !rest.starts_with("::") {
            if found.is_some() {
                return None;
            }
            found = Some(&rest[1..]);
        }
    }
    let rest = found?;
    let mut depth = 0usize;
    let mut stop = rest.len();
    for (at, byte) in rest.bytes().enumerate() {
        match byte {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' if depth == 0 => {
                stop = at;
                break;
            }
            b')' | b']' | b'}' => depth -= 1,
            b',' if depth == 0 => {
                stop = at;
                break;
            }
            _ => {}
        }
    }
    let text = rest[..stop].trim();
    (!text.is_empty()).then(|| text.to_string())
}

/// `name(17, 5)` (optionally with a trailing comma and line breaks) as
/// integer arguments. Anything else, a method receiver, a path, a name or an
/// expression argument included, is not read.
fn call_arguments(call: &str, name: &str) -> Option<Vec<i128>> {
    let tokens = lex(call)?;
    let mut tokens = tokens.iter().peekable();
    if tokens.next()? != &Token::Ident(name.to_string()) || tokens.next()? != &Token::Sym("(") {
        return None;
    }
    let mut arguments = Vec::new();
    loop {
        match tokens.next()? {
            Token::Sym(")") => break,
            Token::Sym("-") => match tokens.next()? {
                Token::Int(digits) => arguments.push(-digits.parse::<i128>().ok()?),
                _ => return None,
            },
            Token::Int(digits) => arguments.push(digits.parse().ok()?),
            _ => return None,
        }
        match tokens.next()? {
            Token::Sym(",") => {}
            Token::Sym(")") => break,
            _ => return None,
        }
    }
    tokens.next().is_none().then_some(arguments)
}

/// The pinned value as a literal: `4`, `-4`, `Ok(4)`, `Some(4)`, `Err(..)`
/// or `None`. An `Err(..)` pin is read without its payload.
fn literal_outcome(expected: &str) -> Option<Outcome> {
    let tokens = lex(expected)?;
    let signed = |tokens: &[Token]| -> Option<i128> {
        match tokens {
            [Token::Int(digits)] => digits.parse().ok(),
            [Token::Sym("-"), Token::Int(digits)] => digits.parse::<i128>().ok().map(|n| -n),
            _ => None,
        }
    };
    match tokens.as_slice() {
        [Token::Ident(word)] if word == "None" => Some(Outcome::None),
        [
            Token::Ident(word),
            Token::Sym("("),
            inner @ ..,
            Token::Sym(")"),
        ] => match word.as_str() {
            "Ok" => signed(inner).map(Outcome::Ok),
            "Some" => signed(inner).map(Outcome::Some),
            "Err" => Some(Outcome::Err),
            _ => None,
        },
        other => signed(other).map(Outcome::Plain),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DIV: &str = "pub fn checked_div(a: u32, b: u32) -> Result<u32, ()> {\n    if b == 0 {\n        return Err(());\n    }\n    Ok(a / b)\n}\n";

    fn differs(definition: &str, name: &str, call: &str, expected: &str) -> bool {
        ClosedOwner::read(definition, name)
            .is_some_and(|owner| owner.provably_differs(name, call, expected))
    }

    const CONFIG: &str =
        "pub fn config(n: u32) -> Result<Config, ()> {\n    Ok(Config { retries: n + 1 })\n}\n";

    fn field_change(before: &str, ty: &str) -> Option<ClosedChange> {
        ClosedChange::read_field(CONFIG, "config", 1, before, "retries", ty)
    }

    /// #12394 item 5: a struct field's initializer is read as a scalar owner.
    #[test]
    fn a_field_initializer_is_unpinned_only_when_neither_version_computes_it() {
        let change = field_change("    Ok(Config { retries: n + 2 })", "u32");
        assert!(change.is_some(), "premise: both versions read");
        let Some(change) = change else {
            return;
        };
        assert!(change.provably_unpinned("config", "config(2)", "9"));
        // The new value (3) and the old value (4) each tell the versions apart.
        assert!(!change.provably_unpinned("config", "config(2)", "3"));
        assert!(!change.provably_unpinned("config", "config(2)", "4"));
        // A call outside the subset, or a non-decimal pin, is no evidence.
        assert!(!change.provably_unpinned("config", "config(n)", "9"));
        assert!(!change.provably_unpinned("config", "config(2)", "limit"));
    }

    #[test]
    fn a_field_initializer_outside_the_subset_is_not_read() {
        // A different integer type for the field than for the parameters.
        assert!(field_change("    Ok(Config { retries: n + 2 })", "u8").is_none());
        // A non-integer field type, a string, a call, a second mention.
        assert!(field_change("    Ok(Config { retries: n + 2 })", "String").is_none());
        assert!(field_change("    Ok(Config { retries: \"a\" })", "u32").is_none());
        assert!(field_change("    Ok(Config { retries: f(n) })", "u32").is_none());
        assert!(field_change("    Ok(Config { retries: n, retries: 1 })", "u32").is_none());
        // A multi-line old text, or none.
        assert!(field_change("", "u32").is_none());
        assert!(field_change("a\nb", "u32").is_none());
    }

    #[test]
    fn the_initializer_stops_at_the_literal_s_closing_bracket_or_comma() {
        let read = |line, field| field_initializer(line, field);
        assert_eq!(
            read("Ok(Config { retries: n + 1 })", "retries").as_deref(),
            Some("n + 1")
        );
        assert_eq!(
            read("    retries: (n + 1) * 2,", "retries").as_deref(),
            Some("(n + 1) * 2")
        );
        // A longer field name is another field.
        assert_eq!(read("Ok(Config { max_retries: 4 })", "retries"), None);
        assert_eq!(read("retries::new(1)", "retries"), None);
    }

    #[test]
    fn a_wrong_literal_provably_differs_and_the_right_one_does_not() {
        assert!(differs(DIV, "checked_div", "checked_div(17, 5)", "Ok(4)"));
        assert!(!differs(DIV, "checked_div", "checked_div(17, 5)", "Ok(3)"));
    }

    #[test]
    fn a_guard_exit_is_the_other_variant() {
        assert!(differs(DIV, "checked_div", "checked_div(17, 0)", "Ok(0)"));
        assert!(!differs(
            DIV,
            "checked_div",
            "checked_div(17, 0)",
            "Err(())"
        ));
    }

    #[test]
    fn multi_line_calls_read_like_single_line_ones() {
        assert!(differs(
            DIV,
            "checked_div",
            "checked_div(\n    17,\n    5,\n)",
            "Ok(4)"
        ));
    }

    #[test]
    fn anything_outside_the_subset_is_no_evidence() {
        // overflow in the parameter type
        let add = "fn add(a: u8, b: u8) -> u8 {\n    a + b\n}\n";
        assert!(!differs(add, "add", "add(200, 100)", "7"));
        assert!(differs(add, "add", "add(2, 3)", "6"));
        // division by zero without a guard
        let div = "fn q(a: u32, b: u32) -> u32 {\n    a / b\n}\n";
        assert!(!differs(div, "q", "q(1, 0)", "9"));
        // a suffixed literal, a name, an expression argument
        assert!(!differs(
            DIV,
            "checked_div",
            "checked_div(17u32, 5)",
            "Ok(4)"
        ));
        assert!(!differs(DIV, "checked_div", "checked_div(x, 5)", "Ok(4)"));
        assert!(!differs(
            DIV,
            "checked_div",
            "checked_div(16 + 1, 5)",
            "Ok(4)"
        ));
        // a method call, a cast, a let, a loop in the owner
        for body in [
            "a.pow(2)",
            "(a as u32) + b",
            "let c = a + b;\n    c",
            "for _ in 0..a {}\n    a",
        ] {
            let owner = format!("fn f(a: u32, b: u32) -> u32 {{\n    {body}\n}}\n");
            assert!(!differs(&owner, "f", "f(1, 2)", "99"), "{body}");
        }
        // mixed parameter types, a generic, a receiver
        assert!(ClosedOwner::read("fn f(a: u32, b: u64) -> u64 { a }", "f").is_none());
        assert!(ClosedOwner::read("fn f<T>(a: u32) -> u32 { a }", "f").is_none());
        assert!(ClosedOwner::read("fn f(&self, a: u32) -> u32 { a }", "f").is_none());
    }

    #[test]
    fn a_pinned_variant_the_owner_returns_is_not_a_mismatch() {
        let half = "fn half(n: u32) -> Option<u32> {\n    if n % 2 == 1 {\n        return None;\n    }\n    Some(n / 2)\n}\n";
        assert!(differs(half, "half", "half(4)", "Some(3)"));
        assert!(!differs(half, "half", "half(4)", "Some(2)"));
        assert!(differs(half, "half", "half(5)", "Some(2)"));
        assert!(!differs(half, "half", "half(5)", "None"));
        // a plain literal against a wrapped return does not compile
        assert!(!differs(half, "half", "half(4)", "3"));
    }

    #[test]
    fn signed_arithmetic_truncates_toward_zero() {
        let neg = "fn q(a: i32, b: i32) -> i32 {\n    a / b\n}\n";
        assert!(!differs(neg, "q", "q(-7, 2)", "-3"));
        assert!(differs(neg, "q", "q(-7, 2)", "-4"));
        let rem = "fn r(a: i32, b: i32) -> i32 {\n    a % b\n}\n";
        assert!(!differs(rem, "r", "r(-7, 2)", "-1"));
    }

    #[test]
    fn mismatched_braces_are_no_evidence_not_a_panic() {
        assert!(ClosedOwner::read("fn f(a: u32) -> u32 } a {", "f").is_none());
        assert!(ClosedOwner::read("fn f(a: u32) -> u32 {", "f").is_none());
    }

    #[test]
    fn a_literal_neither_version_returns_is_unpinned() {
        // The tail moved from `Ok(a / b + 1)` (4 for 17, 5) to `Ok(a / b)` (3).
        let change = ClosedChange::read(DIV, "checked_div", 4, "    Ok(a / b + 1)");
        assert!(change.is_some(), "both versions are in the subset");
        let Some(change) = change else { return };
        let unpinned = |expected: &str| {
            change.provably_unpinned("checked_div", "checked_div(17, 5)", expected)
        };
        assert!(!unpinned("Ok(3)"), "the new value");
        assert!(!unpinned("Ok(4)"), "the old value");
        assert!(unpinned("Ok(9)"));
        // an old line outside the subset reads no evidence
        assert!(ClosedChange::read(DIV, "checked_div", 4, "    Ok(a.pow(2))").is_none());
        // a multi-line or empty old text is not one replaced line
        assert!(
            ClosedChange::read(DIV, "checked_div", 4, "    let c = 1;\n    Ok(a / b + c)")
                .is_none()
        );
        assert!(ClosedChange::read(DIV, "checked_div", 4, "  ").is_none());
    }
}
