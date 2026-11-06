//! Literal option chains retain input identity through the boundary producer.
use ripr::{CheckInput, Mode, OutputFormat, check_workspace, render_check};
use serde_json::Value;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

const STRIP: &str = include_str!(
    "../../../fixtures/rust-verdict-corpus/subjects/authored-spec-b-value/src/v_strip.rs.txt"
);
const TOKENS: &str =
    include_str!("../../../fixtures/rust-verdict-corpus/subjects/authored-tokens/src/flow.rs.txt");
struct Scratch(PathBuf);
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn findings(source: &str, before: &str, after: &str) -> Result<Vec<Value>, String> {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let root = std::env::temp_dir();
    std::fs::create_dir_all(&root).map_err(|e| e.to_string())?;
    let scratch = Scratch(root.join(format!(
        "ripr-option-chain-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    )));
    std::fs::create_dir_all(scratch.0.join("src")).map_err(|e| e.to_string())?;
    std::fs::write(
        scratch.0.join("Cargo.toml"),
        "[package]\nname=\"option_boundary\"\nversion=\"0.1.0\"\nedition=\"2024\"\n[workspace]\n",
    )
    .map_err(|e| e.to_string())?;
    let source = source.replace(before, after);
    std::fs::write(scratch.0.join("src/lib.rs"), &source).map_err(|e| e.to_string())?;
    let line = source
        .lines()
        .position(|line| line.trim() == after)
        .ok_or("changed predicate missing")?
        + 1;
    let diff = scratch.0.join("change.diff");
    std::fs::write(&diff, format!("diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -{line} +{line} @@\n-    {before}\n+    {after}\n")).map_err(|e| e.to_string())?;
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
    let findings: Vec<Value> = json["findings"]
        .as_array()
        .ok_or("missing findings")?
        .iter()
        .filter(|finding| {
            matches!(
                finding["probe"]["family"].as_str(),
                Some("predicate" | "return_value")
            )
        })
        .filter(|finding| finding["probe"]["expression"] == after)
        .cloned()
        .collect();
    assert_eq!(
        findings.len(),
        2,
        "one predicate and returned comparison: {json}"
    );
    for finding in &findings {
        assert_eq!(finding["probe"]["line"], line);
    }
    Ok(findings)
}
fn exposed(findings: &[Value]) {
    for f in findings {
        assert_eq!(f["classification"], "exposed", "{f}");
    }
}
fn not_exposed(findings: &[Value]) {
    for f in findings {
        assert_ne!(f["classification"], "exposed", "{f}");
    }
}

#[test]
fn literal_option_chains_authenticate_the_original_discriminators() -> Result<(), String> {
    let strip = findings(STRIP, "n == 2", "n < 3")?;
    exposed(&strip);
    for f in &strip {
        let facts = f["activation"]["observed_values"]
            .as_array()
            .ok_or("missing value facts")?;
        assert!(
            facts.iter().any(|v| v["value"] == "n == 0"
                && v["provenance"]
                    .as_str()
                    .is_some_and(|p| p.contains("strip_prefix")
                        && p.contains("map_or")
                        && p.contains("exact comparison alternatives"))),
            "{f}"
        );
        assert!(
            f["activation"]["missing_discriminators"]
                .as_array()
                .ok_or("missing gaps")?
                .is_empty()
        );
    }
    exposed(&findings(TOKENS, "name.len() >= 2", "2 <= name.len()")?);
    exposed(&findings(TOKENS, "name.len() >= 2", "name.len() > 1")?);
    Ok(())
}

#[test]
fn option_chain_local_identity_and_opaque_inputs_stay_uncredited() -> Result<(), String> {
    for source in [
        TOKENS.replace("let name =", "let mut name =").replace(
            "    name.len() >= 2",
            "    name = \"ab\";\n    name.len() >= 2",
        ),
        TOKENS.replace(
            "    let name = arg",
            "    let arg = \"\";\n    let name = arg",
        ),
        TOKENS
            .replace("long_flag(arg: &str)", "long_flag(arg: &str, other: &str)")
            .replace("let name = arg.", "let name = other.")
            .replace(
                "assert!(long_flag(\"--ab\"));",
                "assert!(!long_flag(\"--ab\", \"--a\"));",
            )
            .replace("long_flag(\"--a\")", "long_flag(\"--a\", \"--a\")"),
        TOKENS
            .replace("long_flag(\"--ab\")", "long_flag(lookup(\"--ab\"))")
            .replace("long_flag(\"--a\")", "long_flag(lookup(\"--a\"))")
            + "\nfn lookup(arg: &str) -> &str { arg }\n",
    ] {
        not_exposed(&findings(&source, "name.len() >= 2", "2 <= name.len()")?);
    }
    let opaque = STRIP.replace("map_or(\"\",", "map_or(lookup(),")
        + "\nfn lookup() -> &'static str { \"\" }\n";
    for f in findings(&opaque, "n == 2", "n < 3")? {
        assert_eq!(f["classification"], "infection_unknown", "{f}");
    }
    Ok(())
}

#[test]
fn exact_alternative_requires_the_same_operand_and_the_observed_call() -> Result<(), String> {
    let changed = STRIP.replace("n == 2", "n < 3");
    not_exposed(&findings(&changed, "other == 2", "n < 3")?);
    not_exposed(&findings(&changed, "2 == n", "n < 3")?);
    let prefix = STRIP.split("#[cfg(test)]").next().ok_or("missing tests")?;
    for tests in [
        "#[test] fn only_same_outcome() { assert!(flag_long_enough(\"--ab\")); }",
        "#[test] fn discarded_boundary() { let _ = flag_long_enough(\"ab\"); }\n#[test] fn pin_other_row() { assert!(flag_long_enough(\"--ab\")); }",
        "#[test] fn computed_expected() { assert_eq!(flag_long_enough(\"ab\"), flag_long_enough(\"ab\")); }",
        "#[test] fn masked() { assert!(flag_long_enough(\"--ab\") || !flag_long_enough(\"ab\")); }",
        "#[test] fn message() { assert_eq!(flag_long_enough(\"--ab\"), true, \"{:?}\", flag_long_enough(\"ab\")); }",
    ] {
        let source = format!("{prefix}\n#[cfg(test)]\nmod tests {{ use super::*;\n{tests}\n}}\n");
        not_exposed(&findings(&source, "n == 2", "n < 3")?);
    }
    Ok(())
}

#[test]
fn generic_method_names_cannot_mint_literal_option_values() -> Result<(), String> {
    let source = STRIP
        .replace(
            "pub fn flag_long_enough(arg: &str)",
            "pub fn flag_long_enough<T: Like>(arg: T)",
        )
        .replace(
            "assert!(!flag_long_enough(\"ab\"));",
            "assert!(flag_long_enough(\"ab\"));",
        )
        + "\npub trait Like { fn strip_prefix(&self, prefix: &str) -> Option<&'static str>; }\nimpl Like for &'static str { fn strip_prefix(&self, _prefix: &str) -> Option<&'static str> { Some(\"ab\") } }\n";
    for finding in findings(&source, "n == 2", "n < 3")? {
        assert_eq!(finding["classification"], "infection_unknown", "{finding}");
        assert!(
            !finding["activation"]["observed_values"]
                .as_array()
                .ok_or("missing values")?
                .iter()
                .any(|fact| fact["value"]
                    .as_str()
                    .is_some_and(|value| value.contains(" == ")))
        );
    }
    Ok(())
}

#[test]
fn mutable_option_locals_cannot_borrow_their_pre_alias_value() -> Result<(), String> {
    let source = STRIP
        .replace("let n =", "let mut n =")
        .replace(
            "    n == 2",
            "    let alias = &mut n;\n    *alias = 2;\n    n == 2",
        )
        .replace(
            "assert!(!flag_long_enough(\"ab\"));",
            "assert!(flag_long_enough(\"ab\"));",
        );
    for finding in findings(&source, "n == 2", "n < 3")? {
        assert_eq!(finding["classification"], "infection_unknown", "{finding}");
    }
    let source = TOKENS
        .replace("let name =", "let mut name =")
        .replace(
            "    name.len() >= 2",
            "    let alias = &mut name;\n    *alias = \"ab\";\n    name.len() >= 2",
        )
        .replace(
            "assert!(!long_flag(\"--a\"));",
            "assert!(long_flag(\"--a\"));",
        );
    for finding in findings(&source, "name.len() >= 2", "2 <= name.len()")? {
        assert_eq!(finding["classification"], "infection_unknown", "{finding}");
    }
    Ok(())
}
