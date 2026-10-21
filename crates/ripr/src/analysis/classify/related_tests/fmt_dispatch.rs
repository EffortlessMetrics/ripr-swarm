//! Formatting dispatch (RIPR-SPEC-0297) as reach of a changed `Display::fmt` / `Debug::fmt`.
//!
//! A test never spells `fmt`: `format!("{:?}", err)`, `err.to_string()` and
//! `write!(out, "{}", err)` run the owner's `fmt` through the formatting
//! machinery. Name-based call matching therefore never relates such a test to
//! a changed `impl Display for T` / `impl Debug for T`.
//!
//! This module recognises that dispatch conservatively. It admits a test only
//! when every link is established from syntax alone:
//!
//! - the owner is `fmt` of a `Display` or `Debug` impl for `T` (generic
//!   arguments of the self type are ignored, as `impl_self_type_name` does);
//! - exactly one indexed `fmt` carries that trait and self type, and exactly
//!   one `struct`/`enum`/`union`/`type` named `T` is declared in the index (a
//!   same-named type elsewhere, possibly with a derived `Debug`, fails closed);
//! - a formatting macro (or `.to_string()` for `Display`, when no inherent
//!   `to_string` exists for `T`) not rebound by `macro_rules!`/`use` has a
//!   placeholder whose trait matches (`{}` vs `{:?}`), with every positional
//!   argument accounted for by the format string, and
//! - the placeholder's argument is built as `T` (struct literal, tuple
//!   struct, enum variant) directly or by a preceding `let`.
//!
//! An argument of unknown type, a mismatched trait, a width/precision
//! argument (`$`, `*`) or an unrelated type admits nothing. Derived `Debug`
//! has no indexed `fmt` owner and is out of scope.

use super::{
    impl_self_type_name, impl_trait_name, initializer_is_one_head_expr, is_ident_byte,
    is_variant_name, last_let_statement, type_head,
};
use crate::analysis::extract::mask_comments_and_strings;
use crate::analysis::rust_index::{FunctionSummary, RustIndex, TestSummary};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum FmtTrait {
    Display,
    Debug,
}

/// Macros that format their arguments with `{}` / `{:?}` placeholders, and the
/// index of the format-string argument.
const FORMAT_MACROS: &[(&str, usize)] = &[
    ("format", 0),
    ("format_args", 0),
    ("print", 0),
    ("println", 0),
    ("eprint", 0),
    ("eprintln", 0),
    ("write", 1),
    ("writeln", 1),
];

/// Trait and self type of a `Display`/`Debug` `fmt` owner.
fn fmt_owner(owner: &FunctionSummary) -> Option<(FmtTrait, String)> {
    if owner.name != "fmt" {
        return None;
    }
    let fmt_trait = match impl_trait_name(&owner.id.0)?.as_str() {
        "Display" => FmtTrait::Display,
        "Debug" => FmtTrait::Debug,
        _ => return None,
    };
    Some((fmt_trait, impl_self_type_name(&owner.id.0)?))
}

/// Whether `owner` is a `Display`/`Debug` `fmt` whose reach this module judges.
pub(super) fn is_fmt_dispatch_owner(owner: &FunctionSummary) -> bool {
    fmt_owner(owner).is_some()
}

/// Cheap lexical prefilter for the candidate index: the body may format.
pub(super) fn body_may_dispatch_fmt(body: &str) -> bool {
    ["format", "write", "print", "to_string"]
        .iter()
        .any(|needle| body.contains(needle))
}

/// Facts about the owner's type and the workspace that hold for every test
/// of one owner, computed once per owner and probe instead of per test.
pub(super) struct FmtContext {
    /// Exactly one type named like the owner's self type is declared.
    type_declared_once: bool,
    /// Some indexed function named `to_string` belongs to an impl of the
    /// self type: an inherent `to_string` would win over `Display`.
    competing_to_string: bool,
    /// Formatting macros a `macro_rules!` or non-std `use` can rebind.
    shadowed_macros: Vec<&'static str>,
}

impl FmtContext {
    pub(super) fn new(index: &RustIndex, owner: &FunctionSummary) -> Option<Self> {
        let (_, self_type) = fmt_owner(owner)?;
        let competing_to_string = index.functions().iter().any(|function| {
            function.name == "to_string"
                && impl_self_type_name(&function.id.0).as_deref() == Some(self_type.as_str())
        });
        let mut declared = 0usize;
        let mut shadowed_macros = Vec::new();
        for file in index.files().values() {
            let source: &str = &file.source;
            let names_type = source.contains(self_type.as_str());
            let names_macro = source.contains("macro_rules") || source.contains("use ");
            if !names_type && !names_macro {
                continue;
            }
            let masked = mask_comments_and_strings(source);
            if names_type {
                declared += declarations_in(&masked, &self_type);
            }
            if names_macro {
                for &(name, _) in FORMAT_MACROS {
                    if !shadowed_macros.contains(&name) && rebinds_macro(&masked, name) {
                        shadowed_macros.push(name);
                    }
                }
            }
        }
        Some(Self {
            type_declared_once: declared == 1,
            competing_to_string,
            shadowed_macros,
        })
    }
}

