//! Sibling impls of one generic trait on one type (#14074).
//!
//! `impl From<u32> for Cents` and `impl From<&str> for Cents` share a self
//! type and a method name, so a relation that resolves a call by self type
//! alone credits `Cents::from(2u32)` to both. The trait argument is what
//! selects the impl, and for these traits it is the type of the call's first
//! non-`self` argument.
//!
//! This reading only refuses, and only on positive evidence: every call of the
//! name in the test passes an argument whose type is written in the test
//! (a suffixed literal, a string literal, an annotated or literal-initialised
//! `let`, a struct, tuple-struct or variant literal) and that type is a
//! sibling's trait argument, not the owner's. A literal whose type is left to
//! inference still has a kind: an unsuffixed integer is some integer type and
//! `2.0` some float, never `&str`, so it refuses an owner whose argument is a
//! primitive outside that kind when a sibling's argument is inside it. An
//! owner argument that is not a primitive may be an alias of one and is left
//! alone. The same holds for a `let` binding initialised by such a literal
//! (#14074).
//! Anything else whose type would need inference keeps the relation the caller
//! already had, so an unreadable call never turns into a new gap.

use super::{
    FunctionContainer, FunctionSummary, RustIndex, TestSummary, compact_impl_type_name,
    first_argument_has_type, first_argument_span, ident_boundary, is_ident_byte,
    live_let_statement_before, mask_comments_and_strings, receiver_binding_shadowed_at,
    skip_ws_if_paren, suffixed_numeric_literal_type,
};
use crate::analysis::classify::transitive_reach::IMPLICIT_DISPATCH_METHODS;
use crate::analysis::syntax::fn_signature::{OwnerContainer, owner_signature_at_line};

/// Whether every call of `owner`'s name in `test` provably selects a sibling
/// impl of the same generic trait on the same type, so none of them can run
/// `owner`. `false` whenever any part of that is not established.
pub(super) fn every_call_selects_generic_sibling(
    test: &TestSummary,
    owner: &FunctionSummary,
    index: &RustIndex,
) -> bool {
    let FunctionContainer::TraitImpl {
        trait_path,
        self_ty,
        ..
    } = &owner.item.container
    else {
        return false;
    };
    let Some(owner_argument) = sole_trait_argument(trait_path) else {
        return false;
    };
    let Some(owner_head) = selecting_type_head(owner_argument) else {
        return false;
    };
    let (Some(trait_base), Some(self_base)) = (
        compact_impl_type_name(trait_path),
        compact_impl_type_name(self_ty),
    ) else {
        return false;
    };
    let sibling_heads = sibling_argument_heads(owner, index, &trait_base, &self_base);
    // A sibling that reads as the owner's own argument (`From<Foo>` beside
    // `From<&Foo>`) is told apart only by a reference this reading drops.
    if sibling_heads.is_empty() || sibling_heads.contains(&owner_head) {
        return false;
    }
    let masked = mask_comments_and_strings(&test.body);
    // An unqualified primitive name is the primitive unless the owner's file
    // declares or imports an item of that name (`use aliases::u8;`).
    let owner_is_primitive = PRIMITIVE_TYPES.contains(&owner_head.as_str())
        && !owner_argument.contains("::")
        && index.files().get(&owner.file).is_some_and(|file| {
            !file_rebinds_name(file.data().source.as_ref(), &owner_head, owner.start_line)
        });
    if has_unnamed_route(owner.name.as_str(), &masked)
        || !every_site_passes_sibling_type(
            test,
            owner,
            &masked,
            &owner_head,
            owner_is_primitive,
            &sibling_heads,
            owner_argument.starts_with('&'),
        )
    {
        return false;
    }
    // Last, because it parses the owner's file: the owner's first non-`self`
    // parameter must be the trait argument itself, in an impl and a function
    // that declare no type generics, so the argument's type is the selector.
    owner_first_parameter_head(owner, index).is_some_and(|head| head == owner_head)
}

/// The single generic argument of a trait path (`From<&str>` → `&str`).
/// `None` for a plain trait, two or more arguments, or an associated-type
/// binding.
fn sole_trait_argument(trait_path: &str) -> Option<&str> {
    let open = trait_path.find('<')?;
    let inner = trait_path[open + 1..].trim_end().strip_suffix('>')?;
    let mut depth = 0usize;
    for byte in inner.bytes() {
        match byte {
            b'<' | b'(' | b'[' => depth += 1,
            b'>' | b')' | b']' => depth = depth.checked_sub(1)?,
            b',' | b'=' if depth == 0 => return None,
            _ => {}
        }
    }
    Some(inner.trim())
}

/// The last path segment of a type written as a plain path, with one leading
/// reference (and its lifetime and `mut`) dropped: `&'a str` → `str`,
/// `units::Meters` → `Meters`. `None` for anything with generic arguments,
/// a tuple, a slice, a pointer, `dyn`/`impl`, or a second reference: those
/// have no single head a call-site argument could be compared with.
fn selecting_type_head(ty: &str) -> Option<String> {
    let mut ty = ty.trim();
    if let Some(rest) = ty.strip_prefix('&') {
        ty = rest.trim_start();
        if let Some(rest) = ty.strip_prefix('\'') {
            let lifetime_end = rest
                .bytes()
                .position(|byte| !is_ident_byte(byte))
                .unwrap_or(rest.len());
            ty = rest[lifetime_end..].trim_start();
        }
        if let Some(rest) = ty.strip_prefix("mut ") {
            ty = rest.trim_start();
        }
    }
    let head = ty.rsplit("::").next()?;
    let plain_path = ty
        .split("::")
        .all(|segment| !segment.is_empty() && segment.bytes().all(is_ident_byte));
    plain_path.then(|| head.to_string())
}

/// Trait-argument heads of the other impls of `trait_base` on `self_base`
/// in the owner's file that define the owner's method. An impl in another
/// file may be on another type of the same name, and one whose argument has
/// no plain head contributes nothing: no call-site type can be matched
/// against it.
fn sibling_argument_heads(
    owner: &FunctionSummary,
    index: &RustIndex,
    trait_base: &str,
    self_base: &str,
) -> Vec<String> {
    let mut heads = Vec::new();
    let files = index.files();
    let Some(file) = files.get(&owner.file) else {
        return heads;
    };
    for function in file.functions.iter() {
        if function.name != owner.name || function.id == owner.id {
            continue;
        }
        let FunctionContainer::TraitImpl {
            trait_path,
            self_ty,
            ..
        } = &function.item.container
        else {
            continue;
        };
        if compact_impl_type_name(trait_path).as_deref() != Some(trait_base)
            || compact_impl_type_name(self_ty).as_deref() != Some(self_base)
        {
            continue;
        }
        if let Some(head) = sole_trait_argument(trait_path).and_then(selecting_type_head)
            && !heads.contains(&head)
        {
            heads.push(head);
        }
    }
    heads
}

