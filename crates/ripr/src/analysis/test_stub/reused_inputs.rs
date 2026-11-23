//! #8361: the other inputs of a boundary stub, copied from an existing test.
//!
//! A boundary stub writes the inputs the changed comparison reads (`rate_bps =
//! 10_000` for `rate_bps >= 10_000`). The owner's other parameters used to
//! stay `todo!("ripr: choose `amount`")` even when an existing test in the
//! same inline module already calls the owner with literals. This module
//! copies those literals for the parameters the boundary leaves open:
//!
//! - the owner is a free function (no receiver, not in an `impl` or trait),
//!   and the stub goes into an existing inline test module, where an existing
//!   test has already compiled the same call;
//! - the call is the first one in that module, in source order, to the
//!   owner's bare name with exactly one literal argument per parameter
//!   (`5`, `-3`, `"w"`, `true`, `1.5`), either in a test's own statements or
//!   as a direct argument of `assert!`, `assert_eq!` or `assert_ne!`;
//! - it sits in a `#[test]` function that is not `should_panic` and not a
//!   ripr stub, outside closures, `async` blocks and `cfg`;
//! - nothing in the module declares, imports or renames another item or
//!   binding of the owner's name;
//! - the module calls only standard macros (`assert_eq!`, `vec!`,
//!   `println!` and the like), since any other macro or `include!` may emit
//!   a function of the owner's name, and the file defines, imports or
//!   `#[macro_use]`s no macro of a standard macro's name;
//! - each copied parameter's type is not `impl Trait` or `&mut`.
//!
//! The copied values are setup, not evidence: the stub says which test they
//! came from, and the expected value stays a `todo!()`.

use super::StubScope;
use super::receiver::{literal_argument, proven_test_call, scope_shadows};
use crate::analysis::syntax::fn_signature::OwnerSignature;
use crate::analysis::syntax::parse_clean_source_file;
use ra_ap_syntax::ast::{self, HasArgList, HasGenericArgs, HasName};
use ra_ap_syntax::{AstNode, SourceFile, SyntaxKind, SyntaxNode, TextSize};

/// The assertion macros whose arguments a passing test always evaluates.
const ASSERTIONS: [&str; 3] = ["assert", "assert_eq", "assert_ne"];

/// Standard macros a test module may call without blocking the copy; none
/// can emit an item.
const STANDARD_MACROS: [&str; 20] = [
    "assert",
    "assert_eq",
    "assert_ne",
    "debug_assert",
    "debug_assert_eq",
    "debug_assert_ne",
    "dbg",
    "eprint",
    "eprintln",
    "format",
    "matches",
    "panic",
    "print",
    "println",
    "todo",
    "unimplemented",
    "unreachable",
    "vec",
    "write",
    "writeln",
];

/// Literals for the open parameters, and the test they came from.
#[derive(Debug, PartialEq)]
pub(super) struct ReusedInputs {
    /// `(binding, literal)` for each open parameter, in parameter order.
    pub(super) values: Vec<(String, String)>,
    /// The existing test whose call supplied them.
    pub(super) test: String,
}

/// Literals from an existing test's call for each binding in `open`, the
/// parameters the boundary left unwritten. `bindings` names every parameter
/// in order, as the stub spells it.
pub(super) fn reused_inputs(
    signature: &OwnerSignature,
    bindings: &[String],
    open: &[String],
    scope: &StubScope<'_>,
) -> Option<ReusedInputs> {
    if open.is_empty() || scope.range.is_empty() || bindings.len() != signature.params.len() {
        return None;
    }
    let name = signature.name.as_str();
    // Only plain types take a literal argument as is.
    let copyable = signature
        .params
        .iter()
        .zip(bindings)
        .all(|(param, binding)| {
            !open.contains(binding) || {
                let ty = param.ty.split_whitespace().collect::<String>();
                let generic_bound = param
                    .ty
                    .split(|c: char| !(c.is_alphanumeric() || c == '_'))
                    .any(|word| word == "impl");
                !generic_bound && !ty.starts_with("&mut")
            }
        });
    if !copyable || scope_shadows(scope, name) {
        return None;
    }
    let parse = parse_clean_source_file(scope.parse_unit)?;
    let start = TextSize::try_from(scope.range.start).ok()?;
    let end = TextSize::try_from(scope.range.end).ok()?;
    let in_scope = |node: &SyntaxNode| {
        let range = node.text_range();
        range.start() >= start && range.end() <= end
    };
    let file = parse.tree();
    // A `let`, parameter, `fn`, `const` or `static` of the owner's name in
    // the module would take the call.
    let rebinds = file
        .syntax()
        .descendants()
        .filter(|node| in_scope(node))
        .filter_map(ast::Name::cast)
        .any(|bound| bound.text() == name);
    if rebinds || !standard_macros_only(&file, &in_scope) {
        return None;
    }
    let (arguments, test) = file
        .syntax()
        .descendants()
        .filter(|node| in_scope(node))
        .find_map(|node| {
            let arguments = match node.kind() {
                SyntaxKind::CALL_EXPR => owner_call(&ast::CallExpr::cast(node.clone())?, name),
                SyntaxKind::MACRO_CALL => asserted_call(&ast::MacroCall::cast(node.clone())?, name),
                _ => None,
            }?;
            if arguments.len() != bindings.len() {
                return None;
            }
            Some((arguments, proven_test_call(&node, start, end)?))
        })?;
    let values = bindings
        .iter()
        .zip(arguments)
        .filter(|(binding, _)| open.contains(binding))
        .map(|(binding, argument)| (binding.clone(), argument))
        .collect();
    Some(ReusedInputs { values, test })
}

