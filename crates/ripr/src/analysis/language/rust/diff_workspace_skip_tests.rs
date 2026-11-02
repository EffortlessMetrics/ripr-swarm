//! #8547: a diff that cannot index Rust must not walk the workspace.
//!
//! Instant/Draft/Fast with no analyzable changed Rust file and no open
//! index documents return the empty complete result. Deep/Ready still
//! select every file. A real Rust edit still walks and produces findings.
use super::{RustAdapter, diff_needs_rust_workspace};
use crate::analysis::language::LanguageAdapter;
use crate::analysis::{AnalysisMode, AnalysisOptions, diff};
use crate::config::OraclePolicy;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

struct Fixture(PathBuf);

impl Fixture {
    fn new(label: &str) -> Result<Self, String> {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| error.to_string())?
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "ripr-rust-skip-workspace-{label}-{stamp}-{}",
            std::process::id()
        ));
        fs::create_dir_all(root.join("src")).map_err(|error| error.to_string())?;
        Ok(Self(root))
    }

    fn write(&self, relative: &str, text: &str) -> Result<(), String> {
        let path = self.0.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        fs::write(path, text).map_err(|error| error.to_string())
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn options(root: &Path, mode: AnalysisMode) -> AnalysisOptions {
    AnalysisOptions {
        root: root.to_path_buf(),
        base: None,
        diff_file: None,
        mode,
        resolved_subject_identity: None,
        open_rust_index_paths: Default::default(),
        include_unchanged_tests: true,
        resolve_tsconfig_paths: false,
        perl_facts_path: None,
        perl_producer_failure: None,
        git_timeout: None,
        git_candidate: None,
        production_like_targets: Default::default(),
        test_harnesses: Vec::new(),
    }
}

fn analyze(
    root: &Path,
    mode: AnalysisMode,
    files: &[crate::analysis::diff::ChangedFile],
) -> Result<crate::analysis::language::LanguageDiffResult, String> {
    RustAdapter.analyze_diff(&options(root, mode), &OraclePolicy::default(), files)
}

fn docs_diff() -> Vec<crate::analysis::diff::ChangedFile> {
    diff::parse_unified_diff(
        "diff --git a/README.md b/README.md\n\
         --- a/README.md\n\
         +++ b/README.md\n\
         @@ -1,1 +1,1 @@\n\
         -hello\n\
         +hello world\n",
    )
}

fn rust_diff() -> Vec<crate::analysis::diff::ChangedFile> {
    diff::parse_unified_diff(
        "diff --git a/src/lib.rs b/src/lib.rs\n\
         --- a/src/lib.rs\n\
         +++ b/src/lib.rs\n\
         @@ -1,3 +1,3 @@\n \
         pub fn discount(total: u32) -> u32 {\n\
         -    if total >= 100 { total / 10 } else { 0 }\n\
         +    if total > 100 { total / 10 } else { 0 }\n \
         }\n",
    )
}

fn generated_diff() -> Vec<crate::analysis::diff::ChangedFile> {
    diff::parse_unified_diff(
        "diff --git a/src/generated.rs b/src/generated.rs\n\
         --- a/src/generated.rs\n\
         +++ b/src/generated.rs\n\
         @@ -1,1 +1,1 @@\n\
         -pub fn value() -> i32 { 1 }\n\
         +pub fn value() -> i32 { 2 }\n",
    )
}

fn crate_fixture(label: &str) -> Result<Fixture, String> {
    let fixture = Fixture::new(label)?;
    fixture.write(
        "Cargo.toml",
        "[package]\nname = \"skip_workspace\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )?;
    fixture.write(
        "src/lib.rs",
        "pub fn discount(total: u32) -> u32 {\n    if total > 100 { total / 10 } else { 0 }\n}\n",
    )?;
    fixture.write("src/generated.rs", "pub fn value() -> i32 { 2 }\n")?;
    fixture.write("README.md", "hello world\n")?;
    // Noise the skip must not walk: enough sibling files that a missed skip
    // is a real discovery call, not an empty-tree coincidence.
    for index in 0..256 {
        fixture.write(
            &format!("src/extra_{index}.rs"),
            "pub fn extra() -> u32 { 1 }\n",
        )?;
    }
    Ok(fixture)
}

#[test]
fn helper_skips_narrow_modes_without_rust_or_open_documents() {
    let root = PathBuf::from("/tmp/ripr-skip-helper");
    let mut empty = options(&root, AnalysisMode::Instant);
    assert!(!diff_needs_rust_workspace(&empty, &[]));
    empty.mode = AnalysisMode::Draft;
    assert!(!diff_needs_rust_workspace(&empty, &[]));
    empty.mode = AnalysisMode::Fast;
    assert!(!diff_needs_rust_workspace(&empty, &[]));
    empty.mode = AnalysisMode::Deep;
    assert!(diff_needs_rust_workspace(&empty, &[]));
    empty.mode = AnalysisMode::Ready;
    assert!(diff_needs_rust_workspace(&empty, &[]));
    empty.mode = AnalysisMode::Instant;
    empty
        .open_rust_index_paths
        .insert(PathBuf::from("src/lib.rs"));
    assert!(diff_needs_rust_workspace(&empty, &[]));
    assert!(diff_needs_rust_workspace(
        &options(&root, AnalysisMode::Instant),
        &[PathBuf::from("src/lib.rs")]
    ));
}

#[test]
fn docs_only_instant_and_draft_skip_the_workspace() -> Result<(), String> {
    let fixture = crate_fixture("docs-only")?;
    let docs = docs_diff();
    for mode in [
        AnalysisMode::Instant,
        AnalysisMode::Draft,
        AnalysisMode::Fast,
    ] {
        let (result, walks) = crate::analysis::workspace::discover_probe::observe(|| {
            analyze(&fixture.0, mode, &docs)
        });
        let result = result?;
        assert_eq!(
            walks, 0,
            "{mode:?} docs-only diff must not walk the Rust workspace"
        );
        assert!(
            result.findings.is_empty()
                && result.limitations.is_empty()
                && result.changed_files == 0
                && result.skipped_files == 0
                && result.candidate_line_count == 0,
            "{mode:?} docs-only result must be the empty complete result, got {:?}",
            (
                result.findings.len(),
                result.limitations.len(),
                result.changed_files,
                result.skipped_files,
                result.candidate_line_count
            )
        );
    }
    Ok(())
}

#[test]
fn generated_only_instant_skips_walk_and_still_counts_the_skip() -> Result<(), String> {
    let fixture = crate_fixture("generated-only")?;
    let generated = generated_diff();
    let (result, walks) = crate::analysis::workspace::discover_probe::observe(|| {
        analyze(&fixture.0, AnalysisMode::Instant, &generated)
    });
    let result = result?;
    assert_eq!(
        walks, 0,
        "generated-only Instant must not walk the workspace"
    );
    assert_eq!(
        result.skipped_files, 1,
        "the changed generated file must still be counted as skipped"
    );
    assert!(
        result.findings.is_empty() && result.changed_files == 0,
        "generated-only Instant must not invent findings"
    );
    Ok(())
}

#[test]
fn rust_edit_and_deep_docs_still_walk() -> Result<(), String> {
    let fixture = crate_fixture("must-walk")?;
    let (rust_result, rust_walks) = crate::analysis::workspace::discover_probe::observe(|| {
        analyze(&fixture.0, AnalysisMode::Instant, &rust_diff())
    });
    let rust_result = rust_result?;
    assert!(
        rust_walks >= 1,
        "a changed Rust file must still discover the workspace"
    );
    assert!(
        !rust_result.findings.is_empty(),
        "a changed predicate must still produce a finding, got {:?}",
        rust_result.findings
    );
    let (deep_result, deep_walks) = crate::analysis::workspace::discover_probe::observe(|| {
        analyze(&fixture.0, AnalysisMode::Deep, &docs_diff())
    });
    deep_result?;
    assert!(
        deep_walks >= 1,
        "Deep docs-only must still select the whole workspace"
    );
    let mut open = options(&fixture.0, AnalysisMode::Instant);
    open.open_rust_index_paths
        .insert(PathBuf::from("src/lib.rs"));
    let (open_result, open_walks) = crate::analysis::workspace::discover_probe::observe(|| {
        RustAdapter.analyze_diff(&open, &OraclePolicy::default(), &docs_diff())
    });
    open_result?;
    assert!(
        open_walks >= 1,
        "an open Rust document must keep the workspace walk"
    );
    Ok(())
}