/// Whether masked source defines `macro_rules! name` or imports `name` from
/// a non-std/core path (`use mylog::println;`).
fn rebinds_macro(masked: &str, name: &str) -> bool {
    if !masked.contains(name) {
        return false;
    }
    let mut previous = "";
    for token in masked
        .split(|c: char| !(c.is_alphanumeric() || c == '_'))
        .filter(|token| !token.is_empty())
    {
        if token == name && previous == "macro_rules" {
            return true;
        }
        previous = token;
    }
    masked.split(';').any(|statement| {
        let Some(at) = statement
            .match_indices("use ")
            .map(|(at, _)| at)
            .find(|&at| {
                !statement[..at]
                    .bytes()
                    .next_back()
                    .is_some_and(is_ident_byte)
            })
        else {
            return false;
        };
        let path = &statement[at + "use ".len()..];
        let names = path
            .split(|c: char| !(c.is_alphanumeric() || c == '_'))
            .any(|token| token == name);
        let root = path.trim_start().split("::").next().unwrap_or("").trim();
        names && !matches!(root, "std" | "core" | "alloc")
    })
}

/// Whether `test` formats a value of the owner's self type through the
/// owner's trait. `same_name_ids` are the indexed functions named like the
/// owner; more than one `fmt` for the owner's trait and self type is
/// ambiguous and fails closed.
pub(super) fn test_dispatches_to_fmt(
    test: &TestSummary,
    owner: &FunctionSummary,
    same_name_ids: &[&str],
    context: &FmtContext,
) -> bool {
    let Some((fmt_trait, self_type)) = fmt_owner(owner) else {
        return false;
    };
    if !context.type_declared_once || !body_may_dispatch_fmt(&test.body) {
        return false;
    }
    let same_fmt_impls = same_name_ids
        .iter()
        .filter(|id| {
            impl_self_type_name(id).as_deref() == Some(self_type.as_str())
                && impl_trait_name(id).and_then(|name| trait_of(&name)) == Some(fmt_trait)
        })
        .count();
    if same_fmt_impls != 1 {
        return false;
    }
    let masked = mask_comments_and_strings(&test.body);
    (fmt_trait == FmtTrait::Display
        && !context.competing_to_string
        && to_string_dispatch(&masked, &self_type))
        || macro_dispatch(&test.body, &masked, fmt_trait, &self_type, context)
}

/// `value.to_string()` where `value` is typed as `self_type` by the strict
/// rule below (constructor literal or typed `let`).
fn to_string_dispatch(masked: &str, self_type: &str) -> bool {
    let mut search = 0usize;
    while let Some(at) = masked[search..].find(".to_string").map(|at| search + at) {
        search = at + 1;
        let after = masked[at + ".to_string".len()..].trim_start();
        if !after.starts_with("()") {
            continue;
        }
        let receiver = receiver_before(masked, at);
        let lets = lets_before(masked, at);
        if expr_has_type(
            receiver,
            Scope {
                lets: &lets,
                body: masked,
            },
            self_type,
        ) {
            return true;
        }
    }
    false
}

/// The receiver expression ending at `dot`: an identifier or a bracketed
/// constructor (`Site::new(..)`, `Site { .. }`, `(expr)`), else empty.
fn receiver_before(masked: &str, dot: usize) -> &str {
    let bytes = masked.as_bytes();
    let mut start = dot;
    if matches!(bytes.get(dot.wrapping_sub(1)), Some(b')' | b'}')) {
        let (open_byte, close_byte) = if bytes[dot - 1] == b')' {
            (b'(', b')')
        } else {
            (b'{', b'}')
        };
        let mut depth = 0usize;
        let mut open = None;
        for index in (0..dot).rev() {
            if bytes[index] == close_byte {
                depth += 1;
            } else if bytes[index] == open_byte {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    open = Some(index);
                    break;
                }
            }
        }
        let Some(mut open) = open else { return "" };
        while open > 0 && bytes[open - 1].is_ascii_whitespace() && open_byte == b'{' {
            open -= 1;
        }
        start = open;
    }
    while start > 0 && (is_ident_byte(bytes[start - 1]) || bytes[start - 1] == b':') {
        start -= 1;
    }
    masked.get(start..dot).unwrap_or("")
}

/// Count of `struct`/`enum`/`union`/`type` declarations of `name`.
fn declarations_in(masked: &str, name: &str) -> usize {
    let mut previous = "";
    let mut count = 0usize;
    for token in masked
        .split(|c: char| !(c.is_alphanumeric() || c == '_'))
        .filter(|token| !token.is_empty())
    {
        if token == name && matches!(previous, "struct" | "enum" | "union" | "type") {
            count += 1;
        }
        previous = token;
    }
    count
}

fn trait_of(name: &str) -> Option<FmtTrait> {
    match name {
        "Display" => Some(FmtTrait::Display),
        "Debug" => Some(FmtTrait::Debug),
        _ => None,
    }
}

fn macro_dispatch(
    raw: &str,
    masked: &str,
    fmt_trait: FmtTrait,
    self_type: &str,
    context: &FmtContext,
) -> bool {
    if raw.len() != masked.len() {
        return false;
    }
    let bytes = masked.as_bytes();
    let mut search = 0usize;
    while let Some(bang) = masked[search..].find('!').map(|at| search + at) {
        search = bang + 1;
        let name_start = masked[..bang]
            .bytes()
            .rposition(|byte| !is_ident_byte(byte))
            .map_or(0, |at| at + 1);
        let name = masked.get(name_start..bang).unwrap_or("");
        let Some(&(_, format_index)) = FORMAT_MACROS.iter().find(|(known, _)| *known == name)
        else {
            continue;
        };
        if context.shadowed_macros.contains(&name) {
            continue;
        }
        // A path-qualified macro is the std one only through `std::`/`core::`.
        let qualifier = masked.get(..name_start).unwrap_or("");
        if qualifier.ends_with("::")
            && !(qualifier.ends_with("std::") || qualifier.ends_with("core::"))
        {
            continue;
        }
        let after = masked[bang + 1..].trim_start();
        let open = masked.len() - after.len();
        if bytes.get(open) != Some(&b'(') {
            continue;
        }
        let Some(close) = matching_close(bytes, open) else {
            continue;
        };
        let args = split_top_level(masked, raw, open + 1, close);
        let lets = lets_before(masked, name_start);
        let scope = Scope {
            lets: &lets,
            body: masked,
        };
        if args_dispatch(&args, format_index, scope, fmt_trait, self_type) {
            return true;
        }
    }
    false
}

