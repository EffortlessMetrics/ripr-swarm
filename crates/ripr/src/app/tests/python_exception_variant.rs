//! Exception credit must survive real workspace facts and their warm cache.
//! Import configuration remains part of the identity proof, not a test bypass.

use crate::domain::{ExposureClass, Finding, OracleKind, OracleStrength, StageState};
use crate::output::python_repair_card::python_repair_card;
use std::fs;
use std::path::Path;

const SOURCE: &str = r"class EmptyError(Exception): pass
class TooLongError(Exception): pass

def parse(s):
    if not s:
        raise EmptyError()
    if len(s) > 10:
        raise TooLongError()
    return len(s)
";
const TEST: &str = "import pytest\nfrom fx.core import parse, TooLongError\n\ndef test_parse():\n    with pytest.raises(TooLongError):\n        parse('abcdefghijklmnop')\n";
const DIFF: &str = r"diff --git a/fx/core.py b/fx/core.py
index 1111111..2222222 100644
--- a/fx/core.py
+++ b/fx/core.py
@@ -5,5 +5,5 @@ def parse(s):
     if not s:
         raise EmptyError()
     if len(s) > 10:
-        raise EmptyError()
+        raise TooLongError()
     return len(s)
";
const OWN_PATH: &str = "[tool.pytest.ini_options]\npythonpath = [\".\"]\n";
const OTHER_PATH: &str = "[tool.pytest.ini_options]\npythonpath = [\"src\"]\n";
const FAKE_PYTEST: &str = "class _Raises:\n    def __enter__(self): return self\n    def __exit__(self, exc_type, exc, traceback): return True\n\ndef raises(expected):\n    return _Raises()\n";

fn run_check(root: &Path) -> Result<crate::CheckOutput, String> {
    let config =
        crate::config::tests_only_parse("[languages]\nenabled = [\"rust\", \"python\"]\n")?;
    crate::app::check_workspace_with_config(
        crate::CheckInput {
            root: root.to_path_buf(),
            base: None,
            diff_file: Some(root.join("diff.patch")),
            mode: crate::Mode::Draft,
            format: crate::OutputFormat::Json,
            include_unchanged_tests: true,
            perl_facts_path: None,
            suppression_policy: None,
            git_timeout: None,
            git_candidate: None,
            scope: Default::default(),
        },
        &config,
    )
}

fn changed_raise(output: &crate::CheckOutput) -> Result<&Finding, String> {
    let mut findings = output
        .findings
        .iter()
        .filter(|finding| finding.probe.location.file.ends_with("fx/core.py"));
    match (findings.next(), findings.next()) {
        (Some(finding), None) if !finding.related_tests.is_empty() => Ok(finding),
        _ => Err("the changed raise needs exactly one finding with a related test".to_string()),
    }
}

fn assert_variant_credit(finding: &Finding) {
    assert_eq!(finding.class, ExposureClass::Exposed, "{finding:?}");
    assert_eq!(finding.ripr.reveal.discriminate.state, StageState::Yes);
    assert_eq!(
        finding.alignment_reason.as_deref(),
        Some("strong_oracle_observes_changed_exception_variant")
    );
    assert!(finding.related_tests.iter().any(|test| {
        test.name == "test_parse"
            && test.oracle_kind == OracleKind::ExactErrorVariant
            && test.oracle_strength == OracleStrength::Strong
    }));
    assert!(finding.evidence.iter().any(|evidence| {
        evidence.starts_with("exception_variant_read:")
            && evidence.contains("rejects `EmptyError` and accepts `TooLongError`")
    }));
    assert!(python_repair_card(finding).is_none());
}

fn assert_no_variant_credit(finding: &Finding) {
    assert_eq!(finding.class, ExposureClass::WeaklyExposed, "{finding:?}");
    assert_ne!(
        finding.alignment_reason.as_deref(),
        Some("strong_oracle_observes_changed_exception_variant")
    );
    assert!(
        finding
            .evidence
            .iter()
            .all(|evidence| !evidence.starts_with("exception_variant_read:"))
    );
}

