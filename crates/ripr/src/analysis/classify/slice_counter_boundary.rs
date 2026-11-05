//! Per-byte loop counters on immutable slice inputs (#5346, grid-bits-length-pinned).
//! When tests call the owner with literal slice or byte-string arguments whose
//! lengths become the counter after one increment per element, bracket the
//! changed boundary without treating repeat lengths as direct scalar inputs.

use super::activation::{ActivationWithBoundaryInput, sort_value_facts};
use super::context::ProbeContext;
use super::owner_pin::{OwnerPinSyntax, targeted_free_call_reaches_owner};
use super::reach::relation_establishes_reach;
use super::slice_boundary::{slice_argument_byte_length, slice_argument_repeat_element};
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
    ast::{self, HasArgList, HasAttrs, HasGenericArgs, HasGenericParams, HasLoopBody, HasName},
};
use std::path::PathBuf;

const MAX_ROWS: usize = 64;

pub(in crate::analysis) struct SliceCounterBoundary {
    owner: SymbolId,
    expression: String,
    predicate_line: usize,
    counter: String,
    predicate: CounterPredicate,
    /// Compact text of the value the owner returns when the predicate fires.
    fire_value: String,
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

struct SliceCounterShape {
    slice_param: String,
    predicate: CounterPredicate,
    fire_value: String,
    requires_hex_input: bool,
    counter_max: usize,
}

struct SliceCounterRow {
    count: usize,
    /// Compact repeated element when the argument is `&[element; count]`.
    element: Option<String>,
    test_file: PathBuf,
    call_line: usize,
    call_text: String,
}

impl SliceCounterBoundary {
    /// Calls whose lengths differ by one and straddle the boundary: the
    /// shorter does not fire the predicate, the longer does, and both repeat
    /// the same element so they differ only in length. With different
    /// content the outcomes can differ for reasons other than the counter (a
    /// bad digit, say), which a different threshold also passes.
    fn straddling_pairs(&self) -> impl Iterator<Item = (&SliceCounterRow, &SliceCounterRow)> {
        self.rows.iter().filter_map(|short| {
            let long = self.rows.iter().find(|row| row.count == short.count + 1)?;
            (!self.predicate.fires_for_length(short.count)
                && self.predicate.fires_for_length(long.count)
                && short.element.is_some()
                && short.element == long.element)
                .then_some((short, long))
        })
    }

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
        let pinned = self.straddling_pairs().next().is_some();
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
        if !self.valid_for(probe, owner) {
            return false;
        }
        // The boundary is discriminated only when the tests assert the outcome
        // of both calls straddling it: the longer call (the predicate fires)
        // must assert the value the owner returns on firing, and the shorter
        // call must assert something else. One asserted side, or the same
        // outcome on both (for example an earlier `?` exit that never reaches
        // the counter), leaves the boundary edit unnoticed.
        let asserted = |row: &SliceCounterRow| -> Vec<String> {
            let mut outcomes = Vec::new();
            for test in tests {
                if test.file != row.test_file || index.files().get(&test.file).is_none() {
                    continue;
                }
                for assertion in &test.assertions {
                    if assertion_admitted(test, assertion)
                        && (owner_pinned(test, assertion)
                            || assertion.text.contains(&format!("{}(", owner.name)))
                        && let Some(expected) =
                            asserted_outcome(&assertion.text, &row.call_text, &owner.name)
                    {
                        outcomes.push(expected);
                    }
                }
            }
            outcomes
        };
        self.straddling_pairs().any(|(short, long)| {
            outcomes_straddle(&asserted(short), &asserted(long), &self.fire_value)
        })
    }
}

/// Every asserted outcome of the shorter call differs from the firing value,
/// every asserted outcome of the longer call is the firing value, and both
/// calls assert something.
fn outcomes_straddle(short: &[String], long: &[String], fire_value: &str) -> bool {
    !short.is_empty()
        && short.iter().all(|outcome| outcome != fire_value)
        && !long.is_empty()
        && long.iter().all(|outcome| outcome == fire_value)
}

