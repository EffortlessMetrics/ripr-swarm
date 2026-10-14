//! Evidence-only regression for #3161. Retaining a returned Vec ledger
//! (#7355/SPEC-0159) does not establish a String helper's exact observer.
//! No production admission or classification upgrade is introduced here.

#[path = "data/returned_render_buffer.rs"]
mod native;

use ripr::{CheckInput, Mode, OutputFormat, check_workspace, render_check};
use serde_json::Value;

const SOURCE: &str = include_str!("data/returned_render_buffer.rs");
const DIAGNOSTIC: &str = "Focused proof evidence remains incomplete.";
const CALL: &str = "    render_pr_signal(text, \"Focused proof evidence remains incomplete.\");";

struct Scratch(std::path::PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn analyze(case: &str, source: &str) -> Result<Value, String> {
    static SEQUENCE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let scratch = Scratch(std::env::temp_dir().join(format!(
        "ripr-returned-render-{}-{}-{case}",
        std::process::id(),
        SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    )));
    std::fs::create_dir_all(scratch.0.join("src")).map_err(|e| e.to_string())?;
    std::fs::write(
        scratch.0.join("Cargo.toml"),
        "[package]\nname=\"render_witness\"\nversion=\"0.1.0\"\nedition=\"2024\"\n",
    )
    .map_err(|e| e.to_string())?;
    std::fs::write(scratch.0.join("src/lib.rs"), source).map_err(|e| e.to_string())?;
    let line = source
        .lines()
        .position(|line| line == CALL)
        .ok_or("missing unique changed call")?
        + 1;
    assert_eq!(source.lines().filter(|line| *line == CALL).count(), 1);
    let diff = format!(
        "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -{line},1 +{line},1 @@\n-    render_pr_signal(text, \"old diagnostic\");\n+{CALL}\n"
    );
    let diff_file = scratch.0.join("change.diff");
    std::fs::write(&diff_file, diff).map_err(|e| e.to_string())?;
    let report = check_workspace(CheckInput {
        root: scratch.0.clone(),
        diff_file: Some(diff_file),
        mode: Mode::Ready,
        format: OutputFormat::Json,
        include_unchanged_tests: true,
        ..CheckInput::default()
    })?;
    assert!(!report.findings.is_empty(), "{case}: empty analysis");
    let json: Value = serde_json::from_str(&render_check(&report, &OutputFormat::Json)?)
        .map_err(|e| e.to_string())?;
    assert_eq!(
        json["analysis_outcome"]["analysis_complete"], true,
        "{case}: incomplete analysis: {json}"
    );
    let selected = json["findings"]
        .as_array()
        .ok_or("missing findings")?
        .iter()
        .filter(|f| f["probe"]["family"] == "call_deletion" && f["probe"]["line"] == line)
        .collect::<Vec<_>>();
    assert_eq!(selected.len(), 1, "{case}: unique helper call: {json}");
    Ok(selected[0].clone())
}

#[test]
fn returned_string_and_orthogonal_oracles_remain_unconfirmed() -> Result<(), String> {
    let production = SOURCE
        .split("#[cfg(test)]")
        .next()
        .ok_or("production source")?;
    let actual =
        format!("let rendered = rendered_summary(); assert!(rendered.contains(\"{DIAGNOSTIC}\"));");
    let sibling = format!(
        "let _actual = rendered_summary(); let sibling = String::from(\"{DIAGNOSTIC}\"); assert!(sibling.contains(\"{DIAGNOSTIC}\"));"
    );
    let shadow = format!(
        "append_diagnostic(&mut text);\n    let text = String::from(\"{DIAGNOSTIC}\");\n    text"
    );
    let overwritten = format!(
        "append_diagnostic(&mut text);\n    text.clear();\n    text.push_str(\"{DIAGNOSTIC}\");\n    text"
    );
    let wrong = format!(
        "{}\nfn other_render(text: &mut String) {{ text.push_str(\"{DIAGNOSTIC}\"); }}\n",
        production.replace("append_diagnostic(&mut text);", "other_render(&mut text);")
    );
    let cases = [
        ("returned", production.to_string(), actual.clone()),
        (
            "later_write",
            production.replace(
                "append_diagnostic(&mut text);",
                "append_diagnostic(&mut text);\n    text.push_str(\"after\\n\");",
            ),
            actual.clone(),
        ),
        ("sibling_only", production.to_string(), sibling),
        (
            "ignored_return",
            production.to_string(),
            "let _ = rendered_summary(); assert!(\"unchanged\".contains(\"unchanged\"));"
                .to_string(),
        ),
        (
            "shadowed_buffer",
            production.replace("append_diagnostic(&mut text);\n    text", &shadow),
            actual.clone(),
        ),
        (
            "overwritten_buffer",
            production.replace("append_diagnostic(&mut text);\n    text", &overwritten),
            actual.clone(),
        ),
        ("wrong_helper", wrong, actual),
    ];
    for (case, production, oracle) in cases {
        let source = format!("{production}\n#[test]\nfn observes() {{ {oracle} }}\n");
        let finding = analyze(case, &source)?;
        assert_ne!(finding["classification"], "exposed", "{case}: {finding}");
        assert_ne!(
            finding["ripr"]["discriminate"]["state"], "yes",
            "{case}: {finding}"
        );
        let rows = finding["related_tests"].as_array().ok_or("related tests")?;
        assert!(
            rows.iter().any(|row| row["name"] == "observes"),
            "{case}: missing actual oracle row: {finding}"
        );
    }
    Ok(())
}
