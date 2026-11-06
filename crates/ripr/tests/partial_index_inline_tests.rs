//! Partial-index reach must account for tests in ordinary Rust filenames.
use ripr::domain::{RelationReason, StageState, StaticLimitKind};
use ripr::{CheckInput, ExposureClass, Finding, Mode, OutputFormat, check_workspace};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

struct Fixture(PathBuf);
impl Fixture {
    fn new(files: &[(&str, &str)], diff: &str) -> Result<Self, String> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let parent = std::env::temp_dir();
        std::fs::create_dir_all(&parent).map_err(|e| e.to_string())?;
        let root = parent.join(format!(
            "ripr-partial-inline-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&root).map_err(|e| e.to_string())?;
        let fixture = Self(root);
        fixture.write(
            "Cargo.toml",
            "[package]\nname='example'\nversion='0.1.0'\nedition='2021'\n[workspace]\n",
        )?;
        for (path, content) in files {
            fixture.write(path, content)?;
        }
        fixture.write("change.diff", diff)?;
        Ok(fixture)
    }
    fn write(&self, path: &str, content: &str) -> Result<(), String> {
        let path = self.0.join(path);
        std::fs::create_dir_all(path.parent().ok_or("missing parent")?)
            .map_err(|e| e.to_string())?;
        std::fs::write(path, content).map_err(|e| e.to_string())
    }
    fn finding(&self, mode: Mode, include_unchanged_tests: bool) -> Result<Finding, String> {
        let output = check_workspace(CheckInput {
            root: self.0.clone(),
            diff_file: Some(self.0.join("change.diff")),
            mode,
            include_unchanged_tests,
            format: OutputFormat::Json,
            ..CheckInput::default()
        })?;
        if output.findings.len() != 1 {
            return Err(format!("expected one finding, got {:?}", output.findings));
        }
        Ok(output.findings.into_iter().next().unwrap())
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

const PRICING: &str =
    "pub fn discount(price: u32) -> u32 {\n    if price >= 100 { price - 10 } else { price }\n}\n";
const DIFF: &str = "diff --git a/src/pricing.rs b/src/pricing.rs\n--- a/src/pricing.rs\n+++ b/src/pricing.rs\n@@ -2,1 +2,1 @@\n-    if price > 100 { price - 10 } else { price }\n+    if price >= 100 { price - 10 } else { price }\n";
const TEST: &str = "#[cfg(test)] mod tests {\n #[test] fn discount_boundary() { assert_eq!(crate::pricing::discount(100), 90); }\n}\n";
const SIBLING: &str =
    "#[cfg(test)] mod tests {\n #[test] fn sibling_unrelated() { assert_eq!(2 + 2, 4); }\n}\n";

#[test]
fn owner_named_suggestion_does_not_hide_partial_reach_unknown() -> Result<(), String> {
    let pricing = format!(
        "{PRICING}{}",
        SIBLING.replace("sibling_unrelated", "discount_unrelated")
    );
    let fixture = Fixture::new(
        &[
            ("src/lib.rs", "pub mod pricing;\nmod checkout;\n"),
            ("src/pricing.rs", &pricing),
            ("src/checkout.rs", TEST),
        ],
        DIFF,
    )?;
    for (mode, recall) in [(Mode::Instant, true), (Mode::Draft, false)] {
        let finding = fixture.finding(mode, recall)?;
        assert_eq!(finding.class, ExposureClass::StaticUnknown, "{finding:?}");
        assert_eq!(finding.ripr.reach.state, StageState::Unknown);
        assert_eq!(finding.ripr.infect.state, StageState::Unknown);
        assert!(finding.ripr.infect.summary.contains("unindexed files"));
        assert!(
            !finding
                .evidence
                .iter()
                .any(|e| e.starts_with("No test reaches the changed owner"))
        );
        assert!(
            finding
                .related_tests
                .iter()
                .any(|test| test.name.ends_with("discount_unrelated")
                    && test.relation_reason == Some(RelationReason::OwnerNamedTest))
        );
    }
    for mode in [Mode::Draft, Mode::Ready] {
        let finding = fixture.finding(mode, true)?;
        assert_eq!(finding.class, ExposureClass::Exposed, "{finding:?}");
        assert!(
            finding
                .related_tests
                .iter()
                .any(|test| test.name.ends_with("discount_boundary")
                    && test.relation_reason == Some(RelationReason::DirectOwnerCall))
        );
    }
    Ok(())
}

#[test]
fn unrelated_same_file_test_does_not_hide_partial_reach_unknown() -> Result<(), String> {
    let pricing = format!("{PRICING}{SIBLING}");
    let fixture = Fixture::new(
        &[
            ("src/lib.rs", "pub mod pricing;\nmod checkout;\n"),
            ("src/pricing.rs", &pricing),
            ("src/checkout.rs", TEST),
        ],
        DIFF,
    )?;
    for (mode, recall) in [(Mode::Instant, true), (Mode::Draft, false)] {
        let finding = fixture.finding(mode, recall)?;
        assert_eq!(finding.class, ExposureClass::StaticUnknown, "{finding:?}");
        assert_eq!(finding.ripr.reach.state, StageState::Unknown);
        assert_eq!(finding.ripr.infect.state, StageState::Unknown);
        assert_eq!(finding.ripr.reveal.observe.state, StageState::Unknown);
        assert_eq!(finding.ripr.reveal.discriminate.state, StageState::Unknown);
        assert!(finding.ripr.infect.summary.contains("unindexed files"));
        assert!(
            finding
                .related_tests
                .iter()
                .any(|test| test.name.ends_with("sibling_unrelated"))
        );
        assert!(
            !finding
                .evidence
                .iter()
                .any(|e| e.starts_with("No test reaches the changed owner"))
        );
    }
    for mode in [Mode::Draft, Mode::Ready] {
        let finding = fixture.finding(mode, true)?;
        assert_eq!(finding.class, ExposureClass::Exposed, "{finding:?}");
        assert!(
            finding
                .related_tests
                .iter()
                .any(|test| test.name.ends_with("discount_boundary"))
        );
    }
    Ok(())
}

#[test]
fn partial_index_preserves_direct_indexed_test_reach() -> Result<(), String> {
    let pricing = format!(
        "{PRICING}#[cfg(test)] mod tests {{ #[test] fn direct_boundary() {{ assert_eq!(super::discount(100), 90); }} }}\n"
    );
    let fixture = Fixture::new(
        &[
            ("src/lib.rs", "pub mod pricing;\nmod checkout;\n"),
            ("src/pricing.rs", &pricing),
            ("src/checkout.rs", TEST),
        ],
        DIFF,
    )?;
    for (mode, recall) in [(Mode::Instant, true), (Mode::Draft, false)] {
        let finding = fixture.finding(mode, recall)?;
        assert_eq!(finding.class, ExposureClass::Exposed, "{finding:?}");
        assert_eq!(finding.ripr.reach.state, StageState::Yes);
        assert!(
            finding
                .related_tests
                .iter()
                .any(|test| test.name.ends_with("direct_boundary"))
        );
        assert!(finding.static_limit_kind.is_none());
    }
    Ok(())
}

#[test]
fn ordinary_inline_test_requires_abstention_only_for_partial_selection() -> Result<(), String> {
    let fixture = Fixture::new(
        &[
            ("src/lib.rs", "pub mod pricing;\nmod checkout;\n"),
            ("src/pricing.rs", PRICING),
            ("src/checkout.rs", TEST),
        ],
        DIFF,
    )?;
    for (mode, recall) in [(Mode::Instant, true), (Mode::Draft, false)] {
        let finding = fixture.finding(mode, recall)?;
        assert_eq!(finding.class, ExposureClass::StaticUnknown, "{finding:?}");
        assert_eq!(finding.ripr.reach.state, StageState::Unknown);
        assert!(
            finding
                .evidence
                .iter()
                .any(|e| e.starts_with("partial_index_reach:"))
        );
        assert!(
            finding
                .missing
                .iter()
                .any(|e| e.contains("may contain tests"))
        );
    }
    for mode in [Mode::Draft, Mode::Ready] {
        let finding = fixture.finding(mode, true)?;
        assert_eq!(finding.class, ExposureClass::Exposed, "{finding:?}");
        assert!(
            finding
                .related_tests
                .iter()
                .any(|t| t.name.ends_with("discount_boundary"))
        );
        assert!(finding.static_limit_kind.is_none());
    }
    Ok(())
}

#[test]
fn partial_scope_is_conservative_but_complete_index_keeps_no_path() -> Result<(), String> {
    let fixture = Fixture::new(
        &[
            ("src/pricing.rs", PRICING),
            ("src/helper.rs", "pub fn helper() {}\n"),
        ],
        DIFF,
    )?;
    let partial = fixture.finding(Mode::Instant, true)?;
    assert_eq!(partial.class, ExposureClass::StaticUnknown, "{partial:?}");
    let complete = fixture.finding(Mode::Ready, true)?;
    assert_eq!(complete.class, ExposureClass::NoStaticPath, "{complete:?}");
    assert!(complete.static_limit_kind.is_none());
    // Even instant can have a complete index when the changed file is the
    // only analyzable Rust source; do not abstain just because of the mode.
    let single_diff = DIFF.replace("src/pricing.rs", "src/lib.rs");
    let single = Fixture::new(&[("src/lib.rs", PRICING)], &single_diff)?;
    let complete_instant = single.finding(Mode::Instant, true)?;
    assert_eq!(
        complete_instant.class,
        ExposureClass::NoStaticPath,
        "{complete_instant:?}"
    );
    assert!(complete_instant.static_limit_kind.is_none());
    Ok(())
}

#[test]
fn partial_scope_preserves_named_transitive_witness() -> Result<(), String> {
    let fixture = Fixture::new(
        &[
            (
                "src/lib.rs",
                "mod internal;\npub fn outer(a: i32, b: i32) -> i32 { internal::inner(a, b) }\npub fn inner(a: i32, b: i32) -> i32 { a + b }\n#[cfg(test)] mod tests { #[test] fn outer_difference() { assert_eq!(crate::outer(10, 3), 7); } }\n",
            ),
            (
                "src/internal.rs",
                "pub(crate) fn inner(a: i32, b: i32) -> i32 {\n    if a > b {\n        a - b\n    } else {\n        b\n    }\n}\n",
            ),
            ("tests/it.rs", "#[test] fn omitted() {}\n"),
        ],
        "diff --git a/src/internal.rs b/src/internal.rs\n--- a/src/internal.rs\n+++ b/src/internal.rs\n@@ -2,1 +2,1 @@\n-    if a >= b {\n+    if a > b {\n",
    )?;
    let finding = fixture.finding(Mode::Instant, true)?;
    assert_eq!(finding.class, ExposureClass::NoStaticPath, "{finding:?}");
    assert_eq!(
        finding.static_limit_kind,
        Some(StaticLimitKind::RustTransitiveReachUnresolved)
    );
    assert!(
        finding
            .evidence
            .iter()
            .any(|e| e.contains("outer_difference"))
    );
    assert!(
        !finding
            .evidence
            .iter()
            .any(|e| e.starts_with("partial_index_reach:"))
    );
    Ok(())
}

#[test]
fn partial_scope_preserves_named_macro_limit() -> Result<(), String> {
    let fixture = Fixture::new(
        &[
            (
                "src/pricing.rs",
                "pub fn discount(price: u32) -> u32 {\n    if price >= 100 { price - 10 } else { price }\n}\nmacro_rules! proptest { ($($args:tt)*) => {} }\nproptest! { #[test] fn boundary() { assert_eq!(discount(100), 90); } }\n",
            ),
            ("tests/it.rs", "#[test] fn omitted() {}\n"),
        ],
        DIFF,
    )?;
    let finding = fixture.finding(Mode::Instant, true)?;
    assert_eq!(finding.class, ExposureClass::StaticUnknown, "{finding:?}");
    assert_eq!(
        finding.static_limit_kind,
        Some(StaticLimitKind::RustMacroReachUnresolved)
    );
    assert!(
        !finding
            .evidence
            .iter()
            .any(|e| e.starts_with("partial_index_reach:"))
    );
    Ok(())
}
