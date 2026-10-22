//! Operator and `with` entries of dunder owners (#9421): the syntax binds to
//! the owner only where the receiver's class and the operator are certain.

use super::classify::{classify_change_with_old, classify_change_with_source};
use super::owners_tests::{extract_owners, extract_tests};
use super::related_tests::{apply_class_facts, related_test_candidates};
use super::{PythonOwner, PythonTest};
use crate::domain::{ExposureClass, Finding};
use std::path::Path;

type Check = Result<(), String>;

const MONEY: &str = "class Money:\n    def __init__(self, cents):\n        self.cents = cents\n\n    def __add__(self, other):\n        return Money(self.cents + other.cents)\n\n    def __eq__(self, other):\n        return isinstance(other, Money) and self.cents == other.cents\n\n    def __lt__(self, other):\n        return self.cents < other.cents\n";

const RECORDER: &str = "class Recorder:\n    def __init__(self):\n        self.events = []\n\n    def __enter__(self):\n        self.events.append(\"enter\")\n        return self\n\n    def __exit__(self, *exc):\n        self.events.append(\"exit\")\n        return False\n";

const MONEY_IMPORT: &str = "from shapes.money import Money\n\n\n";

fn owners_of(src: &str, file: &str) -> Vec<PythonOwner> {
    let mut owners = extract_owners(Path::new(file), src);
    apply_class_facts(&mut owners);
    owners
}

fn owner<'a>(owners: &'a [PythonOwner], name: &str) -> Result<&'a PythonOwner, String> {
    owners
        .iter()
        .find(|owner| owner.name == name)
        .ok_or_else(|| format!("owner {name} must be extracted"))
}

fn tests_of(src: &str) -> Result<Vec<PythonTest>, String> {
    let tests = extract_tests(Path::new("tests/test_money.py"), src);
    if tests.len() == 1 {
        Ok(tests)
    } else {
        Err("fixture must parse exactly one test".to_string())
    }
}

fn relations(
    owners: &[PythonOwner],
    name: &str,
    test_src: &str,
) -> Result<Vec<&'static str>, String> {
    let tests = tests_of(test_src)?;
    Ok(related_test_candidates(owner(owners, name)?, &tests)
        .iter()
        .map(|candidate| candidate.relation.as_str())
        .collect())
}

fn relation(
    owners: &[PythonOwner],
    name: &str,
    test_src: &str,
) -> Result<Option<&'static str>, String> {
    Ok(relations(owners, name, test_src)?.first().copied())
}

fn classify(
    src: &str,
    file: &str,
    name: &str,
    line_text: &str,
    old: &str,
    test_src: &str,
) -> Result<Finding, String> {
    let owners = owners_of(src, file);
    let line = owner(&owners, name)?.start_line + 1;
    let tests = tests_of(test_src)?;
    classify_change_with_old(Path::new(file), line, line_text, Some(old), &owners, &tests)
        .ok_or_else(|| "the change must yield a finding".to_string())
}

fn classify_add(test_src: &str) -> Result<Finding, String> {
    classify(
        MONEY,
        "shapes/money.py",
        "__add__",
        "        return Money(self.cents - other.cents)",
        "        return Money(self.cents + other.cents)",
        test_src,
    )
}

fn money_test(body: &str) -> String {
    format!("{MONEY_IMPORT}def test_it():\n{body}\n")
}

#[test]
fn operator_on_constructed_operands_credits_the_exact_comparison() -> Check {
    let finding = classify_add(&money_test("    assert Money(5) + Money(7) == Money(12)"))?;
    assert_eq!(finding.class, ExposureClass::Exposed);
    Ok(())
}

