//! `approx(expected, rel=, abs=)` band evaluation for a changed arithmetic
//! return (RIPR-SPEC-0238 rule 21, #9546).
//!
//! An explicit band may admit the old value, so a numeric edit under such an
//! assertion used to abstain. For one narrow shape the band is decidable from
//! the source alone: the owner is a one-statement function that returns
//! `+ - * /` arithmetic over numeric literals and its own parameters, and the
//! selected assertion compares one literal-argument call to that owner with
//! `pytest.approx(<number>, rel=<number>, abs=<number>)`. The old and new
//! return values are evaluated at the call's arguments and compared with the
//! band the way pytest does:
//!
//! * the old value inside the band and the new value outside it: the
//!   assertion fails after the edit, so it discriminates the change;
//! * both inside: the assertion passes either way, a weak gap;
//! * anything else (an old value already outside, a value too close to the
//!   band edge to trust under floating point, an unreadable shape) is
//!   unread and keeps the abstention.

use super::assertion_selection::{PythonAssertionFocus, select_relevant_assertion};
use super::related_tests::PythonRelatedCandidate;
use super::source_facts::parse_module_result;
use super::{PythonAssertion, PythonOwner, expr_full_name};
use crate::domain::{OracleStrength, OwnerKind, ValueContext, ValueFact};
use rustpython_parser::ast::{self, Expr, Mod, Stmt};
use std::collections::BTreeMap;
use std::path::Path;

/// The largest magnitude `f64` holds every integer up to exactly. Python
/// ints are exact, so a literal or intermediate result beyond this is not
/// evaluated.
const EXACT_INTEGER_LIMIT: f64 = 9_007_199_254_740_992.0;

/// Relative distance to the band edge below which the outcome is not trusted:
/// evaluation runs in `f64`, pytest's runs in Python floats and ints.
const EDGE_MARGIN: f64 = 1e-9;