/// Whether the test may run the owner without naming it. A method the
/// language or the standard library calls through syntax is left to the
/// readings that own that syntax; `from` and `try_from` are also reached by
/// `into`/`try_into`, and `from` by `?`.
fn has_unnamed_route(name: &str, masked: &str) -> bool {
    let names_word = |word: &str| {
        masked
            .match_indices(word)
            .any(|(at, matched)| ident_boundary(masked, at, at + matched.len()))
    };
    match name {
        "from" => names_word("into") || masked.contains('?'),
        "try_from" => names_word("try_into"),
        _ => IMPLICIT_DISPATCH_METHODS.contains(&name),
    }
}

/// Whether the test names the owner's method at least once and every mention
/// is a call, in the spelling whose first argument is the selecting one,
/// passing a value typed as one of `sibling_heads`. A method with `self` is
/// read at `value.name(arg)` only (`Trait::name(value, arg)` puts the
/// receiver first); an associated function at `Path::name(arg)` only. A
/// mention that is not a call (`map(Cents::from)`), a turbofish, or the other
/// spelling is not read, so the answer is `false`.
///
/// `owner_takes_reference`: the owner's parameter is a reference, which a
/// borrowed argument of another type can reach by deref coercion
/// (`&String` for `&str`), so a borrowed argument is not read either.
fn every_site_passes_sibling_type(
    test: &TestSummary,
    owner: &FunctionSummary,
    masked: &str,
    owner_head: &str,
    owner_is_primitive: bool,
    sibling_heads: &[String],
    owner_takes_reference: bool,
) -> bool {
    let name = owner.name.as_str();
    let bytes = masked.as_bytes();
    let mut sites = 0usize;
    for (at, matched) in masked.match_indices(name) {
        let after = at + matched.len();
        if !ident_boundary(masked, at, after) {
            continue;
        }
        let before = masked[..at].trim_end();
        // A nested `fn` of this name rebinds it for the rest of the body.
        if before.ends_with("fn") && ident_boundary(before, before.len() - 2, before.len()) {
            return false;
        }
        if !skip_ws_if_paren(masked, after) {
            return false;
        }
        let method_site = at > 0 && bytes[at - 1] == b'.';
        if method_site != owner.item.has_self_param {
            return false;
        }
        if owner_takes_reference
            && first_argument_span(masked, after)
                .and_then(|(start, end)| masked.get(start..end))
                .is_none_or(|argument| argument.trim_start().starts_with('&'))
        {
            return false;
        }
        let lets_before = masked[..at].rfind(';').map_or("", |end| &masked[..=end]);
        let passes_sibling = sibling_heads.iter().any(|head| {
            first_argument_has_type(masked, after, head, lets_before, masked, &test.body, at)
        }) || (sibling_heads.iter().any(|head| head == "str")
            && first_argument_is_string_literal(masked, &test.body, after))
            || literal_kind_rules_out_owner(
                test,
                masked,
                after,
                at,
                lets_before,
                Selectors {
                    owner_head,
                    owner_is_primitive,
                    sibling_heads,
                },
            );
        if !passes_sibling {
            return false;
        }
        sites += 1;
    }
    sites > 0
}

/// The owner's and its siblings' selecting argument heads.
#[derive(Clone, Copy)]
struct Selectors<'a> {
    owner_head: &'a str,
    /// The owner's head is an unqualified primitive name that its file does
    /// not rebind.
    owner_is_primitive: bool,
    sibling_heads: &'a [String],
}

/// The kind of value a literal expression has before inference picks its
/// exact type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LiteralKind<'a> {
    /// A suffixed numeric literal names its type (`2u32`, `1.5f32`).
    Exact(&'a str),
    /// An unsuffixed integer literal is one of the integer primitives.
    Integer,
    /// An unsuffixed float literal (`2.0`, `1e3`) is `f32` or `f64`.
    Float,
    /// A plain string literal is `&'static str`.
    Str,
}

const INTEGER_TYPES: [&str; 12] = [
    "i8", "i16", "i32", "i64", "i128", "isize", "u8", "u16", "u32", "u64", "u128", "usize",
];
const FLOAT_TYPES: [&str; 2] = ["f32", "f64"];
/// Primitive names, which no alias can stand behind.
const PRIMITIVE_TYPES: [&str; 17] = [
    "i8", "i16", "i32", "i64", "i128", "isize", "u8", "u16", "u32", "u64", "u128", "usize", "f32",
    "f64", "str", "bool", "char",
];

impl LiteralKind<'_> {
    fn admits(self, head: &str) -> bool {
        match self {
            LiteralKind::Exact(ty) => ty == head,
            LiteralKind::Integer => INTEGER_TYPES.contains(&head),
            LiteralKind::Float => FLOAT_TYPES.contains(&head),
            LiteralKind::Str => head == "str",
        }
    }
}