/// The compact expected value of `assert_eq!(<call>, <expected>)` (either
/// argument order) for the row's call. `call_text` is either the bare owner
/// call or the whole assertion statement that holds it (the call fact for a
/// call inside an assertion macro is the statement); the owner call is then
/// the argument that starts `owner(`.
fn asserted_outcome(assertion: &str, call_text: &str, owner: &str) -> Option<String> {
    let arguments = super::activation::call_arguments(assertion, "assert_eq!")?;
    let [first, second] = arguments.as_slice() else {
        return None;
    };
    let (first, second) = (compact(first), compact(second));
    let row = compact(call_text);
    let statement = compact(assertion);
    let same_statement = row.trim_end_matches(';') == statement.trim_end_matches(';');
    let prefix = format!("{owner}(");
    let is_call = |argument: &str| {
        if same_statement {
            argument.starts_with(&prefix)
        } else {
            argument == row
        }
    };
    match (is_call(&first), is_call(&second)) {
        (true, false) => Some(second),
        (false, true) => Some(first),
        _ => None,
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
    // Only a pair of adjacent lengths that straddles the boundary replaces the
    // unresolved-boundary abstention; any other rows leave the gathered state
    // as it was.
    if boundary.straddling_pairs().next().is_none() {
        return;
    }
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
    let SliceCounterShape {
        slice_param,
        predicate,
        fire_value,
        requires_hex_input,
        counter_max,
    } = per_byte_counter_shape(
        owner,
        &context.probe.expression,
        &context.index.files().get(&owner.file)?.source,
    )?;
    let counter = predicate.counter.clone();
    let slice_position = immutable_slice_parameter(owner, &slice_param)?;
    let mut rows = Vec::new();
    let mut admitted_calls = 0usize;
    let mut reference_files = std::collections::HashSet::new();
    reference_files.insert(owner.file.clone());
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
            if length == 0 || length > counter_max {
                return None;
            }
            admitted_calls += 1;
            if admitted_calls > MAX_ROWS {
                return None;
            }
            reference_files.insert(test.file.clone());
            let element = slice_argument_repeat_element(argument);
            if requires_hex_input
                && !element
                    .as_deref()
                    .is_some_and(repeated_element_is_hex_digit)
            {
                continue;
            }
            rows.push(SliceCounterRow {
                count: length,
                element,
                test_file: test.file.clone(),
                call_line: call.line,
                call_text: call.text.to_string(),
            });
            if rows.len() > MAX_ROWS {
                return None;
            }
        }
    }
    if rows.is_empty() {
        return None;
    }
    let references = reference_files.iter().try_fold(0usize, |count, path| {
        let facts = context.index.files().get(path)?;
        count.checked_add(owner_reference_count(&facts.source, &owner.name)?)
    });
    // Fail closed on a count that was established and disagrees with the
    // admitted calls. `owner_reference_count` declines any file that defines an
    // enum, struct or impl (every owner returning its own error type), so an
    // unestablished count cannot object without refusing the whole family.
    if let (Some(references), Some(expected)) = (references, admitted_calls.checked_add(1))
        && references != expected
    {
        return None;
    }
    rows.sort_by_key(|row| row.count);
    // One row per length: a duplicate length can drop a pairing, never create one.
    rows.dedup_by_key(|row| row.count);
    Some(SliceCounterBoundary {
        owner: owner.id.clone(),
        expression: context.probe.expression.clone(),
        predicate_line: context.probe.location.line,
        counter,
        predicate,
        fire_value,
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
    retained_source: &str,
) -> Option<SliceCounterShape> {
    let predicate = parse_counter_predicate(expression)?;
    let counter = predicate.counter.as_str();
    let parse = parse_clean_source_file(&owner.body)?;
    let function = parse.tree().syntax().children().find_map(ast::Fn::cast)?;
    if function.async_token().is_some()
        || function.attrs().next().is_some()
        || function.generic_param_list().is_some()
    {
        return None;
    }
    if !result_constructors_unshadowed(&function, retained_source) {
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
                .is_some_and(|lhs| expression_mentions_local(&lhs, counter))
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
                .is_some_and(|inner| expression_mentions_local(&inner, counter))
        });
    let implicit_mutation = body.syntax().descendants().any(|node| {
        ast::MethodCallExpr::cast(node.clone()).is_some_and(|call| call.receiver()
            .is_some_and(|receiver| expression_mentions_local(&receiver, counter)))
            // Ref-mut patterns borrow in let/match/if-let contexts. The
            // bounded loop needs none, so unsupported aliases all refuse.
            || ast::IdentPat::cast(node).is_some_and(|binding|
                binding.ref_token().is_some() && binding.mut_token().is_some())
    });
    if writes != 1 || escapes || implicit_mutation {
        return None;
    }
    let statements = body.stmt_list()?.statements().collect::<Vec<_>>();
    let for_index = statements.iter().position(|statement| {
        matches!(
            statement,
            ast::Stmt::ExprStmt(expr) if matches!(expr.expr(), Some(ast::Expr::ForExpr(_)))
        )
    })?;
    let mut counter_max = None;
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
            counter_max = Some(counter_nonnegative_max(binding.ty().as_ref())?);
            if predicate.literal > counter_max? {
                return None;
            }
        }
    }
    let counter_max = counter_max?;
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
    // The iterated name must be the parameter itself: one binding, so a
    // `let hex = &hex[..4];` rebinding cannot shorten the loop.
    let slice_bindings = body
        .syntax()
        .ancestors()
        .find_map(ast::Fn::cast)
        .map_or(0, |function| {
            function
                .syntax()
                .descendants()
                .filter_map(ast::IdentPat::cast)
                .filter(|pattern| {
                    pattern
                        .name()
                        .is_some_and(|name| name.text() == slice_param)
                })
                .count()
        });
    if slice_bindings != 1 {
        return None;
    }
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
    if compact(&check.condition()?.syntax().text().to_string()) != compact(expression)
        || check.else_branch().is_some()
    {
        return None;
    }
    // The firing branch must be exactly `return <value>`, so the value a test
    // asserts on a firing call can be compared with it.
    let then_list = check.then_branch()?.stmt_list()?;
    let mut then_statements = then_list.statements();
    let fire = match (
        then_statements.next(),
        then_statements.next(),
        then_list.tail_expr(),
    ) {
        (Some(ast::Stmt::ExprStmt(statement)), None, None) => statement.expr()?,
        (None, None, Some(tail)) => tail,
        _ => return None,
    };
    let ast::Expr::ReturnExpr(fire) = fire else {
        return None;
    };
    let fire_value = compact(&fire.expr()?.syntax().text().to_string());
    // Bound the entire normal output path: nothing after the loop can
    // manufacture the firing error, and the tail is a simple success value.
    if statements.len() != for_index + 1 {
        return None;
    }
    let ast::Expr::CallExpr(success) = body.stmt_list()?.tail_expr()? else {
        return None;
    };
    if compact(&success.expr()?.syntax().text().to_string()) != "Ok" {
        return None;
    }
    let success_args = success.arg_list()?.args().collect::<Vec<_>>();
    let [success_arg] = success_args.as_slice() else {
        return None;
    };
    if !matches!(success_arg, ast::Expr::PathExpr(_) | ast::Expr::Literal(_)) {
        return None;
    }
    let ast::Expr::CallExpr(error) = fire.expr()? else {
        return None;
    };
    if compact(&error.expr()?.syntax().text().to_string()) != "Err" {
        return None;
    }
    let error_args = error.arg_list()?.args().collect::<Vec<_>>();
    let [error_arg] = error_args.as_slice() else {
        return None;
    };
    if !matches!(error_arg, ast::Expr::PathExpr(_) | ast::Expr::Literal(_)) {
        return None;
    }
    let firing_error = compact(&error_arg.syntax().text().to_string());
    let loop_body = for_loop.loop_body()?;
    if loop_body.stmt_list()?.tail_expr().is_some() {
        return None;
    }
    // The suffix is straight-line calculation. Its sole admitted early exit
    // is `option.ok_or(OtherError)?`, whose explicit error differs from the
    // firing value; opaque fallible calls and conditional fallbacks decline.
    let mut requires_hex_input = false;
    for statement in &loop_statements[2..] {
        if statement.syntax().descendants().any(|node| {
            ast::IfExpr::can_cast(node.kind())
                || ast::MatchExpr::can_cast(node.kind())
                || ast::BlockExpr::can_cast(node.kind())
                || ast::MacroCall::can_cast(node.kind())
        }) {
            return None;
        }
        for try_expr in statement
            .syntax()
            .descendants()
            .filter_map(ast::TryExpr::cast)
        {
            requires_hex_input = true;
            let ast::Expr::MethodCallExpr(method) = try_expr.expr()? else {
                return None;
            };
            if method.name_ref()?.text() != "ok_or" {
                return None;
            }
            let args = method.arg_list()?.args().collect::<Vec<_>>();
            let [other_error] = args.as_slice() else {
                return None;
            };
            if compact(&method.receiver()?.syntax().text().to_string()) != "(baschar).to_digit(16)"
                || !distinct_identity_error_escape(
                    &function,
                    retained_source,
                    &firing_error,
                    other_error,
                )
            {
                return None;
            }
        }
    }

    // A later return can supply the firing value after a boundary edit has
    // stopped this branch from firing. Distinct asserted outcomes then do not
    // establish that this predicate controls the result.
    if body.syntax().descendants().any(|node| {
        ast::ReturnExpr::cast(node.clone())
            .is_some_and(|returned| returned.syntax() != fire.syntax())
            || ast::BreakExpr::can_cast(node.kind())
            || ast::ContinueExpr::can_cast(node.kind())
    }) {
        return None;
    }
    if requires_hex_input && !fallible_hex_loop_uses_the_input(&function, &pattern, &slice_param) {
        return None;
    }
    Some(SliceCounterShape {
        slice_param,
        predicate,
        fire_value,
        requires_hex_input,
        counter_max,
    })
}