fn with_fixture(check: impl FnOnce(&Path) -> Result<(), String>) -> Result<(), String> {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|error| format!("system time: {error}"))?
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "ripr-python-exception-variant-{}-{stamp}",
        std::process::id()
    ));
    let proof = (|| -> Result<(), String> {
        for directory in ["fx", "tests", "src"] {
            fs::create_dir_all(root.join(directory))
                .map_err(|error| format!("create fixture directory: {error}"))?;
        }
        for (file, source) in [
            ("fx/__init__.py", ""),
            ("fx/core.py", SOURCE),
            ("tests/test_core.py", TEST),
            ("diff.patch", DIFF),
            ("pyproject.toml", OWN_PATH),
        ] {
            fs::write(root.join(file), source)
                .map_err(|error| format!("write fixture {file}: {error}"))?;
        }
        check(&root)
    })();
    let cleanup = fs::remove_dir_all(&root)
        .map_err(|error| format!("remove fixture {}: {error}", root.display()));
    proof?;
    cleanup
}

#[test]
fn check_output_python_exception_variant_preserves_import_environment_guards() -> Result<(), String>
{
    with_fixture(|root| {
        // Flat layout imports under python -m pytest without an unscanned
        // root. The second run exercises the same persisted file facts.
        for _ in 0..2 {
            let output = run_check(root)?;
            assert_variant_credit(changed_raise(&output)?);
        }
        for (argument, credited) in [("éééééééé", false), ("ééééééééééé", true)]
        {
            fs::write(
                root.join("tests/test_core.py"),
                TEST.replace("abcdefghijklmnop", argument),
            )
            .map_err(|error| format!("write Unicode test: {error}"))?;
            let output = run_check(root)?;
            let finding = changed_raise(&output)?;
            if credited {
                assert_variant_credit(finding);
            } else {
                assert_no_variant_credit(finding);
            }
        }
        fs::write(root.join("tests/test_core.py"), TEST)
            .map_err(|error| format!("restore test fixture: {error}"))?;
        fs::write(root.join("pyproject.toml"), OTHER_PATH)
            .map_err(|error| format!("write unresolved import path: {error}"))?;
        let refused = run_check(root)?;
        assert_no_variant_credit(changed_raise(&refused)?);
        // Configuration is checked again even when Python file facts are warm.
        fs::write(root.join("pyproject.toml"), OWN_PATH)
            .map_err(|error| format!("restore own import path: {error}"))?;
        let restored = run_check(root)?;
        assert_variant_credit(changed_raise(&restored)?);
        Ok(())
    })
}

#[test]
fn check_output_python_exception_variant_rejects_framework_shadows() -> Result<(), String> {
    with_fixture(|root| {
        let baseline = run_check(root)?;
        let finding = changed_raise(&baseline)?;
        assert_variant_credit(finding);
        let gap = finding
            .canonical_gap
            .as_ref()
            .ok_or("exception fixture gap identity missing")?
            .id
            .clone();
        // Source shadows implement a raises context that accepts both old and
        // new errors. Compiled entries are presence controls, not executable
        // bytecode or extension fixtures; their identity cannot be established.
        for (relative, contents) in [
            ("pytest.py", FAKE_PYTEST),
            ("tests/pytest.py", FAKE_PYTEST),
            ("fx/pytest.py", FAKE_PYTEST),
            ("pytest/__init__.py", FAKE_PYTEST),
            ("pytest.pyc", "compiled pytest identity unavailable"),
            ("pytest.so", "compiled pytest identity unavailable"),
            ("pytest.pyd", "compiled pytest identity unavailable"),
            (
                "pytest/__init__.pyc",
                "compiled pytest identity unavailable",
            ),
            (
                "tests/pytest/__init__.pyc",
                "compiled pytest identity unavailable",
            ),
        ] {
            let shadow = root.join(relative);
            let parent = shadow.parent().ok_or("shadow path needs a parent")?;
            fs::create_dir_all(parent)
                .map_err(|error| format!("create framework shadow directory: {error}"))?;
            fs::write(&shadow, contents)
                .map_err(|error| format!("write framework shadow: {error}"))?;
            let output = run_check(root)?;
            let finding = changed_raise(&output)?;
            assert_no_variant_credit(finding);
            assert_eq!(
                finding.canonical_gap.as_ref().map(|gap| &gap.id),
                Some(&gap),
                "framework identity cannot change the edited behavior's gap identity"
            );
            fs::remove_file(&shadow)
                .map_err(|error| format!("remove framework shadow: {error}"))?;
            let restored = run_check(root)?;
            let finding = changed_raise(&restored)?;
            assert_variant_credit(finding);
            assert_eq!(
                finding.canonical_gap.as_ref().map(|gap| &gap.id),
                Some(&gap)
            );
        }
        Ok(())
    })
}
