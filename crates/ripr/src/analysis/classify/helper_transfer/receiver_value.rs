//! Receiver values for a method hop (#9329, RIPR-SPEC-0158).
//!
//! A test that reaches the owner through
//!
//! ```text
//! fn assert_to_string(expected: &str, b: ByteSize, si: bool) { .. b.to_string_as(si) .. }
//! fn to_string_as(&self, si_unit: bool) -> String { to_string(self.0, si_unit) }
//! ```
//!
//! passes the boundary value inside the receiver: `ByteSize(215)` or
//! `ByteSize::kib(1)`. This module is the one authority that reads such an
//! argument as a canonical constructed string `Type(<v0>,<v1>..)` and
//! projects one field of it back (`self.0`). Every other shape answers
//! `None`, so the row carries no value and the verdict stays unresolved:
//!
//! - `Type(<integer literals>)` where `Type` is a tuple struct declared
//!   exactly once in the workspace (text based on the masked file source:
//!   there is no struct fact), at file top level, non-generic, every field a
//!   plain integer type, and no other item (`enum`, `type`, `fn`, `use`
//!   import, macro-generated `struct $..`) shares its name;
//! - `Type::ctor(<integer literals>)` where `ctor` is a unique `const fn` in
//!   an inherent `impl Type` taking no receiver, whose whole body is one tail
//!   expression `Type(<expr>, ..)` or `Self(<expr>, ..)`, each `<expr>` a
//!   product of ctor parameters, literals and same-file integer constants
//!   declared once, not rebound and not imported, evaluated exactly and
//!   checked against the field type (an overflow declines).
//!
//! The canonical string exists only as a row cell at the chain's entry and
//! in the pseudo-parameter `self` a method hop hands its callee. It is never
//! an owner parameter value and is never compared against a boundary.

use super::{callee_is_unique, is_identifier, matching_close, split_call_arguments_text};
use crate::analysis::classify::activation::{
    free_identifiers, initializer_constants, is_constant_like, name_imported_in_file,
    signature_parameter_texts,
};
use crate::analysis::classify::value_transfer::{ExactInputs, evaluate_integer_product};
use crate::analysis::facts::{FunctionContainer, RustIndex};

/// Whether a row cell value is a constructed receiver string rather than a
/// scalar literal.
pub(in crate::analysis::classify) fn is_receiver_cell(value: &str) -> bool {
    // `Type(..)`: a quoted string or char literal holding a parenthesis
    // (`"(hi)"`, `'('`) starts with a quote and is a scalar.
    value
        .split_once('(')
        .is_some_and(|(head, _)| is_identifier(head))
        && value.ends_with(')')
}

/// The canonical constructed string of a receiver argument, or `None`.
pub(in crate::analysis::classify) fn receiver_cell_value(
    argument: &str,
    index: &RustIndex,
) -> Option<String> {
    // The parameter's declared type is `Type`, `&Type` or `&mut Type`
    // (checked when the method hop resolved), so a borrow of the constructed
    // value is the same value.
    let argument = strip_borrow(argument.trim());
    let open = argument.find('(')?;
    if matching_close(argument, open)? + 1 != argument.len() {
        return None;
    }
    let head = &argument[..open];
    let callee = head.rsplit("::").next()?;
    let arguments = split_call_arguments_text(argument.get(open - callee.len()..)?, callee)?;
    match head.split_once("::") {
        None if is_identifier(head) => {
            let tuple = tuple_struct(index, head)?;
            canonical(
                &tuple,
                head,
                &literal_arguments(&arguments, &tuple.field_type)?,
            )
        }
        Some((type_name, ctor_name))
            if is_identifier(type_name)
                && is_identifier(ctor_name)
                && !ctor_name.contains("::") =>
        {
            let tuple = tuple_struct(index, type_name)?;
            let values = constructor_values(index, &tuple, type_name, ctor_name, &arguments)?;
            canonical(&tuple, type_name, &values)
        }
        _ => None,
    }
}

/// `&x` and `&mut x` as `x`.
fn strip_borrow(argument: &str) -> &str {
    let Some(rest) = argument.strip_prefix('&') else {
        return argument;
    };
    let rest = rest.trim_start();
    match rest.strip_prefix("mut") {
        Some(after) if after.starts_with(char::is_whitespace) => after.trim_start(),
        _ => rest,
    }
}

