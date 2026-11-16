//! #14139: the repo-scope formats (`ripr check --format repo-*`) withhold the
//! seams of an owner function that carries `#[mutants::skip]` from their
//! counts and disclose the withheld number with the `owner_mutants_skip`
//! reason, consistent with the diff-scope disclosure (#13819).
//!
//! Every test drives the real `ripr` binary on a one-package crate whose
//! `lib.rs` holds two identical owners, `fn_level` and `control`; only the
//! attribute line above `fn_level` varies.

use serde_json::Value;
use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_ID: AtomicU64 = AtomicU64::new(0);

const REASON: &str = "owner_mutants_skip";
const CACHE_MISS_NOTICE: &str = "no usable cache entry for this workspace state";

/// The issue's repro crate. `attr` is always exactly one line.
fn lib_source(attr: &str) -> String {
    format!(
        "{attr}
pub fn fn_level(level: u32) -> bool {{
    level > 3
}}

pub fn control(level: u32) -> bool {{
    level > 3
}}
"
    )
}

const SKIP: &str = "#[mutants::skip]";
const NO_ATTR: &str = "// no attribute";

struct Repo {
    base: PathBuf,
    root: PathBuf,
}

struct Run {
    stdout: String,
    stderr: String,
}

impl Repo {
    fn new(source: &[u8]) -> Result<Self, String> {
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        let base = std::env::temp_dir().join(format!(
            "ripr-repo-mutants-skip-{}-{id}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&base);
        let root = base.join("fx");
        fs::create_dir_all(root.join("src")).map_err(|e| e.to_string())?;
        fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"fx\"\nversion = \"0.0.0\"\nedition = \"2021\"\n\n[workspace]\n",
        )
        .map_err(|e| e.to_string())?;
        // A test far from the boundary: both owners are reached but weakly
        // gripped, so each counts as an actionable gap on the badge.
        fs::create_dir_all(root.join("tests")).map_err(|e| e.to_string())?;
        fs::write(
            root.join("tests/levels.rs"),
            "#[test]\nfn far_from_boundary() {\n    assert!(fx::fn_level(10));\n    assert!(fx::control(10));\n}\n",
        )
        .map_err(|e| e.to_string())?;
        let repo = Self { base, root };
        repo.write_lib(source)?;
        Ok(repo)
    }

    fn write_lib(&self, source: &[u8]) -> Result<(), String> {
        fs::write(self.root.join("src/lib.rs"), source).map_err(|e| format!("write lib.rs: {e}"))
    }

    fn run(&self, format: &str) -> Result<Run, String> {
        let output = Command::new(env!("CARGO_BIN_EXE_ripr"))
            .args(["check", "--root"])
            .arg(&self.root)
            .args(["--format", format])
            .env("RIPR_CACHE_DIR", self.base.join("cache"))
            .env_remove("RIPR_QUIET")
            .output()
            .map_err(|e| format!("run ripr check --format {format}: {e}"))?;
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        if !output.status.success() {
            return Err(format!(
                "ripr check --format {format} failed: {}\n{stderr}",
                output.status
            ));
        }
        let stdout = String::from_utf8(output.stdout).map_err(|e| e.to_string())?;
        Ok(Run { stdout, stderr })
    }

    fn text(&self, format: &str) -> Result<String, String> {
        Ok(self.run(format)?.stdout)
    }

    fn json(&self, format: &str) -> Result<Value, String> {
        serde_json::from_str(&self.text(format)?)
            .map_err(|e| format!("{format} output is not JSON: {e}"))
    }
}

impl Drop for Repo {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.base);
    }
}

fn seams(document: &Value) -> Result<&Vec<Value>, String> {
    document["seams"]
        .as_array()
        .ok_or_else(|| format!("document has no seams array: {document}"))
}

/// Whether a seam's `owner` (`src/lib.rs::name`) names the function `name`.
fn owned_by(seam: &Value, name: &str) -> bool {
    seam["owner"]
        .as_str()
        .is_some_and(|owner| owner.rsplit("::").next() == Some(name))
}

fn count_owned(document: &Value, name: &str) -> Result<usize, String> {
    Ok(seams(document)?
        .iter()
        .filter(|seam| owned_by(seam, name))
        .count())
}

