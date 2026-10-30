//! Repo-mode TypeScript findings never carry a repair-ready signal (#5531).
//!
//! The capability table has no `repair_routing` cell for TypeScript in repo
//! mode (absent cell = `not_proven`), so a repo run (bounded whole-file working
//! set, capped/partial denominators) must not flip `repair_packet_ready` even
//! when the same finding in diff mode is a complete, delegatable packet.
use serde_json::Value;
use std::fs;

const CONFIG: &str = "[languages]\nenabled = [\"rust\", \"typescript\"]\n";
const PACKAGE_JSON: &str = "{\n  \"name\": \"discount-lib\",\n  \"version\": \"1.0.0\",\n  \"devDependencies\": {\n    \"jest\": \"^29.0.0\"\n  },\n  \"scripts\": {\n    \"test\": \"jest\"\n  }\n}\n";
const DISCOUNT_TS: &str = "export function applyDiscount(amount: number, threshold: number): number {\n    if (amount >= threshold) {\n        return amount * 0.9;\n    }\n    return amount;\n}\n";
const DISCOUNT_DIFF: &str = "diff --git a/src/discount.ts b/src/discount.ts\nindex 0000000..1111111 100644\n--- a/src/discount.ts\n+++ b/src/discount.ts\n@@ -1,6 +1,6 @@\n export function applyDiscount(amount: number, threshold: number): number {\n-    if (amount > threshold) {\n+    if (amount >= threshold) {\n         return amount * 0.9;\n     }\n     return amount;\n }\n";
const VALUE_TEST: &str = "import { applyDiscount } from '../src/discount';\n\ntest('applyDiscount applies discount when amount meets threshold', () => {\n    const result = applyDiscount(100, 100);\n    expect(result).toBeGreaterThan(50);\n});\n";

fn run(label: &str, repo: bool) -> Result<Vec<Value>, String> {
    run_with_scopes(label, repo).map(|(findings, _)| findings)
}

/// JSON findings plus the typed `analysis_scope` of every finding the
/// producer returned for the fixture source (#5531).
fn run_with_scopes(
    label: &str,
    repo: bool,
) -> Result<(Vec<Value>, Vec<crate::domain::AnalysisScope>), String> {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|error| format!("system time: {error}"))?
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "ripr-ts-repo-failclosed-{label}-{}-{stamp}",
        std::process::id()
    ));
    let output = (|| -> Result<(String, Vec<crate::domain::AnalysisScope>), String> {
        for (path, contents) in [
            ("package.json", PACKAGE_JSON),
            ("ripr.toml", CONFIG),
            ("src/discount.ts", DISCOUNT_TS),
            ("tests/discount.test.ts", VALUE_TEST),
            ("diff.patch", DISCOUNT_DIFF),
        ] {
            let path = root.join(path);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)
                    .map_err(|error| format!("create {}: {error}", parent.display()))?;
            }
            fs::write(&path, contents)
                .map_err(|error| format!("write {}: {error}", path.display()))?;
        }
        let config = crate::config::tests_only_parse(CONFIG)?;
        let input = crate::CheckInput {
            root: root.clone(),
            base: None,
            diff_file: (!repo).then(|| root.join("diff.patch")),
            mode: crate::Mode::Draft,
            format: crate::OutputFormat::Json,
            include_unchanged_tests: true,
            perl_facts_path: None,
            suppression_policy: None,
            git_timeout: None,
            git_candidate: None,
            scope: Default::default(),
        };
        let output = if repo {
            crate::app::check_workspace_repo_with_config(input, &config)?
        } else {
            crate::app::check_workspace_with_config(input, &config)?
        };
        let scopes = output
            .findings
            .iter()
            .filter(|finding| {
                finding
                    .probe
                    .location
                    .file
                    .to_string_lossy()
                    .contains("discount.ts")
            })
            .map(|finding| finding.analysis_scope)
            .collect();
        Ok((
            crate::render_check(&output, &crate::OutputFormat::Json)?,
            scopes,
        ))
    })();
    let cleanup = fs::remove_dir_all(&root)
        .map_err(|error| format!("remove fixture {}: {error}", root.display()));
    let (rendered, scopes) = output?;
    cleanup?;
    let value: Value =
        serde_json::from_str(&rendered).map_err(|error| format!("parse check output: {error}"))?;
    let findings = value
        .get("findings")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    Ok((findings, scopes))
}

