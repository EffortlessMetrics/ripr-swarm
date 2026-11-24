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
//! - the call is the first one in that module, in source order, through the
//!   owner's bare name with exactly one literal argument per parameter
//!   (`5`, `-3`, `"w"`, `true`, `1.5`), authored at the top level of the
//!   test's own body: a statement or `let` initializer, or an asserted
//!   operand (not a message argument) of a statement `assert!`, `assert_eq!`
//!   or `assert_ne!`, with no `return` or `?` before it;
//! - it sits in a `#[test]` function that is not `should_panic`, `ignore` or
//!   a ripr stub and has only plain attributes, outside closures, `async`
//!   blocks and `cfg`, and the module (inside or on it) carries no attribute
//!   or derive macro;
//! - nothing in the module declares, imports or renames another item or
//!   binding of the owner's name;
//! - the module calls only standard macros (`assert_eq!`, `vec!`,
//!   `println!` and the like), since any other macro or `include!` may emit
//!   a function of the owner's name, and the file defines, imports or
//!   `#[macro_use]`s no macro of a standard macro's name;
//! - each copied literal fits its parameter's declared type as written, so
//!   the stub compiles even if a macro this file cannot see changed what the
//!   test's call meant;
//! - the copied values do not steer the owner away from the statement
//!   holding the changed comparison (see `reaches_seam`); an earlier guard
//!   on the written inputs alone is outside this check.
//!
//! The copied values are setup, not evidence: the stub says which test they
//! came from, and the expected value stays a `todo!()`.

use super::StubScope;
use super::expected_value::condition_value;
use super::match_inputs::{parse_integer, unraw};
use super::receiver::{literal_argument, proven_test_call, scope_shadows};
use crate::analysis::syntax::fn_signature::OwnerSignature;
use crate::analysis::syntax::parse_clean_source_file;
use ra_ap_syntax::ast::{self, HasArgList, HasAttrs, HasGenericArgs, HasName};
use ra_ap_syntax::{AstNode, SourceFile, SyntaxKind, SyntaxNode, TextSize};

/// The assertion macros whose asserted operands are evaluated on every pass.
const ASSERTIONS: [&str; 3] = ["assert", "assert_eq", "assert_ne"];

/// Attributes a copied-from test may carry: none rewrites or skips its body.
const PLAIN_TEST_ATTRIBUTES: [&str; 6] = ["test", "allow", "expect", "warn", "deny", "doc"];

/// Attributes any item in the test module may carry without blocking the
/// copy: the plain test attributes, `cfg`, `should_panic`, `ignore`, and
/// `derive` of standard traits. An attribute or derive macro of any other
/// name may emit a function of the owner's name.
const MODULE_ATTRIBUTES: [&str; 4] = ["cfg", "should_panic", "ignore", "derive"];

/// The primitive integer types.
const INTEGER_TYPES: [&str; 12] = [
    "i8", "i16", "i32", "i64", "i128", "isize", "u8", "u16", "u32", "u64", "u128", "usize",
];