/// What the selected strong assertions' explicit bands say about the edit.
#[derive(Clone, Debug, PartialEq)]
pub(super) enum PythonBandReading {
    /// No selected strong assertion passes an explicit band.
    NotBanded,
    /// A band is present but this bounded evaluation cannot read the edit.
    Unread,
    Read(PythonBandEvidence),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PythonBandVerdict {
    /// The new value leaves the band the old value was inside.
    Rejects,
    /// Old and new values both sit inside the band of every banded assertion.
    Absorbs,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct PythonBandEvidence {
    pub(super) verdict: PythonBandVerdict,
    pub(super) discriminator: String,
    pub(super) observed_values: Vec<ValueFact>,
    pub(super) reason: String,
}

/// Read the explicit `approx` bands of the strong assertions the focus
/// selects against the edit from `old_line` to `new_line`.
pub(super) fn python_band_reading(
    old_line: Option<&str>,
    new_line: &str,
    owner: &PythonOwner,
    related_candidates: &[PythonRelatedCandidate<'_>],
    focus: &PythonAssertionFocus,
) -> PythonBandReading {
    let mut banded = Vec::new();
    let mut unbanded_strong = false;
    for candidate in related_candidates
        .iter()
        .filter(|candidate| candidate.relation.uses_oracle())
    {
        let Some(assertion) =
            select_relevant_assertion(&candidate.test.assertions, Some(focus)).assertion()
        else {
            continue;
        };
        if assertion.oracle_strength.rank() < OracleStrength::Strong.rank() {
            continue;
        }
        if super::static_limits::approx_call_has_band(&assertion.gate_text) {
            banded.push(assertion);
        } else {
            unbanded_strong = true;
        }
    }
    if banded.is_empty() {
        return PythonBandReading::NotBanded;
    }
    let Some(old_line) = old_line else {
        return PythonBandReading::Unread;
    };
    let Some(edit) = ReturnEdit::read(old_line, new_line, owner) else {
        return PythonBandReading::Unread;
    };
    let mut outcomes = Vec::new();
    for assertion in banded {
        match edit.evaluate(assertion, owner) {
            Some(outcome) => outcomes.push(outcome),
            None => return PythonBandReading::Unread,
        }
    }
    let verdict = if outcomes.iter().any(|outcome| outcome.rejects) {
        PythonBandVerdict::Rejects
    } else if !unbanded_strong && outcomes.iter().all(|outcome| outcome.absorbs) {
        // Another strong assertion that is not banded could still catch the
        // edit, so only an all-banded set proves a gap.
        PythonBandVerdict::Absorbs
    } else {
        return PythonBandReading::Unread;
    };
    let matching: Vec<&Outcome> = outcomes
        .iter()
        .filter(|outcome| match verdict {
            PythonBandVerdict::Rejects => outcome.rejects,
            PythonBandVerdict::Absorbs => outcome.absorbs,
        })
        .collect();
    let observed_values = matching
        .iter()
        .map(|outcome| ValueFact {
            file: None,
            line: outcome.line,
            text: outcome.assertion_text.clone(),
            value: outcome.call.clone(),
            context: ValueContext::FunctionArgument,
        })
        .collect();
    let detail = matching
        .iter()
        .map(|outcome| outcome.detail.as_str())
        .collect::<Vec<_>>()
        .join("; ");
    let (discriminator, reason) = match verdict {
        PythonBandVerdict::Rejects => (
            String::new(),
            format!("the edit moves the result outside the asserted `approx` band ({detail})"),
        ),
        PythonBandVerdict::Absorbs => (
            "a result outside the `approx` band".to_string(),
            format!(
                "the asserted `approx` band admits both the old and the new result ({detail}), so the assertion passes either way"
            ),
        ),
    };
    PythonBandReading::Read(PythonBandEvidence {
        verdict,
        discriminator,
        observed_values,
        reason,
    })
}

struct Outcome {
    line: usize,
    assertion_text: String,
    call: String,
    rejects: bool,
    absorbs: bool,
    detail: String,
}

/// The old and new `return <expr>` of a one-statement function owner.
struct ReturnEdit {
    old: Expr,
    new: Expr,
}

impl ReturnEdit {
    fn read(old_line: &str, new_line: &str, owner: &PythonOwner) -> Option<Self> {
        // A decorator, a method receiver or a longer body changes what a call
        // returns or which line runs; the owner is the `def` plus one return.
        if owner.owner_kind != Some(OwnerKind::Function)
            || !owner.decorators.is_empty()
            || owner.end_line != owner.start_line + 1
        {
            return None;
        }
        let old = parse_expression(old_line.trim().strip_prefix("return ")?)?;
        let new = parse_expression(new_line.trim().strip_prefix("return ")?)?;
        Some(Self { old, new })
    }

    fn evaluate(&self, assertion: &PythonAssertion, owner: &PythonOwner) -> Option<Outcome> {
        let comparison =
            parse_expression(assertion.gate_text.trim_start().strip_prefix("assert")?)?;
        let Expr::Compare(compare) = &comparison else {
            return None;
        };
        if compare.ops.len() != 1 || !matches!(compare.ops[0], ast::CmpOp::Eq) {
            return None;
        }
        let right = compare.comparators.first()?;
        let (call, band) = match (compare.left.as_ref(), right) {
            (call @ Expr::Call(_), band @ Expr::Call(_)) => {
                if approx_name(call) {
                    (band, call)
                } else {
                    (call, band)
                }
            }
            _ => return None,
        };
        let band = Band::read(band)?;
        let Expr::Call(call_expr) = call else {
            return None;
        };
        if call_name(&call_expr.func)? != owner.name {
            return None;
        }
        let bindings = bind_arguments(owner, call_expr)?;
        let old = evaluate(&self.old, &bindings)?;
        let new = evaluate(&self.new, &bindings)?;
        let tolerance = band.tolerance();
        let old_distance = (old - band.expected).abs();
        let new_distance = (new - band.expected).abs();
        if near_edge(old_distance, tolerance) || near_edge(new_distance, tolerance) {
            return None;
        }
        // An old value outside the band means the assertion already failed
        // before the edit; whether it still does is not a statement about
        // this edit.
        if old_distance > tolerance {
            return None;
        }
        let call_text = format!(
            "{}({})",
            owner.name,
            owner
                .parameters
                .iter()
                .filter_map(|parameter| bindings
                    .get(&parameter.name)
                    .map(|value| format!("{}={value}", parameter.name)))
                .collect::<Vec<_>>()
                .join(", ")
        );
        Some(Outcome {
            line: assertion.line,
            assertion_text: assertion.text.clone(),
            call: call_text,
            rejects: new_distance > tolerance,
            absorbs: new_distance <= tolerance,
            detail: format!(
                "expected {} with tolerance {tolerance}; old result {old}, new result {new}",
                band.expected
            ),
        })
    }
}

struct Band {
    expected: f64,
    rel: Option<f64>,
    abs: Option<f64>,
}

impl Band {
    /// `approx(<number>, rel=<number>, abs=<number>)` with at most those two
    /// bands as keywords; a positional band, a spread or a non-numeric
    /// argument is not read.
    fn read(call: &Expr) -> Option<Self> {
        let Expr::Call(call) = call else {
            return None;
        };
        if !approx_name(&Expr::Call(call.clone())) || call.args.len() != 1 {
            return None;
        }
        let expected = evaluate(call.args.first()?, &BTreeMap::new())?;
        let (mut rel, mut abs) = (None, None);
        for keyword in &call.keywords {
            let name = keyword.arg.as_ref()?.as_str();
            match name {
                "rel" | "abs" => {
                    let value = match &keyword.value {
                        Expr::Constant(constant)
                            if matches!(constant.value, ast::Constant::None) =>
                        {
                            None
                        }
                        other => Some(evaluate(other, &BTreeMap::new())?),
                    };
                    if name == "rel" {
                        rel = value;
                    } else {
                        abs = value;
                    }
                }
                "nan_ok" => {}
                _ => return None,
            }
        }
        if rel.is_none() && abs.is_none() {
            return None;
        }
        if rel.is_some_and(|value| value < 0.0) || abs.is_some_and(|value| value < 0.0) {
            return None;
        }
        Some(Self { expected, rel, abs })
    }

    /// pytest's `ApproxScalar.tolerance`: `abs` alone is the tolerance, else
    /// the larger of `rel * |expected|` and `abs` (default `1e-12`).
    fn tolerance(&self) -> f64 {
        match (self.rel, self.abs) {
            (None, Some(abs)) => abs,
            (rel, abs) => {
                let relative = rel.unwrap_or(1e-6) * self.expected.abs();
                relative.max(abs.unwrap_or(1e-12))
            }
        }
    }
}

fn near_edge(distance: f64, tolerance: f64) -> bool {
    (distance - tolerance).abs() <= EDGE_MARGIN * tolerance.abs().max(1.0)
}

fn approx_name(expr: &Expr) -> bool {
    let Expr::Call(call) = expr else {
        return false;
    };
    expr_full_name(&call.func).is_some_and(|name| name == "approx" || name == "pytest.approx")
}

fn call_name(func: &Expr) -> Option<&str> {
    match func {
        Expr::Name(name) => Some(name.id.as_str()),
        _ => None,
    }
}

/// The literal value each owner parameter takes at the call: a positional or
/// keyword numeric literal, else a numeric literal default.
fn bind_arguments(owner: &PythonOwner, call: &ast::ExprCall) -> Option<BTreeMap<String, f64>> {
    let positional: Vec<&super::PythonParameter> = owner
        .parameters
        .iter()
        .filter(|parameter| !parameter.keyword_only)
        .collect();
    if call.args.len() > positional.len()
        || call
            .args
            .iter()
            .any(|argument| matches!(argument, Expr::Starred(_)))
    {
        return None;
    }
    let mut bindings = BTreeMap::new();
    for (parameter, argument) in positional.iter().zip(&call.args) {
        bindings.insert(
            parameter.name.clone(),
            evaluate(argument, &BTreeMap::new())?,
        );
    }
    for keyword in &call.keywords {
        let name = keyword.arg.as_ref()?.as_str();
        let parameter = owner
            .parameters
            .iter()
            .find(|parameter| parameter.name == name && !parameter.positional_only)?;
        if bindings
            .insert(
                parameter.name.clone(),
                evaluate(&keyword.value, &BTreeMap::new())?,
            )
            .is_some()
        {
            return None;
        }
    }
    for parameter in &owner.parameters {
        if !bindings.contains_key(&parameter.name) {
            let default = parameter.default.as_deref()?;
            bindings.insert(
                parameter.name.clone(),
                evaluate(&parse_expression(default)?, &BTreeMap::new())?,
            );
        }
    }
    Some(bindings)
}

/// Evaluate `+ - * /` arithmetic over numeric literals and bound names.
fn evaluate(expr: &Expr, names: &BTreeMap<String, f64>) -> Option<f64> {
    let value = match expr {
        Expr::Constant(constant) => match &constant.value {
            ast::Constant::Int(number) => {
                let value = number.to_string().parse::<f64>().ok()?;
                if value.abs() > EXACT_INTEGER_LIMIT {
                    return None;
                }
                value
            }
            ast::Constant::Float(number) => *number,
            _ => return None,
        },
        Expr::Name(name) => *names.get(name.id.as_str())?,
        Expr::UnaryOp(unary) => {
            let operand = evaluate(&unary.operand, names)?;
            match unary.op {
                ast::UnaryOp::USub => -operand,
                ast::UnaryOp::UAdd => operand,
                _ => return None,
            }
        }
        Expr::BinOp(binary) => {
            let left = evaluate(&binary.left, names)?;
            let right = evaluate(&binary.right, names)?;
            match binary.op {
                ast::Operator::Add => left + right,
                ast::Operator::Sub => left - right,
                ast::Operator::Mult => left * right,
                ast::Operator::Div if right != 0.0 => left / right,
                _ => return None,
            }
        }
        _ => return None,
    };
    (value.is_finite() && value.abs() <= EXACT_INTEGER_LIMIT).then_some(value)
}

/// The expression of `text`, read the way the surrounding source would, so
/// spacing and parentheses do not matter.
pub(super) fn parse_expression(text: &str) -> Option<Expr> {
    let source = format!("_ = (\n{text}\n)\n");
    let Ok(Mod::Module(module)) = parse_module_result(Path::new("t.py"), &source) else {
        return None;
    };
    match module.body.into_iter().next()? {
        Stmt::Assign(assign) => Some(*assign.value),
        _ => None,
    }
}
