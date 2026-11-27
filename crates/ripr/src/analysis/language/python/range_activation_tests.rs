//! Whole-diff custody for combined range-header and body edits (#12948).
use super::*;
use crate::analysis::diff::parse_unified_diff;

fn analyze(source: &str, test: &str, patch: &str) -> Result<Vec<Finding>, String> {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|err| err.to_string())?
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "ripr-range-compound-{}-{stamp}",
        std::process::id()
    ));
    for (path, contents) in [
        ("src/subject.py", source),
        ("tests/test_subject.py", test),
        (
            "pyproject.toml",
            "[tool.pytest.ini_options]\npythonpath = ['src']\n",
        ),
    ] {
        let path = root.join(path);
        std::fs::create_dir_all(path.parent().ok_or("fixture parent")?)
            .map_err(|e| e.to_string())?;
        std::fs::write(path, contents).map_err(|e| e.to_string())?;
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
    let changed = parse_unified_diff(&format!(
        "--- a/src/subject.py\n+++ b/src/subject.py\n{patch}"
    ));
    assert_eq!(changed.len(), 1);
    let result = PythonAdapter.analyze_diff(&options, &OraclePolicy::default(), &changed);
    let _ = std::fs::remove_dir_all(&root);
    Ok(result?.findings)
}

#[test]
fn changed_range_combined_body_edits_withhold_every_finding() -> Result<(), String> {
    let source = "def sum_to(n):\n    acc = 0\n    for i in range(1, n):\n        acc += i\n    return acc\n";
    for (expected, patch) in [
        (
            6,
            "@@ -1,5 +1,5 @@\n def sum_to(n):\n     acc = 0\n-    for i in range(1, n + 1):\n-        acc += 0\n+    for i in range(1, n):\n+        acc += i\n     return acc\n",
        ),
        (
            3,
            "@@ -1,5 +1,5 @@\n def sum_to(n):\n     acc = 0\n-    for i in range(1, n + 1):\n+    for i in range(1, n):\n         acc += i\n-    return acc - n\n+    return acc\n",
        ),
    ] {
        let test = format!(
            "from src.subject import sum_to\n\ndef test_sum():\n    assert sum_to(3) == {expected}\n"
        );
        let findings = analyze(source, &test, patch)?;
        assert_eq!(findings.len(), 2, "{findings:?}");
        for finding in findings {
            assert_eq!(finding.class, ExposureClass::StaticUnknown, "{finding:?}");
            assert_ne!(finding.class, ExposureClass::WeaklyExposed);
            assert_eq!(finding.ripr.infect.state, StageState::Unknown);
        }
    }
    Ok(())
}

#[test]
fn changed_range_compound_context_withholds_removed_only_evidence() -> Result<(), String> {
    let source = "def sum_to(n):\n    acc = 0\n    for i in range(1, n):\n        acc += i\n    return acc\n";
    let patch = "@@ -1,6 +1,5 @@\n def sum_to(n):\n     acc = 0\n-    for i in range(1, n + 1):\n+    for i in range(1, n):\n         acc += i\n-    acc += 0\n     return acc\n";
    let tests = "from src.subject import sum_to\n\ndef test_sum():\n    assert sum_to(3) == 6\n";
    let findings = analyze(source, tests, patch)?;
    assert_eq!(findings.len(), 2, "{findings:?}");
    assert!(
        findings
            .iter()
            .any(|f| !f.source_currentness.permits_candidate_action())
    );
    assert!(
        findings
            .iter()
            .all(|f| f.class == ExposureClass::StaticUnknown),
        "{findings:?}"
    );
    Ok(())
}

#[test]
fn changed_range_compound_context_preserves_independent_owner_credit() -> Result<(), String> {
    let source = "def sum_to(n):\n    acc = 0\n    for i in range(1, n):\n        acc += i\n    return acc\n\ndef other():\n    return 8\n";
    let patch = "@@ -1,8 +1,8 @@\n def sum_to(n):\n     acc = 0\n-    for i in range(1, n + 1):\n+    for i in range(1, n):\n         acc += i\n     return acc\n \n def other():\n-    return 7\n+    return 8\n";
    let test = "from src.subject import sum_to, other\n\ndef test_sum():\n    assert sum_to(3) == 6\n\ndef test_other():\n    assert other() == 8\n";
    let findings = analyze(source, test, patch)?;
    assert_eq!(findings.len(), 2, "{findings:?}");
    assert!(
        findings
            .iter()
            .any(|f| f.probe.location.line == 3 && f.class == ExposureClass::StaticUnknown)
    );
    assert!(
        findings
            .iter()
            .any(|f| f.probe.location.line == 8 && f.class == ExposureClass::Exposed)
    );
    Ok(())
}

