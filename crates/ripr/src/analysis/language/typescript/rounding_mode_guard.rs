//! RIPR-SPEC-0248 rule 2: a rounding-function swap needs a tested input
//! that the two functions treat differently.
//!
//! `Math.round(((f - 32) * 5) / 9)` becoming `Math.floor(((f - 32) * 5) / 9)`
//! changes the result only when the pre-rounding value is fractional. An
//! exact assertion on `fromFahrenheit(212)` passes on both versions when the
//! operand folds to the whole number 100, however exact the matcher is. So an
//! `exposed` ladder result whose change swaps one of `Math.round`,
//! `Math.floor`, `Math.ceil` and `Math.trunc` for another keeps its credit
//! unless some observed owner call folds the operand to a value where the two
//! functions differ, or some call cannot be folded at all.

use super::*;

const ROUNDING_FUNCTIONS: [&str; 4] = ["round", "floor", "ceil", "trunc"];

/// One rounding-function swap on a changed line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RoundingSwap {
    pub(crate) old_function: &'static str,
    pub(crate) new_function: &'static str,
    /// The single argument of the new line's rounding call, as written.
    pub(crate) operand: String,
}

/// What the observed owner calls show about a [`RoundingSwap`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RoundingWitness {
    /// Some observed call folds to a value the two functions treat
    /// differently, or some call or relation cannot be folded.
    Discriminates,
    /// Every observed call folds to a whole number.
    AllWhole,
    /// Every observed call folds to a fractional value both functions round
    /// to the same integer.
    SameResult,
}

/// Every `Math.<fn>(` occurrence of `line` with `fn` a rounding function:
/// the offset just after `Math.` and the function.
fn rounding_calls(line: &str) -> Vec<(usize, &'static str)> {
    let mut found = Vec::new();
    for (index, _) in line.match_indices("Math.") {
        if line[..index]
            .chars()
            .next_back()
            .is_some_and(|c| c.is_alphanumeric() || matches!(c, '_' | '$' | '.'))
        {
            continue;
        }
        let rest = &line[index + "Math.".len()..];
        if let Some(function) = ROUNDING_FUNCTIONS.into_iter().find(|function| {
            rest.strip_prefix(function)
                .is_some_and(|tail| tail.trim_start().starts_with('('))
        }) {
            found.push((index + "Math.".len(), function));
        }
    }
    found
}

