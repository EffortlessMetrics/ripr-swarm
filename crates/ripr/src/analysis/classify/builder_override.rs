//! Builder override: a test that overwrites a changed field before reading it
//! (RIPR-SPEC-0040 builder override, #6613).
//!
//! A `return_value` probe on a struct literal (`RetryBuilder { attempts: 4,
//! delay_ms: 100 }`) or a `field_construction` probe on one of its fields
//! (`attempts: 4`) is confirmed by `reveal` when an assertion names one of the
//! changed tokens, typically a field read (`retry.attempts`). When the test
//! sets that same field on the owner's result before reading it
//! (`RetryBuilder::new().attempts(5).build()`), the changed initializer never
//! reaches the assertion: the read observes the test's own value, and the
//! shared field name is coincidence, not observation.
//!
//! Everything here is read from the syntax tree, and every decision fails
//! closed toward the existing credit: a field counts as overridden only when
//! the assertion is the one macro call on its `OracleFact` line, every mention
//! of the field in it is a field read, and each read's value is tied to the
//! owner call through its own method-call receiver chain or through `let`
//! bindings, with a setter whose result flows into the read value: a
//! setter-shaped call (`.field(arg)`, `.with_field(arg)`, `.set_field(arg)`)
//! on that receiver chain, a reassignment `b = b.field(arg)`, or a field
//! assignment `b.field = value` after the binding's `let`. Setter calls inside
//! arguments, a setter whose result is discarded (`b.with_field(5);`), and
//! anything the tree cannot tie to the owner leave the token confirming.

use crate::analysis::syntax::parse_clean_source_file;
use ra_ap_syntax::{
    AstNode, SourceFile, SyntaxKind, SyntaxNode, TextSize,
    ast::{self, HasArgList, HasName},
};

use super::super::rust_index::{OracleFact, TestSummary};

/// Upper bound on `let` hops from the assertion's receiver back to the owner
/// call (`retry` <- `builder.build()` <- `RetryBuilder::new()`).
const MAX_BINDING_HOPS: usize = 4;

/// The owner call a test value must come from: the owner's bare name and,
/// for an associated function, the type it is defined on
/// (`src/settings.rs::impl RetryBuilder::new` -> `RetryBuilder`, `new`).
#[derive(Clone, Copy, Debug)]
pub(super) struct OwnerCall<'a> {
    name: &'a str,
    impl_type: Option<&'a str>,
}

impl<'a> OwnerCall<'a> {
    /// Reads the owner call from the owner symbol id. `None` when the id has
    /// no bare name.
    pub(super) fn from_symbol(symbol: &'a str) -> Option<Self> {
        let mut segments = symbol.rsplit("::");
        let name = segments.next().filter(|name| is_identifier(name))?;
        let impl_type = segments
            .next()
            .and_then(|segment| segment.strip_prefix("impl "))
            .map(|ty| {
                // `impl Trait for Type` names the type last; generics are
                // dropped from the type name.
                let ty = ty.rsplit(" for ").next().unwrap_or(ty).trim();
                ty.split('<').next().unwrap_or(ty).trim()
            });
        Some(Self { name, impl_type })
    }

    /// Whether `call` is a direct call of the owner: `RetryBuilder::new(..)`
    /// or `Self::new(..)` for an associated owner, a bare `owner(..)` for a
    /// free one.
    fn is_called_by(&self, call: &ast::CallExpr) -> bool {
        let Some(ast::Expr::PathExpr(callee)) = call.expr() else {
            return false;
        };
        let Some(path) = callee.path() else {
            return false;
        };
        let named = path
            .segment()
            .and_then(|segment| segment.name_ref())
            .is_some_and(|name| name.text() == self.name);
        if !named {
            return false;
        }
        let qualifier = path
            .qualifier()
            .and_then(|qualifier| qualifier.segment())
            .map(|segment| {
                segment
                    .name_ref()
                    .map(|name| name.text().to_string())
                    .unwrap_or_else(|| segment.syntax().text().to_string())
            });
        match (self.impl_type, qualifier.as_deref()) {
            (Some(ty), Some(qualifier)) => qualifier == ty || qualifier == "Self",
            (None, None) => true,
            _ => false,
        }
    }
}