/// The kind of an expression when it is one numeric or plain string literal,
/// optionally negated. `masked` and `raw` are the same span of the masked and
/// the original test body. `None` for anything else, including a literal
/// that is only the start of a larger expression.
fn literal_kind<'a>(masked: &'a str, raw: &str) -> Option<LiteralKind<'a>> {
    // Masking keeps byte offsets, so one trim of the original text places
    // the same span in both.
    let trimmed = raw.trim_start();
    let lead = raw.len() - trimmed.len();
    let trimmed = trimmed.trim_end();
    let trimmed = trimmed.strip_suffix(';').map_or(trimmed, str::trim_end);
    let expr = masked.get(lead..lead + trimmed.len())?;
    if trimmed.len() >= 2 && trimmed.starts_with('"') && trimmed.ends_with('"') {
        // Masking blanks a literal's contents, so anything left inside the
        // span other than whitespace means the text is a larger expression.
        return expr
            .get(1..expr.len() - 1)?
            .chars()
            .all(char::is_whitespace)
            .then_some(LiteralKind::Str);
    }
    let number = expr.strip_prefix('-').map_or(expr, str::trim_start);
    if let Some(ty) = suffixed_numeric_literal_type(number) {
        return Some(LiteralKind::Exact(ty));
    }
    let bytes = number.as_bytes();
    if !bytes.first().is_some_and(u8::is_ascii_digit) {
        return None;
    }
    if let Some(digits) = number
        .strip_prefix("0x")
        .or_else(|| number.strip_prefix("0o"))
        .or_else(|| number.strip_prefix("0b"))
    {
        // Radix digits are checked loosely: `0x1f` and `0b10` are integers,
        // and anything that is not a digit or `_` is not a literal.
        return (!digits.is_empty() && digits.bytes().all(|b| b == b'_' || b.is_ascii_hexdigit()))
            .then_some(LiteralKind::Integer);
    }
    if bytes.iter().all(|&b| b == b'_' || b.is_ascii_digit()) {
        return Some(LiteralKind::Integer);
    }
    // `2.0`, `2.`, `1e3`, `1.5e-3`: digits, at most one `.`, an optional
    // exponent, and nothing that would make it a method call (`2.max(1)`).
    let (mantissa, exponent) = match number.find(['e', 'E']) {
        Some(e) => (&number[..e], Some(&number[e + 1..])),
        None => (number, None),
    };
    let mantissa_ok = mantissa.bytes().filter(|&b| b == b'.').count() <= 1
        && mantissa
            .bytes()
            .all(|b| b == b'.' || b == b'_' || b.is_ascii_digit());
    let exponent_ok = exponent.is_none_or(|exp| {
        let exp = exp.strip_prefix(['+', '-']).unwrap_or(exp);
        !exp.is_empty() && exp.bytes().all(|b| b == b'_' || b.is_ascii_digit())
    });
    let is_float = mantissa.contains('.') || exponent.is_some();
    (mantissa_ok && exponent_ok && is_float).then_some(LiteralKind::Float)
}

/// Whether the first argument of the call whose name ends at `after` is a
/// literal, or a `let` binding initialised by one with no annotation, whose
/// kind cannot be the owner's trait argument but can be a sibling's. An
/// annotated binding is read by `first_argument_has_type`; a shadowed or
/// unreadable binding is not read at all.
fn literal_kind_rules_out_owner(
    test: &TestSummary,
    masked: &str,
    after: usize,
    call_at: usize,
    lets_before: &str,
    selectors: Selectors<'_>,
) -> bool {
    let Selectors {
        owner_head,
        owner_is_primitive,
        sibling_heads,
    } = selectors;
    let Some((start, end)) = first_argument_span(masked, after) else {
        return false;
    };
    let (Some(masked_argument), Some(raw_argument)) =
        (masked.get(start..end), test.body.get(start..end))
    else {
        return false;
    };
    let argument = masked_argument.trim();
    // A digit is an identifier byte, so a bare number must not be read as a
    // binding name.
    let is_binding = argument.bytes().next().is_some_and(|b| !b.is_ascii_digit())
        && argument.bytes().all(is_ident_byte);
    let kind = if is_binding {
        if receiver_binding_shadowed_at(&test.body, argument, call_at.saturating_sub(1)) {
            return false;
        }
        let Some(statement) =
            live_let_statement_before(lets_before, masked, &test.body, argument, call_at)
        else {
            return false;
        };
        let Some(eq) = statement.find('=') else {
            return false;
        };
        if statement[..eq].contains(':') {
            return false;
        }
        // The statement is a slice of `masked`; the same bytes of the
        // original body still hold the literal's quotes.
        let offset = (statement.as_ptr() as usize).wrapping_sub(masked.as_ptr() as usize);
        // The lookup reads plain `let name` declarations only, so a later
        // destructuring `let` that binds the name hides from it.
        if masked
            .get(offset + statement.len()..call_at)
            .is_none_or(|between| let_pattern_binds(between, argument))
        {
            return false;
        }
        let initializer = offset + eq + 1..offset + statement.len();
        let (Some(masked_init), Some(raw_init)) =
            (masked.get(initializer.clone()), test.body.get(initializer))
        else {
            return false;
        };
        literal_kind(masked_init, raw_init)
    } else {
        literal_kind(masked_argument, raw_argument)
    };
    kind.is_some_and(|kind| {
        // Only a suffixed literal names one type. Any other kind could be the
        // owner's argument through an alias (`type Count = i32;` makes
        // `From<Count>` take an unsuffixed `2`), so it rules the owner out
        // only when the owner's argument is itself a primitive.
        // A qualified spelling (`aliases::u8`) is a path, not the primitive.
        let owner_is_named = matches!(kind, LiteralKind::Exact(_)) || owner_is_primitive;
        owner_is_named
            && !kind.admits(owner_head)
            && sibling_heads.iter().any(|head| kind.admits(head))
    })
}

