use crate::app::{CHECK_OUTPUT_SCHEMA_VERSION, CheckInput};
use crate::core_error::CoreError;
use crate::output::json::render_pretty_with_newline;
use serde_json::json;

const DIFF_SCOPE_OVERSIZED_PREFIX: &str = "diff_scope_oversized:";
const DIFF_SCOPE_REPAIR_ROUTE: &str = "analysis/diff-scope-budget";
const REPO_SCOPE_OVERSIZED_PREFIX: &str = "repo_scope_oversized:";
const REPO_SCOPE_REPAIR_ROUTE: &str = "analysis/repo-scope-budget";

pub(crate) fn render_diff_scope_limited_check_json(
    input: &CheckInput,
    error: &str,
) -> Result<Option<String>, String> {
    if !is_diff_scope_oversized(error) && !is_repo_scope_oversized(error) {
        return Ok(None);
    }

    // Same non-consumable envelope for both scope guards (#2109 review):
    // only the scope identity differs.
    let (scope, run_status, basis, repair_route) = if is_repo_scope_oversized(error) {
        (
            "repo",
            "repo_scope_oversized",
            "rust_repo_scope_budget",
            REPO_SCOPE_REPAIR_ROUTE,
        )
    } else {
        (
            "diff",
            "diff_scope_oversized",
            "rust_diff_scope_budget",
            DIFF_SCOPE_REPAIR_ROUTE,
        )
    };

    let mut value = json!({
        "schema_version": CHECK_OUTPUT_SCHEMA_VERSION,
        "tool": "ripr",
        "mode": input.mode.as_str(),
        "root": input.root.display().to_string(),
        "summary": {
            "changed_rust_files": 0,
            "probes": 0,
            "findings": 0,
            "exposed": 0,
            "weakly_exposed": 0,
            "reachable_unrevealed": 0,
            "no_static_path": 0,
            "infection_unknown": 0,
            "propagation_unknown": 0,
            "static_unknown": 0,
            "changed_files_by_language": []
        },
        "findings": [],
        "analysis_scope": {
            "scope": scope,
            "run_status": run_status,
            "basis": basis,
            "downstream_consumable": false,
            "limitation": run_status,
            "repair_route": repair_route
        },
        "run_limitations": [
            {
                "category": run_status,
                "run_status": run_status,
                "basis": basis,
                "downstream_consumable": false,
                "message": error,
                "repair_route": repair_route
            }
        ]
    });

    if let Some(base) = &input.base {
        value["base"] = json!(base);
    }

    render_pretty_with_newline(&value, "limited check").map(Some)
}

fn is_diff_scope_oversized(error: &str) -> bool {
    error.trim_start().starts_with(DIFF_SCOPE_OVERSIZED_PREFIX)
}

fn is_repo_scope_oversized(error: &str) -> bool {
    error.trim_start().starts_with(REPO_SCOPE_OVERSIZED_PREFIX)
}