/// `(masked, raw)` text of one macro argument.
type Arg<'a> = (&'a str, &'a str);

fn args_dispatch(
    args: &[Arg<'_>],
    format_index: usize,
    scope: Scope<'_>,
    fmt_trait: FmtTrait,
    self_type: &str,
) -> bool {
    let Some((_, format_raw)) = args.get(format_index) else {
        return false;
    };
    let Some(format) = string_literal_content(format_raw) else {
        return false;
    };
    let Some(placeholders) = placeholders(format) else {
        return false;
    };
    let mut positional: Vec<&str> = Vec::new();
    let mut named: Vec<(&str, &str)> = Vec::new();
    for (masked, _) in &args[format_index + 1..] {
        match masked.split_once('=') {
            Some((name, value))
                if !value.starts_with('=')
                    && !name.is_empty()
                    && name.trim().bytes().all(is_ident_byte) =>
            {
                named.push((name.trim(), value.trim()));
            }
            _ => positional.push(masked.trim()),
        }
    }
    // Resolve every placeholder first. The placeholders must account for
    // each positional argument exactly (a turbofish or closure comma splits
    // one argument into several and shifts the rest), and every named
    // argument must be used; otherwise nothing is credited.
    let mut next = 0usize;
    let mut used = vec![false; positional.len()];
    let mut used_named: Vec<&str> = Vec::new();
    let mut resolved: Vec<(Option<FmtTrait>, &str)> = Vec::new();
    for placeholder in &placeholders {
        let expr = match &placeholder.arg {
            PlaceholderArg::Next => {
                let at = next;
                next += 1;
                match positional.get(at) {
                    Some(expr) => {
                        used[at] = true;
                        *expr
                    }
                    None => return false,
                }
            }
            PlaceholderArg::Index(at) => match positional.get(*at) {
                Some(expr) => {
                    used[*at] = true;
                    *expr
                }
                None => return false,
            },
            PlaceholderArg::Name(name) => {
                match named.iter().find(|(candidate, _)| candidate == name) {
                    Some((_, value)) => {
                        used_named.push(name);
                        *value
                    }
                    None => *name,
                }
            }
        };
        resolved.push((placeholder.fmt_trait, expr));
    }
    if used.iter().any(|used| !used) || named.iter().any(|(name, _)| !used_named.contains(name)) {
        return false;
    }
    resolved.iter().any(|(placeholder_trait, expr)| {
        *placeholder_trait == Some(fmt_trait) && expr_has_type(expr, scope, self_type)
    })
}

enum PlaceholderArg<'a> {
    Next,
    Index(usize),
    Name(&'a str),
}

struct Placeholder<'a> {
    arg: PlaceholderArg<'a>,
    /// `None` for `{:x}`, `{:p}` and the like: a different trait.
    fmt_trait: Option<FmtTrait>,
}

/// Placeholders of a format string, or `None` when the string uses width or
/// precision arguments (`$`, `*`) that shift argument positions.
fn placeholders(format: &str) -> Option<Vec<Placeholder<'_>>> {
    let bytes = format.as_bytes();
    let mut found = Vec::new();
    let mut at = 0usize;
    while at < bytes.len() {
        match bytes[at] {
            b'{' if bytes.get(at + 1) == Some(&b'{') => at += 2,
            b'{' => {
                let end = at + 1 + format[at + 1..].find('}')?;
                let inner = &format[at + 1..end];
                // A nested brace is a dynamic spec (`{0:{width}}`): fail closed.
                if inner.contains('{') {
                    return None;
                }
                let (arg, spec) = inner.split_once(':').unwrap_or((inner, ""));
                if spec.contains('$') || spec.contains('*') {
                    return None;
                }
                let arg = arg.trim();
                let arg = if arg.is_empty() {
                    PlaceholderArg::Next
                } else if arg.bytes().all(|byte| byte.is_ascii_digit()) {
                    PlaceholderArg::Index(arg.parse().ok()?)
                } else if arg.bytes().all(is_ident_byte) {
                    PlaceholderArg::Name(arg)
                } else {
                    return None;
                };
                let fmt_trait = if spec.ends_with('?') {
                    Some(FmtTrait::Debug)
                } else if spec.ends_with(['x', 'X', 'o', 'b', 'e', 'E', 'p']) {
                    None
                } else {
                    Some(FmtTrait::Display)
                };
                found.push(Placeholder { arg, fmt_trait });
                at = end + 1;
            }
            _ => at += 1,
        }
    }
    Some(found)
}

/// Content of a plain `"..."` literal; raw and byte strings are not read.
fn string_literal_content(raw: &str) -> Option<&str> {
    let raw = raw.trim();
    raw.strip_prefix('"')?.strip_suffix('"')
}

