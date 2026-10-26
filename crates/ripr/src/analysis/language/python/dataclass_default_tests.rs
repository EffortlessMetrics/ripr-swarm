//! A changed numeric default of a plain `@dataclass` field (#9619): the credit
//! is narrow, so most of these cases are controls that must keep the
//! `decorator_indirection` abstention. A wrong credit is worse than a missed
//! one, so every control changes exactly one condition of the credited shape.

use super::classify::{PythonNoBehaviorContext, classify_change_with_context};
use super::owners_tests::{extract_owners, extract_tests};
use super::related_tests::apply_class_facts;
use crate::domain::{ExposureClass, Finding};
use std::path::Path;

type Check = Result<(), String>;

const MODELS: &str = "from dataclasses import dataclass, field\n\n\n@dataclass\nclass Invoice:\n    amount: int\n    tax_rate: float = 0.25\n    tags: list = field(default_factory=list)\n\n    def total(self):\n        return round(self.amount * (1 + self.tax_rate))\n";
const OLD_LINE: &str = "    tax_rate: float = 0.2";
const NEW_LINE: &str = "    tax_rate: float = 0.25";
const TEST_HEAD: &str = "from shapes.models import Invoice\n\n\n";

fn test_file(body: &str) -> String {
    format!("{TEST_HEAD}def test_it():\n{body}\n")
}

fn classify_edit(
    source: &str,
    old_line: &str,
    new_line: &str,
    test_src: &str,
    sole: bool,
) -> Result<Finding, String> {
    let mut owners = extract_owners(Path::new("shapes/models.py"), source);
    apply_class_facts(&mut owners);
    let line = source
        .lines()
        .position(|text| text == new_line)
        .ok_or_else(|| "the edited line must be in the source".to_string())?
        + 1;
    let tests = extract_tests(Path::new("tests/test_models.py"), test_src);
    let context = PythonNoBehaviorContext {
        sole_line_replacement: sole,
        ..PythonNoBehaviorContext::default()
    };
    classify_change_with_context(
        Path::new("shapes/models.py"),
        line,
        new_line,
        Some(old_line),
        &owners,
        &tests,
        context,
        Some(source),
    )
    .ok_or_else(|| "the edit must yield a finding".to_string())
}

fn classify_rate(source: &str, test_src: &str) -> Result<Finding, String> {
    classify_edit(source, OLD_LINE, NEW_LINE, test_src, true)
}

fn assert_credited(finding: &Finding) {
    assert_eq!(finding.class, ExposureClass::Exposed, "{finding:?}");
    assert!(
        !finding
            .evidence
            .iter()
            .any(|line| line.contains("static_limit")),
        "{:?}",
        finding.evidence
    );
}

fn assert_abstains(finding: &Finding, context: &str) {
    assert_eq!(finding.class, ExposureClass::StaticUnknown, "{context}");
}

fn assert_test_abstains(body: &str) -> Check {
    let finding = classify_rate(MODELS, &test_file(&format!("    {body}")))?;
    assert_abstains(&finding, body);
    Ok(())
}

fn assert_class_abstains(source: &str, context: &str) -> Check {
    let finding = classify_rate(source, &test_file("    assert Invoice(100).total() == 120"))?;
    assert_abstains(&finding, context);
    Ok(())
}

#[test]
fn a_test_that_omits_the_field_notices_the_new_default() -> Check {
    let finding = classify_rate(MODELS, &test_file("    assert Invoice(100).total() == 120"))?;
    assert_credited(&finding);
    assert!(
        finding.evidence.iter().any(|line| line.contains("test_it")),
        "{:?}",
        finding.evidence
    );
    // A trailing comma is outside the literal-argument shape.
    assert_test_abstains("assert Invoice(100, ).total() == 120")
}

#[test]
fn a_test_that_passes_the_field_never_reads_the_default() -> Check {
    for body in [
        "assert Invoice(100, 0.2).total() == 120",
        "assert Invoice(100, tax_rate=0.2).total() == 120",
        "assert Invoice(100, 0.5).total() == 150",
        "assert Invoice(amount=100).total() == 120",
        "assert Invoice(100, 0.2, []).total() == 120",
    ] {
        assert_test_abstains(body)?;
    }
    Ok(())
}

