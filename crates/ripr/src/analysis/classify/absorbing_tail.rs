//! #10875: a changed `if` condition whose only effect is the choice of a `let`
//! value that std's `Ord::min` / `Ord::clamp` then pins to one literal cannot
//! reach the returned value.
//!
//! ```text
//! pub fn f(n: u32) -> u32 {
//!     let r: u32 = if n == 5 { 1 } else { 0 };
//!     r.min(0)
//! }
//! ```
//!
//! The method spelling alone does not establish primitive `Ord` semantics: a
//! `#![no_implicit_prelude]` crate with its own `min` returns the receiver and
//! makes the condition observable. So the rule has two halves, both of which
//! must hold, and it names a propagation limit (never a gap or a credit) when
//! they do:
//!
//! * a body shape in which the absorbing call is the whole tail, its receiver
//!   is a binding nothing else reads, that binding is chosen by the changed
//!   condition between two integer literals, and the owner's return type is a
//!   primitive integer that makes the absorption exact; and
//! * a workspace in which nothing could replace std's method dispatch: no
//!   `no_implicit_prelude`/`no_core`, no `fn min`/`fn clamp` definition (a
//!   workspace trait or impl providing the method), no `include!` of code ripr
//!   did not index, and no item or `use ... as` renaming the primitive.
//!
//! The changed condition is not established as pure: it may index, overflow, panic or
//! have side effects a test could observe. The result only demotes the finding
//! to a propagation limit (`propagation_unknown`); it never credits or flags a
//! gap. An untyped `let r = ..; r.min(0)` does not compile (E0689), so the
//! positives annotate the binding.
//!
//! Residual, named and not closed here: a substituted `std`/`core` package or
//! ambient `RUSTFLAGS` that changes the effective prelude is outside what the
//! index can authenticate.

use super::super::rust_index::{FunctionSummary, RustIndex};
use super::flow::flow_sink;
use crate::analysis::syntax::parse_clean_source_file;
use crate::domain::*;
use ra_ap_syntax::{
    AstNode,
    ast::{self, HasArgList, HasGenericArgs, HasGenericParams, HasName},
};

const UNSIGNED: &[&str] = &["u8", "u16", "u32", "u64", "u128", "usize"];
const SIGNED: &[&str] = &["i8", "i16", "i32", "i64", "i128", "isize"];

/// The unknown sink for a changed condition that an absorbing std call hides,
/// or `None` (keep the ordinary sink) when either half of the rule is missing.
pub(in crate::analysis) fn absorbing_tail_sink(
    probe: &Probe,
    owner_fn: Option<&FunctionSummary>,
    index: &RustIndex,
    workspace_complete: bool,
) -> Option<FlowSinkFact> {
    if !workspace_complete {
        return None;
    }
    let owner = owner_fn?;
    // The cheap body shape first: the workspace scan only runs for it.
    let shape = absorbing_tail_shape(probe, owner)?;
    if !workspace_keeps_std_dispatch(
        index
            .files()
            .values()
            .map(|file| file.data().source.as_ref()),
        &shape.primitive,
    ) {
        return None;
    }
    Some(sink_for(probe, owner, &shape))
}

fn sink_for(probe: &Probe, owner: &FunctionSummary, shape: &AbsorbingTail) -> FlowSinkFact {
    flow_sink(
        FlowSinkKind::Unknown,
        format!(
            "`{}` pins the returned `{}` to one literal; propagation unknown",
            shape.call, shape.primitive
        ),
        probe.location.line,
        Some(owner.id.clone()),
    )
}

#[derive(Debug, PartialEq, Eq)]
struct AbsorbingTail {
    /// The tail call as written, for the sink text.
    call: String,
    /// The owner's primitive integer return type.
    primitive: String,
}