#[test]
fn operator_on_locals_bound_once_to_the_class_credits_the_attribute_read() -> Check {
    let owners = owners_of(MONEY, "shapes/money.py");
    let test_src = money_test("    a = Money(5)\n    b = Money(7)\n    assert (a + b).cents == 12");
    assert_eq!(
        relation(&owners, "__add__", &test_src)?,
        Some("protocol_entry_call")
    );
    assert_eq!(classify_add(&test_src)?.class, ExposureClass::Exposed);
    Ok(())
}

#[test]
fn operator_result_masked_by_a_weak_assertion_is_not_credited() -> Check {
    for body in [
        "    assert (Money(5) + Money(7)).cents >= 0",
        "    assert (Money(5) + Money(7)).cents",
        "    assert Money(5) + Money(7)",
    ] {
        let finding = classify_add(&money_test(body))?;
        assert_ne!(finding.class, ExposureClass::Exposed, "{body}");
    }
    Ok(())
}

#[test]
fn operator_that_is_not_certain_keeps_the_dunder_protocol_relation() -> Check {
    let owners = owners_of(MONEY, "shapes/money.py");
    let uncertain = [
        // The receiver is the right operand: Python runs `__radd__`.
        "    assert 5 + Money(7) == Money(12)",
        // The other operand's type is unknown and may be a subclass.
        "    assert Money(5) + other() == Money(12)",
        // A parameter-like name is not a bound instance.
        "    assert a + b == Money(12)",
        // The operator is part of a larger expression.
        "    assert Money(1) * 2 + Money(1) == Money(3)",
        "    assert Money(1) + Money(1) + Money(1) == Money(3)",
        // A wrapper may ignore the result.
        "    assert always_true(Money(5) + Money(7))",
        // The class is not the imported one.
        "    assert Other(5) + Other(7) == Money(12)",
        // Attribute access on a call result is not a grouping read.
        "    assert (Money(5) + Money(7)).scaled(2) == 24",
    ];
    for body in uncertain {
        assert_eq!(
            relation(&owners, "__add__", &money_test(body))?,
            Some("dunder_protocol"),
            "{body}"
        );
    }
    Ok(())
}

#[test]
fn operator_on_a_rebound_or_multiply_bound_local_is_not_bound() -> Check {
    let owners = owners_of(MONEY, "shapes/money.py");
    for body in [
        "    a = Money(5)\n    a = other\n    assert a + Money(7) == Money(12)",
        "    a = Money(5)\n    b = Money(7)\n    b = Money(8)\n    assert a + b == Money(13)",
        "    a, b = Money(5), Money(7)\n    assert a + b == Money(12)",
        "    for a in [Money(5)]:\n        assert a + Money(7) == Money(12)",
    ] {
        assert_eq!(
            relation(&owners, "__add__", &money_test(body))?,
            Some("dunder_protocol"),
            "{body}"
        );
    }
    Ok(())
}

#[test]
fn subclass_operand_is_not_bound() -> Check {
    let owners = owners_of(MONEY, "shapes/money.py");
    let test_src = format!(
        "{MONEY_IMPORT}class Sub(Money):\n    def __radd__(self, other):\n        return 0\n\n\ndef test_it():\n    assert Money(5) + Sub(7) == Money(12)\n"
    );
    let tests = extract_tests(Path::new("tests/test_money.py"), &test_src);
    let found: Vec<_> = related_test_candidates(owner(&owners, "__add__")?, &tests)
        .iter()
        .map(|candidate| candidate.relation.as_str())
        .collect();
    assert!(
        found.iter().all(|kind| *kind != "protocol_entry_call"),
        "{found:?}"
    );
    Ok(())
}

