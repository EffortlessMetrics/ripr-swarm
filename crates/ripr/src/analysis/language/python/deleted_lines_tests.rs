//! Tests for removed-only Python probes (#7587, RIPR-SPEC-0156): a deleted
//! behavior line no added line pairs with is base-side evidence on its owner,
//! never nothing and never a candidate edit target.

use super::*;
use crate::analysis::diff::ChangedLine;
use crate::domain::SourceCurrentness;

const CART_TEST_PY: &str = "from src.cart import capped_total\n\n\ndef test_capped_total_stops_at_the_cap():\n    assert capped_total([80, 50]) == 100\n";

fn line(number: usize, new_side_line: usize, text: &str) -> ChangedLine {
    ChangedLine {
        line: number,
        new_side_line,
        text: text.to_string(),
    }
}

/// Runs the adapter on `source` (the candidate file) with `added` new-side
/// line numbers and `removed` lines, and returns every finding.
fn analyze(
    label: &str,
    source: &str,
    added: &[usize],
    removed: Vec<ChangedLine>,
) -> Result<Vec<Finding>, String> {
    analyze_with_test(label, source, CART_TEST_PY, added, removed)
}

fn analyze_with_test(
    label: &str,
    source: &str,
    test_source: &str,
    added: &[usize],
    removed: Vec<ChangedLine>,
) -> Result<Vec<Finding>, String> {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|err| format!("system time: {err}"))?
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "ripr-python-deleted-lines-{label}-{}-{nanos}",
        std::process::id()
    ));
    for (path, contents) in [("src/cart.py", source), ("tests/test_cart.py", test_source)] {
        let file = root.join(path);
        if let Some(parent) = file.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|err| format!("create_dir_all({}): {err}", parent.display()))?;
        }
        std::fs::write(&file, contents)
            .map_err(|err| format!("write({}): {err}", file.display()))?;
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
    let mut added_lines = Vec::new();
    for number in added {
        let text = source
            .lines()
            .nth(number - 1)
            .ok_or_else(|| format!("fixture has no line {number}"))?;
        added_lines.push(line(*number, *number, text));
    }
    let changed_files = vec![ChangedFile {
        path: PathBuf::from("src/cart.py"),
        added_lines,
        removed_lines: removed,
    }];
    let result = PythonAdapter.analyze_diff(&options, &OraclePolicy::default(), &changed_files);
    let _ = std::fs::remove_dir_all(&root);
    let result = result?;
    assert_eq!(result.changed_files, 1, "the changed file must be analyzed");
    Ok(result.findings)
}

/// The base-side records: (projected line, owner name, currentness).
fn base_side(findings: &[Finding]) -> Vec<(usize, String, SourceCurrentness)> {
    findings
        .iter()
        .filter(|finding| finding.source_currentness != SourceCurrentness::CandidateCurrent)
        .map(|finding| {
            (
                finding.probe.location.line,
                finding
                    .probe
                    .owner
                    .as_ref()
                    .map(|owner| owner.0.rsplit("::").next().unwrap_or_default().to_string())
                    .unwrap_or_default(),
                finding.source_currentness,
            )
        })
        .collect()
}

fn assert_base_side_shape(findings: &[Finding], expression: &str) -> Result<(), String> {
    let finding = findings
        .iter()
        .find(|finding| finding.probe.expression.trim() == expression)
        .ok_or_else(|| format!("no record for `{expression}`: {findings:?}"))?;
    assert_eq!(finding.probe.after, None, "{finding:?}");
    assert_eq!(
        finding.probe.before.as_deref().map(str::trim),
        Some(expression),
        "{finding:?}"
    );
    assert_eq!(finding.id, finding.probe.id.0);
    assert!(!finding.source_currentness.permits_candidate_action());
    Ok(())
}

const CAPPED: &str = "CAP = 100\n\n\ndef capped_total(items):\n    total = sum(items)\n# keep the cap\n    return total\n";

