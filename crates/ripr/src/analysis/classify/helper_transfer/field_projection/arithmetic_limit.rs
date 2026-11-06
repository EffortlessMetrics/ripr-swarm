//! Name an exact arithmetic transfer without claiming it preserves distinctions.

use super::*;
use crate::analysis::classify::owner_pin::OwnerReturnPin;

pub(in crate::analysis) const ARITHMETIC_FIELD_LIMIT_PREFIX: &str =
    "Transformed-field arithmetic unproved:";

pub(in crate::analysis) fn transformed_field_arithmetic_limitation(
    context: &ProbeContext<'_>,
    syntax: &OwnerPinSyntax,
    witness: &PropagationWitnessV1,
) -> Option<String> {
    let probe = context.probe;
    if probe.family != ProbeFamily::FieldConstruction
        || probe.delta.as_str() != "value"
        || !context.workspace_complete
        || !complete_direct_witness(probe, Some(witness))
    {
        return None;
    }
    let owner = context.owner_fn?;
    let facts = context.index.files().get(&owner.file)?;
    if facts.used_lexical_fallback {
        return None;
    }
    let parse = parse_clean_source_file(&facts.source)?;
    let file = parse.tree();
    let owner_fn = root_function(&file, owner)?;
    let ast::Expr::RecordExpr(record) = sole_tail(&owner_fn)? else {
        return None;
    };
    let ty = record.path()?.syntax().text().to_string();
    if !super::super::is_identifier(&ty)
        || return_type(&owner_fn)? != ty
        || !OwnerReturnPin::returned_types_drop_plain(owner, context.index)
    {
        return None;
    }
    let changed = record.record_expr_field_list()?.fields().find(|field| {
        let offset = usize::from(field.syntax().text_range().start());
        1 + facts.source[..offset].matches('\n').count() == probe.location.line
            && squeeze(&field.syntax().text().to_string()) == squeeze(&probe.expression)
    })?;
    if changed.attrs().next().is_some() {
        return None;
    }
    let field = changed.field_name()?.text().to_string();
    if field_builtin_integer(owner, &ty, &field, context.index, syntax)? != "u32" {
        return None;
    }
    let parameters: Vec<_> = owner_fn.param_list()?.params().collect();
    if !parameters
        .iter()
        .all(|parameter| parameter.ty().is_some_and(|ty| ty.syntax().text() == "u32"))
    {
        return None;
    }
    let arity = parameters.len();
    // Only an actual nested-call observer needs global name membership.
    // The local cell scans once per probe, never per assertion; no cache escapes.
    let functions = std::cell::OnceCell::new();
    let mut selected = None;
    for (test, reason) in &context.related_tests {
        if *reason != RelationReason::DirectOwnerCall
            || test.file != owner.file
            || test.attrs.len() != 1
            || squeeze(&test.attrs[0]) != "#[test]"
            || !targeted_free_call_reaches_owner(test, owner, None, context.index, syntax)
        {
            continue;
        }
        let Some((binding, wrapper_name, assertion)) = observation(test, owner, arity, &field)
        else {
            continue;
        };
        let functions = functions.get_or_init(|| {
            let mut names = std::collections::BTreeMap::<&str, Vec<&FunctionSummary>>::new();
            for function in context.index.functions().iter() {
                names.entry(&function.name).or_default().push(function);
            }
            names
        });
        if functions.get(owner.name.as_str())?.len() != 1 {
            return None;
        }
        let Some(candidates) = functions.get(wrapper_name.as_str()) else {
            continue;
        };
        let [helper] = candidates.as_slice() else {
            continue;
        };
        if helper.file != owner.file
            || !targeted_free_call_reaches_owner(test, helper, None, context.index, syntax)
            || !syntax.admits(test, assertion, context.index)
        {
            continue;
        }
        let Some(transform) = transform(&file, helper, &ty, &field) else {
            continue;
        };
        // Keep a single observer path; another test never supplies missing
        // identity or expected-value independence for this assertion.
        selected = Some((helper.name.clone(), binding, transform, test, assertion));
        break;
    }
    let (helper, binding, transform, test, assertion) = selected?;
    Some(format!(
        "{ARITHMETIC_FIELD_LIMIT_PREFIX} `{}` field `{field}` reaches `{helper}` via `{binding}` and the existing assertion `{}` at {}:{} through `{transform}`; preservation of distinct u32 field values is unestablished because the overflow configuration is unknown (transformed_field_arithmetic_unresolved)",
        owner.name,
        assertion.text,
        test.file.display(),
        assertion.line
    ))
}

