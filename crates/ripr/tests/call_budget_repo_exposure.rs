//! `repo-exposure-json` controls for the per-line call-fact budget (#8600).
//!
//! The diff check withholds a negative claim over a related test whose line
//! holds 256 distinct callees under the typed `rust_call_fact_budget` limit.
//! The repo seam reading is a separate producer, so these controls run the
//! built binary with the real extractor and distinct callee names, then read
//! the seam record a consumer reads: grip class, actionability, canonical item
//! kind, repair command and static limitations. The line carries one owner
//! call and one assertion macro (not a call fact), so 254 helpers leave the
//! line one fact under the budget and 255 reach it.

use serde_json::Value;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

const OWNER: &str = "pub fn row(index: usize) -> usize {
    if index >= 100 { return 100; }
    index / 4
}
";

const BUDGET: &str = "rust_call_fact_budget";

struct Repo {
    root: PathBuf,
}

impl Repo {
    fn create(test_body: &str, extra: Option<(&str, &str)>) -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "ripr-call-budget-repo-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(root.join("src"))
            .map_err(|error| format!("create source directory failed: {error}"))?;
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"call-budget\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[workspace]\n",
        )
        .map_err(|error| format!("write Cargo.toml failed: {error}"))?;
        let helpers: String = (0..400).map(|i| format!("fn h{i}() {{}}\n")).collect();
        let lib = format!(
            "{OWNER}
#[cfg(test)]
mod tests {{
    use super::*;

    #[test]
    fn reaches() {{
        {test_body}
    }}
}}

{helpers}"
        );
        std::fs::write(root.join("src/lib.rs"), lib)
            .map_err(|error| format!("write source failed: {error}"))?;
        if let Some((path, text)) = extra {
            let file = root.join(path);
            if let Some(parent) = file.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|error| format!("create {path} directory failed: {error}"))?;
            }
            std::fs::write(&file, text).map_err(|error| format!("write {path} failed: {error}"))?;
        }
        Ok(Self { root })
    }

    fn run(&self, format: &str) -> Result<Value, String> {
        let mut command = Command::new(env!("CARGO_BIN_EXE_ripr"));
        command
            .args(["check", "--root"])
            .arg(&self.root)
            .args(["--mode", "instant", "--format", format, "--quiet"])
            .env("RIPR_CACHE_DIR", self.root.join("cache"));
        if format == "json" {
            let diff = self.root.join("diff.patch");
            std::fs::write(
                &diff,
                "diff --git a/src/lib.rs b/src/lib.rs
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -1,3 +1,3 @@
 pub fn row(index: usize) -> usize {
-    if index > 99 { return 100; }
+    if index >= 100 { return 100; }
     index / 4
",
            )
            .map_err(|error| format!("write diff failed: {error}"))?;
            command.arg("--diff").arg(&diff);
        }
        let output = command
            .output()
            .map_err(|error| format!("run ripr failed: {error}"))?;
        if !matches!(output.status.code(), Some(0 | 1)) {
            return Err(format!(
                "instrument: ripr exited {:?}: {}",
                output.status,
                String::from_utf8_lossy(&output.stderr)
            ));
        }
        serde_json::from_slice(&output.stdout)
            .map_err(|error| format!("instrument: stdout is not JSON: {error}"))
    }

    /// The repo-exposure seam record of kind `kind` on `line` of `row`.
    fn seam(&self, kind: &str, line: u64) -> Result<Value, String> {
        seam_in(&self.run("repo-exposure-json")?, kind, line)
    }
}

impl Drop for Repo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn seam_in(document: &Value, kind: &str, line: u64) -> Result<Value, String> {
    let seams = document
        .get("seams")
        .and_then(Value::as_array)
        .ok_or_else(|| "instrument: repo exposure has no seams array".to_string())?;
    let mut found = seams.iter().filter(|seam| {
        seam.get("owner")
            .and_then(Value::as_str)
            .is_some_and(|owner| owner.ends_with("::row"))
            && seam.get("kind").and_then(Value::as_str) == Some(kind)
            && seam.get("line").and_then(Value::as_u64) == Some(line)
    });
    match (found.next(), found.next()) {
        (Some(seam), None) => Ok(seam.clone()),
        other => Err(format!(
            "premise: exactly one `{kind}` seam on line {line}, got {other:?}"
        )),
    }
}

fn text<'a>(seam: &'a Value, pointer: &str) -> Result<&'a str, String> {
    seam.pointer(pointer)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("missing string at {pointer} in {seam}"))
}

