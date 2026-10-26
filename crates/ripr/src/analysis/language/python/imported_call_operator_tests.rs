//! An operator swap around a call to an imported workspace function (#9619):
//! credited only through an exact assertion the new operator provably fails,
//! and only for certain bindings.

use super::*;
use crate::domain::ExposureClass;
use std::path::{Path, PathBuf};

type Check = Result<(), String>;

const TABLE: &str = "RATE_TABLE = {\"gold\": 10, \"silver\": 5}\n\n\ndef lookup(tier):\n    return RATE_TABLE.get(tier, 0)\n";
const QUOTE_OLD: &str = "    return lookup(tier) * units";
const QUOTE_NEW: &str = "    return lookup(tier) + units";
const TEST: &str = "from shapes.rates.quote import quote\n\n\ndef test_quote_gold():\n    assert quote(\"gold\", 3) == 30\n";

fn quote_source(body: &str) -> String {
    format!("from .table import lookup\n\n\ndef quote(tier, units):\n{body}\n")
}

fn write_file(path: &Path, contents: &str) -> Check {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|err| format!("create_dir_all({}): {err}", parent.display()))?;
    }
    std::fs::write(path, contents).map_err(|err| format!("write({}): {err}", path.display()))
}

/// The finding for the one-line edit of line 5 of `shapes/rates/quote.py`
/// (`old` -> `new`) in a workspace of `files`.
fn classify(files: &[(&str, &str)], old: &str, new: &str) -> Result<Finding, String> {
    classify_with(files, old, new, Vec::new())
}

/// [`classify`] with more files in the same diff.
fn classify_with(
    files: &[(&str, &str)],
    old: &str,
    new: &str,
    extra: Vec<ChangedFile>,
) -> Result<Finding, String> {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|err| format!("system time: {err}"))?
        .as_nanos();
    let root =
        std::env::temp_dir().join(format!("ripr-imported-call-{}-{nanos}", std::process::id()));
    for (path, text) in files {
        write_file(&root.join(path), text)?;
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
    let mut changed = vec![ChangedFile {
        path: PathBuf::from("shapes/rates/quote.py"),
        added_lines: vec![crate::analysis::diff::ChangedLine {
            line: 5,
            new_side_line: 5,
            text: new.to_string(),
        }],
        removed_lines: vec![crate::analysis::diff::ChangedLine {
            line: 5,
            new_side_line: 5,
            text: old.to_string(),
        }],
    }];
    changed.extend(extra);
    let result = PythonAdapter.analyze_diff(&options, &OraclePolicy::default(), &changed);
    let cleanup = std::fs::remove_dir_all(&root);
    let result = result?;
    cleanup.map_err(|err| format!("remove_dir_all({}): {err}", root.display()))?;
    let [finding] = result.findings.as_slice() else {
        return Err(format!(
            "expected one finding, got {}",
            result.findings.len()
        ));
    };
    Ok(finding.clone())
}

fn workspace<'a>(quote: &'a str, test: &'a str) -> Vec<(&'a str, &'a str)> {
    workspace_with(TABLE, quote, test)
}

fn workspace_with<'a>(table: &'a str, quote: &'a str, test: &'a str) -> Vec<(&'a str, &'a str)> {
    vec![
        ("shapes/__init__.py", ""),
        ("shapes/rates/__init__.py", ""),
        ("shapes/rates/table.py", table),
        ("shapes/rates/quote.py", quote),
        ("tests/test_rates.py", test),
    ]
}

/// The finding for `QUOTE_NEW` in `quote` with the given workspace pieces.
fn quote_finding(files: &[(&str, &str)]) -> Result<Finding, String> {
    classify(files, QUOTE_OLD, QUOTE_NEW)
}

fn withheld(finding: &Finding, label: &str) {
    assert_ne!(
        finding.class,
        ExposureClass::Exposed,
        "{label}: {finding:?}"
    );
    assert!(
        !finding
            .evidence
            .iter()
            .any(|line| line.starts_with("related_test_relation: module_value_call")),
        "{label}: {:?}",
        finding.evidence
    );
}

#[test]
fn a_pinned_operator_swap_around_an_imported_call_is_credited() -> Check {
    let source = quote_source(QUOTE_NEW);
    let finding = classify(&workspace(&source, TEST), QUOTE_OLD, QUOTE_NEW)?;
    assert_eq!(finding.class, ExposureClass::Exposed, "{finding:?}");
    assert!(
        finding
            .evidence
            .iter()
            .any(|line| line.starts_with("related_test_relation: module_value_call")),
        "{:?}",
        finding.evidence
    );
    Ok(())
}

