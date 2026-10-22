//! RIPR-SPEC-0246 rule 5: an edit to a non-first operand of a returned `??`
//! chain needs a tested input that reaches it.
//!
//! In `return p?.nick ?? p?.name ?? "anonymous"` the middle operand decides
//! the result only when `p.nick` is nullish. A test that passes
//! `{ nick, name }` and `undefined` returns the same value from both versions
//! of a changed middle operand, however exact the matcher is. Everything the
//! reader cannot parse reads as `Unknown`, which keeps the ladder result.

use super::spread_override_guard::{
    LiteralValue, is_identifier, key_value, literal_value, object_entries,
};

/// What one operand evaluates to for a tested input.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Operand {
    /// `param?.prop` or `param.prop`.
    Read { prop: String, optional: bool },
    /// A literal fallback.
    Literal(LiteralValue),
}

/// A returned `??` chain whose lines differ in exactly one non-first operand.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct NullishChainChange {
    /// The owner parameter every property read is on.
    pub(crate) subject: String,
    /// The operands before the changed one, all property reads.
    earlier: Vec<Operand>,
    old: Operand,
    new: Operand,
    /// The changed operand ends the chain, so its value is returned as is
    /// and `null` differs from `undefined`.
    last: bool,
}

/// What one tested argument shows about the changed operand.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ChainInput {
    /// The earlier operands are nullish and the versions read different values.
    Differs,
    /// An earlier operand decides the result, or both versions agree there.
    Unreached,
    /// The argument cannot be read.
    Unknown,
}

/// A property value read from an object literal.
enum Value {
    Null,
    Undefined,
    Literal(LiteralValue),
    Unknown,
}

impl NullishChainChange {
    /// Read the change, or `None` for anything but two returned chains of
    /// `??` operands (no `||`, `&&`, ternary or call) that differ in one
    /// non-first operand, where earlier operands are reads of one parameter.
    pub(crate) fn read(old_line: &str, new_line: &str, params: &[String]) -> Option<Self> {
        let old = chain_operands(old_line)?;
        let new = chain_operands(new_line)?;
        if old.len() != new.len() || old.len() < 2 {
            return None;
        }
        let mut differing = (0..old.len()).filter(|at| old[*at] != new[*at]);
        let at = differing.next()?;
        if differing.next().is_some() || at == 0 {
            return None;
        }
        let mut subject: Option<String> = None;
        let mut parse = |text: &str, must_read: bool| -> Option<Operand> {
            if let Some(literal) = literal_value(text) {
                return (!must_read).then_some(Operand::Literal(literal));
            }
            let (param, prop, optional) = property_read(text)?;
            if !params.contains(&param) || subject.as_ref().is_some_and(|s| *s != param) {
                return None;
            }
            subject = Some(param);
            Some(Operand::Read { prop, optional })
        };
        let earlier = old[..at]
            .iter()
            .map(|text| parse(text, true))
            .collect::<Option<Vec<_>>>()?;
        let old_operand = parse(&old[at], false)?;
        let new_operand = parse(&new[at], false)?;
        Some(Self {
            subject: subject?,
            earlier,
            old: old_operand,
            new: new_operand,
            last: at + 1 == old.len(),
        })
    }

    /// What the argument expression `argument` (an object literal as written
    /// in a test, or `undefined` / `null`) shows about the changed operand.
    pub(crate) fn input(&self, argument: &str) -> ChainInput {
        let argument = argument.trim();
        let nullish_argument = argument == "undefined" || argument == "null";
        let entries = if nullish_argument {
            None
        } else {
            match object_entries(argument) {
                Some(entries) => Some(entries),
                None => return ChainInput::Unknown,
            }
        };
        let value = |operand: &Operand| -> Value {
            match operand {
                Operand::Literal(literal) => Value::Literal(literal.clone()),
                Operand::Read { prop, optional } => match &entries {
                    // `undefined.prop` throws; only `?.` reads nullish.
                    None if *optional => Value::Undefined,
                    None => Value::Unknown,
                    Some(entries) => property_value(entries, prop),
                },
            }
        };
        for operand in &self.earlier {
            match value(operand) {
                Value::Null | Value::Undefined => {}
                Value::Literal(_) => return ChainInput::Unreached,
                Value::Unknown => return ChainInput::Unknown,
            }
        }
        match (value(&self.old), value(&self.new)) {
            (Value::Unknown, _) | (_, Value::Unknown) => ChainInput::Unknown,
            (Value::Null, Value::Null) | (Value::Undefined, Value::Undefined) => {
                ChainInput::Unreached
            }
            // Both fall through to the next operand, unless this is the last.
            (Value::Null | Value::Undefined, Value::Null | Value::Undefined) if !self.last => {
                ChainInput::Unreached
            }
            (Value::Literal(a), Value::Literal(b)) if a == b => ChainInput::Unreached,
            _ => ChainInput::Differs,
        }
    }
}

