//! Unicode identifier boundaries on the owner-result field read path: a
//! receiver named `前r` is one identifier, not a delimiter before `r`.

use ripr::{CheckInput, Mode, OutputFormat, check_workspace, render_check};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

const MANIFEST: &str = "[package]\nname = \"unicode-receiver\"\nversion = \"0.1.0\"\nedition = \"2024\"\npublish = false\n\n[dependencies]\n\n[workspace]\n";
const DIFF: &str = "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -7,6 +7,6 @@ pub struct Quote {\n \n pub fn price(n: u32) -> Quote {\n     Quote {\n-        total: n + 1,\n+        total: 1 + n,\n     }\n }\n";

struct Scratch(PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn source(body: &str) -> String {
    format!(
        "#[derive(Debug, Clone, PartialEq)]\npub struct Quote {{\n    pub total: u32,\n}}\n\n\n\npub fn price(n: u32) -> Quote {{\n    Quote {{\n        total: 1 + n,\n    }}\n}}\n\n#[cfg(test)]\nmod tests {{\n    use super::*;\n\n    #[test]\n    fn quotes() {{\n{body}\n    }}\n}}\n"
    )
}

fn classify(body: &str) -> Result<serde_json::Value, String> {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let root = std::env::temp_dir().join(format!(
        "ripr-unicode-receiver-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let scratch = Scratch(root);
    std::fs::create_dir_all(scratch.0.join("src")).map_err(|error| error.to_string())?;
    std::fs::write(scratch.0.join("Cargo.toml"), MANIFEST).map_err(|error| error.to_string())?;
    std::fs::write(scratch.0.join("src/lib.rs"), source(body)).map_err(|error| error.to_string())?;
    let diff = scratch.0.join("change.diff");
    std::fs::write(&diff, DIFF).map_err(|error| error.to_string())?;
    let report = check_workspace(CheckInput {
        root: scratch.0.clone(),
        diff_file: Some(diff),
        mode: Mode::Fast,
        format: OutputFormat::Json,
        include_unchanged_tests: true,
        ..CheckInput::default()
    })?;
    let json: serde_json::Value =
        serde_json::from_str(&render_check(&report, &OutputFormat::Json)?)
            .map_err(|error| error.to_string())?;
    let findings = json["findings"].as_array().ok_or("missing findings")?;
    assert_eq!(findings.len(), 1, "one field subject: {json}");
    let finding = &findings[0];
    assert_eq!(finding["probe"]["family"], "field_construction", "{json}");
    Ok(finding["classification"].clone())
}

#[test]
fn bound_receivers_credit_the_owner_field_ascii_and_unicode_alike() -> Result<(), String> {
    for (label, body) in [
        ("ascii", "let r = price(3);\nassert_eq!(r.total, 4);"),
        ("unicode", "let 前r = price(3);\nassert_eq!(前r.total, 4);"),
    ] {
        assert_eq!(classify(body)?, "exposed", "{label}");
    }
    Ok(())
}

#[test]
fn unrelated_receivers_do_not_credit_the_owner_field() -> Result<(), String> {
    for (label, body) in [
        (
            "ascii_control",
            "let r = price(3);\nlet q = Quote { total: 7 };\nassert_eq!(q.total, 7);\nlet _ = r;",
        ),
        (
            "unicode_prefixed",
            "let r = price(3);\nlet 前r = Quote { total: 7 };\nassert_eq!(前r.total, 7);\nlet _ = r;",
        ),
        (
            "unicode_cjk_text",
            "let r = price(3);\nlet 前r = Quote { total: 7 };\nassert_eq!(\"前/後\".len() + 前r.total as usize, 10);\nlet _ = r;",
        ),
    ] {
        assert_eq!(classify(body)?, "weakly_exposed", "{label}");
    }
    Ok(())
}
