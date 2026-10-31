//! An `async fn` hop above an owner (inherent method or free function) earns
//! helper credit only for a test that provably polls it (#7415, #11151).
//! Calling an `async fn` constructs a future and nothing about the call alone
//! establishes polling, so a test that only constructs or drops the future
//! never executes the owner. A test that `.await`s the call in code it
//! executes, or hands it to a test-local helper the poll-helper reader
//! verified, does. Synchronous hops below the first `async` caller stay
//! credited. Each case writes a small crate, changes the owner's boundary
//! line, and reads the predicate finding through `check_workspace`.
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

/// How the `subject` test is declared.
#[derive(Clone, Copy)]
enum Harness {
    /// A plain `#[test] fn`.
    Sync,
    /// A `#[tokio::test] async fn` whose body can `.await`.
    AsyncTest,
    /// A plain `#[test] fn` with a test-local `block_on` that polls one future
    /// to completion (the shape RIPR-SPEC-0197 verifies from source).
    VerifiedBlockOn,
    /// A plain `#[test] fn` whose `block_on` only spells the name: it never
    /// polls the future it is handed.
    CounterfeitBlockOn,
}

const POLL_HELPER: &str = "    use std::future::Future;\n    use std::pin::pin;\n    use std::task::{Context, Poll, Waker};\n\n    fn block_on<F: Future>(fut: F) -> F::Output {\n        let mut fut = pin!(fut);\n        let mut cx = Context::from_waker(Waker::noop());\n        loop {\n            if let Poll::Ready(v) = fut.as_mut().poll(&mut cx) {\n                return v;\n            }\n        }\n    }\n\n";
const COUNTERFEIT_HELPER: &str = "    fn block_on<F>(_fut: F) -> bool {\n        true\n    }\n\n";

fn source_in(owner: &str, top: &str, tests: &str, harness: Harness) -> String {
    let (preamble, header) = match harness {
        Harness::Sync => ("", "#[test]\n    fn subject()"),
        Harness::AsyncTest => ("", "#[tokio::test]\n    async fn subject()"),
        Harness::VerifiedBlockOn => (POLL_HELPER, "#[test]\n    fn subject()"),
        Harness::CounterfeitBlockOn => (COUNTERFEIT_HELPER, "#[test]\n    fn subject()"),
    };
    format!(
        "{owner}\n{top}\n#[cfg(test)]\nmod tests {{\n    use super::*;\n\n{preamble}    {header} {{\n{tests}\n    }}\n}}\n"
    )
}

const FUTURE_ONLY: &str = "        let pending = load(10);\n        assert_eq!(std::mem::size_of_val(&pending) > 0, true);";

const SYNC_LEAF: &str = "fn load_item(qty: u32) -> bool {\n    Meter { limit: 10 }.over(qty)\n}\n";

fn relation(top: &str, tests: &str) -> Result<Option<String>, String> {
    relation_in(top, tests, Harness::Sync)
}

fn relation_in(top: &str, tests: &str, harness: Harness) -> Result<Option<String>, String> {
    let finding = predicate_finding_of(&source_in(INHERENT_OWNER, top, tests, harness), DIFF)?;
    Ok(relation_of(&finding, "subject").map(str::to_string))
}

fn free_relation(top: &str, tests: &str) -> Result<Option<String>, String> {
    free_relation_in(top, tests, Harness::Sync)
}

fn free_relation_in(top: &str, tests: &str, harness: Harness) -> Result<Option<String>, String> {
    let finding = predicate_finding_of(&source_in(FREE_OWNER, top, tests, harness), FREE_DIFF)?;
    Ok(relation_of(&finding, "subject").map(str::to_string))
}

fn predicate_finding_of(source: &str, diff: &str) -> Result<Value, String> {
    predicate_finding_in_files(source, diff, &[])
}

