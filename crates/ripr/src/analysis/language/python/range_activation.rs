//! A changed range header needs a strong input on which its returned sums differ.
//! This is a withholding gate, not a general loop evaluator or a repair producer.
use super::PythonOwner;
use super::arm_activation::builtin_names_intact;
use super::assertion_selection::{PythonAssertionFocus, select_relevant_assertion};
use super::boundary::{
    CallRow, resolve_operand, strong_owner_call_rows_with_gaps, unrebound_constants,
};
use super::guard_activation::name_rebound_in_owner;
use super::module_constants::mentions_identifier;
use super::related_tests::PythonRelatedCandidate;
use super::static_limits::PythonStaticLimit;
use crate::domain::{OracleStrength, StaticLimitKind};

pub(super) fn changed_range_limit(
    new_line: &str,
    old_line: Option<&str>,
    owner: &PythonOwner,
    source: &str,
    candidates: &[PythonRelatedCandidate<'_>],
    focus: &PythonAssertionFocus,
) -> Option<PythonStaticLimit> {
    let new = range_header(new_line);
    let old = old_line.and_then(range_header);
    if new.is_none() && old.is_none() {
        return None;
    }
    let constants = unrebound_constants(owner, candidates);
    let (rows, _) = strong_owner_call_rows_with_gaps(owner, &constants, candidates, focus);
    let witnessed = !source.is_empty()
        && builtin_names_intact(owner, source, &["range"])
        && sum_accumulator(owner, source, new_line)
        && new.as_ref().zip(old.as_ref()).is_some_and(|(new, old)| {
            new.target == old.target
                && new.readable
                && old.readable
                && owner.parameters.iter().all(|parameter| {
                    !new.args
                        .iter()
                        .chain(&old.args)
                        .any(|arg| mentions_identifier(arg, &parameter.name))
                        || !name_rebound_in_owner(source, owner, &parameter.name)
                })
                && rows.iter().any(|row| {
                    if !row_is_asserted(row, candidates, focus) {
                        return false;
                    }
                    let read = |header: &RangeHeader| {
                        let values: Option<Vec<i128>> = header
                            .args
                            .iter()
                            .map(|arg| resolve_operand(arg, row, &constants)?.parse().ok())
                            .collect();
                        range_sum(&values?)
                    };
                    matches!((read(new), read(old)), (Some(new), Some(old)) if new != old)
                })
        });
    (!witnessed).then(|| PythonStaticLimit {
        kind: StaticLimitKind::UnsupportedSyntax,
        evidence: "static_limit unsupported_syntax: changed range has no established differing accumulator result".to_string(),
        missing: "Static limit `unsupported_syntax`: no strong related Python call establishes different old and new returned range sums. Both ranges may be empty or sum to the same value for the pinned input; other loop bodies, unreadable bounds, rebound names and unavailable source remain unresolved. Check an input that changes the iterations with a runtime test.".to_string(),
    })
}

/// The shared row reader inventories every call in a strong test. A range
/// witness must instead be the call observed by its selected assertion.
/// Parse the complete, bounded row excerpt: partial/multiline rows refuse.
fn row_is_asserted(
    row: &CallRow,
    candidates: &[PythonRelatedCandidate<'_>],
    focus: &PythonAssertionFocus,
) -> bool {
    use rustpython_parser::ast::{self, Expr, Mod, Stmt};
    let selected = candidates.iter().find(|candidate| {
        candidate.test.file == row.file
            && select_relevant_assertion(&candidate.test.assertions, Some(focus))
                .assertion()
                .is_some_and(|assertion| {
                    assertion.line == row.line
                        && assertion.oracle_strength.rank() >= OracleStrength::Strong.rank()
                })
    });
    let Some(candidate) = selected else {
        return false;
    };
    let Ok(Mod::Module(module)) = super::source_facts::parse_module_result(
        std::path::Path::new("<range-assertion>"),
        &row.text,
    ) else {
        return false;
    };
    let pair = match module.body.as_slice() {
        [Stmt::Assert(assertion)] if assertion.msg.is_none() => {
            let Expr::Compare(compare) = assertion.test.as_ref() else {
                return false;
            };
            let ([ast::CmpOp::Eq], [right]) =
                (compare.ops.as_slice(), compare.comparators.as_slice())
            else {
                return false;
            };
            (compare.left.as_ref(), right)
        }
        [Stmt::Expr(statement)] if candidate.test.framework == super::PythonFramework::Unittest => {
            let Expr::Call(assertion) = statement.value.as_ref() else {
                return false;
            };
            if !assertion.keywords.is_empty()
                || !super::expr_full_name(&assertion.func).is_some_and(|name| {
                    matches!(
                        name.rsplit('.').next(),
                        Some("assertEqual" | "assertEquals")
                    )
                })
            {
                return false;
            }
            let [left, right] = assertion.args.as_slice() else {
                return false;
            };
            (left, right)
        }
        _ => return false,
    };
    direct_integer_pin(pair.0, pair.1) || direct_integer_pin(pair.1, pair.0)
}

fn direct_integer_pin(
    value: &rustpython_parser::ast::Expr,
    expected: &rustpython_parser::ast::Expr,
) -> bool {
    use rustpython_parser::ast::{self, Expr};
    let expected = match expected {
        Expr::UnaryOp(unary) if matches!(unary.op, ast::UnaryOp::UAdd | ast::UnaryOp::USub) => {
            unary.operand.as_ref()
        }
        other => other,
    };
    if !matches!(expected, Expr::Constant(c) if matches!(c.value, ast::Constant::Int(_))) {
        return false;
    }
    let Expr::Call(call) = value else {
        return false;
    };
    super::expr_full_name(&call.func).is_some()
        && call.args.iter().all(plain_argument)
        && call
            .keywords
            .iter()
            .all(|keyword| keyword.arg.is_some() && plain_argument(&keyword.value))
}

fn plain_argument(value: &rustpython_parser::ast::Expr) -> bool {
    use rustpython_parser::ast::{self, Expr};
    match value {
        Expr::Constant(_) | Expr::Name(_) => true,
        Expr::Attribute(_) => super::expr_full_name(value).is_some(),
        Expr::UnaryOp(unary) if matches!(unary.op, ast::UnaryOp::UAdd | ast::UnaryOp::USub) => {
            plain_argument(&unary.operand)
        }
        _ => false,
    }
}

struct RangeHeader {
    target: String,
    args: Vec<String>,
    readable: bool,
}

fn range_header(line: &str) -> Option<RangeHeader> {
    use rustpython_parser::ast::{Expr, Mod, Ranged, Stmt};
    let source = format!("{}\n    pass\n", line.trim());
    let Mod::Module(module) =
        super::source_facts::parse_module_result(std::path::Path::new("<range-header>"), &source)
            .or_else(|_| {
                super::source_facts::parse_module_result(
                    std::path::Path::new("<inline-range-header>"),
                    line.trim(),
                )
            })
            .ok()?
    else {
        return None;
    };
    let [Stmt::For(header)] = module.body.as_slice() else {
        return None;
    };
    let Expr::Call(call) = header.iter.as_ref() else {
        return None;
    };
    if !matches!(call.func.as_ref(), Expr::Name(name) if name.id.as_str() == "range") {
        return None;
    }
    Some(RangeHeader {
        target: super::source_utils::text_for_range(&source, header.target.range()).to_string(),
        args: call
            .args
            .iter()
            .map(|arg| super::source_utils::text_for_range(&source, arg.range()).to_string())
            .collect(),
        readable: call.keywords.is_empty()
            && (1..=3).contains(&call.args.len())
            && matches!(header.body.as_slice(), [Stmt::Pass(_)]),
    })
}

/// Canonical sequence identity without materializing or walking the range.
/// Empty sequences agree, and the step is irrelevant for a singleton.
fn range_signature(args: &[i128]) -> Option<(i128, i128, i128)> {
    let (start, stop, step) = match *args {
        [stop] => (0, stop, 1),
        [start, stop] => (start, stop, 1),
        [start, stop, step] if step != 0 => (start, stop, step),
        _ => return None,
    };
    let (distance, stride) = if step > 0 {
        (stop.checked_sub(start)?, step)
    } else {
        (start.checked_sub(stop)?, step.checked_neg()?)
    };
    if distance <= 0 {
        return Some((0, 0, 0));
    }
    let count = distance
        .checked_sub(1)?
        .checked_div(stride)?
        .checked_add(1)?;
    Some((count, start, if count == 1 { 0 } else { step }))
}

/// Only the complete zero-initialized integer sum shape returns the edited
/// iterations without another branch, early exit, transformation or effect.
fn sum_accumulator(owner: &PythonOwner, source: &str, changed_line: &str) -> bool {
    use rustpython_parser::ast::{self, Expr, Mod, Stmt};
    let lines: Vec<_> = source
        .lines()
        .skip(owner.start_line.saturating_sub(1))
        .take(owner.end_line.saturating_sub(owner.start_line) + 1)
        .collect();
    let Some(first) = lines.first() else {
        return false;
    };
    let indent = first.len() - first.trim_start().len();
    let source = lines
        .iter()
        .map(|line| line.get(indent..).unwrap_or(line))
        .collect::<Vec<_>>()
        .join("\n");
    let Ok(Mod::Module(module)) =
        super::source_facts::parse_module_result(std::path::Path::new("<range-sum>"), &source)
    else {
        return false;
    };
    let [Stmt::FunctionDef(function)] = module.body.as_slice() else {
        return false;
    };
    let [
        Stmt::Assign(initial),
        Stmt::For(walk),
        Stmt::Return(returned),
    ] = function.body.as_slice()
    else {
        return false;
    };
    let [Expr::Name(acc)] = initial.targets.as_slice() else {
        return false;
    };
    let Expr::Constant(zero) = initial.value.as_ref() else {
        return false;
    };
    if !matches!(&zero.value, ast::Constant::Int(value) if value.to_string() == "0")
        || !walk.orelse.is_empty()
    {
        return false;
    }
    let Expr::Name(item) = walk.target.as_ref() else {
        return false;
    };
    let [Stmt::AugAssign(update)] = walk.body.as_slice() else {
        return false;
    };
    let (Expr::Name(target), Expr::Name(value), Some(Expr::Name(result))) = (
        update.target.as_ref(),
        update.value.as_ref(),
        returned.value.as_deref(),
    ) else {
        return false;
    };
    use rustpython_parser::ast::Ranged;
    let start = usize::from(walk.range().start());
    let header = source.get(start..).and_then(|rest| rest.lines().next());
    acc.id != item.id
        && target.id == acc.id
        && value.id == item.id
        && result.id == acc.id
        && matches!(update.op, ast::Operator::Add)
        && header.is_some_and(|header| header.trim() == changed_line.trim())
}

fn range_sum(args: &[i128]) -> Option<i128> {
    let (count, start, step) = range_signature(args)?;
    if count == 0 {
        return Some(0);
    }
    let last = start.checked_add(count.checked_sub(1)?.checked_mul(step)?)?;
    let pair = start.checked_add(last)?;
    if count % 2 == 0 {
        (count / 2).checked_mul(pair)
    } else {
        count.checked_mul(pair / 2)
    }
}
