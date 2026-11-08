//! Public-API controls for a future created but never polled (RIPR-SPEC-0240,
//! #13702).
//!
//! A synchronous `#[test]` that calls a changed `async fn` and drops the
//! future never runs the owner's body. `check` names that as
//! `rust_future_never_polled` (reach `unknown`, class `static_unknown`)
//! instead of claiming reach. Each control below changes one thing about the
//! shape and keeps today's reading.

use ripr::domain::StageState;
use ripr::{CheckInput, ExposureClass, Finding, Mode, OutputFormat, check_workspace};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

const LIB_BEFORE: &str = "pub async fn fetch_len(s: &str) -> usize {\n    s.len()\n}\n";
const LIB_AFTER: &str = "pub async fn fetch_len(s: &str) -> usize {\n    s.len() + 1\n}\n";
const DIFF: &str = "diff --git a/src/lib.rs b/src/lib.rs
index 1111111..2222222 100644
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -1,3 +1,3 @@
 pub async fn fetch_len(s: &str) -> usize {
-    s.len()
+    s.len() + 1
 }
";
const LIMIT: &str = "rust_future_never_polled";

struct TempRepo {
    root: PathBuf,
}

impl TempRepo {
    fn create(test_source: &str, extra_lib: &str) -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "ripr-unpolled-future-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(root.join("src"))
            .map_err(|error| format!("create source directory failed: {error}"))?;
        std::fs::create_dir_all(root.join("tests"))
            .map_err(|error| format!("create tests directory failed: {error}"))?;
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"fx\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[workspace]\n",
        )
        .map_err(|error| format!("write Cargo.toml failed: {error}"))?;
        std::fs::write(root.join("src/lib.rs"), format!("{LIB_AFTER}{extra_lib}"))
            .map_err(|error| format!("write source failed: {error}"))?;
        std::fs::write(root.join("tests/t.rs"), test_source)
            .map_err(|error| format!("write test failed: {error}"))?;
        std::fs::write(root.join("diff.patch"), DIFF)
            .map_err(|error| format!("write diff failed: {error}"))?;
        Ok(Self { root })
    }

    fn finding(&self) -> Result<Finding, String> {
        let output = check_workspace(CheckInput {
            root: self.root.clone(),
            base: None,
            diff_file: Some(self.root.join("diff.patch")),
            mode: Mode::Ready,
            format: OutputFormat::Json,
            include_unchanged_tests: true,
            ..CheckInput::default()
        })?;
        let mut on_line = output
            .findings
            .into_iter()
            .filter(|finding| finding.probe.location.line == 2);
        match (on_line.next(), on_line.next()) {
            (Some(finding), None) => Ok(finding),
            _ => Err("premise: exactly one finding on the changed line".to_string()),
        }
    }
}

impl Drop for TempRepo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn read(test_source: &str) -> Result<Finding, String> {
    TempRepo::create(test_source, "")?.finding()
}

fn limited(finding: &Finding) -> bool {
    finding.static_limit_kind.map(|kind| kind.as_str()) == Some(LIMIT)
}

fn premise_lib_is_the_changed_async_owner() {
    assert_ne!(LIB_BEFORE, LIB_AFTER);
    assert!(LIB_AFTER.starts_with("pub async fn fetch_len"));
}

#[test]
fn a_dropped_unpolled_future_is_a_named_limit_not_reach() -> Result<(), String> {
    premise_lib_is_the_changed_async_owner();
    let finding = read(
        "use fx::fetch_len;\n\n#[test]\nfn fetch_len_builds_future() {\n    let _f = fetch_len(\"abc\");\n}\n",
    )?;
    assert!(
        finding
            .related_tests
            .iter()
            .any(|test| test.name == "fetch_len_builds_future"),
        "premise: the unpolled test is related"
    );
    assert!(limited(&finding), "{:?}", finding.static_limit_kind);
    assert_eq!(finding.class, ExposureClass::StaticUnknown);
    assert_eq!(finding.ripr.reach.state, StageState::Unknown);
    assert!(
        finding
            .ripr
            .reach
            .summary
            .starts_with("future created but never polled"),
        "{}",
        finding.ripr.reach.summary
    );
    assert_ne!(finding.ripr.infect.state, StageState::Yes);
    assert!(
        !finding
            .evidence
            .iter()
            .any(|line| line.starts_with("Related tests appear to reach")),
        "stale reach evidence: {:?}",
        finding.evidence
    );
    Ok(())
}

#[test]
fn other_dropped_shapes_are_named_too() -> Result<(), String> {
    for body in [
        "    let _ = fetch_len(\"abc\");\n",
        "    fetch_len(\"abc\");\n",
        "    drop(fetch_len(\"abc\"));\n",
    ] {
        let finding = read(&format!(
            "use fx::fetch_len;\n\n#[test]\nfn builds() {{\n{body}}}\n"
        ))?;
        assert!(limited(&finding), "{body}: {:?}", finding.class);
    }
    Ok(())
}

/// The controls keep today's reading: no limit of this kind, and reach is not
/// rewritten to `unknown` by it.
fn assert_unchanged(test_source: &str, extra_lib: &str) -> Result<(), String> {
    let finding = TempRepo::create(test_source, extra_lib)?.finding()?;
    assert!(
        !limited(&finding),
        "{test_source}\n{extra_lib}: {:?}",
        finding.static_limit_kind
    );
    assert!(
        !finding
            .ripr
            .reach
            .summary
            .starts_with("future created but never polled"),
        "{}",
        finding.ripr.reach.summary
    );
    Ok(())
}

#[test]
fn an_awaited_call_in_a_tokio_test_keeps_its_reading() -> Result<(), String> {
    assert_unchanged(
        "use fx::fetch_len;\n\n#[tokio::test]\nasync fn fetch_len_awaited() {\n    let _n = fetch_len(\"abc\").await;\n}\n",
        "",
    )
}

#[test]
fn a_block_on_call_keeps_its_reading() -> Result<(), String> {
    assert_unchanged(
        "use fx::fetch_len;\n\n#[test]\nfn fetch_len_blocked() {\n    let _n = futures::executor::block_on(fetch_len(\"abc\"));\n}\n",
        "",
    )
}

#[test]
fn shapes_that_may_poll_the_future_keep_their_reading() -> Result<(), String> {
    for test_source in [
        // an async test attribute, even without `.await`
        "use fx::fetch_len;\n\n#[tokio::test]\nasync fn builds() {\n    let _f = fetch_len(\"abc\");\n}\n",
        // the binding is used again
        "use fx::fetch_len;\n\n#[test]\nfn builds() {\n    let f = fetch_len(\"abc\");\n    let _n = futures::executor::block_on(f);\n}\n",
        // passed to a helper that may poll it
        "use fx::fetch_len;\n\nfn drive<F>(_f: F) {}\n\n#[test]\nfn builds() {\n    drive(fetch_len(\"abc\"));\n}\n",
        // a second test drops the future, but the first awaits it
        "use fx::fetch_len;\n\n#[test]\nfn builds() {\n    let _f = fetch_len(\"abc\");\n}\n\n#[tokio::test]\nasync fn awaits() {\n    assert_eq!(fetch_len(\"abc\").await, 4);\n}\n",
    ] {
        assert_unchanged(test_source, "")?;
    }
    // another function names the owner, so it may be a polling path
    assert_unchanged(
        "use fx::fetch_len;\n\n#[test]\nfn builds() {\n    let _f = fetch_len(\"abc\");\n}\n",
        "\npub fn fetch_len_now(s: &str) -> usize {\n    futures::executor::block_on(fetch_len(s))\n}\n",
    )
}
