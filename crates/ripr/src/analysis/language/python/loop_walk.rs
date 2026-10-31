//! Deliberately closed generator walk for rebound while parameters (#9733).
//!
//! Read only a sole `while name OP integer` with one plain yield and one
//! unconditional unit step. Other statements, nesting or rebinding retain the
//! existing unresolved limitation; this is not a general Python evaluator.
use super::{PythonOwner, guard_activation::leading_whitespace};

pub(super) struct LoopWalk {
    pub(super) local: String,
    pub(super) bound: String,
    decreasing: bool,
}

impl LoopWalk {
    pub(super) fn reaches(&self, start: i128) -> bool {
        let Ok(bound) = self.bound.parse::<i128>() else {
            return false;
        };
        if self.decreasing {
            start >= bound
        } else {
            start <= bound
        }
    }
}

pub(super) fn generator_walk(
    source: &str,
    owner: &PythonOwner,
    new_line: &str,
    old_line: Option<&str>,
    local: &str,
    bound: &str,
) -> Option<LoopWalk> {
    bound.parse::<i128>().ok()?;
    if !owner.parameters.iter().any(|p| p.name == local) {
        return None;
    }
    let lines: Vec<_> = source
        .lines()
        .skip(owner.start_line.checked_sub(1)?)
        .take(owner.end_line.checked_sub(owner.start_line)? + 1)
        .filter_map(|raw| {
            let code = raw.split('#').next()?.trim_end();
            (!code.trim().is_empty()).then_some(code)
        })
        .collect();
    let [definition, header, yielded, step] = lines.as_slice() else {
        return None;
    };
    if !definition.trim_start().starts_with("def ")
        || header.trim() != new_line.trim()
        || leading_whitespace(header) <= leading_whitespace(definition)
        || leading_whitespace(yielded) <= leading_whitespace(header)
        || leading_whitespace(step) != leading_whitespace(yielded)
        || yielded.trim() != format!("yield {local}")
    {
        return None;
    }
    for (new_op, old_op, decreasing) in [
        (">=", ">", true),
        (">", ">=", true),
        ("<=", "<", false),
        ("<", "<=", false),
    ] {
        if header.trim() == format!("while {local} {new_op} {bound}:")
            && old_line?.trim() == format!("while {local} {old_op} {bound}:")
            && step.trim() == format!("{local} {}= 1", if decreasing { "-" } else { "+" })
        {
            return Some(LoopWalk {
                local: local.to_string(),
                bound: bound.to_string(),
                decreasing,
            });
        }
    }
    None
}

/// The closed walk also needs an oracle that observes the whole yielded
/// sequence. Exhaustion alone is insufficient: adding zero preserves a sum,
/// and a first-item or sliced assertion never observes the changed suffix.
pub(super) fn sequence_oracle_observes_row(
    row: &super::boundary::CallRow,
    owner: &PythonOwner,
    candidates: &[super::related_tests::PythonRelatedCandidate<'_>],
) -> bool {
    use rustpython_parser::ast::{self, Expr, Mod, Stmt};

    // `row.text` is the evidence excerpt (truncated at CALL_ROW_TEXT_LIMIT),
    // not the original source: an over-long or multi-line assert never parses
    // as exactly one Assert here and the walk refuses. That under-credit is
    // fail-closed; do not widen it without re-pinning this boundary.
    let Ok(Mod::Module(module)) = super::source_facts::parse_module_result(
        std::path::Path::new("<generator-oracle>"),
        &row.text,
    ) else {
        return false;
    };
    let [Stmt::Assert(assertion)] = module.body.as_slice() else {
        return false;
    };
    // A failure message may contain another owner call that does not execute.
    if assertion.msg.is_some() {
        return false;
    }
    let Expr::Compare(compare) = assertion.test.as_ref() else {
        return false;
    };
    let ([ast::CmpOp::Eq], [right]) = (compare.ops.as_slice(), compare.comparators.as_slice())
    else {
        return false;
    };
    let Some((constructor, callee)) =
        sequence_operand(&compare.left, right).or_else(|| sequence_operand(right, &compare.left))
    else {
        return false;
    };
    candidates.iter().any(|candidate| {
        let test = candidate.test;
        let bindings = &test.constant_rebinding;
        candidate.relation.uses_oracle()
            && test.file == row.file
            && test.assertions.iter().any(|assertion| {
                assertion.line == row.line
                    && !assertion.helper_origin.is_helper()
                    && assertion.oracle_strength.rank()
                        >= crate::domain::OracleStrength::Strong.rank()
            })
            && !super::related_tests::builtin_is_shadowed(test, constructor)
            && !bindings.assigns_attribute(constructor)
            && !bindings.assigns_attribute(&owner.name)
            && !bindings.rebinds(callee.split('.').next().unwrap_or(&callee))
            && super::related_tests::owner_module_callees(test, owner).contains(&callee)
    })
}

fn sequence_operand<'a>(
    observed: &'a rustpython_parser::ast::Expr,
    expected: &rustpython_parser::ast::Expr,
) -> Option<(&'a str, String)> {
    use rustpython_parser::ast::Expr;

    let Expr::Call(materialize) = observed else {
        return None;
    };
    let Expr::Name(constructor) = materialize.func.as_ref() else {
        return None;
    };
    let elements = match (constructor.id.as_str(), expected) {
        ("list", Expr::List(list)) => &list.elts,
        ("tuple", Expr::Tuple(tuple)) => &tuple.elts,
        _ => return None,
    };
    let [Expr::Call(call)] = materialize.args.as_slice() else {
        return None;
    };
    if !materialize.keywords.is_empty()
        || !elements.iter().all(integer_literal)
        // Do not let a nested unused generator call borrow the outer oracle.
        || !call.args.iter().all(scalar_argument)
        || !call.keywords.iter().all(|keyword| keyword.arg.is_some() && scalar_argument(&keyword.value))
    {
        return None;
    }
    Some((constructor.id.as_str(), static_callee(&call.func)?))
}

fn integer_literal(expression: &rustpython_parser::ast::Expr) -> bool {
    use rustpython_parser::ast::{self, Expr};
    match expression {
        Expr::Constant(constant) => matches!(constant.value, ast::Constant::Int(_)),
        Expr::UnaryOp(unary) if matches!(unary.op, ast::UnaryOp::UAdd | ast::UnaryOp::USub) => {
            matches!(unary.operand.as_ref(), Expr::Constant(constant) if matches!(constant.value, ast::Constant::Int(_)))
        }
        _ => false,
    }
}

fn scalar_argument(expression: &rustpython_parser::ast::Expr) -> bool {
    use rustpython_parser::ast::Expr;
    matches!(expression, Expr::Constant(_) | Expr::Name(_))
        || integer_literal(expression)
        || matches!(expression, Expr::Attribute(_)) && static_callee(expression).is_some()
}

fn static_callee(expression: &rustpython_parser::ast::Expr) -> Option<String> {
    use rustpython_parser::ast::Expr;
    match expression {
        Expr::Name(name) => Some(name.id.to_string()),
        Expr::Attribute(attribute) => Some(format!(
            "{}.{}",
            static_callee(&attribute.value)?,
            attribute.attr
        )),
        _ => None,
    }
}