/// The body half of the rule. Fails closed on every shape it does not name.
fn absorbing_tail_shape(probe: &Probe, owner: &FunctionSummary) -> Option<AbsorbingTail> {
    if probe.family != ProbeFamily::Predicate
        || probe.owner.as_ref() != Some(&owner.id)
        || !probe.location.file.ends_with(&owner.file)
    {
        return None;
    }
    let code = |text: &str| {
        crate::analysis::language::mask_rust_comments_and_strings(text)
            .trim()
            .to_string()
    };
    let parse = parse_clean_source_file(&owner.body)?;
    let root = parse.tree();
    // The owner is the one function and the one item: a nested `fn`, `impl`,
    // `trait`, `use` or `mod` could provide the method or rename the type.
    if root
        .syntax()
        .descendants()
        .filter(|node| ast::Item::can_cast(node.kind()))
        .count()
        != 1
    {
        return None;
    }
    let function = root.syntax().descendants().find_map(ast::Fn::cast)?;
    if function.generic_param_list().is_some() {
        return None;
    }
    let primitive = match function.ret_type()?.ty()? {
        ast::Type::PathType(path) if path.path()?.qualifier().is_none() => {
            let segment = path.path()?.segment()?;
            if segment.generic_arg_list().is_some() {
                return None;
            }
            segment.name_ref()?.text().to_string()
        }
        _ => return None,
    };
    let unsigned = UNSIGNED.contains(&primitive.as_str());
    if !unsigned && !SIGNED.contains(&primitive.as_str()) {
        return None;
    }
    let list = function.body()?.stmt_list()?;
    let ast::Expr::MethodCallExpr(call) = list.tail_expr()? else {
        return None;
    };
    if call.generic_arg_list().is_some() {
        return None;
    }
    let args: Vec<ast::Expr> = call.arg_list()?.args().collect();
    let literal = |expr: &ast::Expr| match expr {
        ast::Expr::Literal(literal) if matches!(literal.kind(), ast::LiteralKind::IntNumber(_)) => {
            let text = literal.syntax().text().to_string();
            Some(
                text.strip_suffix(primitive.as_str())
                    .unwrap_or(&text)
                    .to_string(),
            )
        }
        _ => None,
    };
    // `min(0)` on an unsigned type is always 0; `clamp(L, L)` is always L on
    // any integer type. `min(0)` on a signed type keeps negative receivers.
    let method = call.name_ref()?.text().to_string();
    match (method.as_str(), args.as_slice()) {
        ("min", [bound]) if unsigned && literal(bound).as_deref() == Some("0") => {}
        ("clamp", [low, high]) if literal(low).is_some() && literal(low) == literal(high) => {}
        _ => return None,
    }
    let ast::Expr::PathExpr(receiver) = call.receiver()? else {
        return None;
    };
    let receiver_path = receiver.path()?;
    if receiver_path.qualifier().is_some() {
        return None;
    }
    let name = receiver_path.segment()?.name_ref()?.text().to_string();
    if name.starts_with("r#") {
        return None;
    }
    // Straight-line `let` statements only: no early `return`, `?`, macro or
    // loop that the changed condition could steer instead.
    let mut chosen: Option<ast::LetStmt> = None;
    for statement in list.statements() {
        let ast::Stmt::LetStmt(binding) = statement else {
            return None;
        };
        if binding.let_else().is_some() {
            return None;
        }
        let ast::Pat::IdentPat(ident) = binding.pat()? else {
            return None;
        };
        if ident.ref_token().is_some() || ident.at_token().is_some() {
            return None;
        }
        if *ident.name()?.text() == *name && chosen.replace(binding).is_some() {
            return None;
        }
    }
    let binding = chosen?;
    let ast::Expr::IfExpr(branch) = binding.initializer()? else {
        return None;
    };
    // The changed condition is the whole `if` condition, once, on the probe's
    // line, between two integer-literal blocks.
    let condition = branch.condition()?;
    if code(&condition.syntax().text().to_string()) != code(&probe.expression)
        || !matches!(&condition, ast::Expr::BinExpr(binary)
            if matches!(binary.op_kind(), Some(ast::BinaryOp::CmpOp(_))))
    {
        return None;
    }
    let offset = probe.location.line.checked_sub(owner.start_line)?;
    let before = owner
        .body
        .get(..usize::from(condition.syntax().text_range().start()))?;
    if before.bytes().filter(|byte| *byte == b'\n').count() != offset
        || code(&owner.body)
            .matches(code(&probe.expression).as_str())
            .count()
            != 1
    {
        return None;
    }
    let literal_block = |block: ast::BlockExpr| {
        let list = block.stmt_list()?;
        let plain = list.statements().next().is_none();
        let integer = matches!(
            list.tail_expr()?,
            ast::Expr::Literal(literal)
                if matches!(literal.kind(), ast::LiteralKind::IntNumber(_))
        );
        (plain && integer).then_some(())
    };
    literal_block(branch.then_branch()?)?;
    match branch.else_branch()? {
        ast::ElseBranch::Block(block) => literal_block(block)?,
        ast::ElseBranch::IfExpr(_) => return None,
    }
    // The receiver is read once, by the tail: any other occurrence of its name
    // (a second use, a shadowing rebind, a macro or closure capture) may let
    // the chosen value escape or panic differently.
    if whole_word_count(&owner.body, &name) != 2 {
        return None;
    }
    Some(AbsorbingTail {
        call: call.syntax().text().to_string(),
        primitive,
    })
}

