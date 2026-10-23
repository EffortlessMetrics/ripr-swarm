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
    finding_for(
        source,
        include_str!("../../../fixtures/proximity_unrelated_oracle_not_credited/diff.patch"),
        "input + 2",
    )
}

fn finding_for(source: &str, diff_text: &str, expression: &str) -> Result<Value, String> {
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
    std::fs::write(&diff, diff_text).map_err(|e| e.to_string())?;
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
        .find(|f| f["probe"]["expression"] == expression)
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

/// Review of #9225: a same-file test that reaches the owner through a
/// production function and a `macro_rules!` hop (outer -> call_inner! -> inner)
/// is not unrelated; the name walk cannot see that hop, so it keeps credit,
/// and a chain past the walk bound does too.
#[test]
fn production_macro_hop_keeps_proximity_credit() -> Result<(), String> {
    let diff = "--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,3 +1,3 @@\n pub fn inner(x: u8) -> u8 {\n-    x + 1\n+    x + 2\n }\n";
    let source = "pub fn inner(x: u8) -> u8 {\n    x + 2\n}\n\nmacro_rules! call_inner {\n    ($x:expr) => {\n        inner($x)\n    };\n}\n\npub fn outer(x: u8) -> u8 {\n    call_inner!(x)\n}\n\npub fn smoke() -> u8 {\n    inner(1)\n}\n\n#[cfg(test)]\nmod tests {\n    use super::*;\n\n    #[test]\n    fn smoke_test() {\n        let _ = smoke();\n    }\n\n    #[test]\n    fn outer_triples() {\n        assert_eq!(outer(4), 6);\n    }\n}\n";
    let actual = finding_for(source, diff, "x + 2")?;
    let rows = actual["related_tests"].as_array().ok_or("missing rows")?;
    let outer = rows
        .iter()
        .find(|r| r["name"] == "outer_triples")
        .ok_or("missing outer_triples row")?;
    assert_ne!(
        outer["miss"], "no_call_path",
        "a macro hop to the owner is not an unrelated test: {outer:?}"
    );
    Ok(())
}
