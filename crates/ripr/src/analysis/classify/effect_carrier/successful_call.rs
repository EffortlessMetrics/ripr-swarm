//! Affirmative successful-call receipt for a pushed literal after error exits.
//! This never establishes collection observation or independent expectations.
use super::{FunctionSummary, OracleFact, RustIndex, TestSummary};
use crate::analysis::classify::{
    builder_override::getter_dispatch_body, implicit_effect::ImplicitExpansionContext, owner_pin,
    propagation_witness, reveal,
};
use crate::analysis::facts::FunctionContainer;
use crate::analysis::syntax::{parse_clean_source_file, source_binds_value_name};
use crate::domain::Probe;
use ra_ap_syntax::{
    AstNode,
    ast::{self, HasArgList, HasGenericParams, HasName},
};

fn parsed_function(source: &str) -> Option<ast::Fn> {
    parse_clean_source_file(source)?
        .tree()
        .syntax()
        .children()
        .find_map(ast::Fn::cast)
}

fn source_wrapper_names_closed(source: &str) -> bool {
    let Some(parsed) = parse_clean_source_file(source) else {
        return false;
    };
    if parsed
        .tree()
        .syntax()
        .descendants()
        .filter_map(ast::Attr::cast)
        .any(|attribute| {
            attribute
                .path()
                .is_some_and(|path| path.syntax().text() == "no_implicit_prelude")
        })
    {
        return false;
    }
    ["Result", "Ok", "Err", "unwrap", "expect"]
        .into_iter()
        .all(|name| !source_binds_value_name(source, name))
}

/// A direct top-level push that every successful Result return must pass.
fn successful_path(owner: &str, changed_line: usize, expression: &str, path: &str) -> bool {
    let Some(function) = parsed_function(owner) else {
        return false;
    };
    if function.generic_param_list().is_some() || function.async_token().is_some() {
        return false;
    }
    let Some(return_type) = function.ret_type().and_then(|ret| ret.ty()) else {
        return false;
    };
    let return_type = return_type.syntax().text().to_string();
    if !return_type
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect::<String>()
        .starts_with("Result<")
    {
        return false;
    }
    let Some(block) = function.body() else {
        return false;
    };
    // Nested execution, opaque expansion and nonlocal transfers have no receipt.
    if block.syntax().descendants().any(|node| {
        ast::ClosureExpr::can_cast(node.kind())
            || ast::MacroExpr::can_cast(node.kind())
            || ast::TryExpr::can_cast(node.kind())
            || ast::LoopExpr::can_cast(node.kind())
            || ast::WhileExpr::can_cast(node.kind())
            || ast::ForExpr::can_cast(node.kind())
            || ast::AwaitExpr::can_cast(node.kind())
    }) {
        return false;
    }
    // Reuse the selected-return authority: no early successful return exists.
    if !block
        .syntax()
        .descendants()
        .filter_map(ast::ReturnExpr::cast)
        .all(|returned| owner_pin::returns_other_variant(&returned, "Ok"))
    {
        return false;
    }
    let Some(list) = block.stmt_list() else {
        return false;
    };
    let Some(ast::Expr::CallExpr(tail)) = list.tail_expr() else {
        return false;
    };
    if !tail
        .expr()
        .is_some_and(|callee| callee.syntax().text() == "Ok")
        || tail.arg_list().is_none_or(|args| args.args().count() != 1)
    {
        return false;
    }
    list.statements().any(|statement| {
        let ast::Stmt::ExprStmt(statement) = statement else {
            return false;
        };
        let Some(ast::Expr::MethodCallExpr(push)) = statement.expr() else {
            return false;
        };
        if !push.name_ref().is_some_and(|name| name.text() == "push")
            || !push.receiver().is_some_and(|receiver| {
                receiver.syntax().text().to_string() == format!("self.{path}")
            })
        {
            return false;
        }
        let at = usize::from(push.syntax().text_range().start());
        let text = push.syntax().text().to_string();
        owner[..at].matches('\n').count() == changed_line
            && text.matches(expression).count() == 1
            && !push.syntax().descendants().any(|node| {
                ast::IfExpr::can_cast(node.kind())
                    || ast::MatchExpr::can_cast(node.kind())
                    || ast::BlockExpr::can_cast(node.kind())
            })
    })
}

/// A direct eager owner call, consumed by Result's success-only operation.
fn consumed_call(
    body: &str,
    start_line: usize,
    read_line: usize,
    binding: &str,
    method: &str,
) -> bool {
    let Some(function) = parsed_function(body) else {
        return false;
    };
    let Some(list) = function.body().and_then(|body| body.stmt_list()) else {
        return false;
    };
    list.statements().any(|statement| {
        let ast::Stmt::ExprStmt(statement) = statement else {
            return false;
        };
        let Some(ast::Expr::MethodCallExpr(consume)) = statement.expr() else {
            return false;
        };
        let name = consume.name_ref().map(|name| name.text().to_string());
        let Some(arguments) = consume.arg_list() else {
            return false;
        };
        if !match name.as_deref() {
            Some("unwrap") => arguments.args().next().is_none(),
            Some("expect") => {
                let mut args = arguments.args();
                matches!(
                    (args.next(), args.next()),
                    (Some(ast::Expr::Literal(_)), None)
                )
            }
            _ => false,
        } {
            return false;
        }
        let Some(ast::Expr::MethodCallExpr(call)) = consume.receiver() else {
            return false;
        };
        let at = usize::from(call.syntax().text_range().start());
        start_line + body[..at].matches('\n').count() < read_line
            && call.name_ref().is_some_and(|name| name.text() == method)
            && call
                .receiver()
                .is_some_and(|receiver| receiver.syntax().text() == binding)
            && call
                .arg_list()
                .is_some_and(|args| !args.syntax().text().to_string().contains(binding))
    })
}

