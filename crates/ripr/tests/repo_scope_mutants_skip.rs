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
/// Same byte length as [`SKIP`], so swapping one for the other moves no seam
/// offset and every seam keeps its id across the two snapshots.
const NO_ATTR: &str = "// no attribute.";

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
        let root = self.root.to_string_lossy().into_owned();
        self.ripr(&["check", "--root", &root, "--format", format])
    }

    /// `ripr <args..>` with this fixture's cache directory.
    fn ripr(&self, args: &[&str]) -> Result<Run, String> {
        let output = Command::new(env!("CARGO_BIN_EXE_ripr"))
            .args(args)
            .env("RIPR_CACHE_DIR", self.base.join("cache"))
            .env_remove("RIPR_QUIET")
            .output()
            .map_err(|e| format!("run ripr {args:?}: {e}"))?;
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        if !output.status.success() {
            return Err(format!("ripr {args:?} failed: {}\n{stderr}", output.status));
        }
        let stdout = String::from_utf8(output.stdout).map_err(|e| e.to_string())?;
        Ok(Run { stdout, stderr })
    }

    /// Save a `repo-exposure-json` snapshot of the current source.
    fn snapshot(&self, name: &str) -> Result<PathBuf, String> {
        let path = self.base.join(name);
        fs::write(&path, self.text("repo-exposure-json")?)
            .map_err(|e| format!("write {name}: {e}"))?;
        Ok(path)
    }

    /// `ripr outcome --format json` over two snapshots.
    fn outcome(&self, before: &std::path::Path, after: &std::path::Path) -> Result<Value, String> {
        let before = before.to_string_lossy().into_owned();
        let after = after.to_string_lossy().into_owned();
        let run = self.ripr(&[
            "outcome", "--before", &before, "--after", &after, "--format", "json",
        ])?;
        serde_json::from_str(&run.stdout).map_err(|e| format!("outcome is not JSON: {e}"))
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
    let disclosure = &document["owner_opt_outs"];
    let ids: Vec<&str> = disclosure["seam_ids"]
        .as_array()
        .map(|ids| ids.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    let sorted_unique = ids.windows(2).all(|pair| pair[0] < pair[1]);
    if disclosure["withheld"] != withheld
        || disclosure["reason"] != REASON
        || ids.len() != withheld
        || !sorted_unique
        || disclosure.as_object().map(serde_json::Map::len) != Some(3)
    {
        return Err(format!(
            "{format}: owner_opt_outs should be {{withheld: {withheld}, reason: {REASON}, seam_ids: [{withheld} sorted ids]}}, got {disclosure}"
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

/// Not a proof that the decision is parser-backed: a lexically read file
/// yields no repo seams at all, so this would pass under either decision. It
/// pins only that such a file discloses nothing. The parser-backed decision
/// is proved by `analysis::owner_opt_out::tests`.
#[test]
fn lexically_read_file_yields_no_repo_seams_and_no_disclosure() -> Result<(), String> {
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

/// The ids `owner_opt_outs` names in a repo-exposure snapshot.
fn disclosed_ids(document: &Value) -> Result<BTreeSet<String>, String> {
    document["owner_opt_outs"]["seam_ids"]
        .as_array()
        .ok_or_else(|| format!("no owner_opt_outs.seam_ids: {document}"))?
        .iter()
        .map(|id| {
            id.as_str()
                .map(str::to_owned)
                .ok_or_else(|| format!("seam id is not a string: {id}"))
        })
        .collect()
}

fn seam_ids_owned_by(document: &Value, name: &str) -> Result<BTreeSet<String>, String> {
    Ok(seams(document)?
        .iter()
        .filter(|seam| owned_by(seam, name))
        .filter_map(|seam| seam["seam_id"].as_str().map(str::to_owned))
        .collect())
}

fn assert_outcome_excludes(
    outcome: &Value,
    withheld: &BTreeSet<String>,
    label: &str,
) -> Result<(), String> {
    for bucket in ["removed", "new"] {
        let count = as_u64(&outcome["summary"][bucket], bucket)?;
        if count != 0 {
            return Err(format!(
                "{label}: {count} {bucket} seam(s), want 0: {outcome}"
            ));
        }
    }
    let disclosed: BTreeSet<String> = outcome["owner_opt_outs"]["seam_ids"]
        .as_array()
        .map(|ids| {
            ids.iter()
                .filter_map(|id| id.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default();
    if &disclosed != withheld
        || outcome["owner_opt_outs"]["reason"] != REASON
        || outcome["owner_opt_outs"]["withheld"] != withheld.len()
    {
        return Err(format!(
            "{label}: outcome must disclose the excluded ids {withheld:?}: {}",
            outcome["owner_opt_outs"]
        ));
    }
    Ok(())
}

/// #14139 review F1: the attribute added between two `ripr check` snapshots
/// with no test change must not read the opted-out owner's seams as removed
/// (resolved). The ids stay equal across the swap, so without the exclusion
/// every fn_level seam would land in `removed`.
#[test]
fn outcome_does_not_resolve_seams_withheld_between_two_check_snapshots() -> Result<(), String> {
    let repo = Repo::new(lib_source(NO_ATTR).as_bytes())?;
    let before = repo.snapshot("before.json")?;
    let before_json: Value =
        serde_json::from_str(&fs::read_to_string(&before).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    let fn_level_ids = seam_ids_owned_by(&before_json, "fn_level")?;
    repo.write_lib(lib_source(SKIP).as_bytes())?;
    let after = repo.snapshot("after.json")?;
    let after_json: Value =
        serde_json::from_str(&fs::read_to_string(&after).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    if fn_level_ids.is_empty() || disclosed_ids(&after_json)? != fn_level_ids {
        return Err(format!(
            "fixture: the after snapshot must withhold exactly fn_level's ids {fn_level_ids:?}: {}",
            after_json["owner_opt_outs"]
        ));
    }
    let outcome = repo.outcome(&before, &after)?;
    assert_outcome_excludes(&outcome, &fn_level_ids, "check -> check")?;
    let control = seam_ids_owned_by(&before_json, "control")?.len() as u64;
    if as_u64(&outcome["summary"]["unchanged"], "unchanged")? != control {
        return Err(format!("control seams must read unchanged: {outcome}"));
    }
    // The reverse pair (attribute removed) is excluded the same way.
    let reversed = repo.outcome(&after, &before)?;
    assert_outcome_excludes(&reversed, &fn_level_ids, "check -> check reversed")
}

/// #14139 review F1: a `ripr pilot` snapshot keeps every seam; compared with a
/// `ripr check` snapshot of the same source, the seams only check withheld
/// must not read as removed.
#[test]
fn outcome_does_not_resolve_seams_a_pilot_snapshot_kept() -> Result<(), String> {
    let repo = Repo::new(lib_source(SKIP).as_bytes())?;
    let root = repo.root.to_string_lossy().into_owned();
    let out = repo.base.join("pilot");
    let out_arg = out.to_string_lossy().into_owned();
    repo.ripr(&["pilot", "--root", &root, "--out", &out_arg])?;
    let pilot = out.join("repo-exposure.json");
    let pilot_json: Value =
        serde_json::from_str(&fs::read_to_string(&pilot).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    let fn_level_ids = seam_ids_owned_by(&pilot_json, "fn_level")?;
    if fn_level_ids.is_empty() || pilot_json.get("owner_opt_outs").is_some() {
        return Err(format!(
            "fixture: the pilot snapshot must keep fn_level's seams undisclosed: {pilot_json}"
        ));
    }
    let check = repo.snapshot("check.json")?;
    let outcome = repo.outcome(&pilot, &check)?;
    assert_outcome_excludes(&outcome, &fn_level_ids, "pilot -> check")
}

/// Positive controls: the exclusion is targeted. A real test fix on the plain
/// `control` owner still moves its seams, and deleting control's predicate
/// still reads as removed, while fn_level stays withheld on both sides.
#[test]
fn outcome_still_reports_real_movement_on_the_plain_owner() -> Result<(), String> {
    let repo = Repo::new(lib_source(SKIP).as_bytes())?;
    let before = repo.snapshot("before.json")?;
    fs::write(
        repo.root.join("tests/control_boundary.rs"),
        "#[test]\nfn control_boundary() {\n    assert_eq!(fx::control(3), false);\n    assert_eq!(fx::control(4), true);\n}\n",
    )
    .map_err(|e| e.to_string())?;
    let fixed = repo.snapshot("fixed.json")?;
    let outcome = repo.outcome(&before, &fixed)?;
    let moved = as_u64(&outcome["summary"]["moved"], "moved")?;
    let improved = outcome["moved"]
        .as_array()
        .map(|moved| {
            moved
                .iter()
                .filter(|movement| {
                    movement["direction"] == "improved"
                        && movement["file"]
                            .as_str()
                            .is_some_and(|file| file.ends_with("src/lib.rs"))
                })
                .count()
        })
        .unwrap_or(0);
    if moved == 0 || improved == 0 {
        return Err(format!(
            "a real test fix on control must read as improved: {outcome}"
        ));
    }
    let before_json: Value =
        serde_json::from_str(&fs::read_to_string(&before).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    let withheld = disclosed_ids(&before_json)?;
    assert_outcome_excludes(&outcome, &withheld, "test fix")?;

    // Removing control's predicate (same file length is not needed: the
    // control seams are the last ones) still reads as removed.
    let source = lib_source(SKIP).replace(
        "pub fn control(level: u32) -> bool {\n    level > 3\n}\n",
        "pub fn control(_level: u32) -> bool {\n    true\n}\n",
    );
    repo.write_lib(source.as_bytes())?;
    let rewritten = repo.snapshot("rewritten.json")?;
    let outcome = repo.outcome(&before, &rewritten)?;
    if as_u64(&outcome["summary"]["removed"], "removed")? == 0 {
        return Err(format!(
            "control's deleted seams must still read as removed: {outcome}"
        ));
    }
    Ok(())
}