fn categories(seam: &Value) -> Result<Vec<String>, String> {
    seam.pointer("/evidence_record/static_limitations")
        .and_then(Value::as_array)
        .ok_or_else(|| "missing static_limitations".to_string())?
        .iter()
        .map(|limitation| {
            limitation
                .get("category")
                .and_then(Value::as_str)
                .map(str::to_string)
                .ok_or_else(|| format!("limitation without category: {limitation}"))
        })
        .collect()
}

fn is_actionable(seam: &Value) -> Result<bool, String> {
    Ok(text(seam, "/evidence_record/actionability/class")?.starts_with("actionable_"))
}

fn helpers(count: usize) -> String {
    (0..count).map(|i| format!("h{i}();")).collect()
}

/// A repair-ready seam beside a budget line is withheld; the same seam one
/// fact under the budget is not. Each body is a premise-checked baseline gap.
#[test]
fn a_saturated_related_line_withholds_the_repair_gap_under_the_typed_reason() -> Result<(), String>
{
    for (label, owner_call, baseline_class, baseline_grip) in [
        (
            "equality oracle without the boundary value",
            "assert_eq!(row(5), 1);",
            "actionable_related_test_extension",
            "weakly_gripped",
        ),
        (
            "weak oracle",
            "assert!(row(5) > 0);",
            "actionable_assertion_upgrade",
            "weakly_gripped",
        ),
    ] {
        let control = Repo::create(&format!("{owner_call}{}", helpers(254)), None)?
            .seam("predicate_boundary", 2)?;
        assert_eq!(text(&control, "/grip_class")?, baseline_grip, "{label}");
        assert_eq!(
            text(&control, "/evidence_record/actionability/class")?,
            baseline_class,
            "premise ({label}): the unsaturated seam is repair-ready"
        );
        assert!(
            !control
                .pointer("/evidence_record/canonical_item/repair_command")
                .is_none_or(Value::is_null),
            "premise ({label}): the baseline carries a repair command"
        );
        assert!(
            !categories(&control)?.iter().any(|c| c == BUDGET),
            "{label}"
        );
        for noise in [255, 299] {
            let seam = Repo::create(&format!("{owner_call}{}", helpers(noise)), None)?
                .seam("predicate_boundary", 2)?;
            assert_eq!(
                text(&seam, "/grip_class")?,
                "discrimination_unknown",
                "{label} {noise}"
            );
            assert!(!is_actionable(&seam)?, "{label} {noise}: {seam}");
            assert_eq!(
                text(&seam, "/evidence_record/actionability/class")?,
                "static_limitation",
                "{label} {noise}"
            );
            assert_eq!(
                text(&seam, "/evidence_record/canonical_item/canonical_item_kind")?,
                "limitation",
                "{label} {noise}"
            );
            assert!(
                seam.pointer("/evidence_record/canonical_item/repair_command")
                    .is_none_or(Value::is_null),
                "{label} {noise}: a withheld seam has no repair command"
            );
            assert!(
                categories(&seam)?.iter().any(|c| c == BUDGET),
                "{label} {noise}: {:?}",
                categories(&seam)?
            );
            assert_eq!(
                seam.pointer("/missing_discriminators")
                    .and_then(Value::as_array)
                    .map(Vec::len),
                Some(0),
                "{label} {noise}: a boundary value claimed over a dropped call"
            );
        }
    }
    Ok(())
}

/// An already-unknown seam keeps an unknown class and gains the typed reason
/// beside its own; the gap class it would otherwise read as is withheld.
#[test]
fn an_already_unknown_seam_gains_the_typed_reason() -> Result<(), String> {
    let control = Repo::create(&format!("let _ = row(100);{}", helpers(254)), None)?
        .seam("predicate_boundary", 2)?;
    assert_eq!(
        text(&control, "/grip_class")?,
        "reachable_unrevealed",
        "premise"
    );
    assert_eq!(
        categories(&control)?,
        vec!["propagation_static_unknown".to_string()],
        "premise: the baseline is already limited by another reason"
    );
    let seam = Repo::create(&format!("let _ = row(100);{}", helpers(255)), None)?
        .seam("predicate_boundary", 2)?;
    assert_eq!(text(&seam, "/grip_class")?, "propagation_unknown");
    let categories = categories(&seam)?;
    assert!(categories.iter().any(|c| c == "propagation_static_unknown"));
    assert!(categories.iter().any(|c| c == BUDGET), "{categories:?}");
    assert!(!is_actionable(&seam)?);
    Ok(())
}

