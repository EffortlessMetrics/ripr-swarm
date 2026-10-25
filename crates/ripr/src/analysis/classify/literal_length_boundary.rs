//! One literal-owned byte-length operand, not general expression evaluation.
//! Typed rows, rather than rendered ValueFact text, authorize both infection
//! and the oracle paired to the exact call occurrence.

use super::activation::{ActivationWithBoundaryInput, sort_value_facts};
use super::context::ProbeContext;
use super::owner_pin::{OwnerPinSyntax, builtin_type_unshadowed, targeted_free_call_reaches_owner};
use super::reach::{includes_external_docs_with, names_identifier_outside_fn_definition};
use super::targeted_calls::{TargetedCallShape, TargetedCallSyntax};
use crate::analysis::facts::{
    FunctionImplContext, FunctionSummary, OracleFact, RustIndex, TestSummary,
};
use crate::analysis::syntax::{owner_reference_count, parse_clean_source_file};
use crate::domain::{
    Confidence, FlowSinkFact, FlowSinkKind, Probe, ProbeFamily, StageEvidence, StageState,
    SymbolId, ValueContext,
};
use crate::value_origins::LocatedValueFact as ValueFact;
use ra_ap_syntax::{
    AstNode, AstToken,
    ast::{self, HasArgList, HasAttrs, HasGenericArgs, HasGenericParams, HasName},
};
use std::{ops::Range, path::PathBuf};

const MAX_ROWS: usize = 64;
const MAX_LITERAL_BYTES: usize = 512;

pub(in crate::analysis) struct LiteralLengthBoundary {
    owner: SymbolId,
    expression: String,
    predicate_line: usize,
    local: String,
    rows: Vec<LiteralLengthRow>,
}

struct LiteralLengthRow {
    test_file: PathBuf,
    test_name: String,
    test_start: usize,
    call_range: Range<usize>,
    call_line: usize,
    call_text: String,
    input: String,
    width: usize,
    byte_length: usize,
}

impl LiteralLengthBoundary {
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
        (self.valid_for(probe, owner) && self.rows.iter().any(|row| row.byte_length == row.width))
            .then(|| {
                StageEvidence::new(
                    StageState::Yes,
                    Confidence::Medium,
                    "A literal-owned call row reaches the string byte-length boundary",
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
                row.byte_length == row.width
                    && tests.iter().any(|test| {
                        test.file == row.test_file
                            && test.name == row.test_name
                            && test.start_line == row.test_start
                            && index.files().get(&test.file).is_some_and(|file| {
                                test.assertions.iter().any(|assertion| {
                                    assertion_admitted(test, assertion)
                                        && owner_pinned(test, assertion)
                                        && row_is_asserted(row, assertion, &file.source)
                                })
                            })
                    })
            })
    }
}

pub(in crate::analysis) fn refine_literal_length_boundary(
    context: &ProbeContext<'_>,
    syntax: &OwnerPinSyntax,
    gathered: &mut ActivationWithBoundaryInput,
    flow_sinks: &mut Vec<FlowSinkFact>,
) {
    let Some(boundary) = boundary(context, syntax) else {
        return;
    };
    // The same closed AST establishes that this exact predicate is the
    // owner's whole returned value. Existing propagation owns the decision.
    if !flow_sinks.iter().any(|sink| {
        sink.kind == FlowSinkKind::ReturnValue
            && sink.owner.as_ref() == Some(&boundary.owner)
            && sink.line == boundary.predicate_line
            && sink.text == boundary.expression
    }) {
        flow_sinks.push(FlowSinkFact {
            kind: FlowSinkKind::ReturnValue,
            text: boundary.expression.clone(),
            line: boundary.predicate_line,
            owner: Some(boundary.owner.clone()),
        });
    }
    for row in &boundary.rows {
        gathered.activation.observed_values.push(ValueFact {
            file: Some(row.test_file.clone()),
            line: row.call_line,
            text: format!(
                "{} | immutable string literal {:?} has byte length {} via as_bytes().len()",
                row.call_text, row.input, row.byte_length
            ),
            value: format!("{} = {}", boundary.local, row.byte_length),
            // A derived local is not an owner-call input or an equality
            // token for the legacy activation matcher.
            context: ValueContext::Unknown,
        });
    }
    sort_value_facts(&mut gathered.activation.observed_values);
    gathered.literal_length_boundary = Some(boundary);
}

