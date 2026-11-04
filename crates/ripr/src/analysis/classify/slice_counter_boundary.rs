//! Per-byte loop counters on immutable slice inputs (#5346, grid-bits-length-pinned).
//! When tests call the owner with literal slice or byte-string arguments whose
//! lengths become the counter after one increment per element, bracket the
//! changed boundary without treating repeat lengths as direct scalar inputs.

use super::activation::{ActivationWithBoundaryInput, sort_value_facts};
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
    predicate: CounterPredicate,
    rows: Vec<SliceCounterRow>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Comparison {
    Lt,
    Le,
    Gt,
    Ge,
    Eq,
    Ne,
}

/// `counter OP literal` or `literal OP counter` over a decimal integer literal.
#[derive(Clone, Debug, PartialEq, Eq)]
struct CounterPredicate {
    counter: String,
    counter_on_left: bool,
    comparison: Comparison,
    literal: usize,
}

impl CounterPredicate {
    fn holds_at(&self, value: usize) -> bool {
        let (left, right) = if self.counter_on_left {
            (value, self.literal)
        } else {
            (self.literal, value)
        };
        match self.comparison {
            Comparison::Lt => left < right,
            Comparison::Le => left <= right,
            Comparison::Gt => left > right,
            Comparison::Ge => left >= right,
            Comparison::Eq => left == right,
            Comparison::Ne => left != right,
        }
    }

