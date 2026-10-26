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

const OLD_GUARD: &str = "        return isinstance(other, Money) and self.cents == other.cents";

/// The guarded line the edit started from: the edited line with the
/// `isinstance(other, Money)` operand guard put back, so a control that changes
/// the remaining expression is still a pure guard removal.
fn guarded_form(new_line: &str) -> String {
    new_line.replacen("return ", "return isinstance(other, Money) and ", 1)
}

/// Classify the `__eq__` edit against the file as it reads after the edit,
/// which is what the workspace pass hands the classifier.
fn classify_guard_removal(
    new_src: &str,
    new_line: &str,
    test_src: &str,
) -> Result<Finding, String> {
    let owners = owners_of(new_src, "shapes/money.py");
    let line = owner(&owners, "__eq__")?.start_line + 1;
    let tests = tests_of(test_src)?;
    let old_line = guarded_form(new_line);
    classify_change_with_source(
        Path::new("shapes/money.py"),
        line,
        new_line,
        Some(&old_line),
        &owners,
        &tests,
        Some(new_src),
    )
    .ok_or_else(|| "the change must yield a finding".to_string())
}

fn money_without_guard() -> String {
    MONEY.replace(OLD_GUARD.trim_start(), "return self.cents == other.cents")
}

const NEW_GUARD_LINE: &str = "        return self.cents == other.cents";

#[test]
fn removed_type_guard_is_credited_to_a_foreign_literal_comparison() -> Check {
    for assertion in [
        "assert Money(1) != 1",
        "assert Money(1) != 'x'",
        "assert not Money(1) == None",
    ] {
        let finding = classify_guard_removal(
            &money_without_guard(),
            NEW_GUARD_LINE,
            &money_test(&format!("    {assertion}")),
        )?;
        assert_eq!(finding.class, ExposureClass::Exposed, "{assertion}");
    }
    Ok(())
}

/// A builtin base answers `!=` itself, so a mixed test whose `==` assertion
/// reaches the edited method must not lend its relation to the `!=` one.
#[test]
fn removed_type_guard_ignores_a_ne_assertion_on_a_builtin_subclass() -> Check {
    for (base, literal) in [("int", "2"), ("str", "'x'"), ("float", "2.5")] {
        let source =
            money_without_guard().replace("class Money:", &format!("class Money({base}):"));
        let test = money_test("    assert Money(1) == Money(1)\n    assert Money(1) != LITERAL")
            .replace("LITERAL", literal);
        let finding = classify_guard_removal(&source, NEW_GUARD_LINE, &test)?;
        assert_ne!(finding.class, ExposureClass::Exposed, "{base}");
    }
    Ok(())
}