/// Render the `check --json` refusal document for a failure that is not one
/// of the two scope guards (#6834).
///
/// Same envelope shape as the scope guards — same keys, same zeroed summary,
/// same non-consumable treatment — so a JSON consumer has one contract: the
/// typed identity selects `run_status`/`category`/`limitation` (and `basis`,
/// which names the limiting condition), and the human diagnostic rides in
/// `message`. The identity comes from [`CoreError::check_refusal`], matched
/// structurally; this renderer never inspects message text.
///
/// Callers try [`render_diff_scope_limited_check_json`] first, so scope-guard
/// bytes stay frozen and #4861 keeps owning those identities.
pub(crate) fn render_check_failure_json(
    input: &CheckInput,
    error: &CoreError,
) -> Result<String, String> {
    let refusal = error.check_refusal();
    let message = if refusal.redact_message {
        crate::config::config_error_summary(&error.to_string())
    } else {
        error.to_string()
    };
    let identity = refusal.identity;
    let repair_route = refusal.repair_route;

    let mut value = json!({
        "schema_version": CHECK_OUTPUT_SCHEMA_VERSION,
        "tool": "ripr",
        "mode": input.mode.as_str(),
        "root": input.root.display().to_string(),
        "summary": {
            "changed_rust_files": 0,
            "probes": 0,
            "findings": 0,
            "exposed": 0,
            "weakly_exposed": 0,
            "reachable_unrevealed": 0,
            "no_static_path": 0,
            "infection_unknown": 0,
            "propagation_unknown": 0,
            "static_unknown": 0,
            "changed_files_by_language": []
        },
        "findings": [],
        "analysis_scope": {
            "scope": "diff",
            "run_status": identity,
            "basis": identity,
            "downstream_consumable": false,
            "limitation": identity,
            "repair_route": repair_route
        },
        "run_limitations": [
            {
                "category": identity,
                "run_status": identity,
                "basis": identity,
                "downstream_consumable": false,
                "message": message,
                "repair_route": repair_route
            }
        ]
    });

    if let Some(base) = &input.base {
        value["base"] = json!(base);
    }

    render_pretty_with_newline(&value, "limited check")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{Mode, OutputFormat};
    use serde_json::Value;
    use std::path::PathBuf;

    fn input() -> CheckInput {
        CheckInput {
            root: PathBuf::from("."),
            base: Some("origin/main".to_string()),
            diff_file: Some(PathBuf::from("example.diff")),
            mode: Mode::Draft,
            format: OutputFormat::Json,
            include_unchanged_tests: true,
            perl_facts_path: None,
            suppression_policy: None,
            git_timeout: None,
            git_candidate: None,
        }
    }

    #[test]
    fn repo_scope_oversized_renders_the_same_non_consumable_envelope() -> Result<(), String> {
        // #2109 review: JSON consumers must get the named error and repair
        // route for the repo guard too, in the same envelope shape.
        let rendered = render_diff_scope_limited_check_json(
            &input(),
            "repo_scope_oversized: 900 indexed Rust files exceed the RIPR_MAX_REPO_INDEX_FILES limit (800); analysis was not run",
        )?;
        let Some(rendered) = rendered else {
            return Err("expected a limited artifact for the repo guard".to_string());
        };
        let value: Value = serde_json::from_str(&rendered)
            .map_err(|err| format!("parse limited artifact: {err}"))?;
        for (pointer, expected) in [
            ("/analysis_scope/scope", "repo"),
            ("/analysis_scope/run_status", "repo_scope_oversized"),
            ("/analysis_scope/basis", "rust_repo_scope_budget"),
            ("/analysis_scope/repair_route", "analysis/repo-scope-budget"),
            ("/run_limitations/0/category", "repo_scope_oversized"),
        ] {
            let actual = value.pointer(pointer).and_then(Value::as_str);
            if actual != Some(expected) {
                return Err(format!("{pointer}: expected {expected}, got {actual:?}"));
            }
        }
        if value.pointer("/analysis_scope/downstream_consumable") != Some(&Value::Bool(false)) {
            return Err("downstream_consumable must be false".to_string());
        }
        Ok(())
    }

    #[test]
    fn limited_artifact_summary_carries_empty_language_breakdown() -> Result<(), String> {
        // #2103 review: the limited artifact must keep the same summary
        // shape as every normal check output at the same schema version.
        let rendered = render_diff_scope_limited_check_json(
            &input(),
            "diff_scope_oversized: 900 indexed Rust files exceed the limit (800)",
        )?;
        let Some(rendered) = rendered else {
            return Err("expected a limited artifact for the oversized error".to_string());
        };
        let value: Value = serde_json::from_str(&rendered)
            .map_err(|err| format!("parse limited artifact: {err}"))?;
        let breakdown = value
            .pointer("/summary/changed_files_by_language")
            .and_then(Value::as_array);
        match breakdown {
            Some(entries) if entries.is_empty() => Ok(()),
            other => Err(format!(
                "expected summary.changed_files_by_language to be an empty array, got {other:?}"
            )),
        }
    }

    #[test]
    fn non_budget_error_does_not_render_limited_artifact() -> Result<(), String> {
        let rendered = render_diff_scope_limited_check_json(&input(), "git diff failed")?;

        assert!(
            rendered.is_none(),
            "non-budget error should not render limited JSON"
        );
        Ok(())
    }

    #[test]
    fn budget_error_renders_non_consumable_limited_artifact() -> Result<(), String> {
        let rendered = render_diff_scope_limited_check_json(
            &input(),
            "diff_scope_oversized: 3 changed Rust lines exceed the limit",
        )?
        .ok_or("expected limited artifact")?;
        let value: Value =
            serde_json::from_str(&rendered).map_err(|err| format!("parse JSON: {err}"))?;

        let cases = [
            (
                &value["schema_version"],
                Value::String(CHECK_OUTPUT_SCHEMA_VERSION.to_string()),
                "schema_version",
            ),
            (
                &value["analysis_scope"]["run_status"],
                Value::String("diff_scope_oversized".to_string()),
                "analysis_scope.run_status",
            ),
            (
                &value["analysis_scope"]["downstream_consumable"],
                Value::Bool(false),
                "analysis_scope.downstream_consumable",
            ),
            (
                &value["run_limitations"][0]["category"],
                Value::String("diff_scope_oversized".to_string()),
                "run_limitations[0].category",
            ),
            (
                &value["run_limitations"][0]["downstream_consumable"],
                Value::Bool(false),
                "run_limitations[0].downstream_consumable",
            ),
        ];
        for (actual, expected, label) in cases {
            assert_eq!(actual, &expected, "unexpected {label}");
        }
        assert_eq!(value["findings"].as_array().map(Vec::len), Some(0));
        Ok(())
    }

    #[test]
    fn budget_error_without_base_omits_base_and_keeps_limitation() -> Result<(), String> {
        let mut check_input = input();
        check_input.base = None;
        let message = "\n  diff_scope_oversized: 4 changed Rust lines exceed the limit";
        let rendered = render_diff_scope_limited_check_json(&check_input, message)?
            .ok_or("expected limited artifact with leading whitespace")?;
        let value: Value =
            serde_json::from_str(&rendered).map_err(|err| format!("parse JSON: {err}"))?;

        assert!(
            value.get("base").is_none(),
            "base should be omitted when absent: {value}"
        );
        let cases = [
            (
                &value["analysis_scope"]["repair_route"],
                Value::String(DIFF_SCOPE_REPAIR_ROUTE.to_string()),
                "analysis_scope.repair_route",
            ),
            (
                &value["run_limitations"][0]["message"],
                Value::String(message.to_string()),
                "run_limitations[0].message",
            ),
        ];
        for (actual, expected, label) in cases {
            assert_eq!(actual, &expected, "unexpected {label}");
        }
        Ok(())
    }

    #[test]
    fn refusal_envelope_names_each_identity_with_one_non_consumable_shape() -> Result<(), String> {
        use crate::core_error::CoreError;
        let cases = [
            (
                CoreError::base_unresolvable("the base `x` does not resolve to a commit"),
                "base_unresolvable",
                "analysis/base-resolution",
            ),
            (
                CoreError::repository_root_unusable("not inside a Git work tree"),
                "repository_root_unusable",
                "analysis/repository-root",
            ),
            (
                CoreError::suppression_policy_invalid("suppression policy `p` is invalid"),
                "suppression_policy_invalid",
                "analysis/suppression-policy",
            ),
            (
                CoreError::git_invocation_timeout("git -C /r [\"diff\"]", 1000, true),
                "git_invocation_timeout",
                "analysis/git-timeout",
            ),
            (
                CoreError::message("some unmigrated analysis failure"),
                "analysis_failed",
                "analysis/failure",
            ),
        ];
        for (error, identity, route) in cases {
            let rendered = render_check_failure_json(&input(), &error)?;
            assert!(!rendered.is_empty(), "refusal stdout must be non-empty");
            let value: Value = serde_json::from_str(&rendered)
                .map_err(|err| format!("parse refusal JSON: {err}"))?;
            for (pointer, expected) in [
                ("/schema_version", CHECK_OUTPUT_SCHEMA_VERSION),
                ("/tool", "ripr"),
                ("/mode", "draft"),
                ("/root", "."),
                ("/base", "origin/main"),
                ("/analysis_scope/scope", "diff"),
                ("/analysis_scope/run_status", identity),
                ("/analysis_scope/basis", identity),
                ("/analysis_scope/limitation", identity),
                ("/analysis_scope/repair_route", route),
                ("/run_limitations/0/category", identity),
                ("/run_limitations/0/run_status", identity),
                ("/run_limitations/0/basis", identity),
                ("/run_limitations/0/repair_route", route),
            ] {
                let actual = value.pointer(pointer).and_then(Value::as_str);
                if actual != Some(expected) {
                    return Err(format!("{pointer}: expected {expected}, got {actual:?}"));
                }
            }
            for pointer in [
                "/analysis_scope/downstream_consumable",
                "/run_limitations/0/downstream_consumable",
            ] {
                if value.pointer(pointer) != Some(&Value::Bool(false)) {
                    return Err(format!("{pointer} must be false: {value}"));
                }
            }
            if value["findings"].as_array().map(Vec::len) != Some(0) {
                return Err(format!("findings must be empty: {value}"));
            }
            // Non-config families echo the stderr diagnostic verbatim.
            if value["run_limitations"][0]["message"] != Value::String(error.to_string()) {
                return Err(format!("message must echo the diagnostic: {value}"));
            }
        }
        Ok(())
    }

    #[test]
    fn refusal_envelope_redacts_config_contents_but_keeps_the_summary() -> Result<(), String> {
        use crate::core_error::CoreError;
        let full = "/repo/ripr.toml: invalid ripr.toml: TOML parse error at line 1, column 9\n  |\n1 | canary_config_secret = [\n  |         ^\nexpected value";
        let error = CoreError::config_invalid(full);
        let rendered = render_check_failure_json(&input(), &error)?;
        let value: Value =
            serde_json::from_str(&rendered).map_err(|err| format!("parse JSON: {err}"))?;
        assert_eq!(value["analysis_scope"]["run_status"], "config_invalid");
        let message = value["run_limitations"][0]["message"]
            .as_str()
            .ok_or("message must be a string")?;
        assert!(
            !message.contains("canary_config_secret"),
            "config source excerpt must not enter machine output: {message}"
        );
        assert!(
            message.contains("invalid ripr.toml"),
            "the actionable summary must survive redaction: {message}"
        );
        // The typed error itself keeps the full prose for stderr.
        assert_eq!(error.to_string(), full);
        Ok(())
    }

    #[test]
    fn refusal_envelope_stays_structural_for_lookalikes_and_wrapped_failures() -> Result<(), String>
    {
        use crate::core_error::CoreError;
        // A forged prefix on an untyped message must not claim the family.
        let lookalike = CoreError::message("base_unresolvable: forged prefix must not match");
        let rendered = render_check_failure_json(&input(), &lookalike)?;
        let value: Value =
            serde_json::from_str(&rendered).map_err(|err| format!("parse JSON: {err}"))?;
        assert_eq!(value["analysis_scope"]["run_status"], "analysis_failed");
        // A wrapped typed failure keeps its identity though its Display
        // names no family at the start.
        let wrapped =
            CoreError::base_unresolvable("the base `x` does not resolve").with_context("outer");
        assert!(!wrapped.to_string().starts_with("base_unresolvable"));
        let rendered = render_check_failure_json(&input(), &wrapped)?;
        let value: Value =
            serde_json::from_str(&rendered).map_err(|err| format!("parse JSON: {err}"))?;
        assert_eq!(value["analysis_scope"]["run_status"], "base_unresolvable");
        assert_eq!(
            value["run_limitations"][0]["message"],
            Value::String(wrapped.to_string())
        );
        Ok(())
    }

    #[test]
    fn refusal_envelope_without_base_omits_base() -> Result<(), String> {
        use crate::core_error::CoreError;
        let mut check_input = input();
        check_input.base = None;
        let rendered = render_check_failure_json(
            &check_input,
            &CoreError::base_unresolvable("could not resolve a default base"),
        )?;
        let value: Value =
            serde_json::from_str(&rendered).map_err(|err| format!("parse JSON: {err}"))?;
        assert!(
            value.get("base").is_none(),
            "base should be omitted when absent: {value}"
        );
        Ok(())
    }
}