/// Offset-independent identity of `name`'s seams: the attribute line shifts
/// byte offsets (and so seam ids), never kind, expression or class.
fn owner_seam_keys(document: &Value, name: &str) -> Result<BTreeSet<String>, String> {
    Ok(seams(document)?
        .iter()
        .filter(|seam| owned_by(seam, name))
        .map(|seam| {
            format!(
                "{}|{}|{}",
                seam["kind"], seam["expression"], seam["grip_class"]
            )
        })
        .collect())
}

fn as_u64(value: &Value, what: &str) -> Result<u64, String> {
    value
        .as_u64()
        .ok_or_else(|| format!("{what} is not a count: {value}"))
}

fn assert_disclosed(document: &Value, withheld: usize, format: &str) -> Result<(), String> {
    let expected = serde_json::json!({"withheld": withheld, "reason": REASON});
    if document["owner_opt_outs"] != expected {
        return Err(format!(
            "{format}: owner_opt_outs should be {expected}, got {}",
            document["owner_opt_outs"]
        ));
    }
    Ok(())
}

fn assert_not_disclosed(text: &str, format: &str) -> Result<(), String> {
    for token in [REASON, "owner_opt_outs", "Withheld by owner opt-out"] {
        if text.contains(token) {
            return Err(format!("{format}: unexpected `{token}` in:\n{text}"));
        }
    }
    Ok(())
}

/// The baseline run: the same crate without the attribute.
fn baseline() -> Result<(Repo, Value, Value), String> {
    let repo = Repo::new(lib_source(NO_ATTR).as_bytes())?;
    let exposure = repo.json("repo-exposure-json")?;
    let summary = repo.json("repo-exposure-summary-json")?;
    Ok((repo, exposure, summary))
}

#[test]
fn repo_exposure_json_and_summary_withhold_mutants_skip_owner_seams() -> Result<(), String> {
    let (base_repo, base_exposure, base_summary) = baseline()?;
    // Fixture construction: both owners produce the same, non-empty seams.
    let skipped_seams = count_owned(&base_exposure, "fn_level")?;
    if skipped_seams == 0
        || owner_seam_keys(&base_exposure, "fn_level")?
            != owner_seam_keys(&base_exposure, "control")?
    {
        return Err(format!(
            "fixture must give fn_level and control identical non-empty seams: {base_exposure}"
        ));
    }
    assert_not_disclosed(&base_repo.text("repo-exposure-json")?, "baseline exposure")?;
    assert_not_disclosed(
        &base_repo.text("repo-exposure-summary-json")?,
        "baseline summary",
    )?;
    let base_gaps = as_u64(
        &base_summary["metrics"]["canonical_gap_records"],
        "baseline canonical_gap_records",
    )?;
    if base_gaps < 2 || base_gaps % 2 != 0 {
        return Err(format!(
            "fixture must count the same gaps for each owner, got {base_gaps}: {base_summary}"
        ));
    }

    let repo = Repo::new(lib_source(SKIP).as_bytes())?;
    let exposure = repo.json("repo-exposure-json")?;
    if count_owned(&exposure, "fn_level")? != 0 {
        return Err(format!("fn_level seams must be withheld: {exposure}"));
    }
    if owner_seam_keys(&exposure, "control")? != owner_seam_keys(&base_exposure, "control")? {
        return Err(format!(
            "control seams must be unchanged:\nwith skip: {exposure}\nbaseline: {base_exposure}"
        ));
    }
    assert_disclosed(&exposure, skipped_seams, "repo-exposure-json")?;
    let total = as_u64(&exposure["metrics"]["seams_total"], "seams_total")?;
    let base_total = as_u64(&base_exposure["metrics"]["seams_total"], "seams_total")?;
    if total + skipped_seams as u64 != base_total || total as usize != seams(&exposure)?.len() {
        return Err(format!(
            "metrics.seams_total must drop by the withheld count: {total} + {skipped_seams} != {base_total}"
        ));
    }

    let summary = repo.json("repo-exposure-summary-json")?;
    assert_disclosed(&summary, skipped_seams, "repo-exposure-summary-json")?;
    let raw = as_u64(&summary["metrics"]["raw_seams"], "raw_seams")?;
    let gaps = as_u64(
        &summary["metrics"]["canonical_gap_records"],
        "canonical_gap_records",
    )?;
    let base_unsuppressed = as_u64(
        &base_summary["metrics"]["unsuppressed_exposure_gaps"],
        "baseline unsuppressed_exposure_gaps",
    )?;
    let unsuppressed = as_u64(
        &summary["metrics"]["unsuppressed_exposure_gaps"],
        "unsuppressed_exposure_gaps",
    )?;
    if unsuppressed > base_unsuppressed {
        return Err(format!(
            "withholding must never raise unsuppressed_exposure_gaps: {unsuppressed} > {base_unsuppressed}"
        ));
    }
    if raw != total || gaps != base_gaps / 2 {
        return Err(format!(
            "summary must count only control: raw {raw} (want {total}), gaps {gaps} (want {})",
            base_gaps / 2
        ));
    }
    Ok(())
}

