//! #10948 item 6: CLI-level coverage for the two qualifier guards that
//! previously had only narrower proof.
//!
//! (a) A member-crate qualifier (`pricing::` for a member-crate owner) has a
//! CLI-level positive only through the `use`-import route
//! (`member_crate_import_admission_matches_cargo_resolution`); in a
//! single-package fixture — where no member manifest binds the name — both
//! the qualified-call spelling and the `use`-import spelling must fail
//! closed. The own-crate-name spelling is the linking control, so a refusal
//! cannot pass for an unrelated reason.
//!
//! (b) The `layout.class` condition for `crate::` was guarded only by the
//! unit test `crate_qualified_method_observes_the_owner_only_from_its_own_crate`.
//! The CLI counterpart below mirrors that unit test exactly (a rival
//! same-`Self`-type method in `src/main.rs` forces the module check): the
//! library's own inline test observes through `crate::W::dbl`, the binary
//! crate's test with the same spelling does not.
//!
//! Test-side only: no production change.

use super::{assert_success, ignore_remove_dir_all, run_command, unique_temp_workspace};
use std::path::Path;

fn write(root: &Path, path: &str, bytes: impl AsRef<[u8]>) -> Result<(), String> {
    std::fs::write(root.join(path), bytes).map_err(|error| format!("write {path}: {error}"))
}

/// Run `ripr check` on a scratch root whose diff is the file `change.diff`,
/// and return the parsed JSON document. The command must succeed: a
/// nonzero exit is a harness failure, never a qualifier refusal.
fn check(root: &Path) -> Result<serde_json::Value, String> {
    let output = run_command(
        env!("CARGO_BIN_EXE_ripr"),
        Some(root),
        &[
            "check",
            "--root",
            ".",
            "--diff",
            "change.diff",
            "--mode",
            "fast",
            "--format",
            "json",
        ],
    )
    .map_err(|error| format!("run ripr check: {error}"))?;
    assert_success(&output);
    serde_json::from_slice(&output.stdout).map_err(|error| format!("parse check JSON: {error}"))
}

fn single_finding(document: &serde_json::Value, label: &str) -> Result<serde_json::Value, String> {
    let findings = document["findings"]
        .as_array()
        .ok_or_else(|| format!("{label}: missing findings array: {document}"))?;
    if findings.len() != 1 {
        return Err(format!(
            "{label}: single-seam fixture must yield one finding: {document}"
        ));
    }
    Ok(findings[0].clone())
}

fn observe_state(finding: &serde_json::Value) -> Result<&str, String> {
    finding["ripr"]["observe"]["state"]
        .as_str()
        .ok_or_else(|| format!("missing observe state: {finding}"))
}

fn related<'a>(
    finding: &'a serde_json::Value,
    name: &str,
    label: &str,
) -> Result<Option<&'a serde_json::Value>, String> {
    let related = finding["related_tests"]
        .as_array()
        .ok_or_else(|| format!("{label}: missing related tests: {finding}"))?;
    Ok(related
        .iter()
        .find(|test| test["name"] == name || test["name"] == format!("r#{name}")))
}

/// No related test may carry an oracle: with the qualifier refused, nothing
/// in the fixture pins the changed value.
fn assert_no_oracle_credit(finding: &serde_json::Value, label: &str) -> Result<(), String> {
    let related = finding["related_tests"]
        .as_array()
        .ok_or_else(|| format!("{label}: missing related tests: {finding}"))?;
    for test in related {
        if !test["oracle"].as_str().unwrap_or_default().is_empty() {
            return Err(format!(
                "{label}: refused qualifier must not credit an oracle: {test}"
            ));
        }
    }
    Ok(())
}

const SCORE_LIB: &str = "pub fn score(points: i64) -> i64 {\n    3 * points\n}\n";
const SCORE_DIFF: &str = "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,3 +1,3 @@\n pub fn score(points: i64) -> i64 {\n-    points * 3\n+    3 * points\n }\n";
const SCORE_MANIFEST: &str =
    "[package]\nname = \"qual_obs\"\nversion = \"0.1.0\"\nedition = \"2021\"\npublish = false\n";

fn score_test(body: &str) -> String {
    format!("#[test]\nfn observes_score() {{\n{body}\n}}\n")
}

