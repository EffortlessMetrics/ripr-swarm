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
    run_check_with_config(root, "[languages]\nenabled = [\"rust\", \"python\"]\n")
}

fn run_check_with_config(root: &Path, text: &str) -> Result<crate::CheckOutput, String> {
    let config = crate::config::tests_only_parse(text)?;
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
    let finding = sole_finding(output)?;
    if finding.related_tests.is_empty() {
        return Err("the changed raise needs a related test".to_string());
    }
    Ok(finding)
}

fn sole_finding(output: &crate::CheckOutput) -> Result<&Finding, String> {
    let mut findings = output.findings.iter();
    match (findings.next(), findings.next()) {
        (Some(finding), None) => Ok(finding),
        _ => Err("the changed raise needs exactly one finding".to_string()),
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
    assert_no_variant_read(finding);
}

fn assert_no_variant_read(finding: &Finding) {
    assert_ne!(finding.class, ExposureClass::Exposed, "{finding:?}");
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

#[test]
fn check_output_python_exception_variant_admits_scanned_src_imports() -> Result<(), String> {
    with_fixture(|root| {
        fs::rename(root.join("fx"), root.join("src/fx"))
            .map_err(|error| format!("move fixture into src layout: {error}"))?;
        fs::write(
            root.join("diff.patch"),
            DIFF.replace("fx/core.py", "src/fx/core.py"),
        )
        .map_err(|error| format!("write src-layout diff: {error}"))?;
        fs::write(root.join("pyproject.toml"), OTHER_PATH)
            .map_err(|error| format!("write canonical src configuration: {error}"))?;
        // Both runs use the complete workspace producer; the second reads
        // warm Python facts without borrowing a manually supplied admission.
        for _ in 0..2 {
            let output = run_check(root)?;
            assert_variant_credit(changed_raise(&output)?);
        }
        for config in [
            r#"[tool.pytest.ini_options]
pythonpath = ["src", "."]
"#,
            r#"[tool.pytest.ini_options]
pythonpath = ["../src"]
"#,
            r#"[tool.pytest.ini_options]
pythonpath = ["./src"]
"#,
            r#"[tool.pytest.ini_options]
pythonpath = ["src/"]
"#,
            r#"[tool.pytest.ini_options]
pythonpath = ["outside"]
"#,
            r#"[tool.pytest.ini_options]
pythonpath = "src"
"#,
            r#"[tool.pytest.ini_options]
python_paths = ["src"]
"#,
            r#"[tool.pytest]
pythonpath = ["src"]
"#,
            r#"[tool.pytest.ini_options]
"python\u0070ath" = ["src"]
"#,
            r#"[tool.pytest.ini_options]
pythonpath = ["src"]
addopts = "-q"
"#,
        ] {
            fs::write(root.join("pyproject.toml"), config)
                .map_err(|error| format!("write refused import configuration: {error}"))?;
            let refused = run_check(root)?;
            assert_no_variant_credit(changed_raise(&refused)?);
        }
        fs::write(root.join("pyproject.toml"), OTHER_PATH)
            .map_err(|error| format!("restore canonical src configuration: {error}"))?;
        let restored = run_check(root)?;
        assert_variant_credit(changed_raise(&restored)?);
        for path in ["pytest.ini", "tests/pytest.ini"] {
            fs::write(root.join(path), "[pytest]\n")
                .map_err(|error| format!("write competing configuration: {error}"))?;
            let refused = run_check(root)?;
            assert_no_variant_credit(changed_raise(&refused)?);
            fs::remove_file(root.join(path))
                .map_err(|error| format!("remove competing configuration: {error}"))?;
        }
        for path in ["src/pytest.py", "src/pytest/__init__.py", "src/pytest.pyc"] {
            let shadow = root.join(path);
            fs::create_dir_all(shadow.parent().ok_or("shadow needs a parent")?)
                .map_err(|error| format!("create src framework directory: {error}"))?;
            fs::write(&shadow, FAKE_PYTEST)
                .map_err(|error| format!("write src framework shadow: {error}"))?;
            let refused = run_check(root)?;
            assert_no_variant_credit(changed_raise(&refused)?);
            fs::remove_file(&shadow)
                .map_err(|error| format!("remove src framework shadow: {error}"))?;
        }
        fs::write(root.join("src/fx/__init__.py"), "MARKER = 1\n")
            .map_err(|error| format!("write active package initializer: {error}"))?;
        let refused = run_check(root)?;
        assert_no_variant_credit(changed_raise(&refused)?);
        fs::write(root.join("src/fx/__init__.py"), "")
            .map_err(|error| format!("restore inert package initializer: {error}"))?;
        for imports in [
            "from src.fx.core import parse\nfrom fx.core import TooLongError",
            "from fx.core import parse\nfrom src.fx.core import TooLongError",
        ] {
            fs::write(
                root.join("tests/test_core.py"),
                TEST.replace("from fx.core import parse, TooLongError", imports),
            )
            .map_err(|error| format!("write mixed module imports: {error}"))?;
            let refused = run_check(root)?;
            assert_no_variant_credit(changed_raise(&refused)?);
        }
        fs::write(root.join("tests/test_core.py"), TEST)
            .map_err(|error| format!("restore same-module imports: {error}"))?;
        let restored = run_check(root)?;
        assert_variant_credit(changed_raise(&restored)?);
        Ok(())
    })
}

#[test]
fn check_output_python_import_identity_checks_excluded_module_sources() -> Result<(), String> {
    with_fixture(|root| {
        fs::rename(root.join("fx/core.py"), root.join("src/core.py"))
            .map_err(|error| format!("move direct module into src: {error}"))?;
        fs::write(
            root.join("diff.patch"),
            DIFF.replace("fx/core.py", "src/core.py"),
        )
        .map_err(|error| format!("write direct-module diff: {error}"))?;
        fs::write(root.join("pyproject.toml"), OTHER_PATH)
            .map_err(|error| format!("write direct-module configuration: {error}"))?;
        for (module, shadow_path) in [("core", "tests/core.py"), ("src.core", "tests/src/core.py")]
        {
            let test = TEST.replace("from fx.core import", &format!("from {module} import"));
            fs::write(root.join("tests/test_core.py"), test)
                .map_err(|error| format!("write direct-module import: {error}"))?;
            let before = run_check(root)?;
            let finding = changed_raise(&before)?;
            assert_variant_credit(finding);
            let gap = finding
                .canonical_gap
                .as_ref()
                .ok_or("direct-module gap identity missing")?
                .id
                .clone();
            let shadow = root.join(shadow_path);
            fs::create_dir_all(shadow.parent().ok_or("shadow needs a parent")?)
                .map_err(|error| format!("create excluded module directory: {error}"))?;
            fs::write(
                &shadow,
                r"class TooLongError(Exception): pass

def parse(s):
    raise TooLongError()
",
            )
            .map_err(|error| format!("write excluded module: {error}"))?;
            let config = format!(
                r#"[languages]
enabled = ["python"]
[languages.python]
excluded_file_patterns = ["{shadow_path}"]
"#
            );
            // The competing file is absent from ForeignIndex. Only physical
            // lookup of every admitted spelling can see this source shadow.
            let refused = run_check_with_config(root, &config)?;
            let finding = changed_raise(&refused)?;
            assert_no_variant_credit(finding);
            assert_eq!(
                finding.canonical_gap.as_ref().map(|gap| &gap.id),
                Some(&gap)
            );
            fs::remove_file(&shadow).map_err(|error| format!("remove excluded module: {error}"))?;
            let restored = run_check_with_config(root, &config)?;
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

#[test]
fn check_output_python_import_identity_rejects_non_module_paths() -> Result<(), String> {
    for (path, import) in [
        ("fx.core.py", "fx.core"),
        ("fx.extra/core.py", "fx.extra.core"),
        ("src/sys.py", "sys"),
    ] {
        with_fixture(|root| {
            let target = root.join(path);
            fs::create_dir_all(target.parent().ok_or("module needs a parent")?)
                .map_err(|error| format!("create module path control: {error}"))?;
            fs::rename(root.join("fx/core.py"), &target)
                .map_err(|error| format!("move module path control: {error}"))?;
            fs::remove_file(root.join("fx/__init__.py"))
                .map_err(|error| format!("remove old package initializer: {error}"))?;
            fs::remove_dir(root.join("fx"))
                .map_err(|error| format!("remove old package directory: {error}"))?;
            if path == "fx.extra/core.py" {
                fs::write(root.join("fx.extra/__init__.py"), "")
                    .map_err(|error| format!("write dotted-directory initializer: {error}"))?;
            }
            if path == "src/sys.py" {
                fs::write(root.join("pyproject.toml"), OTHER_PATH)
                    .map_err(|error| format!("write builtin-shadow configuration: {error}"))?;
            }
            fs::write(root.join("diff.patch"), DIFF.replace("fx/core.py", path))
                .map_err(|error| format!("write module path diff: {error}"))?;
            fs::write(
                root.join("tests/test_core.py"),
                TEST.replace("from fx.core import", &format!("from {import} import")),
            )
            .map_err(|error| format!("write module path import: {error}"))?;
            let refused = run_check(root)?;
            let finding = sole_finding(&refused)?;
            assert_eq!(finding.probe.location.file, Path::new(path));
            assert_eq!(finding.probe.location.line, 8);
            assert_no_variant_read(finding);
            Ok(())
        })?;
    }
    Ok(())
}