/// Saturation that does not limit the proof keeps every finding: repeated
/// names are one fact each, a saturated test of another file that never
/// reaches the owner is not related, and an established positive stays.
#[test]
fn saturation_that_does_not_limit_the_proof_keeps_the_seam() -> Result<(), String> {
    let repeated = format!("assert_eq!(row(5), 1);{}", "h0();".repeat(2000));
    let seam = Repo::create(&repeated, None)?.seam("predicate_boundary", 2)?;
    assert!(is_actionable(&seam)?, "repeated names are one fact: {seam}");
    let unrelated = format!("#[test]\nfn unrelated() {{\n    {}\n}}\n", helpers(300));
    let seam = Repo::create(
        "assert_eq!(row(5), 1);",
        Some(("tests/other.rs", &unrelated)),
    )?
    .seam("predicate_boundary", 2)?;
    assert!(
        is_actionable(&seam)?,
        "premise or unrelated test limited it: {seam}"
    );
    // The strongly gripped return-value seam has an established positive: a
    // saturated line beside it cannot turn it into a gap or an unknown.
    for noise in [10, 299] {
        let seam = Repo::create(&format!("assert_eq!(row(5), 1);{}", helpers(noise)), None)?
            .seam("return_value", 3)?;
        assert_eq!(text(&seam, "/grip_class")?, "strongly_gripped", "{noise}");
        assert!(!categories(&seam)?.iter().any(|c| c == BUDGET), "{noise}");
    }
    Ok(())
}

/// A direct owner call with an unresolved argument leaves activation unknown
/// beside a missing boundary value; clearing that value must still name the
/// typed reason on the existing unknown stage (the category was unreachable
/// when the early return skipped it).
#[test]
fn clearing_a_missing_value_beside_an_unknown_stage_still_names_the_reason() -> Result<(), String> {
    let body = |noise: usize| {
        format!(
            "let n = compute(); assert_eq!(row(n), 25);{}",
            (1..=noise).map(|i| format!("h{i}();")).collect::<String>()
        )
    };
    let control = Repo::create(&body(10), None)?.seam("predicate_boundary", 2)?;
    assert!(
        !categories(&control)?.iter().any(|c| c == BUDGET),
        "premise: {control}"
    );
    let seam = Repo::create(&body(299), None)?.seam("predicate_boundary", 2)?;
    assert_eq!(
        text(&seam, "/grip_class")?,
        text(&control, "/grip_class")?,
        "the reason never changes an unknown class: {seam}"
    );
    assert!(
        categories(&seam)?.iter().any(|c| c == BUDGET),
        "{:?}",
        categories(&seam)?
    );
    Ok(())
}

/// The compact reading behind `repo-exposure-summary-json` is a second
/// producer of the same evidence; its counts must withhold too.
#[test]
fn the_summary_reading_withholds_the_same_seam() -> Result<(), String> {
    let counts = |noise: usize| -> Result<(u64, u64), String> {
        let repo = Repo::create(&format!("assert_eq!(row(5), 1);{}", helpers(noise)), None)?;
        let summary = repo.run("repo-exposure-summary-json")?;
        let count = |pointer: &str| {
            summary
                .pointer(pointer)
                .and_then(Value::as_u64)
                .ok_or_else(|| format!("summary is missing {pointer}"))
        };
        Ok((
            count("/metrics/grip_class/weakly_gripped")?,
            count("/metrics/grip_class/discrimination_unknown")?,
        ))
    };
    assert_eq!(counts(254)?, (1, 0), "premise: the baseline is a weak grip");
    assert_eq!(counts(255)?, (0, 1));
    Ok(())
}

/// A warm rerun over the same cache returns the withheld seam, and the diff
/// check on the same tree names the same typed reason.
#[test]
fn warm_and_diff_readings_agree_on_the_typed_reason() -> Result<(), String> {
    let repo = Repo::create(&format!("assert_eq!(row(5), 1);{}", helpers(299)), None)?;
    let cold = repo.seam("predicate_boundary", 2)?;
    let warm = repo.seam("predicate_boundary", 2)?;
    assert_eq!(cold, warm, "a warm cache must return the same seam record");
    assert!(categories(&warm)?.iter().any(|c| c == BUDGET));
    let diff = repo.run("json")?;
    let kinds: Vec<&str> = diff
        .get("findings")
        .and_then(Value::as_array)
        .ok_or_else(|| "premise: diff output has findings".to_string())?
        .iter()
        .filter_map(|finding| finding.get("static_limit_kind").and_then(Value::as_str))
        .collect();
    assert!(kinds.contains(&BUDGET), "{kinds:?}");
    Ok(())
}