#[test]
fn repo_scope_formats_agree_on_the_withheld_count() -> Result<(), String> {
    let (base_repo, base_exposure, base_summary) = baseline()?;
    let withheld = count_owned(&base_exposure, "fn_level")?;
    let repo = Repo::new(lib_source(SKIP).as_bytes())?;
    let summary = repo.json("repo-exposure-summary-json")?;
    assert_disclosed(&summary, withheld, "repo-exposure-summary-json")?;

    let seams_json = repo.json("repo-seams-json")?;
    assert_disclosed(&seams_json, withheld, "repo-seams-json")?;
    let base_seams_json = base_repo.json("repo-seams-json")?;
    if count_owned(&seams_json, "fn_level")? != 0
        || count_owned(&base_seams_json, "fn_level")? != withheld
        || seams(&seams_json)?.len() + withheld != seams(&base_seams_json)?.len()
    {
        return Err(format!(
            "repo-seams-json must withhold exactly fn_level's {withheld} seams: {seams_json}"
        ));
    }

    let line = format!("Withheld by owner opt-out ({REASON}): {withheld} seam");
    for format in ["repo-seams-md", "repo-exposure-md"] {
        let md = repo.text(format)?;
        if !md.contains(&line) || md.contains("`src/lib.rs::fn_level`") {
            return Err(format!(
                "{format} must disclose `{line}` and drop fn_level:\n{md}"
            ));
        }
        assert_not_disclosed(&base_repo.text(format)?, format)?;
    }
    let seams_md = repo.text("repo-seams-md")?;
    let total_line = format!("Total seams: {}\n", seams(&seams_json)?.len());
    if !seams_md.contains(&total_line) {
        return Err(format!(
            "repo-seams-md must total {total_line:?}:\n{seams_md}"
        ));
    }

    // The badges analyze the same withheld inventory: their seam count is
    // the summary's `raw_seams`, and fn_level's gap leaves the headline.
    let raw = as_u64(&summary["metrics"]["raw_seams"], "raw_seams")?;
    let base_raw = as_u64(&base_summary["metrics"]["raw_seams"], "baseline raw_seams")?;
    let badge = repo.json("repo-badge-json")?;
    let base_badge = base_repo.json("repo-badge-json")?;
    let analyzed = as_u64(&badge["counts"]["analyzed_seams"], "badge analyzed_seams")?;
    let base_analyzed = as_u64(
        &base_badge["counts"]["analyzed_seams"],
        "baseline badge analyzed_seams",
    )?;
    if analyzed != raw || base_analyzed != base_raw || analyzed + withheld as u64 != base_analyzed {
        return Err(format!(
            "repo-badge-json must analyze the withheld inventory: {analyzed} vs raw {raw}, baseline {base_analyzed} vs {base_raw}, withheld {withheld}"
        ));
    }
    let gaps = as_u64(
        &badge["counts"]["unsuppressed_exposure_gaps"],
        "badge unsuppressed_exposure_gaps",
    )?;
    let base_gaps = as_u64(
        &base_badge["counts"]["unsuppressed_exposure_gaps"],
        "baseline badge unsuppressed_exposure_gaps",
    )?;
    if base_gaps < 2 || base_gaps % 2 != 0 || gaps != base_gaps / 2 {
        return Err(format!(
            "repo-badge-json must count only control's gaps: {gaps} of baseline {base_gaps}"
        ));
    }
    if badge.get("owner_opt_outs").is_some() {
        return Err(format!("badges carry no withheld member: {badge}"));
    }
    let shields = repo.json("repo-badge-shields")?;
    let base_shields = base_repo.json("repo-badge-shields")?;
    for (document, want, label) in [
        (&shields, gaps, "repo-badge-shields"),
        (&base_shields, base_gaps, "baseline repo-badge-shields"),
    ] {
        let expected = format!("{want} actionable");
        if document["message"] != expected.as_str() {
            return Err(format!(
                "{label}: message should be {expected:?}: {document}"
            ));
        }
    }
    Ok(())
}

