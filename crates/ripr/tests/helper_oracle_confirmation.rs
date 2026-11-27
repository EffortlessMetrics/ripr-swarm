//! Fixtures for #13533: a same-file helper's `assert_eq!` and the identical
//! assertion written in the test body must earn the same discriminate state.
//!
//! `facts::test_helpers` copies a credited helper's calls and assertions onto
//! the calling test. The assertion text keeps the helper's own spelling, so a
//! helper parameter that happens to share a name with the owner's parameter
//! (`is_match(hay)`) reads as a token from the changed expression, while the
//! same call in the test body (`is_match("xaby")`) carries no such token.
//! Confirmation must follow what the oracle observes, not the helper's
//! parameter naming.
//!
//! Each case writes a small crate with `src/lib.rs` and `tests/suite.rs`,
//! adds the `!hay.is_empty() &&` guard, and reads the finding through
//! `check_workspace`.

use ripr::{CheckInput, Mode, OutputFormat, check_workspace, render_check};
use serde_json::Value;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

struct Scratch(PathBuf);

impl Scratch {
    fn create() -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "ripr-helper-oracle-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(path.join("src")).map_err(|error| error.to_string())?;
        std::fs::create_dir_all(path.join("tests")).map_err(|error| error.to_string())?;
        Ok(Self(path))
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

const MANIFEST: &str = "[package]\nname = \"helper_oracle\"\nversion = \"0.1.0\"\nedition = \"2021\"\npublish = false\n";

const NEW_BODY: &str = "    !hay.is_empty() && hay.contains(\"ab\")";
const OLD_BODY: &str = "    hay.contains(\"ab\")";

fn library() -> String {
    format!(
        "pub fn is_match(hay: &str) -> bool {{\n{NEW_BODY}\n}}\n#[cfg(test)]\nmod tests {{\n    #[test]\n    fn unrelated_unit() {{\n        let v = vec![1, 2, 3];\n        assert!(!v.is_empty());\n    }}\n}}\n"
    )
}

fn diff() -> Result<String, String> {
    let line = library()
        .lines()
        .position(|line| line == NEW_BODY)
        .ok_or("the owner body line is missing from the library fixture")?
        + 1;
    Ok(format!(
        "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -{line},1 +{line},1 @@\n-{OLD_BODY}\n+{NEW_BODY}\n"
    ))
}

fn finding(suite: &str) -> Result<Value, String> {
    let scratch = Scratch::create()?;
    let write = |relative: &str, text: &str| {
        std::fs::write(scratch.0.join(relative), text).map_err(|error| error.to_string())
    };
    write("Cargo.toml", MANIFEST)?;
    write("src/lib.rs", &library())?;
    write("tests/suite.rs", suite)?;
    write("change.diff", &diff()?)?;
    let report = check_workspace(CheckInput {
        root: scratch.0.clone(),
        diff_file: Some(scratch.0.join("change.diff")),
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
        .find(|finding| {
            finding["probe"]["family"] == "return_value"
                && finding["probe"]["expression"]
                    .as_str()
                    .is_some_and(|expression| expression.contains("hay.is_empty()"))
        })
        .cloned()
        .ok_or_else(|| format!("no return_value finding for the changed body: {json}"))
}

fn discriminate_summary(finding: &Value) -> String {
    finding["ripr"]["discriminate"]["summary"]
        .as_str()
        .unwrap_or_default()
        .to_string()
}

/// Body spelling with a literal argument: carries no token of the change.
const BODY_LITERAL: &str = "use helper_oracle::is_match;\n#[test]\nfn direct() {\n    let want = true;\n    assert_eq!(want, is_match(\"xaby\"));\n}\n";

/// Body spelling that names the owner's parameter, exactly as the helper does.
const BODY_SHARED_NAME: &str = "use helper_oracle::is_match;\n#[test]\nfn direct() {\n    let hay = \"xaby\";\n    let want = true;\n    assert_eq!(want, is_match(hay));\n}\n";

/// The helper's parameter is named like the owner's (`hay`), so the copied
/// assertion text mentions a token of the changed expression.
const HELPER_SHARED_NAME: &str = "use helper_oracle::is_match;\nfn run_case(hay: &str, want: bool) {\n    assert_eq!(want, is_match(hay));\n}\n#[test]\nfn via_helper() {\n    run_case(\"xaby\", true);\n}\n";

/// The same helper with a parameter name that shares nothing with the change.
const HELPER_OTHER_NAME: &str = "use helper_oracle::is_match;\nfn run_case(input: &str, want: bool) {\n    assert_eq!(want, is_match(input));\n}\n#[test]\nfn via_helper() {\n    run_case(\"xaby\", true);\n}\n";

const MIXED: &str = "oracle_confirmation_mixed";

fn assert_unconfirmed(finding: &Value) {
    assert_eq!(finding["classification"], "weakly_exposed", "{finding}");
    assert!(discriminate_summary(finding).contains(MIXED), "{finding}");
}

fn assert_confirmed(finding: &Value) {
    assert_eq!(finding["classification"], "exposed", "{finding}");
    assert!(!discriminate_summary(finding).contains(MIXED), "{finding}");
}

/// Negative, test body: an assertion that shares no token with the change does
/// not confirm observation of it.
#[test]
fn body_assertion_without_a_shared_token_is_unconfirmed() -> Result<(), String> {
    assert_unconfirmed(&finding(BODY_LITERAL)?);
    Ok(())
}

/// Negative, helper: the copied `assert_eq!` earns the same state as the body
/// spelling above. The copy is not a stronger oracle than the original.
#[test]
fn helper_assertion_without_a_shared_token_is_unconfirmed() -> Result<(), String> {
    assert_unconfirmed(&finding(HELPER_OTHER_NAME)?);
    Ok(())
}

/// Positive, test body: an assertion that names the owner's parameter shares a
/// token with the changed expression, which is the static confirmation signal
/// for a value family.
#[test]
fn body_assertion_naming_the_owner_parameter_is_confirmed() -> Result<(), String> {
    assert_confirmed(&finding(BODY_SHARED_NAME)?);
    Ok(())
}

/// Positive, helper: the same token in the helper's own spelling earns the same
/// state. This is the #13533 repro. The `exposed` there is the token-match
/// route that a body assertion takes too, not a copy-specific promotion: change
/// the helper's parameter name (`HELPER_OTHER_NAME`) and it reads
/// `weakly_exposed`, exactly like the body spelling without the token. The two
/// spellings agree whenever the assertion text agrees; they differ only in the
/// argument text the author wrote.
#[test]
fn helper_assertion_naming_the_owner_parameter_matches_the_body_spelling() -> Result<(), String> {
    assert_confirmed(&finding(HELPER_SHARED_NAME)?);
    Ok(())
}