/// Field `field` (a bare tuple index such as `0`) of a canonical receiver
/// string. `None` for an index out of range or a value that is not one.
pub(in crate::analysis::classify) fn project_receiver_field(
    cell: &str,
    field: &str,
) -> Option<String> {
    let position: usize = field.parse().ok()?;
    if position.to_string() != field {
        return None;
    }
    let open = cell.find('(')?;
    let head = &cell[..open];
    let inner = cell.strip_suffix(')')?.get(open + 1..)?;
    if !is_identifier(head) {
        return None;
    }
    let values: Vec<&str> = inner.split(',').collect();
    if values
        .iter()
        .any(|value| value.is_empty() || !value.chars().all(|ch| ch.is_ascii_digit()))
    {
        return None;
    }
    values.get(position).map(|value| (*value).to_string())
}

fn canonical(tuple: &TupleStruct, type_name: &str, values: &[String]) -> Option<String> {
    (values.len() == tuple.arity).then(|| format!("{type_name}({})", values.join(",")))
}

/// Plain decimal literal arguments that fit `field_type`, normalized.
fn literal_arguments(arguments: &[String], field_type: &str) -> Option<Vec<String>> {
    let max = integer_max(field_type)?;
    arguments
        .iter()
        .map(|argument| {
            let argument = argument.trim();
            if argument.is_empty() || !argument.chars().all(|ch| ch.is_ascii_digit()) {
                return None;
            }
            let value: u128 = argument.parse().ok()?;
            (value <= max).then(|| value.to_string())
        })
        .collect()
}

fn integer_max(ty: &str) -> Option<u128> {
    Some(match ty {
        "u8" => u128::from(u8::MAX),
        "u16" => u128::from(u16::MAX),
        "u32" => u128::from(u32::MAX),
        "u64" => u128::from(u64::MAX),
        "i8" => u128::from(i8::MAX.unsigned_abs()),
        "i16" => u128::from(i16::MAX.unsigned_abs()),
        "i32" => u128::from(i32::MAX.unsigned_abs()),
        "i64" => u128::from(i64::MAX.unsigned_abs()),
        _ => return None,
    })
}

struct TupleStruct {
    arity: usize,
    /// The one integer type every field shares.
    field_type: String,
}

/// The tuple struct `name`, when it is declared exactly once in the
/// workspace as `struct Name(<integer fields>);` at file top level and no
/// other item, import or macro fragment can stand for the name.
fn tuple_struct(index: &RustIndex, name: &str) -> Option<TupleStruct> {
    let is_word = |ch: char| ch.is_ascii_alphanumeric() || ch == '_';
    let mut declaration: Option<(String, usize)> = None;
    let mut declarations = 0usize;
    for facts in index.files().values() {
        let masked = crate::analysis::language::mask_rust_comments_and_strings(&facts.source);
        if masked.len() != facts.source.len()
            || masked.contains("struct $")
            || name_imported_in_file(&facts.source, name)
        {
            return None;
        }
        for (at, _) in masked.match_indices(name) {
            if masked[..at].chars().next_back().is_some_and(is_word)
                || masked[at + name.len()..]
                    .chars()
                    .next()
                    .is_some_and(is_word)
            {
                continue;
            }
            let keyword = masked[..at]
                .trim_end()
                .rsplit(|ch: char| !is_word(ch))
                .next()
                .unwrap_or_default();
            if !matches!(
                keyword,
                "struct" | "enum" | "union" | "type" | "trait" | "fn" | "const" | "static" | "mod"
            ) {
                continue;
            }
            declarations += 1;
            let depth = masked[..at]
                .chars()
                .filter(|ch| *ch == '{')
                .count()
                .saturating_sub(masked[..at].chars().filter(|ch| *ch == '}').count());
            if keyword == "struct" && depth == 0 {
                declaration = Some((masked.clone(), at + name.len()));
            }
        }
    }
    if declarations != 1 {
        return None;
    }
    let (masked, after_name) = declaration?;
    let rest = &masked[after_name..];
    let open = rest.starts_with('(').then_some(0)?;
    let close = matching_close(rest, open)?;
    if !rest[close + 1..].trim_start().starts_with(';') {
        return None;
    }
    let mut field_type: Option<String> = None;
    let mut arity = 0usize;
    for field in rest[open + 1..close].split(',') {
        let field = field.trim();
        if field.is_empty() {
            continue;
        }
        let field = strip_visibility(field);
        integer_max(field)?;
        if field_type.as_deref().is_some_and(|known| known != field) {
            return None;
        }
        field_type = Some(field.to_string());
        arity += 1;
    }
    Some(TupleStruct {
        arity: (arity > 0).then_some(arity)?,
        field_type: field_type?,
    })
}