/// The #7587 shape: the indented comment is re-added at column 0 and pairs
/// with the old comment by content, so no added line pairs with the deleted
/// clamp. It was silent; it is now base-side evidence on `capped_total`.
#[test]
fn deleted_line_beside_a_moved_comment_is_base_side_evidence() -> Result<(), String> {
    let findings = analyze(
        "moved-comment",
        CAPPED,
        &[6],
        vec![
            line(6, 6, "    # keep the cap"),
            line(7, 6, "    total = min(total, CAP)"),
        ],
    )?;
    assert_eq!(
        base_side(&findings),
        vec![(
            6,
            "capped_total".to_string(),
            SourceCurrentness::BaseDeleted
        )],
        "{findings:?}"
    );
    assert_base_side_shape(&findings, "total = min(total, CAP)")?;
    Ok(())
}

/// A pure deletion inside a body, with no added line at all.
#[test]
fn pure_deletion_inside_a_body_is_base_side_evidence() -> Result<(), String> {
    let source =
        "CAP = 100\n\n\ndef capped_total(items):\n    total = sum(items)\n    return total\n";
    let findings = analyze(
        "pure-deletion",
        source,
        &[],
        vec![line(6, 6, "    total = min(total, CAP)")],
    )?;
    assert_eq!(
        base_side(&findings),
        vec![(
            6,
            "capped_total".to_string(),
            SourceCurrentness::BaseDeleted
        )],
        "{findings:?}"
    );
    assert_base_side_shape(&findings, "total = min(total, CAP)")?;
    Ok(())
}

/// The diff parser records a line deleted from the end of a body at the
/// line after the owner; the owner is the one that ends on the line before.
#[test]
fn deletion_at_the_end_of_a_body_resolves_the_owner_above() -> Result<(), String> {
    let source = "CAP = 100\n\n\ndef capped_total(items):\n    total = sum(items)\n\n\nLIMIT = 3\n";
    let findings = analyze(
        "tail-deletion",
        source,
        &[],
        vec![line(6, 6, "    return min(total, CAP)")],
    )?;
    assert_eq!(
        base_side(&findings),
        vec![(
            6,
            "capped_total".to_string(),
            SourceCurrentness::BaseDeleted
        )],
        "{findings:?}"
    );
    Ok(())
}

/// Every removed line of an even block pairs with an added line, so the
/// rewrite keeps only its candidate-side finding.
#[test]
fn even_block_leaves_no_base_side_record() -> Result<(), String> {
    let source = "CAP = 100\n\n\ndef capped_total(items):\n    total = sum(items)\n    # cap it\n    return min(total, CAP)\n";
    let findings = analyze(
        "even-block",
        source,
        &[6, 7],
        vec![
            line(6, 6, "    # keep the cap"),
            line(7, 6, "    return total"),
        ],
    )?;
    assert_eq!(base_side(&findings), Vec::new(), "{findings:?}");
    assert!(
        findings
            .iter()
            .any(|finding| finding.probe.location.line == 7),
        "the rewritten return keeps its candidate-side finding: {findings:?}"
    );
    Ok(())
}

/// The same text re-added elsewhere in the file is movement evidence, so the
/// record is `moved_or_renamed`, and its probe id does not collide with the
/// candidate-side finding for the re-added line.
#[test]
fn deleted_line_re_added_elsewhere_is_moved_or_renamed() -> Result<(), String> {
    let source = "CAP = 100\n\n\ndef capped_total(items):\n    total = sum(items)\n    return total\n\n\ndef clamp(total):\n    total = min(total, CAP)\n    return total\n";
    let findings = analyze(
        "moved-line",
        source,
        &[10],
        vec![line(6, 6, "    total = min(total, CAP)")],
    )?;
    assert_eq!(
        base_side(&findings),
        vec![(
            6,
            "capped_total".to_string(),
            SourceCurrentness::MovedOrRenamed
        )],
        "{findings:?}"
    );
    let mut ids: Vec<&str> = findings.iter().map(|finding| finding.id.as_str()).collect();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(
        ids.len(),
        findings.len(),
        "probe ids stay unique: {findings:?}"
    );
    Ok(())
}

/// A body line whose `def` was deleted with it belonged to an owner that no
/// longer exists; it is not credited to a neighbour.
#[test]
fn deleted_function_body_is_not_given_to_a_neighbour() -> Result<(), String> {
    let source =
        "CAP = 100\n\n\ndef capped_total(items):\n    total = sum(items)\n    return total\n";
    let findings = analyze(
        "deleted-function",
        source,
        &[],
        vec![
            line(7, 7, ""),
            line(8, 7, ""),
            line(9, 7, "def clamp(total):"),
            line(10, 7, "    return min(total, CAP)"),
        ],
    )?;
    assert_eq!(base_side(&findings), Vec::new(), "{findings:?}");
    Ok(())
}