/// Field names a struct-literal expression initializes, read from its
/// `RecordExpr` (`RetryBuilder { attempts: 4, delay_ms: 100 }` ->
/// `["attempts", "delay_ms"]`). Shorthand initializers count; a `..base`
/// spread does not. Empty for anything that is not one struct literal.
pub(super) fn struct_literal_fields(expression: &str) -> Vec<String> {
    let source = format!(
        "fn __ripr_struct_literal() {{ let _ = {}; }}",
        expression.trim().trim_end_matches(';')
    );
    let Some(parse) = parse_clean_source_file(&source) else {
        return Vec::new();
    };
    let Some(initializer) = parse
        .tree()
        .syntax()
        .descendants()
        .find_map(ast::LetStmt::cast)
        .and_then(|statement| statement.initializer())
    else {
        return Vec::new();
    };
    let ast::Expr::RecordExpr(record) = initializer else {
        return Vec::new();
    };
    record
        .record_expr_field_list()
        .into_iter()
        .flat_map(|list| list.fields())
        .filter_map(|field| field.field_name())
        .map(|name| name.text().to_string())
        .collect()
}

/// The subset of `fields` whose every mention in `assertion` is a field read
/// on a value the test overwrote that field on after obtaining it from the
/// `owner` call. Empty when the assertion cannot be located by its line in
/// `test`'s body, or when anything along the way cannot be read.
pub(super) fn overridden_fields_read(
    test: &TestSummary,
    assertion: &OracleFact,
    owner: OwnerCall<'_>,
    fields: &[String],
) -> Vec<String> {
    if fields.is_empty() {
        return Vec::new();
    }
    let body: &str = &test.body;
    let Some(parse) = parse_clean_source_file(body) else {
        return Vec::new();
    };
    let tree = parse.tree();
    let Some(assertion_call) = assertion_macro_at_line(&tree, body, test, assertion) else {
        return Vec::new();
    };
    let Some(arguments) = assertion_call.token_tree() else {
        return Vec::new();
    };
    let fragment_source = format!(
        "fn __ripr_assertion_arguments() {{ let _ = {}; }}",
        arguments.syntax().text()
    );
    let Some(fragment) = parse_clean_source_file(&fragment_source) else {
        return Vec::new();
    };
    let visible = visible_statements_before(assertion_call.syntax());
    fields
        .iter()
        .filter(|field| every_mention_is_overridden_read(&fragment.tree(), &visible, owner, field))
        .cloned()
        .collect()
}

/// The single macro call (`assert_eq!(..)`) starting on the assertion's line.
/// Two calls on the line, or none, leave the assertion unlocated.
fn assertion_macro_at_line(
    tree: &SourceFile,
    body: &str,
    test: &TestSummary,
    assertion: &OracleFact,
) -> Option<ast::MacroCall> {
    let relative_line = assertion.line.checked_sub(test.start_line)?;
    let mut calls = tree
        .syntax()
        .descendants()
        .filter_map(ast::MacroCall::cast)
        .filter(|call| {
            let start = macro_name_start(call);
            line_of(body, start) == Some(relative_line)
        });
    let call = calls.next()?;
    calls.next().is_none().then_some(call)
}

/// The macro call's own start, at its path (an attributed call spans its
/// attributes).
fn macro_name_start(call: &ast::MacroCall) -> TextSize {
    call.path()
        .map(|path| path.syntax().text_range().start())
        .unwrap_or_else(|| call.syntax().text_range().start())
}

/// Zero-based line of `offset` in `text`.
fn line_of(text: &str, offset: TextSize) -> Option<usize> {
    let offset = usize::from(offset);
    text.get(..offset)
        .map(|prefix| prefix.matches('\n').count())
}

