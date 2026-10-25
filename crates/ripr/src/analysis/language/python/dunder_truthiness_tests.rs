//! The integer truthiness slice of #9795, through the classifier.
use super::classify::{PythonNoBehaviorContext, classify_change_with_context};
use super::owners_tests::{extract_owners, extract_tests};
use super::related_tests::apply_class_facts;
use crate::domain::{ExposureClass, Finding};
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT_WORKSPACE: AtomicUsize = AtomicUsize::new(0);
struct TestWorkspace(std::path::PathBuf);
impl Drop for TestWorkspace {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

type Check = Result<(), String>;
const MONEY: &str = "class Money:\n    def __init__(self, cents):\n        self.cents = cents\n\n    def __bool__(self):\n        return self.cents > 0\n";
const OLD: &str = "        return self.cents != 0";
const NEW: &str = "        return self.cents > 0";

fn classify(source: &str, old: &str, test_source: &str, sole: bool) -> Result<Finding, String> {
    let file = Path::new("shapes/money.py");
    let test_file = Path::new("tests/test_money.py");
    let mut owners = extract_owners(file, source);
    apply_class_facts(&mut owners);
    let root = std::env::temp_dir().join(format!(
        "ripr-dunder-classifier-{}-{}",
        std::process::id(),
        NEXT_WORKSPACE.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(std::env::temp_dir()).map_err(|error| error.to_string())?;
    std::fs::create_dir(&root).map_err(|error| error.to_string())?;
    let workspace = TestWorkspace(root);
    let sources = std::collections::HashMap::from([
        (file.to_path_buf(), source.to_string()),
        (test_file.to_path_buf(), test_source.to_string()),
    ]);
    for (path, text) in &sources {
        let absolute = workspace.0.join(path);
        std::fs::create_dir_all(absolute.parent().ok_or("source needs parent")?)
            .map_err(|error| error.to_string())?;
        std::fs::write(absolute, text).map_err(|error| error.to_string())?;
    }
    super::dunder_truthiness::bind_workspace_observer(&workspace.0, &mut owners, &sources, false);
    for owner in &mut owners {
        if owner.is_module_owner() {
            owner.foreign_writes = super::module_value::ForeignWrites::scan(
                file,
                [(file, source), (test_file, test_source)].into_iter(),
                false,
            );
        }
    }
    let tests = extract_tests(test_file, test_source);
    assert!(!tests.is_empty(), "nonempty parsed test subject");
    let line = source
        .lines()
        .position(|line| line.trim() == NEW.trim())
        .ok_or_else(|| "changed return must exist".to_string())?
        + 1;
    classify_change_with_context(
        file,
        line,
        NEW,
        Some(old),
        &owners,
        &tests,
        PythonNoBehaviorContext {
            sole_line_replacement: sole,
            ..PythonNoBehaviorContext::default()
        },
        Some(source),
    )
    .ok_or_else(|| "the edit must produce a finding".to_string())
}

fn test(body: &str) -> String {
    format!("from shapes.money import Money\n\ndef test_it():\n{body}\n")
}

#[test]
fn integer_truthiness_credits_only_a_changed_truth_value() -> Check {
    let finding = classify(MONEY, OLD, &test("    assert Money(-1)"), true)?;
    assert_eq!(finding.class, ExposureClass::Exposed, "{finding:?}");
    assert!(
        finding
            .evidence
            .iter()
            .any(|line| line.contains("dunder_truthiness_read")),
        "{:?}",
        finding.evidence
    );
    assert_eq!(finding.related_tests.len(), 1);
    assert_eq!(
        finding.related_tests[0].oracle.as_deref(),
        Some("assert Money(-1)")
    );
    let alias = test("    assert Coin(-1)").replace("import Money", "import Money as Coin");
    assert_eq!(
        classify(MONEY, OLD, &alias, true)?.class,
        ExposureClass::Exposed
    );
    assert_eq!(
        classify(
            MONEY,
            "        return self.cents <= 0",
            &test("    assert not Money(1)"),
            true
        )?
        .class,
        ExposureClass::Exposed
    );
    Ok(())
}

#[test]
fn integer_truthiness_retains_the_original_corpus_abstentions() -> Check {
    for body in [
        "    assert Money(1)",
        "    assert not Money(0)",
        "    assert not Money(-1)",
    ] {
        assert_eq!(
            classify(MONEY, OLD, &test(body), true)?.class,
            ExposureClass::StaticUnknown,
            "{body}"
        );
    }
    Ok(())
}

#[test]
fn integer_truthiness_does_not_infer_nonliteral_or_noninteger_inputs() -> Check {
    for body in [
        "    assert Money(value)",
        "    assert Money(-1.0)",
        "    assert Money(True)",
        "    assert Money('-1')",
        "    assert Money(cents=-1)",
        "    assert Money(*[-1])",
        "    assert Money(-9223372036854775809)",
    ] {
        assert_eq!(
            classify(MONEY, OLD, &test(body), true)?.class,
            ExposureClass::StaticUnknown,
            "{body}"
        );
    }
    Ok(())
}

#[test]
fn integer_truthiness_requires_a_plain_constructor_and_field() -> Check {
    for source in [
        MONEY.replace("class Money:", "class Money(Base):"),
        MONEY.replace("class Money:", "@decorate\nclass Money:"),
        MONEY.replace("self.cents = cents", "self.cents = abs(cents)"),
        MONEY.replace(
            "self.cents = cents",
            "self.cents = cents\n        self.cents = 1",
        ),
        MONEY.replace("    def __bool__", "    @property\n    def __bool__"),
        format!("{MONEY}\n    def __getattribute__(self, name):\n        return 1\n"),
        format!("{MONEY}\nMoney.__bool__ = lambda self: True\n"),
        format!("{MONEY}\n    def __slots__(self):\n        pass\n"),
        format!("{MONEY}\n    def __classcell__(self):\n        pass\n"),
        format!("{MONEY}\n    def __qualname__(self):\n        pass\n"),
    ] {
        assert_ne!(
            classify(&source, OLD, &test("    assert Money(-1)"), true)?.class,
            ExposureClass::Exposed,
            "{source}"
        );
    }
    // A changed constructor cannot be reconstructed from the return edit.
    assert_eq!(
        classify(MONEY, OLD, &test("    assert Money(-1)"), false)?.class,
        ExposureClass::StaticUnknown
    );
    Ok(())
}

#[test]
fn integer_truthiness_refuses_test_rebinding_activation_and_wrappers() -> Check {
    for source in [
        test("    if False:\n        assert Money(-1)"),
        test("    assert always_true(Money(-1))"),
        test("    Money = Other\n    assert Money(-1)"),
        test("    touch()\n    assert Money(-1)"),
        test("    assert Money(-1)").replace("def test_it():", "def test_it(fixture):"),
        test("    assert Money(-1)").replace("def test_it():", "@pytest.mark.skip\ndef test_it():"),
        format!(
            "{}\nMoney.__bool__ = lambda self: True\n",
            test("    assert Money(-1)")
        ),
        test("    assert Money(-1)").replace(
            "from shapes.money import Money",
            "from rival.money import Money",
        ),
    ] {
        assert_ne!(
            classify(MONEY, OLD, &source, true)?.class,
            ExposureClass::Exposed,
            "{source}"
        );
    }
    Ok(())
}

#[test]
fn integer_truthiness_requires_both_predicates_to_be_supported() -> Check {
    for old in [
        "        return unknown(self.cents)",
        "        return self.other != 0",
        "        return self.cents != 0 != 1",
        "        return self.cents is not None",
        "return self.cents != 0",
    ] {
        assert_eq!(
            classify(MONEY, old, &test("    assert Money(-1)"), true)?.class,
            ExposureClass::StaticUnknown,
            "{old}"
        );
    }
    Ok(())
}

#[test]
fn integer_truthiness_refuses_replaced_tests_and_definition_effects() -> Check {
    for source in [
        format!("{}\ndef test_it():\n    assert True\n", test("    assert Money(-1)")),
        format!("{}\ntest_it = None\n", test("    assert Money(-1)")),
        test("    assert Money(-1)").replace("def test_it():", "def test_it() -> type.__setattr__(Money, '__bool__', lambda self: True):"),
        test("    assert Money(-1)").replace("def test_it():", "def helper() -> type.__setattr__(Money, '__bool__', lambda self: True):\n    pass\n\ndef test_it():"),
        test("    assert Money(-1)").replace("def test_it():", "def test_it(self=type.__setattr__(Money, '__bool__', lambda self: True)):"),
    ] {
        assert_ne!(classify(MONEY, OLD, &source, true)?.class, ExposureClass::Exposed, "{source}");
    }
    Ok(())
}
