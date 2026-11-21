//! Advice must not imply a value-propagation capability the producer lacks.
use ripr::{CheckInput, Mode, OutputFormat, check_workspace, render_check};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

fn check(source: &str, diff: &str) -> Result<serde_json::Value, String> {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    struct Cleanup(std::path::PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let root = Cleanup(std::env::temp_dir().join(format!(
        "ripr-grid-advice-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    )));
    std::fs::create_dir_all(root.0.join("src")).map_err(|e| e.to_string())?;
    std::fs::write(root.0.join("src/lib.rs"), source).map_err(|e| e.to_string())?;
    std::fs::write(
        root.0.join("Cargo.toml"),
        "[package]\nname='grid_flow'\nversion='0.1.0'\nedition='2021'\n",
    )
    .map_err(|e| e.to_string())?;
    let patch = root.0.join("change.diff");
    std::fs::write(&patch, diff).map_err(|e| e.to_string())?;
    let report = check_workspace(CheckInput {
        root: root.0.clone(),
        diff_file: Some(patch),
        mode: Mode::Fast,
        format: OutputFormat::Json,
        include_unchanged_tests: true,
        ..CheckInput::default()
    })?;
    serde_json::from_str(&render_check(&report, &OutputFormat::Json)?).map_err(|e| e.to_string())
}

#[test]
fn unmapped_field_inputs_do_not_promise_literal_resolution() -> Result<(), String> {
    let corpus = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/rust-verdict-corpus");
    let original =
        std::fs::read_to_string(corpus.join("subjects/authored-grid-flow/src/lib.rs.txt"))
            .map_err(|e| e.to_string())?
            .replace(
                "manifest.version < CURRENT - 2",
                "CURRENT - 2 > manifest.version",
            );
    let diff = std::fs::read_to_string(corpus.join("cases/grid-flow-field-opaque.diff"))
        .map_err(|e| e.to_string())?;
    for source in [
        original.clone(),
        original.replace("lookup(4)", "4").replace("lookup(5)", "5"),
    ] {
        let report = check(&source, &diff)?;
        let findings = report["findings"].as_array().ok_or("findings missing")?;
        assert_eq!(findings.len(), 2);
        for finding in findings {
            assert_eq!(finding["classification"], "infection_unknown");
            assert_eq!(finding["ripr"]["infect"]["state"], "unknown");
            assert_eq!(finding["ripr"]["discriminate"]["state"], "yes");
            let summary = finding["ripr"]["infect"]["summary"]
                .as_str()
                .ok_or("infection summary missing")?;
            assert!(summary.starts_with(
                "Changed boundary input is unresolved: boundary operand `manifest.version`"
            ));
            assert!(!summary.contains("would let ripr decide"), "{summary}");
            assert!(
                summary.contains("literal inputs do not guarantee"),
                "{summary}"
            );
            assert!(summary.contains("required value mapping"), "{summary}");
            assert!(
                finding["recommended_next_step"]
                    .as_str()
                    .ok_or("next step missing")?
                    .contains("Static classification may remain unknown.")
            );
        }
    }
    Ok(())
}

const SCALAR_DIFF: &str = "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,3 +1,3 @@\n pub fn gate(x: u32) -> bool {\n-    x >= 5\n+    x > 5\n }\n";

fn scalar_source(boundary: &str) -> String {
    format!(
        "pub fn gate(x: u32) -> bool {{\n    x > 5\n}}\n#[cfg(test)] mod tests {{ use super::*; #[test] fn values() {{ {boundary} assert_eq!(gate(6), true); }} }}\n"
    )
}

#[test]
fn supported_scalar_literals_keep_credit() -> Result<(), String> {
    let report = check(&scalar_source("assert_eq!(gate(5), false);"), SCALAR_DIFF)?;
    let findings = report["findings"].as_array().ok_or("findings missing")?;
    assert_eq!(findings.len(), 2);
    for finding in findings {
        assert_eq!(finding["classification"], "exposed");
        assert_eq!(finding["ripr"]["infect"]["state"], "yes");
        assert!(
            !finding["ripr"]["infect"]["summary"]
                .as_str()
                .ok_or("summary missing")?
                .contains("required value mapping")
        );
    }
    Ok(())
}

#[test]
fn supported_scalar_missing_boundary_keeps_useful_advice() -> Result<(), String> {
    let report = check(&scalar_source(""), SCALAR_DIFF)?;
    let finding = report["findings"]
        .as_array()
        .ok_or("findings missing")?
        .iter()
        .find(|finding| finding["probe"]["family"] == "predicate")
        .ok_or("predicate missing")?;
    assert_eq!(finding["classification"], "weakly_exposed");
    assert_eq!(finding["ripr"]["infect"]["state"], "weak");
    assert_eq!(
        finding["recommended_next_step"],
        "Add boundary tests for below, equal, and above the changed threshold with exact assertions."
    );
    Ok(())
}