/// The `prop` entry of an object literal's entries.
fn property_value(entries: &[String], prop: &str) -> Value {
    let mut found: Option<Value> = None;
    for entry in entries {
        if entry.starts_with("...") {
            return Value::Unknown;
        }
        match key_value(entry) {
            Some((key, text)) if key == prop => {
                found = Some(match literal_value(&text) {
                    Some(LiteralValue::Word("null")) => Value::Null,
                    Some(literal) => Value::Literal(literal),
                    None if text == "undefined" => Value::Undefined,
                    None => Value::Unknown,
                });
            }
            Some(_) => {}
            None if is_identifier(entry) && entry != prop => {}
            None => return Value::Unknown,
        }
    }
    match found {
        Some(value) => value,
        // A missing own key still reads an inherited member.
        None if OBJECT_PROTOTYPE_MEMBERS.contains(&prop) => Value::Unknown,
        None => Value::Undefined,
    }
}

const OBJECT_PROTOTYPE_MEMBERS: [&str; 8] = [
    "constructor",
    "toString",
    "valueOf",
    "hasOwnProperty",
    "isPrototypeOf",
    "propertyIsEnumerable",
    "toLocaleString",
    "__proto__",
];

/// `true` when the owner's parameter list gives `name` a default value, or
/// the list cannot be read. An omitted or `undefined` argument is then not
/// nullish in the body.
pub(crate) fn parameter_may_have_default(source: &str, name: &str) -> bool {
    let Some(open) = source.find('(') else {
        return false;
    };
    let mut depth = 0usize;
    let mut entries = vec![String::new()];
    let mut closed = false;
    let mut chars = source[open + 1..].chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '(' | '[' | '{' | '<' => depth += 1,
            ')' | ']' | '}' | '>' if depth == 0 => {
                closed = ch == ')';
                break;
            }
            ')' | ']' | '}' | '>' => depth -= 1,
            ',' if depth == 0 => {
                entries.push(String::new());
                continue;
            }
            '=' if depth == 0 && chars.peek() != Some(&'>') => {
                if let Some(entry) = entries.last_mut() {
                    entry.push('=');
                }
                continue;
            }
            _ => {}
        }
        if let Some(entry) = entries.last_mut() {
            entry.push(ch);
        }
    }
    if !closed {
        return true;
    }
    entries.iter().any(|entry| {
        let entry = entry.trim();
        entry.contains('=')
            && entry.strip_prefix(name).is_some_and(|rest| {
                !rest.starts_with(|c: char| c.is_alphanumeric() || c == '_' || c == '$')
            })
    })
}

/// `param?.prop` or `param.prop`, with plain identifiers only.
fn property_read(text: &str) -> Option<(String, String, bool)> {
    let (param, prop, optional) = match text.split_once("?.") {
        Some((param, prop)) => (param, prop, true),
        None => {
            let (param, prop) = text.split_once('.')?;
            (param, prop, false)
        }
    };
    (is_identifier(param) && is_identifier(prop))
        .then(|| (param.to_string(), prop.to_string(), optional))
}

/// The `??`-separated operands of `return a ?? b ?? c;`. Strings are kept
/// whole; any bracket, comment, `||`, `&&`, `?`/`:` outside `?.` and `??`
/// refuses the read.
fn chain_operands(line: &str) -> Option<Vec<String>> {
    let rest = line.trim().strip_prefix("return")?;
    if !rest.starts_with(char::is_whitespace) {
        return None;
    }
    let expression = rest.trim().trim_end_matches(';').trim_end();
    let chars: Vec<char> = expression.chars().collect();
    let mut operands = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    let mut at = 0;
    while at < chars.len() {
        let ch = chars[at];
        if let Some(open) = quote {
            current.push(ch);
            if ch == '\\' || ch == '`' || ch == '$' {
                return None;
            }
            if ch == open {
                quote = None;
            }
            at += 1;
            continue;
        }
        let next = chars.get(at + 1).copied();
        match (ch, next) {
            ('"' | '\'', _) => {
                quote = Some(ch);
                current.push(ch);
            }
            ('?', Some('?')) => {
                operands.push(std::mem::take(&mut current).trim().to_string());
                at += 2;
                continue;
            }
            ('?', Some('.')) => {
                current.push_str("?.");
                at += 2;
                continue;
            }
            ('?' | ':' | '|' | '&' | '(' | ')' | '[' | ']' | '{' | '}' | '/' | '`' | '!', _) => {
                return None;
            }
            _ => current.push(ch),
        }
        at += 1;
    }
    if quote.is_some() {
        return None;
    }
    operands.push(current.trim().to_string());
    operands
        .iter()
        .all(|operand| !operand.is_empty())
        .then_some(operands)
}
