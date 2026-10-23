//! Repo-mode Python findings never carry a repair card or repair routing (#5531).
//!
//! The capability table registers Python `repair_routing` only for diff mode
//! (`python/pytest/diff/...`, `python/unittest/diff/...`); a repo-mode cell is
//! absent, which means `not_proven`. A repo run (bounded whole-file working
//! set) must therefore not emit a repair card or route repair, even when the
//! same finding in diff mode does. The gap ledger may still carry an advisory
//! no-repair-card record for it; that is not repair routing.
use serde_json::Value;
use std::fs;

const CONFIG: &str = "[languages]\nenabled = [\"rust\", \"python\"]\n";
const SOURCE: &str = "class Stock:\n    def reserve(self, qty):\n        if qty > 10:\n            return \"backorder\"\n        return \"ok\"\n";
const TEST: &str = "from src.pricing import Stock\n\ndef test_reserve_ok():\n    stock = Stock()\n    assert stock.reserve(3) == \"ok\"\n";
const DIFF: &str = "diff --git a/src/pricing.py b/src/pricing.py\nindex 1111111..2222222 100644\n--- a/src/pricing.py\n+++ b/src/pricing.py\n@@ -1,5 +1,5 @@\n class Stock:\n     def reserve(self, qty):\n-        if qty >= 10:\n+        if qty > 10:\n             return \"backorder\"\n         return \"ok\"\n";

fn run(label: &str, repo: bool, format: crate::OutputFormat) -> Result<String, String> {
    run_with_scopes(label, repo, format).map(|(rendered, _)| rendered)
}

/// Rendered output plus the typed `analysis_scope` of every finding the
/// producer returned for the fixture source (#5531).
fn run_with_scopes(
    label: &str,
    repo: bool,
    format: crate::OutputFormat,
) -> Result<(String, Vec<crate::domain::AnalysisScope>), String> {
    run_with_test(label, repo, format, TEST)
}

fn run_with_test(
    label: &str,
    repo: bool,
    format: crate::OutputFormat,
    test_source: &str,
) -> Result<(String, Vec<crate::domain::AnalysisScope>), String> {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|error| format!("system time: {error}"))?
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "ripr-py-repo-failclosed-{label}-{}-{stamp}",
        std::process::id()
    ));
    let output = (|| -> Result<(String, Vec<crate::domain::AnalysisScope>), String> {
        for (path, contents) in [
            ("ripr.toml", CONFIG),
            ("src/pricing.py", SOURCE),
            ("tests/test_pricing.py", test_source),
            ("diff.patch", DIFF),
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
            format,
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
                    .contains("pricing.py")
            })
            .map(|finding| finding.analysis_scope)
            .collect();
        Ok((crate::render_check(&output, &format)?, scopes))
    })();
    let cleanup = fs::remove_dir_all(&root)
        .map_err(|error| format!("remove fixture {}: {error}", root.display()));
    let rendered = output?;
    cleanup?;
    Ok(rendered)
}

fn py_findings(rendered: &str) -> Result<Vec<Value>, String> {
    let value: Value =
        serde_json::from_str(rendered).map_err(|error| format!("parse check output: {error}"))?;
    Ok(value
        .get("findings")
        .and_then(Value::as_array)
        .map(|findings| {
            findings
                .iter()
                .filter(|finding| {
                    finding.get("classification").and_then(Value::as_str) == Some("weakly_exposed")
                        && finding.to_string().contains("pricing.py")
                })
                .cloned()
                .collect()
        })
        .unwrap_or_default())
}

#[test]
fn diff_mode_control_keeps_python_repair_card() -> Result<(), String> {
    let findings = py_findings(&run("diff", false, crate::OutputFormat::Json)?)?;
    if findings.len() != 1 || findings[0].get("python_repair_card").is_none() {
        return Err(format!("diff control lost its card: {findings:?}"));
    }
    Ok(())
}

#[test]
fn repo_mode_python_finding_has_no_repair_card() -> Result<(), String> {
    let findings = py_findings(&run("repo", true, crate::OutputFormat::Json)?)?;
    if findings.is_empty() {
        return Err("repo run produced no Python finding".to_string());
    }
    for finding in &findings {
        if finding
            .get("python_repair_card")
            .is_some_and(|card| !card.is_null())
        {
            return Err(format!(
                "repo-mode finding reached repair signal: {finding}"
            ));
        }
    }
    Ok(())
}

#[test]
fn repo_mode_python_human_output_renders_no_repair_card() -> Result<(), String> {
    let diff = run("hdiff", false, crate::OutputFormat::Human)?;
    let repo = run("hrepo", true, crate::OutputFormat::Human)?;

    const CARD_LINE: &str = "apply the next step below to the suggested test";
    const NO_CARD_LINE: &str = "has no repair card";
    if !diff.contains(CARD_LINE) || diff.contains(NO_CARD_LINE) {
        return Err(format!("diff control lost its card:\n{diff}"));
    }
    if repo.contains(CARD_LINE)
        || !repo.contains(NO_CARD_LINE)
        || repo.contains("python_repair_card")
    {
        return Err(format!("repo human output routes a repair card:\n{repo}"));
    }
    Ok(())
}

