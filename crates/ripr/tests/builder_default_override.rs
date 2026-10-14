//! Builder default override (#6613, RIPR-SPEC-0040 builder override). A test
//! that sets a field through the builder before reading it observes its own
//! value, not the changed default; only a setter whose body plainly stores
//! its argument counts. Each case writes a small crate, changes the default
//! on line 13, and reads the `return_value` finding through
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
            "ripr-builder-override-{}-{}",
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

const MANIFEST: &str = "[package]\nname = \"builder_override\"\nversion = \"0.1.0\"\nedition = \"2021\"\npublish = false\n";

const DIFF: &str = "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -12,3 +12,3 @@ impl RetryBuilder {\n     pub fn new() -> Self {\n-        RetryBuilder { attempts: 3, delay_ms: 100 }\n+        RetryBuilder { attempts: 4, delay_ms: 100 }\n     }\n";

/// The changed default is always on line 13; `setter` fills line 16.
fn source(setter: &str) -> String {
    format!(
        "pub struct Retry {{\n    pub attempts: u32,\n    pub delay_ms: u64,\n}}\n\npub struct RetryBuilder {{\n    attempts: u32,\n    delay_ms: u64,\n}}\n\nimpl RetryBuilder {{\n    pub fn new() -> Self {{\n        RetryBuilder {{ attempts: 4, delay_ms: 100 }}\n    }}\n\n    {setter}\n\n    pub fn build(self) -> Retry {{\n        Retry {{ attempts: self.attempts, delay_ms: self.delay_ms }}\n    }}\n}}\n\n#[cfg(test)]\nmod tests {{\n    use super::*;\n\n    #[test]\n    fn override_sets_attempts() {{\n        let retry = RetryBuilder::new().attempts(5).build();\n        assert_eq!(retry.attempts, 5);\n    }}\n}}\n"
    )
}

fn finding(setter: &str) -> Result<Value, String> {
    let scratch = Scratch::create()?;
    std::fs::write(scratch.0.join("Cargo.toml"), MANIFEST).map_err(|error| error.to_string())?;
    std::fs::write(scratch.0.join("src/lib.rs"), source(setter))
        .map_err(|error| error.to_string())?;
    let diff_file = scratch.0.join("change.diff");
    std::fs::write(&diff_file, DIFF).map_err(|error| error.to_string())?;
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
    let found = json["findings"]
        .as_array()
        .ok_or("missing findings")?
        .iter()
        .find(|finding| {
            finding["probe"]["family"] == "return_value" && finding["probe"]["line"] == 13
        })
        .cloned()
        .ok_or_else(|| format!("no return_value finding on line 13: {json}"))?;
    Ok(found)
}

#[test]
fn a_plain_setter_override_does_not_expose_the_changed_default() -> Result<(), String> {
    for setter in [
        "pub fn attempts(mut self, attempts: u32) -> Self { self.attempts = attempts; self }",
        "pub fn attempts(self, attempts: u32) -> Self { Self { attempts, ..self } }",
    ] {
        let finding = finding(setter)?;
        assert_ne!(finding["classification"], "exposed", "{setter}: {finding}");
    }
    Ok(())
}

#[test]
fn a_setter_that_may_keep_the_default_leaves_the_credit() -> Result<(), String> {
    // `max` keeps the default when it is larger, so the default can still
    // reach the assertion: the name alone proves no override.
    let setter = "pub fn attempts(mut self, attempts: u32) -> Self { self.attempts = attempts.max(self.attempts); self }";
    let finding = finding(setter)?;
    assert_eq!(finding["classification"], "exposed", "{finding}");
    Ok(())
}