/// A module-level line deleted right after a function is not indented under
/// its header, so it is not that function's line: it is the module's.
#[test]
fn deleted_module_line_after_a_function_is_the_modules() -> Result<(), String> {
    let source = "CAP = 100\n\n\ndef capped_total(items):\n    total = sum(items)\n    return total\nLIMIT = 3\n";
    let findings = analyze(
        "module-line",
        source,
        &[],
        vec![line(7, 7, "CAP = min(CAP, 10)")],
    )?;
    assert_eq!(
        base_side(&findings),
        vec![(7, "<module>".to_string(), SourceCurrentness::BaseDeleted)],
        "{findings:?}"
    );
    Ok(())
}

/// Comments, docstring-free blank lines, imports and lone brackets run no
/// behavior, so deleting them leaves no record.
#[test]
fn deleted_quiet_lines_leave_no_record() -> Result<(), String> {
    let source =
        "CAP = 100\n\n\ndef capped_total(items):\n    total = sum(items)\n    return total\n";
    let findings = analyze(
        "quiet-lines",
        source,
        &[],
        vec![
            line(6, 6, "    # keep the cap"),
            line(7, 6, "    import math"),
            line(8, 6, "    )"),
            line(9, 6, ""),
        ],
    )?;
    assert_eq!(base_side(&findings), Vec::new(), "{findings:?}");
    Ok(())
}

/// The same line deleted twice from one owner hashes to the same probe id;
/// the second record takes an ordinal suffix, as Rust probe ids do.
#[test]
fn repeated_deleted_line_in_one_owner_keeps_ids_unique() -> Result<(), String> {
    let source =
        "CAP = 100\n\n\ndef capped_total(items):\n    total = sum(items)\n    return total\n";
    let findings = analyze(
        "repeated-line",
        source,
        &[],
        vec![
            line(6, 6, "    total = min(total, CAP)"),
            line(7, 6, "    total = min(total, CAP)"),
        ],
    )?;
    assert_eq!(base_side(&findings).len(), 2, "{findings:?}");
    let first = &findings[0].id;
    assert_eq!(findings[1].id, format!("{first}.2"), "{findings:?}");
    assert_eq!(findings[1].id, findings[1].probe.id.0);
    Ok(())
}

/// A deleted line inside the old docstring ran no behavior, even when its
/// text looks like code; the old-side docstring ranges say so.
#[test]
fn deleted_docstring_line_leaves_no_record() -> Result<(), String> {
    let source = "CAP = 100\n\n\ndef capped_total(items):\n    \"\"\"Sum the items.\n    \"\"\"\n    total = sum(items)\n    return total\n";
    let findings = analyze(
        "docstring-line",
        source,
        &[],
        vec![line(6, 6, "    total = min(total, CAP)")],
    )?;
    assert_eq!(base_side(&findings), Vec::new(), "{findings:?}");
    Ok(())
}

/// Review of #7587: a method's last line deleted where its class goes on
/// sits on a line only the class holds; the method above held it.
#[test]
fn method_tail_deleted_inside_a_continuing_class_is_the_methods() -> Result<(), String> {
    let source = "class Cart:\n    def capped(self, total):\n        total = total + 1\n\n    def size(self):\n        return 0\n";
    let findings = analyze(
        "method-tail",
        source,
        &[],
        vec![line(4, 4, "        return min(total, 100)")],
    )?;
    assert_eq!(
        base_side(&findings),
        vec![(4, "Cart.capped".to_string(), SourceCurrentness::BaseDeleted)],
        "{findings:?}"
    );
    Ok(())
}

