//! #14898: a `#[mutants::skip]` added to an owner after a baseline was
//! recorded must not read as resolved baseline debt.
//!
//! Every test drives the real `ripr` binary through the whole chain:
//! `check --format repo-exposure-json` -> `reports gap-ledger --repo-exposure`
//! -> `gate evaluate --gap-ledger` -> `baseline create` / `baseline diff` /
//! `baseline update --remove-resolved`. The crate holds two identical owners;
//! `control` sits above `fn_level`, so inserting the attribute line above
//! `fn_level` moves only `fn_level`'s seam offsets (and ids).

use serde_json::Value;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_ID: AtomicU64 = AtomicU64::new(0);

const SKIP: &str = "#[mutants::skip]\n";

/// `lib.rs` with `attr` (empty or one whole line) above `fn_level`.
fn lib_source(attr: &str) -> String {
    format!(
        "pub fn control(level: u32) -> bool {{
    level > 3
}}

{attr}pub fn fn_level(level: u32) -> bool {{
    level > 3
}}
"
    )
}

/// Far from both boundaries: each owner is reached but weakly gripped.
const WEAK_TESTS: &str =
    "#[test]\nfn far_from_boundary() {\n    assert!(fx::fn_level(10));\n    assert!(fx::control(10));\n}\n";

/// `WEAK_TESTS` plus a boundary pin on `control` only: a real fix of
/// `control`'s gap.
const CONTROL_PINNED_TESTS: &str = "#[test]\nfn far_from_boundary() {\n    assert!(fx::fn_level(10));\n    assert!(fx::control(10));\n}\n\n#[test]\nfn control_boundary() {\n    assert!(!fx::control(3));\n    assert!(fx::control(4));\n}\n";

struct Repo {
    base: PathBuf,
    root: PathBuf,
}

impl Repo {
    fn new(lib: &str, tests: &str) -> Result<Self, String> {
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        let base = std::env::temp_dir().join(format!(
            "ripr-baseline-owner-opt-out-{}-{id}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&base);
        let root = base.join("fx");
        fs::create_dir_all(root.join("src")).map_err(|e| e.to_string())?;
        fs::create_dir_all(root.join("tests")).map_err(|e| e.to_string())?;
        fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"fx\"\nversion = \"0.0.0\"\nedition = \"2021\"\n\n[workspace]\n",
        )
        .map_err(|e| e.to_string())?;
        let repo = Self { base, root };
        repo.write(lib, tests)?;
        Ok(repo)
    }

    fn write(&self, lib: &str, tests: &str) -> Result<(), String> {
        fs::write(self.root.join("src/lib.rs"), lib).map_err(|e| format!("write lib.rs: {e}"))?;
        fs::write(self.root.join("tests/levels.rs"), tests)
            .map_err(|e| format!("write tests: {e}"))
    }

    fn ripr(&self, args: &[&str]) -> Result<String, String> {
        let output = Command::new(env!("CARGO_BIN_EXE_ripr"))
            .args(args)
            .current_dir(&self.root)
            .env("RIPR_CACHE_DIR", self.base.join("cache"))
            .output()
            .map_err(|e| format!("run ripr {args:?}: {e}"))?;
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        if !output.status.success() {
            return Err(format!("ripr {args:?} failed: {}\n{stderr}", output.status));
        }
        String::from_utf8(output.stdout).map_err(|e| e.to_string())
    }

    fn path(&self, name: &str) -> String {
        self.base.join(name).to_string_lossy().into_owned()
    }

    /// Snapshot, ledger and gate decision of the current source, named
    /// `<label>-*.json`; returns the gate decision's path.
    fn gate(&self, label: &str) -> Result<String, String> {
        let snapshot = self.path(&format!("{label}-repo-exposure.json"));
        let ledger = self.path(&format!("{label}-ledger.json"));
        let gate = self.path(&format!("{label}-gate.json"));
        let exposure = self.ripr(&["check", "--root", ".", "--format", "repo-exposure-json"])?;
        fs::write(&snapshot, exposure).map_err(|e| format!("write snapshot: {e}"))?;
        self.ripr(&[
            "reports",
            "gap-ledger",
            "--repo-exposure",
            &snapshot,
            "--root",
            ".",
            "--out",
            &ledger,
            "--out-md",
            &self.path(&format!("{label}-ledger.md")),
        ])?;
        self.ripr(&[
            "gate",
            "evaluate",
            "--root",
            ".",
            "--gap-ledger",
            &ledger,
            "--mode",
            "visible-only",
            "--out",
            &gate,
            "--out-md",
            &self.path(&format!("{label}-gate.md")),
        ])?;
        Ok(gate)
    }

    fn baseline_create(&self, gate: &str) -> Result<String, String> {
        let baseline = self.path("baseline.json");
        self.ripr(&["baseline", "create", "--from", gate, "--out", &baseline])?;
        Ok(baseline)
    }

    fn delta(&self, baseline: &str, gate: &str, label: &str) -> Result<(Value, String), String> {
        let json = self.path(&format!("{label}-delta.json"));
        let md = self.path(&format!("{label}-delta.md"));
        self.ripr(&[
            "baseline",
            "diff",
            "--baseline",
            baseline,
            "--current",
            gate,
            "--out",
            &json,
            "--out-md",
            &md,
        ])?;
        Ok((read_json(&json)?, read_text(&md)?))
    }
}

impl Drop for Repo {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.base);
    }
}