/// The own-crate-name control links, so the refusals below prove the guard
/// rather than an unrelated setup failure.
#[test]
fn an_unbound_member_crate_qualifier_observes_nothing() -> Result<(), String> {
    let root = unique_temp_workspace("qualifier-member-negative");
    let result = (|| {
        std::fs::create_dir_all(root.join("src"))
            .and_then(|()| std::fs::create_dir_all(root.join("tests")))
            .map_err(|error| format!("create fixture: {error}"))?;
        write(&root, "Cargo.toml", SCORE_MANIFEST)?;
        write(&root, "src/lib.rs", SCORE_LIB)?;
        write(&root, "change.diff", SCORE_DIFF)?;

        // Control: the package's own library name observes the owner.
        write(
            &root,
            "tests/api.rs",
            score_test("    assert_eq!(qual_obs::score(7), 21);"),
        )?;
        let control = single_finding(&check(&root)?, "own-crate control")?;
        if control["classification"] != "exposed" {
            return Err(format!("own-crate control must expose the seam: {control}"));
        }
        if observe_state(&control)? != "yes" {
            return Err(format!("own-crate control must observe: {control}"));
        }
        let credited = related(&control, "observes_score", "own-crate control")?
            .ok_or_else(|| format!("own-crate control must relate the test: {control}"))?;
        if credited["relation_reason"] != "direct_owner_call" {
            return Err(format!(
                "own-crate control must be a direct owner call: {credited}"
            ));
        }
        if credited["oracle_strength"] != "strong" {
            return Err(format!(
                "own-crate control must keep the strong oracle: {credited}"
            ));
        }

        // `pricing` names no package of this single-package fixture: no
        // member manifest binds it, so both spellings must fail closed.
        for (label, tests) in [
            (
                "qualified call",
                score_test("    assert_eq!(pricing::score(7), 21);"),
            ),
            (
                "foreign use import",
                format!(
                    "use pricing::score;\n{}",
                    score_test("    assert_eq!(score(7), 21);")
                ),
            ),
        ] {
            write(&root, "tests/api.rs", tests)?;
            let refused = single_finding(&check(&root)?, label)?;
            if refused["classification"] == "exposed" {
                return Err(format!(
                    "{label}: unbound qualifier must not expose: {refused}"
                ));
            }
            if observe_state(&refused)? != "no" {
                return Err(format!(
                    "{label}: unbound qualifier must not observe: {refused}"
                ));
            }
            assert_no_oracle_credit(&refused, label)?;
        }
        Ok(())
    })();
    ignore_remove_dir_all(&root);
    result
}

const DBL_LIB_WITH_UNIT: &str = "pub struct W;\nimpl W {\n    pub fn dbl(x: i32) -> i32 { x + 1 }\n}\n#[cfg(test)]\nmod tests {\n    #[test]\n    fn unit() { assert_eq!(crate::W::dbl(2), 3); }\n}\n";
const DBL_LIB_WITHOUT_UNIT: &str =
    "pub struct W;\nimpl W {\n    pub fn dbl(x: i32) -> i32 { x + 1 }\n}\n";
/// The binary crate's own `W::dbl` rival: the same `Self` type under another
/// crate target, exactly as in the unit test. Its presence forces the module
/// check, so `crate::` must be admitted by `layout.class`, not by name.
const DBL_MAIN: &str = "struct W;\nimpl W {\n    fn dbl(x: i32) -> i32 { x }\n}\nfn main() {}\n#[test]\nfn bin() { assert_eq!(crate::W::dbl(2), 3); }\n";
const DBL_DIFF: &str = "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,4 +1,4 @@\n pub struct W;\n impl W {\n-    pub fn dbl(x: i32) -> i32 { x }\n+    pub fn dbl(x: i32) -> i32 { x + 1 }\n }\n";
const DBL_MANIFEST: &str =
    "[package]\nname = \"crate_guard\"\nversion = \"0.1.0\"\nedition = \"2021\"\npublish = false\n";

/// CLI counterpart of
/// `crate_qualified_method_observes_the_owner_only_from_its_own_crate`: with
/// the rival in the binary crate, `crate::W::dbl` observes from the
/// library's own inline test and stays refused from the binary's test.
#[test]
fn crate_qualified_method_cli_guards_the_layout_class() -> Result<(), String> {
    let root = unique_temp_workspace("qualifier-crate-layout");
    let result = (|| {
        std::fs::create_dir_all(root.join("src"))
            .and_then(|()| std::fs::create_dir_all(root.join("tests")))
            .map_err(|error| format!("create fixture: {error}"))?;
        write(&root, "Cargo.toml", DBL_MANIFEST)?;
        write(&root, "change.diff", DBL_DIFF)?;
        write(&root, "tests/api.rs", "")?;

        // Both crate targets test the same spelling: only the library's own
        // inline test may observe the owner.
        write(&root, "src/lib.rs", DBL_LIB_WITH_UNIT)?;
        write(&root, "src/main.rs", DBL_MAIN)?;
        let finding = single_finding(&check(&root)?, "paired")?;
        if finding["classification"] != "exposed" {
            return Err(format!("paired: the own-crate test must expose: {finding}"));
        }
        let unit = related(&finding, "unit", "paired")?
            .ok_or_else(|| format!("paired: the own-crate test must relate: {finding}"))?;
        if unit["oracle"].as_str().unwrap_or_default().is_empty() {
            return Err(format!(
                "paired: the own-crate test must carry an oracle: {unit}"
            ));
        }
        let bin = related(&finding, "bin", "paired")?;
        if bin.is_some_and(|test| !test["oracle"].as_str().unwrap_or_default().is_empty()) {
            return Err(format!(
                "paired: the binary-crate test must not be credited: {bin:?}"
            ));
        }

        // Without the library's own test, the binary-crate spelling alone
        // observes nothing and exposes nothing.
        write(&root, "src/lib.rs", DBL_LIB_WITHOUT_UNIT)?;
        let refused = single_finding(&check(&root)?, "binary-only")?;
        if refused["classification"] == "exposed" {
            return Err(format!(
                "binary-only: another crate's `crate::` must not expose: {refused}"
            ));
        }
        if observe_state(&refused)? != "no" {
            return Err(format!(
                "binary-only: another crate's `crate::` must not observe: {refused}"
            ));
        }
        assert_no_oracle_credit(&refused, "binary-only")?;
        Ok(())
    })();
    ignore_remove_dir_all(&root);
    result
}