fn observation<'a>(
    test: &'a TestSummary,
    owner: &FunctionSummary,
    arity: usize,
    field: &str,
) -> Option<(String, String, &'a OracleFact)> {
    let parse = parse_clean_source_file(&test.body)?;
    let function = parse
        .tree()
        .syntax()
        .descendants()
        .find_map(ast::Fn::cast)?;
    let list = function.body()?.stmt_list()?;
    let statements: Vec<_> = list.statements().collect();
    let [ast::Stmt::LetStmt(binding), ast::Stmt::ExprStmt(oracle)] = statements.as_slice() else {
        return None;
    };
    if list.tail_expr().is_some()
        || binding.attrs().next().is_some()
        || binding.let_else().is_some()
    {
        return None;
    }
    let ast::Pat::IdentPat(pattern) = binding.pat()? else {
        return None;
    };
    if pattern.mut_token().is_some() || pattern.ref_token().is_some() || pattern.pat().is_some() {
        return None;
    }
    let name = pattern.name()?.text().to_string();
    let ast::Expr::CallExpr(wrapper) = binding.initializer()? else {
        return None;
    };
    let wrapper_name = wrapper.expr()?.syntax().text().to_string();
    if !super::super::is_identifier(&wrapper_name) {
        return None;
    }
    let args: Vec<_> = wrapper.arg_list()?.args().collect();
    let [ast::Expr::CallExpr(call)] = args.as_slice() else {
        return None;
    };
    if call.expr()?.syntax().text() != owner.name.as_str() {
        return None;
    }
    let args: Vec<_> = call.arg_list()?.args().collect();
    if args.len() != arity
        || !args
            .iter()
            .all(|arg| u32_literal(&arg.syntax().text().to_string()))
    {
        return None;
    }
    let text = oracle.expr()?.syntax().text().to_string();
    let operands = super::super::super::reveal::macro_arguments(text.strip_prefix("assert_eq!")?);
    let [left, right] = operands.as_slice() else {
        return None;
    };
    let projected = format!("{name}.{field}");
    if ![(*left, *right), (*right, *left)]
        .iter()
        .any(|(value, expected)| squeeze(value) == projected && u32_literal(expected.trim()))
    {
        return None;
    }
    let assertion = test.assertions.iter().find(|assertion| {
        assertion.kind == OracleKind::ExactValue
            && assertion.strength == OracleStrength::Strong
            && squeeze(assertion.text.trim_end_matches(';')) == squeeze(&text)
    })?;
    Some((
        squeeze(&wrapper.syntax().text().to_string()),
        wrapper_name,
        assertion,
    ))
}

fn transform(
    file: &ast::SourceFile,
    helper: &FunctionSummary,
    ty: &str,
    field: &str,
) -> Option<String> {
    let function = root_function(file, helper)?;
    let parameters: Vec<_> = function.param_list()?.params().collect();
    let [parameter] = parameters.as_slice() else {
        return None;
    };
    if return_type(&function)? != ty || parameter.ty()?.syntax().text() != ty {
        return None;
    }
    let ast::Pat::IdentPat(pattern) = parameter.pat()? else {
        return None;
    };
    if pattern.mut_token().is_some() || pattern.ref_token().is_some() || pattern.pat().is_some() {
        return None;
    }
    let name = pattern.name()?.text().to_string();
    let ast::Expr::RecordExpr(record) = sole_tail(&function)? else {
        return None;
    };
    if record.path()?.syntax().text() != ty {
        return None;
    }
    let list = record.record_expr_field_list()?;
    if list.spread()?.syntax().text() != name.as_str() {
        return None;
    }
    let fields: Vec<_> = list.fields().collect();
    let [assigned] = fields.as_slice() else {
        return None;
    };
    if assigned.attrs().next().is_some() || assigned.field_name()?.text() != field {
        return None;
    }
    let expression = assigned.expr()?;
    let ast::Expr::BinExpr(sum) = &expression else {
        return None;
    };
    if sum.op_kind() != Some(ast::BinaryOp::ArithOp(ast::ArithOp::Add)) {
        return None;
    }
    let projected = format!("{name}.{field}");
    if squeeze(&sum.lhs()?.syntax().text().to_string()) != projected {
        return None;
    }
    let ast::Expr::BinExpr(division) = sum.rhs()? else {
        return None;
    };
    if division.op_kind() != Some(ast::BinaryOp::ArithOp(ast::ArithOp::Div))
        || squeeze(&division.lhs()?.syntax().text().to_string()) != projected
    {
        return None;
    }
    let denominator = division.rhs()?.syntax().text().to_string();
    if !integer_literal(&denominator) || denominator.parse::<u32>().ok()? == 0 {
        return None;
    }
    Some(squeeze(&expression.syntax().text().to_string()))
}

fn u32_literal(text: &str) -> bool {
    integer_literal(text) && text.parse::<u32>().is_ok()
}
