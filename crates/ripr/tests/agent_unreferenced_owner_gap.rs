#![cfg(feature = "lang-rust")]
//! #14820 production path: the real producer marks a changed owner that
//! nothing in the workspace calls, and `check --format agent` lists it as a
//! gap naming that owner. One non-test caller withholds the marker.

use ripr::{CheckInput, ExposureClass, Finding, OutputFormat, check_workspace, render_check};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

const SOURCE: &str =
    include_str!("../../../fixtures/rust_uncalled_owner_same_file_tests/input/src/lib.rs");
const MANIFEST: &str =
    include_str!("../../../fixtures/rust_uncalled_owner_same_file_tests/input/Cargo.toml");
const DIFF: &str = include_str!("../../../fixtures/rust_uncalled_owner_same_file_tests/diff.patch");
const MARKER: &str = "(unreferenced in workspace)";

struct Workspace(PathBuf);

impl Workspace {
    fn new(extra_source: &str) -> Result<Self, String> {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let root = std::env::temp_dir().join(format!(
            "ripr-agent-unreferenced-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(root.join("src")).map_err(|error| error.to_string())?;
        let workspace = Self(root);
        workspace.write("Cargo.toml", MANIFEST)?;
        workspace.write("src/lib.rs", &format!("{SOURCE}{extra_source}"))?;
        workspace.write("diff.patch", DIFF)?;
        Ok(workspace)
    }

    fn write(&self, relative: &str, text: &str) -> Result<(), String> {
        std::fs::write(Path::new(&self.0).join(relative), text).map_err(|error| error.to_string())
    }

    fn agent(&self) -> Result<(String, Vec<Finding>), String> {
        let output = check_workspace(CheckInput {
            root: self.0.clone(),
            diff_file: Some(self.0.join("diff.patch")),
            ..CheckInput::default()
        })?;
        let text = render_check(&output, &OutputFormat::Agent)?;
        Ok((text, output.findings))
    }
}

impl Drop for Workspace {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn producer_marked_unreferenced_owner_reaches_the_agent_gap_row() -> Result<(), String> {
    let (text, findings) = Workspace::new("")?.agent()?;
    assert!(!findings.is_empty(), "{text}");
    assert!(
        findings
            .iter()
            .all(|finding| finding.class == ExposureClass::NoStaticPath
                && finding.ripr.reach.summary.ends_with(MARKER)
                && finding.is_unreferenced_no_static_path()),
        "{findings:#?}"
    );
    assert!(
        text.contains(&format!(" gaps={} ", findings.len())),
        "{text}"
    );
    assert!(
        text.contains(
            "missing=no test calls `untested_rounding`; assert its exact result in a new test|"
        ),
        "{text}"
    );
    assert!(!text.contains("\nstop="), "{text}");
    Ok(())
}

#[test]
fn a_non_test_caller_withholds_the_uncalled_owner_gap() -> Result<(), String> {
    let caller = "\npub fn rounded_total(cents: u32) -> u32 {\n    untested_rounding(cents)\n}\n";
    let (text, findings) = Workspace::new(caller)?.agent()?;
    assert!(!findings.is_empty(), "{text}");
    assert!(
        findings
            .iter()
            .all(|finding| !finding.is_unreferenced_no_static_path()
                && !finding.ripr.reach.summary.ends_with(MARKER)),
        "{findings:#?}"
    );
    // The caller sits beside the same-file tests, so ripr reads it as a weak
    // path; whatever the class, the uncalled-owner hint is withheld.
    assert!(!text.contains("no test calls"), "{text}");
    Ok(())
}
