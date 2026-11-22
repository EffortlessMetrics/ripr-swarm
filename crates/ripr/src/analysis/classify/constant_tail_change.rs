//! #14852: a returned constant replaced by a different constant.
//!
//! cargo-mutants' most common mutant (FnValue) replaces a function body with
//! a constant (`true` → `false`, `vec![1, 2]` → `vec![]`). When the owner's
//! whole body is one constant tail on both sides of the diff, every call
//! returns the new constant instead of the old one, so the changed value is
//! the owner's return with nothing in between, and the two values differ on
//! every input. Only that shape is admitted; whether a related test pins the
//! returned value stays reveal's question.
//!
//! The decision reads the base-side owner body reconstructed from the diff,
//! never the probe's one paired removed line (`probe.before`), which may be
//! another statement of a body the diff rewrote. Fails closed on:
//!
//! - any name, call, path, field, cast, block or unread macro in either tail
//!   (`-delta`, `a & b`, `p.x`, `flag`): a named value can agree with the
//!   constant on the inputs a test passes;
//! - a base body that is not a whole-body constant tail (an early return, a
//!   statement, a binding);
//! - a base-side item that is not verified to be the same item by the
//!   shared owner [`crate::analysis::diff::verified_base_owner`] (same
//!   qualified path and attributes, unique under it, no changed header, no
//!   changed `mod` declaration on its module chain and no changed `#[path]`
//!   declaration), so a diff that swaps two modules', impls', `cfg` twins'
//!   or `#[path]` targets' functions never pairs one with the other;
//! - a gate the shared owner reports: a `cfg` or `cfg_attr` on the
//!   function, an enclosing item, the file, or a `mod` declaration on its
//!   module chain, or any `#[path]` declaration (a gated function may be
//!   compiled out for one a test runs instead);
//! - any other difference inside the item (signature, comments,
//!   attributes), or a first or last item line the diff touched;
//! - constants that are not provably different values after folding
//!   (`false` → `!true`; an integer outside `i8` range, whose value an
//!   allowed overflow lint could wrap; a float);
//! - a `vec!` (or standard assertion macro) the new-side workspace may
//!   rebind, or that the base side of any changed Rust file may rebind,
//!   including a removed `#[macro_use]`, `macro_rules`, `extern crate` or
//!   `use` line ([`base_side_may_rebind_trusted_macros`]). The diff parser
//!   registers no section for a deleted file, so a deleted file is seen
//!   only through such a removed line in a surviving file.
//!
//! #14938: the same folder also answers the opposite question for probe
//! seeding, [`is_equal_constant_respelling`]: a returned constant rewritten
//! as the same value (`65` → `0x41`, `"A"` → `"\x41"`, `1.0` → `1e0`) with
//! every numeric literal suffix kept. Floats fold (as exact decimals) only
//! for that equality; a float tail is never an admitted change.

use crate::analysis::diff::{ChangedFile, ModuleDeclarations};
use crate::analysis::facts::FunctionSummary;
use crate::analysis::rust_index::{RustIndex, find_file_facts};
use crate::domain::{FlowSinkFact, FlowSinkKind, Probe, ProbeFamily};
use ra_ap_syntax::{AstNode, SyntaxKind, ast, ast::HasArgList};

/// Owners longer than this are not read.
const MAX_OWNER_LINES: usize = 64;
/// Collection literals with more elements than this are not folded.
const MAX_ELEMENTS: usize = 256;
/// Expression nesting deeper than this is not folded.
const MAX_DEPTH: usize = 16;
const INTEGER_SUFFIXES: [&str; 12] = [
    "i8", "i16", "i32", "i64", "i128", "isize", "u8", "u16", "u32", "u64", "u128", "usize",
];

/// A whole-body constant tail the diff replaced with a different constant.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::analysis) struct ConstantTailChange {
    /// New-side line of the changed tail (the probe's line).
    pub(in crate::analysis) line: usize,
    /// The tail's raw source text, never a masked copy.
    pub(in crate::analysis) text: String,
}

impl ConstantTailChange {
    /// The owner's returned-value sink this change reaches.
    pub(in crate::analysis) fn return_sink(&self, owner: &FunctionSummary) -> FlowSinkFact {
        FlowSinkFact {
            kind: FlowSinkKind::ReturnValue,
            text: self.text.clone(),
            line: self.line,
            owner: Some(owner.id.clone()),
        }
    }
}

/// The admitted change for `probe`, read from the owner's new-side item and
/// its base-side counterpart in `changed`. `base_may_rebind_vec` is the
/// diff-wide answer of [`base_side_may_rebind_trusted_macros`]. `None`
/// whenever any condition in the module documentation fails.
pub(in crate::analysis) fn constant_tail_change(
    probe: &Probe,
    owner: &FunctionSummary,
    changed: &ChangedFile,
    index: &RustIndex,
    modules: &ModuleDeclarations,
    base_may_rebind_vec: bool,
) -> Option<ConstantTailChange> {
    if probe.family != ProbeFamily::ReturnValue
        || probe.owner.as_ref() != Some(&owner.id)
        || !probe.location.file.ends_with(&owner.file)
        || owner.end_line < owner.start_line
        || owner.end_line - owner.start_line >= MAX_OWNER_LINES
    {
        return None;
    }
    let facts = find_file_facts(index, &owner.file)?;
    let new_source: &str = facts.source.as_ref();
    // The cheap reading of the owner's own lines runs first; most probes
    // stop here, before any whole-file parse.
    let owner_text = new_source
        .lines()
        .collect::<Vec<_>>()
        .get(owner.start_line.checked_sub(1)?..owner.end_line)?
        .join("\n");
    whole_body_constant_tail(&owner_text)?;

    // Identity of the base-side item is the shared owner's question; this
    // claim reads only the two item texts it verified.
    let base = crate::analysis::diff::verified_base_owner(
        new_source,
        &owner.file,
        changed,
        modules,
        &owner.name,
        owner.start_line,
        owner.end_line,
    )?;
    // A `cfg` or `cfg_attr` on the item, an enclosing item or the file may
    // compile this function out in favour of one a test runs instead.
    if base.cfg_gated {
        return None;
    }
    let new_tail = whole_body_constant_tail(&base.new_text)?;
    let old_tail = whole_body_constant_tail(&base.old_text)?;
    let tail_line =
        base.new_first_line + base.new_text.get(..new_tail.start)?.matches('\n').count();
    if tail_line != probe.location.line || compact(&new_tail.text) != compact(&probe.expression) {
        return None;
    }
    // Only the tail differs inside the item: signature, attributes, comments
    // and closing text are the same on both sides.
    if compact(base.old_text.get(..old_tail.start)?)
        != compact(base.new_text.get(..new_tail.start)?)
        || compact(base.old_text.get(old_tail.end..)?)
            != compact(base.new_text.get(new_tail.end..)?)
    {
        return None;
    }
    // A float anywhere refuses, as before #14938 taught the folder floats
    // for equality only: a float tail is never admitted as a changed value.
    if old_tail.value.holds_float()
        || new_tail.value.holds_float()
        || !distinct(&old_tail.value, &new_tail.value)
    {
        return None;
    }
    if (old_tail.uses_vec || new_tail.uses_vec)
        && (base_may_rebind_vec || super::activation::macro_names_rebound(index, &TRUSTED_MACROS))
    {
        return None;
    }
    Some(ConstantTailChange {
        line: tail_line,
        text: new_tail.text,
    })
}

/// #14936: a deleted early-return guard whose returned constant is the
/// value the candidate owner returns on every input.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::analysis) struct DeletedGuardAgreement {
    /// Raw text of the candidate owner's whole-body constant tail.
    pub(in crate::analysis) tail: String,
}

impl DeletedGuardAgreement {
    /// The discriminate-stage summary that replaces a credited oracle.
    pub(in crate::analysis) fn summary(&self) -> String {
        format!(
            "Discriminator unconfirmed: the deleted early return gives the same value as the remaining tail `{}`, so a test whose input took the return sees the same value on both sides (deleted_guard_same_value)",
            self.tail
        )
    }
}

