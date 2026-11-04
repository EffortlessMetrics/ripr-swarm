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
        "ripr-boundary-elision-{}-{stamp}",
        std::process::id()
    ));
    std::fs::create_dir_all(root.join("src")).map_err(|error| error.to_string())?;
    let fixture = Fixture(root);
    std::fs::write(
        fixture.0.join("Cargo.toml"),
        "[package]\nname=\"boundary-elision\"\nversion=\"0.1.0\"\nedition=\"2021\"\n[workspace]\n",
    )
    .map_err(|error| error.to_string())?;
    let mut source = String::from(
        "pub fn classify(x: i32) -> i32 {\n    if x >= 9999 { 1 } else { 2 }\n}\n#[cfg(test)] mod tests {\nuse super::*;\n#[test] fn exercise() {\n",
    );
    for value in 1000..1030 {
        source.push_str(&format!("assert_ne!(classify({value}), -1);\n"));
    }
    if late_match {
        source.push_str("assert_ne!(classify(9999), -1);\n");
    }
    source.push_str("}\n}\n");
    std::fs::write(fixture.0.join("src/lib.rs"), source).map_err(|error| error.to_string())?;
    std::fs::write(
        fixture.0.join("diff.patch"),
        "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,3 +1,3 @@\n pub fn classify(x: i32) -> i32 {\n-    if x > 9999 { 1 } else { 2 }\n+    if x >= 9999 { 1 } else { 2 }\n }\n",
    )
    .map_err(|error| error.to_string())?;
    let output = check_workspace(CheckInput {
        root: fixture.0.clone(),
        diff_file: Some(fixture.0.join("diff.patch")),
        include_unchanged_tests: true,
        ..CheckInput::default()
    })?;
    let raw = render_check(&output, &OutputFormat::CheckFindingsJson)?;
    serde_json::from_str(&raw).map_err(|error| error.to_string())
}

fn predicate(report: &serde_json::Value) -> Result<&serde_json::Value, String> {
    let findings = report["findings"].as_array().ok_or("missing findings")?;
    let mut matching = findings.iter().filter(|f| {
        f["probe"]["line"] == 2
            && f["probe"]["family"] == "predicate"
            && f["probe"]["expression"] == "x >= 9999"
    });
    let finding = matching.next().ok_or("missing changed predicate")?;
    if matching.next().is_some() {
        return Err("more than one changed predicate".into());
    }
    let tests = finding["related_tests"]
        .as_array()
        .ok_or("missing related tests")?;
    if !tests.iter().any(|test| {
        test["name"] == "exercise"
            && test["file"]
                .as_str()
                .is_some_and(|file| file.ends_with("src/lib.rs"))
            && test["line"] == 6
    }) {
        return Err(format!("missing related test identity: {tests:?}"));
    }
    Ok(finding)
}

#[test]
fn boundary_value_listing_discloses_omissions_without_hiding_late_match() -> Result<(), String> {
    let missing = report(false)?;
    let finding = predicate(&missing)?;
    if finding["ripr"]["infect"]["state"] != "weak" {
        return Err(format!("expected weak infection: {finding}"));
    }
    let facts = finding["missing_discriminators"]
        .as_array()
        .ok_or("missing discriminator facts")?;
    let fact = facts
        .iter()
        .find(|fact| fact["value"] == "x == 9999")
        .ok_or("missing boundary discriminator")?;
    let reason = fact["reason"].as_str().ok_or("missing reason")?;
    if !(reason.contains("1000")
        && reason.contains("1015")
        && !reason.contains("1016")
        && !reason.contains("1029")
        && reason.contains("14 values elided")
        && reason.contains("target 9999 value: 9999"))
    {
        return Err(format!("unexpected bounded reason: {reason}"));
    }
    let late = report(true)?;
    let late = predicate(&late)?;
    if late["ripr"]["infect"]["state"] != "yes" {
        return Err(format!("late boundary input was not credited: {late}"));
    }
    if late["missing_discriminators"]
        .as_array()
        .ok_or("missing late facts")?
        .iter()
        .any(|fact| fact["value"] == "x == 9999")
    {
        return Err(format!("late boundary input still marked missing: {late}"));
    }
    Ok(())
}