#[test]
fn not_equal_binds_the_eq_owner_only_when_the_class_defines_no_ne() -> Check {
    let with_ne =
        format!("{MONEY}\n    def __ne__(self, other):\n        return not self == other\n");
    let test_src = money_test("    assert Money(1) != Money(2)");
    let plain = owners_of(MONEY, "shapes/money.py");
    assert_eq!(
        relation(&plain, "__eq__", &test_src)?,
        Some("protocol_entry_call")
    );
    let overridden = owners_of(&with_ne, "shapes/money.py");
    assert_eq!(
        relation(&overridden, "__eq__", &test_src)?,
        Some("dunder_protocol")
    );
    assert_eq!(
        relation(&overridden, "__ne__", &test_src)?,
        Some("protocol_entry_call")
    );
    // Without the workspace pass the class's methods are unknown: fail closed.
    let unfilled = extract_owners(Path::new("shapes/money.py"), MONEY);
    assert_eq!(
        relation(&unfilled, "__eq__", &test_src)?,
        Some("dunder_protocol")
    );
    Ok(())
}

#[test]
fn weak_equality_read_of_an_eq_owner_abstains_instead_of_grading_a_gap() -> Check {
    // `Money(1) != 1` cannot tell a removed `isinstance` guard from the
    // original through the weak oracle, yet the guard's removal raises.
    let finding = classify(
        MONEY,
        "shapes/money.py",
        "__eq__",
        "        return self.cents == other.cents",
        "        return isinstance(other, Money) and self.cents == other.cents",
        &money_test("    assert Money(1) != 1"),
    )?;
    assert_eq!(finding.class, ExposureClass::StaticUnknown);
    Ok(())
}

#[test]
fn ordering_dunder_with_a_relational_assertion_is_a_gap_not_an_abstention() -> Check {
    let finding = classify(
        MONEY,
        "shapes/money.py",
        "__lt__",
        "        return self.cents <= other.cents",
        "        return self.cents < other.cents",
        &money_test("    assert Money(1) < Money(2)"),
    )?;
    assert_eq!(finding.class, ExposureClass::WeaklyExposed);
    Ok(())
}

fn classify_exit(test_src: &str) -> Result<Finding, String> {
    classify(
        &RECORDER.replace("\"exit\"", "\"leave\""),
        "shapes/ctx.py",
        "__exit__",
        "        self.events.append(\"leave\")",
        "        self.events.append(\"exit\")",
        test_src,
    )
}

fn recorder_test(body: &str) -> String {
    format!("from shapes.ctx import Recorder\n\n\ndef test_it():\n{body}\n")
}

#[test]
fn with_block_state_read_after_the_block_credits_the_exit_effect() -> Check {
    let finding = classify_exit(&recorder_test(
        "    with Recorder() as rec:\n        pass\n    assert rec.events == [\"enter\", \"exit\"]",
    ))?;
    assert_eq!(finding.class, ExposureClass::Exposed);
    let bound = classify_exit(&recorder_test(
        "    rec = Recorder()\n    with rec:\n        pass\n    assert rec.events == [\"enter\", \"exit\"]",
    ))?;
    assert_eq!(bound.class, ExposureClass::Exposed);
    Ok(())
}

#[test]
fn with_block_read_inside_the_block_does_not_credit_the_exit_effect() -> Check {
    let finding = classify_exit(&recorder_test(
        "    with Recorder() as rec:\n        assert rec.events == [\"enter\"]",
    ))?;
    assert_ne!(
        finding.class,
        ExposureClass::Exposed,
        "{:#?}",
        finding.evidence
    );
    Ok(())
}

#[test]
fn with_block_unrelated_assertion_is_a_gap_for_the_suppress_edit() -> Check {
    let finding = classify(
        RECORDER,
        "shapes/ctx.py",
        "__exit__",
        "        return True",
        "        return False",
        &recorder_test(
            "    with Recorder() as rec:\n        pass\n    assert rec.events == [\"enter\", \"exit\"]",
        ),
    )?;
    assert_eq!(finding.class, ExposureClass::WeaklyExposed);
    Ok(())
}

