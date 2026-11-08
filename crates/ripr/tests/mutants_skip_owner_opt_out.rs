//! #13819: a changed owner that carries `#[mutants::skip]` (directly or
//! through `cfg_attr`) is an explicit cargo-mutants opt-out. Its findings are
//! withheld from actionable output and disclosed with a count and the
//! `owner_mutants_skip` reason; the class never changes and nothing is
//! reclassified as `exposed`.
//!
//! Every control varies one thing about the shape below and drives the real
//! `ripr check` binary on a diff file.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_ID: AtomicU64 = AtomicU64::new(0);

const REASON: &str = "owner_mutants_skip";

/// The changed line is line 3, inside `limit`. Each attribute slot is always
/// one line (a comment when empty) so the diff stays fixed across controls.
fn free_fn_lib(limit_attr: &str, sibling_attr: &str) -> String {
    format!(
        "{limit_attr}
pub fn limit(x: u32) -> u32 {{
    if x > 100 {{ 100 }} else {{ x }}
}}

{sibling_attr}
pub fn sibling(x: u32) -> u32 {{
    x + 1
}}
"
    )
}

/// The changed line is line 5, inside the `Cart::limit` method.
fn method_lib(limit_attr: &str) -> String {
    format!(
        "pub struct Cart;

impl Cart {{
    {limit_attr}
    pub fn limit(&self, x: u32) -> u32 {{ if x > 100 {{ 100 }} else {{ x }} }}
}}
"
    )
}

fn free_fn_diff() -> String {
    one_line_diff(
        3,
        "    if x >= 100 { 100 } else { x }",
        "    if x > 100 { 100 } else { x }",
    )
}

fn method_diff() -> String {
    one_line_diff(
        5,
        "    pub fn limit(&self, x: u32) -> u32 { if x >= 100 { 100 } else { x } }",
        "    pub fn limit(&self, x: u32) -> u32 { if x > 100 { 100 } else { x } }",
    )
}

fn one_line_diff(line: usize, before: &str, after: &str) -> String {
    format!(
        "diff --git a/src/lib.rs b/src/lib.rs
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -{line},1 +{line},1 @@
-{before}
+{after}
"
    )
}

struct Repo {
    base: PathBuf,
    root: PathBuf,
}

impl Repo {
    fn new(source: &str, diff: &str) -> Result<Self, String> {
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        let base =
            std::env::temp_dir().join(format!("ripr-mutants-skip-{}-{id}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        let root = base.join("fx");
        fs::create_dir_all(root.join("src")).map_err(|e| e.to_string())?;
        write(
            &root.join("Cargo.toml"),
            "[package]\nname = \"fx\"\nversion = \"0.0.0\"\nedition = \"2021\"\n\n[workspace]\n",
        )?;
        write(&root.join("src/lib.rs"), source)?;
        write(&base.join("change.diff"), diff)?;
        Ok(Self { base, root })
    }

    fn run(&self, extra: &[&str]) -> Result<String, String> {
        let output = Command::new(env!("CARGO_BIN_EXE_ripr"))
            .args(["check", "--mode", "draft", "--root"])
            .arg(&self.root)
            .arg("--diff")
            .arg(self.base.join("change.diff"))
            .args(extra)
            .env("RIPR_CACHE_DIR", self.base.join("cache"))
            .output()
            .map_err(|e| format!("run ripr check: {e}"))?;
        if !output.status.success() {
            return Err(format!(
                "ripr check failed: {}\n{}",
                output.status,
                String::from_utf8_lossy(&output.stderr)
            ));
        }
        String::from_utf8(output.stdout).map_err(|e| e.to_string())
    }

    fn report(&self) -> Result<serde_json::Value, String> {
        serde_json::from_str(&self.run(&["--format", "json"])?)
            .map_err(|e| format!("report is not JSON: {e}"))
    }

    fn human_full(&self) -> Result<String, String> {
        self.run(&["--format", "human-full"])
    }
}

impl Drop for Repo {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.base);
    }
}

fn write(path: &Path, text: &str) -> Result<(), String> {
    fs::write(path, text).map_err(|e| format!("write {}: {e}", path.display()))
}

fn findings(report: &serde_json::Value) -> Result<&Vec<serde_json::Value>, String> {
    report["findings"]
        .as_array()
        .ok_or_else(|| format!("report has no findings array: {report}"))
}

