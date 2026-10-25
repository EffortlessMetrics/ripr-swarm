//! RIPR-SPEC-0238: assertion authority follows bindings, not method spelling.
use super::*;
use crate::analysis::diff::ChangedLine;
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT: AtomicUsize = AtomicUsize::new(0);

fn analyzed(test: &str, helper: &str) -> Result<Finding, String> {
    let root = std::env::temp_dir().join(format!(
        "ripr-python-assert-equal-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(root.join("tests")).map_err(|error| error.to_string())?;
    std::fs::write(
        root.join("shop.py"),
        "def fee(amount):\n    return amount + 2\n",
    )
    .map_err(|error| error.to_string())?;
    std::fs::write(root.join("tests/test_shop.py"), test).map_err(|error| error.to_string())?;
    std::fs::write(root.join("helpers.py"), helper).map_err(|error| error.to_string())?;
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
    let changed = ChangedFile {
        path: PathBuf::from("shop.py"),
        added_lines: vec![ChangedLine {
            line: 2,
            new_side_line: 2,
            text: "    return amount + 2".to_string(),
        }],
        removed_lines: vec![ChangedLine {
            line: 2,
            new_side_line: 2,
            text: "    return amount + 1".to_string(),
        }],
    };
    let result = PythonAdapter.analyze_diff(&options, &OraclePolicy::default(), &[changed]);
    let cleanup = std::fs::remove_dir_all(&root);
    let result = result?;
    cleanup.map_err(|error| error.to_string())?;
    assert!(result.limitations.is_empty(), "{:?}", result.limitations);
    assert_eq!(
        result.findings.len(),
        1,
        "one intended return-value subject"
    );
    let finding = result
        .findings
        .into_iter()
        .next()
        .ok_or("missing finding")?;
    assert_eq!(finding.probe.location.file, PathBuf::from("shop.py"));
    assert_eq!(finding.probe.location.line, 2);
    assert_eq!(finding.related_tests.len(), 1, "the owner remains reached");
    Ok(finding)
}

fn test_class(imports: &str, base: &str, body: &str) -> String {
    format!(
        "{imports}\nfrom shop import fee\n\nclass TestFee({base}):\n    def test_fee(self):\n{body}\n"
    )
}

#[test]
fn rebound_or_unrelated_assert_equal_cannot_promote() -> Result<(), String> {
    let bodies = [
        (
            "import unittest",
            "unittest.TestCase",
            "        assertEqual = self.assertEqual\n        assertEqual = lambda *args: None\n        assertEqual(fee(10), 12)",
        ),
        (
            "from unittest import TestCase\nfrom helpers import Fake\nTestCase = Fake",
            "TestCase",
            "        from helpers import Fake as TestCase\n        self.assertEqual(fee(10), 12)",
        ),
        (
            "import unittest",
            "unittest.TestCase",
            "        self.assertEqual = lambda *args: None\n        self.assertEqual(fee(10), 12)",
        ),
        (
            "import unittest\nfrom types import SimpleNamespace",
            "unittest.TestCase",
            "        self = SimpleNamespace(assertEqual=lambda *args: None)\n        self.assertEqual(fee(10), 12)",
        ),
        (
            "import unittest\nfrom helpers import noop as assertEqual",
            "unittest.TestCase",
            "        assertEqual(fee(10), 12)",
        ),
        (
            "import unittest\nfrom helpers import check_equal as assertEqual",
            "unittest.TestCase",
            "        assertEqual = lambda *args: None\n        assertEqual(fee(10), 12)",
        ),
        (
            "import unittest",
            "unittest.TestCase",
            "        self.assertEqual = lambda *args: True\n        assert self.assertEqual(fee(10), 12) and True",
        ),
    ];
    let mut failures = Vec::new();
    for (imports, base, body) in bodies {
        let source = test_class(imports, base, body);
        let finding = analyzed(
            &source,
            "def noop(*args):\n    pass\n\ndef check_equal(actual, expected):\n    assert actual == expected\n\nclass Fake:\n    @staticmethod\n    def assertEqual(*args):\n        pass\n",
        )?;
        println!(
            "assert_equal_observation {}",
            serde_json::json!({
                "classification": finding.class.as_str(),
                "oracle_strength": finding.related_tests[0].oracle_strength.as_str(),
                "probe": {"family": "return_value"},
                "test_source": source,
            })
        );
        if finding.class == ExposureClass::Exposed
            || finding.related_tests[0].oracle_strength == OracleStrength::Strong
        {
            failures.push(format!(
                "{source}\n{:?} {:?}",
                finding.class, finding.related_tests[0].oracle_strength
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "false assertion authority:\n{}",
        failures.join("\n")
    );
    Ok(())
}

#[test]
fn genuine_testcase_aliases_and_assertion_helpers_still_promote() -> Result<(), String> {
    let bodies = [
        (
            "import unittest",
            "unittest.TestCase",
            "        self.assertEqual(fee(10), 12)",
        ),
        (
            "import unittest",
            "unittest.TestCase",
            "        assertEqual = self.assertEqual\n        assertEqual(fee(10), 12)",
        ),
        (
            "import unittest as ut",
            "ut.TestCase",
            "        self.assertEqual(fee(10), 12)",
        ),
        (
            "from unittest import TestCase as Case",
            "Case",
            "        self.assertEqual(fee(10), 12)",
        ),
        (
            "import unittest\nfrom helpers import check_equal",
            "unittest.TestCase",
            "        check_equal(fee(10), 12)",
        ),
        (
            "import unittest\nfrom helpers import check_equal as assertEqual",
            "unittest.TestCase",
            "        assertEqual(fee(10), 12)",
        ),
    ];
    for (imports, base, body) in bodies {
        let source = test_class(imports, base, body);
        let finding = analyzed(
            &source,
            "def check_equal(actual, expected):\n    assert actual == expected\n",
        )?;
        assert_eq!(finding.class, ExposureClass::Exposed, "{source}");
        assert_eq!(
            finding.related_tests[0].oracle_strength,
            OracleStrength::Strong,
            "{source}"
        );
    }
    Ok(())
}

#[test]
fn visible_class_override_withholds_authority_but_ordinary_setup_does_not() -> Result<(), String> {
    let overridden = "import unittest\nfrom shop import fee\n\nclass TestFee(unittest.TestCase):\n    @staticmethod\n    def assertEqual(first, second):\n        return None\n    def test_fee(self):\n        self.assertEqual(fee(10), 12)\n";
    let finding = analyzed(overridden, "")?;
    assert_ne!(finding.class, ExposureClass::Exposed);
    assert_ne!(
        finding.related_tests[0].oracle_strength,
        OracleStrength::Strong
    );
    let inherited = "import unittest\nfrom shop import fee\n\nclass Base(unittest.TestCase):\n    def setUp(self):\n        self.marker = 1\n\nclass TestFee(Base):\n    def test_fee(self):\n        self.assertEqual(fee(10), 12)\n";
    let finding = analyzed(inherited, "")?;
    assert_eq!(finding.class, ExposureClass::Exposed);
    Ok(())
}

#[test]
fn unrelated_helper_and_standard_module_captures_preserve_authority() -> Result<(), String> {
    let cases = [
        "import unittest\nfrom shop import fee\n\nclass TestFee(unittest.TestCase):\n    def assert_unused(self):\n        pass\n    def test_fee(self):\n        self.assertEqual(fee(10), 12)\n",
        "import unittest\nfrom shop import fee\n\ncase = unittest.TestCase()\nassertEqual = case.assertEqual\n\nclass TestFee(unittest.TestCase):\n    def test_fee(self):\n        assertEqual(fee(10), 12)\n",
        "from unittest import TestCase as Case\nfrom shop import fee\n\nassertEqual = Case().assertEqual\n\nclass TestFee(Case):\n    def test_fee(self):\n        assertEqual(fee(10), 12)\n",
    ];
    for source in cases {
        let finding = analyzed(source, "")?;
        assert_eq!(finding.class, ExposureClass::Exposed, "{source}");
        assert_eq!(
            finding.related_tests[0].oracle_strength,
            OracleStrength::Strong
        );
    }
    Ok(())
}

#[test]
fn replaced_module_capture_or_equality_dispatch_stays_unresolved() -> Result<(), String> {
    let cases = [
        "import unittest\nfrom shop import fee\nfrom helpers import Fake\n\ncase = Fake()\nassertEqual = case.assertEqual\n\nclass TestFee(unittest.TestCase):\n    def test_fee(self):\n        assertEqual(fee(10), 12)\n",
        "import unittest\nfrom shop import fee\nfrom helpers import noop\n\ncase = unittest.TestCase()\nassertEqual = case.assertEqual\nassertEqual = noop\n\nclass TestFee(unittest.TestCase):\n    def test_fee(self):\n        assertEqual(fee(10), 12)\n",
        "import unittest\nfrom shop import fee\nfrom helpers import noop\n\ncase = unittest.TestCase()\ncase.assertEqual = noop\nassertEqual = case.assertEqual\n\nclass TestFee(unittest.TestCase):\n    def test_fee(self):\n        assertEqual(fee(10), 12)\n",
        "import unittest\nfrom shop import fee\n\nclass TestFee(unittest.TestCase):\n    def test_fee(self):\n        self._type_equality_funcs[int] = lambda *args, **kwargs: None\n        self.assertEqual(fee(10), 12)\n",
    ];
    for source in cases {
        let finding = analyzed(
            source,
            "def noop(*args, **kwargs):\n    pass\n\nclass Fake:\n    assertEqual = staticmethod(noop)\n",
        )?;
        assert_ne!(finding.class, ExposureClass::Exposed, "{source}");
        assert_ne!(
            finding.related_tests[0].oracle_strength,
            OracleStrength::Strong
        );
    }
    Ok(())
}