/// Review of #7587: a line deleted from a top-level `if` block is indented
/// but outside every `def` and `class`, so it is the module's line, as the
/// same line added there would be.
#[test]
fn line_deleted_from_a_top_level_block_is_the_modules() -> Result<(), String> {
    let source = "import sys\n\nif sys.argv:\n    LIMIT = 3\n\n\ndef capped_total(items):\n    return sum(items)\n";
    let findings = analyze("module-block", source, &[], vec![line(5, 5, "    MAX = 4")])?;
    assert_eq!(
        base_side(&findings),
        vec![(5, "<module>".to_string(), SourceCurrentness::BaseDeleted)],
        "{findings:?}"
    );
    Ok(())
}

/// Review of #7587: a deleted predicate can read `weakly_exposed` with a
/// missing discriminator, the shape the Python repair card serves. The card
/// itself is withheld from base-side records in `output` (tested beside
/// `start_here_prefers_a_python_finding_with_a_repair_card`).
#[test]
fn deleted_predicate_can_carry_a_repair_card_shape() -> Result<(), String> {
    let source = "def step_value(x):\n    y = 0\n    if x > 1:\n        y = 1\n        y = 2\n    return y\n";
    let test = "from src.cart import step_value\n\n\ndef test_step_value():\n    assert step_value(1) == 2\n";
    let findings = analyze_with_test(
        "card-shape",
        source,
        test,
        &[],
        vec![line(5, 5, "    elif x > 0:")],
    )?;
    assert!(
        findings.iter().any(|finding| {
            finding.source_currentness == SourceCurrentness::BaseDeleted
                && finding.class == ExposureClass::WeaklyExposed
                && !finding.activation.missing_discriminators.is_empty()
        }),
        "{findings:?}"
    );
    Ok(())
}

/// Re-review of #7587: a blank line kept between the body and the deleted
/// `return` sits between them; the owner is still the function above.
#[test]
fn tail_deleted_after_a_kept_blank_line_is_the_functions() -> Result<(), String> {
    let source = "CAP = 100\n\n\ndef capped_total(items):\n    total = sum(items)\n\n\n\ndef other():\n    return 1\n";
    let findings = analyze(
        "blank-before-tail",
        source,
        &[],
        vec![line(7, 7, "    return min(total, CAP)")],
    )?;
    assert_eq!(
        base_side(&findings),
        vec![(
            7,
            "capped_total".to_string(),
            SourceCurrentness::BaseDeleted
        )],
        "{findings:?}"
    );
    Ok(())
}

/// Re-review of #7587: a method tail deleted after a kept comment line,
/// while the class goes on, is the method's.
#[test]
fn method_tail_deleted_after_a_kept_comment_is_the_methods() -> Result<(), String> {
    let source = "class Cart:\n    def capped(self, total):\n        total = total + 1\n        # cap it\n\n    def size(self):\n        return 0\n";
    let findings = analyze(
        "comment-before-tail",
        source,
        &[],
        vec![line(5, 5, "        return min(total, 100)")],
    )?;
    assert_eq!(
        base_side(&findings),
        vec![(5, "Cart.capped".to_string(), SourceCurrentness::BaseDeleted)],
        "{findings:?}"
    );
    Ok(())
}

/// Review of #7680: an added line interrupting a deleted function's body
/// gives the body lines after it another new-side coordinate. They still
/// lost their `def`, so they are not given to a neighbour.
#[test]
fn interrupted_deleted_function_body_is_not_given_to_a_neighbour() -> Result<(), String> {
    let source = "CAP = 100\n\n\ndef capped_total(items):\n    total = sum(items)\n    LIMIT = 3\n    return total\n";
    let findings = analyze(
        "interrupted-deleted-function",
        source,
        &[6],
        vec![
            line(6, 6, "def clamp(total):"),
            line(7, 6, "    total = min(total, CAP)"),
            line(8, 7, "    return total * 2"),
        ],
    )?;
    assert_eq!(base_side(&findings), Vec::new(), "{findings:?}");
    Ok(())
}

/// A kept line between a deleted `def` and a deleted body line means the
/// body line's owner is live, so it keeps its record.
#[test]
fn kept_owner_between_a_deleted_def_and_a_body_line_keeps_the_record() -> Result<(), String> {
    let source =
        "CAP = 100\n\n\ndef capped_total(items):\n    total = sum(items)\n    return total\n";
    let findings = analyze(
        "kept-owner-between",
        source,
        &[],
        vec![
            line(4, 4, "def clamp(total):"),
            line(5, 4, "    return min(total, CAP)"),
            line(8, 6, "    total = min(total, CAP)"),
        ],
    )?;
    assert_eq!(
        base_side(&findings),
        vec![(
            6,
            "capped_total".to_string(),
            SourceCurrentness::BaseDeleted
        )],
        "{findings:?}"
    );
    Ok(())
}

