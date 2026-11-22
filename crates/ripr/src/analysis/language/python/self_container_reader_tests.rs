//! Tests for the recorded-container reader gate (#16010): a rewritten
//! `self._name.append(..)` is observed only by a test that reads the
//! container or calls a function that reads it, not by a pin on the result
//! of the recording method.

use super::*;
use std::path::PathBuf;

/// `tick` records into `_seen` and returns an unrelated sum. `count` reads
/// the container and `total` reaches it through `count`.
const CLOCK_PY: &str = concat!(
    "class Clock:\n",
    "    def __init__(self, hours):\n",
    "        self.hours = hours\n",
    "        self._seen = []\n",
    "\n",
    "    def tick(self, n):\n",
    "        self._seen.append(n + 1)\n",
    "        return self.hours + n\n",
    "\n",
    "    def count(self):\n",
    "        return len(self._seen)\n",
    "\n",
    "    def total(self):\n",
    "        return self.count() * 2\n",
);

const RECORD_LINE: usize = 7;

/// A test that pins the result of `tick`, then runs `after`.
fn tests_then(after: &str) -> String {
    format!(
        "from src.clock import Clock\n\n\ndef test_tick():\n    c = Clock(1)\n    assert c.tick(2) == 3\n{after}"
    )
}

fn analyze(label: &str, source: &str, tests: &str) -> Result<Finding, String> {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|err| format!("system time: {err}"))?
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "ripr-python-self-container-{label}-{}-{nanos}",
        std::process::id()
    ));
    for (path, contents) in [("src/clock.py", source), ("tests/test_clock.py", tests)] {
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
    let new_text = source
        .lines()
        .nth(RECORD_LINE - 1)
        .ok_or_else(|| "fixture has no record line".to_string())?;
    assert!(
        new_text.trim().ends_with(".append(n + 1)"),
        "fixture record line: {new_text}"
    );
    let changed = |text: String| crate::analysis::diff::ChangedLine {
        line: RECORD_LINE,
        new_side_line: RECORD_LINE,
        text,
    };
    let changed_files = vec![ChangedFile {
        path: PathBuf::from("src/clock.py"),
        added_lines: vec![changed(new_text.to_string())],
        removed_lines: vec![changed(new_text.replace("n + 1", "n"))],
    }];
    let result = PythonAdapter.analyze_diff(&options, &OraclePolicy::default(), &changed_files);
    let _ = std::fs::remove_dir_all(&root);
    let mut findings = result?.findings;
    findings.retain(|finding| finding.probe.location.line == RECORD_LINE);
    match findings.len() {
        1 => Ok(findings.remove(0)),
        _ => Err(format!(
            "expected one finding on the record line: {findings:?}"
        )),
    }
}

/// The only assertion pins the result of `tick`, which does not carry the
/// container.
#[test]
fn record_no_test_reads_is_weakly_exposed() -> Result<(), String> {
    let finding = analyze("unread", CLOCK_PY, &tests_then(""))?;
    assert_eq!(finding.class, ExposureClass::WeaklyExposed, "{finding:?}");
    assert!(
        finding
            .missing
            .iter()
            .any(|missing| missing.contains("`self._seen`")
                && missing.contains("`count`")
                && missing.contains("`total`")),
        "{:?}",
        finding.missing
    );
    Ok(())
}

/// A test that names the container, calls a reader, or calls a function
/// that reaches a reader keeps the oracle verdict.
#[test]
fn record_a_test_reads_keeps_the_oracle_verdict() -> Result<(), String> {
    for (label, after) in [
        ("named", "    assert c._seen == [3]\n"),
        ("reader", "    assert c.count() == 1\n"),
        ("caller", "    assert c.total() == 2\n"),
    ] {
        let finding = analyze(label, CLOCK_PY, &tests_then(after))?;
        assert_eq!(
            finding.class,
            ExposureClass::Exposed,
            "{label}: {finding:?}"
        );
    }
    Ok(())
}

/// Shapes whose readers cannot be listed from the file keep the existing
/// verdict. Each rewrites one line of the fixture.
#[test]
fn unreadable_shapes_keep_the_oracle_verdict() -> Result<(), String> {
    for (label, from, to) in [
        ("base", "class Clock:", "class Clock(Base):"),
        ("return-self", "return self.hours + n", "return self"),
        ("aliased-seed", "self._seen = []", "self._seen = hours"),
        (
            "owner-reads",
            "return self.hours + n",
            "return self.hours + len(self._seen)",
        ),
        (
            "owner-calls-reader",
            "return self.hours + n",
            "return self.hours + self.count()",
        ),
        ("public", "_seen", "seen"),
        ("dunder-reader", "def count(self):", "def __len__(self):"),
    ] {
        assert!(CLOCK_PY.contains(from), "{label}: fixture has no `{from}`");
        let source = CLOCK_PY.replace(from, to);
        let finding = analyze(label, &source, &tests_then(""))?;
        assert_eq!(
            finding.class,
            ExposureClass::Exposed,
            "{label}: {finding:?}"
        );
    }
    Ok(())
}

/// `class Clock(object):` has no base that could read the container.
#[test]
fn object_base_is_still_read() -> Result<(), String> {
    let source = CLOCK_PY.replace("class Clock:", "class Clock(object):");
    let finding = analyze("object-base", &source, &tests_then(""))?;
    assert_eq!(finding.class, ExposureClass::WeaklyExposed, "{finding:?}");
    Ok(())
}
