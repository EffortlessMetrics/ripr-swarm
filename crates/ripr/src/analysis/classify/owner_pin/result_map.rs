//! #9870: a bounded Result.map rewrite keeps the original Ok payload.
//!
//! This recognizer supplies only owner-side forwarding. The existing return
//! pin still requires an admitted, owner-bound Ok assertion in each test.
use super::{FunctionSummary, RustIndex, wrapper_may_be_rebound};
use crate::analysis::syntax::parse_clean_source_file;
use crate::domain::{Probe, ProbeFamily};
use ra_ap_syntax::{
    AstNode, SyntaxNode,
    ast::{self, HasArgList, HasGenericArgs, HasName},
};

/// The exact changed tail's line for `Ok(f(receiver?, ...))` rewritten as
/// `receiver.map(|value| f(value, ...))`. A single callback expression must
/// use its binding exactly once, in the old payload's exact position. No
/// statement, second callback, conditional, macro or recomputation qualifies.
pub(in crate::analysis) fn admitted_rewrite(
    probe: &Probe,
    owner: &FunctionSummary,
    index: &RustIndex,
) -> Option<usize> {
    if !probe.expression.contains("map")
        || probe.before.is_none()
        || index
            .files()
            .get(&owner.file)
            .is_none_or(|facts| facts.used_lexical_fallback)
        || probe.family != ProbeFamily::ReturnValue
        || probe.owner.as_ref() != Some(&owner.id)
        || ["Result", "Ok", "Err"]
            .iter()
            .any(|name| wrapper_may_be_rebound(name, index))
    {
        return None;
    }
    let function = function(&owner.body)?;
    if function.async_token().is_some() || !standard_result(&function) {
        return None;
    }
    let block = function.body()?;
    if block.statements().next().is_some() {
        return None;
    }
    let tail = block.tail_expr()?;
    let changed = expression(&probe.expression)?;
    if tokens(tail.syntax()) != tokens(changed.syntax()) {
        return None;
    }
    let line = owner.start_line
        + owner
            .body
            .get(..usize::from(tail.syntax().text_range().start()))?
            .matches('\n')
            .count();
    if line != probe.location.line {
        return None;
    }
    let ast::Expr::MethodCallExpr(map) = tail else {
        return None;
    };
    if map.name_ref()?.text() != "map" || map.generic_arg_list().is_some() {
        return None;
    }
    let receiver = map.receiver()?;
    result_receiver(&receiver, owner, index)?;
    let mut args = map.arg_list()?.args();
    let (Some(ast::Expr::ClosureExpr(callback)), None) = (args.next(), args.next()) else {
        return None;
    };
    if callback.move_token().is_some()
        || callback.async_token().is_some()
        || callback.static_token().is_some()
        || callback.ret_type().is_some()
        || callback
            .syntax()
            .descendants()
            .any(|node| ast::Attr::can_cast(node.kind()))
    {
        return None;
    }
    let mut params = callback.param_list()?.params();
    let param = params.next()?;
    if params.next().is_some() || param.ty().is_some() {
        return None;
    }
    let ast::Pat::IdentPat(binding) = param.pat()? else {
        return None;
    };
    if binding.mut_token().is_some() || binding.ref_token().is_some() || binding.pat().is_some() {
        return None;
    }
    let binding = binding.name()?.text().to_string();
    let body = match callback.body()? {
        ast::Expr::BlockExpr(block) => {
            if !block
                .syntax()
                .text()
                .to_string()
                .trim_start()
                .starts_with('{')
                || block.statements().next().is_some()
            {
                return None;
            }
            block.tail_expr()?
        }
        body => body,
    };
    if body
        .syntax()
        .descendants()
        .filter_map(ast::Expr::cast)
        .any(|expr| {
            !matches!(
                expr,
                ast::Expr::CallExpr(_)
                    | ast::Expr::PathExpr(_)
                    | ast::Expr::Literal(_)
                    | ast::Expr::BinExpr(_)
                    | ast::Expr::PrefixExpr(_)
                    | ast::Expr::ParenExpr(_)
                    | ast::Expr::CastExpr(_)
            )
        })
    {
        return None;
    }
    if super::evaluates_conditionally(&crate::analysis::extract::mask_comments_and_strings(
        &body.syntax().text().to_string(),
    )) {
        return None;
    }
    let uses = body
        .syntax()
        .descendants()
        .filter_map(ast::PathExpr::cast)
        .filter(|path| tokens(path.syntax()) == binding)
        .collect::<Vec<_>>();
    let [used] = uses.as_slice() else {
        return None;
    };
    let body_start = usize::from(body.syntax().text_range().start());
    let start = usize::from(used.syntax().text_range().start()).checked_sub(body_start)?;
    let end = usize::from(used.syntax().text_range().end()).checked_sub(body_start)?;
    let text = body.syntax().text().to_string();
    let reconstructed = format!(
        "{}{}?{}",
        text.get(..start)?,
        receiver.syntax().text(),
        text.get(end..)?
    );
    let before = expression(probe.before.as_deref()?)?;
    let ast::Expr::CallExpr(ok) = before else {
        return None;
    };
    if tokens(ok.expr()?.syntax()) != "Ok" {
        return None;
    }
    let mut args = ok.arg_list()?.args();
    let payload = args.next()?;
    if args.next().is_some() {
        return None;
    }
    let reconstructed = expression(&reconstructed)?;
    (tokens(payload.syntax()) == tokens(reconstructed.syntax())).then_some(line)
}

fn result_receiver(receiver: &ast::Expr, owner: &FunctionSummary, index: &RustIndex) -> Option<()> {
    let ast::Expr::MethodCallExpr(call) = receiver else {
        return None;
    };
    if tokens(call.receiver()?.syntax()) != "self" {
        return None;
    }
    let name = call.name_ref()?.text().to_string();
    let mut declarations = index.functions().iter().filter(|candidate| {
        candidate.name == name
            && candidate.file == owner.file
            && candidate.item.has_self_param
            && candidate.item.container == owner.item.container
    });
    let declaration = declarations.next()?;
    if declarations.next().is_some() || !standard_result(&function(&declaration.body)?) {
        return None;
    }
    Some(())
}
fn standard_result(function: &ast::Fn) -> bool {
    function
        .ret_type()
        .and_then(|returned| returned.ty())
        .is_some_and(|ty| {
            let text = tokens(ty.syntax());
            ["Result<", "std::result::Result<", "core::result::Result<"]
                .iter()
                .any(|head| text.starts_with(head))
        })
}
fn function(source: &str) -> Option<ast::Fn> {
    parse_clean_source_file(source)?
        .tree()
        .syntax()
        .children()
        .find_map(ast::Fn::cast)
}
fn expression(source: &str) -> Option<ast::Expr> {
    function(&format!("fn ripr_result_map() {{ {source} }}"))?
        .body()?
        .tail_expr()
}
fn tokens(node: &SyntaxNode) -> String {
    node.descendants_with_tokens()
        .filter_map(|element| element.into_token())
        .filter(|token| !token.kind().is_trivia())
        .map(|token| token.text().to_string())
        .collect()
}
