//! A one-`return` owner read through a chain of one-`return` callers that a
//! test pins (#9226): credited only when the pinned assertion holds with the
//! old body and fails with the new one, through model callables only.

use super::*;
use crate::domain::ExposureClass;
use std::path::{Path, PathBuf};

type Check = Result<(), String>;

const LEDGER: &str = "class Ledger:\n    def _ledger_pad(self):\n        return 2\n\n    def render_ledger(self):\n        return \"|\" + \" \" * self._ledger_pad() + \"|\"\n";
const LEDGER_TEST: &str = "from reach.ledger import Ledger\n\n\ndef test_render_ledger():\n    ledger = Ledger()\n    assert ledger.render_ledger() == \"|  |\"\n";

fn write_file(path: &Path, contents: &str) -> Check {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|err| format!("create_dir_all({}): {err}", parent.display()))?;
    }
    std::fs::write(path, contents).map_err(|err| format!("write({}): {err}", path.display()))
}

/// The finding for the one-line edit `old` -> `new` at `line` of `changed` in
/// a workspace of `files`, where `files` holds the new side.
fn classify(
    files: &[(&str, &str)],
    changed: &str,
    line: usize,
    old: &str,
    new: &str,
) -> Result<Finding, String> {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|err| format!("system time: {err}"))?
        .as_nanos();
    let root = std::env::temp_dir().join(format!("ripr-call-chain-{}-{nanos}", std::process::id()));
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
    let changed = vec![ChangedFile {
        path: PathBuf::from(changed),
        added_lines: vec![crate::analysis::diff::ChangedLine {
            line,
            new_side_line: line,
            text: new.to_string(),
        }],
        removed_lines: vec![crate::analysis::diff::ChangedLine {
            line,
            new_side_line: line,
            text: old.to_string(),
        }],
    }];
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

fn credited(finding: &Finding) -> bool {
    finding
        .evidence
        .iter()
        .any(|line| line.starts_with("related_test_relation: module_value_call"))
}

/// The ledger edit `return 2` -> `return 3` with `source` (new side) and
/// `test` in the workspace.
fn ledger_finding(source: &str, test: &str) -> Result<Finding, String> {
    ledger_finding_with(source, test, &[])
}

fn ledger_finding_with(
    source: &str,
    test: &str,
    extra: &[(&str, &str)],
) -> Result<Finding, String> {
    let line = source
        .lines()
        .position(|line| line.trim() == "return 3")
        .ok_or("no changed line")?
        + 1;
    let mut files = vec![
        ("reach/__init__.py", ""),
        ("reach/ledger.py", source),
        ("tests/test_ledger.py", test),
    ];
    files.extend_from_slice(extra);
    classify(
        &files,
        "reach/ledger.py",
        line,
        "        return 2",
        "        return 3",
    )
}

fn new_side(source: &str) -> String {
    source.replacen("return 2", "return 3", 1)
}

fn withheld(finding: &Finding, label: &str) {
    assert_ne!(
        finding.class,
        ExposureClass::Exposed,
        "{label}: {finding:?}"
    );
    assert!(!credited(finding), "{label}: {:?}", finding.evidence);
}

#[test]
fn a_self_call_chain_pinned_through_a_local_instance_is_credited() -> Check {
    let finding = ledger_finding(&new_side(LEDGER), LEDGER_TEST)?;
    assert_eq!(finding.class, ExposureClass::Exposed, "{finding:?}");
    assert!(credited(&finding), "{:?}", finding.evidence);
    assert!(
        finding.evidence.iter().any(|line| line.starts_with(
            "module_value_read: `Ledger._ledger_pad` 2 -> 3 through `ledger.render_ledger`"
        )),
        "{:?}",
        finding.evidence
    );
    // The same chain built inline, and through a module receiver.
    for test in [
        "from reach.ledger import Ledger\n\n\ndef test_render_ledger():\n    assert Ledger().render_ledger() == \"|  |\"\n",
        "import reach.ledger as ledger_module\n\n\ndef test_render_ledger():\n    assert ledger_module.Ledger().render_ledger() == \"|  |\"\n",
    ] {
        let finding = ledger_finding(&new_side(LEDGER), test)?;
        assert_eq!(finding.class, ExposureClass::Exposed, "{test}: {finding:?}");
    }
    Ok(())
}