/// Unannotated integer zero can infer a narrow integer through later uses.
/// Only the common nonnegative i8 range is proved without a type annotation.
/// Explicit primitive names are admitted only after lexical shadow checks.
fn counter_nonnegative_max(ty: Option<&ast::Type>) -> Option<usize> {
    let Some(ty) = ty else {
        return Some(127);
    };
    let text = compact(&ty.syntax().text().to_string());
    match text.as_str() {
        "i8" => Some(127),
        "u8" => Some(255),
        "i16" | "isize" => Some(32767),
        "u16" | "usize" => Some(65535),
        "i32" => Some(i32::MAX as usize),
        "u32" => Some(usize::try_from(u32::MAX).unwrap_or(usize::MAX)),
        "i64" => Some(usize::try_from(i64::MAX).unwrap_or(usize::MAX)),
        "u64" | "i128" | "u128" => Some(usize::MAX),
        _ => None,
    }
}

fn counter_primitive_name(name: &str) -> bool {
    matches!(
        name.trim_start_matches("r#"),
        "i8" | "u8"
            | "i16"
            | "u16"
            | "i32"
            | "u32"
            | "i64"
            | "u64"
            | "i128"
            | "u128"
            | "isize"
            | "usize"
    )
}

/// Conservative place/receiver read for the local counter. Groups and
/// destructuring keep their path nodes; fields named `count` do not become a
/// read of the local. Unknown receiver methods can implicitly borrow it mutably.
fn expression_mentions_local(expression: &ast::Expr, local: &str) -> bool {
    std::iter::once(expression.syntax().clone())
        .chain(expression.syntax().descendants())
        .filter_map(ast::PathExpr::cast)
        .any(|path| {
            path.path().is_some_and(|path| {
                compact(&path.syntax().text().to_string()).trim_start_matches("r#") == local
            })
        })
}

