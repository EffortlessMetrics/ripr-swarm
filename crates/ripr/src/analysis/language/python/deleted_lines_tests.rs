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