#[test]
fn a_module_function_that_builds_the_class_is_credited() -> Check {
    let old = "class PreparedRequest:\n    def _encode(self, body):\n        return body.upper()\n\n    def prepare(self, body):\n        return self._encode(body)\n\n\ndef post(body):\n    return PreparedRequest().prepare(body)\n";
    let new = old.replacen("body.upper()", "body.lower()", 1);
    let test = "from reach.prepared import post\n\n\ndef test_post_encodes_body():\n    assert post(\"ok\") == \"OK\"\n";
    let finding = classify(
        &[
            ("reach/__init__.py", ""),
            ("reach/prepared.py", &new),
            ("tests/test_prepared.py", test),
        ],
        "reach/prepared.py",
        3,
        "        return body.upper()",
        "        return body.lower()",
    )?;
    assert_eq!(finding.class, ExposureClass::Exposed, "{finding:?}");
    // A non-ASCII value is outside the case model.
    let test = "from reach.prepared import post\n\n\ndef test_post_encodes_body():\n    assert post(\"ß\") == \"SS\"\n";
    let finding = classify(
        &[
            ("reach/__init__.py", ""),
            ("reach/prepared.py", &new),
            ("tests/test_prepared.py", test),
        ],
        "reach/prepared.py",
        3,
        "        return body.upper()",
        "        return body.lower()",
    )?;
    withheld(&finding, "non-ASCII");
    Ok(())
}

#[test]
fn a_six_hop_chain_is_credited_and_a_recursive_one_decides_nothing() -> Check {
    let chain = "class Chain:\n    def run(self):\n        return self._a()\n\n    def _a(self):\n        return self._b()\n\n    def _b(self):\n        return self._c()\n\n    def _c(self):\n        return self._d()\n\n    def _d(self):\n        return self._e()\n\n    def _e(self):\n        return self._f()\n\n    def _f(self):\n        return 7\n";
    let test = "from reach.chain import Chain\n\n\ndef test_chain_value():\n    assert Chain().run() == 6\n";
    let files = [
        ("reach/__init__.py", ""),
        ("reach/chain.py", chain),
        ("tests/test_chain.py", test),
    ];
    let finding = classify(
        &files,
        "reach/chain.py",
        21,
        "        return 6",
        "        return 7",
    )?;
    assert_eq!(finding.class, ExposureClass::Exposed, "{finding:?}");
    // `_f` calls itself: the evaluation never ends, so nothing is decided.
    let recursive = chain.replacen("return 7", "return self._f() + 7", 1);
    let files = [
        ("reach/__init__.py", ""),
        ("reach/chain.py", recursive.as_str()),
        ("tests/test_chain.py", test),
    ];
    let finding = classify(
        &files,
        "reach/chain.py",
        21,
        "        return self._f() + 6",
        "        return self._f() + 7",
    )?;
    withheld(&finding, "recursive");
    Ok(())
}

