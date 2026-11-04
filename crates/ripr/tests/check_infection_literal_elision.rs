#![cfg(feature = "lang-rust")]

use ripr::app::{CheckInput, OutputFormat, check_workspace, render_check};
use std::path::PathBuf;

struct Fixture(PathBuf);
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn report(late_match: bool) -> Result<serde_json::Value, String> {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|error| error.to_string())?
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "ripr-infection-elision-{}-{stamp}",
        std::process::id()
    ));
    std::fs::create_dir_all(root.join("src")).map_err(|error| error.to_string())?;
    let fixture = Fixture(root);
    std::fs::write(
        fixture.0.join("Cargo.toml"),
        "[package]\nname=\"infection-elision\"\nversion=\"0.1.0\"\nedition=\"2021\"\n[workspace]\n",
    )
    .map_err(|error| error.to_string())?;
    let mut source = String::from(
        "pub fn classify(x: i32) -> bool {\n    x >= 999\n}\nfn wrapper() -> bool { classify(5) }\n#[cfg(test)] mod tests {\nuse super::*;\n#[test] fn exercise() {\nlet values = [\n",
    );
    for value in 100..140 {
        let value = if late_match && value == 139 {
            999
        } else {
            value
        };
        source.push_str(&format!("    {value},\n"));
    }
    source.push_str("];\nassert_ne!(wrapper(), values.len() == 42);\n}\n}\n");
    std::fs::write(fixture.0.join("src/lib.rs"), source).map_err(|error| error.to_string())?;
    std::fs::write(
        fixture.0.join("diff.patch"),
        "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,5 +1,5 @@\n pub fn classify(x: i32) -> bool {\n-    x > 999\n+    x >= 999\n }\n fn wrapper() -> bool { classify(5) }\n #[cfg(test)] mod tests {\n",
    )
    .map_err(|error| error.to_string())?;
    let output = check_workspace(CheckInput {
        root: fixture.0.clone(),
        diff_file: Some(fixture.0.join("diff.patch")),
        include_unchanged_tests: true,
        ..CheckInput::default()
    })?;
    let raw = render_check(&output, &OutputFormat::Json)?;
    serde_json::from_str(&raw).map_err(|error| error.to_string())
}

fn predicate(report: &serde_json::Value) -> Result<&serde_json::Value, String> {
    let findings = report["findings"].as_array().ok_or("missing findings")?;
    let mut matching = findings.iter().filter(|finding| {
        finding["probe"]["line"] == 2
            && finding["probe"]["family"] == "predicate"
            && finding["probe"]["expression"] == "x >= 999"
    });
    let finding = matching.next().ok_or("missing changed predicate")?;
    if matching.next().is_some() {
        return Err("more than one changed predicate".into());
    }
    let related = finding["related_tests"]
        .as_array()
        .ok_or("missing related tests")?;
    if !related.iter().any(|test| {
        test["name"] == "exercise"
            && test["file"]
                .as_str()
                .is_some_and(|file| file.ends_with("src/lib.rs"))
            && test["line"] == 7
    }) {
        return Err(format!("missing intended related test: {related:?}"));
    }
    Ok(finding)
}

#[test]
fn infection_literal_preview_discloses_elision_without_hiding_late_boundary() -> Result<(), String>
{
    let missing = report(false)?;
    let finding = predicate(&missing)?;
    let infection = &finding["ripr"]["infect"];
    let summary = infection["summary"]
        .as_str()
        .ok_or("missing infection summary")?;
    if finding["classification"] != "weakly_exposed"
        || infection["state"] != "weak"
        || !summary.contains("100, 101")
        || !summary.contains("115, (25 values elided)")
        || summary.contains("116")
        || !summary.contains("changed boundary [999]")
    {
        return Err(format!("unexpected missing-boundary evidence: {finding}"));
    }
    let late = report(true)?;
    let finding = predicate(&late)?;
    let infection = &finding["ripr"]["infect"];
    let summary = infection["summary"]
        .as_str()
        .ok_or("missing infection summary")?;
    if finding["classification"] != "weakly_exposed"
        || infection["state"] != "weak"
        || !summary.contains("changed boundary literal [999] only outside")
        || summary.contains("Tests have literals")
    {
        return Err(format!("late oracle literal was not recognized: {finding}"));
    }
    Ok(())
}
