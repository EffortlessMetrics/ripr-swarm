//! RIPR-SPEC-0240: an exact pin whose owner tail ripr cannot read is not a gap.
//!
//! The owner-return pin (`owner_pin`) credits an `assert_eq!` on a direct call
//! only when the changed expression is a tail it can read: one that evaluates
//! every part on every input. Three owner shapes fall outside that reading
//! even though the isolated test pins the call's whole result exactly:
//!
//! - **closure tail**: the tail holds a closure the call runs (a spawned
//!   thread, a returned closure the test applies, an adaptor chain the test or
//!   the owner collects);
//! - **returned receiver**: the changed statement is a method call on a value
//!   the owner then returns (`v.sort_by_key(..); v`);
//! - **let-else exit**: the changed line is the early exit of a `let ... else`,
//!   whose pattern the pinned input decides;
//! - **accumulated local**: the changed statement adds into a zero-initialized
//!   local the owner returns, once per element of a slice parameter
//!   (RIPR-SPEC-0360). The owner pin credits it only for a literal non-empty
//!   input and a nonzero literal total; a pin it cannot read that way (a
//!   bound input, a computed expectation) is withheld here.
//!
//! Reading those as "no assertion references the change" is a certain claim
//! about a test that may well notice it, so the gap is withheld under
//! `rust_pinned_shape_unresolved` and nothing is credited. The rule only
//! withholds, so the actionability flip stays fail-closed.

use super::gap_admission::{is_ignored, withhold_gap};
use super::owner_pin::closure_adaptor::call_argument_texts;
use super::owner_pin::{CONDITIONAL_COMBINATORS, closure_adaptor::adaptor_tail_of};
use super::reveal::assertion_comparison_operands;
use crate::analysis::facts::TestFact;
use crate::analysis::rust_index::RustIndex;
use crate::analysis::syntax::loop_accumulator::loop_accumulator_at;
use crate::analysis::syntax::parse_clean_source_file;
use crate::analysis::test_grip_evidence::text_calls;
use crate::domain::*;
use ra_ap_syntax::{AstNode, SyntaxNode, WalkEvent, ast, ast::HasLoopBody};
use std::ops::Range;

const LIMIT: StaticLimitKind = StaticLimitKind::RustPinnedShapeUnresolved;
const FIRST_UNRESOLVED_EDGE: &str =
    "whether the exact value the related test pins passes through the changed code";

/// The owner shape the pin rules do not read.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Shape {
    ClosureTail,
    ReturnedReceiver,
    LetElseExit,
    AccumulatedLocal,
}

impl Shape {
    fn advice(self) -> &'static str {
        match self {
            Shape::ClosureTail => {
                "the changed code is part of a closure the call runs, and ripr does not map the pinned input to what the closure does"
            }
            Shape::ReturnedReceiver => {
                "the changed statement acts on the value the call returns, and ripr does not follow that effect into the pinned value"
            }
            Shape::LetElseExit => {
                "the changed code is the `else` exit of a `let ... else`, and ripr does not decide whether the pinned input takes it"
            }
            Shape::AccumulatedLocal => {
                "the changed statement adds into a local the call returns, and ripr cannot read that the pinned input walks the loop and totals to a nonzero value"
            }
        }
    }
}

/// Withhold a `weakly_exposed` gap whose related direct-call test pins the
/// owner's whole result exactly while the changed code sits in a [`Shape`]
/// the owner pin does not read. Returns `true` when the finding was
/// rewritten to `static_unknown`.
pub(in crate::analysis) fn withhold_pinned_shape_gap(
    finding: &mut Finding,
    index: &RustIndex,
) -> bool {
    if finding.static_limit_kind.is_some() {
        return false;
    }
    let summary = &finding.ripr.reveal.discriminate.summary;
    // RIPR-SPEC-0360: a pin inside a loop the table reader could not establish
    // reads `reachable_unrevealed` with the assertion context unestablished;
    // only the accumulator shape, whose table-driven pins credit when
    // readable, is withheld there (checked below).
    let loop_unestablished = finding.class == ExposureClass::ReachableUnrevealed
        && summary.contains("(rust_assertion_context_unestablished)");
    if !loop_unestablished
        && (finding.class != ExposureClass::WeaklyExposed
            || !summary.contains("(observation_unverified)"))
    {
        return false;
    }
    // A different missing leg keeps its own gap. Propagation reads `weak`
    // through these shapes, which is the unread part this rule names.
    let ripr = &finding.ripr;
    if ripr.reach.state != StageState::Yes
        || ripr.infect.state != StageState::Yes
        || !matches!(ripr.propagate.state, StageState::Yes | StageState::Weak)
    {
        return false;
    }
    let Some(owner_id) = finding.probe.owner.as_ref() else {
        return false;
    };
    let changed_line = finding.probe.location.line;
    let Some(owner) = index.functions().iter().find(|function| {
        function.id == *owner_id
            && function.start_line <= changed_line
            && changed_line <= function.end_line
    }) else {
        return false;
    };
    let Some(shape) = unread_shape(
        &owner.body,
        changed_line - owner.start_line,
        &finding.probe.family,
    ) else {
        return false;
    };
    if loop_unestablished && shape != Shape::AccumulatedLocal {
        return false;
    }
    // The accumulator's walked argument, so an empty literal elsewhere in the
    // call does not make the pin degenerate.
    let accumulator = loop_accumulator_at(&owner.body, changed_line - owner.start_line);
    let walked = accumulator
        .as_ref()
        .map(|accumulator| accumulator.walked_position);
    let call_term = accumulator.is_some_and(|accumulator| accumulator.term.is_some());
    let labels: Vec<String> = finding
        .related_tests
        .iter()
        .filter(|related| related.relation_reason == Some(RelationReason::DirectOwnerCall))
        .filter_map(|related| {
            let test = index.tests().iter().find(|test| {
                test.name == related.name
                    && test.file == related.file
                    && test.start_line == related.line
            })?;
            // A call term's value is a function ripr may not read, so a related
            // direct test that does not credit it never reports a gap (the
            // pin check below is for the plain term).
            (call_term || pins_owner_exactly(test, &owner.name, shape, walked)).then(|| {
                format!(
                    "`{}` ({}:{})",
                    related.name,
                    related.file.display().to_string().replace('\\', "/"),
                    related.line
                )
            })
        })
        .collect();
    if labels.is_empty() {
        return false;
    }
    let next_step = if call_term {
        format!(
            "{} reaches this code, but the changed statement adds the value of a function call into a local the call returns, and ripr cannot read that the pinned input folds through that function to a value its neighbours do not share, so ripr does not report a gap. Check that the pinned input runs the changed code and that its expected value differs between the old and the new behavior; add a test only if either fails.",
            labels.join(", ")
        )
    } else {
        format!(
            "{} pins this code's whole result with an exact `assert_eq!`, but {}, so ripr does not report a gap. Check that the pinned input runs the changed code and that its expected value differs between the old and the new behavior; add a test only if either fails.",
            labels.join(", "),
            shape.advice()
        )
    };
    withhold_gap(finding, LIMIT, FIRST_UNRESOLVED_EDGE, next_step, &labels);
    true
}

