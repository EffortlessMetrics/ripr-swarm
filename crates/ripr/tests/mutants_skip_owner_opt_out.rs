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

/// Devin on #13874: a raw-string doc that merely spells `mutants::skip`
/// between quote characters opts nothing out. The changed owner's findings
/// still report in JSON.
#[test]
fn raw_string_doc_mentioning_mutants_skip_does_not_withhold() -> Result<(), String> {
    let repo = Repo::new(
        &free_fn_lib(
            r##"#[cfg_attr(test, doc = r#"note ", mutants::skip, " text"#)]"##,
            "//",
        ),
        &free_fn_diff(),
    )?;
    assert_reported(&repo.report()?, 3)
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

/// Both owners change: `limit` on line 3 and `sibling` on line 8.
fn mixed_diff() -> String {
    "diff --git a/src/lib.rs b/src/lib.rs
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -3,1 +3,1 @@
-    if x >= 100 { 100 } else { x }
+    if x > 100 { 100 } else { x }
@@ -8,1 +8,1 @@
-    x + 2
+    x + 1
"
    .to_string()
}

/// A run that changes a skipped owner and an ordinary one withholds only the
/// skipped owner's findings; the other owner keeps reporting.
#[test]
fn mixed_run_withholds_only_the_skipped_owner() -> Result<(), String> {
    let plain = Repo::new(&free_fn_lib("//", "//"), &mixed_diff())?;
    let plain_report = plain.report()?;
    let limit_baseline: Vec<String> = line_findings(&plain_report, 3)?
        .into_iter()
        .map(|(class, _)| class)
        .collect();
    let sibling_baseline = line_findings(&plain_report, 8)?;
    assert!(
        !limit_baseline.is_empty() && !sibling_baseline.is_empty(),
        "premise: both changed owners produce findings: {plain_report}"
    );

    let repo = Repo::new(&free_fn_lib("#[mutants::skip]", "//"), &mixed_diff())?;
    let report = repo.report()?;
    let limit_rows = line_findings(&report, 3)?;
    assert!(
        limit_rows
            .iter()
            .all(|(_, withheld)| withheld.as_deref() == Some(REASON)),
        "the skipped owner's findings are withheld: {limit_rows:?}"
    );
    let sibling_rows = line_findings(&report, 8)?;
    assert_eq!(
        sibling_rows, sibling_baseline,
        "the ordinary owner's findings are unchanged and not withheld"
    );
    assert_eq!(
        report["owner_opt_outs"]["withheld"].as_u64(),
        Some(limit_baseline.len() as u64),
        "only the skipped owner is counted: {report}"
    );
    let summary = &report["summary"];
    assert_eq!(
        class_bucket_total(summary) + limit_baseline.len() as u64,
        summary["findings"].as_u64().unwrap_or(0),
        "{summary}"
    );
    Ok(())
}

/// A same-named function in a nested module carrying the skip does not
/// withhold the changed top-level owner.
#[test]
fn same_name_fn_in_module_does_not_withhold_changed_owner() -> Result<(), String> {
    let source = format!(
        "{}\npub mod a {{\n    #[mutants::skip]\n    pub fn limit(x: u32) -> u32 {{ x }}\n}}\n",
        free_fn_lib("//", "//")
    );
    let repo = Repo::new(&source, &free_fn_diff())?;
    assert_reported(&repo.report()?, 3)
}

/// A same-named skipped method does not withhold the changed free function.
#[test]
fn same_name_skipped_method_does_not_withhold_changed_free_fn() -> Result<(), String> {
    let source = format!(
        "{}\npub struct X;\n\nimpl X {{\n    #[mutants::skip]\n    pub fn limit(&self, x: u32) -> u32 {{ x }}\n}}\n",
        free_fn_lib("//", "//")
    );
    let repo = Repo::new(&source, &free_fn_diff())?;
    assert_reported(&repo.report()?, 3)
}

/// Extracts `line=N` from every GitHub annotation on `src/lib.rs`.
fn annotated_lines(github: &str) -> Vec<u64> {
    github
        .lines()
        .filter(|line| line.contains("file=src/lib.rs"))
        .filter_map(|line| {
            let rest = line.split("line=").nth(1)?;
            rest.split(|c: char| !c.is_ascii_digit())
                .next()?
                .parse()
                .ok()
        })
        .collect()
}

/// GitHub output skips the skipped owner's annotations, keeps the other
/// owner's, and names the withholding in the denominator notice.
#[test]
fn github_skips_withheld_annotations_and_discloses_them() -> Result<(), String> {
    let plain = Repo::new(&free_fn_lib("//", "//"), &mixed_diff())?;
    let plain_lines = annotated_lines(&plain.run(&["--format", "github"])?);
    assert!(
        plain_lines.contains(&3) && plain_lines.contains(&8),
        "premise: both owners are annotated without the attribute: {plain_lines:?}"
    );
    let limit_count = line_findings(&plain.report()?, 3)?.len();

    let repo = Repo::new(&free_fn_lib("#[mutants::skip]", "//"), &mixed_diff())?;
    let github = repo.run(&["--format", "github"])?;
    let lines = annotated_lines(&github);
    assert!(
        !lines.contains(&3),
        "no annotation for the withheld owner:\n{github}"
    );
    assert!(
        lines.contains(&8),
        "the ordinary owner stays annotated:\n{github}"
    );
    assert!(
        github.contains(&format!(
            "{limit_count} withheld by owner opt-out ({REASON}: the changed function carries #[mutants::skip])"
        )),
        "the denominator notice names the count and reason:\n{github}"
    );
    Ok(())
}

/// `missed-txt` lists no line for the skipped owner, keeps the other
/// owner's lines, and names the withholding in its leading comment.
#[test]
fn missed_txt_skips_withheld_lines_and_discloses_them() -> Result<(), String> {
    let gap_lines = |text: &str| -> Vec<u64> {
        text.lines()
            .filter(|line| !line.starts_with("# "))
            .filter_map(|line| line.strip_prefix("src/lib.rs:"))
            .filter_map(|rest| rest.split(':').next()?.parse().ok())
            .collect()
    };
    let plain = Repo::new(&free_fn_lib("//", "//"), &mixed_diff())?;
    let plain_lines = gap_lines(&plain.run(&["--format", "missed-txt"])?);
    assert!(
        plain_lines.contains(&3) && plain_lines.contains(&8),
        "premise: both owners are listed without the attribute: {plain_lines:?}"
    );
    let limit_count = line_findings(&plain.report()?, 3)?.len();

    let repo = Repo::new(&free_fn_lib("#[mutants::skip]", "//"), &mixed_diff())?;
    let text = repo.run(&["--format", "missed-txt"])?;
    let lines = gap_lines(&text);
    assert!(
        !lines.contains(&3),
        "no line for the withheld owner:\n{text}"
    );
    assert!(
        lines.contains(&8),
        "the ordinary owner stays listed:\n{text}"
    );
    assert!(
        text.contains(&format!(
            "{limit_count} withheld by owner opt-out ({REASON}: the changed function carries #[mutants::skip])"
        )),
        "the leading comment names the count and reason:\n{text}"
    );
    Ok(())
}

/// SARIF emits no result for a withheld finding, keeps the other owner's
/// results, and discloses the count in the run properties.
#[test]
fn sarif_drops_withheld_results_and_discloses_them() -> Result<(), String> {
    let result_lines = |sarif: &serde_json::Value| -> Vec<u64> {
        sarif["runs"][0]["results"]
            .as_array()
            .map(|results| {
                results
                    .iter()
                    .filter_map(|result| {
                        result["locations"][0]["physicalLocation"]["region"]["startLine"].as_u64()
                    })
                    .collect()
            })
            .unwrap_or_default()
    };
    let parse = |text: String| -> Result<serde_json::Value, String> {
        serde_json::from_str(&text).map_err(|e| format!("SARIF is not JSON: {e}"))
    };
    let plain = Repo::new(&free_fn_lib("//", "//"), &mixed_diff())?;
    let plain_sarif = parse(plain.run(&["--format", "sarif"])?)?;
    let plain_lines = result_lines(&plain_sarif);
    assert!(
        plain_lines.contains(&3) && plain_lines.contains(&8),
        "premise: both owners have SARIF results: {plain_sarif}"
    );
    assert!(
        plain_sarif["runs"][0]["properties"]
            .get("withheld_by_owner_opt_out")
            .is_none(),
        "no disclosure without the attribute"
    );
    let limit_count = line_findings(&plain.report()?, 3)?.len() as u64;

    let repo = Repo::new(&free_fn_lib("#[mutants::skip]", "//"), &mixed_diff())?;
    let sarif = parse(repo.run(&["--format", "sarif"])?)?;
    let lines = result_lines(&sarif);
    assert!(
        !lines.contains(&3),
        "no result for the withheld owner: {sarif}"
    );
    assert!(
        lines.contains(&8),
        "the ordinary owner keeps results: {sarif}"
    );
    assert_eq!(
        sarif["runs"][0]["properties"]["withheld_by_owner_opt_out"].as_u64(),
        Some(limit_count),
        "{}",
        sarif["runs"][0]["properties"]
    );
    Ok(())
}

/// The diff-scope badge does not count a withheld finding as a gap.
#[test]
fn badge_does_not_count_withheld_findings() -> Result<(), String> {
    let headline = |repo: &Repo| -> Result<(u64, u64), String> {
        let badge: serde_json::Value =
            serde_json::from_str(&repo.run(&["--format", "badge-json"])?)
                .map_err(|e| format!("badge is not JSON: {e}"))?;
        let gaps = badge["counts"]["unsuppressed_exposure_gaps"]
            .as_u64()
            .ok_or_else(|| format!("no gap count: {badge}"))?;
        let message = badge["message"]
            .as_str()
            .and_then(|m| m.parse().ok())
            .ok_or_else(|| format!("non-numeric message: {badge}"))?;
        Ok((gaps, message))
    };
    let (plain_gaps, plain_headline) =
        headline(&Repo::new(&free_fn_lib("//", "//"), &free_fn_diff())?)?;
    assert!(
        plain_gaps > 0,
        "premise: the changed owner is an exposure gap without the attribute"
    );
    assert!(plain_headline > 0);
    let (gaps, message) = headline(&Repo::new(
        &free_fn_lib("#[mutants::skip]", "//"),
        &free_fn_diff(),
    )?)?;
    assert_eq!(gaps, 0, "a withheld finding is not an unsuppressed gap");
    assert_eq!(message, 0, "the badge headline drops the withheld gap");
    Ok(())
}
