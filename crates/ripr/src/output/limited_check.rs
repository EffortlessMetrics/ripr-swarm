use crate::app::{CHECK_OUTPUT_SCHEMA_VERSION, CHECK_REFUSAL_SCHEMA_VERSION, CheckInput};
use crate::core_error::CoreError;
use crate::output::json::render_pretty_with_newline;
use serde_json::{Value, json};

const DIFF_SCOPE_OVERSIZED_PREFIX: &str = "diff_scope_oversized:";
const DIFF_SCOPE_REPAIR_ROUTE: &str = "analysis/diff-scope-budget";
const REPO_SCOPE_OVERSIZED_PREFIX: &str = "repo_scope_oversized:";
const REPO_SCOPE_REPAIR_ROUTE: &str = "analysis/repo-scope-budget";

/// The refusal basis for a diff-scope guard error: which language's budget
/// actually fired (#10730). The preview guards name their env override in
/// the refusal text, so the basis derives from that name; every other
/// diff-scope refusal (Rust line/file guards) keeps `rust_diff_scope_budget`
/// exactly as before. The `--json` consumer must see the budget it can act
/// on — a Python refusal reporting a Rust basis would claim more than the
/// envelope enforces (evidence honesty).
fn diff_scope_basis(error: &str) -> &'static str {
    if error.contains(crate::analysis::DIFF_CHANGED_PYTHON_LINE_LIMIT_ENV) {
        "python_diff_scope_budget"
    } else if error.contains(crate::analysis::DIFF_CHANGED_TYPESCRIPT_LINE_LIMIT_ENV) {
        "typescript_diff_scope_budget"
    } else {
        "rust_diff_scope_budget"
    }
}

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
            diff_scope_basis(error),
            DIFF_SCOPE_REPAIR_ROUTE,
        )
    };

    let mut value = json!({
        "schema_version": CHECK_OUTPUT_SCHEMA_VERSION,
        "tool": "ripr",
        "mode": input.mode.as_str(),
        "root": crate::output::path::display_path(&input.root),
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
        "status": crate::output::cli_refusal::REFUSED_STATUS,
        "error": crate::output::cli_refusal::error_block(
            run_status,
            error,
            crate::output::cli_refusal::next_command_for_route(repair_route),
        ),
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

/// Invocation context a refusal envelope echoes (#7331). Each `--json`
/// command keys the shared envelope shape with its own `schema_version`;
/// `mode` appears only for commands that accept one, and no
/// caller-unsupplied value enters the document (RIPR-SPEC-0241).
pub(crate) struct RefusalEnvelopeContext<'a> {
    /// The refusing command's own schema version.
    pub(crate) schema_version: &'static str,
    /// The analysis scope the command would have run (`"diff"` or `"repo"`).
    pub(crate) scope: &'static str,
    /// The CLI mode label, for commands that accept `--mode`.
    pub(crate) mode: Option<&'static str>,
    pub(crate) root: &'a std::path::Path,
    pub(crate) base: Option<&'a str>,
}

/// Render the shared typed refusal document for a post-argv-parse failure
/// (#6834, generalized #7331).
///
/// Same envelope shape for every command — same keys, same zeroed summary,
/// same empty findings, same non-consumable treatment — so a JSON consumer
/// has one contract: the typed identity selects `run_status`/`category`/
/// `limitation` (and `basis`, which names the limiting condition), and the
/// human diagnostic rides in `message`. The identity comes from
/// [`CoreError::check_refusal`], matched structurally; this renderer never
/// inspects message text.
/// Build the shared refusal document value for a post-argv-parse failure
/// (#6834, generalized #7331).
///
/// Same envelope shape for every command — same keys, same zeroed summary,
/// same empty findings, same non-consumable treatment — so a JSON consumer
/// has one contract: the typed identity selects `run_status`/`category`/
/// `limitation` (and `basis`, which names the limiting condition), and the
/// human diagnostic rides in `message`. The identity comes from
/// [`CoreError::check_refusal`], matched structurally; this renderer never
/// inspects message text.
fn refusal_envelope_value(context: &RefusalEnvelopeContext<'_>, error: &CoreError) -> Value {
    let notice = error.refusal_notice();
    let identity = notice.identity;
    let repair_route = notice.repair_route;
    let remedy = serde_json::to_value(&notice.remedy).unwrap_or(Value::Null);
    let message = notice.message;

    let mut value = json!({
        "schema_version": context.schema_version,
        "tool": "ripr",
        "root": crate::output::path::display_path(context.root),
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
        "status": crate::output::cli_refusal::REFUSED_STATUS,
        "error": crate::output::cli_refusal::error_block(
            identity,
            &message,
            crate::output::cli_refusal::next_command_for_route(repair_route),
        ),
        "analysis_scope": {
            "scope": context.scope,
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
                "repair_route": repair_route,
                "remedy": remedy
            }
        ]
    });

    if let Some(mode) = context.mode {
        value["mode"] = json!(mode);
    }
    if let Some(base) = context.base {
        value["base"] = json!(base);
    }
    value
}

