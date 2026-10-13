//! Tests for old-line pairing inside a replacement block (#7556): every
//! removed line of a block shares the new-side line where the added run
//! starts, so a later added line must take its old line from
//! `ChangedFile::aligned_counterparts`, not from that coordinate.

use super::*;
use std::path::PathBuf;

const ACCOUNT_PY: &str = "class Account:\n    def __init__(self, balance):\n        self._balance = balance\n\n    def balance(self):\n        # the amount on record, never below zero\n        return max(0, self._balance)\n";

/// Calls `balance()` but asserts only the stored operand, which the clamp
/// leaves unchanged: the delta (`max`) is unobserved.
const ACCOUNT_TEST_PY: &str = "from src.account import Account\n\n\ndef test_balance_starts_at_the_opening_amount():\n    account = Account(100)\n    account.balance()\n    assert account._balance == 100\n";

fn line(number: usize, new_side_line: usize, text: &str) -> crate::analysis::diff::ChangedLine {
    crate::analysis::diff::ChangedLine {
        line: number,
        new_side_line,
        text: text.to_string(),
    }
}

fn analyze(
    label: &str,
    source: &str,
    added: &[usize],
    removed: Vec<crate::analysis::diff::ChangedLine>,
) -> Result<Vec<Finding>, String> {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|err| format!("system time: {err}"))?
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "ripr-python-replaced-block-{label}-{}-{nanos}",
        std::process::id()
    ));
    for (path, contents) in [
        ("src/account.py", source),
        ("tests/test_account.py", ACCOUNT_TEST_PY),
    ] {
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
        path: PathBuf::from("src/account.py"),
        added_lines,
        removed_lines: removed,
    }];
    let result = PythonAdapter.analyze_diff(&options, &OraclePolicy::default(), &changed_files);
    let _ = std::fs::remove_dir_all(&root);
    let result = result?;
    assert_eq!(result.changed_files, 1, "the changed file must be analyzed");
    Ok(result.findings)
}

/// Candidate-side classes on one line. A deleted line no added line pairs
/// with (the `if DEBUG:` below) is base-side evidence on the same line, which
/// `deleted_lines_tests` covers.
fn classes_on(findings: &[Finding], number: usize) -> Vec<ExposureClass> {
    findings
        .iter()
        .filter(|finding| finding.source_currentness.permits_candidate_action())
        .filter(|finding| finding.probe.location.line == number)
        .map(|finding| finding.class.clone())
        .collect()
}

/// The one-line control: the return pairs with its old line by coordinate,
/// so only `max` is delta and the test reading `_balance` earns no credit.
#[test]
fn one_line_clamp_reads_weakly_exposed() -> Result<(), String> {
    let findings = analyze(
        "one-line",
        ACCOUNT_PY,
        &[7],
        vec![line(7, 7, "        return self._balance")],
    )?;
    assert_eq!(
        classes_on(&findings, 7),
        vec![ExposureClass::WeaklyExposed],
        "{findings:?}"
    );
    Ok(())
}

/// The same clamp as the second line of an even comment-plus-return block
/// reads the same as the one-line edit. Pairing on the shared coordinate
/// left the return without an old line, so every token, `_balance`
/// included, read as delta and the test earned a false `exposed`.
#[test]
fn even_block_second_line_pairs_its_own_old_line() -> Result<(), String> {
    let findings = analyze(
        "even-block",
        ACCOUNT_PY,
        &[6, 7],
        vec![
            line(6, 6, "        # the amount on record"),
            line(7, 6, "        return self._balance"),
        ],
    )?;
    assert_eq!(
        classes_on(&findings, 7),
        vec![ExposureClass::WeaklyExposed],
        "{findings:?}"
    );
    Ok(())
}

/// An uneven block (one comment line becomes two) pairs the return by
/// content with the old return.
#[test]
fn uneven_block_pairs_the_return_by_content() -> Result<(), String> {
    let source = ACCOUNT_PY.replacen(
        "        # the amount on record, never below zero\n",
        "        # the amount on record,\n        # never below zero\n",
        1,
    );
    let findings = analyze(
        "uneven-block",
        &source,
        &[6, 7, 8],
        vec![
            line(6, 6, "        # the amount on record"),
            line(7, 6, "        return self._balance"),
        ],
    )?;
    assert_eq!(
        classes_on(&findings, 8),
        vec![ExposureClass::WeaklyExposed],
        "{findings:?}"
    );
    Ok(())
}