/// A deleted `def` whose signature spans lines closes with a column-0 `):`;
/// that line continues the signature, so the walk still reaches the `def`
/// and the body line is not given to the function above.
#[test]
fn deleted_function_with_a_multi_line_signature_is_not_given_to_a_neighbour() -> Result<(), String>
{
    let source = "CAP = 100\n\n\ndef keep(items):\n    total = sum(items)\n    return total\n";
    let findings = analyze(
        "multi-line-signature",
        source,
        &[],
        vec![
            line(7, 7, ""),
            line(8, 7, ""),
            line(9, 7, "def clamp("),
            line(10, 7, "    total,"),
            line(11, 7, "):"),
            line(12, 7, "    return min(total, CAP)"),
        ],
    )?;
    assert_eq!(base_side(&findings), Vec::new(), "{findings:?}");
    Ok(())
}

/// The closing quotes of a string written at column 0 inside a deleted body
/// say nothing about nesting either.
#[test]
fn column_zero_string_close_in_a_deleted_body_keeps_the_def() -> Result<(), String> {
    let source = "CAP = 100\n\n\ndef keep(items):\n    total = sum(items)\n    return total\n";
    let findings = analyze(
        "column-zero-string",
        source,
        &[],
        vec![
            line(7, 7, ""),
            line(8, 7, ""),
            line(9, 7, "def clamp(total):"),
            line(10, 7, "    note = \"\"\""),
            line(11, 7, "hello"),
            line(12, 7, "\"\"\""),
            line(13, 7, "    return min(total, CAP)"),
        ],
    )?;
    assert!(
        base_side(&findings)
            .iter()
            .all(|(_, owner, _)| owner != "keep"),
        "{findings:?}"
    );
    Ok(())
}

/// A deleted `try:` inside a live function is a block header: its body lines
/// belong to the function, not to a deleted nested `def` above the `try:`.
#[test]
fn deleted_try_block_after_a_deleted_nested_def_keeps_the_live_owner() -> Result<(), String> {
    let source = "def outer(total):\n    x = 1\n    return x\n";
    let findings = analyze(
        "try-after-nested-def",
        source,
        &[],
        vec![
            line(3, 3, "    def helper():"),
            line(4, 3, "        return 1"),
            line(5, 3, "    try:"),
            line(6, 3, "        total = run(total)"),
            line(7, 3, "    finally:"),
            line(8, 3, "        total = min(total, 100)"),
        ],
    )?;
    let owners: Vec<String> = base_side(&findings)
        .into_iter()
        .map(|(_, owner, _)| owner)
        .collect();
    assert_eq!(
        owners,
        vec!["outer".to_string(), "outer".to_string()],
        "{findings:?}"
    );
    Ok(())
}

/// Devin review of #7680: an annotation-only header edit pairs the old `def`
/// with the new one, so the owner is live and the deleted body line keeps
/// its record.
#[test]
fn deleted_body_line_under_an_annotation_only_header_edit_keeps_its_owner() -> Result<(), String> {
    let source = "def f(x: str):\n    return x\n";
    let findings = analyze(
        "annotation-header",
        source,
        &[1],
        vec![
            line(1, 1, "def f(x: int):"),
            line(2, 2, "    x = adjust(x)"),
        ],
    )?;
    assert_eq!(
        base_side(&findings),
        vec![(2, "f".to_string(), SourceCurrentness::BaseDeleted)],
        "{findings:?}"
    );
    Ok(())
}

/// Devin review of #7680: a module statement deleted at the end of the file
/// projects past every surviving line and still belongs to the module.
#[test]
fn module_line_deleted_at_end_of_file_is_the_modules() -> Result<(), String> {
    let source = "def f():\n    return 1\n";
    let findings = analyze("module-eof", source, &[], vec![line(3, 3, "LIMIT = 3")])?;
    assert_eq!(
        base_side(&findings),
        vec![(3, "<module>".to_string(), SourceCurrentness::BaseDeleted)],
        "{findings:?}"
    );
    Ok(())
}

