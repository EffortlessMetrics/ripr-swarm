//! #9167: a same-file test that cannot reach the owner supplies no oracle while
//! another related test reaches the owner.
use ripr::{CheckInput, Mode, OutputFormat, check_workspace, render_check};
use serde_json::Value;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

struct Scratch(PathBuf);
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

const FIXTURE_SOURCE: &str =
    include_str!("../../../fixtures/proximity_unrelated_oracle_not_credited/input/src/lib.rs");

fn finding(source: &str) -> Result<Value, String> {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let scratch = Scratch(std::env::temp_dir().join(format!(
        "ripr-proximity-oracle-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    )));
    std::fs::create_dir_all(scratch.0.join("src")).map_err(|e| e.to_string())?;
    std::fs::write(
        scratch.0.join("Cargo.toml"),
        include_str!("../../../fixtures/proximity_unrelated_oracle_not_credited/input/Cargo.toml"),
    )
    .map_err(|e| e.to_string())?;
    std::fs::write(scratch.0.join("src/lib.rs"), source).map_err(|e| e.to_string())?;
    let diff = scratch.0.join("change.diff");
    std::fs::write(
        &diff,
        include_str!("../../../fixtures/proximity_unrelated_oracle_not_credited/diff.patch"),
    )
    .map_err(|e| e.to_string())?;
    let report = check_workspace(CheckInput {
        root: scratch.0.clone(),
        diff_file: Some(diff),
        mode: Mode::Fast,
        format: OutputFormat::Json,
        include_unchanged_tests: true,
        ..CheckInput::default()
    })?;
    let json: Value = serde_json::from_str(&render_check(&report, &OutputFormat::Json)?)
        .map_err(|e| e.to_string())?;
    json["findings"]
        .as_array()
        .ok_or("missing findings")?
        .iter()
        .find(|f| f["probe"]["expression"] == "input + 2")
        .cloned()
        .ok_or_else(|| "missing return-value finding".to_string())
}

#[test]
fn unrelated_same_file_assertion_cannot_supply_the_oracle() -> Result<(), String> {
    let actual = finding(FIXTURE_SOURCE)?;
    // Fixture construction: the owner is called by an assertion-free test.
    let rows = actual["related_tests"].as_array().ok_or("missing rows")?;
    assert!(
        rows.iter().any(|r| r["name"] == "make_smoke"
            && r["relation_reason"] == "direct_owner_call"
            && r["oracle_strength"] == "none"),
        "make_smoke must be the assertion-free owner caller: {rows:?}"
    );
    assert_eq!(actual["classification"], "reachable_unrevealed");
    assert_eq!(actual["ripr"]["observe"]["state"], "no");
    // The unrelated assertion stays visible but uncredited.
    let decoys: Vec<_> = rows.iter().filter(|r| r["name"] == "other_test").collect();
    assert_eq!(decoys.len(), 1, "the unrelated assertion row must remain");
    assert_eq!(decoys[0]["relation_reason"], "same_test_file");
    assert_eq!(decoys[0]["miss"], "no_call_path");
    Ok(())
}

#[test]
fn same_file_test_that_reaches_the_owner_through_a_helper_keeps_credit() -> Result<(), String> {
    // Discriminating alternate: `other_test` now reaches `make` through a
    // helper, so its exact assertion may observe the change.
    let source = FIXTURE_SOURCE
        .replace("assert_eq!(other(1), 2);", "assert_eq!(via_helper(1), 3);")
        + "\nfn via_helper(x: u8) -> u8 {\n    make(x)\n}\n";
    let actual = finding(&source)?;
    assert_eq!(actual["ripr"]["observe"]["state"], "yes");
    let rows = actual["related_tests"].as_array().ok_or("missing rows")?;
    assert!(
        rows.iter()
            .any(|r| r["name"] == "other_test" && r["miss"] != "no_call_path"),
        "a test reaching the owner through a helper must keep oracle credit: {rows:?}"
    );
    Ok(())
}
