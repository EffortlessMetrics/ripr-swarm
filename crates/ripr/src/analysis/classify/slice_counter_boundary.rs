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
        let mut outcomes = Vec::with_capacity(self.rows.len());
        for row in &self.rows {
            let ever_true = predicate_ever_true(&self.expression, &self.counter, row.count)?;
            outcomes.push((row.count, ever_true));
        }
        adjacent_flip(&outcomes).then(|| {
            StageEvidence::new(
                StageState::Yes,
                Confidence::Medium,
                "Literal slice inputs at adjacent lengths flip the per-byte counter predicate",
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
    let body = function.body()?;
    let statements = body.stmt_list()?.statements().collect::<Vec<_>>();
    // Exactly one binding of the counter name among the top-level lets, and it
    // is `let mut <counter> = 0;`. An uninitialised or otherwise-initialised
    // counter, or a second binding of the name, fails closed. Other locals do
    // not matter here.
    let mut counter_bindings = 0usize;
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
        if local.name()?.text() != counter.as_str() {
            continue;
        }
        let zero_init = binding
            .initializer()
            .is_some_and(|init| compact(&init.syntax().text().to_string()) == "0");
        if local.mut_token().is_none() || !zero_init {
            return None;
        }
        counter_bindings += 1;
    }
    if counter_bindings != 1 {
        return None;
    }
    let function_node = function.syntax();
    // The counter is bound once: a shadowing `let`, a closure or loop pattern
    // naming it would make the increment count something else.
    let bindings = function_node
        .descendants()
        .filter_map(ast::IdentPat::cast)
        .filter(|pattern| {
            pattern
                .name()
                .is_some_and(|name| name.text() == counter.as_str())
        })
        .count();
    if bindings != 1 {
        return None;
    }
    // Exactly one loop in the function: the per-byte `for` over the slice.
    let loops = function_node
        .descendants()
        .filter(|node| {
            ast::ForExpr::can_cast(node.kind())
                || ast::WhileExpr::can_cast(node.kind())
                || ast::LoopExpr::can_cast(node.kind())
        })
        .count();
    if loops != 1 {
        return None;
    }
    let mut loop_statements = statements.iter().filter_map(|statement| match statement {
        ast::Stmt::ExprStmt(expr_stmt) => match expr_stmt.expr()? {
            ast::Expr::ForExpr(for_loop) => Some(for_loop),
            _ => None,
        },
        _ => None,
    });
    let for_loop = loop_statements.next()?;
    if loop_statements.next().is_some() {
        return None;
    }
    let pat = compact(&for_loop.pat()?.syntax().text().to_string());
    if pat != "_" && pat != "b" && pat != "&b" {
        return None;
    }
    let ast::Expr::PathExpr(path) = for_loop.iterable()? else {
        return None;
    };
    let slice = path.path()?.syntax().text().to_string();
    let loop_body = for_loop.loop_body()?;
    let loop_node = loop_body.syntax();
    // A `break` or `continue` can skip the increment or the predicate, so the
    // loop body's length no longer follows the slice's.
    if loop_node.descendants().any(|node| {
        ast::BreakExpr::can_cast(node.kind()) || ast::ContinueExpr::can_cast(node.kind())
    }) {
        return None;
    }
    // The counter is never borrowed mutably (it could change out of sight).
    if function_node
        .descendants()
        .filter_map(ast::RefExpr::cast)
        .any(|reference| {
            reference.mut_token().is_some()
                && reference
                    .expr()
                    .is_some_and(|inner| compact(&inner.syntax().text().to_string()) == counter)
        })
    {
        return None;
    }
    // Exactly one assignment to the counter in the whole function, and it is
    // the body's top-level `counter += 1;`, which runs once per element:
    // no conditional increment, reset or second update.
    let assignments = function_node
        .descendants()
        .filter_map(ast::BinExpr::cast)
        .filter(|expr| {
            matches!(expr.op_kind(), Some(ast::BinaryOp::Assignment { .. }))
                && expr
                    .lhs()
                    .is_some_and(|lhs| compact(&lhs.syntax().text().to_string()) == counter)
        })
        .collect::<Vec<_>>();
    let [assignment] = assignments.as_slice() else {
        return None;
    };
    let items = loop_items(&loop_body);
    let increment_text = format!("{counter}+=1");
    let increment_at = items.iter().position(|item| {
        top_level_expr(item).is_some_and(|expr| {
            expr.syntax() == assignment.syntax()
                && compact(&expr.syntax().text().to_string()) == increment_text
        })
    })?;
    // The predicate is a top-level `if` after the increment, so it sees the
    // counter at 1..=n, and nothing before it can leave the loop early.
    let predicate = compact(expression);
    let predicate_at =
        items
            .iter()
            .enumerate()
            .skip(increment_at + 1)
            .find_map(|(position, item)| match top_level_expr(item)? {
                ast::Expr::IfExpr(if_expr) => {
                    (compact(&if_expr.condition()?.syntax().text().to_string()) == predicate)
                        .then_some(position)
                }
                _ => None,
            })?;
    let exits_early = items.iter().take(predicate_at).any(|item| {
        item.descendants().any(|node| {
            ast::ReturnExpr::can_cast(node.kind()) || ast::TryExpr::can_cast(node.kind())
        })
    });
    if exits_early {
        return None;
    }
    Some((slice, counter))
}

/// The loop body's top-level statements and tail expression, in order.
fn loop_items(body: &ast::BlockExpr) -> Vec<ra_ap_syntax::SyntaxNode> {
    let Some(list) = body.stmt_list() else {
        return Vec::new();
    };
    let mut items = list
        .statements()
        .map(|statement| statement.syntax().clone())
        .collect::<Vec<_>>();
    if let Some(tail) = list.tail_expr() {
        items.push(tail.syntax().clone());
    }
    items
}

/// The expression a top-level item is, as a statement or the tail.
fn top_level_expr(item: &ra_ap_syntax::SyntaxNode) -> Option<ast::Expr> {
    match ast::ExprStmt::cast(item.clone()) {
        Some(statement) => statement.expr(),
        None => ast::Expr::cast(item.clone()),
    }
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

/// Whether two rows with adjacent lengths `k` and `k + 1` flip the predicate
/// from never-true to true. Non-adjacent lengths straddling the predicate do not
/// pin the boundary: neither could tell `64 < count` from `64 <= count`.
/// Rows are sorted by length and deduplicated.
fn adjacent_flip(outcomes: &[(usize, bool)]) -> bool {
    outcomes.windows(2).any(|pair| {
        let (low, low_true) = pair[0];
        let (high, high_true) = pair[1];
        high == low.saturating_add(1) && !low_true && high_true
    })
}

/// Whether the predicate is true for any counter value the loop reaches on a
/// slice of `length` elements: the increment precedes the predicate, so the
/// counter takes `1..=length`, not only its final value.
fn predicate_ever_true(expression: &str, counter: &str, length: usize) -> Option<bool> {
    let mut seen_true = false;
    for value in 1..=length {
        seen_true |= evaluates_predicate(expression, counter, value)?;
    }
    Some(seen_true)
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

    fn counter_body(loop_body: &str, after_loop: &str) -> String {
        format!(
            "pub fn chunk_size(hex: &[u8]) -> Result<u64, ChunkError> {{\n    let mut count = 0;\n    for &b in hex {{\n{loop_body}    }}\n{after_loop}    Ok(0)\n}}"
        )
    }

    fn shape(body: &str, expression: &str) -> Option<(String, String)> {
        per_byte_counter_shape(&owner_body(body), expression)
    }

    const GUARD: &str =
        "        if 16 < count {\n            return Err(ChunkError::TooLong);\n        }\n";

    #[test]
    fn an_unconditional_increment_before_the_predicate_is_the_accepted_shape() {
        let body = counter_body(&format!("        count += 1;\n{GUARD}"), "");
        assert_eq!(
            shape(&body, "16 < count"),
            Some(("hex".to_string(), "count".to_string()))
        );
    }

    #[test]
    fn a_conditional_increment_is_refused() {
        let body = counter_body(
            &format!("        if b != 0 {{\n            count += 1;\n        }}\n{GUARD}"),
            "",
        );
        assert_eq!(shape(&body, "16 < count"), None);
    }

    #[test]
    fn a_counter_reset_in_the_loop_is_refused() {
        let body = counter_body(
            &format!(
                "        count += 1;\n        if b == 0 {{\n            count = 0;\n        }}\n{GUARD}"
            ),
            "",
        );
        assert_eq!(shape(&body, "16 < count"), None);
    }

    #[test]
    fn a_second_increment_is_refused() {
        let body = counter_body(
            &format!("        count += 1;\n        count += 1;\n{GUARD}"),
            "",
        );
        assert_eq!(shape(&body, "16 < count"), None);
    }

    #[test]
    fn a_break_or_continue_in_the_loop_is_refused() {
        let skip = counter_body(
            &format!(
                "        if b == 0 {{\n            continue;\n        }}\n        count += 1;\n{GUARD}"
            ),
            "",
        );
        assert_eq!(shape(&skip, "16 < count"), None);
        let stop = counter_body(
            &format!(
                "        count += 1;\n        if b == 0 {{\n            break;\n        }}\n{GUARD}"
            ),
            "",
        );
        assert_eq!(shape(&stop, "16 < count"), None);
    }

    #[test]
    fn a_predicate_after_the_loop_is_refused() {
        let body = counter_body(
            "        count += 1;\n",
            "    if 16 < count {\n        return Err(ChunkError::TooLong);\n    }\n",
        );
        assert_eq!(shape(&body, "16 < count"), None);
    }

    #[test]
    fn a_predicate_before_the_increment_is_refused() {
        let body = counter_body(&format!("{GUARD}        count += 1;\n"), "");
        assert_eq!(shape(&body, "16 < count"), None);
    }

    #[test]
    fn an_early_exit_before_the_predicate_is_refused() {
        let body = counter_body(
            &format!(
                "        count += 1;\n        let digit = (b as char).to_digit(16).ok_or(ChunkError::BadDigit)?;\n{GUARD}"
            ),
            "",
        );
        assert_eq!(shape(&body, "16 < count"), None);
    }

    #[test]
    fn a_shadowed_counter_is_refused() {
        let body = counter_body(
            &format!("        count += 1;\n        let count = 0;\n{GUARD}"),
            "",
        );
        assert_eq!(shape(&body, "16 < count"), None);
    }

    #[test]
    fn the_predicate_is_judged_over_every_counter_value_not_the_last() {
        // Equality is true only while the counter passes through 16: the
        // final value of a 17-element slice is 17.
        assert_eq!(predicate_ever_true("count == 16", "count", 17), Some(true));
        assert_eq!(predicate_ever_true("count == 16", "count", 15), Some(false));
        assert_eq!(predicate_ever_true("count != 16", "count", 1), Some(true));
        // A counter-on-left strict bound ends false at the final value only
        // when the loop never reached it.
        assert_eq!(predicate_ever_true("count < 3", "count", 10), Some(true));
        assert_eq!(predicate_ever_true("16 < count", "count", 16), Some(false));
        assert_eq!(predicate_ever_true("16 < count", "count", 17), Some(true));
        assert_eq!(predicate_ever_true("count <= 1", "count", 4), Some(true));
        assert_eq!(predicate_ever_true("count == other", "count", 4), None);
    }

    #[test]
    fn only_adjacent_lengths_flipping_the_predicate_pin_the_boundary() {
        let outcome = |lengths: &[usize]| {
            lengths
                .iter()
                .map(|length| {
                    (
                        *length,
                        predicate_ever_true("64 < count", "count", *length) == Some(true),
                    )
                })
                .collect::<Vec<_>>()
        };
        // 4 and 100 straddle the predicate but neither kills `64 <= count`.
        assert!(!adjacent_flip(&outcome(&[4, 100])));
        assert!(!adjacent_flip(&outcome(&[63, 66])));
        assert!(!adjacent_flip(&outcome(&[4, 63])));
        assert!(adjacent_flip(&outcome(&[64, 65])));
        assert!(adjacent_flip(&outcome(&[4, 64, 65, 100])));
    }

    #[test]
    fn a_later_zero_initialised_local_does_not_hide_the_counter() {
        let body = "pub fn chunk_size(hex: &[u8]) -> Result<u64, ChunkError> {\n    let mut count = 0;\n    let mut size = 0;\n    let mut pending;\n    for &b in hex {\n        count += 1;\n        if 16 < count {\n            return Err(ChunkError::TooLong);\n        }\n    }\n    Ok(0)\n}";
        assert_eq!(
            shape(body, "16 < count"),
            Some(("hex".to_string(), "count".to_string()))
        );
    }

    #[test]
    fn an_uninitialised_or_nonzero_counter_binding_is_refused() {
        let loop_part =
            format!("    for &b in hex {{\n        count += 1;\n{GUARD}    }}\n    Ok(0)\n}}");
        let head = "pub fn chunk_size(hex: &[u8]) -> Result<u64, ChunkError> {\n";
        assert_eq!(
            shape(
                &format!("{head}    let mut count;\n    count = 0;\n{loop_part}"),
                "16 < count"
            ),
            None
        );
        assert_eq!(
            shape(
                &format!("{head}    let mut count = 1;\n{loop_part}"),
                "16 < count"
            ),
            None
        );
        assert_eq!(
            shape(
                &format!("{head}    let count = 0;\n{loop_part}"),
                "16 < count"
            ),
            None
        );
    }
}
