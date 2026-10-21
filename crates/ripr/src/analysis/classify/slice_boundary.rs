//! Cardinality of immutable literal slice tables, not evaluation of their elements.
//! A computed/hidden input never becomes a missing equality from a partial table.

use super::activation::{ActivationWithBoundaryInput, comparison_operands};
use super::context::ProbeContext;
use super::owner_pin::OwnerPinSyntax;
use super::reach::relation_establishes_reach;
use super::reveal::contains_as_whole_word;
use crate::analysis::rust_index::FunctionSummary;
use crate::analysis::syntax::{
    constant_table_call_column, direct_literal_call_column, owner_reference_count,
    parse_clean_source_file,
};
use crate::domain::{
    FlowSinkFact, FlowSinkKind, MissingDiscriminatorFact, ProbeFamily, RelationReason,
    ValueContext, ValueFact,
};
use ra_ap_syntax::{
    AstNode,
    ast::{self, HasGenericParams, HasName},
};

struct BoundaryValues {
    operand: String,
    boundary: String,
    target: usize,
    lengths: Vec<usize>,
    witness: Option<(usize, String)>,
}

pub(in crate::analysis) fn refine_slice_boundary(
    context: &ProbeContext<'_>,
    syntax: &OwnerPinSyntax,
    gathered: &mut ActivationWithBoundaryInput,
    flow_sinks: &[FlowSinkFact],
) {
    let Some(values) = boundary_values(context, syntax) else {
        return;
    };
    match values {
        Err(reason) => gathered.unresolved_boundary = Some(reason),
        Ok(values) => {
            gathered.unresolved_boundary = None;
            let equality = format!("{} == {}", values.operand, values.boundary);
            if let Some((line, source)) = values.witness {
                gathered.activation.observed_values.push(ValueFact {
                    file: None,
                    line,
                    text: format!(
                        "{source} | {} = {} via immutable literal slice table cardinality",
                        values.operand, values.target
                    ),
                    value: equality,
                    context: ValueContext::FunctionArgument,
                });
            } else {
                let observed = values
                    .lengths
                    .iter()
                    .map(usize::to_string)
                    .collect::<Vec<_>>()
                    .join(", ");
                gathered.activation.missing_discriminators.push(MissingDiscriminatorFact {
                    value: equality,
                    reason: format!("No related test call uses {} equal to {}; observed {} values: {observed}; target {} value: {}", values.operand, values.boundary, values.operand, values.boundary, values.target),
                    flow_sink: flow_sinks.iter().find(|sink| sink.kind != FlowSinkKind::Unknown).cloned(),
                });
            }
        }
    }
    super::activation::sort_value_facts(&mut gathered.activation.observed_values);
}