/// Findings on the changed line, with their class and withholding reason.
fn line_findings(
    report: &serde_json::Value,
    line: u64,
) -> Result<Vec<(String, Option<String>)>, String> {
    Ok(findings(report)?
        .iter()
        .filter(|finding| finding["probe"]["line"].as_u64() == Some(line))
        .map(|finding| {
            (
                finding["classification"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string(),
                finding["withheld_by"].as_str().map(str::to_string),
            )
        })
        .collect())
}

fn class_bucket_total(summary: &serde_json::Value) -> u64 {
    [
        "exposed",
        "weakly_exposed",
        "reachable_unrevealed",
        "no_static_path",
        "infection_unknown",
        "propagation_unknown",
        "static_unknown",
    ]
    .iter()
    .map(|key| summary[*key].as_u64().unwrap_or(0))
    .sum()
}

/// Asserts the report reports `line`'s findings as ordinary, actionable
/// findings: present, none withheld, no owner opt-out disclosure.
fn assert_reported(report: &serde_json::Value, line: u64) -> Result<(), String> {
    let rows = line_findings(report, line)?;
    assert!(
        !rows.is_empty(),
        "fixture must produce a finding on line {line}: {report}"
    );
    assert!(
        rows.iter().all(|(_, withheld)| withheld.is_none()),
        "no finding may be withheld: {rows:?}"
    );
    assert!(
        report.get("owner_opt_outs").is_none(),
        "no owner opt-out disclosure expected: {report}"
    );
    assert!(
        report["summary"].get("withheld_by_owner_opt_out").is_none(),
        "no withheld summary count expected: {}",
        report["summary"]
    );
    Ok(())
}

/// Asserts every finding on `line` is withheld under `owner_mutants_skip`,
/// keeps its class, and is disclosed in the top-level object and summary.
fn assert_withheld(
    report: &serde_json::Value,
    line: u64,
    baseline_classes: &[String],
) -> Result<(), String> {
    let rows = line_findings(report, line)?;
    assert!(
        !rows.is_empty(),
        "withheld findings must stay visible in findings[]: {report}"
    );
    assert!(
        rows.iter()
            .all(|(_, withheld)| withheld.as_deref() == Some(REASON)),
        "every finding on the skipped owner must be withheld: {rows:?}"
    );
    let mut classes: Vec<String> = rows.iter().map(|(class, _)| class.clone()).collect();
    classes.sort();
    let mut expected = baseline_classes.to_vec();
    expected.sort();
    assert_eq!(
        classes, expected,
        "withholding must not change any class (never reclassified as exposed)"
    );
    assert!(
        classes.iter().all(|class| class != "exposed"),
        "fixture must withhold a non-exposed finding: {classes:?}"
    );
    let withheld = rows.len() as u64;
    assert_eq!(
        report["owner_opt_outs"]["withheld"].as_u64(),
        Some(withheld)
    );
    assert_eq!(report["owner_opt_outs"]["reason"].as_str(), Some(REASON));
    let summary = &report["summary"];
    assert_eq!(
        summary["withheld_by_owner_opt_out"].as_u64(),
        Some(withheld)
    );
    assert_eq!(
        class_bucket_total(summary) + withheld,
        summary["findings"].as_u64().unwrap_or(0),
        "class buckets plus withheld must add back up to findings: {summary}"
    );
    Ok(())
}

fn baseline_classes(line: u64) -> Result<Vec<String>, String> {
    let repo = Repo::new(&free_fn_lib("//", "//"), &free_fn_diff())?;
    Ok(line_findings(&repo.report()?, line)?
        .into_iter()
        .map(|(class, _)| class)
        .collect())
}

#[test]
fn changed_owner_without_attribute_still_reports() -> Result<(), String> {
    let repo = Repo::new(&free_fn_lib("//", "//"), &free_fn_diff())?;
    let report = repo.report()?;
    assert_reported(&report, 3)?;
    let human = repo.human_full()?;
    assert!(
        !human.contains("Withheld by owner opt-out"),
        "no withholding block expected:\n{human}"
    );
    Ok(())
}

#[test]
fn mutants_skip_on_changed_owner_withholds_and_discloses() -> Result<(), String> {
    let baseline = baseline_classes(3)?;
    let repo = Repo::new(&free_fn_lib("#[mutants::skip]", "//"), &free_fn_diff())?;
    let report = repo.report()?;
    assert_withheld(&report, 3, &baseline)?;

    let human = repo.human_full()?;
    assert!(
        human.contains(&format!(
            "Withheld by owner opt-out ({REASON}): {} finding(s)",
            baseline.len()
        )),
        "human output must disclose the count and reason:\n{human}"
    );
    assert!(
        human.contains(&format!("owner: limit, reason: {REASON}")),
        "human output must name the owner:\n{human}"
    );
    assert!(
        human.contains(&format!("withheld by owner opt-out ({REASON})")),
        "the Summary line must name the withholding:\n{human}"
    );
    Ok(())
}

#[test]
fn cfg_attr_mutants_skip_withholds() -> Result<(), String> {
    let baseline = baseline_classes(3)?;
    let repo = Repo::new(
        &free_fn_lib("#[cfg_attr(test, mutants::skip)]", "//"),
        &free_fn_diff(),
    )?;
    assert_withheld(&repo.report()?, 3, &baseline)
}

#[test]
fn unrelated_inline_attribute_does_not_withhold() -> Result<(), String> {
    let repo = Repo::new(&free_fn_lib("#[inline]", "//"), &free_fn_diff())?;
    assert_reported(&repo.report()?, 3)
}

#[test]
fn mutants_skip_on_sibling_does_not_withhold_changed_owner() -> Result<(), String> {
    let repo = Repo::new(&free_fn_lib("//", "#[mutants::skip]"), &free_fn_diff())?;
    assert_reported(&repo.report()?, 3)
}

#[test]
fn mutants_skip_on_impl_method_withholds() -> Result<(), String> {
    let plain = Repo::new(&method_lib("//"), &method_diff())?;
    let plain_report = plain.report()?;
    assert_reported(&plain_report, 5)?;
    let baseline: Vec<String> = line_findings(&plain_report, 5)?
        .into_iter()
        .map(|(class, _)| class)
        .collect();

    let skipped = Repo::new(&method_lib("#[mutants::skip]"), &method_diff())?;
    assert_withheld(&skipped.report()?, 5, &baseline)
}

/// A finding an explicit `--suppression-policy` already suppressed is not
/// withheld a second time: it is disclosed once, under the policy, and the
/// class buckets are decremented once.
#[test]
fn policy_suppression_wins_without_double_counting() -> Result<(), String> {
    let repo = Repo::new(&free_fn_lib("#[mutants::skip]", "//"), &free_fn_diff())?;
    let policy = repo.base.join("suppressions.toml");
    write(
        &policy,
        "schema_version = 1\n\n[[suppressions]]\nkind = \"exposure_gap\"\npath = \"src/**\"\nreason = \"accepted\"\nowner = \"fixture\"\n",
    )?;
    let policy_arg = policy.to_string_lossy().into_owned();
    let report: serde_json::Value = serde_json::from_str(&repo.run(&[
        "--format",
        "json",
        "--suppression-policy",
        &policy_arg,
    ])?)
    .map_err(|e| format!("report is not JSON: {e}"))?;
    let rows = line_findings(&report, 3)?;
    assert!(
        !rows.is_empty(),
        "premise: a finding on the owner: {report}"
    );
    assert!(
        rows.iter().all(|(_, withheld)| withheld.is_none()),
        "a policy-suppressed finding is not also withheld: {rows:?}"
    );
    assert!(report.get("owner_opt_outs").is_none(), "{report}");
    let summary = &report["summary"];
    let suppressed = summary["suppressed_by_policy"].as_u64().unwrap_or(0);
    assert_eq!(suppressed, rows.len() as u64, "{summary}");
    assert_eq!(
        class_bucket_total(summary) + suppressed,
        summary["findings"].as_u64().unwrap_or(0),
        "{summary}"
    );
    Ok(())
}

/// The emitted additive members must be declared by the published check
/// schema, whose top level, `summary` and `finding` are closed.
#[test]
fn withheld_members_are_declared_by_the_check_schema() -> Result<(), String> {
    let report = Repo::new(&free_fn_lib("#[mutants::skip]", "//"), &free_fn_diff())?.report()?;
    let schema_path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas/ripr/check.schema.json");
    let schema: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(&schema_path).map_err(|e| format!("{}: {e}", schema_path.display()))?,
    )
    .map_err(|e| e.to_string())?;
    let declared = |node: &serde_json::Value| -> Result<Vec<String>, String> {
        assert_eq!(
            node["additionalProperties"], false,
            "premise: closed object"
        );
        Ok(node["properties"]
            .as_object()
            .ok_or("properties")?
            .keys()
            .cloned()
            .collect())
    };
    let top = declared(&schema)?;
    assert!(top.contains(&"owner_opt_outs".to_string()));
    for key in report.as_object().ok_or("report")?.keys() {
        assert!(top.contains(key), "{key} not in check schema");
    }
    let summary = declared(&schema["$defs"]["summary"])?;
    for key in report["summary"].as_object().ok_or("summary")?.keys() {
        assert!(summary.contains(key), "summary.{key} not in check schema");
    }
    let finding = declared(&schema["$defs"]["finding"])?;
    let first = findings(&report)?.first().ok_or("premise: a finding")?;
    assert_eq!(first["withheld_by"], REASON, "premise: {first}");
    for key in first.as_object().ok_or("finding")?.keys() {
        assert!(finding.contains(key), "finding.{key} not in check schema");
    }
    assert_eq!(
        schema["properties"]["owner_opt_outs"]["properties"]["reason"]["enum"][0],
        REASON
    );
    Ok(())
}
