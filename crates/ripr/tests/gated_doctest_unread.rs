//! #14935: an owner compiled only under a cfg that also gates a doc example
//! ripr refused to read is not reported as a gap.
//!
//! anyhow's `provide` methods are `#[cfg(error_generic_member_access)]`, and
//! the doc examples that exercise them wrap themselves in
//! `#[cfg_attr(not(error_generic_member_access), doc = "# _ = stringify! {")]`
//! so they run only when that cfg is on. The doctest admission refuses those
//! examples, so a "no test is seen calling this" reading of a changed
//! `provide` rested on tests ripr never read and was scored as a false gap.
//!
//! Each case writes a small crate whose `provide` line changes, reads the
//! finding through `check_workspace`, and checks the class and limit. The
//! controls change one link at a time: an ungated owner, a gate on another
//! atom, an example that is not refused, and a test that calls the owner.

use ripr::{CheckInput, Mode, OutputFormat, check_workspace, render_check};
use serde_json::Value;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

const LIMIT: &str = "rust_gated_doctest_unread";

struct Scratch(PathBuf);

impl Scratch {
    fn create() -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "ripr-gated-doctest-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(path.join("src")).map_err(|error| error.to_string())?;
        Ok(Self(path))
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

const MANIFEST: &str =
    "[package]\nname = \"gated_lib\"\nversion = \"0.1.0\"\nedition = \"2021\"\npublish = false\n";

/// An admitted example on `plain` in the owner's file.
const PLAIN: &str = "/// ```\n/// assert_eq!(gated_lib::plain(2), 3);\n/// ```\npub fn plain(x: u32) -> u32 {\n    x + 1\n}\n";

/// The nightly-style example, refused by its `cfg_attr` lines.
fn refused_example(atom: &str) -> String {
    format!(
        "/// ```\n#[cfg_attr(not({atom}), doc = \"# _ = stringify! {{\")]\n/// assert_eq!(gated_lib::boxed(2), 4);\n#[cfg_attr(not({atom}), doc = \"# }};\")]\n/// ```\npub fn boxed(x: u32) -> u32 {{\n    x * 2\n}}\n"
    )
}

/// The same example with nothing that refuses it.
const ADMITTED_EXAMPLE: &str = "/// ```\n/// assert_eq!(gated_lib::boxed(2), 4);\n/// ```\npub fn boxed(x: u32) -> u32 {\n    x * 2\n}\n";

/// An integration test that shares only a name token with `provide`, so the
/// owner has a related test and reads reach `weak`, as anyhow's did.
const NAME_TOKEN_TEST: &str =
    "#[test]\nfn provide_plain_value() {\n    assert_eq!(gated_lib::plain(2), 3);\n}\n";

const CHANGED: &str = "        x + 7";

/// The method `Request::provide` with `gate` above it (empty for none) and
/// `tail` after it. `forward` names `provide` outside its definition, as
/// anyhow's `thiserror_provide` does, so a test that only shares a name token
/// reads reach `weak` rather than `no`.
fn source(example: &str, gate: &str, tail: &str) -> String {
    format!(
        "{PLAIN}\n{example}\npub struct Request;\n\nimpl Request {{\n    {gate}pub fn provide(&self, x: u32) -> u32 {{\n{CHANGED}\n    }}\n\n    {gate}pub fn forward(&self, x: u32) -> u32 {{\n        self.provide(x)\n    }}\n}}\n{tail}"
    )
}

/// The finding on the changed `provide` line, with that line's number.
fn provide_finding(source: &str) -> Result<Value, String> {
    let line = source
        .lines()
        .position(|text| text == CHANGED)
        .map(|index| index + 1)
        .ok_or("changed line not in source")?;
    let scratch = Scratch::create()?;
    std::fs::write(scratch.0.join("Cargo.toml"), MANIFEST).map_err(|error| error.to_string())?;
    std::fs::write(scratch.0.join("src/lib.rs"), source).map_err(|error| error.to_string())?;
    std::fs::create_dir_all(scratch.0.join("tests")).map_err(|error| error.to_string())?;
    std::fs::write(scratch.0.join("tests/t.rs"), NAME_TOKEN_TEST)
        .map_err(|error| error.to_string())?;
    let diff = format!(
        "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -{line},1 +{line},1 @@\n-        x + 1\n+{CHANGED}\n"
    );
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
        .find(|finding| finding["probe"]["line"].as_u64() == Some(line as u64))
        .cloned()
        .ok_or_else(|| format!("no finding on line {line}: {json}"))
}

fn class(finding: &Value) -> &str {
    finding["classification"].as_str().unwrap_or("")
}

/// A class that claims a gap: what the withheld reading moves off.
fn is_gap(finding: &Value) -> bool {
    matches!(class(finding), "reachable_unrevealed" | "weakly_exposed")
}

fn limit(finding: &Value) -> Option<&str> {
    finding["static_limit_kind"].as_str()
}

#[test]
fn an_owner_gated_like_a_refused_example_is_withheld() -> Result<(), String> {
    let finding = provide_finding(&source(
        &refused_example("nightly_api"),
        "#[cfg(nightly_api)]\n    ",
        "",
    ))?;
    assert_eq!(class(&finding), "static_unknown", "{finding}");
    assert_eq!(limit(&finding), Some(LIMIT), "{finding}");
    // The rule only withholds: reach is still the weak same-file reading.
    assert_eq!(finding["ripr"]["reach"]["state"], "weak", "{finding}");
    let next = finding["recommended_next_step"].as_str().unwrap_or("");
    assert!(next.contains("`nightly_api`"), "{next}");
    Ok(())
}

#[test]
fn each_missing_link_keeps_the_gap_reading() -> Result<(), String> {
    // Premise: without the gate the owner reads as a gap from the weak
    // same-file relation, so the withheld case above moved off that reading.
    let ungated = provide_finding(&source(&refused_example("nightly_api"), "", ""))?;
    assert!(is_gap(&ungated), "{ungated}");
    assert_eq!(ungated["ripr"]["reach"]["state"], "weak", "{ungated}");

    let mut wrong = Vec::new();
    for (case, text) in [
        (
            "ungated owner",
            source(&refused_example("nightly_api"), "", ""),
        ),
        (
            "gate on another atom",
            source(
                &refused_example("nightly_api"),
                "#[cfg(other_api)]\n    ",
                "",
            ),
        ),
        (
            "example not refused",
            source(ADMITTED_EXAMPLE, "#[cfg(nightly_api)]\n    ", ""),
        ),
    ] {
        let finding = provide_finding(&text)?;
        if !is_gap(&finding) || limit(&finding) == Some(LIMIT) {
            wrong.push((
                case,
                class(&finding).to_string(),
                finding["static_limit_kind"].clone(),
            ));
        }
    }
    assert!(
        wrong.is_empty(),
        "cases that left the gap reading: {wrong:?}"
    );
    Ok(())
}

#[test]
fn a_test_seen_calling_the_owner_keeps_its_own_reading() -> Result<(), String> {
    let tail = "\n#[cfg(test)]\nmod tests {\n    use super::*;\n\n    #[test]\n    fn calls_provide() {\n        assert_eq!(Request.provide(1), 8);\n    }\n}\n";
    let finding = provide_finding(&source(
        &refused_example("nightly_api"),
        "#[cfg(nightly_api)]\n    ",
        tail,
    ))?;
    assert_ne!(finding["ripr"]["reach"]["state"], "weak", "{finding}");
    assert_ne!(limit(&finding), Some(LIMIT), "{finding}");
    Ok(())
}