/// Whether `source` declares an item named `name` or names it in a `use`
/// statement, so an unqualified `name` may not be the primitive. A glob
/// import whose module encloses the owner (which starts on `owner_line`,
/// 1-based) may bring in an alias of that name, so it counts too.
fn file_rebinds_name(source: &str, name: &str, owner_line: usize) -> bool {
    const DECLARING: [&str; 5] = ["type", "struct", "enum", "union", "trait"];
    let masked = mask_comments_and_strings(source);
    // The owner's whole start line: a module opened on the same line as the
    // owner (`mod m { use x::*; impl .. { fn f(..) } }`) may enclose it.
    let line_start = masked
        .split_inclusive('\n')
        .take(owner_line.saturating_sub(1))
        .map(str::len)
        .sum::<usize>();
    let line_end = masked[line_start..]
        .find('\n')
        .map_or(masked.len(), |end| line_start + end);
    if has_glob_import_enclosing(&masked, line_start..line_end) {
        return true;
    }
    masked.match_indices(name).any(|(at, found)| {
        if !ident_boundary(&masked, at, at + found.len()) {
            return false;
        }
        let before = masked[..at].trim_end();
        let previous_word_start = before
            .rfind(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
            .map_or(0, |i| i + 1);
        if DECLARING.contains(&&before[previous_word_start..]) {
            return true;
        }
        // Inside a `use` statement: a `use` keyword after the last `;` or
        // closing brace before this mention.
        let statement_start = before.rfind([';', '}']).map_or(0, |i| i + 1);
        let statement = &before[statement_start..];
        statement
            .match_indices("use")
            .any(|(start, matched)| ident_boundary(statement, start, start + matched.len()))
    })
}

/// Whether masked source has a glob import in a block that may hold the
/// owner, whose start line spans `owner_line`: a `*` written as a path
/// segment (after `::`, `{` or `,`) in a statement with a `use` keyword after
/// the previous `;`, where the block around that `use` (or the file, at the
/// top level) overlaps the owner's line. Any overlap counts, so a module on
/// the owner's line is read as enclosing it, which keeps the direct call. A
/// glob in a sibling or nested module (a test module's `use super::*;`)
/// cannot rebind a name the owner's signature resolves. A dereference after
/// `{` or `,` that happens to share a statement with a `use` only keeps the
/// direct call, which is the conservative answer.
fn has_glob_import_enclosing(masked: &str, owner_line: std::ops::Range<usize>) -> bool {
    masked.match_indices('*').any(|(at, _)| {
        let before = masked[..at].trim_end();
        if !before.ends_with(['{', ',']) && !before.ends_with("::") {
            return false;
        }
        let statement_start = before.rfind(';').map_or(0, |i| i + 1);
        let statement = &before[statement_start..];
        statement
            .match_indices("use")
            .filter(|(start, matched)| ident_boundary(statement, *start, start + matched.len()))
            .any(|(start, _)| {
                enclosing_block(masked, statement_start + start).is_none_or(|block| {
                    block.start < owner_line.end && owner_line.start < block.end
                })
            })
    })
}

/// The byte range of the innermost `{ .. }` block around byte `at` in
/// masked source, or `None` at the top level. An unclosed block runs to the
/// end of the source.
fn enclosing_block(masked: &str, at: usize) -> Option<std::ops::Range<usize>> {
    let bytes = masked.as_bytes();
    let mut depth = 0usize;
    let open = (0..at).rev().find(|&i| match bytes[i] {
        b'}' => {
            depth += 1;
            false
        }
        b'{' if depth == 0 => true,
        b'{' => {
            depth -= 1;
            false
        }
        _ => false,
    })?;
    let mut depth = 0usize;
    let close = (open + 1..bytes.len())
        .find(|&i| match bytes[i] {
            b'{' => {
                depth += 1;
                false
            }
            b'}' if depth == 0 => true,
            b'}' => {
                depth -= 1;
                false
            }
            _ => false,
        })
        .unwrap_or(bytes.len());
    Some(open..close)
}

/// Whether any `let` in `text` binds `name` in its pattern (`let (n,) = ..`,
/// `let Pair { n, .. } = ..`, `let n = ..`).
fn let_pattern_binds(text: &str, name: &str) -> bool {
    text.match_indices("let").any(|(at, matched)| {
        let after = at + matched.len();
        if !ident_boundary(text, at, after) {
            return false;
        }
        let rest = &text[after..];
        let pattern = &rest[..rest.find(['=', ';']).unwrap_or(rest.len())];
        pattern
            .match_indices(name)
            .any(|(start, found)| ident_boundary(pattern, start, start + found.len()))
    })
}

/// Whether the first argument of the call whose name ends at `after_name` is
/// one plain string literal (`"abc"`). The masked body blanks
/// the literal's contents, so anything left between its quotes other than
/// whitespace means the argument is a larger expression.
fn first_argument_is_string_literal(masked: &str, raw: &str, after_name: usize) -> bool {
    let Some((start, end)) = first_argument_span(masked, after_name) else {
        return false;
    };
    let (Some(raw_arg), Some(masked_arg)) = (raw.get(start..end), masked.get(start..end)) else {
        return false;
    };
    let lead = raw_arg.len() - raw_arg.trim_start().len();
    let trail = raw_arg.len() - raw_arg.trim_end().len();
    let raw_arg = raw_arg.trim();
    if raw_arg.len() < 2 || !raw_arg.starts_with('"') || !raw_arg.ends_with('"') {
        return false;
    }
    masked_arg
        .get(lead + 1..masked_arg.len() - trail - 1)
        .is_some_and(|inner| inner.chars().all(char::is_whitespace))
}

/// Head of the owner's first non-`self` parameter type, when the owner sits
/// in a trait impl that declares no type generics and is not itself generic.
/// The index's copy of the owner's file is parsed, so this is asked last.
fn owner_first_parameter_head(owner: &FunctionSummary, index: &RustIndex) -> Option<String> {
    let files = index.files();
    let source = files.get(&owner.file)?.data().source.as_ref();
    let signature = owner_signature_at_line(source, owner.start_line)?;
    if signature.name != owner.name
        || signature.has_type_generics
        || signature.receiver.is_some() != owner.item.has_self_param
        || !matches!(signature.container, OwnerContainer::TraitImpl { .. })
    {
        return None;
    }
    selecting_type_head(&signature.params.first()?.ty)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::syntax::{RaRustSyntaxAdapter, RustSyntaxAdapter};
    use std::path::{Path, PathBuf};

    const CENTS: &str = "#[derive(Debug, PartialEq)]\npub struct Cents(pub u64);\npub struct Foo(pub u64);\n\
        impl From<u32> for Cents {\n    fn from(v: u32) -> Self { Cents(v as u64 * 100) }\n}\n\
        impl From<&str> for Cents {\n    fn from(v: &str) -> Self { Cents(v.len() as u64 + 3) }\n}\n";

    const SCALE: &str = "pub trait Scale<T> { fn scale(&self, by: T) -> u32; }\npub struct Bar(pub u32);\n\
        impl Scale<u8> for Bar {\n    fn scale(&self, by: u8) -> u32 { self.0 * by as u32 + 1 }\n}\n\
        impl Scale<f32> for Bar {\n    fn scale(&self, by: f32) -> u32 { (self.0 as f32 * by) as u32 + 2 }\n}\n";

    /// A parser-backed index of `production` followed by one test whose body
    /// is `test_body`, so the owner's container, signature and symbol id are
    /// the ones the real pipeline produces.
    fn parsed_index(production: &str, test_body: &str) -> Result<RustIndex, String> {
        let source = format!(
            "{production}\n#[cfg(test)]\nmod tests {{\n    use super::*;\n    #[test]\n    fn t() {{ {test_body} }}\n}}\n"
        );
        let path = Path::new("src/lib.rs");
        let facts = RaRustSyntaxAdapter.summarize_file(path, &source)?;
        let mut index = RustIndex::default();
        index.extend_tests(facts.tests.iter().cloned());
        index.extend_functions(facts.functions.iter().cloned());
        index.insert_file_only(PathBuf::from(path), facts);
        Ok(index)
    }

    /// Whether the full owner-call relation `ripr check` applies credits the
    /// fixture's one test as a direct call of the owner whose symbol id ends
    /// in `owner_suffix`.
    fn direct(production: &str, owner_suffix: &str, test_body: &str) -> Result<bool, String> {
        let index = parsed_index(production, test_body)?;
        let functions = index.functions();
        let mut owners = functions
            .iter()
            .filter(|function| function.id.0.ends_with(owner_suffix));
        let (Some(owner), None) = (owners.next(), owners.next()) else {
            return Err(format!("exactly one owner ends in `{owner_suffix}`"));
        };
        if !matches!(owner.item.container, FunctionContainer::TraitImpl { .. }) {
            return Err(format!("`{owner_suffix}` is a parsed trait-impl method"));
        }
        let tests = index.tests();
        let (Some(test), None) = (tests.iter().next(), tests.iter().nth(1)) else {
            return Err("exactly one test is indexed".to_string());
        };
        if !test.body.contains(owner.name.as_str()) {
            return Err(format!("the test body names `{}`", owner.name));
        }
        Ok(super::super::owner_call_is_direct(test, owner, &index))
    }

    /// #14074, the issue's repro in both directions: the only call passes a
    /// value written as the sibling impl's trait argument.
    #[test]
    fn a_call_typed_as_the_sibling_argument_is_not_a_direct_call_of_the_owner() -> Result<(), String>
    {
        for (owner, body) in [
            (
                "impl From<&str> for Cents::from",
                "assert_eq!(Cents::from(2u32), Cents(200));",
            ),
            (
                "impl From<&str> for Cents::from",
                "let n: u32 = 2; assert_eq!(Cents::from(n), Cents(200));",
            ),
            (
                "impl From<&str> for Cents::from",
                "assert_eq!(Cents::from(2u32), Cents(200)); assert_eq!(Cents::from(1u32), Cents(100));",
            ),
            (
                "impl From<u32> for Cents::from",
                r#"assert_eq!(Cents::from("abc"), Cents(6));"#,
            ),
        ] {
            assert!(!direct(CENTS, owner, body)?, "{owner} <- {body}");
        }
        for (owner, body) in [
            (
                "impl Scale<u8> for Bar::scale",
                "let b = Bar(2); assert_eq!(b.scale(2.0f32), 6);",
            ),
            (
                "impl Scale<f32> for Bar::scale",
                "let b = Bar(2); assert_eq!(b.scale(2u8), 5);",
            ),
        ] {
            assert!(!direct(SCALE, owner, body)?, "{owner} <- {body}");
        }
        Ok(())
    }

    /// #14074 residual: a literal whose exact type is left to inference
    /// still has a kind, and an unannotated `let` initialised by a literal
    /// has the literal's kind. An integer is never `&str`, a float is never
    /// `u8`, and a string literal is never `u32`.
    #[test]
    fn a_literal_kind_outside_the_owner_argument_is_not_a_direct_call() -> Result<(), String> {
        for (owner, body) in [
            (
                "impl From<&str> for Cents::from",
                "assert_eq!(Cents::from(2), Cents(200));",
            ),
            (
                "impl From<&str> for Cents::from",
                "let n = 2; assert_eq!(Cents::from(n), Cents(200));",
            ),
            (
                "impl From<&str> for Cents::from",
                "let n = 2u32; assert_eq!(Cents::from(n), Cents(200));",
            ),
            (
                "impl From<&str> for Cents::from",
                "let mut n = 0x2; n += 1; assert_eq!(Cents::from(n), Cents(300));",
            ),
            (
                "impl From<&str> for Cents::from",
                "assert_eq!(Cents::from( 2), Cents(200));",
            ),
            (
                "impl From<&str> for Cents::from",
                "assert_eq!(Cents::from(\n    2,\n), Cents(200));",
            ),
            (
                "impl From<u32> for Cents::from",
                r#"let s = "abc"; assert_eq!(Cents::from(s), Cents(6));"#,
            ),
        ] {
            assert!(!direct(CENTS, owner, body)?, "{owner} <- {body}");
        }
        for body in [
            "let b = Bar(2); assert_eq!(b.scale(2.0), 6);",
            "let b = Bar(2); assert_eq!(b.scale(1e3), 2002);",
            "let b = Bar(2); let by = 2.0; assert_eq!(b.scale(by), 6);",
        ] {
            assert!(
                !direct(SCALE, "impl Scale<u8> for Bar::scale", body)?,
                "{body}"
            );
        }
        Ok(())
    }

    /// The literal-kind reading keeps the direct call whenever the kind fits
    /// the owner's argument, the binding is rebound or shadowed, or the
    /// literal is only part of the argument.
    #[test]
    fn a_literal_kind_that_fits_the_owner_or_is_unread_keeps_the_direct_call() -> Result<(), String>
    {
        let owner = "impl From<&str> for Cents::from";
        for body in [
            r#"let s = "abc"; assert_eq!(Cents::from(s), Cents(6));"#,
            r#"let n = 2; let n = "ab"; assert_eq!(Cents::from(n), Cents(5));"#,
            r#"let n = 2; let f = |n: &str| Cents::from(n); assert_eq!(f("ab"), Cents(5)); assert_eq!(n, 2);"#,
            r#"let x = 1u32; assert_eq!(Cents::from(2 + x), Cents(300)); assert_eq!(Cents::from("ab"), Cents(5));"#,
            // A destructuring `let` rebinds the name after the plain one.
            r#"let n = 2; let (n,) = ("ab",); assert_eq!(Cents::from(n), Cents(5));"#,
            r#"let n = 2; let [n] = ["ab"]; assert_eq!(Cents::from(n), Cents(5));"#,
        ] {
            assert!(!refused(CENTS, owner, body)?, "{body}");
        }
        for body in [
            "let b = Bar(2); assert_eq!(b.scale(2), 5);",
            "let b = Bar(2); let by = 2; assert_eq!(b.scale(by), 5);",
        ] {
            assert!(
                !refused(SCALE, "impl Scale<u8> for Bar::scale", body)?,
                "{body}"
            );
        }
        // An unsuffixed `2` defaults to `i32`, which is `Count`: the owner
        // may run, so a non-primitive owner argument is not ruled out.
        const ALIASED: &str = "pub type Count = i32;\n#[derive(Debug, PartialEq)]\npub struct Cents(pub i64);\n\
            impl From<Count> for Cents {\n    fn from(v: Count) -> Self { Cents(v as i64 + 1) }\n}\n\
            impl From<u64> for Cents {\n    fn from(v: u64) -> Self { Cents(v as i64) }\n}\n";
        for body in [
            "assert_eq!(Cents::from(2), Cents(3));",
            "let n = 2; assert_eq!(Cents::from(n), Cents(3));",
        ] {
            assert!(
                !refused(ALIASED, "impl From<Count> for Cents::from", body)?,
                "{body}"
            );
        }
        assert!(refused(
            ALIASED,
            "impl From<Count> for Cents::from",
            "assert_eq!(Cents::from(2u64), Cents(2));"
        )?);
        // An imported alias with a primitive's name is not the primitive.
        const IMPORTED: &str = "pub mod aliases { pub type u8 = f64; }\nuse aliases::u8;\npub trait Scale<T> { fn scale(&self, by: T) -> u32; }\npub struct Bar(pub u32);\n\
            impl Scale<u8> for Bar {\n    fn scale(&self, by: u8) -> u32 { (self.0 as f64 * by) as u32 + 1 }\n}\n\
            impl Scale<f32> for Bar {\n    fn scale(&self, by: f32) -> u32 { (self.0 as f32 * by) as u32 + 2 }\n}\n";
        assert!(!refused(
            IMPORTED,
            "impl Scale<u8> for Bar::scale",
            "let b = Bar(2); let by = 2.0; let _: f64 = by; assert_eq!(b.scale(by), 5);"
        )?);
        // A glob import from another file may bring in an alias with a
        // primitive's name, so the name is not read as the primitive.
        const GLOB: &str = "mod aliases;\nuse aliases::*;\npub trait Scale<T> { fn scale(&self, by: T) -> u32; }\npub struct Bar(pub u32);\n\
            impl Scale<u8> for Bar {\n    fn scale(&self, by: u8) -> u32 { (self.0 as f64 * by) as u32 + 1 }\n}\n\
            impl Scale<f32> for Bar {\n    fn scale(&self, by: f32) -> u32 { (self.0 as f32 * by) as u32 + 2 }\n}\n";
        assert!(!refused(
            GLOB,
            "impl Scale<u8> for Bar::scale",
            "let b = Bar(2); let by = 2.0; let _: f64 = by; assert_eq!(b.scale(by), 5);"
        )?);
        // A path spelling a primitive's name is not the primitive.
        const QUALIFIED: &str = "pub mod aliases { pub type u8 = f64; }\npub trait Scale<T> { fn scale(&self, by: T) -> u32; }\npub struct Bar(pub u32);\n\
            impl Scale<aliases::u8> for Bar {\n    fn scale(&self, by: aliases::u8) -> u32 { (self.0 as f64 * by) as u32 + 1 }\n}\n\
            impl Scale<f32> for Bar {\n    fn scale(&self, by: f32) -> u32 { (self.0 as f32 * by) as u32 + 2 }\n}\n";
        assert!(!refused(
            QUALIFIED,
            "impl Scale<aliases::u8> for Bar::scale",
            "let b = Bar(2); let by = 2.0; let _: f64 = by; assert_eq!(b.scale(by), 5);"
        )?);
        Ok(())
    }

    #[test]
    fn file_rebinds_name_reads_declarations_and_imports() {
        for (source, owner_line, expected) in [
            ("impl Scale<u8> for Bar {}", 1, false),
            ("type u8 = f64;", 1, true),
            ("pub struct str;", 1, true),
            ("use aliases::u8;", 1, true),
            ("use aliases::{u8, i32};", 1, true),
            ("use aliases::f32 as u8;", 1, true),
            ("use aliases::other; fn f(x: u8) {}", 1, false),
            ("// type u8 = f64;\nfn f(x: u8) {}", 2, false),
            ("let s = \"use u8\"; fn f(x: u8) {}", 1, false),
            ("fn u8_count() {}", 1, false),
            ("use aliases::*;\nimpl Scale<u8> for Bar {}", 2, true),
            (
                "use aliases::{*, other};\nimpl Scale<u8> for Bar {}",
                2,
                true,
            ),
            (
                "use aliases::{other, inner::*};\nimpl Scale<u8> for Bar {}",
                2,
                true,
            ),
            // A glob in the owner's own module counts.
            (
                "mod m {\n    use super::*;\n    impl Scale<u8> for Bar {}\n}",
                3,
                true,
            ),
            // A glob in another module, before or after the owner, does not.
            (
                "impl Scale<u8> for Bar {}\nmod tests {\n    use super::*;\n}\n",
                1,
                false,
            ),
            (
                "mod other { use aliases::*; }\nimpl Scale<u8> for Bar {}",
                2,
                false,
            ),
            // A module opened on the owner's line may enclose it.
            (
                "mod m { use aliases::*; impl Scale<u8> for Bar {} }",
                1,
                true,
            ),
            (
                "mod tests { use super::*; }\nimpl Scale<u8> for Bar {}",
                2,
                false,
            ),
            (
                "mod tests;\nuse aliases::*;\nimpl Scale<u8> for Bar {}",
                3,
                true,
            ),
            ("fn f(x: &mut u8, y: u8) { *x = 2 * y; }", 1, false),
            ("fn f(x: &mut u8) { let a = (1, *x); }", 1, false),
        ] {
            assert_eq!(
                file_rebinds_name(source, "u8", owner_line)
                    || file_rebinds_name(source, "str", owner_line),
                expected,
                "{source}"
            );
        }
    }

    #[test]
    fn literal_kind_reads_one_whole_literal() {
        for (expr, expected) in [
            ("2", Some(LiteralKind::Integer)),
            ("1_000", Some(LiteralKind::Integer)),
            ("-3", Some(LiteralKind::Integer)),
            ("0xff", Some(LiteralKind::Integer)),
            ("0x1f32", Some(LiteralKind::Integer)),
            ("2u32", Some(LiteralKind::Exact("u32"))),
            ("2.5f32", Some(LiteralKind::Exact("f32"))),
            ("2.0", Some(LiteralKind::Float)),
            ("2.", Some(LiteralKind::Float)),
            ("1e3", Some(LiteralKind::Float)),
            ("1.5e-3", Some(LiteralKind::Float)),
            (" 2 ;", Some(LiteralKind::Integer)),
            ("\"abc\"", Some(LiteralKind::Str)),
            ("\"a\";", Some(LiteralKind::Str)),
            ("\"a\".len()", None),
            ("\"a\" .to_owned() + \"b\"", None),
            ("2.max(1)", None),
            ("2.0.max(1.0)", None),
            ("2 + x", None),
            ("1..3", None),
            ("x", None),
            ("", None),
        ] {
            let masked = mask_comments_and_strings(expr);
            assert_eq!(literal_kind(&masked, expr), expected, "{expr:?}");
        }
    }

    /// The same fixtures keep the direct call when a call passes the owner's
    /// own argument type, alone or beside a sibling call.
    #[test]
    fn a_call_typed_as_the_owner_argument_stays_a_direct_call() -> Result<(), String> {
        for (owner, body) in [
            (
                "impl From<u32> for Cents::from",
                "assert_eq!(Cents::from(2u32), Cents(200));",
            ),
            (
                "impl From<&str> for Cents::from",
                r#"assert_eq!(Cents::from("abc"), Cents(6));"#,
            ),
            (
                "impl From<u32> for Cents::from",
                r#"assert_eq!(Cents::from("abc"), Cents(6)); assert_eq!(Cents::from(2u32), Cents(200));"#,
            ),
            (
                "impl From<&str> for Cents::from",
                r#"assert_eq!(Cents::from(2u32), Cents(200)); assert_eq!(Cents::from("abc"), Cents(6));"#,
            ),
        ] {
            assert!(direct(CENTS, owner, body)?, "{owner} <- {body}");
        }
        for (owner, body) in [
            (
                "impl Scale<u8> for Bar::scale",
                "let b = Bar(2); assert_eq!(b.scale(2u8), 5);",
            ),
            (
                "impl Scale<f32> for Bar::scale",
                "let b = Bar(2); assert_eq!(b.scale(2.0f32), 6);",
            ),
        ] {
            assert!(direct(SCALE, owner, body)?, "{owner} <- {body}");
        }
        Ok(())
    }

    /// Whether this reading alone refuses the fixture's one test for the
    /// owner whose symbol id ends in `owner_suffix`.
    fn refused(production: &str, owner_suffix: &str, test_body: &str) -> Result<bool, String> {
        let index = parsed_index(production, test_body)?;
        let functions = index.functions();
        let owner = functions
            .iter()
            .find(|function| function.id.0.ends_with(owner_suffix))
            .ok_or_else(|| format!("owner `{owner_suffix}` indexed"))?;
        let tests = index.tests();
        let test = tests.iter().next().ok_or("test indexed")?;
        Ok(every_call_selects_generic_sibling(test, owner, &index))
    }

    /// An argument whose type the test does not write and an unnamed route
    /// keep the relation the caller already had: nothing here turns an
    /// unreadable call into a gap.
    #[test]
    fn an_unreadable_call_keeps_the_direct_call() -> Result<(), String> {
        for body in [
            // Inferred: an unsuffixed integer literal, bare or bound, may be
            // the owner's own `u32`.
            "assert_eq!(Cents::from(2), Cents(200));",
            "let n = 2; assert_eq!(Cents::from(n), Cents(200));",
            // A binding initialised by a call names no type.
            "let n = u32::default(); assert_eq!(Cents::from(n), Cents(0));",
            // A larger expression that only starts or ends with a literal.
            r#"assert_eq!(Cents::from("abc".len() as u32), Cents(300));"#,
            r#"assert_eq!(Cents::from(if true { "a" } else { "b" }), Cents(4));"#,
            // `into` and `?` run `from` without naming it.
            r#"let c: Cents = 2u32.into(); assert_eq!(c, Cents(200)); assert_eq!(Cents::from("abc"), Cents(6));"#,
            r#"let r: Result<u32, ()> = (|| { let v = Ok::<u32, ()>(2)?; Ok(v) })(); assert_eq!(Cents::from("abc"), Cents(6)); assert!(r.is_ok());"#,
        ] {
            assert!(
                direct(CENTS, "impl From<u32> for Cents::from", body)?,
                "{body}"
            );
        }
        // `Trait::name(value, arg)` puts the receiver first, and a binding
        // of a computed value needs inference.
        for body in [
            "let b = Bar(2); assert_eq!(Scale::scale(&b, 2.0f32), 6);",
            "let b = Bar(2); let by = 1.0 + 1.0; assert_eq!(b.scale(by), 6);",
        ] {
            assert!(
                !refused(SCALE, "impl Scale<u8> for Bar::scale", body)?,
                "{body}"
            );
        }
        Ok(())
    }

    /// A mention that is not a call in the read spelling leaves the answer to
    /// the caller: a function reference, a method spelling of an associated
    /// function, a turbofish, a nested `fn` of the name, and no mention at
    /// all. The control is the same owner with a readable sibling call.
    #[test]
    fn a_mention_that_is_not_a_readable_call_is_not_refused() -> Result<(), String> {
        let owner = "impl From<u32> for Cents::from";
        for body in [
            r#"let all: Vec<Cents> = ["a"].into_iter().map(Cents::from).collect(); assert_eq!(all.len(), 1);"#,
            r#"let all: Vec<Cents> = ["a"].into_iter().map(Cents::from).collect(); assert_eq!(Cents::from("abc"), all[0]);"#,
            r#"assert_eq!(Cents(6), "abc".from("abc"));"#,
            r#"assert_eq!(<Cents as From<&str>>::from::<>("abc"), Cents(6));"#,
            r#"fn from(v: &str) -> Cents { Cents(v.len() as u64) } assert_eq!(from("abc"), Cents(3));"#,
            "assert_eq!(Cents(6), Cents(6));",
        ] {
            assert!(!refused(CENTS, owner, body)?, "{body}");
        }
        assert!(refused(
            CENTS,
            owner,
            r#"assert_eq!(Cents::from("abc"), Cents(6));"#
        )?);
        Ok(())
    }

    /// The reading needs the trait argument to be the first parameter's type
    /// in a non-generic impl and function, a sibling whose head differs from
    /// the owner's, and a method syntax does not run unnamed.
    #[test]
    fn a_shape_whose_argument_is_not_the_selector_keeps_the_direct_call() -> Result<(), String> {
        // The trait argument is the return type; the first parameter is `u32`
        // in both impls, so the argument selects nothing.
        let returns = "pub trait Convert<T> { fn convert(&self, scale: u32) -> T; }\npub struct Bar(pub u32);\n\
            impl Convert<u32> for Bar {\n    fn convert(&self, scale: u32) -> u32 { self.0 * scale }\n}\n\
            impl Convert<f64> for Bar {\n    fn convert(&self, scale: u32) -> f64 { (self.0 * scale) as f64 }\n}\n";
        assert!(direct(
            returns,
            "impl Convert<f64> for Bar::convert",
            "let b = Bar(2); let f: f64 = b.convert(2u32); assert_eq!(f, 4.0);",
        )?);
        // `From<Foo>` beside `From<&Foo>`: the dropped reference is the only
        // difference.
        let by_ref = "pub struct Foo(pub u64);\npub struct Cents(pub u64);\n\
            impl From<Foo> for Cents {\n    fn from(v: Foo) -> Self { Cents(v.0) }\n}\n\
            impl From<&Foo> for Cents {\n    fn from(v: &Foo) -> Self { Cents(v.0 + 1) }\n}\n";
        assert!(direct(
            by_ref,
            "impl From<&Foo> for Cents::from",
            "assert_eq!(Cents::from(Foo(1)).0, 1);",
        )?);
        // A generic owner impl: `T` is not a type a call-site value names.
        let generic = "pub trait Scale<T> { fn scale(&self, by: T) -> u32; }\npub struct Bar<K>(pub K);\n\
            impl<T: Copy> Scale<T> for Bar<u8> {\n    fn scale(&self, _by: T) -> u32 { 1 }\n}\n\
            impl Scale<f32> for Bar<u16> {\n    fn scale(&self, _by: f32) -> u32 { 2 }\n}\n";
        assert!(direct(
            generic,
            "impl Scale<T> for Bar<u8>::scale",
            "let b = Bar(2u8); assert_eq!(b.scale(2.0f32), 1);",
        )?);
        // A borrowed argument can reach a reference parameter by deref
        // coercion, so it does not name the sibling.
        let coerced = "pub struct Name(pub u64);\n\
            impl From<&str> for Name {\n    fn from(v: &str) -> Self { Name(v.len() as u64) }\n}\n\
            impl From<String> for Name {\n    fn from(v: String) -> Self { Name(v.len() as u64 + 1) }\n}\n";
        assert!(direct(
            coerced,
            "impl From<&str> for Name::from",
            "let s: String = String::new(); assert_eq!(Name::from(&s).0, 0);",
        )?);
        // An operator method is left to the operator reading.
        let add = "use std::ops::Add;\n#[derive(Debug, PartialEq, Clone, Copy)]\npub struct Meters(pub u32);\n\
            impl Add<u32> for Meters {\n    type Output = Meters;\n    fn add(self, rhs: u32) -> Meters { Meters(self.0 + rhs) }\n}\n\
            impl Add<Meters> for Meters {\n    type Output = Meters;\n    fn add(self, rhs: Meters) -> Meters { Meters(self.0 + rhs.0) }\n}\n";
        assert!(direct(
            add,
            "impl Add<u32> for Meters::add",
            "let m = Meters(1); assert_eq!(m.add(Meters(2)), Meters(3));",
        )?);
        Ok(())
    }

    /// A same-named impl on another type is not a sibling: this reading has
    /// nothing to say about it.
    #[test]
    fn an_impl_of_the_trait_on_another_type_is_not_a_sibling() -> Result<(), String> {
        let production = "pub struct Cents(pub u64);\npub struct Dollars(pub u64);\n\
            impl From<&str> for Cents {\n    fn from(v: &str) -> Self { Cents(v.len() as u64) }\n}\n\
            impl From<u32> for Dollars {\n    fn from(v: u32) -> Self { Dollars(v as u64) }\n}\n";
        let mut index = parsed_index(production, "assert_eq!(Dollars::from(2u32).0, 2);")?;
        // A second file's `Cents` is another type of the same name: its
        // `From<u32>` is not a sibling of this file's `From<&str>` either.
        let other = "pub struct Cents(pub u64);\n\
            impl From<u32> for Cents {\n    fn from(v: u32) -> Self { Cents(v as u64) }\n}\n";
        let other_path = Path::new("src/other.rs");
        let other_facts = RaRustSyntaxAdapter.summarize_file(other_path, other)?;
        index.extend_functions(other_facts.functions.iter().cloned());
        index.insert_file_only(PathBuf::from(other_path), other_facts);
        let functions = index.functions();
        let owner = functions
            .iter()
            .find(|function| function.id.0.ends_with("impl From<&str> for Cents::from"))
            .ok_or("owner indexed")?;
        let indexed = functions
            .iter()
            .filter(|function| function.name == "from")
            .count();
        assert_eq!(indexed, 3, "all three `from` impls are indexed");
        let tests = index.tests();
        let test = tests.iter().next().ok_or("test indexed")?;
        assert!(!every_call_selects_generic_sibling(test, owner, &index));
        Ok(())
    }

    #[test]
    fn sole_trait_argument_reads_exactly_one_type_argument() {
        for (path, expected) in [
            ("From<u32>", Some("u32")),
            ("From< &'a str >", Some("&'a str")),
            ("convert::From<Vec<u8>>", Some("Vec<u8>")),
            ("Scale<(u8, u8)>", Some("(u8, u8)")),
            ("Render", None),
            ("Pair<u8, u16>", None),
            ("Iterator<Item = u8>", None),
            ("From<u32", None),
        ] {
            assert_eq!(sole_trait_argument(path), expected, "{path}");
        }
    }

    #[test]
    fn selecting_type_head_reads_only_a_plain_path_behind_one_reference() {
        for (ty, expected) in [
            ("u32", Some("u32")),
            ("&str", Some("str")),
            ("&'static str", Some("str")),
            ("&'a mut Meters", Some("Meters")),
            ("& mut Meters", Some("Meters")),
            ("units::Meters", Some("Meters")),
            ("Vec<u8>", None),
            ("&&str", None),
            ("(u8, u8)", None),
            ("[u8]", None),
            ("&[u8]", None),
            ("*const u8", None),
            ("dyn Shape", None),
            ("&dyn Shape", None),
            ("impl Shape", None),
            ("", None),
        ] {
            assert_eq!(selecting_type_head(ty).as_deref(), expected, "{ty}");
        }
    }

    #[test]
    fn unnamed_routes_are_read_per_method_name() {
        assert!(has_unnamed_route("from", "let c: Cents = 2u32.into();"));
        assert!(has_unnamed_route("from", "let c = parse(x)?;"));
        assert!(!has_unnamed_route("from", "let c = Cents::from(2u32);"));
        // `into` inside a longer identifier is not the conversion.
        assert!(!has_unnamed_route(
            "from",
            "let c = Cents::from(into_cents);"
        ));
        assert!(has_unnamed_route(
            "try_from",
            "let c: Cents = 2u32.try_into().unwrap();"
        ));
        assert!(!has_unnamed_route(
            "try_from",
            "let c = Cents::try_from(2u32);"
        ));
        // A method syntax runs unnamed is never read here.
        assert!(has_unnamed_route("add", "let m = a.add(b);"));
        assert!(!has_unnamed_route("scale", "let m = a.scale(2u8);"));
    }
}