#[test]
fn doc_string_naming_mutants_skip_does_not_withhold() -> Result<(), String> {
    let (_base_repo, base_exposure, _) = baseline()?;
    let repo = Repo::new(lib_source("#[doc = \"mutants::skip\"]").as_bytes())?;
    let text = repo.text("repo-exposure-json")?;
    assert_not_disclosed(&text, "doc-attribute repo-exposure-json")?;
    let exposure: Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    if owner_seam_keys(&exposure, "fn_level")? != owner_seam_keys(&base_exposure, "fn_level")?
        || count_owned(&exposure, "fn_level")? == 0
    {
        return Err(format!(
            "a doc string must not withhold fn_level: {exposure}"
        ));
    }
    assert_not_disclosed(
        &repo.text("repo-seams-json")?,
        "doc-attribute repo-seams-json",
    )
}

#[test]
fn lexically_read_file_contributes_no_seam_to_withhold() -> Result<(), String> {
    // A byte that is not UTF-8 keeps the file on the lexical fallback, which
    // carries no parser-backed attributes and no repo seams: nothing is
    // withheld or disclosed (the owner decision for a lexical owner is
    // pinned by the `owner_opt_out` unit tests).
    let mut source = b"// \xFF not utf-8\n".to_vec();
    source.extend_from_slice(lib_source(SKIP).as_bytes());
    let repo = Repo::new(&source)?;
    for format in ["repo-seams-json", "repo-exposure-json"] {
        assert_not_disclosed(&repo.text(format)?, format)?;
    }
    Ok(())
}

#[test]
fn cached_repo_results_follow_the_attribute_being_added_and_removed() -> Result<(), String> {
    let repo = Repo::new(lib_source(NO_ATTR).as_bytes())?;
    let format = "repo-exposure-json";
    let cold = repo.run(format)?;
    let warm = repo.run(format)?;
    // The cache must be live for this test to discriminate anything.
    if !cold.stderr.contains(CACHE_MISS_NOTICE) || warm.stderr.contains(CACHE_MISS_NOTICE) {
        return Err(format!(
            "expected a cold miss then a warm hit:\ncold: {}\nwarm: {}",
            cold.stderr, warm.stderr
        ));
    }
    assert_not_disclosed(&warm.stdout, "warm without attribute")?;
    let withheld = count_owned(
        &serde_json::from_str(&warm.stdout).map_err(|e| e.to_string())?,
        "fn_level",
    )?;

    repo.write_lib(lib_source(SKIP).as_bytes())?;
    let added = repo.run(format)?;
    if !added.stderr.contains(CACHE_MISS_NOTICE) {
        return Err(format!(
            "adding the attribute must miss the cache: {}",
            added.stderr
        ));
    }
    let added_json: Value = serde_json::from_str(&added.stdout).map_err(|e| e.to_string())?;
    assert_disclosed(&added_json, withheld, "after adding the attribute")?;

    repo.write_lib(lib_source(NO_ATTR).as_bytes())?;
    let removed = repo.run(format)?;
    assert_not_disclosed(&removed.stdout, "after removing the attribute")?;
    // The restored bytes match the first entry's key, so it replays; the
    // inventory (not the run-stamped `artifact` envelope) is the earlier one.
    let removed_json: Value = serde_json::from_str(&removed.stdout).map_err(|e| e.to_string())?;
    let warm_json: Value = serde_json::from_str(&warm.stdout).map_err(|e| e.to_string())?;
    if removed.stderr.contains(CACHE_MISS_NOTICE)
        || removed_json["seams"] != warm_json["seams"]
        || removed_json["metrics"] != warm_json["metrics"]
    {
        return Err(format!(
            "removing the attribute must replay the earlier inventory: {}",
            removed.stderr
        ));
    }

    // Re-adding replays the entry stored with the attribute: the opt-out
    // decision must survive the cache round trip.
    repo.write_lib(lib_source(SKIP).as_bytes())?;
    let replayed = repo.run(format)?;
    if replayed.stderr.contains(CACHE_MISS_NOTICE) {
        return Err(format!(
            "re-adding the attribute should hit the stored entry: {}",
            replayed.stderr
        ));
    }
    let replayed_json: Value = serde_json::from_str(&replayed.stdout).map_err(|e| e.to_string())?;
    assert_disclosed(&replayed_json, withheld, "cached replay with the attribute")?;
    if count_owned(&replayed_json, "fn_level")? != 0 {
        return Err(format!(
            "cached replay must still withhold fn_level: {replayed_json}"
        ));
    }
    Ok(())
}