/// The standard derives, which emit only trait impls.
const STANDARD_DERIVES: [&str; 9] = [
    "Clone",
    "Copy",
    "Debug",
    "Default",
    "Eq",
    "Hash",
    "Ord",
    "PartialEq",
    "PartialOrd",
];

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
    written: &[(String, String)],
    seam_offset: usize,
    scope: &StubScope<'_>,
) -> Option<ReusedInputs> {
    if open.is_empty() || scope.range.is_empty() || bindings.len() != signature.params.len() {
        return None;
    }
    let name = signature.name.as_str();
    if scope_shadows(scope, name) {
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
    let owner = owner_fn(&file, seam_offset, name)?;
    // A `let`, parameter, `fn`, `const` or `static` of the owner's name in
    // the module would take the call.
    let rebinds = file
        .syntax()
        .descendants()
        .filter(|node| in_scope(node))
        .filter_map(ast::Name::cast)
        .any(|bound| unraw(bound.text()) == unraw(name));
    // With `overflowing_literals` allowed, a literal may wrap, so neither a
    // copied value nor a guard's comparison reads as written.
    let wraps = file
        .syntax()
        .text()
        .to_string()
        .contains("overflowing_literals");
    // A `mut` parameter may be reassigned before the comparison.
    let reassignable = owner.param_list().is_some_and(|params| {
        params.params().any(|param| match param.pat() {
            Some(ast::Pat::IdentPat(ident)) => ident.mut_token().is_some(),
            _ => true,
        })
    });
    if rebinds || wraps || reassignable || !standard_macros_only(&file, &in_scope, (start, end)) {
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
            if arguments.len() != bindings.len() || !runs_in_test_body(&node) {
                return None;
            }
            let test = proven_test_call(&node, start, end)?;
            let function = node.ancestors().find_map(ast::Fn::cast)?;
            // A `return` or `?` earlier in the test may leave it first.
            // A `?` inside a macro's tokens is only a token, so count those too.
            let leaves_first = function.syntax().descendants_with_tokens().any(|inner| {
                matches!(
                    inner.kind(),
                    SyntaxKind::RETURN_EXPR | SyntaxKind::TRY_EXPR | SyntaxKind::QUESTION
                ) && inner.text_range().end() <= node.text_range().start()
            });
            // An earlier operand of the assertion may hold one in its tokens.
            let exits_inside = ast::MacroCall::cast(node.clone())
                .and_then(|call| call.token_tree())
                .is_some_and(|tree| {
                    tree.syntax().descendants_with_tokens().any(|inner| {
                        matches!(inner.kind(), SyntaxKind::RETURN_KW | SyntaxKind::QUESTION)
                    })
                });
            if leaves_first || exits_inside {
                return None;
            }
            // An ignored test never ran the call, and an attribute macro may
            // rewrite or drop the body, so only plain test attributes count.
            if !function.attrs().all(|attr| {
                attr.meta()
                    .and_then(|meta| meta.simple_name())
                    .is_some_and(|attr_name| PLAIN_TEST_ATTRIBUTES.contains(&attr_name.as_str()))
            }) {
                return None;
            }
            // Each copied literal must fit its parameter's declared type.
            let mut values = Vec::new();
            for ((binding, argument), param) in
                bindings.iter().zip(arguments).zip(&signature.params)
            {
                if open.contains(binding) {
                    if !literal_fits(&param.ty, &argument) {
                        return None;
                    }
                    values.push((binding.clone(), argument));
                }
            }
            // The copied values must not steer the owner away from the
            // changed comparison.
            let mut inputs = written.to_vec();
            inputs.extend(values.iter().cloned());
            let copied: Vec<String> = values.iter().map(|(binding, _)| binding.clone()).collect();
            let typed: Vec<(String, String)> = bindings
                .iter()
                .zip(&signature.params)
                .map(|(binding, param)| (binding.clone(), param.ty.trim().to_string()))
                .collect();
            if !reaches_seam(&file, &owner, seam_offset, &copied, &inputs, &typed) {
                return None;
            }
            Some((values, test))
        })?;
    Some(ReusedInputs {
        values: arguments,
        test,
    })
}