fn boundary(context: &ProbeContext<'_>, syntax: &OwnerPinSyntax) -> Option<LiteralLengthBoundary> {
    let owner = context.owner_fn?;
    if context.probe.family != ProbeFamily::Predicate
        || context.probe.owner.as_ref() != Some(&owner.id)
        || !context.workspace_complete
        || context.helper_chain.is_some()
        || owner.impl_context != FunctionImplContext::Free
        || !owner.attrs.is_empty()
        || context
            .probe
            .after
            .as_deref()
            .is_none_or(|after| !compact(after).ends_with(".as_bytes().len()"))
    {
        return None;
    }
    let parse = parse_clean_source_file(&owner.body)?;
    let function = parse.tree().syntax().children().find_map(ast::Fn::cast)?;
    if function.async_token().is_some()
        || function.const_token().is_some()
        || function.unsafe_token().is_some()
        || function.generic_param_list().is_some()
        || function.attrs().next().is_some()
        || function
            .ret_type()?
            .ty()?
            .syntax()
            .text()
            .to_string()
            .trim()
            != "bool"
    {
        return None;
    }
    let parameters = function.param_list()?;
    if parameters.self_param().is_some() {
        return None;
    }
    let parameters = parameters.params().collect::<Vec<_>>();
    let [text, width] = parameters.as_slice() else {
        return None;
    };
    let text_name = plain_parameter(text, "&str")?;
    let width_name = plain_parameter(width, "usize")?;
    let statements = function.body()?.stmt_list()?;
    let all = statements.statements().collect::<Vec<_>>();
    let [ast::Stmt::LetStmt(binding)] = all.as_slice() else {
        return None;
    };
    if binding.attrs().next().is_some() || binding.ty().is_some() || binding.let_else().is_some() {
        return None;
    }
    let ast::Pat::IdentPat(local) = binding.pat()? else {
        return None;
    };
    if local.mut_token().is_some() || local.ref_token().is_some() || local.at_token().is_some() {
        return None;
    }
    let local = local.name()?.text().to_string();
    if local == text_name || local == width_name {
        return None;
    }
    let initializer = binding.initializer()?;
    if compact(&initializer.syntax().text().to_string()) != format!("{text_name}.as_bytes().len()")
    {
        return None;
    }
    let tail = statements.tail_expr()?;
    if compact(&tail.syntax().text().to_string()) != format!("{local}<={width_name}")
        || compact(&context.probe.expression) != format!("{local}<={width_name}")
        || context.probe.location.line
            != owner.start_line
                + owner
                    .body
                    .get(..usize::from(tail.syntax().text_range().start()))?
                    .bytes()
                    .filter(|b| *b == b'\n')
                    .count()
        || context.probe.after.as_deref().map(compact)
            != Some(format!("{text_name}.as_bytes().len()"))
        // Only the `len()` spelling is a known no-op rewrite. Any other old
        // initializer (`chars().count()`, a trimmed length) may have computed
        // a different value, so the edit could change behavior and the rows
        // were never evaluated against it.
        || context.probe.before.as_deref().map(compact)
            != Some(format!("{text_name}.len()"))
    {
        return None;
    }
    if !["str", "usize", "bool"]
        .iter()
        .all(|name| builtin_type_unshadowed(name, context.index, syntax))
    {
        return None;
    }

    let calls = TargetedCallSyntax::new(context.index);
    let mut rows = Vec::new();
    for (test, _) in &context.related_tests {
        let sites = calls.occurrences(
            test,
            syntax.targeted_source_provenance_is_admitted(test, context.index),
            |line, name| {
                syntax.targeted_macro_binding_is_unambiguous(test, line, name, context.index)
            },
        );
        for site in sites.into_iter().filter(|site| site.name == owner.name) {
            if site.shape != TargetedCallShape::Bare
                || !targeted_free_call_reaches_owner(test, owner, None, context.index, syntax)
            {
                return None;
            }
            let range = site.range?;
            if !context
                .index
                .files()
                .get(&test.file)
                .is_some_and(|facts| straight_line_call(&facts.source, test.start_line, &range))
            {
                return None;
            }
            let (input, width) = literal_arguments(&site.text)?;
            rows.push(LiteralLengthRow {
                test_file: test.file.clone(),
                test_name: test.name.clone(),
                test_start: test.start_line,
                call_range: range,
                call_line: site.line,
                call_text: site.text,
                byte_length: input.len(),
                input,
                width,
            });
            if rows.len() > MAX_ROWS {
                return None;
            }
        }
    }
    let references = context
        .index
        .files()
        .iter()
        .try_fold(0usize, |count, (path, facts)| {
            if includes_external_docs_with(&facts.source, false) {
                return None;
            }
            // Reuse the native free-function reach boundary: source that
            // never names this owner outside a declaration cannot supply an
            // explicit call/reference. This does not resolve generated calls
            // or expand foreign macros. Primitive-name authority stays global.
            if path != &owner.file
                && !names_identifier_outside_fn_definition(&facts.source, &owner.name)
            {
                return Some(count);
            }
            count.checked_add(owner_reference_count(&facts.source, &owner.name)?)
        });
    // Match the existing literal-slice family's conservative completeness
    // authority: one declaration plus these exact source calls. An alias,
    // function pointer or explicitly named hidden input cannot disappear.
    // In these owner-referencing files its closed source grammar also refuses
    // imports except `use super::*`, even when primitive authority admits one.
    if rows.is_empty() || references != rows.len().checked_add(1) {
        return None;
    }
    Some(LiteralLengthBoundary {
        owner: owner.id.clone(),
        expression: context.probe.expression.clone(),
        predicate_line: context.probe.location.line,
        local,
        rows,
    })
}