/// Negatives: shapes where the pinned assertion does not show the edit is
/// observed, or the model cannot show the chain runs the code it reads.
#[test]
fn chains_the_model_cannot_show_observe_the_edit_are_not_credited() -> Check {
    // Both bodies satisfy the assertion: the chain discards the value.
    let source = "class Ledger:\n    def _ledger_pad(self):\n        return 3\n\n    def render_ledger(self):\n        return \"|\" if self._ledger_pad() > 0 else \"-\"\n";
    let test = "from reach.ledger import Ledger\n\n\ndef test_render_ledger():\n    assert Ledger().render_ledger() == \"|\"\n";
    withheld(&ledger_finding(source, test)?, "value discarded");

    // The test pins a method that does not reach the owner.
    let source = format!(
        "{}\n    def title(self):\n        return \"ledger\"\n",
        new_side(LEDGER)
    );
    let test = "from reach.ledger import Ledger\n\n\ndef test_title():\n    assert Ledger().title() == \"ledger\"\n";
    withheld(&ledger_finding(&source, test)?, "other method");

    // A subclass overrides the owner; the test builds the subclass.
    let source = format!(
        "{}\n\nclass Wide(Ledger):\n    def _ledger_pad(self):\n        return 2\n",
        new_side(LEDGER)
    );
    let test = "from reach.ledger import Wide\n\n\ndef test_render_ledger():\n    assert Wide().render_ledger() == \"|  |\"\n";
    withheld(&ledger_finding(&source, test)?, "subclass override");

    // The class has an `__init__`, a class attribute, or a decorated hop.
    for source in [
        new_side(LEDGER).replacen(
            "class Ledger:\n",
            "class Ledger:\n    def __init__(self):\n        self.width = 1\n\n",
            1,
        ),
        new_side(LEDGER).replacen("class Ledger:\n", "class Ledger:\n    width = 1\n\n", 1),
        new_side(LEDGER).replacen(
            "    def render_ledger",
            "    @staticmethod_like\n    def render_ledger",
            1,
        ),
        new_side(LEDGER).replacen("class Ledger:", "class Ledger(Base):", 1),
    ] {
        withheld(&ledger_finding(&source, LEDGER_TEST)?, &source);
    }

    // Something may replace a method: a module write, `setattr`, or a test
    // file that patches the class.
    let source = format!(
        "{}\n\nLedger._ledger_pad = lambda self: 2\n",
        new_side(LEDGER)
    );
    withheld(
        &ledger_finding(&source, LEDGER_TEST)?,
        "module attribute write",
    );
    let source = format!(
        "{}\n\nsetattr(Ledger, \"_ledger_pad\", lambda self: 2)\n",
        new_side(LEDGER)
    );
    withheld(&ledger_finding(&source, LEDGER_TEST)?, "setattr");
    let source = format!(
        "{}\n\nLedger._ledger_pad.__code__ = (lambda self: 2).__code__\n",
        new_side(LEDGER)
    );
    withheld(&ledger_finding(&source, LEDGER_TEST)?, "code slot write");
    let finding = ledger_finding_with(
        &new_side(LEDGER),
        LEDGER_TEST,
        &[(
            "tests/conftest.py",
            "from reach.ledger import Ledger\n\nLedger._ledger_pad = lambda self: 2\n",
        )],
    )?;
    withheld(&finding, "conftest write");

    // The test writes the method on the class or its local instance, or hands
    // the instance to other code, before the assertion.
    for test in [
        "from reach.ledger import Ledger\n\n\ndef test_render_ledger():\n    Ledger._ledger_pad = lambda self: 2\n    assert Ledger().render_ledger() == \"|  |\"\n",
        "from reach.ledger import Ledger\n\n\ndef test_render_ledger():\n    ledger = Ledger()\n    ledger._ledger_pad = lambda: 2\n    assert ledger.render_ledger() == \"|  |\"\n",
        "from reach.ledger import Ledger\nfrom helpers import widen\n\n\ndef test_render_ledger():\n    ledger = Ledger()\n    widen(ledger)\n    assert ledger.render_ledger() == \"|  |\"\n",
    ] {
        withheld(&ledger_finding(&new_side(LEDGER), test)?, test);
    }

    // The test may not run the assertion, rebinds the class, requests a
    // fixture, or binds the local twice.
    for test in [
        "from reach.ledger import Ledger\n\n\ndef test_render_ledger(flag):\n    assert Ledger().render_ledger() == \"|  |\"\n",
        "from reach.ledger import Ledger\n\n\ndef test_render_ledger():\n    if Ledger:\n        assert Ledger().render_ledger() == \"|  |\"\n",
        "from reach.ledger import Ledger\nfrom other import Ledger\n\n\ndef test_render_ledger():\n    assert Ledger().render_ledger() == \"|  |\"\n",
        "from reach.ledger import Ledger\n\n\ndef test_render_ledger():\n    ledger = Ledger()\n    ledger = Other()\n    assert ledger.render_ledger() == \"|  |\"\n",
        "import pytest\nfrom reach.ledger import Ledger\n\n\n@pytest.mark.skip\ndef test_render_ledger():\n    assert Ledger().render_ledger() == \"|  |\"\n",
    ] {
        withheld(&ledger_finding(&new_side(LEDGER), test)?, test);
    }
    Ok(())
}