fn predicate_finding_in_files(
    source: &str,
    diff: &str,
    extra: &[(&str, &str)],
) -> Result<Value, String> {
    let scratch = Scratch::create()?;
    std::fs::write(scratch.0.join("Cargo.toml"), MANIFEST).map_err(|error| error.to_string())?;
    std::fs::write(scratch.0.join("src/lib.rs"), source).map_err(|error| error.to_string())?;
    for (name, text) in extra {
        std::fs::write(scratch.0.join("src").join(name), text)
            .map_err(|error| error.to_string())?;
    }
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

/// The owner's two spellings with an async entry directly over it, and the
/// same over a synchronous hop, so each polling shape runs against both owner
/// kinds and both chain depths.
const INHERENT_ASYNC_ENTRY: &str =
    "async fn load(qty: u32) -> bool {\n    Meter { limit: 10 }.over(qty)\n}\n";
const FREE_ASYNC_ENTRY: &str = "async fn load(qty: u32) -> bool {\n    over(qty)\n}\n";
const FREE_ASYNC_OVER_SYNC: &str = "fn load_item(qty: u32) -> bool {\n    over(qty)\n}\nasync fn load(qty: u32) -> bool {\n    load_item(qty)\n}\n";

/// Run one polling shape against both owner kinds.
fn both_owner_kinds(
    inherent_top: &str,
    free_top: &str,
    tests: &str,
    harness: Harness,
) -> Result<(Option<String>, Option<String>), String> {
    Ok((
        relation_in(inherent_top, tests, harness)?.map(|r| r.to_string()),
        free_relation_in(free_top, tests, harness)?.map(|r| r.to_string()),
    ))
}

fn assert_both(case: &str, got: (Option<String>, Option<String>), expected: &str) {
    assert_eq!(
        (got.0.as_deref(), got.1.as_deref()),
        (Some(expected), Some(expected)),
        "{case}: (inherent, free) relations"
    );
}

#[test]
fn awaited_async_hop_in_an_async_test_keeps_helper_credit_for_both_owner_kinds()
-> Result<(), String> {
    let got = both_owner_kinds(
        INHERENT_ASYNC_ENTRY,
        FREE_ASYNC_ENTRY,
        "        assert_eq!(load(10).await, true);",
        Harness::AsyncTest,
    )?;
    assert_both("await inside assert_eq", got, "helper_owner_call");
    // The same call bound first, then asserted, and the parenthesized form.
    let got = both_owner_kinds(
        INHERENT_ASYNC_ENTRY,
        FREE_ASYNC_ENTRY,
        "        let reached = (load(10)).await;\n        assert!(reached);",
        Harness::AsyncTest,
    )?;
    assert_both("await bound then asserted", got, "helper_owner_call");
    // An `async fn` over a synchronous hop: the sync hop needs no polling, the
    // async entry is awaited.
    let inherent_over_sync =
        format!("{SYNC_LEAF}async fn load(qty: u32) -> bool {{\n    load_item(qty)\n}}\n");
    let got = both_owner_kinds(
        &inherent_over_sync,
        FREE_ASYNC_OVER_SYNC,
        "        assert_eq!(load(10).await, true);",
        Harness::AsyncTest,
    )?;
    assert_both("await async over sync", got, "helper_owner_call");
    Ok(())
}

#[test]
fn async_chain_credit_needs_every_async_hop_polled() -> Result<(), String> {
    let test = "        assert_eq!(load(10).await, true);";
    // Two async hops, the upper one awaiting the lower.
    let inherent_chain = "async fn inner(qty: u32) -> bool {\n    Meter { limit: 10 }.over(qty)\n}\nasync fn load(qty: u32) -> bool {\n    inner(qty).await\n}\n".to_string();
    let free_chain = "async fn inner(qty: u32) -> bool {\n    over(qty)\n}\nasync fn load(qty: u32) -> bool {\n    inner(qty).await\n}\n";
    let got = both_owner_kinds(&inherent_chain, free_chain, test, Harness::AsyncTest)?;
    assert_both("both async hops polled", got, "helper_owner_call");
    // The awaited entry builds the lower future and drops it: the owner never
    // runs even though the test polls `load`.
    let inherent_dropped = "async fn inner(qty: u32) -> bool {\n    Meter { limit: 10 }.over(qty)\n}\nasync fn load(qty: u32) -> bool {\n    let _unpolled = inner(qty);\n    true\n}\n".to_string();
    let free_dropped = "async fn inner(qty: u32) -> bool {\n    over(qty)\n}\nasync fn load(qty: u32) -> bool {\n    let _unpolled = inner(qty);\n    true\n}\n";
    let got = both_owner_kinds(&inherent_dropped, free_dropped, test, Harness::AsyncTest)?;
    assert_both("lower hop built but never polled", got, "same_test_file");
    Ok(())
}

#[test]
fn block_on_credit_requires_a_verified_polling_helper() -> Result<(), String> {
    let driven = "        assert_eq!(block_on(load(10)), true);";
    // A test-local helper the poll-helper reader verified from source drives
    // the future: credited.
    let got = both_owner_kinds(
        INHERENT_ASYNC_ENTRY,
        FREE_ASYNC_ENTRY,
        driven,
        Harness::VerifiedBlockOn,
    )?;
    assert_both("verified test-local block_on", got, "helper_owner_call");
    // The same call text against a helper that only spells the name and never
    // polls is not driving anything.
    let got = both_owner_kinds(
        INHERENT_ASYNC_ENTRY,
        FREE_ASYNC_ENTRY,
        driven,
        Harness::CounterfeitBlockOn,
    )?;
    assert_both("counterfeit block_on", got, "same_test_file");
    // No helper at all: a bare `block_on` is a name, not a proof.
    let got = both_owner_kinds(
        INHERENT_ASYNC_ENTRY,
        FREE_ASYNC_ENTRY,
        driven,
        Harness::Sync,
    )?;
    assert_both("undefined block_on", got, "same_test_file");
    // A third-party executor is refused by design: ripr verifies no external
    // crate's polling semantics.
    for external in [
        "        assert_eq!(futures::executor::block_on(load(10)), true);",
        "        let rt = tokio::runtime::Runtime::new().unwrap();\n        assert_eq!(rt.block_on(load(10)), true);",
    ] {
        let got = both_owner_kinds(
            INHERENT_ASYNC_ENTRY,
            FREE_ASYNC_ENTRY,
            external,
            Harness::Sync,
        )?;
        assert_both(external, got, "same_test_file");
    }
    // Driving a hop whose own callee is unpolled still fails closed.
    let chain_dropped_inherent = "async fn inner(qty: u32) -> bool {\n    Meter { limit: 10 }.over(qty)\n}\nasync fn load(qty: u32) -> bool {\n    let _unpolled = inner(qty);\n    true\n}\n";
    let chain_dropped_free = "async fn inner(qty: u32) -> bool {\n    over(qty)\n}\nasync fn load(qty: u32) -> bool {\n    let _unpolled = inner(qty);\n    true\n}\n";
    let got = both_owner_kinds(
        chain_dropped_inherent,
        chain_dropped_free,
        driven,
        Harness::VerifiedBlockOn,
    )?;
    assert_both("driven entry over an unpolled hop", got, "same_test_file");
    Ok(())
}

#[test]
fn unpolled_async_hops_stay_refused_even_in_an_async_test() -> Result<(), String> {
    // Each body constructs the future in an async test and never polls it.
    let mut mismatched = Vec::new();
    for (case, tests) in [
        ("held in a binding", "        let _pending = load(10);"),
        (
            "bound then size-inspected",
            "        let pending = load(10);\n        assert!(std::mem::size_of_val(&pending) > 0);",
        ),
        ("dropped", "        drop(load(10));"),
        (
            "awaited only inside an async block that is never run",
            "        let _block = async { load(10).await };",
        ),
        (
            "awaited only inside a closure that is never called",
            "        let _later = || async { load(10).await };",
        ),
        (
            "one call polled, another dropped",
            "        let _spare = load(1);\n        assert_eq!(load(10).await, true);",
        ),
        (
            "handed to another call",
            "        let pending = std::future::ready(load(10));\n        assert!(pending.await);",
        ),
    ] {
        let got = both_owner_kinds(
            INHERENT_ASYNC_ENTRY,
            FREE_ASYNC_ENTRY,
            tests,
            Harness::AsyncTest,
        )?;
        if (got.0.as_deref(), got.1.as_deref()) != (Some("same_test_file"), Some("same_test_file"))
        {
            mismatched.push((case, got));
        }
    }
    assert!(
        mismatched.is_empty(),
        "cases whose relation is not exactly same_test_file for both owner kinds: {mismatched:?}"
    );
    Ok(())
}

#[test]
fn undeclared_awaited_callee_earns_no_helper_credit() -> Result<(), String> {
    // `load` is declared nowhere, so there is no hop to follow whatever the
    // test does with the call; and an owner-side `async fn` declared sync
    // cannot be driven by the awaited call to an undeclared name either.
    let got = both_owner_kinds(
        "",
        "",
        "        assert_eq!(load(10).await, true);",
        Harness::AsyncTest,
    )?;
    assert_both("awaited but undeclared", got, "same_test_file");
    Ok(())
}

#[test]
fn awaiting_a_synchronous_hop_below_an_async_entry_keeps_its_own_credit() -> Result<(), String> {
    // The test calls the synchronous hop; it neither needs nor uses polling,
    // and the awaited async entry beside it in the file changes nothing.
    let inherent_top =
        format!("{SYNC_LEAF}async fn load(qty: u32) -> bool {{\n    load_item(qty)\n}}\n");
    let got = both_owner_kinds(
        &inherent_top,
        FREE_ASYNC_OVER_SYNC,
        "        assert_eq!(load_item(10), true);",
        Harness::AsyncTest,
    )?;
    assert_both(
        "sync hop called from an async test",
        got,
        "helper_owner_call",
    );
    Ok(())
}

const ASYNC_INHERENT_OWNER: &str = "pub struct Meter {\n    limit: u32,\n}\nimpl Meter {\n    async fn over(&self, qty: u32) -> bool {\n        qty >= self.limit\n    }\n}\n";
const ASYNC_FREE_OWNER: &str = "pub struct Meter {\n    limit: u32,\n}\nimpl Meter {}\nasync fn over(qty: u32) -> bool {\n    qty >= 10\n}\n// free owner above\n";

fn async_owner_relations(
    inherent_hop: &str,
    free_hop: &str,
    tests: &str,
) -> Result<(Option<String>, Option<String>), String> {
    let inherent = predicate_finding_of(
        &source_in(
            ASYNC_INHERENT_OWNER,
            inherent_hop,
            tests,
            Harness::AsyncTest,
        ),
        DIFF,
    )?;
    let free = predicate_finding_of(
        &source_in(ASYNC_FREE_OWNER, free_hop, tests, Harness::AsyncTest),
        FREE_DIFF,
    )?;
    Ok((
        relation_of(&inherent, "subject").map(str::to_string),
        relation_of(&free, "subject").map(str::to_string),
    ))
}

#[test]
fn an_async_owner_future_must_be_driven_by_the_lowest_hop() -> Result<(), String> {
    let test = "        assert_eq!(load(10).await, true);";
    // The hop awaits the owner's future: credited.
    let got = async_owner_relations(
        "async fn load(qty: u32) -> bool {\n    Meter { limit: 10 }.over(qty).await\n}\n",
        "async fn load(qty: u32) -> bool {\n    over(qty).await\n}\n",
        test,
    )?;
    assert_both("owner future awaited", got, "helper_owner_call");
    // The test polls `load`, but `load` builds the owner's future and drops it.
    for (case, inherent, free) in [
        (
            "owner future discarded as a statement",
            "async fn load(qty: u32) -> bool {\n    Meter { limit: 10 }.over(qty);\n    true\n}\n",
            "async fn load(qty: u32) -> bool {\n    over(qty);\n    true\n}\n",
        ),
        (
            "owner future bound to the wildcard",
            "async fn load(qty: u32) -> bool {\n    let _ = Meter { limit: 10 }.over(qty);\n    true\n}\n",
            "async fn load(qty: u32) -> bool {\n    let _ = over(qty);\n    true\n}\n",
        ),
    ] {
        let got = async_owner_relations(inherent, free, test)?;
        assert_both(case, got, "same_test_file");
    }
    Ok(())
}

#[test]
fn a_hop_local_binding_named_like_the_callee_is_not_the_hop() -> Result<(), String> {
    let test = "        assert_eq!(load(10).await, true);";
    // `inner` is a closure the entry binds and awaits; the real `inner` (which
    // reaches the owner) is never polled.
    let got = both_owner_kinds(
        "async fn inner(qty: u32) -> bool {\n    Meter { limit: 10 }.over(qty)\n}\nasync fn load(qty: u32) -> bool {\n    let inner = |_qty: u32| async { true };\n    inner(qty).await\n}\n",
        "async fn inner(qty: u32) -> bool {\n    over(qty)\n}\nasync fn load(qty: u32) -> bool {\n    let inner = |_qty: u32| async { true };\n    inner(qty).await\n}\n",
        test,
        Harness::AsyncTest,
    )?;
    assert_both("shadowing closure binding", got, "same_test_file");
    Ok(())
}

const FUTURE_INHERENT_OWNER: &str = "pub struct Meter {\n    limit: u32,\n}\nimpl Meter {\n    fn over(&self, qty: u32) -> impl std::future::Future<Output = bool> {\n        async move { qty >= self.limit }\n    }\n}\n";
const FUTURE_FREE_OWNER: &str = "pub struct Meter {\n    limit: u32,\n}\nimpl Meter {}\nfn over(qty: u32) -> impl std::future::Future<Output = bool> {\n    async move { qty >= 10 }\n}\n// free owner above\n";

#[test]
fn a_plain_fn_owner_returning_a_future_must_be_driven_by_the_lowest_hop() -> Result<(), String> {
    let test = "        assert_eq!(load(10).await, true);";
    let relations = |inherent_hop: &str, free_hop: &str| -> Result<_, String> {
        let inherent = predicate_finding_of(
            &source_in(
                FUTURE_INHERENT_OWNER,
                inherent_hop,
                test,
                Harness::AsyncTest,
            ),
            DIFF,
        )?;
        let free = predicate_finding_of(
            &source_in(FUTURE_FREE_OWNER, free_hop, test, Harness::AsyncTest),
            FREE_DIFF,
        )?;
        Ok((
            relation_of(&inherent, "subject").map(str::to_string),
            relation_of(&free, "subject").map(str::to_string),
        ))
    };
    // Awaited: the owner's `async move` block runs.
    let got = relations(
        "async fn load(qty: u32) -> bool {\n    Meter { limit: 10 }.over(qty).await\n}\n",
        "async fn load(qty: u32) -> bool {\n    over(qty).await\n}\n",
    )?;
    assert_both("plain fn future awaited", got, "helper_owner_call");
    // Dropped: `over` is a plain `fn`, but its changed line only runs when polled.
    let got = relations(
        "async fn load(qty: u32) -> bool {\n    let _ = Meter { limit: 10 }.over(qty);\n    true\n}\n",
        "async fn load(qty: u32) -> bool {\n    let _ = over(qty);\n    true\n}\n",
    )?;
    assert_both("plain fn future dropped", got, "same_test_file");
    Ok(())
}

#[test]
fn review_import_alias_shadow_control() -> Result<(), String> {
    let test = "        assert_eq!(load(10).await, true);";
    let other =
        "mod other {\n    pub async fn different(_qty: u32) -> bool {\n        true\n    }\n}\n";
    let hops = |entry_body: &str| {
        (
            format!(
                "{other}async fn inner(qty: u32) -> bool {{\n    Meter {{ limit: 10 }}.over(qty)\n}}\nasync fn load(qty: u32) -> bool {{\n{entry_body}\n}}\n"
            ),
            format!(
                "{other}async fn inner(qty: u32) -> bool {{\n    over(qty)\n}}\nasync fn load(qty: u32) -> bool {{\n{entry_body}\n}}\n"
            ),
        )
    };
    // `inner` is imported from elsewhere under the callee's name: the real
    // `inner` is never awaited.
    let (inherent, free) = hops("    use other::different as inner;\n    inner(qty).await");
    let got = both_owner_kinds(&inherent, &free, test, Harness::AsyncTest)?;
    assert_both("import alias shadows the callee", got, "same_test_file");
    // Controls: an import that binds a different name leaves the real call
    // credited; importing the callee's own path is refused by design (below).
    let (inherent, free) = hops(
        "    use other::different as elsewhere;\n    let _ = elsewhere(qty).await;\n    inner(qty).await",
    );
    let got = both_owner_kinds(&inherent, &free, test, Harness::AsyncTest)?;
    assert_both("import under another name", got, "helper_owner_call");
    let (inherent, free) = hops("    use crate::inner;\n    inner(qty).await");
    let got = both_owner_kinds(&inherent, &free, test, Harness::AsyncTest)?;
    // By design: import identity is not preserved, so a hop that imports a
    // name equal to its callee refuses, including the callee's own path.
    assert_both("unaliased import of the callee", got, "same_test_file");
    Ok(())
}

#[test]
fn hop_imports_that_may_bind_the_callee_refuse_by_design() -> Result<(), String> {
    let test = "        assert_eq!(load(10).await, true);";
    let hops = |entry_body: &str| {
        (
            format!(
                "async fn inner(qty: u32) -> bool {{\n    Meter {{ limit: 10 }}.over(qty)\n}}\nasync fn load(qty: u32) -> bool {{\n{entry_body}\n}}\n"
            ),
            format!(
                "async fn inner(qty: u32) -> bool {{\n    over(qty)\n}}\nasync fn load(qty: u32) -> bool {{\n{entry_body}\n}}\n"
            ),
        )
    };
    // Control: no import, credited.
    let (inherent, free) = hops("    inner(qty).await");
    let got = both_owner_kinds(&inherent, &free, test, Harness::AsyncTest)?;
    assert_both("no import", got, "helper_owner_call");
    for (case, body) in [
        (
            "unaliased external import",
            "    use ext::inner;\n    inner(qty).await",
        ),
        (
            "grouped external import",
            "    use ext::{other, inner};\n    inner(qty).await",
        ),
        (
            "glob import inside the hop",
            "    use ext::*;\n    inner(qty).await",
        ),
    ] {
        let (inherent, free) = hops(body);
        let got = both_owner_kinds(&inherent, &free, test, Harness::AsyncTest)?;
        assert_both(case, got, "same_test_file");
    }
    // A name the import does not bind leaves the call credited.
    let (inherent, free) = hops("    use ext::unrelated;\n    inner(qty).await");
    let got = both_owner_kinds(&inherent, &free, test, Harness::AsyncTest)?;
    assert_both("import of another name", got, "helper_owner_call");
    Ok(())
}

#[test]
fn only_the_hop_that_calls_the_owner_may_discard_it_in_a_two_hop_chain() -> Result<(), String> {
    // `hops[0]` is the caller of the owner (owner-upward). Here the lowest hop
    // `inner` builds the async owner's future and drops it while the entry
    // `load` awaits `inner`: the owner never runs.
    let test = "        assert_eq!(load(10).await, true);";
    let got = async_owner_relations(
        "async fn inner(qty: u32) -> bool {\n    let _ = Meter { limit: 10 }.over(qty);\n    true\n}\nasync fn load(qty: u32) -> bool {\n    inner(qty).await\n}\n",
        "async fn inner(qty: u32) -> bool {\n    let _ = over(qty);\n    true\n}\nasync fn load(qty: u32) -> bool {\n    inner(qty).await\n}\n",
        test,
    )?;
    assert_both(
        "lowest hop discards the owner future",
        got,
        "same_test_file",
    );
    // Mirror: the lowest hop awaits the owner, the entry discards `inner`.
    let got = async_owner_relations(
        "async fn inner(qty: u32) -> bool {\n    Meter { limit: 10 }.over(qty).await\n}\nasync fn load(qty: u32) -> bool {\n    let _ = inner(qty);\n    true\n}\n",
        "async fn inner(qty: u32) -> bool {\n    over(qty).await\n}\nasync fn load(qty: u32) -> bool {\n    let _ = inner(qty);\n    true\n}\n",
        test,
    )?;
    assert_both("entry hop discards the lower future", got, "same_test_file");
    // Control: both await.
    let got = async_owner_relations(
        "async fn inner(qty: u32) -> bool {\n    Meter { limit: 10 }.over(qty).await\n}\nasync fn load(qty: u32) -> bool {\n    inner(qty).await\n}\n",
        "async fn inner(qty: u32) -> bool {\n    over(qty).await\n}\nasync fn load(qty: u32) -> bool {\n    inner(qty).await\n}\n",
        test,
    )?;
    assert_both("both hops await", got, "helper_owner_call");
    Ok(())
}

#[test]
fn a_same_named_method_on_another_type_is_refused_by_the_workspace_unique_callee_rule()
-> Result<(), String> {
    // Upstream-uniqueness pin only: the duplicate `inner` makes the callee name
    // non-unique, so the chain never forms. It does not exercise the hop scan.
    let test = "        assert_eq!(load(10).await, true);";
    let other = "pub struct Other;\nimpl Other {\n    pub async fn inner(&self, _qty: u32) -> bool {\n        true\n    }\n}\n";
    let got = both_owner_kinds(
        &format!(
            "{other}async fn inner(qty: u32) -> bool {{\n    Meter {{ limit: 10 }}.over(qty)\n}}\nasync fn load(qty: u32) -> bool {{\n    Other.inner(qty).await\n}}\n"
        ),
        &format!(
            "{other}async fn inner(qty: u32) -> bool {{\n    over(qty)\n}}\nasync fn load(qty: u32) -> bool {{\n    Other.inner(qty).await\n}}\n"
        ),
        test,
        Harness::AsyncTest,
    )?;
    assert_both(
        "duplicate callee name, method awaited",
        got,
        "same_test_file",
    );
    Ok(())
}

#[test]
fn any_glob_import_in_the_hop_file_refuses_except_in_an_unrelated_test_module() -> Result<(), String>
{
    let test = "        assert_eq!(load(10).await, true);";
    let inner_in = |owner_call: &str| {
        format!(
            "mod a {{\n    pub async fn inner(qty: u32) -> bool {{\n        {owner_call}\n    }}\n}}\n"
        )
    };
    let inherent_call = "Meter { limit: 10 }.over(qty)";
    let free_call = "over(qty)";
    // E1: the callee is defined in this file, the hop sits in a module with a
    // glob import.
    let hop_with_glob = "mod b {\n    use ext::*;\n    pub async fn load(qty: u32) -> bool {\n        inner(qty).await\n    }\n}\n";
    let got = both_owner_kinds(
        &format!("{}{hop_with_glob}", inner_in(inherent_call)),
        &format!("{}{hop_with_glob}", inner_in(free_call)),
        test,
        Harness::AsyncTest,
    )?;
    assert_both("E1: glob in the hop's module", got, "same_test_file");
    // E2: a file-level glob, the callee defined only inside `mod a`, the hop at
    // the file root.
    let root_hop = "pub async fn load(qty: u32) -> bool {\n    inner(qty).await\n}\n";
    let got = both_owner_kinds(
        &format!("use ext::*;\n{}{root_hop}", inner_in(inherent_call)),
        &format!("use ext::*;\n{}{root_hop}", inner_in(free_call)),
        test,
        Harness::AsyncTest,
    )?;
    assert_both("E2: file-level glob", got, "same_test_file");
    // Control: the same shapes with no glob outside the test module are
    // credited (the harness' `mod tests { use super::*; }` is exempt).
    let got = both_owner_kinds(
        &format!("{}{root_hop}", inner_in(inherent_call)),
        &format!("{}{root_hop}", inner_in(free_call)),
        test,
        Harness::AsyncTest,
    )?;
    assert_both("no glob outside the test module", got, "helper_owner_call");
    Ok(())
}

#[test]
fn a_test_module_glob_does_not_refuse_a_hop_whose_callee_is_in_another_file() -> Result<(), String>
{
    let test = "        assert_eq!(load(10).await, true);";
    let hop = "async fn load(qty: u32) -> bool {\n    inner(qty).await\n}\nmod callee;\n";
    let callee = |owner_call: &str| {
        format!("use super::*;\npub async fn inner(qty: u32) -> bool {{\n    {owner_call}\n}}\n")
    };
    let relation = |owner: &str, diff: &str, call: &str| -> Result<Option<String>, String> {
        let finding = predicate_finding_in_files(
            &source_in(owner, hop, test, Harness::AsyncTest),
            diff,
            &[("callee.rs", &callee(call))],
        )?;
        Ok(relation_of(&finding, "subject").map(str::to_string))
    };
    let got = (
        relation(INHERENT_OWNER, DIFF, "Meter { limit: 10 }.over(qty)")?,
        relation(FREE_OWNER, FREE_DIFF, "over(qty)")?,
    );
    assert_both(
        "in-file test glob, callee in another file",
        got,
        "helper_owner_call",
    );
    Ok(())
}