fn whole_word_count(text: &str, word: &str) -> usize {
    let bytes = text.as_bytes();
    let ident = |byte: u8| byte.is_ascii_alphanumeric() || byte == b'_';
    text.match_indices(word)
        .filter(|(start, found)| {
            let before = *start == 0 || bytes.get(start - 1).is_none_or(|byte| !ident(*byte));
            let after = bytes
                .get(start + found.len())
                .is_none_or(|byte| !ident(*byte));
            before && after
        })
        .count()
}

/// The workspace half of the rule: true only when every indexed source leaves
/// std's `Ord` dispatch for `primitive` unreplaceable. Any doubt answers
/// `false`.
fn workspace_keeps_std_dispatch<'a>(
    sources: impl IntoIterator<Item = &'a str>,
    primitive: &str,
) -> bool {
    sources.into_iter().all(|source| {
        !(source.contains("no_implicit_prelude")
            || source.contains("no_core")
            || source.contains("include!")
            || source.contains("r#min")
            || source.contains("r#clamp")
            || source.contains(&format!("r#{primitive}"))
            || word_follows(source, "fn", |next| next == "min" || next == "clamp")
            || ["type", "struct", "enum", "union", "trait", "mod"]
                .iter()
                .any(|keyword| word_follows(source, keyword, |next| next == primitive))
            || use_renames_to(source, primitive)
            || macro_generates_items(source))
    })
}

/// Whether a whole-word `first` is followed, after whitespace, by an
/// identifier `accept` takes.
fn word_follows(source: &str, first: &str, accept: impl Fn(&str) -> bool) -> bool {
    let bytes = source.as_bytes();
    let ident = |byte: u8| byte.is_ascii_alphanumeric() || byte == b'_';
    source.match_indices(first).any(|(start, found)| {
        let before = start == 0 || bytes.get(start - 1).is_none_or(|byte| !ident(*byte));
        let rest = &source[start + found.len()..];
        let trimmed = rest.trim_start();
        let spaced = trimmed.len() < rest.len();
        let word_end = trimmed
            .find(|character: char| !(character.is_ascii_alphanumeric() || character == '_'))
            .unwrap_or(trimmed.len());
        before && spaced && word_end > 0 && accept(&trimmed[..word_end])
    })
}

/// Whether a `use` or `extern crate` statement renames something to
/// `primitive` (`use other::Thing as u32;`, `extern crate other as u32;`). A
/// cast `x as u32` outside those statements is not a rename.
fn use_renames_to(source: &str, primitive: &str) -> bool {
    let bytes = source.as_bytes();
    let ident = |byte: u8| byte.is_ascii_alphanumeric() || byte == b'_';
    ["use", "extern"].iter().any(|keyword| {
        source.match_indices(*keyword).any(|(start, found)| {
            let before = start == 0 || bytes.get(start - 1).is_none_or(|byte| !ident(*byte));
            let after = bytes
                .get(start + found.len())
                .is_some_and(|byte| !ident(*byte));
            if !(before && after) {
                return false;
            }
            let statement = source[start..].split(';').next().unwrap_or_default();
            word_follows(statement, "as", |next| next == primitive)
        })
    })
}

