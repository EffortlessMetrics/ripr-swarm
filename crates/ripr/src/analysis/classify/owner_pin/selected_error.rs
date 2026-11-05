//! A unit-error pattern observes the first return only on its literal guard row.
//!
//! This is not an owner-wide variant pin: a later identical return does not
//! borrow the first branch's input. Reuse binding, call and value authorities.
use super::super::value_transfer::{EvalOutcome, ExactInputs, TypedValue, evaluate_initializer};
use super::*;
use crate::analysis::syntax::parse_clean_source_file_memoized as parse_clean_source_file;

pub(in crate::analysis) fn selected_unit_error_return(
    probe: &Probe,
    owner: &FunctionSummary,
    test: &TestSummary,
    assertion: &OracleFact,
    index: &RustIndex,
    syntax: &OwnerPinSyntax,
) -> bool {
    selected(probe, owner, test, assertion, index, syntax).is_some()
}

fn selected(
    probe: &Probe,
    owner: &FunctionSummary,
    test: &TestSummary,
    assertion: &OracleFact,
    index: &RustIndex,
    syntax: &OwnerPinSyntax,
) -> Option<()> {
    if !matches!(
        probe.family,
        ProbeFamily::ReturnValue | ProbeFamily::ErrorPath
    ) || owner.item.container != FunctionContainer::Free
        || !owner.attrs.is_empty()
        || !syntax.admits_equality_assertion(probe, test, assertion, index)
    {
        return None;
    }
    let condition = matches_pin_condition(&assertion.text)?;
    let args = condition
        .trim()
        .strip_prefix("matches!")?
        .trim()
        .strip_prefix('(')?
        .strip_suffix(')')?;
    let args = split_top_level_arguments(args)?;
    let [subject, pattern] = args.as_slice() else {
        return None;
    };
    let (qualifier, variant) = error_variant_key(pattern)?;
    let compact = |text: &str| text.split_whitespace().collect::<String>();
    if compact(pattern) != format!("Err({qualifier}::{variant})")
        || !owner_file_names_enum(&qualifier, owner, index)
        || declaring_file_rebinds(
            &index.files().get(&test.file)?.source,
            &qualifier,
            true,
            index,
        )
        || source_binds_value_name(&index.files().get(&owner.file)?.source, "Err")
        || source_binds_value_name(&index.files().get(&test.file)?.source, "Err")
        || !super::super::element_field_pin::drop_is_plain(index, &qualifier)
    {
        return None;
    }
    // Pattern matching needs the actual unit variant, not PartialEq metadata.
    let declaring = index
        .files()
        .values()
        .filter(|file| declares_type(&mask_comments_and_strings(&file.source), &qualifier))
        .collect::<Vec<_>>();
    let [declaring] = declaring.as_slice() else {
        return None;
    };
    let parsed = parse_clean_source_file(&declaring.source)?;
    let mut enums = parsed
        .tree()
        .syntax()
        .descendants()
        .filter_map(ast::Enum::cast)
        .filter(|item| {
            item.name()
                .is_some_and(|name| name.text() == qualifier.as_str())
        });
    let (Some(item), None) = (enums.next(), enums.next()) else {
        return None;
    };
    if !item.variant_list()?.variants().any(|value| {
        value
            .name()
            .is_some_and(|name| name.text() == variant.as_str())
            && value.field_list().is_none()
            && value.attrs().next().is_none()
    }) {
        return None;
    }
    let call_text = if is_plain_identifier(subject.trim()) {
        let_bound_owner_call(
            test,
            assertion,
            subject.trim(),
            "",
            &owner.name,
            LetObservation::UnitPattern,
            true,
        )?
    } else {
        subject.trim()
    };
    if !is_whole_free_owner_call(call_text, &owner.name)
        || !super::super::call_identity::bound_free_owner_calls(
            index, owner, test, call_text, syntax,
        )
    {
        return None;
    }
    let parsed_call = parse_clean_source_file(&format!("fn t() {{ {call_text} }}"))?;
    let call = parsed_call
        .tree()
        .syntax()
        .descendants()
        .find_map(ast::CallExpr::cast)?;
    let arguments = call.arg_list()?.args().collect::<Vec<_>>();
    let parsed = parse_clean_source_file(&owner.body)?;
    let function = parsed.tree().syntax().children().find_map(ast::Fn::cast)?;
    if function.async_token().is_some()
        || function.generic_param_list().is_some()
        || function.attrs().next().is_some()
    {
        return None;
    }
    let params = function.param_list()?.params().collect::<Vec<_>>();
    if params.len() != arguments.len() || params.is_empty() {
        return None;
    }
    let mut inputs = ExactInputs::new();
    for (param, argument) in params.iter().zip(&arguments) {
        let ast::Pat::IdentPat(name) = param.pat()? else {
            return None;
        };
        if name.mut_token().is_some()
            || name.ref_token().is_some()
            || name.pat().is_some()
            || compact(&param.ty()?.syntax().text().to_string()) != "&str"
            || !matches!(argument, ast::Expr::Literal(value) if matches!(value.kind(), ast::LiteralKind::String(_)))
        {
            return None;
        }
        inputs.insert(
            name.name()?.text().to_string(),
            argument.syntax().text().to_string(),
        );
    }
    let body = function.body()?.stmt_list()?;
    let ast::Stmt::ExprStmt(statement) = body.statements().next()? else {
        return None;
    };
    let ast::Expr::IfExpr(branch) = statement.expr()? else {
        return None;
    };
    if branch.else_branch().is_some()
        || branch
            .syntax()
            .descendants()
            .any(|node| ast::Attr::can_cast(node.kind()))
    {
        return None;
    }
    let returned = branch.then_branch()?.stmt_list()?;
    if returned.tail_expr().is_some() {
        return None;
    }
    let statements = returned.statements().collect::<Vec<_>>();
    let [ast::Stmt::ExprStmt(statement)] = statements.as_slice() else {
        return None;
    };
    let ast::Expr::ReturnExpr(returned) = statement.expr()? else {
        return None;
    };
    let line = owner.start_line
        + LineIndex::new(&owner.body).line(returned.syntax().text_range().start())
        - 1;
    if probe.location.line != line
        || error_variant_key(&returned.expr()?.syntax().text().to_string())
            != Some((qualifier, variant))
        || compact(&returned.syntax().text().to_string())
            != compact(probe.expression.trim_end_matches(';'))
    {
        return None;
    }
    if wrapper_may_be_rebound("str", index)
        || !super::super::call_identity::closed_method_dispatch_imports(index)
        || index.files().values().any(|file| {
            file.source.contains("no_implicit_prelude")
                || super::super::macro_item_names::macro_may_define_item_named(&file.source, "str")
                || super::super::macro_item_names::file_has_non_local_defining_macro_invocation(
                    &file.source,
                )
        })
    {
        return None;
    }
    matches!(
        evaluate_initializer(&branch.condition()?.syntax().text().to_string(), &inputs),
        EvalOutcome::Exact {
            value: TypedValue::Bool(true),
            ..
        }
    )
    .then_some(())
}