#[test]
fn an_assertion_the_edit_does_not_move_is_not_credited() -> Check {
    for body in [
        // Fails before the edit already.
        "assert Invoice(100).total() == 125",
        "assert Invoice(100).total() == 0",
        // Weak shapes never decided by a value.
        "assert Invoice(100).total() > 0",
        "assert Invoice(100).total()",
        "assert Invoice(100).total() != 0",
        // Holds under both defaults: round(100 * 1.25) is 125, so use a
        // zero amount, where the rate is multiplied away.
        "assert Invoice(0).total() == 0",
    ] {
        assert_test_abstains(body)?;
    }
    Ok(())
}

#[test]
fn a_small_default_change_that_rounds_to_the_same_result_is_not_credited() -> Check {
    let source = MODELS.replace("0.25", "0.204");
    let finding = classify_edit(
        &source,
        OLD_LINE,
        "    tax_rate: float = 0.204",
        &test_file("    assert Invoice(100).total() == 120"),
        true,
    )?;
    assert_abstains(&finding, "0.2 -> 0.204 still rounds to 120");
    Ok(())
}

#[test]
fn the_required_fields_must_be_supplied() -> Check {
    for body in [
        "assert Invoice().total() == 120",
        "assert Invoice(1, 2, [], 4).total() == 120",
        "assert Invoice(100, 'x').total() == 120",
        "assert Invoice(100).total(1) == 120",
        "assert Invoice(100).tax == 120",
        "assert Invoice(100).total() == total",
    ] {
        assert_test_abstains(body)?;
    }
    Ok(())
}

#[test]
fn a_test_that_could_change_or_swallow_the_call_is_not_credited() -> Check {
    for test_src in [
        // More than the one assertion.
        test_file("    invoice = Invoice(100)\n    assert invoice.total() == 120"),
        test_file("    Invoice.tax_rate = 0.2\n    assert Invoice(100).total() == 120"),
        test_file("    try:\n        assert Invoice(100).total() == 120\n    except Exception:\n        pass"),
        test_file("    with pytest.raises(ValueError):\n        assert Invoice(100).total() == 120"),
        // Decorated, parametrized or fixture-driven tests.
        format!(
            "{TEST_HEAD}@pytest.mark.skip\ndef test_it():\n    assert Invoice(100).total() == 120\n"
        ),
        format!("{TEST_HEAD}def test_it(rate):\n    assert Invoice(100).total() == 120\n"),
        format!(
            "{TEST_HEAD}@pytest.mark.parametrize('n', [1])\ndef test_it(n):\n    assert Invoice(100).total() == 120\n"
        ),
        // The class is not the imported one.
        "from other.models import Invoice\n\n\ndef test_it():\n    assert Invoice(100).total() == 120\n".to_string(),
        "from shapes.models import Invoice as Bill\n\n\ndef test_it():\n    assert Bill(100).total() == 120\n".to_string(),
        "from shapes.models import Invoice\nfrom other.models import Invoice\n\n\ndef test_it():\n    assert Invoice(100).total() == 120\n".to_string(),
        "\n\ndef test_it():\n    assert Invoice(100).total() == 120\n".to_string(),
    ] {
        let finding = classify_rate(MODELS, &test_src)?;
        assert_abstains(&finding, &test_src);
    }
    Ok(())
}