/// Devin review of #7680: a deleted tail resolves its owner from the line
/// above it, but every line-bearing fact names the deleted line's own
/// projected coordinate, not that surviving neighbour.
#[test]
fn deleted_tail_evidence_names_its_own_line() -> Result<(), String> {
    let source = "class Cart:\n    def capped(self, total):\n        total = total + 1\n\n    def size(self):\n        return 0\n";
    let findings = analyze(
        "tail-sink-line",
        source,
        &[],
        vec![line(4, 4, "        return min(total, 100)")],
    )?;
    let base: Vec<&Finding> = findings
        .iter()
        .filter(|finding| finding.source_currentness.is_base_side())
        .collect();
    assert_eq!(base.len(), 1, "{findings:?}");
    let finding = base[0];
    assert_eq!(finding.probe.location.line, 4);
    assert!(!finding.flow_sinks.is_empty(), "{finding:?}");
    for sink in &finding.flow_sinks {
        assert_eq!(sink.line, 4, "{sink:?}");
    }
    for missing in &finding.activation.missing_discriminators {
        assert!(!missing.reason.contains("line 3"), "{missing:?}");
        if let Some(sink) = &missing.flow_sink {
            assert_eq!(sink.line, 4, "{sink:?}");
        }
    }
    Ok(())
}

const KEEP: &str = "CAP = 100\n\n\ndef keep(items):\n    total = sum(items)\n    return total\n";

/// The text of every base-side record, trimmed.
fn base_side_texts(findings: &[Finding]) -> Vec<String> {
    findings
        .iter()
        .filter(|finding| finding.source_currentness.is_base_side())
        .filter_map(|finding| finding.probe.before.as_deref())
        .map(|text| text.trim().to_string())
        .collect()
}

/// A deleted statement that wraps its arguments is one record, on the line
/// that starts it: the argument line and the closing line are fragments of
/// it, not statements of their own (#7690).
#[test]
fn deleted_multi_line_statement_is_one_record() -> Result<(), String> {
    let findings = analyze(
        "multi-line-statement",
        KEEP,
        &[],
        vec![
            line(6, 6, "    total = min("),
            line(7, 6, "        total,"),
            line(8, 6, "        CAP,"),
            line(9, 6, "    )"),
        ],
    )?;
    assert_eq!(
        base_side(&findings),
        vec![(6, "keep".to_string(), SourceCurrentness::BaseDeleted)],
        "{findings:?}"
    );
    assert_eq!(base_side_texts(&findings), vec!["total = min("]);
    Ok(())
}

/// The issue's `with open(` shape: the block header wraps, and the body
/// line under it is a statement of its own.
#[test]
fn deleted_wrapped_block_header_keeps_only_statement_starts() -> Result<(), String> {
    let findings = analyze(
        "wrapped-with",
        KEEP,
        &[],
        vec![
            line(6, 6, "    with open("),
            line(7, 6, "        path,"),
            line(8, 6, "    ) as handle:"),
            line(9, 6, "        total = handle.read()"),
        ],
    )?;
    assert_eq!(
        base_side_texts(&findings),
        vec!["with open(", "total = handle.read()"],
        "{findings:?}"
    );
    assert!(
        base_side(&findings)
            .iter()
            .all(|(_, owner, _)| owner == "keep"),
        "{findings:?}"
    );
    Ok(())
}

/// A triple-quoted string deleted from a live function, its body at column
/// 0, is one record on the function: its body and closing quotes are not
/// module statements.
#[test]
fn deleted_triple_quoted_string_body_is_not_the_modules() -> Result<(), String> {
    let findings = analyze(
        "triple-quoted-body",
        KEEP,
        &[],
        vec![
            line(6, 6, "    note = \"\"\""),
            line(7, 6, "hello"),
            line(8, 6, "\"\"\""),
        ],
    )?;
    assert_eq!(
        base_side(&findings),
        vec![(6, "keep".to_string(), SourceCurrentness::BaseDeleted)],
        "{findings:?}"
    );
    Ok(())
}