#[test]
fn removed_type_guard_stays_an_abstention_without_the_exact_shape() -> Check {
    let unguarded = money_without_guard();
    let foreign = money_test("    assert Money(1) != 1");
    let with_ne = unguarded.replace(
        "    def __lt__",
        "    def __ne__(self, other):\n        return True\n\n    def __lt__",
    );
    let with_prelude = unguarded.replace(
        "        return self.cents == other.cents",
        "        if other is None:\n            return False\n        return self.cents == other.cents",
    );
    for (name, source, line, test_src) in [
        (
            "an __ne__ override",
            with_ne.as_str(),
            NEW_GUARD_LINE,
            foreign.clone(),
        ),
        (
            "an early exit before the return",
            with_prelude.as_str(),
            NEW_GUARD_LINE,
            foreign.clone(),
        ),
        (
            "a builtin attribute read",
            &unguarded.replace("other.cents", "other.real"),
            "        return self.cents == other.real",
            foreign.clone(),
        ),
        (
            "a same-type comparison",
            unguarded.as_str(),
            NEW_GUARD_LINE,
            money_test("    assert Money(1) != Money(2)"),
        ),
        (
            "an == read that expects a match",
            unguarded.as_str(),
            NEW_GUARD_LINE,
            money_test("    assert Money(1) == Money(1)"),
        ),
        (
            "a builtin string method read off a str operand",
            &unguarded
                .replace(
                    "self.cents = cents",
                    "self.cents = cents\n        self.upper = cents",
                )
                .replace("other.cents", "other.upper"),
            "        return self.cents == other.upper",
            money_test("    assert Money(1) != 'x'"),
        ),
        (
            "an int subclass, whose inherited __ne__ skips the edited __eq__",
            &unguarded.replace("class Money:", "class Money(int):"),
            NEW_GUARD_LINE,
            foreign.clone(),
        ),
        (
            "a builtin attribute added in a newer Python",
            &unguarded
                .replace(
                    "self.cents = cents",
                    "self.cents = cents\n        self.from_number = cents",
                )
                .replace("other.cents", "other.from_number"),
            "        return self.cents == other.from_number",
            money_test("    assert Money(1) != 1.5"),
        ),
        (
            "a short-circuited read",
            &unguarded.replace("self.cents == other.cents", "False and other.cents"),
            "        return False and other.cents",
            foreign.clone(),
        ),
        (
            "a test that catches the AttributeError",
            unguarded.as_str(),
            NEW_GUARD_LINE,
            money_test(
                "    try:\n        assert Money(1) != 1\n    except AttributeError:\n        pass",
            ),
        ),
        (
            "a literal on the left, which the operator entry does not bind",
            unguarded.as_str(),
            NEW_GUARD_LINE,
            money_test("    assert 1 != Money(1)"),
        ),
        (
            "a list operand",
            unguarded.as_str(),
            NEW_GUARD_LINE,
            money_test("    assert Money(1) != [1]"),
        ),
    ] {
        let finding = classify_guard_removal(source, line, &test_src)?;
        assert_ne!(finding.class, ExposureClass::Exposed, "{name}");
    }
    // A guard for another class is not this owner's operand guard.
    let owners = owners_of(&unguarded, "shapes/money.py");
    let line_no = owner(&owners, "__eq__")?.start_line + 1;
    let tests = tests_of(&foreign)?;
    let finding = classify_change_with_source(
        Path::new("shapes/money.py"),
        line_no,
        NEW_GUARD_LINE,
        Some("        return isinstance(other, Cash) and self.cents == other.cents"),
        &owners,
        &tests,
        Some(&unguarded),
    )
    .ok_or("the change must yield a finding")?;
    assert_ne!(finding.class, ExposureClass::Exposed);
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
        "    rec = Recorder()\n    rec.note(\"error\")\n    rec.note(\"ok\")\n    assert rec.last == \"error\"",
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

#[test]
fn guarded_self_field_credit_refuses_native_escapes_and_binding_changes() -> Check {
    let unchanged = "    rec = Recorder()\n    rec.note(\"error\")\n    assert rec.last == None";
    for preamble in [
        "        if kind == \"error\":\n            marker = \"\\\"\"; return\n",
        "        (kind) = \"ok\"\n",
        "        (\n            kind\n        ) = \"ok\"\n",
        "        other, (kind) = 1, \"ok\"\n",
        "        𝓴𝓲𝓷𝓭 = \"ok\"\n",
        "        (self) = Recorder()\n",
        "        [self] = [Recorder()]\n",
        "        𝓼𝓮𝓵𝓯 = Recorder()\n",
        "        if kind == \"error\":\n            marker = \"\\\"\"; yield 1\n",
        "        marker = f\"{exit() if kind == 'error' else 0}\"\n",
    ] {
        let source = guarded_note_source(preamble, "        if kind == \"error\":");
        assert_ne!(
            classify_guarded(&source, unchanged)?.class,
            ExposureClass::Exposed,
            "{source}"
        );
    }
    let source = guarded_note_source(
        "",
        "        if kind == \"error\":\n            self = Recorder()",
    );
    assert_ne!(
        classify_guarded(&source, unchanged)?.class,
        ExposureClass::Exposed
    );
    for escape in ["raise ValueError()", "assert False", "exit()"] {
        let source = guarded_note_source(
            &format!("        if kind == \"error\":\n            marker = \"\\\"\"; {escape}\n"),
            "        if kind == \"error\":",
        );
        let catches = "    rec = Recorder()\n    try:\n        rec.note(\"error\")\n    except (ValueError, AssertionError, SystemExit):\n        pass\n    assert rec.last == None";
        assert_ne!(
            classify_guarded(&source, catches)?.class,
            ExposureClass::Exposed,
            "{source}"
        );
    }
    Ok(())
}

#[test]
fn guarded_self_field_credit_ignores_nonexecuting_keyword_text() -> Check {
    let entering =
        "    rec = Recorder()\n    rec.note(\"error\")\n    assert rec.last == \"error\"";
    for preamble in [
        "        marker = 1  # return raise while with\n",
        "        marker = \"return raise while with\"\n",
        "        \"\"\"Notes\nreturn raise while with\n        \"\"\"\n",
    ] {
        let source = guarded_note_source(preamble, "        if kind == \"error\":");
        assert_eq!(
            classify_guarded(&source, entering)?.class,
            ExposureClass::Exposed,
            "{source}"
        );
    }
    let source = guarded_note_source(
        "",
        "        if kind == \"error\":  # return is only a comment",
    );
    assert_eq!(
        classify_guarded(&source, entering)?.class,
        ExposureClass::Exposed
    );
    Ok(())
}

#[test]
fn guarded_self_field_credit_refuses_mixed_binding_targets_and_non_space_indents() -> Check {
    let unchanged = "    rec = Recorder()\n    rec.note(\"error\")\n    assert rec.last == None";
    for preamble in [
        "        other = Recorder()\n        (kind), other.last = \"ok\", None\n",
        "        other = {}\n        (kind), other[\"x\"] = \"ok\", None\n",
        "        other = Recorder()\n        (self), other.last = Recorder(), None\n",
        "        other = {}\n        (self), other[\"x\"] = Recorder(), None\n",
    ] {
        let source = guarded_note_source(preamble, "        if kind == \"error\":");
        assert_ne!(
            classify_guarded(&source, unchanged)?.class,
            ExposureClass::Exposed,
            "{source}"
        );
    }
    let prefix = "class Recorder:\n    def __init__(self):\n        self.last = None\n\n    def note(self, kind):\n        if kind == \"error\":\n            pass\n";
    for branch in ["else:", "elif kind == \"error\":"] {
        let source = format!(
            "{prefix}{}        {branch}\n            if kind == \"error\":\n                self.last = \"fault\"\n",
            "\u{c}".repeat(12)
        );
        assert_ne!(
            classify_guarded(&source, unchanged)?.class,
            ExposureClass::Exposed
        );
    }
    Ok(())
}

#[test]
fn guarded_self_field_credit_requires_the_value_to_survive_to_the_assertion() -> Check {
    let source = guarded_note_source("", "        if kind == \"error\":");
    let settled =
        "    rec = Recorder()\n    rec.note(\"error\")\n    assert rec.last == \"settled\"";
    for tail in [
        "            self.last = \"settled\"\n",
        "        self.last = \"settled\"\n",
        "        alias = self\n        alias.last = \"settled\"\n",
        "        (self.last) = \"settled\"\n",
        "        self.last: str = \"settled\"\n",
        "        other = {}\n        (self.last), other[\"x\"] = \"settled\", None\n",
        "        for self.last in [\"settled\"]:\n            pass\n",
        "        for (marker, self.last) in [(0, \"settled\")]:\n            pass\n",
        "        marker = [0 for self.last in [\"settled\"]]\n",
    ] {
        assert_ne!(
            classify_guarded(&format!("{source}{tail}"), settled)?.class,
            ExposureClass::Exposed,
            "{tail}"
        );
    }
    for body in [
        "    rec = Recorder()\n    rec.note(\"error\")\n    rec.last = \"settled\"\n    assert rec.last == \"settled\"",
        "    rec = Recorder()\n    alias = rec\n    rec.note(\"error\")\n    alias.last = \"settled\"\n    assert rec.last == \"settled\"",
        "    rec = Recorder()\n    rec.note(\"error\")\n    (rec.last) = \"settled\"\n    assert rec.last == \"settled\"",
        "    rec = Recorder()\n    rec.last = rec.note(\"error\")\n    assert rec.last == None",
        "    rec = Recorder()\n    rec.last: str = rec.note(\"error\")\n    assert rec.last == None",
        "    rec = Recorder()\n    (marker, rec.last) = (0, rec.note(\"error\"))\n    assert rec.last == None",
        "    rec = Recorder()\n    rec.note(\"error\")\n    rec.last *= 0\n    assert rec.last == \"\"",
        "    rec = Recorder()\n    rec.note(\"error\")\n    setattr(rec, \"last\", \"settled\")\n    assert rec.last == \"settled\"",
        "    rec = Recorder()\n    rec.note(\"error\")\n    del rec.last\n    try:\n        assert rec.last == None\n    except AttributeError:\n        pass",
    ] {
        assert_ne!(
            classify_guarded(&source, body)?.class,
            ExposureClass::Exposed,
            "{body}"
        );
    }
    let catches = "    rec = Recorder()\n    rec.note(\"error\")\n    try:\n        assert rec.last == None\n    except AttributeError:\n        pass";
    assert_ne!(
        classify_guarded(&format!("{source}        del self.last\n"), catches)?.class,
        ExposureClass::Exposed
    );
    Ok(())
}

#[test]
fn guarded_self_field_credit_keeps_unrelated_writes_and_read_only_uses() -> Check {
    let source = guarded_note_source("", "        if kind == \"error\":");
    let entering =
        "    rec = Recorder()\n    rec.note(\"error\")\n    assert rec.last == \"error\"";
    for tail in [
        "        self.other = \"settled\"\n",
        "        marker = self.last\n",
        "        if self.last:\n            marker = 1\n",
        "        marker = {self.last: 1}\n",
        "        marker = \"abcd\"[len(self.last):]\n",
        "        print(self.last, end=\"\")\n",
        "        self.last: str\n",
        "        marker = 1\n        del marker\n",
        "        self.other = 1\n        del self.other\n",
        "        pass\n",
    ] {
        assert_eq!(
            classify_guarded(&format!("{source}{tail}"), entering)?.class,
            ExposureClass::Exposed,
            "{tail}"
        );
    }
    for body in [
        "    rec = Recorder()\n    rec.last = None\n    rec.note(\"error\")\n    assert rec.last == \"error\"",
        "    rec = Recorder()\n    rec.last = None; rec.note(\"error\")\n    assert rec.last == \"error\"",
        "    rec = Recorder()\n    rec.note(\"error\")\n    rec.other = \"settled\"\n    assert rec.last == \"error\"",
        "    rec = Recorder()\n    rec.note(\"error\")\n    assert rec.last == \"error\"\n    rec.last = None",
    ] {
        assert_eq!(
            classify_guarded(&source, body)?.class,
            ExposureClass::Exposed,
            "{body}"
        );
    }
    Ok(())
}

#[test]
fn guarded_self_field_credit_preserves_benign_prefix_expressions_and_scopes() -> Check {
    let entering =
        "    rec = Recorder()\n    rec.note(\"error\")\n    assert rec.last == \"error\"";
    for preamble in [
        "        marker = kind == \"error\" or kind == \"ok\"\n",
        "        marker = not kind\n",
        "        marker = (other := 1)\n",
        "        for marker in []:\n            pass\n",
        "        def marker():\n            pass\n",
        "        def helper(kind):\n            pass\n",
        "\t# comment-only indentation does not make an ancestor\n",
        "        \"\"\"Notes\n\treturn raise with\n        \"\"\"\n",
        "        class Marker:\n            pass\n",
        "        import math\n",
        "        marker = 1\n        del marker\n",
    ] {
        let source = guarded_note_source(preamble, "        if kind == \"error\":");
        assert_eq!(
            classify_guarded(&source, entering)?.class,
            ExposureClass::Exposed,
            "{source}"
        );
    }
    Ok(())
}

#[test]
fn guarded_self_field_credit_refuses_native_binding_declarations() -> Check {
    let imported = guarded_note_source("        import os.path\n", "        if kind == \"error\":")
        .replace("kind", "os");
    let body = "    rec = Recorder()\n    rec.note(\"error\")\n    assert rec.last == None";
    assert_ne!(
        classify_guarded(&imported, body)?.class,
        ExposureClass::Exposed
    );
    let unchanged = "    rec = Recorder()\n    rec.note(\"error\")\n    assert rec.last == None";
    for preamble in [
        "        marker = (kind := \"ok\")\n",
        "        def helper(arg=(kind := \"ok\")):\n            pass\n",
        "        for kind in [\"ok\"]:\n            pass\n",
        "        for self in [Recorder()]:\n            pass\n",
        "        def kind():\n            pass\n",
        "        class self:\n            pass\n",
        "        import os as self\n",
        "        from os import path as kind\n",
        "        other = {}\n        for other[0 in [0]], self in [(0, Recorder())]:\n            pass\n",
    ] {
        let source = guarded_note_source(preamble, "        if kind == \"error\":");
        assert_ne!(
            classify_guarded(&source, unchanged)?.class,
            ExposureClass::Exposed,
            "{source}"
        );
    }
    Ok(())
}

#[test]
fn guarded_self_field_credit_refuses_same_line_masks_and_unresolved_native_literals() -> Check {
    for (guard, argument) in [
        (r#"        if kind != "\x65rror":"#, r#""error""#),
        ("        if kind != 0.10000000000000001:", "0.1"),
        ("        if kind != 1.0000000000000001:", "True"),
        ("        if kind == 9007199254740993:", "9007199254740993.0"),
        ("        if kind == 9007199254740993.0:", "9007199254740993"),
        ("        if kind <= 9007199254740992.0:", "9007199254740993"),
    ] {
        let source = guarded_note_source("", guard);
        let body =
            format!("    rec = Recorder()\n    rec.note({argument})\n    assert rec.last == None");
        assert_ne!(
            classify_guarded(&source, &body)?.class,
            ExposureClass::Exposed,
            "{source}"
        );
    }
    for tail in ["; self.last = \"settled\"", "; del self.last"] {
        let source = guarded_note_source("", "        if kind == \"error\":").replace(
            "self.last = \"fault\"",
            &format!("self.last = \"fault\"{tail}"),
        );
        let finding = classify_note_with_source(
            &source,
            &format!("self.last = \"fault\"{tail}"),
            &format!("self.last = \"error\"{tail}"),
            &noter_test(
                "    rec = Recorder()\n    rec.note(\"error\")\n    try:\n        assert rec.last == \"settled\"\n    except AttributeError:\n        pass",
            ),
        )?;
        assert_ne!(finding.class, ExposureClass::Exposed, "{source}");
    }
    Ok(())
}
