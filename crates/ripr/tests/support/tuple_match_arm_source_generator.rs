//! Temporary guarded source emitter for the test-first tuple-arm repair.
//!
//! The public-API red witness calls this before its expected assertion failure,
//! so CI retains complete candidate source files in a bounded artifact. Every
//! transform is an exact single-occurrence replacement. The emitted files are
//! inspected and compiled before being committed; this helper is removed before
//! qualification.

use std::fs;
use std::path::{Path, PathBuf};

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

fn read(workspace: &Path, relative: &str) -> Result<String, String> {
    let path = workspace.join(relative);
    fs::read_to_string(&path).map_err(|error| format!("read {} failed: {error}", path.display()))
}

fn write(report: &Path, name: &str, source: String) -> Result<(), String> {
    let path = report.join(name);
    fs::write(&path, source).map_err(|error| format!("write {} failed: {error}", path.display()))
}

pub(super) fn emit_candidate() -> Result<(), String> {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let workspace = manifest
        .parent()
        .and_then(Path::parent)
        .ok_or_else(|| format!("cannot derive workspace root from {}", manifest.display()))?;
    let report = workspace.join("target/ripr/reports/tuple-match-arm");
    fs::create_dir_all(&report)
        .map_err(|error| format!("create {} failed: {error}", report.display()))?;

    let mut classify_mod = read(workspace, "crates/ripr/src/analysis/classify/mod.rs")?;
    replace_once(
        &mut classify_mod,
        "pub(in crate::analysis) use activation::activation_evidence;",
        "pub(in crate::analysis) use activation::{activation_evidence, function_parameters};",
    )?;
    write(&report, "classify_mod.rs", classify_mod)?;

    let mut probes_mod = read(workspace, "crates/ripr/src/analysis/probes/mod.rs")?;
    replace_once(
        &mut probes_mod,
        "pub(crate) use classify::parser_expression_for_probe;",
        "pub(crate) use classify::{\n    parser_expression_for_probe, parser_match_arm_has_direct_parameter_tuple,\n};",
    )?;
    write(&report, "probes_mod.rs", probes_mod)?;

    let mut probes_classify = read(workspace, "crates/ripr/src/analysis/probes/classify.rs")?;
    replace_once(
        &mut probes_classify,
        r#"pub(crate) fn parser_expression_for_probe<'a>(
    index: &'a RustIndex,
    file: &Path,
    line: usize,
    family: &ProbeFamily,
    changed_text: &str,
) -> Option<&'a str> {
    parser_probe_shapes_for_changed_line(index, file, line, changed_text)
        .into_iter()
        .find(|shape| &shape.family == family)
        .map(|shape| shape.text)
}
"#,
        r#"pub(crate) fn parser_expression_for_probe<'a>(
    index: &'a RustIndex,
    file: &Path,
    line: usize,
    family: &ProbeFamily,
    changed_text: &str,
) -> Option<&'a str> {
    parser_probe_shapes_for_changed_line(index, file, line, changed_text)
        .into_iter()
        .find(|shape| &shape.family == family)
        .map(|shape| shape.text)
}

/// Whether the exact parser-owned match arm is enclosed by a match whose
/// scrutinee is exactly the owner's two parameters, in declaration order.
///
/// The arm is selected through the same parser shape and fat-arrow byte
/// identity used for the probe. Reordered, transformed, constant, wrapped,
/// nested, trailing-comma, or otherwise non-direct scrutinees fail closed.
pub(crate) fn parser_match_arm_has_direct_parameter_tuple(
    index: &RustIndex,
    file: &Path,
    line: usize,
    changed_text: &str,
    parameters: &[String],
) -> bool {
    if parameters.len() != 2 {
        return false;
    }
    let Some(facts) = file_facts(index, file) else {
        return false;
    };
    let Some(selected) = parser_probe_shapes_for_changed_line(index, file, line, changed_text)
        .into_iter()
        .find(|shape| shape.family == ProbeFamily::MatchArm)
    else {
        return false;
    };
    let parse = SourceFile::parse(&facts.source, Edition::CURRENT);
    if !parse.errors().is_empty() {
        return false;
    }
    for arm in parse
        .tree()
        .syntax()
        .descendants()
        .filter_map(ast::MatchArm::cast)
    {
        let Some(arrow) = arm.fat_arrow_token() else {
            continue;
        };
        if u32::from(arrow.text_range().start()) as usize != selected.start_byte {
            continue;
        }
        let Some(match_expression) = arm.syntax().ancestors().find_map(ast::MatchExpr::cast) else {
            return false;
        };
        let Some(scrutinee) = match_expression.expr() else {
            return false;
        };
        let range = scrutinee.syntax().text_range();
        let start = u32::from(range.start()) as usize;
        let end = u32::from(range.end()) as usize;
        let Some(text) = facts.source.get(start..end) else {
            return false;
        };
        return exact_direct_parameter_tuple(text, parameters);
    }
    false
}