fn plain_parameter(parameter: &ast::Param, ty: &str) -> Option<String> {
    if parameter.attrs().next().is_some()
        || compact(&parameter.ty()?.syntax().text().to_string()) != ty
    {
        return None;
    }
    let ast::Pat::IdentPat(pattern) = parameter.pat()? else {
        return None;
    };
    if pattern.mut_token().is_some()
        || pattern.ref_token().is_some()
        || pattern.at_token().is_some()
    {
        return None;
    }
    Some(pattern.name()?.text().to_string())
}

fn literal_arguments(text: &str) -> Option<(String, usize)> {
    let parse = parse_clean_source_file(&format!("fn row() {{ let _ = {text}; }}"))?;
    let binding = parse
        .tree()
        .syntax()
        .descendants()
        .find_map(ast::LetStmt::cast)?;
    let ast::Expr::CallExpr(call) = binding.initializer()? else {
        return None;
    };
    let ast::Expr::PathExpr(path) = call.expr()? else {
        return None;
    };
    if path
        .path()?
        .segments()
        .any(|segment| segment.generic_arg_list().is_some())
    {
        return None;
    }
    let args = call.arg_list()?.args().collect::<Vec<_>>();
    let [ast::Expr::Literal(input), ast::Expr::Literal(width)] = args.as_slice() else {
        return None;
    };
    let ast::LiteralKind::String(input) = input.kind() else {
        return None;
    };
    if input.text().len() > MAX_LITERAL_BYTES {
        return None;
    }
    let input = input.value().ok()?.into_owned();
    let width = width.syntax().text().to_string();
    if width.is_empty() || !width.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    Some((input, width.parse().ok()?))
}