fn boundary_values(
    context: &ProbeContext<'_>,
    syntax: &OwnerPinSyntax,
) -> Option<Result<BoundaryValues, String>> {
    let probe = context.probe;
    if probe.family != ProbeFamily::Predicate {
        return None;
    }
    let owner = context.owner_fn?;
    let (left, right) = comparison_operands(&probe.expression)?;
    let (operand, boundary) = if left.ends_with(".len()") {
        (left, right)
    } else if right.ends_with(".len()") {
        (right, left)
    } else {
        return None;
    };
    let parameter = operand.strip_suffix(".len()")?;
    let target = boundary.parse::<usize>().ok()?;
    let position = slice_parameter(owner, parameter)?;
    let unresolved = || {
        format!(
            "slice length `{operand}` is outside the bounded eager immutable literal table family; not every input cardinality is established"
        )
    };
    if !matches!(
        owner.impl_context,
        crate::analysis::facts::FunctionImplContext::Free
    ) || !context.workspace_complete
        || !super::helper_transfer::callee_is_unique(&owner.name, context.index)
        || super::helper_transfer::direct_callers(&owner.name, context.index)
            .iter()
            .any(|caller| !caller.source_role.registers_executable_test())
    {
        return Some(Err(unresolved()));
    }
    let mut lengths = Vec::new();
    let mut witness = None;
    let mut admitted_calls = 0usize;
    for (test, reason) in &context.related_tests {
        if !relation_establishes_reach(*reason, true) {
            if contains_as_whole_word(&test.body, &owner.name) {
                return Some(Err(unresolved()));
            }
            continue;
        }
        if *reason != RelationReason::DirectOwnerCall {
            return Some(Err(unresolved()));
        }
        if !super::owner_pin::targeted_free_call_reaches_owner(
            test,
            owner,
            None,
            context.index,
            syntax,
        ) {
            return Some(Err(unresolved()));
        }
        // A line at the call-fact budget may have lost the owner's call, so
        // the single-call absence proof below cannot be trusted.
        if crate::analysis::extract::has_saturated_line(test.body_calls()) {
            return Some(Err(unresolved()));
        }
        let calls: Vec<_> = test
            .body_calls()
            .filter(|call| call.name == owner.name)
            .collect();
        if calls.is_empty() {
            return Some(Err(unresolved()));
        }
        // Either one ordinary call over a constant table, or straight-line
        // direct calls whose slice argument is a literal in each assertion.
        // Additional, macro-hidden or helper calls must not disappear from the
        // absence proof.
        let table = match calls.as_slice() {
            [only] => constant_table_call_column(&test.body, &owner.name, position)
                .map(|(column, text, macro_count)| (column, text, macro_count, only.line)),
            _ => None,
        };
        let (rows, macro_count): (Vec<(String, usize, String)>, usize) = match table {
            Some((column, text, macro_count, line)) => (
                column
                    .into_iter()
                    .map(|cell| (cell, line, text.clone()))
                    .collect(),
                macro_count,
            ),
            None => {
                let Some((column, macro_count)) =
                    direct_literal_call_column(&test.body, &owner.name, position)
                else {
                    return Some(Err(unresolved()));
                };
                if column.len() != calls.len() {
                    return Some(Err(unresolved()));
                }
                let mut sites: Vec<_> = calls.iter().map(|call| (call.line, &call.text)).collect();
                sites.sort_by_key(|(line, _)| *line);
                let rows = column
                    .into_iter()
                    .zip(sites)
                    .map(|(cell, (line, text))| (cell, line, text.to_string()))
                    .collect();
                (rows, macro_count)
            }
        };
        let assertions: Vec<_> = test
            .assertions
            .iter()
            .filter(|assertion| {
                ["assert!(", "assert_eq!(", "assert_ne!("]
                    .iter()
                    .any(|prefix| assertion.text.starts_with(prefix))
            })
            .collect();
        if assertions.len() != macro_count
            || assertions
                .iter()
                .any(|assertion| !syntax.admits(test, assertion, context.index))
        {
            return Some(Err(unresolved()));
        }
        for (cell, line, source) in rows {
            let Some(length) = literal_slice_length(&cell) else {
                return Some(Err(unresolved()));
            };
            if length == target && witness.is_none() {
                witness = Some((line, source));
            }
            lengths.push(length);
        }
        // The table path is one call; direct calls are each a use.
        admitted_calls = admitted_calls.checked_add(calls.len())?;
    }
    if lengths.is_empty() {
        return Some(Err(unresolved()));
    }
    let references = context
        .index
        .files()
        .values()
        .try_fold(0usize, |count, facts| {
            count.checked_add(owner_reference_count(&facts.source, &owner.name)?)
        });
    // One declaration plus the exact ordinary calls admitted above. Never
    // interpret an additional identifier use as an already-read input.
    if references != admitted_calls.checked_add(1) {
        return Some(Err(unresolved()));
    }
    lengths.sort_unstable();
    lengths.dedup();
    Some(Ok(BoundaryValues {
        operand,
        boundary,
        target,
        lengths,
        witness,
    }))
}

/// Slice syntax establishes the intrinsic `.len()` receiver. A same-named
/// custom method, mutable/rebound parameter or shadowed local is outside it.
fn slice_parameter(owner: &FunctionSummary, name: &str) -> Option<usize> {
    let parse = parse_clean_source_file(&owner.body)?;
    let function = parse
        .tree()
        .syntax()
        .descendants()
        .find_map(ast::Fn::cast)?;
    if function.generic_param_list().is_some()
        || !function
            .ret_type()
            .and_then(|result| result.ty())
            .is_some_and(|ty| {
                matches!(
                    ty.syntax().text().to_string().trim(),
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
                        | "f32"
                        | "f64"
                        | "bool"
                        | "char"
                )
            })
    {
        return None;
    }
    if function.async_token().is_some() {
        return None;
    }
    if function
        .syntax()
        .descendants()
        .any(|node| ast::MacroCall::can_cast(node.kind()))
    {
        return None;
    }
    let params = function.param_list()?;
    if params.self_param().is_some() {
        return None;
    }
    let bindings = function
        .syntax()
        .descendants()
        .filter_map(|node| ast::AnyHasName::cast(node).and_then(|named| named.name()))
        .filter(|bound| bound.text() == name)
        .count();
    if bindings != 1 {
        return None;
    }
    params.params().enumerate().find_map(|(position, param)| {
        let ast::Pat::IdentPat(pattern) = param.pat()? else {
            return None;
        };
        if pattern.name()?.text() != name
            || pattern.mut_token().is_some()
            || pattern.ref_token().is_some()
            || pattern.at_token().is_some()
        {
            return None;
        }
        let ast::Type::RefType(reference) = param.ty()? else {
            return None;
        };
        (reference.mut_token().is_none() && matches!(reference.ty(), Some(ast::Type::SliceType(_))))
            .then_some(position)
    })
}

