//! An `async fn` hop above an owner (inherent method or free function) must not earn helper
//! credit (#7415). Calling an `async fn` constructs a future and nothing in
//! the test establishes polling, so the owner never executes. Synchronous
//! hops below the first `async` caller stay credited. Each case writes a small
//! crate, changes the owner's boundary line, and reads the predicate finding
//! through `check_workspace`.
//!
//! The chain is truncated at the first `async` caller, so the activation
//! entry (the function a test must call to earn credit) moves with the
//! truncation: it is the synchronous hop below the `async fn`, never the
//! `async fn` itself. That is why `load_item(10)` is credited while a test
//! that only constructs `load(10)`'s future is not.

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

/// A free-function owner on the same lines: the changed line is again line 6.
const FREE_DIFF: &str = "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -4,4 +4,4 @@\n impl Meter {}\n fn over(qty: u32) -> bool {\n-    qty > 10\n+    qty >= 10\n }\n";
const FREE_OWNER: &str = "pub struct Meter {\n    limit: u32,\n}\nimpl Meter {}\nfn over(qty: u32) -> bool {\n    qty >= 10\n}\n// free owner above\n";
const INHERENT_OWNER: &str = "pub struct Meter {\n    limit: u32,\n}\nimpl Meter {\n    fn over(&self, qty: u32) -> bool {\n        qty >= self.limit\n    }\n}\n";

fn source(top: &str, tests: &str) -> String {
    source_with(INHERENT_OWNER, top, tests)
}

fn source_with(owner: &str, top: &str, tests: &str) -> String {
    format!(
        "{owner}\n{top}\n#[cfg(test)]\nmod tests {{\n    use super::*;\n\n    #[test]\n    fn subject() {{\n{tests}\n    }}\n}}\n"
    )
}

const FUTURE_ONLY: &str = "        let pending = load(10);\n        assert_eq!(std::mem::size_of_val(&pending) > 0, true);";

const SYNC_LEAF: &str = "fn load_item(qty: u32) -> bool {\n    Meter { limit: 10 }.over(qty)\n}\n";

fn relation(top: &str, tests: &str) -> Result<Option<String>, String> {
    let finding = predicate_finding_of(&source(top, tests), DIFF)?;
    Ok(relation_of(&finding, "subject").map(str::to_string))
}

fn free_relation(top: &str, tests: &str) -> Result<Option<String>, String> {
    let finding = predicate_finding_of(&source_with(FREE_OWNER, top, tests), FREE_DIFF)?;
    Ok(relation_of(&finding, "subject").map(str::to_string))
}

fn predicate_finding_of(source: &str, diff: &str) -> Result<Value, String> {
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
    let direct = relation(SYNC_LEAF, "        assert_eq!(load_item(10), true);")?;
    assert_eq!(direct.as_deref(), Some("helper_owner_call"));
    // An `async fn` caller above the synchronous hop does not take the
    // credit away from a test that calls the synchronous hop itself.
    let below_async = relation(
        &format!("{SYNC_LEAF}async fn load(qty: u32) -> bool {{\n    load_item(qty)\n}}\n"),
        "        assert_eq!(load_item(10), true);",
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
        format!("{SYNC_LEAF}async fn load(qty: u32) -> bool {{\n    load_item(qty)\n}}\n");
    let sync_over_async = format!(
        "{SYNC_LEAF}async fn load(qty: u32) -> bool {{\n    load_item(qty)\n}}\nfn read(qty: u32) -> bool {{\n    let _ = load(qty);\n    true\n}}\n"
    );
    // The exact non-credit relation: the future-only test still shares the
    // owner's file, so it surfaces as `same_test_file` and nothing stronger.
    // Asserting only "not helper_owner_call" would also pass if the test
    // vanished from `related_tests` or fell to some other credited relation.
    let mut mismatched = Vec::new();
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
        if relation.as_deref() != Some("same_test_file") {
            mismatched.push((case, relation));
        }
    }
    assert!(
        mismatched.is_empty(),
        "cases whose relation is not exactly same_test_file: {mismatched:?}"
    );
    Ok(())
}

#[test]
fn undeclared_callee_earns_no_helper_credit_but_declared_sync_does() -> Result<(), String> {
    // Same test text, `load(10)`: with no `load` declaration anywhere there is
    // no hop to follow, so the relation stays `same_test_file`. Declaring a
    // synchronous `load` over the owner flips exactly this relation.
    let test = "        assert_eq!(load(10), true);";
    let undeclared = relation("", test)?;
    assert_eq!(undeclared.as_deref(), Some("same_test_file"));
    let declared_sync = relation(
        "fn load(qty: u32) -> bool {\n    Meter { limit: 10 }.over(qty)\n}\n",
        test,
    )?;
    assert_eq!(declared_sync.as_deref(), Some("helper_owner_call"));
    // An undeclared name whose test only builds a future-shaped value is the
    // same: nothing declared, nothing credited.
    let undeclared_future = relation("", FUTURE_ONLY)?;
    assert_eq!(undeclared_future.as_deref(), Some("same_test_file"));
    Ok(())
}

#[test]
fn async_hops_above_free_owner_earn_no_helper_credit_but_sync_hops_do() -> Result<(), String> {
    // Control: the harness can credit a free owner (`helper_owner_call`
    // through a sync hop), so the non-credit below is not a harness artifact.
    let free_sync = "fn load_item(qty: u32) -> bool {\n    over(qty)\n}\n";
    let direct = free_relation(free_sync, "        assert_eq!(load_item(10), true);")?;
    assert_eq!(direct.as_deref(), Some("helper_owner_call"));
    // #11151: an unpolled `async fn` above a free owner never executes it, so
    // it is refused exactly like the inherent-owner case (#7415 / #11065).
    let async_over_free =
        format!("{free_sync}async fn load(qty: u32) -> bool {{\n    load_item(qty)\n}}\n");
    let unpolled = free_relation(&async_over_free, FUTURE_ONLY)?;
    assert_eq!(unpolled.as_deref(), Some("same_test_file"));
    // The synchronous hop below the `async fn` still earns credit.
    let below_async = free_relation(&async_over_free, "        assert_eq!(load_item(10), true);")?;
    assert_eq!(below_async.as_deref(), Some("helper_owner_call"));
    Ok(())
}

#[test]
fn awaited_async_hop_is_a_known_conservative_gap_for_both_owner_kinds() -> Result<(), String> {
    // Known limitation, pinned so a future polling-aware change flips it
    // deliberately: the helper chain is resolved per probe, not per test, and
    // establishes no polling. A test that awaits or blocks on the `async fn`
    // hop does execute the owner, yet earns no helper credit for either owner
    // kind. This is a fail-closed recall gap, never a promotion.
    let async_free = "fn load_item(qty: u32) -> bool {\n    over(qty)\n}\nasync fn load(qty: u32) -> bool {\n    load_item(qty)\n}\n";
    let async_inherent =
        "async fn load(qty: u32) -> bool {\n    Meter { limit: 10 }.over(qty)\n}\n";
    let awaited = "        assert_eq!(block_on(load(10)), true);";
    let free = free_relation(async_free, awaited)?;
    assert_eq!(free.as_deref(), Some("same_test_file"));
    let inherent = relation(async_inherent, awaited)?;
    assert_eq!(inherent.as_deref(), Some("same_test_file"));
    Ok(())
}