fn every_mention_is_overridden_read(
    fragment: &SourceFile,
    visible: &[ast::Stmt],
    owner: OwnerCall<'_>,
    field: &str,
) -> bool {
    let mentions: Vec<_> = fragment
        .syntax()
        .descendants_with_tokens()
        .filter_map(|element| element.into_token())
        .filter(|token| token.kind() == SyntaxKind::IDENT && token.text() == field)
        .collect();
    !mentions.is_empty()
        && mentions.iter().all(|token| {
            let Some(read) = token
                .parent()
                .and_then(ast::NameRef::cast)
                .and_then(|name| name.syntax().parent())
                .and_then(ast::FieldExpr::cast)
            else {
                return false;
            };
            read.expr().is_some_and(|receiver| {
                value_state(&receiver, visible, owner, field, MAX_BINDING_HOPS)
                    == ValueState::Overridden
            })
        })
}

/// What a test value carries in `field` relative to the owner's result.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ValueState {
    /// The value comes from the owner call and the test wrote `field` on it.
    Overridden,
    /// The value comes from the owner call and `field` was not written.
    Default,
    /// The value could not be tied to the owner call.
    Unknown,
}

/// Follows `expression`'s method-call receiver chain back to the owner call,
/// directly or through `let` bindings among `visible` (the statements that
/// run before it, nearest last). Only receivers are followed: a setter inside
/// an argument writes some other value.
fn value_state(
    expression: &ast::Expr,
    visible: &[ast::Stmt],
    owner: OwnerCall<'_>,
    field: &str,
    hops: usize,
) -> ValueState {
    let (root, setter_on_chain) = chain_root(expression, field);
    let combine = |state: ValueState, written: bool| match state {
        ValueState::Unknown => ValueState::Unknown,
        _ if written => ValueState::Overridden,
        state => state,
    };
    match root {
        ast::Expr::CallExpr(call) if owner.is_called_by(&call) => {
            combine(ValueState::Default, setter_on_chain)
        }
        ast::Expr::PathExpr(path) => {
            let Some(binding) = single_identifier(&path) else {
                return ValueState::Unknown;
            };
            let Some(hops) = hops.checked_sub(1) else {
                return ValueState::Unknown;
            };
            let Some((index, statement)) = binding_let(visible, &binding) else {
                return ValueState::Unknown;
            };
            let Some(initializer) = statement.initializer() else {
                return ValueState::Unknown;
            };
            let bound = value_state(
                &initializer,
                &visible_statements_before(statement.syntax()),
                owner,
                field,
                hops,
            );
            match binding_writes(&visible[index + 1..], &binding, field) {
                None => ValueState::Unknown,
                Some(written) => combine(bound, setter_on_chain || written),
            }
        }
        _ => ValueState::Unknown,
    }
}

/// The root of `expression`'s receiver chain (through method calls, parens
/// and `?`), and whether a setter of `field` is on the chain.
fn chain_root(expression: &ast::Expr, field: &str) -> (ast::Expr, bool) {
    let mut current = expression.clone();
    let mut setter = false;
    loop {
        let next = match &current {
            ast::Expr::MethodCallExpr(call) => {
                setter |= is_setter(call, field);
                call.receiver()
            }
            ast::Expr::ParenExpr(paren) => paren.expr(),
            ast::Expr::TryExpr(try_expr) => try_expr.expr(),
            _ => None,
        };
        match next {
            Some(next) => current = next,
            None => return (current, setter),
        }
    }
}

/// A setter-shaped call writing `field` with an argument: `.field(5)`,
/// `.with_field(5)`, `.set_field(5)`. A zero-argument `.field()` reads.
fn is_setter(call: &ast::MethodCallExpr, field: &str) -> bool {
    let named = call.name_ref().is_some_and(|name| {
        let name = name.text();
        name == field
            || name.strip_prefix("with_") == Some(field)
            || name.strip_prefix("set_") == Some(field)
    });
    named
        && call
            .arg_list()
            .is_some_and(|arguments| arguments.args().next().is_some())
}

