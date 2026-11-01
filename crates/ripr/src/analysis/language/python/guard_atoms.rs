//! Closed allow-list of non-comparison guard forms for the guarded-arm gate
//! (RIPR-SPEC-0260, "Understood guard forms").
//!
//! `guard_activation.rs` evaluates `left <op> right` comparisons. A changed
//! line is just as unreached when the guard is `if x is None:`, `if flag:`,
//! `if not items:`, `if kind in ("a", "b"):`, `if isinstance(x, str):` or a
//! `for item in items:` loop over an empty literal, and a strong exact-value
//! oracle passes either way. This reader evaluates exactly those forms
//! against the literal arguments a strong related call binds:
//!
//! - `NAME is None` / `NAME is not None`;
//! - truthiness of a bare `NAME`, and `not` over any form below;
//! - `NAME in <literal display>` / `NAME not in <literal display>`;
//! - `isinstance(NAME, int | float | str | bool)` or a tuple of those, while
//!   the builtins are not rebound;
//! - `len(NAME) > 0`, which is how a `for ... in NAME:` header reads.
//!
//! Like the comparison reader it only ever EXCLUDES a row, so everything it
//! cannot read (`or`, a computed operand, a non-literal binding, a rebound
//! name, a container the owner may have changed, any other shape) answers
//! `None` and keeps the existing verdict. An `except` arm is not a guard this
//! reader can evaluate: whether a test raises is not a literal input.

use super::PythonOwner;
use super::arm_activation::{ScalarKind, builtin_names_intact, scalar_kind};
use super::boundary::{CallRow, canonical_decimal, literal_elements, resolve_operand};
use super::guard_activation::{
    compare_literal_values, condition_holds, keyword_outside_quotes, parameter_rebound_before,
};
use super::module_constants::{PythonModuleConstant, mentions_identifier};

/// Whether one guarding condition holds for one call row, reading the
/// comparison forms of [`condition_holds`] plus the closed atom list above.
/// `Some(false)` provably excludes the row; `None` never does.
pub(super) fn guard_condition_holds(
    owner: &PythonOwner,
    condition: &str,
    condition_line: usize,
    row: &CallRow,
    constants: &[PythonModuleConstant],
    owner_source: &str,
) -> Option<bool> {
    // `or` and conditional expressions change when the whole condition is
    // true, so they are refused before any conjunct is read.
    if ["or", "if", "else"]
        .iter()
        .any(|keyword| keyword_outside_quotes(condition, keyword))
    {
        return None;
    }
    let reader = Reader {
        owner,
        condition_line,
        row,
        constants,
        owner_source,
    };
    let conjuncts = split_top_level(condition, " and ");
    if conjuncts.iter().all(|conjunct| {
        let conjunct = conjunct.trim();
        // Plain comparisons keep the existing reader unchanged.
        reader.atom(conjunct).is_none()
            && !conjunct.starts_with("not ")
            && !keyword_outside_quotes(conjunct, "not")
    }) {
        return condition_holds(
            condition,
            condition_line,
            row,
            constants,
            owner_source,
            owner.start_line,
        );
    }
    for conjunct in conjuncts {
        match reader.conjunct(conjunct.trim()) {
            Some(true) => {}
            Some(false) => return Some(false),
            None => return None,
        }
    }
    Some(true)
}

struct Reader<'a> {
    owner: &'a PythonOwner,
    condition_line: usize,
    row: &'a CallRow,
    constants: &'a [PythonModuleConstant],
    owner_source: &'a str,
}

