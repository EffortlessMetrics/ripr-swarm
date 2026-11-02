//! RIPR-SPEC-0367: a Kani proof harness is verification source. It is not a
//! test, earns no credit, seeds no production probe, and is disclosed with
//! `proof_harness_not_evaluated` instead of the generic "add a test" advice.
//!
//! Every control varies one thing about the shape below and drives the real
//! `ripr check` binary on a diff file.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_ID: AtomicU64 = AtomicU64::new(0);

const LIMIT: &str = "proof_harness_not_evaluated";

/// `limit` is line 2; the harness assertion is line 12.
fn lib(proof_attr: &str, limit_cmp: &str, harness_cmp: &str, tests: &str) -> String {
    format!(
        "pub fn limit(x: u32) -> u32 {{
    if x {limit_cmp} 100 {{ 100 }} else {{ x }}
}}

#[cfg(kani)]
mod verification {{
    use super::limit;

    #[{proof_attr}]
    fn limit_never_exceeds() {{
        let x: u32 = kani::any();
        assert!(limit(x) <= {harness_cmp});
    }}
}}
{tests}"
    )
}

const TEST_MODULE: &str = "
#[cfg(test)]
mod tests {
    use super::limit;

    #[test]
    fn caps_at_one_hundred() {
        assert_eq!(limit(150), 100);
    }
}
";

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
            std::env::temp_dir().join(format!("ripr-proof-harness-{}-{id}", std::process::id()));
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

    fn report(&self) -> Result<serde_json::Value, String> {
        let output = Command::new(env!("CARGO_BIN_EXE_ripr"))
            .args(["check", "--mode", "draft", "--root"])
            .arg(&self.root)
            .arg("--diff")
            .arg(self.base.join("change.diff"))
            .args(["--format", "json"])
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
        serde_json::from_slice(&output.stdout).map_err(|e| format!("report is not JSON: {e}"))
    }

    fn native_report(&self) -> Result<serde_json::Value, String> {
        write(
            &self.root.join("ripr.toml"),
            "[languages]\nenabled = ['rust']\n\n[languages.rust.kani]\nprofile = 'kani_ordinary_proof_v1'\nsource = 'src/lib.rs'\ntests = false\n",
        )?;
        let report = self.report()?;
        assert_eq!(
            report["kani_source_inventory"]["harnesses"]
                .as_array()
                .map(Vec::len),
            Some(1),
            "premise: one harness was admitted through the declared source context: {report}"
        );
        Ok(report)
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

fn run_limitation_details(report: &serde_json::Value) -> Vec<String> {
    report["analysis_outcome"]["outcome"]["limitations"]
        .as_array()
        .map(|limitations| {
            limitations
                .iter()
                .filter_map(|l| l["bounded_detail"].as_str().map(str::to_string))
                .filter(|detail| detail.contains(LIMIT))
                .collect()
        })
        .unwrap_or_default()
}

/// Changing the owner a proof harness calls, with no test: the finding stays
/// `no_static_path`, carries the typed limit, and the next step says a proof
/// harness exists that ripr does not evaluate rather than "add a co-located
/// test".
#[test]
fn owner_called_by_a_proof_harness_is_limited_not_credited() -> Result<(), String> {
    let source = lib("kani::proof", ">", "100", "");
    let repo = Repo::new(
        &source,
        &one_line_diff(
            2,
            "    if x >= 100 { 100 } else { x }",
            "    if x > 100 { 100 } else { x }",
        ),
    )?;
    let report = repo.report()?;
    let on_owner: Vec<_> = findings(&report)?
        .iter()
        .filter(|f| f["probe"]["line"] == 2)
        .collect();
    assert!(
        !on_owner.is_empty(),
        "premise: a finding on the owner: {report}"
    );
    for finding in on_owner {
        assert_eq!(finding["classification"], "no_static_path", "{finding}");
        assert_eq!(finding["static_limit_kind"], LIMIT, "{finding}");
        let step = finding["recommended_next_step"]
            .as_str()
            .unwrap_or_default();
        assert!(
            step.contains("proof harness `limit_never_exceeds`"),
            "{step}"
        );
        assert!(step.contains("does not evaluate proofs"), "{step}");
        assert!(!step.contains("co-located test"), "{step}");
        assert!(
            finding["related_tests"]
                .as_array()
                .is_none_or(|related| related.is_empty()),
            "a harness is not a related test: {finding}"
        );
    }
    Ok(())
}

/// Editing only the harness: no production probe, and the run names the
/// unevaluated harness instead of reading as a clean zero-finding run.
#[test]
fn harness_only_edit_seeds_no_probe_and_names_the_limitation() -> Result<(), String> {
    let source = lib("kani::proof", ">", "101", "");
    let repo = Repo::new(
        &source,
        &one_line_diff(
            12,
            "        assert!(limit(x) <= 100);",
            "        assert!(limit(x) <= 101);",
        ),
    )?;
    let report = repo.report()?;
    assert!(
        findings(&report)?.is_empty(),
        "a harness edit must seed no production probe: {report}"
    );
    let details = run_limitation_details(&report);
    assert_eq!(details.len(), 1, "{report}");
    assert_eq!(
        report["analysis_outcome"]["analysis_complete"], false,
        "{report}"
    );
    Ok(())
}

/// The same edit under a lookalike attribute path stays an ordinary
/// production function: exact-path matching only. The module gate is swapped
/// for a non-Kani feature: a `#[cfg(kani)]` module is verification-only on its
/// own (RIPR-SPEC-0373), whatever its inner attributes say.
#[test]
fn lookalike_attribute_path_is_not_a_proof_harness() -> Result<(), String> {
    let source = lib("my::kani::proof", ">", "101", "")
        .replace("#[cfg(kani)]", "#[cfg(feature = \"proofs\")]");
    let repo = Repo::new(
        &source,
        &one_line_diff(
            12,
            "        assert!(limit(x) <= 100);",
            "        assert!(limit(x) <= 101);",
        ),
    )?;
    let report = repo.report()?;
    assert!(
        !findings(&report)?.is_empty(),
        "a lookalike harness stays production-eligible: {report}"
    );
    assert!(run_limitation_details(&report).is_empty(), "{report}");
    Ok(())
}

/// A real test beside the harness decides the verdict; the harness adds
/// nothing and the proof limit does not appear.
#[test]
fn a_test_path_wins_over_the_harness() -> Result<(), String> {
    let source = lib("kani::proof", ">", "100", TEST_MODULE);
    let repo = Repo::new(
        &source,
        &one_line_diff(
            2,
            "    if x >= 100 { 100 } else { x }",
            "    if x > 100 { 100 } else { x }",
        ),
    )?;
    let report = repo.report()?;
    let on_owner: Vec<_> = findings(&report)?
        .iter()
        .filter(|f| f["probe"]["line"] == 2)
        .collect();
    assert!(
        !on_owner.is_empty(),
        "premise: a finding on the owner: {report}"
    );
    for finding in on_owner {
        assert_ne!(finding["classification"], "no_static_path", "{finding}");
        assert_ne!(finding["static_limit_kind"], LIMIT, "{finding}");
    }
    Ok(())
}

/// Admitting a native context refines source identity without losing the
/// unconditional proof disclosure or letting a harness replace a real test.
#[test]
fn declared_context_keeps_proof_disclosure_and_real_test_priority() -> Result<(), String> {
    for tests in ["", TEST_MODULE] {
        let source = lib("kani::proof", ">", "100", tests);
        let repo = Repo::new(
            &source,
            &one_line_diff(
                2,
                "    if x >= 100 { 100 } else { x }",
                "    if x > 100 { 100 } else { x }",
            ),
        )?;
        let report = repo.native_report()?;
        let on_owner: Vec<_> = findings(&report)?
            .iter()
            .filter(|finding| finding["probe"]["line"] == 2)
            .collect();
        assert!(!on_owner.is_empty(), "premise: changed owner: {report}");
        for finding in on_owner {
            if tests.is_empty() {
                assert_eq!(finding["classification"], "no_static_path", "{finding}");
                assert_eq!(finding["static_limit_kind"], LIMIT, "{finding}");
                let step = finding["recommended_next_step"]
                    .as_str()
                    .unwrap_or_default();
                assert!(
                    step.contains("proof harness `limit_never_exceeds`"),
                    "{step}"
                );
                assert!(step.contains("does not evaluate proofs"), "{step}");
                assert!(
                    finding["related_tests"]
                        .as_array()
                        .is_none_or(|related| related.is_empty()),
                    "native source is not ordinary test evidence: {finding}"
                );
            } else {
                assert_ne!(finding["classification"], "no_static_path", "{finding}");
                assert_ne!(finding["static_limit_kind"], LIMIT, "{finding}");
                assert!(
                    finding["related_tests"].as_array().is_some_and(|related| {
                        related
                            .iter()
                            .any(|test| test["name"] == "caps_at_one_hundred")
                    }),
                    "premise: ordinary test evidence remains present: {finding}"
                );
            }
        }
    }
    Ok(())
}

/// Nested locals stay source-only; edits disclose their enclosing proof,
/// without presenting the local function as another native harness.
#[test]
fn declared_context_keeps_harness_and_local_edits_out_of_production() -> Result<(), String> {
    let source = lib("kani::proof", ">", "101", "").replace(
        "        let x: u32 = kani::any();",
        "        fn local(x: u32) -> bool { x < 10 }\n        let x: u32 = kani::any();\n        let _ = local(x);",
    );
    for (before, after) in [
        (
            "        assert!(limit(x) <= 100);",
            "        assert!(limit(x) <= 101);",
        ),
        (
            "        fn local(x: u32) -> bool { x < 9 }",
            "        fn local(x: u32) -> bool { x < 10 }",
        ),
    ] {
        let line = source
            .lines()
            .position(|line| line == after)
            .ok_or("changed source line missing")?
            + 1;
        let repo = Repo::new(&source, &one_line_diff(line, before, after))?;
        let report = repo.native_report()?;
        assert!(
            findings(&report)?.is_empty(),
            "verification source must seed no production probe: {report}"
        );
        assert_eq!(run_limitation_details(&report).len(), 1, "{report}");
        assert_eq!(
            report["analysis_outcome"]["analysis_complete"], false,
            "{report}"
        );
    }
    Ok(())
}