#[test]
fn class_shapes_that_may_run_another_method_are_not_bound() -> Check {
    let test_ne = money_test("    assert Money(1) != Money(2)");
    let test_add = money_test("    assert Money(5) + Money(7) == Money(12)");
    // A base class may define `__ne__`: `!=` abstains, `+` still binds.
    let based = MONEY.replace("class Money:", "class Money(tuple):");
    let owners = owners_of(&based, "shapes/money.py");
    assert_eq!(
        relation(&owners, "__eq__", &test_ne)?,
        Some("dunder_protocol")
    );
    assert_eq!(
        relation(&owners, "__add__", &test_add)?,
        Some("protocol_entry_call")
    );
    // A decorator, metaclass, class-level dunder assignment, conditional body
    // statement or second definition may regenerate or replace the method.
    let regenerating = [
        MONEY.replace("class Money:", "@attr.s\nclass Money:"),
        MONEY.replace("class Money:", "class Money(metaclass=Meta):"),
        format!("{MONEY}\n    __ne__ = other_ne\n"),
        format!("{MONEY}\n    if FLAG:\n        __ne__ = other_ne\n"),
        format!("{MONEY}\n\n{MONEY}"),
    ];
    for src in regenerating {
        let owners = owners_of(&src, "shapes/money.py");
        assert_eq!(
            relation(&owners, "__eq__", &test_ne)?,
            Some("dunder_protocol"),
            "{src}"
        );
        assert_eq!(
            relation(&owners, "__add__", &test_add)?,
            Some("dunder_protocol"),
            "{src}"
        );
    }
    Ok(())
}

#[test]
fn nested_def_or_lambda_parameter_named_like_an_instance_is_not_bound() -> Check {
    let owners = owners_of(MONEY, "shapes/money.py");
    for body in [
        "    a = Money(5)\n    def check(a):\n        assert a + Money(1) == Money(7)\n    check(Other())",
        "    a = Money(5)\n    def check(x, *a):\n        assert a + Money(1) == Money(7)",
        "    a = Money(5)\n    check = lambda a: a + Money(1) == Money(7)\n    assert check(Other())",
    ] {
        assert_eq!(
            relation(&owners, "__add__", &money_test(body))?,
            Some("dunder_protocol"),
            "{body}"
        );
    }
    Ok(())
}

#[test]
fn guarded_context_manager_write_is_not_credited() -> Check {
    let guarded = "class Recorder:\n    def __init__(self):\n        self.events = []\n\n    def __enter__(self):\n        self.events.append(\"enter\")\n        return self\n\n    def __exit__(self, t, v, tb):\n        if t is not None:\n            self.events.append(\"fault\")\n        return False\n";
    let finding = classify(
        guarded,
        "shapes/ctx.py",
        "__exit__",
        "            self.events.append(\"fault\")",
        "            self.events.append(\"error\")",
        &recorder_test(
            "    with Recorder() as rec:\n        pass\n    assert rec.events == [\"enter\"]",
        ),
    )?;
    assert_ne!(finding.class, ExposureClass::Exposed);
    Ok(())
}

#[test]
fn comment_or_docstring_text_does_not_end_a_with_block_early() -> Check {
    let owners = owners_of(RECORDER, "shapes/ctx.py");
    // A comment at a lower indent keeps the block open: the read inside it is
    // not an after-block read.
    let commented = classify_exit(&recorder_test(
        "    with Recorder() as rec:\n# note\n        assert rec.events == [\"enter\"]",
    ))?;
    assert_ne!(commented.class, ExposureClass::Exposed);
    // Triple-quoted text after the statement cannot be read through.
    assert_eq!(
        relation(
            &owners,
            "__exit__",
            &recorder_test(
                "    with Recorder() as rec:\n        pass\n    \"\"\"\nnote\n\"\"\"\n    assert rec.events == [\"enter\", \"exit\"]"
            )
        )?,
        Some("dunder_protocol")
    );
    Ok(())
}