/// A deleted `def` whose signature closes with `) -> int:` at column 0
/// records nothing: the closing line continues the deleted signature.
#[test]
fn deleted_signature_close_with_a_return_type_records_nothing() -> Result<(), String> {
    let findings = analyze(
        "signature-return-type",
        KEEP,
        &[],
        vec![
            line(7, 7, ""),
            line(8, 7, ""),
            line(9, 7, "def clamp("),
            line(10, 7, "    total,"),
            line(11, 7, ") -> int:"),
            line(12, 7, "    return min(total, CAP)"),
        ],
    )?;
    assert_eq!(base_side(&findings), Vec::new(), "{findings:?}");
    Ok(())
}

/// A backslash continues a statement onto the next line.
#[test]
fn deleted_backslash_continuation_is_one_record() -> Result<(), String> {
    let findings = analyze(
        "backslash",
        KEEP,
        &[],
        vec![
            line(6, 6, "    total = total + \\"),
            line(7, 6, "        CAP"),
        ],
    )?;
    assert_eq!(base_side_texts(&findings), vec!["total = total + \\"]);
    Ok(())
}

/// A deleted function whose docstring spans lines records nothing: the
/// docstring body is one string, so the `return` under it still belongs to
/// the deleted `def`, not to the kept function above (#7690).
#[test]
fn deleted_function_with_a_multi_line_docstring_records_nothing() -> Result<(), String> {
    let findings = analyze(
        "multi-line-docstring",
        KEEP,
        &[],
        vec![
            line(7, 7, ""),
            line(8, 7, ""),
            line(9, 7, "def gone(total):"),
            line(10, 7, "    \"\"\"Doc."),
            line(11, 7, "    more"),
            line(12, 7, "    \"\"\""),
            line(13, 7, "    return min(total, CAP)"),
        ],
    )?;
    assert!(base_side_texts(&findings).is_empty(), "{findings:?}");
    Ok(())
}

/// A backslash that ends a comment is part of the comment, so the next
/// deleted line is a statement of its own (#7690).
#[test]
fn backslash_ending_a_comment_does_not_join_lines() -> Result<(), String> {
    let findings = analyze(
        "comment-backslash",
        KEEP,
        &[],
        vec![
            line(6, 6, "    total = total + 1  # wraps \\"),
            line(7, 6, "    total = min(total, CAP)"),
        ],
    )?;
    assert_eq!(
        base_side_texts(&findings),
        vec!["total = total + 1  # wraps \\", "total = min(total, CAP)"]
    );
    Ok(())
}

/// A deleted run that starts inside a kept docstring closes it: the `"""`
/// is the docstring's close, so the statement after it is still recorded
/// (#7690).
#[test]
fn deleted_run_closing_a_kept_docstring_keeps_the_statement_after_it() -> Result<(), String> {
    let findings = analyze(
        "kept-docstring-opener",
        "def f(x):\n    \"\"\"Summary.\n\n    \"\"\"\n    return x\n",
        &[4],
        vec![
            line(4, 4, "    Details."),
            line(5, 4, "    \"\"\""),
            line(6, 5, "    x = adjust(x)"),
        ],
    )?;
    assert!(
        base_side(&findings).contains(&(5, "f".to_string(), SourceCurrentness::BaseDeleted)),
        "{findings:?}"
    );
    assert!(
        base_side_texts(&findings).contains(&"x = adjust(x)".to_string()),
        "{findings:?}"
    );
    Ok(())
}

/// The same when a kept assignment opens the string the run closes.
#[test]
fn deleted_run_closing_a_kept_string_keeps_the_statement_after_it() -> Result<(), String> {
    let findings = analyze(
        "kept-string-opener",
        "def keep(items):\n    note = \"\"\"\nhello\nbye\"\"\"\n    return items\n",
        &[4],
        vec![line(4, 4, "\"\"\""), line(5, 5, "    items = clamp(items)")],
    )?;
    assert_eq!(
        base_side_texts(&findings),
        vec!["items = clamp(items)"],
        "{findings:?}"
    );
    Ok(())
}

