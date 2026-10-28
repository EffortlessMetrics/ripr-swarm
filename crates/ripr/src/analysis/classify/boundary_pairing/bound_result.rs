//! Select the value a block initializer actually binds, not discarded calls.

use crate::analysis::facts::FunctionContainer;
use crate::analysis::rust_index::FunctionSummary;
use crate::analysis::syntax::{fn_name_binding_count, parse_clean_source_file};
use ra_ap_syntax::{AstNode, SyntaxKind, ast};
use std::borrow::Cow;

pub(super) struct BoundResult<'a> {
    pub(super) text: Cow<'a, str>,
    pub(super) block_tail: bool,
}

/// Preserve nonblock and inherent binding admission. A free-owner block
/// contributes only its whole returned call, through parentheses and tails.
/// Unsupported flow never falls back to scanning its discarded statements.
pub(super) fn boundary_call<'a>(
    binding: &'a str,
    owner: &FunctionSummary,
    test_body: &str,
) -> Option<BoundResult<'a>> {
    let original = || BoundResult {
        text: Cow::Borrowed(binding),
        block_tail: false,
    };
    if owner.item.container != FunctionContainer::Free || !binding.contains('{') {
        return Some(original());
    }
    let wrapped = format!("fn boundary_binding() {{ {binding}; }}");
    let parsed = parse_clean_source_file(&wrapped)?;
    let function = parsed
        .tree()
        .syntax()
        .descendants()
        .find_map(ast::Fn::cast)?;
    let statement = function.body()?.statements().next()?;
    let ast::Stmt::LetStmt(binding) = statement else {
        return None;
    };
    let mut value = binding.initializer()?;
    let mut block_tail = false;
    loop {
        match value {
            ast::Expr::ParenExpr(inner) => value = inner.expr()?,
            ast::Expr::BlockExpr(block) => {
                if block.label().is_some()
                    || block.async_token().is_some()
                    || block.const_token().is_some()
                    || block.gen_token().is_some()
                    || block.try_block_modifier().is_some()
                    || block.syntax().descendants().any(|node| {
                        ast::Item::can_cast(node.kind())
                            || matches!(
                                node.kind(),
                                SyntaxKind::RETURN_EXPR
                                    | SyntaxKind::BREAK_EXPR
                                    | SyntaxKind::CONTINUE_EXPR
                                    | SyntaxKind::TRY_EXPR
                                    | SyntaxKind::CLOSURE_EXPR
                                    | SyntaxKind::MACRO_CALL
                            )
                    })
                {
                    return None;
                }
                block_tail = true;
                value = block.stmt_list()?.tail_expr()?;
            }
            _ => break,
        }
    }
    if !block_tail {
        return Some(original());
    }
    let text = value.syntax().text().to_string();
    // Module/import resolution does not prove that a bare call escaped a
    // local value binding, including a raw-identifier spelling of its name.
    if super::super::owner_pin::direct_call_name(&text) == Some(owner.name.as_str())
        && (fn_name_binding_count(test_body, &owner.name) != Some(0)
            || fn_name_binding_count(test_body, &format!("r#{}", owner.name)) != Some(0))
    {
        return None;
    }
    super::super::owner_pin::is_whole_free_owner_call(&text, &owner.name).then_some(BoundResult {
        text: Cow::Owned(text),
        block_tail: true,
    })
}
