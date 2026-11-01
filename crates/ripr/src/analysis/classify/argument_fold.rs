//! RIPR-SPEC-0158: an owner-call argument that computes over the test's own
//! literal bindings (`score(base + 1)` after `let base = 9;`).
//!
//! A computed argument is not one of its operands (RIPR-SPEC-0001, #6672), so
//! the row reader gives it no value. When every operand is a decimal literal
//! or a binding the test declares exactly once, with a plain decimal
//! initializer and no later write, the arithmetic is as exact as the literal
//! `10` would have been, and the shared evaluator folds it. Anything else
//! (a call, a method, a cast, a `mut` or shadowed name, a value that
//! over- or underflows) keeps the unreadable reading.

use super::value_transfer::{EvalOutcome, ExactInputs, TypedValue, evaluate_initializer};
use crate::analysis::extract::mask_comments_and_strings;
use crate::analysis::rust_index::{RustIndex, TestSummary};
use crate::analysis::syntax::parse_clean_source_file;
use crate::analysis::value_resolution::{NamedConstant, named_constant};
use ra_ap_syntax::{
    AstNode,
    ast::{self, HasName},
};

/// Where a `const` operand or a nullary helper is declared: the source of the
/// file the test lives in. Without it only the test's own bindings fold.
#[derive(Clone, Copy, Default)]
pub(super) struct FoldScope<'a> {
    file_source: Option<&'a str>,
    /// The line the test starts on: a declaration folds only when that
    /// position can see it.
    test_line: usize,
}

impl<'a> FoldScope<'a> {
    /// The scope of `test`'s own file in `index`.
    pub(super) fn of_test(test: &TestSummary, index: &'a RustIndex) -> Self {
        Self {
            file_source: index
                .files()
                .get(&test.file)
                .map(|facts| facts.data().source.as_ref()),
            test_line: test.start_line,
        }
    }

    #[cfg(test)]
    pub(super) fn of_source(file_source: &'a str, test_line: usize) -> Self {
        Self {
            file_source: Some(file_source),
            test_line,
        }
    }

    /// The decimal value of the one `const`/`static` the file declares as
    /// `name`, when its initializer is a plain decimal literal.
    fn const_value(self, name: &str) -> Option<String> {
        let source = self.file_source?;
        if !is_const_like(name) || file_imports(source, name) {
            return None;
        }
        let value = match named_constant(source, name) {
            NamedConstant::Value(value) if is_decimal(&value) => value,
            _ => return None,
        };
        let masked = mask_comments_and_strings(source);
        let [at] = [
            declaration_offsets(&masked, "const", name),
            declaration_offsets(&masked, "static", name),
        ]
        .concat()[..] else {
            return None;
        };
        self.sees_declaration(&masked, at).then_some(value)
    }

    /// The decimal value the one nullary free function `fn name() -> u64 { 1000 }`
    /// of the file returns, when its body is nothing but that literal. A
    /// second `fn name` anywhere in the file, an `impl` or `trait` method,
    /// and an import of the name all leave the call unread: the call may
    /// reach a different function than the one the scan found.
    fn helper_value(self, name: &str) -> Option<String> {
        let source = self.file_source?;
        if !source.contains(&format!("fn {name}")) || file_imports(source, name) {
            return None;
        }
        let masked = mask_comments_and_strings(source);
        let [at] = declaration_offsets(&masked, "fn", name)[..] else {
            return None;
        };
        if declared_in_associated_block(&masked, at) || !self.sees_declaration(&masked, at) {
            return None;
        }
        let rest = masked[at..].strip_prefix("fn")?.trim_start();
        let rest = rest.strip_prefix(name)?.trim_start();
        let rest = rest.strip_prefix('(')?.trim_start().strip_prefix(')')?;
        let rest = rest.trim_start();
        let rest = match rest.strip_prefix("->") {
            Some(typed) => {
                let (ty, body) = typed.split_once('{')?;
                is_integer_type(ty.trim()).then_some(())?;
                body
            }
            None => rest.strip_prefix('{')?,
        };
        let literal = rest.split_once('}')?.0.trim();
        is_decimal(literal).then(|| literal.to_string())
    }
}