/// `Some` when the two lines differ only in one `Math.<fn>` name, both
/// rounding functions.
pub(crate) fn ts_rounding_swap(old_line: &str, new_line: &str) -> Option<RoundingSwap> {
    let (old_line, new_line) = (old_line.trim(), new_line.trim());
    let old_calls = rounding_calls(old_line);
    let new_calls = rounding_calls(new_line);
    if old_calls.len() != new_calls.len() || old_calls.is_empty() {
        return None;
    }
    let normalize = |line: &str, calls: &[(usize, &'static str)]| {
        let mut text = line.to_string();
        for (offset, function) in calls.iter().rev() {
            text.replace_range(*offset..*offset + function.len(), "@");
        }
        text
    };
    if normalize(old_line, &old_calls) != normalize(new_line, &new_calls) {
        return None;
    }
    let mut differing = old_calls
        .iter()
        .zip(&new_calls)
        .filter(|((_, old), (_, new))| old != new);
    let ((_, old_function), (offset, new_function)) = differing.next()?;
    if differing.next().is_some() {
        return None;
    }
    let after = new_line[offset + new_function.len()..].trim_start();
    let mut operands = balanced_call_arguments(after.strip_prefix('(')?)?;
    if operands.len() != 1 {
        return None;
    }
    Some(RoundingSwap {
        old_function,
        new_function,
        operand: operands.remove(0).trim().to_string(),
    })
}

/// JavaScript `Math.round`: halves round toward positive infinity.
fn js_round(value: f64) -> f64 {
    let floor = value.floor();
    if value - floor >= 0.5 {
        floor + 1.0
    } else {
        floor
    }
}

fn apply_rounding(function: &str, value: f64) -> Option<f64> {
    Some(match function {
        "round" => js_round(value),
        "floor" => value.floor(),
        "ceil" => value.ceil(),
        "trunc" => value.trunc(),
        _ => return None,
    })
}

/// A call argument that is a plain, optionally negated numeric literal.
fn signed_literal_argument(argument: &str) -> Option<f64> {
    let argument = argument.trim();
    let (negative, digits) = match argument.strip_prefix('-') {
        Some(rest) => (true, rest.trim_start()),
        None => (false, argument),
    };
    let value = numeric_literal_value(digits)?.parse::<f64>().ok()?;
    Some(if negative { -value } else { value })
}

/// Fold the swap's operand for the arguments of every owner call in every
/// related test. Any piece that cannot be read, from an unmapped relation to
/// a non-literal argument, reads `Discriminates`, which keeps the ladder
/// verdict.
pub(crate) fn ts_rounding_witness(
    swap: &RoundingSwap,
    owner: &TypeScriptOwner,
    candidates: &[TypeScriptRelatedCandidate<'_>],
    all_tests: &[TypeScriptTest],
    alias_map: Option<&TsAliasMap>,
    workspace_root: Option<&Path>,
) -> RoundingWitness {
    witness_or_keep(
        swap,
        owner,
        candidates,
        all_tests,
        alias_map,
        workspace_root,
    )
    .unwrap_or(RoundingWitness::Discriminates)
}

fn witness_or_keep(
    swap: &RoundingSwap,
    owner: &TypeScriptOwner,
    candidates: &[TypeScriptRelatedCandidate<'_>],
    all_tests: &[TypeScriptTest],
    alias_map: Option<&TsAliasMap>,
    workspace_root: Option<&Path>,
) -> Option<RoundingWitness> {
    let mut all_whole = true;
    for bindings in observed_numeric_calls(owner, candidates, all_tests, alias_map, workspace_root)?
    {
        let value = fold_numeric_expression(&swap.operand, &bindings)?;
        if !value.is_finite() {
            return None;
        }
        let old = apply_rounding(swap.old_function, value)?;
        let new = apply_rounding(swap.new_function, value)?;
        if (old - new).abs() >= 0.5 {
            return Some(RoundingWitness::Discriminates);
        }
        all_whole &= value.fract() == 0.0;
    }
    Some(if all_whole {
        RoundingWitness::AllWhole
    } else {
        RoundingWitness::SameResult
    })
}

/// The numeric-literal arguments of every owner call in every related test,
/// one binding list per call (a parameter whose argument is not a plain
/// signed numeric literal has no binding). `None` when any piece cannot be
/// read: an unmapped relation, an unstable parameter, a test file that feeds
/// the owner an input this reader never sees, or no call at all. Callers keep
/// the ladder verdict on `None`.
pub(crate) fn observed_numeric_calls(
    owner: &TypeScriptOwner,
    candidates: &[TypeScriptRelatedCandidate<'_>],
    all_tests: &[TypeScriptTest],
    alias_map: Option<&TsAliasMap>,
    workspace_root: Option<&Path>,
) -> Option<Vec<Vec<(String, f64)>>> {
    let root = workspace_root?;
    let source = owner.source_text.as_deref()?;
    if candidates.is_empty()
        || !matches!(
            owner.owner_kind,
            OwnerKind::Function
                | OwnerKind::ArrowFunction
                | OwnerKind::ModuleFunction
                | OwnerKind::ClassMethod
        )
        || owner.params.is_empty()
        || owner
            .params
            .iter()
            .any(|name| !ts_parameter_is_stable(source, name))
    {
        return None;
    }
    let mut observed = Vec::new();
    for candidate in candidates {
        if !candidate.relation.uses_oracle() {
            continue;
        }
        let test = candidate.test;
        if local_identifier_declared_in_test_body(&test.body_text, &owner.name) {
            return None;
        }
        // The names the test calls the owner by, and the receivers a member
        // call through them must start from.
        let (names, receivers): (Vec<String>, Vec<String>) =
            if owner.owner_kind == OwnerKind::ClassMethod {
                let classes =
                    class_names_for_class_method_owner(test, owner, alias_map, workspace_root);
                if classes.is_empty() {
                    return None;
                }
                (vec![owner.name.clone()], classes)
            } else {
                let receivers =
                    unshadowed_owner_namespace_receivers(test, owner, alias_map, workspace_root);
                let names = ts_owner_call_names(candidate, owner, alias_map, workspace_root)
                    .into_iter()
                    .map(str::to_string)
                    .collect();
                (names, receivers)
            };
        let file_names: Vec<&str> = names
            .iter()
            .map(String::as_str)
            .chain(receivers.iter().map(String::as_str))
            .collect();
        // No hook, file-level helper or other test-file code may feed the
        // owner an input this reader never sees.
        if !test_file_names_owner_only_in_test_bodies(test, all_tests, &file_names, root) {
            return None;
        }
        let masked_body = ts_mask_literals_and_comments(&test.body_text);
        for name in &names {
            if !every_owner_reference_is_a_call(&masked_body, name, &receivers) {
                return None;
            }
            for arguments in owner_call_arguments(&test.body_text, name, &receivers) {
                let arguments = substitute_constant_arguments(
                    arguments,
                    test,
                    owner,
                    alias_map,
                    workspace_root,
                );
                let mut bindings = Vec::new();
                for (index, param) in owner.params.iter().enumerate() {
                    if let Some(value) = arguments
                        .get(index)
                        .and_then(|a| signed_literal_argument(a))
                    {
                        bindings.push((param.clone(), value));
                    }
                }
                observed.push(bindings);
            }
        }
    }
    (!observed.is_empty()).then_some(observed)
}