/// #9490: an `Err(..)` pin does not reach the `Ok` path of a
/// `map(..).collect::<Result<Vec<_>, _>>()` owner tail. A `Result` collect
/// stops at the first `Err` and drops the earlier values, so a test whose
/// only exact pins on the call are `Err(..)` never observes a changed closure
/// value on a later element. Withhold an `exposed` finding whose related
/// direct-call tests carry no `Ok(..)` pin (or `.unwrap()` of the call).
pub(in crate::analysis) fn withhold_result_collect_err_pin(
    finding: &mut Finding,
    index: &RustIndex,
) -> bool {
    if finding.static_limit_kind.is_some()
        || finding.class != ExposureClass::Exposed
        || !matches!(finding.probe.family, ProbeFamily::ReturnValue)
    {
        return false;
    }
    let Some(owner_id) = finding.probe.owner.as_ref() else {
        return false;
    };
    let changed_line = finding.probe.location.line;
    let Some(owner) = index.functions().iter().find(|function| {
        function.id == *owner_id
            && function.start_line <= changed_line
            && changed_line <= function.end_line
    }) else {
        return false;
    };
    let collects_result =
        super::owner_pin::closure_adaptor::admitted_owner_adaptor_tail(owner, index)
            .is_some_and(|tail| tail.tail_is(&finding.probe.expression) && tail.collects_result());
    if !collects_result {
        return false;
    }
    let direct: Vec<&TestFact> = finding
        .related_tests
        .iter()
        .filter(|related| related.relation_reason == Some(RelationReason::DirectOwnerCall))
        .filter_map(|related| {
            index.tests().iter().find(|test| {
                test.name == related.name
                    && test.file == related.file
                    && test.start_line == related.line
            })
        })
        .collect();
    if direct.is_empty() || direct.iter().any(|test| pins_result_ok(test, &owner.name)) {
        return false;
    }
    let labels: Vec<String> = direct
        .iter()
        .map(|test| format!("`{}`", test.name))
        .collect();
    let next_step = format!(
        "{} pins this `Result` collect only through an `Err(..)` value, and a collect stops at the first `Err`, so ripr does not report the change as observed. Pin an `Ok(..)` result whose inputs all succeed so every element reaches the changed closure.",
        labels.join(", ")
    );
    withhold_gap(
        finding,
        LIMIT,
        "whether the pinned `Err` is produced before the changed closure value matters",
        next_step,
        &labels,
    );
    true
}

/// Whether `test` is a live test whose plain `assert_eq!` compares the
/// owner's whole call with an `Ok(..)` value, or the call's `.unwrap()` with
/// anything: every element ran for that pin to hold.
fn pins_result_ok(test: &TestFact, owner: &str) -> bool {
    if is_ignored(test)
        || test
            .attrs
            .iter()
            .map(String::as_str)
            .chain(std::iter::once(test.body.as_str()))
            .any(|text| text.contains("should_panic"))
    {
        return false;
    }
    test.assertions.iter().any(|assertion| {
        assertion_runs_unconditionally(&test.body, &assertion.text)
            && assertion.text.trim_start().starts_with("assert_eq!")
            && assertion_comparison_operands(&assertion.text).is_some_and(|[left, right]| {
                let ok_side = |call: &str, other: &str| {
                    text_calls(call, owner)
                        && !contains_name(other, owner)
                        && is_whole_call(call, owner)
                        && !degenerate_input(call)
                        && (other.trim_start().starts_with("Ok(")
                            || call.trim_end().ends_with(".unwrap()"))
                };
                ok_side(left, right) || ok_side(right, left)
            })
    })
}

/// Whether `test` is a live test with a plain `assert_eq!` whose one operand
/// is the owner's call itself (see [`is_whole_call`]) on an input that is not
/// empty or zero, and whose other operand does not name the owner.
fn pins_owner_exactly(test: &TestFact, owner: &str, shape: Shape, walked: Option<usize>) -> bool {
    if is_ignored(test)
        || test
            .attrs
            .iter()
            .map(String::as_str)
            .chain(std::iter::once(test.body.as_str()))
            .any(|text| text.contains("should_panic"))
    {
        return false;
    }
    test.assertions.iter().any(|assertion| {
        (assertion_runs_unconditionally(&test.body, &assertion.text)
            // RIPR-SPEC-0360: a table loop's `assert_eq!` is how this shape is
            // usually pinned. Credit reads the rows it can; a pin it cannot
            // read is withheld, never left as a gap.
            || (shape == Shape::AccumulatedLocal
                && assertion_is_top_level_loop_statement(&test.body, &assertion.text)))
            && assertion.strength == OracleStrength::Strong
            && assertion.kind == OracleKind::ExactValue
            && assertion.text.trim_start().starts_with("assert_eq!")
            && assertion_comparison_operands(&assertion.text).is_some_and(|[left, right]| {
                let pins = |call: &str, other: &str| {
                    text_calls(call, owner)
                        && !contains_name(other, owner)
                        && is_whole_call(call, owner)
                        && !degenerate_pin(shape, walked, owner, call, other)
                };
                pins(left, right) || pins(right, left)
            })
    })
}

