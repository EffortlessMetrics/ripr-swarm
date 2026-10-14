//! #7763: caller identity and formatted-input limits are distinct from oracle credit.
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

fn finding(source: &str) -> Result<Value, String> {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let scratch = Scratch(std::env::temp_dir().join(format!(
        "ripr-parser-provenance-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    )));
    std::fs::create_dir_all(scratch.0.join("src")).map_err(|e| e.to_string())?;
    std::fs::write(
        scratch.0.join("Cargo.toml"),
        include_str!("../../../fixtures/parser_formatted_input_provenance/input/Cargo.toml"),
    )
    .map_err(|e| e.to_string())?;
    std::fs::write(scratch.0.join("src/lib.rs"), source).map_err(|e| e.to_string())?;
    let diff = scratch.0.join("change.diff");
    std::fs::write(
        &diff,
        include_str!("../../../fixtures/parser_formatted_input_provenance/diff.patch"),
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
        .find(|f| f["probe"]["expression"] == "reason == \"rust_module_unresolved_target\"")
        .cloned()
        .ok_or("missing reason predicate".into())
}

#[test]
fn unrelated_reason_literal_cannot_supply_parser_discrimination() -> Result<(), String> {
    let source =
        include_str!("../../../fixtures/parser_formatted_input_provenance/input/src/lib.rs");
    let actual = finding(source)?;
    let without_decoy = finding(
        &source
            .replace(
                "assert_eq!(plan_reason, \"rust_module_unresolved_target\");",
                "let _ = plan_reason;",
            )
            .replace("assert!(gate_passed);", "let _ = gate_passed;"),
    )?;
    assert_eq!(actual["classification"], "infection_unknown");
    assert_eq!(actual["ripr"]["infect"]["state"], "unknown");
    assert_eq!(
        actual["ripr"]["discriminate"], without_decoy["ripr"]["discriminate"],
        "unrelated reason/gate assertions changed parser discrimination"
    );
    let rows = actual["related_tests"]
        .as_array()
        .ok_or("missing related tests")?;
    for name in [
        "unresolved_target_location_is_required_and_positive",
        "other_parser_condition",
    ] {
        assert!(
            rows.iter()
                .any(|r| r["name"] == name && r["relation_reason"] == "direct_owner_call")
        );
    }
    let decoys: Vec<_> = rows
        .iter()
        .filter(|r| r["name"] == "unrelated_plan_and_gate_oracles")
        .collect();
    assert_eq!(
        decoys.len(),
        2,
        "both plan/gate oracle rows must remain visible"
    );
    for (kind, oracle) in [
        (
            "exact_value",
            "assert_eq!(plan_reason, \"rust_module_unresolved_target\");",
        ),
        ("relational_check", "assert!(gate_passed);"),
    ] {
        let matching: Vec<_> = decoys
            .iter()
            .filter(|r| r["oracle_kind"] == kind && r["oracle"] == oracle)
            .collect();
        assert_eq!(
            matching.len(),
            1,
            "missing or duplicated {kind} decoy oracle"
        );
        assert_eq!(matching[0]["relation_reason"], "same_test_file");
        assert_eq!(matching[0]["miss"], "no_call_path");
    }
    Ok(())
}

#[test]
fn direct_reason_oracle_keeps_credit_beside_the_decoy() -> Result<(), String> {
    let source =
        include_str!("../../../fixtures/parser_formatted_input_provenance/input/src/lib.rs");
    let parser = source
        .split("\n#[cfg(test)]")
        .next()
        .ok_or("missing parser")?;
    let source = format!(
        "{parser}\n#[test]\nfn direct_reason() {{\n    assert_eq!(parse_note(\"ripr: 1 Rust module composition limitation(s): src/a.rs:12:rust_module_unresolved_target; affected module contexts remain fail-closed.\").is_some(), true);\n}}\n#[test]\nfn decoy() {{ assert_eq!(\"rust_module_unresolved_target\", \"rust_module_unresolved_target\"); }}\n"
    );
    let actual = finding(&source)?;
    assert_eq!(actual["ripr"]["discriminate"]["state"], "yes");
    assert_eq!(actual["ripr"]["infect"]["state"], "unknown");
    let rows = actual["related_tests"].as_array().ok_or("missing rows")?;
    assert!(rows.iter().any(|r| r["name"] == "direct_reason"
        && r["relation_reason"] == "direct_owner_call"
        && r.get("miss").is_none()));
    assert!(
        rows.iter()
            .any(|r| r["name"] == "decoy" && r["miss"] == "no_call_path")
    );
    Ok(())
}