#[test]
fn with_entry_whose_target_may_not_be_the_manager_is_not_bound() -> Check {
    let other_target = RECORDER.replace("        return self\n", "        return []\n");
    let owners = owners_of(&other_target, "shapes/ctx.py");
    assert_eq!(
        relation(
            &owners,
            "__exit__",
            &recorder_test(
                "    with Recorder() as rec:\n        pass\n    assert rec == [\"enter\", \"exit\"]"
            )
        )?,
        Some("dunder_protocol")
    );
    // Entering twice cannot be attributed to one statement.
    let owners = owners_of(RECORDER, "shapes/ctx.py");
    assert_eq!(
        relation(
            &owners,
            "__exit__",
            &recorder_test(
                "    with Recorder() as rec:\n        pass\n    with Recorder() as rec:\n        pass\n    assert rec.events == [\"exit\"]"
            )
        )?,
        Some("dunder_protocol")
    );
    // A multi-item `with` may enter another object first.
    assert_eq!(
        relation(
            &owners,
            "__exit__",
            &recorder_test(
                "    with open_thing(), Recorder() as rec:\n        pass\n    assert rec.events == [\"exit\"]"
            )
        )?,
        Some("dunder_protocol")
    );
    Ok(())
}

const NOTER: &str = "class Recorder:\n    def __init__(self):\n        self.events = []\n        self.last = None\n\n    def note(self, kind):\n        if kind == \"error\":\n            self.last = \"error\"\n            self.events.append(\"error\")\n";

fn noter_test(body: &str) -> String {
    format!("from shapes.noter import Recorder\n\n\ndef test_it():\n{body}\n")
}

fn classify_note(line_text: &str, old: &str, test_src: &str) -> Result<Finding, String> {
    classify(
        &NOTER.replace(old.trim(), line_text.trim()),
        "shapes/noter.py",
        "note",
        line_text,
        old,
        test_src,
    )
}

#[test]
fn guarded_self_field_write_in_an_ordinary_method_is_not_credited() -> Check {
    // The test never takes the `if kind == "error"` arm, so the edited write
    // never runs and an exact read of the untouched state cannot notice it.
    let never_runs =
        noter_test("    rec = Recorder()\n    rec.note(\"ok\")\n    assert rec.last == None");
    let finding = classify_note(
        "            self.last = \"fault\"",
        "            self.last = \"error\"",
        &never_runs,
    )?;
    assert_ne!(finding.class, ExposureClass::Exposed);
    Ok(())
}

#[test]
fn unguarded_self_field_write_in_an_ordinary_method_stays_credited() -> Check {
    let direct = "class Recorder:\n    def __init__(self):\n        self.last = None\n\n    def note(self, kind):\n        self.last = \"fault\"\n";
    let finding = classify(
        direct,
        "shapes/noter.py",
        "note",
        "        self.last = \"fault\"",
        "        self.last = \"error\"",
        &noter_test("    rec = Recorder()\n    rec.note(\"ok\")\n    assert rec.last == \"error\""),
    )?;
    assert_eq!(finding.class, ExposureClass::Exposed);
    Ok(())
}

/// Classify an edit of `note` with the new-side source threaded in, as the
/// diff path does, so the guard readers can see the enclosing `if`.
fn classify_note_with_source(
    source: &str,
    after: &str,
    before: &str,
    test_src: &str,
) -> Result<Finding, String> {
    let file = Path::new("shapes/noter.py");
    let owners = owners_of(source, "shapes/noter.py");
    let (index, written) = source
        .lines()
        .enumerate()
        .find(|(_, text)| text.trim() == after.trim())
        .ok_or_else(|| "the edited line must be in the source".to_string())?;
    let indent = &written[..written.len() - written.trim_start().len()];
    let before = format!("{indent}{}", before.trim());
    classify_change_with_source(
        file,
        index + 1,
        written,
        Some(&before),
        &owners,
        &tests_of(test_src)?,
        Some(source),
    )
    .ok_or_else(|| "the change must yield a finding".to_string())
}

