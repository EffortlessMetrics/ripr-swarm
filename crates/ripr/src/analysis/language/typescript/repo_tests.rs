//! Tests for TypeScript / JavaScript repo-mode analysis (#5545, #6392, #6395).

use super::tests::{ts_unique_tempdir, ts_write_file};
use super::*;
use crate::domain::LanguageId as DomainLanguage;

fn repo_options(root: &Path) -> AnalysisOptions {
    AnalysisOptions {
        root: root.to_path_buf(),
        base: None,
        diff_file: None,
        mode: crate::analysis::AnalysisMode::Deep,
        include_unchanged_tests: false,
        resolve_tsconfig_paths: false,
        perl_facts_path: None,
        perl_producer_failure: None,
        git_timeout: None,
        git_candidate: None,
        production_like_targets: Default::default(),
        test_harnesses: Vec::new(),
        resolved_subject_identity: None,
        open_rust_index_paths: Default::default(),
    }
}

const TS_SRC: &str = "export function classify(x: number): number {\n  if (x > 0) {\n    return 1;\n  }\n  return 0;\n}\n";
const JS_SRC: &str =
    "export function clamp(x) {\n  if (x > 10) {\n    return 10;\n  }\n  return x;\n}\n";
const TS_TEST: &str = "import { classify } from \"../src/lib\";\n\ndescribe(\"classify\", () => {\n  it(\"is positive\", () => {\n    expect(classify(5)).toBe(1);\n  });\n});\n";

fn run(root: &Path, limit: usize) -> Result<LanguageRepoResult, String> {
    repo::analyze_repo_with_limit(
        &repo_options(root),
        limit,
        ts_file_read_limit(),
        ts_workspace_read_budget(),
    )
}

#[test]
fn limit_env_contract_is_positive_integer_or_named_error() {
    assert_eq!(
        repo::resolve_repo_working_set_limit(Err(std::env::VarError::NotPresent)),
        Ok(repo::TS_REPO_FILE_LIMIT)
    );
    assert_eq!(
        repo::resolve_repo_working_set_limit(Ok(" 7 ".to_string())),
        Ok(7)
    );
    assert!(
        repo::resolve_repo_working_set_limit(Ok("0".to_string()))
            .err()
            .is_some_and(|err| err.contains("positive integer"))
    );
    assert!(
        repo::resolve_repo_working_set_limit(Ok("abc".to_string()))
            .err()
            .is_some_and(|err| err.contains("RIPR_MAX_REPO_INDEX_FILES"))
    );
}

#[test]
fn pure_typescript_repo_emits_nonzero_findings_and_is_complete() -> Result<(), String> {
    let root = ts_unique_tempdir("repo-pure-ts")?;
    ts_write_file(&root.join("src/lib.ts"), TS_SRC)?;
    ts_write_file(&root.join("test/lib.test.ts"), TS_TEST)?;
    let result = run(&root, 800);
    let _ = std::fs::remove_dir_all(&root);
    let result = result?;
    assert_eq!(result.production_files, 1);
    assert!(!result.findings.is_empty(), "positive production path");
    assert!(
        result
            .findings
            .iter()
            .all(|f| f.language == Some(DomainLanguage::TypeScript)),
        "TypeScript identity kept"
    );
    assert!(
        result
            .findings
            .iter()
            .all(|f| f.probe.location.file == Path::new("src/lib.ts")),
        "test files never seed production findings"
    );
    assert_eq!(result.partial_reason, None, "complete bounded run");
    Ok(())
}

#[test]
fn pure_javascript_repo_keeps_javascript_identity() -> Result<(), String> {
    let root = ts_unique_tempdir("repo-pure-js")?;
    ts_write_file(&root.join("src/clamp.js"), JS_SRC)?;
    let result = run(&root, 800);
    let _ = std::fs::remove_dir_all(&root);
    let result = result?;
    assert!(!result.findings.is_empty());
    assert!(
        result
            .findings
            .iter()
            .all(|f| f.language == Some(DomainLanguage::JavaScript))
    );
    Ok(())
}