impl Reader<'_> {
    /// One `and` conjunct: an atom, `not` over an atom, or a plain comparison.
    fn conjunct(&self, text: &str) -> Option<bool> {
        if let Some(inner) = text.strip_prefix("not ") {
            return self.atom(inner.trim())?.map(|value| !value);
        }
        if let Some(value) = self.atom(text) {
            return value;
        }
        if keyword_outside_quotes(text, "not") {
            return None;
        }
        condition_holds(
            text,
            self.condition_line,
            self.row,
            self.constants,
            self.owner_source,
            self.owner.start_line,
        )
    }

    /// `None` when `text` is not one of the allow-listed atoms; `Some(None)`
    /// when it is one but this row does not bind it readably.
    fn atom(&self, text: &str) -> Option<Option<bool>> {
        if let Some(rest) = text.strip_prefix("isinstance(") {
            let inner = rest.strip_suffix(')')?;
            return Some(self.isinstance(inner));
        }
        if let Some(rest) = text.strip_prefix("len(") {
            let name = rest.strip_suffix(") > 0")?.trim();
            return Some(self.non_empty(name));
        }
        for (needle, negated, is_identity) in [
            (" is not ", true, true),
            (" is ", false, true),
            (" not in ", true, false),
            (" in ", false, false),
        ] {
            if let Some(position) = find_top_level(text, needle) {
                let (left, right) = (
                    text[..position].trim(),
                    text[position + needle.len()..].trim(),
                );
                return Some(if is_identity {
                    self.is_none(left, right).map(|value| value != negated)
                } else {
                    self.membership(left, right).map(|value| value != negated)
                });
            }
        }
        is_plain_name(text).then(|| self.truthiness(text))
    }

    /// The name must be an owner-parameter binding the owner leaves alone.
    fn stable(&self, name: &str) -> bool {
        is_plain_name(name)
            && !parameter_rebound_before(
                self.owner_source,
                self.owner.start_line,
                self.condition_line,
                name,
            )
    }

    /// The literal elements bound to `name` when it is a container display.
    fn elements(&self, name: &str) -> Option<&Vec<String>> {
        if self.row.bindings.contains_key(name) {
            return None;
        }
        let elements = self.row.elements.get(name)?;
        (!self.container_may_change(name)).then_some(elements)
    }

    /// Whether the owner could change a container before the guard: any
    /// earlier mention other than a pure condition test may mutate it.
    fn container_may_change(&self, name: &str) -> bool {
        let lines: Vec<&str> = self.owner_source.lines().collect();
        // `start_line` is 1-based, so as a 0-based index it is already the
        // line after the `def` line; the signature is the line before it.
        let start = self.owner.start_line.saturating_sub(1);
        let Some(header_end) = (start..lines.len()).find(|index| {
            lines[*index]
                .split('#')
                .next()
                .is_some_and(|code| code.trim_end().ends_with(':'))
        }) else {
            return true;
        };
        let loop_enclosed = lines
            .get(start..self.condition_line)
            .unwrap_or(&[])
            .iter()
            .any(|raw| {
                let trimmed = raw.trim_start();
                (trimmed.starts_with("for ") || trimmed.starts_with("while "))
                    && raw.len() - trimmed.len()
                        < lines
                            .get(self.condition_line)
                            .map_or(0, |guard| guard.len() - guard.trim_start().len())
            });
        let end = if loop_enclosed {
            lines.len()
        } else {
            self.condition_line
        };
        (header_end + 1..end).any(|index| {
            let trimmed = lines[index].trim();
            index != self.condition_line
                && mentions_identifier(trimmed, name)
                && !pure_condition_mention(trimmed, name)
        })
    }

    fn scalar(&self, name: &str) -> Option<String> {
        if !self.stable(name) || self.row.elements.contains_key(name) {
            return None;
        }
        // An owner parameter shadows a same-named module constant, so its
        // value is the call row's binding or nothing.
        let value = if self
            .owner
            .parameters
            .iter()
            .any(|parameter| parameter.name == name)
        {
            self.row.bindings.get(name)?.value.clone()
        } else {
            resolve_operand(name, self.row, self.constants)?
        };
        scalar_kind(&value)?;
        Some(value)
    }

    fn is_none(&self, left: &str, right: &str) -> Option<bool> {
        if right != "None" || !self.stable(left) {
            return None;
        }
        if self.elements(left).is_some() {
            return Some(false);
        }
        Some(self.scalar(left)? == "None")
    }

    fn truthiness(&self, name: &str) -> Option<bool> {
        if !self.stable(name) {
            return None;
        }
        if let Some(elements) = self.elements(name) {
            return Some(!elements.is_empty());
        }
        let value = self.scalar(name)?;
        match value.as_str() {
            "None" | "False" => return Some(false),
            "True" => return Some(true),
            _ => {}
        }
        match scalar_kind(&value)? {
            ScalarKind::Str => {
                // A backslash can spell an empty continuation, so only an
                // escape-free string is read.
                let body = &value[1..value.len() - 1];
                (!body.contains('\\')).then_some(!body.is_empty())
            }
            ScalarKind::Int | ScalarKind::Float => {
                canonical_decimal(&value).map(|number| number != "0")
            }
            ScalarKind::None | ScalarKind::Bool => None,
        }
    }

    /// Whether a `for ... in NAME:` header enters its body: the literal
    /// display holds an element, or the string is non-empty.
    fn non_empty(&self, name: &str) -> Option<bool> {
        if !self.stable(name) {
            return None;
        }
        if let Some(elements) = self.elements(name) {
            return Some(!elements.is_empty());
        }
        let value = self.scalar(name)?;
        if scalar_kind(&value)? != ScalarKind::Str || value.contains('\\') {
            return None;
        }
        Some(value.len() > 2)
    }

    fn membership(&self, left: &str, right: &str) -> Option<bool> {
        let value = self.scalar(left)?;
        let members = literal_display(right)?;
        let mut any_unknown = false;
        for member in &members {
            scalar_kind(member)?;
            match compare_literal_values(&value, "==", member) {
                Some(true) => return Some(true),
                Some(false) => {}
                None => any_unknown = true,
            }
        }
        (!any_unknown).then_some(false)
    }

    fn isinstance(&self, arguments: &str) -> Option<bool> {
        let parts = split_top_level(arguments, ",");
        let [subject, classes] = parts.as_slice() else {
            return None;
        };
        let value = self.scalar(subject.trim())?;
        if !builtin_names_intact(
            self.owner,
            self.owner_source,
            &["isinstance", "int", "float", "str", "bool"],
        ) {
            return None;
        }
        let kind = scalar_kind(&value)?;
        let classes = classes.trim();
        let names: Vec<&str> = match classes.strip_prefix('(') {
            Some(tuple) => tuple.strip_suffix(')')?.split(',').map(str::trim).collect(),
            None => vec![classes],
        };
        let mut matched = false;
        for name in names.into_iter().filter(|name| !name.is_empty()) {
            matched |= match name {
                "int" => matches!(kind, ScalarKind::Int | ScalarKind::Bool),
                "float" => kind == ScalarKind::Float,
                "str" => kind == ScalarKind::Str,
                "bool" => kind == ScalarKind::Bool,
                _ => return None,
            };
        }
        Some(matched)
    }
}

