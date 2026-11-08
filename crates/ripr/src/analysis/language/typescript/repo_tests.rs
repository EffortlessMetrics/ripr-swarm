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
        &crate::config::TypescriptLanguageConfig::default(),
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
        &crate::config::TypescriptLanguageConfig::default(),
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
    // The target sits outside the root: a link to a directory the walk
    // already visits is an alias and is not disclosed (#10829).
    let outside = ts_unique_tempdir("repo-links-outside")?;
    ts_write_file(&outside.join("real_more/extra.ts"), TS_SRC)?;
    // A link the discovery walk must not follow; sources behind it are
    // invisible, so the run must disclose the skip even though `src/lib.ts`
    // keeps the production set non-empty.
    #[cfg(unix)]
    let linked = {
        use std::os::unix::fs::symlink;
        let target = outside.join("real_more");
        let link = root.join("linked_more");
        if symlink(&target, &link).is_err() {
            let _ = std::fs::remove_dir_all(&root);
            let _ = std::fs::remove_dir_all(&outside);
            return Ok(()); // platform refused; nothing to assert here
        }
        link
    };
    #[cfg(windows)]
    let linked = {
        use std::os::windows::fs::symlink_dir;
        let target = outside.join("real_more");
        let link = root.join("linked_more");
        if symlink_dir(&target, &link).is_err() {
            let _ = std::fs::remove_dir_all(&root);
            let _ = std::fs::remove_dir_all(&outside);
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
    let _ = std::fs::remove_dir_all(&outside);
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
fn a_link_to_a_non_source_file_does_not_make_repo_mode_partial() -> Result<(), String> {
    let root = ts_unique_tempdir("repo-links-nonsource")?;
    ts_write_file(&root.join("src/lib.ts"), TS_SRC)?;
    ts_write_file(&root.join("README.md"), "# docs\n")?;
    // A generated-named link is a name the walk would not index either.
    for name in ["docs.md", "bundle.generated.ts"] {
        #[cfg(unix)]
        let linked = std::os::unix::fs::symlink(root.join("README.md"), root.join(name));
        #[cfg(windows)]
        let linked = std::os::windows::fs::symlink_file(root.join("README.md"), root.join(name));
        #[cfg(not(any(unix, windows)))]
        let linked: std::io::Result<()> = Err(std::io::ErrorKind::Unsupported.into());
        if linked.is_err() {
            let _ = std::fs::remove_dir_all(&root);
            return Ok(()); // platform or privilege refused; nothing to assert here
        }
    }
    let result = run(&root, 800);
    let _ = std::fs::remove_dir_all(&root);
    let result = result?;
    assert!(
        !result.findings.is_empty(),
        "the real production file still seeds findings"
    );
    assert!(
        result
            .partial_reason
            .as_deref()
            .is_none_or(|reason| !reason.contains("typescript_repo_links_skipped:")),
        "{:?}",
        result.partial_reason
    );
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
        &crate::config::TypescriptLanguageConfig::default(),
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
        &crate::config::TypescriptLanguageConfig {
            handwritten_files: declared,
            ..crate::config::TypescriptLanguageConfig::default()
        },
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

#[test]
fn capped_run_reports_typed_selected_omitted_total_and_first_stage() -> Result<(), String> {
    let root = ts_unique_tempdir("repo-accounting-capped")?;
    for name in ["a", "b", "c"] {
        ts_write_file(&root.join(format!("src/{name}.ts")), TS_SRC)?;
    }
    ts_write_file(&root.join("test/a.test.ts"), TS_TEST)?;
    ts_write_file(&root.join("src/types.d.ts"), "export type T = number;\n")?;
    let result = run(&root, 2);
    let _ = std::fs::remove_dir_all(&root);
    let accounting = result?.file_accounting;
    assert_eq!(accounting.discovered, Some(5));
    assert_eq!(accounting.selected, Some(3), "two production plus one test");
    assert_eq!(accounting.skipped, Some(1), "declaration file");
    assert_eq!(accounting.omitted, Some(1));
    assert_eq!(
        accounting.first_cap_fired,
        Some(RepoCapFired {
            name: "RIPR_MAX_REPO_INDEX_FILES",
            stage: RepoLimitingStage::Selection,
            limit: 2,
        })
    );
    Ok(())
}

#[test]
fn uncapped_run_reports_measured_zero_omitted_and_no_cap() -> Result<(), String> {
    let root = ts_unique_tempdir("repo-accounting-uncapped")?;
    ts_write_file(&root.join("src/a.ts"), TS_SRC)?;
    let result = run(&root, 800);
    let _ = std::fs::remove_dir_all(&root);
    let accounting = result?.file_accounting;
    assert_eq!(accounting.discovered, Some(1));
    assert_eq!(accounting.omitted, Some(0), "measured zero, not unknown");
    assert_eq!(accounting.first_cap_fired, None);
    Ok(())
}

#[test]
fn truncated_walk_names_the_discovery_cap_first_and_leaves_total_unknown() -> Result<(), String> {
    let root = ts_unique_tempdir("repo-accounting-walk")?;
    for name in ["a", "b", "c", "d"] {
        ts_write_file(&root.join(format!("src/{name}.ts")), TS_SRC)?;
    }
    let result = repo::analyze_repo_with_limit(
        &repo_options(&root),
        1,
        2,
        ts_file_read_limit(),
        ts_workspace_read_budget(),
        &crate::config::TypescriptLanguageConfig::default(),
    );
    let _ = std::fs::remove_dir_all(&root);
    let accounting = result?.file_accounting;
    assert_eq!(accounting.discovered, None, "truncated walk has no total");
    assert_eq!(accounting.selected, Some(1), "selected is exact");
    assert_eq!(accounting.skipped, None, "lower bound only");
    assert_eq!(accounting.omitted, None, "lower bound only");
    assert_eq!(
        accounting.first_cap_fired,
        Some(RepoCapFired {
            name: "RIPR_TS_MAX_WORKSPACE_FILES",
            stage: RepoLimitingStage::Discovery,
            limit: 2,
        }),
        "discovery precedes selection even when both fired"
    );
    Ok(())
}

#[test]
fn unreadable_root_leaves_every_count_unknown() -> Result<(), String> {
    let root = ts_unique_tempdir("repo-accounting-missing")?;
    let missing = root.join("absent");
    let result = run(&missing, 800);
    let _ = std::fs::remove_dir_all(&root);
    assert_eq!(result?.file_accounting, RepoFileAccounting::default());
    Ok(())
}

#[test]
fn working_set_boundary_omits_only_beyond_the_limit() -> Result<(), String> {
    let limit = 3;
    for (count, omitted) in [(limit - 1, 0), (limit, 0), (limit + 1, 1)] {
        let root = ts_unique_tempdir("repo-accounting-boundary")?;
        for index in 0..count {
            ts_write_file(&root.join(format!("src/f{index}.ts")), TS_SRC)?;
        }
        let result = run(&root, limit);
        let _ = std::fs::remove_dir_all(&root);
        let accounting = result?.file_accounting;
        assert_eq!(accounting.discovered, Some(count), "count {count}");
        assert_eq!(accounting.selected, Some(count.min(limit)), "count {count}");
        assert_eq!(accounting.omitted, Some(omitted), "count {count}");
        assert_eq!(
            accounting.first_cap_fired.is_some(),
            omitted > 0,
            "count {count}"
        );
    }
    Ok(())
}

#[test]
fn real_analyze_repo_output_flows_into_the_outcome_verbatim() -> Result<(), String> {
    use crate::analysis::language_outcome::LanguageAnalysisOutcomeV1;
    let root = ts_unique_tempdir("repo-accounting-outcome")?;
    for name in ["a", "b", "c"] {
        ts_write_file(&root.join(format!("src/{name}.ts")), TS_SRC)?;
    }
    let result = run(&root, 2);
    let _ = std::fs::remove_dir_all(&root);
    let result = result?;
    let outcome =
        LanguageAnalysisOutcomeV1::from_repo_result(DomainLanguage::TypeScript, &result, None);
    assert_eq!(outcome.resources.files, result.file_accounting);
    assert_eq!(outcome.resources.files.omitted, Some(1));
    assert_eq!(
        outcome.resources.files.first_cap_fired.map(|cap| cap.name),
        Some("RIPR_MAX_REPO_INDEX_FILES")
    );
    Ok(())
}

/// An alias an indexed test imports through (`../alias/lib`, `alias -> src`)
/// names a path the walk never indexes, so repo mode stays partial (#10829);
/// a direct-path import of the same owner relates the test and is not
/// disclosed.
#[cfg(unix)]
#[test]
fn an_alias_imported_through_keeps_repo_mode_partial_but_a_direct_import_does_not()
-> Result<(), String> {
    let root = ts_unique_tempdir("repo-alias-import")?;
    ts_write_file(&root.join("src/lib.ts"), TS_SRC)?;
    ts_write_file(
        &root.join("tests/lib.test.ts"),
        &TS_TEST
            .replace("import {", "/* \" */ import {")
            .replace("../src/lib", "../alias/lib"),
    )?;
    if std::os::unix::fs::symlink("src", root.join("alias")).is_err() {
        let _ = std::fs::remove_dir_all(&root);
        return Ok(());
    }
    let canonical = std::fs::canonicalize(root.join("alias")).map_err(|e| e.to_string())?;
    let through = run(&root, 800);
    // An escaped separator leaves neither `./` nor `/alias` in the raw text.
    ts_write_file(
        &root.join("tests/lib.test.ts"),
        &TS_TEST.replace("../src/lib", "..\\u002falias/lib"),
    )?;
    let through_encoded = run(&root, 800);
    ts_write_file(&root.join("tests/lib.test.ts"), TS_TEST)?;
    let direct = run(&root, 800);
    let _ = std::fs::remove_dir_all(&root);
    assert!(
        canonical.ends_with("src") || !canonical.exists(),
        "{canonical:?}"
    );
    let through = through?;
    let through_reason = through
        .partial_reason
        .ok_or("an import through the alias must keep the run partial")?;
    assert!(
        through_reason.contains("typescript_repo_links_skipped:"),
        "{through_reason}"
    );
    let encoded_reason = through_encoded?
        .partial_reason
        .ok_or("an escaped import through the alias must keep the run partial")?;
    assert!(
        encoded_reason.contains("typescript_repo_links_skipped:"),
        "{encoded_reason}"
    );
    let direct = direct?;
    assert!(
        direct.findings.iter().any(|finding| finding
            .related_tests
            .iter()
            .any(|t| t.name.contains("is positive"))),
        "the direct-path import relates the test to classify: {:?}",
        direct.findings
    );
    assert!(
        direct
            .partial_reason
            .is_none_or(|reason| !reason.contains("typescript_repo_links_skipped:")),
        "a direct-path import never traverses the alias"
    );
    Ok(())
}