    /// The predicate is evaluated once per element with the counter at
    /// `1..=length`, so a call "fires" when it holds at any of those values.
    fn fires_for_length(&self, length: usize) -> bool {
        (1..=length).any(|value| self.holds_at(value))
    }
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
        // Both sides of the boundary are pinned only when two calls differ by
        // exactly one element and the predicate fires for the longer one but
        // not the shorter one; any other lengths leave an adjacent boundary
        // edit (`>` vs `>=`) undiscriminated.
        let lengths: Vec<usize> = self.rows.iter().map(|row| row.count).collect();
        let pinned = lengths.iter().any(|length| {
            lengths.contains(&(length + 1))
                && !self.predicate.fires_for_length(*length)
                && self.predicate.fires_for_length(length + 1)
        });
        pinned.then(|| {
            StageEvidence::new(
                StageState::Yes,
                Confidence::Medium,
                "Literal slice inputs of adjacent lengths establish both sides of the per-byte counter boundary",
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
                "{} | slice input byte length {} drives counter `{}` through 1..={}",
                row.call_text, row.count, boundary.counter, row.count
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
    let (slice_param, predicate) = per_byte_counter_shape(owner, &context.probe.expression)?;
    let counter = predicate.counter.clone();
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
            let length = slice_argument_byte_length(argument)?;
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
        let facts = context.index.files().get(path)?;
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
        predicate,
        rows,
    })
}

/// The owner is one `for _ in slice` loop whose first statement is an
/// unconditional `counter += 1` and whose second is `if <predicate>`, with
/// nothing before the loop that can exit early, so the predicate runs once per
/// element with the counter at `1..=length`.
fn per_byte_counter_shape(
    owner: &FunctionSummary,
    expression: &str,
) -> Option<(String, CounterPredicate)> {
    let predicate = parse_counter_predicate(expression)?;
    let counter = predicate.counter.as_str();
    let parse = parse_clean_source_file(&owner.body)?;
    let function = parse.tree().syntax().children().find_map(ast::Fn::cast)?;
    if function.async_token().is_some() || function.attrs().next().is_some() {
        return None;
    }
    let body = function.body()?;
    let loops = body
        .syntax()
        .descendants()
        .filter(|node| {
            ast::ForExpr::can_cast(node.kind())
                || ast::WhileExpr::can_cast(node.kind())
                || ast::LoopExpr::can_cast(node.kind())
        })
        .count();
    if loops != 1
        || body.syntax().descendants().any(|node| {
            ast::ClosureExpr::can_cast(node.kind())
                || ast::Fn::can_cast(node.kind())
                || ast::Item::can_cast(node.kind())
        })
    {
        return None;
    }
    // Exactly one binding of the counter name (no shadowing, no pattern reuse).
    let bindings = body
        .syntax()
        .descendants()
        .filter_map(ast::IdentPat::cast)
        .filter(|pattern| pattern.name().is_some_and(|name| name.text() == counter))
        .count();
    if bindings != 1 {
        return None;
    }
    // Exactly one write to the counter, and no `&mut counter` escape.
    let writes = body
        .syntax()
        .descendants()
        .filter_map(ast::BinExpr::cast)
        .filter(|binary| matches!(binary.op_kind(), Some(ast::BinaryOp::Assignment { .. })))
        .filter(|binary| {
            binary
                .lhs()
                .is_some_and(|lhs| compact(&lhs.syntax().text().to_string()) == counter)
        })
        .count();
    let escapes = body
        .syntax()
        .descendants()
        .filter_map(ast::RefExpr::cast)
        .filter(|reference| reference.mut_token().is_some() || reference.raw_token().is_some())
        .any(|reference| {
            reference
                .expr()
                .is_some_and(|inner| compact(&inner.syntax().text().to_string()) == counter)
        });
    if writes != 1 || escapes {
        return None;
    }
    let statements = body.stmt_list()?.statements().collect::<Vec<_>>();
    let for_index = statements.iter().position(|statement| {
        matches!(
            statement,
            ast::Stmt::ExprStmt(expr) if matches!(expr.expr(), Some(ast::Expr::ForExpr(_)))
        )
    })?;
    let mut counter_initialized = false;
    for statement in &statements[..for_index] {
        let ast::Stmt::LetStmt(binding) = statement else {
            return None;
        };
        if binding.attrs().next().is_some() || binding.let_else().is_some() {
            return None;
        }
        let initializer = binding.initializer()?;
        if initializer.syntax().descendants().any(|node| {
            ast::TryExpr::can_cast(node.kind())
                || ast::ReturnExpr::can_cast(node.kind())
                || ast::BreakExpr::can_cast(node.kind())
                || ast::ContinueExpr::can_cast(node.kind())
                || ast::MacroCall::can_cast(node.kind())
        }) {
            return None;
        }
        if let ast::Pat::IdentPat(local) = binding.pat()?
            && local.name()?.text() == counter
        {
            if local.mut_token().is_none()
                || local.ref_token().is_some()
                || compact(&initializer.syntax().text().to_string()) != "0"
            {
                return None;
            }
            counter_initialized = true;
        }
    }
    if !counter_initialized {
        return None;
    }
    let ast::Stmt::ExprStmt(loop_statement) = &statements[for_index] else {
        return None;
    };
    let ast::Expr::ForExpr(for_loop) = loop_statement.expr()? else {
        return None;
    };
    let pattern = compact(&for_loop.pat()?.syntax().text().to_string());
    if pattern != "_" && pattern != "b" && pattern != "&b" {
        return None;
    }
    let ast::Expr::PathExpr(path) = for_loop.iterable()? else {
        return None;
    };
    let slice_param = path.path()?.syntax().text().to_string();
    let loop_statements = for_loop
        .loop_body()?
        .stmt_list()?
        .statements()
        .collect::<Vec<_>>();
    let [increment, check, ..] = loop_statements.as_slice() else {
        return None;
    };
    let ast::Stmt::ExprStmt(increment) = increment else {
        return None;
    };
    let ast::Expr::BinExpr(increment) = increment.expr()? else {
        return None;
    };
    if increment.op_kind()
        != Some(ast::BinaryOp::Assignment {
            op: Some(ast::ArithOp::Add),
        })
        || compact(&increment.lhs()?.syntax().text().to_string()) != counter
        || compact(&increment.rhs()?.syntax().text().to_string()) != "1"
    {
        return None;
    }
    let ast::Stmt::ExprStmt(check) = check else {
        return None;
    };
    let ast::Expr::IfExpr(check) = check.expr()? else {
        return None;
    };
    if compact(&check.condition()?.syntax().text().to_string()) != compact(expression) {
        return None;
    }
    Some((slice_param, predicate))
}

fn parse_counter_predicate(expression: &str) -> Option<CounterPredicate> {
    let text = compact(expression);
    let mut found: Option<(usize, usize, Comparison)> = None;
    let mut index = 0;
    while index < text.len() {
        let (comparison, width) = match text.get(index..index + 2) {
            Some("<=") => (Comparison::Le, 2),
            Some(">=") => (Comparison::Ge, 2),
            Some("==") => (Comparison::Eq, 2),
            Some("!=") => (Comparison::Ne, 2),
            _ => match text.as_bytes()[index] {
                b'<' => (Comparison::Lt, 1),
                b'>' => (Comparison::Gt, 1),
                _ => {
                    index += 1;
                    continue;
                }
            },
        };
        if found.is_some() {
            return None;
        }
        found = Some((index, width, comparison));
        index += width;
    }
    let (position, width, comparison) = found?;
    let left = &text[..position];
    let right = &text[position + width..];
    let literal = |operand: &str| {
        (!operand.is_empty() && operand.chars().all(|ch| ch.is_ascii_digit()))
            .then(|| operand.parse::<usize>().ok())
            .flatten()
    };
    let (counter, counter_on_left, literal) = match (
        is_simple_identifier(left),
        literal(left),
        is_simple_identifier(right),
        literal(right),
    ) {
        (true, _, _, Some(literal)) => (left, true, literal),
        (_, Some(literal), true, _) => (right, false, literal),
        _ => return None,
    };
    Some(CounterPredicate {
        counter: counter.to_string(),
        counter_on_left,
        comparison,
        literal,
    })
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

    const GRID_BITS: &str = "pub fn chunk_size(hex: &[u8]) -> Result<u64, ChunkError> {\n    let mut size: u64 = 0;\n    let mut count = 0;\n    for &b in hex {\n        count += 1;\n        if 16 < count {\n            return Err(ChunkError::TooLong);\n        }\n        let digit = (b as char).to_digit(16).ok_or(ChunkError::BadDigit)?;\n        size = (size << 4) | u64::from(digit);\n    }\n    Ok(size)\n}";

    fn boundary_with_lengths(expression: &str, lengths: &[usize]) -> Option<SliceCounterBoundary> {
        Some(SliceCounterBoundary {
            owner: SymbolId("src/lib.rs::chunk_size".to_string()),
            expression: expression.to_string(),
            predicate_line: 15,
            counter: "count".to_string(),
            predicate: parse_counter_predicate(expression)?,
            rows: lengths
                .iter()
                .map(|count| SliceCounterRow {
                    count: *count,
                    test_file: PathBuf::from("tests/t.rs"),
                    call_line: 1,
                    call_text: String::new(),
                })
                .collect(),
        })
    }

    fn infers_yes(expression: &str, lengths: &[usize]) -> bool {
        let owner = owner_body(GRID_BITS);
        let Some(boundary) = boundary_with_lengths(expression, lengths) else {
            return false;
        };
        let probe = Probe {
            id: crate::domain::ProbeId("p".to_string()),
            location: crate::domain::SourceLocation::new("src/lib.rs", 15, 1),
            owner: Some(owner.id.clone()),
            family: ProbeFamily::Predicate,
            delta: crate::domain::DeltaKind::Value,
            before: None,
            after: None,
            expression: expression.to_string(),
            expected_sinks: Vec::new(),
            required_oracles: Vec::new(),
        };
        boundary.infection(&probe, &owner).is_some()
    }

    #[test]
    fn grid_bits_owner_shape_is_recognized() -> Result<(), ()> {
        let owner = owner_body(GRID_BITS);
        let (slice, found) = per_byte_counter_shape(&owner, "16 < count").ok_or(())?;
        assert_eq!(slice, "hex");
        assert_eq!(found.counter, "count");
        assert_eq!(immutable_slice_parameter(&owner, "hex"), Some(0));
        Ok(())
    }

    #[test]
    fn conditionally_skipped_or_shadowed_counters_are_rejected() {
        for body in [
            // increment only on some elements
            GRID_BITS.replace("        count += 1;\n", "        if b != 0 { count += 1; }\n"),
            // increment is not the first loop statement
            GRID_BITS.replace("        count += 1;\n        if 16 < count {", "        let _skip = b;\n        count += 1;\n        if 16 < count {"),
            // continue before the increment
            GRID_BITS.replace("        count += 1;\n", "        if b == 0 { continue; }\n        count += 1;\n"),
            // second write to the counter
            GRID_BITS.replace("        let digit", "        count += 1;\n        let digit"),
            // early exit before the loop
            GRID_BITS.replace("    let mut count = 0;\n", "    let mut count = 0;\n    let _first = hex.first().ok_or(ChunkError::BadDigit)?;\n"),
            // shadowed counter
            GRID_BITS.replace("        let digit", "        let count = 0;\n        let digit"),
            // counter starts at one
            GRID_BITS.replace("let mut count = 0;", "let mut count = 1;"),
            // increment by two
            GRID_BITS.replace("count += 1;", "count += 2;"),
        ] {
            assert!(
                per_byte_counter_shape(&owner_body(&body), "16 < count").is_none(),
                "{body}"
            );
        }
    }

    #[test]
    fn predicate_fires_over_every_counter_value_up_to_the_length() -> Result<(), ()> {
        // Counter on the right: 16 < count fires first at 17.
        let p = parse_counter_predicate("16 < count").ok_or(())?;
        assert!(!p.fires_for_length(16));
        assert!(p.fires_for_length(17));
        // Counter on the left, non-strict: count >= 16 fires first at 16.
        let p = parse_counter_predicate("count >= 16").ok_or(())?;
        assert!(!p.fires_for_length(15));
        assert!(p.fires_for_length(16));
        // Equality fires from the length that reaches it and stays fired.
        let p = parse_counter_predicate("count == 16").ok_or(())?;
        assert!(!p.fires_for_length(15));
        assert!(p.fires_for_length(16));
        assert!(p.fires_for_length(17));
        // Inequality fires on the first element.
        let p = parse_counter_predicate("count != 16").ok_or(())?;
        assert!(p.fires_for_length(1));
        // Reversed non-strict: 16 <= count.
        let p = parse_counter_predicate("16 <= count").ok_or(())?;
        assert!(!p.fires_for_length(15));
        assert!(p.fires_for_length(16));
        Ok(())
    }

    #[test]
    fn predicate_parsing_rejects_shapes_it_cannot_evaluate() {
        for expression in [
            "count",
            "count < limit",
            "16 < 17",
            "count << 2",
            "a < count && count < 16",
            "count < 0x10",
            "count < 1_6",
            "count.len() < 16",
            "count < -1",
        ] {
            assert!(
                parse_counter_predicate(expression).is_none(),
                "{expression}"
            );
        }
        assert!(parse_counter_predicate("count>16").is_some());
    }

    #[test]
    fn infection_needs_adjacent_lengths_that_straddle_the_boundary() {
        // 16 and 17 pin `16 < count` on both sides.
        assert!(infers_yes("16 < count", &[16, 17]));
        assert!(infers_yes("16 < count", &[1, 16, 17]));
        // Off-boundary lengths do not discriminate a `>` to `>=` edit.
        assert!(!infers_yes("16 < count", &[15, 17]));
        assert!(!infers_yes("16 < count", &[10, 20]));
        // One side only.
        assert!(!infers_yes("16 < count", &[16]));
        assert!(!infers_yes("16 < count", &[17, 18]));
        // Equality and counter-on-the-left forms use the same prefix rule.
        assert!(infers_yes("count == 16", &[15, 16]));
        assert!(!infers_yes("count == 16", &[16, 17]));
        assert!(infers_yes("count > 15", &[15, 16]));
        assert!(!infers_yes("count != 16", &[1, 2]));
    }
}