/// Whether `assertion` is a statement of the test's own body, before any
/// `return`: not inside an `if`, `match`, loop, closure or nested block that
/// may not run it, which the owner pin's execution admission would refuse.
fn assertion_runs_unconditionally(test_body: &str, assertion: &str) -> bool {
    let squash = |text: &str| -> String {
        text.chars()
            .filter(|ch| !ch.is_whitespace())
            .collect::<String>()
            .trim_end_matches(';')
            .to_string()
    };
    let Some(parse) = parse_clean_source_file(test_body) else {
        return false;
    };
    let Some(list) = parse
        .tree()
        .syntax()
        .descendants()
        .find_map(ast::Fn::cast)
        .and_then(|function| function.body())
        .and_then(|block| block.stmt_list())
    else {
        return false;
    };
    let wanted = squash(assertion);
    for statement in list.statements() {
        if squash(&statement.syntax().text().to_string()) == wanted {
            return true;
        }
        if let ast::Stmt::ExprStmt(expr) = &statement
            && matches!(expr.expr(), Some(ast::Expr::ReturnExpr(_)))
        {
            return false;
        }
    }
    list.tail_expr()
        .is_some_and(|tail| squash(&tail.syntax().text().to_string()) == wanted)
}

/// Whether `assertion` is a statement of the body of a `for` loop that is
/// itself a top-level statement of the test, before any `return`: it runs once
/// per iteration, which the table-driven pin reads row by row.
fn assertion_is_top_level_loop_statement(test_body: &str, assertion: &str) -> bool {
    let squash = |text: &str| -> String {
        text.chars()
            .filter(|ch| !ch.is_whitespace())
            .collect::<String>()
            .trim_end_matches(';')
            .to_string()
    };
    let Some(parse) = parse_clean_source_file(test_body) else {
        return false;
    };
    let Some(list) = parse
        .tree()
        .syntax()
        .descendants()
        .find_map(ast::Fn::cast)
        .and_then(|function| function.body())
        .and_then(|block| block.stmt_list())
    else {
        return false;
    };
    let wanted = squash(assertion);
    let in_loop = |table: &ast::ForExpr| {
        table
            .loop_body()
            .and_then(|body| body.stmt_list())
            .is_some_and(|body| {
                body.statements()
                    .any(|inner| squash(&inner.syntax().text().to_string()) == wanted)
                    || body
                        .tail_expr()
                        .is_some_and(|tail| squash(&tail.syntax().text().to_string()) == wanted)
            })
    };
    // An earlier `return`, `?` or `panic` anywhere before the loop, in a
    // branch too, may leave the test before the table runs: the assertion
    // is then not established and its gap is not withheld.
    let may_leave = |text: &str| {
        let masked = text.replace(char::is_whitespace, "");
        masked.contains("return") || masked.contains('?') || masked.contains("panic!")
    };
    for statement in list.statements() {
        if let ast::Stmt::ExprStmt(expr) = &statement {
            match expr.expr() {
                Some(ast::Expr::ForExpr(table)) if in_loop(&table) => return true,
                _ => {}
            }
        }
        if may_leave(&statement.syntax().text().to_string()) {
            return false;
        }
    }
    matches!(list.tail_expr(), Some(ast::Expr::ForExpr(table)) if in_loop(&table))
}

/// The operand is `owner(..)`, optionally applied again (`owner(..)(..)`) or
/// consumed by `.collect()` / `.unwrap()`, and nothing else: a projection
/// (`.len()`, a field, an operator, a wrapping call) pins less than the whole
/// result.
fn is_whole_call(operand: &str, owner: &str) -> bool {
    let Some(mut rest) = operand.trim().strip_prefix(owner) else {
        return false;
    };
    let mut groups = 0;
    loop {
        rest = rest.trim_start();
        if rest.is_empty() {
            return groups > 0;
        }
        if rest.starts_with('(') {
            let mut depth = 0usize;
            let mut end = None;
            for (at, ch) in rest.char_indices() {
                match ch {
                    '(' => depth += 1,
                    ')' => {
                        depth -= 1;
                        if depth == 0 {
                            end = Some(at + 1);
                            break;
                        }
                    }
                    _ => {}
                }
            }
            let Some(end) = end else {
                return false;
            };
            rest = &rest[end..];
            groups += 1;
        } else if let Some(after) = rest.strip_prefix(".unwrap()") {
            rest = after;
        } else if let Some(after) = rest.strip_prefix(".collect") {
            let after = after.trim_start();
            let after = match after.strip_prefix("::<") {
                Some(generic) => {
                    let mut depth = 1usize;
                    let Some(end) = generic.char_indices().find_map(|(at, ch)| {
                        match ch {
                            '<' => depth += 1,
                            '>' => depth -= 1,
                            _ => {}
                        }
                        (depth == 0).then_some(at + 1)
                    }) else {
                        return false;
                    };
                    &generic[end..]
                }
                None => after,
            };
            let Some(after) = after.trim_start().strip_prefix("()") else {
                return false;
            };
            rest = after;
        } else {
            return false;
        }
    }
}

/// The pin `call == other` leaves the usual reading for `shape`. The
/// accumulator walks every element, so one zero element is no more degenerate
/// than any other: only an empty input or a zero total (the update then adds
/// nothing a pin could notice) is.
fn degenerate_pin(
    shape: Shape,
    walked: Option<usize>,
    owner: &str,
    call: &str,
    other: &str,
) -> bool {
    if shape != Shape::AccumulatedLocal {
        return degenerate_input(call);
    }
    let is_empty = |text: &str| {
        let compact: String = text.chars().filter(|ch| !ch.is_whitespace()).collect();
        ["[]", "::new()", "Vec::new"]
            .iter()
            .any(|empty| compact.contains(empty))
    };
    // Only the walked argument can be an empty run; other arguments are
    // read whole when the call cannot be split.
    let empty_input = match walked.and_then(|at| {
        call_argument_texts(call, owner).and_then(|arguments| arguments.get(at).cloned())
    }) {
        Some(argument) => is_empty(&argument),
        None => is_empty(call),
    };
    empty_input || is_zero_literal(other)
}