#[test]
fn a_class_that_is_not_a_plain_dataclass_is_not_credited() -> Check {
    for (name, source) in [
        (
            "frozen",
            MODELS.replace("@dataclass\n", "@dataclass(frozen=True)\n"),
        ),
        (
            "init off",
            MODELS.replace("@dataclass\n", "@dataclass(init=False)\n"),
        ),
        (
            "kw only",
            MODELS.replace("@dataclass\n", "@dataclass(kw_only=True)\n"),
        ),
        (
            "stacked decorator",
            MODELS.replace("@dataclass\n", "@register\n@dataclass\n"),
        ),
        (
            "other decorator",
            MODELS.replace("@dataclass\n", "@attrs\n"),
        ),
        (
            "foreign dataclass",
            MODELS.replace(
                "from dataclasses import",
                "from pydantic.dataclasses import",
            ),
        ),
        (
            "rebound dataclass",
            MODELS.replace(
                "\n\n\n@dataclass",
                "\ndataclass = wrap(dataclass)\n\n\n@dataclass",
            ),
        ),
        (
            "base class",
            MODELS.replace("class Invoice:", "class Invoice(Base):"),
        ),
        (
            "metaclass",
            MODELS.replace("class Invoice:", "class Invoice(metaclass=Meta):"),
        ),
        (
            "post init",
            format!("{MODELS}\n    def __post_init__(self):\n        self.tax_rate = 0.1\n"),
        ),
        (
            "own init",
            format!("{MODELS}\n    def __init__(self, amount):\n        self.amount = amount\n"),
        ),
        (
            "new",
            format!("{MODELS}\n    def __new__(cls, *args):\n        return object.__new__(cls)\n"),
        ),
        (
            "setattr",
            format!("{MODELS}\n    def __setattr__(self, name, value):\n        pass\n"),
        ),
        (
            "getattribute",
            format!("{MODELS}\n    def __getattribute__(self, name):\n        return 1\n"),
        ),
        (
            "second class",
            format!("{MODELS}\n\nclass Invoice:\n    pass\n"),
        ),
        (
            "plain assignment in the body",
            MODELS.replace("    amount: int\n", "    amount: int\n    scale = 2\n"),
        ),
        (
            "field with options",
            MODELS.replace(
                "field(default_factory=list)",
                "field(default_factory=list, init=False)",
            ),
        ),
        (
            "duplicate field",
            MODELS.replace(
                "    amount: int\n",
                "    amount: int\n    tax_rate: float = 0.3\n",
            ),
        ),
        (
            "required field after a default",
            MODELS
                .replace("    amount: int\n", "    amount: int = 1\n")
                .replace(
                    "    tags: list = field(default_factory=list)\n",
                    "    count: int\n",
                ),
        ),
    ] {
        assert_class_abstains(&source, name)?;
    }
    Ok(())
}

#[test]
fn a_method_outside_the_closed_arithmetic_is_not_credited() -> Check {
    for (name, expression) in [
        (
            "rounding digits",
            "round(self.amount * (1 + self.tax_rate), 2)",
        ),
        ("floor division", "self.amount * (1 + self.tax_rate) // 1"),
        ("power", "self.amount ** self.tax_rate"),
        ("modulo", "self.amount % 7 + self.tax_rate"),
        ("helper call", "self.compute(self.tax_rate)"),
        ("builtin call", "int(self.amount * self.tax_rate)"),
        ("other field", "self.amount + len(self.tags)"),
        ("attribute chain", "self.amount * self.tax_rate.real"),
        ("global name", "self.amount * RATE"),
        ("condition", "self.amount if self.tax_rate else 0"),
        ("string", "'x' + self.tax_rate"),
        ("exponent literal", "self.amount * 1e2"),
    ] {
        let source = MODELS.replace("round(self.amount * (1 + self.tax_rate))", expression);
        assert_class_abstains(&source, name)?;
    }
    Ok(())
}

#[test]
fn a_method_that_may_not_be_the_plain_one_is_not_credited() -> Check {
    for (name, source) in [
        (
            "property",
            MODELS.replace("    def total", "    @property\n    def total"),
        ),
        (
            "cached",
            MODELS.replace("    def total", "    @cache\n    def total"),
        ),
        (
            "two statements",
            MODELS.replace(
                "        return round",
                "        self.amount += 1\n        return round",
            ),
        ),
        (
            "defined twice",
            format!("{MODELS}\n    def total(self):\n        return 0\n"),
        ),
        (
            "shadowed by a field",
            MODELS.replace("    amount: int\n", "    amount: int\n    total: int = 0\n"),
        ),
        (
            "extra parameter",
            MODELS.replace("def total(self):", "def total(self, extra):"),
        ),
        (
            "rebound builtin",
            MODELS.replace("\n\n\n@dataclass", "\nround = floor\n\n\n@dataclass"),
        ),
        (
            "method is a default expression",
            MODELS.replace(
                "    def total(self):\n        return round(self.amount * (1 + self.tax_rate))\n",
                "    total = staticmethod(lambda: 120)\n",
            ),
        ),
    ] {
        assert_class_abstains(&source, name)?;
    }
    Ok(())
}