/// Whether the argument expression is a value of `self_type`, from syntax
/// that constructs `Self` itself: the bare type name, a struct literal,
/// tuple struct or enum variant, or a `let` binding initialized so (or
/// annotated `: self_type`) before the macro. Associated consts and factory
/// functions (`T::LABEL`, `T::make()`) may have another type and are refused.
fn expr_has_type(expr: &str, scope: Scope<'_>, self_type: &str) -> bool {
    let mut expr = expr.trim();
    loop {
        let stripped = expr
            .strip_prefix('&')
            .or_else(|| expr.strip_prefix("mut "))
            .map(str::trim_start);
        match stripped {
            Some(next) => expr = next,
            None => break,
        }
    }
    if expr.is_empty() {
        return false;
    }
    if expr.bytes().all(is_ident_byte) {
        return expr == self_type || binding_has_type(scope, expr, self_type);
    }
    initializer_is_one_head_expr(expr) && constructs_self(expr, self_type)
}

fn constructs_self(expr: &str, self_type: &str) -> bool {
    let Some((segments, next)) = type_head(expr) else {
        return false;
    };
    let opens = matches!(next, Some(b'(' | b'{'));
    match segments.as_slice() {
        // `Site(..)` / `m::Site { .. }`: only module (lowercase) prefixes may
        // precede the type, so `Error::Site(..)` (a variant named like the
        // type) is not a `Site`.
        [prefix @ .., last]
            if opens
                && *last == self_type
                && prefix
                    .iter()
                    .all(|segment| segment.starts_with(|c: char| c.is_ascii_lowercase())) =>
        {
            true
        }
        [.., ty, variant] => *ty == self_type && is_variant_name(variant),
        _ => false,
    }
}

/// What a macro argument can be typed from: the statements finished before
/// the macro (`lets`) and the whole masked test body (`body`).
#[derive(Clone, Copy)]
struct Scope<'a> {
    lets: &'a str,
    body: &'a str,
}

/// Whether `binding` is confined to the shapes that keep its type known:
/// every whole-word occurrence in the masked test body is either the single
/// typed `let [mut] binding`, an argument of a formatting macro, or the
/// receiver of `.to_string()`. Any other occurrence (a pattern, closure or
/// `fn` parameter, struct shorthand, match arm, another expression) may bind
/// or move the name, so it stays untyped. This is a closed list of allowed
/// shapes rather than an enumeration of binders.
fn binding_confined(body: &str, binding: &str) -> bool {
    let bytes = body.as_bytes();
    let ranges = format_macro_ranges(body);
    let mut typed_lets = 0usize;
    let mut search = 0usize;
    while let Some(at) = body[search..].find(binding).map(|at| search + at) {
        search = at + binding.len();
        let after = at + binding.len();
        let whole = (at == 0 || !is_ident_byte(bytes[at - 1]))
            && bytes.get(after).is_none_or(|byte| !is_ident_byte(*byte));
        if !whole {
            continue;
        }
        let before = body[..at].trim_end();
        let in_macro = ranges
            .iter()
            .any(|&(start, end)| start <= at && at < end && plain_arguments(&body[start..end]));
        let to_string_receiver = body[after..]
            .trim_start()
            .strip_prefix(".to_string")
            .is_some_and(|rest| rest.trim_start().starts_with('('));
        let typed_let = before
            .strip_suffix("let")
            .or_else(|| before.strip_suffix("let mut"))
            .is_some_and(|prefix| !prefix.bytes().next_back().is_some_and(is_ident_byte));
        if typed_let {
            typed_lets += 1;
        } else if !(in_macro || to_string_receiver) {
            return false;
        }
    }
    typed_lets == 1
}

/// Macro arguments that are plain expressions: no blocks, closures or
/// patterns (`{`, `|`, `=>`, `match`, `if`, `let`), where a name could be
/// bound rather than used. A nested macro's own arguments are judged on their
/// own range, so a binder in an enclosing macro's block never counts.
fn plain_arguments(arguments: &str) -> bool {
    !arguments.contains(['{', '|'])
        && !arguments.contains("=>")
        && !arguments
            .split(|c: char| !(c.is_alphanumeric() || c == '_'))
            .any(|token| matches!(token, "match" | "if" | "let"))
}

/// Byte ranges of the arguments of every standard formatting macro call.
fn format_macro_ranges(masked: &str) -> Vec<(usize, usize)> {
    let bytes = masked.as_bytes();
    let mut ranges = Vec::new();
    let mut search = 0usize;
    while let Some(bang) = masked[search..].find('!').map(|at| search + at) {
        search = bang + 1;
        let name_start = masked[..bang]
            .bytes()
            .rposition(|byte| !is_ident_byte(byte))
            .map_or(0, |at| at + 1);
        let name = masked.get(name_start..bang).unwrap_or("");
        if !FORMAT_MACROS.iter().any(|(known, _)| *known == name) {
            continue;
        }
        let open = masked.len() - masked[bang + 1..].trim_start().len();
        if bytes.get(open) != Some(&b'(') {
            continue;
        }
        if let Some(close) = matching_close(bytes, open) {
            ranges.push((open + 1, close));
        }
    }
    ranges
}