/// #5531: the producers set the typed scope by construction: every repo-run
/// finding is `Repo`, and the diff control stays `Diff`.
#[test]
fn producer_sets_typed_analysis_scope_python() -> Result<(), String> {
    let (_, repo) = run_with_scopes("scope-repo", true, crate::OutputFormat::Json)?;
    if repo.is_empty()
        || repo
            .iter()
            .any(|s| *s != crate::domain::AnalysisScope::Repo)
    {
        return Err(format!("repo run findings must all be Repo: {repo:?}"));
    }
    let (_, diff) = run_with_scopes("scope-diff", false, crate::OutputFormat::Json)?;
    if diff.is_empty()
        || diff
            .iter()
            .any(|s| *s != crate::domain::AnalysisScope::Diff)
    {
        return Err(format!("diff control findings must all be Diff: {diff:?}"));
    }
    Ok(())
}

/// Ledger records for untouched `render_check(Json)` bytes of the fixture.
fn ledger_records(
    rendered: String,
) -> Result<Vec<crate::output::gap_decision_ledger::GapRecord>, String> {
    use crate::output::gap_decision_ledger::{
        GapDecisionLedgerInput, GapDecisionLedgerSourceKind, build_gap_decision_ledger_report,
    };
    Ok(build_gap_decision_ledger_report(GapDecisionLedgerInput {
        root: ".".to_string(),
        generated_at: "test".to_string(),
        source_kind: GapDecisionLedgerSourceKind::CheckOutput,
        records_path: "check.json".to_string(),
        records_json: Ok(rendered),
    })
    .records)
}

fn no_card_records(
    records: Vec<crate::output::gap_decision_ledger::GapRecord>,
) -> Vec<crate::output::gap_decision_ledger::GapRecord> {
    records
        .into_iter()
        .filter(|r| r.static_limit_kind.as_deref() == Some("python_repair_card_unavailable"))
        .collect()
}

/// #5531: the real check JSON carries the typed repo scope to the ledger, so
/// the no-repair-card record names the scope (registry-derived) with no route.
/// Diff control keeps the placement-style reason; a repo no-path finding forms
/// no such record.
#[test]
fn real_check_json_carries_repo_scope_into_ledger_reason() -> Result<(), String> {
    let scope_detail = "this Python preview finding has no repair card (repo-mode findings have no repair routing (python/any/repo/repair_routing is not_proven)), so `ripr pilot`, `ripr agent repair` and `ripr first-pr` will not route it; add or strengthen a test by hand, then rerun `ripr check`";

    let repo_json = run("ledger-repo", true, crate::OutputFormat::Json)?;
    let weak = py_findings(&repo_json)?;
    if weak.is_empty() {
        return Err("repo run produced no weak Python subject".to_string());
    }
    if weak.iter().any(|f| f["analysis_scope"] != "repo") {
        return Err(format!("repo finding JSON lacks typed scope: {weak:?}"));
    }
    let records = no_card_records(ledger_records(repo_json)?);
    if records.is_empty() {
        return Err("repo run produced no no-repair-card ledger record".to_string());
    }
    for record in &records {
        if record.static_limit_detail.as_deref() != Some(scope_detail)
            || record.repair_route.is_some()
        {
            return Err(format!("unexpected repo record: {record:?}"));
        }
    }

    let diff_json = run("ledger-diff", false, crate::OutputFormat::Json)?;
    if py_findings(&diff_json)?
        .iter()
        .any(|f| f.get("analysis_scope").is_some())
    {
        return Err("diff finding JSON must omit analysis_scope".to_string());
    }
    for record in no_card_records(ledger_records(diff_json)?) {
        if record
            .static_limit_detail
            .as_deref()
            .is_some_and(|d| d.contains("repo-mode findings"))
        {
            return Err(format!("diff record borrowed scope reason: {record:?}"));
        }
    }

    let (nopath_json, _) = run_with_test(
        "ledger-nopath",
        true,
        crate::OutputFormat::Json,
        "def test_unrelated():\n    assert True\n",
    )?;
    let value: Value = serde_json::from_str(&nopath_json).map_err(|e| e.to_string())?;
    let classes: Vec<&str> = value["findings"]
        .as_array()
        .map(|fs| {
            fs.iter()
                .filter(|f| f.to_string().contains("pricing.py"))
                .filter_map(|f| f["classification"].as_str())
                .collect()
        })
        .unwrap_or_default();
    if !classes.contains(&"no_static_path") || classes.contains(&"weakly_exposed") {
        return Err(format!("no-path fixture premise failed: {classes:?}"));
    }
    if !no_card_records(ledger_records(nopath_json)?).is_empty() {
        return Err("a no-static-path finding must form no no-repair-card record".to_string());
    }
    Ok(())
}