pub(in crate::analysis) fn successful_push_call(
    owner: &FunctionSummary,
    probe: &Probe,
    path: &str,
    index: &RustIndex,
    test: &TestSummary,
    assertion: &OracleFact,
    expansion: &ImplicitExpansionContext<'_>,
) -> bool {
    if !owner.item.has_self_param
        || index
            .files()
            .values()
            .any(|facts| facts.used_lexical_fallback)
        || !matches!(owner.item.container, FunctionContainer::Inherent { .. })
        || index
            .functions()
            .iter()
            .filter(|function| function.name == owner.name)
            .count()
            != 1
        || ["Result", "Ok", "Err", "unwrap", "expect"]
            .into_iter()
            .any(|name| owner_pin::wrapper_may_be_rebound(name, index))
        || index.files().values().any(|facts| {
            !source_wrapper_names_closed(&facts.source)
                || parse_clean_source_file(&facts.source).is_none_or(|parsed| {
                    parsed
                        .tree()
                        .syntax()
                        .descendants()
                        .filter_map(ast::TypeParam::cast)
                        .any(|parameter| {
                            parameter.name().is_some_and(|name| {
                                matches!(name.text().to_string().as_str(), "Result" | "Ok" | "Err")
                            })
                        })
                })
        })
        || !probe
            .location
            .line
            .checked_sub(owner.start_line)
            .is_some_and(|line| successful_path(&owner.body, line, &probe.expression, path))
    {
        return false;
    }
    let FunctionContainer::Inherent { self_ty } = &owner.item.container else {
        return false;
    };
    let Some((resolved, _)) = getter_dispatch_body(index, self_ty, &owner.name, expansion) else {
        return false;
    };
    if resolved.id != owner.id
        || resolved.file != owner.file
        || resolved.start_line != owner.start_line
    {
        return false;
    }
    propagation_witness::assertion_whole_collection_equality(&assertion.text, path)
        .into_iter()
        .any(|read| {
            owner_pin::test_receiver_named_type(
                test,
                &read.binding,
                owner_pin::OwnerScope::new(&owner.name, owner.start_line, &owner.file),
                index,
            )
            .as_deref()
                == Some(self_ty.as_str())
                && !reveal::owner_called_on_other_receiver(&test.body, &read.binding, &owner.name)
                && reveal::owner_call_precedes_read(
                    &test.body,
                    test.start_line,
                    &read.binding,
                    &owner.name,
                    assertion.line,
                )
                && reveal::binding_untouched_after_owner_call(
                    &test.body,
                    test.start_line,
                    &read.binding,
                    &owner.name,
                    assertion.line,
                )
                && consumed_call(
                    &test.body,
                    test.start_line,
                    assertion.line,
                    &read.binding,
                    &owner.name,
                )
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    const OWNER: &str = "fn ship(&mut self) -> Result<(), E> {\n if self.empty { return Err(E); }\n self.log.push(Event { sku: sku.to_owned() });\n Ok(())\n}";
    #[test]
    fn success_requires_the_changed_top_level_push_and_only_error_exits() {
        let admits = |source: &str| successful_path(source, 2, "sku: sku.to_owned()", "log");
        assert!(admits(OWNER));
        assert!(!admits(&OWNER.replace("return Err(E)", "return Ok(())")));
        assert!(!admits(&OWNER.replace(
            " self.log.push(Event { sku: sku.to_owned() });",
            " if self.enabled { self.log.push(Event { sku: sku.to_owned() }); }"
        )));
        assert!(!admits(
            &OWNER.replace(" if self.empty", " guard!(); if self.empty")
        ));
        assert!(!admits(&OWNER.replace("return Err(E)", "return other()?")));
    }
    #[test]
    fn wrapper_and_consumption_names_need_authentication() {
        assert!(source_wrapper_names_closed("fn ordinary() {}"));
        for source in [
            "fn Err<T>(value: T) -> T { value }",
            "struct Result<T, E>(T, E);",
            "fn t() { let Err = identity; }",
            "struct Fake; impl Fake { fn unwrap(self) {} }",
            "use foreign::Result;",
            "#![no_implicit_prelude] fn t() {}",
        ] {
            assert!(!source_wrapper_names_closed(source), "{source}");
        }
    }
    #[test]
    fn consumption_is_direct_eager_and_success_only() {
        let admits = |statement: &str| {
            consumed_call(
                &format!("fn t() {{\n {statement}\n assert_eq!(inv.history(), expected);\n}}"),
                1,
                3,
                "inv",
                "ship",
            )
        };
        assert!(admits("inv.ship().unwrap();"));
        assert!(admits("inv.ship().expect(\"success\");"));
        for refused in [
            "inv.ship().unwrap_err();",
            "let _ = inv.ship();",
            "if enabled { inv.ship().unwrap(); }",
            "(|| inv.ship().unwrap())();",
            "other.ship().unwrap();",
        ] {
            assert!(!admits(refused), "{refused}");
        }
    }
}