fn ts_findings(findings: &[Value]) -> Vec<&Value> {
    findings
        .iter()
        .filter(|finding| finding.to_string().contains("discount.ts"))
        .collect()
}

fn ready(finding: &Value) -> Option<bool> {
    finding
        .get("preview_actionability")
        .and_then(|a| a.get("repair_packet_ready"))
        .and_then(Value::as_bool)
}

#[test]
fn diff_mode_control_is_repair_ready() -> Result<(), String> {
    let findings = run("diff", false)?;
    let ts = ts_findings(&findings);
    if ts.len() != 1 || ready(ts[0]) != Some(true) {
        return Err(format!("diff control lost its packet: {findings:?}"));
    }
    Ok(())
}

#[test]
fn repo_mode_typescript_finding_is_not_repair_ready() -> Result<(), String> {
    let findings = run("repo", true)?;
    let ts = ts_findings(&findings);
    if ts.is_empty() {
        return Err(format!("repo run produced no TS finding: {findings:?}"));
    }
    for finding in ts {
        if ready(finding) == Some(true)
            || finding
                .get("typescript_repair_packet")
                .is_some_and(|packet| !packet.is_null())
        {
            return Err(format!(
                "repo-mode finding reached repair signal: {finding}"
            ));
        }
    }
    Ok(())
}

/// #5531: the producers set the typed scope by construction: every repo-run
/// finding is `Repo`, and the diff control stays `Diff`.
#[test]
fn producer_sets_typed_analysis_scope_typescript() -> Result<(), String> {
    let (_, repo) = run_with_scopes("scope-repo", true)?;
    if repo.is_empty()
        || repo
            .iter()
            .any(|s| *s != crate::domain::AnalysisScope::Repo)
    {
        return Err(format!("repo run findings must all be Repo: {repo:?}"));
    }
    let (_, diff) = run_with_scopes("scope-diff", false)?;
    if diff.is_empty()
        || diff
            .iter()
            .any(|s| *s != crate::domain::AnalysisScope::Diff)
    {
        return Err(format!("diff control findings must all be Diff: {diff:?}"));
    }
    Ok(())
}

/// The fixture for one language: `ext` is `ts` or `js` (the `.js` source
/// routes to the same adapter, #5421). Returns the untouched `CheckOutput`.
fn run_output(label: &str, repo: bool, ext: &str) -> Result<crate::app::CheckOutput, String> {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|error| format!("system time: {error}"))?
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "ripr-ts-repo-matrix-{label}-{}-{stamp}",
        std::process::id()
    ));
    let source_path = format!("src/discount.{ext}");
    let test_path = format!("tests/discount.test.{ext}");
    let diff = DISCOUNT_DIFF.replace("discount.ts", &format!("discount.{ext}"));
    let result = (|| -> Result<crate::app::CheckOutput, String> {
        let files = [
            ("package.json", PACKAGE_JSON.to_string()),
            ("ripr.toml", CONFIG.to_string()),
            (
                source_path.as_str(),
                if ext == "js" {
                    DISCOUNT_TS.replace(": number", "")
                } else {
                    DISCOUNT_TS.to_string()
                },
            ),
            (test_path.as_str(), VALUE_TEST.to_string()),
            ("diff.patch", diff),
        ];
        for (path, contents) in files {
            let path = root.join(path);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)
                    .map_err(|error| format!("create {}: {error}", parent.display()))?;
            }
            fs::write(&path, contents)
                .map_err(|error| format!("write {}: {error}", path.display()))?;
        }
        let config = crate::config::tests_only_parse(CONFIG)?;
        let input = crate::CheckInput {
            root: root.clone(),
            base: None,
            diff_file: (!repo).then(|| root.join("diff.patch")),
            mode: crate::Mode::Draft,
            format: crate::OutputFormat::Json,
            include_unchanged_tests: true,
            perl_facts_path: None,
            suppression_policy: None,
            git_timeout: None,
            git_candidate: None,
            scope: Default::default(),
        };
        if repo {
            crate::app::check_workspace_repo_with_config(input, &config)
        } else {
            crate::app::check_workspace_with_config(input, &config)
        }
    })();
    let cleanup = fs::remove_dir_all(&root)
        .map_err(|error| format!("remove fixture {}: {error}", root.display()));
    let output = result?;
    cleanup?;
    Ok(output)
}

