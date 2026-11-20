//! Activation of an edit to a Python literal `case a | b` pattern (#15691).
use super::approx_band::parse_expression;
use super::assertion_selection::PythonAssertionFocus;
use super::boundary::{
    BoundaryActivation, PythonBoundaryEvidence, strong_owner_call_rows_with_gaps,
    unrebound_constants,
};
use super::module_constants::visit_statements;
use super::module_value::with_line_replaced;
use super::related_tests::{
    PythonRelatedCandidate, owner_class_locals, owner_method_bound_locals, owner_module_callees,
};
use super::source_facts::parse_module_result;
use super::source_utils::{SourceText, line_for_range_end, line_for_range_start, text_for_range};
use super::{PythonFramework, PythonOwner, expr_full_name};
use crate::domain::OracleStrength;
use rustpython_parser::ast::{self, Constant, Expr, Mod, Pattern, Ranged, Stmt};

// Integral floats have an exact integer value, even beyond the point where
// converting a neighboring Python integer to f64 would round it.
fn literal(expr: &Expr) -> Option<Constant> {
    let value = match expr {
        Expr::Constant(value) => value.value.clone(),
        Expr::UnaryOp(unary) if unary.op == ast::UnaryOp::USub => match literal(&unary.operand)? {
            Constant::Int(n) => Constant::Int(-n),
            Constant::Float(n) => Constant::Float(-n),
            Constant::Complex { real, imag } => Constant::Complex {
                real: -real,
                imag: -imag,
            },
            _ => return None,
        },
        // Python permits a signed real literal plus/minus an imaginary literal
        // in a value pattern. Its real part is converted to binary64.
        Expr::BinOp(binary) => {
            let real = match literal(&binary.left)? {
                Constant::Int(n) => n
                    .to_string()
                    .parse::<f64>()
                    .ok()
                    .filter(|n| n.is_finite())?,
                Constant::Float(n) => n,
                _ => return None,
            };
            let Expr::Constant(right) = binary.right.as_ref() else {
                return None;
            };
            let Constant::Complex { real: 0.0, imag } = right.value else {
                return None;
            };
            match binary.op {
                ast::Operator::Add => Constant::Complex { real, imag },
                ast::Operator::Sub => Constant::Complex { real, imag: -imag },
                _ => return None,
            }
        }
        _ => return None,
    };
    // A complex number with no imaginary component compares as its real part.
    let value = match value {
        Constant::Complex { real, imag: 0.0 } => Constant::Float(real),
        other => other,
    };
    match value {
        Constant::Float(n) if n.fract() == 0.0 => {
            Some(Constant::Int(format!("{n:.0}").parse().ok()?))
        }
        Constant::Str(_)
        | Constant::Bytes(_)
        | Constant::Int(_)
        | Constant::Float(_)
        | Constant::Complex { .. }
        | Constant::Bool(_)
        | Constant::None => Some(value),
        _ => None,
    }
}

struct Alternative {
    value: Constant,
    spelling: String,
}

impl Alternative {
    fn accepts(&self, input: &Constant) -> bool {
        // Singleton patterns are identity tests. In the other direction a
        // numeric pattern compares equal to a boolean input (case 1 takes True).
        match (&self.value, input) {
            (Constant::Int(n), Constant::Bool(b)) => *n == u8::from(*b).into(),
            (pattern, value) => pattern == value,
        }
    }
}

fn alternatives(pattern: &Pattern, text: &str) -> Option<Vec<Alternative>> {
    match pattern {
        Pattern::MatchOr(or) => {
            let mut result = Vec::new();
            for pattern in &or.patterns {
                result.extend(alternatives(pattern, text)?);
            }
            Some(result)
        }
        leaf => {
            let value = match leaf {
                Pattern::MatchValue(p) => literal(&p.value)?,
                Pattern::MatchSingleton(p) => p.value.clone(),
                _ => return None,
            };
            Some(vec![Alternative {
                value,
                spelling: text_for_range(text, leaf.range()),
            }])
        }
    }
}

struct Case {
    subject: String,
    arm: (usize, usize),
    entered_directly: bool,
    alternatives: Vec<Alternative>,
    // An earlier nonliteral pattern could already take the row. Even a
    // guarded literal can only be ruled out when its pattern misses.
    earlier: Vec<Option<Vec<Alternative>>>,
    guarded: bool,
}

enum CaseLocation {
    PatternLine(usize),
    Arm(usize, usize),
}

