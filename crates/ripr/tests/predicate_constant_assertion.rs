//! Public-API controls for a predicate finding whose related test asserts
//! only a constant: a call of another, pure function on literal arguments
//! (#15476).
//!
//! The owner is `pub fn discount(total: u32) -> u32`, whose predicate changed
//! from `total >= 100` to `total > 200`. A predicate probe needs no token
//! confirmation, so before #15476 `assert_eq!(label(0), "zero")` in a test
//! that only called `discount(50)` credited `observe: yes` and
//! `discriminate: yes`, while the human card said the assertion was missing.
//! Such an assertion computes the same value on every run, so it observes
//! nothing the owner does, exactly like `assert!(true)` (#14968).
//!
//! Each case writes a small crate and reads the predicate finding through
//! `check_workspace`. Static evidence only: no case here claims a runtime
//! mutation outcome.

use ripr::{CheckInput, Mode, OutputFormat, check_workspace, render_check};
use serde_json::Value;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

struct Scratch(PathBuf);

impl Scratch {
    fn create() -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "ripr-predicate-constant-assertion-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        // The guard owns only a directory this call created, and owns it
        // before anything is written inside.
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

/// The pure helper the issue's test asserts on.
const PURE_LABEL: &str =
    "pub fn label(n: u32) -> &'static str {\n    if n == 0 { \"zero\" } else { \"many\" }\n}\n";

/// The predicate finding for `total >= 100` changed to `total > 200`, with
/// `helpers` beside the owner and `body` as the body of the one unit test.
fn predicate_finding(helpers: &str, body: &str) -> Result<Value, String> {
    predicate_finding_with(helpers, Some(body), None)
}

/// [`predicate_finding`] with the unit test optional and an optional
/// integration test file `tests/it.rs` holding `integration`.
fn predicate_finding_with(
    helpers: &str,
    body: Option<&str>,
    integration: Option<&str>,
) -> Result<Value, String> {
    let scratch = Scratch::create()?;
    let manifest =
        "[package]\nname = \"demo\"\nversion = \"0.1.0\"\nedition = \"2021\"\npublish = false\n";
    let unit = body.map_or_else(String::new, |body| {
        format!(
            "#[cfg(test)]\nmod tests {{\n    use super::*;\n\n    #[test]\n    fn smoke() {{\n        {body}\n    }}\n}}\n"
        )
    });
    let lib = format!(
        "pub fn discount(total: u32) -> u32 {{\n    if total > 200 {{\n        total - 10\n    }} else {{\n        total\n    }}\n}}\n\n{helpers}\n{unit}"
    );
    if let Some(integration) = integration {
        std::fs::create_dir(scratch.0.join("tests")).map_err(|error| error.to_string())?;
        std::fs::write(scratch.0.join("tests/it.rs"), integration)
            .map_err(|error| error.to_string())?;
    }
    let diff = "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -2,1 +2,1 @@\n-    if total >= 100 {\n+    if total > 200 {\n";
    std::fs::write(scratch.0.join("Cargo.toml"), manifest).map_err(|error| error.to_string())?;
    std::fs::write(scratch.0.join("src/lib.rs"), lib).map_err(|error| error.to_string())?;
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

fn stage(finding: &Value, name: &str) -> String {
    finding["ripr"][name]["state"]
        .as_str()
        .unwrap_or("missing")
        .to_string()
}

/// Bodies whose only assertion is a constant of another, pure function.
const CONSTANT: &[&str] = &[
    "let _ = discount(50);\n        assert_eq!(label(0), \"zero\");",
    "let _ = discount(50);\n        assert_eq!(crate::label(0), \"zero\");",
    "let _ = discount(50);\n        assert!(label(1) == \"many\");",
    "let _ = discount(50);\n        assert_ne!(label(0), \"many\");",
];

fn assert_constant_dropped(finding: &Value, case: &str) {
    for name in ["observe", "discriminate"] {
        assert_eq!(stage(finding, name), "no", "{name} for {case}: {finding}");
    }
    assert_eq!(
        finding["classification"], "reachable_unrevealed",
        "{case}: {finding}"
    );
    assert_eq!(
        finding["related_tests"][0]["miss"], "assertion_not_observing",
        "{case}: {finding}"
    );
}

#[test]
fn a_constant_assertion_on_another_pure_function_credits_no_observation() -> Result<(), String> {
    for body in CONSTANT {
        assert_constant_dropped(&predicate_finding(PURE_LABEL, body)?, body);
    }
    // The issue's own shape: an integration test naming the library.
    let integration = "#[test]\nfn smoke() {\n    let _ = demo::discount(50);\n    assert_eq!(demo::label(0), \"zero\");\n}\n";
    assert_constant_dropped(
        &predicate_finding_with(PURE_LABEL, None, Some(integration))?,
        integration,
    );
    Ok(())
}

/// The drop removes only the constant assertion: an assertion on the owner
/// beside it keeps its credit.
#[test]
fn an_owner_assertion_beside_a_constant_keeps_its_credit() -> Result<(), String> {
    let finding = predicate_finding(
        PURE_LABEL,
        "assert_eq!(label(0), \"zero\");\n        assert_eq!(discount(201), 191);\n        assert_eq!(discount(200), 200);",
    )?;
    assert_eq!(stage(&finding, "observe"), "yes", "{finding}");
    assert_eq!(stage(&finding, "discriminate"), "yes", "{finding}");
    Ok(())
}

/// Shapes the constant reading refuses keep the earlier reading: a helper
/// that reads global state, and an argument that is a binding. (A constant
/// inside an `if` is refused by the unit grammar test; this path already
/// reads such an assertion as not credited.)
#[test]
fn a_helper_or_argument_that_may_carry_state_keeps_the_earlier_reading() -> Result<(), String> {
    let baseline = predicate_finding(
        "static SEEN: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);\npub fn label(n: u32) -> u32 {\n    SEEN.load(std::sync::atomic::Ordering::Relaxed) + n\n}\n",
        "let _ = discount(50);\n        assert_eq!(label(0), 0);",
    )?;
    assert_eq!(stage(&baseline, "observe"), "yes", "{baseline}");
    let body = "let n = discount(50);\n        assert_eq!(label(n), \"many\");";
    let finding = predicate_finding(PURE_LABEL, body)?;
    assert_eq!(stage(&finding, "observe"), "yes", "{body}: {finding}");
    Ok(())
}

/// The callee must be the one closed function the assertion's call names:
/// a second function of the name, a callee that reaches the owner, a local
/// shadow of the bare name, and a path the library root does not name all
/// keep the earlier reading.
#[test]
fn a_callee_whose_identity_or_closure_is_unestablished_keeps_the_earlier_reading()
-> Result<(), String> {
    let bare = "let _ = discount(50);\n        assert_eq!(label(0), \"zero\");";
    let cases = [
        (
            format!("{PURE_LABEL}pub mod m {{\n    pub fn label(_n: u32) -> &'static str {{\n        \"zero\"\n    }}\n}}\n"),
            bare.to_string(),
        ),
        (
            "pub fn label(n: u32) -> u32 {\n    discount(n)\n}\n".to_string(),
            "let _ = discount(50);\n        assert_eq!(label(0), 0);".to_string(),
        ),
        (
            PURE_LABEL.to_string(),
            "let label = |_: u32| \"zero\";\n        let _ = discount(50);\n        assert_eq!(label(0), \"zero\");"
                .to_string(),
        ),
        (
            PURE_LABEL.to_string(),
            "let _ = discount(50);\n        assert_eq!(other::label(0), \"zero\");".to_string(),
        ),
    ];
    for (helpers, body) in &cases {
        let finding = predicate_finding(helpers, body)?;
        assert_eq!(
            stage(&finding, "observe"),
            "yes",
            "{helpers}{body}: {finding}"
        );
    }
    Ok(())
}