/// The only admitted `?` must receive the unchanged byte from this exact loop.
/// Together with a valid repeated hex input this proves it cannot exit early;
/// spelling two errors differently does not prove their runtime inequality.
fn fallible_hex_loop_uses_the_input(function: &ast::Fn, pattern: &str, input: &str) -> bool {
    if pattern != "&b" || !function.param_list().is_some_and(|params| params.params().any(|param| {
        matches!(param.pat(), Some(ast::Pat::IdentPat(ref name)) if name.name().is_some_and(|name| name.text() == input))
            && param.ty().is_some_and(|ty| compact(&ty.syntax().text().to_string()) == "&[u8]")
    })) { return false; }
    let bindings = function
        .syntax()
        .descendants()
        .filter_map(ast::IdentPat::cast)
        .filter(|binding| {
            binding
                .name()
                .is_some_and(|name| name.text().trim_start_matches("r#") == "b")
        })
        .count();
    bindings == 1
        && !function.syntax().descendants().any(|node| {
            ast::BinExpr::cast(node.clone()).is_some_and(|binary| {
                matches!(binary.op_kind(), Some(ast::BinaryOp::Assignment { .. }))
                    && binary.lhs().is_some_and(|lhs| {
                        compact(&lhs.syntax().text().to_string()).trim_start_matches("r#") == "b"
                    })
            }) || ast::RefExpr::cast(node).is_some_and(|reference| {
                (reference.mut_token().is_some() || reference.raw_token().is_some())
                    && reference.expr().is_some_and(|expr| {
                        compact(&expr.syntax().text().to_string()).trim_start_matches("r#") == "b"
                    })
            })
        })
}

fn repeated_element_is_hex_digit(element: &str) -> bool {
    let Some(parsed) = parse_clean_source_file(&format!("fn input() {{ let value = {element}; }}"))
    else {
        return false;
    };
    let Some(ast::Expr::Literal(literal)) = parsed
        .tree()
        .syntax()
        .descendants()
        .find_map(ast::LetStmt::cast)
        .and_then(|binding| binding.initializer())
    else {
        return false;
    };
    match literal.kind() {
        ast::LiteralKind::Byte(byte) => byte.value().is_ok_and(|byte| byte.is_ascii_hexdigit()),
        _ => false,
    }
}

/// Bare Result constructors have authority only without a competing binding in
/// the owner's lexical scopes. Unread glob/macro bindings remain conservative.
fn result_constructors_unshadowed(function: &ast::Fn, retained_source: &str) -> bool {
    let Some(source) = parse_clean_source_file(retained_source) else {
        return false;
    };
    let Some(name) = function.name() else {
        return false;
    };
    let mut candidates = source
        .tree()
        .syntax()
        .descendants()
        .filter_map(ast::Fn::cast)
        .filter(|candidate| {
            candidate
                .name()
                .is_some_and(|candidate| candidate.text() == name.text())
        });
    let Some(owner) = candidates.next() else {
        return false;
    };
    if candidates.next().is_some() {
        return false;
    }
    let needs_primitives = function
        .syntax()
        .descendants()
        .any(|node| ast::TryExpr::can_cast(node.kind()));
    let constructor = |name: Option<ast::Name>| {
        name.is_some_and(|name| {
            matches!(
                name.text().trim_start_matches("r#"),
                "Ok" | "Err" | "Result"
            ) || counter_primitive_name(name.text())
                || (needs_primitives && name.text().trim_start_matches("r#") == "char")
        })
    };
    let scoped = source
        .tree()
        .syntax()
        .descendants()
        .filter(|node| {
            owner
                .syntax()
                .ancestors()
                .any(|scope| Some(scope) == node.parent())
                || node.ancestors().any(|ancestor| ancestor == *owner.syntax())
        })
        .collect::<Vec<_>>();
    // Attribute expansion can add bindings without a visible declaration. Do
    // not evaluate cfg here: library and integration-test builds differ.
    if scoped
        .iter()
        .chain(owner.syntax().ancestors().collect::<Vec<_>>().iter())
        .flat_map(|node| node.children().filter_map(ast::Attr::cast))
        .any(|attr| !constructor_scope_attribute_is_inert(&attr))
    {
        return false;
    }
    !scoped.into_iter().any(|node| {
        ast::MacroCall::can_cast(node.kind())
            || ast::ExternCrate::can_cast(node.kind())
            || ast::ExternBlock::can_cast(node.kind())
            || ast::Use::cast(node.clone()).is_some_and(|item| {
                item.syntax()
                    .descendants_with_tokens()
                    .filter_map(|entry| entry.into_token())
                    .any(|token| {
                        counter_primitive_name(token.text())
                            || (needs_primitives && token.text().trim_start_matches("r#") == "char")
                            || matches!(
                                token.text().trim_start_matches("r#"),
                                "Ok" | "Err"
                                    | "Result"
                                    | "*"
                                    | "Clone"
                                    | "Copy"
                                    | "Debug"
                                    | "Default"
                                    | "Eq"
                                    | "PartialEq"
                                    | "Ord"
                                    | "PartialOrd"
                                    | "Hash"
                            )
                    })
            })
            || ast::Fn::cast(node.clone()).is_some_and(|item| constructor(item.name()))
            || ast::Struct::cast(node.clone()).is_some_and(|item| constructor(item.name()))
            || ast::Enum::cast(node.clone()).is_some_and(|item| constructor(item.name()))
            || ast::TypeAlias::cast(node.clone()).is_some_and(|item| constructor(item.name()))
            || ast::Const::cast(node.clone()).is_some_and(|item| constructor(item.name()))
            || ast::Static::cast(node.clone()).is_some_and(|item| constructor(item.name()))
            || ast::IdentPat::cast(node).is_some_and(|item| constructor(item.name()))
    })
}