fn single_identifier(path: &ast::PathExpr) -> Option<String> {
    let path = path.path()?;
    if path.qualifier().is_some() {
        return None;
    }
    let name = path.segment()?.name_ref()?.text().to_string();
    is_identifier(&name).then_some(name)
}

/// The nearest `let [mut] binding = ..;` among `visible`, with its index.
/// A nearer `let` that binds the name through any other pattern
/// (destructuring, `ref`) shadows it unreadably, so nothing is returned.
fn binding_let(visible: &[ast::Stmt], binding: &str) -> Option<(usize, ast::LetStmt)> {
    visible
        .iter()
        .enumerate()
        .rev()
        .find_map(|(index, statement)| {
            let ast::Stmt::LetStmt(statement) = statement else {
                return None;
            };
            let pattern = statement.pat()?;
            let binds = pattern
                .syntax()
                .descendants()
                .filter_map(ast::IdentPat::cast)
                .any(|ident| ident.name().is_some_and(|name| name.text() == binding));
            binds.then(|| (index, statement.clone()))
        })
        .and_then(|(index, statement)| {
            let ast::Pat::IdentPat(ident) = statement.pat()? else {
                return None;
            };
            (ident.ref_token().is_none() && ident.pat().is_none() && statement.let_else().is_none())
                .then_some((index, statement))
        })
}

/// Whether `statements` (after the binding's `let`, before its use) write
/// `field` on `binding`: `binding = binding.field(5)` or `binding.field =
/// value` with a value not read from `binding`. `None` when `binding` is
/// reassigned from anything that is not its own setter chain, which leaves
/// what it holds unread.
fn binding_writes(statements: &[ast::Stmt], binding: &str, field: &str) -> Option<bool> {
    let mut written = false;
    for statement in statements {
        let ast::Stmt::ExprStmt(statement) = statement else {
            continue;
        };
        let Some(ast::Expr::BinExpr(assignment)) = statement.expr() else {
            continue;
        };
        if assignment.op_kind() != Some(ast::BinaryOp::Assignment { op: None }) {
            continue;
        }
        let (Some(target), Some(value)) = (assignment.lhs(), assignment.rhs()) else {
            continue;
        };
        match target {
            ast::Expr::PathExpr(path) if single_identifier(&path).as_deref() == Some(binding) => {
                let (root, setter) = chain_root(&value, field);
                let rooted = matches!(&root, ast::Expr::PathExpr(path)
                    if single_identifier(path).as_deref() == Some(binding));
                if !rooted {
                    return None;
                }
                written |= setter;
            }
            ast::Expr::FieldExpr(target)
                if target.name_ref().is_some_and(|name| name.text() == field)
                    && target.expr().is_some_and(|receiver| {
                        matches!(&receiver, ast::Expr::PathExpr(path)
                            if single_identifier(path).as_deref() == Some(binding))
                    }) =>
            {
                written |= !mentions_identifier(value.syntax(), binding);
            }
            _ => {}
        }
    }
    Some(written)
}

fn mentions_identifier(node: &SyntaxNode, name: &str) -> bool {
    node.descendants_with_tokens()
        .filter_map(|element| element.into_token())
        .any(|token| token.kind() == SyntaxKind::IDENT && token.text() == name)
}

/// The statements that run before `node` in straight-line order: for each
/// enclosing block, its statements that end before the branch holding
/// `node`, outermost block first. Stops at a closure or nested function, whose
/// body runs at another time.
fn visible_statements_before(node: &SyntaxNode) -> Vec<ast::Stmt> {
    let mut levels = Vec::new();
    let mut child = node.clone();
    while let Some(parent) = child.parent() {
        if ast::ClosureExpr::can_cast(parent.kind()) || ast::Fn::can_cast(parent.kind()) {
            break;
        }
        if ast::StmtList::can_cast(parent.kind()) {
            let start = child.text_range().start();
            let level: Vec<ast::Stmt> = parent
                .children()
                .filter(|statement| statement.text_range().end() <= start)
                .filter_map(ast::Stmt::cast)
                .collect();
            levels.push(level);
        }
        child = parent;
    }
    levels.into_iter().rev().flatten().collect()
}