pub(crate) fn render_refusal_envelope(
    context: &RefusalEnvelopeContext<'_>,
    error: &CoreError,
) -> Result<String, String> {
    render_pretty_with_newline(&refusal_envelope_value(context, error), "limited check")
}

/// Render the `check --json` refusal document for a failure that is not one
/// of the two scope guards (#6834).
///
/// Delegates to [`render_refusal_envelope`] with the check output schema
/// version, so the check envelope keeps one builder with its sibling
/// commands (#7331) and stays byte-identical to the #6834 shape: same keys,
/// same zeroed summary, same non-consumable treatment. The identity comes
/// from [`CoreError::check_refusal`], matched structurally; this renderer
/// never inspects message text.
///
/// An `analysis_timeout` refusal consumes the stashed input-context
/// projection (#10796): the envelope then renders at the amended refusal
/// generation [`CHECK_REFUSAL_SCHEMA_VERSION`] with an `input_context` key
/// carrying the requested echo, availability-qualified resolved facts, and
/// one bounded retry. With no stashed projection the timeout refusal keeps
/// the pre-amendment `0.2` bytes, so library callers and sibling surfaces
/// see no change.
///
/// Callers try [`render_diff_scope_limited_check_json`] first, so scope-guard
/// bytes stay frozen and #4861 keeps owning those identities.
pub(crate) fn render_check_failure_json(
    input: &CheckInput,
    error: &CoreError,
) -> Result<String, String> {
    let identity = error.refusal_notice().identity;
    let timeout_context = (identity == "analysis_timeout")
        .then(crate::app::timeout_refusal::take_stashed)
        .flatten();
    let mut context = RefusalEnvelopeContext {
        schema_version: CHECK_OUTPUT_SCHEMA_VERSION,
        scope: "diff",
        mode: Some(input.mode.as_str()),
        root: &input.root,
        base: input.base.as_deref(),
    };
    if timeout_context.is_some() {
        context.schema_version = CHECK_REFUSAL_SCHEMA_VERSION;
    }
    let mut value = refusal_envelope_value(&context, error);
    if let Some(projection) = timeout_context {
        value["input_context"] = projection.input_context_value();
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
            scope: Default::default(),
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
    fn python_budget_refusal_names_python_diff_scope_budget() -> Result<(), String> {
        // #10730: the basis must name the budget that actually fired, so a
        // Python refusal never reports a Rust basis to machine consumers.
        let refusal = "diff_scope_oversized: 2100 changed Python lines across 3 Python files \
                       exceed the RIPR_MAX_DIFF_CHANGED_PYTHON_LINES limit (2000); analysis \
                       was not run to protect runner memory before probe expansion. Repair \
                       route: reduce the diff scope, split the extraction PR, run a narrower \
                       diff, or raise the limit via RIPR_MAX_DIFF_CHANGED_PYTHON_LINES=<number>. \
                       To analyze a subset, pass `--path <glob>` and/or `--exclude <glob>` to \
                       `ripr check` (repeatable, repository-relative; files left out are \
                       reported as not analyzed).";
        let rendered = render_diff_scope_limited_check_json(&input(), refusal)?
            .ok_or("expected a limited artifact for the Python guard")?;
        let value: Value =
            serde_json::from_str(&rendered).map_err(|err| format!("parse JSON: {err}"))?;
        for pointer in ["/analysis_scope/basis", "/run_limitations/0/basis"] {
            let actual = value.pointer(pointer).and_then(Value::as_str);
            if actual != Some("python_diff_scope_budget") {
                return Err(format!(
                    "{pointer}: expected python_diff_scope_budget, got {actual:?}"
                ));
            }
        }
        Ok(())
    }

    #[test]
    fn typescript_budget_refusal_names_typescript_diff_scope_budget() -> Result<(), String> {
        // #10730: same typed basis for the TypeScript/JavaScript family guard.
        let refusal = "diff_scope_oversized: 2100 changed TypeScript lines across 5 \
                       TypeScript files exceed the RIPR_MAX_DIFF_CHANGED_TYPESCRIPT_LINES \
                       limit (2000); analysis was not run to protect runner memory before \
                       probe expansion. Repair route: reduce the diff scope, split the \
                       extraction PR, run a narrower diff, or raise the limit via \
                       RIPR_MAX_DIFF_CHANGED_TYPESCRIPT_LINES=<number>. To analyze a subset, \
                       pass `--path <glob>` and/or `--exclude <glob>` to `ripr check` \
                       (repeatable, repository-relative; files left out are reported as not \
                       analyzed).";
        let rendered = render_diff_scope_limited_check_json(&input(), refusal)?
            .ok_or("expected a limited artifact for the TypeScript guard")?;
        let value: Value =
            serde_json::from_str(&rendered).map_err(|err| format!("parse JSON: {err}"))?;
        for pointer in ["/analysis_scope/basis", "/run_limitations/0/basis"] {
            let actual = value.pointer(pointer).and_then(Value::as_str);
            if actual != Some("typescript_diff_scope_budget") {
                return Err(format!(
                    "{pointer}: expected typescript_diff_scope_budget, got {actual:?}"
                ));
            }
        }
        Ok(())
    }

    #[test]
    fn rust_budget_refusals_keep_the_rust_basis() -> Result<(), String> {
        // Both Rust diff-scope guards keep the basis they reported before
        // the preview guards existed.
        for refusal in [
            "diff_scope_oversized: 2101 changed Rust lines across 2 Rust files exceed the \
             RIPR_MAX_DIFF_CHANGED_RUST_LINES limit (2000); analysis was not run.",
            "diff_scope_oversized: 900 indexed Rust files exceed the \
             RIPR_MAX_DIFF_INDEX_FILES limit (800); analysis was not run.",
        ] {
            let rendered = render_diff_scope_limited_check_json(&input(), refusal)?
                .ok_or("expected a limited artifact for the Rust guard")?;
            let value: Value =
                serde_json::from_str(&rendered).map_err(|err| format!("parse JSON: {err}"))?;
            for pointer in ["/analysis_scope/basis", "/run_limitations/0/basis"] {
                let actual = value.pointer(pointer).and_then(Value::as_str);
                if actual != Some("rust_diff_scope_budget") {
                    return Err(format!(
                        "{pointer}: expected rust_diff_scope_budget, got {actual:?}"
                    ));
                }
            }
        }
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

    #[test]
    fn refusal_envelope_carries_the_remedy_its_message_states() -> Result<(), String> {
        use crate::core_error::CoreError;
        // Positive: the message states a command, a placeholder and a retry.
        let stated = CoreError::base_unresolvable(
            "base `nope` does not resolve; run `ripr check --base <ref>` with a ref that exists, then rerun",
        );
        let rendered = render_check_failure_json(&input(), &stated)?;
        let value: Value =
            serde_json::from_str(&rendered).map_err(|err| format!("parse JSON: {err}"))?;
        let remedy = value
            .pointer("/run_limitations/0/remedy")
            .ok_or_else(|| format!("run_limitations[0].remedy missing: {value}"))?;
        assert_eq!(remedy["kind"], "command", "{remedy}");
        assert_eq!(remedy["command"], "ripr check --base <ref>", "{remedy}");
        assert_eq!(remedy["fill_in"], serde_json::json!(["<ref>"]), "{remedy}");
        assert_eq!(remedy["retry_stated"], true, "{remedy}");

        // Negative control: a message that states no step is an explicit
        // `none`, never a guessed command.
        let silent = CoreError::base_unresolvable("the base does not resolve");
        let rendered = render_check_failure_json(&input(), &silent)?;
        let value: Value =
            serde_json::from_str(&rendered).map_err(|err| format!("parse JSON: {err}"))?;
        let remedy = value
            .pointer("/run_limitations/0/remedy")
            .ok_or_else(|| format!("run_limitations[0].remedy missing: {value}"))?;
        assert_eq!(remedy["kind"], "none", "{remedy}");
        assert_eq!(remedy["command"], Value::Null, "{remedy}");
        assert_eq!(remedy["retry_stated"], Value::Null, "{remedy}");
        assert_eq!(remedy["flags"], serde_json::json!([]), "{remedy}");
        Ok(())
    }

    #[test]
    fn refusal_envelope_names_the_artifact_ledger_and_seam_identities() -> Result<(), String> {
        use crate::core_error::CoreError;
        // #7331: the three new typed families select their own identities and
        // repair routes, with the stderr diagnostic echoed verbatim.
        let cases = [
            (
                CoreError::check_artifact_unusable(
                    "check artifact `nope.json` was unreadable or unparsable",
                ),
                "check_artifact_unusable",
                "analysis/check-artifact",
            ),
            (
                CoreError::gap_ledger_unusable("read gap ledger nope.json failed: missing"),
                "gap_ledger_unusable",
                "analysis/gap-ledger",
            ),
            (
                CoreError::seam_id_unknown(
                    "agent packet seam_id s was not found. Run `ripr pilot`",
                ),
                "seam_id_unknown",
                "analysis/seam-inventory",
            ),
            (
                CoreError::seam_policy_omitted(
                    "agent packet seam_id s is configured off for weakly_gripped seams",
                ),
                "seam_policy_omitted",
                "analysis/seam-policy",
            ),
            (
                CoreError::seam_not_actionable("agent start seam_id s is opaque"),
                "seam_not_actionable",
                "analysis/seam-limitation",
            ),
            (
                CoreError::analysis_timeout(
                    "analysis_timeout: ripr check exceeded its 600s wall-clock budget",
                ),
                "analysis_timeout",
                "analysis/timeout",
            ),
            (
                CoreError::analysis_worker_stopped(
                    "analysis_worker_stopped: the ripr check analysis worker stopped",
                ),
                "analysis_worker_stopped",
                "analysis/worker-stopped",
            ),
        ];
        for (error, identity, route) in cases {
            let rendered = render_check_failure_json(&input(), &error)?;
            assert!(!rendered.is_empty(), "refusal stdout must be non-empty");
            let value: Value = serde_json::from_str(&rendered)
                .map_err(|err| format!("parse refusal JSON: {err}"))?;
            for (pointer, expected) in [
                ("/analysis_scope/run_status", identity),
                ("/analysis_scope/repair_route", route),
                ("/analysis_scope/limitation", identity),
                ("/run_limitations/0/category", identity),
                ("/run_limitations/0/repair_route", route),
            ] {
                let actual = value.pointer(pointer).and_then(Value::as_str);
                if actual != Some(expected) {
                    return Err(format!("{pointer}: expected {expected}, got {actual:?}"));
                }
            }
            if value["run_limitations"][0]["message"] != Value::String(error.to_string()) {
                return Err(format!("message must echo the diagnostic: {value}"));
            }
        }
        Ok(())
    }

    #[test]
    fn sibling_refusal_envelope_omits_mode_and_base_when_unsupplied() -> Result<(), String> {
        use crate::core_error::CoreError;
        // #7331: commands without a `--mode` (rerun, agent brief, agent
        // packet) must not grow a fabricated `mode` key; a repo-scope
        // command keys the same envelope shape with its own version.
        let context = RefusalEnvelopeContext {
            schema_version: "ripr-targeted-rerun-v1",
            scope: "diff",
            mode: None,
            root: std::path::Path::new("."),
            base: None,
        };
        let rendered = render_refusal_envelope(
            &context,
            &CoreError::gap_ledger_unusable("read gap ledger nope.json failed: missing"),
        )?;
        let value: Value =
            serde_json::from_str(&rendered).map_err(|err| format!("parse JSON: {err}"))?;
        for (pointer, expected) in [
            ("/schema_version", "ripr-targeted-rerun-v1"),
            ("/analysis_scope/scope", "diff"),
            ("/analysis_scope/run_status", "gap_ledger_unusable"),
            ("/analysis_scope/repair_route", "analysis/gap-ledger"),
        ] {
            let actual = value.pointer(pointer).and_then(Value::as_str);
            if actual != Some(expected) {
                return Err(format!("{pointer}: expected {expected}, got {actual:?}"));
            }
        }
        if value.get("mode").is_some() {
            return Err(format!(
                "mode must be omitted when the command has none: {value}"
            ));
        }
        if value.get("base").is_some() {
            return Err(format!(
                "base must be omitted when none was supplied: {value}"
            ));
        }
        if value["findings"].as_array().map(Vec::len) != Some(0) {
            return Err(format!("findings must be empty: {value}"));
        }
        if value.pointer("/analysis_scope/downstream_consumable") != Some(&Value::Bool(false)) {
            return Err(format!("downstream_consumable must be false: {value}"));
        }
        Ok(())
    }

    #[test]
    fn limited_and_refused_roots_render_through_display_path() -> Result<(), String> {
        // #9806: both envelopes print `root` with the check-JSON renderer. A
        // `%` in the root is escaped by that renderer on every host, so this
        // fails if either site goes back to the raw `display()` spelling.
        let mut percent_input = input();
        percent_input.root = PathBuf::from("repo%41");
        let limited = render_diff_scope_limited_check_json(
            &percent_input,
            "diff_scope_oversized: 3 changed Rust lines exceed the limit",
        )?
        .ok_or("expected limited artifact")?;
        let refused = render_refusal_envelope(
            &RefusalEnvelopeContext {
                schema_version: CHECK_OUTPUT_SCHEMA_VERSION,
                scope: "diff",
                mode: None,
                root: std::path::Path::new("repo%41"),
                base: None,
            },
            &CoreError::gap_ledger_unusable("read gap ledger nope.json failed: missing"),
        )?;
        for (label, rendered) in [("limited", limited), ("refused", refused)] {
            let value: Value = serde_json::from_str(&rendered)
                .map_err(|err| format!("parse {label} JSON: {err}"))?;
            let root = value.pointer("/root").and_then(Value::as_str);
            if root != Some("repo%2541") {
                return Err(format!(
                    "{label} root must use display_path spelling, got {root:?}"
                ));
            }
        }
        Ok(())
    }

    #[test]
    fn timeout_refusal_with_stashed_context_renders_input_context_at_generation_0_3()
    -> Result<(), String> {
        use crate::app::timeout_refusal;
        use crate::core_error::CoreError;
        timeout_refusal::clear_stashed();
        // Late timeout: the loader and intake published before the deadline.
        timeout_refusal::stash(timeout_refusal::build(
            &input(),
            timeout_refusal::RequestedSourceContext {
                source: crate::analysis::InputSource::Committed,
                worktree_explicit: false,
                committed_explicit: true,
                candidate_tree: None,
                candidate_base: None,
            },
            std::time::Duration::from_mins(10),
            true,
            None,
            None,
            Some(crate::analysis::CapturedInputFacts {
                facts: crate::analysis::PublishedInputFacts {
                    source: Some(crate::analysis::InputSource::Committed),
                    effective_base: Some("origin/main".to_string()),
                    resolved_base_commit: Some("06e6aea".to_string()),
                    merge_base_origin: None,
                    scoped_changed_file_count: Some(6),
                    out_of_scope_file_count: Some(0),
                },
                post_deadline: crate::analysis::PublishedInputFacts::default(),
                loader_published: true,
                intake_published: true,
            }),
        ));
        let error = CoreError::analysis_timeout(
            "analysis_timeout: ripr check exceeded its 600s wall-clock budget",
        );
        let rendered = render_check_failure_json(&input(), &error)?;
        let value: Value =
            serde_json::from_str(&rendered).map_err(|err| format!("parse refusal JSON: {err}"))?;

        // Generation 0.3 and the new key, only because the projection exists.
        if value.pointer("/schema_version").and_then(Value::as_str) != Some("0.3") {
            return Err(format!("timeout refusal must render at 0.3: {value}"));
        }
        for (pointer, expected) in [
            ("/input_context/requested/source", "committed"),
            ("/input_context/requested/base", "origin/main"),
            ("/input_context/requested/analysis_mode", "draft"),
            (
                "/input_context/resolved/effective_base/value",
                "origin/main",
            ),
            ("/input_context/resolved/base_commit/value", "06e6aea"),
            ("/input_context/recovery/kind", "retry_with_larger_budget"),
        ] {
            let actual = value.pointer(pointer).and_then(Value::as_str);
            if actual != Some(expected) {
                return Err(format!("{pointer}: expected {expected}, got {actual:?}"));
            }
        }
        // The scoped intake count is a JSON number, not a string.
        if value
            .pointer("/input_context/resolved/scoped_changed_files/value")
            .and_then(Value::as_u64)
            != Some(6)
        {
            return Err(format!("scoped_changed_files must read 6: {value}"));
        }
        // #10796 acceptance: the typed identity, zeroed summary, empty
        // findings and non-consumable treatment are unchanged.
        for (pointer, expected) in [
            ("/analysis_scope/run_status", "analysis_timeout"),
            ("/analysis_scope/basis", "analysis_timeout"),
            ("/analysis_scope/repair_route", "analysis/timeout"),
            ("/run_limitations/0/category", "analysis_timeout"),
        ] {
            let actual = value.pointer(pointer).and_then(Value::as_str);
            if actual != Some(expected) {
                return Err(format!("{pointer}: expected {expected}, got {actual:?}"));
            }
        }
        if value.pointer("/analysis_scope/downstream_consumable") != Some(&Value::Bool(false))
            || value.pointer("/run_limitations/0/downstream_consumable")
                != Some(&Value::Bool(false))
        {
            return Err(format!(
                "non-consumable treatment must be preserved: {value}"
            ));
        }
        if value["findings"].as_array().map(Vec::len) != Some(0) {
            return Err(format!("findings must stay empty: {value}"));
        }
        if value.pointer("/summary/changed_rust_files") != Some(&Value::from(0)) {
            return Err(format!("summary must stay zeroed: {value}"));
        }
        // Input metadata must not imply any exposure/clean inference.
        if value.pointer("/input_context/resolved/scoped_changed_files/available")
            != Some(&Value::Bool(true))
        {
            return Err(format!("published scoped count must be available: {value}"));
        }
        // The stash is consumed exactly once.
        if timeout_refusal::take_stashed().is_some() {
            return Err("the render must consume the stashed projection".to_string());
        }
        Ok(())
    }

    #[test]
    fn timeout_refusal_without_context_keeps_the_pre_amendment_bytes() -> Result<(), String> {
        use crate::app::timeout_refusal;
        use crate::core_error::CoreError;
        timeout_refusal::clear_stashed();
        let error = CoreError::analysis_timeout(
            "analysis_timeout: ripr check exceeded its 600s wall-clock budget",
        );
        let rendered = render_check_failure_json(&input(), &error)?;
        let value: Value =
            serde_json::from_str(&rendered).map_err(|err| format!("parse refusal JSON: {err}"))?;
        // No stashed projection (library caller): unchanged 0.2 shape, no
        // input_context key, so no caller-unsupplied value enters the doc.
        if value.pointer("/schema_version").and_then(Value::as_str) != Some("0.2") {
            return Err(format!(
                "context-less timeout refusal must stay at 0.2: {value}"
            ));
        }
        if value.get("input_context").is_some() {
            return Err(format!(
                "input_context must be absent without a projection: {value}"
            ));
        }
        if value
            .pointer("/analysis_scope/run_status")
            .and_then(Value::as_str)
            != Some("analysis_timeout")
        {
            return Err(format!("typed identity must be preserved: {value}"));
        }
        Ok(())
    }

    #[test]
    fn non_timeout_refusals_ignore_a_stale_stash() -> Result<(), String> {
        use crate::app::timeout_refusal;
        use crate::core_error::CoreError;
        timeout_refusal::clear_stashed();
        // A stale projection must never leak into a non-timeout refusal.
        timeout_refusal::stash(timeout_refusal::build(
            &input(),
            timeout_refusal::RequestedSourceContext {
                source: crate::analysis::InputSource::Committed,
                worktree_explicit: false,
                committed_explicit: false,
                candidate_tree: None,
                candidate_base: None,
            },
            std::time::Duration::from_mins(1),
            false,
            None,
            None,
            None,
        ));
        let rendered = render_check_failure_json(
            &input(),
            &CoreError::base_unresolvable("the base `x` does not resolve to a commit"),
        )?;
        let value: Value =
            serde_json::from_str(&rendered).map_err(|err| format!("parse refusal JSON: {err}"))?;
        if value.get("input_context").is_some() {
            return Err(format!(
                "non-timeout refusal must not gain input_context: {value}"
            ));
        }
        if value.pointer("/schema_version").and_then(Value::as_str) != Some("0.2") {
            return Err(format!("non-timeout refusal must stay at 0.2: {value}"));
        }
        Ok(())
    }
}
