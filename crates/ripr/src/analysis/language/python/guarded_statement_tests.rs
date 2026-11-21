//! Tests for the guarded-arm gate on lines that are not a `return`: a local
//! assignment, a call statement, and the continuation line of a wrapped call
//! inside an `if` arm no observed input enters never run under the suite, so
//! a strong exact-value oracle on another arm does not discriminate them.

use super::*;
use std::path::PathBuf;

/// `humanize` only reaches the `months` arm when `months >= 1`.
const HUMANIZE_PY: &str = concat!(
    "LOG = []\n",
    "\n",
    "\n",
    "def describe(kind, n):\n",
    "    return f\"{n} {kind}\"\n",
    "\n",
    "\n",
    "def humanize(diff, months):\n",
    "    if diff < 10:\n",
    "        return \"just now\"\n",
    "    elif months >= 1 and diff < 1000:\n",
    "        m = 2 * months\n",
    "        LOG.append(m)\n",
    "        label = \"months\"\n",
    "        text = describe(\n",
    "            label, m\n",
    "        )\n",
    "        return text\n",
    "    else:\n",
    "        return describe(\"years\", diff)\n",
);

const ASSIGNMENT: usize = 12;
const CALL_STATEMENT: usize = 13;
const STRING_ASSIGNMENT: usize = 14;
const CONTINUATION: usize = 16;
const GUARDED_LINES: [usize; 4] = [ASSIGNMENT, CALL_STATEMENT, STRING_ASSIGNMENT, CONTINUATION];

/// Every call takes the first arm; none enters the `months` arm.
const OFF_ARM_TESTS: &str = concat!(
    "from src.subject import humanize\n",
    "\n",
    "\n",
    "def test_now():\n",
    "    assert humanize(1, 0) == \"just now\"\n",
    "\n",
    "\n",
    "def test_still_now():\n",
    "    assert humanize(5, 0) == \"just now\"\n",
);

/// One call enters the `months` arm and pins its result.
const ON_ARM_TESTS: &str = concat!(
    "from src.subject import humanize\n",
    "\n",
    "\n",
    "def test_now():\n",
    "    assert humanize(1, 0) == \"just now\"\n",
    "\n",
    "\n",
    "def test_months():\n",
    "    assert humanize(500, 2) == \"4 months\"\n",
);

fn analyze(
    label: &str,
    source: &str,
    tests: &str,
    added: &[usize],
) -> Result<Vec<Finding>, String> {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|err| format!("system time: {err}"))?
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "ripr-python-guarded-statement-{label}-{}-{nanos}",
        std::process::id()
    ));
    for (path, contents) in [("src/subject.py", source), ("tests/test_subject.py", tests)] {
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
        added_lines.push(crate::analysis::diff::ChangedLine {
            line: *number,
            new_side_line: *number,
            text: text.to_string(),
        });
    }
    let changed_files = vec![ChangedFile {
        path: PathBuf::from("src/subject.py"),
        added_lines,
        removed_lines: Vec::new(),
    }];
    let result = PythonAdapter.analyze_diff(&options, &OraclePolicy::default(), &changed_files);
    let _ = std::fs::remove_dir_all(&root);
    let result = result?;
    assert_eq!(result.changed_files, 1, "the changed file must be analyzed");
    Ok(result.findings)
}

fn finding_on(findings: &[Finding], number: usize) -> Result<&Finding, String> {
    let mut on_line = findings
        .iter()
        .filter(|finding| finding.probe.location.line == number);
    match (on_line.next(), on_line.next()) {
        (Some(finding), None) => Ok(finding),
        _ => Err(format!(
            "expected one finding on line {number}: {findings:?}"
        )),
    }
}

/// The fixture is what the claims below assume: each probed line is the
/// statement named, and the continuation line sits deeper than its opener.
#[test]
fn fixture_lines_are_the_intended_statements() {
    let lines: Vec<&str> = HUMANIZE_PY.lines().collect();
    assert_eq!(lines[ASSIGNMENT - 1].trim(), "m = 2 * months");
    assert_eq!(lines[CALL_STATEMENT - 1].trim(), "LOG.append(m)");
    assert_eq!(lines[STRING_ASSIGNMENT - 1].trim(), "label = \"months\"");
    assert_eq!(lines[CONTINUATION - 2].trim(), "text = describe(");
    assert_eq!(lines[CONTINUATION - 1].trim(), "label, m");
}

#[test]
fn statements_in_an_arm_no_input_enters_are_weakly_exposed() -> Result<(), String> {
    let findings = analyze("off-arm", HUMANIZE_PY, OFF_ARM_TESTS, &GUARDED_LINES)?;
    for number in GUARDED_LINES {
        let finding = finding_on(&findings, number)?;
        assert_eq!(
            finding.class,
            ExposureClass::WeaklyExposed,
            "line {number}: {finding:?}"
        );
        assert!(
            finding
                .missing
                .iter()
                .any(|missing| missing.contains("months >= 1 and diff < 1000")),
            "line {number} must name the guard no input satisfies: {:?}",
            finding.missing
        );
    }
    Ok(())
}

/// The gate is about reach, not shape: once a strong call enters the arm the
/// same lines keep the oracle verdict.
#[test]
fn statements_in_an_arm_an_input_enters_keep_the_oracle_verdict() -> Result<(), String> {
    let findings = analyze("on-arm", HUMANIZE_PY, ON_ARM_TESTS, &GUARDED_LINES)?;
    for number in GUARDED_LINES {
        let finding = finding_on(&findings, number)?;
        assert_eq!(
            finding.class,
            ExposureClass::Exposed,
            "line {number}: {finding:?}"
        );
    }
    Ok(())
}

/// A class body has no parameters of its own. Its `if debug:` reads the
/// module global, never the `debug` the constructor is called with.
const CLASS_BODY_PY: &str = concat!(
    "debug = True\n",
    "\n",
    "\n",
    "def make(name):\n",
    "    return name.upper()\n",
    "\n",
    "\n",
    "class Tracker:\n",
    "    if debug:\n",
    "        LABEL = make(\"x\")\n",
    "\n",
    "    def __init__(self, debug):\n",
    "        self.debug = debug\n",
    "\n",
    "    def value(self):\n",
    "        return self.LABEL\n",
);

const CLASS_BODY_TESTS: &str = concat!(
    "from src.subject import Tracker\n",
    "\n",
    "\n",
    "def test_value():\n",
    "    assert Tracker(False).value() == \"X\"\n",
);

/// Scope control: the gate is not read for a class-body statement, so a
/// constructor argument named like a module global never excludes the row.
#[test]
fn class_body_statement_under_a_module_guard_is_not_gated() -> Result<(), String> {
    let line = 10;
    assert_eq!(
        CLASS_BODY_PY.lines().nth(line - 1).map(str::trim),
        Some("LABEL = make(\"x\")")
    );
    let findings = analyze("class-body", CLASS_BODY_PY, CLASS_BODY_TESTS, &[line])?;
    let finding = finding_on(&findings, line)?;
    assert!(
        !finding
            .missing
            .iter()
            .any(|missing| missing.contains("guarding arm")),
        "{:?}",
        finding.missing
    );
    Ok(())
}
