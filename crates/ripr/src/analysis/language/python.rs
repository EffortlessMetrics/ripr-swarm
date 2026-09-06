//! Python preview adapter.
//!
//! See `docs/specs/RIPR-SPEC-0028-python-preview-static-facts.md` and
//! `docs/adr/0009-python-parser-substrate.md`.
//!
//! This slice extracts the first useful syntax-first Python facts:
//!
//! - owners for module functions, async functions, class methods, and
//!   `@staticmethod` / `@classmethod` methods;
//! - pytest `test_*` functions, parametrized pytest tests, and
//!   `unittest.TestCase.test_*` methods;
//! - pytest, unittest, and mock assertion/oracle facts;
//! - related-test references by direct calls, import-alias calls, and
//!   conservative same-stem proximity.
//!
//! Import-graph matching, static limits, editor routing, generated tests,
//! runtime execution, and provider calls remain out of scope.
//! Strong exact-value assertions can produce `exposed`; weaker or unknown
//! related-test oracles produce `weakly_exposed`; missing related tests produce
//! `no_static_path`.

use super::super::{
    AnalysisOptions, diff::ChangedFile, fingerprint_probe_id, normalize_expression,
};
use super::{LanguageAdapter, LanguageDiffResult, LanguageId, LanguageRepoResult, route};
use crate::config::OraclePolicy;
#[cfg(test)]
use crate::domain::{
    DeltaKind, FlowSinkKind, LanguageId as DomainLanguageId, LanguageStatus, RelatedTest,
    StageState,
};
use crate::domain::{
    ExposureClass, Finding, MissingDiscriminatorFact, OracleKind, OracleStrength, OwnerKind,
    ProbeFamily, StaticLimitKind, StopReason, SymbolId,
};
use rustpython_parser::ast::Expr;
use std::{
    collections::BTreeMap,
    ops::RangeInclusive,
    path::{Path, PathBuf},
};
mod classify;
use classify::{PythonNoBehaviorContext, classify_change_with_context};
#[cfg(test)]
use classify::{classify_change, classify_change_with_old};
mod discriminators;
mod no_behavior;
mod oracles;
mod owners_tests;
mod probe_shape;
mod related_tests;
mod sink_alignment;
mod source_facts;
#[cfg(test)]
use discriminators::{
    first_python_keyword_argument, is_literal_python_model_field_value,
    is_python_constructor_callee, is_simple_python_model_field_value,
    python_keyword_argument_parts, python_output_or_call_discriminator,
    python_return_constructor_field_parts, python_return_dict_field_parts,
    python_route_response_field_discriminator, split_python_constructor_call, top_level_equals,
};
use discriminators::{
    first_python_string_literal, is_python_control_predicate_line, parse_attribute_assignment,
    python_assignment_constructor_field_parts, python_dict_field_segment_parts,
    python_exit_code_discriminator, python_return_constructor_field_discriminator,
    python_return_dict_field_discriminator, python_string_literal_value, split_python_assignment,
    top_level_python_segments,
};
#[cfg(test)]
use no_behavior::{
    analyze_call_args, changed_default_value_params, free_function_call_arglists,
    is_annotation_only_def_change, is_annotation_only_var_change, is_python_no_behavior_line,
};
use oracles::collect_assertions_from_statements;
#[cfg(test)]
use probe_shape::{
    canonical_python_gap_for, classify_probe_shape, contains_mock_initializer,
    looks_like_call_expression,
};
#[cfg(test)]
use related_tests::{
    PythonRelationKind, binding_target_for_construction, body_calls_owner,
    construct_result_is_called, contains_any_attribute_call, find_related_tests,
    has_unclosed_quote, imported_module_matches_owner, local_binding_calls_owner,
    normalize_similarity_key, normalize_test_stem, owner_similarity_keys, related_test_candidates,
    related_test_relation, same_stem_related, similarity_key_contains, verify_command_for_test,
};
use related_tests::{
    first_parenthesized_string_argument, import_source_module_matches_owner,
    strong_test_calls_owner_method_on_bound_receiver, strong_test_imports_owner_from_module,
};
#[cfg(test)]
use sink_alignment::strong_oracle_observes_owner;
#[cfg(test)]
use sink_alignment::{
    classify_sink_alignment, dict_changed_keys_and_values, fstring_change_is_length_invariant,
    fstring_template, oracle_is_pure_len_aggregate, oracle_text_observes_token,
    parse_dict_literal_fields,
};
pub(crate) use source_facts::detect_python_test_framework;
#[cfg(test)]
use source_facts::parse_module;
use source_facts::{extract_source_facts, source_fact_snapshot_observation};
mod source_utils;
#[cfg(test)]
use source_utils::line_for_offset;
use source_utils::{is_test_file, normalized_path};
mod static_limits;
use static_limits::{
    PythonStaticLimit, has_identifier_boundary, line_prefix_before,
    python_callee_start_has_boundary, python_prefix_hides_code,
};
#[cfg(test)]
use static_limits::{
    contains_dynamic_dispatch, contains_dynamic_import, contains_metaprogramming,
    is_known_mock_constructor_import, is_transparent_owner_decorator,
    is_transparent_owner_decorator_for_owner, line_uses_imported_symbol, static_limit_for_change,
    test_has_mocked_module,
};

mod workspace;
#[cfg(test)]
use workspace::visit_workspace;
use workspace::{
    collect_workspace_python_files, is_detectable_generated_python_file, line_is_in_ranges,
    owner_for_changed_line, reconstruct_old_source,
};

const PYTHON_WORKSPACE_EXCLUDED_DIRS: &[&str] = &[
    ".git",
    "target",
    "node_modules",
    ".ripr",
    ".direnv",
    "__pycache__",
    ".venv",
    "venv",
    "env",
    ".tox",
    ".nox",
    "site-packages",
    ".pytest_cache",
    ".mypy_cache",
    "dist",
    "build",
];

/// Python preview adapter.
///
/// Stateless: routing, parsing, and per-file extraction only.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct PythonAdapter;

#[derive(Clone, Debug, PartialEq, Eq)]
struct PythonOwner {
    name: String,
    qualified_name: String,
    file: PathBuf,
    start_line: usize,
    end_line: usize,
    owner_kind: Option<OwnerKind>,
    decorators: Vec<String>,
    imports: Vec<PythonImport>,
    cli_receiver_names: Vec<String>,
    route_paths: Vec<String>,
    dynamic_route_decorators: Vec<String>,
}

impl PythonOwner {
    fn symbol_id(&self) -> SymbolId {
        SymbolId(format!(
            "python:{}::{}",
            normalized_path(&self.file),
            self.qualified_name
        ))
    }

    fn is_module_owner(&self) -> bool {
        self.qualified_name == "<module>"
    }

    fn specificity_rank(&self) -> usize {
        if self.owner_kind.is_some() {
            0
        } else if self.is_module_owner() {
            2
        } else {
            1
        }
    }

    fn span_width(&self) -> usize {
        self.end_line.saturating_sub(self.start_line)
    }

