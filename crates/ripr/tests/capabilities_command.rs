//! Runs the installed `ripr capabilities` binary end to end (#5531).

use std::process::Command;

fn run(args: &[&str]) -> (bool, String, String) {
    let output = Command::new(env!("CARGO_BIN_EXE_ripr"))
        .args(args)
        .output()
        .map(|output| {
            (
                output.status.success(),
                String::from_utf8_lossy(&output.stdout).into_owned(),
                String::from_utf8_lossy(&output.stderr).into_owned(),
            )
        });
    match output {
        Ok(result) => result,
        Err(err) => (false, String::new(), format!("spawn failed: {err}")),
    }
}

fn json(args: &[&str]) -> serde_json::Value {
    let (ok, stdout, stderr) = run(args);
    assert!(ok, "ripr {args:?} failed: {stderr}");
    serde_json::from_str(&stdout).unwrap_or(serde_json::Value::Null)
}

#[test]
fn json_registry_carries_the_documented_fields() {
    let doc = json(&["capabilities", "--json"]);
    assert_eq!(doc["kind"], "ripr_language_capabilities");
    assert_eq!(doc["schema_version"], "0.1");
    assert_eq!(doc["absent_cell_meaning"], "not_proven");
    assert!(
        doc["semantic_digest"]
            .as_str()
            .is_some_and(|digest| digest.starts_with("sha256:")),
        "{doc}"
    );
    let languages = doc["languages"].as_array().cloned().unwrap_or_default();
    for wire in ["rust", "typescript", "javascript", "python", "perl"] {
        assert!(
            languages.iter().any(|entry| entry["language"] == wire),
            "missing language {wire}"
        );
    }
    let cells = doc["cells"].as_array().cloned().unwrap_or_default();
    assert!(!cells.is_empty());
    for cell in &cells {
        for field in [
            "id",
            "language",
            "framework",
            "mode",
            "family",
            "state",
            "tier_ceiling",
            "proof_refs",
        ] {
            assert!(!cell[field].is_null(), "cell missing {field}: {cell}");
        }
        assert!(
            doc["support_state_vocabulary"]
                .as_array()
                .is_some_and(|vocab| vocab.contains(&cell["state"])),
            "state outside the closed vocabulary: {cell}"
        );
    }
}

#[test]
fn language_filter_narrows_cells_and_keeps_the_digest() {
    let all = json(&["capabilities", "--json"]);
    let python = json(&["capabilities", "--json", "--language", "python"]);
    let cells = python["cells"].as_array().cloned().unwrap_or_default();
    assert!(!cells.is_empty());
    assert!(cells.iter().all(|cell| cell["language"] == "python"));
    assert_eq!(all["semantic_digest"], python["semantic_digest"]);
}

#[test]
fn unknown_language_is_refused_with_a_nonzero_exit() {
    let (ok, _stdout, stderr) = run(&["capabilities", "--language", "cobol"]);
    assert!(!ok);
    assert!(stderr.contains("cobol"), "{stderr}");
}

#[test]
fn human_output_and_help_route() {
    let (ok, stdout, _) = run(&["capabilities"]);
    assert!(ok);
    assert!(stdout.contains("not_proven"), "{stdout}");
    let (ok, help, _) = run(&["capabilities", "--help"]);
    assert!(ok);
    assert!(help.contains("Usage: ripr capabilities"), "{help}");
}