impl FoldScope<'_> {
    /// Whether a declaration at byte `at` of the masked file is the item the
    /// test's bare name reaches: declared in the test's own module, or in its
    /// parent module with `use super::*` as the only glob import. A sibling
    /// module's item, an item inside another function, and a name a different
    /// glob import may bind all decline.
    fn sees_declaration(self, masked: &str, at: usize) -> bool {
        let Some(test_at) = line_start(masked, self.test_line) else {
            return false;
        };
        let declared = block_chain(masked, at);
        let tested = block_chain(masked, test_at);
        if !tested.starts_with(&declared) {
            return false;
        }
        let modules = |chain: &[(usize, bool)]| chain.iter().filter(|(_, module)| *module).count();
        match modules(&tested) - modules(&declared) {
            0 => true,
            1 => only_super_glob(masked),
            _ => false,
        }
    }
}

/// Byte offset where 1-based `line` starts.
fn line_start(masked: &str, line: usize) -> Option<usize> {
    let mut start = 0;
    for _ in 1..line {
        start += masked[start..].find('\n')? + 1;
    }
    (line >= 1).then_some(start)
}

/// The braces open at byte `at`, outermost first, each flagged when its header
/// declares a `mod`.
fn block_chain(masked: &str, at: usize) -> Vec<(usize, bool)> {
    let mut open = Vec::new();
    let mut header_start = 0;
    for (offset, ch) in masked[..at].char_indices() {
        match ch {
            '{' => {
                let module = masked[header_start..offset]
                    .split(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_'))
                    .any(|word| word == "mod");
                open.push((offset, module));
                header_start = offset + 1;
            }
            '}' => {
                open.pop();
                header_start = offset + 1;
            }
            ';' => header_start = offset + 1,
            _ => {}
        }
    }
    open
}

/// Whether the file has `use super::*` and no other glob import.
fn only_super_glob(masked: &str) -> bool {
    let mut super_glob = false;
    for line in masked.lines().map(str::trim_start) {
        if !(line.starts_with("use ") || line.starts_with("pub use ")) || !line.contains('*') {
            continue;
        }
        if line.replace(' ', "") == "usesuper::*;" {
            super_glob = true;
        } else {
            return false;
        }
    }
    super_glob
}

/// The decimal value `argument` computes in a test whose body is
/// `test_body`, when every operand is exact; `None` otherwise.
pub(super) fn folded_argument(
    test_body: &str,
    argument: &str,
    scope: FoldScope<'_>,
) -> Option<String> {
    let text = argument.trim();
    if text.is_empty()
        || !text.chars().all(|ch| {
            ch.is_ascii_alphanumeric()
                || ch == '_'
                || ch.is_whitespace()
                || matches!(ch, '+' | '-' | '*' | '/' | '%' | '(' | ')')
        })
    {
        return None;
    }
    let mut inputs = ExactInputs::new();
    let mut expression = String::new();
    let mut resolved = false;
    let mut rest = text;
    while let Some(ch) = rest.chars().next() {
        if !(ch.is_ascii_alphanumeric() || ch == '_') {
            expression.push(ch);
            rest = &rest[ch.len_utf8()..];
            continue;
        }
        let end = rest
            .find(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_'))
            .unwrap_or(rest.len());
        let (token, after) = rest.split_at(end);
        rest = after;
        if token.starts_with(|ch: char| ch.is_ascii_digit()) {
            // A suffixed or separated literal (`1u32`, `1_000`) is not read.
            if !is_decimal(token) {
                return None;
            }
            expression.push_str(token);
        } else if let Some(arguments) = rest.trim_start().strip_prefix('(') {
            // `big_order()`: a call reads only as a nullary helper whose body
            // is one literal, and only when the test binds no such name.
            let closed = arguments.trim_start().strip_prefix(')')?;
            if binding_pattern_ends(test_body, token)?.0 != 0 {
                return None;
            }
            expression.push_str(&scope.helper_value(token)?);
            rest = closed;
            resolved = true;
        } else if is_const_like(token) {
            if binding_pattern_ends(test_body, token)?.0 != 0 {
                return None;
            }
            inputs.insert(token.to_string(), scope.const_value(token)?);
            expression.push_str(token);
            resolved = true;
        } else {
            if !inputs.contains_key(token) {
                let value =
                    crate::analysis::value_resolution::test_let_bound_literal(test_body, token)?;
                if !is_decimal(&value) || !bound_once_before_use(test_body, token, text) {
                    return None;
                }
                inputs.insert(token.to_string(), value);
            }
            expression.push_str(token);
            resolved = true;
        }
    }
    // Arithmetic over literals alone (`10 * 2`) names no binding; activation
    // keeps reading it as before.
    if !resolved {
        return None;
    }
    match evaluate_initializer(&expression, &inputs) {
        EvalOutcome::Exact {
            value: TypedValue::Index(value),
            ..
        } => Some(value.to_string()),
        _ => None,
    }
}

/// The decimal value a chain-edge argument computes over the hop caller's own
/// parameters (`add_fee(items * 10)`), when `read` answers each parameter the
/// expression names with a decimal row value the caller keeps intact.
///
/// Only decimal literals, such parameter names, `+ - * / %` and parentheses
/// are read; a call, method, field, cast, suffixed literal, constant, unary
/// sign, or any name `read` refuses leaves the argument unread. The shared
/// evaluator folds it, so an overflow, an underflow, a division by zero or a
/// result at its bound also leaves it unread. An expression naming no
/// parameter is not this reading.
pub(super) fn folded_hop_argument(
    argument: &str,
    mut read: impl FnMut(&str) -> Option<String>,
) -> Option<String> {
    let text = argument.trim();
    if text.is_empty()
        || !text.chars().all(|ch| {
            ch.is_ascii_alphanumeric()
                || ch == '_'
                || ch.is_whitespace()
                || matches!(ch, '+' | '-' | '*' | '/' | '%' | '(' | ')')
        })
    {
        return None;
    }
    let mut inputs = ExactInputs::new();
    let mut expression = String::new();
    let mut named = false;
    let mut rest = text;
    while let Some(ch) = rest.chars().next() {
        if !(ch.is_ascii_alphanumeric() || ch == '_') {
            expression.push(ch);
            rest = &rest[ch.len_utf8()..];
            continue;
        }
        let end = rest
            .find(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_'))
            .unwrap_or(rest.len());
        let (token, after) = rest.split_at(end);
        rest = after;
        if token.starts_with(|ch: char| ch.is_ascii_digit()) {
            if !is_decimal(token) {
                return None;
            }
        } else {
            // `f(items)` applies a name; it is no operand.
            if rest.trim_start().starts_with('(') {
                return None;
            }
            if !inputs.contains_key(token) {
                let value = read(token)?;
                if !is_decimal(&value) {
                    return None;
                }
                inputs.insert(token.to_string(), value);
            }
            named = true;
        }
        expression.push_str(token);
    }
    if !named {
        return None;
    }
    match evaluate_initializer(&expression, &inputs) {
        EvalOutcome::Exact {
            value: TypedValue::Index(value),
            ..
        } => Some(value.to_string()),
        _ => None,
    }
}

fn is_decimal(text: &str) -> bool {
    !text.is_empty() && text.chars().all(|ch| ch.is_ascii_digit())
}

/// `LIMIT`, `MAX_ORDER`: an upper-case constant name.
fn is_const_like(name: &str) -> bool {
    name.starts_with(|ch: char| ch.is_ascii_uppercase())
        && name
            .chars()
            .all(|ch| ch.is_ascii_uppercase() || ch.is_ascii_digit() || ch == '_')
}

fn is_integer_type(ty: &str) -> bool {
    matches!(
        ty,
        "u8" | "u16"
            | "u32"
            | "u64"
            | "u128"
            | "usize"
            | "i8"
            | "i16"
            | "i32"
            | "i64"
            | "i128"
            | "isize"
    )
}

/// Whether some `use` line of the file mentions `name`: the call or constant
/// may then be another item than the file's own declaration.
fn file_imports(source: &str, name: &str) -> bool {
    mask_comments_and_strings(source).lines().any(|line| {
        let line = line.trim_start();
        (line.starts_with("use ") || line.starts_with("pub use "))
            && line
                .split(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_'))
                .any(|word| word == name)
    })
}

/// Byte offsets of every `<keyword> name` declaration in masked source.
fn declaration_offsets(masked: &str, keyword: &str, name: &str) -> Vec<usize> {
    let width = keyword.len() + 1 + name.len();
    masked
        .match_indices(format!("{keyword} {name}").as_str())
        .filter_map(|(at, _)| {
            let word = |ch: char| ch.is_ascii_alphanumeric() || ch == '_';
            let before = masked[..at].chars().next_back();
            let after = masked[at + width..].chars().next();
            (before.is_none_or(|ch| !word(ch)) && after.is_none_or(|ch| !word(ch))).then_some(at)
        })
        .collect()
}

/// Whether the byte `at` sits inside an `impl` or `trait` block, where a
/// bare call would not reach the function.
fn declared_in_associated_block(masked: &str, at: usize) -> bool {
    let mut open: Vec<bool> = Vec::new();
    let mut header_start = 0;
    for (offset, ch) in masked[..at].char_indices() {
        match ch {
            '{' => {
                let header = &masked[header_start..offset];
                open.push(
                    header
                        .split(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_'))
                        .any(|word| matches!(word, "impl" | "trait")),
                );
                header_start = offset + 1;
            }
            '}' => {
                open.pop();
                header_start = offset + 1;
            }
            ';' => header_start = offset + 1,
            _ => {}
        }
    }
    open.contains(&true)
}

/// How many patterns of the parsed test body bind `name`, and the end offset
/// of each; `None` when the body does not parse or `name` is assigned.
fn binding_pattern_ends(test_body: &str, name: &str) -> Option<(usize, Vec<usize>)> {
    let wrapped;
    let source =
        if test_body.trim_start().starts_with("fn ") || test_body.trim_start().starts_with("#[") {
            test_body
        } else {
            wrapped = format!("fn __test() {{\n{test_body}\n}}");
            &wrapped
        };
    let parse = parse_clean_source_file(source)?;
    let mut ends = Vec::new();
    for node in parse.tree().syntax().descendants() {
        if ast::IdentPat::cast(node.clone())
            .and_then(|pattern| pattern.name())
            .is_some_and(|bound| bound.text() == name)
        {
            ends.push(usize::from(node.text_range().end()));
        }
        if ast::BinExpr::cast(node).is_some_and(|expr| {
            matches!(expr.op_kind(), Some(ast::BinaryOp::Assignment { .. }))
                && expr
                    .lhs()
                    .is_some_and(|lhs| lhs.syntax().text().to_string().trim() == name)
        }) {
            return None;
        }
    }
    Some((ends.len(), ends))
}

/// Whether `name` is bound by exactly one pattern in the test body, never
/// assigned, and that binding ends before the first place `argument` is
/// written: the `let` itself. A `for` pattern, a closure parameter, an
/// `if let` or a match arm that reuses the name is a second binding that may
/// shadow the `let` at the call, and a `let` that follows the call cannot be
/// the value the call saw (a same-named item may). A body that does not parse
/// counts as rebinding.
fn bound_once_before_use(test_body: &str, name: &str, argument: &str) -> bool {
    let Some((_, ends)) = binding_pattern_ends(test_body, name) else {
        return false;
    };
    let [declared_end] = ends[..] else {
        return false;
    };
    let source =
        if test_body.trim_start().starts_with("fn ") || test_body.trim_start().starts_with("#[") {
            test_body.to_string()
        } else {
            format!("fn __test() {{\n{test_body}\n}}")
        };
    source
        .match_indices(argument)
        .next()
        .is_some_and(|(at, _)| at >= declared_end)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fold(body: &str, argument: &str) -> Option<String> {
        folded_argument(body, argument, FoldScope::default())
    }

    fn fold_in(file: &str, body: &str, argument: &str) -> Option<String> {
        fold_at(file, 1, body, argument)
    }

    fn fold_at(file: &str, test_line: usize, body: &str, argument: &str) -> Option<String> {
        folded_argument(body, argument, FoldScope::of_source(file, test_line))
    }

    #[test]
    fn arithmetic_over_a_single_literal_binding_folds() {
        let calls = "score(base + 1); score(base * 2); score((base + 1) * 2); score(base-1); score(2 + 3); score(base + 2 * 3);";
        let body = format!("fn t() {{\n    let base = 9;\n    {calls}\n}}");
        assert_eq!(fold(&body, "base + 1").as_deref(), Some("10"));
        assert_eq!(fold(&body, "base * 2").as_deref(), Some("18"));
        assert_eq!(fold(&body, "(base + 1) * 2").as_deref(), Some("20"));
        assert_eq!(fold(&body, "base-1").as_deref(), Some("8"));
        assert_eq!(fold(&body, "2 + 3"), None, "literals alone name no binding");
        assert_eq!(fold(&body, "base + 2 * 3").as_deref(), Some("15"));
        assert_eq!(
            fold(&body, "base + 7"),
            None,
            "an argument the body never writes"
        );
    }

    #[test]
    fn two_bindings_fold_together() {
        let body = "fn t() {\n    let width = 4;\n    let height = 5;\n    let _ = score(width * height); let _ = score(width * width);\n}";
        assert_eq!(fold(body, "width * height").as_deref(), Some("20"));
        assert_eq!(fold(body, "width * width").as_deref(), Some("16"));
    }

    #[test]
    fn a_binding_that_may_hold_another_value_does_not_fold() {
        let mutable = "fn t() {\n    let mut base = 9;\n    base += 1;\n    score(base + 1);\n}";
        assert_eq!(fold(mutable, "base + 1"), None);
        let shadowed =
            "fn t() {\n    let base = 9;\n    let base = base + 5;\n    score(base + 1);\n}";
        assert_eq!(fold(shadowed, "base + 1"), None);
        let looped = "fn t() {\n    let base = 9;\n    for base in 0..3 {\n        score(base + 1);\n    }\n}";
        assert_eq!(fold(looped, "base + 1"), None);
        let closure = "fn t() {\n    let base = 9;\n    let f = |base: u32| score(base + 1);\n}";
        assert_eq!(fold(closure, "base + 1"), None);
        let pattern = "fn t() {\n    let base = 9;\n    if let Some(base) = lookup() {\n        score(base + 1);\n    }\n}";
        assert_eq!(fold(pattern, "base + 1"), None);
        let later = "fn t() {\n    assert!(score(base + 1));\n    let base = 9;\n}";
        assert_eq!(fold(later, "base + 1"), None, "the let follows the call");
        let assigned = "fn t() {\n    let base = 9;\n    base = 3;\n    score(base + 1);\n}";
        assert_eq!(fold(assigned, "base + 1"), None);
    }

    #[test]
    fn an_operand_that_is_not_a_plain_decimal_binding_does_not_fold() {
        let body = "fn t() {\n    let base = 9;\n    let name = \"ab\";\n    let ratio = 1.5;\n    let sum = base + 1;\n    let big = 9u32;\n}";
        assert_eq!(fold(body, "name + 1"), None);
        assert_eq!(fold(body, "ratio + 1"), None);
        assert_eq!(fold(body, "sum + 1"), None);
        assert_eq!(fold(body, "big + 1"), None);
        assert_eq!(fold(body, "unbound + 1"), None);
        assert_eq!(fold(body, "base + unbound"), None);
    }

    #[test]
    fn a_call_a_cast_or_a_method_around_the_operands_does_not_fold() {
        let body = "fn t() {\n    let base = 9;\n}";
        for argument in [
            "load(base) + 1",
            "base.max(3) + 1",
            "base as u8 + 1",
            "base + 1u32",
            "base + 1_0",
            "base + 1.5",
            "base + helper!()",
            "&base",
            "[base; 2]",
            "-base",
            "",
        ] {
            assert_eq!(fold(body, argument), None, "`{argument}`");
        }
    }

    #[test]
    fn arithmetic_that_does_not_stay_exact_does_not_fold() {
        let body = "fn t() {\n    let base = 9;\n    let zero = 0;\n    let huge = 2147483647;\n}";
        assert_eq!(fold(body, "base - 10"), None, "underflow");
        assert_eq!(fold(body, "base / zero"), None, "division by zero");
        assert_eq!(fold(body, "huge + 1"), None, "past the evaluator's bound");
    }

    #[test]
    fn a_body_that_does_not_parse_folds_nothing() {
        assert_eq!(fold("fn t() { let base = 9; ", "base + 1"), None);
    }

    const FILE: &str = "pub const BASE: u32 = 9;\nconst WIDE: u64 = 4000000000;\nfn big() -> u32 {\n    10\n}\nfn small() -> u64 { 9 }\n";

    #[test]
    fn a_file_const_and_a_nullary_literal_helper_fold() {
        let body = "fn t() {\n    let n = 3;\n    score(big() * n);\n}";
        assert_eq!(fold_in(FILE, body, "BASE + 1").as_deref(), Some("10"));
        assert_eq!(fold_in(FILE, body, "BASE").as_deref(), Some("9"));
        assert_eq!(fold_in(FILE, body, "big()").as_deref(), Some("10"));
        assert_eq!(fold_in(FILE, body, "small( ) + 1").as_deref(), Some("10"));
        assert_eq!(fold_in(FILE, body, "big() - BASE").as_deref(), Some("1"));
        assert_eq!(fold_in(FILE, body, "big() * n").as_deref(), Some("30"));
    }

    #[test]
    fn a_name_the_file_does_not_pin_down_does_not_fold() {
        let body = "fn t() {}";
        for (file, argument) in [
            ("const BASE: u32 = 9;\nconst BASE: u32 = 14;\n", "BASE + 1"),
            ("const BASE: u32 = 4 + 5;\n", "BASE + 1"),
            ("static mut BASE: u32 = 9;\n", "BASE + 1"),
            ("use other::BASE;\nconst BASE: u32 = 9;\n", "BASE + 1"),
            ("const BASE: &str = \"9\";\n", "BASE + 1"),
            ("const OTHER: u32 = 9;\n", "BASE + 1"),
            ("fn big() -> u32 { 10 }\nfn big() -> u32 { 15 }\n", "big()"),
            (
                "mod a { pub fn big() -> u32 { 10 } }\nfn big() -> u32 { 15 }\n",
                "big()",
            ),
            ("use other::big;\nfn big() -> u32 { 10 }\n", "big()"),
            ("fn big() -> u32 { 10 + extra() }\n", "big()"),
            ("fn big() -> u32 { let x = 10; x }\n", "big()"),
            ("fn big(extra: u32) -> u32 { 10 }\n", "big()"),
            ("fn big<T>() -> u32 { 10 }\n", "big()"),
            ("fn big() -> String { 10 }\n", "big()"),
            ("fn big() -> u32 { 1_0 }\n", "big()"),
            ("fn big() -> u32 { 10u32 }\n", "big()"),
            ("impl Rates {\n    fn big() -> u32 { 10 }\n}\n", "big()"),
            ("trait Rates {\n    fn big() -> u32 { 10 }\n}\n", "big()"),
            ("// fn big() -> u32 { 10 }\n", "big()"),
            ("fn bigger() -> u32 { 10 }\n", "big()"),
            ("fn big() -> u32 { 10 }\n", "big(1)"),
            ("fn big() -> u32 { 10 }\n", "BIG()"),
            ("const WIDE: u64 = 4000000000;\n", "WIDE + 1"),
        ] {
            assert_eq!(
                fold_in(file, body, argument),
                None,
                "`{argument}` in `{file}`"
            );
        }
        assert_eq!(
            fold(body, "BASE + 1"),
            None,
            "no file scope reads no constants"
        );
        assert_eq!(fold(body, "big()"), None, "no file scope reads no helpers");
    }

    #[test]
    fn a_helper_or_constant_the_test_may_rebind_does_not_fold() {
        let shadowed = "fn t() {\n    let big = || 15;\n    score(big());\n}";
        assert_eq!(fold_in(FILE, shadowed, "big()"), None, "a local closure");
        let nested = "fn t() {\n    fn big() -> u32 { 15 }\n}";
        let file = format!("{FILE}{nested}");
        assert_eq!(
            fold_in(&file, nested, "big()"),
            None,
            "a second declaration"
        );
        let inner_const = "fn t() {\n    const BASE: u32 = 14;\n}";
        let file = format!("{FILE}{inner_const}");
        assert_eq!(fold_in(&file, inner_const, "BASE + 1"), None);
    }

    #[test]
    fn a_method_call_a_path_or_a_cast_around_a_helper_does_not_fold() {
        let body = "fn t() {}";
        for argument in [
            "Self::big()",
            "rates::big()",
            "x.big()",
            "big() as u8",
            "&big()",
            "big()?",
        ] {
            assert_eq!(fold_in(FILE, body, argument), None, "`{argument}`");
        }
    }

    #[test]
    fn a_declaration_the_test_cannot_see_does_not_fold() {
        let body = "fn t() {}";
        // Line numbers below count from the test's `#[test]` line.
        let parent = "const BASE: u32 = 9;\nfn big() -> u32 { 10 }\n#[cfg(test)]\nmod tests {\n    use super::*;\n    #[test]\n    fn t() {}\n}\n";
        assert_eq!(fold_at(parent, 6, body, "BASE + 1").as_deref(), Some("10"));
        assert_eq!(fold_at(parent, 6, body, "big()").as_deref(), Some("10"));
        // A sibling module's item is not in scope at the test.
        let sibling = "mod hidden {\n    pub const BASE: u32 = 9;\n    pub fn big() -> u32 { 10 }\n}\nmod tests {\n    use super::*;\n    #[test]\n    fn t() {}\n}\n";
        assert_eq!(fold_at(sibling, 7, body, "BASE + 1"), None);
        assert_eq!(fold_at(sibling, 7, body, "big()"), None);
        // A glob of another module may bind the name instead of the parent's.
        let glob = parent.replace(
            "use super::*;",
            "use super::*;\n    use crate::external::*;",
        );
        assert_eq!(fold_at(&glob, 7, body, "BASE + 1"), None);
        assert_eq!(fold_at(&glob, 7, body, "big()"), None);
        // Without `use super::*` the parent's item is not imported at all.
        let bare = parent.replace("    use super::*;\n", "    use crate::external::*;\n");
        assert_eq!(fold_at(&bare, 6, body, "BASE + 1"), None);
        // Two modules down needs a path the fold does not read.
        let deep = "const BASE: u32 = 9;\nmod a {\n    mod tests {\n        use super::*;\n        #[test]\n        fn t() {}\n    }\n}\n";
        assert_eq!(fold_at(deep, 5, body, "BASE + 1"), None);
        // An item inside another function is not in scope either.
        let inner = "fn other() {\n    const BASE: u32 = 9;\n}\n#[test]\nfn t() {}\n";
        assert_eq!(fold_at(inner, 4, body, "BASE + 1"), None);
        // A declaration in the test's own module wins over any glob.
        let own = "mod tests {\n    use crate::external::*;\n    const BASE: u32 = 9;\n    #[test]\n    fn t() {}\n}\n";
        assert_eq!(fold_at(own, 4, body, "BASE + 1").as_deref(), Some("10"));
        // A file with no position for the test reads nothing.
        assert_eq!(fold_at(parent, 0, body, "BASE + 1"), None);
        assert_eq!(fold_at(parent, 99, body, "BASE + 1"), None);
    }

    #[test]
    fn a_hop_argument_folds_only_over_readable_caller_parameters() {
        let read = |name: &str| match name {
            "items" => Some("3".to_string()),
            "big" => Some("2000000000".to_string()),
            "text" => Some("\"3\"".to_string()),
            _ => None,
        };
        let fold = |argument: &str| folded_hop_argument(argument, read);
        assert_eq!(fold("items * 10").as_deref(), Some("30"));
        assert_eq!(fold("(items + 1) * 2").as_deref(), Some("8"));
        assert_eq!(fold("items - 3 + items").as_deref(), Some("3"));
        for refused in [
            "10 * 3",
            "items - 4",
            "items / (items - 3)",
            "big * 2",
            "-items",
            "items * 10u32",
            "items * 1_0",
            "items.pow(2)",
            "items as u8",
            "items * other",
            "items * LIMIT",
            "text + 1",
            "items(2)",
            "items * f(1)",
            "",
        ] {
            assert_eq!(fold(refused), None, "{refused}");
        }
    }
}
