use super::source_utils::normalized_path;
use super::static_limits::PythonStaticLimit;
use super::{
    PythonOwner, is_python_control_predicate_line, python_assignment_constructor_field_parts,
    python_exit_code_discriminator, python_return_constructor_field_discriminator,
    python_return_dict_field_discriminator,
};
use crate::domain::{
    Confidence, DeltaKind, FindingCanonicalGap, FlowSinkFact, FlowSinkKind, OwnerKind, ProbeFamily,
    StageEvidence, StageState,
};
use std::path::Path;

pub(super) fn classify_probe_shape(line_text: &str) -> (ProbeFamily, DeltaKind) {
    classify_probe_shape_gated(line_text, true)
}

/// The shape of a changed line given the line it replaced. A comprehension
/// filter is the changed behavior only when the filter itself changed: an edit
/// to the yielded element or the surrounding call, under an unchanged filter,
/// keeps the return-value shape, so its exact oracle is not asked for an input
/// at a bound the edit never moved.
pub(super) fn classify_probe_shape_with_old(
    line_text: &str,
    old_line_text: Option<&str>,
) -> (ProbeFamily, DeltaKind) {
    let filter_changed = match (
        super::boundary::python_comprehension_filter(line_text),
        old_line_text.and_then(super::boundary::python_comprehension_filter),
    ) {
        (Some(new), Some(old)) => new.condition != old.condition || new.iterable != old.iterable,
        _ => true,
    };
    classify_probe_shape_gated(line_text, filter_changed)
}

fn classify_probe_shape_gated(
    line_text: &str,
    comprehension_filter: bool,
) -> (ProbeFamily, DeltaKind) {
    let trimmed = line_text.trim_start();
    if is_python_cli_exit_line(trimmed) {
        return (ProbeFamily::SideEffect, DeltaKind::Effect);
    }
    if (trimmed.contains(" if ") && trimmed.contains(" else "))
        || trimmed.starts_with("if ")
        || trimmed.starts_with("elif ")
        || trimmed.starts_with("while ")
        || trimmed.starts_with("for ")
        || trimmed.starts_with("match ")
        || trimmed.starts_with("case ")
    {
        return (ProbeFamily::Predicate, DeltaKind::Control);
    }
    if trimmed.starts_with("raise ")
        || trimmed == "raise"
        || trimmed.starts_with("try:")
        || trimmed.starts_with("except ")
        || trimmed.starts_with("except* ")
        || trimmed.starts_with("finally:")
        || (trimmed.starts_with("with ") && trimmed.contains("raises("))
    {
        return (ProbeFamily::ErrorPath, DeltaKind::Control);
    }
    if python_return_dict_field_discriminator(trimmed).is_some()
        || python_return_constructor_field_discriminator(trimmed).is_some()
    {
        return (ProbeFamily::FieldConstruction, DeltaKind::Value);
    }
    if super::boundary::python_return_comparison(trimmed).is_some()
        || (comprehension_filter && super::boundary::python_comprehension_filter(trimmed).is_some())
    {
        return (ProbeFamily::Predicate, DeltaKind::Control);
    }
    if trimmed.starts_with("return ") || trimmed == "return" {
        return (ProbeFamily::ReturnValue, DeltaKind::Value);
    }
    if contains_mock_initializer(trimmed) {
        return (ProbeFamily::SideEffect, DeltaKind::Effect);
    }
    if let Some(eq_idx) = trimmed.find('=')
        && !trimmed.contains("==")
        && !trimmed.contains("!=")
        && !trimmed.contains(">=")
        && !trimmed.contains("<=")
    {
        let lhs = trimmed[..eq_idx].trim();
        if lhs.contains('.')
            && lhs.chars().all(|ch| {
                ch.is_ascii_alphanumeric() || ch == '_' || ch == '.' || ch == '[' || ch == ']'
            })
        {
            return (ProbeFamily::FieldConstruction, DeltaKind::Value);
        }
        if python_assignment_constructor_field_parts(trimmed).is_some() {
            return (ProbeFamily::FieldConstruction, DeltaKind::Value);
        }
        let rhs = trimmed[eq_idx + 1..].trim();
        if looks_like_call_expression(rhs) {
            return (ProbeFamily::SideEffect, DeltaKind::Effect);
        }
    }
    let call_candidate = trimmed.strip_prefix("await ").unwrap_or(trimmed).trim_end();
    if looks_like_call_expression(call_candidate)
        && !call_candidate.starts_with("assert ")
        && !call_candidate.starts_with("def ")
        && !call_candidate.starts_with("class ")
        && !call_candidate.starts_with("with ")
    {
        return (ProbeFamily::SideEffect, DeltaKind::Effect);
    }
    (ProbeFamily::Predicate, DeltaKind::Control)
}

