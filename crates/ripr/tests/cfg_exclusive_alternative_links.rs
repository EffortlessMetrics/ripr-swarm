//! Public-API controls for same-named functions in `#[cfg]`-exclusive
//! alternative module files (#8999).
//!
//! `#[cfg(P)] #[path = "a.rs"] mod imp;` beside `#[cfg(not(P))] #[path =
//! "b.rs"] mod imp;` compiles one of two files as one module. A test in one
//! alternative that calls its own file's `wait` through `super::wait` never
//! reaches the other alternative's `wait`, so a change to the other file is
//! not related to it through a direct call (and so neither reaches nor
//! exposes it). The owner's own alternative keeps its direct-call credit.

use ripr::{CheckInput, Finding, Mode, OutputFormat, ProbeFamily, check_workspace};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

const ROOT: &str = "#[cfg(feature = \"parking_lot\")]\n#[path = \"imp_pl.rs\"]\nmod imp;\n#[cfg(not(feature = \"parking_lot\"))]\n#[path = \"imp_std.rs\"]\nmod imp;\n";

const PL: &str = "pub(crate) fn wait(x: u32) -> u32 {\n    x + 2\n}\n\n#[cfg(test)]\nmod tests {\n    #[test]\n    fn pl_wait() {\n        assert_eq!(super::wait(1), 3);\n    }\n}\n";

const STD_CHANGED: &str = "pub(crate) fn wait(x: u32) -> u32 {\n    x + 1\n}\n";

const STD_TEST: &str = "\n#[cfg(test)]\nmod tests {\n    #[test]\n    fn std_wait() {\n        assert_eq!(super::wait(1), 2);\n    }\n}\n";

const DIFF: &str = "diff --git a/src/imp_std.rs b/src/imp_std.rs
index 1111111..2222222 100644
--- a/src/imp_std.rs
+++ b/src/imp_std.rs
@@ -1,3 +1,3 @@
 pub(crate) fn wait(x: u32) -> u32 {
-    x + 2
+    x + 1
 }
";

struct TempRepo {
    root: PathBuf,
}

impl TempRepo {
    fn create(root_source: &str, std_source: &str) -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "ripr-cfg-exclusive-links-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        let this = Self { root };
        std::fs::create_dir_all(this.root.join("src"))
            .map_err(|error| format!("create source directory failed: {error}"))?;
        for (relative, source) in [
            (
                "Cargo.toml",
                "[package]\nname = \"cfg-exclusive-links\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[features]\nparking_lot = []\n\n[workspace]\n",
            ),
            ("src/lib.rs", root_source),
            ("src/imp_pl.rs", PL),
            ("src/imp_std.rs", std_source),
            ("diff.patch", DIFF),
        ] {
            std::fs::write(this.root.join(relative), source)
                .map_err(|error| format!("write {relative} failed: {error}"))?;
        }
        Ok(this)
    }

    /// The one finding for the changed `x + 1` return in `imp_std.rs`.
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
        let mut findings = output
            .findings
            .into_iter()
            .filter(|finding| finding.probe.family == ProbeFamily::ReturnValue);
        match (findings.next(), findings.next()) {
            (Some(finding), None) => Ok(finding),
            (found, _) => Err(format!(
                "premise: exactly one return-value finding, got {:?}",
                found.map(|finding| finding.id)
            )),
        }
    }
}

impl Drop for TempRepo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn related(finding: &Finding, name: &str) -> Option<String> {
    finding
        .related_tests
        .iter()
        .find(|test| test.name == name)
        .map(|test| format!("{:?}", test.relation_reason))
}

#[test]
fn the_other_alternatives_own_call_is_not_a_direct_call_to_this_one() -> Result<(), String> {
    let finding = TempRepo::create(ROOT, STD_CHANGED)?.finding()?;
    assert_eq!(
        related(&finding, "pl_wait"),
        None,
        "pl_wait calls imp_pl's wait, not the changed imp_std wait: {:?}",
        finding.related_tests
    );
    Ok(())
}

#[test]
fn the_changed_alternatives_own_test_keeps_its_direct_call() -> Result<(), String> {
    let source = format!("{STD_CHANGED}{STD_TEST}");
    let finding = TempRepo::create(ROOT, &source)?.finding()?;
    assert_eq!(
        related(&finding, "std_wait").as_deref(),
        Some("Some(DirectOwnerCall)"),
        "{:?}",
        finding.related_tests
    );
    assert_eq!(related(&finding, "pl_wait"), None);
    Ok(())
}

#[test]
fn a_non_complementary_pair_keeps_the_prior_link() -> Result<(), String> {
    let root = ROOT.replace("not(feature = \"parking_lot\")", "feature = \"std\"");
    let finding = TempRepo::create(&root, STD_CHANGED)?.finding()?;
    assert_eq!(
        related(&finding, "pl_wait").as_deref(),
        Some("Some(DirectOwnerCall)"),
        "both files can compile together, so the link must stay: {:?}",
        finding.related_tests
    );
    Ok(())
}