#[test]
fn an_assertion_both_operators_satisfy_is_not_credited() -> Check {
    // lookup("gold") is 2 and units is 2: 2 * 2 == 2 + 2 == 4.
    let table =
        "RATE_TABLE = {\"gold\": 2}\n\n\ndef lookup(tier):\n    return RATE_TABLE.get(tier, 0)\n";
    let source = quote_source(QUOTE_NEW);
    let test = TEST.replace("quote(\"gold\", 3) == 30", "quote(\"gold\", 2) == 4");
    let finding = quote_finding(&workspace_with(table, &source, &test))?;
    withheld(&finding, "both operators give 4");
    Ok(())
}

#[test]
fn an_assertion_that_does_not_pin_the_call_is_not_credited() -> Check {
    let source = quote_source(QUOTE_NEW);
    for (label, assertion) in [
        ("not an equality", "quote(\"gold\", 3) >= 30"),
        ("too many arguments", "quote(\"gold\", 3, 9) == 30"),
        ("non-literal argument", "quote(\"gold\", units) == 30"),
        ("non-integer expected value", "quote(\"gold\", 3) == 30.0"),
        ("string expected value", "quote(\"gold\", 3) == \"30\""),
        ("too few arguments", "quote(\"gold\") == 30"),
        ("keyword argument", "quote(\"gold\", units=3) == 30"),
        ("another function", "other(\"gold\", 3) == 30"),
    ] {
        let test = TEST.replace("quote(\"gold\", 3) == 30", assertion);
        let finding = quote_finding(&workspace(&source, &test))?;
        withheld(&finding, label);
    }
    Ok(())
}

#[test]
fn an_edit_that_is_more_than_an_operator_swap_is_not_credited() -> Check {
    for (label, new) in [
        ("another operand", "    return lookup(units) + units"),
        ("swapped sides", "    return units + lookup(tier)"),
        (
            "operator outside the set",
            "    return lookup(tier) // units",
        ),
        ("a second call", "    return lookup(tier) + lookup(units)"),
        (
            "an unrelated expression",
            "    return lookup(tier) + units + 1",
        ),
    ] {
        let source = quote_source(new);
        let finding = classify(&workspace(&source, TEST), QUOTE_OLD, new)?;
        withheld(&finding, label);
    }
    Ok(())
}

#[test]
fn an_edit_that_moves_one_operand_of_the_same_length_is_not_credited() -> Check {
    // The same-length operands keep every source range equal, so only the
    // structure of the call, the other operand and their sides can tell.
    for (label, old, new) in [
        (
            "another call argument",
            "    return lookup(tier) * units",
            "    return lookup(kind) + units",
        ),
        (
            "another other operand",
            "    return lookup(tier) * units",
            "    return lookup(tier) + extra",
        ),
        (
            "swapped sides",
            "    return lookup(tier) - units",
            "    return units + lookup(tier)",
        ),
    ] {
        let source =
            format!("from .table import lookup\n\n\ndef quote(tier, kind, units, extra):\n{new}\n");
        let test = TEST.replace(
            "quote(\"gold\", 3) == 30",
            "quote(\"gold\", \"gold\", 3, 3) == 30",
        );
        let finding = classify(&workspace(&source, &test), old, new)?;
        withheld(&finding, label);
    }
    Ok(())
}

#[test]
fn a_callee_that_is_not_one_plain_workspace_function_is_not_credited() -> Check {
    let source = quote_source(QUOTE_NEW);
    let decorated = "from functools import cache\n\nRATE_TABLE = {\"gold\": 10}\n\n\n@cache\ndef lookup(tier):\n    return RATE_TABLE.get(tier, 0)\n";
    let finding = quote_finding(&workspace_with(decorated, &source, TEST))?;
    withheld(&finding, "decorated callee");
    let missing = "RATE_TABLE = {\"gold\": 10}\n";
    let finding = quote_finding(&workspace_with(missing, &source, TEST))?;
    withheld(&finding, "the module does not define the imported name");
    let unresolved = "from .elsewhere import lookup\n\n\ndef quote(tier, units):\n    return lookup(tier) + units\n";
    let finding = quote_finding(&workspace(unresolved, TEST))?;
    withheld(&finding, "the import resolves to no workspace module");
    let rebound = format!("{}\nlookup = other\n", quote_source(QUOTE_NEW));
    let finding = quote_finding(&workspace(&rebound, TEST))?;
    withheld(&finding, "the callee is rebound in the owner module");
    Ok(())
}