    fn kind_label(&self) -> &'static str {
        match self.owner_kind {
            Some(kind) => kind.as_str(),
            None if self.is_module_owner() => "module_function",
            None => "class",
        }
    }

    fn missing_test_reference(&self) -> String {
        if self.is_module_owner() {
            format!("module-level behavior in `{}`", normalized_path(&self.file))
        } else {
            format!("`{}(`", self.name)
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PythonTest {
    name: String,
    qualified_name: String,
    file: PathBuf,
    line: usize,
    body_text: String,
    imports: Vec<PythonImport>,
    decorators: Vec<String>,
    fixtures: Vec<String>,
    parametrized: bool,
    framework: &'static str,
    assertions: Vec<PythonAssertion>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PythonImport {
    imported: String,
    alias: String,
    /// The dotted source module of a `from M import Y` statement (e.g. `src.handler`).
    /// Relative imports are resolved against the importing file when possible (for
    /// example, `from .handler import validate` in `tests/test_api.py` resolves to
    /// `tests.handler`). Empty for a plain `import X`.
    source_module: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PythonAssertion {
    text: String,
    line: usize,
    oracle_kind: OracleKind,
    oracle_strength: OracleStrength,
    oracle_shape: PythonOracleShape,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum PythonOracleShape {
    ExactAssertion,
    BoundaryAssertion,
    ExceptionAssertion,
    FieldAssertion,
    OutputAssertion,
    StatusCodeAssertion,
    BroadSmokeAssertion,
    MockExpectation,
    UnknownCustomHelper,
}

impl PythonOracleShape {
    fn as_str(self) -> &'static str {
        match self {
            Self::ExactAssertion => "exact_assertion",
            Self::BoundaryAssertion => "boundary_assertion",
            Self::ExceptionAssertion => "exception_assertion",
            Self::FieldAssertion => "field_assertion",
            Self::OutputAssertion => "output_assertion",
            Self::StatusCodeAssertion => "status_code_assertion",
            Self::BroadSmokeAssertion => "broad_smoke_assertion",
            Self::MockExpectation => "mock_expectation",
            Self::UnknownCustomHelper => "unknown_custom_helper",
        }
    }
}

fn expr_full_name(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Name(name) => Some(name.id.to_string()),
        Expr::Attribute(attribute) => expr_full_name(attribute.value.as_ref())
            .map(|prefix| format!("{prefix}.{}", attribute.attr)),
        Expr::Call(call) => expr_full_name(call.func.as_ref()),
        _ => None,
    }
}

fn stop_reason_for_python_static_limit(limit: &PythonStaticLimit) -> StopReason {
    match limit.kind {
        StaticLimitKind::DynamicDispatch => StopReason::DynamicDispatchUnresolved,
        _ => StopReason::StaticProbeUnknown,
    }
}

fn python_weak_missing_summary(
    owner: &PythonOwner,
    probe_family: &ProbeFamily,
    strongest_kind: &OracleKind,
) -> String {
    let shape = match probe_family {
        ProbeFamily::Predicate => "the changed boundary",
        ProbeFamily::ReturnValue => "the returned value",
        ProbeFamily::ErrorPath => "the exact exception type/message",
        ProbeFamily::FieldConstruction => "the changed field or object value",
        ProbeFamily::SideEffect | ProbeFamily::CallDeletion => "the changed output/log/call effect",
        ProbeFamily::MatchArm => "the changed match arm",
        ProbeFamily::StaticUnknown => "the changed behavior",
    };
    format!(
        "Related Python test reaches `{}` but the strongest extracted oracle is `{}`; add or strengthen a focused assertion for {shape}.",
        owner.name,
        strongest_kind.as_str()
    )
}

fn python_recommended_next_step(
    class: &ExposureClass,
    probe_family: &ProbeFamily,
    has_oracle_eligible_relation: bool,
    missing_discriminators: &[MissingDiscriminatorFact],
) -> Option<String> {
    match class {
        ExposureClass::StaticUnknown | ExposureClass::NoStaticPath => None,
        ExposureClass::Exposed => {
            Some("Python preview: changed behavior is observed under a strong oracle; verify the assertion targets the changed behavior.".to_string())
        }
        _ if !has_oracle_eligible_relation => None,
        _ => {
            let missing = &missing_discriminators.first()?.value;
            let action = match probe_family {
                ProbeFamily::Predicate => "strengthen the existing related test with a focused boundary assertion",
                ProbeFamily::ReturnValue => {
                    "strengthen the existing related test with an exact return-value assertion"
                }
                ProbeFamily::ErrorPath => {
                    "strengthen the existing related test with an exception assertion"
                }
                ProbeFamily::FieldConstruction => {
                    "strengthen the existing related test with a field/object assertion"
                }
                ProbeFamily::SideEffect | ProbeFamily::CallDeletion => {
                    "strengthen the existing related test with an output/log/call-effect assertion"
                }
                ProbeFamily::MatchArm | ProbeFamily::StaticUnknown => {
                    "strengthen the existing related test with a focused assertion"
                }
            };
            Some(format!(
                "Python preview: {action} for missing discriminator `{missing}`."
            ))
        }
    }
}

/// Significant identifier and string-literal tokens from a changed source line.
/// These approximate the *changed sink* — the attribute/field/value the change
/// touches — so an oracle that asserts on them is observing the change.
fn significant_change_tokens(line_text: &str) -> Vec<String> {
    const STOP: &[&str] = &[
        "self", "cls", "return", "if", "elif", "else", "while", "for", "def", "none", "true",
        "false", "and", "or", "not", "in", "is", "raise", "assert", "yield", "await", "async",
        "class", "import", "from", "as", "with", "try", "except", "lambda",
    ];
    let mut out = Vec::new();
    let mut current = String::new();
    for ch in line_text.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' {
            current.push(ch);
        } else {
            push_identifier(&mut out, &mut current, STOP);
        }
    }
    push_identifier(&mut out, &mut current, STOP);
    for quote in ['"', '\''] {
        for (index, part) in line_text.split(quote).enumerate() {
            if index % 2 == 1 && part.len() >= 2 {
                out.push(part.to_string());
            }
        }
    }
    out
}

fn push_identifier(out: &mut Vec<String>, current: &mut String, stop: &[&str]) {
    if current.len() >= 3
        && !stop.contains(&current.to_ascii_lowercase().as_str())
        && !current.chars().all(|c| c.is_ascii_digit())
    {
        out.push(current.clone());
    }
    current.clear();
}

impl LanguageAdapter for PythonAdapter {
    fn accepts_path(&self, path: &Path) -> bool {
        matches!(route(path), Some(LanguageId::Python))
    }

    fn analyze_diff(
        &self,
        options: &AnalysisOptions,
        _oracle_policy: &OraclePolicy,
        changed_files: &[ChangedFile],
    ) -> Result<LanguageDiffResult, String> {
        let workspace_files = collect_workspace_python_files(&options.root);
        let mut all_owners: Vec<PythonOwner> = Vec::new();
        let mut all_tests: Vec<PythonTest> = Vec::new();
        let mut docstring_ranges_by_file: BTreeMap<PathBuf, Vec<RangeInclusive<usize>>> =
            BTreeMap::new();
        for relative in &workspace_files {
            let absolute = options.root.join(relative);
            let Ok(source) = std::fs::read_to_string(&absolute) else {
                continue;
            };
            let facts = extract_source_facts(relative, &source);
            debug_assert!(source_fact_snapshot_observation(&facts) > 0);
            docstring_ranges_by_file.insert(relative.clone(), facts.docstring_line_ranges.clone());
            if is_test_file(relative) {
                all_tests.extend(facts.tests);
            } else {
                all_owners.extend(facts.owners);
            }
        }

        let mut findings: Vec<Finding> = Vec::new();
        let mut changed_count: usize = 0;
        for changed in changed_files {
            if !self.accepts_path(&changed.path)
                || is_detectable_generated_python_file(&changed.path)
            {
                continue;
            }
            changed_count += 1;
            if is_test_file(&changed.path) {
                continue;
            }
            let new_docstring_ranges = docstring_ranges_by_file
                .get(&changed.path)
                .map(Vec::as_slice)
                .unwrap_or_default();
            let old_docstring_ranges = std::fs::read_to_string(options.root.join(&changed.path))
                .ok()
                .and_then(|source| reconstruct_old_source(&source, changed))
                .map(|source| extract_source_facts(&changed.path, &source).docstring_line_ranges)
                .unwrap_or_default();
            for added in &changed.added_lines {
                // Pair the in-place removed line (same new-side position) so the
                // classifier can credit the changed-sink token on the DELTA only.
                let old_line = changed
                    .removed_lines
                    .iter()
                    .find(|removed| removed.new_side_line == added.line);
                let old_line_text = old_line.map(|removed| removed.text.as_str());
                let no_behavior = PythonNoBehaviorContext {
                    new_line_in_docstring: line_is_in_ranges(added.line, new_docstring_ranges),
                    old_line_in_docstring: old_line.is_some_and(|removed| {
                        line_is_in_ranges(removed.line, &old_docstring_ranges)
                    }),
                };
                if let Some(finding) = classify_change_with_context(
                    &changed.path,
                    added.line,
                    &added.text,
                    old_line_text,
                    &all_owners,
                    &all_tests,
                    no_behavior,
                ) {
                    findings.push(finding);
                }
            }
        }
        Ok(LanguageDiffResult {
            findings,
            harness_projections: Vec::new(),
            changed_files: changed_count,
            candidate_line_count: 0,
            changed_files_by_language: Vec::new(),
            partial_scope: None,
            skipped_files: 0,
            limitations: Vec::new(),
        })
    }

    fn analyze_repo(
        &self,
        _options: &AnalysisOptions,
        _oracle_policy: &OraclePolicy,
    ) -> Result<LanguageRepoResult, String> {
        // Repo-mode preview output lands in a follow-up. The current
        // sub-slice scopes to diff-mode for the smallest useful fixture.
        // This stub returns an empty result; callers that consume
        // repo-scoped formats on a Python-only workspace get zero seams
        // with no warning. See docs/LANGUAGE_ADAPTER_PREVIEW.md
        // § "Repo-Mode Analysis Is Rust-Only" for the limitation contract.
        Ok(LanguageRepoResult {
            findings: Vec::new(),
            harness_projections: Vec::new(),
            production_files: 0,
            skipped_files: 0,
        })
    }
}

#[cfg(test)]
mod python_tests;

#[cfg(test)]
mod tests;

#[cfg(test)]
mod tests_inline {
    use super::owners_tests::{extract_owners, extract_tests};
    use super::tests::{changed, evidence_value, missing_discriminator_values};
    use super::*;
    use std::path::{Path, PathBuf};

    #[test]
    fn free_function_call_arglists_matches_only_direct_calls() {
        let body =
            "render(\"Sam\")\nobj.render(\"X\")\nrenderer(\"Y\")\nrender(\"Z\", verbose=False)\n";
        let calls = free_function_call_arglists(body, "render");
        // `obj.render(...)` (method access) and `renderer(...)` (longer name) excluded.
        assert_eq!(calls, vec!["\"Sam\"", "\"Z\", verbose=False"]);
    }

    #[test]
    fn free_function_call_arglists_skips_comment_and_string_mentions() {
        // A `# comment` or string literal that mentions the owner name must not be
        // read as a live call: a comment with `)` would otherwise break paren
        // matching and a string mention would invent a call that does not run.
        let body = "render(\"Sam\")\n# see also render(other)\nx = \"render(unparsed)\"\n";
        let calls = free_function_call_arglists(body, "render");
        // Only the first, real call is captured.
        assert_eq!(calls, vec!["\"Sam\""]);
    }

    #[test]
    fn fstring_format_spec_change_is_not_length_invariant() {
        // #1290 1b: a format-spec change alters an interpolation, so it is NOT
        // length-invariant — a `len` oracle could discriminate it, and it must not be
        // gated. (Trap 58 / 53 preservation.)
        assert!(!fstring_change_is_length_invariant(
            "    return f\"{value:.2f}\"",
            "    return f\"{value:.3f}\""
        ));
        // Equal-length literal-only change IS length-invariant.
        assert!(fstring_change_is_length_invariant(
            "    return f\"OK:{code}\"",
            "    return f\"NO:{code}\""
        ));
        // A literal change that alters length is NOT invariant (len can discriminate).
        assert!(!fstring_change_is_length_invariant(
            "    return f\"{x}\"",
            "    return f\"{x}!\""
        ));
        // Non-f-string lines are never invariant.
        assert!(!fstring_change_is_length_invariant(
            "    return x + 1",
            "    return x - 1"
        ));
        // Fail open on unsupported shapes (escaped / nested braces) -> no template.
        assert_eq!(fstring_template("    return f\"{{OK}}:{code}\""), None);
        assert_eq!(fstring_template("    return f\"{value:{width}}\""), None);
        assert!(!fstring_change_is_length_invariant(
            "    return f\"{{OK}}:{code}\"",
            "    return f\"{{NO}}:{code}\""
        ));
    }

    #[test]
    fn oracle_pure_len_aggregate_detection() {
        // Pure length-only observation:
        assert!(oracle_is_pure_len_aggregate(
            "len(status_label(7)) == 4",
            "NO:"
        ));
        // Also observes the output exactly -> NOT pure aggregate (keeps credit):
        assert!(!oracle_is_pure_len_aggregate(
            "len(status_label(7)) == 4 and status_label(7) == \"NO:7\"",
            "NO:"
        ));
        // Contains the changed literal -> NOT pure aggregate:
        assert!(!oracle_is_pure_len_aggregate(
            "status_label(7).startswith(\"NO:\")",
            "NO:"
        ));
        // No len at all -> not a len aggregate:
        assert!(!oracle_is_pure_len_aggregate(
            "status_label(7) == \"NO:7\"",
            "NO:"
        ));
    }

    #[test]
    fn fstring_len_plus_exact_oracle_stays_exposed() -> Result<(), String> {
        // #1290 1b hardening: when the test observes BOTH len() AND the exact output
        // (in separate assertions), the exact-value assertion is the strong oracle and
        // must keep the credit — the len-aggregate gate must not downgrade it. (A single
        // `assert a and b` is `smoke_only`/weak for an unrelated reason, so the
        // discriminating form uses separate assertions.)
        let source = "def status_label(code):\n    return f\"NO:{code}\"\n";
        let owners = extract_owners(Path::new("src/status.py"), source);
        let tests = extract_tests(
            Path::new("tests/test_status.py"),
            "from src.status import status_label\n\n\ndef test_both():\n    assert len(status_label(7)) == 4\n    assert status_label(7) == \"NO:7\"\n",
        );
        let Some(finding) = classify_change_with_old(
            Path::new("src/status.py"),
            2,
            "    return f\"NO:{code}\"",
            Some("    return f\"OK:{code}\""),
            &owners,
            &tests,
        ) else {
            return Err("changed f-string should classify".to_string());
        };
        assert_eq!(
            finding.class,
            ExposureClass::Exposed,
            "an oracle that also observes the exact output is not a pure len aggregate"
        );
        Ok(())
    }

    #[test]
    fn dict_changed_element_whole_comparison_stays_exposed() -> Result<(), String> {
        // #1290 preserve: a whole-collection comparison observes every element,
        // including the changed one.
        let source = "def build_config():\n    return {\"host\": \"localhost\", \"port\": 9090}\n";
        let owners = extract_owners(Path::new("src/conf.py"), source);
        let tests = extract_tests(
            Path::new("tests/test_conf.py"),
            "from src.conf import build_config\n\n\ndef test_cfg():\n    assert build_config() == {\"host\": \"localhost\", \"port\": 9090}\n",
        );
        let Some(finding) = classify_change_with_old(
            Path::new("src/conf.py"),
            2,
            "    return {\"host\": \"localhost\", \"port\": 9090}",
            Some("    return {\"host\": \"localhost\", \"port\": 8080}"),
            &owners,
            &tests,
        ) else {
            return Err("changed dict literal should classify".to_string());
        };
        assert_eq!(
            finding.class,
            ExposureClass::Exposed,
            "a whole-collection comparison observes the changed element"
        );
        Ok(())
    }

    #[test]
    fn noop_docstring_only_change_emits_no_probe() -> Result<(), String> {
        // #1279: a docstring-only change has no behavior delta. Even though the
        // strong `== 80` oracle observes the owner's output, there is nothing for
        // the test to discriminate, so no behavior probe must be emitted.
        let source = "def discount(price):\n    \"\"\"Apply the standard discount to a price.\"\"\"\n    return price * 0.8\n";
        let owners = extract_owners(Path::new("src/pricing.py"), source);
        let tests = extract_tests(
            Path::new("tests/test_pricing.py"),
            "from src.pricing import discount\n\n\ndef test_discount():\n    assert discount(100) == 80\n",
        );
        let finding = classify_change_with_old(
            Path::new("src/pricing.py"),
            2,
            "    \"\"\"Apply the standard discount to a price.\"\"\"",
            Some("    \"Apply a discount.\""),
            &owners,
            &tests,
        );
        assert!(
            finding.is_none(),
            "a docstring-only change carries no behavior delta and must emit no probe"
        );
        Ok(())
    }

    #[test]
    fn noop_comment_only_change_emits_no_probe() -> Result<(), String> {
        // #1279: a comment-only change is likewise a no-op.
        let source =
            "def discount(price):\n    # apply the standard discount\n    return price * 0.8\n";
        let owners = extract_owners(Path::new("src/pricing.py"), source);
        let tests = extract_tests(
            Path::new("tests/test_pricing.py"),
            "from src.pricing import discount\n\n\ndef test_discount():\n    assert discount(100) == 80\n",
        );
        let finding = classify_change_with_old(
            Path::new("src/pricing.py"),
            2,
            "    # apply the standard discount",
            Some("    # apply a discount"),
            &owners,
            &tests,
        );
        assert!(
            finding.is_none(),
            "a comment-only change carries no behavior delta and must emit no probe"
        );
        Ok(())
    }

    #[test]
    fn real_body_change_in_same_function_still_classifies() -> Result<(), String> {
        // #1279 inverse control: the no-op guard must not suppress a genuine body
        // change. The same `discount` owner with a real return-value edit still
        // classifies `exposed` under the strong output oracle.
        let source = "def discount(price):\n    return price * 0.8\n";
        let owners = extract_owners(Path::new("src/pricing.py"), source);
        let tests = extract_tests(
            Path::new("tests/test_pricing.py"),
            "from src.pricing import discount\n\n\ndef test_discount():\n    assert discount(100) == 80\n",
        );
        let Some(finding) = classify_change_with_old(
            Path::new("src/pricing.py"),
            2,
            "    return price * 0.8",
            Some("    return price * 0.9"),
            &owners,
            &tests,
        ) else {
            return Err("a real body change must still classify".to_string());
        };
        assert_eq!(
            finding.class,
            ExposureClass::Exposed,
            "a real return-value change observed by a strong oracle stays exposed"
        );
        Ok(())
    }

    #[test]
    fn no_behavior_line_detection_is_conservative() {
        // No-op shapes:
        assert!(is_python_no_behavior_line("    \"\"\"A docstring.\"\"\""));
        assert!(is_python_no_behavior_line("'''triple single'''"));
        assert!(is_python_no_behavior_line("    \"single line string\""));
        assert!(is_python_no_behavior_line("    # a comment"));
        assert!(is_python_no_behavior_line("   "));
        assert!(is_python_no_behavior_line("r\"raw docstring\""));
        assert!(is_python_no_behavior_line("b\"bytes literal\""));
        // Behavioral shapes must NOT be treated as no-ops:
        assert!(!is_python_no_behavior_line("    return \"x\""));
        assert!(!is_python_no_behavior_line("    result = \"x\""));
        assert!(!is_python_no_behavior_line("    raise ValueError(\"x\")"));
        assert!(!is_python_no_behavior_line("    f\"{compute()}\""));
        assert!(!is_python_no_behavior_line("    rf\"{compute()}\""));
        assert!(!is_python_no_behavior_line("    \"a\" + str(x)"));
        assert!(!is_python_no_behavior_line("    return price * 0.8"));
    }

    #[test]
    fn classify_change_exposed_boundary_does_not_emit_missing_discriminator() -> Result<(), String>
    {
        let owners = extract_owners(
            Path::new("src/discount.py"),
            "def apply_discount(amount, threshold):\n    if amount >= threshold:\n        return amount - 10\n    return amount\n",
        );
        let tests = extract_tests(
            Path::new("tests/test_discount.py"),
            "from src.discount import apply_discount\n\ndef test_apply_discount_boundary():\n    assert apply_discount(100, 100) == 90\n",
        );

        let Some(finding) = classify_change(
            Path::new("src/discount.py"),
            2,
            "    if amount >= threshold:",
            &owners,
            &tests,
        ) else {
            return Err("changed predicate inside owner should classify".to_string());
        };

        assert_eq!(finding.class, ExposureClass::Exposed);
        assert!(finding.activation.missing_discriminators.is_empty());
        assert!(
            finding
                .evidence
                .iter()
                .all(|entry| !entry.starts_with("missing_discriminator:"))
        );
        Ok(())
    }

    #[test]
    fn classify_change_returns_weakly_exposed_when_related_test_exists() -> Result<(), String> {
        let owners = extract_owners(
            Path::new("src/pricing.py"),
            "def apply_discount(amount, threshold):\n    if amount >= threshold:\n        return amount - 10\n    return amount\n",
        );
        let tests = extract_tests(
            Path::new("tests/test_pricing.py"),
            "def test_apply_discount():\n    result = apply_discount(100, 50)\n",
        );

        let Some(finding) = classify_change(
            Path::new("src/pricing.py"),
            2,
            "    if amount >= threshold:",
            &owners,
            &tests,
        ) else {
            return Err("changed line inside owner should classify".to_string());
        };

        assert_eq!(finding.class, ExposureClass::WeaklyExposed);
        assert_eq!(finding.language, Some(DomainLanguageId::Python));
        assert_eq!(finding.language_status, Some(LanguageStatus::Preview));
        assert_eq!(finding.owner_kind, Some(OwnerKind::Function));
        assert_eq!(finding.ripr.reach.state, StageState::Yes);
        assert_eq!(finding.ripr.infect.state, StageState::Yes);
        assert_eq!(finding.ripr.propagate.state, StageState::Weak);
        assert_eq!(finding.ripr.reveal.observe.state, StageState::Weak);
        assert_eq!(finding.ripr.reveal.discriminate.state, StageState::Weak);
        assert_eq!(finding.related_tests.len(), 1);
        assert_eq!(finding.flow_sinks.len(), 1);
        assert_eq!(finding.flow_sinks[0].kind, FlowSinkKind::Unknown);
        assert_eq!(finding.activation.missing_discriminators.len(), 1);
        assert_eq!(
            finding.activation.missing_discriminators[0].value,
            "amount == threshold"
        );
        assert!(
            finding
                .evidence
                .iter()
                .any(|entry| entry == "missing_discriminator: amount == threshold")
        );
        assert!(finding.canonical_gap.is_some());
        assert!(finding.recommended_next_step.is_some());
        Ok(())
    }

    #[test]
    fn classify_exception_match_assertion_as_exposed() -> Result<(), String> {
        let finding = classify_change(
            Path::new("src/validation.py"),
            3,
            "        raise ValueError(\"positive required\")",
            &extract_owners(
                Path::new("src/validation.py"),
                "def require_positive(value):\n    if value <= 0:\n        raise ValueError(\"positive required\")\n    return value\n",
            ),
            &extract_tests(
                Path::new("tests/test_validation.py"),
                "import pytest\nfrom src.validation import require_positive\n\n\
                 def test_rejects_zero_value():\n    with pytest.raises(ValueError, match=\"positive required\"):\n        require_positive(0)\n",
            ),
        )
        .ok_or_else(|| "exception-path change should classify".to_string())?;
        assert_eq!(finding.class, ExposureClass::Exposed);
        assert!(missing_discriminator_values(&finding).is_empty());
        assert_eq!(
            finding.related_tests[0].oracle_kind,
            OracleKind::ExactErrorVariant
        );
        assert_eq!(
            finding.related_tests[0].oracle_strength,
            OracleStrength::Strong
        );
        Ok(())
    }

    #[test]
    fn classify_field_assertion_as_exposed() -> Result<(), String> {
        let finding = classify_change(
            Path::new("src/invoice.py"),
            2,
            "    return {\"status\": \"paid\", \"id\": invoice_id}",
            &extract_owners(
                Path::new("src/invoice.py"),
                "def invoice_payload(invoice_id):\n    return {\"status\": \"paid\", \"id\": invoice_id}\n",
            ),
            &extract_tests(
                Path::new("tests/test_invoice.py"),
                "from src.invoice import invoice_payload\n\n\
                 def test_invoice_payload_status():\n    payload = invoice_payload(\"inv-123\")\n    assert payload[\"status\"] == \"paid\"\n",
            ),
        )
        .ok_or_else(|| "field-value change should classify".to_string())?;
        assert_eq!(finding.class, ExposureClass::Exposed);
        assert!(missing_discriminator_values(&finding).is_empty());
        assert_eq!(finding.related_tests[0].oracle_kind, OracleKind::ExactValue);
        assert_eq!(
            finding.related_tests[0].oracle_strength,
            OracleStrength::Strong
        );
        Ok(())
    }

    #[test]
    fn classify_output_assertion_as_exposed() -> Result<(), String> {
        let finding = classify_change(
            Path::new("src/notifications.py"),
            5,
            "    logger.warning(\"coupon expired\")",
            &extract_owners(
                Path::new("src/notifications.py"),
                "import logging\n\nlogger = logging.getLogger(__name__)\n\ndef warn_coupon():\n    logger.warning(\"coupon expired\")\n",
            ),
            &extract_tests(
                Path::new("tests/test_notifications.py"),
                "from src.notifications import warn_coupon\n\n\
                 def test_warn_coupon_exact_output(caplog):\n    warn_coupon()\n    assert caplog.text == \"coupon expired\"\n",
            ),
        )
        .ok_or_else(|| "output/log change should classify".to_string())?;
        assert_eq!(finding.class, ExposureClass::Exposed);
        assert!(missing_discriminator_values(&finding).is_empty());
        assert_eq!(finding.related_tests[0].oracle_kind, OracleKind::ExactValue);
        assert_eq!(
            finding.related_tests[0].oracle_strength,
            OracleStrength::Strong
        );
        Ok(())
    }

    #[test]
    fn classify_click_output_change_as_repairable_cli_gap() -> Result<(), String> {
        let finding = classify_change(
            Path::new("src/commands.py"),
            5,
            "    click.echo(\"shipment queued\")",
            &extract_owners(
                Path::new("src/commands.py"),
                "import click\n\n@click.command()\ndef ship():\n    click.echo(\"shipment queued\")\n",
            ),
            &extract_tests(
                Path::new("tests/test_commands.py"),
                "from src.commands import ship\n\n\
                 def test_ship_smoke(capsys):\n    ship()\n    captured = capsys.readouterr()\n    assert captured.out\n",
            ),
        )
        .ok_or_else(|| "click output change should classify".to_string())?;

        assert_eq!(finding.class, ExposureClass::WeaklyExposed);
        assert_eq!(finding.static_limit_kind, None);
        assert_eq!(
            missing_discriminator_values(&finding),
            vec!["output contains \"shipment queued\""]
        );
        assert_eq!(
            evidence_value(&finding, "suggested_verify_command: "),
            Some("pytest tests/test_commands.py::test_ship_smoke")
        );
        Ok(())
    }

    #[test]
    fn classify_change_emits_first_python_repair_class_discriminators() -> Result<(), String> {
        let return_finding = classify_change(
            Path::new("src/priority.py"),
            2,
            "    return amount >= 100",
            &extract_owners(
                Path::new("src/priority.py"),
                "def is_priority(amount):\n    return amount >= 100\n",
            ),
            &extract_tests(
                Path::new("tests/test_priority.py"),
                "from src.priority import is_priority\n\n\
                 def test_priority_amount():\n    assert is_priority(150)\n",
            ),
        )
        .ok_or_else(|| "return-value change should classify".to_string())?;
        assert_eq!(return_finding.class, ExposureClass::WeaklyExposed);
        assert_eq!(
            missing_discriminator_values(&return_finding),
            vec!["return value == amount >= 100"]
        );
        assert!(
            return_finding
                .recommended_next_step
                .as_deref()
                .is_some_and(|step| step.contains("return-value assertion"))
        );

        let exception_finding = classify_change(
            Path::new("src/validation.py"),
            3,
            "        raise ValueError(\"positive required\")",
            &extract_owners(
                Path::new("src/validation.py"),
                "def require_positive(value):\n    if value <= 0:\n        raise ValueError(\"positive required\")\n    return value\n",
            ),
            &extract_tests(
                Path::new("tests/test_validation.py"),
                "import pytest\nfrom src.validation import require_positive\n\n\
                 def test_rejects_zero_value():\n    with pytest.raises(ValueError):\n        require_positive(0)\n",
            ),
        )
        .ok_or_else(|| "exception-path change should classify".to_string())?;
        assert_eq!(exception_finding.class, ExposureClass::WeaklyExposed);
        assert_eq!(
            missing_discriminator_values(&exception_finding),
            vec!["raises ValueError matching \"positive required\""]
        );
        assert!(
            exception_finding
                .recommended_next_step
                .as_deref()
                .is_some_and(|step| step.contains("exception assertion"))
        );

        let field_finding = classify_change(
            Path::new("src/invoice.py"),
            3,
            "        self.status = \"paid\"",
            &extract_owners(
                Path::new("src/invoice.py"),
                "class Invoice:\n    def mark_paid(self):\n        self.status = \"paid\"\n",
            ),
            &extract_tests(
                Path::new("tests/test_invoice.py"),
                "from src.invoice import Invoice\n\n\
                 def test_mark_paid_smoke():\n    invoice = Invoice()\n    invoice.mark_paid()\n    assert invoice\n",
            ),
        )
        .ok_or_else(|| "field-value change should classify".to_string())?;
        assert_eq!(field_finding.class, ExposureClass::WeaklyExposed);
        assert_eq!(
            missing_discriminator_values(&field_finding),
            vec!["self.status == \"paid\""]
        );
        assert!(
            field_finding
                .recommended_next_step
                .as_deref()
                .is_some_and(|step| step.contains("field/object assertion"))
        );

        let output_finding = classify_change(
            Path::new("src/notifications.py"),
            5,
            "    logger.warning(\"coupon expired\")",
            &extract_owners(
                Path::new("src/notifications.py"),
                "import logging\n\nlogger = logging.getLogger(__name__)\n\ndef warn_coupon():\n    logger.warning(\"coupon expired\")\n",
            ),
            &extract_tests(
                Path::new("tests/test_notifications.py"),
                "from src.notifications import warn_coupon\n\n\
                 def test_warn_coupon_smoke(caplog):\n    warn_coupon()\n    assert caplog.text\n",
            ),
        )
        .ok_or_else(|| "output/log change should classify".to_string())?;
        assert_eq!(output_finding.class, ExposureClass::WeaklyExposed);
        assert_eq!(
            missing_discriminator_values(&output_finding),
            vec!["log contains \"coupon expired\""]
        );
        assert!(
            output_finding
                .recommended_next_step
                .as_deref()
                .is_some_and(|step| step.contains("output/log/call-effect assertion"))
        );

        Ok(())
    }

    #[test]
    fn classify_change_emits_python_repair_placement_and_verify_command() -> Result<(), String> {
        let pytest_finding = classify_change(
            Path::new("src/pricing.py"),
            2,
            "    if amount >= threshold:",
            &extract_owners(
                Path::new("src/pricing.py"),
                "def calculate_discount(amount, threshold):\n    if amount >= threshold:\n        return amount - 10\n    return amount\n",
            ),
            &extract_tests(
                Path::new("tests/test_pricing.py"),
                "from src.pricing import calculate_discount\n\n\
                 def test_calculate_discount_smoke():\n    result = calculate_discount(150, 100)\n    assert result\n",
            ),
        )
        .ok_or_else(|| "pytest boundary change should classify".to_string())?;
        assert_eq!(pytest_finding.class, ExposureClass::WeaklyExposed);
        assert_eq!(
            evidence_value(&pytest_finding, "suggested_repair_action: "),
            Some("strengthen_existing_test")
        );
        assert_eq!(
            evidence_value(&pytest_finding, "suggested_test_file: "),
            Some("tests/test_pricing.py")
        );
        assert_eq!(
            evidence_value(&pytest_finding, "suggested_test_name: "),
            Some("test_calculate_discount_smoke")
        );
        assert_eq!(
            evidence_value(&pytest_finding, "suggested_test_node_id: "),
            Some("tests/test_pricing.py::test_calculate_discount_smoke")
        );
        assert_eq!(
            evidence_value(&pytest_finding, "suggested_verify_command: "),
            Some("pytest tests/test_pricing.py::test_calculate_discount_smoke")
        );
        assert_eq!(
            evidence_value(&pytest_finding, "suggested_verify_command_confidence: "),
            Some("high")
        );

        let unittest_finding = classify_change(
            Path::new("src/validation.py"),
            3,
            "        raise ValueError(\"positive required\")",
            &extract_owners(
                Path::new("src/validation.py"),
                "def require_positive(value):\n    if value <= 0:\n        raise ValueError(\"positive required\")\n    return value\n",
            ),
            &extract_tests(
                Path::new("tests/test_validation.py"),
                "import unittest\nfrom src.validation import require_positive\n\n\
                 class TestValidation(unittest.TestCase):\n    def test_rejects_zero_value(self):\n        with self.assertRaises(ValueError):\n            require_positive(0)\n",
            ),
        )
        .ok_or_else(|| "unittest exception change should classify".to_string())?;
        assert_eq!(unittest_finding.class, ExposureClass::WeaklyExposed);
        assert_eq!(
            evidence_value(&unittest_finding, "suggested_repair_action: "),
            Some("strengthen_existing_test")
        );
        assert_eq!(
            evidence_value(&unittest_finding, "suggested_test_file: "),
            Some("tests/test_validation.py")
        );
        assert_eq!(
            evidence_value(&unittest_finding, "suggested_test_name: "),
            Some("test_rejects_zero_value")
        );
        assert_eq!(
            evidence_value(&unittest_finding, "suggested_test_node_id: "),
            None
        );
        assert_eq!(
            evidence_value(&unittest_finding, "suggested_verify_command: "),
            Some("python -m unittest tests.test_validation.TestValidation.test_rejects_zero_value")
        );
        assert_eq!(
            evidence_value(&unittest_finding, "suggested_verify_command_confidence: "),
            Some("high")
        );

        Ok(())
    }

    #[test]
    fn classify_change_suppresses_repair_guidance_for_non_actionable_python_cases()
    -> Result<(), String> {
        let exposed = classify_change(
            Path::new("src/pricing.py"),
            2,
            "    if amount >= threshold:",
            &extract_owners(
                Path::new("src/pricing.py"),
                "def apply_discount(amount, threshold):\n    if amount >= threshold:\n        return amount - 10\n    return amount\n",
            ),
            &extract_tests(
                Path::new("tests/test_pricing.py"),
                "from src.pricing import apply_discount\n\n\
                 def test_apply_discount_boundary():\n    assert apply_discount(100, 100) == 90\n",
            ),
        )
        .ok_or_else(|| "strong predicate change should classify".to_string())?;
        assert_eq!(exposed.class, ExposureClass::Exposed);
        assert!(exposed.activation.missing_discriminators.is_empty());
        assert!(
            exposed
                .recommended_next_step
                .as_deref()
                .is_some_and(|step| step.contains("observed under a strong oracle"))
        );

        let static_unknown = classify_change(
            Path::new("src/service.py"),
            2,
            "    return getattr(client, name)()",
            &extract_owners(
                Path::new("src/service.py"),
                "def call_named(client, name):\n    return getattr(client, name)()\n",
            ),
            &extract_tests(
                Path::new("tests/test_service.py"),
                "from src.service import call_named\n\n\
                 def test_call_named_dispatches():\n    assert call_named(client, \"total\") == 10\n",
            ),
        )
        .ok_or_else(|| "dynamic dispatch change should classify".to_string())?;
        assert_eq!(static_unknown.class, ExposureClass::StaticUnknown);
        assert!(static_unknown.activation.missing_discriminators.is_empty());
        assert!(static_unknown.recommended_next_step.is_none());

        let no_static_path = classify_change(
            Path::new("src/pricing.py"),
            2,
            "    return amount - 10",
            &extract_owners(
                Path::new("src/pricing.py"),
                "def apply_discount(amount):\n    return amount - 10\n",
            ),
            &extract_tests(
                Path::new("tests/test_other.py"),
                "def test_other():\n    other_behavior()\n",
            ),
        )
        .ok_or_else(|| "unrelated return change should classify".to_string())?;
        assert_eq!(no_static_path.class, ExposureClass::NoStaticPath);
        assert!(no_static_path.activation.missing_discriminators.is_empty());
        assert!(no_static_path.recommended_next_step.is_none());

        Ok(())
    }

    #[test]
    fn classify_change_populates_language_qualified_owner_ids() -> Result<(), String> {
        let function_owners = extract_owners(
            Path::new("src/pricing.py"),
            "def apply_discount(amount):\n    return amount - 10\n",
        );
        let function = classify_change(
            Path::new("src/pricing.py"),
            2,
            "    return amount - 10",
            &function_owners,
            &[],
        )
        .ok_or_else(|| "function changed line should classify".to_string())?;
        assert_eq!(
            function.probe.owner.as_ref().map(ToString::to_string),
            Some("python:src/pricing.py::apply_discount".to_string())
        );

        let method_owners = extract_owners(
            Path::new("src/cart.py"),
            "class Cart:\n    def apply_discount(self, amount):\n        return amount - 10\n",
        );
        let method = classify_change(
            Path::new("src/cart.py"),
            3,
            "        return amount - 10",
            &method_owners,
            &[],
        )
        .ok_or_else(|| "method changed line should classify".to_string())?;
        assert_eq!(
            method.probe.owner.as_ref().map(ToString::to_string),
            Some("python:src/cart.py::Cart.apply_discount".to_string())
        );

        let class_owners = extract_owners(
            Path::new("src/models.py"),
            "class Invoice:\n    status = \"pending\"\n\n    def mark_paid(self):\n        return \"paid\"\n",
        );
        let class_body = classify_change(
            Path::new("src/models.py"),
            2,
            "    status = \"pending\"",
            &class_owners,
            &[],
        )
        .ok_or_else(|| "class body changed line should classify".to_string())?;
        assert_eq!(
            class_body.probe.owner.as_ref().map(ToString::to_string),
            Some("python:src/models.py::Invoice".to_string())
        );
        assert_eq!(class_body.owner_kind, None);
        assert!(
            class_body
                .evidence
                .iter()
                .any(|entry| entry == "owner_kind: class")
        );

        let module_owners = extract_owners(
            Path::new("src/settings.py"),
            "DISCOUNT_THRESHOLD = 100\n\ndef threshold():\n    return DISCOUNT_THRESHOLD\n",
        );
        let module = classify_change(
            Path::new("src/settings.py"),
            1,
            "DISCOUNT_THRESHOLD = 100",
            &module_owners,
            &[],
        )
        .ok_or_else(|| "module changed line should classify".to_string())?;
        assert_eq!(
            module.probe.owner.as_ref().map(ToString::to_string),
            Some("python:src/settings.py::<module>".to_string())
        );
        assert_eq!(module.owner_kind, Some(OwnerKind::ModuleFunction));
        Ok(())
    }

    #[test]
    fn python_owner_id_is_stable_when_owner_line_moves() -> Result<(), String> {
        let before = extract_owners(
            Path::new("src/pricing.py"),
            "def apply_discount(amount):\n    return amount - 10\n",
        );
        let after = extract_owners(
            Path::new("src/pricing.py"),
            "\n\n\ndef apply_discount(amount):\n    return amount - 10\n",
        );
        let before_finding = classify_change(
            Path::new("src/pricing.py"),
            2,
            "    return amount - 10",
            &before,
            &[],
        )
        .ok_or_else(|| "before owner should classify".to_string())?;
        let after_finding = classify_change(
            Path::new("src/pricing.py"),
            5,
            "    return amount - 10",
            &after,
            &[],
        )
        .ok_or_else(|| "after owner should classify".to_string())?;

        assert_eq!(before_finding.probe.owner, after_finding.probe.owner);
        // With content-addressed ids, moving the owner to a different line
        // (without changing the expression) must NOT change the probe id.
        assert_eq!(
            before_finding.probe.id, after_finding.probe.id,
            "content-addressed id must be stable across line movement"
        );
        Ok(())
    }

    #[test]
    fn find_related_tests_matches_import_alias_call() {
        let owners = extract_owners(
            Path::new("src/pricing.py"),
            "def apply_discount(amount):\n    return amount - 10\n",
        );
        let tests = extract_tests(
            Path::new("tests/test_alias_pricing.py"),
            "from src.pricing import apply_discount as discount\n\ndef test_discount_alias():\n    assert discount(100) == 90\n",
        );

        let related = find_related_tests(&owners[0], &tests);

        assert_eq!(related.len(), 1);
        assert_eq!(related[0].name, "test_discount_alias");
        assert_eq!(related[0].oracle_kind, OracleKind::ExactValue);
        assert_eq!(related[0].oracle_strength, OracleStrength::Strong);
    }

    #[test]
    fn related_test_matching_ignores_object_method_calls_for_free_functions() {
        let owners = extract_owners(
            Path::new("src/pricing.py"),
            "def apply_discount(amount):\n    return amount - 10\n",
        );
        let tests = extract_tests(
            Path::new("tests/test_order_methods.py"),
            "def test_order_discount_method():\n    assert order.apply_discount(100) == 90\n",
        );

        let related = related_test_candidates(&owners[0], &tests);

        assert!(related.is_empty());
    }

    #[test]
    fn related_test_matching_accepts_module_alias_attribute_calls() {
        let owners = extract_owners(
            Path::new("src/pricing.py"),
            "def apply_discount(amount):\n    return amount - 10\n",
        );
        let tests = extract_tests(
            Path::new("tests/test_module_alias_pricing.py"),
            "import src.pricing as pricing\n\ndef test_discount_module_alias():\n    assert pricing.apply_discount(100) == 90\n",
        );

        let related = related_test_candidates(&owners[0], &tests);

        assert_eq!(related.len(), 1);
        assert_eq!(related[0].relation, PythonRelationKind::ImportAliasCall);
    }

    #[test]
    fn related_test_matching_keeps_method_owner_object_calls() {
        let owners = extract_owners(
            Path::new("src/cart.py"),
            "class Cart:\n    def apply_discount(self, amount):\n        return amount - 10\n",
        );
        let tests = extract_tests(
            Path::new("tests/test_cart.py"),
            "def test_cart_discount_method():\n    assert cart.apply_discount(100) == 90\n",
        );

        let related = related_test_candidates(&owners[0], &tests);

        assert_eq!(related.len(), 1);
        assert_eq!(related[0].relation, PythonRelationKind::SyntacticCall);
    }

    #[test]
    fn classify_change_uses_import_alias_call_as_strong_relation() -> Result<(), String> {
        let owners = extract_owners(
            Path::new("src/tax.py"),
            "def apply_tax(amount):\n    return amount + 2\n",
        );
        let tests = extract_tests(
            Path::new("tests/test_checkout_tax.py"),
            "from src.tax import apply_tax as taxed\n\ndef test_checkout_tax_alias_import():\n    assert taxed(10) == 12\n",
        );

        let Some(finding) = classify_change(
            Path::new("src/tax.py"),
            2,
            "    return amount + 2",
            &owners,
            &tests,
        ) else {
            return Err("changed line inside owner should classify".to_string());
        };

        assert_eq!(finding.class, ExposureClass::Exposed);
        assert!(finding.evidence.iter().any(|entry| entry
            == "related_test_relation: import_alias_call (test_checkout_tax_alias_import)"));
        Ok(())
    }

    #[test]
    fn classify_change_uses_same_stem_test_as_weak_proximity() -> Result<(), String> {
        let owners = extract_owners(
            Path::new("src/pricing.py"),
            "def apply_discount(amount):\n    return amount - 10\n",
        );
        let tests = extract_tests(
            Path::new("tests/test_pricing.py"),
            "def test_boundary_documented_elsewhere():\n    assert 90 == 90\n",
        );

        let Some(finding) = classify_change(
            Path::new("src/pricing.py"),
            2,
            "    return amount - 10",
            &owners,
            &tests,
        ) else {
            return Err("changed line inside owner should classify".to_string());
        };

        assert_eq!(finding.class, ExposureClass::WeaklyExposed);
        assert_eq!(finding.related_tests.len(), 1);
        assert_eq!(finding.related_tests[0].oracle_kind, OracleKind::Unknown);
        assert!(finding.evidence.iter().any(|entry| entry
            == "related_test_relation: same_stem (test_boundary_documented_elsewhere)"));
        Ok(())
    }

    #[test]
    fn same_stem_relation_accepts_suffix_and_orders_after_direct_calls() {
        let owners = extract_owners(
            Path::new("src/pricing.py"),
            "def apply_discount(amount):\n    return amount - 10\n",
        );
        let mut tests = extract_tests(
            Path::new("tests/pricing_test.py"),
            "def test_same_stem_only():\n    assert 90 == 90\n",
        );
        tests.extend(extract_tests(
            Path::new("tests/test_checkout.py"),
            "def test_direct_call():\n    assert apply_discount(100) == 90\n",
        ));

        let related = related_test_candidates(&owners[0], &tests);

        assert_eq!(normalize_test_stem("pricing_test"), "pricing");
        assert_eq!(related.len(), 2);
        assert_eq!(related[0].relation, PythonRelationKind::SyntacticCall);
        assert_eq!(related[1].relation, PythonRelationKind::SameStem);
    }

    #[test]
    fn static_limit_detection_covers_python_preview_limit_kinds() {
        let imported_owner = extract_owners(
            Path::new("src/service.py"),
            "from external.client import remote_total\n\ndef total():\n    return remote_total()\n",
        )
        .remove(0);
        let decorated_owner = extract_owners(
            Path::new("src/service.py"),
            "@retry(times=3)\ndef total():\n    return 1\n",
        )
        .remove(0);
        let plain_owner =
            extract_owners(Path::new("src/service.py"), "def total():\n    return 1\n").remove(0);
        let tests = extract_tests(
            Path::new("tests/test_service.py"),
            "from unittest.mock import patch\nfrom src.service import total\n\n@patch(\"src.service.remote_total\")\ndef test_total(mock_remote):\n    assert total() == 1\n",
        );
        let candidates = related_test_candidates(&plain_owner, &tests);
        let monkeypatch_tests = extract_tests(
            Path::new("tests/test_service.py"),
            "from src.service import total\n\ndef test_total(monkeypatch):\n    monkeypatch.setattr(\"src.service.remote_total\", lambda: 1)\n    assert total() == 1\n",
        );
        let monkeypatch_candidates = related_test_candidates(&plain_owner, &monkeypatch_tests);
        let property_based_tests = extract_tests(
            Path::new("tests/test_service.py"),
            "from hypothesis import given, strategies as st\nfrom src.service import total\n\n@given(st.integers())\ndef test_total_property_based(value):\n    assert total(value) >= 0\n",
        );
        let property_based_candidates =
            related_test_candidates(&plain_owner, &property_based_tests);
        let property_based_with_exact_tests = {
            let mut tests = property_based_tests.clone();
            tests.extend(extract_tests(
                Path::new("tests/test_service_exact.py"),
                "from src.service import total\n\ndef test_total_exact():\n    assert total(1) == 1\n",
            ));
            tests
        };
        let property_based_with_exact_candidates =
            related_test_candidates(&plain_owner, &property_based_with_exact_tests);
        let unresolved_fixture_tests = extract_tests(
            Path::new("tests/test_service.py"),
            "from src.service import total\n\ndef test_total_fixture_case(case):\n    assert total(case.value) == case.expected\n",
        );
        let unresolved_fixture_candidates =
            related_test_candidates(&plain_owner, &unresolved_fixture_tests);
        let unresolved_fixture_with_exact_tests = {
            let mut tests = unresolved_fixture_tests.clone();
            tests.extend(extract_tests(
                Path::new("tests/test_service_exact.py"),
                "from src.service import total\n\ndef test_total_exact():\n    assert total(1) == 1\n",
            ));
            tests
        };
        let unresolved_fixture_with_exact_candidates =
            related_test_candidates(&plain_owner, &unresolved_fixture_with_exact_tests);
        let parametrized_tests = extract_tests(
            Path::new("tests/test_service.py"),
            "import pytest\nfrom src.service import total\n\n@pytest.mark.parametrize(\"value, expected\", [(1, 1)])\ndef test_total_parametrized(value, expected):\n    assert total(value) == expected\n",
        );
        let parametrized_candidates = related_test_candidates(&plain_owner, &parametrized_tests);
        let property_based_with_same_test_exact_tests = extract_tests(
            Path::new("tests/test_service.py"),
            "from hypothesis import given, strategies as st\nfrom src.service import total\n\n@given(st.integers())\ndef test_total_property_based(value):\n    assert total(1) == 1\n",
        );
        let property_based_with_same_test_exact_candidates =
            related_test_candidates(&plain_owner, &property_based_with_same_test_exact_tests);
        let opaque_helper_tests = extract_tests(
            Path::new("tests/test_service.py"),
            "from src.service import total\n\ndef test_total_custom_helper():\n    result = total()\n    assert_total_result(result)\n",
        );
        let opaque_helper_candidates = related_test_candidates(&plain_owner, &opaque_helper_tests);
        let opaque_helper_with_exact_tests = extract_tests(
            Path::new("tests/test_service.py"),
            "from src.service import total\n\ndef test_total_custom_helper_and_exact():\n    result = total()\n    assert_total_result(result)\n    assert result == 1\n",
        );
        let opaque_helper_with_exact_candidates =
            related_test_candidates(&plain_owner, &opaque_helper_with_exact_tests);

        assert_eq!(
            static_limit_for_change("    return getattr(client, name)()", &plain_owner, &[])
                .map(|limit| limit.kind),
            Some(StaticLimitKind::DynamicDispatch)
        );
        assert_eq!(
            static_limit_for_change("    return type(\"Dynamic\", (), {})", &plain_owner, &[])
                .map(|limit| limit.kind),
            Some(StaticLimitKind::Metaprogramming)
        );
        assert_eq!(
            static_limit_for_change("    return 1", &decorated_owner, &[]).map(|limit| limit.kind),
            Some(StaticLimitKind::DecoratorIndirection)
        );
        assert_eq!(
            static_limit_for_change("    return total()", &plain_owner, &candidates)
                .map(|limit| limit.kind),
            Some(StaticLimitKind::MockedModule)
        );
        assert_eq!(
            static_limit_for_change("    return total()", &plain_owner, &monkeypatch_candidates)
                .map(|limit| limit.kind),
            Some(StaticLimitKind::MockedModule)
        );
        assert_eq!(
            static_limit_for_change("    return 1", &plain_owner, &property_based_candidates)
                .map(|limit| limit.kind),
            Some(StaticLimitKind::PropertyBasedTest)
        );
        assert_eq!(
            static_limit_for_change(
                "    return 1",
                &plain_owner,
                &property_based_with_exact_candidates
            )
            .map(|limit| limit.kind),
            Some(StaticLimitKind::PropertyBasedTest)
        );
        assert_eq!(
            static_limit_for_change(
                "    return 1",
                &plain_owner,
                &property_based_with_same_test_exact_candidates
            )
            .map(|limit| limit.kind),
            None
        );
        assert_eq!(
            static_limit_for_change("    return 1", &plain_owner, &unresolved_fixture_candidates)
                .map(|limit| limit.kind),
            Some(StaticLimitKind::UnresolvedPytestFixture)
        );
        assert_eq!(
            static_limit_for_change(
                "    return 1",
                &plain_owner,
                &unresolved_fixture_with_exact_candidates
            )
            .map(|limit| limit.kind),
            None
        );
        assert_eq!(
            static_limit_for_change("    return 1", &plain_owner, &parametrized_candidates)
                .map(|limit| limit.kind),
            None
        );
        assert_eq!(
            static_limit_for_change("    return 1", &plain_owner, &opaque_helper_candidates)
                .map(|limit| limit.kind),
            Some(StaticLimitKind::OpaqueCustomAssertionHelper)
        );
        assert_eq!(
            static_limit_for_change(
                "    return 1",
                &plain_owner,
                &opaque_helper_with_exact_candidates
            )
            .map(|limit| limit.kind),
            None
        );
        assert_eq!(
            static_limit_for_change("    return remote_total()", &imported_owner, &[])
                .map(|limit| limit.kind),
            Some(StaticLimitKind::MissingImportGraph)
        );
        assert_eq!(
            static_limit_for_change("    return lambda value: value + 1", &plain_owner, &[])
                .map(|limit| limit.kind),
            Some(StaticLimitKind::UnsupportedSyntax)
        );
        let mock_owner = extract_owners(
            Path::new("src/callbacks.py"),
            "from unittest.mock import MagicMock\n\ndef recording_callback():\n    callback = MagicMock(name=\"receipt\")\n    return callback\n",
        )
        .remove(0);
        assert_eq!(
            static_limit_for_change(
                "    callback = MagicMock(name=\"receipt.sent\")",
                &mock_owner,
                &[]
            )
            .map(|limit| limit.kind),
            None
        );
    }

    #[test]
    fn classify_change_static_limit_fails_closed_even_with_strong_oracle() -> Result<(), String> {
        let owners = extract_owners(
            Path::new("src/service.py"),
            "def call_named(client, name):\n    return getattr(client, name)()\n",
        );
        let tests = extract_tests(
            Path::new("tests/test_service.py"),
            "from src.service import call_named\n\ndef test_call_named_dispatches():\n    assert call_named(client, \"total\") == 10\n",
        );

        let Some(finding) = classify_change(
            Path::new("src/service.py"),
            2,
            "    return getattr(client, name)()",
            &owners,
            &tests,
        ) else {
            return Err("changed line inside owner should classify".to_string());
        };

        assert_eq!(finding.class, ExposureClass::StaticUnknown);
        assert_eq!(
            finding.static_limit_kind,
            Some(StaticLimitKind::DynamicDispatch)
        );
        assert_eq!(
            finding.stop_reasons,
            vec![StopReason::DynamicDispatchUnresolved]
        );
        assert!(finding.recommended_next_step.is_none());
        assert!(finding.canonical_gap.is_none());
        assert_eq!(finding.ripr.infect.state, StageState::Unknown);
        assert_eq!(finding.ripr.propagate.state, StageState::Unknown);
        assert_eq!(finding.ripr.reveal.discriminate.state, StageState::Unknown);
        assert!(
            finding
                .evidence
                .iter()
                .any(|entry| entry.starts_with("static_limit dynamic_dispatch:"))
        );
        assert!(
            finding
                .missing
                .iter()
                .any(|entry| entry.contains("Static limit `dynamic_dispatch`"))
        );
        Ok(())
    }

    #[test]
    fn classify_change_property_based_test_fails_closed() -> Result<(), String> {
        let owners = extract_owners(
            Path::new("src/pricing.py"),
            "def apply_discount(amount, threshold):\n    if amount >= threshold:\n        return amount - 10\n    return amount\n",
        );
        let tests = extract_tests(
            Path::new("tests/test_pricing.py"),
            "from hypothesis import given, strategies as st\nfrom src.pricing import apply_discount\n\n@given(st.integers(min_value=0), st.integers(min_value=0))\ndef test_apply_discount_property(amount, threshold):\n    assert apply_discount(amount, threshold) <= amount\n",
        );

        let Some(finding) = classify_change(
            Path::new("src/pricing.py"),
            2,
            "    if amount >= threshold:",
            &owners,
            &tests,
        ) else {
            return Err("changed predicate inside owner should classify".to_string());
        };

        assert_eq!(finding.class, ExposureClass::StaticUnknown);
        assert_eq!(
            finding.static_limit_kind,
            Some(StaticLimitKind::PropertyBasedTest)
        );
        assert_eq!(finding.stop_reasons, vec![StopReason::StaticProbeUnknown]);
        assert!(finding.canonical_gap.is_none());
        assert!(finding.recommended_next_step.is_none());
        assert!(finding.activation.missing_discriminators.is_empty());
        assert_eq!(finding.ripr.reveal.discriminate.state, StageState::Unknown);
        assert!(
            finding
                .evidence
                .iter()
                .any(|entry| entry.starts_with("static_limit property_based_test:"))
        );
        assert!(
            finding
                .missing
                .iter()
                .any(|entry| entry.contains("Static limit `property_based_test`"))
        );
        Ok(())
    }

    #[test]
    fn classify_change_unresolved_pytest_fixture_fails_closed() -> Result<(), String> {
        let owners = extract_owners(
            Path::new("src/pricing.py"),
            "def apply_discount(amount, threshold):\n    if amount >= threshold:\n        return amount - 10\n    return amount\n",
        );
        let tests = extract_tests(
            Path::new("tests/test_pricing.py"),
            "from src.pricing import apply_discount\n\ndef test_apply_discount_fixture_case(discount_case):\n    assert apply_discount(discount_case.amount, discount_case.threshold) == discount_case.expected\n",
        );

        let Some(finding) = classify_change(
            Path::new("src/pricing.py"),
            2,
            "    if amount >= threshold:",
            &owners,
            &tests,
        ) else {
            return Err("changed predicate inside owner should classify".to_string());
        };

        assert_eq!(finding.class, ExposureClass::StaticUnknown);
        assert_eq!(
            finding.static_limit_kind,
            Some(StaticLimitKind::UnresolvedPytestFixture)
        );
        assert_eq!(finding.stop_reasons, vec![StopReason::StaticProbeUnknown]);
        assert!(finding.canonical_gap.is_none());
        assert!(finding.recommended_next_step.is_none());
        assert!(finding.activation.missing_discriminators.is_empty());
        assert_eq!(finding.ripr.reveal.discriminate.state, StageState::Unknown);
        assert!(
            finding
                .evidence
                .iter()
                .any(|entry| entry.starts_with("static_limit unresolved_pytest_fixture:"))
        );
        assert!(
            finding
                .missing
                .iter()
                .any(|entry| entry.contains("Static limit `unresolved_pytest_fixture`"))
        );
        Ok(())
    }

    #[test]
    fn classify_change_opaque_custom_assertion_helper_fails_closed() -> Result<(), String> {
        let owners = extract_owners(
            Path::new("src/pricing.py"),
            "def apply_discount(amount, threshold):\n    if amount >= threshold:\n        return amount - 10\n    return amount\n",
        );
        let tests = extract_tests(
            Path::new("tests/test_pricing.py"),
            "from src.pricing import apply_discount\n\ndef assert_discounted(result):\n    assert result < 100\n\ndef test_apply_discount_custom_helper():\n    result = apply_discount(100, 50)\n    assert_discounted(result)\n",
        );

        let Some(finding) = classify_change(
            Path::new("src/pricing.py"),
            2,
            "    if amount >= threshold:",
            &owners,
            &tests,
        ) else {
            return Err("changed predicate inside owner should classify".to_string());
        };

        assert_eq!(finding.class, ExposureClass::StaticUnknown);
        assert_eq!(
            finding.static_limit_kind,
            Some(StaticLimitKind::OpaqueCustomAssertionHelper)
        );
        assert_eq!(finding.stop_reasons, vec![StopReason::StaticProbeUnknown]);
        assert!(finding.canonical_gap.is_none());
        assert!(finding.recommended_next_step.is_none());
        assert!(finding.activation.missing_discriminators.is_empty());
        assert_eq!(finding.ripr.reveal.discriminate.state, StageState::Unknown);
        assert!(
            finding
                .evidence
                .iter()
                .any(|entry| entry.starts_with("static_limit opaque_custom_assertion_helper:"))
        );
        assert!(
            finding
                .missing
                .iter()
                .any(|entry| entry.contains("Static limit `opaque_custom_assertion_helper`"))
        );
        Ok(())
    }

    #[test]
    fn classify_change_static_limit_omits_activation_discriminators() -> Result<(), String> {
        let owners = extract_owners(
            Path::new("src/service.py"),
            "def has_named_value(client, name, threshold):\n    if getattr(client, name) >= threshold:\n        return True\n    return False\n",
        );
        let tests = extract_tests(
            Path::new("tests/test_service.py"),
            "from src.service import has_named_value\n\ndef test_has_named_value():\n    assert has_named_value(client, \"total\", 10) is True\n",
        );

        let Some(finding) = classify_change(
            Path::new("src/service.py"),
            2,
            "    if getattr(client, name) >= threshold:",
            &owners,
            &tests,
        ) else {
            return Err("changed predicate inside owner should classify".to_string());
        };

        assert_eq!(finding.class, ExposureClass::StaticUnknown);
        assert_eq!(
            finding.static_limit_kind,
            Some(StaticLimitKind::DynamicDispatch)
        );
        assert!(finding.flow_sinks.is_empty());
        assert!(finding.activation.missing_discriminators.is_empty());
        assert!(
            finding
                .evidence
                .iter()
                .all(|entry| !entry.starts_with("missing_discriminator:"))
        );
        Ok(())
    }

    #[test]
    fn classify_change_returns_no_static_path_without_related_test() -> Result<(), String> {
        let owners = extract_owners(
            Path::new("src/pricing.py"),
            "def apply_discount(amount):\n    return amount - 10\n",
        );
        let tests = extract_tests(
            Path::new("tests/test_other.py"),
            "def test_other():\n    other_behavior()\n",
        );

        let Some(finding) = classify_change(
            Path::new("src/pricing.py"),
            2,
            "    return amount - 10",
            &owners,
            &tests,
        ) else {
            return Err("changed line inside owner should classify".to_string());
        };

        assert_eq!(finding.class, ExposureClass::NoStaticPath);
        assert_eq!(finding.owner_kind, Some(OwnerKind::Function));
        assert!(finding.related_tests.is_empty());
        assert_eq!(finding.ripr.reach.state, StageState::No);
        assert_eq!(finding.ripr.infect.state, StageState::Yes);
        assert_eq!(finding.ripr.propagate.state, StageState::Yes);
        assert!(finding.recommended_next_step.is_none());
        Ok(())
    }

    #[test]
    fn classify_change_ignores_unrelated_text_mentions() -> Result<(), String> {
        let owners = extract_owners(
            Path::new("src/pricing.py"),
            "def apply_discount(amount):\n    return amount - 10\n",
        );
        let tests = extract_tests(
            Path::new("tests/test_docs.py"),
            "def test_docs_mentions_owner():\n    assert \"apply_discount(\" in \"apply_discount(\"\n",
        );

        let Some(finding) = classify_change(
            Path::new("src/pricing.py"),
            2,
            "    return amount - 10",
            &owners,
            &tests,
        ) else {
            return Err("changed line inside owner should classify".to_string());
        };

        assert_eq!(finding.class, ExposureClass::NoStaticPath);
        assert!(finding.related_tests.is_empty());
        Ok(())
    }

    #[test]
    fn analyze_diff_returns_zero_findings_and_counts_accepted_files() -> Result<(), String> {
        let adapter = PythonAdapter;
        let options = AnalysisOptions {
            root: PathBuf::from("."),
            base: None,
            diff_file: None,
            mode: crate::analysis::AnalysisMode::Draft,
            include_unchanged_tests: false,
            resolve_tsconfig_paths: false,
            perl_facts_path: None,
            git_timeout: None,
            git_candidate: None,
            production_like_targets: Default::default(),
            test_harnesses: Vec::new(),
            resolved_subject_identity: None,
        };
        let policy = OraclePolicy::default();
        let changed_files = vec![
            changed("scripts/run.py"),
            changed("src/lib.rs"),
            changed("docs/README.md"),
            changed("src/util.py"),
            changed("src/index.ts"),
        ];
        let result = adapter.analyze_diff(&options, &policy, &changed_files)?;
        assert!(result.findings.is_empty());
        assert_eq!(result.changed_files, 2);
        Ok(())
    }

    #[test]
    fn analyze_repo_returns_empty_scaffold() -> Result<(), String> {
        let adapter = PythonAdapter;
        let options = AnalysisOptions {
            root: PathBuf::from("."),
            base: None,
            diff_file: None,
            mode: crate::analysis::AnalysisMode::Deep,
            include_unchanged_tests: false,
            resolve_tsconfig_paths: false,
            perl_facts_path: None,
            git_timeout: None,
            git_candidate: None,
            production_like_targets: Default::default(),
            test_harnesses: Vec::new(),
            resolved_subject_identity: None,
        };
        let policy = OraclePolicy::default();
        let result = adapter.analyze_repo(&options, &policy)?;
        assert!(result.findings.is_empty());
        assert_eq!(result.production_files, 0);
        Ok(())
    }
}