/// An argument deleted from a kept multi-line call is a deletion of its
/// own: the call starts and closes on kept lines, so the removed line is
/// the record, on the function that holds the call (#7690).
#[test]
fn argument_deleted_from_a_kept_call_is_base_side_evidence() -> Result<(), String> {
    let findings = analyze(
        "kept-call-argument",
        "def keep(items):\n    total = min(\n        sum(items),\n    )\n    return total\n",
        &[],
        vec![line(4, 4, "        CAP,")],
    )?;
    assert_eq!(
        base_side(&findings),
        vec![(4, "keep".to_string(), SourceCurrentness::BaseDeleted)],
        "{findings:?}"
    );
    assert_eq!(base_side_texts(&findings), vec!["CAP,"]);
    Ok(())
}

/// Two arguments deleted together from a kept call: the first is the
/// record and the second is a fragment of the same statement.
#[test]
fn arguments_deleted_together_from_a_kept_call_are_one_record() -> Result<(), String> {
    let findings = analyze(
        "kept-call-arguments",
        "def keep(items):\n    total = min(\n        sum(items),\n    )\n    return total\n",
        &[],
        vec![line(4, 4, "        CAP,"), line(5, 4, "        FLOOR,")],
    )?;
    assert_eq!(base_side_texts(&findings), vec!["CAP,"], "{findings:?}");
    Ok(())
}

/// Inside a kept call, a removed argument an added argument replaces is the
/// candidate side's record, so the argument deleted after it still gets
/// its own base-side record (#7690).
#[test]
fn argument_deleted_after_a_replaced_one_in_a_kept_call_is_recorded() -> Result<(), String> {
    let findings = analyze(
        "kept-call-replaced-argument",
        "def keep(items):\n    total = min(\n        c,\n    )\n    return total\n",
        &[3],
        vec![line(3, 3, "        a,"), line(4, 3, "        b,")],
    )?;
    assert_eq!(base_side_texts(&findings), vec!["b,"], "{findings:?}");
    assert_eq!(
        base_side(&findings),
        vec![(3, "keep".to_string(), SourceCurrentness::BaseDeleted)],
        "{findings:?}"
    );
    Ok(())
}

/// A deleted call wrapped in a lone opening bracket is still one record,
/// on its first behavior line: the bracket line carries no behavior of its
/// own (#7690).
#[test]
fn deleted_statement_opening_with_a_lone_bracket_is_recorded() -> Result<(), String> {
    let findings = analyze(
        "lone-bracket-start",
        KEEP,
        &[],
        vec![
            line(6, 6, "    ("),
            line(7, 6, "        send_event()"),
            line(8, 6, "    )"),
        ],
    )?;
    assert_eq!(
        base_side_texts(&findings),
        vec!["send_event()"],
        "{findings:?}"
    );
    Ok(())
}

/// A bracket only the candidate opens does not swallow later deletions: the
/// run's start comes from the old file's kept lines, so each deleted call is
/// a record of its own (#7690).
#[test]
fn candidate_only_bracket_does_not_merge_later_deletions() -> Result<(), String> {
    let findings = analyze(
        "candidate-only-bracket",
        "def keep(items):\n    value = (\n    # note\n    # more\n        1\n    )\n    return value\n",
        &[2, 5, 6],
        vec![
            line(2, 2, "    value = 1"),
            line(4, 4, "    log_a()"),
            line(5, 4, "    log_b()"),
        ],
    )?;
    let texts = base_side_texts(&findings);
    assert!(texts.contains(&"log_a()".to_string()), "{findings:?}");
    assert!(texts.contains(&"log_b()".to_string()), "{findings:?}");
    Ok(())
}

/// A line inside a bracket keeps the indent of the statement it sits in:
/// a zero-indented call inside a deleted function's lone `(` neither moves
/// the function's `return` to the kept function above nor records on it
/// (#7690).
#[test]
fn zero_indented_line_inside_a_bracket_does_not_decide_nesting() -> Result<(), String> {
    let findings = analyze(
        "zero-indented-in-bracket",
        KEEP,
        &[],
        vec![
            line(7, 7, ""),
            line(8, 7, ""),
            line(9, 7, "def gone():"),
            line(10, 7, "    ("),
            line(11, 7, "send_event()"),
            line(12, 7, "    )"),
            line(13, 7, "    return 2"),
        ],
    )?;
    assert!(base_side_texts(&findings).is_empty(), "{findings:?}");
    Ok(())
}