/// Whether a workspace macro can declare an item whose name it computes
/// (`fn $name`, `type $name`, ...), which could define `min`, `clamp` or the
/// primitive's name out of sight of the plain-text scans above.
fn macro_generates_items(source: &str) -> bool {
    let bytes = source.as_bytes();
    let ident = |byte: u8| byte.is_ascii_alphanumeric() || byte == b'_';
    source.contains("macro_rules")
        && ["fn", "type", "struct", "enum", "union", "trait", "mod"]
            .iter()
            .any(|keyword| {
                source.match_indices(*keyword).any(|(start, found)| {
                    let before =
                        start == 0 || bytes.get(start - 1).is_none_or(|byte| !ident(*byte));
                    before && source[start + found.len()..].trim_start().starts_with('$')
                })
            })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::facts::FunctionSourceRole;
    use std::path::PathBuf;

    fn owner(body: &str) -> FunctionSummary {
        FunctionSummary {
            id: SymbolId("src/lib.rs::f".to_string()),
            name: "f".to_string(),
            file: PathBuf::from("src/lib.rs"),
            start_line: 1,
            end_line: body.lines().count(),
            body: body.into(),
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

    fn predicate(expression: &str, line: usize, owner: &FunctionSummary) -> Probe {
        Probe {
            id: ProbeId("probe:src_lib.rs:predicate:test".to_string()),
            location: SourceLocation::new("src/lib.rs", line, 1),
            owner: Some(owner.id.clone()),
            family: ProbeFamily::Predicate,
            delta: DeltaKind::Control,
            before: None,
            after: None,
            expression: expression.to_string(),
            expected_sinks: Vec::new(),
            required_oracles: Vec::new(),
        }
    }

    fn shape(body: &str) -> Option<AbsorbingTail> {
        let owner = owner(body);
        absorbing_tail_shape(&predicate("n == 5", 2, &owner), &owner)
    }

    const CLAMPED: &str =
        "pub fn f(n: u32) -> u32 {\n    let r: u32 = if n == 5 { 1 } else { 0 };\n    r.min(0)\n}";

    /// The issue's shape and its exact absorbing siblings are recognized.
    #[test]
    fn literal_min_and_clamp_over_a_literal_choice_are_absorbed() {
        for (body, call, primitive) in [
            (CLAMPED, "r.min(0)", "u32"),
            (
                "pub fn f(n: u32) -> u32 {\n    let r: u32 = if n == 5 { 1 } else { 0 };\n    r.min(0u32)\n}",
                "r.min(0u32)",
                "u32",
            ),
            (
                "pub fn f(n: u8) -> usize {\n    let r: usize = if n == 5 { 3 } else { 4 };\n    r.min(0)\n}",
                "r.min(0)",
                "usize",
            ),
            (
                "pub fn f(n: i32) -> i32 {\n    let r: i32 = if n == 5 { 1 } else { 0 };\n    r.clamp(2, 2)\n}",
                "r.clamp(2, 2)",
                "i32",
            ),
        ] {
            let found = shape(body);
            assert_eq!(
                found,
                Some(AbsorbingTail {
                    call: call.to_string(),
                    primitive: primitive.to_string()
                }),
                "{body}"
            );
        }
        let owner = owner(CLAMPED);
        let probe = predicate("n == 5", 2, &owner);
        let found = shape(CLAMPED);
        assert!(found.is_some());
        if let Some(found) = found {
            let sink = sink_for(&probe, &owner, &found);
            assert_eq!(sink.kind, FlowSinkKind::Unknown);
            assert_eq!(sink.line, 2);
        }
    }

    /// The parked witness: a non-literal operand keeps the choice observable
    /// (`n / 6` is 1 at `n == 6`), so it must never be absorbed.
    #[test]
    fn non_literal_operand_is_not_absorbed() {
        for body in [
            "pub fn f(n: u32) -> u32 {\n    let cap = n / 6;\n    let r: u32 = if n == 5 { 1 } else { 0 };\n    r.min(cap)\n}",
            "pub fn f(n: u32) -> u32 {\n    let r: u32 = if n == 5 { 1 } else { 0 };\n    r.min(n)\n}",
            "pub fn f(n: u32) -> u32 {\n    let r: u32 = if n == 5 { 1 } else { 0 };\n    r.clamp(0, n)\n}",
            "pub fn f(n: u32) -> u32 {\n    let r: u32 = if n == 5 { 1 } else { 0 };\n    r.clamp(0, 1)\n}",
            "pub fn f(n: u32) -> u32 {\n    let r: u32 = if n == 5 { 1 } else { 0 };\n    r.min(1)\n}",
        ] {
            assert!(shape(body).is_none(), "{body}");
        }
    }

    /// `min(0)` only absorbs an unsigned receiver; the return type must be a
    /// plain primitive integer.
    #[test]
    fn signed_or_non_primitive_types_are_not_absorbed() {
        for body in [
            "pub fn f(n: i32) -> i32 {\n    let r: i32 = if n == 5 { 1 } else { -1 };\n    r.min(0)\n}",
            "pub fn f(n: u32) -> Count {\n    let r: Count = if n == 5 { Count(1) } else { Count(0) };\n    r.min(0)\n}",
            "pub fn f(n: u32) -> f64 {\n    let r: f64 = if n == 5 { 1.0 } else { 0.0 };\n    r.min(0.0)\n}",
            "pub fn f(n: u32) -> std::num::Wrapping<u32> {\n    let r = if n == 5 { 1 } else { 0 };\n    r.min(0)\n}",
            "pub fn f<u32>(n: u32) -> u32 {\n    let r: u32 = if n == 5 { 1 } else { 0 };\n    r.min(0)\n}",
        ] {
            assert!(shape(body).is_none(), "{body}");
        }
    }

    /// Anything beyond one straight-line `let` choice and one tail keeps the
    /// ordinary sink: a second read, a shadow, a control-flow statement, a
    /// nested item, a non-literal arm or an `else if` chain.
    #[test]
    fn other_body_shapes_are_not_absorbed() {
        for body in [
            "pub fn f(n: u32) -> u32 {\n    let r: u32 = if n == 5 { 1 } else { 0 };\n    let s = r + 1;\n    r.min(0)\n}",
            "pub fn f(n: u32) -> u32 {\n    let r: u32 = if n == 5 { 1 } else { 0 };\n    let r = 9;\n    r.min(0)\n}",
            "pub fn f(n: u32) -> u32 {\n    let r: u32 = if n == 5 { 1 } else { 0 };\n    assert!(r < 9);\n    r.min(0)\n}",
            "pub fn f(n: u32) -> u32 {\n    let r: u32 = if n == 5 { return 3 } else { 0 };\n    r.min(0)\n}",
            "pub fn f(n: u32) -> u32 {\n    let r: u32 = if n == 5 { n } else { 0 };\n    r.min(0)\n}",
            "pub fn f(n: u32) -> u32 {\n    let r: u32 = if n == 5 { 1 } else if n == 7 { 2 } else { 0 };\n    r.min(0)\n}",
            "pub fn f(n: u32) -> u32 {\n    fn min(self_: u32) -> u32 { self_ }\n    let r: u32 = if n == 5 { 1 } else { 0 };\n    r.min(0)\n}",
            "pub fn f(n: u32) -> u32 {\n    let r: u32 = if n == 5 { 1 } else { 0 };\n    (r).min(0)\n}",
            "pub fn f(n: u32) -> u32 {\n    let r: u32 = if n == 5 { 1 } else { 0 };\n    r.min::<u32>(0)\n}",
        ] {
            assert!(shape(body).is_none(), "{body}");
        }
        // The probe must be the `if` condition on its own line.
        let owner = owner(CLAMPED);
        assert!(absorbing_tail_shape(&predicate("n == 5", 3, &owner), &owner).is_none());
        assert!(absorbing_tail_shape(&predicate("n == 6", 2, &owner), &owner).is_none());
        let mut other = predicate("n == 5", 2, &owner);
        other.family = ProbeFamily::ReturnValue;
        assert!(absorbing_tail_shape(&other, &owner).is_none());
    }

    /// The custom-dispatch controls: a workspace that can replace std's `min`
    /// or `clamp`, or the primitive's name, is never absorbed. Verified with a
    /// real `#![no_implicit_prelude]` crate whose `min` returns the receiver.
    #[test]
    fn a_workspace_that_can_replace_std_dispatch_is_not_absorbed() {
        let plain = "pub fn other(a: u32) -> u32 { a as u32 }\n";
        assert!(workspace_keeps_std_dispatch([plain, CLAMPED], "u32"));
        for hostile in [
            "#![no_implicit_prelude]\npub trait Ord2 { fn min(self, o: Self) -> Self; }",
            "#![no_core]\n",
            "pub trait Floor { fn min(self, o: u32) -> u32; }",
            "impl Floor for u32 {\n    fn  clamp(self, a: u32, b: u32) -> u32 { self }\n}",
            "include!(concat!(env!(\"OUT_DIR\"), \"/gen.rs\"));",
            "type u32 = Wide;",
            "struct u32;",
            "mod u32 {}",
            "use other::Wide as u32;",
            "use other::{Wide as u32, Other};",
            "pub fn f() { let r#min = 1; }",
            "macro_rules! m { ($n:ident) => { fn $n(self) {} }; }",
            "macro_rules! m { ($n:ident) => { type  $n = Wide; }; }",
            "extern crate other as u32;",
        ] {
            assert!(
                !workspace_keeps_std_dispatch([plain, CLAMPED, hostile], "u32"),
                "{hostile}"
            );
        }
        // A different primitive's rename does not matter here.
        assert!(workspace_keeps_std_dispatch(
            [plain, "type u64 = Wide;"],
            "u32"
        ));
        // A macro that computes no item name does not refuse.
        assert!(workspace_keeps_std_dispatch(
            ["macro_rules! one { () => { 1 } }"],
            "u32"
        ));
        // A cast is not a rename.
        assert!(workspace_keeps_std_dispatch(
            ["use std::fmt;\nfn c(x: u64) -> u32 { x as u32 }"],
            "u32"
        ));
    }
}
