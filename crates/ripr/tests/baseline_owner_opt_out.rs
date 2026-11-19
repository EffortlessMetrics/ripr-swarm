//! #14898: a `#[mutants::skip]` added to an owner after a baseline was
//! recorded must not read as resolved baseline debt.
//!
//! Every test drives the real `ripr` binary along the routes a project uses.
//! The baseline comes from the PR route (`gate check --base --head` ->
//! `baseline create`), whose entries record their changed owner. The current
//! side is the repo route (`check --format repo-exposure-json` ->
//! `reports gap-ledger --repo-exposure` -> `gate evaluate --gap-ledger`),
//! whose complete workspace scope lets an absent identity read as resolved.
//! The crate holds two identical owners; `control` sits above `fn_level`, so
//! inserting the attribute line above `fn_level` moves only `fn_level`'s seam
//! offsets (and ids).

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

/// `lib.rs` with `control` deleted and `fn_level` skipped.
fn lib_without_control() -> String {
    format!("{SKIP}pub fn fn_level(level: u32) -> bool {{\n    level > 3\n}}\n")
}

/// Far from both boundaries: each owner is reached but weakly gripped.
const WEAK_TESTS: &str = "#[test]\nfn far_from_boundary() {\n    assert!(fx::fn_level(10));\n    assert!(fx::control(10));\n}\n";

/// `WEAK_TESTS` without `control`, for the source that deletes it.
const FN_LEVEL_TESTS: &str =
    "#[test]\nfn far_from_boundary() {\n    assert!(fx::fn_level(10));\n}\n";

struct Repo {
    base: PathBuf,
    root: PathBuf,
}