fn strip_visibility(field: &str) -> &str {
    let Some(rest) = field.strip_prefix("pub") else {
        return field;
    };
    let rest = rest.trim_start();
    if rest.starts_with('(') {
        return matching_close(rest, 0).map_or(field, |close| rest[close + 1..].trim_start());
    }
    if rest.len() == field.len() - 3 {
        return field;
    }
    rest
}

/// The evaluated field values of `Type::ctor(<arguments>)`.
fn constructor_values(
    index: &RustIndex,
    tuple: &TupleStruct,
    type_name: &str,
    ctor_name: &str,
    arguments: &[String],
) -> Option<Vec<String>> {
    if !callee_is_unique(ctor_name, index) {
        return None;
    }
    let ctor = index
        .functions()
        .iter()
        .find(|function| function.name == ctor_name)?;
    let FunctionContainer::Inherent { self_ty } = &ctor.item.container else {
        return None;
    };
    if self_ty != type_name || ctor.item.has_self_param || !ctor.item.has_body {
        return None;
    }
    let masked = crate::analysis::language::mask_rust_comments_and_strings(&ctor.body);
    if masked.len() != ctor.body.len() {
        return None;
    }
    // `[pub[(..)]] const fn`: the function summary's body starts at `fn`, so
    // the qualifiers are read from the declaration's own source line. No
    // `async`, `unsafe` or `extern` qualifier is admitted.
    let fn_at = masked.find("fn ")?;
    let source = &index.files().get(&ctor.file)?.source;
    let declaration = crate::analysis::language::mask_rust_comments_and_strings(
        source.lines().nth(ctor.start_line.checked_sub(1)?)?,
    );
    let qualifiers = declaration
        .split_once(&format!("fn {ctor_name}"))?
        .0
        .replace(['(', ')'], " ");
    let words: Vec<&str> = qualifiers.split_whitespace().collect();
    if !words.contains(&"const")
        || words
            .iter()
            .any(|word| !matches!(*word, "pub" | "const" | "crate" | "super" | "self" | "in"))
    {
        return None;
    }
    // Parameters: each `name: <the struct's integer type>`.
    let mut parameters = Vec::new();
    for parameter in signature_parameter_texts(ctor) {
        let (name, declared) = parameter.split_once(':')?;
        let name = name.trim();
        if !is_identifier(name) || declared.trim() != tuple.field_type {
            return None;
        }
        parameters.push(name.to_string());
    }
    if parameters.len() != arguments.len() {
        return None;
    }
    let literals = literal_arguments(arguments, &tuple.field_type)?;
    let mut inputs = ExactInputs::new();
    for (parameter, literal) in parameters.iter().zip(&literals) {
        inputs.insert(parameter.clone(), literal.clone());
    }
    // The whole body is one tail expression `Type(<expr>, ..)` / `Self(..)`.
    let signature_close = masked[fn_at..].find(')')? + fn_at;
    let body_open = masked[signature_close..].find('{')? + signature_close;
    let body_close = masked.rfind('}')?;
    if body_close <= body_open {
        return None;
    }
    let inner = masked[body_open + 1..body_close].trim();
    if inner.contains([';', '{', '}', '?']) {
        return None;
    }
    let open = inner.find('(')?;
    if !matches!(&inner[..open], head if head == type_name || head == "Self")
        || matching_close(inner, open)? + 1 != inner.len()
    {
        return None;
    }
    let expressions = split_call_arguments_text(inner, &inner[..open])?;
    if expressions.len() != tuple.arity {
        return None;
    }
    let max = integer_max(&tuple.field_type)?;
    let mut values = Vec::new();
    for expression in &expressions {
        let constants = initializer_constants(ctor, expression, &parameters, index);
        for (name, value) in constants {
            let value = value.replace('_', "");
            inputs.entry(name).or_insert(value);
        }
        // Every name the expression reads is a ctor parameter or a constant
        // that passed the same-file, declared-once, not-rebound,
        // not-imported rule.
        if free_identifiers(expression).iter().any(|name| {
            !inputs.contains_key(name) || (!parameters.contains(name) && !is_constant_like(name))
        }) {
            return None;
        }
        values.push(evaluate_integer_product(expression, &inputs, max)?.to_string());
    }
    Some(values)
}

#[cfg(test)]
mod tests;
