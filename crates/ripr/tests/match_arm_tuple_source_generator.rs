//! Temporary source generator for the test-first tuple-arm repair.
//!
//! This test applies exact, single-occurrence replacements to the reviewed
//! classifier source and emits the complete candidate file through the normal
//! CI report artifact. The generated file is inspected and committed as source;
//! this generator is then removed before qualification.

use std::fs;
use std::path::Path;

fn replace_once(source: &mut String, needle: &str, replacement: &str) -> Result<(), String> {
    let occurrences = source.matches(needle).count();
    if occurrences != 1 {
        return Err(format!(
            "guarded replacement expected one occurrence, found {occurrences}: {needle}"
        ));
    }
    *source = source.replacen(needle, replacement, 1);
    Ok(())
}

#[test]
fn emit_guarded_tuple_arm_classifier_candidate() -> Result<(), String> {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let workspace = manifest
        .parent()
        .and_then(Path::parent)
        .ok_or_else(|| format!("cannot derive workspace root from {}", manifest.display()))?;
    let source_path = manifest.join("src/analysis/classify/reveal.rs");
    let mut source = fs::read_to_string(&source_path)
        .map_err(|error| format!("read {} failed: {error}", source_path.display()))?;

    replace_once(
        &mut source,
        r#"    let match_arm_literals = if matches!(probe.family, ProbeFamily::MatchArm) {
        match_arm_pattern_literals(analysis_expression)
    } else {
        Vec::new()
    };
"#,
        r#"    let match_arm_literals = if matches!(probe.family, ProbeFamily::MatchArm) {
        match_arm_pattern_literals(analysis_expression)
    } else {
        Vec::new()
    };
    // For MatchArm: retain the exact parser-owned two-boolean tuple pattern
    // when both elements are direct boolean literals. Unsupported tuple
    // shapes remain absent and therefore cannot confirm observation.
    let match_arm_bool_tuple = if matches!(probe.family, ProbeFamily::MatchArm) {
        match_arm_pattern_bool_tuple(analysis_expression)
    } else {
        None
    };
"#,
    )?;

    replace_once(
        &mut source,
        r#"        match_arm_variants: &match_arm_variants,
        match_arm_literals: &match_arm_literals,
        match_arm_guarded,
"#,
        r#"        match_arm_variants: &match_arm_variants,
        match_arm_literals: &match_arm_literals,
        match_arm_bool_tuple,
        match_arm_guarded,
"#,
    )?;

    replace_once(
        &mut source,
        r#"    match_arm_variants: &'a [String],
    match_arm_literals: &'a [String],
    /// True when the parser-owned arm pattern carries a guard. This slice
"#,
        r#"    match_arm_variants: &'a [String],
    match_arm_literals: &'a [String],
    /// Exact direct `(bool, bool)` pattern identity for the bounded tuple
    /// slice. Other tuple and nested pattern shapes remain unsupported.
    match_arm_bool_tuple: Option<[bool; 2]>,
    /// True when the parser-owned arm pattern carries a guard. This slice
"#,
    )?;

    replace_once(
        &mut source,
        r#"fn match_arm_pattern_literals(expression: &str) -> Vec<String> {
    match find_fat_arrow(expression) {
        Some(separator) => match_arm_string_values(&expression[..separator]),
        None => Vec::new(),
    }
}
"#,
        r#"fn match_arm_pattern_literals(expression: &str) -> Vec<String> {
    match find_fat_arrow(expression) {
        Some(separator) => match_arm_string_values(&expression[..separator]),
        None => Vec::new(),
    }
}

fn direct_bool_literal(text: &str) -> Option<bool> {
    match text.trim() {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}

/// Exact two-element boolean tuple on the pattern side of one match arm.
///
/// Parenthesized/nested values, bindings, alternation, constants, wildcards,
/// trailing elements, and malformed syntax fail closed. Guard satisfaction is
/// checked independently by `match_arm_pattern_has_guard`.
fn match_arm_pattern_bool_tuple(expression: &str) -> Option<[bool; 2]> {
    let separator = find_fat_arrow(expression)?;
    let pattern = expression[..separator].trim();
    let inner = pattern.strip_prefix('(')?.strip_suffix(')')?;
    let elements = split_top_level_arguments(inner)?;
    if elements.len() != 2 {
        return None;
    }
    Some([
        direct_bool_literal(elements[0])?,
        direct_bool_literal(elements[1])?,
    ])
}
"#,
    )?;

    replace_once(
        &mut source,
        r#"fn owner_call_literals(text: &str, owner: &str) -> Vec<String> {
    let Some(operands) = assertion_comparison_operands(text) else {
        return Vec::new();
    };
    let mut literals = operands
        .into_iter()
        .filter_map(|operand| direct_owner_string_input(operand, owner))
        .collect::<Vec<_>>();
    literals.sort();
    literals.dedup();
    literals
}
"#,
        r#"fn owner_call_literals(text: &str, owner: &str) -> Vec<String> {
    let Some(operands) = assertion_comparison_operands(text) else {
        return Vec::new();
    };
    let mut literals = operands
        .into_iter()
        .filter_map(|operand| direct_owner_string_input(operand, owner))
        .collect::<Vec<_>>();
    literals.sort();
    literals.dedup();
    literals
}