/// A single-name assignment whose right side is a plain value expression
/// (#13587): `text = f"{text} {label}"`, `total = base - bonus`. It computes a
/// value and neither tests a condition nor calls anything, so it is value
/// shaped, whether or not an unchanged guard encloses it. The recognizer is
/// closed: the target is one identifier, and any right side that could carry a
/// comparison, boolean operator, membership test, conditional or lambda keeps
/// the conservative predicate default. A guard line itself (`if ...:`) never
/// reaches this check.
pub(super) fn is_python_plain_local_value_assignment(line_text: &str) -> bool {
    let trimmed = line_text.trim();
    let Some((lhs, rhs)) = trimmed.split_once('=') else {
        return false;
    };
    // Assignment style puts a space on both sides of `=`; a bare `ident=value`
    // is a keyword-argument or parameter continuation (`timeout=30`).
    if !(lhs.ends_with(' ') && rhs.starts_with(' ')) {
        return false;
    }
    let rhs = rhs.trim();
    // `total: int = ...`: a clean annotation only. `else: x = 3`,
    // `try: x = 3`, `except E: x = 3` and similar one-line compound
    // statements fail the keyword and identifier checks.
    let lhs = match lhs.split_once(':') {
        Some((target, annotation)) => {
            let annotation = annotation.trim();
            let clean = !annotation.is_empty()
                && annotation.chars().all(|ch| {
                    ch.is_ascii_alphanumeric()
                        || matches!(ch, '_' | '.' | '[' | ']' | ',' | ' ' | '|')
                })
                && python_brackets_balanced(annotation)
                && !annotation.contains("lambda");
            if !clean {
                return false;
            }
            target.trim()
        }
        None => lhs.trim(),
    };
    if !is_python_identifier(lhs)
        || PYTHON_NON_TARGET_KEYWORDS.contains(&lhs)
        || rhs.is_empty()
        || rhs.starts_with('=')
        || rhs.ends_with('\\')
        // A trailing comma is a call-argument, parameter or dict continuation
        // (`timeout=30,`), and `;` joins a second statement: neither is one
        // plain assignment.
        || rhs.ends_with(',')
        || rhs.contains(';')
        // A further `=` is a chained write (`x = obj.status = 1`), a walrus or
        // a keyword argument inside the right side: not one plain local value.
        || rhs.contains('=')
        || !python_brackets_balanced(rhs)
    {
        return false;
    }
    // Only the right side's own keywords are checked as word-bounded text.
    let rhs_padded = format!(" {rhs} ");
    ![
        "<", ">", "==", "!=", " if ", " else ", " and ", " or ", " not ", " in ", " is ", " and(",
        " or(", " not(", "lambda", " yield ", " yield)", " await ",
    ]
    .iter()
    .any(|marker| rhs_padded.contains(marker))
}

/// The shape a finding presents (#13587). Analysis keeps the conservative
/// `(Predicate, Control)` default for a plain local assignment, because its
/// gates, oracle admission and credit are calibrated on it. Only what the
/// finding says about itself (probe family, delta, gap id) reads as a value,
/// and only for an indented plain local value assignment inside a callable
/// owner, so no verdict or credit moves.
pub(super) fn presented_python_shape(
    owner: &PythonOwner,
    family: &ProbeFamily,
    delta: &DeltaKind,
    line_text: &str,
) -> (ProbeFamily, DeltaKind) {
    if matches!(family, ProbeFamily::Predicate)
        && matches!(delta, DeltaKind::Control)
        && matches!(
            owner.owner_kind,
            Some(OwnerKind::Function | OwnerKind::Method | OwnerKind::ClassMethod)
        )
        && line_text.starts_with([' ', '\t'])
        && is_python_plain_local_value_assignment(line_text)
    {
        (ProbeFamily::ReturnValue, DeltaKind::Value)
    } else {
        (family.clone(), delta.clone())
    }
}

const PYTHON_NON_TARGET_KEYWORDS: &[&str] = &[
    "False", "None", "True", "and", "as", "assert", "async", "await", "break", "case", "class",
    "continue", "def", "del", "elif", "else", "except", "finally", "for", "from", "global", "if",
    "import", "in", "is", "lambda", "match", "nonlocal", "not", "or", "pass", "print", "raise",
    "return", "try", "while", "with", "yield",
];