/// `0`, `-0`, `0_u32`, `0x0` and the like: an integer literal that is zero.
fn is_zero_literal(text: &str) -> bool {
    let compact: String = text.chars().filter(|ch| *ch != '_').collect();
    let digits = compact.trim().trim_start_matches('-');
    let Some(rest) = digits.strip_prefix('0') else {
        return false;
    };
    // `u` and `i` are not digits in any base, so the first one starts the suffix.
    let (mantissa, suffix) = rest.split_at(rest.find(['u', 'i']).unwrap_or(rest.len()));
    let radix_digits = match mantissa.chars().next() {
        Some('x' | 'b' | 'o') => &mantissa[1..],
        _ => mantissa,
    };
    let radix_prefixed = radix_digits.len() != mantissa.len();
    (!radix_prefixed || !radix_digits.is_empty())
        && radix_digits.chars().all(|ch| ch == '0')
        && (suffix.is_empty() || suffix[1..].chars().all(|ch| ch.is_ascii_alphanumeric()))
}

/// An input the closure or the `else` exit need not see: an empty literal,
/// `None`, or a bare `0`. Such a pin leaves the gap to the usual reading.
fn degenerate_input(call: &str) -> bool {
    let compact: String = call.chars().filter(|ch| !ch.is_whitespace()).collect();
    // A string literal's own spaces are not an empty string.
    if call.contains("\"\"") {
        return true;
    }
    if ["[]", "None", "::new()", "Vec::new", "String::new"]
        .iter()
        .any(|empty| compact.contains(empty))
    {
        return true;
    }
    let bytes = compact.as_bytes();
    bytes.iter().enumerate().any(|(at, &byte)| {
        byte == b'0'
            && !bytes[..at]
                .last()
                .is_some_and(|prev| prev.is_ascii_alphanumeric() || matches!(prev, b'_' | b'.'))
            && !bytes[at + 1..]
                .first()
                .is_some_and(|next| next.is_ascii_alphanumeric() || matches!(next, b'_' | b'.'))
    })
}

fn contains_name(text: &str, name: &str) -> bool {
    let boundary = |ch: char| !(ch.is_alphanumeric() || ch == '_');
    text.match_indices(name).any(|(at, _)| {
        text[..at].chars().next_back().is_none_or(boundary)
            && text[at + name.len()..].chars().next().is_none_or(boundary)
    })
}

/// The unread [`Shape`] the changed line (`line_index`, an offset from the
/// owner text's first line) sits in, for an owner whose only exit is its tail
/// (or, for a let-else exit, its tail and that exit).
fn unread_shape(body: &str, line_index: usize, family: &ProbeFamily) -> Option<Shape> {
    let parse = parse_clean_source_file(body)?;
    let function = parse
        .tree()
        .syntax()
        .descendants()
        .find_map(ast::Fn::cast)?;
    let block = function.body()?;
    let list = block.stmt_list()?;
    let line = line_range(body, line_index)?;
    let overlaps = |node: &SyntaxNode| {
        let range = node.text_range();
        usize::from(range.start()) < line.end && line.start < usize::from(range.end())
    };
    match family {
        ProbeFamily::ReturnValue => {
            if loop_accumulator_at(body, line_index).is_some() {
                return Some(Shape::AccumulatedLocal);
            }
            if let Some(shape) = let_else_exit(&list, &overlaps) {
                return Some(shape);
            }
            // Past a let-else exit the rest of the arm is the closure tail.
            let tail = list.tail_expr()?;
            // RIPR-SPEC-0265 reads the slice-parameter adaptor tail itself:
            // it credits a non-empty literal input and keeps the gap for an
            // empty one, so this rule leaves that shape to it.
            // A closure bound by `let` in the tail block may never be called.
            let on_closure_line = tail.syntax().descendants().any(|node| {
                ast::ClosureExpr::can_cast(node.kind())
                    && overlaps(&node)
                    && !node.ancestors().any(|up| ast::LetStmt::can_cast(up.kind()))
            });
            (on_closure_line
                && adaptor_tail_of(body).is_none()
                && !has_other_exit(block.syntax())
                && closure_tail_runs_unconditionally(tail.syntax()))
            .then_some(Shape::ClosureTail)
        }
        ProbeFamily::CallDeletion | ProbeFamily::SideEffect => {
            let tail = list.tail_expr()?;
            let returned = path_ident(&tail)?;
            let statements: Vec<ast::Stmt> = list.statements().collect();
            let at = statements.iter().position(|statement| {
                matches!(statement, ast::Stmt::ExprStmt(expr)
                    if expr.expr().is_some_and(|expr| overlaps(expr.syntax())))
            })?;
            // A later `let` or assignment gives the tail a different value.
            let rebound = statements[at + 1..].iter().any(|statement| {
                statement.syntax().descendants().any(|node| {
                    ast::IdentPat::cast(node.clone())
                        .is_some_and(|pat| pat.syntax().text() == returned.as_str())
                        || ast::BinExpr::cast(node).is_some_and(|bin| {
                            matches!(bin.op_kind(), Some(ast::BinaryOp::Assignment { .. }))
                                && bin.lhs().is_some_and(|lhs| {
                                    path_ident(&lhs).as_deref() == Some(returned.as_str())
                                })
                        })
                })
            });
            if rebound {
                return None;
            }
            let ast::Stmt::ExprStmt(statement) = &statements[at] else {
                return None;
            };
            let changed = statement.expr()?;
            let ast::Expr::MethodCallExpr(call) = changed else {
                return None;
            };
            (path_ident(&call.receiver()?).as_deref() == Some(returned.as_str())
                && !has_other_exit(block.syntax()))
            .then_some(Shape::ReturnedReceiver)
        }
        _ => None,
    }
}

