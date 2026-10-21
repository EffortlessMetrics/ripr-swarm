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
        ts_workspace_file_limit(),
        ts_file_read_limit(),
        ts_workspace_read_budget(),
        &[],
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
            .is_some_and(|err| err.contains("positive integer, got \"0\""))
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
    assert!(reason.starts_with("typescript_repo_capped:"), "{reason}");
    assert!(reason.contains("RIPR_MAX_REPO_INDEX_FILES"), "{reason}");
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
        result
            .partial_reason
            .as_deref()
            .is_some_and(|reason| reason.contains("typescript_repo_limitations:")),
        "a parse failure must never become a complete zero: {:?}",
        result.partial_reason
    );
    Ok(())
}

#[test]
fn unreadable_root_is_partial_not_honest_zero() -> Result<(), String> {
    let result = run(Path::new("/nonexistent_workspace"), 800)?;
    assert!(result.findings.is_empty());
    assert!(
        result
            .partial_reason
            .as_deref()
            .is_some_and(|reason| reason.starts_with("typescript_repo_root_unreadable:"))
    );
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

#[test]
fn unparseable_test_file_makes_the_run_partial() -> Result<(), String> {
    let root = ts_unique_tempdir("repo-bad-test")?;
    ts_write_file(&root.join("src/lib.ts"), TS_SRC)?;
    ts_write_file(&root.join("test/lib.test.ts"), "describe(((( {\n")?;
    let result = run(&root, 800);
    let _ = std::fs::remove_dir_all(&root);
    let reason = result?.partial_reason.ok_or("must be partial")?;
    assert!(
        reason.starts_with("typescript_repo_test_parse_failure:"),
        "{reason}"
    );
    Ok(())
}

#[test]
fn test_files_beyond_the_limit_do_not_cap_the_run() -> Result<(), String> {
    let root = ts_unique_tempdir("repo-many-tests")?;
    ts_write_file(&root.join("src/lib.ts"), TS_SRC)?;
    for i in 0..4 {
        ts_write_file(&root.join(format!("test/t{i}.test.ts")), TS_TEST)?;
    }
    let result = run(&root, 1);
    let _ = std::fs::remove_dir_all(&root);
    let result = result?;
    assert_eq!(result.partial_reason, None);
    assert_eq!(result.skipped_files, 0);
    Ok(())
}

#[test]
fn truncated_discovery_walk_is_partial() -> Result<(), String> {
    let root = ts_unique_tempdir("repo-walk-cap")?;
    for i in 0..6 {
        ts_write_file(&root.join(format!("src/f{i}.ts")), TS_SRC)?;
    }
    let result = repo::analyze_repo_with_limit(
        &repo_options(&root),
        800,
        3,
        ts_file_read_limit(),
        ts_workspace_read_budget(),
        &[],
    );
    let _ = std::fs::remove_dir_all(&root);
    let reason = result?
        .partial_reason
        .ok_or("truncated walk must be partial")?;
    assert!(
        reason.starts_with("typescript_repo_discovery_truncated:"),
        "{reason}"
    );
    Ok(())
}

#[test]
fn skipped_links_disclose_even_when_production_is_selected() -> Result<(), String> {
    let root = ts_unique_tempdir("repo-links-skipped")?;
    ts_write_file(&root.join("src/lib.ts"), TS_SRC)?;
    ts_write_file(&root.join("real_more/extra.ts"), TS_SRC)?;
    // A link the discovery walk must not follow; sources behind it are
    // invisible, so the run must disclose the skip even though `src/lib.ts`
    // keeps the production set non-empty.
    #[cfg(unix)]
    let linked = {
        use std::os::unix::fs::symlink;
        let target = root.join("real_more");
        let link = root.join("linked_more");
        if symlink(&target, &link).is_err() {
            let _ = std::fs::remove_dir_all(&root);
            return Ok(()); // platform refused; nothing to assert here
        }
        link
    };
    #[cfg(windows)]
    let linked = {
        use std::os::windows::fs::symlink_dir;
        let target = root.join("real_more");
        let link = root.join("linked_more");
        if symlink_dir(&target, &link).is_err() {
            let _ = std::fs::remove_dir_all(&root);
            return Ok(()); // privilege missing; nothing to assert here
        }
        link
    };
    #[cfg(not(any(unix, windows)))]
    let linked: std::path::PathBuf = match () {
        _ => return Ok(()),
    };
    let result = run(&root, 800);
    let _ = std::fs::remove_dir_all(&root);
    let result = result?;
    assert!(
        !result.findings.is_empty(),
        "the real production file still seeds findings"
    );
    let reason = result
        .partial_reason
        .ok_or("skipped links must keep the run partial")?;
    assert!(
        reason.contains("typescript_repo_links_skipped:"),
        "{reason}"
    );
    let _ = linked;
    Ok(())
}

#[test]
fn over_read_bound_file_leaves_the_denominator_and_is_partial() -> Result<(), String> {
    let root = ts_unique_tempdir("repo-read-cap")?;
    ts_write_file(&root.join("src/lib.ts"), TS_SRC)?;
    let result = repo::analyze_repo_with_limit(
        &repo_options(&root),
        800,
        ts_workspace_file_limit(),
        8,
        ts_workspace_read_budget(),
        &[],
    );
    let _ = std::fs::remove_dir_all(&root);
    let result = result?;
    assert!(result.findings.is_empty());
    let reason = result.partial_reason.ok_or("unread file must be partial")?;
    assert!(reason.contains("typescript_repo_unread:"), "{reason}");
    Ok(())
}

#[test]
fn unreadable_test_file_makes_the_run_partial() -> Result<(), String> {
    let root = ts_unique_tempdir("repo-unreadable-test")?;
    ts_write_file(&root.join("src/lib.ts"), TS_SRC)?;
    std::fs::create_dir_all(root.join("test")).map_err(|err| err.to_string())?;
    // Invalid UTF-8: a plain IO/decode failure, not a size limit.
    std::fs::write(root.join("test/lib.test.ts"), [0xff, 0xfe, 0xfd])
        .map_err(|err| err.to_string())?;
    let result = run(&root, 800);
    let _ = std::fs::remove_dir_all(&root);
    let reason = result?.partial_reason.ok_or("must be partial")?;
    assert!(reason.contains("typescript_repo_unread_tests:"), "{reason}");
    Ok(())
}

#[test]
fn declared_handwritten_source_under_an_excluded_directory_is_admitted() -> Result<(), String> {
    let root = ts_unique_tempdir("repo-handwritten")?;
    ts_write_file(&root.join("dist/handwritten.ts"), TS_SRC)?;
    let declared = vec!["dist/handwritten.ts".to_string()];
    let with_decl = repo::analyze_repo_with_limit(
        &repo_options(&root),
        800,
        ts_workspace_file_limit(),
        ts_file_read_limit(),
        ts_workspace_read_budget(),
        &declared,
    );
    let without_decl = run(&root, 800);
    let _ = std::fs::remove_dir_all(&root);
    let with_decl = with_decl?;
    assert_eq!(with_decl.production_files, 1);
    assert!(
        with_decl
            .findings
            .iter()
            .any(|f| f.language == Some(DomainLanguage::TypeScript)),
        "the admitted file must be analyzed, not just counted"
    );
    assert_eq!(without_decl?.production_files, 0);
    Ok(())
}

#[test]
fn distinct_predicates_of_one_owner_both_survive() -> Result<(), String> {
    let root = ts_unique_tempdir("repo-two-predicates")?;
    ts_write_file(
        &root.join("src/lib.ts"),
        "export function f(a: number, b: number): number {\n  if (a > 0) {\n    return 1;\n  }\n  if (b > 7) {\n    return 2;\n  }\n  return 0;\n}\n",
    )?;
    let result = run(&root, 800);
    let _ = std::fs::remove_dir_all(&root);
    let exprs: Vec<String> = result?
        .findings
        .iter()
        .map(|f| f.probe.expression.clone())
        .collect();
    assert!(exprs.iter().any(|e| e.contains("a > 0")), "{exprs:?}");
    assert!(exprs.iter().any(|e| e.contains("b > 7")), "{exprs:?}");
    Ok(())
}