#[test]
fn a_rewritten_owner_or_callee_is_not_credited() -> Check {
    let source = quote_source(QUOTE_NEW);
    for (label, other) in [
        (
            "owner replaced in another file",
            "import shapes.rates.quote as q\nq.quote = lambda tier, units: 30\n",
        ),
        (
            "callee replaced in another file",
            "import shapes.rates.quote as q\nq.lookup = lambda tier: 27\n",
        ),
        (
            "setattr on the module",
            "import shapes.rates.quote as q\nsetattr(q, 'quote', lambda tier, units: 30)\n",
        ),
    ] {
        let mut files = workspace(&source, TEST);
        files.push(("tests/conftest.py", other));
        let finding = quote_finding(&files)?;
        withheld(&finding, label);
    }
    for (label, test) in [
        (
            "test rebinds the owner",
            "from shapes.rates.quote import quote\n\n\ndef test_quote_gold():\n    quote = other\n    assert quote(\"gold\", 3) == 30\n",
        ),
        (
            "test replaces the owner",
            "import shapes.rates.quote as q\nfrom shapes.rates.quote import quote\n\n\ndef test_quote_gold():\n    q.quote = other\n    assert quote(\"gold\", 3) == 30\n",
        ),
        (
            "test may skip",
            "from shapes.rates.quote import quote\n\n\ndef test_quote_gold(cond):\n    if cond:\n        return\n    assert quote(\"gold\", 3) == 30\n",
        ),
    ] {
        let finding = quote_finding(&workspace(&source, test))?;
        withheld(&finding, label);
    }
    Ok(())
}

#[test]
fn a_patched_owner_keeps_the_mocked_module_abstention() -> Check {
    let source = quote_source(QUOTE_NEW);
    let test = "from shapes.rates.quote import quote\n\n\ndef test_quote_gold(mocker):\n    mocker.patch(\"shapes.rates.quote.quote\", return_value=30)\n    assert quote(\"gold\", 3) == 30\n";
    let finding = quote_finding(&workspace(&source, test))?;
    withheld(&finding, "patched owner");
    Ok(())
}

#[test]
fn the_other_operand_may_be_an_expression_and_the_call_may_come_second() -> Check {
    // 3 + X == 13 fixes X = 10; 3 * 10 == 30 != 13.
    let old = "    return units + lookup(tier)";
    let new = "    return units * lookup(tier)";
    let source = quote_source(new);
    let test = TEST.replace("== 30", "== 13");
    let finding = classify(&workspace(&source, &test), old, new)?;
    assert_eq!(finding.class, ExposureClass::Exposed, "{finding:?}");
    // Subtraction either way round: X - 3 == 7 fixes X = 10.
    let old = "    return lookup(tier) - units";
    let new = "    return lookup(tier) + units";
    let source = quote_source(new);
    let test = TEST.replace("== 30", "== 7");
    let finding = classify(&workspace(&source, &test), old, new)?;
    assert_eq!(finding.class, ExposureClass::Exposed, "{finding:?}");
    Ok(())
}

#[test]
fn a_call_with_keywords_or_a_test_that_cannot_run_is_not_credited() -> Check {
    let old = "    return lookup(tier=tier) * units";
    let new = "    return lookup(tier=tier) + units";
    let source = quote_source(new);
    let finding = classify(&workspace(&source, TEST), old, new)?;
    withheld(&finding, "keyword argument in the imported call");

    let source = quote_source(QUOTE_NEW);
    let skipped = TEST.replace(
        "def test_quote_gold",
        "@pytest.mark.skip(reason=\"later\")\ndef test_quote_gold",
    );
    let skipped = format!("import pytest\n{skipped}");
    let finding = quote_finding(&workspace(&source, &skipped))?;
    withheld(&finding, "skipped test");

    // A test file edited in the same diff may assert the new value.
    let edited = ChangedFile {
        path: PathBuf::from("tests/test_rates.py"),
        added_lines: vec![crate::analysis::diff::ChangedLine {
            line: 5,
            new_side_line: 5,
            text: "    assert quote(\"gold\", 3) == 30".to_string(),
        }],
        removed_lines: vec![crate::analysis::diff::ChangedLine {
            line: 5,
            new_side_line: 5,
            text: "    assert quote(\"gold\", 3) == 13".to_string(),
        }],
    };
    let finding = classify_with(
        &workspace(&source, TEST),
        QUOTE_OLD,
        QUOTE_NEW,
        vec![edited],
    )?;
    withheld(&finding, "test edited in the same diff");
    Ok(())
}

#[test]
fn a_workspace_type_with_its_own_arithmetic_keeps_the_abstention() -> Check {
    // `lookup` may return an instance whose `__mul__` and `__add__` both give
    // 30, so the pinned value says nothing about the operator.
    let table = "class Rate:\n    def __mul__(self, units):\n        return 30\n\n    def __add__(self, units):\n        return 30\n\n\ndef lookup(tier):\n    return Rate()\n";
    let source = quote_source(QUOTE_NEW);
    let finding = quote_finding(&workspace_with(table, &source, TEST))?;
    withheld(&finding, "workspace type with __mul__ and __add__");
    Ok(())
}