/// `let ... else { .. return value; }` whose `else` block holds the changed
/// line, in an owner with no other early exit.
fn let_else_exit(list: &ast::StmtList, overlaps: &impl Fn(&SyntaxNode) -> bool) -> Option<Shape> {
    let lets: Vec<ast::LetStmt> = list
        .statements()
        .filter_map(|statement| match statement {
            ast::Stmt::LetStmt(let_stmt) if let_stmt.let_else().is_some() => Some(let_stmt),
            _ => None,
        })
        .collect();
    let else_blocks: Vec<ast::BlockExpr> = lets
        .iter()
        .filter_map(|let_stmt| let_stmt.let_else()?.block_expr())
        .collect();
    // The change is inside an `else` block (its initializer runs on every
    // input), and every other `return`, `?`, `break` or `continue` outside
    // closures is in an `else` block too.
    // Braces on the `let` line are not the else body: look at its statements.
    // The initializer must not share the changed line: a one-line
    // `let .. = init else { return .. }` cannot tell the two apart.
    let changed = lets.iter().any(|let_stmt| {
        let initializer_changed = let_stmt
            .initializer()
            .is_some_and(|init| overlaps(init.syntax()));
        let block = let_stmt
            .let_else()
            .and_then(|let_else| let_else.block_expr());
        !initializer_changed
            && block.is_some_and(|block| {
                block.stmt_list().is_some_and(|inner| {
                    inner
                        .statements()
                        .any(|statement| overlaps(statement.syntax()))
                        || inner
                            .tail_expr()
                            .is_some_and(|tail| overlaps(tail.syntax()))
                })
            })
    });
    let exits_elsewhere = exits_outside(list.syntax(), &else_blocks);
    (changed && !exits_elsewhere).then_some(Shape::LetElseExit)
}

/// An exit outside closures that is not inside one of `allowed`.
fn exits_outside(root: &SyntaxNode, allowed: &[ast::BlockExpr]) -> bool {
    let mut found = false;
    walk_outside_closures(root, &mut |node| {
        let exit = matches!(
            ast::Expr::cast(node.clone()),
            Some(
                ast::Expr::ReturnExpr(_)
                    | ast::Expr::TryExpr(_)
                    | ast::Expr::BreakExpr(_)
                    | ast::Expr::ContinueExpr(_)
            )
        );
        found |= exit
            && !allowed.iter().any(|block| {
                block
                    .syntax()
                    .text_range()
                    .contains_range(node.text_range())
            });
    });
    found
}

fn path_ident(expr: &ast::Expr) -> Option<String> {
    let ast::Expr::PathExpr(path) = expr else {
        return None;
    };
    let path = path.path()?;
    path.qualifier()
        .is_none()
        .then(|| {
            path.segment()?
                .name_ref()
                .map(|name| name.text().to_string())
        })
        .flatten()
}

fn line_range(body: &str, line_index: usize) -> Option<Range<usize>> {
    let mut start = 0;
    for (index, line) in body.split('\n').enumerate() {
        let end = start + line.len();
        if index == line_index {
            return Some(start..end);
        }
        start = end + 1;
    }
    None
}

/// A `return`, `?`, `break` or `continue` outside closures: an exit that can
/// leave before the tail.
fn has_other_exit(root: &SyntaxNode) -> bool {
    let mut found = false;
    walk_outside_closures(root, &mut |node| {
        found |= matches!(
            ast::Expr::cast(node.clone()),
            Some(
                ast::Expr::ReturnExpr(_)
                    | ast::Expr::TryExpr(_)
                    | ast::Expr::BreakExpr(_)
                    | ast::Expr::ContinueExpr(_)
            )
        );
    });
    found
}

/// The tail holds a closure and nothing outside closures branches or applies
/// an argument only on some inputs, so the call runs the closure whenever the
/// test evaluates the pinned value.
fn closure_tail_runs_unconditionally(tail: &SyntaxNode) -> bool {
    let has_closure = tail
        .descendants()
        .any(|node| ast::ClosureExpr::can_cast(node.kind()));
    if !has_closure {
        return false;
    }
    let mut conditional = false;
    walk_outside_closures(tail, &mut |node| {
        let Some(expr) = ast::Expr::cast(node.clone()) else {
            return;
        };
        conditional |= match expr {
            ast::Expr::IfExpr(_)
            | ast::Expr::MatchExpr(_)
            | ast::Expr::LoopExpr(_)
            | ast::Expr::WhileExpr(_)
            | ast::Expr::ForExpr(_) => true,
            ast::Expr::BinExpr(binary) => {
                matches!(binary.op_kind(), Some(ast::BinaryOp::LogicOp(_)))
            }
            ast::Expr::MethodCallExpr(call) => call.name_ref().is_some_and(|name| {
                let name = name.text().to_string();
                (CONDITIONAL_COMBINATORS.contains(&&*name) || EXTRA_CONDITIONAL.contains(&&*name))
                    && !(ITERATOR_ADAPTORS.contains(&&*name) && iterator_rooted(&call))
            }),
            _ => false,
        };
    });
    !conditional
}

/// Further methods that may not run their closure argument.
const EXTRA_CONDITIONAL: &[&str] = &[
    "inspect",
    "inspect_err",
    "is_err_and",
    "or_insert_with",
    "take_if",
    "unwrap_or_else",
    "map_or_default",
];

/// Adaptors whose closure runs once per element of an iterator, which is
/// also the name of an `Option` / `Result` combinator that may not run it.
const ITERATOR_ADAPTORS: &[&str] = &["map", "filter"];

/// Constructors whose result is an iterator, not an `Option` or `Result`.
const ITERATOR_SOURCES: &[&str] = &[
    "iter",
    "iter_mut",
    "into_iter",
    "chars",
    "bytes",
    "lines",
    "windows",
    "chunks",
];

/// Iterator-to-iterator adaptors a chain may pass through on its way from a
/// source to `map` / `filter`; any other method (`next`, `max`, `find`, ...)
/// ends the iterator and makes a later `map` / `filter` an `Option` combinator.
const ITERATOR_THROUGH: &[&str] = &[
    "rev",
    "skip",
    "take",
    "enumerate",
    "zip",
    "chain",
    "cloned",
    "copied",
    "take_while",
    "skip_while",
    "peekable",
    "step_by",
    "map",
    "filter",
];

/// Whether the receiver chain of `call` reaches an iterator source (one of
/// [`ITERATOR_SOURCES`], or a range) through iterator adaptors only.
fn iterator_rooted(call: &ast::MethodCallExpr) -> bool {
    let mut receiver = call.receiver();
    while let Some(expr) = receiver {
        match expr {
            ast::Expr::MethodCallExpr(inner) => {
                let Some(name) = inner.name_ref() else {
                    return false;
                };
                let name = name.text().to_string();
                if ITERATOR_SOURCES.contains(&&*name) {
                    return true;
                }
                if !ITERATOR_THROUGH.contains(&&*name) {
                    return false;
                }
                receiver = inner.receiver();
            }
            ast::Expr::ParenExpr(paren) => receiver = paren.expr(),
            ast::Expr::RangeExpr(_) => return true,
            _ => return false,
        }
    }
    false
}