#[test]
fn changed_range_compound_ownership_uses_source_coordinates() -> Result<(), String> {
    use super::owners_tests::extract_owners;
    let file = Path::new("src/subject.py");
    let cases = [
        // A deleted loop owner must not lend its insertion point to other().
        (
            "def other():\n    return 8\n",
            "def gone(n):\n    for i in range(n):\n        pass\n\ndef other():\n    return 7\n",
            "@@ -1,6 +1,2 @@\n-def gone(n):\n-    for i in range(n):\n-        pass\n-\n def other():\n-    return 7\n+    return 8\n",
            Vec::<&str>::new(),
        ),
        // The new-side owner starts after inserted lines, unlike old coordinates.
        (
            "# inserted\n\ndef sum_to(n):\n    acc = 0\n    for i in range(n):\n        acc += i\n    return acc\n",
            "def sum_to(n):\n    acc = 0\n    for i in range(n + 1):\n        acc += 0\n    return acc\n",
            "@@ -1,5 +1,7 @@\n+# inserted\n+\n def sum_to(n):\n     acc = 0\n-    for i in range(n + 1):\n-        acc += 0\n+    for i in range(n):\n+        acc += i\n     return acc\n",
            vec!["sum_to"],
        ),
        // A nested helper owns its header; the outer return is independent.
        (
            "def outer():\n    def inner(n):\n        for i in range(n):\n            pass\n    return 8\n",
            "def outer():\n    def inner(n):\n        for i in range(n + 1):\n            pass\n    return 7\n",
            "@@ -1,5 +1,5 @@\n def outer():\n     def inner(n):\n-        for i in range(n + 1):\n+        for i in range(n):\n             pass\n-    return 7\n+    return 8\n",
            vec!["outer.inner"],
        ),
        // A multiline iterator's interior belongs to the native For header.
        (
            "def sum_to(n):\n    acc = 0\n    for i in range(\n        n\n    ):\n        acc += i\n    return acc\n",
            "def sum_to(n):\n    acc = 0\n    for i in range(\n        n + 1\n    ):\n        acc += 0\n    return acc\n",
            "@@ -1,7 +1,7 @@\n def sum_to(n):\n     acc = 0\n     for i in range(\n-        n + 1\n+        n\n     ):\n-        acc += 0\n+        acc += i\n     return acc\n",
            vec!["sum_to"],
        ),
        // A removed loop stays associated with its surviving old qualified owner.
        (
            "def sum_to(n):\n    return 3\n",
            "def sum_to(n):\n    for i in range(n):\n        pass\n    return 2\n",
            "@@ -1,4 +1,2 @@\n def sum_to(n):\n-    for i in range(n):\n-        pass\n-    return 2\n+    return 3\n",
            vec!["sum_to"],
        ),
    ];
    for (source, old, patch, expected) in cases {
        let changed = parse_unified_diff(&format!(
            "--- a/src/subject.py\n+++ b/src/subject.py\n{patch}"
        ));
        let owners = extract_owners(file, source);
        let old_owners = extract_owners(file, old);
        let affected = super::range_activation::compound_loop_owners(
            &changed[0],
            source,
            &owners,
            Some(old),
            &old_owners,
        );
        let names: Vec<_> = owners
            .iter()
            .filter(|owner| affected.contains(&owner.symbol_id()))
            .map(|owner| owner.qualified_name.as_str())
            .collect();
        assert_eq!(names, expected, "{patch}");
    }
    Ok(())
}

/// The module owner spans the whole file, so reading its loop headers once per
/// changed top-level line cost O(changed lines x file) (#16426). Every changed
/// line of one owner shares a single read of its span, and the shared read
/// still places a multiline iterator's interior inside its header.
#[test]
fn changed_top_level_lines_read_the_module_loop_headers_once() -> Result<(), String> {
    use super::owners_tests::extract_owners;
    let file = Path::new("src/subject.py");
    let statements: Vec<String> = (0..64).map(|index| format!("x{index} = {index}")).collect();
    let source = format!(
        "{}\nfor i in range(\n    3\n):\n    pass\n",
        statements.join("\n")
    );
    let line_count = source.lines().count();
    let patch = format!(
        "@@ -0,0 +1,{line_count} @@\n{}\n",
        source
            .lines()
            .map(|line| format!("+{line}"))
            .collect::<Vec<_>>()
            .join("\n")
    );
    let changed = parse_unified_diff(&format!(
        "--- a/src/subject.py\n+++ b/src/subject.py\n{patch}"
    ));
    assert_eq!(changed.len(), 1);
    assert_eq!(changed[0].added_lines.len(), line_count);
    let owners = extract_owners(file, &source);
    let module = owners
        .iter()
        .find(|owner| owner.name == "<module>")
        .ok_or("module owner")?;
    assert!(
        module.start_line == 1 && module.end_line >= line_count,
        "{module:?}"
    );
    let reads = || super::range_activation::OWNER_LOOP_HEADER_READS.with(std::cell::Cell::get);
    let before = reads();
    let affected =
        super::range_activation::compound_loop_owners(&changed[0], &source, &owners, None, &[]);
    assert!(affected.contains(&module.symbol_id()), "{affected:?}");
    let first = reads() - before;
    assert!(
        (1..=2).contains(&first),
        "{first} span reads for {line_count} changed lines"
    );
    // A second pass over the same source reuses the read entirely.
    let again =
        super::range_activation::compound_loop_owners(&changed[0], &source, &owners, None, &[]);
    assert_eq!(again, affected);
    assert_eq!(reads() - before, first);
    // The iterator's interior line (`    3`) is a header line; the body is not.
    let interior = statements.len() + 2;
    let body = statements.len() + 4;
    for (line, expected) in [(interior, true), (body, false), (1, false)] {
        let text = source.lines().nth(line - 1).ok_or("line")?;
        let limit = super::range_activation::loop_context_limit(
            module, &source, line, text, None, false, false,
        );
        assert_eq!(limit.is_some(), expected, "line {line}: {text}");
    }
    assert_eq!(reads() - before, first);
    Ok(())
}
