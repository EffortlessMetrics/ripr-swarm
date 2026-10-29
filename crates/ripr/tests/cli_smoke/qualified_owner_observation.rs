//! #10948 item 6: CLI-level coverage for the two qualifier guards that
//! previously had only narrower proof.
//!
//! (a) A member-crate qualifier (`pricing::` for a member-crate owner) has a
//! CLI-level positive only through the `use`-import route
//! (`member_crate_import_admission_matches_cargo_resolution`). In a
//! single-package fixture — where no member manifest binds the name — both
//! the qualified-call spelling (`pricing::score(..)`) and the `use`-import
//! spelling (`use pricing::score;` + `score(..)`) keep the gap open but
//! never fully expose: the coarse related-test fallback may still relate
//! the test, while the owner-return pin refuses the unbound qualifier (no
//! manifest binds it), so the discriminator stays weak and the
//! classification stays below `exposed`. The own-crate-name spelling
//! (`qual_obs::`) is the linking control: it pins and exposes, so a
//! refusal below proves the guard rather than an unrelated setup failure.
//!
//! (b) The `layout.class` condition for `crate::` was guarded only by the
//! unit test `crate_qualified_method_observes_the_owner_only_from_its_own_crate`.
//! The CLI counterpart below mirrors that unit test's rival (a same-`Self`-type
//! method in `src/main.rs`): the rival competes for the owner-return pin, so
//! the paired finding stays `weakly_exposed`, while the rival-free baseline
//! (the library's own inline test alone) pins and exposes. The binary-only
//! phase shows the other direction: from another crate target `crate::`
//! never pins, so the finding stays refused there too.
//!
//! Test-side only: no production change.

use super::{assert_success, ignore_remove_dir_all, run_command, unique_temp_workspace};
use std::path::Path;

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

