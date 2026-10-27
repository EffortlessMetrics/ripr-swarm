//! Ambient declarations (parsed `declare ...` statements and `.d.ts` files)
//! are type-only, so a change to them produces no probe (2026-09-27 re-walk:
//! they produced a `predicate` finding reading `no_static_path`).

use super::*;
use crate::analysis::diff::ChangedLine;
use std::path::PathBuf;

fn changed(path: &str, lines: &[(usize, &str)]) -> ChangedFile {
    ChangedFile {
        path: PathBuf::from(path),
        added_lines: lines
            .iter()
            .map(|(line, text)| ChangedLine {
                line: *line,
                text: (*text).to_string(),
                new_side_line: *line,
            })
            .collect(),
        removed_lines: Vec::new(),
    }
}

fn analyze(
    label: &str,
    files: &[(&str, &str)],
    change: ChangedFile,
) -> Result<LanguageDiffResult, String> {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|err| err.to_string())?
        .as_nanos();
    let root = std::env::temp_dir().join(format!("ripr-ts-ambient-{label}-{nanos}"));
    for (path, source) in files {
        let path = root.join(path);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|err| err.to_string())?;
        }
        std::fs::write(&path, source).map_err(|err| err.to_string())?;
    }
    let options = AnalysisOptions {
        root: root.clone(),
        base: None,
        diff_file: None,
        mode: crate::analysis::AnalysisMode::Draft,
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
    };
    let result = TypeScriptAdapter.analyze_diff(&options, &OraclePolicy::default(), &[change]);
    let _ = std::fs::remove_dir_all(&root);
    result
}

#[test]
fn declaration_file_change_produces_no_probe_but_is_counted() -> Result<(), String> {
    let root_files = [(
        "src/types.d.ts",
        "export declare function typed(x: number): string;\nexport function plain(x: number): string;\n",
    )];
    let result = analyze(
        "dts",
        &root_files,
        changed(
            "src/types.d.ts",
            &[
                (1, "export declare function typed(x: number): string;"),
                (2, "export function plain(x: number): string;"),
            ],
        ),
    )?;
    assert!(result.findings.is_empty(), "{:?}", result.findings);
    assert_eq!(
        result.changed_files, 1,
        "the declaration file stays in the denominator"
    );
    Ok(())
}

#[test]
fn declare_line_in_source_file_is_ignored_but_runtime_line_is_probed() -> Result<(), String> {
    let source = "export declare function typed(x: number): string;\nexport function run(limit: number): number {\n  return limit + 1;\n}\n";
    let findings = analyze(
        "declare-line",
        &[("src/run.ts", source)],
        changed(
            "src/run.ts",
            &[
                (1, "export declare function typed(x: number): string;"),
                (3, "  return limit + 1;"),
            ],
        ),
    )?
    .findings;
    let lines: Vec<usize> = findings
        .iter()
        .map(|finding| finding.probe.location.line)
        .collect();
    assert_eq!(
        lines,
        vec![3],
        "only the runtime line is probed: {findings:?}"
    );
    Ok(())
}

#[test]
fn only_parsed_ambient_declarations_are_skipped() -> Result<(), String> {
    // `declare` is also a runtime identifier, and a template literal line can
    // start with `declare `: both stay probed. The body of `declare module`
    // is ambient even on lines without the keyword.
    let source = "declare module 'x' {\n  export function f(): number;\n}\nexport function emit(): string {\n  return `\ndeclare module y;\n`;\n}\nexport function reset(): number {\n  let declare = 0;\n  declare = compute();\n  return declare;\n}\nfunction compute(): number {\n  return 1;\n}\n";
    let findings = analyze(
        "declare-identifier",
        &[("src/emit.ts", source)],
        changed(
            "src/emit.ts",
            &[
                (2, "  export function f(): number;"),
                (6, "declare module y;"),
                (11, "  declare = compute();"),
            ],
        ),
    )?
    .findings;
    let lines: Vec<usize> = findings
        .iter()
        .map(|finding| finding.probe.location.line)
        .collect();
    assert_eq!(
        lines,
        vec![6, 11],
        "the ambient module body is skipped, runtime lines are probed: {findings:?}"
    );
    Ok(())
}

fn parse_error_count(file: &str, source: &str) -> usize {
    let allocator = oxc_allocator::Allocator::default();
    oxc_parser::Parser::new(&allocator, source, source_type_for(Path::new(file)))
        .parse()
        .errors
        .len()
}

/// #10812: a declaration file's top-level `const` has no initializer by
/// design, so it must parse clean under `.d.ts`, `.d.mts` and `.d.cts`, while
/// the same text in a `.ts` file is still a disclosed parse error.
#[test]
fn declaration_files_parse_ambient_const_without_an_initializer() {
    let source = "export const $: Runner<{}>;\nexport declare function run(): void;\n";
    for file in ["types/api.d.ts", "types/api.d.mts", "types/api.d.cts"] {
        assert_eq!(parse_error_count(file, source), 0, "{file}");
    }
    assert!(
        parse_error_count("src/api.ts", source) > 0,
        "the same text in an ordinary .ts file must stay a parse error"
    );
    assert!(
        parse_error_count("types/api.d.ts", "export const = ;\n") > 0,
        "a real syntax error in a declaration file is still disclosed"
    );
}