/// The row's call must run whenever its test runs: a direct statement of the
/// test body, with no branch, loop, closure, `?` or early exit anywhere in it.
/// Without this an assertion under `if false { .. }` would still supply a row.
fn straight_line_call(source: &str, test_start_line: usize, call: &Range<usize>) -> bool {
    let line_start = source
        .split_inclusive('\n')
        .take(test_start_line.saturating_sub(1))
        .map(str::len)
        .sum::<usize>();
    let masked = crate::analysis::language::mask_rust_comments_and_strings(source);
    let Some(rest) = masked.get(line_start..) else {
        return false;
    };
    let Some(open) = rest.find('{').map(|at| at + line_start) else {
        return false;
    };
    let mut depth = 0usize;
    let mut close = None;
    for (at, byte) in masked.bytes().enumerate().skip(open) {
        match byte {
            b'{' => depth += 1,
            b'}' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    close = Some(at);
                    break;
                }
            }
            _ => {}
        }
    }
    let Some(close) = close else {
        return false;
    };
    if call.start <= open || call.end > close {
        return false;
    }
    let body = masked.get(open + 1..close).unwrap_or_default();
    // A row on the right of `||` or `&&` in its own statement is
    // short-circuited and may never run (`assert!(true || !fits(..))`).
    let statement_start = masked
        .get(open + 1..call.start)
        .and_then(|before| before.rfind(';').map(|at| open + 1 + at + 1))
        .unwrap_or(open + 1);
    if masked
        .get(statement_start..call.start)
        .is_none_or(|before| before.contains("||") || before.contains("&&"))
    {
        return false;
    }
    if body.contains(['{', '}']) && {
        // Only the macro-invocation parentheses/brackets of straight-line
        // statements are expected; any inner brace is a block or branch.
        masked
            .get(open + 1..call.start)
            .is_none_or(|before| before.contains(['{', '}']))
    } {
        return false;
    }
    !body.contains('?')
        && !has_closure_head(body)
        && !body
            .split(|c: char| !(c.is_alphanumeric() || c == '_'))
            .any(|word| {
                matches!(
                    word,
                    "if" | "else"
                        | "match"
                        | "loop"
                        | "while"
                        | "for"
                        | "return"
                        | "break"
                        | "continue"
                        | "async"
                        | "move"
                        | "unsafe"
                )
            })
}

/// A closure head: `|args|`, or an empty `||` where an operand is expected
/// (a boolean `a || b` has an operand before it and stays admitted).
fn has_closure_head(body: &str) -> bool {
    let bytes = body.as_bytes();
    bytes.iter().enumerate().any(|(at, byte)| {
        if *byte != b'|' {
            return false;
        }
        let single = bytes.get(at + 1) != Some(&b'|') && (at == 0 || bytes[at - 1] != b'|');
        if single {
            return true;
        }
        if bytes.get(at + 1) != Some(&b'|') {
            return false;
        }
        bytes[..at]
            .iter()
            .rev()
            .find(|b| !b.is_ascii_whitespace())
            .is_none_or(|prev| matches!(prev, b'=' | b'(' | b',' | b'{' | b';' | b':' | b'&'))
    })
}

fn row_is_asserted(row: &LiteralLengthRow, assertion: &OracleFact, source: &str) -> bool {
    if row.call_line != assertion.line {
        return false;
    }
    let Some(line) = source.lines().nth(assertion.line.saturating_sub(1)) else {
        return false;
    };
    let text = assertion.text.trim();
    let mut matches = line.match_indices(text);
    let (Some((offset, _)), None) = (matches.next(), matches.next()) else {
        return false;
    };
    let line_start = source
        .split_inclusive('\n')
        .take(assertion.line.saturating_sub(1))
        .map(str::len)
        .sum::<usize>();
    let start = line_start + offset;
    row.call_range.start >= start
        && row.call_range.end <= start + text.len()
        && source.get(row.call_range.clone()) == Some(row.call_text.as_str())
}

fn compact(text: &str) -> String {
    text.chars()
        .filter(|character| !character.is_whitespace())
        .collect()
}