fn is_python_identifier(text: &str) -> bool {
    let mut chars = text.chars();
    chars
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic() || first == '_')
        && chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

fn python_brackets_balanced(text: &str) -> bool {
    let mut depth = 0i32;
    for ch in text.chars() {
        match ch {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth -= 1,
            _ => {}
        }
        if depth < 0 {
            return false;
        }
    }
    depth == 0
}

pub(super) fn contains_mock_initializer(text: &str) -> bool {
    text.contains("Mock(") || text.contains("MagicMock(")
}

fn is_python_cli_exit_line(text: &str) -> bool {
    python_exit_code_discriminator(text).is_some()
}

pub(super) fn looks_like_call_expression(text: &str) -> bool {
    let text = text.trim_end_matches(';').trim_end();
    text.contains('(') && text.ends_with(')')
}

pub(super) fn canonical_python_gap_for(
    file: &Path,
    owner: &PythonOwner,
    probe_family: &ProbeFamily,
    line_text: &str,
) -> FindingCanonicalGap {
    let file = normalized_path(file);
    let behavior_kind = python_behavior_kind(probe_family).to_string();
    let probe_kind = probe_family.as_str().to_string();
    let normalized_discriminator = normalize_python_gap_discriminator(probe_family, line_text);
    let id = format!(
        "gap:python:{file}:{}:{behavior_kind}:{probe_kind}:{normalized_discriminator}",
        owner.qualified_name
    );

    FindingCanonicalGap {
        id,
        language: "python".to_string(),
        file,
        owner: owner.qualified_name.clone(),
        behavior_kind,
        probe_kind,
        normalized_discriminator,
    }
}

fn python_behavior_kind(probe_family: &ProbeFamily) -> &'static str {
    match probe_family {
        ProbeFamily::Predicate => "predicate_boundary",
        ProbeFamily::ReturnValue => "return_value",
        ProbeFamily::ErrorPath => "exception_path",
        ProbeFamily::FieldConstruction => "field_value",
        ProbeFamily::SideEffect | ProbeFamily::CallDeletion => "call_or_output_effect",
        ProbeFamily::MatchArm => "match_arm",
        ProbeFamily::StaticUnknown => "static_unknown",
    }
}

fn normalize_python_gap_discriminator(probe_family: &ProbeFamily, line_text: &str) -> String {
    let mut text = line_text.trim().trim_end_matches(';').trim().to_string();
    match probe_family {
        ProbeFamily::Predicate => {
            for prefix in [
                "if ", "elif ", "while ", "for ", "match ", "case ", "return ",
            ] {
                if let Some(stripped) = text.strip_prefix(prefix) {
                    text = stripped.to_string();
                    break;
                }
            }
            text = text.trim_end_matches(':').trim().to_string();
        }
        ProbeFamily::ReturnValue => {
            if let Some(stripped) = text.strip_prefix("return ") {
                text = stripped.to_string();
            }
        }
        ProbeFamily::ErrorPath => {
            if let Some(stripped) = text.strip_prefix("raise ") {
                text = stripped.to_string();
            }
            text = text.trim_end_matches(':').trim().to_string();
        }
        _ => {}
    }
    normalize_gap_key_text(&text)
}

fn normalize_gap_key_text(text: &str) -> String {
    let mut normalized = String::new();
    let mut previous_was_word = false;
    let mut pending_separator = false;

    for character in text.chars() {
        if character.is_ascii_alphanumeric() || character == '_' || character == '.' {
            if pending_separator && previous_was_word {
                normalized.push('_');
            }
            normalized.push(character.to_ascii_lowercase());
            previous_was_word = true;
            pending_separator = false;
        } else if matches!(
            character,
            '=' | '!' | '<' | '>' | '+' | '-' | '*' | '/' | '%' | '[' | ']'
        ) {
            normalized.push(character);
            previous_was_word = false;
            pending_separator = false;
        } else {
            pending_separator = true;
        }
    }

    let trimmed = normalized.trim_matches('_').to_string();
    if trimmed.is_empty() {
        "unknown".to_string()
    } else {
        trimmed
    }
}

