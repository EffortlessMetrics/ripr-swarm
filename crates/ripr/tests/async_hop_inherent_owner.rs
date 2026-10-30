//! An `async fn` hop above an inherent-method owner must not earn helper
//! credit (#7415). Calling an `async fn` constructs a future and nothing in
//! the test establishes polling, so the owner never executes. Synchronous
//! hops below the first `async` caller stay credited. Each case writes a small
//! crate, changes the owner's boundary line, and reads the predicate finding
//! through `check_workspace`.

use ripr::{CheckInput, Mode, OutputFormat, check_workspace, render_check};
use serde_json::Value;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

struct Scratch(PathBuf);

impl Scratch {
    fn create() -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "ripr-async-hop-owner-{}-{}",
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

const MANIFEST: &str = "[package]\nname = \"async_hop_owner\"\nversion = \"0.1.0\"\nedition = \"2021\"\npublish = false\n";

/// The changed owner line is `qty >= self.limit` (line 6).
const DIFF: &str = "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -4,4 +4,4 @@\n impl Meter {\n     fn over(&self, qty: u32) -> bool {\n-        qty > self.limit\n+        qty >= self.limit\n     }\n";

fn source(top: &str, tests: &str) -> String {
    format!(
        "pub struct Meter {{\n    limit: u32,\n}}\nimpl Meter {{\n    fn over(&self, qty: u32) -> bool {{\n        qty >= self.limit\n    }}\n}}\n\n{top}\n#[cfg(test)]\nmod tests {{\n    use super::*;\n\n    #[test]\n    fn subject() {{\n{tests}\n    }}\n}}\n"
    )
}

const FUTURE_ONLY: &str = "        let pending = load(10);\n        assert_eq!(std::mem::size_of_val(&pending) > 0, true);";

const SYNC_LEAF: &str = "fn fetch(qty: u32) -> bool {\n    Meter { limit: 10 }.over(qty)\n}\n";

fn relation(top: &str, tests: &str) -> Result<Option<String>, String> {
    let finding = predicate_finding_of(&source(top, tests))?;
    Ok(relation_of(&finding, "subject").map(str::to_string))
}

fn predicate_finding_of(source: &str) -> Result<Value, String> {
    let scratch = Scratch::create()?;
    std::fs::write(scratch.0.join("Cargo.toml"), MANIFEST).map_err(|error| error.to_string())?;
    std::fs::write(scratch.0.join("src/lib.rs"), source).map_err(|error| error.to_string())?;
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
    json["findings"]
        .as_array()
        .ok_or("missing findings")?
        .iter()
        .find(|finding| finding["probe"]["family"] == "predicate")
        .cloned()
        .ok_or_else(|| format!("no predicate finding: {json}"))
}

fn relation_of<'a>(finding: &'a Value, test: &str) -> Option<&'a str> {
    finding["related_tests"]
        .as_array()?
        .iter()
        .find(|related| related["name"] == test)
        .and_then(|related| related["relation_reason"].as_str())
}

#[test]
fn synchronous_helper_chain_keeps_helper_credit() -> Result<(), String> {
    let direct = relation(SYNC_LEAF, "        assert_eq!(fetch(10), true);")?;
    assert_eq!(direct.as_deref(), Some("helper_owner_call"));
    // An `async fn` caller above the synchronous hop does not take the
    // credit away from a test that calls the synchronous hop itself.
    let below_async = relation(
        &format!("{SYNC_LEAF}async fn load(qty: u32) -> bool {{\n    fetch(qty)\n}}\n"),
        "        assert_eq!(fetch(10), true);",
    )?;
    assert_eq!(below_async.as_deref(), Some("helper_owner_call"));
    Ok(())
}

#[test]
fn async_hops_above_inherent_owner_earn_no_helper_credit() -> Result<(), String> {
    // An `async fn` calling the owner, an `async fn` above a synchronous
    // hop, and a synchronous caller that discards the future all only
    // construct or drop a future; none reaches the owner.
    let async_entry = "async fn load(qty: u32) -> bool {\n    Meter { limit: 10 }.over(qty)\n}\n";
    let async_over_sync =
        format!("{SYNC_LEAF}async fn load(qty: u32) -> bool {{\n    fetch(qty)\n}}\n");
    let sync_over_async = format!(
        "{SYNC_LEAF}async fn load(qty: u32) -> bool {{\n    fetch(qty)\n}}\nfn read(qty: u32) -> bool {{\n    let _ = load(qty);\n    true\n}}\n"
    );
    for (case, top, tests) in [
        ("async entry", async_entry.to_string(), FUTURE_ONLY),
        ("async over sync", async_over_sync, FUTURE_ONLY),
        (
            "sync over async",
            sync_over_async,
            "        assert_eq!(read(10), true);",
        ),
    ] {
        let relation = relation(&top, tests)?;
        assert_ne!(relation.as_deref(), Some("helper_owner_call"), "{case}");
    }
    Ok(())
}