const GUARDED_AFTER: &str = "            self.last = \"fault\"";
const GUARDED_BEFORE: &str = "            self.last = \"error\"";

fn guarded_note_source(preamble: &str, guard: &str) -> String {
    format!(
        "class Recorder:\n    def __init__(self):\n        self.last = None\n\n    def note(self, kind):\n{preamble}{guard}\n{GUARDED_AFTER}\n"
    )
}

fn classify_guarded(source: &str, body: &str) -> Result<Finding, String> {
    classify_note_with_source(source, GUARDED_AFTER, GUARDED_BEFORE, &noter_test(body))
}

#[test]
fn guarded_self_field_write_is_credited_when_every_call_enters_the_guard() -> Result<(), String> {
    let source = guarded_note_source("", "        if kind == \"error\":");
    let finding = classify_guarded(
        &source,
        "    rec = Recorder()\n    rec.note(\"error\")\n    assert rec.last == \"error\"",
    )?;
    assert_eq!(finding.class, ExposureClass::Exposed);
    Ok(())
}

#[test]
fn guarded_self_field_write_stays_uncredited_when_a_call_skips_the_guard() -> Result<(), String> {
    let source = guarded_note_source("", "        if kind == \"error\":");
    // The #9782 repro: the only call skips the guard.
    let skips = classify_guarded(
        &source,
        "    rec = Recorder()\n    rec.note(\"ok\")\n    assert rec.last == None",
    )?;
    assert_ne!(skips.class, ExposureClass::Exposed);
    // One call enters and one skips: the read cannot be tied to the entering call.
    let mixed = classify_guarded(
        &source,
        "    rec = Recorder()\n    rec.note(\"error\")\n    other = Recorder()\n    other.note(\"ok\")\n    assert rec.last == \"error\"",
    )?;
    assert_ne!(mixed.class, ExposureClass::Exposed);
    Ok(())
}

#[test]
fn guarded_self_field_write_stays_uncredited_for_shapes_the_reader_does_not_model()
-> Result<(), String> {
    let entering =
        "    rec = Recorder()\n    rec.note(\"error\")\n    assert rec.last == \"error\"";
    // An `elif` arm is reached only when an earlier arm was not taken.
    let elif = guarded_note_source(
        "        if kind == \"\":\n            self.last = \"empty\"\n",
        "        elif kind == \"error\":",
    );
    // A loop may run zero times, and an earlier exit may leave first.
    let looped = "class Recorder:\n    def __init__(self):\n        self.last = None\n\n    def note(self, kind):\n        for _ in range(kind):\n            if kind == \"error\":\n                self.last = \"fault\"\n".to_string();
    let early_exit = guarded_note_source(
        "        if kind == \"\":\n            return\n",
        "        if kind == \"error\":",
    );
    // An entered outer guard does not enter the inner one, an `else` arm is
    // taken by inputs the guard rejects, and `while` or `with` before the
    // write can loop on or swallow an error.
    let head = "class Recorder:\n    def __init__(self):\n        self.last = None\n\n    def note(self, kind):\n";
    let nested = format!(
        "{head}        if kind == \"error\":\n            if kind == \"fault\":\n                self.last = \"fault\"\n"
    );
    let else_arm = format!(
        "{head}        if kind == \"ok\":\n            self.last = None\n        else:\n            self.last = \"fault\"\n"
    );
    let while_before = guarded_note_source(
        "        while kind != \"error\":\n            kind = \"error\"\n",
        "        if kind == \"error\":",
    );
    let with_before = guarded_note_source(
        "        with open(kind):\n            pass\n",
        "        if kind == \"error\":",
    );
    for source in [
        elif,
        looped,
        early_exit,
        nested,
        else_arm,
        while_before,
        with_before,
    ] {
        let finding = classify_guarded(&source, entering)?;
        assert_ne!(finding.class, ExposureClass::Exposed, "{source}");
    }
    Ok(())
}
