//! Per-byte loop counters on immutable slice inputs (#5346, grid-bits-length-pinned).
//! When tests call the owner with literal slice or byte-string arguments whose
//! lengths become the counter after one increment per element, bracket the
//! changed boundary without treating repeat lengths as direct scalar inputs.

use super::activation::{ActivationWithBoundaryInput, comparison_operands, sort_value_facts};
use super::context::ProbeContext;
use super::owner_pin::{OwnerPinSyntax, targeted_free_call_reaches_owner};
use super::reach::relation_establishes_reach;
use super::slice_boundary::slice_argument_byte_length;
use crate::analysis::facts::{
    FunctionImplContext, FunctionSummary, OracleFact, RustIndex, TestSummary,
};
use crate::analysis::syntax::{owner_reference_count, parse_clean_source_file};
use crate::domain::{
    Confidence, Probe, ProbeFamily, StageEvidence, StageState, SymbolId, ValueContext,
};
use crate::value_origins::LocatedValueFact as ValueFact;
use ra_ap_syntax::{
    AstNode,
    ast::{self, HasAttrs, HasLoopBody, HasName},
};
use std::path::PathBuf;

const MAX_ROWS: usize = 64;

pub(in crate::analysis) struct SliceCounterBoundary {
    owner: SymbolId,
    expression: String,
    predicate_line: usize,
    counter: String,
    rows: Vec<SliceCounterRow>,
}

struct SliceCounterRow {
    count: usize,
    test_file: PathBuf,
    call_line: usize,
    call_text: String,
}

impl SliceCounterBoundary {
    fn valid_for(&self, probe: &Probe, owner: &FunctionSummary) -> bool {
        probe.family == ProbeFamily::Predicate
            && probe.owner.as_ref() == Some(&self.owner)
            && owner.id == self.owner
            && probe.location.line == self.predicate_line
            && probe.expression == self.expression
    }

    pub(in crate::analysis) fn infection(
        &self,
        probe: &Probe,
        owner: &FunctionSummary,
    ) -> Option<StageEvidence> {
        if !self.valid_for(probe, owner) {
            return None;
        }
        let mut saw_true = false;
        let mut saw_false = false;
        for row in &self.rows {
            match evaluates_predicate(&self.expression, &self.counter, row.count) {
                Some(true) => saw_true = true,
                Some(false) => saw_false = true,
                None => return None,
            }
        }
        (saw_true && saw_false).then(|| {
            StageEvidence::new(
                StageState::Yes,
                Confidence::Medium,
                "Literal slice inputs establish both sides of the per-byte counter boundary",
            )
        })
    }

    pub(in crate::analysis) fn pairs(
        &self,
        probe: &Probe,
        owner: &FunctionSummary,
        tests: &[&TestSummary],
        index: &RustIndex,
        assertion_admitted: &dyn Fn(&TestSummary, &OracleFact) -> bool,
        owner_pinned: &dyn Fn(&TestSummary, &OracleFact) -> bool,
    ) -> bool {
        self.valid_for(probe, owner)
            && self.rows.iter().any(|row| {
                tests.iter().any(|test| {
                    test.file == row.test_file
                        && index.files().get(&test.file).is_some_and(|_| {
                            test.assertions.iter().any(|assertion| {
                                assertion_admitted(test, assertion)
                                    && (owner_pinned(test, assertion)
                                        || assertion.text.contains(&format!("{}(", owner.name)))
                                    && assertion.text.contains(row.call_text.as_str())
                            })
                        })
                })
            })
    }
}

pub(in crate::analysis) fn refine_slice_counter_boundary(
    context: &ProbeContext<'_>,
    syntax: &OwnerPinSyntax,
    gathered: &mut ActivationWithBoundaryInput,
) {
    let Some(boundary) = boundary(context, syntax) else {
        return;
    };
    gathered.unresolved_boundary = None;
    gathered.activation.missing_discriminators.retain(|fact| {
        !fact.value.contains(&boundary.counter) && !fact.reason.contains("boundary operand")
    });
    for row in &boundary.rows {
        gathered.activation.observed_values.push(ValueFact {
            file: Some(row.test_file.clone()),
            line: row.call_line,
            text: format!(
                "{} | slice input byte length {} maps to counter `{}` after one increment per element",
                row.call_text, row.count, boundary.counter
            ),
            value: format!("{} = {}", boundary.counter, row.count),
            context: ValueContext::FunctionArgument,
        });
    }
    sort_value_facts(&mut gathered.activation.observed_values);
    gathered.slice_counter_boundary = Some(boundary);
}