/// Whether every macro the module calls is a standard one this file does not
/// replace. Any other macro (or `include!`) may emit a function of the
/// owner's name, and a file-local `macro_rules!`, a `use` or `#[macro_use]`
/// may replace `assert_eq!` with one that never type-checks its arguments.
fn standard_macros_only(
    file: &SourceFile,
    in_scope: &impl Fn(&SyntaxNode) -> bool,
    (scope_start, scope_end): (TextSize, TextSize),
) -> bool {
    let standard = |name: &str| STANDARD_MACROS.contains(&unraw(name));
    // An import or rename may also replace a standard derive or attribute
    // such as `#[test]`.
    let replaceable = |name: &str| {
        let name = unraw(name);
        standard(name)
            || STANDARD_DERIVES.contains(&name)
            || PLAIN_TEST_ATTRIBUTES.contains(&name)
            || MODULE_ATTRIBUTES.contains(&name)
    };
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
                        .is_some_and(|called| standard(called.text()))
            })
        });
    let replaced = file.syntax().descendants().any(|node| {
        if matches!(node.kind(), SyntaxKind::MACRO_RULES | SyntaxKind::MACRO_DEF) {
            return node
                .children()
                .find_map(ast::Name::cast)
                .is_some_and(|defined| standard(defined.text()));
        }
        if let Some(rename) = ast::Rename::cast(node.clone()) {
            return rename
                .name()
                .is_some_and(|renamed| replaceable(renamed.text()));
        }
        if let Some(tree) = ast::UseTree::cast(node.clone()) {
            return tree
                .path()
                .and_then(|path| path.segment())
                .and_then(|segment| segment.name_ref())
                .is_some_and(|imported| replaceable(imported.text()));
        }
        ast::Attr::cast(node).is_some_and(|attr| {
            attr.meta()
                .and_then(|meta| meta.simple_name())
                .is_some_and(|attr_name| attr_name == "macro_use")
        })
    });
    // The test module's own outer attributes sit outside its body, so read
    // them from the innermost module whose body holds the whole scope.
    let module_attributes = file
        .syntax()
        .descendants()
        .filter_map(ast::Module::cast)
        .filter(|module| {
            module.item_list().is_some_and(|items| {
                let range = items.syntax().text_range();
                range.start() < scope_start && scope_end <= range.end()
            })
        })
        .max_by_key(|module| module.syntax().text_range().start())
        .into_iter()
        .flat_map(|module| module.attrs().collect::<Vec<_>>());
    let plain_attributes = file
        .syntax()
        .descendants()
        .filter(|node| in_scope(node))
        .filter_map(ast::Attr::cast)
        .chain(module_attributes)
        .all(|attr| {
            let Some(meta) = attr.meta() else {
                return false;
            };
            let Some(attr_name) = meta.simple_name() else {
                return false;
            };
            if attr_name == "derive" {
                // Every derived name after `derive` itself.
                return meta
                    .syntax()
                    .descendants_with_tokens()
                    .filter_map(|element| element.into_token())
                    .filter(|token| token.kind() == SyntaxKind::IDENT)
                    .skip(1)
                    .all(|token| STANDARD_DERIVES.contains(&token.text()));
            }
            PLAIN_TEST_ATTRIBUTES.contains(&attr_name.as_str())
                || MODULE_ATTRIBUTES.contains(&attr_name.as_str())
        });
    calls_standard && !replaced && plain_attributes
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
        || unraw(segment.name_ref()?.text()) != unraw(name)
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
    let inner = text
        .strip_prefix('(')
        .and_then(|rest| rest.strip_suffix(')'))
        .or_else(|| {
            text.strip_prefix('[')
                .and_then(|rest| rest.strip_suffix(']'))
        })
        .or_else(|| {
            text.strip_prefix('{')
                .and_then(|rest| rest.strip_suffix('}'))
        })?;
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
    // Only the asserted operands always run; the message arguments are
    // formatted only when the assertion fails.
    let operands = if macro_name == "assert" { 1 } else { 2 };
    arguments
        .iter()
        .take(operands)
        .find_map(|argument| match argument {
            ast::Expr::CallExpr(call) => owner_call(call, name),
            _ => None,
        })
}

/// The innermost function around the seam, when it is the owner.
fn owner_fn(file: &SourceFile, seam_offset: usize, name: &str) -> Option<ast::Fn> {
    let offset = TextSize::try_from(seam_offset).ok()?;
    let owner = file
        .syntax()
        .descendants()
        .filter_map(ast::Fn::cast)
        .filter(|function| function.syntax().text_range().contains(offset))
        .max_by_key(|function| function.syntax().text_range().start())?;
    (unraw(owner.name()?.text()) == unraw(name)).then_some(owner)
}