fn case_at(owner: &PythonOwner, source: &str, at: CaseLocation) -> Option<Case> {
    let Mod::Module(module) = parse_module_result(&owner.file, source).ok()? else {
        return None;
    };
    let text = SourceText::new(source);
    let mut entry = None;
    let mut found = None;
    visit_statements(&module.body, &mut |stmt| {
        if let Stmt::FunctionDef(function) = stmt
            && line_for_range_start(&text, function.range()) == owner.start_line
        {
            entry = function
                .body
                .iter()
                .find(|stmt| match stmt {
                    Stmt::Pass(_) => false,
                    Stmt::Expr(expr) if matches!(expr.value.as_ref(), Expr::Constant(_)) => false,
                    _ => true,
                })
                .map(Ranged::range);
        }
        let Stmt::Match(stmt) = stmt else { return };
        let Expr::Name(name) = stmt.subject.as_ref() else {
            return;
        };
        if !owner.parameters.iter().any(|p| p.name == name.id.as_str()) {
            return;
        }
        for (index, case) in stmt.cases.iter().enumerate() {
            let range = case.pattern.range();
            let header = line_for_range_start(&text, stmt.range());
            let selected = match at {
                CaseLocation::PatternLine(line) => (line_for_range_start(&text, range)
                    ..=line_for_range_end(&text, range))
                    .contains(&line),
                CaseLocation::Arm(h, i) => (header, index) == (h, i),
            };
            if selected && let Some(members) = alternatives(&case.pattern, source) {
                found = Some(Case {
                    arm: (header, index),
                    subject: name.id.to_string(),
                    entered_directly: entry == Some(stmt.range()),
                    alternatives: members,
                    earlier: stmt.cases[..index]
                        .iter()
                        .map(|prior| alternatives(&prior.pattern, source))
                        .collect(),
                    guarded: case.guard.is_some(),
                });
            }
        }
    });
    found
}

impl Case {
    fn selects(&self, input: &Constant) -> bool {
        self.alternatives.iter().any(|member| member.accepts(input))
    }

    fn reaches(&self, input: &Constant) -> bool {
        !self.guarded
            && self.earlier.iter().all(|prior| {
                prior
                    .as_ref()
                    .is_some_and(|members| !members.iter().any(|member| member.accepts(input)))
            })
    }
}

enum PinnedInput {
    Literal(Constant),
    RowBinding,
}

// Read an actual direct assertion on the owner. An unrelated strong assert
// must not donate its strength to a discarded call, including one in an
// assertion message. Simple expected operands rule out a second owner call.
fn pinned_input(
    owner: &PythonOwner,
    candidate: &PythonRelatedCandidate<'_>,
    line: usize,
    subject: &str,
    focus: &PythonAssertionFocus,
) -> Option<PinnedInput> {
    let test = candidate.test;
    let source = test.body_text.lines().nth(line.checked_sub(test.line)?)?;
    let assertion = test.assertions.iter().find(|a| {
        a.line == line
            && a.text.trim() == source.trim()
            && focus.admits(a)
            && a.oracle_strength.rank() >= OracleStrength::Strong.rank()
    })?;
    let Ok(Mod::Module(module)) = parse_module_result(&test.file, assertion.text.trim()) else {
        return None;
    };
    let (left, right) = match module.body.as_slice() {
        [Stmt::Assert(stmt)] if stmt.msg.is_none() => {
            let Expr::Compare(compare) = stmt.test.as_ref() else {
                return None;
            };
            let ([ast::CmpOp::Eq | ast::CmpOp::Is], [right]) =
                (compare.ops.as_slice(), compare.comparators.as_slice())
            else {
                return None;
            };
            (compare.left.as_ref(), right)
        }
        [Stmt::Expr(stmt)] if test.framework == PythonFramework::Unittest => {
            let Expr::Call(assertion) = stmt.value.as_ref() else {
                return None;
            };
            let Expr::Attribute(method) = assertion.func.as_ref() else {
                return None;
            };
            if method.attr.as_str() != "assertEqual" || !assertion.keywords.is_empty() {
                return None;
            }
            let [left, right] = assertion.args.as_slice() else {
                return None;
            };
            (left, right)
        }
        _ => return None,
    };
    let call = match (left, right) {
        (Expr::Call(call), Expr::Name(_) | Expr::Constant(_))
        | (Expr::Name(_) | Expr::Constant(_), Expr::Call(call)) => call,
        _ => return None,
    };
    if !super::range_activation::valid_owner_call(owner, call) {
        return None;
    }
    let callee = expr_full_name(&call.func)?;
    let is_owner = owner_module_callees(test, owner).contains(&callee)
        || owner
            .qualified_name
            .rsplit_once('.')
            .is_some_and(|(class, method)| {
                owner_class_locals(test, owner, class)
                    .into_iter()
                    .chain(owner_method_bound_locals(test, owner, class, method))
                    .any(|receiver| callee == format!("{receiver}.{method}"))
            });
    if !is_owner {
        return None;
    }
    let positional = owner
        .parameters
        .iter()
        .skip(super::range_activation::receiver_skip(owner)?)
        .filter(|p| !p.keyword_only)
        .position(|p| p.name == subject);
    let argument = positional.and_then(|i| call.args.get(i)).or_else(|| {
        call.keywords
            .iter()
            .find(|kw| kw.arg.as_ref().is_some_and(|name| name.as_str() == subject))
            .map(|kw| &kw.value)
    });
    // The shared row reader binds names, parametrization and defaults. A
    // literal written directly in the observed call is also native evidence,
    // including byte strings and non-decimal numerals outside that reader.
    Some(
        argument
            .and_then(literal)
            .map_or(PinnedInput::RowBinding, PinnedInput::Literal),
    )
}

