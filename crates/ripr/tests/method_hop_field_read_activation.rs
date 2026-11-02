//! #9603 item 3 / RIPR-SPEC-0158 examples 13–14: the verdict corpus pins
//! exposure for the hit shape and the gap for the trap, but activation
//! infection must be witnessed separately so a regression that credits the
//! trap row at infection time is caught before pairing.

use ripr::{CheckInput, Mode, OutputFormat, check_workspace, render_check};
use serde_json::Value;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

const SOURCE: &str = include_str!(
    "../../../fixtures/rust-verdict-corpus/subjects/authored-method-hop/src/lib.rs.txt"
);
const MANIFEST: &str =
    include_str!("../../../fixtures/rust-verdict-corpus/subjects/authored-method-hop/Cargo.toml");
const HIT_DIFF: &str =
    include_str!("../../../fixtures/rust-verdict-corpus/cases/method-hop-field-read-hit.diff");
const TRAP_DIFF: &str =
    include_str!("../../../fixtures/rust-verdict-corpus/cases/method-hop-field-read-trap.diff");

struct Scratch(PathBuf);

impl Scratch {
    fn create() -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "ripr-method-hop-field-read-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).map_err(|error| error.to_string())?;
        let scratch = Self(path);
        std::fs::create_dir(scratch.0.join("src")).map_err(|error| error.to_string())?;
        Ok(scratch)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn predicate_finding(source: &str, diff: &str) -> Result<Value, String> {
    let scratch = Scratch::create()?;
    std::fs::write(scratch.0.join("Cargo.toml"), MANIFEST).map_err(|error| error.to_string())?;
    std::fs::write(scratch.0.join("src/lib.rs"), source).map_err(|error| error.to_string())?;
    let diff_file = scratch.0.join("change.diff");
    std::fs::write(&diff_file, diff).map_err(|error| error.to_string())?;
    let report = check_workspace(CheckInput {
        root: scratch.0.clone(),
        diff_file: Some(diff_file),
        mode: Mode::Fast,
        format: OutputFormat::Json,
        include_unchanged_tests: true,
        ..CheckInput::default()
    })?;
    let json: Value = serde_json::from_str(&render_check(&report, &OutputFormat::Json)?)
        .map_err(|error| error.to_string())?;
    json["findings"]
        .as_array()
        .ok_or("missing findings")?
        .iter()
        .find(|finding| finding["probe"]["family"] == "predicate")
        .cloned()
        .ok_or_else(|| format!("no predicate finding: {json}"))
}

#[test]
fn method_hop_field_read_hit_reads_infection_yes() -> Result<(), String> {
    let finding = predicate_finding(SOURCE, HIT_DIFF)?;
    assert_eq!(
        finding["ripr"]["infect"]["state"].as_str(),
        Some("yes"),
        "{finding}"
    );
    Ok(())
}

#[test]
fn method_hop_field_read_trap_does_not_read_infection_yes() -> Result<(), String> {
    let finding = predicate_finding(SOURCE, TRAP_DIFF)?;
    let state = finding["ripr"]["infect"]["state"]
        .as_str()
        .ok_or_else(|| format!("missing ripr.infect.state on trap finding: {finding}"))?;
    assert_ne!(state, "yes", "{finding}");
    Ok(())
}