fn binding_has_type(scope: Scope<'_>, binding: &str, self_type: &str) -> bool {
    let lets = scope.lets;
    if !binding_confined(scope.body, binding) {
        return false;
    }
    let Some(statement) = last_let_statement(lets, binding) else {
        return false;
    };
    let Some(eq) = statement.find('=') else {
        return false;
    };
    let pattern = &statement[..eq];
    if let Some(colon) = pattern.find(':') {
        return type_head(&pattern[colon + 1..])
            .is_some_and(|(segments, _)| segments.last() == Some(&self_type));
    }
    let init = statement[eq + 1..].trim();
    initializer_is_one_head_expr(init)
        && constructs_self(
            init.trim_start_matches(['&'])
                .trim_start()
                .trim_start_matches("mut "),
            self_type,
        )
}

/// Statements finished (`;`) before `at`: the only `let`s that can type an
/// argument of a macro at `at`.
fn lets_before(masked: &str, at: usize) -> String {
    let before = &masked[..at];
    before
        .rfind(';')
        .map_or(String::new(), |end| before[..=end].to_string())
}

fn matching_close(bytes: &[u8], open: usize) -> Option<usize> {
    let mut depth = 0usize;
    for (offset, byte) in bytes.get(open..)?.iter().enumerate() {
        match byte {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return (*byte == b')').then_some(open + offset);
                }
            }
            _ => {}
        }
    }
    None
}

/// Split `masked[start..end]` at top-level commas, pairing each piece with the
/// same byte range of `raw` (masking preserves length).
fn split_top_level<'a>(masked: &'a str, raw: &'a str, start: usize, end: usize) -> Vec<Arg<'a>> {
    let mut args = Vec::new();
    let mut depth = 0usize;
    let mut piece = start;
    for (offset, byte) in masked.as_bytes()[start..end].iter().enumerate() {
        let at = start + offset;
        match byte {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth = depth.saturating_sub(1),
            b',' if depth == 0 => {
                args.push((&masked[piece..at], raw.get(piece..at).unwrap_or("")));
                piece = at + 1;
            }
            _ => {}
        }
    }
    let tail = raw.get(piece..end).unwrap_or("");
    if !tail.trim().is_empty() {
        args.push((&masked[piece..end], tail));
    }
    args
}

#[cfg(test)]
mod tests {
    use super::super::find_related_tests;
    use crate::analysis::facts::{FileFacts, FunctionSourceRole, OwnedRustIndex};
    use crate::analysis::rust_index::{FunctionSummary, RustIndex, TestSummary};
    use crate::domain::{
        DeltaKind, Probe, ProbeFamily, ProbeId, RelationReason, SourceLocation, SymbolId,
    };
    use std::path::PathBuf;

    fn fmt_function(file: &str, impl_header: &str) -> FunctionSummary {
        FunctionSummary {
            id: SymbolId(format!("{file}::impl {impl_header}::fmt")),
            name: "fmt".to_string(),
            file: PathBuf::from(file),
            start_line: 1,
            end_line: 3,
            body: String::new().into(),
            calls: Vec::new(),
            returns: Vec::new(),
            literals: Vec::new(),
            source_role: FunctionSourceRole::Production,
            attrs: Vec::new(),
            impl_attrs: Vec::new(),
            nested_fn_names: Vec::new(),
            let_bindings: Vec::new(),
            item: Default::default(),
            impl_context: Default::default(),
        }
    }

    fn test_with_body(name: &str, body: &str) -> TestSummary {
        TestSummary {
            name: name.to_string(),
            file: PathBuf::from("tests/other.rs"),
            start_line: 1,
            end_line: 4,
            body: body.into(),
            calls: Vec::new(),
            assertions: Vec::new(),
            literals: Vec::new(),
            attrs: Vec::new(),
            nested_fn_names: Vec::new(),
            let_bindings: Vec::new(),
        }
    }

    fn probe() -> Probe {
        Probe {
            id: ProbeId("probe:fmt".to_string()),
            location: SourceLocation::new("src/error.rs", 2, 1),
            owner: Some(SymbolId("src/error.rs::fmt".to_string())),
            family: ProbeFamily::Predicate,
            delta: DeltaKind::Value,
            before: None,
            after: Some("write!(f, \"x\")".to_string()),
            expression: "write!(f, \"x\")".to_string(),
            expected_sinks: Vec::new(),
            required_oracles: Vec::new(),
        }
    }

    /// Reasons the tests relate through for the given `fmt` impl owners.
    fn reasons(impls: &[&str], owner: usize, bodies: &[&str]) -> Vec<(String, RelationReason)> {
        reasons_with(impls, owner, bodies, &[])
    }

    /// Like `reasons`, with extra source files (type declarations elsewhere).
    fn reasons_with(
        impls: &[&str],
        owner: usize,
        bodies: &[&str],
        extra_sources: &[&str],
    ) -> Vec<(String, RelationReason)> {
        reasons_full(impls, owner, bodies, extra_sources, Vec::new())
    }

