//! Out-parameter oracle credit (RIPR-SPEC-0351, #8999). A test that passes a
//! local to the changed method as `&mut` and then asserts on that very local
//! observes the method's output. Each case writes a small crate, changes the
//! pushed sign on line 5, and reads the finding on that line through
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
            "ripr-out-parameter-{}-{}",
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

const MANIFEST: &str = "[package]\nname = \"out_parameter\"\nversion = \"0.1.0\"\nedition = \"2021\"\npublish = false\n";

const DIFF: &str = "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -4,3 +4,3 @@ impl OffsetFormat {\n     pub fn format(&self, out: &mut String, minutes: i32) {\n-        out.push(if minutes < 0 { '-' } else { '~' });\n+        out.push(if minutes < 0 { '-' } else { '+' });\n         out.push_str(&format!(\"{:02}\", minutes.abs()));\n";

/// The changed line is always line 5; `body` is the test body.
fn source(body: &str) -> String {
    format!(
        "pub struct OffsetFormat;\n\nimpl OffsetFormat {{\n    pub fn format(&self, out: &mut String, minutes: i32) {{\n        out.push(if minutes < 0 {{ '-' }} else {{ '+' }});\n        out.push_str(&format!(\"{{:02}}\", minutes.abs()));\n    }}\n}}\n\n#[cfg(test)]\nmod tests {{\n    use super::*;\n\n    #[test]\n    fn formats() {{\n{body}\n    }}\n}}\n"
    )
}

fn finding(body: &str) -> Result<Value, String> {
    finding_in(source(body), DIFF.to_string())
}

/// Same crate with a `&mut self` owner, so the receiver is itself a `&mut`
/// parameter that must not shift the out-parameter positions.
fn finding_mut_self(body: &str) -> Result<Value, String> {
    finding_in(
        source(body).replace("(&self, out", "(&mut self, out"),
        DIFF.replace("(&self, out", "(&mut self, out"),
    )
}

fn finding_in(source: String, diff: String) -> Result<Value, String> {
    let scratch = Scratch::create()?;
    std::fs::write(scratch.0.join("Cargo.toml"), MANIFEST).map_err(|error| error.to_string())?;
    std::fs::write(scratch.0.join("src/lib.rs"), source).map_err(|error| error.to_string())?;
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
        .find(|finding| finding["probe"]["line"] == 5)
        .cloned()
        .ok_or_else(|| format!("no finding on line 5: {json}"))
}

const BUILD: &str =
    "        let mut s = String::new();\n        OffsetFormat.format(&mut s, 570);\n";

#[test]
fn asserting_the_local_passed_as_mut_credits_the_call() -> Result<(), String> {
    let finding = finding(&format!("{BUILD}        assert_eq!(s, \"+570\");"))?;
    assert_eq!(finding["classification"], "exposed", "{finding}");
    Ok(())
}

#[test]
fn a_different_asserted_local_is_not_credited() -> Result<(), String> {
    for tail in [
        // another local, not the one passed `&mut`
        "        let t = String::from(\"+570\");\n        assert_eq!(t, \"+570\");",
        // whole word: `s2` is not `s`
        "        let s2 = s.clone();\n        assert_eq!(s2, \"+570\");",
        // assertion that is not on a local
        "        assert_eq!(s.len(), 4);",
    ] {
        let finding = finding(&format!("{BUILD}{tail}"))?;
        assert_ne!(finding["classification"], "exposed", "{tail}: {finding}");
    }
    Ok(())
}

#[test]
fn an_assertion_before_the_call_is_not_credited() -> Result<(), String> {
    let body = "        let mut s = String::new();\n        assert_eq!(s, \"\");\n        OffsetFormat.format(&mut s, 570);";
    let finding = finding(body)?;
    assert_ne!(finding["classification"], "exposed", "{finding}");
    Ok(())
}

#[test]
fn a_rebound_or_reused_local_is_not_credited() -> Result<(), String> {
    for body in [
        // rebound after the call: the assertion reads a fresh local
        "        let mut s = String::new();\n        OffsetFormat.format(&mut s, 570);\n        let mut s = String::from(\"+570\");\n        assert_eq!(s, \"+570\");",
        // written by another statement between definition and call
        "        let mut s = String::new();\n        s.push('x');\n        OffsetFormat.format(&mut s, 570);\n        assert_eq!(s, \"x+570\");",
        // passed twice
        "        let mut s = String::new();\n        OffsetFormat.format(&mut s, 570);\n        OffsetFormat.format(&mut s, 5);\n        assert_eq!(s, \"+570+05\");",
    ] {
        let finding = finding(body)?;
        assert_ne!(finding["classification"], "exposed", "{body}: {finding}");
    }
    Ok(())
}

#[test]
fn a_call_in_another_test_is_not_credited() -> Result<(), String> {
    let body = format!(
        "{BUILD}    }}\n\n    #[test]\n    fn other() {{\n        let mut s = String::from(\"+570\");\n        assert_eq!(s, \"+570\");"
    );
    let finding = finding(&body)?;
    assert_ne!(finding["classification"], "exposed", "{finding}");
    Ok(())
}

#[test]
fn a_mut_self_owner_keeps_receiver_exclusive_positions() -> Result<(), String> {
    // The `&mut self` receiver is not an argument: `&mut s` is argument 0 and
    // maps to `out`, so asserting `s` is credited.
    let credited = finding_mut_self(&format!("{BUILD}        assert_eq!(s, \"+570\");"))?;
    assert_eq!(credited["classification"], "exposed", "{credited}");
    // Passing the local at the `minutes` position (not `&mut`) or a different
    // local at `out` stays uncredited.
    let other = finding_mut_self(
        "        let mut s = String::new();\n        let mut t = String::new();\n        OffsetFormat.format(&mut t, 570);\n        assert_eq!(s, \"\");",
    )?;
    assert_ne!(other["classification"], "exposed", "{other}");
    Ok(())
}

#[test]
fn loop_closure_macro_and_alias_mentions_are_not_credited() -> Result<(), String> {
    for body in [
        // call inside a loop
        "        let mut s = String::new();\n        for _ in 0..1 {\n            OffsetFormat.format(&mut s, 570);\n        }\n        assert_eq!(s, \"+570\");",
        // call inside a closure
        "        let mut s = String::new();\n        let run = || OffsetFormat.format(&mut s, 570);\n        run();\n        assert_eq!(s, \"+570\");",
        // local mentioned by a macro before the assertion
        "        let mut s = String::new();\n        OffsetFormat.format(&mut s, 570);\n        println!(\"{}\", s);\n        assert_eq!(s, \"+570\");",
        // alias of the local is asserted
        "        let mut s = String::new();\n        OffsetFormat.format(&mut s, 570);\n        let alias = &s;\n        assert_eq!(alias, \"+570\");",
        // intervening post-call write
        "        let mut s = String::new();\n        OffsetFormat.format(&mut s, 570);\n        s = String::from(\"+570\");\n        assert_eq!(s, \"+570\");",
        "        let mut s = String::new();\n        OffsetFormat.format(&mut s, 570);\n        s.clear();\n        s.push_str(\"+570\");\n        assert_eq!(s, \"+570\");",
    ] {
        let finding = finding(body)?;
        assert_ne!(finding["classification"], "exposed", "{body}: {finding}");
    }
    Ok(())
}