/// #14936: whether the removed-only `return_value` probe `probe` is a
/// deleted `if <cond> { return <constant>; }` guard whose constant equals
/// what the candidate owner now returns for every input.
///
/// On the base side an input that takes the guard returns the guard's
/// constant; on the candidate side every input returns the owner's
/// whole-body constant tail. When the two fold to the same value, no
/// assertion on the owner's return can tell the guard's deletion apart on the
/// inputs it handled, so an exact pin is no discriminator for it.
///
/// This only ever withholds credit, so it reads more loosely than the
/// admission in [`constant_tail_change`]: `Vec::new()` and `String::new()`
/// read as empty values, and a possibly rebound `vec!` is not refused. It
/// fails closed (keeps today's reading) on everything else:
///
/// - a probe that is not removed-only, or whose owner or file differ;
/// - a candidate body that is not one constant tail (any statement, early
///   return, binding or name);
/// - a base-side item the shared owner
///   [`crate::analysis::diff::verified_base_owner`] does not verify, or one
///   it reports as gated;
/// - a deleted line that is not the base body's first statement, not a
///   guard with no `else` whose block is exactly one `return <constant>`,
///   or not the removed line the probe stands on;
/// - constants that do not fold to the same value.
pub(in crate::analysis) fn deleted_guard_agreement(
    probe: &Probe,
    owner: &FunctionSummary,
    changed: &ChangedFile,
    index: &RustIndex,
    modules: &ModuleDeclarations,
) -> Option<DeletedGuardAgreement> {
    if probe.family != ProbeFamily::ReturnValue
        || probe.after.is_some()
        || probe.owner.as_ref() != Some(&owner.id)
        || !probe.location.file.ends_with(&owner.file)
        || owner.end_line < owner.start_line
        || owner.end_line - owner.start_line >= MAX_OWNER_LINES
    {
        return None;
    }
    let before = probe.before.as_deref()?;
    if !before.contains("return") {
        return None;
    }
    let facts = find_file_facts(index, &owner.file)?;
    let new_source: &str = facts.source.as_ref();
    let base = crate::analysis::diff::verified_base_owner(
        new_source,
        &owner.file,
        changed,
        modules,
        &owner.name,
        owner.start_line,
        owner.end_line,
    )?;
    if base.cfg_gated {
        return None;
    }
    let (tail_value, tail_text) = whole_body_agreement_tail(&base.new_text)?;
    let (guard_value, guard_text, guard_offset) = first_statement_return_guard(&base.old_text)?;
    let guard_line = base.old_first_line + base.old_text.get(..guard_offset)?.matches('\n').count();
    // The guard is exactly the removed line this probe stands on.
    // Compare trimmed text, not whitespace-collapsed text: a collapse would
    // also merge whitespace inside string literals, so a second deleted guard
    // returning `"a  b"` could borrow the first guard's `"a b"` agreement.
    if guard_text.trim() != before.trim()
        || !changed.removed_lines.iter().any(|line| {
            line.line == guard_line
                && line.new_side_line == probe.location.line
                && line.text.trim() == before.trim()
        })
    {
        return None;
    }
    (guard_value == tail_value).then_some(DeletedGuardAgreement { tail: tail_text })
}

/// The folded value and raw text of the tail of the one function in `text`
/// when its body is that tail alone, with no attribute anywhere in it.
fn whole_body_agreement_tail(text: &str) -> Option<(Const, String)> {
    let parse = crate::analysis::syntax::parse_clean_source_file(text)?;
    let mut items = parse.tree().syntax().children();
    let function = ast::Fn::cast(items.next()?)?;
    if items.next().is_some() {
        return None;
    }
    let list = function.body()?.stmt_list()?;
    if list.statements().next().is_some()
        || list
            .syntax()
            .descendants_with_tokens()
            .any(|element| element.kind() == SyntaxKind::POUND)
    {
        return None;
    }
    let tail = list.tail_expr()?;
    Some((
        fold_agreement_value(&tail)?,
        tail.syntax().text().to_string(),
    ))
}

/// The folded returned constant, raw text and byte offset of the first
/// statement of the one function in `text` when that statement is
/// `if <cond> { return <constant>; }` with no `else`.
fn first_statement_return_guard(text: &str) -> Option<(Const, String, usize)> {
    let parse = crate::analysis::syntax::parse_clean_source_file(text)?;
    let mut items = parse.tree().syntax().children();
    let function = ast::Fn::cast(items.next()?)?;
    if items.next().is_some() {
        return None;
    }
    let list = function.body()?.stmt_list()?;
    let first = list.statements().next()?;
    let ast::Stmt::ExprStmt(statement) = first else {
        return None;
    };
    let ast::Expr::IfExpr(guard) = statement.expr()? else {
        return None;
    };
    if guard.else_branch().is_some() {
        return None;
    }
    let block = guard.then_branch()?.stmt_list()?;
    let mut statements = block.statements();
    let returned = match (statements.next(), block.tail_expr()) {
        (Some(ast::Stmt::ExprStmt(only)), None) if statements.next().is_none() => only.expr()?,
        (None, Some(tail)) => tail,
        _ => return None,
    };
    let ast::Expr::ReturnExpr(returned) = returned else {
        return None;
    };
    let value = fold_agreement_value(&returned.expr()?)?;
    let range = statement.syntax().text_range();
    Some((
        value,
        statement.syntax().text().to_string(),
        usize::from(range.start()),
    ))
}

/// [`fold`], plus a bare `Vec::new()` or `String::new()` read as the empty
/// value. Used only where a match withholds credit.
fn fold_agreement_value(expr: &ast::Expr) -> Option<Const> {
    if let ast::Expr::CallExpr(call) = expr
        && call.arg_list()?.args().next().is_none()
        && let Some(ast::Expr::PathExpr(path)) = call.expr()
    {
        return match compact(&path.syntax().text().to_string()).as_str() {
            "Vec::new" => Some(Const::List {
                vec: true,
                elements: Vec::new(),
            }),
            "String::new" => Some(Const::Str(String::new())),
            _ => None,
        };
    }
    let mut uses_vec = false;
    fold(expr, 0, &mut uses_vec)
}

/// `vec` plus the standard assertion macros. The binding scan reads the
/// arguments of every macro it does not trust as a possible definition site,
/// so trusting only `vec` would refuse every `assert_eq!(f(), vec![..])`;
/// trusting the assertion macros also requires that none of them is rebound.
const TRUSTED_MACROS: [&str; 7] = [
    "vec",
    "assert",
    "assert_eq",
    "assert_ne",
    "debug_assert",
    "debug_assert_eq",
    "debug_assert_ne",
];

/// Whether the base side of the diff may bind `vec` (or a standard assertion
/// macro) to something other than the standard one. The new side is the
/// index's question ([`super::activation::macro_names_rebound`]); a binding
/// the diff removed lives only in the base side of a changed file, possibly
/// a deleted file or another crate. Computed once per run over every changed
/// file. Any removed line naming `macro_use`, `macro_rules`, `extern crate`
/// or `no_implicit_prelude`, any changed Rust file whose base side cannot be
/// reconstructed, and any binding site in a reconstructed base side answer
/// yes.
pub(in crate::analysis) fn base_side_may_rebind_trusted_macros(
    changed_files: &[ChangedFile],
    index: &RustIndex,
) -> bool {
    changed_files.iter().any(|changed| {
        let is_rust = changed
            .path
            .extension()
            .is_some_and(|extension| extension == "rs");
        if !is_rust {
            return false;
        }
        if changed.removed_lines.iter().any(|line| {
            [
                "macro_use",
                "macro_rules",
                "extern crate",
                "no_implicit_prelude",
            ]
            .iter()
            .any(|marker| line.text.contains(marker))
        }) {
            return true;
        }
        if changed.removed_lines.is_empty() {
            return false;
        }
        let base =
            if changed.added_lines.is_empty() && find_file_facts(index, &changed.path).is_none() {
                // A file the index does not hold and the diff only removes from
                // (a deleted file, where a diff source registers one): its base
                // side is exactly its removed lines.
                Some(
                    changed
                        .removed_lines
                        .iter()
                        .map(|line| line.text.as_str())
                        .collect::<Vec<_>>()
                        .join("\n"),
                )
            } else {
                find_file_facts(index, &changed.path).and_then(|facts| {
                    crate::analysis::diff::reconstruct_old_source(&facts.source, changed)
                })
            };
        let Some(base) = base else {
            return true;
        };
        !crate::analysis::syntax::trusted_macro_binding_sites(
            &base,
            index.macro_scope_crates(),
            &TRUSTED_MACROS,
            &|_, _| false,
            &|_| false,
        )
        .is_empty()
    })
}