/// Visit every node under `root`, not descending into closure bodies.
fn walk_outside_closures(root: &SyntaxNode, visit: &mut impl FnMut(&SyntaxNode)) {
    let mut events = root.preorder();
    while let Some(event) = events.next() {
        if let WalkEvent::Enter(node) = event {
            if ast::ClosureExpr::can_cast(node.kind()) {
                events.skip_subtree();
            } else {
                visit(&node);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_closure_bound_by_let_in_the_tail_block_is_not_a_closure_tail() {
        let body =
            "fn f(x: u32) -> u32 {\n    {\n        let _unused = || x + 1;\n        7\n    }\n}\n";
        assert!(unread_shape(body, 2, &ProbeFamily::ReturnValue).is_none());
        let called =
            "fn f(x: u32) -> u32 {\n    std::thread::spawn(move || x + 1).join().unwrap()\n}\n";
        assert!(matches!(
            unread_shape(called, 1, &ProbeFamily::ReturnValue),
            Some(Shape::ClosureTail)
        ));
    }

    fn test_fact(attrs: &[&str], body: &str, assertions: &[&str]) -> TestFact {
        TestFact {
            name: "pins".to_string(),
            file: std::path::PathBuf::from("tests/it.rs"),
            start_line: 1,
            end_line: 9,
            body: body.into(),
            calls: crate::analysis::facts::SharedFactSlice::empty(),
            assertions: assertions
                .iter()
                .map(|text| crate::analysis::facts::OracleFact {
                    line: 2,
                    text: (*text).to_string(),
                    kind: OracleKind::ExactValue,
                    strength: OracleStrength::Strong,
                    observed_tokens: Vec::new(),
                    ok_value_observed: None,
                })
                .collect(),
            literals: crate::analysis::facts::SharedFactSlice::empty(),
            attrs: attrs.iter().map(|attr| (*attr).to_string()).collect(),
            nested_fn_names: crate::analysis::facts::SharedFactSlice::empty(),
            let_bindings: crate::analysis::facts::SharedFactSlice::empty(),
            generated: None,
        }
    }

    #[test]
    fn whole_call_accepts_the_call_applied_unwrapped_or_collected_and_nothing_else() {
        for accepted in [
            "owner(1)",
            "  owner(1)  ",
            "owner(1)(2)",
            "owner(1).unwrap()",
            "owner(f(1, 2))",
            "owner(1).collect()",
            "owner(1).collect::<Vec<_>>()",
            "owner(1).collect::<Vec<Vec<u8>>>()",
        ] {
            assert!(is_whole_call(accepted, "owner"), "{accepted}");
        }
        for refused in [
            "owner",
            "owner_two(1)",
            "other(1)",
            "wrap(owner(1))",
            "owner(1).len()",
            "owner(1) + 1",
            "owner(1",
            "owner(1).collect::<Vec<_>(",
            "owner(1).collect",
        ] {
            assert!(!is_whole_call(refused, "owner"), "{refused}");
        }
    }

    #[test]
    fn degenerate_input_is_an_empty_none_or_bare_zero_argument() {
        for degenerate in [
            "f([])",
            "f(None)",
            "f(\"\")",
            "f(0)",
            "f(1, 0)",
            "f(Vec::new())",
            "f(String::new())",
            "f(&[])",
        ] {
            assert!(degenerate_input(degenerate), "{degenerate}");
        }
        for readable in [
            "f(10)",
            "f(1.0)",
            "f(0.5)",
            "f(a0)",
            "f(x_0)",
            "f(\"a b\")",
            "f(vec![1])",
            "f(Some(1))",
        ] {
            assert!(!degenerate_input(readable), "{readable}");
        }
    }

    #[test]
    fn zero_literals_are_integer_zeros_in_any_radix_or_suffix() {
        for zero in ["0", "-0", " 0 ", "00", "0_u32", "0i64", "0x0", "0b0", "0o0"] {
            assert!(is_zero_literal(zero), "{zero}");
        }
        for nonzero in ["1", "10", "-1", "0x1", "0x", "0b1", "0.5", "", "x0"] {
            assert!(!is_zero_literal(nonzero), "{nonzero}");
        }
    }

    #[test]
    fn contains_name_matches_whole_identifiers_only() {
        assert!(contains_name("owner(1)", "owner"));
        assert!(contains_name("1 + owner", "owner"));
        assert!(contains_name("x.owner()", "owner"));
        assert!(contains_name("owner_two + owner", "owner"));
        for text in ["owner_two(1)", "my_owner(1)", "owners", "ownér", "other"] {
            assert!(!contains_name(text, "owner"), "{text}");
        }
    }

    #[test]
    fn only_the_accumulator_shape_reads_a_zero_element_and_a_walked_empty_argument() {
        // A zero element is degenerate for the other shapes but not for the
        // accumulator, which walks every element.
        let call = "sum(&[0, 2])";
        assert!(degenerate_pin(Shape::ClosureTail, None, "sum", call, "2"));
        assert!(!degenerate_pin(
            Shape::AccumulatedLocal,
            Some(0),
            "sum",
            call,
            "2"
        ));
        // An empty literal in a position that is not walked does not count.
        let call = "sum(&[1, 2], Vec::new())";
        assert!(!degenerate_pin(
            Shape::AccumulatedLocal,
            Some(0),
            "sum",
            call,
            "3"
        ));
        assert!(degenerate_pin(
            Shape::AccumulatedLocal,
            Some(1),
            "sum",
            call,
            "3"
        ));
        // Without a walked position the whole call text is read.
        assert!(degenerate_pin(
            Shape::AccumulatedLocal,
            None,
            "sum",
            call,
            "3"
        ));
        // An empty walked run or a zero total adds nothing a pin could notice.
        assert!(degenerate_pin(
            Shape::AccumulatedLocal,
            Some(0),
            "sum",
            "sum(&[])",
            "3"
        ));
        assert!(degenerate_pin(
            Shape::AccumulatedLocal,
            Some(0),
            "sum",
            "sum(&[1, 2])",
            "0_u32"
        ));
    }

    #[test]
    fn a_pin_must_run_unconditionally_in_the_test_body() {
        let run = |body: &str, assertion: &str| assertion_runs_unconditionally(body, assertion);
        let pin = "assert_eq!(f(1), 2);";
        assert!(run(
            "fn t() {\n    let x = 1;\n    assert_eq!(f(1), 2);\n}",
            pin
        ));
        // Whitespace and the trailing semicolon do not matter.
        assert!(run("fn t() {\n    assert_eq!( f(1),2 )\n}", pin));
        for body in [
            "fn t() {\n    if flag {\n        assert_eq!(f(1), 2);\n    }\n}",
            "fn t() {\n    return;\n    assert_eq!(f(1), 2);\n}",
            "fn t() {\n    let c = || assert_eq!(f(1), 2);\n}",
            "fn t() {\n    assert_eq!(f(1), 3);\n}",
            "this is not rust",
        ] {
            assert!(!run(body, pin), "{body}");
        }
    }

    #[test]
    fn a_table_loop_pin_must_be_a_direct_statement_of_a_top_level_loop() {
        let pin = "assert_eq!(sum(&row), 3);";
        let run = |body: &str| assertion_is_top_level_loop_statement(body, pin);
        // The loop as a statement followed by more code.
        assert!(run(
            "fn t() {\n    for row in rows {\n        assert_eq!(sum(&row), 3);\n    }\n    let done = 1;\n}"
        ));
        assert!(run(
            "fn t() {\n    let rows = 1;\n    for row in rows {\n        assert_eq!(sum(&row), 3);\n    }\n}"
        ));
        // The loop as the test's tail expression.
        assert!(run(
            "fn t() {\n    for row in rows {\n        assert_eq!(sum(&row), 3);\n    }\n}"
        ));
        for body in [
            // Not in a loop at all.
            "fn t() {\n    assert_eq!(sum(&row), 3);\n}",
            // Nested in a branch of the loop body.
            "fn t() {\n    for row in rows {\n        if row.ok {\n            assert_eq!(sum(&row), 3);\n        }\n    }\n}",
            // A different assertion in the loop.
            "fn t() {\n    for row in rows {\n        assert_eq!(sum(&row), 4);\n    }\n}",
            // The test may leave before the table runs.
            "fn t() {\n    return;\n    for row in rows {\n        assert_eq!(sum(&row), 3);\n    }\n}",
            "fn t() {\n    let v = parse()?;\n    for row in rows {\n        assert_eq!(sum(&row), 3);\n    }\n}",
            "fn t() {\n    if bad { panic!(\"x\"); }\n    for row in rows {\n        assert_eq!(sum(&row), 3);\n    }\n}",
            "this is not rust",
        ] {
            assert!(!run(body), "{body}");
        }
    }

    #[test]
    fn an_exact_pin_credits_only_a_live_unconditional_whole_call_on_a_readable_input() {
        let pins = |test: &TestFact| pins_owner_exactly(test, "owner", Shape::ClosureTail, None);
        let body = "fn pins() {\n    assert_eq!(owner(3), 4);\n}";
        let pin = "assert_eq!(owner(3), 4);";
        assert!(pins(&test_fact(&["#[test]"], body, &[pin])));
        // The call may stand on either side.
        let flipped = "assert_eq!(4, owner(3));";
        assert!(pins(&test_fact(
            &["#[test]"],
            "fn pins() {\n    assert_eq!(4, owner(3));\n}",
            &[flipped]
        )));
        // Ignored and should_panic tests do not pin.
        assert!(!pins(&test_fact(&["#[test]", "#[ignore]"], body, &[pin])));
        assert!(!pins(&test_fact(
            &["#[test]", "#[should_panic]"],
            body,
            &[pin]
        )));
        // A degenerate input, a projection and a self-referential expectation
        // pin less than the whole result.
        for (text, source) in [
            (
                "assert_eq!(owner(0), 0);",
                "fn pins() {\n    assert_eq!(owner(0), 0);\n}",
            ),
            (
                "assert_eq!(owner(3).len(), 4);",
                "fn pins() {\n    assert_eq!(owner(3).len(), 4);\n}",
            ),
            (
                "assert_eq!(owner(3), owner(3));",
                "fn pins() {\n    assert_eq!(owner(3), owner(3));\n}",
            ),
            (
                "assert_ne!(owner(3), 4);",
                "fn pins() {\n    assert_ne!(owner(3), 4);\n}",
            ),
        ] {
            assert!(!pins(&test_fact(&["#[test]"], source, &[text])), "{text}");
        }
        // A pin under a branch is not unconditional.
        assert!(!pins(&test_fact(
            &["#[test]"],
            "fn pins() {\n    if flag {\n        assert_eq!(owner(3), 4);\n    }\n}",
            &[pin]
        )));
        // A weak assertion does not pin exactly.
        let mut weak = test_fact(&["#[test]"], body, &[pin]);
        weak.assertions[0].strength = OracleStrength::Weak;
        assert!(!pins(&weak));
    }

    #[test]
    fn a_result_pin_needs_an_ok_value_or_an_unwrapped_call() {
        let pins = |text: &str| {
            let body = format!("fn pins() {{\n    {text}\n}}");
            pins_result_ok(&test_fact(&["#[test]"], &body, &[text]), "owner")
        };
        assert!(pins("assert_eq!(owner(3), Ok(vec![4]));"));
        assert!(pins("assert_eq!(Ok(vec![4]), owner(3));"));
        assert!(pins("assert_eq!(owner(3).unwrap(), vec![4]);"));
        for text in [
            "assert_eq!(owner(3), Err(\"bad\"));",
            "assert_eq!(owner(3), vec![4]);",
            "assert_eq!(owner(0), Ok(vec![]));",
            "assert_eq!(owner(3), Ok(owner(3)));",
            "assert_ne!(owner(3), Ok(vec![4]));",
        ] {
            assert!(!pins(text), "{text}");
        }
        let body = "fn pins() {\n    assert_eq!(owner(3), Ok(vec![4]));\n}";
        let pin = "assert_eq!(owner(3), Ok(vec![4]));";
        assert!(!pins_result_ok(
            &test_fact(&["#[test]", "#[ignore]"], body, &[pin]),
            "owner"
        ));
        assert!(!pins_result_ok(
            &test_fact(&["#[test]", "#[should_panic]"], body, &[pin]),
            "owner"
        ));
    }

    #[test]
    fn a_let_else_exit_is_the_else_body_in_an_owner_with_no_other_exit() {
        let family = ProbeFamily::ReturnValue;
        let owner = "fn f(x: Option<u32>) -> u32 {\n    let Some(v) = x else {\n        return 7;\n    };\n    v\n}\n";
        assert!(matches!(
            unread_shape(owner, 2, &family),
            Some(Shape::LetElseExit)
        ));
        // The `let` line itself, or the tail, is not the else exit.
        assert!(unread_shape(owner, 4, &family).is_none());
        // An initializer on the changed line cannot be told from the exit.
        let one_line =
            "fn f(x: Option<u32>) -> u32 {\n    let Some(v) = x else { return 7 };\n    v\n}\n";
        assert!(unread_shape(one_line, 1, &family).is_none());
        // Another early exit means the input may leave elsewhere.
        let other_exit = "fn f(x: Option<u32>) -> u32 {\n    let Some(v) = x else {\n        return 7;\n    };\n    if v > 3 {\n        return 1;\n    }\n    v\n}\n";
        assert!(unread_shape(other_exit, 2, &family).is_none());
        // A `let` without `else` has no exit.
        let plain = "fn f(x: u32) -> u32 {\n    let v = x;\n    v\n}\n";
        assert!(unread_shape(plain, 1, &family).is_none());
    }

    #[test]
    fn a_returned_receiver_is_a_method_call_statement_on_the_value_the_owner_returns() {
        let owner = "fn f(mut v: Vec<u32>) -> Vec<u32> {\n    v.sort_by_key(|k| *k);\n    v\n}\n";
        for family in [ProbeFamily::CallDeletion, ProbeFamily::SideEffect] {
            assert!(matches!(
                unread_shape(owner, 1, &family),
                Some(Shape::ReturnedReceiver)
            ));
        }
        // Other probe families are not this shape.
        assert!(unread_shape(owner, 1, &ProbeFamily::Predicate).is_none());
        // The receiver is not the returned value.
        let other = "fn f(mut v: Vec<u32>, w: Vec<u32>) -> Vec<u32> {\n    w.len();\n    v\n}\n";
        assert!(unread_shape(other, 1, &ProbeFamily::CallDeletion).is_none());
        // The tail is not a plain returned name.
        let computed = "fn f(mut v: Vec<u32>) -> usize {\n    v.sort();\n    v.len()\n}\n";
        assert!(unread_shape(computed, 1, &ProbeFamily::CallDeletion).is_none());
        // A later rebinding or assignment gives the tail another value.
        let rebound = "fn f(mut v: Vec<u32>) -> Vec<u32> {\n    v.sort();\n    let v = Vec::new();\n    v\n}\n";
        assert!(unread_shape(rebound, 1, &ProbeFamily::CallDeletion).is_none());
        let assigned =
            "fn f(mut v: Vec<u32>) -> Vec<u32> {\n    v.sort();\n    v = Vec::new();\n    v\n}\n";
        assert!(unread_shape(assigned, 1, &ProbeFamily::CallDeletion).is_none());
        // Another exit can leave before the tail.
        let early = "fn f(mut v: Vec<u32>) -> Vec<u32> {\n    v.sort();\n    if v.is_empty() {\n        return v;\n    }\n    v\n}\n";
        assert!(unread_shape(early, 1, &ProbeFamily::CallDeletion).is_none());
    }

    #[test]
    fn a_closure_tail_needs_a_closure_the_call_runs_on_every_input() {
        let family = ProbeFamily::ReturnValue;
        let runs = |tail: &str| {
            let body = format!("fn f(x: u32, o: Option<u32>) -> u32 {{\n    {tail}\n}}\n");
            matches!(unread_shape(&body, 1, &family), Some(Shape::ClosureTail))
        };
        assert!(runs("std::thread::spawn(move || x + 1).join().unwrap()"));
        for tail in [
            // Option combinators that may not run the closure.
            "o.map(|v| v + 1).unwrap_or(0)",
            "o.unwrap_or_else(|| x + 1)",
            "o.and_then(|v| Some(v + 1)).unwrap_or(0)",
            // Methods only this rule treats as conditional.
            "o.inspect(|v| drop(*v)).unwrap()",
            "o.take_if(|v| *v > 1).unwrap()",
            "o.is_err_and(|v| v > 1) as u32",
            // A branch around the closure.
            "if x > 1 { (|| x + 1)() } else { 0 }",
            "match x { 0 => 0, _ => (|| x + 1)() }",
            "x > 1 && (|| x > 2)() as u32 == 1",
            // No closure on the line.
            "x + 1",
        ] {
            assert!(!runs(tail), "{tail}");
        }
        // An early exit can leave before the closure runs.
        let early = "fn f(x: u32) -> u32 {\n    if x == 0 {\n        return 0;\n    }\n    std::thread::spawn(move || x + 1).join().unwrap()\n}\n";
        assert!(unread_shape(early, 4, &family).is_none());
    }

    #[test]
    fn a_map_or_filter_closure_runs_only_when_the_chain_is_an_iterator() {
        let rooted = |source: &str| {
            let body = format!("fn f() {{ {source}; }}");
            parse_clean_source_file(&body)
                .and_then(|parse| {
                    parse
                        .tree()
                        .syntax()
                        .descendants()
                        .filter_map(ast::MethodCallExpr::cast)
                        .find(|call| call.name_ref().is_some_and(|name| name.text() == "map"))
                })
                .map(|call| iterator_rooted(&call))
        };
        for source in [
            "xs.iter().map(|x| x + 1)",
            "xs.iter().rev().skip(1).map(|x| x + 1)",
            "(0..n).map(|x| x + 1)",
            "xs.chars().filter(|c| c.is_ascii()).map(|c| c)",
            "(xs.iter()).map(|x| x + 1)",
        ] {
            assert_eq!(rooted(source), Some(true), "{source}");
        }
        for source in [
            "xs.iter().next().map(|x| x + 1)",
            "xs.get(0).map(|x| x + 1)",
            "opt.map(|x| x + 1)",
            "f().map(|x| x + 1)",
        ] {
            assert_eq!(rooted(source), Some(false), "{source}");
        }
    }
}
