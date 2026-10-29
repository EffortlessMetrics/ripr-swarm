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
    // The same chain built inline.
    let test = "from reach.ledger import Ledger\n\n\ndef test_render_ledger():\n    assert Ledger().render_ledger() == \"|  |\"\n";
    let finding = ledger_finding(&new_side(LEDGER), test)?;
    assert_eq!(finding.class, ExposureClass::Exposed, "{test}: {finding:?}");
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

/// Negatives for the review guards: what runs before the assertion, the
/// test's own shape, how the test spells the entry, and code that can
/// replace a hop without naming it.
#[test]
fn guards_around_the_evaluator_refuse_unproven_chains() -> Check {
    let new = new_side(LEDGER);
    // An earlier assertion fails on the old body, so the pinned one never
    // ran against it; a helper call before the assertion may change state.
    for test in [
        "from reach.ledger import Ledger\n\n\ndef test_render_ledger():\n    assert Ledger().render_ledger() == \"|   |\"\n    assert Ledger().render_ledger() == \"|  |\"\n",
        "from reach.ledger import Ledger\nfrom helpers import prepare\n\n\ndef test_render_ledger():\n    prepare()\n    assert Ledger().render_ledger() == \"|  |\"\n",
        // A decorated test, and a module that defines the test twice.
        "from reach.ledger import Ledger\nfrom helpers import retry\n\n\n@retry\ndef test_render_ledger():\n    assert Ledger().render_ledger() == \"|  |\"\n",
        "from reach.ledger import Ledger\n\n\ndef test_render_ledger():\n    assert Ledger().render_ledger() == \"|  |\"\n\n\ndef test_render_ledger():\n    assert True\n",
        // The package is imported but the owner module is not.
        "import reach\n\n\ndef test_render_ledger():\n    assert reach.Ledger().render_ledger() == \"|  |\"\n",
        // A submodule reached as a package attribute, which the package's
        // `__init__.py` may rebind.
        "import reach.ledger as ledger_module\n\n\ndef test_render_ledger():\n    assert ledger_module.Ledger().render_ledger() == \"|  |\"\n",
        "from reach import ledger\n\n\ndef test_render_ledger():\n    assert ledger.Ledger().render_ledger() == \"|  |\"\n",
    ] {
        withheld(&ledger_finding(&new, test)?, test);
    }
    // The package does rebind the submodule name.
    let finding = ledger_finding_with(
        &new,
        "import reach.ledger as ledger_module\n\n\ndef test_render_ledger():\n    assert ledger_module.Ledger().render_ledger() == \"|  |\"\n",
        &[(
            "reach/__init__.py",
            "from . import ledger as _real\n\n\nclass ledger:\n    class Ledger:\n        def render_ledger(self):\n            return \"|  |\"\n",
        )],
    )?;
    withheld(&finding, "package shadows submodule");
    // An earlier assertion that holds on both sides keeps the credit.
    let test = "from reach.ledger import Ledger\n\n\ndef test_render_ledger():\n    assert Ledger().render_ledger() != \"-\"\n    assert Ledger().render_ledger() == \"|  |\"\n";
    let finding = ledger_finding(&new, test)?;
    assert_eq!(finding.class, ExposureClass::Exposed, "{finding:?}");

    // A local bound to a model function's result is not an instance: the
    // call runs code before the assertion (here it may skip the test), as
    // the same call written as a statement does.
    let source = format!(
        "from reach.helpers import skip_all\n\n\n{new}\n\ndef prepare():\n    return skip_all()\n"
    );
    let helpers = [(
        "reach/helpers.py",
        "import pytest\n\n\ndef skip_all():\n    pytest.skip(\"off\")\n",
    )];
    for test in [
        "from reach.ledger import Ledger, prepare\n\n\ndef test_render_ledger():\n    unused = prepare()\n    assert Ledger().render_ledger() == \"|  |\"\n",
        "from reach.ledger import Ledger, prepare\n\n\ndef test_render_ledger():\n    prepare()\n    assert Ledger().render_ledger() == \"|  |\"\n",
    ] {
        withheld(&ledger_finding_with(&source, test, &helpers)?, test);
    }
    let test = "from reach.ledger import Ledger, prepare\n\n\ndef test_render_ledger():\n    ledger = Ledger()\n    assert ledger.render_ledger() == \"|  |\"\n";
    let finding = ledger_finding_with(&source, test, &helpers)?;
    assert_eq!(finding.class, ExposureClass::Exposed, "{finding:?}");

    // An autouse conftest fixture may skip the test.
    let finding = ledger_finding_with(
        &new,
        LEDGER_TEST,
        &[(
            "tests/conftest.py",
            "import pytest\n\n\n@pytest.fixture(autouse=True)\ndef gate():\n    pytest.skip(\"off\")\n",
        )],
    )?;
    withheld(&finding, "autouse skip");

    // A foreign module (a conftest with effects is implicit reach already)
    // writes a dunder hook that can intercept attribute lookup or
    // construction without naming the hop.
    for patches in [
        "from reach.ledger import Ledger\n\nLedger.__getattribute__ = lambda self, name: (lambda: 2)\n",
        "from reach.ledger import Ledger\n\nLedger.__new__ = lambda cls: object()\n",
        "from reach.ledger import Ledger\n\ntype.__setattr__(Ledger, \"_ledger_pad\", lambda self: 2)\n",
        "from reach.ledger import Ledger\n\nAlias = Ledger\nAlias.__getattribute__ = lambda self, name: (lambda: 2)\n",
    ] {
        let finding = ledger_finding_with(&new, LEDGER_TEST, &[("reach/patches.py", patches)])?;
        withheld(&finding, patches);
    }

    // A generator hop, and a name-mangled hop, are outside the model.
    for source in [
        new.replacen(
            "    def render_ledger(self):\n        return",
            "    def render_ledger(self):\n        return (yield) if False else",
            1,
        ),
        new.replace("_ledger_pad", "__ledger_pad"),
    ] {
        withheld(&ledger_finding(&source, LEDGER_TEST)?, &source);
    }

    // A fan-out the step budget cannot finish decides nothing.
    let mut fan = String::from("class Fan:\n    def _leaf(self):\n        return 3\n");
    let mut previous = String::from("_leaf");
    for level in 0..14 {
        let name = format!("_l{level}");
        fan.push_str(&format!(
            "\n    def {name}(self):\n        return self.{previous}() + self.{previous}()\n"
        ));
        previous = name;
    }
    fan.push_str(&format!(
        "\n    def run(self):\n        return self.{previous}()\n"
    ));
    let test = "from reach.fan import Fan\n\n\ndef test_fan():\n    assert Fan().run() == 32768\n";
    let finding = classify(
        &[
            ("reach/__init__.py", ""),
            ("reach/fan.py", fan.as_str()),
            ("tests/test_fan.py", test),
        ],
        "reach/fan.py",
        3,
        "        return 2",
        "        return 3",
    )?;
    withheld(&finding, "fan-out budget");
    Ok(())
}