fn compact(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// One folded constant. Integers keep their mathematical value; whether two
/// of them are different typed values is [`distinct`]'s question.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Const {
    Bool(bool),
    Int(i128),
    Byte(u8),
    Char(char),
    Str(String),
    /// A float literal as its exact decimal value, `digits × 10^exponent`
    /// with no leading or trailing zero digit (zero is empty digits), so two
    /// spellings fold equal only when they are the same rational number and
    /// therefore round alike at every width, `f16` and `f128` included. Two
    /// floats are never [`distinct`]: only equality is read from them.
    Float {
        digits: String,
        exponent: i64,
    },
    /// Elements of an array (`vec == false`) or `vec!` (`vec == true`).
    List {
        vec: bool,
        elements: Vec<Const>,
    },
}

impl Const {
    fn holds_float(&self) -> bool {
        match self {
            Const::Float { .. } => true,
            Const::List { elements, .. } => elements.iter().any(Const::holds_float),
            _ => false,
        }
    }
}

struct ConstantTail {
    /// Byte range of the tail inside the owner text.
    start: usize,
    end: usize,
    /// Raw tail text.
    text: String,
    value: Const,
    uses_vec: bool,
}

/// The tail of `text` when `text` is exactly one function whose body is one
/// tail expression that folds to a constant. Offsets are into `text`.
fn whole_body_constant_tail(text: &str) -> Option<ConstantTail> {
    let parse = crate::analysis::syntax::parse_clean_source_file(text)?;
    let mut items = parse.tree().syntax().children();
    let function = ast::Fn::cast(items.next()?)?;
    if items.next().is_some() {
        return None;
    }
    constant_tail_of(&function)
}

/// The tail of `function` when its body is one tail expression that folds to
/// a constant. Offsets are into the parsed text.
fn constant_tail_of(function: &ast::Fn) -> Option<ConstantTail> {
    let list = function.body()?.stmt_list()?;
    if list.statements().next().is_some() {
        return None;
    }
    // An inner attribute in the body (`#![cfg(any())]`) applies to the
    // function itself, and the shared gate check reads only outer ones, so
    // any attribute in the body refuses (fail closed).
    if list
        .syntax()
        .descendants_with_tokens()
        .any(|element| element.kind() == SyntaxKind::POUND)
    {
        return None;
    }
    let tail = list.tail_expr()?;
    let mut uses_vec = false;
    let value = fold(&tail, 0, &mut uses_vec)?;
    let range = tail.syntax().text_range();
    Some(ConstantTail {
        start: usize::from(range.start()),
        end: usize::from(range.end()),
        text: tail.syntax().text().to_string(),
        value,
        uses_vec,
    })
}

fn fold(expr: &ast::Expr, depth: usize, uses_vec: &mut bool) -> Option<Const> {
    if depth > MAX_DEPTH {
        return None;
    }
    // An attribute anywhere in the constant, such as `#[cfg(any())]` on a
    // collection element, can remove what the syntax shows, so the folded
    // value would not be the returned value (#14953 review).
    if expr
        .syntax()
        .descendants_with_tokens()
        .any(|element| element.kind() == SyntaxKind::POUND)
    {
        return None;
    }
    let next = depth + 1;
    match expr {
        ast::Expr::Literal(literal) => fold_literal(literal),
        ast::Expr::ParenExpr(inner) => fold(&inner.expr()?, next, uses_vec),
        ast::Expr::PrefixExpr(prefix) => {
            let operand = fold(&prefix.expr()?, next, uses_vec)?;
            match (prefix.op_kind()?, operand) {
                (ast::UnaryOp::Not, Const::Bool(value)) => Some(Const::Bool(!value)),
                (ast::UnaryOp::Neg, Const::Int(value)) => Some(Const::Int(value.checked_neg()?)),
                _ => None,
            }
        }
        ast::Expr::BinExpr(binary) => {
            let lhs = fold(&binary.lhs()?, next, uses_vec)?;
            let rhs = fold(&binary.rhs()?, next, uses_vec)?;
            fold_binary(binary.op_kind()?, &lhs, &rhs)
        }
        ast::Expr::ArrayExpr(array) => fold_array(array, false, next, uses_vec),
        ast::Expr::MacroExpr(macro_expr) => {
            let call = macro_expr.macro_call()?;
            if call.excl_token().is_none() || call.path()?.syntax().text() != "vec" {
                return None;
            }
            let tree = call.token_tree()?;
            let inner = tree.syntax().text().to_string();
            let inner = inner.get(1..inner.len().checked_sub(1)?)?;
            let parsed = crate::analysis::syntax::parse_clean_source_file(&format!(
                "fn ripr_vec() {{ let _ = [{inner}]; }}"
            ))?;
            let array = parsed
                .tree()
                .syntax()
                .descendants()
                .find_map(ast::LetStmt::cast)?
                .initializer()?;
            let ast::Expr::ArrayExpr(array) = array else {
                return None;
            };
            *uses_vec = true;
            fold_array(&array, true, next, uses_vec)
        }
        _ => None,
    }
}

fn fold_literal(literal: &ast::Literal) -> Option<Const> {
    match literal.kind() {
        ast::LiteralKind::Bool(value) => Some(Const::Bool(value)),
        ast::LiteralKind::IntNumber(number) => {
            // `1f32` lexes as an integer literal with a float suffix.
            if number
                .suffix()
                .is_some_and(|suffix| !INTEGER_SUFFIXES.contains(&suffix))
            {
                return None;
            }
            Some(Const::Int(i128::try_from(number.value().ok()?).ok()?))
        }
        ast::LiteralKind::Byte(byte) => Some(Const::Byte(byte.value().ok()?)),
        ast::LiteralKind::Char(character) => Some(Const::Char(character.value().ok()?)),
        ast::LiteralKind::String(text) => Some(Const::Str(text.value().ok()?.into_owned())),
        ast::LiteralKind::FloatNumber(number) => {
            let text = ast::AstToken::text(&number);
            let digits = text
                .strip_suffix(number.suffix().unwrap_or(""))?
                .replace('_', "");
            exact_decimal(&digits)
        }
        _ => None,
    }
}

/// A float literal's text (`1.5`, `15e-1`, `1.`, `2E+3`, no `_` or suffix)
/// as its exact decimal [`Const::Float`]; `None` for any other shape.
fn exact_decimal(text: &str) -> Option<Const> {
    let (mantissa, exponent) = match text.find(['e', 'E']) {
        Some(at) => (&text[..at], text.get(at + 1..)?),
        None => (text, "0"),
    };
    let exponent_digits = exponent.strip_prefix(['+', '-']).unwrap_or(exponent);
    if exponent_digits.is_empty()
        || exponent_digits.len() > 9
        || !exponent_digits.bytes().all(|byte| byte.is_ascii_digit())
    {
        return None;
    }
    let (whole, fraction) = mantissa.split_once('.').unwrap_or((mantissa, ""));
    if whole.is_empty()
        || !format!("{whole}{fraction}")
            .bytes()
            .all(|byte| byte.is_ascii_digit())
    {
        return None;
    }
    let mut exponent = exponent.parse::<i64>().ok()? - i64::try_from(fraction.len()).ok()?;
    let digits = format!("{whole}{fraction}");
    let digits = digits.trim_start_matches('0');
    let trimmed = digits.trim_end_matches('0');
    exponent += i64::try_from(digits.len() - trimmed.len()).ok()?;
    if trimmed.is_empty() {
        exponent = 0;
    }
    Some(Const::Float {
        digits: trimmed.to_string(),
        exponent,
    })
}