    fn reasons_full(
        impls: &[&str],
        owner: usize,
        bodies: &[&str],
        extra_sources: &[&str],
        extra_functions: Vec<FunctionSummary>,
    ) -> Vec<(String, RelationReason)> {
        // Each distinct self type is declared once in the owner's file.
        let mut declared: Vec<&str> = impls
            .iter()
            .filter_map(|header| header.rsplit_once(" for ").map(|(_, ty)| ty))
            .collect();
        declared.sort_unstable();
        declared.dedup();
        let owner_source: String = declared
            .iter()
            .map(|ty| format!("pub struct {ty};\n"))
            .collect();
        let mut files = std::collections::BTreeMap::new();
        for (path, source) in std::iter::once(("src/error.rs".to_string(), owner_source.as_str()))
            .chain(
                extra_sources
                    .iter()
                    .enumerate()
                    .map(|(at, source)| (format!("src/extra{at}.rs"), *source)),
            )
        {
            files.insert(
                PathBuf::from(&path),
                FileFacts {
                    path: PathBuf::from(&path),
                    source: source.into(),
                    ..FileFacts::default()
                },
            );
        }
        let mut functions: Vec<FunctionSummary> = impls
            .iter()
            .map(|header| fmt_function("src/error.rs", header))
            .collect();
        let owner = functions[owner].clone();
        functions.extend(extra_functions);
        let tests: Vec<TestSummary> = bodies
            .iter()
            .enumerate()
            .map(|(at, body)| test_with_body(&format!("t{at}"), body))
            .collect();
        let index = RustIndex::from_owned(OwnedRustIndex {
            functions,
            tests,
            files,
            ..Default::default()
        });
        find_related_tests(&probe(), Some(&owner), &index, true, None, None)
            .into_iter()
            .map(|(test, reason)| (test.name.clone(), reason))
            .collect()
    }

    fn direct(reasons: &[(String, RelationReason)], name: &str) -> bool {
        reasons
            .iter()
            .any(|(test, reason)| test == name && *reason == RelationReason::DirectOwnerCall)
    }

    #[test]
    fn debug_format_of_constructed_value_reaches_debug_fmt() {
        let related = reasons(
            &["fmt::Debug for DecodeError"],
            0,
            &[
                "let err = DecodeError::InvalidLastSymbol(1, b'0');\nassert_eq!(format!(\"{:?}\", err), \"InvalidLastSymbol(1, 48)\");",
                "assert_eq!(format!(\"{:?}\", DecodeError::InvalidByte(0, 1)), \"x\");",
                "let err = DecodeError::InvalidByte(0, 1);\nassert_eq!(format!(\"{err:#?}\"), \"x\");",
                "let mut out = String::new();\nlet e = DecodeError::InvalidByte(0, 1);\nwrite!(out, \"{:?}\", &e).unwrap();",
            ],
        );
        for name in ["t0", "t1", "t2", "t3"] {
            assert!(direct(&related, name), "{name}: {related:?}");
        }
    }

    #[test]
    fn display_dispatch_covers_braces_and_to_string() {
        let related = reasons(
            &["fmt::Display for DecodeError"],
            0,
            &[
                "let err = DecodeError::InvalidByte(0, 1);\nassert_eq!(format!(\"{}\", err), \"x\");",
                "let err = DecodeError::InvalidByte(0, 1);\nassert_eq!(err.to_string(), \"x\");",
                "println!(\"{}: {}\", 7, DecodeError::InvalidByte(0, 1));",
            ],
        );
        for name in ["t0", "t1", "t2"] {
            assert!(direct(&related, name), "{name}: {related:?}");
        }
    }

    #[test]
    fn mismatched_trait_unrelated_type_and_unknown_value_do_not_reach() {
        let related = reasons(
            &["fmt::Debug for DecodeError"],
            0,
            &[
                // Display placeholder cannot run Debug::fmt.
                "let err = DecodeError::InvalidByte(0, 1);\nformat!(\"{}\", err);",
                // Debug of an unrelated type.
                "let other = EncodeError::Overflow;\nassert_eq!(format!(\"{:?}\", other), \"x\");",
                // Argument of unknown type.
                "let err = decode(input).unwrap_err();\nformat!(\"{:?}\", err);",
                // Another placeholder's argument is the typed one; this one is not.
                "let err = DecodeError::InvalidByte(0, 1);\nformat!(\"{:?} {}\", 1, err);",
                // `to_string` is Display, not Debug.
                "let err = DecodeError::InvalidByte(0, 1);\nerr.to_string();",
                // `$` width arguments shift positions; fail closed.
                "let err = DecodeError::InvalidByte(0, 1);\nformat!(\"{:1$?}\", err, 5);",
                // Typed only after the macro.
                "format!(\"{:?}\", err);\nlet err = DecodeError::InvalidByte(0, 1);",
            ],
        );
        assert!(related.is_empty(), "{related:?}");
    }

    #[test]
    fn ambiguous_fmt_impls_for_one_type_fail_closed() {
        let related = reasons(
            &["fmt::Debug for DecodeError", "fmt::Debug for DecodeError"],
            0,
            &["let e = DecodeError::InvalidByte(0, 1);\nformat!(\"{:?}\", e);"],
        );
        assert!(related.is_empty(), "{related:?}");
    }

    #[test]
    fn sibling_impl_of_another_type_is_not_linked() {
        // The test formats a `DecodeError`; the changed owner is the
        // `EncodeError` Debug impl.
        let related = reasons(
            &["fmt::Debug for DecodeError", "fmt::Debug for EncodeError"],
            1,
            &["let e = DecodeError::InvalidByte(0, 1);\nformat!(\"{:?}\", e);"],
        );
        assert!(related.is_empty(), "{related:?}");
    }

    #[test]
    fn same_named_type_declared_elsewhere_fails_closed() {
        // Another `DecodeError` (say with a derived `Debug`) makes the bare
        // type name ambiguous, so the formatted value may be that one.
        let related = reasons_with(
            &["fmt::Debug for DecodeError"],
            0,
            &["let e = DecodeError::InvalidByte(0, 1);\nformat!(\"{:?}\", e);"],
            &["#[derive(Debug)]\npub enum DecodeError { A }\n"],
        );
        assert!(related.is_empty(), "{related:?}");
        // A mention in a comment or string is not a declaration.
        let related = reasons_with(
            &["fmt::Debug for DecodeError"],
            0,
            &["let e = DecodeError::InvalidByte(0, 1);\nformat!(\"{:?}\", e);"],
            &["// pub struct DecodeError;\nconst S: &str = \"enum DecodeError\";\n"],
        );
        assert!(direct(&related, "t0"), "{related:?}");
    }