#[test]
fn an_edit_that_is_not_a_lone_numeric_default_replacement_is_not_credited() -> Check {
    let test_src = test_file("    assert Invoice(100).total() == 120");
    // Other lines changed in the file: the old method body is unknown.
    assert_abstains(
        &classify_edit(MODELS, OLD_LINE, NEW_LINE, &test_src, false)?,
        "not a sole replacement",
    );
    for (name, old, new_source, new) in [
        (
            "annotation changed",
            "    tax_rate: int = 0.25",
            MODELS.to_string(),
            NEW_LINE,
        ),
        (
            "field renamed",
            "    rate: float = 0.2",
            MODELS.to_string(),
            NEW_LINE,
        ),
        (
            "default became a call",
            OLD_LINE,
            MODELS.replace("0.25", "compute()"),
            "    tax_rate: float = compute()",
        ),
        (
            "default became a string",
            OLD_LINE,
            MODELS.replace("0.25", "'0.25'"),
            "    tax_rate: float = '0.25'",
        ),
        (
            "default was removed",
            OLD_LINE,
            MODELS.replace(" = 0.25", ""),
            "    tax_rate: float",
        ),
        (
            "different indentation",
            "tax_rate: float = 0.2",
            MODELS.to_string(),
            NEW_LINE,
        ),
        (
            "exponent literal",
            OLD_LINE,
            MODELS.replace("0.25", "2.5e-1"),
            "    tax_rate: float = 2.5e-1",
        ),
    ] {
        let finding = classify_edit(&new_source, old, new, &test_src, true)?;
        assert_ne!(finding.class, ExposureClass::Exposed, "{name}");
    }
    Ok(())
}

#[test]
fn the_default_of_a_field_the_method_does_not_read_is_not_credited() -> Check {
    // Editing `tags`' sibling `amount`-only method: the changed field is
    // never read by the asserted method.
    let source = MODELS.replace(
        "round(self.amount * (1 + self.tax_rate))",
        "self.amount * 2",
    );
    assert_class_abstains(&source, "method ignores the changed field")
}

#[test]
fn an_earlier_defaulted_field_does_not_hide_the_omission() -> Check {
    // `Invoice(100, 0.2)` fills `tax_rate` positionally, so the omitted-field
    // rule applies to the field position, not to the argument count alone.
    let source = MODELS.replace(
        "    tax_rate: float = 0.25\n",
        "    discount: float = 0.0\n    tax_rate: float = 0.25\n",
    );
    for (body, credited) in [
        // `discount` is supplied, `tax_rate` is omitted.
        ("assert Invoice(100, 0.0).total() == 120", true),
        // Both supplied: the edited default is never read.
        ("assert Invoice(100, 0.0, 0.2).total() == 120", false),
    ] {
        let finding = classify_rate(&source, &test_file(&format!("    {body}")))?;
        if credited {
            assert_credited(&finding);
        } else {
            assert_abstains(&finding, body);
        }
    }
    Ok(())
}

#[test]
fn a_field_marker_that_is_not_a_stored_default_is_not_credited() -> Check {
    for annotation in [
        "ClassVar[float]",
        "InitVar[float]",
        "'float'",
        "float | None",
    ] {
        let new_line = format!("    tax_rate: {annotation} = 0.25");
        let old_line = format!("    tax_rate: {annotation} = 0.2");
        let source = MODELS.replace(NEW_LINE, &new_line);
        let finding = classify_edit(
            &source,
            &old_line,
            &new_line,
            &test_file("    assert Invoice(100).total() == 120"),
            true,
        )?;
        assert_abstains(&finding, annotation);
    }
    Ok(())
}