fn exact_direct_parameter_tuple(scrutinee: &str, parameters: &[String]) -> bool {
    let scrutinee = scrutinee.trim();
    let Some(inner) = scrutinee
        .strip_prefix('(')
        .and_then(|inner| inner.strip_suffix(')'))
    else {
        return false;
    };
    let mut elements = inner.split(',');
    let (Some(first), Some(second), None) =
        (elements.next(), elements.next(), elements.next())
    else {
        return false;
    };
    first.trim() == parameters[0] && second.trim() == parameters[1]
}
"#,
    )?;
    if !probes_classify.contains("parser_match_arm_has_direct_parameter_tuple") {
        return Err("generated parser classifier is missing tuple binding".to_string());
    }
    write(&report, "probes_classify.rs", probes_classify)?;

    let mut evidence = read(workspace, "crates/ripr/src/analysis/classifier/evidence.rs")?;
    replace_once(
        &mut evidence,
        r#"    current_path_witness, file_imports_foreign_callee_name, infection_evidence, local_flow_sinks,
    package_prefix, propagation_evidence_with_witness, reach_evidence,
"#,
        r#"    current_path_witness, file_imports_foreign_callee_name, function_parameters,
    infection_evidence, local_flow_sinks, package_prefix, propagation_evidence_with_witness,
    reach_evidence,
"#,
    )?;
    replace_once(
        &mut evidence,
        "use crate::domain::*;\n",
        "use crate::analysis::probes::parser_match_arm_has_direct_parameter_tuple;\nuse crate::domain::*;\n",
    )?;
    replace_once(
        &mut evidence,
        r#"        let owner_package = context
            .owner_fn
            .and_then(|owner| package_prefix(&owner.file));
        let (observe, discriminate, related_tests) = reveal_evidence_with_expression(
"#,
        r#"        let owner_package = context
            .owner_fn
            .and_then(|owner| package_prefix(&owner.file));
        let match_arm_direct_parameter_tuple = matches!(context.probe.family, ProbeFamily::MatchArm)
            && context.owner_fn.is_some_and(|owner| {
                let parameters = function_parameters(owner);
                parser_match_arm_has_direct_parameter_tuple(
                    context.index,
                    &context.probe.location.file,
                    context.probe.location.line,
                    &context.probe.expression,
                    &parameters,
                )
            });
        let (observe, discriminate, related_tests) = reveal_evidence_with_expression(
"#,
    )?;
    replace_once(
        &mut evidence,
        r#"            context.probe,
            reveal_expression,
            &context.related_tests,
            // #3731 review (F11): the related test's file source is
"#,
        r#"            context.probe,
            reveal_expression,
            &context.related_tests,
            match_arm_direct_parameter_tuple,
            // #3731 review (F11): the related test's file source is
"#,
    )?;
    write(&report, "classifier_evidence.rs", evidence)?;

    let mut reveal = read(workspace, "crates/ripr/src/analysis/classify/reveal.rs")?;
    replace_once(
        &mut reveal,
        r#"        related_tests,
        &|_, _| false,
        &|_, _| false,
"#,
        r#"        related_tests,
        false,
        &|_, _| false,
        &|_, _| false,
"#,
    )?;
    replace_once(
        &mut reveal,
        r#"    analysis_expression: &str,
    related_tests: &[(&TestSummary, RelationReason)],
    same_name_import_defeats: &dyn Fn(&TestSummary, &str) -> bool,
"#,
        r#"    analysis_expression: &str,
    related_tests: &[(&TestSummary, RelationReason)],
    match_arm_direct_parameter_tuple: bool,
    same_name_import_defeats: &dyn Fn(&TestSummary, &str) -> bool,
"#,
    )?;
    replace_once(
        &mut reveal,
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
    // The bounded tuple slice is admitted only when the parser has linked the
    // exact arm to a match over the owner's two parameters in declaration
    // order. Pattern syntax or call-argument positions alone are insufficient.
    let match_arm_bool_tuple = if matches!(probe.family, ProbeFamily::MatchArm)
        && match_arm_direct_parameter_tuple
    {
        match_arm_pattern_bool_tuple(analysis_expression)
    } else {
        None
    };
"#,
    )?;
    replace_once(
        &mut reveal,
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
        &mut reveal,
        r#"    match_arm_variants: &'a [String],
    match_arm_literals: &'a [String],
    /// True when the parser-owned arm pattern carries a guard. This slice
"#,
        r#"    match_arm_variants: &'a [String],
    match_arm_literals: &'a [String],
    /// Exact direct `(bool, bool)` pattern identity for the bounded tuple
    /// slice. `None` also represents a non-direct owner scrutinee binding.
    match_arm_bool_tuple: Option<[bool; 2]>,
    /// True when the parser-owned arm pattern carries a guard. This slice
"#,
    )?;
    replace_once(
        &mut reveal,
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
/// Bindings, alternation, constants, wildcards, nested values, trailing
/// elements, and malformed syntax fail closed.
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
        &mut reveal,
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
        &mut reveal,
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
        &mut reveal,
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
        &mut reveal,
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
    if !reveal.contains("match_arm_direct_parameter_tuple")
        || !reveal.contains("owner_call_bool_tuples(&assertion.text, owner).contains(&pattern)")
    {
        return Err("generated reveal source is missing bounded tuple confirmation".to_string());
    }
    write(&report, "reveal.rs", reveal)?;

    let mut manifest = String::new();
    for entry in [
        ("classify_mod.rs", "crates/ripr/src/analysis/classify/mod.rs"),
        ("probes_mod.rs", "crates/ripr/src/analysis/probes/mod.rs"),
        ("probes_classify.rs", "crates/ripr/src/analysis/probes/classify.rs"),
        ("classifier_evidence.rs", "crates/ripr/src/analysis/classifier/evidence.rs"),
        ("reveal.rs", "crates/ripr/src/analysis/classify/reveal.rs"),
    ] {
        manifest.push_str(entry.0);
        manifest.push('\t');
        manifest.push_str(entry.1);
        manifest.push('\n');
    }
    write(&report, "paths.tsv", manifest)?;
    Ok(())
}