fn is_identifier(text: &str) -> bool {
    let mut chars = text.chars();
    chars
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic() || first == '_')
        && chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{OracleKind, OracleStrength};
    use std::path::PathBuf;

    const OWNER: &str = "src/settings.rs::impl RetryBuilder::new";

    fn fields() -> Vec<String> {
        struct_literal_fields("RetryBuilder { attempts: 4, delay_ms: 100 }")
    }

    /// A test whose `fn` keyword sits on line 10, with the assertion on the
    /// body line holding `assert`.
    fn overridden(statements: &str) -> Vec<String> {
        let body = format!("fn case() {{\n{statements}\n}}");
        let line = body
            .lines()
            .position(|line| line.contains("assert"))
            .map_or(0, |index| index + 10);
        let assertion_text = body
            .lines()
            .find(|line| line.contains("assert"))
            .unwrap_or_default()
            .trim()
            .to_string();
        let test = TestSummary {
            name: "case".to_string(),
            file: PathBuf::from("src/settings.rs"),
            start_line: 10,
            end_line: 10 + body.lines().count(),
            body: body.as_str().into(),
            calls: Vec::new(),
            assertions: Vec::new(),
            literals: Vec::new(),
            attrs: Vec::new(),
            nested_fn_names: Vec::new(),
            let_bindings: Vec::new(),
        };
        let assertion = OracleFact {
            line,
            text: assertion_text,
            kind: OracleKind::ExactValue,
            strength: OracleStrength::Strong,
            observed_tokens: Vec::new(),
            ok_value_observed: None,
        };
        let owner = OwnerCall::from_symbol(OWNER).expect("owner");
        overridden_fields_read(&test, &assertion, owner, &fields())
    }

    fn attempts() -> Vec<String> {
        vec!["attempts".to_string()]
    }

    #[test]
    fn struct_literal_fields_reads_the_record_expression() {
        assert_eq!(
            fields(),
            vec!["attempts".to_string(), "delay_ms".to_string()]
        );
        assert_eq!(
            struct_literal_fields("Self { attempts, delay: Duration::from_millis(1), ..base }"),
            vec!["attempts".to_string(), "delay".to_string()]
        );
        assert!(struct_literal_fields("attempts + 1").is_empty());
        assert!(struct_literal_fields("if x { 1 } else { 2 }").is_empty());
        assert!(struct_literal_fields("Some(Retry { attempts: 1 })").is_empty());
    }

    #[test]
    fn owner_call_reads_the_impl_type() {
        let owner = OwnerCall::from_symbol(OWNER).expect("owner");
        assert_eq!(owner.name, "new");
        assert_eq!(owner.impl_type, Some("RetryBuilder"));
        let free = OwnerCall::from_symbol("src/lib.rs::defaults").expect("free owner");
        assert_eq!((free.name, free.impl_type), ("defaults", None));
    }

    #[test]
    fn setter_chained_into_the_binding_overrides_the_read_field() {
        assert_eq!(
            overridden(
                "    let retry = RetryBuilder::new().attempts(5).build();\n    assert_eq!(retry.attempts, 5);"
            ),
            attempts()
        );
        // The chain inside the assertion itself, and `with_`/`set_` setters.
        assert_eq!(
            overridden("    assert_eq!(RetryBuilder::new().with_attempts(5).build().attempts, 5);"),
            attempts()
        );
        assert_eq!(
            overridden(
                "    let retry = Self::new().set_attempts(5).build();\n    assert_eq!(retry.attempts, 5);"
            ),
            attempts()
        );
    }

    #[test]
    fn setter_reaching_the_value_through_let_hops_overrides() {
        // Setter on an intermediate binding's chain.
        assert_eq!(
            overridden(
                "    let builder = RetryBuilder::new();\n    let retry = builder.attempts(5).build();\n    assert_eq!(retry.attempts, 5);"
            ),
            attempts()
        );
        // Reassigned through its own setter.
        assert_eq!(
            overridden(
                "    let mut builder = RetryBuilder::new();\n    builder = builder.attempts(5);\n    let retry = builder.build();\n    assert_eq!(retry.attempts, 5);"
            ),
            attempts()
        );
        // A field assignment after the binding's `let`.
        assert_eq!(
            overridden(
                "    let mut builder = RetryBuilder::new();\n    builder.attempts = 5;\n    let retry = builder.build();\n    assert_eq!(retry.attempts, 5);"
            ),
            attempts()
        );
    }

    #[test]
    fn default_read_without_setter_is_not_overridden() {
        assert!(
            overridden(
                "    let retry = RetryBuilder::new().build();\n    assert_eq!(retry.attempts, 3);"
            )
            .is_empty()
        );
    }

    #[test]
    fn setter_on_another_field_leaves_the_read_field_observed() {
        assert!(
            overridden(
                "    let retry = RetryBuilder::new().delay_ms(5).build();\n    assert_eq!(retry.attempts, 3);"
            )
            .is_empty()
        );
    }

    #[test]
    fn discarded_setter_result_is_not_an_override() {
        // A by-value setter whose new builder is dropped leaves `builder`
        // defaulted; a `&mut self` setter cannot be told apart here, so the
        // statement keeps the credit either way.
        assert!(
            overridden(
                "    let builder = RetryBuilder::new();\n    builder.with_attempts(5);\n    let retry = builder.build();\n    assert_eq!(retry.attempts, 3);"
            )
            .is_empty()
        );
    }

    #[test]
    fn setter_inside_an_argument_is_not_on_the_chain() {
        assert!(
            overridden(
                "    let retry = RetryBuilder::new().with_other(other.attempts(5)).build();\n    assert_eq!(retry.attempts, 3);"
            )
            .is_empty()
        );
    }

    #[test]
    fn writes_before_a_shadowing_let_do_not_count() {
        // The setter applies to the first `builder`; the read comes from the
        // second, defaulted one.
        assert!(
            overridden(
                "    let mut builder = RetryBuilder::new();\n    builder.attempts = 5;\n    let builder = RetryBuilder::new();\n    let retry = builder.build();\n    assert_eq!(retry.attempts, 3);"
            )
            .is_empty()
        );
    }

    #[test]
    fn unrelated_or_unreadable_values_keep_the_credit() {
        // A setter on a value from another type's `new`.
        assert!(
            overridden(
                "    let retry = Other::new().attempts(5).build();\n    assert_eq!(retry.attempts, 5);"
            )
            .is_empty()
        );
        // A binding the test never `let`-binds (a parameter, a static).
        assert!(overridden("    assert_eq!(retry.attempts, 5);").is_empty());
        // A field read on a setter result but also a getter-shaped mention.
        assert!(
            overridden(
                "    let retry = RetryBuilder::new().attempts(5).build();\n    assert_eq!(retry.attempts, retry.attempts());"
            )
            .is_empty()
        );
        // The field name only in a string literal or comment of the body.
        assert!(
            overridden(
                "    // retry.attempts(5)\n    let retry = RetryBuilder::new().build();\n    assert_eq!(retry.attempts, 3, \".attempts(5)\");"
            )
            .is_empty()
        );
        // A write inside a branch is not straight-line.
        assert!(
            overridden(
                "    let mut builder = RetryBuilder::new();\n    if flag { builder.attempts = 5; }\n    let retry = builder.build();\n    assert_eq!(retry.attempts, 3);"
            )
            .is_empty()
        );
    }

    #[test]
    fn an_unlocatable_assertion_keeps_the_credit() {
        // Two assertions on the oracle's line: which one the fact is cannot
        // be read, so nothing is overridden.
        assert!(
            overridden(
                "    let retry = RetryBuilder::new().attempts(5).build();\n    assert_eq!(retry.attempts, 5); assert!(true);"
            )
            .is_empty()
        );
    }
}