fn discriminate_state(finding: &serde_json::Value) -> Result<&str, String> {
    finding["ripr"]["discriminate"]["state"]
        .as_str()
        .ok_or_else(|| format!("missing discriminate state: {finding}"))
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

const SCORE_LIB: &str = "pub fn score(points: i64) -> i64 {\n    3 * points\n}\n";
const SCORE_DIFF: &str = "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,3 +1,3 @@\n pub fn score(points: i64) -> i64 {\n-    points * 3\n+    3 * points\n }\n";
const SCORE_MANIFEST: &str =
    "[package]\nname = \"qual_obs\"\nversion = \"0.1.0\"\nedition = \"2021\"\npublish = false\n";

fn score_test(body: &str) -> String {
    format!("#[test]\nfn observes_score() {{\n{body}\n}}\n")
}

/// The own-crate-name control links, so a refusal below proves the guard
/// rather than an unrelated setup failure. `pricing` names no package of
/// this single-package fixture: no member manifest binds it, so both the
/// qualified-call spelling and the `use`-import spelling keep the gap open
/// but must never fully expose (the owner-return pin refuses the unbound
/// qualifier, leaving the discriminator weak).
#[test]
fn an_unbound_member_crate_qualifier_never_exposes() -> Result<(), String> {
    let root = unique_temp_workspace("qualifier-member-negative");
    let result = (|| {
        std::fs::create_dir_all(root.join("src"))
            .and_then(|()| std::fs::create_dir_all(root.join("tests")))
            .map_err(|error| format!("create fixture: {error}"))?;
        write(&root, "Cargo.toml", SCORE_MANIFEST)?;
        write(&root, "src/lib.rs", SCORE_LIB)?;
        write(&root, "change.diff", SCORE_DIFF)?;

        // Control: the package's own library name pins and exposes the owner.
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
        if credited["miss"].is_string() {
            return Err(format!(
                "own-crate control must carry no observation miss: {credited}"
            ));
        }

        // `pricing` binds nothing here, so each same-shaped spelling keeps
        // the gap open but must not expose: the pin refuses the qualifier.
        // The loop's last spelling is the `use`-import route; its miss
        // field is read below, so a regression that silently drops the
        // test from the related set fails instead of passing vacuously.
        let mut last_related: Option<serde_json::Value> = None;
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
            if discriminate_state(&refused)? != "weak" {
                return Err(format!(
                    "{label}: unbound qualifier must leave the discriminator weak: {refused}"
                ));
            }
            // The gap stays open: the coarse fallback still relates the test.
            // A refusal that unrelated the test entirely would hide the
            // oracle-credit question, so require the relation explicitly.
            let kept = related(&refused, "observes_score", label)?
                .cloned()
                .ok_or_else(|| {
                    format!("{label}: the gap must stay open (test must stay related): {refused}")
                })?;
            if kept["oracle"].as_str().unwrap_or_default().is_empty() {
                return Err(format!(
                    "{label}: the related test must keep its oracle visible: {kept}"
                ));
            }
            last_related = Some(kept);
        }
        // The import route's oracle must not confirm observation of the
        // changed expression: the pin refused it.
        let kept = last_related.ok_or("foreign use import must have run")?;
        if kept["miss"] != "observation_unconfirmed" {
            return Err(format!(
                "foreign use import: the unbound import must not confirm observation: {kept}"
            ));
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
/// crate target, exactly as in the unit test. Its presence competes for the
/// owner-return pin, so `crate::` admits the paired finding only weakly.
/// The rival keeps the library's arithmetic (`{ x + 1 }`, asserting `3`) so
/// the only difference between the two crate targets is layout, never the
/// computed value: a discriminating run of the fixture would pass in both.
const DBL_MAIN: &str = "struct W;\nimpl W {\n    fn dbl(x: i32) -> i32 { x + 1 }\n}\nfn main() {}\n#[test]\nfn bin() { assert_eq!(crate::W::dbl(2), 3); }\n";
const DBL_DIFF: &str = "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,4 +1,4 @@\n pub struct W;\n impl W {\n-    pub fn dbl(x: i32) -> i32 { x }\n+    pub fn dbl(x: i32) -> i32 { x + 1 }\n }\n";
const DBL_MANIFEST: &str =
    "[package]\nname = \"crate_guard\"\nversion = \"0.1.0\"\nedition = \"2021\"\npublish = false\n";

/// CLI counterpart of
/// `crate_qualified_method_observes_the_owner_only_from_its_own_crate`: the
/// library's own inline test pins and exposes while it stands alone, but
/// with the same-`Self`-type rival in the binary crate the paired finding
/// stays `weakly_exposed`, and the binary crate's own `crate::` spelling
/// never pins. The paired phase names both tests: the rival must be indexed
/// and related (otherwise the guard would pass vacuously), while the
/// library's own test keeps the oracle the binary one must not confirm.
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

        // Rival-free baseline: the library's own inline test pins the owner
        // through `crate::`, so the seam exposes.
        write(&root, "src/lib.rs", DBL_LIB_WITH_UNIT)?;
        std::fs::remove_file(root.join("src/main.rs")).ok();
        let baseline = single_finding(&check(&root)?, "baseline")?;
        if baseline["classification"] != "exposed" {
            return Err(format!(
                "baseline: the own-crate test must expose without a rival: {baseline}"
            ));
        }
        related(&baseline, "unit", "baseline")?
            .ok_or_else(|| format!("baseline: the own-crate test must relate: {baseline}"))?;

        // With the same-`Self`-type rival in the binary crate, the paired
        // finding must not expose: the rival competes for the pin.
        write(&root, "src/main.rs", DBL_MAIN)?;
        let paired = single_finding(&check(&root)?, "paired")?;
        if paired["classification"] == "exposed" {
            return Err(format!(
                "paired: the rival must withhold full exposure: {paired}"
            ));
        }
        if discriminate_state(&paired)? != "weak" {
            return Err(format!(
                "paired: the rival must leave the discriminator weak: {paired}"
            ));
        }
        // Both spellings stay related: the rival must be indexed (the guard
        // is not passing because the binary target vanished), while only
        // the library's own test keeps a confirmed oracle.
        let unit = related(&paired, "unit", "paired")?
            .ok_or_else(|| format!("paired: the own-crate test must relate: {paired}"))?;
        if unit["oracle"].as_str().unwrap_or_default().is_empty() {
            return Err(format!(
                "paired: the own-crate test must keep its oracle visible: {unit}"
            ));
        }
        let bin = related(&paired, "bin", "paired")?
            .ok_or_else(|| format!("paired: the rival test must stay indexed: {paired}"))?;
        if bin["miss"] != "observation_unconfirmed" {
            return Err(format!(
                "paired: the binary-crate test must not confirm observation: {bin:?}"
            ));
        }

        // Without the library's own test, the binary-crate spelling alone
        // never pins: another crate's `crate::` must not expose.
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
        Ok(())
    })();
    ignore_remove_dir_all(&root);
    result
}

fn write(root: &Path, path: &str, bytes: impl AsRef<[u8]>) -> Result<(), String> {
    std::fs::write(root.join(path), bytes).map_err(|error| format!("write {path}: {error}"))
}