fn boundary(context: &ProbeContext<'_>, syntax: &OwnerPinSyntax) -> Option<SliceCounterBoundary> {
    let owner = context.owner_fn?;
    if context.probe.family != ProbeFamily::Predicate
        || context.probe.owner.as_ref() != Some(&owner.id)
        || !context.workspace_complete
        || context.helper_chain.is_some()
        || owner.impl_context != FunctionImplContext::Free
        || !owner.attrs.is_empty()
    {
        return None;
    }
    let (slice_param, counter) = per_byte_counter_shape(owner, &context.probe.expression)?;
    let slice_position = immutable_slice_parameter(owner, &slice_param)?;
    let mut rows = Vec::new();
    let mut admitted_calls = 0usize;
    for (test, reason) in &context.related_tests {
        if !relation_establishes_reach(*reason, true) {
            continue;
        }
        if !targeted_free_call_reaches_owner(test, owner, None, context.index, syntax) {
            continue;
        }
        for call in test.body_calls().filter(|call| call.name == owner.name) {
            let arguments = super::activation::call_arguments(&call.text, &call.name)?;
            let argument = arguments.get(slice_position)?.trim();
            let Some(length) = slice_argument_byte_length(argument) else {
                return None;
            };
            if length == 0 {
                return None;
            }
            rows.push(SliceCounterRow {
                count: length,
                test_file: test.file.clone(),
                call_line: call.line,
                call_text: call.text.to_string(),
            });
            admitted_calls += 1;
            if rows.len() > MAX_ROWS {
                return None;
            }
        }
    }
    if rows.is_empty() {
        return None;
    }
    let mut reference_files = std::collections::HashSet::new();
    reference_files.insert(owner.file.clone());
    for row in &rows {
        reference_files.insert(row.test_file.clone());
    }
    let references = reference_files.iter().try_fold(0usize, |count, path| {
        let Some(facts) = context.index.files().get(path) else {
            return None;
        };
        count.checked_add(owner_reference_count(&facts.source, &owner.name)?)
    });
    if let (Some(references), Some(expected)) = (references, admitted_calls.checked_add(1))
        && references != expected
    {
        return None;
    }
    rows.sort_by_key(|row| row.count);
    rows.dedup_by_key(|row| row.count);
    Some(SliceCounterBoundary {
        owner: owner.id.clone(),
        expression: context.probe.expression.clone(),
        predicate_line: context.probe.location.line,
        counter,
        rows,
    })
}

fn per_byte_counter_shape(owner: &FunctionSummary, expression: &str) -> Option<(String, String)> {
    let parse = parse_clean_source_file(&owner.body)?;
    let function = parse.tree().syntax().children().find_map(ast::Fn::cast)?;
    if function.async_token().is_some() || function.attrs().next().is_some() {
        return None;
    }
    let (left, right) = comparison_operands(expression)?;
    let counter = if is_simple_identifier(&left) && !is_simple_identifier(&right) {
        left
    } else if is_simple_identifier(&right) && !is_simple_identifier(&left) {
        right
    } else {
        return None;
    };
    let statements = function
        .body()?
        .stmt_list()?
        .statements()
        .collect::<Vec<_>>();
    let mut slice_param = None;
    let mut counter_binding = None;
    for statement in &statements {
        let ast::Stmt::LetStmt(binding) = statement else {
            continue;
        };
        if binding.attrs().next().is_some() || binding.let_else().is_some() {
            return None;
        }
        let ast::Pat::IdentPat(local) = binding.pat()? else {
            continue;
        };
        if local.mut_token().is_none() {
            continue;
        }
        let init = compact(&binding.initializer()?.syntax().text().to_string());
        if init == "0" {
            counter_binding = Some(local.name()?.text().to_string());
        }
    }
    if counter_binding.as_deref() != Some(counter.as_str()) {
        return None;
    }
    for statement in &statements {
        let ast::Stmt::ExprStmt(expr_stmt) = statement else {
            continue;
        };
        let ast::Expr::ForExpr(for_loop) = expr_stmt.expr()? else {
            continue;
        };
        let pat = for_loop.pat()?.syntax().text().to_string();
        let pat = compact(&pat);
        if pat != "_" && pat != "b" && pat != "&b" {
            return None;
        }
        let iterable = for_loop.iterable()?;
        let ast::Expr::PathExpr(path) = iterable else {
            return None;
        };
        slice_param = Some(path.path()?.syntax().text().to_string());
        let body = for_loop.loop_body()?;
        let body_text = compact(&body.syntax().text().to_string());
        if !body_text.contains(&format!("{counter}+=1"))
            && !body_text.contains(&format!("{counter}+= 1"))
            && !body_text.contains(&format!("{counter} +=1"))
            && !body_text.contains(&format!("{counter} += 1"))
        {
            return None;
        }
    }
    let slice = slice_param?;
    Some((slice, counter))
}