/// A new `def` on a later line of an even block pairs by position with an
/// unrelated old line. That partner does not look like the header, so the
/// new-declaration guard still holds and the header is not probed.
#[test]
fn new_def_paired_with_an_unlike_line_is_not_probed() -> Result<(), String> {
    let source = format!("{ACCOUNT_PY}\n\n# helpers\ndef double(x):\n    return x * 2\n");
    let findings = analyze(
        "new-def-in-block",
        &source,
        &[10, 11, 12],
        vec![
            line(10, 10, "# old helpers"),
            line(11, 10, "RATE = 2"),
            line(12, 10, "SCALE = 3"),
        ],
    )?;
    assert_eq!(
        classes_on(&findings, 11),
        Vec::<ExposureClass>::new(),
        "{findings:?}"
    );
    assert_eq!(
        classes_on(&findings, 12).len(),
        1,
        "the new body line stays the probe: {findings:?}"
    );
    Ok(())
}

/// A renamed method whose header pairs by position with the old header that
/// looks like it is a changed signature, not a new declaration: it keeps its
/// probe even though the old header names a different owner.
#[test]
fn renamed_def_paired_with_a_look_alike_header_keeps_its_probe() -> Result<(), String> {
    let source = ACCOUNT_PY.replacen("def balance(self)", "def current_balance(self)", 1);
    let findings = analyze(
        "renamed-def",
        &source,
        &[5, 6, 7],
        vec![
            line(5, 5, "    def balance(self):"),
            line(6, 5, "        # the amount on record"),
            line(7, 5, "        return self._balance"),
        ],
    )?;
    assert_eq!(classes_on(&findings, 5).len(), 1, "{findings:?}");
    Ok(())
}

/// Review of #7600: a rename that shares almost no tokens with the old header
/// (`def balance(self):` -> `def amount_on_record(this):`) fails the look-alike
/// test, but an old `def` header in its place still marks a changed header.
#[test]
fn renamed_def_paired_with_an_unlike_old_header_keeps_its_probe() -> Result<(), String> {
    let source = ACCOUNT_PY
        .replacen("def balance(self):", "def amount_on_record(this):", 1)
        .replacen(
            "return max(0, self._balance)",
            "return max(0, this._balance)",
            1,
        );
    let findings = analyze(
        "renamed-def-unlike",
        &source,
        &[5, 6, 7],
        vec![
            line(5, 5, "    def balance(self):"),
            line(6, 5, "        # the amount on record"),
            line(7, 5, "        return self._balance"),
        ],
    )?;
    assert_eq!(classes_on(&findings, 5).len(), 1, "{findings:?}");
    Ok(())
}

/// Content pairing compares trimmed text, so a module-scope annotation that
/// leaves an `if` block pairs with its indented old self. Moving out of the
/// block is behavior, so the annotation-only guard must not suppress it.
#[test]
fn dedented_annotation_change_keeps_its_probe() -> Result<(), String> {
    let source = format!("{ACCOUNT_PY}\n\nLIMIT: str = 5\n");
    let findings = analyze(
        "dedented-annotation",
        &source,
        &[10],
        vec![
            line(10, 10, "if DEBUG:"),
            line(11, 10, "    LIMIT: int = 5"),
        ],
    )?;
    assert_eq!(classes_on(&findings, 10).len(), 1, "{findings:?}");
    Ok(())
}

/// A new `def` that takes the place of an old decorator line is a changed
/// declaration, not a new one, so its header keeps its probe.
#[test]
fn def_paired_with_an_old_decorator_keeps_its_probe() -> Result<(), String> {
    let source = format!("{ACCOUNT_PY}\n\n# helpers\ndef double(x):\n    return x * 2\n");
    let findings = analyze(
        "def-over-decorator",
        &source,
        &[10, 11, 12],
        vec![
            line(10, 10, "# helpers"),
            line(11, 10, "@cache"),
            line(12, 10, "RATE = 2"),
        ],
    )?;
    assert_eq!(classes_on(&findings, 11).len(), 1, "{findings:?}");
    Ok(())
}
