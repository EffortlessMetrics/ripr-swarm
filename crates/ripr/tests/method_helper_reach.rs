//! Public-API controls for a private method reached only through a public
//! entry point (#5352; RIPR-SPEC-0159 method hops). Each case writes a small
//! crate, changes the private method's line, and reads the finding through
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
            "ripr-method-helper-{}-{}",
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

const MANIFEST: &str = "[package]\nname = \"method_reach\"\nversion = \"0.1.0\"\nedition = \"2021\"\npublish = false\n";

fn finding(source: &str, diff: &str, family: &str) -> Result<Value, String> {
    let scratch = Scratch::create()?;
    std::fs::write(scratch.0.join("Cargo.toml"), MANIFEST).map_err(|e| e.to_string())?;
    std::fs::write(scratch.0.join("src/lib.rs"), source).map_err(|e| e.to_string())?;
    let diff_file = scratch.0.join("change.diff");
    std::fs::write(&diff_file, diff).map_err(|e| e.to_string())?;
    let report = check_workspace(CheckInput {
        root: scratch.0.clone(),
        diff_file: Some(diff_file),
        mode: Mode::Fast,
        format: OutputFormat::Json,
        include_unchanged_tests: true,
        ..CheckInput::default()
    })?;
    let json: Value = serde_json::from_str(&render_check(&report, &OutputFormat::Json)?)
        .map_err(|e| e.to_string())?;
    let findings = json["findings"].as_array().ok_or("missing findings")?;
    findings
        .iter()
        .find(|f| f["probe"]["family"] == family)
        .or_else(|| {
            findings.iter().find(|f| {
                f["probe"]["expression"]
                    .as_str()
                    .is_some_and(|e| e.contains("1_209_600"))
            })
        })
        .cloned()
        .ok_or_else(|| format!("no {family} finding: {json}"))
}

fn relation_of<'a>(finding: &'a Value, test: &str) -> Option<&'a str> {
    finding["related_tests"]
        .as_array()?
        .iter()
        .find(|r| r["name"] == test)
        .and_then(|r| r["relation_reason"].as_str())
}

// Changed line: `self.n * 1_209_600` inside private `Parser::secs`.
const SECS_DIFF: &str = "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -3,3 +3,3 @@\n impl Parser {\n     fn secs(&self) -> u64 {\n-        self.n * 999_999\n+        self.n * 1_209_600\n";

fn secs_source(tests: &str) -> String {
    format!(
        "struct Parser {{ n: u64 }}\n\nimpl Parser {{\n    fn secs(&self) -> u64 {{\n        self.n * 1_209_600\n    }}\n    fn total(&self) -> u64 {{ self.secs() }}\n}}\n\npub fn entry(n: u64) -> u64 {{\n    Parser {{ n }}.total()\n}}\n\n#[cfg(test)]\nmod tests {{\n    use super::*;\n\n    #[test]\n    fn fortnight_total() {{\n{tests}\n    }}\n}}\n"
    )
}

#[test]
fn public_entry_exact_assertion_credits_private_method() -> Result<(), String> {
    let f = finding(
        &secs_source("        assert_eq!(entry(1), 1_209_600);"),
        SECS_DIFF,
        "return_value",
    )?;
    assert_eq!(
        relation_of(&f, "fortnight_total"),
        Some("helper_owner_call"),
        "{f}"
    );
    assert_eq!(f["classification"], "exposed", "{f}");
    Ok(())
}

#[test]
fn smoke_assertion_through_entry_stays_weak() -> Result<(), String> {
    let f = finding(
        &secs_source("        assert!(entry(1) > 0);"),
        SECS_DIFF,
        "return_value",
    )?;
    assert_ne!(f["classification"], "exposed", "{f}");
    Ok(())
}

#[test]
fn variable_receiver_entry_is_not_credited() -> Result<(), String> {
    // A let-bound receiver's type is not established here, so the hop
    // fails closed: no helper relation, no exposed credit.
    let source = secs_source("        assert_eq!(entry(1), 1_209_600);").replace(
        "    Parser { n }.total()",
        "    let p = Parser { n };\n    p.total()",
    );
    let f = finding(&source, SECS_DIFF, "return_value")?;
    assert_ne!(
        relation_of(&f, "fortnight_total"),
        Some("helper_owner_call"),
        "{f}"
    );
    assert_ne!(f["classification"], "exposed", "{f}");
    Ok(())
}

#[test]
fn match_arm_through_entry_is_not_credited_without_selection() -> Result<(), String> {
    // Humantime shape (#5352): the arm's scrutinee is `self.unit`, set
    // from a string through `Unit::from_str` behind `?`. Selection is
    // not established, so the arm must not read exposed; reach through
    // the method hops is established.
    let source = "use std::time::Duration;\n#[derive(Clone, Copy)]\nenum Unit { Week, Fortnight }\nimpl Unit {\n    fn from_str(s: &str) -> Option<Unit> {\n        match s {\n            \"w\" => Some(Unit::Week),\n            \"fortnight\" => Some(Unit::Fortnight),\n            _ => None,\n        }\n    }\n}\nstruct Parser { n: u64, unit: Unit }\nimpl Parser {\n    fn parse_unit(&self) -> Duration {\n        match self.unit {\n            Unit::Week => Duration::new(self.n * 604_800, 0),\n            Unit::Fortnight => Duration::new(self.n * 1_209_600, 0),\n        }\n    }\n    fn parse(&self) -> Duration { self.parse_unit() }\n}\npub fn parse_duration(n: u64, unit: &str) -> Option<Duration> {\n    let u = Unit::from_str(unit)?;\n    Some(Parser { n, unit: u }.parse())\n}\n#[cfg(test)]\nmod tests {\n    use super::*;\n    #[test]\n    fn fortnight_exact() { assert_eq!(parse_duration(1, \"fortnight\"), Some(Duration::new(1_209_600, 0))); }\n}\n";
    let diff = "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -17,3 +17,3 @@\n         match self.unit {\n             Unit::Week => Duration::new(self.n * 604_800, 0),\n-            Unit::Fortnight => Duration::new(self.n * 999_999, 0),\n+            Unit::Fortnight => Duration::new(self.n * 1_209_600, 0),\n";
    let f = finding(source, diff, "match_arm")?;
    assert_ne!(f["classification"], "exposed", "{f}");
    assert_eq!(f["ripr"]["reach"]["state"], "yes", "{f}");
    Ok(())
}

#[test]
fn wrong_owner_method_is_not_credited() -> Result<(), String> {
    // A second type with its own entry; the test never touches `entry`.
    let source = "struct Parser { n: u64 }\nimpl Parser {\n    fn secs(&self) -> u64 {\n        self.n * 1_209_600\n    }\n    fn total(&self) -> u64 { self.secs() }\n}\nstruct Other { n: u64 }\nimpl Other {\n    fn compute(&self) -> u64 { self.n * 7 }\n}\npub fn entry(n: u64) -> u64 { Parser { n }.total() }\npub fn other_entry(n: u64) -> u64 { Other { n }.compute() }\n#[cfg(test)]\nmod tests {\n    use super::*;\n    #[test]\n    fn other_total() { assert_eq!(other_entry(3), 21); }\n}\n";
    let f = finding(source, SECS_DIFF, "return_value")?;
    assert_ne!(
        relation_of(&f, "other_total"),
        Some("helper_owner_call"),
        "{f}"
    );
    assert_ne!(f["classification"], "exposed", "{f}");
    Ok(())
}
