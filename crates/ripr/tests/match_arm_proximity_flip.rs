//! #6297: editing a same-file test that never calls `seconds` must not
//! flip the `Unit::Fortnight` match-arm verdict.
//!
//! `seconds_total` reaches `seconds` and asserts an exact sum that does not
//! name the arm. `from_str_fortnight` shares the file and names
//! `Unit::Fortnight` through `from_str`. A pooled token-confirmation used to
//! let that second test confirm the arm, so rewriting only its assertion
//! moved the arm from `exposed` to `weakly_exposed`.
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

const FIXTURE_SOURCE: &str = include_str!(
    "../../../fixtures/match_arm_proximity_confirmation_not_credited/input/src/lib.rs"
);
const FIXTURE_CARGO: &str = include_str!(
    "../../../fixtures/match_arm_proximity_confirmation_not_credited/input/Cargo.toml"
);
const FIXTURE_DIFF: &str =
    include_str!("../../../fixtures/match_arm_proximity_confirmation_not_credited/diff.patch");

const NAMING_ASSERTION: &str =
    r#"assert!(matches!(Unit::from_str("fortnight"), Ok(Unit::Fortnight)));"#;
const EDITED_ASSERTION: &str =
    r#"assert_eq!(Unit::from_str("fortnight").map(|u| u == Unit::Week), Ok(false));"#;

fn seconds_arm_finding(source: &str) -> Result<Value, String> {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let scratch = Scratch(std::env::temp_dir().join(format!(
        "ripr-match-arm-proximity-flip-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    )));
    std::fs::create_dir_all(scratch.0.join("src")).map_err(|e| e.to_string())?;
    std::fs::write(scratch.0.join("Cargo.toml"), FIXTURE_CARGO).map_err(|e| e.to_string())?;
    std::fs::write(scratch.0.join("src/lib.rs"), source).map_err(|e| e.to_string())?;
    let diff = scratch.0.join("change.diff");
    std::fs::write(&diff, FIXTURE_DIFF).map_err(|e| e.to_string())?;
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
        .find(|f| f["probe"]["expression"] == "Unit::Fortnight =>")
        .cloned()
        .ok_or_else(|| format!("missing seconds Fortnight arm finding: {json}"))
}

fn require_issue_construction(finding: &Value, from_str_oracle: &str) -> Result<(), String> {
    assert_eq!(
        finding["probe"]["id"], "probe:src_lib.rs:match_arm:9e71545f",
        "subject must be the issue's seconds Fortnight arm: {finding}"
    );
    assert_eq!(finding["probe"]["family"], "match_arm");
    let rows = finding["related_tests"]
        .as_array()
        .ok_or("missing related_tests")?;
    let seconds_total = rows
        .iter()
        .find(|r| r["name"] == "seconds_total")
        .ok_or_else(|| format!("missing seconds_total row: {rows:?}"))?;
    assert_eq!(seconds_total["relation_reason"], "direct_owner_call");
    let from_str = rows
        .iter()
        .find(|r| r["name"] == "from_str_fortnight")
        .ok_or_else(|| format!("missing from_str_fortnight row: {rows:?}"))?;
    assert_eq!(from_str["relation_reason"], "same_test_file");
    assert_eq!(from_str["oracle"], from_str_oracle);
    Ok(())
}

/// The issue's two assertion shapes must share one classification: a
/// same-file `from_str` test cannot confirm the `seconds` arm while
/// `seconds_total` reaches the owner, so editing only that test cannot
/// move `exposed` to `weakly_exposed`.
#[test]
fn editing_from_str_fortnight_does_not_flip_the_seconds_arm() -> Result<(), String> {
    let naming = seconds_arm_finding(FIXTURE_SOURCE)?;
    require_issue_construction(&naming, NAMING_ASSERTION)?;

    let edited_source = FIXTURE_SOURCE.replace(NAMING_ASSERTION, EDITED_ASSERTION);
    if edited_source == FIXTURE_SOURCE {
        return Err("edited assertion was not applied to the fixture source".into());
    }
    let edited = seconds_arm_finding(&edited_source)?;
    require_issue_construction(&edited, EDITED_ASSERTION)?;

    assert_eq!(
        naming["classification"], "weakly_exposed",
        "naming shape (matches! Fortnight) must stay weakly_exposed: {naming}"
    );
    assert_eq!(
        edited["classification"], naming["classification"],
        "editing from_str_fortnight flipped the seconds arm: naming={naming} edited={edited}"
    );
    let naming_summary = naming["ripr"]["discriminate"]["summary"]
        .as_str()
        .ok_or("missing naming discriminate summary")?;
    let edited_summary = edited["ripr"]["discriminate"]["summary"]
        .as_str()
        .ok_or("missing edited discriminate summary")?;
    assert!(
        naming_summary.contains("observation_unverified"),
        "naming shape must stay observation_unverified: {naming_summary}"
    );
    assert!(
        edited_summary.contains("observation_unverified"),
        "edited shape must stay observation_unverified: {edited_summary}"
    );
    Ok(())
}