fn json_of(output: &crate::app::CheckOutput) -> Result<Value, String> {
    let text = crate::render_check(output, &crate::OutputFormat::Json)?;
    serde_json::from_str(&text).map_err(|error| format!("parse check output: {error}"))
}

/// #5531 matrix (8, 9, 10): real repo TS and JS fixtures carry the
/// per-language scope prefix plus the literal specific reason in the
/// actionability JSON, preview card, human limitation row and SARIF, with the
/// packet closed. Real TS check JSON reaches the ledger with the same text and
/// no route; JS keeps the existing zero-record boundary.
#[test]
fn real_repo_fixtures_carry_scope_led_reason_on_every_surface() -> Result<(), String> {
    const SPECIFIC: &str = "TypeScript preview has owner, related-test, oracle, and probe evidence but lacks a complete repair packet contract";
    for (ext, language) in [("ts", "typescript"), ("js", "javascript")] {
        let output = run_output(&format!("surf-{ext}"), true, ext)?;
        let prefix = format!(
            "repo-mode findings have no repair routing ({language}/any/repo/repair_routing is not_proven)"
        );
        let want = format!("{prefix}; ");
        let subjects: Vec<&crate::domain::Finding> = output
            .findings
            .iter()
            .filter(|f| {
                f.probe
                    .location
                    .file
                    .to_string_lossy()
                    .contains("discount.")
            })
            .collect();
        if subjects.is_empty() {
            return Err(format!("{ext}: repo run produced no finding"));
        }
        let json = json_of(&output)?;
        let findings: Vec<&Value> = json["findings"]
            .as_array()
            .map(|f| {
                f.iter()
                    .filter(|f| f.to_string().contains("discount."))
                    .collect()
            })
            .unwrap_or_default();
        for finding in &findings {
            if finding["analysis_scope"] != "repo" {
                return Err(format!("{ext}: typed scope not emitted: {finding}"));
            }
            let a = &finding["preview_actionability"];
            let why = a["why_not_actionable"].as_str().unwrap_or_default();
            if !why.starts_with(&want) || !why.contains(SPECIFIC) || ready(finding) != Some(false) {
                return Err(format!("{ext}: actionability why/ready wrong: {a}"));
            }
            if let Some(card) = finding
                .get("typescript_preview_card")
                .filter(|c| !c.is_null())
                && card["why_not_actionable"] != a["why_not_actionable"]
            {
                return Err(format!("{ext}: card reason differs: {card}"));
            }
        }
        let human = crate::render_check(&output, &crate::OutputFormat::HumanFull)?;
        if !human.contains(&format!("limitation: {want}")) {
            return Err(format!("{ext}: human limitation row lacks scope:\n{human}"));
        }
        let sarif = crate::render_check(&output, &crate::OutputFormat::Sarif)?;
        if !sarif.contains(&prefix) || !sarif.contains(SPECIFIC) {
            return Err(format!("{ext}: SARIF lacks scope-led reason"));
        }

        // Ledger over the untouched real bytes.
        use crate::output::gap_decision_ledger::{
            GapDecisionLedgerInput, GapDecisionLedgerSourceKind, build_gap_decision_ledger_report,
        };
        let records = build_gap_decision_ledger_report(GapDecisionLedgerInput {
            root: ".".to_string(),
            generated_at: "test".to_string(),
            source_kind: GapDecisionLedgerSourceKind::CheckOutput,
            records_path: "check.json".to_string(),
            records_json: Ok(crate::render_check(&output, &crate::OutputFormat::Json)?),
        })
        .records;
        let ts_records: Vec<_> = records
            .iter()
            .filter(|r| r.language == "typescript" || r.language == "javascript")
            .collect();
        if language == "typescript" {
            if ts_records.is_empty()
                || ts_records.iter().any(|r| {
                    r.repair_route.is_some()
                        || !r.static_limits.iter().any(|l| {
                            l["detail"]
                                .as_str()
                                .is_some_and(|d| d.starts_with(&want) && d.contains(SPECIFIC))
                        })
                })
            {
                return Err(format!("ts ledger records wrong: {ts_records:?}"));
            }
        } else if !ts_records.is_empty() {
            return Err(format!("js ledger boundary changed: {ts_records:?}"));
        }
    }
    Ok(())
}