    #[test]
    fn display_and_debug_impls_of_one_type_are_told_apart() {
        let impls = ["fmt::Display for DecodeError", "fmt::Debug for DecodeError"];
        let display_only = "let e = DecodeError::InvalidByte(0, 1);\nformat!(\"{}\", e);";
        let debug_only = "let e = DecodeError::InvalidByte(0, 1);\nformat!(\"{:?}\", e);";
        let display_owner = reasons(&impls, 0, &[display_only, debug_only]);
        assert!(direct(&display_owner, "t0"), "{display_owner:?}");
        assert!(!display_owner.iter().any(|(name, _)| name == "t1"));
        let debug_owner = reasons(&impls, 1, &[display_only, debug_only]);
        assert!(direct(&debug_owner, "t1"), "{debug_owner:?}");
        assert!(!debug_owner.iter().any(|(name, _)| name == "t0"));
    }

    #[test]
    fn placeholder_and_macro_forms_are_parsed() {
        let display = |body: &str| {
            let related = reasons(&["fmt::Display for DecodeError"], 0, &[body]);
            direct(&related, "t0")
        };
        let ctor = "let e = DecodeError::InvalidByte(0, 1);\n";
        // Positional index, named argument, width spec, escaped braces.
        assert!(display(&format!("{ctor}format!(\"{{1}} {{0}}\", 1, e);")));
        assert!(display(&format!("{ctor}format!(\"{{v}}\", v = e);")));
        assert!(display(&format!("{ctor}format!(\"{{:>8}}\", e);")));
        assert!(display(&format!("{ctor}format!(\"{{{{}}}} {{}}\", e);")));
        assert!(display(&format!("{ctor}println!(\"{{}}\", e);")));
        assert!(display(&format!(
            "{ctor}let mut s = String::new();\nwriteln!(s, \"{{}}\", e).unwrap();"
        )));
        assert!(display("format!(\"{}\", DecodeError);"));
        // The Display placeholder takes the other argument; hex ignores `e`.
        assert!(!display(&format!("{ctor}format!(\"{{0}}\", 1, e);")));
        assert!(!display(&format!("{ctor}format!(\"{{:x}}\", e);")));
        assert!(!display(&format!(
            "{ctor}format!(\"{{0:{{width}}}}\", e, width = 5);"
        )));
        assert!(!display(&format!("{ctor}format!(\"{{{{}}}}\", e);")));
        assert!(!display(&format!(
            "{ctor}format!(\"{{v}}\", v = 1, w = e);"
        )));
        // Not a formatting macro, or destination mistaken for the value.
        assert!(!display(&format!(
            "{ctor}assert!(matches!(e, DecodeError::A), \"{{}}\", 1);"
        )));
        assert!(!display(&format!("{ctor}write!(e, \"{{}}\", 1);")));
    }

    const CTOR: &str = "let e = DecodeError::InvalidByte(0, 1);\n";

    #[test]
    fn rebound_formatting_macros_are_not_credited() {
        let body = format!("{CTOR}format!(\"{{}}\", e);");
        let owner = ["fmt::Display for DecodeError"];
        let local = reasons_with(
            &owner,
            0,
            &[&body],
            &["macro_rules! format { ($($t:tt)*) => {}; }"],
        );
        assert!(local.is_empty(), "{local:?}");
        let imported = reasons_with(&owner, 0, &[&body], &["use mylog::format;\n"]);
        assert!(imported.is_empty(), "{imported:?}");
        let std_import = reasons_with(&owner, 0, &[&body], &["use std::format;\n"]);
        assert!(direct(&std_import, "t0"), "{std_import:?}");
        // Rebinding `println` leaves `format!` credited.
        let other = reasons_with(&owner, 0, &[&body], &["use mylog::println;\n"]);
        assert!(direct(&other, "t0"), "{other:?}");
        let qualified = format!("{CTOR}mylog::format!(\"{{}}\", e);");
        assert!(reasons(&owner, 0, &[&qualified]).is_empty());
        let std_qualified = format!("{CTOR}std::format!(\"{{}}\", e);");
        assert!(direct(&reasons(&owner, 0, &[&std_qualified]), "t0"));
    }

    #[test]
    fn associated_consts_and_factories_are_not_typed_as_self() {
        let owner = ["fmt::Debug for DecodeError"];
        for body in [
            "format!(\"{:?}\", DecodeError::LABEL);".to_string(),
            "format!(\"{:?}\", DecodeError::make());".to_string(),
            "let x = DecodeError::make();\nformat!(\"{:?}\", x);".to_string(),
            "let x = DecodeError::LABEL;\nformat!(\"{:?}\", x);".to_string(),
            "let x = DecodeError::new().unwrap();\nformat!(\"{:?}\", x);".to_string(),
        ] {
            assert!(reasons(&owner, 0, &[&body]).is_empty(), "{body}");
        }
        let annotated = "let x: DecodeError = make();\nformat!(\"{:?}\", x);";
        assert!(direct(&reasons(&owner, 0, &[annotated]), "t0"));
        let unit_variant = "format!(\"{:?}\", DecodeError::Overflow);";
        assert!(direct(&reasons(&owner, 0, &[unit_variant]), "t0"));
    }