fn read_text(path: &str) -> Result<String, String> {
    fs::read_to_string(path).map_err(|e| format!("read {path}: {e}"))
}

fn read_json(path: &str) -> Result<Value, String> {
    serde_json::from_str(&read_text(path)?).map_err(|e| format!("{path} is not JSON: {e}"))
}

/// The baseline entries whose `changed_owner` is `owner`.
fn entries_of<'a>(baseline: &'a Value, owner: &str) -> Vec<&'a Value> {
    baseline["entries"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|entry| entry["changed_owner"] == owner)
        .collect()
}

/// The delta items whose identity seam id is one of `seam_ids`.
fn items_for<'a>(delta: &'a Value, seam_ids: &[&str]) -> Vec<&'a Value> {
    delta["items"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|item| {
            item["identity"]["seam_id"]
                .as_str()
                .is_some_and(|id| seam_ids.contains(&id))
        })
        .collect()
}

fn seam_ids<'a>(entries: &[&'a Value]) -> Vec<&'a str> {
    entries
        .iter()
        .filter_map(|entry| entry["identity"]["seam_id"].as_str())
        .collect()
}

/// A baseline of both owners' weak gaps, then the attribute added above
/// `fn_level` (a real line insertion) with the tests `after_tests`.
fn skipped_after_baseline(
    after_tests: &str,
) -> Result<(Repo, Value, Value, String, Vec<String>, Vec<String>), String> {
    let repo = Repo::new(&lib_source(""), WEAK_TESTS)?;
    let before_gate = repo.gate("before")?;
    let baseline_path = repo.baseline_create(&before_gate)?;
    let baseline = read_json(&baseline_path)?;
    let skipped: Vec<String> = seam_ids(&entries_of(&baseline, "src/lib.rs::fn_level"))
        .into_iter()
        .map(String::from)
        .collect();
    let control: Vec<String> = seam_ids(&entries_of(&baseline, "src/lib.rs::control"))
        .into_iter()
        .map(String::from)
        .collect();
    // Fixture construction: both owners have baseline debt that records its
    // changed owner, and the gate saw the whole workspace (so absence could
    // read as resolved at all).
    if skipped.is_empty() || control.is_empty() {
        return Err(format!(
            "fixture: want baseline entries for both owners, got {baseline}"
        ));
    }
    repo.write(&lib_source(SKIP), after_tests)?;
    let after_gate = repo.gate("after")?;
    let gate = read_json(&after_gate)?;
    if gate["analysis_scope"]["scope"] != "rust_workspace_complete" {
        return Err(format!(
            "fixture: want a rust_workspace_complete gate scope, got {}",
            gate["analysis_scope"]
        ));
    }
    let disclosure = &gate["analysis_scope"]["owner_opt_outs"];
    if disclosure["owners"] != serde_json::json!(["src/lib.rs::fn_level"]) {
        return Err(format!(
            "fixture: the gate must carry the ledger's owner_opt_outs, got {disclosure}"
        ));
    }
    // The inserted line moved the skipped owner's seam ids: none of the
    // baseline ids is the one the current side withholds.
    let withheld: Vec<&str> = disclosure["seam_ids"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect();
    if skipped.iter().any(|id| withheld.contains(&id.as_str())) {
        return Err(format!(
            "fixture: the attribute insertion must move fn_level's seam ids ({skipped:?} vs {withheld:?})"
        ));
    }
    Ok((repo, baseline, gate, baseline_path, skipped, control))
}

#[test]
fn a_skip_added_after_the_baseline_is_withheld_not_resolved() -> Result<(), String> {
    let (repo, _, _, baseline_path, skipped, control) = skipped_after_baseline(WEAK_TESTS)?;
    let after_gate = repo.path("after-gate.json");
    let (delta, markdown) = repo.delta(&baseline_path, &after_gate, "skip")?;
    let skipped_ids: Vec<&str> = skipped.iter().map(String::as_str).collect();
    let items = items_for(&delta, &skipped_ids);
    assert_eq!(items.len(), skipped.len(), "{delta}");
    for item in &items {
        assert_eq!(item["bucket"], "suppressed", "{item}");
        assert_eq!(item["withheld_by"], "owner_mutants_skip", "{item}");
    }
    assert_eq!(delta["delta"]["resolved"], 0, "{delta}");
    assert_eq!(
        delta["analysis_scope"]["owner_opt_outs"]["reason"], "owner_mutants_skip",
        "{delta}"
    );
    assert!(markdown.contains("Withheld by owner opt-out"), "{markdown}");
    // The unskipped owner is unchanged and still present.
    let control_ids: Vec<&str> = control.iter().map(String::as_str).collect();
    for item in items_for(&delta, &control_ids) {
        assert_eq!(item["bucket"], "still_present", "{item}");
    }

    // `baseline update --remove-resolved` keeps every withheld entry.
    let updated = repo.path("updated-baseline.json");
    repo.ripr(&[
        "baseline",
        "update",
        "--baseline",
        &baseline_path,
        "--from",
        &after_gate,
        "--remove-resolved",
        "--out",
        &updated,
    ])?;
    let updated = read_json(&updated)?;
    assert_eq!(
        entries_of(&updated, "src/lib.rs::fn_level").len(),
        skipped.len(),
        "{updated}"
    );
    Ok(())
}

/// Positive control: a real fix of the unskipped owner in the same run still
/// reads as resolved.
#[test]
fn a_real_fix_beside_the_skip_still_reads_resolved() -> Result<(), String> {
    let (repo, _, _, baseline_path, skipped, control) =
        skipped_after_baseline(CONTROL_PINNED_TESTS)?;
    let (delta, _) = repo.delta(&baseline_path, &repo.path("after-gate.json"), "fix")?;
    let control_ids: Vec<&str> = control.iter().map(String::as_str).collect();
    let control_items = items_for(&delta, &control_ids);
    assert!(!control_items.is_empty(), "{delta}");
    for item in &control_items {
        assert_eq!(item["bucket"], "resolved", "{item}");
    }
    let skipped_ids: Vec<&str> = skipped.iter().map(String::as_str).collect();
    for item in items_for(&delta, &skipped_ids) {
        assert_eq!(item["bucket"], "suppressed", "{item}");
    }
    Ok(())
}

/// A baseline written before `changed_owner` was recorded keeps working: the
/// entry in the withheld owner's file reads `not_evaluated`, never resolved,
/// and the unaffected entries still match.
#[test]
fn a_legacy_baseline_without_changed_owner_is_not_read_as_resolved() -> Result<(), String> {
    let (repo, mut baseline, _, _, skipped, control) = skipped_after_baseline(WEAK_TESTS)?;
    for entry in baseline["entries"].as_array_mut().into_iter().flatten() {
        if let Some(object) = entry.as_object_mut() {
            object.remove("changed_owner");
        }
    }
    let legacy = repo.path("legacy-baseline.json");
    fs::write(&legacy, baseline.to_string()).map_err(|e| e.to_string())?;
    let (delta, _) = repo.delta(&legacy, &repo.path("after-gate.json"), "legacy")?;
    let skipped_ids: Vec<&str> = skipped.iter().map(String::as_str).collect();
    let items = items_for(&delta, &skipped_ids);
    assert_eq!(items.len(), skipped.len(), "{delta}");
    for item in &items {
        assert_eq!(item["bucket"], "stale_baseline_entry", "{item}");
        assert_eq!(item["evaluation"], "not_evaluated", "{item}");
    }
    assert_eq!(delta["delta"]["resolved"], 0, "{delta}");
    let control_ids: Vec<&str> = control.iter().map(String::as_str).collect();
    for item in items_for(&delta, &control_ids) {
        assert_eq!(item["bucket"], "still_present", "{item}");
    }
    Ok(())
}

/// Reverse direction: a baseline recorded while the owner was skipped, then
/// the attribute removed. The owner's seams are current debt again, so they
/// read as new; nothing reads as resolved.
#[test]
fn removing_the_skip_reads_the_owner_as_new_debt() -> Result<(), String> {
    let repo = Repo::new(&lib_source(SKIP), WEAK_TESTS)?;
    let before_gate = repo.gate("before")?;
    let baseline_path = repo.baseline_create(&before_gate)?;
    let baseline = read_json(&baseline_path)?;
    if !entries_of(&baseline, "src/lib.rs::fn_level").is_empty()
        || entries_of(&baseline, "src/lib.rs::control").is_empty()
    {
        return Err(format!(
            "fixture: a skipped owner records no baseline debt, got {baseline}"
        ));
    }
    repo.write(&lib_source(""), WEAK_TESTS)?;
    let after_gate = repo.gate("after")?;
    let (delta, _) = repo.delta(&baseline_path, &after_gate, "unskip")?;
    assert_eq!(delta["delta"]["resolved"], 0, "{delta}");
    assert!(
        delta["delta"]["new_policy_eligible"].as_u64().unwrap_or(0) > 0,
        "{delta}"
    );
    assert!(read_json(&after_gate)?["analysis_scope"]
        .get("owner_opt_outs")
        .is_none());
    Ok(())
}