pub(super) fn python_infection_evidence(
    probe_family: &ProbeFamily,
    line_text: &str,
) -> StageEvidence {
    let summary = match probe_family {
        ProbeFamily::Predicate => {
            if is_python_control_predicate_line(line_text) {
                format!(
                    "Changed Python predicate can alter branch selection: `{}`",
                    line_text.trim()
                )
            } else {
                format!(
                    "Changed Python expression can alter preview-classified predicate behavior: `{}`",
                    line_text.trim()
                )
            }
        }
        ProbeFamily::ReturnValue => {
            format!(
                "Changed Python return expression can alter the owner return value: `{}`",
                line_text.trim()
            )
        }
        ProbeFamily::ErrorPath => {
            format!(
                "Changed Python error path can alter raised exception/control behavior: `{}`",
                line_text.trim()
            )
        }
        ProbeFamily::FieldConstruction => {
            format!(
                "Changed Python field or attribute construction can alter object state: `{}`",
                line_text.trim()
            )
        }
        ProbeFamily::SideEffect | ProbeFamily::CallDeletion => {
            format!(
                "Changed Python call or output effect can alter observable side effects: `{}`",
                line_text.trim()
            )
        }
        ProbeFamily::MatchArm => {
            format!(
                "Changed Python match arm can alter selected branch behavior: `{}`",
                line_text.trim()
            )
        }
        ProbeFamily::StaticUnknown => {
            "Python preview could not classify the changed behavior shape.".to_string()
        }
    };
    StageEvidence::new(StageState::Yes, Confidence::Low, summary)
}

pub(super) fn python_propagation_evidence(
    probe_family: &ProbeFamily,
    line_text: &str,
    static_limit: Option<&PythonStaticLimit>,
) -> StageEvidence {
    if let Some(limit) = static_limit {
        return StageEvidence::new(
            StageState::Unknown,
            Confidence::Low,
            format!(
                "Static limit `{}` prevents a safe Python propagation claim.",
                limit.kind.as_str()
            ),
        );
    }

    match probe_family {
        ProbeFamily::ReturnValue => StageEvidence::new(
            StageState::Yes,
            Confidence::Low,
            "Changed Python return value is already at the owner output boundary.",
        ),
        ProbeFamily::ErrorPath => StageEvidence::new(
            StageState::Yes,
            Confidence::Low,
            "Changed Python error path propagates through the exception/control boundary.",
        ),
        ProbeFamily::FieldConstruction => StageEvidence::new(
            StageState::Weak,
            Confidence::Low,
            "Changed Python field construction can propagate through returned or retained object state; exact runtime object flow is not resolved.",
        ),
        ProbeFamily::SideEffect | ProbeFamily::CallDeletion => StageEvidence::new(
            StageState::Weak,
            Confidence::Low,
            "Changed Python call/output behavior can propagate through side effects; runtime target resolution is not inferred.",
        ),
        ProbeFamily::Predicate | ProbeFamily::MatchArm => {
            let summary = if matches!(probe_family, ProbeFamily::Predicate)
                && !is_python_control_predicate_line(line_text)
            {
                "Changed Python fallback expression can propagate through selected behavior; preview evidence does not prove the concrete downstream sink."
            } else if matches!(probe_family, ProbeFamily::Predicate) {
                "Changed Python control flow can propagate by selecting a different branch; preview evidence does not prove the concrete downstream sink."
            } else {
                "Changed Python match arm can propagate by selecting a different branch; preview evidence does not prove the concrete downstream sink."
            };
            StageEvidence::new(StageState::Weak, Confidence::Low, summary)
        }
        ProbeFamily::StaticUnknown => StageEvidence::new(
            StageState::Unknown,
            Confidence::Low,
            "Python preview could not classify a propagation path for this changed behavior.",
        ),
    }
}

pub(super) fn python_flow_sink_for(
    probe_family: &ProbeFamily,
    owner: &PythonOwner,
    line: usize,
    line_text: &str,
) -> Option<FlowSinkFact> {
    let kind = match probe_family {
        ProbeFamily::ReturnValue => FlowSinkKind::ReturnValue,
        ProbeFamily::ErrorPath => FlowSinkKind::ErrorVariant,
        ProbeFamily::FieldConstruction => FlowSinkKind::StructField,
        ProbeFamily::SideEffect | ProbeFamily::CallDeletion => FlowSinkKind::CallEffect,
        ProbeFamily::Predicate | ProbeFamily::MatchArm => FlowSinkKind::Unknown,
        ProbeFamily::StaticUnknown => return None,
    };

    Some(FlowSinkFact {
        kind,
        text: line_text.trim().to_string(),
        line,
        owner: Some(owner.symbol_id()),
    })
}