/// Whether `node` is authored at the top level of its test's own body: a
/// statement there, the initializer of one of its `let`s, or (for a macro) a
/// statement macro there. A call under `if`, `&&`, a loop or a nested block
/// may never run.
fn runs_in_test_body(node: &SyntaxNode) -> bool {
    let Some(parent) = node.parent() else {
        return false;
    };
    let statement = match parent.kind() {
        SyntaxKind::EXPR_STMT | SyntaxKind::LET_STMT => parent,
        SyntaxKind::STMT_LIST => node.clone(),
        SyntaxKind::MACRO_STMTS | SyntaxKind::MACRO_EXPR => {
            let Some(grand) = parent.parent() else {
                return false;
            };
            if matches!(grand.kind(), SyntaxKind::EXPR_STMT | SyntaxKind::LET_STMT) {
                grand
            } else if grand.kind() == SyntaxKind::STMT_LIST {
                parent
            } else {
                return false;
            }
        }
        _ => return false,
    };
    if let Some(statement_let) = ast::LetStmt::cast(statement.clone())
        && statement_let.initializer().is_none_or(|init| {
            init.syntax().text_range() != node.text_range()
                && init
                    .syntax()
                    .first_child()
                    .is_none_or(|child| child.text_range() != node.text_range())
        })
    {
        return false;
    }
    statement
        .parent()
        .and_then(ast::StmtList::cast)
        .and_then(|list| list.syntax().parent())
        .and_then(ast::BlockExpr::cast)
        .and_then(|block| block.syntax().parent())
        .is_some_and(|parent| ast::Fn::can_cast(parent.kind()))
}

/// Whether a copied literal fits its parameter's declared type as written:
/// an integer literal for an integer type (a suffix must name that type, a
/// `-` needs a signed one), a number for `f32`/`f64`, `true`/`false` for
/// `bool`, a char literal for `char`, a string literal for `&str` and a byte
/// string for `&[u8]`. Any other type (an alias, `impl Trait`, `&mut`) does
/// not take a copied value.
pub(super) fn literal_fits(ty_text: &str, literal: &str) -> bool {
    let ty: String = ty_text.split_whitespace().collect();
    // `&str`, `&'_ str` or `&'a str` (and likewise `&[u8]`), never `&mut`.
    let reference = |inner: &str| {
        let Some(rest) = ty_text.trim().strip_prefix('&') else {
            return false;
        };
        let rest = rest.trim_start();
        let rest = match rest.strip_prefix('\'') {
            Some(lifetime) => {
                lifetime.trim_start_matches(|c: char| c.is_alphanumeric() || c == '_')
            }
            None => rest,
        };
        rest.split_whitespace().collect::<String>() == inner
    };
    let (negative, body) = match literal.strip_prefix('-') {
        Some(rest) => (true, rest.trim_start()),
        None => (false, literal),
    };
    let numeric = body.starts_with(|c: char| c.is_ascii_digit());
    if INTEGER_TYPES.contains(&ty.as_str()) {
        if !numeric || (negative && ty.starts_with('u')) {
            return false;
        }
        // `usize` and `isize` contain an `e`; look for an exponent only
        // before the suffix.
        let digits = INTEGER_TYPES
            .iter()
            .find_map(|suffix| body.strip_suffix(*suffix))
            .unwrap_or(body);
        let float_like = !body.starts_with("0x")
            && (digits.contains('.')
                || digits.contains(['e', 'E'])
                || body.ends_with("f32")
                || body.ends_with("f64"));
        let suffix = INTEGER_TYPES.iter().find(|suffix| body.ends_with(*suffix));
        // In range for the type, so `overflowing_literals` (deny by
        // default) cannot reject the stub's `let`. `isize`/`usize` use the
        // 32-bit range, which holds on every target.
        let (low, high): (i128, i128) = match ty.as_str() {
            "i8" => (i8::MIN.into(), i8::MAX.into()),
            "i16" => (i16::MIN.into(), i16::MAX.into()),
            "i32" | "isize" => (i32::MIN.into(), i32::MAX.into()),
            "i64" => (i64::MIN.into(), i64::MAX.into()),
            "i128" => (i128::MIN, i128::MAX),
            "u8" => (0, u8::MAX.into()),
            "u16" => (0, u16::MAX.into()),
            "u32" | "usize" => (0, u32::MAX.into()),
            "u64" => (0, u64::MAX.into()),
            _ => (0, i128::MAX),
        };
        let in_range = parse_integer(literal).is_some_and(|value| low <= value && value <= high);
        return !float_like && in_range && suffix.is_none_or(|suffix| *suffix == ty);
    }
    if ty == "f32" || ty == "f64" {
        let other = if ty == "f32" { "f64" } else { "f32" };
        let float_like =
            body.contains('.') || body.contains(['e', 'E']) || body.ends_with(ty.as_str());
        // Finite at the declared precision, or `overflowing_literals`
        // rejects the stub's `let`.
        let value = body
            .strip_suffix(ty.as_str())
            .unwrap_or(body)
            .replace('_', "");
        let finite = if ty == "f32" {
            value.parse::<f32>().is_ok_and(f32::is_finite)
        } else {
            value.parse::<f64>().is_ok_and(f64::is_finite)
        };
        return numeric
            && float_like
            && finite
            && !body.starts_with("0x")
            && !body.starts_with("0b")
            && !body.starts_with("0o")
            && !body.ends_with(other)
            && !INTEGER_TYPES.iter().any(|suffix| body.ends_with(suffix));
    }
    if negative {
        return false;
    }
    match ty.as_str() {
        "bool" => literal == "true" || literal == "false",
        "char" => literal.starts_with('\''),
        _ if reference("str") => {
            literal.starts_with('"') || literal.starts_with("r\"") || literal.starts_with("r#")
        }
        _ if reference("[u8]") => literal.starts_with("b\"") || literal.starts_with("br"),
        _ => false,
    }
}