/// #14938: whether `new` respells the constant expression `old` as the same
/// value (`65` as `0x41`, `"A"` as `"\x41"` or `r#"A"#`, `'A'` as
/// `'\u{41}'`, `1.0` as `1e0`), so no test can tell the two apart. Both
/// sides must fold with the #14852 folder, and every numeric literal keeps
/// its suffix in order: `1` to `1i8` can change the inferred type. A `vec!`,
/// an attribute or anything the folder refuses answers `false`.
pub(in crate::analysis) fn is_equal_constant_respelling(old: &str, new: &str) -> bool {
    let (old, new) = (old.trim(), new.trim());
    if old == new {
        return false;
    }
    let read = |text: &str| {
        let tail = whole_body_constant_tail(&format!("fn ripr_respelling() {{ {text} }}"))?;
        let parse = crate::analysis::syntax::parse_clean_source_file(&format!(
            "fn ripr_suffixes() {{ {text} }}"
        ))?;
        // The folder negates in `i128`, so `-(-128i8)` folds to 128 although
        // it overflows `i8`. Equality reads a minus sign only directly on a
        // literal, where both spellings negate the same typed literal.
        let negates_non_literal = parse
            .tree()
            .syntax()
            .descendants()
            .filter_map(ast::PrefixExpr::cast)
            .any(|prefix| {
                prefix.op_kind() == Some(ast::UnaryOp::Neg)
                    && !matches!(prefix.expr(), Some(ast::Expr::Literal(_)))
            });
        if negates_non_literal {
            return None;
        }
        let suffixes = parse
            .tree()
            .syntax()
            .descendants()
            .filter_map(ast::Literal::cast)
            .map(|literal| match literal.kind() {
                ast::LiteralKind::IntNumber(number) => {
                    Some(number.suffix().unwrap_or("").to_string())
                }
                ast::LiteralKind::FloatNumber(number) => {
                    Some(format!("float:{}", number.suffix().unwrap_or("")))
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        (!tail.uses_vec).then_some((tail.value, suffixes))
    };
    match (read(old), read(new)) {
        (Some(old), Some(new)) => old == new,
        _ => false,
    }
}

fn fold_binary(op: ast::BinaryOp, lhs: &Const, rhs: &Const) -> Option<Const> {
    use ast::{ArithOp, BinaryOp, CmpOp, LogicOp};
    match (op, lhs, rhs) {
        (BinaryOp::LogicOp(LogicOp::And), Const::Bool(a), Const::Bool(b)) => {
            Some(Const::Bool(*a && *b))
        }
        (BinaryOp::LogicOp(LogicOp::Or), Const::Bool(a), Const::Bool(b)) => {
            Some(Const::Bool(*a || *b))
        }
        (BinaryOp::ArithOp(ArithOp::BitAnd), Const::Bool(a), Const::Bool(b)) => {
            Some(Const::Bool(*a & *b))
        }
        (BinaryOp::ArithOp(ArithOp::BitOr), Const::Bool(a), Const::Bool(b)) => {
            Some(Const::Bool(*a | *b))
        }
        (BinaryOp::ArithOp(ArithOp::BitXor), Const::Bool(a), Const::Bool(b)) => {
            Some(Const::Bool(*a ^ *b))
        }
        (BinaryOp::CmpOp(CmpOp::Eq { negated }), a, b) => {
            let equal = match (a, b) {
                (Const::Bool(a), Const::Bool(b)) => a == b,
                (Const::Int(a), Const::Int(b))
                    if fits_every_signed(*a) && fits_every_signed(*b) =>
                {
                    a == b
                }
                _ => return None,
            };
            Some(Const::Bool(equal != negated))
        }
        _ => None,
    }
}

fn fold_array(
    array: &ast::ArrayExpr,
    vec: bool,
    depth: usize,
    uses_vec: &mut bool,
) -> Option<Const> {
    if array.semicolon_token().is_some() {
        return None;
    }
    let mut elements = Vec::new();
    for element in array.exprs() {
        if elements.len() == MAX_ELEMENTS {
            return None;
        }
        elements.push(fold(&element, depth, uses_vec)?);
    }
    Some(Const::List { vec, elements })
}

/// Whether `value` is the same typed value in every integer type that can
/// hold it: `i8::MIN..=i8::MAX`. A negative literal does not compile for an
/// unsigned type, and a larger one could wrap under an allowed
/// `overflowing_literals` lint.
fn fits_every_signed(value: i128) -> bool {
    (i128::from(i8::MIN)..=i128::from(i8::MAX)).contains(&value)
}

/// Whether two folded constants of one owner's return type are provably
/// different values. Anything not provable answers `false`.
fn distinct(old: &Const, new: &Const) -> bool {
    match (old, new) {
        (Const::Bool(a), Const::Bool(b)) => a != b,
        (Const::Int(a), Const::Int(b)) => fits_every_signed(*a) && fits_every_signed(*b) && a != b,
        (Const::Byte(a), Const::Byte(b)) => a != b,
        (Const::Char(a), Const::Char(b)) => a != b,
        (Const::Str(a), Const::Str(b)) => a != b,
        (
            Const::List {
                vec: old_vec,
                elements: old_elements,
            },
            Const::List {
                vec: new_vec,
                elements: new_elements,
            },
        ) => {
            old_vec == new_vec
                && (old_elements.len() != new_elements.len()
                    || old_elements
                        .iter()
                        .zip(new_elements)
                        .any(|(old, new)| distinct(old, new)))
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::rust_index::RaRustSyntaxAdapter;
    use crate::analysis::rust_index::RustSyntaxAdapter;
    use crate::domain::{DeltaKind, ProbeId, SourceLocation};
    use std::path::{Path, PathBuf};

    /// One-hunk unified diff of `old` → `new` (common prefix and suffix
    /// lines kept as context, the middle replaced).
    fn unified_diff_at(path: &str, old: &str, new: &str) -> String {
        let old_lines: Vec<&str> = old.lines().collect();
        let new_lines: Vec<&str> = new.lines().collect();
        let prefix = old_lines
            .iter()
            .zip(&new_lines)
            .take_while(|(a, b)| a == b)
            .count();
        let suffix = old_lines[prefix..]
            .iter()
            .rev()
            .zip(new_lines[prefix..].iter().rev())
            .take_while(|(a, b)| a == b)
            .count();
        let removed = &old_lines[prefix..old_lines.len() - suffix];
        let added = &new_lines[prefix..new_lines.len() - suffix];
        let mut diff = format!(
            "--- a/{path}\n+++ b/{path}\n@@ -{},{} +{},{} @@\n",
            prefix + 1,
            old_lines.len() - prefix,
            prefix + 1,
            new_lines.len() - prefix
        );
        for line in removed {
            diff.push_str(&format!("-{line}\n"));
        }
        for line in added {
            diff.push_str(&format!("+{line}\n"));
        }
        for line in &new_lines[new_lines.len() - suffix..] {
            diff.push_str(&format!(" {line}\n"));
        }
        diff
    }

    /// The change read for the `return_value` probe on the new-side line
    /// holding `changed` inside owner `owner`, with the diff `old` → `new`.
    fn read(
        old: &str,
        new: &str,
        owner: &str,
        changed: &str,
    ) -> Result<Option<ConstantTailChange>, String> {
        read_family(old, new, owner, changed, ProbeFamily::ReturnValue)
    }

    fn read_family(
        old: &str,
        new: &str,
        owner: &str,
        changed: &str,
        family: ProbeFamily,
    ) -> Result<Option<ConstantTailChange>, String> {
        read_with(old, new, owner, changed, family, false)
    }

    fn read_with(
        old: &str,
        new: &str,
        owner: &str,
        changed: &str,
        family: ProbeFamily,
        base_may_rebind_vec: bool,
    ) -> Result<Option<ConstantTailChange>, String> {
        read_in(
            ("src/lib.rs", old, new),
            &[],
            owner,
            changed,
            family,
            base_may_rebind_vec,
        )
    }

    /// [`read_with`] for an owner file `path` beside `others`, each
    /// `(path, old, new)`: every file is indexed, every file whose sides
    /// differ is changed, and the module declarations are read from both as
    /// in production.
    fn read_in(
        (path, old, new): (&str, &str, &str),
        others: &[(&str, &str, &str)],
        owner: &str,
        changed: &str,
        family: ProbeFamily,
        base_may_rebind_vec: bool,
    ) -> Result<Option<ConstantTailChange>, String> {
        let facts = RaRustSyntaxAdapter.summarize_file(Path::new(path), new)?;
        let line = new
            .lines()
            .position(|line| line.trim() == changed)
            .ok_or("fixture premise: changed line missing")?
            + 1;
        let function = facts
            .functions
            .iter()
            .find(|function| {
                function.name == owner && (function.start_line..=function.end_line).contains(&line)
            })
            .cloned()
            .ok_or("fixture premise: owner not indexed")?;
        let mut index = RustIndex::default();
        index.insert_file(PathBuf::from(path), facts, true);
        let changed_file =
            crate::analysis::diff::parse_unified_diff(&unified_diff_at(path, old, new))
                .into_iter()
                .next()
                .ok_or("fixture premise: diff did not parse")?;
        let mut other_changes = Vec::new();
        for (other, other_old, other_new) in others {
            let other_facts = RaRustSyntaxAdapter.summarize_file(Path::new(other), other_new)?;
            index.insert_file(PathBuf::from(other), other_facts, true);
            if other_old != other_new {
                other_changes.extend(crate::analysis::diff::parse_unified_diff(&unified_diff_at(
                    other, other_old, other_new,
                )));
            }
        }
        let all_changes: Vec<ChangedFile> = std::iter::once(changed_file.clone())
            .chain(other_changes)
            .collect();
        let modules = ModuleDeclarations::read(
            std::iter::once((Path::new(path), new)).chain(
                others
                    .iter()
                    .map(|(other, _, other_new)| (Path::new(*other), *other_new)),
            ),
            all_changes.iter().map(|file| {
                let source =
                    find_file_facts(&index, &file.path).map(|facts| facts.data().source.as_ref());
                (file, source)
            }),
        );
        if changed_file.added_lines.is_empty() {
            return Err("fixture premise: diff adds nothing".into());
        }
        let probe = Probe {
            before_condition: None,
            id: ProbeId("probe:src_lib_rs:tail".into()),
            location: SourceLocation::new(path, line, 5),
            owner: Some(function.id.clone()),
            family,
            delta: DeltaKind::Value,
            before: None,
            after: Some(changed.into()),
            expression: changed.into(),
            expected_sinks: Vec::new(),
            required_oracles: Vec::new(),
        };
        Ok(constant_tail_change(
            &probe,
            &function,
            &changed_file,
            &index,
            &modules,
            // As in production: the diff-wide base-side scan, here over the
            // changed files.
            base_may_rebind_vec || base_side_may_rebind_trusted_macros(&all_changes, &index),
        ))
    }

    #[test]
    fn bool_constant_replaced_by_the_other_bool_is_admitted() -> Result<(), String> {
        let old = "pub fn ready() -> bool {\n    true\n}\n";
        let new = "pub fn ready() -> bool {\n    false\n}\n";
        let change = read(old, new, "ready", "false")?.ok_or("true -> false refused")?;
        assert_eq!(change.line, 2);
        assert_eq!(change.text, "false");
        Ok(())
    }

    #[test]
    fn emptied_vec_constant_is_admitted() -> Result<(), String> {
        let old = "pub fn items() -> Vec<i32> {\n    vec![1, 2]\n}\n";
        let new = "pub fn items() -> Vec<i32> {\n    vec![]\n}\n";
        let change = read(old, new, "items", "vec![]")?.ok_or("vec emptied refused")?;
        assert_eq!(change.text, "vec![]");
        // Lengths differ, so element width never decides it.
        let old = "pub fn items() -> Vec<i32> {\n    vec![300]\n}\n";
        assert!(read(old, new, "items", "vec![]")?.is_some());
        Ok(())
    }

    /// The sink text is the raw tail: a masked copy would blank the byte
    /// literals (`vec![b   , b   ]`).
    #[test]
    fn char_literal_tail_keeps_its_raw_text() -> Result<(), String> {
        let old = "pub fn bytes() -> Vec<u8> {\n    vec![b'a', b'b']\n}\n";
        let new = "pub fn bytes() -> Vec<u8> {\n    vec![b'x', b'y']\n}\n";
        let change = read(old, new, "bytes", "vec![b'x', b'y']")?.ok_or("bytes refused")?;
        assert_eq!(change.text, "vec![b'x', b'y']");
        let old = "pub fn letter() -> char {\n    'a'\n}\n";
        let new = "pub fn letter() -> char {\n    'b'\n}\n";
        let change = read(old, new, "letter", "'b'")?.ok_or("char refused")?;
        assert_eq!(change.text, "'b'");
        Ok(())
    }

    #[test]
    fn a_name_in_the_new_tail_is_refused() -> Result<(), String> {
        for (old, new, owner, changed) in [
            (
                "pub fn neg(delta: i32) -> i32 {\n    0\n}\n",
                "pub fn neg(delta: i32) -> i32 {\n    -delta\n}\n",
                "neg",
                "-delta",
            ),
            (
                "pub fn both(a: bool, b: bool) -> bool {\n    false\n}\n",
                "pub fn both(a: bool, b: bool) -> bool {\n    a & b\n}\n",
                "both",
                "a & b",
            ),
            (
                "pub struct P { pub x: i32 }\npub fn px(p: P) -> i32 {\n    0\n}\n",
                "pub struct P { pub x: i32 }\npub fn px(p: P) -> i32 {\n    p.x\n}\n",
                "px",
                "p.x",
            ),
            (
                "const ON: bool = true;\npub fn on() -> bool {\n    false\n}\n",
                "const ON: bool = true;\npub fn on() -> bool {\n    ON\n}\n",
                "on",
                "ON",
            ),
        ] {
            assert_eq!(read(old, new, owner, changed)?, None, "{changed}");
        }
        Ok(())
    }

    #[test]
    fn a_name_in_the_old_tail_is_refused() -> Result<(), String> {
        let old = "pub fn enabled(flag: bool) -> bool {\n    flag\n}\n";
        let new = "pub fn enabled(flag: bool) -> bool {\n    false\n}\n";
        assert_eq!(read(old, new, "enabled", "false")?, None);
        let old = "pub fn enabled(flag: bool) -> bool {\n    !flag\n}\n";
        assert_eq!(read(old, new, "enabled", "false")?, None);
        Ok(())
    }

    /// The probe's paired removed line (`true`) is a constant, but the
    /// base-side owner body also returns early, so it is not a whole-body
    /// constant tail.
    #[test]
    fn a_base_body_that_is_not_a_constant_tail_is_refused() -> Result<(), String> {
        let old =
            "pub fn ready(flag: bool) -> bool {\n    if flag { return false; }\n    true\n}\n";
        let new = "pub fn ready(flag: bool) -> bool {\n    // c\n    false\n}\n";
        assert_eq!(read(old, new, "ready", "false")?, None);
        let old = "pub fn pick(x: i32) -> Vec<i32> {\n    if x == 0 { return Vec::new(); }\n    vec![9]\n}\n";
        let new = "pub fn pick(x: i32) -> Vec<i32> {\n    // c\n    vec![]\n}\n";
        assert_eq!(read(old, new, "pick", "vec![]")?, None);
        let old = "pub fn ready() -> bool {\n    let on = true;\n    on\n}\n";
        let new = "pub fn ready() -> bool {\n    false\n}\n";
        assert_eq!(read(old, new, "ready", "false")?, None);
        Ok(())
    }

    #[test]
    fn constants_that_fold_equal_or_unprovably_apart_are_refused() -> Result<(), String> {
        for (old_tail, new_tail) in [
            ("false", "!true"),
            ("true", "true && !false"),
            ("false", "(true == false)"),
            ("vec![true]", "vec![!false]"),
            // Outside i8 range, an allowed overflow lint could wrap either.
            ("vec![200]", "vec![300]"),
            ("vec![-1]", "vec![255]"),
            // Floats are never folded.
            ("vec![1.0]", "vec![2.0]"),
            ("vec![1f32]", "vec![2f32]"),
            // `!` on an integer depends on its width.
            ("vec![!0]", "vec![1]"),
            // Repeat form is not folded.
            ("vec![true; 2]", "vec![false; 2]"),
        ] {
            let old = format!("pub fn value() -> T {{\n    {old_tail}\n}}\n");
            let new = format!("pub fn value() -> T {{\n    {new_tail}\n}}\n");
            assert_eq!(
                read(&old, &new, "value", new_tail)?,
                None,
                "{old_tail} -> {new_tail}"
            );
        }
        let old = "pub fn value() -> Vec<i8> {\n    vec![1, -2]\n}\n";
        let new = "pub fn value() -> Vec<i8> {\n    vec![1, -3]\n}\n";
        assert!(read(old, new, "value", "vec![1, -3]")?.is_some());
        Ok(())
    }

    #[test]
    fn another_change_inside_the_owner_span_is_refused() -> Result<(), String> {
        // Header changed: the header line has no base-side counterpart.
        let old = "pub fn ready() -> bool {\n    true\n}\n";
        let new = "pub fn ready(_x: u8) -> bool {\n    false\n}\n";
        assert_eq!(read(old, new, "ready", "false")?, None);
        // A comment inside the body changed along with the tail.
        let old = "pub fn ready() -> bool {\n    // on\n    true\n}\n";
        let new = "pub fn ready() -> bool {\n    // off\n    false\n}\n";
        assert_eq!(read(old, new, "ready", "false")?, None);
        Ok(())
    }

    #[test]
    fn a_rebound_vec_macro_is_refused() -> Result<(), String> {
        let rebind = "macro_rules! vec { ($($t:tt)*) => { Vec::<i32>::new() }; }\n";
        let old = format!("{rebind}pub fn items() -> Vec<i32> {{\n    vec![1, 2]\n}}\n");
        let new = format!("{rebind}pub fn items() -> Vec<i32> {{\n    vec![]\n}}\n");
        assert_eq!(read(&old, &new, "items", "vec![]")?, None);
        // Only the base side rebinds it.
        let old = format!("{rebind}pub fn items() -> Vec<i32> {{\n    vec![1, 2]\n}}\n");
        let new = "\npub fn items() -> Vec<i32> {\n    vec![]\n}\n";
        assert_eq!(read(&old, new, "items", "vec![]")?, None);
        Ok(())
    }

    /// #14953 review: an inner `#![cfg(any())]` in the body compiles the
    /// function out, and the shared gate check reads only outer attributes.
    #[test]
    fn an_inner_attribute_in_the_body_is_refused() -> Result<(), String> {
        for inner in ["#![cfg(any())]", "#![allow(unused)]"] {
            let old = format!("pub fn ready() -> bool {{\n    {inner}\n    true\n}}\n");
            let new = format!("pub fn ready() -> bool {{\n    {inner}\n    false\n}}\n");
            assert_eq!(read(&old, &new, "ready", "false")?, None, "{inner}");
        }
        // Control: the same edit without the inner attribute is read.
        let old = "pub fn ready() -> bool {\n    true\n}\n";
        let new = "pub fn ready() -> bool {\n    false\n}\n";
        assert!(read(old, new, "ready", "false")?.is_some());
        Ok(())
    }

    /// #14953 review: `#[cfg(any())]` removes an element, so the syntax
    /// differs while the returned value does not.
    #[test]
    fn an_attribute_inside_the_constant_is_refused() -> Result<(), String> {
        for (ty, old_tail, new_tail) in [
            ("Vec<i32>", "vec![]", "vec![#[cfg(any())] 1]"),
            ("Vec<i32>", "vec![#[cfg(any())] 1]", "vec![]"),
            (
                "Vec<Vec<i32>>",
                "vec![vec![]]",
                "vec![vec![#[cfg(any())] 1]]",
            ),
            ("[i32; 1]", "[1]", "[1, #[cfg(any())] 2]"),
        ] {
            let old = format!("pub fn items() -> {ty} {{\n    {old_tail}\n}}\n");
            let new = format!("pub fn items() -> {ty} {{\n    {new_tail}\n}}\n");
            assert_eq!(
                read(&old, &new, "items", new_tail)?,
                None,
                "{old_tail} -> {new_tail}"
            );
        }
        // Control: the same edit without the attribute is read.
        let old = "pub fn items() -> Vec<i32> {\n    vec![]\n}\n";
        let new = "pub fn items() -> Vec<i32> {\n    vec![1]\n}\n";
        assert!(read(old, new, "items", "vec![1]")?.is_some());
        Ok(())
    }

    /// A `vec![..]` inside a standard assertion is not a binding site.
    #[test]
    fn a_vec_inside_a_standard_assertion_is_not_a_rebinding() -> Result<(), String> {
        let test = "#[test]\nfn pins() {\n    assert_eq!(items(), vec![]);\n}\n";
        let old = format!("pub fn items() -> Vec<i32> {{\n    vec![1, 2]\n}}\n{test}");
        let new = format!("pub fn items() -> Vec<i32> {{\n    vec![]\n}}\n{test}");
        assert!(read(&old, &new, "items", "vec![]")?.is_some());
        // An unknown wrapper's arguments may still define `vec`.
        let test = "#[test]\nfn pins() {\n    wrap!(vec![]);\n}\n";
        let old = format!("pub fn items() -> Vec<i32> {{\n    vec![1, 2]\n}}\n{test}");
        let new = format!("pub fn items() -> Vec<i32> {{\n    vec![]\n}}\n{test}");
        assert_eq!(read(&old, &new, "items", "vec![]")?, None);
        Ok(())
    }

    /// Review F2: the unchanged `pub fn ready() -> bool {` line pairs with
    /// the other module's (or impl's) function when a diff swaps them, so
    /// the base-side item must have the same enclosing items.
    #[test]
    fn a_swapped_function_of_another_scope_is_refused() -> Result<(), String> {
        let module = |name: &str, value: &str| {
            format!(
                "pub mod {name} {{\n    pub fn ready() -> bool {{\n        {value}\n    }}\n}}\n"
            )
        };
        let old = format!("{}\n{}", module("a", "true"), module("b", "false"));
        let new = format!("{}\n{}", module("b", "false"), module("a", "true"));
        let lines: Vec<&str> = new.lines().collect();
        // Premise: the changed tails sit under unchanged headers.
        assert_eq!(lines.get(1), Some(&"    pub fn ready() -> bool {"));
        for tail in ["false", "true"] {
            assert_eq!(read(&old, &new, "ready", tail)?, None, "swap_mods {tail}");
        }
        let imp = |name: &str, value: &str| {
            format!("impl {name} {{\n    pub fn ready() -> bool {{\n        {value}\n    }}\n}}\n")
        };
        let types = "pub struct A;\npub struct B;\n";
        let old = format!("{types}{}\n{}", imp("A", "true"), imp("B", "false"));
        let new = format!("{types}{}\n{}", imp("B", "false"), imp("A", "true"));
        for tail in ["false", "true"] {
            assert_eq!(read(&old, &new, "ready", tail)?, None, "swap_impls {tail}");
        }
        // Positive control: the same impl changes in place.
        let old = format!("{types}{}", imp("A", "true"));
        let new = format!("{types}{}", imp("A", "false"));
        assert!(read(&old, &new, "ready", "false")?.is_some());
        Ok(())
    }

    /// Review F2: a `cfg`-gated function, or one with a same-named twin, may
    /// not be the function a test runs.
    #[test]
    fn a_cfg_gated_or_twinned_function_is_refused() -> Result<(), String> {
        let twin = |gate_a: &str, a: &str, gate_b: &str, b: &str| {
            format!(
                "#[cfg({gate_a})]\npub fn ready() -> bool {{\n    {a}\n}}\n\n#[cfg({gate_b})]\npub fn ready() -> bool {{\n    {b}\n}}\n"
            )
        };
        // cfg_dup2: the non-test twin changes; tests run the other.
        let old = twin("not(test)", "true", "test", "false");
        let new = twin("not(test)", "false", "test", "false");
        assert_eq!(read(&old, &new, "ready", "false")?, None);
        // cfg_never2: the never-compiled twin changes.
        let old = twin("any()", "true", "not(any())", "false");
        let new = twin("any()", "false", "not(any())", "false");
        assert_eq!(read(&old, &new, "ready", "false")?, None);
        // cfg_swap: the gates swap between the twins.
        let old = twin("any()", "false", "not(any())", "true");
        let new = twin("not(any())", "true", "any()", "false");
        assert_eq!(read(&old, &new, "ready", "false")?, None);
        // A gated module.
        let old = "#[cfg(feature = \"x\")]\nmod m {\n    pub fn ready() -> bool {\n        true\n    }\n}\n";
        let new = "#[cfg(feature = \"x\")]\nmod m {\n    pub fn ready() -> bool {\n        false\n    }\n}\n";
        assert_eq!(read(old, new, "ready", "false")?, None);
        // A file-level inner gate.
        let old = "#![cfg(unix)]\npub fn ready() -> bool {\n    true\n}\n";
        let new = "#![cfg(unix)]\npub fn ready() -> bool {\n    false\n}\n";
        assert_eq!(read(old, new, "ready", "false")?, None);
        Ok(())
    }

    /// Review N1: a gate or `#[path]` swap on the parent's `mod`
    /// declaration lives in another file.
    #[test]
    fn a_gate_or_retarget_on_the_parent_declaration_is_refused() -> Result<(), String> {
        let old = "pub fn ready() -> bool {\n    true\n}\n";
        let new = "pub fn ready() -> bool {\n    false\n}\n";
        let read_real = |others: &[(&str, &str, &str)]| {
            read_in(
                ("src/real.rs", old, new),
                others,
                "ready",
                "false",
                ProbeFamily::ReturnValue,
                false,
            )
        };
        // Premise: an ungated parent declaration is admitted.
        let lib = "mod real;\npub use real::ready;\n";
        assert!(read_real(&[("src/lib.rs", lib, lib)])?.is_some());
        // cfg_mod_twin.
        let twin = "#[cfg(not(test))]\nmod real;\n#[cfg(not(test))]\npub use real::ready;\n\n#[cfg(test)]\nmod fake;\n#[cfg(test)]\npub use fake::ready;\n";
        assert_eq!(
            read_real(&[("src/lib.rs", twin, twin), ("src/fake.rs", new, new)])?,
            None
        );
        // path_swap: the parent swaps `#[path]` targets, the files swap tails.
        let old_lib = "#[path = \"x.rs\"]\npub mod a;\n#[path = \"y.rs\"]\npub mod b;\n";
        let new_lib = "#[path = \"y.rs\"]\npub mod a;\n#[path = \"x.rs\"]\npub mod b;\n";
        assert_eq!(
            read_in(
                ("src/x.rs", old, new),
                &[("src/lib.rs", old_lib, new_lib), ("src/y.rs", new, old)],
                "ready",
                "false",
                ProbeFamily::ReturnValue,
                false,
            )?,
            None
        );
        Ok(())
    }

    fn changed(diff: &str) -> Result<Vec<ChangedFile>, String> {
        let files = crate::analysis::diff::parse_unified_diff(diff);
        if files.is_empty() {
            return Err("fixture premise: diff did not parse".into());
        }
        Ok(files)
    }

    /// Review F1: a binding the diff removed from another file (a deleted
    /// `#[macro_use]` module, a removed `#[macro_use] extern crate`) is
    /// only on the base side, which the new-side index never sees.
    #[test]
    fn a_base_side_rebinding_in_another_file_is_seen() -> Result<(), String> {
        let new_lib = "pub mod a;\n";
        let facts = RaRustSyntaxAdapter.summarize_file(Path::new("src/lib.rs"), new_lib)?;
        let mut index = RustIndex::default();
        index.insert_file(PathBuf::from("src/lib.rs"), facts, true);
        // old_macro_use3: `#[macro_use] mod macros;` removed and the module
        // file deleted.
        let files = changed(
            "--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,3 +1,1 @@\n-#[macro_use]\n-mod macros;\n pub mod a;\n\
             --- a/src/macros.rs\n+++ /dev/null\n@@ -1,3 +0,0 @@\n-macro_rules! vec {\n-    ($($x:expr),*) => { ::std::vec::Vec::new() };\n-}\n",
        )?;
        assert!(base_side_may_rebind_trusted_macros(&files, &index));
        // The diff parser registers no section for a deleted file, so the
        // removed parent declaration above is what carries that binding: a
        // `macro_rules!` in a deleted module reaches other files only through
        // a removed `#[macro_use]`, `use` or `extern crate` line.
        // old_extern3: `#[macro_use] extern crate helper;` removed.
        let files = changed(
            "--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,3 +1,1 @@\n-#[macro_use]\n-extern crate helper;\n pub mod a;\n",
        )?;
        assert!(base_side_may_rebind_trusted_macros(&files, &index));
        // A removed `use helper::vec;` import.
        let files = changed(
            "--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,2 +1,1 @@\n-use helper::vec;\n pub mod a;\n",
        )?;
        assert!(base_side_may_rebind_trusted_macros(&files, &index));
        // Control: an unrelated removed line rebinds nothing.
        let files = changed(
            "--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,2 +1,1 @@\n-pub fn gone() {}\n pub mod a;\n",
        )?;
        assert!(!base_side_may_rebind_trusted_macros(&files, &index));
        // A non-Rust file is not read.
        let files = changed(
            "--- a/Cargo.toml\n+++ b/Cargo.toml\n@@ -1,2 +1,1 @@\n-helper = { path = \"helper\" }\n [package]\n",
        )?;
        assert!(!base_side_may_rebind_trusted_macros(&files, &index));
        Ok(())
    }

    /// The reader refuses a `vec!` tail when the diff-wide answer is yes,
    /// but still admits a `bool` tail.
    #[test]
    fn a_base_side_rebinding_refuses_only_vec_tails() -> Result<(), String> {
        let old = "pub fn items() -> Vec<i32> {\n    vec![1, 2]\n}\n";
        let new = "pub fn items() -> Vec<i32> {\n    vec![]\n}\n";
        assert!(read_with(old, new, "items", "vec![]", ProbeFamily::ReturnValue, false)?.is_some());
        assert_eq!(
            read_with(old, new, "items", "vec![]", ProbeFamily::ReturnValue, true)?,
            None
        );
        let old = "pub fn ready() -> bool {\n    true\n}\n";
        let new = "pub fn ready() -> bool {\n    false\n}\n";
        assert!(read_with(old, new, "ready", "false", ProbeFamily::ReturnValue, true)?.is_some());
        Ok(())
    }

    #[test]
    fn only_a_return_value_probe_on_the_tail_line_is_read() -> Result<(), String> {
        let old = "pub fn ready() -> bool {\n    true\n}\n";
        let new = "pub fn ready() -> bool {\n    false\n}\n";
        assert_eq!(
            read_family(old, new, "ready", "false", ProbeFamily::Predicate)?,
            None
        );
        Ok(())
    }

    #[test]
    fn an_equal_value_respelling_is_recognized() {
        for (old, new) in [
            ("65", "0x41"),
            ("-128", "-0x80"),
            ("0", "-0"),
            ("1_000", "1000"),
            ("\"A\"", "\"\\x41\""),
            ("\"A\"", "r#\"A\"#"),
            ("'A'", "'\\u{41}'"),
            ("b'A'", "b'\\x41'"),
            ("1.0", "1e0"),
            ("0.5f32", "5e-1f32"),
            ("0.0", "0e5"),
            ("1.", "10e-1"),
            ("2E+3", "2000.0"),
            ("[1, 2]", "[0x1, 0b10]"),
            ("true", "!false"),
        ] {
            assert!(is_equal_constant_respelling(old, new), "{old} -> {new}");
        }
    }

    #[test]
    fn a_changed_or_unfoldable_respelling_is_not_recognized() {
        for (old, new) in [
            // Same text is not an edit.
            ("65", "65"),
            // A different value.
            ("65", "66"),
            ("\"A\"", "\"B\""),
            ("1.0", "1.5"),
            // Values that round alike in f64 but differ at a wider type.
            ("9007199254740992.0f128", "9007199254740993.0f128"),
            ("9007199254740992.0", "9007199254740993.0"),
            ("0.1", "0.10000000000000000555"),
            ("0.0", "-0.0"),
            ("[1, 2]", "[1, 2, 3]"),
            // A suffix added, removed or changed can change the inferred type.
            ("1", "1i8"),
            ("1u8", "1i8"),
            ("1.0", "1.0f32"),
            // A different literal kind is a different type.
            ("b'A'", "65"),
            ("'A'", "\"A\""),
            // `vec!` may be rebound; names and calls are not constants.
            ("vec![1]", "vec![0x1]"),
            ("LIMIT", "0x41"),
            ("f(65)", "f(0x41)"),
            // An attribute can remove what the syntax shows.
            ("[1, 2]", "[1, #[cfg(any())] 2]"),
            // A minus sign on anything but a literal can overflow the
            // literal's type: `-(-128i8)` panics where `128i8` wraps.
            ("128i8", "-(-128i8)"),
            ("-(-128i8)", "128i8"),
            ("1", "--1"),
            ("-1", "-(1)"),
        ] {
            assert!(!is_equal_constant_respelling(old, new), "{old} -> {new}");
        }
    }

    #[test]
    fn a_float_tail_is_still_not_an_admitted_constant_change() -> Result<(), String> {
        // #14938 folds floats for equality only; #14852 keeps refusing them.
        let old = Const::Float {
            digits: "1".into(),
            exponent: 0,
        };
        let list = Const::List {
            vec: false,
            elements: vec![old.clone()],
        };
        assert!(old.holds_float());
        assert!(list.holds_float());
        assert!(!Const::Int(1).holds_float());
        // The admission itself refuses distinct float tails, including a
        // float list whose length changed, which `distinct` alone would admit.
        for (old_tail, new_tail) in [("1.0", "2.0"), ("[1.0]", "[1.0, 2.0]")] {
            let old = format!("pub fn ratio() -> f64 {{\n    {old_tail}\n}}\n");
            let new = format!("pub fn ratio() -> f64 {{\n    {new_tail}\n}}\n");
            assert_eq!(read(&old, &new, "ratio", new_tail)?, None, "{new_tail}");
        }
        Ok(())
    }

    /// The #14936 reading for the removed-only `return_value` probe on the
    /// deleted base line `deleted` of owner `owner`, with the diff
    /// `old` → `new`.
    fn read_deleted_guard(
        old: &str,
        new: &str,
        owner: &str,
        deleted: &str,
    ) -> Result<Option<DeletedGuardAgreement>, String> {
        let path = "src/lib.rs";
        let facts = RaRustSyntaxAdapter.summarize_file(Path::new(path), new)?;
        let changed_file =
            crate::analysis::diff::parse_unified_diff(&unified_diff_at(path, old, new))
                .into_iter()
                .next()
                .ok_or("fixture premise: diff did not parse")?;
        let removed = changed_file
            .removed_lines
            .iter()
            .find(|line| line.text.trim() == deleted)
            .ok_or("fixture premise: deleted line is not a removed line")?;
        let line = removed.new_side_line;
        let function = facts
            .functions
            .iter()
            .find(|function| {
                function.name == owner && (function.start_line..=function.end_line).contains(&line)
            })
            .cloned()
            .ok_or("fixture premise: owner not indexed")?;
        let mut index = RustIndex::default();
        index.insert_file(PathBuf::from(path), facts, true);
        let modules = ModuleDeclarations::read(
            std::iter::once((Path::new(path), new)),
            std::iter::once((
                &changed_file,
                find_file_facts(&index, Path::new(path)).map(|facts| facts.data().source.as_ref()),
            )),
        );
        let probe = Probe {
            before_condition: None,
            id: ProbeId("probe:src_lib_rs:guard".into()),
            location: SourceLocation::new(path, line, 5),
            owner: Some(function.id.clone()),
            family: ProbeFamily::ReturnValue,
            delta: DeltaKind::Value,
            before: Some(deleted.into()),
            after: None,
            expression: deleted.into(),
            expected_sinks: Vec::new(),
            required_oracles: Vec::new(),
        };
        Ok(deleted_guard_agreement(
            &probe,
            &function,
            &changed_file,
            &index,
            &modules,
        ))
    }

    const GUARD: &str = "if x == 0 { return Vec::new(); }";

    #[test]
    fn deleted_guard_returning_the_remaining_tail_value_agrees() -> Result<(), String> {
        // #14936: `pick(0)` returns an empty vector on both sides.
        let old = format!("pub fn pick(x: u32) -> Vec<u32> {{\n    {GUARD}\n    vec![]\n}}\n");
        let new = "pub fn pick(x: u32) -> Vec<u32> {\n    vec![]\n}\n";
        let agreement =
            read_deleted_guard(&old, new, "pick", GUARD)?.ok_or("same-value guard refused")?;
        assert_eq!(agreement.tail, "vec![]");
        assert!(agreement.summary().contains("deleted_guard_same_value"));
        // The tail the diff also changed (the fixture's `vec![9]` -> `vec![]`)
        // is read on the candidate side, where the guard's inputs now go.
        let old = format!("pub fn pick(x: u32) -> Vec<u32> {{\n    {GUARD}\n    vec![9]\n}}\n");
        assert!(read_deleted_guard(&old, new, "pick", GUARD)?.is_some());
        // Either spelling of the empty value on either side.
        let old = "pub fn pick(x: u32) -> Vec<u32> {\n    if x == 0 { return vec![]; }\n    Vec::new()\n}\n";
        let new = "pub fn pick(x: u32) -> Vec<u32> {\n    Vec::new()\n}\n";
        assert!(read_deleted_guard(old, new, "pick", "if x == 0 { return vec![]; }")?.is_some());
        let old = "pub fn flag(x: u32) -> bool {\n    if x == 0 { return !true; }\n    false\n}\n";
        let new = "pub fn flag(x: u32) -> bool {\n    false\n}\n";
        assert!(read_deleted_guard(old, new, "flag", "if x == 0 { return !true; }")?.is_some());
        Ok(())
    }

    #[test]
    fn deleted_guard_whose_value_differs_or_is_unread_keeps_today() -> Result<(), String> {
        let new_nine = "pub fn pick(x: u32) -> Vec<u32> {\n    vec![9]\n}\n";
        // A remaining tail that differs: the guard's inputs now see `[9]`.
        let old = format!("pub fn pick(x: u32) -> Vec<u32> {{\n    {GUARD}\n    vec![9]\n}}\n");
        assert_eq!(read_deleted_guard(&old, new_nine, "pick", GUARD)?, None);
        // A tail that names a parameter can differ on the guard's inputs.
        let old = "pub fn pick(x: u32) -> u32 {\n    if x == 0 { return 0; }\n    x\n}\n";
        let new = "pub fn pick(x: u32) -> u32 {\n    x\n}\n";
        assert_eq!(
            read_deleted_guard(old, new, "pick", "if x == 0 { return 0; }")?,
            None
        );
        // A candidate body with a statement before its tail.
        let old = "pub fn pick(x: u32) -> u32 {\n    if x == 0 { return 0; }\n    if x == 1 { return 1; }\n    0\n}\n";
        let new = "pub fn pick(x: u32) -> u32 {\n    if x == 1 { return 1; }\n    0\n}\n";
        assert_eq!(
            read_deleted_guard(old, new, "pick", "if x == 0 { return 0; }")?,
            None
        );
        // A guard that is not the base body's first statement.
        let old = "pub fn pick(x: u32) -> u32 {\n    let _y = x;\n    if x == 0 { return 0; }\n    0\n}\n";
        let new = "pub fn pick(x: u32) -> u32 {\n    let _y = x;\n    0\n}\n";
        assert_eq!(
            read_deleted_guard(old, new, "pick", "if x == 0 { return 0; }")?,
            None
        );
        // A guard with an `else`, or a returned value the folder cannot read.
        let old = "pub fn pick(x: u32) -> u32 {\n    if x == 0 { return 0; } else {}\n    0\n}\n";
        let new = "pub fn pick(x: u32) -> u32 {\n    0\n}\n";
        assert_eq!(
            read_deleted_guard(old, new, "pick", "if x == 0 { return 0; } else {}")?,
            None
        );
        let old = "pub fn pick(x: u32) -> u32 {\n    if x == 0 { return zero(); }\n    0\n}\n";
        assert_eq!(
            read_deleted_guard(old, new, "pick", "if x == 0 { return zero(); }")?,
            None
        );
        Ok(())
    }

    #[test]
    fn a_second_deleted_guard_differing_only_inside_a_string_borrows_no_agreement()
    -> Result<(), String> {
        // Both guards are deleted and share one candidate-side line. Only the
        // first returns the tail's value; the second differs only by the
        // whitespace inside its string literal.
        let first = "if x == \"a b\" { return \"a b\"; }";
        let second = "if x == \"a  b\" { return \"a  b\"; }";
        let old = format!(
            "pub fn pick(x: &str) -> &'static str {{\n    {first}\n    {second}\n    \"a b\"\n}}\n"
        );
        let new = "pub fn pick(x: &str) -> &'static str {\n    \"a b\"\n}\n";
        assert!(read_deleted_guard(&old, new, "pick", first)?.is_some());
        assert_eq!(read_deleted_guard(&old, new, "pick", second)?, None);
        Ok(())
    }
}