/// Whether every macro the module calls is a standard one this file does not
/// replace. Any other macro (or `include!`) may emit a function of the
/// owner's name, and a file-local `macro_rules!`, a `use` or `#[macro_use]`
/// may replace `assert_eq!` with one that never type-checks its arguments.
fn standard_macros_only(file: &SourceFile, in_scope: &impl Fn(&SyntaxNode) -> bool) -> bool {
    let standard = |name: &str| STANDARD_MACROS.contains(&name);
    let calls_standard = file
        .syntax()
        .descendants()
        .filter(|node| in_scope(node))
        .filter_map(ast::MacroCall::cast)
        .all(|call| {
            call.path().is_some_and(|path| {
                path.qualifier().is_none()
                    && path
                        .segment()
                        .and_then(|segment| segment.name_ref())
                        .is_some_and(|called| standard(&called.text()))
            })
        });
    let replaced = file.syntax().descendants().any(|node| {
        if matches!(node.kind(), SyntaxKind::MACRO_RULES | SyntaxKind::MACRO_DEF) {
            return node
                .children()
                .find_map(ast::Name::cast)
                .is_some_and(|defined| standard(&defined.text()));
        }
        if let Some(rename) = ast::Rename::cast(node.clone()) {
            return rename
                .name()
                .is_some_and(|renamed| standard(&renamed.text()));
        }
        if let Some(tree) = ast::UseTree::cast(node.clone()) {
            return tree
                .path()
                .and_then(|path| path.segment())
                .and_then(|segment| segment.name_ref())
                .is_some_and(|imported| standard(&imported.text()));
        }
        ast::Attr::cast(node).is_some_and(|attr| {
            attr.meta()
                .and_then(|meta| meta.simple_name())
                .is_some_and(|attr_name| attr_name == "macro_use")
        })
    });
    calls_standard && !replaced
}

/// The literal arguments of `name(..)`, a call through the bare name.
fn owner_call(call: &ast::CallExpr, name: &str) -> Option<Vec<String>> {
    let ast::Expr::PathExpr(callee) = call.expr()? else {
        return None;
    };
    let path = callee.path()?;
    let segment = path.segment()?;
    if path.qualifier().is_some()
        || segment.generic_arg_list().is_some()
        || segment.name_ref()?.text() != name
    {
        return None;
    }
    call.arg_list()?
        .args()
        .map(|arg| literal_argument(&arg))
        .collect()
}

/// The literal arguments of a call to `name` written directly as an
/// argument of `assert!`, `assert_eq!` or `assert_ne!`.
fn asserted_call(call: &ast::MacroCall, name: &str) -> Option<Vec<String>> {
    let path = call.path()?;
    let macro_name = path.segment()?.name_ref()?.text().to_string();
    if path.qualifier().is_some() || !ASSERTIONS.contains(&macro_name.as_str()) {
        return None;
    }
    let tree = call.token_tree()?;
    let text = tree.syntax().text().to_string();
    let inner = text.strip_prefix('(')?.strip_suffix(')')?;
    // The arguments, read as one tuple expression.
    let parsed = parse_clean_source_file(&format!("fn ripr_arguments() {{ let _ = ({inner}); }}"))?;
    let tuple = parsed
        .tree()
        .syntax()
        .descendants()
        .find_map(ast::LetStmt::cast)?
        .initializer()?;
    let arguments: Vec<ast::Expr> = match tuple {
        ast::Expr::TupleExpr(tuple) => tuple.fields().collect(),
        ast::Expr::ParenExpr(paren) => vec![paren.expr()?],
        _ => return None,
    };
    arguments.iter().find_map(|argument| match argument {
        ast::Expr::CallExpr(call) => owner_call(call, name),
        _ => None,
    })
}