/// Negatives for the second review: the owner module itself installs a hook,
/// the class body rebinds a method while it runs, the module namespace is
/// written through a function's globals or a frame, and cost stays linear in
/// the test's assertions.
#[test]
fn owner_module_hooks_and_class_body_code_are_not_credited() -> Check {
    let new = new_side(LEDGER);
    for source in [
        // The owner module writes a lookup or construction hook on its class.
        format!("{new}\n\nLedger.__getattribute__ = lambda self, name: lambda: \"|  |\"\n"),
        format!("{new}\n\nclass _Fixed:\n    def render_ledger(self):\n        return \"|  |\"\n\n\nLedger.__new__ = lambda cls: _Fixed()\n"),
        // A default or decorator in the class body rebinds the method.
        new.replacen(
            "    def render_ledger(self):",
            "    def other(self, x=(_ledger_pad := (lambda self: 2))):\n        return 0\n\n    def render_ledger(self):",
            1,
        ),
        new.replacen(
            "    def render_ledger(self):",
            "    @((_ledger_pad := (lambda self: 2)) and (lambda f: f))\n    def other(self):\n        return 0\n\n    def render_ledger(self):",
            1,
        ),
        // A default that runs code without a walrus.
        new.replacen(
            "    def render_ledger(self):",
            "    def other(self, x=print()):\n        return 0\n\n    def render_ledger(self):",
            1,
        ),
    ] {
        withheld(&ledger_finding(&source, LEDGER_TEST)?, &source);
    }

    // A decorator or default calls a helper that writes the class namespace
    // through its caller's frame, under a name built at runtime.
    let helpers = "import sys\n\n\ndef rebind(f=None):\n    sys._getframe(1).f_locals[\"_ledger\" + \"_pad\"] = lambda self: 2\n    return f\n";
    for source in [
        new.replacen(
            "class Ledger:",
            "from reach.helpers import rebind\n\n\nclass Ledger:",
            1,
        )
        .replacen(
            "    def render_ledger(self):",
            "    @rebind\n    def other(self):\n        return 0\n\n    def render_ledger(self):",
            1,
        ),
        new.replacen(
            "class Ledger:",
            "from reach.helpers import rebind\n\n\nclass Ledger:",
            1,
        )
        .replacen(
            "    def render_ledger(self):",
            "    def other(self, x=rebind()):\n        return 0\n\n    def render_ledger(self):",
            1,
        ),
    ] {
        let finding = ledger_finding_with(&source, LEDGER_TEST, &[("reach/helpers.py", helpers)])?;
        withheld(&finding, &source);
    }

    // A module-level function chain whose namespace is written through
    // `__globals__` or a frame.
    let functions =
        "def _pad():\n    return 3\n\n\ndef render():\n    return \"|\" + \" \" * _pad() + \"|\"\n";
    let test = "from reach.ledger import render\n\n\ndef test_render():\n    assert render() == \"|  |\"\n";
    let run = |source: &str, extra: &[(&str, &str)]| -> Result<Finding, String> {
        let mut files = vec![
            ("reach/__init__.py", ""),
            ("reach/ledger.py", source),
            ("tests/test_ledger.py", test),
        ];
        files.extend_from_slice(extra);
        let line = source
            .lines()
            .position(|line| line == "    return 3")
            .ok_or("no changed line")?
            + 1;
        classify(
            &files,
            "reach/ledger.py",
            line,
            "    return 2",
            "    return 3",
        )
    };
    let finding = run(functions, &[])?;
    assert_eq!(finding.class, ExposureClass::Exposed, "{finding:?}");
    for source in [
        format!("{functions}\n\nrender.__globals__[\"_pad\"] = lambda: 2\n"),
        format!("{functions}\n\nimport sys\nsys._getframe().f_globals[\"_pad\"] = lambda: 2\n"),
    ] {
        withheld(&run(&source, &[])?, &source);
    }
    let finding = run(
        functions,
        &[(
            "reach/patches.py",
            "from reach import ledger as target\n\ngetattr(target, \"ren\" + \"der\").__globals__[\"_p\" + \"ad\"] = lambda: 2\n",
        )],
    )?;
    withheld(&finding, "foreign __globals__ write");
    // The owner module calls a helper at import that writes the owner's
    // globals through its caller's frame, under a name built at runtime.
    let source = format!("from reach.helpers import install\n\n\n{functions}\n\ninstall()\n");
    let finding = run(
        &source,
        &[(
            "reach/helpers.py",
            "import sys\n\n\ndef install():\n    sys._getframe(1).f_globals[\"_p\" + \"ad\"] = lambda: 2\n",
        )],
    )?;
    withheld(&finding, "foreign frame write");

    // Reflection reaches the namespace under a name built at runtime: the
    // garbage collector, in the owner, the package or a helper the test
    // imports, and `getattr` with a built dunder name.
    let gc_write = |name: &str| {
        format!(
            "import gc\n\nfor _space in gc.get_objects():\n    if type(_space) is dict and _space.get(\"render\") is {name}:\n        _space[\"_pad\"] = lambda: 2\n"
        )
    };
    let owner_gc = format!("{functions}\n\n{}", gc_write("render"));
    withheld(&run(&owner_gc, &[])?, "owner gc write");
    let foreign_gc = format!(
        "from reach.ledger import render as _render\n{}",
        gc_write("_render")
    );
    withheld(
        &run(functions, &[("reach/boot.py", foreign_gc.as_str())])?,
        "foreign gc write",
    );
    let built =
        format!("{functions}\n\ngetattr(render, \"__glob\" + \"als__\")[\"_pad\"] = lambda: 2\n");
    withheld(&run(&built, &[])?, "built getattr name");

    // A file the test imports replaces the owner module in `sys.modules`,
    // plainly or with a built name.
    let test_with_boot = "import reach.boot\nfrom reach.ledger import render\n\n\ndef test_render():\n    assert render() == \"|  |\"\n";
    for boot in [
        "import types\nfrom sys import modules\n\nmodules[\"reach.ledger\"] = types.SimpleNamespace(render=lambda: \"|  |\")\n",
        "import sys, types\n\nsys.modules[\"reach.\" + \"led\" + \"ger\"] = types.SimpleNamespace(**{\"ren\" + \"der\": lambda: \"|  |\"})\n",
    ] {
        let files = [
            ("reach/__init__.py", ""),
            ("reach/ledger.py", functions),
            ("reach/boot.py", boot),
            ("tests/test_ledger.py", test_with_boot),
        ];
        let finding = classify(&files, "reach/ledger.py", 2, "    return 2", "    return 3")?;
        withheld(&finding, boot);
    }
    // An import hook hands the test a different owner module.
    let hook = "import sys\nimport types\n\n\nclass _Finder:\n    def find_spec(self, name, path, target=None):\n        if name != \"reach.ledger\":\n            return None\n        return types.SimpleNamespace(\n            name=name, loader=self, origin=None, submodule_search_locations=None,\n            loader_state=None, cached=None, parent=\"reach\", has_location=False,\n        )\n\n    def create_module(self, spec):\n        return types.SimpleNamespace(render=lambda: \"|  |\")\n\n    def exec_module(self, module):\n        pass\n\n\nsys.meta_path.insert(0, _Finder())\n";
    let files = [
        ("reach/__init__.py", ""),
        ("reach/ledger.py", functions),
        ("reach/boot.py", hook),
        ("tests/test_ledger.py", test_with_boot),
    ];
    withheld(
        &classify(&files, "reach/ledger.py", 2, "    return 2", "    return 3")?,
        "meta_path hook",
    );
    // The import may load another file than the owner: the package extends
    // its `__path__` to a sourceless module the scan never reads, a package
    // `ledger/` beats the module `ledger.py`, or a second file has the
    // owner's dotted path under another import root (pytest's `prepend` mode
    // puts `tests/` first).
    let shadow = "def render():\n    return \"|  |\"\n";
    for extra in [
        vec![
            (
                "reach/__init__.py",
                "import os\n\n__path__.insert(0, os.path.join(os.path.dirname(__file__), \"_alt\"))\n",
            ),
            ("reach/_alt/ledger.pyc", shadow),
        ],
        vec![
            ("reach/__init__.py", ""),
            ("reach/ledger/__init__.py", shadow),
        ],
        vec![
            ("reach/__init__.py", ""),
            ("tests/reach/__init__.py", ""),
            ("tests/reach/ledger.py", shadow),
        ],
    ] {
        let mut files = vec![
            ("reach/ledger.py", functions),
            ("tests/test_ledger.py", test),
        ];
        files.extend(extra);
        let finding = classify(&files, "reach/ledger.py", 2, "    return 2", "    return 3")?;
        withheld(&finding, "shadowed owner module");
    }
    // Python NFKC-normalises identifiers: a fullwidth `_\u{ff50}ad` in the
    // owner, a foreign file or the test module binds `_pad`/`render`, while
    // name matches here are byte-exact. A compiled extension beside the
    // owner is loaded before its source.
    let owner_nfkc = format!("{functions}\n\ndef _\u{ff50}ad():\n    return 2\n");
    let nfkc_test = "from reach.ledger import render\n\n\n\u{ff52}ender = lambda: \"|  |\"\n\n\ndef test_render():\n    assert render() == \"|  |\"\n";
    let unicode_string = "from reach.ledger import render\n\n\ndef test_render():\n    assert render() == \"|  |\"\n    assert \"\u{e9}\" == \"\u{e9}\"\n";
    // (label, owner source, files replaced or added, credited)
    type Case<'a> = (&'a str, String, Vec<(&'a str, &'a str)>, bool);
    let cases: Vec<Case> = vec![
        ("owner nfkc def", owner_nfkc, vec![], false),
        (
            "foreign nfkc attribute",
            functions.to_string(),
            vec![
                (
                    "reach/boot.py",
                    "from reach import ledger as _m\n\n_m._\u{ff50}ad = lambda: 2\n",
                ),
                ("tests/test_ledger.py", test_with_boot),
            ],
            false,
        ),
        (
            "test nfkc rebind",
            functions.to_string(),
            vec![("tests/test_ledger.py", nfkc_test)],
            false,
        ),
        (
            "non-ASCII string only",
            functions.to_string(),
            vec![("tests/test_ledger.py", unicode_string)],
            true,
        ),
    ];
    // A source encoding other than UTF-8 decodes an ASCII comment into code
    // (`+AAo-` is a newline in UTF-7); unchecked-hash bytecode runs in place
    // of its source. A UTF-8 declaration and checked or timestamp pycs keep
    // the credit.
    let utf7_owner = format!("# coding: utf-7\n{functions}NOTE = 0  # +AAo-_pad = lambda: 2\n");
    let utf7_boot =
        "# coding: utf-7\nfrom reach import ledger as _m\nNOTE = 0  # +AAo-_m._pad = lambda: 2\n";
    let utf7_test = "# coding: utf-7\nfrom reach.ledger import render\nNOTE = 0  # +AAo-render = lambda: +ACIAfAAgACAAfAAi-\n\n\ndef test_render():\n    assert render() == \"|  |\"\n";
    let utf8_owner = format!("# -*- coding: utf-8 -*-\n{functions}");
    let cr_owner = format!("\r# coding: utf-7\n{functions}NOTE = 0  # +AAo-_pad = lambda: 2\n");
    let cr_boot =
        "\r# coding: utf-7\nfrom reach import ledger as _m\nNOTE = 0  # +AAo-_m._pad = lambda: 2\n";
    let crlf_owner = functions.replace('\n', "\r\n");
    let pyc = |flags: char| format!("\u{d}\r\r\n{flags}\0\0\0{}", "\0".repeat(8));
    let (unchecked, checked, timestamp) = (pyc('\u{1}'), pyc('\u{3}'), pyc('\0'));
    let cases = cases.into_iter().chain([
        ("utf-7 owner", utf7_owner, vec![], false),
        (
            "utf-7 foreign",
            functions.to_string(),
            vec![
                ("reach/boot.py", utf7_boot),
                ("tests/test_ledger.py", test_with_boot),
            ],
            false,
        ),
        (
            "utf-7 test",
            functions.to_string(),
            vec![("tests/test_ledger.py", utf7_test)],
            false,
        ),
        ("utf-8 declared", utf8_owner, vec![], true),
        ("lone carriage return owner", cr_owner, vec![], false),
        (
            "lone carriage return foreign",
            functions.to_string(),
            vec![
                ("reach/boot.py", cr_boot),
                ("tests/test_ledger.py", test_with_boot),
            ],
            false,
        ),
        ("crlf owner", crlf_owner, vec![], true),
        (
            "unchecked pyc",
            functions.to_string(),
            vec![(
                "reach/__pycache__/ledger.cpython-313.pyc",
                unchecked.as_str(),
            )],
            false,
        ),
        (
            "unchecked foreign pyc",
            functions.to_string(),
            vec![
                ("reach/boot.py", "VALUE = 1\n"),
                ("reach/__pycache__/boot.cpython-313.pyc", unchecked.as_str()),
                ("tests/test_ledger.py", test_with_boot),
            ],
            false,
        ),
        (
            "checked pyc",
            functions.to_string(),
            vec![("reach/__pycache__/ledger.cpython-313.pyc", checked.as_str())],
            true,
        ),
        (
            "timestamp pyc",
            functions.to_string(),
            vec![(
                "reach/__pycache__/ledger.cpython-313.pyc",
                timestamp.as_str(),
            )],
            true,
        ),
    ]);
    let extensions = [
        "reach/ledger.cpython-313-x86_64-linux-gnu.so",
        "reach/ledger.abi3.so",
        "reach/ledger.so",
        "reach/ledger.pyd",
    ];
    let cases = cases.into_iter().chain(extensions.iter().map(|extension| {
        (
            *extension,
            functions.to_string(),
            vec![(*extension, "\x7fELF")],
            false,
        )
    }));
    for (label, source, extra, credited) in cases {
        let mut files = vec![
            ("reach/__init__.py", ""),
            ("reach/ledger.py", source.as_str()),
            ("tests/test_ledger.py", test),
        ];
        for (path, text) in extra {
            files.retain(|(present, _)| *present != path);
            files.push((path, text));
        }
        let line = source
            .lines()
            .position(|line| line == "    return 3")
            .ok_or("no changed line")?
            + 1;
        let finding = classify(
            &files,
            "reach/ledger.py",
            line,
            "    return 2",
            "    return 3",
        )?;
        if credited {
            assert_eq!(
                finding.class,
                ExposureClass::Exposed,
                "{label}: {finding:?}"
            );
        } else {
            withheld(&finding, label);
        }
    }
    // A package owner: an `__init__` extension beside `__init__.py` is loaded
    // first.
    let package_test =
        "from reach import render\n\n\ndef test_render():\n    assert render() == \"|  |\"\n";
    for (extension, credited) in [
        (None, true),
        (
            Some("reach/__init__.cpython-313-x86_64-linux-gnu.so"),
            false,
        ),
    ] {
        let mut files = vec![
            ("reach/__init__.py", functions),
            ("tests/test_reach.py", package_test),
        ];
        files.extend(extension.map(|extension| (extension, "\x7fELF")));
        let finding = classify(
            &files,
            "reach/__init__.py",
            2,
            "    return 2",
            "    return 3",
        )?;
        if credited {
            assert_eq!(
                finding.class,
                ExposureClass::Exposed,
                "package owner: {finding:?}"
            );
        } else {
            withheld(&finding, "__init__ extension");
        }
    }
    // A module dict reached through `vars` and written under a built key.
    // `vars` also marks the file a dynamic writer, so the reflection words
    // `vars`/`__dict__` are defense in depth here, not the only refusal.
    let files = [
        ("reach/__init__.py", ""),
        ("reach/ledger.py", functions),
        (
            "reach/boot.py",
            "from reach import ledger as target\n\nvars(target)[\"_p\" + \"ad\"] = lambda: 2\n",
        ),
        ("tests/test_ledger.py", test_with_boot),
    ];
    withheld(
        &classify(&files, "reach/ledger.py", 2, "    return 2", "    return 3")?,
        "vars built key",
    );
    // An inert imported helper keeps the credit.
    let files = [
        ("reach/__init__.py", ""),
        ("reach/ledger.py", functions),
        ("reach/boot.py", "VALUE = 1\n"),
        ("tests/test_ledger.py", test_with_boot),
    ];
    let finding = classify(&files, "reach/ledger.py", 2, "    return 2", "    return 3")?;
    assert_eq!(finding.class, ExposureClass::Exposed, "{finding:?}");

    // An import shadow refuses the credit: a zip or egg in the workspace, or
    // a pytest config that sets `pythonpath`, can load code the scan never
    // reads (#10969). A stale timestamp pyc and an installed copy stay
    // residual risk, not refusals: the scan trusts timestamp pycs the way
    // CPython trusts them, and installed-copy detection needs a live
    // interpreter query outside the bounded static reads.
    //
    // (label, owner source, files replaced or added, credited)
    type ShadowCase<'a> = (&'a str, Vec<(&'a str, &'a str)>, bool);
    let shadow_cases: Vec<ShadowCase> = vec![
        (
            "zip archive in workspace",
            vec![("vendor.zip", "PK\x03\x04")],
            false,
        ),
        (
            "egg archive in workspace",
            vec![("vendor.egg", "PK\x03\x04")],
            false,
        ),
        (
            "nested zip archive",
            vec![("third_party/vendor.zip", "PK\x03\x04")],
            false,
        ),
        (
            "pytest.ini pythonpath",
            vec![("pytest.ini", "[pytest]\npythonpath = vendor.zip\n")],
            false,
        ),
        (
            "pyproject pythonpath",
            vec![(
                "pyproject.toml",
                "[tool.pytest.ini_options]\npythonpath = [\"vendor.zip\"]\n",
            )],
            false,
        ),
        (
            "setup.cfg pythonpath",
            vec![("setup.cfg", "[tool:pytest]\npythonpath = vendor.zip\n")],
            false,
        ),
        (
            "tox.ini pythonpath",
            vec![("tox.ini", "[pytest]\npythonpath = vendor.zip\n")],
            false,
        ),
        (
            "pytest.ini without pythonpath",
            vec![("pytest.ini", "[pytest]\naddopts = -q\n")],
            true,
        ),
        (
            "pythonpath outside the pytest section",
            vec![("pytest.ini", "[other]\npythonpath = vendor.zip\n")],
            true,
        ),
        (
            "commented pythonpath",
            vec![("pytest.ini", "[pytest]\n# pythonpath = vendor.zip\n")],
            true,
        ),
        (
            "pyproject without pytest section",
            vec![("pyproject.toml", "[project]\nname = \"reach\"\n")],
            true,
        ),
    ];
    for (label, extra, credited) in shadow_cases {
        let mut files = vec![
            ("reach/__init__.py", ""),
            ("reach/ledger.py", functions),
            ("tests/test_ledger.py", test),
        ];
        for (path, text) in extra {
            files.retain(|(present, _)| *present != path);
            files.push((path, text));
        }
        let finding = classify(&files, "reach/ledger.py", 2, "    return 2", "    return 3")?;
        if credited {
            assert_eq!(
                finding.class,
                ExposureClass::Exposed,
                "{label}: {finding:?}"
            );
        } else {
            withheld(&finding, label);
        }
    }

    // Many locals and many earlier assertions: the pinned assertion still
    // credits.
    let mut test = String::from("from reach.ledger import Ledger\n\n\ndef test_render_ledger():\n");
    for local in 0..200 {
        test.push_str(&format!("    l{local} = Ledger()\n"));
    }
    for _ in 0..200 {
        test.push_str("    assert 1 == 1\n");
    }
    test.push_str("    assert l0.render_ledger() == \"|  |\"\n");
    let finding = ledger_finding(&new, &test)?;
    assert_eq!(finding.class, ExposureClass::Exposed, "{finding:?}");
    Ok(())
}