/// Only source-inert builtins are understood here. A custom derive (even
/// conditionally present) can emit a module-level `Ok` or `Err`. Builtin derive
/// names additionally require the import/macro namespace checks above.
fn constructor_scope_attribute_is_inert(attr: &ast::Attr) -> bool {
    let Some(name) = attr.simple_name() else {
        return false;
    };
    if name != "derive" {
        return matches!(
            name.as_str(),
            "cfg"
                | "test"
                | "should_panic"
                | "ignore"
                | "doc"
                | "repr"
                | "inline"
                | "cold"
                | "must_use"
                | "allow"
                | "warn"
                | "deny"
                | "forbid"
                | "expect"
                | "deprecated"
                | "non_exhaustive"
                | "track_caller"
                | "automatically_derived"
        );
    }
    let Some((_, tree)) = attr.as_simple_call() else {
        return false;
    };
    let mut expect_name = true;
    for token in tree
        .syntax()
        .descendants_with_tokens()
        .filter_map(|entry| entry.into_token())
    {
        if token.kind().is_trivia() || matches!(token.text(), "(" | ")") {
            continue;
        }
        if expect_name {
            if !matches!(
                token.text().trim_start_matches("r#"),
                "Clone"
                    | "Copy"
                    | "Debug"
                    | "Default"
                    | "Eq"
                    | "PartialEq"
                    | "Ord"
                    | "PartialOrd"
                    | "Hash"
            ) {
                return false;
            }
        } else if token.text() != "," {
            return false;
        }
        expect_name = !expect_name;
    }
    true
}