/// Whether the copied values leave the owner on its way to the statement
/// holding the changed comparison, reading none of them on the way into it
/// (a guard naming only written inputs is not checked). Every
/// earlier statement of the owner's body that names a copied parameter (as
/// an identifier, or as `{name` inside a string a format macro may capture)
/// must be an `if` without `else` whose condition evaluates to false; no
/// earlier statement may bind the name of any written or copied input. The
/// comparison's own statement may name a copied value only after the
/// comparison ends, and is not an assignment or a `let`, whose value side
/// runs first.
fn reaches_seam(
    file: &SourceFile,
    owner: &ast::Fn,
    seam_offset: usize,
    copied: &[String],
    inputs: &[(String, String)],
    typed: &[(String, String)],
) -> bool {
    let Ok(offset) = TextSize::try_from(seam_offset) else {
        return false;
    };
    let Some(list) = owner.body().and_then(|body| body.stmt_list()) else {
        return false;
    };
    let names_copied = |node: &SyntaxNode, within: Option<(TextSize, TextSize)>| {
        node.descendants_with_tokens()
            .filter_map(|element| element.into_token())
            .filter(|token| {
                within.is_none_or(|(start, end)| {
                    token.text_range().start() >= start && token.text_range().end() <= end
                })
            })
            .any(|token| {
                copied.iter().any(|name| match token.kind() {
                    SyntaxKind::IDENT => unraw(name) == unraw(token.text()),
                    // `{name}` and the width or precision capture `{:name$}`.
                    SyntaxKind::STRING => {
                        token.text().contains(&format!("{{{}", unraw(name)))
                            || token.text().contains(&format!("{}$", unraw(name)))
                    }
                    _ => false,
                })
            })
    };
    let is_input = |text: &str| inputs.iter().any(|(name, _)| unraw(name) == unraw(text));
    // A pattern binding an input's name. A macro handed that name may also
    // expand to a `let` of it, which `foreign_macro` refuses.
    let binds_input = |node: &SyntaxNode| {
        node.descendants()
            .filter_map(ast::IdentPat::cast)
            .filter_map(|ident| ident.name())
            .any(|bound| is_input(bound.text()))
    };
    // `include!`, a procedural macro or any macro other than a standard one
    // may read a copied value without spelling its name; so may a macro
    // nested inside a standard one's arguments.
    let foreign_macro = |node: &SyntaxNode| {
        node.descendants()
            .filter_map(ast::MacroCall::cast)
            .any(|call| {
                let standard = call
                    .path()
                    .filter(|path| path.qualifier().is_none())
                    .and_then(|path| path.segment())
                    .and_then(|segment| segment.name_ref())
                    .is_some_and(|called| STANDARD_MACROS.contains(&unraw(called.text())));
                !standard
                    || call.token_tree().is_some_and(|tree| {
                        tree.syntax()
                            .descendants_with_tokens()
                            .any(|inner| inner.kind() == SyntaxKind::BANG)
                    })
            })
    };
    let nodes = list
        .statements()
        .map(|statement| statement.syntax().clone())
        .chain(list.tail_expr().map(|tail| tail.syntax().clone()));
    for node in nodes {
        if node.text_range().contains(offset) {
            // The innermost comparison at the seam bounds the region where
            // the copied values may appear: only after it.
            let Some(comparison) = node
                .descendants()
                .filter_map(ast::BinExpr::cast)
                .filter(|bin| {
                    bin.syntax().text_range().contains(offset)
                        && matches!(bin.op_kind(), Some(ast::BinaryOp::CmpOp(_)))
                })
                .max_by_key(|bin| bin.syntax().text_range().start())
            else {
                return false;
            };
            // A comparison in a closure or `async` block runs only if and
            // when something later calls it, which a copied value may decide.
            let deferred = comparison
                .syntax()
                .ancestors()
                .take_while(|ancestor| ancestor != &node)
                .any(|ancestor| {
                    ast::ClosureExpr::can_cast(ancestor.kind())
                        || ast::BlockExpr::cast(ancestor)
                            .is_some_and(|block| block.async_token().is_some())
                });
            if deferred {
                return false;
            }
            // An assignment runs its right side before its place, so a
            // comparison anywhere inside one may follow a copied value.
            let assignment = comparison
                .syntax()
                .ancestors()
                .take_while(|ancestor| ancestor != &node)
                // The statement itself, when it is a tail assignment.
                .chain(std::iter::once(node.clone()))
                .chain(
                    ast::ExprStmt::cast(node.clone())
                        .and_then(|stmt| stmt.expr())
                        .map(|expr| expr.syntax().clone()),
                )
                .filter_map(ast::BinExpr::cast)
                .any(|bin| matches!(bin.op_kind(), Some(ast::BinaryOp::Assignment { .. })));
            let start = node.text_range().start();
            let end = comparison.syntax().text_range().end();
            let rebinds_before = node
                .descendants()
                .filter(|inner| {
                    inner.text_range().end() <= comparison.syntax().text_range().start()
                })
                .any(|inner| {
                    (ast::IdentPat::can_cast(inner.kind()) && binds_input(&inner))
                        || (ast::MacroCall::can_cast(inner.kind()) && foreign_macro(&inner))
                });
            return !assignment
                && !rebinds_before
                && !ast::LetStmt::can_cast(node.kind())
                && !names_copied(&node, Some((start, end)));
        }
        if binds_input(&node) || foreign_macro(&node) {
            return false;
        }
        if !names_copied(&node, None) {
            continue;
        }
        let Some(ast::Expr::IfExpr(guard)) = ast::ExprStmt::cast(node).and_then(|stmt| stmt.expr())
        else {
            return false;
        };
        if guard.else_branch().is_some() || guard.attrs().next().is_some() {
            return false;
        }
        let Some(condition) = guard.condition() else {
            return false;
        };
        // The evaluator compares integers untyped, so every integer literal
        // in the guard must fit each integer parameter it names, or a lint
        // allowance elsewhere may let the literal wrap.
        let named: Vec<&str> = condition
            .syntax()
            .descendants_with_tokens()
            .filter_map(|element| element.into_token())
            .filter(|token| token.kind() == SyntaxKind::IDENT)
            .filter_map(|token| {
                typed
                    .iter()
                    .find(|(name, _)| unraw(name) == unraw(token.text()))
                    .map(|(_, ty)| ty.as_str())
            })
            .filter(|ty| INTEGER_TYPES.contains(ty))
            .collect();
        let literals_fit = condition
            .syntax()
            .descendants_with_tokens()
            .filter_map(|element| element.into_token())
            .filter(|token| token.kind() == SyntaxKind::INT_NUMBER)
            .all(|literal| named.iter().all(|ty| literal_fits(ty, literal.text())));
        if !literals_fit || condition_value(file, &condition, inputs) != Some(false) {
            return false;
        }
    }
    false
}
