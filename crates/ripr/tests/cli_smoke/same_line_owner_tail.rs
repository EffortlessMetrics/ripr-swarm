//! SPEC-0226/0186: a returned predicate keeps its own same-test discriminator.
use super::{run_command, unique_external_workspace};
use serde_json::{Value, json};
use std::path::Path;

const BASE: &str = include_str!(
    "../../../../fixtures/rust-verdict-corpus/subjects/authored-spec0226-1/src/lib.rs.txt"
);
const DIFF: &str = include_str!(
    "../../../../fixtures/rust-verdict-corpus/cases/spec0226-gate-split-boundary-agreement.diff"
);

struct Cleanup<'a>(&'a Path);
impl Drop for Cleanup<'_> {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(self.0);
    }
}

fn check(source: &str, diff: &str) -> Result<Value, Box<dyn std::error::Error>> {
    let root = unique_external_workspace("same-line-owner-tail")?;
    std::fs::create_dir_all(root.join("src"))?;
    let _cleanup = Cleanup(&root);
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname=\"owner_tail\"\nversion=\"0.0.0\"\nedition=\"2024\"\n",
    )?;
    std::fs::write(root.join("src/lib.rs"), source)?;
    let patch = root.join("change.diff");
    std::fs::write(&patch, diff)?;
    let output = run_command(
        env!("CARGO_BIN_EXE_ripr"),
        Some(&root),
        &[
            "check",
            "--root",
            root.to_str().ok_or("non-UTF8 root")?,
            "--diff",
            patch.to_str().ok_or("non-UTF8 patch")?,
            "--json",
        ],
    )?;
    assert!(output.status.success(), "{output:?}");
    let report: Value = serde_json::from_slice(&output.stdout)?;
    assert_eq!(
        report["analysis_outcome"]["analysis_complete"], true,
        "{report}"
    );
    assert_eq!(report["summary"]["changed_rust_files"], 1, "{report}");
    assert_eq!(report["summary"]["findings"], 2, "{report}");
    Ok(report)
}

fn finding<'a>(report: &'a Value, family: &str) -> Result<&'a Value, String> {
    let rows = report["findings"].as_array().ok_or("missing findings")?;
    let selected: Vec<_> = rows
        .iter()
        .filter(|row| row["probe"]["family"] == family)
        .collect();
    assert_eq!(selected.len(), 1, "{family}: {report}");
    selected
        .first()
        .copied()
        .ok_or_else(|| format!("missing {family}"))
}

fn postimage() -> String {
    assert_eq!(
        BASE.matches("pub fn gate(input: u32) -> bool { input >= 10 }")
            .count(),
        1
    );
    BASE.replace(
        "pub fn gate(input: u32) -> bool { input >= 10 }",
        "pub fn gate(input: u32) -> bool { 10 <= input }",
    )
}

#[test]
fn retained_split_boundary_tail_keeps_the_predicate_gap_and_exposed_return()
-> Result<(), Box<dyn std::error::Error>> {
    let report = check(&postimage(), DIFF)?;
    let predicate = finding(&report, "predicate")?;
    let returned = finding(&report, "return_value")?;
    assert_eq!(predicate["probe"]["line"], 3);
    assert_eq!(predicate["probe"]["expression"], "10 <= input");
    assert_eq!(
        predicate["ripr"]["propagate"]["state"], "yes",
        "{predicate}"
    );
    assert_eq!(
        predicate["flow_sinks"],
        json!([{"kind":"return_value", "text":"10 <= input", "line":3}])
    );
    assert_eq!(predicate["classification"], "weakly_exposed", "{predicate}");
    assert_eq!(predicate["ripr"]["discriminate"]["state"], "weak");
    assert!(
        predicate["ripr"]["discriminate"]["summary"]
            .as_str()
            .ok_or("missing discriminate summary")?
            .contains("same_test_pairing_missing")
    );
    assert_eq!(returned["classification"], "exposed", "{returned}");
    for row in [predicate, returned] {
        assert_eq!(row["related_tests_total"], 2);
        let tests = row["related_tests"]
            .as_array()
            .ok_or("missing related tests")?;
        let mut identity: Vec<_> = tests
            .iter()
            .map(|test| (test["file"].as_str(), test["name"].as_str()))
            .collect();
        identity.sort();
        assert_eq!(
            identity,
            [
                (Some("src/lib.rs"), Some("boundary")),
                (Some("src/lib.rs"), Some("far"))
            ]
        );
    }
    Ok(())
}

#[test]
fn same_test_boundary_assertion_exposes_the_same_line_predicate()
-> Result<(), Box<dyn std::error::Error>> {
    let source = postimage().replace("        gate(10);", "        assert_eq!(gate(10), true);");
    let report = check(&source, DIFF)?;
    let predicate = finding(&report, "predicate")?;
    assert_eq!(predicate["classification"], "exposed", "{predicate}");
    assert_eq!(predicate["ripr"]["propagate"]["state"], "yes");
    assert_eq!(predicate["ripr"]["discriminate"]["state"], "yes");
    assert_eq!(
        finding(&report, "return_value")?["classification"],
        "exposed"
    );
    Ok(())
}

#[test]
fn boundary_calls_without_any_oracle_do_not_expose_the_predicate()
-> Result<(), Box<dyn std::error::Error>> {
    let source = postimage().replace("        assert_eq!(gate(100), true);", "        gate(100);");
    let report = check(&source, DIFF)?;
    let predicate = finding(&report, "predicate")?;
    assert_eq!(
        predicate["ripr"]["propagate"]["state"], "yes",
        "{predicate}"
    );
    assert_eq!(
        predicate["classification"], "reachable_unrevealed",
        "{predicate}"
    );
    Ok(())
}