impl Repo {
    /// A git repository whose first commit holds an empty library and whose
    /// second commit adds `lib` and `tests`, so `HEAD~1..HEAD` is the PR that
    /// introduced both owners.
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
        fs::write(root.join("src/lib.rs"), "").map_err(|e| e.to_string())?;
        let repo = Self { base, root };
        repo.git(&["init", "-q"])?;
        repo.commit("empty")?;
        repo.write(lib, tests)?;
        repo.commit("owners")?;
        Ok(repo)
    }

    fn git(&self, args: &[&str]) -> Result<(), String> {
        let output = Command::new("git")
            .current_dir(&self.root)
            .args(args)
            .output()
            .map_err(|e| format!("run git {args:?}: {e}"))?;
        if output.status.success() {
            Ok(())
        } else {
            Err(format!(
                "git {args:?}: {}",
                String::from_utf8_lossy(&output.stderr)
            ))
        }
    }

    fn commit(&self, message: &str) -> Result<(), String> {
        self.git(&["add", "-A"])?;
        self.git(&[
            "-c",
            "user.name=ripr-test",
            "-c",
            "user.email=ripr-test@example.invalid",
            "-c",
            "commit.gpgsign=false",
            "commit",
            "-q",
            "-m",
            message,
        ])
    }

    fn write(&self, lib: &str, tests: &str) -> Result<(), String> {
        fs::write(self.root.join("src/lib.rs"), lib).map_err(|e| format!("write lib.rs: {e}"))?;
        fs::write(self.root.join("tests/levels.rs"), tests).map_err(|e| format!("write tests: {e}"))
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

    /// Whether this `ripr` was built from a dirty checkout (`--version`
    /// ends in `-dirty)`).
    fn dirty_build(&self) -> Result<bool, String> {
        Ok(self.ripr(&["--version"])?.trim_end().ends_with("-dirty)"))
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

    /// The PR-route baseline of the commit that introduced both owners.
    fn baseline_create(&self) -> Result<String, String> {
        let decision = self.path("pr-gate.json");
        self.ripr(&[
            "gate",
            "check",
            "--base",
            "HEAD~1",
            "--head",
            "HEAD",
            "--mode",
            "visible-only",
            "--out",
            &decision,
            "--out-md",
            &self.path("pr-gate.md"),
            "--guidance-out",
            &self.path("pr-guidance.json"),
        ])?;
        let baseline = self.path("baseline.json");
        self.ripr(&[
            "baseline", "create", "--from", &decision, "--out", &baseline,
        ])?;
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

/// The delta items whose identity seam id is one of `ids`.
fn items_in<'a>(delta: &'a Value, ids: &[String]) -> Vec<&'a Value> {
    let ids: Vec<&str> = ids.iter().map(String::as_str).collect();
    items_for(delta, &ids)
}

fn owner_seam_ids(baseline: &Value, owner: &str) -> Vec<String> {
    seam_ids(&entries_of(baseline, owner))
        .into_iter()
        .map(String::from)
        .collect()
}

struct Scenario {
    repo: Repo,
    baseline_path: String,
    skipped: Vec<String>,
    control: Vec<String>,
    /// The current gate reached `rust_workspace_complete` (a clean build).
    complete: bool,
}

/// A PR-route baseline of both owners' weak gaps, then the current source
/// `after_lib` / `after_tests` committed and gated on the repo route.
/// Returns the repo, the baseline path and the baseline seam ids of
/// `fn_level` and `control`.
fn skipped_after_baseline(after_lib: &str, after_tests: &str) -> Result<Scenario, String> {
    let repo = Repo::new(&lib_source(""), WEAK_TESTS)?;
    let baseline_path = repo.baseline_create()?;
    let baseline = read_json(&baseline_path)?;
    let skipped = owner_seam_ids(&baseline, "src/lib.rs::fn_level");
    let control = owner_seam_ids(&baseline, "src/lib.rs::control");
    // Fixture construction: both owners have baseline debt that records its
    // changed owner.
    if skipped.is_empty() || control.is_empty() {
        return Err(format!(
            "fixture: want baseline entries for both owners, got {baseline}"
        ));
    }
    repo.write(after_lib, after_tests)?;
    repo.commit("after")?;
    let after_gate = repo.gate("after")?;
    let gate = read_json(&after_gate)?;
    // The gate saw the whole workspace, so absence could read as resolved at
    // all (the bug this file pins). A dirty dev build has a per-process
    // analyzer identity, so its gate refuses the complete scope and every
    // absent entry reads not evaluated: only the never-resolved half of each
    // claim is observable there. A clean build (CI) must reach the scope.
    let complete = gate["analysis_scope"]["scope"] == "rust_workspace_complete";
    let identity_refused = gate["analysis_scope"]["coverage_refused"]
        .as_array()
        .is_some_and(|refused| refused.iter().any(|reason| reason == "analyzer_version"));
    if !complete && !(identity_refused && repo.dirty_build()?) {
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
    Ok(Scenario {
        repo,
        baseline_path,
        skipped,
        control,
        complete,
    })
}

#[test]
fn a_skip_added_after_the_baseline_is_withheld_not_resolved() -> Result<(), String> {
    let Scenario {
        repo,
        baseline_path,
        skipped,
        control,
        complete,
    } = skipped_after_baseline(&lib_source(SKIP), WEAK_TESTS)?;
    let after_gate = repo.path("after-gate.json");
    let (delta, markdown) = repo.delta(&baseline_path, &after_gate, "skip")?;
    let items = items_in(&delta, &skipped);
    assert_eq!(items.len(), skipped.len(), "{delta}");
    assert_eq!(delta["delta"]["resolved"], 0, "{delta}");
    if !complete {
        return Ok(());
    }
    for item in &items {
        assert_eq!(item["bucket"], "suppressed", "{item}");
        assert_eq!(item["withheld_by"], "owner_mutants_skip", "{item}");
    }
    assert_eq!(
        delta["analysis_scope"]["owner_opt_outs"]["reason"], "owner_mutants_skip",
        "{delta}"
    );
    assert!(markdown.contains("Withheld by owner opt-out"), "{markdown}");
    // The unskipped owner's seams are still current (as repo-scoped records
    // the gate does not target), and no owner opt-out explains them.
    let control_items = items_in(&delta, &control);
    assert_eq!(control_items.len(), control.len(), "{delta}");
    for item in &control_items {
        assert_eq!(item["bucket"], "suppressed", "{item}");
        assert!(item.get("withheld_by").is_none_or(Value::is_null), "{item}");
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

/// Positive control: debt that really left (its owner deleted) in the same run
/// as the skip still reads as resolved, so the withholding is owner-targeted.
#[test]
fn a_deleted_owner_beside_the_skip_still_reads_resolved() -> Result<(), String> {
    let Scenario {
        repo,
        baseline_path,
        skipped,
        control,
        complete,
    } = skipped_after_baseline(&lib_without_control(), FN_LEVEL_TESTS)?;
    let (delta, _) = repo.delta(&baseline_path, &repo.path("after-gate.json"), "drop")?;
    if !complete {
        // No absence can read as resolved without the complete scope.
        assert_eq!(delta["delta"]["resolved"], 0, "{delta}");
        return Ok(());
    }
    let control_items = items_in(&delta, &control);
    assert_eq!(control_items.len(), control.len(), "{delta}");
    for item in &control_items {
        assert_eq!(item["bucket"], "resolved", "{item}");
    }
    let skipped_items = items_in(&delta, &skipped);
    assert_eq!(skipped_items.len(), skipped.len(), "{delta}");
    for item in &skipped_items {
        assert_eq!(item["bucket"], "suppressed", "{item}");
        assert_eq!(item["withheld_by"], "owner_mutants_skip", "{item}");
    }
    assert_eq!(delta["delta"]["resolved"], control.len(), "{delta}");
    Ok(())
}

/// A baseline written before `changed_owner` was recorded keeps working: the
/// entry in the withheld owner's file reads `not_evaluated`, never resolved.
#[test]
fn a_legacy_baseline_without_changed_owner_is_not_read_as_resolved() -> Result<(), String> {
    let Scenario {
        repo,
        baseline_path,
        skipped,
        ..
    } = skipped_after_baseline(&lib_source(SKIP), WEAK_TESTS)?;
    let mut baseline = read_json(&baseline_path)?;
    for entry in baseline["entries"].as_array_mut().into_iter().flatten() {
        if let Some(object) = entry.as_object_mut() {
            object.remove("changed_owner");
        }
    }
    let legacy = repo.path("legacy-baseline.json");
    fs::write(&legacy, baseline.to_string()).map_err(|e| e.to_string())?;
    let (delta, _) = repo.delta(&legacy, &repo.path("after-gate.json"), "legacy")?;
    let items = items_in(&delta, &skipped);
    assert_eq!(items.len(), skipped.len(), "{delta}");
    for item in &items {
        assert_eq!(item["bucket"], "stale_baseline_entry", "{item}");
        assert_eq!(item["evaluation"], "not_evaluated", "{item}");
    }
    assert_eq!(delta["delta"]["resolved"], 0, "{delta}");
    Ok(())
}