#[test]
fn mixed_repo_preserves_per_language_identity() -> Result<(), String> {
    let root = ts_unique_tempdir("repo-mixed")?;
    ts_write_file(&root.join("src/lib.ts"), TS_SRC)?;
    ts_write_file(&root.join("src/clamp.js"), JS_SRC)?;
    let result = run(&root, 800);
    let _ = std::fs::remove_dir_all(&root);
    let result = result?;
    assert_eq!(result.production_files, 2);
    let has = |lang| result.findings.iter().any(|f| f.language == Some(lang));
    assert!(has(DomainLanguage::TypeScript) && has(DomainLanguage::JavaScript));
    Ok(())
}

#[test]
fn generated_vendor_build_declarations_and_tests_never_seed_findings() -> Result<(), String> {
    let root = ts_unique_tempdir("repo-excluded")?;
    ts_write_file(&root.join("node_modules/dep/index.js"), JS_SRC)?;
    ts_write_file(&root.join("dist/out.js"), JS_SRC)?;
    ts_write_file(&root.join("src/api.generated.ts"), TS_SRC)?;
    ts_write_file(
        &root.join("src/types.d.ts"),
        "export declare function f(): number;\n",
    )?;
    ts_write_file(&root.join("test/lib.test.ts"), TS_TEST)?;
    let result = run(&root, 800);
    let _ = std::fs::remove_dir_all(&root);
    let result = result?;
    assert!(result.findings.is_empty());
    assert_eq!(result.production_files, 0);
    // Honest complete zero: discovery finished and no production source exists.
    assert_eq!(result.partial_reason, None);
    Ok(())
}

#[test]
fn over_budget_repo_is_partial_and_names_recovery() -> Result<(), String> {
    let root = ts_unique_tempdir("repo-capped")?;
    ts_write_file(&root.join("src/a.ts"), TS_SRC)?;
    ts_write_file(&root.join("src/b.ts"), TS_SRC)?;
    let result = run(&root, 1);
    let _ = std::fs::remove_dir_all(&root);
    let result = result?;
    assert_eq!(result.production_files, 1);
    assert_eq!(result.skipped_files, 1);
    let reason = result.partial_reason.ok_or("capped run must be partial")?;
    assert!(reason.contains("RIPR_MAX_REPO_INDEX_FILES"), "{reason}");
    assert!(reason.contains("capped"), "{reason}");
    Ok(())
}

#[test]
fn parse_failure_is_partial_not_complete_zero() -> Result<(), String> {
    let root = ts_unique_tempdir("repo-parse-fail")?;
    ts_write_file(&root.join("src/broken.ts"), "export function (((( {\n")?;
    let result = run(&root, 800);
    let _ = std::fs::remove_dir_all(&root);
    let result = result?;
    assert!(
        result.partial_reason.is_some(),
        "a parse failure must never become a complete zero"
    );
    Ok(())
}

#[test]
fn unreadable_root_is_partial_not_honest_zero() -> Result<(), String> {
    let result = run(Path::new("/nonexistent_workspace"), 800)?;
    assert!(result.findings.is_empty());
    assert!(result.partial_reason.is_some());
    Ok(())
}

#[test]
fn repo_run_is_deterministic() -> Result<(), String> {
    let root = ts_unique_tempdir("repo-determinism")?;
    ts_write_file(&root.join("src/lib.ts"), TS_SRC)?;
    ts_write_file(&root.join("src/clamp.js"), JS_SRC)?;
    let first = run(&root, 800);
    let second = run(&root, 800);
    let _ = std::fs::remove_dir_all(&root);
    let (first, second) = (first?, second?);
    let ids = |r: &LanguageRepoResult| r.findings.iter().map(|f| f.id.clone()).collect::<Vec<_>>();
    assert_eq!(ids(&first), ids(&second));
    Ok(())
}
