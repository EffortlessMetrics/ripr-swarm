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
use ra_ap_syntax::{AstNode, SyntaxNode, WalkEvent, ast};
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
    if finding.static_limit_kind.is_some()
        || finding.class != ExposureClass::WeaklyExposed
        || !finding
            .ripr
            .reveal
            .discriminate
            .summary
            .contains("(observation_unverified)")
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
    // The accumulator's walked argument, so an empty literal elsewhere in the
    // call does not make the pin degenerate.
    let walked = loop_accumulator_at(&owner.body, changed_line - owner.start_line)
        .map(|accumulator| accumulator.walked_position);
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
            pins_owner_exactly(test, &owner.name, shape, walked).then(|| {
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
    let next_step = format!(
        "{} pins this code's whole result with an exact `assert_eq!`, but {}, so ripr does not report a gap. Check that the pinned input runs the changed code and that its expected value differs between the old and the new behavior; add a test only if either fails.",
        labels.join(", "),
        shape.advice()
    );
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
        assertion_runs_unconditionally(&test.body, &assertion.text)
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
}