fn immutable_slice_parameter(owner: &FunctionSummary, name: &str) -> Option<usize> {
    let parse = parse_clean_source_file(&owner.body)?;
    let function = parse.tree().syntax().children().find_map(ast::Fn::cast)?;
    let params = function.param_list()?;
    if params.self_param().is_some() {
        return None;
    }
    params.params().enumerate().find_map(|(position, param)| {
        let ast::Pat::IdentPat(pattern) = param.pat()? else {
            return None;
        };
        if pattern.name()?.text() != name
            || pattern.mut_token().is_some()
            || pattern.ref_token().is_some()
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

fn is_simple_identifier(text: &str) -> bool {
    !text.is_empty()
        && text
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
        && !text.starts_with(|ch: char| ch.is_ascii_digit())
}

fn compact(text: &str) -> String {
    text.chars()
        .filter(|character| !character.is_whitespace())
        .collect()
}

fn evaluates_predicate(expression: &str, counter: &str, value: usize) -> Option<bool> {
    let (left, right) = comparison_operands(expression)?;
    let substitute = |operand: &str| -> Option<usize> {
        if operand == counter {
            Some(value)
        } else {
            operand.parse().ok()
        }
    };
    let left = substitute(&left)?;
    let right = substitute(&right)?;
    if expression.contains("<=") {
        return Some(left <= right);
    }
    if expression.contains(">=") {
        return Some(left >= right);
    }
    if expression.contains("==") {
        return Some(left == right);
    }
    if expression.contains("!=") {
        return Some(left != right);
    }
    if expression.contains('<') {
        return Some(left < right);
    }
    if expression.contains('>') {
        return Some(left > right);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::facts::FunctionSourceRole;
    use std::path::PathBuf;

    fn owner_body(body: &str) -> FunctionSummary {
        FunctionSummary {
            id: SymbolId("src/lib.rs::chunk_size".to_string()),
            name: "chunk_size".to_string(),
            file: PathBuf::from("src/lib.rs"),
            start_line: 10,
            end_line: 22,
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
            impl_context: FunctionImplContext::Free,
        }
    }

    #[test]
    fn grid_bits_owner_shape_is_recognized() -> Result<(), ()> {
        let body = "pub fn chunk_size(hex: &[u8]) -> Result<u64, ChunkError> {\n    let mut size: u64 = 0;\n    let mut count = 0;\n    for &b in hex {\n        count += 1;\n        if 16 < count {\n            return Err(ChunkError::TooLong);\n        }\n        let digit = (b as char).to_digit(16).ok_or(ChunkError::BadDigit)?;\n        size = (size << 4) | u64::from(digit);\n    }\n    Ok(size)\n}";
        let owner = owner_body(body);
        let (slice, counter) = per_byte_counter_shape(&owner, "16 < count").ok_or(())?;
        assert_eq!(slice, "hex");
        assert_eq!(counter, "count");
        assert_eq!(immutable_slice_parameter(&owner, "hex"), Some(0));
        Ok(())
    }
}