pub(super) fn missing(
    owner: &PythonOwner,
    source: &str,
    line: usize,
    counterpart: &str,
    isolated: bool,
    candidates: &[PythonRelatedCandidate<'_>],
    focus: &PythonAssertionFocus,
) -> Option<PythonBoundaryEvidence> {
    let new = case_at(owner, source, CaseLocation::PatternLine(line))?;
    // A blank counterpart removes one physical alternative line and keeps all
    // other byte-line coordinates intact. Locate the same arm even when its
    // shortened pattern now ends before the changed line.
    let old_source = with_line_replaced(source, line, counterpart)?;
    let old = case_at(owner, &old_source, CaseLocation::Arm(new.arm.0, new.arm.1))?;
    if old.subject != new.subject || (old.alternatives.len() < 2 && new.alternatives.len() < 2) {
        return None;
    }
    let changed =
        |a: &Alternative, other: &Case| !other.alternatives.iter().any(|b| a.value == b.value);
    let mut altered = old
        .alternatives
        .iter()
        .filter(|a| changed(a, &new))
        .chain(new.alternatives.iter().filter(|a| changed(a, &old)))
        .peekable();
    let any_change = altered.peek().is_some();
    if !any_change && old.alternatives.len() == new.alternatives.len() {
        // The arm's suite or guard can change on the same line while the
        // literal alternatives stay the same. Its existing gate owns that edit.
        return None;
    }
    let mut inputs = Vec::new();
    // Before this first statement a bare parameter still has its call value
    // and no enclosing guard or early exit can have diverted the call.
    if isolated && new.entered_directly && old.entered_directly {
        let constants = unrebound_constants(owner, candidates);
        for candidate in candidates {
            let (rows, _) = strong_owner_call_rows_with_gaps(
                owner,
                &constants,
                std::slice::from_ref(candidate),
                focus,
            );
            let mut observed_lines = std::collections::BTreeMap::new();
            for row in rows {
                let pin = observed_lines.entry(row.line).or_insert_with(|| {
                    pinned_input(owner, candidate, row.line, &new.subject, focus)
                });
                let value = match pin {
                    Some(PinnedInput::Literal(value)) => Some(value.clone()),
                    Some(PinnedInput::RowBinding) => row
                        .bindings
                        .get(&new.subject)
                        .and_then(|binding| parse_expression(&binding.value))
                        .and_then(|expr| literal(&expr)),
                    None => None,
                };
                if let Some(value) = value
                    && new.reaches(&value)
                    && old.reaches(&value)
                    && new.selects(&value) != old.selects(&value)
                {
                    inputs.push(value);
                }
            }
        }
    }
    let missing = altered.find(|a| !inputs.iter().any(|input| a.accepts(input)));
    if any_change && missing.is_none() {
        return None;
    }
    let discriminator = missing
        .filter(|a| {
            old.reaches(&a.value)
                && new.reaches(&a.value)
                && old.selects(&a.value) != new.selects(&a.value)
        })
        .map(|a| {
            let op = if matches!(a.value, Constant::Bool(_) | Constant::None) {
                "is"
            } else {
                "=="
            };
            format!("{} {op} {}", new.subject, a.spelling)
        });
    Some(PythonBoundaryEvidence {
        activation: BoundaryActivation::Missing,
        reason: format!(
            "No strong related test row establishes that this edit changes the arm selected by `match {}`. Check earlier arms and pin the result for a distinguishing input.",
            new.subject
        ),
        discriminator,
        observed_values: Vec::new(),
    })
}