fn direct_owner_bool_tuple_input(expression: &str, owner: &str) -> Option<[bool; 2]> {
    let expression = expression.trim();
    let rest = expression.strip_prefix(owner)?;
    if rest
        .chars()
        .next()
        .is_some_and(|ch| ch.is_ascii_alphanumeric() || ch == '_')
    {
        return None;
    }

    let rest = rest.trim_start();
    if !rest.starts_with('(') {
        return None;
    }
    let opening = expression.len() - rest.len();
    let closing = matching_parenthesis(expression, opening)?;
    if !expression[closing + 1..].trim().is_empty() {
        return None;
    }

    let arguments = split_top_level_arguments(&expression[opening + 1..closing])?;
    if arguments.len() != 2 {
        return None;
    }
    Some([
        direct_bool_literal(arguments[0])?,
        direct_bool_literal(arguments[1])?,
    ])
}

fn owner_call_bool_tuples(text: &str, owner: &str) -> Vec<[bool; 2]> {
    let Some(operands) = assertion_comparison_operands(text) else {
        return Vec::new();
    };
    let mut tuples = operands
        .into_iter()
        .filter_map(|operand| direct_owner_bool_tuple_input(operand, owner))
        .collect::<Vec<_>>();
    tuples.sort();
    tuples.dedup();
    tuples
}
"#,
    )?;

    replace_once(
        &mut source,
        r#"        match_arm_variants,
        match_arm_literals,
        match_arm_guarded,
"#,
        r#"        match_arm_variants,
        match_arm_literals,
        match_arm_bool_tuple,
        match_arm_guarded,
"#,
    )?;

    replace_once(
        &mut source,
        r#"    let has_token_match = if matches!(family, ProbeFamily::MatchArm) {
        !match_arm_guarded
            && (match_arm_variants
                .iter()
                .any(|v| contains_as_whole_word(&assertion.text, v))
                || !match_arm_literals.is_empty()
                    && !import_defeats_owner
                    && !cross_package_defeats_owner
                    && owner_callee.is_some_and(|owner| {
                        owner_call_literals(&assertion.text, owner)
                            .iter()
                            .any(|literal| match_arm_literals.contains(literal))
                    }))
    } else if wrapper_seam {
"#,
        r#"    let has_token_match = if matches!(family, ProbeFamily::MatchArm) {
        !match_arm_guarded
            && (match_arm_variants
                .iter()
                .any(|v| contains_as_whole_word(&assertion.text, v))
                || !match_arm_literals.is_empty()
                    && !import_defeats_owner
                    && !cross_package_defeats_owner
                    && owner_callee.is_some_and(|owner| {
                        owner_call_literals(&assertion.text, owner)
                            .iter()
                            .any(|literal| match_arm_literals.contains(literal))
                    })
                || match_arm_bool_tuple.is_some_and(|pattern| {
                    !import_defeats_owner
                        && !cross_package_defeats_owner
                        && owner_callee.is_some_and(|owner| {
                            owner_call_bool_tuples(&assertion.text, owner).contains(&pattern)
                        })
                }))
    } else if wrapper_seam {
"#,
    )?;

    replace_once(
        &mut source,
        r#"            match_arm_variants,
            match_arm_literals,
            match_arm_guarded: false,
"#,
        r#"            match_arm_variants,
            match_arm_literals,
            match_arm_bool_tuple: None,
            match_arm_guarded: false,
"#,
    )?;

    if !source.contains("owner_call_bool_tuples(&assertion.text, owner).contains(&pattern)") {
        return Err("generated source is missing tuple-arm confirmation".to_string());
    }

    let report_dir = workspace.join("target/ripr/reports/tuple-match-arm");
    fs::create_dir_all(&report_dir)
        .map_err(|error| format!("create {} failed: {error}", report_dir.display()))?;
    let output = report_dir.join("reveal.rs");
    fs::write(&output, source)
        .map_err(|error| format!("write {} failed: {error}", output.display()))?;
    Ok(())
}