/// A line that only tests the container (`if not items:`, `if items is
/// None:`, `if len(items) > 3:`, `if items:`) cannot change it.
fn pure_condition_mention(trimmed: &str, name: &str) -> bool {
    let Some(condition) = ["if ", "elif "]
        .iter()
        .find_map(|keyword| trimmed.strip_prefix(keyword))
        .and_then(|rest| rest.trim_end().strip_suffix(':'))
    else {
        return false;
    };
    let mut rest = condition.trim().to_string();
    for form in [
        format!("len({name})"),
        format!("{name} is not None"),
        format!("{name} is None"),
        format!("not {name}"),
    ] {
        rest = strip_standalone_form(&rest, &form);
    }
    rest.trim() == name || !mentions_identifier(&rest, name)
}

/// Removes occurrences of `form` from `text` only when they stand alone: not
/// part of a longer identifier and not followed by an attribute access,
/// subscript or call (`.`, `[`, `(`) that could mutate the value
/// (`not items.pop()` is not a pure test of `items`).
fn strip_standalone_form(text: &str, form: &str) -> String {
    let is_ident_char = |c: char| c.is_alphanumeric() || c == '_';
    let mut out = String::with_capacity(text.len());
    let mut cursor = 0;
    while let Some(offset) = text[cursor..].find(form) {
        let start = cursor + offset;
        let end = start + form.len();
        let before_ok = text[..start]
            .chars()
            .next_back()
            .is_none_or(|c| !is_ident_char(c));
        let after_ok = text[end..]
            .chars()
            .next()
            .is_none_or(|c| !(is_ident_char(c) || matches!(c, '.' | '[' | '(')));
        out.push_str(&text[cursor..start]);
        if before_ok && after_ok {
            out.push(' ');
        } else {
            out.push_str(form);
        }
        cursor = end;
    }
    out.push_str(&text[cursor..]);
    out
}

fn is_plain_name(text: &str) -> bool {
    let mut chars = text.chars();
    chars
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic() || first == '_')
        && chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
        && !matches!(
            text,
            "None" | "True" | "False" | "not" | "and" | "or" | "in" | "is" | "lambda" | "await"
        )
}

/// The scalar literals of a list, set or multi-element tuple display. A
/// dict display, a parenthesized scalar and any computed element are refused.
fn literal_display(text: &str) -> Option<Vec<String>> {
    let text = text.trim();
    if text.contains(':') || text.contains('*') {
        return None;
    }
    let inner = text.get(1..text.len().checked_sub(1)?)?;
    if text.starts_with('(') && !inner.trim().is_empty() && !inner.contains(',') {
        return None;
    }
    literal_elements(text)
}

/// Splits `text` at every top-level occurrence of `separator`, outside quotes
/// and brackets.
fn split_top_level<'a>(text: &'a str, separator: &str) -> Vec<&'a str> {
    let mut parts = Vec::new();
    let mut last = 0;
    let mut position = 0;
    while let Some(found) = find_top_level(&text[position..], separator) {
        let at = position + found;
        parts.push(&text[last..at]);
        last = at + separator.len();
        position = last;
    }
    parts.push(&text[last..]);
    parts
}

/// The byte offset of the first top-level occurrence of `needle`.
fn find_top_level(text: &str, needle: &str) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut quote: Option<u8> = None;
    let mut depth = 0usize;
    let mut index = 0;
    while index < bytes.len() {
        let byte = bytes[index];
        if let Some(active) = quote {
            if byte == b'\\' {
                index += 2;
                continue;
            }
            if byte == active {
                quote = None;
            }
            index += 1;
            continue;
        }
        match byte {
            b'\'' | b'"' => quote = Some(byte),
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth = depth.saturating_sub(1),
            _ => {}
        }
        if depth == 0 && quote.is_none() && bytes[index..].starts_with(needle.as_bytes()) {
            return Some(index);
        }
        index += 1;
    }
    None
}