    #[test]
    fn inherent_to_string_wins_over_display() {
        let body = format!("{CTOR}assert_eq!(e.to_string(), \"x\");");
        let owner = ["fmt::Display for DecodeError"];
        assert!(
            direct(&reasons(&owner, 0, &[&body]), "t0"),
            "blanket ToString"
        );
        let mut inherent = fmt_function("src/error.rs", "DecodeError");
        inherent.name = "to_string".to_string();
        inherent.id = SymbolId("src/error.rs::impl DecodeError::to_string".to_string());
        let related = reasons_full(&owner, 0, &[&body], &[], vec![inherent]);
        assert!(related.is_empty(), "{related:?}");
    }

    #[test]
    fn unrelated_fmt_call_does_not_hide_a_valid_dispatch() {
        let body = format!("{CTOR}let _ = fmt::Display::fmt(&other, f);\nformat!(\"{{}}\", e);");
        let related = reasons(
            &["fmt::Display for DecodeError", "fmt::Display for Other"],
            0,
            &[&body],
        );
        assert!(direct(&related, "t0"), "{related:?}");
    }

    #[test]
    fn placeholders_must_account_for_every_argument() {
        let owner = ["fmt::Display for DecodeError"];
        for body in [
            // Turbofish and closure commas split one argument into several.
            format!("{CTOR}format!(\"{{:?}} {{:?}} {{}}\", g::<A, B>(), e, y);"),
            format!("{CTOR}format!(\"{{:?}} {{:?}} {{}}\", f(|a, b| a), e, y);"),
            // Unused positional, out-of-range index, unused named argument.
            format!("{CTOR}format!(\"{{}}\", e, 1);"),
            format!("{CTOR}format!(\"{{1}}\", e);"),
            format!("{CTOR}format!(\"{{}}\", e, unused = 1);"),
            format!("{CTOR}format!(\"{{}} {{}}\", e);"),
        ] {
            assert!(reasons(&owner, 0, &[&body]).is_empty(), "{body}");
        }
    }

    #[test]
    fn variant_named_like_the_type_is_not_the_type() {
        let owner = ["fmt::Debug for DecodeError"];
        for body in [
            "format!(\"{:?}\", Error::DecodeError(inner));".to_string(),
            "let e = Error::DecodeError(inner);\nformat!(\"{:?}\", e);".to_string(),
        ] {
            assert!(reasons(&owner, 0, &[&body]).is_empty(), "{body}");
        }
        for body in [
            "format!(\"{:?}\", DecodeError(inner));",
            "format!(\"{:?}\", m::DecodeError(inner));",
            "let e = crate::m::DecodeError(inner);\nformat!(\"{:?}\", e);",
        ] {
            assert!(direct(&reasons(&owner, 0, &[body]), "t0"), "{body}");
        }
    }

    #[test]
    fn rebinding_in_scope_of_the_macro_refuses() {
        let owner = ["fmt::Debug for DecodeError"];
        let typed = "let e = DecodeError::InvalidByte(0, 1);\n";
        let fmt = "format!(\"{:?}\", e)";
        for scoped in [
            format!("if let Some(e) = found {{\n{fmt};\n}}"),
            format!("for e in items {{\n{fmt};\n}}"),
            format!("while let Some(e) = next() {{\n{fmt};\n}}"),
            format!("let f = |e| {{ {fmt} }};"),
            format!("match found {{ Some(e) => {fmt}, None => String::new() }};"),
            format!("let e = other();\n{fmt};"),
            format!("match found {{ None => 0, Some(e) => {fmt} }};"),
            format!("let W {{ e, .. }} = w;\n{fmt};"),
            format!("consume(e);\n{fmt};"),
            format!("fn inner(e: u8) {{ {fmt}; }}"),
        ] {
            let body = format!("{typed}{scoped}");
            assert!(reasons(&owner, 0, &[&body]).is_empty(), "{body}");
        }
        // An unrelated binder name leaves `e` typed.
        let body = format!("{typed}if let Some(x) = found {{\n{fmt};\n}}");
        assert!(direct(&reasons(&owner, 0, &[&body]), "t0"), "{body}");
    }

    #[test]
    fn crate_qualified_variant_named_like_the_type_is_not_the_type() {
        let owner = ["fmt::Debug for DecodeError"];
        let body = "format!(\"{:?}\", crate::Error::DecodeError(inner));";
        assert!(reasons(&owner, 0, &[body]).is_empty());
    }

    #[test]
    fn binders_nested_inside_formatting_macro_arguments_refuse() {
        let owner = ["fmt::Debug for DecodeError"];
        let typed = "let e = DecodeError::InvalidByte(0, 1);\n";
        for nested in [
            "format!(\"{}\", match r { Ok(e) => format!(\"{:?}\", e), Err(_) => String::new() });",
            "format!(\"{}\", if let Some(e) = f { format!(\"{:?}\", e) } else { String::new() });",
            "format!(\"{}\", { let W { e, .. } = w; format!(\"{:?}\", e) });",
        ] {
            let body = format!("{typed}{nested}");
            assert!(reasons(&owner, 0, &[&body]).is_empty(), "{body}");
        }
        // A plain nested macro still credits.
        let body = format!("{typed}format!(\"{{}}\", format!(\"{{:?}}\", e));");
        assert!(direct(&reasons(&owner, 0, &[&body]), "t0"), "{body}");
    }
}