/// `?` is admitted only when it cannot convert an intermediate error into
/// the firing value: both errors have the directly declared Result error type.
fn distinct_identity_error_escape(
    function: &ast::Fn,
    retained_source: &str,
    firing_error: &str,
    other_error: &ast::Expr,
) -> bool {
    let Some(ast::Type::PathType(result)) = function.ret_type().and_then(|ret| ret.ty()) else {
        return false;
    };
    let Some(path) = result.path() else {
        return false;
    };
    if compact(&path.syntax().text().to_string()).split('<').next() != Some("Result") {
        return false;
    }
    let Some(args) = path
        .segment()
        .and_then(|segment| segment.generic_arg_list())
    else {
        return false;
    };
    let args = args.generic_args().collect::<Vec<_>>();
    let [_, ast::GenericArg::TypeArg(error)] = args.as_slice() else {
        return false;
    };
    let Some(error) = error.ty() else {
        return false;
    };
    let error_type = compact(&error.syntax().text().to_string());
    let other = compact(&other_error.syntax().text().to_string());
    if firing_error == other {
        return false;
    }
    let Some(source) = parse_clean_source_file(retained_source) else {
        return false;
    };
    let Some(name) = function.name() else {
        return false;
    };
    let mut functions = source
        .tree()
        .syntax()
        .descendants()
        .filter_map(ast::Fn::cast)
        .filter(|candidate| {
            candidate
                .name()
                .is_some_and(|candidate| candidate.text() == name.text())
        });
    let Some(owner) = functions.next() else {
        return false;
    };
    if functions.next().is_some() {
        return false;
    }
    // Reject local/imported Result aliases; the prelude Result supplies identity
    // conversion only after this spelling is known not to name a local type.
    if source
        .tree()
        .syntax()
        .descendants()
        .filter(|node| {
            owner
                .syntax()
                .ancestors()
                .any(|scope| Some(scope) == node.parent())
                || node.ancestors().any(|ancestor| ancestor == *owner.syntax())
        })
        .any(|node| {
            ast::MacroCall::can_cast(node.kind())
                || ast::Use::cast(node.clone()).is_some_and(|use_item| {
                    let text = compact(&use_item.syntax().text().to_string());
                    text.contains("Result") || text.contains('*')
                })
                || ast::TypeAlias::cast(node.clone())
                    .is_some_and(|item| item.name().is_some_and(|name| name.text() == "Result"))
                || ast::Struct::cast(node.clone())
                    .is_some_and(|item| item.name().is_some_and(|name| name.text() == "Result"))
                || ast::Enum::cast(node.clone())
                    .is_some_and(|item| item.name().is_some_and(|name| name.text() == "Result"))
        })
    {
        return false;
    }
    if matches!(error_type.as_str(), "&str" | "&'staticstr") {
        // Escape spellings can denote the same value (`"x"` vs `"\\x78"`).
        // Keep only ordinary unescaped strings until decoded literal identity
        // is available; raw/escaped spellings cannot establish distinct errors.
        return firing_error.starts_with('"')
            && !firing_error.contains('\\')
            && matches!(other_error, ast::Expr::Literal(_))
            && other.starts_with('"')
            && !other.contains('\\');
    }
    let Some(firing_variant) = firing_error.strip_prefix(&format!("{error_type}::")) else {
        return false;
    };
    let Some(other_variant) = other.strip_prefix(&format!("{error_type}::")) else {
        return false;
    };
    if !is_simple_identifier(&error_type)
        || !is_simple_identifier(firing_variant)
        || !is_simple_identifier(other_variant)
    {
        return false;
    }
    let mut definitions = source
        .tree()
        .syntax()
        .descendants()
        .filter_map(ast::Enum::cast)
        .filter(|definition| {
            definition
                .name()
                .is_some_and(|name| name.text() == error_type)
        });
    let Some(definition) = definitions.next() else {
        return false;
    };
    if definitions.next().is_some() {
        return false;
    }
    if owner.syntax().parent() != definition.syntax().parent() {
        return false;
    }
    let Some(variants) = definition.variant_list() else {
        return false;
    };
    [firing_variant, other_variant].into_iter().all(|expected| {
        variants.variants().any(|variant| {
            variant.field_list().is_none()
                && variant.name().is_some_and(|name| name.text() == expected)
        })
    })
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

    fn shape(owner: &FunctionSummary, expression: &str) -> Option<SliceCounterShape> {
        let source = format!("enum ChunkError {{ TooLong, BadDigit }}\n{}", owner.body);
        per_byte_counter_shape(owner, expression, &source)
    }

    const GRID_BITS: &str = "pub fn chunk_size(hex: &[u8]) -> Result<u64, ChunkError> {\n    let mut size: u64 = 0;\n    let mut count = 0;\n    for &b in hex {\n        count += 1;\n        if 16 < count {\n            return Err(ChunkError::TooLong);\n        }\n        let digit = (b as char).to_digit(16).ok_or(ChunkError::BadDigit)?;\n        size = (size << 4) | u64::from(digit);\n    }\n    Ok(size)\n}";

    fn boundary_with_lengths(expression: &str, lengths: &[usize]) -> Option<SliceCounterBoundary> {
        Some(SliceCounterBoundary {
            owner: SymbolId("src/lib.rs::chunk_size".to_string()),
            expression: expression.to_string(),
            predicate_line: 15,
            counter: "count".to_string(),
            predicate: parse_counter_predicate(expression)?,
            fire_value: "Err(ChunkError::TooLong)".to_string(),
            rows: lengths
                .iter()
                .map(|count| SliceCounterRow {
                    count: *count,
                    element: Some("b'f'".to_string()),
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
        let SliceCounterShape {
            slice_param: slice,
            predicate: found,
            fire_value: fire,
            requires_hex_input,
            ..
        } = shape(&owner, "16 < count").ok_or(())?;
        assert!(requires_hex_input);
        assert_eq!(fire, "Err(ChunkError::TooLong)");
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
            // the iterated slice is rebound before the loop
            GRID_BITS.replace(
                "    let mut count = 0;\n",
                "    let mut count = 0;\n    let hex = &hex[..4];\n",
            ),
            // A generic error parameter can shadow the retained enum name.
            GRID_BITS.replace("fn chunk_size(", "fn chunk_size<ChunkError: Errors>("),
            // counter starts at one
            GRID_BITS.replace("let mut count = 0;", "let mut count = 1;"),
            // increment by two
            GRID_BITS.replace("count += 1;", "count += 2;"),
        ] {
            assert!(
                shape(&owner_body(&body), "16 < count").is_none(),
                "{body}"
            );
        }
    }

    #[test]
    fn a_fallback_return_cannot_supply_the_counter_firing_value() {
        let fallback = GRID_BITS.replace(
            "        let digit",
            "        if count == 17 { return Err(ChunkError::TooLong); }\n        let digit",
        );
        assert!(shape(&owner_body(&fallback), "16 < count").is_none());
        let post_loop = GRID_BITS.replace(
            "    Ok(size)",
            "    if count == 17 { return Err(ChunkError::TooLong); }\n    Ok(size)",
        );
        assert!(shape(&owner_body(&post_loop), "16 < count").is_none());
        for masked in [
            GRID_BITS.replace(
                "    Ok(size)",
                "    if count == 17 { Err(ChunkError::TooLong) } else { Ok(size) }",
            ),
            GRID_BITS.replace("ChunkError::BadDigit)?", "ChunkError::TooLong)?"),
            GRID_BITS
                .replace(
                    "        let digit",
                    "        let fallback = ChunkError::TooLong;\n        let digit",
                )
                .replace("ChunkError::BadDigit)?", "fallback)?"),
            GRID_BITS.replace("ChunkError::BadDigit)?", "OtherError::BadDigit)?"),
            GRID_BITS.replace(
                "        let digit",
                "        fallible_fallback(count)?;\n        let digit",
            ),
        ] {
            assert!(
                shape(&owner_body(&masked), "16 < count").is_none(),
                "{masked}"
            );
        }
        assert!(shape(&owner_body(GRID_BITS), "16 < count").is_some());
    }

    #[test]
    fn result_constructor_bindings_cannot_mask_the_counter() {
        for extra in [
            "fn Ok(n: u64) -> Result<u64, ChunkError> { std::result::Result::Ok(n) }",
            "fn Err(_: ChunkError) -> Result<u64, ChunkError> { std::result::Result::Ok(0) }",
            "use crate::masked_ok as Ok;",
            "use crate::masked_ok as r#Ok;",
            "fn r#Ok(n: u64) -> Result<u64, ChunkError> { std::result::Result::Ok(n) }",
            "use crate::masked_err as Err;",
            "use crate::constructors::*;",
            "const Ok: fn(u64) -> Result<u64, ChunkError> = masked_ok;",
        ] {
            let source = format!("enum ChunkError {{ TooLong, BadDigit }}\n{GRID_BITS}\n{extra}");
            assert!(
                per_byte_counter_shape(&owner_body(GRID_BITS), "16 < count", &source).is_none(),
                "{extra}"
            );
        }
        let no_try = GRID_BITS.replace(
            "(b as char).to_digit(16).ok_or(ChunkError::BadDigit)?",
            "0u32",
        );
        let source = format!(
            "enum ChunkError {{ TooLong, BadDigit }}\n{no_try}\nfn Ok(n: u64) -> Result<u64, ChunkError> {{ std::result::Result::Ok(n) }}"
        );
        assert!(per_byte_counter_shape(&owner_body(&no_try), "16 < count", &source).is_none());
        let unrelated = format!(
            "enum ChunkError {{ TooLong, BadDigit }}\n{GRID_BITS}\n#[cfg(test)] mod other {{ fn masked_ok() {{}} use self::masked_ok as Ok; }}"
        );
        assert!(
            per_byte_counter_shape(&owner_body(GRID_BITS), "16 < count", &unrelated).is_some(),
            "a sibling test import cannot shadow the production owner"
        );
    }

    #[test]
    fn opaque_attributes_cannot_generate_result_constructors() {
        for extra in [
            "#[derive(InjectOk)] struct Generated;",
            "#[derive(macros::InjectOk)] struct Generated;",
            "#[derive(Debug)] struct Generated; use macros::InjectOk as Debug;",
            "#[derive(Debug)] struct Generated; use macros::InjectOk as r#Debug;",
            "#[derive(Debug)] struct Generated; use macros::*;",
            "#[cfg_attr(test, derive(InjectOk))] struct Generated;",
            "#[cfg_attr(not(test), derive(InjectOk))] struct Generated;",
            "#[cfg(any())] #[derive(InjectOk)] struct Generated;",
            "#[inject_ok] struct Generated;",
            "#[macro_use] extern crate macros;",
            "#[macro_use] mod macros {}",
        ] {
            let source = format!("enum ChunkError {{ TooLong, BadDigit }}\n{GRID_BITS}\n{extra}");
            assert!(
                per_byte_counter_shape(&owner_body(GRID_BITS), "16 < count", &source).is_none(),
                "{extra}"
            );
        }
        let source = format!(
            "#[derive(Debug, PartialEq, Eq)] enum ChunkError {{ TooLong, BadDigit }}\n{GRID_BITS}"
        );
        assert!(per_byte_counter_shape(&owner_body(GRID_BITS), "16 < count", &source).is_some());
        let unrelated =
            format!("{source}\n#[cfg(test)] mod tests {{ #[derive(Unknown)] struct Local; }}");
        assert!(per_byte_counter_shape(&owner_body(GRID_BITS), "16 < count", &unrelated).is_some());
    }

    #[test]
    fn fallible_hex_suffix_requires_the_unchanged_primitive_loop_byte() {
        let honest = shape(&owner_body(GRID_BITS), "16 < count");
        assert!(honest.is_some_and(|shape| shape.requires_hex_input));
        for source in [
            GRID_BITS.replace(
                "        let digit",
                "        let b = b'z';\n        let digit",
            ),
            GRID_BITS.replace("for &b in hex", "for _ in hex"),
            GRID_BITS.replace("for &b in hex", "for b in hex"),
            GRID_BITS.replace(
                "        let digit",
                "        let escaped = &mut b;\n        let digit",
            ),
        ] {
            assert!(
                shape(&owner_body(&source), "16 < count").is_none(),
                "{source}"
            );
        }
        for declaration in [
            "type char = u8;",
            "use crate::other as char;",
            "type u8 = Other;",
        ] {
            let source =
                format!("enum ChunkError {{ TooLong, BadDigit }}\n{declaration}\n{GRID_BITS}");
            assert!(
                per_byte_counter_shape(&owner_body(GRID_BITS), "16 < count", &source).is_none()
            );
        }
        for element in ["b'f'", "b'F'", "b'0'", "b'9'", "b'\\x66'"] {
            assert!(repeated_element_is_hex_digit(element), "{element}");
        }
        for element in [
            "b'z'",
            "b'/'",
            "b'\\0'",
            "unknown",
            "choose()",
            "choose(b'f')",
            "102",
        ] {
            assert!(!repeated_element_is_hex_digit(element), "{element}");
        }
    }

    #[test]
    fn counter_places_and_receivers_cannot_change_the_length_trajectory() {
        for statement in [
            "(count,) = (count + count / 16,);",
            "(count) = count + count / 16;",
            "[count] = [count + count / 16];",
            "count.clone_from(&(count + count / 16));",
            "(count).clone_from(&(count + count / 16));",
            "let ref mut alias = count; *alias += *alias / 16;",
            "let ignored = match count { ref mut alias => { *alias += *alias / 16; 0 } };",
            "let ignored = if let ref mut alias = count { *alias += *alias / 16; 0 } else { 0 };",
            "let alias = &mut (count); *alias += *alias / 16;",
            "let alias = &raw mut (count);",
        ] {
            let source = GRID_BITS.replace(
                "        let digit",
                &format!("        {statement}\n        let digit"),
            );
            assert!(
                shape(&owner_body(&source), "16 < count").is_none(),
                "{statement}"
            );
        }
        assert!(shape(&owner_body(GRID_BITS), "16 < count").is_some());
        assert!(
            shape(
                &owner_body(&GRID_BITS.replace(
                    "        let digit",
                    "        let read = count;\n        let digit"
                )),
                "16 < count"
            )
            .is_some(),
            "an ordinary by-value counter read is unchanged"
        );
    }

    #[test]
    fn counter_representation_bounds_the_predicate_and_input_lengths() {
        for (ty, limit) in [("i8", 127), ("u8", 255), ("usize", 65535)] {
            let base = GRID_BITS.replace("let mut count = 0", &format!("let mut count: {ty} = 0"));
            let valid = base.replace("16 < count", &format!("count >= {limit}"));
            let invalid = base.replace("16 < count", &format!("count >= {}", limit + 1));
            assert_eq!(
                shape(&owner_body(&valid), &format!("count >= {limit}"))
                    .map(|shape| shape.counter_max),
                Some(limit)
            );
            assert!(shape(&owner_body(&invalid), &format!("count >= {}", limit + 1)).is_none());
        }
        let inferred = GRID_BITS.replace("16 < count", "count >= 128");
        assert!(shape(&owner_body(&inferred), "count >= 128").is_none());
        let alias = GRID_BITS.replace("let mut count = 0", "let mut count: Counter = 0");
        assert!(shape(&owner_body(&alias), "16 < count").is_none());
        let shadow = GRID_BITS.replace("let mut count = 0", "let mut count: usize = 0");
        assert!(
            per_byte_counter_shape(
                &owner_body(&shadow),
                "16 < count",
                &format!("type usize = u8;\n{shadow}")
            )
            .is_none()
        );
        assert_eq!(
            shape(&owner_body(GRID_BITS), "16 < count").map(|shape| shape.counter_max),
            Some(127)
        );
    }

    #[test]
    fn escaped_literal_alias_cannot_establish_a_distinct_error() {
        let plain = GRID_BITS
            .replace("Result<u64, ChunkError>", "Result<u64, &'static str>")
            .replace("ChunkError::TooLong", "\"too long\"")
            .replace("ChunkError::BadDigit", "\"bad digit\"");
        assert!(shape(&owner_body(&plain), "16 < count").is_some());
        let masked = plain.replace("\"bad digit\"", "\"\\x74oo long\"");
        assert_ne!(plain, masked);
        assert!(shape(&owner_body(&masked), "16 < count").is_none());
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

    fn outcomes(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_string()).collect()
    }

    #[test]
    fn asserted_outcome_reads_assert_eq_in_either_order() {
        let call = "chunk_size(&[b'f'; 16])";
        let statement = "assert_eq!(chunk_size(&[b'f'; 16]), Ok(u64::MAX));";
        for (assertion, row, expected) in [
            (statement, call, Some("Ok(u64::MAX)")),
            // the call fact is the whole assertion statement
            (statement, statement, Some("Ok(u64::MAX)")),
            (
                "assert_eq!(Ok(u64::MAX), chunk_size(&[b'f'; 16]))",
                call,
                Some("Ok(u64::MAX)"),
            ),
            ("assert!(chunk_size(&[b'f'; 16]).is_ok())", call, None),
            // another length's assertion is not this row's
            ("assert_eq!(chunk_size(&[b'f'; 17]), Ok(1))", call, None),
            (
                "assert_eq!(chunk_size(&[b'f'; 17]), Ok(1));",
                statement,
                None,
            ),
            (
                "assert_eq!(chunk_size(&[b'f'; 16]), chunk_size(&[b'f'; 16]))",
                call,
                None,
            ),
        ] {
            assert_eq!(
                asserted_outcome(assertion, row, "chunk_size").as_deref(),
                expected,
                "{assertion} / {row}"
            );
        }
    }

    #[test]
    fn only_both_sides_asserted_with_differing_outcomes_discriminate() {
        let fire = "Err(ChunkError::TooLong)";
        // 16 asserts Ok, 17 asserts the firing value: pinned.
        assert!(outcomes_straddle(
            &outcomes(&["Ok(u64::MAX)"]),
            &outcomes(&[fire]),
            fire
        ));
        // Asserting only the 17-byte call.
        assert!(!outcomes_straddle(&[], &outcomes(&[fire]), fire));
        // Asserting only the 16-byte call.
        assert!(!outcomes_straddle(&outcomes(&["Ok(u64::MAX)"]), &[], fire));
        // BadDigit at both lengths: the loop exits at the first element, the
        // counter is never reached, and the two outcomes are equal.
        let bad = "Err(ChunkError::BadDigit)";
        assert!(!outcomes_straddle(
            &outcomes(&[bad]),
            &outcomes(&[bad]),
            fire
        ));
        // Different outcomes that are not the firing value do not count.
        assert!(!outcomes_straddle(
            &outcomes(&["Ok(1)"]),
            &outcomes(&[bad]),
            fire
        ));
        // Both sides assert the firing value.
        assert!(!outcomes_straddle(
            &outcomes(&[fire]),
            &outcomes(&[fire]),
            fire
        ));
    }

    #[test]
    fn the_firing_branch_must_be_a_single_return() {
        for body in [
            GRID_BITS.replace(
                "return Err(ChunkError::TooLong);",
                "log(count); return Err(ChunkError::TooLong);",
            ),
            GRID_BITS.replace("return Err(ChunkError::TooLong);", "break;"),
            GRID_BITS.replace(
                "        }\n        let digit",
                "        } else {\n            log(count);\n        }\n        let digit",
            ),
        ] {
            assert!(shape(&owner_body(&body), "16 < count").is_none(), "{body}");
        }
    }

    #[test]
    fn straddling_pairs_need_the_same_repeated_element() -> Result<(), ()> {
        let mut boundary = boundary_with_lengths("16 < count", &[16, 17]).ok_or(())?;
        assert_eq!(boundary.straddling_pairs().count(), 1);
        // `[b'z'; 16]` with `[b'f'; 17]` differ in content, not only length.
        boundary.rows[0].element = Some("b'z'".to_string());
        assert_eq!(boundary.straddling_pairs().count(), 0);
        // Non-repeat arguments carry no element and never pair.
        boundary.rows[0].element = None;
        boundary.rows[1].element = None;
        assert_eq!(boundary.straddling_pairs().count(), 0);
        Ok(())
    }
}