/// #5531 matrix (5, 6): a real diff finding omits the typed field and keeps
/// its packet; adding only the legacy repo evidence line is a contradiction
/// that closes the packet and selects no repo reason.
#[test]
fn real_diff_fixtures_omit_scope_and_contradiction_closes_packet() -> Result<(), String> {
    for ext in ["ts", "js"] {
        let mut output = run_output(&format!("diff-{ext}"), false, ext)?;
        let json = json_of(&output)?;
        let diff: Vec<&Value> = ts_findings_in(&json);
        if diff.is_empty()
            || diff
                .iter()
                .any(|f| f.get("analysis_scope").is_some() || ready(f) != Some(true))
        {
            return Err(format!("{ext}: diff control wrong: {diff:?}"));
        }
        for finding in &mut output.findings {
            finding
                .evidence
                .push(crate::domain::language_capability::REPO_SCOPE_EVIDENCE_LINE.to_string());
        }
        let json = json_of(&output)?;
        for f in ts_findings_in(&json) {
            if ready(f) == Some(true) || f.to_string().contains("repo-mode findings") {
                return Err(format!("{ext}: contradiction not closed: {f}"));
            }
        }
    }
    Ok(())
}

fn ts_findings_in(json: &Value) -> Vec<&Value> {
    json["findings"]
        .as_array()
        .map(|f| {
            f.iter()
                .filter(|f| f.to_string().contains("discount."))
                .collect()
        })
        .unwrap_or_default()
}

/// Every key of every real finding must be declared by the published schema
/// (`additionalProperties: false`), so a producer cannot add an undeclared one.
fn assert_finding_keys_in_published_schema(json: &Value, label: &str) -> Result<(), String> {
    let schema: Value = serde_json::from_str(include_str!(
        "../../../../../schemas/ripr/check.schema.json"
    ))
    .map_err(|e| e.to_string())?;
    let props = schema["$defs"]["finding"]["properties"]
        .as_object()
        .ok_or_else(|| "schema finding properties missing".to_string())?;
    let findings = json["findings"].as_array().cloned().unwrap_or_default();
    if findings.is_empty() {
        return Err(format!("{label}: no findings to check"));
    }
    for finding in &findings {
        for key in finding.as_object().map(|o| o.keys()).into_iter().flatten() {
            if !props.contains_key(key) {
                return Err(format!(
                    "{label}: finding key `{key}` is not in the published schema"
                ));
            }
        }
    }
    Ok(())
}

#[test]
fn real_ts_and_js_payload_finding_keys_are_declared_by_schema() -> Result<(), String> {
    for (ext, repo) in [("ts", true), ("js", true), ("ts", false)] {
        let json = json_of(&run_output(&format!("schema-{ext}-{repo}"), repo, ext)?)?;
        assert_finding_keys_in_published_schema(&json, &format!("{ext} repo={repo}"))?;
    }
    Ok(())
}