pub(super) fn literal_slice_length(text: &str) -> Option<usize> {
    if text.len() > 4096 {
        return None;
    }
    let parse = parse_clean_source_file(&format!("fn input() {{ let value = {text}; }}"))?;
    let binding = parse
        .tree()
        .syntax()
        .descendants()
        .find_map(ast::LetStmt::cast)?;
    let ast::Expr::RefExpr(reference) = binding.initializer()? else {
        return None;
    };
    if reference.mut_token().is_some() || reference.raw_token().is_some() {
        return None;
    }
    let ast::Expr::ArrayExpr(array) = reference.expr()? else {
        return None;
    };
    if array.semicolon_token().is_some()
        || array
            .syntax()
            .descendants()
            .any(|node| ast::Attr::can_cast(node.kind()))
    {
        return None;
    }
    let elements: Vec<_> = array.exprs().collect();
    (elements.len() <= 256
        && elements.iter().all(|element| match element {
            ast::Expr::Literal(_) => true,
            ast::Expr::PrefixExpr(prefix) => {
                prefix.op_kind() == Some(ast::UnaryOp::Neg)
                    && matches!(prefix.expr(), Some(ast::Expr::Literal(_)))
            }
            _ => false,
        }))
    .then_some(elements.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::facts::FunctionSourceRole;
    use crate::domain::SymbolId;
    use std::path::PathBuf;

    fn owner_with_body(body: &str) -> FunctionSummary {
        FunctionSummary {
            id: SymbolId("src/lib.rs::mean".to_string()),
            name: "mean".to_string(),
            file: PathBuf::from("src/lib.rs"),
            start_line: 1,
            end_line: 3,
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

    #[test]
    fn literal_slice_length_counts_only_plain_immutable_literal_rows() {
        for (text, length) in [
            ("&[]", Some(0)),
            ("&[1, 2, 3]", Some(3)),
            ("&[-1, 2]", Some(2)),
            ("&[\"a\", \"b\"]", Some(2)),
            ("&mut [1]", None),
            ("&raw const [1]", None),
            ("[1, 2]", None),
            ("&[0; 4]", None),
            ("&[1 + 1]", None),
            ("&[x]", None),
            ("&[-x]", None),
            ("&[!1]", None),
            ("&[#[cfg(a)] 1]", None),
            ("&values", None),
        ] {
            assert_eq!(literal_slice_length(text), length, "{text}");
        }
    }

    #[test]
    fn literal_slice_length_enforces_element_and_size_bounds() {
        let at_limit = format!("&[{}]", vec!["1"; 256].join(", "));
        assert_eq!(literal_slice_length(&at_limit), Some(256));
        let over_elements = format!("&[{}]", vec!["1"; 257].join(", "));
        assert_eq!(literal_slice_length(&over_elements), None);
        let over_bytes = format!("&[\"{}\"]", "a".repeat(4096));
        assert_eq!(literal_slice_length(&over_bytes), None);
    }

    #[test]
    fn slice_parameter_finds_the_position_of_an_immutable_slice_reference() {
        let owner = owner_with_body("fn mean(scale: u64, xs: &[u64]) -> u64 { 0 }");
        assert_eq!(slice_parameter(&owner, "xs"), Some(1));
        assert_eq!(slice_parameter(&owner, "scale"), None);
        assert_eq!(slice_parameter(&owner, "missing"), None);
    }

    #[test]
    fn slice_parameter_refuses_shapes_that_change_the_len_receiver() {
        for body in [
            "fn mean(xs: &mut [u64]) -> u64 { 0 }",
            "fn mean(mut xs: &[u64]) -> u64 { 0 }",
            "fn mean(xs: &Vec<u64>) -> u64 { 0 }",
            "fn mean(xs: [u64; 2]) -> u64 { 0 }",
            "fn mean(ref xs: &[u64]) -> u64 { 0 }",
            "fn mean(xs: &[u64]) -> String { String::new() }",
            "fn mean(xs: &[u64]) { }",
            "fn mean<T>(xs: &[T]) -> u64 { 0 }",
            "async fn mean(xs: &[u64]) -> u64 { 0 }",
            "fn mean(&self, xs: &[u64]) -> u64 { 0 }",
            "fn mean(xs: &[u64]) -> u64 { let xs = &[1u64][..]; xs.len() as u64 }",
            "fn mean(xs: &[u64]) -> u64 { println!(\"x\"); 0 }",
            "not rust {",
        ] {
            assert_eq!(
                slice_parameter(&owner_with_body(body), "xs"),
                None,
                "{body}"
            );
        }
        let float = owner_with_body("fn mean(xs: &[f64]) -> f64 { 0.0 }");
        assert_eq!(slice_parameter(&float, "xs"), Some(0));
    }
}
